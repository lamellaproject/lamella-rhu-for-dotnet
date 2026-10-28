//! The STM32F7 family's quad-SPI flash bring-up: the QUADSPI block driving
//! the STM32F769 Discovery board's 512 Mbit (64 MB) NOR flash part, mapped
//! at 0x90000000 in the block's memory-mapped mode so the hosted program's
//! code and read-only data an execute-in-place map places there run in
//! place through the window. Brought up from kmain with the clock final and
//! no interrupt source enabled, when the map the image was linked under
//! declares the window (the brackets below; the kernel is one for every
//! map, and the block stays disabled under a map that declares no window),
//! before the SDRAM and before any payload address is touched.
//!
//! The order: the ports' and the block's clocks; the six pins; the
//! controller's words; the exit from the four-line command mode a debug
//! session may have left the part in; the part's identification bytes read
//! on one line and matched against the table of the two parts the board's
//! schematic names (an answer that matches neither is a loud death naming
//! the bytes); the part's register fields for the four-line read written
//! and read back; the window's first 256 bytes read by the controller's
//! indirect mode; the memory-mapped mode entered; the same bytes read
//! through the window and compared word by word (the two-path read-back:
//! a frame the part and the controller disagree on shows here, never as
//! garbage executed later); the window header's words compared with the
//! image's own link-time words (the matched-set check: the window's half
//! and the internal half of an image are one build, flashed together).
//! Every refusal is a loud death inside the call naming the step.
//!
//! Every word the controller takes is DERIVED at compile time -- the
//! prescaler from the posture's bus clock and the chosen flash clock, the
//! size field from the facts' flash range, the chip-select high time from
//! the parts' limits -- by `const fn`s whose asserts refuse a posture the
//! chip or a part cannot meet: a bad posture fails the BUILD, never a
//! flight. The part table's two rows carry the facts the parts' datasheets
//! state, each cited at its row; a row whose datasheet has not been read is
//! a sentinel row (`read: false`) that matches no part, and the bring-up
//! refuses before the part is touched while any row is a sentinel.
//!
//! Register addresses and fields per RM0410 chapter 14 (the QUADSPI
//! block), 5.3 (the RCC enables) and 6.4 (the GPIO registers); the pins
//! per the board's user manual UM2033 and its schematic, checked against
//! the chip datasheet DS11532's alternate-function table; the clock
//! ceiling per DS11532's quad-SPI characteristics; the workarounds per the
//! device errata ES0334 section 2.4. The kernel takes no vendor headers.
//! Nothing here masks an interrupt or takes the critical section.

use core::arch::asm;
use core::fmt;
use core::ptr::{addr_of, addr_of_mut, read_volatile, write_volatile};

use crate::board::facts;

// ---------------- the block and the board's pins ----------------

/// The quad-SPI block of a row and the board's six flash pins, as the
/// row's facts state them: the block's register base, its clock enable
/// register and bit, and the pins as lines (the port times sixteen plus the
/// pin) with their alternate-function numbers, in the order CLK, NCS, IO0,
/// IO1, IO2, IO3.
pub struct QspiFacts {
    pub base: usize,
    pub enable_reg: usize,
    pub enable_bit: u32,
    pub pins: [(u32, u8); 6],
}

/// The ports' clock enables (RM0410 5.3.10 RCC_AHB1ENR: bit n is port n).
const RCC_AHB1ENR: *mut u32 = 0x4002_3830 as *mut u32;
/// The port blocks (RM0410 6.4: GPIOA at 0x4002_0000, one 1 KB block per
/// port; MODER at 0x00, OTYPER at 0x04, OSPEEDR at 0x08, PUPDR at 0x0C,
/// AFRL at 0x20, AFRH at 0x24).
const GPIO_BASE: usize = 0x4002_0000;
const GPIO_PORT_STRIDE: usize = 0x400;
const GPIO_MODER: usize = 0x00;
const GPIO_OTYPER: usize = 0x04;
const GPIO_OSPEEDR: usize = 0x08;
const GPIO_PUPDR: usize = 0x0C;
const GPIO_AFRL: usize = 0x20;
const GPIO_AFRH: usize = 0x24;

/// The block's registers (RM0410 14.5.14): the control, device
/// configuration, status, flag clear, data length, communication
/// configuration, address, alternate-byte and data registers.
const QSPI_CR: usize = 0x00;
const QSPI_DCR: usize = 0x04;
const QSPI_SR: usize = 0x08;
const QSPI_FCR: usize = 0x0C;
const QSPI_DLR: usize = 0x10;
const QSPI_CCR: usize = 0x14;
const QSPI_AR: usize = 0x18;
const QSPI_ABR: usize = 0x1C;
const QSPI_DR: usize = 0x20;

/// QUADSPI_CR (14.5.1): EN [0], ABORT [1], TCEN [3], SSHIFT [4], FTHRES
/// [12:8], PRESCALER [31:24]; every field is written with BUSY clear. The
/// timeout counter stays off (ES0334 2.4.3), the sample shift as the
/// posture's timing derives it (SAMPLE_SHIFT below), the FIFO threshold 0
/// (the FIFO flag at one byte).
const CR_EN: u32 = 1 << 0;
const CR_ABORT: u32 = 1 << 1;
const CR_SSHIFT: u32 = 1 << 4;
/// QUADSPI_SR (14.5.3): TEF [0], TCF [1], FTF [2], BUSY [5].
const SR_TEF: u32 = 1 << 0;
const SR_FTF: u32 = 1 << 2;
const SR_BUSY: u32 = 1 << 5;
/// QUADSPI_FCR (14.5.4): CTEF [0], CTCF [1].
const FCR_CLEAR: u32 = (1 << 0) | (1 << 1);
/// QUADSPI_CCR (14.5.6): INSTRUCTION [7:0], IMODE [9:8], ADMODE [11:10],
/// ADSIZE [13:12], ABMODE [15:14], ABSIZE [17:16], DCYC [22:18], DMODE
/// [25:24], FMODE [27:26]. A phase's mode 00 skips it, 01 sends it on one
/// line, 11 on four; FMODE 00 is the indirect write mode, 01 the indirect
/// read mode, 11 the memory-mapped mode; ADSIZE 11 a 32-bit address; ABSIZE
/// 00 one alternate byte. In a one-line phase the block drives IO2 low and
/// IO3 high (the hold function inactive) and in a four-line dummy phase
/// leaves the four lines undriven (14.3.3), which is why a part whose first
/// dummy clocks read a mode indicator is given it as an alternate byte.
const LINES_NONE: u32 = 0b00;
const LINES_ONE: u32 = 0b01;
const LINES_FOUR: u32 = 0b11;
const FMODE_IWRITE: u32 = 0b00;
const FMODE_IREAD: u32 = 0b01;
const FMODE_MMAP: u32 = 0b11;
const ADSIZE_32: u32 = 0b11;

