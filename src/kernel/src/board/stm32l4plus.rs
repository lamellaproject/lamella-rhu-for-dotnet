//! The STM32L4+ family module: the boot posture, the two clock postures,
//! the console on LPUART1 driven the ring way, the random source and the
//! calendar facts every board of the family shares. Each board file
//! states its own facts -- the name, the console's pins, the crystal, the
//! interrupt count, the RAM windows, the stack pool, the kheap carve --
//! and re-exports the rest from here (the STM32F7 family module's pattern).

use core::ptr::{read_volatile, write_volatile};

use super::ring;

/// Clock posture 1 (default): HSI16, SYSCLK = HCLK = PCLK1 = PCLK2 (all
/// prescalers div1). `clock_init` enforces exactly this posture.
#[cfg(not(stm32l4plus_pll120))]
pub const SYSCLK_HZ: u32 = 16_000_000;

/// Clock posture 2 (the board's `-pll120` feature): the main PLL at 120
/// MHz off HSI16 -- HSI16/2 gives an 8 MHz VCO input (the top of the
/// input window), x30 puts the VCO at 240 MHz (inside its 64 to 344 MHz
/// window), /2 lands SYSCLK at 120, the part's ceiling, which the main
/// regulator's range-1 boost mode allows. HCLK = SYSCLK; both APB domains
/// run at SYSCLK (their ceiling is the same 120 MHz). The register recipe
/// and the boost sequence per the reference manual.
#[cfg(stm32l4plus_pll120)]
pub const SYSCLK_HZ: u32 = 120_000_000;

/// The LPUART's kernel clock: HSI16 on every posture (the clock select in
/// `console_hw_init`), so the baud divisor is one number.
const LPUART_CK_HZ: u32 = 16_000_000;

/// SysTick rate.
pub const TICK_HZ: u32 = 1_000;

/// Stack for the kmain thread (the boot thread's continuation).
pub const KMAIN_STACK_BYTES: usize = 16 * 1024;

/// FPU shape: single precision on every part of the family (the
/// double-precision forms run in the emulation). Consumed by records and
/// gates, never by context-switch code.
/// The part: the STM32L4R5ZI's Cortex-M4 with a single-precision FPU (its
/// datasheet: "a single-precision floating-point unit").
#[allow(dead_code)] // board-facts surface; the board facade's assert reads it
pub const FPU_DP: bool = false;
/// The cycle counter the lost-tick witness reads: the core's data
/// watchpoint and trace unit counts processor cycles, enabled and
/// verified to advance at the scheduler's init.
pub const CYCLE_COUNTER: bool = true;

const RCC: usize = 0x4002_1000;
const RCC_CR: *mut u32 = RCC as *mut u32;
const RCC_CFGR: *mut u32 = (RCC + 0x08) as *mut u32;
#[cfg(stm32l4plus_pll120)]
const RCC_PLLCFGR: *mut u32 = (RCC + 0x0C) as *mut u32;
const RCC_AHB2ENR: *mut u32 = (RCC + 0x4C) as *mut u32;
const RCC_APB1ENR1: *mut u32 = (RCC + 0x58) as *mut u32;
const RCC_APB1ENR2: *mut u32 = (RCC + 0x5C) as *mut u32;
const RCC_CCIPR: *mut u32 = (RCC + 0x88) as *mut u32;
const RCC_CRRCR: *mut u32 = (RCC + 0x98) as *mut u32;

