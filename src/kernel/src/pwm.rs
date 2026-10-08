//! The PWM doors' bodies: a program claims a line a timer can drive, sets
//! its period and duty in nanoseconds, reads the period the timer runs,
//! drives or parks the output, releases it and asks its timer-and-channel
//! numbering -- the eight verbs `api.rs` exports and `include/rhu.h`
//! states. A row's PWM-capable lines are a TABLE in its facts
//! (`board::facts::PWM_LINES` over `PWM_TIMERS`): the count door says how
//! many, the line door the index-th line's number, so a program enumerates
//! without guessing. Beneath the doors: the claims table (a claim takes
//! the line AND the timer channel, so no program can take either away),
//! the pure arithmetic (`pwm_math`: the quantization of a period to the
//! timer's prescaler and period registers, the achieved period back from
//! them) and the board's backend (`board::pwm_backend`: the registers). On
//! a timer whose channels share one period -- every timer served -- a set
//! whose quantized pair disagrees with the pair another held channel of
//! the timer already runs is refused with the holder's line in the
//! witness. No interrupt: a timer runs free once set. The claims' few
//! instructions and the witness's writes are the only critical sections.

#![cfg_attr(not(rhu_pwm_adc_rows), allow(dead_code))]

use core::ptr::{addr_of_mut, read_volatile, write_volatile};

use crate::board::facts;
use crate::critical::with_cs;
use crate::pwm_math as m;
use crate::sched;

/// The code this module adds to the hosting API's set (`include/rhu.h`).
pub const ERR_BUSY: i32 = -16;

/// The row's counts, constant per image (the banner line's numbers).
pub const LINES: usize = facts::PWM_LINES.len();
pub const TIMERS: usize = facts::PWM_TIMERS.len();
const _: () = assert!(m::table_sound(facts::PWM_LINES, facts::PWM_TIMERS), "the PWM tables are not sound: a timer past the claims table's reach, a line's timer or channel outside the table, or two entries for one line");

/// The witness, by symbol. `last_refused_line` names the line, or the
/// handle, the last refusal was asked with; `last_holder_line` the line
/// whose period a shared-period refusal met.
#[repr(C)]
pub struct PwmWitness {
    pub lines: u32,
    pub timers: u32,
    pub claims_taken: u32,
    pub releases: u32,
    pub sets: u32,
    pub enables: u32,
    pub refused_busy: u32,
    pub refused_inval: u32,
    pub last_refused_line: u32,
    pub last_refused_code: i32,
    pub last_holder_line: u32,
    pub reserved: u32,
    pub last_period_ns: u64,
}

#[unsafe(export_name = "__rhu_pwm_witness")]
pub static mut WITNESS: PwmWitness = PwmWitness {
    lines: 0,
    timers: 0,
    claims_taken: 0,
    releases: 0,
    sets: 0,
    enables: 0,
    refused_busy: 0,
    refused_inval: 0,
    last_refused_line: 0,
    last_refused_code: 0,
    last_holder_line: 0,
    reserved: 0,
    last_period_ns: 0,
};

/// The witness's fields are written through raw pointers, one field at a
/// time, under the critical section; no reference to the static is ever
/// formed, so a reader's snapshot by value aliases nothing.
fn witness() -> *mut PwmWitness {
    addr_of_mut!(WITNESS)
}

unsafe fn bump(field: *mut u32, by: u32) {
    unsafe { write_volatile(field, read_volatile(field).wrapping_add(by)) }
}

fn refused(line: u32, code: i32) -> i32 {
    with_cs(|| unsafe {
        let w = witness();
        match code {
            ERR_BUSY => bump(addr_of_mut!((*w).refused_busy), 1),
            _ => bump(addr_of_mut!((*w).refused_inval), 1),
        }
        write_volatile(addr_of_mut!((*w).last_refused_line), line);
        write_volatile(addr_of_mut!((*w).last_refused_code), code);
    });
    code
}

/// The tables' sizes recorded; nothing else moves at boot (a line is
/// claimed by a program, never at boot).
pub fn init() {
    with_cs(|| unsafe {
        let w = witness();
        write_volatile(addr_of_mut!((*w).lines), LINES as u32);
        write_volatile(addr_of_mut!((*w).timers), TIMERS as u32);
    });
}

pub fn count() -> i32 {
    LINES as i32
}

/// The enumeration: the number of the index-th line a timer can drive; the
/// invalid code past the count.
pub fn line(index: u32) -> i32 {
    match facts::PWM_LINES.get(index as usize) {
        Some(l) => l.line as i32,
        None => refused(index, sched::ERR_INVAL),
    }
}


#[cfg(rhu_pwm_adc_rows)]
mod served {
    use super::*;
    use crate::board::pwm_backend as hw;
    use crate::claims;
    use core::sync::atomic::{AtomicU32, AtomicU8, Ordering};

    const NONE: u32 = u32::MAX;

