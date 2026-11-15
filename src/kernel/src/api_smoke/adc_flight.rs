//! The ST converter rows' flight check, the flight flavor of the row's api
//! image (`test-adc-flight`): the kernel's analog API over the chip's own
//! converters on the board's connector pins, with nothing wired -- the
//! converters' power-up read from the backend's record, the table stated
//! and enumerated, every channel of the check's own list claimed and
//! converted inside its budget, the chip's internal reference read inside
//! the band its datasheet gives -- and one case on a jumper wire from a PWM
//! pin to the first analog pin, which reads skipped when the wire is not
//! there. The image runs this check alone.
//!
//! THE VERDICTS. Every case ends on one line: PASS, REFUSED (the row did
//! what the case forbids), SKIPPED (not flown, with the reason) or OBSERVED
//! (a reading that decides nothing). A refusal does not stop the check; it
//! ends with its summary, the END line and the exit function, 0 without a
//! refusal.
//!
//! A CHANNEL IS CLAIMED ONLY THROUGH THE CHECK's OWN LIST
//! (`facts::ADC_FLIGHT`, read by `flown`). A row leaves out of it the
//! positions whose pins must not be put into analog mode as the board
//! stands on the bench; each prints one SKIPPED line, and `adc0-modes`
//! reads their pins' mode fields at the end. The wire case holds one PWM
//! line, `facts::ADC_WIRE_OUT`, and no other.
#![cfg_attr(feature = "test-asv-probe", allow(dead_code, unused_imports))]

use core::fmt;
use core::ptr::{addr_of, read_volatile};

use crate::adc_math::NO_LINE;
use crate::board::adc_backend as hw;
use crate::board::facts;
use crate::pwm_math::POLARITY_NORMAL;
use crate::stm32_adc_math as m;
use crate::{api, gpio_math, sched};

/// The busy code of a claim another holder refuses.
const BUSY: i32 = crate::adc::ERR_BUSY;
/// A millisecond, in the functions' nanoseconds.
const MS: u64 = 1_000_000;
/// The port's mode field values the check reads: a plain input, and analog.
const MODE_INPUT: u32 = 0b00;
const MODE_ANALOG: u32 = 0b11;
/// The reads of one channel in `adc0-convert` and of the reference.
const READS: usize = 8;

/// The row's reading limits, each from the manufacturer's page: the
/// internal reference's voltage range in microvolts, the converter's total
/// unadjusted error in counts, the typical reference voltage, and the two
/// levels of the jumper wire -- the most a held-low wire reads and the
/// least a held-high one does, in microvolts at the nominal supply.
#[cfg(feature = "board-st-stm32f769i-disco")]
mod limits {
    /// DS11532 Table 81: 1.18 V to 1.24 V, typically 1.21 V.
    pub const REFERENCE_UV: (u32, u32, u32) = (1_180_000, 1_240_000, 1_210_000);
    /// DS11532 Table 75: 7 counts at the fastest converter clock.
    pub const ERROR: u32 = 7;
    /// The wire runs through the seated shield's level translator, port A
    /// at a 3.3 V rail (its data sheet's electrical characteristics: an
    /// output low of 0.55 V at most, an output high of 0.67 of the rail at
    /// least).
    pub const WIRE_UV: (u32, u32) = (550_000, 2_211_000);
}
#[cfg(feature = "board-st-nucleo-u5a5zj-q")]
mod limits {
    /// DS13543 Table 37: 1.175 V to 1.255 V, typically 1.215 V.
    pub const REFERENCE_UV: (u32, u32, u32) = (1_175_000, 1_255_000, 1_215_000);
    /// DS13543 Table 99: 12 counts, single ended.
    pub const ERROR: u32 = 12;
    /// The wire is plain: the chip's own output (DS13543 Table 88: 0.4 V at
    /// most low and the supply less 0.4 V at least high, at 8 mA).
    pub const WIRE_UV: (u32, u32) = (400_000, 2_900_000);
}

struct Flight {
    pass: u32,
    refused: u32,
    skipped: u32,
    observed: u32,
}

