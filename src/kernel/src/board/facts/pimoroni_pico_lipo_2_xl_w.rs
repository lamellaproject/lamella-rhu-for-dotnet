//! The Pimoroni Pico LiPo 2 XL W's GPIO facts: the kernel's own, read from
//! the maker's schematic (PIM776, four sheets) and the chip's datasheet --
//! this board has no vendored board description. The console's lines, the
//! line count of the RP2350B and the user switch are stated here, cited; the
//! board's LED hangs on the wireless module (whose lines the wifi driver
//! claims at its own open), so no LED row exists.
//!
//! The RP2350 datasheet: 48 user GPIOs on the RP2350B (chapter 1, Table 1;
//! section 9.1). The maker's schematic, sheet 1: the Raspberry Pi Radio
//! Module 2 on GPIO 23 (its power enable), 24 (its data line), 25 (its
//! select) and 29 (its clock); the user switch on GPIO 30; the Qw/ST
//! connector on GPIO 4 and 5; the SP/CE connector on GPIO 32 to 36; the
//! header's GP26, GP27 and GP28 reaching the chip's GPIO 40, 41 and 42
//! through 1 k resistors; the PSRAM's chip select on GPIO 47. Sheet 2: the
//! user LED on the module's GPIO0 through 1 k, lit when high, and the VBUS
//! sense on the module's GPIO2 behind two equal 5.1 k resistors -- neither a
//! line of the chip; the switch pulled up on the board and low while the
//! combined boot and user button is pressed (the same press pulls the
//! flash's chip select low through a diode; the boot select is sampled at
//! reset alone, so a press at run time is a plain input). Sheet 3: two rows
//! of 30 pins -- the first 40 the Raspberry Pi Pico 2's positions (GP0 to
//! GP22, GP26 to GP28), the other 20 carrying GP31 to GP39, GP43 to GP47,
//! the boot switch's net and the rails -- so every line of the chip reaches
//! a net of the board. Sheet 4: the battery sense on GPIO 43 behind a 1.5 M
//! over 750 k divider (a third of the battery's voltage), switched in while
//! the 3.3 V rail is up.

use crate::claims;

pub const LINE_COUNT: usize = 48;

/// The console's GPIO 0 and 1.
pub const UART0_TX: u32 = 0;
#[allow(dead_code)] // the pair is the facts' UART row's; this row reads the transmit side alone (CHECK_BOARD)
pub const UART0_RX: u32 = 1;
/// No line is claimed by the board by default -- the memory console takes
/// no pin; a UART the kernel holds as its wire claims its two lines at init.
pub const BOARD_CLAIMS: &[u16] = &[];
pub const BOARD_CLAIM_BITS: [u32; claims::BANKS] = claims::fold_board(BOARD_CLAIMS);
pub const BANNER: &str = "48 lines, 0 claimed by the board";

/// The wireless module's four lines through the SIO, as the Pico 2 W's rows
/// state them: its power enable (`WL_ON`, high to power), its data line
/// (`WL_D`), its select (`WL_CS`, low to select) and its clock (`WL_CLK`).
/// This board has no vendored description, so the values are stated here
/// from the maker's schematic (sheet 1): the nets run between the RP2350B's
/// GPIO23, GPIO24, GPIO25 and GPIO29 and the module's B5 `WLON`, A6 `DO`
/// (and A5 `DI` through 470 ohms), B2 `CS` and A3 `SCLK`. The wifi driver
/// claims them at its own open and ties its facts to these; the board
/// claims none of them.
#[allow(dead_code)] // the wifi driver's facts tie to these rows
pub const WIRELESS_POWER_LINE: u32 = 23;
#[allow(dead_code)] // the wifi driver's facts tie to these rows
pub const WIRELESS_DATA_LINE: u32 = 24;
#[allow(dead_code)] // the wifi driver's facts tie to these rows
pub const WIRELESS_SELECT_LINE: u32 = 25;
#[allow(dead_code)] // the wifi driver's facts tie to these rows
pub const WIRELESS_CLOCK_LINE: u32 = 29;
const _: () = assert!(
    WIRELESS_POWER_LINE < LINE_COUNT as u32
        && WIRELESS_DATA_LINE < LINE_COUNT as u32
        && WIRELESS_SELECT_LINE < LINE_COUNT as u32
        && WIRELESS_CLOCK_LINE < LINE_COUNT as u32,
    "a wireless line lies outside the RP2350B's line space"
);

