//! The NUCLEO-L4R5ZI's GPIO facts: the kernel's own, read from the part's
//! reference manual and the board's user manual -- no board description is
//! vendored for this board yet. The block, the ports' clock enable, the
//! classic external-interrupt blocks, the console's lines, the user LEDs
//! and the user button are stated here, cited; they move to a description's
//! rows when one is emitted.

use crate::claims;
use crate::gpio_math;

/// Nine ports of sixteen lines: PA0 = 0 ... PI15 = 143.
#[allow(dead_code)]
pub const PORT_COUNT: u32 = 9;
pub const LINE_COUNT: usize = 144;

/// The block's base and the ports' clock enable (RCC_AHB2ENR).
pub const GPIO_BASE: usize = 0x4800_0000;
pub const RCC_GPIO_EN: *mut u32 = 0x4002_104C as *mut u32;

/// The classic external-interrupt arm's blocks, which the family GPIO
/// driver reads from the board's facts: the EXTI block, the first of the
/// four SYSCFG port-selection registers, and the SYSCFG clock enable
/// (RCC_APB2ENR) with its bit.
pub const EXTI_BASE: usize = 0x4001_0400;
pub const SYSCFG_EXTICR1: usize = 0x4001_0008;
pub const RCC_SYSCFG_EN: *mut u32 = 0x4002_1060 as *mut u32;
pub const RCC_SYSCFG_EN_BIT: u32 = 1 << 0;

/// The console's transmit and receive pins (PG7 and PG8); the debug
/// probe's PA13 and PA14.
pub const CONSOLE_TX: u32 = 6 * 16 + 7;
pub const CONSOLE_RX: u32 = 6 * 16 + 8;
pub const BOARD_CLAIMS: &[u16] = &[CONSOLE_TX as u16, CONSOLE_RX as u16, 13, 14];
pub const BOARD_CLAIM_BITS: [u32; claims::BANKS] = claims::fold_board(BOARD_CLAIMS);
pub const BANNER: &str = "144 lines, 4 claimed by the board (the console PG7/PG8, the debug pins PA13/PA14)";

/// The board's rows: the green user LED on PC7, lit when high (the two
/// other user LEDs on PB7 and PB14, the same way, for the flights); the
/// user button on PC13, high when pressed -- the part's own pull-down is
/// applied beside whatever the board carries, so the line never floats.
#[allow(dead_code)]
pub const LED0_LINE: u32 = 2 * 16 + 7;
#[allow(dead_code)]
pub const LED0_ACTIVE_LOW: bool = false;
#[allow(dead_code)]
pub const LED1_LINE: u32 = 16 + 7;
#[allow(dead_code)]
pub const LED2_LINE: u32 = 16 + 14;
#[allow(dead_code)]
pub const BUTTON0_LINE: u32 = 2 * 16 + 13;
#[allow(dead_code)]
pub const BUTTON0_ACTIVE_LOW: bool = false;
/// The board's own resistor holds the released level low (the MB1312
/// schematic, sheet 3: 220 k from the button's line to ground; the button
/// joins the line to the supply); the pad's pull-down sits beside it.
#[allow(dead_code)]
pub const BUTTON0_PULL: u32 = gpio_math::PULL_DOWN;

/// The api flavor's check lines: LED0 as the output claimed at its initial
/// level and toggled; the Arduino connector's A0 line (PA3, a header pin
/// with nothing behind it) as the input the software-triggered edges are
/// counted on and as the free line of the claim and refusal checks; the
/// button as the input-only line; the console's transmit pin as the
/// board-claimed line.
#[allow(dead_code)]
pub const CHECK_FREE: u32 = 3;
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
/// n): the STM32L4R5ZI in its 144-pin package, from the part's datasheet
/// (DS12023 Table 15, the pin definitions, the package's column; the same
/// set as the package's pinout figure). Ports A to G are whole; port H has
/// pins 0, 1 and 3; port I has none.
/// A program may claim any pin listed here, routed to a header or not; a
/// line inside the space that names a pin the package does not have is
/// refused.
pub const PRESENT: [u16; PORT_COUNT as usize] = [0xFFFF, 0xFFFF, 0xFFFF, 0xFFFF, 0xFFFF, 0xFFFF, 0xFFFF, 0x000B, 0x0000];

pub const fn populated(line: u32) -> bool {
    (line as usize) < LINE_COUNT && PRESENT[line as usize / 16] & (1 << (line % 16)) != 0
}

/// A line inside the space that the package does not bond out (PH2).
#[allow(dead_code)]
pub const CHECK_UNPOPULATED: Option<u32> = Some(7 * 16 + 2);
const _: () = assert!(!populated(7 * 16 + 2) && populated(13) && populated(14) && populated(BUTTON0_LINE) && populated(LED2_LINE), "the present list contradicts the check line, the board's own pins or a named signal");

/// The bus tables of this row: empty in this generation, so the bus count
/// doors answer 0, the enumeration doors refuse every index, and every
/// other bus door answers RHU_ENODEV. The tables are shaped for a board's
/// every bus (one entry per block, named by the chip's own block number).
pub const I2C_BUSES: &[crate::bus_math::I2cBus] = &[];
pub const SPI_BUSES: &[crate::bus_math::SpiBus] = &[];
