//! The double-precision emulation for the single-precision FPU class: a
//! double-precision form faults on these cores -- as an undefined instruction
//! on the Cortex-M33 and on a Cortex-M7 with a single-precision FPU, as a
//! coprocessor access denied on the Cortex-M4F (the double-precision
//! encodings name coprocessor 11, which that FPU does not implement); both
//! measured on the bench, both UsageFaults with the same frame -- and the
//! fault funnel hands the UsageFault here. This module decides whether
//! the fault is such a form, computes it by the architecture's rules
//! (`soft`), puts the result where the instruction would have, and returns
//! so the funnel resumes the program at the next instruction. The Cortex-M7
//! class computes the same forms in hardware; the api flavor's check
//! compares the two, bit for bit and flag for flag. The fault runs at the
//! lowest priority, so every interrupt preempts an emulated form; the rule
//! that implies: no double-precision instruction in a handler or under a
//! raised priority (a critical section) on these cores -- such a form
//! escalates to the hard fault. Each emulated instruction costs one fault;
//! the measured costs are on `trap`. The frame: the hardware pushes a basic
//! frame (8 words) or an extended one (then S0-S15, FPSCR and a reserved
//! word), saying which in EXC_RETURN bit 4; with lazy stacking on, an
//! extended frame's floating-point area is unwritten until a floating-point
//! instruction executes, so this handler executes one first, then reads and
//! writes S0-S15 and FPSCR in the frame and S16-S31 live; a basic frame (no
//! floating-point context yet) is computed into a fresh context and promoted
//! to an extended one, the context its first such instruction would create.

use core::arch::asm;
use core::ptr::{read_volatile, write_volatile};
use core::sync::atomic::{AtomicU32, Ordering};

use crate::funnel::ExcFrame;

pub mod decode;
mod frame;
pub mod soft;

use decode::{Decoded, Form, Rounding, SelCond};
use soft::{Ctx, Round};

const FPCCR: *const u32 = 0xE000_EF34 as *const u32;
const FPCAR: *const u32 = 0xE000_EF38 as *const u32;
const FPDSCR: *const u32 = 0xE000_EF3C as *const u32;
const CFSR: *mut u32 = 0xE000_ED28 as *mut u32;
const CFSR_UNDEFINSTR: u32 = 1 << 16;
const CFSR_NOCP: u32 = 1 << 19;
/// The two UsageFault classes a double-precision form raises on a core without
/// the double-precision extension: the funnel's gate condition, and the bits
/// the trap clears -- exactly the ones it found set.
pub const CFSR_FORM_CLASSES: u32 = CFSR_UNDEFINSTR | CFSR_NOCP;
/// The frame's floating-point area: S0 at this offset, FPSCR sixteen words later.
const FRAME_S0: usize = 0x20;
const FRAME_FPSCR: usize = 0x60;
const EXC_RETURN_FTYPE: u32 = 1 << 4;

/// The witness, debug-facing and exported by symbol: the forms that came
/// through the funnel, the frames promoted, the refusals, the last form's
/// code (one per form, the decoder's order), the forms skipped on a failed
/// condition inside an IT block.
#[repr(C)]
pub struct Witness {
    pub traps: AtomicU32,
    pub promotions: AtomicU32,
    pub refusals: AtomicU32,
    pub last: AtomicU32,
    pub skipped: AtomicU32,
}

#[unsafe(export_name = "__rhu_fpemul_witness")]
pub static WITNESS: Witness = Witness {
    traps: AtomicU32::new(0),
    promotions: AtomicU32::new(0),
    refusals: AtomicU32::new(0),
    last: AtomicU32::new(0),
    skipped: AtomicU32::new(0),
};

/// The forms skipped on a failed condition, for the api flavor's checks.
#[cfg(feature = "api-smoke")]
pub fn skipped() -> u32 {
    WITNESS.skipped.load(Ordering::Acquire)
}

/// The witness's counts: (traps, promotions, refusals), for the api flavor's checks.
#[cfg(feature = "api-smoke")]
pub fn counts() -> (u32, u32, u32) {
    (
        WITNESS.traps.load(Ordering::Acquire),
        WITNESS.promotions.load(Ordering::Acquire),
        WITNESS.refusals.load(Ordering::Acquire),
    )
}

