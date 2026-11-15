//! The ST timer rows' flight check, the flight flavor of the row's api image
//! (`test-pwm-flight`): the kernel's PWM API over the chip's timers on the
//! board's own connector pins, with nothing wired -- every line of the row's
//! table claimed and set, the period the function answers tied to the
//! timer's own registers, the pad read back through its input register
//! while the timer drives it -- and one case on a jumper wire from a PWM
//! pin to a plain input, which reads skipped when the wire is not there. On
//! the row whose chip has a second I/O supply the check opens with that
//! supply: the boot's verdict and a line of the supply driven and read
//! back. Because several of these pins are also the SPI bus's, the check
//! shows the rule between the two families and serves one byte under the
//! bus's last chip select with no device fitted. The image runs this check
//! alone.
//!
//! THE VERDICTS. Every case ends on one line: PASS, REFUSED (the row did
//! what the case forbids), SKIPPED (not flown, with the reason) or OBSERVED
//! (a reading that decides nothing). A refusal does not stop the check; it
//! ends with its summary, the END line and the exit function, 0 without a
//! refusal.
//!
//! NO TWO LINES ARE EVER DRIVEN AT ONCE. The line cases hold one line at a
//! time; the shared-period case holds two lines of one timer parked, both
//! at the idle level; so a jumper wire that joins two connector pins never
//! joins two outputs at different levels.

use core::fmt;
#[cfg(not(feature = "test-io2-probe"))]
use core::ptr::{addr_of, read_volatile};

use crate::board::facts;
#[cfg(not(feature = "test-io2-probe"))]
use crate::board::gpio_backend as pins;
#[cfg(not(feature = "test-io2-probe"))]
use crate::board::pwm_backend as hw;
#[cfg(not(feature = "test-io2-probe"))]
use crate::pwm_math::{self, POLARITY_INVERSED, POLARITY_NORMAL};
use crate::{api, gpio_math, sched};

/// The busy code of a claim another holder refuses.
#[cfg(not(feature = "test-io2-probe"))]
const BUSY: i32 = crate::pwm::ERR_BUSY;
/// A millisecond, in the functions' nanoseconds.
#[cfg(not(feature = "test-io2-probe"))]
const MS: u64 = 1_000_000;

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
    kprintln!("[flight] the PWM flight check starts {} ms after the boot", sched::now_ms());
    #[cfg(rhu_io2_supply)]
    io2(&mut f);
    #[cfg(not(feature = "test-io2-probe"))]
    {
        table(&mut f);
        for ix in 0..facts::PWM_LINES.len() {
            line(&mut f, ix);
        }
        clock(&mut f);
        shared(&mut f);
        gpio_refused(&mut f);
        spi_rule(&mut f);
        released(&mut f);
        select(&mut f);
        wire(&mut f);
    }
    kprintln!("[flight] SUMMARY leg pwm: {} PASS, {} REFUSED, {} SKIPPED, {} OBSERVED", f.pass, f.refused, f.skipped, f.observed);
    kprintln!("[flight] END");
    api::rhu_exit(if f.refused == 0 { 0 } else { 1 })
}

/// A line taken as an output through the kernel's GPIO API, driven high and
/// then low, the pad read back after each write through the same API, and
/// released: the two readings, or the code of the function that refused.
#[cfg(rhu_io2_supply)]
fn follow(line: u32) -> Result<(i32, i32), i32> {
    let r = api::rhu_gpio_claim(line, gpio_math::MODE_OUTPUT, gpio_math::PULL_NONE, 0);
    if r != 0 {
        return Err(r);
    }
    let mut read = [0i32; 2];
    for (i, level) in [1u32, 0].into_iter().enumerate() {
        let r = api::rhu_gpio_write(line, level);
        if r != 0 {
            let _ = api::rhu_gpio_release(line);
            return Err(r);
        }
        sched::sleep_ms(2);
        read[i] = api::rhu_gpio_read(line);
    }
    let r = api::rhu_gpio_release(line);
    if r != 0 {
        return Err(r);
    }
    Ok((read[0], read[1]))
}

