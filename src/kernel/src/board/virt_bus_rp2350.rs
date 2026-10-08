//! The Cortex-M33 emulator row's bus backend under the test feature
//! `test-bus-rp2350` (a gate-only feature; no board image is built with it): register-level models of the
//! RP2350's two bus blocks in place of the ST blocks' models, so the chip's
//! bus drivers run on QEMU against the register files the boards carry --
//! the Synopsys I2C block (the enable with its abort request and its status,
//! the target address, the command queue with its read, stop and restart
//! bits, the receive queue, the abort's sources with the count of commands
//! flushed, the stop flag, the two queue levels) and the Arm PL022 SPI block
//! (the two eight-entry queues, the enable, the prescaler pair, the busy
//! flag, the receive overrun). Behind the blocks, the same two devices as
//! the ST models carry, as this file's own small copies: a sensor at 0x76
//! answering its identity register and a burst, and a memory answering the
//! identity command and echoing every other byte. The wire is PACED by the
//! kernel's clock -- a byte completes after its bits' time at the programmed
//! rate, never earlier -- so the drivers' budgets are proven against real
//! wire times. What a driver must not do is refused loudly (a driver's
//! mistake caught on the emulator): a register read or written while the
//! block is in reset, a control or count register written while the block
//! is enabled, counts that break the datasheet's rules for the control
//! word's mode or a count pair that is not that mode's, a
//! command written to a disabled block or to a full queue, an interrupt
//! unmasked, a frame the model does not serve. And what the blocks do in
//! time is modelled in time, so a driver that does not wait is refused: the
//! I2C enable's STATUS follows a disable of an enabled block a moment later,
//! and a register that needs a disabled block, or a new enable, is judged by
//! the status; a start sooner than the bus-free time after the last stop is
//! refused; the SPI clock line is low while the block is disabled and
//! reaches a high idle level a moment after the enable, and a select
//! asserted while the line is not at the mode's idle level is refused. A
//! read of the I2C data register that no read command is owed, and a
//! reserved target address at the enable, are refused too. The block's reset
//! is the reset controller's bit, reached through `block_reset`; the pad
//! seam is empty but for the SPI clock line and the I2C recovery's two lines
//! (the models have no other pin): the recovery's clock released after less
//! than the specification's 35 ms of low is refused loudly.
//! The doctored feature `test-bus-doctored` makes the sensor refuse the
//! second data byte of a write, the I2C block never perform a repeated start
//! (the clock held: an abort asked of it never completes), the driver's pump
//! unbounded, one byte nobody asked for sit in the I2C receive queue at
//! every enable, and the SPI clock line late to its idle level at either
//! polarity -- the negative controls the api flavor's doctored image runs.
//! That image also arms, through `doctor`, one event at a time: a device
//! that holds the clock low 25 ms in one byte and then answers; a device
//! left holding the data line low at the block's next reset (until three
//! clocks have come, until the clock has been held low 35 ms, or for good);
//! a block whose reset at the claim does not complete (the I2C enable's
//! status then reads 1 for a millisecond); an enable status stuck at 1 for a
//! millisecond; an abort whose stop never comes; the clock held from the
//! next command on; an SPI clock line that never reaches its idle level; a
//! receive overrun shown at the next SPI enable.

use core::ptr::addr_of_mut;

use crate::board::facts;
use crate::bus_math::{I2cBus, Pad, SpiBus};
use crate::rp2350_bus_math as rp;
use crate::sched;

const DOCTORED: bool = cfg!(feature = "test-bus-doctored");


/// What the doctored image arms, one event at a time (the module's head
/// tells each).
#[allow(dead_code)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Doctor {
    Stretch,
    HeldClocks,
    HeldLong,
    HeldStuck,
    Free,
    ClaimUnrested,
    StatusStuck,
    AbortNoStop,
    StallNext,
    SpiClockNever,
    SpiOverrun,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Held {
    No,
    Clocks(u32),
    Long,
    Stuck,
}

/// The modelled wire: what holds the data line now, what will at the
/// block's next reset, and when the recovery's clock went low.
struct Wire {
    held: Held,
    next: Held,
    low_ns: u64,
}

static mut WIRE: Wire = Wire { held: Held::No, next: Held::No, low_ns: 0 };

fn wire() -> &'static mut Wire {
    unsafe { &mut *addr_of_mut!(WIRE) }
}

/// The clock held low in one byte by the doctored device; the low time the
/// specification's remedy asks of the recovery's clock; how long the I2C
/// enable's status reads 1 behind a reset that did not complete.
const STRETCH_NS: u64 = 25_000_000;
const HOLD_NS: u64 = 35_000_000;
const UNRESTED_NS: u64 = 1_000_000;

