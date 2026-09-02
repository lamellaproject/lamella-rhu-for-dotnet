//! NUCLEO-U5A5ZJ-Q (STM32U5A5ZJ, Cortex-M33): the ARMv8-M silicon
//! board. Flash-boot at 0x08000000, USART1 console on the ST-LINK VCP.

use core::ptr::{read_volatile, write_volatile};

pub const NAME: &str = "NUCLEO-U5A5ZJ-Q";
pub const CONSOLE: &str = "USART1 (ST-LINK VCP)";

/// Clock posture 1 (default): HSI16, SYSCLK = HCLK = PCLK1 = PCLK2 =
/// 16 MHz (all prescalers div1). `clock_init` enforces exactly this
/// posture.
#[cfg(not(feature = "nucleo-u5a5zj-q-pll160"))]
pub const SYSCLK_HZ: u32 = 16_000_000;
/// Clock posture 2 (`nucleo-u5a5zj-q-pll160`): PLL1 at 160 MHz off
/// HSI16 -- 16 MHz /1 into the VCO, x20 = 320 MHz (inside the 128-544
/// MHz VCO window), /2 on the R output lands SYSCLK at 160 MHz, the
/// VCORE range-1 posture. HCLK = PCLK1 = PCLK2 = SYSCLK (div1: on this
/// part every bus runs to 160 MHz in range 1, so no cap prescalers).
#[cfg(feature = "nucleo-u5a5zj-q-pll160")]
pub const SYSCLK_HZ: u32 = 160_000_000;
/// USART1's kernel clock is PCLK2 (RCC_CCIPR1.USART1SEL = 00, stated
/// explicitly by `console_init`), and PCLK2 = SYSCLK under div1.
const PCLK2_HZ: u32 = SYSCLK_HZ;

/// SysTick rate.
pub const TICK_HZ: u32 = 1_000;

/// Thread stack pool (lives in .kstacks) -- the same budgets as the
/// other boards; 2496 K of internal SRAM leaves wide headroom (the link
/// asserts it).
pub const STACK_POOL_BYTES: usize = 64 * 1024;

/// The boot kheap carve out of the arena (the GC board table's
/// `kheap_carve` row). Sized conservatively rather than from measured
/// demand.
pub const KHEAP_BYTES: usize = 64 * 1024;

/// Stack for the kmain thread (the boot thread's continuation).
pub const KMAIN_STACK_BYTES: usize = 16 * 1024;

/// External IRQ lines: the full depth of the RM0456 STM32U5 series
/// vector table (positions 0..=140), so every position the manual
/// names has a funnel entry. The U5A5's last present line is DCACHE2
/// at position 138; the two lines above it belong to other family
/// members and are reserved on this part -- a reserved line never
/// fires, and its slot costs eight bytes of flash.
pub const NIRQ: usize = 141;

/// FPU shape: FPv5 SINGLE precision (the Cortex-M33 FPU) -- kernel
/// f64 stays on the soft-float path. Consumed by records and gates,
/// never by context-switch code (lazy stacking is FP-shape-agnostic).
#[allow(dead_code)]
pub const FPU_DP: bool = false;

/// RAM windows a hardware-pushed exception frame may plausibly live in:
/// the one contiguous internal SRAM span this image uses as DATA
/// (SRAM1+2+3+5, 0x20000000..0x20270000), each end backed off by one
/// 8-word frame. SRAM4 (the SmartRun domain, 0x28000000) is outside
/// this map, so no frame may legitimately point there.
pub const FRAME_RANGES: [(usize, usize); 1] = [(0x2000_0000, 0x2027_0000 - 32)];

const RCC: usize = 0x4602_0C00;
const RCC_CR: *mut u32 = RCC as *mut u32;
const RCC_CFGR1: *mut u32 = (RCC + 0x1C) as *mut u32;
const RCC_CFGR2: *mut u32 = (RCC + 0x20) as *mut u32;
const RCC_AHB2ENR1: *mut u32 = (RCC + 0x8C) as *mut u32;
const RCC_APB2ENR: *mut u32 = (RCC + 0xA4) as *mut u32;
const RCC_CCIPR1: *mut u32 = (RCC + 0xE0) as *mut u32;

