//! The STM32F7 family's SDRAM bring-up: the flexible memory
//! controller's bank 1 driving the STM32F769 Discovery board's 16 MB
//! synchronous DRAM part (Micron's MT48LC4M32B2: four banks of 4096 rows
//! by 256 columns by 32 bits) at 0xC0000000, brought up from kmain with the
//! clock final and no interrupt source enabled, proved by read-back at
//! every boot through the PSRAM crate's generic walks over a plain-access
//! memory (the caches are off on this row, the family module's law), and
//! offered to the hosted runtime as the board's second RAM range through
//! the RAM-range doors ship.
//!
//! Every word the controller takes is DERIVED at compile time from the
//! part's limits (its datasheet), the posture's bus
//! clock and the chip's bound on the memory clock (the chip datasheet's
//! FMC_SDCLK maximum), by `const fn`s whose asserts pin the two
//! postures' words to the derived numbers and refuse a posture the
//! part cannot be met at: a bad posture fails the BUILD. The control
//! `test-sdram-overclock` lowers the bound under every legal divider, so
//! the build must refuse with the derivation's words.

use core::ptr::{read_volatile, write_volatile};

use lamella_psram_qspi::proof::Memory;

/// A synchronous DRAM part's limits as a value: its geometry, the CAS
/// latency the design drives it at with the part's minimum clock period at
/// that latency, the AC limits the controller's timing word is derived
/// from, the refresh period over the rows it covers, and the wake-up
/// sequence's two numbers (the pause after the clock is stable, the
/// auto-refresh commands the part asks after the precharge).
pub struct SdramPart {
    /// The part's name, a fact of the row's record (the banner names the
    /// controller and the clock, not the part).
    #[allow(dead_code)]
    pub name: &'static str,
    /// Column, row and bank address bits; the data width in bits.
    pub col_bits: u32,
    pub row_bits: u32,
    pub bank_bits: u32,
    pub width_bits: u32,
    /// The CAS latency in clocks and the minimum clock period at it, in
    /// picoseconds.
    pub cas: u32,
    pub t_ck_min_ps: u32,
    /// Active-to-precharge (tRAS), row cycle (tRC), auto-refresh period
    /// (tRFC), active-to-column (tRCD), precharge (tRP) and self-refresh
    /// exit (tXSR), in nanoseconds.
    pub t_ras_ns: u32,
    pub t_rc_ns: u32,
    pub t_rfc_ns: u32,
    pub t_rcd_ns: u32,
    pub t_rp_ns: u32,
    pub t_xsr_ns: u32,
    /// The write recovery's fixed part after one clock (tWR = 1 CLK + this).
    pub t_wr_ns_after_clk: u32,
    /// The load-mode-register-to-active delay, in clocks (tMRD).
    pub t_mrd_ck: u32,
    /// The refresh period in milliseconds and the rows it must cover.
    pub t_ref_ms: u32,
    pub refresh_rows: u32,
    /// The wake-up: the pause after the clock is stable, in microseconds,
    /// and the least auto-refresh commands after the precharge.
    pub init_pause_us: u32,
    pub init_refreshes_min: u32,
}

/// Micron MT48LC4M32B2, the -6A speed grade (the datasheet "128Mb: x32
/// SDRAM" Rev. V 03/16:
/// the key timing table and table 12's -6A column, the initialization
/// section, the mode register's figure 13). The board's part by the
/// schematic: MT48LC4M32B2B5-6A.
pub const MT48LC4M32B2_6A: SdramPart = SdramPart {
    name: "MT48LC4M32B2",
    col_bits: 8,
    row_bits: 12,
    bank_bits: 2,
    width_bits: 32,
    cas: 3,
    t_ck_min_ps: 6_000,
    t_ras_ns: 42,
    t_rc_ns: 60,
    t_rfc_ns: 60,
    t_rcd_ns: 18,
    t_rp_ns: 18,
    t_xsr_ns: 67,
    t_wr_ns_after_clk: 7,
    t_mrd_ck: 2,
    t_ref_ms: 64,
    refresh_rows: 4096,
    init_pause_us: 100,
    init_refreshes_min: 2,
};

