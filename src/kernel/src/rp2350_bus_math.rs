//! The RP2350's bus arithmetic and bus facts: the I2C block's counts for a
//! rate at the block's clock, the SPI block's prescaler pair for a rate, the
//! role a line carries on the chip's function table, the classes of the I2C
//! block's transmit abort, and the bus rows every board of this chip
//! shares. No hardware access and no state, covered by the host tests; the drivers and the board facts cite the
//! datasheet for the numbers here.
#![allow(dead_code)]

use crate::bus_math::{I2cBus, I2cMsg, Pad, PadRole, SpiBus, SpiFile};

/// One I2C mode these rows serve: the rate a claim names for it, the SPEED
/// field of the control register (1 standard, 2 fast -- fast mode plus
/// shares the field and the FS register pair), the minimum low and high
/// times in nanoseconds (Table 1053) and the block clock under which the
/// mode's counts cannot be made (the table's own floor).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct I2cMode {
    pub rate_hz: u32,
    pub speed: u32,
    pub low_ns: u64,
    pub high_ns: u64,
    pub floor_hz: u32,
}

/// The closed set of three, in the order of `bus_math::I2C_RATES_HZ` (the
/// claims table stores a claimed bus's mode as its index plus one).
pub const I2C_MODES: [I2cMode; 3] = [
    I2cMode { rate_hz: 100_000, speed: 1, low_ns: 4_700, high_ns: 4_000, floor_hz: 2_700_000 },
    I2cMode { rate_hz: 400_000, speed: 2, low_ns: 1_300, high_ns: 600, floor_hz: 12_000_000 },
    I2cMode { rate_hz: 1_000_000, speed: 2, low_ns: 500, high_ns: 500, floor_hz: 32_000_000 },
];
const _: () = assert!(
    I2C_MODES[0].rate_hz == crate::bus_math::I2C_RATES_HZ[0] && I2C_MODES[1].rate_hz == crate::bus_math::I2C_RATES_HZ[1] && I2C_MODES[2].rate_hz == crate::bus_math::I2C_RATES_HZ[2],
    "the RP2350 rows' mode set is not the bus doors' rate set in its order"
);

/// A rate's mode, as its index in the set; None for a rate outside it.
pub const fn i2c_mode(rate_hz: u32) -> Option<usize> {
    let mut i = 0;
    while i < I2C_MODES.len() {
        if I2C_MODES[i].rate_hz == rate_hz {
            return Some(i);
        }
        i += 1;
    }
    None
}
/// The SPI blocks' clock: clk_peri, the 12 MHz crystal on both clock postures.
pub const SPI_CLOCK_HZ: u32 = 12_000_000;
/// The reset controller's RESET register. On this chip a bus row's
/// `enable_reg` names it and its `enable_bit` the block's reset bit: a block
/// is taken by cycling it through its reset, and it has no kernel-clock
/// selection of its own (`mux_reg` 0).
pub const RESETS_RESET: usize = 0x4002_0000;
const I2C0_BASE: usize = 0x4009_0000;
const SPI0_BASE: usize = 0x4008_0000;
/// The second block of each family sits 0x8000 above the first.
const BLOCK_STRIDE: usize = 0x8000;
const I2C0_RESET_BIT: u32 = 4;
const SPI0_RESET_BIT: u32 = 18;
/// The pad function that selects each family.
pub const I2C_FUNCTION: u8 = 3;
pub const SPI_FUNCTION: u8 = 1;
/// The I2C block's queues hold sixteen entries, the SPI block's eight.
pub const I2C_QUEUE: u32 = 16;
pub const SPI_QUEUE: u32 = 8;


/// The four numbers the I2C block is programmed with for a rate: the low
/// count, the high count, the spike length and the transmit data hold, all
/// in cycles of the block's clock -- with the mode they belong to (its index
/// in `I2C_MODES`, and its SPEED field, which names the register pair: SS
/// for 1, FS for 2) and the rate they achieve, so the driver programs from
/// the answer alone.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct I2cCounts {
    pub lcnt: u32,
    pub hcnt: u32,
    pub spklen: u32,
    pub hold: u32,
    pub mode: u8,
    pub speed: u32,
    pub achieved_hz: u32,
}

/// A time in nanoseconds as whole cycles of a clock, rounded up.
const fn cycles_up(ns: u64, clk_hz: u32) -> u32 {
    (ns * clk_hz as u64).div_ceil(1_000_000_000) as u32
}

