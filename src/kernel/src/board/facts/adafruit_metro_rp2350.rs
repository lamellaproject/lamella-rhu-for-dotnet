//! The Adafruit Metro RP2350's GPIO facts, shared by the board's two
//! variants -- the board without PSRAM (the maker's product 6003) and the
//! board with 8 MB of PSRAM (product 6267): one wiring, the PSRAM part and
//! its chip select the difference -- the kernel's own, read from the
//! maker's pinout page and the maker's schematic (one sheet, "Adafruit
//! Metro RP2350", 12/18/2024) and the chip's datasheet; this board has no
//! vendored board description. Where the pinout page and the schematic
//! differ, the schematic is the source of record. The console's lines, the
//! line count of the RP2350B, the LED, the button and the board's own lines
//! are stated here, cited.
//!
//! The RP2350 datasheet: 48 user GPIOs on the RP2350B (section 9.1; chapter
//! 1, Table 1). The schematic (the chip's pins, the headers, the parts):
//! UART0 on GPIO 0 (transmit) and 1 (receive), reaching the Arduino
//! header's TX (1) and RX (0) positions through a two-pole slide switch
//! (SW4) that routes the pair straight or crossed at the header -- the
//! chip's pins are the same in either position; the header's D2 to D11 on
//! GPIO 2 to 11; the HSTX connector's eight lines on GPIO 12 to 19; I2C0 on
//! GPIO 20 (data) and 21 (clock), the header's SDA and SCL beside AREF, the
//! STEMMA QT connector and the HSTX connector, with the board's own 10 k
//! pull-ups; the header's 12 and 13 positions on GPIO 22 and 23 (the
//! maker's D22 and D23); the red D23 LED on GPIO 23 through 2.2 k to
//! ground, lit when high; the BOOT button on GPIO 24, a switch to ground
//! (the same press pulls the flash's chip select low through a diode and
//! 1 k, the boot ROM's way into its USB loader at reset); the addressable
//! LED's data line on GPIO 25; GPIO 26 and 27 on the HSTX connector alone;
//! the ICSP header's SPI on the chip's second SPI block, GPIO 28 (data in,
//! MISO), 30 (clock, SCK) and 31 (data out, MOSI), with no chip select of
//! the board's; the USB host port's 5 V switch on GPIO 29 (high to power,
//! through a transistor pair) and its data lines on GPIO 32 (D+) and 33
//! (D-); the microSD socket on the chip's first SPI block -- GPIO 34
//! (clock), 35 (data out to the card), 36 (data in from the card), 37 and
//! 38 (the card's unused data lines 1 and 2), 39 (the card's select, its
//! DAT3 line) and 40 (the card-detect switch, pulled up on the board
//! through 10 k); the header's A0 to A5 on GPIO 41 to 46, the converter's
//! channels 1 to 6 on the chip's own pads; the PSRAM's chip select on GPIO
//! 47, pulled up on the board through 10 k (the part fitted on the PSRAM
//! variant alone; the plain variant's pad carries no part). The 16 MB
//! flash (a W25Q128 part) on the chip's QSPI pins, its select pulled up
//! through 10 k; the 12 MHz crystal; the RESET button on the chip's RUN
//! pin; AREF tied to the 3.3 V rail; USB-C for the device port; the barrel
//! jack through a buck regulator to 5 V and a linear regulator to 3.3 V,
//! an on-off switch on the jack's path; no supply sense reaches the
//! converter. The header's labels beside the chip's numbers: TX 0, RX 1,
//! D2 to D11 2 to 11, 12 22, 13 23, SDA 20, SCL 21, A0 to A5 41 to 46;
//! the ICSP header MISO 28, SCK 30, MOSI 31; every line of the chip on a
//! net of the board.

use crate::bus_math::{Pad, PadRole};
use crate::claims;

pub const LINE_COUNT: usize = 48;

