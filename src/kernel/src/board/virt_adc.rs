//! The virtual ADC backend of the emulator rows: two converter blocks in
//! kernel memory, so the doors, the claims of the line and the channel,
//! the width, the reference and the wait against a budget are gated on
//! QEMU by the same verbs the boards serve. A conversion is PACED by the
//! kernel's clock -- it completes after its block's cycles at the block's
//! clock, never earlier -- so the door's budget is proven against a real
//! wait. The channels read constants of the model: a pin channel a fixed
//! fraction of full scale, the internal reference channel a known voltage
//! against its block's reference, and the LOOP channel the first PWM line's
//! duty cycle as the fixture's resistor-capacitor network settles to it
//! (the compare over the period of full scale while the output is driven,
//! 0 while it is parked, the complement under the inversed polarity). The
//! doctored control `test-adc-doctored` makes the loop channel answer the
//! previous conversion's sample (a stale conversion), which the api
//! flavor's loop check must refuse at its second reading. Under
//! `test-adc-flagged` the model flags conversions as the boards' blocks
//! do -- the first conversion of one pin channel, every conversion of
//! another -- so the door's discard, retry and bounded refusal are proved
//! on the emulator rows; its doctored control `test-adc-flagged-doctored`
//! answers a flagged conversion's undefined result as a sample, which the
//! api flavor's check must refuse at its first reading.

use core::sync::atomic::{AtomicU32, AtomicU8, Ordering};

use crate::adc_math;
use crate::board::{facts, virt_pwm};
use crate::sched;

const DOCTORED: bool = cfg!(feature = "test-adc-doctored");
const FLAGGED: bool = cfg!(feature = "test-adc-flagged");
const FLAGGED_DOCTORED: bool = cfg!(feature = "test-adc-flagged-doctored");
const NC: usize = facts::ADC_CHANNELS.len();
/// Under `test-adc-flagged`: the pin channel whose first conversion the
/// model flags, and the one it flags on every conversion (both on the
/// first block; neither the loop nor the reference channel).
#[allow(dead_code)] // read by the api flavor's conversion-error check alone
pub const FLAGGED_ONCE: usize = 2;
#[allow(dead_code)] // read by the api flavor's conversion-error check alone
pub const FLAGGED_ALWAYS: usize = 3;
const _: () = assert!(FLAGGED_ONCE < NC && FLAGGED_ALWAYS < NC && FLAGGED_ONCE != FLAGGED_ALWAYS && FLAGGED_ONCE != facts::ADC_LOOP && FLAGGED_ALWAYS != facts::ADC_LOOP && FLAGGED_ONCE != facts::ADC_REFERENCE && FLAGGED_ALWAYS != facts::ADC_REFERENCE, "the flagged channels are two pin channels of the table");
/// The bits the doctored converter flips in a flagged conversion's result:
/// an in-range value that is not the channel's.
const FLAGGED_UNDEFINED: u32 = 0x2AA;

/// A pending conversion's start (the clock's low word: the elapsed time is
/// read modulo 2^32 ns, which no conversion approaches) and its flag; the
/// last sample each channel answered.
static START: [AtomicU32; NC] = [const { AtomicU32::new(0) }; NC];
static PENDING: [AtomicU8; NC] = [const { AtomicU8::new(0) }; NC];
static LAST: [AtomicU32; NC] = [const { AtomicU32::new(0) }; NC];
/// Whether a channel has converted before (the doctored loop answers a
/// stale sample only where a previous conversion exists).
static CONVERTED: [AtomicU8; NC] = [const { AtomicU8::new(0) }; NC];

/// The pad seam is empty: the models have no pins.
pub fn configure(_index: usize) {}

pub fn release(index: usize) {
    PENDING[index].store(0, Ordering::Release);
}

pub fn start(index: usize) {
    START[index].store(sched::now_ns() as u32, Ordering::Release);
    PENDING[index].store(1, Ordering::Release);
}

/// The conversion is done once its block's cycles have elapsed at the
/// block's clock.
pub fn ready(index: usize) -> bool {
    if PENDING[index].load(Ordering::Acquire) == 0 {
        return false;
    }
    let b = &facts::ADC_BLOCKS[facts::ADC_CHANNELS[index].block as usize];
    let need = adc_math::conversion_ns(b.conversion_cycles, b.clock_hz) as u32;
    (sched::now_ns() as u32).wrapping_sub(START[index].load(Ordering::Acquire)) >= need
}

/// The sample of a finished conversion, by the channel's kind -- or none
/// for a conversion the model flags (under `test-adc-flagged`: the first
/// conversion of `FLAGGED_ONCE`, every conversion of `FLAGGED_ALWAYS`):
/// the sample discarded, the next conversion started when the door asks
/// for another attempt. The doctored converter answers the flagged
/// conversion's undefined result as a sample instead.
pub fn sample(index: usize, again: bool) -> Option<u32> {
    PENDING[index].store(0, Ordering::Release);
    let c = &facts::ADC_CHANNELS[index];
    let b = &facts::ADC_BLOCKS[c.block as usize];
    let full = adc_math::full_scale(b.bits);
    let fresh = if index == facts::ADC_LOOP {
        let (compare, period, driven, polarity) = virt_pwm::output(facts::ADC_LOOP_SOURCE);
        if !driven {
            0
        } else {
            let s = adc_math::loop_sample(compare, period, full);
            if polarity == crate::pwm_math::POLARITY_INVERSED {
                full - s
            } else {
                s
            }
        }
    } else if index == facts::ADC_REFERENCE {
        ((facts::ADC_REFERENCE_MV as u64 * full as u64 + b.reference_mv as u64 / 2) / b.reference_mv as u64) as u32
    } else {
        ((c.channel as u64 + 1) * full as u64 / 5) as u32
    };
    let previous = LAST[index].swap(fresh, Ordering::AcqRel);
    let first = CONVERTED[index].swap(1, Ordering::AcqRel) == 0;
    let answered = if DOCTORED && index == facts::ADC_LOOP && !first { previous } else { fresh };
    if FLAGGED && (index == FLAGGED_ALWAYS || (index == FLAGGED_ONCE && first)) {
        if FLAGGED_DOCTORED {
            return Some(answered ^ FLAGGED_UNDEFINED);
        }
        if again {
            start(index);
        }
        return None;
    }
    Some(answered)
}

/// The block reset: every pending conversion of its channels dropped.
pub fn reset(block: usize) {
    for (i, c) in facts::ADC_CHANNELS.iter().enumerate() {
        if c.block as usize == block {
            PENDING[i].store(0, Ordering::Release);
        }
    }
}