/// A communication word: the functional mode, the instruction's lines and
/// byte, the address's lines (32 bits wide where sent), the alternate
/// byte's lines (one byte where sent), the dummy clocks and the data's
/// lines.
const fn ccr(fmode: u32, imode: u32, instruction: u8, admode: u32, abmode: u32, dcyc: u32, dmode: u32) -> u32 {
    (fmode << 26) | (dmode << 24) | (dcyc << 18) | (abmode << 14) | (ADSIZE_32 << 12) | (admode << 10) | (imode << 8) | instruction as u32
}

/// The read-identification instruction on one line, three bytes answered
/// (the manufacturer, the memory type, the capacity): JEDEC's standard
/// instruction, carried by both parts' datasheets (the rows cite them).
const READ_ID: u8 = 0x9F;
/// The exit from four-line command mode, sent on four lines before
/// anything else (both parts: Micron's RESET QUAD INPUT/OUTPUT MODE,
/// Macronix's RSTQIO): a debug session's board file leaves the part in
/// that mode, where a one-line instruction is not decoded, and a part in
/// one-line mode ignores the frame.
const EXIT_QUAD_COMMAND_MODE: u8 = 0xF5;
/// The quad input/output fast read with a 32-bit address: the instruction
/// on one line, the address and the data on four, the part's dummy clocks
/// between them (Micron's 4-BYTE QUAD INPUT/OUTPUT FAST READ, Macronix's
/// 4READ4B; the frame a debug probe's board file for this board runs the
/// part with in its memory-mapped word, decoded by RM0410 14.5.6).
pub const QUAD_READ_4B: u8 = 0xEC;

/// The polls of a status flag before a step is declared stuck (each a
/// register read; a command's longest frame here is 256 bytes on four
/// lines, under 600 clocks of the flash clock), and the status reads a
/// part may stay busy for after a register write (the Macronix part's
/// status write cycle tW is 40 ms; a read is about a microsecond).
const FLAG_POLL_BOUND: u32 = 2_000_000;
const STATUS_POLL_BOUND: u32 = 400_000;

// ---------------- the parts ----------------

/// One register field of a part the bring-up sets before the four-line
/// read (the dummy-clock count; a quad-enable bit where the part has one;
/// a pin function turned off): the instructions that read the register's
/// bytes, the instruction that writes them back, and the field's place and
/// value in those bytes.
pub struct RegField {
    /// Whether the row carries the field (an unused slot carries none).
    pub used: bool,
    /// Up to two reads, each an instruction and the bytes it answers,
    /// whose bytes concatenate into the image the write takes; a pair of
    /// zeros is an unused slot.
    pub reads: [(u8, u8); 2],
    /// The instruction that writes the image and the bytes it takes.
    pub write: (u8, u8),
    /// The field: the byte of the image, the shift inside it, the mask
    /// after the shift and the value that selects the row's posture.
    pub byte: u8,
    pub shift: u8,
    pub mask: u8,
    pub code: u8,
}

/// One flash part the board's schematic names: its identification bytes,
/// the four-line read, the dummy clocks it takes at the posture the fields
/// select and how the controller fills them, the clock ceiling for that
/// read, the chip-select high time, the instructions around a register
/// write, the register fields set before the read, and the page and erase
/// sizes (the debug probe's programming units, a record). A row is read
/// from the part's datasheet or is a sentinel; the asserts below refuse a
/// half-filled row.
pub struct FlashPart {
    pub name: &'static str,
    pub maker: &'static str,
    /// The datasheet the row's values are read from.
    pub datasheet: &'static str,
    /// False while the row's values are sentinels.
    pub read: bool,
    pub id: [u8; 3],
    pub quad_read: u8,
    /// The dummy clocks the part expects between the address and the data
    /// at the posture the fields select, and how the controller fills them:
    /// `alt_bytes` alternate bytes (0 or 1) driven on four lines first (two
    /// clocks per byte, the value `alt_value`: a part whose first dummy
    /// clocks read a mode indicator is told "no mode" explicitly, the lines
    /// being undriven in the controller's dummy phase), then `dummy_cycles`
    /// clocks the controller counts with the lines undriven.
    pub dummy_total: u8,
    pub alt_bytes: u8,
    pub alt_value: u8,
    pub dummy_cycles: u8,
    /// The part's ceiling on the flash clock for the read at that count, in hertz.
    pub clk_ceiling_hz: u32,
    /// The part's clock-low-to-output-valid time at the heaviest load its
    /// datasheet tables, in picoseconds: the sample shift's derivation.
    pub clock_to_output_ps: u32,
    /// The chip-select high time the part needs between commands, in nanoseconds.
    pub cs_high_ns: u32,
    pub write_enable: u8,
    pub status_read: u8,
    pub status_busy_mask: u8,
    pub fields: [RegField; 3],
    #[allow(dead_code)] // a record of the row: a flash programmer's page unit
    pub page_bytes: u32,
    #[allow(dead_code)] // a record of the row: a flash programmer's erase unit
    pub erase_bytes: u32,
}

const NO_FIELD: RegField = RegField { used: false, reads: [(0, 0), (0, 0)], write: (0, 0), byte: 0, shift: 0, mask: 0, code: 0 };

