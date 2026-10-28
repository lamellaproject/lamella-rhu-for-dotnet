//! Adafruit Feather RP2350 with HSTX (RP2350A, the 60-pin package; 8 MB of
//! QSPI flash; the 12 MHz crystal; no wireless module; the HSTX connector
//! and the addressable LED are a program's lines, which this kernel never
//! drives). Everything but the board facts below is the chip module's
//! (`rp2350.rs`): the boot block, the clock postures, the console on
//! UART0 (GPIO 0 transmits, GPIO 1 receives) and the console mirror a
//! debug probe reads the transcript from.

pub use super::rp2350::*;

pub const NAME: &str = "Adafruit Feather RP2350 with HSTX";
/// The chip, as the SDK's facts file names it (the quiet line of the log doors).
pub const CHIP: &str = "RP2350A";
pub const CONSOLE: &str = "UART0 (GPIO 0 TX, GPIO 1 RX)";

/// The boot kheap carve out of the arena (the GC board table's
/// `kheap_carve` row): the 512 K-class carve, the same as the Pico 2's.
pub const KHEAP_BYTES: usize = 16 * 1024;
