//! The host tier's cases. Each case that touches the arena or the heap
//! boots them first (the arena's init resets its window and the heap's
//! init re-carves, so the cases are independent in any order); the wake
//! cases are pure arithmetic.

use crate::{abi_window, adc_math, arena, board, bus_math, calendar_math, claims, gpio_math, kheap, pwm_math, rp2350_map, wake};

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
    assert!(kheap::alloc(0xFFFF_FFF7, 8).is_null(), "the reported request: a 16-byte block");
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
    assert!(wake::expired(now as u32, target as u32), "the reported shape: a truncated arm of the same target reads as due at once");
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


#[test]
fn calendar_days_from_civil_round_trips_the_constants() {
    let pins: [(u64, (u32, u32, u32, u32, u32, u32)); 4] = [
        (0, (1, 1, 1, 0, 0, 0)),
        (calendar_math::TICKS_UNIX_EPOCH, (1970, 1, 1, 0, 0, 0)),
        (calendar_math::TICKS_2000, (2000, 1, 1, 0, 0, 0)),
        (631_167_699_060_000_000, (2001, 2, 3, 4, 5, 6)),
    ];
    for (ticks, (y, mo, d, h, mi, s)) in pins {
        let c = calendar_math::Civil { year: y, month: mo, day: d, hour: h, minute: mi, second: s };
        assert_eq!(calendar_math::civil_to_ticks(c), ticks, "civil -> ticks for {y}-{mo}-{d}");
        assert_eq!(calendar_math::ticks_to_civil(ticks), c, "ticks -> civil for {ticks}");
    }
    assert_eq!(calendar_math::days_from_civil(1970, 1, 1), 719_162);
    assert_eq!(calendar_math::weekday_from_days(719_162), 4);
    assert_eq!(calendar_math::weekday_from_days(0), 1);
    assert_eq!(calendar_math::weekday_from_days(calendar_math::days_from_civil(2000, 1, 1)), 6);
}

#[test]
fn calendar_civil_from_days_at_the_maximum() {
    let c = calendar_math::ticks_to_civil(calendar_math::TICKS_MAX);
    assert_eq!(
        c,
        calendar_math::Civil { year: 9999, month: 12, day: 31, hour: 23, minute: 59, second: 59 }
    );
    let d2000 = calendar_math::days_from_civil(2000, 1, 1);
    for i in 0..366u64 {
        let (y, m, d) = calendar_math::civil_from_days(d2000 + i);
        assert_eq!(calendar_math::days_from_civil(y, m, d), d2000 + i, "day {i} of 2000");
    }
    assert_eq!(calendar_math::days_from_civil(1900, 3, 1) - calendar_math::days_from_civil(1900, 2, 28), 1);
    assert_eq!(calendar_math::days_from_civil(2000, 3, 1) - calendar_math::days_from_civil(2000, 2, 28), 2);
}

#[test]
fn calendar_bcd_packs_both_ways() {
    for v in 0..100u32 {
        assert_eq!(calendar_math::unbcd(calendar_math::bcd(v)), v);
    }
    assert_eq!(calendar_math::bcd(59), 0x59);
    assert_eq!(calendar_math::bcd(7), 0x07);
    let (tr, dr) = calendar_math::st_pack(631_167_699_060_000_000).expect("inside the century");
    assert_eq!(tr, 0x0004_0506);
    assert_eq!(dr, 0x0001_C203);
    assert_eq!(calendar_math::st_unpack(tr, dr), 631_167_699_060_000_000);
}

#[test]
fn calendar_century_bound_refuses_1999_and_2100() {
    assert!(calendar_math::st_pack(calendar_math::TICKS_2000 - 1).is_none(), "the last instant before the century packed");
    assert!(calendar_math::st_pack(calendar_math::TICKS_2000).is_some(), "the century's first instant refused");
    let c2100 = calendar_math::Civil { year: 2100, month: 1, day: 1, hour: 0, minute: 0, second: 0 };
    assert!(calendar_math::st_pack(calendar_math::civil_to_ticks(c2100)).is_none(), "the first instant after the century packed");
    let c2099 = calendar_math::Civil { year: 2099, month: 12, day: 31, hour: 23, minute: 59, second: 59 };
    assert!(calendar_math::st_pack(calendar_math::civil_to_ticks(c2099)).is_some(), "the century's last second refused");
}

#[test]
fn calendar_set_saturates_at_the_epoch() {
    assert_eq!(calendar_math::base_for(1_000, 400), 600);
    assert_eq!(calendar_math::base_for(400, 1_000), 0);
    assert_eq!(calendar_math::base_for(0, 0), 0);
    assert_eq!(calendar_math::base_for(calendar_math::TICKS_MAX, 0), calendar_math::TICKS_MAX);
}

#[test]
fn abi_window_judges_the_window() {
    use abi_window::{judge, Verdict};
    let (abi, compat) = (32, 31);
    assert_eq!(judge(false, 0, abi, compat), Verdict::DefaultUnstated);
    assert_eq!(judge(true, 0, abi, compat), Verdict::RefusedUnstated);
    assert_eq!(judge(false, 32, abi, compat), Verdict::DefaultStated(32));
    assert_eq!(judge(false, 31, abi, compat), Verdict::DefaultStated(31));
    assert_eq!(judge(true, 32, abi, compat), Verdict::ProvidedInside(32));
    assert_eq!(judge(true, 31, abi, compat), Verdict::ProvidedInside(31));
    for n in [30, 33, 1, u32::MAX] {
        assert_eq!(judge(true, n, abi, compat), Verdict::RefusedOutside(n));
        assert_eq!(judge(false, n, abi, compat), Verdict::RefusedOutside(n));
    }
    assert_eq!(judge(true, 32, 32, 32), Verdict::ProvidedInside(32));
    assert_eq!(judge(true, 31, 32, 32), Verdict::RefusedOutside(31));
}


mod fp {
    pub use crate::fpemul::soft::*;
}

/// The context every emulator row boots with: round to nearest, no default-NaN, no flush-to-zero.
fn fp_rn() -> fp::Ctx {
    fp::Ctx::from_fpscr(0)
}

/// A deterministic 64-bit generator for the sweeps (xorshift64*).
struct Sweep(u64);
impl Sweep {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    /// A double drawn over the whole bit space with the exponent biased toward the usable range.
    fn next_f64(&mut self) -> u64 {
        let r = self.next();
        match r >> 61 {
            0 => r,
            1 => (r & 0x800F_FFFF_FFFF_FFFF) | ((1000 + (r >> 4) % 48) << 52),
            2 => (r & 0x800F_FFFF_FFFF_FFFF) | (((r >> 4) % 2047) << 52),
            3 => r & 0x800F_FFFF_FFFF_FFFF,
            _ => ((r >> 12) as f64 * (1.0 / 4096.0)).to_bits() ^ (r & (1 << 63)),
        }
    }
}

/// The edge table: every class and boundary a rounding or a NaN rule can turn on.
fn fp_edges() -> Vec<u64> {
    let v: [f64; 20] = [
        0.0, 1.0, -1.0, 2.0, 0.5, 3.0, 1.0 / 3.0, 10.0, 1e10, 1e-10, 1e300, 1e-300, f64::MAX, f64::MIN_POSITIVE,
        f64::EPSILON, 4503599627370496.0, 4503599627370497.0, 9007199254740992.0, 2147483647.0, 4294967295.0,
    ];
    let mut t: Vec<u64> = v.iter().flat_map(|&x| [x.to_bits(), (-x).to_bits()]).collect();
    t.extend_from_slice(&[
        0x8000_0000_0000_0000,
        0x7FF0_0000_0000_0000,
        0xFFF0_0000_0000_0000,
        0x7FF8_0000_0000_0000,
        0x7FF8_0000_0000_0001,
        0xFFF8_0000_0000_0002,
        0x7FF0_0000_0000_0001,
        0x7FF4_0000_0000_0000,
        0x0000_0000_0000_0001,
        0x8000_0000_0000_0001,
        0x000F_FFFF_FFFF_FFFF,
        0x0010_0000_0000_0000,
        0x7FEF_FFFF_FFFF_FFFF,
        0x3FF0_0000_0000_0001,
        0x3FEF_FFFF_FFFF_FFFF,
        0x4340_0000_0000_0001,
        0x41E0_0000_0000_0000,
        0x41EF_FFFF_FFC0_0000,
        0xC1E0_0000_0020_0000,
    ]);
    t
}

fn fp_same(a: u64, b: u64) -> bool {
    let (x, y) = (f64::from_bits(a), f64::from_bits(b));
    if x.is_nan() && y.is_nan() {
        return true;
    }
    a == b
}

fn fp_check2(name: &str, a: u64, b: u64, soft: u64, host: f64) {
    assert!(
        fp_same(soft, host.to_bits()),
        "{name}({:#018x}, {:#018x}) = {:#018x} (soft) vs {:#018x} (host)",
        a,
        b,
        soft,
        host.to_bits()
    );
}

