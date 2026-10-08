//! The RP2350's I2C driver: the Synopsys DW_apb_i2c block (the RP2350
//! datasheet, section 12.2: the control register, the target address, the
//! data-and-command register, the standard- and fast-speed counts, the raw interrupt
//! status with its clear registers, the enable with its abort request, the
//! status, the two queue levels, the data hold, the abort's source, the
//! spike length), driven over the register seam so the same code runs
//! against the Cortex-M33 emulator row's model and the boards' blocks. The
//! block addresses ONE target a transaction, has no address-only frame and
//! reserves the addresses under 0x08 and over 0x77 (the note under its
//! address registers: its operation is not guaranteed at one), so this
//! family refuses, before the wire, a list naming two addresses, a message
//! of length 0 and a reserved address. A transaction is one command a byte
//! (12.2.7.1): a data byte or the read bit, a repeated start on the first
//! byte of every message after the first, a stop on the last byte of the
//! last message; an empty transmit queue does not end the transfer -- the
//! block holds the clock until the next command. The pump keeps the read
//! commands issued and not yet taken at or under the receive queue's
//! sixteen, and a byte is taken from the receive queue only while a read
//! command is owed one: a byte nobody asked for is never stored (the end's
//! disable erases it). Every wait is a poll of the queue levels and the raw
//! status against its own budget (two bytes' wire time plus the 25 ms the
//! SMBus specification lets a device hold the clock low) and the caller's
//! deadline, and covers one step of progress: a wait starts over with a
//! fresh budget each time the block takes a command. The block's abort is read at
//! every poll and before every command: the address not acknowledged, a data
//! byte not acknowledged (named from the count of commands the block
//! flushed), arbitration lost, anything else. A command written after an
//! abort is dropped by the block, and no register tells a dropped command
//! from an accepted one: one command's race remains between that read and
//! the write, and in it the byte a refusal names is one too high. A wait
//! past its budget asks the block to abort (12.2.10.4: it sends a stop and
//! flushes its queue), gives it two byte times, then cycles the block through
//! its reset and sets the claim's registers again -- a block whose clock a
//! device holds low never finishes its abort, and a reset releases the
//! lines. While the block is in that reset's wake and off its lines the bus
//! is recovered as at the claim: a device left holding the data line low is
//! clocked free, or failing that reset by the clock held low 35 ms. The
//! block is disabled around every transaction and the disable is
//! awaited in time (12.2.10.3.1: the enable's status is the one register
//! that tells when the block has shut down): an opening disable that does
//! not complete, and an abort whose stop or whose disable does not come,
//! take the same reset path and answer the timeout. The claim's registers
//! take a write only while the block is disabled (the control register's
//! own page), so they are written only after a disable that completed, and
//! every opening reads the control word back: a block that came to rest
//! after its registers were written has lost them, and gets them again
//! before it is enabled. At every opening the
//! bus-free time between a stop and the next start is spun: the block times
//! it only inside one enabled session. No interrupt of the block is used;
//! nothing of a transfer runs under the kernel critical section.

use crate::board::bus_backend as hw;
use crate::bus::{i2c_recover, wait_until, Fault, Outcome, Recovery};
use crate::bus_math::{I2cBus, I2cMsg, MSG_READ};
use crate::rp2350_bus_math::{self as rp, Abort};
use crate::sched;

const IC_CON: usize = 0x00;
const IC_TAR: usize = 0x04;
const IC_DATA_CMD: usize = 0x10;
const IC_SS_SCL_HCNT: usize = 0x14;
const IC_SS_SCL_LCNT: usize = 0x18;
const IC_FS_SCL_HCNT: usize = 0x1C;
const IC_FS_SCL_LCNT: usize = 0x20;
const IC_INTR_MASK: usize = 0x30;
const IC_RAW_INTR_STAT: usize = 0x34;
const IC_RX_TL: usize = 0x38;
const IC_TX_TL: usize = 0x3C;
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

