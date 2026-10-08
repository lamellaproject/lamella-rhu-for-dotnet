//! The STM32F7 family's SPI driver: the sixteen-bit register file with the
//! four-byte queues (the reference manual's SPI chapter: control register 1
//! with the polarity, the phase, the master bit, the baud-rate field, the
//! enable and the software slave management; control register 2 with the
//! data size and the receive threshold; the status register; the data
//! register accessed as a byte so one eight-bit frame moves per access),
//! driven over the register seam so the same code runs against the
//! emulator row's model and the boards' blocks. The block's clock is the
//! APB bus clock, so the divisor is chosen per transfer from the posture's
//! clock and the rate achieved is what the door answers. The pump keeps at
//! most two bytes ahead of the bytes received (half the receive queue) so
//! the queue never overflows. Every wait is a poll of the status register
//! against its own budget. At the end of an exchange the transmitter's
//! drain is awaited (the busy flag) before the select is released, then the
//! block is disabled; a wait past its budget takes the same path without
//! the drain: the select released, the block disabled. Nothing of a
//! transfer runs under the kernel critical section.

use crate::board::bus_backend as hw;
use crate::bus::{wait_until, Fault, Outcome};
use crate::bus_math::SpiBus;
use crate::sched;

const CR1: usize = 0x00;
const CR2: usize = 0x04;
const SR: usize = 0x08;
const DR: usize = 0x0C;

const CR1_CPHA: u32 = 1 << 0;
const CR1_CPOL: u32 = 1 << 1;
const CR1_MSTR: u32 = 1 << 2;
const CR1_BR_SHIFT: u32 = 3;
const CR1_SPE: u32 = 1 << 6;
const CR1_SSI: u32 = 1 << 8;
const CR1_SSM: u32 = 1 << 9;
const CR2_DS_8: u32 = 0b0111 << 8;
const CR2_FRXTH: u32 = 1 << 12;
const SR_RXNE: u32 = 1 << 0;
const SR_TXE: u32 = 1 << 1;
const SR_BSY: u32 = 1 << 7;

/// The bus taken: the block's clock enabled and the pads handed to it; the
/// block itself is configured per transfer.
pub fn claim(d: &SpiBus) {
    hw::rmw(d.enable_reg, 0, 1 << d.enable_bit);
    for p in d.pads {
        hw::pad_take(p);
    }
    hw::wr(d.base + CR1, 0);
}

/// One full-duplex exchange under one chip select at the device's mode and
/// divisor: the block configured and enabled, the select asserted, the
/// bytes pumped, the transmitter drained, the select released, the block
/// disabled.
#[allow(clippy::too_many_arguments)]
pub fn transfer(d: &SpiBus, sel: Option<u32>, cpol: bool, cpha: bool, divisor: u32, tx: Option<&[u8]>, rx: Option<&mut [u8]>, len: u32, budget_ticks: u32, out: &mut Outcome) -> i32 {
    let base = d.base;
    let spin_ns = 2 * 8_000_000_000 / (d.kernel_hz >> (divisor + 1)) as u64;
    let mut cr1 = CR1_MSTR | CR1_SSM | CR1_SSI | (divisor << CR1_BR_SHIFT);
    if cpol {
        cr1 |= CR1_CPOL;
    }
    if cpha {
        cr1 |= CR1_CPHA;
    }
    hw::wr(base + CR1, cr1);
    hw::wr(base + CR2, CR2_DS_8 | CR2_FRXTH);
    hw::wr(base + CR1, cr1 | CR1_SPE);
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
        let sr = hw::rd(base + SR);
        if sent < len && sent - received < ahead && sr & SR_TXE != 0 {
            let b = tx.map_or(0xFF, |t| t[sent as usize]);
            hw::wr8(base + DR, b);
            sent += 1;
        } else if sr & SR_RXNE != 0 && !(doctored && sent < len) {
            let b = hw::rd8(base + DR);
            if let Some(r) = rx.as_deref_mut() {
                r[received as usize] = b;
            }
            received += 1;
        } else if !wait_until(budget_ticks, sched::DEADLINE_NONE, spin_ns, || {
            let sr = hw::rd(base + SR);
            (sr & SR_RXNE != 0 && !(doctored && sent < len)) || (sent < len && sent - received < ahead && sr & SR_TXE != 0)
        }) {
            break false;
        }
    };
    let drained = ok && wait_until(budget_ticks, sched::DEADLINE_NONE, spin_ns, || hw::rd(base + SR) & SR_BSY == 0);
    if let Some(i) = sel {
        hw::select_drive(d, i, false);
    }
    hw::wr(base + CR1, cr1);
    if !drained {
        out.byte = received;
        out.kind = Fault::Budget;
        return sched::ERR_TIMEDOUT;
    }
    0
}

/// The divisor for a rate at the block's kernel clock, with the rate it
/// achieves: this family's ladder, asked by the bus doors through the driver.
pub use crate::bus_math::spi_divisor as divisor;