impl Flight {
    fn pass(&mut self, case: &str, what: fmt::Arguments) {
        self.pass += 1;
        kprintln!("[flight] PASS {} ({})", case, what);
    }

    fn refused(&mut self, case: &str, what: fmt::Arguments) {
        self.refused += 1;
        kprintln!("[flight] REFUSED {}: {}", case, what);
    }

    #[allow(dead_code)]
    fn skipped(&mut self, case: &str, why: fmt::Arguments) {
        self.skipped += 1;
        kprintln!("[flight] SKIPPED {}: not flown: {}", case, why);
    }

    #[allow(dead_code)]
    fn observed(&mut self, case: &str, what: fmt::Arguments) {
        self.observed += 1;
        kprintln!("[flight] OBSERVED {}: {}", case, what);
    }
}

pub fn run() {
    let mut f = Flight { pass: 0, refused: 0, skipped: 0, observed: 0 };
    kprintln!("[flight] the analog flight check starts {} ms after the boot", sched::now_ms());
    #[cfg(feature = "test-asv-probe")]
    asv(&mut f);
    #[cfg(not(feature = "test-asv-probe"))]
    {
        power(&mut f);
        table(&mut f);
        for ix in 0..facts::ADC_CHANNELS.len() {
            if facts::ADC_CHANNELS[ix].line == NO_LINE {
                continue;
            }
            if flown(ix) {
                channel(&mut f, ix);
            } else {
                f.skipped("adc0-convert", format_args!("position {} (line {}) is left out of the check's list: as the board stands its pin must not be put into analog mode", ix, facts::ADC_CHANNELS[ix].line));
            }
        }
        reference(&mut f);
        modes(&mut f);
        busy(&mut f);
        released(&mut f);
        wire(&mut f);
    }
    kprintln!("[flight] SUMMARY leg adc: {} PASS, {} REFUSED, {} SKIPPED, {} OBSERVED", f.pass, f.refused, f.skipped, f.observed);
    kprintln!("[flight] END");
    api::rhu_exit(if f.refused == 0 { 0 } else { 1 })
}

/// Whether the check may claim a table position: the row's own list.
fn flown(ix: usize) -> bool {
    facts::ADC_FLIGHT.contains(&ix)
}

/// The full scale of a position's block.
fn full_of(ix: usize) -> u32 {
    crate::adc_math::full_scale(facts::ADC_BLOCKS[facts::ADC_CHANNELS[ix].block as usize].bits)
}

/// The table position of the internal reference, when the check may claim it.
fn reference_position() -> Option<usize> {
    (0..facts::ADC_CHANNELS.len()).find(|&i| facts::ADC_CHANNELS[i].line == NO_LINE && flown(i))
}

/// Every block claimed once through its first channel of the list, so that
/// the backend has brought every converter up before its record is read.
#[cfg(not(feature = "test-asv-probe"))]
fn wake() -> u32 {
    let mut wrong = 0;
    for block in 0..facts::ADC_BLOCKS.len() {
        if let Some(ix) = (0..facts::ADC_CHANNELS.len()).find(|&i| flown(i) && facts::ADC_CHANNELS[i].block as usize == block) {
            let h = api::rhu_adc_claim(ix as u32);
            wrong += (h != ix as i32 || api::rhu_adc_release(h) != 0) as u32;
        }
    }
    wrong
}

