//! The kernel's bus API's pure arithmetic and facts shapes: the block descriptors a
//! board's facts state per bus, the I2C message record as the header lays
//! it out, the partition of a transaction's arguments into served and
//! refused, the I2C timing word, the SPI divisor ladder and the wire-wait
//! budget in ticks. No hardware access and no state, covered by the host tests; the drivers cite the manuals for the
//! numbers here.
#![allow(dead_code)] // a row without a bus, and the host tier, compile the module unused in part

/// What a pad does for its bus. A board's facts state it per pad, and the
/// silicon backend derives the pad's electrical setup from it: the two I2C
/// roles are open-drain with the pad's own pull-up, the SPI clock and output
/// push-pull at the high speed setting, the SPI input pulled up so a bus
/// with no device answering reads ones.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PadRole {
    I2cClock,
    I2cData,
    SpiClock,
    SpiIn,
    SpiOut,
}

/// A pad a bus takes: the GPIO line, the alternate function it runs at and
/// its role on the bus.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Pad {
    pub line: u16,
    pub alternate: u8,
    pub role: PadRole,
}

/// An I2C bus as a board's facts state it: its NUMBER -- the chip's own
/// block number (I2C1 is bus 1), the name a program gives the functions, as a
/// Linux device node is named -- the block's base, its clock enable (a
/// register and a bit), its kernel-clock selection (a register, a shift and
/// the value that selects the fixed source: 16 MHz on the F7 and the U5,
/// the 64 MHz internal kernel clock on the H7), the block's kernel
/// clock in Hz, and the two pads with their alternate function (an empty
/// list on a row whose model has no pins). The table position is the
/// enumeration index the count and bus functions walk.
#[derive(Clone, Copy, Debug)]
pub struct I2cBus {
    pub number: u32,
    pub base: usize,
    pub enable_reg: usize,
    pub enable_bit: u32,
    pub mux_reg: usize,
    pub mux_shift: u32,
    pub mux_value: u32,
    pub kernel_hz: u32,
    pub pads: &'static [Pad],
}

/// The SPI register file a block carries: the STM32F7's sixteen-bit file
/// with the four-byte queues, the STM32U5's file with the sixteen-byte queues
/// and the transfer-size counter, or the RP2350's PL022 with eight-entry queues.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpiFile {
    F7,
    U5,
    Pl022,
}

/// An SPI bus as a board's facts state it: its NUMBER (the chip's own block
/// number: SPI2 is bus 2, the name the functions take, as a Linux device node is
/// named), the block's base and file, its clock enable, its kernel-clock
/// selection (a register of 0 where the block follows a bus clock), the
/// block's kernel clock in Hz at this posture, the three pads, the chip-select
/// lines the bus offers (the list empty on a row whose model has no pins;
/// `select_count` the number the function accepts), and the pump's bound -- how
/// many bytes may be sent ahead of the bytes received, half the receive queue.
#[derive(Clone, Copy, Debug)]
pub struct SpiBus {
    pub number: u32,
    pub base: usize,
    pub file: SpiFile,
    pub enable_reg: usize,
    pub enable_bit: u32,
    pub mux_reg: usize,
    pub mux_shift: u32,
    pub mux_value: u32,
    pub kernel_hz: u32,
    pub pads: &'static [Pad],
    pub selects: &'static [u16],
    pub select_count: u32,
    pub ahead: u32,
}

/// The I2C message record as the header lays it out: the address, the
/// flags (bit 0 the read flag), the length, a reserved word that must read
/// 0, the buffer. Twelve bytes on the target, 4-aligned.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct I2cMsg {
    pub addr: u16,
    pub flags: u16,
    pub len: u16,
    pub reserved: u16,
    pub buf: *mut u8,
}

/// The constants the header states.
pub const MSG_READ: u16 = 1;
pub const MSG_MAX: usize = 8;
pub const LEN_MAX: u32 = 255;
pub const SPI_LEN_MAX: u32 = 65535;
pub const SELECT_NONE: u32 = u32::MAX;
pub const SPI_MODE_MAX: u32 = 3;
/// The code this family adds to the hosting API's set (`include/rhu.h`).
pub const ERR_NXIO: i32 = -6;
const ERR_INVAL: i32 = -22;

/// The bits a byte takes on each wire: nine on I2C (the acknowledge), eight on SPI.
pub const I2C_BITS_PER_BYTE: u32 = 9;
pub const SPI_BITS_PER_BYTE: u32 = 8;

/// The table position of the bus a number names, or None when the board
/// has no such bus (the function then answers RHU_ENODEV).
pub fn i2c_index(table: &[I2cBus], number: u32) -> Option<usize> {
    table.iter().position(|b| b.number == number)
}

