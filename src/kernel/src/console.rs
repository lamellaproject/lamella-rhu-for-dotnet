//! Kernel console: TWO STREAMS, each routed at build time to a sink, and
//! the kernel's OUTPUT LEVEL. The
//! board module owns the wire device; this module
//! owns the streams, their sinks, the level, the CRLF wire discipline, the
//! `core::fmt` plumbing and the line locking. Every line holds the kernel
//! critical section for the whole formatted write, so concurrent contexts
//! never tear a line whichever sinks the streams share.
//!
//! THE CHANNELS a stream may name, chosen in the project at build time and
//! carried to this prebuilt kernel as a LINK-TIME WORD (the absolute symbols
//! `__rhu_log_channel` and `__rhu_console_channel`; `PROVIDE(... = 0)` in
//! every link and payload script, so an image linked without the knob gets
//! 0 = the row's default; the SDK passes `--defsym` from the project's
//! properties; the kernel's own images take `RHU_LOG_CHANNEL` and
//! `RHU_CONSOLE_CHANNEL` from the environment through build.rs):
//!   0        the row's default (the facts' `channels` row)
//!   1  off   the stream discarded whole (the write door still answers the count)
//!   2  memory  no wire: the 16 KB memory console alone (the Pico default)
//!   3  probe   the ST-Link boards' virtual COM port (the board's USART path)
//!   0x10 + n  uart n: a UART of the row's facts table, held by the kernel
//! THE MIRROR IS A TEE: every line of either stream
//! lands in the 16 KB mirror `__rhu_console_log` on every row whatever the
//! stream's channel says, and the channel names the WIRE; `off` discards the
//! stream whole, the mirror included. So a debugger reads the same
//! transcript on every board.
//! The mirror's header carries the program's end (`exit_word.rs`): the exit
//! code and the end magic written by the exit paths. A reader over the debug
//! port takes the end from the header; no line on the wire says it.
//!
//! THE LEVEL: `__rhu_log_level` = 1 silent | 2 quiet | 3 verbose
//! (0 = unset = verbose, the kernel's own posture for every cargo-built
//! flavor and every harness-linked gate image). A
//! line carries the level of the quietest image that still prints it:
//! `klog!(level, ...)` tests the image's level before formatting;
//! `kprintln!` is `klog!(LEVEL_VERBOSE, ...)`. The funnel's fatal reports
//! print at every level (`fatal()`). The program's stream is never filtered.

use core::fmt;
use core::ptr::{addr_of, addr_of_mut, read_volatile, write_volatile};

use crate::board;
use crate::board::ring;

/// A line of the log stream at a level: dropped by one compare when the
/// image's level is below it, else formatted and written whole under the
/// line lock.
macro_rules! klog {
    ($level:expr, $($arg:tt)*) => {{
        if $crate::console::prints($level) {
            $crate::console::locked($crate::console::Stream::Log, |out| {
                use ::core::fmt::Write;
                let _ = ::core::writeln!(out, $($arg)*);
            });
        }
    }};
}

/// A verbose line of the log stream.
macro_rules! kprintln {
    () => {{
        klog!($crate::console::LEVEL_VERBOSE, "");
    }};
    ($($arg:tt)*) => {{
        klog!($crate::console::LEVEL_VERBOSE, $($arg)*);
    }};
}

/// The levels: a line's level names the quietest image that prints it.
pub const LEVEL_SILENT: u32 = 1;
pub const LEVEL_QUIET: u32 = 2;
pub const LEVEL_VERBOSE: u32 = 3;

/// The channel codes the link-time words carry.
#[allow(dead_code)] // the encoding's zero, stated for the reader
pub const CH_DEFAULT: u8 = 0;
pub const CH_OFF: u8 = 1;
pub const CH_MEMORY: u8 = 2;
pub const CH_PROBE: u8 = 3;
pub const CH_UART0: u8 = 0x10;
#[allow(dead_code)] // the rows with one UART row name no second block
pub const CH_UART1: u8 = 0x11;

