//! The Pimoroni Pico Plus 2 W's GPIO facts: the kernel's own, read from
//! the board's schematic and the chip's datasheet -- this board has no
//! vendored board description. The console's lines, the line count of the
//! RP2350B and the user switch are stated here, cited; the board's LED
//! hangs on the wireless module (whose lines the wifi driver claims at its
//! own open), so no LED row exists.

use crate::claims;

pub const LINE_COUNT: usize = 48;

/// The console's GPIO 0 and 1.
pub const CONSOLE_TX: u32 = 0;
pub const CONSOLE_RX: u32 = 1;
pub const BOARD_CLAIMS: &[u16] = &[CONSOLE_TX as u16, CONSOLE_RX as u16];
pub const BOARD_CLAIM_BITS: [u32; claims::BANKS] = claims::fold_board(BOARD_CLAIMS);
pub const BANNER: &str = "48 lines, 2 claimed by the board (the console GPIO 0/1)";

/// The board's row: the user switch on GPIO 45, low when pressed, the
/// board's own pull-up holding its level, so none is applied. No LED row:
/// the LED is the wireless module's.
#[allow(dead_code)]
pub const BUTTON0_LINE: u32 = 45;
#[allow(dead_code)]
pub const BUTTON0_PULL: u32 = crate::gpio_math::PULL_NONE;
#[allow(dead_code)]
pub const BUTTON0_ACTIVE_LOW: bool = true;

/// The api flavor's check lines: no output toggled (no LED line); GPIO 26
/// (a header line) as the input the software-triggered edges are counted
/// on and as the free line of the claim and refusal checks; the switch as
/// the input-only line; the console's GPIO 0 as the board-claimed line.
#[allow(dead_code)]
pub const CHECK_FREE: u32 = 26;
#[allow(dead_code)]
pub const CHECK_OUT: Option<u32> = None;
#[allow(dead_code)]
pub const CHECK_IN: u32 = 26;
#[allow(dead_code)]
pub const CHECK_BOARD: u32 = CONSOLE_TX;
/// No loopback on a board: the software trigger makes the edge beside the toggle.
#[allow(dead_code)]
pub const LOOPBACK: bool = false;

/// The switch's line is an input only: an output claim on it is refused.
pub const fn input_only(line: u32) -> bool {
    line == BUTTON0_LINE
}

#[allow(dead_code)]
pub const CHECK_INPUT_ONLY: Option<u32> = Some(BUTTON0_LINE);

/// No pin present-list is stated for this board, so no unpopulated line can
/// be stated: the whole space is treated as populated.
#[allow(dead_code)]
pub const CHECK_UNPOPULATED: Option<u32> = None;

pub const fn populated(_line: u32) -> bool {
    true
}

/// The bus tables of this row: empty in this generation, so the bus count
/// doors answer 0, the enumeration doors refuse every index, and every
/// other bus door answers RHU_ENODEV. The tables are shaped for a board's
/// every bus (one entry per block, named by the chip's own block number).
pub const I2C_BUSES: &[crate::bus_math::I2cBus] = &[];
pub const SPI_BUSES: &[crate::bus_math::SpiBus] = &[];
