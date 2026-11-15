//! The radio's three images, their pins, and the corner of the flash they
//! travel in. The images are the chip vendor's: the firmware, the
//! regulatory blob and the board settings in their converted form. No byte
//! of them is in this crate or in a kernel image: a flash tool writes them
//! into the radio's corner, the last 256 KB of the flash device, and this
//! module finds and checks them there. A pin is an image's length and its
//! SHA-256 as published.
//!
//! THE CORNER, every number little-endian. Its last two 4 KB units are the
//! stored network's; the bytes before them are the room for the images.
//! The room begins with a directory in a unit of its own: the magic
//! `LWFD`; the format, 1; the directory's length with its CRC-32, two
//! bytes; the number of entries; the entries, 44 bytes each -- the kind (1
//! the firmware, 2 the regulatory blob, 3 the settings; other kinds are
//! other readers'), three reserved bytes, the image's offset in the
//! corner, its length, its SHA-256 -- and the CRC-32 of every byte before
//! it. An image starts on a unit boundary after the directory's unit and
//! ends inside the room. A kind may be listed more than once.
//!
//! WHAT THE KERNEL DOES AT A START, before the radio's power line is
//! raised: it reads the directory, takes for each of its three pins the
//! entry of that kind whose length and digest are the pin's, and then
//! computes the SHA-256 of each image where it lies and compares it with
//! the pin. The directory's word alone is never trusted: the content
//! decides, so a write of the corner that was cut short is caught here
//! whatever the directory says. A directory that cannot be read, a pin
//! with no entry or an image that is not its pinned bytes leaves the radio
//! off. This kernel never writes or erases the corner.
//!
//! A kernel is built for one set of pins: a new firmware is a new set,
//! never a replacement in place.

use crate::digest::{self, Sha256, DIGEST_BYTES};
use crate::wifi_stored::crc32;

/// One erase unit of the flash.
pub const UNIT: usize = 4096;
/// The radio's corner: the last bytes of the flash device, the stored
/// network's two units among them.
pub const CORNER: usize = 262_144;
/// The room for the directory and the images.
pub const ROOM: usize = CORNER - 2 * UNIT;

const MAGIC: [u8; 4] = *b"LWFD";
const FORMAT: u8 = 1;
/// The directory's bytes before its entries, and one entry.
const HEAD: usize = 8;
const ENTRY: usize = 44;
const KIND_FIRMWARE: u8 = 1;
const KIND_REGULATORY: u8 = 2;
const KIND_SETTINGS: u8 = 3;

/// One image's pin.
pub struct Pin {
    pub len: usize,
    /// Its SHA-256, as 64 lowercase hexadecimal characters.
    pub sha256: &'static str,
}

/// The three pins and the version string the firmware must answer with.
pub struct Pins {
    pub firmware: Pin,
    pub regulatory: Pin,
    pub settings: Pin,
    pub version: &'static [u8],
}

/// The pins this kernel is built for: the CYW43439's firmware 7.95.49 and
/// its regulatory blob, as the chip vendor's redistributable combined
/// artifact carries them, and the settings image converted from the
/// vendor's board-settings header for the module.
pub const PINS: Pins = Pins {
    firmware: Pin { len: 224_190, sha256: "dda7d2527e1cc9b1ebdd107d46932643617f6e83040f24cba722cbb0ded8c4a8" },
    regulatory: Pin { len: 984, sha256: "e712b3d218e8b1e2747b092e03b8b0afcb8c8c8e355d2a4a0d47b493800f3f89" },
    settings: Pin { len: 741, sha256: "0517e0a609291e10c264d7d470acf3d823c23a9e7f7d5b12816a6d45b5cf1f71" },
    version: b"7.95.49",
};

/// Which image.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Image {
    Firmware,
    Regulatory,
    Settings,
}

/// Why the corner does not give this kernel its images.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Fault {
    /// The slice is not the directory's whole unit.
    Unit,
    /// No directory: an erased unit, a zeroed one, or other bytes.
    Magic,
    /// A directory of a format this reader does not know.
    Format,
    /// The length is not the one its number of entries gives, or there is
    /// no entry.
    Length,
    /// The directory's CRC-32 does not match.
    Crc,
    /// Under a matching CRC-32, the entry of this index lies off a unit
    /// boundary, inside the directory's unit or past the room, or is empty.
    Entry(u8),
    /// No entry of this image's kind carries the pin's length and digest.
    Missing(Image),
    /// The image's bytes are not its pinned bytes.
    Content(Image),
}

/// Where the three images lie in the corner: the offset and the length of
/// each.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Places {
    pub firmware: (usize, usize),
    pub regulatory: (usize, usize),
    pub settings: (usize, usize),
}

fn word(bytes: &[u8], at: usize) -> usize {
    u32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]]) as usize
}