/// The console's GPIO 0 and 1.
#[allow(dead_code)] // the pair is the facts' UART row's (UART_ROWS below names the lines by number)
pub const UART0_TX: u32 = 0;
#[allow(dead_code)] // the pair is the facts' UART row's
pub const UART0_RX: u32 = 1;
/// The board's claims: the microSD socket's seven lines (GPIO 34 to 40),
/// the kernel's card host's alone, so a program's open of any of them is
/// refused. The memory console takes no pin; a UART the kernel holds as its
/// wire claims its two lines at init.
pub const BOARD_CLAIMS: &[u16] = &[SD_CLOCK_LINE as u16, SD_OUT_LINE as u16, SD_IN_LINE as u16, SD_DATA1_LINE as u16, SD_DATA2_LINE as u16, SD_SELECT_LINE as u16, SD_DETECT_LINE as u16];
pub const BOARD_CLAIM_BITS: [u32; claims::BANKS] = claims::fold_board(BOARD_CLAIMS);
pub const BANNER: &str = "48 lines, 7 claimed by the board (the microSD socket on GPIO 34 to 40)";

/// The board's rows: the red D23 LED on GPIO 23 through the SIO, lit when
/// high (the schematic: through 2.2 k to ground; the line is the header's
/// 13 position as well, as an UNO's pin 13 is its LED's); the BOOT button
/// on GPIO 24, low when pressed (the schematic: a switch to ground with no
/// pull-up of the board's on the line, so the kernel pulls it up; the same
/// press pulls the flash's chip select low through a diode and 1 k, which
/// the boot ROM reads at reset as the way into its USB loader -- at run
/// time the chip drives the select and the press is a plain input).
#[allow(dead_code)] // the silicon leg's check and the flights read the rows
pub const LED_LINE: u32 = 23;
#[allow(dead_code)] // the silicon leg's check and the flights read the rows
pub const LED_ACTIVE_LOW: bool = false;
#[allow(dead_code)] // the board page's row
pub const NEOPIXEL_LINE: u32 = 25;
#[allow(dead_code)] // the flight image reads the row
pub const LED0_LINE: u32 = LED_LINE;
#[allow(dead_code)] // the flight image reads the row
pub const LED0_ACTIVE_LOW: bool = LED_ACTIVE_LOW;
#[allow(dead_code)] // the flight image reads the row
pub const BUTTON0_LINE: u32 = 24;
#[allow(dead_code)] // the flight image reads the row
pub const BUTTON0_PULL: u32 = crate::gpio_math::PULL_UP;
#[allow(dead_code)] // the flight image reads the row
pub const BUTTON0_ACTIVE_LOW: bool = true;

/// The USB host port's lines (the schematic: a second USB connector's
/// data pair on GPIO 32 and 33, its 5 V switched by GPIO 29 through a
/// transistor pair, high to power), named here and driven by nothing of
/// this kernel.
#[allow(dead_code)] // the board page's rows
pub const USB_HOST_POWER_LINE: u32 = 29;
#[allow(dead_code)] // the board page's rows
pub const USB_HOST_DP_LINE: u32 = 32;
#[allow(dead_code)] // the board page's rows
pub const USB_HOST_DM_LINE: u32 = 33;

