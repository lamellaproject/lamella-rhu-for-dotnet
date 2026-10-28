//! The console mirror and the console wire ring. THE MIRROR (`__rhu_console_log`,
//! 16 KB in SRAM) is a TEE on every row: every byte of either stream lands
//! in it whatever the stream's channel says -- the transcript a debug probe
//! or a debugger reads out of RAM by symbol, with its layout and its reader
//! protocol
//! unchanged by the UART doors. THE WIRE RING, on
//! the rows whose wire is driven the ring way (the ST rows' probe port),
//! takes the same bytes after the mirror: the board's
//! transmitter is primed from it while it has room (a bounded register loop,
//! never a wait on the wire), and the transmitter's interrupt drains the rest
//! as the wire takes it, at the lowest interrupt priority band. A writer
//! holding the kernel critical section is never paced by the wire, so the
//! tick is never masked across a character time. The panic and fault paths
//! switch the driver to a polled fallback that pushes the whole wire ring
//! through synchronously, so a report is complete on the wire before the
//! machine parks. The emulator rows carry the mirror alone (their default
//! wire is the polled CMSDK UART0 the board module drives); on the rows with
//! a UART table a stream's UART wire is a port the UART doors' module holds
//! (`uart.rs`: its own rings and interrupt, this record's counts kept).

use core::ptr::{addr_of, addr_of_mut, read_volatile, write_volatile};

#[cfg(rhu_console_probe)]
use crate::board;

/// The console mirror's buffer: twice the largest flavor transcript.
pub const CONSOLE_LOG_BYTES: usize = 16 * 1024;
/// "RLOG" in a byte dump.
pub const CONSOLE_LOG_MAGIC: u32 = crate::exit_word::MAGIC_RUNNING;

/// The console mirror: every byte either stream writes is appended here
/// first, so a debug probe reads the whole transcript out of RAM over the
/// debug port -- no serial wiring needed, nothing lost at reset. Layout
/// (little-endian words): the magic, the buffer size, the count of bytes
/// written since the clock came up (a count past the size means the buffer
/// wrapped and the earliest bytes are gone -- exactly the oldest
/// `written - size` bytes at the moment of a read), the exit code (0
/// while the program runs; the door's code, whole, stored by the exit paths
/// after the transcript's last byte and before the magic turns into the end
/// state's "RRET" or "RABT" -- `exit_word.rs` states the three magics), then the bytes. The writer lands a byte before it
/// moves the count; it never waits for a reader. The block sits in its own section after the thread
/// stacks and outside the kernel's zero-fill and statics windows (a
/// conservative statics scan never walks transcript text); the board's
/// `clock_init` writes the header once per boot through `boot_mark` (the
/// emulator rows' console init through `ensure_armed`), and a console
/// re-init on the panic path leaves it alone so a panic transcript appends
/// to the boot's.
#[repr(C)]
pub struct ConsoleLog {
    pub magic: u32,
    pub size: u32,
    pub written: u32,
    pub exit_code: u32,
    pub bytes: [u8; CONSOLE_LOG_BYTES],
}

#[unsafe(no_mangle)]
#[unsafe(link_section = ".rhu_console_log")]
pub static mut __rhu_console_log: ConsoleLog = ConsoleLog {
    magic: 0,
    size: 0,
    written: 0,
    exit_code: 0,
    bytes: [0; CONSOLE_LOG_BYTES],
};

/// The wire ring's state, beside the mirror for a debug probe to read by
/// symbol: the driver mode (0 = not started; 1 = the transmit interrupt
/// drains the ring; 2 = polled, the mode the panic and fault paths switch
/// to; 3 = no wire on this image), the count of bytes handed to the
/// transmitter, the count of bytes the wire never carried because the
/// writer lapped the wire ring, and the
/// count of transmit interrupts taken. Plain statics in the zero-fill
/// window: written by the producer under the kernel critical section and
/// by the interrupt that section masks, so the two halves never interleave;
/// the room query reads the two counts outside the section (each an aligned
/// word, read whole) and its answer is re-checked under the section before
/// a write commits.
#[repr(C)]
pub struct ConsoleRing {
    pub mode: u32,
    pub drained: u32,
    pub dropped: u32,
    pub irqs: u32,
}

