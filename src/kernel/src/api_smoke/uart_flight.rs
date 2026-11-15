//! The ST serial rows' flight check, the flight flavor of the row's api
//! image (`test-uart-flight`): the kernel's serial API over the chip's own
//! serial block on the board's connector pins D1 and D0 -- the block's
//! bring-up read from the chip, the table stated and enumerated, the
//! divisor and the frame fields read back from the block at each rate and
//! frame, the frames and rates the block cannot serve refused, every byte
//! value out and back over the chip's own loop, the transmitter timed
//! against the kernel's clock, the claims refused while the port is held,
//! the port given back -- and one case on a jumper wire from D1 to D0,
//! which reads skipped when the wire is not there. The image runs this
//! check alone.
//!
//! THE VERDICTS. Every case ends on one line: PASS, REFUSED (the row did
//! what the case forbids), SKIPPED (not flown, with the reason) or OBSERVED
//! (a reading that decides nothing). A refusal does not stop the check; it
//! ends with its summary, the END line and the exit function, 0 without a
//! refusal.
//!
//! THE CHECK DRIVES D1 ALONE: as the port's transmit line and, in the wire
//! case's first lines, as a plain output. D0 is only ever an input. No PWM
//! line and no analog channel is claimed.
//!
//! THE CONTROL (`test-uart-doctored` beside it): the backend writes the
//! divisor twice as long as the rate rule gives, so the wire runs at half
//! the rate the function answers. A loop cannot see that: the transmitter
//! and the receiver share the wrong divisor and every byte comes back
//! equal. Two cases must refuse it: `uart0-registers`, which reads the
//! divisor back from the block, and `uart0-rate`, which times the
//! transmitter; the loop and the wire cases pass at half the rates they
//! name.

use core::fmt;
use core::ptr::{addr_of, read_volatile};

use crate::board::facts;
use crate::board::uart_backend as hw;
use crate::stm32_usart_math as m;
use crate::uart::{UartWitness, FLUSH_RX, PARITY_EVEN, PARITY_MARK, PARITY_NONE, PARITY_ODD, PARITY_SPACE};
use crate::{api, gpio_math, sched};

/// The two codes the cases read.
const BUSY: i32 = crate::uart::ERR_BUSY;
const INVAL: i32 = sched::ERR_INVAL;
/// A millisecond, in the functions' nanoseconds.
const MS: u64 = 1_000_000;
/// The longest wait for a transfer's last byte: the slowest one, 256 bytes
/// at 9600 Bd, takes 267 ms, and twice that under the control.
const WAIT_MS: u64 = 3_000;
/// The bytes of the wire's unjudged run at the top rate.
const TOP_BYTES: u32 = 4_000;

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

    fn skipped(&mut self, case: &str, why: fmt::Arguments) {
        self.skipped += 1;
        kprintln!("[flight] SKIPPED {}: not flown: {}", case, why);
    }

    fn observed(&mut self, case: &str, what: fmt::Arguments) {
        self.observed += 1;
        kprintln!("[flight] OBSERVED {}: {}", case, what);
    }
}

/// The port: its row of the table and its block.
fn row() -> &'static crate::console::UartRow {
    &facts::UART_ROWS[0]
}

fn block() -> &'static m::Block {
    &facts::UART_BLOCKS[0]
}

/// The block's name as its manufacturer writes it.
struct Name;

impl fmt::Display for Name {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}{}", if matches!(block().law, m::Law::LowPower) { "LPUART" } else { "USART" }, row().block)
    }
}

/// A time in nanoseconds printed in milliseconds with two decimals.
struct Millis(u64);

impl fmt::Display for Millis {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}.{:02}", self.0 / MS, self.0 % MS / 10_000)
    }
}

fn witness() -> UartWitness {
    unsafe { read_volatile(addr_of!(crate::uart::WITNESS)) }
}

/// The receive faults counted since a snapshot: overruns, framing errors,
/// parity errors, and bytes a full ring dropped.
fn faults_since(w0: &UartWitness) -> (u32, u32, u32, u32) {
    let w = witness();
    (w.overruns.wrapping_sub(w0.overruns), w.framing.wrapping_sub(w0.framing), w.parity.wrapping_sub(w0.parity), w.rx_dropped.wrapping_sub(w0.rx_dropped))
}