#[allow(dead_code)]
pub fn doctor(what: Doctor) {
    let w = wire();
    match what {
        Doctor::Stretch => i2c().doctor_stretch = true,
        Doctor::HeldClocks => w.next = Held::Clocks(3),
        Doctor::HeldLong => w.next = Held::Long,
        Doctor::HeldStuck => w.next = Held::Stuck,
        Doctor::Free => {
            w.held = Held::No;
            w.next = Held::No;
        }
        Doctor::ClaimUnrested => {
            i2c().doctor_reset = true;
            spi().doctor_reset = true;
        }
        Doctor::StatusStuck => i2c().rest_ns = sched::now_ns() + UNRESTED_NS,
        Doctor::AbortNoStop => i2c().doctor_no_stop = true,
        Doctor::StallNext => i2c().doctor_stall = true,
        Doctor::SpiClockNever => spi().doctor_clock = true,
        Doctor::SpiOverrun => spi().doctor_overrun = true,
    }
}

pub fn pad_take(_pad: &Pad) {}

pub fn i2c_sda_low(_d: &I2cBus) -> bool {
    wire().held != Held::No
}

pub fn i2c_scl_pulse(_d: &I2cBus) {
    let w = wire();
    if let Held::Clocks(n) = w.held {
        w.held = if n > 1 { Held::Clocks(n - 1) } else { Held::No };
    }
}

/// The recovery's long clock: released after less than the specification's
/// 35 ms of low, it is refused; a device that resets on the clock held low
/// lets the data line go at the release.
pub fn i2c_scl_hold(_d: &I2cBus, low: bool) {
    let w = wire();
    if low {
        w.low_ns = sched::now_ns();
        return;
    }
    let held_ns = sched::now_ns() - w.low_ns;
    assert!(held_ns >= HOLD_NS, "[bus model] the recovery's clock released after {} ns of low, under the 35 ms of the specification's remedy", held_ns);
    if w.held == Held::Long {
        w.held = Held::No;
    }
}

pub fn i2c_stop(_d: &I2cBus) {}

pub fn select_take(_d: &SpiBus, _index: u32) {}

pub fn select_rest(_d: &SpiBus) {}

/// A chip select driven: the first select reaches the memory model; the
/// second reaches nothing (a byte clocked under it answers ones). A select
/// asserted while the clock line is not at the mode's idle level is refused.
pub fn select_drive(_d: &SpiBus, index: u32, low: bool) {
    let s = spi();
    assert!(!low || s.clock_high() == s.idle_high(), "[bus model] an SPI select asserted while the clock line is not at the mode's idle level");
    if index == 0 {
        s.dev.select(low);
    }
}

/// Whether the modelled SPI clock line reads at the level asked.
pub fn spi_clock_at(_d: &SpiBus, high: bool) -> bool {
    spi().clock_high() == high
}


/// A block cycled through its reset: the block the descriptor's reset bit
/// names starts over, every register at its model's reset state. The model's
/// reset completes -- but for the one the doctored image arms: that one
/// answers false, and behind it the I2C enable's status reads 1 for a while.
pub fn block_reset(reset_reg: usize, bit: u32) -> bool {
    let i = &facts::I2C_BUSES[0];
    let s = &facts::SPI_BUSES[0];
    if reset_reg == i.enable_reg && bit == i.enable_bit {
        let m = i2c();
        m.reset();
        if core::mem::take(&mut m.doctor_reset) {
            m.rest_ns = sched::now_ns() + UNRESTED_NS;
            return false;
        }
    } else if reset_reg == s.enable_reg && bit == s.enable_bit {
        let m = spi();
        m.reset();
        if core::mem::take(&mut m.doctor_reset) {
            return false;
        }
    } else {
        panic!("[bus model] no block behind reset bit {} of {:#010x}", bit, reset_reg);
    }
    true
}

pub fn rd(addr: usize) -> u32 {
    let i = &facts::I2C_BUSES[0];
    let s = &facts::SPI_BUSES[0];
    if addr >= i.base && addr < i.base + 0x100 {
        return i2c().rd(addr - i.base);
    }
    if addr >= s.base && addr < s.base + 0x28 {
        return spi().rd(addr - s.base);
    }
    panic!("[bus model] no register at {:#010x}", addr);
}

pub fn wr(addr: usize, v: u32) {
    let i = &facts::I2C_BUSES[0];
    let s = &facts::SPI_BUSES[0];
    if addr >= i.base && addr < i.base + 0x100 {
        return i2c().wr(addr - i.base, v);
    }
    if addr >= s.base && addr < s.base + 0x28 {
        return spi().wr(addr - s.base, v);
    }
    panic!("[bus model] no register at {:#010x}", addr);
}

