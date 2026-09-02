//! The kernel heap: the malloc-class allocator the shim
//! maps onto. Carved FROM the arena once at boot (`KHEAP_BYTES`, a
//! board const -- the GC board table's `kheap_carve` row) and never
//! grown: the arena
//! stays the single window every dynamic consumer draws from, and the
//! kheap is one registered draw with one audited site.

use core::ptr::{self, read_volatile, write_volatile};

use crate::arena::{self, sites};
use crate::board;
use crate::critical::with_cs;

/// Boundary header, 16 bytes, 8-aligned so payloads are 8-aligned.
/// `size` spans the WHOLE block (header included); `next` links the
/// address-ordered free list (0 = end) and is meaningful only while
/// the block is FREE.
const HDR_WORDS: usize = 4;
pub(crate) const HDR_BYTES: u32 = (HDR_WORDS * 4) as u32;

/// Smallest block worth splitting off: a header plus the 8-byte
/// minimum payload. Leading alignment gaps below this advance to the
/// next alignment step instead of orphaning unusable bytes.
const MIN_SPAN: u32 = HDR_BYTES + 8;

const STATE_FREE: u32 = 0x4652_4545;
const STATE_USED: u32 = 0x5553_4544;

/// Address-derived, so a header memcpy'd to another address fails its
/// canary there -- corruption cannot masquerade by relocation.
fn canary_for(h: usize) -> u32 {
    (h as u32) ^ 0x4B48_5552
}

struct Kheap {
    lo: usize,
    hi: usize,
    /// Head of the address-ordered free list (0 = exhausted).
    free_head: usize,
}

static mut KHEAP: Kheap = Kheap { lo: 0, hi: 0, free_head: 0 };

#[allow(clippy::mut_from_ref)]
unsafe fn kheap() -> &'static mut Kheap { unsafe {
    &mut *ptr::addr_of_mut!(KHEAP)
}}

#[inline]
unsafe fn hdr_read(h: usize, word: usize) -> u32 { unsafe {
    read_volatile((h + word * 4) as *const u32)
}}

#[inline]
unsafe fn hdr_write(h: usize, word: usize, v: u32) { unsafe {
    write_volatile((h + word * 4) as *mut u32, v)
}}

/// Die loud on any header whose canary or state word is wrong. One
/// constant message on purpose: the kheap-loud designed-death golden
/// pins this text, and no address may ride in it.
unsafe fn validate(h: usize) { unsafe {
    let state = hdr_read(h, 1);
    let ok = (state == STATE_FREE || state == STATE_USED) && hdr_read(h, 2) == canary_for(h);
    assert!(ok, "kheap: corrupt block header");
}}

/// Carve the heap from the arena and shape it as one free block.
/// Called from `kmain` right after `arena::init()`: single-threaded,
/// silent on the console. A carve the arena cannot cover is a
/// board-sizing bug and dies loud at first boot, never a quiet
/// degradation.
pub fn init() {
    let carve = arena::alloc_at(board::KHEAP_BYTES as u32, 8, sites::KHEAP_CARVE);
    assert!(!carve.is_null(), "kheap: arena cannot cover the carve (board sizing bug)");
    let base = carve as usize;
    unsafe {
        hdr_write(base, 0, board::KHEAP_BYTES as u32);
        hdr_write(base, 1, STATE_FREE);
        hdr_write(base, 2, canary_for(base));
        hdr_write(base, 3, 0);
        let k = kheap();
        k.lo = base;
        k.hi = base + board::KHEAP_BYTES;
        k.free_head = base;
    }
}

