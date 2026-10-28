//! The Adafruit Feather RP2350 with HSTX and PSRAM's GPIO facts: the kernel's own,
//! read from the board's schematic, the maker's pinout page and the chip's
//! datasheet -- this board has no vendored board description. The console's
//! lines, the line count of the RP2350A, the user LED and the addressable
//! LED are stated here, cited.
//!
//! The RP2350 datasheet: 30 user GPIOs on the RP2350A (chapter 1, Table 1;
//! section 9.1). The board's schematic (the maker's sheet "Adafruit Feather
//! RP2350"): the red LED on GPIO 7 through a series resistor to ground, lit
//! when high; the addressable LED's data line on GPIO 21; the BOOT button
//! on the flash's chip-select line, not on a GPIO, so a press at run time
//! is no pin's input; the STEMMA QT connector's I2C pair on GPIO 2 (data)
//! and 3 (clock), the chip's second I2C block, with the board's own 10 k
//! pull-ups; SCK, MOSI and MISO on GPIO 22, 23 and 20, the chip's first
//! SPI block, with no chip-select pin of the board's; UART0 on GPIO 0 (TX)
//! and 1 (RX); the converter's four inputs A0 to A3 on GPIO 26 to 29, all
//! on the header and none a supply sense; the HSTX connector on GPIO 12 to
//! 19 beside GPIO 10 and 11 and the I2C pair. The PSRAM's chip select
//! on GPIO 8 (the header's PCS pin), which the board pulls up with a 10 k
//! resistor of its own; a program leaves that pin alone. The header's labels
//! beside the chip's numbers: TX 0, RX 1, SDA 2, SCL 3, D12 4, D5 5, D6 6,
//! D13 7 (the LED), D9 9, D10 10, D11 11, MISO 20, SCK 22, MOSI 23, D24 24,
//! D25 25, A0 26, A1 27, A2 28, A3 29, and PCS 8 (the PSRAM's select).

use crate::bus_math::{Pad, PadRole};
use crate::claims;

pub const LINE_COUNT: usize = 30;

/// The console's GPIO 0 and 1.
pub const UART0_TX: u32 = 0;
#[allow(dead_code)] // the pair is the facts' UART row's; this row reads the transmit side alone (CHECK_BOARD)
pub const UART0_RX: u32 = 1;
/// No line is claimed by the board by default -- the memory console takes
/// no pin; a UART the kernel holds as its wire claims its two lines at init.
pub const BOARD_CLAIMS: &[u16] = &[];
pub const BOARD_CLAIM_BITS: [u32; claims::BANKS] = claims::fold_board(BOARD_CLAIMS);
pub const BANNER: &str = "30 lines, 0 claimed by the board";

/// The board's rows: the red LED on GPIO 7 through the SIO, lit when high;
/// the addressable LED's data line on GPIO 21 (a plain output to a
/// program; the kernel drives no pattern on it). The board has no user
/// button on a GPIO (the BOOT button sits on the flash's chip-select
/// line), so the acceptance run's button is a header line: D5 (GPIO 5),
/// pulled up by the kernel and pressed by a jumper touched to ground --
/// active low.
#[allow(dead_code)] // the silicon leg's check and the flights read the rows
pub const LED_LINE: u32 = 7;
#[allow(dead_code)] // the silicon leg's check and the flights read the rows
pub const LED_ACTIVE_LOW: bool = false;
#[allow(dead_code)] // the board page's row
pub const NEOPIXEL_LINE: u32 = 21;
#[allow(dead_code)] // the flight image reads the row
pub const LED0_LINE: u32 = LED_LINE;
#[allow(dead_code)] // the flight image reads the row
pub const LED0_ACTIVE_LOW: bool = LED_ACTIVE_LOW;
#[allow(dead_code)] // the flight image reads the row
pub const BUTTON0_LINE: u32 = 5;
#[allow(dead_code)] // the flight image reads the row
pub const BUTTON0_PULL: u32 = crate::gpio_math::PULL_UP;
#[allow(dead_code)] // the flight image reads the row
pub const BUTTON0_ACTIVE_LOW: bool = true;

