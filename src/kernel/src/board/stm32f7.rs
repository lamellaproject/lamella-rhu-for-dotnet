//! The STM32F7 family module: the boot posture, the two clock postures,
//! the console on USART1 driven the ring way, the random source and the
//! calendar facts every board of the family shares. Each board file
//! states its own facts -- the name, the console's pins, the crystal, the
//! interrupt count, the RAM windows, the stack pool, the kheap carve --
//! and re-exports the rest from here (the RP2350 chip module's pattern).

use core::ptr::{read_volatile, write_volatile};

use super::ring;

/// Clock posture 1 (default): HSI 16 MHz, SYSCLK = HCLK = PCLK1 = PCLK2
/// (all prescalers div1). `clock_init` enforces exactly this posture.
#[cfg(not(stm32f7_pll216))]
pub const SYSCLK_HZ: u32 = 16_000_000;
#[cfg(not(stm32f7_pll216))]
const PCLK2_HZ: u32 = SYSCLK_HZ;
/// The APB1 bus clock: the clock of the blocks that have no selection of
/// their own (the family's SPI2 among them); a board's bus facts read it.
#[cfg(not(stm32f7_pll216))]
#[allow(dead_code)]
pub const PCLK1_HZ: u32 = SYSCLK_HZ;

/// Clock posture 2 (the board's `-pll216` feature): PLL at 216 MHz off
/// HSI -- HSI/8 gives a 2 MHz VCO input, x216 puts the VCO at its 432 MHz
/// ceiling, /2 lands SYSCLK at 216. HCLK = SYSCLK; APB1 = /4 (54 MHz, its
/// cap); APB2 = /2 (108 MHz, its cap). Bus-frequency caps per the parts'
/// datasheets (the 216/108/54 MHz maxima); the register recipe per the
/// reference manuals.
#[cfg(stm32f7_pll216)]
pub const SYSCLK_HZ: u32 = 216_000_000;
#[cfg(stm32f7_pll216)]
const PCLK2_HZ: u32 = SYSCLK_HZ / 2;
#[cfg(stm32f7_pll216)]
#[allow(dead_code)]
pub const PCLK1_HZ: u32 = SYSCLK_HZ / 4;

/// SysTick rate.
pub const TICK_HZ: u32 = 1_000;

/// Stack for the kmain thread (the boot thread's continuation).
pub const KMAIN_STACK_BYTES: usize = 16 * 1024;

/// The cycle counter the lost-tick witness reads: the core's data
/// watchpoint and trace unit counts processor cycles, enabled and
/// verified to advance at the scheduler's init.
pub const CYCLE_COUNTER: bool = true;

const RCC: usize = 0x4002_3800;
const RCC_CR: *mut u32 = RCC as *mut u32;
#[cfg(stm32f7_pll216)]
const RCC_PLLCFGR: *mut u32 = (RCC + 0x04) as *mut u32;
const RCC_CFGR: *mut u32 = (RCC + 0x08) as *mut u32;
const RCC_AHB1ENR: *mut u32 = (RCC + 0x30) as *mut u32;
#[cfg(stm32f7_pll216)]
const RCC_APB1ENR: *mut u32 = (RCC + 0x40) as *mut u32;
const RCC_APB2ENR: *mut u32 = (RCC + 0x44) as *mut u32;

const CR_HSION: u32 = 1 << 0;
const CR_HSIRDY: u32 = 1 << 1;
#[cfg(stm32f7_pll216)]
const CR_PLLON: u32 = 1 << 24;
#[cfg(stm32f7_pll216)]
const CR_PLLRDY: u32 = 1 << 25;
/// CFGR fields: SW [1:0], SWS [3:2], HPRE [7:4], PPRE1 [12:10],
/// PPRE2 [15:13]. Zero in every field = HSI selected, div1 everywhere.
const CFGR_SW: u32 = 0b11;
const CFGR_SWS: u32 = 0b11 << 2;
const CFGR_HPRE: u32 = 0b1111 << 4;
const CFGR_PPRE1: u32 = 0b111 << 10;
const CFGR_PPRE2: u32 = 0b111 << 13;
#[cfg(stm32f7_pll216)]
const CFGR_SW_PLL: u32 = 0b10;
#[cfg(stm32f7_pll216)]
const CFGR_SWS_PLL: u32 = 0b10 << 2;
/// APB prescaler codes: 0b101 = /4, 0b100 = /2 (top bit = "divide").
#[cfg(stm32f7_pll216)]
const CFGR_PPRE1_DIV4: u32 = 0b101 << 10;
#[cfg(stm32f7_pll216)]
const CFGR_PPRE2_DIV2: u32 = 0b100 << 13;

