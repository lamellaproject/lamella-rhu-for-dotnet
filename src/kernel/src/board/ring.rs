//! The console ring, shared by every board whose console is driven the
//! ring way: every byte the console transmits is appended to the console
//! mirror in SRAM -- the transcript a debug probe reads out of RAM by
//! symbol, and the transmit ring's storage -- then the board's
//! transmitter is primed from the ring while it has room (a bounded
//! register loop, never a wait on the wire), and the transmitter's
//! interrupt drains the rest as the wire takes it, at the lowest
//! interrupt priority band. A writer holding the kernel critical section
//! is never paced by the wire, so the tick is never masked across a
//! character time. The panic and fault paths switch the driver to a
//! polled fallback that pushes the whole ring through the wire
//! synchronously, so a report is complete on the wire before the machine
//! parks.

use core::ptr::{addr_of, addr_of_mut, read_volatile, write_volatile};

use crate::board;

/// The console mirror's buffer: twice the largest flavor transcript.
pub const CONSOLE_LOG_BYTES: usize = 16 * 1024;
/// "RLOG" in a byte dump.
pub const CONSOLE_LOG_MAGIC: u32 = 0x474f_4c52;

/// The console mirror: every byte the console transmits is appended here
/// first, so a debug probe reads the whole transcript out of RAM over the
/// debug port -- no serial wiring needed, nothing lost at reset -- and the
/// transmit ring drains the wire from the same bytes. Layout
/// (little-endian words): the magic, the buffer size, the count of bytes
/// written since the clock came up (a count past the size means the
/// buffer wrapped and the earliest bytes are gone), a reserved zero, then
/// the bytes. The block sits in its own section after the thread stacks
/// and outside the kernel's zero-fill and statics windows (a conservative
/// statics scan never walks transcript text); the board's `clock_init`
/// writes the header once per boot through `boot_mark`, and a console
/// re-init on the panic path leaves it alone so a panic transcript
/// appends to the boot's.
#[repr(C)]
pub struct ConsoleLog {
    pub magic: u32,
    pub size: u32,
    pub written: u32,
    pub reserved: u32,
    pub bytes: [u8; CONSOLE_LOG_BYTES],
}

#[unsafe(no_mangle)]
#[unsafe(link_section = ".rhu_console_log")]
pub static mut __rhu_console_log: ConsoleLog = ConsoleLog {
    magic: 0,
    size: 0,
    written: 0,
    reserved: 0,
    bytes: [0; CONSOLE_LOG_BYTES],
};

/// The transmit ring's state, beside the mirror for a debug probe to read
/// by symbol: the driver mode (0 = not started; 1 = the transmit
/// interrupt drains the ring; 2 = polled, the mode the panic and fault
/// paths switch to), the count of bytes handed to the transmitter (equal
/// to the mirror's `written` once the wire has caught up), the count of
/// bytes the wire never carried because the writer lapped the ring (zero
/// in every gated transcript), and the count of transmit interrupts
/// taken. Plain statics in the zero-fill window: written by the producer
/// under the kernel critical section and by the interrupt that section
/// masks, so the two halves never interleave; the room query reads the
/// two counts outside the section (each an aligned word, read whole) and
/// its answer is re-checked under the section before a write commits.
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

/// The interrupt mode: the transmit interrupt drains the ring.
pub const MODE_IRQ: u32 = 1;
/// The polled mode: the ring is drained to the wire synchronously.
pub const MODE_POLLED: u32 = 2;

/// Set by the board's `clock_init` (the boot path only) through
/// `boot_mark` and consumed by the first `console_init` after it: that
/// call is the boot's and arms the transmit interrupt; any later call is
/// a re-init from the panic or fault path and takes the polled mode.
static mut BOOT_PENDING: u32 = 0;

/// The boot path's mark: the mirror's header written and the console init
/// that follows marked as the boot's.
pub fn boot_mark() {
    arm();
    unsafe { write_volatile(addr_of_mut!(BOOT_PENDING), 1) }
}

/// Consume the boot mark: true for the one console init that follows the
/// boot's `clock_init`, false for every re-init after it.
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
pub fn mode() -> u32 {
    unsafe { read_volatile(addr_of!((*addr_of!(__rhu_console_ring)).mode)) }
}

/// Record the driver mode the board's init settled on.
pub fn set_mode(mode: u32) {
    unsafe { write_volatile(addr_of_mut!((*addr_of_mut!(__rhu_console_ring)).mode), mode) }
}

