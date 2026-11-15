//! The ST timers' arithmetic and facts shapes, pure: the timer block as a
//! board's facts state it, the rule that gives a timer its clock, the
//! registers' offsets and the fields of a channel, and the soundness of a
//! row's block and line tables. The numbers are the reference manuals':
//! RM0410 (the STM32F76x/F77x: the advanced timers' chapter 25, the
//! general-purpose timers' chapters 26 and 27) and RM0456 (the STM32U5:
//! chapters 54 and 55), which state one register file for the fields used
//! here -- the control register at 0x00, the event generation register at
//! 0x14, the two capture/compare mode registers at 0x18 and 0x1C, the
//! capture/compare enable register at 0x20, the counter at 0x24, the
//! prescaler at 0x28, the auto-reload register at 0x2C, the four
//! capture/compare registers from 0x34 and, on an advanced timer, the
//! break and dead-time register at 0x44. A timer's clock follows its bus
//! (RM0410 5.2, RM0456 11.4.15): the bus clock when the bus prescaler is 1,
//! twice the bus clock otherwise; on the F7 that holds while the selector
//! TIMPRE of RCC_DCKCFGR1 stays at its reset value. No hardware access and
//! no state, covered by the host tests.
#![allow(dead_code)] // the host tier compiles the module whole; a row reads what it needs

use crate::pwm_math::{PwmLine, PwmTimer};

/// A timer block as a board's facts state it: the chip's own number (TIM1
/// is 1), the block's base, the clock enable of its bus (register and
/// bit), the bus (APB1 is 1, APB2 is 2), whether it is an advanced timer --
/// its outputs then wait for the main output enable -- and how many of its
/// channels have an output.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TimBlock {
    pub number: u32,
    pub base: usize,
    pub enable_reg: usize,
    pub enable_bit: u32,
    pub apb: u8,
    pub advanced: bool,
    pub channels: u32,
}

/// TIMx_CR1, the control register, at 0x00.
pub const CR1: usize = 0x00;
/// TIMx_EGR, the event generation register, at 0x14.
pub const EGR: usize = 0x14;
/// TIMx_CCMR1 and TIMx_CCMR2, the mode registers of channels 1 and 2 and of
/// channels 3 and 4, at 0x18 and 0x1C.
pub const CCMR1: usize = 0x18;
pub const CCMR2: usize = 0x1C;
/// TIMx_CCER, the capture/compare enable register, at 0x20.
pub const CCER: usize = 0x20;
/// TIMx_CNT, the counter, at 0x24.
pub const CNT: usize = 0x24;
/// TIMx_PSC, the prescaler, at 0x28: the counter's clock is the timer's over
/// the register's value plus one.
pub const PSC: usize = 0x28;
/// TIMx_ARR, the auto-reload register, at 0x2C: the period is the
/// register's value plus one.
pub const ARR: usize = 0x2C;
/// TIMx_CCR1, the first capture/compare register, at 0x34; the others
/// follow at a stride of 4.
pub const CCR1: usize = 0x34;
/// TIMx_BDTR, an advanced timer's break and dead-time register, at 0x44.
pub const BDTR: usize = 0x44;

/// TIMx_CR1 CEN, bit 0: the counter counts.
pub const CR1_CEN: u32 = 1 << 0;
/// TIMx_CR1 ARPE, bit 7: the auto-reload register is preloaded.
pub const CR1_ARPE: u32 = 1 << 7;
/// TIMx_EGR UG, bit 0: an update, which loads the preloaded registers and
/// restarts the counter.
pub const EGR_UG: u32 = 1 << 0;
/// TIMx_BDTR MOE, bit 15: an advanced timer's main output enable.
pub const BDTR_MOE: u32 = 1 << 15;
/// The output compare modes used (the field OCxM): the output forced to
/// its inactive level, and PWM mode 1 -- active while the counter is below
/// the compare register.
pub const MODE_FORCED_INACTIVE: u32 = 0b100;
pub const MODE_PWM1: u32 = 0b110;

/// The reaches every timer is given in a row's table, as VALUES: the
/// prescaler register holds 16 bits (the value minus one), so its reach is
/// 65536; the period's reach is one less, so that a duty equal to the
/// period -- the output active for the whole period -- still fits the
/// 16-bit compare register as the auto-reload register plus one.
pub const REACH_PRESCALER: u32 = 65536;
pub const REACH_PERIOD: u32 = 65535;

