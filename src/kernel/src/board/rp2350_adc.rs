//! The RP2350's ADC backend: the chip's one successive-approximation
//! converter behind the doors, in the shape of the emulator rows' virtual
//! converter. The block (the RP2350 datasheet, chapter 12.4) at
//! 0x400a0000: CS (EN bit 0, TS_EN bit 1, START_ONCE bit 2, READY bit 8,
//! ERR bit 9, AINSEL 15:12; Table 1121) and RESULT (11:0; Table 1122). One
//! conversion: the input selected in AINSEL, START_ONCE written, READY low
//! for 96 cycles of the converter's clock and high after, the 12-bit
//! result in RESULT (12.4.3.1; switching the input needs no settling
//! time). The converter's clock is the crystal's 12 MHz on both clock
//! postures: the block's registers do not answer while that clock runs
//! faster than the system clock (a measured fact of the part, not in its
//! datasheet), and the crystal never does; the generator (CLK_ADC_CTRL, Table
//! 571) is switched with the block held in reset (8.1.3.2). An analog pad
//! has its input disabled and its output disabled -- the chapter's rule,
//! and erratum E9 makes an enabled input leak -- and no function. The
//! rows whose channels have a digital twin pad on their net park the twin
//! the same way at the claim when no program holds it. The temperature
//! sensor is the channel past the package's last input; its bias (TS_EN)
//! stays on for the block's life. One conversion at a time: the block has
//! one owner, and a second channel's start waits its turn inside the
//! door's budget. No interrupt, no FIFO, no free running; nothing masks.
//! The record `__rhu_rp2350_adc` is read by symbol.

use core::ptr::{addr_of, addr_of_mut, read_volatile, write_volatile};
use core::sync::atomic::{AtomicBool, AtomicU32, AtomicU8, Ordering};

use super::rp2350::gpio_backend::{ctrl, pad, sio, GPIO_OE_CLR, GPIO_OUT_CLR};
use super::rp2350::{
    release_reset, CLOCKS, PAD_IE, PAD_ISO, PAD_OD, PAD_PDE, PAD_PUE, RESETS_RESET, RESETS_RESET_DONE, RESET_IO_BANK0,
    RESET_PADS_BANK0, SYSCLK_HZ, XOSC_HZ,
};
use crate::adc_math;
use crate::board::facts;
use crate::claims;
use crate::critical::with_cs;
use crate::gpio;

/// The converter's clock: the crystal, on both postures.
pub const CLK_ADC_HZ: u32 = XOSC_HZ;
const _: () = assert!(CLK_ADC_HZ <= SYSCLK_HZ, "the converter's registers do not answer while its clock runs faster than the system clock");

