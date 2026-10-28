//! The emulator rows' virtual medium: an Ethernet medium in kernel memory, no QEMU device.
//! Every frame the stack transmits is staged in a small ring; at the
//! network thread's service call each staged frame is offered to the
//! scripted peer (`net_peer`), which answers the exchanges a loopback
//! cannot (ARP for its address, DHCP, DNS, ICMP echo, a reset for its one
//! TCP port) with real checksums; a frame the peer does not answer is
//! handed back to the stack as received -- the loopback the socket doors
//! are proven over (one interface, a listening socket and a connecting
//! socket on the same address serve both ends of one connection). The
//! receive ring is sized from the gate's largest exchange; a reply or a
//! looped frame that finds no slot is DROPPED AND COUNTED, and the api
//! flavor's checks assert that count zero. The medium claims no line and
//! raises no interrupt: its wake word is the network module's, bumped by the
//! doors that queue work.

use core::sync::atomic::AtomicU32;

use crate::board::net_medium::{Link, Medium, RxSide, TxSide};
use crate::net_peer::{self, Peer, Verdict};

/// The frame unit (the Ethernet frame without its check sequence) and the
/// ring slot that holds one.
pub const FRAME_UNIT: usize = 1514;
pub const SLOT: usize = 1536;
/// The receive ring: the gate's largest exchange is the multi-segment echo
/// of `net-stream` (three segments out and their acknowledgements back,
/// looped) beside the peer's replies; eight slots hold it with room.
pub const RX_SLOTS: usize = 8;
/// The staging ring for the stack's own frames: the stack sends at most a
/// burst of one per poll (the capabilities say so), and the service call
/// drains the ring before every poll.
pub const TX_SLOTS: usize = 4;

/// The medium's counters, read by the api flavor's checks through the
/// network module (a copy under the tenancy).
#[derive(Clone, Copy)]
pub struct VirtWitness {
    pub frames_tx: u32,
    pub frames_rx: u32,
    pub looped: u32,
    pub answered: u32,
    pub dropped_full: u32,
    pub dropped_by_peer: u32,
    /// The head and the length of the last frame looped back (a frame the peer
    /// passed): the record of what the stack sent that nobody answered.
    pub looped_len: u32,
    pub looped_head: [u8; 48],
}

impl Default for VirtWitness {
    // By hand: the standard library derives Default for arrays of 32 at most.
    fn default() -> Self {
        VirtWitness { frames_tx: 0, frames_rx: 0, looped: 0, answered: 0, dropped_full: 0, dropped_by_peer: 0, looped_len: 0, looped_head: [0; 48] }
    }
}

struct Ring<const N: usize> {
    buf: [[u8; SLOT]; N],
    len: [u16; N],
    /// The oldest slot (the reader's) and the count held.
    tail: usize,
    count: usize,
}

impl<const N: usize> Ring<N> {
    const fn new() -> Self {
        Ring { buf: [[0; SLOT]; N], len: [0; N], tail: 0, count: 0 }
    }
    fn full(&self) -> bool {
        self.count == N
    }
    fn head(&self) -> usize {
        (self.tail + self.count) % N
    }
    /// The next free slot, whole.
    fn slot(&mut self) -> Option<&mut [u8]> {
        if self.full() {
            return None;
        }
        let h = self.head();
        Some(&mut self.buf[h][..])
    }
    fn commit(&mut self, len: usize) {
        let h = self.head();
        self.len[h] = len.min(SLOT) as u16;
        self.count += 1;
    }
    fn peek(&mut self) -> Option<&mut [u8]> {
        if self.count == 0 {
            return None;
        }
        let t = self.tail;
        let n = self.len[t] as usize;
        Some(&mut self.buf[t][..n])
    }
    fn pop(&mut self) {
        if self.count > 0 {
            self.tail = (self.tail + 1) % N;
            self.count -= 1;
        }
    }
}

