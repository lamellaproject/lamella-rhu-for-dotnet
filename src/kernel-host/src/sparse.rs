//! The host tier's sparse block device: a map from sector number to its 512 bytes, an absent
//! sector reading zero, loaded from a golden's .rsi file under goldens/fat/
//! after the manifest's digests are checked (the sparse file's
//! sha256 and the expanded image's sha256 both against MANIFEST.md), with
//! a trace of every device call for the cases that count them and a patch
//! for the doctored controls (applied in memory, never a separate golden).
//! Host-only.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use crate::fat::{BlockDevice, Error, SECTOR};
use crate::sha256::sha256_hex;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Op {
    Read,
    Write,
    Sync,
}

#[derive(Clone)]
pub struct Sparse {
    sectors: BTreeMap<u32, [u8; SECTOR]>,
    count: u32,
    /// Every call: the verb, the first sector, the sector count.
    pub trace: Vec<(Op, u32, u32)>,
}

impl Sparse {
    pub fn blank(count: u32) -> Sparse {
        Sparse { sectors: BTreeMap::new(), count, trace: Vec::new() }
    }

    /// The sparse form read (sparse.py's layout: the magic line, the sector
    /// count line, then records of a little-endian sector number and 512 bytes).
    pub fn from_rsi(data: &[u8]) -> Result<Sparse, String> {
        const MAGIC: &[u8] = b"RHU sparse image 1\n";
        if !data.starts_with(MAGIC) {
            return Err("not an RHU sparse image".into());
        }
        let nl = data[MAGIC.len()..].iter().position(|&b| b == b'\n').ok_or("no count line")? + MAGIC.len();
        let count: u32 = std::str::from_utf8(&data[MAGIC.len()..nl]).map_err(|e| e.to_string())?.parse().map_err(|_| "a bad count".to_string())?;
        let mut s = Sparse::blank(count);
        let mut pos = nl + 1;
        let mut last: i64 = -1;
        while pos < data.len() {
            if pos + 4 + SECTOR > data.len() {
                return Err(format!("a short record at byte {pos}"));
            }
            let i = u32::from_le_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]]);
            if i as i64 <= last || i >= count {
                return Err(format!("a record out of order or out of range at byte {pos}"));
            }
            let mut sec = [0u8; SECTOR];
            sec.copy_from_slice(&data[pos + 4..pos + 4 + SECTOR]);
            s.sectors.insert(i, sec);
            last = i as i64;
            pos += 4 + SECTOR;
        }
        Ok(s)
    }

    pub fn sector_count(&self) -> u32 {
        self.count
    }

    pub fn kept(&self) -> usize {
        self.sectors.len()
    }

    pub fn sector(&self, lba: u32) -> [u8; SECTOR] {
        self.sectors.get(&lba).copied().unwrap_or([0; SECTOR])
    }

    /// The expanded image's sha256 (the manifest's raw digest).
    pub fn raw_sha256(&self) -> String {
        let mut raw = vec![0u8; self.count as usize * SECTOR];
        for (&i, s) in &self.sectors {
            raw[i as usize * SECTOR..(i as usize + 1) * SECTOR].copy_from_slice(s);
        }
        sha256_hex(&raw)
    }

    /// A doctored control: the bytes patched in memory at the sector and offset.
    pub fn patch(&mut self, lba: u32, off: usize, bytes: &[u8]) {
        let mut s = self.sector(lba);
        s[off..off + bytes.len()].copy_from_slice(bytes);
        self.sectors.insert(lba, s);
    }

    /// The image placed behind a master boot record: a fresh device of base + count
    /// sectors, sector 0 the record with one entry of the given type at the base, every kept sector shifted up by the base.
    pub fn behind_table(&self, base: u32, ptype: u8) -> Sparse {
        let mut out = Sparse::blank(base + self.count);
        let mut mbr = [0u8; SECTOR];
        mbr[447..450].copy_from_slice(&[0xFE, 0xFF, 0xFF]);
        mbr[450] = ptype;
        mbr[451..454].copy_from_slice(&[0xFE, 0xFF, 0xFF]);
        mbr[454..458].copy_from_slice(&base.to_le_bytes());
        mbr[458..462].copy_from_slice(&self.count.to_le_bytes());
        mbr[510] = 0x55;
        mbr[511] = 0xAA;
        out.sectors.insert(0, mbr);
        for (lba, s) in &self.sectors {
            out.sectors.insert(lba + base, *s);
        }
        out
    }

    pub fn calls(&self, op: Op) -> u32 {
        self.trace.iter().filter(|t| t.0 == op).count() as u32
    }

    pub fn sectors_moved(&self, op: Op) -> u32 {
        self.trace.iter().filter(|t| t.0 == op).map(|t| t.2).sum()
    }
}

impl BlockDevice for Sparse {
    fn blocks(&mut self) -> u32 {
        self.count
    }

    fn read(&mut self, lba: u32, buf: &mut [u8]) -> Result<(), Error> {
        let n = buf.len() / SECTOR;
        assert!(buf.len() % SECTOR == 0 && n >= 1 && n as u32 <= crate::fat::BLK_MAX, "a device call moves whole sectors, 1 to 128: {} bytes", buf.len());
        assert!(buf.as_ptr() as usize % 4 == 0, "a device buffer is 4-byte aligned");
        if lba as u64 + n as u64 > self.count as u64 {
            return Err(Error::Io);
        }
        self.trace.push((Op::Read, lba, n as u32));
        for k in 0..n {
            buf[k * SECTOR..(k + 1) * SECTOR].copy_from_slice(&self.sector(lba + k as u32));
        }
        Ok(())
    }

