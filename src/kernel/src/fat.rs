//! The FAT filesystem layer: RHU's own FAT12, FAT16 and FAT32 with long
//! names, one pure module over a block-device trait of its own, written from
//! the Microsoft FAT Specification (the section numbers in this file are that
//! document's) -- no_std, no allocator, no interrupt, nothing under the
//! critical section. The kernel declares it where a row has a card socket
//! (blk.rs's socket half implements the trait over the identified card under
//! the device's tenancy); the host tier compiles it from this file over a
//! sparse in-memory device and pins its cases. The module adds no door: the
//! api checks of the socket row are its caller.
//!
//! THE RULES: the type by the count of clusters alone (3.5 of the
//! specification), never by the type string; every chain walk bounded; a
//! new cluster written END first, then linked, then filled, and the freeing
//! its mirror (the write side); the boot sector, its backup, the FSInfo's
//! signatures and FAT[0] and FAT[1] never written; the FAT read from the
//! active copy and written to every copy when mirroring is on; the errors
//! the Linux codes as the enum's values.
#![allow(dead_code)] // the host tier pins every operation; each kernel image calls the subset its checks need

use crate::calendar_math::{self, Civil};

/// The one sector size supported (the block doors move 512-byte blocks).
pub const SECTOR: usize = 512;
/// The most sectors one device call moves (the block doors' RHU_BLK_MAX, 64 KB).
pub const BLK_MAX: u32 = 128;
/// A name's limit in UTF-8 bytes and in UTF-16 units (Linux's NAME_MAX).
pub const NAME_MAX: usize = 255;
const CACHE_BUFS: usize = 8;
/// The maximum directory size, 2^21 bytes (6.4 of the specification): 65,536 entries.
const DIR_MAX_ENTRIES: u32 = 65_536;
const ATTR_READ_ONLY: u8 = 0x01;
const ATTR_VOLUME_ID: u8 = 0x08;
const ATTR_DIRECTORY: u8 = 0x10;
const ATTR_LONG_NAME: u8 = 0x0F;
const ATTR_LONG_MASK: u8 = 0x3F;
const NTRES_LOWER_BASE: u8 = 0x08;
const NTRES_LOWER_EXT: u8 = 0x10;
const ATTR_ARCHIVE: u8 = 0x20;
/// The FAT floor's packed date, 1 January 1980 (the day in bits 0 to 4, the month in 5 to 8, the years since 1980 above).
const FLOOR_DATE: u16 = (1 << 5) | 1;
/// A long entry's thirteen unit offsets (the specification's section 7: five at 1, six at 14, two at 28).
const LONG_OFFSETS: [usize; 13] = [1, 3, 5, 7, 9, 14, 16, 18, 20, 22, 24, 28, 30];

/// The one error type: the Linux codes as the values, so the map to the
/// doors' answers is the identity.
#[repr(i32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    NotFound = -2,
    Io = -5,
    BadHandle = -9,
    Exists = -17,
    NoDevice = -19,
    NotDir = -20,
    IsDir = -21,
    Invalid = -22,
    TooManyOpen = -24,
    NoSpace = -28,
    ReadOnly = -30,
    NameTooLong = -36,
    NotEmpty = -39,
    Timeout = -110,
}

impl Error {
    pub fn code(self) -> i32 {
        self as i32
    }
}

/// A mount's refusal with its words.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refused {
    NoSignature,
    SectorSize,
    BadBpb,
    ExFat,
    /// The partition table's first entry is GPT's protective one.
    Gpt,
    /// No entry of a FAT type in the partition table.
    NoPartition,
    /// An entry that starts at 0, holds no sector or runs past the device.
    BadPartition,
    Device(Error),
}

impl Refused {
    pub fn words(&self) -> &'static str {
        match self {
            Refused::NoSignature => "no boot signature",
            Refused::SectorSize => "unsupported sector size",
            Refused::BadBpb => "bad BPB",
            Refused::ExFat => "exFAT: not supported",
            Refused::Gpt => "GPT: not supported",
            Refused::NoPartition => "no FAT partition",
            Refused::BadPartition => "bad partition table",
            Refused::Device(Error::NoDevice) => "no device",
            Refused::Device(Error::Timeout) => "the device timed out",
            Refused::Device(_) => "a device error",
        }
    }
}

/// The seam under the layer: whole 512-byte sectors by number, up to
/// BLK_MAX per call, the buffers 4-byte aligned (16 for the F7 host's
/// INCR4 bursts); the errors Io, Timeout, ReadOnly and NoDevice.
pub trait BlockDevice {
    fn blocks(&mut self) -> u32;
    fn read(&mut self, lba: u32, buf: &mut [u8]) -> Result<(), Error>;
    fn write(&mut self, lba: u32, data: &[u8]) -> Result<(), Error>;
    fn sync(&mut self) -> Result<(), Error>;
}

/// A device read and a write with the volume's base added: the layer's sector numbers are the
/// volume's, the device's are the partition's; the master boot record below the base is never written.
fn dread<D: BlockDevice>(dev: &mut D, base: u32, lba: u32, buf: &mut [u8]) -> Result<(), Error> {
    dev.read(base + lba, buf)
}

fn dwrite<D: BlockDevice>(dev: &mut D, base: u32, lba: u32, data: &[u8]) -> Result<(), Error> {
    dev.write(base + lba, data)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Fat12,
    Fat16,
    Fat32,
}

/// The volume's geometry as the boot sector states it, the type by the
/// count of clusters (3.5 of the specification, its arithmetic verbatim).
#[derive(Clone, Copy, Debug)]
pub struct Geometry {
    pub kind: Kind,
    pub sec_per_clus: u32,
    pub cluster_bytes: u32,
    pub rsvd: u32,
    pub num_fats: u32,
    pub root_ent_cnt: u32,
    pub tot_sec: u32,
    pub fat_sz: u32,
    pub root_dir_sectors: u32,
    pub first_root_sector: u32,
    pub first_data_sector: u32,
    pub count_of_clusters: u32,
    /// The maximum valid cluster number, CountOfClusters + 1.
    pub max: u32,
    /// The root's first cluster on FAT32; 0 on FAT12 and FAT16 (the fixed region).
    pub root_clus: u32,
    /// The FSInfo's sector on FAT32 when its signatures held; 0 otherwise.
    pub fsinfo: u32,
    pub mirroring: bool,
    pub active_fat: u32,
    /// The volume's first device sector: the partition table's entry, or 0 for a boot sector at block 0.
    pub base: u32,
}

impl Geometry {
    /// The type from the count alone (specification 3.5): under 4085 FAT12, under 65525 FAT16, else FAT32.
    pub const fn kind_of(count_of_clusters: u32) -> Kind {
        if count_of_clusters < 4085 {
            Kind::Fat12
        } else if count_of_clusters < 65525 {
            Kind::Fat16
        } else {
            Kind::Fat32
        }
    }

    /// A cluster's first sector (specification 6.7): (N - 2) * SecPerClus + FirstDataSector.
    pub fn first_sector_of(&self, cluster: u32) -> u32 {
        (cluster - 2) * self.sec_per_clus + self.first_data_sector
    }

    fn is_root_cluster(&self, c: u32) -> bool {
        c == 0 || (self.kind == Kind::Fat32 && c == self.root_clus)
    }

    fn eoc_mark(&self) -> u32 {
        match self.kind {
            Kind::Fat12 => 0xFFF,
            Kind::Fat16 => 0xFFFF,
            Kind::Fat32 => 0x0FFF_FFFF,
        }
    }
}

/// One cache buffer: a sector, aligned for the host's bursts.
#[repr(C, align(16))]
#[derive(Clone, Copy)]
pub struct Sector(pub [u8; SECTOR]);

/// The sector cache's readings (the host case (20)).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CacheStats {
    pub hits: u32,
    pub misses: u32,
    pub evictions: u32,
    pub flushes: u32,
}

struct Cache {
    bufs: [Sector; CACHE_BUFS],
    tag: [u32; CACHE_BUFS],
    dirty: [bool; CACHE_BUFS],
    fat: [bool; CACHE_BUFS],
    used: [u32; CACHE_BUFS],
    clock: u32,
    /// The dirtying order: the sequence number taken when a buffer first became dirty (the flush order of the write side).
    stamp: [u32; CACHE_BUFS],
    seq: u32,
    stats: CacheStats,
}

const NO_TAG: u32 = u32::MAX;

impl Cache {
    const fn new() -> Cache {
        Cache { bufs: [Sector([0; SECTOR]); CACHE_BUFS], tag: [NO_TAG; CACHE_BUFS], dirty: [false; CACHE_BUFS], fat: [false; CACHE_BUFS], used: [0; CACHE_BUFS], clock: 0, stamp: [0; CACHE_BUFS], seq: 0, stats: CacheStats { hits: 0, misses: 0, evictions: 0, flushes: 0 } }
    }
}

/// A directory entry as the walk serves it: the name in UTF-8 (the long
/// name when its set is whole, else the short name as BASE.EXT), the
/// attributes, the times, the first cluster, the size, and the entry's
/// place on the volume with the start of its long set (the write side's).
#[derive(Clone, Copy)]
pub struct Entry {
    pub name: [u8; 256],
    pub name_len: u16,
    pub attr: u8,
    pub ntres: u8,
    pub crt_tenth: u8,
    pub crt_time: u16,
    pub crt_date: u16,
    pub acc_date: u16,
    pub wrt_time: u16,
    pub wrt_date: u16,
    pub first: u32,
    pub size: u32,
    /// The short entry's sector and byte offset; (0, 0) for the root itself.
    pub place: (u32, u16),
    /// The first long entry's sector and offset, or the short entry's when it has no set.
    pub set_start: (u32, u16),
    pub short: [u8; 11],
    /// Served by its long name.
    pub long: bool,
    /// The short entry's raw index in its directory and the count of long entries before it (the write side's).
    pub index: u32,
    pub set_len: u8,
    /// The directory holding the entry as a walk names it: 0 the fixed root, the root cluster on FAT32, else the first cluster.
    pub dir_first: u32,
}

impl Entry {
    pub const EMPTY: Entry = Entry { name: [0; 256], name_len: 0, attr: 0, ntres: 0, crt_tenth: 0, crt_time: 0, crt_date: 0, acc_date: 0, wrt_time: 0, wrt_date: 0, first: 0, size: 0, place: (0, 0), set_start: (0, 0), short: [0x20; 11], long: false, index: 0, set_len: 0, dir_first: 0 };

    pub fn name(&self) -> &[u8] {
        &self.name[..self.name_len as usize]
    }

    pub fn is_dir(&self) -> bool {
        self.attr & ATTR_DIRECTORY != 0
    }

    pub fn read_only(&self) -> bool {
        self.attr & ATTR_READ_ONLY != 0
    }

    fn set_name(&mut self, bytes: &[u8]) {
        self.name[..bytes.len()].copy_from_slice(bytes);
        self.name_len = bytes.len() as u16;
    }
}

/// A file's record for stat and fstat: the size, the kind, the read-only
/// flag, the three times as Unix seconds (two-second resolution; the
/// access time its date at midnight), the first cluster as the inode-like
/// number (0 for every empty file).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Stat {
    pub size: u32,
    pub is_dir: bool,
    pub read_only: bool,
    pub attr: u8,
    pub first: u32,
    pub mtime: i64,
    pub ctime: i64,
    pub atime: i64,
}

/// The volume's figures for statfs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Statfs {
    pub block_size: u32,
    pub blocks: u32,
    pub free: u32,
    pub name_max: u32,
}

pub const O_READ: u32 = 1;
pub const O_WRITE: u32 = 2;
pub const O_CREATE: u32 = 4;
pub const O_EXCL: u32 = 8;
pub const O_TRUNC: u32 = 16;
pub const O_APPEND: u32 = 32;
pub const SEEK_SET: u32 = 0;
pub const SEEK_CUR: u32 = 1;
pub const SEEK_END: u32 = 2;

