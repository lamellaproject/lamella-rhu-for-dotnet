//! QEMU mps2-an500 (Cortex-M7 FPGA image): the QEMU-first development
//! board. CMSDK APB UART0 console, semihosting exits.

use core::ptr::{read_volatile, write_volatile};

pub const NAME: &str = "QEMU mps2-an500";
/// The chip, as the SDK's facts file names it (the quiet line of the log doors).
pub const CHIP: &str = "Cortex-M7";
pub const CONSOLE: &str = "CMSDK UART0";

/// FPGA system/APB clock (AN500 application note; QEMU models it).
pub const SYSCLK_HZ: u32 = 25_000_000;

/// SysTick rate.
pub const TICK_HZ: u32 = 1_000;

/// Thread stack pool (lives in .kstacks): 128 KiB on this row's 4 MB data
/// window -- the idle thread's 1 KiB, the hosting thread's and the
/// finalizer's 16 KiB each, and five default 16 KiB workers (the runtime's
/// thread pool takes two of them); the link asserts the arena window that
/// remains above the row's floor.
pub const STACK_POOL_BYTES: usize = 128 * 1024;

/// The boot kheap carve out of the arena (the GC board table's
/// `kheap_carve` row). Sized conservatively rather than from measured
/// demand.
pub const KHEAP_BYTES: usize = 64 * 1024;

/// Stack for the kmain thread (the boot thread's continuation).
pub const KMAIN_STACK_BYTES: usize = 16 * 1024;

/// External IRQ lines wired by the machine model -- sizes the vector
/// table.
pub const NIRQ: usize = 32;

/// The vector line a test image may claim: the table's last, implemented
/// by the machine model and claimed by no kernel handler.
#[allow(dead_code)] // board-facts surface; the api flavor's band-interrupt control is its consumer
pub const SPARE_IRQ: usize = NIRQ - 1;

/// FPU shape: the AN500's Cortex-M7 carries FPv5 double precision.
/// Consumed by records and gates, never by context-switch code (lazy
/// stacking of S0-S31 is FP-shape-agnostic).
/// The part: QEMU's Cortex-M7 model with the double-precision FPU (the
/// reference row's hardware).
#[allow(dead_code)] // board-facts surface; the board facade's assert reads it
pub const FPU_DP: bool = true;
/// The cycle counter the lost-tick witness reads: this emulator row
/// models no data watchpoint and trace unit (its registers read as
/// zero and ignore writes), so the witness says so on its
/// census line instead of counting.
pub const CYCLE_COUNTER: bool = false;

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

/// The row's two CMSDK UART blocks for the UART doors' backend:
/// UART0 (the consoles' polled default wire above) and UART1 at
/// 0x4000_5000, each with its receive and transmit interrupt lines (QEMU's
/// mps2.c, the AN385/AN386/AN500 block: the receive line, the transmit line
/// one greater -- uart0 {0, 1}, uart1 {2, 3}; the copies of record).
pub const UART_BASES: [usize; 2] = [UART0, 0x4000_5000];
pub const UART_IRQS: [(usize, usize); 2] = [(0, 1), (2, 3)];

const STATE_TXFULL: u32 = 1 << 0;
const STATE_RXFULL: u32 = 1 << 1;
const CTRL_TXEN: u32 = 1 << 0;
const CTRL_RXEN: u32 = 1 << 1;
const BAUD: u32 = 115_200;

/// Clock posture: the FPGA image runs at its fixed 25 MHz; there is
/// nothing to program. Boards with a real clock tree enforce their
/// posture here, before the console exists.
pub fn clock_init() {}

/// Boring posture: no clock line in the banner (the recorded boot transcript
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

/// The room the console has for a writer: a polled transmitter takes
/// every byte at the wire's pace inside the critical section, so the
/// door's back-pressure never waits here.
pub fn console_room() -> usize {
    usize::MAX
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

/// Whether address 0 is readable memory on this row (a read that returns
/// a word, or a bus fault): the api flavor's observation reads it before
/// it touches the address. With the null-page guard the protection unit denies the first
/// page on every row, so a guarded image never attempts the read; the
/// `null-guard-off` control image alone does.
/// This row: the image's own vector table lives at 0.
#[allow(dead_code)] // read by the api flavor's observation only
pub const ADDRESS_ZERO_READABLE: bool = true;

/// The entropy posture of this row: no hardware noise source on the emulator rows.
pub const ENTROPY: super::EntropyPosture = super::EntropyPosture::None;
/// Whether this row carries a real-time clock the kernel reads at boot and
/// writes at an explicit set.
#[allow(dead_code)] // read by the api flavor's calendar check
pub const RTC_PRESENT: bool = false;
/// The calendar banner's text for this row, constant per image.
pub const RTC_LINE: &str = "no calendar source on this row; the base is the epoch until set";
pub use super::absent::{entropy_draw, entropy_init, rtc_read_at_boot, rtc_write};
