//! The host tier's cases. Each case that touches the arena or the heap
//! boots them first (the arena's init resets its window and the heap's
//! init re-carves, so the cases are independent in any order); the wake
//! cases are pure arithmetic.

use crate::{abi_window, adc_math, arena, board, bus_math, calendar_math, claims, gpio_math, kheap, net_math, pwm_math, rp2350_map, uart_math, wake};

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
    let target = now + wake::WINDOW + 2; // 2^31 + 1 ticks ahead: past the signed window
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

// ---- the calendar arithmetic: the pure half of the calendar, compiled
// from its own file.

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
    // The Unix epoch is day 719162 after the year-1 epoch, a Thursday.
    assert_eq!(calendar_math::days_from_civil(1970, 1, 1), 719_162);
    assert_eq!(calendar_math::weekday_from_days(719_162), 4);
    // The year-1 epoch is a Monday; the century base a Saturday.
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
    // Every day of a leap year round-trips (2000 is a leap year; 1900 is not).
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
    // a window of one generation runs that generation alone
    assert_eq!(judge(true, 32, 32, 32), Verdict::ProvidedInside(32));
    assert_eq!(judge(true, 31, 32, 32), Verdict::RefusedOutside(31));
}

// ---- the double-precision emulation's arithmetic: the soft binary64 against the host's IEEE double ----

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
            0 => r,                                                   // any bit pattern (NaNs, infinities, denormals included)
            1 => (r & 0x800F_FFFF_FFFF_FFFF) | ((1000 + (r >> 4) % 48) << 52), // exponents near 1
            2 => (r & 0x800F_FFFF_FFFF_FFFF) | (((r >> 4) % 2047) << 52),      // any finite exponent
            3 => r & 0x800F_FFFF_FFFF_FFFF,                              // denormals and zeros
            _ => ((r >> 12) as f64 * (1.0 / 4096.0)).to_bits() ^ (r & (1 << 63)), // small exact values
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
        0x8000_0000_0000_0000,       // -0
        0x7FF0_0000_0000_0000,       // +inf
        0xFFF0_0000_0000_0000,       // -inf
        0x7FF8_0000_0000_0000,       // the default quiet NaN
        0x7FF8_0000_0000_0001,       // a quiet NaN with a payload
        0xFFF8_0000_0000_0002,       // a negative quiet NaN
        0x7FF0_0000_0000_0001,       // a signalling NaN
        0x7FF4_0000_0000_0000,       // a signalling NaN with the top payload bit
        0x0000_0000_0000_0001,       // the smallest denormal
        0x8000_0000_0000_0001,
        0x000F_FFFF_FFFF_FFFF,       // the largest denormal
        0x0010_0000_0000_0000,       // the smallest normal
        0x7FEF_FFFF_FFFF_FFFF,       // the largest finite
        0x3FF0_0000_0000_0001,       // 1 + ulp
        0x3FEF_FFFF_FFFF_FFFF,       // 1 - ulp/2
        0x4340_0000_0000_0001,       // 2^53 + 2
        0x41E0_0000_0000_0000,       // 2^31
        0x41EF_FFFF_FFC0_0000,       // 2^32 - 1
        0xC1E0_0000_0020_0000,       // -(2^31 + 1)
    ]);
    t
}

