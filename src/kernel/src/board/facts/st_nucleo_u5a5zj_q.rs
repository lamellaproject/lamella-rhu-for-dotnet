//! The NUCLEO-U5A5ZJ-Q's GPIO facts: the board's generated description,
//! vendored beside this file at its tagged version (`vendored/`), read here
//! into the members the kernel's API and drivers use. The block's base,
//! the ports' clock enable and the console's pins are DERIVED from the
//! vendored constants and tied to the manuals' values below; the port
//! count, the line count and the debug probe's pins are RHU's own (the
//! description does not carry them), cited. The named signals -- the three
//! user LEDs and the user button with its pull -- are the description's
//! rows, never typed here.
//!
//! RM0456: Table 6 (GPIOA at 0x4202_0000 in the non-secure alias, one 1 KB
//! block per port, A to J); 11.8.30 RCC_AHB2ENR1 (offset 0x08C; GPIOAEN bit
//! 0 to GPIOJEN bit 9). UM2861: SWD on PA13 and PA14.

#[allow(dead_code)] // the description's whole emission; the kernel reads what it needs
#[path = "vendored/st_nucleo_u5a5zj_q_bindings.rs"]
mod bindings;

use crate::claims;
use crate::gpio_math;

/// Ten ports of sixteen lines: PA0 = 0 ... PJ15 = 159.
#[allow(dead_code)] // the facts' statement of the block; the driver derives the port from the line
pub const PORT_COUNT: u32 = 10;
pub const LINE_COUNT: usize = 160;

/// The block's base and the ports' clock enable, from the description's
/// console binding: the transmit pin's MODER register is port A's block
/// (the block's first register), its RCC enable register is the ports'.
/// Both tied to the manual's values.
pub const GPIO_BASE: usize = bindings::VCP_TX_MODER_REG as usize;
pub const RCC_GPIO_EN: *mut u32 = bindings::VCP_TX_PORT_RCC_EN_REG as *mut u32;
const _: () = assert!(GPIO_BASE == 0x4202_0000, "the description's block base is not RM0456's GPIOA");
const _: () = assert!(bindings::VCP_TX_PORT_RCC_EN_REG == 0x4602_0C8C, "the description's clock enable is not RM0456's RCC_AHB2ENR1");
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
/// PA10); the debug probe's PA13 and PA14 (UM2861).
pub const CONSOLE_TX: u32 = line(bindings::VCP_TX_MODER_REG, pin_of_moder_mask(bindings::VCP_TX_MODER_MASK));
pub const CONSOLE_RX: u32 = line(bindings::VCP_RX_MODER_REG, pin_of_moder_mask(bindings::VCP_RX_MODER_MASK));
const _: () = assert!(CONSOLE_TX == 9 && CONSOLE_RX == 10, "the console's pins are not PA9 and PA10");
pub const BOARD_CLAIMS: &[u16] = &[CONSOLE_TX as u16, CONSOLE_RX as u16, 13, 14];
pub const BOARD_CLAIM_BITS: [u32; claims::BANKS] = claims::fold_board(BOARD_CLAIMS);
pub const BANNER: &str = "160 lines, 4 claimed by the board (the console PA9/PA10, the debug pins PA13/PA14)";