/// The api flavor's check lines: the LED as the output claimed at its
/// initial level and toggled; GPIO 6 (the header's D6, a line no bus of
/// this row takes) as the input the software-triggered edges are counted
/// on and as the free line of the claim and refusal checks; the console's
/// GPIO 0 as the board-claimed line.
#[allow(dead_code)] // the api flavor's checks
pub const CHECK_FREE: u32 = 6;
#[allow(dead_code)] // the api flavor's checks
pub const CHECK_OUT: Option<u32> = Some(LED_LINE);
#[allow(dead_code)] // the api flavor's checks
pub const CHECK_IN: u32 = 6;
#[allow(dead_code)] // the api flavor's checks
pub const CHECK_BOARD: u32 = UART0_TX;
/// No loopback on a board: the software trigger makes the edge beside the toggle.
#[allow(dead_code)] // the api flavor's checks
pub const LOOPBACK: bool = false;

/// No line of this board is an input only: the BOOT button is not on a GPIO.
pub const fn input_only(_line: u32) -> bool {
    false
}

#[allow(dead_code)] // the api flavor's checks
pub const CHECK_INPUT_ONLY: Option<u32> = None;

/// No pin present-list is stated for this board, so no unpopulated line can
/// be stated: the whole space is treated as populated.
#[allow(dead_code)] // the api flavor's checks
pub const CHECK_UNPOPULATED: Option<u32> = None;

pub const fn populated(_line: u32) -> bool {
    true
}

/// The bus tables of this row, each a bus named by the chip's own block
/// number: I2C bus 1 (the chip's second I2C block) on GPIO 2 (data) and 3
/// (clock) -- the STEMMA QT connector's pair and the header's SDA and SCL,
/// which the board pulls up with 10 k resistors of its own; SPI bus 0 (the
/// chip's first SPI block) on GPIO 22 (clock), 23 (output) and 20 (input)
/// -- the header's SCK, MOSI and MISO -- with NO chip select of the
/// board's: the block's own frame line is the addressable LED's GPIO 21,
/// so a program drives a select pin of its own choosing (the settings'
/// chip-select line -1) low before a transfer and high after it. The I2C
/// block runs on the system clock at this posture: 100 kHz, 400 kHz and 1
/// MHz (no 1 MHz in the crystal posture); the SPI block on the crystal at
/// both, 184 Hz to 6 MHz. The chip's first I2C block and second SPI block
/// have no default pins on this board and are no bus of this row.
pub const I2C_PADS: [Pad; 2] = [
    Pad { line: 2, alternate: crate::rp2350_bus_math::I2C_FUNCTION, role: PadRole::I2cData },
    Pad { line: 3, alternate: crate::rp2350_bus_math::I2C_FUNCTION, role: PadRole::I2cClock },
];
pub const SPI_PADS: [Pad; 3] = [
    Pad { line: 22, alternate: crate::rp2350_bus_math::SPI_FUNCTION, role: PadRole::SpiClock },
    Pad { line: 23, alternate: crate::rp2350_bus_math::SPI_FUNCTION, role: PadRole::SpiOut },
    Pad { line: 20, alternate: crate::rp2350_bus_math::SPI_FUNCTION, role: PadRole::SpiIn },
];
pub const SPI_SELECTS: [u16; 0] = [];
pub const I2C_BUSES: &[crate::bus_math::I2cBus] = &[crate::rp2350_bus_math::i2c_bus(1, super::rp2350::SYSCLK_HZ, &I2C_PADS)];
pub const SPI_BUSES: &[crate::bus_math::SpiBus] = &[crate::rp2350_bus_math::spi_bus(0, &SPI_PADS, &SPI_SELECTS, 0)];
const _: () = assert!(crate::rp2350_bus_math::rows_sound(I2C_BUSES, SPI_BUSES), "a bus row's pad does not carry its role on the chip's function table");

