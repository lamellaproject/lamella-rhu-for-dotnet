//! The bus doors' bodies: a program claims an I2C or SPI bus for its whole
//! run and moves bytes over it -- the eight verbs `api.rs` exports and
//! `include/rhu.h` states. A bus is named by its NUMBER, the chip's own
//! block number (I2C1 is bus 1, SPI2 is bus 2), as a Linux device node is
//! named; the count doors say how many buses the board has and the bus
//! doors answer the number of the index-th, so a program enumerates
//! without guessing; a number naming no bus of the board answers
//! RHU_ENODEV. Beneath them: the board's bus tables
//! (`board::facts::I2C_BUSES` and `SPI_BUSES`), the claims table (a bus's
//! pads and a chip select become the board's at the claim or at the
//! select's first use, so no program can take them away), the family
//! drivers over the register seam, and the tenancy: one word per bus that
//! IS a futex word, taken by compare-and-swap, slept on through the
//! scheduler's futex by a thread that finds it held, released with a wake.
//! Nothing of a transaction runs under the kernel critical section (the
//! claims' few instructions are the only section, as every claim's); every
//! wait on the wire is a poll against a budget, yielding between polls, and
//! a budget passed resets the block. An I2C wait's budget is the wire time
//! of two bytes plus the 25 ms the SMBus specification lets a device hold
//! the clock low; after an I2C budget the data line is read, and a device
//! left holding it low is given up to nine clocks and a stop before the
//! call returns.
//!
//! The witness `__rhu_bus_witness` is read by symbol (a debugger, the api
//! flavor's checks). The kernel's lines, each a rare event: the bus
//! recovered at a claim; a data byte not acknowledged, a bus error or
//! arbitration lost (the message named); a wire wait past its budget (the
//! block reset), and after it what the recovery found when a device held
//! the data line. An address not acknowledged is a scan's answer: no line.

#![cfg_attr(not(rhu_bus_rows), allow(dead_code))] // a row without a bus compiles the doors' helpers unused

use core::ptr::{addr_of_mut, read_volatile, write_volatile};
use core::sync::atomic::{AtomicU32, AtomicU8, Ordering};

use crate::board::facts;
use crate::bus_math as m;
use crate::critical::with_cs;
use crate::sched;

pub const ERR_NXIO: i32 = m::ERR_NXIO;
pub const ERR_NOTSUP: i32 = -95;

const NI: usize = facts::I2C_BUSES.len();
const NS: usize = facts::SPI_BUSES.len();
const _: () = assert!(m::i2c_numbers_unique(facts::I2C_BUSES) && m::spi_numbers_unique(facts::SPI_BUSES), "two buses of one family carry one number");
const _: () = assert!(m::spi_selects_sound(facts::SPI_BUSES), "a bus offers more than eight selects, or its select list is neither empty nor as long as its count");
const _: () = assert!(core::mem::size_of::<m::I2cMsg>() == 12, "the I2C message record is twelve bytes");

/// Per bus: the claimed mode plus one (0 free; 1 standard, 2 fast, 3 fast
/// mode plus -- the rate's index in `I2C_RATES_HZ` plus one; an SPI bus 1),
/// and the tenancy word (0 free, 1 held) that is a futex word; per SPI bus
/// the chip selects already claimed, one bit each.
static I2C_CLAIMED: [AtomicU8; NI] = [const { AtomicU8::new(0) }; NI];
static I2C_TENANCY: [AtomicU32; NI] = [const { AtomicU32::new(0) }; NI];
static SPI_CLAIMED: [AtomicU8; NS] = [const { AtomicU8::new(0) }; NS];
static SPI_TENANCY: [AtomicU32; NS] = [const { AtomicU32::new(0) }; NS];
static SPI_SELECTS: [AtomicU8; NS] = [const { AtomicU8::new(0) }; NS];

