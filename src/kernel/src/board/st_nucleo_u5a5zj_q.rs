//! NUCLEO-U5A5ZJ-Q (STM32U5A5ZJ, Cortex-M33): the ARMv8-M silicon
//! board. Flash-boot at 0x08000000, USART1 console on the ST-LINK VCP.

use core::ptr::{read_volatile, write_volatile};

use super::ring;

pub const NAME: &str = "NUCLEO-U5A5ZJ-Q";
pub const CONSOLE: &str = "USART1 (ST-LINK VCP)";

/// Clock posture 1 (default): HSI16, SYSCLK = HCLK = PCLK1 = PCLK2 =
/// 16 MHz (all prescalers div1). `clock_init` enforces exactly this
/// posture.
#[cfg(not(feature = "st-nucleo-u5a5zj-q-pll160"))]
pub const SYSCLK_HZ: u32 = 16_000_000;
/// Clock posture 2 (`st-nucleo-u5a5zj-q-pll160`): PLL1 at 160 MHz off
/// HSI16 -- 16 MHz /1 into the VCO, x20 = 320 MHz (inside the 128-544
/// MHz VCO window), /2 on the R output lands SYSCLK at 160 MHz, the
/// VCORE range-1 posture. HCLK = PCLK1 = PCLK2 = SYSCLK (div1: on this
/// part every bus runs to 160 MHz in range 1, so no cap prescalers).
#[cfg(feature = "st-nucleo-u5a5zj-q-pll160")]
pub const SYSCLK_HZ: u32 = 160_000_000;
/// USART1's kernel clock is PCLK2 (RCC_CCIPR1.USART1SEL = 00, stated
/// explicitly by `console_init`), and PCLK2 = SYSCLK under div1.
const PCLK2_HZ: u32 = SYSCLK_HZ;

/// SysTick rate.
pub const TICK_HZ: u32 = 1_000;

/// Thread stack pool (lives in .kstacks): 256 KiB on this row's 2496 K of
/// internal SRAM -- the idle thread's 1 KiB, the hosting thread's and the
/// finalizer's 16 KiB each, and thirteen default 16 KiB workers; the link
/// asserts the arena window that remains above the row's floor.
pub const STACK_POOL_BYTES: usize = 256 * 1024;

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

/// The vector line a test image may claim: RM0456's position 138 (DCACHE2),
/// the last line this part implements -- the two reserved positions above
/// it take no priority write, so a control pended there would never fire;
/// its peripheral is never clocked by the kernel, the line claimed by no
/// kernel handler.
#[allow(dead_code)]
pub const SPARE_IRQ: usize = 138;

/// FPU shape: FPv5 SINGLE precision (the Cortex-M33 FPU) -- kernel
/// f64 stays on the soft-float path. Consumed by records and gates,
/// never by context-switch code (lazy stacking is FP-shape-agnostic).
/// The part: the STM32U5A5's Cortex-M33 with a single-precision FPU (its
/// datasheet: "a single-precision FPU").
#[allow(dead_code)]
pub const FPU_DP: bool = false;
/// The cycle counter the lost-tick witness reads: the core's data
/// watchpoint and trace unit counts processor cycles, enabled and
/// verified to advance at the scheduler's init.
pub const CYCLE_COUNTER: bool = true;

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

#[cfg(feature = "st-nucleo-u5a5zj-q-pll160")]
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

/// The transmit-empty interrupt's enable (USART_CR1 TXEIE, bit 7): the
/// level-shaped interrupt the ring's drain runs on -- asserted whenever
/// the transmit data register is empty, so the ring module sets it only
/// while a backlog exists and clears it when the ring empties.
const CR1_TXEIE: u32 = 1 << 7;

/// USART1's interrupt line (RM0456's vector table, position 61), its
/// enable bit in the interrupt controller's second set-enable and
/// clear-pending registers, and its priority byte in the priority array.
pub const USART1_IRQ: usize = 61;
const NVIC_ISER1: *mut u32 = 0xE000_E104 as *mut u32;
const NVIC_ICPR1: *mut u32 = 0xE000_E284 as *mut u32;
const NVIC_IPR: *mut u8 = 0xE000_E400 as *mut u8;
/// The lowest band: masked inside a kernel critical section (the ceiling
/// 0x40), below the tick (0x80), above the context switch (0xF0) -- the
/// drain preempts thread code and nothing else, and the writer's section
/// is its lock.
const USART1_IRQ_PRIO: u8 = 0xE0;