#[test]
fn fpemul_arithmetic_matches_the_host_in_round_to_nearest() {
    let edges = fp_edges();
    let mut n = 0u32;
    for &a in &edges {
        for &b in &edges {
            let (x, y) = (f64::from_bits(a), f64::from_bits(b));
            fp_check2("add", a, b, fp::add(&mut fp_rn(), a, b), x + y);
            fp_check2("sub", a, b, fp::sub(&mut fp_rn(), a, b), x - y);
            fp_check2("mul", a, b, fp::mul(&mut fp_rn(), a, b), x * y);
            fp_check2("div", a, b, fp::div(&mut fp_rn(), a, b), x / y);
            n += 4;
        }
        let x = f64::from_bits(a);
        fp_check2("sqrt", a, 0, fp::sqrt(&mut fp_rn(), a), x.sqrt());
        n += 1;
    }
    let mut s = Sweep(0x9E37_79B9_7F4A_7C15);
    for _ in 0..65536 {
        let (a, b, c) = (s.next_f64(), s.next_f64(), s.next_f64());
        let (x, y, z) = (f64::from_bits(a), f64::from_bits(b), f64::from_bits(c));
        fp_check2("add", a, b, fp::add(&mut fp_rn(), a, b), x + y);
        fp_check2("sub", a, b, fp::sub(&mut fp_rn(), a, b), x - y);
        fp_check2("mul", a, b, fp::mul(&mut fp_rn(), a, b), x * y);
        fp_check2("div", a, b, fp::div(&mut fp_rn(), a, b), x / y);
        fp_check2("sqrt", a, 0, fp::sqrt(&mut fp_rn(), a), x.sqrt());
        let f = fp::fma(&mut fp_rn(), c, a, b);
        assert!(fp_same(f, z.mul_add(x, y).to_bits()) || fp_same(f, x.mul_add(y, z).to_bits()), "fma({a:#x},{b:#x},{c:#x})");
        n += 6;
    }
    assert!(n > 65536 * 6);
}

#[test]
fn fpemul_fma_is_fused_and_matches_the_host() {
    let a = 1.0f64 + f64::EPSILON;
    let b = 1.0f64 - f64::EPSILON;
    let c = -1.0f64;
    let fused = fp::fma(&mut fp_rn(), c.to_bits(), a.to_bits(), b.to_bits());
    assert_eq!(fused, a.mul_add(b, c).to_bits());
    assert_ne!(fused, (a * b + c).to_bits(), "the unfused sum differs: the emulation must be fused");
    let mut s = Sweep(0xD1B5_4A32_D192_ED03);
    for _ in 0..65536 {
        let (a, b, c) = (s.next_f64(), s.next_f64(), s.next_f64());
        let (x, y, z) = (f64::from_bits(a), f64::from_bits(b), f64::from_bits(c));
        let f = fp::fma(&mut fp_rn(), c, a, b);
        assert!(fp_same(f, x.mul_add(y, z).to_bits()), "fma addend {c:#x} + {a:#x} * {b:#x}: {f:#x} vs {:#x}", x.mul_add(y, z).to_bits());
    }
}

#[test]
fn fpemul_conversions_match_the_host() {
    let edges = fp_edges();
    let mut s = Sweep(0x1234_5678_9ABC_DEF1);
    let more: Vec<u64> = (0..65536).map(|_| s.next_f64()).collect();
    for &a in edges.iter().chain(more.iter()) {
        let x = f64::from_bits(a);
        let f32_soft = fp::to_f32(&mut fp_rn(), a);
        let f32_host = (x as f32).to_bits();
        assert!(
            (f32::from_bits(f32_soft).is_nan() && f32::from_bits(f32_host).is_nan()) || f32_soft == f32_host,
            "to_f32({a:#x}) = {f32_soft:#x} vs {f32_host:#x}"
        );
        let back = fp::from_f32(&mut fp_rn(), f32_host);
        assert!(fp_same(back, (f32::from_bits(f32_host) as f64).to_bits()), "from_f32({f32_host:#x})");
        assert_eq!(fp::to_int32(&mut fp_rn(), a, true, fp::Round::Zero) as i32, x as i32, "to_int32 signed {a:#x}");
        assert_eq!(fp::to_int32(&mut fp_rn(), a, false, fp::Round::Zero), x as u32, "to_int32 unsigned {a:#x}");
        let i = a as u32;
        assert_eq!(fp::from_int32(&mut fp_rn(), i, true), (i as i32 as f64).to_bits());
        assert_eq!(fp::from_int32(&mut fp_rn(), i, false), (i as f64).to_bits());
        if !x.is_nan() {
            assert_eq!(fp::round_to_integral(&mut fp_rn(), a, fp::Round::Zero, false), x.trunc().to_bits(), "vrintz {a:#x}");
            assert_eq!(fp::round_to_integral(&mut fp_rn(), a, fp::Round::Plus, false), x.ceil().to_bits(), "vrintp {a:#x}");
            assert_eq!(fp::round_to_integral(&mut fp_rn(), a, fp::Round::Minus, false), x.floor().to_bits(), "vrintm {a:#x}");
            assert_eq!(fp::round_to_integral(&mut fp_rn(), a, fp::Round::Away, false), x.round().to_bits(), "vrinta {a:#x}");
            assert_eq!(fp::round_to_integral(&mut fp_rn(), a, fp::Round::Nearest, false), x.round_ties_even().to_bits(), "vrintn {a:#x}");
        }
    }
    for (h, want) in [(0x3C00u32, 1.0f64), (0xC000, -2.0), (0x7BFF, 65504.0), (0x0001, 5.960464477539063e-8), (0x0400, 6.103515625e-5)] {
        assert_eq!(fp::from_f16(&mut fp_rn(), h), want.to_bits(), "from_f16 {h:#x}");
        assert_eq!(fp::to_f16(&mut fp_rn(), want.to_bits()), h, "to_f16 {want}");
    }
    assert_eq!(fp::to_f16(&mut fp_rn(), 70000.0f64.to_bits()), 0x7C00, "overflow to +inf in the IEEE half");
    let mut ahp = fp::Ctx::from_fpscr(fp::FPSCR_AHP);
    assert_eq!(fp::to_f16(&mut ahp, 70000.0f64.to_bits()), 0x7C46, "the alternative format reaches 70000");
    assert_eq!(fp::from_f16(&mut fp::Ctx::from_fpscr(fp::FPSCR_AHP), 0x7C46), 70016.0f64.to_bits());
    let mut ahp2 = fp::Ctx::from_fpscr(fp::FPSCR_AHP);
    assert_eq!(fp::to_f16(&mut ahp2, 200000.0f64.to_bits()), 0x7FFF, "beyond the alternative format's largest value: saturated");
    assert_eq!(ahp2.flags, fp::IOC, "an overflow in the alternative format: invalid alone, as the reference row raises it");
    let mut ahp3 = fp::Ctx::from_fpscr(fp::FPSCR_AHP);
    assert_eq!(fp::to_f16(&mut ahp3, 131072.0f64.to_bits()), 0x7FFF, "an exact power of two beyond the format: saturated");
    assert_eq!(ahp3.flags, fp::IOC, "and invalid alone for an exact power of two as well");
    assert_eq!(fp::to_fixed(&mut fp_rn(), 1.5f64.to_bits(), true, 32, 4), 24);
    assert_eq!(fp::to_fixed(&mut fp_rn(), (-1.5f64).to_bits(), true, 16, 8), (-384i32) as u32 & 0xFFFF);
    assert_eq!(fp::from_fixed(&mut fp_rn(), 24, true, 32, 4), 1.5f64.to_bits());
    assert_eq!(fp::from_fixed(&mut fp_rn(), 0xFE80, true, 16, 8), (-1.5f64).to_bits());
    assert_eq!(fp::expand_imm(0x70), 1.0f64.to_bits());
    assert_eq!(fp::expand_imm(0xF0), (-1.0f64).to_bits());
    assert_eq!(fp::expand_imm(0x00), 2.0f64.to_bits());
    assert_eq!(fp::expand_imm(0x7C), 1.75f64.to_bits());
    assert_eq!(fp::expand_imm(0x3F), 31.0f64.to_bits(), "the largest immediate: 31/16 * 2^4");
    assert_eq!(fp::expand_imm(0x1F), 7.75f64.to_bits(), "31/16 * 2^2");
    assert_eq!(fp::expand_imm(0x40), 0.125f64.to_bits(), "the smallest: 16/16 * 2^-3");
}

