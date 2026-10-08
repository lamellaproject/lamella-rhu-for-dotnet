//! The STM32F769I-DISCO's GPIO facts: the board's generated description,
//! vendored beside this file at its tagged version (`vendored/`), read here
//! into the members the kernel's doors and drivers use. The block's base,
//! the ports' clock enable and the console's pins are DERIVED from the
//! vendored constants and tied to the manuals' values below; the port
//! count, the line count and the debug probe's pins are RHU's own (the
//! description does not carry them), cited. The named signals -- the two
//! user LEDs and the user button -- are the description's rows, never
//! typed here.

#[allow(dead_code)]
#[path = "vendored/st_stm32f769i_disco_bindings.rs"]
mod bindings;

use crate::claims;
use crate::gpio_math;

/// Eleven ports of sixteen lines: PA0 = 0 ... PK15 = 175.
#[allow(dead_code)]
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
pub const BOARD_CLAIMS: &[u16] = &[CONSOLE_TX as u16, CONSOLE_RX as u16, 13, 14];
pub const BOARD_CLAIM_BITS: [u32; claims::BANKS] = claims::fold_board(BOARD_CLAIMS);
pub const BANNER: &str = "176 lines, 4 claimed by the board (the console PA9/PA10, the debug pins PA13/PA14)";

/// The description's rows: two user LEDs, lit when their line is low, and
/// the user button, high when pressed; the description emits no pull for
/// the button (the board's own circuit holds its level), so none is applied.
#[allow(dead_code)]
pub const LED0_LINE: u32 = line(bindings::LED0_PORT_BASE, bindings::LED0_PIN);
#[allow(dead_code)]
pub const LED0_ACTIVE_LOW: bool = bindings::LED0_ACTIVE_LOW != 0;
#[allow(dead_code)]
pub const LED1_LINE: u32 = line(bindings::LED1_PORT_BASE, bindings::LED1_PIN);
#[allow(dead_code)]
pub const LED1_ACTIVE_LOW: bool = bindings::LED1_ACTIVE_LOW != 0;
#[allow(dead_code)]
pub const BUTTON0_LINE: u32 = line(bindings::BUTTON0_PORT_BASE, bindings::BUTTON0_PIN);
#[allow(dead_code)]
pub const BUTTON0_ACTIVE_LOW: bool = bindings::BUTTON0_ACTIVE_LOW != 0;
#[allow(dead_code)]
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
#[allow(dead_code)]
pub const CHECK_FREE: u32 = LED1_LINE;
#[allow(dead_code)]
pub const CHECK_OUT: Option<u32> = Some(LED0_LINE);
#[allow(dead_code)]
pub const CHECK_IN: u32 = LED1_LINE;
#[allow(dead_code)]
pub const CHECK_BOARD: u32 = CONSOLE_TX;
/// No loopback on a board: the software trigger makes the edge beside the toggle.
#[allow(dead_code)]
pub const LOOPBACK: bool = false;

/// The button's line is an input only: an output claim on it is refused.
pub const fn input_only(line: u32) -> bool {
    line == BUTTON0_LINE
}

#[allow(dead_code)]
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
#[allow(dead_code)]
pub const CHECK_UNPOPULATED: Option<u32> = Some(9 * 16 + 6);
const _: () = assert!(!populated(9 * 16 + 6) && populated(CONSOLE_TX) && populated(CONSOLE_RX) && populated(13) && populated(14) && populated(LED0_LINE) && populated(LED1_LINE) && populated(BUTTON0_LINE), "the present list contradicts the check line, the board's own pins or a named signal");

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
/// connector's D10 and D9 (PA11, PH6) the two chip selects. RM0410: Table 1
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
    selects: &[11, 7 * 16 + 6],
    select_count: 2,
    ahead: 2,
}];
const _: () = assert!(populated(16 + 8) && populated(16 + 9) && populated(12) && populated(16 + 14) && populated(16 + 15) && populated(11) && populated(7 * 16 + 6), "a bus pad is not a pin of the package");
const _: () = assert!(SPI_BUSES[0].kernel_hz == crate::board::SYSCLK_HZ || SPI_BUSES[0].kernel_hz * 4 == crate::board::SYSCLK_HZ, "SPI2's clock is not this clock configuration's APB1 clock");

/// The PWM and ADC tables of this row: empty in this generation, so the
/// count doors answer 0, the enumeration doors refuse every index, and
/// every other door of the two families answers RHU_ENODEV. The tables
/// are shaped for a board's every timer output and every converter
/// channel (one entry per line and per channel, the blocks named by the
/// chip's own numbers).
pub const PWM_TIMERS: &[crate::pwm_math::PwmTimer] = &[];
pub const PWM_LINES: &[crate::pwm_math::PwmLine] = &[];
pub const ADC_BLOCKS: &[crate::adc_math::AdcBlock] = &[];
pub const ADC_CHANNELS: &[crate::adc_math::AdcChannel] = &[];