/// The board's row: the user switch on GPIO 30, low when pressed; the
/// kernel pulls the line up as well, so the row does not rest on the
/// board's resistor. No LED row: the LED is the wireless module's.
#[allow(dead_code)] // the flight image reads the row
pub const BUTTON0_LINE: u32 = 30;
#[allow(dead_code)] // the flight image reads the row
pub const BUTTON0_PULL: u32 = crate::gpio_math::PULL_UP;
#[allow(dead_code)] // the flight image reads the row
pub const BUTTON0_ACTIVE_LOW: bool = true;

/// The api flavor's check lines: no output toggled (no LED line); GPIO 26
/// (a header line) as the input the
/// software-triggered edges are counted on and as the free line of the
/// claim and refusal checks; the switch as the input-only line; the
/// console's GPIO 0 as the board-claimed line.
#[allow(dead_code)] // the api flavor's checks
pub const CHECK_FREE: u32 = 26;
#[allow(dead_code)] // the api flavor's checks
pub const CHECK_OUT: Option<u32> = None;
#[allow(dead_code)] // the api flavor's checks
pub const CHECK_IN: u32 = 26;
#[allow(dead_code)] // the api flavor's checks
pub const CHECK_BOARD: u32 = UART0_TX;
/// No loopback on a board: the software trigger makes the edge beside the toggle.
#[allow(dead_code)] // the api flavor's checks
pub const LOOPBACK: bool = false;

/// The switch's line is an input only: an output claim on it is refused.
pub const fn input_only(line: u32) -> bool {
    line == BUTTON0_LINE
}

#[allow(dead_code)] // the api flavor's checks
pub const CHECK_INPUT_ONLY: Option<u32> = Some(BUTTON0_LINE);

/// No unpopulated line is stated for this board: every line of the chip
/// reaches a net of the board (the headers, the wireless module, the switch,
/// the converter twins; sheets 1 and 3), so the whole space is populated.
#[allow(dead_code)] // the api flavor's checks
pub const CHECK_UNPOPULATED: Option<u32> = None;

pub const fn populated(_line: u32) -> bool {
    true
}