/// The PWM facts of this row: every line of the package carries a timer
/// output -- the RP2350 datasheet, section 12.5.2, Table 1130 (the mapping
/// of PWM channels to GPIO pins): lines 0 to 15 carry slice 0's A output,
/// slice 0's B, slice 1's A and so on to slice 7's B, and lines 16 to 29
/// repeat that order, so every slice output reaches TWO lines, n and n + 16,
/// and carries the same signal on both when both select it; the claims
/// table keeps the channel exclusive, so the second line's PWM claim
/// answers RHU_EBUSY while the first holds the channel. The
/// timers are the eight slices a line of this package reaches, numbered by
/// the slice's index, counting the system clock at this posture (Table
/// 1133); the prescaler is the divider's integer part, 1 to 255, and the
/// period's reach 65535 counts (sections 12.5.2.1 and 12.5.2.2); the pad's
/// function 4 selects the output (Table 645). The rule's rows are
/// spot-checked against the table below.
pub const PWM_TIMERS: &[crate::pwm_math::PwmTimer] = &super::rp2350_pwm::timers::<8>();
pub const PWM_LINES: &[crate::pwm_math::PwmLine] = &super::rp2350_pwm::lines::<LINE_COUNT>();
const _: () = assert!(
    PWM_LINES[0].timer == 0
        && PWM_LINES[0].channel == 0
        && PWM_LINES[2].timer == 1
        && PWM_LINES[2].channel == 0
        && PWM_LINES[15].timer == 7
        && PWM_LINES[15].channel == 1
        && PWM_LINES[16].timer == 0
        && PWM_LINES[16].channel == 0
        && PWM_LINES[29].timer == 6
        && PWM_LINES[29].channel == 1,
    "the slice mapping does not match the datasheet's table"
);

/// The ADC facts of this row: the chip's one converter (block 0; the
/// datasheet numbers it nothing), 12 bits, 96 cycles a conversion (section
/// 12.4.3), clocked from the crystal at 12 MHz on BOTH clock postures -- the
/// datasheet asks 48 MHz of the converter's clock for its rated 500
/// thousand samples a second (the RP2350 datasheet's section 12.4.3), but the block's registers
/// do not answer while that clock runs faster than the system clock (a
/// fact of the part, measured, not in the text) and a crystal-posture image's
/// system clock is 12 MHz, so the crystal drives the converter on both
/// postures, 8 microseconds a sample -- its
/// reference the board's ADC supply -- the 3.3 V rail through a filter
/// (the board's schematic). The channels (Table 1118): 0 to 3 on GPIO 26
/// to 29, the header's A0 to A3, every one a plain input (no line of this
/// board senses its supply), and 4 the temperature sensor (the RP2350
/// datasheet's section 12.4.6), no line. No line of this board twins a
/// channel's pad.
pub const ADC_BLOCKS: &[crate::adc_math::AdcBlock] =
    &[crate::adc_math::AdcBlock { number: 0, clock_hz: super::rp2350_adc::CLK_ADC_HZ, bits: 12, reference_mv: 3300, conversion_cycles: 96 }];
pub const ADC_CHANNELS: &[crate::adc_math::AdcChannel] = &[
    crate::adc_math::AdcChannel { block: 0, channel: 0, line: 26 },
    crate::adc_math::AdcChannel { block: 0, channel: 1, line: 27 },
    crate::adc_math::AdcChannel { block: 0, channel: 2, line: 28 },
    crate::adc_math::AdcChannel { block: 0, channel: 3, line: 29 },
    crate::adc_math::AdcChannel { block: 0, channel: 4, line: crate::adc_math::NO_LINE },
];
/// The channels' twins: a digital pad on a channel's net, parked with it.
pub const ADC_TWINS: &[(u16, u16)] = &[];
/// The channel table's position of the temperature sensor.
#[allow(dead_code)] // the api flavor's checks
pub const ADC_SENSOR: usize = 4;
/// The api flavor's check rows: the pin channel it claims and reads (a
/// header line with nothing on it); the PWM line it claims (GPIO 25, the
/// header's D25, slice 4's B output: a line no bus of this row takes), the
/// line sharing that slice output (GPIO 9, D9) and the slice's other
/// output (GPIO 24, D24); no resistor-capacitor loop between a PWM line
/// and a channel is wired on this row.
#[allow(dead_code)] // the api flavor's checks
pub const ADC_CHECK: usize = 0;
#[allow(dead_code)] // the api flavor's checks
pub const PWM_CHECK_LINE: u32 = 25;
#[allow(dead_code)] // the api flavor's checks
pub const PWM_CHECK_TWIN: u32 = 9;
#[allow(dead_code)] // the api flavor's checks
pub const PWM_CHECK_SIBLING: u32 = 24;
#[allow(dead_code)] // the api flavor's checks
pub const ADC_LOOP: Option<(usize, usize)> = None;

