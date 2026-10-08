//! Pimoroni Pico Plus 2 W (RP2350B, the 80-pin package; 16 MB of QSPI
//! flash; 8 MB of PSRAM behind the flash interface's second chip select,
//! unused by this kernel; the 12 MHz crystal; a wireless module whose
//! lines the wifi driver claims at its own open -- the boot never touches
//! them). Everything but the board
//! facts below is the chip module's (`rp2350.rs`): the boot block, the
//! clock postures, the console on UART0 (GPIO 0 transmits, GPIO 1
//! receives) and the console mirror a debug probe reads the transcript
//! from.

pub use super::rp2350::*;

pub const NAME: &str = "Pimoroni Pico Plus 2 W";
pub const CONSOLE: &str = "UART0 (GPIO 0 TX, GPIO 1 RX)";

/// The boot kheap carve out of the arena (the GC board table's
/// `kheap_carve` row): the 512 K-class carve, the same as the Pico 2's --
/// this kernel keeps its arena in the chip's SRAM and leaves the PSRAM
/// unmapped.
pub const KHEAP_BYTES: usize = 16 * 1024;
