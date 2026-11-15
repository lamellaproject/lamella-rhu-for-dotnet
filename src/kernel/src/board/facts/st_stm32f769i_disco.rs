//! The STM32F769I-DISCO's GPIO facts: the board's generated description,
//! vendored beside this file at its tagged version (`vendored/`), read here
//! into the members the kernel's API and drivers use. The block's base,
//! the ports' clock enable and the console's pins are DERIVED from the
//! vendored constants and tied to the manuals' values below; the port
//! count, the line count and the debug probe's pins are RHU's own (the
//! description does not carry them), cited. The named signals -- the two
//! user LEDs and the user button -- are the description's rows, never
//! typed here.
//!
//! RM0410: Table 1 (GPIOA at 0x4002_0000, one 1 KB block per port, A to
//! K); 5.3.10 RCC_AHB1ENR (offset 0x30; GPIOAEN bit 0 to GPIOKEN bit 10).
//! UM2033: SWD on PA13 and PA14.

#[allow(dead_code)] // the description's whole emission; the kernel reads what it needs
#[path = "vendored/st_stm32f769i_disco_bindings.rs"]
mod bindings;

use crate::claims;
use crate::gpio_math;

/// Eleven ports of sixteen lines: PA0 = 0 ... PK15 = 175.
#[allow(dead_code)] // the facts' statement of the block; the driver derives the port from the line
pub const PORT_COUNT: u32 = 11;
pub const LINE_COUNT: usize = 176;

/// The block's base and the ports' clock enable, from the description's
/// console binding: the transmit pin's MODER register is port A's block
/// (the block's first register), its RCC enable register is the ports'.
/// Both tied to the manual's values.
pub const GPIO_BASE: usize = bindings::VCP_TX_MODER_REG as usize;
pub const RCC_GPIO_EN: *mut u32 = bindings::VCP_TX_PORT_RCC_EN_REG as *mut u32;
const _: () = assert!(GPIO_BASE == 0x4002_0000, "the description's block base is not RM0410's GPIOA");
const _: () = assert!(bindings::VCP_TX_PORT_RCC_EN_REG == 0x4002_3830, "the description's clock enable is not RM0410's RCC_AHB1ENR");
const _: () = assert!(bindings::VCP_RX_MODER_REG == bindings::VCP_TX_MODER_REG, "the console's two pins are not on one port");

/// A line number from a port's block base and a pin index (the
/// description states a device's port by its block base).
const fn line(port_base: u32, pin: u32) -> u32 {
    ((port_base as usize - GPIO_BASE) / 0x400) as u32 * 16 + pin
}

/// A pin index from a MODER register's two-bit field mask.
const fn pin_of_moder_mask(mask: u32) -> u32 {
    mask.trailing_zeros() / 2
}

