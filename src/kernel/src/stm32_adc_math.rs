//! The ST converters' arithmetic, facts shapes and register sequences,
//! pure: the two converter blocks of the rows that read their analog pins
//! through the chip's own converters -- the STM32F7's (RM0410 chapter 15)
//! and the STM32U5's 14-bit one (RM0456 chapter 33) -- written over a
//! two-verb register seam, a read and a write at an address, so that the
//! host tests hold every sequence no emulator row models; a board's backend
//! makes the two verbs volatile accesses and adds the pads and the record.
//! Also here: the rule for one conversion a block at a time (`Turns`), the
//! limits the chips' datasheets state for the converters' clocks and for
//! reading the internal reference, and the band a reading must fall in.
//! No hardware access; the only state is the turns' own words. Covered by
//! the host tests.
#![allow(dead_code)] // the host tier compiles the module whole; a row reads its own chip's half

use core::sync::atomic::{AtomicU32, AtomicU8, Ordering};

/// The register seam: a 32-bit read and a 32-bit write at an address.
pub trait Regs {
    fn rd(&mut self, addr: usize) -> u32;
    fn wr(&mut self, addr: usize, value: u32);
}

/// A converter block as a board's facts state it for the backend: the
/// block's base and the clock enable of its bus (register and bit). The
/// table position is the block table's.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AdcUnit {
    pub base: usize,
    pub enable_reg: usize,
    pub enable_bit: u32,
}

/// The unit table against the block and channel tables: one unit a block,
/// each with a word-aligned base and enable register and an enable bit
/// inside the register, no two units of one base, and every channel's block
/// inside the table.
pub const fn units_fit(units: &[AdcUnit], blocks: usize, channels: &[crate::adc_math::AdcChannel]) -> bool {
    if units.len() != blocks {
        return false;
    }
    let mut i = 0;
    while i < units.len() {
        let u = &units[i];
        if u.base == 0 || u.base & 3 != 0 || u.enable_reg == 0 || u.enable_reg & 3 != 0 || u.enable_bit > 31 {
            return false;
        }
        let mut j = i + 1;
        while j < units.len() {
            if units[j].base == u.base {
                return false;
            }
            j += 1;
        }
        i += 1;
    }
    i = 0;
    while i < channels.len() {
        if channels[i].block as usize >= units.len() {
            return false;
        }
        i += 1;
    }
    true
}

/// A list of table positions (the flight check's own list of the channels
/// it may claim): every position inside the table and none twice.
pub const fn positions_fit(list: &[usize], channels: usize) -> bool {
    let mut i = 0;
    while i < list.len() {
        if list[i] >= channels {
            return false;
        }
        let mut j = i + 1;
        while j < list.len() {
            if list[j] == list[i] {
                return false;
            }
            j += 1;
        }
        i += 1;
    }
    true
}

/// Whether a converter's clock lies inside the range its datasheet states.
pub const fn clock_ok(hz: u32, min_hz: u32, max_hz: u32) -> bool {
    hz >= min_hz && hz <= max_hz
}

/// Whether a sampling time of `cycles` of a clock lasts at least `min_ns`
/// (the datasheets' minimum for reading the internal reference).
pub const fn sampling_ok(cycles: u32, hz: u32, min_ns: u64) -> bool {
    hz != 0 && cycles as u64 * 1_000_000_000 / hz as u64 >= min_ns
}

/// The band a reading of a voltage between `low_uv` and `high_uv`
/// microvolts falls in at a reference of `reference_mv` and a width of
/// `bits`: the two ends converted (the low end rounded down, the high end
/// up), the converter's total unadjusted error `error` allowed on each
/// side, kept inside 0 and the full scale.
pub const fn band(low_uv: u32, high_uv: u32, reference_mv: u32, bits: u32, error: u32) -> (u32, u32) {
    let full = ((1u64 << bits) - 1) as u64;
    let r = reference_mv as u64 * 1000;
    let lo = low_uv as u64 * full / r;
    let hi = (high_uv as u64 * full).div_ceil(r) + error as u64;
    ((if lo > error as u64 { lo - error as u64 } else { 0 }) as u32, (if hi > full { full } else { hi }) as u32)
}

