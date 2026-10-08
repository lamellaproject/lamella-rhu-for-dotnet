//! The PWM doors' pure arithmetic and facts shapes: the timer and line
//! descriptors a board's facts state, the quantization of a period in
//! nanoseconds to a prescaler and a period register on a timer's clock, the
//! achieved period back from them, the duty's compare value, the
//! shared-period comparison and the tables' soundness. No hardware access and no state, covered by the host tests;
//! the drivers cite the manuals for the numbers here.
#![allow(dead_code)]

/// A timer as a board's facts state it: its NUMBER (the chip's own: TIM1 is
/// timer 1, a slice its index; the number the info door answers and the
/// device files present), its input clock in Hz at this posture, how many
/// output channels it has, and the reach of its prescaler and period
/// registers as VALUES (a 16-bit register holds the value minus one, so
/// its reach is 65536). The table position is the index the line table
/// names.
#[derive(Clone, Copy, Debug)]
pub struct PwmTimer {
    pub number: u32,
    pub clock_hz: u32,
    pub channels: u32,
    pub prescaler_max: u32,
    pub period_max: u32,
}

/// A line a timer can drive: the GPIO line, its timer (the timer table's
/// index), the channel on that timer (0-based) and the alternate function
/// the pad runs at (0 on a row whose models have no pins). The table
/// position is the enumeration index the count door and the line door walk,
/// and the handle a claim answers.
#[derive(Clone, Copy, Debug)]
pub struct PwmLine {
    pub line: u16,
    pub timer: u8,
    pub channel: u8,
    pub alternate: u8,
}

/// The polarities the header states: the output high for the duty
/// (normal) or low for it (inversed), the Linux PWM class's two words.
pub const POLARITY_NORMAL: u32 = 0;
pub const POLARITY_INVERSED: u32 = 1;

/// The claims table's reach: sixteen timers of eight channels (the largest
/// family in hand has twelve timers with outputs; the advanced timers have
/// six channels).
pub const MAX_TIMERS: usize = 16;
pub const MAX_CHANNELS: usize = 8;

pub const fn polarity_valid(polarity: u32) -> bool {
    polarity <= POLARITY_INVERSED
}

/// A duration in nanoseconds as ticks of a clock, rounded to nearest.
pub const fn ticks(ns: u64, clock_hz: u32) -> u64 {
    ((ns as u128 * clock_hz as u128 + 500_000_000) / 1_000_000_000) as u64
}

/// The quantization: the period's ticks on the timer's clock; the smallest
/// prescaler that brings them inside the period register; the period
/// register as the ticks over the prescaler, rounded to nearest, never
/// below 1. None when the period rounds to no tick at all, or when even
/// the largest prescaler cannot hold it. The pair is (prescaler, period)
/// as VALUES.
pub const fn quantize(period_ns: u64, t: &PwmTimer) -> Option<(u32, u32)> {
    let ticks = ticks(period_ns, t.clock_hz);
    if ticks == 0 {
        return None;
    }
    let amax = t.period_max as u64;
    let p = ticks.div_ceil(amax);
    if p > t.prescaler_max as u64 {
        return None;
    }
    let mut arr = (ticks + p / 2) / p;
    if arr < 1 {
        arr = 1;
    }
    if arr > amax {
        arr = amax;
    }
    Some((p as u32, arr as u32))
}

/// The period a pair runs, in nanoseconds: prescaler times period ticks of
/// the clock, rounded to nearest (exact wherever the clock divides 1e9).
pub const fn achieved_ns(prescaler: u32, period: u32, clock_hz: u32) -> u64 {
    let ticks = prescaler as u128 * period as u128;
    ((ticks * 1_000_000_000 + clock_hz as u128 / 2) / clock_hz as u128) as u64
}

/// The duty's compare value on a pair: the duty's ticks over the prescaler,
/// rounded to nearest, never past the period (a compare equal to the period
/// is the output high for the whole period).
pub const fn compare(duty_ns: u64, clock_hz: u32, prescaler: u32, period: u32) -> u32 {
    let t = ticks(duty_ns, clock_hz);
    let c = (t + prescaler as u64 / 2) / prescaler as u64;
    if c > period as u64 {
        period
    } else {
        c as u32
    }
}

/// Two channels of one timer agree on the period they run when their pairs
/// are equal (two requests that quantize alike agree).
pub const fn same_pair(a: (u32, u32), b: (u32, u32)) -> bool {
    let ((ap, aa), (bp, ba)) = (a, b);
    ap == bp && aa == ba
}

/// The table position of the line a number names, or None when no timer
/// drives that line (the claim then answers RHU_EINVAL).
pub fn line_index(table: &[PwmLine], line: u32) -> Option<usize> {
    table.iter().position(|l| l.line as u32 == line)
}

/// The claims table's bit for a timer channel.
pub const fn channel_bit(timer: usize, channel: usize) -> usize {
    timer * MAX_CHANNELS + channel
}

/// The invariants a growing table can break, asserted at compile time where
/// the tables are consumed: at most sixteen timers of at most eight
/// channels, every timer's registers reaching at least 1, every line's
/// timer inside the timer table and its channel inside that timer, and no
/// two entries for one line. Two lines MAY share one channel: a chip that
/// routes one timer output to two pads lists both, and the claims table
/// keeps the channel exclusive (the second line's claim is refused busy).
pub const fn table_sound(lines: &[PwmLine], timers: &[PwmTimer]) -> bool {
    if timers.len() > MAX_TIMERS {
        return false;
    }
    let mut i = 0;
    while i < timers.len() {
        let t = &timers[i];
        if t.channels as usize > MAX_CHANNELS || t.prescaler_max == 0 || t.period_max == 0 || t.clock_hz == 0 {
            return false;
        }
        i += 1;
    }
    i = 0;
    while i < lines.len() {
        let l = &lines[i];
        if l.timer as usize >= timers.len() || l.channel as u32 >= timers[l.timer as usize].channels {
            return false;
        }
        let mut j = i + 1;
        while j < lines.len() {
            if lines[j].line == l.line {
                return false;
            }
            j += 1;
        }
        i += 1;
    }
    true
}
