//! IEEE-754 binary64 arithmetic with the Arm floating-point extension's
//! rules, in integer code only: no floating-point instruction, no
//! floating-point type. The kernel's double-precision emulation computes
//! with it after the fault funnel catches a double-precision form on a
//! single-precision FPU (the M33 class), and the host tier tests it on the
//! desk machine. Every operation takes the context the frame's FPSCR
//! describes -- the rounding mode, default-NaN, flush-to-zero and the
//! alternative half-precision format -- and accumulates the cumulative
//! exception flags the way the hardware does (invalid, division by zero,
//! overflow, underflow, inexact, input denormal). Values travel as their
//! bit patterns (`u64` for doubles, `u32` for singles and for the
//! half-precision word).

#![allow(clippy::many_single_char_names)]

/// FPSCR bits (the frame's word; the cumulative flags are the low bits).
pub const FPSCR_N: u32 = 1 << 31;
pub const FPSCR_Z: u32 = 1 << 30;
pub const FPSCR_C: u32 = 1 << 29;
pub const FPSCR_V: u32 = 1 << 28;
pub const FPSCR_AHP: u32 = 1 << 26;
pub const FPSCR_DN: u32 = 1 << 25;
pub const FPSCR_FZ: u32 = 1 << 24;
pub const FPSCR_RMODE_SHIFT: u32 = 22;
pub const IOC: u32 = 1 << 0;
pub const DZC: u32 = 1 << 1;
pub const OFC: u32 = 1 << 2;
pub const UFC: u32 = 1 << 3;
pub const IXC: u32 = 1 << 4;
pub const IDC: u32 = 1 << 7;

/// A rounding direction. `Nearest` is ties-to-even; `Away` is ties-away
/// from zero (the A-suffixed forms only).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Round {
    Nearest,
    Plus,
    Minus,
    Zero,
    Away,
}

/// The context an operation runs under and the flags it raised.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Ctx {
    pub round: Round,
    pub dn: bool,
    pub fz: bool,
    pub ahp: bool,
    pub flags: u32,
}

impl Ctx {
    /// The context the FPSCR word describes, with no flags raised yet.
    pub fn from_fpscr(w: u32) -> Ctx {
        let round = match (w >> FPSCR_RMODE_SHIFT) & 3 {
            0 => Round::Nearest,
            1 => Round::Plus,
            2 => Round::Minus,
            _ => Round::Zero,
        };
        Ctx { round, dn: w & FPSCR_DN != 0, fz: w & FPSCR_FZ != 0, ahp: w & FPSCR_AHP != 0, flags: 0 }
    }

    /// The FPSCR word with this context's flags merged in (the cumulative bits only).
    pub fn merge_into(&self, w: u32) -> u32 {
        w | (self.flags & (IOC | DZC | OFC | UFC | IXC | IDC))
    }
}


/// A binary interchange format: the significand width including the
/// implicit bit, the exponent bias, the width of the whole word.
#[derive(Clone, Copy)]
struct Fmt {
    mant_bits: u32,
    exp_bits: u32,
    bias: i32,
}

const BIN64: Fmt = Fmt { mant_bits: 53, exp_bits: 11, bias: 1023 };
const BIN32: Fmt = Fmt { mant_bits: 24, exp_bits: 8, bias: 127 };
const BIN16: Fmt = Fmt { mant_bits: 11, exp_bits: 5, bias: 15 };

impl Fmt {
    fn width(&self) -> u32 {
        1 + self.exp_bits + self.mant_bits - 1
    }
    fn frac_mask(&self) -> u64 {
        (1u64 << (self.mant_bits - 1)) - 1
    }
    fn exp_max(&self) -> i32 {
        (1i32 << self.exp_bits) - 1
    }
    fn sign_bit(&self) -> u64 {
        1u64 << (self.width() - 1)
    }
    fn quiet_bit(&self) -> u64 {
        1u64 << (self.mant_bits - 2)
    }
    fn default_nan(&self) -> u64 {
        ((self.exp_max() as u64) << (self.mant_bits - 1)) | self.quiet_bit()
    }
    fn inf(&self, sign: bool) -> u64 {
        ((self.exp_max() as u64) << (self.mant_bits - 1)) | if sign { self.sign_bit() } else { 0 }
    }
    fn zero(&self, sign: bool) -> u64 {
        if sign { self.sign_bit() } else { 0 }
    }
    fn max_finite(&self, sign: bool) -> u64 {
        (((self.exp_max() - 1) as u64) << (self.mant_bits - 1)) | self.frac_mask() | if sign { self.sign_bit() } else { 0 }
    }
    /// The smallest exponent of a normal number, unbiased.
    fn emin(&self) -> i32 {
        1 - self.bias
    }
}

/// The class of a value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Class {
    Zero,
    Denormal,
    Normal,
    Inf,
    QNan,
    SNan,
}

/// An unpacked finite value: `mant * 2^exp`, the significand an integer
/// (normals carry the implicit bit; denormals a smaller integer; zeros 0).
#[derive(Clone, Copy, Debug)]
struct Unp {
    sign: bool,
    exp: i32,
    mant: u64,
}

fn classify_in(fmt: Fmt, x: u64) -> Class {
    let exp = ((x >> (fmt.mant_bits - 1)) & ((1u64 << fmt.exp_bits) - 1)) as i32;
    let frac = x & fmt.frac_mask();
    if exp == fmt.exp_max() {
        if frac == 0 {
            Class::Inf
        } else if frac & fmt.quiet_bit() != 0 {
            Class::QNan
        } else {
            Class::SNan
        }
    } else if exp == 0 {
        if frac == 0 { Class::Zero } else { Class::Denormal }
    } else {
        Class::Normal
    }
}

/// The class of a double.
pub fn classify(x: u64) -> Class {
    classify_in(BIN64, x)
}

fn sign_in(fmt: Fmt, x: u64) -> bool {
    x & fmt.sign_bit() != 0
}