/// The converters' power-up, from the backend's record after a first claim
/// of each block: both blocks on, the common prescaler at the APB2 clock
/// over 4 and the internal reference enabled, as the registers read back.
#[cfg(all(not(feature = "test-asv-probe"), feature = "board-st-stm32f769i-disco"))]
fn power(f: &mut Flight) {
    use m::f7;
    let wrong = wake();
    let (up, common, on) = hw::probe_power();
    let all = (1u32 << facts::ADC_BLOCKS.len()) - 1;
    if wrong == 0 && up == all && on == all && common & (f7::CCR_ADCPRE | f7::CCR_TSVREFE) == f7::CCR_ADCPRE_4 | f7::CCR_TSVREFE {
        f.pass("adc0-power", format_args!("{} converter blocks brought up at their first claim and reading on; the common control register reads {:#010x}: the prescaler at the bus clock over 4, the internal reference enabled", facts::ADC_BLOCKS.len(), common));
    } else {
        f.refused("adc0-power", format_args!("the record's bring-up word reads {:#x} and the blocks read on as {:#x} ({:#x} wanted for both); the common control register reads {:#010x} (the prescaler field 01 and the reference's enable wanted); {} first claims or releases answered wrongly", up, on, all, common, wrong));
    }
}

/// The converter's power-up, from the backend's record after a first
/// claim: the analog supply declared valid, the regulator, the calibration
/// and the enable each inside its bound.
#[cfg(all(not(feature = "test-asv-probe"), feature = "board-st-nucleo-u5a5zj-q"))]
fn power(f: &mut Flight) {
    let wrong = wake();
    let (stage, supply, regulator, calibration, enables) = hw::probe_power();
    if wrong == 0 && stage == m::u5::STAGE_UP && supply > 0 && regulator > 0 && calibration > 0 && enables > 0 {
        f.pass("adc0-power", format_args!("the analog supply read ready after {} status reads and is declared valid; the regulator ready after {} reads, the calibration ended after {}, the converter ready after {} enables", supply, regulator, calibration, enables));
    } else {
        f.refused("adc0-power", format_args!("the power-up ended on stage {} (0 up; 1 the regulator, 2 the calibration, 3 the enable missed its bound; 4 the analog supply not ready) with {} supply reads, {} regulator reads, {} calibration reads and {} enables; {} first claims or releases answered wrongly", stage, supply, regulator, calibration, enables, wrong));
    }
}

/// The count, the enumeration of every position's facts, the refusal past
/// the count, each block's reference voltage and the refusal of a block
/// the row does not have.
#[cfg(not(feature = "test-asv-probe"))]
fn table(f: &mut Flight) {
    let channels = facts::ADC_CHANNELS;
    let n = api::rhu_adc_count();
    let mut wrong = (n != channels.len() as i32) as u32;
    let (mut b, mut ch, mut line, mut bits) = (0u32, 0u32, 0u32, 0u32);
    for (i, c) in channels.iter().enumerate() {
        let block = &facts::ADC_BLOCKS[c.block as usize];
        let r = api::rhu_adc_channel(i as u32, &mut b, &mut ch, &mut line, &mut bits);
        let want_line = if c.line == NO_LINE { u32::MAX } else { c.line as u32 };
        wrong += (r != 0 || b != block.number || ch != c.channel as u32 || line != want_line || bits != block.bits) as u32;
    }
    wrong += (api::rhu_adc_channel(channels.len() as u32, &mut b, &mut ch, &mut line, &mut bits) != sched::ERR_INVAL) as u32;
    let mut mv = 0u32;
    let mut unknown = 0u32;
    for block in facts::ADC_BLOCKS {
        wrong += (api::rhu_adc_reference(block.number, &mut mv) != 0 || mv != block.reference_mv) as u32;
        unknown = unknown.max(block.number + 1);
    }
    wrong += (api::rhu_adc_reference(unknown, &mut mv) != sched::ERR_NODEV) as u32;
    if wrong == 0 {
        f.pass("adc0-table", format_args!("{} channels on {} blocks stated and enumerated with their block, channel, line and width; a position past the count refused; each block's reference {} mV; block {} refused as no device", channels.len(), facts::ADC_BLOCKS.len(), mv, unknown));
    } else {
        f.refused("adc0-table", format_args!("the count reads {} against a table of {} channels; {} of the count, the positions' facts, the refusal past the count, the references and the unknown block's refusal differ", n, channels.len(), wrong));
    }
}