/// The control word: a master at the mode's speed (the SPEED field: 1
/// standard, 2 fast and fast mode plus), the repeated start allowed, the
/// slave half off, the clock held while the receive queue is full.
const CON_MASTER: u32 = 1 << 0;
const CON_SPEED_STANDARD: u32 = 1 << 1;
const CON_SPEED_FAST: u32 = 2 << 1;
const CON_RESTART_EN: u32 = 1 << 5;
const CON_SLAVE_DISABLE: u32 = 1 << 6;
const CON_RX_FULL_HOLD: u32 = 1 << 9;
/// The two control words of record, one a mode's SPEED field. Every bit of
/// either reads back as written (the register's read-only bits are 10 and
/// above, and reset to 0); the block's reset word has bit 9 clear, so a
/// block at its reset values never reads as one.
const CON_WORD_SS: u32 = CON_MASTER | CON_SPEED_STANDARD | CON_RESTART_EN | CON_SLAVE_DISABLE | CON_RX_FULL_HOLD;
const CON_WORD_FS: u32 = CON_MASTER | CON_SPEED_FAST | CON_RESTART_EN | CON_SLAVE_DISABLE | CON_RX_FULL_HOLD;

/// The control word for a mode's SPEED field.
const fn con_word(speed: u32) -> u32 {
    if speed == 2 {
        CON_WORD_FS
    } else {
        CON_WORD_SS
    }
}

/// The control word a claimed rate's mode wants (the standard word for a
/// rate the counts refuse, which a claimed bus never carries).
fn con_word_of(d: &I2cBus, hz: u32) -> u32 {
    match rp::i2c_counts(d.kernel_hz, hz) {
        Some(c) => con_word(c.speed),
        None => CON_WORD_SS,
    }
}
const CMD_READ: u32 = 1 << 8;
const CMD_STOP: u32 = 1 << 9;
const CMD_RESTART: u32 = 1 << 10;
const RAW_TX_ABRT: u32 = 1 << 6;
const RAW_STOP_DET: u32 = 1 << 9;
const ENABLE_ENABLE: u32 = 1 << 0;
const ENABLE_ABORT: u32 = 1 << 1;
const STATUS_TFNF: u32 = 1 << 1;
const STATUS_MST_ACTIVITY: u32 = 1 << 5;
/// A wait spins for two byte times at 100 kHz before its first yield; an
/// abort asked after a budget is given the same two byte times.
const SPIN_NS: u64 = 2 * 9 * 10_000;
/// The bus-free time between a stop and the next start, spun at every
/// opening: the low count's time, 4.7 microseconds at standard speed (the
/// datasheet's table of timing parameters, with 12.2.14.3), and above the
/// bus-free time of the faster modes (1.3 and 0.5 microseconds), which the
/// block itself times from the mode's low count inside a session. The spin's floor
/// is this less one count of the kernel's clock: the clock interpolates the
/// tick timer, which counts the processor's clock.
const BUS_FREE_NS: u64 = 5_000;
const _: () = assert!(BUS_FREE_NS - (1_000_000_000 / crate::board::SYSCLK_HZ as u64 + 1) >= 4_700, "the bus-free spin's floor falls under the standard-mode bus-free time at this row's clock");

/// The timing word for a rate at the block's clock: the rate itself where
/// the counts exist (the claim computes them), None for a rate not served.
pub fn timing(kernel_hz: u32, hz: u32) -> Option<u32> {
    rp::i2c_counts(kernel_hz, hz).map(|_| hz)
}