const CR_HSION: u32 = 1 << 8;
const CR_HSIRDY: u32 = 1 << 10;
/// CFGR1 fields: SW [1:0] (00 MSIS, 01 HSI16, 10 HSE, 11 PLL1),
/// SWS [3:2] with the same encoding.
const CFGR1_SW: u32 = 0b11;
const CFGR1_SW_HSI16: u32 = 0b01;
const CFGR1_SWS: u32 = 0b11 << 2;
const CFGR1_SWS_HSI16: u32 = 0b01 << 2;
/// CFGR2 fields: HPRE [3:0], PPRE1 [6:4], PPRE2 [10:8]. Zero in every
/// field = div1 everywhere.
const CFGR2_HPRE: u32 = 0b1111;
const CFGR2_PPRE1: u32 = 0b111 << 4;
const CFGR2_PPRE2: u32 = 0b111 << 8;
const AHB2ENR1_GPIOAEN: u32 = 1 << 0;
const APB2ENR_USART1EN: u32 = 1 << 14;
/// CCIPR1: USART1SEL [1:0] (00 PCLK2, 01 SYSCLK, 10 HSI16, 11 LSE).
const CCIPR1_USART1SEL: u32 = 0b11;

const FLASH_ACR: *mut u32 = 0x4002_2000 as *mut u32;
const ACR_LATENCY: u32 = 0b1111;
/// 16 MHz in VCORE range 4 (the reset range): 1 wait state -- the
/// range-4 column of RM0456's wait-state table tops 0 WS at 12 MHz.
const ACR_LATENCY_1WS: u32 = 1;

#[cfg(feature = "nucleo-u5a5zj-q-pll160")]
mod pll160 {
    /// 160 MHz in VCORE range 1: 4 wait states (RM0456 Table 54, the
    /// range-1 column: 3 WS tops at 128 MHz, 4 WS at 160).
    pub const ACR_LATENCY_4WS: u32 = 4;

    pub const RCC_PLL1CFGR: *mut u32 = (super::RCC + 0x28) as *mut u32;
    pub const RCC_PLL1DIVR: *mut u32 = (super::RCC + 0x34) as *mut u32;
    /// PWR sits on AHB3 on this family (not APB1 as on the F7); its bus
    /// clock must be enabled -- and read back -- before any PWR register
    /// is touched: a read of an unclocked peripheral returns zeros, so a
    /// VOSRDY poll without PWREN spins forever.
    pub const RCC_AHB3ENR: *mut u32 = (super::RCC + 0x94) as *mut u32;
    pub const AHB3ENR_PWREN: u32 = 1 << 2;

    pub const CR_PLL1ON: u32 = 1 << 24;
    pub const CR_PLL1RDY: u32 = 1 << 25;
    pub const CFGR1_SW_PLL1: u32 = 0b11;
    pub const CFGR1_SWS_PLL1: u32 = 0b11 << 2;
    /// HPRE = 0b1000: AHB /2 -- the intermediate stage of this part's
    /// mandated two-step switch (the STM32U59x/5Ax SYSCLK step in
    /// range 1 may not exceed 80 MHz).
    pub const CFGR2_HPRE_DIV2: u32 = 0b1000;

    /// PLL1CFGR shape while the PLL is off: EPOD booster prescaler /2
    /// (16 MHz HSI16 -> 8 MHz, inside the 4-16 MHz booster window,
    /// clear of both edges), PLL1M /1 (VCO input 16 MHz), input range
    /// row 8-16 MHz, source HSI16.
    pub const PLL1CFGR_MBOOST_DIV2: u32 = 0b0001 << 12;
    pub const PLL1CFGR_RGE_8_16: u32 = 0b11 << 2;
    pub const PLL1CFGR_SRC_HSI16: u32 = 0b10;
    pub const PLL1CFGR_REN: u32 = 1 << 18;

    /// PLL1DIVR: every field holds its divider MINUS ONE. N = 20
    /// (VCO 16 x 20 = 320 MHz), R = 2 (SYSCLK 160); P and Q keep their
    /// reset /2 (outputs disabled -- unpulled levers, stated).
    pub const PLL1DIVR_VALUE: u32 = (1 << 24) | (1 << 16) | (1 << 9) | 19;

    pub const PWR_VOSR: *mut u32 = 0x4602_080C as *mut u32;
    /// VOS[17:16] = 0b11: range 1 (00 is range 4, the reset).
    pub const VOSR_VOS_RANGE1: u32 = 0b11 << 16;
    pub const VOSR_VOSRDY: u32 = 1 << 15;
    pub const VOSR_BOOSTEN: u32 = 1 << 18;
    pub const VOSR_BOOSTRDY: u32 = 1 << 14;
}