#[unsafe(no_mangle)]
pub static mut __rhu_console_ring: ConsoleRing = ConsoleRing {
    mode: 0,
    drained: 0,
    dropped: 0,
    irqs: 0,
};

/// The interrupt mode: the transmit interrupt drains the wire ring.
#[allow(dead_code)] // the emulator rows carry the mirror alone
pub const MODE_IRQ: u32 = 1;
/// The polled mode: the wire ring is drained to the wire synchronously.
#[allow(dead_code)] // the emulator rows carry the mirror alone
pub const MODE_POLLED: u32 = 2;
/// No wire: the mirror alone (the memory console on both streams).
#[allow(dead_code)] // the emulator rows carry the mirror alone
pub const MODE_NO_WIRE: u32 = 3;

/// Set by the board's `clock_init` (the boot path only) through
/// `boot_mark` and consumed by the first `console_init` after it: that
/// call is the boot's and arms the transmit interrupt; any later call is
/// a re-init from the panic or fault path and takes the polled mode.
#[allow(dead_code)] // the emulator rows carry the mirror alone
static mut BOOT_PENDING: u32 = 0;

/// The boot path's mark: the mirror's header written and the console init
/// that follows marked as the boot's.
#[allow(dead_code)] // the emulator rows carry the mirror alone
pub fn boot_mark() {
    arm();
    unsafe { write_volatile(addr_of_mut!(BOOT_PENDING), 1) }
}

/// Consume the boot mark: true for the one console init that follows the
/// boot's `clock_init`, false for every re-init after it.
#[allow(dead_code)] // the emulator rows carry the mirror alone
pub fn take_boot_mark() -> bool {
    unsafe {
        let pending = read_volatile(addr_of!(BOOT_PENDING)) != 0;
        if pending {
            write_volatile(addr_of_mut!(BOOT_PENDING), 0);
        }
        pending
    }
}

/// The driver mode as the ring records it (0 before the first init).
#[allow(dead_code)] // the emulator rows carry the mirror alone
pub fn mode() -> u32 {
    unsafe { read_volatile(addr_of!((*addr_of!(__rhu_console_ring)).mode)) }
}

/// Record the driver mode the board's init settled on.
#[allow(dead_code)] // the emulator rows carry the mirror alone
pub fn set_mode(mode: u32) {
    unsafe { write_volatile(addr_of_mut!((*addr_of_mut!(__rhu_console_ring)).mode), mode) }
}

/// Write the mirror's header: the magic cleared first (a mirror retained
/// from the last run, ended or not, reads as no mirror while the words are
/// written), then the count, the exit code and the size, then "RLOG" last
/// after a barrier, so a reader that sees the magic sees this boot's words
/// beside it.
fn arm() {
    unsafe {
        let log = addr_of_mut!(__rhu_console_log);
        write_volatile(addr_of_mut!((*log).magic), 0);
        write_volatile(addr_of_mut!((*log).written), 0);
        write_volatile(addr_of_mut!((*log).exit_code), 0);
        write_volatile(addr_of_mut!((*log).size), CONSOLE_LOG_BYTES as u32);
        core::sync::atomic::fence(core::sync::atomic::Ordering::SeqCst);
        write_volatile(addr_of_mut!((*log).magic), CONSOLE_LOG_MAGIC);
    }
}

/// The mirror's header written where no boot path has written it (the
/// emulator rows, whose clock init marks nothing); a header already
/// present is left alone so a transcript keeps appending.
pub fn ensure_armed() {
    unsafe {
        if read_volatile(addr_of!((*addr_of!(__rhu_console_log)).magic)) != CONSOLE_LOG_MAGIC {
            arm();
        }
    }
}

/// Append one byte to the mirror (the byte lands before the count moves,
/// so a reader never sees a count that runs past written bytes). The tee
/// every stream's bytes pass through on every row.
pub fn mirror(b: u8) {
    unsafe {
        let log = addr_of_mut!(__rhu_console_log);
        let written = read_volatile(addr_of!((*log).written));
        let bytes = addr_of_mut!((*log).bytes) as *mut u8;
        write_volatile(bytes.add(written as usize % CONSOLE_LOG_BYTES), b);
        write_volatile(addr_of_mut!((*log).written), written.wrapping_add(1));
    }
}