/// The bus tables of this row: the chip's first I2C block and first SPI
/// block, each a bus named by the chip's own block number (I2C0 is bus 0),
/// on the pins the Raspberry Pi Pico 2's datasheet marks as that block's
/// default, at the same header positions -- this board is made pin-compatible
/// with that one, so a project moves between the two without changing its pins
/// (`crate::rp2350_bus_math`): I2C bus 0 on GPIO 4 (data)
/// and 5 (clock); SPI bus 0 on GPIO 18 (clock), 19 (output) and 16 (input)
/// with its chip select on GPIO 17. The I2C block runs on the system clock
/// at this posture: 100 kHz, 400 kHz and 1 MHz (no 1 MHz in the crystal posture);
/// the SPI block on the crystal at both, 184 Hz to 6 MHz. The pads' pull-ups make an empty bus
/// answer; an I2C bus with a device on it wants pull-ups of its own. The
/// chip's second I2C and SPI blocks have no default pins on this board and
/// are no bus of this row.
pub const I2C_BUSES: &[crate::bus_math::I2cBus] = &crate::rp2350_bus_math::i2c_buses(super::rp2350::SYSCLK_HZ);
pub const SPI_BUSES: &[crate::bus_math::SpiBus] = &crate::rp2350_bus_math::spi_buses();

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
/// maker's schematic, sheet 1: 220 ohms and 2.2 uF). The
/// channels (Table 1119, the maker's schematic for the nets): 0, 1
/// and 2 on GPIO 40, 41 and 42, whose nets reach the header pins GP26, GP27
/// and GP28 through 1 k resistors (sheet 1, the network RN2) while the chip's own GPIO 26, 27 and
/// 28 sit on those nets directly -- each of the three has a digital TWIN pad
/// on its net, parked with the channel when no program holds it (a program
/// that holds the twin sets its pad, and the reading follows); 3 on GPIO 43,
/// the battery sense (sheet 4: a 1.5 M over 750 k divider, a third of the
/// battery's voltage, switched in while the 3.3 V rail is up; the schematic is
/// the source of record -- the VBUS sense is the wireless module's GPIO2,
/// sheet 2); 4, 5 and 6 on GPIO 44, 45 and 46, the header's own pins
/// (sheet 3); 8 the temperature sensor (the RP2350 datasheet's section
/// 12.4.6), no line. NOT offered: channel 7 on GPIO 47, the PSRAM's chip
/// select.
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
/// The channels' twins: a digital pad on a channel's net, parked with it.
pub const ADC_TWINS: &[(u16, u16)] = &[(40, 26), (41, 27), (42, 28)];
/// The channel table's position of the temperature sensor.
#[allow(dead_code)] // the api flavor's checks
pub const ADC_SENSOR: usize = 7;
/// The api flavor's check rows: the pin channel it claims and reads (a
/// header net with nothing on it; its twin parked); the PWM line it claims
/// (GPIO 6, slice 3's A output: a header line no bus of this row takes),
/// the line sharing that slice output (GPIO 22) and the slice's other
/// output (GPIO 7); no resistor-capacitor loop
/// between a PWM line and a channel is wired on this row.
#[allow(dead_code)] // the api flavor's checks
pub const ADC_CHECK: usize = 0;
#[allow(dead_code)] // the api flavor's checks
pub const PWM_CHECK_LINE: u32 = 6;
#[allow(dead_code)] // the api flavor's checks
pub const PWM_CHECK_TWIN: u32 = 22;
#[allow(dead_code)] // the api flavor's checks
pub const PWM_CHECK_SIBLING: u32 = 7;
#[allow(dead_code)] // the api flavor's checks
pub const ADC_LOOP: Option<(usize, usize)> = None;

/// The RAM ranges of this row (the hosted runtime learns them
/// through the `rhu_ram_count` and `rhu_ram_range` doors): the physical
/// range as the board's facts file states it in the SDK (`boards/<row>.json`,
/// its `ram` array; the two kept equal by the hosted-runtime gate's facts
/// tie), range 0 the internal SRAM the kernel's map calls DATA. Range 0's
/// `base` and `bytes` are the map's DATA region, whole: the arena window
/// the runtime is handed lies inside it, bracketed by the map.
/// Range 1 is the board's 8 MB PSRAM part behind the flash interface's
/// second chip select: external, reached through the chip's XIP
/// cache, shown at the second select's cached window. The part's chip
/// select is on GPIO 47 (the maker's schematic, sheet 2, with a 100 k
/// pull-up on the board; the maker's board definition names the line as well).
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
/// no pins); a program's serial doors and the console's wire read them.
/// UART1 has NO default pair on this row (no maker document names one) -- the block stays in the
/// table and becomes claimable when the project's pin words name a legal pair.
pub const UART_ROWS: &[crate::console::UartRow] = &[crate::console::UartRow { block: 0, tx: 0, rx: 1 }, crate::console::UartRow { block: 1, tx: crate::console::NO_PIN, rx: crate::console::NO_PIN }];
/// The gpio banner's line when the kernel holds a UART of this row as its
/// console wire, by block (empty on a row with no UART row).
pub const BANNER_UART: &[&str] = &["48 lines, 2 claimed by the board (the console UART0 on GPIO 0/1)", "48 lines, 2 claimed by the board (the console UART1 on the project's pins)"];
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
pub const BANNER_UART_BOTH: &str = "48 lines, 4 claimed by the board (the console UART0 on GPIO 0/1, UART1 on the project's pins)";
