//! The RP2350's SPI driver: the Arm PL022 block (the RP2350 datasheet,
//! section 12.3: control register 0 with the clock-rate step, the phase, the
//! polarity, the frame format and the data size; control register 1 with
//! the enable; the data register; the status register; the prescale
//! register), driven over the register seam so the same code runs against
//! the Cortex-M33 emulator row's model and the boards' blocks. The block's
//! clock is the peripheral clock -- the crystal on both clock postures -- so
//! the prescaler pair is chosen per transfer and the rate achieved is what
//! the door answers. The block's own frame signal pulses between words when
//! the phase is 0 (12.3.4.10), so a chip select is a plain output line the
//! backend drives, never the block's. The datasheet states the clock line's
//! level for an ENABLED block alone (12.3.4.9.1: low at polarity 0, high at
//! polarity 1 when no data moves) and none for a disabled one, so after the
//! enable the clock pad is READ until it rests at the mode's idle level, and
//! only then does a select move: a select that fell first would show a mode
//! 2 or 3 device an edge nobody clocked. That orders the bus's own selects;
//! a select the program drives is already low when the transfer starts, and
//! the kernel cannot order a line it does not drive. The pump keeps at most
//! four bytes ahead of the bytes received (half the eight-entry receive
//! queue) so the queue never overflows; the receiver's overrun is read after
//! the burst all the same, and an exchange that lost a byte never answers
//! its rate. Every wait is a poll of the status register against its own
//! budget. At the end of an exchange the transmitter's drain is awaited (the
//! busy flag) before the select is released, then the block is disabled; a
//! wait past its budget releases the select, disables the block and cycles
//! it through its reset. No interrupt of the block is used (its mask
//! register stays at its reset value, every interrupt masked); nothing of a
//! transfer runs under the kernel critical section.

use crate::board::bus_backend as hw;
use crate::bus::{wait_until, Fault, Outcome};
use crate::bus_math::SpiBus;
use crate::rp2350_bus_math as rp;
use crate::sched;

const SSPCR0: usize = 0x000;
const SSPCR1: usize = 0x004;
const SSPDR: usize = 0x008;
const SSPSR: usize = 0x00C;
const SSPCPSR: usize = 0x010;
const SSPRIS: usize = 0x018;

const CR0_DSS_8: u32 = 0b0111;
const CR0_SPO: u32 = 1 << 6;
const CR0_SPH: u32 = 1 << 7;
const CR0_SCR_SHIFT: u32 = 8;
const CR1_SSE: u32 = 1 << 1;
const SR_TNF: u32 = 1 << 1;
const SR_RNE: u32 = 1 << 2;
const SR_BSY: u32 = 1 << 4;
/// The receiver's overrun, raw: a frame arrived at a full receive queue.
const RIS_ROR: u32 = 1 << 0;
/// A wait's spin before its first yield is never longer than this.
const SPIN_MAX_NS: u64 = 1_000_000;
/// After the enable the clock pad is given this long to rest at the mode's
/// idle level.
const CLOCK_IDLE_NS: u64 = 100_000;

/// The prescaler pair for a rate at the block's clock, packed, with the
/// rate it achieves: this family's ladder.
pub fn divisor(kernel_hz: u32, hz: u32) -> Option<(u32, u32)> {
    rp::spi_pair(kernel_hz, hz)
}

/// The bus taken: the block cycled through its reset and the pads handed to
/// it, every listed select nobody holds given its resting level; the block
/// itself is configured per transfer and left disabled. A reset that did not
/// complete is told in one kernel line and counted; the claim answers all
/// the same, and a transfer answers by its budget.
pub fn claim(d: &SpiBus) {
    let reset = hw::block_reset(d.enable_reg, d.enable_bit);
    for p in d.pads {
        hw::pad_take(p);
    }
    hw::select_rest(d);
    hw::wr(d.base + SSPCR1, 0);
    if !reset {
        unrested(d.number);
    }
}

/// The block's reset did not complete at the claim: one kernel line, and one
/// more on the witness's resets (the bus module's field, written its way:
/// through a raw pointer, under the critical section).
fn unrested(bus: u32) {
    crate::critical::with_cs(|| unsafe {
        let resets = core::ptr::addr_of_mut!(crate::bus::WITNESS.resets);
        resets.write_volatile(resets.read_volatile().wrapping_add(1));
    });
    kprintln!("[bus] spi {}: the block's reset did not complete at the claim; while it stays so, a transfer on this bus times out instead of hanging", bus);
}

