//! The calendar: the date and time as a BASE plus the monotonic clock.
//! The base is the calendar tick (100 nanoseconds since
//! 0001-01-01T00:00:00Z) at the instant the boot clock read zero; the
//! calendar at any moment is the base plus the boot clock's elapsed
//! ticks, so setting the base never moves the elapsed clock. The base is
//! the epoch until a source sets it. The sources, in the order they can
//! arrive: a board's real-time clock read once at boot where the part
//! has one and it was ever set; the developer's tooling writing the
//! push mailbox below over the debug probe; and the hosted side's
//! explicit sets (network time by a call, or the application). The first
//! source establishes the base; any later EXPLICIT set replaces it; the
//! source query says which is current. Where a part has a real-time
//! clock, an explicit set is written back to it so the next boot starts
//! true across a reset (a power cycle needs backup power the boards do
//! not carry).

use core::ptr::{addr_of, addr_of_mut, read_volatile, write_volatile};

use crate::calendar_math as cm;
use crate::critical::with_cs;
use crate::{board, sched};

/// The sources (the header states them to the consumer).
#[allow(dead_code)]
pub const SOURCE_UNSET: i32 = 0;
pub const SOURCE_RTC: i32 = 1;
pub const SOURCE_PUSH: i32 = 2;
pub const SOURCE_NETWORK: i32 = 3;
pub const SOURCE_APP: i32 = 4;

/// The real-time clock's state in the record.
pub const RTC_NONE: u32 = 0;
pub const RTC_COLD: u32 = 1;
pub const RTC_UNINITIALIZED: u32 = 2;
pub const RTC_READ: u32 = 3;
pub const RTC_WRITTEN: u32 = 4;
pub const RTC_SKIPPED_LSE: u32 = 5;
pub const RTC_SKIPPED_RANGE: u32 = 6;
pub const RTC_SKIPPED_SOURCE: u32 = 7;

/// The low-speed oscillator's state in the record.
pub const LSE_NONE: u32 = 0;
pub const LSE_STARTED: u32 = 1;
pub const LSE_READY: u32 = 2;

/// The calendar record, exported by symbol for a probe: six words.
#[repr(C)]
pub struct CalendarRecord {
    pub base_lo: u32,
    pub base_hi: u32,
    pub source: u32,
    pub rtc: u32,
    pub lse: u32,
    pub pushes: u32,
}

#[unsafe(export_name = "__rhu_calendar")]
pub static mut RECORD: CalendarRecord = CalendarRecord {
    base_lo: 0,
    base_hi: 0,
    source: 0,
    rtc: 0,
    lse: 0,
    pushes: 0,
};

/// The push mailbox, exported by symbol: the tooling writes the ticks,
/// then a new sequence number, on the running image; the kernel consumes
/// a sequence number it has not seen at boot and at every tick.
#[repr(C)]
pub struct PushMailbox {
    pub magic: u32,
    pub seq: u32,
    pub ticks_lo: u32,
    pub ticks_hi: u32,
}

/// The mailbox's magic word: the letters R H U C.
pub const PUSH_MAGIC: u32 = 0x5248_5543;

#[unsafe(export_name = "__rhu_calendar_push")]
pub static mut PUSH: PushMailbox = PushMailbox {
    magic: 0,
    seq: 0,
    ticks_lo: 0,
    ticks_hi: 0,
};

static mut PUSH_SEEN: u32 = 0;

fn elapsed_ticks() -> u64 {
    sched::now_ns() / 100
}

fn write_base(base: u64, source: u32) {
    unsafe {
        let r = &mut *addr_of_mut!(RECORD);
        write_volatile(&mut r.base_lo, base as u32);
        write_volatile(&mut r.base_hi, (base >> 32) as u32);
        write_volatile(&mut r.source, source);
    }
}

fn read_base() -> (u64, u32) {
    unsafe {
        let r = &*addr_of!(RECORD);
        let lo = read_volatile(&r.base_lo) as u64;
        let hi = read_volatile(&r.base_hi) as u64;
        (lo | (hi << 32), read_volatile(&r.source))
    }
}

