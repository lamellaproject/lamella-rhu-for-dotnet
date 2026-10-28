//! The RAM-range doors' bodies: the board's RAM as the
//! kernel offers it to the hosted runtime, so a program's heap can span
//! more than the chip's own SRAM -- the three verbs `api.rs` exports and
//! `include/rhu.h` states. A row's ranges are a TABLE in its facts
//! (`board::facts::RAM_RANGES`, the kernel's mirror of the SDK's
//! `boards/<row>.json` and its `ram` array; the hosted-runtime gate ties the
//! two): range 0 is always the internal SRAM (the arena's window 0, the
//! range behind `rhu_arena_alloc_at`), range 1 an external RAM
//! where a board has one and the image brought it up and proved it at boot
//! (the Pimoroni rows' PSRAM; the STM32F769's SDRAM; the emulator rows' PSRAM bank). The count door says how many ranges
//! THIS IMAGE serves, the range door the index-th range's record -- the
//! physical range as the facts state it, the arena window inside it the
//! runtime may be handed, that window's headroom at the call, the flags --
//! and the allocation door draws from a named range (`arena::alloc_from`).
//!
//! Behind the doors on the rows that carry the part (`rhu_psram_window`):
//! the bring-up through the PSRAM crate (`lamella-psram-qspi`:
//! the second chip select's pad, the part's direct-mode sequence, the
//! window's timing derived from the part's limits and the system clock) and
//! the read-back proof at every boot, called from kmain right after the
//! clock is final and before any interrupt source is enabled. A refusal is a
//! loud death with the error's words: the kernel never boots into a heap it
//! cannot trust.

use core::ptr::write_volatile;

use crate::arena;
use crate::board::facts;
use crate::sched;

/// One range as the board's facts state it (`board::facts::RAM_RANGES`).
pub struct RamFacts {
    /// The name the board's documents give the memory (the banner's word).
    pub name: &'static str,
    /// The physical range, whole.
    pub base: usize,
    pub bytes: usize,
    /// Behind a memory controller rather than the chip's own SRAM.
    pub external: bool,
    /// Reached through a cache the kernel manages (the RP2350's XIP cache
    /// for the PSRAM; the Cortex-M7's data cache where a board has one).
    pub cached: bool,
    /// Whether the hosted runtime may run code from the range (the
    /// flag per range): the null-page guard's MPU tables keep every internal RAM
    /// region executable (the shim's thunks live there) and the RP2350's
    /// flash-interface region, where the PSRAM window lies, is executable
    /// too; the STM32F769's SDRAM is execute-never (mpu.rs, region 3).
    pub exec: bool,
}

/// One flash range as a board's facts state it (`board::facts::FLASH_RANGES`
/// on the rows that state them; the SDK's facts file's `flash` array, the
/// two kept equal by the hosted-runtime gate's facts tie): the chip's own
/// flash and, on the STM32F769 row, the quad-SPI window. No door serves
/// these: a program's code has its home by its map.
#[allow(dead_code)] // the rows that state flash ranges and the quad-SPI bring-up read it
pub struct FlashFacts {
    pub name: &'static str,
    pub base: u32,
    pub bytes: u32,
    /// Outside the chip (the board's part behind a memory controller).
    pub external: bool,
    /// Presented in the address space, read like internal memory.
    pub mapped: bool,
    /// Whether code runs from it.
    pub executable: bool,
}

/// The record the range door fills (`rhu_ram_desc` in `include/rhu.h`):
/// six 32-bit words, C layout.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct RamRange {
    /// The physical range as the facts state it.
    pub base: u32,
    pub bytes: u32,
    /// The arena window inside it: the first byte the runtime may be
    /// handed and one past the last.
    pub lo: u32,
    pub hi: u32,
    /// That window's headroom at the call (range 0's equals
    /// `rhu_arena_headroom`).
    pub free: u32,
    pub flags: u32,
}

/// The flags (`RHU_RAM_*` in the header).
pub const FLAG_EXTERNAL: u32 = 1 << 0;
pub const FLAG_CACHED: u32 = 1 << 1;
pub const FLAG_EXEC: u32 = 1 << 2;
pub const FLAG_DMA: u32 = 1 << 3;