pub fn run() {
    let mut f = Flight { pass: 0, refused: 0, skipped: 0, observed: 0 };
    kprintln!("[flight] the serial flight check starts {} ms after the boot: {} on line {} (transmit, D1) and line {} (receive, D0); it claims no PWM line and no analog channel, drives D1 alone and only reads D0", sched::now_ms(), Name, row().tx, row().rx);
    let h = api::rhu_uart_claim(row().block as u32);
    if h < 0 {
        f.refused("uart0-power", format_args!("the claim of {} answered {}", Name, h));
    } else {
        power(&mut f);
        table(&mut f, h);
        registers(&mut f, h);
        frames(&mut f, h);
        looped(&mut f, h);
        rate(&mut f, h);
        busy(&mut f, h);
        released(&mut f, h);
        wire(&mut f);
    }
    kprintln!("[flight] SUMMARY leg uart: {} PASS, {} REFUSED, {} SKIPPED, {} OBSERVED", f.pass, f.refused, f.skipped, f.observed);
    kprintln!("[flight] END");
    api::rhu_exit(if f.refused == 0 { 0 } else { 1 })
}

/// The block after the first claim, read from the chip: its bus clock on,
/// its selector at the internal oscillator, the block enabled with both
/// acknowledge flags and no acknowledge missed; the FIFO mode as the row's
/// facts state it; on a row with a second I/O supply, its valid bit set.
fn power(f: &mut Flight) {
    let (clocked, selected, status, regs, missed, supply) = hw::probe(0);
    let acks = m::ISR_TEACK | m::ISR_REACK;
    let fifo = regs[2] & m::CR1_FIFOEN != 0;
    let supply_wanted = if cfg!(rhu_io2_supply) { 1 } else { 2 };
    if clocked && selected && regs[2] & m::CR1_UE != 0 && status & acks == acks && missed == 0 && fifo == block().fifo && supply == supply_wanted {
        let tail = if cfg!(rhu_io2_supply) { "; the FIFO mode is on and the second I/O supply's valid bit reads set" } else { "; the block has no FIFO and the row states no second I/O supply" };
        f.pass("uart0-power", format_args!("{} after its first claim: the bus clock is on, the selector reads the 16 MHz internal oscillator, the block is enabled and both acknowledge flags read set (the status register reads {:#010x}, the first control register {:#010x}){}", Name, status, regs[2], tail));
    } else {
        f.refused("uart0-power", format_args!("{} after its first claim: the bus clock on: {}; the selector at the internal oscillator: {}; the status register reads {:#010x} (both acknowledge flags, {:#010x}, wanted); the first control register reads {:#010x} (the enable wanted, the FIFO mode {}); {} acknowledges missed; the supply word reads {} ({} wanted: 1 the valid bit set, 2 the row states no such supply)", Name, clocked, selected, status, acks, regs[2], if block().fifo { "on" } else { "off" }, missed, supply, supply_wanted));
    }
}

/// The count, the port's facts through the enumeration and through the
/// held port's own function, and the refusal past the count.
fn table(f: &mut Flight, h: i32) {
    let r = row();
    let (mut a, mut b, mut c, mut d) = (0u32, 0u32, 0u32, 0u32);
    let (mut ia, mut ib, mut ic, mut id) = (0u32, 0u32, 0u32, 0u32);
    let (mut s0, mut s1, mut s2, mut s3) = (0u32, 0u32, 0u32, 0u32);
    let n = api::rhu_uart_count();
    let p = api::rhu_uart_port(0, &mut a, &mut b, &mut c, &mut d);
    let i = api::rhu_uart_info(h, &mut ia, &mut ib, &mut ic, &mut id);
    let past = api::rhu_uart_port(n.max(0) as u32, &mut s0, &mut s1, &mut s2, &mut s3);
    let stated = (r.block as u32, r.tx as u32, r.rx as u32, m::CLOCK_HZ);
    if n == 1 && p == 0 && i == 0 && (a, b, c, d) == stated && (ia, ib, ic, id) == stated && past == INVAL {
        f.pass("uart0-table", format_args!("1 port: position 0 is block {} ({}) on lines {} and {} at {} Hz, by the enumeration and by the held port's own function; a position past the count refused", a, Name, b, c, d));
    } else {
        f.refused("uart0-table", format_args!("the count reads {}; position 0 answered {} with block {} lines {} and {} clock {}; the held port answered {} with block {} lines {} and {} clock {} (block {} lines {} and {} clock {} wanted); a position past the count answered {}", n, p, a, b, c, d, i, ia, ib, ic, id, stated.0, stated.1, stated.2, stated.3, past));
    }
}

