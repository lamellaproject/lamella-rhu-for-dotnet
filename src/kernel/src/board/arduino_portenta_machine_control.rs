//! Arduino Portenta Machine Control (a Portenta H7 module on an industrial
//! carrier with 24 V terminals; STM32H747XI, the Cortex-M7 core):
//! flash-boot at 0x08000000, the memory console alone -- the maker's console
//! pins PA9 and PA10 are the carrier's serial transceiver's mode lines and
//! no debug header reaches the module, so no legal channel of this row
//! names a wire. The board facts are the Portenta family's
//! (`portenta_h7_common.rs`: the module is a Portenta H7 with the radio
//! module, the secure element and the video bridge fitted); everything else
//! is the chip family module's (`stm32h7.rs`): the supply entry, the two
//! clock postures (`arduino-portenta-machine-control-hsi64` selects the
//! internal oscillator posture), the random source and the calendar seam.
//! The carrier's terminals are the facts module's named-signal table.

pub use super::stm32h7::*;
pub use super::portenta_h7_common::*;

pub const NAME: &str = "Arduino Portenta Machine Control";

/// The wire the probe channel names on the other Portenta rows reaches no
/// terminal of this carrier (USART1's pins are the serial transceiver's
/// mode lines), and the row's legal channels omit that code, so the boot
/// takes the memory console on any word naming it; this text stands in the
/// family's console clause alone.
pub const CONSOLE: &str = "no console wire on this row (USART1's pins PA9 and PA10 are the carrier's serial transceiver's mode lines; the memory console alone)";
