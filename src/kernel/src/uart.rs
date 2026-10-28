//! The UART doors' bodies: a program's serial port on a UART block
//! of the board's facts table -- the twelve verbs `api.rs` exports and
//! `include/rhu.h` states (`rhu_uart_count`, `rhu_uart_port`,
//! `rhu_uart_claim`, `rhu_uart_configure`, `rhu_uart_write`,
//! `rhu_uart_read`, `rhu_uart_status`, `rhu_uart_event_word`,
//! `rhu_uart_flush`, `rhu_uart_break`, `rhu_uart_release`,
//! `rhu_uart_info`). A row's ports are a TABLE in its facts
//! (`board::facts::UART_ROWS`: every UART the board pre-defines, numbered
//! by the chip's own block number, with its two pins where the row has
//! pins), so a program enumerates every UART and learns which one it may
//! open: a block the kernel's console holds as a stream's wire is IN the
//! table and REFUSED at the claim with the busy code.
//!
//! Beneath the doors: the rings (a receive ring and a transmit ring per
//! port, single producer single consumer with free-running indices read
//! whole), the board's backend (`board::uart_backend`: the registers, the
//! pins, the interrupt lines), the claims table (a claim takes the port's
//! two lines as BOARD claims under one critical section, all or nothing, so
//! a GPIO claim of either after it is refused with the holder named) and
//! the event word per port -- a futex word a waiting thread sleeps on
//! through `rhu_futex_wait`, bumped once per interrupt pass that moved a
//! byte in or made room out, and once more at the release so a thread
//! parked on a port another thread closes wakes (the word's storage is a
//! static, readable after the release).
//!
//! The witness `__rhu_uart_witness` is read by symbol (a debugger, the api
//! flavor's checks). The transmit ring is sized so a
//! verbose boot report fits the wire of a held console port without loss
//! (4 KB); the receive ring holds
//! 89 ms of a 115200 Bd wire (1 KB).

#![cfg_attr(not(rhu_uart_rows), allow(dead_code))] // a row without a UART row compiles the doors' helpers unused

use core::ptr::{addr_of_mut, read_volatile, write_volatile};

use crate::board::facts;
use crate::critical::with_cs;
use crate::sched;

/// The code this module adds to the hosting API's set (`include/rhu.h`).
pub const ERR_BUSY: i32 = -16;

/// The row's port count, constant per image (the facts table's rows).
pub const PORTS: usize = facts::UART_ROWS.len();

/// The ring sizes: the receive ring (89 ms of a 115200 Bd wire, 13.6 ms at
/// 750,000 Bd), the transmit ring (a verbose boot report's burst on a held
/// console port).
pub const RX_BYTES: usize = 1024;
pub const TX_BYTES: usize = 4096;

/// The parity codes (`RHU_UART_PARITY_*`) and the flush selectors
/// (`RHU_UART_FLUSH_*`) the header states.
pub const PARITY_NONE: u32 = 0;
#[allow(dead_code)] // the PL011 rows' frame set; the emulator rows take 8N1 alone
pub const PARITY_ODD: u32 = 1;
#[allow(dead_code)]
pub const PARITY_EVEN: u32 = 2;
#[allow(dead_code)]
pub const PARITY_MARK: u32 = 3;
pub const PARITY_SPACE: u32 = 4;
pub const FLUSH_RX: u32 = 1;
pub const FLUSH_TX: u32 = 2;
pub const FLUSH_BOTH: u32 = 3;
/// The pin answer of a port on a row whose UARTs have no pins
/// (`RHU_UART_NO_PIN`: the emulator rows).
pub const NO_PIN: u32 = crate::uart_math::NO_PIN;

/// The holder of a port: nobody, a program (through the claim door), or
/// the kernel's console (a stream's wire).
const FREE: u8 = 0;
const PROGRAM: u8 = 1;
const CONSOLE: u8 = 2;

/// The claim's line state, the termios image a hosting layer mirrors:
/// 115200 8N1.
pub const DEFAULT_BAUD: u32 = 115_200;

/// The witness, by symbol. The counters are cumulative since boot;
/// `last_refused_block` names the block, or the handle, the last refusal
/// was asked with.
#[repr(C)]
pub struct UartWitness {
    pub ports: u32,
    pub claims_taken: u32,
    pub releases: u32,
    pub configures: u32,
    pub bytes_in: u32,
    pub bytes_out: u32,
    pub irqs: u32,
    pub overruns: u32,
    pub framing: u32,
    pub parity: u32,
    pub breaks_seen: u32,
    pub rx_dropped: u32,
    pub tx_dropped: u32,
    pub wakes_sent: u32,
    pub refused_busy: u32,
    pub refused_inval: u32,
    pub last_refused_block: u32,
    pub last_refused_code: i32,
}