/// Whether a set of counts keeps every rule the datasheet states for its
/// mode at this clock: the clock at or above the mode's floor, the SPEED
/// field the mode's, the two inequalities of 12.2.14.1, the mode's two
/// minimum times (Table 1053), the spike length of at least one cycle, the
/// hold between one cycle and the low period less two, every field inside
/// its register, the achieved rate the clock over the period.
pub const fn i2c_counts_sound(c: &I2cCounts, kernel_hz: u32) -> bool {
    if c.mode as usize >= I2C_MODES.len() {
        return false;
    }
    let m = &I2C_MODES[c.mode as usize];
    let clk = kernel_hz as u64;
    let low = c.lcnt as u64 + 1;
    let high = c.hcnt as u64 + c.spklen as u64 + 7;
    kernel_hz >= m.floor_hz
        && c.speed == m.speed
        && c.spklen >= 1
        && c.spklen <= 0xFF
        && c.lcnt <= 0xFFFF
        && c.hcnt <= 0xFFFF
        && c.lcnt > c.spklen + 7
        && c.hcnt > c.spklen + 5
        && low * 1_000_000_000 >= m.low_ns * clk
        && high * 1_000_000_000 >= m.high_ns * clk
        && c.hold >= 1
        && c.hold + 2 <= c.lcnt + 1
        && c.achieved_hz as u64 == clk / (low + high)
}

/// The counts for a rate at the block's clock, or None when the rate is not
/// in the set or the clock is under the mode's floor. The period is the
/// clock over the rate; the low period is the mode's minimum low time
/// rounded up to whole cycles (never under the spike length plus 9, so LCNT
/// exceeds the spike length plus 7); the high period is the rest of the
/// period, but never under the mode's minimum high time nor under twice
/// the spike length plus 13 (HCNT exceeds the spike length plus 5, and the
/// block adds the spike length plus 7: the strict reading of 12.2.14.1);
/// when low plus high exceeds the period, the period stretches to them and
/// the achieved rate is the clock over low plus high -- the fastest the
/// block makes at or below the request within the mode's rules. The spike
/// length is 50 ns rounded down, at least one cycle; the data hold is 300
/// ns rounded up. At 12 MHz: 100 kHz 56, 55, 1 and 4; 400 kHz 15, 7, 1 and
/// 4 at 387,096 Hz; 1 MHz None (the floor is 32 MHz). At 150 MHz: 100 kHz
/// 704, 781, 7 and 45; 400 kHz 194 and 166; 1 MHz 74 and 61, both exact.
pub const fn i2c_counts(kernel_hz: u32, rate_hz: u32) -> Option<I2cCounts> {
    let mode = match i2c_mode(rate_hz) {
        Some(i) => i,
        None => return None,
    };
    let m = &I2C_MODES[mode];
    if kernel_hz < m.floor_hz {
        return None;
    }
    let period = kernel_hz / rate_hz;
    let spike = (50 * kernel_hz as u64 / 1_000_000_000) as u32;
    let spklen = if spike < 1 { 1 } else { spike };
    let mut low = cycles_up(m.low_ns, kernel_hz);
    if low < spklen + 9 {
        low = spklen + 9;
    }
    let mut high_min = cycles_up(m.high_ns, kernel_hz);
    if high_min < 2 * spklen + 13 {
        high_min = 2 * spklen + 13;
    }
    let mut high = if period > low { period - low } else { 0 };
    if high < high_min {
        high = high_min;
    }
    let c = I2cCounts {
        lcnt: low - 1,
        hcnt: high - spklen - 7,
        spklen,
        hold: cycles_up(300, kernel_hz),
        mode: mode as u8,
        speed: m.speed,
        achieved_hz: kernel_hz / (low + high),
    };
    if i2c_counts_sound(&c, kernel_hz) {
        Some(c)
    } else {
        None
    }
}


/// The prescaler pair for a rate at the block's clock: the pair whose rate is
/// the HIGHEST at or below the request, packed as CPSDVSR in bits 7:0 and
/// SCR in bits 15:8, with the rate it achieves in whole Hz -- the answer the
/// door gives. Every even prescaler from 2 to 254 is tried with the smallest
/// SCR that brings the rate to the request or under it; the smallest product
/// wins. None when even the largest product (254 x 256) leaves the rate
/// above the request, or the request is 0. At 12 MHz the ladder runs from
/// 6 MHz down to 184 Hz; a request above the top is served at the top.
pub const fn spi_pair(kernel_hz: u32, rate_hz: u32) -> Option<(u32, u32)> {
    if rate_hz == 0 || kernel_hz == 0 {
        return None;
    }
    let floor = (kernel_hz as u64 / (rate_hz as u64 + 1)) as u32 + 1;
    let mut best = 0u32;
    let mut pair = 0u32;
    let mut cpsdvsr = 2u32;
    while cpsdvsr <= 254 {
        let steps = floor.div_ceil(cpsdvsr);
        if steps <= 256 {
            let product = cpsdvsr * steps;
            if best == 0 || product < best {
                best = product;
                pair = cpsdvsr | ((steps - 1) << 8);
            }
        }
        cpsdvsr += 2;
    }
    if best == 0 {
        None
    } else {
        Some((pair, kernel_hz / best))
    }
}