/// The description's rows: three user LEDs, lit when their line is high,
/// and the user button, high when pressed, with the pull the description
/// states as two boolean constants (a pull-down here: the board's manual
/// warns the line is never to be driven, and an input with its pull-down
/// is the posture it names).
#[allow(dead_code)]
pub const LED0_LINE: u32 = line(bindings::LED0_PORT_BASE, bindings::LED0_PIN);
#[allow(dead_code)]
pub const LED0_ACTIVE_LOW: bool = bindings::LED0_ACTIVE_LOW != 0;
#[allow(dead_code)]
pub const LED1_LINE: u32 = line(bindings::LED1_PORT_BASE, bindings::LED1_PIN);
#[allow(dead_code)]
pub const LED1_ACTIVE_LOW: bool = bindings::LED1_ACTIVE_LOW != 0;
#[allow(dead_code)]
pub const LED2_LINE: u32 = line(bindings::LED2_PORT_BASE, bindings::LED2_PIN);
#[allow(dead_code)]
pub const LED2_ACTIVE_LOW: bool = bindings::LED2_ACTIVE_LOW != 0;
#[allow(dead_code)]
pub const BUTTON0_LINE: u32 = line(bindings::BUTTON0_PORT_BASE, bindings::BUTTON0_PIN);
#[allow(dead_code)] // the silicon leg's check and the flights read the rows
pub const BUTTON0_ACTIVE_LOW: bool = bindings::BUTTON0_ACTIVE_LOW != 0;
#[allow(dead_code)] // the silicon leg's check and the flights read the rows
pub const BUTTON0_PULL: u32 = if bindings::BUTTON0_PULL_DOWN != 0 {
    gpio_math::PULL_DOWN
} else if bindings::BUTTON0_PULL_UP != 0 {
    gpio_math::PULL_UP
} else {
    gpio_math::PULL_NONE
};
const _: () = assert!(!(bindings::BUTTON0_PULL_DOWN != 0 && bindings::BUTTON0_PULL_UP != 0), "the button states both pulls");
const _: () = assert!(bindings::LED0_MASK == 1 << bindings::LED0_PIN, "LED0's mask is not its pin's bit");
const _: () = assert!(bindings::LED1_MASK == 1 << bindings::LED1_PIN, "LED1's mask is not its pin's bit");
const _: () = assert!(bindings::LED2_MASK == 1 << bindings::LED2_PIN, "LED2's mask is not its pin's bit");
const _: () = assert!(bindings::BUTTON0_MASK == 1 << bindings::BUTTON0_PIN, "BUTTON0's mask is not its pin's bit");
const _: () = assert!(bindings::DEVICE_COUNT == 4, "the description's device count is not the four rows read here");

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
/// n): the STM32U5A5ZJ in its 144-pin package with the switched-mode supply,
/// from the part's datasheet (DS13543 Table 27, the pin and ball
/// definitions, the package's column; the same set as the package's pinout
/// figure). Ports A, D, E and F are whole; port B lacks pin 12, port C pins
/// 4 and 5, port G pin 11 (the supply's pins take their places); port H has
/// pins 0, 1 and 3; ports I and J have none.
/// A program may claim any pin listed here, routed to a header or not; a
/// line inside the space that names a pin the package does not have is
/// refused.
pub const PRESENT: [u16; PORT_COUNT as usize] = [0xFFFF, 0xEFFF, 0xFFCF, 0xFFFF, 0xFFFF, 0xFFFF, 0xF7FF, 0x000B, 0x0000, 0x0000];

pub const fn populated(line: u32) -> bool {
    (line as usize) < LINE_COUNT && PRESENT[line as usize / 16] & (1 << (line % 16)) != 0
}

/// A line inside the space that the package does not bond out (PB12).
#[allow(dead_code)] // the api flavor's checks
pub const CHECK_UNPOPULATED: Option<u32> = Some(16 + 12);
const _: () = assert!(!populated(16 + 12) && populated(CONSOLE_TX) && populated(CONSOLE_RX) && populated(13) && populated(14) && populated(BUTTON0_LINE), "the present list contradicts the check line, the board's own pins or a named signal");

