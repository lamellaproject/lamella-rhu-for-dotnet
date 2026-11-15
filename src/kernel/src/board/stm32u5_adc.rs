//! The STM32U5's analog backend: the chip's 14-bit converter behind the
//! analog functions, in the shape of the emulator rows' virtual converter
//! and of the RP2350's. The register sequences are the pure module's
//! (`crate::stm32_adc_math::u5`, which states the registers and the
//! manual's sections); this file makes them volatile accesses and adds the
//! pads, the turn and the record. The converter has a power-up of its own,
//! run once at the first claim of a channel: THE ANALOG SUPPLY first -- the
//! chip isolates its analog blocks after a reset until the supply is
//! declared valid (RM0456 10.4.4), and the manual's sequence for it
//! (10.6.3: the supply's monitor enabled, its verdict awaited, the monitor
//! disabled, the valid bit set) is the second I/O supply's, so the same
//! code runs it with the analog supply's bits -- then the converters'
//! clocks (the 16 MHz internal oscillator as the kernel clock, which both
//! clock configurations of the row keep running, so a conversion's time
//! does not move with the configuration), the regulator, the calibration
//! with its linearity (about 2 ms), the enable, and one conversion of 14
//! bits at the longest sampling time, with every channel of the row's
//! table preselected. A supply that does not read ready, or a stage that
//! misses its bound, leaves the converter down with one kernel line: a
//! read then passes its budget and answers the timeout code, and the
//! block's reset tries the power-up again. One conversion at a time: a
//! second channel waits its turn inside the function's budget. A pin is
//! put into analog mode at its channel's claim and given back as a plain
//! input without a pull at its release. A conversion the block flags as an
//! overrun answers no sample. No interrupt, no DMA. The record
//! `__rhu_stm32u5_adc` is read by symbol.

use core::ptr::{addr_of_mut, read_volatile, write_volatile};
use core::sync::atomic::{AtomicBool, Ordering};

use crate::adc_math;
use crate::board::facts;
use crate::board::gpio_backend as pins;
use crate::critical::with_cs;
use crate::gpio_math as g;
use crate::stm32_adc_math::{self as m, u5, Regs};

const NC: usize = facts::ADC_CHANNELS.len();
const _: () = assert!(facts::ADC_BLOCKS.len() == 1 && m::units_fit(&[facts::ADC_FACTS.unit], 1, facts::ADC_CHANNELS) && u5::facts_sound(&facts::ADC_FACTS), "the row's converter facts are not sound: more than one block, an address that is not a word's, or a clock selector code outside its field");
const _: () = assert!(facts::ADC_BLOCKS[0].bits == u5::BITS && facts::ADC_BLOCKS[0].conversion_cycles == u5::CONVERSION_CYCLES && facts::ADC_BLOCKS[0].clock_hz == facts::ADC_KERNEL_HZ, "the block's width, conversion cycles or clock is not the backend's: 14 bits, the sampling time plus 17 cycles, the kernel clock the selector names");
const _: () = assert!(m::clock_ok(facts::ADC_KERNEL_HZ, u5::CLOCK_MIN_HZ, u5::CLOCK_MAX_HZ) && m::sampling_ok(u5::SAMPLE_CYCLES, facts::ADC_KERNEL_HZ, u5::REFERENCE_SAMPLE_MIN_NS), "the converter's clock is outside the datasheet's range, or the sampling time is under the internal reference's minimum");
const _: () = assert!(crate::stm32u5_supply_math::facts_sound(&facts::ANALOG_SUPPLY), "the analog supply's facts are not sound");

/// Whether the row reads the internal reference, and the preselection word
/// of every channel of its table.
const REFERENCE: bool = {
    let mut i = 0;
    let mut found = false;
    while i < NC {
        found |= facts::ADC_CHANNELS[i].line == adc_math::NO_LINE;
        i += 1;
    }
    found
};
const PRESELECT: u32 = {
    let mut i = 0;
    let mut word = 0;
    while i < NC {
        word |= 1 << facts::ADC_CHANNELS[i].channel;
        i += 1;
    }
    word
};

