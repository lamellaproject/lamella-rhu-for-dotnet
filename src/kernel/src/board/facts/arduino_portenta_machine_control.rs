//! The Arduino Portenta Machine Control's GPIO facts, RHU's own: the
//! Portenta H7's facts (the module on this carrier is a Portenta H7: one
//! chip, one power-management chip, one oscillator arrangement, the radio
//! module, the secure element and the video bridge fitted) with the
//! carrier's terminals added as a named-signal table and the board's claims
//! stated for this carrier. Every value below is stated here from the module's
//! schematics (SL-ABX00042 V4.10), the carrier's schematic (AKX00032 V1.1,
//! nineteen sheets) and full-pinout document, the chip's datasheet
//! (DS12930) and its reference manual (RM0399), and tied to the manuals'
//! values by the asserts beside it.

use crate::claims;

/// Eleven ports of sixteen lines: PA0 = 0 ... PK15 = 175.
#[allow(dead_code)] // the facts' statement of the block; the driver derives the port from the line
pub const PORT_COUNT: u32 = 11;
pub const LINE_COUNT: usize = 176;

/// The block's base and the ports' clock enable (RM0399: GPIOA's block,
/// RCC_AHB4ENR).
pub const GPIO_BASE: usize = 0x5802_0000;
pub const RCC_GPIO_EN: *mut u32 = 0x5802_44E0 as *mut u32;

/// A line number from a port index and a pin index.
const fn line(port: u32, pin: u32) -> u32 {
    port * 16 + pin
}

/// The debug probe's lines PA13 (SWDIO) and PA14 (SWCLK), the board's by
/// the family's rule although no header carries them on this carrier: on
/// this carrier the claim also keeps the CAN transceiver's standby net and
/// the eighth digital output's switch input at the chip's reset pulls.
pub const DEBUG_SWDIO: u32 = line(0, 13);
pub const DEBUG_SWCLK: u32 = line(0, 14);
const _: () = assert!(DEBUG_SWDIO == 13 && DEBUG_SWCLK == 14, "the debug pins are not PA13 and PA14");

/// The board's own lines, refused to a program: the power-management
/// chip's bus (PB6, PB7: a program that drove it could reprogram the chip's
/// rails and the module's supply; the carrier's real-time clock hangs on the
/// same bus and is reachable through the kernel's claim alone) and the 25
/// MHz oscillator's enable (PH1: the chip's clock source and the Ethernet
/// transceiver's).
pub const PMIC_SCL: u32 = line(1, 6);
pub const PMIC_SDA: u32 = line(1, 7);
pub const OSC_ENABLE_LINE: u32 = line(7, 1);
const _: () = assert!(PMIC_SCL == 22 && PMIC_SDA == 23 && OSC_ENABLE_LINE == 113, "the board's lines are PB6, PB7 and PH1");
pub const BOARD_CLAIMS: &[u16] = &[DEBUG_SWDIO as u16, DEBUG_SWCLK as u16, PMIC_SCL as u16, PMIC_SDA as u16, OSC_ENABLE_LINE as u16];
pub const BOARD_CLAIM_BITS: [u32; claims::BANKS] = claims::fold_board(BOARD_CLAIMS);
pub const BANNER: &str = "176 lines, 5 claimed by the board (the debug pins PA13/PA14, the power chip's bus PB6/PB7, the oscillator enable PH1)";

/// The named signals of the module: the RGB LED's three lines, each lit
/// when low (a common anode on the module's logic rail), inside the housing.
#[allow(dead_code)]
pub const LED0_LINE: u32 = line(10, 5);
#[allow(dead_code)]
pub const LED0_ACTIVE_LOW: bool = true;
#[allow(dead_code)]
pub const LED1_LINE: u32 = line(10, 6);
#[allow(dead_code)]
pub const LED1_ACTIVE_LOW: bool = true;
#[allow(dead_code)]
pub const LED2_LINE: u32 = line(10, 7);
#[allow(dead_code)] // the silicon leg's check and the flights read the rows
pub const LED2_ACTIVE_LOW: bool = true;
const _: () = assert!(LED0_LINE == 165 && LED1_LINE == 166 && LED2_LINE == 167, "the named signals are PK5, PK6 and PK7");