#[test]
fn fpemul_compare_max_min_match_the_host() {
    use std::cmp::Ordering;
    let edges = fp_edges();
    for &a in &edges {
        for &b in &edges {
            let (x, y) = (f64::from_bits(a), f64::from_bits(b));
            let mut c = fp_rn();
            let nzcv = fp::compare(&mut c, a, b, false);
            let want = match x.partial_cmp(&y) {
                Some(Ordering::Equal) => fp::FPSCR_Z | fp::FPSCR_C,
                Some(Ordering::Less) => fp::FPSCR_N,
                Some(Ordering::Greater) => fp::FPSCR_C,
                None => fp::FPSCR_C | fp::FPSCR_V,
            };
            assert_eq!(nzcv, want, "compare({a:#x}, {b:#x})");
            let snan = |v: u64| (v & 0x7FF0_0000_0000_0000) == 0x7FF0_0000_0000_0000 && v & 0x000F_FFFF_FFFF_FFFF != 0 && v & (1 << 51) == 0;
            assert_eq!(c.flags & fp::IOC != 0, snan(a) || snan(b), "vcmp signals only on a signalling NaN ({a:#x}, {b:#x})");
            let mut ce = fp_rn();
            fp::compare(&mut ce, a, b, true);
            assert_eq!(ce.flags & fp::IOC != 0, x.is_nan() || y.is_nan(), "vcmpe signals on any NaN");
            if !x.is_nan() && !y.is_nan() && !(x == 0.0 && y == 0.0) {
                assert_eq!(fp::max_min_num(&mut fp_rn(), a, b, true), x.max(y).to_bits(), "maxnm({a:#x}, {b:#x})");
                assert_eq!(fp::max_min_num(&mut fp_rn(), a, b, false), x.min(y).to_bits(), "minnm({a:#x}, {b:#x})");
            }
        }
    }
    assert_eq!(fp::max_min_num(&mut fp_rn(), 0x7FF8_0000_0000_0000, 1.0f64.to_bits(), true), 1.0f64.to_bits());
    assert_eq!(fp::max_min_num(&mut fp_rn(), 1.0f64.to_bits(), 0x7FF8_0000_0000_0000, false), 1.0f64.to_bits());
    assert_eq!(fp::max_min_num(&mut fp_rn(), 0x8000_0000_0000_0000, 0, true), 0);
    assert_eq!(fp::max_min_num(&mut fp_rn(), 0x8000_0000_0000_0000, 0, false), 0x8000_0000_0000_0000);
}

#[test]
fn fpemul_flags_and_directed_rounding_by_hand() {
    let one = 1.0f64.to_bits();
    let zero = 0u64;
    let inf = 0x7FF0_0000_0000_0000u64;
    let mut c = fp_rn();
    assert_eq!(fp::div(&mut c, one, zero), inf);
    assert_eq!(c.flags, fp::DZC);
    let mut c = fp_rn();
    assert_eq!(fp::sub(&mut c, inf, inf), 0x7FF8_0000_0000_0000);
    assert_eq!(c.flags, fp::IOC);
    let mut c = fp_rn();
    fp::mul(&mut c, zero, inf);
    assert_eq!(c.flags, fp::IOC);
    let mut c = fp_rn();
    assert_eq!(fp::sqrt(&mut c, (-1.0f64).to_bits()), 0x7FF8_0000_0000_0000);
    assert_eq!(c.flags, fp::IOC);
    let mut c = fp_rn();
    assert_eq!(fp::add(&mut c, 0x7FF0_0000_0000_0001, one), 0x7FF8_0000_0000_0001, "the signalling NaN quieted, its payload kept");
    assert_eq!(c.flags, fp::IOC);
    let mut c = fp::Ctx::from_fpscr(fp::FPSCR_DN);
    assert_eq!(fp::add(&mut c, 0x7FF8_0000_0000_0001, one), 0x7FF8_0000_0000_0000, "the default NaN under DN");
    let mut c = fp_rn();
    assert_eq!(fp::mul(&mut c, f64::MAX.to_bits(), 2.0f64.to_bits()), inf);
    assert_eq!(c.flags, fp::OFC | fp::IXC);
    let mut c = fp::Ctx::from_fpscr(3 << fp::FPSCR_RMODE_SHIFT);
    assert_eq!(fp::mul(&mut c, f64::MAX.to_bits(), 2.0f64.to_bits()), f64::MAX.to_bits(), "overflow toward zero stays finite");
    assert_eq!(c.flags, fp::OFC | fp::IXC);
    let mut c = fp_rn();
    assert_eq!(fp::div(&mut c, 1u64, 2.0f64.to_bits()), 0, "the smallest denormal halved rounds to even: zero");
    assert_eq!(c.flags, fp::UFC | fp::IXC);
    let mut c = fp_rn();
    assert_eq!(fp::div(&mut c, 3u64, 2.0f64.to_bits()), 2, "three units halved rounds to even: two");
    assert_eq!(c.flags, fp::UFC | fp::IXC);
    let mut c = fp_rn();
    assert_eq!(fp::div(&mut c, one, 3.0f64.to_bits()), (1.0f64 / 3.0).to_bits());
    assert_eq!(c.flags, fp::IXC);
    let mut c = fp_rn();
    assert_eq!(fp::add(&mut c, one, one), 2.0f64.to_bits());
    assert_eq!(c.flags, 0, "an exact sum raises nothing");
    let mut c = fp::Ctx::from_fpscr(fp::FPSCR_FZ);
    assert_eq!(fp::add(&mut c, 1u64, zero), 0);
    assert_eq!(c.flags, fp::IDC);
    let mut c = fp::Ctx::from_fpscr(fp::FPSCR_FZ);
    assert_eq!(fp::mul(&mut c, f64::MIN_POSITIVE.to_bits(), 0.5f64.to_bits()), 0);
    assert_eq!(c.flags, fp::UFC);
    let mut c = fp::Ctx::from_fpscr(fp::FPSCR_FZ);
    assert_eq!(fp::add(&mut c, 0x7FF8_0000_0000_0001, 1u64), 0x7FF8_0000_0000_0001, "the quiet NaN beside a denormal");
    assert_eq!(c.flags, fp::IDC, "and the denormal's flush is flagged");
    let mut c = fp::Ctx::from_fpscr(fp::FPSCR_FZ);
    assert_eq!(fp::max_min_num(&mut c, 0x7FF0_0000_0000_0001, 1u64, true), 0x7FF8_0000_0000_0001);
    assert_eq!(c.flags, fp::IOC | fp::IDC, "a signalling NaN beside a denormal: invalid and the flush");
    let mut c = fp_rn();
    assert_eq!(fp::fma(&mut c, 0xFFF8_0000_0000_0000, zero, inf), 0x7FF8_0000_0000_0000, "a quiet-NaN addend beside infinity times zero: the default NaN, not the addend");
    assert_eq!(c.flags, fp::IOC);
    let mut c = fp::Ctx::from_fpscr(fp::FPSCR_FZ);
    assert_eq!(fp::from_f16(&mut c, 0x0001), 0x3E70_0000_0000_0000, "a denormal half converted exactly under FZ");
    assert_eq!(c.flags, 0);
    let mut c = fp::Ctx::from_fpscr(fp::FPSCR_FZ);
    assert_eq!(fp::to_f16(&mut c, 0x3CB0_0000_0000_0000), 0, "a double far below the half's range rounds to zero under FZ");
    assert_eq!(c.flags, fp::UFC | fp::IXC, "as a rounding, not a flush");
    let half_ulp_over_one = fp::add(&mut fp_rn(), one, 0x3CA0_0000_0000_0000);
    assert_eq!(half_ulp_over_one, one);
    let mut c = fp::Ctx::from_fpscr(1 << fp::FPSCR_RMODE_SHIFT);
    assert_eq!(fp::add(&mut c, one, 0x3CA0_0000_0000_0000), 0x3FF0_0000_0000_0001);
    let mut c = fp::Ctx::from_fpscr(2 << fp::FPSCR_RMODE_SHIFT);
    assert_eq!(fp::add(&mut c, one, 0x3CA0_0000_0000_0000), one);
    assert_eq!(fp::sub(&mut c, (-1.0f64).to_bits(), 0x3CA0_0000_0000_0000), 0xBFF0_0000_0000_0001);
    let mut c = fp::Ctx::from_fpscr(2 << fp::FPSCR_RMODE_SHIFT);
    assert_eq!(fp::sub(&mut c, one, one), 0x8000_0000_0000_0000, "an exact zero difference is -0 toward minus infinity");
    assert_eq!(fp::sub(&mut fp_rn(), one, one), 0, "and +0 otherwise");
    let mut c = fp::Ctx::from_fpscr(3 << fp::FPSCR_RMODE_SHIFT);
    assert_eq!(fp::div(&mut c, one, 3.0f64.to_bits()), (1.0f64 / 3.0).to_bits() - 0, "1/3 toward zero is the truncated value");
    let mut s = Sweep(0x0BAD_5EED_CAFE_F00D);
    for _ in 0..16384 {
        let (a, b) = (s.next_f64(), s.next_f64());
        let (x, y) = (f64::from_bits(a), f64::from_bits(b));
        if !x.is_finite() || !y.is_finite() {
            continue;
        }
        let rn = f64::from_bits(fp::add(&mut fp_rn(), a, b));
        let rp = f64::from_bits(fp::add(&mut fp::Ctx::from_fpscr(1 << fp::FPSCR_RMODE_SHIFT), a, b));
        let rm = f64::from_bits(fp::add(&mut fp::Ctx::from_fpscr(2 << fp::FPSCR_RMODE_SHIFT), a, b));
        let rz = f64::from_bits(fp::add(&mut fp::Ctx::from_fpscr(3 << fp::FPSCR_RMODE_SHIFT), a, b));
        if rn.is_finite() {
            assert!(rp >= rn && rm <= rn, "directed bounds {a:#x} + {b:#x}: rn {rn} rp {rp} rm {rm}");
            assert!(rz.abs() <= rn.abs() || rz == rn, "toward zero {a:#x} + {b:#x}");
            assert!(rp == rn || rp == rn.next_up(), "plus within one unit");
            assert!(rm == rn || rm == rn.next_down(), "minus within one unit");
        }
    }
}


