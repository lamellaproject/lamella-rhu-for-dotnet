//! QEMU mps2-an505 (Cortex-M33 FPGA image): the ARMv8-M mainline
//! architecture leg. CMSDK APB UART0 console, semihosting exits.

use core::ptr::{read_volatile, write_volatile};

pub const NAME: &str = "QEMU mps2-an505";
pub const CONSOLE: &str = "CMSDK UART0";

/// FPGA system/APB clock (AN505 application note; QEMU models it).
pub const SYSCLK_HZ: u32 = 20_000_000;

/// SysTick rate.
pub const TICK_HZ: u32 = 1_000;

/// Thread stack pool (lives in .kstacks) -- a board-scalable knob;
/// small-RAM boards re-derive it from their own budgets.
pub const STACK_POOL_BYTES: usize = 64 * 1024;

/// The boot kheap carve out of the arena (the GC board table's
/// `kheap_carve` row). Sized conservatively rather than from measured
/// demand.
pub const KHEAP_BYTES: usize = 64 * 1024;

/// Stack for the kmain thread (the boot thread's continuation).
pub const KMAIN_STACK_BYTES: usize = 16 * 1024;

/// External IRQ lines wired by the machine model -- sizes the vector
/// table. 32 SSE-internal lines + 92 expansion lines (EXP_NUMIRQ,
/// probed live against QEMU; the AN505 note's interrupt map agrees).
pub const NIRQ: usize = 124;

/// FPU shape: the AN505's Cortex-M33 carries FPv5 SINGLE precision
/// only -- kernel f64 stays on the soft-float path, byte-for-byte the
/// an500 posture. Consumed by records and gates, never by
/// context-switch code (lazy stacking of S0-S31 is FP-shape-agnostic).
#[allow(dead_code)]
pub const FPU_DP: bool = false;

/// RAM windows a hardware-pushed exception frame may plausibly live in
/// (SSRAM2/3 through the secure alias; each end backed off by one
/// 8-word frame). The funnel refuses to dereference a frame pointer
/// outside them.
pub const FRAME_RANGES: [(usize, usize); 1] = [(0x3800_0000, 0x3840_0000 - 32)];

/// CMSDK APB UART0 through the secure alias (0x40200000 | 0x10000000).
const UART0: usize = 0x5020_0000;
const DATA: *mut u32 = UART0 as *mut u32;
const STATE: *mut u32 = (UART0 + 0x04) as *mut u32;
const CTRL: *mut u32 = (UART0 + 0x08) as *mut u32;
const BAUDDIV: *mut u32 = (UART0 + 0x10) as *mut u32;

const STATE_TXFULL: u32 = 1 << 0;
const STATE_RXFULL: u32 = 1 << 1;
const CTRL_TXEN: u32 = 1 << 0;
const CTRL_RXEN: u32 = 1 << 1;
const BAUD: u32 = 115_200;

/// Clock posture: the FPGA image runs at its fixed 20 MHz; there is
/// nothing to program. Boards with a real clock tree enforce their
/// posture here, before the console exists.
pub fn clock_init() {}

/// Boring posture: no clock line in the banner (the banked transcript
/// surface stands byte-identical).
pub fn clock_readback() -> Option<crate::board::ClockReadback> {
    None
}

/// Program the divider and enable both directions. Idempotent, so the
/// panic/fault paths may call it again defensively.
pub fn console_init() {
    unsafe {
        write_volatile(BAUDDIV, SYSCLK_HZ / BAUD);
        write_volatile(CTRL, CTRL_TXEN | CTRL_RXEN);
    }
}

/// Blocking byte transmit (polled).
pub fn console_putb(b: u8) {
    unsafe {
        while read_volatile(STATE) & STATE_TXFULL != 0 {}
        write_volatile(DATA, b as u32);
    }
}

/// Non-blocking byte receive: one pending byte, or None immediately.
pub fn console_getb() -> Option<u8> {
    unsafe {
        if read_volatile(STATE) & STATE_RXFULL != 0 {
            Some(read_volatile(DATA) as u8)
        } else {
            None
        }
    }
}