/// Eight reads of a held channel: how many answered a sample inside the
/// full scale, the smallest and the largest, and the last code that was
/// not one.
fn reads(h: i32, full: u32) -> (u32, u32, u32, i32) {
    let (mut good, mut min, mut max, mut code) = (0u32, u32::MAX, 0u32, 0i32);
    for _ in 0..READS {
        let s = api::rhu_adc_read(h);
        if s >= 0 && s as u32 <= full {
            good += 1;
            min = min.min(s as u32);
            max = max.max(s as u32);
        } else {
            code = s;
        }
    }
    (good, min, max, code)
}

/// One pin channel of the list, alone: claimed, its pin's mode field read
/// analog; eight reads, each inside its budget and the full scale, none
/// flagged; released, the mode field read a plain input again.
#[cfg(not(feature = "test-asv-probe"))]
fn channel(f: &mut Flight, ix: usize) {
    let c = &facts::ADC_CHANNELS[ix];
    let line = c.line as u32;
    let h = api::rhu_adc_claim(ix as u32);
    let mode = hw::probe_mode(line);
    if h != ix as i32 || mode != MODE_ANALOG {
        f.refused("adc0-claim", format_args!("position {} (line {}): the claim answered {} and the pin's mode field reads {} (the position and 3, analog, wanted)", ix, line, h, mode));
        if h >= 0 {
            let _ = api::rhu_adc_release(h);
        }
        return;
    }
    f.pass("adc0-claim", format_args!("position {} (line {}): handle {}, block {} channel {}, the pin's mode field analog", ix, line, h, facts::ADC_BLOCKS[c.block as usize].number, c.channel));
    let full = full_of(ix);
    let flagged = hw::probe_flagged();
    let (good, min, max, code) = reads(h, full);
    let flagged = hw::probe_flagged() - flagged;
    let rel = api::rhu_adc_release(h);
    let after = hw::probe_mode(line);
    if good == READS as u32 && flagged == 0 && rel == 0 && after == MODE_INPUT {
        f.pass("adc0-convert", format_args!("position {} (line {}): {} conversions inside their budget, none flagged, reading {} to {} of {}; released, the pin a plain input again", ix, line, good, min, max, full));
    } else {
        f.refused("adc0-convert", format_args!("position {} (line {}): {} of {} reads answered a sample inside the full scale {} (the last other code {}), {} conversions flagged; the release answered {} and the pin's mode field then reads {} (0, a plain input, wanted)", ix, line, good, READS, full, code, flagged, rel, after));
    }
}

/// The internal reference: claimed, read nine times, the first discarded
/// and the eight after it each inside the band the datasheet's limits give
/// at the board's nominal reference voltage.
#[cfg(not(feature = "test-asv-probe"))]
fn reference(f: &mut Flight) {
    let Some(ix) = reference_position() else {
        f.skipped("adc0-reference", format_args!("the check's list names no internal reference"));
        return;
    };
    let block = &facts::ADC_BLOCKS[facts::ADC_CHANNELS[ix].block as usize];
    let (lo, hi) = m::band(limits::REFERENCE_UV.0, limits::REFERENCE_UV.1, block.reference_mv, block.bits, limits::ERROR);
    let h = api::rhu_adc_claim(ix as u32);
    let first = if h >= 0 { api::rhu_adc_read(h) } else { h };
    let (good, min, max, code) = if h >= 0 { reads(h, full_of(ix)) } else { (0, 0, 0, h) };
    let rel = if h >= 0 { api::rhu_adc_release(h) } else { h };
    if h == ix as i32 && good == READS as u32 && min >= lo && max <= hi && rel == 0 {
        let supply = limits::REFERENCE_UV.2 as u64 * full_of(ix) as u64 / ((min + max) as u64 / 2) / 1000;
        f.pass("adc0-reference", format_args!("the internal reference at position {}: the first reading {} discarded, the eight after it {} to {}, inside the band {} to {} of {}; at the datasheet's typical reference the supply reads about {} mV", ix, first, min, max, lo, hi, full_of(ix), supply));
    } else {
        f.refused("adc0-reference", format_args!("the internal reference at position {}: the claim answered {}, the first reading {}, {} of {} readings inside the full scale reading {} to {} (the band {} to {} wanted; the last other code {}), the release {}", ix, h, first, good, READS, min, max, lo, hi, code, rel));
    }
}

