//! The GPIO doors' bodies: a program claims a line as an input with its
//! pull or as an output, releases it, writes it, reads it, arms an edge on
//! it and takes the line's event word -- the seven verbs `api.rs` exports
//! and `include/rhu.h` states. Beneath them: the claims table (every owner
//! of a line recorded; two owners refused), the board's family driver
//! (`board::gpio_backend`: the registers), the board's facts
//! (`board::facts`: the line count, the board's own claims, the lines a
//! program may not drive). The event path is the futex rule's: every line
//! has a counter that IS a futex word; a detected edge increments it and
//! wakes the word's waiters through the scheduler's critical-section wake
//! path; the program's thread waits in `rhu_futex_wait` on the value it
//! read, so an edge between its read and its wait returns at once and
//! nothing is lost. No blocking verb, no callback into the program.

use core::ptr::{addr_of, addr_of_mut, read_volatile, write_volatile};
use core::sync::atomic::{AtomicU32, AtomicU8, Ordering};

use crate::board::{facts, gpio_backend as backend};
use crate::claims;
use crate::critical::with_cs;
use crate::gpio_math as m;
use crate::sched;

/// The codes this module adds to the hosting API's set (`include/rhu.h`).
pub const ERR_BUSY: i32 = -16;
pub const ERR_ACCES: i32 = -13;
pub const ERR_NOTSUP: i32 = -95;

const N: usize = facts::LINE_COUNT;

/// Mode per line: 0 unclaimed, else the claim's mode + 1.
static MODE: [AtomicU8; N] = [const { AtomicU8::new(0) }; N];
/// The armed edge kind per line (0 none).
static EDGE: [AtomicU8; N] = [const { AtomicU8::new(0) }; N];
/// The event counters: each one a futex word.
static EVENTS: [AtomicU32; N] = [const { AtomicU32::new(0) }; N];

/// The claims record, by symbol.
#[unsafe(export_name = "__rhu_claims")]
pub static mut CLAIMS: claims::Claims = claims::EMPTY;

/// The witness, by symbol.
#[repr(C)]
pub struct GpioWitness {
    pub lines: u32,
    pub claims_taken: u32,
    pub reconfigured: u32,
    pub refused_busy: u32,
    pub refused_inval: u32,
    pub refused_acces: u32,
    pub refused_notsup: u32,
    pub edges_armed: u32,
    pub edges_counted: u32,
    pub wakes_sent: u32,
    pub last_refused_line: u32,
    pub last_refused_code: i32,
}

#[unsafe(export_name = "__rhu_gpio_witness")]
pub static mut WITNESS: GpioWitness = GpioWitness {
    lines: 0,
    claims_taken: 0,
    reconfigured: 0,
    refused_busy: 0,
    refused_inval: 0,
    refused_acces: 0,
    refused_notsup: 0,
    edges_armed: 0,
    edges_counted: 0,
    wakes_sent: 0,
    last_refused_line: 0,
    last_refused_code: 0,
};

/// The witness's fields are written through raw pointers, one field at a
/// time, under the critical section by the doors and by the handlers; no
/// reference to the static is ever formed, so a reader's snapshot by value
/// (the api flavor's checks; the probe over the debug port) aliases nothing.
fn witness() -> *mut GpioWitness {
    addr_of_mut!(WITNESS)
}

/// One field bumped by `by` through its raw pointer: a volatile read and
/// write, under the caller's section.
unsafe fn bump(field: *mut u32, by: u32) {
    unsafe { write_volatile(field, read_volatile(field).wrapping_add(by)) }
}

fn refused(line: u32, code: i32) -> i32 {
    with_cs(|| unsafe {
        let w = witness();
        match code {
            ERR_BUSY => bump(addr_of_mut!((*w).refused_busy), 1),
            ERR_ACCES => bump(addr_of_mut!((*w).refused_acces), 1),
            ERR_NOTSUP => bump(addr_of_mut!((*w).refused_notsup), 1),
            _ => bump(addr_of_mut!((*w).refused_inval), 1),
        }
        write_volatile(addr_of_mut!((*w).last_refused_line), line);
        write_volatile(addr_of_mut!((*w).last_refused_code), code);
    });
    code
}

/// The board's claims fixed at boot; the counters zeroed.
pub fn init() {
    with_cs(|| unsafe {
        claims::init(&mut *addr_of_mut!(CLAIMS), N as u32, facts::BOARD_CLAIM_BITS);
        write_volatile(addr_of_mut!((*witness()).lines), N as u32);
    });
}

/// The banner's text after "[boot] gpio: ", constant per image.
pub fn banner() -> &'static str {
    facts::BANNER
}

/// The line SPACE (the highest line plus one), the library's range bound;
/// an unpopulated line inside it is refused at the claim.
pub fn count() -> i32 {
    N as i32
}