#[unsafe(export_name = "__rhu_uart_witness")]
pub static mut WITNESS: UartWitness = UartWitness {
    ports: 0,
    claims_taken: 0,
    releases: 0,
    configures: 0,
    bytes_in: 0,
    bytes_out: 0,
    irqs: 0,
    overruns: 0,
    framing: 0,
    parity: 0,
    breaks_seen: 0,
    rx_dropped: 0,
    tx_dropped: 0,
    wakes_sent: 0,
    refused_busy: 0,
    refused_inval: 0,
    last_refused_block: 0,
    last_refused_code: 0,
};

/// The witness's fields are written through raw pointers, one field at a
/// time, under the critical section by the doors and by the handler the
/// section masks; no reference to the static is ever formed.
fn witness() -> *mut UartWitness {
    addr_of_mut!(WITNESS)
}

unsafe fn bump(field: *mut u32, by: u32) {
    unsafe { write_volatile(field, read_volatile(field).wrapping_add(by)) }
}

fn refused(block: u32, code: i32) -> i32 {
    with_cs(|| unsafe {
        let w = witness();
        match code {
            ERR_BUSY => bump(addr_of_mut!((*w).refused_busy), 1),
            _ => bump(addr_of_mut!((*w).refused_inval), 1),
        }
        write_volatile(addr_of_mut!((*w).last_refused_block), block);
        write_volatile(addr_of_mut!((*w).last_refused_code), code);
    });
    code
}

/// The table's size recorded; nothing else moves at boot (a port is held by
/// the console at the console's init, or claimed by a program, never here).
pub fn init() {
    with_cs(|| unsafe { write_volatile(addr_of_mut!((*witness()).ports), PORTS as u32) });
}

pub fn count() -> i32 {
    PORTS as i32
}

/// The table index of a block number, if the row pre-defines it.
fn index_of(block: u32) -> Option<usize> {
    facts::UART_ROWS.iter().position(|r| r.block as u32 == block)
}

/// A facts pin as the doors answer it.
fn pin_word(p: u8) -> u32 {
    if p == crate::console::NO_PIN { NO_PIN } else { p as u32 }
}

/// The enumeration: the index-th row's facts -- the block number, the two
/// pins a claim would take (the project's pin words' pair when they name a
/// legal pair for the block, else the row's pre-defined pair; `NO_PIN` on
/// a row whose UARTs have no pins) and the UART's input clock; the invalid
/// code past the count or for a NULL or misaligned pointer.
pub fn port(index: u32, block: *mut u32, tx: *mut u32, rx: *mut u32, clock_hz: *mut u32) -> i32 {
    let Some(row) = facts::UART_ROWS.get(index as usize) else {
        return refused(index, sched::ERR_INVAL);
    };
    if block.is_null() || tx.is_null() || rx.is_null() || clock_hz.is_null() || (block as usize | tx as usize | rx as usize | clock_hz as usize) & 3 != 0 {
        return refused(index, sched::ERR_INVAL);
    }
    let (ptx, prx) = served::pins_of(index as usize);
    unsafe {
        block.write(row.block as u32);
        tx.write(ptx);
        rx.write(prx);
        clock_hz.write(served::CLOCK_HZ);
    }
    0
}

// ---------------- the rows with a table: the doors over the backend ----------------

#[cfg(rhu_uart_rows)]
mod served {
    use super::*;
    use crate::board::uart_backend as hw;
    use crate::claims;
    use core::sync::atomic::{AtomicBool, AtomicU32, AtomicU8, Ordering};

    pub const CLOCK_HZ: u32 = hw::CLOCK_HZ;

    /// The rings' storage, written through raw pointers under the section
    /// (the doors) or in the band the section masks (the handler).
    #[repr(C)]
    pub struct Rings {
        rx: [u8; RX_BYTES],
        tx: [u8; TX_BYTES],
    }
    static mut RINGS: [Rings; PORTS] = [const { Rings { rx: [0; RX_BYTES], tx: [0; TX_BYTES] } }; PORTS];
    /// The free-running indices: the receive ring's head (the handler
    /// produces) and tail (the doors consume), the transmit ring's head
    /// (the doors produce) and tail (the handler and the primes consume).
    static RX_HEAD: [AtomicU32; PORTS] = [const { AtomicU32::new(0) }; PORTS];
    static RX_TAIL: [AtomicU32; PORTS] = [const { AtomicU32::new(0) }; PORTS];
    static TX_HEAD: [AtomicU32; PORTS] = [const { AtomicU32::new(0) }; PORTS];
    static TX_TAIL: [AtomicU32; PORTS] = [const { AtomicU32::new(0) }; PORTS];
    /// The event words: each one a futex word (the storage stays readable
    /// after a release).
    static EVENT: [AtomicU32; PORTS] = [const { AtomicU32::new(0) }; PORTS];
    static HOLDER: [AtomicU8; PORTS] = [const { AtomicU8::new(FREE) }; PORTS];
    /// The effective pins of a held port (the facts' pair or the project's
    /// override), `NO_PIN` on a row without pins.
    static PIN_TX: [AtomicU32; PORTS] = [const { AtomicU32::new(NO_PIN) }; PORTS];
    static PIN_RX: [AtomicU32; PORTS] = [const { AtomicU32::new(NO_PIN) }; PORTS];
    /// The bytes handed to the transmitter since it was last seen idle: the
    /// FIFO's share of `tx_pending` (the PL011 has no FIFO level register,
    /// so the count is an upper bound while the wire is busy and exactly 0
    /// when it is idle -- the drain's witness).
    static FIFO_HANDED: [AtomicU32; PORTS] = [const { AtomicU32::new(0) }; PORTS];
    static BREAK_ON: [AtomicU8; PORTS] = [const { AtomicU8::new(0) }; PORTS];
    /// Set by the funnel through the console: every held port drains
    /// synchronously from here on.
    static POLLED: AtomicBool = AtomicBool::new(false);

