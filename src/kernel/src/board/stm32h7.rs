//! The STM32H7 family module: the chip's supply configuration written at
//! the reset vector by register-only code, the boot posture, the two clock
//! postures, the console on USART1 driven the ring way, the random source
//! and the calendar seam every board of the family shares. Each board file
//! states its own facts -- the name, the console's pins, the external clock
//! source and whether it is a crystal or an oscillator, the oscillator's
//! enable line where one exists, the supply wiring, the PLL's two dividers
//! -- and re-exports the rest from here (the STM32F7 family module's
//! pattern).

use core::arch::global_asm;
use core::ptr::{read_volatile, write_volatile};

use super::ring;

// ---------------------------------------------------------------- postures

/// Clock posture 1 (the default image set): PLL1 at 400 MHz from the
/// board's external source at voltage scale 1; the system clock is the
/// CPU clock (D1CPRE /1), the AXI and AHB clocks are its half (HPRE /2), the
/// four APB clocks a quarter (every APB prescaler /2). The datasheet's
/// maxima at scale 1: CPU 400, AXI and AHB 200, APB 100 MHz (DS12930
/// Table 22).
#[cfg(not(stm32h7_hsi64))]
pub const SYSCLK_HZ: u32 = 400_000_000;
#[cfg(not(stm32h7_hsi64))]
const HCLK_HZ: u32 = SYSCLK_HZ / 2;
#[cfg(not(stm32h7_hsi64))]
const APB_DIV: u32 = 2;
#[cfg(not(stm32h7_hsi64))]
const CPU_MAX_HZ: u32 = 400_000_000;
#[cfg(not(stm32h7_hsi64))]
const HCLK_MAX_HZ: u32 = 200_000_000;
#[cfg(not(stm32h7_hsi64))]
const PCLK_MAX_HZ: u32 = 100_000_000;

/// Clock posture 2 (the board's `-hsi64` feature): the internal 64 MHz
/// oscillator the chip resets to, at the reset voltage scale 3, HPRE /1,
/// every APB prescaler /2 -- the datasheet's APB maximum at scale 3 is 50
/// MHz (DS12930 Table 22), so the reset state's 64 MHz APB clocks are not
/// kept. The PLLs stay off and the external source untouched.
#[cfg(stm32h7_hsi64)]
pub const SYSCLK_HZ: u32 = 64_000_000;
#[cfg(stm32h7_hsi64)]
const HCLK_HZ: u32 = SYSCLK_HZ;
#[cfg(all(stm32h7_hsi64, not(feature = "test-h7-apb-doctored")))]
const APB_DIV: u32 = 2;
/// The doctored control: the APB prescalers left at /1, which the maximum
/// below must refuse (a negative build case, never a shipped image).
#[cfg(all(stm32h7_hsi64, feature = "test-h7-apb-doctored"))]
const APB_DIV: u32 = 1;
#[cfg(stm32h7_hsi64)]
const CPU_MAX_HZ: u32 = 200_000_000;
#[cfg(stm32h7_hsi64)]
const HCLK_MAX_HZ: u32 = 100_000_000;
#[cfg(stm32h7_hsi64)]
const PCLK_MAX_HZ: u32 = 50_000_000;

/// The APB clock of both postures: the clock of USART1 (its kernel clock
/// selection left at the bus clock) and of every block without a selection.
const PCLK_HZ: u32 = HCLK_HZ / APB_DIV;
const _: () = assert!(SYSCLK_HZ <= CPU_MAX_HZ, "the CPU clock exceeds the datasheet's maximum at this voltage scale");
const _: () = assert!(HCLK_HZ <= HCLK_MAX_HZ, "the AXI and AHB clock exceeds the datasheet's maximum at this voltage scale");
const _: () = assert!(PCLK_HZ <= PCLK_MAX_HZ, "an APB clock exceeds the datasheet's maximum at this voltage scale (50 MHz at scale 3, 100 MHz at scale 1): the APB prescalers must divide");

/// SysTick rate.
pub const TICK_HZ: u32 = 1_000;

/// Stack for the kmain thread (the boot thread's continuation).
pub const KMAIN_STACK_BYTES: usize = 16 * 1024;

/// FPU shape: the STM32H747XI's Cortex-M7 carries the double-precision
/// floating-point unit (DS12930: "double-precision and single-precision
/// data-processing instructions"), so no row of this family runs the
/// emulation; the board facade ties the feature to this fact.
pub const FPU_DP: bool = true;

/// The cycle counter the lost-tick witness reads: the core's data
/// watchpoint and trace unit counts processor cycles, enabled and verified
/// to advance at the scheduler's init.
pub const CYCLE_COUNTER: bool = true;

/// External interrupt lines of the Cortex-M7's interrupt controller on this
/// chip: positions 0 (WWDG1) to 149 (WKUP) of RM0399's vector table (150
/// positions, 166 vector words with the sixteen exceptions) -- sizes the
/// vector table.
pub const NIRQ: usize = 150;

/// The vector line a test image may claim: position 94 (HDMI-CEC), a block
/// the kernel never clocks, its line claimed by no kernel handler.
#[allow(dead_code)] // board-facts surface; the api flavor's band-interrupt control is its consumer
pub const SPARE_IRQ: usize = 94;

/// Thread stack pool (lives in .kstacks): 96 KiB on the 512 KiB AXI SRAM
/// this image's DATA region is (the STM32F769 row's figure on the same 512 KiB);
/// the link asserts the arena window that remains above the row's floor.
pub const STACK_POOL_BYTES: usize = 96 * 1024;

/// The boot kheap carve out of the arena (the GC board table's
/// `kheap_carve` row): in a hosted image the C library's whole heap; 16K
/// as on the STM32F769 row.
pub const KHEAP_BYTES: usize = 16 * 1024;

/// RAM windows a hardware-pushed exception frame may plausibly live in: the
/// AXI SRAM, the one contiguous 512 KiB window this image uses (stacks live
/// here; each end backed off by one 8-word frame).
pub const FRAME_RANGES: [(usize, usize); 1] = [(0x2400_0000, 0x2408_0000 - 32)];

/// The chip, as the SDK's facts files name it (the quiet line of the log doors).
pub const CHIP: &str = "STM32H747XI";

// ------------------------------------------------------- the register map

/// The power controller (RM0399 7.4): the status register with ACTVOSRDY,
/// the supply configuration register, the D3 domain control register with
/// the voltage scale.
const PWR: usize = 0x5802_4800;
const PWR_CSR1_OFFSET: u32 = 0x04;
const PWR_CR3_OFFSET: u32 = 0x0C;
const PWR_D3CR: *mut u32 = (PWR + 0x18) as *mut u32;
const CSR1_ACTVOSRDY: u32 = 1 << 13;
const D3CR_VOS: u32 = 0b11 << 14;
#[cfg(not(stm32h7_hsi64))]
const D3CR_VOS_SCALE1: u32 = 0b11 << 14;
#[cfg(not(stm32h7_hsi64))]
const D3CR_VOSRDY: u32 = 1 << 13;