/// The two parts the board's schematic names on one footprint (its flash
/// sheet, the pin names DQ3/HOLD# on the one and SIO3/RESET# on the other;
/// the user manual UM2033 5.14 says 512 Mbit): which one a board carries
/// is read from the part at boot.
pub const PARTS: [FlashPart; 2] = [
    // Micron MT25QL512ABB (the MT25Q 512 Mbit 3 V datasheet): Table 19 (the
    // identification 20h, BAh for 3 V, 20h for 512 Mb); Table 21 (4-BYTE QUAD
    // INPUT/OUTPUT FAST READ ECh, 1-4-4, ten dummy clocks as shipped, note 5;
    // READ VOLATILE CONFIGURATION REGISTER 85h and WRITE 81h; READ ENHANCED
    // VOLATILE CONFIGURATION REGISTER 65h and WRITE 61h, one byte each, the
    // WRITE ENABLE 06h first by note 8; READ STATUS REGISTER 05h); Tables 10
    // and 11 (the quad I/O fast read's clock at ten dummy clocks: 133 MHz for
    // the IT and AT grades, 108 MHz for the UT grade -- the lower taken, the
    // grade not on the schematic); Table 8 (the volatile configuration
    // register: bits 7:4 the dummy clocks, 1010 ten; bit 3 XIP, 1 disabled,
    // so no dummy clock carries a confirmation bit); Table 14 (the enhanced
    // volatile configuration register: bit 4 HOLD# or RESET# on DQ3, 0
    // disabled -- the line is data in the four-line phases); the status
    // register's bit 0 write in progress; the AC table 51 (tSHSL2, 50 ns of
    // deselect after a command that is not a read; tCLQV, the clock low to
    // output valid, 6 ns at 30 pF and 5 at 10 pF -- the heavier taken);
    // 256-byte pages, 64 KB sectors.
    FlashPart {
        name: "MT25QL512ABB1EW9",
        maker: "Micron",
        datasheet: "the MT25QL512ABB datasheet",
        read: true,
        id: [0x20, 0xBA, 0x20],
        quad_read: QUAD_READ_4B,
        dummy_total: 10,
        alt_bytes: 0,
        alt_value: 0,
        dummy_cycles: 10,
        clk_ceiling_hz: 108_000_000,
        clock_to_output_ps: 6_000,
        cs_high_ns: 50,
        write_enable: 0x06,
        status_read: 0x05,
        status_busy_mask: 0x01,
        fields: [
            RegField { used: true, reads: [(0x85, 1), (0, 0)], write: (0x81, 1), byte: 0, shift: 4, mask: 0x0F, code: 0b1010 },
            RegField { used: true, reads: [(0x85, 1), (0, 0)], write: (0x81, 1), byte: 0, shift: 3, mask: 0x01, code: 0x01 },
            RegField { used: true, reads: [(0x65, 1), (0, 0)], write: (0x61, 1), byte: 0, shift: 4, mask: 0x01, code: 0x00 },
        ],
        page_bytes: 256,
        erase_bytes: 65536,
    },
    // Macronix MX25L51245G (the 3 V 512 Mb datasheet, v1.8): Table 6 (RDID
    // 9Fh answers C2h, 20h, 1Ah); the four-byte-address command set (4READ4B
    // ECh, four address bytes); 9-18 and figure 40 (the four-line read: the
    // two clocks after the address carry the performance-enhance indicator
    // P[7:0], toggling nibbles enter that mode and 00h does not; six dummy
    // clocks at the default DC); Table 10 (the quad I/O fast read: six clocks
    // at DC 00 for 84 MHz, four at 01 for 70, eight at 10 for 104, ten at 11
    // for 133); Table 8 (the configuration register: bit 7 DC1, bit 6 DC0);
    // the status register (bit 6 QE, non-volatile, 0 as shipped: with it set
    // the quad commands are decoded and the WP#/SIO2 and RESET#/SIO3 pins are
    // data; bit 0 WIP); the register commands (WREN 06h; RDSR 05h; RDCR 15h;
    // WRSR 01h writing the status byte then the configuration byte, its
    // cycle tW 40 ms); the AC table 25 (CS# deselect 30 ns from a write to a
    // status read, the width RSTQIO needs too; tCLQV, the clock low to output
    // valid, 5 ns at 15 pF and at 10 pF at the default driver strength;
    // tCLQX, the output hold, 1 ns); 256-byte pages, 4 KB sectors, 64 KB
    // blocks. The dummy fields are the step of Table 10 at this posture's
    // clock (MACRONIX_LADDER).
    FlashPart {
        name: "MX25L51245GZ2I-08G",
        maker: "Macronix",
        datasheet: "the MX25L51245G datasheet",
        read: true,
        id: [0xC2, 0x20, 0x1A],
        quad_read: QUAD_READ_4B,
        dummy_total: MACRONIX_LADDER.0,
        alt_bytes: 1,
        alt_value: 0x00,
        dummy_cycles: MACRONIX_LADDER.1,
        clk_ceiling_hz: MACRONIX_LADDER.2,
        clock_to_output_ps: 5_000,
        cs_high_ns: 30,
        write_enable: 0x06,
        status_read: 0x05,
        status_busy_mask: 0x01,
        fields: [
            RegField { used: true, reads: [(0x05, 1), (0x15, 1)], write: (0x01, 2), byte: 0, shift: 6, mask: 0x01, code: 0x01 },
            RegField { used: true, reads: [(0x05, 1), (0x15, 1)], write: (0x01, 2), byte: 1, shift: 6, mask: 0x03, code: MACRONIX_LADDER.3 },
            NO_FIELD,
        ],
        page_bytes: 256,
        erase_bytes: 65536,
    },
];

/// The Macronix row's step of its Table 10 for the quad I/O fast read at
/// this posture's flash clock: the dummy clocks in total (the two of the
/// performance-enhance indicator among them), the clocks the controller
/// counts, the step's clock ceiling and its DC code (Table 8, bits 7:6,
/// volatile: written at every boot). The part's default step of six clocks
/// stands wherever its 84 MHz ceiling covers the clock (the 54 MHz opt-out,
/// the 72 MHz posture); above it the next step that does: eight clocks
/// (DC 10) to 104 MHz, ten (DC 11) to 133 -- the default posture's at 108.
/// The four-clock step (DC 01, 70 MHz) is not taken: the part's default
/// count is the floor.
const MACRONIX_LADDER: (u8, u8, u32, u8) = if CLK_HZ <= 84_000_000 { (6, 4, 84_000_000, 0b00) } else if CLK_HZ <= 104_000_000 { (8, 6, 104_000_000, 0b10) } else { (10, 8, 133_000_000, 0b11) };

/// Whether every row is read from its datasheet: the bring-up refuses
/// before the part is touched until then.
pub const PARTS_READ: bool = PARTS[0].read && PARTS[1].read;