/// The chip's bound on the memory clock (the STM32F769NI datasheet, "SDRAM
/// waveforms and timings"): 100 MHz at a 20 pF load on FMC_SDCLK for 3.0 to
/// 3.6 V, 90 MHz at 30 pF; the board's trace load is not a number this
/// kernel has, so the conservative 90 MHz is the bound.
/// The control `test-sdram-overclock` puts the bound under every
/// legal divider at every posture, so the derivation must refuse the build.
pub const SDCLK_BOUND_HZ: u32 = if cfg!(feature = "test-sdram-overclock") { 1 } else { 90_000_000 };

/// The controller's words for one posture, derived from a part and the bus
/// clock.
pub struct SdramWords {
    /// The memory clock (HCLK over the divider).
    pub sdclk_hz: u32,
    /// The divider: 2 or 3, the only ones the controller offers.
    pub divider: u32,
    /// FMC_SDCR1: the geometry, the latency, the divider, the read burst.
    pub sdcr: u32,
    /// FMC_SDTR1: the seven delays, each in memory clocks minus one.
    pub sdtr: u32,
    /// The part's mode register content (the load-mode-register command's
    /// MRD field).
    pub mode: u32,
    /// FMC_SDRTR's COUNT: the refresh timer in memory clocks.
    pub refresh_count: u32,
    /// The CAS latency the words drive, for the banner.
    pub cas: u32,
}

/// Memory clocks that cover `ns` at `hz`, rounded up.
const fn ceil_cycles(ns: u32, hz: u32) -> u32 {
    ((ns as u64 * hz as u64).div_ceil(1_000_000_000)) as u32
}

const fn max(a: u32, b: u32) -> u32 {
    if a > b { a } else { b }
}

/// A delay field is at least one clock (the controller's 0000 = 1 cycle).
const fn at_least_one(c: u32) -> u32 {
    if c == 0 { 1 } else { c }
}

/// Whether a memory clock is legal: under the chip's bound and not under
/// the part's minimum period at the latency.
const fn legal(part: &SdramPart, hz: u32, bound_hz: u32) -> bool {
    hz <= bound_hz && hz > 0 && 1_000_000_000_000u64 / hz as u64 >= part.t_ck_min_ps as u64
}