/// The rate a packed pair makes at the block's clock, in whole Hz (0 for a
/// pair with no prescaler: no such pair comes from the ladder).
pub const fn spi_pair_rate(kernel_hz: u32, pair: u32) -> u32 {
    let product = (pair & 0xFF) * (((pair >> 8) & 0xFF) + 1);
    if product == 0 {
        0
    } else {
        kernel_hz / product
    }
}


/// The I2C block a line's function 3 belongs to: bit 1 of the line number.
pub const fn i2c_block_of(line: u32) -> u32 {
    (line >> 1) & 1
}

/// The I2C role of a line: an even line carries the data, an odd one the clock.
pub const fn i2c_role_of(line: u32) -> PadRole {
    if line & 1 == 0 {
        PadRole::I2cData
    } else {
        PadRole::I2cClock
    }
}

/// The SPI block a line's function 1 belongs to: bit 3 of the line number.
pub const fn spi_block_of(line: u32) -> u32 {
    (line >> 3) & 1
}

/// The SPI role of a line, by its number modulo 4: the block's input, its
/// own frame signal (no pad of a bus row: a chip select is a plain output
/// line, never this function), its clock, its output.
pub const fn spi_role_of(line: u32) -> Option<PadRole> {
    match line % 4 {
        0 => Some(PadRole::SpiIn),
        2 => Some(PadRole::SpiClock),
        3 => Some(PadRole::SpiOut),
        _ => None,
    }
}

/// Whether every pad of every row carries its role on the function table:
/// the family's function number, the row's own block, the role the line has.
pub const fn rows_sound(i2c: &[I2cBus], spi: &[SpiBus]) -> bool {
    let mut i = 0;
    while i < i2c.len() {
        let b = &i2c[i];
        let mut k = 0;
        while k < b.pads.len() {
            let p = &b.pads[k];
            let line = p.line as u32;
            if p.alternate != I2C_FUNCTION || i2c_block_of(line) != b.number || p.role as u8 != i2c_role_of(line) as u8 {
                return false;
            }
            k += 1;
        }
        i += 1;
    }
    let mut i = 0;
    while i < spi.len() {
        let b = &spi[i];
        let mut k = 0;
        while k < b.pads.len() {
            let p = &b.pads[k];
            let line = p.line as u32;
            let role = match spi_role_of(line) {
                Some(r) => r as u8 == p.role as u8,
                None => false,
            };
            if p.alternate != SPI_FUNCTION || spi_block_of(line) != b.number || !role {
                return false;
            }
            k += 1;
        }
        i += 1;
    }
    true
}


/// The pins of the first bus of each family: the ones the board maker's
/// datasheet marks as that block's default (the Raspberry Pi Pico 2
/// datasheet, Figure 2, the pinout: the labels in the "default" shade) --
/// I2C0's data on GPIO 4 and its clock on GPIO 5; SPI0's clock on GPIO 18,
/// its output on GPIO 19, its input on GPIO 16 and its chip select on GPIO
/// 17. The datasheet marks no default for the chip's second I2C block or
/// its second SPI block, so no row carries them: the constructors and the
/// drivers serve either block, and a second bus follows once a program can
/// choose a bus's pins.
pub const I2C0_PADS: [Pad; 2] =
    [Pad { line: 4, alternate: I2C_FUNCTION, role: PadRole::I2cData }, Pad { line: 5, alternate: I2C_FUNCTION, role: PadRole::I2cClock }];
pub const SPI0_PADS: [Pad; 3] = [
    Pad { line: 18, alternate: SPI_FUNCTION, role: PadRole::SpiClock },
    Pad { line: 19, alternate: SPI_FUNCTION, role: PadRole::SpiOut },
    Pad { line: 16, alternate: SPI_FUNCTION, role: PadRole::SpiIn },
];
pub const SPI0_SELECTS: [u16; 1] = [17];

