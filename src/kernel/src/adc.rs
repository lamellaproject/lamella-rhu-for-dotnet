//! The ADC doors' bodies: a program claims a converter channel, reads it
//! one conversion at a time, asks a block's reference voltage and releases
//! the channel -- the six verbs `api.rs` exports and `include/rhu.h`
//! states. A row's channels are a TABLE in its facts
//! (`board::facts::ADC_CHANNELS` over `ADC_BLOCKS`): the count door says
//! how many across the blocks, the channel door the index-th channel's
//! facts, so a program enumerates without guessing. Beneath the doors: the
//! claims table (a claim takes the channel's line in analog mode AND the
//! channel bit of its block), the pure arithmetic (`adc_math`: the width's
//! full scale, the conversion time and the budget it bounds) and the
//! board's backend (`board::adc_backend`: the registers). A read starts one
//! conversion and polls for its end against a budget from the block's
//! clock -- spinning, then yielding, the bus doors' wait -- and a budget
//! passed resets the block and answers the timeout code. A conversion the
//! block flags as erroneous is discarded and the next started, at most
//! eight in one read; the eighth flagged refuses the read with the
//! input/output code and the block released. No interrupt; no conversion
//! runs under the kernel's critical section.

#![cfg_attr(not(rhu_pwm_adc_rows), allow(dead_code))]

use core::ptr::{addr_of_mut, read_volatile, write_volatile};

use crate::adc_math as m;
use crate::board::facts;
use crate::critical::with_cs;
use crate::sched;

/// The code this module adds to the hosting API's set (`include/rhu.h`).
pub const ERR_BUSY: i32 = -16;
/// The code a read answers when the block flagged every one of its attempts
/// (`include/rhu.h` RHU_EIO, the value the other doors answer for a part
/// that failed).
pub const ERR_IO: i32 = sched::ERR_IO;
/// The conversions one read runs at most. A conversion the block flags as
/// erroneous is discarded -- the RP2350 datasheet's section 12.4.3 leaves
/// its result undefined and says such samples are discarded -- and the
/// next is started with the block kept, up to this many; the last one
/// flagged refuses the read. Eight conversions are 64 microseconds of
/// converting at that part's 96-cycle conversion on its 12 MHz clock: an
/// error that outlasts them is not the comparator's chance miss near its
/// threshold (the datasheet's cause) but a converter or an input that
/// cannot be read, which the program hears as a failure.
pub const CONVERSION_ATTEMPTS: u32 = 8;

/// The row's counts, constant per image (the banner line's numbers).
pub const CHANNELS: usize = facts::ADC_CHANNELS.len();
pub const BLOCKS: usize = facts::ADC_BLOCKS.len();
const _: () = assert!(m::table_sound(facts::ADC_CHANNELS, facts::ADC_BLOCKS), "the ADC tables are not sound: a block past the claims table's reach or without a clock, a width or a cycle count, a channel outside its block, or two entries for one channel or one line");

/// The witness, by symbol. `last_refused_index` names the channel index,
/// or the handle, the last refusal was asked with.
#[repr(C)]
pub struct AdcWitness {
    pub channels: u32,
    pub blocks: u32,
    pub claims_taken: u32,
    pub releases: u32,
    pub conversions: u32,
    pub timeouts: u32,
    pub resets: u32,
    pub refused_busy: u32,
    pub refused_inval: u32,
    pub last_sample: u32,
    pub last_refused_index: u32,
    pub last_refused_code: i32,
    /// The conversions started again after a flagged one was discarded, and
    /// the reads refused after their last attempt was flagged.
    pub retries: u32,
    pub conversion_errors: u32,
}

#[unsafe(export_name = "__rhu_adc_witness")]
pub static mut WITNESS: AdcWitness = AdcWitness {
    channels: 0,
    blocks: 0,
    claims_taken: 0,
    releases: 0,
    conversions: 0,
    timeouts: 0,
    resets: 0,
    refused_busy: 0,
    refused_inval: 0,
    last_sample: 0,
    last_refused_index: 0,
    last_refused_code: 0,
    retries: 0,
    conversion_errors: 0,
};

/// The witness's fields are written through raw pointers, one field at a
/// time, under the critical section; no reference to the static is ever
/// formed, so a reader's snapshot by value aliases nothing.
fn witness() -> *mut AdcWitness {
    addr_of_mut!(WITNESS)
}