/// The console's transmit and receive pins from the description (PA9 and
/// PA10); the debug probe's PA13 and PA14 (UM2033).
pub const CONSOLE_TX: u32 = line(bindings::VCP_TX_MODER_REG, pin_of_moder_mask(bindings::VCP_TX_MODER_MASK));
pub const CONSOLE_RX: u32 = line(bindings::VCP_RX_MODER_REG, pin_of_moder_mask(bindings::VCP_RX_MODER_MASK));
const _: () = assert!(CONSOLE_TX == 9 && CONSOLE_RX == 10, "the console's pins are not PA9 and PA10");
pub const BOARD_CLAIMS: &[u16] = if cfg!(feature = "net") { &[CONSOLE_TX as u16, CONSOLE_RX as u16, 13, 14, 16 + 3, 16 + 4, 3 * 16 + 6, 3 * 16 + 7, 6 * 16 + 9, 6 * 16 + 10, 8 * 16 + 15, 16 + 2, 16 + 6, 2 * 16 + 9, 2 * 16 + 10, 3 * 16 + 13, 4 * 16 + 2, 1, 2, 7, 2 * 16 + 1, 2 * 16 + 4, 2 * 16 + 5, 3 * 16 + 5, 6 * 16 + 11, 6 * 16 + 13, 6 * 16 + 14] } else { &[CONSOLE_TX as u16, CONSOLE_RX as u16, 13, 14, 16 + 3, 16 + 4, 3 * 16 + 6, 3 * 16 + 7, 6 * 16 + 9, 6 * 16 + 10, 8 * 16 + 15, 16 + 2, 16 + 6, 2 * 16 + 9, 2 * 16 + 10, 3 * 16 + 13, 4 * 16 + 2] };
pub const BOARD_CLAIM_BITS: [u32; claims::BANKS] = claims::fold_board(BOARD_CLAIMS);
pub const BANNER: &str = if cfg!(feature = "net") { "176 lines, 27 claimed by the board (the console PA9/PA10, the debug pins PA13/PA14, the microSD socket PB3/PB4/PD6/PD7/PG9/PG10 and its detect PI15, the quad-SPI flash PB2/PB6/PC9/PC10/PD13/PE2, the Ethernet transceiver PA1/PA2/PA7/PC1/PC4/PC5/PG11/PG13/PG14 and its error line PD5)" } else { "176 lines, 17 claimed by the board (the console PA9/PA10, the debug pins PA13/PA14, the microSD socket PB3/PB4/PD6/PD7/PG9/PG10 and its detect PI15, the quad-SPI flash PB2/PB6/PC9/PC10/PD13/PE2)" };
/// The microSD socket (UM2033 5.10 and Table 14, the schematic's
/// sheet 13, the chip datasheet's Table 13): the six bus pads with their
/// alternate functions -- SDMMC2_CK PD6 and SDMMC2_CMD PD7 at AF11, SDMMC2_D0
/// PG9 and SDMMC2_D1 PG10 at AF11, SDMMC2_D2 PB3 and SDMMC2_D3 PB4 at AF10
/// -- and the detect line PI15 (the socket's switch to ground, the chip's
/// own pull-up: LOW with a card seated). All seven are board claims above.
pub const SD_PADS: [(u32, u8); 6] = [(3 * 16 + 6, 11), (3 * 16 + 7, 11), (6 * 16 + 9, 11), (6 * 16 + 10, 11), (16 + 3, 10), (16 + 4, 10)];
pub const SD_D0_LINE: u32 = 6 * 16 + 9;
pub const SD_DETECT_LINE: u32 = 8 * 16 + 15;
/// The quad-SPI block and the board's six flash pins (RM0410 Table 1: the
/// QUADSPI registers at 0xA000_1000 on AHB3; 5.3.12 RCC_AHB3ENR, bit 1
/// QSPIEN; UM2033's pin assignment table and the schematic's flash sheet,
/// the functions by DS11532 Table 13): QUADSPI_CLK PB2 at AF9, BK1_NCS PB6
/// at AF10, BK1_IO0 PC9, BK1_IO1 PC10, BK1_IO2 PE2 and BK1_IO3 PD13 at AF9.
/// All six are board claims above.
pub const QSPI: crate::board::stm32f7_qspi::QspiFacts = crate::board::stm32f7_qspi::QspiFacts {
    base: 0xA000_1000,
    enable_reg: 0x4002_3838,
    enable_bit: 1,
    pins: [(16 + 2, 9), (16 + 6, 10), (2 * 16 + 9, 9), (2 * 16 + 10, 9), (4 * 16 + 2, 9), (3 * 16 + 13, 9)],
};

/// The description's rows: two user LEDs, lit when their line is low, and
/// the user button, high when pressed; the description emits no pull for
/// the button (the board's own circuit holds its level), so none is applied.
#[allow(dead_code)]
pub const LED0_LINE: u32 = line(bindings::LED0_PORT_BASE, bindings::LED0_PIN);
#[allow(dead_code)] // the silicon leg's check and the flights read the rows
pub const LED0_ACTIVE_LOW: bool = bindings::LED0_ACTIVE_LOW != 0;
#[allow(dead_code)] // the silicon leg's check and the flights read the rows
pub const LED1_LINE: u32 = line(bindings::LED1_PORT_BASE, bindings::LED1_PIN);
#[allow(dead_code)] // the silicon leg's check and the flights read the rows
pub const LED1_ACTIVE_LOW: bool = bindings::LED1_ACTIVE_LOW != 0;
#[allow(dead_code)] // the silicon leg's check and the flights read the rows
pub const BUTTON0_LINE: u32 = line(bindings::BUTTON0_PORT_BASE, bindings::BUTTON0_PIN);
#[allow(dead_code)] // the silicon leg's check and the flights read the rows
pub const BUTTON0_ACTIVE_LOW: bool = bindings::BUTTON0_ACTIVE_LOW != 0;
#[allow(dead_code)] // the silicon leg's check and the flights read the rows
pub const BUTTON0_PULL: u32 = gpio_math::PULL_NONE;
const _: () = assert!(bindings::LED0_MASK == 1 << bindings::LED0_PIN, "LED0's mask is not its pin's bit");
const _: () = assert!(bindings::LED1_MASK == 1 << bindings::LED1_PIN, "LED1's mask is not its pin's bit");
const _: () = assert!(bindings::BUTTON0_MASK == 1 << bindings::BUTTON0_PIN, "BUTTON0's mask is not its pin's bit");
const _: () = assert!(bindings::DEVICE_COUNT == 3, "the description's device count is not the three rows read here");