/// The interrupt lines this board claims with its own handlers, folded
/// into the vector table by the boot module: USART1's line, the console
/// ring's transmit drain.
pub const BOARD_IRQS: &[(usize, unsafe extern "C" fn())] = &[(USART1_IRQ, __rhu_u5a5_usart1_irq)];
/// The family GPIO driver's external-interrupt lines (the U5 arm).
pub use super::stm32_gpio::IRQS as GPIO_IRQS;

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
#[cfg(not(feature = "st-nucleo-u5a5zj-q-pll160"))]
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
    ring::boot_mark();
}

/// Boring posture: no clock line in the banner (the banked transcript
/// surface stands byte-identical).
#[cfg(not(feature = "st-nucleo-u5a5zj-q-pll160"))]
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
#[cfg(feature = "st-nucleo-u5a5zj-q-pll160")]
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
    ring::boot_mark();
}

/// Gated posture: report the live clock tree for the banner line, every
/// numeric field decoded from registers (a silently-failed PLL yields
/// SWS=HSI16 at 16 MHz -- or MSIS at 4 -- here, and the kmain assert
/// dies loudly). Field mapping per the registered ruling: `pllp`
/// carries PLL1 R (this family's divider that lands SYSCLK, as P is the
/// F7's) and `boost` states the EPOD booster's state (BOOSTRDY) in the
/// reference manual's own words: each family's go-fast handshake by name.
#[cfg(feature = "st-nucleo-u5a5zj-q-pll160")]
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
            boost: if read_volatile(PWR_VOSR) & VOSR_BOOSTRDY != 0 { "EPOD boost on" } else { "EPOD boost off" },
        })
    }
}