    /// Per line: claimed, a period set, the output enabled.
    static CLAIMED: [AtomicU8; LINES] = [const { AtomicU8::new(0) }; LINES];
    static SET: [AtomicU8; LINES] = [const { AtomicU8::new(0) }; LINES];
    static ENABLED: [AtomicU8; LINES] = [const { AtomicU8::new(0) }; LINES];
    /// Per timer: the pair it runs (prescaler 0 = none), how many held
    /// channels have a period set, and the line whose set established the
    /// pair (the holder a refusal names).
    static PAIR_P: [AtomicU32; TIMERS] = [const { AtomicU32::new(0) }; TIMERS];
    static PAIR_A: [AtomicU32; TIMERS] = [const { AtomicU32::new(0) }; TIMERS];
    static SET_COUNT: [AtomicU32; TIMERS] = [const { AtomicU32::new(0) }; TIMERS];
    static HOLDER: [AtomicU32; TIMERS] = [const { AtomicU32::new(NONE) }; TIMERS];

    /// The table index of a handle this program holds.
    fn held(h: i32) -> Option<usize> {
        if h < 0 || h as usize >= LINES || CLAIMED[h as usize].load(Ordering::Acquire) == 0 {
            None
        } else {
            Some(h as usize)
        }
    }

    /// A claim: the line inside the table, then the line AND the timer
    /// channel taken under one section, all or nothing (a channel refused
    /// gives the line back before anything is programmed).
    pub fn claim(line: u32) -> i32 {
        let Some(ix) = m::line_index(facts::PWM_LINES, line) else {
            return refused(line, sched::ERR_INVAL);
        };
        let l = &facts::PWM_LINES[ix];
        let r = with_cs(|| unsafe {
            let c = &mut *addr_of_mut!(crate::gpio::CLAIMS);
            let r = claims::claim(c, line);
            if r != 0 {
                return r;
            }
            let r = claims::pwm_channel_claim(c, l.timer as u32, l.channel as u32);
            if r != 0 {
                claims::release(c, line);
                return r;
            }
            0
        });
        if r != 0 {
            return refused(line, r);
        }
        hw::configure(ix);
        CLAIMED[ix].store(1, Ordering::Release);
        with_cs(|| unsafe { bump(addr_of_mut!((*witness()).claims_taken), 1) });
        ix as i32
    }

    /// A set: the arguments partitioned, the period quantized on the
    /// timer's clock, the shared period judged under the section, then the
    /// timer's pair and the channel's compare programmed.
    pub fn set(h: i32, period_ns: u64, duty_ns: u64, polarity: u32) -> i32 {
        let Some(ix) = held(h) else {
            return refused(h as u32, sched::ERR_INVAL);
        };
        let l = &facts::PWM_LINES[ix];
        let t = &facts::PWM_TIMERS[l.timer as usize];
        if !m::polarity_valid(polarity) || period_ns == 0 || duty_ns > period_ns {
            return refused(l.line as u32, sched::ERR_INVAL);
        }
        let Some(pair) = m::quantize(period_ns, t) else {
            return refused(l.line as u32, sched::ERR_INVAL);
        };
        let tix = l.timer as usize;
        let verdict = with_cs(|| {
            let cur = (PAIR_P[tix].load(Ordering::Acquire), PAIR_A[tix].load(Ordering::Acquire));
            let others = SET_COUNT[tix].load(Ordering::Acquire) - SET[ix].load(Ordering::Acquire) as u32;
            if others > 0 && !m::same_pair(cur, pair) {
                return Err(HOLDER[tix].load(Ordering::Acquire));
            }
            if SET[ix].load(Ordering::Acquire) == 0 {
                SET_COUNT[tix].fetch_add(1, Ordering::AcqRel);
                SET[ix].store(1, Ordering::Release);
            }
            if others == 0 {
                HOLDER[tix].store(l.line as u32, Ordering::Release);
            }
            PAIR_P[tix].store(pair.0, Ordering::Release);
            PAIR_A[tix].store(pair.1, Ordering::Release);
            Ok(())
        });
        if let Err(holder) = verdict {
            with_cs(|| unsafe { write_volatile(addr_of_mut!((*witness()).last_holder_line), holder) });
            return refused(l.line as u32, ERR_BUSY);
        }
        let achieved = m::achieved_ns(pair.0, pair.1, t.clock_hz);
        hw::set_timer(tix, pair.0, pair.1);
        hw::set_channel(ix, m::compare(duty_ns, t.clock_hz, pair.0, pair.1), polarity);
        with_cs(|| unsafe {
            let w = witness();
            bump(addr_of_mut!((*w).sets), 1);
            write_volatile(addr_of_mut!((*w).last_period_ns), achieved);
        });
        0
    }

