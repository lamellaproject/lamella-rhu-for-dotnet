//! Readers over an ELF32 relocatable object's own bytes -- a section by
//! name, a symbol's word, the producer stamp a compiler leaves in the
//! object's comment section -- and the identity of an object apart from
//! that stamp, taken through the toolchain's object copier.

use crate::sha256::sha256_file;
use crate::tools::{llvm_tool, run_captured, RustcFacts};
use std::path::Path;
use std::time::Duration;

fn u16_at(b: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(b.get(at..at + 2)?.try_into().ok()?))
}

fn u32_at(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(at..at + 4)?.try_into().ok()?))
}

/// One section header's facts.
struct Section {
    name: String,
    kind: u32,
    offset: usize,
    size: usize,
    link: usize,
    entsize: usize,
}

/// A NUL-terminated string out of a string table.
fn cstr(table: &[u8], at: usize) -> Option<String> {
    let bytes = table.get(at..)?.split(|&c| c == 0).next()?;
    Some(String::from_utf8_lossy(bytes).into_owned())
}

/// The section table of an ELF32 little-endian relocatable object; None
/// when the bytes are not one, or its tables cannot be read.
fn sections(obj: &[u8]) -> Option<Vec<Section>> {
    if obj.get(0..4)? != b"\x7fELF" || *obj.get(4)? != 1 || *obj.get(5)? != 1 || u16_at(obj, 0x10)? != 1 {
        return None;
    }
    let shoff = u32_at(obj, 0x20)? as usize;
    let shentsize = u16_at(obj, 0x2e)? as usize;
    let shnum = u16_at(obj, 0x30)? as usize;
    let shstrndx = u16_at(obj, 0x32)? as usize;
    if shentsize < 40 || shstrndx >= shnum {
        return None;
    }
    let raw = |i: usize| -> Option<(usize, u32, usize, usize, usize, usize)> {
        let at = shoff + i * shentsize;
        Some((
            u32_at(obj, at)? as usize,
            u32_at(obj, at + 4)?,
            u32_at(obj, at + 16)? as usize,
            u32_at(obj, at + 20)? as usize,
            u32_at(obj, at + 24)? as usize,
            u32_at(obj, at + 36)? as usize,
        ))
    };
    let (_, _, names_off, names_size, _, _) = raw(shstrndx)?;
    let names = obj.get(names_off..names_off + names_size)?;
    let mut out = Vec::with_capacity(shnum);
    for i in 0..shnum {
        let (name_at, kind, offset, size, link, entsize) = raw(i)?;
        out.push(Section { name: cstr(names, name_at)?, kind, offset, size, link, entsize });
    }
    Some(out)
}

/// A named section's file range (offset, size); None when the object
/// carries no section of that name.
pub fn section_span(obj: &[u8], name: &str) -> Option<(usize, usize)> {
    sections(obj)?.into_iter().find(|s| s.name == name).map(|s| (s.offset, s.size))
}

/// The first section whose name starts with `prefix` and that carries
/// bytes (a progbits section with a nonzero size), as (offset, size).
pub fn section_span_prefixed(obj: &[u8], prefix: &str) -> Option<(usize, usize)> {
    sections(obj)?
        .into_iter()
        .find(|s| s.name.starts_with(prefix) && s.kind == 1 && s.size > 0)
        .map(|s| (s.offset, s.size))
}

/// The 32-bit word a defined data symbol names, read at the symbol's
/// place in its section, with the word's file offset; None when the
/// object defines no such symbol.
pub fn symbol_word(obj: &[u8], symbol: &str) -> Option<(u32, usize)> {
    let secs = sections(obj)?;
    for s in secs.iter().filter(|s| s.kind == 2) {
        let strtab_sec = secs.get(s.link)?;
        let strtab = obj.get(strtab_sec.offset..strtab_sec.offset + strtab_sec.size)?;
        let entsize = if s.entsize == 0 { 16 } else { s.entsize };
        for k in 0..s.size / entsize {
            let at = s.offset + k * entsize;
            let name_at = u32_at(obj, at)? as usize;
            let value = u32_at(obj, at + 4)? as usize;
            let shndx = u16_at(obj, at + 14)? as usize;
            if cstr(strtab, name_at)? != symbol {
                continue;
            }
            if shndx == 0 || shndx >= 0xff00 {
                return None;
            }
            let home = secs.get(shndx)?;
            if value + 4 > home.size {
                return None;
            }
            let word_at = home.offset + value;
            return Some((u32_at(obj, word_at)?, word_at));
        }
    }
    None
}

/// The producer stamp: the strings of the object's comment section,
/// joined with " | " when there are several; None when the object
/// carries no comment section.
pub fn producer_stamp(obj: &[u8]) -> Option<String> {
    let (off, size) = section_span(obj, ".comment")?;
    let bytes = obj.get(off..off + size)?;
    let parts: Vec<String> = bytes
        .split(|&c| c == 0)
        .filter(|p| !p.is_empty())
        .map(|p| String::from_utf8_lossy(p).into_owned())
        .collect();
    Some(parts.join(" | "))
}

/// The producer stamp of an object on disk, as the record prints it:
/// the stamp, or "none" for an object without a comment section.
pub fn producer_stamp_of(path: &Path) -> String {
    match std::fs::read(path) {
        Ok(bytes) => producer_stamp(&bytes).unwrap_or_else(|| "none".to_string()),
        Err(e) => format!("unreadable: {e}"),
    }
}

/// The distinct stamps of a leg's objects in the order first seen, each
/// quoted and joined with "; " -- one value when every object carries the
/// same stamp, "none" when the list is empty.
pub fn distinct_stamps(stamps: &[String]) -> String {
    let mut seen: Vec<&String> = Vec::new();
    for s in stamps {
        if !seen.contains(&s) {
            seen.push(s);
        }
    }
    if seen.is_empty() {
        return "none".to_string();
    }
    seen.iter().map(|s| format!("'{s}'")).collect::<Vec<_>>().join("; ")
}

/// The sha256 of an object with its comment section removed by the
/// toolchain's object copier: the copy is written to `scratch` (a fresh
/// path; the input is never touched) and hashed. The copier lays the copy
/// out itself, so this identity is the copy's rather than the file's --
/// on both sides of a comparison alike. A refusal names the copier's own
/// reason (a section that a relocation targets cannot be removed).
pub fn sha256_without_stamp(facts: &RustcFacts, obj: &Path, scratch: &Path) -> Result<String, String> {
    let objcopy = llvm_tool(facts, "llvm-objcopy")?;
    if let Some(dir) = scratch.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = std::fs::remove_file(scratch);
    let obj_s = obj.to_string_lossy().into_owned();
    let out_s = scratch.to_string_lossy().into_owned();
    let stdout = scratch.with_extension("objcopy.stdout");
    let stderr = scratch.with_extension("objcopy.stderr");
    let run = run_captured(&objcopy, &["--remove-section", ".comment", &obj_s, &out_s], &stdout, &stderr, Duration::from_secs(60))?;
    if run.exit != 0 {
        let reason = std::fs::read_to_string(&stderr).unwrap_or_default();
        let reason = reason.lines().next().unwrap_or("").trim();
        return Err(format!("llvm-objcopy failed on {} (exit {}): {reason}", obj.display(), run.exit));
    }
    sha256_file(scratch).map_err(|e| format!("{}: {e}", scratch.display()))
}