/// The block's base address (Table 1120).
const ADC: usize = 0x400a_0000;
/// CS, the control and status register, at +0x00 (Table 1120).
const CS: *mut u32 = ADC as *mut u32;
/// RESULT, the most recent conversion, at +0x04 (Table 1120).
const RESULT: *mut u32 = (ADC + 0x04) as *mut u32;
/// CS EN, bit 0: the converter powered (Table 1121).
const CS_EN: u32 = 1 << 0;
/// CS TS_EN, bit 1: the temperature sensor's bias on (Table 1121).
const CS_TS_EN: u32 = 1 << 1;
/// CS START_ONCE, bit 2: one conversion, self-clearing (Table 1121).
const CS_START_ONCE: u32 = 1 << 2;
/// CS READY, bit 8: high once the conversion is done (Table 1121).
const CS_READY: u32 = 1 << 8;
/// CS ERR, bit 9: the most recent conversion had an error (Table 1121).
const CS_ERR: u32 = 1 << 9;
/// CS AINSEL, bits 15:12: the input selected (Table 1121).
const CS_AINSEL_SHIFT: u32 = 12;
/// RESULT, bits 11:0 (Table 1122).
const RESULT_MASK: u32 = 0xFFF;
/// RESETS.RESET ADC, bit 0 (Table 535).
const RESET_ADC: u32 = 1 << 0;
/// The converter's clock generator (Tables 571 and 572): ENABLE bit 11,
/// AUXSRC 7:5 (3 = the crystal), ENABLED bit 28; the divider's integer part
/// in 19:16.
/// CLK_ADC_CTRL at CLOCKS + 0x6c (Table 571).
const CLK_ADC_CTRL: *mut u32 = (CLOCKS + 0x6C) as *mut u32;
/// CLK_ADC_DIV at CLOCKS + 0x70 (Table 572).
const CLK_ADC_DIV: *mut u32 = (CLOCKS + 0x70) as *mut u32;
/// CLK_ADC_CTRL ENABLE, bit 11: the generator started (Table 571).
const CLK_ADC_ENABLE: u32 = 1 << 11;
/// CLK_ADC_CTRL ENABLED, bit 28, read-only: the generator running (Table 571).
const CLK_ADC_ENABLED: u32 = 1 << 28;
/// CLK_ADC_CTRL AUXSRC, bits 7:5: the source selected (Table 571).
const CLK_ADC_AUXSRC: u32 = 0b111 << 5;
/// CLK_ADC_CTRL AUXSRC at 3: the crystal (Table 571).
const CLK_ADC_AUXSRC_XOSC: u32 = 0b011 << 5;
/// CLK_ADC_DIV INT, bits 19:16, at 1: no division (Table 572).
const CLK_ADC_DIV_1: u32 = 1 << 16;
/// The clock gates' atomic set alias (section 2.1.3; WAKE_EN0 at CLOCKS + 0xac):
/// the converter's own clock and its bus clock (Table 587, bits 2 and 3).
const WAKE_EN0_SET: *mut u32 = (CLOCKS + 0x2000 + 0xAC) as *mut u32;
/// WAKE_EN0 CLK_ADC_ADC, bit 2, and CLK_SYS_ADC, bit 3: the block's two clock gates (Table 587).
const WAKE_EN0_ADC: u32 = (1 << 2) | (1 << 3);
/// GPIOn_CTRL FUNCSEL, bits 4:0, at 0x1f: no function (Table 651).
const FUNCSEL_NULL: u32 = 0x1F;
/// The handshakes' bound (at least a hundred microseconds of the posture's
/// clock; the block's power-up takes a few converter clocks).
const SPIN_SHORT: u32 = SYSCLK_HZ / 10_000 / 4;
const NONE: u32 = u32::MAX;
const NC: usize = facts::ADC_CHANNELS.len();

/// Which channel owns the block (its conversion in flight), and the
/// channels waiting their turn.
static OWNER: AtomicU32 = AtomicU32::new(NONE);
static QUEUED: [AtomicU8; NC] = [const { AtomicU8::new(0) }; NC];
static UP: AtomicBool = AtomicBool::new(false);

/// The chip's record, by symbol: the bring-up's verdict (1 = the clock
/// enabled, the block reset, released and ready; 2 = a handshake missed its
/// bound), the conversions, the conversions the block flagged (CS.ERR), the
/// starts that waited for the block, the block resets, and the clock
/// control word read back at the bring-up.
#[repr(C)]
pub struct AdcRecord {
    pub up: u32,
    pub conversions: u32,
    pub flagged: u32,
    pub waited: u32,
    pub resets: u32,
    pub clock_ctrl: u32,
}

#[unsafe(export_name = "__rhu_rp2350_adc")]
pub static mut RECORD: AdcRecord = AdcRecord { up: 0, conversions: 0, flagged: 0, waited: 0, resets: 0, clock_ctrl: 0 };

unsafe fn bump(p: *mut u32) {
    unsafe { write_volatile(p, read_volatile(p).wrapping_add(1)) }
}

/// The block enabled with the sensor's bias, READY polled under the bound.
unsafe fn power_up() -> bool {
    unsafe {
        write_volatile(CS, CS_EN | CS_TS_EN);
        let mut n = 0u32;
        while read_volatile(CS) & CS_READY == 0 {
            n += 1;
            if n > SPIN_SHORT {
                return false;
            }
        }
        true
    }
}