/// Unpack a finite value of the format; a denormal input under FZ is
/// flushed to zero (its sign kept) with IDC raised, as the hardware's
/// unpack does for the double and single formats (the half-precision
/// callers pass a context with the flush off). The class returned is the
/// class AFTER the flush.
fn unpack_in(fmt: Fmt, ctx: &mut Ctx, x: u64) -> (Class, Unp) {
    let sign = sign_in(fmt, x);
    let exp = ((x >> (fmt.mant_bits - 1)) & ((1u64 << fmt.exp_bits) - 1)) as i32;
    let frac = x & fmt.frac_mask();
    let class = classify_in(fmt, x);
    match class {
        Class::Zero => (Class::Zero, Unp { sign, exp: 0, mant: 0 }),
        Class::Denormal => {
            if ctx.fz {
                ctx.flags |= IDC;
                (Class::Zero, Unp { sign, exp: 0, mant: 0 })
            } else {
                (Class::Denormal, Unp { sign, exp: fmt.emin() - (fmt.mant_bits as i32 - 1), mant: frac })
            }
        }
        Class::Normal => (
            Class::Normal,
            Unp { sign, exp: exp - fmt.bias - (fmt.mant_bits as i32 - 1), mant: frac | (1u64 << (fmt.mant_bits - 1)) },
        ),
        other => (other, Unp { sign, exp: 0, mant: 0 }),
    }
}

/// The flush-to-zero of denormal inputs, noted for every operand of an
/// operation before the NaN rule decides it: the hardware unpacks each
/// operand first, so a denormal beside a NaN still raises IDC.
fn note_denormals(ctx: &mut Ctx, ops: &[u64]) {
    if ctx.fz && ops.iter().any(|&x| classify(x) == Class::Denormal) {
        ctx.flags |= IDC;
    }
}

/// Quiet a NaN of the format (the quiet bit set), or the default NaN under DN.
fn quiet_in(fmt: Fmt, ctx: &Ctx, x: u64) -> u64 {
    if ctx.dn { fmt.default_nan() } else { x | fmt.quiet_bit() }
}

/// The architecture's two-operand NaN rule: a signalling NaN in either
/// operand wins (the first one, quieted, IOC raised); else the first quiet
/// NaN; else no NaN. Returns the result when a NaN decides the operation.
fn process_nans_in(fmt: Fmt, ctx: &mut Ctx, a: u64, b: u64) -> Option<u64> {
    let ca = classify_in(fmt, a);
    let cb = classify_in(fmt, b);
    if ca == Class::SNan {
        ctx.flags |= IOC;
        Some(quiet_in(fmt, ctx, a))
    } else if cb == Class::SNan {
        ctx.flags |= IOC;
        Some(quiet_in(fmt, ctx, b))
    } else if ca == Class::QNan {
        Some(quiet_in(fmt, ctx, a))
    } else if cb == Class::QNan {
        Some(quiet_in(fmt, ctx, b))
    } else {
        None
    }
}

/// The one-operand NaN rule.
fn process_nan_in(fmt: Fmt, ctx: &mut Ctx, a: u64) -> Option<u64> {
    match classify_in(fmt, a) {
        Class::SNan => {
            ctx.flags |= IOC;
            Some(quiet_in(fmt, ctx, a))
        }
        Class::QNan => Some(quiet_in(fmt, ctx, a)),
        _ => None,
    }
}


/// Round and pack `sign * mant * 2^exp` (plus a sticky bit meaning
/// "nonzero bits below `mant`") into the format under the context's
/// rounding direction, raising overflow, underflow and inexact as the
/// architecture does (tininess before rounding; flush-to-zero honoured on
/// the result).
fn round_pack_in(fmt: Fmt, ctx: &mut Ctx, sign: bool, exp: i32, mant: u128, sticky: bool) -> u64 {
    if mant == 0 {
        if sticky {
            return tiny_in(fmt, ctx, sign, exp, mant, sticky);
        }
        return fmt.zero(sign);
    }
    let bits = 128 - mant.leading_zeros() as i32;
    let lead_exp = exp + bits - 1;
    if lead_exp < fmt.emin() {
        return tiny_in(fmt, ctx, sign, exp, mant, sticky);
    }
    let mb = fmt.mant_bits as i32;
    let (mut m, round, stick) = if bits > mb {
        let shift = (bits - mb) as u32;
        let lost = mant & ((1u128 << shift) - 1);
        let round = (lost >> (shift - 1)) & 1 != 0;
        let below = lost & ((1u128 << (shift - 1)) - 1) != 0;
        ((mant >> shift) as u64, round, below || sticky)
    } else {
        ((mant << (mb - bits) as u32) as u64, false, sticky)
    };
    let mut e = lead_exp;
    let inexact = round || stick;
    if inexact {
        ctx.flags |= IXC;
        if increment(ctx.round, sign, m & 1 != 0, round, stick) {
            m += 1;
            if m == 1u64 << mb {
                m >>= 1;
                e += 1;
            }
        }
    }
    if e > fmt.bias {
        ctx.flags |= OFC | IXC;
        return overflow_in(fmt, ctx.round, sign);
    }
    pack_in(fmt, sign, e, m)
}

/// Whether the rounding direction adds one unit to the truncated significand.
fn increment(round: Round, sign: bool, lsb: bool, round_bit: bool, sticky: bool) -> bool {
    match round {
        Round::Nearest => round_bit && (sticky || lsb),
        Round::Away => round_bit,
        Round::Plus => !sign && (round_bit || sticky),
        Round::Minus => sign && (round_bit || sticky),
        Round::Zero => false,
    }
}

/// The overflow result per direction.
fn overflow_in(fmt: Fmt, round: Round, sign: bool) -> u64 {
    match round {
        Round::Nearest | Round::Away => fmt.inf(sign),
        Round::Zero => fmt.max_finite(sign),
        Round::Plus => if sign { fmt.max_finite(true) } else { fmt.inf(false) },
        Round::Minus => if sign { fmt.inf(true) } else { fmt.max_finite(false) },
    }
}