/// The RAM ranges of this row (the hosted runtime learns them
/// through the `rhu_ram_count` and `rhu_ram_range` doors): the physical
/// range as the board's facts file states it in the SDK (`boards/<row>.json`,
/// its `ram` array; the two kept equal by the hosted-runtime gate's facts
/// tie), range 0 the internal SRAM the kernel's map calls DATA. Range 0's
/// `base` and `bytes` are the map's DATA region, whole: the arena window
/// the runtime is handed lies inside it, bracketed by the map.
/// 520 KB of SRAM (SRAM0 to SRAM9, one contiguous window).
/// Range 1 is the board's 8 MB PSRAM part behind the flash interface's
/// second chip select: external, reached through the chip's XIP
/// cache, shown at the second select's cached window. The part's chip
/// select is on GPIO 8 (the board's schematic: the net PSRAM_CS on the
/// chip's GPIO8 pad, pulled up by the board).
pub const RAM_RANGES: &[crate::ram::RamFacts] = &[
    crate::ram::RamFacts { name: "SRAM", base: 0x2000_0000, bytes: 0x8_2000, external: false, cached: false, exec: true },
    crate::ram::RamFacts { name: "PSRAM", base: 0x1100_0000, bytes: 0x80_0000, external: true, cached: true, exec: true },
];
/// The GPIO line the board wires the PSRAM's chip select to.
#[allow(dead_code)] // the crystal posture builds without the window
pub const PSRAM_SELECT_LINE: u8 = 8;

/// The console's channel facts of this row (the SDK's
/// `channels` member, kept equal by the parity leg): the two streams'
/// default channel codes and the row's legal set (`crate::console`: 1 off,
/// 2 memory, 3 probe, 0x10 + n uart n).
pub const CHANNELS: crate::console::ChannelFacts = crate::console::ChannelFacts { log: crate::console::CH_MEMORY, console: crate::console::CH_MEMORY, legal: &[crate::console::CH_MEMORY, crate::console::CH_OFF, crate::console::CH_UART0, crate::console::CH_UART1] };
/// The pre-defined UART rows of this row (the SDK's `uart` member), by the
/// chip's own block number with their pins (none on a row whose UARTs have
/// no pins); a program's serial doors and the console's wire read them.
/// UART1 has NO default pair on this row (no maker document names one) -- the block stays in the
/// table and becomes claimable when the project's pin words name a legal pair.
pub const UART_ROWS: &[crate::console::UartRow] = &[crate::console::UartRow { block: 0, tx: 0, rx: 1 }, crate::console::UartRow { block: 1, tx: crate::console::NO_PIN, rx: crate::console::NO_PIN }];
/// The gpio banner's line when the kernel holds a UART of this row as its
/// console wire, by block (empty on a row with no UART row).
pub const BANNER_UART: &[&str] = &["30 lines, 2 claimed by the board (the console UART0 on GPIO 0/1)", "30 lines, 2 claimed by the board (the console UART1 on the project's pins)"];
/// The UART rows' names for the boot banner's console clause, by block.
#[allow(dead_code)] // the Pico rows' banner clause reads it
pub const UART_NAMES: &[&str] = &["UART0 (GPIO 0 TX, GPIO 1 RX)", "UART1 (no default pins; the project's pin words name a legal pair)"];

/// The chip's legal pin sets per block for a project's pin override (the SDK's `uartPairs`
/// member): the UART function's TX and RX
/// pins (the function table's F2 column, the RP2350A's GPIO 0 to 29; the
/// UART AUX function, select 11, is not supported).
pub const UART_PAIRS: &[crate::console::UartPairs] = &[
    crate::console::UartPairs { block: 0, tx: &[0, 12, 16, 28], rx: &[1, 13, 17, 29] },
    crate::console::UartPairs { block: 1, tx: &[4, 8, 20, 24], rx: &[5, 9, 21, 25] },
];
/// The gpio banner's line when the kernel holds BOTH UART rows as wires.
pub const BANNER_UART_BOTH: &str = "30 lines, 4 claimed by the board (the console UART0 on GPIO 0/1, UART1 on the project's pins)";