/// The witness, by symbol.
#[repr(C)]
pub struct BusWitness {
    pub i2c_buses: u32,
    pub spi_buses: u32,
    pub i2c_claims: u32,
    pub i2c_transfers: u32,
    pub i2c_bytes: u32,
    pub spi_claims: u32,
    pub spi_transfers: u32,
    pub spi_bytes: u32,
    pub spi_last_hz: u32,
    pub tenancy_waits: u32,
    pub recoveries: u32,
    pub resets: u32,
    pub refused: u32,
    pub last_refused_code: i32,
    pub last_message: u32,
    pub last_byte: u32,
}

#[unsafe(export_name = "__rhu_bus_witness")]
pub static mut WITNESS: BusWitness = BusWitness {
    i2c_buses: 0,
    spi_buses: 0,
    i2c_claims: 0,
    i2c_transfers: 0,
    i2c_bytes: 0,
    spi_claims: 0,
    spi_transfers: 0,
    spi_bytes: 0,
    spi_last_hz: 0,
    tenancy_waits: 0,
    recoveries: 0,
    resets: 0,
    refused: 0,
    last_refused_code: 0,
    last_message: 0,
    last_byte: 0,
};

/// The witness's fields are written through raw pointers, one field at a
/// time, under the critical section; no reference to the static is ever
/// formed, so a reader's snapshot by value aliases nothing.
fn witness() -> *mut BusWitness {
    addr_of_mut!(WITNESS)
}

unsafe fn bump(field: *mut u32, by: u32) {
    unsafe { write_volatile(field, read_volatile(field).wrapping_add(by)) }
}

/// A refusal recorded: the count and the code; the message and byte
/// indices where the driver named them.
fn refused(code: i32, message: u32, byte: u32) -> i32 {
    with_cs(|| unsafe {
        let w = witness();
        bump(addr_of_mut!((*w).refused), 1);
        write_volatile(addr_of_mut!((*w).last_refused_code), code);
        write_volatile(addr_of_mut!((*w).last_message), message);
        write_volatile(addr_of_mut!((*w).last_byte), byte);
    });
    code
}

/// The tables' sizes recorded; nothing else moves at boot (a bus is
/// claimed by a program, never at boot; no banner line).
pub fn init() {
    with_cs(|| unsafe {
        let w = witness();
        write_volatile(addr_of_mut!((*w).i2c_buses), NI as u32);
        write_volatile(addr_of_mut!((*w).spi_buses), NS as u32);
    });
}

pub fn i2c_count() -> i32 {
    NI as i32
}

pub fn spi_count() -> i32 {
    NS as i32
}

/// The enumeration: the number of the index-th bus of a family (the chip's
/// own block number); the invalid code past the count.
pub fn i2c_bus(index: u32) -> i32 {
    match facts::I2C_BUSES.get(index as usize) {
        Some(d) => d.number as i32,
        None => refused(sched::ERR_INVAL, 0, 0),
    }
}

pub fn spi_bus(index: u32) -> i32 {
    match facts::SPI_BUSES.get(index as usize) {
        Some(d) => d.number as i32,
        None => refused(sched::ERR_INVAL, 0, 0),
    }
}

/// What a driver reports back beside its code: the message and byte the
/// refusal names, the fault's kind for the kernel's line, and what the
/// recovery after an I2C budget found.
pub struct Outcome {
    pub message: u32,
    pub byte: u32,
    pub kind: Fault,
    pub recovery: Recovery,
}

/// What a recovery of an I2C bus found: the data line free; a device holding
/// it low and clocked free; held through nine clocks and freed by the clock
/// held low; still held when the recovery ended.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Recovery {
    None,
    Clocked,
    Held,
    Stuck,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Fault {
    None,
    DataNack,
    BusError,
    ArbitrationLost,
    Budget,
}

