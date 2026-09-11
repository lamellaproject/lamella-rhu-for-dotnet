//! The image reader the gates share: the ELF32 program headers of an
//! artifact, words read at a virtual address through its load segments,
//! and the two records every RHU image states about itself -- the
//! in-image header (`__rhu_image_header`, section `.rhu_header` at the
//! image base + 0x400: layout version 1, sixteen words -- magic, layout
//! version, size, load address, hosting-API contract generation,
//! debug-layout version, reserved) and the debug-visible layout record
//! (`__rhu_debug_layout`, version 2 and later carry the thread-local-base
//! door: the TCB's per-module block slots). A gate reads both OFF THE
//! ARTIFACT, by symbol through the program headers (the layout record
//! lives inside `.rodata`; a section-name read cannot find it), and ties
//! them: the header at its stated load address + 0x400, that load
//! address the lowest load segment's physical address, the two records'
//! layout versions equal, the generation equal to the RHU_ABI the gate
//! read from include/rhu.h. The kernel sources (src/kernel/src/header.rs
//! and sched.rs) state the layouts; this file only reads them. The
//! ELF32 helpers were the payload gate's; they moved here when a second
//! gate needed them (behavior unchanged).

use crate::text::word_sym_value;

pub const PT_LOAD: u32 = 1;
pub const PT_TLS: u32 = 7;
/// The header's offset from the image base, the same on every board.
pub const HEADER_OFFSET: u64 = 0x400;
pub const HEADER_MAGIC: u32 = 0x4955_4852;
pub const HEADER_LAYOUT: u32 = 1;
pub const HEADER_BYTES: u32 = 64;
pub const LAYOUT_MAGIC: u32 = 0x4448_5552;
/// The debug-visible layout version the thread-local-base door arrived
/// with; an image whose record predates it has no door to read.
pub const LAYOUT_DOOR_VERSION: u32 = 2;

#[derive(Clone, Copy)]
pub struct Phdr {
    pub p_type: u32,
    pub p_offset: u64,
    pub p_vaddr: u64,
    pub p_paddr: u64,
    pub p_filesz: u64,
    pub p_memsz: u64,
    pub p_align: u64,
}

/// Program headers of an ELF32 little-endian image (the artifact class
/// every board map produces). None when the bytes are not that shape.
pub fn elf32_phdrs(elf: &[u8]) -> Option<Vec<Phdr>> {
    if elf.len() < 52 || &elf[0..4] != b"\x7fELF" || elf[4] != 1 || elf[5] != 1 {
        return None;
    }
    let u16_at = |o: usize| u16::from_le_bytes([elf[o], elf[o + 1]]);
    let u32_at = |o: usize| u32::from_le_bytes([elf[o], elf[o + 1], elf[o + 2], elf[o + 3]]);
    let phoff = u32_at(28) as usize;
    let phentsize = u16_at(42) as usize;
    let phnum = u16_at(44) as usize;
    if phentsize < 32 {
        return None;
    }
    let mut out = Vec::with_capacity(phnum);
    for i in 0..phnum {
        let b = phoff + i * phentsize;
        if b + 32 > elf.len() {
            return None;
        }
        out.push(Phdr {
            p_type: u32_at(b),
            p_offset: u32_at(b + 4) as u64,
            p_vaddr: u32_at(b + 8) as u64,
            p_paddr: u32_at(b + 12) as u64,
            p_filesz: u32_at(b + 16) as u64,
            p_memsz: u32_at(b + 20) as u64,
            p_align: u32_at(b + 28) as u64,
        });
    }
    Some(out)
}