/// The controller's words from the part, the bus clock (HCLK) and the
/// chip's bound: the fastest legal divider, every delay the ceiling of the
/// part's limit over the memory clock's period (TRC covering tRFC too, TWR
/// at least the controller's two constraints), the mode register at the
/// part's latency with a burst length of one and single-location writes,
/// the refresh count by the manual's formula. A posture the part cannot be
/// met at is a const panic, so the image fails to build.
pub const fn derive(part: &SdramPart, hclk_hz: u32, bound_hz: u32) -> SdramWords {
    let divider = if legal(part, hclk_hz / 2, bound_hz) {
        2
    } else if legal(part, hclk_hz / 3, bound_hz) {
        3
    } else {
        panic!("sdram: no legal divider puts the memory clock under the chip's bound and over the part's minimum period at this posture")
    };
    let hz = hclk_hz / divider;
    let tmrd = part.t_mrd_ck;
    let txsr = at_least_one(ceil_cycles(part.t_xsr_ns, hz));
    let tras = at_least_one(ceil_cycles(part.t_ras_ns, hz));
    let trc = at_least_one(max(ceil_cycles(part.t_rc_ns, hz), ceil_cycles(part.t_rfc_ns, hz)));
    let trcd = at_least_one(ceil_cycles(part.t_rcd_ns, hz));
    let trp = at_least_one(ceil_cycles(part.t_rp_ns, hz));
    let twr = max(max(1 + ceil_cycles(part.t_wr_ns_after_clk, hz), tras.saturating_sub(trcd)), trc.saturating_sub(trcd + trp));
    assert!(tmrd >= 1 && tmrd <= 16, "sdram: TMRD outside the controller's 1 to 16 clocks");
    assert!(txsr <= 16 && tras <= 16 && trc <= 16 && twr <= 16 && trp <= 16 && trcd <= 16, "sdram: a delay over the controller's 16 clocks at this posture");
    let sdtr = ((trcd - 1) << 24) | ((trp - 1) << 20) | ((twr - 1) << 16) | ((trc - 1) << 12) | ((tras - 1) << 8) | ((txsr - 1) << 4) | (tmrd - 1);
    assert!(part.col_bits >= 8 && part.col_bits <= 11, "sdram: the controller addresses 8 to 11 column bits");
    assert!(part.row_bits >= 11 && part.row_bits <= 13, "sdram: the controller addresses 11 to 13 row bits");
    let mwid = match part.width_bits {
        8 => 0,
        16 => 1,
        32 => 2,
        _ => panic!("sdram: the controller drives an 8, 16 or 32-bit part"),
    };
    let nb = match part.bank_bits {
        1 => 0,
        2 => 1,
        _ => panic!("sdram: the controller drives a two- or four-bank part"),
    };
    assert!(part.cas >= 1 && part.cas <= 3, "sdram: the controller's CAS latency is 1, 2 or 3");
    // NC [1:0], NR [3:2], MWID [5:4], NB [6], CAS [8:7], WP [9] = 0, SDCLK
    // [11:10] (10 = HCLK/2, 11 = HCLK/3), RBURST [12] = 1, RPIPE [14:13] = 00.
    let sdcr = (part.col_bits - 8) | ((part.row_bits - 11) << 2) | (mwid << 4) | (nb << 6) | (part.cas << 7) | (divider << 10) | (1 << 12);
    // M[2:0] = 000 (burst length 1), bit 3 = 0 (sequential), M[6:4] = the
    // latency, M[8:7] = 00 (standard), bit 9 = 1 (single location access).
    let mode = (part.cas << 4) | (1 << 9);
    // COUNT = (refresh period / rows) in memory clocks, minus 20 (the
    // manual's formula); at least 41; never the forbidden sum.
    let per_row = hz as u64 * part.t_ref_ms as u64 / 1000 / part.refresh_rows as u64;
    assert!(per_row > 20, "sdram: the refresh period per row is under the controller's margin at this clock");
    let refresh_count = (per_row - 20) as u32;
    assert!(refresh_count >= 41, "sdram: the refresh count is under the controller's minimum of 41");
    assert!(refresh_count != twr + trp + trc + trcd + 4, "sdram: the refresh count equals TWR + TRP + TRC + TRCD + 4, which the controller forbids");
    assert!(refresh_count < (1 << 13), "sdram: the refresh count overflows its 13-bit field");
    SdramWords { sdclk_hz: hz, divider, sdcr, sdtr, mode, refresh_count, cas: part.cas }
}

/// The words for this image's posture: HCLK = SYSCLK on both of the
/// family's postures (the AHB prescaler stays at 1; the family module's
/// clock code), so the bus clock is the row's `SYSCLK_HZ`.
pub const WORDS: SdramWords = derive(&MT48LC4M32B2_6A, crate::board::SYSCLK_HZ, SDCLK_BOUND_HZ);

// The two postures' words, pinned (HCLK/3 =
// 72 MHz at PLL216 since HCLK/2 = 108 MHz is over the bound; HCLK/2 = 8 MHz
// at the HSI default).
#[cfg(stm32f7_pll216)]
const _: () = {
    assert!(WORDS.divider == 3 && WORDS.sdclk_hz == 72_000_000, "sdram at PLL216: the memory clock is HCLK/3 = 72 MHz");
    assert!(WORDS.sdtr == 0x0111_4341, "sdram at PLL216: FMC_SDTR1 is 0x01114341 (TRCD 2, TRP 2, TWR 2, TRC 5, TRAS 4, TXSR 5, TMRD 2 clocks)");
    assert!(WORDS.sdcr == 0x1DE4, "sdram at PLL216: FMC_SDCR1 is 0x1DE4");
    assert!(WORDS.refresh_count == 1105, "sdram at PLL216: the refresh count is 1105");
};
#[cfg(not(stm32f7_pll216))]
const _: () = {
    assert!(WORDS.divider == 2 && WORDS.sdclk_hz == 8_000_000, "sdram at HSI: the memory clock is HCLK/2 = 8 MHz");
    assert!(WORDS.sdtr == 0x0001_0001, "sdram at HSI: FMC_SDTR1 is 0x00010001 (every delay one clock but TWR and TMRD at two)");
    assert!(WORDS.sdcr == 0x19E4, "sdram at HSI: FMC_SDCR1 is 0x19E4");
    assert!(WORDS.refresh_count == 105, "sdram at HSI: the refresh count is 105");
};
const _: () = assert!(WORDS.mode == 0x230, "sdram: the mode register is 0x230 (burst length 1, sequential, CAS 3, standard, single-location writes)");