/// The block up once per boot: the gates set, the block put into reset, the
/// generator stopped, sourced from the crystal, divided by one, started and
/// polled (the block in reset meanwhile), the block and the pad blocks
/// released (the done bits polled), the block powered up and READY polled.
fn bring_up() {
    if UP.swap(true, Ordering::AcqRel) {
        return;
    }
    let (ok, clk) = unsafe {
        write_volatile(WAKE_EN0_SET, WAKE_EN0_ADC);
        write_volatile(RESETS_RESET, read_volatile(RESETS_RESET) | RESET_ADC);
        let mut ok = true;
        let mut n = 0u32;
        while read_volatile(RESETS_RESET_DONE) & RESET_ADC != 0 {
            n += 1;
            if n > SPIN_SHORT {
                ok = false;
                break;
            }
        }
        let c = read_volatile(CLK_ADC_CTRL) & !(CLK_ADC_ENABLE | CLK_ADC_AUXSRC);
        write_volatile(CLK_ADC_CTRL, c);
        write_volatile(CLK_ADC_DIV, CLK_ADC_DIV_1);
        write_volatile(CLK_ADC_CTRL, c | CLK_ADC_AUXSRC_XOSC);
        write_volatile(CLK_ADC_CTRL, c | CLK_ADC_AUXSRC_XOSC | CLK_ADC_ENABLE);
        n = 0;
        while read_volatile(CLK_ADC_CTRL) & CLK_ADC_ENABLED == 0 {
            n += 1;
            if n > SPIN_SHORT {
                ok = false;
                break;
            }
        }
        release_reset(RESET_ADC | RESET_IO_BANK0 | RESET_PADS_BANK0);
        let ok = ok && power_up();
        (ok, read_volatile(CLK_ADC_CTRL))
    };
    with_cs(|| unsafe {
        let r = addr_of_mut!(RECORD);
        write_volatile(addr_of_mut!((*r).up), if ok { 1 } else { 2 });
        write_volatile(addr_of_mut!((*r).clock_ctrl), clk);
    });
}

/// A pad parked for analog use: the SIO output and enable cleared, no
/// function, the pad written with its isolation latch set and released
/// after (section 9.7) -- the output disabled, the input disabled, no pull.
fn park(line: u32) {
    with_cs(|| unsafe {
        let (oe_clr, mask) = sio(GPIO_OE_CLR, line);
        write_volatile(oe_clr, mask);
        let (out_clr, _) = sio(GPIO_OUT_CLR, line);
        write_volatile(out_clr, mask);
        write_volatile(ctrl(line), FUNCSEL_NULL);
        let p = pad(line);
        let v = (read_volatile(p) & !(PAD_IE | PAD_PUE | PAD_PDE | PAD_ISO)) | PAD_OD;
        write_volatile(p, v | PAD_ISO);
        write_volatile(p, v);
    });
}

/// The twin pad on a channel's net, as the row's facts state it.
fn twin_of(line: u16) -> Option<u32> {
    let mut i = 0;
    while i < facts::ADC_TWINS.len() {
        let (l, twin) = facts::ADC_TWINS[i];
        if l == line {
            return Some(twin as u32);
        }
        i += 1;
    }
    None
}

/// The channel pad on a line's net, the converse of `twin_of`: the twin rule's
/// lookup for the pins backend.
fn channel_of(line: u32) -> Option<u32> {
    let mut i = 0;
    while i < facts::ADC_TWINS.len() {
        let (l, twin) = facts::ADC_TWINS[i];
        if twin as u32 == line {
            return Some(l as u32);
        }
        i += 1;
    }
    None
}

/// A line opened as a pin: the channel pad on its net parked unless a
/// program holds the channel's line (the twin rule, the mirror of `configure`'s twin
/// parking); nothing on a row that names no twin.
pub fn park_twin_channel(line: u32) {
    if let Some(channel) = channel_of(line) {
        if !held(channel) {
            park(channel);
        }
    }
}