/// The pins of the positions the list leaves out were never put into
/// analog mode: their mode fields read a plain input at the end.
#[cfg(not(feature = "test-asv-probe"))]
fn modes(f: &mut Flight) {
    let (mut out, mut wrong) = (0u32, 0u32);
    for (ix, c) in facts::ADC_CHANNELS.iter().enumerate() {
        if c.line != NO_LINE && !flown(ix) {
            out += 1;
            wrong += (hw::probe_mode(c.line as u32) != MODE_INPUT) as u32;
        }
    }
    if out == 0 {
        return;
    }
    if wrong == 0 {
        f.pass("adc0-modes", format_args!("the {} pins the check's list leaves out read a plain input in their mode fields: never in analog mode", out));
    } else {
        f.refused("adc0-modes", format_args!("{} of the {} pins the check's list leaves out do not read a plain input in their mode fields", wrong, out));
    }
}

/// A held channel refuses a second claim, and its line refuses a GPIO
/// claim with the holder named.
#[cfg(not(feature = "test-asv-probe"))]
fn busy(f: &mut Flight) {
    let Some(ix) = (0..facts::ADC_CHANNELS.len()).find(|&i| flown(i) && facts::ADC_CHANNELS[i].line != NO_LINE && i != facts::ADC_WIRE_IN) else {
        f.skipped("adc0-busy", format_args!("the check's list names no pin channel beside the wire's"));
        return;
    };
    let line = facts::ADC_CHANNELS[ix].line as u32;
    let h = api::rhu_adc_claim(ix as u32);
    let again = api::rhu_adc_claim(ix as u32);
    let gpio = api::rhu_gpio_claim(line, gpio_math::MODE_INPUT, gpio_math::PULL_NONE, 0);
    let holder = unsafe { read_volatile(addr_of!((*addr_of!(crate::gpio::WITNESS)).last_busy_holder)) };
    if gpio == 0 {
        let _ = api::rhu_gpio_release(line);
    }
    let rel = if h >= 0 { api::rhu_adc_release(h) } else { h };
    if h == ix as i32 && again == BUSY && gpio == BUSY && holder == crate::gpio::HOLDER_ADC && rel == 0 {
        f.pass("adc0-busy", format_args!("position {} held: a second claim of it refused with the busy code, and a GPIO claim of its line {} refused with the busy code, the holder named as an analog channel", ix, line));
    } else {
        f.refused("adc0-busy", format_args!("position {} (line {}): the claim answered {}, the second claim {} and the GPIO claim {} (the busy code {} wanted for both) with the holder word {} ({} wanted), the release {}", ix, line, h, again, gpio, BUSY, holder, crate::gpio::HOLDER_ADC, rel));
    }
}

/// Every channel of the list given back: each claims and releases once
/// more, and the analog witness counts as many releases as claims.
#[cfg(not(feature = "test-asv-probe"))]
fn released(f: &mut Flight) {
    let (mut n, mut wrong) = (0u32, 0u32);
    for &ix in facts::ADC_FLIGHT {
        let h = api::rhu_adc_claim(ix as u32);
        n += 1;
        wrong += (h != ix as i32 || api::rhu_adc_release(h) != 0) as u32;
    }
    let (claims, releases) = unsafe {
        let w = addr_of!(crate::adc::WITNESS);
        (read_volatile(addr_of!((*w).claims_taken)), read_volatile(addr_of!((*w).releases)))
    };
    if wrong == 0 && claims == releases {
        f.pass("adc0-release", format_args!("{} channels claimed and given back once more; the witness counts {} claims and {} releases", n, claims, releases));
    } else {
        f.refused("adc0-release", format_args!("{} of {} channels did not claim and release; the witness counts {} claims and {} releases", wrong, n, claims, releases));
    }
}