/// The ranges this image serves: the facts' first range on every row; the
/// facts' second where the external window is built and proved at boot.
pub const SERVED: usize = if cfg!(rhu_extram_window) { 2 } else { 1 };
const _: () = assert!(SERVED <= facts::RAM_RANGES.len(), "the image serves a range the facts do not state");
const _: () = assert!(facts::RAM_RANGES.len() <= arena::WINDOWS, "more ranges than the arena has windows");
const _: () = assert!(!facts::RAM_RANGES[0].external && !facts::RAM_RANGES[0].cached, "range 0 is the internal SRAM");

/// The DMA-coherence flag is clear on every range: no door of
/// the kernel drives a bus master, and the PSRAM is behind a cache a
/// bus master would not see through.
const DMA: bool = false;

fn flags_of(f: &RamFacts) -> u32 {
    (if f.external { FLAG_EXTERNAL } else { 0 })
        | (if f.cached { FLAG_CACHED } else { 0 })
        | (if f.exec { FLAG_EXEC } else { 0 })
        | (if DMA { FLAG_DMA } else { 0 })
}

/// The count door: the ranges this image serves (at least 1).
pub fn count() -> i32 {
    let n = arena::count() as i32;
    // The doctored control: one range more than the facts state, with a
    // record the range door fills below; the api flavor's check must refuse
    // the count against the facts.
    #[cfg(feature = "test-ram-doctored")]
    let n = n + 1;
    n
}

/// The range door: the index-th range's record; the invalid code past the
/// count or for a NULL or misaligned record.
pub fn range(index: u32, out: *mut RamRange) -> i32 {
    if out.is_null() || (out as usize) & 3 != 0 {
        return sched::ERR_INVAL;
    }
    #[cfg(feature = "test-ram-doctored")]
    if index as usize == arena::count() {
        // The phantom: a range the facts do not state, sized like a second
        // internal bank, at an address no map of this tree places RAM at.
        unsafe {
            write_volatile(out, RamRange { base: 0x3000_0000, bytes: 0x1_0000, lo: 0x3000_0000, hi: 0x3001_0000, free: 0x1_0000, flags: FLAG_EXEC });
        }
        return 0;
    }
    let Some(f) = facts::RAM_RANGES.get(index as usize) else {
        return sched::ERR_INVAL;
    };
    let Some((lo, hi, free)) = arena::window(index as usize) else {
        return sched::ERR_INVAL;
    };
    unsafe {
        write_volatile(
            out,
            RamRange { base: f.base as u32, bytes: f.bytes as u32, lo: lo as u32, hi: hi as u32, free, flags: flags_of(f) },
        );
    }
    0
}

/// A range's words for the boot banner: `SRAM 520 KB at 0x20000000` (a
/// whole number of megabytes is said in MB).
pub struct Words(pub usize);

impl core::fmt::Display for Words {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let r = &facts::RAM_RANGES[self.0];
        if r.bytes % (1024 * 1024) == 0 {
            write!(f, "{} {} MB at {:#010x}", r.name, r.bytes / (1024 * 1024), r.base)
        } else {
            write!(f, "{} {} KB at {:#010x}", r.name, r.bytes / 1024, r.base)
        }
    }
}