/// Every form of the closed set as the pinned toolchain's assembler encodes
/// it, decoded to the form and the register numbers the source named; the
/// loads, stores, transfers and single-precision forms refused. The vectors
/// are a generated table, never typed.
#[test]
fn fpemul_decoder_matches_the_assembler() {
    use crate::fpemul::decode::{decode, name};
    use crate::fpemul_vectors::{DECODE_VECTORS, NOT_DECODE_VECTORS};
    assert!(DECODE_VECTORS.len() >= 200, "the table is short: {}", DECODE_VECTORS.len());
    for &(hw1, hw2, want, d, n, m, sd, sm) in DECODE_VECTORS {
        let dec = decode(hw1, hw2).unwrap_or_else(|| panic!("{want}: {hw1:04x} {hw2:04x} did not decode"));
        assert_eq!(name(dec.form), want, "{hw1:04x} {hw2:04x}");
        if d != 0xFF {
            assert_eq!(dec.d, d, "{want} {hw1:04x} {hw2:04x}: d");
        }
        if n != 0xFF {
            assert_eq!(dec.n, n, "{want} {hw1:04x} {hw2:04x}: n");
        }
        if m != 0xFF {
            assert_eq!(dec.m, m, "{want} {hw1:04x} {hw2:04x}: m");
        }
        if sd != 0xFF {
            assert_eq!(dec.sd, sd, "{want} {hw1:04x} {hw2:04x}: sd");
        }
        if sm != 0xFF {
            assert_eq!(dec.sm, sm, "{want} {hw1:04x} {hw2:04x}: sm");
        }
    }
    for &(hw1, hw2) in NOT_DECODE_VECTORS {
        assert!(decode(hw1, hw2).is_none(), "{hw1:04x} {hw2:04x} decoded, but it is not a double-precision data-processing form");
    }
    use crate::fpemul::decode::Form;
    let one = DECODE_VECTORS.iter().find(|v| v.2 == "vmov.f64 #imm").expect("an immediate vector");
    let dec = decode(one.0, one.1).expect("the immediate decodes");
    assert!(matches!(dec.form, Form::MovImm(v) if v == 1.0f64.to_bits() || v == (-1.0f64).to_bits() || v == 2.0f64.to_bits() || v == 1.75f64.to_bits() || v == 31.0f64.to_bits() || v == 0.125f64.to_bits() || v == (-0.5f64).to_bits() || v == 10.0f64.to_bits() || v == 3.5f64.to_bits() || v == (-31.0f64).to_bits()));
    let fx = DECODE_VECTORS.iter().filter(|v| v.2 == "vcvt.fixed.f64" || v.2 == "vcvt.f64.fixed").count();
    assert_eq!(fx, 32, "the fixed-point vectors");
    use crate::fpemul::decode::condition_passed;
    let judge: [fn(bool, bool, bool, bool) -> bool; 16] = [
        |_, z, _, _| z, |_, z, _, _| !z, |_, _, c, _| c, |_, _, c, _| !c,
        |n, _, _, _| n, |n, _, _, _| !n, |_, _, _, v| v, |_, _, _, v| !v,
        |_, z, c, _| c && !z, |_, z, c, _| !(c && !z), |n, _, _, v| n == v, |n, _, _, v| n != v,
        |n, z, _, v| !z && n == v, |n, z, _, v| !(!z && n == v), |_, _, _, _| true, |_, _, _, _| true,
    ];
    for cond in 0u8..16 {
        for flags in 0u32..16 {
            let (n, z, c, v) = (flags & 8 != 0, flags & 4 != 0, flags & 2 != 0, flags & 1 != 0);
            let xpsr = (flags << 28) | 0x0100_0000 | 0x0000_0200;
            assert_eq!(condition_passed(cond, xpsr), judge[cond as usize](n, z, c, v), "condition {cond:x} under flags {flags:x}");
        }
    }
}

#[test]
fn claims_board_fold_and_program_claims() {
    let bits = claims::fold_board(&[4, 9, 33]);
    assert_eq!(bits[0], (1 << 4) | (1 << 9));
    assert_eq!(bits[1], 1 << 1);
    let mut c = claims::EMPTY;
    claims::init(&mut c, 48, bits);
    assert_eq!(claims::claim(&mut c, 4), claims::REFUSED_BUSY, "a board line");
    assert_eq!(claims::claim(&mut c, 5), 0, "a free line");
    assert_eq!(claims::claim(&mut c, 5), claims::REFUSED_BUSY, "the line already held");
    assert!(claims::is_program(&c, 5) && !claims::is_board(&c, 5) && claims::is_board(&c, 33));
    assert_eq!(claims::claim(&mut c, 48), claims::REFUSED_INVAL, "past the count");
    assert_eq!(claims::release(&mut c, 6), claims::REFUSED_INVAL, "a line not held");
    assert_eq!(claims::release(&mut c, 5), 0);
    assert!(!claims::is_program(&c, 5));
    assert_eq!((c.claims_taken, c.releases, c.refused_busy, c.refused_inval, c.last_refused_line), (1, 1, 2, 2, 6));
}

#[test]
fn claims_irq_slots_and_tenants() {
    let mut c = claims::EMPTY;
    claims::init(&mut c, 176, [0; claims::BANKS]);
    assert_eq!(claims::irq_slot_claim(&mut c, 3, 3), 0, "PA3 takes slot 3");
    assert_eq!(claims::irq_slot_claim(&mut c, 3, 19), claims::REFUSED_BUSY, "PB3 refused on slot 3");
    assert_eq!(claims::irq_slot_claim(&mut c, 3, 3), 0, "PA3 re-arms its own slot");
    assert_eq!(claims::irq_slot_owner(&c, 3), Some(3));
    claims::irq_slot_release(&mut c, 3, 19);
    assert_eq!(claims::irq_slot_owner(&c, 3), Some(3), "a release by another line is ignored");
    claims::irq_slot_release(&mut c, 3, 3);
    assert_eq!(claims::irq_slot_owner(&c, 3), None);
    assert_eq!(claims::irq_slot_claim(&mut c, 32, 5), claims::REFUSED_INVAL, "a slot outside the table");
    assert_eq!(claims::tenant_add(&mut c, 0x4002_8254, 1, 0x2000_1000), 0, "the first tenant");
    assert_eq!(claims::tenant_add(&mut c, 0x4002_8254, 1, 0x2000_2000), claims::REFUSED_BUSY, "the same bit twice");
    assert_eq!(claims::tenant_for(&c, 0x4002_8254, 1).map(|t| t.wake), Some(0x2000_1000));
    assert_eq!(claims::tenant_for(&c, 0x4002_8254, 2), None);
    for b in 2..5 {
        assert_eq!(claims::tenant_add(&mut c, 0x4002_8254, b, 0x2000_1000 + b), 0);
    }
    assert_eq!(claims::tenant_add(&mut c, 0x4002_8254, 9, 0x2000_9000), claims::REFUSED_BUSY, "the table full");
}

#[test]
fn gpio_math_partition_wrap_and_families() {
    use gpio_math::KindClass;
    assert_eq!(gpio_math::kind_class(gpio_math::KIND_NONE), KindClass::None);
    assert_eq!(gpio_math::kind_class(gpio_math::KIND_RISING), KindClass::Edge { rising: true, falling: false });
    assert_eq!(gpio_math::kind_class(gpio_math::KIND_FALLING), KindClass::Edge { rising: false, falling: true });
    assert_eq!(gpio_math::kind_class(gpio_math::KIND_BOTH), KindClass::Edge { rising: true, falling: true });
    for k in gpio_math::KIND_LEVEL_LOW..=gpio_math::KIND_WAKE_BOTH {
        assert_eq!(gpio_math::kind_class(k), KindClass::Unsupported, "kind {k}");
    }
    assert_eq!(gpio_math::kind_class(9), KindClass::Invalid);
    assert!(gpio_math::mode_valid(2) && !gpio_math::mode_valid(3) && gpio_math::pull_valid(2) && !gpio_math::pull_valid(3));
    assert_eq!(gpio_math::edges_since(3, 0xFFFF_FFFE), 5, "the counter wraps");
    assert_eq!(gpio_math::edges_since(7, 7), 0);
    assert_eq!(gpio_math::st_split(110), (6, 14), "PG14");
    assert_eq!((gpio_math::st_f7_nvic(0), gpio_math::st_f7_nvic(4), gpio_math::st_f7_nvic(5), gpio_math::st_f7_nvic(9), gpio_math::st_f7_nvic(10), gpio_math::st_f7_nvic(15)), (6, 10, 23, 23, 40, 40));
    assert_eq!((gpio_math::st_u5_nvic(0), gpio_math::st_u5_nvic(13), gpio_math::st_u5_nvic(15)), (11, 24, 26));
    assert_eq!(gpio_math::rp_word(24), (3, 0), "GPIO 24: the fourth word, the lowest nibble");
    assert_eq!(gpio_math::rp_word(31), (3, 28));
    assert_eq!(gpio_math::rp_word(47), (5, 28));
}