fn is_pin(pin: &Pin, len: usize, digest: &[u8]) -> bool {
    let mut sum = [0u8; DIGEST_BYTES];
    sum.copy_from_slice(digest);
    len == pin.len && digest::hex(&sum)[..] == *pin.sha256.as_bytes()
}

/// The directory read: the places of the three images of `pins`. `unit`
/// is the corner's first unit as it lies in flash.
pub fn places(unit: &[u8], pins: &Pins) -> Result<Places, Fault> {
    if unit.len() != UNIT {
        return Err(Fault::Unit);
    }
    if unit[..4] != MAGIC {
        return Err(Fault::Magic);
    }
    if unit[4] != FORMAT {
        return Err(Fault::Format);
    }
    let length = usize::from(u16::from_le_bytes([unit[5], unit[6]]));
    let count = usize::from(unit[7]);
    if count == 0 || length != HEAD + ENTRY * count + 4 || length > UNIT {
        return Err(Fault::Length);
    }
    if crc32(&unit[..length - 4]) != word(unit, length - 4) as u32 {
        return Err(Fault::Crc);
    }
    let wanted = [(KIND_FIRMWARE, &pins.firmware), (KIND_REGULATORY, &pins.regulatory), (KIND_SETTINGS, &pins.settings)];
    let mut found = [None; 3];
    for i in 0..count {
        let entry = &unit[HEAD + ENTRY * i..HEAD + ENTRY * (i + 1)];
        let (offset, len) = (word(entry, 4), word(entry, 8));
        let inside = offset % UNIT == 0 && offset >= UNIT && len != 0 && offset.checked_add(len).is_some_and(|end| end <= ROOM);
        if !inside {
            return Err(Fault::Entry(i as u8));
        }
        for (slot, (kind, pin)) in found.iter_mut().zip(wanted) {
            if slot.is_none() && entry[0] == kind && is_pin(pin, len, &entry[12..]) {
                *slot = Some((offset, len));
            }
        }
    }
    match found {
        [Some(firmware), Some(regulatory), Some(settings)] => Ok(Places { firmware, regulatory, settings }),
        [None, _, _] => Err(Fault::Missing(Image::Firmware)),
        [_, None, _] => Err(Fault::Missing(Image::Regulatory)),
        _ => Err(Fault::Missing(Image::Settings)),
    }
}

/// One step of the content's check.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Step {
    /// More bytes remain: step again.
    More,
    /// The three images are their pinned bytes.
    Good,
    /// This image is not its pinned bytes.
    Bad(Image),
}

/// The check of the three images' content, in steps of a bounded number of
/// bytes, so that no pass of the caller's thread grows.
pub struct Check {
    places: Places,
    hash: Sha256,
    /// The image in work (0 to 2), and its bytes digested so far.
    image: u8,
    done: usize,
    verdict: Option<Step>,
}

impl Check {
    pub const fn new(places: Places) -> Self {
        Check { places, hash: Sha256::new(), image: 0, done: 0, verdict: None }
    }

    /// Digest at most `budget` further bytes of the corner's `room` and
    /// say where the check stands. A verdict, once given, is repeated.
    pub fn step(&mut self, room: &[u8], pins: &Pins, budget: usize) -> Step {
        if let Some(verdict) = self.verdict {
            return verdict;
        }
        let (image, (offset, len), pin) = match self.image {
            0 => (Image::Firmware, self.places.firmware, &pins.firmware),
            1 => (Image::Regulatory, self.places.regulatory, &pins.regulatory),
            _ => (Image::Settings, self.places.settings, &pins.settings),
        };
        // The directory's reader bounded the places by the room; a slice
        // shorter than they say is not the room.
        let Some(bytes) = room.get(offset..offset + len) else {
            return self.settle(Step::Bad(image));
        };
        let take = (len - self.done).min(budget.max(1));
        self.hash.update(&bytes[self.done..self.done + take]);
        self.done += take;
        if self.done < len {
            return Step::More;
        }
        let sum = core::mem::replace(&mut self.hash, Sha256::new()).finish();
        if !is_pin(pin, len, &sum) {
            return self.settle(Step::Bad(image));
        }
        self.done = 0;
        self.image += 1;
        if self.image == 3 {
            return self.settle(Step::Good);
        }
        Step::More
    }

    fn settle(&mut self, verdict: Step) -> Step {
        self.verdict = Some(verdict);
        verdict
    }
}

/// The whole check at once: the directory read and the three images
/// digested where they lie in `room`, the corner without the stored
/// network's two units.
pub fn check(room: &[u8], pins: &Pins) -> Result<Places, Fault> {
    let places = places(room.get(..UNIT).ok_or(Fault::Unit)?, pins)?;
    let mut check = Check::new(places);
    loop {
        match check.step(room, pins, usize::MAX) {
            Step::More => {}
            Step::Good => return Ok(places),
            Step::Bad(image) => return Err(Fault::Content(image)),
        }
    }
}