/// No channel: a block's turn that nobody holds.
pub const NONE: u32 = u32::MAX;

/// One conversion a block at a time. A converter block converts one channel
/// and has one data register, so a channel takes its block for the length
/// of its conversion; a second channel of the same block queues and takes
/// the block when it is free (its read's wait polls `promote`). A block
/// reset clears the block's turn and the queue of its channels.
pub struct Turns<const NB: usize, const NC: usize> {
    owner: [AtomicU32; NB],
    queued: [AtomicU8; NC],
}

impl<const NB: usize, const NC: usize> Turns<NB, NC> {
    pub const fn new() -> Self {
        Self { owner: [const { AtomicU32::new(NONE) }; NB], queued: [const { AtomicU8::new(0) }; NC] }
    }

    /// The channel asks for its block: true when it took it (its conversion
    /// starts now), false when it queued behind the conversion in flight.
    pub fn start(&self, block: usize, channel: usize) -> bool {
        if self.owner[block].compare_exchange(NONE, channel as u32, Ordering::AcqRel, Ordering::Acquire).is_ok() {
            true
        } else {
            self.queued[channel].store(1, Ordering::Release);
            false
        }
    }

    /// Whether the channel holds its block.
    pub fn owns(&self, block: usize, channel: usize) -> bool {
        self.owner[block].load(Ordering::Acquire) == channel as u32
    }

    /// A queued channel takes its block once it is free: true when it did
    /// (its conversion starts now). A channel that never queued takes nothing.
    pub fn promote(&self, block: usize, channel: usize) -> bool {
        if self.queued[channel].load(Ordering::Acquire) != 0 && self.owner[block].compare_exchange(NONE, channel as u32, Ordering::AcqRel, Ordering::Acquire).is_ok() {
            self.queued[channel].store(0, Ordering::Release);
            true
        } else {
            false
        }
    }

    /// The block given back by the channel that holds it; nothing moves
    /// when another channel does.
    pub fn give(&self, block: usize, channel: usize) {
        let _ = self.owner[block].compare_exchange(channel as u32, NONE, Ordering::AcqRel, Ordering::Acquire);
    }

    /// A released channel: out of the queue, and its block given back.
    pub fn forget(&self, block: usize, channel: usize) {
        self.queued[channel].store(0, Ordering::Release);
        self.give(block, channel);
    }

    /// A block reset: every queued channel `of_block` names dropped, the
    /// block's turn cleared.
    pub fn clear(&self, block: usize, of_block: impl Fn(usize) -> bool) {
        let mut i = 0;
        while i < NC {
            if of_block(i) {
                self.queued[i].store(0, Ordering::Release);
            }
            i += 1;
        }
        self.owner[block].store(NONE, Ordering::Release);
    }
}

/// The STM32F7's converter (RM0410 chapter 15). One register file a block
/// (15.13): the status register at 0x00, the two control registers at 0x04
/// and 0x08, the two sample time registers at 0x0C and 0x10, the first
/// regular sequence register at 0x2C, the third at 0x34 and the data
/// register at 0x4C; the common control register stands at 0x04 of the
/// common registers. The converters' clock is the APB2 clock over the
/// common prescaler (15.3.3); a conversion takes its sampling time plus 12
/// cycles (15.5).
pub mod f7 {
    use super::{AdcUnit, Regs};

