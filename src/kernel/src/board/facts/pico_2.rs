//! The Raspberry Pi Pico 2's GPIO facts: the board's generated description,
//! vendored beside this file at its tagged version (`vendored/`), read here
//! into the members the kernel's doors and drivers use. The console's lines
//! are DERIVED from the vendored constants (the UART's pin control
//! registers in the IO bank) and the LED's line from its device row, both
//! tied to the chip's block bases; the line count is RHU's own (the
//! description does not carry it), cited. The named signal -- the LED -- is
//! the description's row, never typed here; the board has no user button
//! (BOOTSEL is not a GPIO row), so no line is input only.

#[allow(dead_code)]
#[path = "vendored/pico2_bindings.rs"]
mod bindings;

use crate::claims;

pub const LINE_COUNT: usize = 30;

/// The IO bank's and the SIO's block bases, as the chip module states them.
const IO_BANK0: u32 = 0x4002_8000;
const SIO: u32 = 0xD000_0000;

/// A line number from a GPIO's IO bank CTRL register address (0x04 + 8n).
const fn line_of_ctrl(ctrl: u32) -> u32 {
    (ctrl - IO_BANK0 - 0x04) / 8
}

/// The console's transmit and receive lines from the description (GPIO 0
/// and 1, the debug probe's serial pair).
pub const CONSOLE_TX: u32 = line_of_ctrl(bindings::UART0_IO_TX_CTRL);
pub const CONSOLE_RX: u32 = line_of_ctrl(bindings::UART0_IO_RX_CTRL);
const _: () = assert!(CONSOLE_TX == 0 && CONSOLE_RX == 1, "the console's lines are not GPIO 0 and 1");
pub const BOARD_CLAIMS: &[u16] = &[CONSOLE_TX as u16, CONSOLE_RX as u16];
pub const BOARD_CLAIM_BITS: [u32; claims::BANKS] = claims::fold_board(BOARD_CLAIMS);
pub const BANNER: &str = "30 lines, 2 claimed by the board (the console GPIO 0/1)";

/// The description's row: the LED on GPIO 25 through the SIO, lit when high.
#[allow(dead_code)]
pub const LED_LINE: u32 = bindings::LED_PIN;
#[allow(dead_code)]
pub const LED_ACTIVE_LOW: bool = bindings::LED_ACTIVE_LOW != 0;
const _: () = assert!(bindings::LED_PORT_BASE == SIO, "the LED's port base is not the SIO block");
const _: () = assert!(bindings::LED_MASK == 1 << bindings::LED_PIN, "the LED's mask is not its pin's bit");
const _: () = assert!(bindings::DEVICE_COUNT == 1, "the description's device count is not the one row read here");

/// RHU's own row for what the description lacks: the acceptance flight's
/// button on a header line (GP22, header pin 29, beside a ground pin), pulled
/// up by the kernel and pressed by a jumper touched to ground -- active low;
/// the flight's LED row is the board's LED.
#[allow(dead_code)]
pub const BUTTON0_LINE: u32 = 22;
#[allow(dead_code)]
pub const BUTTON0_PULL: u32 = crate::gpio_math::PULL_UP;
#[allow(dead_code)]
pub const BUTTON0_ACTIVE_LOW: bool = true;
#[allow(dead_code)]
pub const LED0_LINE: u32 = LED_LINE;
#[allow(dead_code)]
pub const LED0_ACTIVE_LOW: bool = LED_ACTIVE_LOW;

/// The api flavor's check lines: the LED as the output claimed at its
/// initial level and toggled; GPIO 26 (a header line, ADC0) as the input the
/// software-triggered edges are counted on and as the free line of the
/// claim and refusal checks; the console's GPIO 0 as the board-claimed line.
#[allow(dead_code)]
pub const CHECK_FREE: u32 = 26;
#[allow(dead_code)]
pub const CHECK_OUT: Option<u32> = Some(LED_LINE);
#[allow(dead_code)]
pub const CHECK_IN: u32 = 26;
#[allow(dead_code)]
pub const CHECK_BOARD: u32 = CONSOLE_TX;
/// No loopback on a board: the software trigger makes the edge beside the toggle.
#[allow(dead_code)]
pub const LOOPBACK: bool = false;

pub const fn input_only(_line: u32) -> bool {
    false
}

#[allow(dead_code)]
pub const CHECK_INPUT_ONLY: Option<u32> = None;

/// The description carries no pin present-list at this tag, so no
/// unpopulated line can be stated: the whole space is populated.
#[allow(dead_code)]
pub const CHECK_UNPOPULATED: Option<u32> = None;

pub const fn populated(_line: u32) -> bool {
    true
}
