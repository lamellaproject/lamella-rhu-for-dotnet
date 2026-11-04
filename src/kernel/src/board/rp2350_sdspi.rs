//! The RP2350 boards' microSD socket as the card layer's host (the two
//! Adafruit Metro RP2350 boards' on the chip's first SPI block, the SparkFun
//! IoT RedBoard RP2350's on its second): sdspi.rs's SPI-mode host over the
//! socket's SPI block -- the Arm PL022 of the RP2350 datasheet's section
//! 12.3 -- on the socket's clock, data-out and data-in lines (the board's
//! facts: SD_BUS), with the card's select driven as a plain output line
//! (SD_SELECT_LINE) and the card-detect switch read as a plain input under
//! the pull the facts name (SD_DETECT_LINE and SD_DETECT_PULL; the level a
//! seated card gives the line SD_DETECT_CARD_LEVEL). Every line of the
//! socket is the board's by the claims table (the facts' board claims), so a
//! program's open of any of them is refused. The block is taken as the bus
//! layer takes a bus (its reset
//! cycled and the pads handed to it: rp2350_spi::claim) and the rates come off
//! the same ladder (rp2350_bus_math::spi_pair at the block's clock, the 12 MHz
//! crystal on both clock postures): the identification at 375 kHz, the data
//! phase at 6 MHz, the block's top. The bytes are pumped here under this
//! module's own bounds -- the block's queue flags polled a bounded number of
//! times per byte, a count scaled to the system clock and never a clock
//! reading -- because the boot banner identifies the card before the kernel's
//! tick and before the scheduler run; once the scheduler runs, a long wait
//! yields between its polls and a pause sleeps. No interrupt of the block is
//! used; nothing of a transfer runs under the kernel's critical section.

use core::sync::atomic::{AtomicU32, Ordering};

use super::rp2350_spi::{self, CR0_DSS_8, CR0_SCR_SHIFT, CR1_SSE, RIS_ROR, SR_RNE, SR_TNF, SSPCPSR, SSPCR0, SSPCR1, SSPDR, SSPRIS, SSPSR};
use crate::board::bus_backend as hw;
use crate::board::facts;
use crate::board::gpio_backend as pins;
use crate::gpio_math as g;
use crate::rp2350_bus_math as rp;
use crate::sched;
use crate::sdhost::{Micros, Rate, Refusal};
use crate::sdspi::{SpiHost, SpiWire};

/// The block's interrupt clear register and its receive-overrun bit (the
/// datasheet's register list for the block: SSPICR, RORIC).
const SSPICR: usize = 0x020;
const ICR_RORIC: u32 = 1 << 0;

/// The rates: the identification UNDER 400 kHz -- the ladder's step below it
/// at the crystal is 375 kHz (CPSDVSR 2, SCR 15) -- and the data phase at
/// the block's top, 6 MHz (CPSDVSR 2, SCR 0).
const IDENT_REQUEST_HZ: u32 = 399_999;
pub const DATA_HZ: u32 = 6_000_000;
const fn pair(hz: u32) -> (u32, u32) {
    match rp::spi_pair(rp::SPI_CLOCK_HZ, hz) {
        Some(p) => p,
        None => panic!("the SPI block's ladder has no step for the card host's rate"),
    }
}
const IDENT_PAIR: (u32, u32) = pair(IDENT_REQUEST_HZ);
const DATA_PAIR: (u32, u32) = pair(DATA_HZ);
pub const IDENT_HZ: u32 = IDENT_PAIR.1;
const _: () = assert!(IDENT_HZ == 375_000 && IDENT_PAIR.0 == 0x0F02 && DATA_PAIR.1 == DATA_HZ && DATA_PAIR.0 == 0x0002, "the card host's rates are not the pinned ones");

/// The block's name for the boot banner (the facts').
pub const BUS_NAME: &str = facts::SD_BUS_NAME;
/// The pump's bound on the bytes sent ahead of the bytes received: half the
/// receive queue, so the queue never overflows.
const AHEAD: usize = (rp::SPI_QUEUE / 2) as usize;
/// A queue flag is polled at most this many times for one byte: about a
/// millisecond of reads at either clock posture, where a byte takes 21
/// microseconds at the identification rate.
const BYTE_POLLS: u32 = crate::board::SYSCLK_HZ / 1_000;
/// The host's ticks do not run at the boot banner (the kernel's clock
/// advances with the tick, which starts after it).
#[cfg(feature = "api-smoke")]
pub const TICKS_AT_BOOT: bool = false;

/// The yields the long waits took once the scheduler ran (the cost a wait
/// puts on the calling thread).
pub static YIELDS: AtomicU32 = AtomicU32::new(0);
/// The witness words at the last refusal: the last R1 byte and the last
/// token the host read.
static LAST_R1: AtomicU32 = AtomicU32::new(0xFF);
static LAST_TOKEN: AtomicU32 = AtomicU32::new(0xFF);

/// The socket's wire: the block, the select, the detect line, the rate.
pub struct Wire {
    /// Whether the scheduler runs: a long wait yields between its polls and a
    /// pause sleeps when it does; the boot banner's identification runs before
    /// it and clocks instead.
    pub yielding: bool,
    /// The rate in force, in Hz (a pause before the scheduler is clocked at it).
    hz: u32,
}

/// The host the block layer holds.
pub type Host = SpiHost<Wire>;

pub const fn new() -> Host {
    SpiHost::new(Wire { yielding: false, hz: 0 })
}

pub fn set_yielding(h: &mut Host, on: bool) {
    h.wire.yielding = on;
}

#[cfg(feature = "api-smoke")]
pub fn yields() -> u32 {
    YIELDS.load(Ordering::Relaxed)
}