// ---------------- the registers ----------------

const RCC: usize = 0x4002_3800;
const RCC_AHB1ENR: *mut u32 = (RCC + 0x30) as *mut u32;
const RCC_AHB3ENR: *mut u32 = (RCC + 0x38) as *mut u32;
const RCC_APB1ENR: *mut u32 = (RCC + 0x40) as *mut u32;
/// RCC_AHB3ENR bit 0: the flexible memory controller's clock.
const AHB3ENR_FMCEN: u32 = 1 << 0;
/// RCC_APB1ENR: the power controller's interface (bit 28) and the RTC's
/// register interface (bit 10, RTCAPBEN), the backup registers' bus.
const APB1ENR_PWREN: u32 = 1 << 28;
const APB1ENR_RTCAPBEN: u32 = 1 << 10;

/// The port blocks: GPIOA at 0x4002_0000, one 1 KB block per port, their
/// clock enables the same bit positions in RCC_AHB1ENR; the mode register
/// at a block's base, the output type at 0x04, the speed at 0x08, the
/// pull at 0x0C, the alternate-function pair at 0x20 and 0x24.
const GPIO_BASE: usize = 0x4002_0000;
const GPIO_PORT_STRIDE: usize = 0x400;
const GPIO_MODER: usize = 0x00;
const GPIO_OTYPER: usize = 0x04;
const GPIO_OSPEEDR: usize = 0x08;
const GPIO_PUPDR: usize = 0x0C;
const GPIO_AFRL: usize = 0x20;
const GPIO_AFRH: usize = 0x24;
/// Every FMC pin carries alternate function 12 (the chip datasheet's
/// alternate-function table).
const FMC_AF: u32 = 12;

/// The controller's SDRAM registers (RM0410, the FMC register map at
/// 0xA000_0000): bank 1's control and timing, the command, the refresh
/// timer and the status.
const FMC: usize = 0xA000_0000;
const FMC_SDCR1: *mut u32 = (FMC + 0x140) as *mut u32;
const FMC_SDTR1: *mut u32 = (FMC + 0x148) as *mut u32;
const FMC_SDCMR: *mut u32 = (FMC + 0x150) as *mut u32;
const FMC_SDRTR: *mut u32 = (FMC + 0x154) as *mut u32;
const FMC_SDSR: *const u32 = (FMC + 0x158) as *const u32;
/// FMC_SDCMR: MODE [2:0], CTB1 bit 4 (bank 1 is the target), NRFS [8:5]
/// (the auto-refresh commands minus one), MRD [21:9] (the mode register).
const CMD_CTB1: u32 = 1 << 4;
const CMD_NRFS_SHIFT: u32 = 5;
const CMD_MRD_SHIFT: u32 = 9;
/// The auto-refresh commands sent at the wake-up: the part asks at least
/// two; eight is the makers' common practice, costs eight tRFC (under half
/// a microsecond) and leaves margin.
pub const INIT_REFRESHES: u32 = 8;
const _: () = assert!(INIT_REFRESHES >= MT48LC4M32B2_6A.init_refreshes_min && INIT_REFRESHES <= 16, "sdram: the wake-up's auto-refresh count is outside the part's minimum and the field's 16");
const CMD_CLOCK_ENABLE: u32 = 0b001 | CMD_CTB1;
const CMD_PALL: u32 = 0b010 | CMD_CTB1;
const CMD_AUTO_REFRESH: u32 = 0b011 | CMD_CTB1 | ((INIT_REFRESHES - 1) << CMD_NRFS_SHIFT);
const CMD_LOAD_MODE: u32 = 0b100 | CMD_CTB1 | (WORDS.mode << CMD_MRD_SHIFT);
const _: () = assert!(CMD_CLOCK_ENABLE == 0x11 && CMD_PALL == 0x12 && CMD_AUTO_REFRESH == 0xF3 && CMD_LOAD_MODE == 0x4_6014, "sdram: the command words are the pinned ones");
/// FMC_SDRTR: COUNT [13:1]; CRE bit 0 clears the refresh error flag.
const SDRTR_CRE: u32 = 1 << 0;
/// FMC_SDSR: RE bit 0 (a refresh error), BUSY bit 5 (a command in flight).
const SDSR_RE: u32 = 1 << 0;
const SDSR_BUSY: u32 = 1 << 5;
/// The bounded spin on the busy flag: a command completes in a few memory
/// clocks; this many polls is milliseconds at either posture.
const BUSY_BOUND: u32 = 1_000_000;