/// The chip's supply configurations a board of this family wires (RM0399
/// Table 33), as the board facts state them.
#[allow(dead_code)] // each board constructs its own wiring; the other variant is the derivation's
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Supply {
    /// The internal linear regulator alone (Table 33 ID 1): VDD on the
    /// regulator's input, the switching regulator's pins grounded, no
    /// inductor fitted.
    LdoOnly,
    /// The internal switching regulator at 1.8 V feeding the linear
    /// regulator (Table 33 ID 3): the inductor fitted, the switching
    /// regulator's output on VDDLDO.
    SmpsFeedsLdo18,
}

/// PWR_CR3's fields: BYPASS bit 0, LDOEN bit 1, SDEN bit 2, SDEXTHP bit 3,
/// SDLEVEL bits 5:4 (01 = 1.8 V).
const CR3_LDOEN: u32 = 1 << 1;
const CR3_SDEN: u32 = 1 << 2;
const CR3_SDLEVEL_1V8: u32 = 0b01 << 4;

/// The supply word a wiring takes -- derived, never typed.
pub const fn supply_word(supply: Supply) -> u32 {
    match supply {
        Supply::LdoOnly => CR3_LDOEN,
        Supply::SmpsFeedsLdo18 => CR3_SDLEVEL_1V8 | CR3_SDEN | CR3_LDOEN,
    }
}

/// The row's word: the board facts' wiring statement checked against it --
/// the switching regulator is enabled exactly where the board fits its
/// inductor; a board whose SMPS pins are grounded takes the linear
/// regulator alone (the doctored control forces the other wiring onto such
/// a board and this assert refuses the build).
const SUPPLY_WORD: u32 = supply_word(crate::board::SUPPLY);
const _: () = assert!(
    (SUPPLY_WORD & CR3_SDEN != 0) == crate::board::SMPS_INDUCTOR_FITTED,
    "the supply word enables the switching regulator on a board whose SMPS pins are grounded (no inductor), or leaves it off on a board wired for it: the board facts' wiring and its supply configuration disagree"
);
const _: () = assert!(SUPPLY_WORD == 0x0000_0002 || SUPPLY_WORD == 0x0000_0016, "the supply word is neither Table 33's configuration 1 nor its configuration 3");

global_asm!(
    ".section .text.__rhu_stm32h7_supply_reset, \"ax\", %progbits",
    ".global __rhu_stm32h7_supply_reset",
    ".global __rhu_stm32h7_supply_wait",
    ".type __rhu_stm32h7_supply_reset, %function",
    ".thumb_func",
    "__rhu_stm32h7_supply_reset:",
    "    movw r2, #0x4800",
    "    movt r2, #0x5802",
    "    movw r0, #{word}",
    "    str  r0, [r2, #{cr3}]",
    "    ldr  r0, [r2, #{cr3}]",
    "__rhu_stm32h7_supply_wait:",
    "    ldr  r1, [r2, #{csr1}]",
    "    tst  r1, #{rdy}",
    "    beq  __rhu_stm32h7_supply_wait",
    "    b    Reset",
    ".size __rhu_stm32h7_supply_reset, . - __rhu_stm32h7_supply_reset",
    word = const SUPPLY_WORD,
    cr3 = const PWR_CR3_OFFSET,
    csr1 = const PWR_CSR1_OFFSET,
    rdy = const CSR1_ACTVOSRDY,
);
const _: () = assert!(PWR + PWR_CSR1_OFFSET as usize == 0x5802_4804 && PWR + PWR_CR3_OFFSET as usize == 0x5802_480C, "the power controller's two registers of the entry");

unsafe extern "C" {
    /// The entry the row's reset vector names (the boot module reads it).
    #[link_name = "__rhu_stm32h7_supply_reset"]
    pub fn supply_reset();
}

/// The reset and clock control (RM0399 9.7).
const RCC: usize = 0x5802_4400;
const RCC_CR: *mut u32 = RCC as *mut u32;
const RCC_CFGR: *mut u32 = (RCC + 0x10) as *mut u32;
const RCC_D1CFGR: *mut u32 = (RCC + 0x18) as *mut u32;
const RCC_D2CFGR: *mut u32 = (RCC + 0x1C) as *mut u32;
const RCC_D3CFGR: *mut u32 = (RCC + 0x20) as *mut u32;
const RCC_PLLCKSELR: *mut u32 = (RCC + 0x28) as *mut u32;
#[cfg(not(stm32h7_hsi64))]
const RCC_PLLCFGR: *mut u32 = (RCC + 0x2C) as *mut u32;
const RCC_PLL1DIVR: *mut u32 = (RCC + 0x30) as *mut u32;
const RCC_D2CCIP2R: *mut u32 = (RCC + 0x54) as *mut u32;
const RCC_AHB2ENR: *mut u32 = (RCC + 0xDC) as *mut u32;
const RCC_AHB4ENR: *mut u32 = (RCC + 0xE0) as *mut u32;
const RCC_APB2ENR: *mut u32 = (RCC + 0xF0) as *mut u32;

