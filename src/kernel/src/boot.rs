//! Boot: the vector table and Reset. Hand-rolled -- the kernel owns its
//! exception plumbing outright (the fault funnel; the scheduler
//! and fault-hosting seams are kernel property), so no runtime crate may
//! own the vectors first.

use core::arch::global_asm;
use core::ptr::{self, addr_of, addr_of_mut};

/// One vector-table slot: a handler address or a reserved zero.
#[derive(Clone, Copy)]
#[repr(C)]
pub union Vector {
    handler: unsafe extern "C" fn(),
    reserved: usize,
}

unsafe extern "C" {
    #[cfg_attr(rhu_supply_stub, allow(dead_code))] // the supply entry's assembly names Reset directly
    fn Reset();
    fn ExceptionTrampoline();
    fn SvcHandler();
    fn PendSvHandler();
    static __vector_table: u32;
    static mut __sdata: u32;
    static __edata: u32;
    static __sidata: u32;
    static mut __sbss: u32;
    static __ebss: u32;
}

/// [1] Reset -- hardware loads MSP from slot 0 (linker-provided), then here.
#[cfg(not(rhu_supply_stub))]
#[unsafe(link_section = ".vector_table.reset")]
#[unsafe(no_mangle)]
static __RESET_VECTOR: unsafe extern "C" fn() = Reset;
/// [1] On a row whose chip must have its supply configuration written
/// before any RAM write (the STM32H7 rows), the family's register-only
/// entry, which branches to Reset once the supply is ready.
#[cfg(rhu_supply_stub)]
#[unsafe(link_section = ".vector_table.reset")]
#[unsafe(no_mangle)]
static __RESET_VECTOR: unsafe extern "C" fn() = crate::board::supply_reset;

/// [2..=15] System exceptions. Faults funnel through the trampoline
/// (the Rust side decodes IPSR); the scheduler owns SVCall, PendSV, and
/// SysTick.
#[unsafe(link_section = ".vector_table.exceptions")]
#[unsafe(no_mangle)]
static __EXCEPTIONS: [Vector; 14] = [
    Vector { handler: ExceptionTrampoline }, // 2  NMI
    Vector { handler: ExceptionTrampoline }, // 3  HardFault
    Vector { handler: ExceptionTrampoline }, // 4  MemManage
    Vector { handler: ExceptionTrampoline }, // 5  BusFault
    Vector { handler: ExceptionTrampoline }, // 6  UsageFault
    Vector { reserved: 0 },                  // 7
    Vector { reserved: 0 },                  // 8
    Vector { reserved: 0 },                  // 9
    Vector { reserved: 0 },                  // 10
    Vector { handler: SvcHandler },          // 11 SVCall (first start)
    Vector { handler: ExceptionTrampoline }, // 12 DebugMonitor
    Vector { reserved: 0 },                  // 13
    Vector { handler: PendSvHandler },       // 14 PendSV (context switch)
    Vector { handler: crate::sched::systick_vector }, // 15 SysTick
];

/// [16..) External IRQ lines -- the count is a board fact. Every line
/// funnels: an unexpected interrupt is a loud fault report, not a spin --
/// except the lines a board claims with its own handlers
/// (`board::BOARD_IRQS`; an empty list on boards that claim none), folded
/// into the table here at compile time.
#[unsafe(link_section = ".vector_table.interrupts")]
#[unsafe(no_mangle)]
static __INTERRUPTS: [Vector; crate::board::NIRQ] = interrupts();

/// The external-line table: the trampoline on every line, the board's own
/// handlers on the lines it claims. A claimed line outside the table
/// refuses to compile.
const fn interrupts() -> [Vector; crate::board::NIRQ] {
    let mut table = [Vector { handler: ExceptionTrampoline }; crate::board::NIRQ];
    // The lines claimed so far, as a bitmap: a line claimed twice -- inside
    // either list or across the two -- refuses to compile (function pointers
    // cannot be compared in constant evaluation, so the bitmap is the check).
    let mut claimed = [0u64; (crate::board::NIRQ + 63) / 64];
    let mut i = 0;
    while i < crate::board::BOARD_IRQS.len() {
        let (line, handler) = crate::board::BOARD_IRQS[i];
        assert!(line < crate::board::NIRQ, "a board interrupt handler names a line outside the vector table");
        assert!(claimed[line / 64] & (1u64 << (line % 64)) == 0, "an interrupt line is claimed twice");
        claimed[line / 64] |= 1u64 << (line % 64);
        table[line] = Vector { handler };
        i += 1;
    }
    // The family GPIO driver's handlers (the device seam): the lines it owns,
    // folded the same way through the same bitmap.
    let mut j = 0;
    while j < crate::board::GPIO_IRQS.len() {
        let (line, handler) = crate::board::GPIO_IRQS[j];
        assert!(line < crate::board::NIRQ, "a GPIO handler names a line outside the vector table");
        assert!(claimed[line / 64] & (1u64 << (line % 64)) == 0, "an interrupt line is claimed twice");
        claimed[line / 64] |= 1u64 << (line % 64);
        table[line] = Vector { handler };
        j += 1;
    }
    // The api flavor's band-interrupt control line (a test image, never a
    // shipped one): folded the same way; the list is empty otherwise.
    let mut k = 0;
    while k < crate::TEST_IRQS.len() {
        let (line, handler) = crate::TEST_IRQS[k];
        assert!(line < crate::board::NIRQ, "a test handler names a line outside the vector table");
        assert!(claimed[line / 64] & (1u64 << (line % 64)) == 0, "an interrupt line is claimed twice");
        claimed[line / 64] |= 1u64 << (line % 64);
        table[line] = Vector { handler };
        k += 1;
    }
    table
}