// ---------------- the derived words ----------------

/// The window as the facts state it: the row's second flash range.
pub const WINDOW: &crate::ram::FlashFacts = &facts::FLASH_RANGES[1];
const _: () = assert!(
    facts::FLASH_RANGES.len() == 2 && !facts::FLASH_RANGES[0].external && facts::FLASH_RANGES[1].external,
    "the facts' flash ranges are the chip's flash and the quad-SPI window"
);
const _: () = assert!(WINDOW.base == 0x9000_0000 && WINDOW.mapped && WINDOW.executable, "the window is the block's memory-mapped range (RM0410 table 1), executable");
/// FSIZE (14.5.2): the part holds 2^(FSIZE+1) bytes; 0x19 for 64 MB. No
/// more than 256 MB can be addressed (14.3.7).
pub const FSIZE: u32 = WINDOW.bytes.trailing_zeros() - 1;
const _: () = assert!(WINDOW.bytes.is_power_of_two() && 1u64 << (FSIZE + 1) == WINDOW.bytes as u64 && FSIZE + 1 <= 28, "the window's bytes are a power of two the block can address");

/// The bus clock the block divides: HCLK, which equals SYSCLK at both of
/// the family's postures (the AHB prescaler is 1, board/stm32f7.rs).
pub const HCLK_HZ: u32 = crate::board::SYSCLK_HZ;
/// The flash clock the posture aims at, 108 MHz: the prescaler is the
/// division that lands at or under it (14.5.1: CLK is HCLK over PRESCALER
/// plus one), so the 216 MHz posture runs the bus at 108 MHz (prescaler 1,
/// the chip's ceiling) and the 16 MHz posture at 16. The opt-out
/// `st-stm32f769i-disco-qspi-54mhz` aims at 54 MHz (prescaler 3, the first
/// posture of record); the measurement posture `test-qspi-72mhz` (a board
/// flight, never a shipped image) at 72 (prescaler 2). The part rows' dummy
/// clocks and the sample shift follow the clock.
pub const CLK_TARGET_HZ: u32 = if cfg!(feature = "st-stm32f769i-disco-qspi-54mhz") { 54_000_000 } else if cfg!(feature = "test-qspi-72mhz") { 72_000_000 } else { 108_000_000 };
const _: () = assert!(!(cfg!(feature = "st-stm32f769i-disco-qspi-54mhz") && cfg!(feature = "test-qspi-72mhz")), "the flash clock's opt-out and its measurement posture are refused together");
pub const PRESCALER: u32 = HCLK_HZ.div_ceil(CLK_TARGET_HZ) - 1;
pub const CLK_HZ: u32 = HCLK_HZ / (PRESCALER + 1);
/// The chip's ceiling on the flash clock in single-data-rate mode
/// (DS11532 table 119: 108 MHz at 2.7 to 3.6 V and 20 pF).
pub const CHIP_CLK_CEILING_HZ: u32 = 108_000_000;
const _: () = assert!(PRESCALER <= 255, "the prescaler is eight bits");
const _: () = assert!(CLK_HZ <= CHIP_CLK_CEILING_HZ && CLK_HZ <= CLK_TARGET_HZ, "the flash clock exceeds the chip's ceiling or the posture's target");

/// The chip's input setup time on the data lines, in picoseconds (DS11532
/// table 119: 1.5 ns at 2.7 to 3.6 V; its input hold the same).
const CHIP_INPUT_SETUP_PS: u32 = 1_500;
/// The flash clock's period and half period, in picoseconds.
const PERIOD_PS: u32 = 1_000_000_000 / (CLK_HZ / 1_000);
const HALF_PERIOD_PS: u32 = PERIOD_PS / 2;
/// Whether the controller samples the data half a clock later than by
/// default (14.3.9 and SSHIFT, 14.5.1: the part drives the data on the
/// falling edge; the controller samples on the following rising edge by
/// default and, shifted, on the falling edge after it; AN4760 3.4.2): set
/// where a read row's clock-low-to-output time plus the chip's input setup
/// exceeds the half period, so the data would not be valid at the default
/// sampling edge -- at the default 108 MHz (a 4629 ps half period against
/// the Macronix row's 5000 plus 1500) and at 72 (6944 against the Micron
/// row's 7500; the Macronix row's 6500 fits, as the flight without the
/// shift read), clear at the opt-out's 54 (9259). With the shift the full
/// period must cover the sum, or
/// the posture refuses the build. The hold side with the shift (the
/// sampling edge is the edge the part changes its output after, holding
/// the old data its output hold time plus the clock's and data's path
/// delays, against the chip's 1.5 ns input hold) is tabled by neither
/// datasheet and is proven on the board by the identification read and the
/// two-path read-back. The shift is never set in double-data-rate mode
/// (14.5.1); this driver runs none.
const fn sample_shift_needed() -> bool {
    let mut i = 0;
    while i < PARTS.len() {
        if PARTS[i].read && PARTS[i].clock_to_output_ps + CHIP_INPUT_SETUP_PS > HALF_PERIOD_PS {
            return true;
        }
        i += 1;
    }
    false
}
const fn period_covers() -> bool {
    let mut i = 0;
    while i < PARTS.len() {
        if PARTS[i].read && PARTS[i].clock_to_output_ps + CHIP_INPUT_SETUP_PS > PERIOD_PS {
            return false;
        }
        i += 1;
    }
    true
}
pub const SAMPLE_SHIFT: bool = sample_shift_needed();
const _: () = assert!(period_covers(), "a read row's clock-to-output time plus the chip's input setup exceeds the flash clock's period: no sampling edge serves this posture");

/// Flash clocks that cover `ns` at `hz`, rounded up.
const fn ceil_cycles(ns: u32, hz: u32) -> u32 {
    ((ns as u64 * hz as u64).div_ceil(1_000_000_000)) as u32
}

