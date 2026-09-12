//! The Pimoroni Pico Plus 2's GPIO facts (the Pico Plus 2 W is the same
//! board with a wireless module fitted): the kernel's own, stating only what
//! the kernel's board file already states (the console's lines, the line
//! count of the RP2350B), cited to the manuals, and no named signal -- this
//! board has no vendored board description.

use crate::claims;

pub const LINE_COUNT: usize = 48;

/// The console's GPIO 0 and 1.
pub const BOARD_CLAIMS: &[u16] = &[0, 1];
pub const BOARD_CLAIM_BITS: [u32; claims::BANKS] = claims::fold_board(BOARD_CLAIMS);
pub const BANNER: &str = "48 lines, 2 claimed by the board (the console GPIO 0/1)";

/// The api flavor's check lines: GPIO 26 (a header line) as the input the
/// software-triggered edge is counted on, no output toggled (no named
/// signal), the console's GPIO 0 as the board-claimed line.
#[allow(dead_code)]
pub const CHECK_FREE: u32 = 26;
#[allow(dead_code)]
pub const CHECK_OUT: Option<u32> = None;
#[allow(dead_code)]
pub const CHECK_IN: u32 = 26;
#[allow(dead_code)]
pub const CHECK_BOARD: u32 = 0;
/// No loopback on a board: the software trigger makes the edge beside the toggle.
#[allow(dead_code)]
pub const LOOPBACK: bool = false;

pub const fn input_only(_line: u32) -> bool {
    false
}

/// No named signal, so no input-only line and no unpopulated line can be
/// stated: the block's whole space is treated as populated.
#[allow(dead_code)]
pub const CHECK_INPUT_ONLY: Option<u32> = None;
#[allow(dead_code)]
pub const CHECK_UNPOPULATED: Option<u32> = None;

pub const fn populated(_line: u32) -> bool {
    true
}