fn fp_same(a: u64, b: u64) -> bool {
    let (x, y) = (f64::from_bits(a), f64::from_bits(b));
    if x.is_nan() && y.is_nan() {
        return true; // the host's NaN sign and payload conventions are not the architecture's
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
    // a product that cancels the addend exactly only when fused
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
        // to single, round to nearest
        let f32_soft = fp::to_f32(&mut fp_rn(), a);
        let f32_host = (x as f32).to_bits();
        assert!(
            (f32::from_bits(f32_soft).is_nan() && f32::from_bits(f32_host).is_nan()) || f32_soft == f32_host,
            "to_f32({a:#x}) = {f32_soft:#x} vs {f32_host:#x}"
        );
        // from single (the same value back through the single)
        let back = fp::from_f32(&mut fp_rn(), f32_host);
        assert!(fp_same(back, (f32::from_bits(f32_host) as f64).to_bits()), "from_f32({f32_host:#x})");
        // to integers toward zero: the host's `as` saturates and maps NaN to 0, as the architecture does
        assert_eq!(fp::to_int32(&mut fp_rn(), a, true, fp::Round::Zero) as i32, x as i32, "to_int32 signed {a:#x}");
        assert_eq!(fp::to_int32(&mut fp_rn(), a, false, fp::Round::Zero), x as u32, "to_int32 unsigned {a:#x}");
        // from integers
        let i = a as u32;
        assert_eq!(fp::from_int32(&mut fp_rn(), i, true), (i as i32 as f64).to_bits());
        assert_eq!(fp::from_int32(&mut fp_rn(), i, false), (i as f64).to_bits());
        // round to integral
        if !x.is_nan() {
            assert_eq!(fp::round_to_integral(&mut fp_rn(), a, fp::Round::Zero, false), x.trunc().to_bits(), "vrintz {a:#x}");
            assert_eq!(fp::round_to_integral(&mut fp_rn(), a, fp::Round::Plus, false), x.ceil().to_bits(), "vrintp {a:#x}");
            assert_eq!(fp::round_to_integral(&mut fp_rn(), a, fp::Round::Minus, false), x.floor().to_bits(), "vrintm {a:#x}");
            assert_eq!(fp::round_to_integral(&mut fp_rn(), a, fp::Round::Away, false), x.round().to_bits(), "vrinta {a:#x}");
            assert_eq!(fp::round_to_integral(&mut fp_rn(), a, fp::Round::Nearest, false), x.round_ties_even().to_bits(), "vrintn {a:#x}");
        }
    }
    // half precision, both formats, a few exact and rounded values
    for (h, want) in [(0x3C00u32, 1.0f64), (0xC000, -2.0), (0x7BFF, 65504.0), (0x0001, 5.960464477539063e-8), (0x0400, 6.103515625e-5)] {
        assert_eq!(fp::from_f16(&mut fp_rn(), h), want.to_bits(), "from_f16 {h:#x}");
        assert_eq!(fp::to_f16(&mut fp_rn(), want.to_bits()), h, "to_f16 {want}");
    }
    assert_eq!(fp::to_f16(&mut fp_rn(), 70000.0f64.to_bits()), 0x7C00, "overflow to +inf in the IEEE half");
    let mut ahp = fp::Ctx::from_fpscr(fp::FPSCR_AHP);
    // the alternative format: exponent field 31 is 2^16; 70000 = 65536 * (1 + 69.7/1024) rounds to 70/1024 -> 0x7C46
    assert_eq!(fp::to_f16(&mut ahp, 70000.0f64.to_bits()), 0x7C46, "the alternative format reaches 70000");
    assert_eq!(fp::from_f16(&mut fp::Ctx::from_fpscr(fp::FPSCR_AHP), 0x7C46), 70016.0f64.to_bits());
    let mut ahp2 = fp::Ctx::from_fpscr(fp::FPSCR_AHP);
    assert_eq!(fp::to_f16(&mut ahp2, 200000.0f64.to_bits()), 0x7FFF, "beyond the alternative format's largest value: saturated");
    assert_eq!(ahp2.flags, fp::IOC, "an overflow in the alternative format: invalid alone, as the reference row raises it");
    let mut ahp3 = fp::Ctx::from_fpscr(fp::FPSCR_AHP);
    assert_eq!(fp::to_f16(&mut ahp3, 131072.0f64.to_bits()), 0x7FFF, "an exact power of two beyond the format: saturated");
    assert_eq!(ahp3.flags, fp::IOC, "and invalid alone for an exact power of two as well");
    // fixed point
    assert_eq!(fp::to_fixed(&mut fp_rn(), 1.5f64.to_bits(), true, 32, 4), 24);
    assert_eq!(fp::to_fixed(&mut fp_rn(), (-1.5f64).to_bits(), true, 16, 8), (-384i32) as u32 & 0xFFFF);
    assert_eq!(fp::from_fixed(&mut fp_rn(), 24, true, 32, 4), 1.5f64.to_bits());
    assert_eq!(fp::from_fixed(&mut fp_rn(), 0xFE80, true, 16, 8), (-1.5f64).to_bits());
    // the immediate
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
    // the quiet NaN yields the number; +0 is greater than -0
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
    // division by zero
    let mut c = fp_rn();
    assert_eq!(fp::div(&mut c, one, zero), inf);
    assert_eq!(c.flags, fp::DZC);
    // invalid: inf - inf, 0 * inf, sqrt(-1), a signalling NaN operand
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
    // overflow, underflow, inexact
    let mut c = fp_rn();
    assert_eq!(fp::mul(&mut c, f64::MAX.to_bits(), 2.0f64.to_bits()), inf);
    assert_eq!(c.flags, fp::OFC | fp::IXC);
    let mut c = fp::Ctx::from_fpscr(3 << fp::FPSCR_RMODE_SHIFT); // toward zero
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
    // flush-to-zero: a denormal input flushed with IDC; a tiny result flushed with UFC
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
    // directed rounding on halfway and near-halfway cases
    let half_ulp_over_one = fp::add(&mut fp_rn(), one, 0x3CA0_0000_0000_0000); // 1 + 2^-53: exactly halfway, ties to even -> 1
    assert_eq!(half_ulp_over_one, one);
    let mut c = fp::Ctx::from_fpscr(1 << fp::FPSCR_RMODE_SHIFT); // plus infinity
    assert_eq!(fp::add(&mut c, one, 0x3CA0_0000_0000_0000), 0x3FF0_0000_0000_0001);
    let mut c = fp::Ctx::from_fpscr(2 << fp::FPSCR_RMODE_SHIFT); // minus infinity
    assert_eq!(fp::add(&mut c, one, 0x3CA0_0000_0000_0000), one);
    assert_eq!(fp::sub(&mut c, (-1.0f64).to_bits(), 0x3CA0_0000_0000_0000), 0xBFF0_0000_0000_0001);
    let mut c = fp::Ctx::from_fpscr(2 << fp::FPSCR_RMODE_SHIFT);
    assert_eq!(fp::sub(&mut c, one, one), 0x8000_0000_0000_0000, "an exact zero difference is -0 toward minus infinity");
    assert_eq!(fp::sub(&mut fp_rn(), one, one), 0, "and +0 otherwise");
    let mut c = fp::Ctx::from_fpscr(3 << fp::FPSCR_RMODE_SHIFT);
    assert_eq!(fp::div(&mut c, one, 3.0f64.to_bits()), (1.0f64 / 3.0).to_bits() - 0, "1/3 toward zero is the truncated value");
    // the sweep property: every directed result lies within one unit of the nearest result on its side
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

// ---- the double-precision decoder against the assembler's encodings ----

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
    // the immediate's value and the fixed-point widths, from the encodings the assembler gave
    use crate::fpemul::decode::Form;
    let one = DECODE_VECTORS.iter().find(|v| v.2 == "vmov.f64 #imm").expect("an immediate vector");
    let dec = decode(one.0, one.1).expect("the immediate decodes");
    assert!(matches!(dec.form, Form::MovImm(v) if v == 1.0f64.to_bits() || v == (-1.0f64).to_bits() || v == 2.0f64.to_bits() || v == 1.75f64.to_bits() || v == 31.0f64.to_bits() || v == 0.125f64.to_bits() || v == (-0.5f64).to_bits() || v == 10.0f64.to_bits() || v == 3.5f64.to_bits() || v == (-31.0f64).to_bits()));
    let fx = DECODE_VECTORS.iter().filter(|v| v.2 == "vcvt.fixed.f64" || v.2 == "vcvt.f64.fixed").count();
    assert_eq!(fx, 32, "the fixed-point vectors");
    // the condition judge against a reference table, every condition over every flag pattern
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
    assert_eq!(bus_math::i2c_timing(64_000_000, 100_000), Some(0xF042_0F13), "the H7's 64 MHz kernel clock: prescaler 15 is the 16 MHz row's 250 ns tick");
    assert_eq!(bus_math::i2c_timing(64_000_000, 400_000), None);
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
    // The I2C block's counts at both clock postures, as the datasheet's rules make them.
    let c12 = rp::i2c_counts(12_000_000, 100_000).expect("the counts at 12 MHz");
    assert_eq!((c12.lcnt, c12.hcnt, c12.spklen, c12.hold), (56, 55, 1, 4));
    let c150 = rp::i2c_counts(150_000_000, 100_000).expect("the counts at 150 MHz");
    assert_eq!((c150.lcnt, c150.hcnt, c150.spklen, c150.hold), (704, 781, 7, 45));
    for (clk, c) in [(12_000_000u32, c12), (150_000_000u32, c150)] {
        // The four inequalities: the two of the block, the two minimum periods.
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
    // The SPI ladder on the crystal.
    assert_eq!(rp::spi_pair(12_000_000, 1_000_000), Some((2 | (5 << 8), 1_000_000)));
    assert_eq!(rp::spi_pair(12_000_000, 500_000).map(|p| p.1), Some(500_000));
    assert_eq!(rp::spi_pair(12_000_000, 6_000_000), Some((2, 6_000_000)), "the top of the ladder");
    assert_eq!(rp::spi_pair(12_000_000, 50_000_000), Some((2, 6_000_000)), "a request over the top is served at the top");
    assert_eq!(rp::spi_pair(12_000_000, 184).map(|p| p.1), Some(184), "the ladder's floor");
    assert_eq!(rp::spi_pair(12_000_000, 183), None, "under the floor");
    assert_eq!(rp::spi_pair(12_000_000, 0), None);
    // Against a brute force over every pair the block can be programmed with.
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
    // The rows every board of the chip shares: the first block of each family, on the
    // pins the board maker's datasheet marks as that block's default.
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
    // The second block of each family: the constructors serve it, no row carries it.
    const I2C1: [Pad; 2] = [Pad { line: 6, alternate: 3, role: PadRole::I2cData }, Pad { line: 7, alternate: 3, role: PadRole::I2cClock }];
    const SPI1: [Pad; 3] =
        [Pad { line: 10, alternate: 1, role: PadRole::SpiClock }, Pad { line: 11, alternate: 1, role: PadRole::SpiOut }, Pad { line: 12, alternate: 1, role: PadRole::SpiIn }];
    let second_i2c = rp::i2c_bus(1, 12_000_000, &I2C1);
    let second_spi = rp::spi_bus(1, &SPI1, &[], 0);
    assert_eq!((second_i2c.number, second_i2c.base, second_i2c.enable_bit), (1, 0x4009_8000, 5));
    assert_eq!((second_spi.number, second_spi.base, second_spi.enable_bit), (1, 0x4008_8000, 19));
    assert!(rp::rows_sound(&[second_i2c], &[second_spi]));
    // A row whose pad sits on the other block's line is refused.
    assert!(!rp::rows_sound(&[rp::i2c_bus(0, 12_000_000, &I2C1)], &[]));
    assert!(!rp::rows_sound(&[], &[rp::spi_bus(0, &SPI1, &[], 0)]));
    // The function table's rule, on rows read off the datasheet's page.
    assert_eq!((rp::i2c_block_of(4), rp::i2c_role_of(4)), (0, PadRole::I2cData));
    assert_eq!((rp::i2c_block_of(5), rp::i2c_role_of(5)), (0, PadRole::I2cClock));
    assert_eq!((rp::i2c_block_of(15), rp::i2c_role_of(15)), (1, PadRole::I2cClock));
    assert_eq!((rp::i2c_block_of(26), rp::i2c_role_of(26)), (1, PadRole::I2cData));
    assert_eq!((rp::spi_block_of(16), rp::spi_role_of(16)), (0, Some(PadRole::SpiIn)));
    assert_eq!((rp::spi_block_of(10), rp::spi_role_of(10)), (1, Some(PadRole::SpiClock)));
    assert_eq!((rp::spi_block_of(19), rp::spi_role_of(19)), (0, Some(PadRole::SpiOut)));
    assert_eq!((rp::spi_block_of(9), rp::spi_role_of(9)), (1, None), "the block's own frame signal is no pad of a bus row");
    assert_eq!((rp::spi_block_of(35), rp::spi_block_of(44)), (0, 1));
    // The abort's classes; the flushed count is no source bit.
    assert_eq!(rp::abort_class(rp::ABRT_7B_ADDR_NOACK), rp::Abort::Address);
    assert_eq!(rp::abort_class(rp::ABRT_TXDATA_NOACK), rp::Abort::Data);
    assert_eq!(rp::abort_class(rp::ABRT_ARB_LOST), rp::Abort::Arbitration);
    assert_eq!(rp::abort_class(rp::ABRT_USER_ABRT), rp::Abort::Other);
    assert_eq!(rp::abort_class((3 << rp::TX_FLUSH_SHIFT) | rp::ABRT_TXDATA_NOACK), rp::Abort::Data);
    assert_eq!(rp::abort_class(3 << rp::TX_FLUSH_SHIFT), rp::Abort::Other);
    // The command the block took last, and the message and byte it names.
    assert_eq!((rp::last_taken(2, 0), rp::last_taken(2, 1), rp::last_taken(2, 2), rp::last_taken(0, 0)), (1, 0, 0, 0));
    let mut a = [0u8; 1];
    let mut b = [0u8; 3];
    let msg = |addr: u16, flags: u16, buf: &mut [u8]| I2cMsg { addr, flags, len: buf.len() as u16, reserved: 0, buf: buf.as_mut_ptr() };
    let msgs = [msg(0x76, 0, &mut a), msg(0x76, MSG_READ, &mut b)];
    assert_eq!((rp::locate(&msgs, 0), rp::locate(&msgs, 1), rp::locate(&msgs, 3), rp::locate(&msgs, 9)), ((0, 0), (1, 0), (1, 2), (1, 0)));
    // The family's own partition: one target a transaction, no address-only frame.
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
    // The three modes against the bus doors' rate set, in its order.
    for (i, m) in rp::I2C_MODES.iter().enumerate() {
        assert_eq!(m.rate_hz, bus_math::I2C_RATES_HZ[i]);
        assert_eq!(bus_math::i2c_mode_index(m.rate_hz), Some(i as u8));
        assert_eq!(rp::i2c_mode(m.rate_hz), Some(i));
    }
    assert_eq!(bus_math::i2c_mode_index(50_000), None);
    assert_eq!(rp::i2c_mode(50_000), None);
    // The six (clock, rate) pairs: the counts, the mode in each answer, the achieved
    // rate (the RP2350 datasheet's section 12.2.14.1 read strictly: 400 kHz at 12 MHz is 387,096 Hz), and the two Nones (the floor; outside the set).
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
    // The strict reading: the table's fast-mode row at 12 MHz (a high count of 6) breaks the block's rule.
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
    // The sixteen addresses the block reserves: under 0x08 and over 0x77.
    for addr in 0u32..=0x7F {
        assert_eq!(rp::i2c_reserved(addr), !(0x08..=0x77).contains(&addr), "address {addr:#x}");
    }
    let mut a = [0u8; 1];
    let mut b = [0u8; 3];
    let msg = |addr: u16, flags: u16, buf: &mut [u8]| I2cMsg { addr, flags, len: buf.len() as u16, reserved: 0, buf: buf.as_mut_ptr() };
    // The partition names the first message at a reserved address and serves the two
    // addresses at the edges of the served range.
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

// ---------------- the card layer over a scripted host ----------------

use crate::sdcard::{self, Card, Step, Stuck, Why};
use crate::sdhost::{Rate, Refusal, Reply, Response, SdHost, Width};

/// A host played from a transcript: every command the card layer sends is
/// matched against the next scripted (index, argument) pair and answered
/// with the scripted result; a command the script did not expect fails
/// the case with both named. Data transfers are recorded; a queued refusal
/// is answered first.
struct Script {
    steps: Vec<(u8, u32, Result<Reply, Refusal>)>,
    next: usize,
    bus: Vec<(Rate, Width)>,
    present: Option<bool>,
    pauses: u32,
    data: Vec<(u8, u32, u32)>,
    refuse: Vec<Refusal>,
}

impl Script {
    fn new(steps: Vec<(u8, u32, Result<Reply, Refusal>)>) -> Script {
        Script { steps, next: 0, bus: Vec::new(), present: Some(true), pauses: 0, data: Vec::new(), refuse: Vec::new() }
    }
}

impl SdHost for Script {
    fn attach(&mut self) -> Result<(), Refusal> {
        Ok(())
    }
    fn card_present(&mut self) -> Option<bool> {
        self.present
    }
    fn set_bus(&mut self, rate: Rate, width: Width) -> Result<(), Refusal> {
        self.bus.push((rate, width));
        Ok(())
    }
    fn command(&mut self, index: u8, arg: u32, _response: Response) -> Result<Reply, Refusal> {
        let (i, a, r) = *self.steps.get(self.next).unwrap_or_else(|| panic!("the script ended before CMD{index} {arg:#010x}"));
        assert!(i == index && a == arg, "the script expected CMD{i} {a:#010x} at step {}, the card layer sent CMD{index} {arg:#010x}", self.next);
        self.next += 1;
        r
    }
    fn read_blocks(&mut self, index: u8, arg: u32, blocks: u32, buf: &mut [u8]) -> Result<(), Refusal> {
        self.data.push((index, arg, blocks));
        if !self.refuse.is_empty() {
            return Err(self.refuse.remove(0));
        }
        for (k, b) in buf.iter_mut().enumerate() {
            *b = (arg as u8).wrapping_add(k as u8);
        }
        Ok(())
    }
    fn write_blocks(&mut self, index: u8, arg: u32, blocks: u32, _data: &[u8]) -> Result<(), Refusal> {
        self.data.push((index, arg, blocks));
        if !self.refuse.is_empty() {
            return Err(self.refuse.remove(0));
        }
        Ok(())
    }
    fn pause(&mut self, _micros: u32) {
        self.pauses += 1;
    }
}

/// A version 2.0 CSD: C_SIZE [69:48] = 0x3B37, so (0x3B37 + 1) x 1024 blocks (a 7.9 GB card); the write-protect bits clear.
const CSD_V2: [u32; 4] = [0x4000_0000, 0x0000_0000, 0x3B37_0000, 0x0000_0000];
/// A version 1.0 CSD: READ_BL_LEN [83:80] = 9, C_SIZE [73:62] = 4095, C_SIZE_MULT [49:47] = 7: 4096 x 2^18 bytes = 1 GB.
const CSD_V1: [u32; 4] = [0x0000_0000, 0x0009_03FF, 0xC003_8000, 0x0000_0000];
/// A CID: the maker 0x03, the product "SD8GB" in [103:64], the serial 0x1234_5678 in [55:24].
const CID: [u32; 4] = [0x0353_4453, 0x4438_4742, 0x1012_3456, 0x7800_0000];
const RCA: u32 = 0x1234_0000;
const STATUS_TRAN_READY: u32 = (4 << 9) | (1 << 8);
const STATUS_STBY_READY: u32 = (3 << 9) | (1 << 8);
const STATUS_PRG: u32 = 7 << 9;

fn hc_steps(csd: [u32; 4]) -> Vec<(u8, u32, Result<Reply, Refusal>)> {
    vec![
        (0, 0, Ok(Reply::None)),
        (8, 0x1AA, Ok(Reply::Short(0x1AA))),
        (55, 0, Ok(Reply::Short(0x120))),
        (41, 0x4030_0000, Ok(Reply::Short(0x00FF_8000))),
        (55, 0, Ok(Reply::Short(0x120))),
        (41, 0x4030_0000, Ok(Reply::Short(0xC0FF_8000))),
        (2, 0, Ok(Reply::Long(CID))),
        (3, 0, Ok(Reply::Short(RCA | 0x0500))),
        (9, RCA, Ok(Reply::Long(csd))),
        (7, RCA, Ok(Reply::Short(STATUS_STBY_READY))),
        (55, RCA, Ok(Reply::Short(STATUS_TRAN_READY | 0x20))),
        (6, 2, Ok(Reply::Short(STATUS_TRAN_READY))),
        (16, 512, Ok(Reply::Short(STATUS_TRAN_READY))),
    ]
}

fn card(high_capacity: bool, write_protected: bool) -> Card {
    Card { version2: true, high_capacity, capacity_blocks: 1000, rca: 0x1234, cid: [0; 4], csd: [0; 4], write_protected, width: Width::Four }
}

#[test]
fn sdcard_crc7_and_crc16_match_the_fact_sheets_frames() {
    // The last byte of the driver's CMD0 frame is 0x95 and of its CMD8 frame 0x87.
    assert_eq!(sdcard::frame_crc_byte(0, 0), 0x95);
    assert_eq!(sdcard::frame_crc_byte(8, 0x1AA), 0x87);
    // The CCITT check value of "123456789" with the initial value 0 (the XMODEM form).
    assert_eq!(sdcard::crc16(b"123456789"), 0x31C3);
    assert_eq!(sdcard::crc16(&[0u8; 512]), 0);
    // The field reader on the long response's word order.
    assert_eq!(sdcard::field(&CID, 127, 120), 0x03);
    assert_eq!(&sdcard::cid_product(&CID), b"SD8GB");
    assert_eq!(sdcard::cid_serial(&CID), 0x1234_5678);
    assert_eq!(sdcard::cid_manufacturer(&CID), 0x03);
}

#[test]
fn sdcard_csd_decoders_and_both_capacity_formulas() {
    let v2 = sdcard::decode_csd(&CSD_V2).unwrap();
    assert_eq!((v2.capacity_blocks, v2.write_protected, v2.version2_csd), (15_523_840, false, true));
    let v1 = sdcard::decode_csd(&CSD_V1).unwrap();
    assert_eq!((v1.capacity_blocks, v1.write_protected, v1.version2_csd), (2_097_152, false, false));
    let mut wp = CSD_V2;
    wp[3] |= 1 << 12;
    assert!(sdcard::decode_csd(&wp).unwrap().write_protected);
    let mut v3 = CSD_V2;
    v3[0] = 0xC000_0000;
    assert_eq!(sdcard::decode_csd(&v3), Err(Why::Geometry));
    let mut bad_len = CSD_V1;
    bad_len[1] = 0x000C_03FF;
    assert_eq!(sdcard::decode_csd(&bad_len), Err(Why::Geometry));
}

#[test]
fn sdcard_identify_high_capacity_over_the_scripted_host() {
    let mut h = Script::new(hc_steps(CSD_V2));
    let c = sdcard::identify(&mut h).unwrap();
    assert!(c.version2 && c.high_capacity && !c.write_protected);
    assert_eq!((c.capacity_blocks, c.rca, c.width), (15_523_840, 0x1234, Width::Four));
    assert_eq!(c.cid, CID);
    assert_eq!(h.bus, vec![(Rate::Identification, Width::One), (Rate::Default, Width::Four)]);
    assert_eq!((h.pauses, h.next), (1, h.steps.len()));
}

#[test]
fn sdcard_identify_standard_capacity_over_the_scripted_host() {
    // A version 1 card: no answer to CMD8, ACMD41 without HCS, no CCS, a version 1.0 CSD, four lines refused.
    let steps = vec![
        (0, 0, Ok(Reply::None)),
        (8, 0x1AA, Err(Refusal::Timeout)),
        (55, 0, Ok(Reply::Short(0x120))),
        (41, 0x0030_0000, Ok(Reply::Short(0x80FF_8000))),
        (2, 0, Ok(Reply::Long(CID))),
        (3, 0, Ok(Reply::Short(0x0000_0500))),
        (3, 0, Ok(Reply::Short(RCA | 0x0500))),
        (9, RCA, Ok(Reply::Long(CSD_V1))),
        (7, RCA, Ok(Reply::Short(STATUS_STBY_READY))),
        (55, RCA, Ok(Reply::Short(STATUS_TRAN_READY | 0x20))),
        (6, 2, Err(Refusal::Timeout)),
        (16, 512, Ok(Reply::Short(STATUS_TRAN_READY))),
    ];
    let mut h = Script::new(steps);
    let c = sdcard::identify(&mut h).unwrap();
    assert!(!c.version2 && !c.high_capacity);
    assert_eq!((c.capacity_blocks, c.rca, c.width), (2_097_152, 0x1234, Width::One));
    assert_eq!(h.bus, vec![(Rate::Identification, Width::One), (Rate::Default, Width::One)]);
    assert_eq!((h.pauses, h.next), (0, h.steps.len()));
}

#[test]
fn sdcard_identify_refuses_the_doctored_scripts() {
    // (a) A card that never leaves busy: refused at ACMD41 after the one-second budget, one pause per poll but the last.
    let mut busy = vec![(0, 0, Ok(Reply::None)), (8, 0x1AA, Ok(Reply::Short(0x1AA)))];
    for _ in 0..sdcard::ACMD41_TRIES {
        busy.push((55, 0, Ok(Reply::Short(0x120))));
        busy.push((41, 0x4030_0000, Ok(Reply::Short(0x00FF_8000))));
    }
    let mut h = Script::new(busy);
    assert_eq!(sdcard::identify(&mut h), Err(Stuck { step: Step::Acmd41, why: Why::Busy }));
    assert_eq!(h.pauses, sdcard::ACMD41_TRIES - 1);
    // (b) A CRC refusal on CMD9, the step named.
    let mut steps = hc_steps(CSD_V2);
    steps[8].2 = Err(Refusal::Crc);
    assert_eq!(sdcard::identify(&mut Script::new(steps)), Err(Stuck { step: Step::Cmd9, why: Why::Refused(Refusal::Crc) }));
    // (c) A CSD with an impossible geometry (a structure version this layer does not serve).
    let mut v3 = CSD_V2;
    v3[0] = 0xC000_0000;
    assert_eq!(sdcard::identify(&mut Script::new(hc_steps(v3))), Err(Stuck { step: Step::Csd, why: Why::Geometry }));
    // (d) CMD8's echo wrong.
    let mut steps = hc_steps(CSD_V2);
    steps[1].2 = Ok(Reply::Short(0x155));
    assert_eq!(sdcard::identify(&mut Script::new(steps)), Err(Stuck { step: Step::Cmd8, why: Why::Echo }));
    // (e) No card in the socket.
    let mut h = Script::new(hc_steps(CSD_V2));
    h.present = Some(false);
    assert_eq!(sdcard::identify(&mut h), Err(Stuck { step: Step::Attach, why: Why::NoCard }));
    let mut steps = hc_steps(CSD_V2);
    steps[5].2 = Ok(Reply::Short(0x80FF_8000));
    assert_eq!(sdcard::identify(&mut Script::new(steps)), Err(Stuck { step: Step::Csd, why: Why::Geometry }));
    // (g) No answer to CMD55: an MMC or a dead card.
    let mut steps = hc_steps(CSD_V2);
    steps[2].2 = Err(Refusal::Timeout);
    assert_eq!(sdcard::identify(&mut Script::new(steps)), Err(Stuck { step: Step::Acmd41, why: Why::Unsupported }));
}

#[test]
fn sdcard_read_and_write_address_by_capacity_class() {
    let mut buf = [0u8; 4 * 512];
    // High capacity: the block number; CMD17 for one block, CMD18 for more.
    let mut h = Script::new(vec![]);
    sdcard::read(&mut h, &card(true, false), 100, 1, &mut buf).unwrap();
    sdcard::read(&mut h, &card(true, false), 100, 4, &mut buf).unwrap();
    assert_eq!(h.data, vec![(17, 100, 1), (18, 100, 4)]);
    assert_eq!(buf[0], 100);
    // Standard capacity: the byte address.
    let mut h = Script::new(vec![]);
    sdcard::read(&mut h, &card(false, false), 100, 1, &mut buf).unwrap();
    assert_eq!(h.data, vec![(17, 51_200, 1)]);
    // The arguments refused: a zero count, a range past the end, a short buffer.
    let mut h = Script::new(vec![]);
    assert_eq!(sdcard::read(&mut h, &card(true, false), 0, 0, &mut buf), Err(Stuck { step: Step::Transfer, why: Why::Argument }));
    assert_eq!(sdcard::read(&mut h, &card(true, false), 999, 2, &mut buf), Err(Stuck { step: Step::Transfer, why: Why::Argument }));
    assert_eq!(sdcard::read(&mut h, &card(true, false), 0, 8, &mut buf), Err(Stuck { step: Step::Transfer, why: Why::Argument }));
    assert!(h.data.is_empty());
    // One retry on a CRC refusal, after the recovery (CMD12, then CMD13 to the transfer state);
    // a second refusal is final, after a second recovery.
    let rec = |n: usize| (0..n).flat_map(|_| [(12, 0, Ok(Reply::Short(STATUS_TRAN_READY))), (13, RCA, Ok(Reply::Short(STATUS_TRAN_READY)))]).collect::<Vec<_>>();
    let mut h = Script::new(rec(1));
    h.refuse = vec![Refusal::Crc];
    sdcard::read(&mut h, &card(true, false), 7, 1, &mut buf).unwrap();
    assert_eq!((h.data.len(), h.next), (2, 2));
    let mut h = Script::new(rec(2));
    h.refuse = vec![Refusal::Crc, Refusal::Crc];
    assert_eq!(sdcard::read(&mut h, &card(true, false), 7, 1, &mut buf), Err(Stuck { step: Step::Transfer, why: Why::Refused(Refusal::Crc) }));
    assert_eq!((h.data.len(), h.next), (2, 4));
    // A write: CMD24, then CMD13 polled through the programming state to the transfer state.
    let mut h = Script::new(vec![(13, RCA, Ok(Reply::Short(STATUS_PRG))), (13, RCA, Ok(Reply::Short(STATUS_TRAN_READY)))]);
    sdcard::write(&mut h, &card(true, false), 5, 1, &buf).unwrap();
    assert_eq!((h.data.clone(), h.pauses), (vec![(24, 5, 1)], 1));
    // A multiple-block write on a standard-capacity card: CMD25 at the byte address.
    let mut h = Script::new(vec![(13, RCA, Ok(Reply::Short(STATUS_TRAN_READY)))]);
    sdcard::write(&mut h, &card(false, false), 2, 3, &buf).unwrap();
    assert_eq!(h.data, vec![(25, 1024, 3)]);
    // A write-protected card refuses before any command; a status with the violation bit refuses ReadOnly.
    let mut h = Script::new(vec![]);
    assert_eq!(sdcard::write(&mut h, &card(true, true), 5, 1, &buf), Err(Stuck { step: Step::Transfer, why: Why::ReadOnly }));
    let mut h = Script::new(vec![(13, RCA, Ok(Reply::Short(STATUS_TRAN_READY | sdcard::STATUS_WP_VIOLATION)))]);
    assert_eq!(sdcard::write(&mut h, &card(true, false), 5, 1, &buf), Err(Stuck { step: Step::Cmd13, why: Why::ReadOnly }));
    // The programming state never left: refused Busy at the per-block budget.
    let mut steps = Vec::new();
    for _ in 0..sdcard::PROGRAM_TRIES_PER_BLOCK {
        steps.push((13, RCA, Ok(Reply::Short(STATUS_PRG))));
    }
    let mut h = Script::new(steps);
    assert_eq!(sdcard::write(&mut h, &card(true, false), 5, 1, &buf), Err(Stuck { step: Step::Cmd13, why: Why::Busy }));
}

// The checksum matcher against the vendored stack's own parser.
use smoltcp::wire::{checksum, Icmpv4Packet, IpAddress, IpProtocol, Ipv4Address, Ipv4Packet, TcpPacket, UdpPacket};

const NET_SRC: [u8; 4] = [192, 0, 2, 1];
const NET_DST: [u8; 4] = [192, 0, 2, 2];

fn net_addresses() -> (IpAddress, IpAddress) {
    (IpAddress::Ipv4(Ipv4Address::new(192, 0, 2, 1)), IpAddress::Ipv4(Ipv4Address::new(192, 0, 2, 2)))
}

#[test]
fn net_math_rfc1071_sum_matches_the_parsers() {
    // RFC 1071 section 3's worked example: the words 0001 f203 f4f5 f6f7 sum to ddf2; the checksum is 220d.
    let rfc = [0x00, 0x01, 0xf2, 0x03, 0xf4, 0xf5, 0xf6, 0xf7];
    assert_eq!(net_math::ones_complement_sum(0, &rfc), 0xddf2);
    assert_eq!(net_math::checksum(&rfc), 0x220d);
    // The stack's own sum (not complemented) agrees on every shape: even, odd, carry-heavy, empty, a single byte.
    let shapes: [&[u8]; 6] = [&rfc, &rfc[..7], &[0xff; 33], &[], &[0x80], &[0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc, 0xde, 0xf0, 0x01]];
    for data in shapes {
        assert_eq!(net_math::ones_complement_sum(0, data), checksum::data(data), "the sum over {} bytes", data.len());
    }
    // A sum taken in two parts on a word boundary is the sum over the whole.
    assert_eq!(net_math::ones_complement_sum(net_math::ones_complement_sum(0, &rfc[..4]), &rfc[4..]), 0xddf2);
    // The pseudo-header: the stack's words for the same addresses, protocol and length.
    let (src, dst) = (Ipv4Address::new(192, 0, 2, 1), Ipv4Address::new(192, 0, 2, 2));
    assert_eq!(
        net_math::pseudo_header_v4(NET_SRC, NET_DST, net_math::PROTOCOL_UDP, 11),
        checksum::pseudo_header_v4(&src, &dst, IpProtocol::Udp, 11)
    );
    assert_eq!(
        net_math::pseudo_header_v4(NET_SRC, NET_DST, net_math::PROTOCOL_TCP, 0xffff),
        checksum::pseudo_header_v4(&src, &dst, IpProtocol::Tcp, 0xffff)
    );
}

#[test]
fn net_math_ipv4_header_verified_by_the_parser() {
    // A header without options: version 4, 20 bytes, total length 20, identification 0x1c46, DF, TTL 64, TCP, the two addresses.
    let mut header = [0x45, 0x00, 0x00, 0x14, 0x1c, 0x46, 0x40, 0x00, 0x40, 0x06, 0, 0, 192, 0, 2, 1, 192, 0, 2, 2];
    let sum = net_math::ipv4_header_checksum(&header);
    header[10..12].copy_from_slice(&sum.to_be_bytes());
    // The field in place, a receiver's sum over the whole header is all ones: its complement is zero.
    assert_eq!(net_math::checksum(&header), 0);
    let packet = Ipv4Packet::new_checked(&header[..]).expect("a well-formed header");
    assert!(packet.verify_checksum(), "the stack verifies the matcher's header checksum");
    let mut theirs = header;
    theirs[10] = 0;
    theirs[11] = 0;
    Ipv4Packet::new_unchecked(&mut theirs[..]).fill_checksum();
    assert_eq!(theirs, header, "the stack's fill writes the matcher's word");
    // The control: the TTL doctored, the field stale -- refused.
    let mut doctored = header;
    doctored[8] = 0x3f;
    assert!(!Ipv4Packet::new_checked(&doctored[..]).expect("still well-formed").verify_checksum(), "a doctored header is refused");
}

#[test]
fn net_math_udp_pseudo_header_verified_by_the_parser() {
    // The DHCP port pair over a 3-byte payload: an odd length, so the padding rule is in the sum.
    let mut datagram = [0x00, 0x44, 0x00, 0x43, 0x00, 0x0b, 0, 0, b'r', b'h', b'u'];
    let sum = net_math::udp_checksum_v4(NET_SRC, NET_DST, &datagram);
    datagram[6..8].copy_from_slice(&sum.to_be_bytes());
    let (src, dst) = net_addresses();
    let packet = UdpPacket::new_checked(&datagram[..]).expect("a well-formed datagram");
    assert!(packet.verify_checksum(&src, &dst), "the stack verifies the matcher's UDP checksum");
    let mut theirs = datagram;
    theirs[6] = 0;
    theirs[7] = 0;
    UdpPacket::new_unchecked(&mut theirs[..]).fill_checksum(&src, &dst);
    assert_eq!(theirs, datagram, "the stack's fill writes the matcher's word");
    let mut doctored = datagram;
    doctored[10] ^= 0x01;
    assert!(!UdpPacket::new_checked(&doctored[..]).expect("still well-formed").verify_checksum(&src, &dst), "a doctored payload is refused");
}

#[test]
fn net_math_tcp_pseudo_header_verified_by_the_parser() {
    // A SYN from port 49152 to port 80: sequence 1, no acknowledgement, a 20-byte header, window 1024, no urgent pointer.
    let mut segment = [0xc0, 0x00, 0x00, 0x50, 0, 0, 0, 1, 0, 0, 0, 0, 0x50, 0x02, 0x04, 0x00, 0, 0, 0, 0];
    let sum = net_math::tcp_checksum_v4(NET_SRC, NET_DST, &segment);
    segment[16..18].copy_from_slice(&sum.to_be_bytes());
    let (src, dst) = net_addresses();
    let packet = TcpPacket::new_checked(&segment[..]).expect("a well-formed segment");
    assert!(packet.verify_checksum(&src, &dst), "the stack verifies the matcher's TCP checksum");
    let mut theirs = segment;
    theirs[16] = 0;
    theirs[17] = 0;
    TcpPacket::new_unchecked(&mut theirs[..]).fill_checksum(&src, &dst);
    assert_eq!(theirs, segment, "the stack's fill writes the matcher's word");
    // The control: the window doubled, the field stale -- refused.
    let mut doctored = segment;
    doctored[14] = 0x08;
    assert!(!TcpPacket::new_checked(&doctored[..]).expect("still well-formed").verify_checksum(&src, &dst), "a doctored segment is refused");
}

#[test]
fn net_math_icmpv4_verified_by_the_parser() {
    // An echo request: type 8, code 0, identifier 0x1234, sequence 1, a 5-byte payload (odd).
    let mut message = [8, 0, 0, 0, 0x12, 0x34, 0x00, 0x01, b'r', b'h', b'u', b'-', b'1'];
    let sum = net_math::icmpv4_checksum(&message);
    message[2..4].copy_from_slice(&sum.to_be_bytes());
    assert_eq!(net_math::checksum(&message), 0, "the field in place, the receiver's complement is zero");
    let packet = Icmpv4Packet::new_checked(&message[..]).expect("a well-formed echo request");
    assert!(packet.verify_checksum(), "the stack verifies the matcher's ICMP checksum");
    let mut theirs = message;
    theirs[2] = 0;
    theirs[3] = 0;
    Icmpv4Packet::new_unchecked(&mut theirs[..]).fill_checksum();
    assert_eq!(theirs, message, "the stack's fill writes the matcher's word");
    // The control: the sequence number doctored, the field stale -- refused.
    let mut doctored = message;
    doctored[7] = 2;
    assert!(!Icmpv4Packet::new_checked(&doctored[..]).expect("still well-formed").verify_checksum(), "a doctored message is refused");
}

#[test]
fn sdcard_transfer_refusals_recover_by_cmd12_before_any_retry() {
    let buf = [0u8; 3 * 512];
    let tran = Ok(Reply::Short(STATUS_TRAN_READY));
    let prg = Ok(Reply::Short(STATUS_PRG));
    // (a) The FIFO lost pace on a three-block write: CMD12, CMD13 through the programming state, the ONE retry, then
    // the retry's own programming wait; the script consumed whole, the programming poll's one pause counted.
    let mut h = Script::new(vec![(12, 0, tran), (13, RCA, prg), (13, RCA, tran), (13, RCA, tran)]);
    h.refuse = vec![Refusal::Overrun];
    sdcard::write(&mut h, &card(true, false), 5, 3, &buf).unwrap();
    assert_eq!((h.data.clone(), h.next, h.pauses), (vec![(25, 5, 3), (25, 5, 3)], 4, 1));
    // (b) The control: a second Overrun is final, after a second recovery; two transfers, no third.
    let mut h = Script::new(vec![(12, 0, tran), (13, RCA, tran), (12, 0, tran), (13, RCA, tran)]);
    h.refuse = vec![Refusal::Overrun, Refusal::Overrun];
    assert_eq!(sdcard::write(&mut h, &card(true, false), 5, 3, &buf), Err(Stuck { step: Step::Transfer, why: Why::Refused(Refusal::Overrun) }));
    assert_eq!((h.data.len(), h.next), (2, 4));
    // (c) A timeout on a one-block write is final after the recovery: one transfer, no retry.
    let mut h = Script::new(vec![(12, 0, tran), (13, RCA, tran)]);
    h.refuse = vec![Refusal::Timeout];
    assert_eq!(sdcard::write(&mut h, &card(true, false), 5, 1, &buf), Err(Stuck { step: Step::Transfer, why: Why::Refused(Refusal::Timeout) }));
    assert_eq!((h.data.clone(), h.next), (vec![(24, 5, 1)], 2));
    // (d) The data line stuck busy: final too, after the recovery.
    let mut h = Script::new(vec![(12, 0, tran), (13, RCA, tran)]);
    h.refuse = vec![Refusal::Busy];
    assert_eq!(sdcard::write(&mut h, &card(true, false), 5, 2, &buf), Err(Stuck { step: Step::Transfer, why: Why::Refused(Refusal::Busy) }));
    assert_eq!((h.data.len(), h.next), (1, 2));
    // (e) A CRC failure on a multiple-block read: CMD12, CMD13, the retry; the read served after it.
    let mut out = [0u8; 2 * 512];
    let mut h = Script::new(vec![(12, 0, tran), (13, RCA, tran)]);
    h.refuse = vec![Refusal::Crc];
    sdcard::read(&mut h, &card(true, false), 9, 2, &mut out).unwrap();
    assert_eq!((h.data.clone(), h.next, out[0]), (vec![(18, 9, 2), (18, 9, 2)], 2, 9));
    // (f) The control: CMD12 unanswered (a card gone or dead) -- the transfer's own refusal returned, no CMD13, no retry.
    let mut h = Script::new(vec![(12, 0, Err(Refusal::Timeout))]);
    h.refuse = vec![Refusal::Overrun];
    assert_eq!(sdcard::write(&mut h, &card(true, false), 5, 3, &buf), Err(Stuck { step: Step::Transfer, why: Why::Refused(Refusal::Overrun) }));
    assert_eq!((h.data.len(), h.next), (1, 1));
    // (g) The control: the card never leaves the programming state after CMD12 -- Busy at CMD13's budget, no retry.
    let mut steps = vec![(12, 0, tran)];
    for _ in 0..sdcard::PROGRAM_TRIES_PER_BLOCK * 3 {
        steps.push((13, RCA, prg));
    }
    let mut h = Script::new(steps);
    h.refuse = vec![Refusal::Overrun];
    assert_eq!(sdcard::write(&mut h, &card(true, false), 5, 3, &buf), Err(Stuck { step: Step::Cmd13, why: Why::Busy }));
    assert_eq!((h.data.len(), h.next as u32, h.pauses), (1, 1 + sdcard::PROGRAM_TRIES_PER_BLOCK * 3, sdcard::PROGRAM_TRIES_PER_BLOCK * 3 - 1));
    // (h) The control: an error bit in the status after CMD12 refuses at CMD13 (the write-protect violation ReadOnly).
    let mut h = Script::new(vec![(12, 0, tran), (13, RCA, Ok(Reply::Short(STATUS_TRAN_READY | sdcard::STATUS_WP_VIOLATION)))]);
    h.refuse = vec![Refusal::Overrun];
    assert_eq!(sdcard::write(&mut h, &card(true, false), 5, 3, &buf), Err(Stuck { step: Step::Cmd13, why: Why::ReadOnly }));
    assert_eq!(h.data.len(), 1);
    // (i) No card and an argument need no recovery: no command at all (a CMD12 would end the empty script).
    for r in [Refusal::NoCard, Refusal::Argument] {
        let mut h = Script::new(vec![]);
        h.refuse = vec![r];
        assert_eq!(sdcard::write(&mut h, &card(true, false), 5, 1, &buf), Err(Stuck { step: Step::Transfer, why: Why::Refused(r) }));
        assert_eq!((h.data.len(), h.next), (1, 0));
    }
}
// ---- The FAT layer's read-side cases over the
// ---- three sparse goldens of goldens/fat/, each loaded from the manifest (both digests checked once per process)
// ---- and taken as the case's own copy; the doctored controls patched in memory, never a separate golden.
use crate::fat::{Entry, Error, Kind, Refused, Volume, O_READ, SEEK_CUR, SEEK_END, SEEK_SET};
use crate::sha256::sha256_hex;
use crate::sparse::{golden, manifest, recipe_bytes, Op, Sparse};

const GOLDENS: [&str; 3] = ["fat12-1m", "fat16-32m", "fat32-64m"];
/// The non-ASCII name as the golden stores it, from its UTF-8 bytes: a-umlaut, o-umlaut, u-umlaut, sharp s, U+D11E, ".txt".
fn non_ascii() -> &'static str {
    std::str::from_utf8(b"\xc3\xa4\xc3\xb6\xc3\xbc\xc3\x9f\xed\x84\x9e.txt").unwrap()
}
/// The generator was given U+1D11E; mtools 4.0.43 stored its low sixteen bits (the manifest's reading), so the bytes are
/// the rule's for the name as the recipe wrote it and the name on the volume ends in the Hangul syllable U+D11E.
fn non_ascii_seed() -> &'static str {
    std::str::from_utf8(b"\xc3\xa4\xc3\xb6\xc3\xbc\xc3\x9f\xf0\x9d\x84\x9e.txt").unwrap()
}
const NON_ASCII_KEY: &str = "U+00E4 U+00F6 U+00FC U+00DF U+D11E .txt";
/// U+1D11E (the musical G clef), from its UTF-8 bytes.
fn clef() -> &'static str {
    std::str::from_utf8(b"\xf0\x9d\x84\x9e").unwrap()
}
/// "ete" with both e acute (U+00E9), from its UTF-8 bytes.
fn e_acute_name() -> &'static str {
    std::str::from_utf8(b"\xc3\xa9t\xc3\xa9.txt").unwrap()
}
/// The golden's nested directory, three deep.
const DEEP_DIR: &str = concat!("/a", "/b", "/c");
/// The manifest's spelling of the nested file's row (its file table's key).
const DEEP_KEY: &str = "deep.txt, three directories down (a, then b, then c)";

