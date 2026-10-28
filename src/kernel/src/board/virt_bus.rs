//! The emulator rows' bus backend: register-level models of the ST bus
//! blocks behind the register seam, so the family drivers run on QEMU
//! against the same register files the boards carry -- this row models the
//! I2C register file the two ST families share (the enable and the clock
//! selection cells are each family's own) and one SPI file: the STM32F7's
//! sixteen-bit file with four-byte queues on the Cortex-M7 twin, the
//! STM32U5's file with sixteen-byte queues and a transfer-size counter on
//! the Cortex-M33 twin. Behind the blocks, two devices: a sensor at 0x76
//! on the I2C model answering its identity register and a burst, and a
//! memory on the SPI model answering the identity command and echoing
//! every other byte. The wire is PACED by the kernel's clock -- a byte
//! completes after its bits' time at the configured rate, never earlier --
//! so the drivers' budgets are proven against real wire times. A block
//! whose clock is off answers no flag; a timing word or a frame size the
//! model does not serve is refused loudly (a driver's mistake caught on
//! the emulator). The STM32U5's file raises its mode fault as the block
//! does on the chip: at the write that leaves a master with the
//! software select management and a low internal select, whether or not
//! the block is enabled -- the fault flag set, the master bit cleared, the
//! enable refused until the flag-clear bit clears the fault. The pad seam
//! is empty but for the I2C recovery's (the models have no other pin): the
//! recovery's clock released after less than the specification's 35 ms of
//! low is refused loudly. The
//! doctored feature `test-bus-doctored` makes the sensor refuse the second
//! data byte of a write, the I2C block never complete a non-last message,
//! and the drivers' pumps unbounded -- the negative controls the api
//! flavor's doctored image runs -- and that image arms, through `doctor`,
//! a device that holds the clock low 25 ms in one byte and then answers, and
//! a device left holding the data line low at the block's next reset: until
//! three clocks have come, until the clock has been held low 35 ms, or for
//! good.

use core::ptr::addr_of_mut;

use crate::board::facts;
use crate::bus_math::{I2cBus, Pad, SpiBus};
use crate::critical::with_cs;
use crate::sched;

const DOCTORED: bool = cfg!(feature = "test-bus-doctored");

// ---------------- the pad seam: the I2C recovery's two lines alone ----------------

/// What the doctored image arms before a transfer: a device that holds the
/// clock low 25 ms in the next byte and then answers; a device left holding
/// the data line low at the block's next reset -- until three clocks have
/// come, until the clock has been held low 35 ms, or for good; the line free
/// again.
#[allow(dead_code)] // the api flavor's doctored image
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Doctor {
    Stretch,
    HeldClocks,
    HeldLong,
    HeldStuck,
    Free,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Held {
    No,
    Clocks(u32),
    Long,
    Stuck,
}

/// The modelled wire: what holds the data line now, what will at the
/// block's next reset, whether the next byte is stretched, and when the
/// recovery's clock went low.
struct Wire {
    held: Held,
    next: Held,
    stretch: bool,
    low_ns: u64,
}

static mut WIRE: Wire = Wire { held: Held::No, next: Held::No, stretch: false, low_ns: 0 };

fn wire() -> &'static mut Wire {
    unsafe { &mut *addr_of_mut!(WIRE) }
}

/// The clock held low in one byte by the doctored device, and the low time
/// the specification's remedy asks of the recovery's clock.
const STRETCH_NS: u64 = 25_000_000;
const HOLD_NS: u64 = 35_000_000;