/// The channel facts of a row (`board/facts/<row>.rs`, `CHANNELS`; the
/// SDK's `channels` member): the two streams' default codes and the row's
/// legal set.
pub struct ChannelFacts {
    pub log: u8,
    pub console: u8,
    pub legal: &'static [u8],
}

/// A pre-defined UART row of a board (`UART_ROWS`; the SDK's `uart`
/// member): the chip's block number and its pins (`NO_PIN` on a row whose
/// UARTs have no pins, the emulator rows).
#[cfg_attr(not(rhu_uart_rows), allow(dead_code))] // unused on a row without UART pins
pub struct UartRow {
    pub block: u8,
    pub tx: u8,
    pub rx: u8,
}
pub const NO_PIN: u8 = 0xFF;

/// The chip's legal pin sets for a block's override (`UART_PAIRS`; the
/// SDK's `uartPairs` member): the TX pins and the RX pins the UART
/// function reaches, from the chip's function table.
#[cfg_attr(not(rhu_uart_rows), allow(dead_code))] // unused on a row without UART pins
pub struct UartPairs {
    pub block: u8,
    pub tx: &'static [u8],
    pub rx: &'static [u8],
}

/// The two streams.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Stream {
    Log = 0,
    Console = 1,
}

/// A resolved sink: nothing; the mirror alone; the board's wire for a
/// channel code (the probe's port, or uart n).
#[derive(Clone, Copy, PartialEq, Eq)]
enum Sink {
    Off,
    Memory,
    Wire(u8),
}

unsafe extern "C" {
    static __rhu_log_level: u8;
    static __rhu_log_channel: u8;
    static __rhu_console_channel: u8;
    static __rhu_uart0_tx: u8;
    static __rhu_uart0_rx: u8;
    static __rhu_uart1_tx: u8;
    static __rhu_uart1_rx: u8;
}

/// An absolute link-time word read as the symbol's ADDRESS, through the
/// assembler so the compiler never assumes a static's address is non-zero
/// (the `PROVIDE(... = 0)` default IS zero).
macro_rules! link_word {
    ($sym:ident) => {{
        let v: u32;
        unsafe {
            core::arch::asm!(
                "movw {0}, :lower16:{1}",
                "movt {0}, :upper16:{1}",
                out(reg) v,
                sym $sym,
                options(nomem, nostack, preserves_flags)
            )
        };
        v
    }};
}

/// The project's pin words for a UART block (`__rhu_uart<n>_tx/rx`):
/// 0x100 | gpio when a pin is named, 0 unset; (0, 0) for a block past the
/// two the words name.
#[cfg_attr(not(rhu_uart_rows), allow(dead_code))] // read at a claim on the rows with a UART table
pub fn pin_words(block: u8) -> (u32, u32) {
    match block {
        0 => (link_word!(__rhu_uart0_tx), link_word!(__rhu_uart0_rx)),
        1 => (link_word!(__rhu_uart1_tx), link_word!(__rhu_uart1_rx)),
        _ => (0, 0),
    }
}

/// The image's level (0 until the first init reads the word).
static mut LEVEL: u32 = 0;
/// The two streams' sinks, by `Stream`.
static mut SINKS: [Sink; 2] = [Sink::Memory, Sink::Memory];
/// The two streams' effective channel codes, by `Stream`.
static mut CODES: [u8; 2] = [CH_MEMORY, CH_MEMORY];
/// Set by the funnel: every line prints from here on (the machine parks).
static mut FATAL: bool = false;
/// The UART blocks the kernel holds as wires, as a bitmap by block.
static mut HELD: u32 = 0;
/// The boot banner's console clause, built at init.
static mut CLAUSE: [u8; 96] = [0; 96];
static mut CLAUSE_LEN: usize = 0;
/// The refusals the first init found (bit 0 the level, 1 the log channel,
/// 2 the console channel, 3 UART0's pin words, 4 UART1's pin words, 5 the
/// log stream and 6 the console stream naming a UART block the row lists
/// with no default pins when the project's pin words name no legal pair:
/// a legal channel without its pins, said in its own words), said once the
/// sinks exist.
static mut REFUSED: u32 = 0;
/// The UART block each stream named when bit 5 or 6 is set, by `Stream`
/// (the log stream first), for the refusal line's words.
static mut UNPINNED: [u8; 2] = [0; 2];
static mut READY: bool = false;
static mut HELD_DONE: bool = false;