/// An open file: a plain struct the caller owns.
#[derive(Clone, Copy)]
pub struct Handle {
    first: u32,
    size: u32,
    pos: u32,
    cur: u32,
    cur_idx: u32,
    attr: u8,
    place: (u32, u16),
    times: [u16; 5],
    crt_tenth: u8,
    flags: u32,
    open: bool,
    /// The directory entry owed a write (the size, the first cluster, the times) at sync and close.
    dirty: bool,
}

impl Handle {
    pub fn position(&self) -> u32 {
        self.pos
    }
    pub fn size(&self) -> u32 {
        self.size
    }
    pub fn first_cluster(&self) -> u32 {
        self.first
    }
}

/// An open directory: the walk's position.
#[derive(Clone, Copy)]
pub struct Dir {
    first: u32,
    index: u32,
    cur: u32,
    cur_idx: u32,
    root: bool,
    open: bool,
}

/// A path's resolution: the entry and whether it is the root itself.
#[derive(Clone, Copy)]
struct Located {
    entry: Entry,
    is_root: bool,
}

/// The long-name set under assembly (the specification's section 7): the entries stand before
/// the short entry in reverse order, each with its ordinal, 13 UTF-16 units
/// and the short name's checksum; a set that fails any check is dropped.
struct LongBuf {
    units: [u16; 260],
    n: u8,
    expect: u8,
    chksum: u8,
    valid: bool,
    start: (u32, u16),
}

impl LongBuf {
    const fn new() -> LongBuf {
        LongBuf { units: [0; 260], n: 0, expect: 0, chksum: 0, valid: false, start: (0, 0) }
    }

    fn reset(&mut self) {
        self.valid = false;
        self.n = 0;
        self.expect = 0;
    }

    fn feed(&mut self, e: &[u8; 32], lba: u32, off: u16) {
        let ord = e[0];
        if ord & 0x40 != 0 {
            let n = ord & 0x3F;
            if n == 0 || n > 20 {
                self.reset();
                return;
            }
            self.n = n;
            self.expect = n;
            self.chksum = e[13];
            self.valid = true;
            self.start = (lba, off);
        } else if !self.valid || ord != self.expect || e[13] != self.chksum {
            self.reset();
            return;
        }
        let base = (self.expect as usize - 1) * 13;
        let mut k = 0;
        for &o in &[1usize, 3, 5, 7, 9, 14, 16, 18, 20, 22, 24, 28, 30] {
            self.units[base + k] = rd16(e, o);
            k += 1;
        }
        self.expect -= 1;
    }

    fn complete(&self, sum: u8) -> bool {
        self.valid && self.n > 0 && self.expect == 0 && self.chksum == sum
    }

    fn name(&self) -> &[u16] {
        let total = self.n as usize * 13;
        let mut end = self.units[..total].iter().position(|&u| u == 0).unwrap_or(total);
        while end > 0 && self.units[end - 1] == 0xFFFF {
            end -= 1;
        }
        &self.units[..end]
    }
}

fn rd16(b: &[u8], o: usize) -> u16 {
    u16::from_le_bytes([b[o], b[o + 1]])
}

fn rd32(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
}

/// The specification's 7.2 checksum over the short name's eleven bytes.
pub fn checksum(short: &[u8]) -> u8 {
    let mut sum: u8 = 0;
    for &b in &short[..11] {
        sum = ((sum & 1) << 7).wrapping_add(sum >> 1).wrapping_add(b);
    }
    sum
}

/// The case fold of the match (specification 7.4): ASCII letters and the Latin-1 letters
/// U+00C0 to U+00FE (not the two signs), every other unit as it is.
pub fn fold(u: u16) -> u16 {
    match u {
        0x41..=0x5A => u + 0x20,
        0xC0..=0xDE if u != 0xD7 => u + 0x20,
        _ => u,
    }
}

fn eq_fold(a: &[u16], b: &[u16]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(x, y)| fold(*x) == fold(*y))
}

/// A short name's BASE.EXT as UTF-16 units (for the match): the padding
/// stripped, a byte over 0x7F served as U+FFFD, the Kanji lead restored.
fn short_units(raw: &[u8; 11], out: &mut [u16; 13]) -> usize {
    let mut n = 0;
    let base_end = raw[..8].iter().rposition(|&b| b != 0x20).map(|p| p + 1).unwrap_or(0);
    let ext_end = raw[8..].iter().rposition(|&b| b != 0x20).map(|p| p + 1).unwrap_or(0);
    for (i, &b) in raw[..base_end].iter().enumerate() {
        let b = if i == 0 && b == 0x05 { 0xE5 } else { b };
        out[n] = if b < 0x80 { b as u16 } else { 0xFFFD };
        n += 1;
    }
    if ext_end > 0 {
        out[n] = b'.' as u16;
        n += 1;
        for &b in &raw[8..8 + ext_end] {
            out[n] = if b < 0x80 { b as u16 } else { 0xFFFD };
            n += 1;
        }
    }
    n
}

/// A short name as the walk serves it: BASE.EXT with the NTRes case bits applied.
fn short_name_utf8(raw: &[u8; 11], ntres: u8, out: &mut [u8; 256]) -> usize {
    let mut units = [0u16; 13];
    let n = short_units(raw, &mut units);
    let mut len = 0;
    let mut in_ext = false;
    for &u in &units[..n] {
        if u == b'.' as u16 {
            in_ext = true;
            out[len] = b'.';
            len += 1;
        } else if u == 0xFFFD {
            out[len..len + 3].copy_from_slice(&[0xEF, 0xBF, 0xBD]);
            len += 3;
        } else {
            let lower = if in_ext { ntres & NTRES_LOWER_EXT != 0 } else { ntres & NTRES_LOWER_BASE != 0 };
            let b = u as u8;
            out[len] = if lower && b.is_ascii_uppercase() { b + 0x20 } else { b };
            len += 1;
        }
    }
    len
}

/// A long name's UTF-16 units as UTF-8 (a surrogate pair one supplementary
/// character, a lone surrogate U+FFFD); None past NAME_MAX bytes.
fn utf16_to_utf8(units: &[u16], out: &mut [u8; 256]) -> Option<usize> {
    let mut n = 0;
    let mut i = 0;
    while i < units.len() {
        let u = units[i];
        let cp: u32 = if (0xD800..0xDC00).contains(&u) && i + 1 < units.len() && (0xDC00..0xE000).contains(&units[i + 1]) {
            i += 1;
            0x10000 + ((u as u32 - 0xD800) << 10) + (units[i] as u32 - 0xDC00)
        } else if (0xD800..0xE000).contains(&u) {
            0xFFFD
        } else {
            u as u32
        };
        i += 1;
        let len = if cp < 0x80 { 1 } else if cp < 0x800 { 2 } else if cp < 0x10000 { 3 } else { 4 };
        if n + len > NAME_MAX {
            return None;
        }
        match len {
            1 => out[n] = cp as u8,
            2 => {
                out[n] = 0xC0 | (cp >> 6) as u8;
                out[n + 1] = 0x80 | (cp & 0x3F) as u8;
            }
            3 => {
                out[n] = 0xE0 | (cp >> 12) as u8;
                out[n + 1] = 0x80 | ((cp >> 6) & 0x3F) as u8;
                out[n + 2] = 0x80 | (cp & 0x3F) as u8;
            }
            _ => {
                out[n] = 0xF0 | (cp >> 18) as u8;
                out[n + 1] = 0x80 | ((cp >> 12) & 0x3F) as u8;
                out[n + 2] = 0x80 | ((cp >> 6) & 0x3F) as u8;
                out[n + 3] = 0x80 | (cp & 0x3F) as u8;
            }
        }
        n += len;
    }
    Some(n)
}

/// A caller's name component (specification 7.4 and 6.1): UTF-8 decoded to
/// UTF-16 (a malformed sequence Invalid), leading and trailing spaces and
/// trailing periods dropped, empty and "." and ".." Invalid, a unit under
/// 0x20 or one of the nine the long set forbids Invalid, past 255 units
/// NameTooLong.
pub fn decode_name(utf8: &[u8], out: &mut [u16; 256]) -> Result<usize, Error> {
    let mut n = 0;
    let mut i = 0;
    while i < utf8.len() {
        let b0 = utf8[i] as u32;
        let (cp, len) = if b0 < 0x80 {
            (b0, 1)
        } else if b0 & 0xE0 == 0xC0 && b0 >= 0xC2 {
            (b0 & 0x1F, 2)
        } else if b0 & 0xF0 == 0xE0 {
            (b0 & 0x0F, 3)
        } else if b0 & 0xF8 == 0xF0 && b0 <= 0xF4 {
            (b0 & 0x07, 4)
        } else {
            return Err(Error::Invalid);
        };
        if i + len > utf8.len() {
            return Err(Error::Invalid);
        }
        let mut cp = cp;
        for k in 1..len {
            let b = utf8[i + k] as u32;
            if b & 0xC0 != 0x80 {
                return Err(Error::Invalid);
            }
            cp = (cp << 6) | (b & 0x3F);
        }
        let overlong = (len == 3 && cp < 0x800) || (len == 4 && cp < 0x10000);
        if overlong || cp > 0x10FFFF || (0xD800..0xE000).contains(&cp) {
            return Err(Error::Invalid);
        }
        i += len;
        let units = if cp >= 0x10000 { 2 } else { 1 };
        if n + units > NAME_MAX {
            return Err(Error::NameTooLong);
        }
        if cp >= 0x10000 {
            let c = cp - 0x10000;
            out[n] = 0xD800 | (c >> 10) as u16;
            out[n + 1] = 0xDC00 | (c & 0x3FF) as u16;
        } else {
            out[n] = cp as u16;
        }
        n += units;
    }
    let mut start = 0;
    while start < n && out[start] == 0x20 {
        start += 1;
    }
    let mut end = n;
    while end > start && (out[end - 1] == 0x20 || out[end - 1] == b'.' as u16) {
        end -= 1;
    }
    if end == start {
        return Err(Error::Invalid);
    }
    for &u in &out[start..end] {
        if u < 0x20 || matches!(u as u8, b'"' | b'*' | b'/' | b':' | b'<' | b'>' | b'?' | b'\\' | b'|') && u < 0x80 {
            return Err(Error::Invalid);
        }
    }
    if start > 0 {
        out.copy_within(start..end, 0);
    }
    Ok(end - start)
}

/// A packed FAT date and time as Unix seconds (specification 6.3); the floor,
/// midnight of 1 January 1980, for a field outside its range.
pub fn unix_from_packed(date: u16, time: u16) -> i64 {
    let (y, m, d) = ((date >> 9) as u32 + 1980, ((date >> 5) & 0xF) as u32, (date & 0x1F) as u32);
    let (hh, mm, ss) = ((time >> 11) as u32, ((time >> 5) & 0x3F) as u32, (time & 0x1F) as u32 * 2);
    if m == 0 || m > 12 || d == 0 || d > 31 || hh > 23 || mm > 59 || ss > 58 {
        return unix_from_packed(1 << 5 | 1, 0);
    }
    let days = calendar_math::days_from_civil(y, m, d) as i64 - calendar_math::days_from_civil(1970, 1, 1) as i64;
    days * 86_400 + hh as i64 * 3_600 + mm as i64 * 60 + ss as i64
}

/// A mounted volume over its device: the geometry, the sector cache, the
/// label, the free count and the next-free hint.
pub struct Volume<D: BlockDevice> {
    dev: D,
    /// The partition's base added to every device call.
    base: u32,
    geo: Geometry,
    cache: Cache,
    label: [u8; 11],
    free_count: u32,
    next_free: u32,
    hint_served: bool,
    pass_calls: u32,
    /// The clock seam: the kernel gives the calendar's reading, the host tier its own; None is the floor.
    now: fn() -> Option<Civil>,
    /// The FSInfo's two fields as read at mount or last written: a sync writes them when they differ.
    fsinfo_seen: (u32, u32),
}