/// The pause after the clock reaches the part: ten times the part's 100 us,
/// by the core's cycle counter at the row's clock.
const PAUSE_CYCLES: u32 = crate::board::SYSCLK_HZ / 1_000_000 * (10 * MT48LC4M32B2_6A.init_pause_us);
/// The wait before the refresh error flag is read back clear: two refresh
/// intervals (the timer's period per row, 15.625 us) at the row's clock.
const REFRESH_CHECK_CYCLES: u32 = crate::board::SYSCLK_HZ / 1_000_000 * 32;

/// The core's cycle counter (the Data Watchpoint and Trace unit's DWT_CYCCNT),
/// armed by the chip module before each pause (the unit's software lock
/// opened, the counter proved to advance).
const DWT_CYCCNT: *mut u32 = 0xE000_1004 as *mut u32;

/// The power controller and the backup domain: PWR_CR1's DBP (bit 8)
/// unlocks the backup domain's registers; RTC_BKP0R is the first of the
/// backup registers at the RTC block, outside the RTC's own write
/// protection key, held across a system reset while the chip is powered.
const PWR_CR1: *mut u32 = 0x4000_7000 as *mut u32;
const PWR_CR1_DBP: u32 = 1 << 8;
const RTC_BKP0R: *mut u32 = (0x4000_2800 + 0x50) as *mut u32;

// ---------------- the pins ----------------

const PORT_D: u8 = 3;
const PORT_E: u8 = 4;
const PORT_F: u8 = 5;
const PORT_G: u8 = 6;
const PORT_H: u8 = 7;
const PORT_I: u8 = 8;

/// The 57 FMC pins of the board's SDRAM (UM2033 table 14, each pin checked
/// against the chip datasheet's alternate-function table; the schematic's
/// SDRAM sheet for the byte strobes). A12 is wired by the board though the
/// part's row uses A[11:0]: configured, harmless. Bank 1 alone: SDNE1 and
/// SDCKE1 are not wired.
pub const PINS: [(u8, u8); 57] = [
    // The address lines A0 to A12.
    (PORT_F, 0), (PORT_F, 1), (PORT_F, 2), (PORT_F, 3), (PORT_F, 4), (PORT_F, 5),
    (PORT_F, 12), (PORT_F, 13), (PORT_F, 14), (PORT_F, 15),
    (PORT_G, 0), (PORT_G, 1), (PORT_G, 2),
    // The bank lines BA0 and BA1.
    (PORT_G, 4), (PORT_G, 5),
    // The data lines D0 to D31.
    (PORT_D, 14), (PORT_D, 15), (PORT_D, 0), (PORT_D, 1),
    (PORT_E, 7), (PORT_E, 8), (PORT_E, 9), (PORT_E, 10), (PORT_E, 11), (PORT_E, 12), (PORT_E, 13), (PORT_E, 14), (PORT_E, 15),
    (PORT_D, 8), (PORT_D, 9), (PORT_D, 10),
    (PORT_H, 8), (PORT_H, 9), (PORT_H, 10), (PORT_H, 11), (PORT_H, 12), (PORT_H, 13), (PORT_H, 14), (PORT_H, 15),
    (PORT_I, 0), (PORT_I, 1), (PORT_I, 2), (PORT_I, 3), (PORT_I, 6), (PORT_I, 7), (PORT_I, 9), (PORT_I, 10),
    // The byte strobes NBL0 to NBL3 (the part's DQM0 to DQM3).
    (PORT_E, 0), (PORT_E, 1), (PORT_I, 4), (PORT_I, 5),
    // The control lines: SDCKE0, SDCLK, SDNCAS, SDNE0, SDNRAS, SDNWE. SDNCAS is
    // PG15 by the chip datasheet's own row (PG11 carries no FMC function).
    (PORT_H, 2), (PORT_G, 8), (PORT_G, 15), (PORT_H, 3), (PORT_F, 11), (PORT_H, 5),
];