/// The api flavor's check lines: LED0 as the output claimed at its initial
/// level and toggled; LED1 as the input the software-triggered edges are
/// counted on (a defined level through the LED's resistor: a quiet pad) and
/// as the free line of the claim and refusal checks; the button as the
/// input-only line; the console's transmit pin as the board-claimed line.
#[allow(dead_code)] // the api flavor's checks
pub const CHECK_FREE: u32 = LED1_LINE;
#[allow(dead_code)] // the api flavor's checks
pub const CHECK_OUT: Option<u32> = Some(LED0_LINE);
#[allow(dead_code)] // the api flavor's checks
pub const CHECK_IN: u32 = LED1_LINE;
#[allow(dead_code)] // the api flavor's checks
pub const CHECK_BOARD: u32 = CONSOLE_TX;
/// No loopback on a board: the software trigger makes the edge beside the toggle.
#[allow(dead_code)] // the api flavor's checks
pub const LOOPBACK: bool = false;

/// The button's line is an input only: an output claim on it is refused.
pub const fn input_only(line: u32) -> bool {
    line == BUTTON0_LINE
}

#[allow(dead_code)] // the api flavor's checks
pub const CHECK_INPUT_ONLY: Option<u32> = Some(BUTTON0_LINE);

/// The pins this part's package bonds out, one mask per port (bit n is pin
/// n): the STM32F769NI in its 216-ball package, from the part's datasheet
/// (DS11532 Table 11, the pin and ball definitions, the package's column;
/// the same set as the package's ballout figure). Ports A to I are whole;
/// port J lacks pins 6 to 11 and port K has pins 3 to 7 alone (the display
/// interface takes those balls on this part).
/// A program may claim any pin listed here, routed to a header or not; a
/// line inside the space that names a pin the package does not have is
/// refused.
pub const PRESENT: [u16; PORT_COUNT as usize] = [0xFFFF, 0xFFFF, 0xFFFF, 0xFFFF, 0xFFFF, 0xFFFF, 0xFFFF, 0xFFFF, 0xFFFF, 0xF03F, 0x00F8];

pub const fn populated(line: u32) -> bool {
    (line as usize) < LINE_COUNT && PRESENT[line as usize / 16] & (1 << (line % 16)) != 0
}

/// A line inside the space that the package does not bond out (PJ6).
#[allow(dead_code)] // the api flavor's checks
pub const CHECK_UNPOPULATED: Option<u32> = Some(9 * 16 + 6);
const _: () = assert!(!populated(9 * 16 + 6) && populated(CONSOLE_TX) && populated(CONSOLE_RX) && populated(13) && populated(14) && populated(LED0_LINE) && populated(LED1_LINE) && populated(BUTTON0_LINE), "the present list contradicts the check line, the board's own pins or a named signal");
const _: () = assert!(populated(QSPI.pins[0].0) && populated(QSPI.pins[1].0) && populated(QSPI.pins[2].0) && populated(QSPI.pins[3].0) && populated(QSPI.pins[4].0) && populated(QSPI.pins[5].0), "a flash pin is not a pin of the package");

/// The classic external-interrupt arm's blocks, which the family GPIO
/// driver reads from the board's facts (RM0410): the EXTI block at
/// 0x4001_3C00 (11.9), the first SYSCFG port-selection register at
/// 0x4001_3808 (7.2.3), the SYSCFG clock enable RCC_APB2ENR (5.3.14) with
/// its bit 14.
pub const EXTI_BASE: usize = 0x4001_3C00;
pub const SYSCFG_EXTICR1: usize = 0x4001_3808;
pub const RCC_SYSCFG_EN: *mut u32 = 0x4002_3844 as *mut u32;
pub const RCC_SYSCFG_EN_BIT: u32 = 1 << 14;

