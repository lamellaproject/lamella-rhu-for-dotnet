//! The family I2C driver: one register file on the STM32F7 and the STM32U5
//! (the reference manuals' I2C chapter: control register 1 with the enable,
//! control register 2 with the address, the byte count, the direction, the
//! start, the stop and the automatic end, the timing register, the status
//! and clear registers, the two data registers), driven over the register
//! seam so the same code runs against the emulator rows' model and the
//! boards' blocks. A transaction is one combined transfer: every message
//! its own address phase, a repeated start between messages, a stop after
//! the last. Every wait is a poll of the status register, a byte at a time,
//! against its own budget (two bytes' wire time plus the 25 ms the SMBus
//! specification lets a device hold the clock low) and the caller's
//! deadline; a budget passed asserts
//! a stop, clears the flags and resets the block by its enable bit -- the
//! manuals' sequence (RM0410 33.4.6, RM0456 65.4.6): the enable written 0,
//! read back until it reads 0 (the block needs it low for three cycles of
//! its bus clock), then written 1 -- and, while the enable is 0 and the
//! block is off its lines, the bus is recovered as at the claim: a device
//! left holding the data line low is clocked free, or failing that reset by
//! the clock held low 35 ms. Nothing of a transfer runs under the
//! kernel critical section; the claim's two clock writes are
//! read-modify-writes of registers other blocks share, so the seam takes
//! the section for those few instructions.

use crate::board::bus_backend as hw;
use crate::bus::{i2c_recover, wait_until, Fault, Outcome, Recovery};
use crate::bus_math::{I2cBus, I2cMsg, MSG_READ};
use crate::sched;

const CR1: usize = 0x00;
const CR2: usize = 0x04;
const TIMINGR: usize = 0x10;
const ISR: usize = 0x18;
const ICR: usize = 0x1C;
const RXDR: usize = 0x24;
const TXDR: usize = 0x28;

const CR1_PE: u32 = 1 << 0;
const CR2_RD_WRN: u32 = 1 << 10;
const CR2_START: u32 = 1 << 13;
const CR2_STOP: u32 = 1 << 14;
const CR2_NBYTES_SHIFT: u32 = 16;
const CR2_AUTOEND: u32 = 1 << 25;
const ISR_TXIS: u32 = 1 << 1;
const ISR_RXNE: u32 = 1 << 2;
const ISR_NACKF: u32 = 1 << 4;
const ISR_STOPF: u32 = 1 << 5;
const ISR_TC: u32 = 1 << 6;
const ISR_BERR: u32 = 1 << 8;
const ISR_ARLO: u32 = 1 << 9;
const ICR_ALL: u32 = ISR_NACKF | ISR_STOPF | ISR_BERR | ISR_ARLO;
const ISR_FAULTS: u32 = ISR_NACKF | ISR_BERR | ISR_ARLO;
/// A wait spins for two byte times at 100 kHz before its first yield.
const SPIN_NS: u64 = 2 * 9 * 10_000;

/// The bus taken: the block's clock enabled and its kernel clock selected,
/// the bus recovered through the pads while they are still plain lines (a
/// device left mid-read holds the data line low: up to nine clocks free it,
/// then a stop), the pads handed to the block, the block enabled at the
/// timing word. Returns whether a recovery happened.
pub fn claim(d: &I2cBus, timing: u32) -> bool {
    hw::rmw(d.enable_reg, 0, 1 << d.enable_bit);
    hw::rmw(d.mux_reg, 0b11 << d.mux_shift, d.mux_value << d.mux_shift);
    let recovered = i2c_recover(d, false) != Recovery::None;
    for p in d.pads {
        hw::pad_take(p);
    }
    disable(d);
    hw::wr(d.base + TIMINGR, timing);
    hw::wr(d.base + CR1, CR1_PE);
    recovered
}

/// The block disabled the way the manuals spell it: the enable written 0 and
/// read back until it reads 0 (a bounded number of reads; the read itself
/// keeps the bit low for the bus clock cycles the block's reset needs).
fn disable(d: &I2cBus) {
    hw::wr(d.base + CR1, 0);
    for _ in 0..16 {
        if hw::rd(d.base + CR1) & CR1_PE == 0 {
            break;
        }
    }
}