fn long_name() -> String {
    let s = "the-two-hundred-character-name-".repeat(7);
    format!("{}.txt", &s[..196])
}

fn mount_dev(dev: Sparse, scratch_len: usize) -> Result<Volume<Sparse>, Refused> {
    let mut scratch = vec![0u8; scratch_len];
    Volume::mount(dev, &mut scratch)
}

fn mount(name: &str) -> Volume<Sparse> {
    mount_dev(golden(name), 64 * 1024).unwrap_or_else(|r| panic!("{name}: {}", r.words()))
}

fn refused(dev: Sparse) -> &'static str {
    match mount_dev(dev, 64 * 1024) {
        Ok(_) => "mounted",
        Err(r) => r.words(),
    }
}

fn list(v: &mut Volume<Sparse>, path: &str) -> Vec<(String, bool, u32, u32, bool)> {
    let mut d = v.opendir(path.as_bytes()).unwrap_or_else(|e| panic!("opendir {path}: {e:?}"));
    let mut e = Entry::EMPTY;
    let mut out = vec![];
    while v.readdir(&mut d, &mut e).unwrap() {
        out.push((String::from_utf8(e.name().to_vec()).unwrap(), e.is_dir(), e.size, e.first, e.long));
    }
    v.closedir(&mut d).unwrap();
    out
}

fn names(l: &[(String, bool, u32, u32, bool)]) -> Vec<String> {
    l.iter().map(|e| e.0.clone()).collect()
}

fn read_all(v: &mut Volume<Sparse>, path: &str) -> Vec<u8> {
    let mut h = v.open(path.as_bytes(), O_READ).unwrap_or_else(|e| panic!("open {path}: {e:?}"));
    let mut out = vec![0u8; h.size() as usize];
    let mut done = 0;
    while done < out.len() {
        let n = v.read(&mut h, &mut out[done..]).unwrap();
        assert!(n > 0, "{path}: a read inside the file moved nothing");
        done += n;
    }
    assert_eq!(v.read(&mut h, &mut [0u8; 16]).unwrap(), 0, "{path}: 0 at the end");
    v.close(&mut h).unwrap();
    out
}

fn runs(v: &mut Volume<Sparse>, first: u32) -> u32 {
    let mut c = first;
    let mut runs = 1;
    while let Some(n) = v.next_cluster(c).unwrap() {
        if n != c + 1 {
            runs += 1;
        }
        c = n;
    }
    runs
}

/// The FAT entry decoded a second time, straight from the device's bytes (the independent check for the getter).
fn raw_fat_entry(dev: &Sparse, kind: Kind, rsvd: u32, n: u32) -> u32 {
    match kind {
        Kind::Fat12 => {
            let off = n + n / 2;
            let b0 = dev.sector(rsvd + off / 512)[(off % 512) as usize];
            let b1 = dev.sector(rsvd + (off + 1) / 512)[((off + 1) % 512) as usize];
            let v = b0 as u32 | (b1 as u32) << 8;
            if n & 1 == 1 { v >> 4 } else { v & 0xFFF }
        }
        Kind::Fat16 => {
            let s = dev.sector(rsvd + n * 2 / 512);
            let o = (n * 2 % 512) as usize;
            u16::from_le_bytes([s[o], s[o + 1]]) as u32
        }
        Kind::Fat32 => {
            let s = dev.sector(rsvd + n * 4 / 512);
            let o = (n * 4 % 512) as usize;
            u32::from_le_bytes([s[o], s[o + 1], s[o + 2], s[o + 3]]) & 0x0FFF_FFFF
        }
    }
}

#[test]
fn fat_bpb_refuses_the_doctored_boot_sectors() {
    for name in GOLDENS {
        let base = golden(name);
        assert_eq!(refused(base.clone()), "mounted", "{name} mounts as it is");
        let mut d = base.clone();
        d.patch(0, 510, &[0, 0]);
        assert_eq!(refused(d), "no boot signature", "{name}");
        let mut d = base.clone();
        d.patch(0, 11, &1024u16.to_le_bytes());
        assert_eq!(refused(d), "unsupported sector size", "{name}");
        let mut d = base.clone();
        d.patch(0, 11, &100u16.to_le_bytes());
        assert_eq!(refused(d), "bad BPB", "{name}: a sector size that is not a legal value");
        let mut d = base.clone();
        d.patch(0, 13, &[3]);
        assert_eq!(refused(d), "bad BPB", "{name}: 3 sectors per cluster");
        let mut d = base.clone();
        d.patch(0, 16, &[0]);
        assert_eq!(refused(d), "bad BPB", "{name}: 0 FATs");
        let mut d = base.clone();
        d.patch(0, 16, &[3]);
        assert_eq!(refused(d), "bad BPB", "{name}: 3 FATs");
        let mut d = base.clone();
        d.patch(0, 14, &[0, 0]);
        assert_eq!(refused(d), "bad BPB", "{name}: 0 reserved sectors");
        let mut d = base.clone();
        d.patch(0, 19, &[0, 0]);
        d.patch(0, 32, &[0, 0, 0, 0]);
        assert_eq!(refused(d), "bad BPB", "{name}: 0 sectors");
        let mut d = base.clone();
        d.patch(0, 19, &[0, 0]);
        d.patch(0, 32, &(base.sector_count() + 1).to_le_bytes());
        assert_eq!(refused(d), "bad BPB", "{name}: more sectors than the device");
        if name == "fat32-64m" {
            let mut d = base.clone();
            d.patch(0, 44, &1u32.to_le_bytes());
            assert_eq!(refused(d), "bad BPB", "{name}: a root cluster of 1");
            let mut d = base.clone();
            d.patch(0, 17, &16u16.to_le_bytes());
            assert_eq!(refused(d), "bad BPB", "{name}: root entries on FAT32");
        } else {
            let mut d = base.clone();
            d.patch(0, 17, &0u16.to_le_bytes());
            assert_eq!(refused(d), "bad BPB", "{name}: no root region");
        }
        let mut d = base.clone();
        d.patch(0, 3, b"EXFAT   ");
        d.patch(0, 11, &[0, 0]);
        assert_eq!(refused(d), "exFAT: not supported", "{name}");
    }
    let mut small = Sparse::blank(100);
    small.patch(0, 0, &golden("fat12-1m").sector(0));
    assert_eq!(refused(small), "bad BPB", "a device shorter than the volume");
    assert_eq!(Refused::Device(Error::NoDevice).words(), "no device");
    assert_eq!(Error::NotFound.code(), -2);
    assert_eq!(Error::Timeout.code(), -110);
}

#[test]
fn fat_type_by_the_cluster_count_on_the_three_goldens() {
    let m = manifest();
    assert_eq!(m.images.len(), 3, "the manifest's three images");
    for (name, kind, label) in [("fat12-1m", Kind::Fat12, &b"RHUFAT12"[..]), ("fat16-32m", Kind::Fat16, b"RHUFAT16"), ("fat32-64m", Kind::Fat32, b"RHUFAT32")] {
        let row = m.images.iter().find(|r| r.name == name).unwrap();
        let v = mount(name);
        let g = *v.geometry();
        assert_eq!(g.kind, kind, "{name}");
        assert_eq!(g.tot_sec, row.sectors, "{name}: the sectors");
        assert_eq!(g.count_of_clusters, row.clusters, "{name}: the clusters");
        assert_eq!(g.cluster_bytes, row.cluster_bytes, "{name}: the cluster size");
        assert_eq!(g.max, row.clusters + 1);
        assert_eq!(v.label(), label, "{name}: the label");
        assert_eq!(v.free_clusters(), row.free, "{name}: the free count");
        assert_eq!(g.first_data_sector, g.rsvd + g.num_fats * g.fat_sz + g.root_dir_sectors);
        assert_eq!(g.root_dir_sectors, if kind == Kind::Fat32 { 0 } else { g.root_ent_cnt * 32 / 512 });
        assert_eq!(g.num_fats, 2);
        assert!(g.mirroring && g.active_fat == 0);
        assert_eq!(g.fsinfo != 0, kind == Kind::Fat32, "{name}: the FSInfo only on FAT32");
        assert_eq!(Kind::Fat12, crate::fat::Geometry::kind_of(4084));
        assert_eq!(Kind::Fat16, crate::fat::Geometry::kind_of(4085));
        assert_eq!(Kind::Fat16, crate::fat::Geometry::kind_of(65524));
        assert_eq!(Kind::Fat32, crate::fat::Geometry::kind_of(65525));
    }
    // The type string never decides: the fat16 image labelled FAT32 still mounts FAT16, the fat12 one labelled FAT16 FAT12.
    let mut d = golden("fat16-32m");
    d.patch(0, 54, b"FAT32   ");
    assert_eq!(mount_dev(d, 65536).unwrap().geometry().kind, Kind::Fat16);
    let mut d = golden("fat12-1m");
    d.patch(0, 54, b"FAT16   ");
    assert_eq!(mount_dev(d, 65536).unwrap().geometry().kind, Kind::Fat12);
    // The label from the root's volume-ID entry over the boot sector's: the boot sector's blanked, the entry serves.
    let mut d = golden("fat16-32m");
    d.patch(0, 43, b"           ");
    assert_eq!(mount_dev(d, 65536).unwrap().label(), b"RHUFAT16");
}

#[test]
fn fat_entries_at_all_three_widths() {
    for name in GOLDENS {
        let mut v = mount(name);
        let g = *v.geometry();
        let dev = golden(name);
        let upto = g.max.min(6000);
        for n in 0..=upto {
            assert_eq!(v.fat_get(n).unwrap(), raw_fat_entry(&dev, g.kind, g.rsvd, n), "{name}: entry {n}");
        }
        assert_eq!(v.fat_get(g.max + 1), Err(Error::Io), "{name}: past MAX");
        assert_eq!(v.fat_set(1, 0), Err(Error::Invalid), "{name}: FAT[1] never written");
        assert_eq!(v.fat_get(0).unwrap() & 0xFF, 0xF8, "{name}: the media byte in FAT[0]");
    }
    // FAT12: the entry spanning sectors (341 + 170 = byte 511) and its even neighbour, through a write and the flush.
    let mut v = mount("fat12-1m");
    let g = *v.geometry();
    let before_340 = v.fat_get(340).unwrap();
    v.fat_set(341, 0xABC).unwrap();
    assert_eq!(v.fat_get(341).unwrap(), 0xABC);
    assert_eq!(v.fat_get(340).unwrap(), before_340, "the even neighbour's nibble kept");
    v.fat_set(340, 0x123).unwrap();
    assert_eq!(v.fat_get(341).unwrap(), 0xABC, "the odd neighbour kept");
    assert_eq!(v.fat_get(340).unwrap(), 0x123);
    v.flush().unwrap();
    let dev = v.device();
    assert_eq!(raw_fat_entry(dev, Kind::Fat12, g.rsvd, 341), 0xABC);
    assert_eq!(raw_fat_entry(dev, Kind::Fat12, g.rsvd, 340), 0x123);
    assert_eq!(raw_fat_entry(dev, Kind::Fat12, g.rsvd + g.fat_sz, 341), 0xABC, "the second copy equal");
    assert_eq!(raw_fat_entry(dev, Kind::Fat12, g.rsvd + g.fat_sz, 340), 0x123);
    assert_eq!(dev.calls(Op::Write), 4, "two sectors to each of two copies");
    // FAT16: a word.
    let mut v = mount("fat16-32m");
    let g = *v.geometry();
    v.fat_set(g.max, 0x1234).unwrap();
    v.flush().unwrap();
    assert_eq!(raw_fat_entry(v.device(), Kind::Fat16, g.rsvd, g.max), 0x1234);
    assert_eq!(raw_fat_entry(v.device(), Kind::Fat16, g.rsvd + g.fat_sz, g.max), 0x1234);
    // FAT32: the high nibble preserved through a write.
    let mut d = golden("fat32-64m");
    let g = *mount("fat32-64m").geometry();
    let n = g.max;
    let (s, o) = (g.rsvd + n * 4 / 512, (n * 4 % 512) as usize);
    let mut word = d.sector(s)[o..o + 4].to_vec();
    word[3] |= 0xA0;
    d.patch(s, o, &word);
    let mut v = mount_dev(d, 65536).unwrap();
    assert_eq!(v.fat_get(n).unwrap(), 0, "the nibble masked on read");
    v.fat_set(n, 0x0FFF_FFF8).unwrap();
    assert_eq!(v.fat_get(n).unwrap(), 0x0FFF_FFF8);
    v.flush().unwrap();
    let w = v.device().sector(s);
    assert_eq!(u32::from_le_bytes([w[o], w[o + 1], w[o + 2], w[o + 3]]), 0xAFFF_FFF8, "the high nibble preserved");
}

#[test]
fn fat_chain_walk_bounded_and_a_loop_refused() {
    let mut v = mount("fat16-32m");
    let first = v.stat(b"/big.bin").unwrap().first;
    assert_eq!(v.chain_length(first).unwrap(), 1024, "1 MiB in 1 KiB clusters");
    assert_eq!(v.chain_length(0).unwrap(), 0, "an empty file");
    let mut c = first;
    let mut tenth = 0;
    let mut i = 0;
    while let Some(n) = v.next_cluster(c).unwrap() {
        i += 1;
        if i == 10 {
            tenth = c;
        }
        c = n;
    }
    let last = c;
    // The last entry pointed back at the first: refused at once.
    v.fat_set(last, first).unwrap();
    assert_eq!(v.chain_length(first), Err(Error::Io));
    // A loop that never returns to the first: refused when the walk passes the count of clusters, never a hang.
    v.fat_set(last, tenth).unwrap();
    assert_eq!(v.chain_length(first), Err(Error::Io));
    // The file read through the loop stops at its size.
    let bytes = read_all(&mut v, "/big.bin");
    assert_eq!(bytes.len(), 1 << 20);
    // A free entry inside a chain: refused.
    v.fat_set(tenth, 0).unwrap();
    assert_eq!(v.chain_length(first), Err(Error::Io));
    let mut h = v.open(b"/big.bin", O_READ).unwrap();
    let mut buf = vec![0u8; 64 * 1024];
    assert_eq!(v.read(&mut h, &mut buf), Err(Error::Io));
    // A directory chain that loops: the walk ends (the root of fat32 is a chain; the many/ directory's too).
    let mut v = mount("fat32-64m");
    let many = v.stat(b"/many").unwrap().first;
    v.fat_set(many, many).unwrap();
    let mut d = v.opendir(b"/many").unwrap();
    let mut e = Entry::EMPTY;
    let r = loop {
        match v.readdir(&mut d, &mut e) {
            Ok(true) => continue,
            other => break other,
        }
    };
    assert_eq!(r, Err(Error::Io), "a looping directory chain is refused, not walked forever");
}

#[test]
fn fat_root_walk_lists_every_entry_with_its_long_name() {
    for name in GOLDENS {
        let mut v = mount(name);
        let root = list(&mut v, "/");
        let mut got = names(&root);
        got.sort();
        let mut want = vec![".".to_string(), "..".to_string(), "hello.txt".into(), long_name(), non_ascii().into(), "empty.bin".into(), "a".into(), "big.bin".into(), "many".into()];
        want.sort();
        assert_eq!(got, want, "{name}: the root's names (the label skipped)");
        for e in &root {
            match e.0.as_str() {
                "." | ".." => assert!(e.1 && e.3 == v.geometry().root_clus),
                "a" | "many" => assert!(e.1 && e.2 == 0 && e.3 >= 2, "{name}: {} is a directory", e.0),
                "hello.txt" => assert!(!e.1 && e.2 == 58 && !e.4, "{name}: an 8.3 lower-case name is a short entry with the NT case bits"),
                "empty.bin" => assert!(!e.1 && e.2 == 0 && e.3 == 0, "{name}: an empty file's first cluster is 0"),
                "big.bin" => assert_eq!(e.2, if name == "fat12-1m" { 262144 } else { 1 << 20 }),
                n if n == long_name() => assert!(e.2 == 200 && n.len() == 200 && e.4),
                n if n == non_ascii() => assert!(e.2 == 40 && e.4),
                other => panic!("{name}: an unexpected entry {other}"),
            }
        }
    }
}

#[test]
fn fat_deep_tree_and_the_thousand_entries() {
    for name in GOLDENS {
        let mut v = mount(name);
        let s = v.stat(format!("{DEEP_DIR}/deep.txt").as_bytes()).unwrap();
        assert!(s.size == 100 && !s.is_dir && !s.read_only, "{name}");
        assert_eq!(names(&list(&mut v, "/a")), ["." , "..", "b"], "{name}");
        assert_eq!(names(&list(&mut v, &DEEP_DIR[..4])), [".", "..", "c"], "{name}");
        assert_eq!(names(&list(&mut v, DEEP_DIR)), [".", "..", "deep.txt"], "{name}");
        let many = list(&mut v, "/many");
        assert_eq!(many.len(), 1002, "{name}: 1,000 files and the two dot entries");
        let mut got: Vec<String> = many.iter().filter(|e| !e.1).map(|e| e.0.clone()).collect();
        got.sort();
        let want: Vec<String> = (1..=1000).map(|i| format!("f{i:04}.txt")).collect();
        assert_eq!(got, want, "{name}: every name");
        assert!(many.iter().filter(|e| !e.1).all(|e| e.2 == 16 && !e.4), "{name}: 16 bytes each, every one a short entry served in lower case by its NT case bits");
        assert_eq!(v.stat(b"/many/f0500.txt").unwrap().size, 16);
        assert_eq!(v.stat(b"/many/f1000.txt").unwrap().size, 16);
        let dot = many.iter().find(|e| e.0 == ".").unwrap();
        let dotdot = many.iter().find(|e| e.0 == "..").unwrap();
        assert!(dot.1 && dot.3 == v.stat(b"/many").unwrap().first, "{name}: '.' names the directory's own cluster");
        assert!(dotdot.1 && dotdot.3 == 0, "{name}: '..' of a root child is 0");
        assert_eq!(v.stat(b"/many/..").unwrap().first, v.geometry().root_clus, "{name}: '..' resolves to the root");
    }
}

