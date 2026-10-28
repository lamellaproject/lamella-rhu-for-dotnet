//! The arena: the GC's memory substrate. One arena per image over up to
//! TWO WINDOWS: window 0 is the internal SRAM's link-defined slack
//! every board has, the window every dynamic consumer draws from
//! (the kheap carve, TLS blocks, GC aux structures, the
//! FOH pool, GC regions) through `alloc_at`; window
//! 1 is an external RAM a board brings up and proves at boot (the
//! Pimoroni rows' PSRAM), offered to the hosted runtime through the
//! RAM-range doors (`ram.rs`) and drawn from by name through
//! `alloc_from`. Each window is bump-only at this level: individual frees
//! never exist, and **rezero** is the recycling story -- the decommit
//! contract on a flat address space.

use core::ptr;
#[cfg(feature = "api-smoke")]
use core::sync::atomic::{AtomicU32, Ordering};

use crate::critical::with_cs;
use crate::sched;

/// Fixed audit capacity: enough for the kernel's own registered callers
/// plus the hosted consumers' call sites; growth past it lands in the
/// loud overflow row, never a silent drop.
const AUDIT_ROWS: usize = 16;

/// The windows an image can carry: the internal SRAM and one external RAM.
pub const WINDOWS: usize = 2;

/// One per-site audit row. A row is occupied iff `count != 0`, so site
/// value 0 stays a legal (if odd) caller key.
#[derive(Clone, Copy)]
struct AuditRow {
    site: u32,
    count: u32,
    bytes: u32,
}

/// One window: its brackets and its next hand-out address.
#[derive(Clone, Copy)]
struct Window {
    /// Next hand-out address; 8-aligned always (sizes round up to 8),
    /// so `hi - bump` is exactly allocatable headroom.
    bump: usize,
    lo: usize,
    hi: usize,
    /// The init frontier -- `[lo, clean)` reads zero and the arena
    /// knows it; `clean >= bump` always (a hand-out that reaches above it
    /// moves it to the hand-out's end; the idle step advances it). A
    /// window zeroed at boot carries `clean == hi` from `take`.
    #[cfg(any(rhu_ram_init_first_use, rhu_ram_init_background))]
    clean: usize,
    /// The piece in flight, `[piece_lo, piece_hi)` -- the span the
    /// idle step is zeroing OUTSIDE the critical section; 0/0 when none.
    /// `piece_lo == clean` while a piece is in flight (the step publishes
    /// it at the frontier; whatever moves the frontier past it clears it).
    #[cfg(rhu_ram_init_background)]
    piece_lo: usize,
    #[cfg(rhu_ram_init_background)]
    piece_hi: usize,
    /// The window's generation -- bumped by every takeover of the
    /// piece in flight (a hand-out whose range reaches it) and by the api
    /// smoke's frontier rewind; the step's completion compares it against
    /// the copy it took when it published the piece, and the zeroing loop
    /// re-reads it between every exclusive pair.
    #[cfg(rhu_ram_init_background)]
    generation: u32,
}

const EMPTY: Window = Window {
    bump: 0,
    lo: 0,
    hi: 0,
    #[cfg(any(rhu_ram_init_first_use, rhu_ram_init_background))]
    clean: 0,
    #[cfg(rhu_ram_init_background)]
    piece_lo: 0,
    #[cfg(rhu_ram_init_background)]
    piece_hi: 0,
    #[cfg(rhu_ram_init_background)]
    generation: 0,
};

/// The init mode's words for the boot banner's ram line on a row
/// with an external window; the rows
/// with one range print nothing of the mode, so the words exist where the
/// line does.
#[cfg(rhu_extram_window)]
pub const INIT_MODE_WORDS: &str = if cfg!(rhu_ram_init_first_use) {
    "init at first use"
} else if cfg!(rhu_ram_init_background) {
    "init in background"
} else {
    "init at boot"
};

const _: () = assert!(!(cfg!(rhu_ram_init_first_use) && cfg!(rhu_ram_init_background)), "one init mode at a time (build.rs refuses both)");

