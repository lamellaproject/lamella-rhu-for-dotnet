//! STM32F769I-DISCO (STM32F769NI, Cortex-M7 r1p0): the first silicon
//! board. Flash-boot at 0x08000000, USART1 console on the ST-LINK VCP.
//! Two clock postures, feature-selected (`stm32f769i-disco-pll216` -- the gated-
//! posture pattern: own feature, own image names, own goldens, banner
//! line from register read-back): the boring HSI 16 MHz default, and
//! PLL216 (216 MHz, the production posture). Caches and ART stay off
//! in both -- cache enablement would be its own gated posture. The
//! cache posture is a law the hosting tier reads: with the caches off,
//! the shim's instruction-stream sync after it writes a fresh block of
//! executable thunks is two barriers and no cache maintenance; the day
//! a posture turns a cache on, a cache-sync door lands in this kernel
//! beside it and that sync calls it.

use core::ptr::{read_volatile, write_volatile};

use super::ring;

pub const NAME: &str = "STM32F769I-DISCO";
pub const CONSOLE: &str = "USART1 (ST-LINK VCP)";

/// Clock posture 1 (default): HSI 16 MHz, SYSCLK = HCLK = PCLK1 = PCLK2
/// (all prescalers div1). `clock_init` enforces exactly this posture.
#[cfg(not(feature = "stm32f769i-disco-pll216"))]
pub const SYSCLK_HZ: u32 = 16_000_000;
#[cfg(not(feature = "stm32f769i-disco-pll216"))]
const PCLK2_HZ: u32 = SYSCLK_HZ;

/// Clock posture 2 (`stm32f769i-disco-pll216`): PLL at 216 MHz off HSI -- HSI/8
/// gives a 2 MHz VCO input, x216 puts the VCO at its 432 MHz ceiling,
/// /2 lands SYSCLK at 216. HCLK = SYSCLK; APB1 = /4 (54 MHz, its cap);
/// APB2 = /2 (108 MHz, its cap). Bus-frequency caps per the F769NI
/// datasheet DS11532 (the 216/108/54 MHz maxima); the register recipe
/// per RM0410.
#[cfg(feature = "stm32f769i-disco-pll216")]
pub const SYSCLK_HZ: u32 = 216_000_000;
#[cfg(feature = "stm32f769i-disco-pll216")]
const PCLK2_HZ: u32 = SYSCLK_HZ / 2;

/// SysTick rate.
pub const TICK_HZ: u32 = 1_000;

/// Thread stack pool (lives in .kstacks) -- same budgets as the QEMU
/// board; 512 K of contiguous RAM (the DTCM inside DATA) leaves wide
/// headroom (the link asserts it).
pub const STACK_POOL_BYTES: usize = 64 * 1024;

/// The boot kheap carve out of the arena (the GC board table's
/// `kheap_carve` row): in a hosted image the C library's whole heap.
/// 16K is the mini row's carve, funded inside the payload map's 192K
/// floor beside the 96K region and a frozen-object pool of one stock
/// commit; the first real hosted program booted to its verdict on
/// carves down to 8K (measured on the QEMU twin at this revision).
pub const KHEAP_BYTES: usize = 16 * 1024;

/// Stack for the kmain thread (the boot thread's continuation).
pub const KMAIN_STACK_BYTES: usize = 16 * 1024;

/// External IRQ lines on the F76x/F77x (RM0410 vector table, positions
/// 0..=109) -- sizes the vector table.
pub const NIRQ: usize = 110;

/// FPU shape: FPv5 double precision. Consumed by records and gates,
/// never by context-switch code.
#[allow(dead_code)]
pub const FPU_DP: bool = true;
/// The cycle counter the lost-tick witness reads: the core's data
/// watchpoint and trace unit counts processor cycles, enabled and
/// verified to advance at the scheduler's init.
pub const CYCLE_COUNTER: bool = true;

/// RAM windows a hardware-pushed exception frame may plausibly live in:
/// DTCM+SRAM1+SRAM2, the one contiguous 512 K window this image uses
/// (stacks live here; each end backed off by one 8-word frame).
pub const FRAME_RANGES: [(usize, usize); 1] = [(0x2000_0000, 0x2008_0000 - 32)];

const RCC: usize = 0x4002_3800;
const RCC_CR: *mut u32 = RCC as *mut u32;
#[cfg(feature = "stm32f769i-disco-pll216")]
const RCC_PLLCFGR: *mut u32 = (RCC + 0x04) as *mut u32;
const RCC_CFGR: *mut u32 = (RCC + 0x08) as *mut u32;
const RCC_AHB1ENR: *mut u32 = (RCC + 0x30) as *mut u32;
#[cfg(feature = "stm32f769i-disco-pll216")]
const RCC_APB1ENR: *mut u32 = (RCC + 0x40) as *mut u32;
const RCC_APB2ENR: *mut u32 = (RCC + 0x44) as *mut u32;