#[test]
fn fat_stale_long_name_sets_served_by_the_short_name() {
    for name in GOLDENS {
        let mut v = mount(name);
        let mut d = v.opendir(b"/").unwrap();
        let mut e = Entry::EMPTY;
        let mut found = None;
        while v.readdir(&mut d, &mut e).unwrap() {
            if e.name() == long_name().as_bytes() {
                found = Some(e);
            }
        }
        let e = found.expect("the 200-character name listed");
        assert!(e.long && e.set_start != e.place, "{name}: a long set before the short entry");
        let base_count = list(&mut v, "/").len();
        // (a) a checksum byte of the first long entry patched: the set dropped, the alias served.
        let mut dev = golden(name);
        let mut sec = dev.sector(e.set_start.0);
        sec[e.set_start.1 as usize + 13] ^= 0x01;
        dev.patch(e.set_start.0, 0, &sec);
        let mut v2 = mount_dev(dev, 65536).unwrap();
        let l = list(&mut v2, "/");
        assert_eq!(l.len(), base_count, "{name}: every entry still listed");
        let alias = l.iter().find(|x| x.2 == 200).expect("the file by its short name");
        assert!(!alias.4 && alias.0.len() <= 12 && alias.0.ends_with(".TXT") && alias.0.contains('~'), "{name}: the alias {}", alias.0);
        assert_eq!(read_all(&mut v2, &format!("/{}", alias.0)).len(), 200, "{name}: opened by the alias");
        assert_eq!(v2.stat(format!("/{}", long_name()).as_bytes()), Err(Error::NotFound), "{name}: the long name no longer matches");
        // (b) an ordinal patched: the set's count wrong, dropped.
        let mut dev = golden(name);
        let mut sec = dev.sector(e.set_start.0);
        sec[e.set_start.1 as usize] = 0x40 | 0x0F;
        dev.patch(e.set_start.0, 0, &sec);
        let mut v3 = mount_dev(dev, 65536).unwrap();
        let l = list(&mut v3, "/");
        assert_eq!(l.len(), base_count);
        assert!(l.iter().any(|x| x.2 == 200 && !x.4), "{name}: served by the short name");
        // (c) the set without its short entry: the short entry turned into a stray long entry, the file gone, the rest intact.
        let mut dev = golden(name);
        let mut sec = dev.sector(e.place.0);
        sec[e.place.1 as usize + 11] = 0x0F;
        dev.patch(e.place.0, 0, &sec);
        let mut v4 = mount_dev(dev, 65536).unwrap();
        let l = list(&mut v4, "/");
        assert_eq!(l.len(), base_count - 1, "{name}: one entry fewer");
        assert!(l.iter().all(|x| x.2 != 200), "{name}: the 200-character file is gone");
        let lower: Vec<String> = l.iter().map(|x| x.0.to_lowercase()).collect();
        for n in ["hello.txt", "empty.bin", "a", "big.bin", "many", non_ascii()] {
            assert!(lower.contains(&n.to_lowercase()), "{name}: {n} still listed");
        }
    }
}

#[test]
fn fat_reads_every_file_byte_exact_against_the_recipe() {
    let m = manifest();
    assert!(m.files.len() >= 8, "the manifest's file table");
    for name in GOLDENS {
        let mut v = mount(name);
        let big_len = if name == "fat12-1m" { 262144 } else { 1 << 20 };
        let mut table: Vec<(String, usize)> = vec![("hello.txt".into(), 58), (format!("{}/deep.txt", &DEEP_DIR[1..]), 100), (long_name(), 200), (non_ascii().into(), 40), ("empty.bin".into(), 0), ("big.bin".into(), big_len)];
        for i in 1..=1000 {
            table.push((format!("many/f{i:04}.txt"), 16));
        }
        for (path, len) in &table {
            let file = path.rsplit('/').next().unwrap();
            let seed = if *path == non_ascii() { non_ascii_seed() } else { file };
            let key = if *path == non_ascii() { NON_ASCII_KEY.to_string() } else if path.ends_with("/deep.txt") { DEEP_KEY.to_string() } else { path.clone() };
            let bytes = read_all(&mut v, &format!("/{path}"));
            assert_eq!(bytes.len(), *len, "{name}: /{path}'s length");
            assert!(bytes == recipe_bytes(seed, *len), "{name}: /{path}'s bytes differ from the recipe's rule");
            if let Some(row) = m.files.iter().find(|r| r.0 == key || (file == "big.bin" && r.0.starts_with("big.bin") && r.1 as usize == *len)) {
                assert_eq!(sha256_hex(&bytes), row.2, "{name}: /{path}'s sha256 against the manifest");
            }
        }
        let big = v.stat(b"/big.bin").unwrap();
        assert_eq!(runs(&mut v, big.first), 8, "{name}: big.bin's chain in eight runs");
        assert_eq!(v.chain_length(big.first).unwrap(), big_len as u32 / v.geometry().cluster_bytes);
        assert_eq!(v.stat(b"/empty.bin").unwrap().first, 0);
        // The direct path: big.bin into an aligned buffer moves whole runs in few calls and nothing through the cache.
        v.device().trace.clear();
        let before = v.cache_stats();
        let bytes = read_all(&mut v, "/big.bin");
        assert_eq!(bytes.len(), big_len);
        let after = v.cache_stats();
        let run_sectors = (big_len as u32 / 8) / 512;
        let per_run = run_sectors.div_ceil(128);
        let multi = v.device().trace.iter().filter(|t| t.0 == Op::Read && t.2 > 1).count() as u32;
        let moved = v.device().sectors_moved(Op::Read);
        assert!(multi <= 8 * per_run + 2, "{name}: {multi} multi-sector reads for eight runs of {run_sectors} sectors");
        assert!(moved - big_len as u32 / 512 <= 48, "{name}: {moved} sectors read for {} of data, the rest the FAT's", big_len / 512);
        assert!(after.misses - before.misses <= 48, "{name}: only FAT and directory sectors through the cache ({} misses)", after.misses - before.misses);
    }
}

#[test]
fn fat_seek_and_partial_reads() {
    for name in ["fat32-64m", "fat12-1m"] {
        let mut v = mount(name);
        let big_len = if name == "fat12-1m" { 262144 } else { 1 << 20 };
        let content = recipe_bytes("big.bin", big_len);
        let cb = v.geometry().cluster_bytes as usize;
        let mut h = v.open(b"/big.bin", O_READ).unwrap();
        let mut buf = vec![0u8; 4097];
        assert_eq!(v.seek(&mut h, 12345, SEEK_SET).unwrap(), 12345);
        assert_eq!(v.read(&mut h, &mut buf[..1000]).unwrap(), 1000);
        assert_eq!(&buf[..1000], &content[12345..13345], "{name}: an unaligned offset");
        assert_eq!(h.position(), 13345);
        let at = 3 * cb - 7;
        v.seek(&mut h, at as i64, SEEK_SET).unwrap();
        assert_eq!(v.read(&mut h, &mut buf[..100]).unwrap(), 100);
        assert_eq!(&buf[..100], &content[at..at + 100], "{name}: across a cluster boundary");
        assert_eq!(v.seek(&mut h, -16, SEEK_END).unwrap(), big_len as u64 - 16);
        assert_eq!(v.read(&mut h, &mut buf[..64]).unwrap(), 16, "{name}: the last 16 bytes then the end");
        assert_eq!(&buf[..16], &content[big_len - 16..]);
        assert_eq!(v.read(&mut h, &mut buf[..64]).unwrap(), 0);
        assert_eq!(v.seek(&mut h, 100, SEEK_END).unwrap(), big_len as u64 + 100, "{name}: a seek past the end is allowed");
        assert_eq!(v.read(&mut h, &mut buf[..64]).unwrap(), 0, "{name}: nothing past the end");
        assert_eq!(v.seek(&mut h, -1, SEEK_SET), Err(Error::Invalid));
        assert_eq!(v.seek(&mut h, 0x1_0000_0000, SEEK_SET), Err(Error::Invalid));
        assert_eq!(v.seek(&mut h, 0, 7), Err(Error::Invalid));
        v.seek(&mut h, 0, SEEK_SET).unwrap();
        assert_eq!(v.seek(&mut h, 10, SEEK_CUR).unwrap(), 10);
        assert_eq!(v.seek(&mut h, -5, SEEK_CUR).unwrap(), 5);
        // A misaligned caller buffer goes through the cache and reads the same bytes as the direct path.
        v.seek(&mut h, 0, SEEK_SET).unwrap();
        let mut done = 0;
        while done < 4096 {
            done += v.read(&mut h, &mut buf[1 + done..4097]).unwrap();
        }
        assert_eq!(&buf[1..4097], &content[..4096], "{name}: the cache path");
        // One byte at a time over the first 600 bytes.
        v.seek(&mut h, 0, SEEK_SET).unwrap();
        for i in 0..600 {
            let mut one = [0u8; 1];
            assert_eq!(v.read(&mut h, &mut one).unwrap(), 1);
            assert_eq!(one[0], content[i], "{name}: byte {i}");
        }
        v.close(&mut h).unwrap();
        assert_eq!(v.read(&mut h, &mut buf[..8]), Err(Error::BadHandle), "{name}: a closed handle");
        assert_eq!(v.close(&mut h), Err(Error::BadHandle));
        // An empty file: 0 at once, a seek allowed.
        let mut e = v.open(b"/empty.bin", O_READ).unwrap();
        assert_eq!(v.read(&mut e, &mut buf[..8]).unwrap(), 0);
        assert_eq!(v.seek(&mut e, 5, SEEK_SET).unwrap(), 5);
        assert_eq!(v.read(&mut e, &mut buf[..8]).unwrap(), 0);
        assert_eq!(v.fstat(&e).unwrap().size, 0);
    }
}

#[test]
fn fat_path_rules_and_the_fold() {
    for name in ["fat32-64m", "fat12-1m"] {
        let mut v = mount(name);
        let up2 = format!("{}/../../hello.txt", &DEEP_DIR[..4]);
        for p in ["/hello.txt", "/HELLO.TXT", "/Hello.Txt", "//hello.txt", "/./hello.txt", "/a/../hello.txt", up2.as_str(), "/ hello.txt ", "/hello.txt...", "/many/../hello.txt"] {
            assert_eq!(v.stat(p.as_bytes()).map(|s| s.size), Ok(58), "{name}: {p:?}");
        }
        assert_eq!(v.stat(format!("{}/DEEP.TXT", DEEP_DIR.to_ascii_uppercase()).as_bytes()).unwrap().size, 100, "{name}");
        assert_eq!(v.stat(format!("{}{}{}/Deep.TXT", &DEEP_DIR[..2], DEEP_DIR[2..4].to_ascii_uppercase(), &DEEP_DIR[4..]).as_bytes()).unwrap().size, 100, "{name}");
        assert_eq!(v.stat(b"/\xc3\x84\xc3\x96\xc3\x9c\xc3\x9f\xed\x84\x9e.TXT").unwrap().size, 40, "{name}: the Latin-1 fold");
        assert_eq!(v.stat(b"/many/F0001.TXT").unwrap().size, 16, "{name}: the short form of a long name's alias is not the name; the long name folds");
        assert_eq!(v.stat(b"/MANY/f0001.txt").unwrap().size, 16, "{name}");
        assert!(v.stat(b"/").unwrap().is_dir && v.stat(format!("{}/..", &DEEP_DIR[..4]).as_bytes()).unwrap().is_dir && v.stat(b"/..").unwrap().is_dir, "{name}");
        assert_eq!(v.stat(format!("{DEEP_DIR}/deep.txt/x").as_bytes()), Err(Error::NotDir), "{name}");
        assert_eq!(v.stat(b"/hello.txt/"), Ok(v.stat(b"/hello.txt").unwrap()), "{name}: a trailing slash on a file");
        assert_eq!(v.open(b"/a", O_READ).err(), Some(Error::IsDir), "{name}");
        assert_eq!(v.opendir(b"/hello.txt").err(), Some(Error::NotDir), "{name}");
        assert_eq!(v.stat(b"/nope"), Err(Error::NotFound), "{name}");
        assert_eq!(v.stat(b"/a/nope/x"), Err(Error::NotFound), "{name}");
        for c in ['*', '?', '<', '>', ':', '"', '|', '\\'] {
            assert_eq!(v.stat(format!("/a{c}b").as_bytes()), Err(Error::Invalid), "{name}: {c:?}");
        }
        assert_eq!(v.stat(b"/a\x01b"), Err(Error::Invalid), "{name}: a control character");
        assert_eq!(v.stat(b"/a\xffb"), Err(Error::Invalid), "{name}: malformed UTF-8");
        assert_eq!(v.stat(b"/\xed\xa0\x80"), Err(Error::Invalid), "{name}: an encoded surrogate");
        assert_eq!(v.stat(format!("/{}", "a".repeat(256)).as_bytes()), Err(Error::NameTooLong), "{name}");
        assert_eq!(v.stat(format!("/{}", "a".repeat(255)).as_bytes()), Err(Error::NotFound), "{name}: 255 units is a legal name");
        assert_eq!(v.stat(format!("/{}", clef().repeat(128)).as_bytes()), Err(Error::NameTooLong), "{name}: 256 units of pairs");
        assert_eq!(v.stat(b"/ "), Err(Error::Invalid), "{name}: a name of spaces alone");
        assert_eq!(v.stat(b"/..."), Err(Error::Invalid), "{name}: periods alone");
        assert_eq!(v.stat(b""), Err(Error::Invalid), "{name}");
        assert_eq!(v.stat(b"hello.txt"), Err(Error::Invalid), "{name}: a relative path");
        // The non-ASCII name's bytes come back exactly as UTF-8.
        let root = list(&mut v, "/");
        assert!(root.iter().any(|e| e.0 == non_ascii() && e.0.len() == 15), "{name}: the name's UTF-8 (15 bytes)");
        // The stat's times: midnight of 1 January 2026, UTC, as the recipe stamped every entry.
        let s = v.stat(b"/hello.txt").unwrap();
        assert_eq!(s.mtime, 1_767_225_600, "{name}: the write time");
        assert_eq!(s.ctime, 1_767_225_600, "{name}: the creation time");
        assert_eq!(s.atime, 1_767_225_600, "{name}: the access date at midnight");
        assert_eq!(crate::fat::unix_from_packed(0, 0), 315_532_800, "the floor, midnight of 1 January 1980");
        assert_eq!(crate::fat::unix_from_packed((46 << 9) | (1 << 5) | 1, 0), 1_767_225_600);
        assert_eq!(crate::fat::unix_from_packed((46 << 9) | (1 << 5) | 1, (23 << 11) | (59 << 5) | 29), 1_767_225_600 + 86_398);
        assert_eq!(crate::fat::fold(b'A' as u16), b'a' as u16);
        assert_eq!(crate::fat::fold(0xC4), 0xE4);
        assert_eq!(crate::fat::fold(0xD7), 0xD7, "the multiplication sign is not a letter");
        assert_eq!(crate::fat::fold(0x0130), 0x0130, "outside Latin-1 exact");
        assert_eq!(crate::fat::checksum(b"THE-TW~1TXT"), crate::fat::checksum(b"THE-TW~1TXT"));
        let st = v.statfs();
        assert_eq!((st.block_size, st.blocks, st.name_max), (v.geometry().cluster_bytes, v.geometry().count_of_clusters, 255));
        assert_eq!(st.free, v.free_clusters());
    }
}

#[test]
fn fat_free_count_from_the_fsinfo_or_the_pass() {
    let m = manifest();
    let row32 = m.images.iter().find(|r| r.name == "fat32-64m").unwrap().clone();
    // FAT32: the FSInfo's count accepted when sane; the pass otherwise, with its call count.
    let v = mount("fat32-64m");
    assert!(v.hint_served() && v.pass_calls() == 0, "a PC-formatted FSInfo serves");
    assert_eq!(v.free_clusters(), row32.free);
    let g = *v.geometry();
    let entry_bytes = (g.max as u64 + 1) * 4;
    let calls_64k = entry_bytes.div_ceil(65536) as u32;
    let fsi = g.fsinfo;
    for (what, patch, served, hint2) in [("0xFFFFFFFF", 0xFFFF_FFFFu32, false, false), ("over the clusters", g.count_of_clusters + 1, false, false), ("sane", row32.free, true, false)] {
        let mut d = golden("fat32-64m");
        d.patch(fsi, 488, &patch.to_le_bytes());
        let v = mount_dev(d, 65536).unwrap();
        assert_eq!(v.hint_served(), served, "the count {what}");
        assert_eq!(v.free_clusters(), row32.free, "the count {what}: the free count equal either way");
        assert_eq!(v.pass_calls(), if served { 0 } else { calls_64k }, "the count {what}: the pass's calls");
        let _ = hint2;
    }
    // A hint past MAX: the count still serves, the hint falls to 2.
    let mut d = golden("fat32-64m");
    d.patch(fsi, 492, &(g.max + 10).to_le_bytes());
    let v = mount_dev(d, 65536).unwrap();
    assert!(v.hint_served() && v.next_free_hint() == 2);
    // A sane hint is taken as it is.
    let mut d = golden("fat32-64m");
    d.patch(fsi, 492, &1234u32.to_le_bytes());
    assert_eq!(mount_dev(d, 65536).unwrap().next_free_hint(), 1234);
    // The FSInfo's signatures broken: no hint, the pass.
    let mut d = golden("fat32-64m");
    d.patch(fsi, 0, &[0, 0, 0, 0]);
    let v = mount_dev(d, 65536).unwrap();
    assert!(!v.hint_served() && v.pass_calls() == calls_64k && v.free_clusters() == row32.free);
    // The pass through a 4 KB scratch: more calls, the same count.
    let v = mount_dev(golden("fat12-1m"), 4096).unwrap();
    let g12 = *v.geometry();
    let bytes12 = (((g12.max as u64 + 1) * 3 + 1) / 2 + 1).min(g12.fat_sz as u64 * 512);
    assert_eq!(v.pass_calls(), bytes12.div_ceil(4096) as u32);
    // FAT12 and FAT16 always the pass; the hint the first free cluster; the pass's calls through 64 KB.
    for name in ["fat12-1m", "fat16-32m"] {
        let row = m.images.iter().find(|r| r.name == name).unwrap();
        let mut v = mount(name);
        let g = *v.geometry();
        assert!(!v.hint_served() && v.pass_calls() >= 1, "{name}: the pass ran");
        assert_eq!(v.free_clusters(), row.free, "{name}: the free count");
        let bytes = match g.kind {
            Kind::Fat12 => ((g.max as u64 + 1) * 3 + 1) / 2 + 1,
            _ => (g.max as u64 + 1) * 2,
        }
        .min(g.fat_sz as u64 * 512);
        assert_eq!(v.pass_calls(), bytes.div_ceil(65536) as u32, "{name}: the calls through 64 KB");
        let mut first_free = 0;
        for n in 2..=g.max {
            if v.fat_get(n).unwrap() == 0 {
                first_free = n;
                break;
            }
        }
        assert_eq!(v.next_free_hint(), first_free, "{name}: the hint is the first free cluster");
        let mut count = 0;
        for n in 2..=g.max {
            if v.fat_get(n).unwrap() == 0 {
                count += 1;
            }
        }
        assert_eq!(count, v.free_clusters(), "{name}: the count equals a cluster-by-cluster scan");
    }
    // A scratch too small for one sector refuses the mount.
    assert_eq!(mount_dev(golden("fat12-1m"), 100).err().map(|r| r.words()), Some("a device error"));
}

// ---- The FAT layer's write-side cases: the host crate's own second
// ---- reader (fatread.rs) checks the volume after every write scenario; the written images are dumped as
// ---- raw files with their listings under target/fat-second-reader/ for mtools' third reading when
// ---- RHU_FAT_SECOND_READER_DUMP is set (the gate's run writes nothing).
use crate::calendar_math::{days_from_civil, Civil};
use crate::fat::{O_APPEND, O_CREATE, O_EXCL, O_TRUNC, O_WRITE};
use crate::fatread::Reader;

const RW: u32 = O_READ | O_WRITE;

fn create(v: &mut Volume<Sparse>, path: &str, bytes: &[u8]) {
    let mut h = v.open(path.as_bytes(), RW | O_CREATE | O_EXCL).unwrap_or_else(|e| panic!("create {path}: {e:?}"));
    let mut done = 0;
    while done < bytes.len() {
        let n = v.write(&mut h, &bytes[done..]).unwrap_or_else(|e| panic!("write {path}: {e:?}"));
        assert!(n > 0, "{path}: a write moved nothing");
        done += n;
    }
    v.close(&mut h).unwrap_or_else(|e| panic!("close {path}: {e:?}"));
}

fn walk_all(v: &mut Volume<Sparse>, path: &str, out: &mut Vec<(String, bool, u32, u32)>) {
    for e in list(v, path) {
        if e.0 == "." || e.0 == ".." {
            continue;
        }
        let p = if path == "/" { format!("/{}", e.0) } else { format!("{path}/{}", e.0) };
        out.push((p.clone(), e.1, e.2, e.3));
        if e.1 {
            walk_all(v, &p, out);
        }
    }
}

fn find_entry(v: &mut Volume<Sparse>, dir: &str, name: &str) -> Entry {
    let mut d = v.opendir(dir.as_bytes()).unwrap();
    let mut e = Entry::EMPTY;
    let mut found = None;
    while v.readdir(&mut d, &mut e).unwrap() {
        if e.name() == name.as_bytes() {
            found = Some(e);
        }
    }
    v.closedir(&mut d).unwrap();
    found.unwrap_or_else(|| panic!("{dir}: {name} listed"))
}

