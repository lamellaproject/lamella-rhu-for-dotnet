//! The RP2350's PWM backend: the block's slices behind the doors, in the
//! shape of the emulator rows' virtual timers. A slice (the RP2350
//! datasheet, chapter 12.5) is a 16-bit counter that runs from 0 to TOP and
//! wraps, compared against one 16-bit value per output; it advances every
//! DIV.INT system clocks (the divider's fractional part is left at zero:
//! its first-order delta-sigma spacing would jitter the edges). The block
//! at 0x400a8000, a slice at a stride of 0x14: CSR (EN bit 0, A_INV bit 2,
//! B_INV bit 3; the other fields at their reset values -- free running,
//! trailing edge), DIV (INT 11:4), CTR, CC (A 15:0, B 31:16), TOP (Table
//! 1131, Tables 1132 to 1136). The output is high while the count is below
//! the compare; CC = 0 is a toggle-free 0 percent and CC = TOP + 1 a
//! toggle-free 100 percent (12.5.2.2); TOP and CC are double-buffered and
//! take effect at the wrap (12.5.2.3), so a compare change on a running
//! slice is written live, and a period change stops the slice, writes it
//! whole and restarts it from 0 (a slice's other output is never held on a
//! different period: the doors refuse that first). A line reaches its
//! slice output through the pad's function 4 (Table 645); while the line is
//! parked its output is overridden to the polarity's idle level through
//! the control register's OUTOVER field (Table 651), so an unset or stopped
//! slice never shows on the pad. The block counts the system clock (Table
//! 1133); its clock gate is WAKE_EN0's CLK_SYS_PWM (Table 587) and its
//! reset RESETS bit 16 (Table 535): both are enforced once per boot at the
//! first claim, whatever a resident image left behind. No interrupt;
//! nothing masks. The record `__rhu_rp2350_pwm` is read by symbol.

use core::ptr::{addr_of_mut, read_volatile, write_volatile};
use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use super::rp2350::gpio_backend::{ctrl, pad, sio, GPIO_OE_CLR, GPIO_OUT_CLR};
use super::rp2350::{
    release_reset, CLOCKS, PAD_IE, PAD_ISO, PAD_OD, PAD_PDE, PAD_PUE, RESETS_RESET_DONE, RESETS_RESET_SET,
    RESET_IO_BANK0, RESET_PADS_BANK0, SYSCLK_HZ,
};
use super::rp2350_map as map;
use crate::board::facts;
use crate::critical::with_cs;
use crate::pwm_math::{self, PwmLine, PwmTimer};