/// The chip-select high time between commands, in flash clocks: the
/// longest the read rows need, at least one (14.5.2 CSHT holds it minus
/// one, 1 to 8 clocks).
const fn cs_high_cycles() -> u32 {
    let mut c = 1;
    let mut i = 0;
    while i < PARTS.len() {
        if PARTS[i].read {
            let n = ceil_cycles(PARTS[i].cs_high_ns, CLK_HZ);
            if n > c {
                c = n;
            }
        }
        i += 1;
    }
    c
}
pub const CSHT_CYCLES: u32 = cs_high_cycles();
const _: () = assert!(CSHT_CYCLES >= 1 && CSHT_CYCLES <= 8, "the chip-select high time is 1 to 8 flash clocks (14.5.2 CSHT)");
/// QUADSPI_DCR: FSIZE, CSHT, the clock low between commands (CKMODE 0).
pub const DCR_WORD: u32 = (FSIZE << 16) | ((CSHT_CYCLES - 1) << 8);
/// QUADSPI_CR without the enable: the prescaler and the sample shift the
/// posture derives; the timeout counter off, the threshold 0.
pub const CR_WORD: u32 = (PRESCALER << 24) | (if SAMPLE_SHIFT { CR_SSHIFT } else { 0 });

/// A row is whole (read from its datasheet, every fact nonzero, the dummy
/// clocks split between at most one alternate byte and the counted
/// clocks inside DCYC's five bits, the clock ceiling at or above this
/// posture's flash clock) or a sentinel (every fact zero, no field).
const fn row_sound(p: &FlashPart) -> bool {
    let whole = p.id[0] != 0
        && p.dummy_total != 0
        && p.alt_bytes <= 1
        && p.dummy_cycles != 0
        && p.dummy_cycles <= 31
        && p.dummy_total == 2 * p.alt_bytes + p.dummy_cycles
        && p.clk_ceiling_hz != 0
        && p.clock_to_output_ps != 0
        && p.cs_high_ns != 0
        && p.write_enable != 0
        && p.status_read != 0
        && p.status_busy_mask != 0
        && p.fields[0].used
        && p.page_bytes != 0
        && p.erase_bytes != 0;
    let sentinel = p.id[0] == 0
        && p.id[1] == 0
        && p.id[2] == 0
        && p.dummy_total == 0
        && p.alt_bytes == 0
        && p.alt_value == 0
        && p.dummy_cycles == 0
        && p.clk_ceiling_hz == 0
        && p.clock_to_output_ps == 0
        && p.cs_high_ns == 0
        && p.write_enable == 0
        && p.status_read == 0
        && p.status_busy_mask == 0
        && !p.fields[0].used
        && !p.fields[1].used
        && !p.fields[2].used
        && p.page_bytes == 0
        && p.erase_bytes == 0;
    (p.read && whole && CLK_HZ <= p.clk_ceiling_hz) || (!p.read && sentinel)
}
const _: () = assert!(row_sound(&PARTS[0]) && row_sound(&PARTS[1]), "a part row is half-filled, or a read row's clock ceiling lies under this posture's flash clock");

/// A used field reads as many bytes as it writes (one to four), names a
/// byte inside them and a value inside its mask.
const fn field_sound(f: &RegField) -> bool {
    if !f.used {
        return true;
    }
    let n = f.reads[0].1 + f.reads[1].1;
    n != 0 && n <= 4 && n == f.write.1 && f.byte < n && f.mask != 0 && f.code & !f.mask == 0 && f.shift <= 7 && f.write.0 != 0
}
const fn fields_sound(p: &FlashPart) -> bool {
    field_sound(&p.fields[0]) && field_sound(&p.fields[1]) && field_sound(&p.fields[2])
}
const _: () = assert!(fields_sound(&PARTS[0]) && fields_sound(&PARTS[1]), "a part row's register field is not a whole read-modify-write");
const _: () = assert!(!PARTS_READ || PARTS[0].id[0] != PARTS[1].id[0] || PARTS[0].id[1] != PARTS[1].id[1] || PARTS[0].id[2] != PARTS[1].id[2], "the two part rows answer the same identification");

/// No pin twice, every pin inside the line space, every function number a
/// nibble, and every pin a board claim (so no program opens one as a GPIO
/// line while the window is mapped).
const fn pins_sound(p: &[(u32, u8); 6]) -> bool {
    let mut i = 0;
    while i < p.len() {
        if p[i].0 as usize >= facts::LINE_COUNT || p[i].1 > 15 {
            return false;
        }
        let mut claimed = false;
        let mut k = 0;
        while k < facts::BOARD_CLAIMS.len() {
            if facts::BOARD_CLAIMS[k] as u32 == p[i].0 {
                claimed = true;
            }
            k += 1;
        }
        if !claimed {
            return false;
        }
        let mut j = i + 1;
        while j < p.len() {
            if p[i].0 == p[j].0 {
                return false;
            }
            j += 1;
        }
        i += 1;
    }
    true
}
const _: () = assert!(pins_sound(&facts::QSPI.pins), "a flash pin is listed twice, lies outside the line space, or is not a board claim");

/// The doctored controls (board flights, never a shipped image): the part
/// table's manufacturer bytes made impossible, so the identification read
/// must refuse; the frame both read paths share programmed with one dummy
/// clock less than the part's count, so the window's words read shifted on
/// both paths and the header's magic must refuse; the memory-mapped frame
/// alone one clock less, so the two-path read-back must mismatch; the
/// image's own matched-set words shifted by a byte count, so the window
/// header's comparison must refuse.
const ID_DOCTOR: u8 = if cfg!(feature = "test-qspi-wrong-id") { 0x80 } else { 0 };
const DUMMY_DOCTOR: u32 = if cfg!(feature = "test-qspi-dummy") { 1 } else { 0 };
const MMAP_DUMMY_DOCTOR: u32 = if cfg!(feature = "test-qspi-mmap-dummy") { 1 } else { 0 };
const HEADER_DOCTOR: u32 = if cfg!(feature = "test-xip-header") { 16 } else { 0 };

// ---------------- the map's words ----------------

unsafe extern "C" {
    static __rhu_xip_lo: u32;
    static __rhu_xip_hi: u32;
    static __rhu_managedcode_lo: u32;
    static __rhu_managedcode_hi: u32;
    static __rhu_frozen_lo: u32;
}

