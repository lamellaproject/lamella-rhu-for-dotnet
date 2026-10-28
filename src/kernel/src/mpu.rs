//! The memory protection unit: the first page of the address space
//! no-access on every row (the null-page guard), and on the ARMv7-M
//! rows a 32-byte no-access block at the floor of the running thread's
//! stack (the stack floor the Cortex-M7 and Cortex-M4 cores have no
//! limit register for). Programmed once at boot from kmain, before the
//! scheduler starts; the guard block follows the picked thread through the
//! switch.
//!
//! - PMSAv7 (the Cortex-M7 and Cortex-M4 rows): the unit's regions overlay
//!   a background map that stays live for privileged code (PRIVDEFENA), and
//!   a region may say "no access" (AP 000). Region 0 is the null page,
//!   region 1 the running thread's stack guard, region 2 on the STM32F769 row
//!   the quad-SPI flash window as Normal, read-only, executable memory when
//!   the image's map declares it, region 3 there the SDRAM as Normal,
//!   execute-never memory; everything else is the default map with its
//!   attributes unchanged (the STM32F769's data cache stays off; its
//!   instruction cache goes on under the window alone, cache.rs).
//! - PMSAv8 (the Cortex-M33 rows): no access permission denies, so the
//!   background is OFF and the row's table (`board::MPU_REGIONS`) names
//!   everything the kernel and a hosted program may touch; the first page is
//!   denied by lying outside every region. A region the table misses is a
//!   MemManage death, never a silent hole. The table is asserted against
//!   the row's link scripts
//!   at build time (the ranges `build.rs` reads out of them): a code or data
//!   region the table does not cover refuses to compile, never boots.

use core::arch::asm;
use core::ptr::{read_volatile, write_volatile};

const MPU_TYPE: *const u32 = 0xE000_ED90 as *const u32;
const MPU_CTRL: *mut u32 = 0xE000_ED94 as *mut u32;
/// PMSAv7: the region number register, written by the guard's reprogram
/// alone (the init writes RBAR with VALID, which selects the region).
#[cfg(all(not(armv8m), not(feature = "stack-guard-off")))]
const MPU_RNR: *mut u32 = 0xE000_ED98 as *mut u32;
const MPU_RBAR: *mut u32 = 0xE000_ED9C as *mut u32;
/// PMSAv7: the region attribute and size register.
#[cfg(not(armv8m))]
const MPU_RASR: *mut u32 = 0xE000_EDA0 as *mut u32;
/// PMSAv8: the region limit address register (the same offset).
#[cfg(armv8m)]
const MPU_RLAR: *mut u32 = 0xE000_EDA0 as *mut u32;
#[cfg(armv8m)]
const MPU_RNR: *mut u32 = 0xE000_ED98 as *mut u32;
#[cfg(armv8m)]
const MPU_MAIR0: *mut u32 = 0xE000_EDC0 as *mut u32;
/// SHPR1: MemManage's priority in [7:0] (UsageFault's in [23:16], the
/// scheduler's write; BusFault's in [15:8] stays at the reset 0).
const SHPR1: *mut u32 = 0xE000_ED18 as *mut u32;

const CTRL_ENABLE: u32 = 1 << 0;
#[cfg(not(armv8m))]
const CTRL_PRIVDEFENA: u32 = 1 << 2;

/// The page the guard denies: the runtime treats an access violation below
/// 4 KB as a null reference, and the compiler omits the null check for any
/// field within 2047 bytes of the reference.
pub const NULL_PAGE_BYTES: u32 = 0x1000;

/// The guard block at a thread's floor on the PMSAv7 rows: the smallest
/// region the architecture allows.
pub const GUARD_BYTES: u32 = 32;

/// Whether this image programs the stack guard (the PMSAv7 rows, without
/// the `stack-guard-off` control).
pub const GUARD_ON: bool = cfg!(all(not(armv8m), not(feature = "stack-guard-off")));

/// Whether this image denies the first page (every row, without the
/// `null-guard-off` control).
pub const NULL_GUARD_ON: bool = !cfg!(feature = "null-guard-off");