/// PLLCFGR fields: PLLM [5:0], PLLN [14:6], PLLP [17:16] (00 = /2),
/// PLLSRC [22] (0 = HSI), PLLQ [27:24]. PLLQ 9 taps the 432 MHz VCO to
/// a well-formed 48 MHz -- the random source's clock on this posture; a
/// garbage field is not worth the two bits it saves.
#[cfg(stm32f7_pll216)]
const PLLM: u32 = 8;
#[cfg(stm32f7_pll216)]
const PLLN: u32 = 216;
#[cfg(stm32f7_pll216)]
const PLLQ: u32 = 9;
#[cfg(stm32f7_pll216)]
const PLLCFGR_VALUE: u32 = PLLM | (PLLN << 6) | (PLLQ << 24);

const FLASH_ACR: *mut u32 = 0x4002_3C00 as *mut u32;
const ACR_LATENCY: u32 = 0b1111;
/// 216 MHz at VOS1 + over-drive, 2.7-3.6 V supply row: 7 wait states.
#[cfg(stm32f7_pll216)]
const ACR_LATENCY_7WS: u32 = 7;

#[cfg(stm32f7_pll216)]
const APB1ENR_PWREN: u32 = 1 << 28;
#[cfg(stm32f7_pll216)]
const PWR: usize = 0x4000_7000;
#[cfg(stm32f7_pll216)]
const PWR_CR1: *mut u32 = PWR as *mut u32;
#[cfg(stm32f7_pll216)]
const PWR_CSR1: *mut u32 = (PWR + 0x04) as *mut u32;
/// PWR_CR1: VOS [15:14] (0b11 = Scale 1), ODEN [16], ODSWEN [17];
/// PWR_CSR1: VOSRDY [14], ODRDY [16], ODSWRDY [17].
#[cfg(stm32f7_pll216)]
const CR1_VOS_SCALE1: u32 = 0b11 << 14;
#[cfg(stm32f7_pll216)]
const CR1_ODEN: u32 = 1 << 16;
#[cfg(stm32f7_pll216)]
const CR1_ODSWEN: u32 = 1 << 17;
#[cfg(stm32f7_pll216)]
const CSR1_VOSRDY: u32 = 1 << 14;
#[cfg(stm32f7_pll216)]
const CSR1_ODRDY: u32 = 1 << 16;
#[cfg(stm32f7_pll216)]
const CSR1_ODSWRDY: u32 = 1 << 17;

/// The port blocks: GPIOA at 0x4002_0000, one 1 KB block per port, their
/// clock enables the same bit positions in RCC_AHB1ENR; the mode register
/// at a block's base, the alternate-function pair at 0x20 and 0x24.
const GPIO_BASE: usize = 0x4002_0000;
const GPIO_PORT_STRIDE: usize = 0x400;
const GPIO_AFRL: usize = 0x20;
const GPIO_AFRH: usize = 0x24;
const APB2ENR_USART1EN: u32 = 1 << 4;
/// USART1's pins carry alternate function 7 on every port that routes
/// them, on both manuals' alternate-function tables.
const USART1_AF: u32 = 7;

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

/// USART1's interrupt line (both manuals' vector tables, position 37),
/// its enable bit in the interrupt controller's second set-enable and
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

/// The interrupt lines a board of this family claims with its own
/// handlers, folded into the vector table by the boot module: USART1's
/// line, the console ring's transmit drain.
pub const BOARD_IRQS: &[(usize, unsafe extern "C" fn())] = &[(USART1_IRQ, __rhu_stm32f7_usart1_irq)];
/// The family GPIO driver's external-interrupt lines (the F7 arm).
pub use super::stm32_gpio::IRQS as GPIO_IRQS;

/// Enforce the HSI-16 posture. On a cold flash boot every write below
/// restates reset state; with a probe or resident image on the bus,
/// nothing upstream is assumed: HSI is turned on and waited for, SYSCLK
/// is forced back to HSI with div1 prescalers, and flash latency drops
/// to the 16 MHz row only after the downshift. The PLL and the flash
/// ART/prefetch bits are left as found -- both are off the HSI path,
/// and levers this posture does not need stay unpulled.
#[cfg(not(stm32f7_pll216))]
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
#[cfg(not(stm32f7_pll216))]
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
#[cfg(stm32f7_pll216)]
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
/// SWS=HSI and 16 MHz here, and the kmain assert dies loudly). The
/// board's crystal is decode-only: this posture never enables HSE, but an
/// honest decoder still names the source bit it read.
#[cfg(stm32f7_pll216)]
pub fn clock_readback() -> Option<crate::board::ClockReadback> {
    const HSE_HZ: u32 = crate::board::HSE_HZ;
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
            boost: if read_volatile(PWR_CSR1) & CSR1_ODSWRDY != 0 { "over-drive on" } else { "over-drive off" },
        })
    }
}