/// The settings the registers case walks: the rate, the data bits, the
/// parity, the stop bits.
const SETTINGS: [(u32, u32, u32, u32); 5] = [(9_600, 8, PARITY_NONE, 1), (115_200, 8, PARITY_NONE, 1), (1_000_000, 8, PARITY_NONE, 1), (115_200, 7, PARITY_EVEN, 1), (115_200, 8, PARITY_ODD, 2)];

/// At each setting: the rate the function answers equal to the rate rule's
/// arithmetic, and the divisor, the prescaler's code on a block with one,
/// the frame fields and the stop field READ BACK FROM THE BLOCK equal to
/// the pure module's, with the block enabled both ways.
fn registers(f: &mut Flight, h: i32) {
    let b = block();
    let low_power = matches!(b.law, m::Law::LowPower);
    let frame = m::CR1_M0 | m::CR1_M1 | m::CR1_PCE | m::CR1_PS;
    let on = m::CR1_UE | m::CR1_TE | m::CR1_RE | m::CR1_RXNEIE | if b.fifo { m::CR1_FIFOEN } else { 0 };
    let mut wrong = 0u32;
    let mut first = (0u32, 0u32, 0u32, 0u32, 0i32, 0u32, [0u32; 5]);
    for &(baud, data, parity, stop) in SETTINGS.iter() {
        let mut achieved = 0u32;
        let r = api::rhu_uart_configure(h, baud, data, parity, stop, &mut achieved);
        let regs = hw::probe(0).3;
        let ok = match m::divisor(b.law, baud) {
            Some((word, got)) => r == 0 && achieved == got && regs[0] == word & 0xF_FFFF && (!low_power || regs[1] & 0xF == word >> 20) && regs[2] & frame == m::cr1_frame(data, parity) && regs[2] & on == on && regs[3] & (0b11 << 12) == m::cr2_stop(stop),
            None => false,
        };
        if !ok {
            if wrong == 0 {
                first = (baud, data, parity, stop, r, achieved, regs);
            }
            wrong += 1;
        }
    }
    let mut achieved = 0u32;
    let back = api::rhu_uart_configure(h, 115_200, 8, PARITY_NONE, 1, &mut achieved);
    let word = |baud: u32| m::divisor(b.law, baud).unwrap_or((0, 0));
    if wrong == 0 && back == 0 {
        f.pass("uart0-registers", format_args!("{} settings, each read back from the block: 9600 Bd is the divisor {} and {} Bd, 115200 Bd the divisor {} and {} Bd, 1000000 Bd the divisor {} and {} Bd{}; 7 data bits with even parity and 8 with odd parity and 2 stop bits read back in the word-length, parity and stop fields; the function's rate is the rule's at every one", SETTINGS.len(), word(9_600).0 & 0xF_FFFF, word(9_600).1, word(115_200).0 & 0xF_FFFF, word(115_200).1, word(1_000_000).0 & 0xF_FFFF, word(1_000_000).1, if low_power { " (the prescaler's code read back beside each)" } else { "" }));
    } else {
        let (baud, data, parity, stop, r, got, regs) = first;
        let (w, rule) = word(baud);
        f.refused("uart0-registers", format_args!("{} of {} settings differ; the first: {} Bd with {} data bits, parity {} and {} stop bits answered {} and {} Bd where the rule gives the divisor {} with the prescaler's code {} and {} Bd; the block reads the divisor {}, the prescaler register {}, the first control register {:#010x} and the second {:#010x} (the frame fields {:#010x} and the stop field {:#06x} wanted); the configure back to 115200 Bd answered {}", wrong, SETTINGS.len(), baud, data, parity, stop, r, got, w & 0xF_FFFF, w >> 20, rule, regs[0], regs[1], regs[2], regs[3], m::cr1_frame(data, parity), m::cr2_stop(stop), back));
    }
}