/// The window's brackets as the image's map states them: both zero where
/// the map declares no window; the first byte past the window header and
/// the end of the window's sections where it does (equal, for an image
/// that carries the header alone).
pub fn brackets() -> (usize, usize) {
    // The pair reads pass through black_box, as the module table's own
    // bracket reads do: the compiler takes a sized extern static's address
    // as never null, so the bare address compared with zero folds to true
    // at compile time and a map that declares no window would be brought
    // up and refused at the bracket check.
    (core::hint::black_box(addr_of!(__rhu_xip_lo) as usize), core::hint::black_box(addr_of!(__rhu_xip_hi) as usize))
}

/// Whether the image's map declares the window.
pub fn declared() -> bool {
    brackets().0 != 0
}

/// The five matched-set words the image's map states: the window
/// brackets, the managed-code brackets and the frozen bracket's start.
pub fn matched_set_words() -> [u32; 5] {
    [
        addr_of!(__rhu_xip_lo) as u32,
        addr_of!(__rhu_xip_hi) as u32,
        addr_of!(__rhu_managedcode_lo) as u32,
        addr_of!(__rhu_managedcode_hi) as u32,
        addr_of!(__rhu_frozen_lo) as u32,
    ]
}

/// The window header, 64 bytes at the window's base, emitted by every map
/// that declares the window: the magic ("RHUX" in a byte dump), the
/// header's layout version 1, its byte count, the five matched-set words
/// and eight zero words.
pub const WINDOW_HEADER_MAGIC: u32 = 0x5855_4852;
pub const WINDOW_HEADER_LAYOUT: u32 = 1;
pub const WINDOW_HEADER_BYTES: u32 = 64;

// ---------------- the bring-up ----------------

/// What the bring-up read and measured: the boot line and the api
/// flavor's window case read it.
#[derive(Clone, Copy)]
pub struct Window {
    pub part: &'static FlashPart,
    pub id: [u8; 3],
    pub clk_hz: u32,
    /// The window's sections' bytes past the header (zero for an image
    /// that carries the header alone).
    pub image_bytes: u32,
    /// The header's eight words as the boot read them through the window.
    #[allow(dead_code)] // the api flavor's window case reads it; a record under the other features
    pub header: [u32; 8],
    /// The cycle counter over the identification, the register fields and
    /// the indirect read; and over the mode switch, the read-back and the
    /// header check.
    #[allow(dead_code)] // the api flavor's window case reads it; a record under the other features
    pub cycles_indirect: u32,
    #[allow(dead_code)] // the api flavor's window case reads it; a record under the other features
    pub cycles_window: u32,
}

static mut RECORD: Option<Window> = None;

/// The bring-up's record, None under a map that declares no window.
pub fn record() -> Option<&'static Window> {
    unsafe { (*addr_of!(RECORD)).as_ref() }
}

/// The core's cycle counter (the Data Watchpoint and Trace unit), armed by
/// the chip module on every call: the bring-up's own cost and the api
/// flavor's timed reads.
pub fn cycles() -> u32 {
    const DWT_CYCCNT: *const u32 = 0xE000_1004 as *const u32;
    super::stm32f7::arm_cycle_counter();
    unsafe { read_volatile(DWT_CYCCNT) }
}

/// The boot step: nothing under a map that declares no window (the block
/// stays disabled, so a program that touches the window faults as before);
/// the bring-up and its record otherwise.
pub fn boot() -> Option<&'static Window> {
    if !declared() {
        return None;
    }
    let w = bring_up();
    unsafe { *addr_of_mut!(RECORD) = Some(w) };
    record()
}

/// The block's registers.
struct Block(usize);

impl Block {
    unsafe fn rd(&self, off: usize) -> u32 {
        unsafe { read_volatile((self.0 + off) as *const u32) }
    }
    unsafe fn wr(&self, off: usize, v: u32) {
        unsafe { write_volatile((self.0 + off) as *mut u32, v) }
    }
    /// A status flag awaited at a level under the bound; a flag that never
    /// arrives is a loud death naming the step.
    unsafe fn wait(&self, reg: usize, mask: u32, set: bool, step: &'static str) {
        let mut polls = 0u32;
        unsafe {
            while ((self.rd(reg) & mask) != 0) != set {
                polls += 1;
                if polls > FLAG_POLL_BOUND {
                    panic!(
                        "qspi: the controller's status stayed {:#010x} during {} (the flag {:#04x} of the register at {:#04x} never {})",
                        self.rd(QSPI_SR),
                        step,
                        mask,
                        reg,
                        if set { "set" } else { "cleared" }
                    );
                }
            }
        }
    }
    /// A command's end: the controller idle, no transfer error, the flags
    /// cleared.
    unsafe fn finish(&self, step: &'static str) {
        unsafe {
            self.wait(QSPI_SR, SR_BUSY, false, step);
            if self.rd(QSPI_SR) & SR_TEF != 0 {
                panic!("qspi: a transfer error during {}", step);
            }
            self.wr(QSPI_FCR, FCR_CLEAR);
        }
    }
    /// An instruction with no address and no data: sent at the write of
    /// the communication word (14.3.4).
    unsafe fn send(&self, word: u32, step: &'static str) {
        unsafe {
            self.wr(QSPI_CCR, word);
            self.finish(step);
        }
    }
    /// An indirect read: the length, the word, the address where the frame
    /// carries one (the read starts at its write), the bytes taken from the
    /// data register as the FIFO flag announces each.
    unsafe fn read(&self, word: u32, addr: Option<u32>, buf: &mut [u8], step: &'static str) {
        unsafe {
            self.wr(QSPI_DLR, buf.len() as u32 - 1);
            self.wr(QSPI_CCR, word);
            if let Some(a) = addr {
                self.wr(QSPI_AR, a);
            }
            for b in buf.iter_mut() {
                self.wait(QSPI_SR, SR_FTF, true, step);
                *b = read_volatile((self.0 + QSPI_DR) as *const u8);
            }
            self.finish(step);
        }
    }
    /// An indirect write with no address: the length, the word, the bytes
    /// given to the data register as the FIFO flag announces room.
    unsafe fn write(&self, word: u32, buf: &[u8], step: &'static str) {
        unsafe {
            self.wr(QSPI_DLR, buf.len() as u32 - 1);
            self.wr(QSPI_CCR, word);
            for b in buf {
                self.wait(QSPI_SR, SR_FTF, true, step);
                write_volatile((self.0 + QSPI_DR) as *mut u8, *b);
            }
            self.finish(step);
        }
    }
    /// An abort (14.3.14): ends any operation in any mode, clears BUSY and
    /// flushes the FIFO; the bit clears itself when done.
    unsafe fn abort(&self, step: &'static str) {
        unsafe {
            self.wr(QSPI_CR, self.rd(QSPI_CR) | CR_ABORT);
            self.wait(QSPI_CR, CR_ABORT, false, step);
            self.wait(QSPI_SR, SR_BUSY, false, step);
            self.wr(QSPI_FCR, FCR_CLEAR);
        }
    }
}