/// The bus taken: the block cycled through its reset, the bus recovered
/// through the pads while they are still plain lines (a device left mid-read
/// holds the data line low: up to nine clocks free it, then a stop), the
/// pads handed to the block, the block's registers set. A reset or a first
/// disable that did not complete is told in one kernel line and counted; the
/// claim answers all the same, and a transfer answers by its budget. Returns
/// whether a recovery happened.
pub fn claim(d: &I2cBus, timing: u32) -> bool {
    let reset = hw::block_reset(d.enable_reg, d.enable_bit);
    let recovered = i2c_recover(d, false) != Recovery::None;
    for p in d.pads {
        hw::pad_take(p);
    }
    let rested = setup(d, timing);
    if !(reset && rested) {
        unrested(d.number);
    }
    recovered
}

/// The block did not come to rest at the claim: one kernel line, and one
/// more on the witness's resets (the bus module's field, written its way:
/// through a raw pointer, under the critical section).
fn unrested(bus: u32) {
    crate::critical::with_cs(|| unsafe {
        let resets = core::ptr::addr_of_mut!(crate::bus::WITNESS.resets);
        resets.write_volatile(resets.read_volatile().wrapping_add(1));
    });
    kprintln!("[bus] i2c {}: the block did not come to rest at the claim (its reset or its disable did not complete); while it stays so, a transfer on this bus times out instead of hanging", bus);
}

/// The block disabled: the enable written 0 and its status polled by the
/// kernel's clock until it reads 0, for two byte times at most (a block
/// stops at the end of its current byte), then read once more -- a thread
/// kept off the processor in the middle of the poll does not report a block
/// that has stopped. Returns whether the block stopped.
fn disable(d: &I2cBus) -> bool {
    hw::wr(d.base + IC_ENABLE, 0);
    let t0 = sched::now_ns();
    while sched::now_ns().wrapping_sub(t0) < SPIN_NS {
        if hw::rd(d.base + IC_ENABLE_STATUS) & ENABLE_ENABLE == 0 {
            return true;
        }
    }
    hw::rd(d.base + IC_ENABLE_STATUS) & ENABLE_ENABLE == 0
}

/// The claim's registers: the block disabled, then -- only when that disable
/// completed, for a block that has not shut down takes no write -- the
/// registers written. The block is left disabled; a transfer enables it.
/// Returns whether the disable completed.
fn setup(d: &I2cBus, hz: u32) -> bool {
    let rested = disable(d);
    if rested {
        registers(d, hz);
    }
    rested
}

/// The registers themselves, written to a block that is disabled: the
/// mode's control word, the two counts in the mode's register pair (SS for
/// standard speed, FS for fast and fast mode plus), the spike length, the
/// data hold, both thresholds 0, every interrupt masked.
fn registers(d: &I2cBus, hz: u32) {
    let Some(c) = rp::i2c_counts(d.kernel_hz, hz) else {
        return;
    };
    let base = d.base;
    let (hcnt_reg, lcnt_reg) = if c.speed == 2 { (IC_FS_SCL_HCNT, IC_FS_SCL_LCNT) } else { (IC_SS_SCL_HCNT, IC_SS_SCL_LCNT) };
    hw::wr(base + IC_CON, con_word(c.speed));
    hw::wr(base + hcnt_reg, c.hcnt);
    hw::wr(base + lcnt_reg, c.lcnt);
    hw::wr(base + IC_FS_SPKLEN, c.spklen);
    hw::wr(base + IC_SDA_HOLD, c.hold);
    hw::wr(base + IC_RX_TL, 0);
    hw::wr(base + IC_TX_TL, 0);
    hw::wr(base + IC_INTR_MASK, 0);
}

/// The command index of the next read byte at or after `from`.
fn next_read(msgs: &[I2cMsg], from: u32) -> u32 {
    let mut start = 0u32;
    for msg in msgs {
        let end = start + msg.len as u32;
        if msg.flags & MSG_READ != 0 && from < end {
            return if from > start { from } else { start };
        }
        start = end;
    }
    start
}