global_asm!(
    r#"
    .section .text.Reset, "ax", %progbits
    .global Reset
    .type Reset, %function
    .thumb_func
Reset:
    /* FPU access on before ANY Rust runs: under the hard-float ABI the
       compiler may use FP registers in arbitrary functions, and CPACR
       resets to access-denied -- the failure would be a NOCP UsageFault
       before the console exists. CP10/CP11 = full access. */
    movw r0, #0xED88            /* CPACR = 0xE000ED88 */
    movt r0, #0xE000
    ldr  r1, [r0]
    orr  r1, r1, #0x00F00000
    str  r1, [r0]
    dsb
    isb
    bl   __rhu_reset
1:  b    1b
    .size Reset, . - Reset
"#
);

const SCB_VTOR: *mut u32 = 0xE000_ED08 as *mut u32;
const SCB_SHCSR: *mut u32 = 0xE000_ED24 as *mut u32;

#[unsafe(no_mangle)]
unsafe extern "C" fn __rhu_reset() -> ! { unsafe {
    // The handler stack's watermark: the reserve filled with its pattern
    // before anything else runs (a no-op unless the image measures).
    crate::stack_watermark::fill();

    // .data: copy initializers from their load address inside CODE.
    let sdata = addr_of_mut!(__sdata) as *mut u8;
    let edata = addr_of!(__edata) as *const u8;
    let sidata = addr_of!(__sidata) as *const u8;
    ptr::copy_nonoverlapping(sidata, sdata, edata as usize - sdata as usize);

    // .bss: zero-fill.
    let sbss = addr_of_mut!(__sbss) as *mut u8;
    let ebss = addr_of!(__ebss) as *const u8;
    ptr::write_bytes(sbss, 0, ebss as usize - sbss as usize);

    // The payload's GC arena and its start bits: their own NOLOAD section
    // at the head of DATA, outside [__sbss, __ebss) (the payload link
    // scripts place it), zeroed here under the payload
    // feature alone. An honest image has neither the section nor the
    // symbols.
    #[cfg(feature = "payload")]
    {
        unsafe extern "C" {
            static mut __rhu_payload_arena_lo: u32;
            static __rhu_payload_arena_hi: u32;
        }
        let lo = addr_of_mut!(__rhu_payload_arena_lo) as *mut u8;
        let hi = addr_of!(__rhu_payload_arena_hi) as *const u8;
        ptr::write_bytes(lo, 0, hi as usize - lo as usize);
    }

    // Point VTOR at our table explicitly. It is already 0 out of reset
    // here; stating it keeps the boot path identical on images whose
    // table is not at the reset default.
    SCB_VTOR.write_volatile(addr_of!(__vector_table) as usize as u32);
    // The write completes and the pipeline drains before an exception
    // can be taken through the table: the barrier pair, the reset stub's
    // idiom.
    core::arch::asm!("dsb sy", "isb sy", options(nostack, preserves_flags));

    // Enable MemManage/BusFault/UsageFault as their own exceptions, so
    // IPSR names the real fault instead of escalating to HardFault.
    SCB_SHCSR.write_volatile(SCB_SHCSR.read_volatile() | (0b111 << 16));
    core::arch::asm!("dsb sy", "isb sy", options(nostack, preserves_flags));

    // Clocks before console: the console divisor derives from the
    // board's clock posture. QEMU boards' clock_init is a no-op.
    crate::board::clock_init();
    crate::console::init();
    crate::kmain()
}}

/// The hosted runtime's rehydrated-data region on a row with an external
/// RAM range: the map's two symbols around the section at the head of the
/// external region (empty in a kernel image but for the api flavor's test
/// static).
#[cfg(rhu_extram_window)]
pub fn hydrated_bracket() -> (usize, usize) {
    unsafe extern "C" {
        static __rhu_hydrated_lo: u32;
        static __rhu_hydrated_hi: u32;
    }
    (addr_of!(__rhu_hydrated_lo) as usize, addr_of!(__rhu_hydrated_hi) as usize)
}

/// Zero the rehydrated-data region. The section lies outside the zero-fill
/// window [__sbss, __ebss) and outside the arena's windows, so neither the
/// reset's .bss fill nor the arena's memset covers it: this memset does, in
/// every init mode, called from kmain once the external RAM is brought up
/// and proved and before arena::init. The hosted runtime's rehydration
/// writes only the bytes its compact form names and relies on the zero
/// fill for the rest. A bracket inside the zero-fill window (a map that
/// keeps the region in .bss) is the reset's own and is left alone. Under
/// the api flavor the bracket is poisoned first, so the flavor's zero
/// check proves this memset and not the memory's reset state.
#[cfg(rhu_extram_window)]
pub fn zero_hydrated() {
    let (lo, hi) = hydrated_bracket();
    let (sbss, ebss) = (addr_of_mut!(__sbss) as usize, addr_of!(__ebss) as usize);
    if lo >= sbss && hi <= ebss {
        return;
    }
    unsafe {
        #[cfg(feature = "api-smoke")]
        ptr::write_bytes(lo as *mut u8, 0xA5, hi - lo);
        ptr::write_bytes(lo as *mut u8, 0, hi - lo);
    }
}