/// The second I/O supply (the STM32U5 row). In the flight image: the boot's
/// verdict from the step's record, and the board's third LED line -- a pin
/// of that supply -- driven both ways and read back. In the probe image:
/// the control register as the reset left it, the line driven and read
/// back before the step, the step, and the line again.
#[cfg(rhu_io2_supply)]
fn io2(f: &mut Flight) {
    use crate::board::stm32u5_supply as supply;
    let s = &facts::IO2_SUPPLY;
    let line = facts::LED2_LINE;
    let (verdict, polls, entry) = supply::verdict();
    if verdict != supply::VERDICT_LEFT_OUT {
        match follow(line) {
            Ok((1, 0)) if verdict == supply::VERDICT_VALID => f.pass("io2-boot", format_args!("the boot declared the second I/O supply valid after {} status reads, the control register {:#010x} as it was found; line {} driven high read 1 and driven low read 0", polls, entry, line)),
            Ok((h, l)) => f.refused("io2-boot", format_args!("the step's verdict {} after {} status reads (1 valid, 2 not ready), the control register {:#010x} as found; line {} driven high read {} and driven low read {}", verdict, polls, entry, line, h, l)),
            Err(code) => f.refused("io2-boot", format_args!("the step's verdict {} after {} status reads; line {} could not be driven: a GPIO function answered {}", verdict, polls, line, code)),
        }
        return;
    }
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
    kprintln!("[flight] io2: the boot left the step out; the monitoring control register reads {:#010x} as found (the valid bit {}, the monitor's enable {})", found, (found >> s.valid_bit) & 1, (found >> s.monitor_bit) & 1);
    if cleared {
        kprintln!("[flight] io2: the valid bit was found set, left by an earlier program across the reset, and is cleared for the probe");
    }
    let before = follow(line);
    let o = supply::bring_up();
    let after = follow(line);
    kprintln!("[flight] io2: the step read the supply {} after {} status reads", if o.valid { "ready" } else { "NOT ready inside the bound" }, o.polls);
    match (o.valid, before, after) {
        (true, Ok((0, 0)), Ok((1, 0))) => f.pass("io2-probe", format_args!("IO2-0: PG2 follows AFTER the step and NOT BEFORE -- line {} driven high and low read 0 and 0 with the valid bit clear, 1 and 0 with it set: the supply's lines are isolated until the step", line)),
        (true, Ok((1, 0)), Ok((1, 0))) => f.observed("io2-probe", format_args!("IO2-0: PG2 follows BEFORE the step too -- line {} driven high and low read 1 and 0 with the valid bit clear and 1 and 0 with it set", line)),
        (valid, b, a) => f.refused("io2-probe", format_args!("neither verdict: the supply read ready {}; line {} before the step {:?}, after it {:?} (the readings driven high and driven low, or the refusing function's code)", valid, line, b, a)),
    }
}

/// A line's pad read through its port's input register for a span of the
/// kernel's clock: the samples taken, how many read high, and how many
/// times a sample read high after one that read low.
#[cfg(not(feature = "test-io2-probe"))]
fn sample(line: u32, span_ns: u64) -> (u32, u32, u32) {
    let t0 = sched::now_ns();
    let (mut n, mut high, mut rises) = (0u32, 0u32, 0u32);
    let mut last = pins::read(line);
    while sched::now_ns().wrapping_sub(t0) < span_ns {
        let v = pins::read(line);
        n += 1;
        high += v as u32;
        rises += (v && !last) as u32;
        last = v;
    }
    (n, high, rises)
}

/// The share of high samples, in whole percent.
#[cfg(not(feature = "test-io2-probe"))]
fn percent(n: u32, high: u32) -> u32 {
    if n == 0 {
        0
    } else {
        (high as u64 * 100 / n as u64) as u32
    }
}

/// The count and the enumeration against the row's table.
#[cfg(not(feature = "test-io2-probe"))]
fn table(f: &mut Flight) {
    let lines = facts::PWM_LINES;
    let n = api::rhu_pwm_count();
    let mut wrong = (n != lines.len() as i32) as u32;
    for (i, l) in lines.iter().enumerate() {
        wrong += (api::rhu_pwm_line(i as u32) != l.line as i32) as u32;
    }
    wrong += (api::rhu_pwm_line(lines.len() as u32) != sched::ERR_INVAL) as u32;
    if wrong == 0 {
        f.pass("pwm0-table", format_args!("{} lines on {} timers stated and enumerated, an index past the count refused", lines.len(), facts::PWM_TIMERS.len()));
    } else {
        f.refused("pwm0-table", format_args!("the count reads {} against a table of {} lines; {} of the count, the lines' numbers and the refusal past the count differ", n, lines.len(), wrong));
    }
}