/// A claim: the line inside the space and populated, the mode and pull
/// and initial level valid, the input-only rule honoured, then the table
/// -- or, when THIS program already holds the line, a RE-CONFIGURATION
/// (idempotent: the library sets a pin's mode on an open pin and again to
/// change it, and a release-then-reclaim would leave a moment with no
/// owner). An output claim carries its initial level,
/// written to the data register BEFORE the mode register so the pad never
/// shows the wrong level at open. All-or-nothing: on a refusal
/// nothing is programmed.
pub fn claim(line: u32, mode: u32, pull: u32, initial: u32) -> i32 {
    if line as usize >= N || !m::mode_valid(mode) || !m::pull_valid(pull) || initial > 1 {
        return refused(line, sched::ERR_INVAL);
    }
    if !facts::populated(line) {
        return refused(line, sched::ERR_INVAL);
    }
    if mode != m::MODE_INPUT && facts::input_only(line) {
        return refused(line, ERR_ACCES);
    }
    let held = with_cs(|| unsafe { claims::is_program(&*addr_of!(CLAIMS), line) });
    if !held {
        let r = with_cs(|| unsafe { claims::claim(&mut *addr_of_mut!(CLAIMS), line) });
        if r != 0 {
            return refused(line, r);
        }
    } else if EDGE[line as usize].load(Ordering::Acquire) != 0 && mode != m::MODE_INPUT {
        backend::disarm(line);
        EDGE[line as usize].store(0, Ordering::Release);
    }
    backend::configure(line, mode, pull, initial != 0);
    MODE[line as usize].store(mode as u8 + 1, Ordering::Release);
    with_cs(|| unsafe {
        let w = witness();
        if held {
            bump(addr_of_mut!((*w).reconfigured), 1);
        } else {
            bump(addr_of_mut!((*w).claims_taken), 1);
        }
    });
    0
}

pub fn release(line: u32) -> i32 {
    if line as usize >= N || MODE[line as usize].load(Ordering::Acquire) == 0 {
        return refused(line, sched::ERR_INVAL);
    }
    if EDGE[line as usize].load(Ordering::Acquire) != 0 {
        backend::disarm(line);
        EDGE[line as usize].store(0, Ordering::Release);
    }
    backend::release(line);
    MODE[line as usize].store(0, Ordering::Release);
    with_cs(|| unsafe { claims::release(&mut *addr_of_mut!(CLAIMS), line) })
}

pub fn write(line: u32, value: u32) -> i32 {
    if line as usize >= N {
        return refused(line, sched::ERR_INVAL);
    }
    match MODE[line as usize].load(Ordering::Acquire) {
        0 => refused(line, sched::ERR_INVAL),
        1 => refused(line, ERR_ACCES),
        mode => {
            backend::write(line, mode as u32 - 1, value != 0);
            0
        }
    }
}

pub fn read(line: u32) -> i32 {
    if line as usize >= N || MODE[line as usize].load(Ordering::Acquire) == 0 {
        return refused(line, sched::ERR_INVAL);
    }
    backend::read(line) as i32
}

pub fn edge(line: u32, kind: u32) -> i32 {
    if line as usize >= N || MODE[line as usize].load(Ordering::Acquire) == 0 {
        return refused(line, sched::ERR_INVAL);
    }
    match m::kind_class(kind) {
        m::KindClass::Invalid => refused(line, sched::ERR_INVAL),
        m::KindClass::Unsupported => refused(line, ERR_NOTSUP),
        m::KindClass::None => {
            if EDGE[line as usize].load(Ordering::Acquire) != 0 {
                backend::disarm(line);
                EDGE[line as usize].store(0, Ordering::Release);
            }
            0
        }
        m::KindClass::Edge { rising, falling } => {
            if !backend::HAS_EVENTS {
                return refused(line, ERR_NOTSUP);
            }
            EDGE[line as usize].store(kind as u8, Ordering::Release);
            let r = backend::arm(line, rising, falling);
            if r != 0 {
                EDGE[line as usize].store(0, Ordering::Release);
                return refused(line, r);
            }
            with_cs(|| unsafe { bump(addr_of_mut!((*witness()).edges_armed), 1) });
            0
        }
    }
}

pub fn event_word(line: u32) -> *const u32 {
    if line as usize >= N || MODE[line as usize].load(Ordering::Acquire) == 0 {
        return core::ptr::null();
    }
    EVENTS[line as usize].as_ptr()
}

/// The handlers' half: an edge detected on `line` (by the family handler in
/// the board band, or by the virtual backend in thread context), filtered
/// by the armed kind, counted, and the word's waiters woken through the
/// scheduler's critical-section path -- every wake path pending explicitly.
pub fn fire(line: u32, rising: bool) {
    if line as usize >= N {
        return;
    }
    let kind = EDGE[line as usize].load(Ordering::Acquire) as u32;
    let served = match m::kind_class(kind) {
        m::KindClass::Edge { rising: r, falling: f } => (rising && r) || (!rising && f),
        _ => false,
    };
    if !served {
        return;
    }
    EVENTS[line as usize].fetch_add(1, Ordering::AcqRel);
    let woken = sched::futex_wake(EVENTS[line as usize].as_ptr(), u32::MAX);
    with_cs(|| unsafe {
        let w = witness();
        bump(addr_of_mut!((*w).edges_counted), 1);
        if woken > 0 {
            bump(addr_of_mut!((*w).wakes_sent), woken as u32);
        }
    });
}

/// The silicon check's software-triggered edge on a claimed, armed line
/// (the family's software-interrupt register); false where the backend
/// offers none (the virtual backend, which fires its loop instead).
#[allow(dead_code)]
pub fn software_edge(line: u32) -> bool {
    if line as usize >= N || EDGE[line as usize].load(Ordering::Acquire) == 0 {
        return false;
    }
    backend::software_edge(line)
}