pub struct VirtNet {
    rx: Ring<RX_SLOTS>,
    tx: Ring<TX_SLOTS>,
    peer: Peer,
    link: Link,
    wake: &'static AtomicU32,
    pub witness: VirtWitness,
}

impl VirtNet {
    pub const fn new(wake: &'static AtomicU32, arp_silent: bool) -> Self {
        VirtNet { rx: Ring::new(), tx: Ring::new(), peer: Peer::new(arp_silent), link: Link::Up, wake, witness: VirtWitness { frames_tx: 0, frames_rx: 0, looped: 0, answered: 0, dropped_full: 0, dropped_by_peer: 0, looped_len: 0, looped_head: [0; 48] } }
    }

    /// Whether frames wait in the staging ring (the service call has work).
    pub fn has_staged(&self) -> bool {
        self.tx.count > 0
    }

    /// The scripted peer's counters.
    pub fn peer(&self) -> &Peer {
        &self.peer
    }
}

impl RxSide for Ring<RX_SLOTS> {
    fn peek(&mut self) -> Option<&mut [u8]> {
        Ring::peek(self)
    }
    fn consume(&mut self) {
        self.pop();
    }
}

impl TxSide for Ring<TX_SLOTS> {
    fn room(&self) -> bool {
        !self.full()
    }
    fn slot(&mut self, len: usize) -> Option<&mut [u8]> {
        if len > SLOT {
            return None;
        }
        Ring::slot(self)
    }
    fn commit(&mut self, len: usize) {
        Ring::commit(self, len);
    }
}

impl Medium for VirtNet {
    fn hardware_address(&self) -> Option<[u8; 6]> {
        Some(net_peer::OUR_MAC)
    }

    fn link(&self) -> Link {
        self.link
    }

    fn wake_word(&self) -> &AtomicU32 {
        self.wake
    }

    /// The staged frames moved: each offered to the peer, its reply or the
    /// frame itself into the receive ring; a frame that finds no slot is
    /// dropped and counted.
    fn service(&mut self, _now_ms: u32) {
        while let Some(frame) = Ring::peek(&mut self.tx) {
            self.witness.frames_tx += 1;
            let n = frame.len();
            // The reply is built in the receive ring's next slot directly
            // (no staging copy); a looped frame is copied there.
            let full = self.rx.full();
            let verdict = if full {
                // No room: the peer is still asked, so its counters move as
                // they would, and the frame is dropped and counted.
                let mut scratch = [0u8; net_peer::MAX_REPLY];
                self.peer.answer(frame, &mut scratch)
            } else {
                let h = self.rx.head();
                self.peer.answer(frame, &mut self.rx.buf[h][..])
            };
            match verdict {
                Verdict::Drop => self.witness.dropped_by_peer += 1,
                Verdict::Reply(len) => {
                    if full {
                        self.witness.dropped_full += 1;
                    } else {
                        self.rx.commit(len);
                        self.witness.answered += 1;
                        self.witness.frames_rx += 1;
                    }
                }
                Verdict::Pass => {
                    if full {
                        self.witness.dropped_full += 1;
                    } else {
                        let h = self.rx.head();
                        let (src, dst) = (&self.tx.buf[self.tx.tail][..n], &mut self.rx.buf[h][..n]);
                        dst.copy_from_slice(src);
                        self.rx.commit(n);
                        let k = n.min(self.witness.looped_head.len());
                        self.witness.looped_head[..k].copy_from_slice(&self.tx.buf[self.tx.tail][..k]);
                        self.witness.looped_len = n as u32;
                        self.witness.looped += 1;
                        self.witness.frames_rx += 1;
                    }
                }
            }
            self.tx.pop();
        }
    }

    fn split(&mut self) -> (&mut dyn RxSide, &mut dyn TxSide) {
        (&mut self.rx, &mut self.tx)
    }

    fn frame_unit(&self) -> usize {
        FRAME_UNIT
    }
}
