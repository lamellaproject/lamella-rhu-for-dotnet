//! The decoder of the double-precision forms: from the two halfwords of a
//! T32 floating-point data-processing encoding to the form and its
//! register numbers. Pure; no register is read here. Every table below is
//! proven against the toolchain's assembler by the host tier's vectors (the
//! bytes the assembler emits for every form, decoded to the same form and
//! fields), so the tables are checked, not trusted.

use super::soft::Round;

/// The rounding a conversion or integral rounding uses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rounding {
    /// The frame's FPSCR mode.
    Fpscr,
    /// A fixed direction (toward zero, or the A/N/P/M directed forms).
    Fixed(Round),
}

/// The condition of a VSEL (the two-bit field: EQ, VS, GE, GT; the
/// architecture inverts the low bit for the negated forms in software).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SelCond {
    Eq,
    Vs,
    Ge,
    Gt,
}

/// A double-precision form and the operand facts that are not register numbers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Form {
    Add,
    Sub,
    Mul,
    Nmul,
    Div,
    /// Dd = Dd + Dn * Dm, two roundings.
    Mla,
    /// Dd = Dd - Dn * Dm, two roundings.
    Mls,
    /// Dd = -Dd - Dn * Dm, two roundings.
    Nmla,
    /// Dd = -Dd + Dn * Dm, two roundings.
    Nmls,
    /// Dd = Dd + Dn * Dm, fused.
    Fma,
    /// Dd = Dd - Dn * Dm, fused.
    Fms,
    /// Dd = -Dd - Dn * Dm, fused.
    Fnma,
    /// Dd = -Dd + Dn * Dm, fused.
    Fnms,
    MaxNm,
    MinNm,
    Sel(SelCond),
    Mov,
    Abs,
    Neg,
    Sqrt,
    /// VCMP or VCMPE (`e`), against Dm or against zero (`zero`).
    Cmp { e: bool, zero: bool },
    /// Round to an integral value in floating-point format; `exact` raises inexact (VRINTX).
    Rint { rounding: Rounding, exact: bool },
    /// A double to a single (Sd = Dm).
    ToF32,
    /// A single to a double (Dd = Sm).
    FromF32,
    /// A double to a signed or unsigned 32-bit integer (Sd = Dm) under the rounding.
    ToInt { signed: bool, rounding: Rounding },
    /// A signed or unsigned 32-bit integer to a double (Dd = Sm).
    FromInt { signed: bool },
    /// A double to fixed-point (Dd in place; the register is both source and destination), toward zero.
    ToFixed { signed: bool, bits: u32, fbits: u32 },
    /// Fixed-point to a double (Dd in place).
    FromFixed { signed: bool, bits: u32, fbits: u32 },
    /// A half to a double (Dd = the bottom or top halfword of Sm).
    FromF16 { top: bool },
    /// A double to a half (the bottom or top halfword of Sd = Dm).
    ToF16 { top: bool },
    /// Dd = the expanded immediate.
    MovImm(u64),
}

/// A decoded double-precision instruction: the form and the register
/// numbers (doubleword numbers for the D operands, single-precision numbers
/// for the S operands; a form uses the ones its description names).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Decoded {
    pub form: Form,
    /// Dd (or Sd for the forms whose destination is single).
    pub d: u8,
    pub n: u8,
    pub m: u8,
    /// The single-precision register number of the S operand (Vd:D, Vn:N or Vm:M as the form names it).
    pub sd: u8,
    pub sn: u8,
    pub sm: u8,
}