    fn rx_ring(ix: usize) -> *mut u8 {
        unsafe { addr_of_mut!((*addr_of_mut!(RINGS[ix])).rx) as *mut u8 }
    }
    fn tx_ring(ix: usize) -> *mut u8 {
        unsafe { addr_of_mut!((*addr_of_mut!(RINGS[ix])).tx) as *mut u8 }
    }
    fn rx_avail(ix: usize) -> u32 {
        RX_HEAD[ix].load(Ordering::Acquire).wrapping_sub(RX_TAIL[ix].load(Ordering::Acquire))
    }
    fn tx_backlog(ix: usize) -> u32 {
        TX_HEAD[ix].load(Ordering::Acquire).wrapping_sub(TX_TAIL[ix].load(Ordering::Acquire))
    }
    fn tx_room(ix: usize) -> u32 {
        TX_BYTES as u32 - tx_backlog(ix)
    }

    /// The effective pins of a port: the project's words (0x100 | gpio when
    /// named; 0 unset; anything else malformed) judged by `uart_math` against
    /// the chip's legal pin sets, else the facts' pair. The second word says
    /// whether the pins are legal: a named pair that is not legal is not; and
    /// a block with NO
    /// default pair on a row whose chip lists legal pins for it (the Pico
    /// rows' UART1) is legal only when the project's words name a legal pair,
    /// while a row whose UARTs have no pins at all (the emulator rows: no
    /// sets) is legal as it stands. The host tier proves the judgement on the
    /// RP2350 rows' tables (the pin override's kernel half).
    pub fn pins_for(ix: usize) -> (u32, u32, bool) {
        let row = &facts::UART_ROWS[ix];
        let sets = sets_of(row.block);
        let (wtx, wrx) = crate::console::pin_words(row.block);
        crate::uart_math::effective_pair((pin_word(row.tx), pin_word(row.rx)), sets.as_ref(), wtx, wrx)
    }