/// The block reset after a wait passed its budget: a stop asserted, the
/// flags cleared, the enable bit dropped (the state machine's reset; the
/// block is off its lines while it is 0), the bus recovered through the pads
/// as plain lines, the pads handed back, the enable raised. Returns what the
/// recovery found.
fn reset(d: &I2cBus) -> Recovery {
    hw::wr(d.base + CR2, CR2_STOP);
    hw::wr(d.base + ICR, ICR_ALL);
    disable(d);
    let recovery = i2c_recover(d, true);
    for p in d.pads {
        hw::pad_take(p);
    }
    hw::wr(d.base + CR1, CR1_PE);
    recovery
}

/// A fault read off the status register: the acknowledge missing, a bus
/// error, arbitration lost -- cleared, and the block's own stop awaited
/// where it sends one (the acknowledge missing). The code by the fault and
/// by whether a byte of the message had moved.
fn fault(d: &I2cBus, isr: u32, budget: u32, deadline_ns: u64, i: u32, done: u32, out: &mut Outcome) -> i32 {
    out.message = i;
    out.byte = done.saturating_sub(1);
    if isr & ISR_NACKF != 0 {
        let _ = wait_until(budget, deadline_ns, SPIN_NS, || hw::rd(d.base + ISR) & ISR_STOPF != 0);
        hw::wr(d.base + ICR, ISR_NACKF | ISR_STOPF);
        if done == 0 {
            return crate::bus::ERR_NXIO;
        }
        out.kind = Fault::DataNack;
        return sched::ERR_IO;
    }
    hw::wr(d.base + ICR, ICR_ALL);
    out.kind = if isr & ISR_BERR != 0 { Fault::BusError } else { Fault::ArbitrationLost };
    sched::ERR_IO
}

/// One combined transaction. `_hz` is the claimed rate, which this family
/// takes and ignores (its one rate is its timing word); `budget_ticks` each
/// wire wait's budget; `deadline_ns` the caller's bound on the whole
/// transaction.
pub fn transfer(d: &I2cBus, msgs: &[I2cMsg], _hz: u32, budget_ticks: u32, deadline_ns: u64, out: &mut Outcome) -> i32 {
    let base = d.base;
    let n = msgs.len();
    for (i, msg) in msgs.iter().enumerate() {
        let last = i + 1 == n;
        let read = msg.flags & MSG_READ != 0;
        let len = msg.len as u32;
        let mut cr2 = ((msg.addr as u32) << 1) | (len << CR2_NBYTES_SHIFT) | CR2_START;
        if read {
            cr2 |= CR2_RD_WRN;
        }
        if last {
            cr2 |= CR2_AUTOEND;
        }
        hw::wr(base + CR2, cr2);
        let mut done = 0u32;
        while done < len {
            let want = if read { ISR_RXNE } else { ISR_TXIS };
            if !wait_until(budget_ticks, deadline_ns, SPIN_NS, || hw::rd(base + ISR) & (want | ISR_FAULTS) != 0) {
                out.recovery = reset(d);
                out.message = i as u32;
                out.byte = done;
                out.kind = Fault::Budget;
                return sched::ERR_TIMEDOUT;
            }
            let isr = hw::rd(base + ISR);
            if isr & ISR_FAULTS != 0 {
                return fault(d, isr, budget_ticks, deadline_ns, i as u32, done, out);
            }
            if read {
                unsafe { msg.buf.add(done as usize).write(hw::rd(base + RXDR) as u8) };
            } else {
                hw::wr(base + TXDR, unsafe { msg.buf.add(done as usize).read() } as u32);
            }
            done += 1;
        }
        let end = if last { ISR_STOPF } else { ISR_TC };
        if !wait_until(budget_ticks, deadline_ns, SPIN_NS, || hw::rd(base + ISR) & (end | ISR_FAULTS) != 0) {
            out.recovery = reset(d);
            out.message = i as u32;
            out.byte = done;
            out.kind = Fault::Budget;
            return sched::ERR_TIMEDOUT;
        }
        let isr = hw::rd(base + ISR);
        if isr & ISR_FAULTS != 0 {
            return fault(d, isr, budget_ticks, deadline_ns, i as u32, done, out);
        }
        if last {
            hw::wr(base + ICR, ISR_STOPF);
        }
    }
    0
}

/// The timing word for a rate at the block's kernel clock: this family's row
/// of the bus arithmetic, asked by the bus doors through the driver.
pub use crate::bus_math::i2c_timing as timing;