const CR_HSION: u32 = 1 << 8;
const CR_HSIRDY: u32 = 1 << 10;
#[cfg(stm32l4plus_pll120)]
const CR_PLLON: u32 = 1 << 24;
#[cfg(stm32l4plus_pll120)]
const CR_PLLRDY: u32 = 1 << 25;
/// CFGR fields: SW [1:0] (00 MSI, 01 HSI16, 10 HSE, 11 PLL), SWS [3:2] the
/// same codes, HPRE [7:4] (0xxx = div1, 1000 = div2), PPRE1 [10:8] and
/// PPRE2 [13:11] (0xx = div1).
const CFGR_SW: u32 = 0b11;
const CFGR_SWS: u32 = 0b11 << 2;
const CFGR_HPRE: u32 = 0b1111 << 4;
const CFGR_PPRE1: u32 = 0b111 << 8;
const CFGR_PPRE2: u32 = 0b111 << 11;
const CFGR_SW_HSI: u32 = 0b01;
const CFGR_SWS_HSI: u32 = 0b01 << 2;
#[cfg(stm32l4plus_pll120)]
const CFGR_SW_PLL: u32 = 0b11;
#[cfg(stm32l4plus_pll120)]
const CFGR_SWS_PLL: u32 = 0b11 << 2;
#[cfg(stm32l4plus_pll120)]
const CFGR_HPRE_DIV2: u32 = 0b1000 << 4;

/// PLLCFGR fields: PLLSRC [1:0] (10 = HSI16), PLLM [7:4] (the field plus
/// one divides: 0001 = /2), PLLN [14:8] (x30), PLLREN [24] (the system
/// clock output), PLLR [26:25] (00 = /2). The other outputs stay off: the
/// random source's 48 MHz clock is HSI48, not a PLL tap.
#[cfg(stm32l4plus_pll120)]
const PLLSRC_HSI: u32 = 0b10;
#[cfg(stm32l4plus_pll120)]
const PLLM_DIV2: u32 = 0b0001 << 4;
#[cfg(stm32l4plus_pll120)]
const PLLN: u32 = 30;
#[cfg(stm32l4plus_pll120)]
const PLLREN: u32 = 1 << 24;
#[cfg(stm32l4plus_pll120)]
const PLLCFGR_VALUE: u32 = PLLSRC_HSI | PLLM_DIV2 | (PLLN << 8) | PLLREN;

const FLASH_ACR: *mut u32 = 0x4002_2000 as *mut u32;
const ACR_LATENCY: u32 = 0b1111;
/// 120 MHz in range-1 boost mode: 5 wait states (the manual's table).
#[cfg(stm32l4plus_pll120)]
const ACR_LATENCY_5WS: u32 = 5;

const APB1ENR1_PWREN: u32 = 1 << 28;
const PWR: usize = 0x4000_7000;
const PWR_CR1: *mut u32 = PWR as *mut u32;
const PWR_CR2: *mut u32 = (PWR + 0x04) as *mut u32;
const PWR_SR2: *mut u32 = (PWR + 0x14) as *mut u32;
#[cfg(stm32l4plus_pll120)]
const PWR_CR5: *mut u32 = (PWR + 0x80) as *mut u32;
/// PWR_CR1: VOS [10:9] (01 = range 1); PWR_SR2: VOSF [10], set while the
/// regulator moves; PWR_CR5: R1MODE [8] (0 = range-1 boost mode, the
/// 120 MHz mode); PWR_CR2: IOSV [9], the independent I/O supply valid --
/// port G's pins 2 to 15 are isolated after reset until it is set.
const CR1_VOS: u32 = 0b11 << 9;
const CR1_VOS_RANGE1: u32 = 0b01 << 9;
const SR2_VOSF: u32 = 1 << 10;
#[cfg(stm32l4plus_pll120)]
const CR5_R1MODE: u32 = 1 << 8;
const CR2_IOSV: u32 = 1 << 9;

/// The port blocks: GPIOA at 0x4800_0000, one 1 KB block per port, their
/// clock enables the same bit positions in RCC_AHB2ENR; the mode register
/// at a block's base, the alternate-function pair at 0x20 and 0x24.
const GPIO_BASE: usize = 0x4800_0000;
const GPIO_PORT_STRIDE: usize = 0x400;
const GPIO_AFRL: usize = 0x20;
const GPIO_AFRH: usize = 0x24;
const APB1ENR2_LPUART1EN: u32 = 1 << 0;
/// LPUART1's pins carry alternate function 8 on the port that routes them
/// (the part's datasheet's alternate-function table).
const LPUART1_AF: u32 = 8;
/// CCIPR's LPUART1SEL [11:10]: 10 = HSI16 as the LPUART's kernel clock.
const CCIPR_LPUART1SEL: u32 = 0b11 << 10;
const CCIPR_LPUART1SEL_HSI: u32 = 0b10 << 10;