/// The bus tables of this row, one entry per bus, each named by the chip's
/// own block number: the buses the board's Arduino-style connector carries
/// (UM2861 Table 15) -- I2C1 on D15 and D14 (PB8 the clock, PB9 the data,
/// alternate function 4) and SPI1 on D13, D12 and D11 (PA5 the clock, PA6
/// the input, PA7 the output, alternate function 5; DS13543 Table 28), the
/// connector's D10 and D9 (PD14, PD15) the two chip selects. RM0456: Table 6
/// (I2C1 at 0x4000_5400, SPI1 at 0x4001_3000); 11.8.33 RCC_APB1ENR1 (I2C1EN
/// bit 21); 11.8.35 RCC_APB2ENR (SPI1EN bit 12); 11.8.46 RCC_CCIPR1 (I2C1SEL
/// bits 11:10 and SPI1SEL bits 21:20, the value 10 the 16 MHz internal
/// oscillator, which runs at both clock configurations, so both rate
/// ladders are the same at each). A board's other buses are further entries
/// here and nothing else.
pub const I2C_BUSES: &[crate::bus_math::I2cBus] = &[crate::bus_math::I2cBus {
    number: 1,
    base: 0x4000_5400,
    enable_reg: 0x4602_0C9C,
    enable_bit: 21,
    mux_reg: 0x4602_0CE0,
    mux_shift: 10,
    mux_value: 0b10,
    kernel_hz: 16_000_000,
    pads: &[
        crate::bus_math::Pad { line: 16 + 8, alternate: 4, role: crate::bus_math::PadRole::I2cClock },
        crate::bus_math::Pad { line: 16 + 9, alternate: 4, role: crate::bus_math::PadRole::I2cData },
    ],
}];
/// Fast mode plus on I2C1: from the bus's own kernel clock (the manual's
/// clock requirement holds at the internal oscillator, RM0456 65.4.3, and
/// DS13543 states no least clock for the mode), with the drive bit of the
/// block's control register 1 (RM0456 65.9.1): PB8 and PB9 are FT_f pads,
/// the kind that sinks the mode's 20 mA (DS13543's pin table and 5.3.15).
pub const I2C_FAST_PLUS: Option<crate::bus_math::I2cFastPlus> = Some(crate::bus_math::I2cFastPlus { mux_value: I2C_BUSES[0].mux_value, kernel_hz: I2C_BUSES[0].kernel_hz, drive: true });
pub const SPI_BUSES: &[crate::bus_math::SpiBus] = &[crate::bus_math::SpiBus {
    number: 1,
    base: 0x4001_3000,
    file: crate::bus_math::SpiFile::U5,
    enable_reg: 0x4602_0CA4,
    enable_bit: 12,
    mux_reg: 0x4602_0CE0,
    mux_shift: 20,
    mux_value: 0b10,
    kernel_hz: 16_000_000,
    pads: &[
        crate::bus_math::Pad { line: 5, alternate: 5, role: crate::bus_math::PadRole::SpiClock },
        crate::bus_math::Pad { line: 6, alternate: 5, role: crate::bus_math::PadRole::SpiIn },
        crate::bus_math::Pad { line: 7, alternate: 5, role: crate::bus_math::PadRole::SpiOut },
    ],
    selects: &[3 * 16 + 14, 3 * 16 + 15],
    select_count: 2,
    ahead: 8,
}];
const _: () = assert!(populated(16 + 8) && populated(16 + 9) && populated(5) && populated(6) && populated(7) && populated(3 * 16 + 14) && populated(3 * 16 + 15), "a bus pad is not a pin of the package");

/// The second I/O supply of this chip (RM0456): the power controller's
/// clock enable RCC_AHB3ENR (11.8.32, offset 0x094) with PWREN at bit 2;
/// the monitoring control register PWR_SVMCR (10.10.5: the power controller
/// at 0x4602_0800, offset 0x10) with the supply monitor's enable IO2VMEN at
/// bit 25 and the valid bit IO2SV at bit 29; the monitoring status register
/// PWR_SVMSR (10.10.16, offset 0x3C) with VDDIO2RDY at bit 25. The supply
/// feeds PG2 to PG15 (10.4.5), the lines 98 to 111: the board's third LED
/// (PG2) and the connector's D1 and D0 (PG7, PG8) are among them. The
/// board feeds the supply from the main one through a solder bridge that is
/// closed as built (UM2861, the solder bridge table: SB30).
pub const IO2_SUPPLY: crate::stm32u5_supply_math::SupplyFacts = crate::stm32u5_supply_math::SupplyFacts { clock_reg: 0x4602_0C94, clock_bit: 2, control_reg: 0x4602_0810, monitor_bit: 25, valid_bit: 29, status_reg: 0x4602_083C, ready_bit: 25, first_line: 98, last_line: 111 };
const _: () = assert!(LED2_LINE == 6 * 16 + 2 && crate::stm32u5_supply_math::feeds(&IO2_SUPPLY, LED2_LINE) && crate::stm32u5_supply_math::feeds(&IO2_SUPPLY, 6 * 16 + 7) && crate::stm32u5_supply_math::feeds(&IO2_SUPPLY, 6 * 16 + 8) && !crate::stm32u5_supply_math::feeds(&IO2_SUPPLY, 6 * 16 + 1) && !crate::stm32u5_supply_math::feeds(&IO2_SUPPLY, 7 * 16), "the second I/O supply's line range is not PG2 to PG15, or the third LED is not on PG2");