/// The models' counters the api flavor's doctored image reads: bytes the
/// SPI receive queue dropped on overrun; the I2C block's resets after the
/// claim's own.
#[allow(dead_code)]
pub fn probe_spi_drops() -> u32 {
    spi().drops
}

#[allow(dead_code)]
pub fn probe_i2c_resets() -> u32 {
    i2c().resets
}

/// The counts the block holds and the register pair they were written to
/// last (1 the SS pair, 2 the FS pair, 0 none since the reset).
#[allow(dead_code)]
pub fn probe_i2c_counts() -> (u32, u32, u32, u8) {
    let m = i2c();
    (m.lcnt, m.hcnt, m.spklen, m.pair)
}


const IC_CON: usize = 0x00;
const IC_TAR: usize = 0x04;
const IC_DATA_CMD: usize = 0x10;
const IC_SS_SCL_HCNT: usize = 0x14;
const IC_SS_SCL_LCNT: usize = 0x18;
const IC_FS_SCL_HCNT: usize = 0x1C;
const IC_FS_SCL_LCNT: usize = 0x20;
const IC_INTR_MASK: usize = 0x30;
const IC_RAW_INTR_STAT: usize = 0x34;
const IC_CLR_TX_ABRT: usize = 0x54;
const IC_CLR_STOP_DET: usize = 0x60;
const IC_ENABLE: usize = 0x6C;
const IC_STATUS: usize = 0x70;
const IC_TXFLR: usize = 0x74;
const IC_RXFLR: usize = 0x78;
const IC_SDA_HOLD: usize = 0x7C;
const IC_TX_ABRT_SOURCE: usize = 0x80;
const IC_ENABLE_STATUS: usize = 0x9C;
const IC_FS_SPKLEN: usize = 0xA0;
/// The two control words the model serves: a master (bit 0) at standard
/// speed (bits 2:1 = 1) or at fast speed (bits 2:1 = 2; fast mode plus
/// shares it), the repeated start allowed (bit 5), the slave half off
/// (bit 6), the clock held on a full receive queue (bit 9).
const CON_SERVED_SS: u32 = 0x263;
const CON_SERVED_FS: u32 = 0x265;
const SOURCE_ADDRESS: u32 = 1 << 0;
const SOURCE_DATA: u32 = 1 << 3;
const SOURCE_USER: u32 = 1 << 16;
const FLUSH_SHIFT: u32 = 23;
const I2C_DEPTH: usize = 16;
const CMD_READ: u16 = 1 << 8;
/// The enable's status follows a disable of an enabled block this long after
/// it (12.2.10.3: the block shuts down at the end of what it is doing).
const STATUS_LAG_NS: u64 = 20_000;
/// Standard mode's bus-free time between a stop and the next start.
const BUS_FREE_NS: u64 = 4_700;

/// The sensor: a register pointer, the identity register, the data burst;
/// every register write acknowledged (the reset among them). The doctored
/// feature refuses the second data byte of a write.
struct Sensor {
    ptr: u8,
}

const SENSOR_ADDR: u32 = 0x76;
const SENSOR_ID: u8 = 0x60;
const SENSOR_BURST: [u8; 8] = [0x80, 0x00, 0x00, 0x80, 0x00, 0x00, 0x80, 0x00];

impl Sensor {
    fn write_byte(&mut self, b: u8, index: u32) -> bool {
        if DOCTORED && index == 1 {
            return false;
        }
        if index == 0 {
            self.ptr = b;
        } else {
            self.ptr = self.ptr.wrapping_add(1);
        }
        true
    }

    fn read_byte(&mut self) -> u8 {
        let v = match self.ptr {
            0xD0 => SENSOR_ID,
            0xF7..=0xFE => SENSOR_BURST[(self.ptr - 0xF7) as usize],
            _ => 0,
        };
        self.ptr = self.ptr.wrapping_add(1);
        v
    }
}

/// What is on the wire: nothing; a data byte written (acknowledged or not);
/// a byte read; an address nobody acknowledges.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Flight {
    None,
    Write { stop: bool, ack: bool },
    Read { stop: bool },
    AddressNack,
}