#[cfg(all(feature = "test-ram-init-window0", not(feature = "api-smoke")))]
compile_error!("test-ram-init-window0 is the api flavor's gate feature: select api-smoke beside it");

#[cfg(all(feature = "test-ram-init-doctored-section", not(feature = "api-smoke")))]
compile_error!("test-ram-init-doctored-section is the api flavor's control feature: select api-smoke beside it");

/// Whether window `index` follows the image's lazy init mode -- the
/// external window (index 1) always; window 0 under the api gate's test
/// feature alone.
#[cfg(any(rhu_ram_init_first_use, rhu_ram_init_background))]
const fn lazy_window(index: usize) -> bool {
    index >= 1 || cfg!(feature = "test-ram-init-window0")
}

/// The piece one idle step zeroes OUTSIDE the critical section. The
/// section does not bound it (nothing is held while a piece is
/// written); the piece bounds the bookkeeping rate (one section entry and
/// exit per piece) and the span a takeover re-dirties (the part of the
/// piece above the taker's end, which the next step returns to). 1 KB on
/// every row, one constant.
#[cfg(rhu_ram_init_background)]
const RAM_INIT_PIECE: usize = 1024;

/// The background mode's witness (the api flavor only): the word the step holds at 1
/// while a piece is zeroing outside the section and at 0 otherwise,
/// sampled by the TICK handler (sched.rs); the ticks served while it read
/// 1 are the proof that the kernel band runs mid-piece -- the zeroing is
/// not under the section. The control `test-ram-init-doctored-section`
/// sets the word inside the section around a memset put back there, so
/// the tick, masked throughout, never samples it and the count stays 0.
#[cfg(all(feature = "api-smoke", rhu_ram_init_background))]
static RAM_INIT_IN_FLIGHT: AtomicU32 = AtomicU32::new(0);
#[cfg(all(feature = "api-smoke", rhu_ram_init_background))]
static RAM_INIT_TICKS_MID_PIECE: AtomicU32 = AtomicU32::new(0);
/// The takeovers (the generation bumps by a hand-out), the `strex` retries
/// and the pieces abandoned, for the api walk's record.
#[cfg(all(feature = "api-smoke", rhu_ram_init_background))]
static RAM_INIT_TAKEOVERS: AtomicU32 = AtomicU32::new(0);
#[cfg(all(feature = "api-smoke", rhu_ram_init_background))]
static RAM_INIT_RETRIES: AtomicU32 = AtomicU32::new(0);
#[cfg(all(feature = "api-smoke", rhu_ram_init_background))]
static RAM_INIT_ABANDONED: AtomicU32 = AtomicU32::new(0);

struct Arena {
    windows: [Window; WINDOWS],
    /// The windows established at boot: 1 on every row, 2 where an
    /// external RAM was brought up and proved.
    count: usize,
    audit: [AuditRow; AUDIT_ROWS],
    overflow_allocs: u32,
    overflow_bytes: u32,
}

static mut ARENA: Arena = Arena {
    windows: [EMPTY; WINDOWS],
    count: 0,
    audit: [AuditRow { site: 0, count: 0, bytes: 0 }; AUDIT_ROWS],
    overflow_allocs: 0,
    overflow_bytes: 0,
};

#[allow(clippy::mut_from_ref)]
unsafe fn arena() -> &'static mut Arena { unsafe {
    &mut *core::ptr::addr_of_mut!(ARENA)
}}

/// Registered site keys for kernel-internal Rust callers (C callers get
/// their return address free via the `rhu_arena_alloc` header macro).
/// Small named integers, distinct from any function address by
/// construction. The kheap carve is the only
/// kernel-internal arena caller (TLS draws from the kheap, not the
/// arena).
pub mod sites {
    /// The one boot-time kheap carve (kheap::init).
    pub const KHEAP_CARVE: u32 = 1;
}