    /// ADC_SR at 0x00: EOC bit 1 (the conversion's end; cleared by a read of
    /// the data register), STRT bit 4, OVR bit 5 (data lost; cleared by
    /// writing 0). Every flag of the register clears on a 0 written and
    /// keeps on a 1.
    pub const SR: usize = 0x00;
    pub const SR_EOC: u32 = 1 << 1;
    pub const SR_OVR: u32 = 1 << 5;
    /// ADC_CR1 at 0x04: the resolution field at bits 25:24, 00 for 12 bits;
    /// written 0 whole -- no scan, no interrupt, no watchdog.
    pub const CR1: usize = 0x04;
    /// ADC_CR2 at 0x08: ADON bit 0 (the converter on), EOCS bit 10 (the end
    /// flag after each conversion, which turns the overrun detection on
    /// without DMA: 15.8.2), SWSTART bit 30 (a conversion started).
    pub const CR2: usize = 0x08;
    pub const CR2_ADON: u32 = 1 << 0;
    pub const CR2_EOCS: u32 = 1 << 10;
    pub const CR2_SWSTART: u32 = 1 << 30;
    /// ADC_SMPR1 at 0x0C (channels 10 to 18, three bits each in 26:0) and
    /// ADC_SMPR2 at 0x10 (channels 0 to 9, in 29:0): every field at 111, the
    /// longest sampling time, 480 cycles.
    pub const SMPR1: usize = 0x0C;
    pub const SMPR2: usize = 0x10;
    pub const SMPR1_ALL_480: u32 = 0x07FF_FFFF;
    pub const SMPR2_ALL_480: u32 = 0x3FFF_FFFF;
    /// ADC_SQR1 at 0x2C: the sequence length in 23:20, 0 for one conversion.
    pub const SQR1: usize = 0x2C;
    /// ADC_SQR3 at 0x34: the sequence's first conversion in 4:0.
    pub const SQR3: usize = 0x34;
    /// ADC_DR at 0x4C: the sample in 15:0, right-aligned.
    pub const DR: usize = 0x4C;
    /// ADC_CCR at 0x04 of the common registers: the prescaler ADCPRE at
    /// 17:16 (01: the APB2 clock over 4), TSVREFE bit 23 (the internal
    /// reference and the temperature sensor enabled).
    pub const CCR: usize = 0x04;
    pub const CCR_ADCPRE: u32 = 0b11 << 16;
    pub const CCR_ADCPRE_4: u32 = 0b01 << 16;
    pub const CCR_TSVREFE: u32 = 1 << 23;

    /// The prescaler taken, the sampling time, a conversion's cycles and the
    /// sample's width.
    pub const PRESCALER: u32 = 4;
    pub const SAMPLE_CYCLES: u32 = 480;
    pub const CONVERSION_CYCLES: u32 = SAMPLE_CYCLES + 12;
    pub const BITS: u32 = 12;
    /// The datasheet's limits (DS11532 Table 72: the converter's clock from
    /// 0.6 to 36 MHz at an analog supply of 2.4 to 3.6 V; Table 81: the
    /// internal reference sampled for at least 10 microseconds).
    pub const CLOCK_MIN_HZ: u32 = 600_000;
    pub const CLOCK_MAX_HZ: u32 = 36_000_000;
    pub const REFERENCE_SAMPLE_MIN_NS: u64 = 10_000;
    /// The wait after a converter is switched on, in nanoseconds: its
    /// power-up takes 3 microseconds at most (Table 72) and the internal
    /// reference's start 10 at most (Table 81).
    pub const SETTLE_NS: u64 = 10_000;

    /// A block brought up: its bus clock enabled and read back; the common
    /// control register given the prescaler and, when the row reads the
    /// internal reference, its enable -- written only when it differs, so a
    /// second block's bring-up does not write under the first block's
    /// conversion; the first control register 0; every sampling time at its
    /// longest; a sequence of one; the converter on with the end flag after
    /// each conversion. The caller waits `SETTLE_NS` after it.
    pub fn bring_up<R: Regs>(r: &mut R, u: &AdcUnit, common: usize, reference: bool) {
        let en = r.rd(u.enable_reg);
        r.wr(u.enable_reg, en | (1 << u.enable_bit));
        let _ = r.rd(u.enable_reg);
        let ccr = r.rd(common + CCR);
        let want = (ccr & !CCR_ADCPRE) | CCR_ADCPRE_4 | if reference { CCR_TSVREFE } else { 0 };
        if want != ccr {
            r.wr(common + CCR, want);
        }
        r.wr(u.base + CR1, 0);
        r.wr(u.base + SMPR1, SMPR1_ALL_480);
        r.wr(u.base + SMPR2, SMPR2_ALL_480);
        r.wr(u.base + SQR1, 0);
        r.wr(u.base + CR2, CR2_ADON | CR2_EOCS);
    }