/// RCC_CR: HSION 0, HSIRDY 2, HSIDIV 4:3, HSI48ON 12, HSI48RDY 13, HSEON
/// 16, HSERDY 17, HSEBYP 18, HSECSSON 19, PLL1ON 24, PLL1RDY 25, PLL2ON 26,
/// PLL3ON 28.
const CR_HSIDIV: u32 = 0b11 << 3;
const CR_HSI48ON: u32 = 1 << 12;
const CR_HSI48RDY: u32 = 1 << 13;
#[cfg(not(stm32h7_hsi64))]
const CR_HSEON: u32 = 1 << 16;
#[cfg(not(stm32h7_hsi64))]
const CR_HSERDY: u32 = 1 << 17;
#[cfg(not(stm32h7_hsi64))]
const CR_HSEBYP: u32 = 1 << 18;
const CR_PLL1ON: u32 = 1 << 24;
#[cfg(not(stm32h7_hsi64))]
const CR_PLL1RDY: u32 = 1 << 25;
const CR_PLL2ON: u32 = 1 << 26;
const CR_PLL3ON: u32 = 1 << 28;
const CR_ANY_PLLON: u32 = CR_PLL1ON | CR_PLL2ON | CR_PLL3ON;
/// RCC_CFGR: SW 2:0 and SWS 5:3 -- 000 HSI, 001 CSI, 010 HSE, 011 PLL1.
#[cfg(not(stm32h7_hsi64))]
const CFGR_SW: u32 = 0b111;
const CFGR_SWS: u32 = 0b111 << 3;
#[cfg(not(stm32h7_hsi64))]
const SW_PLL1: u32 = 0b011;
/// RCC_D1CFGR: HPRE 3:0 (0xxx /1, 1000 /2), D1PPRE 6:4 (0xx /1, 100 /2),
/// D1CPRE 11:8 (/1 at 0xxx); RCC_D2CFGR: D2PPRE1 6:4, D2PPRE2 10:8;
/// RCC_D3CFGR: D3PPRE 6:4 -- the APB fields with D1PPRE's encoding.
const D1CFGR_HPRE: u32 = 0b1111;
const D1CFGR_HPRE_DIV2: u32 = 0b1000;
const D1CFGR_D1PPRE: u32 = 0b111 << 4;
const D1CFGR_D1CPRE: u32 = 0b1111 << 8;
const D2CFGR_D2PPRE1: u32 = 0b111 << 4;
const D2CFGR_D2PPRE2: u32 = 0b111 << 8;
const D3CFGR_D3PPRE: u32 = 0b111 << 4;
/// The APB prescaler field value of this posture (/2 everywhere; the
/// doctored control's /1).
const PPRE_FIELD: u32 = if APB_DIV == 2 { 0b100 } else { 0b000 };
const D1CFGR_VALUE: u32 = (if HCLK_HZ == SYSCLK_HZ { 0 } else { D1CFGR_HPRE_DIV2 }) | (PPRE_FIELD << 4);
const D2CFGR_VALUE: u32 = (PPRE_FIELD << 4) | (PPRE_FIELD << 8);
const D3CFGR_VALUE: u32 = PPRE_FIELD << 4;
const _: () = assert!(HCLK_HZ == SYSCLK_HZ || HCLK_HZ * 2 == SYSCLK_HZ, "HPRE is /1 or /2 in this module");

/// The flash access control register (RM0399 4.9.1): LATENCY 3:0,
/// WRHIGHFREQ 5:4. Table 16 at voltage scale 1: an AXI clock of 185 to 210
/// MHz takes 2 wait states with WRHIGHFREQ 10. The register resets to 7 wait
/// states (legal at any frequency), which the HSI64 posture keeps.
const FLASH_ACR: *mut u32 = 0x5200_2000 as *mut u32;
const ACR_LATENCY: u32 = 0b1111;
#[cfg(not(stm32h7_hsi64))]
const ACR_WRHIGHFREQ: u32 = 0b11 << 4;
#[cfg(not(stm32h7_hsi64))]
const ACR_PLL400: u32 = 2 | (0b10 << 4);
#[cfg(not(stm32h7_hsi64))]
const _: () = assert!(ACR_PLL400 == 0x22 && HCLK_HZ > 185_000_000 && HCLK_HZ <= 210_000_000, "the flash latency word is Table 16's row for the AXI clock of this posture");

/// The debug block's control register (RM0399 chapter 62, DBGMCU_CR at
/// 0x5C00_1004): DBGSLEEP_D1 bit 0, D1DBGCKEN bit 21, D3DBGCKEN bit 22.
/// Without them the debug port is unreadable whenever the D1 domain sleeps,
/// so a core parked in the idle thread's wait could not be examined; set by
/// the kernel, never left to the probe's script.
const DBGMCU_CR: *mut u32 = 0x5C00_1004 as *mut u32;
const DBGMCU_DEBUG_CLOCKS: u32 = (1 << 22) | (1 << 21) | 1;
const _: () = assert!(DBGMCU_DEBUG_CLOCKS == 0x0060_0001, "the debug block's three bits");

/// RCC_AHB2ENR: SRAM1EN 29, SRAM2EN 30, SRAM3EN 31 -- the D2 domain's RAM
/// blocks allocated to this CPU (the manual calls them allocation bits; the
/// blocks are unusable before them); RNGEN 6.
const AHB2ENR_SRAM_EN: u32 = (1 << 29) | (1 << 30) | (1 << 31);
const AHB2ENR_RNGEN: u32 = 1 << 6;
/// RCC_AHB4ENR: GPIOAEN bit 0 to GPIOKEN bit 10, in port order.
const fn ahb4enr_gpio(port: usize) -> u32 {
    1 << port
}
/// RCC_APB2ENR: USART1EN bit 4 (the bus clock and the kernel clock).
const APB2ENR_USART1EN: u32 = 1 << 4;
/// RCC_D2CCIP2R: USART16SEL 5:3 (000 = the APB2 clock), RNGSEL 9:8 (00 =
/// the 48 MHz internal oscillator).
const D2CCIP2R_USART16SEL: u32 = 0b111 << 3;
const D2CCIP2R_RNGSEL: u32 = 0b11 << 8;