fn no_clock() -> Option<Civil> {
    None
}

/// A unit of the short set (specification 6.1): printable ASCII but the sixteen the
/// specification lists and the space; lower-case letters are not excluded
/// here (the legal-short test excludes them, the alias folds them).
fn short_ok(u: u16) -> bool {
    u > 0x20 && u < 0x7F && !matches!(u as u8, b'"' | b'*' | b'+' | b',' | b'.' | b'/' | b':' | b';' | b'<' | b'=' | b'>' | b'?' | b'[' | b'\\' | b']' | b'|')
}

/// A long name's unit as the alias takes it (specification 7.4): upper-cased; outside the
/// short set or over 0x7F '_'.
fn alias_byte(u: u16) -> u8 {
    if u < 0x80 {
        let b = u as u8;
        if b.is_ascii_lowercase() {
            b - 0x20
        } else if short_ok(u) {
            b
        } else {
            b'_'
        }
    } else {
        b'_'
    }
}

/// A legal short name in upper case already: 1 to 8 base
/// characters from the short set, an optional period with 1 to 3 extension
/// characters, no other period, no lower-case letter; its eleven bytes
/// when so.
fn legal_short(units: &[u16], out: &mut [u8; 11]) -> bool {
    let dot = units.iter().position(|&u| u == b'.' as u16);
    let (b, e): (&[u16], &[u16]) = match dot {
        Some(p) => (&units[..p], &units[p + 1..]),
        None => (units, &[]),
    };
    if b.is_empty() || b.len() > 8 || (dot.is_some() && (e.is_empty() || e.len() > 3)) {
        return false;
    }
    let ok = |u: u16| short_ok(u) && !(b'a' as u16..=b'z' as u16).contains(&u);
    if !b.iter().all(|&u| ok(u)) || !e.iter().all(|&u| ok(u)) {
        return false;
    }
    *out = [0x20; 11];
    for (i, &u) in b.iter().enumerate() {
        out[i] = u as u8;
    }
    for (i, &u) in e.iter().enumerate() {
        out[8 + i] = u as u8;
    }
    true
}

impl<D: BlockDevice> Volume<D> {
    /// The mount: the boot sector read and checked, the
    /// type by the count, the FSInfo on FAT32, the free count from the
    /// FSInfo when sane else one pass over the FAT through the caller's
    /// scratch (whole sectors, up to BLK_MAX per call), the label from the
    /// root's first sector.
    pub fn mount(dev: D, scratch: &mut [u8]) -> Result<Volume<D>, Refused> {
        let mut slot = None;
        Self::mount_in(&mut slot, dev, scratch)?;
        Ok(slot.unwrap())
    }

    /// The mount into the caller's slot (the kernel's static: the volume is
    /// built in place, its 4.5 KB never crossing the stack as a return
    /// value); the slot is left None on a refusal.
    pub fn mount_in(slot: &mut Option<Volume<D>>, mut dev: D, scratch: &mut [u8]) -> Result<(), Refused> {
        let device_blocks = dev.blocks();
        let mut bs = Sector([0; SECTOR]);
        dev.read(0, &mut bs.0).map_err(Refused::Device)?;
        if bs.0[510] != 0x55 || bs.0[511] != 0xAA {
            return Err(Refused::NoSignature);
        }
        if &bs.0[3..11] == b"EXFAT   " && rd16(&bs.0, 11) == 0 {
            return Err(Refused::ExFat);
        }
        // The partition-table step: block 0 is the volume's own
        // boot sector when it begins with a jump (BS_jmpBoot, the mark every formatter writes and no PC's record begins
        // with); otherwise it is a master boot record whose first FAT-type entry (or a 0x07 the boot sector decides)
        // places the volume at its starting sector. The boot sector's own fields are checked after, whichever path.
        let (base, blocks) = if matches!(bs.0[0], 0xEB | 0xE9) {
            (0, device_blocks)
        } else {
            let mut found = None;
            for i in 0..4 {
                let e = &bs.0[446 + i * 16..462 + i * 16];
                if e[4] == 0xEE {
                    return Err(Refused::Gpt);
                }
                if found.is_none() && matches!(e[4], 0x01 | 0x04 | 0x06 | 0x07 | 0x0B | 0x0C | 0x0E) {
                    found = Some((rd32(e, 8), rd32(e, 12)));
                }
            }
            let (start, count) = found.ok_or(Refused::NoPartition)?;
            if start == 0 || count == 0 || start as u64 + count as u64 > device_blocks as u64 {
                return Err(Refused::BadPartition);
            }
            dev.read(start, &mut bs.0).map_err(Refused::Device)?;
            if bs.0[510] != 0x55 || bs.0[511] != 0xAA {
                return Err(Refused::NoSignature);
            }
            if &bs.0[3..11] == b"EXFAT   " && rd16(&bs.0, 11) == 0 {
                return Err(Refused::ExFat);
            }
            (start, count)
        };
        let b = &bs.0;
        let bps = rd16(b, 11) as u32;
        if bps != 512 {
            return Err(if matches!(bps, 1024 | 2048 | 4096) { Refused::SectorSize } else { Refused::BadBpb });
        }
        let spc = b[13] as u32;
        let rsvd = rd16(b, 14) as u32;
        let nfats = b[16] as u32;
        let rootent = rd16(b, 17) as u32;
        let tot = match rd16(b, 19) as u32 {
            0 => rd32(b, 32),
            t => t,
        };
        let fatsz = match rd16(b, 22) as u32 {
            0 => rd32(b, 36),
            f => f,
        };
        if spc == 0 || spc > 128 || !spc.is_power_of_two() || rsvd == 0 || !(1..=2).contains(&nfats) || rootent % 16 != 0 || tot == 0 || tot > blocks || fatsz == 0 {
            return Err(Refused::BadBpb);
        }
        let root_dir_sectors = (rootent * 32 + 511) / 512;
        let meta = rsvd as u64 + nfats as u64 * fatsz as u64 + root_dir_sectors as u64;
        if meta >= tot as u64 {
            return Err(Refused::BadBpb);
        }
        let data_sec = tot - meta as u32;
        let count = data_sec / spc;
        if count == 0 {
            return Err(Refused::BadBpb);
        }
        let kind = Geometry::kind_of(count);
        let max = count + 1;
        let fat_bytes_needed: u64 = match kind {
            Kind::Fat12 => ((max as u64 + 1) * 3 + 1) / 2 + 1,
            Kind::Fat16 => (max as u64 + 1) * 2,
            Kind::Fat32 => (max as u64 + 1) * 4,
        };
        if fat_bytes_needed > fatsz as u64 * 512 {
            return Err(Refused::BadBpb);
        }
        let (root_clus, mirroring, active_fat, fsinfo, label_off) = if kind == Kind::Fat32 {
            if rootent != 0 || rd16(b, 22) != 0 {
                return Err(Refused::BadBpb);
            }
            let root_clus = rd32(b, 44);
            if root_clus < 2 || root_clus > max {
                return Err(Refused::BadBpb);
            }
            let flags = rd16(b, 40);
            let mirroring = flags & 0x80 == 0;
            let active = if mirroring { 0 } else { (flags & 0xF) as u32 };
            if active >= nfats {
                return Err(Refused::BadBpb);
            }
            let fsi = rd16(b, 48) as u32;
            (root_clus, mirroring, active, if fsi != 0 && fsi < rsvd { fsi } else { 0 }, 71)
        } else {
            if rootent == 0 {
                return Err(Refused::BadBpb);
            }
            (0, true, 0, 0, 43)
        };
        let geo = Geometry {
            kind,
            sec_per_clus: spc,
            cluster_bytes: spc * 512,
            rsvd,
            num_fats: nfats,
            root_ent_cnt: rootent,
            tot_sec: tot,
            fat_sz: fatsz,
            root_dir_sectors,
            first_root_sector: rsvd + nfats * fatsz,
            first_data_sector: rsvd + nfats * fatsz + root_dir_sectors,
            count_of_clusters: count,
            max,
            root_clus,
            fsinfo,
            mirroring,
            active_fat,
            base,
        };
        let mut label = [0x20u8; 11];
        label.copy_from_slice(&b[label_off..label_off + 11]);
        *slot = Some(Volume { dev, base, geo, cache: Cache::new(), label, free_count: u32::MAX, next_free: 2, hint_served: false, pass_calls: 0, now: no_clock, fsinfo_seen: (u32::MAX, u32::MAX) });
        let v = slot.as_mut().unwrap();
        if let Err(r) = Self::mount_tail(v, scratch) {
            *slot = None;
            return Err(r);
        }
        Ok(())
    }

    /// The mount's second half over the volume in place: the FSInfo, the
    /// free count, the label.
    fn mount_tail(v: &mut Volume<D>, scratch: &mut [u8]) -> Result<(), Refused> {
        let geo = v.geo;
        // The FSInfo (the specification's section 5): advisory, validated at mount; a signature
        // mismatch means no hint, not a refusal.
        if geo.fsinfo != 0 {
            let mut fs = Sector([0; SECTOR]);
            dread(&mut v.dev, v.base, geo.fsinfo, &mut fs.0).map_err(Refused::Device)?;
            if rd32(&fs.0, 0) == 0x4161_5252 && rd32(&fs.0, 484) == 0x6141_7272 && rd32(&fs.0, 508) == 0xAA55_0000 {
                let count = rd32(&fs.0, 488);
                let hint = rd32(&fs.0, 492);
                v.fsinfo_seen = (count, hint);
                if count != 0xFFFF_FFFF && count <= geo.count_of_clusters {
                    v.free_count = count;
                    v.hint_served = true;
                    v.next_free = if (2..=geo.max).contains(&hint) { hint } else { 2 };
                }
            } else {
                v.geo.fsinfo = 0;
            }
        }
        if !v.hint_served {
            let (count, first_free) = v.free_pass(scratch).map_err(Refused::Device)?;
            v.free_count = count;
            v.next_free = first_free;
        }
        // The label from the root's volume-ID entry when its first sector holds one.
        let root_lba = if geo.kind == Kind::Fat32 { geo.first_sector_of(geo.root_clus) } else { geo.first_root_sector };
        let i = v.slot(root_lba, false).map_err(Refused::Device)?;
        let s = v.cache.bufs[i].0;
        for e in s.chunks_exact(32) {
            if e[0] == 0 {
                break;
            }
            if e[0] != 0xE5 && e[11] & ATTR_LONG_MASK != ATTR_LONG_NAME && e[11] & ATTR_VOLUME_ID != 0 {
                v.label.copy_from_slice(&e[..11]);
                break;
            }
        }
        Ok(())
    }

    pub fn geometry(&self) -> &Geometry {
        &self.geo
    }

    pub fn device(&mut self) -> &mut D {
        &mut self.dev
    }

    /// The label's bytes with the padding stripped; "NO NAME" when blank.
    pub fn label(&self) -> &[u8] {
        let end = self.label.iter().rposition(|&b| b != 0x20 && b != 0).map(|p| p + 1).unwrap_or(0);
        if end == 0 { b"NO NAME" } else { &self.label[..end] }
    }

    pub fn free_clusters(&self) -> u32 {
        self.free_count
    }

    pub fn next_free_hint(&self) -> u32 {
        self.next_free
    }

    /// Whether the FSInfo's count served at mount (else the pass ran).
    pub fn hint_served(&self) -> bool {
        self.hint_served
    }

    /// The device calls the free pass made at mount (0 when the hint served).
    pub fn pass_calls(&self) -> u32 {
        self.pass_calls
    }