/// The api flavor's check lines: the red LED as the output claimed at its
/// initial level and toggled; the green LED as the input the
/// software-triggered edges are counted on (a defined level through the
/// LED's resistor: a quiet pad) and as the free line of the claim and
/// refusal checks; no input-only line on this module; the debug pin PA13 as
/// the board-claimed line.
#[allow(dead_code)] // the api flavor's checks
pub const CHECK_FREE: u32 = LED1_LINE;
#[allow(dead_code)] // the api flavor's checks
pub const CHECK_OUT: Option<u32> = Some(LED0_LINE);
#[allow(dead_code)] // the api flavor's checks
pub const CHECK_IN: u32 = LED1_LINE;
#[allow(dead_code)] // the api flavor's checks
pub const CHECK_BOARD: u32 = DEBUG_SWDIO;
/// No loopback on a board: the software trigger makes the edge beside the toggle.
#[allow(dead_code)] // the api flavor's checks
pub const LOOPBACK: bool = false;

/// No line of this module is an input only.
pub const fn input_only(_line: u32) -> bool {
    false
}

#[allow(dead_code)] // the api flavor's checks
pub const CHECK_INPUT_ONLY: Option<u32> = None;

/// The pins this part's package bonds out, one mask per port (bit n is pin
/// n): the STM32H747XI in its 240-ball package, from the part's datasheet
/// (DS12930: 168 I/O ports in that package's column of the pin table; ports
/// A to J whole, port K pins 0 to 7 alone). A program may claim any pin
/// listed here, routed to a terminal or not; a line inside the space that
/// names a pin the package does not have is refused.
pub const PRESENT: [u16; PORT_COUNT as usize] = [0xFFFF, 0xFFFF, 0xFFFF, 0xFFFF, 0xFFFF, 0xFFFF, 0xFFFF, 0xFFFF, 0xFFFF, 0xFFFF, 0x00FF];

pub const fn populated(line: u32) -> bool {
    (line as usize) < LINE_COUNT && PRESENT[line as usize / 16] & (1 << (line % 16)) != 0
}

/// A line inside the space that the package does not bond out (PK8).
#[allow(dead_code)] // the api flavor's checks
pub const CHECK_UNPOPULATED: Option<u32> = Some(line(10, 8));
const _: () = assert!(!populated(line(10, 8)) && populated(DEBUG_SWDIO) && populated(DEBUG_SWCLK) && populated(LED0_LINE) && populated(LED1_LINE) && populated(LED2_LINE) && populated(PMIC_SCL) && populated(PMIC_SDA) && populated(OSC_ENABLE_LINE), "the present list contradicts the check line, the board's own pins or a named signal");
const _: () = {
    let mut n = 0u32;
    let mut i = 0;
    while i < PORT_COUNT as usize {
        n += PRESENT[i].count_ones();
        i += 1;
    }
    assert!(n == 168, "the present list is not the package's 168 I/O ports");
};