/// RCC_PLLCKSELR: PLLSRC 1:0 (00 HSI, 10 HSE; one field for all three
/// PLLs, written only with every PLL off), DIVM1 9:4 (1 to 63), DIVM2 17:12
/// and DIVM3 25:20 (left at their reset /32, their PLLs off).
#[cfg(not(stm32h7_hsi64))]
const PLLSRC_HSE: u32 = 0b10;
#[cfg(not(stm32h7_hsi64))]
const PLLCKSELR_DIVM23_RESET: u32 = (0x20 << 20) | (0x20 << 12);
/// RCC_PLLCFGR: PLL1FRACEN 0, PLL1VCOSEL 1 (0 = the wide range, 192 to 960
/// MHz), PLL1RGE 3:2 (10 = a reference of 4 to 8 MHz), DIVP1EN 16, DIVQ1EN
/// 17, DIVR1EN 18; PLL2's and PLL3's fields left at zero (their outputs
/// disabled).
#[cfg(not(stm32h7_hsi64))]
const PLLCFGR_PLL1RGE_4_8: u32 = 0b10 << 2;
#[cfg(not(stm32h7_hsi64))]
const PLLCFGR_DIVP1EN: u32 = 1 << 16;
#[cfg(not(stm32h7_hsi64))]
const PLLCFGR_VALUE: u32 = PLLCFGR_DIVP1EN | PLLCFGR_PLL1RGE_4_8;
/// RCC_PLL1DIVR: DIVN1 8:0 (the multiplier less one, 4 to 512), DIVP1 15:9,
/// DIVQ1 22:16, DIVR1 30:24 (each the divider less one; DIVP1 even only;
/// the disabled Q and R outputs at 1).
#[cfg(not(stm32h7_hsi64))]
const DIVM1: u32 = crate::board::PLL_DIVM1;
#[cfg(all(not(stm32h7_hsi64), not(feature = "test-h7-pll-doctored")))]
const PLL_N: u32 = crate::board::PLL_DIVN1;
/// The doctored control: the multiplier moved so the VCO leaves its range,
/// which the asserts below must refuse (a negative build case, never a
/// shipped image).
#[cfg(all(not(stm32h7_hsi64), feature = "test-h7-pll-doctored"))]
const PLL_N: u32 = crate::board::PLL_DIVN1 + 100;
#[cfg(not(stm32h7_hsi64))]
const PLL_P: u32 = 2;
#[cfg(not(stm32h7_hsi64))]
const PLL_REF_HZ: u64 = crate::board::HSE_HZ as u64 / DIVM1 as u64;
#[cfg(not(stm32h7_hsi64))]
const PLL_VCO_HZ: u64 = crate::board::HSE_HZ as u64 * PLL_N as u64 / DIVM1 as u64;
#[cfg(not(stm32h7_hsi64))]
const _: () = assert!(DIVM1 >= 1 && DIVM1 <= 63, "DIVM1 is 1 to 63");
#[cfg(not(stm32h7_hsi64))]
const _: () = assert!(PLL_N >= 4 && PLL_N <= 512, "DIVN1 is 4 to 512");
#[cfg(not(stm32h7_hsi64))]
const _: () = assert!(PLL_P % 2 == 0 && PLL_P >= 2 && PLL_P <= 128, "DIVP1 is even, 2 to 128");
#[cfg(not(stm32h7_hsi64))]
const _: () = assert!(PLL_REF_HZ >= 4_000_000 && PLL_REF_HZ < 8_000_000, "the PLL reference is inside the 4 to 8 MHz band PLL1RGE names, below the band's boundary");
#[cfg(not(stm32h7_hsi64))]
const _: () = assert!(PLL_VCO_HZ >= 192_000_000 && PLL_VCO_HZ <= 960_000_000, "the PLL's VCO leaves its wide range (192 to 960 MHz)");
#[cfg(not(stm32h7_hsi64))]
const _: () = assert!((crate::board::HSE_HZ as u64 * PLL_N as u64) % (DIVM1 as u64 * PLL_P as u64) == 0, "the PLL output is not a whole number of hertz");
#[cfg(not(stm32h7_hsi64))]
const _: () = assert!(PLL_VCO_HZ / PLL_P as u64 == SYSCLK_HZ as u64, "the PLL output is not the posture's system clock");
#[cfg(not(stm32h7_hsi64))]
const _: () = assert!(PLL_VCO_HZ / PLL_P as u64 <= 400_000_000, "the PLL output exceeds the datasheet's maximum at voltage scale 1");
#[cfg(not(stm32h7_hsi64))]
const PLLCKSELR_VALUE: u32 = PLLCKSELR_DIVM23_RESET | (DIVM1 << 4) | PLLSRC_HSE;
#[cfg(not(stm32h7_hsi64))]
const PLL1DIVR_VALUE: u32 = (1 << 24) | (1 << 16) | ((PLL_P - 1) << 9) | (PLL_N - 1);

/// A bounded wait's iteration count: at least 100 ms at the 64 MHz the core
/// runs at until the switch (the body is a peripheral read of several
/// cycles; four is the floor) -- long against every start-up here: a
/// crystal's start-up is milliseconds (DS12930), the PLL's lock and the
/// regulator's settle under a millisecond. The bound is a fault detector,
/// never a wait that ends on time.
#[cfg(not(stm32h7_hsi64))]
const WAIT_ITERATIONS: u32 = 64_000_000 / 10 / 4;

/// An oscillator's start-up after its enable line rises: the Portentas'
/// 25 MHz part specifies 1.5 ms at most from 90 percent of its supply to a
/// valid output (DS20006155, tSU), taken three times over and counted at
/// the 64 MHz the core runs at here.
#[cfg(not(stm32h7_hsi64))]
const OSC_START_ITERATIONS: u32 = 64_000_000 / 1000 * 5 / 4;

/// The loud stop (the rule of the default): the step named, the four clock
/// registers as they read, the clock left as it stands, the kernel's death
/// path -- the message lands in the memory console, the machine parks.
fn clock_stop(what: &str) -> ! {
    unsafe {
        panic!(
            "[clock] {} did not come within the bound: no fall-back, the clock left as it stands; RCC_CR={:#010x} RCC_CFGR={:#010x} PWR_D3CR={:#010x} FLASH_ACR={:#010x}",
            what,
            read_volatile(RCC_CR),
            read_volatile(RCC_CFGR),
            read_volatile(PWR_D3CR),
            read_volatile(FLASH_ACR)
        )
    }
}

/// Wait for the field under the mask to read the value, bounded.
#[cfg(not(stm32h7_hsi64))]
unsafe fn wait_for(reg: *mut u32, mask: u32, value: u32, what: &str) {
    let mut n = 0u32;
    while unsafe { read_volatile(reg) } & mask != value {
        n += 1;
        if n > WAIT_ITERATIONS {
            clock_stop(what);
        }
    }
}

/// A counted delay of peripheral reads (the tick is not running at boot).
#[cfg(not(stm32h7_hsi64))]
fn spin(iterations: u32) {
    for _ in 0..iterations {
        unsafe {
            let _ = read_volatile(RCC_CR);
        }
    }
}

/// The two steps before any posture: the debug clocks kept alive in the
/// domain's sleep states, and the D2 domain's three RAM blocks allocated to
/// this CPU and read back.
unsafe fn debug_and_sram_enables() {
    unsafe {
        write_volatile(DBGMCU_CR, read_volatile(DBGMCU_CR) | DBGMCU_DEBUG_CLOCKS);
        write_volatile(RCC_AHB2ENR, read_volatile(RCC_AHB2ENR) | AHB2ENR_SRAM_EN);
        if read_volatile(RCC_AHB2ENR) & AHB2ENR_SRAM_EN != AHB2ENR_SRAM_EN {
            clock_stop("the D2 domain's RAM allocation bits (RCC_AHB2ENR SRAM1EN, SRAM2EN, SRAM3EN) read back");
        }
    }
}

/// The reset state asserted, not assumed: the system clock the internal
/// oscillator at its reset division, every PLL off. A self-booting image
/// arrives this way after any reset; a resident that left another state is
/// a finding, not a thing to repair quietly.
unsafe fn assert_reset_clock_state() {
    unsafe {
        if read_volatile(RCC_CFGR) & CFGR_SWS != 0 || read_volatile(RCC_CR) & (CR_ANY_PLLON | CR_HSIDIV) != 0 {
            clock_stop("the clock tree's reset state (the system clock is not the undivided internal oscillator with every PLL off)");
        }
    }
}