/// A wire wait: the condition polled until it holds -- spinning for the
/// first `spin_ns` (about two byte times: a byte in flight arrives sooner
/// than a context switch costs), then yielding between reads so the core
/// stays available while a stuck wire is awaited; false when THIS WAIT's
/// budget (the caller's: on I2C two bytes' wire time plus the 25 ms a device
/// may hold the clock low, on SPI twice a byte's time plus two ticks) or the
/// caller's deadline passes first. A slow poller never runs out of budget by
/// its own pace: each wait's budget is its own.
pub fn wait_until(budget_ticks: u32, deadline_ns: u64, spin_ns: u64, mut cond: impl FnMut() -> bool) -> bool {
    let start_ms = sched::now_ms();
    let t0 = sched::now_ns();
    loop {
        if cond() {
            return true;
        }
        if sched::now_ms().wrapping_sub(start_ms) >= budget_ticks {
            return false;
        }
        if deadline_ns != sched::DEADLINE_NONE && sched::now_ns() >= deadline_ns {
            return false;
        }
        if sched::now_ns().wrapping_sub(t0) >= spin_ns {
            sched::yield_now();
        }
    }
}

/// The tenancy taken: a compare-and-swap of the word from 0 to 1; a loser
/// sleeps on the word through the futex until the holder's release wakes
/// it (or the word already moved), then tries again. Err(the timeout
/// code) when the deadline passes first; Ok(whether a wait happened).
pub(crate) fn tenancy_take(word: &AtomicU32, deadline_ns: u64) -> Result<bool, i32> {
    let mut waited = false;
    loop {
        if word.compare_exchange(0, 1, Ordering::AcqRel, Ordering::Acquire).is_ok() {
            return Ok(waited);
        }
        waited = true;
        match sched::futex_wait(word.as_ptr(), 1, deadline_ns) {
            0 | sched::ERR_AGAIN | sched::ERR_INTR => continue,
            code => return Err(code),
        }
    }
}

/// The tenancy released: the word stored 0 and one waiter woken (a wake
/// with no waiter is a bounded walk under the section).
pub(crate) fn tenancy_give(word: &AtomicU32) {
    word.store(0, Ordering::Release);
    sched::futex_wake(word.as_ptr(), 1);
}

/// The pads claimed for the board, all or nothing under one section: a
/// program holding any of them refuses the whole list before any register
/// moves (the busy code); a line the board already holds is fine.
fn claim_lines(lines: impl Iterator<Item = u32> + Clone) -> i32 {
    with_cs(|| unsafe {
        let c = &mut *addr_of_mut!(crate::gpio::CLAIMS);
        for l in lines.clone() {
            if crate::claims::is_program(c, l) {
                return crate::claims::REFUSED_BUSY;
            }
        }
        for l in lines {
            let r = crate::claims::claim_board(c, l);
            if r != 0 {
                return r;
            }
        }
        0
    })
}

// ---------------- the rows with a bus: the doors over the drivers ----------------

#[cfg(rhu_bus_rows)]
mod served {
    use super::*;
    use crate::board::{bus_backend as hw, i2c_driver, spi_driver};

    /// Whether a bus of this row runs the SPI register file that keeps the
    /// master bit and the internal select in two registers: the one file
    /// whose block can fall out of master mode at its configuration. The
    /// refusal's line is compiled for those rows alone.
    const SPI_FILE_SPLIT: bool = {
        let mut i = 0;
        let mut any = false;
        while i < facts::SPI_BUSES.len() {
            if matches!(facts::SPI_BUSES[i].file, m::SpiFile::U5) {
                any = true;
            }
            i += 1;
        }
        any
    };

    /// An I2C bus recovered through its pads as plain lines, the block off
    /// them: the data line read; low, up to nine clocks with a read after
    /// each (a device left in the middle of a byte lets go within them), then
    /// a stop. With `hold`, a line still low after the nine clocks gets the
    /// SMBus specification's remedy once: the clock held low for its 35 ms
    /// (a sleep: the bus's tenancy is held, the core is not), released, a
    /// stop, and the line read once more. The caller hands the pads back.
    pub fn i2c_recover(d: &m::I2cBus, hold: bool) -> Recovery {
        if !hw::i2c_sda_low(d) {
            return Recovery::None;
        }
        let mut free = false;
        for _ in 0..9 {
            hw::i2c_scl_pulse(d);
            if !hw::i2c_sda_low(d) {
                free = true;
                break;
            }
        }
        if free || !hold {
            hw::i2c_stop(d);
            return Recovery::Clocked;
        }
        hw::i2c_scl_hold(d, true);
        sched::sleep_ns(m::I2C_CLOCK_HOLD_MS as u64 * 1_000_000);
        hw::i2c_scl_hold(d, false);
        hw::i2c_stop(d);
        if hw::i2c_sda_low(d) {
            Recovery::Stuck
        } else {
            Recovery::Held
        }
    }