/// The block's base address (Table 1131).
const PWM: usize = 0x400a_8000;
/// The stride of one slice's five registers: CH1 at 0x14 after CH0 at 0x00 (Table 1131).
const SLICE_STRIDE: usize = 0x14;
/// CHn_CSR, the slice's control and status register, at +0x00 (Table 1131).
const CSR: usize = 0x00;
/// CHn_DIV, the slice's clock divider, at +0x04 (Table 1131).
const DIV: usize = 0x04;
/// CHn_CTR, the slice's counter, at +0x08 (Table 1131).
const CTR: usize = 0x08;
/// CHn_CC, the slice's two compare values, at +0x0c (Table 1131).
const CC: usize = 0x0c;
/// CHn_TOP, the slice's counter wrap value, at +0x10 (Table 1131).
const TOP: usize = 0x10;
/// CHn_CSR EN, bit 0: the slice counts (Table 1132).
const CSR_EN: u32 = 1 << 0;
/// CHn_CSR A_INV, bit 2: output A inverted (Table 1132).
const CSR_A_INV: u32 = 1 << 2;
/// CHn_CSR B_INV, bit 3: output B inverted (Table 1132).
const CSR_B_INV: u32 = 1 << 3;
/// CHn_DIV INT, bits 11:4: the divider's integer part (Table 1133).
const DIV_INT_SHIFT: u32 = 4;
/// CHn_DIV INT, bits 11:4, as a mask (Table 1133).
const DIV_INT_MASK: u32 = 0xFF << DIV_INT_SHIFT;
/// RESETS.RESET PWM, bit 16 (Table 535).
const RESET_PWM: u32 = 1 << 16;
/// The clock gate's atomic set alias (the RP2350 datasheet's section 2.1.3: the bitmask set at
/// the register's address plus 0x2000; WAKE_EN0 at CLOCKS + 0xac, Table 587).
const WAKE_EN0_SET: *mut u32 = (CLOCKS + 0x2000 + 0xAC) as *mut u32;
/// WAKE_EN0 CLK_SYS_PWM, bit 25: the block's clock gate (Table 587).
const WAKE_EN0_CLK_SYS_PWM: u32 = 1 << 25;
/// The control register's fields (Table 651): the function select, and the
/// output override -- normal (the slice's output), or driven low or high.
/// GPIOn_CTRL FUNCSEL, bits 4:0, at 4: the line's PWM function (Tables 645 and 651).
const FUNCSEL_PWM: u32 = 4;
/// GPIOn_CTRL FUNCSEL at 0x1f: no function (Table 651).
const FUNCSEL_NULL: u32 = 0x1F;
/// GPIOn_CTRL OUTOVER, bits 13:12, at 2: the output driven low (Table 651).
const OUTOVER_LOW: u32 = 0b10 << 12;
/// GPIOn_CTRL OUTOVER, bits 13:12, at 3: the output driven high (Table 651).
const OUTOVER_HIGH: u32 = 0b11 << 12;
/// A short bound for the reset handshake (at least a hundred microseconds
/// of the posture's clock; the block answers within a few clocks).
const SPIN_SHORT: u32 = SYSCLK_HZ / 10_000 / 4;

/// The slices a line of the package reaches, as the facts table states them:
/// the slice's index as the number, the system clock at this posture, two
/// outputs, the divider's integer part 1 to 255, the period's reach 65535
/// counts (a whole period's compare is TOP + 1, held in 16 bits: sections
/// 12.5.2.1 and 12.5.2.2).
pub const fn timers<const N: usize>() -> [PwmTimer; N] {
    let mut t = [PwmTimer { number: 0, clock_hz: SYSCLK_HZ, channels: 2, prescaler_max: 255, period_max: 65535 }; N];
    let mut i = 0;
    while i < N {
        t[i].number = i as u32;
        i += 1;
    }
    t
}

/// Every line of the package on its slice and output, by Table 1130's rule
/// (the pure mapping module beside this one); the pad's function 4 selects
/// the output.
pub const fn lines<const N: usize>() -> [PwmLine; N] {
    let mut l = [PwmLine { line: 0, timer: 0, channel: 0, alternate: FUNCSEL_PWM as u8 }; N];
    let mut n = 0;
    while n < N {
        l[n] = PwmLine { line: n as u16, timer: map::slice_of(n), channel: map::output_of(n), alternate: FUNCSEL_PWM as u8 };
        n += 1;
    }
    l
}

const NT: usize = facts::PWM_TIMERS.len();
const NL: usize = facts::PWM_LINES.len();
const _: () = assert!(NT <= 12, "the chip has twelve slices");

/// The slices' shadows: the pair each is to run (a prescaler of 0 = none).
static PAIR_P: [AtomicU32; NT] = [const { AtomicU32::new(0) }; NT];
static PAIR_A: [AtomicU32; NT] = [const { AtomicU32::new(0) }; NT];
/// Per line: the polarity set, and whether the output is driven.
static POL: [AtomicU32; NL] = [const { AtomicU32::new(0) }; NL];
static DRIVEN: [AtomicU32; NL] = [const { AtomicU32::new(0) }; NL];
static UP: AtomicBool = AtomicBool::new(false);