/// The bus tables of this row, one entry per bus, each named by the chip's
/// own block number: the buses the board's Arduino-style connector carries
/// (UM2033 Table 6) -- I2C1 on D15 and D14 (PB8 the clock, PB9 the data,
/// alternate function 4) and SPI2 on D13, D12 and D11 (PA12 the clock, PB14
/// the input, PB15 the output, alternate function 5; DS11532 Table 13), the
/// connector's D10, D9 and D8 (PA11, PH6, PJ4) the three chip selects. RM0410: Table 1
/// (I2C1 at 0x4000_5400, SPI2 at 0x4000_3800); 5.3.13 RCC_APB1ENR (I2C1EN
/// bit 21, SPI2EN bit 14); 5.3.26 RCC_DCKCFGR2 (I2C1SEL bits 17:16, the
/// value 10 the 16 MHz internal oscillator, which runs at both clock
/// configurations). SPI2 has no selection: its clock is the APB1 bus clock,
/// so its rate ladder follows the clock configuration, and the rate a
/// transfer achieves is what the kernel call answers. A board's other buses
/// are further entries here and nothing else.
pub const I2C_BUSES: &[crate::bus_math::I2cBus] = &[crate::bus_math::I2cBus {
    number: 1,
    base: 0x4000_5400,
    enable_reg: 0x4002_3840,
    enable_bit: 21,
    mux_reg: 0x4002_3890,
    mux_shift: 16,
    mux_value: 0b10,
    kernel_hz: 16_000_000,
    pads: &[
        crate::bus_math::Pad { line: 16 + 8, alternate: 4, role: crate::bus_math::PadRole::I2cClock },
        crate::bus_math::Pad { line: 16 + 9, alternate: 4, role: crate::bus_math::PadRole::I2cData },
    ],
}];
/// Fast mode plus on I2C1. DS11532 Table 84 guarantees the mode's timings
/// with an I2C clock above 16 MHz, and the 16 MHz internal oscillator runs
/// up to 8 % slow (Table 45): the block takes the APB1 clock for the mode
/// (RCC_DCKCFGR2 I2C1SEL = 00) where that clock stands above 16 MHz even
/// then, and the row serves no fast mode plus where it does not. The two
/// pads have no drive of the mode's 20 mA to enable (DS11532 6.3.29).
pub const I2C_FAST_PLUS: Option<crate::bus_math::I2cFastPlus> = if crate::board::PCLK1_HZ as u64 * 92 / 100 > 16_000_000 {
    Some(crate::bus_math::I2cFastPlus { mux_value: 0b00, kernel_hz: crate::board::PCLK1_HZ, drive: false })
} else {
    None
};
const _: () = assert!(
    match I2C_FAST_PLUS {
        Some(f) => crate::bus_math::i2c_timing(f.kernel_hz, crate::bus_math::I2C_RATES_HZ[2]).is_some(),
        None => true,
    },
    "the kernel clock of fast mode plus has no timing word"
);
pub const SPI_BUSES: &[crate::bus_math::SpiBus] = &[crate::bus_math::SpiBus {
    number: 2,
    base: 0x4000_3800,
    file: crate::bus_math::SpiFile::F7,
    enable_reg: 0x4002_3840,
    enable_bit: 14,
    mux_reg: 0,
    mux_shift: 0,
    mux_value: 0,
    kernel_hz: crate::board::PCLK1_HZ,
    pads: &[
        crate::bus_math::Pad { line: 12, alternate: 5, role: crate::bus_math::PadRole::SpiClock },
        crate::bus_math::Pad { line: 16 + 14, alternate: 5, role: crate::bus_math::PadRole::SpiIn },
        crate::bus_math::Pad { line: 16 + 15, alternate: 5, role: crate::bus_math::PadRole::SpiOut },
    ],
    selects: &[11, 7 * 16 + 6, 9 * 16 + 4],
    select_count: 3,
    ahead: 2,
}];
const _: () = assert!(populated(16 + 8) && populated(16 + 9) && populated(12) && populated(16 + 14) && populated(16 + 15) && populated(11) && populated(7 * 16 + 6) && populated(9 * 16 + 4), "a bus pad is not a pin of the package");
const _: () = assert!(SPI_BUSES[0].kernel_hz == crate::board::SYSCLK_HZ || SPI_BUSES[0].kernel_hz * 4 == crate::board::SYSCLK_HZ, "SPI2's clock is not this clock configuration's APB1 clock");
const _: () = assert!(SPI_BUSES[0].select_count == 3 && SPI_BUSES[0].selects[0] == 11 && SPI_BUSES[0].selects[1] == 7 * 16 + 6 && SPI_BUSES[0].selects[2] == 9 * 16 + 4, "SPI2's chip selects are not D10 (PA11), D9 (PH6) and D8 (PJ4), in that order");