const GPIOA: usize = 0x4202_0000;
const GPIOA_MODER: *mut u32 = GPIOA as *mut u32;
const GPIOA_AFRH: *mut u32 = (GPIOA + 0x24) as *mut u32;

const USART1: usize = 0x4001_3800;
const USART1_CR1: *mut u32 = USART1 as *mut u32;
const USART1_BRR: *mut u32 = (USART1 + 0x0C) as *mut u32;
const USART1_ISR: *mut u32 = (USART1 + 0x1C) as *mut u32;
const USART1_ICR: *mut u32 = (USART1 + 0x20) as *mut u32;
const USART1_RDR: *mut u32 = (USART1 + 0x24) as *mut u32;
const USART1_TDR: *mut u32 = (USART1 + 0x28) as *mut u32;

const CR1_UE: u32 = 1 << 0;
const CR1_RE: u32 = 1 << 2;
const CR1_TE: u32 = 1 << 3;
/// ISR bit 7 reads TXE while the FIFO is disabled (CR1.FIFOEN = 0, the
/// reset state this driver keeps); bit 5 likewise reads RXNE.
const ISR_TXE: u32 = 1 << 7;
const ISR_RXNE: u32 = 1 << 5;
const ISR_ORE: u32 = 1 << 3;
const ICR_ORECF: u32 = 1 << 3;
const BAUD: u32 = 115_200;

/// Enforce the HSI16 posture. Entry assumes nothing: out of reset the
/// part runs on MSIS at 0 wait states, and a probe or resident image
/// may have left any clock state behind. Never fast with low WS, in
/// both directions: latency RISES to the 16 MHz row before the switch
/// when the current setting is below it (the reset path), and DROPS to
/// it only after the switch when above it (a resident's high-clock
/// posture). HSI16 is turned on and waited for, prescalers are forced
/// to div1, SYSCLK switches and the switch is polled. MSIS, the PLLs
/// and the VCORE range are left as found -- off the HSI16 path, so
/// levers this posture does not need stay unpulled.
#[cfg(not(feature = "nucleo-u5a5zj-q-pll160"))]
pub fn clock_init() {
    unsafe {
        let acr = read_volatile(FLASH_ACR);
        if acr & ACR_LATENCY < ACR_LATENCY_1WS {
            write_volatile(FLASH_ACR, (acr & !ACR_LATENCY) | ACR_LATENCY_1WS);
            while read_volatile(FLASH_ACR) & ACR_LATENCY != ACR_LATENCY_1WS {}
        }

        write_volatile(RCC_CR, read_volatile(RCC_CR) | CR_HSION);
        while read_volatile(RCC_CR) & CR_HSIRDY == 0 {}

        let cfgr2 = read_volatile(RCC_CFGR2) & !(CFGR2_HPRE | CFGR2_PPRE1 | CFGR2_PPRE2);
        write_volatile(RCC_CFGR2, cfgr2);
        let cfgr1 = read_volatile(RCC_CFGR1) & !CFGR1_SW;
        write_volatile(RCC_CFGR1, cfgr1 | CFGR1_SW_HSI16);
        while read_volatile(RCC_CFGR1) & CFGR1_SWS != CFGR1_SWS_HSI16 {}

        let acr = read_volatile(FLASH_ACR) & !ACR_LATENCY;
        write_volatile(FLASH_ACR, acr | ACR_LATENCY_1WS);
        while read_volatile(FLASH_ACR) & ACR_LATENCY != ACR_LATENCY_1WS {}
    }
}

/// Boring posture: no clock line in the banner (the banked transcript
/// surface stands byte-identical).
#[cfg(not(feature = "nucleo-u5a5zj-q-pll160"))]
pub fn clock_readback() -> Option<crate::board::ClockReadback> {
    None
}

