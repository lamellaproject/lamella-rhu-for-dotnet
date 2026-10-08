//! The ADC doors' arithmetic (no hardware access) and facts shapes: the block and channel
//! descriptors a board's facts state, a sample's full scale from its
//! width, a conversion's time from the block's clock and the wait budget it
//! bounds, the loop channel's expected reading of a duty cycle (the mean a
//! resistor-capacitor network settles to) and the tables' soundness. No hardware
//! access and no state, covered by the host tests; the drivers cite the manuals for the numbers here.
#![allow(dead_code)]

/// A converter block as a board's facts state it: its NUMBER (the chip's
/// own: ADC1 is block 1, the number the channel door answers and the
/// reference door takes), its clock in Hz at this posture, the sample's
/// width in bits, its reference voltage in millivolts (a board fact) and
/// the cycles of its clock one conversion takes (the sampling time and the
/// conversion time together). The table position is the index the channel
/// table names.
#[derive(Clone, Copy, Debug)]
pub struct AdcBlock {
    pub number: u32,
    pub clock_hz: u32,
    pub bits: u32,
    pub reference_mv: u32,
    pub conversion_cycles: u32,
}

/// A channel: its block (the block table's index), the channel number on
/// that block (the chip's own), and the GPIO line it reads -- or
/// `NO_LINE` for an internal source (the reference, the temperature
/// sensor, the battery). The table position is the enumeration index the
/// count door and the channel door walk, and the handle a claim answers.
#[derive(Clone, Copy, Debug)]
pub struct AdcChannel {
    pub block: u8,
    pub channel: u8,
    pub line: u16,
}

/// An internal channel's line, as the table states it; the header states
/// RHU_ADC_NO_LINE for the same fact at the door.
pub const NO_LINE: u16 = u16::MAX;

/// The claims table's reach: four blocks of thirty-two channels (the
/// largest family in hand has three converters of at most twenty channels).
pub const MAX_BLOCKS: usize = 4;
pub const MAX_CHANNELS: usize = 32;
/// The widths served: one to sixteen bits (the sample is answered in a
/// signed 32-bit word beside the refusal codes).
pub const MAX_BITS: u32 = 16;

/// The largest sample a width can answer: 2^bits - 1.
pub const fn full_scale(bits: u32) -> u32 {
    (1u32 << bits) - 1
}

/// One conversion's time in nanoseconds, rounded up.
pub const fn conversion_ns(cycles: u32, clock_hz: u32) -> u64 {
    (cycles as u64 * 1_000_000_000).div_ceil(clock_hz as u64)
}

/// A conversion wait's budget in ticks: twice the conversion time in whole
/// ticks (rounded up) plus two, never fewer than three -- the bus doors'
/// wait shape, so a stuck converter fails within milliseconds.
pub const fn budget_ticks(conversion_ns: u64, tick_hz: u32) -> u32 {
    let wait = (conversion_ns * tick_hz as u64).div_ceil(1_000_000_000) as u32;
    let b = 2 * wait + 2;
    if b < 3 {
        3
    } else {
        b
    }
}

/// The loop channel's reading of a duty cycle: compare over period of the
/// full scale, rounded to nearest -- the mean voltage the network settles
/// to, as a fraction of the reference.
pub const fn loop_sample(compare: u32, period: u32, full: u32) -> u32 {
    ((compare as u64 * full as u64 + period as u64 / 2) / period as u64) as u32
}

/// Whether a sample lies within a band of a wanted value.
pub const fn within_band(sample: u32, want: u32, band: u32) -> bool {
    let d = if sample > want { sample - want } else { want - sample };
    d <= band
}

/// The table position of the block a number names, or None when the board
/// has no such block (the reference door then answers RHU_ENODEV).
pub fn block_index(table: &[AdcBlock], number: u32) -> Option<usize> {
    table.iter().position(|b| b.number == number)
}

/// The claims table's bit for a block's channel.
pub const fn channel_bit(block: usize, channel: usize) -> usize {
    block * MAX_CHANNELS + channel
}

/// The invariants a growing table can break, asserted at compile time where
/// the tables are consumed: at most four blocks, each with a clock, a width
/// from one to sixteen bits and at least one conversion cycle; every
/// channel's block inside the block table and its number inside the
/// bitmap's reach; no two entries for one channel of one block, and no two
/// entries reading one line.
pub const fn table_sound(channels: &[AdcChannel], blocks: &[AdcBlock]) -> bool {
    if blocks.len() > MAX_BLOCKS {
        return false;
    }
    let mut i = 0;
    while i < blocks.len() {
        let b = &blocks[i];
        if b.clock_hz == 0 || b.bits == 0 || b.bits > MAX_BITS || b.conversion_cycles == 0 {
            return false;
        }
        i += 1;
    }
    i = 0;
    while i < channels.len() {
        let c = &channels[i];
        if c.block as usize >= blocks.len() || c.channel as usize >= MAX_CHANNELS {
            return false;
        }
        let mut j = i + 1;
        while j < channels.len() {
            if (channels[j].block == c.block && channels[j].channel == c.channel) || (c.line != NO_LINE && channels[j].line == c.line) {
                return false;
            }
            j += 1;
        }
        i += 1;
    }
    true
}