/// Take one window: the api-smoke poison first, then the shipped memset
/// that establishes the zero-birth invariant over it.
fn take(index: usize, lo: usize, hi: usize) {
    // The api-smoke poison: fill BEFORE the shipped memset, so the
    // zero-birth check proves the memset did the zeroing -- pristine
    // emulator RAM (or a part the proof just zeroed) cannot satisfy it
    // vacuously.
    #[cfg(feature = "api-smoke")]
    unsafe {
        ptr::write_bytes(lo as *mut u8, 0xA5, hi - lo);
    }
    unsafe {
        // A lazy window keeps its contents (the poison under
        // api-smoke; whatever the boot's proof left otherwise) and starts
        // its frontier at `lo`; every other window is zeroed whole here,
        // its frontier `hi`.
        #[cfg(any(rhu_ram_init_first_use, rhu_ram_init_background))]
        let clean = if lazy_window(index) {
            lo
        } else {
            ptr::write_bytes(lo as *mut u8, 0, hi - lo);
            hi
        };
        #[cfg(not(any(rhu_ram_init_first_use, rhu_ram_init_background)))]
        ptr::write_bytes(lo as *mut u8, 0, hi - lo);
        let a = arena();
        a.windows[index] = Window {
            bump: lo,
            lo,
            hi,
            #[cfg(any(rhu_ram_init_first_use, rhu_ram_init_background))]
            clean,
            #[cfg(rhu_ram_init_background)]
            piece_lo: 0,
            #[cfg(rhu_ram_init_background)]
            piece_hi: 0,
            #[cfg(rhu_ram_init_background)]
            generation: 0,
        };
        a.count = index + 1;
    }
}

/// Establish the windows and the zero-birth invariant over each. Called
/// from `kmain` immediately before `sched::init()`: single-threaded,
/// before any thread exists, and deliberately silent on the console.
/// Window 0 on every row; window 1 on a row whose external RAM was
/// brought up and proved earlier in the boot (the rows' maps bracket it;
/// the boot dies before this point when the part refuses).
pub fn init() {
    let (lo, hi) = crate::board::arena_window();
    take(0, lo, hi);
    #[cfg(rhu_extram_window)]
    {
        let (lo1, hi1) = crate::board::arena_window1();
        take(1, lo1, hi1);
    }
}

/// The windows established at boot (1 or 2).
pub fn count() -> usize {
    with_cs(|| unsafe { arena().count })
}

/// A window's brackets and its headroom, for the RAM-range doors: `None`
/// past the count.
pub fn window(index: usize) -> Option<(usize, usize, u32)> {
    with_cs(|| unsafe {
        let a = arena();
        if index >= a.count {
            return None;
        }
        let w = a.windows[index];
        Some((w.lo, w.hi, (w.hi - w.bump) as u32))
    })
}

fn audit_add(a: &mut Arena, site: u32, consumed: usize) {
    let consumed = consumed as u32;
    for row in a.audit.iter_mut() {
        if row.count != 0 && row.site == site {
            row.count += 1;
            row.bytes = row.bytes.saturating_add(consumed);
            return;
        }
    }
    for row in a.audit.iter_mut() {
        if row.count == 0 {
            *row = AuditRow { site, count: 1, bytes: consumed };
            return;
        }
    }
    a.overflow_allocs += 1;
    a.overflow_bytes = a.overflow_bytes.saturating_add(consumed);
}

/// Aligned bump allocation from window 0 (the internal SRAM) attributed
/// to `site`. NULL on exhaustion only; wild arguments die loud (see the
/// module header).
pub fn alloc_at(bytes: u32, align: u32, site: u32) -> *mut u8 {
    alloc_from(0, bytes, align, site)
}

