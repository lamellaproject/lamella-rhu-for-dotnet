//! The STM32U5 row's second I/O supply at boot: the pure module's sequence
//! (`crate::stm32u5_supply_math`: the reference manual's, RM0456 10.6.3)
//! run once over the chip's own registers, before the banner's gpio line and
//! before any program can claim a line. The supply feeds PG2 to PG15, which
//! stay isolated after a reset until the valid bit is set; on this board
//! the board's third LED and the connector's D0 and D1 are among them.
//! A supply that reads ready is declared valid and nothing is printed: the
//! verdict is in the record `__rhu_u5_io2`, read by symbol. A supply that
//! does not read ready inside the bound leaves its pins isolated; the board
//! then holds every line the supply feeds, so a claim of one -- as a GPIO
//! line, a PWM line or a bus pad -- answers the busy code, and one kernel
//! line says why. At boot the tick clock is not yet running, so the settle
//! and the bound are counts of status reads, never a clock: a read takes
//! at least four cycles of the system clock.
//! Behind it the same step switches the chip's I/O compensation cells on
//! (`cells`): the data sheet states the pads with them on, and they are off
//! after a reset. Its verdict is in the record `__rhu_u5_cells`.

use core::ptr::{addr_of_mut, read_volatile, write_volatile};

use crate::board::facts;
use crate::claims;
use crate::critical::with_cs;
use crate::stm32u5_supply_math::{self as m, Regs};

const _: () = assert!(m::facts_sound(&facts::IO2_SUPPLY), "the second I/O supply's facts are not sound: a bit past 31, a register that is not a word address, or an empty line range");

/// The settle after the monitor is enabled: the status reads of at least
/// 100 microseconds, discarded.
const SETTLE_READS: u32 = crate::board::SYSCLK_HZ / 10_000 / 4;
/// The bound on the wait for the monitor's verdict: the status reads of at
/// least two milliseconds.
const BOUND_READS: u32 = crate::board::SYSCLK_HZ / 500 / 4;

/// The I/O compensation cells' registers (RM0456): the block's clock is
/// SYSCFGEN, bit 1 of RCC_APB3ENR (11.8.36, offset 0xA8 of the RCC at
/// 0x4602 0C00); SYSCFG_CCCSR stands at offset 0x1C of the block at
/// 0x4600 0400 (15.3.8). The cell of the I/Os on VDD, the cell of the I/Os
/// on the second supply and the cell of the HSPI pads each have, in that
/// order from bit 0 up, an enable bit and a code-select bit, and from bit
/// 8 up a ready flag. The step serves the first two, the cells of the pads
/// the kernel drives; the HSPI pads' cell keeps its reset value, since no
/// driver of the kernel uses an HSPI pad.
const RCC_APB3ENR: usize = 0x4602_0CA8;
const APB3ENR_SYSCFGEN: u32 = 1 << 1;
const SYSCFG_CCCSR: usize = 0x4600_041C;
const CELLS: u32 = 2;
const CELL_IO2: u32 = 1;
const CELL_READY_SHIFT: u32 = 8;
const fn cell_enable(i: u32) -> u32 {
    1 << (2 * i)
}
const fn cell_select(i: u32) -> u32 {
    2 << (2 * i)
}
/// The bound on the wait for the cells' ready flags: the status reads of at
/// least ten milliseconds (neither the manual nor the data sheet states a
/// time; the record keeps the reads the wait took).
const CELLS_BOUND_READS: u32 = crate::board::SYSCLK_HZ / 100 / 4;

/// The verdicts the record carries.
pub const VERDICT_NOT_RUN: u32 = 0;
pub const VERDICT_VALID: u32 = 1;
pub const VERDICT_NOT_READY: u32 = 2;
pub const VERDICT_LEFT_OUT: u32 = 3;

/// The step's record, by symbol: the verdict (0 the step has not run, 1 the
/// supply read ready and is declared valid, 2 it did not read ready and its
/// lines are held by the board, 3 the boot left the step out), the status
/// reads the verdict took, and the control register as the step found it.
#[repr(C)]
pub struct SupplyRecord {
    pub verdict: u32,
    pub polls: u32,
    pub control_at_entry: u32,
}

#[unsafe(export_name = "__rhu_u5_io2")]
pub static mut RECORD: SupplyRecord = SupplyRecord { verdict: VERDICT_NOT_RUN, polls: 0, control_at_entry: 0 };

/// The cells' record, by symbol: which cells hand their code to their pads
/// (bit 0 the cell of the I/Os on VDD, bit 1 of those on the second
/// supply), the status reads the wait took, and the register as the step
/// left it.
#[repr(C)]
pub struct CellsRecord {
    pub active: u32,
    pub polls: u32,
    pub status: u32,
}

#[unsafe(export_name = "__rhu_u5_cells")]
pub static mut CELLS_RECORD: CellsRecord = CellsRecord { active: 0, polls: 0, status: 0 };

struct Chip;