/// The bound of each polled stage: the status reads of at least twenty
/// milliseconds (a read takes at least four cycles of the system clock; the
/// calibration, the longest stage, takes about two).
const BOUND_READS: u32 = crate::board::SYSCLK_HZ / 50 / 4;
/// The port block's mode register at 0x00 of a port (RM0456 13.4.1): two
/// bits a pin, 11 the analog mode.
const MODE_ANALOG: u32 = 0b11;

/// The record's stage words beyond the pure module's: the power-up has not
/// run, and the analog supply did not read ready.
pub const STAGE_NOT_RUN: u32 = u32::MAX;
pub const STAGE_SUPPLY: u32 = 4;

static TURNS: m::Turns<1, NC> = m::Turns::new();
static UP: AtomicBool = AtomicBool::new(false);
/// The doctored control alone: the sample before the last.
#[cfg(feature = "test-adc-doctored")]
static PREVIOUS: core::sync::atomic::AtomicU32 = core::sync::atomic::AtomicU32::new(0);

/// The backend's record, by symbol: the power-up's stage (0 the converter
/// is up; 1 the regulator, 2 the calibration or 3 the enable missed its
/// bound; 4 the analog supply did not read ready; all ones before the
/// first power-up), the status reads the supply's verdict and each stage
/// took, the conversions taken, those the block flagged, the starts that
/// waited for the block, and the block resets.
#[repr(C)]
pub struct AdcRecord {
    pub stage: u32,
    pub supply_polls: u32,
    pub regulator_polls: u32,
    pub calibration_polls: u32,
    pub enables: u32,
    pub conversions: u32,
    pub flagged: u32,
    pub waited: u32,
    pub resets: u32,
}

#[unsafe(export_name = "__rhu_stm32u5_adc")]
pub static mut RECORD: AdcRecord = AdcRecord { stage: STAGE_NOT_RUN, supply_polls: 0, regulator_polls: 0, calibration_polls: 0, enables: 0, conversions: 0, flagged: 0, waited: 0, resets: 0 };

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

const BASE: usize = facts::ADC_FACTS.unit.base;

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

/// The record's power-up words: the stage, the supply's status reads, the
/// regulator's, the calibration's and the enables (the flight check).
#[allow(dead_code)]
pub fn probe_power() -> (u32, u32, u32, u32, u32) {
    unsafe {
        let r = addr_of_mut!(RECORD);
        (read_volatile(addr_of_mut!((*r).stage)), read_volatile(addr_of_mut!((*r).supply_polls)), read_volatile(addr_of_mut!((*r).regulator_polls)), read_volatile(addr_of_mut!((*r).calibration_polls)), read_volatile(addr_of_mut!((*r).enables)))
    }
}

/// The conversions the block flagged, from the record.
#[allow(dead_code)]
pub fn probe_flagged() -> u32 {
    unsafe { read_volatile(addr_of_mut!((*addr_of_mut!(RECORD)).flagged)) }
}

/// The power-up over the chip's registers, recorded: the analog supply's
/// step when `supply` (the probe flavor's first power-up leaves it out),
/// then the clocks under one section and the converter's own sequence. The
/// stage it ended on.
pub fn power_up(supply: bool) -> u32 {
    let mut supply_polls = 0;
    if supply {
        let s = super::stm32u5_supply::sequence(&facts::ANALOG_SUPPLY);
        supply_polls = s.polls;
        if !s.valid {
            record(STAGE_SUPPLY, supply_polls, 0, 0, 0);
            kprintln!("[adc] the analog supply did not read ready inside its bound: the converter stays off and a read answers the timeout code");
            return STAGE_SUPPLY;
        }
    }
    with_cs(|| u5::clocks(&mut Chip, &facts::ADC_FACTS));
    let o = u5::power_up(&mut Chip, &facts::ADC_FACTS, REFERENCE, PRESELECT, BOUND_READS);
    record(o.stage, supply_polls, o.regulator_polls, o.calibration_polls, o.enables);
    if o.stage != u5::STAGE_UP {
        kprintln!("[adc] the converter's power-up missed its bound at stage {} (1 the regulator, 2 the calibration, 3 the enable): the converter stays off and a read answers the timeout code", o.stage);
    }
    o.stage
}