struct I2cModel {
    up: bool,
    ever_up: bool,
    resets: u32,
    enabled: bool,
    stalled: bool,
    con: u32,
    tar: u32,
    hcnt: u32,
    lcnt: u32,
    spklen: u32,
    hold: u32,
    tx: [u16; I2C_DEPTH],
    tx_len: usize,
    rx: [u8; I2C_DEPTH],
    rx_len: usize,
    tx_abrt: bool,
    stop_det: bool,
    source: u32,
    active: bool,
    reading: bool,
    moved: u32,
    flight: Flight,
    due_ns: u64,
    byte_ns: u64,
    /// The read commands taken and not yet answered by a read of the data
    /// register; the time the enable's status follows the last disable; the
    /// time of the last stop on the wire.
    owed: u32,
    rest_ns: u64,
    stop_ns: u64,
    /// What the doctored image armed, each spent by the event it names.
    doctor_stretch: bool,
    doctor_reset: bool,
    doctor_no_stop: bool,
    doctor_stall: bool,
    /// The register pair the counts were written to last: 1 the SS pair, 2
    /// the FS pair, 0 none since the reset (a byte of the struct's padding).
    pair: u8,
    dev: Sensor,
}

static mut I2C: I2cModel = I2cModel {
    up: false,
    ever_up: false,
    resets: 0,
    enabled: false,
    stalled: false,
    con: 0,
    tar: 0,
    hcnt: 0,
    lcnt: 0,
    spklen: 0,
    hold: 0,
    tx: [0; I2C_DEPTH],
    tx_len: 0,
    rx: [0; I2C_DEPTH],
    rx_len: 0,
    tx_abrt: false,
    stop_det: false,
    source: 0,
    active: false,
    reading: false,
    moved: 0,
    flight: Flight::None,
    due_ns: 0,
    byte_ns: 0,
    owed: 0,
    rest_ns: 0,
    stop_ns: 0,
    doctor_stretch: false,
    doctor_reset: false,
    doctor_no_stop: false,
    doctor_stall: false,
    pair: 0,
    dev: Sensor { ptr: 0 },
};

fn i2c() -> &'static mut I2cModel {
    unsafe { &mut *addr_of_mut!(I2C) }
}

impl I2cModel {
    /// The reset controller's bit cycled: the block starts over; the device
    /// behind it keeps its own state, and the wire the time of its last
    /// stop. Every reset after the first is counted.
    fn reset(&mut self) {
        if self.ever_up {
            self.resets += 1;
        }
        self.ever_up = true;
        self.up = true;
        self.enabled = false;
        self.rest_ns = 0;
        self.owed = 0;
        self.stalled = false;
        self.con = 0;
        self.tar = 0;
        self.hcnt = 0;
        self.lcnt = 0;
        self.spklen = 0;
        self.hold = 0;
        self.pair = 0;
        self.tx_len = 0;
        self.rx_len = 0;
        self.tx_abrt = false;
        self.stop_det = false;
        self.source = 0;
        self.active = false;
        self.flight = Flight::None;
        let w = wire();
        if w.next != Held::No {
            w.held = w.next;
            w.next = Held::No;
        }
    }

    /// The enable's status: 1 while the block is enabled, and for the lag
    /// after a disable of an enabled block.
    fn status_on(&self) -> bool {
        self.enabled || sched::now_ns() < self.rest_ns
    }

    /// The enable: refused before the status has followed the last disable
    /// and at a target address the block reserves; the control word and the
    /// counts checked against the datasheet's rules for the control word's
    /// mode at this row's clock (the SPEED field names the register pair the
    /// counts must have come through; standard under SPEED 1; under SPEED 2
    /// fast mode plus when the period is under fast mode's, else fast), and
    /// the byte's time taken from the counts (nine bit
    /// times). The doctored block wakes with one byte nobody asked for in its
    /// receive queue.
    fn enable(&mut self) {
        assert!(!self.status_on(), "[bus model] the I2C block enabled while its enable status still reads 1");
        assert!((0x08..=0x77).contains(&self.tar), "[bus model] the I2C target address {:#x} is one the block reserves", self.tar);
        if DOCTORED {
            self.rx[0] = 0xA5;
            self.rx_len = 1;
        }
        assert!(self.con == CON_SERVED_SS || self.con == CON_SERVED_FS, "[bus model] the I2C control word {:#x} is neither master word the model serves (standard speed 0x263, fast speed 0x265)", self.con);
        let speed = (self.con >> 1) & 3;
        assert!(self.pair == speed as u8, "[bus model] the I2C counts were written last to register pair {} under a control word whose SPEED field is {} (1 names the SS pair, 2 the FS pair)", self.pair, speed);
        let clk = facts::I2C_BUSES[0].kernel_hz as u64;
        let (l, h, s, hold) = (self.lcnt as u64, self.hcnt as u64, self.spklen as u64, self.hold as u64);
        assert!(s >= 1 && l > s + 7 && h > s + 5, "[bus model] the I2C counts break the block's rule (low {} high {} spike {})", l, h, s);
        let period_ns = ((l + 1) + (h + s + 7)) * 1_000_000_000 / clk;
        let mode = if speed == 1 {
            0
        } else if period_ns < 2_500 {
            2
        } else {
            1
        };
        let m = &rp::I2C_MODES[mode];
        assert!(clk >= m.floor_hz as u64, "[bus model] the block's clock {} Hz is under the {} Hz floor of the {} Hz mode", clk, m.floor_hz, m.rate_hz);
        assert!(
            (l + 1) * 1_000_000_000 >= m.low_ns * clk && (h + s + 7) * 1_000_000_000 >= m.high_ns * clk,
            "[bus model] the I2C counts make a period under the {} Hz mode's minimum times (low {} high {} spike {})",
            m.rate_hz,
            l,
            h,
            s
        );
        assert!(hold >= 1 && hold + 2 <= l + 1, "[bus model] the I2C data hold {} is outside one cycle to the low period less two", hold);
        self.byte_ns = 9 * ((l + 1) + (h + s + 7)) * 1_000_000_000 / clk;
        self.enabled = true;
    }