impl Regs for Chip {
    fn rd(&mut self, addr: usize) -> u32 {
        unsafe { read_volatile(addr as *const u32) }
    }
    fn wr(&mut self, addr: usize, value: u32) {
        unsafe { write_volatile(addr as *mut u32, value) }
    }
}

fn record(verdict: u32, polls: u32, control_at_entry: u32) {
    with_cs(|| unsafe {
        let r = addr_of_mut!(RECORD);
        write_volatile(addr_of_mut!((*r).verdict), verdict);
        write_volatile(addr_of_mut!((*r).polls), polls);
        write_volatile(addr_of_mut!((*r).control_at_entry), control_at_entry);
    });
}

/// The record, by value.
#[allow(dead_code)]
pub fn verdict() -> (u32, u32, u32) {
    unsafe {
        let r = addr_of_mut!(RECORD);
        (read_volatile(addr_of_mut!((*r).verdict)), read_volatile(addr_of_mut!((*r).polls)), read_volatile(addr_of_mut!((*r).control_at_entry)))
    }
}

/// The sequence over the chip's registers, its outcome recorded.
pub fn bring_up() -> m::Outcome {
    let o = m::bring_up(&mut Chip, &facts::IO2_SUPPLY, SETTLE_READS, BOUND_READS);
    record(if o.valid { VERDICT_VALID } else { VERDICT_NOT_READY }, o.polls, o.control_at_entry);
    o
}

/// The I/O compensation cells switched on (RM0456 15.2.1): the data sheet
/// states the pads' speed classes and every interface's table with them on
/// (DS13543, Table 90's note 5), and after a reset they are off, every pad
/// on a fixed code. The HSI, which the cells use, runs in both of the
/// row's postures. The block's clock; the two cells enabled -- the second
/// supply's only where that supply is valid; the register read until every
/// enabled cell reads ready, or the bound. A cell that reads ready then
/// has its code-select bit cleared, so its pads receive its code; one that
/// does not is switched off again, its pads on the fixed code. The outcome
/// in the record, and the kernel line where a cell did not read ready.
fn cells(io2_valid: bool) {
    let mut c = Chip;
    let enables = c.rd(RCC_APB3ENR);
    c.wr(RCC_APB3ENR, enables | APB3ENR_SYSCFGEN);
    let _ = c.rd(RCC_APB3ENR);
    let (mut asked, mut v, mut i) = (0, c.rd(SYSCFG_CCCSR), 0);
    while i < CELLS {
        if i != CELL_IO2 || io2_valid {
            asked |= 1 << i;
            v |= cell_enable(i);
        }
        i += 1;
    }
    c.wr(SYSCFG_CCCSR, v);
    let (mut polls, mut ready) = (0, 0);
    while polls < CELLS_BOUND_READS && ready != asked {
        ready = (c.rd(SYSCFG_CCCSR) >> CELL_READY_SHIFT) & asked;
        polls += 1;
    }
    v = c.rd(SYSCFG_CCCSR);
    i = 0;
    while i < CELLS {
        if asked & (1 << i) != 0 {
            v &= !(if ready & (1 << i) != 0 { cell_select(i) } else { cell_enable(i) });
        }
        i += 1;
    }
    c.wr(SYSCFG_CCCSR, v);
    let status = c.rd(SYSCFG_CCCSR);
    with_cs(|| unsafe {
        let r = addr_of_mut!(CELLS_RECORD);
        write_volatile(addr_of_mut!((*r).active), ready);
        write_volatile(addr_of_mut!((*r).polls), polls);
        write_volatile(addr_of_mut!((*r).status), status);
    });
    if ready != asked {
        kprintln!("[gpio] an I/O compensation cell did not read ready inside its bound: its pads keep the fixed code");
    }
}

/// The step at boot, after the claims table is made. A supply that did not
/// read ready: its lines taken for the board, and the kernel line. The
/// compensation cells follow it on both ways.
pub fn boot() {
    if cfg!(feature = "test-io2-probe") {
        record(VERDICT_LEFT_OUT, 0, 0);
        return;
    }
    if bring_up().valid {
        cells(true);
        return;
    }
    with_cs(|| unsafe {
        let c = &mut *addr_of_mut!(crate::gpio::CLAIMS);
        let mut line = facts::IO2_SUPPLY.first_line as u32;
        while line <= facts::IO2_SUPPLY.last_line as u32 {
            let _ = claims::claim_board(c, line);
            line += 1;
        }
    });
    kprintln!("[gpio] the second I/O supply did not read ready inside its bound: PG2 to PG15 stay isolated and are held by the board");
    cells(false);
}

/// The same sequence for another supply of the chip whose monitor and valid
/// bit stand in the same two registers (the analog supply, at the
/// converter's first power-up), over the chip's own registers with this
/// file's settle and bound; nothing is recorded here.
pub fn sequence(f: &m::SupplyFacts) -> m::Outcome {
    m::bring_up(&mut Chip, f, SETTLE_READS, BOUND_READS)
}