/// The PWM tables of this row: the six PWM pins of the board's Arduino-style
/// connector (UM2033 Table 6), each a channel of one of the chip's timers at
/// the alternate function the chip's datasheet states (DS11532 Table 13) --
/// D3 on PF6 (TIM10 channel 1, AF3), D5 on PC8 (TIM3 channel 3, AF2), D6 on
/// PF7 (TIM11 channel 1, AF3), D9 on PH6 (TIM12 channel 1, AF9), D10 on PA11
/// (TIM1 channel 4, AF1) and D11 on PB15 (TIM12 channel 2, AF9). D9 and D11
/// are two channels of TIM12 and share its period. D10 and D9 are also the
/// first two chip selects of SPI2 above and D11 its output: a line serves
/// one function at a time, and the first claim holds it. The blocks (RM0410:
/// Table 1 for the bases; 5.3.13 RCC_APB1ENR at 0x4002_3840 with TIM3EN at
/// bit 1 and TIM12EN at bit 6; 5.3.14 RCC_APB2ENR at 0x4002_3844 with TIM1EN
/// at bit 0, TIM10EN at bit 17 and TIM11EN at bit 18): TIM1 is an advanced
/// timer, TIM3 a general-purpose one with four channels, TIM10 and TIM11
/// have one channel each and TIM12 two. A timer's clock follows its bus by
/// the manual's rule (5.2: the bus clock at a bus prescaler of 1, twice it
/// otherwise), computed here from the clock configuration's constants: 216
/// MHz on APB2 and 108 MHz on APB1 at the PLL configuration, 16 MHz on both
/// at the internal oscillator's. The rule holds while the timer prescaler
/// selector stays at its reset value (5.3.25 RCC_DCKCFGR1 at 0x4002_388C,
/// TIMPRE at bit 24); the backend reads it and records what it found. A
/// line's number is its port and pin (PA0 is 0, sixteen a port), written as
/// a plain number in the table and tied to the port and pin below.
pub const PWM_BLOCKS: &[crate::stm32_tim_math::TimBlock] = &[
    crate::stm32_tim_math::TimBlock { number: 1, base: 0x4001_0000, enable_reg: 0x4002_3844, enable_bit: 0, apb: 2, advanced: true, channels: 4 },
    crate::stm32_tim_math::TimBlock { number: 3, base: 0x4000_0400, enable_reg: 0x4002_3840, enable_bit: 1, apb: 1, advanced: false, channels: 4 },
    crate::stm32_tim_math::TimBlock { number: 10, base: 0x4001_4400, enable_reg: 0x4002_3844, enable_bit: 17, apb: 2, advanced: false, channels: 1 },
    crate::stm32_tim_math::TimBlock { number: 11, base: 0x4001_4800, enable_reg: 0x4002_3844, enable_bit: 18, apb: 2, advanced: false, channels: 1 },
    crate::stm32_tim_math::TimBlock { number: 12, base: 0x4000_1800, enable_reg: 0x4002_3840, enable_bit: 6, apb: 1, advanced: false, channels: 2 },
];
pub const PWM_TIMERS: &[crate::pwm_math::PwmTimer] = &crate::stm32_tim_math::timers::<{ PWM_BLOCKS.len() }>(PWM_BLOCKS, crate::board::PCLK1_HZ, crate::board::PCLK2_HZ, crate::board::SYSCLK_HZ);
pub const PWM_LINES: &[crate::pwm_math::PwmLine] = &[
    crate::pwm_math::PwmLine { line: 86, timer: 2, channel: 0, alternate: 3 },
    crate::pwm_math::PwmLine { line: 40, timer: 1, channel: 2, alternate: 2 },
    crate::pwm_math::PwmLine { line: 87, timer: 3, channel: 0, alternate: 3 },
    crate::pwm_math::PwmLine { line: 118, timer: 4, channel: 0, alternate: 9 },
    crate::pwm_math::PwmLine { line: 11, timer: 0, channel: 3, alternate: 1 },
    crate::pwm_math::PwmLine { line: 31, timer: 4, channel: 1, alternate: 9 },
];
/// The timer prescaler selector's register and bit, on a chip that has one.
pub const PWM_SELECTOR: Option<(usize, u32)> = Some((0x4002_388C, 24));
const _: () = assert!(PWM_LINES[0].line as u32 == 5 * 16 + 6 && PWM_LINES[1].line as u32 == 2 * 16 + 8 && PWM_LINES[2].line as u32 == 5 * 16 + 7 && PWM_LINES[3].line as u32 == 7 * 16 + 6 && PWM_LINES[4].line as u32 == 11 && PWM_LINES[5].line as u32 == 16 + 15, "a PWM line's number is not its port and pin");
const _: () = assert!(populated(5 * 16 + 6) && populated(2 * 16 + 8) && populated(5 * 16 + 7) && populated(7 * 16 + 6) && populated(11) && populated(16 + 15) && populated(2 * 16 + 2), "a PWM line, or the flight check's input line, is not a pin of the package");
/// The lines of the flight check's jumper wire: the PWM line whose edges the wire
/// carries, and the plain input that counts them -- D9 (PH6) to A2 (PC2).
#[allow(dead_code)]
pub const PWM_WIRE_OUT: u32 = 7 * 16 + 6;
#[allow(dead_code)]
pub const PWM_WIRE_IN: u32 = 2 * 16 + 2;

