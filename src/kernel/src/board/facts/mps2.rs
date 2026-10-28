//! The emulator rows' GPIO facts: a line
//! space of six VIRTUAL lines served by the virtual backend -- three
//! latches (0 to 2), line 2 also the output of a loopback pair whose input
//! is line 3, line 4 claimed by the board as the virtual console's line so
//! the board-claim refusal is gated here too, and line 5 UNPOPULATED (a
//! line inside the space with no pad) so the unpopulated refusal is gated
//! here. Line 1 is marked input-only (a virtual button) so the input-only
//! refusal is gated here as well; lines 0, 2 and 3 also carry timer outputs
//! and lines 0 to 3 converter inputs, the tables below. No register, no generated description: these facts are
//! the kernel's own, stated here.

use crate::bus_math;
use crate::claims;

/// The line space: the highest line plus one.
pub const LINE_COUNT: usize = 6;
#[cfg(not(feature = "test-no-board-claims"))]
pub const BOARD_CLAIMS: &[u16] = &[4];
#[cfg(not(feature = "test-no-board-claims"))]
pub const BANNER: &str = "6 virtual lines (5 populated), 1 claimed by the board (the virtual console)";
/// The api flavor's control `test-no-board-claims` (never a shipped image):
/// the Pico rows' shape on this row -- no line claimed by the board (as
/// the memory console takes no pin) -- so the gpio-refuse check's skip path
/// and its witness bound run on the emulator.
#[cfg(feature = "test-no-board-claims")]
pub const BOARD_CLAIMS: &[u16] = &[];
#[cfg(feature = "test-no-board-claims")]
pub const BANNER: &str = "6 virtual lines (5 populated), 0 claimed by the board (the control's shape: test-no-board-claims)";
pub const BOARD_CLAIM_BITS: [u32; claims::BANKS] = claims::fold_board(BOARD_CLAIMS);

#[cfg(all(feature = "test-no-board-claims", not(feature = "api-smoke")))]
compile_error!("test-no-board-claims is the api flavor's control feature: select api-smoke beside it");

pub const LOOP_OUT: u32 = 2;
pub const LOOP_IN: u32 = 3;

/// The api flavor's check lines: a free latch, the loop pair, the board's
/// line, the unpopulated line.
#[allow(dead_code)] // the api flavor's checks
pub const CHECK_FREE: u32 = 0;
#[allow(dead_code)] // the api flavor's checks
pub const CHECK_INPUT_ONLY: Option<u32> = Some(1);
#[allow(dead_code)] // the api flavor's checks
pub const CHECK_OUT: Option<u32> = Some(LOOP_OUT);
#[allow(dead_code)] // the api flavor's checks
pub const CHECK_IN: u32 = LOOP_IN;
#[allow(dead_code)] // the api flavor's checks
pub const CHECK_BOARD: u32 = 4;
/// The output loops back to the input on this row: a toggle IS the edge.
#[allow(dead_code)] // the api flavor's checks
pub const LOOPBACK: bool = true;
#[allow(dead_code)] // the api flavor's checks
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
#[cfg(all(feature = "board-mps2-an505", not(feature = "test-bus-rp2350")))]
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
#[cfg(all(feature = "board-mps2-an505", not(feature = "test-bus-rp2350")))]
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
/// Under the test feature `test-bus-rp2350` the Cortex-M33 twin carries the
/// RP2350's first I2C block and first SPI block in place of the ST blocks:
/// register-level models behind the virtual backend, so that chip's bus
/// drivers are gated here before a board. The rows are the chip's own (bus
/// 0 of each family, the I2C block at the 12 MHz posture); the models have
/// no pins, so the pad lists are empty and the selects are the models' two.
#[cfg(all(feature = "board-mps2-an505", feature = "test-bus-rp2350"))]
pub const I2C_BUSES: &[bus_math::I2cBus] = &[crate::rp2350_bus_math::i2c_bus(0, 12_000_000, &[])];
#[cfg(all(feature = "board-mps2-an505", feature = "test-bus-rp2350"))]
pub const SPI_BUSES: &[bus_math::SpiBus] = &[crate::rp2350_bus_math::spi_bus(0, &[], &[], 2)];

/// The PWM facts of the emulator rows: two VIRTUAL timers of two channels
/// each on a 1 MHz clock with 16-bit prescaler and period registers (the
/// ST shape, so the quantization the door reports is measurable here),
/// driving three of the virtual lines as their alternate function -- lines
/// 0 and 2 on timer 1 (so the shared-period rule is exercised), line 3 on
/// timer 2 -- the way a real pin carries a timer output beside its GPIO
/// function. The models have no pins: the alternate function is 0.
pub const PWM_TIMERS: &[crate::pwm_math::PwmTimer] = &[
    crate::pwm_math::PwmTimer { number: 1, clock_hz: 1_000_000, channels: 2, prescaler_max: 65536, period_max: 65536 },
    crate::pwm_math::PwmTimer { number: 2, clock_hz: 1_000_000, channels: 2, prescaler_max: 65536, period_max: 65536 },
];
pub const PWM_LINES: &[crate::pwm_math::PwmLine] = &[
    crate::pwm_math::PwmLine { line: 0, timer: 0, channel: 0, alternate: 0 },
    crate::pwm_math::PwmLine { line: 2, timer: 0, channel: 1, alternate: 0 },
    crate::pwm_math::PwmLine { line: 3, timer: 1, channel: 0, alternate: 0 },
];

