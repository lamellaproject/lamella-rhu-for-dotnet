//! Boot: the vector table and Reset. Hand-rolled -- the kernel owns its
//! exception plumbing outright (the fault funnel today; the scheduler
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
#[unsafe(link_section = ".vector_table.reset")]
#[unsafe(no_mangle)]
static __RESET_VECTOR: unsafe extern "C" fn() = Reset;

/// [2..=15] System exceptions. Faults funnel through the trampoline
/// (the Rust side decodes IPSR); the scheduler owns SVCall, PendSV, and
/// SysTick.
#[unsafe(link_section = ".vector_table.exceptions")]
#[unsafe(no_mangle)]
static __EXCEPTIONS: [Vector; 14] = [
    Vector { handler: ExceptionTrampoline },
    Vector { handler: ExceptionTrampoline },
    Vector { handler: ExceptionTrampoline },
    Vector { handler: ExceptionTrampoline },
    Vector { handler: ExceptionTrampoline },
    Vector { reserved: 0 },
    Vector { reserved: 0 },
    Vector { reserved: 0 },
    Vector { reserved: 0 },
    Vector { handler: SvcHandler },
    Vector { handler: ExceptionTrampoline },
    Vector { reserved: 0 },
    Vector { handler: PendSvHandler },
    Vector { handler: crate::sched::systick_vector },
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
    let mut i = 0;
    while i < crate::board::BOARD_IRQS.len() {
        let (line, handler) = crate::board::BOARD_IRQS[i];
        assert!(line < crate::board::NIRQ, "a board interrupt handler names a line outside the vector table");
        table[line] = Vector { handler };
        i += 1;
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
    let sdata = addr_of_mut!(__sdata) as *mut u8;
    let edata = addr_of!(__edata) as *const u8;
    let sidata = addr_of!(__sidata) as *const u8;
    ptr::copy_nonoverlapping(sidata, sdata, edata as usize - sdata as usize);

    let sbss = addr_of_mut!(__sbss) as *mut u8;
    let ebss = addr_of!(__ebss) as *const u8;
    ptr::write_bytes(sbss, 0, ebss as usize - sbss as usize);

    SCB_VTOR.write_volatile(addr_of!(__vector_table) as usize as u32);

    SCB_SHCSR.write_volatile(SCB_SHCSR.read_volatile() | (0b111 << 16));

    crate::board::clock_init();
    crate::console::init();
    crate::kmain()
}}