fn code_of(word: u32, default: u8) -> (u8, bool) {
    if word == 0 {
        return (default, true);
    }
    if word > 0xFF {
        return (default, false);
    }
    let code = word as u8;
    if board::facts::CHANNELS.legal.contains(&code) {
        (code, true)
    } else {
        (default, false)
    }
}

/// Whether this row serves a wire for the code (the rows differ: the ST
/// rows' probe, the Pico and emulator rows' UARTs of their facts table).
#[cfg(rhu_console_probe)]
fn wire_served(code: u8) -> bool {
    code == CH_PROBE
}
#[cfg(rhu_uart_rows)]
fn wire_served(code: u8) -> bool {
    // A block with no default
    // pair and no legal project words (the Pico rows' UART1) serves no wire;
    // the stream takes the row's default and the refusal line says so.
    code >= CH_UART0 && board::facts::UART_ROWS.iter().any(|r| r.block == code - CH_UART0) && crate::uart::block_serves_wire((code - CH_UART0) as u32)
}

/// Whether a wire code names a UART block this row lists WITHOUT default
/// pins on a chip that lists legal pairs for it (the Pico rows' UART1): the
/// case `wire_served` refuses for the pins' sake, not the word's, said in
/// its own line.
#[cfg(rhu_console_probe)]
fn wire_unpinned(_code: u8) -> bool {
    false
}
#[cfg(rhu_uart_rows)]
fn wire_unpinned(code: u8) -> bool {
    code >= CH_UART0
        && board::facts::UART_ROWS.iter().any(|r| r.block == code - CH_UART0 && r.tx == NO_PIN)
        && board::facts::UART_PAIRS.iter().any(|p| p.block == code - CH_UART0)
}

fn sink_of(code: u8) -> Sink {
    match code {
        CH_OFF => Sink::Off,
        CH_MEMORY => Sink::Memory,
        c => Sink::Wire(c),
    }
}

/// The name of a channel code for the banner's clause.
fn channel_name(code: u8) -> &'static str {
    match code {
        CH_OFF => "off",
        CH_MEMORY => memory_name(),
        CH_PROBE => board::CONSOLE,
        c => uart_name(c - CH_UART0),
    }
}
#[cfg(rhu_uart_pl011)]
fn memory_name() -> &'static str {
    "memory console (no UART pin)"
}
#[cfg(not(rhu_uart_pl011))]
fn memory_name() -> &'static str {
    "memory console"
}
#[cfg(rhu_uart_pl011)]
fn uart_name(n: u8) -> &'static str {
    board::facts::UART_NAMES.get(n as usize).copied().unwrap_or("UART (unknown block)")
}
#[cfg(rhu_uart_cmsdk)]
fn uart_name(n: u8) -> &'static str {
    if n == 0 { board::CONSOLE } else { "CMSDK UART1" }
}
#[cfg(rhu_console_probe)]
fn uart_name(_n: u8) -> &'static str {
    "UART (no row on this board)"
}

fn clause_push(s: &str) {
    unsafe {
        let len = read_volatile(addr_of!(CLAUSE_LEN));
        let buf = addr_of_mut!(CLAUSE) as *mut u8;
        let mut i = len;
        for &b in s.as_bytes() {
            if i < 96 {
                write_volatile(buf.add(i), b);
                i += 1;
            }
        }
        write_volatile(addr_of_mut!(CLAUSE_LEN), i);
    }
}