const LPUART1: usize = 0x4000_8000;
const LPUART1_CR1: *mut u32 = LPUART1 as *mut u32;
const LPUART1_BRR: *mut u32 = (LPUART1 + 0x0C) as *mut u32;
const LPUART1_ISR: *mut u32 = (LPUART1 + 0x1C) as *mut u32;
const LPUART1_ICR: *mut u32 = (LPUART1 + 0x20) as *mut u32;
const LPUART1_RDR: *mut u32 = (LPUART1 + 0x24) as *mut u32;
const LPUART1_TDR: *mut u32 = (LPUART1 + 0x28) as *mut u32;

const CR1_UE: u32 = 1 << 0;
const CR1_RE: u32 = 1 << 2;
const CR1_TE: u32 = 1 << 3;
const ISR_TXE: u32 = 1 << 7;
const ISR_RXNE: u32 = 1 << 5;
const ISR_ORE: u32 = 1 << 3;
const ICR_ORECF: u32 = 1 << 3;
const BAUD: u32 = 115_200;
/// The LPUART's divisor is 256 times its kernel clock over the baud rate
/// (the manual's formula; never below 0x300, the clock between 3 and 4096
/// times the rate): 35,556 here, rounded to nearest.
const BRR_VALUE: u32 = ((256u64 * LPUART_CK_HZ as u64 + (BAUD / 2) as u64) / BAUD as u64) as u32;
const _: () = assert!(BRR_VALUE >= 0x300 && BRR_VALUE < (1 << 20), "the LPUART divisor is outside its window");

/// The transmit-empty interrupt's enable (LPUART_CR1 TXEIE, bit 7, the
/// FIFO disabled): the level-shaped interrupt the ring's drain runs on --
/// asserted whenever the transmit data register is empty, so the ring
/// module sets it only while a backlog exists and clears it when the ring
/// empties.
const CR1_TXEIE: u32 = 1 << 7;

/// LPUART1's interrupt line (the manual's vector table, position 70), its
/// enable bit in the interrupt controller's third set-enable and
/// clear-pending registers, and its priority byte in the priority array.
pub const LPUART1_IRQ: usize = 70;
const NVIC_ISER2: *mut u32 = 0xE000_E108 as *mut u32;
const NVIC_ICPR2: *mut u32 = 0xE000_E288 as *mut u32;
const NVIC_IPR: *mut u8 = 0xE000_E400 as *mut u8;
/// The lowest band: masked inside a kernel critical section (the ceiling
/// 0x40), below the tick (0x80), above the context switch (0xF0) -- the
/// drain preempts thread code and nothing else, and the writer's section
/// is its lock.
const LPUART1_IRQ_PRIO: u8 = 0xE0;

/// The interrupt lines a board of this family claims with its own
/// handlers, folded into the vector table by the boot module: LPUART1's
/// line, the console ring's transmit drain.
pub const BOARD_IRQS: &[(usize, unsafe extern "C" fn())] = &[(LPUART1_IRQ, __rhu_stm32l4plus_lpuart1_irq)];
/// The family GPIO driver's external-interrupt lines (the classic arm).
pub use super::stm32_gpio::IRQS as GPIO_IRQS;

/// The regulator to range 1, stated explicitly (it is the reset value; a
/// resident may have changed it), and its flag waited clear: the flash
/// latency rows below assume range 1.
unsafe fn regulator_range1() {
    unsafe {
        write_volatile(RCC_APB1ENR1, read_volatile(RCC_APB1ENR1) | APB1ENR1_PWREN);
        let _ = read_volatile(RCC_APB1ENR1);
        write_volatile(PWR_CR1, (read_volatile(PWR_CR1) & !CR1_VOS) | CR1_VOS_RANGE1);
        while read_volatile(PWR_SR2) & SR2_VOSF != 0 {}
    }
}

