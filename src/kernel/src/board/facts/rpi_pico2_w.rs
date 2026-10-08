//! The Raspberry Pi Pico 2 W's GPIO facts: the board's generated
//! description, vendored beside this file at its public commit
//! (`vendored/`), read here into the members the kernel's doors and
//! drivers use. The console's lines are DERIVED from the vendored
//! constants (the UART's pin control registers in the IO bank) and the
//! wireless module's four lines from its device rows, all tied to the
//! chip's block bases; the line count is RHU's own (the description does
//! not carry it), cited. The board's LED hangs on the wireless module, not
//! on a chip line, so no LED row exists here; the board has no user button
//! (BOOTSEL is not a GPIO row), so no line is input only, and a hardware run's
//! button is RHU's own header row as on the Pico 2.

#[allow(dead_code)]
#[path = "vendored/rpi_pico2_w_bindings.rs"]
mod bindings;

use crate::claims;

pub const LINE_COUNT: usize = 30;

/// The IO bank's and the SIO's block bases, as the chip module states them.
const IO_BANK0: u32 = 0x4002_8000;
const SIO: u32 = 0xD000_0000;

/// A line number from a GPIO's IO bank CTRL register address (0x04 + 8n).
const fn line_of_ctrl(ctrl: u32) -> u32 {
    (ctrl - IO_BANK0 - 0x04) / 8
}

/// The console's transmit and receive lines from the description (GPIO 0
/// and 1, the debug probe's serial pair).
pub const CONSOLE_TX: u32 = line_of_ctrl(bindings::UART0_IO_TX_CTRL);
pub const CONSOLE_RX: u32 = line_of_ctrl(bindings::UART0_IO_RX_CTRL);
const _: () = assert!(CONSOLE_TX == 0 && CONSOLE_RX == 1, "the console's lines are not GPIO 0 and 1");
pub const BOARD_CLAIMS: &[u16] = &[CONSOLE_TX as u16, CONSOLE_RX as u16];
pub const BOARD_CLAIM_BITS: [u32; claims::BANKS] = claims::fold_board(BOARD_CLAIMS);
pub const BANNER: &str = "30 lines, 2 claimed by the board (the console GPIO 0/1)";

/// The description's rows: the wireless module's four lines through the
/// SIO -- its power enable (high to power), its data line, its select
/// (low to select) and its clock. The wifi driver claims them at its own
/// open and ties its facts to these; the board claims none of them.
#[allow(dead_code)]
pub const WIRELESS_POWER_LINE: u32 = bindings::CYW43439_WL_REG_ON_PIN;
#[allow(dead_code)]
pub const WIRELESS_DATA_LINE: u32 = bindings::CYW43439_DATA_PIN;
#[allow(dead_code)]
pub const WIRELESS_SELECT_LINE: u32 = bindings::CYW43439_CS_PIN;
#[allow(dead_code)]
pub const WIRELESS_CLOCK_LINE: u32 = bindings::CYW43439_CLK_PIN;
const _: () = assert!(
    bindings::CYW43439_WL_REG_ON_PORT_BASE == SIO
        && bindings::CYW43439_DATA_PORT_BASE == SIO
        && bindings::CYW43439_CS_PORT_BASE == SIO
        && bindings::CYW43439_CLK_PORT_BASE == SIO,
    "a wireless line's port base is not the SIO block"
);
const _: () = assert!(
    bindings::CYW43439_WL_REG_ON_MASK == 1 << bindings::CYW43439_WL_REG_ON_PIN
        && bindings::CYW43439_DATA_MASK == 1 << bindings::CYW43439_DATA_PIN
        && bindings::CYW43439_CS_MASK == 1 << bindings::CYW43439_CS_PIN
        && bindings::CYW43439_CLK_MASK == 1 << bindings::CYW43439_CLK_PIN,
    "a wireless line's mask is not its pin's bit"
);
const _: () = assert!(bindings::CYW43439_WL_REG_ON_ACTIVE_LOW == 0 && bindings::CYW43439_CS_ACTIVE_LOW == 1, "the wireless module's power and select levels are not high and low");
const _: () = assert!(bindings::DEVICE_COUNT == 4, "the description's device count is not the four rows read here");

/// RHU's own row for what the description lacks: the acceptance run's
/// button on a header line (GP22, header pin 29, beside a ground pin),
/// pulled up by the kernel and pressed by a jumper touched to ground --
/// active low; this row has no LED for a hardware run to drive.
#[allow(dead_code)]
pub const BUTTON0_LINE: u32 = 22;
#[allow(dead_code)]
pub const BUTTON0_PULL: u32 = crate::gpio_math::PULL_UP;
#[allow(dead_code)]
pub const BUTTON0_ACTIVE_LOW: bool = true;

/// The api flavor's check lines: no output toggled (no LED line); GPIO 26
/// (a header line, ADC0) as the input the software-triggered edges are
/// counted on and as the free line of the claim and refusal checks; the
/// console's GPIO 0 as the board-claimed line.
#[allow(dead_code)]
pub const CHECK_FREE: u32 = 26;
#[allow(dead_code)]
pub const CHECK_OUT: Option<u32> = None;
#[allow(dead_code)]
pub const CHECK_IN: u32 = 26;
#[allow(dead_code)]
pub const CHECK_BOARD: u32 = CONSOLE_TX;
/// No loopback on a board: the software trigger makes the edge beside the toggle.
#[allow(dead_code)]
pub const LOOPBACK: bool = false;

pub const fn input_only(_line: u32) -> bool {
    false
}

#[allow(dead_code)]
pub const CHECK_INPUT_ONLY: Option<u32> = None;

