//! Kernel console: the board module owns the device; this module owns
//! CRLF wire discipline, `core::fmt` plumbing, and line locking. Every
//! kprintln holds the kernel critical section for the whole formatted
//! write, so concurrent contexts never tear a line; a board driven the
//! ring way takes the bytes without pacing the section by the wire,
//! a polled board (the emulator rows) paces it.

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

/// The write door's plumbing: the WHOLE buffer under one console
/// lock, so a hosted writer never tears against a kprintln line (the
/// same line-atomicity every kprintln already buys; callers size
/// their lines accordingly). Wire grammar applies. Back-pressure runs
/// OUTSIDE the lock: the door waits, with interrupts open, until the
/// board's console has room for the write's wire length (the bytes
/// plus one per newline), takes the lock, re-checks the room under it
/// and writes -- a writer that outruns the wire waits on the
/// transmitter's drain, never inside the critical section, and never
/// longer than a bounded write takes to drain (a polled board answers
/// unbounded room and never waits here).
pub fn door_write(bytes: &[u8]) {
    let wire = bytes.len() + bytes.iter().filter(|&&b| b == b'\n').count();
    loop {
        while board::console_room() < wire {
            core::hint::spin_loop();
        }
        let written = locked(|_out| {
            if board::console_room() >= wire {
                write_bytes(bytes);
                true
            } else {
                false
            }
        });
        if written {
            return;
        }
    }
}

/// The read door's plumbing: drain pending receiver bytes -- up to
/// the buffer's length, RAW (no echo, no translation) -- and return
/// the count. Polls only; a board with nothing pending answers 0
/// immediately. CS-bounded like every other verb (bounded by the
/// buffer and the receiver's shallow hardware depth).
pub fn door_read(buf: &mut [u8]) -> usize {
    crate::critical::with_cs(|| {
        let mut i = 0;
        while i < buf.len() {
            match board::console_getb() {
                Some(b) => {
                    buf[i] = b;
                    i += 1;
                }
                None => break,
            }
        }
        i
    })
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
