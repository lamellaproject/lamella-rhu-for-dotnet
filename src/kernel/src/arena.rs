//! The arena: the GC's memory substrate. One arena per
//! image -- the single window every dynamic consumer draws from (the
//! kheap carve, TLS blocks, GC aux structures, the FOH pool, GC
//! regions). Bump-only at this level: individual frees never exist, and
//! **rezero** is the recycling story -- the decommit contract on a flat
//! address space.

use core::ptr::{self, addr_of};

use crate::critical::with_cs;
use crate::sched;

unsafe extern "C" {
    static __rhu_arena_lo: u32;
    static __rhu_arena_hi: u32;
}

/// Fixed audit capacity: enough for the kernel's own registered callers
/// plus the hosted consumers' call sites; growth past it lands in the
/// loud overflow row, never a silent drop.
const AUDIT_ROWS: usize = 16;

/// One per-site audit row. A row is occupied iff `count != 0`, so site
/// value 0 stays a legal (if odd) caller key.
#[derive(Clone, Copy)]
struct AuditRow {
    site: u32,
    count: u32,
    bytes: u32,
}

struct Arena {
    /// Next hand-out address; 8-aligned always (sizes round up to 8),
    /// so `hi - bump` is exactly allocatable headroom.
    bump: usize,
    lo: usize,
    hi: usize,
    audit: [AuditRow; AUDIT_ROWS],
    overflow_allocs: u32,
    overflow_bytes: u32,
}

static mut ARENA: Arena = Arena {
    bump: 0,
    lo: 0,
    hi: 0,
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
/// construction. One row today: the kheap carve is the only
/// kernel-internal arena caller (TLS draws from the kheap, not the
/// arena).
pub mod sites {
    /// The one boot-time kheap carve (kheap::init).
    pub const KHEAP_CARVE: u32 = 1;
}

/// Establish the window and the zero-birth invariant. Called from
/// `kmain` immediately before `sched::init()`: single-threaded, before
/// any thread exists, and deliberately silent on the console.
pub fn init() {
    let lo = addr_of!(__rhu_arena_lo) as usize;
    let hi = addr_of!(__rhu_arena_hi) as usize;
    #[cfg(feature = "api-smoke")]
    unsafe {
        ptr::write_bytes(lo as *mut u8, 0xA5, hi - lo);
    }
    unsafe {
        ptr::write_bytes(lo as *mut u8, 0, hi - lo);
        let a = arena();
        a.lo = lo;
        a.hi = hi;
        a.bump = lo;
    }
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

/// Aligned bump allocation attributed to `site`. NULL on exhaustion
/// only; wild arguments die loud (see the module header).
pub fn alloc_at(bytes: u32, align: u32, site: u32) -> *mut u8 {
    assert!(
        align.is_power_of_two() && align <= 4096,
        "arena: wild align {} (caller bug)",
        align
    );
    assert!(bytes != 0, "arena: zero-byte alloc (caller bug)");
    with_cs(|| unsafe {
        let a = arena();
        let mask = align as usize - 1;
        let base = match a.bump.checked_add(mask) {
            Some(v) => v & !mask,
            None => return ptr::null_mut(),
        };
        let size = match (bytes as usize).checked_add(7) {
            Some(v) => v & !7,
            None => return ptr::null_mut(),
        };
        match base.checked_add(size) {
            Some(end) if end <= a.hi => {
                a.bump = end;
                audit_add(a, site, size);
                base as *mut u8
            }
            _ => ptr::null_mut(),
        }
    })
}

/// Re-establish the zero-birth invariant over `[p, p+bytes)`. The range
/// must lie inside HANDED-OUT arena space (`[lo, bump)`); 0 on success,
/// `ERR_INVAL` otherwise with nothing touched. Headroom and the next
/// hand-out address are untouched either way -- that is the "zero VA
/// burned" contract the gate checks.
pub fn rezero(p: *mut u8, bytes: u32) -> i32 {
    with_cs(|| unsafe {
        let a = arena();
        let start = p as usize;
        let end = match start.checked_add(bytes as usize) {
            Some(e) => e,
            None => return sched::ERR_INVAL,
        };
        if start < a.lo || end > a.bump {
            return sched::ERR_INVAL;
        }
        ptr::write_bytes(p, 0, bytes as usize);
        0
    })
}

/// Bytes an immediately following 8-aligned allocation could still get,
/// exactly (`hi - bump`; both stay 8-aligned). 0 means exhausted.
pub fn headroom() -> u32 {
    with_cs(|| unsafe {
        let a = arena();
        (a.hi - a.bump) as u32
    })
}

/// Dump the audit table. Diagnostic: noisy by design, never called by
/// ship flavors or gated images (site values are image-layout facts, so
/// this output must never join a golden or a parity stream).
pub fn report() {
    let (rows, of_allocs, of_bytes, free) = with_cs(|| unsafe {
        let a = arena();
        (a.audit, a.overflow_allocs, a.overflow_bytes, (a.hi - a.bump) as u32)
    });
    kprintln!("[arena] audit table (site / allocs / bytes):");
    for row in rows.iter().filter(|r| r.count != 0) {
        kprintln!("[arena]   {:#010x} {} {}", row.site, row.count, row.bytes);
    }
    if of_allocs != 0 {
        kprintln!("[arena]   sites beyond table: {} allocs, {} bytes", of_allocs, of_bytes);
    }
    kprintln!("[arena] headroom: {} bytes", free);
}

/// Window + bump observability for the api smoke ONLY (the carve and
/// VA-burn checks read these under the CS; never shipped in the default
/// build).
#[cfg(feature = "api-smoke")]
pub fn probe_window() -> (usize, usize, usize) {
    with_cs(|| unsafe {
        let a = arena();
        (a.lo, a.hi, a.bump)
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
