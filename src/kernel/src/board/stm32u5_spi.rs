//! The STM32U5 family's SPI driver: the register file with the sixteen-byte
//! queues and the transfer-size counter (the reference manual's SPI
//! chapter: control register 1 with the enable, the start and the internal
//! slave select; control register 2 with the transfer size; configuration
//! register 1 with the data size and the baud-rate field; configuration
//! register 2 with the master bit, the polarity, the phase, the software
//! slave management and the alternate-function control; the status and
//! flag-clear registers; the two data registers accessed as bytes), driven
//! over the register seam so the same code runs against the emulator row's
//! model and the boards' blocks. The block's kernel clock is selected to
//! the fixed 16 MHz source at the claim, so the divisor ladder is the same
//! at every posture. The internal slave select is raised at the claim,
//! BEFORE the master bit is ever written, and stays high from then on: a
//! master with the software slave management whose internal select reads
//! low raises the mode fault at once, EVEN WHILE THE BLOCK IS DISABLED,
//! loses its master bit and refuses its enable until the fault is cleared.
//! The reference manual's own configuration steps write the configuration
//! registers first and the internal select afterwards, and the block
//! faults on exactly that order: the order here is deliberate and is not
//! to be put back. A block found out of master mode after its
//! configuration is refused at once as a bus error, the fault cleared by
//! its flag-clear bit, never waited on. The receiver is clocked by the
//! clock line as its pad reads it back, so a transfer completes only with
//! the clock pad handed to the block, which the claim does. The CRC size
//! field is kept at eight bits, a value the manual defines, with the CRC
//! itself off. The pump keeps at most eight bytes
//! ahead of the bytes received (half the receive queue) so the queue never
//! overflows; the end-of-transfer flag closes the exchange. Every wait is a
//! poll of the status register against its own budget; a budget
//! passed disables the block and releases the select. Nothing here runs
//! under the kernel critical section.

use crate::board::bus_backend as hw;
use crate::bus::{wait_until, Fault, Outcome};
use crate::bus_math::SpiBus;
use crate::sched;

const CR1: usize = 0x00;
const CR2: usize = 0x04;
const CFG1: usize = 0x08;
const CFG2: usize = 0x0C;
const SR: usize = 0x14;
const IFCR: usize = 0x18;
const TXDR: usize = 0x20;
const RXDR: usize = 0x30;

const CR1_SPE: u32 = 1 << 0;
const CR1_CSTART: u32 = 1 << 9;
const CR1_SSI: u32 = 1 << 12;
const CFG1_DSIZE_8: u32 = 7;
const CFG1_CRCSIZE_8: u32 = 7 << 16;
const CFG1_MBR_SHIFT: u32 = 28;
const CFG2_MASTER: u32 = 1 << 22;
const CFG2_CPHA: u32 = 1 << 24;
const CFG2_CPOL: u32 = 1 << 25;
const CFG2_SSM: u32 = 1 << 26;
const CFG2_AFCNTR: u32 = 1 << 31;
const SR_RXP: u32 = 1 << 0;
const SR_TXP: u32 = 1 << 1;
const SR_EOT: u32 = 1 << 3;
const SR_TXTF: u32 = 1 << 4;
const SR_MODF: u32 = 1 << 9;
const IFCR_END: u32 = SR_EOT | SR_TXTF;
const IFCR_MODFC: u32 = 1 << 9;

/// The negative control of the write order: the doctored image's first
/// transfer after a claim drops the internal select at its head, as the
/// order once stood, so the block falls out of master mode and the refusal
/// in the transfer is what answers.
#[cfg(feature = "test-bus-doctored")]
static OLD_ORDER: core::sync::atomic::AtomicBool = core::sync::atomic::AtomicBool::new(false);

/// The bus taken: the block's clock enabled and its kernel clock selected,
/// the pads handed to it, the internal select raised; the block itself is
/// configured per transfer.
pub fn claim(d: &SpiBus) {
    hw::rmw(d.enable_reg, 0, 1 << d.enable_bit);
    hw::rmw(d.mux_reg, 0b11 << d.mux_shift, d.mux_value << d.mux_shift);
    for p in d.pads {
        hw::pad_take(p);
    }
    hw::wr(d.base + CR1, CR1_SSI);
    #[cfg(feature = "test-bus-doctored")]
    OLD_ORDER.store(true, core::sync::atomic::Ordering::Release);
}