/// The ADC facts of the emulator rows: two VIRTUAL converter blocks on a
/// 4 MHz clock -- block 1 of 12 bits against a 3300 mV reference with four
/// pin channels reading lines 0 to 3 (a converter input beside the line's
/// GPIO function, as a real pin carries it), block 2 of 14 bits against a
/// 3000 mV reference with the internal reference channel and the LOOP
/// channel, which reads the first PWM line's duty cycle as the fixture's
/// resistor-capacitor network will (an internal source of the model; on the
/// fixture board the analog header pin). A conversion takes the width's
/// cycles plus three of sampling. The internal reference measures a known
/// 1212 mV.
pub const ADC_BLOCKS: &[crate::adc_math::AdcBlock] = &[
    crate::adc_math::AdcBlock { number: 1, clock_hz: 4_000_000, bits: 12, reference_mv: 3300, conversion_cycles: 15 },
    crate::adc_math::AdcBlock { number: 2, clock_hz: 4_000_000, bits: 14, reference_mv: 3000, conversion_cycles: 17 },
];
pub const ADC_CHANNELS: &[crate::adc_math::AdcChannel] = &[
    crate::adc_math::AdcChannel { block: 0, channel: 0, line: 0 },
    crate::adc_math::AdcChannel { block: 0, channel: 1, line: 1 },
    crate::adc_math::AdcChannel { block: 0, channel: 2, line: 2 },
    crate::adc_math::AdcChannel { block: 0, channel: 3, line: 3 },
    crate::adc_math::AdcChannel { block: 1, channel: 0, line: crate::adc_math::NO_LINE },
    crate::adc_math::AdcChannel { block: 1, channel: 1, line: crate::adc_math::NO_LINE },
];
/// The channel table's positions the model reads specially, and the
/// internal reference's known voltage.
pub const ADC_REFERENCE: usize = 4;
pub const ADC_LOOP: usize = 5;
/// The PWM line table's position the loop channel reads.
pub const ADC_LOOP_SOURCE: usize = 0;
pub const ADC_REFERENCE_MV: u32 = 1212;

/// The RAM ranges of this row (the hosted runtime learns them
/// through the `rhu_ram_count` and `rhu_ram_range` doors): the physical
/// range as the board's facts file states it in the SDK (`boards/<row>.json`,
/// its `ram` array; the two kept equal by the hosted-runtime gate's facts
/// tie), range 0 the internal SRAM the kernel's map calls DATA. Range 0's
/// `base` and `bytes` are the map's DATA region, whole: the arena window
/// the runtime is handed lies inside it, bracketed by the map.
/// The emulator rows' 4 MB of ZBT SSRAM (the application notes' SSRAM2
/// and SSRAM3 as one window; the AN500 at 0x20000000, the AN505 through
/// its secure alias at 0x38000000), and the machines' 16 MB PSRAM bank
/// (the AN500's at 0x60000000, the AN505's at 0x80000000; QEMU models
/// both as plain RAM): the second range, external, uncached, execute-never
/// by the protection unit's region over it.
#[cfg(feature = "board-mps2-an500")]
pub const RAM_RANGES: &[crate::ram::RamFacts] = &[
    crate::ram::RamFacts { name: "SSRAM", base: 0x2000_0000, bytes: 0x40_0000, external: false, cached: false, exec: true },
    crate::ram::RamFacts { name: "PSRAM", base: 0x6000_0000, bytes: 0x100_0000, external: true, cached: false, exec: false },
];
#[cfg(feature = "board-mps2-an505")]
pub const RAM_RANGES: &[crate::ram::RamFacts] = &[
    crate::ram::RamFacts { name: "SSRAM", base: 0x3800_0000, bytes: 0x40_0000, external: false, cached: false, exec: true },
    crate::ram::RamFacts { name: "PSRAM", base: 0x8000_0000, bytes: 0x100_0000, external: true, cached: false, exec: false },
];

/// The console's channel facts of this row (the SDK's `channels` member):
/// the two streams' default channel codes and the row's legal set
/// (`crate::console`: 1 off, 2 memory, 3 probe, 0x10 + n uart n).
pub const CHANNELS: crate::console::ChannelFacts = crate::console::ChannelFacts { log: crate::console::CH_UART0, console: crate::console::CH_UART0, legal: &[crate::console::CH_UART0, crate::console::CH_UART1, crate::console::CH_MEMORY, crate::console::CH_OFF] };
/// The pre-defined UART rows of this row (the SDK's `uart` member), by the
/// chip's own block number with their pins (none on a row whose UARTs have
/// no pins); a program's serial doors and the console's wire read them.
pub const UART_ROWS: &[crate::console::UartRow] = &[crate::console::UartRow { block: 0, tx: crate::console::NO_PIN, rx: crate::console::NO_PIN }, crate::console::UartRow { block: 1, tx: crate::console::NO_PIN, rx: crate::console::NO_PIN }];
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