/// The program's end stored into the mirror's header: the exit code,
/// whole, at offset 12 first, then after a barrier the end magic in ONE
/// store at offset 0 in place of "RLOG" -- so a reader that sees the end
/// magic sees the code, every byte and the count written before it. The
/// exit paths call it after the transcript's last byte; the api flavor's
/// check calls it and restores the running state by `mark_running`.
pub fn end_mark(state: u32, code: i32) {
    unsafe {
        let log = addr_of_mut!(__rhu_console_log);
        write_volatile(addr_of_mut!((*log).exit_code), stored(code) as u32);
        core::sync::atomic::fence(core::sync::atomic::Ordering::SeqCst);
        write_volatile(addr_of_mut!((*log).magic), crate::exit_word::magic(state));
    }
}

/// The running state put back after the api flavor's check: the code 0,
/// a barrier, "RLOG".
#[allow(dead_code)] // the api flavor's check
pub fn mark_running() {
    unsafe {
        let log = addr_of_mut!(__rhu_console_log);
        write_volatile(addr_of_mut!((*log).exit_code), 0);
        core::sync::atomic::fence(core::sync::atomic::Ordering::SeqCst);
        write_volatile(addr_of_mut!((*log).magic), CONSOLE_LOG_MAGIC);
    }
}

/// The header's magic and exit code as it holds them (the api flavor's
/// check reads them back by symbol).
#[allow(dead_code)] // the api flavor's check
pub fn end_words() -> (u32, i32) {
    unsafe {
        let log = addr_of!(__rhu_console_log);
        (read_volatile(addr_of!((*log).magic)), read_volatile(addr_of!((*log).exit_code)) as i32)
    }
}

/// The code as stored: itself, whole.
#[cfg(not(feature = "test-exit-word-doctored"))]
fn stored(code: i32) -> i32 {
    code
}
/// The doctored store (the api gate's control, never a shipped image): the
/// code's low 16 bits alone, sign-extended, so a code past 16 bits reads
/// back truncated and the check's read-back disagrees with the door's code.
#[cfg(feature = "test-exit-word-doctored")]
fn stored(code: i32) -> i32 {
    (code as i16) as i32
}

/// The wire record's counts for a wire the UART doors' module drives (a
/// console-held UART port on the rows with a UART table): the bytes
/// handed to the transmitter, the bytes lost to a lapped transmit ring, the
/// interrupts taken -- so `__rhu_console_ring` reads the same by symbol
/// whichever driver carries the wire.
#[allow(dead_code)] // the probe rows' wire is this module's own ring
pub fn account_drained(n: u32) {
    unsafe {
        let ring = addr_of_mut!(__rhu_console_ring);
        let d = read_volatile(addr_of!((*ring).drained));
        write_volatile(addr_of_mut!((*ring).drained), d.wrapping_add(n));
    }
}
#[allow(dead_code)]
pub fn account_dropped(n: u32) {
    unsafe {
        let ring = addr_of_mut!(__rhu_console_ring);
        let d = read_volatile(addr_of!((*ring).dropped));
        write_volatile(addr_of_mut!((*ring).dropped), d.wrapping_add(n));
    }
}
#[allow(dead_code)]
pub fn account_irq() {
    unsafe {
        let ring = addr_of_mut!(__rhu_console_ring);
        let n = read_volatile(addr_of!((*ring).irqs));
        write_volatile(addr_of_mut!((*ring).irqs), n.wrapping_add(1));
    }
}

/// The wire ring of the probe rows: the bytes bound for the board's
/// transmitter, after the mirror. Sized to absorb a verbose transcript's
/// bursts (16 KB, the mirror's lap tolerance).
/// On the rows with a UART table a stream's UART wire is a port the UART
/// doors' module holds, with that module's rings (`uart.rs`).
#[cfg(rhu_console_probe)]
pub const WIRE_BYTES: usize = 16 * 1024;

#[cfg(rhu_console_probe)]
static mut WIRE: [u8; WIRE_BYTES] = [0; WIRE_BYTES];
/// The count of bytes pushed to the wire ring since the clock came up.
#[cfg(rhu_console_probe)]
static mut WIRE_WRITTEN: u32 = 0;