/// The PWM tables of this row: the six PWM pins of the board's Arduino-style
/// connector (UM2861 Table 15), each a channel of one of the chip's timers at
/// the alternate function the chip's datasheet states (DS13543 Table 28) --
/// D3 on PE13 (TIM1 channel 3, AF1), D5 on PE11 (TIM1 channel 2, AF1), D6 on
/// PE9 (TIM1 channel 1, AF1), D9 on PD15 (TIM4 channel 4, AF2), D10 on PD14
/// (TIM4 channel 3, AF2) and D11 on PA7 (TIM3 channel 2, AF2). D3, D5 and D6
/// are three channels of TIM1 and share its period; D9 and D10 share TIM4's.
/// D10 and D9 are also the two chip selects of SPI1 above and D11 its output:
/// a line serves one function at a time, and the first claim holds it. The
/// blocks (RM0456: Table 6 for the bases, the non-secure alias; 11.8.33
/// RCC_APB1ENR1 at 0x4602_0C9C with TIM3EN at bit 1 and TIM4EN at bit 2;
/// 11.8.35 RCC_APB2ENR at 0x4602_0CA4 with TIM1EN at bit 11): TIM1 is an
/// advanced timer, TIM3 and TIM4 general-purpose ones with four channels.
/// Their counters are 32 bits wide; the tables give every timer a 16-bit
/// reach. A timer's clock follows its bus by the manual's rule (11.4.15: the
/// bus clock at a bus prescaler of 1, twice it otherwise): every bus
/// prescaler is 1 at both clock configurations, so each timer runs at the
/// system clock. The chip has no selector for the timers' clock. A line's
/// number is its port and pin (PA0 is 0, sixteen a port), written as a plain
/// number in the table and tied to the port and pin below.
pub const PWM_BLOCKS: &[crate::stm32_tim_math::TimBlock] = &[
    crate::stm32_tim_math::TimBlock { number: 1, base: 0x4001_2C00, enable_reg: 0x4602_0CA4, enable_bit: 11, apb: 2, advanced: true, channels: 4 },
    crate::stm32_tim_math::TimBlock { number: 3, base: 0x4000_0400, enable_reg: 0x4602_0C9C, enable_bit: 1, apb: 1, advanced: false, channels: 4 },
    crate::stm32_tim_math::TimBlock { number: 4, base: 0x4000_0800, enable_reg: 0x4602_0C9C, enable_bit: 2, apb: 1, advanced: false, channels: 4 },
];
pub const PWM_TIMERS: &[crate::pwm_math::PwmTimer] = &crate::stm32_tim_math::timers::<{ PWM_BLOCKS.len() }>(PWM_BLOCKS, crate::board::SYSCLK_HZ, crate::board::SYSCLK_HZ, crate::board::SYSCLK_HZ);
pub const PWM_LINES: &[crate::pwm_math::PwmLine] = &[
    crate::pwm_math::PwmLine { line: 77, timer: 0, channel: 2, alternate: 1 },
    crate::pwm_math::PwmLine { line: 75, timer: 0, channel: 1, alternate: 1 },
    crate::pwm_math::PwmLine { line: 73, timer: 0, channel: 0, alternate: 1 },
    crate::pwm_math::PwmLine { line: 63, timer: 2, channel: 3, alternate: 2 },
    crate::pwm_math::PwmLine { line: 62, timer: 2, channel: 2, alternate: 2 },
    crate::pwm_math::PwmLine { line: 7, timer: 1, channel: 1, alternate: 2 },
];
/// The timer prescaler selector's register and bit, on a chip that has one.
pub const PWM_SELECTOR: Option<(usize, u32)> = None;
const _: () = assert!(PWM_LINES[0].line as u32 == 4 * 16 + 13 && PWM_LINES[1].line as u32 == 4 * 16 + 11 && PWM_LINES[2].line as u32 == 4 * 16 + 9 && PWM_LINES[3].line as u32 == 3 * 16 + 15 && PWM_LINES[4].line as u32 == 3 * 16 + 14 && PWM_LINES[5].line as u32 == 7, "a PWM line's number is not its port and pin");
const _: () = assert!(populated(4 * 16 + 13) && populated(4 * 16 + 11) && populated(4 * 16 + 9) && populated(3 * 16 + 15) && populated(3 * 16 + 14) && populated(7) && populated(4 * 16 + 13), "a PWM line, or the flight check's input line, is not a pin of the package");
/// The lines of the flight check's jumper wire: the PWM line whose edges the wire
/// carries, and the plain input that counts them -- D5 (PE11) to D3 (PE13), itself a PWM line of this table that the check holds as a plain input for the wire.
#[allow(dead_code)]
pub const PWM_WIRE_OUT: u32 = 4 * 16 + 11;
#[allow(dead_code)]
pub const PWM_WIRE_IN: u32 = 4 * 16 + 13;