/// `n` little-endian words at `vaddr`, from the load segment whose FILE
/// bytes carry them (zero-fill never counts). None when no segment does.
pub fn read_words(elf: &[u8], phdrs: &[Phdr], vaddr: u64, n: usize) -> Option<Vec<u32>> {
    let bytes = (n * 4) as u64;
    let p = phdrs
        .iter()
        .find(|p| p.p_type == PT_LOAD && p.p_vaddr <= vaddr && vaddr + bytes <= p.p_vaddr + p.p_filesz)?;
    let at = (p.p_offset + (vaddr - p.p_vaddr)) as usize;
    let raw = elf.get(at..at + n * 4)?;
    Some(raw.chunks_exact(4).map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect())
}

/// What a passing record read reports.
pub struct ImageRecords {
    pub header_at: u64,
    pub load_addr: u32,
    pub abi: u32,
    pub layout_version: u32,
    pub layout_at: u64,
    pub max_threads: u32,
    pub tcb_stride: u32,
    pub tcb_off_tls: u32,
    pub tls_slots: u32,
    pub tls_slot_bytes: u32,
}

/// The two records of one linked image, read by symbol through the load
/// segments and tied. `abi` is the RHU_ABI the caller read from
/// include/rhu.h. Every refusal names what it found.
pub fn check_records(dump: &[u8], elf: &[u8], abi: u32) -> Result<ImageRecords, String> {
    let sym = |s: &str| {
        word_sym_value(dump, s)
            .ok_or_else(|| format!("symbol '{s}' not value-parseable from the linked artifact"))
    };
    let header_at = sym("__rhu_image_header")?;
    let layout_at = sym("__rhu_debug_layout")?;
    let phdrs = elf32_phdrs(elf)
        .ok_or_else(|| "linked artifact is not an ELF32 little-endian image".to_string())?;
    let low = phdrs
        .iter()
        .filter(|p| p.p_type == PT_LOAD && p.p_filesz > 0)
        .map(|p| p.p_paddr)
        .min()
        .ok_or_else(|| "no load segment carries file bytes".to_string())?;
    let h = read_words(elf, &phdrs, header_at, 16)
        .ok_or_else(|| "image header bytes not carried by a load segment".to_string())?;
    if h[0] != HEADER_MAGIC {
        return Err(format!("image header magic {:#010x}, wanted {HEADER_MAGIC:#010x}", h[0]));
    }
    if h[1] != HEADER_LAYOUT || h[2] != HEADER_BYTES {
        return Err(format!(
            "image header layout {} / {} bytes, wanted {HEADER_LAYOUT} / {HEADER_BYTES}",
            h[1], h[2]
        ));
    }
    let load = h[3];
    if header_at != load as u64 + HEADER_OFFSET {
        return Err(format!(
            "image header at {header_at:#x}, its stated load address + 0x400 is {:#x}",
            load as u64 + HEADER_OFFSET
        ));
    }
    if load as u64 != low {
        return Err(format!(
            "image header states load address {load:#010x}, the lowest load segment sits at {low:#010x}"
        ));
    }
    if h[6..].iter().any(|&w| w != 0) {
        return Err("image header reserved words not zero".to_string());
    }
    if h[4] != abi {
        return Err(format!(
            "image header states hosting-API contract generation {}, include/rhu.h states {abi}",
            h[4]
        ));
    }
    let l = read_words(elf, &phdrs, layout_at, 16)
        .ok_or_else(|| "debug-layout record bytes not carried by a load segment".to_string())?;
    if l[0] != LAYOUT_MAGIC {
        return Err(format!("debug-layout record magic {:#010x}, wanted {LAYOUT_MAGIC:#010x}", l[0]));
    }
    if l[1] < LAYOUT_DOOR_VERSION {
        return Err(format!(
            "debug-layout record version {} predates the thread-local-base door (version {LAYOUT_DOOR_VERSION})",
            l[1]
        ));
    }
    if h[5] != l[1] {
        return Err(format!(
            "image header states debug-layout version {}, the record carries {}",
            h[5], l[1]
        ));
    }
    let (stride, off_tls, slots, slot_bytes) = (l[5], l[13], l[14], l[15]);
    if slot_bytes != 4 || slots == 0 || off_tls.saturating_add(slots.saturating_mul(slot_bytes)) > stride {
        return Err(format!(
            "debug-layout tls slots ({slots} x {slot_bytes} bytes at offset {off_tls}) overrun the TCB stride {stride}"
        ));
    }
    Ok(ImageRecords {
        header_at,
        load_addr: load,
        abi: h[4],
        layout_version: l[1],
        layout_at,
        max_threads: l[2],
        tcb_stride: stride,
        tcb_off_tls: off_tls,
        tls_slots: slots,
        tls_slot_bytes: slot_bytes,
    })
}