/// The carrier's terminals as named signals: the terminal (the carrier's
/// connector and its label), the chip line, the line's direction at the
/// chip and the part behind the terminal, from the carrier's schematic
/// (sheets 1, 2, 4, 6, 7, 9, 10 and 12 to 19) and full-pinout document. In
/// RHU a program opens a terminal's line by number through the
/// GPIO door and drives the part itself (the switches' inputs, the mode
/// lines, the selects); the expanders behind the digital inputs and the
/// programmable block, the thermocouple and RTD front-ends and the two
/// transceivers are bus devices on I2C3, SPI2, UART4 and FDCAN1, which no
/// door of this row serves. Every line is asserted inside the package's
/// present list; the two on a board-claimed line are the debug pins, whose
/// claim and its effect the row states (`DEBUG_SWDIO`, `DEBUG_SWCLK`). The
/// directions: `in` a signal into the chip, `out` a chip output into a part,
/// `analog` the ADC pad paired with the line (the SYSCFG analog switch
/// routes the pad; the GPIO line itself keeps its digital function), `pwm`
/// a timer output into the analog output stage, `spi`, `uart`, `can` and
/// `i2c` the bus lines of the peripheral blocks named.
#[allow(dead_code)] // stated for the drivers and the programs that drive the carrier's parts
pub const CARRIER_LINES: &[(&str, u16, &str, &str)] = &[
    // J10 encoders: each terminal pulled to 24 V by 10 k and divided to the chip.
    ("J10 A0", line(9, 8) as u16, "in", "encoder 0 phase A"),
    ("J10 B0", line(7, 12) as u16, "in", "encoder 0 phase B"),
    ("J10 Z0", line(7, 11) as u16, "in", "encoder 0 index"),
    ("J10 A1", line(2, 13) as u16, "in", "encoder 1 phase A"),
    ("J10 B1", line(8, 7) as u16, "in", "encoder 1 phase B"),
    ("J10 Z1", line(9, 10) as u16, "in", "encoder 1 index"),
    // J6 digital outputs: two quad high-side switches (TPS4H160) fed from the terminal's own 24 V input.
    ("J6 00", line(8, 6) as u16, "out", "high-side switch U2 input 1"),
    ("J6 01", line(7, 9) as u16, "out", "high-side switch U2 input 2"),
    ("J6 02", line(9, 9) as u16, "out", "high-side switch U2 input 3"),
    ("J6 03", line(4, 2) as u16, "out", "high-side switch U2 input 4"),
    ("J6 04", line(8, 3) as u16, "out", "high-side switch U3 input 1"),
    ("J6 05", line(8, 2) as u16, "out", "high-side switch U3 input 2"),
    ("J6 06", line(3, 3) as u16, "out", "high-side switch U3 input 3"),
    ("J6 07", line(0, 14) as u16, "out", "high-side switch U3 input 4 on the chip's SWCLK pin, board-claimed: the reset pull-down keeps the output off"),
    ("J6 latch/retry", line(1, 2) as u16, "out", "both switches' thermal-behaviour pins (latch or retry at the current limit)"),
    // J8 programmable block: three high-side switches driven by an I2C expander; two chip lines alone.
    ("J8 latch/retry", line(7, 14) as u16, "out", "the three switches' thermal-behaviour pins"),
    ("J8 expander INT", line(8, 5) as u16, "in", "the I2C expander U21's interrupt (its enables and read-backs are port bits on I2C3)"),
    // J3 digital inputs: eight divided inputs read by an I2C expander; one chip line.
    ("J3 expander INT", line(1, 4) as u16, "in", "the I2C expander U25's interrupt (the eight inputs are its port bits on I2C3)"),
    // J9 analog inputs: the direct analog pads and the per-channel mode switches (0 to 10 V, 4 to 20 mA, NTC), each mode line inverted by a transistor.
    ("J9 AI0", line(2, 3) as u16, "analog", "the ADC pad PC3_C beside this line (the line itself is SPI2 MOSI)"),
    ("J9 AI1", line(2, 2) as u16, "analog", "the ADC pad PC2_C beside this line (the line itself is SPI2 MISO)"),
    ("J9 AI2", line(0, 1) as u16, "analog", "the ADC pad PA1_C beside this line"),
    ("J9 AI0 mode 1", line(3, 4) as u16, "out", "analog switch U4 IN1 through an inverting transistor"),
    ("J9 AI0 mode 2", line(3, 5) as u16, "out", "analog switch U4 IN2 through an inverting transistor"),
    ("J9 AI0 mode 3", line(4, 3) as u16, "out", "analog switch U4 IN3 through an inverting transistor"),
    ("J9 AI0 mode 4", line(6, 3) as u16, "out", "analog switch U4 IN4 through an inverting transistor"),
    ("J9 AI1 mode 1", line(3, 7) as u16, "out", "analog switch U5 IN1 through an inverting transistor"),
    ("J9 AI1 mode 2", line(7, 6) as u16, "out", "analog switch U5 IN2 through an inverting transistor"),
    ("J9 AI1 mode 3", line(9, 7) as u16, "out", "analog switch U5 IN3 through an inverting transistor"),
    ("J9 AI1 mode 4", line(7, 15) as u16, "out", "analog switch U5 IN4 through an inverting transistor"),
    ("J9 AI2 mode 1", line(7, 10) as u16, "out", "analog switch U6 IN1 through an inverting transistor"),
    ("J9 AI2 mode 2", line(0, 4) as u16, "out", "analog switch U6 IN2 through an inverting transistor"),
    ("J9 AI2 mode 3", line(0, 8) as u16, "out", "analog switch U6 IN3 through an inverting transistor"),
    ("J9 AI2 mode 4", line(2, 6) as u16, "out", "analog switch U6 IN4 through an inverting transistor"),
    // J11 analog outputs: a timer output into a double low-pass filter and a gain-3.3 amplifier stage (0 to 10 V).
    ("J11 AO0", line(9, 11) as u16, "pwm", "the filter and amplifier stage of analog output 0"),
    ("J11 AO1", line(10, 1) as u16, "pwm", "the filter and amplifier stage of analog output 1"),
    ("J11 AO2", line(6, 7) as u16, "pwm", "the filter and amplifier stage of analog output 2 (the high-resolution timer's pin)"),
    ("J11 AO3", line(2, 7) as u16, "pwm", "the filter and amplifier stage of analog output 3"),
    // J7 temperature probes: the thermocouple and RTD front-ends on SPI2, one channel and one front-end selected at a time.
    ("J7 SPI2 SCK", line(8, 1) as u16, "spi", "the thermocouple and RTD front-ends' clock"),
    ("J7 SPI2 MISO", line(2, 2) as u16, "spi", "the thermocouple and RTD front-ends' data out"),
    ("J7 SPI2 MOSI", line(2, 3) as u16, "spi", "the RTD front-end's data in"),
    ("J7 CS0", line(8, 0) as u16, "out", "the thermocouple front-end U10's select"),
    ("J7 CS1", line(0, 6) as u16, "out", "the RTD front-end U13's select"),
    ("J7 front-end select", line(2, 15) as u16, "out", "the triple switch U11 between the thermocouple and RTD front-ends"),
    ("J7 channel 0 select", line(3, 6) as u16, "out", "the channel switch U14's four enables"),
    ("J7 channel 1 select", line(8, 4) as u16, "out", "the channel switch U12's four enables"),
    ("J7 channel 2 select", line(6, 10) as u16, "out", "the channel switch U9's four enables"),
    // J5 serial: the RS-485/RS-422/RS-232 transceiver (SP335) on UART4; its mode lines rest by the carrier's resistors (shut down until EN-485 is driven high).
    ("J5 UART4 TX", line(0, 0) as u16, "uart", "the serial transceiver's driver input"),
    ("J5 UART4 RX", line(8, 9) as u16, "uart", "the serial transceiver's receiver output"),
    ("J5 DE", line(8, 13) as u16, "out", "the serial transceiver's driver enable"),
    ("J5 RE", line(8, 10) as u16, "out", "the serial transceiver's receiver enable"),
    ("J5 EN-485", line(6, 9) as u16, "out", "the serial transceiver's shutdown line (the part rests shut down through 100 k)"),
    ("J5 H/F", line(0, 9) as u16, "out", "half or full duplex: the maker's console transmit pin, not a console on this carrier"),
    ("J5 485/232", line(0, 10) as u16, "out", "RS-485 or RS-232: the maker's console receive pin, not a console on this carrier"),
    ("J5 SLEW", line(6, 14) as u16, "out", "the serial transceiver's slew-rate limit"),
    ("J5 TERM", line(8, 14) as u16, "out", "the serial transceiver's receiver termination"),
    ("J5 FD_TX_TERM", line(8, 15) as u16, "out", "the serial transceiver's driver termination"),
    // J5 CAN: the CAN transceiver (TJA1049) on FDCAN1; the bus terminated on the carrier.
    ("J5 CAN TX", line(7, 13) as u16, "can", "FDCAN1 transmit into the CAN transceiver"),
    ("J5 CAN RX", line(1, 8) as u16, "can", "FDCAN1 receive from the CAN transceiver"),
    ("J5 CAN standby", line(0, 13) as u16, "out", "the CAN transceiver's standby net on the chip's SWDIO pin, board-claimed: the reset pull-up holds the transceiver in standby"),
    // J15 USB-A: the chip's full-speed port behind a power switch (NCP383).
    ("J15 power enable", line(1, 14) as u16, "out", "the USB-A power switch U24's enable"),
    ("J15 power fault", line(1, 15) as u16, "in", "the USB-A power switch's fault flag"),
    // The real-time clock (PCF8563) on the power-management chip's bus, its interrupt alone a free line.
    ("RTC INT", line(1, 9) as u16, "in", "the real-time clock U18's interrupt (the clock itself is on the board-claimed PB6/PB7)"),
    // I2C3: the two expanders and the Grove connector J17.
    ("I2C3 SCL", line(7, 7) as u16, "i2c", "the two I2C expanders and the Grove connector J17"),
    ("I2C3 SDA", line(7, 8) as u16, "i2c", "the two I2C expanders and the Grove connector J17"),
];

