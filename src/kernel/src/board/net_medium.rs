//! The medium seam: the
//! one small contract between the kernel's network stack and a medium
//! driver -- the emulator rows' virtual medium. A medium
//! tells its hardware address and its link state, hands received frames to
//! the stack IN PLACE, takes the stack's frames into a slot of its own,
//! names the word its interrupt (or its timer) bumps when something
//! arrived, and does its bounded service work when the network thread asks.
//!
//! THE DISJOINT-BORROW RULE: the stack asks for the receive side
//! and the transmit side together, so a medium gives them as two disjoint
//! borrows (`split`) -- its receive ring and its transmit slot are separate
//! fields by construction, and no frame is copied through a staging buffer
//! to satisfy the borrow checker. The adapter below presents any medium to
//! the stack as its network device; it carries no controller-specific
//! knowledge.

use core::sync::atomic::AtomicU32;

use smoltcp::phy::{Device, DeviceCapabilities, Medium as PhyMedium, RxToken, TxToken};
use smoltcp::time::Instant;

/// The link, as the medium reads it: `Down` or `Up`. A lease is not a seam
/// state.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Link {
    #[allow(dead_code)] // the wired medium reads it from its PHY; the virtual medium is always up
    Down,
    Up,
}

/// The receive side: the next received frame in place, consumed after the
/// stack read it.
pub trait RxSide {
    /// The next received frame, whole, in the medium's own storage; `None`
    /// when nothing arrived.
    fn peek(&mut self) -> Option<&mut [u8]>;
    /// The frame `peek` answered is consumed (the ring slot given back).
    fn consume(&mut self);
}

/// The transmit side: a slot the stack fills, written to the wire when the
/// stack commits it.
pub trait TxSide {
    /// Whether a slot is free: the adapter gives the stack a transmit token
    /// only then, so the stack retries at its next poll and nothing is dropped.
    fn room(&self) -> bool;
    /// A slot of at least `len` bytes for the stack to fill (`room` was true).
    fn slot(&mut self, len: usize) -> Option<&mut [u8]>;
    /// The slot last given is on its way: `len` bytes.
    fn commit(&mut self, len: usize);
}

/// The medium.
pub trait Medium {
    /// Known once the medium is up.
    fn hardware_address(&self) -> Option<[u8; 6]>;
    fn link(&self) -> Link;
    /// The word the medium's interrupt handler or timer bumps (and wakes)
    /// when frames arrived or the link changed; the network thread sleeps on it.
    fn wake_word(&self) -> &AtomicU32;
    /// The medium's bounded work per poll (a descriptor ring reclaimed, a
    /// PHY status read; the virtual medium's staged frames moved).
    fn service(&mut self, now_ms: u32);
    /// The receive side and the transmit side, two disjoint borrows.
    fn split(&mut self) -> (&mut dyn RxSide, &mut dyn TxSide);
    /// The largest frame the medium carries (the Ethernet unit, 1514).
    fn frame_unit(&self) -> usize;
}

/// Any medium as the stack's device.
pub struct Adapter<'m> {
    pub medium: &'m mut dyn Medium,
}

pub struct Rx<'a> {
    rx: &'a mut dyn RxSide,
}

pub struct Tx<'a> {
    tx: &'a mut dyn TxSide,
}

impl Device for Adapter<'_> {
    type RxToken<'a>
        = Rx<'a>
    where
        Self: 'a;
    type TxToken<'a>
        = Tx<'a>
    where
        Self: 'a;

    fn receive(&mut self, _now: Instant) -> Option<(Self::RxToken<'_>, Self::TxToken<'_>)> {
        let (rx, tx) = self.medium.split();
        if rx.peek().is_none() || !tx.room() {
            return None;
        }
        Some((Rx { rx }, Tx { tx }))
    }

    fn transmit(&mut self, _now: Instant) -> Option<Self::TxToken<'_>> {
        let (_, tx) = self.medium.split();
        if !tx.room() {
            return None;
        }
        Some(Tx { tx })
    }

    fn capabilities(&self) -> DeviceCapabilities {
        let mut caps = DeviceCapabilities::default();
        caps.medium = PhyMedium::Ethernet;
        caps.max_transmission_unit = self.medium.frame_unit();
        caps.max_burst_size = Some(1);
        caps
    }
}

impl RxToken for Rx<'_> {
    fn consume<R, F>(self, f: F) -> R
    where
        F: FnOnce(&[u8]) -> R,
    {
        let r = {
            let frame = self.rx.peek().expect("the receive token was given with a frame in place");
            f(frame)
        };
        self.rx.consume();
        r
    }
}

impl TxToken for Tx<'_> {
    fn consume<R, F>(self, len: usize, f: F) -> R
    where
        F: FnOnce(&mut [u8]) -> R,
    {
        // The token was given with `room` true, so the slot is there; a
        // medium that answers otherwise has broken the seam's contract,
        // which is a kernel bug and stays loud.
        let slot = self.tx.slot(len).expect("the transmit token was given with a slot free");
        let r = f(&mut slot[..len]);
        self.tx.commit(len);
        r
    }
}