/// The prescalers of the posture, written and read back.
unsafe fn program_prescalers() {
    unsafe {
        write_volatile(RCC_D1CFGR, (read_volatile(RCC_D1CFGR) & !(D1CFGR_HPRE | D1CFGR_D1PPRE | D1CFGR_D1CPRE)) | D1CFGR_VALUE);
        write_volatile(RCC_D2CFGR, (read_volatile(RCC_D2CFGR) & !(D2CFGR_D2PPRE1 | D2CFGR_D2PPRE2)) | D2CFGR_VALUE);
        write_volatile(RCC_D3CFGR, (read_volatile(RCC_D3CFGR) & !D3CFGR_D3PPRE) | D3CFGR_VALUE);
        if read_volatile(RCC_D1CFGR) & (D1CFGR_HPRE | D1CFGR_D1PPRE | D1CFGR_D1CPRE) != D1CFGR_VALUE
            || read_volatile(RCC_D2CFGR) & (D2CFGR_D2PPRE1 | D2CFGR_D2PPRE2) != D2CFGR_VALUE
            || read_volatile(RCC_D3CFGR) & D3CFGR_D3PPRE != D3CFGR_VALUE
        {
            clock_stop("the domain prescalers read back");
        }
    }
}

/// Enforce the PLL400 posture in the manual's order (RM0399 9.5.7 and
/// 4.3.8): the voltage scale first, the flash latency before anything runs
/// fast, the external source started (an oscillator's enable line driven
/// and its start-up waited out, the bypass bit written while the source is
/// off), the PLL programmed and locked, the prescalers before the switch so
/// no bus overshoots its cap for even a cycle, then the switch. Every wait
/// bounded; a failed wait is the loud stop. The clock security system
/// stays off.
#[cfg(not(stm32h7_hsi64))]
pub fn clock_init() {
    unsafe {
        debug_and_sram_enables();
        assert_reset_clock_state();

        // 1. Voltage scale 1 before the frequency rises.
        write_volatile(PWR_D3CR, (read_volatile(PWR_D3CR) & !D3CR_VOS) | D3CR_VOS_SCALE1);
        wait_for(PWR_D3CR, D3CR_VOSRDY, D3CR_VOSRDY, "the voltage scale 1 ready flag (PWR_D3CR VOSRDY)");

        // 2. The flash latency of the target AXI clock, read back.
        write_volatile(FLASH_ACR, (read_volatile(FLASH_ACR) & !(ACR_LATENCY | ACR_WRHIGHFREQ)) | ACR_PLL400);
        if read_volatile(FLASH_ACR) & (ACR_LATENCY | ACR_WRHIGHFREQ) != ACR_PLL400 {
            clock_stop("the flash latency word read back (FLASH_ACR)");
        }

        // 3. The external source: an oscillator's enable line first, the
        //    bypass bit while the source is off, the source on, its ready flag.
        if let Some((port, pin)) = crate::board::OSC_ENABLE {
            drive_high(port, pin);
            spin(OSC_START_ITERATIONS);
        }
        let cr = read_volatile(RCC_CR) & !(CR_HSEON | CR_HSEBYP);
        write_volatile(RCC_CR, if crate::board::HSE_BYPASS { cr | CR_HSEBYP } else { cr });
        write_volatile(RCC_CR, read_volatile(RCC_CR) | CR_HSEON);
        wait_for(
            RCC_CR,
            CR_HSERDY,
            CR_HSERDY,
            if crate::board::HSE_BYPASS {
                "the external oscillator's ready flag (RCC_CR HSERDY: the oscillator gives no clock)"
            } else {
                "the external crystal's ready flag (RCC_CR HSERDY: the crystal did not start)"
            },
        );

        // 4 to 6. The PLL: its source and first divider, its range and output
        //    enables, its multiplier and output divider -- every PLL off, so
        //    the fields are writable.
        write_volatile(RCC_PLLCKSELR, PLLCKSELR_VALUE);
        write_volatile(RCC_PLLCFGR, PLLCFGR_VALUE);
        write_volatile(RCC_PLL1DIVR, PLL1DIVR_VALUE);

        // 7. The PLL on and locked.
        write_volatile(RCC_CR, read_volatile(RCC_CR) | CR_PLL1ON);
        wait_for(RCC_CR, CR_PLL1RDY, CR_PLL1RDY, "the PLL's lock (RCC_CR PLL1RDY)");

        // 8. The prescalers, then 9. the switch.
        program_prescalers();
        write_volatile(RCC_CFGR, (read_volatile(RCC_CFGR) & !CFGR_SW) | SW_PLL1);
        wait_for(RCC_CFGR, CFGR_SWS, SW_PLL1 << 3, "the system clock switch to the PLL (RCC_CFGR SWS)");
    }
    ring::boot_mark();
}

/// Enforce the HSI64 posture: the reset state asserted, the APB prescalers
/// to /2 for the datasheet's 50 MHz APB maximum at the reset voltage scale,
/// the flash latency left at its reset 7 wait states (legal at any
/// frequency), the PLLs off, the external source and an oscillator's enable
/// line untouched.
#[cfg(stm32h7_hsi64)]
pub fn clock_init() {
    unsafe {
        debug_and_sram_enables();
        assert_reset_clock_state();
        program_prescalers();
    }
    ring::boot_mark();
}

/// One pin driven high as an output: the port's clock, the level in the
/// bit set register before the mode, the mode last (the pin leaves its reset
/// analog state already at its level).
#[cfg(not(stm32h7_hsi64))]
unsafe fn drive_high(port: usize, pin: u32) {
    unsafe {
        write_volatile(RCC_AHB4ENR, read_volatile(RCC_AHB4ENR) | ahb4enr_gpio(port));
        let _ = read_volatile(RCC_AHB4ENR);
        let base = GPIO_BASE + port * GPIO_PORT_STRIDE;
        write_volatile((base + GPIO_BSRR) as *mut u32, 1 << pin);
        let moder = (base + GPIO_MODER) as *mut u32;
        write_volatile(moder, (read_volatile(moder) & !(0b11 << (2 * pin))) | (0b01 << (2 * pin)));
    }
}