/// A value tiny before rounding: flushed under FZ (underflow raised, the
/// result a signed zero), else denormalized and rounded in units of the
/// smallest denormal, underflow and inexact raised together when bits are
/// lost (a result that rounds up to the smallest normal keeps the flags).
fn tiny_in(fmt: Fmt, ctx: &mut Ctx, sign: bool, exp: i32, mant: u128, sticky: bool) -> u64 {
    if ctx.fz {
        ctx.flags |= UFC;
        return fmt.zero(sign);
    }
    let unit = fmt.emin() - (fmt.mant_bits as i32 - 1);
    let shift = unit - exp;
    let (mut m, round, stick) = if shift <= 0 {
        let s = (-shift) as u32;
        ((mant << s) as u64, false, sticky)
    } else if shift >= 128 {
        (0u64, false, mant != 0 || sticky)
    } else {
        let s = shift as u32;
        let lost = mant & ((1u128 << s) - 1);
        let round = (lost >> (s - 1)) & 1 != 0;
        let below = lost & ((1u128 << (s - 1)) - 1) != 0;
        ((mant >> s) as u64, round, below || sticky)
    };
    let inexact = round || stick;
    if inexact {
        ctx.flags |= UFC | IXC;
        if increment(ctx.round, sign, m & 1 != 0, round, stick) {
            m += 1;
        }
    }
    let frac = m & fmt.frac_mask();
    let expfield: u64 = if m >> (fmt.mant_bits - 1) != 0 { 1 } else { 0 };
    (expfield << (fmt.mant_bits - 1)) | frac | if sign { fmt.sign_bit() } else { 0 }
}

/// Pack a normal number: `m` carries the implicit bit, `e` is the unbiased exponent of its leading bit.
fn pack_in(fmt: Fmt, sign: bool, e: i32, m: u64) -> u64 {
    let expfield = (e + fmt.bias) as u64;
    (expfield << (fmt.mant_bits - 1)) | (m & fmt.frac_mask()) | if sign { fmt.sign_bit() } else { 0 }
}


#[derive(Clone, Copy, PartialEq, Eq)]
struct U256 {
    hi: u128,
    lo: u128,
}

impl U256 {
    const ZERO: U256 = U256 { hi: 0, lo: 0 };
    fn from_u128(x: u128) -> U256 {
        U256 { hi: 0, lo: x }
    }
    fn is_zero(&self) -> bool {
        self.hi == 0 && self.lo == 0
    }
    fn shl(self, n: u32) -> U256 {
        if n == 0 {
            self
        } else if n >= 256 {
            U256::ZERO
        } else if n >= 128 {
            U256 { hi: self.lo << (n - 128), lo: 0 }
        } else {
            U256 { hi: (self.hi << n) | (self.lo >> (128 - n)), lo: self.lo << n }
        }
    }
    /// Shift right, returning the shifted value and whether any set bit was lost.
    fn shr_sticky(self, n: u32) -> (U256, bool) {
        if n == 0 {
            (self, false)
        } else if n >= 256 {
            (U256::ZERO, !self.is_zero())
        } else if n >= 128 {
            let k = n - 128;
            let lost = self.lo != 0 || (k > 0 && self.hi & ((1u128 << k) - 1) != 0);
            (U256 { hi: 0, lo: self.hi >> k }, lost)
        } else {
            let lost = self.lo & ((1u128 << n) - 1) != 0;
            (U256 { hi: self.hi >> n, lo: (self.lo >> n) | (self.hi << (128 - n)) }, lost)
        }
    }
    fn add(self, o: U256) -> U256 {
        let (lo, c) = self.lo.overflowing_add(o.lo);
        U256 { hi: self.hi.wrapping_add(o.hi).wrapping_add(c as u128), lo }
    }
    /// self - o, requiring self >= o.
    fn sub(self, o: U256) -> U256 {
        let (lo, b) = self.lo.overflowing_sub(o.lo);
        U256 { hi: self.hi.wrapping_sub(o.hi).wrapping_sub(b as u128), lo }
    }
    fn cmp(self, o: U256) -> core::cmp::Ordering {
        (self.hi, self.lo).cmp(&(o.hi, o.lo))
    }
    /// Reduce to a 128-bit significand and a sticky bit, shifting right as needed; returns (mant, exp_adjust, sticky).
    fn to_u128_sticky(self) -> (u128, i32, bool) {
        if self.hi == 0 {
            (self.lo, 0, false)
        } else {
            let n = 128 - self.hi.leading_zeros();
            let (v, lost) = self.shr_sticky(n);
            (v.lo, n as i32, lost)
        }
    }
}


/// Add two doubles.
pub fn add(ctx: &mut Ctx, a: u64, b: u64) -> u64 {
    note_denormals(ctx, &[a, b]);
    if let Some(r) = process_nans_in(BIN64, ctx, a, b) {
        return r;
    }
    let (ca, ua) = unpack_in(BIN64, ctx, a);
    let (cb, ub) = unpack_in(BIN64, ctx, b);
    match (ca, cb) {
        (Class::Inf, Class::Inf) => {
            if ua.sign == ub.sign {
                BIN64.inf(ua.sign)
            } else {
                ctx.flags |= IOC;
                BIN64.default_nan()
            }
        }
        (Class::Inf, _) => BIN64.inf(ua.sign),
        (_, Class::Inf) => BIN64.inf(ub.sign),
        (Class::Zero, Class::Zero) => {
            if ua.sign == ub.sign { BIN64.zero(ua.sign) } else { BIN64.zero(ctx.round == Round::Minus) }
        }
        (Class::Zero, _) => pack_unp(ctx, ub),
        (_, Class::Zero) => pack_unp(ctx, ua),
        _ => {
            let mut c2 = *ctx;
            let r = add_finite_flags(&mut c2, ua, ub);
            ctx.flags |= c2.flags;
            r
        }
    }
}