/// The reset path's end: the block cycled through its reset, the bus
/// recovered through the pads as plain lines while the block is off them and
/// the pads handed back, the claim's registers set again at the claimed
/// rate, the message and byte named, the timeout answered. Neither the
/// reset's answer nor the setup's is judged here: the next opening's disable
/// and its read of the control word are the guard.
fn reset_out(d: &I2cBus, hz: u32, message: u32, byte: u32, out: &mut Outcome) -> i32 {
    hw::block_reset(d.enable_reg, d.enable_bit);
    out.recovery = i2c_recover(d, true);
    for p in d.pads {
        hw::pad_take(p);
    }
    setup(d, hz);
    out.message = message;
    out.byte = byte;
    out.kind = Fault::Budget;
    sched::ERR_TIMEDOUT
}

/// An abort read off the block: its source classed, the message and byte
/// named from the count of commands the block flushed, the block's own stop
/// awaited, the abort and the stop cleared, the block disabled. A stop that
/// does not come, or a disable that does not complete, takes the reset path.
fn aborted(d: &I2cBus, hz: u32, msgs: &[I2cMsg], issued: u32, budget_ticks: u32, deadline_ns: u64, out: &mut Outcome) -> i32 {
    let base = d.base;
    let source = hw::rd(base + IC_TX_ABRT_SOURCE);
    let (message, byte) = rp::locate(msgs, rp::last_taken(issued, source >> rp::TX_FLUSH_SHIFT));
    let idle = wait_until(budget_ticks, deadline_ns, SPIN_NS, || hw::rd(base + IC_STATUS) & STATUS_MST_ACTIVITY == 0);
    let _ = hw::rd(base + IC_CLR_TX_ABRT);
    let _ = hw::rd(base + IC_CLR_STOP_DET);
    if !idle || !disable(d) {
        return reset_out(d, hz, message, byte, out);
    }
    out.message = message;
    out.byte = byte;
    match rp::abort_class(source) {
        Abort::Address => {
            out.byte = 0;
            crate::bus::ERR_NXIO
        }
        Abort::Data => {
            out.kind = Fault::DataNack;
            sched::ERR_IO
        }
        Abort::Arbitration => {
            out.kind = Fault::ArbitrationLost;
            sched::ERR_IO
        }
        Abort::Other => {
            out.kind = Fault::BusError;
            sched::ERR_IO
        }
    }
}

/// A wait passed its budget (or the caller's deadline): the command the
/// block took last is named, the block asked to abort and given two byte
/// times to send its stop, then cycled through its reset and set up again.
fn gave_up(d: &I2cBus, hz: u32, msgs: &[I2cMsg], issued: u32, out: &mut Outcome) -> i32 {
    let base = d.base;
    let (message, byte) = rp::locate(msgs, rp::last_taken(issued, hw::rd(base + IC_TXFLR)));
    hw::wr(base + IC_ENABLE, ENABLE_ENABLE | ENABLE_ABORT);
    let t0 = sched::now_ns();
    while hw::rd(base + IC_RAW_INTR_STAT) & RAW_TX_ABRT == 0 && sched::now_ns().wrapping_sub(t0) < SPIN_NS {}
    reset_out(d, hz, message, byte, out)
}

