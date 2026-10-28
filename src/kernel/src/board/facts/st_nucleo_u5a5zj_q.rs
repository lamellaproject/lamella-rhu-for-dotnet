//! The NUCLEO-U5A5ZJ-Q's GPIO facts: the board's generated description,
//! vendored beside this file at its tagged version (`vendored/`), read here
//! into the members the kernel's doors and drivers use. The block's base,
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

/// The PWM and ADC tables of this row: empty, so the
/// count doors answer 0, the enumeration doors refuse every index, and
/// every other door of the two families answers RHU_ENODEV. The tables
/// are shaped for a board's every timer output and every converter
/// channel (one entry per line and per channel, the blocks named by the
/// chip's own numbers).
pub const PWM_TIMERS: &[crate::pwm_math::PwmTimer] = &[];
pub const PWM_LINES: &[crate::pwm_math::PwmLine] = &[];
pub const ADC_BLOCKS: &[crate::adc_math::AdcBlock] = &[];
pub const ADC_CHANNELS: &[crate::adc_math::AdcChannel] = &[];

/// The RAM ranges of this row (the hosted runtime learns them
/// through the `rhu_ram_count` and `rhu_ram_range` doors): the physical
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
/// chip's own block number with their pins (none on a row whose UARTs have
/// no pins); a program's serial doors and the console's wire read them.
pub const UART_ROWS: &[crate::console::UartRow] = &[];
/// The gpio banner's line when the kernel holds a UART of this row as its
/// console wire, by block (empty on a row with no UART row).
pub const BANNER_UART: &[&str] = &[];
/// The UART rows' names for the boot banner's console clause, by block.
#[allow(dead_code)] // the Pico rows' banner clause reads it
pub const UART_NAMES: &[&str] = &[];
/// No pin override on this row (the UART rows have no pins, or there are
/// none): the SDK's `uartPairs` member is empty.
#[allow(dead_code)] // no row of this board reads it: the UART doors answer ENODEV here
pub const UART_PAIRS: &[crate::console::UartPairs] = &[];
/// No both-held banner line on this row.
pub const BANNER_UART_BOTH: &str = "";