/// Enforce the PLL160 posture, every step on its hardware ready flag
/// (RM0456 section 10's range L-to-P sequence, with this part's own
/// two-step switch law embedded where the RM puts it). Entry assumes
/// nothing: first the HSI16 downshift -- latency raised to the 16 MHz
/// row BEFORE the switch when below it (never fast with low WS), HSI16
/// on and waited for, prescalers div1, switch polled -- then PLL1 is
/// stopped so its configuration may be written. The PWR bus clock is
/// enabled on AHB3 and READ BACK before any PWR register is touched
/// (an unclocked peripheral reads as zeros -- a VOSRDY poll without
/// PWREN spins forever). VOS range 1 and the EPOD boost (required
/// above 55 MHz) are programmed together and both ready flags waited
/// out while SYSCLK is still HSI16; flash latency then rises to the
/// 160 MHz row (4 WS; SRAM wait states need NO write -- RAMCFG WSC's
/// 0 WS row holds to 160 MHz in range 1 per RM0456 Table 47, an
/// unpulled lever); and the switch runs this part's mandated two-step
/// shape: on STM32U59x/5Ax the SYSCLK step in range 1 may not exceed
/// 80 MHz, so HPRE goes /2 first, the PLL locks, SYSCLK switches (HCLK
/// lands at 80 MHz -- a 64 MHz step), a >= 5 us hold elapses, and HPRE
/// releases to /1 (the exactly-80 MHz second step). Caches stay off;
/// MSIS is left as found.
#[cfg(feature = "nucleo-u5a5zj-q-pll160")]
pub fn clock_init() {
    use pll160::*;
    unsafe {
        let acr = read_volatile(FLASH_ACR);
        if acr & ACR_LATENCY < ACR_LATENCY_1WS {
            write_volatile(FLASH_ACR, (acr & !ACR_LATENCY) | ACR_LATENCY_1WS);
            while read_volatile(FLASH_ACR) & ACR_LATENCY != ACR_LATENCY_1WS {}
        }
        write_volatile(RCC_CR, read_volatile(RCC_CR) | CR_HSION);
        while read_volatile(RCC_CR) & CR_HSIRDY == 0 {}
        let cfgr2 = read_volatile(RCC_CFGR2) & !(CFGR2_HPRE | CFGR2_PPRE1 | CFGR2_PPRE2);
        write_volatile(RCC_CFGR2, cfgr2);
        let cfgr1 = read_volatile(RCC_CFGR1) & !CFGR1_SW;
        write_volatile(RCC_CFGR1, cfgr1 | CFGR1_SW_HSI16);
        while read_volatile(RCC_CFGR1) & CFGR1_SWS != CFGR1_SWS_HSI16 {}

        write_volatile(RCC_CR, read_volatile(RCC_CR) & !CR_PLL1ON);
        while read_volatile(RCC_CR) & CR_PLL1RDY != 0 {}

        write_volatile(RCC_AHB3ENR, read_volatile(RCC_AHB3ENR) | AHB3ENR_PWREN);
        let _ = read_volatile(RCC_AHB3ENR);

        write_volatile(
            RCC_PLL1CFGR,
            PLL1CFGR_MBOOST_DIV2 | PLL1CFGR_RGE_8_16 | PLL1CFGR_SRC_HSI16,
        );

        write_volatile(
            PWR_VOSR,
            read_volatile(PWR_VOSR) | VOSR_VOS_RANGE1 | VOSR_BOOSTEN,
        );
        while read_volatile(PWR_VOSR) & VOSR_VOSRDY == 0 {}
        while read_volatile(PWR_VOSR) & VOSR_BOOSTRDY == 0 {}

        let acr = read_volatile(FLASH_ACR) & !ACR_LATENCY;
        write_volatile(FLASH_ACR, acr | ACR_LATENCY_4WS);
        while read_volatile(FLASH_ACR) & ACR_LATENCY != ACR_LATENCY_4WS {}

        let cfgr2 = read_volatile(RCC_CFGR2) & !CFGR2_HPRE;
        write_volatile(RCC_CFGR2, cfgr2 | CFGR2_HPRE_DIV2);

        write_volatile(RCC_PLL1DIVR, PLL1DIVR_VALUE);
        write_volatile(RCC_PLL1CFGR, read_volatile(RCC_PLL1CFGR) | PLL1CFGR_REN);
        write_volatile(RCC_CR, read_volatile(RCC_CR) | CR_PLL1ON);
        while read_volatile(RCC_CR) & CR_PLL1RDY == 0 {}

        let cfgr1 = read_volatile(RCC_CFGR1) & !CFGR1_SW;
        write_volatile(RCC_CFGR1, cfgr1 | CFGR1_SW_PLL1);
        while read_volatile(RCC_CFGR1) & CFGR1_SWS != CFGR1_SWS_PLL1 {}
        for _ in 0..1000 {
            core::arch::asm!("nop", options(nomem, nostack));
        }
        let cfgr2 = read_volatile(RCC_CFGR2) & !CFGR2_HPRE;
        write_volatile(RCC_CFGR2, cfgr2);
    }
}