    pub fn cache_stats(&self) -> CacheStats {
        self.cache.stats
    }

    pub fn statfs(&self) -> Statfs {
        Statfs { block_size: self.geo.cluster_bytes, blocks: self.geo.count_of_clusters, free: self.free_count, name_max: NAME_MAX as u32 }
    }

    /// One pass over the active FAT (specification 4.3) through the caller's scratch:
    /// the free entries 2 to MAX counted and the first noted, the bytes
    /// fed one at a time so a FAT12 entry spanning two calls is whole.
    fn free_pass(&mut self, scratch: &mut [u8]) -> Result<(u32, u32), Error> {
        let per_call = core::cmp::min(BLK_MAX, (scratch.len() / SECTOR) as u32);
        if per_call == 0 {
            return Err(Error::Invalid);
        }
        let g = self.geo;
        let total: u64 = core::cmp::min(
            match g.kind {
                Kind::Fat12 => ((g.max as u64 + 1) * 3 + 1) / 2 + 1,
                Kind::Fat16 => (g.max as u64 + 1) * 2,
                Kind::Fat32 => (g.max as u64 + 1) * 4,
            },
            g.fat_sz as u64 * 512,
        );
        let base = g.rsvd + g.active_fat * g.fat_sz;
        let (mut sector, mut byte, mut count, mut first_free, mut n) = (0u32, 0u64, 0u32, 0u32, 0u32);
        let mut acc = [0u8; 4];
        let mut acc_len = 0;
        let mut emit = |n: u32, v: u32| {
            if n >= 2 && n <= g.max && v == 0 {
                count += 1;
                if first_free == 0 {
                    first_free = n;
                }
            }
        };
        while byte < total {
            let want = core::cmp::min(per_call, g.fat_sz - sector);
            let len = want as usize * SECTOR;
            dread(&mut self.dev, self.base, base + sector, &mut scratch[..len])?;
            self.pass_calls += 1;
            let used = core::cmp::min(len as u64, total - byte) as usize;
            for &b in &scratch[..used] {
                acc[acc_len] = b;
                acc_len += 1;
                match g.kind {
                    Kind::Fat16 if acc_len == 2 => {
                        emit(n, rd16(&acc, 0) as u32);
                        n += 1;
                        acc_len = 0;
                    }
                    Kind::Fat32 if acc_len == 4 => {
                        emit(n, rd32(&acc, 0) & 0x0FFF_FFFF);
                        n += 1;
                        acc_len = 0;
                    }
                    Kind::Fat12 if acc_len == 3 => {
                        emit(n, acc[0] as u32 | ((acc[1] & 0xF) as u32) << 8);
                        emit(n + 1, (acc[1] >> 4) as u32 | (acc[2] as u32) << 4);
                        n += 2;
                        acc_len = 0;
                    }
                    _ => {}
                }
            }
            byte += len as u64;
            sector += want;
        }
        Ok((count, if first_free == 0 { 2 } else { first_free }))
    }

    /// The cache: a hit; else a free buffer; else, among the
    /// non-FAT buffers first, the least recently used clean one, else the
    /// least recently used dirty one flushed first; a FAT buffer goes only
    /// when no non-FAT buffer is left.
    fn slot(&mut self, lba: u32, is_fat: bool) -> Result<usize, Error> {
        self.cache.clock = self.cache.clock.wrapping_add(1);
        let clock = self.cache.clock;
        for i in 0..CACHE_BUFS {
            if self.cache.tag[i] == lba {
                self.cache.used[i] = clock;
                self.cache.stats.hits += 1;
                return Ok(i);
            }
        }
        self.cache.stats.misses += 1;
        let mut pick = CACHE_BUFS;
        for i in 0..CACHE_BUFS {
            if self.cache.tag[i] == NO_TAG {
                pick = i;
                break;
            }
        }
        if pick == CACHE_BUFS {
            let mut best = (true, true, u32::MAX);
            for i in 0..CACHE_BUFS {
                let key = (self.cache.fat[i], self.cache.dirty[i], self.cache.used[i]);
                if key < best {
                    best = key;
                    pick = i;
                }
            }
            if self.cache.dirty[pick] {
                self.flush_slot(pick)?;
            }
            self.cache.stats.evictions += 1;
        }
        self.cache.tag[pick] = NO_TAG;
        dread(&mut self.dev, self.base, lba, &mut self.cache.bufs[pick].0)?;
        self.cache.tag[pick] = lba;
        self.cache.dirty[pick] = false;
        self.cache.fat[pick] = is_fat;
        self.cache.used[pick] = clock;
        Ok(pick)
    }

    /// A dirty buffer to the device: a FAT sector to every copy when
    /// mirroring is on (the first copy first), to the active copy alone when
    /// it is off; any other sector to its place.
    fn flush_slot(&mut self, i: usize) -> Result<(), Error> {
        if !self.cache.dirty[i] || self.cache.tag[i] == NO_TAG {
            return Ok(());
        }
        let lba = self.cache.tag[i];
        if self.cache.fat[i] {
            let g = self.geo;
            let within = lba - (g.rsvd + g.active_fat * g.fat_sz);
            if g.mirroring {
                for c in 0..g.num_fats {
                    dwrite(&mut self.dev, self.base, g.rsvd + c * g.fat_sz + within, &self.cache.bufs[i].0)?;
                }
            } else {
                dwrite(&mut self.dev, self.base, lba, &self.cache.bufs[i].0)?;
            }
        } else {
            dwrite(&mut self.dev, self.base, lba, &self.cache.bufs[i].0)?;
        }
        self.cache.dirty[i] = false;
        self.cache.stats.flushes += 1;
        Ok(())
    }

    /// Every dirty buffer written in the write side's order, then the device synced.
    pub fn flush(&mut self) -> Result<(), Error> {
        self.flush_buffers()?;
        self.dev.sync()
    }

    /// The volume's sync: the buffers, then the FSInfo's two fields when they changed, then the device.
    pub fn sync(&mut self) -> Result<(), Error> {
        self.flush_buffers()?;
        self.fsinfo_sync()?;
        self.dev.sync()
    }

    /// The unmount: the sync, the device given back.
    pub fn unmount(mut self) -> Result<D, Error> {
        self.sync()?;
        Ok(self.dev)
    }

    fn fat_place(&self, n: u32) -> (u32, usize) {
        let g = self.geo;
        let off = match g.kind {
            Kind::Fat12 => n + n / 2,
            Kind::Fat16 => n * 2,
            Kind::Fat32 => n * 4,
        };
        (g.rsvd + g.active_fat * g.fat_sz + off / 512, (off % 512) as usize)
    }

    /// The FAT entry of cluster N from the active copy (specification 4.1, its arithmetic
    /// verbatim): FAT12's odd entry the word's high twelve bits and the even
    /// one its low twelve, the sector-spanning entry read as two bytes.
    pub fn fat_get(&mut self, n: u32) -> Result<u32, Error> {
        if n > self.geo.max {
            return Err(Error::Io);
        }
        let (s, o) = self.fat_place(n);
        let i = self.slot(s, true)?;
        match self.geo.kind {
            Kind::Fat12 => {
                let b0 = self.cache.bufs[i].0[o];
                let b1 = if o == SECTOR - 1 {
                    let j = self.slot(s + 1, true)?;
                    self.cache.bufs[j].0[0]
                } else {
                    self.cache.bufs[i].0[o + 1]
                };
                let v = b0 as u32 | (b1 as u32) << 8;
                Ok(if n & 1 == 1 { v >> 4 } else { v & 0xFFF })
            }
            Kind::Fat16 => Ok(rd16(&self.cache.bufs[i].0, o) as u32),
            Kind::Fat32 => Ok(rd32(&self.cache.bufs[i].0, o) & 0x0FFF_FFFF),
        }
    }

    /// The FAT entry of cluster N written into the cache (the active copy;
    /// every copy at the flush when mirroring is on): FAT32's high nibble
    /// preserved, FAT12's neighbour nibble preserved, FAT[0] and FAT[1]
    /// never written.
    pub fn fat_set(&mut self, n: u32, v: u32) -> Result<(), Error> {
        if n < 2 || n > self.geo.max {
            return Err(Error::Invalid);
        }
        let (s, o) = self.fat_place(n);
        let i = self.slot(s, true)?;
        match self.geo.kind {
            Kind::Fat12 => {
                let b = &mut self.cache.bufs[i].0;
                let (lo, hi) = if n & 1 == 1 { ((b[o] & 0x0F) | ((v << 4) & 0xF0) as u8, (v >> 4) as u8) } else { (v as u8, 0) };
                b[o] = lo;
                self.mark_dirty(i);
                let j = if o == SECTOR - 1 { self.slot(s + 1, true)? } else { i };
                let p = if o == SECTOR - 1 { 0 } else { o + 1 };
                let b = &mut self.cache.bufs[j].0;
                b[p] = if n & 1 == 1 { hi } else { (b[p] & 0xF0) | ((v >> 8) & 0x0F) as u8 };
                self.mark_dirty(j);
            }
            Kind::Fat16 => {
                self.cache.bufs[i].0[o..o + 2].copy_from_slice(&(v as u16).to_le_bytes());
                self.mark_dirty(i);
            }
            Kind::Fat32 => {
                let old = rd32(&self.cache.bufs[i].0, o);
                self.cache.bufs[i].0[o..o + 4].copy_from_slice(&((old & 0xF000_0000) | (v & 0x0FFF_FFFF)).to_le_bytes());
                self.mark_dirty(i);
            }
        }
        Ok(())
    }

    /// The end-of-chain mark of the volume's width.
    pub fn eoc(&self) -> u32 {
        self.geo.eoc_mark()
    }

    /// The next cluster of a chain: None at the end (every value past MAX),
    /// Io for a free or reserved entry inside a chain.
    pub fn next_cluster(&mut self, c: u32) -> Result<Option<u32>, Error> {
        let v = self.fat_get(c)?;
        if v > self.geo.max {
            return Ok(None);
        }
        if v < 2 {
            return Err(Error::Io);
        }
        Ok(Some(v))
    }

    /// A chain's length in clusters, the walk bounded by the count of
    /// clusters and refused Io past it or on a step back to the first (a
    /// looping chain cannot hang the kernel).
    pub fn chain_length(&mut self, first: u32) -> Result<u32, Error> {
        if first == 0 {
            return Ok(0);
        }
        let mut c = first;
        let mut n = 1;
        while let Some(next) = self.next_cluster(c)? {
            n += 1;
            if next == first || n > self.geo.count_of_clusters {
                return Err(Error::Io);
            }
            c = next;
        }
        Ok(n)
    }

    /// The handle's cluster at the index, walked forward from the one
    /// remembered (restarted from the first when seeking back).
    fn seek_cluster(&mut self, h: &mut Handle, idx: u32) -> Result<u32, Error> {
        if idx < h.cur_idx || h.cur == 0 {
            h.cur = h.first;
            h.cur_idx = 0;
        }
        if h.cur == 0 {
            return Err(Error::Io);
        }
        while h.cur_idx < idx {
            match self.next_cluster(h.cur)? {
                Some(n) if n != h.first => {
                    h.cur = n;
                    h.cur_idx += 1;
                }
                _ => return Err(Error::Io),
            }
        }
        Ok(h.cur)
    }