/// The live clock tree for the banner line on both postures, every numeric
/// field decoded from registers (a silently-failed posture yields its real
/// clock here, and the kmain assert dies loudly). The system clock from the
/// switch status and, for the PLL, from the source the PLL names and its
/// three fields; the CPU prescaler applied; the flash latency and the
/// voltage scale read back; the prescalers checked against the posture's.
pub fn clock_readback() -> Option<crate::board::ClockReadback> {
    const HSI_HZ: u32 = 64_000_000;
    const CSI_HZ: u32 = 4_000_000;
    unsafe {
        let cr = read_volatile(RCC_CR);
        let cfgr = read_volatile(RCC_CFGR);
        let ckselr = read_volatile(RCC_PLLCKSELR);
        let divr = read_volatile(RCC_PLL1DIVR);
        let d1 = read_volatile(RCC_D1CFGR);
        let d2 = read_volatile(RCC_D2CFGR);
        let d3 = read_volatile(RCC_D3CFGR);
        let hsi_hz = HSI_HZ >> ((cr & CR_HSIDIV) >> 3);
        let m = (ckselr >> 4) & 0x3F;
        let n = (divr & 0x1FF) + 1;
        let p = ((divr >> 9) & 0x7F) + 1;
        let (src, base) = match ckselr & 0b11 {
            0b00 => ("HSI", hsi_hz),
            0b01 => ("CSI", CSI_HZ),
            0b10 => ("HSE", crate::board::HSE_HZ),
            _ => ("none", 0),
        };
        let (sws, sys_hz) = match (cfgr & CFGR_SWS) >> 3 {
            0b000 => ("HSI", hsi_hz),
            0b001 => ("CSI", CSI_HZ),
            0b010 => ("HSE", crate::board::HSE_HZ),
            // The multiplication first, as the compile-time PLL_VCO_HZ is formed:
            // dividing first rounds 16 MHz / 3 down and the GIGA's posture decodes
            // 25 Hz short, which the kmain tripwire rightly refuses).
            0b011 => ("PLL1", if m == 0 { 0 } else { (base as u64 * n as u64 / m as u64 / p as u64) as u32 }),
            _ => ("??", 0),
        };
        let d1cpre = (d1 & D1CFGR_D1CPRE) >> 8;
        // D1CPRE 1000 to 1011 divide by 2, 4, 8 and 16; 1100 to 1111 by 64,
        // 128, 256 and 512 (the table has no /32).
        let cpu_div = if d1cpre & 0b1000 == 0 { 1 } else { 2u32 << ((d1cpre & 0b111) + (d1cpre & 0b100) / 4) };
        let presc = d1 & (D1CFGR_HPRE | D1CFGR_D1PPRE | D1CFGR_D1CPRE) == D1CFGR_VALUE
            && d2 & (D2CFGR_D2PPRE1 | D2CFGR_D2PPRE2) == D2CFGR_VALUE
            && d3 & D3CFGR_D3PPRE == D3CFGR_VALUE;
        let boost = match ((read_volatile(PWR_D3CR) & D3CR_VOS) >> 14, presc) {
            (0b11, true) => "VOS1, the prescalers as programmed",
            (0b11, false) => "VOS1, the prescalers NOT the posture's",
            (0b01, true) => "VOS3, the prescalers as programmed",
            (0b01, false) => "VOS3, the prescalers NOT the posture's",
            (0b10, _) => "VOS2 (no posture of this kernel)",
            _ => "VOS unknown",
        };
        Some(crate::board::ClockReadback {
            posture: if cfg!(stm32h7_hsi64) { "HSI64" } else { "PLL400" },
            sysclk_hz: sys_hz / cpu_div,
            sws,
            src,
            pllm: m,
            plln: n,
            pllp: p,
            flash_ws: read_volatile(FLASH_ACR) & ACR_LATENCY,
            boost,
        })
    }
}

// ---------------------------------------------------------------- console

/// The port blocks: GPIOA at 0x5802_0000 on AHB4, one 1 KB block per port,
/// their clock enables the same bit positions in RCC_AHB4ENR; the mode
/// register at a block's base, the pull register at 0x0C, the bit set and
/// reset register at 0x18, the alternate-function pair at 0x20 and 0x24.
const GPIO_BASE: usize = 0x5802_0000;
const GPIO_PORT_STRIDE: usize = 0x400;
const GPIO_MODER: usize = 0x00;
const GPIO_PUPDR: usize = 0x0C;
#[cfg(not(stm32h7_hsi64))]
const GPIO_BSRR: usize = 0x18;
const GPIO_AFRL: usize = 0x20;
const GPIO_AFRH: usize = 0x24;
/// USART1's pins carry alternate function 7 on every port that routes
/// them (DS12930's alternate-function table: PA9, PA10, PB6, PB7).
const USART1_AF: u32 = 7;

const USART1: usize = 0x4001_1000;
const USART1_CR1: *mut u32 = USART1 as *mut u32;
const USART1_BRR: *mut u32 = (USART1 + 0x0C) as *mut u32;
const USART1_ISR: *mut u32 = (USART1 + 0x1C) as *mut u32;
const USART1_ICR: *mut u32 = (USART1 + 0x20) as *mut u32;
const USART1_RDR: *mut u32 = (USART1 + 0x24) as *mut u32;
const USART1_TDR: *mut u32 = (USART1 + 0x28) as *mut u32;

/// USART_CR1 without the FIFO (FIFOEN 0, the reset): UE 0, RE 2, TE 3, the
/// transmit-empty interrupt enable 7; USART_ISR: ORE 3, RXNE 5, TXE 7;
/// USART_ICR: ORECF 3. 8 data bits, one stop bit, no parity at the resets.
const CR1_UE: u32 = 1 << 0;
const CR1_RE: u32 = 1 << 2;
const CR1_TE: u32 = 1 << 3;
const CR1_TXEIE: u32 = 1 << 7;
const ISR_ORE: u32 = 1 << 3;
const ISR_RXNE: u32 = 1 << 5;
const ISR_TXE: u32 = 1 << 7;
const ICR_ORECF: u32 = 1 << 3;
const BAUD: u32 = 115_200;
/// The divisor at oversampling 16 from the APB2 clock (USART16SEL at the
/// bus clock): 868 at 100 MHz, 278 at 32 MHz, each within 0.1 percent.
const USART1_BRR_VALUE: u32 = (PCLK_HZ + BAUD / 2) / BAUD;
const _: () = assert!(USART1_BRR_VALUE >= 16 && USART1_BRR_VALUE <= 0xFFFF, "the baud divisor is 16 to 65535");

/// USART1's interrupt line (RM0399's vector table, position 37), its enable
/// bit in the interrupt controller's second set-enable and clear-pending
/// registers, and its priority byte in the priority array.
pub const USART1_IRQ: usize = 37;
const NVIC_ISER1: *mut u32 = 0xE000_E104 as *mut u32;
const NVIC_ICPR1: *mut u32 = 0xE000_E284 as *mut u32;
const NVIC_IPR: *mut u8 = 0xE000_E400 as *mut u8;
/// The lowest band: masked inside a kernel critical section (the ceiling
/// 0x40), below the tick (0x80), above the context switch (0xF0) -- the
/// drain preempts thread code and nothing else, and the writer's section
/// is its lock.
const USART1_IRQ_PRIO: u8 = 0xE0;

