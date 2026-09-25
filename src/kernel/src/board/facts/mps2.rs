//! The emulator rows' GPIO facts: a line
//! space of six VIRTUAL lines served by the virtual backend -- three
//! latches (0 to 2), line 2 also the output of a loopback pair whose input
//! is line 3, line 4 claimed by the board as the virtual console's line so
//! the board-claim refusal is gated here too, and line 5 UNPOPULATED (a
//! line inside the space with no pad) so the unpopulated refusal is gated
//! here. Line 1 is marked input-only (a virtual button) so the input-only
//! refusal is gated here as well. No register, no generated description: these facts are
//! the kernel's own, stated here.

use crate::bus_math;
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

/// The bus facts of the emulator rows: one I2C bus and one SPI bus, each a
/// register-level model of an ST block behind the virtual backend -- the
/// Cortex-M7 twin models the STM32F7 family's I2C1 and SPI2 (SPI2's clock
/// the APB1 bus clock, 16 MHz at the internal-oscillator posture), the
/// Cortex-M33 twin the STM32U5 family's I2C1 and SPI1 (both blocks on the
/// 16 MHz internal oscillator). Each bus carries its NUMBER, the chip's
/// own block number, the name the doors take (I2C1 = 1 on both rows; the
/// F7's SPI2 = 2, the U5's SPI1 = 1). The tables are shaped for a board's
/// every bus (one entry per block: the number, the base, the clock enable,
/// the kernel-clock selection, the pads, the selects); the models have no pins, so the pad
/// lists are empty and the selects are the models' own two. The bound is
/// half the block's receive queue: two bytes on the F7 file, eight on the U5.
#[cfg(feature = "board-mps2-an500")]
pub const I2C_BUSES: &[bus_math::I2cBus] = &[bus_math::I2cBus {
    number: 1,
    base: 0x4000_5400,
    enable_reg: 0x4002_3840,
    enable_bit: 21,
    mux_reg: 0x4002_3890,
    mux_shift: 16,
    mux_value: 0b10,
    kernel_hz: 16_000_000,
    pads: &[],
}];
#[cfg(feature = "board-mps2-an505")]
pub const I2C_BUSES: &[bus_math::I2cBus] = &[bus_math::I2cBus {
    number: 1,
    base: 0x4000_5400,
    enable_reg: 0x4602_0C9C,
    enable_bit: 21,
    mux_reg: 0x4602_0CE0,
    mux_shift: 10,
    mux_value: 0b10,
    kernel_hz: 16_000_000,
    pads: &[],
}];
#[cfg(feature = "board-mps2-an500")]
pub const SPI_BUSES: &[bus_math::SpiBus] = &[bus_math::SpiBus {
    number: 2,
    base: 0x4000_3800,
    file: bus_math::SpiFile::F7,
    enable_reg: 0x4002_3840,
    enable_bit: 14,
    mux_reg: 0,
    mux_shift: 0,
    mux_value: 0,
    kernel_hz: 16_000_000,
    pads: &[],
    selects: &[],
    select_count: 2,
    ahead: 2,
}];
#[cfg(feature = "board-mps2-an505")]
pub const SPI_BUSES: &[bus_math::SpiBus] = &[bus_math::SpiBus {
    number: 1,
    base: 0x4001_3000,
    file: bus_math::SpiFile::U5,
    enable_reg: 0x4602_0CA4,
    enable_bit: 12,
    mux_reg: 0x4602_0CE0,
    mux_shift: 20,
    mux_value: 0b10,
    kernel_hz: 16_000_000,
    pads: &[],
    selects: &[],
    select_count: 2,
    ahead: 8,
}];