/// Whether a line is one of the board's own.
const fn board_claimed(l: u32) -> bool {
    let mut j = 0;
    while j < BOARD_CLAIMS.len() {
        if BOARD_CLAIMS[j] as u32 == l {
            return true;
        }
        j += 1;
    }
    false
}

const _: () = {
    assert!(CARRIER_LINES.len() == 64, "the carrier's table is not the schematic's 64 chip lines");
    let mut i = 0;
    while i < CARRIER_LINES.len() {
        let l = CARRIER_LINES[i].1 as u32;
        assert!(populated(l), "a carrier terminal names a pin the package does not bond out");
        assert!(!board_claimed(l) || l == DEBUG_SWDIO || l == DEBUG_SWCLK, "a carrier terminal sits on a board-claimed line other than the two debug pins");
        i += 1;
    }
};

/// The carrier's logic rail. The carrier has no 3.3 V regulator of its own:
/// its +3V3 net is the module's VCC output, the power-management chip's
/// second switching output (the carrier's schematic, sheets 1 and 4), and
/// every front-end's logic supply hangs on it. On this silicon's factory
/// program the output's voltage registers are read-only (the chip's
/// datasheet, Table 31: the DVS-disabled mode), so the level is the chip's
/// one-time-programmed value and the kernel writes nothing to the chip: the
/// maker's 3.3 V output set by a register write is not reachable by that
/// path.
#[allow(dead_code)] // stated for the drivers that depend on the rail
pub const LOGIC_RAIL_MV_REFERENCE: u32 = 3105;
#[allow(dead_code)] // stated for the drivers that depend on the rail
pub const LOGIC_RAIL_SET_BY_KERNEL: bool = false;