fn set_rtc_state(rtc: u32, lse: u32) {
    unsafe {
        let r = &mut *addr_of_mut!(RECORD);
        write_volatile(&mut r.rtc, rtc);
        write_volatile(&mut r.lse, lse);
    }
}

/// The boot path's call, after the clock posture and before the
/// scheduler starts (the boot clock reads zero here, so the base IS the
/// instant read): the board's real-time clock first, then the mailbox.
pub fn init_at_boot() {
    match board::rtc_read_at_boot() {
        board::RtcBoot::NoRtc => set_rtc_state(RTC_NONE, LSE_NONE),
        board::RtcBoot::Cold => set_rtc_state(RTC_COLD, LSE_STARTED),
        board::RtcBoot::Uninitialized => set_rtc_state(RTC_UNINITIALIZED, LSE_READY),
        board::RtcBoot::Read(ticks) => {
            set_rtc_state(RTC_READ, LSE_READY);
            if ticks <= cm::TICKS_MAX {
                write_base(ticks, SOURCE_RTC as u32);
            }
        }
    }
    consume_push(true);
}

/// The push mailbox consumed if its sequence number is new: at boot the
/// base is the ticks themselves; later the boot clock's elapsed is taken
/// off. Called from the tick handler (no section needed there: the
/// doors' writes hold the kernel critical section, which masks the tick).
fn consume_push(at_boot: bool) {
    unsafe {
        let p = &*addr_of!(PUSH);
        if read_volatile(&p.magic) != PUSH_MAGIC {
            return;
        }
        let seq = read_volatile(&p.seq);
        if seq == read_volatile(addr_of!(PUSH_SEEN)) {
            return;
        }
        write_volatile(addr_of_mut!(PUSH_SEEN), seq);
        let ticks = read_volatile(&p.ticks_lo) as u64 | ((read_volatile(&p.ticks_hi) as u64) << 32);
        if ticks > cm::TICKS_MAX {
            return;
        }
        let base = if at_boot { ticks } else { cm::base_for(ticks, elapsed_ticks()) };
        write_base(base, SOURCE_PUSH as u32);
        let r = &mut *addr_of_mut!(RECORD);
        write_volatile(&mut r.pushes, read_volatile(&r.pushes).wrapping_add(1));
    }
}

/// The tick handler's poll: one load of the mailbox's magic and one of
/// its sequence number on the common path.
pub fn tick_poll() {
    consume_push(false);
}

/// The read door: the calendar now and the source that set it.
pub fn get() -> (u64, i32) {
    let (base, source) = with_cs(read_base);
    (base.saturating_add(elapsed_ticks()), source as i32)
}

/// The explicit set door: the base moved so the calendar reads the
/// given instant now, the source recorded, the real-time clock written
/// where the part has one. Refuses a source the hosted side may not
/// claim and an instant past the maximum.
pub fn set(ticks: u64, source: i32) -> i32 {
    if !(source == SOURCE_NETWORK || source == SOURCE_APP) || ticks > cm::TICKS_MAX {
        return sched::ERR_INVAL;
    }
    with_cs(|| write_base(cm::base_for(ticks, elapsed_ticks()), source as u32));
    match board::rtc_write(ticks) {
        board::RtcWrite::NoRtc => {}
        board::RtcWrite::Written => set_rtc_state(RTC_WRITTEN, LSE_READY),
        board::RtcWrite::SkippedLseNotReady => set_rtc_state(RTC_SKIPPED_LSE, LSE_STARTED),
        board::RtcWrite::SkippedRange => set_rtc_state(RTC_SKIPPED_RANGE, LSE_READY),
        board::RtcWrite::SkippedSourceHeld => set_rtc_state(RTC_SKIPPED_SOURCE, LSE_READY),
    }
    0
}

/// The words of the api flavor's check: the real-time clock's
/// state, the oscillator's state, the pushes consumed.
#[allow(dead_code)]
pub fn record_states() -> (u32, u32, u32) {
    unsafe {
        let r = &*addr_of!(RECORD);
        (read_volatile(&r.rtc), read_volatile(&r.lse), read_volatile(&r.pushes))
    }
}