/// THE SECOND READER's CHECK: the volume synced, the second reader opened on a copy of the device, every
/// path, kind, size and first cluster compared, every file's bytes compared, the FAT copies compared, the free count
/// compared; the image and its listing dumped for the third reader when asked.
fn second_reader(v: &mut Volume<Sparse>, tag: &str) {
    v.sync().unwrap_or_else(|e| panic!("{tag}: the sync refused {e:?}"));
    let dev = v.device().clone();
    let r = Reader::open(&dev).unwrap_or_else(|e| panic!("{tag}: the second reader refused the volume: {e}"));
    let theirs = r.list_all().unwrap_or_else(|e| panic!("{tag}: the second reader's walk: {e}"));
    let mut ours = vec![];
    walk_all(v, "/", &mut ours);
    ours.sort();
    let t: Vec<(String, bool, u32, u32)> = theirs.iter().map(|i| (i.path.clone(), i.is_dir, i.size, i.first)).collect();
    assert_eq!(ours, t, "{tag}: the two readers' listings differ");
    let mut listing = String::new();
    for (item, mine) in theirs.iter().zip(&ours) {
        if item.is_dir {
            listing.push_str(&format!("{}\t<DIR>\t-\n", item.path));
        } else {
            let a = read_all(v, &mine.0);
            let b = r.read_file(item).unwrap_or_else(|e| panic!("{tag}: {}: {e}", item.path));
            assert!(a == b, "{tag}: {}'s bytes differ between the two readers", item.path);
            listing.push_str(&format!("{}\t{}\t{}\n", item.path, item.size, sha256_hex(&a)));
        }
    }
    assert!(r.fat_copies_agree(), "{tag}: the FAT copies differ");
    assert_eq!(r.free_clusters(), v.free_clusters(), "{tag}: the free count by the second reader's scan");
    dump(tag, &dev, &listing);
}

fn dump(tag: &str, dev: &Sparse, listing: &str) {
    if std::env::var_os("RHU_FAT_SECOND_READER_DUMP").is_none() {
        return;
    }
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target").join("fat-second-reader");
    std::fs::create_dir_all(&dir).unwrap();
    let mut raw = vec![0u8; dev.sector_count() as usize * 512];
    for i in 0..dev.sector_count() {
        let s = dev.sector(i);
        if s != [0; 512] {
            raw[i as usize * 512..(i as usize + 1) * 512].copy_from_slice(&s);
        }
    }
    std::fs::write(dir.join(format!("{tag}.img")), &raw).unwrap();
    std::fs::write(dir.join(format!("{tag}.list")), listing).unwrap();
}

fn clock_2026() -> Option<Civil> {
    Some(Civil { year: 2026, month: 10, day: 15, hour: 12, minute: 34, second: 57 })
}
fn clock_later() -> Option<Civil> {
    Some(Civil { year: 2027, month: 1, day: 2, hour: 3, minute: 4, second: 6 })
}
fn clock_1970() -> Option<Civil> {
    Some(Civil { year: 1970, month: 6, day: 1, hour: 0, minute: 0, second: 0 })
}
fn clock_2200() -> Option<Civil> {
    Some(Civil { year: 2200, month: 1, day: 1, hour: 0, minute: 0, second: 0 })
}

#[test]
fn fat_create_write_sync_and_the_reread_after_a_remount() {
    for name in GOLDENS {
        let mut v = mount(name);
        let cb = v.geometry().cluster_bytes;
        let free0 = v.free_clusters();
        let small = recipe_bytes("new-file.txt", 10_000);
        let deep = recipe_bytes("deep2.bin", 70_000);
        create(&mut v, "/new-file.txt", &small);
        create(&mut v, &format!("{DEEP_DIR}/deep2.bin"), &deep);
        assert_eq!(v.open(b"/new-file.txt", RW | O_CREATE | O_EXCL).err(), Some(Error::Exists), "{name}: EXCL on an existing name");
        assert_eq!(v.open(b"/nope.txt", RW).err(), Some(Error::NotFound), "{name}: no CREATE, no file");
        assert_eq!(v.open(b"/a", RW | O_CREATE).err(), Some(Error::IsDir), "{name}");
        assert_eq!(v.open(b"/nodir/x.txt", RW | O_CREATE).err(), Some(Error::NotFound), "{name}: a missing parent");
        assert_eq!(v.open(b"/hello.txt/x.txt", RW | O_CREATE).err(), Some(Error::NotDir), "{name}: a file as the parent");
        assert_eq!(v.open(b"/x.txt", O_READ | O_TRUNC).err(), Some(Error::Invalid), "{name}: TRUNC needs WRITE");
        assert_eq!(v.open(b"/x.txt", 0).err(), Some(Error::Invalid), "{name}: no access flag");
        let used = 10_000u32.div_ceil(cb) + 70_000u32.div_ceil(cb);
        assert_eq!(v.free_clusters(), free0 - used, "{name}: the clusters taken");
        assert!(small == read_all(&mut v, "/new-file.txt") && deep == read_all(&mut v, &format!("{DEEP_DIR}/deep2.bin")), "{name}: read back before the remount");
        second_reader(&mut v, &format!("{name}-11a"));
        // The remount: the entries, the sizes, the bytes and the free count from the device alone.
        let dev = v.unmount().unwrap();
        let mut v = mount_dev(dev, 65536).unwrap();
        assert_eq!(v.free_clusters(), free0 - used, "{name}: the free count after the remount");
        let s = v.stat(b"/new-file.txt").unwrap();
        assert!(s.size == 10_000 && !s.is_dir && s.attr & 0x20 != 0 && s.first >= 2, "{name}: the stat after the remount {s:?}");
        assert!(small == read_all(&mut v, "/new-file.txt"), "{name}: the bytes after the remount");
        assert!(deep == read_all(&mut v, &format!("{DEEP_DIR}/deep2.bin")), "{name}: the deep file after the remount");
        assert_eq!(v.stat(format!("{DEEP_DIR}/deep2.bin").as_bytes()).unwrap().size, 70_000);
        assert!(list(&mut v, "/").iter().any(|e| e.0 == "new-file.txt" && e.4), "{name}: the new name is a long name");
        // O_CREATE without EXCL opens the existing file; O_TRUNC empties it and frees its clusters.
        let mut h = v.open(b"/new-file.txt", RW | O_CREATE).unwrap();
        assert_eq!(h.size(), 10_000);
        v.close(&mut h).unwrap();
        let mut h = v.open(b"/new-file.txt", RW | O_TRUNC).unwrap();
        assert!(h.size() == 0 && h.first_cluster() == 0, "{name}: truncated at the open");
        v.close(&mut h).unwrap();
        assert_eq!(v.free_clusters(), free0 - 70_000u32.div_ceil(cb), "{name}: the truncated file's clusters freed");
        assert_eq!(v.stat(b"/new-file.txt").unwrap().first, 0);
        second_reader(&mut v, &format!("{name}-11b"));
    }
}

#[test]
fn fat_append_and_truncate() {
    for name in ["fat32-64m", "fat12-1m"] {
        let mut v = mount(name);
        assert_eq!(v.geometry().cluster_bytes, 512, "{name}: 512-byte clusters make every sector a cluster boundary");
        let free0 = v.free_clusters();
        let a = recipe_bytes("grow-a", 700);
        let b = recipe_bytes("grow-b", 900);
        create(&mut v, "/grow.bin", &a);
        let mut h = v.open(b"/grow.bin", RW | O_APPEND).unwrap();
        assert_eq!(v.seek(&mut h, 0, SEEK_SET).unwrap(), 0);
        assert_eq!(v.write(&mut h, &b).unwrap(), 900, "{name}: APPEND writes at the end whatever the position");
        assert_eq!(h.position(), 1600);
        v.close(&mut h).unwrap();
        let mut want = a.clone();
        want.extend_from_slice(&b);
        assert!(read_all(&mut v, "/grow.bin") == want, "{name}: the appended bytes");
        assert_eq!(v.free_clusters(), free0 - 4, "{name}: four clusters of 512");
        let first = v.stat(b"/grow.bin").unwrap().first;
        assert_eq!(v.chain_length(first).unwrap(), 4);
        second_reader(&mut v, &format!("{name}-12a"));
        // Shorter: the tail freed, the kept tail's entry END.
        let mut h = v.open(b"/grow.bin", RW).unwrap();
        v.truncate(&mut h, 1000).unwrap();
        assert_eq!(h.size(), 1000);
        v.close(&mut h).unwrap();
        assert_eq!(v.free_clusters(), free0 - 2, "{name}: two clusters kept");
        assert_eq!(v.chain_length(first).unwrap(), 2);
        assert!(read_all(&mut v, "/grow.bin") == want[..1000], "{name}: the kept bytes");
        second_reader(&mut v, &format!("{name}-12b"));
        // Longer: zero-filled.
        let mut h = v.open(b"/grow.bin", RW).unwrap();
        v.truncate(&mut h, 3000).unwrap();
        v.close(&mut h).unwrap();
        let got = read_all(&mut v, "/grow.bin");
        assert!(got.len() == 3000 && got[..1000] == want[..1000] && got[1000..].iter().all(|&x| x == 0), "{name}: the zero fill");
        assert_eq!(v.free_clusters(), free0 - 6);
        assert_eq!(v.chain_length(first).unwrap(), 6);
        second_reader(&mut v, &format!("{name}-12c"));
        // A seek past the end then a write: the gap zero-filled, the skipped clusters allocated.
        let mut h = v.open(b"/grow.bin", RW).unwrap();
        assert_eq!(v.seek(&mut h, 5000, SEEK_SET).unwrap(), 5000);
        assert_eq!(v.write(&mut h, &a[..100]).unwrap(), 100);
        assert_eq!(h.size(), 5100);
        v.close(&mut h).unwrap();
        let got = read_all(&mut v, "/grow.bin");
        assert!(got.len() == 5100 && got[3000..5000].iter().all(|&x| x == 0) && got[5000..] == a[..100], "{name}: the gap");
        assert_eq!(v.free_clusters(), free0 - 10);
        second_reader(&mut v, &format!("{name}-12d"));
        // A write inside the file through the cache, then the direct path over the same sector sees it.
        let mut h = v.open(b"/grow.bin", RW).unwrap();
        v.seek(&mut h, 1234, SEEK_SET).unwrap();
        assert_eq!(v.write(&mut h, &b[..100]).unwrap(), 100);
        assert_eq!(v.seek(&mut h, 0, SEEK_CUR).unwrap(), 1334);
        let mut back = vec![0u8; 100];
        v.seek(&mut h, 1234, SEEK_SET).unwrap();
        assert_eq!(v.read(&mut h, &mut back).unwrap(), 100, "{name}: read through the cache after a partial write");
        assert!(back == b[..100]);
        v.seek(&mut h, 1024, SEEK_SET).unwrap();
        let mut page = vec![0u8; 1024];
        assert_eq!(v.read(&mut h, &mut page).unwrap(), 1024, "{name}: the direct path after a partial write");
        assert!(page[210..310] == b[..100], "{name}: the direct path sees the cached partial write");
        v.close(&mut h).unwrap();
        // To zero: everything freed, the first cluster 0.
        let mut h = v.open(b"/grow.bin", RW).unwrap();
        v.truncate(&mut h, 0).unwrap();
        assert!(h.size() == 0 && h.first_cluster() == 0);
        assert_eq!(v.read(&mut h, &mut back).unwrap(), 0);
        v.close(&mut h).unwrap();
        assert_eq!(v.free_clusters(), free0, "{name}: every cluster back");
        assert_eq!(v.stat(b"/grow.bin").unwrap().first, 0);
        // A read-only handle refuses the write and the truncate; a write-only one the read.
        let mut h = v.open(b"/grow.bin", O_READ).unwrap();
        assert_eq!(v.write(&mut h, &a), Err(Error::BadHandle));
        assert_eq!(v.truncate(&mut h, 5), Err(Error::BadHandle));
        v.close(&mut h).unwrap();
        let mut h = v.open(b"/grow.bin", O_WRITE).unwrap();
        assert_eq!(v.read(&mut h, &mut back), Err(Error::BadHandle), "{name}: a write-only handle refuses the read");
        v.close(&mut h).unwrap();
        second_reader(&mut v, &format!("{name}-12e"));
    }
}

#[test]
fn fat_alias_generation_unique_within_a_directory() {
    let mut v = mount("fat16-32m");
    v.mkdir(b"/aliases").unwrap();
    for i in 1..=12 {
        create(&mut v, &format!("/aliases/new file {i}.txt"), &recipe_bytes("x", 4));
    }
    let want: Vec<[u8; 11]> = vec![*b"NEWFIL~1TXT", *b"NEWFIL~2TXT", *b"NEWFIL~3TXT", *b"NEWFIL~4TXT", *b"NEWFIL~5TXT", *b"NEWFIL~6TXT", *b"NEWFIL~7TXT", *b"NEWFIL~8TXT", *b"NEWFIL~9TXT", *b"NEWFI~10TXT", *b"NEWFI~11TXT", *b"NEWFI~12TXT"];
    let mut d = v.opendir(b"/aliases").unwrap();
    let mut e = Entry::EMPTY;
    let mut shorts = vec![];
    let mut longs = vec![];
    while v.readdir(&mut d, &mut e).unwrap() {
        if e.name() == b"." || e.name() == b".." {
            continue;
        }
        shorts.push(e.short);
        longs.push((String::from_utf8(e.name().to_vec()).unwrap(), e.long, e.set_len));
    }
    v.closedir(&mut d).unwrap();
    assert_eq!(shorts, want, "the aliases ~1 to ~9 then ~10 with the base cut shorter");
    for (i, l) in longs.iter().enumerate() {
        assert_eq!((l.0.as_str(), l.1, l.2), (format!("new file {}.txt", i + 1).as_str(), true, 2), "entry {i}: a long name of 14 or 15 units in two long entries");
    }
    // An upper-case 8.3 name gets no long entry; a lower-case one a set and an alias; the lossy replacements.
    create(&mut v, "/aliases/README.TXT", &[1, 2, 3]);
    create(&mut v, "/aliases/Readme2.txt", &[4]);
    create(&mut v, "/aliases/a+b.c", &[5]);
    create(&mut v, &format!("/aliases/{}", e_acute_name()), &[6]);
    create(&mut v, "/aliases/.hidden", &[7]);
    create(&mut v, "/aliases/a.b.c.d.txt", &[8]);
    create(&mut v, "/aliases/thirteen.unit", &[9]);
    let mut m: std::collections::BTreeMap<String, ([u8; 11], bool, u8)> = Default::default();
    let mut d = v.opendir(b"/aliases").unwrap();
    while v.readdir(&mut d, &mut e).unwrap() {
        m.insert(String::from_utf8(e.name().to_vec()).unwrap(), (e.short, e.long, e.set_len));
    }
    v.closedir(&mut d).unwrap();
    assert_eq!(m["README.TXT"], (*b"README  TXT", false, 0), "an upper-case 8.3 name: the short entry alone");
    assert_eq!(m["Readme2.txt"], (*b"README~1TXT", true, 1), "a lower-case name: a set and an alias");
    assert_eq!(m["a+b.c"], (*b"A_B~1   C  ", true, 1), "the plus replaced");
    assert_eq!(m[e_acute_name()], (*b"_T_~1   TXT", true, 1), "the Latin-1 letters replaced");
    assert_eq!(m[".hidden"], (*b"HIDDEN~1   ", true, 1), "the leading period dropped");
    assert_eq!(m["a.b.c.d.txt"], (*b"ABCD~1  TXT", true, 1), "the periods dropped but the last");
    assert_eq!(m["thirteen.unit"], (*b"THIRTE~1UNI", true, 1), "thirteen units fill one long entry exactly");
    // The raw long entry of the thirteen-unit name: no terminator, no padding (specification 7.3).
    let t = find_entry(&mut v, "/aliases", "thirteen.unit");
    v.sync().unwrap();
    let raw = v.device().sector(t.set_start.0);
    let o = t.set_start.1 as usize;
    assert_eq!((raw[o], raw[o + 11], raw[o + 13]), (0x41, 0x0F, crate::fat::checksum(b"THIRTE~1UNI")), "the one long entry: ordinal 1 and last, the attribute, the checksum");
    assert_eq!(u16::from_le_bytes([raw[o + 30], raw[o + 31]]), b't' as u16, "the thirteenth unit fills the entry");
    // A longer name: the terminator then 0xFFFF padding in its last entry.
    let r2 = find_entry(&mut v, "/aliases", "Readme2.txt");
    let raw = v.device().sector(r2.set_start.0);
    let o = r2.set_start.1 as usize;
    assert_eq!((u16::from_le_bytes([raw[o + 24], raw[o + 25]]), u16::from_le_bytes([raw[o + 28], raw[o + 29]]), u16::from_le_bytes([raw[o + 30], raw[o + 31]])), (b't' as u16, 0, 0xFFFF), "the eleventh unit, the terminator at unit 12, the padding after");
    // The alias's checksum ties the set: a long entry's checksum byte patched, the set is dropped and the alias serves.
    let e7 = find_entry(&mut v, "/aliases", "new file 7.txt");
    let mut dev = v.device().clone();
    let mut sec = dev.sector(e7.set_start.0);
    sec[e7.set_start.1 as usize + 13] ^= 0x40;
    dev.patch(e7.set_start.0, 0, &sec);
    let mut v2 = mount_dev(dev, 65536).unwrap();
    let l = list(&mut v2, "/aliases");
    assert!(l.iter().any(|x| x.0 == "NEWFIL~7.TXT" && !x.4), "the alias served when the set's checksum fails");
    assert!(!l.iter().any(|x| x.0 == "new file 7.txt"));
    // The thirteenth alias skips every short name in use; a name differing by case alone conflicts (specification 7.4); the alias opens the file.
    create(&mut v, "/aliases/new file 13.txt", &[1]);
    assert_eq!(find_entry(&mut v, "/aliases", "new file 13.txt").short, *b"NEWFI~13TXT");
    assert_eq!(v.open(b"/aliases/NEW FILE 1.TXT", RW | O_CREATE | O_EXCL).err(), Some(Error::Exists));
    assert_eq!(v.open(b"/aliases/newfil~1.txt", O_READ).map(|h| h.size()), Ok(4), "the alias opens the file too");
    second_reader(&mut v, "fat16-32m-13");
}

#[test]
fn fat_unlink_mkdir_rmdir_and_rename() {
    for name in GOLDENS {
        let mut v = mount(name);
        let cb = v.geometry().cluster_bytes;
        let free0 = v.free_clusters();
        let big_len = if name == "fat12-1m" { 262_144u32 } else { 1 << 20 };
        let big = recipe_bytes("big.bin", big_len as usize);
        assert_eq!(v.unlink(b"/a"), Err(Error::IsDir), "{name}");
        assert_eq!(v.unlink(b"/"), Err(Error::IsDir), "{name}");
        assert_eq!(v.unlink(b"/nope"), Err(Error::NotFound), "{name}");
        v.unlink(b"/hello.txt").unwrap();
        assert_eq!(v.stat(b"/hello.txt"), Err(Error::NotFound), "{name}: gone");
        assert_eq!(v.free_clusters(), free0 + 1, "{name}: hello.txt's one cluster freed");
        assert!(!list(&mut v, "/").iter().any(|e| e.0 == "hello.txt"));
        v.mkdir(b"/d1").unwrap();
        assert_eq!(v.mkdir(b"/d1"), Err(Error::Exists), "{name}");
        assert_eq!(v.mkdir(b"/"), Err(Error::Exists), "{name}");
        assert_eq!(v.mkdir(b"/nope/d"), Err(Error::NotFound), "{name}");
        assert_eq!(v.mkdir(b"/big.bin/d"), Err(Error::NotDir), "{name}");
        let d1 = list(&mut v, "/d1");
        assert_eq!(names(&d1), [".", ".."], "{name}");
        let d1_first = v.stat(b"/d1").unwrap().first;
        assert!(d1[0].1 && d1[0].3 == d1_first && d1[1].1 && d1[1].3 == 0, "{name}: '.' the own cluster, '..' 0 for the root");
        assert_eq!(v.free_clusters(), free0, "{name}: one cluster for d1");
        v.mkdir(b"/d1/d2").unwrap();
        let d2 = list(&mut v, "/d1/d2");
        assert!(d2[1].0 == ".." && d2[1].3 == d1_first, "{name}: '..' names the parent");
        assert_eq!(v.stat(b"/d1/d2/..").unwrap().first, d1_first);
        assert_eq!(v.rmdir(b"/d1"), Err(Error::NotEmpty), "{name}");
        assert_eq!(v.rmdir(b"/big.bin"), Err(Error::NotDir), "{name}");
        assert_eq!(v.rmdir(b"/"), Err(Error::Invalid), "{name}");
        assert_eq!(v.rmdir(b"/d1/.."), Err(Error::Invalid), "{name}");
        assert_eq!(v.rmdir(b"/nope"), Err(Error::NotFound), "{name}");
        second_reader(&mut v, &format!("{name}-14a"));
        // A rename across directories keeps the chain and the size.
        let big_first = v.stat(b"/big.bin").unwrap().first;
        v.rename(b"/big.bin", b"/d1/d2/moved.bin").unwrap();
        assert_eq!(v.stat(b"/big.bin"), Err(Error::NotFound), "{name}");
        let s = v.stat(b"/d1/d2/moved.bin").unwrap();
        assert!(s.first == big_first && s.size == big_len, "{name}: moved with its chain");
        assert!(read_all(&mut v, "/d1/d2/moved.bin") == big, "{name}: the bytes through the moved entry");
        assert_eq!(v.free_clusters(), free0 - 1, "{name}: d2's cluster, nothing else");
        // A directory moved across parents has its '..' rewritten.
        v.rename(b"/d1/d2", b"/d2x").unwrap();
        assert_eq!(v.stat(b"/d2x/..").unwrap().first, v.geometry().root_clus, "{name}: '..' now the root");
        assert_eq!(find_entry(&mut v, "/d2x", "..").first, 0, "{name}: the raw '..' entry 0 for the root");
        assert!(list(&mut v, "/d2x").iter().any(|e| e.0 == "moved.bin"), "{name}");
        assert_eq!(names(&list(&mut v, "/d1")), [".", ".."], "{name}: d1 empty again");
        v.rename(b"/d2x", b"/d1/d2").unwrap();
        assert_eq!(v.stat(b"/d1/d2/..").unwrap().first, d1_first, "{name}: '..' d1 again");
        // Onto an existing file: replaced in two steps; onto a directory Exists; into itself Invalid; onto itself a no-op.
        v.rename(b"/d1/d2/moved.bin", b"/empty.bin").unwrap();
        let s = v.stat(b"/empty.bin").unwrap();
        assert!(s.size == big_len && s.first == big_first, "{name}: the replace");
        assert_eq!(v.rename(b"/d1", b"/a"), Err(Error::Exists), "{name}: onto a directory");
        assert_eq!(v.rename(b"/d1", b"/d1/d2/x"), Err(Error::Invalid), "{name}: into its own subtree");
        assert_eq!(v.rename(b"/d1", b"/empty.bin"), Err(Error::NotDir), "{name}: a directory onto a file");
        assert_eq!(v.rename(b"/empty.bin", b"/EMPTY.BIN"), Ok(()), "{name}: onto itself by case");
        assert_eq!(v.rename(b"/nope", b"/x"), Err(Error::NotFound), "{name}");
        assert_eq!(v.rename(b"/", b"/x"), Err(Error::Invalid), "{name}");
        v.rename(b"/empty.bin", b"/renamed.bin").unwrap();
        assert!(read_all(&mut v, "/renamed.bin") == big, "{name}");
        second_reader(&mut v, &format!("{name}-14b"));
        v.rmdir(b"/d1/d2").unwrap();
        v.rmdir(b"/d1").unwrap();
        assert_eq!(v.stat(b"/d1"), Err(Error::NotFound), "{name}");
        v.unlink(b"/renamed.bin").unwrap();
        assert_eq!(v.free_clusters(), free0 + 1 + big_len / cb, "{name}: hello.txt's and big.bin's clusters free, empty.bin had none, the directories' back");
        second_reader(&mut v, &format!("{name}-14c"));
    }
}

