//! The host tier's cases. Each case that touches the arena or the heap
//! boots them first (the arena's init resets its window and the heap's
//! init re-carves, so the cases are independent in any order); the wake
//! cases are pure arithmetic.

use crate::{arena, board, kheap, wake};

fn boot() {
    arena::init();
    kheap::init();
}

#[test]
fn kheap_rounding_wraps_refused() {
    let heap = board::KHEAP_BYTES as u32;
    assert_eq!(kheap::rounded_request(1, heap), Some(8));
    assert_eq!(kheap::rounded_request(8, heap), Some(8));
    assert_eq!(kheap::rounded_request(9, heap), Some(16));
    assert_eq!(kheap::rounded_request(0xFFFF_FFF7, heap), None, "the rounding wraps");
    assert_eq!(kheap::rounded_request(0xFFFF_FFF8, heap), None, "the rounding wraps");
    assert_eq!(kheap::rounded_request(heap - 16, heap), Some(heap - 16), "the largest span the heap holds");
    assert_eq!(kheap::rounded_request(heap - 15, heap), None, "one byte past the largest span");
    assert_eq!(kheap::rounded_request(heap, heap), None, "a heap-sized payload plus its header");
}

#[test]
fn kheap_wrapping_request_refused_null_heap_intact() {
    boot();
    assert!(kheap::alloc(0xFFFF_FFF7, 8).is_null(), "the finding's request: a 16-byte block today");
    assert!(kheap::alloc(0x7000_0000, 8).is_null(), "an over-heap request");
    let p = kheap::alloc(40, 8);
    assert!(!p.is_null(), "the heap intact after the refusals");
    kheap::free(p);
}

#[test]
fn kheap_largest_fits_and_one_more_byte_does_not() {
    boot();
    let heap = board::KHEAP_BYTES as u32;
    assert!(kheap::alloc(heap, 8).is_null());
    assert!(kheap::alloc(heap - 15, 8).is_null());
    let p = kheap::alloc(heap - 16, 8);
    assert!(!p.is_null(), "the whole heap less one header, in one block");
    assert!(kheap::alloc(8, 8).is_null(), "nothing left beside it");
    kheap::free(p);
    let q = kheap::alloc(heap - 16, 8);
    assert!(!q.is_null(), "the freed block is the whole heap again");
    kheap::free(q);
}

#[test]
fn wake_arm_inside_window_is_exact() {
    assert_eq!(wake::arm(100, 200), (200, false));
    assert_eq!(wake::arm(100, 100), (100, false));
    assert_eq!(wake::arm(100, 50), (50, false), "a target already behind arms as itself and expires at once");
    assert_eq!(wake::arm(100, 100 + wake::WINDOW), ((100 + wake::WINDOW) as u32, false), "the window's last tick is exact");
}

#[test]
fn wake_arm_beyond_window_clamps_and_rearms() {
    let now = 5u64;
    let target = now + wake::WINDOW + 2;
    let (armed, clamped) = wake::arm(now, target);
    assert!(clamped && armed as u64 == now + wake::WINDOW);
    assert!(!wake::expired(now as u32, armed), "the clamped arm is not due now");
    assert!(wake::expired(target as u32, target as u32));
    assert!(wake::expired(now as u32, target as u32), "the finding's shape: a truncated arm of the same target reads as due at once");
    assert!(!wake::expired(now as u32, (now + wake::WINDOW + 1) as u32), "exactly 2^31 ahead is the last tick the truncation survives");
    assert!(wake::expired((now + wake::WINDOW) as u32, armed), "the clamped arm is due at the window's edge");
    let (armed2, clamped2) = wake::arm(now + wake::WINDOW, target);
    assert!(!clamped2 && armed2 == target as u32, "re-armed at the edge, the target now lies inside");
    let far = now + (1u64 << 40);
    let (a1, c1) = wake::arm(now, far);
    assert!(c1 && a1 as u64 == now + wake::WINDOW, "2^40 ticks out: the first arm is the window's edge");
}

#[test]
fn wake_arm_across_the_32bit_wrap() {
    let now = (1u64 << 32) + 5;
    let target = now + 10;
    assert_eq!(wake::arm(now, target), (15, false));
    assert!(!wake::expired(5, 15));
    assert!(wake::expired(15, 15));
    assert!(wake::expired(16, 15));
    assert!(!wake::expired(0xFFFF_FFF0, 3), "a target just past the wrap is ahead of a now just before it");
    assert!(wake::expired(3, 0xFFFF_FFF0), "and due once now has wrapped past it");
}

#[test]
fn wake_tick_for_deadline_ceils_and_saturates() {
    assert_eq!(wake::tick_for_deadline(0, 1_000_000), 1);
    assert_eq!(wake::tick_for_deadline(1, 1_000_000), 2);
    assert_eq!(wake::tick_for_deadline(1_000_000, 1_000_000), 2);
    assert_eq!(wake::tick_for_deadline(1_000_001, 1_000_000), 3);
    assert_eq!(wake::tick_for_deadline(u64::MAX, 1), u64::MAX, "the top of the range saturates");
}

#[test]
fn arena_overflow_refused_headroom_intact() {
    boot();
    let before = arena::headroom();
    assert!(arena::alloc_at(u32::MAX - 3, 8, 7).is_null(), "a request past the window (its rounding wraps on a 32-bit target)");
    assert!(arena::alloc_at(before.saturating_add(8), 8, 7).is_null(), "a request past the headroom");
    assert_eq!(arena::headroom(), before, "a refused alloc changed nothing");
    let p = arena::alloc_at(64, 8, 7);
    assert!(!p.is_null() && (p as usize) % 8 == 0);
    assert_eq!(arena::headroom(), before - 64, "an 8-aligned draw spends exactly its size");
    let q = arena::alloc_at(64, 64, 7);
    assert!(!q.is_null() && (q as usize) % 64 == 0, "a 64-aligned draw lands aligned");
    assert!(arena::headroom() <= before - 128 && arena::headroom() >= before - 128 - 56, "and spends its size plus at most the alignment slack");
}
