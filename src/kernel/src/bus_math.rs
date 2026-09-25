//! The bus doors' pure arithmetic and facts shapes: the block descriptors a
//! board's facts state per bus, the I2C message record as the header lays
//! it out, the partition of a transaction's arguments into served and
//! refused, the I2C timing word, the SPI divisor ladder and the wire-wait
//! budget in ticks. Pure, host-tested; the drivers cite the manuals for the
//! numbers here.
#![allow(dead_code)]

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
/// block number (I2C1 is bus 1), the name a program gives the doors, as a
/// Linux device node is named -- the block's base, its clock enable (a
/// register and a bit), its kernel-clock selection (a register, a shift and
/// the value that selects the fixed 16 MHz source), the block's kernel
/// clock in Hz, and the two pads with their alternate function (an empty
/// list on a row whose model has no pins). The table position is the
/// enumeration index the count and the bus doors walk.
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
/// with the four-byte queues, or the STM32U5's file with the sixteen-byte
/// queues and the transfer-size counter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpiFile {
    F7,
    U5,
}

/// An SPI bus as a board's facts state it: its NUMBER (the chip's own
/// block number: SPI2 is bus 2, the name the doors take, as a Linux device
/// node is named), the block's base and file, its
/// clock enable, its kernel-clock selection (a register of 0 where the
/// block follows a bus clock), the block's kernel clock in Hz at this
/// posture, the three pads, the chip-select lines the bus offers (the list
/// empty on a row whose model has no pins; `select_count` the number the
/// door accepts), and the pump's bound -- how many bytes may be sent ahead
/// of the bytes received, half the receive queue's depth.
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
/// has no such bus (the door then answers RHU_ENODEV).
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

/// The I2C timing word for a rate at a kernel clock: the manuals' own
/// example row for standard mode at 16 MHz (prescaler 3, the data and
/// clock setup fields 2 and 4, the high and low counts 0x0F and 0x13);
/// any other pair is not served in this generation.
pub const fn i2c_timing(kernel_hz: u32, rate_hz: u32) -> Option<u32> {
    if kernel_hz == 16_000_000 && rate_hz == 100_000 {
        Some(0x3042_0F13)
    } else {
        None
    }
}

/// The SPI divisor ladder: eight steps, the kernel clock over 2^(n + 1).
/// The largest divisor whose rate is at or below the request wins, and the
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
/// no-select value (the program drives its own through the GPIO doors), or invalid.
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
