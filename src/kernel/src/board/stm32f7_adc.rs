//! The STM32F7's analog backend: the chip's converter blocks behind the
//! analog functions, in the shape of the emulator rows' virtual converter
//! and of the RP2350's. The register sequences are the pure module's
//! (`crate::stm32_adc_math::f7`, which states the registers and the
//! manual's sections); this file makes them volatile accesses and adds the
//! pads, the turns and the record. A row may carry several blocks -- the
//! STM32F769 Discovery's connector reaches three of its analog pins on the
//! third converter alone -- and each block converts one channel at a time:
//! a second channel of the same block waits its turn inside the function's
//! budget, and two blocks convert side by side. A block is brought up at
//! the first claim of one of its channels: its bus clock, the converters'
//! common prescaler (the APB2 clock over 4, on both clock configurations
//! inside the datasheet's range) and, where the row reads it, the internal
//! reference's enable; the longest sampling time for every channel; then a
//! wait by the kernel's clock for the converter's power-up and the
//! reference's start. A pin is put into analog mode at its channel's claim
//! and given back as a plain input without a pull at its release (the GPIO
//! backend's released shape), so a pin regains its 5 V tolerance, which the
//! datasheet withdraws in analog mode. A conversion the block flags as an
//! overrun answers no sample: the next is started with the block kept
//! while the function asks for another attempt. No interrupt, no DMA;
//! nothing masks beyond the few register writes of a start. The record
//! `__rhu_stm32_adc` is read by symbol.

use core::ptr::{addr_of_mut, read_volatile, write_volatile};
use core::sync::atomic::{AtomicBool, Ordering};

use crate::adc_math;
use crate::board::facts;
use crate::board::gpio_backend as pins;
use crate::critical::with_cs;
use crate::gpio_math as g;
use crate::sched;
use crate::stm32_adc_math::{self as m, f7, Regs};

const NB: usize = facts::ADC_BLOCKS.len();
const NC: usize = facts::ADC_CHANNELS.len();
const _: () = assert!(m::units_fit(facts::ADC_UNITS, NB, facts::ADC_CHANNELS), "the row's converter units are not sound: not one a block, an address that is not a word's, two units of one base, or a channel on a block without a unit");
const _: () = {
    let mut i = 0;
    while i < NB {
        let b = &facts::ADC_BLOCKS[i];
        assert!(b.bits == f7::BITS && b.conversion_cycles == f7::CONVERSION_CYCLES && b.clock_hz == crate::board::PCLK2_HZ / f7::PRESCALER, "a block's width, conversion cycles or clock is not the backend's: 12 bits, the sampling time plus 12 cycles, the APB2 clock over the prescaler");
        assert!(m::clock_ok(b.clock_hz, f7::CLOCK_MIN_HZ, f7::CLOCK_MAX_HZ) && m::sampling_ok(f7::SAMPLE_CYCLES, b.clock_hz, f7::REFERENCE_SAMPLE_MIN_NS), "the converters' clock is outside the datasheet's range, or the sampling time is under the internal reference's minimum");
        i += 1;
    }
};

/// Whether the row reads the internal reference (a channel without a pin).
const REFERENCE: bool = {
    let mut i = 0;
    let mut found = false;
    while i < NC {
        found |= facts::ADC_CHANNELS[i].line == adc_math::NO_LINE;
        i += 1;
    }
    found
};

/// The port block's mode register at 0x00 of a port (RM0410 6.4.1): two
/// bits a pin, 11 the analog mode.
const MODE_ANALOG: u32 = 0b11;

static TURNS: m::Turns<NB, NC> = m::Turns::new();
static UP: [AtomicBool; NB] = [const { AtomicBool::new(false) }; NB];
/// The doctored control alone: each block's sample before the last.
#[cfg(feature = "test-adc-doctored")]
static PREVIOUS: [core::sync::atomic::AtomicU32; NB] = [const { core::sync::atomic::AtomicU32::new(0) }; NB];

/// The backend's record, by symbol: the blocks brought up (one bit per
/// table position), the conversions taken, those the block flagged, the
/// starts that waited for their block, the block resets, and the common
/// control register as the last bring-up read it back.
#[repr(C)]
pub struct AdcRecord {
    pub up: u32,
    pub conversions: u32,
    pub flagged: u32,
    pub waited: u32,
    pub resets: u32,
    pub common: u32,
}

#[unsafe(export_name = "__rhu_stm32_adc")]
pub static mut RECORD: AdcRecord = AdcRecord { up: 0, conversions: 0, flagged: 0, waited: 0, resets: 0, common: 0 };

unsafe fn bump(p: *mut u32) {
    unsafe { write_volatile(p, read_volatile(p).wrapping_add(1)) }
}

struct Chip;

impl Regs for Chip {
    fn rd(&mut self, addr: usize) -> u32 {
        unsafe { read_volatile(addr as *const u32) }
    }
    fn wr(&mut self, addr: usize, value: u32) {
        unsafe { write_volatile(addr as *mut u32, value) }
    }
}

/// A spin by the kernel's clock, interrupts free.
fn spin(ns: u64) {
    let t0 = sched::now_ns();
    while sched::now_ns().wrapping_sub(t0) < ns {}
}

fn block_of(index: usize) -> usize {
    facts::ADC_CHANNELS[index].block as usize
}

fn base_of(index: usize) -> usize {
    facts::ADC_UNITS[block_of(index)].base
}

