//! The virtual GPIO backend of the emulator rows: latches and a loopback
//! pair in kernel memory, so the doors, the claims table, the refusals, the
//! event counting, the coalescing and the futex recipe are gated on QEMU
//! by the same verbs the boards serve, and only the real interrupt path is
//! left to silicon. Line 2 is an output wired to line 3: a write on 2 sets
//! 3's level and, when the two differ, fires 3's event in thread context
//! (the family handlers do the same from the board band). The doctored
//! control `test-gpio-doctored-latch` fires the event twice per change --
//! the emulator rows' control, which the api flavor's edge check must
//! refuse (the silicon control is the removed pending-mask clear, a
//! hardware-side control).

use core::sync::atomic::{AtomicU8, Ordering};

use crate::board::facts;
use crate::gpio;

pub const HAS_EVENTS: bool = true;

static LEVEL: [AtomicU8; facts::LINE_COUNT] = [const { AtomicU8::new(0) }; facts::LINE_COUNT];

pub fn configure(line: u32, mode: u32, pull: u32, initial: bool) {
    let level = if mode != crate::gpio_math::MODE_INPUT {
        initial as u8
    } else if line == facts::LOOP_IN {
        LEVEL[facts::LOOP_OUT as usize].load(Ordering::Acquire)
    } else if pull == crate::gpio_math::PULL_UP {
        1
    } else {
        0
    };
    LEVEL[line as usize].store(level, Ordering::Release);
    if line == facts::LOOP_OUT && mode != crate::gpio_math::MODE_INPUT {
        LEVEL[facts::LOOP_IN as usize].store(level, Ordering::Release);
    }
}

pub fn release(_line: u32) {}

pub fn write(line: u32, _mode: u32, high: bool) {
    let v = high as u8;
    LEVEL[line as usize].store(v, Ordering::Release);
    if line == facts::LOOP_OUT {
        let old = LEVEL[facts::LOOP_IN as usize].swap(v, Ordering::AcqRel);
        if old != v {
            gpio::fire(facts::LOOP_IN, high);
            #[cfg(any(feature = "test-gpio-doctored-latch", feature = "test-gpio-doctored-edge"))]
            doctored(high);
        }
    }
}

pub fn read(line: u32) -> bool {
    LEVEL[line as usize].load(Ordering::Acquire) != 0
}

pub fn arm(_line: u32, _rising: bool, _falling: bool) -> i32 {
    0
}

pub fn disarm(_line: u32) {}

/// No software trigger on the virtual backend: the loop is the trigger.
pub fn software_edge(_line: u32) -> bool {
    false
}

/// The latch control's extra event: every change of the loop fires twice.
#[cfg(feature = "test-gpio-doctored-latch")]
fn doctored(high: bool) {
    gpio::fire(facts::LOOP_IN, high);
}

/// The edge control's extra event: ONE event of the backend's own, beside the
/// tenth edge the doors counted and no other -- an edge the program did not
/// make, which a check's total must refuse.
#[cfg(all(feature = "test-gpio-doctored-edge", not(feature = "test-gpio-doctored-latch")))]
fn doctored(high: bool) {
    let counted = unsafe { (*core::ptr::addr_of!(gpio::WITNESS)).edges_counted };
    if counted == 10 {
        gpio::fire(facts::LOOP_IN, high);
    }
}
