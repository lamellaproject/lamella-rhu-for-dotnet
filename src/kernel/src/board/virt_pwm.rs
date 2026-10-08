//! The virtual PWM backend of the emulator rows: two timers of two channels
//! in kernel memory, so the doors, the claims of the line and the channel,
//! the quantization, the shared-period rule and the refusals are gated on
//! QEMU by the same verbs the boards serve, and only the pads are left to
//! silicon. Each timer keeps a prescaler and a period register in the
//! shape the ST timers use (the register holds the value minus one) and
//! each channel its compare, its polarity and its enable; the timer's own
//! reading of the period it runs, from those registers, is what the api
//! flavor's check ties the door's answer to (on silicon the same verb
//! reads the block back). The doctored control `test-pwm-doctored` makes
//! that reading run the prescaler one step long, so the door's arithmetic
//! and the timer's disagree and the check must refuse.

use core::sync::atomic::{AtomicU32, Ordering};

use crate::board::facts;
use crate::pwm_math;

const NT: usize = facts::PWM_TIMERS.len();
const NL: usize = facts::PWM_LINES.len();

/// The timers' registers: the prescaler and the period, each the value
/// minus one.
static PSC: [AtomicU32; NT] = [const { AtomicU32::new(0) }; NT];
static ARR: [AtomicU32; NT] = [const { AtomicU32::new(0) }; NT];
/// The channels' registers: the compare, the polarity and the enable.
static CCR: [AtomicU32; NL] = [const { AtomicU32::new(0) }; NL];
static POL: [AtomicU32; NL] = [const { AtomicU32::new(0) }; NL];
static EN: [AtomicU32; NL] = [const { AtomicU32::new(0) }; NL];

/// The pad seam is empty: the models have no pins.
pub fn configure(_index: usize) {}

pub fn release(index: usize) {
    CCR[index].store(0, Ordering::Release);
    POL[index].store(0, Ordering::Release);
    EN[index].store(0, Ordering::Release);
}

/// The timer's pair programmed as the registers hold it.
pub fn set_timer(timer: usize, prescaler: u32, period: u32) {
    PSC[timer].store(prescaler - 1, Ordering::Release);
    ARR[timer].store(period - 1, Ordering::Release);
}

pub fn clear_timer(timer: usize) {
    PSC[timer].store(0, Ordering::Release);
    ARR[timer].store(0, Ordering::Release);
}

pub fn set_channel(index: usize, compare: u32, polarity: u32) {
    CCR[index].store(compare, Ordering::Release);
    POL[index].store(polarity, Ordering::Release);
}

pub fn enable(index: usize, on: bool) {
    EN[index].store(on as u32, Ordering::Release);
}

pub fn park(index: usize) {
    EN[index].store(0, Ordering::Release);
}

/// The period the timer runs as its registers state it: (PSC + 1) x
/// (ARR + 1) ticks of its clock, in nanoseconds (the doctored control's
/// reading runs the prescaler one step long).
#[allow(dead_code)]
pub fn probe_period_ns(timer: usize) -> u64 {
    let p = PSC[timer].load(Ordering::Acquire) + 1 + cfg!(feature = "test-pwm-doctored") as u32;
    let a = ARR[timer].load(Ordering::Acquire) + 1;
    pwm_math::achieved_ns(p, a, facts::PWM_TIMERS[timer].clock_hz)
}

/// What a line's output does, for the ADC model's loop channel: the
/// compare, the period in ticks, whether the output is driven, and the
/// polarity.
pub fn output(index: usize) -> (u32, u32, bool, u32) {
    let t = facts::PWM_LINES[index].timer as usize;
    (CCR[index].load(Ordering::Acquire), ARR[t].load(Ordering::Acquire) + 1, EN[index].load(Ordering::Acquire) != 0, POL[index].load(Ordering::Acquire))
}
