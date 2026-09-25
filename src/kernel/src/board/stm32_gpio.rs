//! The GPIO family driver for ST's Cortex-M parts. The port block's layout
//! is what the reference manuals state: RM0410 (the STM32F76x/F77x) and
//! RM0385 (the STM32F74x/F75x), 6.4.1 to 6.4.10 in both; RM0432 (the
//! STM32L4+), 8.4.1 to 8.4.11; RM0456 (the STM32U5), 13.4.1 to 13.4.11. A
//! board's row names its arm; a part whose manual states another layout
//! gets its own, never one of these by implication. The routing arms: the
//! classic arm (the F7 and the L4+) selects a line's port in SYSCFG and its
//! EXTI block feeds vector positions shared by groups of line numbers, both
//! blocks at the addresses the board's facts state; the U5's EXTI block
//! (RM0456 chapter 23) selects the port itself, keeps rising and falling
//! pending bits apart and has one vector position per line. The board's
//! facts state the block's base, the port count and the clock-enable
//! register; the mode is programmed explicitly on every family; no vendor
//! header is used. The handlers do bounded work in the board band: read
//! the pending bits, clear them, fire the owning line's event (the counter
//! and the wake through the scheduler's critical-section path), return.

use core::ptr::{addr_of, addr_of_mut, read_volatile, write_volatile};

use crate::board::facts;
use crate::claims;
use crate::critical::with_cs;
use crate::gpio;
use crate::gpio_math as m;

pub const HAS_EVENTS: bool = true;

/// The port block's registers (the same offsets on both families).
const MODER: usize = 0x00;
const OTYPER: usize = 0x04;
const PUPDR: usize = 0x0C;
const IDR: usize = 0x10;
const BSRR: usize = 0x18;

fn reg(port: u32, off: usize) -> *mut u32 {
    (facts::GPIO_BASE + port as usize * 0x400 + off) as *mut u32
}

unsafe fn rmw(r: *mut u32, clear: u32, set: u32) {
    unsafe { write_volatile(r, (read_volatile(r) & !clear) | set) }
}

/// The port's clock enable in the family's RCC register (a bit per port),
/// read back so the enable takes effect before the port is touched.
fn enable_port_clock(port: u32) {
    with_cs(|| unsafe {
        rmw(facts::RCC_GPIO_EN, 0, 1 << port);
        let _ = read_volatile(facts::RCC_GPIO_EN);
    });
}

pub fn configure(line: u32, mode: u32, pull: u32, initial: bool) {
    let (port, bit) = m::st_split(line);
    enable_port_clock(port);
    let moder = if mode == m::MODE_INPUT { 0b00 } else { 0b01 };
    let od = (mode == m::MODE_OUTPUT_OPEN_DRAIN) as u32;
    with_cs(|| unsafe {
        rmw(reg(port, PUPDR), 0b11 << (2 * bit), pull << (2 * bit));
        rmw(reg(port, OTYPER), 1 << bit, od << bit);
        if mode != m::MODE_INPUT {
            write_volatile(reg(port, BSRR), if initial { 1 << bit } else { 1 << (bit + 16) });
        }
        rmw(reg(port, MODER), 0b11 << (2 * bit), moder << (2 * bit));
    });
}

/// Back to a floating input.
pub fn release(line: u32) {
    let (port, bit) = m::st_split(line);
    with_cs(|| unsafe {
        rmw(reg(port, MODER), 0b11 << (2 * bit), 0);
        rmw(reg(port, PUPDR), 0b11 << (2 * bit), 0);
        rmw(reg(port, OTYPER), 1 << bit, 0);
    });
}

/// The atomic path: BSRR's set half (bits 15:0) or reset half (bits
/// 31:16), one write, no read-modify-write. An open-drain output's "high"
/// is the pad released, by the output type.
pub fn write(line: u32, _mode: u32, high: bool) {
    let (port, bit) = m::st_split(line);
    unsafe { write_volatile(reg(port, BSRR), if high { 1 << bit } else { 1 << (bit + 16) }) }
}

/// The pad, from the input data register (an output reads back what the
/// pad sees).
pub fn read(line: u32) -> bool {
    let (port, bit) = m::st_split(line);
    unsafe { read_volatile(reg(port, IDR)) & (1 << bit) != 0 }
}