/// Aligned bump allocation from the named window attributed to `site`.
/// NULL on that window's exhaustion only; wild arguments -- the align,
/// the size, or a window index past the count -- die loud.
pub fn alloc_from(range: u32, bytes: u32, align: u32, site: u32) -> *mut u8 {
    assert!(align.is_power_of_two(), "arena: wild align {} (caller bug)", align);
    assert!(bytes != 0, "arena: zero-byte alloc (caller bug)");
    // The api flavor's guard on the fact the collision rule rests on:
    // hand-outs are thread calls only, never a handler's (a takeover of the
    // idle step's piece needs the taker's thread to run, so it needs the
    // context switch that clears the exclusive monitor). IPSR 0 is thread
    // mode; anything else names the exception.
    #[cfg(feature = "api-smoke")]
    {
        let ipsr: u32;
        unsafe { core::arch::asm!("mrs {}, IPSR", out(reg) ipsr, options(nomem, nostack, preserves_flags)) };
        assert!(ipsr == 0, "arena: alloc_from reached in handler mode (exception {}; caller bug)", ipsr);
    }
    // The hand-out leaves the critical section with the span it must
    // still zero -- `[max(base, clean), end)` of a lazy window, nothing of
    // an eager one -- and zeroes it out here, where the tick is never
    // masked by it. From the moment `bump` moved the span is this caller's
    // alone (bump-only: no other hand-out can reach it; the idle step never
    // enters below `clean`; a `rezero` from another thread over a span that
    // is not its own is a caller bug), so the memset needs no
    // lock. The door returns only after the span reads zero.
    let (p, dirty) = with_cs(|| -> (*mut u8, Option<(usize, usize)>) { unsafe {
        let a = arena();
        assert!((range as usize) < a.count, "arena: range {} past the {} this image serves (caller bug)", range, a.count);
        let w = &mut a.windows[range as usize];
        assert!(align as usize <= w.hi - w.lo, "arena: wild align {} over the window's {} bytes (caller bug)", align, w.hi - w.lo);
        let mask = align as usize - 1;
        let base = match w.bump.checked_add(mask) {
            Some(v) => v & !mask,
            None => return (ptr::null_mut(), None),
        };
        let size = match (bytes as usize).checked_add(7) {
            Some(v) => v & !7,
            None => return (ptr::null_mut(), None),
        };
        match base.checked_add(size) {
            Some(end) if end <= w.hi => {
                w.bump = end;
                // The frontier jumps to the hand-out's end when the span
                // reaches above it: the part above the old frontier is the
                // caller's to zero (the first-use mode's every hand-out; the
                // background mode's fall-back), and the idle step is told
                // the span is spoken for.
                #[cfg(any(rhu_ram_init_first_use, rhu_ram_init_background))]
                let dirty = if w.clean < end {
                    let from = if base > w.clean { base } else { w.clean };
                    w.clean = end;
                    Some((from, end))
                } else {
                    None
                };
                #[cfg(not(any(rhu_ram_init_first_use, rhu_ram_init_background)))]
                let dirty = None;
                // A range that reaches the piece the idle step is
                // zeroing outside the section takes it over -- the step's
                // claim is cancelled by the generation and the piece
                // cleared; the taker zeroes [dirty_lo, end) itself below,
                // the piece's overlap included, and the step's next piece
                // starts at the frontier the taker left. One compare and
                // one increment on the hand-out path.
                #[cfg(rhu_ram_init_background)]
                if w.piece_hi != 0 && end > w.piece_lo {
                    w.generation = w.generation.wrapping_add(1);
                    w.piece_lo = 0;
                    w.piece_hi = 0;
                    #[cfg(feature = "api-smoke")]
                    RAM_INIT_TAKEOVERS.fetch_add(1, Ordering::Relaxed);
                }
                audit_add(a, site, size);
                (base as *mut u8, dirty)
            }
            _ => (ptr::null_mut(), None), // honest exhaustion: state untouched
        }
    }});
    if let Some((from, to)) = dirty {
        // The doctored control (never a shipped image): the zeroing skipped,
        // so the zero-birth check reads the boot poison and refuses.
        #[cfg(not(feature = "test-ram-init-doctored"))]
        unsafe {
            ptr::write_bytes(from as *mut u8, 0, to - from);
        }
        #[cfg(feature = "test-ram-init-doctored")]
        let _ = (from, to);
    }
    p
}

