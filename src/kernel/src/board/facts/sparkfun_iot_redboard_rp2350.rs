//! The SparkFun IoT RedBoard RP2350's GPIO facts: the kernel's own, read from
//! the maker's schematic (four sheets: the connectors, the core and the
//! wireless module, the power circuits, the peripheral parts) and the chip's
//! datasheet -- this board has no vendored board description. Where the
//! schematic and the maker's guide differ, the schematic is the source of
//! record. The console's lines, the line count of the RP2350B, the user LED,
//! the user button and the board's own lines are stated here, cited.
//!
//! The RP2350 datasheet: 48 user GPIOs on the RP2350B (chapter 1, Table 1;
//! section 9.1). The maker's schematic, sheet 2 (the chip's pins): UART0 on
//! GPIO 0 (transmit) and 1 (receive), the header's D1 and D0; the microSD
//! socket's card detect on GPIO 2; the addressable RGB LED's data line on
//! GPIO 3; I2C0 on GPIO 4 (data) and 5 (clock); the fuel gauge's alert on
//! GPIO 6; the peripheral rail's enable on GPIO 7; the microSD socket on the
//! chip's second SPI block, GPIO 8 (data in), 9 (select), 10 (clock) and 11
//! (data out); the HSTX connector's eight lines on GPIO 12 to 19; the
//! header's SPI on the chip's first SPI block, GPIO 20 (data in, D12), 21
//! (select, D10), 22 (clock, D13) and 23 (data out, D11); the Raspberry Pi
//! Radio Module 2 on GPIO 24 (its power enable), 36 (its select), 37 (its
//! clock) and 38 (its data line); the STAT LED on GPIO 25; the two power
//! selectors' status outputs on GPIO 26 and 27; the header's D2 to D9 on
//! GPIO 28 to 35; the user button on GPIO 39; the header's A0 to A5 on GPIO
//! 40 to 45, the converter's channels 0 to 5 on the chip's own pads; the
//! barrel-jack input's sense on GPIO 46; the PSRAM's chip select on GPIO 47,
//! pulled up on the board through 10 k. Sheet 4: the STAT LED through 2.2 k
//! to ground, lit when high; the user button pulled up through 10 k with a
//! 100 nF capacitor, low while pressed (the maker's guide names GPIO 36 for
//! it; the schematic's net sits on GPIO 39); the BOOT button on the flash's
//! chip select, no line of the chip; the RESET button on the chip's RUN pin;
//! the input sense a 41.2 k over 10 k divider (2.93 V at a 15 V input). Sheet
//! 3: the fuel gauge (a MAX17048 at the I2C address 0x36) on the I2C0 pair,
//! its alert pulled up through 10 k; the peripheral rail's enable pulled up
//! through the LP solder jumper, closed as shipped; the status outputs pulled
//! up through 10 k each. Sheet 1: the Qwiic connector and the HSTX
//! connector's pins 2 and 3 on the I2C0 pair, with 2.2 k pull-ups behind the
//! I2C solder jumper, closed as shipped; the header's labels beside the
//! chip's numbers -- D0 1, D1 0, D2 28, D3 29, D4 30, D5 31, D6 32, D7 33,
//! D8 34, D9 35, D10 21, D11 23, D12 20, D13 22, SDA 4, SCL 5, A0 to A5 40
//! to 45; the HSTX connector carrying GPIO 4, 5, 28 and 29 beside its eight
//! lines; every line of the chip on a net of the board.

use crate::bus_math::{Pad, PadRole};
use crate::claims;

pub const LINE_COUNT: usize = 48;

/// The console's GPIO 0 and 1.
#[allow(dead_code)] // the pair is the facts' UART row's (UART_ROWS below names the lines by number)
pub const UART0_TX: u32 = 0;
#[allow(dead_code)] // the pair is the facts' UART row's
pub const UART0_RX: u32 = 1;
/// The board's claims: the microSD socket's five lines (GPIO 2 and 8 to 11),
/// the kernel's card host's alone, so a program's open of any of them is
/// refused. The memory console takes no pin; a UART the kernel holds as its
/// wire claims its two lines at init.
pub const BOARD_CLAIMS: &[u16] = &[SD_DETECT_LINE as u16, SD_IN_LINE as u16, SD_SELECT_LINE as u16, SD_CLOCK_LINE as u16, SD_OUT_LINE as u16];
pub const BOARD_CLAIM_BITS: [u32; claims::BANKS] = claims::fold_board(BOARD_CLAIMS);
pub const BANNER: &str = "48 lines, 5 claimed by the board (the microSD socket on GPIO 2 and 8 to 11)";

