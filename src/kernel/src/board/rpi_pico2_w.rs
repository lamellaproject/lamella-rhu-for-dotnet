//! Raspberry Pi Pico 2 W (RP2350A, the 60-pin package; 4 MB of QSPI
//! flash; the 12 MHz crystal; a wireless module whose lines the wifi
//! driver claims at its own open -- the boot never touches them). The
//! board's LED hangs on the wireless module, so this row states no LED
//! line. Everything but the board facts below is the chip module's
//! (`rp2350.rs`): the boot block, the clock postures, the console on
//! UART0 (GPIO 0 transmits, GPIO 1 receives -- the pair a debug probe's
//! serial bridge is documented for) and the console mirror a debug probe
//! reads the transcript from.

pub use super::rp2350::*;

pub const NAME: &str = "Raspberry Pi Pico 2 W";
pub const CONSOLE: &str = "UART0 (GPIO 0 TX, GPIO 1 RX)";

/// The boot kheap carve out of the arena (the GC board table's
/// `kheap_carve` row): the 512 K-class carve the other boards of that
/// class boot their verdicts on.
pub const KHEAP_BYTES: usize = 16 * 1024;