    fn tx_pop(&mut self) -> u16 {
        let c = self.tx[0];
        self.tx.copy_within(1..self.tx_len, 0);
        self.tx_len -= 1;
        c
    }

    fn rx_pop(&mut self) -> u8 {
        if self.rx_len == 0 {
            return 0;
        }
        let b = self.rx[0];
        self.rx.copy_within(1..self.rx_len, 0);
        self.rx_len -= 1;
        b
    }

    /// A command done: its stop, where it carried one, at the byte's end.
    fn finish(&mut self, stop: bool) {
        if stop {
            self.stop_det = true;
            self.active = false;
            self.stop_ns = self.due_ns;
        }
    }

    /// An abort: the source with the count of commands flushed from the
    /// queue, the queue emptied (a read command flushed is owed no byte), a
    /// stop sent at the time given -- but for the abort the doctored image
    /// armed, whose stop never comes: the block stays active.
    fn abort(&mut self, source: u32, at_ns: u64) {
        self.tx_abrt = true;
        self.source = source | ((self.tx_len as u32) << FLUSH_SHIFT);
        let flushed = self.tx[..self.tx_len].iter().filter(|c| **c & CMD_READ != 0).count() as u32;
        self.owed = self.owed.saturating_sub(flushed);
        self.tx_len = 0;
        if core::mem::take(&mut self.doctor_no_stop) {
            self.active = true;
            self.stalled = true;
            return;
        }
        self.stop_det = true;
        self.active = false;
        self.stop_ns = at_ns;
    }

    /// The wire moved to now: the byte in flight completes at its time; the
    /// next command takes the wire -- a start (or a repeated start) and the
    /// address byte before it where the transfer opens, the direction turns
    /// or the command asks. The doctored block never performs a repeated
    /// start: it holds the clock from there on.
    fn advance(&mut self) {
        loop {
            let now = sched::now_ns();
            if self.flight != Flight::None {
                if now < self.due_ns {
                    return;
                }
                match self.flight {
                    Flight::Write { stop, ack: true } => {
                        self.moved += 1;
                        self.finish(stop);
                    }
                    Flight::Write { ack: false, .. } => self.abort(SOURCE_DATA, self.due_ns),
                    Flight::Read { stop } => {
                        if self.rx_len < I2C_DEPTH {
                            self.rx[self.rx_len] = self.dev.read_byte();
                            self.rx_len += 1;
                        }
                        self.finish(stop);
                    }
                    Flight::AddressNack => self.abort(SOURCE_ADDRESS, self.due_ns),
                    Flight::None => {}
                }
                self.flight = Flight::None;
            }
            if !self.enabled || self.stalled || self.tx_len == 0 {
                return;
            }
            let cmd = self.tx[0] as u32;
            let read = cmd & (1 << 8) != 0;
            let stop = cmd & (1 << 9) != 0;
            let restart = cmd & (1 << 10) != 0;
            if (DOCTORED && restart) || core::mem::take(&mut self.doctor_stall) {
                self.stalled = true;
                return;
            }
            if read && self.rx_len == I2C_DEPTH {
                return;
            }
            self.tx_pop();
            let stretch = if core::mem::take(&mut self.doctor_stretch) { STRETCH_NS } else { 0 };
            let from = if self.due_ns > now { self.due_ns } else { now };
            if !self.active || restart || read != self.reading {
                assert!(self.active || from >= self.stop_ns + BUS_FREE_NS, "[bus model] an I2C start {} ns after the last stop, under the bus-free time", from.saturating_sub(self.stop_ns));
                self.active = true;
                self.reading = read;
                self.moved = 0;
                if self.tar != SENSOR_ADDR {
                    self.flight = Flight::AddressNack;
                    self.due_ns = from + self.byte_ns + stretch;
                    continue;
                }
                self.due_ns = from + 2 * self.byte_ns + stretch;
            } else {
                self.due_ns = from + self.byte_ns + stretch;
            }
            self.flight = if read { Flight::Read { stop } } else { Flight::Write { stop, ack: self.dev.write_byte(cmd as u8, self.moved) } };
        }
    }