/// The board's rows: the STAT LED on GPIO 25 through the SIO, lit when high
/// (sheet 4: through 2.2 k to ground); the user button on GPIO 39, low when
/// pressed (sheet 4: a 10 k pull-up and a 100 nF capacitor on the board);
/// the kernel pulls the line up as well, so the row does not rest on the
/// board's resistor.
#[allow(dead_code)] // the silicon leg's check and the flights read the rows
pub const LED_LINE: u32 = 25;
#[allow(dead_code)] // the silicon leg's check and the flights read the rows
pub const LED_ACTIVE_LOW: bool = false;
#[allow(dead_code)] // the flight image reads the row
pub const LED0_LINE: u32 = LED_LINE;
#[allow(dead_code)] // the flight image reads the row
pub const LED0_ACTIVE_LOW: bool = LED_ACTIVE_LOW;
#[allow(dead_code)] // the flight image reads the row
pub const BUTTON0_LINE: u32 = 39;
#[allow(dead_code)] // the flight image reads the row
pub const BUTTON0_PULL: u32 = crate::gpio_math::PULL_UP;
#[allow(dead_code)] // the flight image reads the row
pub const BUTTON0_ACTIVE_LOW: bool = true;

/// The wireless module's four lines through the SIO, in the shape the Pico
/// 2 W's rows state them: its power enable (`WRL_ON`, high to power), its
/// select (`WRL_CS`, low to select), its clock (`WRL_CLK`) and its data line
/// (`WRL_DAT`). This board wires the module to lines of its own (sheet 2):
/// the nets run between the RP2350B's GPIO24, GPIO36, GPIO37 and GPIO38 and
/// the module's `WLON`, `CS`, `SCLK` and `POCI` (its `PICO` through 470 ohms
/// to the same data net); the module's own GPIO0 drives the blue WRL LED
/// (sheet 4) and its GPIO2 reads the board's power circuit, neither a line
/// of the chip. The wifi driver claims the four lines at its own open and
/// ties its facts to these; the board claims none of them.
#[allow(dead_code)] // the wifi driver's facts tie to these rows
pub const WIRELESS_POWER_LINE: u32 = 24;
#[allow(dead_code)] // the wifi driver's facts tie to these rows
pub const WIRELESS_DATA_LINE: u32 = 38;
#[allow(dead_code)] // the wifi driver's facts tie to these rows
pub const WIRELESS_SELECT_LINE: u32 = 36;
#[allow(dead_code)] // the wifi driver's facts tie to these rows
pub const WIRELESS_CLOCK_LINE: u32 = 37;
const _: () = assert!(
    WIRELESS_POWER_LINE < LINE_COUNT as u32
        && WIRELESS_DATA_LINE < LINE_COUNT as u32
        && WIRELESS_SELECT_LINE < LINE_COUNT as u32
        && WIRELESS_CLOCK_LINE < LINE_COUNT as u32,
    "a wireless line lies outside the RP2350B's line space"
);

/// The board's other lines, each a plain GPIO line to a program unless a row
/// above or below says otherwise (sheets 2 to 4): the addressable
/// RGB LED's data line (a plain output; the LED's data output continues to
/// the header's first pin); the fuel gauge's alert output, pulled up on the
/// board (an input alone); the peripheral 3.3 V rail's enable, pulled up on
/// the board through the LP solder jumper, so the microSD socket and the
/// RGB LED are powered as shipped and a program that drives the line low
/// powers them off; the two power selectors' status outputs, pulled up on
/// the board (inputs alone).
#[allow(dead_code)] // the board page's rows
pub const RGB_LED_LINE: u32 = 3;
#[allow(dead_code)] // the board page's rows
pub const GAUGE_ALERT_LINE: u32 = 6;
#[allow(dead_code)] // the board page's rows
pub const PERIPHERAL_POWER_LINE: u32 = 7;
#[allow(dead_code)] // the board page's rows
pub const POWER_SOURCE_LINE: u32 = 26;
#[allow(dead_code)] // the board page's rows
pub const BATTERY_POWER_LINE: u32 = 27;