/// One line of the table, alone: claimed; its period tied to the timer's
/// own registers; driven and read back at both polarities; parked and read
/// back at both; its edges counted at a slow period; released.
#[cfg(not(feature = "test-io2-probe"))]
fn line(f: &mut Flight, ix: usize) {
    let l = &facts::PWM_LINES[ix];
    let t = &facts::PWM_TIMERS[l.timer as usize];
    let line = l.line as u32;
    let h = api::rhu_pwm_claim(line);
    let (mut tn, mut ch, mut hz) = (0u32, 0u32, 0u32);
    let info = if h == ix as i32 { api::rhu_pwm_info(h, &mut tn, &mut ch, &mut hz) } else { sched::ERR_INVAL };
    if h != ix as i32 || info != 0 || tn != t.number || ch != l.channel as u32 || hz != t.clock_hz {
        f.refused("pwm0-claim", format_args!("line {}: the claim answered {} (its index is {}) and the info function {} with timer {} channel index {} clock {} Hz; the table says timer {} channel index {} clock {} Hz", line, h, ix, info, tn, ch, hz, t.number, l.channel, t.clock_hz));
        if h >= 0 {
            let _ = api::rhu_pwm_release(h);
        }
        return;
    }
    f.pass("pwm0-claim", format_args!("line {}: handle {}, timer {} channel index {} on a clock of {} Hz", line, h, tn, ch, hz));

    let want = match pwm_math::quantize(MS, t) {
        Some((p, a)) => pwm_math::achieved_ns(p, a, t.clock_hz),
        None => 0,
    };
    let mut ns = 0u64;
    let set = api::rhu_pwm_set(h, MS, MS / 4, POLARITY_NORMAL);
    let got = api::rhu_pwm_period(h, &mut ns);
    let own = hw::probe_period_ns(l.timer as usize);
    if set == 0 && got == 0 && want != 0 && ns == want && own == ns {
        f.pass("pwm0-tie", format_args!("line {}: a period of 1000000 ns runs as {} ns by the function, by the arithmetic and by the timer's own registers", line, ns));
    } else {
        f.refused("pwm0-tie", format_args!("line {}: the function answers {} ns and the timer's own registers {} ns (the arithmetic says {} ns; the set answered {} and the period function {})", line, ns, own, want, set, got));
    }

    let on = api::rhu_pwm_enable(h, 1);
    let (n1, high1, _) = sample(line, 20 * MS);
    let inv = api::rhu_pwm_set(h, MS, MS / 4, POLARITY_INVERSED);
    let (n2, high2, _) = sample(line, 20 * MS);
    let (p1, p2) = (percent(n1, high1), percent(n2, high2));
    if on == 0 && inv == 0 && n1 >= 400 && n2 >= 400 && (20..=30).contains(&p1) && (70..=80).contains(&p2) {
        f.pass("pwm0-duty", format_args!("line {}: driven at a quarter duty the pad read high on {} percent of {} samples, and at the inversed polarity on {} percent of {}", line, p1, n1, p2, n2));
    } else {
        f.refused("pwm0-duty", format_args!("line {}: at a quarter duty the pad read high on {} percent of {} samples (20 to 30 wanted), at the inversed polarity on {} percent of {} (70 to 80 wanted); the enable answered {} and the inversed set {}", line, p1, n1, p2, n2, on, inv));
    }

    let off = api::rhu_pwm_enable(h, 0);
    let (n3, high3, _) = sample(line, 3 * MS);
    let norm = api::rhu_pwm_set(h, MS, MS / 4, POLARITY_NORMAL);
    let (n4, high4, _) = sample(line, 3 * MS);
    if off == 0 && norm == 0 && n3 > 0 && high3 == n3 && n4 > 0 && high4 == 0 {
        f.pass("pwm0-park", format_args!("line {}: parked, the pad read low on every one of {} samples at the normal polarity and high on every one of {} at the inversed one", line, n4, n3));
    } else {
        f.refused("pwm0-park", format_args!("line {}: parked at the inversed polarity the pad read high on {} of {} samples (all wanted), at the normal polarity on {} of {} (none wanted); the park answered {} and the set {}", line, high3, n3, high4, n4, off, norm));
    }

    let slow = api::rhu_pwm_set(h, 10 * MS, 10 * MS / 4, POLARITY_NORMAL);
    let on = api::rhu_pwm_enable(h, 1);
    let (n5, _, rises) = sample(line, 100 * MS);
    if slow == 0 && on == 0 && (9..=11).contains(&rises) {
        f.pass("pwm0-edges", format_args!("line {}: at a period of 10 ms the pad rose {} times in 100 ms ({} samples)", line, rises, n5));
    } else {
        f.refused("pwm0-edges", format_args!("line {}: at a period of 10 ms the pad rose {} times in 100 ms over {} samples (9 to 11 wanted); the set answered {} and the enable {}", line, rises, n5, slow, on));
    }

    let rel = api::rhu_pwm_release(h);
    if rel != 0 {
        f.refused("pwm0-release", format_args!("line {}: the release answered {}", line, rel));
    }
}