/// The NVIC: the priority byte first, then the set-enable bit (a
/// write-one register: the single bit, no read-modify-write). The board
/// band 0xE0: masked inside a kernel critical section, below the tick,
/// above the context switch -- the console's band.
const NVIC_ISER: usize = 0xE000_E100;
const NVIC_IPR: usize = 0xE000_E400;
const EXTI_PRIO: u8 = 0xE0;

fn nvic_enable(line: usize) {
    unsafe {
        write_volatile((NVIC_IPR + line) as *mut u8, EXTI_PRIO);
        write_volatile((NVIC_ISER + 4 * (line / 32)) as *mut u32, 1 << (line % 32));
    }
}

/// The classic arm (the F7 family and the L4+): SYSCFG selects the port of
/// each of the sixteen lines (EXTICR1..4, four bits per line, 0000 PA up the
/// alphabet; RM0410 7.2.3, RM0432 9.2.3); the EXTI block (IMR 0x00, RTSR 0x08,
/// FTSR 0x0C, SWIER 0x10, PR 0x14, a pending bit cleared by writing 1; RM0410
/// 11.9, RM0432 chapter 16) feeds the vector positions 6 to 10 for lines 0 to
/// 4, 23 for lines 5 to 9, 40 for lines 10 to 15; the addresses the facts'.
#[cfg(any(feature = "board-st-nucleo-l4r5zi", feature = "board-st-stm32f746g-disco", feature = "board-st-stm32f769i-disco"))]
mod exti {
    use core::sync::atomic::{AtomicU8, Ordering};

    use super::*;

    const RCC_SYSCFG_EN: *mut u32 = facts::RCC_SYSCFG_EN;
    const RCC_SYSCFG_EN_BIT: u32 = facts::RCC_SYSCFG_EN_BIT;
    const SYSCFG_EXTICR1: usize = facts::SYSCFG_EXTICR1;
    const EXTI: usize = facts::EXTI_BASE;
    const EXTI_IMR: *mut u32 = EXTI as *mut u32;
    const EXTI_RTSR: *mut u32 = (EXTI + 0x08) as *mut u32;
    const EXTI_FTSR: *mut u32 = (EXTI + 0x0C) as *mut u32;
    const EXTI_SWIER: *mut u32 = (EXTI + 0x10) as *mut u32;
    const EXTI_PR: *mut u32 = (EXTI + 0x14) as *mut u32;

    pub fn route(line: u32, rising: bool, falling: bool) -> i32 {
        let (port, bit) = m::st_split(line);
        let r = with_cs(|| unsafe { claims::irq_slot_claim(&mut *addr_of_mut!(gpio::CLAIMS), bit, line) });
        if r != 0 {
            return r;
        }
        with_cs(|| unsafe {
            rmw(RCC_SYSCFG_EN, 0, RCC_SYSCFG_EN_BIT);
            let _ = read_volatile(RCC_SYSCFG_EN);
            let cr = (SYSCFG_EXTICR1 + 4 * (bit as usize / 4)) as *mut u32;
            rmw(cr, 0xF << (4 * (bit % 4)), port << (4 * (bit % 4)));
            rmw(EXTI_RTSR, 1 << bit, (rising as u32) << bit);
            rmw(EXTI_FTSR, 1 << bit, (falling as u32) << bit);
            ARMED[bit as usize].store((rising as u8) | ((falling as u8) << 1), Ordering::Release);
            write_volatile(EXTI_PR, 1 << bit);
            rmw(EXTI_IMR, 0, 1 << bit);
        });
        nvic_enable(m::st_f7_nvic(bit));
        0
    }

    pub fn unroute(line: u32) {
        let (_, bit) = m::st_split(line);
        with_cs(|| unsafe {
            rmw(EXTI_IMR, 1 << bit, 0);
            ARMED[bit as usize].store(0, Ordering::Release);
            rmw(EXTI_RTSR, 1 << bit, 0);
            rmw(EXTI_FTSR, 1 << bit, 0);
            write_volatile(EXTI_PR, 1 << bit);
            claims::irq_slot_release(&mut *addr_of_mut!(gpio::CLAIMS), bit, line);
        });
    }

