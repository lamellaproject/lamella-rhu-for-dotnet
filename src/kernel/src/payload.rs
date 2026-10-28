//! Feature `payload`: host a minirt no-runtime payload (a frozen ILC
//! object linked in by the external payload link, `payload-<board>.x`).
//!
//! The import surface below is exactly the payload objects' undefined
//! symbol table: console write, the conservative GC's five memory
//! windows, two register-level GC helpers, the stack-guard cookie, an
//! unwind personality stub, and the three 64-bit shift helpers (the
//! cookie and the shift helpers are weak: a hosted runtime that defines
//! them displaces the kernel's).
//! memset/memcpy/memmove resolve from the staticlib's bundled
//! compiler-builtins. Payload bytes go through the kernel console's
//! `\n` -> `\r\n` wire path unprefixed, so the payload stream filters
//! and byte-diffs mechanically (console grammar).
//!
//! The entry seam (`rhu_payload_entry`, the one hosting-API member the
//! kernel CALLS rather than serves): `run` brings the platform up,
//! settles the contract-generation handshake, and hands the kmain
//! thread to exactly one function that returns the program's exit
//! code. The kernel provides that function as a WEAK default -- the
//! constructor-array placement check and walks, the module list,
//! managed Main. A payload that links a strong `rhu_payload_entry`
//! owns the whole order instead, and the default never runs. The thread
//! runs the entry at the hosted flat priority -- inside the band every
//! thread the payload starts is clamped into, level with them, so a
//! start waited for by yield alone completes -- and takes the kernel's
//! own urgency back when the entry returns.

use core::hint::black_box;
use core::ptr::{addr_of, addr_of_mut, read_volatile};

use crate::abi_window::{self, Verdict};
use crate::{api, console, sched, semihost};

#[cfg(all(feature = "test-payload-cycles", not(rhu_qspi_window)))]
compile_error!("test-payload-cycles is a measurement posture of board-st-stm32f769i-disco -- select that board");

/// GC arena, 96 KB. The hosted GC treats [arena_base, arena_base +
/// arena_bytes) as its whole heap; the golden transcripts' heap-size
/// and free-byte lines pin this value, so it is part of the golden
/// contract, not a tuning knob.
const ARENA_BYTES: usize = 96 * 1024;

#[repr(align(8))]
struct Arena([u8; ARENA_BYTES]);
/// Its own NOLOAD section at the head of DATA (the payload link scripts'
/// `.rhu_payload_arena`, zeroed by boot.rs under this feature): no static
/// added before it moves the arena, so the collector's count is a layout
/// figure of an address fixed at ORIGIN(DATA). 8-aligned because
/// the payload allocates in 8-byte granules.
#[unsafe(link_section = ".bss.rhu_payload_arena")]
static mut ARENA: Arena = Arena([0; ARENA_BYTES]);

/// Object-start bitmap for the conservative GC: one mark bit per
/// 8-byte arena granule, so bytes = ARENA_BYTES / 8 / 8.
#[unsafe(link_section = ".bss.rhu_payload_startbits")]
static mut STARTBITS: [u8; ARENA_BYTES / 64] = [0; ARENA_BYTES / 64];

// ---- console ----

/// Write a NUL-terminated byte string to the console, unprefixed,
/// under the console lock (payload lines must not tear against kernel
/// lines from other threads).
#[unsafe(no_mangle)]
extern "C" fn sh_write0(p: *const u8) {
    unsafe {
        let mut n = 0usize;
        while *p.add(n) != 0 {
            n += 1;
        }
        let bytes = core::slice::from_raw_parts(p, n);
        #[cfg(all(feature = "test-payload-cycles", rhu_qspi_window))]
        let t0 = console::door_cycles_now();
        console::locked(console::Stream::Console, |_| console::write_bytes(console::Stream::Console, bytes));
        #[cfg(all(feature = "test-payload-cycles", rhu_qspi_window))]
        console::door_cycles_add(t0);
    }
}

// ---- conservative-GC memory windows ----

#[unsafe(no_mangle)]
extern "C" fn arena_base() -> *mut u8 {
    unsafe { addr_of_mut!(ARENA.0) as *mut u8 }
}

#[unsafe(no_mangle)]
extern "C" fn arena_bytes() -> u32 {
    ARENA_BYTES as u32
}

#[unsafe(no_mangle)]
extern "C" fn startbits_base() -> *mut u8 {
    addr_of_mut!(STARTBITS) as *mut u8
}