/// The microSD socket's five lines of the chip (sheet 2): the card-detect
/// switch and, on the chip's second SPI block, data in from the card, the
/// card's select, the clock and data out to the card -- named here for the
/// kernel's card host (board/rp2350_sdspi.rs over sdspi.rs), which alone
/// drives them; all five are the board's claims above. The card-detect line
/// is an input alone (its level and its pull below). The four bus lines rest
/// low while the peripheral rail is off (the maker's caution: a high level
/// on them while the card is unpowered can damage the card's pins); the rail
/// is on as shipped (its enable above).
#[allow(dead_code)] // the board page's rows; the card host's facts tie to these
pub const SD_DETECT_LINE: u32 = 2;
#[allow(dead_code)] // the board page's rows; the card host's facts tie to these
pub const SD_IN_LINE: u32 = 8;
#[allow(dead_code)] // the board page's rows; the card host's facts tie to these
pub const SD_SELECT_LINE: u32 = 9;
#[allow(dead_code)] // the board page's rows; the card host's facts tie to these
pub const SD_CLOCK_LINE: u32 = 10;
#[allow(dead_code)] // the board page's rows; the card host's facts tie to these
pub const SD_OUT_LINE: u32 = 11;
#[allow(dead_code)] // the board page's rows
pub const SD_LINES: [u32; 5] = [SD_DETECT_LINE, SD_IN_LINE, SD_SELECT_LINE, SD_CLOCK_LINE, SD_OUT_LINE];
const _: () = assert!(SD_DETECT_LINE < LINE_COUNT as u32 && SD_SELECT_LINE < LINE_COUNT as u32, "a socket line lies outside the RP2350B's line space");
/// The socket's bus row for the kernel's card host: the chip's second SPI
/// block on the clock, data-out and data-in lines (GPIO 10 is the block's
/// clock, 11 its output, 8 its input on the chip's function table), with
/// the card's select as the one select line, a plain output the host
/// drives. The row is the host's alone: not a bus of the kernel's bus API.
pub const SD_PADS: [Pad; 3] = [
    Pad { line: SD_CLOCK_LINE as u16, alternate: crate::rp2350_bus_math::SPI_FUNCTION, role: PadRole::SpiClock },
    Pad { line: SD_OUT_LINE as u16, alternate: crate::rp2350_bus_math::SPI_FUNCTION, role: PadRole::SpiOut },
    Pad { line: SD_IN_LINE as u16, alternate: crate::rp2350_bus_math::SPI_FUNCTION, role: PadRole::SpiIn },
];
pub const SD_SELECTS: [u16; 1] = [SD_SELECT_LINE as u16];
pub const SD_BUS: crate::bus_math::SpiBus = crate::rp2350_bus_math::spi_bus(1, &SD_PADS, &SD_SELECTS, 1);
/// The block's name for the boot banner's sd line.
pub const SD_BUS_NAME: &str = "SPI1";
const _: () = assert!(crate::rp2350_bus_math::rows_sound(&[], &[SD_BUS]), "the socket's pads do not carry their roles on the chip's function table");
/// The level the card-detect line reads with a card seated: LOW -- read on
/// the board through the probe with a card in the socket, low with no pull
/// of the chip's and low with its pull-up (the switch closes to ground).
/// With the socket empty the line reads HIGH on the board's own pull-up:
/// read on the board through the probe with the card out, high with no
/// pull of the chip's and high with its pull-up.
pub const SD_DETECT_CARD_LEVEL: bool = false;
/// The chip's pull on the card-detect line at the host's attach: none, by
/// the readings above (the board's pull-up holds the line).
pub const SD_DETECT_PULL: u32 = crate::gpio_math::PULL_NONE;

