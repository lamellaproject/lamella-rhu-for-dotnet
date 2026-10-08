//! The RP2350's bus backend: what the chip's bus drivers reach through the
//! seam on a board, where the Cortex-M33 emulator row puts register-level
//! models under its test feature. The register seam is a volatile access at
//! the address the board's facts state. A block is taken by cycling it
//! through the reset controller (the datasheet, section 7.5): its
//! wake-enable clock gates set, the block put into reset and released with
//! the done bit polled both ways under a bound, and whether both polls ended
//! is the answer -- a loader or a resident image may have left the block in
//! any state, and a reset is the one known one. The gates and the reset bit
//! are written through the registers' atomic set and clear aliases (section
//! 2.1.3: a register's address plus 0x2000 sets the bits written, plus
//! 0x3000 clears them), so two threads claiming two buses at once lose no
//! write and the reset takes no critical section.

use core::ptr::{addr_of, read_volatile, write_volatile};

use super::rp2350::gpio_backend::{ctrl, pad};
use super::rp2350::{CLOCKS, PAD_IE, PAD_ISO, PAD_OD, PAD_PDE, PAD_PUE, PERI_HZ, RESET_IO_BANK0, RESET_PADS_BANK0, SYSCLK_HZ};
use crate::board::gpio_backend as pins;
use crate::bus_math::{I2cBus, Pad, PadRole, SpiBus};
use crate::critical::with_cs;
use crate::gpio_math as g;
use crate::rp2350_bus_math as rp;
use crate::sched;

const _: () = assert!(PERI_HZ == rp::SPI_CLOCK_HZ, "the SPI rows' clock is not the peripheral clock the chip module runs");


pub fn rd(addr: usize) -> u32 {
    unsafe { read_volatile(addr as *const u32) }
}

pub fn wr(addr: usize, v: u32) {
    unsafe { write_volatile(addr as *mut u32, v) }
}

/// The atomic aliases of a register: its address plus these.
const ALIAS_SET: usize = 0x2000;
const ALIAS_CLR: usize = 0x3000;
/// The reset controller's done register, beside its reset register.
const RESET_DONE: usize = 0x08;
/// The clock block's two wake-enable registers.
const WAKE_EN0: usize = 0xAC;
const WAKE_EN1: usize = 0xB0;
/// A reset's entry and its release are each polled at most this many times
/// (about a tenth of a millisecond of reads at either posture).
const RESET_POLLS: u32 = SYSCLK_HZ / 10_000;

/// A block's wake-enable gates by its reset bit: the I2C blocks' system
/// clock (WAKE_EN0 bits 11 and 12); the SPI blocks' peripheral and system
/// clocks (WAKE_EN1 bits 0 and 1, 2 and 3).
const fn gates(reset_bit: u32) -> (usize, u32) {
    match reset_bit {
        4 => (WAKE_EN0, 1 << 11),
        5 => (WAKE_EN0, 1 << 12),
        18 => (WAKE_EN1, 0b11),
        _ => (WAKE_EN1, 0b11 << 2),
    }
}

/// A register's bits polled until they read as wanted: whether they did
/// within the bound.
fn polled(reg: usize, mask: u32, want: u32) -> bool {
    let mut n = 0;
    while rd(reg) & mask != want {
        if n == RESET_POLLS {
            return false;
        }
        n += 1;
    }
    true
}

/// A block cycled through its reset: the gates set, the two pad banks
/// released (idempotent, as the console's start releases them), the block
/// put into reset until its done bit reads 0, then released until it reads
/// 1. Returns whether both polls ended inside their bound.
pub fn block_reset(reset_reg: usize, bit: u32) -> bool {
    let mask = 1u32 << bit;
    let (gate_reg, gate_bits) = gates(bit);
    let done = reset_reg + RESET_DONE;
    wr(CLOCKS + ALIAS_SET + gate_reg, gate_bits);
    wr(reset_reg + ALIAS_CLR, RESET_IO_BANK0 | RESET_PADS_BANK0);
    wr(reset_reg + ALIAS_SET, mask);
    let entered = polled(done, mask, 0);
    wr(reset_reg + ALIAS_CLR, mask);
    let released = polled(done, mask, mask);
    entered && released
}


/// The pad register's fields beside the ones the chip module names: the
/// Schmitt trigger, the fast slew, the drive strength (01 is 4 mA).
const PAD_SCHMITT: u32 = 1 << 1;
const PAD_SLEWFAST: u32 = 1 << 0;
const PAD_DRIVE: u32 = 0b11 << 4;
const PAD_DRIVE_4MA: u32 = 0b01 << 4;

/// A released line settles through the pad's own pull-up before it is read.
const SETTLE_NS: u64 = 100_000;
/// Half a bit time at 100 kHz: the recovery's clock pulses and its stop.
const HALF_BIT_NS: u64 = 5_000;

/// A spin by the kernel's clock, interrupts free: in a recovery (at a claim,
/// or after a transfer timed out), never while a transfer moves.
fn spin(ns: u64) {
    let t0 = sched::now_ns();
    while sched::now_ns().wrapping_sub(t0) < ns {}
}

fn line_of(d: &I2cBus, role: PadRole) -> Option<u32> {
    d.pads.iter().find(|p| p.role == role).map(|p| p.line as u32)
}

/// A pad handed to its block: the pad written whole with its isolation latch
/// set, the function selected, the latch cleared.
pub fn pad_take(p: &Pad) {
    let line = p.line as u32;
    let pull = match p.role {
        PadRole::I2cClock | PadRole::I2cData | PadRole::SpiIn => PAD_PUE,
        PadRole::SpiClock | PadRole::SpiOut => 0,
    };
    unsafe {
        let r = pad(line);
        let keep = read_volatile(r) & !(PAD_ISO | PAD_OD | PAD_IE | PAD_DRIVE | PAD_PUE | PAD_PDE | PAD_SCHMITT | PAD_SLEWFAST);
        let v = keep | PAD_IE | PAD_DRIVE_4MA | PAD_SCHMITT | pull;
        write_volatile(r, v | PAD_ISO);
        write_volatile(ctrl(line), p.alternate as u32);
        write_volatile(r, v);
    }
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

/// A chip select taken: a plain output at its released level.
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

/// Every listed chip select that nobody holds -- neither a program nor the
/// board, by the claims table -- given its resting level: the pad's pull-up
/// set, its pull-down and its isolation latch cleared, its input enabled so
/// the level can be read. The table is read and the pad written under one
/// critical section; the line is not claimed and its function not touched.
pub fn select_rest(d: &SpiBus) {
    for &line in d.selects {
        let line = line as u32;
        with_cs(|| unsafe {
            let c = &*addr_of!(crate::gpio::CLAIMS);
            if crate::claims::is_program(c, line) || crate::claims::is_board(c, line) {
                return;
            }
            let r = pad(line);
            write_volatile(r, (read_volatile(r) & !(PAD_PDE | PAD_ISO)) | PAD_PUE | PAD_IE);
        });
    }
}

/// Whether the SPI clock pad reads at the level asked: the pad's input is
/// enabled when the pad is handed to the block, and the plain-line input
/// register follows the pad while the block drives it.
pub fn spi_clock_at(d: &SpiBus, high: bool) -> bool {
    match d.pads.iter().find(|p| p.role == PadRole::SpiClock) {
        Some(p) => pins::read(p.line as u32) == high,
        None => true,
    }
}