/// The decoder. `hw1` is the first halfword at the faulting address, `hw2`
/// the second. Returns the form for a double-precision data-processing
/// encoding and nothing for anything else (a single-precision form, a
/// load, a store, a transfer, a non-floating-point word).
pub fn decode(hw1: u16, hw2: u16) -> Option<Decoded> {
    let group = hw1 >> 8;
    if group != 0xEE && group != 0xFE {
        return None;
    }
    if (hw2 >> 9) & 7 != 0b101 || (hw2 >> 4) & 1 != 0 {
        return None;
    }
    let sz = (hw2 >> 8) & 1;
    let o1_3 = ((hw1 >> 7) & 1) as u8;
    let dbit = ((hw1 >> 6) & 1) as u8;
    let o1_lo = ((hw1 >> 4) & 3) as u8;
    let opc2 = (hw1 & 0xF) as u8;
    let vd = ((hw2 >> 12) & 0xF) as u8;
    let nbit = ((hw2 >> 7) & 1) as u8;
    let op = ((hw2 >> 6) & 1) as u8;
    let mbit = ((hw2 >> 5) & 1) as u8;
    let vm = (hw2 & 0xF) as u8;
    let d = (dbit << 4) | vd;
    let n = (nbit << 4) | opc2;
    let m = (mbit << 4) | vm;
    let sd = (vd << 1) | dbit;
    let sn = (opc2 << 1) | nbit;
    let sm = (vm << 1) | mbit;
    let regs = |form: Form| Some(Decoded { form, d, n, m, sd, sn, sm });

    if sz == 0 {
        if group == 0xEE && o1_3 == 1 && o1_lo == 3 && op == 1 && opc2 == 0b0111 && nbit == 1 {
            return regs(Form::FromF32);
        }
        return None;
    }

    if group == 0xFE {
        if o1_3 == 0 {
            if op != 0 {
                return None;
            }
            let cond = match o1_lo {
                0 => SelCond::Eq,
                1 => SelCond::Vs,
                2 => SelCond::Ge,
                _ => SelCond::Gt,
            };
            return regs(Form::Sel(cond));
        }
        if o1_lo == 0 {
            return regs(if op == 0 { Form::MaxNm } else { Form::MinNm });
        }
        if o1_lo == 3 {
            let rm = match opc2 & 3 {
                0 => Round::Away,
                1 => Round::Nearest,
                2 => Round::Plus,
                _ => Round::Minus,
            };
            match opc2 >> 2 {
                0b11 => {
                    if op != 1 {
                        return None;
                    }
                    return regs(Form::ToInt { signed: nbit == 1, rounding: Rounding::Fixed(rm) });
                }
                0b10 => {
                    if nbit != 0 || op != 1 {
                        return None;
                    }
                    return regs(Form::Rint { rounding: Rounding::Fixed(rm), exact: false });
                }
                _ => return None,
            }
        }
        return None;
    }

    if o1_3 == 0 {
        let form = match (o1_lo, op) {
            (0, 0) => Form::Mla,
            (0, 1) => Form::Mls,
            (1, 0) => Form::Nmls,
            (1, 1) => Form::Nmla,
            (2, 0) => Form::Mul,
            (2, 1) => Form::Nmul,
            (3, 0) => Form::Add,
            _ => Form::Sub,
        };
        #[cfg(feature = "test-fpemul-doctored")]
        let form = match form {
            Form::Add => Form::Sub,
            Form::Sub => Form::Add,
            other => other,
        };
        return regs(form);
    }
    match o1_lo {
        0 => return if op == 0 { regs(Form::Div) } else { None },
        1 => return regs(if op == 0 { Form::Fnms } else { Form::Fnma }),
        2 => return regs(if op == 0 { Form::Fma } else { Form::Fms }),
        _ => {}
    }
    if op == 0 {
        let imm8 = ((opc2 as u32) << 4) | vm as u32;
        return Some(Decoded { form: Form::MovImm(super::soft::expand_imm(imm8)), d, n: 0, m: 0, sd, sn: 0, sm: 0 });
    }
    let opc3_hi = nbit;
    let form = match opc2 {
        0b0000 => if opc3_hi == 0 { Form::Mov } else { Form::Abs },
        0b0001 => if opc3_hi == 0 { Form::Neg } else { Form::Sqrt },
        0b0010 => Form::FromF16 { top: opc3_hi == 1 },
        0b0011 => Form::ToF16 { top: opc3_hi == 1 },
        0b0100 => Form::Cmp { e: opc3_hi == 1, zero: false },
        0b0101 => Form::Cmp { e: opc3_hi == 1, zero: true },
        0b0110 => Form::Rint { rounding: if opc3_hi == 0 { Rounding::Fpscr } else { Rounding::Fixed(Round::Zero) }, exact: false },
        0b0111 => if opc3_hi == 0 { Form::Rint { rounding: Rounding::Fpscr, exact: true } } else { Form::ToF32 },
        0b1000 => Form::FromInt { signed: opc3_hi == 1 },
        0b1010 | 0b1011 | 0b1110 | 0b1111 => {
            let signed = opc2 & 1 == 0;
            let bits = if opc3_hi == 1 { 32 } else { 16 };
            let imm = ((vm as u32) << 1) | mbit as u32;
            if imm > bits {
                return None;
            }
            let fbits = bits - imm;
            if opc2 & 0b0100 == 0 { Form::FromFixed { signed, bits, fbits } } else { Form::ToFixed { signed, bits, fbits } }
        }
        0b1100 => Form::ToInt { signed: false, rounding: if opc3_hi == 1 { Rounding::Fixed(Round::Zero) } else { Rounding::Fpscr } },
        0b1101 => Form::ToInt { signed: true, rounding: if opc3_hi == 1 { Rounding::Fixed(Round::Zero) } else { Rounding::Fpscr } },
        _ => return None,
    };
    regs(form)
}