    /// The sector and offset of a directory's raw entry index: the fixed
    /// root region on FAT12 and FAT16, else the chain; None past the end.
    fn entry_place(&mut self, d: &mut Dir, raw: u32) -> Result<Option<(u32, usize)>, Error> {
        if raw >= DIR_MAX_ENTRIES {
            return Ok(None);
        }
        let g = self.geo;
        if g.kind != Kind::Fat32 && g.is_root_cluster(d.first) {
            let sec = raw / 16;
            if sec >= g.root_dir_sectors {
                return Ok(None);
            }
            return Ok(Some((g.first_root_sector + sec, (raw % 16) as usize * 32)));
        }
        let first = if d.first == 0 { g.root_clus } else { d.first };
        let epc = g.sec_per_clus * 16;
        let cidx = raw / epc;
        if cidx < d.cur_idx || d.cur == 0 {
            d.cur = first;
            d.cur_idx = 0;
        }
        while d.cur_idx < cidx {
            match self.next_cluster(d.cur)? {
                Some(n) if n != first => {
                    d.cur = n;
                    d.cur_idx += 1;
                }
                Some(_) => return Err(Error::Io),
                None => return Ok(None),
            }
        }
        let within = raw % epc;
        Ok(Some((g.first_sector_of(d.cur) + within / 16, (within % 16) as usize * 32)))
    }

    /// The next short entry of the directory with its long set assembled in
    /// `long` (the specification's section 7 checks; a failed set dropped), the free entries
    /// and the volume label skipped, the end at the first 0x00 byte.
    fn next_entry(&mut self, d: &mut Dir, out: &mut Entry, long: &mut LongBuf) -> Result<bool, Error> {
        long.reset();
        loop {
            let Some((lba, off)) = self.entry_place(d, d.index)? else {
                return Ok(false);
            };
            let i = self.slot(lba, false)?;
            let mut e = [0u8; 32];
            e.copy_from_slice(&self.cache.bufs[i].0[off..off + 32]);
            d.index += 1;
            match e[0] {
                0 => return Ok(false),
                0xE5 => {
                    long.reset();
                    continue;
                }
                _ => {}
            }
            if e[11] & ATTR_LONG_MASK == ATTR_LONG_NAME {
                long.feed(&e, lba, off as u16);
                continue;
            }
            if e[11] & ATTR_VOLUME_ID != 0 {
                long.reset();
                continue;
            }
            *out = Entry::EMPTY;
            out.short.copy_from_slice(&e[..11]);
            out.attr = e[11];
            out.ntres = e[12];
            out.crt_tenth = e[13];
            out.crt_time = rd16(&e, 14);
            out.crt_date = rd16(&e, 16);
            out.acc_date = rd16(&e, 18);
            out.wrt_time = rd16(&e, 22);
            out.wrt_date = rd16(&e, 24);
            out.first = if self.geo.kind == Kind::Fat32 { (rd16(&e, 20) as u32) << 16 | rd16(&e, 26) as u32 } else { rd16(&e, 26) as u32 };
            out.size = rd32(&e, 28);
            out.place = (lba, off as u16);
            out.long = long.complete(checksum(&e[..11]));
            out.set_start = if out.long { long.start } else { out.place };
            out.index = d.index - 1;
            out.set_len = if out.long { long.n } else { 0 };
            out.dir_first = d.first;
            return Ok(true);
        }
    }

    fn fill_name(out: &mut Entry, long: &LongBuf) {
        let mut name = [0u8; 256];
        let len = if out.long { utf16_to_utf8(long.name(), &mut name) } else { None };
        match len {
            Some(n) if n > 0 => {
                out.name = name;
                out.name_len = n as u16;
            }
            _ => {
                out.long = false;
                let n = short_name_utf8(&out.short, out.ntres, &mut name);
                out.name = name;
                out.name_len = n as u16;
            }
        }
    }

    /// The directory's entry matching the name (specification 7.4: case-insensitive under
    /// the fold; the long name or the short name's BASE.EXT form).
    fn find(&mut self, dir_first: u32, want: &[u16]) -> Result<Option<Entry>, Error> {
        let mut d = Dir { first: dir_first, index: 0, cur: 0, cur_idx: 0, root: false, open: true };
        let mut e = Entry::EMPTY;
        let mut long = LongBuf::new();
        while self.next_entry(&mut d, &mut e, &mut long)? {
            if e.long && eq_fold(long.name(), want) {
                Self::fill_name(&mut e, &long);
                return Ok(Some(e));
            }
            let mut su = [0u16; 13];
            let n = short_units(&e.short, &mut su);
            if eq_fold(&su[..n], want) {
                Self::fill_name(&mut e, &long);
                return Ok(Some(e));
            }
        }
        Ok(None)
    }

    /// The parent of a directory through its ".." entry (0 is the root).
    fn parent_of(&mut self, dir_first: u32) -> Result<u32, Error> {
        let mut d = Dir { first: dir_first, index: 0, cur: 0, cur_idx: 0, root: false, open: true };
        let mut e = Entry::EMPTY;
        let mut long = LongBuf::new();
        while self.next_entry(&mut d, &mut e, &mut long)? {
            if &e.short == b"..         " {
                return Ok(e.first);
            }
            if d.index > 2 {
                break;
            }
        }
        Err(Error::Io)
    }

    fn root_entry(&self) -> Entry {
        let mut e = Entry::EMPTY;
        e.attr = ATTR_DIRECTORY;
        e.first = self.geo.root_clus;
        e.set_name(b"/");
        e
    }

    /// A path resolved: absolute inside the volume, split on '/'
    /// alone, "." and ".." by the walk (".." of the root the root), a
    /// middle component that is a file NotDir, a component past 255 units
    /// NameTooLong.
    fn resolve(&mut self, path: &[u8]) -> Result<Located, Error> {
        if path.first() != Some(&b'/') {
            return Err(Error::Invalid);
        }
        let mut cur = Located { entry: self.root_entry(), is_root: true };
        for comp in path.split(|&b| b == b'/') {
            if comp.is_empty() || comp == b"." {
                continue;
            }
            if !cur.entry.is_dir() {
                return Err(Error::NotDir);
            }
            if comp == b".." {
                if cur.is_root {
                    continue;
                }
                let parent = self.parent_of(cur.entry.first)?;
                if self.geo.is_root_cluster(parent) {
                    cur = Located { entry: self.root_entry(), is_root: true };
                } else {
                    let mut e = Entry::EMPTY;
                    e.attr = ATTR_DIRECTORY;
                    e.first = parent;
                    cur = Located { entry: e, is_root: false };
                }
                continue;
            }
            let mut want = [0u16; 256];
            let n = decode_name(comp, &mut want)?;
            let dir_first = if cur.is_root { self.geo.root_clus } else { cur.entry.first };
            match self.find(dir_first, &want[..n])? {
                Some(e) => cur = Located { entry: e, is_root: false },
                None => return Err(Error::NotFound),
            }
        }
        Ok(cur)
    }

    fn stat_of(e: &Entry) -> Stat {
        Stat {
            size: e.size,
            is_dir: e.is_dir(),
            read_only: e.read_only(),
            attr: e.attr,
            first: e.first,
            mtime: unix_from_packed(e.wrt_date, e.wrt_time),
            ctime: unix_from_packed(e.crt_date, e.crt_time),
            atime: unix_from_packed(e.acc_date, 0),
        }
    }

    pub fn stat(&mut self, path: &[u8]) -> Result<Stat, Error> {
        let l = self.resolve(path)?;
        Ok(Self::stat_of(&l.entry))
    }

    /// The file opened with the flags of the specification's 5.6: O_READ and O_WRITE the access;
    /// O_CREATE makes a missing last component (its entry with the ARCHIVE
    /// bit and the times now); O_EXCL with O_CREATE refuses an existing name
    /// Exists; O_TRUNC frees the chain; O_APPEND seeks to the end before each
    /// write (the last two need O_WRITE); a directory IsDir, a read-only file
    /// opened for writing ReadOnly.
    pub fn open(&mut self, path: &[u8], flags: u32) -> Result<Handle, Error> {
        if flags & (O_READ | O_WRITE) == 0 || (flags & (O_TRUNC | O_APPEND) != 0 && flags & O_WRITE == 0) {
            return Err(Error::Invalid);
        }
        let (parent, units, n) = self.split_parent(path)?;
        if n == 0 {
            return Err(Error::IsDir);
        }
        let dir_first = self.dir_first_of(&parent);
        let e = match self.find(dir_first, &units[..n])? {
            Some(e) => {
                if flags & (O_CREATE | O_EXCL) == O_CREATE | O_EXCL {
                    return Err(Error::Exists);
                }
                if e.is_dir() {
                    return Err(Error::IsDir);
                }
                if flags & O_WRITE != 0 && e.read_only() {
                    return Err(Error::ReadOnly);
                }
                e
            }
            None => {
                if flags & O_CREATE == 0 {
                    return Err(Error::NotFound);
                }
                let (d, t) = self.stamp_now();
                self.create_entry(dir_first, &units[..n], ATTR_ARCHIVE, 0, 0, [t, d, d, t, d], 0)?
            }
        };
        let mut h = Handle { first: e.first, size: e.size, pos: 0, cur: e.first, cur_idx: 0, attr: e.attr, place: e.place, times: [e.crt_time, e.crt_date, e.acc_date, e.wrt_time, e.wrt_date], crt_tenth: e.crt_tenth, flags, open: true, dirty: false };
        if flags & O_TRUNC != 0 && (h.size != 0 || h.first != 0) {
            self.truncate(&mut h, 0)?;
        }
        Ok(h)
    }

    pub fn fstat(&mut self, h: &Handle) -> Result<Stat, Error> {
        if !h.open {
            return Err(Error::BadHandle);
        }
        Ok(Stat {
            size: h.size,
            is_dir: false,
            read_only: h.attr & ATTR_READ_ONLY != 0,
            attr: h.attr,
            first: h.first,
            mtime: unix_from_packed(h.times[4], h.times[3]),
            ctime: unix_from_packed(h.times[1], h.times[0]),
            atime: unix_from_packed(h.times[2], 0),
        })
    }

    /// The read: whole sectors of one run straight into the
    /// caller's 4-byte-aligned buffer in one device call up to BLK_MAX,
    /// adjacent clusters joined; a partial or misaligned piece through the
    /// cache with a copy; 0 at the end.
    pub fn read(&mut self, h: &mut Handle, buf: &mut [u8]) -> Result<usize, Error> {
        if !h.open || h.flags & O_READ == 0 {
            return Err(Error::BadHandle);
        }
        let g = self.geo;
        let mut done = 0;
        while done < buf.len() && h.pos < h.size {
            let cidx = h.pos / g.cluster_bytes;
            let within = h.pos % g.cluster_bytes;
            let c = self.seek_cluster(h, cidx)?;
            let remaining = core::cmp::min(buf.len() - done, (h.size - h.pos) as usize);
            let sector_in_cluster = within / 512;
            let in_sector = (within % 512) as usize;
            let lba = g.first_sector_of(c) + sector_in_cluster;
            if in_sector == 0 && remaining >= SECTOR && (buf[done..].as_ptr() as usize) % 4 == 0 {
                let want = core::cmp::min((remaining / SECTOR) as u32, BLK_MAX);
                let mut avail = g.sec_per_clus - sector_in_cluster;
                let mut last = c;
                while avail < want {
                    match self.next_cluster(last)? {
                        Some(n) if n == last + 1 => {
                            avail += g.sec_per_clus;
                            last = n;
                        }
                        _ => break,
                    }
                }
                let n = core::cmp::min(want, avail) as usize * SECTOR;
                self.flush_range(lba, (n / SECTOR) as u32)?;
                dread(&mut self.dev, self.base, lba, &mut buf[done..done + n])?;
                h.pos += n as u32;
                done += n;
            } else {
                let i = self.slot(lba, false)?;
                let n = core::cmp::min(remaining, SECTOR - in_sector);
                buf[done..done + n].copy_from_slice(&self.cache.bufs[i].0[in_sector..in_sector + n]);
                h.pos += n as u32;
                done += n;
            }
        }
        Ok(done)
    }

