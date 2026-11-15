//! The STM32U5's second I/O supply, pure: the facts a board states for it
//! and the reference manual's sequence over a two-verb register seam.
//! Fourteen pins of port G (PG2 to PG15) are fed by a supply pin of their
//! own, VDDIO2, and are isolated after a reset: "The isolation must be
//! removed before using any I/O from PG[15:2], by setting the IO2SV bit in
//! the PWR_SVMCR register, once the VDDIO2 supply is present" (RM0456
//! 10.4.5), and at the bit: "Setting this bit is mandatory to use
//! PG[15:2]" (10.10.5). The sequence is the manual's (10.6.3): enable the
//! supply's voltage monitor, wait for the monitor's wake-up, wait until
//! the monitor reads the supply ready, disable the monitor, set the valid
//! bit. The manual asks for the monitor where the supply is independent of
//! the main one; it is run on every board, because a board whose bridge to
//! the main supply was opened is a board whose supply may be absent. The
//! datasheet gives the monitor's threshold and no wake-up time (DS13543
//! Table 35), so the caller states a settle: that many reads of the status
//! register are discarded before a verdict counts. A supply that does not
//! read ready inside the bound leaves the pins isolated -- the valid bit is
//! left clear, and cleared if it was found set. Every write keeps the
//! control register's other bits (the analog supply's valid bit and the
//! other monitors live in the same register). No hardware access and no
//! state, covered by the host tests.
#![allow(dead_code)] // the host tier compiles the module whole; a row reads what it needs

/// The supply as a board's facts state it: the power controller's clock
/// enable (register and bit), the monitoring control register with the
/// monitor's enable bit and the valid bit, the monitoring status register
/// with the ready bit, and the first and last line the supply feeds.
#[derive(Clone, Copy, Debug)]
pub struct SupplyFacts {
    pub clock_reg: usize,
    pub clock_bit: u32,
    pub control_reg: usize,
    pub monitor_bit: u32,
    pub valid_bit: u32,
    pub status_reg: usize,
    pub ready_bit: u32,
    pub first_line: u16,
    pub last_line: u16,
}

/// The register seam: a 32-bit read and a 32-bit write at an address. The
/// board's shell makes them volatile accesses; the host tests script them.
pub trait Regs {
    fn rd(&mut self, addr: usize) -> u32;
    fn wr(&mut self, addr: usize, value: u32);
}

/// What the sequence found: whether the supply read ready and the valid
/// bit was set, how many reads of the status register the verdict took
/// (the bound when none came), and the control register as it was found.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Outcome {
    pub valid: bool,
    pub polls: u32,
    pub control_at_entry: u32,
}

/// The facts' own invariants: every bit inside a 32-bit register, every
/// register a word-aligned address, the monitor's bit and the valid bit two
/// bits, a line range that is not empty.
pub const fn facts_sound(f: &SupplyFacts) -> bool {
    f.clock_bit < 32
        && f.monitor_bit < 32
        && f.valid_bit < 32
        && f.ready_bit < 32
        && f.monitor_bit != f.valid_bit
        && f.clock_reg != 0
        && f.control_reg != 0
        && f.status_reg != 0
        && (f.clock_reg | f.control_reg | f.status_reg) & 3 == 0
        && f.control_reg != f.status_reg
        && f.first_line <= f.last_line
}

/// Whether the supply feeds the line: such a line is isolated until the
/// valid bit is set.
pub const fn feeds(f: &SupplyFacts, line: u32) -> bool {
    line >= f.first_line as u32 && line <= f.last_line as u32
}

/// The manual's sequence, in its order: the power controller's clock
/// enabled and read back; the monitor enabled; `settle` reads of the status
/// register discarded; the ready bit polled, at most `bound` reads; the
/// monitor disabled; then the valid bit set when the supply read ready, or
/// cleared when it did not.
pub fn bring_up<R: Regs>(r: &mut R, f: &SupplyFacts, settle: u32, bound: u32) -> Outcome {
    let clock = r.rd(f.clock_reg);
    r.wr(f.clock_reg, clock | (1 << f.clock_bit));
    let _ = r.rd(f.clock_reg);
    let entry = r.rd(f.control_reg);
    r.wr(f.control_reg, entry | (1 << f.monitor_bit));
    let mut i = 0;
    while i < settle {
        let _ = r.rd(f.status_reg);
        i += 1;
    }
    let mut polls = 0;
    let mut ready = false;
    while polls < bound {
        polls += 1;
        if r.rd(f.status_reg) & (1 << f.ready_bit) != 0 {
            ready = true;
            break;
        }
    }
    let quiet = r.rd(f.control_reg) & !(1 << f.monitor_bit);
    r.wr(f.control_reg, quiet);
    if ready {
        r.wr(f.control_reg, quiet | (1 << f.valid_bit));
    } else if quiet & (1 << f.valid_bit) != 0 {
        r.wr(f.control_reg, quiet & !(1 << f.valid_bit));
    }
    Outcome { valid: ready, polls, control_at_entry: entry }
}