    /// Whether the block is on.
    pub fn is_on<R: Regs>(r: &mut R, base: usize) -> bool {
        r.rd(base + CR2) & CR2_ADON != 0
    }

    /// One conversion started: the channel the sequence's first, every
    /// status flag cleared, the start bit. Nothing is written to a block
    /// that is not on.
    pub fn kick<R: Regs>(r: &mut R, base: usize, channel: u32) {
        if !is_on(r, base) {
            return;
        }
        r.wr(base + SQR3, channel & 0x1F);
        r.wr(base + SR, 0);
        r.wr(base + CR2, CR2_ADON | CR2_EOCS | CR2_SWSTART);
    }

    /// The conversion's end.
    pub fn done<R: Regs>(r: &mut R, base: usize) -> bool {
        r.rd(base + SR) & SR_EOC != 0
    }

    /// The sample: the status, then the data register (whose read clears
    /// the end flag). An overrun flagged answers no sample; its flag alone
    /// is cleared, after which the manual's recovery is a new start.
    pub fn take<R: Regs>(r: &mut R, base: usize) -> Option<u32> {
        let sr = r.rd(base + SR);
        let dr = r.rd(base + DR);
        if sr & SR_OVR != 0 {
            r.wr(base + SR, !SR_OVR);
            return None;
        }
        Some(dr & ((1 << BITS) - 1))
    }

    /// A block reset, first half: the converter off and its flags cleared;
    /// `on` and the settle follow.
    pub fn off<R: Regs>(r: &mut R, base: usize) {
        r.wr(base + CR2, 0);
        r.wr(base + SR, 0);
    }

    pub fn on<R: Regs>(r: &mut R, base: usize) {
        r.wr(base + CR2, CR2_ADON | CR2_EOCS);
    }
}

/// The STM32U5's 14-bit converter (RM0456 chapter 33). One register file a
/// block (33.6): the interrupt and status register at 0x00, the control
/// register at 0x08, the first configuration register at 0x0C, the two
/// sample time registers at 0x14 and 0x18, the channel preselection
/// register at 0x1C, the first regular sequence register at 0x30 and the
/// data register at 0x40; the common control register stands at 0x08 of
/// the common registers. The converter has a power-up of its own (33.4.6,
/// 33.4.8, 33.4.9) and rules for when a register may be written (33.4.10),
/// which the sequences below follow. Its clock is a kernel clock the reset
/// and clock controller selects; a conversion takes its sampling time plus
/// 17 cycles at 14 bits (33.4.13).
pub mod u5 {
    use super::{AdcUnit, Regs};