pub fn spi_index(table: &[SpiBus], number: u32) -> Option<usize> {
    table.iter().position(|b| b.number == number)
}

/// The invariants a growing table can break, asserted at compile time where
/// the tables are consumed: no two buses of a family carry one number (a
/// number names one bus); a bus offers at most eight chip selects (the
/// claimed selects are a byte of bits), and its select list is empty (a
/// model with no pins) or exactly as long as the count it states.
pub const fn i2c_numbers_unique(table: &[I2cBus]) -> bool {
    let mut i = 0;
    while i < table.len() {
        let mut j = i + 1;
        while j < table.len() {
            if table[i].number == table[j].number {
                return false;
            }
            j += 1;
        }
        i += 1;
    }
    true
}

pub const fn spi_numbers_unique(table: &[SpiBus]) -> bool {
    let mut i = 0;
    while i < table.len() {
        let mut j = i + 1;
        while j < table.len() {
            if table[i].number == table[j].number {
                return false;
            }
            j += 1;
        }
        i += 1;
    }
    true
}

pub const fn spi_selects_sound(table: &[SpiBus]) -> bool {
    let mut i = 0;
    while i < table.len() {
        let b = &table[i];
        if b.select_count > 8 || !(b.selects.is_empty() || b.selects.len() == b.select_count as usize) {
            return false;
        }
        i += 1;
    }
    true
}

/// The partition of a message list: 0 when every record is served; the
/// invalid code for a count of 0 or over the maximum, a flag other than
/// the read flag, a reserved word not 0, a length over the maximum, an
/// address over 0x7F, or a NULL buffer with a length.
pub fn i2c_msgs_check(msgs: &[I2cMsg]) -> i32 {
    if msgs.is_empty() || msgs.len() > MSG_MAX {
        return ERR_INVAL;
    }
    for m in msgs {
        if m.flags & !MSG_READ != 0 || m.reserved != 0 || m.len as u32 > LEN_MAX || m.addr > 0x7F {
            return ERR_INVAL;
        }
        if m.buf.is_null() && m.len != 0 {
            return ERR_INVAL;
        }
    }
    0
}

/// The bytes a transaction puts on the wire: every message's bytes plus
/// one address byte per message.
pub fn i2c_wire_bytes(msgs: &[I2cMsg]) -> u32 {
    let mut n = 0u32;
    for m in msgs {
        n += m.len as u32 + 1;
    }
    n
}

/// The I2C timing word for a rate at a kernel clock; a pair with no word is
/// not served. At 16 MHz and at 54 MHz, in every mode: the fastest word
/// that keeps the rule `i2c_timing_sound` states. Standard mode at the
/// STM32H7's 64 MHz internal kernel clock: the manuals' example row for
/// 16 MHz (RM0399's Table 409) with the prescaler 15, which its formula
/// tPRESC = (PRESC + 1) x tI2CCLK makes the same 250 ns tick at 64 MHz --
/// the register reading of the power-management chip.
pub const fn i2c_timing(kernel_hz: u32, rate_hz: u32) -> Option<u32> {
    match (kernel_hz, rate_hz) {
        (16_000_000, 100_000) => Some(0x10A1_2828),
        (16_000_000, 400_000) => Some(0x0062_0C17),
        (16_000_000, 1_000_000) => Some(0x0020_0408),
        (54_000_000, 1_000_000) => Some(0x0091_101E),
        (64_000_000, 100_000) => Some(0xF042_0F13),
        _ => None,
    }
}

/// The SPI divisor ladder: eight steps, the kernel clock over 2^(n + 1).
/// The smallest divisor whose rate is at or below the request wins, and the
/// rate it achieves is the answer; None when even the largest divisor's
/// rate is above the request (or the request is 0).
pub const fn spi_divisor(kernel_hz: u32, rate_hz: u32) -> Option<(u32, u32)> {
    let mut n = 0u32;
    while n < 8 {
        let achieved = kernel_hz >> (n + 1);
        if achieved <= rate_hz {
            return Some((n, achieved));
        }
        n += 1;
    }
    None
}

/// A wire wait's budget in ticks: twice the wire time plus two ticks, never
/// fewer than three -- the wire time the bytes awaited times the bits per
/// byte at the rate, rounded UP to whole ticks before it is doubled (the
/// drivers wait a byte at a time: four ticks at every rate they serve).
pub const fn budget_ticks(bytes: u32, bits_per_byte: u32, rate_hz: u32, tick_hz: u32) -> u32 {
    let bits = bytes as u64 * bits_per_byte as u64 * tick_hz as u64;
    let wire = bits.div_ceil(rate_hz as u64) as u32;
    let b = 2 * wire + 2;
    if b < 3 {
        3
    } else {
        b
    }
}

