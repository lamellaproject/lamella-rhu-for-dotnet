//! Kernel console: the board module owns the device; this module owns
//! CRLF wire discipline, `core::fmt` plumbing, and line locking. Polled
//! TX; every kprintln holds the kernel critical section for the whole
//! formatted write, so concurrent contexts never tear a line.

use core::fmt;

use crate::board;

pub fn init() {
    board::console_init();
}

pub fn write_bytes(bytes: &[u8]) {
    for &b in bytes {
        if b == b'\n' {
            board::console_putb(b'\r');
        }
        board::console_putb(b);
    }
}

/// Zero-sized writer: `core::fmt` plumbing for the kprintln macro.
pub struct Out;

impl fmt::Write for Out {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        write_bytes(s.as_bytes());
        Ok(())
    }
}

/// Run `f` with exclusive console access (kernel critical section).
pub fn locked<R>(f: impl FnOnce(&mut Out) -> R) -> R {
    crate::critical::with_cs(|| f(&mut Out))
}

macro_rules! kprintln {
    () => {{
        $crate::console::locked(|_out| $crate::console::write_bytes(b"\n"));
    }};
    ($($arg:tt)*) => {{
        $crate::console::locked(|out| {
            use ::core::fmt::Write;
            let _ = ::core::writeln!(out, $($arg)*);
        });
    }};
}
