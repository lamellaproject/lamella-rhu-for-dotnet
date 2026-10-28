//! Adafruit Feather RP2350 with HSTX and PSRAM (RP2350A, the 60-pin
//! package; 8 MB of QSPI flash; 8 MB of PSRAM behind the flash interface's
//! second chip select, brought up and proved at boot as the kernel's second
//! RAM range at the PLL posture (the 12 MHz crystal posture leaves it down
//! and offers the SRAM alone); the 12 MHz crystal; no wireless module).
//! Everything but the board facts below is the chip module's
//! (`rp2350.rs`): the boot block, the clock postures, the console on
//! UART0 (GPIO 0 transmits, GPIO 1 receives) and the console mirror a
//! debug probe reads the transcript from.

pub use super::rp2350::*;

pub const NAME: &str = "Adafruit Feather RP2350 with HSTX and PSRAM";
/// The chip, as the SDK's facts file names it (the quiet line of the log doors).
pub const CHIP: &str = "RP2350A";
pub const CONSOLE: &str = "UART0 (GPIO 0 TX, GPIO 1 RX)";

/// The boot kheap carve out of the arena (the GC board table's
/// `kheap_carve` row): the 512 K-class carve, the same as the Pico 2's --
/// the kheap is carved from the arena's window 0 in the chip's SRAM, as on
/// every row; the PSRAM is the arena's window 1 and no boot-time
/// carve draws from it.
pub const KHEAP_BYTES: usize = 16 * 1024;