/// The ADC tables of this row: the six analog pins of the board's
/// Arduino-style connector (UM2033 Table 14) and the chip's internal
/// reference as a seventh channel, a channel without a pin. The connector's
/// A0, A1 and A2 are channels of ADC1 (PA6 channel 6, PA4 channel 4, PC2
/// channel 12); A3, A4 and A5 exist on ADC3 alone (PF10 channel 8, PF8
/// channel 6, PF9 channel 7; DS11532 Table 11), so the row carries two
/// converter blocks; the internal reference is ADC1's channel 17 (RM0410
/// 15.3.3). Both blocks: 12 bits; the reference voltage the board's 3.3 V
/// analog supply; the converters' clock the APB2 clock over 4 (27 MHz at
/// the PLL configuration, 4 MHz at the internal oscillator's, inside
/// DS11532 Table 72's 0.6 to 36 MHz); a conversion 492 cycles, the longest
/// sampling time of 480 and the conversion's 12 (RM0410 15.5). A pin of the
/// connector that a seated shield holds above the analog supply must not be
/// claimed as an analog channel: the datasheet's 5 V tolerance does not
/// hold in analog mode (DS11532 Table 11, its note on 5 V tolerant pins). A line's number is its
/// port and pin (PA0 is 0, sixteen a port), written as a plain number in
/// the table and tied to the port and pin below.
pub const ADC_BLOCKS: &[crate::adc_math::AdcBlock] = &[
    crate::adc_math::AdcBlock { number: 1, clock_hz: crate::board::PCLK2_HZ / 4, bits: 12, reference_mv: 3300, conversion_cycles: 492 },
    crate::adc_math::AdcBlock { number: 3, clock_hz: crate::board::PCLK2_HZ / 4, bits: 12, reference_mv: 3300, conversion_cycles: 492 },
];
pub const ADC_CHANNELS: &[crate::adc_math::AdcChannel] = &[
    crate::adc_math::AdcChannel { block: 0, channel: 6, line: 6 },
    crate::adc_math::AdcChannel { block: 0, channel: 4, line: 4 },
    crate::adc_math::AdcChannel { block: 0, channel: 12, line: 34 },
    crate::adc_math::AdcChannel { block: 1, channel: 8, line: 90 },
    crate::adc_math::AdcChannel { block: 1, channel: 6, line: 88 },
    crate::adc_math::AdcChannel { block: 1, channel: 7, line: 89 },
    crate::adc_math::AdcChannel { block: 0, channel: 17, line: crate::adc_math::NO_LINE },
];
/// The converter blocks for the backend, by the block table's position
/// (RM0410 Table 1: the converters' window at 0x4001_2000; Table 102: ADC1
/// at 0x000 of it, ADC3 at 0x200, the common registers at 0x300; 5.3.14
/// RCC_APB2ENR at 0x4002_3844 with ADC1EN at bit 8 and ADC3EN at bit 10).
pub const ADC_UNITS: &[crate::stm32_adc_math::AdcUnit] = &[
    crate::stm32_adc_math::AdcUnit { base: 0x4001_2000, enable_reg: 0x4002_3844, enable_bit: 8 },
    crate::stm32_adc_math::AdcUnit { base: 0x4001_2200, enable_reg: 0x4002_3844, enable_bit: 10 },
];
pub const ADC_COMMON: usize = 0x4001_2300;
const _: () = assert!(ADC_CHANNELS[0].line as u32 == 6 && ADC_CHANNELS[1].line as u32 == 4 && ADC_CHANNELS[2].line as u32 == 2 * 16 + 2 && ADC_CHANNELS[3].line as u32 == 5 * 16 + 10 && ADC_CHANNELS[4].line as u32 == 5 * 16 + 8 && ADC_CHANNELS[5].line as u32 == 5 * 16 + 9, "an analog channel's line is not its port and pin");
const _: () = assert!(populated(6) && populated(4) && populated(2 * 16 + 2) && populated(5 * 16 + 10) && populated(5 * 16 + 8) && populated(5 * 16 + 9), "an analog pin is not a pin of the package");
/// The flight check's own list of the channels it may claim, by table
/// position: A0, A1, A4, A5 and the internal reference. The positions of
/// A2 and A3 are left out: with the Click Shield for Arduino UNO seated,
/// their pins sit on level translator channels pulled up to the shield's
/// 5 V rail, and a pin above the analog supply is never put into analog
/// mode.
#[allow(dead_code)]
pub const ADC_FLIGHT: &[usize] = &[0, 1, 4, 5, 6];
const _: () = assert!(crate::stm32_adc_math::positions_fit(ADC_FLIGHT, ADC_CHANNELS.len()), "the flight check's list names a position outside the channel table, or one twice");
/// The flight check's jumper wire: the PWM line that holds it, D6 (PF7),
/// and the table position of the channel that reads it, A0.
#[allow(dead_code)]
pub const ADC_WIRE_OUT: u32 = 5 * 16 + 7;
#[allow(dead_code)]
pub const ADC_WIRE_IN: usize = 0;