/// The PSRAM's bring-up and proof on the rows that carry the part,
/// through the PSRAM crate, from kmain with the clock final and no
/// interrupt source enabled. Returns the window the crate established (its
/// identification bytes go in the banner line). Every refusal is a loud
/// death with its words.
#[cfg(rhu_psram_window)]
pub fn psram_bring_up() -> lamella_psram_qspi::Window {
    use lamella_psram_qspi::proof::{Pattern, View};
    use lamella_psram_qspi::rp2350::{Config, Mmio, bring_up};
    use lamella_psram_qspi::{Error, Grade, Part, Step, TimingError};

    let part: &'static Part = &Part::APS6404L_3SQR;
    let cfg = Config {
        select_line: facts::PSRAM_SELECT_LINE,
        part,
        sys_hz: crate::board::SYSCLK_HZ,
        grade: Grade::Standard,
        bytes: facts::RAM_RANGES[1].bytes,
    };
    // The crate's preconditions, met by position in kmain: a single core,
    // no interrupt source enabled yet, the system clock final (the clock
    // read-back's assert just passed), window 1 unused, the direct-mode
    // nucleus in RAM (the maps copy every `.data.*` section at boot).
    let w = match unsafe { bring_up(&cfg) } {
        Ok(w) => w,
        Err(Error::NotPresent { vendor, kgd }) => {
            panic!("psram: no part answered the Read ID on the second chip select with a good die (vendor {:#04x}, KGD {:#04x}; the pass value is {:#04x})", vendor, kgd, part.kgd_pass)
        }
        Err(Error::Stuck(step)) => {
            let at = match step {
                Step::Resets => "releasing the pad and function blocks from reset",
                Step::Enter => "entering direct mode",
                Step::Nudge => "the exit-quad nudge",
                Step::ResetEnable => "the reset-enable command",
                Step::Reset => "the reset command",
                Step::ReadId => "the Read ID frame",
                Step::EnterQuad => "the enter-quad command",
            };
            panic!("psram: the flash interface's busy flag never cleared during {}", at)
        }
        Err(Error::Timing(TimingError::SelectBound { tcem_cycles, tail_cycles })) => {
            panic!("psram: the part's chip-select limit ({} cycles) cannot bound a transfer of {} cycles at this clock", tcem_cycles, tail_cycles)
        }
        Err(Error::Timing(e)) => panic!("psram: the part's limits cannot be met at this clock ({:?})", e),
        Err(Error::SelectLine(line)) => panic!("psram: GPIO {} cannot carry the second chip select", line),
        Err(Error::WindowSize(bytes)) => panic!("psram: a window of {} bytes is not one the second chip select serves", bytes),
    };

    #[cfg(feature = "test-psram-doctored")]
    unsafe {
        const WRITABLE_M1: u32 = 1 << 11;
        let ctrl = lamella_psram_qspi::rp2350::XIP_CTRL as *mut u32;
        write_volatile(ctrl, core::ptr::read_volatile(ctrl) & !WRITABLE_M1);
    }

    // The proof: the stride walk at every boot (one word per page and the
    // last word, under a pattern a previous boot cannot have left in the
    // part); the full walk too in the gate flavor (every word under two
    // patterns). Either leaves the window zero in memory with the cache
    // clean, so the arena's own memset meets zeros.
    let salt = crate::board::boot_salt();
    let proof = unsafe { lamella_psram_qspi::proof::stride_walk(&mut Mmio, &w, salt) };
    #[cfg(feature = "api-smoke")]
    let proof = proof.and_then(|()| unsafe { lamella_psram_qspi::proof::full_walk(&mut Mmio, &w, salt) });
    if let Err(m) = proof {
        let view = match m.view {
            View::Cached => "cached",
            View::Uncached => "uncached",
        };
        let pattern = match m.pattern {
            Pattern::Address => "address",
            Pattern::Complement => "complement",
        };
        panic!(
            "psram: read-back mismatch at {:#010x} through the {} view under the {} pattern: expected {:#010x}, read {:#010x}",
            m.addr, view, pattern, m.expected, m.read
        );
    }
    w
}

