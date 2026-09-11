//! The entropy doors and their policy. Two doors: `rhu_entropy` is the
//! software stream -- one SplitMix64 stream, well mixed and entirely
//! predictable from its seed -- for everything that does not need
//! secrecy, seeded at its first draw from the board's hardware source
//! where one serves and from the nanosecond clock otherwise;
//! `rhu_entropy_secure` serves bytes for cryptography ONLY from a
//! hardware noise source whose self-tests read clean at that draw, and
//! refuses honestly otherwise: no source on this row or posture, a
//! source that failed its check (retired for the boot's life from that
//! moment: the answer to "is there a hardware source" goes back to no
//! and no more bytes are served), or a source whose bytes were not ready
//! within the door's bound. A build option, named on the command line
//! and implied by no board, lets an image -- the emulator rows' gate
//! images -- accept the software stream for the cryptographic door; the
//! boot banner says so on every image that carries it, and the hardware
//! query still answers no.

use core::ptr::{addr_of, addr_of_mut, read_volatile, write_volatile};
use core::sync::atomic::{AtomicU32, Ordering};

use crate::critical::with_cs;
use crate::{board, sched};

/// The largest secure draw in one call: a larger buffer is filled in
/// bounded pieces by the caller (the header states it). The draw lands
/// in a kernel buffer first and is copied out only when whole, so a
/// refusal writes nothing.
pub const SECURE_MAX_DRAW: u32 = 256;

/// The witness, exported by symbol: nine words.
#[repr(C)]
pub struct EntropyWitness {
    pub source: u32,
    pub state: u32,
    pub draws: u32,
    pub bytes: u32,
    pub refusals: u32,
    pub errors: u32,
    pub resets: u32,
    pub timeouts: u32,
    pub seed: u32,
}

#[unsafe(export_name = "__rhu_entropy_witness")]
pub static mut WITNESS: EntropyWitness = EntropyWitness {
    source: 0,
    state: 0,
    draws: 0,
    bytes: 0,
    refusals: 0,
    errors: 0,
    resets: 0,
    timeouts: 0,
    seed: 0,
};

/// The source's state.
pub const STATE_ABSENT: u32 = 0;
pub const STATE_SERVING: u32 = 1;
pub const STATE_RETIRED: u32 = 2;

/// Which seed the software stream took.
#[allow(dead_code)]
pub const SEED_NONE: u32 = 0;
pub const SEED_CLOCK: u32 = 1;
pub const SEED_HARDWARE: u32 = 2;

/// Consecutive timeouts that retire a healthy-but-silent source (a
/// stopped block is a fault, not a tail).
const TIMEOUTS_TO_RETIRE: u32 = 3;

static mut STREAM: u64 = 0;
static mut BOUNCE: [u8; SECURE_MAX_DRAW as usize] = [0; SECURE_MAX_DRAW as usize];
static DRAW_BUSY: AtomicU32 = AtomicU32::new(0);
static TIMEOUTS_IN_A_ROW: AtomicU32 = AtomicU32::new(0);

fn word(p: *const u32) -> u32 {
    unsafe { read_volatile(p) }
}

fn set_word(p: *mut u32, v: u32) {
    unsafe { write_volatile(p, v) }
}

fn bump(p: *mut u32, by: u32) {
    unsafe { write_volatile(p, read_volatile(p).wrapping_add(by)) }
}

fn w() -> *mut EntropyWitness {
    addr_of_mut!(WITNESS)
}

/// The boot path's call: the board's source started where one serves,
/// the witness's source and state set.
pub fn init() {
    unsafe {
        set_word(addr_of_mut!((*w()).source), board::ENTROPY.id());
        let serving = board::entropy_init();
        set_word(addr_of_mut!((*w()).state), if serving { STATE_SERVING } else { STATE_ABSENT });
    }
}

/// The banner's text after "[boot] entropy: ", constant per row, posture
/// and build option.
pub fn banner() -> &'static str {
    match board::ENTROPY {
        board::EntropyPosture::None => {
            if cfg!(feature = "entropy-accept-software") {
                "no hardware source on this row; the software stream is ACCEPTED for cryptographic draws by the build opt-in entropy-accept-software"
            } else {
                "no hardware source on this row; cryptographic draws are refused, the software stream serves the rest"
            }
        }
        board::EntropyPosture::AbsentByPosture { line, .. } => line,
        board::EntropyPosture::Source { line, .. } => line,
    }
}

pub fn state() -> u32 {
    unsafe { word(addr_of!((*w()).state)) }
}

/// The three words the api flavor's check ties across its draws.
#[allow(dead_code)]
pub fn counts() -> (u32, u32, u32) {
    unsafe {
        (
            word(addr_of!((*w()).draws)),
            word(addr_of!((*w()).errors)),
            word(addr_of!((*w()).refusals)),
        )
    }
}

fn refuse(code: i32) -> i32 {
    with_cs(|| unsafe { bump(addr_of_mut!((*w()).refusals), 1) });
    code
}