/// The downshift to a known-safe HSI16 world: HSI16 on and waited for,
/// SYSCLK forced to it with div1 prescalers, the switch status read back.
unsafe fn downshift_to_hsi16() {
    unsafe {
        write_volatile(RCC_CR, read_volatile(RCC_CR) | CR_HSION);
        while read_volatile(RCC_CR) & CR_HSIRDY == 0 {}
        let cfgr = read_volatile(RCC_CFGR) & !(CFGR_SW | CFGR_HPRE | CFGR_PPRE1 | CFGR_PPRE2);
        write_volatile(RCC_CFGR, cfgr | CFGR_SW_HSI);
        while read_volatile(RCC_CFGR) & CFGR_SWS != CFGR_SWS_HSI {}
    }
}

/// Enforce the HSI16 posture. On a cold flash boot every write below
/// restates reset state but the clock source (the part resets on its 4 MHz
/// RC); with a probe or resident image on the bus, nothing upstream is
/// assumed: the downshift first, the regulator to range 1, and flash
/// latency dropped to the 16 MHz row only after both. The PLL, the 4 MHz
/// RC and the flash interface's cache and prefetch bits are left as found
/// -- all off the HSI16 path, and levers this posture does not need stay
/// unpulled.
#[cfg(not(stm32l4plus_pll120))]
pub fn clock_init() {
    unsafe {
        downshift_to_hsi16();
        regulator_range1();
        write_volatile(FLASH_ACR, read_volatile(FLASH_ACR) & !ACR_LATENCY);
    }
    ring::boot_mark();
}

/// Boring posture: no clock line in the banner (the recorded boot transcript
/// surface stands byte-identical).
#[cfg(not(stm32l4plus_pll120))]
pub fn clock_readback() -> Option<crate::board::ClockReadback> {
    None
}

/// A bounded settle after the switch to the PLL: the manual asks at least
/// one microsecond at the halved bus clock before the prescaler is restored;
/// each iteration is one peripheral register read (several cycles), so the
/// count is worth more than two microseconds at 60 MHz.
#[cfg(stm32l4plus_pll120)]
const SETTLE_READS: u32 = 120;

/// Enforce the PLL120 posture, every step on its hardware ready flag, the
/// manual's boost sequence in its order. Entry assumes nothing (probe or
/// resident may have left any state): first the downshift to HSI16, then
/// the PLL is stopped so it can be reconfigured, the regulator to range
/// 1. Then the boost sequence: the AHB prescaler to div2 while still on
/// HSI16 (the manual halves the bus clock before a switch above 80 MHz),
/// the boost mode selected, the latency RAISED to the 120 MHz row and read
/// back BEFORE the up-switch (the same never-fast-with-low-WS principle
/// that dropped it after the downshift in posture 1), the PLL configured
/// and locked, the switch, the settle, the prescaler back to div1. The APB
/// prescalers stay at div1: both domains are allowed the full 120 MHz.
/// The flash interface's cache and prefetch bits are left as found.
#[cfg(stm32l4plus_pll120)]
pub fn clock_init() {
    unsafe {
        downshift_to_hsi16();

        // PLL off and unlocked before PLLCFGR may be written.
        write_volatile(RCC_CR, read_volatile(RCC_CR) & !CR_PLLON);
        while read_volatile(RCC_CR) & CR_PLLRDY != 0 {}

        regulator_range1();

        // The boost sequence: the bus clock halved first, the boost mode
        // selected, then 5 wait states before anything runs fast.
        write_volatile(RCC_CFGR, read_volatile(RCC_CFGR) | CFGR_HPRE_DIV2);
        write_volatile(PWR_CR5, read_volatile(PWR_CR5) & !CR5_R1MODE);
        let acr = read_volatile(FLASH_ACR) & !ACR_LATENCY;
        write_volatile(FLASH_ACR, acr | ACR_LATENCY_5WS);
        while read_volatile(FLASH_ACR) & ACR_LATENCY != ACR_LATENCY_5WS {}

        // HSI16/2 x30 /2: the VCO in at 8 MHz, out at 240, SYSCLK 120.
        write_volatile(RCC_PLLCFGR, PLLCFGR_VALUE);
        write_volatile(RCC_CR, read_volatile(RCC_CR) | CR_PLLON);
        while read_volatile(RCC_CR) & CR_PLLRDY == 0 {}

        // The switch at the halved bus clock, the settle, the prescaler back.
        let cfgr = read_volatile(RCC_CFGR) & !CFGR_SW;
        write_volatile(RCC_CFGR, cfgr | CFGR_SW_PLL);
        while read_volatile(RCC_CFGR) & CFGR_SWS != CFGR_SWS_PLL {}
        let mut n = 0u32;
        while n < SETTLE_READS {
            let _ = read_volatile(RCC_CFGR);
            n += 1;
        }
        write_volatile(RCC_CFGR, read_volatile(RCC_CFGR) & !CFGR_HPRE);
    }
    ring::boot_mark();
}