#[test]
fn fat_directory_grows_past_one_cluster() {
    let mut v = mount("fat32-64m");
    let free0 = v.free_clusters();
    v.mkdir(b"/grow").unwrap();
    let first = v.stat(b"/grow").unwrap().first;
    assert_eq!(v.chain_length(first).unwrap(), 1);
    for i in 1..=14 {
        create(&mut v, &format!("/grow/G{i:02}.TXT"), &[i as u8]);
    }
    assert_eq!(v.chain_length(first).unwrap(), 1, "sixteen entries fill the one 512-byte cluster: the dots and fourteen short entries");
    create(&mut v, "/grow/G15.TXT", &[15]);
    assert_eq!(v.chain_length(first).unwrap(), 2, "the seventeenth entry takes a new cluster");
    let second = v.next_cluster(first).unwrap().unwrap();
    let lba = v.geometry().first_sector_of(second);
    v.sync().unwrap();
    let s = v.device().sector(lba);
    assert!(s[0] == b'G' && s[32..].iter().all(|&b| b == 0), "the new cluster: the one entry, the rest zero");
    for i in 16..=40 {
        create(&mut v, &format!("/grow/G{i:02}.TXT"), &[i as u8]);
    }
    let l = list(&mut v, "/grow");
    assert_eq!(l.len(), 42);
    for i in 1..=40 {
        assert!(l.iter().any(|e| e.0 == format!("G{i:02}.TXT") && e.2 == 1), "G{i:02}.TXT listed");
    }
    assert_eq!(v.chain_length(first).unwrap(), 3, "42 entries in three clusters of sixteen");
    assert_eq!(v.free_clusters(), free0 - 3 - 40, "three directory clusters and forty one-byte files");
    // The root is a chain on FAT32: it grows the same way under long names (two entries each).
    let root_len = v.chain_length(v.geometry().root_clus).unwrap();
    for i in 1..=40 {
        create(&mut v, &format!("/root-grow-{i}.txt"), &[1]);
    }
    assert_eq!(list(&mut v, "/").iter().filter(|e| e.0.starts_with("root-grow-")).count(), 40);
    assert!(v.chain_length(v.geometry().root_clus).unwrap() >= root_len + 5, "the root grew by the eighty entries");
    second_reader(&mut v, "fat32-64m-15");
    // The fixed root of FAT12 cannot grow: NoSpace when its entries are taken, nothing else moved.
    let mut v = mount("fat12-1m");
    let g = *v.geometry();
    let free12 = v.free_clusters();
    let mut created = 0;
    let r = loop {
        match v.open(format!("/R{created:05}").as_bytes(), RW | O_CREATE | O_EXCL) {
            Ok(mut h) => {
                v.close(&mut h).unwrap();
                created += 1;
            }
            Err(e) => break e,
        }
    };
    assert_eq!(r, Error::NoSpace, "the fixed root full");
    assert_eq!(created + 25, g.root_ent_cnt, "every root entry taken: the twenty-five raw entries the recipe's names hold and the rest ours");
    assert_eq!(v.free_clusters(), free12, "no cluster taken by empty files or the fixed root");
    assert_eq!(v.mkdir(b"/rootfull"), Err(Error::NoSpace), "a directory needs a root entry too");
    for i in 0..created {
        v.unlink(format!("/R{i:05}").as_bytes()).unwrap();
    }
    assert_eq!(list(&mut v, "/").len(), 9);
    second_reader(&mut v, "fat12-1m-15");
}

#[test]
fn fat_volume_filled_to_no_space() {
    let mut v = mount("fat12-1m");
    let free0 = v.free_clusters();
    let cb = v.geometry().cluster_bytes;
    let room = free0 * cb;
    let data = recipe_bytes("fill.bin", room as usize + 100_000);
    let mut h = v.open(b"/fill.bin", RW | O_CREATE | O_EXCL).unwrap();
    assert_eq!(v.write(&mut h, &data).unwrap(), room as usize, "the partial count: every free cluster filled");
    assert_eq!(v.write(&mut h, &data[room as usize..]), Err(Error::NoSpace), "then NoSpace");
    assert_eq!(v.free_clusters(), 0);
    assert_eq!(h.size(), room);
    v.close(&mut h).unwrap();
    assert_eq!(v.stat(b"/fill.bin").unwrap().size, room, "the size consistent after the close");
    assert!(read_all(&mut v, "/fill.bin") == data[..room as usize], "the bytes that fit");
    assert_eq!(v.statfs().free, 0);
    assert_eq!(v.mkdir(b"/nospace"), Err(Error::NoSpace), "no cluster for a directory");
    let mut h0 = v.open(b"/zero.txt", RW | O_CREATE | O_EXCL).unwrap();
    assert_eq!(v.write(&mut h0, &[1, 2, 3]), Err(Error::NoSpace), "no cluster for the first byte");
    v.close(&mut h0).unwrap();
    assert_eq!(v.stat(b"/zero.txt").unwrap().size, 0, "the empty file stands in the fixed root");
    second_reader(&mut v, "fat12-1m-16a");
    v.unlink(b"/zero.txt").unwrap();
    v.unlink(b"/fill.bin").unwrap();
    assert_eq!(v.free_clusters(), free0, "the count back");
    assert_eq!(v.stat(b"/fill.bin"), Err(Error::NotFound));
    create(&mut v, "/after.bin", &recipe_bytes("after.bin", 3 * cb as usize));
    assert_eq!(v.free_clusters(), free0 - 3, "allocation works again after the fill");
    second_reader(&mut v, "fat12-1m-16b");
}

#[test]
fn fat_fat_copies_and_the_fsinfo_written_in_order() {
    let base = golden("fat32-64m");
    let mut v = mount("fat32-64m");
    let g = *v.geometry();
    let fsi = base.sector(g.fsinfo);
    create(&mut v, "/order.bin", &recipe_bytes("order.bin", 200_000));
    v.sync().unwrap();
    assert!(Reader::open(v.device()).unwrap().fat_copies_agree(), "mirroring on: both copies written");
    let now = v.device().sector(g.fsinfo);
    assert_eq!(u32::from_le_bytes(now[488..492].try_into().unwrap()), v.free_clusters(), "FSI_Free_Count written");
    assert_eq!(u32::from_le_bytes(now[492..496].try_into().unwrap()), v.next_free_hint(), "FSI_Nxt_Free written");
    assert!(now[..488] == fsi[..488] && now[496..] == fsi[496..], "the sector's other bytes as read");
    assert_eq!(v.device().trace.iter().filter(|t| t.0 == Op::Write && t.1 == g.fsinfo).count(), 1, "the FSInfo written once, at the close's sync");
    // The END-before-link order from the trace: the FAT sector holding the first cluster's END reached the device
    // before the directory sector holding the entry that points at it; across FAT sectors, the sector holding the
    // next cluster's END before the final write of the sector holding the link to it.
    let first = v.stat(b"/order.bin").unwrap().first;
    let tr = v.device().trace.clone();
    let fat_lba = |c: u32| g.rsvd + c * 4 / 512;
    let root_lba = find_entry(&mut v, "/", "order.bin").place.0;
    let first_write = |lba: u32| tr.iter().position(|t| t.0 == Op::Write && t.1 <= lba && lba < t.1 + t.2).unwrap_or_else(|| panic!("sector {lba} never written"));
    let last_write = |lba: u32| tr.iter().rposition(|t| t.0 == Op::Write && t.1 <= lba && lba < t.1 + t.2).unwrap();
    assert!(first_write(fat_lba(first)) < first_write(root_lba), "the first cluster's END on the device before the entry that points at it");
    let mut c = first;
    let mut crossings = 0;
    while let Some(n) = v.next_cluster(c).unwrap() {
        if fat_lba(n) != fat_lba(c) {
            crossings += 1;
            assert!(first_write(fat_lba(n)) < last_write(fat_lba(c)), "the END in the next FAT sector before the link's sector's final write");
        }
        c = n;
    }
    assert!(crossings >= 3, "{crossings} FAT sector crossings in 391 clusters");
    second_reader(&mut v, "fat32-64m-18");
    // Mirroring off (ExtFlags bit 7, the active FAT 0): the active copy alone written, the second as the golden's.
    let mut d = golden("fat32-64m");
    d.patch(0, 40, &0x0080u16.to_le_bytes());
    let mut v = mount_dev(d, 65536).unwrap();
    assert!(!v.geometry().mirroring && v.geometry().active_fat == 0);
    create(&mut v, "/single.bin", &recipe_bytes("single.bin", 50_000));
    v.sync().unwrap();
    for s in 0..g.fat_sz {
        assert_eq!(v.device().sector(g.rsvd + g.fat_sz + s), base.sector(g.rsvd + g.fat_sz + s), "the second copy untouched at its sector {s}");
    }
    assert!(!Reader::open(v.device()).unwrap().fat_copies_agree(), "the copies differ where the active one moved");
    assert!(read_all(&mut v, "/single.bin") == recipe_bytes("single.bin", 50_000));
    // The active FAT 1 with mirroring off: the reads and the writes go to the second copy alone.
    let mut d = golden("fat32-64m");
    d.patch(0, 40, &0x0081u16.to_le_bytes());
    let mut v = mount_dev(d, 65536).unwrap();
    assert_eq!(v.geometry().active_fat, 1);
    create(&mut v, "/second.bin", &recipe_bytes("second.bin", 5_000));
    v.sync().unwrap();
    for s in 0..g.fat_sz {
        assert_eq!(v.device().sector(g.rsvd + s), base.sector(g.rsvd + s), "the first copy untouched at its sector {s}");
    }
    assert!(read_all(&mut v, "/second.bin") == recipe_bytes("second.bin", 5_000));
    // FAT12 and FAT16: both copies equal after writes and frees too; no FSInfo there.
    for name in ["fat12-1m", "fat16-32m"] {
        let mut v = mount(name);
        create(&mut v, "/copies.bin", &recipe_bytes("copies.bin", 20_000));
        v.unlink(b"/hello.txt").unwrap();
        v.sync().unwrap();
        assert!(Reader::open(v.device()).unwrap().fat_copies_agree(), "{name}");
        let rsvd = v.geometry().rsvd;
        assert!(!v.device().trace.iter().any(|t| t.0 == Op::Write && t.1 < rsvd), "{name}: nothing written below the FATs");
    }
}

#[test]
fn fat_timestamps_from_the_calendar() {
    let mut v = mount("fat16-32m");
    v.set_clock(clock_2026);
    create(&mut v, "/stamped.txt", &[1, 2, 3]);
    let s = v.stat(b"/stamped.txt").unwrap();
    let midnight = (days_from_civil(2026, 10, 15) - days_from_civil(1970, 1, 1)) as i64 * 86_400;
    assert_eq!(s.mtime, midnight + 12 * 3600 + 34 * 60 + 56, "the write time at two-second resolution");
    assert_eq!(s.ctime, s.mtime, "the creation time equal at creation");
    assert_eq!(s.atime, midnight, "the access date at midnight");
    let e = find_entry(&mut v, "/", "stamped.txt");
    assert_eq!(e.wrt_date, (46 << 9) | (10 << 5) | 15);
    assert_eq!(e.wrt_time, (12 << 11) | (34 << 5) | 28);
    assert_eq!((e.crt_date, e.crt_time, e.acc_date, e.crt_tenth, e.ntres), (e.wrt_date, e.wrt_time, e.wrt_date, 0, 0));
    // A later write moves the write time and the access date, never the creation time.
    v.set_clock(clock_later);
    let mut h = v.open(b"/stamped.txt", RW | O_APPEND).unwrap();
    v.write(&mut h, &[4]).unwrap();
    v.close(&mut h).unwrap();
    let s2 = v.stat(b"/stamped.txt").unwrap();
    let later_midnight = (days_from_civil(2027, 1, 2) - days_from_civil(1970, 1, 1)) as i64 * 86_400;
    assert_eq!(s2.mtime, later_midnight + 3 * 3600 + 4 * 60 + 6);
    assert_eq!(s2.ctime, s.ctime);
    assert_eq!(s2.atime, later_midnight);
    // The floor: no clock, a clock before 1980, a clock past 2107.
    for (what, clock) in [("unset", None), ("1970", Some(clock_1970 as fn() -> Option<Civil>)), ("2200", Some(clock_2200 as fn() -> Option<Civil>))] {
        let mut v = mount("fat12-1m");
        if let Some(c) = clock {
            v.set_clock(c);
        }
        create(&mut v, &format!("/floor-{what}.txt"), &[1]);
        let s = v.stat(format!("/floor-{what}.txt").as_bytes()).unwrap();
        assert_eq!((s.mtime, s.ctime, s.atime), (315_532_800, 315_532_800, 315_532_800), "the floor, midnight of 1 January 1980 for the clock {what}");
        let e = find_entry(&mut v, "/", &format!("floor-{what}.txt"));
        assert_eq!((e.wrt_date, e.wrt_time), ((1 << 5) | 1, 0));
    }
    // mkdir stamps "." and ".." with the directory's own times (specification 6.5).
    let mut v = mount("fat32-64m");
    v.set_clock(clock_2026);
    v.mkdir(b"/stamped").unwrap();
    let dot = find_entry(&mut v, "/stamped", ".");
    let own = find_entry(&mut v, "/", "stamped");
    assert_eq!((dot.wrt_date, dot.wrt_time, dot.crt_date), (own.wrt_date, own.wrt_time, own.crt_date));
    assert_eq!(own.wrt_time, (12 << 11) | (34 << 5) | 28);
    second_reader(&mut v, "fat32-64m-19");
}

#[test]
fn fat_sector_cache_write_back_and_eviction() {
    let mut v = mount("fat16-32m");
    v.device().trace.clear();
    let s0 = v.cache_stats();
    // A partial write stays in the cache until the sync: no device write before it; at the sync the FAT sector to
    // both copies, the directory sector and the data sector.
    let mut h = v.open(b"/cached.txt", RW | O_CREATE | O_EXCL).unwrap();
    assert_eq!(v.write(&mut h, &[7u8; 100]).unwrap(), 100);
    assert_eq!(v.device().calls(Op::Write), 0, "nothing written before the sync");
    v.sync_file(&mut h).unwrap();
    let w = v.device().calls(Op::Write);
    assert!((4..=6).contains(&w), "{w} writes at the sync");
    assert!(v.cache_stats().flushes - s0.flushes >= 3);
    let mut buf = [0u8; 50];
    v.seek(&mut h, 10, SEEK_SET).unwrap();
    v.read(&mut h, &mut buf[..1]).unwrap();
    let a = v.cache_stats();
    v.read(&mut h, &mut buf[..1]).unwrap();
    let b = v.cache_stats();
    assert_eq!((b.hits - a.hits, b.misses - a.misses), (1, 0), "the second read of a sector a hit");
    v.close(&mut h).unwrap();
    // The FAT buffer kept over data buffers: twelve different data sectors through the cache, then the FAT sector again is a hit.
    let big = v.stat(b"/big.bin").unwrap().first;
    let _ = v.fat_get(big).unwrap();
    let before = v.cache_stats();
    let mut h = v.open(b"/big.bin", O_READ).unwrap();
    for k in 0..12 {
        v.seek(&mut h, k * 4096 + 1, SEEK_SET).unwrap();
        v.read(&mut h, &mut buf[..1]).unwrap();
    }
    let mid = v.cache_stats();
    assert!(mid.misses - before.misses >= 12, "twelve data sectors missed");
    assert!(mid.evictions - before.evictions >= 4, "evictions among the data buffers");
    let _ = v.fat_get(big).unwrap();
    let after = v.cache_stats();
    assert_eq!((after.hits - mid.hits, after.misses - mid.misses), (1, 0), "the FAT sector survived the data traffic");
    v.close(&mut h).unwrap();
    // Dirty data buffers are flushed on eviction alone before any sync: nine partial writes to nine sectors force some.
    v.device().trace.clear();
    let mut h = v.open(b"/big.bin", RW).unwrap();
    for k in 0..9u32 {
        v.seek(&mut h, (k * 2048 + 5) as i64, SEEK_SET).unwrap();
        v.write(&mut h, &[0xEEu8]).unwrap();
    }
    let evicted = v.device().calls(Op::Write);
    assert!((1..9).contains(&evicted), "{evicted} dirty buffers flushed on eviction before any sync, not all nine");
    v.close(&mut h).unwrap();
    let mut want = recipe_bytes("big.bin", 1 << 20);
    for k in 0..9usize {
        want[k * 2048 + 5] = 0xEE;
    }
    v.device().trace.clear();
    let s = v.cache_stats();
    assert!(read_all(&mut v, "/big.bin") == want, "the nine bytes landed, the rest intact");
    let s2 = v.cache_stats();
    assert!(s2.misses - s.misses <= 8, "the direct path for aligned whole-sector reads: the FAT's sectors alone through the cache");
    assert!(v.device().sectors_moved(Op::Read) >= 2048);
    second_reader(&mut v, "fat16-32m-20");
}

#[test]
fn fat_second_reader_agrees_after_every_write_scenario() {
    for name in GOLDENS {
        let mut v = mount(name);
        v.set_clock(clock_2026);
        let cb = v.geometry().cluster_bytes as usize;
        second_reader(&mut v, &format!("{name}-s0-as-mounted"));
        create(&mut v, "/s1 created.txt", &recipe_bytes("s1", 3 * cb + 17));
        create(&mut v, &format!("{}/s1-deep.bin", &DEEP_DIR[..4]), &recipe_bytes("s1-deep", 2 * cb));
        second_reader(&mut v, &format!("{name}-s1-create"));
        let mut h = v.open(b"/s1 created.txt", RW | O_APPEND).unwrap();
        v.write(&mut h, &recipe_bytes("s2", cb + 1)).unwrap();
        v.close(&mut h).unwrap();
        second_reader(&mut v, &format!("{name}-s2-append"));
        let mut h = v.open(b"/s1 created.txt", RW).unwrap();
        v.truncate(&mut h, (cb + 5) as u32).unwrap();
        v.close(&mut h).unwrap();
        second_reader(&mut v, &format!("{name}-s3-truncate"));
        v.unlink(b"/hello.txt").unwrap();
        v.unlink(format!("{}/s1-deep.bin", &DEEP_DIR[..4]).as_bytes()).unwrap();
        second_reader(&mut v, &format!("{name}-s4-unlink"));
        v.mkdir(b"/s5 dir").unwrap();
        v.mkdir(b"/s5 dir/inner").unwrap();
        create(&mut v, "/s5 dir/inner/leaf.txt", &recipe_bytes("leaf", 77));
        second_reader(&mut v, &format!("{name}-s5-mkdir"));
        v.rename(b"/s5 dir/inner/leaf.txt", b"/s5 dir/moved leaf.txt").unwrap();
        v.rename(b"/s5 dir/inner", b"/a/inner moved").unwrap();
        v.rename(b"/big.bin", b"/s5 dir/big renamed.bin").unwrap();
        second_reader(&mut v, &format!("{name}-s6-rename"));
        v.rmdir(b"/a/inner moved").unwrap();
        v.unlink(b"/s5 dir/moved leaf.txt").unwrap();
        second_reader(&mut v, &format!("{name}-s7-rmdir"));
        if name == "fat12-1m" {
            let mut h = v.open(b"/fill.bin", RW | O_CREATE).unwrap();
            let data = recipe_bytes("fill", 400_000);
            let n = v.write(&mut h, &data).unwrap();
            assert!(n > 0 && v.write(&mut h, &data[n..]) == Err(Error::NoSpace));
            v.close(&mut h).unwrap();
            second_reader(&mut v, &format!("{name}-s8-full"));
        }
        let dev = v.unmount().unwrap();
        let mut v = mount_dev(dev, 65536).unwrap();
        second_reader(&mut v, &format!("{name}-s9-remount"));
    }
}

/// The test's own minimal formatter (host-only): a FAT32 volume of a 32 GB card's shape -- 32 GB at
/// 32 KB clusters -- as a BPB with its backup, an FSInfo with its copy, two FATs and a root cluster over a blank
/// sparse device.
fn format_card_sized(fsinfo_count: u32, hint: u32) -> (Sparse, u32) {
    let total: u32 = 67_108_864;
    let (spc, rsvd, fatsz): (u32, u32, u32) = (64, 32, 8193);
    let mut d = Sparse::blank(total);
    let mut b = [0u8; 512];
    b[0..3].copy_from_slice(&[0xEB, 0x58, 0x90]);
    b[3..11].copy_from_slice(b"RHUTEST ");
    b[11..13].copy_from_slice(&512u16.to_le_bytes());
    b[13] = spc as u8;
    b[14..16].copy_from_slice(&(rsvd as u16).to_le_bytes());
    b[16] = 2;
    b[21] = 0xF8;
    b[32..36].copy_from_slice(&total.to_le_bytes());
    b[36..40].copy_from_slice(&fatsz.to_le_bytes());
    b[44..48].copy_from_slice(&2u32.to_le_bytes());
    b[48..50].copy_from_slice(&1u16.to_le_bytes());
    b[50..52].copy_from_slice(&6u16.to_le_bytes());
    b[66] = 0x29;
    b[71..82].copy_from_slice(b"RHUCARD    ");
    b[82..90].copy_from_slice(b"FAT32   ");
    b[510] = 0x55;
    b[511] = 0xAA;
    d.patch(0, 0, &b);
    d.patch(6, 0, &b);
    let mut f = [0u8; 512];
    f[0..4].copy_from_slice(&0x4161_5252u32.to_le_bytes());
    f[484..488].copy_from_slice(&0x6141_7272u32.to_le_bytes());
    f[488..492].copy_from_slice(&fsinfo_count.to_le_bytes());
    f[492..496].copy_from_slice(&hint.to_le_bytes());
    f[508..512].copy_from_slice(&0xAA55_0000u32.to_le_bytes());
    d.patch(1, 0, &f);
    d.patch(7, 0, &f);
    let mut fat0 = [0u8; 512];
    fat0[0..4].copy_from_slice(&0x0FFF_FFF8u32.to_le_bytes());
    fat0[4..8].copy_from_slice(&0x0FFF_FFFFu32.to_le_bytes());
    fat0[8..12].copy_from_slice(&0x0FFF_FFFFu32.to_le_bytes());
    d.patch(rsvd, 0, &fat0);
    d.patch(rsvd + fatsz, 0, &fat0);
    (d, (total - rsvd - 2 * fatsz) / spc)
}