const CR_HSION: u32 = 1 << 0;
const CR_HSIRDY: u32 = 1 << 1;
#[cfg(feature = "stm32f769i-disco-pll216")]
const CR_PLLON: u32 = 1 << 24;
#[cfg(feature = "stm32f769i-disco-pll216")]
const CR_PLLRDY: u32 = 1 << 25;
/// CFGR fields: SW [1:0], SWS [3:2], HPRE [7:4], PPRE1 [12:10],
/// PPRE2 [15:13]. Zero in every field = HSI selected, div1 everywhere.
const CFGR_SW: u32 = 0b11;
const CFGR_SWS: u32 = 0b11 << 2;
const CFGR_HPRE: u32 = 0b1111 << 4;
const CFGR_PPRE1: u32 = 0b111 << 10;
const CFGR_PPRE2: u32 = 0b111 << 13;
#[cfg(feature = "stm32f769i-disco-pll216")]
const CFGR_SW_PLL: u32 = 0b10;
#[cfg(feature = "stm32f769i-disco-pll216")]
const CFGR_SWS_PLL: u32 = 0b10 << 2;
/// APB prescaler codes: 0b101 = /4, 0b100 = /2 (top bit = "divide").
#[cfg(feature = "stm32f769i-disco-pll216")]
const CFGR_PPRE1_DIV4: u32 = 0b101 << 10;
#[cfg(feature = "stm32f769i-disco-pll216")]
const CFGR_PPRE2_DIV2: u32 = 0b100 << 13;

/// PLLCFGR fields: PLLM [5:0], PLLN [14:6], PLLP [17:16] (00 = /2),
/// PLLSRC [22] (0 = HSI), PLLQ [27:24]. PLLQ 9 taps the 432 MHz VCO to
/// a well-formed 48 MHz -- nothing consumes it; a garbage field is not
/// worth the two bits it saves.
#[cfg(feature = "stm32f769i-disco-pll216")]
const PLLM: u32 = 8;
#[cfg(feature = "stm32f769i-disco-pll216")]
const PLLN: u32 = 216;
#[cfg(feature = "stm32f769i-disco-pll216")]
const PLLQ: u32 = 9;
#[cfg(feature = "stm32f769i-disco-pll216")]
const PLLCFGR_VALUE: u32 = PLLM | (PLLN << 6) | (PLLQ << 24);

const FLASH_ACR: *mut u32 = 0x4002_3C00 as *mut u32;
const ACR_LATENCY: u32 = 0b1111;
/// 216 MHz at VOS1 + over-drive, 2.7-3.6 V supply row: 7 wait states.
#[cfg(feature = "stm32f769i-disco-pll216")]
const ACR_LATENCY_7WS: u32 = 7;

#[cfg(feature = "stm32f769i-disco-pll216")]
const APB1ENR_PWREN: u32 = 1 << 28;
#[cfg(feature = "stm32f769i-disco-pll216")]
const PWR: usize = 0x4000_7000;
#[cfg(feature = "stm32f769i-disco-pll216")]
const PWR_CR1: *mut u32 = PWR as *mut u32;
#[cfg(feature = "stm32f769i-disco-pll216")]
const PWR_CSR1: *mut u32 = (PWR + 0x04) as *mut u32;
/// PWR_CR1: VOS [15:14] (0b11 = Scale 1), ODEN [16], ODSWEN [17];
/// PWR_CSR1: VOSRDY [14], ODRDY [16], ODSWRDY [17].
#[cfg(feature = "stm32f769i-disco-pll216")]
const CR1_VOS_SCALE1: u32 = 0b11 << 14;
#[cfg(feature = "stm32f769i-disco-pll216")]
const CR1_ODEN: u32 = 1 << 16;
#[cfg(feature = "stm32f769i-disco-pll216")]
const CR1_ODSWEN: u32 = 1 << 17;
#[cfg(feature = "stm32f769i-disco-pll216")]
const CSR1_VOSRDY: u32 = 1 << 14;
#[cfg(feature = "stm32f769i-disco-pll216")]
const CSR1_ODRDY: u32 = 1 << 16;
#[cfg(feature = "stm32f769i-disco-pll216")]
const CSR1_ODSWRDY: u32 = 1 << 17;

const GPIOA: usize = 0x4002_0000;
const GPIOA_MODER: *mut u32 = GPIOA as *mut u32;
const GPIOA_AFRH: *mut u32 = (GPIOA + 0x24) as *mut u32;
const AHB1ENR_GPIOAEN: u32 = 1 << 0;
const APB2ENR_USART1EN: u32 = 1 << 4;