/// What the blocks cannot serve, each refused with the invalid code and
/// the block's registers unmoved: 5, 6 and 9 data bits, mark and space
/// parity, 0 and 3 stop bits, a rate one step under the rule's floor and
/// one over its ceiling.
fn frames(f: &mut Flight, h: i32) {
    let b = block();
    let (under, over) = match b.law {
        m::Law::Usart16 => (244, 1_020_409),
        m::Law::LowPower => (15, 5_333_334),
    };
    let asks: [(u32, u32, u32, u32); 9] = [(115_200, 5, PARITY_NONE, 1), (115_200, 6, PARITY_NONE, 1), (115_200, 9, PARITY_NONE, 1), (115_200, 8, PARITY_MARK, 1), (115_200, 8, PARITY_SPACE, 1), (115_200, 8, PARITY_NONE, 0), (115_200, 8, PARITY_NONE, 3), (under, 8, PARITY_NONE, 1), (over, 8, PARITY_NONE, 1)];
    let mut achieved = 0u32;
    let base = api::rhu_uart_configure(h, 115_200, 8, PARITY_NONE, 1, &mut achieved);
    let before = hw::probe(0).3;
    let mut taken = 0u32;
    let mut first = (0u32, 0u32, 0u32, 0u32, 0i32);
    for &(baud, data, parity, stop) in asks.iter() {
        let r = api::rhu_uart_configure(h, baud, data, parity, stop, &mut achieved);
        if r != INVAL {
            if taken == 0 {
                first = (baud, data, parity, stop, r);
            }
            taken += 1;
        }
    }
    let after = hw::probe(0).3;
    let rule = m::divisor(b.law, under).is_none() && m::divisor(b.law, over).is_none();
    if base == 0 && taken == 0 && before == after && rule {
        f.pass("uart0-frames", format_args!("{} settings the block cannot serve, each refused with the invalid code and the block's registers unmoved: 5, 6 and 9 data bits, mark and space parity, 0 and 3 stop bits, {} Bd (under the rule's floor) and {} Bd (over its ceiling)", asks.len(), under, over));
    } else {
        f.refused("uart0-frames", format_args!("{} of {} settings were not refused with the invalid code; the first: {} Bd with {} data bits, parity {} and {} stop bits answered {}; the registers unmoved: {}; the rule refuses {} and {} Bd: {}; the configure before them answered {}", taken, asks.len(), first.0, first.1, first.2, first.3, first.4, before == after, under, over, rule, base));
    }
}

/// The pattern's byte at an index: every value once in 256 bytes, and every
/// 7-bit value once in 128 under the 7-bit mask.
fn pattern(i: usize, mask: u8) -> u8 {
    ((i as u8) ^ 0x5A) & mask
}

/// One transfer out and back on a held port: `n` bytes of the pattern (256
/// at the most) written in partial writes while what arrives is read, until
/// all are back or the wait runs out. Answers the bytes back, the first
/// index whose byte differs (none when all that came back are equal), and
/// the writes it took.
fn round_trip(h: i32, n: usize, mask: u8) -> (usize, Option<usize>, u32) {
    let n = n.min(256);
    let mut out = [0u8; 256];
    for (i, b) in out.iter_mut().enumerate() {
        *b = pattern(i, mask);
    }
    let mut back = [0u8; 256];
    let (mut sent, mut recv, mut writes) = (0usize, 0usize, 0u32);
    let deadline = api::rhu_now_ns() + WAIT_MS * MS;
    while recv < n {
        let mut moved = false;
        if sent < n {
            let t = api::rhu_uart_write(h, out[sent..n].as_ptr(), (n - sent) as u32);
            if t < 0 {
                break;
            }
            if t > 0 {
                sent += t as usize;
                writes += 1;
                moved = true;
            }
        }
        let r = api::rhu_uart_read(h, back[recv..n].as_mut_ptr(), (n - recv) as u32);
        if r < 0 {
            break;
        }
        if r > 0 {
            recv += r as usize;
            moved = true;
        }
        if !moved {
            if api::rhu_now_ns() >= deadline {
                break;
            }
            api::rhu_sleep_ns(MS);
        }
    }
    (recv, (0..recv).find(|&i| back[i] != out[i]), writes)
}