/// RHU_ABI as the header states it: the gate reads include/rhu.h
/// itself, so the generation the kernel prints is tied to the header
/// at every run. A header without the define fails the gate.
pub fn header_abi(text: &str) -> Option<u32> {
    text.lines().find_map(|l| {
        let rest = l.trim_end_matches('\r').strip_prefix("#define RHU_ABI")?.trim();
        if rest.is_empty() || !rest.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        rest.parse().ok()
    })
}

/// `RHU_ABI_COMPAT` as `include/rhu.h` states it: the oldest generation
/// the kernel's entry handshake still runs (the window's floor).
pub fn header_abi_compat(text: &str) -> Option<u32> {
    text.lines().find_map(|l| {
        let rest = l.trim_end_matches('\r').strip_prefix("#define RHU_ABI_COMPAT")?.trim();
        if rest.is_empty() || !rest.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        rest.parse().ok()
    })
}

/// A minimal ELF32 little-endian image for the selfchecks' probes: the
/// given program headers (type, offset, vaddr, paddr, filesz, memsz,
/// align) and `data` placed at file offset `data_at`.
pub fn synth_elf32(phdrs: &[(u32, u32, u32, u32, u32, u32, u32)], data_at: usize, data: &[u8]) -> Vec<u8> {
    let mut e = vec![0u8; 52];
    e[0..4].copy_from_slice(b"\x7fELF");
    e[4] = 1;
    e[5] = 1;
    e[6] = 1;
    e[16..18].copy_from_slice(&2u16.to_le_bytes());
    e[18..20].copy_from_slice(&40u16.to_le_bytes());
    e[28..32].copy_from_slice(&52u32.to_le_bytes());
    e[40..42].copy_from_slice(&52u16.to_le_bytes());
    e[42..44].copy_from_slice(&32u16.to_le_bytes());
    e[44..46].copy_from_slice(&(phdrs.len() as u16).to_le_bytes());
    for &(t, off, va, pa, fsz, msz, al) in phdrs {
        for v in [t, off, va, pa, fsz, msz, 6, al] {
            e.extend_from_slice(&v.to_le_bytes());
        }
    }
    if e.len() < data_at + data.len() {
        e.resize(data_at + data.len(), 0);
    }
    e[data_at..data_at + data.len()].copy_from_slice(data);
    e
}

/// A synthesized image and symbol dump carrying both records, for the
/// selfchecks: one load segment at virtual address `load` with physical
/// address `paddr` (equal in a genuine image), the header words at
/// `load + 0x400`, the layout words at `load + 0x800`.
pub fn synth_records(load: u32, paddr: u32, header: &[u32; 16], layout: &[u32; 16]) -> (Vec<u8>, Vec<u8>) {
    let mut data = Vec::new();
    for w in header {
        data.extend_from_slice(&w.to_le_bytes());
    }
    data.resize(0x400, 0);
    for w in layout {
        data.extend_from_slice(&w.to_le_bytes());
    }
    let elf = synth_elf32(&[(PT_LOAD, 0x1000, load, paddr, 0x840, 0x840, 4)], 0x1000 + 0x400, &data);
    let dump = format!(
        "{:08x} R __vector_table\n{:08x} R __rhu_image_header\n{:08x} R __rhu_debug_layout\n",
        load,
        load + 0x400,
        load + 0x800
    )
    .into_bytes();
    (dump, elf)
}