    /// The software trigger: one pending edge on the line without a finger.
    pub fn software_edge(line: u32) -> bool {
        let (_, bit) = m::st_split(line);
        unsafe { write_volatile(EXTI_SWIER, 1 << bit) }
        true
    }

    /// The kinds armed per EXTI line (bit 0 rising, bit 1 falling), recorded
    /// at the routing: one pending bit per line, not per edge, so the
    /// dispatch reports the armed kind when one alone is armed and reads the
    /// pad's level after the edge only when both are (a pulse shorter than
    /// the handler's latency, or the software trigger at either level, is
    /// then the edge that was armed).
    static ARMED: [AtomicU8; 16] = [const { AtomicU8::new(0) }; 16];

    unsafe fn dispatch(mask: u32) {
        unsafe {
            let pr = read_volatile(EXTI_PR) & mask;
            if pr == 0 {
                return;
            }
            if cfg!(not(feature = "test-gpio-doctored-mask")) {
                write_volatile(EXTI_PR, pr);
            }
            let mut p = pr;
            while p != 0 {
                let bit = p.trailing_zeros();
                p &= p - 1;
                if let Some(owner) = claims::irq_slot_owner(&*addr_of!(gpio::CLAIMS), bit) {
                    let rising = match ARMED[bit as usize].load(Ordering::Acquire) {
                        1 => true,
                        2 => false,
                        _ => read(owner),
                    };
                    gpio::fire(owner, rising);
                }
            }
        }
    }

    pub unsafe extern "C" fn exti0() {
        unsafe { dispatch(1 << 0) }
    }
    pub unsafe extern "C" fn exti1() {
        unsafe { dispatch(1 << 1) }
    }
    pub unsafe extern "C" fn exti2() {
        unsafe { dispatch(1 << 2) }
    }
    pub unsafe extern "C" fn exti3() {
        unsafe { dispatch(1 << 3) }
    }
    pub unsafe extern "C" fn exti4() {
        unsafe { dispatch(1 << 4) }
    }
    pub unsafe extern "C" fn exti9_5() {
        unsafe { dispatch(0x3E0) }
    }
    pub unsafe extern "C" fn exti15_10() {
        unsafe { dispatch(0xFC00) }
    }

    /// The handlers the board file re-exports as `GPIO_IRQS`, folded into
    /// the vector table beside `BOARD_IRQS`.
    pub const IRQS: [(usize, unsafe extern "C" fn()); 7] = [
        (6, exti0),
        (7, exti1),
        (8, exti2),
        (9, exti3),
        (10, exti4),
        (23, exti9_5),
        (40, exti15_10),
    ];
}

/// The U5 arm: the EXTI block at 0x4602_2000 (the non-secure alias, AHB3)
/// carries the port selection itself (EXTICR1..4 at 0x060 to 0x06C, EIGHT
/// bits per line, 0x00 PA to 0x09 PJ), separate rising and falling pending
/// registers (RPR1 0x00C, FPR1 0x010, each bit cleared by writing 1), the
/// triggers (RTSR1 0x000, FTSR1 0x004), the software trigger (SWIER1 0x008,
/// which sets the rising pending bit) and the mask (IMR1 0x080) -- RM0456
/// chapter 23; one vector position per line, EXTI0 to EXTI15 at 11 to 26.
#[cfg(feature = "board-st-nucleo-u5a5zj-q")]
mod exti {
    use super::*;

    const EXTI: usize = 0x4602_2000;
    const EXTI_RTSR1: *mut u32 = EXTI as *mut u32;
    const EXTI_FTSR1: *mut u32 = (EXTI + 0x004) as *mut u32;
    const EXTI_SWIER1: *mut u32 = (EXTI + 0x008) as *mut u32;
    const EXTI_RPR1: *mut u32 = (EXTI + 0x00C) as *mut u32;
    const EXTI_FPR1: *mut u32 = (EXTI + 0x010) as *mut u32;
    const EXTI_EXTICR1: usize = EXTI + 0x060;
    const EXTI_IMR1: *mut u32 = (EXTI + 0x080) as *mut u32;