#[cfg(rhu_console_probe)]
fn wire_push(b: u8) {
    unsafe {
        let written = read_volatile(addr_of!(WIRE_WRITTEN));
        let bytes = addr_of_mut!(WIRE) as *mut u8;
        write_volatile(bytes.add(written as usize % WIRE_BYTES), b);
        write_volatile(addr_of_mut!(WIRE_WRITTEN), written.wrapping_add(1));
    }
}

/// The bytes pushed to the wire ring and not yet handed to the transmitter.
#[cfg(rhu_console_probe)]
fn backlog() -> u32 {
    unsafe {
        let ring = addr_of!(__rhu_console_ring);
        read_volatile(addr_of!(WIRE_WRITTEN)).wrapping_sub(read_volatile(addr_of!((*ring).drained)))
    }
}

/// The wire ring's consumer half, shared by the transmit interrupt and the
/// polled fallback: move bytes into the transmitter while it has room (at
/// most one FIFO's worth of register writes, never a wait). A writer that
/// lapped the ring -- more than the buffer's worth ahead of the wire -- has
/// overwritten the oldest undrained bytes; those are skipped and counted,
/// never sent out of order.
#[cfg(rhu_console_probe)]
unsafe fn drain() {
    unsafe {
        let ring = addr_of_mut!(__rhu_console_ring);
        let written = read_volatile(addr_of!(WIRE_WRITTEN));
        let mut drained = read_volatile(addr_of!((*ring).drained));
        let behind = written.wrapping_sub(drained);
        if behind > WIRE_BYTES as u32 {
            let lost = behind - WIRE_BYTES as u32;
            let dropped = read_volatile(addr_of!((*ring).dropped));
            write_volatile(addr_of_mut!((*ring).dropped), dropped.wrapping_add(lost));
            drained = drained.wrapping_add(lost);
        }
        let bytes = addr_of!(WIRE) as *const u8;
        while drained != written && board::ring_tx_room() {
            let b = read_volatile(bytes.add(drained as usize % WIRE_BYTES));
            board::ring_tx_put(b);
            drained = drained.wrapping_add(1);
        }
        write_volatile(addr_of_mut!((*ring).drained), drained);
    }
}

/// The polled fallback: the wire ring pushed through the wire to the last
/// byte written, waiting on the transmitter between refills.
#[cfg(rhu_console_probe)]
unsafe fn drain_polled() {
    unsafe {
        loop {
            drain();
            if backlog() == 0 {
                return;
            }
            while !board::ring_tx_room() {}
        }
    }
}

/// The transmit interrupt's body, called by the board's handler: the
/// interrupt counted and acknowledged, the transmitter refilled from the
/// wire ring, and the level-shaped enable left set exactly while a backlog
/// remains. Runs at the lowest priority band: a kernel critical section
/// (the writer's lock) holds it off, and it holds off only the context
/// switch.
#[cfg(rhu_console_probe)]
pub unsafe fn irq() {
    unsafe {
        let ring = addr_of_mut!(__rhu_console_ring);
        let n = read_volatile(addr_of!((*ring).irqs));
        write_volatile(addr_of_mut!((*ring).irqs), n.wrapping_add(1));
        board::ring_tx_ack();
        drain();
        board::ring_tx_irq(backlog() != 0);
    }
}

/// Byte transmit on the wire: the byte appended to the mirror (the tee),
/// then to the wire ring, then the transmitter primed from the ring while
/// it has room -- a bounded register loop, never a wait on the wire -- and
/// the level-shaped interrupt enable set exactly while a backlog remains,
/// so the interrupt drains whatever the transmitter could not take. In the
/// polled mode (the panic and fault paths) the ring is drained to the wire
/// here, synchronously. The caller holds the kernel critical section (the
/// console's line lock), which masks the interrupt.
#[cfg(rhu_console_probe)]
pub fn putb(b: u8) {
    mirror(b);
    wire_push(b);
    unsafe {
        if mode() == MODE_POLLED {
            drain_polled();
        } else {
            drain();
            board::ring_tx_irq(backlog() != 0);
        }
    }
}

/// The room the wire ring has for a writer, in bytes: the buffer less the
/// backlog (a writer that outran the wire reads zero until the interrupt
/// drains). Read outside the critical section by the door's back-pressure
/// and re-checked inside it before the write commits.
#[cfg(rhu_console_probe)]
pub fn room() -> usize {
    WIRE_BYTES.saturating_sub(backlog() as usize)
}