    /// The seek: 64-bit at the door, a position past 0xFFFFFFFF or below 0
    /// Invalid, past the end allowed (the gap the write side's).
    pub fn seek(&mut self, h: &mut Handle, offset: i64, whence: u32) -> Result<u64, Error> {
        if !h.open {
            return Err(Error::BadHandle);
        }
        let base = match whence {
            SEEK_SET => 0,
            SEEK_CUR => h.pos as i64,
            SEEK_END => h.size as i64,
            _ => return Err(Error::Invalid),
        };
        let new = base + offset;
        if new < 0 || new > 0xFFFF_FFFF {
            return Err(Error::Invalid);
        }
        h.pos = new as u32;
        Ok(new as u64)
    }

    /// The close: the file's sync when the directory is owed (a write, a
    /// truncate), the handle freed.
    pub fn close(&mut self, h: &mut Handle) -> Result<(), Error> {
        if !h.open {
            return Err(Error::BadHandle);
        }
        let r = if h.dirty { self.sync_file(h) } else { Ok(()) };
        h.open = false;
        r
    }

    pub fn opendir(&mut self, path: &[u8]) -> Result<Dir, Error> {
        let l = self.resolve(path)?;
        if !l.entry.is_dir() {
            return Err(Error::NotDir);
        }
        let root = l.is_root || self.geo.is_root_cluster(l.entry.first);
        Ok(Dir { first: if root { self.geo.root_clus } else { l.entry.first }, index: 0, cur: 0, cur_idx: 0, root, open: true })
    }

    /// One entry per call, the name as the walk serves it; "." and ".." as
    /// Linux serves them (the root's synthesized); false at the end.
    pub fn readdir(&mut self, d: &mut Dir, out: &mut Entry) -> Result<bool, Error> {
        if !d.open {
            return Err(Error::BadHandle);
        }
        if d.root && d.index < 2 {
            *out = self.root_entry();
            out.set_name(if d.index == 0 { b"." } else { b".." });
            d.index += 1;
            return Ok(true);
        }
        let mut long = LongBuf::new();
        let saved = d.index;
        d.index = if d.root { saved - 2 } else { saved };
        let r = self.next_entry(d, out, &mut long);
        if d.root {
            d.index += 2;
        }
        if r? {
            Self::fill_name(out, &long);
            return Ok(true);
        }
        Ok(false)
    }

    pub fn closedir(&mut self, d: &mut Dir) -> Result<(), Error> {
        if !d.open {
            return Err(Error::BadHandle);
        }
        d.open = false;
        Ok(())
    }

    // ---------------- THE WRITE SIDE ----------------

    /// The clock seam: the kernel's adapter gives the calendar's reading,
    /// the host tier its own; None is the FAT floor.
    pub fn set_clock(&mut self, now: fn() -> Option<Civil>) {
        self.now = now;
    }

    /// The packed date and time now (specification 6.3): the day, the month and the years
    /// since 1980; the hours, the minutes and the two-second units; outside
    /// 1980 to 2107, or with no calendar set, the floor, midnight of 1 January 1980.
    fn stamp_now(&self) -> (u16, u16) {
        match (self.now)() {
            Some(c) if (1980..=2107).contains(&c.year) && (1..=12).contains(&c.month) && (1..=31).contains(&c.day) => (
                (((c.year - 1980) << 9) | (c.month << 5) | c.day) as u16,
                ((c.hour.min(23) << 11) | (c.minute.min(59) << 5) | (c.second.min(59) / 2)) as u16,
            ),
            _ => (FLOOR_DATE, 0),
        }
    }

    /// A buffer marked dirty with its place in the flush order (the sequence
    /// number of its first dirtying).
    fn mark_dirty(&mut self, i: usize) {
        if !self.cache.dirty[i] {
            self.cache.dirty[i] = true;
            self.cache.seq = self.cache.seq.wrapping_add(1);
            self.cache.stamp[i] = self.cache.seq;
        }
    }

    /// The dirty buffers written in order: the FAT's by their dirtying
    /// sequence first (an END mark before the directory entry that points
    /// at its cluster), then the directories' and the data's by theirs.
    fn flush_buffers(&mut self) -> Result<(), Error> {
        for pass in [true, false] {
            loop {
                let mut pick = CACHE_BUFS;
                let mut best = u32::MAX;
                for i in 0..CACHE_BUFS {
                    if self.cache.dirty[i] && self.cache.fat[i] == pass && self.cache.stamp[i] < best {
                        best = self.cache.stamp[i];
                        pick = i;
                    }
                }
                if pick == CACHE_BUFS {
                    break;
                }
                self.flush_slot(pick)?;
            }
        }
        Ok(())
    }

    /// The dirty buffers inside a sector range written now (a direct read
    /// of data a partial write left in the cache).
    fn flush_range(&mut self, lba: u32, n: u32) -> Result<(), Error> {
        for i in 0..CACHE_BUFS {
            let t = self.cache.tag[i];
            if t != NO_TAG && t >= lba && t - lba < n && self.cache.dirty[i] {
                self.flush_slot(i)?;
            }
        }
        Ok(())
    }

    /// The cache's copies of sectors about to be written straight to the
    /// device dropped (a dirty copy is superseded by the whole sector).
    fn drop_cached(&mut self, lba: u32, n: u32) {
        for i in 0..CACHE_BUFS {
            let t = self.cache.tag[i];
            if t != NO_TAG && t >= lba && t - lba < n {
                self.cache.tag[i] = NO_TAG;
                self.cache.dirty[i] = false;
            }
        }
    }

    /// One sector's dirty buffer written now (the write orders
    /// state: an entry set before its chain is freed, an END before its link).
    fn flush_lba(&mut self, lba: u32) -> Result<(), Error> {
        for i in 0..CACHE_BUFS {
            if self.cache.tag[i] == lba && self.cache.dirty[i] {
                return self.flush_slot(i);
            }
        }
        Ok(())
    }

    /// The FAT sector holding cluster N's entry written now (FAT12's
    /// spanning entry: both).
    fn fat_flush_entry(&mut self, n: u32) -> Result<(), Error> {
        let (s, o) = self.fat_place(n);
        self.flush_lba(s)?;
        if self.geo.kind == Kind::Fat12 && o == SECTOR - 1 {
            self.flush_lba(s + 1)?;
        }
        Ok(())
    }

    /// Whole sectors zeroed straight to the device (a new directory
    /// cluster's content), the cache's copies dropped.
    fn zero_sectors(&mut self, lba: u32, n: u32) -> Result<(), Error> {
        self.drop_cached(lba, n);
        let z = Sector([0; SECTOR]);
        for k in 0..n {
            dwrite(&mut self.dev, self.base, lba + k, &z.0)?;
        }
        Ok(())
    }

    fn zero_cluster(&mut self, c: u32) -> Result<(), Error> {
        let g = self.geo;
        self.zero_sectors(g.first_sector_of(c), g.sec_per_clus)
    }

    /// One cluster allocated: the free search from the hint
    /// forward with one wrap; the entry written END FIRST (and put on the
    /// device before the link when the predecessor's entry lives in another
    /// FAT sector), THEN linked from the predecessor (0: the caller's
    /// directory entry carries the first cluster); the caller fills it. A
    /// bad-cluster mark is never free. NoSpace when nothing is.
    fn alloc_cluster(&mut self, prev: u32) -> Result<u32, Error> {
        let g = self.geo;
        if self.free_count == 0 {
            return Err(Error::NoSpace);
        }
        let start = if (2..=g.max).contains(&self.next_free) { self.next_free } else { 2 };
        let mut n = start;
        let found = loop {
            if self.fat_get(n)? == 0 {
                break n;
            }
            n = if n == g.max { 2 } else { n + 1 };
            if n == start {
                self.free_count = 0;
                return Err(Error::NoSpace);
            }
        };
        self.fat_set(found, g.eoc_mark())?;
        if prev != 0 {
            if self.fat_place(prev).0 != self.fat_place(found).0 {
                self.fat_flush_entry(found)?;
            }
            self.fat_set(prev, found)?;
        }
        self.free_count -= 1;
        self.next_free = if found == g.max { 2 } else { found + 1 };
        Ok(found)
    }

    /// A chain freed from `first` forward, the mirror of the allocation
    /// (the caller has written the kept tail's END or zeroed the entry's
    /// first cluster first): every entry zeroed, the count and the hint kept.
    fn free_chain(&mut self, first: u32) -> Result<(), Error> {
        let g = self.geo;
        let mut c = first;
        let mut n = 0;
        while (2..=g.max).contains(&c) {
            let next = self.fat_get(c)?;
            self.fat_set(c, 0)?;
            self.free_count += 1;
            if c < self.next_free {
                self.next_free = c;
            }
            n += 1;
            if n > g.count_of_clusters {
                return Err(Error::Io);
            }
            c = next;
        }
        Ok(())
    }

    /// The handle's cluster at the index, allocated END-first as the chain
    /// runs out (the write's path).
    fn cluster_for_write(&mut self, h: &mut Handle, cidx: u32) -> Result<u32, Error> {
        let g = self.geo;
        if h.first == 0 {
            let c = self.alloc_cluster(0)?;
            h.first = c;
            h.cur = c;
            h.cur_idx = 0;
            h.dirty = true;
        }
        if cidx < h.cur_idx || h.cur == 0 {
            h.cur = h.first;
            h.cur_idx = 0;
        }
        while h.cur_idx < cidx {
            if h.cur_idx >= g.count_of_clusters {
                return Err(Error::Io);
            }
            match self.next_cluster(h.cur)? {
                Some(n) if n != h.first => {
                    h.cur = n;
                    h.cur_idx += 1;
                }
                Some(_) => return Err(Error::Io),
                None => {
                    let c = self.alloc_cluster(h.cur)?;
                    h.cur = c;
                    h.cur_idx += 1;
                }
            }
        }
        Ok(h.cur)
    }

    /// The handle's write time now, the access date equal to it, the
    /// ARCHIVE bit set, the directory entry owed.
    fn touch(&mut self, h: &mut Handle) {
        let (d, t) = self.stamp_now();
        h.times[2] = d;
        h.times[3] = t;
        h.times[4] = d;
        h.attr |= ATTR_ARCHIVE;
        h.dirty = true;
    }

