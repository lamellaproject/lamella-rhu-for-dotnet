//! The ST timers' PWM backend: the chip's timer blocks behind the PWM
//! functions, in the shape of the emulator rows' virtual timers and of the
//! RP2350's slices. One file serves the STM32F7 and the STM32U5: the
//! fields used are the same register file on both (`crate::stm32_tim_math`
//! states the offsets and the manuals' chapters), and a board's facts
//! state each block's base, its clock enable and its kind. A timer counts
//! its clock over the prescaler from 0 to the auto-reload value and wraps;
//! a channel in PWM mode 1 is active while the counter is below its
//! compare register, so a compare of 0 is a toggle-free 0 percent and a
//! compare one past the auto-reload value a toggle-free 100 percent. The
//! auto-reload and compare registers are preloaded: a compare written
//! while the counter runs is taken at the next period's start, and a
//! period change stops the counter, writes the prescaler, the period and
//! the compare, loads them by one update and restarts from 0 (a timer's
//! other channels are never held on a different period: the functions
//! refuse that first). A claimed channel that is not driven is FORCED
//! INACTIVE with its output enabled, so its pad stands at the polarity's
//! idle level and never floats; an advanced timer's outputs need its main
//! output enable, set once at the block's first use. A line reaches its
//! channel through the pad's alternate function (the board's facts state
//! the number), push-pull without a pull at the port's high speed setting,
//! as the bus backend sets a driven bus pad. A block's clock is enabled at
//! its first claim and read back. No interrupt; nothing masks. The record
//! `__rhu_stm32_tim_pwm` is read by symbol.

use core::ptr::{addr_of_mut, read_volatile, write_volatile};
use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use crate::board::facts;
use crate::board::gpio_backend as pins;
use crate::board::stm32_bus as pads;
use crate::critical::with_cs;
use crate::gpio_math as g;
use crate::pwm_math;
use crate::stm32_tim_math as m;

const NT: usize = facts::PWM_TIMERS.len();
const _: () = assert!(facts::PWM_BLOCKS.len() == NT, "the row's block table and its timer table are not of one length");
const _: () = assert!(m::blocks_sound(facts::PWM_BLOCKS), "the row's timer blocks are not sound: a bus that is neither APB1 nor APB2, no channel or more than four, an address that is not a word's, or two blocks of one number or base");
const _: () = assert!(m::lines_fit(facts::PWM_LINES, facts::PWM_BLOCKS), "a PWM line names a timer outside the block table or a channel its block does not have");

/// The pad's output speed (the port's OSPEEDR field): the high setting.
const SPEED_HIGH: u32 = 0b10;

/// The timers' shadows: the pair each is to run (a prescaler of 0 = none).
static PAIR_P: [AtomicU32; NT] = [const { AtomicU32::new(0) }; NT];
static PAIR_A: [AtomicU32; NT] = [const { AtomicU32::new(0) }; NT];
/// Per timer: its clock enabled and its control register written.
static UP: [AtomicBool; NT] = [const { AtomicBool::new(false) }; NT];

/// The backend's record, by symbol: the timers brought up (one bit per
/// table position), the counters restarted on a period change, the
/// compares written to a running counter, and the F7's timer prescaler
/// selector as it was found at the first bring-up (0 at its reset value,
/// 1 set, 2 the row states no selector).
#[repr(C)]
pub struct TimRecord {
    pub up: u32,
    pub restarts: u32,
    pub live: u32,
    pub selector: u32,
}

#[unsafe(export_name = "__rhu_stm32_tim_pwm")]
pub static mut RECORD: TimRecord = TimRecord { up: 0, restarts: 0, live: 0, selector: 2 };

unsafe fn bump(p: *mut u32) {
    unsafe { write_volatile(p, read_volatile(p).wrapping_add(1)) }
}

fn reg(timer: usize, off: usize) -> *mut u32 {
    (facts::PWM_BLOCKS[timer].base + off) as *mut u32
}

unsafe fn rmw(r: *mut u32, clear: u32, set: u32) {
    unsafe { write_volatile(r, (read_volatile(r) & !clear) | set) }
}

/// A block up once per boot: its bus clock enabled and read back, its
/// control register written whole (the counter stopped, the auto-reload
/// preload on, counting up, edge-aligned), and on an advanced timer the
/// main output enabled.
fn bring_up(timer: usize) {
    if UP[timer].swap(true, Ordering::AcqRel) {
        return;
    }
    let b = &facts::PWM_BLOCKS[timer];
    with_cs(|| unsafe {
        let en = b.enable_reg as *mut u32;
        rmw(en, 0, 1 << b.enable_bit);
        let _ = read_volatile(en);
        write_volatile(reg(timer, m::CR1), m::CR1_ARPE);
        if b.advanced {
            rmw(reg(timer, m::BDTR), 0, m::BDTR_MOE);
        }
        let r = addr_of_mut!(RECORD);
        write_volatile(addr_of_mut!((*r).up), read_volatile(addr_of_mut!((*r).up)) | (1 << timer));
        if let Some((selector, bit)) = facts::PWM_SELECTOR {
            write_volatile(addr_of_mut!((*r).selector), (read_volatile(selector as *const u32) >> bit) & 1);
        }
    });
}

/// The timer prescaler selector as the first bring-up found it (the
/// record's word): the table's clocks hold while it reads 0, or 2 on a row
/// that states none.
#[allow(dead_code)]
pub fn probe_selector() -> u32 {
    unsafe { read_volatile(addr_of_mut!((*addr_of_mut!(RECORD)).selector)) }
}

