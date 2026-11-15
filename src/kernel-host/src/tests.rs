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
fn bus_math_i2c_timing_serves_the_faster_rates_by_clock() {
    assert_eq!(bus_math::i2c_timing(16_000_000, 400_000), Some(0x0062_0C17), "fast mode at the 16 MHz clock");
    assert_eq!(bus_math::i2c_timing(16_000_000, 1_000_000), Some(0x0020_0408), "fast mode plus at the 16 MHz clock");
    assert_eq!(bus_math::i2c_timing(54_000_000, 1_000_000), Some(0x0091_101E), "fast mode plus at a 54 MHz clock");
    for (clock, rate) in [(16_000_000, 50_000), (54_000_000, 400_000), (64_000_000, 1_000_000), (12_000_000, 400_000)] {
        assert_eq!(bus_math::i2c_timing(clock, rate), None, "{clock} Hz at {rate} Hz has no word");
    }
}

#[test]
fn bus_math_i2c_timing_words_keep_the_rule() {
    for (clock, rate) in [(16_000_000u32, 100_000u32), (16_000_000, 400_000), (16_000_000, 1_000_000), (54_000_000, 1_000_000)] {
        let word = bus_math::i2c_timing(clock, rate).expect("a served pair");
        assert!(bus_math::i2c_timing_sound(word, clock, rate), "{word:#010x} at {clock} Hz does not keep the rule for {rate} Hz");
        // One count less in a field and the rule refuses the word: the low count, the high count, the setup, the hold.
        for (shift, mask) in [(0u32, 0xFFu32), (8, 0xFF), (20, 0xF), (16, 0xF)] {
            if (word >> shift) & mask != 0 {
                let less = word - (1 << shift);
                assert!(!bus_math::i2c_timing_sound(less, clock, rate), "{less:#010x}, one count less at bit {shift} of {word:#010x}, passes");
            }
        }
        assert!(!bus_math::i2c_timing_sound(word | 1 << 24, clock, rate), "a reserved bit passes");
        assert!(!bus_math::i2c_timing_sound(word, clock, 50_000), "a rate outside the set passes");
    }
    // The manuals' example rows for 16 MHz count on the bus's edges for their rate: the rule refuses all three.
    assert!(!bus_math::i2c_timing_sound(0x3042_0F13, 16_000_000, 100_000), "the manuals' 100 kHz row passes");
    assert!(!bus_math::i2c_timing_sound(0x1032_0309, 16_000_000, 400_000), "the manuals' 400 kHz row passes");
    assert!(!bus_math::i2c_timing_sound(0x0020_0204, 16_000_000, 1_000_000), "the manuals' 1 MHz row passes");
    // A word is its clock's: the 16 MHz word of fast mode plus is refused at 54 MHz.
    assert!(!bus_math::i2c_timing_sound(0x0020_0408, 54_000_000, 1_000_000), "the 16 MHz word passes at 54 MHz");
}

#[test]
fn bus_math_i2c_setup_takes_the_rows_fast_plus_fact() {
    use bus_math::{i2c_setup, I2cFastPlus, I2cSetup};
    // Two facts: a block that takes another clock for fast mode plus and has no drive bit; one that keeps its bus's
    // clock and has the bit.
    let other_clock = Some(I2cFastPlus { mux_value: 0b00, kernel_hz: 54_000_000, drive: false });
    let own_clock = Some(I2cFastPlus { mux_value: 0b10, kernel_hz: 16_000_000, drive: true });
    assert_eq!(i2c_setup(0b10, 16_000_000, other_clock, 1_000_000), Some(I2cSetup { mux_value: 0b00, timing: 0x0091_101E, drive: false }));
    assert_eq!(i2c_setup(0b10, 16_000_000, own_clock, 1_000_000), Some(I2cSetup { mux_value: 0b10, timing: 0x0020_0408, drive: true }));
    // The other rates stay at the bus's own clock and never set the drive, whatever the fact.
    for fact in [other_clock, own_clock, None] {
        assert_eq!(i2c_setup(0b10, 16_000_000, fact, 400_000), Some(I2cSetup { mux_value: 0b10, timing: 0x0062_0C17, drive: false }));
        assert_eq!(i2c_setup(0b10, 16_000_000, fact, 100_000), Some(I2cSetup { mux_value: 0b10, timing: 0x10A1_2828, drive: false }));
        assert_eq!(i2c_setup(0b10, 16_000_000, fact, 50_000), None);
    }
    // No fact: no fast mode plus, whatever the bus's clock. A fact whose clock has no word: none either.
    assert_eq!(i2c_setup(0b10, 16_000_000, None, 1_000_000), None);
    assert_eq!(i2c_setup(0b10, 16_000_000, Some(I2cFastPlus { mux_value: 0b00, kernel_hz: 16_000_001, drive: false }), 1_000_000), None);
    // A 64 MHz bus serves standard mode alone.
    assert_eq!(i2c_setup(0b10, 64_000_000, None, 100_000), Some(I2cSetup { mux_value: 0b10, timing: 0xF042_0F13, drive: false }));
    assert_eq!(i2c_setup(0b10, 64_000_000, None, 400_000), None);
    assert_eq!(i2c_setup(0b10, 64_000_000, None, 1_000_000), None);
}

