//! The stored network's reader. A network to join is stored in the
//! board's flash by the lamella tool, never by this kernel: two slots, one
//! 4 KB erase unit each, at the end of the flash device. The reader takes
//! the two slots as they lie in flash, decides which holds the current
//! record and hands its fields back as slices in place. It writes nothing,
//! erases nothing, copies nothing and never prints a name or a secret.
//!
//! THE FORMAT, every number little-endian. A slot begins with the first
//! record, 192 bytes: the magic `LWFR`; the format, 1; the security kinds
//! a join accepts, as bits; the name's length, 1 to 32; the secret's
//! length, 0 to 128; the sequence number; the name in 32 bytes; the secret
//! in 128; the reconnection kind; the boot setting; 14 reserved bytes; the
//! CRC-32 of the 188 bytes before it. Later records follow from offset 192
//! with no padding: a four-byte magic, a format byte, a two-byte length
//! that counts the whole record (11 or more), the body, and the CRC-32 of
//! everything before it. The list ends where the next magic would start
//! and reads four `FF` bytes, or at the unit's end.
//!
//! THE FRAME, kept by every format of the first record, this one and any
//! later one: the magic at offset 0, the format byte at 4, the sequence
//! number at 8 to 11, and at 188 the CRC-32 of the 188 bytes before it. A
//! later format changes only what lies between those fields.
//!
//! A COPY IS DAMAGED -- a save cut short -- when the frame's CRC-32 does
//! not match, whatever its format byte says, or when, in the format this
//! reader knows, a later record has a length under 11, reaches past the
//! unit or carries a CRC-32 that does not match, or the list does not end
//! as said. A damaged copy is no copy, and a slot without the magic holds
//! none. A later record of a kind this reader does not know is skipped by
//! its length.
//!
//! THE NEWEST UNDAMAGED COPY RULES, WHATEVER IT HOLDS. Of two undamaged
//! copies the one with the newer sequence number is current, and only then
//! is it judged: a format this reader does not know, or a value outside
//! the known set (the accepted kinds are open, WPA2, WPA3, or WPA2 and
//! WPA3 together, and an open kind never stands beside a secured one; the
//! name has 1 to 32 bytes and the secret at most 128; the boot setting is
//! at most 2 and the reconnection kind at most 1), means that no network
//! is stored for this kernel. The older copy is never taken in its place:
//! such a copy was stored by a later tool, and two boards must not end on
//! different networks because one of them fell back to an old copy. A
//! secret whose length does not fit its kind is no fault of the copy: the
//! join fails at the network.

/// One slot: one erase unit.
pub const UNIT: usize = 4096;
/// The first record's length, and so the offset of the later records.
const FIRST: usize = 192;
/// The bytes of the first record its CRC-32 covers.
const COVERED: usize = 188;
/// The shortest later record: its magic, format, length and CRC-32.
const LATER_MIN: usize = 11;
const MAGIC: [u8; 4] = *b"LWFR";
/// The sequence number's place, in every format.
const SEQUENCE_AT: usize = 8;
const NAME_AT: usize = 12;
const NAME_MAX: usize = 32;
const SECRET_AT: usize = 44;
const SECRET_MAX: usize = 128;

/// The security kinds a join accepts, as the record's bits.
pub const KIND_OPEN: u8 = 1;
pub const KIND_WPA2: u8 = 2;
pub const KIND_WPA3: u8 = 4;
/// WPA2 and WPA3 together: the join takes WPA3 where the network offers
/// it and the radio's firmware runs its exchange, else WPA2.
pub const KIND_WPA2_OR_WPA3: u8 = KIND_WPA2 | KIND_WPA3;

/// The boot setting: join while the program runs, join before the program
/// starts, or join when a program asks.
pub const BOOT_BACKGROUND: u8 = 0;
pub const BOOT_BEFORE_PROGRAM: u8 = 1;
pub const BOOT_ON_REQUEST: u8 = 2;

/// The reconnection kind after a lost link: by itself, or when asked.
pub const RECONNECT_AUTOMATIC: u8 = 0;
pub const RECONNECT_MANUAL: u8 = 1;

/// Why a slot gives no record.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Fault {
    /// The slice is not one whole unit.
    Unit,
    /// No first record: an erased slot, a zeroed one, or other bytes.
    Magic,
    /// Under a matching CRC-32, a first record of a format this reader
    /// does not know.
    Format,
    /// The first record's CRC-32 does not match, whatever its format byte
    /// says.
    Crc,
    /// Under a matching CRC-32, the name's or the secret's length lies
    /// outside its field.
    Lengths,
    /// Under a matching CRC-32, the accepted kinds are none of the four.
    Kinds,
    /// Under a matching CRC-32, a boot setting the format does not state.
    Boot,
    /// Under a matching CRC-32, a reconnection kind the format does not
    /// state.
    Reconnect,
    /// A later record shorter than 11 bytes or reaching past the unit.
    LaterLength,
    /// A later record's CRC-32 does not match.
    LaterCrc,
    /// The list does not end at erased bytes or at the unit's end.
    LaterEnd,
}

/// The record of an undamaged copy this reader can use, its slices in
/// place.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Record<'a> {
    pub sequence: u32,
    pub name: &'a [u8],
    pub secret: &'a [u8],
    pub kinds: u8,
    pub reconnect: u8,
    pub boot: u8,
    /// The later records skipped by their lengths.
    pub later: usize,
}

impl Fault {
    /// Whether the slot holds an undamaged copy all the same, one this
    /// reader cannot use: the frame's CRC-32 matches, and the format is
    /// not known here or, in the known format, the later records hold and
    /// a value lies outside the known set. Such a copy takes part in the
    /// choice by its sequence number.
    pub fn is_undamaged(self) -> bool {
        matches!(self, Fault::Format | Fault::Lengths | Fault::Kinds | Fault::Boot | Fault::Reconnect)
    }
}

