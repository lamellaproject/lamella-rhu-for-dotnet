//! The gates' own client of the emulator's debug port, the walk of a
//! stopped image's stack by its exception index table, and the reading and
//! the four runs of the debugger's unhandled-exception event.
//!
//! The emulator opens a debug port that speaks the remote serial protocol
//! of the GNU debugger. A gate that must see what a debugger sees -- a
//! word written into the image's RAM, a stop at an address, the registers
//! and the stack at that stop -- connects to the port itself: the
//! verification tier carries no debugger, and the packets it needs are
//! plain text over one loopback connection. The client asks for the
//! target description once, so that the emulator answers the
//! one-register packets, and reads and writes registers one at a time;
//! it never depends on the layout of the all-registers packet.
//!
//! The walk uses the image's exception index table alone (the sections of
//! type SHT_ARM_EXIDX and the tables they point into): no debug
//! information and no instruction reading. It is what a debugger is
//! promised at the unhandled event, so it is what the gate walks with.

use std::fs::File;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use crate::text::contains_fixed;
use crate::tools::free_port;

/// The checksum of a packet's payload: the sum of its bytes modulo 256.
pub fn packet_checksum(payload: &[u8]) -> u8 {
    payload.iter().fold(0u8, |a, b| a.wrapping_add(*b))
}

/// A payload as the protocol frames it: `$`, the payload, `#` and the
/// checksum's two hexadecimal digits.
pub fn frame_packet(payload: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(payload.len() + 4);
    out.push(b'$');
    out.extend_from_slice(payload.as_bytes());
    out.extend_from_slice(format!("#{:02x}", packet_checksum(payload.as_bytes())).as_bytes());
    out
}

/// The first whole packet of a received byte stream: its payload, with the
/// protocol's escapes and run-length codes undone, and the number of bytes
/// it took. Acknowledgements before the packet are skipped. `Ok(None)`
/// says the stream holds no whole packet yet; a checksum that does not
/// match, a refused packet (`-`) or a byte that begins no packet is an
/// error.
pub fn unframe_packet(bytes: &[u8]) -> Result<Option<(Vec<u8>, usize)>, String> {
    let mut i = 0;
    while i < bytes.len() && bytes[i] == b'+' {
        i += 1;
    }
    if i == bytes.len() {
        return Ok(None);
    }
    if bytes[i] == b'-' {
        return Err("the debug port refused a packet (it answered '-')".to_string());
    }
    if bytes[i] != b'$' {
        return Err(format!("the debug port sent a byte that begins no packet (0x{:02x})", bytes[i]));
    }
    let start = i + 1;
    let Some(hash) = bytes[start..].iter().position(|b| *b == b'#') else {
        return Ok(None);
    };
    let end = start + hash;
    if bytes.len() < end + 3 {
        return Ok(None);
    }
    let raw = &bytes[start..end];
    let stated = std::str::from_utf8(&bytes[end + 1..end + 3]).ok().and_then(|s| u8::from_str_radix(s, 16).ok());
    if stated != Some(packet_checksum(raw)) {
        return Err(format!(
            "a packet's checksum does not match its payload (stated {}, computed {:02x})",
            String::from_utf8_lossy(&bytes[end + 1..end + 3]),
            packet_checksum(raw)
        ));
    }
    let mut payload = Vec::with_capacity(raw.len());
    let mut k = 0;
    while k < raw.len() {
        match raw[k] {
            b'}' if k + 1 < raw.len() => {
                payload.push(raw[k + 1] ^ 0x20);
                k += 2;
            }
            b'*' if k + 1 < raw.len() && !payload.is_empty() => {
                let last = payload[payload.len() - 1];
                for _ in 0..raw[k + 1].saturating_sub(29) {
                    payload.push(last);
                }
                k += 2;
            }
            b => {
                payload.push(b);
                k += 1;
            }
        }
    }
    Ok(Some((payload, end + 3)))
}

/// What the emulator reports when the machine stops or ends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Halt {
    /// The machine stopped with this signal number (5 at a breakpoint and
    /// at the first halt).
    Stopped(u8),
    /// The machine ended with this exit code.
    Exited(u8),
    /// The port closed with no report.
    Closed,
}

/// A stop reply read as a halt: `S` or `T` and a signal's two digits, or
/// `W` and an exit code's. Any other reply is an error.
pub fn parse_halt(reply: &str) -> Result<Halt, String> {
    let code = reply.get(1..3).and_then(|s| u8::from_str_radix(s, 16).ok());
    match (reply.as_bytes().first(), code) {
        (Some(b'S') | Some(b'T'), Some(c)) => Ok(Halt::Stopped(c)),
        (Some(b'W'), Some(c)) => Ok(Halt::Exited(c)),
        _ => Err(format!("the debug port's reply is no stop and no exit: '{}'", reply.chars().take(40).collect::<String>())),
    }
}

/// Eight hexadecimal digits in the target's byte order, least significant
/// byte first, as a word.
pub fn word_from_wire(hex: &str) -> Option<u32> {
    if hex.len() != 8 || !hex.is_ascii() {
        return None;
    }
    let mut v = 0u32;
    for i in 0..4 {
        v |= (u8::from_str_radix(&hex[2 * i..2 * i + 2], 16).ok()? as u32) << (8 * i);
    }
    Some(v)
}

/// A word as eight hexadecimal digits in the target's byte order.
pub fn word_to_wire(v: u32) -> String {
    v.to_le_bytes().iter().map(|b| format!("{b:02x}")).collect()
}

/// One connection to the emulator's debug port. Every packet sent and
/// every reply is kept in `log`, in order, for the run's record.
pub struct DebugPort {
    stream: TcpStream,
    pending: Vec<u8>,
    reply_timeout: Duration,
    pub log: Vec<String>,
}

impl DebugPort {
    /// Connect to the port on the loopback interface, retrying every 25 ms
    /// until the emulator listens or the deadline passes.
    pub fn connect(port: u16, deadline: Duration) -> Result<DebugPort, String> {
        let start = Instant::now();
        let stream = loop {
            match TcpStream::connect(("127.0.0.1", port)) {
                Ok(s) => break s,
                Err(e) => {
                    if start.elapsed() >= deadline {
                        return Err(format!("no connection to the debug port {port} within {} s: {e}", deadline.as_secs()));
                    }
                    std::thread::sleep(Duration::from_millis(25));
                }
            }
        };
        let _ = stream.set_nodelay(true);
        stream
            .set_write_timeout(Some(Duration::from_secs(5)))
            .map_err(|e| format!("the debug port's write timeout cannot be set: {e}"))?;
        Ok(DebugPort { stream, pending: Vec::new(), reply_timeout: Duration::from_secs(10), log: Vec::new() })
    }

