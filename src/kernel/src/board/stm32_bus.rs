//! The silicon bus backend of ST's Cortex-M parts: what the family bus
//! drivers reach through the seam on a real board, where the emulator rows
//! put their register-level models. The register seam is a volatile access
//! at the address the board's facts state; a clock register's
//! read-modify-write takes the kernel critical section for its few
//! instructions, because those registers are shared between blocks (two
//! buses' enables are two bits of one register) and two threads claiming
//! two buses at once must not lose one write. The pad seam goes through the
//! family GPIO driver and the port block's alternate-function registers
//! (the reference manuals' GPIO chapter: the mode, output type, output
//! speed and pull registers and the alternate-function pair at the same
//! offsets on the families this kernel serves): a pad's electrical setup
//! follows its ROLE in the board's facts. The two I2C pads are open-drain
//! with the pad's own pull-up at the low speed setting; the SPI clock and
//! output are push-pull at the high speed setting (the datasheets' I/O
//! tables rate the medium setting below the fastest rate the SPI ladder can
//! ask); the SPI input is pulled up. The pads' own pull-ups define an EMPTY
//! bus's answers only -- a bus with nothing fitted idles released, an
//! address nobody answers reads as not acknowledged, a transfer with no
//! device reads ones -- so the kernel's answers are the same with and
//! without a device; a real bus brings its own pull-ups, and the pads' own
//! are not a bus's pull-ups at speed. A pad is made an input with its pull
//! before anything is read from it: on a part whose pads leave reset in
//! analog mode the input register reads 0 until then, which would read as a
//! data line held low. Nothing of a transfer runs under the critical section.

use core::ptr::{read_volatile, write_volatile};

use crate::board::facts;
use crate::board::gpio_backend as pins;
use crate::bus_math::{I2cBus, Pad, PadRole, SpiBus};
use crate::critical::with_cs;
use crate::gpio_math as g;
use crate::sched;

// ---------------- the register seam ----------------

pub fn rd(addr: usize) -> u32 {
    unsafe { read_volatile(addr as *const u32) }
}

pub fn wr(addr: usize, v: u32) {
    unsafe { write_volatile(addr as *mut u32, v) }
}

pub fn rd8(addr: usize) -> u8 {
    unsafe { read_volatile(addr as *const u8) }
}

pub fn wr8(addr: usize, v: u8) {
    unsafe { write_volatile(addr as *mut u8, v) }
}

/// A shared register's read-modify-write under the kernel critical section,
/// read back so a clock enable has taken effect before its block is touched.
pub fn rmw(addr: usize, clear: u32, set: u32) {
    with_cs(|| {
        wr(addr, (rd(addr) & !clear) | set);
        let _ = rd(addr);
    });
}

// ---------------- the pad seam ----------------

/// The port block's registers beside the ones the family GPIO driver
/// programs: the output type, the output speed and the alternate-function
/// pair (pins 0 to 7 in the low register, 8 to 15 in the high one).
const MODER: usize = 0x00;
const OTYPER: usize = 0x04;
const OSPEEDR: usize = 0x08;
const AFRL: usize = 0x20;
const MODE_ALTERNATE: u32 = 0b10;
const SPEED_LOW: u32 = 0b00;
const SPEED_HIGH: u32 = 0b10;

/// A released line settles through the pad's own pull-up before it is read.
const SETTLE_NS: u64 = 100_000;
/// Half a bit time at 100 kHz: the recovery's clock pulses and its stop.
const HALF_BIT_NS: u64 = 5_000;

fn port_reg(line: u32, off: usize) -> usize {
    let (port, _) = g::st_split(line);
    facts::GPIO_BASE + port as usize * 0x400 + off
}

/// A spin by the kernel's clock, interrupts free: in a recovery (at a claim,
/// or after a transfer timed out), never while a transfer moves.
fn spin(ns: u64) {
    let t0 = sched::now_ns();
    while sched::now_ns().wrapping_sub(t0) < ns {}
}

fn line_of(d: &I2cBus, role: PadRole) -> Option<u32> {
    d.pads.iter().find(|p| p.role == role).map(|p| p.line as u32)
}

