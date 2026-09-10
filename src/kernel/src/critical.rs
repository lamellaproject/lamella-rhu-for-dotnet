//! BASEPRI-only critical sections. PRIMASK is never touched: interrupt
//! priorities numerically below the ceiling (0x00..0x3F, the
//! zero-latency band) stay live even inside a kernel critical section.

use core::arch::asm;
use core::sync::atomic::{compiler_fence, Ordering};

/// Kernel masking ceiling: BASEPRI at this value masks kernel-band
/// interrupt priorities and leaves 0x00..0x3F unmasked.
pub const BASEPRI_CEILING: u32 = 0x40;

/// Run `f` with kernel-band interrupts masked. Nests safely: the entry
/// raises the mask with the monotone form (BASEPRI_MAX only ever
/// tightens, so a section entered under a stricter mask never loosens
/// it) and the exit restores the value read at entry. Single-core: this
/// is also the kernel's lock primitive.
#[inline(always)]
pub fn with_cs<R>(f: impl FnOnce() -> R) -> R {
    let old: u32;
    unsafe {
        asm!("mrs {}, BASEPRI", out(reg) old, options(nomem, nostack));
        asm!("msr BASEPRI_MAX, {}", in(reg) BASEPRI_CEILING, options(nomem, nostack));
    }
    compiler_fence(Ordering::SeqCst);
    let r = f();
    compiler_fence(Ordering::SeqCst);
    unsafe {
        asm!("msr BASEPRI, {}", in(reg) old, options(nomem, nostack));
    }
    r
}