/// Route PA9/PA10 to USART1 (AF7 -- the ST-LINK VCP wiring), pin the
/// USART1 kernel clock to PCLK2 (its reset selection, stated
/// explicitly), and program 115200 8N1 from PCLK2 with the FIFO
/// disabled. Runs once, at the first console init (a panic before the
/// boot's own init reaches it through the same path), so the UE-low
/// reprogram window that could tear an in-flight character is never
/// re-entered.
unsafe fn console_hw_init() {
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

/// The console driver's init. The first call configures the hardware.
/// The call the boot path makes (marked by `clock_init` through the ring
/// module) arms the transmit interrupt -- its priority byte, its enable
/// bit in the interrupt controller -- and takes the interrupt mode; any
/// later call is the panic or fault path's re-init and switches the
/// driver to the polled mode with the transmit-empty interrupt disabled,
/// so the report that follows goes through the wire synchronously. The
/// console mirror is not touched here (see `clock_init`), so a panic
/// transcript appends to the boot's.
pub fn console_init() {
    unsafe {
        if ring::mode() == 0 {
            console_hw_init();
        }
        if ring::take_boot_mark() {
            write_volatile(NVIC_IPR.add(USART1_IRQ), USART1_IRQ_PRIO);
            write_volatile(NVIC_ICPR1, 1 << (USART1_IRQ - 32));
            write_volatile(NVIC_ISER1, 1 << (USART1_IRQ - 32));
            ring::set_mode(ring::MODE_IRQ);
        } else {
            ring_tx_irq(false);
            ring::set_mode(ring::MODE_POLLED);
        }
    }
}

/// The ring's four hooks on this USART: the transmit data register takes
/// a byte while its empty flag is set; a byte goes into the transmit data
/// register (which clears the flag until the byte moves on to the shift
/// register); nothing latches, so the acknowledge is empty; and the
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
pub unsafe extern "C" fn __rhu_u5a5_usart1_irq() {
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

/// Whether address 0 is readable memory on this row -- the low-address
/// question's per-row record, MEASURED on the part (a read that returns
/// a word, or a bus fault): the api flavor's observation reads it before
/// it touches the address.
/// This row: a precise bus fault, BFAR 0, measured on the board: no memory answers at 0 with the boot address the option bytes hold.
#[allow(dead_code)]
pub const ADDRESS_ZERO_READABLE: bool = false;


/// The RNG block (RM0456 chapter 48): a true random number generator
/// with an analog noise source, a NIST-style conditioning stage and
/// startup and continuous health tests in hardware. Its kernel clock is
/// HSI48 (RCC_CCIPR2.RNGSEL = 00), on at both clock postures of this
/// board; with the clock checker enabled (CED = 0) the block flags a
/// clock error whenever that clock runs below the AHB clock over 32,
/// which neither posture approaches. The configuration written is the
/// manual's table's configuration C, in full: its certified
/// configuration A is stated there only by reference to the vendor's
/// application note, and the constant pair below is the one place it would move.
const RNG: usize = 0x420C_0800;
const RNG_CR: *mut u32 = RNG as *mut u32;
const RNG_SR: *mut u32 = (RNG + 0x04) as *mut u32;
const RNG_DR: *mut u32 = (RNG + 0x08) as *mut u32;
const RNG_HTCR: *mut u32 = (RNG + 0x10) as *mut u32;
/// RNG_CR: CONDRST bit 30 (the conditioning soft reset), RNG_CONFIG1
/// bits 25:20, CLKDIV bits 19:16 (0: the 48 MHz clock undivided),
/// RNG_CONFIG2 bits 15:13, NISTC bit 12, RNG_CONFIG3 bits 11:8, ARDIS
/// bit 7 (0: the block auto-resets its conditioning after a seed error),
/// CED bit 5 (0: clock error detection ON), IE bit 3, RNGEN bit 2.
const RNG_CR_CONDRST: u32 = 1 << 30;
const RNG_CR_CONFIG_C: u32 = (0x0F << 20) | (0xD << 8);
const RNG_CR_RNGEN: u32 = 1 << 2;
/// RNG_HTCR's health-test thresholds for configuration C; taken into
/// account only while CONDRST is set.
const RNG_HTCR_CONFIG_C: u32 = 0x0000_AAC7;
/// RNG_SR: SEIS bit 6 and CEIS bit 5 (the error interrupt flags, cleared
/// by writing 0), SECS bit 2 and CECS bit 1 (the current status), DRDY
/// bit 0.
const RNG_SR_SEIS: u32 = 1 << 6;
const RNG_SR_CEIS: u32 = 1 << 5;
const RNG_SR_SECS: u32 = 1 << 2;
const RNG_SR_CECS: u32 = 1 << 1;
const RNG_SR_DRDY: u32 = 1 << 0;
const RNG_SR_ERRORS: u32 = RNG_SR_SEIS | RNG_SR_CEIS | RNG_SR_SECS | RNG_SR_CECS;

const CR_HSI48ON: u32 = 1 << 12;
const CR_HSI48RDY: u32 = 1 << 13;
const RCC_CCIPR2: *mut u32 = (RCC + 0xE4) as *mut u32;
/// CCIPR2.RNGSEL [13:12]: 00 HSI48.
const CCIPR2_RNGSEL: u32 = 0b11 << 12;
const AHB2ENR1_RNGEN: u32 = 1 << 18;

/// A bounded wait's iteration count for about two milliseconds: the
/// loop body is at least four cycles, so the count is the cycles of two
/// milliseconds over four -- at the boot the tick clock is not yet
/// running, so every wait here is an iteration bound, never a clock.
const SPIN_2MS: u32 = SYSCLK_HZ / 500 / 4;

/// The entropy posture of this row: the RNG block served on both clock
/// postures, its self-tests read at every draw.
pub const ENTROPY: super::EntropyPosture = super::EntropyPosture::Source {
    id: 2,
    name: "STM32U5 RNG",
    line: "hardware source STM32U5 RNG enabled on HSI48; its self-tests gate every draw",
};

/// The block started: HSI48 on and ready, the kernel clock selected, the
/// bus clock enabled, the configuration written under the conditioning
/// reset, the block enabled, the reset's clearance and the first ready
/// word waited for under the bound, the status read. False when the
/// startup tests failed or the clock checker refused (the witness then
/// reads the source absent and every secure draw refuses).
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
        write_volatile(RCC_CCIPR2, read_volatile(RCC_CCIPR2) & !CCIPR2_RNGSEL);
        write_volatile(RCC_AHB2ENR1, read_volatile(RCC_AHB2ENR1) | AHB2ENR1_RNGEN);
        let _ = read_volatile(RCC_AHB2ENR1);
        write_volatile(RNG_CR, RNG_CR_CONDRST | RNG_CR_CONFIG_C);
        write_volatile(RNG_HTCR, RNG_HTCR_CONFIG_C);
        write_volatile(RNG_CR, RNG_CR_CONFIG_C | RNG_CR_RNGEN);
        n = 0;
        while read_volatile(RNG_CR) & RNG_CR_CONDRST != 0 {
            n += 1;
            if n > SPIN_2MS {
                return false;
            }
        }
        n = 0;
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
/// polled in the caller's thread until a word is ready or a flag is set,
/// under the bound -- then, under the kernel critical section, the status
/// read again and, still ready and clean, the data word read: an error
/// flag is the health check failing at this draw (counted; the policy
/// retires the source), a zero word the raced seed error the manual names,
/// treated the same. The section covers two or three bus reads and never
/// a wait: an interrupt at or above the ceiling is delayed by those reads
/// alone, the zero-latency band below it never. The bytes land in the
/// caller's buffer word by word; the policy layer copies out only when
/// the whole draw succeeded.
pub fn entropy_draw(buf: &mut [u8], counters: &mut super::DrawCounters) -> Result<(), super::EntropyErr> {
    for chunk in buf.chunks_mut(4) {
        let mut n = 0u32;
        let word = loop {
            let sr = unsafe { read_volatile(RNG_SR) };
            if sr & RNG_SR_ERRORS != 0 {
                break Err(super::EntropyErr::Health);
            }
            if sr & RNG_SR_DRDY != 0 {
                let got = crate::critical::with_cs(|| unsafe {
                    let sr = read_volatile(RNG_SR);
                    if sr & RNG_SR_ERRORS != 0 {
                        return Some(Err(super::EntropyErr::Health));
                    }
                    if sr & RNG_SR_DRDY == 0 {
                        return None;
                    }
                    let w = read_volatile(RNG_DR);
                    if w == 0 {
                        return Some(Err(super::EntropyErr::Health));
                    }
                    Some(Ok(w))
                });
                if let Some(r) = got {
                    break r;
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

/// The real-time clock (RM0456 chapter 63) in the backup domain, clocked
/// by the board's 32.768 kHz crystal through the low-speed external
/// oscillator (the Nucleo-144 board's default configuration). The
/// domain's write protection is released through PWR (its bus clock first),
/// the clock's registers are reached through their APB clock, and the
/// clock's own key sequence opens its registers for a write. The
/// prescalers keep their domain-reset values, which are exactly the
/// 32.768 kHz configuration (a 1 Hz calendar).
const PWR_DBPR: *mut u32 = 0x4602_0828 as *mut u32;
const DBPR_DBP: u32 = 1 << 0;
const RCC_AHB3ENR_RTC: *mut u32 = (RCC + 0x94) as *mut u32;
const AHB3ENR_PWREN_RTC: u32 = 1 << 2;
const RCC_APB3ENR: *mut u32 = (RCC + 0xA8) as *mut u32;
const APB3ENR_RTCAPBEN: u32 = 1 << 21;
const RCC_BDCR: *mut u32 = (RCC + 0xF0) as *mut u32;
const BDCR_LSEON: u32 = 1 << 0;
const BDCR_LSERDY: u32 = 1 << 1;
const BDCR_RTCSEL: u32 = 0b11 << 8;
const BDCR_RTCSEL_LSE: u32 = 0b01 << 8;
const BDCR_RTCEN: u32 = 1 << 15;
const RTC: usize = 0x4600_7800;
const RTC_TR: *mut u32 = RTC as *mut u32;
const RTC_DR: *mut u32 = (RTC + 0x04) as *mut u32;
const RTC_ICSR: *mut u32 = (RTC + 0x0C) as *mut u32;
const RTC_WPR: *mut u32 = (RTC + 0x24) as *mut u32;
/// RTC_ICSR: INIT bit 7 (the initialization mode), INITF bit 6 (the mode
/// entered), RSF bit 5 (the shadow registers synchronized; cleared by
/// writing 0), INITS bit 4 (the calendar was ever initialized: the year
/// field is not the domain-reset zero).
const ICSR_INIT: u32 = 1 << 7;
const ICSR_INITF: u32 = 1 << 6;
const ICSR_RSF: u32 = 1 << 5;
const ICSR_INITS: u32 = 1 << 4;
const WPR_KEY1: u32 = 0xCA;
const WPR_KEY2: u32 = 0x53;

/// Whether this row carries a real-time clock the kernel reads at boot and
/// writes at an explicit set.
#[allow(dead_code)]
pub const RTC_PRESENT: bool = true;
/// The calendar banner's text for this row, constant per image.
pub const RTC_LINE: &str = "RTC block present, 32.768 kHz crystal mounted, no backup cell: the base carries across a reset, not a power cycle";

/// The backup domain reachable: PWR's bus clock, the clock's APB clock,
/// the write protection released.
unsafe fn rtc_reach() {
    unsafe {
        write_volatile(RCC_AHB3ENR_RTC, read_volatile(RCC_AHB3ENR_RTC) | AHB3ENR_PWREN_RTC);
        let _ = read_volatile(RCC_AHB3ENR_RTC);
        write_volatile(RCC_APB3ENR, read_volatile(RCC_APB3ENR) | APB3ENR_RTCAPBEN);
        let _ = read_volatile(RCC_APB3ENR);
        write_volatile(PWR_DBPR, read_volatile(PWR_DBPR) | DBPR_DBP);
        let _ = read_volatile(PWR_DBPR);
    }
}

/// The boot read: a running clock that was ever set is read once (the
/// shadow registers resynchronized first, the time word before the date
/// word so the pair is consistent); a running clock never set reports
/// so; a cold domain gets its oscillator started and reports cold (the
/// boot never waits for a crystal).
pub fn rtc_read_at_boot() -> super::RtcBoot {
    unsafe {
        rtc_reach();
        let bdcr = read_volatile(RCC_BDCR);
        let running = bdcr & BDCR_RTCEN != 0 && bdcr & BDCR_RTCSEL == BDCR_RTCSEL_LSE && bdcr & BDCR_LSERDY != 0;
        if !running {
            if bdcr & BDCR_LSEON == 0 {
                write_volatile(RCC_BDCR, bdcr | BDCR_LSEON);
            }
            return super::RtcBoot::Cold;
        }
        if read_volatile(RTC_ICSR) & ICSR_INITS == 0 {
            return super::RtcBoot::Uninitialized;
        }
        write_volatile(RTC_ICSR, read_volatile(RTC_ICSR) & !ICSR_RSF);
        let mut n = 0u32;
        while read_volatile(RTC_ICSR) & ICSR_RSF == 0 {
            n += 1;
            if n > SPIN_2MS {
                return super::RtcBoot::Uninitialized;
            }
        }
        let tr = read_volatile(RTC_TR);
        let dr = read_volatile(RTC_DR);
        super::RtcBoot::Read(crate::calendar_math::st_unpack(tr, dr))
    }
}

/// The write-back of an explicit set: skipped while the crystal is not
/// yet stable and for an instant outside the clock's century; otherwise
/// the clock is selected and enabled on a cold domain, its registers
/// unlocked, the initialization mode entered and waited for, the time
/// and date words written, the mode left (the counting restarts within
/// four clock cycles) and the registers locked again.
pub fn rtc_write(ticks: u64) -> super::RtcWrite {
    unsafe {
        rtc_reach();
        let bdcr = read_volatile(RCC_BDCR);
        if bdcr & BDCR_LSERDY == 0 {
            return super::RtcWrite::SkippedLseNotReady;
        }
        let Some((tr, dr)) = crate::calendar_math::st_pack(ticks) else {
            return super::RtcWrite::SkippedRange;
        };
        if bdcr & BDCR_RTCEN == 0 {
            write_volatile(RCC_BDCR, (bdcr & !BDCR_RTCSEL) | BDCR_RTCSEL_LSE | BDCR_RTCEN);
        }
        if read_volatile(RCC_BDCR) & BDCR_RTCSEL != BDCR_RTCSEL_LSE {
            return super::RtcWrite::SkippedSourceHeld;
        }
        write_volatile(RTC_WPR, WPR_KEY1);
        write_volatile(RTC_WPR, WPR_KEY2);
        write_volatile(RTC_ICSR, read_volatile(RTC_ICSR) | ICSR_INIT);
        let mut n = 0u32;
        while read_volatile(RTC_ICSR) & ICSR_INITF == 0 {
            n += 1;
            if n > SPIN_2MS {
                write_volatile(RTC_ICSR, read_volatile(RTC_ICSR) & !ICSR_INIT);
                write_volatile(RTC_WPR, 0xFF);
                return super::RtcWrite::SkippedLseNotReady;
            }
        }
        write_volatile(RTC_TR, tr);
        write_volatile(RTC_DR, dr);
        write_volatile(RTC_ICSR, read_volatile(RTC_ICSR) & !ICSR_INIT);
        write_volatile(RTC_WPR, 0xFF);
        super::RtcWrite::Written
    }
}