/// One pin to USART1's alternate function: the mode field's two bits at
/// twice the pin in the port's mode register, the function's four bits
/// in the low alternate-function register for pins 0 to 7 and the high
/// one for 8 to 15.
unsafe fn route_usart1_pin(port: usize, pin: u32) {
    unsafe {
        let base = GPIO_BASE + port * GPIO_PORT_STRIDE;
        let moder = base as *mut u32;
        write_volatile(moder, (read_volatile(moder) & !(0b11 << (2 * pin))) | (0b10 << (2 * pin)));
        let afr = (base + if pin < 8 { GPIO_AFRL } else { GPIO_AFRH }) as *mut u32;
        let shift = 4 * (pin % 8);
        write_volatile(afr, (read_volatile(afr) & !(0xF << shift)) | (USART1_AF << shift));
    }
}

/// Route the board's two pins to USART1 (the ST-LINK VCP wiring) and
/// program 115200 8N1 from PCLK2. Runs once, at the first console init (a
/// panic before the boot's own init reaches it through the same path), so
/// the UE-low reprogram window that could tear an in-flight character is
/// never re-entered.
unsafe fn console_hw_init() {
    unsafe {
        let [(tx_port, tx_pin), (rx_port, rx_pin)] = crate::board::CONSOLE_PINS;
        write_volatile(RCC_AHB1ENR, read_volatile(RCC_AHB1ENR) | (1 << tx_port) | (1 << rx_port));
        let _ = read_volatile(RCC_AHB1ENR);
        write_volatile(RCC_APB2ENR, read_volatile(RCC_APB2ENR) | APB2ENR_USART1EN);
        let _ = read_volatile(RCC_APB2ENR);

        route_usart1_pin(tx_port, tx_pin);
        route_usart1_pin(rx_port, rx_pin);

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
pub unsafe extern "C" fn __rhu_stm32f7_usart1_irq() {
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
/// This family: the ITCM RAM at 0 on every part it serves.
#[allow(dead_code)]
pub const ADDRESS_ZERO_READABLE: bool = true;

/// The entropy posture of the family's rows, feature-selected with the
/// clock posture (the manuals' RNG chapters): the part's RNG block samples
/// its noise source on the 48 MHz clock the main PLL's Q output forms
/// (RCC_DCKCFGR2's CK48MSEL at its reset value selects that output), so the
/// source serves on the PLL216 posture -- whose PLLQ of 9 taps the 432 MHz
/// VCO to exactly 48 MHz -- and is absent by posture on the HSI default,
/// where the PLL is off: no driver runs there and cryptographic draws
/// refuse honestly.
#[cfg(stm32f7_pll216)]
pub const ENTROPY: super::EntropyPosture = super::EntropyPosture::Source {
    id: 1,
    name: "STM32F7 RNG",
    line: "hardware source STM32F7 RNG enabled on PLL48CLK; its self-tests gate every draw",
};
#[cfg(not(stm32f7_pll216))]
pub const ENTROPY: super::EntropyPosture = super::EntropyPosture::AbsentByPosture {
    name: "STM32F7 RNG",
    line: "hardware source STM32F7 RNG needs PLL48CLK, absent on this clock posture; cryptographic draws are refused, the software stream serves the rest",
};
/// Whether this row carries a real-time clock the kernel reads at boot and
/// writes at an explicit set.
#[allow(dead_code)]
pub const RTC_PRESENT: bool = false;
/// The calendar banner's text for the family's rows, constant per image:
/// both boards mount the 32.768 kHz crystal and carry no backup cell.
pub const RTC_LINE: &str = "RTC block present on this part, a mounted 32.768 kHz crystal, no backup cell; this row does not read the clock: the base is the epoch";
#[cfg(not(stm32f7_pll216))]
pub use super::absent::{entropy_draw, entropy_init};
pub use super::absent::{rtc_read_at_boot, rtc_write};

/// The RNG block (the manuals' RNG chapters) at 0x5006_0800 on the AHB2
/// bus: three registers. RNG_CR: CED bit 5 (0 = the clock checker on; its
/// reset value, and it cannot change while the block is enabled), IE bit
/// 3, RNGEN bit 2; every other bit reserved on these parts (no conditioning
/// reset and no configuration fields here). RNG_SR: SEIS bit 6 and CEIS bit
/// 5 (the seed and clock error interrupt flags, cleared by writing 0), SECS
/// bit 2 and CECS bit 1 (the current seed and clock error status,
/// read-only), DRDY bit 0. RNG_DR: the 32-bit sample, valid when DRDY is set
/// and the value is not zero.
#[cfg(stm32f7_pll216)]
const RNG: usize = 0x5006_0800;
#[cfg(stm32f7_pll216)]
const RNG_CR: *mut u32 = RNG as *mut u32;
#[cfg(stm32f7_pll216)]
const RNG_SR: *mut u32 = (RNG + 0x04) as *mut u32;
#[cfg(stm32f7_pll216)]
const RNG_DR: *mut u32 = (RNG + 0x08) as *mut u32;
#[cfg(stm32f7_pll216)]
const RNG_CR_RNGEN: u32 = 1 << 2;
#[cfg(stm32f7_pll216)]
const RNG_SR_SEIS: u32 = 1 << 6;
#[cfg(stm32f7_pll216)]
const RNG_SR_CEIS: u32 = 1 << 5;
#[cfg(stm32f7_pll216)]
const RNG_SR_SECS: u32 = 1 << 2;
#[cfg(stm32f7_pll216)]
const RNG_SR_CECS: u32 = 1 << 1;
#[cfg(stm32f7_pll216)]
const RNG_SR_DRDY: u32 = 1 << 0;
#[cfg(stm32f7_pll216)]
const RNG_SR_ERRORS: u32 = RNG_SR_SEIS | RNG_SR_CEIS | RNG_SR_SECS | RNG_SR_CECS;
/// The block's bus clock (RCC_AHB2ENR, RNGEN bit 6) and the 48 MHz clock's
/// source select (RCC_DCKCFGR2, CK48MSEL bit 27: 0 = the main PLL's Q output,
/// the reset value, which this posture forms; 1 = the PLLSAI's, which it
/// does not form).
#[cfg(stm32f7_pll216)]
const RCC_AHB2ENR: *mut u32 = (RCC + 0x34) as *mut u32;
#[cfg(stm32f7_pll216)]
const AHB2ENR_RNGEN: u32 = 1 << 6;
#[cfg(stm32f7_pll216)]
const RCC_DCKCFGR2: *mut u32 = (RCC + 0x90) as *mut u32;
#[cfg(stm32f7_pll216)]
const DCKCFGR2_CK48MSEL: u32 = 1 << 27;
/// A bounded wait's iteration count for at least two milliseconds: the loop
/// body is at least four cycles, so the count is the cycles of two
/// milliseconds over four -- at the boot the tick clock is not yet running,
/// so every wait here is an iteration bound, never a clock. The block
/// delivers a word every 42 of its 48 MHz clocks and its first word 46
/// clocks after the enable, three orders of magnitude inside the bound: the
/// bound is a fault detector, never a wait.
#[cfg(stm32f7_pll216)]
const SPIN_2MS: u32 = SYSCLK_HZ / 500 / 4;

/// The block started on the PLL216 posture: the bus clock enabled and read
/// back, the 48 MHz source select read (the main PLL's Q output, or the
/// source is absent), the block enabled with the clock checker on, the first
/// ready word waited for under the bound, the status read. False when the
/// checker or the noise source flagged, or no word came (the witness then
/// reads the source absent and every secure draw refuses). With the checker
/// on, the block flags a clock error whenever its clock is slower than a
/// sixteenth of the bus clock; 48 MHz against 216 / 16 = 13.5 MHz clears it.
/// Nothing here masks an interrupt: the boot path runs before the scheduler.
#[cfg(stm32f7_pll216)]
pub fn entropy_init() -> bool {
    unsafe {
        write_volatile(RCC_AHB2ENR, read_volatile(RCC_AHB2ENR) | AHB2ENR_RNGEN);
        let _ = read_volatile(RCC_AHB2ENR);
        if read_volatile(RCC_DCKCFGR2) & DCKCFGR2_CK48MSEL != 0 {
            return false;
        }
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
#[cfg(stm32f7_pll216)]
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