#[allow(dead_code)] // the api flavor's doctored image
pub fn doctor(what: Doctor) {
    let w = wire();
    match what {
        Doctor::Stretch => w.stretch = true,
        Doctor::HeldClocks => w.next = Held::Clocks(3),
        Doctor::HeldLong => w.next = Held::Long,
        Doctor::HeldStuck => w.next = Held::Stuck,
        Doctor::Free => {
            w.held = Held::No;
            w.next = Held::No;
        }
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

/// A chip select driven: the first select reaches the memory model; the
/// second reaches nothing (a byte clocked under it answers ones).
pub fn select_drive(_d: &SpiBus, index: u32, low: bool) {
    if index == 0 {
        spi().dev.select(low);
    }
}

// ---------------- the clock cells: the enable and selection registers ----------------

/// The reset and clock control registers the descriptors name, as cells
/// the drivers write and the models consult: a block whose enable bit is
/// clear answers no flag and takes no write.
struct Cells {
    addr: [usize; 4],
    value: [u32; 4],
}

static mut CELLS: Cells = Cells { addr: [0; 4], value: [0; 4] };

fn cells() -> &'static mut Cells {
    let c = unsafe { &mut *addr_of_mut!(CELLS) };
    if c.addr[0] == 0 {
        let i = &facts::I2C_BUSES[0];
        let s = &facts::SPI_BUSES[0];
        c.addr = [i.enable_reg, i.mux_reg, s.enable_reg, if s.mux_reg == 0 { usize::MAX } else { s.mux_reg }];
    }
    c
}

/// The cell a register address names. The cells are borrowed ONCE by the
/// caller and passed in: no second reference to the static is taken while
/// the first is live.
fn cell_index(c: &Cells, addr: usize) -> usize {
    for (k, a) in c.addr.iter().enumerate() {
        if *a == addr {
            return k;
        }
    }
    panic!("[bus model] no register at {:#010x}", addr);
}

fn enabled(reg: usize, bit: u32) -> bool {
    let c = cells();
    let k = cell_index(c, reg);
    c.value[k] & (1 << bit) != 0
}

// ---------------- the register seam ----------------

pub fn rd(addr: usize) -> u32 {
    let i = &facts::I2C_BUSES[0];
    let s = &facts::SPI_BUSES[0];
    if addr >= i.base && addr < i.base + 0x2C {
        if !enabled(i.enable_reg, i.enable_bit) {
            return 0;
        }
        return i2c().rd(addr - i.base);
    }
    if addr >= s.base && addr < s.base + 0x34 {
        if !enabled(s.enable_reg, s.enable_bit) {
            return 0;
        }
        return spi().rd(addr - s.base);
    }
    let c = cells();
    let k = cell_index(c, addr);
    c.value[k]
}

pub fn wr(addr: usize, v: u32) {
    let i = &facts::I2C_BUSES[0];
    let s = &facts::SPI_BUSES[0];
    if addr >= i.base && addr < i.base + 0x2C {
        if enabled(i.enable_reg, i.enable_bit) {
            i2c().wr(addr - i.base, v);
        }
        return;
    }
    if addr >= s.base && addr < s.base + 0x34 {
        if enabled(s.enable_reg, s.enable_bit) {
            spi().wr(addr - s.base, v);
        }
        return;
    }
    let c = cells();
    let k = cell_index(c, addr);
    c.value[k] = v;
}

/// A clock register's read-modify-write under the kernel critical section,
/// as the silicon backend's: those registers are shared between blocks, so
/// two threads claiming two buses at once must not lose one write.
pub fn rmw(addr: usize, clear: u32, set: u32) {
    with_cs(|| wr(addr, (rd(addr) & !clear) | set));
}

pub fn rd8(addr: usize) -> u8 {
    let s = &facts::SPI_BUSES[0];
    if addr >= s.base && addr < s.base + 0x34 && enabled(s.enable_reg, s.enable_bit) {
        return spi().rd8(addr - s.base);
    }
    0
}

pub fn wr8(addr: usize, v: u8) {
    let s = &facts::SPI_BUSES[0];
    if addr >= s.base && addr < s.base + 0x34 && enabled(s.enable_reg, s.enable_bit) {
        spi().wr8(addr - s.base, v);
    }
}

/// The models' counters the api flavor's doctored image reads: bytes the
/// SPI receive queue dropped on overflow; the I2C block's resets (the
/// enable bit raised again after the first time).
#[allow(dead_code)] // the api flavor's doctored image
pub fn probe_spi_drops() -> u32 {
    spi().core.drops
}

#[allow(dead_code)] // the api flavor's doctored image
pub fn probe_i2c_resets() -> u32 {
    i2c().resets
}

// ---------------- the I2C block model and the sensor behind it ----------------

/// The bits' time of one byte with its acknowledge at 100 kHz.
const I2C_BYTE_NS: u64 = 90_000;
const TIMING_100K_AT_16M: u32 = 0x3042_0F13;
const I2C_CR1: usize = 0x00;
const I2C_CR2: usize = 0x04;
const I2C_TIMINGR: usize = 0x10;
const I2C_ISR: usize = 0x18;
const I2C_ICR: usize = 0x1C;
const I2C_RXDR: usize = 0x24;
const I2C_TXDR: usize = 0x28;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Pending {
    None,
    Txis,
    Rxne,
    End,
    Nack,
}

/// The sensor: a register pointer, the identity register, the data burst;
/// every register write acknowledged (the reset among them). The doctored
/// feature refuses the second data byte of a write.
struct Sensor {
    ptr: u8,
}

const SENSOR_ADDR: u8 = 0x76;
const SENSOR_ID: u8 = 0x60;
const SENSOR_BURST: [u8; 8] = [0x80, 0x00, 0x00, 0x80, 0x00, 0x00, 0x80, 0x00];

impl Sensor {
    fn address(&mut self, addr: u8) -> bool {
        addr == SENSOR_ADDR
    }

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

struct I2cModel {
    pe: bool,
    ever_enabled: bool,
    resets: u32,
    cr2: u32,
    timingr: u32,
    txis: bool,
    rxne: bool,
    nackf: bool,
    stopf: bool,
    tc: bool,
    rxdr: u8,
    remaining: u32,
    moved: u32,
    read: bool,
    autoend: bool,
    active: bool,
    pending: Pending,
    due_ns: u64,
    dev: Sensor,
}

static mut I2C: I2cModel = I2cModel {
    pe: false,
    ever_enabled: false,
    resets: 0,
    cr2: 0,
    timingr: 0,
    txis: false,
    rxne: false,
    nackf: false,
    stopf: false,
    tc: false,
    rxdr: 0,
    remaining: 0,
    moved: 0,
    read: false,
    autoend: false,
    active: false,
    pending: Pending::None,
    due_ns: 0,
    dev: Sensor { ptr: 0 },
};

fn i2c() -> &'static mut I2cModel {
    unsafe { &mut *addr_of_mut!(I2C) }
}

impl I2cModel {
    /// The next flag after the bytes' time -- and, once, after the clock the
    /// doctored device holds low in that byte.
    fn schedule(&mut self, what: Pending, bytes: u64) {
        let stretch = if core::mem::take(&mut wire().stretch) { STRETCH_NS } else { 0 };
        self.pending = what;
        self.due_ns = sched::now_ns() + bytes * I2C_BYTE_NS + stretch;
    }

    /// The message's end: a stop where the automatic end was asked, else
    /// the transfer-complete flag -- which the doctored block never raises.
    fn end(&mut self) {
        self.active = false;
        if self.autoend {
            self.stopf = true;
        } else if !DOCTORED {
            self.tc = true;
        }
    }

    fn advance(&mut self) {
        if self.pending == Pending::None || sched::now_ns() < self.due_ns {
            return;
        }
        let what = self.pending;
        self.pending = Pending::None;
        match what {
            Pending::Txis => self.txis = true,
            Pending::Rxne => {
                self.rxdr = self.dev.read_byte();
                self.rxne = true;
                if self.remaining == 1 {
                    self.end();
                }
            }
            Pending::End => self.end(),
            Pending::Nack => {
                self.nackf = true;
                self.stopf = true;
                self.active = false;
            }
            Pending::None => {}
        }
    }

    fn clear_all(&mut self) {
        self.txis = false;
        self.rxne = false;
        self.nackf = false;
        self.stopf = false;
        self.tc = false;
        self.pending = Pending::None;
        self.active = false;
        self.remaining = 0;
    }

    fn rd(&mut self, off: usize) -> u32 {
        match off {
            I2C_CR1 => self.pe as u32,
            I2C_CR2 => self.cr2,
            I2C_TIMINGR => self.timingr,
            I2C_ISR => {
                self.advance();
                (self.txis as u32) << 1
                    | (self.rxne as u32) << 2
                    | (self.nackf as u32) << 4
                    | (self.stopf as u32) << 5
                    | (self.tc as u32) << 6
                    | (self.active as u32) << 15
            }
            I2C_RXDR => {
                self.advance();
                if !self.rxne {
                    return 0;
                }
                self.rxne = false;
                self.moved += 1;
                self.remaining -= 1;
                if self.remaining > 0 {
                    self.schedule(Pending::Rxne, 1);
                }
                self.rxdr as u32
            }
            _ => 0,
        }
    }

    fn wr(&mut self, off: usize, v: u32) {
        match off {
            I2C_CR1 => {
                let pe = v & 1 != 0;
                if pe && !self.pe {
                    assert!(self.timingr == TIMING_100K_AT_16M, "[bus model] the I2C timing word {:#010x} is not the 16 MHz standard-mode row", self.timingr);
                    if self.ever_enabled {
                        self.resets += 1;
                    }
                    self.ever_enabled = true;
                }
                if !pe {
                    self.clear_all();
                    // The block's reset: the doctored device armed for it is
                    // left in the middle of its byte, holding the data line.
                    let w = wire();
                    if self.pe && w.next != Held::No {
                        w.held = w.next;
                        w.next = Held::No;
                    }
                }
                self.pe = pe;
            }
            I2C_TIMINGR => self.timingr = v,
            I2C_CR2 => {
                if !self.pe {
                    return;
                }
                self.cr2 = v;
                if v & (1 << 14) != 0 {
                    self.pending = Pending::None;
                    self.active = false;
                    self.stopf = true;
                }
                if v & (1 << 13) != 0 {
                    let addr = ((v >> 1) & 0x7F) as u8;
                    self.remaining = (v >> 16) & 0xFF;
                    self.read = v & (1 << 10) != 0;
                    self.autoend = v & (1 << 25) != 0;
                    self.moved = 0;
                    self.tc = false;
                    self.txis = false;
                    self.rxne = false;
                    self.active = true;
                    if !self.dev.address(addr) {
                        self.schedule(Pending::Nack, 1);
                    } else if self.remaining == 0 {
                        self.schedule(Pending::End, 1);
                    } else if self.read {
                        self.schedule(Pending::Rxne, 2);
                    } else {
                        self.schedule(Pending::Txis, 1);
                    }
                }
            }
            I2C_TXDR => {
                if !self.active || self.read || self.remaining == 0 {
                    return;
                }
                self.txis = false;
                let ack = self.dev.write_byte(v as u8, self.moved);
                self.moved += 1;
                self.remaining -= 1;
                if !ack {
                    self.schedule(Pending::Nack, 1);
                } else if self.remaining > 0 {
                    self.schedule(Pending::Txis, 1);
                } else {
                    self.schedule(Pending::End, 1);
                }
            }
            I2C_ICR => {
                if v & (1 << 4) != 0 {
                    self.nackf = false;
                }
                if v & (1 << 5) != 0 {
                    self.stopf = false;
                }
            }
            _ => {}
        }
    }
}

// ---------------- the SPI models and the memory behind them ----------------

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

/// The part of an SPI block both files share: the two queues, the byte in
/// flight and its completion time, the frames completed, the overflow count.
struct SpiCore {
    depth: usize,
    tx: [u8; 16],
    tx_len: usize,
    rx: [u8; 16],
    rx_len: usize,
    in_flight: Option<u8>,
    done_ns: u64,
    byte_ns: u64,
    spe: bool,
    frames: u32,
    drops: u32,
    ovr: bool,
}

impl SpiCore {
    fn flush(&mut self) {
        self.tx_len = 0;
        self.rx_len = 0;
        self.in_flight = None;
    }

    fn tx_push(&mut self, b: u8) {
        if self.tx_len < self.depth {
            self.tx[self.tx_len] = b;
            self.tx_len += 1;
        }
    }

    fn tx_pop(&mut self) -> u8 {
        let b = self.tx[0];
        self.tx.copy_within(1..self.tx_len, 0);
        self.tx_len -= 1;
        b
    }

    fn rx_push(&mut self, b: u8) {
        if self.rx_len < self.depth {
            self.rx[self.rx_len] = b;
            self.rx_len += 1;
        } else {
            self.drops += 1;
            self.ovr = true;
        }
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

    /// The wire moved to now: a byte in flight completes at its time and
    /// the device's answer lands in the receive queue; the next queued byte
    /// takes the wire.
    fn advance(&mut self, dev: &mut Memory) {
        let now = sched::now_ns();
        loop {
            if let Some(b) = self.in_flight {
                if now < self.done_ns {
                    break;
                }
                let a = dev.exchange(b);
                self.rx_push(a);
                self.frames += 1;
                self.in_flight = None;
            }
            if self.spe && self.tx_len > 0 {
                let b = self.tx_pop();
                self.in_flight = Some(b);
                self.done_ns = core::cmp::max(self.done_ns, now) + self.byte_ns;
            } else {
                break;
            }
        }
    }

    fn set_rate(&mut self, kernel_hz: u32, divisor: u32) {
        let rate = kernel_hz >> (divisor + 1);
        self.byte_ns = 8_000_000_000 / rate as u64;
    }
}

#[cfg_attr(feature = "board-mps2-an500", allow(dead_code))] // the U5 file's fields, unread by the F7 file's row
struct SpiModel {
    core: SpiCore,
    dev: Memory,
    cr1: u32,
    cr2: u32,
    cfg1: u32,
    cfg2: u32,
    tsize: u32,
    written: u32,
    started: bool,
    modf: bool,
}

static mut SPI: SpiModel = SpiModel {
    core: SpiCore {
        depth: if cfg!(feature = "board-mps2-an500") { 4 } else { 16 },
        tx: [0; 16],
        tx_len: 0,
        rx: [0; 16],
        rx_len: 0,
        in_flight: None,
        done_ns: 0,
        byte_ns: 8_000,
        spe: false,
        frames: 0,
        drops: 0,
        ovr: false,
    },
    dev: Memory { selected: false, cmd: None, idx: 0 },
    cr1: 0,
    cr2: 0,
    cfg1: 0,
    cfg2: 0,
    tsize: 0,
    written: 0,
    started: false,
    modf: false,
};

fn spi() -> &'static mut SpiModel {
    unsafe { &mut *addr_of_mut!(SPI) }
}

/// The STM32F7's sixteen-bit file on the Cortex-M7 twin.
#[cfg(feature = "board-mps2-an500")]
impl SpiModel {
    fn rd(&mut self, off: usize) -> u32 {
        match off {
            0x00 => self.cr1,
            0x04 => self.cr2,
            0x08 => {
                self.core.advance(&mut self.dev);
                let txe = self.core.tx_len <= self.core.depth / 2;
                let rxne = self.core.rx_len >= 1;
                let bsy = self.core.in_flight.is_some() || self.core.tx_len > 0;
                (rxne as u32) | (txe as u32) << 1 | (self.core.ovr as u32) << 6 | (bsy as u32) << 7
            }
            _ => 0,
        }
    }

    fn wr(&mut self, off: usize, v: u32) {
        match off {
            0x00 => {
                let spe = v & (1 << 6) != 0;
                if spe && !self.core.spe {
                    assert!(self.cr2 & 0x0F00 == 0x0700 && self.cr2 & (1 << 12) != 0, "[bus model] the SPI frame size is not eight bits with the receive threshold at one byte");
                    self.core.set_rate(facts::SPI_BUSES[0].kernel_hz, (v >> 3) & 7);
                }
                if !spe && self.core.spe {
                    self.core.flush();
                }
                self.core.spe = spe;
                self.cr1 = v;
            }
            0x04 => self.cr2 = v,
            _ => {}
        }
    }

    fn rd8(&mut self, off: usize) -> u8 {
        if off == 0x0C {
            self.core.advance(&mut self.dev);
            return self.core.rx_pop();
        }
        0
    }

    fn wr8(&mut self, off: usize, v: u8) {
        if off == 0x0C && self.core.spe {
            self.core.tx_push(v);
            self.core.advance(&mut self.dev);
        }
    }
}

/// The STM32U5's file on the Cortex-M33 twin.
#[cfg(feature = "board-mps2-an505")]
impl SpiModel {
    /// The mode fault, judged at every write of the two registers it reads:
    /// a master with the software select management whose internal select
    /// is low, enabled or not.
    fn mode_fault_check(&mut self) {
        if self.cfg2 & (1 << 22) != 0 && self.cfg2 & (1 << 26) != 0 && self.cr1 & (1 << 12) == 0 {
            self.modf = true;
            self.cfg2 &= !(1 << 22);
            self.cr1 &= !1;
            if self.core.spe {
                self.core.flush();
                self.started = false;
            }
            self.core.spe = false;
        }
    }

    fn rd(&mut self, off: usize) -> u32 {
        match off {
            0x00 => self.cr1,
            0x04 => self.cr2,
            0x08 => self.cfg1,
            0x0C => self.cfg2,
            0x14 => {
                self.core.advance(&mut self.dev);
                let rxp = self.core.rx_len >= 1;
                let txp = self.core.tx_len < self.core.depth;
                let eot = self.started && self.core.frames >= self.tsize;
                let txtf = self.started && self.written >= self.tsize;
                (rxp as u32) | (txp as u32) << 1 | (eot as u32) << 3 | (txtf as u32) << 4 | (self.modf as u32) << 9
            }
            _ => 0,
        }
    }

    fn wr(&mut self, off: usize, v: u32) {
        match off {
            0x00 => {
                // The enable and the start are refused while the mode fault stands.
                let v = if self.modf { v & !0x201 } else { v };
                let spe = v & 1 != 0;
                if spe && !self.core.spe {
                    assert!(self.cfg1 & 0x1F == 7, "[bus model] the SPI frame size is not eight bits");
                    assert!((self.cfg1 >> 16) & 0x1F >= 3, "[bus model] the SPI CRC size field holds a value the block does not define");
                    self.core.set_rate(facts::SPI_BUSES[0].kernel_hz, (self.cfg1 >> 28) & 7);
                }
                if !spe && self.core.spe {
                    self.core.flush();
                    self.started = false;
                }
                self.core.spe = spe;
                if spe && v & (1 << 9) != 0 && !self.started {
                    self.started = true;
                    self.core.frames = 0;
                    self.written = 0;
                }
                self.cr1 = v;
                self.mode_fault_check();
            }
            0x04 => {
                self.cr2 = v;
                self.tsize = v & 0xFFFF;
            }
            0x08 => self.cfg1 = v,
            0x0C => {
                self.cfg2 = v;
                self.mode_fault_check();
            }
            0x18 => {
                if v & (1 << 3) != 0 {
                    self.started = false;
                }
                if v & (1 << 9) != 0 {
                    self.modf = false;
                }
            }
            _ => {}
        }
    }

    fn rd8(&mut self, off: usize) -> u8 {
        if off == 0x30 {
            self.core.advance(&mut self.dev);
            return self.core.rx_pop();
        }
        0
    }

    fn wr8(&mut self, off: usize, v: u8) {
        if off == 0x20 && self.core.spe {
            self.core.tx_push(v);
            self.written += 1;
            self.core.advance(&mut self.dev);
        }
    }
}