/// The line's channel and pad: the channel an output with its compare
/// preload on, forced inactive at the normal polarity with a compare of 0,
/// its output enabled; then the pad handed to the block -- the pad shows the
/// idle level, low, until a set and an enable.
pub fn configure(index: usize) {
    let l = &facts::PWM_LINES[index];
    let (t, ch) = (l.timer as usize, l.channel as u32);
    bring_up(t);
    with_cs(|| unsafe {
        rmw(reg(t, m::ccmr(ch).0), m::select_mask(ch) | m::mode_mask(ch) | m::mode_high_bit(ch), m::preload_bit(ch) | m::mode_bits(ch, m::MODE_FORCED_INACTIVE));
        write_volatile(reg(t, m::ccr(ch)), 0);
        rmw(reg(t, m::CCER), m::polarity_bit(ch), m::enable_bit(ch));
    });
    pads::pad_take_as(l.line as u32, l.alternate, false, g::PULL_NONE, SPEED_HIGH);
}

/// The pad back to a plain input without a pull (the GPIO backend's
/// released shape), then the channel's output disabled and its mode,
/// preload and compare back at their reset values.
pub fn release(index: usize) {
    let l = &facts::PWM_LINES[index];
    let (t, ch) = (l.timer as usize, l.channel as u32);
    pins::release(l.line as u32);
    with_cs(|| unsafe {
        rmw(reg(t, m::CCER), m::enable_bit(ch) | m::polarity_bit(ch), 0);
        rmw(reg(t, m::ccmr(ch).0), m::mode_mask(ch) | m::preload_bit(ch), 0);
        write_volatile(reg(t, m::ccr(ch)), 0);
    });
}

/// The pair the timer is to run, recorded; the registers are written at
/// the channel's set that follows.
pub fn set_timer(timer: usize, prescaler: u32, period: u32) {
    PAIR_P[timer].store(prescaler, Ordering::Release);
    PAIR_A[timer].store(period, Ordering::Release);
}

/// The counter stopped and cleared (no held channel has a period).
pub fn clear_timer(timer: usize) {
    PAIR_P[timer].store(0, Ordering::Release);
    PAIR_A[timer].store(0, Ordering::Release);
    with_cs(|| unsafe {
        rmw(reg(timer, m::CR1), m::CR1_CEN, 0);
        write_volatile(reg(timer, m::CNT), 0);
    });
}

/// The channel's compare and polarity. The polarity bit is written at once
/// (it is not preloaded). On a counter already running the recorded pair
/// the compare alone is written, taken at the next period's start;
/// otherwise the counter is stopped, the prescaler and the auto-reload
/// register written as the pair's values less one, the compare written, the
/// three loaded by one update -- which restarts the counter from 0 -- and
/// the counter started.
pub fn set_channel(index: usize, compare: u32, polarity: u32) {
    let l = &facts::PWM_LINES[index];
    let (t, ch) = (l.timer as usize, l.channel as u32);
    let (p, a) = (PAIR_P[t].load(Ordering::Acquire), PAIR_A[t].load(Ordering::Acquire));
    if p == 0 || a == 0 {
        return;
    }
    with_cs(|| unsafe {
        let live = read_volatile(reg(t, m::CR1)) & m::CR1_CEN != 0 && read_volatile(reg(t, m::PSC)) == p - 1 && read_volatile(reg(t, m::ARR)) == a - 1;
        rmw(reg(t, m::CCER), m::polarity_bit(ch), if polarity == pwm_math::POLARITY_INVERSED { m::polarity_bit(ch) } else { 0 });
        let r = addr_of_mut!(RECORD);
        if live {
            write_volatile(reg(t, m::ccr(ch)), compare);
            bump(addr_of_mut!((*r).live));
        } else {
            rmw(reg(t, m::CR1), m::CR1_CEN, 0);
            write_volatile(reg(t, m::PSC), p - 1);
            write_volatile(reg(t, m::ARR), a - 1);
            write_volatile(reg(t, m::ccr(ch)), compare);
            write_volatile(reg(t, m::EGR), m::EGR_UG);
            rmw(reg(t, m::CR1), 0, m::CR1_CEN);
            bump(addr_of_mut!((*r).restarts));
        }
    });
}

/// The output driven (PWM mode 1) or parked (forced inactive: the
/// polarity's idle level, low at the normal polarity and high at the
/// inversed one).
pub fn enable(index: usize, on: bool) {
    let l = &facts::PWM_LINES[index];
    let (t, ch) = (l.timer as usize, l.channel as u32);
    with_cs(|| unsafe { rmw(reg(t, m::ccmr(ch).0), m::mode_mask(ch), m::mode_bits(ch, if on { m::MODE_PWM1 } else { m::MODE_FORCED_INACTIVE })) });
}

pub fn park(index: usize) {
    enable(index, false);
}

/// The period the timer runs as its registers state it: the prescaler
/// register plus one times the auto-reload register plus one ticks of the
/// timer's clock, in nanoseconds (the flight check's tie of the function's
/// answer to the block). The doctored control reads the prescaler one step
/// long.
#[allow(dead_code)]
pub fn probe_period_ns(timer: usize) -> u64 {
    let (psc, arr) = unsafe { (read_volatile(reg(timer, m::PSC)), read_volatile(reg(timer, m::ARR))) };
    pwm_math::achieved_ns(psc + 1 + cfg!(feature = "test-pwm-doctored") as u32, arr + 1, facts::PWM_TIMERS[timer].clock_hz)
}