/// The guard block the running thread's carve carries on the PMSAv7 rows:
/// `[lo, hi)`, the lowest 32-aligned block at or above the carve's base
/// (carves are 8-aligned; the block overlays the bottom 32 to 56 bytes of
/// the carve, never the neighbour's).
pub const fn guard_block(stack_base: u32) -> (u32, u32) {
    let lo = (stack_base + (GUARD_BYTES - 1)) & !(GUARD_BYTES - 1);
    (lo, lo + GUARD_BYTES)
}

/// The bytes a delivery must leave above a thread's floor besides its own:
/// the guard block's worst placement (56 bytes), rounded to the carve's
/// alignment. Zero where no guard is programmed.
pub const GUARD_SLACK: u32 = if GUARD_ON { 64 } else { 0 };

/// One entry of a PMSAv8 row's table (`board::MPU_REGIONS`): an inclusive
/// address range, its memory type and whether code may execute from it.
/// Every region is read/write for any privilege level (the least change
/// from the default map), 32-byte granular.
#[cfg(armv8m)]
#[derive(Clone, Copy)]
pub struct Region {
    pub lo: u32,
    pub hi: u32,
    pub device: bool,
    pub exec: bool,
}

// ---------------- PMSAv7: the Cortex-M7 and Cortex-M4 rows ----------------

/// RBAR: the base address with VALID set and the region number in [3:0]
/// (the write updates RNR as well).
#[cfg(not(armv8m))]
const RBAR_VALID: u32 = 1 << 4;
/// RASR: ENABLE [0], SIZE [5:1] (the region is 2^(SIZE+1) bytes), AP
/// [26:24] = 000 (no access for anyone), XN [28].
#[cfg(not(armv8m))]
const RASR_NULL_PAGE: u32 = (1 << 28) | (11 << 1) | 1;
#[cfg(all(not(armv8m), not(feature = "stack-guard-off")))]
const RASR_GUARD: u32 = (1 << 28) | (4 << 1) | 1;
#[cfg(not(armv8m))]
const REGION_NULL: u32 = 0;
#[cfg(not(armv8m))]
const REGION_GUARD: u32 = 1;

/// Region 3 (the STM32F769 row and the an500 row): the row's 16 MB external
/// RAM range (the SDRAM at 0xC0000000; the emulator's bank at 0x60000000)
/// made Normal memory (the default map calls the SDRAM's external device
/// space Device memory, where the hosted runtime's unaligned accesses
/// would fault; the bank's space it calls Normal already) and execute-never
/// on both rows (the facts' exec flag is the unit's truth), full access:
/// TEX 001, C, B (write-back write-allocate, the attributes a cached
/// posture would use; with the caches off they make the range Normal and
/// nothing else), SIZE 23 (16 MB), XN. Programmed before the bring-up, so
/// the proof runs under the shipped attributes.
#[cfg(all(not(armv8m), any(rhu_sdram_window, rhu_mps2_window)))]
const REGION_EXTRAM: u32 = 3;
#[cfg(all(not(armv8m), any(rhu_sdram_window, rhu_mps2_window)))]
const RASR_EXTRAM: u32 = (1 << 28) | (0b011 << 24) | (0b001 << 19) | (1 << 17) | (1 << 16) | (23 << 1) | 1;
#[cfg(all(not(armv8m), any(rhu_sdram_window, rhu_mps2_window)))]
const _: () = assert!(RASR_EXTRAM == 0x130B_002F, "the external region's RASR is 0x130B002F");
#[cfg(all(not(armv8m), any(rhu_sdram_window, rhu_mps2_window)))]
const _: () = assert!(
    1u64 << (23 + 1) == crate::board::facts::RAM_RANGES[1].bytes as u64 && crate::board::facts::RAM_RANGES[1].base % (1 << 24) == 0,
    "the external region's SIZE encoding is the facts' 16 MB at a 16 MB-aligned base"
);