    /// The write: a handle opened for writing; APPEND
    /// seeks to the end first; a position past the end allocates and zeroes
    /// the gap; whole aligned sectors of one run go straight to the device
    /// in one call up to BLK_MAX (the run extended cluster by cluster,
    /// allocating as the chain ends while the hint runs on), the rest
    /// through the cache; the bytes written before the volume filled are
    /// counted, then NoSpace on the next call; the entry's size and times
    /// reach the directory at sync and close.
    pub fn write(&mut self, h: &mut Handle, data: &[u8]) -> Result<usize, Error> {
        if !h.open || h.flags & O_WRITE == 0 {
            return Err(Error::BadHandle);
        }
        if data.is_empty() {
            return Ok(0);
        }
        if h.flags & O_APPEND != 0 {
            h.pos = h.size;
        }
        if h.pos > h.size {
            self.extend_to(h, h.pos)?;
        }
        let g = self.geo;
        let cb = g.cluster_bytes;
        let room = (0xFFFF_FFFFu32 - h.pos) as usize;
        let data = &data[..data.len().min(room)];
        // write(2)'s rule: a refusal after bytes have
        // moved returns their count -- the handle's position and size already
        // cover them -- and the refusal itself comes back at the next call, which
        // moves nothing; a refusal before any byte moved is returned at once.
        let mut done = 0;
        'chunks: while done < data.len() {
            let cidx = h.pos / cb;
            let within = h.pos % cb;
            let c = match self.cluster_for_write(h, cidx) {
                Ok(c) => c,
                Err(Error::NoSpace) => break,
                Err(_) if done > 0 => break 'chunks,
                Err(e) => return Err(e),
            };
            let remaining = data.len() - done;
            let sic = within / 512;
            let in_sector = (within % 512) as usize;
            let lba = g.first_sector_of(c) + sic;
            let n = if in_sector == 0 && remaining >= SECTOR && (data[done..].as_ptr() as usize) % 4 == 0 {
                let want = ((remaining / SECTOR) as u32).min(BLK_MAX);
                let mut avail = g.sec_per_clus - sic;
                let (mut last, mut idx) = (c, cidx);
                while avail < want {
                    let next = match self.next_cluster(last) {
                        Ok(Some(n)) => Some(n),
                        Ok(None) => match self.alloc_cluster(last) {
                            Ok(n) => Some(n),
                            Err(Error::NoSpace) => None,
                            Err(_) if done > 0 => break 'chunks,
                            Err(e) => return Err(e),
                        },
                        Err(_) if done > 0 => break 'chunks,
                        Err(e) => return Err(e),
                    };
                    match next {
                        Some(n) if n == last + 1 => {
                            avail += g.sec_per_clus;
                            last = n;
                            idx += 1;
                        }
                        _ => break,
                    }
                }
                let n = want.min(avail) as usize * SECTOR;
                self.drop_cached(lba, (n / SECTOR) as u32);
                match dwrite(&mut self.dev, self.base, lba, &data[done..done + n]) {
                    Ok(()) => {}
                    Err(_) if done > 0 => break 'chunks,
                    Err(e) => return Err(e),
                }
                h.cur = last;
                h.cur_idx = idx;
                n
            } else {
                let i = match self.slot(lba, false) {
                    Ok(i) => i,
                    Err(_) if done > 0 => break 'chunks,
                    Err(e) => return Err(e),
                };
                let n = remaining.min(SECTOR - in_sector);
                self.cache.bufs[i].0[in_sector..in_sector + n].copy_from_slice(&data[done..done + n]);
                self.mark_dirty(i);
                n
            };
            h.pos += n as u32;
            done += n;
            if h.pos > h.size {
                h.size = h.pos;
            }
        }
        if done == 0 {
            return Err(Error::NoSpace);
        }
        self.touch(h);
        Ok(done)
    }

    /// The file grown to `len` with zeros: the bytes from the old end
    /// onward written as zeros through the write's own paths (the skipped
    /// clusters allocated END-first and filled), the position restored.
    fn extend_to(&mut self, h: &mut Handle, len: u32) -> Result<(), Error> {
        let z = Sector([0; SECTOR]);
        let saved = h.pos;
        h.pos = h.size;
        let r = loop {
            if h.size >= len {
                break Ok(());
            }
            let n = ((len - h.size) as usize).min(SECTOR);
            if let Err(e) = self.write(h, &z.0[..n]) {
                break Err(e);
            }
        };
        h.pos = saved;
        r
    }

    /// The truncate: longer zero-fills; shorter writes the kept
    /// tail's END first, on the device, then frees the cut (the mirror of
    /// the allocation); to zero zeroes the entry's first cluster first, on
    /// the device, then frees the whole chain.
    pub fn truncate(&mut self, h: &mut Handle, len: u32) -> Result<(), Error> {
        if !h.open || h.flags & O_WRITE == 0 {
            return Err(Error::BadHandle);
        }
        if len > h.size {
            return self.extend_to(h, len);
        }
        if len == h.size {
            return Ok(());
        }
        let g = self.geo;
        let keep = len.div_ceil(g.cluster_bytes);
        if keep == 0 {
            let first = h.first;
            h.first = 0;
            h.size = 0;
            h.cur = 0;
            h.cur_idx = 0;
            self.touch(h);
            self.write_entry(h)?;
            self.flush_lba(h.place.0)?;
            self.free_chain(first)?;
        } else {
            let tail = self.cluster_for_write(h, keep - 1)?;
            let next = self.next_cluster(tail)?;
            self.fat_set(tail, g.eoc_mark())?;
            self.fat_flush_entry(tail)?;
            if let Some(n) = next {
                self.free_chain(n)?;
            }
            h.size = len;
            self.touch(h);
        }
        Ok(())
    }

    /// The directory entry's fields written through the cache: the size,
    /// the first cluster, the attributes, the times.
    fn write_entry(&mut self, h: &mut Handle) -> Result<(), Error> {
        let (lba, off) = h.place;
        let off = off as usize;
        let i = self.slot(lba, false)?;
        let hi = if self.geo.kind == Kind::Fat32 { (h.first >> 16) as u16 } else { 0 };
        let e = &mut self.cache.bufs[i].0[off..off + 32];
        e[11] = h.attr;
        e[13] = h.crt_tenth;
        e[14..16].copy_from_slice(&h.times[0].to_le_bytes());
        e[16..18].copy_from_slice(&h.times[1].to_le_bytes());
        e[18..20].copy_from_slice(&h.times[2].to_le_bytes());
        e[20..22].copy_from_slice(&hi.to_le_bytes());
        e[22..24].copy_from_slice(&h.times[3].to_le_bytes());
        e[24..26].copy_from_slice(&h.times[4].to_le_bytes());
        e[26..28].copy_from_slice(&(h.first as u16).to_le_bytes());
        e[28..32].copy_from_slice(&h.size.to_le_bytes());
        self.mark_dirty(i);
        h.dirty = false;
        Ok(())
    }

    /// The file's sync: its entry when owed, the buffers (the FAT copies
    /// first), the FSInfo, the device.
    pub fn sync_file(&mut self, h: &mut Handle) -> Result<(), Error> {
        if !h.open {
            return Err(Error::BadHandle);
        }
        if h.dirty {
            self.write_entry(h)?;
        }
        self.sync()
    }

    /// The FSInfo's two fields written when they changed since they were
    /// read or last written (FAT32 with a valid FSInfo alone; the sector's
    /// other bytes as read): the one write outside the FAT, the directories
    /// and the data.
    fn fsinfo_sync(&mut self) -> Result<(), Error> {
        let g = self.geo;
        if g.kind != Kind::Fat32 || g.fsinfo == 0 || (self.free_count, self.next_free) == self.fsinfo_seen {
            return Ok(());
        }
        let mut s = Sector([0; SECTOR]);
        dread(&mut self.dev, self.base, g.fsinfo, &mut s.0)?;
        s.0[488..492].copy_from_slice(&self.free_count.to_le_bytes());
        s.0[492..496].copy_from_slice(&self.next_free.to_le_bytes());
        dwrite(&mut self.dev, self.base, g.fsinfo, &s.0)?;
        self.fsinfo_seen = (self.free_count, self.next_free);
        Ok(())
    }

    /// A path's parent directory and last component: the parent resolved
    /// (a directory), the component's units; a path ending in the root, "."
    /// or ".." resolves whole with no component (0 units).
    fn split_parent(&mut self, path: &[u8]) -> Result<(Located, [u16; 256], usize), Error> {
        if path.first() != Some(&b'/') {
            return Err(Error::Invalid);
        }
        let mut end = path.len();
        while end > 1 && path[end - 1] == b'/' {
            end -= 1;
        }
        let p = &path[..end];
        let cut = p.iter().rposition(|&b| b == b'/').unwrap_or(0);
        let last = &p[cut + 1..];
        let mut units = [0u16; 256];
        if last.is_empty() || last == b"." || last == b".." {
            let l = self.resolve(p)?;
            return Ok((l, units, 0));
        }
        let n = decode_name(last, &mut units)?;
        let parent = self.resolve(if cut == 0 { b"/" } else { &p[..cut] })?;
        if !parent.entry.is_dir() {
            return Err(Error::NotDir);
        }
        Ok((parent, units, n))
    }

    fn dir_first_of(&self, l: &Located) -> u32 {
        if l.is_root { self.geo.root_clus } else { l.entry.first }
    }

    /// One raw entry written through the cache at the directory's index; its place.
    fn put_raw(&mut self, d: &mut Dir, idx: u32, e: &[u8; 32]) -> Result<(u32, u16), Error> {
        let Some((lba, off)) = self.entry_place(d, idx)? else {
            return Err(Error::Io);
        };
        let i = self.slot(lba, false)?;
        self.cache.bufs[i].0[off..off + 32].copy_from_slice(e);
        self.mark_dirty(i);
        Ok((lba, off as u16))
    }

    /// The first run of `need` free entries (0xE5 or 0x00) in the
    /// directory, as the raw index of its first entry; the directory grown
    /// by one zeroed cluster, linked END-first, when its entries run out
    /// (the fixed root NoSpace; the specification's 6.4 size bound NoSpace).
    fn find_free_run(&mut self, dir_first: u32, need: u32) -> Result<u32, Error> {
        let g = self.geo;
        let mut d = Dir { first: dir_first, index: 0, cur: 0, cur_idx: 0, root: false, open: true };
        let (mut run_start, mut run_len, mut idx) = (0u32, 0u32, 0u32);
        loop {
            if idx >= DIR_MAX_ENTRIES {
                return Err(Error::NoSpace);
            }
            let place = match self.entry_place(&mut d, idx)? {
                Some(p) => p,
                None => {
                    if g.kind != Kind::Fat32 && g.is_root_cluster(dir_first) {
                        return Err(Error::NoSpace);
                    }
                    let c = self.alloc_cluster(d.cur)?;
                    self.zero_cluster(c)?;
                    continue;
                }
            };
            let i = self.slot(place.0, false)?;
            let b0 = self.cache.bufs[i].0[place.1];
            if b0 == 0xE5 || b0 == 0x00 {
                if run_len == 0 {
                    run_start = idx;
                }
                run_len += 1;
                if run_len == need {
                    return Ok(run_start);
                }
            } else {
                run_len = 0;
            }
            idx += 1;
        }
    }

    /// Whether a short name's eleven bytes stand in the directory (the alias's uniqueness).
    fn short_exists(&mut self, dir_first: u32, short: &[u8; 11]) -> Result<bool, Error> {
        let mut d = Dir { first: dir_first, index: 0, cur: 0, cur_idx: 0, root: false, open: true };
        let mut idx = 0;
        while idx < DIR_MAX_ENTRIES {
            let Some((lba, off)) = self.entry_place(&mut d, idx)? else {
                return Ok(false);
            };
            let i = self.slot(lba, false)?;
            let e = &self.cache.bufs[i].0[off..off + 32];
            if e[0] == 0 {
                return Ok(false);
            }
            if e[0] != 0xE5 && e[11] & ATTR_LONG_MASK != ATTR_LONG_NAME && &e[..11] == short {
                return Ok(true);
            }
            idx += 1;
        }
        Ok(false)
    }

    /// The alias (specification 7.4): the name upper-cased, every unit outside
    /// the short set or over 0x7F '_', the spaces and the leading periods
    /// dropped, the base before the last period cut to six characters
    /// followed by '~' and N from 1, the extension the first three
    /// characters after the last period; N advanced until no short name of
    /// the directory matches, the base cut shorter as N passes 9, 99, 999
    /// and 9999; past 99999 Exists.
    fn make_alias(&mut self, dir_first: u32, units: &[u16], short: &mut [u8; 11]) -> Result<(), Error> {
        let mut base = [0u8; 8];
        let mut ext = [0x20u8; 3];
        let mut lead = 0;
        while lead < units.len() && units[lead] == b'.' as u16 {
            lead += 1;
        }
        let body = &units[lead..];
        let (bpart, epart): (&[u16], &[u16]) = match body.iter().rposition(|&u| u == b'.' as u16) {
            Some(p) => (&body[..p], &body[p + 1..]),
            None => (body, &[]),
        };
        let mut bl = 0;
        for &u in bpart {
            if bl == 8 {
                break;
            }
            if u == 0x20 || u == b'.' as u16 {
                continue;
            }
            base[bl] = alias_byte(u);
            bl += 1;
        }
        if bl == 0 {
            base[0] = b'_';
            bl = 1;
        }
        let mut el = 0;
        for &u in epart {
            if el == 3 {
                break;
            }
            if u == 0x20 {
                continue;
            }
            ext[el] = alias_byte(u);
            el += 1;
        }
        let mut n: u32 = 1;
        loop {
            let digits = if n < 10 { 1 } else if n < 100 { 2 } else if n < 1000 { 3 } else if n < 10_000 { 4 } else { 5 };
            let cut = bl.min(7 - digits);
            let mut s = [0x20u8; 11];
            s[..cut].copy_from_slice(&base[..cut]);
            s[cut] = b'~';
            let mut m = n;
            for k in (0..digits).rev() {
                s[cut + 1 + k] = b'0' + (m % 10) as u8;
                m /= 10;
            }
            s[8..11].copy_from_slice(&ext);
            if !self.short_exists(dir_first, &s)? {
                *short = s;
                return Ok(());
            }
            n += 1;
            if n > 99_999 {
                return Err(Error::Exists);
            }
        }
    }

    /// An entry created in the directory: a short entry alone
    /// when the name is a legal short name in upper case, else a long set
    /// over a generated alias unique within the directory, the alias's
    /// checksum in every long entry; the set in the first run of free
    /// entries that fits. The entry as the walk would serve it.
    fn create_entry(&mut self, dir_first: u32, units: &[u16], attr: u8, first: u32, size: u32, times: [u16; 5], crt_tenth: u8) -> Result<Entry, Error> {
        let mut short = [0x20u8; 11];
        let long_n = if legal_short(units, &mut short) {
            0
        } else {
            self.make_alias(dir_first, units, &mut short)?;
            (units.len() + 12) / 13
        };
        let start = self.find_free_run(dir_first, long_n as u32 + 1)?;
        let sum = checksum(&short);
        let mut d = Dir { first: dir_first, index: 0, cur: 0, cur_idx: 0, root: false, open: true };
        let mut set_start = (0u32, 0u16);
        for k in 0..long_n {
            let ord = long_n - k;
            let mut e = [0u8; 32];
            e[0] = ord as u8 | if k == 0 { 0x40 } else { 0 };
            e[11] = ATTR_LONG_NAME;
            e[13] = sum;
            let base = (ord - 1) * 13;
            for (j, &o) in LONG_OFFSETS.iter().enumerate() {
                let idx = base + j;
                let u = if idx < units.len() { units[idx] } else if idx == units.len() { 0 } else { 0xFFFF };
                e[o..o + 2].copy_from_slice(&u.to_le_bytes());
            }
            let place = self.put_raw(&mut d, start + k as u32, &e)?;
            if k == 0 {
                set_start = place;
            }
        }
        let hi = if self.geo.kind == Kind::Fat32 { (first >> 16) as u16 } else { 0 };
        let mut e = [0u8; 32];
        e[..11].copy_from_slice(&short);
        e[11] = attr;
        e[13] = crt_tenth;
        e[14..16].copy_from_slice(&times[0].to_le_bytes());
        e[16..18].copy_from_slice(&times[1].to_le_bytes());
        e[18..20].copy_from_slice(&times[2].to_le_bytes());
        e[20..22].copy_from_slice(&hi.to_le_bytes());
        e[22..24].copy_from_slice(&times[3].to_le_bytes());
        e[24..26].copy_from_slice(&times[4].to_le_bytes());
        e[26..28].copy_from_slice(&(first as u16).to_le_bytes());
        e[28..32].copy_from_slice(&size.to_le_bytes());
        let place = self.put_raw(&mut d, start + long_n as u32, &e)?;
        let mut out = Entry::EMPTY;
        out.short = short;
        out.attr = attr;
        out.crt_tenth = crt_tenth;
        out.crt_time = times[0];
        out.crt_date = times[1];
        out.acc_date = times[2];
        out.wrt_time = times[3];
        out.wrt_date = times[4];
        out.first = first;
        out.size = size;
        out.place = place;
        out.set_start = if long_n > 0 { set_start } else { place };
        out.index = start + long_n as u32;
        out.set_len = long_n as u8;
        out.dir_first = dir_first;
        let mut name = [0u8; 256];
        let len = if long_n > 0 { utf16_to_utf8(units, &mut name).unwrap_or(0) } else { 0 };
        if len > 0 {
            out.long = true;
            out.name = name;
            out.name_len = len as u16;
        } else {
            let n = short_name_utf8(&short, 0, &mut name);
            out.name = name;
            out.name_len = n as u16;
        }
        Ok(out)
    }

    /// An entry's set (its long entries and the short entry) marked free,
    /// 0xE5, and the sectors put on the device before the caller frees the
    /// chain.
    fn remove_entry_set(&mut self, e: &Entry) -> Result<(), Error> {
        let mut d = Dir { first: e.dir_first, index: 0, cur: 0, cur_idx: 0, root: false, open: true };
        let from = e.index - e.set_len as u32;
        let mut touched = [NO_TAG; 4];
        for idx in from..=e.index {
            let Some((lba, off)) = self.entry_place(&mut d, idx)? else {
                return Err(Error::Io);
            };
            let i = self.slot(lba, false)?;
            self.cache.bufs[i].0[off] = 0xE5;
            self.mark_dirty(i);
            if !touched.contains(&lba) {
                if let Some(t) = touched.iter_mut().find(|t| **t == NO_TAG) {
                    *t = lba;
                }
            }
        }
        for lba in touched {
            if lba != NO_TAG {
                self.flush_lba(lba)?;
            }
        }
        Ok(())
    }

    /// The unlink: a directory IsDir; a read-only file ReadOnly;
    /// the entry set marked free FIRST, then the chain freed.
    pub fn unlink(&mut self, path: &[u8]) -> Result<(), Error> {
        let l = self.resolve(path)?;
        if l.is_root || l.entry.is_dir() {
            return Err(Error::IsDir);
        }
        if l.entry.read_only() {
            return Err(Error::ReadOnly);
        }
        self.remove_entry_set(&l.entry)?;
        self.free_chain(l.entry.first)?;
        self.sync()
    }

    /// The mkdir (specification 6.5): one cluster allocated END-first and zeroed, "." with
    /// the directory's own cluster and ".." with the parent's (0 for the
    /// root) at its head with the directory's times, then the parent's
    /// entry set.
    pub fn mkdir(&mut self, path: &[u8]) -> Result<(), Error> {
        let (parent, units, n) = self.split_parent(path)?;
        if n == 0 {
            return Err(Error::Exists);
        }
        let dir_first = self.dir_first_of(&parent);
        if self.find(dir_first, &units[..n])?.is_some() {
            return Err(Error::Exists);
        }
        let (d, t) = self.stamp_now();
        let c = self.alloc_cluster(0)?;
        self.zero_cluster(c)?;
        let g = self.geo;
        let parent_first = if parent.is_root { 0 } else { parent.entry.first };
        let lba = g.first_sector_of(c);
        let i = self.slot(lba, false)?;
        let dots: [(&[u8; 11], u32); 2] = [(b".          ", c), (b"..         ", parent_first)];
        for (k, (name, first)) in dots.iter().enumerate() {
            let hi = if g.kind == Kind::Fat32 { (first >> 16) as u16 } else { 0 };
            let e = &mut self.cache.bufs[i].0[k * 32..k * 32 + 32];
            e[..11].copy_from_slice(name.as_slice());
            e[11] = ATTR_DIRECTORY;
            e[14..16].copy_from_slice(&t.to_le_bytes());
            e[16..18].copy_from_slice(&d.to_le_bytes());
            e[18..20].copy_from_slice(&d.to_le_bytes());
            e[20..22].copy_from_slice(&hi.to_le_bytes());
            e[22..24].copy_from_slice(&t.to_le_bytes());
            e[24..26].copy_from_slice(&d.to_le_bytes());
            e[26..28].copy_from_slice(&(*first as u16).to_le_bytes());
        }
        self.mark_dirty(i);
        self.create_entry(dir_first, &units[..n], ATTR_DIRECTORY, c, 0, [t, d, d, t, d], 0)?;
        self.sync()
    }

    /// The rmdir: NotEmpty while any entry but "." and ".." lives, NotDir
    /// for a file, the root Invalid; the entry set marked free FIRST, then
    /// the chain freed.
    pub fn rmdir(&mut self, path: &[u8]) -> Result<(), Error> {
        let l = self.resolve(path)?;
        if l.is_root || self.geo.is_root_cluster(l.entry.first) || l.entry.place == (0, 0) {
            return Err(Error::Invalid);
        }
        if !l.entry.is_dir() {
            return Err(Error::NotDir);
        }
        if l.entry.read_only() {
            return Err(Error::ReadOnly);
        }
        let mut d = Dir { first: l.entry.first, index: 0, cur: 0, cur_idx: 0, root: false, open: true };
        let mut e = Entry::EMPTY;
        let mut long = LongBuf::new();
        while self.next_entry(&mut d, &mut e, &mut long)? {
            if &e.short != b".          " && &e.short != b"..         " {
                return Err(Error::NotEmpty);
            }
        }
        self.remove_entry_set(&l.entry)?;
        self.free_chain(l.entry.first)?;
        self.sync()
    }

    /// Whether `dir` is `of` or one of its ancestors (a directory may not
    /// move into itself).
    fn is_ancestor(&mut self, dir: u32, of: u32) -> Result<bool, Error> {
        let mut c = of;
        for _ in 0..64 {
            if self.geo.is_root_cluster(c) {
                return Ok(false);
            }
            if c == dir {
                return Ok(true);
            }
            c = self.parent_of(c)?;
        }
        Err(Error::Io)
    }

    /// The rename: within the volume; the new entry set written
    /// in the target directory with the old entry's cluster, size,
    /// attributes and times and put on the device, THEN the old set marked
    /// free; a directory moved across parents has its ".." rewritten; onto
    /// an existing file the target is unlinked first (Linux's replace, in
    /// two steps as FAT allows); onto an existing directory Exists (the
    /// stated departure); onto itself, by any case, a no-op.
    pub fn rename(&mut self, old: &[u8], new: &[u8]) -> Result<(), Error> {
        let src = self.resolve(old)?;
        if src.is_root || src.entry.place == (0, 0) {
            return Err(Error::Invalid);
        }
        if src.entry.read_only() {
            return Err(Error::ReadOnly);
        }
        let (parent, units, n) = self.split_parent(new)?;
        if n == 0 {
            return Err(Error::Exists);
        }
        let dst_first = self.dir_first_of(&parent);
        let e = src.entry;
        if e.is_dir() && self.is_ancestor(e.first, dst_first)? {
            return Err(Error::Invalid);
        }
        if let Some(t) = self.find(dst_first, &units[..n])? {
            if t.place == e.place {
                return Ok(());
            }
            if t.is_dir() {
                return Err(Error::Exists);
            }
            if e.is_dir() {
                return Err(Error::NotDir);
            }
            if t.read_only() {
                return Err(Error::ReadOnly);
            }
            self.remove_entry_set(&t)?;
            self.free_chain(t.first)?;
        }
        self.create_entry(dst_first, &units[..n], e.attr, e.first, e.size, [e.crt_time, e.crt_date, e.acc_date, e.wrt_time, e.wrt_date], e.crt_tenth)?;
        self.flush_buffers()?;
        self.remove_entry_set(&e)?;
        if e.is_dir() && dst_first != e.dir_first {
            let pf = if parent.is_root { 0 } else { parent.entry.first };
            let hi = if self.geo.kind == Kind::Fat32 { (pf >> 16) as u16 } else { 0 };
            let lba = self.geo.first_sector_of(e.first);
            let i = self.slot(lba, false)?;
            let x = &mut self.cache.bufs[i].0[32..64];
            if &x[..11] == b"..         " {
                x[20..22].copy_from_slice(&hi.to_le_bytes());
                x[26..28].copy_from_slice(&(pf as u16).to_le_bytes());
                self.mark_dirty(i);
            }
        }
        self.sync()
    }
}