/// The microSD socket's seven lines on the chip's first SPI block (the
/// schematic: the socket's clock, data in, data out, select, the card's
/// two unused data lines and the card-detect switch), named here for the
/// kernel's card host (board/rp2350_sdspi.rs over sdspi.rs), which alone
/// drives them; all seven are the board's claims above. The card-detect
/// line is an input alone: the board pulls it up through 10 k and the
/// socket's switch sits between the line and ground. The two unused data
/// lines are the card's DAT1 and DAT2, which the SPI mode leaves alone.
#[allow(dead_code)] // the board page's rows; the card host's facts tie to these
pub const SD_CLOCK_LINE: u32 = 34;
#[allow(dead_code)] // the board page's rows; the card host's facts tie to these
pub const SD_OUT_LINE: u32 = 35;
#[allow(dead_code)] // the board page's rows; the card host's facts tie to these
pub const SD_IN_LINE: u32 = 36;
#[allow(dead_code)] // the board page's rows
pub const SD_DATA1_LINE: u32 = 37;
#[allow(dead_code)] // the board page's rows
pub const SD_DATA2_LINE: u32 = 38;
#[allow(dead_code)] // the board page's rows; the card host's facts tie to these
pub const SD_SELECT_LINE: u32 = 39;
#[allow(dead_code)] // the board page's rows; the card host's facts tie to these
pub const SD_DETECT_LINE: u32 = 40;
#[allow(dead_code)] // the board page's rows
pub const SD_LINES: [u32; 7] = [SD_CLOCK_LINE, SD_OUT_LINE, SD_IN_LINE, SD_DATA1_LINE, SD_DATA2_LINE, SD_SELECT_LINE, SD_DETECT_LINE];
const _: () = assert!(SD_DETECT_LINE < LINE_COUNT as u32 && SD_SELECT_LINE < LINE_COUNT as u32, "a socket line lies outside the RP2350B's line space");
/// The socket's bus row for the kernel's card host: the chip's first SPI
/// block on the clock, data-out and data-in lines (GPIO 34 is the block's
/// clock, 35 its output, 36 its input on the chip's function table), with
/// the card's select as the one select line, a plain output the host
/// drives. The row is the host's alone: not a bus of the kernel's bus API.
pub const SD_PADS: [Pad; 3] = [
    Pad { line: SD_CLOCK_LINE as u16, alternate: crate::rp2350_bus_math::SPI_FUNCTION, role: PadRole::SpiClock },
    Pad { line: SD_OUT_LINE as u16, alternate: crate::rp2350_bus_math::SPI_FUNCTION, role: PadRole::SpiOut },
    Pad { line: SD_IN_LINE as u16, alternate: crate::rp2350_bus_math::SPI_FUNCTION, role: PadRole::SpiIn },
];
pub const SD_SELECTS: [u16; 1] = [SD_SELECT_LINE as u16];
pub const SD_BUS: crate::bus_math::SpiBus = crate::rp2350_bus_math::spi_bus(0, &SD_PADS, &SD_SELECTS, 1);
/// The block's name for the boot banner's sd line.
pub const SD_BUS_NAME: &str = "SPI0";
const _: () = assert!(crate::rp2350_bus_math::rows_sound(&[], &[SD_BUS]), "the socket's pads do not carry their roles on the chip's function table");
/// The level the card-detect line reads with a card seated: LOW -- read on
/// the board with a card in the socket (the switch closes to ground). With
/// the socket empty the line rises only under the chip's pull-up, which the
/// host enables at its attach: read on the board through the probe with the
/// card out, low with no pull of the chip's and high with it.
pub const SD_DETECT_CARD_LEVEL: bool = false;
/// The chip's pull on the card-detect line at the host's attach: the
/// pull-up, by the reading above.
pub const SD_DETECT_PULL: u32 = crate::gpio_math::PULL_UP;

/// The api flavor's check lines: the LED as the output claimed at its
/// initial level and toggled; GPIO 4 (the header's D4, a line no bus of
/// this row takes) as the input the software-triggered edges are counted
/// on and as the free line of the claim and refusal checks; the button as
/// the input-only line; the microSD socket's card-detect line as the
/// board-claimed line.
#[allow(dead_code)] // the api flavor's checks
pub const CHECK_FREE: u32 = 4;
#[allow(dead_code)] // the api flavor's checks
pub const CHECK_OUT: Option<u32> = Some(LED_LINE);
#[allow(dead_code)] // the api flavor's checks
pub const CHECK_IN: u32 = 4;
#[allow(dead_code)] // the api flavor's checks
pub const CHECK_BOARD: u32 = SD_DETECT_LINE;
/// No loopback on a board: the software trigger makes the edge beside the toggle.
#[allow(dead_code)] // the api flavor's checks
pub const LOOPBACK: bool = false;

/// The lines another part of the board drives are inputs only: an output
/// claim on them is refused -- the BOOT button's line (a switch to ground)
/// and the microSD socket's card-detect switch.
pub const fn input_only(line: u32) -> bool {
    line == BUTTON0_LINE || line == SD_DETECT_LINE
}

#[allow(dead_code)] // the api flavor's checks
pub const CHECK_INPUT_ONLY: Option<u32> = Some(BUTTON0_LINE);