/// The exact sum of two finite nonzero values, then one rounding. The
/// significands are widened by 64 bits and aligned on the larger exponent;
/// the bits the smaller operand loses below that width are a sticky bit.
/// When the operands subtract, a sticky on the smaller operand means the
/// true magnitude is a fraction of a unit BELOW the integer difference, so
/// one unit is borrowed and the sticky carries the rest; when the two align
/// equal and the smaller lost bits, the true result is a fraction of a unit
/// on the SMALLER operand's side.
fn add_finite_flags(ctx: &mut Ctx, a: Unp, b: Unp) -> u64 {
    let (big, small) = if a.exp >= b.exp { (a, b) } else { (b, a) };
    let mb_ = U256::from_u128((big.mant as u128) << 64);
    let ms = U256::from_u128((small.mant as u128) << 64);
    let gap = (big.exp - small.exp) as u32;
    let (ms_aligned, lost) = ms.shr_sticky(gap);
    let exp = big.exp - 64;
    let (sign, mag, sticky) = if a.sign == b.sign {
        (a.sign, mb_.add(ms_aligned), lost)
    } else {
        match mb_.cmp(ms_aligned) {
            core::cmp::Ordering::Equal => {
                if lost {
                    (small.sign, U256::ZERO, true)
                } else {
                    return BIN64.zero(ctx.round == Round::Minus);
                }
            }
            core::cmp::Ordering::Greater => {
                let d = mb_.sub(ms_aligned);
                if lost { (big.sign, d.sub(U256::from_u128(1)), true) } else { (big.sign, d, false) }
            }
            core::cmp::Ordering::Less => (small.sign, ms_aligned.sub(mb_), lost),
        }
    };
    let (m128, adj, more) = mag.to_u128_sticky();
    round_pack_in(BIN64, ctx, sign, exp + adj, m128, sticky || more)
}

/// Pack a finite unpacked value exactly (a zero, denormal or normal that came from a double).
fn pack_unp(ctx: &mut Ctx, u: Unp) -> u64 {
    if u.mant == 0 {
        return BIN64.zero(u.sign);
    }
    round_pack_in(BIN64, ctx, u.sign, u.exp, u.mant as u128, false)
}

/// Subtract two doubles.
pub fn sub(ctx: &mut Ctx, a: u64, b: u64) -> u64 {
    note_denormals(ctx, &[a, b]);
    if let Some(r) = process_nans_in(BIN64, ctx, a, b) {
        return r;
    }
    add(ctx, a, b ^ BIN64.sign_bit())
}

/// Multiply two doubles.
pub fn mul(ctx: &mut Ctx, a: u64, b: u64) -> u64 {
    note_denormals(ctx, &[a, b]);
    if let Some(r) = process_nans_in(BIN64, ctx, a, b) {
        return r;
    }
    let (ca, ua) = unpack_in(BIN64, ctx, a);
    let (cb, ub) = unpack_in(BIN64, ctx, b);
    let sign = ua.sign ^ ub.sign;
    match (ca, cb) {
        (Class::Inf, Class::Zero) | (Class::Zero, Class::Inf) => {
            ctx.flags |= IOC;
            BIN64.default_nan()
        }
        (Class::Inf, _) | (_, Class::Inf) => BIN64.inf(sign),
        (Class::Zero, _) | (_, Class::Zero) => BIN64.zero(sign),
        _ => {
            let prod = (ua.mant as u128) * (ub.mant as u128);
            round_pack_in(BIN64, ctx, sign, ua.exp + ub.exp, prod, false)
        }
    }
}

/// Divide two doubles.
pub fn div(ctx: &mut Ctx, a: u64, b: u64) -> u64 {
    note_denormals(ctx, &[a, b]);
    if let Some(r) = process_nans_in(BIN64, ctx, a, b) {
        return r;
    }
    let (ca, ua) = unpack_in(BIN64, ctx, a);
    let (cb, ub) = unpack_in(BIN64, ctx, b);
    let sign = ua.sign ^ ub.sign;
    match (ca, cb) {
        (Class::Inf, Class::Inf) | (Class::Zero, Class::Zero) => {
            ctx.flags |= IOC;
            BIN64.default_nan()
        }
        (Class::Inf, _) => BIN64.inf(sign),
        (_, Class::Inf) => BIN64.zero(sign),
        (_, Class::Zero) => {
            ctx.flags |= DZC;
            BIN64.inf(sign)
        }
        (Class::Zero, _) => BIN64.zero(sign),
        _ => {
            let nbits = 64 - ua.mant.leading_zeros() as i32;
            let shift = (117 - nbits) as u32;
            let num = (ua.mant as u128) << shift;
            let den = ub.mant as u128;
            let q = num / den;
            let r = num % den;
            round_pack_in(BIN64, ctx, sign, ua.exp - ub.exp - shift as i32, q, r != 0)
        }
    }
}

/// The integer square root of a 128-bit value, with whether a remainder was left.
fn isqrt128(v: u128) -> (u128, bool) {
    if v == 0 {
        return (0, false);
    }
    let bits = 128 - v.leading_zeros();
    let mut x: u128 = 1u128 << ((bits + 1) / 2);
    loop {
        let y = (x + v / x) >> 1;
        if y >= x {
            break;
        }
        x = y;
    }
    while x * x > v {
        x -= 1;
    }
    while (x + 1) * (x + 1) <= v {
        x += 1;
    }
    (x, x * x != v)
}