    fn send(&mut self, payload: &str) -> Result<(), String> {
        self.log.push(format!("> {payload}"));
        self.stream.write_all(&frame_packet(payload)).map_err(|e| format!("the packet '{payload}' was not sent: {e}"))
    }

    /// The next packet's payload, acknowledged; `None` when the peer
    /// closed the connection first.
    fn receive(&mut self, timeout: Duration) -> Result<Option<String>, String> {
        let deadline = Instant::now() + timeout;
        let mut buf = [0u8; 4096];
        loop {
            if let Some((payload, used)) = unframe_packet(&self.pending)? {
                self.pending.drain(..used);
                // The acknowledgement of an exit report meets a port that is already closing: its failure says nothing.
                let _ = self.stream.write_all(b"+");
                let text = String::from_utf8_lossy(&payload).into_owned();
                self.log.push(format!("< {}", text.chars().take(120).collect::<String>()));
                return Ok(Some(text));
            }
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return Err(format!("the debug port sent no reply within {} s", timeout.as_secs()));
            }
            self.stream
                .set_read_timeout(Some(left.min(Duration::from_millis(250))))
                .map_err(|e| format!("the debug port's read timeout cannot be set: {e}"))?;
            match self.stream.read(&mut buf) {
                Ok(0) => {
                    self.log.push("< (the port closed)".to_string());
                    return Ok(None);
                }
                Ok(n) => self.pending.extend_from_slice(&buf[..n]),
                Err(e) if matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut) => {}
                // A reset connection is the emulator's exit seen from this side.
                Err(_) => {
                    self.log.push("< (the port closed)".to_string());
                    return Ok(None);
                }
            }
        }
    }

    fn ask(&mut self, payload: &str) -> Result<String, String> {
        self.send(payload)?;
        match self.receive(self.reply_timeout)? {
            Some(r) => Ok(r),
            None => Err(format!("the debug port closed before it answered '{payload}'")),
        }
    }

    fn ask_ok(&mut self, payload: &str, what: &str) -> Result<(), String> {
        match self.ask(payload)?.as_str() {
            "OK" => Ok(()),
            other => Err(format!("{what}: the debug port answered '{other}' to '{payload}'")),
        }
    }

    /// The session's opening: the target description asked for (the
    /// emulator answers the one-register packets after it), then why the
    /// machine is halted.
    pub fn open(&mut self) -> Result<Halt, String> {
        self.ask("qSupported")?;
        self.ask("qXfer:features:read:target.xml:0,ffb")?;
        parse_halt(&self.ask("?")?)
    }

    /// One core register: 0 to 12 for r0 to r12, 13 the stack pointer, 14
    /// the link register, 15 the program counter.
    pub fn register(&mut self, n: u32) -> Result<u32, String> {
        let reply = self.ask(&format!("p{n:x}"))?;
        word_from_wire(&reply).ok_or_else(|| format!("register {n} did not read: the debug port answered '{reply}'"))
    }

    /// The sixteen core registers, r0 first.
    pub fn registers(&mut self) -> Result<[u32; 16], String> {
        let mut regs = [0u32; 16];
        for (n, r) in regs.iter_mut().enumerate() {
            *r = self.register(n as u32)?;
        }
        Ok(regs)
    }

    pub fn set_register(&mut self, n: u32, v: u32) -> Result<(), String> {
        self.ask_ok(&format!("P{n:x}={}", word_to_wire(v)), &format!("register {n} was not written"))
    }

    pub fn read_word(&mut self, addr: u32) -> Result<u32, String> {
        let reply = self.ask(&format!("m{addr:x},4"))?;
        word_from_wire(&reply).ok_or_else(|| format!("the word at {addr:#010x} did not read: the debug port answered '{reply}'"))
    }

    pub fn write_word(&mut self, addr: u32, v: u32) -> Result<(), String> {
        self.ask_ok(&format!("M{addr:x},4:{}", word_to_wire(v)), &format!("the word at {addr:#010x} was not written"))
    }

    /// A breakpoint of the client's own on a Thumb instruction's address.
    pub fn set_breakpoint(&mut self, addr: u32) -> Result<(), String> {
        self.ask_ok(&format!("Z0,{addr:x},2"), &format!("no breakpoint was set at {addr:#010x}"))
    }

    pub fn clear_breakpoint(&mut self, addr: u32) -> Result<(), String> {
        self.ask_ok(&format!("z0,{addr:x},2"), &format!("the breakpoint at {addr:#010x} was not removed"))
    }

    /// Let the machine run until it stops or ends. Console packets the
    /// emulator sends meanwhile are passed over.
    pub fn resume(&mut self, timeout: Duration) -> Result<Halt, String> {
        self.send("c")?;
        let deadline = Instant::now() + timeout;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return Err(format!("the machine neither stopped nor ended within {} s", timeout.as_secs()));
            }
            match self.receive(left) {
                Ok(Some(r)) if r.starts_with('O') && !r.starts_with("OK") => continue,
                Ok(Some(r)) => return parse_halt(&r),
                Ok(None) => return Ok(Halt::Closed),
                Err(e) if e.contains("no reply within") => {
                    return Err(format!("the machine neither stopped nor ended within {} s", timeout.as_secs()));
                }
                Err(e) => return Err(e),
            }
        }
    }
}

/// One symbol of an image's symbol table.
#[derive(Debug, Clone)]
pub struct Symbol {
    pub name: String,
    pub value: u32,
    pub size: u32,
    /// The symbol's type: 0 none, 1 an object, 2 a function.
    pub kind: u8,
    pub defined: bool,
}

struct Section {
    kind: u32,
    flags: u32,
    addr: u32,
    off: usize,
    size: usize,
    link: usize,
}

/// One entry of the exception index table: the function start it covers
/// from, its second word and that word's address.
#[derive(Debug, Clone, Copy)]
struct IndexEntry {
    start: u32,
    word: u32,
    at: u32,
}