#[test]
fn claims_board_claim_at_open() {
    let mut c = claims::EMPTY;
    claims::init(&mut c, 48, claims::fold_board(&[4]));
    assert_eq!(claims::claim(&mut c, 5), 0, "a program holds line 5");
    assert_eq!(claims::claim_board(&mut c, 5), claims::REFUSED_BUSY, "the board's claim on a program's line");
    assert_eq!(claims::claim_board(&mut c, 6), 0, "the board takes a free line");
    assert!(claims::is_board(&c, 6) && !claims::is_program(&c, 6));
    assert_eq!(claims::claim_board(&mut c, 6), 0, "the board's second claim is idempotent");
    assert_eq!(claims::claim_board(&mut c, 4), 0, "a line the board held at boot");
    assert_eq!(claims::claim(&mut c, 6), claims::REFUSED_BUSY, "a program's claim on the board's line after");
    assert_eq!(claims::claim_board(&mut c, 48), claims::REFUSED_INVAL, "past the count");
    assert_eq!((c.claims_taken, c.refused_busy, c.refused_inval, c.last_refused_line), (2, 2, 1, 48));
}

/// The I2C wire wait's budget: two bytes' wire time and the SMBus
/// specification's 25 ms of clock held low fit before the earliest tick at
/// which the wait can fail (its budget less one tick, the tick's phase), the
/// wait fails within its budget, and the budget with the timeout path's four
/// ticks stays inside the specification's 35 ms to a new start.
#[test]
fn bus_math_i2c_wait_covers_the_clock_low_limit() {
    let t = bus_math::i2c_wait_ticks(100_000, 1_000);
    assert_eq!(t, 27, "one tick of wire, twenty-five of clock held low, one for the tick's phase");
    assert_eq!((bus_math::I2C_CLOCK_LOW_MS, bus_math::I2C_CLOCK_HOLD_MS), (25, 35), "the specification's two figures");
    let earliest_us = (t as u64 - 1) * 1_000;
    assert!(earliest_us >= 2 * 9 * 10 + 25_000, "180 us of wire and 25 ms of clock held low end before the wait can fail");
    assert!(t + 4 <= bus_math::I2C_CLOCK_HOLD_MS, "the budget and the timeout path end inside 35 ms");
    assert!(t > bus_math::budget_ticks(1, 9, 100_000, 1_000), "longer than the budget it replaces");
    assert_eq!(bus_math::i2c_wait_ticks(100_000, 100), 1 + 3 + 1, "a 10 ms tick: every term rounds up");
    assert_eq!(bus_math::i2c_wait_ticks(10_000, 1_000), 2 + 25 + 1, "a slower wire: 1.8 ms of two bytes is two ticks");
}

#[test]
fn bus_math_budget_divisor_and_messages() {
    assert_eq!(bus_math::i2c_timing(16_000_000, 100_000), Some(0x3042_0F13));
    assert_eq!(bus_math::i2c_timing(16_000_000, 400_000), None);
    assert_eq!(bus_math::i2c_timing(54_000_000, 100_000), None);
    assert_eq!(bus_math::spi_divisor(16_000_000, 1_000_000), Some((3, 1_000_000)), "16 MHz over 16");
    assert_eq!(bus_math::spi_divisor(54_000_000, 1_000_000), Some((5, 843_750)), "54 MHz over 64: the largest divisor at or below the request");
    assert_eq!(bus_math::spi_divisor(16_000_000, 8_000_000), Some((0, 8_000_000)));
    assert_eq!(bus_math::spi_divisor(16_000_000, 62_500), Some((7, 62_500)), "the ladder's floor");
    assert_eq!(bus_math::spi_divisor(16_000_000, 62_499), None, "below the floor");
    assert_eq!(bus_math::spi_divisor(16_000_000, 0), None);
    assert_eq!(bus_math::budget_ticks(4096, 8, 1_000_000, 1000), 68, "32.768 ms of wire: 33 ticks, twice plus two");
    assert_eq!(bus_math::budget_ticks(1, 9, 100_000, 1000), 4, "90 us of wire: one tick, twice plus two");
    assert_eq!(bus_math::budget_ticks(0, 9, 100_000, 1000), 3, "never fewer than three");
    assert_eq!(bus_math::budget_ticks(65535, 8, 62_500, 1000), 2 * 8389 + 2, "the longest exchange at the slowest rate");
    assert_eq!(bus_math::budget_ticks(1, 8, 62_500, 1000), 4, "one byte at the slowest SPI rate: one tick, twice plus two");
    const TI: &[bus_math::I2cBus] = &[
        bus_math::I2cBus { number: 1, base: 0, enable_reg: 0, enable_bit: 0, mux_reg: 0, mux_shift: 0, mux_value: 0, kernel_hz: 16_000_000, pads: &[] },
        bus_math::I2cBus { number: 3, base: 0, enable_reg: 0, enable_bit: 0, mux_reg: 0, mux_shift: 0, mux_value: 0, kernel_hz: 16_000_000, pads: &[] },
    ];
    assert_eq!(bus_math::i2c_index(TI, 1), Some(0), "the bus named 1 is the first entry");
    assert_eq!(bus_math::i2c_index(TI, 3), Some(1), "the bus named 3 is the second entry");
    assert_eq!(bus_math::i2c_index(TI, 2), None, "no bus named 2: the door answers ENODEV");
    assert_eq!(bus_math::i2c_index(&[], 1), None, "an empty table names no bus");
    const TS: &[bus_math::SpiBus] = &[bus_math::SpiBus { number: 2, base: 0, file: bus_math::SpiFile::F7, enable_reg: 0, enable_bit: 0, mux_reg: 0, mux_shift: 0, mux_value: 0, kernel_hz: 16_000_000, pads: &[], selects: &[], select_count: 2, ahead: 2 }];
    assert_eq!(bus_math::spi_index(TS, 2), Some(0), "the bus named 2 is the entry");
    assert_eq!(bus_math::spi_index(TS, 0), None, "no bus named 0");
    assert!(bus_math::i2c_numbers_unique(TI) && bus_math::spi_numbers_unique(TS), "distinct numbers are sound");
    const TI_TWICE: &[bus_math::I2cBus] = &[TI[0], TI[0]];
    assert!(!bus_math::i2c_numbers_unique(TI_TWICE), "one number on two buses is refused");
    const TS_TWICE: &[bus_math::SpiBus] = &[TS[0], TS[0]];
    assert!(!bus_math::spi_numbers_unique(TS_TWICE), "one number on two buses is refused");
    assert!(bus_math::spi_selects_sound(TS), "a model's empty select list beside a count is sound");
    const TS_MANY: &[bus_math::SpiBus] = &[bus_math::SpiBus { select_count: 9, ..TS[0] }];
    assert!(!bus_math::spi_selects_sound(TS_MANY), "more than eight selects is refused");
    const TS_SHORT: &[bus_math::SpiBus] = &[bus_math::SpiBus { selects: &[5], ..TS[0] }];
    assert!(!bus_math::spi_selects_sound(TS_SHORT), "a select list shorter than its count is refused");
    let mut b = [0u8; 4];
    let ok = [bus_math::I2cMsg { addr: 0x76, flags: 0, len: 1, reserved: 0, buf: b.as_mut_ptr() }];
    assert_eq!(bus_math::i2c_msgs_check(&ok), 0);
    assert_eq!(bus_math::i2c_wire_bytes(&ok), 2, "one data byte and one address byte");
    let probe = [bus_math::I2cMsg { addr: 0x77, flags: 0, len: 0, reserved: 0, buf: core::ptr::null_mut() }];
    assert_eq!(bus_math::i2c_msgs_check(&probe), 0, "a NULL buffer with no length is a probe");
    for bad in [
        bus_math::I2cMsg { addr: 0x76, flags: 2, len: 1, reserved: 0, buf: b.as_mut_ptr() },
        bus_math::I2cMsg { addr: 0x76, flags: 0, len: 1, reserved: 1, buf: b.as_mut_ptr() },
        bus_math::I2cMsg { addr: 0x76, flags: 0, len: 256, reserved: 0, buf: b.as_mut_ptr() },
        bus_math::I2cMsg { addr: 0x80, flags: 0, len: 1, reserved: 0, buf: b.as_mut_ptr() },
        bus_math::I2cMsg { addr: 0x76, flags: 0, len: 1, reserved: 0, buf: core::ptr::null_mut() },
    ] {
        assert_eq!(bus_math::i2c_msgs_check(&[bad]), -22, "a bad record refused EINVAL");
    }
    assert_eq!(bus_math::i2c_msgs_check(&[]), -22, "no message");
    assert_eq!(bus_math::i2c_msgs_check(&[ok[0]; 9]), -22, "nine messages");
    assert_eq!(bus_math::i2c_msgs_check(&[ok[0]; 8]), 0, "eight messages");
    assert_eq!(bus_math::select_class(0, 2), bus_math::SelectClass::Index(0));
    assert_eq!(bus_math::select_class(1, 2), bus_math::SelectClass::Index(1));
    assert_eq!(bus_math::select_class(2, 2), bus_math::SelectClass::Invalid);
    assert_eq!(bus_math::select_class(bus_math::SELECT_NONE, 2), bus_math::SelectClass::None);
    assert_eq!(bus_math::mode_bits(0), Some((false, false)));
    assert_eq!(bus_math::mode_bits(1), Some((false, true)));
    assert_eq!(bus_math::mode_bits(2), Some((true, false)));
    assert_eq!(bus_math::mode_bits(3), Some((true, true)));
    assert_eq!(bus_math::mode_bits(4), None);
}