/// The description carries no pin present-list at this commit, so no
/// unpopulated line can be stated: the whole space is populated.
#[allow(dead_code)]
pub const CHECK_UNPOPULATED: Option<u32> = None;

pub const fn populated(_line: u32) -> bool {
    true
}

/// The bus tables of this row: the chip's first I2C block and first SPI
/// block, each a bus named by the chip's own block number (I2C0 is bus 0),
/// on the pins the board's datasheet marks as that block's default (`crate::rp2350_bus_math`): I2C bus 0 on GPIO 4 (data)
/// and 5 (clock); SPI bus 0 on GPIO 18 (clock), 19 (output) and 16 (input)
/// with its chip select on GPIO 17. The I2C block runs on the system clock
/// at this posture: 100 kHz, 400 kHz and 1 MHz (no 1 MHz in the crystal posture);
/// the SPI block on the crystal at both, 184 Hz to 6 MHz. The pads' pull-ups make an empty bus
/// answer; an I2C bus with a device on it wants pull-ups of its own. The
/// chip's second I2C and SPI blocks have no default pins on this board and
/// are no bus of this row.
pub const I2C_BUSES: &[crate::bus_math::I2cBus] = &crate::rp2350_bus_math::i2c_buses(super::rp2350::SYSCLK_HZ);
pub const SPI_BUSES: &[crate::bus_math::SpiBus] = &crate::rp2350_bus_math::spi_buses();
/// Bus 0 of each family is the board description's own row (the vendored
/// constants: the block's base, its reset bit, the pin-control registers, the
/// function number, the SPI block's clock), tied here.
const _: () = assert!(
    bindings::I2C0_BASE as usize == I2C_BUSES[0].base
        && bindings::I2C0_RESET_MASK & (1 << I2C_BUSES[0].enable_bit) != 0
        && line_of_ctrl(bindings::I2C0_IO_SDA_CTRL) == I2C_BUSES[0].pads[0].line as u32
        && line_of_ctrl(bindings::I2C0_IO_SCL_CTRL) == I2C_BUSES[0].pads[1].line as u32
        && bindings::I2C0_FUNCSEL == I2C_BUSES[0].pads[0].alternate as u32,
    "I2C bus 0's row is not the board description's I2C0"
);
const _: () = assert!(
    bindings::SPI0_BASE as usize == SPI_BUSES[0].base
        && bindings::SPI0_RESET_MASK & (1 << SPI_BUSES[0].enable_bit) != 0
        && line_of_ctrl(bindings::SPI0_IO_SCK_CTRL) == SPI_BUSES[0].pads[0].line as u32
        && line_of_ctrl(bindings::SPI0_IO_MOSI_CTRL) == SPI_BUSES[0].pads[1].line as u32
        && line_of_ctrl(bindings::SPI0_IO_MISO_CTRL) == SPI_BUSES[0].pads[2].line as u32
        && line_of_ctrl(bindings::SPI0_IO_CS_CTRL) == SPI_BUSES[0].selects[0] as u32
        && bindings::SPI0_FUNCSEL == SPI_BUSES[0].pads[0].alternate as u32
        && bindings::SPI0_SSPCLK_HZ == SPI_BUSES[0].kernel_hz,
    "SPI bus 0's row is not the board description's SPI0"
);

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
/// function 4 selects the output (Table 645). The wireless module's four
/// lines are listed as every other line is: the wifi driver claims them at
/// its own open. The rule's rows are spot-checked against the table below.
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
/// thousand samples a second (section 12.4.3), but the block's registers
/// do not answer while that clock runs faster than the system clock (a
/// fact of the part, measured, not in the text) and a crystal-posture image's
/// system clock is 12 MHz, so the crystal drives the converter on both
/// postures, 8 microseconds a sample -- its
/// reference the board's ADC supply -- the 3.3 V rail through a filter (the
/// Pico 2 W datasheet, section 3.3). The channels (Table 1118): 0 to 2 on
/// GPIO 26 to 28 and 4 the temperature sensor (section 12.4.6), no line.
/// Channel 3 is NOT offered on this row: its line, GPIO 29, is the wireless
/// module's clock (the board datasheet's pinout: the clock is shared with
/// the supply monitor, readable only while the module is selected and
/// idle), the wifi driver's line. No line of this board twins a channel's
/// pad.
pub const ADC_BLOCKS: &[crate::adc_math::AdcBlock] =
    &[crate::adc_math::AdcBlock { number: 0, clock_hz: super::rp2350_adc::CLK_ADC_HZ, bits: 12, reference_mv: 3300, conversion_cycles: 96 }];
pub const ADC_CHANNELS: &[crate::adc_math::AdcChannel] = &[
    crate::adc_math::AdcChannel { block: 0, channel: 0, line: 26 },
    crate::adc_math::AdcChannel { block: 0, channel: 1, line: 27 },
    crate::adc_math::AdcChannel { block: 0, channel: 2, line: 28 },
    crate::adc_math::AdcChannel { block: 0, channel: 4, line: crate::adc_math::NO_LINE },
];
/// The channels' twins: a digital pad on a channel's net, parked with it.
pub const ADC_TWINS: &[(u16, u16)] = &[];
/// The channel table's position of the temperature sensor.
#[allow(dead_code)]
pub const ADC_SENSOR: usize = 3;
/// The api flavor's check rows: the pin channel it claims and reads (a
/// header line with nothing on it); the PWM line it claims (GPIO 6, slice
/// 3's A output: a header line no bus of this row takes), the line sharing
/// that slice output (GPIO 22) and the slice's other output (GPIO 7); no resistor-capacitor loop between a PWM
/// line and a channel is wired on this row.
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