    /// The chip's legal pin sets for a block (the facts' `UART_PAIRS` row of
    /// the block), none on a chip without a table.
    fn sets_of(block: u8) -> Option<crate::uart_math::PinSets<'static>> {
        facts::UART_PAIRS.iter().find(|p| p.block == block).map(|p| crate::uart_math::PinSets { tx: p.tx, rx: p.rx })
    }

    /// Whether a block can serve a stream's wire: its pins legal (the facts'
    /// pair, or the project's legal words), or the facts' pair standing behind
    /// illegal words (taken, as the refusal line says); a block with no default
    /// pair and no legal words serves none.
    pub fn block_serves_wire(block: u32) -> bool {
        match index_of(block) {
            Some(ix) => {
                let (tx, _, legal) = pins_for(ix);
                legal || tx != NO_PIN
            }
            None => false,
        }
    }

    /// The pins the enumeration answers: a held port's effective pins, else
    /// the pair a claim would take (the project's words when they name a
    /// legal pair for the block, else the facts' pair), so a program learns a
    /// port's pins before it opens the port (the pin override's kernel half).
    pub fn pins_of(ix: usize) -> (u32, u32) {
        if HOLDER[ix].load(Ordering::Acquire) != FREE {
            (PIN_TX[ix].load(Ordering::Acquire), PIN_RX[ix].load(Ordering::Acquire))
        } else {
            let (tx, rx, _) = pins_for(ix);
            (tx, rx)
        }
    }

    /// The table index of a handle a program holds.
    fn held(h: i32) -> Option<usize> {
        if h < 0 || h as usize >= PORTS || HOLDER[h as usize].load(Ordering::Acquire) != PROGRAM {
            None
        } else {
            Some(h as usize)
        }
    }

    /// The transmitter primed from the ring while it has room: a bounded
    /// register loop, never a wait on the wire. The caller holds the
    /// section or runs in the band.
    fn kick(ix: usize) -> u32 {
        let head = TX_HEAD[ix].load(Ordering::Acquire);
        let mut tail = TX_TAIL[ix].load(Ordering::Acquire);
        let mut moved = 0u32;
        let ring = tx_ring(ix);
        while tail != head && hw::tx_room(ix) {
            let b = unsafe { read_volatile(ring.add(tail as usize % TX_BYTES)) };
            hw::tx_put(ix, b);
            tail = tail.wrapping_add(1);
            moved += 1;
        }
        if moved > 0 {
            TX_TAIL[ix].store(tail, Ordering::Release);
            FIFO_HANDED[ix].fetch_add(moved, Ordering::AcqRel);
            unsafe { bump(addr_of_mut!((*witness()).bytes_out), moved) };
            if HOLDER[ix].load(Ordering::Acquire) == CONSOLE {
                crate::board::ring::account_drained(moved);
            }
        }
        moved
    }

    /// The transmit ring pushed through the wire to the last byte written,
    /// waiting on the transmitter between refills (the polled mode of the
    /// panic and fault paths, and the exit's drain). Each refill runs under
    /// the section, so a transmit interrupt never moves the ring's tail
    /// beside it; the wait on the transmitter runs outside it.
    fn drain_polled(ix: usize) {
        loop {
            with_cs(|| kick(ix));
            if tx_backlog(ix) == 0 {
                return;
            }
            while !hw::tx_room(ix) {}
        }
    }

    /// The bytes pending on the transmit side: the ring's backlog and the
    /// transmitter's share (0 exactly when the wire is idle).
    fn tx_pending(ix: usize) -> u32 {
        let backlog = tx_backlog(ix);
        if hw::tx_idle(ix) && backlog == 0 {
            FIFO_HANDED[ix].store(0, Ordering::Release);
            0
        } else {
            backlog + FIFO_HANDED[ix].load(Ordering::Acquire).clamp(1, hw::FIFO_DEPTH)
        }
    }

    fn rings_reset(ix: usize) {
        RX_TAIL[ix].store(RX_HEAD[ix].load(Ordering::Acquire), Ordering::Release);
        TX_TAIL[ix].store(TX_HEAD[ix].load(Ordering::Acquire), Ordering::Release);
        FIFO_HANDED[ix].store(0, Ordering::Release);
    }

    /// The port's two lines taken as BOARD claims, all or nothing (a row
    /// without pins takes none).
    fn claim_lines(ix: usize, tx: u32, rx: u32) -> i32 {
        if tx == NO_PIN {
            return 0;
        }
        let _ = ix;
        unsafe {
            let c = &mut *addr_of_mut!(crate::gpio::CLAIMS);
            let r = claims::claim_board(c, tx);
            if r != 0 {
                return r;
            }
            let r = claims::claim_board(c, rx);
            if r != 0 {
                claims::release_board(c, tx);
                return r;
            }
        }
        0
    }

    fn release_lines(tx: u32, rx: u32) {
        if tx == NO_PIN {
            return;
        }
        unsafe {
            let c = &mut *addr_of_mut!(crate::gpio::CLAIMS);
            claims::release_board(c, tx);
            claims::release_board(c, rx);
        }
    }

    /// A claim: the block inside the table and free (the console's block,
    /// or a block already claimed, is RHU_EBUSY), the pins legal, then the
    /// two lines taken under one section, then the block brought up at the
    /// claim's state (115200 8N1, the FIFOs on, the receiver on, the
    /// interrupts armed) with the rings empty. The handle is the table
    /// index.
    pub fn claim(block: u32) -> i32 {
        let Some(ix) = index_of(block) else {
            return refused(block, sched::ERR_INVAL);
        };
        let (tx, rx, legal) = pins_for(ix);
        if !legal {
            // The cause in the line's own words: no words on a block without a
            // pre-defined pair, or words that are not a legal pair (the legal
            // pins named by number from the facts).
            let named = crate::console::pin_words(block as u8) != (0, 0);
            let words = if named { crate::uart_math::REFUSED_NOT_LEGAL } else { crate::uart_math::REFUSED_NO_DEFAULT };
            let sets = sets_of(block as u8);
            klog!(crate::console::LEVEL_VERBOSE, "[uart] UART{} refused: {} (the legal pins: {})", block, words, crate::uart_math::LegalPins(sets.as_ref()));
            return refused(block, sched::ERR_INVAL);
        }
        let r = with_cs(|| {
            if HOLDER[ix].load(Ordering::Acquire) != FREE {
                return ERR_BUSY;
            }
            let r = claim_lines(ix, tx, rx);
            if r != 0 {
                return r;
            }
            HOLDER[ix].store(PROGRAM, Ordering::Release);
            PIN_TX[ix].store(tx, Ordering::Release);
            PIN_RX[ix].store(rx, Ordering::Release);
            rings_reset(ix);
            BREAK_ON[ix].store(0, Ordering::Release);
            0
        });
        if r != 0 {
            return refused(block, r);
        }
        hw::claim(ix, tx, rx);
        with_cs(|| unsafe { bump(addr_of_mut!((*witness()).claims_taken), 1) });
        ix as i32
    }

    /// The line discipline, live: the five values judged first (the rate
    /// law by the backend's divisor, the frame set of the part), the
    /// pending transmit allowed to leave (the door discards nothing; a
    /// hosting layer drains first), then the block reprogrammed under the
    /// section with the achieved rate answered.
    pub fn configure(h: i32, baud: u32, data_bits: u32, parity: u32, stop_bits: u32, achieved_baud: *mut u32) -> i32 {
        let Some(ix) = held(h) else {
            return refused(h as u32, sched::ERR_INVAL);
        };
        let block = facts::UART_ROWS[ix].block as u32;
        if achieved_baud.is_null() || (achieved_baud as usize) & 3 != 0 {
            return refused(block, sched::ERR_INVAL);
        }
        let frame_ok = if hw::FRAMES_8N1_ONLY {
            data_bits == 8 && parity == PARITY_NONE && stop_bits == 1
        } else {
            (5..=8).contains(&data_bits) && parity <= PARITY_SPACE && (1..=2).contains(&stop_bits)
        };
        if !frame_ok {
            return refused(block, sched::ERR_INVAL);
        }
        let Some((regs, achieved)) = hw::divisor(baud) else {
            return refused(block, sched::ERR_INVAL);
        };
        // The pending transmit leaves at the old rate before the block is
        // reprogrammed: a wait outside the section, bounded by the wire.
        while tx_pending(ix) != 0 {
            core::hint::spin_loop();
        }
        with_cs(|| {
            hw::configure(ix, regs, data_bits, parity, stop_bits, BREAK_ON[ix].load(Ordering::Acquire) != 0);
            unsafe { bump(addr_of_mut!((*witness()).configures), 1) };
        });
        unsafe { achieved_baud.write(achieved) };
        0
    }

    /// The bytes the transmit ring has room for, 0 to n, copied under the
    /// section and the transmitter primed -- never a wait for room (a
    /// hosting layer's blocking write loops on the event word).
    pub fn write(h: i32, buf: *const u8, n: u32) -> i32 {
        let Some(ix) = held(h) else {
            return refused(h as u32, sched::ERR_INVAL);
        };
        if buf.is_null() && n > 0 {
            return refused(facts::UART_ROWS[ix].block as u32, sched::ERR_INVAL);
        }
        if n == 0 {
            return 0;
        }
        with_cs(|| {
            let room = tx_room(ix).min(n);
            let mut head = TX_HEAD[ix].load(Ordering::Acquire);
            let ring = tx_ring(ix);
            for i in 0..room as usize {
                unsafe { write_volatile(ring.add(head as usize % TX_BYTES), *buf.add(i)) };
                head = head.wrapping_add(1);
            }
            TX_HEAD[ix].store(head, Ordering::Release);
            if room > 0 {
                if POLLED.load(Ordering::Acquire) { drain_polled(ix) } else { kick(ix); }
            }
            room as i32
        })
    }

    /// The bytes pending in the receive ring, 0 to n (0 when empty; never a
    /// wait); raw bytes, no echo, no translation.
    pub fn read(h: i32, buf: *mut u8, n: u32) -> i32 {
        let Some(ix) = held(h) else {
            return refused(h as u32, sched::ERR_INVAL);
        };
        if buf.is_null() && n > 0 {
            return refused(facts::UART_ROWS[ix].block as u32, sched::ERR_INVAL);
        }
        if n == 0 {
            return 0;
        }
        with_cs(|| {
            let avail = rx_avail(ix).min(n);
            let mut tail = RX_TAIL[ix].load(Ordering::Acquire);
            let ring = rx_ring(ix);
            for i in 0..avail as usize {
                unsafe { *buf.add(i) = read_volatile(ring.add(tail as usize % RX_BYTES)) };
                tail = tail.wrapping_add(1);
            }
            RX_TAIL[ix].store(tail, Ordering::Release);
            avail as i32
        })
    }

    /// The three counts a hosting layer's poll, FIONREAD and TIOCOUTQ need.
    pub fn status(h: i32, rx_avail_out: *mut u32, tx_pending_out: *mut u32, tx_room_out: *mut u32) -> i32 {
        let Some(ix) = held(h) else {
            return refused(h as u32, sched::ERR_INVAL);
        };
        if rx_avail_out.is_null() || tx_pending_out.is_null() || tx_room_out.is_null() || (rx_avail_out as usize | tx_pending_out as usize | tx_room_out as usize) & 3 != 0 {
            return refused(facts::UART_ROWS[ix].block as u32, sched::ERR_INVAL);
        }
        let (a, p, r) = with_cs(|| (rx_avail(ix), tx_pending(ix), tx_room(ix)));
        unsafe {
            rx_avail_out.write(a);
            tx_pending_out.write(p);
            tx_room_out.write(r);
        }
        0
    }

    /// The port's event word, or NULL for a handle the program does not hold.
    pub fn event_word(h: i32) -> *const u32 {
        match held(h) {
            Some(ix) => EVENT[ix].as_ptr(),
            None => core::ptr::null(),
        }
    }

    /// 1 = the receive ring and FIFO discarded, 2 = the transmit ring
    /// discarded (the transmitter's FIFO finishes on the wire), 3 = both.
    pub fn flush(h: i32, which: u32) -> i32 {
        let Some(ix) = held(h) else {
            return refused(h as u32, sched::ERR_INVAL);
        };
        if which == 0 || which > FLUSH_BOTH {
            return refused(facts::UART_ROWS[ix].block as u32, sched::ERR_INVAL);
        }
        with_cs(|| {
            if which & FLUSH_RX != 0 {
                hw::rx_flush(ix);
                RX_TAIL[ix].store(RX_HEAD[ix].load(Ordering::Acquire), Ordering::Release);
            }
            if which & FLUSH_TX != 0 {
                TX_TAIL[ix].store(TX_HEAD[ix].load(Ordering::Acquire), Ordering::Release);
            }
        });
        0
    }

    /// The break condition held (1) or released (0); RHU_EINVAL for on > 1
    /// or on a part without break control.
    pub fn brk(h: i32, on: u32) -> i32 {
        let Some(ix) = held(h) else {
            return refused(h as u32, sched::ERR_INVAL);
        };
        if on > 1 || !hw::HAS_BREAK {
            return refused(facts::UART_ROWS[ix].block as u32, sched::ERR_INVAL);
        }
        with_cs(|| {
            hw::set_break(ix, on != 0);
            BREAK_ON[ix].store(on as u8, Ordering::Release);
            if on != 0 {
                unsafe { bump(addr_of_mut!((*witness()).breaks_seen), 1) };
            }
        });
        0
    }

    /// The release: the block disabled and held in reset, its interrupts
    /// masked, the rings emptied (a hosting layer drains first), the pads
    /// returned, the two lines released, the event word bumped and every
    /// waiter woken once (the hosting layer's ask).
    pub fn release(h: i32) -> i32 {
        let Some(ix) = held(h) else {
            return refused(h as u32, sched::ERR_INVAL);
        };
        let (tx, rx) = (PIN_TX[ix].load(Ordering::Acquire), PIN_RX[ix].load(Ordering::Acquire));
        hw::release(ix, tx, rx);
        with_cs(|| {
            rings_reset(ix);
            BREAK_ON[ix].store(0, Ordering::Release);
            release_lines(tx, rx);
            HOLDER[ix].store(FREE, Ordering::Release);
            unsafe { bump(addr_of_mut!((*witness()).releases), 1) };
        });
        EVENT[ix].fetch_add(1, Ordering::AcqRel);
        let woken = sched::futex_wake(EVENT[ix].as_ptr(), u32::MAX);
        if woken > 0 {
            with_cs(|| unsafe { bump(addr_of_mut!((*witness()).wakes_sent), woken as u32) });
        }
        0
    }

    /// The held port's facts (the PWM info door's shape).
    pub fn info(h: i32, block: *mut u32, tx: *mut u32, rx: *mut u32, clock_hz: *mut u32) -> i32 {
        let Some(ix) = held(h) else {
            return refused(h as u32, sched::ERR_INVAL);
        };
        if block.is_null() || tx.is_null() || rx.is_null() || clock_hz.is_null() || (block as usize | tx as usize | rx as usize | clock_hz as usize) & 3 != 0 {
            return refused(facts::UART_ROWS[ix].block as u32, sched::ERR_INVAL);
        }
        unsafe {
            block.write(facts::UART_ROWS[ix].block as u32);
            tx.write(PIN_TX[ix].load(Ordering::Acquire));
            rx.write(PIN_RX[ix].load(Ordering::Acquire));
            clock_hz.write(CLOCK_HZ);
        }
        0
    }

    // ---------------- the handler's half ----------------

    /// The interrupt's body, called by the backend's handler for a block:
    /// the pending sources acknowledged first (a level condition still
    /// present re-raises after the return), the receive FIFO drained into
    /// the ring (a byte with an error flag dropped and counted, a full ring
    /// dropping the newest byte and counting it), the transmitter refilled
    /// from the ring, the event word bumped and its waiters woken once when
    /// a byte came in or room was made.
    pub fn irq(ix: usize) {
        if ix >= PORTS {
            return;
        }
        let flags = hw::ack(ix);
        unsafe {
            let w = witness();
            bump(addr_of_mut!((*w).irqs), 1);
            if flags & hw::ACK_OVERRUN != 0 {
                bump(addr_of_mut!((*w).overruns), 1);
            }
        }
        if HOLDER[ix].load(Ordering::Acquire) == FREE {
            hw::rx_flush(ix);
            return;
        }
        let mut moved_in = false;
        let mut head = RX_HEAD[ix].load(Ordering::Acquire);
        let tail = RX_TAIL[ix].load(Ordering::Acquire);
        let ring = rx_ring(ix);
        while let Some((b, err)) = hw::rx_get(ix) {
            if err != 0 {
                unsafe {
                    let w = witness();
                    if err & hw::ERR_OVERRUN != 0 {
                        bump(addr_of_mut!((*w).overruns), 1);
                    }
                    if err & hw::ERR_FRAMING != 0 {
                        bump(addr_of_mut!((*w).framing), 1);
                    }
                    if err & hw::ERR_PARITY != 0 {
                        bump(addr_of_mut!((*w).parity), 1);
                    }
                    if err & hw::ERR_BREAK != 0 {
                        bump(addr_of_mut!((*w).breaks_seen), 1);
                    }
                }
                continue;
            }
            if head.wrapping_sub(tail) >= RX_BYTES as u32 {
                unsafe { bump(addr_of_mut!((*witness()).rx_dropped), 1) };
                continue;
            }
            unsafe { write_volatile(ring.add(head as usize % RX_BYTES), b) };
            head = head.wrapping_add(1);
            moved_in = true;
            unsafe { bump(addr_of_mut!((*witness()).bytes_in), 1) };
        }
        if moved_in {
            RX_HEAD[ix].store(head, Ordering::Release);
        }
        let made_room = kick(ix) > 0;
        if HOLDER[ix].load(Ordering::Acquire) == CONSOLE {
            crate::board::ring::account_irq();
            return;
        }
        if moved_in || made_room {
            EVENT[ix].fetch_add(1, Ordering::AcqRel);
            let woken = sched::futex_wake(EVENT[ix].as_ptr(), u32::MAX);
            if woken > 0 {
                unsafe { bump(addr_of_mut!((*witness()).wakes_sent), woken as u32) };
            }
        }
    }

    // ---------------- the console's held ports ----------------

    /// A block held by the kernel's console as a stream's wire (the
    /// console's init; idempotent): the block brought up at the claim's
    /// state through the same backend, the pins the project's or the facts'
    /// (an illegal pair falls back to the facts' and is reported through
    /// the second word). The lines are taken as board claims by the GPIO
    /// module's init, which reads `console_holds`. `hardware` false leaves
    /// the registers to the board module's own console driver (the
    /// emulator rows' polled UART0).
    pub fn console_hold(block: u32, hardware: bool) -> bool {
        let Some(ix) = index_of(block) else {
            return true;
        };
        if HOLDER[ix].load(Ordering::Acquire) != FREE {
            return true;
        }
        let (tx, rx, legal) = pins_for(ix);
        HOLDER[ix].store(CONSOLE, Ordering::Release);
        PIN_TX[ix].store(tx, Ordering::Release);
        PIN_RX[ix].store(rx, Ordering::Release);
        rings_reset(ix);
        if hardware {
            hw::claim(ix, tx, rx);
        }
        legal
    }

    /// Whether the console holds the block.
    pub fn console_holds(block: u32) -> bool {
        matches!(index_of(block), Some(ix) if HOLDER[ix].load(Ordering::Acquire) == CONSOLE)
    }

    /// Whether the console holds any block through the backend.
    #[allow(dead_code)] // the Pico rows' console init reads it
    pub fn console_holds_any() -> bool {
        (0..PORTS).any(|ix| HOLDER[ix].load(Ordering::Acquire) == CONSOLE)
    }

    /// The pins of a console-held block (for the GPIO module's board
    /// claims), `NO_PIN` where none.
    pub fn held_pins(block: u32) -> (u32, u32) {
        match index_of(block) {
            Some(ix) => (PIN_TX[ix].load(Ordering::Acquire), PIN_RX[ix].load(Ordering::Acquire)),
            None => (NO_PIN, NO_PIN),
        }
    }

    /// One byte of a stream to a held block's wire: into the transmit ring
    /// (a full ring drops the byte and counts it -- the mirror has it) and
    /// the transmitter primed; in the polled mode the ring is drained here.
    /// The caller holds the console's line lock.
    pub fn held_putb(block: u32, b: u8) {
        let Some(ix) = index_of(block) else { return };
        if tx_room(ix) == 0 {
            unsafe { bump(addr_of_mut!((*witness()).tx_dropped), 1) };
            crate::board::ring::account_dropped(1);
            return;
        }
        let head = TX_HEAD[ix].load(Ordering::Acquire);
        unsafe { write_volatile(tx_ring(ix).add(head as usize % TX_BYTES), b) };
        TX_HEAD[ix].store(head.wrapping_add(1), Ordering::Release);
        if POLLED.load(Ordering::Acquire) {
            drain_polled(ix);
        } else {
            kick(ix);
        }
    }

    /// The room a held block's transmit ring has (the write door's
    /// back-pressure).
    pub fn held_room(block: u32) -> usize {
        match index_of(block) {
            Some(ix) => tx_room(ix) as usize,
            None => usize::MAX,
        }
    }

    /// One pending received byte of a held block, or None (the console
    /// stream's read door; the caller holds the section).
    pub fn held_getb(block: u32) -> Option<u8> {
        let ix = index_of(block)?;
        if rx_avail(ix) == 0 {
            return None;
        }
        let tail = RX_TAIL[ix].load(Ordering::Acquire);
        let b = unsafe { read_volatile(rx_ring(ix).add(tail as usize % RX_BYTES)) };
        RX_TAIL[ix].store(tail.wrapping_add(1), Ordering::Release);
        Some(b)
    }

    /// The polled mode from here on (the funnel's entry through the
    /// console): every held port's backlog drained now and every later byte
    /// drained synchronously.
    pub fn set_polled() {
        POLLED.store(true, Ordering::Release);
        drain_held();
    }

    /// Every console-held block's transmit ring pushed through the wire
    /// (the exit paths and the polled mode's entry): the report is on the
    /// wire before the machine parks or exits.
    pub fn drain_held() {
        for ix in 0..PORTS {
            if HOLDER[ix].load(Ordering::Acquire) == CONSOLE && hw::is_up(ix) {
                drain_polled(ix);
                while !hw::tx_idle(ix) {}
            }
        }
    }

    /// The loopback bit of a held port's block (the flight's own image; a
    /// part without one answers false).
    #[allow(dead_code)]
    pub fn loopback(h: i32, on: bool) -> bool {
        match held(h) {
            Some(ix) => hw::loopback(ix, on),
            None => false,
        }
    }
}