/// The classic external-interrupt arm's blocks, which the family GPIO
/// driver reads from the board's facts (RM0399): the EXTI block at
/// 0x5800_0000 (chapter 21), the first SYSCFG port-selection register at
/// 0x5800_0408 (13.3.2), the SYSCFG clock enable RCC_APB4ENR (offset 0x0F4)
/// with its bit 1.
pub const EXTI_BASE: usize = 0x5800_0000;
pub const SYSCFG_EXTICR1: usize = 0x5800_0408;
pub const RCC_SYSCFG_EN: *mut u32 = 0x5802_44F4 as *mut u32;
pub const RCC_SYSCFG_EN_BIT: u32 = 1 << 1;

/// The bus tables of this row: empty, so the count doors
/// answer 0, the enumeration doors refuse every index and every other door
/// of the two families answers RHU_ENODEV.
/// Under the power-management chip's reading flavor alone
/// (`test-h7-pmic-read`) the I2C table carries that chip's bus, the
/// family's entry, for the unit's own rail reading.
#[cfg(not(feature = "test-h7-pmic-read"))]
pub const I2C_BUSES: &[crate::bus_math::I2cBus] = &[];
#[cfg(feature = "test-h7-pmic-read")]
pub const I2C_BUSES: &[crate::bus_math::I2cBus] = &[crate::board::portenta_h7_common::PMIC_I2C1];
pub const SPI_BUSES: &[crate::bus_math::SpiBus] = &[];

/// The PWM and ADC tables of this row: empty (the same
/// answers as the bus tables'); the analog outputs' timer lines and the
/// analog inputs' pads of the carrier table are not registered on this row.
pub const PWM_TIMERS: &[crate::pwm_math::PwmTimer] = &[];
pub const PWM_LINES: &[crate::pwm_math::PwmLine] = &[];
pub const ADC_BLOCKS: &[crate::adc_math::AdcBlock] = &[];
pub const ADC_CHANNELS: &[crate::adc_math::AdcChannel] = &[];

