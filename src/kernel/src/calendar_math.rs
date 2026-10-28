//! Calendar arithmetic, pure: the calendar tick (100 nanoseconds since
//! 0001-01-01T00:00:00Z, the hosted runtime's own unit) to and from a
//! civil date and time in the proleptic Gregorian calendar, the
//! two-digit packing the ST real-time clocks use for every field, and
//! the century those clocks keep. No state and no peripheral: the host
//! tier compiles this file from its own source and pins its cases.
#![allow(dead_code)] // the host tier exercises the whole surface; each row uses what its clock needs

/// One second in ticks.
pub const TICKS_PER_SECOND: u64 = 10_000_000;
/// One day in ticks.
pub const TICKS_PER_DAY: u64 = 864_000_000_000;
/// 1970-01-01T00:00:00Z.
pub const TICKS_UNIX_EPOCH: u64 = 621_355_968_000_000_000;
/// 2000-01-01T00:00:00Z: the ST real-time clocks' century base.
pub const TICKS_2000: u64 = 630_822_816_000_000_000;
/// 9999-12-31T23:59:59.9999999Z, the last representable instant (the
/// hosted runtime's maximum).
pub const TICKS_MAX: u64 = 3_155_378_975_999_999_999;

/// A civil date and time, UTC, year 1 to 9999.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Civil {
    pub year: u32,
    pub month: u32,
    pub day: u32,
    pub hour: u32,
    pub minute: u32,
    pub second: u32,
}

/// Days since 0001-01-01 (day 0) of a civil date, proleptic Gregorian:
/// the integer algorithm over 400-year eras with March as the first
/// month of the computational year, shifted from the era origin
/// (0000-03-01) to day 0 by the 306 days between them. Valid for every
/// date the tick range covers.
pub fn days_from_civil(year: u32, month: u32, day: u32) -> u64 {
    let y: i64 = if month <= 2 { year as i64 - 1 } else { year as i64 };
    let era = y.div_euclid(400);
    let yoe = y.rem_euclid(400) as u64;
    let m = month as u64;
    let shifted = if m > 2 { m - 3 } else { m + 9 };
    let doy = (153 * shifted + 2) / 5 + day as u64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    (era * 146_097 + doe as i64 - 306) as u64
}

/// The civil date of a day count since 0001-01-01: the inverse of
/// `days_from_civil`, the same eras.
pub fn civil_from_days(days: u64) -> (u32, u32, u32) {
    let z = days + 306;
    let era = z / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if m <= 2 { y + 1 } else { y };
    (year as u32, m as u32, d as u32)
}

/// The weekday of a day count, Monday = 1 through Sunday = 7 (the ST
/// clocks' encoding): day 0, 0001-01-01, was a Monday.
pub fn weekday_from_days(days: u64) -> u32 {
    (days % 7) as u32 + 1
}

/// Ticks of a civil date and time.
pub fn civil_to_ticks(c: Civil) -> u64 {
    days_from_civil(c.year, c.month, c.day) * TICKS_PER_DAY
        + (c.hour as u64 * 3600 + c.minute as u64 * 60 + c.second as u64) * TICKS_PER_SECOND
}

/// The civil date and time of a tick count (the sub-second part dropped).
pub fn ticks_to_civil(ticks: u64) -> Civil {
    let days = ticks / TICKS_PER_DAY;
    let seconds = (ticks % TICKS_PER_DAY) / TICKS_PER_SECOND;
    let (year, month, day) = civil_from_days(days);
    Civil {
        year,
        month,
        day,
        hour: (seconds / 3600) as u32,
        minute: ((seconds / 60) % 60) as u32,
        second: (seconds % 60) as u32,
    }
}

/// Two decimal digits packed a nibble each (0..=99).
pub fn bcd(v: u32) -> u32 {
    ((v / 10) << 4) | (v % 10)
}

/// The inverse of `bcd`.
pub fn unbcd(b: u32) -> u32 {
    (b >> 4) * 10 + (b & 0xF)
}

/// The ST real-time clocks' time and date words for an instant: the
/// time word (hours, minutes, seconds, each two packed digits; the
/// 24-hour form) and the date word (the year within the century 2000
/// to 2099, the weekday, the month, the day). None outside that century.
pub fn st_pack(ticks: u64) -> Option<(u32, u32)> {
    let c = ticks_to_civil(ticks);
    if !(2000..=2099).contains(&c.year) {
        return None;
    }
    let days = ticks / TICKS_PER_DAY;
    let tr = (bcd(c.hour) << 16) | (bcd(c.minute) << 8) | bcd(c.second);
    let dr = (bcd(c.year - 2000) << 16)
        | (weekday_from_days(days) << 13)
        | (bcd(c.month) << 8)
        | bcd(c.day);
    Some((tr, dr))
}

/// The instant of the ST clocks' time and date words (the century 2000).
pub fn st_unpack(tr: u32, dr: u32) -> u64 {
    civil_to_ticks(Civil {
        year: 2000 + unbcd((dr >> 16) & 0xFF),
        month: unbcd((dr >> 8) & 0x1F),
        day: unbcd(dr & 0x3F),
        hour: unbcd((tr >> 16) & 0x3F),
        minute: unbcd((tr >> 8) & 0x7F),
        second: unbcd(tr & 0x7F),
    })
}

/// The base a set establishes: the instant less the boot clock's
/// elapsed ticks, saturating at the epoch (a set of the epoch itself,
/// or of an instant earlier than the boot's own age, reads as the epoch).
pub fn base_for(ticks: u64, elapsed_ticks: u64) -> u64 {
    ticks.saturating_sub(elapsed_ticks)
}