/// The ADC tables of this row: the six analog pins of the board's
/// Arduino-style connector (UM2861 Table 15) and the chip's internal
/// reference as a seventh channel, a channel without a pin, all on the
/// 14-bit converter ADC1 (DS13543 Table 27: PA3 channel 8, PA2 channel 7,
/// PC3 channel 4, PB0 channel 15, PC1 channel 2, PC0 channel 1; RM0456
/// Table 306: the internal reference on channel 0). 14 bits; the reference
/// voltage the board's 3.3 V supply (UM2861 7.4.4: VDD at 3.3 V by default,
/// the reference pin fed from the analog supply through SB2); the
/// converter's clock the 16 MHz internal oscillator as its kernel clock,
/// which both clock configurations of the row keep running (inside DS13543
/// Table 97's 5 to 55 MHz); a conversion 831 cycles, the longest sampling
/// time of 814 and the conversion's 17 at 14 bits (RM0456 33.4.13). A
/// line's number is its port and pin (PA0 is 0, sixteen a port), written as
/// a plain number in the table and tied to the port and pin below.
pub const ADC_BLOCKS: &[crate::adc_math::AdcBlock] = &[crate::adc_math::AdcBlock { number: 1, clock_hz: ADC_KERNEL_HZ, bits: 14, reference_mv: 3300, conversion_cycles: 831 }];
pub const ADC_CHANNELS: &[crate::adc_math::AdcChannel] = &[
    crate::adc_math::AdcChannel { block: 0, channel: 8, line: 3 },
    crate::adc_math::AdcChannel { block: 0, channel: 7, line: 2 },
    crate::adc_math::AdcChannel { block: 0, channel: 4, line: 35 },
    crate::adc_math::AdcChannel { block: 0, channel: 15, line: 16 },
    crate::adc_math::AdcChannel { block: 0, channel: 2, line: 33 },
    crate::adc_math::AdcChannel { block: 0, channel: 1, line: 32 },
    crate::adc_math::AdcChannel { block: 0, channel: 0, line: crate::adc_math::NO_LINE },
];
/// The converter's kernel clock: the 16 MHz internal oscillator.
pub const ADC_KERNEL_HZ: u32 = 16_000_000;
/// The converter for the backend (RM0456 Table 6: the 14-bit converters'
/// window at 0x4202_8000, ADC1 at its start; 33.7: the common registers at
/// 0x300 of it; 11.8.30 RCC_AHB2ENR1 at 0x4602_0C8C with ADC12EN at bit 10;
/// 11.8.48 RCC_CCIPR3 at 0x4602_0CE8 with the converters' kernel clock
/// selector ADCDACSEL at bits 14:12, 100 the 16 MHz internal oscillator).
pub const ADC_FACTS: crate::stm32_adc_math::u5::Facts = crate::stm32_adc_math::u5::Facts {
    unit: crate::stm32_adc_math::AdcUnit { base: 0x4202_8000, enable_reg: 0x4602_0C8C, enable_bit: 10 },
    common: 0x4202_8300,
    clock_sel_reg: 0x4602_0CE8,
    clock_sel_shift: 12,
    clock_sel_mask: 0b111,
    clock_sel_code: 0b100,
};
/// The analog supply (RM0456 10.4.4: the analog blocks are isolated after a
/// reset until the supply is declared valid): the second I/O supply's
/// facts shape with the analog supply's bits in the same registers --
/// PWR_SVMCR at 0x4602_0810 with the first analog monitor's enable AVM1EN
/// at bit 26 and the valid bit ASV at bit 30 (10.10.5), PWR_SVMSR at
/// 0x4602_083C with VDDA1RDY at bit 26 (10.10.16). The supply feeds no
/// line of its own: the line range names the no-line value alone.
pub const ANALOG_SUPPLY: crate::stm32u5_supply_math::SupplyFacts = crate::stm32u5_supply_math::SupplyFacts { clock_reg: 0x4602_0C94, clock_bit: 2, control_reg: 0x4602_0810, monitor_bit: 26, valid_bit: 30, status_reg: 0x4602_083C, ready_bit: 26, first_line: 65535, last_line: 65535 };
const _: () = assert!(ADC_CHANNELS[0].line as u32 == 3 && ADC_CHANNELS[1].line as u32 == 2 && ADC_CHANNELS[2].line as u32 == 2 * 16 + 3 && ADC_CHANNELS[3].line as u32 == 16 && ADC_CHANNELS[4].line as u32 == 2 * 16 + 1 && ADC_CHANNELS[5].line as u32 == 2 * 16, "an analog channel's line is not its port and pin");
const _: () = assert!(populated(3) && populated(2) && populated(2 * 16 + 3) && populated(16) && populated(2 * 16 + 1) && populated(2 * 16), "an analog pin is not a pin of the package");
/// The flight check's own list of the channels it may claim, by table
/// position: every channel of the table.
#[allow(dead_code)]
pub const ADC_FLIGHT: &[usize] = &[0, 1, 2, 3, 4, 5, 6];
const _: () = assert!(crate::stm32_adc_math::positions_fit(ADC_FLIGHT, ADC_CHANNELS.len()), "the flight check's list names a position outside the channel table, or one twice");
/// The flight check's jumper wire: the PWM line that holds it, D6 (PE9),
/// and the table position of the channel that reads it, A0.
#[allow(dead_code)]
pub const ADC_WIRE_OUT: u32 = 4 * 16 + 9;
#[allow(dead_code)]
pub const ADC_WIRE_IN: usize = 0;