/// Gated posture: report the live clock tree for the banner line, every
/// numeric field decoded from registers (a silently-failed PLL yields
/// SWS=HSI16 at 16 MHz -- or MSIS at 4 -- here, and the kmain assert
/// dies loudly). Field mapping per the registered ruling: `pllp`
/// carries PLL1 R (this family's divider that lands SYSCLK, as P is the
/// F7's) and `overdrive` carries the EPOD boost state (BOOSTRDY) --
/// each family's go-fast handshake under the shared banner wording.
#[cfg(feature = "nucleo-u5a5zj-q-pll160")]
pub fn clock_readback() -> Option<crate::board::ClockReadback> {
    use pll160::*;
    /// Nominal MSIS out of reset -- decode-only: this posture leaves
    /// MSIS as found and only ever reads the selector bits.
    const MSIS_HZ: u32 = 4_000_000;
    const HSI16_HZ: u32 = 16_000_000;
    unsafe {
        let cfgr1 = read_volatile(RCC_CFGR1);
        let pllcfgr = read_volatile(RCC_PLL1CFGR);
        let plldivr = read_volatile(RCC_PLL1DIVR);
        let m = ((pllcfgr >> 8) & 0xF) + 1;
        let n = (plldivr & 0x1FF) + 1;
        let r = ((plldivr >> 24) & 0x7F) + 1;
        let (src, base) = match pllcfgr & 0b11 {
            0b01 => ("MSIS", MSIS_HZ),
            0b10 => ("HSI16", HSI16_HZ),
            0b11 => ("HSE", 0),
            _ => ("none", 0),
        };
        let (sws, sysclk_hz) = match (cfgr1 >> 2) & 0b11 {
            0b00 => ("MSIS", MSIS_HZ),
            0b01 => ("HSI16", HSI16_HZ),
            0b10 => ("HSE", 0),
            _ => ("PLL1", base / m * n / r),
        };
        Some(crate::board::ClockReadback {
            posture: "PLL160",
            sysclk_hz,
            sws,
            src,
            pllm: m,
            plln: n,
            pllp: r,
            flash_ws: read_volatile(FLASH_ACR) & ACR_LATENCY,
            overdrive: read_volatile(PWR_VOSR) & VOSR_BOOSTRDY != 0,
        })
    }
}

/// Route PA9/PA10 to USART1 (AF7 -- the ST-LINK VCP wiring), pin the
/// USART1 kernel clock to PCLK2 (its reset selection, stated
/// explicitly), and program 115200 8N1 from PCLK2 with the FIFO
/// disabled. Idempotent by construction (every write lands the same
/// values); the UE-low reprogram window can tear at most one in-flight
/// character when the panic path re-runs it; output checks anchor on
/// the panic banner that follows, so a single torn character is
/// harmless.
pub fn console_init() {
    unsafe {
        write_volatile(RCC_AHB2ENR1, read_volatile(RCC_AHB2ENR1) | AHB2ENR1_GPIOAEN);
        let _ = read_volatile(RCC_AHB2ENR1);
        write_volatile(RCC_APB2ENR, read_volatile(RCC_APB2ENR) | APB2ENR_USART1EN);
        let _ = read_volatile(RCC_APB2ENR);
        write_volatile(RCC_CCIPR1, read_volatile(RCC_CCIPR1) & !CCIPR1_USART1SEL);

        let moder = read_volatile(GPIOA_MODER) & !((0b11 << 18) | (0b11 << 20));
        write_volatile(GPIOA_MODER, moder | (0b10 << 18) | (0b10 << 20));
        let afrh = read_volatile(GPIOA_AFRH) & !((0xF << 4) | (0xF << 8));
        write_volatile(GPIOA_AFRH, afrh | (0x7 << 4) | (0x7 << 8));

        write_volatile(USART1_CR1, 0);
        write_volatile(USART1_BRR, (PCLK2_HZ + BAUD / 2) / BAUD);
        write_volatile(USART1_CR1, CR1_UE | CR1_TE | CR1_RE);
    }
}

/// Blocking byte transmit (polled).
pub fn console_putb(b: u8) {
    unsafe {
        while read_volatile(USART1_ISR) & ISR_TXE == 0 {}
        write_volatile(USART1_TDR, b as u32);
    }
}

/// Non-blocking byte receive: one pending byte, or None immediately.
/// A latched overrun (missed polls) is cleared on the empty path so
/// the receiver never wedges; the lost bytes stay lost, honestly.
pub fn console_getb() -> Option<u8> {
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
