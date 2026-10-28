//! The designed death inside an interrupt handler: the row's spare
//! interrupt line, enabled at the lowest urgency and pended from the thread;
//! its handler executes an undefined instruction. A fault raised inside a
//! handler of the lowest urgency cannot preempt it, so the UsageFault
//! escalates to the HardFault the funnel names from handler mode, and the
//! handler stack's watermark line at the end carries the depth of the
//! handler's frame, the fault's stacking and the funnel's report. A handler
//! that survives prints the UNREACHABLE line and the flavor exits 3.

use core::ptr::{read_volatile, write_volatile};

use crate::{board, semihost};

const NVIC_ISER: usize = 0xE000_E100;
const NVIC_ISPR: usize = 0xE000_E200;
const NVIC_IPR: usize = 0xE000_E400;
/// The lowest urgency a row with three priority bits can hold: every other
/// exception preempts the handler and a fault inside it escalates.
const PRIO: u8 = 0xE0;
const BIT: u32 = 1 << ((board::SPARE_IRQ % 32) as u32);

fn reg(base: usize) -> *mut u32 {
    (base + 4 * (board::SPARE_IRQ / 32)) as *mut u32
}

/// The line armed and pended; the handler runs at once (the thread runs
/// with every line open) and the report ends the run before the spin below
/// completes.
pub fn run() {
    kprintln!("[fault-in-handler] the spare interrupt line {} enabled at the lowest urgency and pended; its handler executes udf #0 -- expect a HardFault dump from handler mode (the UsageFault escalated)", board::SPARE_IRQ);
    unsafe {
        write_volatile((NVIC_IPR + board::SPARE_IRQ) as *mut u8, PRIO);
        let got = read_volatile((NVIC_IPR + board::SPARE_IRQ) as *const u8);
        assert!(got == PRIO, "fault-in-handler: the spare line {} is not implemented on this row (its priority byte does not take a write)", board::SPARE_IRQ);
        write_volatile(reg(NVIC_ISER), BIT);
        write_volatile(reg(NVIC_ISPR), BIT);
        core::arch::asm!("dsb sy", "isb sy", options(nostack, preserves_flags));
    }
    for _ in 0..1_000_000u32 {
        core::hint::spin_loop();
    }
    kprintln!("[fault-in-handler] UNREACHABLE: the handler survived its undefined instruction");
    semihost::exit(3)
}

/// The handler: the undefined instruction, nothing else.
pub unsafe extern "C" fn __rhu_fault_in_handler_irq() {
    unsafe { core::arch::asm!("udf #0") };
}