/// Resolve the three words once: the level, the two sinks, the held UART
/// blocks and the banner's clause. Idempotent.
fn resolve() {
    if unsafe { read_volatile(addr_of!(READY)) } {
        return;
    }
    let lw = link_word!(__rhu_log_level);
    let log_word = link_word!(__rhu_log_channel);
    let con_word = link_word!(__rhu_console_channel);
    unsafe {
        let mut refused = 0u32;
        let level = match lw {
            0 => LEVEL_VERBOSE,
            1..=3 => lw,
            _ => {
                refused |= 1;
                LEVEL_VERBOSE
            }
        };
        let facts = &board::facts::CHANNELS;
        let (mut log_code, ok) = code_of(log_word, facts.log);
        if !ok {
            refused |= 2;
        }
        let (mut con_code, ok) = code_of(con_word, facts.console);
        if !ok {
            refused |= 4;
        }
        // A wire this row does not serve: the row's default. A UART block the
        // row lists with no default pins, when the project's pin words name
        // no legal pair (the Pico rows' UART1), is a legal channel without its pins: its own
        // bit and its own line, naming the cause and the cure.
        let mut unpinned = [0u8; 2];
        if let Sink::Wire(c) = sink_of(log_code) {
            if !wire_served(c) {
                if wire_unpinned(c) {
                    refused |= 32;
                    unpinned[0] = c - CH_UART0;
                } else {
                    refused |= 2;
                }
                log_code = facts.log;
            }
        }
        if let Sink::Wire(c) = sink_of(con_code) {
            if !wire_served(c) {
                if wire_unpinned(c) {
                    refused |= 64;
                    unpinned[1] = c - CH_UART0;
                } else {
                    refused |= 4;
                }
                con_code = facts.console;
            }
        }
        let mut held = 0u32;
        for code in [con_code, log_code] {
            if code >= CH_UART0 {
                held |= 1 << (code - CH_UART0);
            }
        }
        write_volatile(addr_of_mut!(LEVEL), level);
        write_volatile(addr_of_mut!(CODES), [log_code, con_code]);
        write_volatile(addr_of_mut!(SINKS), [sink_of(log_code), sink_of(con_code)]);
        write_volatile(addr_of_mut!(HELD), held);
        write_volatile(addr_of_mut!(REFUSED), refused);
        write_volatile(addr_of_mut!(UNPINNED), unpinned);
        write_volatile(addr_of_mut!(CLAUSE_LEN), 0);
        if log_code == con_code {
            clause_push(channel_name(con_code));
        } else {
            clause_push("log ");
            clause_push(channel_name(log_code));
            clause_push(" | console ");
            clause_push(channel_name(con_code));
        }
        write_volatile(addr_of_mut!(READY), true);
    }
}

/// Whether a stream's resolved sink is the wire of this channel code: the
/// rows whose wire exists only when a stream names it (the STM32H7 rows'
/// probe channel, USART1 on the maker's console pins) bring its hardware
/// up on this answer and touch no pin otherwise.
#[allow(dead_code)] // read by the board modules whose wire is optional
pub fn wire_named(code: u8) -> bool {
    resolve();
    unsafe { read_volatile(addr_of!(SINKS)).iter().any(|s| matches!(s, Sink::Wire(c) if *c == code)) }
}

/// The UART blocks the streams name held through the UART doors' module
/// (once; the emulator rows' UART0 stays the board module's polled wire,
/// so it is held without its hardware). An illegal pin override is
/// recorded for `say_refusals`.
fn hold_wires() {
    if unsafe { read_volatile(addr_of!(HELD_DONE)) } {
        return;
    }
    let held = unsafe { read_volatile(addr_of!(HELD)) };
    for block in 0..2u32 {
        if held & (1 << block) == 0 {
            continue;
        }
        let hardware = !(cfg!(rhu_uart_cmsdk) && block == 0);
        if !crate::uart::console_hold(block, hardware) {
            unsafe {
                let r = read_volatile(addr_of!(REFUSED));
                write_volatile(addr_of_mut!(REFUSED), r | (8 << block));
            }
        }
    }
    unsafe { write_volatile(addr_of_mut!(HELD_DONE), true) };
}

/// The console's init: the words resolved on the first call, the mirror's
/// header written where the boot path has not (the emulator rows), the
/// UART wires held, the board's wire brought up. Called once by the boot
/// path and again by the funnel (the board's re-init takes its polled
/// mode).
pub fn init() {
    resolve();
    ring::ensure_armed();
    hold_wires();
    board::console_init();
}