/// The gate's answer.
pub enum Trap {
    /// Not a double-precision form: the funnel's terminal path.
    NotOurs,
    /// Emulated; the frame's PC advanced; the EXC_RETURN to return through
    /// (bit 4 cleared when a basic frame was promoted).
    Emulated(u32),
    /// A double-precision form the handler refuses, with the reason: the
    /// funnel prints it and takes the terminal path.
    Refused(&'static str),
}

/// The register file the execution reads and writes: S0-S15 and FPSCR in
/// the frame (an extended frame) or in a fresh context (a basic frame,
/// promoted at the end); S16-S31 live.
struct File {
    frame: *mut ExcFrame,
    ext: bool,
    fresh: [u32; 16],
    fpscr: u32,
}

impl File {
    fn slot(&self, s: u8) -> *mut u32 {
        (self.frame as usize + FRAME_S0 + 4 * s as usize) as *mut u32
    }
    fn s_read(&self, s: u8) -> u32 {
        if s < 16 {
            if self.ext { unsafe { read_volatile(self.slot(s)) } } else { self.fresh[s as usize] }
        } else {
            live_read(s)
        }
    }
    fn s_write(&mut self, s: u8, v: u32) {
        if s < 16 {
            if self.ext {
                unsafe { write_volatile(self.slot(s), v) }
            } else {
                self.fresh[s as usize] = v;
            }
        } else {
            live_write(s, v)
        }
    }
    fn d_read(&self, d: u8) -> u64 {
        (self.s_read(2 * d) as u64) | ((self.s_read(2 * d + 1) as u64) << 32)
    }
    fn d_write(&mut self, d: u8, v: u64) {
        self.s_write(2 * d, v as u32);
        self.s_write(2 * d + 1, (v >> 32) as u32);
    }
}

macro_rules! live_arms {
    (read $s:expr, $($n:literal => $reg:tt),*) => {
        match $s { $($n => { let v: u32; unsafe { asm!(concat!("vmov {0}, ", $reg), out(reg) v, options(nomem, nostack, preserves_flags)) }; v })*  _ => 0 }
    };
    (write $s:expr, $v:expr, $($n:literal => $reg:tt),*) => {
        match $s { $($n => unsafe { asm!(concat!("vmov ", $reg, ", {0}"), in(reg) $v, options(nomem, nostack, preserves_flags)) },)* _ => {} }
    };
}

/// A live read of S16-S31 (the registers no frame carries).
fn live_read(s: u8) -> u32 {
    live_arms!(read s,
        16 => "s16", 17 => "s17", 18 => "s18", 19 => "s19", 20 => "s20", 21 => "s21", 22 => "s22", 23 => "s23",
        24 => "s24", 25 => "s25", 26 => "s26", 27 => "s27", 28 => "s28", 29 => "s29", 30 => "s30", 31 => "s31")
}

/// A live write of S16-S31.
fn live_write(s: u8, v: u32) {
    live_arms!(write s, v,
        16 => "s16", 17 => "s17", 18 => "s18", 19 => "s19", 20 => "s20", 21 => "s21", 22 => "s22", 23 => "s23",
        24 => "s24", 25 => "s25", 26 => "s26", 27 => "s27", 28 => "s28", 29 => "s29", 30 => "s30", 31 => "s31")
}

unsafe extern "C" {
    static __stext: u8;
    static __etext: u8;
}

/// The gate: the funnel calls it at a UsageFault whose CFSR carries one of
/// the form classes (`class`: the UNDEFINSTR or NOCP bit found set, cleared
/// here when the form is emulated), with the frame the trampoline found and
/// the EXC_RETURN it arrived with. The caller has checked nothing else. The
/// cost per emulated instruction,
/// measured by the api flavor's cost check on the Cortex-M33 silicon at
/// 160 MHz (the cycle counter around one instruction, the bracket's own 7
/// cycles included): about 1,900 cycles for an add, 2,100 for a subtract,
/// 1,700 for a multiply, 2,400 to 2,600 for a multiply-accumulate, 9,400
/// for a divide, 46,000 for a square root, and 900 to 1,500 for a move, a
/// negation, a compare, a rounding or a conversion.
pub fn trap(frame: *mut ExcFrame, exc_return: u32, class: u32) -> Trap {
    if exc_return & 0xC != 0xC {
        return refuse("outside thread mode or on the main stack");
    }
    let f = frame as usize;
    let need = if exc_return & EXC_RETURN_FTYPE == 0 { frame::EXTENDED_BYTES } else { frame::BASIC_BYTES };
    if !crate::board::FRAME_RANGES.iter().any(|&(lo, hi)| f >= lo && f + need <= hi) {
        return refuse("the frame outside the thread ranges");
    }
    let pc = unsafe { read_volatile(&(*frame).pc) };
    if pc & 1 != 0 {
        return refuse("an odd stacked pc");
    }
    let (text_lo, text_hi) = (core::ptr::addr_of!(__stext) as usize, core::ptr::addr_of!(__etext) as usize);
    if ((pc as usize) < text_lo || (pc as usize) + 4 > text_hi) && !crate::registry::code_holds(pc, 4) {
        return refuse("the stacked pc outside every registered code range");
    }
    let hw1 = unsafe { read_volatile(pc as *const u16) };
    let hw2 = unsafe { read_volatile((pc + 2) as *const u16) };
    let dec = match decode::decode(hw1, hw2) {
        Some(d) => d,
        None => return Trap::NotOurs,
    };
    let (ud, un, um) = decode::uses(dec.form);
    if (ud && dec.d >= 16) || (un && dec.n >= 16) || (um && dec.m >= 16) {
        return refuse("a doubleword register beyond D15");
    }
    let xpsr = unsafe { read_volatile(&(*frame).xpsr) };
    let it = it_state(xpsr);
    if it != 0 && !decode::condition_passed((it >> 4) as u8, xpsr) {
        unsafe {
            write_volatile(&mut (*frame).pc, pc + 4);
            write_volatile(&mut (*frame).xpsr, it_advance(xpsr));
            write_volatile(CFSR, class);
        }
        WITNESS.skipped.fetch_add(1, Ordering::AcqRel);
        WITNESS.traps.fetch_add(1, Ordering::AcqRel);
        WITNESS.last.store(form_code(dec.form), Ordering::Release);
        return Trap::Emulated(exc_return);
    }
    let ext = exc_return & EXC_RETURN_FTYPE == 0;
    if ext {
        let fpccr = unsafe { read_volatile(FPCCR) };
        if fpccr & 1 != 0 {
            let fpcar = unsafe { read_volatile(FPCAR) } as usize;
            if fpcar != f + FRAME_S0 {
                return refuse("lazy state pending for another frame");
            }
            unsafe { asm!("vmov.f32 s0, s0", options(nostack, preserves_flags)) };
        }
    }
    let fpscr = if ext {
        unsafe { read_volatile((f + FRAME_FPSCR) as *const u32) }
    } else {
        unsafe { read_volatile(FPDSCR) }
    };
    let mut file = File { frame, ext, fresh: [0; 16], fpscr };
    execute(&dec, &mut file, xpsr);
    let mut out = exc_return;
    if ext {
        unsafe { write_volatile((f + FRAME_FPSCR) as *mut u32, file.fpscr) };
    } else {
        match unsafe { frame::promote(frame, &file.fresh, file.fpscr) } {
            Ok(()) => {
                out &= !EXC_RETURN_FTYPE;
                WITNESS.promotions.fetch_add(1, Ordering::AcqRel);
            }
            Err(reason) => return refuse(reason),
        }
    }
    unsafe {
        let fr = frame::current(frame, out);
        write_volatile(&mut (*fr).pc, pc + 4);
        write_volatile(&mut (*fr).xpsr, it_advance(read_volatile(&(*fr).xpsr)));
        write_volatile(CFSR, class);
    }
    WITNESS.traps.fetch_add(1, Ordering::AcqRel);
    WITNESS.last.store(form_code(dec.form), Ordering::Release);
    Trap::Emulated(out)
}

fn refuse(reason: &'static str) -> Trap {
    WITNESS.refusals.fetch_add(1, Ordering::AcqRel);
    Trap::Refused(reason)
}

/// The IT state as the xPSR carries it: its low two bits at bits 26 and 25,
/// the upper six at bits 15 to 10; zero outside an IT block.
fn it_state(xpsr: u32) -> u32 {
    ((xpsr >> 25) & 0x3) | (((xpsr >> 10) & 0x3F) << 2)
}

/// The IT state advanced by one instruction (the architecture's ITAdvance):
/// the block ends when its low three bits are zero, else the state shifts.
fn it_advance(xpsr: u32) -> u32 {
    let it = ((xpsr >> 25) & 0x3) | (((xpsr >> 10) & 0x3F) << 2);
    if it == 0 {
        return xpsr;
    }
    let next = if it & 0x7 == 0 { 0 } else { (it & 0xE0) | ((it << 1) & 0x1F) };
    (xpsr & !((0x3 << 25) | (0x3F << 10))) | ((next & 0x3) << 25) | (((next >> 2) & 0x3F) << 10)
}

/// The rounding a form uses: the frame's, or its own fixed direction.
fn resolve(r: Rounding, ctx: &Ctx) -> Round {
    match r {
        Rounding::Fpscr => ctx.round,
        Rounding::Fixed(x) => x,
    }
}

/// Execute one decoded form over the file; the flags and the compare
/// result go to the file's FPSCR.
fn execute(dec: &Decoded, file: &mut File, xpsr: u32) {
    let mut ctx = Ctx::from_fpscr(file.fpscr);
    let (d, n, m) = (dec.d, dec.n, dec.m);
    match dec.form {
        Form::Add => { let r = soft::add(&mut ctx, file.d_read(n), file.d_read(m)); file.d_write(d, r) }
        Form::Sub => { let r = soft::sub(&mut ctx, file.d_read(n), file.d_read(m)); file.d_write(d, r) }
        Form::Mul => { let r = soft::mul(&mut ctx, file.d_read(n), file.d_read(m)); file.d_write(d, r) }
        Form::Nmul => { let r = soft::neg(soft::mul(&mut ctx, file.d_read(n), file.d_read(m))); file.d_write(d, r) }
        Form::Div => { let r = soft::div(&mut ctx, file.d_read(n), file.d_read(m)); file.d_write(d, r) }
        Form::Mla => { let p = soft::mul(&mut ctx, file.d_read(n), file.d_read(m)); let r = soft::add(&mut ctx, file.d_read(d), p); file.d_write(d, r) }
        Form::Mls => { let p = soft::mul(&mut ctx, file.d_read(n), file.d_read(m)); let r = soft::add(&mut ctx, file.d_read(d), soft::neg(p)); file.d_write(d, r) }
        Form::Nmla => { let p = soft::mul(&mut ctx, file.d_read(n), file.d_read(m)); let r = soft::add(&mut ctx, soft::neg(file.d_read(d)), soft::neg(p)); file.d_write(d, r) }
        Form::Nmls => { let p = soft::mul(&mut ctx, file.d_read(n), file.d_read(m)); let r = soft::add(&mut ctx, soft::neg(file.d_read(d)), p); file.d_write(d, r) }
        Form::Fma => { let r = soft::fma(&mut ctx, file.d_read(d), file.d_read(n), file.d_read(m)); file.d_write(d, r) }
        Form::Fms => { let r = soft::fma(&mut ctx, file.d_read(d), soft::neg(file.d_read(n)), file.d_read(m)); file.d_write(d, r) }
        Form::Fnma => { let r = soft::fma(&mut ctx, soft::neg(file.d_read(d)), soft::neg(file.d_read(n)), file.d_read(m)); file.d_write(d, r) }
        Form::Fnms => { let r = soft::fma(&mut ctx, soft::neg(file.d_read(d)), file.d_read(n), file.d_read(m)); file.d_write(d, r) }
        Form::MaxNm => { let r = soft::max_min_num(&mut ctx, file.d_read(n), file.d_read(m), true); file.d_write(d, r) }
        Form::MinNm => { let r = soft::max_min_num(&mut ctx, file.d_read(n), file.d_read(m), false); file.d_write(d, r) }
        Form::Sel(cond) => {
            let (nf, zf, cf, vf) = (xpsr >> 31 & 1 != 0, xpsr >> 30 & 1 != 0, xpsr >> 29 & 1 != 0, xpsr >> 28 & 1 != 0);
            let _ = cf;
            let take = match cond {
                SelCond::Eq => zf,
                SelCond::Vs => vf,
                SelCond::Ge => nf == vf,
                SelCond::Gt => !zf && nf == vf,
            };
            let r = if take { file.d_read(n) } else { file.d_read(m) };
            file.d_write(d, r)
        }
        Form::Mov => { let r = file.d_read(m); file.d_write(d, r) }
        Form::Abs => { let r = soft::abs(file.d_read(m)); file.d_write(d, r) }
        Form::Neg => { let r = soft::neg(file.d_read(m)); file.d_write(d, r) }
        Form::Sqrt => { let r = soft::sqrt(&mut ctx, file.d_read(m)); file.d_write(d, r) }
        Form::Cmp { e, zero } => {
            let b = if zero { 0 } else { file.d_read(m) };
            let nzcv = soft::compare(&mut ctx, file.d_read(d), b, e);
            file.fpscr = (file.fpscr & 0x0FFF_FFFF) | nzcv;
        }
        Form::Rint { rounding, exact } => {
            let dir = resolve(rounding, &ctx);
            let r = soft::round_to_integral(&mut ctx, file.d_read(m), dir, exact);
            file.d_write(d, r)
        }
        Form::ToF32 => { let r = soft::to_f32(&mut ctx, file.d_read(m)); file.s_write(dec.sd, r) }
        Form::FromF32 => { let r = soft::from_f32(&mut ctx, file.s_read(dec.sm)); file.d_write(d, r) }
        Form::ToInt { signed, rounding } => {
            let dir = resolve(rounding, &ctx);
            let r = soft::to_int32(&mut ctx, file.d_read(m), signed, dir);
            file.s_write(dec.sd, r)
        }
        Form::FromInt { signed } => { let r = soft::from_int32(&mut ctx, file.s_read(dec.sm), signed); file.d_write(d, r) }
        Form::ToFixed { signed, bits, fbits } => {
            let r = soft::to_fixed(&mut ctx, file.d_read(d), signed, bits, fbits);
            let w = if signed { (((r << (32 - bits)) as i32) >> (32 - bits)) as i64 as u64 } else { r as u64 };
            file.d_write(d, w)
        }
        Form::FromFixed { signed, bits, fbits } => {
            let r = soft::from_fixed(&mut ctx, file.d_read(d) as u32, signed, bits, fbits);
            file.d_write(d, r)
        }
        Form::FromF16 { top } => {
            let w = file.s_read(dec.sm);
            let h = if top { w >> 16 } else { w & 0xFFFF };
            let r = soft::from_f16(&mut ctx, h);
            file.d_write(d, r)
        }
        Form::ToF16 { top } => {
            let h = soft::to_f16(&mut ctx, file.d_read(m));
            let w = file.s_read(dec.sd);
            let w2 = if top { (w & 0x0000_FFFF) | (h << 16) } else { (w & 0xFFFF_0000) | h };
            file.s_write(dec.sd, w2)
        }
        Form::MovImm(v) => file.d_write(d, v),
    }
    file.fpscr = ctx.merge_into(file.fpscr);
}

/// A small code per form for the witness's last-form word.
fn form_code(form: Form) -> u32 {
    match form {
        Form::Add => 1, Form::Sub => 2, Form::Mul => 3, Form::Nmul => 4, Form::Div => 5,
        Form::Mla => 6, Form::Mls => 7, Form::Nmla => 8, Form::Nmls => 9,
        Form::Fma => 10, Form::Fms => 11, Form::Fnma => 12, Form::Fnms => 13,
        Form::MaxNm => 14, Form::MinNm => 15, Form::Sel(_) => 16,
        Form::Mov => 17, Form::Abs => 18, Form::Neg => 19, Form::Sqrt => 20,
        Form::Cmp { e: false, .. } => 21, Form::Cmp { e: true, .. } => 22,
        Form::Rint { .. } => 23, Form::ToF32 => 24, Form::FromF32 => 25,
        Form::ToInt { .. } => 26, Form::FromInt { .. } => 27,
        Form::ToFixed { .. } => 28, Form::FromFixed { .. } => 29,
        Form::FromF16 { .. } => 30, Form::ToF16 { .. } => 31, Form::MovImm(_) => 32,
    }
}

/// The name of a form for the funnel's refusal line, from the halfwords at a pc.
pub fn name_at(pc: u32) -> &'static str {
    if pc & 1 != 0 {
        return "?";
    }
    let hw1 = unsafe { read_volatile(pc as *const u16) };
    let hw2 = unsafe { read_volatile((pc + 2) as *const u16) };
    match decode::decode(hw1, hw2) {
        Some(d) => decode::name(d.form),
        None => "?",
    }
}