/// The RAM ranges of this row (the hosted runtime learns them
/// through the `rhu_ram_count` and `rhu_ram_range` functions): the physical
/// range as the board's facts file states it in the SDK (`boards/<row>.json`,
/// its `ram` array; the two kept equal by the hosted-runtime gate's facts
/// tie), range 0 the internal SRAM the kernel's map calls DATA. Range 0's
/// `base` and `bytes` are the map's DATA region, whole: the arena window
/// the runtime is handed lies inside it, bracketed by the map.
/// 512 KB of SRAM (the DTCM, SRAM1 and SRAM2, one contiguous window); the
/// 16 MB SDRAM behind the flexible memory controller's bank 1, the
/// caches off, execute-never (the protection unit's region 3).
pub const RAM_RANGES: &[crate::ram::RamFacts] = &[
    crate::ram::RamFacts { name: "SRAM", base: 0x2000_0000, bytes: 0x8_0000, external: false, cached: false, exec: true },
    crate::ram::RamFacts { name: "SDRAM", base: 0xC000_0000, bytes: 0x100_0000, external: true, cached: false, exec: false },
];

/// The flash ranges of this row (the SDK's facts file's `flash` array, kept
/// equal by the hosted-runtime gate's facts tie): the chip's 2 MB flash at
/// 0x08000000 (the boot flash, the kernel's map's CODE and APP regions),
/// and the board's 64 MB quad-SPI part memory-mapped at 0x90000000 (the
/// window the execute-in-place map places the hosted program's code and
/// read-only data in; read-only, executable by the protection unit's
/// region 2 when an image's map declares it).
pub const FLASH_RANGES: &[crate::ram::FlashFacts] = &[
    crate::ram::FlashFacts { name: "FLASH", base: 0x0800_0000, bytes: 0x20_0000, external: false, mapped: true, executable: true },
    crate::ram::FlashFacts { name: "QSPI", base: 0x9000_0000, bytes: 0x400_0000, external: true, mapped: true, executable: true },
];

