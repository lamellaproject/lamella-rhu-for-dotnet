//! The Pimoroni Pico Plus 2's GPIO facts: the kernel's own, read from the
//! board's schematic and the chip's datasheet -- this board has no vendored
//! board description. The console's lines, the line count of the RP2350B,
//! the user LED and the user button are stated here, cited.

use crate::claims;

pub const LINE_COUNT: usize = 48;

/// The console's GPIO 0 and 1.
pub const CONSOLE_TX: u32 = 0;
pub const CONSOLE_RX: u32 = 1;
pub const BOARD_CLAIMS: &[u16] = &[CONSOLE_TX as u16, CONSOLE_RX as u16];
pub const BOARD_CLAIM_BITS: [u32; claims::BANKS] = claims::fold_board(BOARD_CLAIMS);
pub const BANNER: &str = "48 lines, 2 claimed by the board (the console GPIO 0/1)";

/// The board's rows: the LED on GPIO 25 through the SIO, lit when high; the
/// user switch on GPIO 45, low when pressed, the board's own pull-up
/// holding its level, so none is applied.
#[allow(dead_code)]
pub const LED_LINE: u32 = 25;
#[allow(dead_code)]
pub const LED_ACTIVE_LOW: bool = false;
#[allow(dead_code)]
pub const LED0_LINE: u32 = LED_LINE;
#[allow(dead_code)]
pub const LED0_ACTIVE_LOW: bool = LED_ACTIVE_LOW;
#[allow(dead_code)]
pub const BUTTON0_LINE: u32 = 45;
#[allow(dead_code)]
pub const BUTTON0_PULL: u32 = crate::gpio_math::PULL_NONE;
#[allow(dead_code)]
pub const BUTTON0_ACTIVE_LOW: bool = true;

/// The api flavor's check lines: the LED as the output claimed at its
/// initial level and toggled; GPIO 26 (a header line) as the input the
/// software-triggered edges are counted on and as the free line of the
/// claim and refusal checks; the switch as the input-only line; the
/// console's GPIO 0 as the board-claimed line.
#[allow(dead_code)]
pub const CHECK_FREE: u32 = 26;
#[allow(dead_code)]
pub const CHECK_OUT: Option<u32> = Some(LED_LINE);
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
/// thousand samples a second (section 12.4.3), but the block's registers
/// do not answer while that clock runs faster than the system clock (a
/// fact of the part, measured, not in the text) and a crystal-posture image's
/// system clock is 12 MHz, so the crystal drives the converter on both
/// postures, 8 microseconds a sample -- its
/// reference the board's ADC supply, the 3.3 V rail through a filter. The
/// channels (Table 1119, the board's schematic sheet 1 for the nets): 0, 1
/// and 2 on GPIO 40, 41 and 42, whose nets reach the header pins GP26, GP27
/// and GP28 through a resistor network while the chip's own GPIO 26, 27 and
/// 28 sit on those nets directly -- each of the three has a digital TWIN pad
/// on its net, parked with the channel when no program holds it (a program
/// that holds the twin sets its pad, and the reading follows); 3 on GPIO 43,
/// the board's supply sense; 4 on GPIO 44 and 6 on GPIO 46, which the board
/// routes to no net; 8 the temperature sensor (section 12.4.6), no line.
/// NOT offered: channel 5 on GPIO 45, the board's user switch (a row of its
/// own, input only), and channel 7 on GPIO 47, the PSRAM's chip select.
pub const ADC_BLOCKS: &[crate::adc_math::AdcBlock] =
    &[crate::adc_math::AdcBlock { number: 0, clock_hz: super::rp2350_adc::CLK_ADC_HZ, bits: 12, reference_mv: 3300, conversion_cycles: 96 }];
pub const ADC_CHANNELS: &[crate::adc_math::AdcChannel] = &[
    crate::adc_math::AdcChannel { block: 0, channel: 0, line: 40 },
    crate::adc_math::AdcChannel { block: 0, channel: 1, line: 41 },
    crate::adc_math::AdcChannel { block: 0, channel: 2, line: 42 },
    crate::adc_math::AdcChannel { block: 0, channel: 3, line: 43 },
    crate::adc_math::AdcChannel { block: 0, channel: 4, line: 44 },
    crate::adc_math::AdcChannel { block: 0, channel: 6, line: 46 },
    crate::adc_math::AdcChannel { block: 0, channel: 8, line: crate::adc_math::NO_LINE },
];
/// The channels' twins: a digital pad on a channel's net, parked with it.
pub const ADC_TWINS: &[(u16, u16)] = &[(40, 26), (41, 27), (42, 28)];
/// The channel table's position of the temperature sensor.
#[allow(dead_code)]
pub const ADC_SENSOR: usize = 6;
/// The api flavor's check rows: the pin channel it claims and reads (a
/// header net with nothing on it; its twin parked); the PWM line it claims
/// (GPIO 6, slice 3's A output: a header line no bus of this row takes),
/// the line sharing that slice output (GPIO 22) and the slice's other
/// output (GPIO 7); no resistor-capacitor loop
/// between a PWM line and a channel is wired on this row.
#[allow(dead_code)]
pub const ADC_CHECK: usize = 0;
#[allow(dead_code)]
pub const PWM_CHECK_LINE: u32 = 6;
#[allow(dead_code)]
pub const PWM_CHECK_TWIN: u32 = 22;
#[allow(dead_code)]
pub const PWM_CHECK_SIBLING: u32 = 7;
#[allow(dead_code)]
pub const ADC_LOOP: Option<(usize, usize)> = None;