/// The refusals the first init found, said once the sinks exist (the boot
/// path calls it right after `init`): one fatal-level line per refusal.
pub fn say_refusals() {
    let refused = unsafe { read_volatile(addr_of!(REFUSED)) };
    if refused == 0 {
        return;
    }
    if refused & 1 != 0 {
        klog!(LEVEL_SILENT, "[boot] the kernel output level word is not 1, 2 or 3: verbose taken");
    }
    if refused & 2 != 0 {
        klog!(LEVEL_SILENT, "[boot] the log stream's channel word is not legal on this row: the row's default taken");
    }
    if refused & 4 != 0 {
        klog!(LEVEL_SILENT, "[boot] the console stream's channel word is not legal on this row: the row's default taken");
    }
    for block in 0..2u32 {
        if refused & (8 << block) != 0 {
            klog!(LEVEL_SILENT, "[boot] the project's pin words for UART{} are not a legal pair of this chip: the facts' pair taken", block);
        }
    }
    // A legal channel without its pins (bits 5 and 6): the cause and the cure in the line's
    // own words, the row's default named; the line calls the place the pins are named "the project file".
    let unpinned = unsafe { read_volatile(addr_of!(UNPINNED)) };
    let defaults = [board::facts::CHANNELS.log, board::facts::CHANNELS.console];
    for (i, stream) in ["log", "console"].iter().enumerate() {
        if refused & (32 << i) != 0 {
            let block = unpinned[i];
            let (wtx, wrx) = pin_words(block);
            let words = if wtx == 0 && wrx == 0 { "the project named none" } else { "the project's pin words are not a legal pair of this chip" };
            klog!(LEVEL_SILENT, "[boot] the {} stream names uart{}, but this row gives UART{} no default pins and {}; the {} stays on the row's default, {}; name the pair in the project file", stream, block, block, words, stream, channel_name(defaults[i]));
        }
    }
}

/// The funnel's entry: every line prints from here on, the held UART wires
/// take the polled mode, and the board's console takes its polled mode.
pub fn fatal() {
    unsafe { write_volatile(addr_of_mut!(FATAL), true) };
    crate::uart::set_polled();
    init();
}

/// The image's level, 1 to 3 (the words read if the boot path has not yet).
pub fn level() -> u32 {
    resolve();
    unsafe { read_volatile(addr_of!(LEVEL)) }
}

/// Whether a line at `level` prints in this image.
pub fn prints(level: u32) -> bool {
    resolve();
    unsafe { read_volatile(addr_of!(FATAL)) || level <= read_volatile(addr_of!(LEVEL)) }
}

/// The effective channel code of a stream.
pub fn channel(stream: Stream) -> u8 {
    resolve();
    unsafe { read_volatile(addr_of!(CODES[stream as usize])) }
}

/// Whether the mirror holds the boot banner's first line at its start (the
/// boot-memory image's assertion before it exits).
pub fn mirror_holds(prefix: &[u8]) -> bool {
    let log = addr_of!(ring::__rhu_console_log);
    unsafe {
        if read_volatile(addr_of!((*log).magic)) != ring::CONSOLE_LOG_MAGIC || (read_volatile(addr_of!((*log).written)) as usize) < prefix.len() {
            return false;
        }
        let bytes = addr_of!((*log).bytes) as *const u8;
        prefix.iter().enumerate().all(|(i, &b)| read_volatile(bytes.add(i)) == b)
    }
}

/// The boot banner's console clause: one name when both streams share a
/// sink, "log <sink> | console <sink>" when they part.
pub fn clause() -> &'static str {
    resolve();
    unsafe {
        let len = read_volatile(addr_of!(CLAUSE_LEN));
        let bytes = core::slice::from_raw_parts(addr_of!(CLAUSE) as *const u8, len);
        core::str::from_utf8_unchecked(bytes)
    }
}

fn sink(stream: Stream) -> Sink {
    unsafe { read_volatile(addr_of!(SINKS[stream as usize])) }
}