/// The table's clocks stand on the timer prescaler selector's reset value.
#[cfg(not(feature = "test-io2-probe"))]
fn clock(f: &mut Flight) {
    match hw::probe_selector() {
        1 => f.refused("pwm0-clock", format_args!("the timer prescaler selector was found set: the table's clocks are computed for its reset value")),
        0 => f.pass("pwm0-clock", format_args!("the timer prescaler selector reads its reset value: each timer runs the clock its table row states")),
        _ => f.pass("pwm0-clock", format_args!("the chip has no timer prescaler selector: each timer runs the clock its table row states")),
    }
}

/// Two lines of one timer, held together and never driven: the second
/// line's other period refused with the busy code and the holder named, the
/// same period served. The wire's input line is left out of the pair.
#[cfg(not(feature = "test-io2-probe"))]
fn shared(f: &mut Flight) {
    let lines = facts::PWM_LINES;
    let mut pair = None;
    'find: for a in 0..lines.len() {
        for b in a + 1..lines.len() {
            if lines[a].timer == lines[b].timer && lines[a].line as u32 != facts::PWM_WIRE_IN && lines[b].line as u32 != facts::PWM_WIRE_IN {
                pair = Some((a, b));
                break 'find;
            }
        }
    }
    let Some((a, b)) = pair else {
        f.skipped("pwm0-shared", format_args!("no two lines of the table share a timer"));
        return;
    };
    let (la, lb) = (lines[a].line as u32, lines[b].line as u32);
    let (ha, hb) = (api::rhu_pwm_claim(la), api::rhu_pwm_claim(lb));
    let s1 = api::rhu_pwm_set(ha, MS, MS / 4, POLARITY_NORMAL);
    let s2 = api::rhu_pwm_set(hb, 2 * MS, MS / 2, POLARITY_NORMAL);
    let holder = unsafe { read_volatile(addr_of!((*addr_of!(crate::pwm::WITNESS)).last_holder_line)) };
    let s3 = api::rhu_pwm_set(hb, MS, MS / 2, POLARITY_NORMAL);
    let (mut pa, mut pb) = (0u64, 0u64);
    let (ga, gb) = (api::rhu_pwm_period(ha, &mut pa), api::rhu_pwm_period(hb, &mut pb));
    let (rb, ra) = (if hb >= 0 { api::rhu_pwm_release(hb) } else { hb }, if ha >= 0 { api::rhu_pwm_release(ha) } else { ha });
    if ha == a as i32 && hb == b as i32 && s1 == 0 && s2 == BUSY && holder == la && s3 == 0 && ga == 0 && gb == 0 && pa != 0 && pa == pb && ra == 0 && rb == 0 {
        f.pass("pwm0-shared", format_args!("lines {} and {} of timer {}: a second period refused with the busy code and line {} named as the holder, the same period served, both run {} ns", la, lb, facts::PWM_TIMERS[lines[a].timer as usize].number, holder, pa));
    } else {
        f.refused("pwm0-shared", format_args!("lines {} and {}: the claims answered {} and {}, the first set {}, the other period {} (the busy code {} wanted) with the holder {} in the witness, the same period {}, the periods {} and {} ns, the releases {} and {}", la, lb, ha, hb, s1, s2, BUSY, holder, s3, pa, pb, ra, rb));
    }
}