/// One full-duplex exchange under one chip select at the device's mode and
/// divisor: the block configured with the transfer size while the internal
/// select stays high, the block enabled -- refused at once as a bus error
/// when it fell out of master mode -- and started, the select asserted, the
/// bytes pumped, the end of the transfer awaited and cleared, the select
/// released, the block disabled.
#[allow(clippy::too_many_arguments)]
pub fn transfer(d: &SpiBus, sel: Option<u32>, cpol: bool, cpha: bool, divisor: u32, tx: Option<&[u8]>, rx: Option<&mut [u8]>, len: u32, budget_ticks: u32, out: &mut Outcome) -> i32 {
    let base = d.base;
    let spin_ns = 2 * 8_000_000_000 / (d.kernel_hz >> (divisor + 1)) as u64;
    hw::wr(base + CR1, CR1_SSI);
    #[cfg(feature = "test-bus-doctored")]
    if OLD_ORDER.swap(false, core::sync::atomic::Ordering::AcqRel) {
        hw::wr(base + CR1, 0);
    }
    hw::wr(base + CFG1, CFG1_DSIZE_8 | CFG1_CRCSIZE_8 | (divisor << CFG1_MBR_SHIFT));
    let mut cfg2 = CFG2_MASTER | CFG2_SSM | CFG2_AFCNTR;
    if cpol {
        cfg2 |= CFG2_CPOL;
    }
    if cpha {
        cfg2 |= CFG2_CPHA;
    }
    hw::wr(base + CFG2, cfg2);
    hw::wr(base + CR2, len);
    hw::wr(base + CR1, CR1_SSI | CR1_SPE);
    if hw::rd(base + SR) & SR_MODF != 0 || hw::rd(base + CR1) & CR1_SPE == 0 {
        hw::wr(base + IFCR, IFCR_MODFC);
        hw::wr(base + CR1, CR1_SSI);
        out.byte = 0;
        out.kind = Fault::BusError;
        return sched::ERR_IO;
    }
    if let Some(i) = sel {
        hw::select_drive(d, i, true);
    }
    hw::wr(base + CR1, CR1_SSI | CR1_SPE | CR1_CSTART);
    let doctored = cfg!(feature = "test-bus-doctored");
    let ahead = if doctored { len } else { d.ahead };
    let mut rx = rx;
    let (mut sent, mut received) = (0u32, 0u32);
    let ok = loop {
        if received >= len {
            break true;
        }
        let sr = hw::rd(base + SR);
        if sent < len && sent - received < ahead && sr & SR_TXP != 0 {
            let b = tx.map_or(0xFF, |t| t[sent as usize]);
            hw::wr8(base + TXDR, b);
            sent += 1;
        } else if sr & SR_RXP != 0 && !(doctored && sent < len) {
            let b = hw::rd8(base + RXDR);
            if let Some(r) = rx.as_deref_mut() {
                r[received as usize] = b;
            }
            received += 1;
        } else if !wait_until(budget_ticks, sched::DEADLINE_NONE, spin_ns, || {
            let sr = hw::rd(base + SR);
            (sr & SR_RXP != 0 && !(doctored && sent < len)) || (sent < len && sent - received < ahead && sr & SR_TXP != 0)
        }) {
            break false;
        }
    };
    let ended = ok && wait_until(budget_ticks, sched::DEADLINE_NONE, spin_ns, || hw::rd(base + SR) & SR_EOT != 0);
    if let Some(i) = sel {
        hw::select_drive(d, i, false);
    }
    hw::wr(base + IFCR, IFCR_END);
    hw::wr(base + CR1, CR1_SSI);
    if !ended {
        out.byte = received;
        out.kind = Fault::Budget;
        return sched::ERR_TIMEDOUT;
    }
    0
}

/// The divisor for a rate at the block's kernel clock, with the rate it
/// achieves: this family's ladder, asked by the bus doors through the driver.
pub use crate::bus_math::spi_divisor as divisor;
