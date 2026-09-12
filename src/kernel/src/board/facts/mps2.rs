//! The emulator rows' GPIO facts: a line
//! space of six VIRTUAL lines served by the virtual backend -- three
//! latches (0 to 2), line 2 also the output of a loopback pair whose input
//! is line 3, line 4 claimed by the board as the virtual console's line so
//! the board-claim refusal is gated here too, and line 5 UNPOPULATED (a
//! line inside the space with no pad) so the unpopulated refusal is gated
//! here. Line 1 is marked input-only (a virtual button) so the input-only
//! refusal is gated here as well. No register, no generated description: these facts are
//! the kernel's own, stated here.

use crate::claims;

/// The line space: the highest line plus one.
pub const LINE_COUNT: usize = 6;
pub const BOARD_CLAIMS: &[u16] = &[4];
pub const BOARD_CLAIM_BITS: [u32; claims::BANKS] = claims::fold_board(BOARD_CLAIMS);
pub const BANNER: &str = "6 virtual lines (5 populated), 1 claimed by the board (the virtual console)";

pub const LOOP_OUT: u32 = 2;
pub const LOOP_IN: u32 = 3;

/// The api flavor's check lines: a free latch, the loop pair, the board's
/// line, the unpopulated line.
#[allow(dead_code)]
pub const CHECK_FREE: u32 = 0;
#[allow(dead_code)]
pub const CHECK_INPUT_ONLY: Option<u32> = Some(1);
#[allow(dead_code)]
pub const CHECK_OUT: Option<u32> = Some(LOOP_OUT);
#[allow(dead_code)]
pub const CHECK_IN: u32 = LOOP_IN;
#[allow(dead_code)]
pub const CHECK_BOARD: u32 = 4;
/// The output loops back to the input on this row: a toggle IS the edge.
#[allow(dead_code)]
pub const LOOPBACK: bool = true;
#[allow(dead_code)]
pub const CHECK_UNPOPULATED: Option<u32> = Some(5);

pub const fn input_only(line: u32) -> bool {
    line == 1
}

pub const fn populated(line: u32) -> bool {
    line != 5
}