    pub fn i2c_claim(bus: u32, hz: u32) -> i32 {
        let Some(ix) = m::i2c_index(facts::I2C_BUSES, bus) else {
            return refused(sched::ERR_NODEV, 0, 0);
        };
        let d = &facts::I2C_BUSES[ix];
        let Some(timing) = i2c_driver::timing(d.kernel_hz, hz) else {
            return refused(ERR_NOTSUP, 0, 0);
        };
        let Some(mode) = m::i2c_mode_index(hz) else {
            return refused(ERR_NOTSUP, 0, 0);
        };
        let word = &I2C_TENANCY[ix];
        let _ = tenancy_take(word, sched::DEADLINE_NONE);
        // A claimed bus: at its own rate the claim answers 0 and changes
        // nothing; at another rate it is refused busy, nothing programmed.
        let held = I2C_CLAIMED[ix].load(Ordering::Acquire);
        if held != 0 {
            tenancy_give(word);
            return if held == 1 + mode { 0 } else { refused(crate::claims::REFUSED_BUSY, 0, 0) };
        }
        let r = claim_lines(d.pads.iter().map(|p| p.line as u32));
        if r != 0 {
            tenancy_give(word);
            return refused(r, 0, 0);
        }
        let recovered = i2c_driver::claim(d, timing);
        I2C_CLAIMED[ix].store(1 + mode, Ordering::Release);
        tenancy_give(word);
        with_cs(|| unsafe {
            let w = witness();
            bump(addr_of_mut!((*w).i2c_claims), 1);
            if recovered {
                bump(addr_of_mut!((*w).recoveries), 1);
            }
        });
        if recovered {
            kprintln!("[bus] i2c {}: the bus recovered at the claim (a device held the data line low; clocked free, then a stop)", bus);
        }
        0
    }