/// Gated posture: report the live clock tree for the banner line, every
/// numeric field decoded from registers (a silently-failed PLL yields
/// SWS=HSI16 and 16 MHz here, and the kmain assert dies loudly). The
/// board's crystal is decode-only: this posture never enables HSE, but an
/// honest decoder still names the source code it read. The 4 MHz RC is
/// named with its reset rate: this posture never sends it to the PLL. The
/// `pllp` field carries the R divider (the system clock's); `boost` states
/// the regulator's range-1 boost mode in the reference manual's own words.
#[cfg(stm32l4plus_pll120)]
pub fn clock_readback() -> Option<crate::board::ClockReadback> {
    const HSE_HZ: u32 = crate::board::HSE_HZ;
    const HSI_HZ: u32 = 16_000_000;
    const MSI_RESET_HZ: u32 = 4_000_000;
    unsafe {
        let cfgr = read_volatile(RCC_CFGR);
        let pllcfgr = read_volatile(RCC_PLLCFGR);
        let m = ((pllcfgr >> 4) & 0xF) + 1;
        let n = (pllcfgr >> 8) & 0x7F;
        let r = (((pllcfgr >> 25) & 0b11) + 1) * 2;
        let (src, base) = match pllcfgr & 0b11 {
            0b01 => ("MSI", MSI_RESET_HZ),
            0b10 => ("HSI16", HSI_HZ),
            0b11 => ("HSE", HSE_HZ),
            _ => ("none", 0),
        };
        let (sws, sysclk_hz) = match (cfgr >> 2) & 0b11 {
            0b00 => ("MSI", MSI_RESET_HZ),
            0b01 => ("HSI16", HSI_HZ),
            0b10 => ("HSE", HSE_HZ),
            _ => ("PLL", base / m * n / r),
        };
        Some(crate::board::ClockReadback {
            posture: "PLL120",
            sysclk_hz,
            sws,
            src,
            pllm: m,
            plln: n,
            pllp: r,
            flash_ws: read_volatile(FLASH_ACR) & ACR_LATENCY,
            boost: if read_volatile(PWR_CR5) & CR5_R1MODE == 0 { "range-1 boost on" } else { "range-1 boost off" },
        })
    }
}

/// One pin to LPUART1's alternate function: the mode field's two bits at
/// twice the pin in the port's mode register, the function's four bits
/// in the low alternate-function register for pins 0 to 7 and the high
/// one for 8 to 15.
unsafe fn route_lpuart1_pin(port: usize, pin: u32) {
    unsafe {
        let base = GPIO_BASE + port * GPIO_PORT_STRIDE;
        let moder = base as *mut u32;
        write_volatile(moder, (read_volatile(moder) & !(0b11 << (2 * pin))) | (0b10 << (2 * pin)));
        let afr = (base + if pin < 8 { GPIO_AFRL } else { GPIO_AFRH }) as *mut u32;
        let shift = 4 * (pin % 8);
        write_volatile(afr, (read_volatile(afr) & !(0xF << shift)) | (LPUART1_AF << shift));
    }
}