/// No unpopulated line is stated for this board: every line of the chip
/// reaches a net of the board (the headers, the connectors, the socket,
/// the LEDs, the button, the host port, the PSRAM's select), so the whole
/// space is populated.
#[allow(dead_code)] // the api flavor's checks
pub const CHECK_UNPOPULATED: Option<u32> = None;

pub const fn populated(_line: u32) -> bool {
    true
}

/// The bus tables of this row, each a bus named by the chip's own block
/// number: I2C bus 0 (the chip's first I2C block) on GPIO 20 (data) and 21
/// (clock) -- the header's SDA and SCL beside AREF, the STEMMA QT connector
/// and the HSTX connector's pair, which the board pulls up with 10 k
/// resistors of its own; SPI bus 1 (the chip's second SPI block) on GPIO
/// 30 (clock), 31 (output) and 28 (input) -- the six-pin ICSP header's
/// SCK, MOSI and MISO -- with NO chip select of the board's: the block's
/// own frame line for this pin set is GPIO 29, the USB host port's power
/// switch, so a program drives a select pin of its own choosing (the
/// settings' chip-select line -1) low before a transfer and high after it.
/// The I2C block runs on the system clock at this posture: 100 kHz, 400
/// kHz and 1 MHz (no 1 MHz in the crystal posture); the SPI block on the
/// crystal at both, 184 Hz to 6 MHz. The chip's first SPI block serves the
/// microSD socket's lines (above) and the chip's second I2C block has no
/// pins on this board; neither is a bus of this row.
pub const I2C_PADS: [Pad; 2] = [
    Pad { line: 20, alternate: crate::rp2350_bus_math::I2C_FUNCTION, role: PadRole::I2cData },
    Pad { line: 21, alternate: crate::rp2350_bus_math::I2C_FUNCTION, role: PadRole::I2cClock },
];
pub const SPI_PADS: [Pad; 3] = [
    Pad { line: 30, alternate: crate::rp2350_bus_math::SPI_FUNCTION, role: PadRole::SpiClock },
    Pad { line: 31, alternate: crate::rp2350_bus_math::SPI_FUNCTION, role: PadRole::SpiOut },
    Pad { line: 28, alternate: crate::rp2350_bus_math::SPI_FUNCTION, role: PadRole::SpiIn },
];
pub const SPI_SELECTS: [u16; 0] = [];
pub const I2C_BUSES: &[crate::bus_math::I2cBus] = &[crate::rp2350_bus_math::i2c_bus(0, super::rp2350::SYSCLK_HZ, &I2C_PADS)];
pub const SPI_BUSES: &[crate::bus_math::SpiBus] = &[crate::rp2350_bus_math::spi_bus(1, &SPI_PADS, &SPI_SELECTS, 0)];
const _: () = assert!(crate::rp2350_bus_math::rows_sound(I2C_BUSES, SPI_BUSES), "a bus row's pad does not carry its role on the chip's function table");

