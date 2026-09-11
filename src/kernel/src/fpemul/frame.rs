//! The basic-to-extended frame promotion. A thread that faulted with a basic
//! frame had no floating-point context; after the emulation it needs one, so
//! its next context switch saves and restores the registers the result
//! landed in. The hardware would have created that context at the thread's
//! first successful floating-point instruction by allocating the extended
//! frame at the next exception; this module creates it now: the eight
//! hardware words move down by the floating-point area's size, S0-S15 are
//! written from the fresh context, FPSCR from the default the hardware
//! itself uses for a new context, the reserved word zero, the process stack
//! pointer moves to the new frame, and the caller returns with EXC_RETURN
//! bit 4 clear so the hardware unstacks the context and sets CONTROL.FPCA.
//! The stack alignment bit in xPSR is untouched: the pad, if any, stays
//! above the frame.

use core::ptr::{read_volatile, write_volatile};

use crate::funnel::ExcFrame;

/// The extended frame's size (the eight words, S0-S15, FPSCR, the reserved word).
pub const EXTENDED_BYTES: usize = 0x68;
pub const BASIC_BYTES: usize = 0x20;
const FP_AREA_BYTES: usize = EXTENDED_BYTES - BASIC_BYTES;

/// Promote the basic frame at `frame` to an extended one carrying `s` and
/// `fpscr`, and point the process stack at it. Refuses when the thread's
/// stack limit would be crossed (the architecture refuses stacking below the
/// limit; the handler refuses ahead of it).
pub unsafe fn promote(frame: *mut ExcFrame, s: &[u32; 16], fpscr: u32) -> Result<(), &'static str> {
    let old = frame as usize;
    let new = old - FP_AREA_BYTES;
    #[cfg(armv8m)]
    {
        let limit: u32;
        unsafe { core::arch::asm!("mrs {}, PSPLIM", out(reg) limit, options(nomem, nostack, preserves_flags)) };
        if new < limit as usize {
            return Err("the promoted frame would cross the thread's stack limit");
        }
    }
    if !crate::board::FRAME_RANGES.iter().any(|&(lo, hi)| new >= lo && new + EXTENDED_BYTES <= hi) {
        return Err("the promoted frame outside the thread ranges");
    }
    let mut words = [0u32; 8];
    for (i, w) in words.iter_mut().enumerate() {
        *w = unsafe { read_volatile((old + 4 * i) as *const u32) };
    }
    for (i, w) in words.iter().enumerate() {
        unsafe { write_volatile((new + 4 * i) as *mut u32, *w) };
    }
    for (i, v) in s.iter().enumerate() {
        unsafe { write_volatile((new + BASIC_BYTES + 4 * i) as *mut u32, *v) };
    }
    unsafe {
        write_volatile((new + 0x60) as *mut u32, fpscr);
        write_volatile((new + 0x64) as *mut u32, 0);
        core::arch::asm!("msr PSP, {}", in(reg) new as u32, options(nomem, nostack, preserves_flags));
    }
    Ok(())
}

/// The frame the return will use: the promoted one when EXC_RETURN says
/// extended and the original was basic, else the original.
pub unsafe fn current(frame: *mut ExcFrame, exc_return_out: u32) -> *mut ExcFrame {
    let psp: u32;
    unsafe { core::arch::asm!("mrs {}, PSP", out(reg) psp, options(nomem, nostack, preserves_flags)) };
    let _ = exc_return_out;
    if psp as usize == frame as usize - FP_AREA_BYTES { psp as *mut ExcFrame } else { frame }
}