/// The converter taken down for the probe: disabled, the regulator off,
/// deep power-down entered. True when it reads disabled.
#[allow(dead_code)]
pub fn power_down() -> bool {
    u5::power_down(&mut Chip, &facts::ADC_FACTS, true, BOUND_READS)
}

fn record(stage: u32, supply_polls: u32, regulator_polls: u32, calibration_polls: u32, enables: u32) {
    with_cs(|| unsafe {
        let r = addr_of_mut!(RECORD);
        write_volatile(addr_of_mut!((*r).stage), stage);
        write_volatile(addr_of_mut!((*r).supply_polls), supply_polls);
        write_volatile(addr_of_mut!((*r).regulator_polls), regulator_polls);
        write_volatile(addr_of_mut!((*r).calibration_polls), calibration_polls);
        write_volatile(addr_of_mut!((*r).enables), enables);
    });
}

/// The converter up once per boot; in the probe flavor the flight check
/// powers it itself.
fn bring_up() {
    if UP.swap(true, Ordering::AcqRel) || cfg!(feature = "test-asv-probe") {
        return;
    }
    let _ = power_up(true);
}

/// The converter brought up and the channel's pin put into analog mode: a
/// plain input without a pull first (the GPIO backend enables the port's
/// clock), then the mode field. An internal channel has no pin.
pub fn configure(index: usize) {
    bring_up();
    let c = &facts::ADC_CHANNELS[index];
    if c.line == adc_math::NO_LINE {
        return;
    }
    pins::configure(c.line as u32, g::MODE_INPUT, g::PULL_NONE, false);
    let (moder, shift) = mode_field(c.line as u32);
    with_cs(|| unsafe { write_volatile(moder, read_volatile(moder) | (MODE_ANALOG << shift)) });
}

/// The channel out of the turn and the queue; its pin back to a plain
/// input without a pull.
pub fn release(index: usize) {
    TURNS.forget(0, index);
    let c = &facts::ADC_CHANNELS[index];
    if c.line != adc_math::NO_LINE {
        pins::release(c.line as u32);
    }
}

fn kick(index: usize) {
    let ch = facts::ADC_CHANNELS[index].channel as u32;
    with_cs(|| u5::kick(&mut Chip, BASE, ch));
}

/// The block taken and the conversion started, or the channel queued
/// behind the conversion in flight.
pub fn start(index: usize) {
    if TURNS.start(0, index) {
        kick(index);
    } else {
        with_cs(|| unsafe { bump(addr_of_mut!((*addr_of_mut!(RECORD)).waited)) });
    }
}

/// The owner's conversion is done when the end flag is set; a queued
/// channel takes the block and starts once it is free.
pub fn ready(index: usize) -> bool {
    if TURNS.owns(0, index) {
        return u5::done(&mut Chip, BASE);
    }
    if TURNS.promote(0, index) {
        kick(index);
    }
    false
}

/// The sample and the block given to the next channel -- or none for a
/// conversion the block flagged: counted and discarded, the next started
/// with the block kept when the function asks for another attempt, the
/// block given back otherwise.
pub fn sample(index: usize, again: bool) -> Option<u32> {
    let s = u5::take(&mut Chip, BASE);
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
    TURNS.give(0, index);
    #[cfg(feature = "test-adc-doctored")]
    let s = s.map(|v| PREVIOUS.swap(v, Ordering::AcqRel));
    s
}

/// A block reset after a conversion passed its budget: the turn and the
/// queue cleared; the converter stopped and disabled, then powered up
/// again, the calibration with it. The probe flavor leaves the power to
/// the flight check.
pub fn reset(_block: usize) {
    TURNS.clear(0, |_| true);
    with_cs(|| unsafe { bump(addr_of_mut!((*addr_of_mut!(RECORD)).resets)) });
    if cfg!(feature = "test-asv-probe") {
        return;
    }
    let _ = u5::power_down(&mut Chip, &facts::ADC_FACTS, false, BOUND_READS);
    let _ = power_up(true);
}