#[test]
fn pwm_math_quantization_and_shared_period() {
    const T: pwm_math::PwmTimer = pwm_math::PwmTimer { number: 1, clock_hz: 1_000_000, channels: 2, prescaler_max: 65536, period_max: 65536 };
    assert_eq!(pwm_math::quantize(1_000_000, &T), Some((1, 1000)), "a millisecond at 1 MHz: exact");
    assert_eq!(pwm_math::quantize(70_000_000, &T), Some((2, 35000)), "70 ms: past the period register, the prescaler 2, exact");
    assert_eq!(pwm_math::quantize(1234, &T), Some((1, 1)), "1234 ns rounds to one tick");
    assert_eq!(pwm_math::quantize(500, &T), Some((1, 1)), "half a tick rounds up");
    assert_eq!(pwm_math::quantize(499, &T), None, "under half a tick: no tick");
    assert_eq!(pwm_math::quantize(0, &T), None);
    assert_eq!(pwm_math::quantize(100_000_000_123, &T), Some((1526, 65531)), "100 s: the smallest prescaler that fits, the period rounded");
    assert_eq!(pwm_math::quantize(4_294_967_296_000, &T), Some((65536, 65536)), "the range's ceiling, exact");
    assert_eq!(pwm_math::quantize(4_294_967_297_000, &T), None, "past the range");
    assert_eq!(pwm_math::quantize(65_537_000_000, &T), Some((1001, 65472)));
    assert_eq!(pwm_math::achieved_ns(1, 1000, 1_000_000), 1_000_000);
    assert_eq!(pwm_math::achieved_ns(1526, 65531, 1_000_000), 100_000_306_000);
    assert_eq!(pwm_math::achieved_ns(1001, 65472, 1_000_000), 65_537_472_000);
    assert_eq!(pwm_math::achieved_ns(65536, 65536, 1_000_000), 4_294_967_296_000);
    assert_eq!(pwm_math::achieved_ns(3, 7, 54_000_000), 389, "21 ticks of 54 MHz: 388.9 ns rounded");
    assert_eq!(pwm_math::compare(100_000, 1_000_000, 1, 1000), 100, "10 percent of a millisecond");
    assert_eq!(pwm_math::compare(50_000_000_000, 1_000_000, 1526, 65531), 32765, "half of 100 s over the prescaler, rounded");
    assert_eq!(pwm_math::compare(2_000_000, 1_000_000, 1, 1000), 1000, "a duty past the period is the whole period");
    assert_eq!(pwm_math::compare(0, 1_000_000, 1, 1000), 0);
    assert!(pwm_math::polarity_valid(0) && pwm_math::polarity_valid(1) && !pwm_math::polarity_valid(2));
    assert!(pwm_math::same_pair((1526, 65531), (1526, 65531)) && !pwm_math::same_pair((1, 1000), (2, 500)));
    const L: &[pwm_math::PwmLine] = &[
        pwm_math::PwmLine { line: 6, timer: 0, channel: 0, alternate: 0 },
        pwm_math::PwmLine { line: 7, timer: 0, channel: 1, alternate: 0 },
        pwm_math::PwmLine { line: 8, timer: 1, channel: 0, alternate: 0 },
    ];
    const TT: &[pwm_math::PwmTimer] = &[T, pwm_math::PwmTimer { number: 2, ..T }];
    assert_eq!(pwm_math::line_index(L, 8), Some(2));
    assert_eq!(pwm_math::line_index(L, 5), None, "no timer drives line 5: the claim answers EINVAL");
    assert_eq!(pwm_math::line_index(&[], 6), None);
    assert_eq!(pwm_math::channel_bit(1, 1), 9);
    assert!(pwm_math::table_sound(L, TT), "the emulator shape is sound");
    assert!(pwm_math::table_sound(&[], &[]), "empty tables are sound");
    const L_TWICE: &[pwm_math::PwmLine] = &[L[0], pwm_math::PwmLine { line: 9, timer: 0, channel: 0, alternate: 0 }];
    assert!(pwm_math::table_sound(L_TWICE, TT), "two lines on one channel are sound (one output at two pads; the claims table keeps the channel exclusive)");
    for n in 0..48usize {
        let twin = if n < 16 { n + 16 } else if n < 32 { n - 16 } else if n < 40 { n + 8 } else { n - 8 };
        assert!(rp2350_map::slice_of(n) == rp2350_map::slice_of(twin) && rp2350_map::output_of(n) == rp2350_map::output_of(twin), "the RP2350 mapping: lines {} and {} reach one slice output", n, twin);
        assert_eq!(rp2350_map::twin_of(n, 48), Some(twin), "the RP2350 mapping: the twin of line {} on the 48-line package", n);
        assert_eq!(rp2350_map::twin_of(n, 30), if twin < 30 { Some(twin) } else { None }, "the RP2350 mapping: the twin of line {} on the 30-line package", n);
    }
    assert_eq!((rp2350_map::slice_of(0), rp2350_map::output_of(0), rp2350_map::slice_of(2), rp2350_map::slice_of(15), rp2350_map::output_of(15)), (0, 0, 1, 7, 1), "the RP2350 mapping: the datasheet's first rows");
    assert_eq!((rp2350_map::slice_of(29), rp2350_map::slice_of(32), rp2350_map::slice_of(40), rp2350_map::slice_of(47), rp2350_map::output_of(47)), (6, 8, 8, 11, 1), "the RP2350 mapping: the datasheet's later rows");
    const L_SAME: &[pwm_math::PwmLine] = &[L[0], pwm_math::PwmLine { line: 6, timer: 1, channel: 1, alternate: 0 }];
    assert!(!pwm_math::table_sound(L_SAME, TT), "two entries for one line are refused");
    const L_CH: &[pwm_math::PwmLine] = &[pwm_math::PwmLine { line: 6, timer: 0, channel: 2, alternate: 0 }];
    assert!(!pwm_math::table_sound(L_CH, TT), "a channel past the timer's count is refused");
    const L_T: &[pwm_math::PwmLine] = &[pwm_math::PwmLine { line: 6, timer: 2, channel: 0, alternate: 0 }];
    assert!(!pwm_math::table_sound(L_T, TT), "a timer past the table is refused");
    const T_WIDE: &[pwm_math::PwmTimer] = &[pwm_math::PwmTimer { channels: 9, ..T }];
    assert!(!pwm_math::table_sound(&[], T_WIDE), "nine channels on a timer are refused");
    const T_MANY: &[pwm_math::PwmTimer] = &[T; 17];
    assert!(!pwm_math::table_sound(&[], T_MANY), "seventeen timers are refused");
}