/// Region 2 (the STM32F769 row, when the image's map declares the quad-SPI
/// window): the 64 MB window at 0x90000000 as Normal, write-through,
/// non-shareable memory (TEX 000, C set, B clear: the attributes the default
/// map gives the range, and the one kind the Cortex-M7 caches), read-only
/// for every privilege (AP 110: a store to the flash is a fault, never a
/// silent no-op), executable (XN clear), SIZE 25 (2^26 bytes, the facts'
/// window). Programmed before the bring-up when the map declares the
/// window, so the proof runs under the shipped attributes.
#[cfg(all(not(armv8m), rhu_qspi_window))]
pub const REGION_QSPI: u32 = 2;
#[cfg(all(not(armv8m), rhu_qspi_window))]
pub const RASR_QSPI: u32 = (0b110 << 24) | (1 << 17) | (25 << 1) | 1;
#[cfg(all(not(armv8m), rhu_qspi_window))]
const _: () = assert!(RASR_QSPI == 0x0602_0033, "the window region's RASR is 0x06020033");
#[cfg(all(not(armv8m), rhu_qspi_window))]
const _: () = assert!(
    1u64 << (25 + 1) == crate::board::facts::FLASH_RANGES[1].bytes as u64 && crate::board::facts::FLASH_RANGES[1].base % (1 << 26) == 0 && crate::board::facts::FLASH_RANGES[1].executable,
    "the window region's SIZE encoding is the facts' 64 MB at a 64 MB-aligned base, executable"
);

#[cfg(not(armv8m))]
const _: () = assert!(1u32 << (11 + 1) == NULL_PAGE_BYTES, "the null page's SIZE encoding is 4 KB");
#[cfg(not(armv8m))]
const _: () = assert!(1u32 << (4 + 1) == GUARD_BYTES, "the guard block's SIZE encoding is 32 bytes");

/// Program the PMSAv7 posture; returns the regions programmed.
#[cfg(not(armv8m))]
unsafe fn program() -> u32 {
    let mut programmed = 0;
    unsafe {
        // Region 0: the null page, no access for anyone, never executable.
        // The control image leaves the slot disabled and the default map
        // answers the first page as the part does.
        write_volatile(MPU_RBAR, 0 | RBAR_VALID | REGION_NULL);
        if NULL_GUARD_ON {
            write_volatile(MPU_RASR, RASR_NULL_PAGE);
            programmed += 1;
        } else {
            write_volatile(MPU_RASR, 0);
        }
        // Region 1: the running thread's stack guard, written disabled here
        // and placed by the switch at every pick (`guard_set`).
        write_volatile(MPU_RBAR, 0x2000_0000 | RBAR_VALID | REGION_GUARD);
        write_volatile(MPU_RASR, 0);
        if GUARD_ON {
            programmed += 1;
        }
        // Region 3: the row's external RAM range as Normal, execute-never
        // memory.
        #[cfg(any(rhu_sdram_window, rhu_mps2_window))]
        {
            write_volatile(MPU_RBAR, crate::board::facts::RAM_RANGES[1].base as u32 | RBAR_VALID | REGION_EXTRAM);
            write_volatile(MPU_RASR, RASR_EXTRAM);
            programmed += 1;
        }
        // Region 2: the quad-SPI window as Normal, read-only, executable
        // memory, when the image's map declares the window.
        #[cfg(rhu_qspi_window)]
        if crate::board::stm32f7_qspi::declared() {
            write_volatile(MPU_RBAR, crate::board::facts::FLASH_RANGES[1].base as u32 | RBAR_VALID | REGION_QSPI);
            write_volatile(MPU_RASR, RASR_QSPI);
            programmed += 1;
        }
        write_volatile(MPU_CTRL, CTRL_ENABLE | CTRL_PRIVDEFENA);
    }
    programmed
}

/// One PMSAv7 region's base and attribute words read back from the unit
/// (the region number register selects it): the api flavor's window case.
#[cfg(all(not(armv8m), rhu_qspi_window, feature = "api-smoke"))]
pub fn region_readback(n: u32) -> (u32, u32) {
    const RNR: *mut u32 = 0xE000_ED98 as *mut u32;
    unsafe {
        write_volatile(RNR, n);
        (read_volatile(MPU_RBAR), read_volatile(MPU_RASR))
    }
}

/// The regions this posture needs on the core: the highest slot it writes
/// plus one (the guard's slot 1 on every row; the external range's slot 3).
#[cfg(not(armv8m))]
fn regions_needed() -> u32 {
    if cfg!(any(rhu_sdram_window, rhu_mps2_window)) { 4 } else { 2 }
}