/// The two words of the last refusal.
pub fn last_words() -> (u32, u32) {
    (LAST_R1.load(Ordering::Relaxed), LAST_TOKEN.load(Ordering::Relaxed))
}

/// The host's ticks: the kernel's clock in microseconds (a reading taken
/// before the tick runs does not advance).
pub fn ticks() -> u32 {
    (sched::now_ns() / 1_000) as u32
}

/// The card-detect line's level as the pad reads it.
pub fn detect_level() -> bool {
    pins::read(facts::SD_DETECT_LINE)
}

impl Wire {
    /// The pump: a byte written while the transmit queue has room and the
    /// bytes in flight stay under the bound, a byte taken while the receive
    /// queue holds one; a flag that stays clear past the bound refuses as the
    /// timeout, and a receive queue that overran lost a byte, which refuses
    /// as the overrun after the flag is cleared.
    fn exchange(&mut self, tx: Option<&[u8]>, mut rx: Option<&mut [u8]>, len: usize) -> Result<(), Refusal> {
        let base = facts::SD_BUS.base;
        let (mut sent, mut received, mut polls) = (0usize, 0usize, 0u32);
        while received < len {
            let sr = hw::rd(base + SSPSR);
            if sent < len && sent - received < AHEAD && sr & SR_TNF != 0 {
                let b = tx.and_then(|t| t.get(sent).copied()).unwrap_or(0xFF);
                hw::wr(base + SSPDR, b as u32);
                sent += 1;
                polls = 0;
            } else if sr & SR_RNE != 0 {
                let b = hw::rd(base + SSPDR) as u8;
                if let Some(slot) = rx.as_deref_mut().and_then(|r| r.get_mut(received)) {
                    *slot = b;
                }
                received += 1;
                polls = 0;
            } else {
                polls += 1;
                if polls > BYTE_POLLS {
                    return Err(Refusal::Timeout);
                }
            }
        }
        if hw::rd(base + SSPRIS) & RIS_ROR != 0 {
            hw::wr(base + SSPICR, ICR_RORIC);
            return Err(Refusal::Overrun);
        }
        Ok(())
    }
}

impl SpiWire for Wire {
    /// The block taken as a bus is (its reset cycled, the three pads handed
    /// to it, the select given its resting level), the select a plain output
    /// at its released level, the detect line a plain input under the pull
    /// the facts name (the Metro rows' line rises with the socket empty only
    /// under the chip's pull-up: read on the board through the probe with the
    /// card out, low without the pull, high with it; the switch pulls the
    /// line low with a card seated).
    fn attach(&mut self) -> Result<(), Refusal> {
        rp2350_spi::claim(&facts::SD_BUS);
        hw::select_take(&facts::SD_BUS, 0);
        pins::configure(facts::SD_DETECT_LINE, g::MODE_INPUT, facts::SD_DETECT_PULL, false);
        Ok(())
    }

    /// The detect line against the level a seated card gives it (the facts).
    fn card_present(&mut self) -> Option<bool> {
        Some(pins::read(facts::SD_DETECT_LINE) == facts::SD_DETECT_CARD_LEVEL)
    }

    /// The block disabled, configured for the rate (mode 0, eight-bit frames,
    /// the prescaler pair) and enabled; the clock pad read until it rests low
    /// (the mode's idle level) before any select moves -- a clock that never
    /// rests is a block that is not serving: disabled and refused.
    fn rate(&mut self, rate: Rate) -> Result<u32, Refusal> {
        let (pair, hz) = match rate {
            Rate::Identification => IDENT_PAIR,
            Rate::Default => DATA_PAIR,
        };
        let base = facts::SD_BUS.base;
        hw::wr(base + SSPCR1, 0);
        hw::wr(base + SSPCR0, (((pair >> 8) & 0xFF) << CR0_SCR_SHIFT) | CR0_DSS_8);
        hw::wr(base + SSPCPSR, pair & 0xFF);
        hw::wr(base + SSPCR1, CR1_SSE);
        let mut polls = 0;
        while !hw::spi_clock_at(&facts::SD_BUS, false) {
            polls += 1;
            if polls > BYTE_POLLS {
                hw::wr(base + SSPCR1, 0);
                return Err(Refusal::Timeout);
            }
        }
        self.hz = hz;
        Ok(hz)
    }

    fn select(&mut self, low: bool) {
        hw::select_drive(&facts::SD_BUS, 0, low);
    }

    fn write(&mut self, tx: &[u8]) -> Result<(), Refusal> {
        self.exchange(Some(tx), None, tx.len())
    }

    fn read(&mut self, rx: &mut [u8]) -> Result<(), Refusal> {
        let n = rx.len();
        self.exchange(None, Some(rx), n)
    }

    /// A pause: the thread slept by the tick where the scheduler runs; the
    /// time clocked out at the rate in force before it (the select is high
    /// between commands, so the clocks reach no command).
    fn pause(&mut self, micros: Micros) {
        if self.yielding {
            sched::sleep_ms(micros.div_ceil(1000).max(1));
        } else {
            let bytes = (self.hz as u64 * micros as u64 / 8_000_000).max(1) as usize;
            let _ = self.exchange(None, None, bytes);
        }
    }

    fn breathe(&mut self) {
        if self.yielding {
            YIELDS.fetch_add(1, Ordering::Relaxed);
            sched::yield_now();
        }
    }

    fn fault(&mut self, r1: u8, token: u8) {
        LAST_R1.store(r1 as u32, Ordering::Relaxed);
        LAST_TOKEN.store(token as u32, Ordering::Relaxed);
    }
}