/// The console's channel facts of this row (the SDK's `channels` member):
/// the two streams' default channel codes and the row's legal set
/// (`crate::console`: 1 off, 2 memory, 3 probe, 0x10 + n uart n).
pub const CHANNELS: crate::console::ChannelFacts = crate::console::ChannelFacts { log: crate::console::CH_PROBE, console: crate::console::CH_PROBE, legal: &[crate::console::CH_PROBE, crate::console::CH_MEMORY, crate::console::CH_OFF] };
/// The pre-defined UART rows of this row (the SDK's `uart` member), by the
/// chip's own block number with their pins: one port, USART6 on the
/// Arduino-style connector's D1 (PC6, transmit) and D0 (PC7, receive)
/// (UM2033 Table 14), where a seated shield puts a click's serial lines.
/// The kernel's UART API reads the table. The console keeps its own block,
/// USART1 on the lines to the board's probe: it is not a row here and is
/// never a program's port. A line's number is its port and pin, written as
/// a plain number in the table and tied to the port and pin below.
pub const UART_ROWS: &[crate::console::UartRow] = &[crate::console::UartRow { block: 6, tx: 38, rx: 39 }];
/// The gpio banner's line when the kernel holds a UART of this row as its
/// console wire, by block (empty on a row with no UART row).
pub const BANNER_UART: &[&str] = &[];
/// The UART rows' names for the boot banner's console clause, by block.
#[allow(dead_code)] // the Pico rows' banner clause reads it
pub const UART_NAMES: &[&str] = &[];
/// No pin override on this row (the port's pair is fixed by the board's
/// connector): the SDK's `uartPairs` member is empty.
pub const UART_PAIRS: &[crate::console::UartPairs] = &[];
/// No both-held banner line on this row.
pub const BANNER_UART_BOTH: &str = "";
/// The serial block behind each row of `UART_ROWS`, by the table's
/// position, for the backend. USART6 at 0x4001_1400 (RM0410 Rev 5 page 79),
/// a USART at oversampling by 16; its bit 5 in the clock controller's
/// three APB2 registers (the reset register at 0x24, the clock enable at
/// 0x44, the clock while the core sleeps at 0x64; pages 182, 192 and 203);
/// its kernel clock selector at bits 11:10 of the dedicated clocks
/// register at 0x90, the code 10 the 16 MHz internal oscillator (page
/// 217); no FIFOs; both pins at alternate function 8 (DS11532 Table 13);
/// the interrupt line 71 (RM0410 Table 46). The block's kind -- a USART or
/// a low-power UART -- is stated by its row's `law` field.
pub const UART_BLOCKS: &[crate::stm32_usart_math::Block] = &[crate::stm32_usart_math::Block { base: 0x4001_1400, law: crate::stm32_usart_math::Law::Usart16, enable_reg: 0x4002_3844, reset_reg: 0x4002_3824, sleep_reg: 0x4002_3864, bus_bit: 5, select_reg: 0x4002_3890, select_shift: 10, select_mask: 0x3, select_code: 0x2, fifo: false, alternate: 8, irq: 71 }];
/// The two rings of a port, in bytes. This row offers no port as a console
/// wire, the use the larger transmit ring of the other serial rows is
/// sized for, and its own RAM is the tightest of the rows: a program's
/// write copies what fits and its blocking write waits for room, so a
/// smaller ring loses nothing. The receive ring holds 89 ms of a 115200 Bd
/// wire.
pub const UART_RX_BYTES: usize = 1024;
pub const UART_TX_BYTES: usize = 1024;
const _: () = assert!(UART_ROWS.len() == 1 && UART_ROWS[0].block == 6 && UART_ROWS[0].tx as u32 == 2 * 16 + 6 && UART_ROWS[0].rx as u32 == 2 * 16 + 7, "the serial port's row is not USART6 on D1 (PC6) and D0 (PC7)");
const _: () = assert!(populated(2 * 16 + 6) && populated(2 * 16 + 7) && crate::stm32_usart_math::pair_sound(UART_ROWS[0].tx as u32, UART_ROWS[0].rx as u32), "a serial line is not a pin of the package, or the port's two lines are one");
const _: () = assert!(crate::stm32_usart_math::lines_free(&BOARD_CLAIM_BITS, UART_ROWS[0].tx as u32, UART_ROWS[0].rx as u32), "a serial line is among the board's own claims");
/// The Ethernet transceiver under the `net` feature (UM2033 Table 14, the
/// schematic's Ethernet sheet, the chip datasheet's alternate function
/// table): the nine lines of its RMII and its management interface, each
/// at alternate function 11 -- the reference clock PA1, the management
/// data PA2, carrier and data valid PA7, the management clock PC1, the two
/// receive data lines PC4 and PC5, transmit enable PG11, the two transmit
/// data lines PG13 and PG14. The transceiver's receive error output is on
/// PD5, a plain input the board holds and the controller does not read.
#[cfg(feature = "net")]
pub const ETH_PADS: [(u32, u8); 9] = [(1, 11), (2, 11), (7, 11), (2 * 16 + 1, 11), (2 * 16 + 4, 11), (2 * 16 + 5, 11), (6 * 16 + 11, 11), (6 * 16 + 13, 11), (6 * 16 + 14, 11)];
/// The transceiver's management address: its address strap is held low.
#[cfg(feature = "net")]
pub const ETH_PHY_ADDRESS: u8 = 0;
/// The RAM the controller's DMA reaches: from the end of the core's own
/// 128 K at the region's start to the region's end.
#[cfg(feature = "net")]
pub const ETH_DMA_WINDOW: (usize, usize) = (0x2002_0000, 0x2008_0000);