/// A linked 32-bit little-endian Arm image read for what a walker needs:
/// its symbol table, its loaded bytes by address and its exception index
/// table.
pub struct ArmImage {
    bytes: Vec<u8>,
    sections: Vec<Section>,
    pub symbols: Vec<Symbol>,
    functions: Vec<(u32, u32, usize)>,
    entries: Vec<IndexEntry>,
}

const SHT_SYMTAB: u32 = 2;
const SHT_NOBITS: u32 = 8;
const SHT_ARM_EXIDX: u32 = 0x7000_0001;
const SHF_ALLOC: u32 = 2;

fn le16(d: &[u8], at: usize) -> Option<u32> {
    d.get(at..at + 2).map(|b| u16::from_le_bytes([b[0], b[1]]) as u32)
}

fn le32(d: &[u8], at: usize) -> Option<u32> {
    d.get(at..at + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

fn prel31(word: u32, place: u32) -> u32 {
    let mut off = word & 0x7fff_ffff;
    if off & 0x4000_0000 != 0 {
        off |= 0x8000_0000;
    }
    place.wrapping_add(off)
}

impl ArmImage {
    /// Read an image's bytes. An image that is no 32-bit little-endian ELF
    /// or that carries no symbol table is refused by name.
    pub fn read(bytes: Vec<u8>) -> Result<ArmImage, String> {
        let d = &bytes;
        if d.len() < 52 || &d[..4] != b"\x7fELF" || d[4] != 1 || d[5] != 1 {
            return Err("the image is no 32-bit little-endian ELF file".to_string());
        }
        let bad = || "the image's section table does not read".to_string();
        let shoff = le32(d, 0x20).ok_or_else(bad)? as usize;
        let shentsize = le16(d, 0x2e).ok_or_else(bad)? as usize;
        let shnum = le16(d, 0x30).ok_or_else(bad)? as usize;
        if shoff == 0 || shnum == 0 || shentsize < 40 {
            return Err("the image has no section table".to_string());
        }
        let mut sections = Vec::with_capacity(shnum);
        for i in 0..shnum {
            let at = shoff + i * shentsize;
            sections.push(Section {
                kind: le32(d, at + 4).ok_or_else(bad)?,
                flags: le32(d, at + 8).ok_or_else(bad)?,
                addr: le32(d, at + 12).ok_or_else(bad)?,
                off: le32(d, at + 16).ok_or_else(bad)? as usize,
                size: le32(d, at + 20).ok_or_else(bad)? as usize,
                link: le32(d, at + 24).ok_or_else(bad)? as usize,
            });
        }
        let mut symbols = Vec::new();
        let mut tables = 0;
        for s in sections.iter().filter(|s| s.kind == SHT_SYMTAB) {
            tables += 1;
            let strings = sections.get(s.link).ok_or_else(|| "the symbol table names no string table".to_string())?;
            for i in 0..s.size / 16 {
                let at = s.off + 16 * i;
                let (Some(name_off), Some(value), Some(size), Some(info), Some(shndx)) =
                    (le32(d, at), le32(d, at + 4), le32(d, at + 8), d.get(at + 12), le16(d, at + 14))
                else {
                    return Err("the image's symbol table does not read".to_string());
                };
                let from = strings.off + name_off as usize;
                let name = match d.get(from..).and_then(|t| t.iter().position(|b| *b == 0).map(|n| &t[..n])) {
                    Some(n) => String::from_utf8_lossy(n).into_owned(),
                    None => return Err("a symbol's name does not read".to_string()),
                };
                symbols.push(Symbol { name, value, size, kind: info & 15, defined: shndx != 0 });
            }
        }
        if tables == 0 {
            return Err("the image has no symbol table".to_string());
        }
        let mut functions: Vec<(u32, u32, usize)> =
            symbols.iter().enumerate().filter(|(_, s)| s.kind == 2 && s.defined).map(|(i, s)| (s.value & !1, s.size, i)).collect();
        functions.sort();
        let mut entries = Vec::new();
        for s in sections.iter().filter(|s| s.kind == SHT_ARM_EXIDX) {
            for i in 0..s.size / 8 {
                let at = s.addr.wrapping_add(8 * i as u32);
                let (Some(w0), Some(w1)) = (le32(d, s.off + 8 * i), le32(d, s.off + 8 * i + 4)) else {
                    return Err("the image's exception index table does not read".to_string());
                };
                entries.push(IndexEntry { start: prel31(w0, at) & !1, word: w1, at: at.wrapping_add(4) });
            }
        }
        entries.sort_by_key(|e| e.start);
        Ok(ArmImage { bytes, sections, symbols, functions, entries })
    }

    /// The defined symbol of this name.
    pub fn symbol(&self, name: &str) -> Option<&Symbol> {
        self.symbols.iter().find(|s| s.defined && s.name == name)
    }

    /// The one defined function whose name ends with the suffix; none, or
    /// more than one, is an error that says so.
    pub fn function_ending(&self, suffix: &str) -> Result<&Symbol, String> {
        let found: Vec<&Symbol> = self.symbols.iter().filter(|s| s.defined && s.kind == 2 && s.name.ends_with(suffix)).collect();
        match found.len() {
            1 => Ok(found[0]),
            n => Err(format!("{n} functions of the image have a name that ends with '{suffix}' (1 expected)")),
        }
    }

    /// The function that holds an address, by the symbol table.
    pub fn function_at(&self, addr: u32) -> Option<&Symbol> {
        let i = self.functions.partition_point(|f| f.0 <= addr);
        if i == 0 {
            return None;
        }
        let (start, size, index) = self.functions[i - 1];
        if size == 0 || addr < start.wrapping_add(size) { Some(&self.symbols[index]) } else { None }
    }

    fn file_offset(&self, addr: u32, len: usize) -> Option<usize> {
        self.sections
            .iter()
            .find(|s| s.kind != SHT_NOBITS && s.flags & SHF_ALLOC != 0 && addr >= s.addr && (addr - s.addr) as usize + len <= s.size)
            .map(|s| s.off + (addr - s.addr) as usize)
    }

    /// The image's own bytes at a loaded address.
    pub fn bytes_at(&self, addr: u32, len: usize) -> Option<&[u8]> {
        let off = self.file_offset(addr, len)?;
        self.bytes.get(off..off + len)
    }

    fn word(&self, addr: u32) -> Option<u32> {
        self.bytes_at(addr, 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    fn entry_for(&self, addr: u32) -> Option<IndexEntry> {
        let i = self.entries.partition_point(|e| e.start <= addr);
        if i == 0 { None } else { Some(self.entries[i - 1]) }
    }

    /// The unwind opcodes of an index-table entry and the entry's kind:
    /// `inline`, `table-pr0` to `table-pr2`, `table-own-routine`, or, with
    /// no opcodes, `cannot-unwind` and `table-unread`.
    fn opcodes(&self, e: IndexEntry) -> (String, Option<Vec<u8>>) {
        if e.word == 1 {
            return ("cannot-unwind".to_string(), None);
        }
        let three = |w: u32| vec![(w >> 16) as u8, (w >> 8) as u8, w as u8];
        if e.word & 0x8000_0000 != 0 {
            return ("inline".to_string(), Some(three(e.word)));
        }
        let table = prel31(e.word, e.at);
        let Some(w) = self.word(table) else {
            return ("table-unread".to_string(), None);
        };
        let (kind, mut ops, more, base) = if w & 0x8000_0000 != 0 {
            let index = (w >> 24) & 15;
            if index == 0 {
                return ("table-pr0".to_string(), Some(three(w)));
            }
            (format!("table-pr{index}"), vec![(w >> 8) as u8, w as u8], (w >> 16) & 255, table.wrapping_add(4))
        } else {
            let Some(x) = self.word(table.wrapping_add(4)) else {
                return ("table-unread".to_string(), None);
            };
            ("table-own-routine".to_string(), three(x), (x >> 24) & 255, table.wrapping_add(8))
        };
        for k in 0..more {
            let Some(y) = self.word(base.wrapping_add(4 * k)) else {
                return ("table-unread".to_string(), None);
            };
            ops.extend_from_slice(&y.to_be_bytes());
        }
        (kind, Some(ops))
    }
}

/// One frame up by an entry's unwind opcodes: the sixteen registers are
/// changed in place to the caller's. The stack is read through `read32`;
/// an opcode that refuses to unwind, one this reader does not know, or a
/// stack word that does not read is an error.
pub fn unwind_step(regs: &mut [u32; 16], ops: &[u8], read32: &mut dyn FnMut(u32) -> Option<u32>) -> Result<(), String> {
    let mut vsp = regs[13];
    let mut pc_set = false;
    let mut i = 0;
    let short = || "the entry's opcodes end inside an opcode".to_string();
    let mut pop = |regs: &mut [u32; 16], vsp: &mut u32, pc_set: &mut bool, list: &[usize]| -> Result<(), String> {
        for r in list {
            regs[*r] = read32(*vsp).ok_or_else(|| format!("the stack at {:#010x} did not read", *vsp))?;
            *vsp = vsp.wrapping_add(4);
        }
        if list.contains(&13) {
            *vsp = regs[13];
        }
        if list.contains(&15) {
            *pc_set = true;
        }
        Ok(())
    };
    while i < ops.len() {
        let op = ops[i];
        i += 1;
        if op & 0xc0 == 0x00 {
            vsp = vsp.wrapping_add((((op & 0x3f) as u32) << 2) + 4);
        } else if op & 0xc0 == 0x40 {
            vsp = vsp.wrapping_sub((((op & 0x3f) as u32) << 2) + 4);
        } else if op & 0xf0 == 0x80 {
            let mask = (((op & 15) as u32) << 8) | *ops.get(i).ok_or_else(short)? as u32;
            i += 1;
            if mask == 0 {
                return Err("the entry refuses to unwind".to_string());
            }
            let list: Vec<usize> = (0..12).filter(|b| mask & (1 << b) != 0).map(|b| 4 + b).collect();
            pop(&mut *regs, &mut vsp, &mut pc_set, &list)?;
        } else if op & 0xf0 == 0x90 {
            if matches!(op & 15, 13 | 15) {
                return Err(format!("a reserved opcode {op:#04x}"));
            }
            vsp = regs[(op & 15) as usize];
        } else if op & 0xf8 == 0xa0 {
            let list: Vec<usize> = (4..5 + (op & 7) as usize).collect();
            pop(&mut *regs, &mut vsp, &mut pc_set, &list)?;
        } else if op & 0xf8 == 0xa8 {
            let mut list: Vec<usize> = (4..5 + (op & 7) as usize).collect();
            list.push(14);
            pop(&mut *regs, &mut vsp, &mut pc_set, &list)?;
        } else if op == 0xb0 {
            break;
        } else if op == 0xb1 {
            let m = *ops.get(i).ok_or_else(short)?;
            i += 1;
            if m == 0 || m & 0xf0 != 0 {
                return Err(format!("a spare opcode 0xb1 {m:#04x}"));
            }
            let list: Vec<usize> = (0..4).filter(|b| m & (1 << b) != 0).collect();
            pop(&mut *regs, &mut vsp, &mut pc_set, &list)?;
        } else if op == 0xb2 {
            let (mut value, mut shift) = (0u32, 0u32);
            loop {
                let b = *ops.get(i).ok_or_else(short)?;
                i += 1;
                value |= ((b & 0x7f) as u32) << shift;
                shift += 7;
                if b & 0x80 == 0 || shift > 28 {
                    break;
                }
            }
            vsp = vsp.wrapping_add(0x204 + (value << 2));
        } else if op == 0xb3 {
            vsp = vsp.wrapping_add((((*ops.get(i).ok_or_else(short)? & 15) as u32) + 1) * 8 + 4);
            i += 1;
        } else if op & 0xf8 == 0xb8 {
            vsp = vsp.wrapping_add((((op & 7) as u32) + 1) * 8 + 4);
        } else if op == 0xc8 || op == 0xc9 {
            vsp = vsp.wrapping_add((((*ops.get(i).ok_or_else(short)? & 15) as u32) + 1) * 8);
            i += 1;
        } else if op & 0xf8 == 0xd0 {
            vsp = vsp.wrapping_add((((op & 7) as u32) + 1) * 8);
        } else {
            return Err(format!("an opcode {op:#04x} this reader does not know"));
        }
    }
    if !pc_set {
        regs[15] = regs[14];
    }
    regs[13] = vsp;
    Ok(())
}

/// One frame of a walk.
#[derive(Debug, Clone)]
pub struct Frame {
    pub pc: u32,
    pub sp: u32,
    /// The function that holds the frame's address, by the symbol table.
    pub name: Option<String>,
    /// Whether the covering entry begins at that function's own start.
    pub own: bool,
    /// The covering entry's kind, or `no-entry`.
    pub kind: String,
    /// Why the walk ended at this frame; `None` on every frame but the last.
    pub end: Option<String>,
}

/// The frames from a set of registers up, each step by the unwind opcodes
/// of the index-table entry that covers the frame's address. The first
/// frame is looked up at its own address and every caller two bytes
/// before its return address, inside the call. The walk ends at the first
/// frame whose entry gives no opcodes, whose step fails or moves nothing,
/// or whose caller's address is zero, and at the limit.
pub fn walk(img: &ArmImage, start: [u32; 16], read32: &mut dyn FnMut(u32) -> Option<u32>, limit: usize) -> Vec<Frame> {
    let mut regs = start;
    let mut out: Vec<Frame> = Vec::new();
    for n in 0..limit {
        let pc = regs[15] & !1;
        let look = if n == 0 { pc } else { pc.wrapping_sub(2) };
        let entry = img.entry_for(look);
        let function = img.function_at(look);
        let (kind, ops) = match entry {
            Some(e) => img.opcodes(e),
            None => ("no-entry".to_string(), None),
        };
        let own = matches!((entry, function), (Some(e), Some(f)) if e.start == f.value & !1);
        out.push(Frame { pc, sp: regs[13], name: function.map(|f| f.name.clone()), own, kind: kind.clone(), end: None });
        let last = out.len() - 1;
        let Some(ops) = ops else {
            out[last].end = Some(format!("the entry that covers this frame is {kind}"));
            return out;
        };
        let before = (regs[15], regs[13]);
        if let Err(why) = unwind_step(&mut regs, &ops, read32) {
            out[last].end = Some(why);
            return out;
        }
        if (regs[15], regs[13]) == before {
            out[last].end = Some("the step moved nothing".to_string());
            return out;
        }
        if matches!(regs[15] & !1, 0 | 0xffff_fffe) {
            out[last].end = Some(format!("the caller's address is {:#010x}", regs[15]));
            return out;
        }
    }
    if let Some(f) = out.last_mut() {
        f.end = Some(format!("the limit of {limit} frames"));
    }
    out
}

/// The word a debugger writes to ask for events, the shim's routine that
/// raises the unhandled event, and the event's instruction.
pub const EVENT_WORD: &str = "__lamella_debug_events";
pub const EVENT_ROUTINE: &str = "rhu_debug_event_unhandled";
/// The magic of the word's high half with the unhandled event's bit.
pub const EVENT_WORD_UNHANDLED: u32 = 0x4442_0001;
/// `BKPT #0x60` as the Thumb instruction's sixteen bits, and the
/// no-operation the control's doctored image carries in its place.
const EVENT_BKPT: u16 = 0xbe60;
const THUMB_NOP: u16 = 0xbf00;

/// What an image states about the unhandled event, read off the artifact.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EventFacts {
    /// The address of the events word.
    pub word: u32,
    /// The routine's address and its size in bytes.
    pub routine: u32,
    pub routine_size: u32,
    /// The address of the routine's one `BKPT #0x60`.
    pub bkpt: u32,
}

/// The addresses of a routine's sixteen-bit instructions equal to `want`,
/// by a walk of its Thumb instructions from its start: a first half of a
/// thirty-two-bit instruction takes its second half with it, and the
/// routine's data, where the symbol table marks it, is passed over.
fn thumb16_sites(img: &ArmImage, start: u32, size: u32, want: u16) -> Vec<u32> {
    let end = start.wrapping_add(size);
    // The mapping symbols inside the routine, in address order: `$d` opens data, `$t` opens Thumb code.
    let mut marks: Vec<(u32, bool)> = img
        .symbols
        .iter()
        .filter(|s| s.defined && s.value >= start && s.value < end && (s.name == "$d" || s.name.starts_with("$d.") || s.name == "$t" || s.name.starts_with("$t.")))
        .map(|s| (s.value, s.name.starts_with("$d")))
        .collect();
    marks.sort();
    let is_data = |a: u32| marks.iter().rev().find(|m| m.0 <= a).map(|m| m.1).unwrap_or(false);
    let mut found = Vec::new();
    let mut a = start;
    while a.wrapping_add(2) <= end {
        if is_data(a) {
            a = a.wrapping_add(2);
            continue;
        }
        let Some(b) = img.bytes_at(a, 2) else { break };
        let half = u16::from_le_bytes([b[0], b[1]]);
        if half & 0xe000 == 0xe000 && half & 0x1800 != 0 {
            a = a.wrapping_add(4);
            continue;
        }
        if half == want {
            found.push(a);
        }
        a = a.wrapping_add(2);
    }
    found
}

/// The event's reading of an image: the events word is a four-byte object
/// inside the zero-filled window `[__sbss, __ebss)`; the routine is a
/// function with an index-table entry of its own whose opcodes unwind;
/// the routine holds exactly one `BKPT #0x60`. A reading that fails says
/// which fact is missing.
pub fn read_event_image(img: &ArmImage) -> Result<EventFacts, String> {
    let word = img.symbol(EVENT_WORD).ok_or_else(|| format!("the image defines no {EVENT_WORD}"))?;
    if word.size != 4 || word.value % 4 != 0 {
        return Err(format!("{EVENT_WORD} is {} bytes at {:#010x}, not one aligned word", word.size, word.value));
    }
    let (Some(lo), Some(hi)) = (img.symbol("__sbss"), img.symbol("__ebss")) else {
        return Err("the image does not name its zero-filled window (__sbss, __ebss)".to_string());
    };
    if word.value < lo.value || word.value.wrapping_add(4) > hi.value {
        return Err(format!("{EVENT_WORD} at {:#010x} lies outside the zero-filled window [{:#010x}, {:#010x})", word.value, lo.value, hi.value));
    }
    let routine = img.symbol(EVENT_ROUTINE).filter(|s| s.kind == 2).ok_or_else(|| format!("the image defines no function {EVENT_ROUTINE}"))?;
    let start = routine.value & !1;
    if routine.size == 0 || img.bytes_at(start, routine.size as usize).is_none() {
        return Err(format!("the routine {EVENT_ROUTINE} has no bytes in the image"));
    }
    let sites = thumb16_sites(img, start, routine.size, EVENT_BKPT);
    let bkpt = match sites.len() {
        1 => sites[0],
        0 => return Err("the routine holds no BKPT #0x60".to_string()),
        n => return Err(format!("the routine holds {n} BKPT #0x60 instructions (1 expected)")),
    };
    let entry = img.entry_for(start).filter(|e| e.start == start).ok_or_else(|| "the routine has no index-table entry of its own".to_string())?;
    let (kind, ops) = img.opcodes(entry);
    let Some(ops) = ops else {
        return Err(format!("the routine's index-table entry does not unwind ({kind})"));
    };
    // The entry is tried on registers of the reader's own: it must take the caller's address from the link register
    // and read no stack.
    let mut regs = [0u32; 16];
    regs[13] = 0x2000_0000;
    regs[14] = 0x0000_1235;
    regs[15] = bkpt;
    let mut no_stack = |_: u32| None;
    match unwind_step(&mut regs, &ops, &mut no_stack) {
        Ok(()) if regs[15] == 0x0000_1235 && regs[13] == 0x2000_0000 => {}
        Ok(()) => return Err("the routine's index-table entry does not say that the return address is in the link register".to_string()),
        Err(why) => return Err(format!("the routine's index-table entry does not unwind ({why})")),
    }
    Ok(EventFacts { word: word.value, routine: start, routine_size: routine.size, bkpt })
}

/// The control's doctored input: a copy of an image's bytes with the two
/// bytes of the routine's `BKPT #0x60` overwritten by a no-operation.
pub fn doctor_event_bkpt(bytes: &[u8]) -> Result<Vec<u8>, String> {
    let img = ArmImage::read(bytes.to_vec())?;
    let facts = read_event_image(&img)?;
    let off = img.file_offset(facts.bkpt, 2).ok_or_else(|| "the BKPT's bytes are not in the file".to_string())?;
    let mut out = bytes.to_vec();
    out[off..off + 2].copy_from_slice(&THUMB_NOP.to_le_bytes());
    Ok(out)
}

/// The emulator started halted with its debug port open and connected,
/// its console captured to a file. Semihosting stays on the emulator's
/// own side, so the image's exit ends the emulator with the image's code
/// as it does with no debugger. Dropping a session ends the emulator.
pub struct Session {
    child: Child,
    pub port: DebugPort,
    console: PathBuf,
}

impl Session {
    pub fn start(qemu: &Path, machine: &str, image: &Path, console: &Path, stderr: &Path) -> Result<Session, String> {
        let tcp = free_port()?;
        let out = File::create(console).map_err(|e| format!("cannot create {}: {e}", console.display()))?;
        let err = File::create(stderr).map_err(|e| format!("cannot create {}: {e}", stderr.display()))?;
        let gdb = format!("tcp:127.0.0.1:{tcp}");
        let mut child = Command::new(qemu)
            .args(["-M", machine, "-display", "none", "-monitor", "none", "-serial", "stdio", "-semihosting-config", "enable=on,target=native", "-kernel"])
            .arg(image)
            .args(["-gdb", gdb.as_str(), "-S"])
            .stdin(Stdio::null())
            .stdout(Stdio::from(out))
            .stderr(Stdio::from(err))
            .spawn()
            .map_err(|e| format!("failed to spawn {}: {e}", qemu.display()))?;
        match DebugPort::connect(tcp, Duration::from_secs(20)) {
            Ok(port) => Ok(Session { child, port, console: console.to_path_buf() }),
            Err(e) => {
                let _ = child.kill();
                let _ = child.wait();
                Err(e)
            }
        }
    }

    /// The console's capture so far.
    pub fn console(&self) -> Vec<u8> {
        std::fs::read(&self.console).unwrap_or_default()
    }

    /// The emulator's exit code once the machine has ended; an emulator
    /// that does not leave within the timeout is an error.
    pub fn exit_code(&mut self, timeout: Duration) -> Result<i32, String> {
        let deadline = Instant::now() + timeout;
        loop {
            match self.child.try_wait() {
                Ok(Some(status)) => return Ok(status.code().unwrap_or(1)),
                Ok(None) if Instant::now() >= deadline => return Err(format!("the emulator did not leave within {} s of the machine's end", timeout.as_secs())),
                Ok(None) => std::thread::sleep(Duration::from_millis(25)),
                Err(e) => return Err(format!("wait on the emulator failed: {e}")),
            }
        }
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// The program of the unhandled-event case, as far as its runs need it.
#[derive(Debug, Clone, Copy)]
pub struct EventProgram<'a> {
    /// The ending of the name of the program's Main and of the method
    /// that throws.
    pub main_ending: &'a str,
    pub thrower_ending: &'a str,
    /// The symbol at the thrown type's record.
    pub type_symbol: &'a str,
    /// The line the program's own unhandled-exception handler prints, the
    /// line the runtime prints for an exception nothing catches, and the
    /// program's last line before the throw.
    pub handler_line: &'a str,
    pub unhandled_line: &'a str,
    pub before_line: &'a str,
    /// The image's exit code when an exception is unhandled, and at the
    /// kernel's fault report.
    pub exit_unhandled: i32,
    pub exit_fault: i32,
}

/// The four runs of the case.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventRun {
    /// The word never written: the program ends as it does with no
    /// debugger.
    Clear,
    /// The word written at the first halt, before any instruction ran:
    /// the startup zeroes it.
    Early,
    /// The word written at Main and a breakpoint of the client's own on
    /// the BKPT's address: one stop there, then the run to its end.
    Set,
    /// The word written at Main and no breakpoint: the instruction is
    /// executed and, with no debugger to halt the core, faults.
    Raw,
}

impl EventRun {
    pub const ALL: [EventRun; 4] = [EventRun::Clear, EventRun::Early, EventRun::Set, EventRun::Raw];

    pub fn name(self) -> &'static str {
        match self {
            EventRun::Clear => "clear",
            EventRun::Early => "early",
            EventRun::Set => "set",
            EventRun::Raw => "raw",
        }
    }
}

const SESSION_RUN_S: u64 = 300;

fn count_lines(console: &[u8], part: &str) -> usize {
    String::from_utf8_lossy(console).lines().filter(|l| l.contains(part)).count()
}

/// One run of the case through the debug port. `Ok` carries what was
/// seen, for the case's line; `Err` the first expectation that did not
/// hold. The console is left at `<out>/<tag>-<run>.raw` and the packets
/// at `<out>/<tag>-<run>.port.txt`, whatever the outcome.
pub fn event_run(qemu: &Path, machine: &str, image: &Path, img: &ArmImage, facts: &EventFacts, program: &EventProgram, run: EventRun, out: &Path, tag: &str) -> Result<String, String> {
    let stem = format!("{tag}-{}", run.name());
    let console = out.join(format!("{stem}.raw"));
    let stderr = out.join(format!("{stem}.stderr"));
    let mut session = Session::start(qemu, machine, image, &console, &stderr)?;
    let result = event_run_inner(&mut session, img, facts, program, run);
    let _ = std::fs::write(out.join(format!("{stem}.port.txt")), session.port.log.join("\n") + "\n");
    result
}

fn event_run_inner(session: &mut Session, img: &ArmImage, facts: &EventFacts, program: &EventProgram, run: EventRun) -> Result<String, String> {
    let main = img.function_ending(program.main_ending)?.value & !1;
    let run_time = Duration::from_secs(SESSION_RUN_S);
    match session.port.open()? {
        Halt::Stopped(_) => {}
        other => return Err(format!("the machine is not halted at the session's opening ({other:?})")),
    }
    if run == EventRun::Early {
        session.port.write_word(facts.word, EVENT_WORD_UNHANDLED)?;
        let back = session.port.read_word(facts.word)?;
        if back != EVENT_WORD_UNHANDLED {
            return Err(format!("the word written at the first halt reads back {back:#010x}"));
        }
    }
    session.port.set_breakpoint(main)?;
    match session.port.resume(run_time)? {
        Halt::Stopped(_) => {}
        other => return Err(format!("the machine did not stop at the program's Main ({other:?})")),
    }
    let pc = session.port.register(15)? & !1;
    if pc != main {
        return Err(format!("the first stop is at {pc:#010x}, not at the program's Main {main:#010x}"));
    }
    let at_main = session.port.read_word(facts.word)?;
    if at_main != 0 {
        return Err(format!("the events word reads {at_main:#010x} at the program's Main (0 expected: the startup zeroes it)"));
    }
    session.port.clear_breakpoint(main)?;
    let mut seen = String::new();
    if matches!(run, EventRun::Set | EventRun::Raw) {
        session.port.write_word(facts.word, EVENT_WORD_UNHANDLED)?;
    }
    if run == EventRun::Set {
        session.port.set_breakpoint(facts.bkpt)?;
        match session.port.resume(run_time)? {
            Halt::Stopped(_) => {}
            other => return Err(format!("the machine did not stop at the event's BKPT ({other:?})")),
        }
        let regs = session.port.registers()?;
        if regs[15] & !1 != facts.bkpt {
            return Err(format!("the stop is at {:#010x}, not at the event's BKPT {:#010x}", regs[15] & !1, facts.bkpt));
        }
        let record = img.symbol(program.type_symbol).ok_or_else(|| format!("the image defines no {}", program.type_symbol))?.value;
        let first = session.port.read_word(regs[0])?;
        if first != record {
            return Err(format!("r0's first word is {first:#010x}, not the thrown type's record {} at {record:#010x}", program.type_symbol));
        }
        let now = session.console();
        if !contains_fixed(&now, program.before_line) {
            return Err(format!("the console at the stop lacks the program's line before the throw ('{}')", program.before_line));
        }
        for line in [program.handler_line, program.unhandled_line] {
            if contains_fixed(&now, line) {
                return Err(format!("the console at the stop already holds '{line}': the event is not before the handlers and the print"));
            }
        }
        let port = &mut session.port;
        let mut read32 = |a: u32| port.read_word(a).ok();
        let frames = walk(img, regs, &mut read32, 48);
        let reached = frames.iter().position(|f| f.name.as_deref().is_some_and(|n| n.ends_with(program.thrower_ending)));
        let Some(reached) = reached else {
            let last = frames.last();
            return Err(format!(
                "the walk by the index table did not reach the throwing method: {} frame(s), the last in {} ({})",
                frames.len(),
                last.and_then(|f| f.name.clone()).unwrap_or_else(|| "no function".to_string()),
                last.and_then(|f| f.end.clone()).unwrap_or_default()
            ));
        };
        if frames[0].name.as_deref() != Some(EVENT_ROUTINE) || frames.len() < 2 || frames[1].name.as_deref() == Some(EVENT_ROUTINE) {
            return Err("the walk's first frame is not the routine alone".to_string());
        }
        let lr_in = frames[1].name.clone().unwrap_or_else(|| "no function".to_string());
        seen = format!(
            "one stop at the BKPT {:#010x} with r0's first word the record {}, lr inside {lr_in}, the console without the handler's line and the runtime's; the walk by the index table reaches {} at frame {reached} of {}, {} of those frames with an entry of the frame's own function; ",
            facts.bkpt,
            program.type_symbol,
            frames[reached].name.clone().unwrap_or_default(),
            frames.len(),
            frames[..=reached].iter().filter(|f| f.own).count()
        );
        // A debugger goes on from the address after the instruction.
        session.port.set_register(15, facts.bkpt.wrapping_add(2))?;
    }
    let (want_exit, fault) = if run == EventRun::Raw { (program.exit_fault, true) } else { (program.exit_unhandled, false) };
    let reported = match session.port.resume(run_time)? {
        Halt::Exited(code) => Some(code),
        Halt::Closed => None,
        Halt::Stopped(signal) => {
            let pc = session.port.register(15).unwrap_or(0) & !1;
            return Err(format!("the machine stopped once more (signal {signal}), at {pc:#010x}, where the run must go to its end"));
        }
    };
    let exit = session.exit_code(Duration::from_secs(20))?;
    if let Some(code) = reported {
        if code as i32 != exit & 0xff {
            return Err(format!("the debug port reported the exit code {code} and the emulator left with {exit}"));
        }
    }
    let end = session.console();
    if exit != want_exit {
        return Err(format!("the emulator's exit code is {exit}, not {want_exit}"));
    }
    let (handlers, unhandled, faults) = (count_lines(&end, program.handler_line), count_lines(&end, program.unhandled_line), count_lines(&end, "[fault]"));
    if fault {
        let pc = format!("pc={:#010x}", facts.bkpt);
        for part in ["[fault] *** EXCEPTION: HardFault (IPSR=3) ***", "HFSR=0x40000000", pc.as_str(), "r2=0x00004442"] {
            if !contains_fixed(&end, part) {
                return Err(format!("the kernel's fault report lacks '{part}'"));
            }
        }
        if handlers != 0 || unhandled != 0 {
            return Err("the handler's line or the runtime's line was printed on a run that must end at the kernel's fault report".to_string());
        }
        seen.push_str(&format!("the kernel's report of a HardFault at the BKPT {:#010x} with the magic's half in r2, exit {exit}", facts.bkpt));
    } else {
        if handlers != 1 || unhandled != 1 || faults != 0 {
            return Err(format!("the console at the end holds the handler's line {handlers} time(s), the runtime's line {unhandled} time(s) and {faults} line(s) of a fault report (1, 1 and 0 expected)"));
        }
        seen.push_str(&format!("the word zero at the program's Main; exit {exit} with the handler's line and the runtime's line once each and no fault report"));
    }
    Ok(seen)
}

/// A small image of the reader's own for the selfcheck: a routine of the
/// event's shape at 0x100 (a compare, the BKPT or a no-operation, a
/// branch, a literal word marked as data), its index-table entry (its own
/// and unwinding by the link register, or one that cannot unwind) with
/// the entry that closes the table after the routine,
/// the events word inside the zero-filled window or outside it, and a
/// symbol table (or none).
pub fn synth_event_image(bkpt: bool, entry_unwinds: bool, word_in_window: bool, symbols: bool) -> Vec<u8> {
    let text_addr = 0x100u32;
    let mut text: Vec<u8> = Vec::new();
    for half in [0x4b01u16, 0x681b, if bkpt { EVENT_BKPT } else { THUMB_NOP }, 0x4770] {
        text.extend_from_slice(&half.to_le_bytes());
    }
    // The literal: a word whose low half reads as the BKPT, to show that data is passed over.
    text.extend_from_slice(&0x2000_be60u32.to_le_bytes());
    let exidx_addr = 0x200u32;
    let to_text = text_addr.wrapping_sub(exidx_addr) & 0x7fff_ffff;
    let mut exidx: Vec<u8> = Vec::new();
    exidx.extend_from_slice(&to_text.to_le_bytes());
    exidx.extend_from_slice(&(if entry_unwinds { 0x80b0_b0b0u32 } else { 1u32 }).to_le_bytes());
    // The table's last entry, as a linker closes it: everything after the routine cannot be unwound.
    let past_text = (text_addr + text.len() as u32).wrapping_sub(exidx_addr + 8) & 0x7fff_ffff;
    exidx.extend_from_slice(&past_text.to_le_bytes());
    exidx.extend_from_slice(&1u32.to_le_bytes());
    let names: [&str; 6] = ["", EVENT_ROUTINE, EVENT_WORD, "__sbss", "__ebss", "$d"];
    let mut strtab: Vec<u8> = Vec::new();
    let mut name_at = Vec::new();
    for n in names {
        name_at.push(strtab.len() as u32);
        strtab.extend_from_slice(n.as_bytes());
        strtab.push(0);
    }
    let word_addr = if word_in_window { 0x2000_0010u32 } else { 0x2000_0100 };
    let mut symtab: Vec<u8> = vec![0u8; 16];
    for (name, value, size, info, shndx) in [
        (name_at[1], text_addr | 1, 12u32, 0x12u8, 1u16),
        (name_at[2], word_addr, 4, 0x11, 3),
        (name_at[3], 0x2000_0000, 0, 0x10, 3),
        (name_at[4], 0x2000_0040, 0, 0x10, 3),
        (name_at[5], text_addr + 8, 0, 0x00, 1),
    ] {
        symtab.extend_from_slice(&name.to_le_bytes());
        symtab.extend_from_slice(&value.to_le_bytes());
        symtab.extend_from_slice(&size.to_le_bytes());
        symtab.push(info);
        symtab.push(0);
        symtab.extend_from_slice(&shndx.to_le_bytes());
    }
    // The file: the ELF header, the four sections' bytes, then the section table.
    let mut file = vec![0u8; 52];
    let place = |file: &mut Vec<u8>, bytes: &[u8]| -> u32 {
        let at = file.len() as u32;
        file.extend_from_slice(bytes);
        while file.len() % 4 != 0 {
            file.push(0);
        }
        at
    };
    let text_off = place(&mut file, &text);
    let exidx_off = place(&mut file, &exidx);
    let symtab_off = place(&mut file, &symtab);
    let strtab_off = place(&mut file, &strtab);
    let shoff = file.len() as u32;
    let mut sections: Vec<[u32; 10]> = vec![
        [0; 10],
        [0, 1, 6, text_addr, text_off, text.len() as u32, 0, 0, 4, 0],
        [0, SHT_ARM_EXIDX, 0x82, exidx_addr, exidx_off, exidx.len() as u32, 1, 0, 4, 8],
        [0, SHT_NOBITS, 3, 0x2000_0000, 0, 0x200, 0, 0, 4, 0],
    ];
    if symbols {
        sections.push([0, SHT_SYMTAB, 0, 0, symtab_off, symtab.len() as u32, 5, 1, 4, 16]);
        sections.push([0, 3, 0, 0, strtab_off, strtab.len() as u32, 0, 0, 1, 0]);
    }
    for s in &sections {
        for w in s {
            file.extend_from_slice(&w.to_le_bytes());
        }
    }
    file[..7].copy_from_slice(&[0x7f, b'E', b'L', b'F', 1, 1, 1]);
    file[0x20..0x24].copy_from_slice(&shoff.to_le_bytes());
    file[0x2e..0x30].copy_from_slice(&40u16.to_le_bytes());
    file[0x30..0x32].copy_from_slice(&(sections.len() as u16).to_le_bytes());
    file
}