/// The runs of the loop case and of the wire case: the rate, the data
/// bits, the parity, the stop bits, the bytes.
const LOOP_RUNS: [(u32, u32, u32, u32, usize); 5] = [(9_600, 8, PARITY_NONE, 1, 256), (57_600, 8, PARITY_NONE, 1, 256), (115_200, 8, PARITY_NONE, 1, 256), (115_200, 7, PARITY_EVEN, 1, 128), (115_200, 8, PARITY_ODD, 2, 256)];
const WIRE_RUNS: [(u32, u32, u32, u32, usize); 3] = [(9_600, 8, PARITY_NONE, 1, 256), (57_600, 8, PARITY_NONE, 1, 256), (115_200, 8, PARITY_NONE, 1, 256)];

/// A list of runs on a held port, each configured, the receiver flushed,
/// and the pattern sent and read back. Answers the runs that did not come
/// back whole and equal, and the first such run's rate, data bits,
/// configure answer, bytes back and first differing index.
fn runs(h: i32, list: &[(u32, u32, u32, u32, usize)]) -> (u32, (u32, u32, i32, usize, Option<usize>)) {
    let mut wrong = 0u32;
    let mut first = (0u32, 0u32, 0i32, 0usize, None);
    for &(baud, data, parity, stop, n) in list.iter() {
        let mut achieved = 0u32;
        let r = api::rhu_uart_configure(h, baud, data, parity, stop, &mut achieved);
        let _ = api::rhu_uart_flush(h, FLUSH_RX);
        let (recv, differs, _) = if r == 0 { round_trip(h, n, if data == 7 { 0x7F } else { 0xFF }) } else { (0, None, 0) };
        if r != 0 || recv != n || differs.is_some() {
            if wrong == 0 {
                first = (baud, data, r, recv, differs);
            }
            wrong += 1;
        }
    }
    (wrong, first)
}

/// The chip's own loop on: every byte value out and back at three rates
/// and in two more frames, no receive fault counted.
fn looped(f: &mut Flight, h: i32) {
    let on = crate::uart::loopback(h, true);
    let w0 = witness();
    let (wrong, first) = runs(h, &LOOP_RUNS);
    let faults = faults_since(&w0);
    let off = crate::uart::loopback(h, false);
    if on && off && wrong == 0 && faults == (0, 0, 0, 0) {
        f.pass("uart0-loop", format_args!("the chip's own loop on: 256 bytes, every value, out and back equal at 9600, 57600 and 115200 Bd with 8 data bits, no parity and 1 stop bit; 128 bytes, every 7-bit value, with even parity; 256 with odd parity and 2 stop bits; no overrun, framing error, parity error or dropped byte counted"));
    } else {
        f.refused("uart0-loop", format_args!("the loop turned on and off: {} and {}; {} of {} runs did not come back whole and equal; the first: {} Bd with {} data bits, the configure answered {}, {} bytes back, the first differing index {:?}; counted meanwhile: {} overruns, {} framing errors, {} parity errors, {} dropped bytes", on, off, wrong, LOOP_RUNS.len(), first.0, first.1, first.2, first.3, first.4, faults.0, faults.1, faults.2, faults.3));
    }
}

/// The runs of the rate case: the rate and the bytes.
const RATE_RUNS: [(u32, u32); 3] = [(9_600, 96), (115_200, 1_152), (1_000_000, 4_000)];