/// The RAM ranges of this row (the hosted runtime learns them
/// through the `rhu_ram_count` and `rhu_ram_range` doors): the physical
/// range as the board's facts file states it in the SDK (`boards/<row>.json`,
/// its `ram` array; the two kept equal by the hosted-runtime gate's facts
/// tie), range 0 the internal SRAM the kernel's map calls DATA, whole: the
/// 512 KB AXI SRAM of the D1 domain (the one contiguous window every kernel
/// static, the console mirror, the stacks and the arena live in; reachable
/// by the chip's DMA masters), the caches off.
pub const RAM_RANGES: &[crate::ram::RamFacts] = &[crate::ram::RamFacts { name: "SRAM", base: 0x2400_0000, bytes: 0x8_0000, external: false, cached: false, exec: true }];

/// The chip's further internal memories, enabled at boot and stated here
/// as the chip's facts (the arena carries two windows, the internal SRAM and
/// one external RAM, so they are not RAM ranges): the 128 KB data tightly
/// coupled memory (the core's own
/// port; the DMA masters do not reach it), the D2 domain's three blocks as
/// one contiguous 288 KB, the D3 domain's 64 KB. Names, bases and sizes
/// per RM0399's memory map.
#[allow(dead_code)] // stated for the drivers that take these memories
pub const INTERNAL_MEMORIES: &[(&str, usize, usize)] = &[("DTCM", 0x2000_0000, 0x2_0000), ("SRAM1", 0x3000_0000, 0x4_8000), ("SRAM4", 0x3800_0000, 0x1_0000)];

/// What this product carries (the module is a Portenta H7 by the carrier's
/// schematic, sheet 2, and the product's datasheet): the radio module (the
/// one driver's rows; its antenna line is cabled to the carrier's SMA
/// connector), the SE050 secure element and the video bridge (on the
/// module, wired to nothing on this carrier), the Ethernet transceiver (on
/// the module, clocked by the oscillator behind PH1, through the carrier's
/// magnetics and jack), the 8 MB SDRAM behind the flexible memory controller
/// and the 16 MB QSPI flash. A driver compiles only into the rows whose
/// facts carry its hardware.
#[allow(dead_code)]
pub const HAS_RADIO: bool = true;
#[allow(dead_code)]
pub const HAS_ETHERNET_PHY: bool = true;
#[allow(dead_code)]
pub const HAS_SDRAM: bool = true;
#[allow(dead_code)]
pub const HAS_QSPI_FLASH: bool = true;
#[allow(dead_code)]
pub const HAS_SE050: bool = true;
#[allow(dead_code)] // read by the drivers whose acts compile into the rows that carry their hardware
pub const HAS_VIDEO_BRIDGE: bool = true;

/// The console's channel facts of this row (the SDK's `channels` member):
/// both streams on the memory console by default, and the legal codes
/// memory and off alone
/// (`crate::console`: 1 off, 2 memory). The probe channel is NOT legal on
/// this row: no debug header reaches the module on this carrier, and the
/// wire that channel names on the Portenta rows (USART1 on PA9 and PA10)
/// would drive the serial transceiver's mode lines at every byte; a
/// project word naming it takes the row's default, said by the boot's
/// refusal line.
pub const CHANNELS: crate::console::ChannelFacts = crate::console::ChannelFacts { log: crate::console::CH_MEMORY, console: crate::console::CH_MEMORY, legal: &[crate::console::CH_MEMORY, crate::console::CH_OFF] };
/// The pre-defined UART rows of this row (the SDK's `uart` member): none;
/// a program's serial doors answer that the board has none.
pub const UART_ROWS: &[crate::console::UartRow] = &[];
/// The gpio banner's line when the kernel holds a UART of this row as its
/// console wire, by block (empty on a row with no UART row).
pub const BANNER_UART: &[&str] = &[];
/// The UART rows' names for the boot banner's console clause, by block.
#[allow(dead_code)] // the Pico rows' banner clause reads it
pub const UART_NAMES: &[&str] = &[];
/// No pin override on this row (there are no UART rows): the SDK's
/// `uartPairs` member is empty.
#[allow(dead_code)] // no row of this board reads it: the UART doors answer ENODEV here
pub const UART_PAIRS: &[crate::console::UartPairs] = &[];
/// No both-held banner line on this row.
pub const BANNER_UART_BOTH: &str = "";