/// The PWM facts of this row: every line of the package carries a timer
/// output -- the RP2350 datasheet, section 12.5.2, Table 1130 (the mapping
/// of PWM channels to GPIO pins): lines 0 to 15 carry slice 0's A output,
/// slice 0's B, slice 1's A and so on to slice 7's B; lines 16 to 31 repeat
/// that order; lines 32 to 39 carry slices 8 to 11 and lines 40 to 47 repeat
/// them -- so every slice output reaches TWO lines, n and n + 16 below line
/// 32 and n and n + 8 above it, and carries the same signal on both when
/// both select it; the claims table keeps the channel exclusive, so the
/// second line's PWM claim answers RHU_EBUSY while the first holds the
/// channel. The timers are
/// the twelve slices, numbered by the slice's index, counting the system
/// clock at this posture (Table 1133); the prescaler is the divider's
/// integer part, 1 to 255, and the period's reach 65535 counts (sections
/// 12.5.2.1 and 12.5.2.2); the pad's function 4 selects the output (Table
/// 645). The rule's rows are spot-checked against the table below.
pub const PWM_TIMERS: &[crate::pwm_math::PwmTimer] = &super::rp2350_pwm::timers::<12>();
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
        && PWM_LINES[29].channel == 1
        && PWM_LINES[31].timer == 7
        && PWM_LINES[31].channel == 1
        && PWM_LINES[32].timer == 8
        && PWM_LINES[32].channel == 0
        && PWM_LINES[40].timer == 8
        && PWM_LINES[40].channel == 0
        && PWM_LINES[47].timer == 11
        && PWM_LINES[47].channel == 1,
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
/// reference the board's ADC supply, the 3.3 V rail through a filter (the
/// maker's schematic: 10 ohms and 2.2 uF). The channels (Table 1119, the
/// maker's schematic for the nets): 1 to 6 on GPIO 41 to 46, the header's
/// A0 to A5 -- the chip's own pads reach the header directly, so no channel
/// has a twin pad; 8 the temperature sensor (the RP2350 datasheet's section
/// 12.4.6), no line. NOT offered: channel 0 on GPIO 40, the microSD
/// socket's card-detect line, and channel 7 on GPIO 47, the PSRAM's chip
/// select (a pad with no part on the plain variant).
pub const ADC_BLOCKS: &[crate::adc_math::AdcBlock] =
    &[crate::adc_math::AdcBlock { number: 0, clock_hz: super::rp2350_adc::CLK_ADC_HZ, bits: 12, reference_mv: 3300, conversion_cycles: 96 }];
pub const ADC_CHANNELS: &[crate::adc_math::AdcChannel] = &[
    crate::adc_math::AdcChannel { block: 0, channel: 1, line: 41 },
    crate::adc_math::AdcChannel { block: 0, channel: 2, line: 42 },
    crate::adc_math::AdcChannel { block: 0, channel: 3, line: 43 },
    crate::adc_math::AdcChannel { block: 0, channel: 4, line: 44 },
    crate::adc_math::AdcChannel { block: 0, channel: 5, line: 45 },
    crate::adc_math::AdcChannel { block: 0, channel: 6, line: 46 },
    crate::adc_math::AdcChannel { block: 0, channel: 8, line: crate::adc_math::NO_LINE },
];
/// No channel of this row has a twin pad: the header's analog pins are the
/// converter's own pads.
pub const ADC_TWINS: &[(u16, u16)] = &[];
/// The channel table's position of the temperature sensor.
#[allow(dead_code)] // the api flavor's checks
pub const ADC_SENSOR: usize = 6;
/// The api flavor's check rows: the pin channel it claims and reads (the
/// header's A0, channel 1, a net with nothing on it); the PWM line it
/// claims (GPIO 2, the header's D2, slice 1's A output: a header line no
/// bus of this row takes), the line sharing that slice output (GPIO 18, on
/// the HSTX connector) and the slice's other output (GPIO 3, the header's
/// D3); no resistor-capacitor loop between a PWM line and a channel is
/// wired on this row.
#[allow(dead_code)] // the api flavor's checks
pub const ADC_CHECK: usize = 0;
#[allow(dead_code)] // the api flavor's checks
pub const PWM_CHECK_LINE: u32 = 2;
#[allow(dead_code)] // the api flavor's checks
pub const PWM_CHECK_TWIN: u32 = 18;
#[allow(dead_code)] // the api flavor's checks
pub const PWM_CHECK_SIBLING: u32 = 3;
#[allow(dead_code)] // the api flavor's checks
pub const ADC_LOOP: Option<(usize, usize)> = None;