/// No pin twice, every pin number inside its port.
const fn pins_sound(p: &[(u8, u8)]) -> bool {
    let mut i = 0;
    while i < p.len() {
        if p[i].1 > 15 {
            return false;
        }
        let mut j = i + 1;
        while j < p.len() {
            if p[i].0 == p[j].0 && p[i].1 == p[j].1 {
                return false;
            }
            j += 1;
        }
        i += 1;
    }
    true
}
const _: () = assert!(pins_sound(&PINS), "sdram: a pin is listed twice or lies outside its port");

/// One pin to the controller's alternate function: push-pull, no pull,
/// very high speed, the function's four bits in the low register for pins
/// 0 to 7 and the high one for 8 to 15.
unsafe fn route_fmc_pin(port: u8, pin: u8) {
    unsafe {
        let base = GPIO_BASE + port as usize * GPIO_PORT_STRIDE;
        let pin = pin as u32;
        let moder = (base + GPIO_MODER) as *mut u32;
        write_volatile(moder, (read_volatile(moder) & !(0b11 << (2 * pin))) | (0b10 << (2 * pin)));
        let otyper = (base + GPIO_OTYPER) as *mut u32;
        write_volatile(otyper, read_volatile(otyper) & !(1 << pin));
        let ospeedr = (base + GPIO_OSPEEDR) as *mut u32;
        write_volatile(ospeedr, read_volatile(ospeedr) | (0b11 << (2 * pin)));
        let pupdr = (base + GPIO_PUPDR) as *mut u32;
        write_volatile(pupdr, read_volatile(pupdr) & !(0b11 << (2 * pin)));
        let afr = (base + if pin < 8 { GPIO_AFRL } else { GPIO_AFRH }) as *mut u32;
        let shift = 4 * (pin % 8);
        write_volatile(afr, (read_volatile(afr) & !(0xF << shift)) | (FMC_AF << shift));
    }
}

/// One command to the part, then the controller's busy flag awaited under
/// the bound; a flag that never clears is a loud death naming the step.
unsafe fn command(word: u32, step: &'static str) {
    unsafe {
        write_volatile(FMC_SDCMR, word);
        let mut polls = 0u32;
        while read_volatile(FMC_SDSR) & SDSR_BUSY != 0 {
            polls += 1;
            if polls > BUSY_BOUND {
                panic!("sdram: the controller's busy flag never cleared during {}", step);
            }
        }
    }
}

/// A pause of `cycles` core clocks by the cycle counter.
unsafe fn pause(cycles: u32) {
    super::stm32f7::arm_cycle_counter();
    unsafe {
        let start = read_volatile(DWT_CYCCNT);
        while read_volatile(DWT_CYCCNT).wrapping_sub(start) < cycles {}
    }
}