/// The jumper wire from the PWM pin to the first analog pin. The wire is
/// looked for first -- the PWM pin taken as a plain output and driven high
/// and low eight times, the analog pin as a plain input must follow each
/// time -- and the case is skipped without it. With it the PWM channel
/// holds the pin: high (a reading that is printed and not judged), low,
/// high, low, each read 2 ms after the change; the low readings inside the
/// low band and the high one inside the high band.
#[cfg(not(feature = "test-asv-probe"))]
fn wire(f: &mut Flight) {
    let (out, ix) = (facts::ADC_WIRE_OUT, facts::ADC_WIRE_IN);
    let inp = facts::ADC_CHANNELS[ix].line as u32;
    let c_in = api::rhu_gpio_claim(inp, gpio_math::MODE_INPUT, gpio_math::PULL_NONE, 0);
    let c_out = api::rhu_gpio_claim(out, gpio_math::MODE_OUTPUT, gpio_math::PULL_NONE, 0);
    if c_in != 0 || c_out != 0 {
        if c_in == 0 {
            let _ = api::rhu_gpio_release(inp);
        }
        if c_out == 0 {
            let _ = api::rhu_gpio_release(out);
        }
        f.refused("adc1-wire", format_args!("lines {} and {} could not be taken as plain lines: the claims answered {} and {}", out, inp, c_out, c_in));
        return;
    }
    let mut follows = 0u32;
    for i in 0..8u32 {
        let level = (i + 1) & 1;
        let _ = api::rhu_gpio_write(out, level);
        sched::sleep_ms(2);
        follows += (api::rhu_gpio_read(inp) == level as i32) as u32;
    }
    let r_out = api::rhu_gpio_release(out);
    let r_in = api::rhu_gpio_release(inp);
    if follows != 8 {
        f.skipped("adc1-wire", format_args!("no jumper wire from line {} to line {}: driven high and low eight times as a plain output, the input followed {} times of 8", out, inp, follows));
        return;
    }
    let full = full_of(ix);
    let mv = facts::ADC_BLOCKS[facts::ADC_CHANNELS[ix].block as usize].reference_mv;
    let bits = facts::ADC_BLOCKS[facts::ADC_CHANNELS[ix].block as usize].bits;
    let low = m::band(0, limits::WIRE_UV.0, mv, bits, limits::ERROR);
    let high = m::band(limits::WIRE_UV.1, mv * 1000, mv, bits, limits::ERROR);
    let p = api::rhu_pwm_claim(out);
    let a = api::rhu_adc_claim(ix as u32);
    let mut got = [0i32; 4];
    let mut codes = r_out == 0 && r_in == 0 && p >= 0 && a == ix as i32;
    if codes {
        for (i, duty) in [MS, 0, MS, 0].into_iter().enumerate() {
            codes &= api::rhu_pwm_set(p, MS, duty, POLARITY_NORMAL) == 0 && api::rhu_pwm_enable(p, 1) == 0;
            sched::sleep_ms(2);
            got[i] = api::rhu_adc_read(a);
        }
    }
    let r_a = if a >= 0 { api::rhu_adc_release(a) } else { a };
    let r_p = if p >= 0 { api::rhu_pwm_release(p) } else { p };
    codes &= r_a == 0 && r_p == 0;
    let inside = |s: i32, b: (u32, u32)| s >= 0 && s as u32 >= b.0 && s as u32 <= b.1;
    if codes && inside(got[1], low) && inside(got[2], high) && inside(got[3], low) {
        f.pass("adc1-wire", format_args!("line {} held by its PWM channel and position {} (line {}) reading it on the wire: first held high it read {} (not judged); held low {}, held high {}, held low again {}, of {} -- the low readings inside 0 to {} and the high one inside {} to {}", out, ix, inp, got[0], got[1], got[2], got[3], full, low.1, high.0, high.1));
    } else {
        f.refused("adc1-wire", format_args!("line {} to position {} (line {}): first held high it read {}; held low {} (0 to {} wanted), held high {} ({} to {} wanted), held low again {} (0 to {} wanted), of {}; every claim, set, enable and release answered as wanted: {} (the PWM claim {}, the analog claim {})", out, ix, inp, got[0], got[1], low.1, got[2], high.0, high.1, got[3], low.1, full, codes, p, a));
    }
}