/// The RAM ranges of this row (the hosted runtime learns them
/// through the `rhu_ram_count` and `rhu_ram_range` functions): the physical
/// range as the board's facts file states it in the SDK (`boards/<row>.json`,
/// its `ram` array; the two kept equal by the hosted-runtime gate's facts
/// tie), range 0 the internal SRAM the kernel's map calls DATA. Range 0's
/// `base` and `bytes` are the map's DATA region, whole: the arena window
/// the runtime is handed lies inside it, bracketed by the map.
/// 520 KB of SRAM (SRAM0 to SRAM9, one contiguous window); the plain
/// variant carries no PSRAM part, so its one range is the SRAM.
#[cfg(feature = "board-adafruit-metro-rp2350")]
pub const RAM_RANGES: &[crate::ram::RamFacts] = &[
    crate::ram::RamFacts { name: "SRAM", base: 0x2000_0000, bytes: 0x8_2000, external: false, cached: false, exec: true },
];
/// The PSRAM variant's ranges: range 1 is the board's 8 MB PSRAM part
/// behind the flash interface's second chip select: external, reached
/// through the chip's XIP cache, shown at the second select's cached
/// window. The part's chip select is on GPIO 47 (the maker's schematic,
/// with a 10 k pull-up on the board).
#[cfg(feature = "board-adafruit-metro-rp2350-psram")]
pub const RAM_RANGES: &[crate::ram::RamFacts] = &[
    crate::ram::RamFacts { name: "SRAM", base: 0x2000_0000, bytes: 0x8_2000, external: false, cached: false, exec: true },
    crate::ram::RamFacts { name: "PSRAM", base: 0x1100_0000, bytes: 0x80_0000, external: true, cached: true, exec: true },
];
/// The GPIO line the board wires the PSRAM's chip select to (the PSRAM
/// variant; the plain variant builds without the window).
#[cfg(feature = "board-adafruit-metro-rp2350-psram")]
#[allow(dead_code)] // the crystal posture builds without the window
pub const PSRAM_SELECT_LINE: u8 = 47;

/// The console's channel facts of this row (the SDK's
/// `channels` member, kept equal by the parity leg): the two streams'
/// default channel codes and the row's legal set (`crate::console`: 1 off,
/// 2 memory, 3 probe, 0x10 + n uart n).
pub const CHANNELS: crate::console::ChannelFacts = crate::console::ChannelFacts { log: crate::console::CH_MEMORY, console: crate::console::CH_MEMORY, legal: &[crate::console::CH_MEMORY, crate::console::CH_OFF, crate::console::CH_UART0, crate::console::CH_UART1] };
/// The pre-defined UART rows of this row (the SDK's `uart` member), by the
/// chip's own block number with their pins (none on a row whose UARTs have
/// no pins); the kernel's UART API and the console's wire read them.
/// UART1 has NO default pair on this row (no maker document names one) -- the block stays in the
/// table and becomes claimable when the project's pin words name a legal pair.
pub const UART_ROWS: &[crate::console::UartRow] = &[crate::console::UartRow { block: 0, tx: 0, rx: 1 }, crate::console::UartRow { block: 1, tx: crate::console::NO_PIN, rx: crate::console::NO_PIN }];
/// The gpio banner's line when the kernel holds a UART of this row as its
/// console wire, by block (empty on a row with no UART row).
pub const BANNER_UART: &[&str] = &["48 lines, 9 claimed by the board (the console UART0 on GPIO 0/1, the microSD socket on GPIO 34 to 40)", "48 lines, 9 claimed by the board (the console UART1 on the project's pins, the microSD socket on GPIO 34 to 40)"];
/// The UART rows' names for the boot banner's console clause, by block.
#[allow(dead_code)] // the Pico rows' banner clause reads it
pub const UART_NAMES: &[&str] = &["UART0 (GPIO 0 TX, GPIO 1 RX)", "UART1 (no default pins; the project's pin words name a legal pair)"];

/// The chip's legal pin sets per block for a project's pin override (the SDK's `uartPairs`
/// member): the UART function's TX and RX
/// pins (the function table's F2 column, the RP2350B's GPIO 0 to 47; the
/// UART AUX function, select 11, is not supported).
pub const UART_PAIRS: &[crate::console::UartPairs] = &[
    crate::console::UartPairs { block: 0, tx: &[0, 12, 16, 28, 32, 44], rx: &[1, 13, 17, 29, 33, 45] },
    crate::console::UartPairs { block: 1, tx: &[4, 8, 20, 24, 36, 40], rx: &[5, 9, 21, 25, 37, 41] },
];
/// The gpio banner's line when the kernel holds BOTH UART rows as wires.
pub const BANNER_UART_BOTH: &str = "48 lines, 11 claimed by the board (the console UART0 on GPIO 0/1, UART1 on the project's pins, the microSD socket on GPIO 34 to 40)";