    fn rd(&mut self, off: usize) -> u32 {
        assert!(self.up, "[bus model] the I2C block read at {:#x} while in reset", off);
        match off {
            IC_CON => self.con,
            IC_TAR => self.tar,
            IC_DATA_CMD => {
                self.advance();
                assert!(self.owed > 0, "[bus model] the I2C data register read with no read command outstanding");
                self.owed -= 1;
                self.rx_pop() as u32
            }
            IC_SS_SCL_HCNT => if self.pair == 1 { self.hcnt } else { 0 },
            IC_SS_SCL_LCNT => if self.pair == 1 { self.lcnt } else { 0 },
            IC_FS_SCL_HCNT => if self.pair == 2 { self.hcnt } else { 0 },
            IC_FS_SCL_LCNT => if self.pair == 2 { self.lcnt } else { 0 },
            IC_RAW_INTR_STAT => {
                self.advance();
                (self.tx_abrt as u32) << 6 | (self.stop_det as u32) << 9
            }
            IC_CLR_TX_ABRT => {
                self.tx_abrt = false;
                self.source = 0;
                0
            }
            IC_CLR_STOP_DET => {
                self.stop_det = false;
                0
            }
            IC_ENABLE => self.enabled as u32,
            IC_ENABLE_STATUS => self.status_on() as u32,
            IC_STATUS => {
                self.advance();
                let busy = self.active || self.flight != Flight::None;
                (busy as u32)
                    | ((self.tx_len < I2C_DEPTH) as u32) << 1
                    | ((self.tx_len == 0) as u32) << 2
                    | ((self.rx_len > 0) as u32) << 3
                    | ((self.rx_len == I2C_DEPTH) as u32) << 4
                    | (self.active as u32) << 5
            }
            IC_TXFLR => {
                self.advance();
                self.tx_len as u32
            }
            IC_RXFLR => {
                self.advance();
                self.rx_len as u32
            }
            IC_SDA_HOLD => self.hold,
            IC_TX_ABRT_SOURCE => self.source,
            IC_FS_SPKLEN => self.spklen,
            _ => 0,
        }
    }

    fn wr(&mut self, off: usize, v: u32) {
        assert!(self.up, "[bus model] the I2C block written at {:#x} while in reset", off);
        match off {
            IC_CON | IC_TAR | IC_SS_SCL_HCNT | IC_SS_SCL_LCNT | IC_FS_SCL_HCNT | IC_FS_SCL_LCNT | IC_SDA_HOLD | IC_FS_SPKLEN => {
                assert!(!self.status_on(), "[bus model] the I2C register {:#x} written while the block's enable status reads 1", off);
                match off {
                    IC_CON => self.con = v,
                    IC_TAR => self.tar = v & 0x3FF,
                    IC_SS_SCL_HCNT => {
                        self.hcnt = v;
                        self.pair = 1;
                    }
                    IC_SS_SCL_LCNT => {
                        self.lcnt = v;
                        self.pair = 1;
                    }
                    IC_FS_SCL_HCNT => {
                        self.hcnt = v;
                        self.pair = 2;
                    }
                    IC_FS_SCL_LCNT => {
                        self.lcnt = v;
                        self.pair = 2;
                    }
                    IC_SDA_HOLD => self.hold = v & 0xFFFF,
                    _ => self.spklen = v,
                }
            }
            IC_DATA_CMD => {
                assert!(self.enabled, "[bus model] an I2C command written while the block is disabled");
                assert!(self.tx_len < I2C_DEPTH, "[bus model] an I2C command written to a full queue");
                self.tx[self.tx_len] = v as u16;
                self.tx_len += 1;
                if v as u16 & CMD_READ != 0 {
                    self.owed += 1;
                }
                self.advance();
            }
            IC_INTR_MASK => assert!(v == 0, "[bus model] an I2C interrupt unmasked ({:#x})", v),
            IC_ENABLE => {
                self.advance();
                if v & 2 != 0 {
                    if self.enabled && !self.stalled {
                        if let Flight::Read { .. } = self.flight {
                            self.owed = self.owed.saturating_sub(1);
                        }
                        self.flight = Flight::None;
                        self.abort(SOURCE_USER, sched::now_ns());
                    }
                    return;
                }
                let on = v & 1 != 0;
                if on && !self.enabled {
                    self.enable();
                }
                if !on && self.enabled {
                    assert!(!self.active, "[bus model] the I2C block disabled in the middle of a transfer");
                    self.enabled = false;
                    self.rest_ns = sched::now_ns() + STATUS_LAG_NS;
                    self.owed = 0;
                    self.tx_len = 0;
                    self.rx_len = 0;
                    self.flight = Flight::None;
                    self.source &= (1 << FLUSH_SHIFT) - 1;
                }
            }
            _ => {}
        }
    }
}