/// The background init mode (the bookkeeping inside the critical section,
/// the writing outside it).
/// One idle step zeroes the next piece of the first unfinished window:
/// INSIDE the section it picks `[clean, clean + RAM_INIT_PIECE)` clipped
/// to `hi`, publishes it as the piece in flight and copies the window's
/// generation; OUTSIDE it zeroes the piece word by word under the
/// exclusive-monitor rule (`zero_piece`); INSIDE again it advances the
/// frontier to the piece's end only if the generation is unchanged -- a
/// hand-out that reached the piece bumped it, zeroed its own range and
/// left the frontier at its end, so then nothing moves and the next step
/// starts there. Answers whether work remains; the idle thread calls it in
/// place of its `wfi` until it answers false. No interrupt band is held
/// for the zeroing: the two bookkeeping entries (a few dozen cycles each)
/// are the mode's only masked spans, and the idle thread is preemptible
/// by every band at every word.
#[cfg(all(rhu_ram_init_background, not(feature = "test-ram-init-doctored-section")))]
pub fn ram_init_step() -> bool {
    // Inside: the claim -- the piece published at the frontier, the
    // generation copied, the generation's address taken (the loop
    // re-reads it; the window lives in the static arena for ever).
    let claim = with_cs(|| unsafe {
        let a = arena();
        for (i, w) in a.windows[..a.count].iter_mut().enumerate() {
            if w.clean >= w.hi {
                continue;
            }
            let lo = w.clean;
            let hi = if w.hi - lo > RAM_INIT_PIECE { lo + RAM_INIT_PIECE } else { w.hi };
            w.piece_lo = lo;
            w.piece_hi = hi;
            return Some((i, lo, hi, w.generation, ptr::addr_of!(w.generation) as *const u32));
        }
        None
    });
    let Some((index, lo, hi, generation, genp)) = claim else {
        return false;
    };
    // Outside: the zeroing under the rule, the kernel band live throughout.
    #[cfg(feature = "api-smoke")]
    RAM_INIT_IN_FLIGHT.store(1, Ordering::Relaxed);
    let (reached, retries) = unsafe { zero_piece(lo, hi, genp, generation) };
    #[cfg(feature = "api-smoke")]
    {
        RAM_INIT_IN_FLIGHT.store(0, Ordering::Relaxed);
        RAM_INIT_RETRIES.fetch_add(retries, Ordering::Relaxed);
        if reached < hi {
            RAM_INIT_ABANDONED.fetch_add(1, Ordering::Relaxed);
        }
    }
    #[cfg(not(feature = "api-smoke"))]
    let _ = retries;
    // Inside: the completion -- the frontier moves to the piece's end only
    // under the generation the claim saw; otherwise the piece was taken
    // over (its taker cleared it and moved the frontier) and nothing moves.
    with_cs(|| unsafe {
        let a = arena();
        let w = &mut a.windows[index];
        if w.generation == generation {
            debug_assert!(reached == hi && w.clean == lo, "ram-init: the piece's claim held but the frontier moved (kernel bug)");
            w.clean = hi;
            w.piece_lo = 0;
            w.piece_hi = 0;
        }
        a.windows[..a.count].iter().any(|w| w.clean < w.hi)
    })
}