/// The SDRAM's bring-up and proof on the STM32F769 row, through the
/// family's SDRAM module and the PSRAM crate's generic walks over a
/// plain-access memory, from kmain with the clock final and no interrupt
/// source enabled (the protection unit's region 3 already programmed, so
/// the proof runs under the shipped attributes). Every refusal is a loud
/// death with its words.
#[cfg(rhu_sdram_window)]
pub fn sdram_bring_up() {
    use crate::board::stm32f7_sdram::{self as sdram, Plain};
    use lamella_psram_qspi::proof::{Pattern, View};
    use lamella_psram_qspi::{Id, Window, WindowTiming};

    sdram::bring_up();
    // The crate's window over the facts' range: one view (the caches are
    // off on this row), no identification bytes (a DRAM has none), and the
    // timing fields of the crate's QSPI host left at zero -- the walks read
    // the base, the size and the views alone.
    let w = Window {
        base: facts::RAM_RANGES[1].base,
        bytes: facts::RAM_RANGES[1].bytes,
        uncached_base: facts::RAM_RANGES[1].base,
        id: Id::NONE,
        timing: WindowTiming { clkdiv: 0, rxdelay: 0, min_deselect: 0, max_select: 0, select_hold: 0, select_setup: false, pagebreak: 0, cooldown: 0 },
    };
    // The proof: the stride walk at every boot (one word per page and the
    // last word, under a pattern a previous boot cannot have left in the
    // part); the full walk too in the gate flavor (every word under two
    // patterns). Either leaves the window zero, so the arena's own memset
    // meets zeros.
    let salt = sdram::boot_salt();
    let proof = unsafe { lamella_psram_qspi::proof::stride_walk(&mut Plain, &w, salt) };
    #[cfg(feature = "api-smoke")]
    let proof = proof.and_then(|()| unsafe { lamella_psram_qspi::proof::full_walk(&mut Plain, &w, salt) });
    if let Err(m) = proof {
        let view = match m.view {
            View::Cached => "cached",
            View::Uncached => "uncached",
        };
        let pattern = match m.pattern {
            Pattern::Address => "address",
            Pattern::Complement => "complement",
        };
        panic!(
            "sdram: read-back mismatch at {:#010x} through the {} view under the {} pattern: expected {:#010x}, read {:#010x}",
            m.addr, view, pattern, m.expected, m.read
        );
    }
}

/// The proof's memory on the emulator rows: plain volatile word access
/// through the one view (no cache in the path, so the "uncached view" is
/// the same address and the flush has nothing to maintain).
#[cfg(rhu_mps2_window)]
struct Plain;

#[cfg(rhu_mps2_window)]
impl lamella_psram_qspi::proof::Memory for Plain {
    unsafe fn write(&mut self, addr: usize, v: u32) {
        unsafe { write_volatile(addr as *mut u32, v) }
    }
    unsafe fn read(&mut self, addr: usize) -> u32 {
        unsafe { core::ptr::read_volatile(addr as *const u32) }
    }
    unsafe fn flush(&mut self) {}
}

/// The emulator rows' PSRAM bank: nothing to program (QEMU models the
/// machines' 16 MB bank as plain RAM at the application notes' addresses),
/// so the bring-up is the proof alone -- the PSRAM crate's stride walk over
/// the facts' range (one word per page and the last word; the full walk
/// too in the gate flavor), from kmain with no interrupt source enabled
/// and the protection unit's region over the bank programmed. A bank the
/// machine does not model refuses the walk (a mismatch or a bus fault)
/// before arena::init: the kernel never boots into a heap it cannot trust.
/// The pattern's salt is a constant: the emulator's RAM starts every run
/// empty, so no earlier boot's pattern can sit in it.
#[cfg(rhu_mps2_window)]
pub fn bank_bring_up() {
    use lamella_psram_qspi::proof::{Pattern, View};
    use lamella_psram_qspi::{Id, Window, WindowTiming};

    const SALT: u32 = 0x5A5A_A5A5;
    let w = Window {
        base: facts::RAM_RANGES[1].base,
        bytes: facts::RAM_RANGES[1].bytes,
        uncached_base: facts::RAM_RANGES[1].base,
        id: Id::NONE,
        timing: WindowTiming { clkdiv: 0, rxdelay: 0, min_deselect: 0, max_select: 0, select_hold: 0, select_setup: false, pagebreak: 0, cooldown: 0 },
    };
    let proof = unsafe { lamella_psram_qspi::proof::stride_walk(&mut Plain, &w, SALT) };
    #[cfg(feature = "api-smoke")]
    let proof = proof.and_then(|()| unsafe { lamella_psram_qspi::proof::full_walk(&mut Plain, &w, SALT) });
    if let Err(m) = proof {
        let view = match m.view {
            View::Cached => "cached",
            View::Uncached => "uncached",
        };
        let pattern = match m.pattern {
            Pattern::Address => "address",
            Pattern::Complement => "complement",
        };
        panic!(
            "ram: read-back mismatch at {:#010x} through the {} view under the {} pattern: expected {:#010x}, read {:#010x} (the machine's bank)",
            m.addr, view, pattern, m.expected, m.read
        );
    }
}
