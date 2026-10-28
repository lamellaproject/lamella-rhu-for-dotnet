//! The host tier's SECOND READER: a read-only FAT12, FAT16
//! and FAT32 walker with long names, written from the Microsoft FAT
//! Specification SEPARATELY from the kernel's fat.rs -- no parsing helper,
//! no constant table, no type shared with it; the std library's Vec and
//! String where the kernel's module has fixed buffers -- so that the two
//! can disagree. The differential case lists and reads every file of a
//! volume the kernel's layer wrote and compares name by name and byte by
//! byte. Host-only; no cache, every sector read from the
//! device afresh.

use crate::sparse::Sparse;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Item {
    pub path: String,
    pub is_dir: bool,
    pub size: u32,
    pub first: u32,
}

pub struct Reader<'a> {
    dev: &'a Sparse,
    /// The partition's first sector, found by this reader's own table step (0 for a boot sector at 0).
    base: u32,
    sectors_per_cluster: u32,
    reserved: u32,
    fat_count: u32,
    fat_sectors: u32,
    root_entries: u32,
    total_sectors: u32,
    bits: u8,
    root_cluster: u32,
    data_start: u32,
    cluster_count: u32,
}

fn le16(s: &[u8], at: usize) -> u32 {
    s[at] as u32 | (s[at + 1] as u32) << 8
}

fn le32(s: &[u8], at: usize) -> u32 {
    le16(s, at) | le16(s, at + 2) << 16
}