/// Why the two slots give no record.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Absent {
    /// No network is stored: neither slot holds an undamaged copy (each
    /// is erased, cut short or foreign).
    Nothing,
    /// The newest undamaged copy is one this reader cannot use: a format
    /// it does not know, or a value outside the known set. The older copy
    /// is not taken.
    Unknown,
}

/// CRC-32 as IEEE 802.3 has it: the reflected polynomial 0xEDB88320, the
/// initial value and the final XOR both all ones; four bits a step from a
/// table of sixteen words.
pub fn crc32(bytes: &[u8]) -> u32 {
    const TABLE: [u32; 16] = {
        let mut table = [0u32; 16];
        let mut i = 0;
        while i < 16 {
            let mut c = i as u32;
            let mut k = 0;
            while k < 4 {
                c = if c & 1 != 0 { 0xEDB8_8320 ^ (c >> 1) } else { c >> 1 };
                k += 1;
            }
            table[i] = c;
            i += 1;
        }
        table
    };
    let mut c = !0u32;
    for &b in bytes {
        c ^= b as u32;
        c = TABLE[(c & 15) as usize] ^ (c >> 4);
        c = TABLE[(c & 15) as usize] ^ (c >> 4);
    }
    !c
}

fn word(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
}

/// One slot read: its record when the slot holds an undamaged copy this
/// reader can use, else why not. The order is the rule's. The frame's
/// CRC-32 is judged before the format byte, so a copy cut short is damaged
/// whatever its format byte says; and in the known format the later
/// records are walked before any value is judged, so a list that does not
/// hold is damage whatever the first record holds.
pub fn read(unit: &[u8]) -> Result<Record<'_>, Fault> {
    if unit.len() != UNIT {
        return Err(Fault::Unit);
    }
    if unit[..4] != MAGIC {
        return Err(Fault::Magic);
    }
    if crc32(&unit[..COVERED]) != word(unit, COVERED) {
        return Err(Fault::Crc);
    }
    if unit[4] != 1 {
        return Err(Fault::Format);
    }
    let later = later_records(unit)?;
    let (name_len, secret_len) = (unit[6] as usize, unit[7] as usize);
    if name_len == 0 || name_len > NAME_MAX || secret_len > SECRET_MAX {
        return Err(Fault::Lengths);
    }
    if !matches!(unit[5], KIND_OPEN | KIND_WPA2 | KIND_WPA3 | KIND_WPA2_OR_WPA3) {
        return Err(Fault::Kinds);
    }
    if unit[173] > BOOT_ON_REQUEST {
        return Err(Fault::Boot);
    }
    if unit[172] > RECONNECT_MANUAL {
        return Err(Fault::Reconnect);
    }
    Ok(Record {
        sequence: word(unit, SEQUENCE_AT),
        name: &unit[NAME_AT..NAME_AT + name_len],
        secret: &unit[SECRET_AT..SECRET_AT + secret_len],
        kinds: unit[5],
        reconnect: unit[172],
        boot: unit[173],
        later,
    })
}

/// The later records of a whole unit in the known format, walked by their
/// lengths to the list's end: how many there are, or what does not hold.
fn later_records(unit: &[u8]) -> Result<usize, Fault> {
    let mut at = FIRST;
    let mut later = 0;
    loop {
        let left = UNIT - at;
        if left < 4 {
            // No magic can start here: the unit's end, when what is left
            // is erased.
            if unit[at..].iter().any(|&b| b != 0xFF) {
                return Err(Fault::LaterEnd);
            }
            break;
        }
        if unit[at..at + 4] == [0xFF; 4] {
            break;
        }
        if left < LATER_MIN {
            return Err(Fault::LaterLength);
        }
        let len = u16::from_le_bytes([unit[at + 5], unit[at + 6]]) as usize;
        if len < LATER_MIN || len > left {
            return Err(Fault::LaterLength);
        }
        if crc32(&unit[at..at + len - 4]) != word(unit, at + len - 4) {
            return Err(Fault::LaterCrc);
        }
        at += len;
        later += 1;
    }
    Ok(later)
}

/// Whether sequence number `a` is newer than `b`, across a wrap: their
/// difference modulo 2^32, read as a signed number, is above zero.
pub fn newer(a: u32, b: u32) -> bool {
    (a.wrapping_sub(b) as i32) > 0
}

/// What one slot gives the choice: nothing when it holds no undamaged
/// copy; else the copy's sequence number, read at its place in every
/// format, with its record or why this reader cannot use it.
fn copy(unit: &[u8]) -> Option<(u32, Result<Record<'_>, Fault>)> {
    match read(unit) {
        Ok(record) => Some((record.sequence, Ok(record))),
        Err(fault) if fault.is_undamaged() => Some((word(unit, SEQUENCE_AT), Err(fault))),
        Err(_) => None,
    }
}

/// The current record of the two slots. Of the undamaged copies the one
/// with the newer sequence number rules (the first slot when their numbers
/// are equal), whatever it holds: when this reader cannot use it, no
/// network is stored for this kernel and the older copy is not taken.
pub fn current<'a>(first: &'a [u8], second: &'a [u8]) -> Result<Record<'a>, Absent> {
    let newest = match (copy(first), copy(second)) {
        (Some((number, held)), Some((other, beside))) => Some(if newer(other, number) { beside } else { held }),
        (Some((_, held)), None) | (None, Some((_, held))) => Some(held),
        (None, None) => None,
    };
    match newest {
        Some(Ok(record)) => Ok(record),
        Some(Err(_)) => Err(Absent::Unknown),
        None => Err(Absent::Nothing),
    }
}