/// Route the board's two pins to LPUART1 (the ST-LINK VCP wiring) and
/// program 115200 8N1 from HSI16. Runs once, at the first console init (a
/// panic before the boot's own init reaches it through the same path, so
/// HSI16 is switched on here too, and the port's supply declared present),
/// so the UE-low reprogram window that could tear an in-flight character
/// is never re-entered.
unsafe fn console_hw_init() {
    unsafe {
        let [(tx_port, tx_pin), (rx_port, rx_pin)] = crate::board::CONSOLE_PINS;
        write_volatile(RCC_CR, read_volatile(RCC_CR) | CR_HSION);
        while read_volatile(RCC_CR) & CR_HSIRDY == 0 {}
        write_volatile(RCC_APB1ENR1, read_volatile(RCC_APB1ENR1) | APB1ENR1_PWREN);
        let _ = read_volatile(RCC_APB1ENR1);
        write_volatile(PWR_CR2, read_volatile(PWR_CR2) | CR2_IOSV);
        write_volatile(RCC_AHB2ENR, read_volatile(RCC_AHB2ENR) | (1 << tx_port) | (1 << rx_port));
        let _ = read_volatile(RCC_AHB2ENR);
        write_volatile(RCC_APB1ENR2, read_volatile(RCC_APB1ENR2) | APB1ENR2_LPUART1EN);
        let _ = read_volatile(RCC_APB1ENR2);
        write_volatile(RCC_CCIPR, (read_volatile(RCC_CCIPR) & !CCIPR_LPUART1SEL) | CCIPR_LPUART1SEL_HSI);

        route_lpuart1_pin(tx_port, tx_pin);
        route_lpuart1_pin(rx_port, rx_pin);

        write_volatile(LPUART1_CR1, 0);
        write_volatile(LPUART1_BRR, BRR_VALUE);
        write_volatile(LPUART1_CR1, CR1_UE | CR1_TE | CR1_RE);
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
            write_volatile(NVIC_IPR.add(LPUART1_IRQ), LPUART1_IRQ_PRIO);
            write_volatile(NVIC_ICPR2, 1 << (LPUART1_IRQ - 64));
            write_volatile(NVIC_ISER2, 1 << (LPUART1_IRQ - 64));
            ring::set_mode(ring::MODE_IRQ);
        } else {
            ring_tx_irq(false);
            ring::set_mode(ring::MODE_POLLED);
        }
    }
}

/// The ring's four hooks on this LPUART: the transmit data register takes
/// a byte while its empty flag is set; a byte goes into the transmit data
/// register (which clears the flag until the byte moves on to the shift
/// register); nothing latches, so the acknowledge is empty; and the
/// transmit-empty interrupt is level-shaped, so its enable is set only
/// while the ring holds a backlog and cleared when the ring empties.
pub unsafe fn ring_tx_room() -> bool {
    unsafe { read_volatile(LPUART1_ISR) & ISR_TXE != 0 }
}
pub unsafe fn ring_tx_put(b: u8) {
    unsafe { write_volatile(LPUART1_TDR, b as u32) }
}
pub unsafe fn ring_tx_ack() {}
pub unsafe fn ring_tx_irq(enabled: bool) {
    unsafe {
        let cr1 = read_volatile(LPUART1_CR1);
        let want = if enabled { cr1 | CR1_TXEIE } else { cr1 & !CR1_TXEIE };
        if want != cr1 {
            write_volatile(LPUART1_CR1, want);
        }
    }
}

/// LPUART1's interrupt: the ring's drain in the interrupt mode, raised
/// while the transmit data register is empty and the enable is set.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __rhu_stm32l4plus_lpuart1_irq() {
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
        if read_volatile(LPUART1_ISR) & ISR_RXNE != 0 {
            return Some(read_volatile(LPUART1_RDR) as u8);
        }
        if read_volatile(LPUART1_ISR) & ISR_ORE != 0 {
            write_volatile(LPUART1_ICR, ICR_ORECF);
        }
        None
    }
}

/// Whether address 0 is readable memory on this row (a read that returns
/// a word, or a bus fault): the api flavor's observation reads it before
/// it touches the address. With the null-page guard the protection unit denies the first
/// page on every row, so a guarded image never attempts the read; the
/// `null-guard-off` control image alone does.
/// This family: the first flash bank's boot alias at 0 on every part it
/// serves.
#[allow(dead_code)] // read by the api flavor's observation only
pub const ADDRESS_ZERO_READABLE: bool = true;

