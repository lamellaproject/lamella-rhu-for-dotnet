//! The ST timer, converter and serial rows' part of the api flavor's checks:
//! the row's PWM table, its analog table and its serial port stated and
//! enumerated through the kernel's functions, with the refusals past each
//! count. The timers, the converters and the serial block themselves are
//! proven on the board by the flight flavors (`test-pwm-flight`, which reads
//! the pads the timers drive, `test-adc-flight`, which converts the pins, and
//! `test-uart-flight`, which sends and receives on the port's own lines);
//! nothing here claims a line, a channel or a port, so no timer, no
//! converter, no serial block and no pad is touched.

use crate::adc_math::NO_LINE;
use crate::board::facts;
use crate::{api, sched};

pub fn check() {
    kprintln!("[api] pwm-st-table: the PWM lines, the analog channels and the serial port stated and enumerated against the row's tables; nothing claimed");
    let lines = facts::PWM_LINES;
    let n = api::rhu_pwm_count();
    assert!(n == lines.len() as i32, "pwm-st-table: the count reads {}, the table has {}", n, lines.len());
    for (i, l) in lines.iter().enumerate() {
        assert!(api::rhu_pwm_line(i as u32) == l.line as i32, "pwm-st-table: line {} reads {}", i, api::rhu_pwm_line(i as u32));
    }
    assert!(api::rhu_pwm_line(n as u32) == sched::ERR_INVAL, "pwm-st-table: an index past the count not refused EINVAL");
    let channels = facts::ADC_CHANNELS;
    let (mut a, mut b, mut c, mut d) = (0u32, 0u32, 0u32, 0u32);
    assert!(api::rhu_adc_count() == channels.len() as i32, "pwm-st-table: the analog count reads {}, the table has {}", api::rhu_adc_count(), channels.len());
    for (i, ch) in channels.iter().enumerate() {
        let block = &facts::ADC_BLOCKS[ch.block as usize];
        let r = api::rhu_adc_channel(i as u32, &mut a, &mut b, &mut c, &mut d);
        let line = if ch.line == NO_LINE { u32::MAX } else { ch.line as u32 };
        assert!(r == 0 && a == block.number && b == ch.channel as u32 && c == line && d == block.bits, "pwm-st-table: analog position {} answered {} with block {} channel {} line {} bits {}", i, r, a, b, c, d);
    }
    assert!(api::rhu_adc_channel(channels.len() as u32, &mut a, &mut b, &mut c, &mut d) == sched::ERR_INVAL, "pwm-st-table: an analog position past the count not refused EINVAL");
    for block in facts::ADC_BLOCKS {
        assert!(api::rhu_adc_reference(block.number, &mut a) == 0 && a == block.reference_mv, "pwm-st-table: block {}'s reference reads {} mV", block.number, a);
    }
    assert!(api::rhu_adc_read(0) == sched::ERR_INVAL && api::rhu_adc_release(0) == sched::ERR_INVAL, "pwm-st-table: a read or a release of a channel nobody holds not refused EINVAL");
    let ports = facts::UART_ROWS;
    assert!(api::rhu_uart_count() == ports.len() as i32, "pwm-st-table: the serial count reads {}, the table has {}", api::rhu_uart_count(), ports.len());
    for (i, p) in ports.iter().enumerate() {
        let r = api::rhu_uart_port(i as u32, &mut a, &mut b, &mut c, &mut d);
        assert!(r == 0 && a == p.block as u32 && b == p.tx as u32 && c == p.rx as u32 && d == crate::stm32_usart_math::CLOCK_HZ, "pwm-st-table: serial position {} answered {} with block {} lines {} and {} clock {}", i, r, a, b, c, d);
    }
    assert!(api::rhu_uart_port(ports.len() as u32, &mut a, &mut b, &mut c, &mut d) == sched::ERR_INVAL, "pwm-st-table: a serial position past the count not refused EINVAL");
    kprintln!("[api] PASS pwm-st-table ({} PWM lines on {} timers, {} analog channels on {} blocks and {} serial port stated and enumerated; a position past each count refused; a channel nobody holds refuses its read and release)", lines.len(), facts::PWM_TIMERS.len(), channels.len(), facts::ADC_BLOCKS.len(), ports.len());
}