/// A GPIO claim of a line a PWM channel holds: refused, the holder named.
#[cfg(not(feature = "test-io2-probe"))]
fn gpio_refused(f: &mut Flight) {
    let line = facts::PWM_LINES[0].line as u32;
    let h = api::rhu_pwm_claim(line);
    let g = api::rhu_gpio_claim(line, gpio_math::MODE_OUTPUT, gpio_math::PULL_NONE, 0);
    let holder = unsafe { read_volatile(addr_of!((*addr_of!(crate::gpio::WITNESS)).last_busy_holder)) };
    if g == 0 {
        let _ = api::rhu_gpio_release(line);
    }
    let r = if h >= 0 { api::rhu_pwm_release(h) } else { h };
    if h == 0 && g == BUSY && holder == crate::gpio::HOLDER_PWM && r == 0 {
        f.pass("pwm0-gpio", format_args!("a GPIO claim of line {} refused with the busy code while a PWM channel holds the line, the holder named as a PWM channel", line));
    } else {
        f.refused("pwm0-gpio", format_args!("line {}: the PWM claim answered {}, the GPIO claim {} (the busy code {} wanted) with the holder word {} ({} wanted), the release {}", line, h, g, BUSY, holder, crate::gpio::HOLDER_PWM, r));
    }
}

/// A line that is both a PWM line and a pad of the row's SPI bus serves
/// one at a time: the bus's claim refused while the PWM channel holds the
/// line; the line released, the bus claimed, the PWM claim refused.
#[cfg(not(feature = "test-io2-probe"))]
fn spi_rule(f: &mut Flight) {
    let Some(bus) = facts::SPI_BUSES.first() else {
        f.skipped("pwm0-spi", format_args!("the row has no SPI bus"));
        return;
    };
    let Some(ix) = facts::PWM_LINES.iter().position(|l| bus.pads.iter().any(|p| p.line == l.line)) else {
        f.skipped("pwm0-spi", format_args!("no PWM line is a pad of SPI{}", bus.number));
        return;
    };
    let line = facts::PWM_LINES[ix].line as u32;
    let h = api::rhu_pwm_claim(line);
    let c1 = api::rhu_spi_claim(bus.number);
    let r = if h >= 0 { api::rhu_pwm_release(h) } else { h };
    let c2 = api::rhu_spi_claim(bus.number);
    let h2 = api::rhu_pwm_claim(line);
    if h2 >= 0 {
        let _ = api::rhu_pwm_release(h2);
    }
    if h == ix as i32 && c1 == BUSY && r == 0 && c2 == 0 && h2 == BUSY {
        f.pass("pwm0-spi", format_args!("line {} is a pad of SPI{}: the bus's claim refused with the busy code while a PWM channel holds the line; the line released, the bus claimed, and the PWM claim of the line refused the same way", line, bus.number));
    } else {
        f.refused("pwm0-spi", format_args!("line {} and SPI{}: the PWM claim answered {}, the bus's claim under it {} (the busy code {} wanted), the release {}, the bus's claim after it {}, the PWM claim after that {} (the busy code wanted)", line, bus.number, h, c1, BUSY, r, c2, h2));
    }
}

/// Every line given back: each line no bus holds claims and releases once
/// more, and the PWM witness counts as many releases as claims.
#[cfg(not(feature = "test-io2-probe"))]
fn released(f: &mut Flight) {
    let (mut free, mut held, mut wrong) = (0u32, 0u32, 0u32);
    for (i, l) in facts::PWM_LINES.iter().enumerate() {
        let line = l.line as u32;
        let h = api::rhu_pwm_claim(line);
        let board = crate::critical::with_cs(|| unsafe { crate::claims::is_board(&*addr_of!(crate::gpio::CLAIMS), line) });
        if h == i as i32 {
            free += 1;
            wrong += (api::rhu_pwm_release(h) != 0) as u32;
        } else if h == BUSY && board {
            held += 1;
        } else {
            wrong += 1;
        }
    }
    let (claims, releases) = unsafe {
        let w = addr_of!(crate::pwm::WITNESS);
        (read_volatile(addr_of!((*w).claims_taken)), read_volatile(addr_of!((*w).releases)))
    };
    if wrong == 0 && claims == releases {
        f.pass("pwm0-release", format_args!("{} lines claimed and given back once more, {} held by the bus this check claimed; the witness counts {} claims and {} releases", free, held, claims, releases));
    } else {
        f.refused("pwm0-release", format_args!("{} lines neither free nor held by a bus; the witness counts {} claims and {} releases", wrong, claims, releases));
    }
}