unsafe fn bump(field: *mut u32, by: u32) {
    unsafe { write_volatile(field, read_volatile(field).wrapping_add(by)) }
}

fn refused(index: u32, code: i32) -> i32 {
    with_cs(|| unsafe {
        let w = witness();
        match code {
            ERR_BUSY => bump(addr_of_mut!((*w).refused_busy), 1),
            sched::ERR_TIMEDOUT => bump(addr_of_mut!((*w).timeouts), 1),
            ERR_IO => bump(addr_of_mut!((*w).conversion_errors), 1),
            _ => bump(addr_of_mut!((*w).refused_inval), 1),
        }
        write_volatile(addr_of_mut!((*w).last_refused_index), index);
        write_volatile(addr_of_mut!((*w).last_refused_code), code);
    });
    code
}

/// The tables' sizes recorded; nothing else moves at boot (a channel is
/// claimed by a program, never at boot).
pub fn init() {
    with_cs(|| unsafe {
        let w = witness();
        write_volatile(addr_of_mut!((*w).channels), CHANNELS as u32);
        write_volatile(addr_of_mut!((*w).blocks), BLOCKS as u32);
    });
}

pub fn count() -> i32 {
    CHANNELS as i32
}

/// The enumeration: the index-th channel's facts -- its block's number,
/// its channel number, its line (the no-line value for an internal source)
/// and its width; the invalid code past the count or for a NULL or
/// misaligned pointer.
pub fn channel(index: u32, block: *mut u32, channel: *mut u32, line: *mut u32, bits: *mut u32) -> i32 {
    let Some(c) = facts::ADC_CHANNELS.get(index as usize) else {
        return refused(index, sched::ERR_INVAL);
    };
    if block.is_null() || channel.is_null() || line.is_null() || bits.is_null() || (block as usize | channel as usize | line as usize | bits as usize) & 3 != 0 {
        return refused(index, sched::ERR_INVAL);
    }
    let b = &facts::ADC_BLOCKS[c.block as usize];
    unsafe {
        block.write(b.number);
        channel.write(c.channel as u32);
        line.write(if c.line == m::NO_LINE { u32::MAX } else { c.line as u32 });
        bits.write(b.bits);
    }
    0
}


#[cfg(rhu_pwm_adc_rows)]
mod served {
    use super::*;
    use crate::board::adc_backend as hw;
    use crate::claims;
    use core::sync::atomic::{AtomicU8, Ordering};

    static CLAIMED: [AtomicU8; CHANNELS] = [const { AtomicU8::new(0) }; CHANNELS];

    fn held(h: i32) -> Option<usize> {
        if h < 0 || h as usize >= CHANNELS || CLAIMED[h as usize].load(Ordering::Acquire) == 0 {
            None
        } else {
            Some(h as usize)
        }
    }

    /// Whether a claimed channel reads the line: the GPIO side refuses a
    /// claim of such a line until the channel is released (the other
    /// direction is the claim below, refused on a line a program holds as
    /// a GPIO line).
    pub fn line_held(line: u32) -> bool {
        facts::ADC_CHANNELS.iter().enumerate().any(|(ix, c)| c.line != m::NO_LINE && c.line as u32 == line && CLAIMED[ix].load(Ordering::Acquire) != 0)
    }

    /// A claim: the index inside the table, then the line (where the channel
    /// reads one) AND the channel bit taken under one section, all or
    /// nothing.
    pub fn claim(index: u32) -> i32 {
        let Some(c) = facts::ADC_CHANNELS.get(index as usize) else {
            return refused(index, sched::ERR_INVAL);
        };
        let ix = index as usize;
        let r = with_cs(|| unsafe {
            let t = &mut *addr_of_mut!(crate::gpio::CLAIMS);
            if c.line != m::NO_LINE {
                let r = claims::claim(t, c.line as u32);
                if r != 0 {
                    return r;
                }
            }
            let r = claims::adc_channel_claim(t, c.block as u32, c.channel as u32);
            if r != 0 && c.line != m::NO_LINE {
                claims::release(t, c.line as u32);
            }
            r
        });
        if r != 0 {
            return refused(index, r);
        }
        hw::configure(ix);
        CLAIMED[ix].store(1, Ordering::Release);
        with_cs(|| unsafe { bump(addr_of_mut!((*witness()).claims_taken), 1) });
        ix as i32
    }