/// The chip's record, by symbol: the bring-up's verdict (1 = the block reset
/// and released; 2 = the reset handshake missed its bound), the slices
/// restarted on a period change, the compares written live.
#[repr(C)]
pub struct PwmRecord {
    pub up: u32,
    pub restarts: u32,
    pub live: u32,
}

#[unsafe(export_name = "__rhu_rp2350_pwm")]
pub static mut RECORD: PwmRecord = PwmRecord { up: 0, restarts: 0, live: 0 };

unsafe fn bump(p: *mut u32) {
    unsafe { write_volatile(p, read_volatile(p).wrapping_add(1)) }
}

fn reg(timer: usize, off: usize) -> *mut u32 {
    (PWM + SLICE_STRIDE * facts::PWM_TIMERS[timer].number as usize + off) as *mut u32
}

/// The parked output's level: the polarity's idle level.
fn parked(polarity: u32) -> u32 {
    if polarity == pwm_math::POLARITY_INVERSED {
        OUTOVER_HIGH
    } else {
        OUTOVER_LOW
    }
}

/// The block up once per boot: the clock gate set, the block put into reset
/// and released (the done bit polled both ways, the entry under a bound),
/// the pad blocks released as the console's init releases them
/// (idempotent).
fn bring_up() {
    if UP.swap(true, Ordering::AcqRel) {
        return;
    }
    let ok = unsafe {
        write_volatile(WAKE_EN0_SET, WAKE_EN0_CLK_SYS_PWM);
        write_volatile(RESETS_RESET_SET, RESET_PWM);
        let mut n = 0u32;
        let mut ok = true;
        while read_volatile(RESETS_RESET_DONE) & RESET_PWM != 0 {
            n += 1;
            if n > SPIN_SHORT {
                ok = false;
                break;
            }
        }
        release_reset(RESET_PWM | RESET_IO_BANK0 | RESET_PADS_BANK0);
        ok
    };
    with_cs(|| unsafe {
        let r = addr_of_mut!(RECORD);
        write_volatile(addr_of_mut!((*r).up), if ok { 1 } else { 2 });
    });
}

/// The line's pad and function: the SIO output and enable cleared, the
/// function 4 with the output overridden low (parked until a set and an
/// enable), the pad written with its isolation latch set and released after
/// (the RP2350 datasheet's section 9.7) -- the input enabled, no pull, the output enabled (a driven
/// pad reads back through the SIO for a hardware run's own check; the leakage of
/// erratum E9 is an undriven input's).
pub fn configure(index: usize) {
    bring_up();
    let line = facts::PWM_LINES[index].line as u32;
    POL[index].store(pwm_math::POLARITY_NORMAL, Ordering::Release);
    DRIVEN[index].store(0, Ordering::Release);
    with_cs(|| unsafe {
        let (oe_clr, mask) = sio(GPIO_OE_CLR, line);
        write_volatile(oe_clr, mask);
        let (out_clr, _) = sio(GPIO_OUT_CLR, line);
        write_volatile(out_clr, mask);
        write_volatile(ctrl(line), FUNCSEL_PWM | OUTOVER_LOW);
        let p = pad(line);
        let v = (read_volatile(p) & !(PAD_OD | PAD_PUE | PAD_PDE | PAD_ISO)) | PAD_IE;
        write_volatile(p, v | PAD_ISO);
        write_volatile(p, v);
    });
}

/// Back to a function-less pad with no pull and its input enabled (the GPIO
/// backend's released shape), the override normal.
pub fn release(index: usize) {
    let line = facts::PWM_LINES[index].line as u32;
    with_cs(|| unsafe {
        write_volatile(ctrl(line), FUNCSEL_NULL);
        let p = pad(line);
        write_volatile(p, read_volatile(p) & !(PAD_PUE | PAD_PDE));
    });
    DRIVEN[index].store(0, Ordering::Release);
}

/// The pair the slice is to run, recorded; the registers are written whole
/// at the output's set that follows.
pub fn set_timer(timer: usize, prescaler: u32, period: u32) {
    PAIR_P[timer].store(prescaler, Ordering::Release);
    PAIR_A[timer].store(period, Ordering::Release);
}