/// The last chip select of the row's SPI bus, on a bus with no device
/// fitted: a one-byte transfer under it is served and reads ones (the
/// input pad's pull-up answers for the empty bus), the select's line is
/// then the board's and its pad stands high, and the select one past the
/// last is refused.
#[cfg(not(feature = "test-io2-probe"))]
fn select(f: &mut Flight) {
    let Some(bus) = facts::SPI_BUSES.first() else {
        f.skipped("spi-select", format_args!("the row has no SPI bus"));
        return;
    };
    let Some((&pad, last)) = bus.selects.last().zip(bus.select_count.checked_sub(1)) else {
        f.skipped("spi-select", format_args!("SPI{} has no chip select", bus.number));
        return;
    };
    let line = pad as u32;
    let claim = api::rhu_spi_claim(bus.number);
    let tx = [0x55u8];
    let mut rx = [0u8];
    let hz = api::rhu_spi_transfer(bus.number, last, 0, 1_000_000, tx.as_ptr(), rx.as_mut_ptr(), 1);
    let board = crate::critical::with_cs(|| unsafe { crate::claims::is_board(&*addr_of!(crate::gpio::CLAIMS), line) });
    let high = pins::read(line);
    let past = api::rhu_spi_transfer(bus.number, bus.select_count, 0, 1_000_000, tx.as_ptr(), rx.as_mut_ptr(), 1);
    if claim == 0 && hz > 0 && rx[0] == 0xFF && board && high && past == sched::ERR_INVAL {
        f.pass("spi-select", format_args!("select {} of SPI{} is line {}: a one-byte transfer under it served at {} Hz and read 0xff on the empty bus; the line is then held by the board and its pad stands high; select {} refused as invalid", last, bus.number, line, hz, bus.select_count));
    } else {
        f.refused("spi-select", format_args!("select {} of SPI{} on line {}: the bus's claim answered {}, the transfer {} (a rate wanted) reading {:#04x} (0xff wanted), the line held by the board {} and its pad high {}, the select past the last {} ({} wanted)", last, bus.number, line, claim, hz, rx[0], board, high, past, sched::ERR_INVAL));
    }
}

/// The share of high readings of a plain input line, through the kernel's
/// GPIO API, over a span of the kernel's clock.
#[cfg(not(feature = "test-io2-probe"))]
fn percent_read(line: u32, span_ns: u64) -> u32 {
    let t0 = sched::now_ns();
    let (mut n, mut high) = (0u32, 0u32);
    while sched::now_ns().wrapping_sub(t0) < span_ns {
        n += 1;
        high += (api::rhu_gpio_read(line) == 1) as u32;
    }
    percent(n, high)
}

/// The events the kernel's GPIO API counts on a line over a span, for one
/// kind of edge: armed, the counter read, the span slept, the counter read
/// again, disarmed. `u32::MAX` when a function refuses.
#[cfg(not(feature = "test-io2-probe"))]
fn events(line: u32, kind: u32, ms: u32) -> u32 {
    if api::rhu_gpio_edge(line, kind) != 0 {
        return u32::MAX;
    }
    let w = api::rhu_gpio_event_word(line);
    if w.is_null() {
        let _ = api::rhu_gpio_edge(line, gpio_math::KIND_NONE);
        return u32::MAX;
    }
    let start = unsafe { read_volatile(w) };
    sched::sleep_ms(ms);
    let n = gpio_math::edges_since(unsafe { read_volatile(w) }, start);
    let _ = api::rhu_gpio_edge(line, gpio_math::KIND_NONE);
    n
}

