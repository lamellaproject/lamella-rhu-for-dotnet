//! Arduino GIGA R1 WiFi (STM32H747XI, the Cortex-M7 core): flash-boot at
//! 0x08000000 after the board is erased, the memory console by default and
//! USART1 on the maker's console pins (PA9 transmits, PB7 receives: the
//! header's D1 and D0) as the probe channel. Everything but the board facts
//! below is the family module's (`stm32h7.rs`): the supply entry, the two
//! clock postures (`arduino-giga-r1-wifi-hsi64` selects the internal
//! oscillator posture), the console driver, the random source and the
//! calendar seam. Facts per the board's schematics (ABX00063 V1.6) and
//! RM0399.

pub use super::stm32h7::*;

pub const NAME: &str = "Arduino GIGA R1 WiFi";
pub const CONSOLE: &str = "USART1 (PA9 TX, PB7 RX: the header's D1 and D0; the probe's serial port or a 3.3 V adapter)";

/// The console's pins as (port index, pin): PA9 and PB7 (USART1 on both;
/// PA10, the chip's other receive pin, is the radio module's Bluetooth
/// enable on this board). The family routes them to USART1 when a stream
/// names the wire.
pub const CONSOLE_PINS: [(usize, u32); 2] = [(0, 9), (1, 7)];

/// The chip's supply wiring, from the schematics: VDD, VDDLDO and VDDA on
/// the 3.3 V rail, the three VCAP pins decoupled, the switching regulator's
/// pins VDDSMPS, VFBSMPS and VSSSMPS tied to ground with VLXSMPS open and NO
/// inductor fitted -- the manual's linear-regulator-alone configuration and
/// nothing else. The maker's own board files name the same configuration.
/// The doctored control forces the Portentas' wiring onto this board and
/// the family's derivation refuses the build.
#[cfg(not(feature = "test-h7-supply-doctored"))]
pub const SUPPLY: Supply = Supply::LdoOnly;
#[cfg(feature = "test-h7-supply-doctored")]
pub const SUPPLY: Supply = Supply::SmpsFeedsLdo18;
pub const SMPS_INDUCTOR_FITTED: bool = false;

/// The board's high-speed source: a 16.000 MHz crystal (Y3) on PH0 and PH1,
/// no oscillator and no enable line.
pub const HSE_HZ: u32 = 16_000_000;
#[allow(dead_code)] // read by the family's PLL posture alone
pub const HSE_BYPASS: bool = false;
#[allow(dead_code)] // read by the family's PLL posture alone
pub const OSC_ENABLE: Option<(usize, u32)> = None;

/// The PLL's two dividers for 400 MHz from 16 MHz: /3 gives a 5.33 MHz
/// reference inside the 4 to 8 MHz band, x150 puts the VCO at 800 MHz, /2
/// lands the system clock at 400. The family asserts every bound.
#[allow(dead_code)] // read by the family's PLL posture alone
pub const PLL_DIVM1: u32 = 3;
#[allow(dead_code)] // read by the family's PLL posture alone
pub const PLL_DIVN1: u32 = 150;

/// The calendar banner's text for this row, constant per image: the
/// 32.768 kHz crystal (Y2) is mounted, the backup pin is the header's VRTC
/// with no cell; this row does not read the clock.
pub const RTC_LINE: &str = "RTC block present on this part, a mounted 32.768 kHz crystal, no backup cell (the VRTC header pin); this row does not read the clock: the base is the epoch";