    /// A read: a conversion started and awaited against the block's budget,
    /// the sample answered inside the width. A conversion the block flagged
    /// is discarded and the next started with the block kept, the retry
    /// counted, up to the attempts' bound; the last attempt flagged answers
    /// the input/output code with the block released, never a sample. The
    /// timeout code with the block reset when a budget passes first.
    pub fn read(h: i32) -> i32 {
        let Some(ix) = held(h) else {
            return refused(h as u32, sched::ERR_INVAL);
        };
        let c = &facts::ADC_CHANNELS[ix];
        let b = &facts::ADC_BLOCKS[c.block as usize];
        let conversion = m::conversion_ns(b.conversion_cycles, b.clock_hz);
        let budget = m::budget_ticks(conversion, crate::board::TICK_HZ);
        hw::start(ix);
        let mut attempt = 1;
        loop {
            if !crate::bus::wait_until(budget, sched::DEADLINE_NONE, 2 * conversion, || hw::ready(ix)) {
                hw::reset(c.block as usize);
                with_cs(|| unsafe { bump(addr_of_mut!((*witness()).resets), 1) });
                kprintln!("[adc] block {}: the conversion passed its budget on channel {}; the block reset", b.number, c.channel);
                return refused(ix as u32, sched::ERR_TIMEDOUT);
            }
            let again = attempt < CONVERSION_ATTEMPTS;
            if let Some(s) = hw::sample(ix, again) {
                let sample = s & m::full_scale(b.bits);
                with_cs(|| unsafe {
                    let w = witness();
                    bump(addr_of_mut!((*w).conversions), 1);
                    write_volatile(addr_of_mut!((*w).last_sample), sample);
                });
                return sample as i32;
            }
            if !again {
                kprintln!("[adc] block {}: the converter flagged {} conversions in a row on channel {}; the read refused", b.number, CONVERSION_ATTEMPTS, c.channel);
                return refused(ix as u32, ERR_IO);
            }
            attempt += 1;
            with_cs(|| unsafe { bump(addr_of_mut!((*witness()).retries), 1) });
        }
    }

    /// A block's reference voltage in millivolts, a fact of the board.
    pub fn reference(block: u32, millivolts: *mut u32) -> i32 {
        let Some(bix) = m::block_index(facts::ADC_BLOCKS, block) else {
            return refused(block, sched::ERR_NODEV);
        };
        if millivolts.is_null() || (millivolts as usize) & 3 != 0 {
            return refused(block, sched::ERR_INVAL);
        }
        unsafe { millivolts.write(facts::ADC_BLOCKS[bix].reference_mv) };
        0
    }

    pub fn release(h: i32) -> i32 {
        let Some(ix) = held(h) else {
            return refused(h as u32, sched::ERR_INVAL);
        };
        let c = &facts::ADC_CHANNELS[ix];
        hw::release(ix);
        CLAIMED[ix].store(0, Ordering::Release);
        with_cs(|| unsafe {
            let t = &mut *addr_of_mut!(crate::gpio::CLAIMS);
            claims::adc_channel_release(t, c.block as u32, c.channel as u32);
            if c.line != m::NO_LINE {
                claims::release(t, c.line as u32);
            }
            bump(addr_of_mut!((*witness()).releases), 1);
        });
        0
    }
}

#[cfg(rhu_pwm_adc_rows)]
pub use served::{claim, line_held, read, reference, release};


#[cfg(not(rhu_pwm_adc_rows))]
pub fn claim(index: u32) -> i32 {
    refused(index, sched::ERR_NODEV)
}

#[cfg(not(rhu_pwm_adc_rows))]
pub fn read(h: i32) -> i32 {
    refused(h as u32, sched::ERR_NODEV)
}

#[cfg(not(rhu_pwm_adc_rows))]
pub fn reference(block: u32, _millivolts: *mut u32) -> i32 {
    refused(block, sched::ERR_NODEV)
}

#[cfg(not(rhu_pwm_adc_rows))]
pub fn release(h: i32) -> i32 {
    refused(h as u32, sched::ERR_NODEV)
}

#[cfg(not(rhu_pwm_adc_rows))]
pub fn line_held(_line: u32) -> bool {
    false
}
