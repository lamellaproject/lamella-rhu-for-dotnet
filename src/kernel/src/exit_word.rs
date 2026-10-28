//! The exit word: how a reader learns that the program ended, and how.
//! The console mirror's header carries it in two of its words: the magic
//! at offset 0 of `__rhu_console_log`, which reads "RLOG" while the
//! program runs and an end magic once it ended ("RRET" returned, "RABT"
//! aborted), and the exit code at offset 12, whole, 32 bits, 0 while the
//! program runs. The exit paths store the code first, then after a barrier
//! the end magic in ONE store, after the transcript's last byte and its
//! count, so a reader over the debug port that sees an end magic sees the
//! code and the whole transcript, with no halt and no serial line. No line
//! on the wire says the end: a reader takes it from the header. This
//! module is the pure half: the three states, their magics, the decoding
//! of a header's two words, the states' names. The store into the mirror
//! is `board/ring.rs`, the exit paths are `semihost.rs`.
//!
//! THE TWO WORDS, little-endian:
//!   offset 0   the magic: "RLOG" (0x474F4C52) running; "RRET" (0x54455252)
//!              returned; "RABT" (0x54424152) aborted; anything else is
//!              not an armed mirror (0 between the boot's clear and its
//!              "RLOG")
//!   offset 12  the exit code, a 32-bit two's complement integer, whole:
//!              0 while the program runs, the door's code at its end
//! THE STATES: running -- the boot clears the magic, writes the other
//! words and writes "RLOG" last, so a mirror retained from the last run
//! never reads as this one; returned -- the program returned or called the
//! exit door `rhu_exit`, and the code is the program's own; aborted -- the
//! program ended without returning: the hosted runtime ended it after an
//! exception nothing caught or a fail-fast (the door `rhu_abort`),
//! or the kernel's fault funnel ended the run after a panic or a processor
//! fault; the code is the ending path's.

/// The program runs: the boot's state, announced by "RLOG".
pub const STATE_RUNNING: u32 = 0;
/// The program returned or called the exit door; the code is its own.
pub const STATE_RETURNED: u32 = 1;
/// The program ended without returning; the code is the ending path's.
pub const STATE_ABORTED: u32 = 2;

/// "RLOG": the mirror armed and the program running.
pub const MAGIC_RUNNING: u32 = u32::from_le_bytes(*b"RLOG");
/// "RRET": the program returned; the code at offset 12 is its own.
pub const MAGIC_RETURNED: u32 = u32::from_le_bytes(*b"RRET");
/// "RABT": the program ended without returning; the code at offset 12 is
/// the ending path's.
pub const MAGIC_ABORTED: u32 = u32::from_le_bytes(*b"RABT");

/// The magic's offset in the mirror's header, in bytes.
#[allow(dead_code)] // a reader's fact: the host tier's cases and the api flavor's check pin it
pub const MAGIC_OFFSET: usize = 0;
/// The exit code's offset in the mirror's header, in bytes.
#[allow(dead_code)] // a reader's fact: the host tier's cases and the api flavor's check pin it
pub const CODE_OFFSET: usize = 12;

/// The magic a state is announced by.
pub const fn magic(state: u32) -> u32 {
    match state {
        STATE_RUNNING => MAGIC_RUNNING,
        STATE_RETURNED => MAGIC_RETURNED,
        _ => MAGIC_ABORTED,
    }
}

/// The state and the code a header's magic and exit code carry, or None
/// for a magic outside the contract (a mirror not armed, or one retained
/// from the last run between the boot's clear and its "RLOG"). While the
/// program runs the code is 0 by the boot, and a code beside "RLOG" is the
/// instant between an end's two stores: a reader takes a code only beside
/// an end magic.
#[allow(dead_code)] // a reader's half: the host tier's cases and the api flavor's check
pub const fn decode(magic: u32, code: i32) -> Option<(u32, i32)> {
    match magic {
        MAGIC_RUNNING => Some((STATE_RUNNING, 0)),
        MAGIC_RETURNED => Some((STATE_RETURNED, code)),
        MAGIC_ABORTED => Some((STATE_ABORTED, code)),
        _ => None,
    }
}

/// The state's name, the contract's word for its magic.
#[allow(dead_code)] // the api flavor's check and the host tier's cases; no row's boot path names a state
pub const fn name(state: u32) -> &'static str {
    match state {
        STATE_RUNNING => "running",
        STATE_RETURNED => "returned",
        _ => "aborted",
    }
}
