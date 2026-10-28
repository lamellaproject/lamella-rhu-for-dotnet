//! The Cortex-M7's instruction cache on the STM32F769 row: turned on from
//! kmain under the execute-in-place map alone (the quad-SPI window's fetch
//! path is a serial bus the cache hides on hot code; under a map that
//! declares no window the row's caches stay off, as before), the data cache
//! left off (the device errata ES0334 2.1.1 against the data cache's
//! write-through posture; the heap, the network descriptors and the shim's
//! thunks rest on the uncached data side). The sequence per the Cortex-M7
//! technical reference manual's L1 cache section: the whole instruction
//! cache invalidated, a data barrier, the enable bit, a data barrier and an
//! instruction barrier so every later fetch sees the cache. The branch
//! predictor's bit is read back and left as found. The state is read back
//! on the boot line and asserted.

use core::arch::asm;
use core::fmt;
use core::ptr::{read_volatile, write_volatile};

/// The configuration and control register (ARMv7-M B3.2.8 CCR): DC [16]
/// the data cache enable, IC [17] the instruction cache enable, BP [18]
/// the branch prediction enable.
const SCB_CCR: *mut u32 = 0xE000_ED14 as *mut u32;
const CCR_DC: u32 = 1 << 16;
const CCR_IC: u32 = 1 << 17;
const CCR_BP: u32 = 1 << 18;
/// The instruction cache invalidate-all register (ARMv7-M B3.2, the cache
/// maintenance operations at 0xE000EF50 onward; a write of any value).
const ICIALLU: *mut u32 = 0xE000_EF50 as *mut u32;

/// The boot step: the instruction cache on where `on` says so, the data
/// cache untouched (off), the state read back and asserted.
pub fn boot(on: bool) {
    if on {
        unsafe {
            write_volatile(ICIALLU, 0);
            asm!("dsb", options(nostack, preserves_flags));
            write_volatile(SCB_CCR, read_volatile(SCB_CCR) | CCR_IC);
            asm!("dsb", "isb", options(nostack, preserves_flags));
        }
    }
    let (ic, dc, _) = posture();
    assert!(ic == on && !dc, "icache: CCR reads IC {} DC {} after the boot's cache step (IC {} wanted, DC off)", ic as u32, dc as u32, on as u32);
}

/// The caches' state read back from the core: the instruction cache, the
/// data cache, the branch predictor.
pub fn posture() -> (bool, bool, bool) {
    let c = unsafe { read_volatile(SCB_CCR) };
    (c & CCR_IC != 0, c & CCR_DC != 0, c & CCR_BP != 0)
}

/// The boot line's words after "[boot] icache: ", from the read-back.
pub struct Banner;

impl fmt::Display for Banner {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (ic, dc, bp) = posture();
        if ic {
            write!(f, "on (CCR.IC 1, CCR.DC {}, CCR.BP {}; the cache invalidated before the enable)", dc as u32, bp as u32)
        } else {
            write!(f, "off (CCR.IC 0, CCR.DC {}, CCR.BP {})", dc as u32, bp as u32)
        }
    }
}