/// The background mode's rule: zero `[p, end)` word by word
/// as an exclusive pair, abandoning the piece the moment the window's
/// generation moves. ONE inline-assembly block: the
/// `ldrex` on the word, the generation's load, the compare and the branch
/// out, the `strex` of zero and the retry branch stand together, so the
/// compiler can neither hoist the generation's read out of the retry nor
/// split the pair. The local exclusive monitor is cleared at every context
/// switch (`clrex` in PendSV's path, sched.rs), so a `strex` that follows
/// a preemption FAILS and the loop re-reads the generation; a takeover
/// needs the taker's thread to run, so it needs a switch, so the re-read
/// sees the bumped generation and the loop leaves (its own `clrex` first)
/// -- no store of the step lands after a takeover. A failed `strex` costs
/// one re-read and one retry. Returns the address the loop reached (`end`
/// when the piece is whole) and the retries. `p < end`, both word-aligned
/// (the windows' brackets and the piece are 8-aligned).
#[cfg(all(rhu_ram_init_background, not(feature = "test-ram-init-doctored-section")))]
unsafe fn zero_piece(p: usize, end: usize, genp: *const u32, generation: u32) -> (usize, u32) {
    debug_assert!(p < end && p & 3 == 0 && end & 3 == 0, "ram-init: a piece that is not whole words (kernel bug)");
    let mut at = p;
    let mut retries: u32 = 0;
    unsafe {
        core::arch::asm!(
            "2:",
            "ldrex {t}, [{at}]",
            "ldr {g}, [{gp}]",
            "cmp {g}, {generation}",
            "bne 4f",
            "strex {t}, {z}, [{at}]",
            "cmp {t}, #0",
            "bne 3f",
            "adds {at}, {at}, #4",
            "cmp {at}, {end}",
            "blo 2b",
            "b 5f",
            "3:",
            "adds {r}, {r}, #1",
            "b 2b",
            "4:",
            "clrex",
            "5:",
            at = inout(reg) at,
            r = inout(reg) retries,
            end = in(reg) end,
            gp = in(reg) genp,
            generation = in(reg) generation,
            z = in(reg) 0u32,
            t = out(reg) _,
            g = out(reg) _,
            options(nostack),
        );
    }
    (at, retries)
}

/// THE DOCTORED CONTROL of the background mode (`test-ram-init-doctored-section`; the api
/// flavor only, never a shipped image): the idle step with
/// the piece's memset INSIDE the critical section, with the
/// witness word set around it, so the tick, masked throughout, never
/// samples a piece in flight and the frontier check's witness count reads
/// 0: the check must refuse with its words. The piece and generation
/// fields are kept written and read so the control's image carries the
/// shipped hand-out unchanged.
#[cfg(all(rhu_ram_init_background, feature = "test-ram-init-doctored-section"))]
pub fn ram_init_step() -> bool {
    with_cs(|| unsafe {
        let a = arena();
        let mut remaining = false;
        let mut stepped = false;
        for w in a.windows[..a.count].iter_mut() {
            if w.clean >= w.hi {
                continue;
            }
            if !stepped {
                let end = if w.hi - w.clean > RAM_INIT_PIECE { w.clean + RAM_INIT_PIECE } else { w.hi };
                let _ = (w.piece_lo, w.piece_hi, w.generation);
                RAM_INIT_IN_FLIGHT.store(1, Ordering::Relaxed);
                ptr::write_bytes(w.clean as *mut u8, 0, end - w.clean);
                RAM_INIT_IN_FLIGHT.store(0, Ordering::Relaxed);
                w.clean = end;
                stepped = true;
            }
            if w.clean < w.hi {
                remaining = true;
            }
        }
        remaining
    })
}

/// The background mode's witness sample, the TICK handler's (sched.rs; the api flavor's
/// background image only): a tick served while a piece is in flight is a
/// tick served while the step is writing outside the section.
#[cfg(all(feature = "api-smoke", rhu_ram_init_background))]
pub fn ram_init_tick_sample() {
    if RAM_INIT_IN_FLIGHT.load(Ordering::Relaxed) != 0 {
        RAM_INIT_TICKS_MID_PIECE.fetch_add(1, Ordering::Relaxed);
    }
}

/// The background mode's record for the api smoke ONLY: (the ticks served mid-piece since
/// boot, the takeovers, the `strex` retries, the pieces abandoned).
#[cfg(all(feature = "api-smoke", rhu_ram_init_background))]
pub fn probe_ram_init_witness() -> (u32, u32, u32, u32) {
    (
        RAM_INIT_TICKS_MID_PIECE.load(Ordering::Relaxed),
        RAM_INIT_TAKEOVERS.load(Ordering::Relaxed),
        RAM_INIT_RETRIES.load(Ordering::Relaxed),
        RAM_INIT_ABANDONED.load(Ordering::Relaxed),
    )
}