/// The square root of a double.
pub fn sqrt(ctx: &mut Ctx, a: u64) -> u64 {
    if let Some(r) = process_nan_in(BIN64, ctx, a) {
        return r;
    }
    let (ca, ua) = unpack_in(BIN64, ctx, a);
    match ca {
        Class::Zero => BIN64.zero(ua.sign),
        Class::Inf => {
            if ua.sign {
                ctx.flags |= IOC;
                BIN64.default_nan()
            } else {
                BIN64.inf(false)
            }
        }
        _ => {
            if ua.sign {
                ctx.flags |= IOC;
                return BIN64.default_nan();
            }
            let (mut m, mut e) = (ua.mant as u128, ua.exp);
            let bits = 128 - m.leading_zeros() as i32;
            let mut shift = 117 - bits;
            if (e - shift) & 1 != 0 {
                shift -= 1;
            }
            m <<= shift as u32;
            e -= shift;
            let (s, rem) = isqrt128(m);
            round_pack_in(BIN64, ctx, false, e / 2, s, rem)
        }
    }
}

/// The three-operand NaN rule (the addend first, then the two factors), and
/// the invalid product beside a quiet-NaN addend.
fn process_nans3(ctx: &mut Ctx, addend: u64, n: u64, m: u64) -> Option<u64> {
    let (ca, cn, cm) = (classify(addend), classify(n), classify(m));
    let order = [(ca, addend), (cn, n), (cm, m)];
    for (c, v) in order.iter() {
        if *c == Class::SNan {
            ctx.flags |= IOC;
            return Some(quiet_in(BIN64, ctx, *v));
        }
    }
    let fz = ctx.fz;
    let after_flush = |c: Class| if c == Class::Denormal && fz { Class::Zero } else { c };
    let (fn_, fm) = (after_flush(cn), after_flush(cm));
    let inf_times_zero = (fn_ == Class::Inf && fm == Class::Zero) || (fn_ == Class::Zero && fm == Class::Inf);
    if inf_times_zero && ca == Class::QNan {
        ctx.flags |= IOC;
        return Some(BIN64.default_nan());
    }
    for (c, v) in order.iter() {
        if *c == Class::QNan {
            return Some(quiet_in(BIN64, ctx, *v));
        }
    }
    None
}

/// The fused multiply-add `addend + n * m` with one rounding (VFMA's shape;
/// the other fused forms negate their inputs before calling).
pub fn fma(ctx: &mut Ctx, addend: u64, n: u64, m: u64) -> u64 {
    note_denormals(ctx, &[addend, n, m]);
    if let Some(r) = process_nans3(ctx, addend, n, m) {
        return r;
    }
    let (cc, uc) = unpack_in(BIN64, ctx, addend);
    let (cn, un) = unpack_in(BIN64, ctx, n);
    let (cm, um) = unpack_in(BIN64, ctx, m);
    let psign = un.sign ^ um.sign;
    let pinf = cn == Class::Inf || cm == Class::Inf;
    let pzero = cn == Class::Zero || cm == Class::Zero;
    if pinf && pzero {
        ctx.flags |= IOC;
        return BIN64.default_nan();
    }
    if pinf {
        if cc == Class::Inf && uc.sign != psign {
            ctx.flags |= IOC;
            return BIN64.default_nan();
        }
        return BIN64.inf(psign);
    }
    if cc == Class::Inf {
        return BIN64.inf(uc.sign);
    }
    if pzero {
        if cc == Class::Zero {
            return if uc.sign == psign { BIN64.zero(psign) } else { BIN64.zero(ctx.round == Round::Minus) };
        }
        return pack_unp(ctx, uc);
    }
    let prod = (un.mant as u128) * (um.mant as u128);
    let pexp = un.exp + um.exp;
    if cc == Class::Zero {
        return round_pack_in(BIN64, ctx, psign, pexp, prod, false);
    }
    let p = U256::from_u128(prod).shl(64);
    let c = U256::from_u128((uc.mant as u128) << 64);
    let (pe, ce) = (pexp - 64, uc.exp - 64);
    let (big, bsign, bexp, small, ssign, sexp) = if pe >= ce { (p, psign, pe, c, uc.sign, ce) } else { (c, uc.sign, ce, p, psign, pe) };
    let (small_al, lost) = small.shr_sticky((bexp - sexp) as u32);
    let (sign, mag, sticky) = if bsign == ssign {
        (bsign, big.add(small_al), lost)
    } else {
        match big.cmp(small_al) {
            core::cmp::Ordering::Equal => {
                if lost {
                    (ssign, U256::ZERO, true)
                } else {
                    return BIN64.zero(ctx.round == Round::Minus);
                }
            }
            core::cmp::Ordering::Greater => {
                let d = big.sub(small_al);
                if lost { (bsign, d.sub(U256::from_u128(1)), true) } else { (bsign, d, false) }
            }
            core::cmp::Ordering::Less => (ssign, small_al.sub(big), lost),
        }
    };
    let (m128, adj, more) = mag.to_u128_sticky();
    round_pack_in(BIN64, ctx, sign, bexp + adj, m128, sticky || more)
}

/// Negate (the sign bit flipped; a NaN is not quieted, as the hardware's VNEG does not signal).
pub fn neg(a: u64) -> u64 {
    a ^ BIN64.sign_bit()
}

/// Absolute value (the sign bit cleared; no signalling).
pub fn abs(a: u64) -> u64 {
    a & !BIN64.sign_bit()
}

/// The condition flags of a compare: (N, Z, C, V) packed as FPSCR bits, and
/// the invalid flag when a NaN is involved (any NaN for the E form, a
/// signalling NaN only otherwise).
pub fn compare(ctx: &mut Ctx, a: u64, b: u64, signal_quiet: bool) -> u32 {
    note_denormals(ctx, &[a, b]);
    let (ca, cb) = (classify(a), classify(b));
    let nan = matches!(ca, Class::QNan | Class::SNan) || matches!(cb, Class::QNan | Class::SNan);
    if nan {
        if ca == Class::SNan || cb == Class::SNan || signal_quiet {
            ctx.flags |= IOC;
        }
        return FPSCR_C | FPSCR_V;
    }
    let (_, ua) = unpack_in(BIN64, ctx, a);
    let (_, ub) = unpack_in(BIN64, ctx, b);
    let ord = ordering_finite_or_inf(ca, ua, cb, ub);
    match ord {
        core::cmp::Ordering::Equal => FPSCR_Z | FPSCR_C,
        core::cmp::Ordering::Less => FPSCR_N,
        core::cmp::Ordering::Greater => FPSCR_C,
    }
}

