//! The facts the three Arduino Portenta H7 rows share (the H7, the H7 Lite
//! and the H7 Lite Connected carry one chip, one power-management chip, one
//! oscillator arrangement and one supply wiring on one drawing; they differ
//! by what is populated, which each row's facts module states). Each
//! Portenta board file re-exports the family module and this one. Facts per
//! the family's schematics (SL-ABX00042 V4.10, one drawing stamped with the
//! three product IDs), the Portenta Mid Carrier's (ASX00055 V1.2) and
//! RM0399.

use super::stm32h7::Supply;

/// The probe channel's wire on the Portenta H7 rows. The Machine Control row
/// shadows it with its own text (no console wire reaches that carrier's
/// terminals), so this one is unused there.
#[allow(dead_code)] // shadowed by the Machine Control row's own console text
pub const CONSOLE: &str = "USART1 (PA9 TX, PA10 RX: the carrier's J14 pins 10 and 8; the probe's serial port or a 3.3 V adapter)";

/// The console's pins as (port index, pin): PA9 and PA10 (USART1), on the
/// module's connector J1 and the carrier's UART header. The family routes
/// them to USART1 when a stream names the wire.
pub const CONSOLE_PINS: [(usize, u32); 2] = [(0, 9), (0, 10)];

/// The chip's supply wiring, from the drawing: VDD and the switching
/// regulator's input on the power-management chip's 3.1 V rail, the
/// inductor L2 fitted, the regulator's feedback feeding VDDLDO through a
/// fitted solder jumper, the capacitor banks fitted -- the manual's
/// configuration in which the switching regulator at 1.8 V supplies the
/// linear regulator. The maker's own board files name the same
/// configuration. The same on all three variants: no power part differs.
pub const SUPPLY: Supply = Supply::SmpsFeedsLdo18;
pub const SMPS_INDUCTOR_FITTED: bool = true;

/// The board's high-speed source: a 25 MHz MEMS oscillator (U17, a
/// DSC6151 part) into PH0 in bypass, its standby pin driven by PH1 (the net
/// OSCEN; the pin rests low through 200 k until the chip drives it; high =
/// the part runs, low = standby). The same oscillator clocks the Ethernet
/// transceiver, so an act that needs that transceiver on another posture
/// drives the line itself.
pub const HSE_HZ: u32 = 25_000_000;
#[allow(dead_code)] // read by the family's PLL posture alone
pub const HSE_BYPASS: bool = true;
#[allow(dead_code)] // read by the family's PLL posture alone
pub const OSC_ENABLE: Option<(usize, u32)> = Some((7, 1));

/// The PLL's two dividers for 400 MHz from 25 MHz: /5 gives a 5 MHz
/// reference inside the 4 to 8 MHz band, x160 puts the VCO at 800 MHz, /2
/// lands the system clock at 400. The family asserts every bound.
#[allow(dead_code)] // read by the family's PLL posture alone
pub const PLL_DIVM1: u32 = 5;
#[allow(dead_code)] // read by the family's PLL posture alone
pub const PLL_DIVN1: u32 = 160;

/// The calendar banner's text for these rows, constant per image: a
/// 32.768 kHz MEMS oscillator into PC14 (the low-speed source in bypass) on
/// the power-management chip's always-on 3.0 V rail, the carrier's coin
/// cell behind it; this row does not read the clock.
pub const RTC_LINE: &str = "RTC block present on this part, a 32.768 kHz oscillator on the always-on rail (the carrier's coin cell behind it); this row does not read the clock: the base is the epoch";

/// The power-management chip's bus as a bus-table entry, under the chip's
/// reading flavor alone (`test-h7-pmic-read`; the rows' shipped
/// tables are empty). The chip's I2C1 at 0x4000_5400 (RM0399's
/// memory map); its clock enable RCC_APB1LENR bit 21, I2C1EN (the register
/// at RCC + 0x0E8); its kernel clock by RCC_D2CCIP2R I2C123SEL, bits 13:12,
/// 0b10 = hsi_ker_ck -- the internal oscillator's 64 MHz kernel clock, which
/// both postures leave running, so one timing word serves both; the pads PB6
/// (I2C1_SCL) and PB7 (I2C1_SDA) at alternate function 4 (DS12930's
/// alternate-function table), the board's own lines. The chip answers at
/// address 0x08 (the family's schematics).
#[cfg(feature = "test-h7-pmic-read")]
pub const PMIC_I2C1: crate::bus_math::I2cBus = crate::bus_math::I2cBus {
    number: 1,
    base: 0x4000_5400,
    enable_reg: 0x5802_4400 + 0x0E8,
    enable_bit: 21,
    mux_reg: 0x5802_4400 + 0x54,
    mux_shift: 12,
    mux_value: 0b10,
    kernel_hz: 64_000_000,
    pads: &[
        crate::bus_math::Pad { line: super::facts::PMIC_SCL as u16, alternate: 4, role: crate::bus_math::PadRole::I2cClock },
        crate::bus_math::Pad { line: super::facts::PMIC_SDA as u16, alternate: 4, role: crate::bus_math::PadRole::I2cData },
    ],
};
#[cfg(feature = "test-h7-pmic-read")]
const _: () = assert!(super::facts::PMIC_SCL == 16 + 6 && super::facts::PMIC_SDA == 16 + 7, "the power-management chip's bus is PB6 and PB7");