/// One hardware draw into the bounce buffer, the witness updated; the
/// board's driver holds the section per block. Returns the door's answer.
fn draw_hardware(n: usize) -> i32 {
    let mut counters = board::DrawCounters { errors: 0, resets: 0 };
    let bounce = unsafe { core::slice::from_raw_parts_mut(addr_of_mut!(BOUNCE) as *mut u8, n) };
    let result = board::entropy_draw(bounce, &mut counters);
    with_cs(|| unsafe {
        bump(addr_of_mut!((*w()).errors), counters.errors);
        bump(addr_of_mut!((*w()).resets), counters.resets);
        match result {
            Ok(()) => {
                bump(addr_of_mut!((*w()).draws), 1);
                bump(addr_of_mut!((*w()).bytes), n as u32);
                TIMEOUTS_IN_A_ROW.store(0, Ordering::Relaxed);
                0
            }
            Err(board::EntropyErr::NoSource) => {
                bump(addr_of_mut!((*w()).refusals), 1);
                sched::ERR_NODEV
            }
            Err(board::EntropyErr::Health) => {
                bump(addr_of_mut!((*w()).refusals), 1);
                set_word(addr_of_mut!((*w()).state), STATE_RETIRED);
                sched::ERR_IO
            }
            Err(board::EntropyErr::Timeout) => {
                bump(addr_of_mut!((*w()).refusals), 1);
                bump(addr_of_mut!((*w()).timeouts), 1);
                let t = TIMEOUTS_IN_A_ROW.fetch_add(1, Ordering::Relaxed) + 1;
                if t >= TIMEOUTS_TO_RETIRE {
                    set_word(addr_of_mut!((*w()).state), STATE_RETIRED);
                }
                sched::ERR_AGAIN
            }
        }
    })
}

/// The cryptographic door's body: under the build option the software
/// stream; otherwise the hardware source through the bounce buffer,
/// copied out only when whole. One draw at a time: a second caller while
/// a draw is in flight is answered "try again" and counted.
pub fn fill_secure(buf: &mut [u8]) -> i32 {
    if cfg!(feature = "entropy-accept-software") {
        fill(buf);
        with_cs(|| unsafe {
            bump(addr_of_mut!((*w()).draws), 1);
            bump(addr_of_mut!((*w()).bytes), buf.len() as u32);
        });
        return 0;
    }
    if DRAW_BUSY.swap(1, Ordering::AcqRel) != 0 {
        return refuse(sched::ERR_AGAIN);
    }
    let r = if state() == STATE_SERVING {
        draw_hardware(buf.len())
    } else {
        refuse(sched::ERR_NODEV)
    };
    if r == 0 {
        let whole = unsafe { core::slice::from_raw_parts(addr_of!(BOUNCE) as *const u8, buf.len()) };
        buf.copy_from_slice(whole);
    }
    DRAW_BUSY.store(0, Ordering::Release);
    r
}

/// The software stream's seed, taken once: eight bytes from the hardware
/// source where it serves, else the nanosecond clock (forced odd so a
/// zero read still seeds). The hardware draw happens outside any
/// section; the seed lands under one, once.
fn seed_if_virgin() {
    if unsafe { read_volatile(addr_of!(STREAM)) } != 0 {
        return;
    }
    if state() == STATE_SERVING && !cfg!(feature = "entropy-accept-software") {
        let mut eight = [0u8; 8];
        if fill_secure(&mut eight) == 0 {
            let v = u64::from_le_bytes(eight) | 1;
            with_cs(|| unsafe {
                if read_volatile(addr_of!(STREAM)) == 0 {
                    write_volatile(addr_of_mut!(STREAM), v);
                    set_word(addr_of_mut!((*w()).seed), SEED_HARDWARE);
                }
            });
            return;
        }
    }
    with_cs(|| unsafe {
        if read_volatile(addr_of!(STREAM)) == 0 {
            write_volatile(addr_of_mut!(STREAM), sched::now_ns() | 1);
            set_word(addr_of_mut!((*w()).seed), SEED_CLOCK);
        }
    });
}

/// One SplitMix64 step under the section: advance, mix.
fn next_u64() -> u64 {
    seed_if_virgin();
    with_cs(|| unsafe {
        let s = &mut *addr_of_mut!(STREAM);
        *s = s.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = *s;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    })
}

/// Fill the whole buffer from the software stream -- all-or-nothing by
/// construction (the argument refusals live at the door).
pub fn fill(buf: &mut [u8]) {
    for chunk in buf.chunks_mut(8) {
        let v = next_u64().to_le_bytes();
        chunk.copy_from_slice(&v[..chunk.len()]);
    }
}

/// The declared-quality query: 1 while a hardware source serves and its
/// last check passed, 0 otherwise -- also 0 under the build option.
pub fn is_hardware() -> i32 {
    (state() == STATE_SERVING) as i32
}