/// One pin to the block's alternate function: push-pull, no pull, very
/// high speed, the function's nibble in the low register for pins 0 to 7
/// and the high one for 8 to 15.
unsafe fn route_pin(line: u32, af: u8) {
    unsafe {
        let base = GPIO_BASE + (line / 16) as usize * GPIO_PORT_STRIDE;
        let pin = line % 16;
        let moder = (base + GPIO_MODER) as *mut u32;
        write_volatile(moder, (read_volatile(moder) & !(0b11 << (2 * pin))) | (0b10 << (2 * pin)));
        let otyper = (base + GPIO_OTYPER) as *mut u32;
        write_volatile(otyper, read_volatile(otyper) & !(1 << pin));
        let ospeedr = (base + GPIO_OSPEEDR) as *mut u32;
        write_volatile(ospeedr, read_volatile(ospeedr) | (0b11 << (2 * pin)));
        let pupdr = (base + GPIO_PUPDR) as *mut u32;
        write_volatile(pupdr, read_volatile(pupdr) & !(0b11 << (2 * pin)));
        let afr = (base + if pin < 8 { GPIO_AFRL } else { GPIO_AFRH }) as *mut u32;
        let shift = 4 * (pin % 8);
        write_volatile(afr, (read_volatile(afr) & !(0xF << shift)) | ((af as u32) << shift));
    }
}

/// A register field's image read by the row's instructions, one line each.
unsafe fn read_image(b: &Block, f: &RegField, img: &mut [u8; 4]) -> usize {
    let mut n = 0usize;
    for (ins, bytes) in f.reads {
        if bytes == 0 {
            continue;
        }
        unsafe { b.read(ccr(FMODE_IREAD, LINES_ONE, ins, LINES_NONE, LINES_NONE, 0, LINES_ONE), None, &mut img[n..n + bytes as usize], "a register read") };
        n += bytes as usize;
    }
    n
}

/// One register field set to the row's value: read, left alone when it
/// already reads so (a non-volatile register is not rewritten), else
/// write-enabled, written whole, the part's busy bit polled clear, and read
/// back equal -- a disagreement is a loud death before the window is
/// mapped.
unsafe fn set_field(b: &Block, part: &FlashPart, f: &RegField) {
    let at = |img: &[u8; 4]| (img[f.byte as usize] >> f.shift) & f.mask;
    let mut img = [0u8; 4];
    let n = unsafe { read_image(b, f, &mut img) };
    if at(&img) == f.code {
        return;
    }
    img[f.byte as usize] = (img[f.byte as usize] & !(f.mask << f.shift)) | (f.code << f.shift);
    unsafe {
        b.send(ccr(FMODE_IWRITE, LINES_ONE, part.write_enable, LINES_NONE, LINES_NONE, 0, LINES_NONE), "the write enable before a register write");
        b.write(ccr(FMODE_IWRITE, LINES_ONE, f.write.0, LINES_NONE, LINES_NONE, 0, LINES_ONE), &img[..n], "a register write");
        let mut polls = 0u32;
        loop {
            let mut s = [0u8; 1];
            b.read(ccr(FMODE_IREAD, LINES_ONE, part.status_read, LINES_NONE, LINES_NONE, 0, LINES_ONE), None, &mut s, "the status read after a register write");
            if s[0] & part.status_busy_mask == 0 {
                break;
            }
            polls += 1;
            if polls > STATUS_POLL_BOUND {
                panic!("qspi: the part's status register stayed busy ({:#04x}) after a register write", s[0]);
            }
        }
        let mut back = [0u8; 4];
        read_image(b, f, &mut back);
        if at(&back) != f.code {
            panic!("qspi: a register field reads {:#04x} after the write where {:#04x} was written (the register behind instruction {:#04x}): refusing before the window is mapped", at(&back), f.code, f.write.0);
        }
    }
}

/// The four-line read's communication word for a part: the instruction on
/// one line, the address on four, the alternate byte on four where the row
/// has one, the counted dummy clocks (one less under the dummy control;
/// one less in the memory-mapped frame alone under the second), the data
/// on four.
fn frame(part: &FlashPart, fmode: u32) -> u32 {
    let abmode = if part.alt_bytes != 0 { LINES_FOUR } else { LINES_NONE };
    let mmap_doctor = if fmode == FMODE_MMAP { MMAP_DUMMY_DOCTOR } else { 0 };
    ccr(fmode, LINES_ONE, part.quad_read, LINES_FOUR, abmode, part.dummy_cycles as u32 - DUMMY_DOCTOR - mmap_doctor, LINES_FOUR)
}