/// The api flavor's check lines: the LED as the output claimed at its
/// initial level and toggled; GPIO 30 (the header's D4, a line no bus of
/// this row takes) as the input the
/// software-triggered edges are counted on and as the free line of the
/// claim and refusal checks; the button as the input-only line; the
/// microSD socket's card-detect line as the board-claimed line.
#[allow(dead_code)] // the api flavor's checks
pub const CHECK_FREE: u32 = 30;
#[allow(dead_code)] // the api flavor's checks
pub const CHECK_OUT: Option<u32> = Some(LED_LINE);
#[allow(dead_code)] // the api flavor's checks
pub const CHECK_IN: u32 = 30;
#[allow(dead_code)] // the api flavor's checks
pub const CHECK_BOARD: u32 = SD_DETECT_LINE;
/// No loopback on a board: the software trigger makes the edge beside the toggle.
#[allow(dead_code)] // the api flavor's checks
pub const LOOPBACK: bool = false;

/// The lines another part of the board drives are inputs only: an output
/// claim on them is refused -- the user button's line (a switch to ground),
/// the card-detect switch's, the fuel gauge's alert (its open-drain output)
/// and the two power selectors' status outputs (sheets 3 and 4).
pub const fn input_only(line: u32) -> bool {
    line == BUTTON0_LINE || line == SD_DETECT_LINE || line == GAUGE_ALERT_LINE || line == POWER_SOURCE_LINE || line == BATTERY_POWER_LINE
}

#[allow(dead_code)] // the api flavor's checks
pub const CHECK_INPUT_ONLY: Option<u32> = Some(BUTTON0_LINE);

/// No unpopulated line is stated for this board: every line of the chip
/// reaches a net of the board (the header, the connectors, the module, the
/// socket, the LEDs, the button, the senses; sheet 2), so the whole space is
/// populated.
#[allow(dead_code)] // the api flavor's checks
pub const CHECK_UNPOPULATED: Option<u32> = None;

pub const fn populated(_line: u32) -> bool {
    true
}

/// The bus tables of this row, each a bus named by the chip's own block
/// number (I2C0 is bus 0): I2C bus 0 on GPIO 4 (data) and 5 (clock), the
/// pins the Raspberry Pi Pico 2's datasheet marks as the block's default --
/// the header's SDA and SCL, the Qwiic connector and the HSTX connector's
/// pins 2 and 3, with the board's own 2.2 k pull-ups behind the I2C solder
/// jumper (closed as shipped) and the fuel gauge at the address 0x36 on the
/// bus (sheets 1 and 3); SPI bus 0 (the chip's first SPI block) on GPIO 22
/// (clock), 23 (output) and 20 (input) with its chip select on GPIO 21 --
/// the header's D13, D11, D12 and D10 (sheets 1 and 2), one of the block's
/// pin sets on the chip's function table. The I2C block runs on the system
/// clock at this posture: 100 kHz, 400 kHz and 1 MHz (no 1 MHz in the
/// crystal posture); the SPI block on the crystal at both, 184 Hz to 6 MHz.
/// The chip's second SPI block serves the microSD socket's lines (above)
/// and the chip's second I2C block has no pins on this board; neither is a
/// bus of this row.
pub const I2C_PADS: [Pad; 2] = [
    Pad { line: 4, alternate: crate::rp2350_bus_math::I2C_FUNCTION, role: PadRole::I2cData },
    Pad { line: 5, alternate: crate::rp2350_bus_math::I2C_FUNCTION, role: PadRole::I2cClock },
];
pub const SPI_PADS: [Pad; 3] = [
    Pad { line: 22, alternate: crate::rp2350_bus_math::SPI_FUNCTION, role: PadRole::SpiClock },
    Pad { line: 23, alternate: crate::rp2350_bus_math::SPI_FUNCTION, role: PadRole::SpiOut },
    Pad { line: 20, alternate: crate::rp2350_bus_math::SPI_FUNCTION, role: PadRole::SpiIn },
];
pub const SPI_SELECTS: [u16; 1] = [21];
pub const I2C_BUSES: &[crate::bus_math::I2cBus] = &[crate::rp2350_bus_math::i2c_bus(0, super::rp2350::SYSCLK_HZ, &I2C_PADS)];
pub const SPI_BUSES: &[crate::bus_math::SpiBus] = &[crate::rp2350_bus_math::spi_bus(0, &SPI_PADS, &SPI_SELECTS, 1)];
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
/// maker's schematic, sheet 2: 200 ohms and 4.7 uF). The
/// channels (Table 1119, the maker's schematic for the nets): 0 to 5 on
/// GPIO 40 to 45, the header's A0 to A5 -- the chip's own pads reach the
/// header directly, so no channel has a twin pad; 6 on GPIO 46, the
/// barrel-jack input's sense (sheet 4: a 41.2 k over 10 k divider, 2.93 V
/// at a 15 V input); 8 the temperature sensor (the RP2350 datasheet's
/// section 12.4.6), no line. NOT offered: channel 7 on GPIO 47, the PSRAM's
/// chip select.
pub const ADC_BLOCKS: &[crate::adc_math::AdcBlock] =
    &[crate::adc_math::AdcBlock { number: 0, clock_hz: super::rp2350_adc::CLK_ADC_HZ, bits: 12, reference_mv: 3300, conversion_cycles: 96 }];