/// The interrupt lines a board of this family claims with its own
/// handlers, folded into the vector table by the boot module: USART1's
/// line, the console ring's transmit drain.
pub const BOARD_IRQS: &[(usize, unsafe extern "C" fn())] = &[(USART1_IRQ, __rhu_stm32h7_usart1_irq)];
/// The family GPIO driver's external-interrupt lines (the H7 arm).
pub use super::stm32_gpio::IRQS as GPIO_IRQS;

/// One pin to USART1's alternate function with a pull-up: the mode field's
/// two bits at twice the pin in the port's mode register, the function's
/// four bits in the low alternate-function register for pins 0 to 7 and
/// the high one for 8 to 15.
unsafe fn route_usart1_pin(port: usize, pin: u32) {
    unsafe {
        let base = GPIO_BASE + port * GPIO_PORT_STRIDE;
        let pupdr = (base + GPIO_PUPDR) as *mut u32;
        write_volatile(pupdr, (read_volatile(pupdr) & !(0b11 << (2 * pin))) | (0b01 << (2 * pin)));
        let afr = (base + if pin < 8 { GPIO_AFRL } else { GPIO_AFRH }) as *mut u32;
        let shift = 4 * (pin % 8);
        write_volatile(afr, (read_volatile(afr) & !(0xF << shift)) | (USART1_AF << shift));
        let moder = (base + GPIO_MODER) as *mut u32;
        write_volatile(moder, (read_volatile(moder) & !(0b11 << (2 * pin))) | (0b10 << (2 * pin)));
    }
}

/// Route the board's two pins to USART1 and program 115200 8N1 from the
/// APB2 clock, the kernel clock selection stated (the bus clock), never
/// trusted. Runs once, at the first console init that names the wire.
unsafe fn console_hw_init() {
    unsafe {
        let [(tx_port, tx_pin), (rx_port, rx_pin)] = crate::board::CONSOLE_PINS;
        write_volatile(RCC_AHB4ENR, read_volatile(RCC_AHB4ENR) | ahb4enr_gpio(tx_port) | ahb4enr_gpio(rx_port));
        let _ = read_volatile(RCC_AHB4ENR);
        write_volatile(RCC_D2CCIP2R, read_volatile(RCC_D2CCIP2R) & !D2CCIP2R_USART16SEL);
        write_volatile(RCC_APB2ENR, read_volatile(RCC_APB2ENR) | APB2ENR_USART1EN);
        let _ = read_volatile(RCC_APB2ENR);

        route_usart1_pin(tx_port, tx_pin);
        route_usart1_pin(rx_port, rx_pin);

        write_volatile(USART1_CR1, 0);
        write_volatile(USART1_BRR, USART1_BRR_VALUE);
        write_volatile(USART1_CR1, CR1_UE | CR1_TE | CR1_RE);
    }
}

/// The console driver's init. The wire exists only when a stream names the
/// probe channel: then the first call configures the hardware, the boot's
/// call (marked by `clock_init` through the ring module) arms the transmit
/// interrupt and takes the interrupt mode, and a later call is the panic or
/// fault path's re-init and switches to the polled mode. With no stream on
/// the wire the ring records the no-wire mode (the memory console) and no
/// pin is touched. The console mirror is not touched here, so a panic
/// transcript appends to the boot's.
pub fn console_init() {
    let wire = crate::console::wire_named(crate::console::CH_PROBE);
    unsafe {
        if wire && ring::mode() == 0 {
            console_hw_init();
        }
        if ring::take_boot_mark() || ring::mode() == 0 {
            if wire {
                write_volatile(NVIC_IPR.add(USART1_IRQ), USART1_IRQ_PRIO);
                write_volatile(NVIC_ICPR1, 1 << (USART1_IRQ - 32));
                write_volatile(NVIC_ISER1, 1 << (USART1_IRQ - 32));
                ring::set_mode(ring::MODE_IRQ);
            } else {
                ring::set_mode(ring::MODE_NO_WIRE);
            }
        } else if wire {
            ring_tx_irq(false);
            ring::set_mode(ring::MODE_POLLED);
        }
    }
}

/// The ring's four hooks on this USART: the transmit data register takes
/// a byte while its empty flag is set; a byte goes into the transmit data
/// register; nothing latches, so the acknowledge is empty; and the
/// transmit-empty interrupt is level-shaped, so its enable is set only
/// while the ring holds a backlog and cleared when the ring empties.
pub unsafe fn ring_tx_room() -> bool {
    unsafe { read_volatile(USART1_ISR) & ISR_TXE != 0 }
}
pub unsafe fn ring_tx_put(b: u8) {
    unsafe { write_volatile(USART1_TDR, b as u32) }
}
pub unsafe fn ring_tx_ack() {}
pub unsafe fn ring_tx_irq(enabled: bool) {
    unsafe {
        let cr1 = read_volatile(USART1_CR1);
        let want = if enabled { cr1 | CR1_TXEIE } else { cr1 & !CR1_TXEIE };
        if want != cr1 {
            write_volatile(USART1_CR1, want);
        }
    }
}

/// USART1's interrupt: the ring's drain in the interrupt mode, raised
/// while the transmit data register is empty and the enable is set.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __rhu_stm32h7_usart1_irq() {
    unsafe { ring::irq() }
}

/// Byte transmit through the shared ring (the mirror, the primed
/// transmitter, the interrupt drain; the polled drain on the panic and
/// fault paths). The caller holds the kernel critical section (the
/// console's line lock), which masks the interrupt.
pub fn console_putb(b: u8) {
    ring::putb(b)
}

/// The room the ring has for a writer, in bytes (the door's back-pressure
/// reads it outside the critical section).
pub fn console_room() -> usize {
    ring::room()
}

/// Non-blocking byte receive: one pending byte, or None immediately. A
/// latched overrun (missed polls) is cleared on the empty path so the
/// receiver never wedges; the lost bytes stay lost, honestly. Nothing to
/// read when no stream holds the wire (the block is unclocked then).
pub fn console_getb() -> Option<u8> {
    if ring::mode() != ring::MODE_IRQ && ring::mode() != ring::MODE_POLLED {
        return None;
    }
    unsafe {
        if read_volatile(USART1_ISR) & ISR_RXNE != 0 {
            return Some(read_volatile(USART1_RDR) as u8);
        }
        if read_volatile(USART1_ISR) & ISR_ORE != 0 {
            write_volatile(USART1_ICR, ICR_ORECF);
        }
        None
    }
}

/// Whether address 0 is readable memory on this row: the instruction
/// tightly coupled memory at 0 on this chip, 64 KB. With the null-page guard the
/// protection unit denies the first page on every row, so a guarded image
/// never attempts the read; the `null-guard-off` control image alone does.
#[allow(dead_code)] // read by the api flavor's observation only
pub const ADDRESS_ZERO_READABLE: bool = true;