/// The mode's clock polarity and phase bits, or None past mode 3.
pub const fn mode_bits(mode: u32) -> Option<(bool, bool)> {
    if mode > SPI_MODE_MAX {
        None
    } else {
        Some((mode & 2 != 0, mode & 1 != 0))
    }
}

/// A transfer's chip-select argument: one of the bus's selects, the
/// no-select value (the program drives its own through the kernel's GPIO API), or invalid.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SelectClass {
    Index(u32),
    None,
    Invalid,
}

pub const fn select_class(select: u32, select_count: u32) -> SelectClass {
    if select == SELECT_NONE {
        SelectClass::None
    } else if select < select_count {
        SelectClass::Index(select)
    } else {
        SelectClass::Invalid
    }
}

/// The SMBus specification's clock-low limits in milliseconds (its timing
/// table's row tTIMEOUT: 25 at the least, 35 at the most): a device may hold
/// the clock low up to the first in a byte and is still served; the recovery
/// after a timeout holds the clock low for the second, once.
pub const I2C_CLOCK_LOW_MS: u32 = 25;
pub const I2C_CLOCK_HOLD_MS: u32 = 35;

/// The I2C rates a claim may name, in the order of the modes the bus
/// specification defines: standard, fast, fast mode plus. A row serves the
/// ones its block makes from its clock within the mode's rules (its family
/// driver's `timing` answers for a rate); the claims table stores a claimed
/// bus's mode as its index here plus one.
pub const I2C_RATES_HZ: [u32; 3] = [100_000, 400_000, 1_000_000];

/// A rate's place in the set, or None for a rate outside it.
pub const fn i2c_mode_index(rate_hz: u32) -> Option<u8> {
    let mut i = 0;
    while i < I2C_RATES_HZ.len() {
        if I2C_RATES_HZ[i] == rate_hz {
            return Some(i as u8);
        }
        i += 1;
    }
    None
}

/// An I2C wire wait's budget in ticks: the wire time of two bytes at the rate
/// (an address and the byte after it ride one wait), rounded UP to whole
/// ticks, plus the clock-low limit in ticks, plus one tick for the tick's
/// phase -- 27 at 100 kHz and a tick of a millisecond. A wait never fails
/// while its step arrives within the wire time and the limit; it fails within
/// one tick more. The SPI drivers keep `budget_ticks`.
pub const fn i2c_wait_ticks(rate_hz: u32, tick_hz: u32) -> u32 {
    let wire = (2 * I2C_BITS_PER_BYTE as u64 * tick_hz as u64).div_ceil(rate_hz as u64) as u32;
    let low = (I2C_CLOCK_LOW_MS as u64 * tick_hz as u64).div_ceil(1_000) as u32;
    wire + low + 1
}

/// A mode's times in the bus specification, in nanoseconds: the clock's
/// least low and high periods, the least data setup, the slowest rise and
/// fall (the manuals' table of the specification's setup and hold times,
/// RM0410 Table 212 and RM0456 Table 660, with the specification's two
/// clock periods; the least data hold is 0 in every mode).
struct I2cModeTimes {
    low_ns: u64,
    high_ns: u64,
    setup_ns: u64,
    rise_ns: u64,
    fall_ns: u64,
}

/// In the order of `I2C_RATES_HZ`.
const I2C_MODE_TIMES: [I2cModeTimes; 3] = [
    I2cModeTimes { low_ns: 4_700, high_ns: 4_000, setup_ns: 250, rise_ns: 1_000, fall_ns: 300 },
    I2cModeTimes { low_ns: 1_300, high_ns: 600, setup_ns: 100, rise_ns: 300, fall_ns: 300 },
    I2cModeTimes { low_ns: 500, high_ns: 260, setup_ns: 50, rise_ns: 120, fall_ns: 120 },
];

/// The widest figures the data sheets of the chips with this block state:
/// the kernel clock at its fastest, in thousandths of its nominal rate (an
/// internal oscillator's accuracy over the whole temperature range), and the
/// analog filter's least and most delay in nanoseconds.
const I2C_CLOCK_FASTEST_PER_MILLE: u64 = 1_045;
const I2C_FILTER_MIN_NS: u64 = 50;
const I2C_FILTER_MAX_NS: u64 = 115;

/// Whether `periods` of a clock last at least `ns` while the clock runs at
/// its fastest (`fastest` is its rate in thousandths of a hertz).
#[inline]
const fn i2c_periods_cover(periods: u64, ns: u64, fastest: u64) -> bool {
    periods * 1_000_000_000_000 >= ns * fastest
}