pub const ADC_CHANNELS: &[crate::adc_math::AdcChannel] = &[
    crate::adc_math::AdcChannel { block: 0, channel: 0, line: 40 },
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
pub const ADC_SENSOR: usize = 7;
/// The api flavor's check rows: the pin channel it claims and reads (the
/// header's A0, a net with nothing on it); the PWM line it claims (GPIO 28,
/// the header's D2, slice 6's A output: a header line no bus of this row
/// takes), the line sharing that slice output (GPIO 12, on the HSTX
/// connector) and the slice's other output (GPIO 29, the header's D3); no
/// resistor-capacitor loop between a PWM line and a channel is wired on
/// this row.
#[allow(dead_code)] // the api flavor's checks
pub const ADC_CHECK: usize = 0;
#[allow(dead_code)] // the api flavor's checks
pub const PWM_CHECK_LINE: u32 = 28;
#[allow(dead_code)] // the api flavor's checks
pub const PWM_CHECK_TWIN: u32 = 12;
#[allow(dead_code)] // the api flavor's checks
pub const PWM_CHECK_SIBLING: u32 = 29;
#[allow(dead_code)] // the api flavor's checks
pub const ADC_LOOP: Option<(usize, usize)> = None;

/// The RAM ranges of this row (the hosted runtime learns them
/// through the `rhu_ram_count` and `rhu_ram_range` functions): the physical
/// range as the board's facts file states it in the SDK (`boards/<row>.json`,
/// its `ram` array; the two kept equal by the hosted-runtime gate's facts
/// tie), range 0 the internal SRAM the kernel's map calls DATA. Range 0's
/// `base` and `bytes` are the map's DATA region, whole: the arena window
/// the runtime is handed lies inside it, bracketed by the map.
/// Range 1 is the board's 8 MB PSRAM part behind the flash interface's
/// second chip select: external, reached through the chip's XIP
/// cache, shown at the second select's cached window. The part's chip
/// select is on GPIO 47 (the maker's schematic, sheet 2, with a 10 k
/// pull-up on the board).
pub const RAM_RANGES: &[crate::ram::RamFacts] = &[
    crate::ram::RamFacts { name: "SRAM", base: 0x2000_0000, bytes: 0x8_2000, external: false, cached: false, exec: true },
    crate::ram::RamFacts { name: "PSRAM", base: 0x1100_0000, bytes: 0x80_0000, external: true, cached: true, exec: true },
];
/// The GPIO line the board wires the PSRAM's chip select to.
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
pub const BANNER_UART: &[&str] = &["48 lines, 7 claimed by the board (the console UART0 on GPIO 0/1, the microSD socket on GPIO 2 and 8 to 11)", "48 lines, 7 claimed by the board (the console UART1 on the project's pins, the microSD socket on GPIO 2 and 8 to 11)"];
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
pub const BANNER_UART_BOTH: &str = "48 lines, 9 claimed by the board (the console UART0 on GPIO 0/1, UART1 on the project's pins, the microSD socket on GPIO 2 and 8 to 11)";