/// Which of the doubleword register fields a form reads or writes as
/// doubleword registers: (d, n, m). A form whose destination or source is a
/// single-precision register uses the S number instead, so its D field is
/// not a doubleword operand and no bound applies to it.
pub fn uses(form: Form) -> (bool, bool, bool) {
    match form {
        Form::Add | Form::Sub | Form::Mul | Form::Nmul | Form::Div | Form::Mla | Form::Mls | Form::Nmla | Form::Nmls
        | Form::Fma | Form::Fms | Form::Fnma | Form::Fnms | Form::MaxNm | Form::MinNm | Form::Sel(_) => (true, true, true),
        Form::Mov | Form::Abs | Form::Neg | Form::Sqrt | Form::Rint { .. } => (true, false, true),
        Form::Cmp { zero: false, .. } => (true, false, true),
        Form::Cmp { zero: true, .. } => (true, false, false),
        Form::ToF32 | Form::ToInt { .. } | Form::ToF16 { .. } => (false, false, true),
        Form::FromF32 | Form::FromInt { .. } | Form::FromF16 { .. } => (true, false, false),
        Form::ToFixed { .. } | Form::FromFixed { .. } | Form::MovImm(_) => (true, false, false),
    }
}

/// The architecture's condition test for a form inside an IT block: the
/// four-bit condition the block gives the form, against the N, Z, C and V
/// flags in the stacked xPSR (bits 31 to 28). The two encodings of
/// "always" both pass; every other odd condition is its even twin negated.
pub fn condition_passed(cond: u8, xpsr: u32) -> bool {
    let n = xpsr >> 31 & 1 != 0;
    let z = xpsr >> 30 & 1 != 0;
    let c = xpsr >> 29 & 1 != 0;
    let v = xpsr >> 28 & 1 != 0;
    let base = match cond >> 1 {
        0 => z,
        1 => c,
        2 => n,
        3 => v,
        4 => c && !z,
        5 => n == v,
        6 => !z && n == v,
        _ => true,
    };
    if cond & 1 != 0 && cond != 0xF { !base } else { base }
}

/// A short name for a form, for the witness and the refusal line.
pub fn name(form: Form) -> &'static str {
    match form {
        Form::Add => "vadd.f64",
        Form::Sub => "vsub.f64",
        Form::Mul => "vmul.f64",
        Form::Nmul => "vnmul.f64",
        Form::Div => "vdiv.f64",
        Form::Mla => "vmla.f64",
        Form::Mls => "vmls.f64",
        Form::Nmla => "vnmla.f64",
        Form::Nmls => "vnmls.f64",
        Form::Fma => "vfma.f64",
        Form::Fms => "vfms.f64",
        Form::Fnma => "vfnma.f64",
        Form::Fnms => "vfnms.f64",
        Form::MaxNm => "vmaxnm.f64",
        Form::MinNm => "vminnm.f64",
        Form::Sel(_) => "vsel.f64",
        Form::Mov => "vmov.f64",
        Form::Abs => "vabs.f64",
        Form::Neg => "vneg.f64",
        Form::Sqrt => "vsqrt.f64",
        Form::Cmp { e: false, .. } => "vcmp.f64",
        Form::Cmp { e: true, .. } => "vcmpe.f64",
        Form::Rint { exact: true, .. } => "vrintx.f64",
        Form::Rint { rounding: Rounding::Fpscr, .. } => "vrintr.f64",
        Form::Rint { rounding: Rounding::Fixed(Round::Zero), .. } => "vrintz.f64",
        Form::Rint { rounding: Rounding::Fixed(Round::Away), .. } => "vrinta.f64",
        Form::Rint { rounding: Rounding::Fixed(Round::Nearest), .. } => "vrintn.f64",
        Form::Rint { rounding: Rounding::Fixed(Round::Plus), .. } => "vrintp.f64",
        Form::Rint { rounding: Rounding::Fixed(Round::Minus), .. } => "vrintm.f64",
        Form::ToF32 => "vcvt.f32.f64",
        Form::FromF32 => "vcvt.f64.f32",
        Form::ToInt { signed: true, rounding: Rounding::Fixed(Round::Zero) } => "vcvt.s32.f64",
        Form::ToInt { signed: false, rounding: Rounding::Fixed(Round::Zero) } => "vcvt.u32.f64",
        Form::ToInt { signed: true, rounding: Rounding::Fpscr } => "vcvtr.s32.f64",
        Form::ToInt { signed: false, rounding: Rounding::Fpscr } => "vcvtr.u32.f64",
        Form::ToInt { signed: true, .. } => "vcvt{a,n,p,m}.s32.f64",
        Form::ToInt { signed: false, .. } => "vcvt{a,n,p,m}.u32.f64",
        Form::FromInt { signed: true } => "vcvt.f64.s32",
        Form::FromInt { signed: false } => "vcvt.f64.u32",
        Form::ToFixed { .. } => "vcvt.fixed.f64",
        Form::FromFixed { .. } => "vcvt.f64.fixed",
        Form::FromF16 { top: false } => "vcvtb.f64.f16",
        Form::FromF16 { top: true } => "vcvtt.f64.f16",
        Form::ToF16 { top: false } => "vcvtb.f16.f64",
        Form::ToF16 { top: true } => "vcvtt.f16.f64",
        Form::MovImm(_) => "vmov.f64 #imm",
    }
}
