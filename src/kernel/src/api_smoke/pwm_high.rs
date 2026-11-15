//! The emulator rows' PWM line of a large number, under the test word
//! `test-pwm-high-line`: the rows' facts state a fourth PWM line inside a
//! wider line space (the word's statements in the facts file), and this
//! check of the api flavor serves that line through the kernel's PWM API as
//! a program would -- enumerated, claimed, its period set and tied to the
//! timer's own reading of its registers, driven, parked and released, with
//! the refusals around it. It runs in place of the rows' pwm-claim check,
//! which states the three lines of the facts without the word. The doctored
//! control `test-pwm-doctored` beside the word makes the timer's reading one
//! prescaler step long, which the tie here must refuse.

use core::ptr::{addr_of, read_volatile};

use crate::board::{facts, pwm_backend};
use crate::{api, gpio, gpio_math, pwm, pwm_math, sched};

#[cfg(not(rhu_pwm_adc_models))]
compile_error!("test-pwm-high-line is a word of the emulator rows' facts: select an emulator row beside it");

/// What this check expects of the facts under the word, stated here for
/// itself: the line's number, its position in the PWM table (the handle a
/// claim answers), its timer's position, number and channel, and the line
/// space.
const LINE: u32 = 118;
const INDEX: i32 = 3;
const TIMER_INDEX: usize = 1;
const TIMER_NUMBER: u32 = 2;
const CHANNEL: u32 = 1;
const SPACE: u32 = 176;
/// A period the timer's clock divides and a quarter of it as the duty, with
/// the ticks the virtual timer's registers hold for them.
const PERIOD_NS: u64 = 1_000_000;
const DUTY_NS: u64 = 250_000;
const PERIOD_TICKS: u32 = 1000;
const DUTY_TICKS: u32 = 250;

fn pwm_witness() -> pwm::PwmWitness {
    unsafe { read_volatile(addr_of!(pwm::WITNESS)) }
}

fn gpio_witness() -> gpio::GpioWitness {
    unsafe { read_volatile(addr_of!(gpio::WITNESS)) }
}

pub fn check() {
    kprintln!("[api] pwm-high-line: PWM line {} in a line space of {} enumerated, claimed, set, tied to its timer, driven, parked and released; the lines beside it and the space's end refused", LINE, SPACE);
    let w0 = pwm_witness();
    let n = api::rhu_pwm_count();
    assert!(n == facts::PWM_LINES.len() as i32 && n == INDEX + 1, "pwm-high-line: the count reads {}", n);
    for (i, l) in facts::PWM_LINES.iter().enumerate() {
        assert!(api::rhu_pwm_line(i as u32) == l.line as i32, "pwm-high-line: position {} reads {}", i, api::rhu_pwm_line(i as u32));
    }
    assert!(api::rhu_pwm_line(INDEX as u32) == LINE as i32, "pwm-high-line: the last position reads {}", api::rhu_pwm_line(INDEX as u32));
    assert!(api::rhu_pwm_line(n as u32) == sched::ERR_INVAL, "pwm-high-line: a position past the count not refused EINVAL");
    assert!(api::rhu_gpio_count() == SPACE as i32, "pwm-high-line: the line space reads {}", api::rhu_gpio_count());
    for l in [LINE - 1, LINE + 1, SPACE] {
        assert!(api::rhu_pwm_claim(l) == sched::ERR_INVAL, "pwm-high-line: the PWM claim of line {} not refused EINVAL", l);
        assert!(api::rhu_gpio_claim(l, gpio_math::MODE_INPUT, gpio_math::PULL_NONE, 0) == sched::ERR_INVAL, "pwm-high-line: the GPIO claim of line {} not refused EINVAL", l);
    }
    let h = api::rhu_pwm_claim(LINE);
    assert!(h == INDEX, "pwm-high-line: the claim answered {}", h);
    assert!(api::rhu_pwm_claim(LINE) == pwm::ERR_BUSY, "pwm-high-line: a second claim not refused EBUSY");
    let r = api::rhu_gpio_claim(LINE, gpio_math::MODE_INPUT, gpio_math::PULL_NONE, 0);
    let g = gpio_witness();
    assert!(r == gpio::ERR_BUSY && g.last_busy_holder == gpio::HOLDER_PWM && g.last_refused_line == LINE, "pwm-high-line: the GPIO claim of the held line answered {} with holder {} on line {}", r, g.last_busy_holder, g.last_refused_line);
    let mut ns = 0u64;
    assert!(api::rhu_pwm_set(h, PERIOD_NS, DUTY_NS, pwm_math::POLARITY_NORMAL) == 0, "pwm-high-line: the set refused");
    assert!(api::rhu_pwm_period(h, &mut ns) == 0 && ns == PERIOD_NS, "pwm-high-line: the period reads {} ns", ns);
    let own = pwm_backend::probe_period_ns(TIMER_INDEX);
    assert!(own == ns, "pwm-high-line: the period reads {} ns but the timer runs {} ns", ns, own);
    let (mut t, mut c, mut hz) = (0u32, 0u32, 0u32);
    assert!(api::rhu_pwm_info(h, &mut t, &mut c, &mut hz) == 0 && t == TIMER_NUMBER && c == CHANNEL && hz == facts::PWM_TIMERS[TIMER_INDEX].clock_hz, "pwm-high-line: the info reads timer {} channel {} clock {}", t, c, hz);
    assert!(api::rhu_pwm_enable(h, 1) == 0, "pwm-high-line: the enable refused");
    let (compare, ticks, driven, _) = pwm_backend::output(INDEX as usize);
    assert!(driven && ticks == PERIOD_TICKS && compare == DUTY_TICKS, "pwm-high-line: the driven output reads compare {} of {} ticks, driven {}", compare, ticks, driven);
    assert!(api::rhu_pwm_enable(h, 0) == 0 && !pwm_backend::output(INDEX as usize).2, "pwm-high-line: the output not parked");
    assert!(api::rhu_pwm_release(h) == 0, "pwm-high-line: the release refused");
    assert!(api::rhu_gpio_claim(LINE, gpio_math::MODE_INPUT, gpio_math::PULL_NONE, 0) == 0 && api::rhu_gpio_release(LINE) == 0, "pwm-high-line: the released line not served through the GPIO API");
    assert!(api::rhu_pwm_claim(LINE) == INDEX && api::rhu_pwm_release(INDEX) == 0, "pwm-high-line: the claim after the release refused");
    let w = pwm_witness();
    assert!(w.lines == n as u32 && w.claims_taken == w0.claims_taken + 2 && w.releases == w0.releases + 2 && w.sets == w0.sets + 1 && w.enables == w0.enables + 1, "pwm-high-line: the witness counts {} lines, {} claims, {} releases, {} sets, {} enables", w.lines, w.claims_taken, w.releases, w.sets, w.enables);
    assert!(w.refused_busy == w0.refused_busy + 1 && w.refused_inval == w0.refused_inval + 4 && w.last_refused_line == LINE && w.last_refused_code == pwm::ERR_BUSY, "pwm-high-line: the witness counts busy {} inval {}, the last refusal {} on line {}", w.refused_busy, w.refused_inval, w.last_refused_code, w.last_refused_line);
    kprintln!("[api] PASS pwm-high-line (line {} served on timer {} channel {} at {} ns; 1 busy and 4 invalid refusals counted)", LINE, TIMER_NUMBER, CHANNEL, ns);
}