/// Place the guard block at the picked thread's floor: called inside the
/// switch, in handler mode, a bounded handful of register writes. The
/// region is disabled while its base moves, then enabled over the new
/// block; the barrier pair makes the new map visible before the exception
/// return.
#[cfg(all(not(armv8m), not(feature = "stack-guard-off")))]
#[inline(always)]
pub fn guard_set(stack_base: u32) {
    let (lo, _) = guard_block(stack_base);
    unsafe {
        write_volatile(MPU_RNR, REGION_GUARD);
        write_volatile(MPU_RASR, 0);
        write_volatile(MPU_RBAR, lo | RBAR_VALID | REGION_GUARD);
        write_volatile(MPU_RASR, RASR_GUARD);
        asm!("dsb", "isb", options(nostack, preserves_flags));
    }
}

/// No guard block on the PMSAv8 rows (the limit registers bound the
/// thread stack there) or under the control.
#[cfg(any(armv8m, feature = "stack-guard-off"))]
#[inline(always)]
pub fn guard_set(_stack_base: u32) {}

// ---------------- PMSAv8: the Cortex-M33 rows ----------------

/// MAIR0: attribute 0 = Normal memory, non-cacheable (0x44); attribute 1 =
/// Device-nGnRE (0x04).
#[cfg(armv8m)]
const MAIR0_VALUE: u32 = 0x0000_0444;
#[cfg(armv8m)]
const ATTR_NORMAL: u32 = 0;
#[cfg(armv8m)]
const ATTR_DEVICE: u32 = 1;
/// RBAR: BASE [31:5], SH [4:3] = 00 (non-shareable: the regions' attribute
/// keeps every ldrex/strex pair on the core's local monitor and sends none to
/// the fabric's global monitor, which on the RP2350 answers for the SRAM
/// alone; the runtime's atomics on a word of the external window rest on it,
/// so the external window's region keeps this attribute), AP [2:1] = 01
/// (read/write by any privilege level), XN [0]. The control feature
/// `test-excl-shareable` programs the Normal regions Shareable (SH = 11)
/// instead, so the fabric answers the pairs.
#[cfg(all(feature = "test-excl-shareable", not(all(armv8m, feature = "api-smoke"))))]
compile_error!("test-excl-shareable is the api flavor's control on the PMSAv8 rows: select api-smoke beside it on an armv8m row");
#[cfg(armv8m)]
const RBAR_AP_RW_ANY: u32 = 0b01 << 1;
#[cfg(armv8m)]
const RBAR_XN: u32 = 1;
/// RLAR: LIMIT [31:5] (the last byte's address with its low bits set),
/// AttrIndx [3:1], EN [0].
#[cfg(armv8m)]
const RLAR_EN: u32 = 1;

#[cfg(armv8m)]
unsafe fn write_region(n: u32, r: &Region) {
    unsafe {
        write_volatile(MPU_RNR, n);
        let xn = if r.exec { 0 } else { RBAR_XN };
        #[cfg(not(feature = "test-excl-shareable"))]
        let sh = 0;
        #[cfg(feature = "test-excl-shareable")]
        let sh = if r.device { 0 } else { 0b11 << 3 };
        write_volatile(MPU_RBAR, (r.lo & !0x1F) | sh | RBAR_AP_RW_ANY | xn);
        let attr = if r.device { ATTR_DEVICE } else { ATTR_NORMAL };
        write_volatile(MPU_RLAR, (r.hi & !0x1F) | (attr << 1) | RLAR_EN);
    }
}

/// The posture the exclusive pairs rest on, read back from the unit and the
/// core: for every region the core implements, its RLAR enable bit and its
/// RBAR shareability bits (SH [4:3]), and the core's ACTLR.EXTEXCLALL (bit
/// 29, which sends every exclusive to the fabric when set). Returns (the
/// regions enabled, those reading SH = 00, those reading a Shareable value,
/// EXTEXCLALL); the api flavor's excl-posture check judges them.
#[cfg(all(armv8m, feature = "api-smoke"))]
pub fn exclusive_posture() -> (u32, u32, u32, u32) {
    const ACTLR: *const u32 = 0xE000_E008 as *const u32;
    let dregion = unsafe { (read_volatile(MPU_TYPE) >> 8) & 0xFF };
    let (mut enabled, mut nonshare, mut share) = (0u32, 0u32, 0u32);
    for n in 0..dregion {
        let (rbar, rlar) = unsafe {
            write_volatile(MPU_RNR, n);
            (read_volatile(MPU_RBAR), read_volatile(MPU_RLAR))
        };
        if rlar & RLAR_EN != 0 {
            enabled += 1;
            if (rbar >> 3) & 0b11 == 0 { nonshare += 1 } else { share += 1 }
        }
    }
    let extexclall = unsafe { (read_volatile(ACTLR) >> 29) & 1 };
    (enabled, nonshare, share, extexclall)
}