    /// ADC_ISR at 0x00, every flag cleared by writing 1: ADRDY bit 0 (the
    /// converter ready), EOC bit 2 (also cleared by a read of the data
    /// register), EOS bit 3, OVR bit 4 (a conversion ended with the end flag
    /// still set), LDORDY bit 12 (the regulator ready; read-only).
    pub const ISR: usize = 0x00;
    pub const ISR_ADRDY: u32 = 1 << 0;
    pub const ISR_EOC: u32 = 1 << 2;
    pub const ISR_EOS: u32 = 1 << 3;
    pub const ISR_OVR: u32 = 1 << 4;
    pub const ISR_LDORDY: u32 = 1 << 12;
    /// ADC_CR at 0x08. Commands, each acting on a 1 written and cleared by
    /// the chip: ADEN bit 0, ADDIS bit 1, ADSTART bit 2, JADSTART bit 3,
    /// ADSTP bit 4, JADSTP bit 5, ADCAL bit 31. Plain bits: ADCALLIN bit 16
    /// (the calibration includes the linearity), ADVREGEN bit 28 (the
    /// regulator), DEEPPWD bit 29 (deep power-down, set at reset). A write
    /// keeps the plain bits and carries no command but the one it means.
    pub const CR: usize = 0x08;
    pub const CR_ADEN: u32 = 1 << 0;
    pub const CR_ADDIS: u32 = 1 << 1;
    pub const CR_ADSTART: u32 = 1 << 2;
    pub const CR_ADSTP: u32 = 1 << 4;
    pub const CR_ADCALLIN: u32 = 1 << 16;
    pub const CR_ADVREGEN: u32 = 1 << 28;
    pub const CR_DEEPPWD: u32 = 1 << 29;
    pub const CR_ADCAL: u32 = 1 << 31;
    pub const CR_COMMANDS: u32 = CR_ADEN | CR_ADDIS | CR_ADSTART | (1 << 3) | CR_ADSTP | (1 << 5) | CR_ADCAL;
    /// ADC_CFGR1 at 0x0C: the data management DMNGT at 1:0 (00: the data
    /// register alone), the resolution RES at 3:2 (00: 14 bits), the
    /// trigger EXTEN at 11:10 (00: software), OVRMOD bit 12 (0: an overrun
    /// keeps the old data), CONT bit 13 (0: one conversion).
    pub const CFGR1: usize = 0x0C;
    pub const CFGR1_SINGLE_14: u32 = 0b11 | (0b11 << 2) | (0b11 << 10) | (1 << 12) | (1 << 13);
    /// ADC_SMPR1 at 0x14 (channels 0 to 9) and ADC_SMPR2 at 0x18 (channels
    /// 10 to 19), three bits a channel in 29:0: every field at 111, the
    /// longest sampling time, 814 cycles.
    pub const SMPR1: usize = 0x14;
    pub const SMPR2: usize = 0x18;
    pub const SMPR_ALL_814: u32 = 0x3FFF_FFFF;
    /// ADC_PCSEL at 0x1C: a bit a channel, set before the channel is
    /// converted; written only while no conversion runs.
    pub const PCSEL: usize = 0x1C;
    /// ADC_SQR1 at 0x30: the sequence length in 3:0 (0: one conversion) and
    /// the sequence's first conversion in 10:6.
    pub const SQR1: usize = 0x30;
    /// ADC_DR at 0x40: the sample, right-aligned.
    pub const DR: usize = 0x40;
    /// ADC12_CCR at 0x08 of the common registers, written only while the
    /// converters are disabled: the dual mode DUAL at 4:0 (0: independent),
    /// the prescaler PRESC at 21:18 (0: the kernel clock undivided), VREFEN
    /// bit 22 (the internal reference's buffer).
    pub const CCR: usize = 0x08;
    pub const CCR_DUAL: u32 = 0x1F;
    pub const CCR_PRESC: u32 = 0xF << 18;
    pub const CCR_VREFEN: u32 = 1 << 22;

    /// The sampling time, a conversion's cycles and the sample's width.
    pub const SAMPLE_CYCLES: u32 = 814;
    pub const CONVERSION_CYCLES: u32 = SAMPLE_CYCLES + 17;
    pub const BITS: u32 = 14;
    /// The datasheet's limits (DS13543 Table 97: the converter's clock from
    /// 5 to 55 MHz; Table 37: the internal reference sampled for at least
    /// 12.65 microseconds).
    pub const CLOCK_MIN_HZ: u32 = 5_000_000;
    pub const CLOCK_MAX_HZ: u32 = 55_000_000;
    pub const REFERENCE_SAMPLE_MIN_NS: u64 = 12_650;

    /// The converter as a board's facts state it: its unit, the common
    /// registers' base, and the kernel clock selector -- its register, the
    /// field's shift and mask and the code written (the source the row's
    /// clock configurations keep running).
    #[derive(Clone, Copy, Debug)]
    pub struct Facts {
        pub unit: AdcUnit,
        pub common: usize,
        pub clock_sel_reg: usize,
        pub clock_sel_shift: u32,
        pub clock_sel_mask: u32,
        pub clock_sel_code: u32,
    }