// ---------------------------------------------------------------- entropy

/// The entropy posture of the family's rows, on BOTH clock postures: the
/// part's RNG block samples its noise source on the kernel clock RNGSEL
/// names, the 48 MHz internal oscillator at its reset value -- independent
/// of the PLL, so the source serves at the bring-up posture too (RM0399
/// chapter 34 and 9.7.21).
pub const ENTROPY: super::EntropyPosture = super::EntropyPosture::Source {
    id: 5,
    name: "STM32H7 RNG",
    line: "hardware source STM32H7 RNG enabled on the 48 MHz internal oscillator; its self-tests gate every draw",
};
/// Whether this row carries a real-time clock the kernel reads at boot and
/// writes at an explicit set.
#[allow(dead_code)] // read by the api flavor's calendar check
pub const RTC_PRESENT: bool = false;
pub use super::absent::{rtc_read_at_boot, rtc_write};

/// The RNG block (RM0399 chapter 34) at 0x4802_1800 on AHB2: RNG_CR with
/// CED bit 5 (0 = the clock error detection on, its reset value), IE bit 3,
/// RNGEN bit 2; RNG_SR with SEIS bit 6 and CEIS bit 5 (the seed and clock
/// error interrupt flags, cleared by writing 0), SECS bit 2 and CECS bit 1
/// (the current seed and clock error status, read-only), DRDY bit 0;
/// RNG_DR the 32-bit sample, valid when DRDY is set and the value is not
/// zero. With the clock check on, the block flags a clock error whenever
/// its clock is slower than a thirty-second of the AHB clock: 48 MHz
/// against 200 / 32 = 6.25 MHz clears it on both postures.
const RNG: usize = 0x4802_1800;
const RNG_CR: *mut u32 = RNG as *mut u32;
const RNG_SR: *mut u32 = (RNG + 0x04) as *mut u32;
const RNG_DR: *mut u32 = (RNG + 0x08) as *mut u32;
const RNG_CR_RNGEN: u32 = 1 << 2;
const RNG_SR_SEIS: u32 = 1 << 6;
const RNG_SR_CEIS: u32 = 1 << 5;
const RNG_SR_SECS: u32 = 1 << 2;
const RNG_SR_CECS: u32 = 1 << 1;
const RNG_SR_DRDY: u32 = 1 << 0;
const RNG_SR_ERRORS: u32 = RNG_SR_SEIS | RNG_SR_CEIS | RNG_SR_SECS | RNG_SR_CECS;
/// A bounded wait's iteration count for at least two milliseconds at the
/// posture's clock (the loop body is at least four cycles); at the boot the
/// tick clock is not yet running, so every wait here is an iteration bound.
/// The block delivers a word every 42 of its 48 MHz clocks and its first
/// word 46 clocks after the enable, three orders of magnitude inside the
/// bound: the bound is a fault detector, never a wait.
const SPIN_2MS: u32 = SYSCLK_HZ / 500 / 4;

/// The block started: the 48 MHz internal oscillator on and waited for
/// under a bound, the kernel clock selection stated (RNGSEL = that
/// oscillator), the bus clock enabled and read back, the block enabled
/// with the clock checker on, the first ready word waited for under the
/// bound, the status read. False when the oscillator did not start, the
/// checker or the noise source flagged, or no word came (the witness then
/// reads the source absent and every secure draw refuses). Nothing here
/// masks an interrupt: the boot path runs before the scheduler.
pub fn entropy_init() -> bool {
    unsafe {
        write_volatile(RCC_CR, read_volatile(RCC_CR) | CR_HSI48ON);
        let mut n = 0u32;
        while read_volatile(RCC_CR) & CR_HSI48RDY == 0 {
            n += 1;
            if n > SPIN_2MS {
                return false;
            }
        }
        write_volatile(RCC_D2CCIP2R, read_volatile(RCC_D2CCIP2R) & !D2CCIP2R_RNGSEL);
        write_volatile(RCC_AHB2ENR, read_volatile(RCC_AHB2ENR) | AHB2ENR_RNGEN);
        let _ = read_volatile(RCC_AHB2ENR);
        write_volatile(RNG_CR, RNG_CR_RNGEN);
        let mut n = 0u32;
        loop {
            let sr = read_volatile(RNG_SR);
            if sr & RNG_SR_ERRORS != 0 {
                return false;
            }
            if sr & RNG_SR_DRDY != 0 {
                return true;
            }
            n += 1;
            if n > SPIN_2MS {
                return false;
            }
        }
    }
}

/// One draw: per 32-bit word, THE WAIT OUTSIDE ANY SECTION -- the status
/// polled until a word is ready, an error flag shows or the bound is met --
/// then, under the kernel critical section, the status-and-data pair: the
/// status read again (an error flag is the health check failing at this
/// draw: counted, and the policy retires the source), a ready word read (a
/// zero word is the seed error that raced the status read, treated the
/// same), a word that was ready and is not any more sends the wait round
/// again. The section covers two or three register reads and never a wait,
/// so an interrupt at or above the ceiling is delayed by those reads alone
/// and the zero-latency band below it never. The bytes land in the caller's
/// buffer word by word; the policy layer copies out only when the whole
/// draw succeeded.
pub fn entropy_draw(buf: &mut [u8], counters: &mut super::DrawCounters) -> Result<(), super::EntropyErr> {
    for chunk in buf.chunks_mut(4) {
        let mut n = 0u32;
        let word = loop {
            let sr = unsafe { read_volatile(RNG_SR) };
            if sr & RNG_SR_ERRORS != 0 {
                break Err(super::EntropyErr::Health);
            }
            if sr & RNG_SR_DRDY != 0 {
                let pair = crate::critical::with_cs(|| unsafe {
                    let sr = read_volatile(RNG_SR);
                    if sr & RNG_SR_ERRORS != 0 {
                        return Err(super::EntropyErr::Health);
                    }
                    if sr & RNG_SR_DRDY == 0 {
                        return Ok(None);
                    }
                    let w = read_volatile(RNG_DR);
                    if w == 0 {
                        return Err(super::EntropyErr::Health);
                    }
                    Ok(Some(w))
                });
                match pair {
                    Ok(Some(w)) => break Ok(w),
                    Ok(None) => {}
                    Err(e) => break Err(e),
                }
            }
            n += 1;
            if n > SPIN_2MS {
                break Err(super::EntropyErr::Timeout);
            }
        };
        match word {
            Ok(w) => chunk.copy_from_slice(&w.to_le_bytes()[..chunk.len()]),
            Err(e) => {
                if matches!(&e, super::EntropyErr::Health) {
                    counters.errors += 1;
                }
                return Err(e);
            }
        }
    }
    Ok(())
}