    pub fn route(line: u32, rising: bool, falling: bool) -> i32 {
        let (port, bit) = m::st_split(line);
        let r = with_cs(|| unsafe { claims::irq_slot_claim(&mut *addr_of_mut!(gpio::CLAIMS), bit, line) });
        if r != 0 {
            return r;
        }
        with_cs(|| unsafe {
            let cr = (EXTI_EXTICR1 + 4 * (bit as usize / 4)) as *mut u32;
            rmw(cr, 0xFF << (8 * (bit % 4)), port << (8 * (bit % 4)));
            rmw(EXTI_RTSR1, 1 << bit, (rising as u32) << bit);
            rmw(EXTI_FTSR1, 1 << bit, (falling as u32) << bit);
            write_volatile(EXTI_RPR1, 1 << bit);
            write_volatile(EXTI_FPR1, 1 << bit);
            rmw(EXTI_IMR1, 0, 1 << bit);
        });
        nvic_enable(m::st_u5_nvic(bit));
        0
    }

    pub fn unroute(line: u32) {
        let (_, bit) = m::st_split(line);
        with_cs(|| unsafe {
            rmw(EXTI_IMR1, 1 << bit, 0);
            rmw(EXTI_RTSR1, 1 << bit, 0);
            rmw(EXTI_FTSR1, 1 << bit, 0);
            write_volatile(EXTI_RPR1, 1 << bit);
            write_volatile(EXTI_FPR1, 1 << bit);
            claims::irq_slot_release(&mut *addr_of_mut!(gpio::CLAIMS), bit, line);
        });
    }

    /// The software trigger sets the rising pending bit (RM0456 23.6.4).
    pub fn software_edge(line: u32) -> bool {
        let (_, bit) = m::st_split(line);
        unsafe { write_volatile(EXTI_SWIER1, 1 << bit) }
        true
    }

    /// The two pending registers say which edge; both are read and cleared.
    unsafe fn dispatch(bit: u32) {
        unsafe {
            let rp = read_volatile(EXTI_RPR1) & (1 << bit);
            let fp = read_volatile(EXTI_FPR1) & (1 << bit);
            if cfg!(not(feature = "test-gpio-doctored-mask")) {
                if rp != 0 {
                    write_volatile(EXTI_RPR1, rp);
                }
                if fp != 0 {
                    write_volatile(EXTI_FPR1, fp);
                }
            }
            if rp == 0 && fp == 0 {
                return;
            }
            if let Some(owner) = claims::irq_slot_owner(&*addr_of!(gpio::CLAIMS), bit) {
                if rp != 0 {
                    gpio::fire(owner, true);
                }
                if fp != 0 {
                    gpio::fire(owner, false);
                }
            }
        }
    }

    macro_rules! handler {
        ($name:ident, $bit:expr) => {
            pub unsafe extern "C" fn $name() {
                unsafe { dispatch($bit) }
            }
        };
    }
    handler!(exti0, 0);
    handler!(exti1, 1);
    handler!(exti2, 2);
    handler!(exti3, 3);
    handler!(exti4, 4);
    handler!(exti5, 5);
    handler!(exti6, 6);
    handler!(exti7, 7);
    handler!(exti8, 8);
    handler!(exti9, 9);
    handler!(exti10, 10);
    handler!(exti11, 11);
    handler!(exti12, 12);
    handler!(exti13, 13);
    handler!(exti14, 14);
    handler!(exti15, 15);

    /// The handlers the board file re-exports as `GPIO_IRQS`, folded into
    /// the vector table beside `BOARD_IRQS`.
    pub const IRQS: [(usize, unsafe extern "C" fn()); 16] = [
        (11, exti0),
        (12, exti1),
        (13, exti2),
        (14, exti3),
        (15, exti4),
        (16, exti5),
        (17, exti6),
        (18, exti7),
        (19, exti8),
        (20, exti9),
        (21, exti10),
        (22, exti11),
        (23, exti12),
        (24, exti13),
        (25, exti14),
        (26, exti15),
    ];
}

pub use exti::IRQS;

pub fn arm(line: u32, rising: bool, falling: bool) -> i32 {
    exti::route(line, rising, falling)
}

pub fn disarm(line: u32) {
    exti::unroute(line)
}

pub fn software_edge(line: u32) -> bool {
    exti::software_edge(line)
}