/// The entropy posture of the family's rows: the part's random number
/// generator samples its noise sources on the 48 MHz internal RC
/// (RCC_CCIPR's CLK48SEL at 00), a clock no posture forms or stops, so
/// the source serves on both clock postures.
pub const ENTROPY: super::EntropyPosture = super::EntropyPosture::Source {
    id: 4,
    name: "STM32L4+ RNG",
    line: "hardware source STM32L4+ RNG enabled on HSI48; its self-tests gate every draw",
};
/// Whether this row carries a real-time clock the kernel reads at boot and
/// writes at an explicit set.
#[allow(dead_code)] // read by the api flavor's calendar check
pub const RTC_PRESENT: bool = false;
/// The calendar banner's text for the family's rows, constant per image:
/// the board mounts the 32.768 kHz crystal and carries no backup cell.
pub const RTC_LINE: &str = "RTC block present on this part, a mounted 32.768 kHz crystal, no backup cell; this row does not read the clock: the base is the epoch";
pub use super::absent::{rtc_read_at_boot, rtc_write};

/// The RNG block (the manual's random number generator chapter) at
/// 0x5006_0800 on the AHB2 bus: three registers. RNG_CR: CED bit 5 (0 =
/// the clock checker on; its reset value, and it cannot change while the
/// block is enabled), IE bit 3, RNGEN bit 2. RNG_SR: SEIS bit 6 and CEIS
/// bit 5 (the seed and clock error interrupt flags, cleared by writing 0),
/// SECS bit 2 and CECS bit 1 (the current seed and clock error status,
/// read-only), DRDY bit 0. RNG_DR: the 32-bit sample, valid when DRDY is
/// set and the value is not zero. The block's bus clock is RCC_AHB2ENR's
/// RNGEN (bit 18); its sampling clock the 48 MHz RC, switched on through
/// RCC_CRRCR (HSI48ON bit 0, HSI48RDY bit 1) and selected by CLK48SEL.
const RNG: usize = 0x5006_0800;
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
const AHB2ENR_RNGEN: u32 = 1 << 18;
const CCIPR_CLK48SEL: u32 = 0b11 << 26;
const CRRCR_HSI48ON: u32 = 1 << 0;
const CRRCR_HSI48RDY: u32 = 1 << 1;
/// A bounded wait's iteration count for at least two milliseconds: the loop
/// body is at least four cycles, so the count is the cycles of two
/// milliseconds over four -- at the boot the tick clock is not yet running,
/// so every wait here is an iteration bound, never a clock. The 48 MHz RC
/// starts in microseconds and the block conditions its first word in
/// microseconds, orders of magnitude inside the bound: the bound is a fault
/// detector, never a wait.
const SPIN_2MS: u32 = SYSCLK_HZ / 500 / 4;

/// The block started: the 48 MHz RC switched on and waited ready under the
/// bound, selected as the 48 MHz clock and read back, the bus clock enabled
/// and read back, the block enabled with the clock checker on, the first
/// ready word waited for under the bound, the status read. False when the
/// RC did not come ready, the checker or the noise source flagged, or no
/// word came (the witness then reads the source absent and every secure
/// draw refuses). With the checker on, the block flags a clock error
/// whenever its clock is slower than a thirty-second of the bus clock; 48
/// MHz against 120 / 32 clears it on the fast posture and 16 / 32 on the
/// default. Nothing here masks an interrupt: the boot path runs before the
/// scheduler.
pub fn entropy_init() -> bool {
    unsafe {
        write_volatile(RCC_CRRCR, read_volatile(RCC_CRRCR) | CRRCR_HSI48ON);
        let mut n = 0u32;
        while read_volatile(RCC_CRRCR) & CRRCR_HSI48RDY == 0 {
            n += 1;
            if n > SPIN_2MS {
                return false;
            }
        }
        write_volatile(RCC_CCIPR, read_volatile(RCC_CCIPR) & !CCIPR_CLK48SEL);
        if read_volatile(RCC_CCIPR) & CCIPR_CLK48SEL != 0 {
            return false;
        }
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
