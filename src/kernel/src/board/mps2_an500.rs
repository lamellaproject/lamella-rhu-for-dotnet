//! QEMU mps2-an500 (Cortex-M7 FPGA image): the QEMU-first development
//! board. CMSDK APB UART0 console, semihosting exits.

use core::ptr::{read_volatile, write_volatile};

pub const NAME: &str = "QEMU mps2-an500";
pub const CONSOLE: &str = "CMSDK UART0";

/// FPGA system/APB clock (AN500 application note; QEMU models it).
pub const SYSCLK_HZ: u32 = 25_000_000;

/// SysTick rate.
pub const TICK_HZ: u32 = 1_000;

/// Thread stack pool (lives in .kstacks) -- a board-scalable knob;
/// small-RAM boards re-derive it from their own budgets.
pub const STACK_POOL_BYTES: usize = 64 * 1024;

/// Stack for the kmain thread (the boot thread's continuation).
pub const KMAIN_STACK_BYTES: usize = 16 * 1024;

/// External IRQ lines wired by the machine model -- sizes the vector
/// table.
pub const NIRQ: usize = 32;

/// FPU shape: the AN500's Cortex-M7 carries FPv5 double precision.
/// Consumed by records and gates, never by context-switch code (lazy
/// stacking of S0-S31 is FP-shape-agnostic).
#[allow(dead_code)]
pub const FPU_DP: bool = true;

/// RAM windows a hardware-pushed exception frame may plausibly live in
/// (SSRAM2/3 and PSRAM; each end backed off by one 8-word frame). The
/// funnel refuses to dereference a frame pointer outside them.
pub const FRAME_RANGES: [(usize, usize); 2] = [
    (0x2000_0000, 0x2040_0000 - 32),
    (0x6000_0000, 0x6100_0000 - 32),
];

const UART0: usize = 0x4000_4000;
const DATA: *mut u32 = UART0 as *mut u32;
const STATE: *mut u32 = (UART0 + 0x04) as *mut u32;
const CTRL: *mut u32 = (UART0 + 0x08) as *mut u32;
const BAUDDIV: *mut u32 = (UART0 + 0x10) as *mut u32;

const STATE_TXFULL: u32 = 1 << 0;
const CTRL_TXEN: u32 = 1 << 0;
const BAUD: u32 = 115_200;

/// Clock posture: the FPGA image runs at its fixed 25 MHz; there is
/// nothing to program. Boards with a real clock tree enforce their
/// posture here, before the console exists.
pub fn clock_init() {}

/// Boring posture: no clock line in the banner (the banked transcript
/// surface stands byte-identical).
pub fn clock_readback() -> Option<crate::board::ClockReadback> {
    None
}

/// Program the divider and enable the transmitter. Idempotent, so the
/// panic/fault paths may call it again defensively.
pub fn console_init() {
    unsafe {
        write_volatile(BAUDDIV, SYSCLK_HZ / BAUD);
        write_volatile(CTRL, CTRL_TXEN);
    }
}

/// Blocking byte transmit (polled).
pub fn console_putb(b: u8) {
    unsafe {
        while read_volatile(STATE) & STATE_TXFULL != 0 {}
        write_volatile(DATA, b as u32);
    }
}