#[cfg(rhu_uart_rows)]
#[allow(unused_imports)] // the rows use different halves of the set
pub use served::{block_serves_wire, brk, claim, configure, console_hold, console_holds, console_holds_any, drain_held, event_word, flush, held_getb, held_pins, held_putb, held_room, info, irq, loopback, pins_for, read, release, set_polled, status, write};

// ---------------- the rows without a table: every door but the count and the enumeration refuses ----------------

#[cfg(not(rhu_uart_rows))]
mod served {
    pub const CLOCK_HZ: u32 = 0;
    pub fn pins_of(_ix: usize) -> (u32, u32) {
        (super::NO_PIN, super::NO_PIN)
    }
}

#[cfg(not(rhu_uart_rows))]
pub fn claim(block: u32) -> i32 {
    refused(block, sched::ERR_NODEV)
}
#[cfg(not(rhu_uart_rows))]
pub fn configure(h: i32, _baud: u32, _data_bits: u32, _parity: u32, _stop_bits: u32, _achieved: *mut u32) -> i32 {
    refused(h as u32, sched::ERR_NODEV)
}
#[cfg(not(rhu_uart_rows))]
pub fn write(h: i32, _buf: *const u8, _n: u32) -> i32 {
    refused(h as u32, sched::ERR_NODEV)
}
#[cfg(not(rhu_uart_rows))]
pub fn read(h: i32, _buf: *mut u8, _n: u32) -> i32 {
    refused(h as u32, sched::ERR_NODEV)
}
#[cfg(not(rhu_uart_rows))]
pub fn status(h: i32, _a: *mut u32, _p: *mut u32, _r: *mut u32) -> i32 {
    refused(h as u32, sched::ERR_NODEV)
}
#[cfg(not(rhu_uart_rows))]
pub fn event_word(_h: i32) -> *const u32 {
    core::ptr::null()
}
#[cfg(not(rhu_uart_rows))]
pub fn flush(h: i32, _which: u32) -> i32 {
    refused(h as u32, sched::ERR_NODEV)
}
#[cfg(not(rhu_uart_rows))]
pub fn brk(h: i32, _on: u32) -> i32 {
    refused(h as u32, sched::ERR_NODEV)
}
#[cfg(not(rhu_uart_rows))]
pub fn release(h: i32) -> i32 {
    refused(h as u32, sched::ERR_NODEV)
}
#[cfg(not(rhu_uart_rows))]
pub fn info(h: i32, _block: *mut u32, _tx: *mut u32, _rx: *mut u32, _clock_hz: *mut u32) -> i32 {
    refused(h as u32, sched::ERR_NODEV)
}
#[cfg(not(rhu_uart_rows))]
pub fn console_hold(_block: u32, _hardware: bool) -> bool {
    true
}
#[cfg(not(rhu_uart_rows))]
pub fn console_holds(_block: u32) -> bool {
    false
}
#[cfg(not(rhu_uart_rows))]
pub fn console_holds_any() -> bool {
    false
}
#[cfg(not(rhu_uart_rows))]
pub fn held_pins(_block: u32) -> (u32, u32) {
    (NO_PIN, NO_PIN)
}
#[cfg(not(rhu_uart_rows))]
pub fn held_putb(_block: u32, _b: u8) {}
#[cfg(not(rhu_uart_rows))]
pub fn held_room(_block: u32) -> usize {
    usize::MAX
}
#[cfg(not(rhu_uart_rows))]
pub fn held_getb(_block: u32) -> Option<u8> {
    None
}
#[cfg(not(rhu_uart_rows))]
pub fn set_polled() {}
#[cfg(not(rhu_uart_rows))]
pub fn drain_held() {}
#[cfg(not(rhu_uart_rows))]
pub fn irq(_ix: usize) {}
#[cfg(not(rhu_uart_rows))]
pub fn loopback(_h: i32, _on: bool) -> bool {
    false
}