/// The order of two non-NaN values (zeros of both signs equal).
fn ordering_finite_or_inf(ca: Class, ua: Unp, cb: Class, ub: Unp) -> core::cmp::Ordering {
    use core::cmp::Ordering::*;
    let key = |c: Class, u: Unp| -> (i8, i32, u64) {
        match c {
            Class::Zero => (0, i32::MIN, 0),
            Class::Inf => (if u.sign { -2 } else { 2 }, 0, 0),
            _ => {
                let bits = 64 - u.mant.leading_zeros() as i32;
                (if u.sign { -1 } else { 1 }, u.exp + bits, u.mant << (64 - bits))
            }
        }
    };
    let (sa, ea, ma) = key(ca, ua);
    let (sb, eb, mb) = key(cb, ub);
    if sa != sb {
        return sa.cmp(&sb);
    }
    if sa == 0 || sa.abs() == 2 {
        return Equal;
    }
    let mag = (ea, ma).cmp(&(eb, mb));
    if sa < 0 { mag.reverse() } else { mag }
}

/// IEEE 754-2008 maxNum / minNum with the architecture's NaN handling: a
/// quiet NaN beside a number yields the number; a signalling NaN signals and
/// yields the quieted NaN; +0 is greater than -0.
pub fn max_min_num(ctx: &mut Ctx, a: u64, b: u64, want_max: bool) -> u64 {
    note_denormals(ctx, &[a, b]);
    let (ca, cb) = (classify(a), classify(b));
    let (a2, b2) = if ca == Class::QNan && cb != Class::QNan && cb != Class::SNan {
        (BIN64.inf(want_max), b)
    } else if cb == Class::QNan && ca != Class::QNan && ca != Class::SNan {
        (a, BIN64.inf(want_max))
    } else {
        (a, b)
    };
    if let Some(r) = process_nans_in(BIN64, ctx, a2, b2) {
        return r;
    }
    let (ca2, ua) = unpack_in(BIN64, ctx, a2);
    let (cb2, ub) = unpack_in(BIN64, ctx, b2);
    let ord = if ca2 == Class::Zero && cb2 == Class::Zero {
        ub.sign.cmp(&ua.sign)
    } else {
        ordering_finite_or_inf(ca2, ua, cb2, ub)
    };
    let pick_a = match ord {
        core::cmp::Ordering::Greater => want_max,
        core::cmp::Ordering::Less => !want_max,
        core::cmp::Ordering::Equal => true,
    };
    let chosen = if pick_a { a2 } else { b2 };
    match classify(chosen) {
        Class::Denormal if ctx.fz => BIN64.zero(sign_in(BIN64, chosen)),
        _ => chosen,
    }
}


/// A double to a single.
pub fn to_f32(ctx: &mut Ctx, a: u64) -> u32 {
    let ca = classify(a);
    match ca {
        Class::SNan | Class::QNan => {
            if ca == Class::SNan {
                ctx.flags |= IOC;
            }
            if ctx.dn {
                return BIN32.default_nan() as u32;
            }
            let sign = (a >> 63) as u32;
            let frac = ((a & BIN64.frac_mask()) >> 29) as u32;
            return (sign << 31) | 0x7F80_0000 | 0x0040_0000 | frac;
        }
        Class::Inf => return BIN32.inf(sign_in(BIN64, a)) as u32,
        _ => {}
    }
    let (c, u) = unpack_in(BIN64, ctx, a);
    if c == Class::Zero {
        return BIN32.zero(u.sign) as u32;
    }
    round_pack_in(BIN32, ctx, u.sign, u.exp, u.mant as u128, false) as u32
}

/// A single to a double (exact; a signalling NaN signals).
pub fn from_f32(ctx: &mut Ctx, a: u32) -> u64 {
    let a64 = a as u64;
    let ca = classify_in(BIN32, a64);
    match ca {
        Class::SNan | Class::QNan => {
            if ca == Class::SNan {
                ctx.flags |= IOC;
            }
            if ctx.dn {
                return BIN64.default_nan();
            }
            let sign = (a64 >> 31) << 63;
            let frac = (a64 & BIN32.frac_mask()) << 29;
            return sign | BIN64.exp_bits_all() | BIN64.quiet_bit() | frac;
        }
        Class::Inf => return BIN64.inf(sign_in(BIN32, a64)),
        _ => {}
    }
    let (c, u) = unpack_in(BIN32, ctx, a64);
    if c == Class::Zero {
        return BIN64.zero(u.sign);
    }
    round_pack_in(BIN64, ctx, u.sign, u.exp, u.mant as u128, false)
}

impl Fmt {
    fn exp_bits_all(&self) -> u64 {
        (self.exp_max() as u64) << (self.mant_bits - 1)
    }
}

/// Round `mant * 2^exp` (a positive magnitude) to an integer under the direction; returns (value, inexact); saturates at 2^64.
fn to_integer_mag(round: Round, sign: bool, exp: i32, mant: u64) -> Option<(u64, bool)> {
    if mant == 0 {
        return Some((0, false));
    }
    if exp >= 0 {
        if exp >= 64 || (mant as u128) << exp >= 1u128 << 64 {
            return None;
        }
        return Some((mant << exp, false));
    }
    let s = (-exp) as u32;
    if s >= 64 {
        let round_bit = s == 64 && mant >> 63 != 0;
        let sticky = if s == 64 { mant & (u64::MAX >> 1) != 0 } else { true };
        let up = increment(round, sign, false, round_bit, sticky);
        return Some((up as u64, true));
    }
    let lost = mant & ((1u64 << s) - 1);
    let round_bit = (lost >> (s - 1)) & 1 != 0;
    let sticky = lost & ((1u64 << (s - 1)) - 1) != 0;
    let mut v = mant >> s;
    let inexact = round_bit || sticky;
    if inexact && increment(round, sign, v & 1 != 0, round_bit, sticky) {
        v += 1;
    }
    Some((v, inexact))
}