#[test]
fn adc_math_budget_mask_and_channels() {
    assert_eq!((adc_math::full_scale(12), adc_math::full_scale(14), adc_math::full_scale(16), adc_math::full_scale(1)), (4095, 16383, 65535, 1));
    assert_eq!(adc_math::conversion_ns(15, 4_000_000), 3750, "fifteen cycles at 4 MHz");
    assert_eq!(adc_math::conversion_ns(17, 4_000_000), 4250);
    assert_eq!(adc_math::conversion_ns(1, 3), 333_333_334, "rounded up");
    assert_eq!(adc_math::budget_ticks(3750, 1000), 4, "3.75 us of conversion: one tick, twice plus two");
    assert_eq!(adc_math::budget_ticks(0, 1000), 3, "never fewer than three");
    assert_eq!(adc_math::budget_ticks(2_500_000, 1000), 8, "2.5 ms: three ticks, twice plus two");
    assert_eq!((adc_math::loop_sample(100, 1000, 16383), adc_math::loop_sample(500, 1000, 16383), adc_math::loop_sample(900, 1000, 16383)), (1638, 8192, 14745));
    assert_eq!((adc_math::loop_sample(1000, 1000, 16383), adc_math::loop_sample(0, 1000, 4095)), (16383, 0));
    assert!(adc_math::within_band(1640, 1638, 327) && !adc_math::within_band(0, 1638, 327));
    assert!(adc_math::within_band(1638, 1640, 2) && !adc_math::within_band(1638, 1641, 2));
    const B: &[adc_math::AdcBlock] = &[
        adc_math::AdcBlock { number: 1, clock_hz: 4_000_000, bits: 12, reference_mv: 3300, conversion_cycles: 15 },
        adc_math::AdcBlock { number: 3, clock_hz: 4_000_000, bits: 14, reference_mv: 3000, conversion_cycles: 17 },
    ];
    assert_eq!(adc_math::block_index(B, 3), Some(1), "the block named 3 is the second entry");
    assert_eq!(adc_math::block_index(B, 2), None, "no block named 2: the door answers ENODEV");
    assert_eq!(adc_math::block_index(&[], 1), None);
    assert_eq!(adc_math::channel_bit(1, 1), 33);
    const C: &[adc_math::AdcChannel] = &[
        adc_math::AdcChannel { block: 0, channel: 0, line: 10 },
        adc_math::AdcChannel { block: 0, channel: 1, line: 11 },
        adc_math::AdcChannel { block: 1, channel: 0, line: adc_math::NO_LINE },
        adc_math::AdcChannel { block: 1, channel: 1, line: adc_math::NO_LINE },
    ];
    assert!(adc_math::table_sound(C, B), "the emulator shape is sound; two internal channels share no line");
    assert!(adc_math::table_sound(&[], &[]), "empty tables are sound");
    const C_BLOCK: &[adc_math::AdcChannel] = &[adc_math::AdcChannel { block: 2, channel: 0, line: 10 }];
    assert!(!adc_math::table_sound(C_BLOCK, B), "a block past the table is refused");
    const C_TWICE: &[adc_math::AdcChannel] = &[C[0], adc_math::AdcChannel { block: 0, channel: 0, line: 12 }];
    assert!(!adc_math::table_sound(C_TWICE, B), "two entries for one channel are refused");
    const C_LINE: &[adc_math::AdcChannel] = &[C[0], adc_math::AdcChannel { block: 1, channel: 5, line: 10 }];
    assert!(!adc_math::table_sound(C_LINE, B), "two channels reading one line are refused");
    const C_FAR: &[adc_math::AdcChannel] = &[adc_math::AdcChannel { block: 0, channel: 32, line: 10 }];
    assert!(!adc_math::table_sound(C_FAR, B), "a channel past the bitmap's reach is refused");
    const B_WIDE: &[adc_math::AdcBlock] = &[adc_math::AdcBlock { bits: 17, ..B[0] }];
    assert!(!adc_math::table_sound(&[], B_WIDE), "seventeen bits are refused");
    const B_STILL: &[adc_math::AdcBlock] = &[adc_math::AdcBlock { conversion_cycles: 0, ..B[0] }];
    assert!(!adc_math::table_sound(&[], B_STILL), "a block without a conversion cycle is refused");
    const B_MANY: &[adc_math::AdcBlock] = &[B[0]; 5];
    assert!(!adc_math::table_sound(&[], B_MANY), "five blocks are refused");
}

#[test]
fn rp2350_bus_math_counts_and_ladder() {
    use crate::rp2350_bus_math as rp;
    let c12 = rp::i2c_counts(12_000_000, 100_000).expect("the counts at 12 MHz");
    assert_eq!((c12.lcnt, c12.hcnt, c12.spklen, c12.hold), (56, 55, 1, 4));
    let c150 = rp::i2c_counts(150_000_000, 100_000).expect("the counts at 150 MHz");
    assert_eq!((c150.lcnt, c150.hcnt, c150.spklen, c150.hold), (704, 781, 7, 45));
    for (clk, c) in [(12_000_000u32, c12), (150_000_000u32, c150)] {
        assert!(c.lcnt > c.spklen + 7, "the low count against the spike length at {clk}");
        assert!(c.hcnt > c.spklen + 5, "the high count against the spike length at {clk}");
        let low_ns = (c.lcnt as u64 + 1) * 1_000_000_000 / clk as u64;
        let high_ns = (c.hcnt as u64 + c.spklen as u64 + 7) * 1_000_000_000 / clk as u64;
        assert!(low_ns >= 4_700, "the low period {low_ns} ns at {clk}");
        assert!(high_ns >= 4_000, "the high period {high_ns} ns at {clk}");
        assert_eq!((c.lcnt + 1) + (c.hcnt + c.spklen + 7), clk / 100_000, "the bit time is the rate's period exactly");
        assert!(c.hold >= 1 && c.hold + 2 <= c.lcnt + 1, "the data hold inside the low period");
        assert!(rp::i2c_counts_sound(&c, clk));
    }
    assert_eq!(rp::i2c_counts(12_000_000, 50_000), None, "a rate outside the set is refused");
    assert_eq!(rp::i2c_counts(1_000_000, 100_000), None, "too slow a clock makes no counts");
    assert!(!rp::i2c_counts_sound(&rp::I2cCounts { lcnt: 8, ..c12 }, 12_000_000), "a low count at the spike length plus 7 is refused");
    assert!(!rp::i2c_counts_sound(&rp::I2cCounts { hold: 56, ..c12 }, 12_000_000), "a hold past the low period less two is refused");
    assert_eq!(rp::spi_pair(12_000_000, 1_000_000), Some((2 | (5 << 8), 1_000_000)));
    assert_eq!(rp::spi_pair(12_000_000, 500_000).map(|p| p.1), Some(500_000));
    assert_eq!(rp::spi_pair(12_000_000, 6_000_000), Some((2, 6_000_000)), "the top of the ladder");
    assert_eq!(rp::spi_pair(12_000_000, 50_000_000), Some((2, 6_000_000)), "a request over the top is served at the top");
    assert_eq!(rp::spi_pair(12_000_000, 184).map(|p| p.1), Some(184), "the ladder's floor");
    assert_eq!(rp::spi_pair(12_000_000, 183), None, "under the floor");
    assert_eq!(rp::spi_pair(12_000_000, 0), None);
    for rate in [184u32, 185, 200, 999, 1_000, 9_600, 100_000, 400_000, 843_750, 1_000_000, 1_000_001, 2_999_999, 3_000_000, 5_999_999, 6_000_000, 7_000_000, u32::MAX] {
        let mut best: Option<u32> = None;
        for c in (2u32..=254).step_by(2) {
            for s in 1u32..=256 {
                let r = 12_000_000 / (c * s);
                if r <= rate && best.is_none_or(|b| r > b) {
                    best = Some(r);
                }
            }
        }
        let got = rp::spi_pair(12_000_000, rate);
        assert_eq!(got.map(|p| p.1), best, "the highest rate at or below {rate}");
        if let Some((pair, achieved)) = got {
            let (c, s) = (pair & 0xFF, (pair >> 8) + 1);
            assert!((2..=254).contains(&c) && c % 2 == 0 && (1..=256).contains(&s), "the pair {c} x {s} for {rate}");
            assert_eq!(12_000_000 / (c * s), achieved);
            assert_eq!(rp::spi_pair_rate(12_000_000, pair), achieved);
        }
    }
}