/// The transmitter alone, the loop off, timed by the kernel's clock from
/// the first write to the transmitter idle: each run inside 5 percent of
/// the arithmetic's time for its bytes at the rate the function answered
/// (10 bits a byte), and 3 ms more either way. The check's thread polls
/// without a sleep, so the ring is refilled before it runs empty. What
/// arrives on the receive line meanwhile is discarded and not judged.
fn rate(f: &mut Flight, h: i32) {
    let chunk = [0x55u8; 64];
    let mut got = [(0u64, 0u64, 0u64, false); 3];
    for (i, &(baud, bytes)) in RATE_RUNS.iter().enumerate() {
        let mut achieved = 0u32;
        if api::rhu_uart_configure(h, baud, 8, PARITY_NONE, 1, &mut achieved) != 0 || achieved == 0 {
            continue;
        }
        let time = bytes as u64 * 10 * 1_000_000_000 / achieved as u64;
        let (lo, hi) = (time * 95 / 100 - 3 * MS, time * 105 / 100 + 3 * MS);
        let t0 = api::rhu_now_ns();
        let deadline = t0 + 4 * time + 100 * MS;
        let mut sent = 0u32;
        let mut sound = true;
        while sent < bytes && sound && api::rhu_now_ns() < deadline {
            let w = api::rhu_uart_write(h, chunk.as_ptr(), (bytes - sent).min(chunk.len() as u32));
            sound = w >= 0;
            sent += w.max(0) as u32;
        }
        let (mut avail, mut pending, mut room) = (0u32, 1u32, 0u32);
        while sent == bytes && sound && pending != 0 && api::rhu_now_ns() < deadline {
            sound = api::rhu_uart_status(h, &mut avail, &mut pending, &mut room) == 0;
        }
        let took = api::rhu_now_ns() - t0;
        let _ = api::rhu_uart_flush(h, FLUSH_RX);
        got[i] = (took, lo, hi, sound && sent == bytes && pending == 0);
    }
    let inside = |g: &(u64, u64, u64, bool)| g.3 && g.0 >= g.1 && g.0 <= g.2;
    let all = got.iter().all(inside);
    let mut achieved = 0u32;
    let back = api::rhu_uart_configure(h, 115_200, 8, PARITY_NONE, 1, &mut achieved);
    let _ = api::rhu_uart_flush(h, FLUSH_RX);
    if all && back == 0 {
        f.pass("uart0-rate", format_args!("the transmitter alone, timed from the first write to the transmitter idle: 96 bytes at 9600 Bd took {} ms (inside {} to {}), 1152 at 115200 Bd {} ms (inside {} to {}), 4000 at 1000000 Bd {} ms (inside {} to {})", Millis(got[0].0), Millis(got[0].1), Millis(got[0].2), Millis(got[1].0), Millis(got[1].1), Millis(got[1].2), Millis(got[2].0), Millis(got[2].1), Millis(got[2].2)));
    } else {
        f.refused("uart0-rate", format_args!("the transmitter alone, timed from the first write to the transmitter idle: 96 bytes at 9600 Bd took {} ms ({} to {} wanted; every byte written and the wire idle inside the wait: {}), 1152 at 115200 Bd {} ms ({} to {} wanted; {}), 4000 at 1000000 Bd {} ms ({} to {} wanted; {}); the configure back to 115200 Bd answered {}", Millis(got[0].0), Millis(got[0].1), Millis(got[0].2), got[0].3, Millis(got[1].0), Millis(got[1].1), Millis(got[1].2), got[1].3, Millis(got[2].0), Millis(got[2].1), Millis(got[2].2), got[2].3, back));
    }
}

/// While the port is held: a second claim of it refused with the busy
/// code, and a GPIO claim of each of its two lines refused with the busy
/// code.
fn busy(f: &mut Flight, h: i32) {
    let r = row();
    let again = api::rhu_uart_claim(r.block as u32);
    let g_tx = api::rhu_gpio_claim(r.tx as u32, gpio_math::MODE_INPUT, gpio_math::PULL_NONE, 0);
    let g_rx = api::rhu_gpio_claim(r.rx as u32, gpio_math::MODE_INPUT, gpio_math::PULL_NONE, 0);
    if g_tx == 0 {
        let _ = api::rhu_gpio_release(r.tx as u32);
    }
    if g_rx == 0 {
        let _ = api::rhu_gpio_release(r.rx as u32);
    }
    if again == BUSY && g_tx == BUSY && g_rx == BUSY {
        f.pass("uart0-busy", format_args!("{} held as handle {}: a second claim of it refused with the busy code, and a GPIO claim of line {} and of line {} refused with the busy code", Name, h, r.tx, r.rx));
    } else {
        f.refused("uart0-busy", format_args!("{} held as handle {}: a second claim answered {}, a GPIO claim of line {} answered {} and of line {} answered {} (the busy code {} wanted three times)", Name, h, again, r.tx, g_tx, r.rx, g_rx, BUSY));
    }
}