/// Static-roots window start: the image's whole .data..bss range. The
/// board scripts keep .kstacks OUTSIDE this window precisely so a
/// conservative statics scan never walks stale thread-stack contents.
#[unsafe(no_mangle)]
extern "C" fn statics_start() -> *mut u8 {
    unsafe extern "C" {
        static mut __sdata: u32;
    }
    addr_of_mut!(__sdata) as *mut u8
}

/// Static-roots window end (see `statics_start`).
#[unsafe(no_mangle)]
extern "C" fn statics_end() -> *mut u8 {
    unsafe extern "C" {
        static __ebss: u32;
    }
    addr_of!(__ebss) as *mut u8
}

/// The CURRENT thread's stack top, read live: the collector scans
/// [current SP, stack top) of the thread it runs on, and under a
/// scheduler that is that thread's own pool-carved stack -- never the
/// boot stack, never another thread's top.
#[unsafe(no_mangle)]
extern "C" fn stack_top() -> *mut u8 {
    sched::current_stack_top() as *mut u8
}

core::arch::global_asm!(
    r#"
    .section .text.gc_flush_regs, "ax", %progbits
    .global gc_flush_regs
    .type gc_flush_regs, %function
    .thumb_func
gc_flush_regs:
    stmia r0!, {{r4-r11}}
    bx    lr
    .size gc_flush_regs, . - gc_flush_regs

    .section .text.gc_current_sp, "ax", %progbits
    .global gc_current_sp
    .type gc_current_sp, %function
    .thumb_func
gc_current_sp:
    mov   r0, sp
    bx    lr
    .size gc_current_sp, . - gc_current_sp
"#
);

// ---- link-closure statics and stubs ----

// The stack-guard cookie and the three 64-bit shift helpers are minirt
// imports that a full runtime defines for itself, so the kernel's
// definitions are WEAK: an object on the link line that carries its own
// displaces them, and the minirt payloads keep the kernel's. The cookie
// is the word the payload's /GS-style epilogue checks compare against
// (entry and exit read the same word, so any consistent constant
// satisfies the check; this profile has no runtime randomization source
// at boot). Each shift helper is a weak trampoline into its Rust body
// below -- the entry seam's idiom.
core::arch::global_asm!(
    r#"
    .section .rodata.__security_cookie, "a", %progbits
    .weak __security_cookie
    .type __security_cookie, %object
    .p2align 2
__security_cookie:
    .word 0x52485521
    .size __security_cookie, 4

    .section .text.RhpLLsh, "ax", %progbits
    .weak RhpLLsh
    .type RhpLLsh, %function
    .thumb_func
RhpLLsh:
    b.w   __rhu_minirt_llsh
    .size RhpLLsh, . - RhpLLsh

    .section .text.RhpLRsz, "ax", %progbits
    .weak RhpLRsz
    .type RhpLRsz, %function
    .thumb_func
RhpLRsz:
    b.w   __rhu_minirt_lrsz
    .size RhpLRsz, . - RhpLRsz

    .section .text.RhpLRsh, "ax", %progbits
    .weak RhpLRsh
    .type RhpLRsh, %function
    .thumb_func
RhpLRsh:
    b.w   __rhu_minirt_lrsh
    .size RhpLRsh, . - RhpLRsh
"#
);

/// EHABI personality routine named by the payload's unwind tables. The
/// hosted profile fail-fasts instead of unwinding, so this is
/// unreachable by design; park loudly-adjacent (wfi forever) if a
/// payload ever routes here.
#[unsafe(no_mangle)]
extern "C" fn __aeabi_unwind_cpp_pr0() -> ! {
    semihost::park()
}

// ---- 64-bit variable-shift helpers (the weak names above branch here) ----

#[unsafe(no_mangle)]
extern "C" fn __rhu_minirt_llsh(value: u64, count: i32) -> u64 {
    value << (count & 63)
}

#[unsafe(no_mangle)]
extern "C" fn __rhu_minirt_lrsz(value: u64, count: i32) -> u64 {
    value >> (count & 63)
}

#[unsafe(no_mangle)]
extern "C" fn __rhu_minirt_lrsh(value: i64, count: i32) -> i64 {
    value >> (count & 63)
}

// ---- the entry seam ----