#[test]
fn fat_card_sized_geometry_over_a_sparse_device() {
    let (d, clusters) = format_card_sized(0xFFFF_FFFF, 0xFFFF_FFFF);
    let v = mount_dev(d, 65536).unwrap();
    let g = *v.geometry();
    assert_eq!((g.kind, g.cluster_bytes, g.count_of_clusters), (Kind::Fat32, 32768, clusters));
    assert!(clusters > 1_000_000, "{clusters} clusters");
    assert!(!v.hint_served(), "the unknown count forces the pass");
    assert_eq!(v.free_clusters(), clusters - 1, "every cluster but the root's free");
    let entry_bytes = (g.max as u64 + 1) * 4;
    assert_eq!(v.pass_calls(), entry_bytes.div_ceil(65536) as u32, "the pass in calls of 64 KB");
    assert!(v.pass_calls() <= 70, "the prediction: about sixty calls for a 32 GB card's FAT, {} here", v.pass_calls());
    assert_eq!(v.next_free_hint(), 3);
    assert_eq!(v.label(), b"RHUCARD");
    // The hint near the end of the volume: a file written there with DIR_FstClusHI in use, the search wrapping to
    // the front when the end is reached; the FSInfo written at the sync; the second reader on the card-sized image.
    let (d, _) = format_card_sized(clusters - 1, g.max - 5);
    let mut v = mount_dev(d, 65536).unwrap();
    assert!(v.hint_served() && v.next_free_hint() == g.max - 5);
    let data = recipe_bytes("end.bin", 10 * 32768);
    create(&mut v, "/end.bin", &data);
    let s = v.stat(b"/end.bin").unwrap();
    assert_eq!(s.first, g.max - 5, "the first cluster at the hint");
    assert!(s.first > 0xFFFF, "DIR_FstClusHI in use");
    let mut chain = vec![s.first];
    let mut c = s.first;
    while let Some(n) = v.next_cluster(c).unwrap() {
        chain.push(n);
        c = n;
    }
    assert_eq!(chain.len(), 10);
    assert_eq!(&chain[..6], &[g.max - 5, g.max - 4, g.max - 3, g.max - 2, g.max - 1, g.max], "to the end");
    assert_eq!(&chain[6..], &[3, 4, 5, 6], "then the wrap to the front, past the root's cluster");
    assert_eq!(v.free_clusters(), clusters - 11);
    assert!(read_all(&mut v, "/end.bin") == data);
    v.sync().unwrap();
    let fsi = v.device().sector(1);
    assert_eq!(u32::from_le_bytes(fsi[488..492].try_into().unwrap()), clusters - 11);
    let r = Reader::open(v.device()).unwrap();
    assert_eq!(r.bits(), 32);
    let items = r.list_all().unwrap();
    assert_eq!(items.len(), 1);
    assert!(items[0].path == "/end.bin" && items[0].first == g.max - 5 && items[0].size == 10 * 32768);
    assert!(r.read_file(&items[0]).unwrap() == data);
    assert!(r.fat_copies_agree());
    assert_eq!(r.free_clusters(), clusters - 11);
    let dev = v.unmount().unwrap();
    let v = mount_dev(dev, 65536).unwrap();
    assert!(v.hint_served() && v.free_clusters() == clusters - 11 && v.next_free_hint() == 7, "the FSInfo's values as written serve the remount");
}

// A PC-formatted card's block 0 is a master boot record,
// the volume behind its first FAT-type entry. Each golden behind a record at a base of its own; the controls.
#[test]
fn fat_mounts_behind_a_partition_table() {
    let m = manifest();
    for (name, base, ptype) in [("fat12-1m", 63u32, 0x01u8), ("fat16-32m", 2048, 0x06), ("fat32-64m", 8192, 0x0C)] {
        let row = m.images.iter().find(|r| r.name == name).unwrap();
        let plain = golden(name);
        let hello = read_all(&mut mount(name), "/hello.txt");
        let dev = plain.behind_table(base, ptype);
        assert_eq!(dev.sector_count(), base + plain.sector_count(), "{name}: the device grows by the base");
        let mut v = mount_dev(dev, 65536).unwrap_or_else(|r| panic!("{name} behind the table at {base}: {}", r.words()));
        let g = *v.geometry();
        assert_eq!(g.base, base, "{name}: the base");
        assert_eq!(g.tot_sec, row.sectors, "{name}: the sectors");
        assert_eq!(g.count_of_clusters, row.clusters, "{name}: the clusters");
        assert_eq!(g.cluster_bytes, row.cluster_bytes, "{name}: the cluster size");
        assert_eq!(v.free_clusters(), row.free, "{name}: the free count through the partition");
        assert_eq!(v.stat(b"/hello.txt").map(|s| s.size), Ok(hello.len() as u32), "{name}: hello.txt behind the table");
        assert_eq!(read_all(&mut v, "/hello.txt"), hello, "{name}: hello.txt's bytes through the partition");
        {
            let t = &v.device().trace;
            assert_eq!(t.iter().filter(|c| c.0 == Op::Read && c.1 == 0).count(), 1, "{name}: the record read once");
            assert!(t.iter().all(|c| c.0 == Op::Sync || c.1 >= base || (c.0 == Op::Read && c.1 == 0 && c.2 == 1)), "{name}: a device call below the base");
        }
        let data = recipe_bytes("behind.bin", 100_000);
        let mut h = v.open(b"/behind.bin", O_WRITE | O_CREATE | O_EXCL).unwrap();
        let mut done = 0;
        while done < data.len() {
            done += v.write(&mut h, &data[done..]).unwrap();
        }
        v.close(&mut h).unwrap();
        v.mkdir(b"/behind.dir").unwrap();
        assert_eq!(read_all(&mut v, "/behind.bin"), data, "{name}: the file written through the partition");
        second_reader(&mut v, &format!("partition-{name}"));
        assert!(v.device().trace.iter().all(|c| c.0 != Op::Write || c.1 >= base), "{name}: a write below the base");
        assert_eq!(v.device().sector(0)[450], ptype, "{name}: the record untouched");
        assert_eq!(v.device().sector(0)[510..512], [0x55, 0xAA], "{name}: the record's signature untouched");
        let dev = v.unmount().unwrap();
        let mut v = mount_dev(dev, 65536).unwrap();
        assert_eq!(v.geometry().base, base, "{name}: the base after a remount");
        assert_eq!(read_all(&mut v, "/behind.bin"), data, "{name}: the file after a remount");
        assert!(v.stat(b"/behind.dir").map(|s| s.is_dir).unwrap_or(false), "{name}: the directory after a remount");
    }
    // The controls, fat16-32m behind the table at 2048.
    let behind = golden("fat16-32m").behind_table(2048, 0x06);
    let mut d = behind.clone();
    d.patch(0, 450, &[0xEE]);
    assert_eq!(refused(d), "GPT: not supported", "a GPT protective entry");
    let mut d = behind.clone();
    d.patch(0, 450, &[0x83]);
    assert_eq!(refused(d), "no FAT partition", "no entry of a FAT type");
    let mut d = behind.clone();
    d.patch(0, 458, &(u32::MAX - 4096).to_le_bytes());
    assert_eq!(refused(d), "bad partition table", "a span past the device");
    let mut d = behind.clone();
    d.patch(0, 454, &0u32.to_le_bytes());
    assert_eq!(refused(d), "bad partition table", "a start of 0");
    let mut d = behind.clone();
    d.patch(0, 458, &0u32.to_le_bytes());
    assert_eq!(refused(d), "bad partition table", "a count of 0");
    let mut d = behind.clone();
    d.patch(0, 450, &[0x07]);
    d.patch(2048, 3, b"EXFAT   ");
    d.patch(2048, 11, &[0, 0]);
    assert_eq!(refused(d), "exFAT: not supported", "an exFAT volume behind a type-0x07 entry");
    let mut d = behind.clone();
    d.patch(0, 450, &[0x07]);
    assert_eq!(refused(d), "mounted", "a type-0x07 entry over the FAT16 volume: the boot sector decides");
    let mut d = behind.clone();
    let mut entry = [0u8; 16];
    entry.copy_from_slice(&behind.sector(0)[446..462]);
    d.patch(0, 450, &[0x83]);
    d.patch(0, 462, &entry);
    assert_eq!(mount_dev(d, 65536).map(|v| v.geometry().base), Ok(2048), "the FAT entry second in the table after a non-FAT first");
    let mut d = behind.clone();
    d.patch(2048, 510, &[0, 0]);
    assert_eq!(refused(d), "no boot signature", "the partition's boot sector without its signature");
    for name in GOLDENS {
        assert_eq!(mount(name).geometry().base, 0, "{name}: a superfloppy mounts at 0");
    }
}

// The scripted peer's replies parsed by the vendored stack's own wire types.
use crate::net_peer::{self, Peer, Verdict};
use smoltcp::wire::{
    ArpOperation, ArpPacket, ArpRepr, DhcpMessageType, DhcpPacket, DhcpRepr, DnsFlags, DnsPacket, DnsQueryType, DnsQuestion, DnsRcode, DnsRecord, DnsRecordData, EthernetAddress, EthernetFrame, EthernetProtocol,
    Icmpv4Message,
};

fn peer_v4(o: [u8; 4]) -> Ipv4Address {
    Ipv4Address::new(o[0], o[1], o[2], o[3])
}

/// An Ethernet frame with an IPv4 header and a UDP header around `payload`,
/// as a request to the peer (the checksums left zero: the peer reads fields,
/// the stack's parsers verify the peer's replies).
fn udp_frame(dst_mac: [u8; 6], src_ip: [u8; 4], dst_ip: [u8; 4], sp: u16, dp: u16, payload: &[u8]) -> Vec<u8> {
    let mut f = Vec::new();
    f.extend_from_slice(&dst_mac);
    f.extend_from_slice(&net_peer::OUR_MAC);
    f.extend_from_slice(&0x0800u16.to_be_bytes());
    let total = 20 + 8 + payload.len();
    f.extend_from_slice(&[0x45, 0, (total >> 8) as u8, total as u8, 0, 0, 0x40, 0, 64, 17, 0, 0]);
    f.extend_from_slice(&src_ip);
    f.extend_from_slice(&dst_ip);
    let ulen = 8 + payload.len();
    f.extend_from_slice(&sp.to_be_bytes());
    f.extend_from_slice(&dp.to_be_bytes());
    f.extend_from_slice(&(ulen as u16).to_be_bytes());
    f.extend_from_slice(&[0, 0]);
    f.extend_from_slice(payload);
    f
}

/// A DHCP client message of `kind` with the given options appended after
/// the message-type option.
fn dhcp_message(kind: u8, xid: u32, extra: &[u8]) -> Vec<u8> {
    let mut d = vec![0u8; 240];
    d[0] = 1;
    d[1] = 1;
    d[2] = 6;
    d[4..8].copy_from_slice(&xid.to_be_bytes());
    d[10] = 0x80; // the broadcast flag
    d[28..34].copy_from_slice(&net_peer::OUR_MAC);
    d[236..240].copy_from_slice(&[0x63, 0x82, 0x53, 0x63]);
    d.extend_from_slice(&[53, 1, kind]);
    d.extend_from_slice(extra);
    d.push(255);
    d
}

/// The peer's reply as the stack's frame, its IPv4 and UDP checksums
/// verified; the UDP payload and the IPv4 source answered.
fn parse_udp_reply(out: &[u8]) -> (Vec<u8>, Ipv4Address, Ipv4Address, u16, u16, EthernetAddress) {
    let frame = EthernetFrame::new_checked(out).expect("a well-formed frame");
    assert_eq!(frame.ethertype(), EthernetProtocol::Ipv4);
    assert_eq!(frame.src_addr(), EthernetAddress(net_peer::PEER_MAC));
    let ip = Ipv4Packet::new_checked(frame.payload()).expect("a well-formed IPv4 header");
    assert!(ip.verify_checksum(), "the stack verifies the peer's IPv4 header checksum");
    assert_eq!(ip.next_header(), IpProtocol::Udp);
    let (src, dst) = (ip.src_addr(), ip.dst_addr());
    let udp = UdpPacket::new_checked(ip.payload()).expect("a well-formed UDP header");
    assert!(udp.verify_checksum(&IpAddress::Ipv4(src), &IpAddress::Ipv4(dst)), "the stack verifies the peer's UDP checksum");
    (udp.payload().to_vec(), src, dst, udp.src_port(), udp.dst_port(), frame.dst_addr())
}

#[test]
fn net_peer_arp_reply_parsed_by_the_stack() {
    // A request from the stack for the peer's address, broadcast.
    let mut f = Vec::new();
    f.extend_from_slice(&net_peer::BROADCAST_MAC);
    f.extend_from_slice(&net_peer::OUR_MAC);
    f.extend_from_slice(&0x0806u16.to_be_bytes());
    f.extend_from_slice(&[0, 1, 8, 0, 6, 4, 0, 1]);
    f.extend_from_slice(&net_peer::OUR_MAC);
    f.extend_from_slice(&net_peer::LEASE_IP);
    f.extend_from_slice(&[0; 6]);
    f.extend_from_slice(&net_peer::PEER_IP);
    let mut out = [0u8; net_peer::MAX_REPLY];
    let mut peer = Peer::new(false);
    let Verdict::Reply(n) = peer.answer(&f, &mut out) else { panic!("the peer did not answer the ARP request") };
    assert_eq!(n, 60, "an ARP reply is padded to the minimum frame");
    let frame = EthernetFrame::new_checked(&out[..n]).unwrap();
    assert_eq!(frame.dst_addr(), EthernetAddress(net_peer::OUR_MAC));
    assert_eq!(frame.ethertype(), EthernetProtocol::Arp);
    let arp = ArpPacket::new_checked(frame.payload()).unwrap();
    let ArpRepr::EthernetIpv4 { operation, source_hardware_addr, source_protocol_addr, target_hardware_addr, target_protocol_addr } = ArpRepr::parse(&arp).unwrap() else { panic!("not an Ethernet/IPv4 ARP reply") };
    assert_eq!(operation, ArpOperation::Reply);
    assert_eq!(source_hardware_addr, EthernetAddress(net_peer::PEER_MAC));
    assert_eq!(source_protocol_addr, peer_v4(net_peer::PEER_IP));
    assert_eq!(target_hardware_addr, EthernetAddress(net_peer::OUR_MAC));
    assert_eq!(target_protocol_addr, peer_v4(net_peer::LEASE_IP));
    assert_eq!(peer.arps_answered, 1);
    // A request for an address nobody owns is handed back (the loopback), and the doctored peer answers nothing.
    let mut unowned = f.clone();
    unowned[38..42].copy_from_slice(&net_peer::UNOWNED_IP);
    assert_eq!(peer.answer(&unowned, &mut out), Verdict::Pass);
    let mut silent = Peer::new(true);
    assert_eq!(silent.answer(&f, &mut out), Verdict::Pass, "the doctored peer never answers an ARP request");
    assert_eq!(silent.arps_answered, 0);
}

#[test]
fn net_peer_dhcp_offer_and_ack_parsed_by_the_stack() {
    let discover = udp_frame(net_peer::BROADCAST_MAC, [0; 4], net_peer::BROADCAST_IP, 68, 67, &dhcp_message(1, 0x1234_5678, &[55, 3, 1, 3, 6]));
    let mut out = [0u8; net_peer::MAX_REPLY];
    let mut peer = Peer::new(false);
    // The scenario: the first discover ever seen goes unanswered.
    assert_eq!(peer.answer(&discover, &mut out), Verdict::Drop);
    assert_eq!((peer.discovers, peer.offers, peer.dropped, peer.first_xid), (1, 0, 1, 0x1234_5678));
    let retry = udp_frame(net_peer::BROADCAST_MAC, [0; 4], net_peer::BROADCAST_IP, 68, 67, &dhcp_message(1, 0x1234_5679, &[55, 3, 1, 3, 6]));
    let Verdict::Reply(n) = peer.answer(&retry, &mut out) else { panic!("the second discover was not answered") };
    assert_eq!((peer.discovers, peer.offers, peer.last_xid), (2, 1, 0x1234_5679));
    let (payload, src, dst, sp, dp, mac) = parse_udp_reply(&out[..n]);
    assert_eq!((src, dst, sp, dp, mac), (peer_v4(net_peer::PEER_IP), peer_v4(net_peer::BROADCAST_IP), 67, 68, EthernetAddress(net_peer::BROADCAST_MAC)));
    let packet = DhcpPacket::new_checked(&payload[..]).expect("a well-formed DHCP message");
    let repr = DhcpRepr::parse(&packet).expect("the stack parses the offer");
    assert_eq!(repr.message_type, DhcpMessageType::Offer);
    assert_eq!(repr.transaction_id, 0x1234_5679);
    assert_eq!(repr.client_hardware_address, EthernetAddress(net_peer::OUR_MAC));
    assert_eq!(repr.your_ip, peer_v4(net_peer::LEASE_IP));
    assert_eq!(repr.server_identifier, Some(peer_v4(net_peer::PEER_IP)));
    assert_eq!(repr.subnet_mask, Some(peer_v4(net_peer::LEASE_MASK)));
    assert_eq!(repr.router, Some(peer_v4(net_peer::PEER_IP)));
    assert_eq!(repr.dns_servers.as_deref(), Some(&[peer_v4(net_peer::PEER_IP), peer_v4(net_peer::LEASE_DNS2)][..]));
    assert_eq!(repr.lease_duration, Some(net_peer::LEASE_SECONDS));
    assert!(repr.broadcast, "the client's broadcast flag copied back");
    // The request, acknowledged with the same record.
    let mut req_opts = vec![50, 4];
    req_opts.extend_from_slice(&net_peer::LEASE_IP);
    req_opts.extend_from_slice(&[54, 4]);
    req_opts.extend_from_slice(&net_peer::PEER_IP);
    let request = udp_frame(net_peer::BROADCAST_MAC, [0; 4], net_peer::BROADCAST_IP, 68, 67, &dhcp_message(3, 0x1234_5679, &req_opts));
    let Verdict::Reply(n) = peer.answer(&request, &mut out) else { panic!("the request was not answered") };
    let (payload, ..) = parse_udp_reply(&out[..n]);
    let packet = DhcpPacket::new_checked(&payload[..]).unwrap();
    let ack = DhcpRepr::parse(&packet).expect("the stack parses the acknowledgement");
    assert_eq!(ack.message_type, DhcpMessageType::Ack);
    assert_eq!((ack.your_ip, ack.server_identifier, ack.subnet_mask, ack.lease_duration), (peer_v4(net_peer::LEASE_IP), Some(peer_v4(net_peer::PEER_IP)), Some(peer_v4(net_peer::LEASE_MASK)), Some(net_peer::LEASE_SECONDS)));
    assert_eq!((peer.requests, peer.acks), (1, 1));
    // A discover that is not a DHCP message (the cookie wrong) is handed back.
    let mut bad = dhcp_message(1, 1, &[]);
    bad[236] = 0;
    assert_eq!(peer.answer(&udp_frame(net_peer::BROADCAST_MAC, [0; 4], net_peer::BROADCAST_IP, 68, 67, &bad), &mut out), Verdict::Pass);
}

fn dns_query(id: u16, name: &str) -> Vec<u8> {
    let mut q = Vec::new();
    q.extend_from_slice(&id.to_be_bytes());
    q.extend_from_slice(&[0x01, 0x00, 0, 1, 0, 0, 0, 0, 0, 0]);
    for label in name.split('.') {
        q.push(label.len() as u8);
        q.extend_from_slice(label.as_bytes());
    }
    q.extend_from_slice(&[0, 0, 1, 0, 1]);
    q
}

fn labels(packet: &DnsPacket<&[u8]>, name: &[u8]) -> Vec<String> {
    packet.parse_name(name).map(|l| String::from_utf8(l.unwrap().to_vec()).unwrap()).collect()
}

#[test]
fn net_peer_dns_answer_and_name_error_parsed_by_the_stack() {
    let mut out = [0u8; net_peer::MAX_REPLY];
    let mut peer = Peer::new(false);
    let query = udp_frame(net_peer::PEER_MAC, net_peer::LEASE_IP, net_peer::PEER_IP, 40000, 53, &dns_query(0xBEEF, "PEER.rhu.Test"));
    let Verdict::Reply(n) = peer.answer(&query, &mut out) else { panic!("the query was not answered") };
    let (payload, src, dst, sp, dp, mac) = parse_udp_reply(&out[..n]);
    assert_eq!((src, dst, sp, dp, mac), (peer_v4(net_peer::PEER_IP), peer_v4(net_peer::LEASE_IP), 53, 40000, EthernetAddress(net_peer::OUR_MAC)));
    let packet = DnsPacket::new_checked(&payload[..]).expect("a well-formed DNS message");
    assert_eq!(packet.transaction_id(), 0xBEEF);
    assert!(packet.flags().contains(DnsFlags::RESPONSE | DnsFlags::RECURSION_AVAILABLE));
    assert_eq!(packet.rcode(), DnsRcode::NoError);
    assert_eq!((packet.question_count(), packet.answer_record_count()), (1, 1));
    let (rest, question) = DnsQuestion::parse(packet.payload()).expect("the question copied back");
    assert_eq!(labels(&packet, question.name), ["PEER", "rhu", "Test"]);
    assert_eq!(question.type_, DnsQueryType::A);
    let (_, record) = DnsRecord::parse(rest).expect("the answer record");
    assert_eq!(labels(&packet, record.name), ["PEER", "rhu", "Test"], "the answer's name is a pointer to the question's");
    assert_eq!(record.ttl, net_peer::DNS_TTL);
    assert_eq!(record.data, DnsRecordData::A(peer_v4(net_peer::PEER_IP)));
    // Every name of the table, and the one the peer has not.
    for (name, addr) in net_peer::NAMES.iter() {
        let q = udp_frame(net_peer::PEER_MAC, net_peer::LEASE_IP, net_peer::PEER_IP, 40001, 53, &dns_query(7, name));
        let Verdict::Reply(n) = peer.answer(&q, &mut out) else { panic!("{name} was not answered") };
        let (payload, ..) = parse_udp_reply(&out[..n]);
        let packet = DnsPacket::new_checked(&payload[..]).unwrap();
        let (rest, _) = DnsQuestion::parse(packet.payload()).unwrap();
        let (_, record) = DnsRecord::parse(rest).unwrap();
        assert_eq!(record.data, DnsRecordData::A(peer_v4(*addr)), "the record of {name}");
    }
    let q = udp_frame(net_peer::PEER_MAC, net_peer::LEASE_IP, net_peer::PEER_IP, 40002, 53, &dns_query(9, net_peer::UNKNOWN_NAME));
    let Verdict::Reply(n) = peer.answer(&q, &mut out) else { panic!("the unknown name was not answered") };
    let (payload, ..) = parse_udp_reply(&out[..n]);
    let packet = DnsPacket::new_checked(&payload[..]).unwrap();
    assert_eq!(packet.rcode(), DnsRcode::NXDomain);
    assert_eq!((packet.question_count(), packet.answer_record_count()), (1, 0));
    assert_eq!((peer.dns_answered, peer.dns_refused), (4, 1));
}