/// The RAM ranges of this row (the hosted runtime learns them
/// through the `rhu_ram_count` and `rhu_ram_range` functions): the physical
/// range as the board's facts file states it in the SDK (`boards/<row>.json`,
/// its `ram` array; the two kept equal by the hosted-runtime gate's facts
/// tie), range 0 the internal SRAM the kernel's map calls DATA. Range 0's
/// `base` and `bytes` are the map's DATA region, whole: the arena window
/// the runtime is handed lies inside it, bracketed by the map.
/// 2496 KB of SRAM (SRAM1, 2, 3 and 5, one contiguous window; SRAM4 at 0x28000000 is not offered).
pub const RAM_RANGES: &[crate::ram::RamFacts] =
    &[crate::ram::RamFacts { name: "SRAM", base: 0x2000_0000, bytes: 0x27_0000, external: false, cached: false, exec: true }];

/// The console's channel facts of this row (the SDK's `channels` member):
/// the two streams' default channel codes and the row's legal set
/// (`crate::console`: 1 off, 2 memory, 3 probe, 0x10 + n uart n).
pub const CHANNELS: crate::console::ChannelFacts = crate::console::ChannelFacts { log: crate::console::CH_PROBE, console: crate::console::CH_PROBE, legal: &[crate::console::CH_PROBE, crate::console::CH_MEMORY, crate::console::CH_OFF] };
/// The pre-defined UART rows of this row (the SDK's `uart` member), by the
/// chip's own block number with their pins: one port, the low-power UART
/// LPUART1 on the Arduino-style connector's D1 (PG7, transmit) and D0
/// (PG8, receive), as the board is built (UM2861 Table 11), where a seated
/// shield puts a click's serial lines. Both pins are fed by the second I/O
/// supply, which the boot declares valid. The kernel's UART API reads the
/// table. The console keeps its own block, USART1 on the lines to the
/// board's probe: it is not a row here and is never a program's port. A
/// line's number is its port and pin, written as a plain number in the
/// table and tied to the port and pin below.
pub const UART_ROWS: &[crate::console::UartRow] = &[crate::console::UartRow { block: 1, tx: 103, rx: 104 }];
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
/// position, for the backend. LPUART1 at 0x4600_2400 (RM0456 Rev 7 page
/// 145), a low-power UART with its prescaler; its bit 6 in the clock
/// controller's three APB3 registers (the reset register at 0x080, the
/// clock enable at 0x0A8, the clock while the core sleeps at 0x0D0; pages
/// 552, 568 and 585); its kernel clock selector at bits 2:0 of the third
/// independent clock configuration register at 0x0E8, the code 010 the
/// 16 MHz internal oscillator (page 597); FIFOs, 8 deep each (Table 686);
/// both pins at alternate function 8 (DS13543 Table 29); the interrupt
/// line 66 (RM0456 Table 186). The block's kind -- a USART or a low-power
/// UART -- is stated by its row's `law` field.
pub const UART_BLOCKS: &[crate::stm32_usart_math::Block] = &[crate::stm32_usart_math::Block { base: 0x4600_2400, law: crate::stm32_usart_math::Law::LowPower, enable_reg: 0x4602_0CA8, reset_reg: 0x4602_0C80, sleep_reg: 0x4602_0CD0, bus_bit: 6, select_reg: 0x4602_0CE8, select_shift: 0, select_mask: 0x7, select_code: 0x2, fifo: true, alternate: 8, irq: 66 }];
/// The two rings of a port, in bytes. This row offers no port as a console
/// wire, the use the larger transmit ring of the other serial rows is
/// sized for: a program's write copies what fits and its blocking write
/// waits for room, so a smaller ring loses nothing. The receive ring holds
/// 89 ms of a 115200 Bd wire.
pub const UART_RX_BYTES: usize = 1024;
pub const UART_TX_BYTES: usize = 1024;
const _: () = assert!(UART_ROWS.len() == 1 && UART_ROWS[0].block == 1 && UART_ROWS[0].tx as u32 == 6 * 16 + 7 && UART_ROWS[0].rx as u32 == 6 * 16 + 8, "the serial port's row is not LPUART1 on D1 (PG7) and D0 (PG8)");
const _: () = assert!(populated(6 * 16 + 7) && populated(6 * 16 + 8) && crate::stm32_usart_math::pair_sound(UART_ROWS[0].tx as u32, UART_ROWS[0].rx as u32), "a serial line is not a pin of the package, or the port's two lines are one");
const _: () = assert!(crate::stm32_usart_math::lines_free(&BOARD_CLAIM_BITS, UART_ROWS[0].tx as u32, UART_ROWS[0].rx as u32), "a serial line is among the board's own claims");
const _: () = assert!(crate::stm32u5_supply_math::feeds(&IO2_SUPPLY, UART_ROWS[0].tx as u32) && crate::stm32u5_supply_math::feeds(&IO2_SUPPLY, UART_ROWS[0].rx as u32), "the serial lines are not fed by the second I/O supply the boot declares valid");
