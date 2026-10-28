//! The handler stack's watermark: the boot stack reserve -- the main
//! stack pointer's region between the arena window's high edge and the
//! stack's top, the boot's stack and then every handler's -- filled with a
//! pattern word at reset and read back at the program's end as its deepest
//! use. Off in a shipped image: the measurement runs when the image names
//! the kernel feature `stack-watermark` (the gate flavors and the kernel's
//! own test images) or when its link-time word `__rhu_stack_watermark` is
//! nonzero (`PROVIDE(... = 0)` in every link and payload script; a project's
//! link passes --defsym to turn it on). The read-out is one verbose line of
//! the log stream, printed by the exit paths after the tick census line.

use core::arch::global_asm;
use core::ptr::{addr_of, read_volatile};

/// The pattern word the fill writes and the scan looks for.
const PATTERN: u32 = 0xA5A5_5A5A;

unsafe extern "C" {
    static __rhu_stack_watermark: u8;
    static __stack_start: u32;
    static __rhu_arena_hi: u32;
    fn __rhu_stack_fill(floor: *mut u32, pattern: u32);
}

// The fill routine, a leaf that reads the stack pointer itself and uses no
// stack: every word from the floor up to the pointer it reads is free (the
// caller's frames sit above it), so no frame of its own can be overwritten.
// A compiled function cannot promise that: its prologue may be placed after
// the pointer is read, and the frame it then pushes lies inside the fill.
global_asm!(
    r#"
    .section .text.__rhu_stack_fill, "ax", %progbits
    .global __rhu_stack_fill
    .type __rhu_stack_fill, %function
    .thumb_func
__rhu_stack_fill:
    /* r0 = the floor (word aligned), r1 = the pattern: [r0, sp) filled. */
    mov  r2, sp
    bic  r2, r2, #3
1:  cmp  r0, r2
    bhs  2f
    str  r1, [r0], #4
    b    1b
2:  bx   lr
    .size __rhu_stack_fill, . - __rhu_stack_fill
"#
);

/// Whether this image measures: the feature, or the link-time word nonzero.
#[inline(always)]
pub fn on() -> bool {
    cfg!(feature = "stack-watermark") || link_word!(__rhu_stack_watermark) != 0
}

/// The reserve: its floor (the arena window's high edge, the limit
/// register's value on the v8-M rows) and its top (the stack's start, the
/// reset vector's stack pointer).
fn bounds() -> (usize, usize) {
    (addr_of!(__rhu_arena_hi) as usize, addr_of!(__stack_start) as usize)
}

/// The fill, from the current main stack pointer down to the floor: the
/// first act of the reset path, before the statics are copied and zeroed
/// and before any deep call, so every word the boot and the handlers will
/// use holds the pattern. Nothing above the pointer the leaf reads is
/// touched (the reset path's own frame lives there). The doctored control
/// skips it.
pub fn fill() {
    if !on() || cfg!(feature = "test-stack-watermark-doctored") {
        return;
    }
    let (floor, _top) = bounds();
    unsafe { __rhu_stack_fill(floor as *mut u32, PATTERN) };
}

/// The high-water: the scan from the floor upward to the first word that
/// is not the pattern; the bytes from there to the top are the deepest use
/// so far, beside the reserve's size. None when the image does not measure.
pub fn high_water() -> Option<(u32, u32)> {
    if !on() {
        return None;
    }
    let (floor, top) = bounds();
    let mut p = floor as *const u32;
    let end = top as *const u32;
    while p < end && unsafe { read_volatile(p) } == PATTERN {
        p = unsafe { p.add(1) };
    }
    Some(((top - p as usize) as u32, (top - floor) as u32))
}

/// The read-out line (the exit paths print it after the census line).
pub fn report() {
    if let Some((used, reserve)) = high_water() {
        kprintln!("[stack] handler stack high-water: {} of {} bytes (the boot stack reserve, filled with a pattern at reset and read back here)", used, reserve);
    }
}