/// One byte to a stream's sink: the mirror is a tee on every row; the wire
/// takes the byte after it.
fn put(s: Sink, b: u8) {
    match s {
        Sink::Off => {}
        Sink::Memory => ring::mirror(b),
        Sink::Wire(c) => wire_put(c, b),
    }
}

/// The wire's byte. On the probe rows the board's `console_putb` is the
/// ring's own `putb` (the mirror, then the wire ring); on the emulator rows
/// UART0 is the polled board path with the mirror fed first; every other
/// UART wire is a held port of the UART doors' module.
#[cfg(rhu_console_probe)]
fn wire_put(_c: u8, b: u8) {
    board::console_putb(b)
}
#[cfg(rhu_uart_cmsdk)]
fn wire_put(c: u8, b: u8) {
    ring::mirror(b);
    if c == CH_UART0 { board::console_putb(b) } else { crate::uart::held_putb((c - CH_UART0) as u32, b) }
}
#[cfg(rhu_uart_pl011)]
fn wire_put(c: u8, b: u8) {
    ring::mirror(b);
    crate::uart::held_putb((c - CH_UART0) as u32, b)
}

/// The wire's room for a stream (the door's back-pressure): a sink without
/// a wire never waits.
fn room(stream: Stream) -> usize {
    match sink(stream) {
        Sink::Wire(c) => wire_room(c),
        _ => usize::MAX,
    }
}
#[cfg(rhu_console_probe)]
fn wire_room(_c: u8) -> usize {
    board::console_room()
}
#[cfg(rhu_uart_cmsdk)]
fn wire_room(c: u8) -> usize {
    if c == CH_UART0 { board::console_room() } else { crate::uart::held_room((c - CH_UART0) as u32) }
}
#[cfg(rhu_uart_pl011)]
fn wire_room(c: u8) -> usize {
    crate::uart::held_room((c - CH_UART0) as u32)
}

/// The wire's whole capacity for a stream: the most room a drained wire
/// can offer (a sink without a wire, and the emulator rows' polled UART0,
/// answer unbounded).
fn capacity(stream: Stream) -> usize {
    match sink(stream) {
        Sink::Wire(c) => wire_capacity(c),
        _ => usize::MAX,
    }
}
#[cfg(rhu_console_probe)]
fn wire_capacity(_c: u8) -> usize {
    ring::WIRE_BYTES
}
#[cfg(rhu_uart_cmsdk)]
fn wire_capacity(c: u8) -> usize {
    if c == CH_UART0 { usize::MAX } else { crate::uart::TX_BYTES }
}
#[cfg(rhu_uart_pl011)]
fn wire_capacity(_c: u8) -> usize {
    crate::uart::TX_BYTES
}

/// One pending byte of the console stream's wire, or None.
#[cfg(rhu_console_probe)]
fn wire_getb(_c: u8) -> Option<u8> {
    board::console_getb()
}
#[cfg(rhu_uart_cmsdk)]
fn wire_getb(c: u8) -> Option<u8> {
    if c == CH_UART0 { board::console_getb() } else { crate::uart::held_getb((c - CH_UART0) as u32) }
}
#[cfg(rhu_uart_pl011)]
fn wire_getb(c: u8) -> Option<u8> {
    crate::uart::held_getb((c - CH_UART0) as u32)
}

pub fn write_bytes(stream: Stream, bytes: &[u8]) {
    let s = sink(stream);
    if s == Sink::Off {
        return;
    }
    for &b in bytes {
        if b == b'\n' {
            put(s, b'\r');
        }
        put(s, b);
    }
}

/// A stream's writer: `core::fmt` plumbing for the line macros.
pub struct Out(pub Stream);

impl fmt::Write for Out {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        write_bytes(self.0, s.as_bytes());
        Ok(())
    }
}

/// Run `f` with exclusive console access (kernel critical section).
pub fn locked<R>(stream: Stream, f: impl FnOnce(&mut Out) -> R) -> R {
    crate::critical::with_cs(|| f(&mut Out(stream)))
}