/// Re-establish the zero-birth invariant over `[p, p+bytes)`. The range
/// must lie inside HANDED-OUT arena space of one window (`[lo, bump)`);
/// 0 on success, `ERR_INVAL` otherwise with nothing touched. Headroom
/// and the next hand-out address are untouched either way -- that is the
/// "zero VA burned" contract the gate checks.
///
/// The arena's state is read under the critical section and the
/// memset runs OUTSIDE it, with the kernel band live. A section that
/// held for the whole memset at any length (a 1 MB range on the slowest
/// external memory would mask the tick, the console and every other
/// kernel-band handler for about 113 ms) would protect nothing the
/// memset needs: a handed-out range is its holder's alone (hand-outs are
/// bump-only and never reach below `bump`; the idle step never enters below
/// `clean`, which is at or above `bump`; a rezero from another thread over
/// a span that is not its own is a caller bug), and a range
/// once inside a window's handed-out space stays inside it (windows never
/// shrink, `bump` never retreats). The hand-out path zeroes its own span
/// outside the section by the same argument (`alloc_from`).
pub fn rezero(p: *mut u8, bytes: u32) -> i32 {
    let start = p as usize;
    let end = match start.checked_add(bytes as usize) {
        Some(e) => e,
        None => return sched::ERR_INVAL,
    };
    let inside = with_cs(|| unsafe {
        let a = arena();
        a.windows[..a.count].iter().any(|w| start >= w.lo && end <= w.bump)
    });
    if !inside {
        return sched::ERR_INVAL;
    }
    // The api flavor's band control samples the in-flight word from its
    // handler: a hit while the word reads 1 is a kernel-band interrupt
    // served while the memset ran.
    #[cfg(not(feature = "test-rezero-doctored"))]
    {
        #[cfg(feature = "api-smoke")]
        REZERO_IN_FLIGHT.store(1, Ordering::Release);
        unsafe { ptr::write_bytes(p, 0, bytes as usize) };
        #[cfg(feature = "api-smoke")]
        REZERO_IN_FLIGHT.store(0, Ordering::Release);
    }
    #[cfg(feature = "test-rezero-doctored")]
    with_cs(|| {
        #[cfg(feature = "api-smoke")]
        REZERO_IN_FLIGHT.store(1, Ordering::Release);
        unsafe { ptr::write_bytes(p, 0, bytes as usize) };
        #[cfg(feature = "api-smoke")]
        REZERO_IN_FLIGHT.store(0, Ordering::Release);
    });
    0
}

#[cfg(all(feature = "test-rezero-doctored", not(feature = "api-smoke")))]
compile_error!("test-rezero-doctored is the api flavor's control feature: select api-smoke beside it");

/// The rezero's control word: 1 while a rezero's memset is in flight (the api
/// flavor alone; the band control's handler samples it).
#[cfg(feature = "api-smoke")]
static REZERO_IN_FLIGHT: AtomicU32 = AtomicU32::new(0);

/// Whether a rezero's memset is in flight (the api flavor's band control).
#[cfg(feature = "api-smoke")]
pub fn rezero_in_flight() -> u32 {
    REZERO_IN_FLIGHT.load(Ordering::Acquire)
}

/// Bytes an immediately following 8-aligned allocation from window 0
/// could still get, exactly (`hi - bump`; both stay 8-aligned). 0 means
/// exhausted.
pub fn headroom() -> u32 {
    with_cs(|| unsafe {
        let w = arena().windows[0];
        (w.hi - w.bump) as u32
    })
}

