//! Feature `payload`: host a minirt no-runtime payload (a frozen ILC
//! object linked in by the external payload link, `payload-<board>.x`).

use core::ptr::{addr_of, addr_of_mut};
use core::sync::atomic::{AtomicU32, Ordering};

use crate::{console, sched, semihost};

/// GC arena, 96 KB. The hosted GC treats [arena_base, arena_base +
/// arena_bytes) as its whole heap; the golden transcripts' heap-size
/// and free-byte lines pin this value, so it is part of the golden
/// contract, not a tuning knob.
const ARENA_BYTES: usize = 96 * 1024;

#[repr(align(8))]
struct Arena([u8; ARENA_BYTES]);
/// .bss (zero at boot); 8-aligned because the payload allocates in
/// 8-byte granules.
static mut ARENA: Arena = Arena([0; ARENA_BYTES]);

/// Object-start bitmap for the conservative GC: one mark bit per
/// 8-byte arena granule, so bytes = ARENA_BYTES / 8 / 8.
static mut STARTBITS: [u8; ARENA_BYTES / 64] = [0; ARENA_BYTES / 64];

/// Stack top of the thread hosting the payload, published by `run()`
/// before the managed entry: the GC scans [current SP, stack_top) of
/// the thread it runs on, and under a scheduler that is a pool-carved
/// thread stack, never the boot stack.
static PAYLOAD_STACK_TOP: AtomicU32 = AtomicU32::new(0);


/// Write a NUL-terminated byte string to the console, unprefixed,
/// under the console lock (payload lines must not tear against kernel
/// lines from other threads).
#[no_mangle]
extern "C" fn sh_write0(p: *const u8) {
    unsafe {
        let mut n = 0usize;
        while *p.add(n) != 0 {
            n += 1;
        }
        let bytes = core::slice::from_raw_parts(p, n);
        console::locked(|_| console::write_bytes(bytes));
    }
}


#[no_mangle]
extern "C" fn arena_base() -> *mut u8 {
    unsafe { addr_of_mut!(ARENA.0) as *mut u8 }
}

#[no_mangle]
extern "C" fn arena_bytes() -> u32 {
    ARENA_BYTES as u32
}

#[no_mangle]
extern "C" fn startbits_base() -> *mut u8 {
    addr_of_mut!(STARTBITS) as *mut u8
}

/// Static-roots window start: the image's whole .data..bss range. The
/// board scripts keep .kstacks OUTSIDE this window precisely so a
/// conservative statics scan never walks stale thread-stack contents.
#[no_mangle]
extern "C" fn statics_start() -> *mut u8 {
    extern "C" {
        static mut __sdata: u32;
    }
    addr_of_mut!(__sdata) as *mut u8
}

/// Static-roots window end (see `statics_start`).
#[no_mangle]
extern "C" fn statics_end() -> *mut u8 {
    extern "C" {
        static __ebss: u32;
    }
    addr_of!(__ebss) as *mut u8
}

#[no_mangle]
extern "C" fn stack_top() -> *mut u8 {
    PAYLOAD_STACK_TOP.load(Ordering::Relaxed) as *mut u8
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


/// Stack-guard cookie the payload's /GS-style epilogue checks compare
/// against. Entry and exit read the same static, so any consistent
/// constant satisfies the check; this profile has no runtime
/// randomization source at boot.
#[no_mangle]
pub static __security_cookie: usize = 0x5248_5521;

/// EHABI personality routine named by the payload's unwind tables. The
/// hosted profile fail-fasts instead of unwinding, so this is
/// unreachable by design; park loudly-adjacent (wfi forever) if a
/// future payload ever routes here.
#[no_mangle]
extern "C" fn __aeabi_unwind_cpp_pr0() -> ! {
    semihost::park()
}


#[no_mangle]
extern "C" fn RhpLLsh(value: u64, count: i32) -> u64 {
    value << (count & 63)
}

#[no_mangle]
extern "C" fn RhpLRsz(value: u64, count: i32) -> u64 {
    value >> (count & 63)
}

#[no_mangle]
extern "C" fn RhpLRsh(value: i64, count: i32) -> i64 {
    value >> (count & 63)
}


/// Publish this thread's stack top, run the payload's module
/// initializers, enter managed Main, and exit the machine with its
/// return code. Runs on the kmain thread; never returns.
pub fn run() -> ! {
    PAYLOAD_STACK_TOP.store(sched::current_stack_top(), Ordering::Relaxed);

    extern "C" {
        fn InitializeModules(
            os_module: usize,
            headers: *const usize,
            count: i32,
            classlib: *const usize,
            n_classlib: i32,
        );
        fn __managed__Main(argc: i32, argv: *const *const u8) -> i32;
        static __start___modules: usize;
        static __stop___modules: usize;
    }

    let rc = unsafe {
        let start = addr_of!(__start___modules) as usize;
        let stop = addr_of!(__stop___modules) as usize;
        InitializeModules(
            0,
            start as *const usize,
            ((stop - start) / core::mem::size_of::<usize>()) as i32,
            core::ptr::null(),
            0,
        );
        __managed__Main(0, core::ptr::null())
    };

    kprintln!("[payload] managed Main returned {}", rc);
    semihost::exit(rc as u32)
}