/// A timer's clock from its bus clock and the clock the bus divides: the
/// bus clock when the two are equal (a bus prescaler of 1), twice the bus
/// clock otherwise.
pub const fn timer_clock_hz(pclk_hz: u32, hclk_hz: u32) -> u32 {
    if pclk_hz == hclk_hz {
        pclk_hz
    } else {
        2 * pclk_hz
    }
}

/// The mode register of a channel (0-based) and the shift of its group of
/// fields inside it: channels 1 and 3 at bit 0, channels 2 and 4 at bit 8.
pub const fn ccmr(channel: u32) -> (usize, u32) {
    (if channel < 2 { CCMR1 } else { CCMR2 }, 8 * (channel % 2))
}

/// The channel's selection field, CCxS, two bits: 00 makes it an output.
pub const fn select_mask(channel: u32) -> u32 {
    0b11 << ccmr(channel).1
}

/// The channel's compare preload enable, OCxPE: a compare written while the
/// counter runs is taken at the next update.
pub const fn preload_bit(channel: u32) -> u32 {
    1 << (ccmr(channel).1 + 3)
}

/// The channel's mode field, OCxM, its three low bits; and a mode's word in
/// that field.
pub const fn mode_mask(channel: u32) -> u32 {
    0b111 << (ccmr(channel).1 + 4)
}

pub const fn mode_bits(channel: u32, mode: u32) -> u32 {
    (mode & 0b111) << (ccmr(channel).1 + 4)
}

/// The mode field's fourth bit, apart from the others at bit 16 or 24: clear
/// in both modes used.
pub const fn mode_high_bit(channel: u32) -> u32 {
    1 << (16 + ccmr(channel).1)
}

/// The channel's output enable, CCxE, and its polarity, CCxP (set: the
/// output is active low), in the enable register.
pub const fn enable_bit(channel: u32) -> u32 {
    1 << (4 * channel)
}

pub const fn polarity_bit(channel: u32) -> u32 {
    1 << (4 * channel + 1)
}

/// The channel's capture/compare register.
pub const fn ccr(channel: u32) -> usize {
    CCR1 + 4 * channel as usize
}

/// The timer table's rows from a row's block table and the clock
/// configuration's bus clocks: each block's number, its clock by the rule,
/// its channels, and the two reaches. The length is the block table's.
pub const fn timers<const N: usize>(blocks: &[TimBlock], pclk1_hz: u32, pclk2_hz: u32, hclk_hz: u32) -> [PwmTimer; N] {
    assert!(blocks.len() == N, "the timer table is not as long as the block table");
    let mut t = [PwmTimer { number: 0, clock_hz: 0, channels: 0, prescaler_max: REACH_PRESCALER, period_max: REACH_PERIOD }; N];
    let mut i = 0;
    while i < N {
        let b = &blocks[i];
        t[i].number = b.number;
        t[i].clock_hz = timer_clock_hz(if b.apb == 1 { pclk1_hz } else { pclk2_hz }, hclk_hz);
        t[i].channels = b.channels;
        i += 1;
    }
    t
}

/// The block table's own invariants: a number, a word-aligned base and
/// enable register, an enable bit inside the register, a bus that is APB1
/// or APB2, one to four channels, and no two blocks of one number or one
/// base.
pub const fn blocks_sound(blocks: &[TimBlock]) -> bool {
    let mut i = 0;
    while i < blocks.len() {
        let b = &blocks[i];
        if b.number == 0 || b.base == 0 || b.base & 3 != 0 || b.enable_reg == 0 || b.enable_reg & 3 != 0 || b.enable_bit > 31 || (b.apb != 1 && b.apb != 2) || b.channels == 0 || b.channels > 4 {
            return false;
        }
        let mut j = i + 1;
        while j < blocks.len() {
            if blocks[j].number == b.number || blocks[j].base == b.base {
                return false;
            }
            j += 1;
        }
        i += 1;
    }
    true
}

/// The line table against the block table: every line's timer inside the
/// table and its channel one the block has.
pub const fn lines_fit(lines: &[PwmLine], blocks: &[TimBlock]) -> bool {
    let mut i = 0;
    while i < lines.len() {
        let l = &lines[i];
        if l.timer as usize >= blocks.len() || l.channel as u32 >= blocks[l.timer as usize].channels {
            return false;
        }
        i += 1;
    }
    true
}