#[test]
fn net_peer_icmp_echo_reply_parsed_by_the_stack() {
    let mut f = Vec::new();
    f.extend_from_slice(&net_peer::PEER_MAC);
    f.extend_from_slice(&net_peer::OUR_MAC);
    f.extend_from_slice(&0x0800u16.to_be_bytes());
    let icmp = [8u8, 0, 0, 0, 0xAB, 0xCD, 0x00, 0x07, b'r', b'h', b'u'];
    let total = 20 + icmp.len();
    f.extend_from_slice(&[0x45, 0, 0, total as u8, 0, 0, 0x40, 0, 64, 1, 0, 0]);
    f.extend_from_slice(&net_peer::LEASE_IP);
    f.extend_from_slice(&net_peer::PEER_IP);
    f.extend_from_slice(&icmp);
    let mut out = [0u8; net_peer::MAX_REPLY];
    let mut peer = Peer::new(false);
    let Verdict::Reply(n) = peer.answer(&f, &mut out) else { panic!("the echo request was not answered") };
    let frame = EthernetFrame::new_checked(&out[..n]).unwrap();
    assert_eq!(frame.dst_addr(), EthernetAddress(net_peer::OUR_MAC));
    let ip = Ipv4Packet::new_checked(frame.payload()).unwrap();
    assert!(ip.verify_checksum());
    assert_eq!((ip.src_addr(), ip.dst_addr(), ip.next_header()), (peer_v4(net_peer::PEER_IP), peer_v4(net_peer::LEASE_IP), IpProtocol::Icmp));
    let reply = Icmpv4Packet::new_checked(ip.payload()).unwrap();
    assert!(reply.verify_checksum(), "the stack verifies the echo reply's checksum");
    assert_eq!(reply.msg_type(), Icmpv4Message::EchoReply);
    assert_eq!((reply.echo_ident(), reply.echo_seq_no()), (0xABCD, 7));
    assert_eq!(reply.data(), b"rhu");
    assert_eq!(peer.echoes, 1);
}

#[test]
fn net_peer_tcp_reset_parsed_by_the_stack() {
    let syn = |port: u16| -> Vec<u8> {
        let mut f = Vec::new();
        f.extend_from_slice(&net_peer::PEER_MAC);
        f.extend_from_slice(&net_peer::OUR_MAC);
        f.extend_from_slice(&0x0800u16.to_be_bytes());
        f.extend_from_slice(&[0x45, 0, 0, 40, 0, 0, 0x40, 0, 64, 6, 0, 0]);
        f.extend_from_slice(&net_peer::LEASE_IP);
        f.extend_from_slice(&net_peer::PEER_IP);
        f.extend_from_slice(&49152u16.to_be_bytes());
        f.extend_from_slice(&port.to_be_bytes());
        f.extend_from_slice(&1000u32.to_be_bytes());
        f.extend_from_slice(&[0, 0, 0, 0, 0x50, 0x02, 0x10, 0x00, 0, 0, 0, 0]);
        f
    };
    let mut out = [0u8; net_peer::MAX_REPLY];
    let mut peer = Peer::new(false);
    let Verdict::Reply(n) = peer.answer(&syn(net_peer::REFUSED_PORT), &mut out) else { panic!("the connection attempt was not reset") };
    let frame = EthernetFrame::new_checked(&out[..n]).unwrap();
    let ip = Ipv4Packet::new_checked(frame.payload()).unwrap();
    assert!(ip.verify_checksum());
    let (src, dst) = (IpAddress::Ipv4(ip.src_addr()), IpAddress::Ipv4(ip.dst_addr()));
    assert_eq!((src, dst), (IpAddress::Ipv4(peer_v4(net_peer::PEER_IP)), IpAddress::Ipv4(peer_v4(net_peer::LEASE_IP))));
    let tcp = TcpPacket::new_checked(ip.payload()).unwrap();
    assert!(tcp.verify_checksum(&src, &dst), "the stack verifies the reset's checksum");
    assert!(tcp.rst() && tcp.ack() && !tcp.syn());
    assert_eq!((tcp.src_port(), tcp.dst_port()), (net_peer::REFUSED_PORT, 49152));
    assert_eq!(tcp.ack_number(), smoltcp::wire::TcpSeqNumber(1001), "the reset acknowledges the SYN");
    assert_eq!(peer.resets, 1);
    // A connection attempt to another port of the peer is swallowed (addressed to the peer, unanswered).
    assert_eq!(peer.answer(&syn(80), &mut out), Verdict::Drop);
}

#[test]
fn net_peer_passes_what_it_does_not_answer() {
    let mut out = [0u8; net_peer::MAX_REPLY];
    let mut peer = Peer::new(false);
    // A frame to the stack's own address (the loopback's traffic) is handed back whole.
    let ours = udp_frame(net_peer::OUR_MAC, net_peer::LEASE_IP, net_peer::LEASE_IP, 9000, 9001, b"loopback");
    assert_eq!(peer.answer(&ours, &mut out), Verdict::Pass);
    // A broadcast the peer does not understand is handed back; one addressed to the peer alone is dropped.
    let mut other = Vec::new();
    other.extend_from_slice(&net_peer::BROADCAST_MAC);
    other.extend_from_slice(&net_peer::OUR_MAC);
    other.extend_from_slice(&0x88B5u16.to_be_bytes());
    other.extend_from_slice(&[0; 46]);
    assert_eq!(peer.answer(&other, &mut out), Verdict::Pass);
    other[..6].copy_from_slice(&net_peer::PEER_MAC);
    assert_eq!(peer.answer(&other, &mut out), Verdict::Drop);
    assert_eq!(peer.dropped, 1);
    // A runt frame and a reply buffer too small are passed, never read past.
    assert_eq!(peer.answer(&other[..10], &mut out), Verdict::Pass);
    assert_eq!(peer.answer(&ours, &mut out[..100]), Verdict::Pass);
    assert_eq!(net_peer::lookup(b"Second.RHU.test"), Some(net_peer::LEASE_DNS2));
    assert_eq!(net_peer::lookup(b"second.rhu.tes"), None);
}

// ---- write(2)'s rule for the layer's write().

/// A device over a golden that refuses the n-th direct data write (a call of more than one sector: the cache's own
/// flushes move one) with Io -- once, or from then on -- and serves every other call.
struct RefusingDevice {
    inner: Sparse,
    at: u32,
    once: bool,
    seen: u32,
    refusals: u32,
}

impl crate::fat::BlockDevice for RefusingDevice {
    fn blocks(&mut self) -> u32 {
        self.inner.blocks()
    }
    fn read(&mut self, lba: u32, buf: &mut [u8]) -> Result<(), Error> {
        self.inner.read(lba, buf)
    }
    fn write(&mut self, lba: u32, data: &[u8]) -> Result<(), Error> {
        if data.len() > crate::fat::SECTOR {
            self.seen += 1;
            if self.seen == self.at || (!self.once && self.seen > self.at) {
                self.refusals += 1;
                return Err(Error::Io);
            }
        }
        self.inner.write(lba, data)
    }
    fn sync(&mut self) -> Result<(), Error> {
        self.inner.sync()
    }
}

#[test]
fn fat_write_returns_the_bytes_moved_before_a_refusal() {
    const LEN: usize = 1 << 20;
    let mut raw = vec![0u8; LEN + 4];
    let off = raw.as_ptr().align_offset(4);
    let pattern = &mut raw[off..off + LEN];
    for (i, b) in pattern.iter_mut().enumerate() {
        *b = (i as u8).wrapping_mul(7) ^ ((i >> 9) as u8);
    }
    let pattern: &[u8] = pattern;
    // The chunk refused: the first (the control: nothing moved, the refusal itself), the third once (the rest goes in
    // at the next calls), the fifth and every one after it (the next call moves nothing and returns the refusal).
    for (at, once) in [(1u32, true), (3, true), (5, false)] {
        let tag = format!("the chunk {at} refused {}", if once { "once" } else { "from then on" });
        let dev = RefusingDevice { inner: golden("fat16-32m"), at, once, seen: 0, refusals: 0 };
        let mut scratch = vec![0u8; 64 * 1024];
        let mut v = Volume::mount(dev, &mut scratch).unwrap_or_else(|r| panic!("{tag}: {}", r.words()));
        let free0 = v.free_clusters();
        let mut h = v.open(b"/partial.bin", RW | O_CREATE | O_EXCL).unwrap();
        let first = v.write(&mut h, pattern);
        let took: usize = v.device().inner.trace.iter().filter(|t| t.0 == Op::Write && t.2 > 1).map(|t| t.2 as usize * crate::fat::SECTOR).sum();
        let size = v.fstat(&h).unwrap().size as usize;
        if at == 1 {
            assert_eq!((first, took, size, h.position()), (Err(Error::Io), 0, 0, 0), "{tag}: the refusal itself, nothing moved");
        } else {
            let n1 = first.unwrap_or_else(|e| panic!("{tag}: refused {e:?} with {took} bytes taken"));
            assert!(n1 > 0 && n1 < LEN && n1 % crate::fat::SECTOR == 0, "{tag}: the count {n1}");
            assert_eq!((n1, v.device().refusals, v.device().seen), (took, 1, at), "{tag}: the count is what the device took before the refusal");
            assert_eq!((size, h.position() as usize), (n1, n1), "{tag}: the size and the position cover the bytes moved");
            if once {
                let mut done = n1;
                while done < LEN {
                    let n = v.write(&mut h, &pattern[done..]).unwrap_or_else(|e| panic!("{tag}: the next call refused {e:?}"));
                    assert!(n > 0, "{tag}: a call moved nothing");
                    done += n;
                }
                v.sync_file(&mut h).unwrap();
                v.close(&mut h).unwrap();
                let mut h = v.open(b"/partial.bin", O_READ).unwrap();
                let mut back = vec![0u8; LEN];
                let mut done = 0;
                while done < LEN {
                    let n = v.read(&mut h, &mut back[done..]).unwrap();
                    assert!(n > 0);
                    done += n;
                }
                v.close(&mut h).unwrap();
                assert!(back == pattern, "{tag}: the file read back differs");
                v.sync().unwrap();
                let dev = v.device().inner.clone();
                let r = Reader::open(&dev).unwrap();
                let item = r.list_all().unwrap().into_iter().find(|i| i.path == "/partial.bin").unwrap_or_else(|| panic!("{tag}: the second reader does not list the file"));
                assert_eq!(item.size as usize, LEN, "{tag}: the second reader's size");
                assert!(r.read_file(&item).unwrap() == pattern, "{tag}: the second reader's bytes differ");
                assert_eq!(r.free_clusters(), v.free_clusters(), "{tag}: the free count by the second reader");
            } else {
                assert_eq!(v.write(&mut h, &pattern[n1..]), Err(Error::Io), "{tag}: the next call moves nothing and returns the refusal");
                assert_eq!(v.fstat(&h).unwrap().size as usize, n1, "{tag}: the size unchanged by the refused call");
                v.close(&mut h).unwrap();
            }
        }
        if at == 1 {
            v.close(&mut h).unwrap();
        }
        v.unlink(b"/partial.bin").unwrap();
        assert_eq!(v.free_clusters(), free0, "{tag}: the free count after the unlink");
    }
}

const A_UART0: uart_math::PinSets<'static> = uart_math::PinSets { tx: &[0, 12, 16, 28], rx: &[1, 13, 17, 29] };
const A_UART1: uart_math::PinSets<'static> = uart_math::PinSets { tx: &[4, 8, 20, 24], rx: &[5, 9, 21, 25] };
const B_UART0: uart_math::PinSets<'static> = uart_math::PinSets { tx: &[0, 12, 16, 28, 32, 44], rx: &[1, 13, 17, 29, 33, 45] };
const B_UART1: uart_math::PinSets<'static> = uart_math::PinSets { tx: &[4, 8, 20, 24, 36, 40], rx: &[5, 9, 21, 25, 37, 41] };
/// A block without a pre-defined pair (the Pico rows' UART1), and UART0's maker pair.
const NONE: (u32, u32) = (uart_math::NO_PIN, uart_math::NO_PIN);
const MAKER0: (u32, u32) = (0, 1);
fn named(gpio: u8) -> u32 {
    uart_math::NAMED | gpio as u32
}

#[test]
fn uart_math_effective_pair_on_the_rp2350_rows() {
    use uart_math::{effective_pair, read_word, Word, NO_PIN};
    assert_eq!(read_word(0), Word::Unset);
    assert_eq!(read_word(0x108), Word::Named(8));
    assert_eq!(read_word(0x1FF), Word::Named(255));
    // UART1 (no pre-defined pair): every TX of the set with every RX of the set, on both tables.
    let mut pairs = 0;
    for (table, want) in [(&A_UART1, 16), (&B_UART1, 36)] {
        let mut n = 0;
        for &tx in table.tx {
            for &rx in table.rx {
                assert_eq!(effective_pair(NONE, Some(table), named(tx), named(rx)), (tx as u32, rx as u32, true), "UART1 TX {tx} RX {rx}");
                n += 1;
            }
        }
        assert_eq!(n, want);
        pairs += n;
    }
    assert_eq!(pairs, 52);
    // UART0 (the maker's pair): unset words keep it; a named legal pair replaces it; a half pair completes from it.
    for table in [&A_UART0, &B_UART0] {
        assert_eq!(effective_pair(MAKER0, Some(table), 0, 0), (0, 1, true));
        for &tx in table.tx {
            for &rx in table.rx {
                assert_eq!(effective_pair(MAKER0, Some(table), named(tx), named(rx)), (tx as u32, rx as u32, true), "UART0 TX {tx} RX {rx}");
            }
        }
        assert_eq!(effective_pair(MAKER0, Some(table), named(12), 0), (12, 1, true), "TX named, RX the maker's");
        assert_eq!(effective_pair(MAKER0, Some(table), 0, named(13)), (0, 13, true), "RX named, TX the maker's");
    }
    // A row whose UARTs have no pins on a chip without a table (the emulator rows): legal as it stands, the words ignored.
    assert_eq!(effective_pair(NONE, None, 0, 0), (NO_PIN, NO_PIN, true));
    assert_eq!(effective_pair(NONE, None, named(8), named(9)), (NO_PIN, NO_PIN, true));
    assert_eq!(effective_pair(NONE, None, 8, 0x308), (NO_PIN, NO_PIN, true));
}

#[test]
fn uart_math_pin_words_refused_with_the_words() {
    use uart_math::{effective_pair, read_word, LegalPins, Word, NO_PIN, REFUSED_NOT_LEGAL, REFUSED_NO_DEFAULT};
    assert_eq!(read_word(8), Word::Malformed, "the valid bit clear");
    assert_eq!(read_word(0x308), Word::Malformed, "a bit above 8");
    assert_eq!(read_word(0x200), Word::Malformed);
    assert_eq!(read_word(0xFFFF_FFFF), Word::Malformed);
    // UART1 on the Pimoroni table: each doctored input answers the facts' pair (none) and not legal.
    let refused1 = (NO_PIN, NO_PIN, false);
    let t = &B_UART1;
    assert_eq!(effective_pair(NONE, Some(t), 0, 0), refused1, "no words on a block without a pre-defined pair");
    assert_eq!(effective_pair(NONE, Some(t), named(8), 0), refused1, "one word alone (TX)");
    assert_eq!(effective_pair(NONE, Some(t), 0, named(9)), refused1, "one word alone (RX)");
    assert_eq!(effective_pair(NONE, Some(t), named(8), named(8)), refused1, "TX equal to RX");
    assert_eq!(effective_pair(NONE, Some(t), named(0), named(1)), refused1, "UART0's pins on UART1");
    assert_eq!(effective_pair(NONE, Some(t), named(9), named(8)), refused1, "a swapped pair");
    assert_eq!(effective_pair(NONE, Some(t), named(46), named(47)), refused1, "past the set");
    assert_eq!(effective_pair(NONE, Some(t), 8, 9), refused1, "the valid bit clear");
    assert_eq!(effective_pair(NONE, Some(t), 0x308, 0x309), refused1, "a bit above 8");
    assert_eq!(effective_pair(NONE, Some(t), 0x1FF, named(9)), refused1, "GPIO 255");
    assert_eq!(effective_pair(NONE, Some(t), named(36), 0x209), refused1, "one word malformed");
    // UART0 on the RP2350A table: the maker's pair answered and not legal.
    let refused0 = (0, 1, false);
    let a = &A_UART0;
    assert_eq!(effective_pair(MAKER0, Some(a), named(4), named(5)), refused0, "UART1's pins on UART0");
    assert_eq!(effective_pair(MAKER0, Some(a), named(32), named(33)), refused0, "a B-row pair on an A row");
    assert_eq!(effective_pair(MAKER0, Some(a), named(12), named(12)), refused0, "TX equal to RX");
    assert_eq!(effective_pair(MAKER0, Some(a), 0x10C, 13), refused0, "the valid bit clear on one word");
    assert_eq!(effective_pair(MAKER0, Some(&B_UART0), named(46), named(47)), refused0, "past the B set");
    // A row with a pre-defined pair on a chip without a table: named words have no law and are refused; unset words keep the pair.
    assert_eq!(effective_pair(MAKER0, None, named(12), named(13)), refused0);
    assert_eq!(effective_pair(MAKER0, None, 0, 0), (0, 1, true));
    // The refusal's words and the legal pins as the claim's line prints them.
    assert_eq!(REFUSED_NO_DEFAULT, "no default pins on this row; the project's pin words must name a legal pair of this chip");
    assert_eq!(REFUSED_NOT_LEGAL, "the project's pin words are not a legal pair of this chip");
    assert_eq!(format!("{}", LegalPins(Some(&B_UART1))), "TX 4, 8, 20, 24, 36 or 40; RX 5, 9, 21, 25, 37 or 41");
    assert_eq!(format!("{}", LegalPins(Some(&A_UART0))), "TX 0, 12, 16 or 28; RX 1, 13, 17 or 29");
    assert_eq!(format!("{}", LegalPins(Some(&uart_math::PinSets { tx: &[7], rx: &[3, 5] }))), "TX 7; RX 3 or 5");
    assert_eq!(format!("{}", LegalPins(None)), "none listed for this chip");
}

#[test]
fn exit_word_encodes_the_contracts_numbers() {
    use crate::exit_word::{decode, magic, name, CODE_OFFSET, MAGIC_OFFSET, MAGIC_RETURNED, MAGIC_RUNNING, MAGIC_ABORTED, STATE_RETURNED, STATE_RUNNING, STATE_ABORTED};
    assert_eq!(MAGIC_OFFSET, 0);
    assert_eq!(CODE_OFFSET, 12);
    // The magics as a little-endian reader sees them in a byte dump.
    assert_eq!(MAGIC_RUNNING.to_le_bytes(), *b"RLOG");
    assert_eq!(MAGIC_RETURNED.to_le_bytes(), *b"RRET");
    assert_eq!(MAGIC_ABORTED.to_le_bytes(), *b"RABT");
    assert_eq!(MAGIC_RUNNING, 0x474F_4C52);
    assert_eq!(MAGIC_RETURNED, 0x5445_5252);
    assert_eq!(MAGIC_ABORTED, 0x5442_4152);
    assert_eq!(magic(STATE_RUNNING), MAGIC_RUNNING);
    assert_eq!(magic(STATE_RETURNED), MAGIC_RETURNED);
    assert_eq!(magic(STATE_ABORTED), MAGIC_ABORTED);
    // The code rides whole, 32 bits, beside an end magic.
    for (state, code) in [
        (STATE_RETURNED, 0),
        (STATE_RETURNED, 42),
        (STATE_RETURNED, 255),
        (STATE_RETURNED, 70000),
        (STATE_RETURNED, i32::MAX),
        (STATE_ABORTED, 134),
        (STATE_ABORTED, -1),
        (STATE_ABORTED, -70000),
        (STATE_ABORTED, i32::MIN),
    ] {
        assert_eq!(decode(magic(state), code), Some((state, code)), "{state} {code}");
    }
    // While the program runs the code is 0 by the boot; a reader takes a code only beside an end magic.
    assert_eq!(decode(MAGIC_RUNNING, 0), Some((STATE_RUNNING, 0)));
    assert_eq!(decode(MAGIC_RUNNING, 70000), Some((STATE_RUNNING, 0)));
    assert_eq!(name(STATE_RUNNING), "running");
    assert_eq!(name(STATE_RETURNED), "returned");
    assert_eq!(name(STATE_ABORTED), "aborted");
}

#[test]
fn exit_word_refuses_the_words_outside_the_contract() {
    use crate::exit_word::decode;
    assert_eq!(decode(0, 0), None); // the boot's clear, before "RLOG": not armed
    assert_eq!(decode(0, 42), None);
    assert_eq!(decode(u32::from_le_bytes(*b"RREt"), 42), None); // one byte off an end magic
    assert_eq!(decode(u32::from_le_bytes(*b"RABt"), 134), None);
    assert_eq!(decode(u32::from_le_bytes(*b"RLOX"), 0), None); // one byte off the running magic
    assert_eq!(decode(u32::from_le_bytes(*b"TERR"), 42), None); // "RRET" read in the wrong byte order
    assert_eq!(decode(1, 0), None); // a state number is not a magic
    assert_eq!(decode(2, 134), None);
}