/// Program the PMSAv8 posture: the row's table, the background off;
/// returns the regions programmed.
#[cfg(armv8m)]
unsafe fn program() -> u32 {
    let mut n = 0u32;
    unsafe {
        write_volatile(MPU_MAIR0, MAIR0_VALUE);
        for r in crate::board::MPU_REGIONS.iter() {
            write_region(n, r);
            n += 1;
        }
        // The control image fills the hole: the first page readable and
        // writable as the part answers it (the alias, the boot ROM, or a
        // bus fault where nothing is mapped), never executable.
        if !NULL_GUARD_ON {
            write_region(n, &Region { lo: 0, hi: NULL_PAGE_BYTES - 1, device: false, exec: false });
            n += 1;
        }
        write_volatile(MPU_CTRL, CTRL_ENABLE);
    }
    n
}

#[cfg(armv8m)]
const fn regions_needed() -> u32 {
    crate::board::MPU_REGIONS.len() as u32 + if NULL_GUARD_ON { 0 } else { 1 }
}

/// Every region of a PMSAv8 table is 32-byte granular, ordered, and never
/// names the first page: the table denies the page by absence.
#[cfg(armv8m)]
const fn table_sound(t: &[Region]) -> bool {
    let mut i = 0;
    while i < t.len() {
        let r = &t[i];
        if r.lo & 0x1F != 0 || r.hi & 0x1F != 0x1F || r.lo > r.hi {
            return false;
        }
        if r.lo < NULL_PAGE_BYTES {
            return false;
        }
        i += 1;
    }
    true
}
#[cfg(armv8m)]
const _: () = assert!(table_sound(crate::board::MPU_REGIONS), "board::MPU_REGIONS: a region is not 32-byte granular, is inverted, or covers the first page");
#[cfg(armv8m)]
const _: () = assert!(crate::board::MPU_REGIONS.len() <= 7, "board::MPU_REGIONS: more regions than an eight-region core leaves beside the control's slot");

/// The link scripts' memory ranges, as `build.rs` reads them out of the
/// row's kernel script and its payload script: `NAME lo len` triples joined
/// by `;`, the numbers decimal. The build fails when a range the linker
/// may fill lies outside the row's table, or when a code range lies in a
/// region that forbids execution.
#[cfg(armv8m)]
const LINK_RANGES: &str = env!("RHU_LINK_RANGES");

#[cfg(armv8m)]
const fn parse_dec(b: &[u8], mut i: usize) -> (u64, usize) {
    let mut v: u64 = 0;
    while i < b.len() && b[i] >= b'0' && b[i] <= b'9' {
        v = v * 10 + (b[i] - b'0') as u64;
        i += 1;
    }
    (v, i)
}

/// Does one region of the table hold `[lo, lo + len)` whole, executable
/// where the range is a code range?
#[cfg(armv8m)]
const fn covered(t: &[Region], lo: u64, len: u64, exec: bool) -> bool {
    if len == 0 {
        return true;
    }
    let hi = lo + len - 1;
    let mut i = 0;
    while i < t.len() {
        let r = &t[i];
        if (r.lo as u64) <= lo && hi <= (r.hi as u64) && (!exec || r.exec) && !r.device {
            return true;
        }
        i += 1;
    }
    false
}