#[test]
fn bus_math_budget_divisor_and_messages() {
    assert_eq!(bus_math::i2c_timing(16_000_000, 100_000), Some(0x10A1_2828), "standard mode at the 16 MHz clock: the rule's word");
    assert_eq!(bus_math::i2c_timing(16_000_000, 400_000), Some(0x0062_0C17), "fast mode at the 16 MHz clock: the rule's word");
    assert_eq!(bus_math::i2c_timing(54_000_000, 100_000), None);
    assert_eq!(bus_math::i2c_timing(64_000_000, 100_000), Some(0xF042_0F13), "the H7's 64 MHz kernel clock: prescaler 15 is the 250 ns tick of the manuals' 16 MHz row");
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
    // The three modes against the kernel's bus API's rate set, in its order.
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

// ---------------- the SPI-mode host over a scripted wire ----------------

use crate::sdspi::{self, SpiHost, SpiWire};
use std::collections::VecDeque;

/// A wire played from a byte transcript: every byte the host writes is
/// logged (the frames, the tokens, the data, the trailing clocks); every byte
/// the host reads comes off a queue the case filled with the card's answers,
/// the idle line (0xFF) once the queue is empty; the selects, the rates, the
/// pauses, the breaths and the witness words are recorded. A queued refusal
/// is answered to the next write.
struct Wire {
    written: Vec<u8>,
    answers: VecDeque<u8>,
    selects: Vec<bool>,
    rates: Vec<Rate>,
    present: Option<bool>,
    pauses: u32,
    breaths: u32,
    faults: Vec<(u8, u8)>,
    refuse_write: Option<Refusal>,
}

impl Wire {
    fn new(answers: &[u8]) -> Wire {
        Wire { written: Vec::new(), answers: answers.iter().copied().collect(), selects: Vec::new(), rates: Vec::new(), present: Some(true), pauses: 0, breaths: 0, faults: Vec::new(), refuse_write: None }
    }
}

impl SpiWire for Wire {
    fn attach(&mut self) -> Result<(), Refusal> {
        Ok(())
    }
    fn card_present(&mut self) -> Option<bool> {
        self.present
    }
    fn rate(&mut self, rate: Rate) -> Result<u32, Refusal> {
        self.rates.push(rate);
        Ok(match rate {
            Rate::Identification => 375_000,
            Rate::Default => 6_000_000,
        })
    }
    fn select(&mut self, low: bool) {
        self.selects.push(low);
    }
    fn write(&mut self, tx: &[u8]) -> Result<(), Refusal> {
        if let Some(r) = self.refuse_write.take() {
            return Err(r);
        }
        self.written.extend_from_slice(tx);
        Ok(())
    }
    fn read(&mut self, rx: &mut [u8]) -> Result<(), Refusal> {
        for b in rx.iter_mut() {
            *b = self.answers.pop_front().unwrap_or(0xFF);
        }
        Ok(())
    }
    fn pause(&mut self, _micros: u32) {
        self.pauses += 1;
    }
    fn breathe(&mut self) {
        self.breaths += 1;
    }
    fn fault(&mut self, r1: u8, token: u8) {
        self.faults.push((r1, token));
    }
}

/// A register's four words as the sixteen bytes the card sends, most significant first.
fn wire_be(words: &[u32; 4]) -> Vec<u8> {
    words.iter().flat_map(|w| w.to_be_bytes()).collect()
}
/// A card's R1 after one byte of the idle line.
fn wire_r1(b: u8) -> Vec<u8> {
    vec![0xFF, b]
}
/// A data block as the card sends it: two bytes of the idle line, the start token, the bytes, their CRC16.
fn wire_block(data: &[u8]) -> Vec<u8> {
    let mut v = vec![0xFF, 0xFF, sdspi::TOKEN_START];
    v.extend_from_slice(data);
    let c = sdcard::crc16(data);
    v.push((c >> 8) as u8);
    v.push(c as u8);
    v
}
/// What the host writes for one command: its frame, then the trailing clocks after the select is released.
fn wire_cmd(index: u8, arg: u32) -> Vec<u8> {
    let mut v = sdspi::frame(index, arg).to_vec();
    v.push(0xFF);
    v
}
/// The bytes a write's data phase puts on the wire: the token, the block, its CRC16.
fn wire_data(token: u8, data: &[u8]) -> Vec<u8> {
    let mut v = vec![token];
    v.extend_from_slice(data);
    let c = sdcard::crc16(data);
    v.push((c >> 8) as u8);
    v.push(c as u8);
    v
}
fn pattern(seed: u8) -> Vec<u8> {
    (0..512).map(|i| (i as u8).wrapping_mul(3).wrapping_add(seed)).collect()
}
fn cat(parts: &[Vec<u8>]) -> Vec<u8> {
    parts.iter().flatten().copied().collect()
}

#[test]
fn sdspi_identify_high_capacity_over_the_scripted_wire() {
    // The frames' CRC7 bytes the fact sheets give, and the status maps.
    assert_eq!(sdspi::frame(0, 0), [0x40, 0, 0, 0, 0, 0x95]);
    assert_eq!(sdspi::frame(8, 0x1AA), [0x48, 0, 0, 0x01, 0xAA, 0x87]);
    assert_eq!(sdspi::status_of_r1(sdspi::R1_IDLE), 0);
    assert_eq!(sdspi::status_of_r1(sdspi::R1_ILLEGAL_COMMAND | sdspi::R1_COM_CRC_ERROR), sdcard::STATUS_ILLEGAL_COMMAND | sdcard::STATUS_COM_CRC_ERROR);
    assert_eq!(sdspi::status_of_r1(sdspi::R1_ADDRESS_ERROR | sdspi::R1_PARAMETER_ERROR), sdcard::STATUS_ADDRESS_ERROR | sdcard::STATUS_OUT_OF_RANGE);
    assert_eq!(sdspi::status_of_r2(0, 0), STATUS_TRAN_READY);
    assert_eq!(sdspi::status_of_r2(0, sdspi::R2_WP_VIOLATION | sdspi::R2_CC_ERROR), STATUS_TRAN_READY | sdcard::STATUS_WP_VIOLATION | sdcard::STATUS_CC_ERROR);
    // A version 2 high-capacity card: CMD8 echoed, ACMD41 idle once then ready, the OCR with CCS, the CID and the CSD as blocks.
    let answers = cat(&[
        wire_r1(0x01),
        wire_r1(0x01), vec![0x00, 0x00, 0x01, 0xAA],
        wire_r1(0x01), wire_r1(0x01),
        wire_r1(0x01), wire_r1(0x00),
        wire_r1(0x00), vec![0xC0, 0xFF, 0x80, 0x00],
        wire_r1(0x00), wire_block(&wire_be(&CID)),
        wire_r1(0x00), wire_block(&wire_be(&CSD_V2)),
        wire_r1(0x00),
        wire_r1(0x00),
    ]);
    let mut h = SpiHost::new(Wire::new(&answers));
    let c = sdcard::identify(&mut h).unwrap();
    assert!(c.version2 && c.high_capacity && c.width == Width::One && !c.write_protected);
    assert_eq!((c.capacity_blocks, c.rca, c.cid, c.csd), ((0x3B37 + 1) * 1024, sdspi::RCA, CID, CSD_V2));
    // The wire saw: the power-up clocks, then every command the SPI mode carries, each with its trailing clocks --
    // ACMD41 with the host capacity bit alone, CMD58 after the ready R1, CMD10 for the CID, CMD9 with a zero argument,
    // the second CMD55 with the card layer's address argument, CMD16; no CMD2, CMD3, CMD7 or CMD6.
    let mut want = vec![0xFF; sdspi::POWER_UP_BYTES];
    for (i, a) in [(0u8, 0u32), (8, 0x1AA), (55, 0), (41, 0x4000_0000), (55, 0), (41, 0x4000_0000), (58, 0), (10, 0), (9, 0), (55, 0x1_0000), (16, 512)] {
        want.extend(wire_cmd(i, a));
    }
    assert_eq!(h.wire.written, want);
    assert_eq!(h.wire.rates, vec![Rate::Identification, Rate::Identification, Rate::Default]);
    assert_eq!(h.wire.pauses, 1);
    assert!(h.wire.faults.is_empty());
    // The selects: released at the attach, then asserted and released once per command on the wire.
    let mut selects = vec![false];
    for _ in 0..11 {
        selects.extend([true, false]);
    }
    assert_eq!(h.wire.selects, selects);
    // Four data lines are not a width this host drives.
    assert_eq!(h.set_bus(Rate::Default, Width::Four), Err(Refusal::Argument));
}

#[test]
fn sdspi_identify_version1_and_standard_capacity_over_the_scripted_wire() {
    // A version 1 card calls CMD8 illegal; ACMD41 goes without the host capacity bit and is ready at once; the OCR has no CCS; the CSD is version 1.0.
    let answers = cat(&[
        wire_r1(0x01),
        wire_r1(0x05),
        wire_r1(0x01), wire_r1(0x00),
        wire_r1(0x00), vec![0x80, 0xFF, 0x80, 0x00],
        wire_r1(0x00), wire_block(&wire_be(&CID)),
        wire_r1(0x00), wire_block(&wire_be(&CSD_V1)),
        wire_r1(0x00),
        wire_r1(0x00),
    ]);
    let mut h = SpiHost::new(Wire::new(&answers));
    let c = sdcard::identify(&mut h).unwrap();
    assert!(!c.version2 && !c.high_capacity && c.width == Width::One);
    assert_eq!(c.capacity_blocks, 4096 * (1 << 18) / 512);
    let mut want = vec![0xFF; sdspi::POWER_UP_BYTES];
    for (i, a) in [(0u8, 0u32), (8, 0x1AA), (55, 0), (41, 0), (58, 0), (10, 0), (9, 0), (55, 0x1_0000), (16, 512)] {
        want.extend(wire_cmd(i, a));
    }
    assert_eq!(h.wire.written, want);
    assert_eq!(h.wire.pauses, 0);
    // The address rule follows the class: a standard-capacity card's read carries the byte address.
    let mut buf = [0u8; 512];
    let mut h = SpiHost::new(Wire::new(&cat(&[wire_r1(0x00), wire_block(&pattern(1))])));
    h.set_bus(Rate::Default, Width::One).unwrap();
    let mut card1 = card(false, false);
    card1.capacity_blocks = c.capacity_blocks;
    sdcard::read(&mut h, &card1, 100, 1, &mut buf).unwrap();
    assert_eq!(h.wire.written, wire_cmd(17, 100 * 512));
    assert_eq!(buf.to_vec(), pattern(1));
}

#[test]
fn sdspi_reads_and_writes_blocks_with_the_tokens_and_crcs() {
    let hc = card(true, false);
    // One block: CMD17, the token, the bytes and the CRC; the host releases the select and clocks once after.
    let mut buf = [0u8; 1024];
    let mut h = SpiHost::new(Wire::new(&cat(&[wire_r1(0x00), wire_block(&pattern(7))])));
    h.set_bus(Rate::Default, Width::One).unwrap();
    sdcard::read(&mut h, &hc, 7, 1, &mut buf).unwrap();
    assert_eq!(buf[..512].to_vec(), pattern(7));
    assert_eq!(h.wire.written, wire_cmd(17, 7));
    assert_eq!(h.wire.selects, vec![true, false]);
    // Two blocks: CMD18, two blocks, then CMD12 with its stuff byte, its R1 and its busy signal.
    let mut h = SpiHost::new(Wire::new(&cat(&[wire_r1(0x00), wire_block(&pattern(1)), wire_block(&pattern(2)), vec![0xFF, 0xFF, 0x00, 0x00, 0x00, 0xFF]])));
    h.set_bus(Rate::Default, Width::One).unwrap();
    sdcard::read(&mut h, &hc, 20, 2, &mut buf).unwrap();
    assert_eq!((buf[..512].to_vec(), buf[512..].to_vec()), (pattern(1), pattern(2)));
    assert_eq!(h.wire.written, cat(&[wire_cmd(18, 20), wire_cmd(12, 0)]));
    // One block written: CMD24, the token, the bytes, the CRC, the data response, the busy signal; then the card layer's CMD13 (R2 both zero).
    let data = cat(&[pattern(3), pattern(4)]);
    let mut h = SpiHost::new(Wire::new(&cat(&[wire_r1(0x00), vec![0xFF, 0x05, 0x00, 0x00, 0xFF], wire_r1(0x00), vec![0x00]])));
    h.set_bus(Rate::Default, Width::One).unwrap();
    sdcard::write(&mut h, &hc, 5, 1, &data).unwrap();
    assert_eq!(h.wire.written, cat(&[sdspi::frame(24, 5).to_vec(), wire_data(sdspi::TOKEN_START, &data[..512]), vec![0xFF], wire_cmd(13, 0)]));
    assert_eq!(h.wire.breaths, 0);
    // Two blocks written: CMD25, each block behind its multiple-write token with its data response and busy signal, the stop token,
    // a skipped byte and the busy signal after it; then CMD13.
    let mut h = SpiHost::new(Wire::new(&cat(&[wire_r1(0x00), vec![0x05, 0x00, 0xFF], vec![0x05, 0xFF], vec![0xFF, 0x00, 0xFF], wire_r1(0x00), vec![0x00]])));
    h.set_bus(Rate::Default, Width::One).unwrap();
    sdcard::write(&mut h, &hc, 8, 2, &data).unwrap();
    assert_eq!(
        h.wire.written,
        cat(&[sdspi::frame(25, 8).to_vec(), wire_data(sdspi::TOKEN_START_MULTI, &data[..512]), wire_data(sdspi::TOKEN_START_MULTI, &data[512..]), vec![sdspi::TOKEN_STOP, 0xFF], wire_cmd(13, 0)])
    );
    // A card the detect line says is gone refuses the data path without a byte on the wire.
    let mut h = SpiHost::new(Wire::new(&[]));
    h.wire.present = Some(false);
    assert_eq!(h.read_blocks(17, 0, 1, &mut buf), Err(Refusal::NoCard));
    assert!(h.wire.written.is_empty());
    // A long busy signal is breathed through: the yield point every sixteen polls.
    let mut busy = vec![0x05];
    busy.extend(vec![0x00; 100]);
    busy.push(0xFF);
    let mut h = SpiHost::new(Wire::new(&cat(&[wire_r1(0x00), busy, wire_r1(0x00), vec![0x00]])));
    h.set_bus(Rate::Default, Width::One).unwrap();
    sdcard::write(&mut h, &hc, 5, 1, &data).unwrap();
    assert_eq!(h.wire.breaths, 100 / sdspi::BREATH_BYTES);
}

#[test]
fn sdspi_refuses_the_doctored_wire() {
    let hc = card(true, false);
    let data = pattern(9);
    let mut buf = [0u8; 512];
    // A CRC16 broken on the wire: the host refuses Crc; the card layer recovers by CMD12 and CMD13 and reads once more, which passes.
    let mut bad = wire_block(&data);
    let n = bad.len();
    bad[n - 1] ^= 0x01;
    let recover = cat(&[vec![0xFF, 0xFF, 0x00, 0xFF], wire_r1(0x00), vec![0x00]]);
    let mut h = SpiHost::new(Wire::new(&cat(&[wire_r1(0x00), bad, recover.clone(), wire_r1(0x00), wire_block(&data)])));
    h.set_bus(Rate::Default, Width::One).unwrap();
    sdcard::read(&mut h, &hc, 7, 1, &mut buf).unwrap();
    assert_eq!(buf.to_vec(), data);
    assert_eq!(h.wire.written, cat(&[wire_cmd(17, 7), wire_cmd(12, 0), wire_cmd(13, 0), wire_cmd(17, 7)]));
    assert_eq!(h.wire.faults, vec![(0x00, sdspi::TOKEN_START)]);
    // An error token in the start token's place: the card's ECC failure is retried once as a CRC refusal; an out-of-range
    // token is final, as the timeout, after the recovery.
    let mut h = SpiHost::new(Wire::new(&cat(&[wire_r1(0x00), vec![0xFF, sdspi::ERROR_TOKEN_OUT_OF_RANGE], recover.clone()])));
    h.set_bus(Rate::Default, Width::One).unwrap();
    assert_eq!(sdcard::read(&mut h, &hc, 7, 1, &mut buf), Err(Stuck { step: Step::Transfer, why: Why::Refused(Refusal::Timeout) }));
    assert_eq!(h.wire.faults, vec![(0x00, sdspi::ERROR_TOKEN_OUT_OF_RANGE)]);
    let mut h = SpiHost::new(Wire::new(&cat(&[wire_r1(0x00), vec![sdspi::ERROR_TOKEN_CARD_ECC_FAILED], recover.clone(), wire_r1(0x00), wire_block(&data)])));
    h.set_bus(Rate::Default, Width::One).unwrap();
    sdcard::read(&mut h, &hc, 7, 1, &mut buf).unwrap();
    // A data response refusing the CRC: retried once after the recovery; a write error is final after it.
    let mut h = SpiHost::new(Wire::new(&cat(&[wire_r1(0x00), vec![sdspi::DATA_CRC_ERROR], recover.clone(), wire_r1(0x00), vec![sdspi::DATA_ACCEPTED, 0xFF], wire_r1(0x00), vec![0x00]])));
    h.set_bus(Rate::Default, Width::One).unwrap();
    sdcard::write(&mut h, &hc, 5, 1, &data).unwrap();
    assert_eq!(h.wire.faults, vec![(0x00, sdspi::DATA_CRC_ERROR)]);
    let mut h = SpiHost::new(Wire::new(&cat(&[wire_r1(0x00), vec![sdspi::DATA_WRITE_ERROR], recover.clone()])));
    h.set_bus(Rate::Default, Width::One).unwrap();
    assert_eq!(sdcard::write(&mut h, &hc, 5, 1, &data), Err(Stuck { step: Step::Transfer, why: Why::Refused(Refusal::Timeout) }));
    // A busy signal that lasts the write time's budget (the bytes of 250 ms at 6 MHz): the host's Busy, final after the recovery.
    let busy = vec![0x00; (6_000_000 / 8_000 * sdspi::WRITE_TIMEOUT_MS) as usize];
    let mut h = SpiHost::new(Wire::new(&cat(&[wire_r1(0x00), vec![sdspi::DATA_ACCEPTED], busy, recover.clone()])));
    h.set_bus(Rate::Default, Width::One).unwrap();
    assert_eq!(sdcard::write(&mut h, &hc, 5, 1, &data), Err(Stuck { step: Step::Transfer, why: Why::Refused(Refusal::Busy) }));
    // The R1 that never comes: the idle line for eight bytes, the native timeout at CMD0.
    let mut h = SpiHost::new(Wire::new(&[]));
    assert_eq!(sdcard::identify(&mut h), Err(Stuck { step: Step::Cmd0, why: Why::Refused(Refusal::Timeout) }));
    assert_eq!(h.wire.faults, vec![(0xFF, 0xFF)]);
    assert_eq!(h.wire.written.len(), sdspi::POWER_UP_BYTES + 6 + 1);
    // A card that never leaves idle: the card layer's budget of polls, a pause between each.
    let mut answers = cat(&[wire_r1(0x01), wire_r1(0x01), vec![0x00, 0x00, 0x01, 0xAA]]);
    for _ in 0..sdcard::ACMD41_TRIES {
        answers.extend(wire_r1(0x01));
        answers.extend(wire_r1(0x01));
    }
    let mut h = SpiHost::new(Wire::new(&answers));
    assert_eq!(sdcard::identify(&mut h), Err(Stuck { step: Step::Acmd41, why: Why::Busy }));
    assert_eq!(h.wire.pauses, sdcard::ACMD41_TRIES - 1);
    // A card that calls CMD55 illegal (not an SD memory card): unsupported.
    let mut h = SpiHost::new(Wire::new(&cat(&[wire_r1(0x01), wire_r1(0x01), vec![0x00, 0x00, 0x01, 0xAA], wire_r1(0x05)])));
    assert_eq!(sdcard::identify(&mut h), Err(Stuck { step: Step::Acmd41, why: Why::Unsupported }));
    // A write-protect violation in R2 after a write: the card layer's ReadOnly.
    let mut h = SpiHost::new(Wire::new(&cat(&[wire_r1(0x00), vec![sdspi::DATA_ACCEPTED, 0xFF], wire_r1(0x00), vec![sdspi::R2_WP_VIOLATION]])));
    h.set_bus(Rate::Default, Width::One).unwrap();
    assert_eq!(sdcard::write(&mut h, &hc, 5, 1, &data), Err(Stuck { step: Step::Cmd13, why: Why::ReadOnly }));
    // The wire's own refusal on a write reaches the card layer as the host's: the overrun is recovered from and retried once.
    let mut h = SpiHost::new(Wire::new(&cat(&[recover.clone(), wire_r1(0x00), wire_block(&data)])));
    h.set_bus(Rate::Default, Width::One).unwrap();
    h.wire.refuse_write = Some(Refusal::Overrun);
    sdcard::read(&mut h, &hc, 7, 1, &mut buf).unwrap();
    assert_eq!(h.wire.written, cat(&[vec![0xFF], wire_cmd(12, 0), wire_cmd(13, 0), wire_cmd(17, 7)]));
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
fn net_peer_dns_answer_with_two_records_parsed_by_the_stack() {
    let mut out = [0u8; net_peer::MAX_REPLY];
    let mut peer = Peer::new(false);
    let query = udp_frame(net_peer::PEER_MAC, net_peer::LEASE_IP, net_peer::PEER_IP, 40003, 53, &dns_query(0x2A2A, "PAIR.rhu.Test"));
    let Verdict::Reply(n) = peer.answer(&query, &mut out) else { panic!("the query for the name with two records was not answered") };
    let (payload, src, dst, sp, dp, mac) = parse_udp_reply(&out[..n]);
    assert_eq!((src, dst, sp, dp, mac), (peer_v4(net_peer::PEER_IP), peer_v4(net_peer::LEASE_IP), 53, 40003, EthernetAddress(net_peer::OUR_MAC)));
    let packet = DnsPacket::new_checked(&payload[..]).expect("a well-formed DNS message");
    assert_eq!(packet.transaction_id(), 0x2A2A);
    assert!(packet.flags().contains(DnsFlags::RESPONSE | DnsFlags::RECURSION_AVAILABLE));
    assert_eq!(packet.rcode(), DnsRcode::NoError);
    assert_eq!((packet.question_count(), packet.answer_record_count()), (1, 2));
    let (mut rest, question) = DnsQuestion::parse(packet.payload()).expect("the question copied back");
    assert_eq!(labels(&packet, question.name), ["PAIR", "rhu", "Test"]);
    assert_eq!(question.type_, DnsQueryType::A);
    for (i, addr) in net_peer::PAIR_IPS.iter().enumerate() {
        let (after, record) = DnsRecord::parse(rest).unwrap_or_else(|_| panic!("the answer record {i} does not parse"));
        assert_eq!(labels(&packet, record.name), ["PAIR", "rhu", "Test"], "the name of record {i} is a pointer to the question's");
        assert_eq!(record.ttl, net_peer::DNS_TTL);
        assert_eq!(record.data, DnsRecordData::A(peer_v4(*addr)), "record {i} in the table's order");
        rest = after;
    }
    assert!(rest.is_empty(), "the message ends at the second record's end ({} bytes follow)", rest.len());
    assert_eq!((peer.dns_answered, peer.dns_refused), (1, 0), "one query answered, whatever its records");
    // The deliberately wrong version: the same reply claiming a third record fails the walk at that record.
    let mut wrong = payload.clone();
    wrong[7] = 3;
    let claimed = DnsPacket::new_checked(&wrong[..]).expect("the header is still well-formed");
    assert_eq!(claimed.answer_record_count(), 3);
    let (mut walk, _) = DnsQuestion::parse(claimed.payload()).expect("the question of the doctored reply");
    for _ in 0..2 {
        walk = DnsRecord::parse(walk).expect("the two records the reply does carry").0;
    }
    assert!(DnsRecord::parse(walk).is_err(), "a third record was parsed from a reply that carries two");
    // A name with one record still carries one.
    let q = udp_frame(net_peer::PEER_MAC, net_peer::LEASE_IP, net_peer::PEER_IP, 40004, 53, &dns_query(8, net_peer::NAMES[0].0));
    let Verdict::Reply(n) = peer.answer(&q, &mut out) else { panic!("the name with one record was not answered") };
    let (single, ..) = parse_udp_reply(&out[..n]);
    assert_eq!(DnsPacket::new_checked(&single[..]).unwrap().answer_record_count(), 1);
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

// ---- The kernel's file API's table (fs.rs) over the sparse device: the routing against the mount
// ---- point, the two handle tables with their generation, the verbs through the table, the codes.
use crate::fs::{self, Dirent, MountDesc, StatDesc, StatfsDesc, Table};

fn fs_no_clock() -> Option<crate::calendar_math::Civil> {
    None
}
/// A table mounted over a fresh copy of the named golden (a 4 KB scratch, as the kernel's).
fn fs_table(name: &str) -> Table<Sparse> {
    let mut t: Table<Sparse> = Table::new();
    let mut scratch = vec![0u8; 4096];
    t.mount(golden(name), &mut scratch, fs_no_clock).unwrap_or_else(|r| panic!("{name}: the table's mount refused: {}", r.words()));
    t
}
/// The root's entries through the table: (name, is_dir, size).
fn fs_list(t: &mut Table<Sparse>, path: &[u8]) -> Vec<(String, bool, u32)> {
    let d = t.opendir(path);
    assert!(d >= 0, "opendir {:?} answered {d}", std::str::from_utf8(path));
    let mut e = Dirent::EMPTY;
    let mut out = vec![];
    loop {
        match t.readdir(d, &mut e) {
            1 => out.push((String::from_utf8_lossy(e.name()).into_owned(), e.kind == fs::KIND_DIR, e.size)),
            0 => break,
            c => panic!("readdir answered {c} after {} entries", out.len()),
        }
    }
    assert_eq!(t.closedir(d), 0);
    out
}
fn fs_read_all(t: &mut Table<Sparse>, h: i32, len: usize) -> Vec<u8> {
    let mut out = vec![0u8; len];
    let mut done = 0;
    while done < len {
        let n = t.read(h, &mut out[done..]);
        assert!(n > 0, "the read answered {n} after {done} bytes");
        done += n as usize;
    }
    out
}

#[test]
fn fs_doors_route_paths_and_refuse_outside_the_mount() {
    // No volume: the table's row exists with the no-card state, every path absent, every handle outside the tables.
    let mut t: Table<Sparse> = Table::new();
    let mut d = MountDesc::EMPTY;
    assert_eq!(t.mount_count(), 1);
    assert_eq!(t.mount_desc(0, &mut d), 0);
    assert_eq!((d.state, d.fat_width, d.clusters, d.device), (fs::STATE_NO_CARD, 0, 0, 0));
    assert_eq!(MountDesc::text(&d.point), b"/media/sd0");
    assert_eq!(t.mount_desc(1, &mut d), fs::ERR_INVAL);
    let mut st = StatDesc::EMPTY;
    assert_eq!(t.stat(b"/media/sd0", &mut st), fs::ERR_NOENT);
    assert_eq!(t.open(b"/media/sd0/x", fs::O_READ), fs::ERR_NOENT);
    assert_eq!(t.opendir(b"/media/sd0", ), fs::ERR_NOENT);
    assert_eq!(t.read(0, &mut [0u8; 4]), fs::ERR_BADF);
    assert_eq!(t.close(0), fs::ERR_BADF);
    assert_eq!(t.closedir(0), fs::ERR_BADF);
    // The flags are judged before the path: an empty flag word, a bit outside the six, the truncate and append bits without the write bit.
    assert_eq!(t.open(b"/media/sd0/x", 0), fs::ERR_INVAL);
    assert_eq!(t.open(b"/media/sd0/x", 64), fs::ERR_INVAL);
    assert_eq!(t.open(b"/media/sd0/x", fs::O_READ | fs::O_TRUNC), fs::ERR_INVAL);
    assert_eq!(t.open(b"/media/sd0/x", fs::O_READ | fs::O_APPEND), fs::ERR_INVAL);
    // A volume: the point itself and the point with a slash are the root; a look-alike, another socket's point and the rest of the tree are absent.
    let mut t = fs_table("fat32-64m");
    assert_eq!(t.mount_desc(0, &mut d), 0);
    assert_eq!((d.state, d.fat_width), (fs::STATE_MOUNTED, 32));
    assert!(d.cluster_bytes >= 512 && d.clusters > 0 && d.free_clusters <= d.clusters);
    assert_eq!(t.stat(b"/media/sd0", &mut st), 0);
    assert_eq!((st.kind, st.size), (fs::KIND_DIR, 0));
    assert_eq!(t.stat(b"/media/sd0/", &mut st), 0);
    assert_eq!(t.stat(b"/media/sd0x", &mut st), fs::ERR_NOENT);
    assert_eq!(t.stat(b"/media/sd1", &mut st), fs::ERR_NOENT);
    assert_eq!(t.stat(b"/media", &mut st), fs::ERR_NOENT);
    assert_eq!(t.stat(b"/tmp/x", &mut st), fs::ERR_NOENT);
    assert_eq!(t.stat(b"/", &mut st), fs::ERR_NOENT);
    assert_eq!(t.stat(b"media/sd0", &mut st), fs::ERR_INVAL);
    assert_eq!(t.stat(b"", &mut st), fs::ERR_INVAL);
    assert_eq!(t.stat(b"/media/sd0/no-such-name", &mut st), fs::ERR_NOENT);
    // The window: a path past 4096 bytes; a component past 255 units.
    let mut long = b"/media/sd0/".to_vec();
    long.extend(std::iter::repeat(b'a').take(4096));
    assert_eq!(t.stat(&long, &mut st), fs::ERR_NAMETOOLONG);
    let mut comp = b"/media/sd0/".to_vec();
    comp.extend(std::iter::repeat(b'b').take(256));
    assert_eq!(t.stat(&comp, &mut st), fs::ERR_NAMETOOLONG);
    assert_eq!(t.open(&comp, fs::O_WRITE | fs::O_CREATE), fs::ERR_NAMETOOLONG);
    // "." and ".." walk as on Linux: the first directory of the root reached through them.
    let entries = fs_list(&mut t, b"/media/sd0");
    assert_eq!(&entries[..2], &[(".".to_string(), true, 0), ("..".to_string(), true, 0)]);
    let dir = entries.iter().skip(2).find(|e| e.1).map(|e| e.0.clone()).expect("the golden's root holds a directory");
    let walked = format!("/media/sd0/./{dir}/../{dir}");
    assert_eq!(t.stat(walked.as_bytes(), &mut st), 0);
    assert_eq!(st.kind, fs::KIND_DIR);
    assert_eq!(t.stat(format!("/media/sd0/{dir}/..").as_bytes(), &mut st), 0);
    // statfs for a path that exists, and not for one that does not.
    let mut sf = StatfsDesc::EMPTY;
    assert_eq!(t.statfs(b"/media/sd0", &mut sf), 0);
    assert_eq!((sf.block_size, sf.blocks, sf.free, sf.name_max), (d.cluster_bytes, d.clusters, d.free_clusters, 255));
    assert_eq!(t.statfs(b"/media/sd0/no-such-name", &mut sf), fs::ERR_NOENT);
    assert_eq!(t.statfs(b"/tmp", &mut sf), fs::ERR_NOENT);
}

#[test]
fn fs_doors_open_read_write_seek_truncate_and_sync() {
    let mut t = fs_table("fat16-32m");
    let p: &[u8] = b"/media/sd0/doors.bin";
    let mut st = StatDesc::EMPTY;
    assert_eq!(t.stat(p, &mut st), fs::ERR_NOENT);
    assert_eq!(t.open(p, fs::O_READ), fs::ERR_NOENT);
    let h = t.open(p, fs::O_READ | fs::O_WRITE | fs::O_CREATE | fs::O_EXCL);
    assert_eq!(h, 0);
    assert_eq!(t.open(p, fs::O_WRITE | fs::O_CREATE | fs::O_EXCL), fs::ERR_EXIST);
    let data: Vec<u8> = (0..100_000u32).map(|i| (i * 7 % 251) as u8).collect();
    // The write, then a positional write of the rest: the handle's position stays where the write left it.
    assert_eq!(t.write(h, &data[..60_000]), 60_000);
    assert_eq!(t.pwrite(h, &data[60_000..], 60_000), 40_000);
    let mut pos = -1i64;
    assert_eq!(t.seek(h, 0, fs::SEEK_CUR, &mut pos), 0);
    assert_eq!(pos, 60_000);
    assert_eq!(t.sync(h), 0);
    assert_eq!(t.fstat(h, &mut st), 0);
    assert_eq!((st.size, st.kind, st.read_only), (100_000, fs::KIND_FILE, 0));
    assert!(st.first_cluster >= 2);
    let mut by_path = StatDesc::EMPTY;
    assert_eq!(t.stat(p, &mut by_path), 0);
    assert_eq!(by_path, st);
    assert_eq!(t.close(h), 0);
    // Read back: an aligned run, then a misaligned run, then by position; the end answers 0.
    let h = t.open(p, fs::O_READ);
    assert_eq!(h, 0);
    let mut back = vec![0u8; 100_016];
    let mut done = 0;
    while done < 50_000 {
        let n = t.read(h, &mut back[done..50_000]);
        assert!(n > 0);
        done += n as usize;
    }
    let mut done = 50_001;
    while done < 100_001 {
        let n = t.read(h, &mut back[done..100_001]);
        assert!(n > 0);
        done += n as usize;
    }
    assert_eq!(&back[..50_000], &data[..50_000]);
    assert_eq!(&back[50_001..100_001], &data[50_000..]);
    assert_eq!(t.read(h, &mut back[..16]), 0);
    assert_eq!(t.seek(h, 0, fs::SEEK_CUR, &mut pos), 0);
    assert_eq!(pos, 100_000);
    let mut piece = vec![0u8; 1000];
    assert_eq!(t.pread(h, &mut piece, 10_000), 1000);
    assert_eq!(&piece[..], &data[10_000..11_000]);
    assert_eq!(t.seek(h, 0, fs::SEEK_CUR, &mut pos), 0);
    assert_eq!(pos, 100_000, "the positional read moved the handle's position");
    assert_eq!(t.pread(h, &mut piece, 100_000), 0);
    assert_eq!(t.pread(h, &mut piece, 99_990), 10);
    assert_eq!(&piece[..10], &data[99_990..]);
    assert_eq!(t.pread(h, &mut piece, -1), fs::ERR_INVAL);
    assert_eq!(t.pread(h, &mut piece, 1 << 32), fs::ERR_INVAL);
    // The seek by every whence; past the end allowed; below 0 or past the layer's size refused; a bad whence refused.
    assert_eq!(t.seek(h, -10, fs::SEEK_END, &mut pos), 0);
    assert_eq!(pos, 99_990);
    assert_eq!(t.read(h, &mut piece[..100]), 10);
    assert_eq!(&piece[..10], &data[99_990..]);
    assert_eq!(t.seek(h, 200_000, fs::SEEK_SET, &mut pos), 0);
    assert_eq!(pos, 200_000);
    assert_eq!(t.read(h, &mut piece), 0);
    assert_eq!(t.seek(h, -1, fs::SEEK_SET, &mut pos), fs::ERR_INVAL);
    assert_eq!(t.seek(h, 1 << 32, fs::SEEK_SET, &mut pos), fs::ERR_INVAL);
    assert_eq!(t.seek(h, 0, 3, &mut pos), fs::ERR_INVAL);
    // A read-only handle takes no write, truncate or positional write.
    assert_eq!(t.write(h, &data[..10]), fs::ERR_BADF);
    assert_eq!(t.pwrite(h, &data[..10], 0), fs::ERR_BADF);
    assert_eq!(t.truncate(h, 10), fs::ERR_BADF);
    assert_eq!(t.close(h), 0);
    // The truncate both ways, with the gap zeroed; the limits refused.
    let h = t.open(p, fs::O_READ | fs::O_WRITE);
    assert_eq!(t.truncate(h, 4096), 0);
    assert_eq!(t.fstat(h, &mut st), 0);
    assert_eq!(st.size, 4096);
    assert_eq!(t.truncate(h, 8192), 0);
    assert_eq!(t.seek(h, 0, fs::SEEK_SET, &mut pos), 0);
    let whole = fs_read_all(&mut t, h, 8192);
    assert_eq!(&whole[..4096], &data[..4096]);
    assert!(whole[4096..].iter().all(|&b| b == 0), "the gap of a zero-extending truncate is not zero");
    assert_eq!(t.truncate(h, -1), fs::ERR_INVAL);
    assert_eq!(t.truncate(h, 1 << 32), fs::ERR_INVAL);
    // A positional write past the end allocates and zeroes the gap, the position unmoved.
    assert_eq!(t.seek(h, 100, fs::SEEK_SET, &mut pos), 0);
    assert_eq!(t.pwrite(h, &data[..100], 10_000), 100);
    assert_eq!(t.seek(h, 0, fs::SEEK_CUR, &mut pos), 0);
    assert_eq!(pos, 100);
    assert_eq!(t.fstat(h, &mut st), 0);
    assert_eq!(st.size, 10_100);
    assert_eq!(t.pread(h, &mut piece, 9_000), 1000);
    assert!(piece[..].iter().all(|&b| b == 0));
    assert_eq!(t.pread(h, &mut piece[..100], 10_000), 100);
    assert_eq!(&piece[..100], &data[..100]);
    assert_eq!(t.close(h), 0);
    // The truncate flag at the open; the append flag's writes at the end and its refusal of a positional write.
    let h = t.open(p, fs::O_WRITE | fs::O_TRUNC);
    assert_eq!(t.fstat(h, &mut st), 0);
    assert_eq!(st.size, 0);
    assert_eq!(t.write(h, &data[..10]), 10);
    assert_eq!(t.close(h), 0);
    let h = t.open(p, fs::O_WRITE | fs::O_APPEND);
    assert_eq!(t.write(h, &data[10..20]), 10);
    assert_eq!(t.pwrite(h, &data[..10], 0), fs::ERR_INVAL);
    assert_eq!(t.close(h), 0);
    assert_eq!(t.stat(p, &mut st), 0);
    assert_eq!(st.size, 20);
    let h = t.open(p, fs::O_READ);
    assert_eq!(fs_read_all(&mut t, h, 20), &data[..20]);
    assert_eq!(t.close(h), 0);
    assert_eq!(t.unlink(p), 0);
    assert_eq!(t.stat(p, &mut st), fs::ERR_NOENT);
    // A read-only entry, where a golden carries one: opened for writing and unlinked, the read-only code.
    let mut ro = None;
    for name in GOLDENS {
        let mut v = mount(name);
        let mut d = v.opendir(b"/").unwrap();
        let mut e = Entry::EMPTY;
        while v.readdir(&mut d, &mut e).unwrap() {
            if e.read_only() && !e.is_dir() {
                ro = Some((name, String::from_utf8_lossy(e.name()).into_owned()));
            }
        }
        v.closedir(&mut d).unwrap();
    }
    match ro {
        Some((name, entry)) => {
            let mut t = fs_table(name);
            let path = format!("/media/sd0/{entry}");
            assert_eq!(t.stat(path.as_bytes(), &mut st), 0);
            assert_eq!(st.read_only, 1);
            assert_eq!(t.open(path.as_bytes(), fs::O_WRITE), fs::ERR_ROFS);
            assert_eq!(t.unlink(path.as_bytes()), fs::ERR_ROFS);
            let h = t.open(path.as_bytes(), fs::O_READ);
            assert!(h >= 0);
            assert_eq!(t.close(h), 0);
        }
        None => println!("fs_doors: no golden carries a read-only file at its root; the read-only code is the layer's own case's"),
    }
    // write(2)'s rule through the table on the smallest golden: the bytes moved before the volume filled, then the no-space code.
    let mut t = fs_table("fat12-1m");
    let h = t.open(b"/media/sd0/fill.bin", fs::O_WRITE | fs::O_CREATE | fs::O_EXCL);
    assert!(h >= 0);
    let chunk = vec![0x5Au8; 64 * 1024];
    let mut total = 0u64;
    let mut short = false;
    for _ in 0..64 {
        let n = t.write(h, &chunk);
        if n == fs::ERR_NOSPC {
            assert!(short, "the no-space code came before a short write");
            break;
        }
        assert!(n > 0, "the write answered {n}");
        total += n as u64;
        if (n as usize) < chunk.len() {
            short = true;
        }
    }
    assert!(short && total > 0, "the volume never filled ({total} bytes written)");
    assert_eq!(t.write(h, &chunk), fs::ERR_NOSPC);
    assert_eq!(t.close(h), 0);
    assert_eq!(t.stat(b"/media/sd0/fill.bin", &mut st), 0);
    assert_eq!(st.size as u64, total);
    assert_eq!(t.unlink(b"/media/sd0/fill.bin"), 0);
}

#[test]
fn fs_doors_handle_tables_and_their_limits() {
    let mut t = fs_table("fat32-64m");
    let p: &[u8] = b"/media/sd0/handles.bin";
    let h = t.open(p, fs::O_WRITE | fs::O_CREATE | fs::O_EXCL);
    assert_eq!(h, 0);
    assert_eq!(t.write(h, b"sixteen handles"), 15);
    assert_eq!(t.close(h), 0);
    // Sixteen file handles, the seventeenth refused; a close frees its slot and the next open takes it.
    let handles: Vec<i32> = (0..fs::FILES).map(|_| t.open(p, fs::O_READ)).collect();
    assert_eq!(handles, (0..fs::FILES as i32).collect::<Vec<_>>());
    assert_eq!(t.open(p, fs::O_READ), fs::ERR_MFILE);
    assert_eq!(t.close(3), 0);
    assert_eq!(t.open(p, fs::O_READ), 3);
    assert_eq!(t.open(p, fs::O_READ), fs::ERR_MFILE);
    for h in 0..fs::FILES as i32 {
        assert_eq!(t.close(h), 0);
    }
    // A closed handle, and the numbers outside the table.
    let mut buf = [0u8; 8];
    for h in [0, 15, 16, -1, 1_000_000, i32::MIN, i32::MAX] {
        assert_eq!(t.read(h, &mut buf), fs::ERR_BADF, "handle {h}");
        assert_eq!(t.write(h, &buf), fs::ERR_BADF, "handle {h}");
        assert_eq!(t.close(h), fs::ERR_BADF, "handle {h}");
        let mut st = StatDesc::EMPTY;
        assert_eq!(t.fstat(h, &mut st), fs::ERR_BADF, "handle {h}");
        let mut pos = 0;
        assert_eq!(t.seek(h, 0, fs::SEEK_SET, &mut pos), fs::ERR_BADF, "handle {h}");
        assert_eq!(t.sync(h), fs::ERR_BADF, "handle {h}");
        assert_eq!(t.truncate(h, 0), fs::ERR_BADF, "handle {h}");
        assert_eq!(t.pread(h, &mut buf, 0), fs::ERR_BADF, "handle {h}");
        assert_eq!(t.pwrite(h, &buf, 0), fs::ERR_BADF, "handle {h}");
    }
    // Four directory handles, the fifth refused; the numbers of the two tables are separate spaces.
    let dirs: Vec<i32> = (0..fs::DIRS).map(|_| t.opendir(b"/media/sd0")).collect();
    assert_eq!(dirs, (0..fs::DIRS as i32).collect::<Vec<_>>());
    assert_eq!(t.opendir(b"/media/sd0"), fs::ERR_MFILE);
    let f = t.open(p, fs::O_READ);
    assert_eq!(f, 0);
    let mut e = Dirent::EMPTY;
    assert_eq!(t.readdir(0, &mut e), 1, "the directory table's 0 is a directory while the file table's 0 is a file");
    assert_eq!(e.name(), b".");
    assert_eq!(t.read(0, &mut buf), 8);
    assert_eq!(&buf, b"sixteen ");
    assert_eq!(t.readdir(4, &mut e), fs::ERR_BADF);
    assert_eq!(t.readdir(-1, &mut e), fs::ERR_BADF);
    for d in dirs {
        assert_eq!(t.closedir(d), 0);
    }
    assert_eq!(t.closedir(0), fs::ERR_BADF);
    assert_eq!(t.readdir(0, &mut e), fs::ERR_BADF);
    assert_eq!(t.read(0, &mut buf), 7);
    assert_eq!(t.close(f), 0);
    assert_eq!(t.unlink(p), 0);
}

#[test]
fn fs_doors_directories_listing_and_the_tree_verbs() {
    let mut t = fs_table("fat32-64m");
    // The listing through the table equals the layer's own listing of the golden: names, kinds, sizes, the dots first.
    let ours = fs_list(&mut t, b"/media/sd0");
    let mut v = mount("fat32-64m");
    let theirs: Vec<(String, bool, u32)> = list(&mut v, "/").into_iter().map(|e| (e.0, e.1, if e.1 { 0 } else { e.2 })).collect();
    assert_eq!(ours, theirs, "the table's listing differs from the layer's");
    assert!(ours.len() > 2);
    // The tree verbs: a directory made (once), a file inside it, the refusals, the rename within the volume and onto the kinds, the removal.
    let d1: &[u8] = b"/media/sd0/d1";
    let inner: &[u8] = b"/media/sd0/d1/inner.txt";
    let mut st = StatDesc::EMPTY;
    assert_eq!(t.mkdir(d1), 0);
    assert_eq!(t.mkdir(d1), fs::ERR_EXIST);
    assert_eq!(t.mkdir(b"/media/sd0/no-such-parent/x"), fs::ERR_NOENT);
    assert_eq!(t.stat(d1, &mut st), 0);
    assert_eq!((st.kind, st.size), (fs::KIND_DIR, 0));
    assert_eq!(fs_list(&mut t, d1), vec![(".".to_string(), true, 0), ("..".to_string(), true, 0)]);
    let h = t.open(inner, fs::O_WRITE | fs::O_CREATE | fs::O_EXCL);
    assert!(h >= 0);
    assert_eq!(t.write(h, b"inner"), 5);
    assert_eq!(t.close(h), 0);
    assert_eq!(fs_list(&mut t, d1)[2], ("inner.txt".to_string(), false, 5));
    assert_eq!(t.open(d1, fs::O_READ), fs::ERR_ISDIR);
    assert_eq!(t.opendir(inner), fs::ERR_NOTDIR);
    assert_eq!(t.stat(b"/media/sd0/d1/inner.txt/x", &mut st), fs::ERR_NOTDIR);
    assert_eq!(t.rmdir(d1), fs::ERR_NOTEMPTY);
    assert_eq!(t.rmdir(inner), fs::ERR_NOTDIR);
    assert_eq!(t.unlink(d1), fs::ERR_ISDIR);
    assert_eq!(t.rmdir(b"/media/sd0"), fs::ERR_INVAL);
    assert_eq!(t.rename(inner, b"/media/sd0/moved.txt"), 0);
    assert_eq!(t.stat(inner, &mut st), fs::ERR_NOENT);
    assert_eq!(t.stat(b"/media/sd0/moved.txt", &mut st), 0);
    assert_eq!(st.size, 5);
    assert_eq!(t.rename(b"/media/sd0/moved.txt", d1), fs::ERR_EXIST);
    assert_eq!(t.rename(b"/media/sd0/moved.txt", b"/tmp/moved.txt"), fs::ERR_NOENT);
    assert_eq!(t.rename(b"/media/sd0/no-such", b"/media/sd0/x"), fs::ERR_NOENT);
    let h = t.open(b"/media/sd0/other.txt", fs::O_WRITE | fs::O_CREATE | fs::O_EXCL);
    assert_eq!(t.close(h), 0);
    assert_eq!(t.rename(b"/media/sd0/moved.txt", b"/media/sd0/other.txt"), 0);
    assert_eq!(t.stat(b"/media/sd0/moved.txt", &mut st), fs::ERR_NOENT);
    assert_eq!(t.stat(b"/media/sd0/other.txt", &mut st), 0);
    assert_eq!(st.size, 5);
    assert_eq!(t.rename(d1, b"/media/sd0/d2"), 0);
    assert_eq!(t.stat(b"/media/sd0/d2", &mut st), 0);
    assert_eq!(t.rmdir(b"/media/sd0/d2"), 0);
    assert_eq!(t.unlink(b"/media/sd0/other.txt"), 0);
    assert_eq!(t.stat(b"/media/sd0/d2", &mut st), fs::ERR_NOENT);
    let after = fs_list(&mut t, b"/media/sd0");
    assert_eq!(after, theirs, "the root is not as it was after the tree verbs");
    // The counts through the table equal the layer's.
    let mut sf = StatfsDesc::EMPTY;
    assert_eq!(t.statfs(b"/media/sd0", &mut sf), 0);
    let s = v.statfs();
    assert_eq!((sf.block_size, sf.blocks, sf.name_max), (s.block_size, s.blocks, 255));
    assert_eq!(sf.free, s.free, "the free count after the verbs is not the untouched golden's");
}

#[test]
fn fs_doors_unmount_kills_handles_and_a_remount_starts_fresh() {
    let mut t = fs_table("fat16-32m");
    let g0 = t.generation();
    let p: &[u8] = b"/media/sd0/kept.bin";
    let h = t.open(p, fs::O_READ | fs::O_WRITE | fs::O_CREATE | fs::O_EXCL);
    assert_eq!(h, 0);
    assert_eq!(t.write(h, b"kept"), 4);
    assert_eq!(t.sync(h), 0);
    let d = t.opendir(b"/media/sd0");
    assert_eq!(d, 0);
    // The card leaves: the volume dropped, the state no card, the handles dead until their close, which answers 0.
    t.unmount();
    assert!(!t.mounted() && t.state() == fs::STATE_NO_CARD && t.generation() != g0);
    let mut d_rec = MountDesc::EMPTY;
    assert_eq!(t.mount_desc(0, &mut d_rec), 0);
    assert_eq!((d_rec.state, d_rec.fat_width, d_rec.clusters, MountDesc::text(&d_rec.why)), (fs::STATE_NO_CARD, 0, 0, &b""[..]));
    let mut buf = [0u8; 4];
    let mut st = StatDesc::EMPTY;
    let mut e = Dirent::EMPTY;
    let mut pos = 0;
    assert_eq!(t.read(h, &mut buf), fs::ERR_IO);
    assert_eq!(t.write(h, &buf), fs::ERR_IO);
    assert_eq!(t.pread(h, &mut buf, 0), fs::ERR_IO);
    assert_eq!(t.fstat(h, &mut st), fs::ERR_IO);
    assert_eq!(t.seek(h, 0, fs::SEEK_SET, &mut pos), fs::ERR_IO);
    assert_eq!(t.sync(h), fs::ERR_IO);
    assert_eq!(t.readdir(d, &mut e), fs::ERR_IO);
    assert_eq!(t.stat(b"/media/sd0", &mut st), fs::ERR_NOENT);
    assert_eq!(t.open(p, fs::O_READ), fs::ERR_NOENT);
    assert_eq!(t.close(h), 0);
    assert_eq!(t.close(h), fs::ERR_BADF);
    assert_eq!(t.closedir(d), 0);
    assert_eq!(t.closedir(d), fs::ERR_BADF);
    // A handle kept across a remount is dead too: the generation moved twice.
    let mut scratch = vec![0u8; 4096];
    t.mount(golden("fat16-32m"), &mut scratch, fs_no_clock).unwrap();
    let kept = t.open(b"/media/sd0", fs::O_READ);
    assert_eq!(kept, fs::ERR_ISDIR);
    let h1 = t.open(b"/media/sd0/kept.bin", fs::O_WRITE | fs::O_CREATE);
    assert_eq!(h1, 0);
    let g1 = t.generation();
    t.unmount();
    t.mount(golden("fat16-32m"), &mut scratch, fs_no_clock).unwrap();
    assert!(t.generation() != g1 && t.mounted() && t.state() == fs::STATE_MOUNTED);
    assert_eq!(t.read(h1, &mut buf), fs::ERR_IO);
    assert_eq!(t.close(h1), 0);
    // A fresh copy of the golden stands: the file of the first mount is not there (the device is a new copy), a fresh open takes slot 0.
    assert_eq!(t.stat(b"/media/sd0/kept.bin", &mut st), fs::ERR_NOENT);
    let h2 = t.open(b"/media/sd0/fresh.bin", fs::O_WRITE | fs::O_CREATE | fs::O_EXCL);
    assert_eq!(h2, 0);
    assert_eq!(t.close(h2), 0);
    // A refused volume: the unreadable state with the refusal's words, kept until the unmount.
    assert!(t.mount(Sparse::blank(2048), &mut scratch, fs_no_clock).is_err());
    assert_eq!((t.state(), t.why(), t.mounted()), (fs::STATE_UNREADABLE, "no boot signature", false));
    assert_eq!(t.mount_desc(0, &mut d_rec), 0);
    assert_eq!((d_rec.state, MountDesc::text(&d_rec.why)), (fs::STATE_UNREADABLE, &b"no boot signature"[..]));
    assert_eq!(t.open(b"/media/sd0/x", fs::O_READ), fs::ERR_NOENT);
    t.unmount();
    assert_eq!((t.state(), t.why()), (fs::STATE_NO_CARD, ""));
    // A device that answers no device at the mount is the no-card state, not unreadable.
    struct Gone;
    impl crate::fat::BlockDevice for Gone {
        fn blocks(&mut self) -> u32 {
            0
        }
        fn read(&mut self, _lba: u32, _buf: &mut [u8]) -> Result<(), crate::fat::Error> {
            Err(crate::fat::Error::NoDevice)
        }
        fn write(&mut self, _lba: u32, _data: &[u8]) -> Result<(), crate::fat::Error> {
            Err(crate::fat::Error::NoDevice)
        }
        fn sync(&mut self) -> Result<(), crate::fat::Error> {
            Err(crate::fat::Error::NoDevice)
        }
    }
    let mut g: Table<Gone> = Table::new();
    assert!(g.mount(Gone, &mut scratch, fs_no_clock).is_err());
    assert_eq!((g.state(), g.why()), (fs::STATE_NO_CARD, ""));
}

#[test]
fn fs_doors_no_volume_shape() {
    // A socket with no card: the table's row with the no-card state and no figures, every path absent, every handle outside the tables, the flags judged first.
    let mut t: Table<Sparse> = Table::new();
    assert_eq!((t.state(), t.mounted(), t.fat_width(), t.why()), (fs::STATE_NO_CARD, false, 0, ""));
    let mut d = MountDesc::EMPTY;
    assert_eq!(t.mount_desc(0, &mut d), 0);
    assert_eq!(d, MountDesc { point: *b"/media/sd0\0\0\0\0\0\0", device: 0, state: fs::STATE_NO_CARD, fat_width: 0, cluster_bytes: 0, clusters: 0, free_clusters: 0, label: [0; 12], why: [0; 24] });
    let mut st = StatDesc::EMPTY;
    let mut sf = StatfsDesc::EMPTY;
    let mut e = Dirent::EMPTY;
    let mut pos = 0;
    let mut buf = [0u8; 4];
    for p in [&b"/media/sd0"[..], b"/media/sd0/x", b"/", b"/tmp/x"] {
        assert_eq!(t.open(p, fs::O_READ), fs::ERR_NOENT);
        assert_eq!(t.stat(p, &mut st), fs::ERR_NOENT);
        assert_eq!(t.unlink(p), fs::ERR_NOENT);
        assert_eq!(t.mkdir(p), fs::ERR_NOENT);
        assert_eq!(t.rmdir(p), fs::ERR_NOENT);
        assert_eq!(t.rename(p, b"/media/sd0/y"), fs::ERR_NOENT);
        assert_eq!(t.opendir(p), fs::ERR_NOENT);
        assert_eq!(t.statfs(p, &mut sf), fs::ERR_NOENT);
    }
    assert_eq!(t.open(b"media/sd0/x", fs::O_READ), fs::ERR_INVAL);
    assert_eq!(t.open(b"/media/sd0/x", 0), fs::ERR_INVAL);
    for h in [0, 3, 15, 16, -1] {
        assert_eq!(t.read(h, &mut buf), fs::ERR_BADF);
        assert_eq!(t.close(h), fs::ERR_BADF);
        assert_eq!(t.fstat(h, &mut st), fs::ERR_BADF);
        assert_eq!(t.seek(h, 0, fs::SEEK_SET, &mut pos), fs::ERR_BADF);
        assert_eq!(t.readdir(h, &mut e), fs::ERR_BADF);
        assert_eq!(t.closedir(h), fs::ERR_BADF);
    }
    assert_eq!(t.seek(0, 0, 9, &mut pos), fs::ERR_INVAL, "a bad whence is judged before the handle");
    assert_eq!(t.pread(0, &mut buf, -5), fs::ERR_INVAL, "a bad offset is judged before the handle");
    // The records' sizes are the header's.
    assert_eq!((std::mem::size_of::<MountDesc>(), std::mem::size_of::<StatDesc>(), std::mem::size_of::<Dirent>(), std::mem::size_of::<StatfsDesc>()), (76, 40, 264, 16));
}

// ---------------- the wired medium's arithmetic (eth_math.rs) and the transceiver's decisions (board/phy_lan8742.rs) ----------------
use crate::eth_math::{self, MdcDivider, RxVerdict};
use crate::phy_lan8742::{self as phy, Decision, Mdio, MdioTimeout, Readings, Resolution};

/// A unique identifier as it stands in memory: three little-endian words.
fn unique_id(words: [u32; 3]) -> [u8; 12] {
    let mut id = [0u8; 12];
    for (i, w) in words.iter().enumerate() {
        id[i * 4..i * 4 + 4].copy_from_slice(&w.to_le_bytes());
    }
    id
}

#[test]
fn eth_math_address_is_a_digest_of_the_unique_id() {
    let a = unique_id([0x0123_4567, 0x89AB_CDEF, 0x0F1E_2D3C]);
    let b = unique_id([0x0123_4567, 0x89AB_CDEF, 0x0F1E_2D3D]);
    let first = eth_math::address_from_unique_id(&a);
    // The same twelve bytes give the same six.
    assert_eq!(first, eth_math::address_from_unique_id(&a));
    // Locally administered (bit 1 of the first byte set) and unicast (bit 0 clear), whatever the identifier.
    for id in [a, b, [0u8; 12], [0xFF; 12]] {
        let m = eth_math::address_from_unique_id(&id);
        assert_eq!(m[0] & 0b11, 0b10, "the first byte of {m:02x?}");
    }
    // The fold by hand for the all-zero identifier: twelve rounds of FNV-1a from the offset basis.
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for _ in 0..12 {
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    let z = h.to_be_bytes();
    assert_eq!(eth_math::address_from_unique_id(&[0u8; 12]), [(z[0] & 0xFC) | 0x02, z[1], z[2], z[3], z[4], z[5]]);
    // Two identifiers one bit apart, in the last word and in the first, give different addresses.
    assert_ne!(first, eth_math::address_from_unique_id(&b));
    assert_ne!(first, eth_math::address_from_unique_id(&unique_id([0x0123_4566, 0x89AB_CDEF, 0x0F1E_2D3C])));
    // The address in the controller's two registers: the first byte on the wire is the low register's lowest byte,
    // the fifth and the sixth are the high register's low sixteen bits.
    assert_eq!(eth_math::address_words(&[0x02, 0x11, 0x22, 0x33, 0x44, 0x55]), (0x0000_5544, 0x3322_1102));
    // The control: a digest that drops the last word cannot tell the first pair apart.
    let doctored = |id: &[u8; 12]| {
        let mut cut = *id;
        cut[8..].fill(0);
        eth_math::address_from_unique_id(&cut)
    };
    assert_eq!(doctored(&a), doctored(&b), "the doctored digest is refused by the assertion on the last word");
}

#[test]
fn eth_math_mdc_divider_follows_the_manual_and_refuses_under_20_mhz() {
    // RM0410's five rows, each at its two ends.
    let rows = [(20_000_000, 34_999_999, 0b010, 16), (35_000_000, 59_999_999, 0b011, 26), (60_000_000, 99_999_999, 0b000, 42), (100_000_000, 149_999_999, 0b001, 62), (150_000_000, 216_000_000, 0b100, 102)];
    for (lo, hi, code, divisor) in rows {
        assert_eq!(eth_math::mdc_divider(lo), Some(MdcDivider { code, divisor }), "{lo} Hz");
        assert_eq!(eth_math::mdc_divider(hi), Some(MdcDivider { code, divisor }), "{hi} Hz");
        // The management clock the row gives never passes the 2.5 MHz of clause 22.
        assert!(hi / divisor <= 2_500_000, "{hi} Hz over {divisor}");
    }
    // The STM32F769 row's two clocks: the PLL's 216 MHz has its row, the internal oscillator's 16 MHz has none.
    assert_eq!(eth_math::mdc_divider(216_000_000), Some(MdcDivider { code: 0b100, divisor: 102 }));
    assert_eq!(eth_math::mdc_divider(16_000_000), None);
    assert_eq!(eth_math::mdc_divider(19_999_999), None);
    assert_eq!(eth_math::mdc_divider(216_000_001), None);
    assert_eq!(eth_math::mdc_divider(0), None);
    // The control: a table whose first row starts at 16 MHz clocks the wire from a clock the manual gives no row;
    // the refusal above is what tells the two apart.
    let doctored = |hz: u32| if (16_000_000..20_000_000).contains(&hz) { Some(MdcDivider { code: 0b010, divisor: 16 }) } else { eth_math::mdc_divider(hz) };
    assert_ne!(doctored(16_000_000), eth_math::mdc_divider(16_000_000));
}

/// A receive status word: the frame length field with the marks given.
fn rdes0(length: u32, marks: u32) -> u32 {
    (length << eth_math::RDES0_FL_SHIFT) | marks
}

#[test]
fn eth_math_receive_descriptor_whole_frame() {
    let whole = eth_math::RDES0_FS | eth_math::RDES0_LS;
    // A 64-byte frame on the wire is 60 bytes for the stack; the largest untagged frame, 1518, is 1514.
    assert_eq!(eth_math::rx_judge(rdes0(64, whole), 1536), RxVerdict::Frame(60));
    assert_eq!(eth_math::rx_judge(rdes0(1518, whole), 1536), RxVerdict::Frame(1514));
    assert_eq!(eth_math::rx_judge(rdes0(1536, whole), 1536), RxVerdict::Frame(1532));
    // The header alone with its check sequence is the shortest frame handed up.
    assert_eq!(eth_math::rx_judge(rdes0(18, whole), 1536), RxVerdict::Frame(14));
    // The field's fourteen bits and their place, bits 29 to 16: 05EEh is 1518.
    assert_eq!(eth_math::rx_judge(0x05EE_0300, 1536), RxVerdict::Frame(1514));
    // The control: a judgement that forgets the check sequence hands the stack four bytes that are not the frame's.
    let doctored = |w: u32| ((w >> 16) & 0x3FFF) as usize;
    assert_ne!(RxVerdict::Frame(doctored(rdes0(64, whole))), eth_math::rx_judge(rdes0(64, whole), 1536));
}

#[test]
fn eth_math_receive_descriptor_errored_is_refused() {
    let whole = eth_math::RDES0_FS | eth_math::RDES0_LS;
    // The error summary set on a whole frame.
    assert_eq!(eth_math::rx_judge(rdes0(64, whole | eth_math::RDES0_ES), 1536), RxVerdict::Errored);
    // A length no frame in the buffer can have: under the header and its check sequence, or over the buffer.
    assert_eq!(eth_math::rx_judge(rdes0(17, whole), 1536), RxVerdict::Errored);
    assert_eq!(eth_math::rx_judge(rdes0(0, whole), 1536), RxVerdict::Errored);
    assert_eq!(eth_math::rx_judge(rdes0(1537, whole), 1536), RxVerdict::Errored);
    // The error summary is bit 15 and no other: every other bit of the low half leaves a whole frame whole.
    for bit in (0..15).filter(|b| *b != 8 && *b != 9) {
        assert_eq!(eth_math::rx_judge(rdes0(64, whole | 1 << bit), 1536), RxVerdict::Frame(60), "bit {bit}");
    }
    // The control: a judgement blind to the summary hands an errored frame up.
    let doctored = |w: u32| eth_math::rx_judge(w & !eth_math::RDES0_ES, 1536);
    assert_ne!(doctored(rdes0(64, whole | eth_math::RDES0_ES)), RxVerdict::Errored);
}

#[test]
fn eth_math_receive_descriptor_fragment_is_refused() {
    // A first part without the last, a last part without the first, a middle part: none is a frame.
    assert_eq!(eth_math::rx_judge(rdes0(1536, eth_math::RDES0_FS), 1536), RxVerdict::Fragment);
    assert_eq!(eth_math::rx_judge(rdes0(200, eth_math::RDES0_LS), 1536), RxVerdict::Fragment);
    assert_eq!(eth_math::rx_judge(rdes0(1536, 0), 1536), RxVerdict::Fragment);
    // A part is a fragment whatever its error summary says (the summary is the last part's to carry).
    assert_eq!(eth_math::rx_judge(rdes0(1536, eth_math::RDES0_FS | eth_math::RDES0_ES), 1536), RxVerdict::Fragment);
    // The control: a judgement that asks for the last mark alone takes a giant frame's tail for a frame.
    let doctored = |w: u32| if w & eth_math::RDES0_LS != 0 { eth_math::rx_judge(w | eth_math::RDES0_FS, 1536) } else { RxVerdict::Fragment };
    assert_eq!(doctored(rdes0(200, eth_math::RDES0_LS)), RxVerdict::Frame(196));
}

#[test]
fn eth_math_transmit_descriptor_words() {
    // One whole frame handed to the DMA: the owner bit 31, the completion interrupt 30, the last segment 29, the
    // first segment 28, the chained mark 20; the length in the low thirteen bits of the second word.
    assert_eq!(eth_math::tx_commit(60), (0xF010_0000, 60));
    assert_eq!(eth_math::tx_commit(1514), (0xF010_0000, 1514));
    assert_eq!(eth_math::tx_commit(1536).1, 1536);
    // At rest the descriptor is the host's: the chained mark alone, the owner bit clear.
    assert_eq!(eth_math::TDES0_IDLE, 0x0010_0000);
    assert_eq!(eth_math::TDES0_IDLE & eth_math::TDES0_OWN, 0);
    // The error summary the DMA writes back is bit 15, outside every bit a commit sets.
    assert_eq!(eth_math::TDES0_ES, 0x0000_8000);
    assert_eq!(eth_math::tx_commit(60).0 & eth_math::TDES0_ES, 0);
    // A receive descriptor armed: the DMA's, chained, its buffer's size.
    assert_eq!(eth_math::rx_arm(1536), (0x8000_0000, 0x4000 | 1536));
    // The control: a commit without the owner bit never leaves the ring -- its word is not the one above.
    let doctored = |len: usize| (eth_math::tx_commit(len).0 & !eth_math::TDES0_OWN, len as u32);
    assert_ne!(doctored(60), eth_math::tx_commit(60));
}

/// The datasheet's register values: the status register's abilities (100 and 10 Mbit, both duplexes, negotiation
/// able, extended capabilities) with the link and the negotiation's bits as given; register 31 with its reserved
/// field as the part reads it (0000010b in bits 11 to 5).
const BMSR_ABILITIES: u16 = 0x7809;
const SCSR_BASE: u16 = 0x0040;

fn readings(bmcr: u16, first: u16, second: u16, scsr: u16) -> Readings {
    Readings { bmcr, bmsr_first: BMSR_ABILITIES | first, bmsr_second: BMSR_ABILITIES | second, scsr: SCSR_BASE | scsr }
}

/// A management interface that answers a script: each read must name the next register of the script.
struct Scripted {
    phy: u8,
    script: &'static [(u8, u16)],
    at: usize,
    written: Option<(u8, u16)>,
}

impl Mdio for Scripted {
    fn read(&mut self, phy: u8, reg: u8) -> Result<u16, MdioTimeout> {
        assert_eq!(phy, self.phy, "the transceiver's address");
        let Some(&(want, value)) = self.script.get(self.at) else { return Err(MdioTimeout) };
        assert_eq!(reg, want, "the register of read {}", self.at);
        self.at += 1;
        Ok(value)
    }
    fn write(&mut self, phy: u8, reg: u8, value: u16) -> Result<(), MdioTimeout> {
        assert_eq!(phy, self.phy, "the transceiver's address");
        self.written = Some((reg, value));
        Ok(())
    }
}

#[test]
fn phy_lan8742_decision_has_four_outcomes() {
    let up = phy::BMSR_LINK;
    let done = phy::BMSR_ANEG_DONE;
    let hundred_full = phy::SCSR_AUTODONE | 0b110 << 2;
    // Keep: the link up and the negotiation done by both registers; the first status reading low is the kept drop of
    // a cold part and changes nothing.
    assert_eq!(readings(0x3100, up | done, up | done, hundred_full).decide(), Decision::Keep);
    assert_eq!(readings(0x1000, 0, up | done, hundred_full).decide(), Decision::Keep);
    // Wait: the link up, the negotiation enabled and still running.
    assert_eq!(readings(0x1000, up, up, 0).decide(), Decision::Wait);
    // The status register says done and register 31 does not yet: still a wait.
    assert_eq!(readings(0x1000, up, up | done, 0b110 << 2).decide(), Decision::Wait);
    // Restart: the link up with the negotiation disabled -- a forced link (the control register 2100h, and 0000h).
    assert_eq!(readings(0x2100, up, up, 0b110 << 2).decide(), Decision::Restart);
    assert_eq!(readings(0x0000, up, up, 0b001 << 2).decide(), Decision::Restart);
    // Down: no link at the second reading, whatever the rest says.
    assert_eq!(readings(0x1000, up | done, done, hundred_full).decide(), Decision::Down);
    assert_eq!(readings(0x1000, 0, 0, 0).decide(), Decision::Down);
    // A link that went away and came back since the reading before: the first reading low, the second high.
    assert!(readings(0x1000, 0, up | done, hundred_full).dropped_since());
    assert!(!readings(0x1000, up | done, up | done, hundred_full).dropped_since());
    assert!(!readings(0x1000, 0, 0, 0).dropped_since());
    // The words are read in their order through the management interface: control, status, status, register 31.
    let mut wire = Scripted { phy: 0, script: &[(phy::REG_BMCR, 0x1000), (phy::REG_BMSR, BMSR_ABILITIES), (phy::REG_BMSR, BMSR_ABILITIES | 0x0024), (phy::REG_SCSR, 0x1058)], at: 0, written: None };
    let r = phy::read_state(&mut wire, 0).unwrap();
    assert_eq!((r.decide(), r.dropped_since(), wire.at), (Decision::Keep, true, 4));
    // The control: a decision that trusts the first status reading takes a healthy link for a dead one on every cold boot.
    let doctored = |r: &Readings| if r.bmsr_first & phy::BMSR_LINK == 0 { Decision::Down } else { r.decide() };
    assert_ne!(doctored(&readings(0x1000, 0, up | done, hundred_full)), Decision::Keep);
}

#[test]
fn phy_lan8742_resolution_is_read_twice_and_stable() {
    let done = phy::SCSR_AUTODONE | SCSR_BASE;
    // The datasheet's four codes of the speed indication, bits 4 to 2.
    assert_eq!(phy::resolution(done | 0b001 << 2), Resolution { hundred: false, full: false });
    assert_eq!(phy::resolution(done | 0b101 << 2), Resolution { hundred: false, full: true });
    assert_eq!(phy::resolution(done | 0b010 << 2), Resolution { hundred: true, full: false });
    assert_eq!(phy::resolution(done | 0b110 << 2), Resolution { hundred: true, full: true });
    // A code the datasheet does not list is the floor, 10 Mbit half duplex.
    for code in [0b000, 0b011, 0b100, 0b111] {
        assert_eq!(phy::resolution(done | code << 2), phy::FLOOR, "the code {code:03b}");
    }
    // Two equal readings stand; two that differ in the speed indication are refused; the other bits may move.
    assert_eq!(phy::resolve(done | 0b110 << 2, done | 0b110 << 2), Some(Resolution { hundred: true, full: true }));
    assert_eq!(phy::resolve(done | 0b010 << 2, done | 0b110 << 2), None);
    assert_eq!(phy::resolve(SCSR_BASE | 0b110 << 2, done | 0b110 << 2), Some(Resolution { hundred: true, full: true }));
    // Read back through the management interface: register 31, twice.
    let mut wire = Scripted { phy: 0, script: &[(phy::REG_SCSR, 0x1058), (phy::REG_SCSR, 0x1058)], at: 0, written: None };
    assert_eq!(phy::read_resolution(&mut wire, 0), Ok(Some(Resolution { hundred: true, full: true })));
    let mut moving = Scripted { phy: 0, script: &[(phy::REG_SCSR, 0x1048), (phy::REG_SCSR, 0x1058)], at: 0, written: None };
    assert_eq!(phy::read_resolution(&mut moving, 0), Ok(None));
    // A wire that stops answering is a timeout, not a resolution.
    let mut dead = Scripted { phy: 0, script: &[(phy::REG_SCSR, 0x1058)], at: 0, written: None };
    assert_eq!(phy::read_resolution(&mut dead, 0), Err(MdioTimeout));
    // The control: a read-back that takes the second reading alone programs full duplex from a negotiation still moving.
    let doctored = |_first: u16, second: u16| Some(phy::resolution(second));
    assert_ne!(doctored(done | 0b010 << 2, done | 0b110 << 2), phy::resolve(done | 0b010 << 2, done | 0b110 << 2));
}

#[test]
fn phy_lan8742_identity_matches_the_datasheet() {
    // Registers 2 and 3 as the datasheet prints their defaults, and with every revision the low four bits can hold.
    assert!(phy::identity(0x0007, 0xC130));
    for revision in 0..16 {
        assert!(phy::identity(0x0007, 0xC130 | revision), "revision {revision}");
    }
    // What a dead wire reads (all ones, all zeros) and a neighbouring model number are not the part.
    assert!(!phy::identity(0xFFFF, 0xFFFF));
    assert!(!phy::identity(0x0000, 0x0000));
    assert!(!phy::identity(0x0007, 0xC0F1));
    assert!(!phy::identity(0x0007, 0xC120));
    assert!(!phy::identity(0x0006, 0xC130));
    // The identity is read first, at the address the board's strap gives, register 2 then register 3; nothing is written.
    let mut wire = Scripted { phy: 0, script: &[(phy::REG_ID1, 0x0007), (phy::REG_ID2, 0xC131)], at: 0, written: None };
    let (id1, id2) = phy::read_identity(&mut wire, 0).unwrap();
    assert!(phy::identity(id1, id2));
    assert_eq!(wire.written, None);
    // The two writes the decisions ask for are the control register's: the restart is not a reset.
    phy::restart(&mut wire, 0).unwrap();
    assert_eq!(wire.written, Some((phy::REG_BMCR, 0x1200)));
    phy::reset(&mut wire, 0).unwrap();
    assert_eq!(wire.written, Some((phy::REG_BMCR, 0x8000)));
    // The control: an identity that masks the model number away takes a neighbouring part for this one.
    let doctored = |id1: u16, id2: u16| id1 == 0x0007 && id2 & 0xFC00 == 0xC000;
    assert_ne!(doctored(0x0007, 0xC0F1), phy::identity(0x0007, 0xC0F1));
}

/// A declaration of a facts file read as text, from `pub const <name>:` to its first semicolon: the bytes the kernel compiles.
fn facts_decl<'a>(text: &'a str, name: &str) -> &'a str {
    let key = format!("pub const {name}: ");
    let start = text.find(&key).unwrap_or_else(|| panic!("no declaration of {name}"));
    let end = text[start..].find(';').unwrap_or_else(|| panic!("the declaration of {name} does not end")) + start;
    &text[start..end]
}

/// The number after a field's name in a row of such a declaration: decimal or hexadecimal, underscores allowed.
fn facts_field(row: &str, key: &str) -> u64 {
    let i = row.find(key).unwrap_or_else(|| panic!("no field {key} in the row {row}")) + key.len();
    let t: String = row[i..].trim_start().chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '_').collect();
    let t = t.replace('_', "");
    let n = match t.strip_prefix("0x") {
        Some(h) => u64::from_str_radix(h, 16),
        None => t.parse(),
    };
    n.unwrap_or_else(|_| panic!("the field {key} does not read as a number: {t}"))
}

/// The second I/O supply's registers, scripted: the status register shows the ready bit from one read to another
/// (counted from 1; 0 = never) beside a bit the sequence does not ask for, and every write is logged in order.
struct SupplyScript {
    facts: crate::stm32u5_supply_math::SupplyFacts,
    clock: u32,
    control: u32,
    status_reads: u32,
    ready_from: u32,
    ready_until: u32,
    log: Vec<(usize, u32)>,
}

impl crate::stm32u5_supply_math::Regs for SupplyScript {
    fn rd(&mut self, addr: usize) -> u32 {
        if addr == self.facts.clock_reg {
            self.clock
        } else if addr == self.facts.control_reg {
            self.control
        } else if addr == self.facts.status_reg {
            self.status_reads += 1;
            let ready = self.ready_from != 0 && self.status_reads >= self.ready_from && self.status_reads <= self.ready_until;
            (1 << 26) | if ready { 1 << self.facts.ready_bit } else { 0 }
        } else {
            panic!("the sequence read an address that is none of its three registers: {addr:#x}")
        }
    }
    fn wr(&mut self, addr: usize, value: u32) {
        self.log.push((addr, value));
        if addr == self.facts.clock_reg {
            self.clock = value;
        } else if addr == self.facts.control_reg {
            self.control = value;
        } else {
            panic!("the sequence wrote an address that is neither the clock enable nor the control register: {addr:#x}")
        }
    }
}

#[test]
fn stm32u5_supply_sequence_by_the_manual_and_its_refusals() {
    use crate::stm32u5_supply_math as m;
    // The row's facts as the kernel compiles them, against the reference manual's numbers (RM0456 Rev 7: RCC_AHB3ENR at
    // offset 0x094 of the RCC at 0x4602_0C00 with PWREN at bit 2, pages 561 and 562; PWR_SVMCR at offset 0x10 of the
    // power controller at 0x4602_0800 with IO2VMEN at bit 25 and IO2SV at bit 29, pages 451 and 452; PWR_SVMSR at
    // offset 0x3C with VDDIO2RDY at bit 25, page 462; the supply feeds PG2 to PG15, page 407).
    let row = facts_decl(include_str!("../../kernel/src/board/facts/st_nucleo_u5a5zj_q.rs"), "IO2_SUPPLY");
    let f = m::SupplyFacts {
        clock_reg: facts_field(row, "clock_reg:") as usize,
        clock_bit: facts_field(row, "clock_bit:") as u32,
        control_reg: facts_field(row, "control_reg:") as usize,
        monitor_bit: facts_field(row, "monitor_bit:") as u32,
        valid_bit: facts_field(row, "valid_bit:") as u32,
        status_reg: facts_field(row, "status_reg:") as usize,
        ready_bit: facts_field(row, "ready_bit:") as u32,
        first_line: facts_field(row, "first_line:") as u16,
        last_line: facts_field(row, "last_line:") as u16,
    };
    assert_eq!((f.clock_reg, f.clock_bit), (0x4602_0C00 + 0x094, 2), "the power controller's clock enable");
    assert_eq!((f.control_reg, f.monitor_bit, f.valid_bit), (0x4602_0800 + 0x10, 25, 29), "the monitoring control register, the monitor's enable and the valid bit");
    assert_eq!((f.status_reg, f.ready_bit), (0x4602_0800 + 0x3C, 25), "the monitoring status register and the ready bit");
    assert_eq!((f.first_line, f.last_line), (6 * 16 + 2, 6 * 16 + 15), "PG2 to PG15");
    assert!(m::facts_sound(&f));
    assert!(!m::feeds(&f, 6 * 16 + 1) && m::feeds(&f, 6 * 16 + 2) && m::feeds(&f, 6 * 16 + 15) && !m::feeds(&f, 7 * 16), "the supply feeds PG2 to PG15 alone");
    for doctored in [
        m::SupplyFacts { valid_bit: 32, ..f },
        m::SupplyFacts { ready_bit: 32, ..f },
        m::SupplyFacts { control_reg: f.control_reg + 1, ..f },
        m::SupplyFacts { status_reg: f.control_reg, ..f },
        m::SupplyFacts { monitor_bit: f.valid_bit, ..f },
        m::SupplyFacts { first_line: f.last_line + 1, ..f },
        m::SupplyFacts { clock_reg: 0, ..f },
    ] {
        assert!(!m::facts_sound(&doctored), "a doctored facts row passed as sound: {doctored:?}");
    }
    // The other bits of the two registers, which every write must keep: the analog supply's valid bit and its
    // monitor's enable in the control register, another block's enable in the clock register.
    let kept = (1 << 30) | (1 << 26);
    let script = |control: u32, ready_from: u32, ready_until: u32| SupplyScript { facts: f, clock: 1 << 9, control, status_reads: 0, ready_from, ready_until, log: Vec::new() };
    let monitor = 1u32 << f.monitor_bit;
    let valid = 1u32 << f.valid_bit;

    // The supply reads ready at the third read after a settle of four: the writes in the manual's order.
    let mut s = script(kept, 7, u32::MAX);
    assert_eq!(m::bring_up(&mut s, &f, 4, 10), m::Outcome { valid: true, polls: 3, control_at_entry: kept });
    assert_eq!(
        s.log,
        vec![(f.clock_reg, (1 << 9) | (1 << f.clock_bit)), (f.control_reg, kept | monitor), (f.control_reg, kept), (f.control_reg, kept | valid)],
        "the clock enabled; the monitor enabled; the monitor disabled; the valid bit set -- the other bits kept in each"
    );
    assert_eq!(s.status_reads, 7, "four reads discarded and three counted");

    // Ready at the first counted read, and at the last one the bound allows.
    let mut s = script(0, 5, u32::MAX);
    assert_eq!(m::bring_up(&mut s, &f, 4, 10), m::Outcome { valid: true, polls: 1, control_at_entry: 0 });
    let mut s = script(0, 14, u32::MAX);
    assert_eq!(m::bring_up(&mut s, &f, 4, 10), m::Outcome { valid: true, polls: 10, control_at_entry: 0 });
    assert_eq!(s.control, valid);

    // THE REFUSALS. A supply that never reads ready: the valid bit is never written, the monitor is left disabled,
    // the polls are the bound.
    let mut s = script(kept, 0, 0);
    assert_eq!(m::bring_up(&mut s, &f, 4, 10), m::Outcome { valid: false, polls: 10, control_at_entry: kept });
    assert!(s.log.iter().all(|&(a, v)| a != f.control_reg || v & valid == 0), "the valid bit was written without a ready verdict: {:x?}", s.log);
    assert_eq!(s.control, kept, "the monitor disabled, the other bits kept");
    // One read past the bound is not waited for.
    let mut s = script(0, 15, u32::MAX);
    assert_eq!(m::bring_up(&mut s, &f, 4, 10), m::Outcome { valid: false, polls: 10, control_at_entry: 0 });
    assert_eq!(s.control, 0);
    // A ready that shows inside the settle alone is not counted.
    let mut s = script(0, 1, 4);
    assert_eq!(m::bring_up(&mut s, &f, 4, 10), m::Outcome { valid: false, polls: 10, control_at_entry: 0 });
    assert_eq!(s.control, 0);
    // A valid bit found set (left by an earlier program) over a supply that does not read ready is cleared.
    let mut s = script(kept | valid, 0, 0);
    assert_eq!(m::bring_up(&mut s, &f, 4, 10), m::Outcome { valid: false, polls: 10, control_at_entry: kept | valid });
    assert_eq!(s.control, kept, "the valid bit cleared, the other bits kept");
    assert_eq!(s.log.last(), Some(&(f.control_reg, kept)));
    // A valid bit found set over a supply that reads ready stays set.
    let mut s = script(kept | valid, 5, u32::MAX);
    assert_eq!(m::bring_up(&mut s, &f, 4, 10), m::Outcome { valid: true, polls: 1, control_at_entry: kept | valid });
    assert_eq!(s.control, kept | valid);
    // No bound, no verdict.
    let mut s = script(0, 1, u32::MAX);
    assert_eq!(m::bring_up(&mut s, &f, 0, 0), m::Outcome { valid: false, polls: 0, control_at_entry: 0 });
    assert_eq!(s.control, 0);
}

#[test]
fn stm32_tim_math_clock_rule_and_channel_fields() {
    use crate::stm32_tim_math as m;
    // The clock rule at the two ST rows' bus shapes (RM0410 5.2, RM0456 11.4.15): the bus clock at a bus prescaler of 1,
    // twice the bus clock otherwise.
    assert_eq!(m::timer_clock_hz(54_000_000, 216_000_000), 108_000_000, "the F7's APB1 at a prescaler of 4");
    assert_eq!(m::timer_clock_hz(108_000_000, 216_000_000), 216_000_000, "the F7's APB2 at a prescaler of 2");
    assert_eq!(m::timer_clock_hz(16_000_000, 16_000_000), 16_000_000, "a prescaler of 1 at the internal oscillator's clock");
    assert_eq!(m::timer_clock_hz(160_000_000, 160_000_000), 160_000_000, "the U5 at a prescaler of 1");
    // The registers' offsets and bits (RM0410 Rev 5 pages 914 to 940; RM0456 Rev 7 pages 2183 to 2339).
    assert_eq!((m::CR1, m::EGR, m::CCMR1, m::CCMR2, m::CCER, m::CNT, m::PSC, m::ARR, m::CCR1, m::BDTR), (0x00, 0x14, 0x18, 0x1C, 0x20, 0x24, 0x28, 0x2C, 0x34, 0x44));
    assert_eq!((m::CR1_CEN, m::CR1_ARPE, m::EGR_UG, m::BDTR_MOE), (1 << 0, 1 << 7, 1 << 0, 1 << 15));
    assert_eq!((m::MODE_FORCED_INACTIVE, m::MODE_PWM1), (0b100, 0b110));
    assert_eq!((m::REACH_PRESCALER, m::REACH_PERIOD), (65536, 65535));
    // The fields of the four channels: the mode register, the select mask, the preload bit, the mode mask, PWM mode 1
    // and the forced inactive mode in the field, the mode's fourth bit, the enable bit, the polarity bit, the compare
    // register.
    let want: [(usize, u32, u32, u32, u32, u32, u32, u32, u32, usize); 4] = [
        (0x18, 0x0000_0003, 1 << 3, 0x0000_0070, 0x0000_0060, 0x0000_0040, 1 << 16, 1 << 0, 1 << 1, 0x34),
        (0x18, 0x0000_0300, 1 << 11, 0x0000_7000, 0x0000_6000, 0x0000_4000, 1 << 24, 1 << 4, 1 << 5, 0x38),
        (0x1C, 0x0000_0003, 1 << 3, 0x0000_0070, 0x0000_0060, 0x0000_0040, 1 << 16, 1 << 8, 1 << 9, 0x3C),
        (0x1C, 0x0000_0300, 1 << 11, 0x0000_7000, 0x0000_6000, 0x0000_4000, 1 << 24, 1 << 12, 1 << 13, 0x40),
    ];
    for (ch, w) in want.iter().enumerate() {
        let ch = ch as u32;
        let got = (m::ccmr(ch).0, m::select_mask(ch), m::preload_bit(ch), m::mode_mask(ch), m::mode_bits(ch, m::MODE_PWM1), m::mode_bits(ch, m::MODE_FORCED_INACTIVE), m::mode_high_bit(ch), m::enable_bit(ch), m::polarity_bit(ch), m::ccr(ch));
        assert_eq!(got, *w, "the fields of channel {}", ch + 1);
    }
    // The timer rows from block rows and a clock configuration's bus clocks, with the two reaches.
    let blocks = [
        m::TimBlock { number: 1, base: 0x4001_0000, enable_reg: 0x4002_3844, enable_bit: 0, apb: 2, advanced: true, channels: 4 },
        m::TimBlock { number: 12, base: 0x4000_1800, enable_reg: 0x4002_3840, enable_bit: 6, apb: 1, advanced: false, channels: 2 },
    ];
    let t: [pwm_math::PwmTimer; 2] = m::timers(&blocks, 54_000_000, 108_000_000, 216_000_000);
    assert_eq!((t[0].number, t[0].clock_hz, t[0].channels, t[0].prescaler_max, t[0].period_max), (1, 216_000_000, 4, 65536, 65535));
    assert_eq!((t[1].number, t[1].clock_hz, t[1].channels, t[1].prescaler_max, t[1].period_max), (12, 108_000_000, 2, 65536, 65535));
    // A duty equal to the period is a compare equal to the period's value -- one past the auto-reload register -- and at
    // the longest period the table allows that compare still fits sixteen bits.
    assert_eq!(pwm_math::compare(1_000_000, 216_000_000, 4, 54_000), 54_000);
    assert!(m::REACH_PERIOD <= u16::MAX as u32);
    assert!(pwm_math::table_sound(&[pwm_math::PwmLine { line: 11, timer: 0, channel: 3, alternate: 1 }, pwm_math::PwmLine { line: 31, timer: 1, channel: 1, alternate: 9 }], &t));
    // THE REFUSALS: a block table that is not sound, and a line its block cannot serve.
    assert!(m::blocks_sound(&blocks));
    let b0 = blocks[0];
    for doctored in [
        m::TimBlock { apb: 3, ..b0 },
        m::TimBlock { apb: 0, ..b0 },
        m::TimBlock { channels: 0, ..b0 },
        m::TimBlock { channels: 5, ..b0 },
        m::TimBlock { base: 0, ..b0 },
        m::TimBlock { base: b0.base + 2, ..b0 },
        m::TimBlock { enable_reg: 0, ..b0 },
        m::TimBlock { enable_bit: 32, ..b0 },
        m::TimBlock { number: 0, ..b0 },
    ] {
        assert!(!m::blocks_sound(&[doctored, blocks[1]]), "a doctored block passed as sound: {doctored:?}");
    }
    assert!(!m::blocks_sound(&[b0, m::TimBlock { number: 1, ..blocks[1] }]), "two blocks of one number");
    assert!(!m::blocks_sound(&[b0, m::TimBlock { base: b0.base, ..blocks[1] }]), "two blocks of one base");
    let fit = |timer: u8, channel: u8| m::lines_fit(&[pwm_math::PwmLine { line: 1, timer, channel, alternate: 1 }], &blocks);
    assert!(fit(0, 3) && fit(1, 1), "the last channel of each block");
    assert!(!fit(1, 2), "a third channel on a block of two");
    assert!(!fit(0, 4), "a fifth channel");
    assert!(!fit(2, 0), "a timer past the block table");
}

/// What an ST row's PWM tables must read, from the board's connector table, the chip's alternate-function table and the
/// reference manual: the file as the kernel compiles it; the block rows (the timer's number, its base, its clock enable's
/// register and bit, its bus, whether it is an advanced timer, its channels); the line rows (the line, its timer's
/// number, its channel as the manual counts it, its alternate function); each timer's clock at the row's two clock
/// configurations, the bus clocks those configurations state (APB1, APB2 and the clock they divide), and the arguments
/// the file hands the timer table's constructor.
struct TimRowWant {
    file: &'static str,
    text: &'static str,
    blocks: &'static [(u32, usize, usize, u32, u8, bool, u32)],
    lines: &'static [(u16, u32, u8, u8)],
    clocks: &'static [(u32, u32)],
    buses: [(u32, u32, u32); 2],
    timers_args: &'static str,
}

const TIM_ROWS: [TimRowWant; 2] = [
    TimRowWant {
        file: "st_stm32f769i_disco.rs",
        text: include_str!("../../kernel/src/board/facts/st_stm32f769i_disco.rs"),
        blocks: &[(1, 0x4001_0000, 0x4002_3844, 0, 2, true, 4), (3, 0x4000_0400, 0x4002_3840, 1, 1, false, 4), (10, 0x4001_4400, 0x4002_3844, 17, 2, false, 1), (11, 0x4001_4800, 0x4002_3844, 18, 2, false, 1), (12, 0x4000_1800, 0x4002_3840, 6, 1, false, 2)],
        lines: &[(86, 10, 1, 3), (40, 3, 3, 2), (87, 11, 1, 3), (118, 12, 1, 9), (11, 1, 4, 1), (31, 12, 2, 9)],
        clocks: &[(216000000, 16000000), (108000000, 16000000), (216000000, 16000000), (216000000, 16000000), (108000000, 16000000)],
        buses: [(54000000, 108000000, 216000000), (16000000, 16000000, 16000000)],
        timers_args: "(PWM_BLOCKS, crate::board::PCLK1_HZ, crate::board::PCLK2_HZ, crate::board::SYSCLK_HZ)",
    },
    TimRowWant {
        file: "st_nucleo_u5a5zj_q.rs",
        text: include_str!("../../kernel/src/board/facts/st_nucleo_u5a5zj_q.rs"),
        blocks: &[(1, 0x4001_2C00, 0x4602_0CA4, 11, 2, true, 4), (3, 0x4000_0400, 0x4602_0C9C, 1, 1, false, 4), (4, 0x4000_0800, 0x4602_0C9C, 2, 1, false, 4)],
        lines: &[(77, 1, 3, 1), (75, 1, 2, 1), (73, 1, 1, 1), (63, 4, 4, 2), (62, 4, 3, 2), (7, 3, 2, 2)],
        clocks: &[(160000000, 16000000), (160000000, 16000000), (160000000, 16000000)],
        buses: [(160000000, 160000000, 160000000), (16000000, 16000000, 16000000)],
        timers_args: "(PWM_BLOCKS, crate::board::SYSCLK_HZ, crate::board::SYSCLK_HZ, crate::board::SYSCLK_HZ)",
    },
];

/// A facts file's block rows and line rows, read from its text.
fn tim_rows(text: &str) -> (Vec<crate::stm32_tim_math::TimBlock>, Vec<pwm_math::PwmLine>) {
    let blocks = facts_decl(text, "PWM_BLOCKS")
        .split("TimBlock {")
        .skip(1)
        .map(|r| crate::stm32_tim_math::TimBlock {
            number: facts_field(r, "number:") as u32,
            base: facts_field(r, "base:") as usize,
            enable_reg: facts_field(r, "enable_reg:") as usize,
            enable_bit: facts_field(r, "enable_bit:") as u32,
            apb: facts_field(r, "apb:") as u8,
            advanced: r.contains("advanced: true"),
            channels: facts_field(r, "channels:") as u32,
        })
        .collect();
    let lines = facts_decl(text, "PWM_LINES")
        .split("PwmLine {")
        .skip(1)
        .map(|r| pwm_math::PwmLine { line: facts_field(r, "line:") as u16, timer: facts_field(r, "timer:") as u8, channel: facts_field(r, "channel:") as u8, alternate: facts_field(r, "alternate:") as u8 })
        .collect();
    (blocks, lines)
}

/// A facts file's text against what its row must read: the first disagreement in words, or none.
fn tim_row_fault(text: &str, w: &TimRowWant) -> Option<String> {
    use crate::stm32_tim_math as m;
    let (blocks, lines) = tim_rows(text);
    if blocks.len() != w.blocks.len() || lines.len() != w.lines.len() {
        return Some(format!("{} blocks and {} lines, {} and {} wanted", blocks.len(), lines.len(), w.blocks.len(), w.lines.len()));
    }
    for (i, (b, e)) in blocks.iter().zip(w.blocks).enumerate() {
        if (b.number, b.base, b.enable_reg, b.enable_bit, b.apb, b.advanced, b.channels) != *e {
            return Some(format!("block {i} reads {b:?}"));
        }
    }
    if !m::blocks_sound(&blocks) || !m::lines_fit(&lines, &blocks) {
        return Some("the block table is not sound, or a line names a channel its block does not have".to_string());
    }
    for (i, (l, e)) in lines.iter().zip(w.lines).enumerate() {
        if (l.line, blocks[l.timer as usize].number, l.channel + 1, l.alternate) != *e {
            return Some(format!("line row {i} reads line {} on timer {} channel {} at alternate function {}", l.line, blocks[l.timer as usize].number, l.channel + 1, l.alternate));
        }
    }
    for (k, (pclk1, pclk2, hclk)) in w.buses.iter().enumerate() {
        let timers: Vec<pwm_math::PwmTimer> = blocks
            .iter()
            .map(|b| pwm_math::PwmTimer { number: b.number, clock_hz: m::timer_clock_hz(if b.apb == 1 { *pclk1 } else { *pclk2 }, *hclk), channels: b.channels, prescaler_max: m::REACH_PRESCALER, period_max: m::REACH_PERIOD })
            .collect();
        for (i, t) in timers.iter().enumerate() {
            let want = if k == 0 { w.clocks[i].0 } else { w.clocks[i].1 };
            if t.clock_hz != want {
                return Some(format!("timer {} runs {} Hz at clock configuration {k}, {want} wanted", t.number, t.clock_hz));
            }
        }
        if !pwm_math::table_sound(&lines, &timers) {
            return Some("the line and timer tables are not sound".to_string());
        }
    }
    if !facts_decl(text, "PWM_TIMERS").ends_with(w.timers_args) {
        return Some(format!("the timer table is not built from {}", w.timers_args));
    }
    None
}

#[test]
fn stm32_tim_rows_tables_tied_to_the_connector_tables() {
    for w in &TIM_ROWS {
        assert_eq!(tim_row_fault(w.text, w), None, "{}", w.file);
        // THE REFUSALS: the same comparison over doctored copies of the file's text, each with one row changed.
        let (blocks, lines) = tim_rows(w.text);
        let l0 = &lines[0];
        let row = format!("PwmLine {{ line: {}, timer: {}, channel: {}, alternate: {} }}", l0.line, l0.timer, l0.channel, l0.alternate);
        let b0 = &blocks[0];
        let base = format!("base: 0x{:04X}_{:04X}", b0.base >> 16, b0.base & 0xFFFF);
        let moved = format!("base: 0x{:04X}_{:04X}", (b0.base + 0x400) >> 16, (b0.base + 0x400) & 0xFFFF);
        let doctored = [
            ("a line moved to its timer's next channel", w.text.replacen(&row, &format!("PwmLine {{ line: {}, timer: {}, channel: {}, alternate: {} }}", l0.line, l0.timer, l0.channel + 1, l0.alternate), 1)),
            ("a line's number changed", w.text.replacen(&row, &format!("PwmLine {{ line: {}, timer: {}, channel: {}, alternate: {} }}", l0.line + 1, l0.timer, l0.channel, l0.alternate), 1)),
            ("a line's alternate function changed", w.text.replacen(&row, &format!("PwmLine {{ line: {}, timer: {}, channel: {}, alternate: {} }}", l0.line, l0.timer, l0.channel, l0.alternate + 1), 1)),
            ("a block's base moved", w.text.replacen(&base, &moved, 1)),
            ("the timer table built from other bus clocks", w.text.replacen(w.timers_args, "(PWM_BLOCKS, crate::board::SYSCLK_HZ, crate::board::PCLK1_HZ, crate::board::SYSCLK_HZ)", 1)),
        ];
        for (what, text) in &doctored {
            assert_ne!(text.as_str(), w.text, "{}: the doctored copy is the file itself ({what})", w.file);
            assert!(tim_row_fault(text, w).is_some(), "{}: a doctored copy passed ({what})", w.file);
        }
    }
}

#[test]
fn stm32_adc_math_fields_times_and_bands() {
    use crate::stm32_adc_math::{self as m, f7, u5};
    // The STM32F7's converter as RM0410 Rev 5 states it: the registers' offsets (pages 468 to 479), the status and
    // control bits (pages 468, 471 and 472), the common control register's fields (page 480), the longest sampling
    // code 111 in every field (page 473: nine channels in the first sample time register, ten in the second), and a
    // conversion of the sampling time plus 12 cycles (page 450).
    assert_eq!((f7::SR, f7::CR1, f7::CR2, f7::SMPR1, f7::SMPR2, f7::SQR1, f7::SQR3, f7::DR, f7::CCR), (0x00, 0x04, 0x08, 0x0C, 0x10, 0x2C, 0x34, 0x4C, 0x04));
    assert_eq!((f7::SR_EOC, f7::SR_OVR, f7::CR2_ADON, f7::CR2_EOCS, f7::CR2_SWSTART), (1 << 1, 1 << 5, 1, 1 << 10, 1 << 30));
    assert_eq!((f7::CCR_ADCPRE, f7::CCR_ADCPRE_4, f7::CCR_TSVREFE), (0b11 << 16, 0b01 << 16, 1 << 23));
    assert_eq!(f7::SMPR1_ALL_480, (0..9).fold(0, |w, i| w | (0b111 << (3 * i))));
    assert_eq!(f7::SMPR2_ALL_480, (0..10).fold(0, |w, i| w | (0b111 << (3 * i))));
    assert_eq!((f7::SAMPLE_CYCLES, f7::CONVERSION_CYCLES, f7::BITS, f7::PRESCALER), (480, 492, 12, 4));
    // The STM32U5's 14-bit converter as RM0456 Rev 7 states it: the offsets (pages 1341 to 1359 and 1371), the status
    // bits (pages 1342 and 1343), the control register's commands and plain bits (pages 1345 to 1348), the longest
    // sampling code in every field, and a conversion of the sampling time plus 17 cycles at 14 bits (page 1285).
    assert_eq!((u5::ISR, u5::CR, u5::CFGR1, u5::SMPR1, u5::SMPR2, u5::PCSEL, u5::SQR1, u5::DR, u5::CCR), (0x00, 0x08, 0x0C, 0x14, 0x18, 0x1C, 0x30, 0x40, 0x08));
    assert_eq!((u5::ISR_ADRDY, u5::ISR_EOC, u5::ISR_EOS, u5::ISR_OVR, u5::ISR_LDORDY), (1, 1 << 2, 1 << 3, 1 << 4, 1 << 12));
    assert_eq!((u5::CR_ADEN, u5::CR_ADDIS, u5::CR_ADSTART, u5::CR_ADSTP, u5::CR_ADCALLIN, u5::CR_ADVREGEN, u5::CR_DEEPPWD, u5::CR_ADCAL), (1, 1 << 1, 1 << 2, 1 << 4, 1 << 16, 1 << 28, 1 << 29, 1 << 31));
    assert_eq!(u5::CR_COMMANDS, 0x8000_003F, "the seven bits of the control register that act on a 1 written");
    assert_eq!(u5::CFGR1_SINGLE_14, 0b11 | (0b11 << 2) | (0b11 << 10) | (1 << 12) | (1 << 13));
    assert_eq!((u5::CCR_DUAL, u5::CCR_PRESC, u5::CCR_VREFEN), (0x1F, 0xF << 18, 1 << 22));
    assert_eq!(u5::SMPR_ALL_814, (0..10).fold(0, |w, i| w | (0b111 << (3 * i))));
    assert_eq!((u5::SAMPLE_CYCLES, u5::CONVERSION_CYCLES, u5::BITS), (814, 831, 14));
    // The clocks the two rows give their converters (the STM32F769's APB2 clock over the prescaler at its two clock
    // configurations; the U5A5's 16 MHz kernel clock): inside the datasheets' ranges, the internal reference sampled
    // for at least its minimum; a conversion's time and the wait budget the seam computes from it at a 1 kHz tick.
    for (hz, ns) in [(108_000_000 / f7::PRESCALER, 18_223), (16_000_000 / f7::PRESCALER, 123_000)] {
        assert!(m::clock_ok(hz, f7::CLOCK_MIN_HZ, f7::CLOCK_MAX_HZ) && m::sampling_ok(f7::SAMPLE_CYCLES, hz, f7::REFERENCE_SAMPLE_MIN_NS), "{hz} Hz");
        assert_eq!((adc_math::conversion_ns(f7::CONVERSION_CYCLES, hz), adc_math::budget_ticks(ns, 1000)), (ns, 4), "{hz} Hz");
    }
    assert!(m::clock_ok(16_000_000, u5::CLOCK_MIN_HZ, u5::CLOCK_MAX_HZ) && m::sampling_ok(u5::SAMPLE_CYCLES, 16_000_000, u5::REFERENCE_SAMPLE_MIN_NS));
    assert_eq!((adc_math::conversion_ns(u5::CONVERSION_CYCLES, 16_000_000), adc_math::budget_ticks(51_938, 1000)), (51_938, 4));
    assert_eq!((adc_math::full_scale(f7::BITS), adc_math::full_scale(u5::BITS)), (4095, 16383));
    // THE REFUSALS: a clock one hertz outside either end of each range; a sampling time under the reference's minimum
    // (480 cycles last 10000 ns at 48 MHz and less one hertz above it; 814 cycles last 12650 ns at 64347826 Hz).
    assert!(!m::clock_ok(f7::CLOCK_MIN_HZ - 1, f7::CLOCK_MIN_HZ, f7::CLOCK_MAX_HZ) && !m::clock_ok(f7::CLOCK_MAX_HZ + 1, f7::CLOCK_MIN_HZ, f7::CLOCK_MAX_HZ));
    assert!(!m::clock_ok(u5::CLOCK_MIN_HZ - 1, u5::CLOCK_MIN_HZ, u5::CLOCK_MAX_HZ) && !m::clock_ok(u5::CLOCK_MAX_HZ + 1, u5::CLOCK_MIN_HZ, u5::CLOCK_MAX_HZ));
    assert!(m::sampling_ok(480, 48_000_000, f7::REFERENCE_SAMPLE_MIN_NS) && !m::sampling_ok(480, 48_000_001, f7::REFERENCE_SAMPLE_MIN_NS));
    assert!(m::sampling_ok(814, 64_347_826, u5::REFERENCE_SAMPLE_MIN_NS) && !m::sampling_ok(814, 64_347_827, u5::REFERENCE_SAMPLE_MIN_NS));
    assert!(!m::sampling_ok(480, 0, 1));
    // THE BANDS the two rows' flight checks judge by, each from a data sheet's limits at the nominal 3300 mV reference:
    // the internal reference, the wire held low and the wire held high, on each row.
    assert_eq!(m::band(1_180_000, 1_240_000, 3300, 12, 7), (1457, 1546));
    assert_eq!(m::band(0, 550_000, 3300, 12, 7), (0, 690));
    assert_eq!(m::band(2_211_000, 3_300_000, 3300, 12, 7), (2736, 4095));
    assert_eq!(m::band(1_175_000, 1_255_000, 3300, 14, 12), (5821, 6243));
    assert_eq!(m::band(0, 400_000, 3300, 14, 12), (0, 1998));
    assert_eq!(m::band(2_900_000, 3_300_000, 3300, 14, 12), (14385, 16383));
    // A band asked at another reference voltage, or at another width, is another band.
    assert_ne!(m::band(1_180_000, 1_240_000, 3000, 12, 7), (1457, 1546));
    assert_ne!(m::band(1_180_000, 1_240_000, 3300, 14, 7), (1457, 1546));
}

/// What a row's analog tables must read: its facts file, the block rows (the chip's number, the width, the reference
/// in millivolts, a conversion's cycles), the channel rows (the block's position, the channel, the line or none), the
/// declaration that carries the converter units with its rows (base, clock enable register and bit), and the flight
/// check's list. The rows are each board's connector pins as its manual tables them, with the chip's channel of each.
struct AdcRowWant {
    file: &'static str,
    text: &'static str,
    blocks: &'static [(u32, u32, u32, u32)],
    channels: &'static [(u8, u8, Option<u16>)],
    units_decl: &'static str,
    units: &'static [(usize, usize, u32)],
    flight: &'static [usize],
}

const ADC_ROWS: [AdcRowWant; 2] = [
    AdcRowWant {
        file: "st_stm32f769i_disco.rs",
        text: include_str!("../../kernel/src/board/facts/st_stm32f769i_disco.rs"),
        blocks: &[(1, 12, 3300, 492), (3, 12, 3300, 492)],
        channels: &[(0, 6, Some(6)), (0, 4, Some(4)), (0, 12, Some(34)), (1, 8, Some(90)), (1, 6, Some(88)), (1, 7, Some(89)), (0, 17, None)],
        units_decl: "ADC_UNITS",
        units: &[(0x4001_2000, 0x4002_3844, 8), (0x4001_2200, 0x4002_3844, 10)],
        flight: &[0, 1, 4, 5, 6],
    },
    AdcRowWant {
        file: "st_nucleo_u5a5zj_q.rs",
        text: include_str!("../../kernel/src/board/facts/st_nucleo_u5a5zj_q.rs"),
        blocks: &[(1, 14, 3300, 831)],
        channels: &[(0, 8, Some(3)), (0, 7, Some(2)), (0, 4, Some(35)), (0, 15, Some(16)), (0, 2, Some(33)), (0, 1, Some(32)), (0, 0, None)],
        units_decl: "ADC_FACTS",
        units: &[(0x4202_8000, 0x4602_0C8C, 10)],
        flight: &[0, 1, 2, 3, 4, 5, 6],
    },
];

/// A facts file's analog tables as its text states them: the block rows as the kernel's block type (the clock, an
/// expression the tables' readers do not need, written 1), the channel rows, the unit rows and the flight list.
fn adc_rows(text: &str, units_decl: &str) -> (Vec<adc_math::AdcBlock>, Vec<adc_math::AdcChannel>, Vec<crate::stm32_adc_math::AdcUnit>, Vec<usize>) {
    let blocks = facts_decl(text, "ADC_BLOCKS")
        .split("AdcBlock {")
        .skip(1)
        .map(|r| adc_math::AdcBlock { number: facts_field(r, "number:") as u32, clock_hz: 1, bits: facts_field(r, "bits:") as u32, reference_mv: facts_field(r, "reference_mv:") as u32, conversion_cycles: facts_field(r, "conversion_cycles:") as u32 })
        .collect();
    let channels = facts_decl(text, "ADC_CHANNELS")
        .split("AdcChannel {")
        .skip(1)
        .map(|r| adc_math::AdcChannel { block: facts_field(r, "block:") as u8, channel: facts_field(r, "channel:") as u8, line: if r.contains("line: crate::adc_math::NO_LINE") { adc_math::NO_LINE } else { facts_field(r, "line:") as u16 } })
        .collect();
    let units = facts_decl(text, units_decl)
        .split("AdcUnit {")
        .skip(1)
        .map(|r| crate::stm32_adc_math::AdcUnit { base: facts_field(r, "base:") as usize, enable_reg: facts_field(r, "enable_reg:") as usize, enable_bit: facts_field(r, "enable_bit:") as u32 })
        .collect();
    let list = facts_decl(text, "ADC_FLIGHT");
    let flight = list[list.rfind('[').expect("the flight list has no bracket") + 1..].trim_end_matches(']').split(',').map(|n| n.trim().parse().expect("a flight position is not a number")).collect();
    (blocks, channels, units, flight)
}

/// A facts file's text against what its row must read: the first disagreement in words, or none.
fn adc_row_fault(text: &str, w: &AdcRowWant) -> Option<String> {
    use crate::stm32_adc_math as m;
    let (blocks, channels, units, flight) = adc_rows(text, w.units_decl);
    let got_blocks: Vec<(u32, u32, u32, u32)> = blocks.iter().map(|b| (b.number, b.bits, b.reference_mv, b.conversion_cycles)).collect();
    if got_blocks != w.blocks {
        return Some(format!("the block rows read {got_blocks:?}"));
    }
    let got_channels: Vec<(u8, u8, Option<u16>)> = channels.iter().map(|c| (c.block, c.channel, if c.line == adc_math::NO_LINE { None } else { Some(c.line) })).collect();
    if got_channels != w.channels {
        return Some(format!("the channel rows read {got_channels:?}"));
    }
    let got_units: Vec<(usize, usize, u32)> = units.iter().map(|u| (u.base, u.enable_reg, u.enable_bit)).collect();
    if got_units != w.units {
        return Some(format!("the unit rows read {got_units:x?}"));
    }
    if flight != w.flight {
        return Some(format!("the flight list reads {flight:?}"));
    }
    if !adc_math::table_sound(&channels, &blocks) || !m::units_fit(&units, blocks.len(), &channels) || !m::positions_fit(&flight, channels.len()) {
        return Some("the tables are not sound, a block has no unit, or the flight list does not fit the table".to_string());
    }
    None
}

#[test]
fn stm32_adc_rows_tables_tied_to_the_connector_tables() {
    use crate::stm32_adc_math as m;
    for w in &ADC_ROWS {
        assert_eq!(adc_row_fault(w.text, w), None, "{}", w.file);
        // THE REFUSALS: the same comparison over doctored copies of the file's text, each with one row changed.
        let (blocks, channels, units, _) = adc_rows(w.text, w.units_decl);
        let (c0, c1) = (&channels[0], &channels[1]);
        let row0 = format!("AdcChannel {{ block: {}, channel: {}, line: {} }}", c0.block, c0.channel, c0.line);
        let row1 = format!("AdcChannel {{ block: {}, channel: {}, line: {} }}", c1.block, c1.channel, c1.line);
        let bits = format!("bits: {}, reference_mv", blocks[0].bits);
        let enable = format!("enable_bit: {} }}", units[0].enable_bit);
        let doctored = [
            ("a channel's line changed", w.text.replacen(&row0, &format!("AdcChannel {{ block: {}, channel: {}, line: {} }}", c0.block, c0.channel, c0.line + 1), 1)),
            ("a channel named twice", w.text.replacen(&row1, &format!("AdcChannel {{ block: {}, channel: {}, line: {} }}", c0.block, c0.channel, c1.line), 1)),
            ("a channel moved to a block the row does not have", w.text.replacen(&row0, &format!("AdcChannel {{ block: {}, channel: {}, line: {} }}", blocks.len(), c0.channel, c0.line), 1)),
            ("a block's width changed", w.text.replacen(&bits, &format!("bits: {}, reference_mv", blocks[0].bits + 2), 1)),
            ("a unit's clock enable bit changed", w.text.replacen(&enable, &format!("enable_bit: {} }}", units[0].enable_bit + 1), 1)),
            ("the flight list names a position past the table", w.text.replacen("pub const ADC_FLIGHT: &[usize] = &[0, 1, ", "pub const ADC_FLIGHT: &[usize] = &[0, 7, ", 1)),
        ];
        for (what, text) in &doctored {
            assert_ne!(text.as_str(), w.text, "{}: the doctored copy is the file itself ({what})", w.file);
            assert!(adc_row_fault(text, w).is_some(), "{}: a doctored copy passed ({what})", w.file);
        }
        // The soundness rules by themselves, on the parsed tables with one row changed.
        let mut twice = channels.clone();
        twice[1].channel = twice[0].channel;
        twice[1].block = twice[0].block;
        assert!(!adc_math::table_sound(&twice, &blocks), "{}: a channel named twice passed as sound", w.file);
        let mut stray = channels.clone();
        stray[0].block = units.len() as u8;
        assert!(!m::units_fit(&units, blocks.len(), &stray), "{}: a channel on a block without a unit passed", w.file);
        assert!(!m::units_fit(&units[1..], blocks.len(), &channels), "{}: a block without a unit passed", w.file);
        assert!(!m::positions_fit(&[0, channels.len()], channels.len()) && !m::positions_fit(&[1, 1], channels.len()), "{}: a flight list past the table, or with a position twice, passed", w.file);
    }
    // The STM32F769's flight list leaves out the positions of A2 and A3 (the pins a seated shield holds at 5 V).
    assert!(!ADC_ROWS[0].flight.contains(&2) && !ADC_ROWS[0].flight.contains(&3));
}

/// An STM32F7 converter block and its clock and common registers, scripted: every write is logged in order; a flag
/// of the status register clears on a 0 written and keeps on a 1; a start makes the end flag appear at a scripted
/// read of the status register, with the scripted sample in the data register and, when asked, the overrun flag;
/// a read of the data register clears the end flag.
struct F7Script {
    base: usize,
    mem: std::collections::BTreeMap<usize, u32>,
    log: Vec<(usize, u32)>,
    eoc_at: u32,
    status_reads: u32,
    converting: bool,
    sample: u32,
    overrun: bool,
}

impl F7Script {
    fn new(base: usize) -> Self {
        F7Script { base, mem: Default::default(), log: Vec::new(), eoc_at: 3, status_reads: 0, converting: false, sample: 0, overrun: false }
    }
    fn at(&self, addr: usize) -> u32 {
        *self.mem.get(&addr).unwrap_or(&0)
    }
}

impl crate::stm32_adc_math::Regs for F7Script {
    fn rd(&mut self, addr: usize) -> u32 {
        use crate::stm32_adc_math::f7;
        if addr == self.base + f7::SR && self.converting {
            self.status_reads += 1;
            if self.eoc_at != 0 && self.status_reads >= self.eoc_at {
                self.converting = false;
                let sr = self.at(addr) | f7::SR_EOC | if self.overrun { f7::SR_OVR } else { 0 };
                self.mem.insert(addr, sr);
                self.mem.insert(self.base + f7::DR, self.sample);
            }
        }
        if addr == self.base + f7::DR {
            let sr = self.at(self.base + f7::SR) & !f7::SR_EOC;
            self.mem.insert(self.base + f7::SR, sr);
        }
        self.at(addr)
    }
    fn wr(&mut self, addr: usize, value: u32) {
        use crate::stm32_adc_math::f7;
        self.log.push((addr, value));
        if addr == self.base + f7::SR {
            let kept = self.at(addr) & value;
            self.mem.insert(addr, kept);
        } else if addr == self.base + f7::CR2 && value & f7::CR2_SWSTART != 0 {
            self.converting = true;
            self.status_reads = 0;
            self.mem.insert(addr, value & !f7::CR2_SWSTART);
        } else {
            self.mem.insert(addr, value);
        }
    }
}

#[test]
fn stm32f7_adc_sequences_over_a_scripted_block() {
    use crate::stm32_adc_math::{f7, AdcUnit};
    let (enable_reg, common) = (0x4002_3844, 0x4001_2300);
    let u1 = AdcUnit { base: 0x4001_2000, enable_reg, enable_bit: 8 };
    let u3 = AdcUnit { base: 0x4001_2200, enable_reg, enable_bit: 10 };
    let idle = f7::CR2_ADON | f7::CR2_EOCS;
    // THE BRING-UP, in its order, the other bits of the shared registers kept: another block's clock enable (bit 14),
    // and in the common control register the battery channel's enable (bit 22) beside a prescaler found at over 8.
    let mut s = F7Script::new(u1.base);
    s.mem.insert(enable_reg, 1 << 14);
    s.mem.insert(common + f7::CCR, (1 << 22) | (0b11 << 16));
    f7::bring_up(&mut s, &u1, common, true);
    assert_eq!(
        s.log,
        vec![
            (enable_reg, (1 << 14) | (1 << 8)),
            (common + f7::CCR, (1 << 22) | f7::CCR_ADCPRE_4 | f7::CCR_TSVREFE),
            (u1.base + f7::CR1, 0),
            (u1.base + f7::SMPR1, f7::SMPR1_ALL_480),
            (u1.base + f7::SMPR2, f7::SMPR2_ALL_480),
            (u1.base + f7::SQR1, 0),
            (u1.base + f7::CR2, idle),
        ],
        "the clock; the common prescaler and the reference's enable; 12 bits; the sampling times; a sequence of one; the converter on with the end flag after each conversion"
    );
    assert!(f7::is_on(&mut s, u1.base));
    // The second block's bring-up enables its own clock and does not write the common control register again.
    s.log.clear();
    f7::bring_up(&mut s, &u3, common, true);
    assert_eq!(s.log[0], (enable_reg, (1 << 14) | (1 << 8) | (1 << 10)));
    assert!(s.log.iter().all(|(a, _)| *a != common + f7::CCR), "the common control register written under another block: {:x?}", s.log);
    assert_eq!(s.log.last(), Some(&(u3.base + f7::CR2, idle)));
    // A row that does not read the internal reference leaves its enable clear.
    let mut plain = F7Script::new(u1.base);
    f7::bring_up(&mut plain, &u1, common, false);
    assert_eq!(plain.at(common + f7::CCR), f7::CCR_ADCPRE_4);

    // A CONVERSION: the channel the sequence's first, every flag cleared, the start; no end before the scripted
    // read; the sample masked to 12 bits; the end flag gone once the data register is read.
    s.base = u1.base;
    s.log.clear();
    s.sample = 0xF123;
    f7::kick(&mut s, u1.base, 17);
    assert_eq!(s.log, vec![(u1.base + f7::SQR3, 17), (u1.base + f7::SR, 0), (u1.base + f7::CR2, idle | f7::CR2_SWSTART)]);
    assert!(!f7::done(&mut s, u1.base) && !f7::done(&mut s, u1.base), "an end before the conversion's");
    assert!(f7::done(&mut s, u1.base));
    assert_eq!(f7::take(&mut s, u1.base), Some(0x123));
    assert!(!f7::done(&mut s, u1.base), "the end flag outlived the data register's read");

    // THE REFUSALS. An overrun flagged conversion answers no sample and clears the overrun flag alone; the next
    // start, the manual's recovery, converts.
    s.overrun = true;
    f7::kick(&mut s, u1.base, 6);
    while !f7::done(&mut s, u1.base) {}
    s.log.clear();
    assert_eq!(f7::take(&mut s, u1.base), None, "a flagged conversion answered a sample");
    assert_eq!(s.log, vec![(u1.base + f7::SR, !f7::SR_OVR)]);
    assert_eq!(s.at(u1.base + f7::SR) & f7::SR_OVR, 0);
    s.overrun = false;
    s.sample = 0x0ABC;
    f7::kick(&mut s, u1.base, 6);
    while !f7::done(&mut s, u1.base) {}
    assert_eq!(f7::take(&mut s, u1.base), Some(0xABC));
    // A start on a block that is not on writes nothing.
    let mut off = F7Script::new(u1.base);
    f7::kick(&mut off, u1.base, 6);
    assert!(off.log.is_empty(), "a start wrote to a block that is not on: {:x?}", off.log);
    // The reset's two halves: off with the flags cleared, on again.
    s.log.clear();
    f7::off(&mut s, u1.base);
    assert!(!f7::is_on(&mut s, u1.base));
    f7::on(&mut s, u1.base);
    assert_eq!(s.log, vec![(u1.base + f7::CR2, 0), (u1.base + f7::SR, 0), (u1.base + f7::CR2, idle)]);
}

/// The STM32U5's 14-bit converter with its clock and common registers, scripted: every write is logged in order.
/// The status register's flags clear on a 1 written; the regulator's ready flag reads set from a scripted read of
/// the status register after the regulator is enabled (0 = never); the calibration command reads set until a
/// scripted read of the control register (0 = for ever); the enable takes, and the ready flag sets, at a scripted
/// enable write (0 = never); a start makes the end flag appear at a scripted status read with the scripted sample
/// and, when asked, the overrun flag; a stop ends a conversion; a disable clears the enable.
struct U5Script {
    base: usize,
    mem: std::collections::BTreeMap<usize, u32>,
    log: Vec<(usize, u32)>,
    ldo_at: u32,
    ldo_reads: u32,
    cal_until: u32,
    cal_reads: u32,
    ready_at: u32,
    enables: u32,
    eoc_at: u32,
    status_reads: u32,
    converting: bool,
    sample: u32,
    overrun: bool,
}

impl U5Script {
    fn new(base: usize, ldo_at: u32, cal_until: u32, ready_at: u32) -> Self {
        let mut s = U5Script { base, mem: Default::default(), log: Vec::new(), ldo_at, ldo_reads: 0, cal_until, cal_reads: 0, ready_at, enables: 0, eoc_at: 2, status_reads: 0, converting: false, sample: 0, overrun: false };
        s.mem.insert(base + crate::stm32_adc_math::u5::CR, crate::stm32_adc_math::u5::CR_DEEPPWD);
        s
    }
    fn at(&self, addr: usize) -> u32 {
        *self.mem.get(&addr).unwrap_or(&0)
    }
}

impl crate::stm32_adc_math::Regs for U5Script {
    fn rd(&mut self, addr: usize) -> u32 {
        use crate::stm32_adc_math::u5;
        let cr = self.at(self.base + u5::CR);
        if addr == self.base + u5::ISR {
            let mut v = self.at(addr);
            if cr & u5::CR_ADVREGEN != 0 {
                self.ldo_reads += 1;
                if self.ldo_at != 0 && self.ldo_reads >= self.ldo_at {
                    v |= u5::ISR_LDORDY;
                }
            }
            if self.converting {
                self.status_reads += 1;
                if self.eoc_at != 0 && self.status_reads >= self.eoc_at {
                    self.converting = false;
                    let flags = self.at(addr) | u5::ISR_EOC | u5::ISR_EOS | if self.overrun { u5::ISR_OVR } else { 0 };
                    self.mem.insert(addr, flags);
                    self.mem.insert(self.base + u5::DR, self.sample);
                    self.mem.insert(self.base + u5::CR, cr & !u5::CR_ADSTART);
                    v |= flags;
                }
            }
            return v;
        }
        if addr == self.base + u5::CR && cr & u5::CR_ADCAL != 0 {
            self.cal_reads += 1;
            if self.cal_until != 0 && self.cal_reads >= self.cal_until {
                self.mem.insert(addr, cr & !u5::CR_ADCAL);
            }
        }
        if addr == self.base + u5::DR {
            let isr = self.at(self.base + u5::ISR) & !u5::ISR_EOC;
            self.mem.insert(self.base + u5::ISR, isr);
        }
        self.at(addr)
    }
    fn wr(&mut self, addr: usize, value: u32) {
        use crate::stm32_adc_math::u5;
        self.log.push((addr, value));
        if addr == self.base + u5::ISR {
            let kept = self.at(addr) & !value;
            self.mem.insert(addr, kept);
        } else if addr == self.base + u5::CR {
            let old = self.at(addr);
            let mut v = (value & !u5::CR_COMMANDS) | (old & (u5::CR_ADEN | u5::CR_ADCAL | u5::CR_ADSTART));
            if value & u5::CR_ADVREGEN != 0 && old & u5::CR_ADVREGEN == 0 {
                self.ldo_reads = 0;
            }
            if value & u5::CR_ADCAL != 0 {
                v |= u5::CR_ADCAL;
                self.cal_reads = 0;
            }
            if value & u5::CR_ADEN != 0 {
                self.enables += 1;
                if self.ready_at != 0 && self.enables >= self.ready_at && v & u5::CR_ADCAL == 0 {
                    v |= u5::CR_ADEN;
                    let isr = self.at(self.base + u5::ISR) | u5::ISR_ADRDY;
                    self.mem.insert(self.base + u5::ISR, isr);
                }
            }
            if value & u5::CR_ADSTART != 0 {
                self.converting = true;
                self.status_reads = 0;
                v |= u5::CR_ADSTART;
            }
            if value & u5::CR_ADSTP != 0 {
                self.converting = false;
                v &= !u5::CR_ADSTART;
            }
            if value & u5::CR_ADDIS != 0 {
                v &= !u5::CR_ADEN;
            }
            self.mem.insert(addr, v);
        } else {
            self.mem.insert(addr, value);
        }
    }
}

#[test]
fn stm32u5_adc_power_up_by_the_manual_and_its_refusals() {
    use crate::stm32_adc_math::{u5, AdcUnit};
    use crate::stm32u5_supply_math as supply;
    // THE ANALOG SUPPLY. The row's facts as the kernel compiles them, against the reference manual's numbers (RM0456
    // Rev 7: PWR_SVMCR at offset 0x10 of the power controller at 0x4602_0800 with AVM1EN at bit 26 and ASV at bit
    // 30, pages 451 and 452; PWR_SVMSR at offset 0x3C with VDDA1RDY at bit 26, page 462), and the manual's sequence
    // for it (page 414) by the second I/O supply's own function: the monitor enabled, the verdict awaited, the
    // monitor disabled, the valid bit set -- the second I/O supply's valid bit (bit 29) and its monitor's enable
    // (bit 25) kept in every write.
    let text = include_str!("../../kernel/src/board/facts/st_nucleo_u5a5zj_q.rs");
    let row = facts_decl(text, "ANALOG_SUPPLY");
    let a = supply::SupplyFacts {
        clock_reg: facts_field(row, "clock_reg:") as usize,
        clock_bit: facts_field(row, "clock_bit:") as u32,
        control_reg: facts_field(row, "control_reg:") as usize,
        monitor_bit: facts_field(row, "monitor_bit:") as u32,
        valid_bit: facts_field(row, "valid_bit:") as u32,
        status_reg: facts_field(row, "status_reg:") as usize,
        ready_bit: facts_field(row, "ready_bit:") as u32,
        first_line: facts_field(row, "first_line:") as u16,
        last_line: facts_field(row, "last_line:") as u16,
    };
    assert_eq!((a.clock_reg, a.clock_bit), (0x4602_0C00 + 0x094, 2), "the power controller's clock enable");
    assert_eq!((a.control_reg, a.monitor_bit, a.valid_bit), (0x4602_0800 + 0x10, 26, 30), "the monitoring control register, the analog monitor's enable and the analog supply's valid bit");
    assert_eq!((a.status_reg, a.ready_bit), (0x4602_0800 + 0x3C, 26), "the monitoring status register and the analog supply's ready bit");
    assert!(supply::facts_sound(&a) && !supply::feeds(&a, 0) && !supply::feeds(&a, 3), "the analog supply feeds no line");
    // The supply's three registers, scripted: the status register shows the analog supply's ready bit from a scripted
    // read on (counted from 1; 0 = never) beside the second I/O supply's ready bit, which this sequence does not ask
    // for; every write is logged in order.
    struct Analog {
        f: supply::SupplyFacts,
        clock: u32,
        control: u32,
        status_reads: u32,
        ready_from: u32,
        log: Vec<(usize, u32)>,
    }
    impl supply::Regs for Analog {
        fn rd(&mut self, addr: usize) -> u32 {
            if addr == self.f.clock_reg {
                self.clock
            } else if addr == self.f.control_reg {
                self.control
            } else {
                assert_eq!(addr, self.f.status_reg, "the sequence read an address that is none of its three registers");
                self.status_reads += 1;
                (1 << 25) | if self.ready_from != 0 && self.status_reads >= self.ready_from { 1 << self.f.ready_bit } else { 0 }
            }
        }
        fn wr(&mut self, addr: usize, value: u32) {
            self.log.push((addr, value));
            if addr == self.f.clock_reg {
                self.clock = value;
            } else {
                assert_eq!(addr, self.f.control_reg, "the sequence wrote an address that is neither the clock enable nor the control register");
                self.control = value;
            }
        }
    }
    let kept = (1 << 29) | (1 << 25);
    let mut s = Analog { f: a, clock: 1 << 9, control: kept, status_reads: 0, ready_from: 7, log: Vec::new() };
    assert_eq!(supply::bring_up(&mut s, &a, 4, 10), supply::Outcome { valid: true, polls: 3, control_at_entry: kept });
    assert_eq!(s.log, vec![(a.clock_reg, (1 << 9) | (1 << 2)), (a.control_reg, kept | (1 << 26)), (a.control_reg, kept), (a.control_reg, kept | (1 << 30))]);
    let mut s = Analog { f: a, clock: 0, control: kept, status_reads: 0, ready_from: 0, log: Vec::new() };
    assert_eq!(supply::bring_up(&mut s, &a, 4, 10), supply::Outcome { valid: false, polls: 10, control_at_entry: kept });
    assert_eq!(s.control, kept, "a supply that never read ready was declared valid");

    // THE CONVERTER. The row's facts as the kernel compiles them (RM0456 Rev 7: the converters' window at 0x4202_8000,
    // page 146; RCC_AHB2ENR1 at offset 0x08C with ADC12EN at bit 10, pages 556 and 558; RCC_CCIPR3 at offset 0x0E8
    // with ADCDACSEL at bits 14:12, 100 the 16 MHz internal oscillator, pages 595 and 596).
    let row = facts_decl(text, "ADC_FACTS");
    let f = u5::Facts {
        unit: AdcUnit { base: facts_field(row, "base:") as usize, enable_reg: facts_field(row, "enable_reg:") as usize, enable_bit: facts_field(row, "enable_bit:") as u32 },
        common: facts_field(row, "common:") as usize,
        clock_sel_reg: facts_field(row, "clock_sel_reg:") as usize,
        clock_sel_shift: facts_field(row, "clock_sel_shift:") as u32,
        clock_sel_mask: 0b111,
        clock_sel_code: 0b100,
    };
    assert!(row.contains("clock_sel_mask: 0b111,") && row.contains("clock_sel_code: 0b100,"), "the kernel clock selector's field and code");
    assert_eq!((f.unit.base, f.unit.enable_reg, f.unit.enable_bit, f.common), (0x4202_8000, 0x4602_0C00 + 0x08C, 10, 0x4202_8000 + 0x300));
    assert_eq!((f.clock_sel_reg, f.clock_sel_shift), (0x4602_0C00 + 0x0E8, 12));
    assert!(u5::facts_sound(&f) && !u5::facts_sound(&u5::Facts { clock_sel_code: 0b1000, ..f }) && !u5::facts_sound(&u5::Facts { common: f.common + 2, ..f }));
    let base = f.unit.base;
    let preselect = 0b1000_0001_1001_0111; // the row's seven channels: 0, 1, 2, 4, 7, 8 and 15
    let plain = u5::CR_ADVREGEN | u5::CR_ADCALLIN;
    let fresh = |ldo_at: u32, cal_until: u32, ready_at: u32| {
        let mut s = U5Script::new(base, ldo_at, cal_until, ready_at);
        s.mem.insert(f.unit.enable_reg, 1); // a port's clock enable, kept
        s.mem.insert(f.clock_sel_reg, (0b101 << 12) | 0x7); // another source selected, and a neighbour's field, kept
        s.mem.insert(f.common + u5::CCR, (1 << 24) | (0x3 << 18)); // the battery channel's enable, kept; a prescaler found set
        s.mem.insert(base + u5::CFGR1, 0x8000_0000 | (1 << 13) | (0b01 << 2)); // the reset value's bit 31, kept; continuous and 12 bits found set
        s
    };

    // THE CLOCKS, THEN THE POWER-UP, in the manual's order: the regulator ready at the third read, the calibration
    // over at the fifth, the enable taking at the second write.
    let mut s = fresh(3, 5, 2);
    u5::clocks(&mut s, &f);
    assert_eq!(s.log, vec![(f.unit.enable_reg, 1 | (1 << 10)), (f.clock_sel_reg, (0b100 << 12) | 0x7)]);
    s.log.clear();
    assert_eq!(u5::power_up(&mut s, &f, true, preselect, 10), u5::Outcome { stage: u5::STAGE_UP, regulator_polls: 3, calibration_polls: 5, enables: 2 });
    assert_eq!(
        s.log,
        vec![
            (base + u5::CR, 0),
            (base + u5::CR, u5::CR_ADVREGEN),
            (f.common + u5::CCR, (1 << 24) | u5::CCR_VREFEN),
            (base + u5::CR, plain),
            (base + u5::CR, plain | u5::CR_ADCAL),
            (base + u5::ISR, u5::ISR_ADRDY),
            (base + u5::CR, plain | u5::CR_ADEN),
            (base + u5::CR, plain | u5::CR_ADEN),
            (base + u5::CFGR1, 0x8000_0000),
            (base + u5::SMPR1, u5::SMPR_ALL_814),
            (base + u5::SMPR2, u5::SMPR_ALL_814),
            (base + u5::PCSEL, preselect),
        ],
        "deep power-down left, then the regulator; the common register and the calibration while disabled; the ready flag cleared and the enable until it answers; the configuration and the preselection after it"
    );
    assert!(u5::is_up(&mut s, base));

    // A CONVERSION: the channel the sequence's one conversion, the flags cleared, the start; the sample masked to 14
    // bits; then an overrun flagged conversion, which answers no sample and clears its flag.
    s.log.clear();
    s.sample = 0xFFFF_7ABC;
    u5::kick(&mut s, base, 8);
    assert_eq!(s.log, vec![(base + u5::SQR1, 8 << 6), (base + u5::ISR, u5::ISR_EOC | u5::ISR_EOS | u5::ISR_OVR), (base + u5::CR, plain | u5::CR_ADSTART)]);
    assert!(!u5::done(&mut s, base), "an end before the conversion's");
    assert!(u5::done(&mut s, base));
    assert_eq!(u5::take(&mut s, base), Some(0x3ABC));
    assert!(!u5::done(&mut s, base), "the end flag outlived the data register's read");
    s.overrun = true;
    u5::kick(&mut s, base, 0);
    while !u5::done(&mut s, base) {}
    s.log.clear();
    assert_eq!(u5::take(&mut s, base), None, "a flagged conversion answered a sample");
    assert_eq!(s.log, vec![(base + u5::ISR, u5::ISR_OVR)]);
    assert_eq!(s.at(base + u5::ISR) & u5::ISR_OVR, 0);

    // THE POWER-DOWN: a running conversion stopped, the converter disabled; deep, the regulator off and deep
    // power-down entered.
    s.overrun = false;
    s.eoc_at = 0;
    u5::kick(&mut s, base, 1);
    s.log.clear();
    assert!(u5::power_down(&mut s, &f, true, 10));
    assert_eq!(s.log, vec![(base + u5::CR, plain | u5::CR_ADSTP), (base + u5::CR, plain | u5::CR_ADDIS), (base + u5::CR, u5::CR_ADCALLIN), (base + u5::CR, u5::CR_ADCALLIN | u5::CR_DEEPPWD)]);
    assert!(!u5::is_up(&mut s, base));

    // THE REFUSALS, each naming its stage, with nothing written after it. The regulator never ready.
    let mut s = fresh(0, 5, 2);
    assert_eq!(u5::power_up(&mut s, &f, true, preselect, 10), u5::Outcome { stage: u5::STAGE_REGULATOR, regulator_polls: 10, calibration_polls: 0, enables: 0 });
    assert_eq!(s.log, vec![(base + u5::CR, 0), (base + u5::CR, u5::CR_ADVREGEN)]);
    // A calibration that never ends: no enable is written.
    let mut s = fresh(1, 0, 2);
    assert_eq!(u5::power_up(&mut s, &f, true, preselect, 10), u5::Outcome { stage: u5::STAGE_CALIBRATION, regulator_polls: 1, calibration_polls: 10, enables: 0 });
    assert_eq!(s.log.last(), Some(&(base + u5::CR, plain | u5::CR_ADCAL)));
    // An enable that never answers: no configuration register and no preselection is written.
    let mut s = fresh(1, 1, 0);
    assert_eq!(u5::power_up(&mut s, &f, false, preselect, 10), u5::Outcome { stage: u5::STAGE_ENABLE, regulator_polls: 1, calibration_polls: 1, enables: 10 });
    assert!(s.log.iter().all(|(a, _)| ![base + u5::CFGR1, base + u5::SMPR1, base + u5::SMPR2, base + u5::PCSEL].contains(a)), "a configuration register written to a converter that is not enabled: {:x?}", s.log);
    assert_eq!(s.at(f.common + u5::CCR), 1 << 24, "a row that does not read the internal reference leaves its buffer off");
    // A start on a converter that is not up writes nothing.
    s.log.clear();
    u5::kick(&mut s, base, 8);
    assert!(s.log.is_empty(), "a start wrote to a converter that is not up: {:x?}", s.log);
}

#[test]
fn stm32_adc_turns_one_conversion_a_block() {
    use crate::stm32_adc_math::Turns;
    // Two blocks and seven channels: the positions 0, 1, 2 and 6 on block 0, the positions 3, 4 and 5 on block 1.
    let t: Turns<2, 7> = Turns::new();
    let of_block_0 = |i: usize| i < 3 || i == 6;
    // A free block is taken; a second channel of the same block queues and is not promoted while the block is held.
    assert!(t.start(0, 0) && t.owns(0, 0));
    assert!(!t.start(0, 1) && !t.owns(0, 1) && !t.promote(0, 1));
    // The other block is independent.
    assert!(t.start(1, 3) && t.owns(1, 3) && t.owns(0, 0));
    // The block given back: the queued channel takes it, once.
    t.give(0, 0);
    assert!(!t.owns(0, 0) && t.promote(0, 1) && t.owns(0, 1) && !t.promote(0, 1));
    // THE REFUSALS. A channel that is neither owner nor queued never starts by a promotion, even on a free block.
    t.give(0, 1);
    assert!(!t.promote(0, 2) && !t.owns(0, 2));
    // A give by a channel that does not own the block changes nothing.
    assert!(t.start(0, 2));
    t.give(0, 6);
    t.give(0, 0);
    assert!(t.owns(0, 2));
    // A reset clears its block's turn and the queue of its channels, and no other block's.
    assert!(!t.start(0, 0) && !t.start(1, 4));
    t.clear(0, of_block_0);
    assert!(!t.owns(0, 2) && !t.promote(0, 0), "a reset left its block held, or a channel queued");
    assert!(t.owns(1, 3), "a reset cleared another block's turn");
    t.give(1, 3);
    assert!(t.promote(1, 4) && t.owns(1, 4), "a reset dropped another block's queued channel");
    // A released channel leaves the queue and gives its block back.
    assert!(!t.start(1, 5));
    t.forget(1, 5);
    t.forget(1, 4);
    assert!(!t.owns(1, 4) && !t.promote(1, 5));
    assert!(t.start(1, 5) && t.start(0, 6));
}

#[test]
fn stm32_usart_math_divisor_laws_and_frames() {
    use crate::stm32_usart_math as m;
    use m::Law::{LowPower, Usart16};
    // THE USART's LAW at 16 MHz, oversampling by 16 (RM0410 pages 1248 and 1287): the divisor to nearest, 16 to 65535.
    assert_eq!(m::CLOCK_HZ, 16_000_000);
    assert_eq!(m::LOW_POWER_FIFO, 8, "the low-power block's FIFOs are 8 deep (RM0456 Table 686 on page 2847)");
    assert_eq!(m::divisor(Usart16, 9_600), Some((1667, 9_598)));
    assert_eq!(m::divisor(Usart16, 57_600), Some((278, 57_553)));
    assert_eq!(m::divisor(Usart16, 115_200), Some((139, 115_107)));
    assert_eq!(m::divisor(Usart16, 460_800), Some((35, 457_142)));
    assert_eq!(m::divisor(Usart16, 1_000_000), Some((16, 1_000_000)));
    assert_eq!(m::divisor(Usart16, 921_600), None, "the nearest divisor, 17, lands 2.1 percent high");
    assert_eq!(m::divisor(Usart16, 245), Some((65_306, 245)), "the floor");
    assert_eq!(m::divisor(Usart16, 244), None, "one step under the floor: the divisor would pass 65535");
    assert_eq!(m::divisor(Usart16, 1_020_408), Some((16, 1_000_000)), "the ceiling: the shortest divisor, 2 percent low");
    assert_eq!(m::divisor(Usart16, 1_020_409), None, "one step over the ceiling");
    assert_eq!(m::divisor(Usart16, 0), None);
    assert_eq!(m::divisor_usart16(16_000_000, 2_000_000), None, "a divisor under 16");
    // THE LOW-POWER BLOCK's LAW (RM0456 pages 2861, 2887 and 2898): 256 times the clock after the prescaler over the
    // rate, 0x300 to 0xFFFFF, the smallest prescaler that fits; the word carries the prescaler's code above the divisor.
    assert_eq!(m::PRESCALERS, [1, 2, 4, 6, 8, 10, 12, 16, 32, 64, 128, 256]);
    assert_eq!(m::divisor(LowPower, 9_600), Some((426_667, 9_599)));
    assert_eq!(m::divisor(LowPower, 57_600), Some((71_111, 57_600)));
    assert_eq!(m::divisor(LowPower, 115_200), Some((35_556, 115_198)));
    assert_eq!(m::divisor(LowPower, 921_600), Some((4_444, 921_692)), "the rate the USART's law refuses");
    assert_eq!(m::divisor(LowPower, 1_000_000), Some((4_096, 1_000_000)));
    assert_eq!(m::divisor(LowPower, 3_907), Some((1_048_375, 3_906)), "the lowest rate without a prescaler");
    assert_eq!(m::divisor(LowPower, 3_906), Some(((1 << 20) | 524_322, 3_905)), "one step lower: the clock over 2");
    assert_eq!(m::divisor(LowPower, 2_400), Some(((1 << 20) | 853_333, 2_400)));
    assert_eq!(m::divisor(LowPower, 300), Some(((7 << 20) | 853_333, 300)), "the clock over 16");
    assert_eq!(m::divisor(LowPower, 244), Some(((8 << 20) | 524_590, 244)), "the clock over 32");
    assert_eq!(m::divisor(LowPower, 16), Some(((11 << 20) | 1_000_000, 16)), "the floor: the clock over 256");
    assert_eq!(m::divisor(LowPower, 15), None, "one step under the floor");
    assert_eq!(m::divisor(LowPower, 5_333_333), Some((0x300, 5_333_333)), "the ceiling: the shortest divisor, a third of the clock");
    assert_eq!(m::divisor(LowPower, 5_333_334), None, "one step over the ceiling: the clock under three times the rate");
    assert_eq!(m::divisor(LowPower, 0), None);
    // Every word keeps its two parts inside their fields, and a divisor twice as long (the flight's doctored control)
    // still fits both registers at the three rates the flight times.
    for baud in [9_600, 115_200, 1_000_000] {
        let (u, _) = m::divisor(Usart16, baud).unwrap();
        let (l, _) = m::divisor(LowPower, baud).unwrap();
        assert!(u * 2 <= 65_535 && (l & 0xF_FFFF) * 2 <= 0xF_FFFF && l >> 20 == 0, "{baud}");
    }
    assert!(m::within_2_percent(100, 102) && m::within_2_percent(100, 98) && !m::within_2_percent(100, 103) && !m::within_2_percent(100, 97));
    // THE FRAME SET: 7 or 8 data bits, no parity or odd or even, 1 or 2 stop bits; the rest of the functions' vocabulary refused.
    for (d, p, s) in [(8, m::PARITY_NONE, 1), (8, m::PARITY_ODD, 2), (8, m::PARITY_EVEN, 1), (7, m::PARITY_NONE, 2), (7, m::PARITY_EVEN, 1), (7, m::PARITY_ODD, 1)] {
        assert!(m::frame_ok(d, p, s), "{d} {p} {s}");
    }
    for (d, p, s) in [(5, 0, 1), (6, 0, 1), (9, 0, 1), (8, 3, 1), (8, 4, 1), (8, 0, 0), (8, 0, 3)] {
        assert!(!m::frame_ok(d, p, s), "{d} {p} {s}");
    }
    // The word length counts the parity bit (RM0410 page 1276: its two bits read 00 for eight bits, 01 for nine, 10 for seven).
    assert_eq!(m::cr1_frame(8, m::PARITY_NONE), 0);
    assert_eq!(m::cr1_frame(8, m::PARITY_EVEN), m::CR1_M0 | m::CR1_PCE);
    assert_eq!(m::cr1_frame(8, m::PARITY_ODD), m::CR1_M0 | m::CR1_PCE | m::CR1_PS);
    assert_eq!(m::cr1_frame(7, m::PARITY_NONE), m::CR1_M1);
    assert_eq!(m::cr1_frame(7, m::PARITY_EVEN), m::CR1_PCE);
    assert_eq!(m::cr1_frame(7, m::PARITY_ODD), m::CR1_PCE | m::CR1_PS);
    assert_eq!((m::cr2_stop(1), m::cr2_stop(2)), (0, 0b10 << 12));
    assert_eq!((m::rx_mask(7), m::rx_mask(8)), (0x7F, 0xFF));
    // The registers and their bits as the two manuals state them (one layout for both blocks).
    assert_eq!((m::CR1, m::CR2, m::CR3, m::BRR, m::RQR, m::ISR, m::ICR, m::RDR, m::TDR, m::PRESC), (0x00, 0x04, 0x08, 0x0C, 0x18, 0x1C, 0x20, 0x24, 0x28, 0x2C));
    assert_eq!((m::CR1_UE, m::CR1_RE, m::CR1_TE, m::CR1_RXNEIE, m::CR1_TXEIE, m::CR1_PS, m::CR1_PCE, m::CR1_M0, m::CR1_M1, m::CR1_FIFOEN), (1, 1 << 2, 1 << 3, 1 << 5, 1 << 7, 1 << 9, 1 << 10, 1 << 12, 1 << 28, 1 << 29));
    assert_eq!((m::CR3_HDSEL, m::RQR_RXFRQ), (1 << 3, 1 << 3));
    assert_eq!((m::ISR_PE, m::ISR_FE, m::ISR_NF, m::ISR_ORE, m::ISR_RXNE, m::ISR_TC, m::ISR_TXE, m::ISR_TEACK, m::ISR_REACK), (1, 1 << 1, 1 << 2, 1 << 3, 1 << 5, 1 << 6, 1 << 7, 1 << 21, 1 << 22));
}

/// What a row's serial tables must read: its facts file, the port's row (the chip's own block number, the transmit
/// line, the receive line), the block's record as the manuals place it, and the two ring sizes in bytes. The lines are
/// each board's connector pins D1 and D0 as its manual tables them; a line's number is its port and pin.
struct UsartRowWant {
    file: &'static str,
    text: &'static str,
    port: (u64, u64, u64),
    block: crate::stm32_usart_math::Block,
    rings: (u64, u64),
}

const USART_ROWS: [UsartRowWant; 2] = [
    UsartRowWant {
        file: "st_stm32f769i_disco.rs",
        text: include_str!("../../kernel/src/board/facts/st_stm32f769i_disco.rs"),
        port: (6, 38, 39),
        block: crate::stm32_usart_math::Block { base: 0x4001_1400, law: crate::stm32_usart_math::Law::Usart16, enable_reg: 0x4002_3844, reset_reg: 0x4002_3824, sleep_reg: 0x4002_3864, bus_bit: 5, select_reg: 0x4002_3890, select_shift: 10, select_mask: 0b11, select_code: 0b10, fifo: false, alternate: 8, irq: 71 },
        rings: (1024, 1024),
    },
    UsartRowWant {
        file: "st_nucleo_u5a5zj_q.rs",
        text: include_str!("../../kernel/src/board/facts/st_nucleo_u5a5zj_q.rs"),
        port: (1, 103, 104),
        block: crate::stm32_usart_math::Block { base: 0x4600_2400, law: crate::stm32_usart_math::Law::LowPower, enable_reg: 0x4602_0CA8, reset_reg: 0x4602_0C80, sleep_reg: 0x4602_0CD0, bus_bit: 6, select_reg: 0x4602_0CE8, select_shift: 0, select_mask: 0b111, select_code: 0b010, fifo: true, alternate: 8, irq: 66 },
        rings: (1024, 1024),
    },
];

/// A facts file's serial tables as its text states them, against what its row must read: the first disagreement in
/// words, or none. The block's kind is the rate rule its row names.
fn usart_row_fault(text: &str, w: &UsartRowWant) -> Option<String> {
    use crate::stm32_usart_math as m;
    let ports: Vec<(u64, u64, u64)> = facts_decl(text, "UART_ROWS").split("UartRow {").skip(1).map(|r| (facts_field(r, "block:"), facts_field(r, "tx:"), facts_field(r, "rx:"))).collect();
    if ports != [w.port] {
        return Some(format!("the port rows read {ports:?}"));
    }
    let blocks: Vec<m::Block> = facts_decl(text, "UART_BLOCKS")
        .split("Block {")
        .skip(1)
        .map(|r| m::Block {
            base: facts_field(r, "base:") as usize,
            law: match (r.contains("law: crate::stm32_usart_math::Law::Usart16"), r.contains("law: crate::stm32_usart_math::Law::LowPower")) {
                (true, false) => m::Law::Usart16,
                (false, true) => m::Law::LowPower,
                _ => panic!("a block's row does not name one rate rule"),
            },
            enable_reg: facts_field(r, "enable_reg:") as usize,
            reset_reg: facts_field(r, "reset_reg:") as usize,
            sleep_reg: facts_field(r, "sleep_reg:") as usize,
            bus_bit: facts_field(r, "bus_bit:") as u32,
            select_reg: facts_field(r, "select_reg:") as usize,
            select_shift: facts_field(r, "select_shift:") as u32,
            select_mask: facts_field(r, "select_mask:") as u32,
            select_code: facts_field(r, "select_code:") as u32,
            fifo: r.contains("fifo: true"),
            alternate: facts_field(r, "alternate:") as u8,
            irq: facts_field(r, "irq:") as usize,
        })
        .collect();
    if blocks != [w.block] {
        return Some(format!("the block rows read {blocks:x?}"));
    }
    let rings = (facts_field(facts_decl(text, "UART_RX_BYTES"), "usize ="), facts_field(facts_decl(text, "UART_TX_BYTES"), "usize ="));
    if rings != w.rings {
        return Some(format!("the ring sizes read {rings:?}"));
    }
    if !m::blocks_fit(ports.len(), &blocks) || !m::pair_sound(ports[0].1 as u32, ports[0].2 as u32) {
        return Some("the block table does not fit the port table, or the port's two lines are one".to_string());
    }
    None
}

#[test]
fn stm32_usart_rows_tables_tied_to_the_connector_tables() {
    use crate::stm32_usart_math as m;
    for w in &USART_ROWS {
        assert_eq!(usart_row_fault(w.text, w), None, "{}", w.file);
        let (block, tx, rx) = w.port;
        // THE REFUSALS: the same comparison over doctored copies of the file's text, each with one fact changed.
        let row = format!("UartRow {{ block: {block}, tx: {tx}, rx: {rx} }}");
        let doctored = [
            ("the port's block number changed", w.text.replacen(&row, &format!("UartRow {{ block: {}, tx: {tx}, rx: {rx} }}", block + 1), 1)),
            ("the transmit line moved a port over", w.text.replacen(&row, &format!("UartRow {{ block: {block}, tx: {}, rx: {rx} }}", tx + 16), 1)),
            ("the transmit and receive lines one line", w.text.replacen(&row, &format!("UartRow {{ block: {block}, tx: {rx}, rx: {rx} }}"), 1)),
            ("no port", w.text.replacen(&row, "", 1)),
            ("the block's interrupt line changed", w.text.replacen(&format!("irq: {} }}", w.block.irq), &format!("irq: {} }}", w.block.irq + 1), 1)),
            ("the block's bit in the clock controller changed", w.text.replacen(&format!("bus_bit: {},", w.block.bus_bit), &format!("bus_bit: {},", w.block.bus_bit + 1), 1)),
            ("the selector's code changed to another clock's", w.text.replacen("select_code: 0x2,", "select_code: 0x1,", 1)),
            ("the alternate function changed", w.text.replacen("fifo: false, alternate: 8,", "fifo: false, alternate: 7,", 1).replacen("fifo: true, alternate: 8,", "fifo: true, alternate: 7,", 1)),
            ("the transmit ring at the size of a row with a console wire", w.text.replacen("pub const UART_TX_BYTES: usize = 1024;", "pub const UART_TX_BYTES: usize = 4096;", 1)),
        ];
        for (what, text) in &doctored {
            assert_ne!(text.as_str(), w.text, "{}: the doctored copy is the file itself ({what})", w.file);
            assert!(usart_row_fault(text, w).is_some(), "{}: a doctored copy passed ({what})", w.file);
        }
        // The soundness rules by themselves, on the block's record with one fact changed.
        assert!(m::block_sound(&w.block) && m::blocks_fit(1, &[w.block]));
        assert!(!m::blocks_fit(2, &[w.block]) && !m::blocks_fit(2, &[w.block, w.block]) && !m::blocks_fit(0, &[w.block]), "{}: a port without a block, or two ports on one block, passed", w.file);
        let mut b = w.block;
        b.select_code = b.select_mask + 1;
        assert!(!m::block_sound(&b), "{}: a selector code outside its field passed", w.file);
        b = w.block;
        b.base += 2;
        assert!(!m::block_sound(&b), "{}: a base that is not a word's passed", w.file);
        b = w.block;
        b.fifo = true;
        assert_eq!(m::block_sound(&b), w.block.law == m::Law::LowPower, "{}: FIFOs are the low-power block's alone", w.file);
        assert_eq!(m::fifo_depth(&w.block), if w.block.fifo { 8 } else { 1 });
        let (tx, rx) = (tx as u32, rx as u32);
        assert!(m::pair_sound(tx, rx) && !m::pair_sound(tx, tx));
        // THE BOARD's OWN CLAIMS: the rule each facts file asserts refuses a port whose line the board holds.
        let free = crate::claims::fold_board(&[13, 14]);
        assert!(m::lines_free(&free, tx, rx));
        assert!(!m::lines_free(&crate::claims::fold_board(&[13, 14, rx as u16]), tx, rx) && !m::lines_free(&crate::claims::fold_board(&[tx as u16]), tx, rx), "{}: a port on a line the board holds passed", w.file);
        // THE FUNCTIONS' PIN RULE on a row whose pair is fixed and whose chip lists no legal pins for an override:
        // legal with the project's words unset; with a word named, not legal, and the facts' pair is the answer.
        assert_eq!(crate::uart_math::effective_pair((tx, rx), None, 0, 0), (tx, rx, true), "{}", w.file);
        assert_eq!(crate::uart_math::effective_pair((tx, rx), None, 0x100 | 40, 0), (tx, rx, false), "{}", w.file);
        assert_eq!(crate::uart_math::effective_pair((tx, rx), None, 0x100 | 40, 0x100 | 41), (tx, rx, false), "{}", w.file);
    }
    // The connector's D1 and D0 by port and pin: PC6 and PC7 on the STM32F769 Discovery, PG7 and PG8 on the NUCLEO-U5A5ZJ-Q.
    assert_eq!((USART_ROWS[0].port.1, USART_ROWS[0].port.2), (2 * 16 + 6, 2 * 16 + 7));
    assert_eq!((USART_ROWS[1].port.1, USART_ROWS[1].port.2), (6 * 16 + 7, 6 * 16 + 8));
    // The block's kind is the rate rule its row names: a USART on the first row, a low-power UART on the second.
    assert!(USART_ROWS[0].block.law == m::Law::Usart16 && USART_ROWS[1].block.law == m::Law::LowPower);
}

/// A scripted serial block for the ST serial module's sequences: the registers kept in a table, every write logged; the
/// two acknowledge flags show in the status register `acks_after` reads after an enable (never when it is 0); a byte
/// waiting shows the receive flag with its own error flags and leaves at a read of the data register; a write to the
/// clear register clears the standing flags it names AND the same flags of the byte waiting in the data register (an
/// error flag belongs to the byte in that register: RM0456 page 2892), so a decode that cleared a byte's flags after
/// taking it would clear those of the byte behind it; the receive flush empties the queue.
struct UsartScript {
    base: usize,
    mem: std::collections::BTreeMap<usize, u32>,
    log: Vec<(usize, u32)>,
    acks_after: u32,
    status_reads: u32,
    rx: std::collections::VecDeque<(u32, u32)>,
}

impl UsartScript {
    fn new(base: usize, acks_after: u32) -> Self {
        UsartScript { base, mem: Default::default(), log: Vec::new(), acks_after, status_reads: 0, rx: Default::default() }
    }
    fn at(&self, addr: usize) -> u32 {
        *self.mem.get(&addr).unwrap_or(&0)
    }
}

impl crate::stm32_usart_math::Regs for UsartScript {
    fn rd(&mut self, addr: usize) -> u32 {
        use crate::stm32_usart_math as m;
        if addr == self.base + m::ISR {
            let mut v = self.at(addr);
            if self.at(self.base + m::CR1) & m::CR1_UE != 0 {
                self.status_reads += 1;
                if self.acks_after != 0 && self.status_reads >= self.acks_after {
                    v |= m::ISR_TEACK | m::ISR_REACK;
                }
            }
            if let Some(&(_, flags)) = self.rx.front() {
                v |= m::ISR_RXNE | flags;
            }
            return v;
        }
        if addr == self.base + m::RDR {
            return self.rx.pop_front().map(|(d, _)| d).unwrap_or(0);
        }
        self.at(addr)
    }
    fn wr(&mut self, addr: usize, value: u32) {
        use crate::stm32_usart_math as m;
        self.log.push((addr, value));
        if addr == self.base + m::ICR {
            let kept = self.at(self.base + m::ISR) & !value;
            self.mem.insert(self.base + m::ISR, kept);
            if let Some(front) = self.rx.front_mut() {
                front.1 &= !value;
            }
        } else if addr == self.base + m::RQR {
            if value & m::RQR_RXFRQ != 0 {
                self.rx.clear();
            }
        } else {
            if addr == self.base + m::CR1 && value & m::CR1_UE == 0 {
                self.status_reads = 0;
            }
            self.mem.insert(addr, value);
        }
    }
}

#[test]
fn stm32_usart_sequences_over_a_scripted_block() {
    use crate::stm32_usart_math as m;
    // The two blocks as the manuals place them: USART6 on the STM32F7 (its bit 5 in the three APB2 registers, its
    // selector at bits 11:10 of the dedicated clocks register, code 10 for the internal oscillator) and LPUART1 on the
    // STM32U5 (its bit 6 in the three APB3 registers, its selector at bits 2:0, code 010).
    let f7 = m::Block { base: 0x4001_1400, law: m::Law::Usart16, enable_reg: 0x4002_3844, reset_reg: 0x4002_3824, sleep_reg: 0x4002_3864, bus_bit: 5, select_reg: 0x4002_3890, select_shift: 10, select_mask: 0b11, select_code: 0b10, fifo: false, alternate: 8, irq: 71 };
    let u5 = m::Block { base: 0x4600_2400, law: m::Law::LowPower, enable_reg: 0x4602_0CA8, reset_reg: 0x4602_0C80, sleep_reg: 0x4602_0CD0, bus_bit: 6, select_reg: 0x4602_0CE8, select_shift: 0, select_mask: 0b111, select_code: 0b010, fifo: true, alternate: 8, irq: 66 };
    // THE BRING-UP on the STM32F7, every other bit of the shared registers kept: the console's own block is enabled
    // in the same enable register (bit 4) and has its own selector in the same selector register (bits 1:0), beside
    // another block's at bits 13:12 and a stale code in this block's own field.
    let mut s = UsartScript::new(f7.base, 3);
    s.mem.insert(f7.enable_reg, 1 << 4);
    s.mem.insert(f7.reset_reg, 0);
    s.mem.insert(f7.select_reg, (0b11 << 12) | (0b01 << 10) | 0b01);
    s.mem.insert(f7.sleep_reg, 0x06F7_7F93);
    assert!(!m::selected(&mut s, &f7));
    m::bring_up(&mut s, &f7);
    assert_eq!(
        s.log,
        vec![
            (f7.enable_reg, (1 << 4) | (1 << 5)),
            (f7.reset_reg, 1 << 5),
            (f7.reset_reg, 0),
            (f7.select_reg, (0b11 << 12) | (0b10 << 10) | 0b01),
            (f7.sleep_reg, 0x06F7_7FB3),
        ],
        "the bus clock; the reset set and cleared; the selector's own field alone; the clock while the core sleeps"
    );
    assert!(m::selected(&mut s, &f7) && s.at(f7.select_reg) & 0b11 == 0b01, "the console's selector is as it was");
    assert_ne!(s.at(f7.select_reg), 0b10 << 10, "a write of the code alone would have cleared the neighbouring fields");
    // The same on the STM32U5, where the selector register carries the converter's clock choice at bits 14:12.
    let mut t = UsartScript::new(u5.base, 2);
    t.mem.insert(u5.select_reg, (0b101 << 12) | 0b100);
    m::bring_up(&mut t, &u5);
    assert_eq!(t.at(u5.select_reg), (0b101 << 12) | 0b010);
    assert_eq!((t.at(u5.enable_reg), t.at(u5.reset_reg), t.at(u5.sleep_reg)), (1 << 6, 0, 1 << 6));
    // CONFIGURE on the STM32F7 at 115200 Bd, 8 data bits, no parity, 1 stop bit: the block disabled, the divisor, the
    // stop field, no loop, the frame with the transmitter, the receiver and the receive interrupt, the enable, and
    // both acknowledges at the third read of the status register. No prescaler register is written on this block.
    s.log.clear();
    let (word, _) = m::divisor(f7.law, 115_200).unwrap();
    let on = m::CR1_TE | m::CR1_RE | m::CR1_RXNEIE;
    assert!(m::configure(&mut s, &f7, word, 8, m::PARITY_NONE, 1, false, 50));
    assert_eq!(s.log, vec![(f7.base + m::CR1, 0), (f7.base + m::BRR, 139), (f7.base + m::CR2, 0), (f7.base + m::CR3, 0), (f7.base + m::CR1, on), (f7.base + m::CR1, on | m::CR1_UE)]);
    assert!(m::is_up(&mut s, &f7) && s.status_reads == 3);
    // On the STM32U5 at 2400 Bd, 7 data bits, even parity, 2 stop bits, the loop on: the prescaler's code beside the
    // divisor, the single-wire mode, and the FIFO bit written before the enable.
    let (word, got) = m::divisor(u5.law, 2_400).unwrap();
    let on5 = m::CR1_PCE | m::CR1_TE | m::CR1_RE | m::CR1_RXNEIE | m::CR1_FIFOEN;
    t.log.clear();
    assert!(got == 2_400 && m::configure(&mut t, &u5, word, 7, m::PARITY_EVEN, 2, true, 50));
    assert_eq!(t.log, vec![(u5.base + m::CR1, 0), (u5.base + m::BRR, 853_333), (u5.base + m::PRESC, 1), (u5.base + m::CR2, m::CR2_STOP_2), (u5.base + m::CR3, m::CR3_HDSEL), (u5.base + m::CR1, on5), (u5.base + m::CR1, on5 | m::CR1_UE)]);
    // A BLOCK THAT NEVER ACKNOWLEDGES: the bound runs out, the block is disabled again, the answer is false.
    let mut dead = UsartScript::new(f7.base, 0);
    assert!(!m::configure(&mut dead, &f7, 139, 8, m::PARITY_NONE, 1, false, 50));
    assert_eq!(dead.log.last(), Some(&(f7.base + m::CR1, 0)));
    assert!(!m::is_up(&mut dead, &f7) && dead.status_reads == 0);
    // A DIVISOR TWICE AS LONG AS THE RULE GIVES (the flight's doctored control) is seen by a read-back of the divisor register.
    let mut twice = UsartScript::new(f7.base, 1);
    assert!(m::configure(&mut twice, &f7, 2 * 139, 8, m::PARITY_NONE, 1, false, 50));
    assert_ne!(twice.at(f7.base + m::BRR), m::divisor(f7.law, 115_200).unwrap().0);
    // THE TRANSMIT INTERRUPT IS A LEVEL: a byte put writes the data register and turns it on; the acknowledge turns it
    // off, clears a standing overrun and answers it once.
    s.log.clear();
    m::tx_put(&mut s, &f7, 0x55);
    assert_eq!(s.log, vec![(f7.base + m::TDR, 0x55), (f7.base + m::CR1, on | m::CR1_UE | m::CR1_TXEIE)]);
    s.mem.insert(f7.base + m::ISR, m::ISR_ORE | m::ISR_TXE | m::ISR_TC);
    s.log.clear();
    assert_eq!(m::ack(&mut s, &f7), m::ACK_OVERRUN);
    assert_eq!(s.log, vec![(f7.base + m::CR1, on | m::CR1_UE), (f7.base + m::ICR, m::ISR_ORE)]);
    assert_eq!(m::ack(&mut s, &f7), 0, "the overrun was cleared");
    assert!(m::tx_room(&mut s, &f7) && m::tx_idle(&mut s, &f7));
    // A BYTE WAITS AND THE DATA REGISTER IS FULL: the answer is no room, and the transmit interrupt is turned on, so that
    // the pass which could not send is followed by one when room is made (the acknowledge alone would leave the ring's
    // bytes waiting for the next byte received). With room, no register is written.
    s.mem.insert(f7.base + m::ISR, 0);
    s.log.clear();
    assert!(!m::tx_room(&mut s, &f7) && !m::tx_idle(&mut s, &f7));
    assert_eq!(s.log, vec![(f7.base + m::CR1, on | m::CR1_UE | m::CR1_TXEIE)]);
    assert_eq!(m::ack(&mut s, &f7), 0);
    assert_eq!(s.at(f7.base + m::CR1), on | m::CR1_UE, "the acknowledge turned it off again");
    s.mem.insert(f7.base + m::ISR, m::ISR_TXE);
    s.log.clear();
    assert!(m::tx_room(&mut s, &f7) && s.log.is_empty(), "with room, no register is written");
    s.mem.insert(f7.base + m::ISR, 0);
    // THE RECEIVE DECODE: a sound byte; a framing error with a zero byte is a break; with another byte a framing
    // error; a parity error; the byte masked to the frame's data bits; an overrun is not the byte's fault.
    s.rx.extend([(0x41, 0), (0x00, m::ISR_FE), (0x42, m::ISR_FE | m::ISR_NF), (0x1C3, m::ISR_PE), (0x43, m::ISR_ORE)]);
    s.log.clear();
    assert_eq!(m::rx_get(&mut s, &f7, 0xFF), Some((0x41, 0)));
    assert_eq!(m::rx_get(&mut s, &f7, 0xFF), Some((0x00, m::ERR_BREAK)));
    assert_eq!(m::rx_get(&mut s, &f7, 0xFF), Some((0x42, m::ERR_FRAMING)));
    assert_eq!(m::rx_get(&mut s, &f7, 0xFF), Some((0xC3, m::ERR_PARITY)));
    assert_eq!(m::rx_get(&mut s, &f7, 0x7F), Some((0x43, 0)));
    assert_eq!(m::rx_get(&mut s, &f7, 0xFF), None);
    assert_eq!(s.log, vec![(f7.base + m::ICR, m::ISR_FE), (f7.base + m::ICR, m::ISR_FE | m::ISR_NF), (f7.base + m::ICR, m::ISR_PE)], "each byte's own flags cleared, the overrun left to the acknowledge");
    // THE FLUSH discards what waits and clears every error flag.
    s.rx.extend([(0x44, 0), (0x45, 0)]);
    s.log.clear();
    m::rx_flush(&mut s, &f7);
    assert_eq!(s.log, vec![(f7.base + m::RQR, m::RQR_RXFRQ), (f7.base + m::ICR, m::ISR_PE | m::ISR_FE | m::ISR_NF | m::ISR_ORE)]);
    assert_eq!(m::rx_get(&mut s, &f7, 0xFF), None);
    // THE LOOP turned on a configured block: disabled for the write, put back as it was.
    s.log.clear();
    m::set_loop(&mut s, &f7, true);
    assert_eq!(s.log, vec![(f7.base + m::CR1, on), (f7.base + m::CR3, m::CR3_HDSEL), (f7.base + m::CR1, on | m::CR1_UE)]);
    m::set_loop(&mut s, &f7, false);
    assert_eq!(s.at(f7.base + m::CR3), 0);
    // THE RELEASE: the block disabled with its interrupts off.
    m::release(&mut s, &f7);
    assert!(!m::is_up(&mut s, &f7) && s.at(f7.base + m::CR1) == 0);
}

// ---------------- the wireless rows: the digest, the radio's images, the stored network ----------------

fn digest_hex(digest: [u8; 32]) -> String {
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

fn digest_whole(data: &[u8]) -> String {
    let mut h = crate::digest::Sha256::new();
    h.update(data);
    digest_hex(h.finish())
}

/// The stream digest against the standard's own answers, against the gates'
/// one-shot implementation at every length around a block's and the
/// padding's edges, and fed in slices of every awkward size.
#[test]
fn digest_sha256_known_answers_whole_and_in_slices() {
    assert_eq!(digest_whole(b""), "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
    assert_eq!(digest_whole(b"abc"), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
    assert_eq!(digest_whole(b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"), "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1");
    let million = vec![b'a'; 1_000_000];
    assert_eq!(digest_whole(&million), "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0");
    let data: Vec<u8> = (0..300u32).map(|i| (i.wrapping_mul(2_654_435_761) >> 24) as u8).collect();
    for n in 0..=data.len() {
        assert_eq!(digest_whole(&data[..n]), sha256_hex(&data[..n]), "a message of {n} bytes");
    }
    let whole = digest_whole(&data);
    for slice in [1usize, 3, 7, 55, 56, 57, 63, 64, 65, 119, 120, 127, 128, 129, 299] {
        let mut h = crate::digest::Sha256::new();
        for part in data.chunks(slice) {
            h.update(part);
        }
        assert_eq!(digest_hex(h.finish()), whole, "fed in slices of {slice} bytes");
    }
    // An empty slice between two others changes nothing.
    let mut h = crate::digest::Sha256::new();
    h.update(&data[..100]);
    h.update(&[]);
    h.update(&data[100..]);
    assert_eq!(digest_hex(h.finish()), whole);
    // A digest in the characters a pin is published in.
    let mut h = crate::digest::Sha256::new();
    h.update(b"abc");
    assert_eq!(&crate::digest::hex(&h.finish())[..], b"ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
}

/// The digest of `data` as bytes, by the gates' own implementation.
fn wifi_sum(data: &[u8]) -> [u8; 32] {
    wifi_unhex(&sha256_hex(data))
}

fn wifi_unhex(text: &str) -> [u8; 32] {
    let mut sum = [0u8; 32];
    for (i, byte) in sum.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&text[2 * i..2 * i + 2], 16).unwrap();
    }
    sum
}

/// The directory of the radio's corner built from its fields: the kind,
/// the offset, the length and the digest of each entry.
fn wifi_directory(entries: &[(u8, u32, u32, [u8; 32])]) -> Vec<u8> {
    let mut d = b"LWFD".to_vec();
    d.push(1);
    d.extend_from_slice(&((12 + 44 * entries.len()) as u16).to_le_bytes());
    d.push(entries.len() as u8);
    for (kind, offset, len, sum) in entries {
        d.push(*kind);
        d.extend_from_slice(&[0; 3]);
        d.extend_from_slice(&offset.to_le_bytes());
        d.extend_from_slice(&len.to_le_bytes());
        d.extend_from_slice(sum);
    }
    d.extend_from_slice(&[0; 4]);
    wifi_directory_reseal(&mut d);
    d
}

/// The directory's CRC-32 written anew after an edit of its fields.
fn wifi_directory_reseal(d: &mut [u8]) {
    let n = d.len();
    let crc = crate::wifi_stored::crc32(&d[..n - 4]);
    d[n - 4..].copy_from_slice(&crc.to_le_bytes());
}

/// The corner's first unit: the directory, then erased bytes.
fn wifi_unit(directory: &[u8]) -> Vec<u8> {
    let mut unit = vec![0xFFu8; crate::wifi_images::UNIT];
    unit[..directory.len()].copy_from_slice(directory);
    unit
}

/// The room of a corner staged by the test: the directory of the images
/// at their offsets, the images, and every other byte erased.
fn wifi_corner(images: &[(u8, usize, &[u8])]) -> Vec<u8> {
    let entries: Vec<(u8, u32, u32, [u8; 32])> = images.iter().map(|(kind, offset, bytes)| (*kind, *offset as u32, bytes.len() as u32, wifi_sum(bytes))).collect();
    let directory = wifi_directory(&entries);
    let mut room = vec![0xFFu8; crate::wifi_images::ROOM];
    room[..directory.len()].copy_from_slice(&directory);
    for (_, offset, bytes) in images {
        room[*offset..*offset + bytes.len()].copy_from_slice(bytes);
    }
    room
}

/// The pins of three images of the test's own.
fn wifi_pins(firmware: &[u8], regulatory: &[u8], settings: &[u8], version: &'static [u8]) -> &'static crate::wifi_images::Pins {
    let pin = |bytes: &[u8]| crate::wifi_images::Pin { len: bytes.len(), sha256: Box::leak(sha256_hex(bytes).into_boxed_str()) };
    Box::leak(Box::new(crate::wifi_images::Pins { firmware: pin(firmware), regulatory: pin(regulatory), settings: pin(settings), version }))
}

/// The check of the radio's corner over a corner the test stages itself:
/// the directory read, the three images digested where they lie. CONTROL:
/// one flipped bit in each image is refused by the image's name, bytes
/// that are no image's are not read, and a corner written for another set
/// of pins gives this kernel nothing.
#[test]
fn wifi_images_corner_check_accepts_a_staged_corner() {
    use crate::wifi_images as g;
    assert_eq!((g::PINS.firmware.len, g::PINS.regulatory.len, g::PINS.settings.len), (224_190, 984, 741));
    assert_eq!(g::PINS.version, b"7.95.49");
    for pin in [&g::PINS.firmware, &g::PINS.regulatory, &g::PINS.settings] {
        assert_eq!(pin.sha256.len(), 64);
        assert!(pin.sha256.bytes().all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c)), "a pin is written in lowercase hexadecimal");
    }
    assert_eq!((g::UNIT, g::CORNER, g::ROOM), (4096, 262_144, 253_952));
    // Three images of the test's own, of the pinned images' lengths, at the
    // layout's places, and their pins.
    let image = |n: u32, salt: u32| -> Vec<u8> { (0..n).map(|i| (i.wrapping_mul(0x9E37_79B1).wrapping_add(salt) >> 13) as u8).collect() };
    let (firmware, regulatory, settings) = (image(224_190, 1), image(984, 2), image(741, 3));
    let pins = wifi_pins(&firmware, &regulatory, &settings, b"0.0.0");
    let room = wifi_corner(&[(1, 0x1000, &firmware[..]), (2, 0x3A000, &regulatory[..]), (3, 0x3B000, &settings[..])]);
    let want = g::Places { firmware: (0x1000, 224_190), regulatory: (0x3A000, 984), settings: (0x3B000, 741) };
    assert_eq!(g::check(&room, pins), Ok(want));
    // CONTROL: one flipped bit in each image, at its first byte, its middle
    // and its last, is refused by the image's name.
    for (image, (offset, len)) in [(g::Image::Firmware, want.firmware), (g::Image::Regulatory, want.regulatory), (g::Image::Settings, want.settings)] {
        for at in [offset, offset + len / 2, offset + len - 1] {
            let mut bad = room.clone();
            bad[at] ^= 0x04;
            assert_eq!(g::check(&bad, pins), Err(g::Fault::Content(image)), "a flipped bit at {at}");
        }
    }
    // Bytes that are no image's are not read: the byte after an image, a
    // unit no image lies in, the directory's unit after the directory.
    let mut padded = room.clone();
    padded[0x1000 + 224_190] ^= 1;
    padded[0x3C000] = 0;
    padded[200] = 0;
    assert_eq!(g::check(&padded, pins), Ok(want));
    // The directory's own fault comes first.
    let mut bad = room.clone();
    bad[20] ^= 1;
    assert_eq!(g::check(&bad, pins), Err(g::Fault::Crc));
    assert_eq!(g::check(&room[..4095], pins), Err(g::Fault::Unit));
    // A corner written for another set of pins gives this kernel nothing.
    assert_eq!(g::check(&room, &g::PINS), Err(g::Fault::Missing(g::Image::Firmware)));
    // In steps of one unit: 55 for the firmware, one for each of the other
    // two; the verdict at the last step, and repeated after it.
    let mut check = g::Check::new(want);
    for step in 1..57 {
        assert_eq!(check.step(&room, pins, g::UNIT), g::Step::More, "step {step}");
    }
    assert_eq!(check.step(&room, pins, g::UNIT), g::Step::Good);
    assert_eq!(check.step(&room, pins, g::UNIT), g::Step::Good);
    let mut bad = room.clone();
    bad[0x3A000 + 500] ^= 0x80;
    let mut check = g::Check::new(want);
    for step in 1..56 {
        assert_eq!(check.step(&bad, pins, g::UNIT), g::Step::More, "step {step}");
    }
    assert_eq!(check.step(&bad, pins, g::UNIT), g::Step::Bad(g::Image::Regulatory));
    assert_eq!(check.step(&bad, pins, g::UNIT), g::Step::Bad(g::Image::Regulatory));
    // A slice shorter than the places say is not the room.
    let mut check = g::Check::new(want);
    assert_eq!(check.step(&room[..0x1000 + 1000], pins, g::UNIT), g::Step::Bad(g::Image::Firmware));
}

/// The directory of the radio's corner, read by its rules: the directory
/// a packager writes for the three pinned images is the published one
/// byte for byte, and its three places are read. CONTROL: every single
/// flipped bit of it is refused; a wrong magic, format, length or CRC-32,
/// an entry off a unit boundary, inside the directory's unit or past the
/// room, and a pin with no entry are each refused by name; another
/// reader's entries and the reserved bytes are passed over.
#[test]
fn wifi_images_directory_is_read_by_its_rules() {
    use crate::wifi_images as g;
    let today = [
        (1u8, 0x1000u32, 224_190u32, wifi_unhex(g::PINS.firmware.sha256)),
        (2, 0x3A000, 984, wifi_unhex(g::PINS.regulatory.sha256)),
        (3, 0x3B000, 741, wifi_unhex(g::PINS.settings.sha256)),
    ];
    let d = wifi_directory(&today);
    assert_eq!(d.len(), 144);
    assert_eq!(sha256_hex(&d), "7df597c60f85a18144dcb158bc328f5bccc27a6b0a1e6f5f306c417211e25465", "the directory as it was published with the corner's shape");
    let want = g::Places { firmware: (0x1000, 224_190), regulatory: (0x3A000, 984), settings: (0x3B000, 741) };
    assert_eq!(g::places(&wifi_unit(&d), &g::PINS), Ok(want));
    for bit in 0..d.len() * 8 {
        let mut bad = d.clone();
        bad[bit / 8] ^= 1 << (bit % 8);
        assert!(g::places(&wifi_unit(&bad), &g::PINS).is_err(), "bit {bit} flipped");
    }
    assert_eq!(g::places(&[0xFF; 4096], &g::PINS), Err(g::Fault::Magic), "an erased unit");
    assert_eq!(g::places(&[0; 4096], &g::PINS), Err(g::Fault::Magic), "a zeroed unit");
    assert_eq!(g::places(&wifi_unit(&d)[..4095], &g::PINS), Err(g::Fault::Unit));
    let edited = |edit: &dyn Fn(&mut Vec<u8>), reseal: bool| {
        let mut bad = d.clone();
        edit(&mut bad);
        if reseal {
            wifi_directory_reseal(&mut bad);
        }
        g::places(&wifi_unit(&bad), &g::PINS)
    };
    assert_eq!(edited(&|b| b[0] = b'M', true), Err(g::Fault::Magic));
    assert_eq!(edited(&|b| b[4] = 2, true), Err(g::Fault::Format));
    assert_eq!(edited(&|b| b[5] = 143, true), Err(g::Fault::Length), "a length that is not the entries'");
    assert_eq!(edited(&|b| b[7] = 4, true), Err(g::Fault::Length), "a count the length does not carry");
    assert_eq!(edited(&|b| b[7] = 0, true), Err(g::Fault::Length), "no entry");
    assert_eq!(edited(&|b| b[60] ^= 1, false), Err(g::Fault::Crc), "a digest edited under the old CRC-32");
    assert_eq!(edited(&|b| b[143] ^= 0x80, false), Err(g::Fault::Crc));
    // Under a matching CRC-32, an entry outside its bounds, by its index.
    let entry = |i: usize, offset: u32, len: u32| {
        edited(
            &move |b: &mut Vec<u8>| {
                b[8 + 44 * i + 4..8 + 44 * i + 8].copy_from_slice(&offset.to_le_bytes());
                b[8 + 44 * i + 8..8 + 44 * i + 12].copy_from_slice(&len.to_le_bytes());
            },
            true,
        )
    };
    assert_eq!(entry(0, 0x1001, 224_190), Err(g::Fault::Entry(0)), "off a unit boundary");
    assert_eq!(entry(1, 0, 984), Err(g::Fault::Entry(1)), "inside the directory's unit");
    assert_eq!(entry(2, 0x3E000, 741), Err(g::Fault::Entry(2)), "past the room");
    assert_eq!(entry(2, 0x3D000, 4097), Err(g::Fault::Entry(2)), "ending past the room");
    assert_eq!(entry(1, 0x3A000, 0), Err(g::Fault::Entry(1)), "an empty image");
    assert_eq!(entry(0, 0xFFFF_F000, 0x2000), Err(g::Fault::Entry(0)), "an end that wraps");
    assert_eq!(entry(2, 0x3D000, 4096), Err(g::Fault::Missing(g::Image::Settings)), "the room's last unit is inside it, and the entry is not the pin's any more");
    // A pin with no entry: its kind not listed, another digest, another
    // length, its digest under another kind.
    assert_eq!(g::places(&wifi_unit(&wifi_directory(&today[..2])), &g::PINS), Err(g::Fault::Missing(g::Image::Settings)));
    assert_eq!(g::places(&wifi_unit(&wifi_directory(&today[1..])), &g::PINS), Err(g::Fault::Missing(g::Image::Firmware)));
    let mut other = today;
    other[1].3[0] ^= 1;
    assert_eq!(g::places(&wifi_unit(&wifi_directory(&other)), &g::PINS), Err(g::Fault::Missing(g::Image::Regulatory)), "another digest");
    let mut other = today;
    other[0].2 -= 1;
    assert_eq!(g::places(&wifi_unit(&wifi_directory(&other)), &g::PINS), Err(g::Fault::Missing(g::Image::Firmware)), "another length");
    let mut other = today;
    other[2].0 = 4;
    assert_eq!(g::places(&wifi_unit(&wifi_directory(&other)), &g::PINS), Err(g::Fault::Missing(g::Image::Settings)), "the digest under another kind");
    // Passed over: another reader's entries listed before this kernel's (a
    // second firmware of another digest, a Bluetooth patch), and the
    // reserved bytes.
    let mut more = vec![(1u8, 0x1000u32, 224_190u32, [0x5A; 32]), (4, 0x3C000, 6970, [0xA5; 32])];
    more.extend_from_slice(&today);
    let mut d5 = wifi_directory(&more);
    assert_eq!(d5.len(), 12 + 44 * 5);
    assert_eq!(g::places(&wifi_unit(&d5), &g::PINS), Ok(want));
    d5[8 + 44 * 2 + 1] = 0x77;
    wifi_directory_reseal(&mut d5);
    assert_eq!(g::places(&wifi_unit(&d5), &g::PINS), Ok(want), "the reserved bytes are not read");
    // The largest directory a unit holds: 92 entries.
    let many: Vec<(u8, u32, u32, [u8; 32])> = (0..89u32).map(|i| (9u8, 0x1000 + 0x1000 * (i % 60), 1u32, [i as u8; 32])).chain(today).collect();
    let big = wifi_directory(&many);
    assert_eq!((many.len(), big.len()), (92, 4060));
    assert_eq!(g::places(&wifi_unit(&big), &g::PINS), Ok(want));
}

/// One slot of the stored network's format built from its fields: the
/// first record at the unit's start, erased bytes after it.
fn wifi_slot(kinds: u8, name: &[u8], secret: &[u8], sequence: u32, reconnect: u8, boot: u8) -> Vec<u8> {
    let mut unit = vec![0xFFu8; crate::wifi_stored::UNIT];
    unit[..192].fill(0);
    unit[..4].copy_from_slice(b"LWFR");
    unit[4] = 1;
    unit[5] = kinds;
    unit[6] = name.len() as u8;
    unit[7] = secret.len() as u8;
    unit[8..12].copy_from_slice(&sequence.to_le_bytes());
    unit[12..12 + name.len()].copy_from_slice(name);
    unit[44..44 + secret.len()].copy_from_slice(secret);
    unit[172] = reconnect;
    unit[173] = boot;
    wifi_reseal(&mut unit);
    unit
}

/// The first record's CRC-32 written anew after an edit of its fields.
fn wifi_reseal(unit: &mut [u8]) {
    let crc = crate::wifi_stored::crc32(&unit[..188]);
    unit[188..192].copy_from_slice(&crc.to_le_bytes());
}

/// A later record written at `at`; answers the offset after it.
fn wifi_later(unit: &mut [u8], at: usize, magic: &[u8; 4], body: &[u8]) -> usize {
    let len = 11 + body.len();
    unit[at..at + 4].copy_from_slice(magic);
    unit[at + 4] = 1;
    unit[at + 5..at + 7].copy_from_slice(&(len as u16).to_le_bytes());
    unit[at + 7..at + 7 + body.len()].copy_from_slice(body);
    let crc = crate::wifi_stored::crc32(&unit[at..at + len - 4]);
    unit[at + len - 4..at + len].copy_from_slice(&crc.to_le_bytes());
    at + len
}

/// The four sample slots published with the format for its readers (test
/// values, never a real network), rebuilt here from their fields; each is
/// proven to be the published file byte for byte by the file's SHA-256.
fn wifi_sample(n: u8) -> Vec<u8> {
    const NAME: &[u8] = b"LamellaTest";
    const SECRET: &[u8] = b"lamella-test-passphrase";
    let (unit, sha256) = match n {
        1 => (wifi_slot(6, NAME, SECRET, 1, 0, 0), "895d1985867765b4d2e396af30a922a0fdcff302dad47f681abcdd771cbbc564"),
        2 => (wifi_slot(6, NAME, SECRET, 2, 0, 1), "353fbd1700e3bc434533cd1de88adddb48acd619d24ded88ddb5d230ceca4e50"),
        3 => (wifi_slot(1, b"Lamella Caf\xc3\xa9", b"", 1, 0, 0), "0123a397802b09a63e3555bedf059e6d96e37c1fc25eb6377d63233d56831862"),
        _ => {
            // Sample 2's record, then one later record of a test kind no
            // reader knows: 131 bytes from offset 192, across the first
            // page's end.
            let mut unit = wifi_slot(6, NAME, SECRET, 2, 0, 1);
            let body: Vec<u8> = b"A later record of a kind this writer does not know. ".iter().copied().cycle().take(120).collect();
            assert_eq!(wifi_later(&mut unit, 192, b"LTST", &body), 323);
            (unit, "2570e6067ebada9361d9432854766f0a3a7ef3c2edba10fc372bc30b9adf2e5f")
        }
    };
    assert_eq!(sha256_hex(&unit), sha256, "sample {n} rebuilt from its fields is not the published slot");
    unit
}

#[test]
fn wifi_stored_sample_one_is_read() {
    use crate::wifi_stored as w;
    let unit = wifi_sample(1);
    let r = w::read(&unit).expect("sample 1 is a valid slot");
    assert_eq!(r.name, b"LamellaTest");
    assert_eq!(r.secret, b"lamella-test-passphrase");
    assert_eq!((r.sequence, r.kinds, r.reconnect, r.boot, r.later), (1, w::KIND_WPA2 | w::KIND_WPA3, w::RECONNECT_AUTOMATIC, w::BOOT_BACKGROUND, 0));
    // In place: the slices lie inside the unit at the format's offsets.
    assert_eq!(r.name.as_ptr() as usize - unit.as_ptr() as usize, 12);
    assert_eq!(r.secret.as_ptr() as usize - unit.as_ptr() as usize, 44);
    // Alone, beside an erased slot, it is current in either place.
    let erased = vec![0xFFu8; w::UNIT];
    assert_eq!(w::current(&unit, &erased), Ok(r));
    assert_eq!(w::current(&erased, &unit), Ok(r));
    assert_eq!(w::current(&erased, &erased), Err(w::Absent::Nothing));
}

#[test]
fn wifi_stored_newer_sequence_is_current_across_a_wrap() {
    use crate::wifi_stored as w;
    // Samples 1 and 2 together are the state a power cut leaves between a
    // write and the erase that follows it: two valid slots.
    let (one, two) = (wifi_sample(1), wifi_sample(2));
    for (a, b) in [(&one, &two), (&two, &one)] {
        let c = w::current(a, b).expect("two valid slots");
        assert_eq!((c.sequence, c.boot), (2, w::BOOT_BEFORE_PROGRAM));
    }
    assert!(w::newer(2, 1) && !w::newer(1, 2) && !w::newer(7, 7));
    assert!(w::newer(0, u32::MAX) && !w::newer(u32::MAX, 0), "across the wrap");
    assert!(w::newer(0x8000_0000, 1) && !w::newer(0x8000_0001, 1), "half the number space is the horizon");
    let before = wifi_slot(6, b"LamellaTest", b"lamella-test-passphrase", u32::MAX, 0, 0);
    let after = wifi_slot(6, b"LamellaTest", b"lamella-test-passphrase", 0, 0, 1);
    assert_eq!(w::current(&before, &after).map(|r| r.sequence), Ok(0));
    assert_eq!(w::current(&after, &before).map(|r| r.sequence), Ok(0));
    // Equal numbers: the first slot.
    let twin = wifi_slot(6, b"LamellaTest", b"lamella-test-passphrase", 2, 0, 0);
    assert_eq!(w::current(&twin, &two).map(|r| r.boot), Ok(w::BOOT_BACKGROUND));
    assert_eq!(w::current(&two, &twin).map(|r| r.boot), Ok(w::BOOT_BEFORE_PROGRAM));
}

#[test]
fn wifi_stored_name_is_counted_in_bytes() {
    use crate::wifi_stored as w;
    // Sample 3: an open network whose name is 12 characters and 13 bytes.
    let unit = wifi_sample(3);
    let r = w::read(&unit).expect("sample 3 is a valid slot");
    assert_eq!(r.name.len(), 13);
    assert_eq!(r.name, b"Lamella Caf\xc3\xa9");
    assert_eq!(r.secret, b"");
    assert_eq!((r.kinds, r.sequence, r.later), (w::KIND_OPEN, 1, 0));
}

#[test]
fn wifi_stored_later_record_is_skipped_by_its_length() {
    use crate::wifi_stored as w;
    let unit = wifi_sample(4);
    let r = w::read(&unit).expect("sample 4 is a valid slot");
    let two = wifi_sample(2);
    let r2 = w::read(&two).expect("sample 2 is a valid slot");
    assert_eq!(r.later, 1);
    assert_eq!((r.name, r.secret, r.sequence, r.kinds, r.reconnect, r.boot), (r2.name, r2.secret, r2.sequence, r2.kinds, r2.reconnect, r2.boot), "the network reads exactly as in sample 2");
    assert_eq!(&unit[192..196], b"LTST");
    assert_eq!(unit[323..327], [0xFF; 4], "the list ends at offset 323");
    assert_eq!(w::current(&wifi_sample(1), &unit).map(|r| (r.sequence, r.later)), Ok((2, 1)));
    // Two later records, the second ending exactly at the unit's end.
    let mut full = wifi_sample(2);
    let at = wifi_later(&mut full, 192, b"LTST", &[0x55; 100]);
    let body = vec![0xAA; w::UNIT - at - 11];
    assert_eq!(wifi_later(&mut full, at, b"MORE", &body), w::UNIT);
    assert_eq!(w::read(&full).map(|r| r.later), Ok(2));
    // A list that leaves three erased bytes ends at the unit's end too.
    let mut near = wifi_sample(2);
    let body = vec![0x33; w::UNIT - 192 - 11 - 3];
    assert_eq!(wifi_later(&mut near, 192, b"LTST", &body), w::UNIT - 3);
    assert_eq!(w::read(&near).map(|r| r.later), Ok(1));
    // CONTROL: those three bytes not erased.
    near[w::UNIT - 1] = 0;
    assert_eq!(w::read(&near), Err(w::Fault::LaterEnd));
}

/// CONTROL: a slot that holds no undamaged copy is never taken, each
/// refused by its named reason, and the undamaged older copy beside it
/// stays current. A format byte edited under the old CRC-32 is damage like
/// any other: the CRC-32 is judged before the format.
#[test]
fn wifi_stored_cut_slots_are_never_current() {
    use crate::wifi_stored as w;
    let older = wifi_sample(1);
    let kept = w::read(&older).expect("sample 1 is a valid slot");
    let good = wifi_sample(4);
    type Cut = (&'static str, w::Fault, fn(&mut Vec<u8>));
    let cuts: [Cut; 12] = [
        ("an erased slot", w::Fault::Magic, |u| u.fill(0xFF)),
        ("a zeroed slot", w::Fault::Magic, |u| u.fill(0)),
        ("one bit of the magic", w::Fault::Magic, |u| u[3] ^= 0x01),
        ("the format byte edited under the old CRC-32", w::Fault::Crc, |u| u[4] = 2),
        ("one bit of the first record", w::Fault::Crc, |u| u[100] ^= 0x01),
        ("one bit of the first record's CRC", w::Fault::Crc, |u| u[188] ^= 0x01),
        ("a later record's length under 11", w::Fault::LaterLength, |u| u[197..199].copy_from_slice(&10u16.to_le_bytes())),
        ("a later record's length past the unit", w::Fault::LaterLength, |u| u[197..199].copy_from_slice(&3905u16.to_le_bytes())),
        ("one bit of a later record", w::Fault::LaterCrc, |u| u[250] ^= 0x80),
        ("a later record cut at the first page's end", w::Fault::LaterCrc, |u| u[256..].fill(0xFF)),
        ("bytes after the list that are not erased", w::Fault::LaterLength, |u| u[323] = 0),
        ("a slice that is not one whole unit", w::Fault::Unit, |u| u.truncate(256)),
    ];
    for (name, fault, cut) in cuts {
        let mut slot = good.clone();
        cut(&mut slot);
        assert_eq!(w::read(&slot), Err(fault), "{name}");
        assert!(!fault.is_undamaged(), "{name}: no undamaged copy");
        assert_eq!(w::current(&older, &slot), Ok(kept), "{name}: the older slot stays current");
        assert_eq!(w::current(&slot, &older), Ok(kept), "{name}: in either place");
        assert_eq!(w::current(&slot, &slot), Err(w::Absent::Nothing), "{name}: alone it is no network");
    }
}

/// CONTROL: a value outside the known set under a good CRC is refused by
/// the value's name, and the copy that holds it is undamaged: newest, it
/// rules, so the usable older copy beside it is NOT taken and no network
/// is stored for this kernel; alone it reads the same. A secret whose
/// length does not fit its kind is no fault of the copy.
#[test]
fn wifi_stored_values_outside_their_range_are_refused() {
    use crate::wifi_stored as w;
    let older = wifi_sample(1);
    assert!(w::read(&older).is_ok(), "sample 1 is a usable copy");
    let pass = [0x61u8; 128];
    let slot = |kinds: u8, secret: &[u8], reconnect: u8, boot: u8| wifi_slot(kinds, b"LamellaTest", secret, 5, reconnect, boot);
    let mut refused: Vec<(&str, w::Fault, Vec<u8>)> = vec![
        ("no accepted kind", w::Fault::Kinds, slot(0, &pass[..10], 0, 0)),
        ("open beside WPA2", w::Fault::Kinds, slot(3, &pass[..10], 0, 0)),
        ("open beside WPA3", w::Fault::Kinds, slot(5, &pass[..10], 0, 0)),
        ("open beside both", w::Fault::Kinds, slot(7, &pass[..10], 0, 0)),
        ("a bit beyond the three kinds", w::Fault::Kinds, slot(8, &pass[..10], 0, 0)),
        ("a bit beyond them beside two kinds", w::Fault::Kinds, slot(14, &pass[..10], 0, 0)),
        ("a boot setting the format does not state", w::Fault::Boot, slot(6, &pass[..10], 0, 3)),
        ("a reconnection kind the format does not state", w::Fault::Reconnect, slot(6, &pass[..10], 2, 0)),
    ];
    for (name, at, value) in [("a name of no bytes", 6usize, 0u8), ("a name longer than its field", 6, 33), ("a secret longer than its field", 7, 129)] {
        let mut unit = slot(6, &pass[..10], 0, 0);
        unit[at] = value;
        wifi_reseal(&mut unit);
        refused.push((name, w::Fault::Lengths, unit));
    }
    for (name, fault, unit) in &refused {
        assert_eq!(w::crc32(&unit[..188]).to_le_bytes(), unit[188..192], "{name}: the CRC holds");
        assert_eq!(w::read(unit), Err(*fault), "{name}");
        assert!(fault.is_undamaged(), "{name}: an undamaged copy this reader cannot use");
        assert_eq!(w::current(&older, unit), Err(w::Absent::Unknown), "{name}: newest, it rules, and the usable older copy is not taken");
        assert_eq!(w::current(unit, &older), Err(w::Absent::Unknown), "{name}: in either place");
        assert_eq!(w::current(unit, unit), Err(w::Absent::Unknown), "{name}: alone it is a setting this reader does not know");
        assert_eq!(w::current(unit, &vec![0xFFu8; w::UNIT]), Err(w::Absent::Unknown), "{name}: beside an erased slot too");
    }
    for damage in [w::Fault::Unit, w::Fault::Magic, w::Fault::Crc, w::Fault::LaterLength, w::Fault::LaterCrc, w::Fault::LaterEnd] {
        assert!(!damage.is_undamaged(), "{damage:?} is damage or no copy");
    }
    assert!(w::Fault::Format.is_undamaged(), "a format this reader does not know stands under a matching CRC-32");
    // Inside the ranges: the four kinds, the three boot settings, the two
    // reconnection kinds.
    for kinds in [w::KIND_OPEN, w::KIND_WPA2, w::KIND_WPA3, w::KIND_WPA2_OR_WPA3] {
        for boot in [w::BOOT_BACKGROUND, w::BOOT_BEFORE_PROGRAM, w::BOOT_ON_REQUEST] {
            for reconnect in [w::RECONNECT_AUTOMATIC, w::RECONNECT_MANUAL] {
                let unit = slot(kinds, &pass[..20], reconnect, boot);
                let r = w::read(&unit).expect("every stated value is valid");
                assert_eq!((r.kinds, r.boot, r.reconnect), (kinds, boot, reconnect));
            }
        }
    }
    // A secret that does not fit its kind is valid: the join fails at the
    // network, not here.
    for (kinds, secret) in [(w::KIND_OPEN, &pass[..10]), (w::KIND_WPA2, &pass[..3]), (w::KIND_WPA2, &pass[..65]), (w::KIND_WPA3, &pass[..0]), (w::KIND_WPA2_OR_WPA3, &pass[..0])] {
        let unit = slot(kinds, secret, 0, 0);
        assert_eq!(w::read(&unit).map(|r| r.secret.len()), Ok(secret.len()), "kinds {kinds} with a secret of {} bytes", secret.len());
    }
}

/// The newest undamaged copy rules whatever it holds, and nothing falls
/// back to the older one: two boards must not end on different networks
/// because one of them took an old copy. A copy this reader cannot use --
/// a value outside the known set, or a format it does not know -- takes
/// part in the choice by its sequence number, which stands at the same
/// place in every format. CONTROLS: the same copy with the lower number
/// loses to a usable newer one, and a damaged newer copy is no copy.
#[test]
fn wifi_stored_newest_undamaged_copy_rules_whatever_it_holds() {
    use crate::wifi_stored as w;
    let erased = vec![0xFFu8; w::UNIT];
    // A usable copy with sequence number `n`.
    let usable = |n: u32| wifi_slot(6, b"LamellaTest", b"lamella-test-passphrase", n, 0, 0);
    let middle = usable(5);
    let kept = w::read(&middle).expect("a usable copy");
    assert_eq!(kept.sequence, 5);
    // The copies this reader cannot use, each made from a usable one by
    // one edit under a CRC-32 written anew: every value outside the known
    // set in turn, then a format this reader does not know -- once with
    // the known format's bytes, once with other bytes between the frame's
    // fields, which a later format is free to change.
    type Edit = (&'static str, w::Fault, fn(&mut Vec<u8>));
    let edits: [Edit; 10] = [
        ("no accepted kind", w::Fault::Kinds, |u| u[5] = 0),
        ("an accepted kind beyond the three", w::Fault::Kinds, |u| u[5] = 8),
        ("an open kind beside a secured one", w::Fault::Kinds, |u| u[5] = 3),
        ("a boot setting above 2", w::Fault::Boot, |u| u[173] = 3),
        ("a reconnection kind above 1", w::Fault::Reconnect, |u| u[172] = 2),
        ("a name of no bytes", w::Fault::Lengths, |u| u[6] = 0),
        ("a name of more than 32 bytes", w::Fault::Lengths, |u| u[6] = 33),
        ("a secret of more than 128 bytes", w::Fault::Lengths, |u| u[7] = 129),
        ("a format this reader does not know", w::Fault::Format, |u| u[4] = 2),
        ("a later format with other bytes between the frame's fields", w::Fault::Format, |u| {
            u[4] = 9;
            u[5..8].fill(0xEE);
            u[12..188].fill(0x5A);
        }),
    ];
    for (name, fault, edit) in edits {
        let odd = |n: u32| {
            let mut unit = usable(n);
            edit(&mut unit);
            wifi_reseal(&mut unit);
            unit
        };
        let (newest, oldest) = (odd(8), odd(2));
        assert_eq!(w::read(&newest), Err(fault), "{name}");
        assert_eq!(w::read(&oldest), Err(fault), "{name}");
        assert!(fault.is_undamaged(), "{name}: the frame's CRC-32 holds");
        // Newest, it rules: no record, and the usable older copy is NOT
        // taken.
        assert_eq!(w::current(&middle, &newest), Err(w::Absent::Unknown), "{name}: newest, beside a usable older copy");
        assert_eq!(w::current(&newest, &middle), Err(w::Absent::Unknown), "{name}: newest, in either place");
        // CONTROL: the same copy with the lower number loses to the usable
        // newer one.
        assert_eq!(w::current(&oldest, &middle), Ok(kept), "{name}: older, beside a usable newer copy");
        assert_eq!(w::current(&middle, &oldest), Ok(kept), "{name}: older, in either place");
        // Without a usable copy beside it: an erased slot, or its own
        // older twin.
        assert_eq!(w::current(&newest, &erased), Err(w::Absent::Unknown), "{name}: beside an erased slot");
        assert_eq!(w::current(&erased, &newest), Err(w::Absent::Unknown), "{name}: beside an erased slot, in either place");
        assert_eq!(w::current(&newest, &oldest), Err(w::Absent::Unknown), "{name}: two copies this reader cannot use");
        // Equal numbers: the first slot, whatever it holds.
        let twin = odd(5);
        assert_eq!(w::current(&twin, &middle), Err(w::Absent::Unknown), "{name}: equal numbers, the first slot rules");
        assert_eq!(w::current(&middle, &twin), Ok(kept), "{name}: equal numbers, the first slot rules");
        // Across the wrap of the number, 0 is newer than the highest.
        assert_eq!(w::current(&usable(u32::MAX), &odd(0)), Err(w::Absent::Unknown), "{name}: newest across the wrap");
        assert_eq!(w::current(&odd(u32::MAX), &usable(0)).map(|r| r.sequence), Ok(0), "{name}: older across the wrap");
    }
    // A damaged newer copy is no copy, whatever else it holds: the
    // undamaged older copy beside it is current. Each kind of damage, then
    // the two that the order of the reading decides -- a format byte this
    // reader does not know under a CRC-32 that does not match, and a value
    // outside the known set beside a later record that does not hold.
    let good = {
        let mut unit = usable(8);
        assert_eq!(wifi_later(&mut unit, 192, b"LTST", &[0x55; 120]), 323);
        unit
    };
    assert_eq!(w::current(&middle, &good).map(|r| (r.sequence, r.later)), Ok((8, 1)), "undamaged and usable, the newer copy rules");
    type Damage = (&'static str, w::Fault, fn(&mut Vec<u8>));
    let damages: [Damage; 7] = [
        ("one bit of the first record", w::Fault::Crc, |u| u[100] ^= 0x01),
        ("a later record's length under 11", w::Fault::LaterLength, |u| u[197..199].copy_from_slice(&10u16.to_le_bytes())),
        ("one bit of a later record", w::Fault::LaterCrc, |u| u[250] ^= 0x80),
        ("a list that does not end as said", w::Fault::LaterEnd, |u| {
            let body = vec![0x33; w::UNIT - 323 - 11 - 3];
            assert_eq!(wifi_later(u, 323, b"MORE", &body), w::UNIT - 3);
            u[w::UNIT - 1] = 0;
        }),
        ("no first record", w::Fault::Magic, |u| u[..4].fill(0xFF)),
        ("a format this reader does not know under a CRC-32 that does not match", w::Fault::Crc, |u| u[4] = 2),
        ("a value outside the known set beside a later record that does not hold", w::Fault::LaterCrc, |u| {
            u[173] = 3;
            wifi_reseal(u);
            u[250] ^= 0x80;
        }),
    ];
    // An undamaged older copy this reader cannot use, for the last leg.
    let odd_older = {
        let mut unit = usable(5);
        unit[4] = 2;
        wifi_reseal(&mut unit);
        unit
    };
    for (name, fault, damage) in damages {
        let mut slot = good.clone();
        damage(&mut slot);
        assert_eq!(w::read(&slot), Err(fault), "{name}");
        assert!(!fault.is_undamaged(), "{name}: damage, or no copy");
        assert_eq!(w::current(&middle, &slot), Ok(kept), "{name}: the undamaged older copy is current");
        assert_eq!(w::current(&slot, &middle), Ok(kept), "{name}: in either place");
        assert_eq!(w::current(&slot, &erased), Err(w::Absent::Nothing), "{name}: alone it is no network");
        // The damage hides nothing: an undamaged older copy this reader
        // cannot use still rules.
        assert_eq!(w::current(&slot, &odd_older), Err(w::Absent::Unknown), "{name}: beside an undamaged copy this reader cannot use");
    }
}

#[test]
fn wifi_stored_crc32_is_the_ieee_one() {
    use crate::wifi_stored as w;
    assert_eq!(w::crc32(b"123456789"), 0xCBF4_3926, "the check value");
    assert_eq!(w::crc32(b""), 0);
    assert_eq!(w::crc32(&[0u8; 32]), 0x190A_55AD);
    assert_eq!(w::crc32(&[0xFFu8; 32]), 0xFF6C_AB0B);
    // The three CRC-32 values published with the sample slots.
    for (n, crc) in [(1u8, 0x2E1B_0440u32), (2, 0x2F42_37C8), (3, 0x72B8_045C)] {
        let unit = wifi_sample(n);
        assert_eq!(u32::from_le_bytes([unit[188], unit[189], unit[190], unit[191]]), crc, "sample {n}");
        assert_eq!(w::crc32(&unit[..188]), crc, "sample {n}");
    }
    // A record's CRC-32 covers its own bytes alone, so a later record
    // copied to another offset stays valid.
    let mut a = wifi_sample(2);
    let mut b = wifi_sample(2);
    let end = wifi_later(&mut a, 192, b"LTST", &[7; 20]);
    wifi_later(&mut b, 192, b"PADS", &[1; 40]);
    let moved = a[192..end].to_vec();
    b[192 + 51..192 + 51 + moved.len()].copy_from_slice(&moved);
    assert_eq!(w::read(&b).map(|r| r.later), Ok(2));
}

// ---------------- the wireless medium's pure half, over the wifi driver's scripted transport ----------------

/// The medium's clock in a test: a cell the test sets, read as the
/// driver's scripted clock is read. With a step it moves at every reading.
struct WifiClock {
    now: &'static std::cell::Cell<u64>,
    step: u64,
}

impl lamella_wifi_cyw4343x::Clock for WifiClock {
    fn now_us(&mut self) -> u64 {
        let now = self.now.get();
        self.now.set(now + self.step);
        now
    }
    fn delay_us(&mut self, us: u32) {
        self.now.set(self.now.get() + u64::from(us));
    }
}

type WifiScript = lamella_wifi_cyw4343x::fixture::Op;
type WifiStored = Result<crate::wifi_stored::Record<'static>, crate::wifi_stored::Absent>;
type WifiCorner = (&'static [u8], &'static crate::wifi_images::Pins);

static WIFI_WAKE: core::sync::atomic::AtomicU32 = core::sync::atomic::AtomicU32::new(0);

/// The medium over a recorded exchange, with what a test reads of it.
struct WifiRig {
    medium: crate::wifi_medium::Wifi<'static, lamella_wifi_cyw4343x::fixture::FakeTransport<'static>, WifiClock>,
    clock: &'static std::cell::Cell<u64>,
    notes: Vec<crate::wifi_medium::Note>,
    rows: usize,
}

/// The exchanges' corner: their synthetic images at the layout's places,
/// with the pins of those images.
fn wifi_exchange_corner() -> WifiCorner {
    use crate::wifi_exchange as x;
    let room = wifi_corner(&[(1, 0x1000, &x::FIRMWARE[..]), (2, 0x3A000, &x::REGULATORY[..]), (3, 0x3B000, &x::SETTINGS[..])]);
    (Box::leak(room.into_boxed_slice()), wifi_pins(&x::FIRMWARE, &x::REGULATORY, &x::SETTINGS, &x::VERSION))
}

/// The exchanges' network as a stored record, the other slot erased.
fn wifi_exchange_record(kinds: u8, reconnect: u8, boot: u8) -> WifiStored {
    use crate::wifi_exchange as x;
    let first: &'static [u8] = Box::leak(wifi_slot(kinds, &x::NAME, &x::SECRET, 7, reconnect, boot).into_boxed_slice());
    let second: &'static [u8] = Box::leak(vec![0xFFu8; crate::wifi_stored::UNIT].into_boxed_slice());
    crate::wifi_stored::current(first, second)
}

fn wifi_rig(tables: &[&[WifiScript]], stored: WifiStored, corner: WifiCorner, step: u64) -> WifiRig {
    let script: &'static [WifiScript] = Box::leak(tables.concat().into_boxed_slice());
    let clock: &'static std::cell::Cell<u64> = Box::leak(Box::new(std::cell::Cell::new(0)));
    let bus = lamella_wifi_cyw4343x::fixture::FakeTransport::new(script, lamella_wifi_cyw4343x::Part::Cyw43439);
    let medium = crate::wifi_medium::Wifi::new(bus, WifiClock { now: clock, step }, &WIFI_WAKE, stored, corner.0, corner.1);
    WifiRig { medium, clock, notes: Vec::new(), rows: script.len() }
}

impl WifiRig {
    /// One service call, the notes it left taken.
    fn service(&mut self) {
        use crate::board::net_medium::Medium;
        self.medium.service(0);
        while let Some(note) = self.medium.take_note() {
            self.notes.push(note);
        }
    }

    /// Service calls until `done` holds, the clock moved to each instant
    /// the medium names and never further; answers the number of waits.
    /// Before the clock moves, a service call must poll nothing: the
    /// driver is never polled before the instant it named.
    fn drive(&mut self, what: &str, mut done: impl FnMut(&mut WifiRig) -> bool) -> u32 {
        let mut waits = 0;
        for _ in 0..200_000 {
            if done(self) {
                return waits;
            }
            self.service();
            if done(self) {
                return waits;
            }
            match self.medium.wait_us() {
                None => panic!("{what}: the medium stopped first, at {:?} with {:?}; the transport's fault {:?}", self.medium.stage(), self.notes, self.medium.bus().fault()),
                Some(0) => {}
                Some(wait) => {
                    let polls = self.medium.counts().polls;
                    self.service();
                    assert_eq!(self.medium.counts().polls, polls, "{what}: a poll before the instant the driver named");
                    self.clock.set(self.clock.get() + wait);
                    waits += 1;
                }
            }
        }
        panic!("{what}: not reached; the medium at {:?} with {:?}, {} rows left", self.medium.stage(), self.notes, self.left());
    }

    /// Service calls at the driver's cadence whatever the medium says, for
    /// a medium that is expected to do nothing.
    fn idle(&mut self, calls: u32) {
        for _ in 0..calls {
            self.service();
            self.clock.set(self.clock.get() + 10_000);
        }
    }

    /// The rows of the exchange not yet asked for.
    fn left(&self) -> usize {
        self.medium.bus().remaining()
    }

    /// The frame in the hold, as the stack would read it.
    fn held(&mut self) -> Option<Vec<u8>> {
        use crate::board::net_medium::Medium;
        self.medium.split().0.peek().map(|frame| frame.to_vec())
    }

    /// The held frame taken, as the stack takes it.
    fn take(&mut self) {
        use crate::board::net_medium::Medium;
        self.medium.split().0.consume();
    }

    /// A frame staged and committed through the transmit side, as the
    /// stack does it; answers whether a slot was free.
    fn send(&mut self, frame: &[u8]) -> bool {
        use crate::board::net_medium::Medium;
        let (_, tx) = self.medium.split();
        if !tx.room() {
            return false;
        }
        tx.slot(frame.len()).expect("a slot, with room").copy_from_slice(frame);
        tx.commit(frame.len());
        true
    }

    fn room(&mut self) -> bool {
        use crate::board::net_medium::Medium;
        self.medium.split().1.room()
    }

    /// Whether the medium reads its link up.
    fn up(&self) -> bool {
        use crate::board::net_medium::{Link, Medium};
        self.medium.link() == Link::Up
    }

    /// The console's lines of the notes so far.
    fn lines(&self) -> Vec<String> {
        self.notes.iter().map(wifi_line).collect()
    }
}

fn wifi_line(note: &crate::wifi_medium::Note) -> String {
    let mut line = String::new();
    note.write_line(&mut line).unwrap();
    line
}

/// The ladder joins and the link rises with the address: under the record
/// the tool writes by default (WPA2 or WPA3, join while the program runs)
/// the corner is checked first, the firmware is asked whether it runs the
/// WPA3 exchange, the scan hears the stored name offering WPA3 and the
/// join is WPA3, the recorded exchange consumed to its last row. The same
/// record joins as WPA2 where the network offers WPA2 alone, where the
/// scan does not hear the name, and on a firmware without the WPA3
/// exchange; a join asked as the wrong kind would not match its exchange.
#[test]
fn wifi_medium_ladder_joins_and_the_link_rises_with_the_address() {
    use crate::board::net_medium::Medium;
    use crate::wifi_exchange as x;
    use crate::wifi_medium::{Note, Stage};
    use crate::wifi_stored::{BOOT_BACKGROUND, KIND_WPA2_OR_WPA3, RECONNECT_AUTOMATIC};
    let corner = wifi_exchange_corner();
    let record = wifi_exchange_record(KIND_WPA2_OR_WPA3, RECONNECT_AUTOMATIC, BOOT_BACKGROUND);
    let mut rig = wifi_rig(&[&x::OPENING[..], &x::EITHER_WPA3[..]], record, corner, 0);
    assert_eq!((rig.medium.stage(), rig.up(), rig.medium.hardware_address()), (Stage::Corner, false, None));
    assert_eq!(rig.medium.wait_us(), Some(0));
    // The corner first: the directory in one service call, the three
    // images in three, and nothing on the transport meanwhile.
    for stage in [Stage::Checking, Stage::Checking, Stage::Checking, Stage::Attaching] {
        assert!(rig.notes.is_empty());
        rig.service();
        assert_eq!((rig.medium.stage(), rig.medium.wait_us(), rig.medium.counts().polls, rig.left()), (stage, Some(0), 0, rig.rows));
    }
    assert_eq!(rig.notes, [Note::Starting]);
    // The radio up: the chip's address is known, the link still down.
    rig.drive("the radio up", |r| r.medium.hardware_address().is_some());
    assert_eq!((rig.medium.hardware_address(), rig.up()), (Some(x::STATION), false));
    assert!(!rig.room(), "no slot for the stack before the link is up");
    rig.drive("the join", |r| r.up());
    assert_eq!(rig.medium.stage(), Stage::Joining);
    assert_eq!(rig.notes, [Note::Starting, Note::Joined]);
    assert_eq!(rig.lines(), ["[wifi] starting; joining the stored network", "[wifi] joined"]);
    // The exchange's last rows: a frame in, read in the hold, and a frame
    // out through the transmit side.
    rig.drive("a frame in", |r| r.held().is_some());
    assert_eq!(rig.held(), Some(x::ARP_REQUEST_IN.to_vec()));
    rig.take();
    assert!(rig.send(&x::ARP_REPLY_OUT));
    rig.drive("the exchange's end", |r| r.left() == 0);
    assert_eq!((rig.medium.bus().fault(), rig.medium.wait_us(), rig.medium.counts().dropped), (None, Some(10_000), 0), "consumed to its last row; the driver's next instant is the medium's");
    // A refusal later in a run -- here the transport's own, at an operation
    // past the exchange's end -- parks the medium with the link down.
    rig.clock.set(rig.clock.get() + 10_000);
    rig.service();
    assert_eq!((rig.medium.stage(), rig.up(), rig.room(), rig.medium.wait_us()), (Stage::Parked, false, false, None));
    assert!(matches!(rig.notes[..], [Note::Starting, Note::Joined, Note::Refused(_)]), "{:?}", rig.notes);
    for (what, table) in [("heard offering WPA2 alone", &x::EITHER_WPA2_HEARD[..]), ("not heard", &x::EITHER_WPA2_UNHEARD[..]), ("a firmware without the WPA3 exchange", &x::EITHER_NO_WPA3[..])] {
        let mut rig = wifi_rig(&[&x::OPENING[..], table], record, corner, 0);
        rig.drive(what, |r| r.left() == 0);
        assert_eq!((rig.up(), rig.medium.bus().fault(), &rig.notes[..]), (true, None, &[Note::Starting, Note::Joined][..]), "{what}");
    }
    // No line of the medium's carries a name, an address or a secret, and
    // each is a short statement of the state behind the same first word,
    // with no full stop at its end.
    let refusal = lamella_wifi_cyw4343x::Refusal::new("a stage", 0x1234);
    let every = [
        Note::Starting,
        Note::StartingOlderSetting,
        Note::OnRequest,
        Note::NothingStored,
        Note::UnknownSetting,
        Note::FirmwareNotFound(crate::wifi_images::Fault::Magic),
        Note::NotFound,
        Note::FirstTryFailed,
        Note::Joined,
        Note::SecretRefused { again_s: 5 },
        Note::AuthenticationFailed { again_s: 360 },
        Note::SecretUnusable,
        Note::NoAcceptedKind,
        Note::LinkLost,
        Note::Refused(refusal),
    ];
    for note in &every {
        let line = wifi_line(note);
        assert!(line.starts_with("[wifi] ") && !line.ends_with('.') && line.is_ascii() && !line.contains('\n'), "{line}");
        for secret in [&x::NAME[..], &x::SECRET[..]] {
            assert!(!line.contains(std::str::from_utf8(secret).unwrap()), "{line}");
        }
    }
    assert_eq!(wifi_line(&Note::NoAcceptedKind), "[wifi] stored network found, security type does not match; still looking");
    assert_eq!(wifi_line(&Note::SecretRefused { again_s: 5 }), "[wifi] passphrase not accepted; retrying in 5 s");
    assert_eq!(wifi_line(&Note::AuthenticationFailed { again_s: 360 }), "[wifi] authentication failed; retrying in 360 s");
    assert_eq!(wifi_line(&Note::LinkLost), "[wifi] link lost; rejoining");
    assert_eq!(wifi_line(&Note::Refused(refusal)), "[wifi] off: radio did not start (stage a stage, status 0x00001234)");
}

/// A frame passes each way through the kernel's own adapter and a stack
/// interface: the access point's ARP request is read by the stack in the
/// medium's hold, and the stack's reply is the exchange's next row byte
/// for byte; a datagram with an unset checksum is taken and answered with
/// nothing.
#[test]
fn wifi_medium_frame_passes_each_way_through_the_adapter() {
    use crate::board::net_medium::{Adapter, Medium};
    use crate::wifi_exchange as x;
    use crate::wifi_stored::{BOOT_BACKGROUND, KIND_WPA2, RECONNECT_AUTOMATIC};
    use smoltcp::iface::{Config, Interface, SocketSet, SocketStorage};
    use smoltcp::time::Instant;
    use smoltcp::wire::{EthernetAddress, HardwareAddress, IpCidr};
    let mut rig = wifi_rig(&[&x::OPENING[..], &x::WPA2[..]], wifi_exchange_record(KIND_WPA2, RECONNECT_AUTOMATIC, BOOT_BACKGROUND), wifi_exchange_corner(), 0);
    rig.drive("the join", |r| r.up());
    // The stack's interface over the adapter, with the chip's address as
    // the medium tells it and the exchange's address on its network.
    let hw = rig.medium.hardware_address().expect("the address, with the radio up");
    assert_eq!(rig.medium.frame_unit(), 1514);
    let mut config = Config::new(HardwareAddress::Ethernet(EthernetAddress(hw)));
    config.random_seed = 1;
    let mut iface = Interface::new(config, &mut Adapter { medium: &mut rig.medium }, Instant::from_millis(0));
    iface.update_ip_addrs(|addrs| addrs.push(IpCidr::new(IpAddress::v4(x::OUR_IP[0], x::OUR_IP[1], x::OUR_IP[2], x::OUR_IP[3]), 24)).unwrap());
    let mut storage: [SocketStorage; 0] = [];
    let mut sockets = SocketSet::new(&mut storage[..]);
    rig.drive("the request", |r| r.held().is_some());
    assert_eq!(rig.held(), Some(x::ARP_REQUEST_IN.to_vec()));
    let before = rig.left();
    let _ = iface.poll(Instant::from_millis(0), &mut Adapter { medium: &mut rig.medium }, &mut sockets);
    assert_eq!((rig.held(), rig.left(), rig.medium.bus().fault()), (None, before - 1, None), "the stack took the request, and the one row asked for is its reply's write");
    let frames = rig.medium.driver().frames();
    assert_eq!((frames.data_received, frames.data_sent), (1, 1));
    rig.drive("the datagram", |r| r.held().is_some());
    assert_eq!(rig.held(), Some(x::DATAGRAM_IN.to_vec()));
    let before = rig.left();
    let _ = iface.poll(Instant::from_millis(0), &mut Adapter { medium: &mut rig.medium }, &mut sockets);
    assert_eq!((rig.held(), rig.left(), rig.medium.bus().fault()), (None, before, None), "taken, and nothing written for it");
    assert_eq!(rig.medium.counts().dropped, 0);
}

/// CONTROL: without a valid record the radio stays off -- not one
/// operation reaches the transport -- and the same for a record that says
/// to join on request; each with its own line.
#[test]
fn wifi_medium_radio_stays_off_without_a_record_or_on_request() {
    use crate::board::net_medium::Medium;
    use crate::wifi_exchange as x;
    use crate::wifi_medium::{Note, Stage};
    use crate::wifi_stored::{Absent, BOOT_BACKGROUND, BOOT_ON_REQUEST, KIND_WPA2, RECONNECT_AUTOMATIC};
    let corner = wifi_exchange_corner();
    let erased: &'static [u8] = Box::leak(vec![0xFFu8; crate::wifi_stored::UNIT].into_boxed_slice());
    let nothing = crate::wifi_stored::current(erased, erased);
    assert_eq!(nothing, Err(Absent::Nothing));
    // The newest copy holds a setting this kernel does not know, and a
    // usable older copy lies beside it: the older one is not taken.
    let mut odd = wifi_slot(KIND_WPA2, &x::NAME, &x::SECRET, 7, RECONNECT_AUTOMATIC, BOOT_BACKGROUND);
    odd[173] = 3;
    wifi_reseal(&mut odd);
    let older: &'static [u8] = Box::leak(wifi_slot(KIND_WPA2, &x::NAME, &x::SECRET, 6, RECONNECT_AUTOMATIC, BOOT_BACKGROUND).into_boxed_slice());
    assert!(crate::wifi_stored::read(older).is_ok(), "the older copy is one this kernel can use");
    let unknown = crate::wifi_stored::current(Box::leak(odd.into_boxed_slice()), older);
    assert_eq!(unknown, Err(Absent::Unknown));
    let on_request = wifi_exchange_record(KIND_WPA2, RECONNECT_AUTOMATIC, BOOT_ON_REQUEST);
    assert!(on_request.is_ok(), "a record that says to join on request is a valid record");
    let cases = [
        (nothing, Note::NothingStored, "[wifi] off: no network stored"),
        (unknown, Note::UnknownSetting, "[wifi] off: stored network has an unknown setting"),
        (on_request, Note::OnRequest, "[wifi] off: stored network is not set to join at start"),
    ];
    for (stored, note, line) in cases {
        let mut rig = wifi_rig(&[&x::OPENING[..], &x::WPA2[..]], stored, corner, 0);
        rig.idle(200);
        assert_eq!(rig.medium.stage(), Stage::Off, "{note:?}");
        assert_eq!((rig.left(), rig.medium.bus().fault(), rig.medium.counts().polls), (rig.rows, None, 0), "{note:?}: not one operation reached the transport");
        assert_eq!((rig.up(), rig.medium.hardware_address(), rig.medium.wait_us()), (false, None, None), "{note:?}");
        assert_eq!(rig.notes, [note]);
        assert_eq!(rig.lines(), [line]);
        assert!(rig.held().is_none() && !rig.room(), "{note:?}: nothing to take and no slot to fill");
    }
}

/// CONTROL: a corner that fails its check leaves the radio off, not one
/// operation reaching the transport: an erased corner, one flipped bit in
/// each image, an image the directory does not list, and a write that was
/// cut short behind a whole directory.
#[test]
fn wifi_medium_radio_stays_off_when_the_corner_fails_its_check() {
    use crate::board::net_medium::Medium;
    use crate::wifi_exchange as x;
    use crate::wifi_images::{Fault, Image, ROOM};
    use crate::wifi_medium::{Note, Stage};
    use crate::wifi_stored::{BOOT_BACKGROUND, KIND_WPA2, RECONNECT_AUTOMATIC};
    let (room, pins) = wifi_exchange_corner();
    let mut cases: Vec<(&str, Vec<u8>, Fault)> = vec![("an erased corner", vec![0xFFu8; ROOM], Fault::Magic)];
    for (image, at) in [(Image::Firmware, 0x1000 + 100), (Image::Regulatory, 0x3A000 + 1499), (Image::Settings, 0x3B000)] {
        let mut bad = room.to_vec();
        bad[at] ^= 1;
        cases.push(("one flipped bit in an image", bad, Fault::Content(image)));
    }
    cases.push(("no settings image listed", wifi_corner(&[(1, 0x1000, &x::FIRMWARE[..]), (2, 0x3A000, &x::REGULATORY[..])]), Fault::Missing(Image::Settings)));
    let mut cut = room.to_vec();
    cut[0x1000 + 100..0x1000 + 200].fill(0xFF);
    cases.push(("a write cut short behind a whole directory", cut, Fault::Content(Image::Firmware)));
    for (what, bad, fault) in cases {
        let bad: &'static [u8] = Box::leak(bad.into_boxed_slice());
        let mut rig = wifi_rig(&[&x::OPENING[..], &x::WPA2[..]], wifi_exchange_record(KIND_WPA2, RECONNECT_AUTOMATIC, BOOT_BACKGROUND), (bad, pins), 0);
        rig.idle(200);
        assert_eq!(rig.medium.stage(), Stage::Off, "{what}");
        assert_eq!((rig.left(), rig.medium.bus().fault(), rig.medium.counts().polls), (rig.rows, None, 0), "{what}: not one operation reached the transport");
        assert_eq!((rig.up(), rig.medium.hardware_address(), rig.medium.wait_us()), (false, None, None), "{what}");
        assert_eq!(rig.notes, [Note::FirmwareNotFound(fault)], "{what}");
        assert_eq!(rig.lines(), ["[wifi] off: radio firmware not found"]);
    }
}

/// CONTROL: a doctored exchange -- the firmware's version reply without
/// the pinned string -- ends in the driver's refusal by name: the medium
/// parked, the link down, the driver not polled again.
#[test]
fn wifi_medium_doctored_version_reply_parks_the_medium() {
    use crate::wifi_exchange as x;
    use crate::wifi_medium::{Note, Stage};
    use crate::wifi_stored::{BOOT_BACKGROUND, KIND_WPA2, RECONNECT_AUTOMATIC};
    let mut opening = x::OPENING.to_vec();
    let mut edits = 0;
    for op in opening.iter_mut() {
        if let WifiScript::F2Read { data } = *op {
            if let Some(at) = data.windows(15).position(|w| w == b"version 7.95.49") {
                let mut doctored = data.to_vec();
                doctored[at + 14] = b'8';
                *op = WifiScript::F2Read { data: Box::leak(doctored.into_boxed_slice()) };
                edits += 1;
            }
        }
    }
    assert_eq!(edits, 1, "the version reply is one row of the opening");
    let mut rig = wifi_rig(&[&opening[..], &x::WPA2[..]], wifi_exchange_record(KIND_WPA2, RECONNECT_AUTOMATIC, BOOT_BACKGROUND), wifi_exchange_corner(), 0);
    for _ in 0..200_000 {
        rig.service();
        match rig.medium.wait_us() {
            Some(wait) => rig.clock.set(rig.clock.get() + wait),
            None => break,
        }
    }
    assert_eq!((rig.medium.stage(), rig.up(), rig.medium.wait_us()), (Stage::Parked, false, None));
    let refusal = match rig.notes[..] {
        [Note::Starting, Note::Refused(refusal)] => refusal,
        _ => panic!("the notes {:?}", rig.notes),
    };
    assert_eq!(refusal.stage, lamella_wifi_cyw4343x::control::STAGE_VERSION_MISMATCH);
    assert_eq!(
        rig.lines()[1],
        format!("[wifi] off: radio did not start (stage firmware version mismatch, status 0x{:08X})", refusal.status)
    );
    assert_eq!(rig.medium.bus().fault(), None, "the driver refused; the transport saw nothing it did not expect");
    let (left, polls) = (rig.left(), rig.medium.counts().polls);
    assert!(left > x::WPA2.len(), "the opening's last rows were never asked for");
    rig.idle(50);
    assert_eq!((rig.left(), rig.medium.counts().polls, rig.notes.len()), (left, polls, 2), "parked: not polled again");
    assert!(rig.held().is_none() && !rig.room());
}

/// A lost link recovers by the driver -- it rests, scans and joins again by
/// itself, and frames pass again -- and a refused secret is rested and
/// tried again, whatever the record says of reconnection: under a record
/// set to reconnect on request both go as they go under one that says to
/// rejoin, by the same polls at the same instants with the same lines.
#[test]
fn wifi_medium_lost_link_and_refusal_go_the_same_way_whatever_the_record_says_of_reconnection() {
    use crate::wifi_exchange as x;
    use crate::wifi_medium::{Note, Stage};
    use crate::wifi_stored::{BOOT_BACKGROUND, KIND_WPA2, RECONNECT_AUTOMATIC, RECONNECT_MANUAL};
    let corner = wifi_exchange_corner();
    assert_eq!(wifi_exchange_record(KIND_WPA2, RECONNECT_MANUAL, BOOT_BACKGROUND).expect("a slot that reads the other setting is valid").reconnect, 1, "the reader still reads the setting");
    let lost = |reconnect: u8| {
        let mut rig = wifi_rig(&[&x::OPENING[..], &x::WPA2[..]], wifi_exchange_record(KIND_WPA2, reconnect, BOOT_BACKGROUND), corner, 0);
        rig.drive("the join", |r| r.up());
        rig.drive("the request", |r| r.held().is_some());
        rig.take();
        assert!(rig.send(&x::ARP_REPLY_OUT));
        rig.drive("the datagram", |r| r.held().is_some());
        rig.take();
        rig.drive("the loss", |r| !r.up());
        assert_eq!(rig.notes, [Note::Starting, Note::Joined, Note::LinkLost]);
        assert_eq!(rig.medium.stage(), Stage::Joining, "the driver keeps the network");
        assert!(!rig.send(&x::ARP_REPLY_OUT), "no slot for the stack while the link is down");
        let waits = rig.drive("the join again", |r| r.up());
        assert!(waits >= 100, "the driver's rest of a second, at its own cadence ({waits} waits)");
        assert_eq!(rig.notes, [Note::Starting, Note::Joined, Note::LinkLost, Note::Joined]);
        assert_eq!((rig.medium.counts().joined, rig.medium.counts().lost), (2, 1));
        rig.drive("the request again", |r| r.held().is_some());
        assert_eq!(rig.held(), Some(x::ARP_REQUEST_IN.to_vec()));
        rig.take();
        assert!(rig.send(&x::ARP_REPLY_OUT));
        rig.drive("the exchange's end", |r| r.left() == 0);
        assert_eq!((rig.medium.bus().fault(), rig.up(), rig.medium.counts().dropped), (None, true, 0));
        (rig.medium.counts().polls, rig.clock.get(), rig.lines())
    };
    assert_eq!(lost(RECONNECT_AUTOMATIC), lost(RECONNECT_MANUAL), "a lost link: the same polls, the same instants, the same lines");
    let refused = |reconnect: u8| {
        let mut rig = wifi_rig(&[&x::OPENING[..], &x::WPA2_REFUSED[..]], wifi_exchange_record(KIND_WPA2, reconnect, BOOT_BACKGROUND), corner, 0);
        rig.drive("the exchange's end", |r| r.left() == 0);
        assert_eq!((rig.medium.bus().fault(), rig.up(), rig.medium.counts().joined, rig.medium.counts().lost), (None, true, 2, 1));
        (rig.medium.counts().polls, rig.clock.get(), rig.lines())
    };
    assert_eq!(refused(RECONNECT_AUTOMATIC), refused(RECONNECT_MANUAL), "a refused secret: the same polls, the same instants, the same lines");
}

/// One line for a change of state, never one for a try: the stored network
/// is not there at the start, the first failed try says so once, the
/// driver's next try says nothing, and the join is said when it comes. The
/// other wording is chosen by what the driver's failure says.
#[test]
fn wifi_medium_first_failed_try_says_one_line() {
    use crate::wifi_exchange as x;
    use crate::wifi_medium::{Note, Stage};
    use crate::wifi_stored::{BOOT_BACKGROUND, KIND_WPA2, RECONNECT_AUTOMATIC};
    use lamella_wifi_cyw4343x::JoinFailure;
    let corner = wifi_exchange_corner();
    let mut rig = wifi_rig(&[&x::OPENING[..], &x::WPA2_NOT_FOUND[..]], wifi_exchange_record(KIND_WPA2, RECONNECT_AUTOMATIC, BOOT_BACKGROUND), corner, 0);
    rig.drive("the first failed try", |r| r.medium.counts().retried == 1);
    assert_eq!(rig.notes, [Note::Starting, Note::NotFound]);
    assert_eq!(rig.lines()[1], "[wifi] stored network not found; still looking");
    assert_eq!((rig.medium.stage(), rig.up()), (Stage::Joining, false), "the driver keeps the network");
    rig.drive("the second failed try", |r| r.medium.counts().retried == 2);
    assert_eq!(rig.notes, [Note::Starting, Note::NotFound], "a later try says nothing");
    rig.drive("the join", |r| r.up());
    assert_eq!(rig.notes, [Note::Starting, Note::NotFound, Note::Joined]);
    rig.drive("the exchange's end", |r| r.left() == 0);
    assert_eq!((rig.medium.bus().fault(), rig.up()), (None, true));
    assert_eq!((rig.medium.counts().retried, rig.medium.counts().joined, rig.medium.counts().lost), (2, 1, 0));
    // After a join the loss's line has said that the radio is looking: a
    // failed try behind it says nothing.
    let mut rig = wifi_rig(&[&x::OPENING[..], &x::WPA2_LOST_UNSEEN[..]], wifi_exchange_record(KIND_WPA2, RECONNECT_AUTOMATIC, BOOT_BACKGROUND), corner, 0);
    // In this exchange the loss follows the join with no wait between
    // them, so one service call sees both: the counters tell them.
    rig.drive("the join and the loss", |r| r.medium.counts().lost == 1);
    assert_eq!(rig.notes, [Note::Starting, Note::Joined, Note::LinkLost]);
    rig.drive("the failed try behind the loss", |r| r.medium.counts().retried == 1);
    assert_eq!((rig.notes.as_slice(), rig.up()), (&[Note::Starting, Note::Joined, Note::LinkLost][..], false), "a failed try behind a loss says nothing");
    rig.drive("the join again", |r| r.up());
    assert_eq!(rig.notes, [Note::Starting, Note::Joined, Note::LinkLost, Note::Joined]);
    rig.drive("the exchange's end", |r| r.left() == 0);
    assert_eq!((rig.medium.bus().fault(), rig.medium.counts().retried, rig.medium.counts().joined, rig.medium.counts().lost), (None, 1, 2, 1));
    // The wording by the failure: the firmware's own scan found no network
    // of the name, or the driver's scan did not see it; anything else the
    // driver tries again after is a try that did not complete.
    assert_eq!(Note::first_try(JoinFailure::Association { status: 3, assoc_status: 0, auth_status: 0 }), Note::NotFound);
    assert_eq!(Note::first_try(JoinFailure::NotFound), Note::NotFound);
    assert_eq!(Note::first_try(JoinFailure::Association { status: 1, assoc_status: 5, auth_status: 0 }), Note::FirstTryFailed);
    assert_eq!(Note::first_try(JoinFailure::Lost { number: 12, reason: 2 }), Note::FirstTryFailed);
    assert_eq!(Note::first_try(JoinFailure::Timeout), Note::FirstTryFailed);
    assert_eq!(wifi_line(&Note::FirstTryFailed), "[wifi] join did not complete; retrying");
}

/// A network that does not broadcast its name is asked for by its name.
/// Under the record the tool writes by default, the scan before the first
/// join hears other networks alone, the scan that carries the stored name
/// hears it offering WPA3, and the join is WPA3 at the first try: the
/// exchange consumed to its last row, and no line but the two of a start.
/// A scan that did not carry the name, or a join asked as WPA2, would not
/// match its exchange.
#[test]
fn wifi_medium_hidden_network_offering_wpa3_is_joined_as_wpa3_at_the_first_try() {
    use crate::wifi_exchange as x;
    use crate::wifi_medium::{Note, Stage};
    use crate::wifi_stored::{BOOT_BACKGROUND, KIND_WPA2_OR_WPA3, RECONNECT_AUTOMATIC};
    let corner = wifi_exchange_corner();
    let record = wifi_exchange_record(KIND_WPA2_OR_WPA3, RECONNECT_AUTOMATIC, BOOT_BACKGROUND);
    let mut rig = wifi_rig(&[&x::OPENING[..], &x::HIDDEN_WPA3[..]], record, corner, 0);
    rig.drive("the join", |r| r.up());
    assert_eq!(rig.notes, [Note::Starting, Note::Joined]);
    assert_eq!((rig.medium.stage(), rig.medium.counts().joined, rig.medium.counts().retried), (Stage::Joining, 1, 0), "joined at the first try");
    rig.drive("the exchange's end", |r| r.left() == 0);
    assert_eq!((rig.medium.bus().fault(), rig.up(), rig.medium.counts().retried), (None, true, 0));
}

/// A network that does not broadcast its name is found again after a lost
/// link: the driver's own scan does not hear it, its scan that asks for
/// the name does, and the join is entered again in the same cycle -- no
/// failed try between the loss and the join, and no line but theirs.
#[test]
fn wifi_medium_hidden_network_is_found_again_after_a_lost_link() {
    use crate::wifi_exchange as x;
    use crate::wifi_medium::{Note, Stage};
    use crate::wifi_stored::{BOOT_BACKGROUND, KIND_WPA2, RECONNECT_AUTOMATIC};
    let corner = wifi_exchange_corner();
    let mut rig = wifi_rig(&[&x::OPENING[..], &x::HIDDEN_LOST[..]], wifi_exchange_record(KIND_WPA2, RECONNECT_AUTOMATIC, BOOT_BACKGROUND), corner, 0);
    // In this exchange the loss follows the join with no wait between
    // them, so one service call sees both: the counters tell them.
    rig.drive("the join and the loss", |r| r.medium.counts().lost == 1);
    assert_eq!(rig.notes, [Note::Starting, Note::Joined, Note::LinkLost]);
    let waits = rig.drive("the join again", |r| r.up());
    assert!(waits >= 100, "the driver's rest of a second, at its own cadence ({waits} waits)");
    assert_eq!(rig.notes, [Note::Starting, Note::Joined, Note::LinkLost, Note::Joined]);
    assert_eq!((rig.medium.stage(), rig.medium.counts().retried, rig.medium.counts().joined, rig.medium.counts().lost), (Stage::Joining, 0, 2, 1), "found in the first cycle: no failed try");
    rig.drive("the exchange's end", |r| r.left() == 0);
    assert_eq!((rig.medium.bus().fault(), rig.up()), (None, true));
}

/// The second choice: under a record that accepts WPA2 or WPA3, a network
/// that was joined with one kind and is found, after a lost link, offering
/// only the other, is joined with the other kind -- in both directions,
/// by a whole join, and with no line between the loss's and the join's.
#[test]
fn wifi_medium_network_found_offering_only_the_other_kind_is_joined_with_it() {
    use crate::wifi_exchange as x;
    use crate::wifi_medium::{Note, Stage};
    use crate::wifi_stored::{BOOT_BACKGROUND, KIND_WPA2_OR_WPA3, RECONNECT_AUTOMATIC};
    let corner = wifi_exchange_corner();
    let record = wifi_exchange_record(KIND_WPA2_OR_WPA3, RECONNECT_AUTOMATIC, BOOT_BACKGROUND);
    for (what, table) in [("joined as WPA2, found offering WPA3 alone", &x::OTHER_TO_WPA3[..]), ("joined as WPA3, found offering WPA2 alone", &x::OTHER_TO_WPA2[..])] {
        let mut rig = wifi_rig(&[&x::OPENING[..], table], record, corner, 0);
        rig.drive(what, |r| r.medium.counts().lost == 1);
        assert_eq!(rig.notes, [Note::Starting, Note::Joined, Note::LinkLost], "{what}");
        rig.drive(what, |r| r.up());
        assert_eq!(rig.notes, [Note::Starting, Note::Joined, Note::LinkLost, Note::Joined], "{what}: no line between the loss's and the join's");
        assert_eq!((rig.medium.stage(), rig.medium.counts().joined, rig.medium.counts().lost, rig.medium.counts().retried), (Stage::Joining, 2, 1, 0), "{what}");
        rig.drive(what, |r| r.left() == 0);
        assert_eq!((rig.medium.bus().fault(), rig.up()), (None, true), "{what}: a join asked as the first kind again would not match its exchange");
    }
}

/// The three ends that are ends no more. Where the network found after a
/// lost link offers no kind that the record can use -- neither kind; the
/// other kind under a record that accepts one alone; WPA3 alone on a
/// firmware that does not run the WPA3 exchange -- the other kind is not
/// taken, the line is said once, and the medium asks the same join again:
/// the driver looks for the network as it does for one it does not see,
/// and joins it when it offers a kind the record can use.
#[test]
fn wifi_medium_network_that_offers_no_usable_kind_is_looked_for_and_joined_when_it_offers_one() {
    use crate::wifi_exchange as x;
    use crate::wifi_medium::{Note, Stage};
    use crate::wifi_stored::{BOOT_BACKGROUND, KIND_WPA2_OR_WPA3, KIND_WPA3, RECONNECT_AUTOMATIC};
    let corner = wifi_exchange_corner();
    let line = "[wifi] stored network found, security type does not match; still looking";
    let both = wifi_exchange_record(KIND_WPA2_OR_WPA3, RECONNECT_AUTOMATIC, BOOT_BACKGROUND);
    let one = wifi_exchange_record(KIND_WPA3, RECONNECT_AUTOMATIC, BOOT_BACKGROUND);
    // Two rounds of the looking, then a network that offers WPA2: the line
    // once, the same join asked twice, and the join that the driver enters.
    let mut rig = wifi_rig(&[&x::OPENING[..], &x::END_NEITHER[..]], both, corner, 0);
    rig.drive("the first round's end", |r| r.medium.counts().retried == 1);
    assert_eq!(rig.notes, [Note::Starting, Note::Joined, Note::LinkLost, Note::NoAcceptedKind]);
    assert_eq!(rig.lines()[3], line);
    assert_eq!((rig.medium.stage(), rig.up(), rig.medium.counts().failed), (Stage::Joining, false, [1, 3, 0, 0]), "the same join was asked again, and the firmware found no network it can join");
    rig.drive("the second round's end", |r| r.medium.counts().retried == 2);
    assert_eq!(rig.notes.len(), 4, "a later round says nothing");
    rig.drive("the exchange's end", |r| r.left() == 0);
    assert_eq!(rig.notes, [Note::Starting, Note::Joined, Note::LinkLost, Note::NoAcceptedKind, Note::Joined]);
    assert_eq!((rig.medium.bus().fault(), rig.up(), rig.medium.counts().joined, rig.medium.counts().lost, rig.medium.counts().retried), (None, true, 2, 1, 2));
    // The two other ends, through one round each.
    for (what, table, record) in [
        ("a record that accepts WPA3 alone, the network offering WPA2 alone", &x::END_ONE_KIND[..], one),
        ("WPA3 alone on a firmware without the exchange", &x::END_NO_EXCHANGE[..], both),
    ] {
        let mut rig = wifi_rig(&[&x::OPENING[..], table], record, corner, 0);
        rig.drive(what, |r| r.left() == 0);
        assert_eq!(rig.notes, [Note::Starting, Note::Joined, Note::LinkLost, Note::NoAcceptedKind], "{what}");
        assert_eq!(rig.lines()[3], line, "{what}");
        assert_eq!((rig.medium.stage(), rig.up(), rig.medium.bus().fault(), rig.medium.wait_us()), (Stage::Joining, false, None, Some(10_000)), "{what}: the join is asked again, and the driver rests at its own cadence");
        assert_eq!((rig.medium.counts().joined, rig.medium.counts().lost, rig.medium.counts().retried, rig.medium.counts().failed), (1, 1, 1, [1, 3, 0, 0]), "{what}");
    }
}

/// The fourth end, which is an end no more either. A record that accepts
/// WPA2 or WPA3 with a secret of five bytes -- a WPA3 password, and too
/// short to be a WPA2 passphrase -- is joined as WPA3; after a lost link
/// the network is found offering WPA2 alone, which the stored secret
/// cannot join. The other kind is not asked; the line of a network that
/// offers no usable kind is said, and the WPA3 join is asked again with
/// the same secret.
#[test]
fn wifi_medium_other_kind_is_not_taken_with_a_secret_that_cannot_be_its_credential() {
    use crate::wifi_exchange as x;
    use crate::wifi_medium::{Note, Stage};
    use crate::wifi_stored::{BOOT_BACKGROUND, KIND_WPA2_OR_WPA3, RECONNECT_AUTOMATIC};
    let corner = wifi_exchange_corner();
    let line = "[wifi] stored network found, security type does not match; still looking";
    assert!(lamella_wifi_cyw4343x::SaePassword::new(&x::SECRET_SHORT).is_some() && lamella_wifi_cyw4343x::Passphrase::new(&x::SECRET_SHORT).is_none(), "the short secret is a WPA3 password and no WPA2 passphrase");
    let slot: &'static [u8] = Box::leak(wifi_slot(KIND_WPA2_OR_WPA3, &x::NAME, &x::SECRET_SHORT, 7, RECONNECT_AUTOMATIC, BOOT_BACKGROUND).into_boxed_slice());
    let erased: &'static [u8] = Box::leak(vec![0xFFu8; crate::wifi_stored::UNIT].into_boxed_slice());
    let stored = crate::wifi_stored::current(slot, erased);
    assert!(stored.is_ok(), "a secret that fits one of the record's two kinds is no fault of the slot");
    let mut rig = wifi_rig(&[&x::OPENING[..], &x::END_NO_PASSPHRASE[..]], stored, corner, 0);
    rig.drive("the lost link", |r| r.medium.counts().lost == 1);
    assert_eq!(rig.notes, [Note::Starting, Note::Joined, Note::LinkLost], "joined as WPA3 with the short secret, then lost");
    rig.drive("the exchange's end", |r| r.left() == 0);
    assert_eq!(rig.notes, [Note::Starting, Note::Joined, Note::LinkLost, Note::NoAcceptedKind], "the other kind is not tried: the secret cannot be its credential");
    assert_eq!(rig.lines()[3], line);
    assert_eq!((rig.medium.stage(), rig.up(), rig.medium.bus().fault(), rig.medium.wait_us()), (Stage::Joining, false, None, Some(10_000)), "the WPA3 join is asked again with the same secret, and the driver rests at its own cadence");
    assert_eq!((rig.medium.counts().joined, rig.medium.counts().lost, rig.medium.counts().retried), (1, 1, 1));
}

/// A record that reads the older setting, join before the program starts,
/// starts as one that reads the default: the same rows asked for by the
/// same polls at the same instants, nothing held, and the same line.
#[test]
fn wifi_medium_older_boot_setting_starts_as_the_default() {
    use crate::wifi_exchange as x;
    use crate::wifi_medium::{Note, Stage};
    use crate::wifi_stored::{BOOT_BACKGROUND, BOOT_BEFORE_PROGRAM, KIND_WPA2, RECONNECT_AUTOMATIC};
    let corner = wifi_exchange_corner();
    let stored = wifi_exchange_record(KIND_WPA2, RECONNECT_AUTOMATIC, BOOT_BEFORE_PROGRAM).expect("a slot that reads the older setting is valid");
    assert_eq!(stored.boot, 1);
    let run = |boot: u8| {
        let mut rig = wifi_rig(&[&x::OPENING[..], &x::WPA2[..]], wifi_exchange_record(KIND_WPA2, RECONNECT_AUTOMATIC, boot), corner, 0);
        assert_eq!((rig.medium.stage(), rig.medium.wait_us()), (Stage::Corner, Some(0)), "the medium starts at its first service call");
        rig.drive("the join", |r| r.up());
        (rig.left(), rig.medium.counts().polls, rig.clock.get(), rig.notes.clone(), rig.lines())
    };
    let (default, older) = (run(BOOT_BACKGROUND), run(BOOT_BEFORE_PROGRAM));
    assert_eq!((default.0, default.1, default.2), (older.0, older.1, older.2), "the same rows, the same polls, the same instants");
    assert_eq!(default.3, [Note::Starting, Note::Joined]);
    assert_eq!(older.3, [Note::StartingOlderSetting, Note::Joined]);
    assert_eq!(older.4[0], "[wifi] starting; joining the stored network");
    assert_eq!(default.4[0], "[wifi] starting; joining the stored network");
}

/// A secret that the network refuses is tried again behind a rest that
/// doubles, with one line at each refusal that says when the next try
/// comes; nothing stops. Three handshakes time out in a row -- 5 s, 10 s
/// and 20 s -- and the fourth join completes; behind a lost link the
/// handshake times out again, and the rest is 5 s again.
#[test]
fn wifi_medium_refused_passphrase_is_tried_again_after_a_rest_that_doubles() {
    use crate::wifi_exchange as x;
    use crate::wifi_medium::{Note, Stage};
    use crate::wifi_stored::{BOOT_BACKGROUND, KIND_WPA2, RECONNECT_AUTOMATIC};
    let corner = wifi_exchange_corner();
    let mut rig = wifi_rig(&[&x::OPENING[..], &x::WPA2_REFUSED[..]], wifi_exchange_record(KIND_WPA2, RECONNECT_AUTOMATIC, BOOT_BACKGROUND), corner, 0);
    let mut began = Vec::new();
    for (round, again_s) in [5u32, 10, 20].into_iter().enumerate() {
        rig.drive("a refusal", |r| r.medium.counts().retried == round as u32 + 1);
        began.push(rig.clock.get());
        assert_eq!(rig.notes.last(), Some(&Note::SecretRefused { again_s }), "the refusal {}", round + 1);
        assert_eq!(rig.notes.len(), round + 2, "one line at each refusal, and no other");
        assert_eq!((rig.medium.stage(), rig.up(), rig.medium.counts().failed), (Stage::Joining, false, [2, 15, 0, 0]), "the driver keeps the network");
    }
    assert_eq!(rig.lines()[1], "[wifi] passphrase not accepted; retrying in 5 s");
    assert_eq!(rig.lines()[3], "[wifi] passphrase not accepted; retrying in 20 s");
    assert!(began[1] - began[0] >= 5_000_000 && began[2] - began[1] >= 10_000_000, "each try comes behind its rest");
    // In this exchange the loss follows the join with no wait between
    // them, so one service call sees both: the counters tell them.
    rig.drive("the join and the loss behind it", |r| r.medium.counts().lost == 1);
    assert!(rig.clock.get() - began[2] >= 20_000_000);
    assert_eq!((rig.notes[4], rig.notes[5], rig.medium.counts().joined), (Note::Joined, Note::LinkLost, 1));
    rig.drive("the refusal behind the lost link", |r| r.medium.counts().retried == 4);
    assert_eq!(
        rig.notes,
        [Note::Starting, Note::SecretRefused { again_s: 5 }, Note::SecretRefused { again_s: 10 }, Note::SecretRefused { again_s: 20 }, Note::Joined, Note::LinkLost, Note::SecretRefused { again_s: 5 }],
        "the join put the rest back"
    );
    rig.drive("the exchange's end", |r| r.left() == 0);
    assert_eq!((rig.medium.bus().fault(), rig.up(), rig.medium.stage()), (None, true, Stage::Joining));
    assert_eq!((rig.medium.counts().joined, rig.medium.counts().lost, rig.medium.counts().retried), (2, 1, 4));
    assert_eq!(rig.notes.last(), Some(&Note::Joined));
}

/// A WPA3 join that fails at its authentication is rested as a refused
/// secret is, under a line of its own that does not call it a refusal: no
/// sign of the driver tells a password that does not match from another
/// failed authentication. A failed attempt of another kind between two of
/// them says nothing and does not move the rests. The counters keep the
/// last failed attempt as four words for a row's witness.
#[test]
fn wifi_medium_failed_authentication_of_a_wpa3_join_says_a_line_of_its_own() {
    use crate::wifi_exchange as x;
    use crate::wifi_medium::{failed_words, Note, Stage};
    use crate::wifi_stored::{BOOT_BACKGROUND, KIND_WPA3, RECONNECT_AUTOMATIC};
    use lamella_wifi_cyw4343x::JoinFailure;
    let corner = wifi_exchange_corner();
    let mut rig = wifi_rig(&[&x::OPENING[..], &x::WPA3_REFUSED[..]], wifi_exchange_record(KIND_WPA3, RECONNECT_AUTOMATIC, BOOT_BACKGROUND), corner, 0);
    rig.drive("the first failed authentication", |r| r.medium.counts().retried == 1);
    assert_eq!(rig.notes, [Note::Starting, Note::AuthenticationFailed { again_s: 5 }]);
    assert_eq!(rig.lines()[1], "[wifi] authentication failed; retrying in 5 s");
    assert_eq!((rig.medium.stage(), rig.up(), rig.medium.counts().failed), (Stage::Joining, false, [1, 1, 0, 1]), "an association failure: its status, no association status, the authentication's status");
    rig.drive("the association that is not acknowledged", |r| r.medium.counts().retried == 2);
    assert_eq!(rig.notes.len(), 2, "a failed attempt that is no failed authentication says nothing behind the first line");
    assert_eq!(rig.medium.counts().failed, [1, 1, 5, 0], "the last failed attempt, not the first");
    rig.drive("the second failed authentication", |r| r.medium.counts().retried == 3);
    assert_eq!(rig.notes, [Note::Starting, Note::AuthenticationFailed { again_s: 5 }, Note::AuthenticationFailed { again_s: 10 }], "the second in the row");
    assert_eq!(rig.medium.counts().failed, [1, 1, 0, 1]);
    rig.drive("the exchange's end", |r| r.left() == 0);
    assert_eq!(rig.notes.last(), Some(&Note::Joined));
    assert_eq!((rig.medium.bus().fault(), rig.up(), rig.medium.counts().joined, rig.medium.counts().retried), (None, true, 1, 3));
    // The four words by the failure's class.
    assert_eq!(failed_words(JoinFailure::Association { status: 3, assoc_status: 0, auth_status: 0 }), [1, 3, 0, 0]);
    assert_eq!(failed_words(JoinFailure::Supplicant { reason: 15 }), [2, 15, 0, 0]);
    assert_eq!(failed_words(JoinFailure::Lost { number: 6, reason: 2 }), [3, 6, 2, 0]);
    assert_eq!(failed_words(JoinFailure::Timeout), [4, 0, 0, 0]);
    assert_eq!(failed_words(JoinFailure::NotFound), [5, 0, 0, 0]);
    assert_eq!(failed_words(JoinFailure::Mismatch { advertised: lamella_wifi_cyw4343x::Advertised::default() }), [6, 0, 0, 0]);
}

/// A network that is not seen between two refusals keeps its own rest and
/// says nothing, and the second refusal's rest is the second in the row.
#[test]
fn wifi_medium_network_not_seen_between_two_refusals_keeps_its_rest_and_says_nothing() {
    use crate::wifi_exchange as x;
    use crate::wifi_medium::{Note, Stage};
    use crate::wifi_stored::{BOOT_BACKGROUND, KIND_WPA2, RECONNECT_AUTOMATIC};
    let corner = wifi_exchange_corner();
    let mut rig = wifi_rig(&[&x::OPENING[..], &x::WPA2_REFUSED_UNSEEN[..]], wifi_exchange_record(KIND_WPA2, RECONNECT_AUTOMATIC, BOOT_BACKGROUND), corner, 0);
    rig.drive("the first refusal", |r| r.medium.counts().retried == 1);
    let refused_at = rig.clock.get();
    assert_eq!(rig.notes, [Note::Starting, Note::SecretRefused { again_s: 5 }]);
    rig.drive("the network not seen", |r| r.medium.counts().retried == 2);
    let unseen_at = rig.clock.get();
    assert_eq!(rig.notes.len(), 2, "a network not seen behind a refusal says nothing");
    assert_eq!((rig.medium.stage(), rig.up(), rig.medium.counts().failed), (Stage::Joining, false, [5, 0, 0, 0]));
    rig.drive("the second refusal", |r| r.medium.counts().retried == 3);
    let again_at = rig.clock.get();
    assert_eq!(rig.notes, [Note::Starting, Note::SecretRefused { again_s: 5 }, Note::SecretRefused { again_s: 10 }], "the count stood across the network not seen");
    assert!(unseen_at - refused_at >= 5_000_000 && again_at - unseen_at >= 5_000_000 && again_at - unseen_at < 10_000_000, "the network not seen rested its own five seconds");
    rig.drive("the exchange's end", |r| r.left() == 0);
    assert_eq!(rig.notes.last(), Some(&Note::Joined));
    assert_eq!((rig.medium.bus().fault(), rig.up(), rig.medium.counts().joined, rig.medium.counts().retried), (None, true, 1, 3));
}

/// The row's seed spreads the rests: under a seed the driver's rests are
/// longer by under a fifth, the line says each rounded up to a second, and
/// the next try comes at the first pass behind the spread rest.
#[test]
fn wifi_medium_seed_spreads_the_rests_and_the_line_says_them_rounded_up() {
    use crate::wifi_exchange as x;
    use crate::wifi_medium::{Note, Stage};
    use crate::wifi_stored::{BOOT_BACKGROUND, KIND_WPA2, RECONNECT_AUTOMATIC};
    let corner = wifi_exchange_corner();
    let mut rig = wifi_rig(&[&x::OPENING[..], &x::WPA2_REFUSED_SPREAD[..]], wifi_exchange_record(KIND_WPA2, RECONNECT_AUTOMATIC, BOOT_BACKGROUND), corner, 0);
    // The row gives the seed at the service call in which the radio
    // starts, behind the attach request.
    rig.drive("the radio's start", |r| r.medium.stage() == Stage::Attaching);
    rig.medium.seed(0x9E37_79B9);
    rig.drive("the first refusal", |r| r.medium.counts().retried == 1);
    let first = rig.clock.get();
    rig.drive("the second refusal", |r| r.medium.counts().retried == 2);
    let second = rig.clock.get();
    assert_eq!(rig.notes, [Note::Starting, Note::SecretRefused { again_s: 6 }, Note::SecretRefused { again_s: 12 }], "rests of 5316593 and 11751413 microseconds, each said rounded up");
    assert_eq!(second - first, 5_320_000, "the next try came at the first pass behind the first rest");
    rig.drive("the exchange's end", |r| r.left() == 0);
    assert_eq!(rig.notes.last(), Some(&Note::Joined));
    assert_eq!((rig.medium.bus().fault(), rig.up(), rig.medium.counts().joined), (None, true, 1));
}

/// A stored secret whose length the join's kind does not take ends the
/// trying before any join is asked, with a line of its own: the radio
/// stays up with no network, the link down. Five bytes under a record
/// that accepts WPA2 alone can be no passphrase.
#[test]
fn wifi_medium_secret_outside_its_kinds_bounds_ends_the_trying_with_a_line_of_its_own() {
    use crate::board::net_medium::Medium;
    use crate::wifi_exchange as x;
    use crate::wifi_medium::{Note, Stage};
    use crate::wifi_stored::{BOOT_BACKGROUND, KIND_WPA2, RECONNECT_AUTOMATIC};
    let corner = wifi_exchange_corner();
    let short: &'static [u8] = Box::leak(wifi_slot(KIND_WPA2, &x::NAME, b"short", 7, RECONNECT_AUTOMATIC, BOOT_BACKGROUND).into_boxed_slice());
    let erased: &'static [u8] = Box::leak(vec![0xFFu8; crate::wifi_stored::UNIT].into_boxed_slice());
    let stored = crate::wifi_stored::current(short, erased);
    assert!(stored.is_ok(), "a secret that does not fit its kind is no fault of the slot");
    let mut rig = wifi_rig(&[&x::OPENING[..], &x::UP_ALONE[..]], stored, corner, 0);
    rig.drive("the exchange's end", |r| r.left() == 0);
    assert_eq!(rig.notes, [Note::Starting, Note::SecretUnusable]);
    assert_eq!(rig.lines()[1], "[wifi] stopped: stored passphrase has an invalid length");
    assert_eq!((rig.medium.stage(), rig.up(), rig.medium.bus().fault(), rig.medium.hardware_address(), rig.medium.wait_us()), (Stage::Stopped, false, None, Some(x::STATION), Some(10_000)));
    assert_eq!((rig.medium.counts().retried, rig.medium.counts().failed), (0, [0; 4]), "no join was asked");
}

/// The service call stays inside its budget: on a clock that moves at
/// every reading, no call makes more polls than its 4 ms hold, the upload
/// spends a whole budget at least once and goes on at the next call; and
/// on the exact clock the driver is never polled before the instant it
/// named.
#[test]
fn wifi_medium_service_stays_inside_its_budget() {
    use crate::wifi_exchange as x;
    use crate::wifi_medium::{Stage, BUDGET_US};
    use crate::wifi_stored::{BOOT_BACKGROUND, KIND_WPA2, RECONNECT_AUTOMATIC};
    let corner = wifi_exchange_corner();
    let record = wifi_exchange_record(KIND_WPA2, RECONNECT_AUTOMATIC, BOOT_BACKGROUND);
    // Half a millisecond at every reading: a poll reads the clock twice
    // (the driver once, the medium once), so four polls spend the budget.
    let step = 500;
    let mut rig = wifi_rig(&[&x::OPENING[..], &x::WPA2[..]], record, corner, step);
    let (mut calls, mut spent, mut most) = (0u32, 0u32, 0u32);
    for _ in 0..100_000 {
        if !matches!(rig.medium.stage(), Stage::Corner | Stage::Checking | Stage::Attaching | Stage::Uploading | Stage::Opening) {
            break;
        }
        let (polls, began) = (rig.medium.counts().polls, rig.clock.get());
        rig.service();
        let (made, took) = (rig.medium.counts().polls - polls, rig.clock.get() - began);
        assert!(made <= 4, "a service call of {made} polls");
        assert!(took <= BUDGET_US + 2 * step + 1_000, "a service call of {took} us on the medium's clock");
        calls += 1;
        most = most.max(made);
        let wait = rig.medium.wait_us().expect("a medium on its ladder has a deadline");
        if made == 4 && wait == 0 {
            spent += 1;
        }
        rig.clock.set(rig.clock.get() + wait);
    }
    assert_eq!(rig.medium.bus().fault(), None);
    assert!(matches!(rig.medium.stage(), Stage::RadioUp | Stage::Joining), "the opening ended: {:?}", rig.medium.stage());
    assert!(spent >= 1 && most == 4, "the upload spends a whole budget ({spent} calls of {calls} did; the most polls in one {most})");
    assert!(u64::from(rig.medium.counts().service_max_us) <= BUDGET_US + 2 * step + 1_000);
    // On the exact clock: every wait of the ladder is the driver's own
    // instant, and a service call before it polls nothing.
    let mut rig = wifi_rig(&[&x::OPENING[..], &x::WPA2[..]], record, corner, 0);
    let waits = rig.drive("the join", |r| r.up());
    assert_eq!(waits, 18, "the ladder's waits to the join, each checked");
}

/// The hold keeps one frame and counts a drop: by itself, and through the
/// medium -- while a frame of the stack's stays staged the stack cannot
/// take the held frame, the medium polls on to get the staged frame
/// written, and a second frame that arrives meanwhile is dropped and
/// counted; once the chip took the staged frame the medium waits for the
/// stack to take the held one.
#[test]
fn wifi_medium_hold_keeps_one_frame_and_counts_a_drop() {
    use crate::board::net_medium::RxSide;
    use crate::wifi_exchange as x;
    use crate::wifi_medium::{Hold, HOLD};
    use crate::wifi_stored::{BOOT_BACKGROUND, KIND_WPA2, RECONNECT_AUTOMATIC};
    let mut hold = Hold::new();
    assert!(hold.peek().is_none() && !hold.is_full());
    assert!(hold.offer(&[1; 60]));
    assert!(!hold.offer(&[2; 60]), "a second frame finds the hold full");
    assert_eq!((hold.dropped(), hold.peek().map(|frame| frame.to_vec())), (1, Some(vec![1u8; 60])));
    hold.consume();
    assert!(hold.peek().is_none());
    assert!(hold.offer(&[3; 1514]), "the frame unit");
    hold.consume();
    assert!(hold.offer(&[4; HOLD]));
    hold.consume();
    assert!(!hold.offer(&[5; HOLD + 1]) && !hold.offer(&[]) && hold.peek().is_none());
    assert_eq!(hold.dropped(), 3);
    let mut rig = wifi_rig(&[&x::OPENING[..], &x::WPA2_HELD[..]], wifi_exchange_record(KIND_WPA2, RECONNECT_AUTOMATIC, BOOT_BACKGROUND), wifi_exchange_corner(), 0);
    rig.drive("the join", |r| r.up());
    // The stack's frame, which the chip does not take at its commit.
    assert!(rig.send(&x::ARP_REPLY_OUT));
    assert!(!rig.room(), "the frame stays staged");
    rig.drive("the first frame", |r| r.held().is_some());
    assert_eq!(rig.held(), Some(x::ARP_REQUEST_IN.to_vec()));
    assert!(!rig.room());
    rig.drive("the drop", |r| r.medium.counts().dropped != 0);
    assert_eq!((rig.medium.counts().dropped, rig.held()), (1, Some(x::ARP_REQUEST_IN.to_vec())), "the held frame is the first");
    rig.drive("the staged frame's write", |r| r.left() == 0);
    assert!(rig.room(), "the chip took the staged frame");
    let polls = rig.medium.counts().polls;
    rig.service();
    rig.service();
    assert_eq!((rig.medium.counts().polls, rig.medium.bus().fault()), (polls, None), "the stack takes the held frame first");
    rig.take();
    assert_eq!(rig.held(), None);
}

/// The program's end leaves the network and lowers the power line: joined,
/// the end asks the disassociation, its request and its reply are
/// consumed, and the row's closure is called once, behind the reply and
/// inside the bound. From there the medium is down -- the link down, no
/// room to send, nothing asked of the transport by any later service call
/// -- and the console's line says so. A second end does nothing.
#[test]
fn wifi_medium_program_end_leaves_the_network_and_lowers_the_power_line() {
    use crate::wifi_exchange as x;
    use crate::wifi_medium::{End, Note, Stage, END_BOUND_US};
    use crate::wifi_stored::{BOOT_BACKGROUND, KIND_WPA2, RECONNECT_AUTOMATIC};
    let mut rig = wifi_rig(&[&x::OPENING[..], &x::WPA2_LEFT[..]], wifi_exchange_record(KIND_WPA2, RECONNECT_AUTOMATIC, BOOT_BACKGROUND), wifi_exchange_corner(), 0);
    rig.drive("the join", |r| r.up());
    assert!(rig.room(), "joined, with room to send");
    assert!(rig.left() > 0, "the disassociation's rows are not asked for before the end");
    let (began, clock) = (rig.clock.get(), rig.clock);
    let mut lowered = Vec::new();
    let end = rig.medium.end(|bus| lowered.push((bus.remaining(), clock.get())));
    assert_eq!(lowered.len(), 1, "the power line is lowered once");
    let (left, at) = lowered[0];
    assert_eq!(left, 0, "the line is lowered behind the disassociation's request and its reply");
    assert!(at - began < END_BOUND_US, "the line is lowered inside the bound ({} us after the end began)", at - began);
    assert_eq!((rig.left(), rig.medium.bus().fault()), (0, None));
    assert_eq!(end, Some(End::Answered));
    assert_eq!((rig.medium.stage(), rig.up(), rig.room(), rig.medium.wait_us()), (Stage::Down, false, false, None));
    assert!(!rig.send(&x::ARP_REPLY_OUT), "no slot for the stack once the medium is down");
    let polls = rig.medium.counts().polls;
    rig.idle(50);
    assert_eq!((rig.medium.counts().polls, rig.left(), rig.medium.bus().fault(), rig.medium.stage()), (polls, 0, None, Stage::Down), "later service calls ask nothing of the transport");
    assert_eq!(rig.notes, [Note::Starting, Note::Joined, Note::Ended]);
    assert_eq!(rig.lines()[2], "[wifi] program ended; radio off");
    let mut again = 0;
    assert_eq!(rig.medium.end(|_| again += 1), None, "a second end finds the medium down");
    rig.idle(1);
    assert_eq!((again, rig.notes.len()), (0, 3), "and lowers nothing and says nothing");
}

/// The program's end lowers the power line at its bound when the
/// disconnect is not answered: the request is written, no reply comes, and
/// the row's closure is called once, not before the bound of 250 ms and no
/// later than one of the driver's cadences behind it. The medium is down
/// all the same and says its line.
#[test]
fn wifi_medium_program_end_lowers_the_power_line_at_its_bound_when_the_disconnect_is_not_answered() {
    use crate::wifi_exchange as x;
    use crate::wifi_medium::{End, Note, Stage, END_BOUND_US};
    use crate::wifi_stored::{BOOT_BACKGROUND, KIND_WPA2, RECONNECT_AUTOMATIC};
    assert_eq!(END_BOUND_US, 250_000, "25 of the driver's service cadences of 10 ms");
    let mut rig = wifi_rig(&[&x::OPENING[..], &x::WPA2_LEFT_UNANSWERED[..]], wifi_exchange_record(KIND_WPA2, RECONNECT_AUTOMATIC, BOOT_BACKGROUND), wifi_exchange_corner(), 0);
    rig.drive("the join", |r| r.up());
    let (began, clock, before) = (rig.clock.get(), rig.clock, rig.left());
    let mut lowered = Vec::new();
    let end = rig.medium.end(|bus| lowered.push((bus.remaining(), clock.get())));
    assert_eq!(end, Some(End::Unanswered));
    assert_eq!(lowered.len(), 1, "the power line is lowered once, whatever the disconnect answered");
    let (left, at) = lowered[0];
    assert!(left < before && rig.medium.bus().fault().is_none(), "the disassociation's request was written and the driver waited for its reply ({left} rows left of {before})");
    assert!(at - began >= END_BOUND_US && at - began <= END_BOUND_US + 10_000, "the line is lowered at the bound, no later than one cadence behind it ({} us after the end began)", at - began);
    assert!(left > 0 && rig.left() == left, "the empty passes beyond the bound are never asked for");
    assert_eq!((rig.medium.stage(), rig.up(), rig.room(), rig.medium.wait_us()), (Stage::Down, false, false, None));
    let polls = rig.medium.counts().polls;
    rig.idle(50);
    assert_eq!((rig.medium.counts().polls, rig.left(), rig.medium.bus().fault(), rig.medium.stage()), (polls, left, None, Stage::Down), "later service calls ask nothing of the transport");
    assert_eq!(rig.notes, [Note::Starting, Note::Joined, Note::Ended]);
    assert_eq!(rig.lines()[2], "[wifi] program ended; radio off");
}

/// The program's end asks no disconnect where no network is held. A medium
/// that stopped trying, and a parked one, are lowered at once with nothing
/// asked of the transport. One whose radio was never powered -- no record,
/// or an end during the corner's check -- lowers nothing, says no line and
/// powers nothing afterwards. And a medium that the row cut asks nothing of
/// the transport at its next service call, nor does an end behind the cut.
#[test]
fn wifi_medium_program_end_asks_no_disconnect_where_no_network_is_held() {
    use crate::wifi_exchange as x;
    use crate::wifi_medium::{End, Note, Stage};
    use crate::wifi_stored::{Absent, BOOT_BACKGROUND, KIND_WPA2, RECONNECT_AUTOMATIC};
    assert_eq!((End::Answered as u32, End::Unanswered as u32, End::NotAsked as u32, End::Cut as u32, Stage::Down as u32), (1, 2, 3, 4, 12), "the words a row's witness reads");
    let corner = wifi_exchange_corner();
    let record = wifi_exchange_record(KIND_WPA2, RECONNECT_AUTOMATIC, BOOT_BACKGROUND);
    // Stopped: the stored secret cannot be the passphrase of its kind, the
    // radio is up and holds no network.
    let short: &'static [u8] = Box::leak(wifi_slot(KIND_WPA2, &x::NAME, b"short", 7, RECONNECT_AUTOMATIC, BOOT_BACKGROUND).into_boxed_slice());
    let erased: &'static [u8] = Box::leak(vec![0xFFu8; crate::wifi_stored::UNIT].into_boxed_slice());
    let mut rig = wifi_rig(&[&x::OPENING[..], &x::UP_ALONE[..]], crate::wifi_stored::current(short, erased), corner, 0);
    rig.drive("the exchange's end", |r| r.left() == 0);
    assert_eq!(rig.medium.stage(), Stage::Stopped);
    let (began, clock) = (rig.clock.get(), rig.clock);
    let mut lowered = Vec::new();
    assert_eq!(rig.medium.end(|bus| lowered.push((bus.remaining(), clock.get()))), Some(End::NotAsked));
    assert_eq!(lowered, [(0usize, began)], "stopped: the line is lowered once and at once");
    rig.idle(50);
    assert_eq!((rig.medium.stage(), rig.up(), rig.medium.bus().fault(), rig.medium.wait_us()), (Stage::Down, false, None, None));
    assert_eq!(rig.notes, [Note::Starting, Note::SecretUnusable, Note::Ended]);
    // Parked: the driver refused at the firmware's version reply.
    let mut opening = x::OPENING.to_vec();
    let mut edits = 0;
    for op in opening.iter_mut() {
        if let WifiScript::F2Read { data } = *op {
            if let Some(at) = data.windows(15).position(|w| w == b"version 7.95.49") {
                let mut doctored = data.to_vec();
                doctored[at + 14] = b'8';
                *op = WifiScript::F2Read { data: Box::leak(doctored.into_boxed_slice()) };
                edits += 1;
            }
        }
    }
    assert_eq!(edits, 1, "the version reply is one row of the opening");
    let mut rig = wifi_rig(&[&opening[..], &x::WPA2[..]], record, corner, 0);
    for _ in 0..200_000 {
        rig.service();
        match rig.medium.wait_us() {
            Some(wait) => rig.clock.set(rig.clock.get() + wait),
            None => break,
        }
    }
    assert_eq!(rig.medium.stage(), Stage::Parked);
    let (began, clock, rows, polls) = (rig.clock.get(), rig.clock, rig.left(), rig.medium.counts().polls);
    let mut lowered = Vec::new();
    assert_eq!(rig.medium.end(|bus| lowered.push((bus.remaining(), clock.get()))), Some(End::NotAsked));
    assert_eq!(lowered, [(rows, began)], "parked: the line is lowered once and at once, with nothing asked of the transport");
    rig.idle(50);
    assert_eq!((rig.medium.stage(), rig.left(), rig.medium.counts().polls, rig.medium.bus().fault()), (Stage::Down, rows, polls, None));
    assert!(matches!(rig.notes[..], [Note::Starting, Note::Refused(_), Note::Ended]), "the notes {:?}", rig.notes);
    // Never powered: no network is stored.
    let erased: &'static [u8] = Box::leak(vec![0xFFu8; crate::wifi_stored::UNIT].into_boxed_slice());
    let nothing = crate::wifi_stored::current(erased, erased);
    assert_eq!(nothing, Err(Absent::Nothing));
    let mut rig = wifi_rig(&[&x::OPENING[..], &x::WPA2[..]], nothing, corner, 0);
    rig.idle(3);
    assert_eq!(rig.medium.stage(), Stage::Off);
    let mut calls = 0;
    assert_eq!(rig.medium.end(|_| calls += 1), Some(End::NotAsked));
    rig.idle(200);
    assert_eq!((calls, rig.medium.stage(), rig.left(), rig.medium.counts().polls, rig.medium.bus().fault()), (0, Stage::Down, rig.rows, 0, None), "no record: nothing is lowered and nothing reaches the transport");
    assert_eq!(rig.notes, [Note::NothingStored], "and no line is said for the end");
    // Never powered: the end comes while the corner is checked.
    let mut rig = wifi_rig(&[&x::OPENING[..], &x::WPA2[..]], record, corner, 0);
    rig.service();
    rig.service();
    assert_eq!(rig.medium.stage(), Stage::Checking);
    let mut calls = 0;
    assert_eq!(rig.medium.end(|_| calls += 1), Some(End::NotAsked));
    rig.idle(200);
    assert_eq!((calls, rig.medium.stage(), rig.left(), rig.medium.counts().polls, rig.medium.bus().fault()), (0, Stage::Down, rig.rows, 0, None), "an end during the check: nothing is lowered, and the check does not go on to power the radio");
    assert!(rig.notes.is_empty(), "and no line is said: {:?}", rig.notes);
    // Cut: joined, the row lowers the power by itself and tells the medium.
    let mut rig = wifi_rig(&[&x::OPENING[..], &x::WPA2_LEFT[..]], record, corner, 0);
    rig.drive("the join", |r| r.up());
    let (rows, polls) = (rig.left(), rig.medium.counts().polls);
    assert!(rows > 0 && rig.room());
    rig.medium.cut();
    assert_eq!((rig.medium.stage(), rig.up(), rig.room(), rig.medium.wait_us()), (Stage::Down, false, false, None));
    assert!(!rig.send(&x::ARP_REPLY_OUT), "no slot for the stack once the medium is cut");
    rig.idle(50);
    let mut calls = 0;
    assert_eq!(rig.medium.end(|_| calls += 1), None, "an end behind the cut finds the medium down");
    rig.idle(1);
    assert_eq!((calls, rig.left(), rig.medium.counts().polls, rig.medium.bus().fault()), (0, rows, polls, None), "cut: nothing is asked of the transport, by a service call or by an end");
    assert_eq!(rig.notes, [Note::Starting, Note::Joined], "and no line is said by the medium");
}

/// The chip's interrupt line polls the driver before its instant behind a
/// wait that named the line, and behind no other. Joined and waiting, with
/// a frame in the chip: a service call before the instant polls nothing by
/// itself, and with the line read high the frame is in the hold at that
/// call. A line read low asks nothing. A line read high with nothing to
/// read costs one poll, and behind that poll the row does not arm the line
/// again before the driver's instant, so a line that stays high cannot
/// keep the thread awake. Along a whole ladder the line is neither read
/// nor armed at a wait at an instant alone, and no such wait is cut short;
/// at every other wait the row arms the line. Where the driver is not
/// polled -- during the corner's check, behind a cut, behind the program's
/// end -- the line is neither read nor armed. And the service calls'
/// summed time holds every poll's.
#[test]
fn wifi_medium_interrupt_polls_before_the_instant_of_an_interrupt_wake_alone() {
    use crate::wifi_exchange as x;
    use crate::wifi_medium::{End, Stage};
    use crate::wifi_stored::{BOOT_BACKGROUND, KIND_WPA2, RECONNECT_AUTOMATIC};
    let corner = wifi_exchange_corner();
    let record = wifi_exchange_record(KIND_WPA2, RECONNECT_AUTOMATIC, BOOT_BACKGROUND);
    // The exchange, of today's rows: the WPA2 join, an empty pass, the ARP
    // request's arrival, three empty passes.
    let wpa2 = &x::WPA2[..];
    let read = wpa2
        .iter()
        .position(|op| matches!(op, WifiScript::F2Read { data } if data.windows(x::ARP_REQUEST_IN.len()).any(|w| w == &x::ARP_REQUEST_IN[..])))
        .expect("the request's read is a row of the exchange");
    let (join, arrival, empty) = (&wpa2[..read - 4], &wpa2[read - 4..=read], &wpa2[wpa2.len() - 2..]);
    assert!(matches!(arrival[0], WifiScript::TakeInterrupt { value: 0x0020 }), "the arrival begins at the chip's word that a frame is there");
    assert!(matches!(empty, [WifiScript::TakeInterrupt { value: 0x0000 }, WifiScript::F2Available { len: None }]), "an empty pass is the chip's word that none is");
    let mut rig = wifi_rig(&[&x::OPENING[..], join, empty, arrival, empty, empty, empty], record, corner, 0);
    let (mut reads, mut arms) = (0u32, 0u32);
    // During the corner's check the line is not read and not armed.
    rig.service();
    assert_eq!(rig.medium.stage(), Stage::Checking);
    assert_eq!((rig.medium.sample(|_| { reads += 1; true }), rig.medium.arm(|_| arms += 1), reads, arms), (None, false, 0, 0), "the radio is not powered yet");
    rig.drive("the join", |r| r.up());
    rig.drive("the wait in front of the arrival", |r| r.left() == arrival.len() + 3 * empty.len());
    assert_eq!((rig.medium.wait_us(), rig.held()), (Some(10_000), None), "joined and waiting for the driver's instant, the frame in the chip");
    // By itself a service call before the instant polls nothing.
    let (polls, rows, at) = (rig.medium.counts().polls, rig.left(), rig.clock.get());
    rig.service();
    assert_eq!((rig.medium.counts().polls, rig.left(), rig.held()), (polls, rows, None), "no poll before the instant without the line");
    assert!(rig.medium.arm(|_| arms += 1) && arms == 1, "the driver's last answer named the line: the row arms it across the wait");
    // With the line read high the frame is in the hold at that call.
    assert_eq!(rig.medium.sample(|_| { reads += 1; true }), Some(true));
    rig.service();
    assert_eq!((rig.held(), rig.left(), reads), (Some(x::ARP_REQUEST_IN.to_vec()), rows - arrival.len(), 1), "the frame is read at the call that found the line high");
    assert!(rig.clock.get() - at < 10_000, "and before the instant, {} us behind the wait's begin", rig.clock.get() - at);
    assert_eq!((rig.medium.wait_us(), rig.medium.sample(|_| { reads += 1; true }), rig.medium.arm(|_| arms += 1), reads, arms), (Some(0), None, false, 1, 1), "the stack takes the frame first: no wait, and the line neither read nor armed");
    // The stack takes the frame; the next call finds nothing more, and the
    // driver waits on the line again.
    rig.take();
    rig.service();
    let (polls, rows) = (rig.medium.counts().polls, rig.left());
    assert_eq!((rows, rig.medium.wait_us()), (2 * empty.len(), Some(10_000)));
    // A line read low asks nothing.
    assert_eq!(rig.medium.sample(|_| false), Some(false));
    rig.service();
    assert_eq!((rig.medium.counts().polls, rig.left()), (polls, rows), "a low line polls nothing before the instant");
    // A line read high with nothing to read: one poll, the driver's
    // instant stands, and the line is not armed behind it -- nor does it
    // ask for a second poll before the instant.
    assert!(rig.medium.arm(|_| arms += 1) && arms == 2);
    assert_eq!(rig.medium.sample(|_| true), Some(true));
    rig.service();
    assert_eq!((rig.medium.counts().polls, rig.left(), rig.medium.wait_us()), (polls + 1, rows - empty.len(), Some(10_000)), "one poll for the line, which read nothing; the driver's instant stands");
    assert_eq!((rig.medium.arm(|_| arms += 1), arms), (false, 2), "behind a poll that the line asked for and that read nothing, the line is not armed");
    assert_eq!((rig.medium.sample(|_| { reads += 1; true }), reads), (None, 1), "nor read again before the instant");
    rig.service();
    assert_eq!((rig.medium.counts().polls, rig.left()), (polls + 1, rows - empty.len()), "so a line that stays high gets no second poll before the instant");
    // At the instant the driver is polled as ever, and the line is armed
    // again.
    rig.clock.set(rig.clock.get() + 10_000);
    rig.service();
    assert_eq!((rig.medium.counts().polls, rig.left(), rig.medium.bus().fault()), (polls + 2, 0, None));
    assert!(rig.medium.arm(|_| arms += 1) && arms == 3, "behind the poll at the instant the line is armed again");
    // Behind a cut the line is neither read nor armed.
    rig.medium.cut();
    assert_eq!((rig.medium.sample(|_| { reads += 1; true }), rig.medium.arm(|_| arms += 1), reads, arms), (None, false, 1, 3), "cut: the driver is not polled, whatever its last answer was");
    // Nor behind the program's end.
    let mut rig = wifi_rig(&[&x::OPENING[..], &x::WPA2_LEFT[..]], record, corner, 0);
    rig.drive("the join", |r| r.up());
    rig.drive("the wait behind the join", |r| r.medium.wait_us() != Some(0));
    assert!(rig.medium.arm(|_| arms += 1) && arms == 4, "joined and waiting on the line");
    assert_eq!(rig.medium.end(|_| {}), Some(End::Answered));
    assert_eq!((rig.medium.sample(|_| { reads += 1; true }), rig.medium.arm(|_| arms += 1), reads, arms), (None, false, 1, 4), "ended: the line is neither read nor armed");
    // Along the whole ladder: at a wait the driver named the line and an
    // instant, or an instant alone. The row arms the line at the first
    // kind alone, and a high line cuts no wait of the second kind short.
    let mut rig = wifi_rig(&[&x::OPENING[..], &x::WPA2[..]], record, corner, 0);
    let (mut named, mut alone) = (0u32, 0u32);
    for _ in 0..200_000 {
        if rig.up() {
            break;
        }
        rig.service();
        let wait = rig.medium.wait_us().expect("a medium on its ladder has a deadline");
        if wait == 0 {
            continue;
        }
        if rig.medium.arm(|_| {}) {
            named += 1;
        } else {
            alone += 1;
            let polls = rig.medium.counts().polls;
            assert_eq!(rig.medium.sample(|_| true), None, "a wait at an instant alone: the line is not read");
            rig.service();
            assert_eq!((rig.medium.counts().polls, rig.medium.wait_us()), (polls, Some(wait)), "a wait at an instant alone, cut short");
        }
        rig.clock.set(rig.clock.get() + wait);
    }
    assert_eq!((rig.up(), rig.medium.bus().fault()), (true, None));
    assert!(named > 0 && alone > 0, "the ladder waits both ways: {named} waits on the line, {alone} at an instant alone");
    // The summed time: on a clock that moves at every reading a poll costs
    // two readings at least, the driver's and the medium's.
    let step = 500;
    let mut rig = wifi_rig(&[&x::OPENING[..], &x::WPA2[..]], record, corner, step);
    for _ in 0..100_000 {
        if !matches!(rig.medium.stage(), Stage::Corner | Stage::Checking | Stage::Attaching | Stage::Uploading | Stage::Opening) {
            break;
        }
        rig.service();
        let wait = rig.medium.wait_us().expect("a medium on its ladder has a deadline");
        rig.clock.set(rig.clock.get() + wait);
    }
    let counts = rig.medium.counts();
    assert_eq!(rig.medium.bus().fault(), None);
    assert!(counts.polls > 0 && u64::from(counts.service_us) >= 2 * step * u64::from(counts.polls), "{} us summed over {} polls", counts.service_us, counts.polls);
    assert!(counts.service_us >= counts.service_max_us && u64::from(counts.service_us) <= rig.clock.get(), "{} us summed, the longest call {} us, the clock at {} us", counts.service_us, counts.service_max_us, rig.clock.get());
}