    pub fn i2c_transfer(bus: u32, msgs: *const m::I2cMsg, count: u32, deadline_ns: u64) -> i32 {
        let Some(ix) = m::i2c_index(facts::I2C_BUSES, bus) else {
            return refused(sched::ERR_NODEV, 0, 0);
        };
        if msgs.is_null() || (msgs as usize) & 3 != 0 || count == 0 || count as usize > m::MSG_MAX {
            return refused(sched::ERR_INVAL, 0, 0);
        }
        let msgs = unsafe { core::slice::from_raw_parts(msgs, count as usize) };
        let held = I2C_CLAIMED[ix].load(Ordering::Acquire);
        if m::i2c_msgs_check(msgs) != 0 || held == 0 {
            return refused(sched::ERR_INVAL, 0, 0);
        }
        let d = &facts::I2C_BUSES[ix];
        // The claimed rate, read back from the stored mode (the driver stores
        // nothing). The wait's budget is the standard rate's: 27 ticks with a
        // millisecond tick, and the same at 400 kHz and 1 MHz (the wire term
        // is one tick at every rate from 100 kHz up), so one figure serves
        // every mode.
        let hz = match m::I2C_RATES_HZ.get(held as usize - 1) {
            Some(&hz) => hz,
            None => m::I2C_RATES_HZ[0],
        };
        let budget = m::i2c_wait_ticks(100_000, crate::board::TICK_HZ);
        let word = &I2C_TENANCY[ix];
        let waited = match tenancy_take(word, deadline_ns) {
            Ok(w) => w,
            Err(code) => return refused(code, 0, 0),
        };
        let mut out = Outcome { message: 0, byte: 0, kind: Fault::None, recovery: Recovery::None };
        let r = i2c_driver::transfer(d, msgs, hz, budget, deadline_ns, &mut out);
        tenancy_give(word);
        with_cs(|| unsafe {
            let w = witness();
            if waited {
                bump(addr_of_mut!((*w).tenancy_waits), 1);
            }
            if r == 0 {
                bump(addr_of_mut!((*w).i2c_transfers), 1);
                bump(addr_of_mut!((*w).i2c_bytes), m::i2c_wire_bytes(msgs) - count);
            }
            if out.kind == Fault::Budget {
                bump(addr_of_mut!((*w).resets), 1);
            }
            if matches!(out.recovery, Recovery::Clocked | Recovery::Held) {
                bump(addr_of_mut!((*w).recoveries), 1);
            }
        });
        match out.kind {
            Fault::DataNack => kprintln!("[bus] i2c {}: a data byte not acknowledged at message {} byte {}", bus, out.message, out.byte),
            Fault::BusError => kprintln!("[bus] i2c {}: a bus error at message {}", bus, out.message),
            Fault::ArbitrationLost => kprintln!("[bus] i2c {}: arbitration lost at message {}", bus, out.message),
            Fault::Budget => kprintln!("[bus] i2c {}: the wire wait passed its budget at message {}; the block reset", bus, out.message),
            Fault::None => {}
        }
        match out.recovery {
            Recovery::Clocked => kprintln!("[bus] i2c {}: the bus recovered after the timeout (a device held the data line low; clocked free, then a stop)", bus),
            Recovery::Held => kprintln!("[bus] i2c {}: the bus recovered after the timeout (a device held the data line low through nine clocks; the clock held low 35 ms, then a stop)", bus),
            Recovery::Stuck => kprintln!("[bus] i2c {}: the data line is still held low after the timeout's recovery (nine clocks, the clock held low 35 ms, a stop); while it stays so, every transfer on this bus fails", bus),
            Recovery::None => {}
        }
        if r != 0 {
            return refused(r, out.message, out.byte);
        }
        0
    }

    pub fn spi_claim(bus: u32) -> i32 {
        let Some(ix) = m::spi_index(facts::SPI_BUSES, bus) else {
            return refused(sched::ERR_NODEV, 0, 0);
        };
        let d = &facts::SPI_BUSES[ix];
        let word = &SPI_TENANCY[ix];
        let _ = tenancy_take(word, sched::DEADLINE_NONE);
        if SPI_CLAIMED[ix].load(Ordering::Acquire) != 0 {
            tenancy_give(word);
            return 0;
        }
        let r = claim_lines(d.pads.iter().map(|p| p.line as u32));
        if r != 0 {
            tenancy_give(word);
            return refused(r, 0, 0);
        }
        spi_driver::claim(d);
        SPI_CLAIMED[ix].store(1, Ordering::Release);
        tenancy_give(word);
        with_cs(|| unsafe { bump(addr_of_mut!((*witness()).spi_claims), 1) });
        0
    }