/// Walk the triples: a range named CODE or APP must be executable in its
/// region; DATA must be normal memory. Any other name is ignored.
#[cfg(armv8m)]
const fn ranges_covered(spec: &[u8], t: &[Region]) -> bool {
    let mut i = 0;
    while i < spec.len() {
        // The name.
        let name_at = i;
        while i < spec.len() && spec[i] != b' ' {
            i += 1;
        }
        let name = (name_at, i);
        if i >= spec.len() {
            return false;
        }
        i += 1;
        let (lo, j) = parse_dec(spec, i);
        if j >= spec.len() || spec[j] != b' ' {
            return false;
        }
        let (len, k) = parse_dec(spec, j + 1);
        i = k;
        let is_code = name_eq(spec, name, b"CODE") || name_eq(spec, name, b"APP");
        let is_data = name_eq(spec, name, b"DATA");
        if (is_code || is_data) && !covered(t, lo, len, is_code) {
            return false;
        }
        // The separator, or the end.
        if i < spec.len() {
            if spec[i] != b';' {
                return false;
            }
            i += 1;
        }
    }
    true
}

#[cfg(armv8m)]
const fn name_eq(spec: &[u8], name: (usize, usize), want: &[u8]) -> bool {
    let (a, b) = name;
    if b - a != want.len() {
        return false;
    }
    let mut k = 0;
    while k < want.len() {
        if spec[a + k] != want[k] {
            return false;
        }
        k += 1;
    }
    true
}

#[cfg(armv8m)]
const _: () = assert!(
    ranges_covered(LINK_RANGES.as_bytes(), crate::board::MPU_REGIONS),
    "a link-script memory range (CODE, APP or DATA, read by build.rs) lies outside the row's MPU table (board::MPU_REGIONS), or a code range lies in a region that forbids execution: the image would fault at boot"
);

// ---------------- both architectures ----------------

/// What the boot line states, read back from the unit.
pub struct Posture {
    /// The regions the core implements (MPU_TYPE.DREGION), read, never
    /// assumed.
    pub dregion: u32,
    /// The regions this image programmed (the guard's slot among them on
    /// the PMSAv7 rows).
    pub programmed: u32,
    /// Whether the default map answers for an address no region covers.
    pub background: bool,
}

/// Program the unit for the row and return the posture; a core with fewer
/// regions than the posture needs is a loud boot death with the count
/// (never a silent boot without the guard). The MemManage priority moves
/// below the band in the same step.
pub fn init() -> Posture {
    let dregion = unsafe { (read_volatile(MPU_TYPE) >> 8) & 0xFF };
    let needed = regions_needed();
    if dregion < needed {
        panic!(
            "MPU: this core implements {} regions, the row's posture needs {} -- refusing to boot without the null-page guard",
            dregion, needed
        );
    }
    unsafe {
        // MemManage at the emulation's level, below the zero-latency band
        // and below the tick; the other two bytes of SHPR1 untouched.
        let shpr1 = read_volatile(SHPR1);
        write_volatile(SHPR1, (shpr1 & !0xFF) | crate::sched::MEMMANAGE_PRIO);
        asm!("dmb", options(nostack, preserves_flags));
        write_volatile(MPU_CTRL, 0);
        // Every region the core implements disabled before the row's are
        // written, so a region an earlier stage left enabled takes no part
        // in the map (where two regions overlap the higher number wins).
        for n in 0..dregion {
            #[cfg(not(armv8m))]
            {
                write_volatile(MPU_RBAR, RBAR_VALID | n);
                write_volatile(MPU_RASR, 0);
            }
            #[cfg(armv8m)]
            {
                write_volatile(MPU_RNR, n);
                write_volatile(MPU_RLAR, 0);
            }
        }
        let programmed = program();
        asm!("dsb", "isb", options(nostack, preserves_flags));
        Posture { dregion, programmed, background: cfg!(not(armv8m)) }
    }
}

/// The boot line: pinned by the harness like the clock line.
pub fn banner(p: &Posture) {
    kprintln!(
        "[boot] MPU: {} regions, {} programmed, first page {}{}, background {}",
        p.dregion,
        p.programmed,
        if NULL_GUARD_ON { "no-access" } else { "OPEN (null-guard-off, a control image)" },
        if cfg!(all(not(armv8m), feature = "stack-guard-off")) { ", stack guard OFF (stack-guard-off, a control image)" } else { "" },
        if p.background { "on" } else { "off" }
    );
}

/// Switch the unit off: the terminal funnel's first act, so a dump that
/// reads a frame at a floor, or the console's own path, can never fault a
/// second time inside the report. The program is ending.
pub fn off() {
    unsafe {
        write_volatile(MPU_CTRL, 0);
        asm!("dsb", "isb", options(nostack, preserves_flags));
    }
}
