//! The Arduino Portenta H7 Lite's GPIO facts, RHU's own (the Portenta H7's
//! facts with this variant's population: no radio module, no secure element,
//! no video bridge): every value below is
//! stated here from the family's schematics (SL-ABX00042 V4.10, one drawing
//! stamped with the three product IDs), the Portenta Mid Carrier's
//! (ASX00055 V1.2), the chip's datasheet (DS12930) and its reference manual
//! (RM0399), and tied to the manuals' values by the asserts beside it. The
//! named signals are the module's RGB LED (three lines, lit low); the
//! module has no user button (the carrier's button is the power-management
//! chip's power line, never a GPIO).

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

/// The console's transmit and receive pins (PA9 and PA10: USART1 on the
/// module's connector and the carrier's UART header); the debug probe's
/// PA13 and PA14.
pub const CONSOLE_TX: u32 = line(0, 9);
pub const CONSOLE_RX: u32 = line(0, 10);
const _: () = assert!(CONSOLE_TX == 9 && CONSOLE_RX == 10, "the console's pins are not PA9 and PA10");

/// The board's own lines, refused to a program: the power-management
/// chip's bus (PB6, PB7: a program that drove it could reprogram the chip's
/// rails and the module's supply) and the 25 MHz oscillator's enable (PH1:
/// the chip's clock source and the Ethernet transceiver's).
pub const PMIC_SCL: u32 = line(1, 6);
pub const PMIC_SDA: u32 = line(1, 7);
pub const OSC_ENABLE_LINE: u32 = line(7, 1);
const _: () = assert!(PMIC_SCL == 22 && PMIC_SDA == 23 && OSC_ENABLE_LINE == 113, "the board's lines are PB6, PB7 and PH1");
pub const BOARD_CLAIMS: &[u16] = &[CONSOLE_TX as u16, CONSOLE_RX as u16, 13, 14, PMIC_SCL as u16, PMIC_SDA as u16, OSC_ENABLE_LINE as u16];
pub const BOARD_CLAIM_BITS: [u32; claims::BANKS] = claims::fold_board(BOARD_CLAIMS);
pub const BANNER: &str = "176 lines, 7 claimed by the board (the console PA9/PA10, the debug pins PA13/PA14, the power chip's bus PB6/PB7, the oscillator enable PH1)";

/// The named signals: the RGB LED's three lines, each lit when low (a
/// common anode on the module's 3.1 V rail).
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
/// refusal checks; no input-only line on this module; the console's
/// transmit pin as the board-claimed line.
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
/// listed here, routed to a connector or not; a line inside the space that
/// names a pin the package does not have is refused.
pub const PRESENT: [u16; PORT_COUNT as usize] = [0xFFFF, 0xFFFF, 0xFFFF, 0xFFFF, 0xFFFF, 0xFFFF, 0xFFFF, 0xFFFF, 0xFFFF, 0xFFFF, 0x00FF];

pub const fn populated(line: u32) -> bool {
    (line as usize) < LINE_COUNT && PRESENT[line as usize / 16] & (1 << (line % 16)) != 0
}

/// A line inside the space that the package does not bond out (PK8).
#[allow(dead_code)] // the api flavor's checks
pub const CHECK_UNPOPULATED: Option<u32> = Some(line(10, 8));
const _: () = assert!(!populated(line(10, 8)) && populated(CONSOLE_TX) && populated(CONSOLE_RX) && populated(13) && populated(14) && populated(LED0_LINE) && populated(LED1_LINE) && populated(LED2_LINE) && populated(PMIC_SCL) && populated(PMIC_SDA) && populated(OSC_ENABLE_LINE), "the present list contradicts the check line, the board's own pins or a named signal");
const _: () = {
    let mut n = 0u32;
    let mut i = 0;
    while i < PORT_COUNT as usize {
        n += PRESENT[i].count_ones();
        i += 1;
    }
    assert!(n == 168, "the present list is not the package's 168 I/O ports");
};

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
/// of the two families answers RHU_ENODEV. Under the
/// power-management chip's reading flavor alone (`test-h7-pmic-read`, the power chip's register
/// reading) the I2C table carries that chip's bus, the family's entry.
#[cfg(not(feature = "test-h7-pmic-read"))]
pub const I2C_BUSES: &[crate::bus_math::I2cBus] = &[];
#[cfg(feature = "test-h7-pmic-read")]
pub const I2C_BUSES: &[crate::bus_math::I2cBus] = &[crate::board::portenta_h7_common::PMIC_I2C1];
pub const SPI_BUSES: &[crate::bus_math::SpiBus] = &[];

/// The PWM and ADC tables of this row: empty (the same
/// answers as the bus tables').
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

/// What this variant carries (the collective datasheet, revision 7, and
/// the one drawing's population): the radio module, the SE050 secure element and the video bridge are
/// NOT this variant's (the Lite is the H7 without them); the
/// Ethernet transceiver (on the module itself, clocked by the oscillator
/// behind PH1), the 8 MB SDRAM behind the flexible memory controller and
/// the 16 MB QSPI flash are every variant's. A driver compiles only into
/// the rows whose facts carry its hardware.
#[allow(dead_code)]
pub const HAS_RADIO: bool = false;
#[allow(dead_code)]
pub const HAS_ETHERNET_PHY: bool = true;
#[allow(dead_code)]
pub const HAS_SDRAM: bool = true;
#[allow(dead_code)]
pub const HAS_QSPI_FLASH: bool = true;
#[allow(dead_code)]
pub const HAS_SE050: bool = false;
#[allow(dead_code)] // read by the drivers whose acts compile into the rows that carry their hardware
pub const HAS_VIDEO_BRIDGE: bool = false;

/// The console's channel facts of this row (the SDK's `channels` member):
/// both streams on the memory console by default; the probe channel (USART1
/// on the maker's console pins) and off the other legal codes
/// (`crate::console`: 1 off, 2 memory, 3 probe).
pub const CHANNELS: crate::console::ChannelFacts = crate::console::ChannelFacts { log: crate::console::CH_MEMORY, console: crate::console::CH_MEMORY, legal: &[crate::console::CH_MEMORY, crate::console::CH_PROBE, crate::console::CH_OFF] };
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