/// Dump the audit table. Diagnostic: noisy by design, never called by
/// a shipped image (site values are image-layout facts, so
/// this output must never join a golden or a parity stream).
pub fn report() {
    let (rows, of_allocs, of_bytes, windows, count) = with_cs(|| unsafe {
        let a = arena();
        (a.audit, a.overflow_allocs, a.overflow_bytes, a.windows, a.count)
    });
    kprintln!("[arena] audit table (site / allocs / bytes):");
    for row in rows.iter().filter(|r| r.count != 0) {
        kprintln!("[arena]   {:#010x} {} {}", row.site, row.count, row.bytes);
    }
    if of_allocs != 0 {
        kprintln!("[arena]   sites beyond table: {} allocs, {} bytes", of_allocs, of_bytes);
    }
    for (i, w) in windows[..count].iter().enumerate() {
        kprintln!("[arena] window {}: headroom {} bytes", i, (w.hi - w.bump) as u32);
        #[cfg(any(rhu_ram_init_first_use, rhu_ram_init_background))]
        kprintln!("[arena]   init frontier {:#010x} ({} bytes above it still to zero)", w.clean, (w.hi - w.clean) as u32);
    }
}

/// Window 0's brackets + bump observability for the api smoke ONLY (the
/// carve and VA-burn checks read these under the CS; never shipped in
/// the default build).
#[cfg(feature = "api-smoke")]
pub fn probe_window() -> (usize, usize, usize) {
    with_cs(|| unsafe {
        let w = arena().windows[0];
        (w.lo, w.hi, w.bump)
    })
}

/// Audit-row observability for the api smoke ONLY: (count, bytes) for
/// `site`, (0, 0) when no row carries it -- the probe never invents a
/// row.
#[cfg(feature = "api-smoke")]
pub fn probe_site(site: u32) -> (u32, u32) {
    with_cs(|| unsafe {
        let a = arena();
        for row in a.audit.iter() {
            if row.count != 0 && row.site == site {
                return (row.count, row.bytes);
            }
        }
        (0, 0)
    })
}

/// The api smoke ONLY: the first window that follows the image's
/// lazy init mode (window 0 under `test-ram-init-window0`; the external
/// window otherwise) -- its index, brackets, hand-out edge and frontier:
/// (index, lo, hi, bump, clean). Dies loud in an image with no lazy window.
/// The frontier check's probe, so it exists where the check does (the
/// background mode).
#[cfg(all(feature = "api-smoke", rhu_ram_init_background))]
pub fn probe_frontier() -> (usize, usize, usize, usize, usize) {
    with_cs(|| unsafe {
        let a = arena();
        for (i, w) in a.windows[..a.count].iter().enumerate() {
            if lazy_window(i) {
                return (i, w.lo, w.hi, w.bump, w.clean);
            }
        }
        panic!("ram-init: no lazy window in this image (caller bug)");
    })
}

/// The api smoke ONLY (the background mode): re-arm the frontier
/// check's proof -- poison `bytes` of the lazy window above its hand-out
/// edge (clipped to the window) and rewind the frontier to that edge, so
/// the next hand-out straddles the frontier over poison and the idle step
/// has poison to clear, whatever the idle thread did before the check.
/// Inside the critical section whole (no step runs between the poison and
/// the rewind), bounded by the caller's `bytes`. Returns the poisoned
/// span's end.
#[cfg(all(feature = "api-smoke", rhu_ram_init_background))]
pub fn probe_frontier_rewind(bytes: usize) -> usize {
    with_cs(|| unsafe {
        let a = arena();
        for (i, w) in a.windows[..a.count].iter_mut().enumerate() {
            if lazy_window(i) {
                let end = if w.hi - w.bump > bytes { w.bump + bytes } else { w.hi };
                ptr::write_bytes(w.bump as *mut u8, 0xA5, end - w.bump);
                w.clean = w.bump;
                // A piece in flight is cancelled with the rewind as a
                // takeover cancels it (the generation bumped, the piece
                // cleared): the step's completion then moves nothing and
                // its next piece starts at the rewound frontier.
                if w.piece_hi != 0 {
                    w.generation = w.generation.wrapping_add(1);
                    w.piece_lo = 0;
                    w.piece_hi = 0;
                }
                return end;
            }
        }
        panic!("ram-init: no lazy window in this image (caller bug)");
    })
}