/// The port given back: the release answered, the block disabled, each of
/// the two lines claimed as a plain line and given back, the witness
/// counting as many releases as claims, and a second release of the handle
/// refused.
fn released(f: &mut Flight, h: i32) {
    let r = row();
    let rel = api::rhu_uart_release(h);
    let up = hw::is_up(0);
    let w = witness();
    let c_tx = api::rhu_gpio_claim(r.tx as u32, gpio_math::MODE_INPUT, gpio_math::PULL_NONE, 0);
    let r_tx = if c_tx == 0 { api::rhu_gpio_release(r.tx as u32) } else { c_tx };
    let c_rx = api::rhu_gpio_claim(r.rx as u32, gpio_math::MODE_INPUT, gpio_math::PULL_NONE, 0);
    let r_rx = if c_rx == 0 { api::rhu_gpio_release(r.rx as u32) } else { c_rx };
    let again = api::rhu_uart_release(h);
    if rel == 0 && !up && w.claims_taken == w.releases && c_tx == 0 && r_tx == 0 && c_rx == 0 && r_rx == 0 && again == INVAL {
        f.pass("uart0-release", format_args!("the port given back: the block reads disabled, lines {} and {} each claimed as a plain line and given back, the witness counts {} claims and {} releases, a second release of the handle refused", r.tx, r.rx, w.claims_taken, w.releases));
    } else {
        f.refused("uart0-release", format_args!("the release answered {}; the block reads enabled: {}; the witness counts {} claims and {} releases; line {} claimed and given back: {} and {}; line {}: {} and {}; a second release answered {} ({} wanted)", rel, up, w.claims_taken, w.releases, r.tx, c_tx, r_tx, r.rx, c_rx, r_rx, again, INVAL));
    }
}

/// The wire from D1 to D0. Found first with the two lines as plain lines:
/// D1 driven high and low eight times, D0 read with its pull-up, so that
/// without a wire it follows four times of eight. Then the port again with
/// the chip's own loop off: every byte value out and back over the wire at
/// three rates with no receive fault counted; a break held 20 ms, counted
/// by the functions at the request and once by the receiver, with no byte
/// delivered from it; and every byte value once more, which shows the
/// transmit line is the block's again. Last, not judged: the top rate over
/// the wire.
fn wire(f: &mut Flight) {
    let r = row();
    let (out, inp) = (r.tx as u32, r.rx as u32);
    let c_in = api::rhu_gpio_claim(inp, gpio_math::MODE_INPUT, gpio_math::PULL_UP, 0);
    let c_out = api::rhu_gpio_claim(out, gpio_math::MODE_OUTPUT, gpio_math::PULL_NONE, 1);
    if c_in != 0 || c_out != 0 {
        if c_in == 0 {
            let _ = api::rhu_gpio_release(inp);
        }
        if c_out == 0 {
            let _ = api::rhu_gpio_release(out);
        }
        f.refused("uart1-wire", format_args!("lines {} and {} could not be taken as plain lines: the claims answered {} and {}", out, inp, c_out, c_in));
        return;
    }
    let mut follows = 0u32;
    for i in 0..8u32 {
        let level = i & 1;
        let _ = api::rhu_gpio_write(out, level);
        sched::sleep_ms(2);
        follows += (api::rhu_gpio_read(inp) == level as i32) as u32;
    }
    let r_out = api::rhu_gpio_release(out);
    let r_in = api::rhu_gpio_release(inp);
    if follows != 8 {
        f.skipped("uart1-wire", format_args!("no wire from D1 to D0: line {} driven low and high eight times as a plain output, line {} followed {} times of 8; the top rate over the wire is not flown either", out, inp, follows));
        return;
    }
    let h = api::rhu_uart_claim(r.block as u32);
    if r_out != 0 || r_in != 0 || h < 0 {
        f.refused("uart1-wire", format_args!("the wire was found, and then the plain lines' releases answered {} and {} and the port's claim {}", r_out, r_in, h));
        return;
    }
    let w0 = witness();
    let (wrong, first) = runs(h, &WIRE_RUNS);
    let faults = faults_since(&w0);
    // The break: held 20 ms on an idle line at 115200 Bd.
    let mut achieved = 0u32;
    let cfg = api::rhu_uart_configure(h, 115_200, 8, PARITY_NONE, 1, &mut achieved);
    let _ = api::rhu_uart_flush(h, FLUSH_RX);
    let wb = witness();
    let b_on = api::rhu_uart_break(h, 1);
    sched::sleep_ms(20);
    let b_off = api::rhu_uart_break(h, 0);
    sched::sleep_ms(5);
    let (mut avail, mut pending, mut room) = (0u32, 0u32, 0u32);
    let st = api::rhu_uart_status(h, &mut avail, &mut pending, &mut room);
    let we = witness();
    let breaks = we.breaks_seen.wrapping_sub(wb.breaks_seen);
    let delivered = we.bytes_in.wrapping_sub(wb.bytes_in);
    let framing = we.framing.wrapping_sub(wb.framing);
    let (after, differs, _) = round_trip(h, 256, 0xFF);
    let sound = wrong == 0 && faults == (0, 0, 0, 0) && cfg == 0 && b_on == 0 && b_off == 0 && st == 0 && breaks == 2 && delivered == 0 && avail == 0 && framing == 0 && after == 256 && differs.is_none();
    if sound {
        f.pass("uart1-wire", format_args!("the wire from D1 to D0 found (8 levels of 8 followed) and the port on it with the chip's own loop off: 256 bytes, every value, out and back equal over the wire at 9600, 57600 and 115200 Bd, no overrun, framing error, parity error or dropped byte counted; a break held 20 ms counted twice by the witness, once at the request and once by the receiver, and no byte delivered from it; 256 bytes out and back equal after it"));
    } else {
        f.refused("uart1-wire", format_args!("the wire was found (8 levels of 8 followed); {} of {} runs did not come back whole and equal over it; the first: {} Bd, the configure answered {}, {} bytes back, the first differing index {:?}; counted meanwhile: {} overruns, {} framing errors, {} parity errors, {} dropped bytes; the break: the configure answered {}, the requests {} and {}, the witness counted {} breaks (2 wanted: the request's and the receiver's), {} framing errors and {} bytes delivered, {} bytes waiting; after it {} bytes of 256 came back, the first differing index {:?}", wrong, WIRE_RUNS.len(), first.0, first.2, first.3, first.4, faults.0, faults.1, faults.2, faults.3, cfg, b_on, b_off, breaks, framing, delivered, avail, after, differs));
    }
    top(f, h);
    let rel = api::rhu_uart_release(h);
    if rel != 0 {
        f.refused("uart1-wire", format_args!("the port's release after the wire's runs answered {}", rel));
    }
}

