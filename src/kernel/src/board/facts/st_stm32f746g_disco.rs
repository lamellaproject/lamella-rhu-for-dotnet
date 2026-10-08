//! The STM32F746G-DISCO's GPIO facts: the board's generated description,
//! vendored beside this file at its public commit (`vendored/`), read here
//! into the members the kernel's doors and drivers use. The block's base,
//! the ports' clock enable and the console's pins are DERIVED from the
//! vendored constants and tied to the manual's values below; the port
//! count, the line count and the debug probe's pins are RHU's own (the
//! description does not carry them), cited. The description emits no LED
//! and no button row for this board yet, so the two are RHU's own, cited
//! to the board's schematic and its user manual; they move to the
//! description's rows when it emits them.

#[allow(dead_code)]
#[path = "vendored/st_stm32f746g_disco_bindings.rs"]
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
/// Both tied to the manual's values. The receive pin sits on port B, the
/// block after the first (asserted).
pub const GPIO_BASE: usize = bindings::VCP_TX_MODER_REG as usize;
pub const RCC_GPIO_EN: *mut u32 = bindings::VCP_TX_PORT_RCC_EN_REG as *mut u32;
const _: () = assert!(GPIO_BASE == 0x4002_0000, "the description's block base is not RM0385's GPIOA");
const _: () = assert!(bindings::VCP_TX_PORT_RCC_EN_REG == 0x4002_3830, "the description's clock enable is not RM0385's RCC_AHB1ENR");
const _: () = assert!(bindings::VCP_RX_MODER_REG as usize == GPIO_BASE + 0x400, "the console's receive pin is not on port B");

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
/// PB7); the debug probe's PA13 and PA14 (UM1907).
pub const CONSOLE_TX: u32 = line(bindings::VCP_TX_MODER_REG, pin_of_moder_mask(bindings::VCP_TX_MODER_MASK));
pub const CONSOLE_RX: u32 = line(bindings::VCP_RX_MODER_REG, pin_of_moder_mask(bindings::VCP_RX_MODER_MASK));
const _: () = assert!(CONSOLE_TX == 9 && CONSOLE_RX == 23, "the console's pins are not PA9 and PB7");
pub const BOARD_CLAIMS: &[u16] = &[CONSOLE_TX as u16, CONSOLE_RX as u16, 13, 14];
pub const BOARD_CLAIM_BITS: [u32; claims::BANKS] = claims::fold_board(BOARD_CLAIMS);
pub const BANNER: &str = "176 lines, 4 claimed by the board (the console PA9/PB7, the debug pins PA13/PA14)";

/// RHU's own rows until the description emits them: the user LED on PI1,
/// lit when high; the user button on PI11, high when pressed, the board's
/// own pull-down holding its level, so none is applied. The description's
/// one device row (the audio codec's bus address) is not read here.
#[allow(dead_code)]
pub const LED0_LINE: u32 = 8 * 16 + 1;
#[allow(dead_code)]
pub const LED0_ACTIVE_LOW: bool = false;
#[allow(dead_code)]
pub const BUTTON0_LINE: u32 = 8 * 16 + 11;
#[allow(dead_code)]
pub const BUTTON0_ACTIVE_LOW: bool = false;
#[allow(dead_code)]
pub const BUTTON0_PULL: u32 = gpio_math::PULL_NONE;
const _: () = assert!(bindings::DEVICE_COUNT == 1, "the description's device count is not the one row it carries");

/// The api flavor's check lines: LED0 as the output claimed at its initial
/// level and toggled; the Arduino connector's A0 line from the description
/// (PA0, a header pin with nothing behind it) as the input the
/// software-triggered edges are counted on and as the free line of the
/// claim and refusal checks; the button as the input-only line; the
/// console's transmit pin as the board-claimed line.
#[allow(dead_code)]
pub const CHECK_FREE: u32 = line(bindings::CONNECTOR_ARDUINO_A0_PORT_BASE, bindings::CONNECTOR_ARDUINO_A0_PIN);
const _: () = assert!(CHECK_FREE == 0, "the connector's A0 line is not PA0");
#[allow(dead_code)]
pub const CHECK_OUT: Option<u32> = Some(LED0_LINE);
#[allow(dead_code)]
pub const CHECK_IN: u32 = CHECK_FREE;
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
/// n): the STM32F746NG in its 216-ball package, from the part's datasheet
/// (DS10916 Table 10, the pin and ball definitions, the package's column;
/// the same set as the package's ballout figure). Ports A to J are whole;
/// port K has pins 0 to 7.
/// A program may claim any pin listed here, routed to a header or not; a
/// line inside the space that names a pin the package does not have is
/// refused.
pub const PRESENT: [u16; PORT_COUNT as usize] = [0xFFFF, 0xFFFF, 0xFFFF, 0xFFFF, 0xFFFF, 0xFFFF, 0xFFFF, 0xFFFF, 0xFFFF, 0xFFFF, 0x00FF];

pub const fn populated(line: u32) -> bool {
    (line as usize) < LINE_COUNT && PRESENT[line as usize / 16] & (1 << (line % 16)) != 0
}

/// A line inside the space that the package does not bond out (PK8).
#[allow(dead_code)]
pub const CHECK_UNPOPULATED: Option<u32> = Some(10 * 16 + 8);
const _: () = assert!(!populated(10 * 16 + 8) && populated(CONSOLE_TX) && populated(CONSOLE_RX) && populated(13) && populated(14) && populated(BUTTON0_LINE), "the present list contradicts the check line, the board's own pins or a named signal");

/// The classic external-interrupt arm's blocks, which the family GPIO
/// driver reads from the board's facts (RM0385): the EXTI block at
/// 0x4001_3C00 (11.9), the first SYSCFG port-selection register at
/// 0x4001_3808 (7.2.3), the SYSCFG clock enable RCC_APB2ENR (5.3.14) with
/// its bit 14.
pub const EXTI_BASE: usize = 0x4001_3C00;
pub const SYSCFG_EXTICR1: usize = 0x4001_3808;
pub const RCC_SYSCFG_EN: *mut u32 = 0x4002_3844 as *mut u32;
pub const RCC_SYSCFG_EN_BIT: u32 = 1 << 14;

/// The bus tables of this row: empty in this generation, so the bus count
/// doors answer 0, the enumeration doors refuse every index, and every
/// other bus door answers RHU_ENODEV. The tables are shaped for a board's
/// every bus (one entry per block, named by the chip's own block number).
pub const I2C_BUSES: &[crate::bus_math::I2cBus] = &[];
pub const SPI_BUSES: &[crate::bus_math::SpiBus] = &[];

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