const SSPCR0: usize = 0x00;
const SSPCR1: usize = 0x04;
const SSPDR: usize = 0x08;
const SSPSR: usize = 0x0C;
const SSPCPSR: usize = 0x10;
const SSPIMSC: usize = 0x14;
const SSPRIS: usize = 0x18;
const SSPICR: usize = 0x20;
const SPI_DEPTH: usize = 8;
const CR0_SPO: u32 = 1 << 6;
/// The clock line reaches a high idle level this long after the enable; the
/// doctored line reaches its idle level, at either polarity, this long after.
const CLOCK_RISE_NS: u64 = 250;
const CLOCK_LATE_NS: u64 = 50_000;

/// The memory: the first byte under the select is the command; the
/// identity command answers three bytes then zeros; any other command
/// echoes each byte in its own frame; a byte clocked with no select
/// asserted answers ones.
struct Memory {
    selected: bool,
    cmd: Option<u8>,
    idx: usize,
}

const MEMORY_ID: [u8; 3] = [0xEF, 0x40, 0x18];

impl Memory {
    fn select(&mut self, low: bool) {
        self.selected = low;
        if !low {
            self.cmd = None;
            self.idx = 0;
        }
    }

    fn exchange(&mut self, b: u8) -> u8 {
        if !self.selected {
            return 0xFF;
        }
        match self.cmd {
            None => {
                self.cmd = Some(b);
                0xFF
            }
            Some(0x9F) => {
                let a = MEMORY_ID.get(self.idx).copied().unwrap_or(0);
                self.idx += 1;
                a
            }
            Some(_) => b,
        }
    }
}

struct SpiModel {
    up: bool,
    cr0: u32,
    cpsr: u32,
    sse: bool,
    tx: [u8; SPI_DEPTH],
    tx_len: usize,
    rx: [u8; SPI_DEPTH],
    rx_len: usize,
    in_flight: Option<u8>,
    done_ns: u64,
    byte_ns: u64,
    /// The time the clock line reaches the mode's idle level.
    clock_ns: u64,
    drops: u32,
    ovr: bool,
    /// What the doctored image armed, each spent by the event it names.
    doctor_reset: bool,
    doctor_clock: bool,
    doctor_overrun: bool,
    dev: Memory,
}

static mut SPI: SpiModel = SpiModel {
    up: false,
    cr0: 0,
    cpsr: 0,
    sse: false,
    tx: [0; SPI_DEPTH],
    tx_len: 0,
    rx: [0; SPI_DEPTH],
    rx_len: 0,
    in_flight: None,
    done_ns: 0,
    byte_ns: 0,
    clock_ns: 0,
    drops: 0,
    ovr: false,
    doctor_reset: false,
    doctor_clock: false,
    doctor_overrun: false,
    dev: Memory { selected: false, cmd: None, idx: 0 },
};

fn spi() -> &'static mut SpiModel {
    unsafe { &mut *addr_of_mut!(SPI) }
}

impl SpiModel {
    /// The reset controller's bit cycled: the block starts over, its queues
    /// empty; the device behind it and the overrun count keep their state.
    fn reset(&mut self) {
        self.up = true;
        self.cr0 = 0;
        self.cpsr = 0;
        self.sse = false;
        self.tx_len = 0;
        self.rx_len = 0;
        self.in_flight = None;
        self.ovr = false;
    }

    /// The mode's idle level of the clock line: high at polarity 1.
    fn idle_high(&self) -> bool {
        self.cr0 & CR0_SPO != 0
    }

    /// The clock line's level: low while the block is disabled; once it is
    /// enabled, away from the mode's idle level until its time, at it after.
    fn clock_high(&self) -> bool {
        self.sse && ((sched::now_ns() >= self.clock_ns) == self.idle_high())
    }

