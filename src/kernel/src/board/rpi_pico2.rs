//! Raspberry Pi Pico 2 (RP2350A, the 60-pin package; 4 MB of QSPI flash;
//! the 12 MHz crystal): the chip's reference board. The Pico 2 W is the
//! same board with a wireless module fitted, which this kernel never
//! touches, so one board module serves both. Everything but the board
//! facts below is the chip module's (`rp2350.rs`): the boot block, the
//! clock postures, the console on UART0 (GPIO 0 transmits, GPIO 1
//! receives -- the pair a debug probe's serial bridge is documented
//! for) and the console mirror a debug probe reads the transcript from.

pub use super::rp2350::*;

pub const NAME: &str = "Raspberry Pi Pico 2";
/// The chip, as the SDK's facts file names it (the quiet line of the log doors).
pub const CHIP: &str = "RP2350A";
pub const CONSOLE: &str = "UART0 (GPIO 0 TX, GPIO 1 RX)";

/// The boot kheap carve out of the arena (the GC board table's
/// `kheap_carve` row): the 512 K-class carve the other board of that
/// class boots its verdicts on.
pub const KHEAP_BYTES: usize = 16 * 1024;