/// One combined transaction. `hz` is the claimed rate (the bus doors store
/// it as the mode; the driver stores nothing); `budget_ticks` one step's
/// wire-wait budget; `deadline_ns` the caller's bound on the whole
/// transaction.
pub fn transfer(d: &I2cBus, msgs: &[I2cMsg], hz: u32, budget_ticks: u32, deadline_ns: u64, out: &mut Outcome) -> i32 {
    let base = d.base;
    if let Some(i) = rp::i2c_refused_message(msgs) {
        out.message = i;
        return sched::ERR_INVAL;
    }
    let Some(first) = msgs.first() else {
        return sched::ERR_INVAL;
    };
    let mut total = 0u32;
    let mut total_reads = 0u32;
    for msg in msgs {
        total += msg.len as u32;
        if msg.flags & MSG_READ != 0 {
            total_reads += msg.len as u32;
        }
    }
    if !disable(d) {
        return reset_out(d, hz, 0, 0, out);
    }
    let word = con_word_of(d, hz);
    if hw::rd(base + IC_CON) != word {
        registers(d, hz);
        if hw::rd(base + IC_CON) != word {
            return reset_out(d, hz, 0, 0, out);
        }
    }
    let t0 = sched::now_ns();
    while sched::now_ns().wrapping_sub(t0) < BUS_FREE_NS {}
    hw::wr(base + IC_TAR, first.addr as u32);
    let _ = hw::rd(base + IC_CLR_TX_ABRT);
    let _ = hw::rd(base + IC_CLR_STOP_DET);
    hw::wr(base + IC_ENABLE, ENABLE_ENABLE);
    let (mut issued, mut reads, mut taken, mut next) = (0u32, 0u32, 0u32, 0u32);
    loop {
        if hw::rd(base + IC_RAW_INTR_STAT) & RAW_TX_ABRT != 0 {
            return aborted(d, hz, msgs, issued, budget_ticks, deadline_ns, out);
        }
        let mut moved = false;
        while taken < reads && hw::rd(base + IC_RXFLR) != 0 {
            let b = hw::rd(base + IC_DATA_CMD) as u8;
            next = next_read(msgs, next);
            let (i, at) = rp::locate(msgs, next);
            if let Some(msg) = msgs.get(i as usize) {
                unsafe { msg.buf.add(at as usize).write(b) };
            }
            next += 1;
            taken += 1;
            moved = true;
        }
        while issued < total && reads - taken < rp::I2C_QUEUE && hw::rd(base + IC_RAW_INTR_STAT) & RAW_TX_ABRT == 0 && hw::rd(base + IC_STATUS) & STATUS_TFNF != 0 {
            let (i, at) = rp::locate(msgs, issued);
            let Some(msg) = msgs.get(i as usize) else {
                break;
            };
            let read = msg.flags & MSG_READ != 0;
            let mut cmd = if read { CMD_READ } else { (unsafe { msg.buf.add(at as usize).read() }) as u32 };
            if at == 0 && i > 0 {
                cmd |= CMD_RESTART;
            }
            if issued + 1 == total {
                cmd |= CMD_STOP;
            }
            hw::wr(base + IC_DATA_CMD, cmd);
            if read {
                reads += 1;
            }
            issued += 1;
            moved = true;
        }
        if issued == total && taken == total_reads {
            break;
        }
        if moved {
            continue;
        }
        let more = issued < total;
        let level = hw::rd(base + IC_TXFLR);
        let ok = wait_until(budget_ticks, deadline_ns, SPIN_NS, || {
            (taken < reads && hw::rd(base + IC_RXFLR) != 0)
                || hw::rd(base + IC_RAW_INTR_STAT) & RAW_TX_ABRT != 0
                || (more && reads - taken < rp::I2C_QUEUE && hw::rd(base + IC_STATUS) & STATUS_TFNF != 0)
                || hw::rd(base + IC_TXFLR) < level
        });
        if !ok {
            return gave_up(d, hz, msgs, issued, out);
        }
    }
    loop {
        let level = hw::rd(base + IC_TXFLR);
        if !wait_until(budget_ticks, deadline_ns, SPIN_NS, || hw::rd(base + IC_RAW_INTR_STAT) & (RAW_STOP_DET | RAW_TX_ABRT) != 0 || hw::rd(base + IC_TXFLR) < level) {
            return gave_up(d, hz, msgs, issued, out);
        }
        if hw::rd(base + IC_RAW_INTR_STAT) & (RAW_STOP_DET | RAW_TX_ABRT) != 0 {
            break;
        }
    }
    if hw::rd(base + IC_RAW_INTR_STAT) & RAW_TX_ABRT != 0 {
        return aborted(d, hz, msgs, issued, budget_ticks, deadline_ns, out);
    }
    let _ = hw::rd(base + IC_CLR_STOP_DET);
    disable(d);
    0
}