/// The analog supply's probe (the STM32U5 row, the probe flavor). The
/// valid bit cleared if an earlier program left it set across the reset;
/// the converter powered up without the supply's step and the internal
/// reference read; the converter powered down; the supply's step and the
/// power-up; the reference read again.
#[cfg(feature = "test-asv-probe")]
fn asv(f: &mut Flight) {
    let s = &facts::ANALOG_SUPPLY;
    let (found, cleared) = unsafe {
        let clock = s.clock_reg as *mut u32;
        core::ptr::write_volatile(clock, core::ptr::read_volatile(clock) | (1 << s.clock_bit));
        let _ = core::ptr::read_volatile(clock);
        let control = s.control_reg as *mut u32;
        let found = core::ptr::read_volatile(control);
        let set = found & (1 << s.valid_bit) != 0;
        if set {
            core::ptr::write_volatile(control, found & !(1 << s.valid_bit));
        }
        (found, set)
    };
    kprintln!("[flight] asv: the claim leaves the power-up to this check; the monitoring control register reads {:#010x} as found (the analog supply's valid bit {}, its monitor's enable {})", found, (found >> s.valid_bit) & 1, (found >> s.monitor_bit) & 1);
    if cleared {
        kprintln!("[flight] asv: the valid bit was found set, left by an earlier program across the reset, and is cleared for the probe");
    }
    let Some(ix) = reference_position() else {
        f.refused("asv-probe", format_args!("the check's list names no internal reference to read"));
        return;
    };
    let block = &facts::ADC_BLOCKS[facts::ADC_CHANNELS[ix].block as usize];
    let (lo, hi) = m::band(limits::REFERENCE_UV.0, limits::REFERENCE_UV.1, block.reference_mv, block.bits, limits::ERROR);
    let try_read = |supply: bool| -> (u32, i32, i32) {
        let stage = hw::power_up(supply);
        let h = api::rhu_adc_claim(ix as u32);
        let (first, second) = if h >= 0 && stage == m::u5::STAGE_UP { (api::rhu_adc_read(h), api::rhu_adc_read(h)) } else { (h.min(-1), h.min(-1)) };
        if h >= 0 {
            let _ = api::rhu_adc_release(h);
        }
        (stage, first, second)
    };
    let before = try_read(false);
    let down = hw::power_down();
    let after = try_read(true);
    let (_, supply_polls, _, _, _) = hw::probe_power();
    kprintln!("[flight] asv: before the step the power-up ended on stage {} and the reference read {} then {}; powered down: {}; after the step (the supply ready after {} status reads) stage {} and the reference read {} then {}; the band is {} to {}", before.0, before.1, before.2, down, supply_polls, after.0, after.1, after.2, lo, hi);
    let inside = |t: (u32, i32, i32)| t.0 == m::u5::STAGE_UP && t.2 >= lo as i32 && t.2 <= hi as i32;
    match (inside(before), inside(after), down) {
        (false, true, true) => f.pass("asv-probe", format_args!("ASV-0: the converter answers AFTER the step and NOT BEFORE -- with the analog supply's valid bit clear the power-up ended on stage {} and the reference read {}; with it set, stage 0 and {} inside the band {} to {}: the analog blocks are isolated until the step", before.0, before.2, after.2, lo, hi)),
        (true, true, true) => f.observed("asv-probe", format_args!("ASV-0: the converter answers BEFORE the step too -- the reference read {} with the valid bit clear and {} with it set, both inside the band {} to {}", before.2, after.2, lo, hi)),
        _ => f.refused("asv-probe", format_args!("neither verdict: before the step stage {} and the reference {}; the power-down answered {}; after the step stage {} and the reference {} (the band {} to {})", before.0, before.2, down, after.0, after.2, lo, hi)),
    }
}