// The weak default entry and the contract-generation sentinel. The
// trampoline `rhu_payload_entry` is WEAK: an object on the link line
// that defines the name strongly displaces it, and `run` tells the two
// apart by comparing the entry's address with the non-weak alias
// `__rhu_payload_entry_weak` at the trampoline. `rhu_payload_abi` is
// the weak sentinel 0 ("no generation stated"): a payload that provides
// its own entry defines it strongly as the RHU_ABI it was built
// against. Both live in sections of their own, so a displaced default
// costs the image nothing but the trampoline the alias keeps.
core::arch::global_asm!(
    r#"
    .section .text.rhu_payload_entry, "ax", %progbits
    .weak rhu_payload_entry
    .type rhu_payload_entry, %function
    .global __rhu_payload_entry_weak
    .type __rhu_payload_entry_weak, %function
    .thumb_func
rhu_payload_entry:
    .thumb_func
__rhu_payload_entry_weak:
    b.w   rhu_payload_entry_default
    .size rhu_payload_entry, . - rhu_payload_entry
    .size __rhu_payload_entry_weak, . - __rhu_payload_entry_weak

    .section .rodata.rhu_payload_abi, "a", %progbits
    .weak rhu_payload_abi
    .type rhu_payload_abi, %object
    .p2align 2
rhu_payload_abi:
    .word 0
    .size rhu_payload_abi, 4
"#
);

unsafe extern "C" {
    fn rhu_payload_entry() -> i32;
    fn __rhu_payload_entry_weak() -> i32;
    static rhu_payload_abi: u32;
}

/// The exit code of a refused entry: the payload states a contract
/// generation that is not this kernel's, or provides an entry without
/// stating one. The sentence prints first, so a mismatched peer reports
/// instead of hanging (the exit is a park on silicon).
const EXIT_ENTRY_REFUSED: u32 = 4;

/// One constructor array as the map brackets it: the walk's input.
struct CtorArray {
    lo: usize,
    hi: usize,
}

impl CtorArray {
    /// The bracket pair, each address through `black_box` (sized extern
    /// statics that may share an address never sit un-opaque on both
    /// sides of the arithmetic below), checked against the boot copy
    /// window: an array outside it holds whatever the RAM held at
    /// reset, so the refusal is loud and comes before any entry is read.
    fn checked(name: &str, lo: *const usize, hi: *const usize, window: (usize, usize)) -> CtorArray {
        let lo = black_box(lo as usize);
        let hi = black_box(hi as usize);
        assert!(lo <= hi, "{} brackets inverted", name);
        assert!(lo >= window.0 && hi <= window.1, "{} outside the boot copy window", name);
        CtorArray { lo, hi }
    }

    fn len(&self) -> usize {
        (self.hi - self.lo) / core::mem::size_of::<usize>()
    }

    /// Call every entry in order. Each word is a Thumb function pointer
    /// the linker resolved with its low bit set.
    fn walk(&self) {
        for i in 0..self.len() {
            let f: extern "C" fn() = unsafe {
                let word = read_volatile((self.lo as *const usize).add(i));
                core::mem::transmute::<usize, extern "C" fn()>(word)
            };
            f();
        }
    }
}

/// The kernel's default entry, reached through the weak trampoline when
/// no payload provides its own: the placement check on the three
/// arrays, the pre-init then init constructor walks, one line stating
/// what ran and what follows, the module list, managed Main -- whose
/// code it returns.
#[unsafe(no_mangle)]
extern "C" fn rhu_payload_entry_default() -> i32 {
    unsafe extern "C" {
        static __sdata: u32;
        static __edata: u32;
        static __preinit_array_start: usize;
        static __preinit_array_end: usize;
        static __init_array_start: usize;
        static __init_array_end: usize;
        static __fini_array_start: usize;
        static __fini_array_end: usize;
        static __start___modules: usize;
        static __stop___modules: usize;
        fn InitializeModules(
            os_module: usize,
            headers: *const usize,
            count: i32,
            classlib: *const usize,
            n_classlib: i32,
        );
        fn __managed__Main(argc: i32, argv: *const *const u8) -> i32;
    }
    let window = (
        black_box(addr_of!(__sdata) as usize),
        black_box(addr_of!(__edata) as usize),
    );
    let pre = CtorArray::checked(
        ".preinit_array",
        addr_of!(__preinit_array_start),
        addr_of!(__preinit_array_end),
        window,
    );
    let init = CtorArray::checked(
        ".init_array",
        addr_of!(__init_array_start),
        addr_of!(__init_array_end),
        window,
    );
    // The destructor array is placed and checked like the other two and
    // never walked here: the machine exit runs no destructors.
    let _fini = CtorArray::checked(
        ".fini_array",
        addr_of!(__fini_array_start),
        addr_of!(__fini_array_end),
        window,
    );
    pre.walk();
    init.walk();

    let start = black_box(addr_of!(__start___modules) as usize);
    let stop = black_box(addr_of!(__stop___modules) as usize);
    let modules = (stop - start) / core::mem::size_of::<usize>();
    kprintln!(
        "[payload] constructors ran: {} pre-init, {} init; modules to initialize: {}",
        pre.len(),
        init.len(),
        modules
    );
    unsafe {
        InitializeModules(0, start as *const usize, modules as i32, core::ptr::null(), 0);
        __managed__Main(0, core::ptr::null())
    }
}