/// The bring-up, in the manual's order: the ports'
/// and the controller's clocks; the 57 pins; the control and timing words;
/// the clock to the part and the pause it asks; precharge all; the
/// auto-refreshes; the mode register; the refresh timer; the refresh error
/// flag cleared and read back clear after two refresh intervals. The
/// sequence runs after every reset, whatever the part's state (a reset
/// during an access can leave the part driving the bus). Every refusal is
/// a loud death inside the call: the kernel never boots into a heap it
/// cannot trust.
pub fn bring_up() {
    unsafe {
        let mut ports = 0u32;
        let mut i = 0;
        while i < PINS.len() {
            ports |= 1 << PINS[i].0;
            i += 1;
        }
        write_volatile(RCC_AHB1ENR, read_volatile(RCC_AHB1ENR) | ports);
        let _ = read_volatile(RCC_AHB1ENR);
        write_volatile(RCC_AHB3ENR, read_volatile(RCC_AHB3ENR) | AHB3ENR_FMCEN);
        let _ = read_volatile(RCC_AHB3ENR);
        for (port, pin) in PINS {
            route_fmc_pin(port, pin);
        }
        #[cfg(feature = "test-sdram-doctored")]
        let sdcr = (WORDS.sdcr & !0b11) | 0b01;
        #[cfg(not(feature = "test-sdram-doctored"))]
        let sdcr = WORDS.sdcr;
        write_volatile(FMC_SDCR1, sdcr);
        write_volatile(FMC_SDTR1, WORDS.sdtr);
        command(CMD_CLOCK_ENABLE, "the clock configuration enable command");
        pause(PAUSE_CYCLES);
        command(CMD_PALL, "the precharge-all command");
        command(CMD_AUTO_REFRESH, "the auto-refresh command");
        command(CMD_LOAD_MODE, "the load-mode-register command");
        write_volatile(FMC_SDRTR, (WORDS.refresh_count << 1) | SDRTR_CRE);
        pause(REFRESH_CHECK_CYCLES);
        if read_volatile(FMC_SDSR) & SDSR_RE != 0 {
            panic!("sdram: the controller's refresh error flag set within two refresh intervals of the timer's start");
        }
    }
}

/// The proof's memory on this row: plain volatile word access through the
/// one view (the caches are off, the family module's law, so the "uncached
/// view" is the same address and the flush has nothing to maintain).
pub struct Plain;

impl Memory for Plain {
    unsafe fn write(&mut self, addr: usize, v: u32) {
        unsafe { write_volatile(addr as *mut u32, v) }
    }
    unsafe fn read(&mut self, addr: usize) -> u32 {
        unsafe { read_volatile(addr as *const u32) }
    }
    unsafe fn flush(&mut self) {}
}

/// A per-boot salt for the proof's pattern: a boot count kept in the
/// backup domain's first register, which a system reset leaves alone while
/// the chip stays powered -- the one case in which a pattern of a previous
/// boot could still sit in the part (a power cycle empties a DRAM within
/// seconds, and then the count restarts with it). Read, advanced, written
/// back and read again: a register that does not take the count is a loud
/// death (never a weaker salt taken quietly). The chip's random source is
/// not used: its clock is absent at the HSI posture (the family module's
/// entropy facts), and the proof wants a value a previous boot cannot have
/// used, nothing more. Not entropy for anything else.
pub fn boot_salt() -> u32 {
    unsafe {
        write_volatile(RCC_APB1ENR, read_volatile(RCC_APB1ENR) | APB1ENR_PWREN | APB1ENR_RTCAPBEN);
        let _ = read_volatile(RCC_APB1ENR);
        let cr1 = read_volatile(PWR_CR1);
        write_volatile(PWR_CR1, cr1 | PWR_CR1_DBP);
        let n = read_volatile(RTC_BKP0R).wrapping_add(1);
        write_volatile(RTC_BKP0R, n);
        let back = read_volatile(RTC_BKP0R);
        write_volatile(PWR_CR1, cr1);
        if back != n {
            panic!("sdram: the backup domain's first register did not take the boot count (wrote {:#010x}, read {:#010x})", n, back);
        }
        n.wrapping_mul(0x9E37_79B9)
    }
}