/// One I2C block as a bus row (either block of the chip): the number is the
/// block's own (I2C0 is bus 0), the clock is the system clock at the posture.
pub const fn i2c_bus(block: u32, kernel_hz: u32, pads: &'static [Pad]) -> I2cBus {
    I2cBus {
        number: block,
        base: I2C0_BASE + BLOCK_STRIDE * block as usize,
        enable_reg: RESETS_RESET,
        enable_bit: I2C0_RESET_BIT + block,
        mux_reg: 0,
        mux_shift: 0,
        mux_value: 0,
        kernel_hz,
        pads,
    }
}

/// One SPI block as a bus row (either block of the chip): the number is the
/// block's own, the clock the crystal, the pump's bound half the receive queue.
pub const fn spi_bus(block: u32, pads: &'static [Pad], selects: &'static [u16], select_count: u32) -> SpiBus {
    SpiBus {
        number: block,
        base: SPI0_BASE + BLOCK_STRIDE * block as usize,
        file: SpiFile::Pl022,
        enable_reg: RESETS_RESET,
        enable_bit: SPI0_RESET_BIT + block,
        mux_reg: 0,
        mux_shift: 0,
        mux_value: 0,
        kernel_hz: SPI_CLOCK_HZ,
        pads,
        selects,
        select_count,
        ahead: SPI_QUEUE / 2,
    }
}

/// The rows: one I2C bus and one SPI bus, the chip's first block of each.
pub const fn i2c_buses(sysclk_hz: u32) -> [I2cBus; 1] {
    [i2c_bus(0, sysclk_hz, &I2C0_PADS)]
}

pub const fn spi_buses() -> [SpiBus; 1] {
    [spi_bus(0, &SPI0_PADS, &SPI0_SELECTS, 1)]
}

const _: () = assert!(rows_sound(&i2c_buses(SPI_CLOCK_HZ), &spi_buses()), "a bus row's pad does not carry its role on the chip's function table");


/// The abort's source bits the driver tells apart, and where the count of
/// flushed commands starts.
pub const ABRT_7B_ADDR_NOACK: u32 = 1 << 0;
pub const ABRT_TXDATA_NOACK: u32 = 1 << 3;
pub const ABRT_ARB_LOST: u32 = 1 << 12;
pub const ABRT_USER_ABRT: u32 = 1 << 16;
pub const TX_FLUSH_SHIFT: u32 = 23;

/// What an abort was: the address not acknowledged (a scan's answer), a data
/// byte not acknowledged, arbitration lost, or anything else the block names.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Abort {
    Address,
    Data,
    Arbitration,
    Other,
}

pub const fn abort_class(source: u32) -> Abort {
    if source & ABRT_7B_ADDR_NOACK != 0 {
        Abort::Address
    } else if source & ABRT_TXDATA_NOACK != 0 {
        Abort::Data
    } else if source & ABRT_ARB_LOST != 0 {
        Abort::Arbitration
    } else {
        Abort::Other
    }
}

/// The command the block took last, counted over the whole transaction from
/// 0: the commands issued, less the ones still in the queue (or flushed from
/// it by an abort), less one; 0 when it took none.
pub const fn last_taken(issued: u32, queued: u32) -> u32 {
    issued.saturating_sub(queued).saturating_sub(1)
}

/// The message and the byte a command names: one command a byte, in the
/// order of the list. A command past the end names the last message.
pub fn locate(msgs: &[I2cMsg], command: u32) -> (u32, u32) {
    let mut k = command;
    for (i, m) in msgs.iter().enumerate() {
        if k < m.len as u32 {
            return (i as u32, k);
        }
        k -= m.len as u32;
    }
    (msgs.len().saturating_sub(1) as u32, 0)
}

/// Whether the block reserves a 7-bit address: under 0x08 or over 0x77 (the
/// datasheet's note under the block's address registers: its operation is
/// not guaranteed at a reserved value).
pub const fn i2c_reserved(addr: u32) -> bool {
    !matches!(addr, 0x08..=0x77)
}

/// The message this block's family refuses before the wire, or None when the
/// list is served: the block addresses one target a transaction, has no
/// address-only frame and reserves sixteen addresses, so a message of length
/// 0, one naming another address than the first, or one at a reserved
/// address is refused.
pub fn i2c_refused_message(msgs: &[I2cMsg]) -> Option<u32> {
    let first = msgs.first()?;
    for (i, m) in msgs.iter().enumerate() {
        if m.len == 0 || m.addr != first.addr || i2c_reserved(m.addr as u32) {
            return Some(i as u32);
        }
    }
    None
}
