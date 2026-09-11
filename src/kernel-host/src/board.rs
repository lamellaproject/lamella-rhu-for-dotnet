//! The host board: the row facts the arithmetic modules read (the emulator
//! rows' kernel-heap carve) and the arena's window over a host buffer of
//! one megabyte, 8-aligned like a map's window.

/// The boot kheap carve out of the arena (the emulator rows' value).
pub const KHEAP_BYTES: usize = 64 * 1024;

const WINDOW_BYTES: usize = 1024 * 1024;

#[repr(align(8))]
struct Window([u8; WINDOW_BYTES]);

static mut WINDOW: Window = Window([0; WINDOW_BYTES]);

/// The arena's window: the host buffer's bounds.
pub fn arena_window() -> (usize, usize) {
    let lo = core::ptr::addr_of!(WINDOW) as usize;
    (lo, lo + WINDOW_BYTES)
}