/// A pad handed to its block: an input with its pull first (the family GPIO
/// driver enables the port's clock and programs the pull), then under one
/// section the output type, the speed, the alternate function and LAST the
/// mode, so the pad leaves the plain-line state in one step.
pub fn pad_take(p: &Pad) {
    let (open_drain, pull, speed) = match p.role {
        PadRole::I2cClock | PadRole::I2cData => (true, g::PULL_UP, SPEED_LOW),
        PadRole::SpiClock | PadRole::SpiOut => (false, g::PULL_NONE, SPEED_HIGH),
        PadRole::SpiIn => (false, g::PULL_UP, SPEED_LOW),
    };
    pad_take_as(p.line as u32, p.alternate, open_drain, pull, speed);
}

/// A pad handed to a block at a stated electrical setup (the bus pads'
/// steps above; the card socket's six pads come through here too,
/// push-pull without a pull at the very high speed setting): the output
/// type, the speed in OSPEEDR's two bits, the alternate function and LAST
/// the mode.
pub fn pad_take_as(line: u32, alternate: u8, open_drain: bool, pull: u32, speed: u32) {
    pins::configure(line, g::MODE_INPUT, pull, false);
    let (_, bit) = g::st_split(line);
    with_cs(|| {
        let otyper = port_reg(line, OTYPER);
        wr(otyper, (rd(otyper) & !(1 << bit)) | ((open_drain as u32) << bit));
        let ospeedr = port_reg(line, OSPEEDR);
        wr(ospeedr, (rd(ospeedr) & !(0b11 << (2 * bit))) | (speed << (2 * bit)));
        let afr = port_reg(line, AFRL + 4 * (bit as usize / 8));
        let shift = 4 * (bit % 8);
        wr(afr, (rd(afr) & !(0xF << shift)) | ((alternate as u32) << shift));
        let moder = port_reg(line, MODER);
        wr(moder, (rd(moder) & !(0b11 << (2 * bit))) | (MODE_ALTERNATE << (2 * bit)));
    });
}

/// Whether a device holds the data line low, read while the two pads are
/// still plain lines: both made inputs with their pull-ups, the settle, the
/// data line's level.
pub fn i2c_sda_low(d: &I2cBus) -> bool {
    let (Some(scl), Some(sda)) = (line_of(d, PadRole::I2cClock), line_of(d, PadRole::I2cData)) else {
        return false;
    };
    pins::configure(scl, g::MODE_INPUT, g::PULL_UP, false);
    pins::configure(sda, g::MODE_INPUT, g::PULL_UP, false);
    spin(SETTLE_NS);
    !pins::read(sda)
}

/// One recovery clock: the clock pad an open-drain output, low for half a
/// bit time, released for half a bit time.
pub fn i2c_scl_pulse(d: &I2cBus) {
    let Some(scl) = line_of(d, PadRole::I2cClock) else {
        return;
    };
    pins::configure(scl, g::MODE_OUTPUT_OPEN_DRAIN, g::PULL_UP, false);
    spin(HALF_BIT_NS);
    pins::write(scl, g::MODE_OUTPUT_OPEN_DRAIN, true);
    spin(HALF_BIT_NS);
}

/// The recovery's long clock: the clock pad an open-drain output driven low,
/// or released; the caller times the hold between the two.
pub fn i2c_scl_hold(d: &I2cBus, low: bool) {
    let Some(scl) = line_of(d, PadRole::I2cClock) else {
        return;
    };
    if low {
        pins::configure(scl, g::MODE_OUTPUT_OPEN_DRAIN, g::PULL_UP, false);
    } else {
        pins::write(scl, g::MODE_OUTPUT_OPEN_DRAIN, true);
        spin(HALF_BIT_NS);
    }
}

/// The recovery's stop: the data pad low while the clock is released, then
/// released -- the data line rising with the clock high is the stop.
pub fn i2c_stop(d: &I2cBus) {
    let Some(sda) = line_of(d, PadRole::I2cData) else {
        return;
    };
    pins::configure(sda, g::MODE_OUTPUT_OPEN_DRAIN, g::PULL_UP, false);
    spin(HALF_BIT_NS);
    pins::write(sda, g::MODE_OUTPUT_OPEN_DRAIN, true);
    spin(HALF_BIT_NS);
}

/// A chip select taken: a push-pull output at its released level.
pub fn select_take(d: &SpiBus, index: u32) {
    if let Some(&line) = d.selects.get(index as usize) {
        pins::configure(line as u32, g::MODE_OUTPUT, g::PULL_NONE, true);
    }
}

/// A chip select driven: low asserts it.
pub fn select_drive(d: &SpiBus, index: u32, low: bool) {
    if let Some(&line) = d.selects.get(index as usize) {
        pins::write(line as u32, g::MODE_OUTPUT, !low);
    }
}