    pub fn spi_transfer(bus: u32, select: u32, mode: u32, hz: u32, tx: *const u8, rx: *mut u8, len: u32) -> i32 {
        let Some(ix) = m::spi_index(facts::SPI_BUSES, bus) else {
            return refused(sched::ERR_NODEV, 0, 0);
        };
        if len == 0 || len > m::SPI_LEN_MAX || (tx.is_null() && rx.is_null()) {
            return refused(sched::ERR_INVAL, 0, 0);
        }
        let d = &facts::SPI_BUSES[ix];
        let Some((cpol, cpha)) = m::mode_bits(mode) else {
            return refused(sched::ERR_INVAL, 0, 0);
        };
        let Some((divisor, achieved)) = spi_driver::divisor(d.kernel_hz, hz) else {
            return refused(sched::ERR_INVAL, 0, 0);
        };
        let sel = match m::select_class(select, d.select_count) {
            m::SelectClass::Index(i) => Some(i),
            m::SelectClass::None => None,
            m::SelectClass::Invalid => return refused(sched::ERR_INVAL, 0, 0),
        };
        if SPI_CLAIMED[ix].load(Ordering::Acquire) == 0 {
            return refused(sched::ERR_INVAL, 0, 0);
        }
        let budget = m::budget_ticks(1, m::SPI_BITS_PER_BYTE, achieved, crate::board::TICK_HZ);
        // The bus is shared: the wait for it has no deadline of its own --
        // every holder's exchange is bounded wait by wait, so the wait is.
        let word = &SPI_TENANCY[ix];
        let waited = match tenancy_take(word, sched::DEADLINE_NONE) {
            Ok(w) => w,
            Err(code) => return refused(code, 0, 0),
        };
        // The chip select is the device's: claimed for the board at its first
        // use, UNDER THE BUS'S TENANCY, so two threads meeting at one select's
        // first use cannot both claim it and drive the pad twice.
        if let Some(i) = sel {
            let bit = 1u8 << i;
            if SPI_SELECTS[ix].load(Ordering::Acquire) & bit == 0 {
                let r = claim_lines(d.selects.get(i as usize).map(|&l| l as u32).into_iter());
                if r != 0 {
                    tenancy_give(word);
                    return refused(r, 0, 0);
                }
                hw::select_take(d, i);
                SPI_SELECTS[ix].fetch_or(bit, Ordering::AcqRel);
            }
        }
        let tx = if tx.is_null() { None } else { Some(unsafe { core::slice::from_raw_parts(tx, len as usize) }) };
        let rx = if rx.is_null() { None } else { Some(unsafe { core::slice::from_raw_parts_mut(rx, len as usize) }) };
        let mut out = Outcome { message: 0, byte: 0, kind: Fault::None, recovery: Recovery::None };
        let r = spi_driver::transfer(d, sel, cpol, cpha, divisor, tx, rx, len, budget, &mut out);
        tenancy_give(word);
        with_cs(|| unsafe {
            let w = witness();
            if waited {
                bump(addr_of_mut!((*w).tenancy_waits), 1);
            }
            if r == 0 {
                bump(addr_of_mut!((*w).spi_transfers), 1);
                bump(addr_of_mut!((*w).spi_bytes), len);
                write_volatile(addr_of_mut!((*w).spi_last_hz), achieved);
            }
            if out.kind == Fault::Budget {
                bump(addr_of_mut!((*w).resets), 1);
            }
        });
        if out.kind == Fault::Budget {
            kprintln!("[bus] spi {}: the wire wait passed its budget at byte {}; the block reset", bus, out.byte);
        }
        if SPI_FILE_SPLIT && out.kind == Fault::BusError {
            kprintln!("[bus] spi {}: the block fell out of master mode at its configuration; the fault cleared, nothing clocked", bus);
        }
        if r != 0 {
            return refused(r, 0, out.byte);
        }
        achieved as i32
    }
}

#[cfg(rhu_bus_rows)]
pub use served::{i2c_claim, i2c_recover, i2c_transfer, spi_claim, spi_transfer};

// ---------------- the rows without a bus: every door but the count refuses ----------------

#[cfg(not(rhu_bus_rows))]
pub fn i2c_claim(_bus: u32, _hz: u32) -> i32 {
    refused(sched::ERR_NODEV, 0, 0)
}

#[cfg(not(rhu_bus_rows))]
pub fn i2c_transfer(_bus: u32, _msgs: *const m::I2cMsg, _count: u32, _deadline_ns: u64) -> i32 {
    refused(sched::ERR_NODEV, 0, 0)
}

#[cfg(not(rhu_bus_rows))]
pub fn spi_claim(_bus: u32) -> i32 {
    refused(sched::ERR_NODEV, 0, 0)
}

#[cfg(not(rhu_bus_rows))]
pub fn spi_transfer(_bus: u32, _select: u32, _mode: u32, _hz: u32, _tx: *const u8, _rx: *mut u8, _len: u32) -> i32 {
    refused(sched::ERR_NODEV, 0, 0)
}