const USART1: usize = 0x4001_1000;
const USART1_CR1: *mut u32 = USART1 as *mut u32;
const USART1_BRR: *mut u32 = (USART1 + 0x0C) as *mut u32;
const USART1_ISR: *mut u32 = (USART1 + 0x1C) as *mut u32;
const USART1_ICR: *mut u32 = (USART1 + 0x20) as *mut u32;
const USART1_RDR: *mut u32 = (USART1 + 0x24) as *mut u32;
const USART1_TDR: *mut u32 = (USART1 + 0x28) as *mut u32;

const CR1_UE: u32 = 1 << 0;
const CR1_RE: u32 = 1 << 2;
const CR1_TE: u32 = 1 << 3;
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

/// USART1's interrupt line (RM0410's vector table, position 37), its
/// enable bit in the interrupt controller's second set-enable and
/// clear-pending registers, and its priority byte in the priority array.
pub const USART1_IRQ: usize = 37;
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
pub const BOARD_IRQS: &[(usize, unsafe extern "C" fn())] = &[(USART1_IRQ, __rhu_stm32f769_usart1_irq)];

/// Enforce the HSI-16 posture. On a cold flash boot every write below
/// restates reset state; with a probe or resident image on the bus,
/// nothing upstream is assumed: HSI is turned on and waited for, SYSCLK
/// is forced back to HSI with div1 prescalers, and flash latency drops
/// to the 16 MHz row only after the downshift. The PLL and the flash
/// ART/prefetch bits are left as found -- both are off the HSI path,
/// and levers this posture does not need stay unpulled.
#[cfg(not(feature = "stm32f769i-disco-pll216"))]
pub fn clock_init() {
    unsafe {
        write_volatile(RCC_CR, read_volatile(RCC_CR) | CR_HSION);
        while read_volatile(RCC_CR) & CR_HSIRDY == 0 {}
        let cfgr = read_volatile(RCC_CFGR)
            & !(CFGR_SW | CFGR_HPRE | CFGR_PPRE1 | CFGR_PPRE2);
        write_volatile(RCC_CFGR, cfgr);
        while read_volatile(RCC_CFGR) & CFGR_SWS != 0 {}
        write_volatile(FLASH_ACR, read_volatile(FLASH_ACR) & !ACR_LATENCY);
    }
    ring::boot_mark();
}

/// Boring posture: no clock line in the banner (the banked transcript
/// surface stands byte-identical).
#[cfg(not(feature = "stm32f769i-disco-pll216"))]
pub fn clock_readback() -> Option<crate::board::ClockReadback> {
    None
}

/// Enforce the PLL216 posture, every step on its hardware ready flag.
/// Entry assumes nothing (probe or resident may have left any state):
/// first the HSI-16 downshift -- HSI on and waited for, SYSCLK forced
/// to HSI with div1 prescalers -- then the PLL is stopped so it can be
/// reconfigured. Latency RISES to the 216 MHz row BEFORE the up-switch
/// (the same never-fast-with-low-WS principle that dropped it after
/// the downshift in posture 1). Over-drive is required above 180 MHz:
/// both handshakes (ODEN/ODRDY, then the ODSWEN/ODSWRDY regulator
/// switch) run while SYSCLK is still HSI. APB prescalers land before
/// the switch so no bus overshoots its cap for even a cycle. The flash
/// ART/prefetch bits are left as found; caches stay off (posture law).
#[cfg(feature = "stm32f769i-disco-pll216")]
pub fn clock_init() {
    unsafe {
        write_volatile(RCC_CR, read_volatile(RCC_CR) | CR_HSION);
        while read_volatile(RCC_CR) & CR_HSIRDY == 0 {}
        let cfgr = read_volatile(RCC_CFGR)
            & !(CFGR_SW | CFGR_HPRE | CFGR_PPRE1 | CFGR_PPRE2);
        write_volatile(RCC_CFGR, cfgr);
        while read_volatile(RCC_CFGR) & CFGR_SWS != 0 {}

        write_volatile(RCC_CR, read_volatile(RCC_CR) & !CR_PLLON);
        while read_volatile(RCC_CR) & CR_PLLRDY != 0 {}

        write_volatile(RCC_APB1ENR, read_volatile(RCC_APB1ENR) | APB1ENR_PWREN);
        let _ = read_volatile(RCC_APB1ENR);
        write_volatile(PWR_CR1, read_volatile(PWR_CR1) | CR1_VOS_SCALE1);

        let acr = read_volatile(FLASH_ACR) & !ACR_LATENCY;
        write_volatile(FLASH_ACR, acr | ACR_LATENCY_7WS);
        while read_volatile(FLASH_ACR) & ACR_LATENCY != ACR_LATENCY_7WS {}

        write_volatile(RCC_PLLCFGR, PLLCFGR_VALUE);
        write_volatile(RCC_CR, read_volatile(RCC_CR) | CR_PLLON);
        while read_volatile(RCC_CR) & CR_PLLRDY == 0 {}
        while read_volatile(PWR_CSR1) & CSR1_VOSRDY == 0 {}

        write_volatile(PWR_CR1, read_volatile(PWR_CR1) | CR1_ODEN);
        while read_volatile(PWR_CSR1) & CSR1_ODRDY == 0 {}
        write_volatile(PWR_CR1, read_volatile(PWR_CR1) | CR1_ODSWEN);
        while read_volatile(PWR_CSR1) & CSR1_ODSWRDY == 0 {}

        let cfgr = read_volatile(RCC_CFGR) & !(CFGR_PPRE1 | CFGR_PPRE2);
        write_volatile(RCC_CFGR, cfgr | CFGR_PPRE1_DIV4 | CFGR_PPRE2_DIV2);
        let cfgr = read_volatile(RCC_CFGR) & !CFGR_SW;
        write_volatile(RCC_CFGR, cfgr | CFGR_SW_PLL);
        while read_volatile(RCC_CFGR) & CFGR_SWS != CFGR_SWS_PLL {}
    }
    ring::boot_mark();
}