    fn write(&mut self, lba: u32, data: &[u8]) -> Result<(), Error> {
        let n = data.len() / SECTOR;
        assert!(data.len() % SECTOR == 0 && n >= 1 && n as u32 <= crate::fat::BLK_MAX, "a device call moves whole sectors, 1 to 128: {} bytes", data.len());
        if lba as u64 + n as u64 > self.count as u64 {
            return Err(Error::Io);
        }
        self.trace.push((Op::Write, lba, n as u32));
        for k in 0..n {
            let mut s = [0u8; SECTOR];
            s.copy_from_slice(&data[k * SECTOR..(k + 1) * SECTOR]);
            if s == [0; SECTOR] {
                self.sectors.remove(&(lba + k as u32));
            } else {
                self.sectors.insert(lba + k as u32, s);
            }
        }
        Ok(())
    }

    fn sync(&mut self) -> Result<(), Error> {
        self.trace.push((Op::Sync, 0, 0));
        Ok(())
    }
}

/// goldens/fat beside the crate.
pub fn goldens_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..").join("goldens").join("fat")
}

/// One image's row of the manifest.
#[derive(Clone, Debug)]
pub struct ImageRow {
    pub name: String,
    pub sectors: u32,
    pub clusters: u32,
    pub cluster_bytes: u32,
    pub free: u32,
    pub raw_sha256: String,
    pub sparse_sha256: String,
    pub sparse_bytes: u64,
}

/// The manifest's tables: the images and the recipe's file table (the
/// volume path, the bytes, the sha256).
#[derive(Clone, Debug, Default)]
pub struct Manifest {
    pub images: Vec<ImageRow>,
    pub files: Vec<(String, u64, String)>,
    pub tools: Vec<(String, String)>,
}

pub fn manifest() -> Manifest {
    let text = std::fs::read_to_string(goldens_dir().join("MANIFEST.md")).expect("goldens/fat/MANIFEST.md");
    let mut m = Manifest::default();
    let mut section = "";
    for line in text.lines() {
        if line.starts_with("## ") {
            section = if line.contains("images") {
                "images"
            } else if line.contains("file table") {
                "files"
            } else if line.contains("tools") {
                "tools"
            } else {
                ""
            };
            continue;
        }
        if !line.starts_with("| ") || line.starts_with("|--") || line.starts_with("| golden") || line.starts_with("| volume path") || line.starts_with("| tool") {
            continue;
        }
        let cols: Vec<&str> = line.trim_matches('|').split('|').map(|c| c.trim()).collect();
        match section {
            "images" if cols.len() >= 8 => m.images.push(ImageRow {
                name: cols[0].to_string(),
                sectors: cols[1].parse().expect("sectors"),
                clusters: cols[2].parse().expect("clusters"),
                cluster_bytes: cols[3].parse().expect("cluster bytes"),
                free: cols[4].parse().expect("free clusters"),
                raw_sha256: cols[5].to_string(),
                sparse_sha256: cols[6].to_string(),
                sparse_bytes: cols[7].parse().expect("sparse bytes"),
            }),
            "files" if cols.len() >= 3 => {
                if let Ok(n) = cols[1].parse::<u64>() {
                    m.files.push((cols[0].to_string(), n, cols[2].to_string()));
                }
            }
            "tools" if cols.len() >= 2 => m.tools.push((cols[0].to_string(), cols[1].to_string())),
            _ => {}
        }
    }
    m
}

static CHECKED: OnceLock<Mutex<BTreeMap<String, Sparse>>> = OnceLock::new();

/// A golden loaded and checked against the manifest (both digests), once
/// per process; each case takes its own copy, so a patched or written copy
/// never reaches another case.
pub fn golden(name: &str) -> Sparse {
    let cache = CHECKED.get_or_init(|| Mutex::new(BTreeMap::new()));
    let mut cache = cache.lock().unwrap();
    if let Some(s) = cache.get(name) {
        let mut c = s.clone();
        c.trace.clear();
        return c;
    }
    let m = manifest();
    let row = m.images.iter().find(|r| r.name == name).unwrap_or_else(|| panic!("the manifest names {name}"));
    let bytes = std::fs::read(goldens_dir().join(format!("{name}.rsi"))).unwrap_or_else(|e| panic!("goldens/fat/{name}.rsi: {e}"));
    assert_eq!(sha256_hex(&bytes), row.sparse_sha256, "{name}.rsi: the sparse file's sha256 differs from the manifest's");
    let s = Sparse::from_rsi(&bytes).unwrap_or_else(|e| panic!("{name}.rsi: {e}"));
    assert_eq!(s.count, row.sectors, "{name}: the sector count differs from the manifest's");
    assert_eq!(s.raw_sha256(), row.raw_sha256, "{name}: the expanded image's sha256 differs from the manifest's");
    cache.insert(name.to_string(), s.clone());
    s
}

/// The recipe's generator, derived a second time here from its stated
/// rule (the host case (8) compares every file's bytes against it): the
/// bytes are little-endian 32-bit words counting up from the first four
/// bytes of the sha256 of the file's name, cut to the length.
pub fn recipe_bytes(name: &str, len: usize) -> Vec<u8> {
    let hex = sha256_hex(name.as_bytes());
    let seed = u32::from_le_bytes([
        u8::from_str_radix(&hex[0..2], 16).unwrap(),
        u8::from_str_radix(&hex[2..4], 16).unwrap(),
        u8::from_str_radix(&hex[4..6], 16).unwrap(),
        u8::from_str_radix(&hex[6..8], 16).unwrap(),
    ]);
    let mut out = Vec::with_capacity(len + 4);
    let mut i = 0u32;
    while out.len() < len {
        out.extend_from_slice(&seed.wrapping_add(i).to_le_bytes());
        i += 1;
    }
    out.truncate(len);
    out
}