/// The slice stopped and its counter cleared (no held output has a period).
pub fn clear_timer(timer: usize) {
    PAIR_P[timer].store(0, Ordering::Release);
    PAIR_A[timer].store(0, Ordering::Release);
    with_cs(|| unsafe {
        write_volatile(reg(timer, CSR), read_volatile(reg(timer, CSR)) & !CSR_EN);
        write_volatile(reg(timer, CTR), 0);
    });
}

/// The output's compare and polarity. On a slice already running the
/// recorded pair, the compare's half and the invert bit are written live
/// (taken at the wrap); otherwise the slice is stopped, written whole -- the
/// divider's integer part, TOP as the period less one, the compare, the
/// counter at 0 -- and restarted. The parked level follows the polarity.
pub fn set_channel(index: usize, compare: u32, polarity: u32) {
    let l = &facts::PWM_LINES[index];
    let t = l.timer as usize;
    let b = l.channel == 1;
    let (p, a) = (PAIR_P[t].load(Ordering::Acquire), PAIR_A[t].load(Ordering::Acquire));
    if p == 0 || a == 0 {
        return;
    }
    POL[index].store(polarity, Ordering::Release);
    let line = l.line as u32;
    with_cs(|| unsafe {
        let csr = read_volatile(reg(t, CSR));
        let inv = if b { CSR_B_INV } else { CSR_A_INV };
        let csr_new = if polarity == pwm_math::POLARITY_INVERSED { csr | inv } else { csr & !inv };
        let cc = read_volatile(reg(t, CC));
        let cc_new = if b { (cc & 0xFFFF) | (compare << 16) } else { (cc & 0xFFFF_0000) | compare };
        let live = csr & CSR_EN != 0
            && read_volatile(reg(t, DIV)) == p << DIV_INT_SHIFT
            && read_volatile(reg(t, TOP)) == a.wrapping_sub(1);
        let r = addr_of_mut!(RECORD);
        if live {
            write_volatile(reg(t, CC), cc_new);
            write_volatile(reg(t, CSR), csr_new);
            bump(addr_of_mut!((*r).live));
        } else {
            write_volatile(reg(t, CSR), csr_new & !CSR_EN);
            write_volatile(reg(t, DIV), p << DIV_INT_SHIFT);
            write_volatile(reg(t, TOP), a.wrapping_sub(1));
            write_volatile(reg(t, CC), cc_new);
            write_volatile(reg(t, CTR), 0);
            write_volatile(reg(t, CSR), csr_new | CSR_EN);
            bump(addr_of_mut!((*r).restarts));
        }
        if DRIVEN[index].load(Ordering::Acquire) == 0 {
            write_volatile(ctrl(line), FUNCSEL_PWM | parked(polarity));
        }
    });
}

/// The output driven (the override normal) or parked at its idle level.
pub fn enable(index: usize, on: bool) {
    let line = facts::PWM_LINES[index].line as u32;
    let polarity = POL[index].load(Ordering::Acquire);
    DRIVEN[index].store(on as u32, Ordering::Release);
    with_cs(|| unsafe { write_volatile(ctrl(line), FUNCSEL_PWM | if on { 0 } else { parked(polarity) }) });
}

pub fn park(index: usize) {
    enable(index, false);
}

/// The period the slice runs as its registers state it: DIV.INT times
/// TOP + 1 ticks of the system clock, in nanoseconds (the api flavor's tie
/// of the door's answer to the block).
#[allow(dead_code)]
pub fn probe_period_ns(timer: usize) -> u64 {
    let (d, top) = unsafe {
        ((read_volatile(reg(timer, DIV)) & DIV_INT_MASK) >> DIV_INT_SHIFT, read_volatile(reg(timer, TOP)) & 0xFFFF)
    };
    pwm_math::achieved_ns(d, top + 1, facts::PWM_TIMERS[timer].clock_hz)
}