    pub const fn facts_sound(f: &Facts) -> bool {
        f.common != 0 && f.common & 3 == 0 && f.clock_sel_reg != 0 && f.clock_sel_reg & 3 == 0 && f.clock_sel_shift < 32 && f.clock_sel_mask != 0 && f.clock_sel_code & !f.clock_sel_mask == 0
    }

    /// The stages of the power-up; the record's word names the stage that
    /// missed its bound, or `UP`.
    pub const STAGE_UP: u32 = 0;
    pub const STAGE_REGULATOR: u32 = 1;
    pub const STAGE_CALIBRATION: u32 = 2;
    pub const STAGE_ENABLE: u32 = 3;

    /// What the power-up found: the stage it ended on and the reads each
    /// polled stage took (the bound where it missed).
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct Outcome {
        pub stage: u32,
        pub regulator_polls: u32,
        pub calibration_polls: u32,
        pub enables: u32,
    }

    /// The converters' clocks, before the power-up: their bus clock enabled
    /// and read back and their kernel clock selected (the reset and clock
    /// controller's bits are written while the converter is disabled). Two
    /// short read-modify-writes of registers other drivers write too: the
    /// backend runs them under the kernel's critical section.
    pub fn clocks<R: Regs>(r: &mut R, f: &Facts) {
        let en = r.rd(f.unit.enable_reg);
        r.wr(f.unit.enable_reg, en | (1 << f.unit.enable_bit));
        let _ = r.rd(f.unit.enable_reg);
        let sel = r.rd(f.clock_sel_reg);
        r.wr(f.clock_sel_reg, (sel & !(f.clock_sel_mask << f.clock_sel_shift)) | (f.clock_sel_code << f.clock_sel_shift));
    }

    /// The converter's power-up, in the manual's order, after `clocks`:
    /// deep power-down left, then the regulator enabled, two
    /// writes, and its ready flag polled; the common control register --
    /// independent converters, no prescaler, the internal reference's
    /// buffer when the row reads it; the calibration with the linearity,
    /// the linearity bit first and the calibration command after it, polled
    /// until the chip clears the command; the ready flag cleared and the
    /// enable written until the flag answers (it does not take while the
    /// calibration's last cycles run); then, the converter enabled, one
    /// conversion of 14 bits by software, every sampling time at its
    /// longest, and every channel of `preselect` preselected -- no
    /// conversion can run yet, which is when that register may be written.
    /// A stage that misses `bound` reads ends the sequence there: nothing
    /// after it is written.
    pub fn power_up<R: Regs>(r: &mut R, f: &Facts, reference: bool, preselect: u32, bound: u32) -> Outcome {
        let base = f.unit.base;
        let mut o = Outcome { stage: STAGE_REGULATOR, regulator_polls: 0, calibration_polls: 0, enables: 0 };
        let cr = r.rd(base + CR) & !(CR_COMMANDS | CR_DEEPPWD);
        r.wr(base + CR, cr);
        r.wr(base + CR, cr | CR_ADVREGEN);
        loop {
            if o.regulator_polls == bound {
                return o;
            }
            o.regulator_polls += 1;
            if r.rd(base + ISR) & ISR_LDORDY != 0 {
                break;
            }
        }
        o.stage = STAGE_CALIBRATION;
        let ccr = r.rd(f.common + CCR);
        r.wr(f.common + CCR, (ccr & !(CCR_DUAL | CCR_PRESC | CCR_VREFEN)) | if reference { CCR_VREFEN } else { 0 });
        let cr = (r.rd(base + CR) & !CR_COMMANDS) | CR_ADCALLIN;
        r.wr(base + CR, cr);
        r.wr(base + CR, cr | CR_ADCAL);
        loop {
            if o.calibration_polls == bound {
                return o;
            }
            o.calibration_polls += 1;
            if r.rd(base + CR) & CR_ADCAL == 0 {
                break;
            }
        }
        o.stage = STAGE_ENABLE;
        r.wr(base + ISR, ISR_ADRDY);
        loop {
            if o.enables == bound {
                return o;
            }
            o.enables += 1;
            let cr = r.rd(base + CR) & !CR_COMMANDS;
            r.wr(base + CR, cr | CR_ADEN);
            if r.rd(base + ISR) & ISR_ADRDY != 0 {
                break;
            }
        }
        o.stage = STAGE_UP;
        let cfgr = r.rd(base + CFGR1);
        r.wr(base + CFGR1, cfgr & !CFGR1_SINGLE_14);
        r.wr(base + SMPR1, SMPR_ALL_814);
        r.wr(base + SMPR2, SMPR_ALL_814);
        r.wr(base + PCSEL, preselect & 0x000F_FFFF);
        o
    }