    /// The wire moved to now: a byte in flight completes at its time and the
    /// device's answer lands in the receive queue (or is lost to the overrun
    /// when the queue is full); the next queued byte takes the wire.
    fn advance(&mut self) {
        let now = sched::now_ns();
        loop {
            if let Some(b) = self.in_flight {
                if now < self.done_ns {
                    break;
                }
                let a = self.dev.exchange(b);
                if self.rx_len < SPI_DEPTH {
                    self.rx[self.rx_len] = a;
                    self.rx_len += 1;
                } else {
                    self.drops += 1;
                    self.ovr = true;
                }
                self.in_flight = None;
            }
            if self.sse && self.tx_len > 0 {
                let b = self.tx[0];
                self.tx.copy_within(1..self.tx_len, 0);
                self.tx_len -= 1;
                self.in_flight = Some(b);
                self.done_ns = core::cmp::max(self.done_ns, now) + self.byte_ns;
            } else {
                break;
            }
        }
    }

    fn rd(&mut self, off: usize) -> u32 {
        assert!(self.up, "[bus model] the SPI block read at {:#x} while in reset", off);
        self.advance();
        match off {
            SSPCR0 => self.cr0,
            SSPCR1 => (self.sse as u32) << 1,
            SSPDR => {
                if self.rx_len == 0 {
                    return 0;
                }
                let b = self.rx[0];
                self.rx.copy_within(1..self.rx_len, 0);
                self.rx_len -= 1;
                b as u32
            }
            SSPSR => {
                let busy = self.sse && (self.in_flight.is_some() || self.tx_len > 0);
                ((self.tx_len == 0) as u32)
                    | ((self.tx_len < SPI_DEPTH) as u32) << 1
                    | ((self.rx_len > 0) as u32) << 2
                    | ((self.rx_len == SPI_DEPTH) as u32) << 3
                    | (busy as u32) << 4
            }
            SSPCPSR => self.cpsr,
            SSPRIS => (self.ovr as u32) | ((self.rx_len >= SPI_DEPTH / 2) as u32) << 2 | ((self.tx_len <= SPI_DEPTH / 2) as u32) << 3,
            _ => 0,
        }
    }

    fn wr(&mut self, off: usize, v: u32) {
        assert!(self.up, "[bus model] the SPI block written at {:#x} while in reset", off);
        self.advance();
        match off {
            SSPCR0 => {
                assert!(!self.sse, "[bus model] the SPI control register 0 written while the block is enabled");
                self.cr0 = v & 0xFFFF;
            }
            SSPCPSR => {
                assert!(!self.sse, "[bus model] the SPI prescaler written while the block is enabled");
                self.cpsr = v & 0xFF;
            }
            SSPCR1 => {
                assert!(v & !2 == 0, "[bus model] the SPI control register 1 written {:#x}: the model serves a master with no loopback", v);
                let sse = v & 2 != 0;
                if sse && !self.sse {
                    assert!(self.cr0 & 0x3F == 0x07, "[bus model] the SPI frame {:#x} is not eight bits in the Motorola format", self.cr0 & 0x3F);
                    assert!(self.cpsr >= 2 && self.cpsr & 1 == 0, "[bus model] the SPI prescaler {} is not an even number from 2", self.cpsr);
                    let rate = facts::SPI_BUSES[0].kernel_hz / (self.cpsr * (1 + ((self.cr0 >> 8) & 0xFF)));
                    assert!(rate > 0, "[bus model] the SPI prescaler pair makes no rate");
                    self.byte_ns = 8_000_000_000 / rate as u64;
                    let late = if DOCTORED {
                        CLOCK_LATE_NS
                    } else if self.idle_high() {
                        CLOCK_RISE_NS
                    } else {
                        0
                    };
                    self.clock_ns = sched::now_ns() + late;
                    if core::mem::take(&mut self.doctor_clock) {
                        self.clock_ns = u64::MAX;
                    }
                    if core::mem::take(&mut self.doctor_overrun) {
                        self.ovr = true;
                    }
                }
                if !sse {
                    self.in_flight = None;
                }
                self.sse = sse;
            }
            SSPDR => {
                assert!(self.tx_len < SPI_DEPTH, "[bus model] an SPI byte written to a full queue");
                self.tx[self.tx_len] = v as u8;
                self.tx_len += 1;
                self.advance();
            }
            SSPIMSC => assert!(v == 0, "[bus model] an SPI interrupt unmasked ({:#x})", v),
            SSPICR => {
                if v & 1 != 0 {
                    self.ovr = false;
                }
            }
            _ => {}
        }
    }
}