/// A line's mode register and the shift of its field.
fn mode_field(line: u32) -> (*mut u32, u32) {
    let (port, bit) = g::st_split(line);
    ((facts::GPIO_BASE + port as usize * 0x400) as *mut u32, 2 * bit)
}

/// A line's mode field as the port states it (the flight check reads it).
#[allow(dead_code)]
pub fn probe_mode(line: u32) -> u32 {
    let (moder, shift) = mode_field(line);
    unsafe { (read_volatile(moder) >> shift) & 0b11 }
}

/// The record's bring-up word and common control word, and whether a block
/// reads on (the flight check ties the record to the block).
#[allow(dead_code)]
pub fn probe_power() -> (u32, u32, u32) {
    let (up, common) = unsafe {
        let r = addr_of_mut!(RECORD);
        (read_volatile(addr_of_mut!((*r).up)), read_volatile(addr_of_mut!((*r).common)))
    };
    let mut on = 0;
    for (i, u) in facts::ADC_UNITS.iter().enumerate() {
        on |= (f7::is_on(&mut Chip, u.base) as u32) << i;
    }
    (up, common, on)
}

/// The conversions the blocks flagged, from the record.
#[allow(dead_code)]
pub fn probe_flagged() -> u32 {
    unsafe { read_volatile(addr_of_mut!((*addr_of_mut!(RECORD)).flagged)) }
}

/// A block up once per boot: the pure module's bring-up over the chip's
/// registers under one section, then the settle by the clock.
fn bring_up(block: usize) {
    if UP[block].swap(true, Ordering::AcqRel) {
        return;
    }
    let u = &facts::ADC_UNITS[block];
    with_cs(|| unsafe {
        f7::bring_up(&mut Chip, u, facts::ADC_COMMON, REFERENCE);
        let r = addr_of_mut!(RECORD);
        write_volatile(addr_of_mut!((*r).up), read_volatile(addr_of_mut!((*r).up)) | (1 << block));
        write_volatile(addr_of_mut!((*r).common), read_volatile((facts::ADC_COMMON + f7::CCR) as *const u32));
    });
    spin(f7::SETTLE_NS);
}

/// The channel's block brought up and its pin put into analog mode: a
/// plain input without a pull first (the GPIO backend enables the port's
/// clock), then the mode field. An internal channel has no pin.
pub fn configure(index: usize) {
    bring_up(block_of(index));
    let c = &facts::ADC_CHANNELS[index];
    if c.line == adc_math::NO_LINE {
        return;
    }
    pins::configure(c.line as u32, g::MODE_INPUT, g::PULL_NONE, false);
    let (moder, shift) = mode_field(c.line as u32);
    with_cs(|| unsafe { write_volatile(moder, read_volatile(moder) | (MODE_ANALOG << shift)) });
}

/// The channel out of its block's turn and queue; its pin back to a plain
/// input without a pull.
pub fn release(index: usize) {
    TURNS.forget(block_of(index), index);
    let c = &facts::ADC_CHANNELS[index];
    if c.line != adc_math::NO_LINE {
        pins::release(c.line as u32);
    }
}

fn kick(index: usize) {
    let ch = facts::ADC_CHANNELS[index].channel as u32;
    with_cs(|| f7::kick(&mut Chip, base_of(index), ch));
}

/// The block taken and the conversion started, or the channel queued
/// behind its block's conversion.
pub fn start(index: usize) {
    if TURNS.start(block_of(index), index) {
        kick(index);
    } else {
        with_cs(|| unsafe { bump(addr_of_mut!((*addr_of_mut!(RECORD)).waited)) });
    }
}

/// The owner's conversion is done when the block's end flag is set; a
/// queued channel takes its block and starts once it is free (the
/// function's wait polls this).
pub fn ready(index: usize) -> bool {
    let block = block_of(index);
    if TURNS.owns(block, index) {
        return f7::done(&mut Chip, base_of(index));
    }
    if TURNS.promote(block, index) {
        kick(index);
    }
    false
}

/// The sample and the block given to the next channel -- or none for a
/// conversion the block flagged: counted and discarded, the next started
/// with the block kept when the function asks for another attempt, the
/// block given back otherwise.
pub fn sample(index: usize, again: bool) -> Option<u32> {
    let block = block_of(index);
    let s = f7::take(&mut Chip, base_of(index));
    with_cs(|| unsafe {
        let r = addr_of_mut!(RECORD);
        bump(addr_of_mut!((*r).conversions));
        if s.is_none() {
            bump(addr_of_mut!((*r).flagged));
        }
    });
    if s.is_none() && again {
        kick(index);
        return None;
    }
    TURNS.give(block, index);
    #[cfg(feature = "test-adc-doctored")]
    let s = s.map(|v| PREVIOUS[block].swap(v, Ordering::AcqRel));
    s
}

/// A block reset after a conversion passed its budget: the converter off
/// and its flags cleared, the block's turn and queue cleared, the
/// converter on again and the settle.
pub fn reset(block: usize) {
    let base = facts::ADC_UNITS[block].base;
    with_cs(|| f7::off(&mut Chip, base));
    TURNS.clear(block, |i| block_of(i) == block);
    with_cs(|| unsafe {
        f7::on(&mut Chip, base);
        bump(addr_of_mut!((*addr_of_mut!(RECORD)).resets));
    });
    spin(f7::SETTLE_NS);
}