/// A double to a 32-bit integer (signed or unsigned) under the direction,
/// saturating with the invalid flag as the architecture does; a NaN gives 0
/// with the invalid flag; an inexact in-range result raises inexact.
pub fn to_int32(ctx: &mut Ctx, a: u64, signed: bool, round: Round) -> u32 {
    let ca = classify(a);
    if matches!(ca, Class::QNan | Class::SNan) {
        ctx.flags |= IOC;
        return 0;
    }
    let (c, u) = unpack_in(BIN64, ctx, a);
    if c == Class::Inf {
        ctx.flags |= IOC;
        return saturate32(signed, u.sign);
    }
    if c == Class::Zero {
        return 0;
    }
    match to_integer_mag(round, u.sign, u.exp, u.mant) {
        None => {
            ctx.flags |= IOC;
            saturate32(signed, u.sign)
        }
        Some((v, inexact)) => {
            let out = if signed {
                if u.sign {
                    if v > 1u64 << 31 {
                        ctx.flags |= IOC;
                        return 0x8000_0000;
                    }
                    (v as u32).wrapping_neg()
                } else {
                    if v > 0x7FFF_FFFF {
                        ctx.flags |= IOC;
                        return 0x7FFF_FFFF;
                    }
                    v as u32
                }
            } else {
                if u.sign {
                    if v != 0 {
                        ctx.flags |= IOC;
                        return 0;
                    }
                    0
                } else {
                    if v > 0xFFFF_FFFF {
                        ctx.flags |= IOC;
                        return 0xFFFF_FFFF;
                    }
                    v as u32
                }
            };
            if inexact {
                ctx.flags |= IXC;
            }
            out
        }
    }
}

fn saturate32(signed: bool, negative: bool) -> u32 {
    match (signed, negative) {
        (true, true) => 0x8000_0000,
        (true, false) => 0x7FFF_FFFF,
        (false, true) => 0,
        (false, false) => 0xFFFF_FFFF,
    }
}

/// A 32-bit integer to a double (exact).
pub fn from_int32(ctx: &mut Ctx, v: u32, signed: bool) -> u64 {
    let (sign, mag) = if signed && (v as i32) < 0 { (true, (v as i32).unsigned_abs() as u64) } else { (false, v as u64) };
    if mag == 0 {
        return BIN64.zero(false);
    }
    round_pack_in(BIN64, ctx, sign, 0, mag as u128, false)
}

/// A double to a fixed-point integer with `fbits` fraction bits (the value
/// scaled by 2^fbits, rounded toward zero, saturated to the width with the
/// invalid flag; inexact raised when bits were lost).
pub fn to_fixed(ctx: &mut Ctx, a: u64, signed: bool, bits: u32, fbits: u32) -> u32 {
    let ca = classify(a);
    if matches!(ca, Class::QNan | Class::SNan) {
        ctx.flags |= IOC;
        return 0;
    }
    let (c, u) = unpack_in(BIN64, ctx, a);
    let (lo, hi): (i64, i64) = if signed { (-(1i64 << (bits - 1)), (1i64 << (bits - 1)) - 1) } else { (0, (1i64 << bits) - 1) };
    let sat = |negative: bool| -> u32 { (if negative { lo } else { hi }) as u32 & (if bits == 32 { u32::MAX } else { (1u32 << bits) - 1 }) };
    if c == Class::Inf {
        ctx.flags |= IOC;
        return sat(u.sign);
    }
    if c == Class::Zero {
        return 0;
    }
    match to_integer_mag(Round::Zero, u.sign, u.exp + fbits as i32, u.mant) {
        None => {
            ctx.flags |= IOC;
            sat(u.sign)
        }
        Some((v, inexact)) => {
            let sv: i128 = if u.sign { -(v as i128) } else { v as i128 };
            if sv < lo as i128 || sv > hi as i128 {
                ctx.flags |= IOC;
                return sat(u.sign);
            }
            if inexact {
                ctx.flags |= IXC;
            }
            (sv as i64 as u32) & (if bits == 32 { u32::MAX } else { (1u32 << bits) - 1 })
        }
    }
}

/// A fixed-point integer with `fbits` fraction bits to a double (exact).
pub fn from_fixed(ctx: &mut Ctx, v: u32, signed: bool, bits: u32, fbits: u32) -> u64 {
    let raw = if bits == 32 { v } else { v & ((1u32 << bits) - 1) };
    let (sign, mag) = if signed {
        let s = ((raw << (32 - bits)) as i32) >> (32 - bits);
        (s < 0, s.unsigned_abs() as u64)
    } else {
        (false, raw as u64)
    };
    if mag == 0 {
        return BIN64.zero(false);
    }
    round_pack_in(BIN64, ctx, sign, -(fbits as i32), mag as u128, false)
}