/// First-fit allocation. NULL on exhaustion only; wild arguments die
/// loud (module header). The returned payload satisfies `align`.
pub fn alloc(bytes: u32, align: u32) -> *mut u8 {
    assert!(
        align.is_power_of_two() && align <= 4096,
        "kheap: wild align {} (caller bug)",
        align
    );
    assert!(bytes != 0, "kheap: zero-byte alloc (caller bug)");
    let need_payload = (bytes + 7) & !7;
    with_cs(|| unsafe {
        let k = kheap();
        let mut prev: usize = 0;
        let mut p = k.free_head;
        while p != 0 {
            validate(p);
            assert!(hdr_read(p, 1) == STATE_FREE, "kheap: corrupt block header");
            let size = hdr_read(p, 0);
            let block_end = p + size as usize;

            let mut payload = (p + HDR_BYTES as usize + (align as usize - 1)) & !(align as usize - 1);
            while payload - HDR_BYTES as usize > p
                && ((payload - HDR_BYTES as usize - p) as u32) < MIN_SPAN
            {
                payload += align as usize;
            }
            let h = payload - HDR_BYTES as usize;
            let used_end = payload + need_payload as usize;
            if used_end <= block_end {
                let gap = (h - p) as u32;
                let tail = (block_end - used_end) as u32;
                let (used_span, tail_at) = if tail >= MIN_SPAN {
                    (HDR_BYTES + need_payload, used_end)
                } else {
                    (HDR_BYTES + need_payload + tail, 0)
                };
                let next_after = if tail_at != 0 {
                    hdr_write(tail_at, 0, tail);
                    hdr_write(tail_at, 1, STATE_FREE);
                    hdr_write(tail_at, 2, canary_for(tail_at));
                    hdr_write(tail_at, 3, hdr_read(p, 3));
                    tail_at
                } else {
                    hdr_read(p, 3) as usize
                };
                if gap != 0 {
                    hdr_write(p, 0, gap);
                    hdr_write(p, 3, next_after as u32);
                } else if prev == 0 {
                    k.free_head = next_after;
                } else {
                    hdr_write(prev, 3, next_after as u32);
                }
                hdr_write(h, 0, used_span);
                hdr_write(h, 1, STATE_USED);
                hdr_write(h, 2, canary_for(h));
                hdr_write(h, 3, 0);
                return payload as *mut u8;
            }
            prev = p;
            p = hdr_read(p, 3) as usize;
        }
        ptr::null_mut()
    })
}

/// Free with coalescing: the block merges with an adjacent free
/// neighbor on either side. NULL is a no-op; freeing a block that is
/// not USED (double free, wild pointer into the heap) dies loud.
pub fn free(p: *mut u8) {
    if p.is_null() {
        return;
    }
    let h = p as usize - HDR_BYTES as usize;
    with_cs(|| unsafe {
        let k = kheap();
        assert!(h >= k.lo && h < k.hi, "kheap: free of a pointer outside the heap");
        validate(h);
        assert!(hdr_read(h, 1) == STATE_USED, "kheap: free of a block not in use");
        let mut size = hdr_read(h, 0);

        let mut prev: usize = 0;
        let mut next = k.free_head;
        while next != 0 && next < h {
            validate(next);
            prev = next;
            next = hdr_read(next, 3) as usize;
        }
        if next != 0 {
            validate(next);
        }

        let mut link_next = next;
        if next != 0 && h + size as usize == next {
            size += hdr_read(next, 0);
            link_next = hdr_read(next, 3) as usize;
            ptr::write_bytes(next as *mut u8, 0, HDR_BYTES as usize);
        }
        if prev != 0 && prev + hdr_read(prev, 0) as usize == h {
            hdr_write(prev, 0, hdr_read(prev, 0) + size);
            hdr_write(prev, 3, link_next as u32);
            ptr::write_bytes(h as *mut u8, 0, HDR_BYTES as usize);
            return;
        }
        hdr_write(h, 0, size);
        hdr_write(h, 1, STATE_FREE);
        hdr_write(h, 2, canary_for(h));
        hdr_write(h, 3, link_next as u32);
        if prev == 0 {
            k.free_head = h;
        } else {
            hdr_write(prev, 3, h as u32);
        }
    })
}

/// Corrupt the first free block's canary -- the kheap-loud designed
/// death's trigger and nothing else's. The next walk that touches the
/// block must die through the funnel.
#[cfg(feature = "test-kheap-loud")]
pub fn probe_corrupt_first_free() {
    with_cs(|| unsafe {
        let k = kheap();
        assert!(k.free_head != 0, "kheap-loud probe: no free block to corrupt");
        let h = k.free_head;
        hdr_write(h, 2, hdr_read(h, 2) ^ 1);
    })
}