/// Whether a program holds a line (a twin is parked only when free).
fn held(line: u32) -> bool {
    with_cs(|| unsafe { claims::is_program(&*addr_of!(gpio::CLAIMS), line) })
}

/// The channel's pad parked, and its twin's when the row names one and no
/// program holds it; an internal channel has no pad.
pub fn configure(index: usize) {
    bring_up();
    let c = &facts::ADC_CHANNELS[index];
    if c.line == adc_math::NO_LINE {
        return;
    }
    park(c.line as u32);
    if let Some(twin) = twin_of(c.line) {
        if !held(twin) {
            park(twin);
        }
    }
}

/// The pad and its free twin stay analog (high impedance; the leakage of
/// erratum E9 is an enabled input's); the block's owner and queue forget
/// the channel.
pub fn release(index: usize) {
    QUEUED[index].store(0, Ordering::Release);
    let _ = OWNER.compare_exchange(index as u32, NONE, Ordering::AcqRel, Ordering::Acquire);
}

/// One conversion started on the block: the input selected with the
/// sensor's bias kept, then the start bit -- the block's one control word,
/// two writes under the critical section.
fn kick(index: usize) {
    let ch = facts::ADC_CHANNELS[index].channel as u32;
    with_cs(|| unsafe {
        let base = CS_EN | CS_TS_EN | (ch << CS_AINSEL_SHIFT);
        write_volatile(CS, base);
        write_volatile(CS, base | CS_START_ONCE);
    });
}

/// The block taken and the conversion started, or the channel queued
/// behind the owner's conversion.
pub fn start(index: usize) {
    if OWNER.compare_exchange(NONE, index as u32, Ordering::AcqRel, Ordering::Acquire).is_ok() {
        kick(index);
    } else {
        QUEUED[index].store(1, Ordering::Release);
        with_cs(|| unsafe {
            let r = addr_of_mut!(RECORD);
            bump(addr_of_mut!((*r).waited));
        });
    }
}

/// The owner's conversion is done when READY is high; a queued channel
/// takes the block and starts once it is free (the door's wait polls this).
pub fn ready(index: usize) -> bool {
    if OWNER.load(Ordering::Acquire) == index as u32 {
        return unsafe { read_volatile(CS) } & CS_READY != 0;
    }
    if QUEUED[index].load(Ordering::Acquire) != 0
        && OWNER.compare_exchange(NONE, index as u32, Ordering::AcqRel, Ordering::Acquire).is_ok()
    {
        QUEUED[index].store(0, Ordering::Release);
        kick(index);
    }
    false
}

/// The result, the block released to the next channel; a conversion the
/// block flagged is counted in the record (the sample is still answered:
/// the flag means noisy, and the door's caller judges its readings).
pub fn sample(index: usize) -> u32 {
    let (cs, r) = unsafe { (read_volatile(CS), read_volatile(RESULT)) };
    with_cs(|| unsafe {
        let rec = addr_of_mut!(RECORD);
        bump(addr_of_mut!((*rec).conversions));
        if cs & CS_ERR != 0 {
            bump(addr_of_mut!((*rec).flagged));
        }
    });
    let _ = OWNER.compare_exchange(index as u32, NONE, Ordering::AcqRel, Ordering::Acquire);
    r & RESULT_MASK
}

/// The block reset after a conversion passed its budget: disabled, every
/// queued start dropped, the owner cleared, re-enabled and READY polled.
pub fn reset(_block: usize) {
    unsafe { write_volatile(CS, 0) };
    for q in QUEUED.iter() {
        q.store(0, Ordering::Release);
    }
    OWNER.store(NONE, Ordering::Release);
    let ok = unsafe { power_up() };
    with_cs(|| unsafe {
        let rec = addr_of_mut!(RECORD);
        bump(addr_of_mut!((*rec).resets));
        if !ok {
            write_volatile(addr_of_mut!((*rec).up), 2);
        }
    });
}