/// The top rate over the wire, printed and not judged: 4000 bytes of a
/// counting pattern at 1000000 Bd, the bytes back, the places where the
/// count jumps and the bytes those jumps skip, with the receiver's counts.
/// The STM32F769's block takes one interrupt a byte, and what stands
/// between the two pins is the board's and the shield's.
fn top(f: &mut Flight, h: i32) {
    let mut achieved = 0u32;
    let cfg = api::rhu_uart_configure(h, 1_000_000, 8, PARITY_NONE, 1, &mut achieved);
    let _ = api::rhu_uart_flush(h, FLUSH_RX);
    let w0 = witness();
    let mut chunk = [0u8; 64];
    let mut back = [0u8; 64];
    let (mut sent, mut recv, mut jumps, mut skipped) = (0u32, 0u32, 0u32, 0u32);
    let mut expect = 0u8;
    let start = api::rhu_now_ns();
    let mut last = start;
    while cfg == 0 {
        let mut moved = false;
        if sent < TOP_BYTES {
            let n = (TOP_BYTES - sent).min(chunk.len() as u32);
            for (i, b) in chunk.iter_mut().enumerate() {
                *b = (sent as usize + i) as u8;
            }
            let w = api::rhu_uart_write(h, chunk.as_ptr(), n);
            if w > 0 {
                sent += w as u32;
                moved = true;
            }
        }
        let r = api::rhu_uart_read(h, back.as_mut_ptr(), back.len() as u32);
        if r > 0 {
            for &b in back[..r as usize].iter() {
                if b != expect {
                    jumps += 1;
                    skipped += b.wrapping_sub(expect) as u32;
                }
                expect = b.wrapping_add(1);
            }
            recv += r as u32;
            moved = true;
        }
        let now = api::rhu_now_ns();
        if moved {
            last = now;
        } else if sent == TOP_BYTES && now - last > 20 * MS {
            break;
        }
        if now - start > WAIT_MS * MS {
            break;
        }
    }
    let faults = faults_since(&w0);
    let _ = api::rhu_uart_flush(h, FLUSH_RX);
    f.observed("uart1-wire-top", format_args!("{} bytes of a counting pattern at {} Bd over the wire (the configure answered {}): {} written, {} back, the count jumped {} times over {} bytes; counted by the receiver: {} overruns, {} framing errors, {} parity errors, {} bytes dropped by a full ring", TOP_BYTES, achieved, cfg, sent, recv, jumps, skipped, faults.0, faults.1, faults.2, faults.3));
}