    /// The period the timer runs, exact from its clock and the pair; 0
    /// before any set.
    pub fn period(h: i32, achieved_ns: *mut u64) -> i32 {
        let Some(ix) = held(h) else {
            return refused(h as u32, sched::ERR_INVAL);
        };
        if achieved_ns.is_null() || (achieved_ns as usize) & 3 != 0 {
            return refused(facts::PWM_LINES[ix].line as u32, sched::ERR_INVAL);
        }
        let l = &facts::PWM_LINES[ix];
        let tix = l.timer as usize;
        let ns = if SET[ix].load(Ordering::Acquire) == 0 {
            0
        } else {
            m::achieved_ns(PAIR_P[tix].load(Ordering::Acquire), PAIR_A[tix].load(Ordering::Acquire), facts::PWM_TIMERS[tix].clock_hz)
        };
        unsafe { achieved_ns.write_unaligned(ns) };
        0
    }

    /// The output driven (1) or parked at its inactive level (0); refused
    /// before any set (there is nothing to run).
    pub fn enable(h: i32, on: u32) -> i32 {
        let Some(ix) = held(h) else {
            return refused(h as u32, sched::ERR_INVAL);
        };
        if on > 1 || SET[ix].load(Ordering::Acquire) == 0 {
            return refused(facts::PWM_LINES[ix].line as u32, sched::ERR_INVAL);
        }
        hw::enable(ix, on != 0);
        ENABLED[ix].store(on as u8, Ordering::Release);
        if on != 0 {
            with_cs(|| unsafe { bump(addr_of_mut!((*witness()).enables), 1) });
        }
        0
    }

    /// The release: the output parked, the timer's pair dropped when no
    /// held channel of it has a period any more, the channel and the line
    /// given back.
    pub fn release(h: i32) -> i32 {
        let Some(ix) = held(h) else {
            return refused(h as u32, sched::ERR_INVAL);
        };
        let l = &facts::PWM_LINES[ix];
        let tix = l.timer as usize;
        hw::park(ix);
        ENABLED[ix].store(0, Ordering::Release);
        let last = with_cs(|| {
            let mut last = false;
            if SET[ix].load(Ordering::Acquire) != 0 {
                SET[ix].store(0, Ordering::Release);
                if SET_COUNT[tix].fetch_sub(1, Ordering::AcqRel) == 1 {
                    PAIR_P[tix].store(0, Ordering::Release);
                    PAIR_A[tix].store(0, Ordering::Release);
                    HOLDER[tix].store(NONE, Ordering::Release);
                    last = true;
                }
            }
            last
        });
        if last {
            hw::clear_timer(tix);
        }
        hw::release(ix);
        CLAIMED[ix].store(0, Ordering::Release);
        with_cs(|| unsafe {
            let c = &mut *addr_of_mut!(crate::gpio::CLAIMS);
            claims::pwm_channel_release(c, l.timer as u32, l.channel as u32);
            claims::release(c, l.line as u32);
            bump(addr_of_mut!((*witness()).releases), 1);
        });
        0
    }

    /// The timer's number, the channel's index and the timer's input clock.
    pub fn info(h: i32, timer: *mut u32, channel: *mut u32, clock_hz: *mut u32) -> i32 {
        let Some(ix) = held(h) else {
            return refused(h as u32, sched::ERR_INVAL);
        };
        let l = &facts::PWM_LINES[ix];
        if timer.is_null() || channel.is_null() || clock_hz.is_null() || (timer as usize | channel as usize | clock_hz as usize) & 3 != 0 {
            return refused(l.line as u32, sched::ERR_INVAL);
        }
        let t = &facts::PWM_TIMERS[l.timer as usize];
        unsafe {
            timer.write(t.number);
            channel.write(l.channel as u32);
            clock_hz.write(t.clock_hz);
        }
        0
    }
}

#[cfg(rhu_pwm_adc_rows)]
pub use served::{claim, enable, info, period, release, set};


#[cfg(not(rhu_pwm_adc_rows))]
pub fn claim(line: u32) -> i32 {
    refused(line, sched::ERR_NODEV)
}

#[cfg(not(rhu_pwm_adc_rows))]
pub fn set(h: i32, _period_ns: u64, _duty_ns: u64, _polarity: u32) -> i32 {
    refused(h as u32, sched::ERR_NODEV)
}

#[cfg(not(rhu_pwm_adc_rows))]
pub fn period(h: i32, _achieved_ns: *mut u64) -> i32 {
    refused(h as u32, sched::ERR_NODEV)
}

#[cfg(not(rhu_pwm_adc_rows))]
pub fn enable(h: i32, _on: u32) -> i32 {
    refused(h as u32, sched::ERR_NODEV)
}

#[cfg(not(rhu_pwm_adc_rows))]
pub fn release(h: i32) -> i32 {
    refused(h as u32, sched::ERR_NODEV)
}

#[cfg(not(rhu_pwm_adc_rows))]
pub fn info(h: i32, _timer: *mut u32, _channel: *mut u32, _clock_hz: *mut u32) -> i32 {
    refused(h as u32, sched::ERR_NODEV)
}