/// The jumper wire: a PWM line's edges counted by a plain input line of the
/// kernel's GPIO API. The wire is looked for first -- the PWM pin taken as a
/// plain output and driven high and low eight times, the input must follow
/// each time -- and the case is skipped without it. With it: at 100 Hz and
/// a quarter duty the input's rising and falling events over half a second
/// each, the share of high readings at both polarities, and no event and a
/// low level while the line is parked.
#[cfg(not(feature = "test-io2-probe"))]
fn wire(f: &mut Flight) {
    let (out, inp) = (facts::PWM_WIRE_OUT, facts::PWM_WIRE_IN);
    let Some(ix) = facts::PWM_LINES.iter().position(|l| l.line as u32 == out) else {
        f.refused("pwm1-wire", format_args!("the wire's driving line {} is not a PWM line of the table", out));
        return;
    };
    let c_in = api::rhu_gpio_claim(inp, gpio_math::MODE_INPUT, gpio_math::PULL_NONE, 0);
    let c_out = api::rhu_gpio_claim(out, gpio_math::MODE_OUTPUT, gpio_math::PULL_NONE, 0);
    if c_in != 0 || c_out != 0 {
        if c_in == 0 {
            let _ = api::rhu_gpio_release(inp);
        }
        if c_out == 0 {
            let _ = api::rhu_gpio_release(out);
        }
        f.refused("pwm1-wire", format_args!("lines {} and {} could not be taken as plain lines: the claims answered {} and {}", out, inp, c_out, c_in));
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
    if follows != 8 {
        let _ = api::rhu_gpio_release(inp);
        f.skipped("pwm1-wire", format_args!("no jumper wire from line {} to line {}: driven high and low eight times as a plain output, the input followed {} times of 8", out, inp, follows));
        return;
    }
    let h = api::rhu_pwm_claim(out);
    let set = if h >= 0 { api::rhu_pwm_set(h, 10 * MS, 10 * MS / 4, POLARITY_NORMAL) } else { h };
    let on = if h >= 0 { api::rhu_pwm_enable(h, 1) } else { h };
    sched::sleep_ms(20);
    let rises = events(inp, gpio_math::KIND_RISING, 500);
    let falls = events(inp, gpio_math::KIND_FALLING, 500);
    let p1 = percent_read(inp, 100 * MS);
    let inv = if h >= 0 { api::rhu_pwm_set(h, 10 * MS, 10 * MS / 4, POLARITY_INVERSED) } else { h };
    sched::sleep_ms(20);
    let p2 = percent_read(inp, 100 * MS);
    let norm = if h >= 0 { api::rhu_pwm_set(h, 10 * MS, 10 * MS / 4, POLARITY_NORMAL) } else { h };
    let off = if h >= 0 { api::rhu_pwm_enable(h, 0) } else { h };
    sched::sleep_ms(20);
    let quiet = events(inp, gpio_math::KIND_BOTH, 200);
    let low = api::rhu_gpio_read(inp);
    let r_pwm = if h >= 0 { api::rhu_pwm_release(h) } else { h };
    let r_in = api::rhu_gpio_release(inp);
    let codes = r_out == 0 && h == ix as i32 && set == 0 && on == 0 && inv == 0 && norm == 0 && off == 0 && r_pwm == 0 && r_in == 0;
    if codes && (49..=51).contains(&rises) && (49..=51).contains(&falls) && (20..=30).contains(&p1) && (70..=80).contains(&p2) && quiet == 0 && low == 0 {
        f.pass("pwm1-wire", format_args!("line {} drives at 100 Hz and a quarter duty and line {} counts it on the wire: {} rising and {} falling events in half a second each, high on {} percent of its readings and on {} percent at the inversed polarity; parked, no event in 200 ms and the level low", out, inp, rises, falls, p1, p2));
    } else {
        f.refused("pwm1-wire", format_args!("line {} to line {}: {} rising and {} falling events in half a second each (49 to 51 wanted), high on {} percent of the readings (20 to 30 wanted) and on {} percent inversed (70 to 80 wanted); parked, {} events in 200 ms and the level {} (0 and 0 wanted); every function answered 0: {}", out, inp, rises, falls, p1, p2, quiet, low, codes));
    }
}