/// The bring-up in the order above; every refusal a loud death naming the
/// step, before any payload address is touched.
fn bring_up() -> Window {
    if !PARTS_READ {
        panic!(
            "qspi: the part table's rows are sentinels -- {} and {} have not been read into them: the bring-up refuses before the part is touched",
            PARTS[0].datasheet, PARTS[1].datasheet
        );
    }
    let base = WINDOW.base as usize;
    let (lo, hi) = brackets();
    assert!(
        lo == base + WINDOW_HEADER_BYTES as usize && hi >= lo && hi <= base + WINDOW.bytes as usize,
        "qspi: the map's window brackets [{:#010x}, {:#010x}) do not start past the header at {:#010x} inside the window",
        lo,
        hi,
        base
    );
    let b = Block(facts::QSPI.base);
    unsafe {
        // The ports' and the block's clocks, then the six pins.
        let mut ports = 0u32;
        for (line, _) in facts::QSPI.pins {
            ports |= 1 << (line / 16);
        }
        write_volatile(RCC_AHB1ENR, read_volatile(RCC_AHB1ENR) | ports);
        let _ = read_volatile(RCC_AHB1ENR);
        let enable = facts::QSPI.enable_reg as *mut u32;
        write_volatile(enable, read_volatile(enable) | (1 << facts::QSPI.enable_bit));
        let _ = read_volatile(enable);
        for (line, af) in facts::QSPI.pins {
            route_pin(line, af);
        }
        // The controller: a previous session's operation ended and the
        // block disabled while its words are written (14.5.1: the fields
        // are modified with BUSY clear), then the device word and the
        // control word with the enable.
        if b.rd(QSPI_CR) & CR_EN != 0 {
            b.abort("the abort of a previous session's operation");
            b.wr(QSPI_CR, 0);
        }
        b.wr(QSPI_DCR, DCR_WORD);
        b.wr(QSPI_CR, CR_WORD | CR_EN);
        let t0 = cycles();
        b.send(ccr(FMODE_IWRITE, LINES_FOUR, EXIT_QUAD_COMMAND_MODE, LINES_NONE, LINES_NONE, 0, LINES_NONE), "the exit from four-line command mode");
        // The identification, on one line.
        let mut id = [0u8; 3];
        b.read(ccr(FMODE_IREAD, LINES_ONE, READ_ID, LINES_NONE, LINES_NONE, 0, LINES_ONE), None, &mut id, "the identification read");
        if id == [0, 0, 0] {
            panic!("qspi: the part answered 00 00 00 to the identification read (the instruction never reached it: the pins, the clock or the chip select)");
        }
        if id == [0xFF, 0xFF, 0xFF] {
            panic!("qspi: the part answered ff ff ff to the identification read (the data line floated: no part answered)");
        }
        let mut part = None;
        for p in PARTS.iter() {
            if p.read && [p.id[0] ^ ID_DOCTOR, p.id[1], p.id[2]] == id {
                part = Some(p);
            }
        }
        let Some(part) = part else {
            panic!(
                "qspi: the part's identification {:02x} {:02x} {:02x} matches neither {} ({}) nor {} ({}): refusing before the window is mapped",
                id[0], id[1], id[2], PARTS[0].name, PARTS[0].maker, PARTS[1].name, PARTS[1].maker
            );
        };
        // The part's register fields for the four-line read, written and
        // read back.
        for f in part.fields.iter() {
            if f.used {
                set_field(&b, part, f);
            }
        }
        // The window's first 256 bytes by the controller's indirect mode,
        // the alternate byte in place where the row has one.
        b.wr(QSPI_ABR, part.alt_value as u32);
        let mut first = [0u8; 256];
        b.read(frame(part, FMODE_IREAD), Some(0), &mut first, "the indirect read of the window's first bytes");
        let t1 = cycles();
        // ES0334 2.4.2: before the memory-mapped mode after an indirect
        // read, the address register cleared, then an abort to stop the
        // read that starts and to clear BUSY. Then the memory-mapped mode
        // (14.3.11: reads alone; the timeout counter off, so the chip
        // select stays low and BUSY stays set after the first access).
        b.wr(QSPI_AR, 0);
        b.abort("the errata workaround's abort before the memory-mapped mode");
        b.wr(QSPI_CCR, frame(part, FMODE_MMAP));
        asm!("dsb", "isb", options(nostack, preserves_flags));
        // The two-path read-back: the same bytes through the window.
        for i in 0..64 {
            let got = read_volatile((base + 4 * i) as *const u32);
            let want = u32::from_le_bytes([first[4 * i], first[4 * i + 1], first[4 * i + 2], first[4 * i + 3]]);
            if got != want {
                panic!(
                    "qspi: read-back mismatch at {:#010x}: the controller's indirect read gave {:#010x}, the window reads {:#010x} (the four-line frame's dummy clocks or its sampling): refusing before any payload address is touched",
                    base + 4 * i,
                    want,
                    got
                );
            }
        }
        // The window header: this image's own half, or a loud refusal.
        let mut header = [0u32; 8];
        for (k, h) in header.iter_mut().enumerate() {
            *h = read_volatile((base + 4 * k) as *const u32);
        }
        if header[0] != WINDOW_HEADER_MAGIC {
            panic!(
                "qspi: no image header in the window at {:#010x} (its first word reads {:#010x}, the header's magic is {:#010x}): the window half of this image is not programmed",
                base, header[0], WINDOW_HEADER_MAGIC
            );
        }
        if header[1] != WINDOW_HEADER_LAYOUT || header[2] != WINDOW_HEADER_BYTES {
            panic!("qspi: the window header's layout {} of {} bytes is not layout {} of {} bytes", header[1], header[2], WINDOW_HEADER_LAYOUT, WINDOW_HEADER_BYTES);
        }
        let mine = matched_set_words();
        for (k, m) in mine.iter().enumerate() {
            let want = m.wrapping_add(HEADER_DOCTOR);
            if header[3 + k] != want {
                panic!(
                    "qspi: the window's image is not this image's: matched-set word {} reads {:#010x} in the window where this image says {:#010x} -- reflash both halves",
                    k,
                    header[3 + k],
                    want
                );
            }
        }
        let t2 = cycles();
        Window { part, id, clk_hz: CLK_HZ, image_bytes: (hi - lo) as u32, header, cycles_indirect: t1.wrapping_sub(t0), cycles_window: t2.wrapping_sub(t1) }
    }
}

/// The boot line's words after "[boot] flash: ".
pub struct Banner(pub Option<&'static Window>);

impl fmt::Display for Banner {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let a = &facts::FLASH_RANGES[0];
        let q = WINDOW;
        match self.0 {
            Some(w) => write!(
                f,
                "2 ranges -- {} {} MB at {:#010x}; {} {} MB at {:#010x} (the quad-SPI window: {} {}, id {:02x} {:02x} {:02x}, {} MHz, {} dummy clocks; read-back held; the window's image {} bytes past its header)",
                a.name,
                a.bytes >> 20,
                a.base,
                q.name,
                q.bytes >> 20,
                q.base,
                w.part.maker,
                w.part.name,
                w.id[0],
                w.id[1],
                w.id[2],
                w.clk_hz / 1_000_000,
                w.part.dummy_total,
                w.image_bytes
            ),
            None => write!(
                f,
                "1 range -- {} {} MB at {:#010x} (the quad-SPI window at {:#010x} is not declared by this image's map: the block stays disabled)",
                a.name,
                a.bytes >> 20,
                a.base,
                q.base
            ),
        }
    }
}
