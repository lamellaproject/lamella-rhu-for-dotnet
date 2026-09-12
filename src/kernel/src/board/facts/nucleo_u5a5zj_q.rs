//! The NUCLEO-U5A5ZJ-Q's GPIO facts: the board's generated description,
//! vendored beside this file at its tagged version (`vendored/`), read here
//! into the members the kernel's doors and drivers use. The block's base,
//! the ports' clock enable and the console's pins are DERIVED from the
//! vendored constants and tied to the manuals' values below; the port
//! count, the line count and the debug probe's pins are RHU's own (the
//! description does not carry them), cited. The named signals -- the three
//! user LEDs and the user button with its pull -- are the description's
//! rows, never typed here.

#[allow(dead_code)]
#[path = "vendored/nucleo_u5a5zj_q_bindings.rs"]
mod bindings;

use crate::claims;
use crate::gpio_math;

/// Ten ports of sixteen lines: PA0 = 0 ... PJ15 = 159.
#[allow(dead_code)]
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
#[allow(dead_code)]
pub const BUTTON0_ACTIVE_LOW: bool = bindings::BUTTON0_ACTIVE_LOW != 0;
#[allow(dead_code)]
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

/// The description carries no pin present-list at this tag, so no
/// unpopulated line can be stated: the block's whole space is populated.
#[allow(dead_code)]
pub const CHECK_UNPOPULATED: Option<u32> = None;

pub const fn populated(_line: u32) -> bool {
    true
}