    /// Whether the converter is enabled and no disable is pending: the
    /// state in which a conversion may be started.
    pub fn is_up<R: Regs>(r: &mut R, base: usize) -> bool {
        r.rd(base + CR) & (CR_ADEN | CR_ADDIS) == CR_ADEN
    }

    /// One conversion started: the channel the sequence's one conversion,
    /// the end and overrun flags cleared, the start command. Nothing is
    /// written to a converter that is not up.
    pub fn kick<R: Regs>(r: &mut R, base: usize, channel: u32) {
        if !is_up(r, base) {
            return;
        }
        r.wr(base + SQR1, (channel & 0x1F) << 6);
        r.wr(base + ISR, ISR_EOC | ISR_EOS | ISR_OVR);
        let cr = r.rd(base + CR) & !CR_COMMANDS;
        r.wr(base + CR, cr | CR_ADSTART);
    }

    /// The conversion's end.
    pub fn done<R: Regs>(r: &mut R, base: usize) -> bool {
        r.rd(base + ISR) & ISR_EOC != 0
    }

    /// The sample: the status, then the data register (whose read clears
    /// the end flag). An overrun flagged answers no sample; its flag is
    /// cleared.
    pub fn take<R: Regs>(r: &mut R, base: usize) -> Option<u32> {
        let isr = r.rd(base + ISR);
        let dr = r.rd(base + DR);
        if isr & ISR_OVR != 0 {
            r.wr(base + ISR, ISR_OVR);
            return None;
        }
        Some(dr & ((1 << BITS) - 1))
    }

    /// The converter taken down as far as a new power-up needs: a running
    /// conversion stopped and its stop awaited, the converter disabled and
    /// its disable awaited, each under `bound` reads; with `deep`, the
    /// regulator off and deep power-down entered, which loses the
    /// calibration. True when the converter reads disabled.
    pub fn power_down<R: Regs>(r: &mut R, f: &Facts, deep: bool, bound: u32) -> bool {
        let base = f.unit.base;
        let cr = r.rd(base + CR);
        if cr & CR_ADEN != 0 {
            if cr & CR_ADSTART != 0 && cr & CR_ADDIS == 0 {
                r.wr(base + CR, (cr & !CR_COMMANDS) | CR_ADSTP);
                let mut n = 0;
                while n < bound && r.rd(base + CR) & CR_ADSTP != 0 {
                    n += 1;
                }
            }
            let cr = r.rd(base + CR);
            if cr & CR_ADDIS == 0 {
                r.wr(base + CR, (cr & !CR_COMMANDS) | CR_ADDIS);
            }
            let mut n = 0;
            while n < bound && r.rd(base + CR) & CR_ADEN != 0 {
                n += 1;
            }
        }
        let cr = r.rd(base + CR);
        if cr & (CR_ADEN | CR_ADDIS | CR_ADCAL) != 0 {
            return false;
        }
        if deep {
            r.wr(base + CR, cr & !(CR_COMMANDS | CR_ADVREGEN));
            r.wr(base + CR, (cr & !(CR_COMMANDS | CR_ADVREGEN)) | CR_DEEPPWD);
        }
        true
    }
}