/// The clock pad read until it rests at the mode's idle level (high at
/// polarity 1, low at 0): polled by the kernel's clock, then read once more
/// -- a thread kept off the processor in the middle of the poll does not
/// report a clock that has come to rest.
fn clock_rests(d: &SpiBus, high: bool) -> bool {
    let t0 = sched::now_ns();
    while sched::now_ns().wrapping_sub(t0) < CLOCK_IDLE_NS {
        if hw::spi_clock_at(d, high) {
            return true;
        }
    }
    hw::spi_clock_at(d, high)
}

/// One full-duplex exchange under one chip select at the device's mode and
/// prescaler pair: the block configured and enabled, the clock awaited at
/// its idle level, the select asserted, the bytes pumped, the transmitter
/// drained, the select released, the block disabled.
#[allow(clippy::too_many_arguments)]
pub fn transfer(d: &SpiBus, sel: Option<u32>, cpol: bool, cpha: bool, divisor: u32, tx: Option<&[u8]>, rx: Option<&mut [u8]>, len: u32, budget_ticks: u32, out: &mut Outcome) -> i32 {
    let base = d.base;
    let rate = rp::spi_pair_rate(d.kernel_hz, divisor).max(1);
    let two_bytes = 2 * 8_000_000_000 / rate as u64;
    let spin_ns = if two_bytes < SPIN_MAX_NS { two_bytes } else { SPIN_MAX_NS };
    let mut cr0 = (((divisor >> 8) & 0xFF) << CR0_SCR_SHIFT) | CR0_DSS_8;
    if cpol {
        cr0 |= CR0_SPO;
    }
    if cpha {
        cr0 |= CR0_SPH;
    }
    hw::wr(base + SSPCR1, 0);
    hw::wr(base + SSPCR0, cr0);
    hw::wr(base + SSPCPSR, divisor & 0xFF);
    hw::wr(base + SSPCR1, CR1_SSE);
    if !clock_rests(d, cpol) {
        hw::wr(base + SSPCR1, 0);
        hw::block_reset(d.enable_reg, d.enable_bit);
        out.byte = 0;
        out.kind = Fault::Budget;
        return sched::ERR_TIMEDOUT;
    }
    if let Some(i) = sel {
        hw::select_drive(d, i, true);
    }
    let doctored = cfg!(feature = "test-bus-doctored");
    let ahead = if doctored { len } else { d.ahead };
    let mut rx = rx;
    let (mut sent, mut received) = (0u32, 0u32);
    let ok = loop {
        if received >= len {
            break true;
        }
        let sr = hw::rd(base + SSPSR);
        if sent < len && sent - received < ahead && sr & SR_TNF != 0 {
            let b = tx.and_then(|t| t.get(sent as usize).copied()).unwrap_or(0xFF);
            hw::wr(base + SSPDR, b as u32);
            sent += 1;
        } else if sr & SR_RNE != 0 && !(doctored && sent < len) {
            let b = hw::rd(base + SSPDR) as u8;
            if let Some(slot) = rx.as_deref_mut().and_then(|r| r.get_mut(received as usize)) {
                *slot = b;
            }
            received += 1;
        } else if !wait_until(budget_ticks, sched::DEADLINE_NONE, spin_ns, || {
            let sr = hw::rd(base + SSPSR);
            (sr & SR_RNE != 0 && !(doctored && sent < len)) || (sent < len && sent - received < ahead && sr & SR_TNF != 0)
        }) {
            break false;
        }
    };
    let overran = hw::rd(base + SSPRIS) & RIS_ROR != 0;
    let drained = ok && !overran && wait_until(budget_ticks, sched::DEADLINE_NONE, spin_ns, || hw::rd(base + SSPSR) & SR_BSY == 0);
    if let Some(i) = sel {
        hw::select_drive(d, i, false);
    }
    hw::wr(base + SSPCR1, 0);
    if !drained {
        hw::block_reset(d.enable_reg, d.enable_bit);
        out.byte = received.min(len.saturating_sub(1));
        out.kind = Fault::Budget;
        return sched::ERR_TIMEDOUT;
    }
    0
}