#[test]
fn rp2350_bus_math_rows_roles_and_aborts() {
    use crate::bus_math::{I2cMsg, Pad, PadRole, MSG_READ};
    use crate::rp2350_bus_math as rp;
    let i2c = rp::i2c_buses(150_000_000);
    let spi = rp::spi_buses();
    assert!(rp::rows_sound(&i2c, &spi));
    assert_eq!((i2c.len(), spi.len()), (1, 1));
    assert_eq!((i2c[0].number, i2c[0].base, i2c[0].enable_reg, i2c[0].enable_bit, i2c[0].kernel_hz), (0, 0x4009_0000, 0x4002_0000, 4, 150_000_000));
    assert_eq!((spi[0].number, spi[0].base, spi[0].enable_bit, spi[0].kernel_hz, spi[0].select_count, spi[0].ahead), (0, 0x4008_0000, 18, 12_000_000, 1, 4));
    assert_eq!(spi[0].file, bus_math::SpiFile::Pl022);
    let lines = |pads: &[Pad]| pads.iter().map(|p| p.line).collect::<Vec<_>>();
    assert_eq!(lines(i2c[0].pads), [4, 5]);
    assert_eq!(lines(spi[0].pads), [18, 19, 16]);
    assert_eq!(spi[0].selects, [17]);
    assert!(bus_math::i2c_numbers_unique(&i2c) && bus_math::spi_numbers_unique(&spi) && bus_math::spi_selects_sound(&spi));
    const I2C1: [Pad; 2] = [Pad { line: 6, alternate: 3, role: PadRole::I2cData }, Pad { line: 7, alternate: 3, role: PadRole::I2cClock }];
    const SPI1: [Pad; 3] =
        [Pad { line: 10, alternate: 1, role: PadRole::SpiClock }, Pad { line: 11, alternate: 1, role: PadRole::SpiOut }, Pad { line: 12, alternate: 1, role: PadRole::SpiIn }];
    let second_i2c = rp::i2c_bus(1, 12_000_000, &I2C1);
    let second_spi = rp::spi_bus(1, &SPI1, &[], 0);
    assert_eq!((second_i2c.number, second_i2c.base, second_i2c.enable_bit), (1, 0x4009_8000, 5));
    assert_eq!((second_spi.number, second_spi.base, second_spi.enable_bit), (1, 0x4008_8000, 19));
    assert!(rp::rows_sound(&[second_i2c], &[second_spi]));
    assert!(!rp::rows_sound(&[rp::i2c_bus(0, 12_000_000, &I2C1)], &[]));
    assert!(!rp::rows_sound(&[], &[rp::spi_bus(0, &SPI1, &[], 0)]));
    assert_eq!((rp::i2c_block_of(4), rp::i2c_role_of(4)), (0, PadRole::I2cData));
    assert_eq!((rp::i2c_block_of(5), rp::i2c_role_of(5)), (0, PadRole::I2cClock));
    assert_eq!((rp::i2c_block_of(15), rp::i2c_role_of(15)), (1, PadRole::I2cClock));
    assert_eq!((rp::i2c_block_of(26), rp::i2c_role_of(26)), (1, PadRole::I2cData));
    assert_eq!((rp::spi_block_of(16), rp::spi_role_of(16)), (0, Some(PadRole::SpiIn)));
    assert_eq!((rp::spi_block_of(10), rp::spi_role_of(10)), (1, Some(PadRole::SpiClock)));
    assert_eq!((rp::spi_block_of(19), rp::spi_role_of(19)), (0, Some(PadRole::SpiOut)));
    assert_eq!((rp::spi_block_of(9), rp::spi_role_of(9)), (1, None), "the block's own frame signal is no pad of a bus row");
    assert_eq!((rp::spi_block_of(35), rp::spi_block_of(44)), (0, 1));
    assert_eq!(rp::abort_class(rp::ABRT_7B_ADDR_NOACK), rp::Abort::Address);
    assert_eq!(rp::abort_class(rp::ABRT_TXDATA_NOACK), rp::Abort::Data);
    assert_eq!(rp::abort_class(rp::ABRT_ARB_LOST), rp::Abort::Arbitration);
    assert_eq!(rp::abort_class(rp::ABRT_USER_ABRT), rp::Abort::Other);
    assert_eq!(rp::abort_class((3 << rp::TX_FLUSH_SHIFT) | rp::ABRT_TXDATA_NOACK), rp::Abort::Data);
    assert_eq!(rp::abort_class(3 << rp::TX_FLUSH_SHIFT), rp::Abort::Other);
    assert_eq!((rp::last_taken(2, 0), rp::last_taken(2, 1), rp::last_taken(2, 2), rp::last_taken(0, 0)), (1, 0, 0, 0));
    let mut a = [0u8; 1];
    let mut b = [0u8; 3];
    let msg = |addr: u16, flags: u16, buf: &mut [u8]| I2cMsg { addr, flags, len: buf.len() as u16, reserved: 0, buf: buf.as_mut_ptr() };
    let msgs = [msg(0x76, 0, &mut a), msg(0x76, MSG_READ, &mut b)];
    assert_eq!((rp::locate(&msgs, 0), rp::locate(&msgs, 1), rp::locate(&msgs, 3), rp::locate(&msgs, 9)), ((0, 0), (1, 0), (1, 2), (1, 0)));
    assert_eq!(rp::i2c_refused_message(&msgs), None);
    let two = [msg(0x76, 0, &mut a), msg(0x77, MSG_READ, &mut b)];
    assert_eq!(rp::i2c_refused_message(&two), Some(1), "a second address");
    let mut none = [0u8; 0];
    let empty = [msg(0x76, 0, &mut none)];
    assert_eq!(rp::i2c_refused_message(&empty), Some(0), "a message of length 0");
}

#[test]
fn rp2350_bus_math_i2c_modes() {
    use crate::bus_math;
    use crate::rp2350_bus_math as rp;
    for (i, m) in rp::I2C_MODES.iter().enumerate() {
        assert_eq!(m.rate_hz, bus_math::I2C_RATES_HZ[i]);
        assert_eq!(bus_math::i2c_mode_index(m.rate_hz), Some(i as u8));
        assert_eq!(rp::i2c_mode(m.rate_hz), Some(i));
    }
    assert_eq!(bus_math::i2c_mode_index(50_000), None);
    assert_eq!(rp::i2c_mode(50_000), None);
    let want = [
        (12_000_000u32, 100_000u32, (56u32, 55u32, 1u32, 4u32), 0u8, 1u32, 100_000u32),
        (12_000_000, 400_000, (15, 7, 1, 4), 1, 2, 387_096),
        (150_000_000, 100_000, (704, 781, 7, 45), 0, 1, 100_000),
        (150_000_000, 400_000, (194, 166, 7, 45), 1, 2, 400_000),
        (150_000_000, 1_000_000, (74, 61, 7, 45), 2, 2, 1_000_000),
    ];
    for (clk, rate, counts, mode, speed, achieved) in want {
        let c = rp::i2c_counts(clk, rate).unwrap_or_else(|| panic!("the counts for {rate} at {clk}"));
        assert_eq!((c.lcnt, c.hcnt, c.spklen, c.hold), counts, "the counts for {rate} at {clk}");
        assert_eq!((c.mode, c.speed, c.achieved_hz), (mode, speed, achieved), "the mode and the achieved rate for {rate} at {clk}");
        assert!(rp::i2c_counts_sound(&c, clk), "sound at {rate}, {clk}");
        let m = &rp::I2C_MODES[c.mode as usize];
        assert!((c.lcnt as u64 + 1) * 1_000_000_000 >= m.low_ns * clk as u64, "the low time at {rate}, {clk}");
        assert!((c.hcnt as u64 + c.spklen as u64 + 7) * 1_000_000_000 >= m.high_ns * clk as u64, "the high time at {rate}, {clk}");
        assert!(c.achieved_hz <= rate, "the achieved rate at or below the request at {rate}, {clk}");
        assert_eq!(c.achieved_hz, clk / ((c.lcnt + 1) + (c.hcnt + c.spklen + 7)), "the achieved rate is the clock over the period");
    }
    assert_eq!(rp::i2c_counts(12_000_000, 1_000_000), None, "1 MHz under the block's 32 MHz floor");
    assert_eq!(rp::i2c_counts(150_000_000, 50_000), None, "a rate outside the set at the PLL posture");
    let c = rp::i2c_counts(12_000_000, 400_000).expect("the fast counts at 12 MHz");
    assert!(!rp::i2c_counts_sound(&rp::I2cCounts { hcnt: 6, ..c }, 12_000_000), "a high count at the spike length plus 5 is refused");
    assert!(!rp::i2c_counts_sound(&rp::I2cCounts { speed: 1, ..c }, 12_000_000), "the fast counts under the standard SPEED field are refused");
    assert!(!rp::i2c_counts_sound(&rp::I2cCounts { mode: 3, ..c }, 12_000_000), "a mode outside the set is refused");
    assert!(!rp::i2c_counts_sound(&c, 11_000_000), "the fast counts under the mode's floor are refused");
}

#[test]
fn rp2350_bus_math_reserved_addresses() {
    use crate::bus_math::{I2cMsg, MSG_READ};
    use crate::rp2350_bus_math as rp;
    for addr in 0u32..=0x7F {
        assert_eq!(rp::i2c_reserved(addr), !(0x08..=0x77).contains(&addr), "address {addr:#x}");
    }
    let mut a = [0u8; 1];
    let mut b = [0u8; 3];
    let msg = |addr: u16, flags: u16, buf: &mut [u8]| I2cMsg { addr, flags, len: buf.len() as u16, reserved: 0, buf: buf.as_mut_ptr() };
    for addr in [0x03u16, 0x78] {
        assert_eq!(rp::i2c_refused_message(&[msg(addr, 0, &mut a), msg(addr, MSG_READ, &mut b)]), Some(0), "address {addr:#x}");
    }
    for addr in [0x08u16, 0x77] {
        assert_eq!(rp::i2c_refused_message(&[msg(addr, 0, &mut a), msg(addr, MSG_READ, &mut b)]), None, "address {addr:#x}");
    }
    assert_eq!(rp::i2c_refused_message(&[msg(0x76, 0, &mut a), msg(0x77, MSG_READ, &mut b)]), Some(1), "a second address");
    let mut none = [0u8; 0];
    assert_eq!(rp::i2c_refused_message(&[msg(0x76, 0, &mut none)]), Some(0), "a message of length 0");
}