/// The measurement posture test-payload-cycles (the STM32F769 row; a board
/// flight's, never a shipped image): the cycles the console's write doors
/// spend -- their waits for the wire's room included -- and the doors' write
/// count, read once at the hosted program's return by the payload module;
/// the core's cycle counter, armed by the payload module at the entry.
#[cfg(all(feature = "test-payload-cycles", rhu_qspi_window))]
static DOOR_CYCLES: core::sync::atomic::AtomicU32 = core::sync::atomic::AtomicU32::new(0);
#[cfg(all(feature = "test-payload-cycles", rhu_qspi_window))]
static DOOR_WRITES: core::sync::atomic::AtomicU32 = core::sync::atomic::AtomicU32::new(0);
#[cfg(all(feature = "test-payload-cycles", rhu_qspi_window))]
pub fn door_cycles_now() -> u32 {
    const DWT_CYCCNT: *const u32 = 0xE000_1004 as *const u32;
    unsafe { read_volatile(DWT_CYCCNT) }
}
#[cfg(all(feature = "test-payload-cycles", rhu_qspi_window))]
pub fn door_cycles_add(t0: u32) {
    use core::sync::atomic::Ordering::Relaxed;
    DOOR_CYCLES.fetch_add(door_cycles_now().wrapping_sub(t0), Relaxed);
    DOOR_WRITES.fetch_add(1, Relaxed);
}
#[cfg(all(feature = "test-payload-cycles", rhu_qspi_window))]
pub fn door_cycles() -> (u32, u32) {
    use core::sync::atomic::Ordering::Relaxed;
    (DOOR_CYCLES.load(Relaxed), DOOR_WRITES.load(Relaxed))
}

/// The write doors' plumbing: a write under one console lock, so a hosted
/// writer never tears against a kernel line (the same line-atomicity every
/// kernel line already buys; callers size their lines accordingly). Wire
/// grammar applies. Back-pressure runs OUTSIDE the lock: the door waits,
/// with interrupts open, until the stream's wire has room for the write's
/// wire length (the bytes plus one per newline), takes the lock, re-checks
/// the room under it and writes -- a writer that outruns the wire waits on
/// the transmitter's drain, never inside the critical section, and never
/// longer than a bounded write takes to drain (a sink without a wire
/// answers unbounded room and never waits here). A write whose wire length
/// does not fit half the wire is written in pieces, each the longest prefix
/// that fits, each under the lock and each waiting for its own room: a
/// bounded write never waits for more room than a draining wire can offer,
/// so it never waits forever.
pub fn door_write(stream: Stream, bytes: &[u8]) {
    #[cfg(all(feature = "test-payload-cycles", rhu_qspi_window))]
    let t0 = door_cycles_now();
    let limit = capacity(stream) / 2;
    let mut rest = bytes;
    while !rest.is_empty() {
        let mut n = 0;
        let mut wire = 0;
        for &b in rest {
            let w = if b == b'\n' { 2 } else { 1 };
            if wire + w > limit {
                break;
            }
            wire += w;
            n += 1;
        }
        let piece = &rest[..n];
        loop {
            while room(stream) < wire {
                core::hint::spin_loop();
            }
            let written = locked(stream, |_out| {
                if room(stream) >= wire {
                    write_bytes(stream, piece);
                    true
                } else {
                    false
                }
            });
            if written {
                break;
            }
        }
        rest = &rest[n..];
    }
    #[cfg(all(feature = "test-payload-cycles", rhu_qspi_window))]
    door_cycles_add(t0);
}

/// The read door's plumbing: drain pending receiver bytes of the console
/// stream's wire -- up to the buffer's length, RAW (no echo, no
/// translation) -- and return the count. Polls only; a stream without a
/// wire, or a wire with nothing pending, answers 0 immediately.
/// CS-bounded like every other verb (bounded by the buffer and the
/// receiver's shallow hardware depth, or the held port's receive ring).
pub fn door_read(buf: &mut [u8]) -> usize {
    let Sink::Wire(c) = sink(Stream::Console) else {
        return 0;
    };
    crate::critical::with_cs(|| {
        let mut i = 0;
        while i < buf.len() {
            match wire_getb(c) {
                Some(b) => {
                    buf[i] = b;
                    i += 1;
                }
                None => break,
            }
        }
        i
    })
}