/// Write the mirror's header: the count first (so a reader that sees the
/// magic never sees a stale count beside it), then the size and the magic.
fn arm() {
    unsafe {
        let log = addr_of_mut!(__rhu_console_log);
        write_volatile(addr_of_mut!((*log).written), 0);
        write_volatile(addr_of_mut!((*log).reserved), 0);
        write_volatile(addr_of_mut!((*log).size), CONSOLE_LOG_BYTES as u32);
        write_volatile(addr_of_mut!((*log).magic), CONSOLE_LOG_MAGIC);
    }
}

/// Append one byte to the mirror (the byte lands before the count moves,
/// so a reader never sees a count that runs past written bytes).
fn push(b: u8) {
    unsafe {
        let log = addr_of_mut!(__rhu_console_log);
        let written = read_volatile(addr_of!((*log).written));
        let bytes = addr_of_mut!((*log).bytes) as *mut u8;
        write_volatile(bytes.add(written as usize % CONSOLE_LOG_BYTES), b);
        write_volatile(addr_of_mut!((*log).written), written.wrapping_add(1));
    }
}

/// The bytes written and not yet handed to the transmitter.
fn backlog() -> u32 {
    unsafe {
        let log = addr_of!(__rhu_console_log);
        let ring = addr_of!(__rhu_console_ring);
        read_volatile(addr_of!((*log).written)).wrapping_sub(read_volatile(addr_of!((*ring).drained)))
    }
}

/// The ring's consumer half, shared by the transmit interrupt and the
/// polled fallback: move bytes from the mirror into the transmitter while
/// it has room (at most one FIFO's worth of register writes, never a
/// wait). A writer that lapped the ring -- more than the buffer's worth
/// ahead of the wire -- has overwritten the oldest undrained bytes; those
/// are skipped and counted, never sent out of order.
unsafe fn drain() {
    unsafe {
        let log = addr_of_mut!(__rhu_console_log);
        let ring = addr_of_mut!(__rhu_console_ring);
        let written = read_volatile(addr_of!((*log).written));
        let mut drained = read_volatile(addr_of!((*ring).drained));
        let behind = written.wrapping_sub(drained);
        if behind > CONSOLE_LOG_BYTES as u32 {
            let lost = behind - CONSOLE_LOG_BYTES as u32;
            let dropped = read_volatile(addr_of!((*ring).dropped));
            write_volatile(addr_of_mut!((*ring).dropped), dropped.wrapping_add(lost));
            drained = drained.wrapping_add(lost);
        }
        let bytes = addr_of!((*log).bytes) as *const u8;
        while drained != written && board::ring_tx_room() {
            let b = read_volatile(bytes.add(drained as usize % CONSOLE_LOG_BYTES));
            board::ring_tx_put(b);
            drained = drained.wrapping_add(1);
        }
        write_volatile(addr_of_mut!((*ring).drained), drained);
    }
}

/// The polled fallback: the ring pushed through the wire to the last
/// byte written, waiting on the transmitter between refills.
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
/// ring, and the level-shaped enable left set exactly while a backlog
/// remains. Runs at the lowest priority band: a kernel critical section
/// (the writer's lock) holds it off, and it holds off only the context
/// switch.
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

/// Byte transmit: the byte appended to the mirror (the ring's storage),
/// then the transmitter primed from the ring while it has room -- a
/// bounded register loop, never a wait on the wire -- and the level-shaped
/// interrupt enable set exactly while a backlog remains, so the interrupt
/// drains whatever the transmitter could not take. In the polled mode
/// (the panic and fault paths) the ring is drained to the wire here,
/// synchronously. The caller holds the kernel critical section (the
/// console's line lock), which masks the interrupt.
pub fn putb(b: u8) {
    push(b);
    unsafe {
        if mode() == MODE_POLLED {
            drain_polled();
        } else {
            drain();
            board::ring_tx_irq(backlog() != 0);
        }
    }
}

/// The room the ring has for a writer, in bytes: the buffer less the
/// backlog (a writer that outran the wire reads zero until the interrupt
/// drains). Read outside the critical section by the door's back-pressure
/// and re-checked inside it before the write commits.
pub fn room() -> usize {
    CONSOLE_LOG_BYTES.saturating_sub(backlog() as usize)
}