/// A double to a half-precision word (IEEE, or the alternative format under AHP:
/// no infinities and no NaNs, the largest magnitude on overflow with the invalid flag).
pub fn to_f16(ctx: &mut Ctx, a: u64) -> u32 {
    let ca = classify(a);
    let sign = sign_in(BIN64, a);
    match ca {
        Class::SNan | Class::QNan => {
            if ca == Class::SNan || ctx.ahp {
                ctx.flags |= IOC;
            }
            if ctx.ahp {
                return (BIN16.zero(sign)) as u32;
            }
            if ctx.dn {
                return BIN16.default_nan() as u32;
            }
            let frac = ((a & BIN64.frac_mask()) >> 42) as u32;
            return ((sign as u32) << 15) | 0x7C00 | 0x0200 | frac;
        }
        Class::Inf => {
            if ctx.ahp {
                ctx.flags |= IOC;
                return (0x7FFF | ((sign as u64) << 15)) as u32;
            }
            return BIN16.inf(sign) as u32;
        }
        _ => {}
    }
    let (c, u) = unpack_in(BIN64, ctx, a);
    if c == Class::Zero {
        return BIN16.zero(u.sign) as u32;
    }
    if ctx.ahp {
        let mut c2 = *ctx;
        c2.fz = false;
        let r = round_pack_ahp(&mut c2, u.sign, u.exp, u.mant as u128);
        ctx.flags |= c2.flags;
        return r;
    }
    let mut c2 = *ctx;
    c2.fz = false;
    let r = round_pack_in(BIN16, &mut c2, u.sign, u.exp, u.mant as u128, false) as u32;
    ctx.flags |= c2.flags;
    r
}

/// The alternative half-precision rounding: the format's exponent field 31 is
/// ordinary, so the largest value is 0x7FFF; overflow gives it with IOC.
fn round_pack_ahp(ctx: &mut Ctx, sign: bool, exp: i32, mant: u128) -> u32 {
    let fmt = BIN16;
    let bits = 128 - mant.leading_zeros() as i32;
    let lead_exp = exp + bits - 1;
    if lead_exp < fmt.emin() {
        return tiny_in(fmt, ctx, sign, exp, mant, false) as u32;
    }
    let mb = fmt.mant_bits as i32;
    let (mut m, round, stick) = if bits > mb {
        let shift = (bits - mb) as u32;
        let lost = mant & ((1u128 << shift) - 1);
        ((mant >> shift) as u64, (lost >> (shift - 1)) & 1 != 0, lost & ((1u128 << (shift - 1)) - 1) != 0)
    } else {
        ((mant << (mb - bits) as u32) as u64, false, false)
    };
    let mut e = lead_exp;
    let inexact = round || stick;
    if inexact && increment(ctx.round, sign, m & 1 != 0, round, stick) {
        m += 1;
        if m == 1u64 << mb {
            m >>= 1;
            e += 1;
        }
    }
    if e > fmt.bias + 1 {
        ctx.flags |= IOC;
        return (0x7FFF | ((sign as u64) << 15)) as u32;
    }
    if inexact {
        ctx.flags |= IXC;
    }
    (((e + fmt.bias) as u64) << (fmt.mant_bits - 1) | (m & fmt.frac_mask()) | if sign { fmt.sign_bit() } else { 0 }) as u32
}

/// A half-precision word to a double (exact; the alternative format under AHP).
pub fn from_f16(ctx: &mut Ctx, h: u32) -> u64 {
    let h64 = (h & 0xFFFF) as u64;
    let sign = h64 >> 15 != 0;
    let exp = ((h64 >> 10) & 0x1F) as i32;
    let frac = h64 & 0x3FF;
    if ctx.ahp {
        if exp == 0 {
            if frac == 0 {
                return BIN64.zero(sign);
            }
            return round_pack_in(BIN64, ctx, sign, -24, frac as u128, false);
        }
        return round_pack_in(BIN64, ctx, sign, exp - 15 - 10, (frac | 0x400) as u128, false);
    }
    let ca = classify_in(BIN16, h64);
    match ca {
        Class::SNan | Class::QNan => {
            if ca == Class::SNan {
                ctx.flags |= IOC;
            }
            if ctx.dn {
                return BIN64.default_nan();
            }
            return ((sign as u64) << 63) | BIN64.exp_bits_all() | BIN64.quiet_bit() | (frac << 42);
        }
        Class::Inf => return BIN64.inf(sign),
        _ => {}
    }
    let mut c2 = *ctx;
    c2.fz = false;
    let (c, u) = unpack_in(BIN16, &mut c2, h64);
    if c == Class::Zero {
        return BIN64.zero(u.sign);
    }
    round_pack_in(BIN64, ctx, u.sign, u.exp, u.mant as u128, false)
}

/// Round a double to an integral value in floating-point format under the
/// direction; `exact` says whether a change raises the inexact flag (VRINTX
/// only). A NaN is quieted (signalling with the invalid flag); infinities and
/// zeros stand; a result that rounds to zero keeps the sign of the input.
pub fn round_to_integral(ctx: &mut Ctx, a: u64, round: Round, exact: bool) -> u64 {
    if let Some(r) = process_nan_in(BIN64, ctx, a) {
        return r;
    }
    let (c, u) = unpack_in(BIN64, ctx, a);
    match c {
        Class::Zero => return BIN64.zero(u.sign),
        Class::Inf => return BIN64.inf(u.sign),
        _ => {}
    }
    if u.exp >= 0 {
        return a;
    }
    let (v, inexact) = to_integer_mag(round, u.sign, u.exp, u.mant).unwrap_or((0, true));
    if inexact && exact {
        ctx.flags |= IXC;
    }
    if v == 0 {
        return BIN64.zero(u.sign);
    }
    round_pack_in(BIN64, ctx, u.sign, 0, v as u128, false)
}

/// The VFP 8-bit modified immediate `abcdefgh` expanded to a double
/// (VMOV.BIN64 Dd, #imm): the sign a, the 11-bit exponent field NOT(b) then
/// b eight times then c d, the fraction's top four bits e f g h, the rest
/// zero.
pub fn expand_imm(imm8: u32) -> u64 {
    let a = ((imm8 >> 7) & 1) as u64;
    let b = ((imm8 >> 6) & 1) as u64;
    let cdefgh = (imm8 & 0x3F) as u64;
    let expfield = ((b ^ 1) << 10) | (if b != 0 { 0xFF << 2 } else { 0 }) | (cdefgh >> 4);
    let frac = (cdefgh & 0xF) << 48;
    (a << 63) | (expfield << 52) | frac
}