/// The rule a timing word of the 16 MHz and the 54 MHz clock keeps, with the
/// analog filter on and the digital filter off, every time taken at the
/// kernel clock's fastest (RM0410 33.4.3, 33.4.5, 33.4.9 and 33.9.5; RM0456
/// 65.4.3, 65.4.5, 65.4.9 and 65.9.5). The counted low and the counted high
/// are each at least the mode's least period -- the block makes the
/// bus-free time and the start's hold from the same counts -- and so are
/// the wire's low and high: the count, plus the filter's least delay and
/// two clock periods of synchronization, less the mode's slowest edge. The
/// two counts with the least synchronization (four clock periods and twice
/// the filter's least delay) are at least the mode's period, so the rate
/// is at or below the request whatever the bus's edges add. The setup and
/// hold fields keep the manuals' two conditions at the mode's slowest
/// edges, the block's own hold of the clock fits inside the counted low,
/// the clock requirement holds, and the reserved bits read 0.
#[inline]
pub const fn i2c_timing_sound(word: u32, kernel_hz: u32, rate_hz: u32) -> bool {
    let m = match i2c_mode_index(rate_hz) {
        Some(i) => &I2C_MODE_TIMES[i as usize],
        None => return false,
    };
    let prescaler = (word >> 28) as u64 + 1;
    let setup = ((word >> 20) & 0xF) as u64;
    let hold = ((word >> 16) & 0xF) as u64;
    let high = (((word >> 8) & 0xFF) as u64 + 1) * prescaler;
    let low = ((word & 0xFF) as u64 + 1) * prescaler;
    let fastest = kernel_hz as u64 * I2C_CLOCK_FASTEST_PER_MILLE;
    word & (0xF << 24) == 0
        && i2c_periods_cover(low, m.low_ns, fastest)
        && i2c_periods_cover(low + 2, m.low_ns + m.fall_ns - I2C_FILTER_MIN_NS, fastest)
        && i2c_periods_cover(high, m.high_ns, fastest)
        && i2c_periods_cover(high + 2, m.high_ns + m.rise_ns - I2C_FILTER_MIN_NS, fastest)
        && i2c_periods_cover(low + high + 4, 1_000_000_000 / rate_hz as u64 - 2 * I2C_FILTER_MIN_NS, fastest)
        && i2c_periods_cover((setup + 1) * prescaler, m.rise_ns + m.setup_ns, fastest)
        && i2c_periods_cover(hold * prescaler + 3, m.fall_ns - I2C_FILTER_MIN_NS, fastest)
        && low >= (hold + setup + 1) * prescaler + 1
        && low > 4
        && (low - 4) * 1_000_000_000_000 > I2C_FILTER_MAX_NS * fastest
        && high >= 2
}

const _: () = {
    let pairs = [(16_000_000u32, I2C_RATES_HZ[0]), (16_000_000, I2C_RATES_HZ[1]), (16_000_000, I2C_RATES_HZ[2]), (54_000_000, I2C_RATES_HZ[2])];
    let mut i = 0;
    while i < pairs.len() {
        match i2c_timing(pairs[i].0, pairs[i].1) {
            Some(word) => assert!(i2c_timing_sound(word, pairs[i].0, pairs[i].1), "a timing word of the 16 MHz or the 54 MHz clock does not keep the rule"),
            None => panic!("a pair the rule's check names has no timing word"),
        }
        i += 1;
    }
};

/// A row's fact for fast mode plus on a block of this register file: the
/// kernel clock the block takes for the mode (the value of its selection
/// field and that clock's rate) and whether the block's control register 1
/// carries the mode's drive bit on the row's chip. A row states it in its
/// facts beside its bus table; a row that serves no fast mode plus states
/// none.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct I2cFastPlus {
    pub mux_value: u32,
    pub kernel_hz: u32,
    pub drive: bool,
}

/// What a claim programs on such a block for a rate: the kernel clock's
/// selection, the timing word at that clock, whether the drive bit is set.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct I2cSetup {
    pub mux_value: u32,
    pub timing: u32,
    pub drive: bool,
}

/// The setup for a rate on a bus whose own kernel clock is `bus_hz` behind
/// the selection `bus_mux`: fast mode plus by the row's fact alone, the
/// other rates at the bus's own clock; None where no word is served.
#[inline]
pub const fn i2c_setup(bus_mux: u32, bus_hz: u32, fast_plus: Option<I2cFastPlus>, rate_hz: u32) -> Option<I2cSetup> {
    if rate_hz == I2C_RATES_HZ[2] {
        match fast_plus {
            Some(f) => match i2c_timing(f.kernel_hz, rate_hz) {
                Some(timing) => Some(I2cSetup { mux_value: f.mux_value, timing, drive: f.drive }),
                None => None,
            },
            None => None,
        }
    } else {
        match i2c_timing(bus_hz, rate_hz) {
            Some(timing) => Some(I2cSetup { mux_value: bus_mux, timing, drive: false }),
            None => None,
        }
    }
}