/// Refuse the entry: one sentence on the console, then the exit -- a
/// mismatched peer reports, never hangs.
fn refuse_entry(reason: core::fmt::Arguments) -> ! {
    kprintln!("[payload] entry REFUSED: {}", reason);
    semihost::end(crate::exit_word::STATE_ABORTED, EXIT_ENTRY_REFUSED as i32)
}

// ---- managed entry sequence ----

/// Settle the contract handshake and hand the thread to the payload
/// entry; exit the machine with the code it returns. Runs on the kmain
/// thread; never returns.
pub fn run() -> ! {
    // The handshake: whose entry, and which contract generation. The
    // entry's address against the trampoline's alias says whether a
    // payload displaced the default; a stated generation must be inside
    // this kernel's window (the floor through its own), and a payload
    // that provides an entry must state one. The judge is pure and the
    // host tier tests it (abi_window).
    let provided = black_box(rhu_payload_entry as *const () as usize)
        != black_box(__rhu_payload_entry_weak as *const () as usize);
    let stated = unsafe { read_volatile(addr_of!(rhu_payload_abi)) };
    match abi_window::judge(provided, stated, api::ABI, api::ABI_COMPAT) {
        Verdict::DefaultUnstated => kprintln!(
            "[payload] entry: kernel default (hosting-API contract generation {})",
            api::ABI
        ),
        Verdict::DefaultStated(n) if n == api::ABI => kprintln!(
            "[payload] entry: kernel default; the payload states hosting-API contract generation {} (this kernel: {})",
            n,
            api::ABI
        ),
        Verdict::DefaultStated(n) => kprintln!(
            "[payload] entry: kernel default; the payload states hosting-API contract generation {} (this kernel serves {}; the window reaches back to {})",
            n,
            api::ABI,
            api::ABI_COMPAT
        ),
        Verdict::ProvidedInside(n) if n == api::ABI => kprintln!(
            "[payload] entry: payload-provided, built for hosting-API contract generation {} (this kernel: {})",
            n,
            api::ABI
        ),
        Verdict::ProvidedInside(n) => kprintln!(
            "[payload] entry: payload-provided, built for hosting-API contract generation {} (this kernel serves {}; the window reaches back to {})",
            n,
            api::ABI,
            api::ABI_COMPAT
        ),
        Verdict::RefusedUnstated => refuse_entry(format_args!(
            "the payload provides rhu_payload_entry but states no hosting-API contract generation (rhu_payload_abi)"
        )),
        Verdict::RefusedOutside(n) => refuse_entry(format_args!(
            "the payload was built for hosting-API contract generation {}, outside this kernel's window {}..{}",
            n,
            api::ABI_COMPAT,
            api::ABI
        )),
    }

    // The host thread joins the hosted band for the entry's life: the
    // one flat priority the door hands every hosted thread, so a thread
    // the payload starts and waits for by yield alone can run (a yield
    // rotates equal-priority peers only; at kernel urgency the host
    // could never release it). Kernel urgency returns with the entry,
    // so the exit path keeps it.
    let kernel_prio = sched::set_current_prio(api::HOSTED_PRIO_FLAT as u8);
    kprintln!(
        "[payload] host thread: hosted priority {:#04x} for the entry, kernel urgency {} restored at return",
        api::HOSTED_PRIO_FLAT,
        kernel_prio
    );
    #[cfg(all(feature = "test-payload-cycles", rhu_qspi_window))]
    let t0 = crate::board::stm32f7_qspi::cycles();
    let rc = unsafe { rhu_payload_entry() };
    #[cfg(all(feature = "test-payload-cycles", rhu_qspi_window))]
    let ran = crate::board::stm32f7_qspi::cycles().wrapping_sub(t0);
    sched::set_current_prio(kernel_prio);
    kprintln!("[payload] managed Main returned {}", rc);
    #[cfg(all(feature = "test-payload-cycles", rhu_qspi_window))]
    {
        let (doors, writes) = console::door_cycles();
        kprintln!(
            "[payload] cycles: managed Main ran {} cycles from the entry to the return, {} of them in the console's write doors over {} writes (the core's cycle counter at the core clock; the measurement posture test-payload-cycles)",
            ran,
            doors,
            writes
        );
    }
    semihost::end(crate::exit_word::STATE_RETURNED, rc)
}