/// Gated posture: report the live clock tree for the banner line, every
/// numeric field decoded from registers (a silently-failed PLL yields
/// SWS=HSI and 16 MHz here, and the kmain assert dies loudly).
#[cfg(feature = "stm32f769i-disco-pll216")]
pub fn clock_readback() -> Option<crate::board::ClockReadback> {
    /// The DISCO's HSE crystal (X2, 25 MHz per UM2033) -- decode-only:
    /// this posture never enables HSE, but an honest decoder still
    /// names the source bit it read.
    const HSE_HZ: u32 = 25_000_000;
    const HSI_HZ: u32 = 16_000_000;
    unsafe {
        let cfgr = read_volatile(RCC_CFGR);
        let pllcfgr = read_volatile(RCC_PLLCFGR);
        let m = pllcfgr & 0x3F;
        let n = (pllcfgr >> 6) & 0x1FF;
        let p = (((pllcfgr >> 16) & 0b11) + 1) * 2;
        let hse_src = (pllcfgr >> 22) & 1 == 1;
        let (src, base) = if hse_src { ("HSE", HSE_HZ) } else { ("HSI", HSI_HZ) };
        let (sws, sysclk_hz) = match (cfgr >> 2) & 0b11 {
            0b00 => ("HSI", HSI_HZ),
            0b01 => ("HSE", HSE_HZ),
            0b10 => ("PLL", base / m * n / p),
            _ => ("??", 0),
        };
        Some(crate::board::ClockReadback {
            posture: "PLL216",
            sysclk_hz,
            sws,
            src,
            pllm: m,
            plln: n,
            pllp: p,
            flash_ws: read_volatile(FLASH_ACR) & ACR_LATENCY,
            overdrive: read_volatile(PWR_CSR1) & CSR1_ODSWRDY != 0,
        })
    }
}

/// Route PA9/PA10 to USART1 (AF7 -- the ST-LINK VCP wiring) and program
/// 115200 8N1 from PCLK2. Runs once, at the first console init (a panic
/// before the boot's own init reaches it through the same path), so the
/// UE-low reprogram window that could tear an in-flight character is
/// never re-entered.
unsafe fn console_hw_init() {
    unsafe {
        write_volatile(RCC_AHB1ENR, read_volatile(RCC_AHB1ENR) | AHB1ENR_GPIOAEN);
        let _ = read_volatile(RCC_AHB1ENR);
        write_volatile(RCC_APB2ENR, read_volatile(RCC_APB2ENR) | APB2ENR_USART1EN);
        let _ = read_volatile(RCC_APB2ENR);

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
pub unsafe extern "C" fn __rhu_stm32f769_usart1_irq() {
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
/// This row: the ITCM RAM at 0.
#[allow(dead_code)]
pub const ADDRESS_ZERO_READABLE: bool = true;

/// The entropy posture of this row: the part carries an RNG block that no driver
/// serves on this row, so no source serves.
pub const ENTROPY: super::EntropyPosture = super::EntropyPosture::None;
/// Whether this row carries a real-time clock the kernel reads at boot and
/// writes at an explicit set.
#[allow(dead_code)]
pub const RTC_PRESENT: bool = false;
/// The calendar banner's text for this row, constant per image.
pub const RTC_LINE: &str = "RTC block present on this part, a mounted 32.768 kHz crystal, no backup cell; this row does not read the clock: the base is the epoch until set";
pub use super::absent::{entropy_draw, entropy_init, rtc_read_at_boot, rtc_write};