impl<'a> Reader<'a> {
    pub fn open(dev: &'a Sparse) -> Result<Reader<'a>, String> {
        let b0 = dev.sector(0);
        if le16(&b0, 510) != 0xAA55 {
            return Err("no boot signature".into());
        }
        // This reader's own partition step: a boot sector begins with a jump; anything else is a master boot record
        // whose table at 0x1BE names the volume's first sector in its first FAT-type entry.
        let base = if b0[0] == 0xEB || b0[0] == 0xE9 {
            0
        } else {
            let mut at = None;
            for i in 0..4 {
                let e = &b0[0x1BE + i * 16..0x1BE + i * 16 + 16];
                if [0x01, 0x04, 0x06, 0x07, 0x0B, 0x0C, 0x0E].contains(&e[4]) {
                    at = Some(le32(e, 8));
                    break;
                }
            }
            at.ok_or_else(|| String::from("no FAT partition in the table"))?
        };
        let b = dev.sector(base);
        if le16(&b, 510) != 0xAA55 {
            return Err("no boot signature at the partition".into());
        }
        if le16(&b, 11) != 512 {
            return Err(format!("sector size {}", le16(&b, 11)));
        }
        let sectors_per_cluster = b[13] as u32;
        let reserved = le16(&b, 14);
        let fat_count = b[16] as u32;
        let root_entries = le16(&b, 17);
        let total_sectors = if le16(&b, 19) != 0 { le16(&b, 19) } else { le32(&b, 32) };
        let fat_sectors = if le16(&b, 22) != 0 { le16(&b, 22) } else { le32(&b, 36) };
        if sectors_per_cluster == 0 || reserved == 0 || fat_count == 0 || total_sectors == 0 || fat_sectors == 0 {
            return Err("a zero field in the BPB".into());
        }
        let root_sectors = (root_entries * 32).div_ceil(512);
        let data_start = reserved + fat_count * fat_sectors + root_sectors;
        if data_start >= total_sectors {
            return Err("no data region".into());
        }
        let cluster_count = (total_sectors - data_start) / sectors_per_cluster;
        let bits = if cluster_count < 4085 {
            12
        } else if cluster_count < 65525 {
            16
        } else {
            32
        };
        let root_cluster = if bits == 32 { le32(&b, 44) } else { 0 };
        Ok(Reader { dev, base, sectors_per_cluster, reserved, fat_count, fat_sectors, root_entries, total_sectors, bits, root_cluster, data_start, cluster_count })
    }

    pub fn bits(&self) -> u8 {
        self.bits
    }

    pub fn cluster_count(&self) -> u32 {
        self.cluster_count
    }

    /// The FAT entry of a cluster from the first FAT copy, by the width.
    pub fn fat(&self, n: u32) -> u32 {
        let base = self.reserved as u64 * 512;
        let byte = match self.bits {
            12 => base + n as u64 * 3 / 2,
            16 => base + n as u64 * 2,
            _ => base + n as u64 * 4,
        };
        let read = |at: u64| self.dev.sector(self.base + (at / 512) as u32)[(at % 512) as usize] as u32;
        match self.bits {
            12 => {
                let word = read(byte) | read(byte + 1) << 8;
                if n % 2 == 1 { word >> 4 } else { word & 0xFFF }
            }
            16 => read(byte) | read(byte + 1) << 8,
            _ => (read(byte) | read(byte + 1) << 8 | read(byte + 2) << 16 | read(byte + 3) << 24) & 0x0FFF_FFFF,
        }
    }

    /// Both FAT copies compared entry by entry (the mirroring check of the differential case).
    pub fn fat_copies_agree(&self) -> bool {
        if self.fat_count < 2 {
            return true;
        }
        for s in 0..self.fat_sectors {
            if self.dev.sector(self.base + self.reserved + s) != self.dev.sector(self.base + self.reserved + self.fat_sectors + s) {
                return false;
            }
        }
        true
    }

    /// The free clusters by a scan of the first FAT copy.
    pub fn free_clusters(&self) -> u32 {
        (2..self.cluster_count + 2).filter(|&n| self.fat(n) == 0).count() as u32
    }

    /// A chain's clusters in order, bounded by the count (a loop stops the walk with an error).
    pub fn chain(&self, first: u32) -> Result<Vec<u32>, String> {
        let mut out = Vec::new();
        let mut c = first;
        while c >= 2 && c <= self.cluster_count + 1 {
            if out.len() as u32 > self.cluster_count {
                return Err(format!("the chain from {first} loops"));
            }
            out.push(c);
            c = self.fat(c);
            if c == 0 {
                return Err(format!("a free cluster inside the chain from {first}"));
            }
        }
        Ok(out)
    }

    fn cluster_bytes(&self, c: u32) -> Vec<u8> {
        let mut out = Vec::new();
        let first = self.data_start + (c - 2) * self.sectors_per_cluster;
        for s in 0..self.sectors_per_cluster {
            out.extend_from_slice(&self.dev.sector(self.base + first + s));
        }
        out
    }

    /// A directory's raw bytes: the fixed root region on FAT12 and FAT16, else the chain.
    fn directory_bytes(&self, first: u32) -> Result<Vec<u8>, String> {
        if first == 0 && self.bits != 32 {
            let mut out = Vec::new();
            let start = self.reserved + self.fat_count * self.fat_sectors;
            for s in 0..(self.root_entries * 32).div_ceil(512) {
                out.extend_from_slice(&self.dev.sector(self.base + start + s));
            }
            return Ok(out);
        }
        let first = if first == 0 { self.root_cluster } else { first };
        let mut out = Vec::new();
        for c in self.chain(first)? {
            out.extend_from_slice(&self.cluster_bytes(c));
        }
        Ok(out)
    }

    fn short_checksum(name: &[u8]) -> u8 {
        let mut sum: u8 = 0;
        for &b in name.iter().take(11) {
            sum = (if sum & 1 != 0 { 0x80 } else { 0 }) + (sum >> 1) + b;
            sum = sum.wrapping_add(0);
        }
        sum
    }

    fn short_checksum_wrapping(name: &[u8]) -> u8 {
        let mut sum: u8 = 0;
        for &b in name.iter().take(11) {
            sum = (if sum & 1 != 0 { 0x80u8 } else { 0 }).wrapping_add(sum >> 1).wrapping_add(b);
        }
        sum
    }

    fn short_name(raw: &[u8], case: u8) -> String {
        let piece = |bytes: &[u8], lower: bool| -> String {
            let end = bytes.iter().rposition(|&b| b != b' ').map(|p| p + 1).unwrap_or(0);
            bytes[..end]
                .iter()
                .map(|&b| {
                    let b = if b == 0x05 { 0xE5 } else { b };
                    if b < 0x80 {
                        let c = b as char;
                        if lower { c.to_ascii_lowercase() } else { c }
                    } else {
                        char::REPLACEMENT_CHARACTER
                    }
                })
                .collect()
        };
        let base = piece(&raw[..8], case & 0x08 != 0);
        let ext = piece(&raw[8..11], case & 0x10 != 0);
        if ext.is_empty() { base } else { format!("{base}.{ext}") }
    }

    /// One directory's entries: (name, attributes, first cluster, size),
    /// the long name assembled from its set when the set is whole.
    pub fn entries(&self, first: u32) -> Result<Vec<(String, u8, u32, u32)>, String> {
        let bytes = self.directory_bytes(first)?;
        let mut out = Vec::new();
        let mut pieces: Vec<(u8, u8, Vec<u16>)> = Vec::new();
        for e in bytes.chunks_exact(32) {
            if e[0] == 0 {
                break;
            }
            if e[0] == 0xE5 {
                pieces.clear();
                continue;
            }
            if e[11] & 0x3F == 0x0F {
                let mut units = Vec::new();
                for o in [1, 3, 5, 7, 9, 14, 16, 18, 20, 22, 24, 28, 30] {
                    units.push(le16(e, o) as u16);
                }
                pieces.push((e[0], e[13], units));
                continue;
            }
            if e[11] & 0x08 != 0 {
                pieces.clear();
                continue;
            }
            let sum = Self::short_checksum_wrapping(&e[..11]);
            let mut long = None;
            if let Some(last) = pieces.first() {
                let n = (last.0 & 0x3F) as usize;
                let ordered: Vec<usize> = pieces.iter().map(|p| (p.0 & 0x3F) as usize).collect();
                let expected: Vec<usize> = (1..=n).rev().collect();
                if last.0 & 0x40 != 0 && ordered == expected && pieces.iter().all(|p| p.1 == sum) {
                    let mut units: Vec<u16> = Vec::new();
                    for p in pieces.iter().rev() {
                        units.extend_from_slice(&p.2);
                    }
                    if let Some(z) = units.iter().position(|&u| u == 0) {
                        units.truncate(z);
                    }
                    while units.last() == Some(&0xFFFF) {
                        units.pop();
                    }
                    let s = String::from_utf16_lossy(&units);
                    if !s.is_empty() && s.len() <= 255 {
                        long = Some(s);
                    }
                }
            }
            pieces.clear();
            let name = long.unwrap_or_else(|| Self::short_name(e, e[12]));
            let first_cluster = if self.bits == 32 { le16(e, 20) << 16 | le16(e, 26) } else { le16(e, 26) };
            out.push((name, e[11], first_cluster, le32(e, 28)));
        }
        Ok(out)
    }

    /// Every file and directory of the volume with its full path, sorted.
    pub fn list_all(&self) -> Result<Vec<Item>, String> {
        let mut out = Vec::new();
        self.walk("", 0, &mut out, 0)?;
        out.sort();
        Ok(out)
    }

    fn walk(&self, prefix: &str, first: u32, out: &mut Vec<Item>, depth: u32) -> Result<(), String> {
        if depth > 64 {
            return Err("a directory tree deeper than 64".into());
        }
        for (name, attr, cluster, size) in self.entries(first)? {
            if name == "." || name == ".." {
                continue;
            }
            let path = format!("{prefix}/{name}");
            let is_dir = attr & 0x10 != 0;
            out.push(Item { path: path.clone(), is_dir, size, first: cluster });
            if is_dir {
                self.walk(&path, cluster, out, depth + 1)?;
            }
        }
        Ok(())
    }

    /// A file's bytes through its chain, cut to its size.
    pub fn read_file(&self, item: &Item) -> Result<Vec<u8>, String> {
        if item.first == 0 {
            return Ok(Vec::new());
        }
        let mut out = Vec::new();
        for c in self.chain(item.first)? {
            out.extend_from_slice(&self.cluster_bytes(c));
            if out.len() as u64 >= item.size as u64 {
                break;
            }
        }
        if (out.len() as u64) < item.size as u64 {
            return Err(format!("{}: the chain is shorter than the size", item.path));
        }
        out.truncate(item.size as usize);
        Ok(out)
    }

    pub fn total_sectors(&self) -> u32 {
        self.total_sectors
    }

    #[allow(dead_code)]
    fn unused_checksum_form(name: &[u8]) -> u8 {
        Self::short_checksum(name)
    }
}
