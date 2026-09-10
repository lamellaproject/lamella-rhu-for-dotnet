//! The payload gate on the runner: hosting the version-frozen minirt
//! payloads on QEMU mps2-an500.

use crate::gate::{write_fixture, GateCtx, Selfcheck};
use crate::image::{check_records, elf32_phdrs, header_abi, synth_elf32, Phdr, PT_LOAD, PT_TLS};
use crate::results::{chain_json_from_env, golden_json, validate_results, Results};
use crate::sha256::sha256_file;
use crate::text::{
    contains_fixed, cr_count, final_free, final_gcs, final_verdict_has, lf_normalize,
    payload_stream, word_sym_present, word_sym_value,
};
use crate::tools::{
    cargo_build_kernel_at, command_words, echo_file, llvm_tool, resolve_qemu, run_captured,
    rustc_facts, status_bounded, ExecProfile, RustcFacts,
};
use std::path::{Path, PathBuf};
use std::process::{exit, Command, Stdio};
use std::time::Duration;

const GATE: &str = "payload";
const DISPLAY: &str = "PAYLOAD GATE";
const HOST: &str = "qemu-mps2-an500";
const RUNNER: &str = "src/harness";
const TIMEOUT_S: u64 = 120;

/// Deadline for one external-linker merge. A link completes in seconds;
/// even a WSL-crossing external linker gets enormous headroom here, and a
/// linker still running at this bound is wedged, not slow.
const LINK_TIMEOUT_S: u64 = 120;

/// Frozen-input pin table: must equal goldens/payload/MANIFEST.md.
const SHA_RTSMOKE_O: &str = "9df5a916053a0417eed6107a1656c47786d49814093ee44b9d28e4249be66ad4";
const SHA_GCTEST_O: &str = "234c41569f26f190d12220d1e13799a34e05ccebb11f0b57cea4cd3ff21d7cc2";
const SHA_RTSMOKE_G: &str = "d6c040565d4346117cac8d750244ec62a9cffbb228fdd6088d5dfc8336df1dc5";
const SHA_GCTEST_G: &str = "5900b85b44294d57e75e9160feed525a8f0465b481d6992e716d6331b5d49fa1";

/// Collections pin: RHU's own derived value, never inherited, re-derived
/// per image and per clock. Derived on the canonical link (the rust-lld
/// default) under the Deterministic execution profile. The golden's own
/// final line remains the expected-free source because the derived value
/// equals it; a divergent derivation would have moved that expectation
/// into a gate-owned constant.
const COLLECTIONS_EXACT: &str = "62";

/// The deterministic profile's name in the results surface (additive,
/// report-only, between "control" and "image"). Emitted only on case
/// lines at or after a run under the profile: no run, no clock claim.
const TIMING_DET: &str = "icount-shift0";

/// The rt-link family's frozen input: the merged runtime object of the rt
/// tier and its demand list (goldens/rt, tied through that tier's
/// manifest pin), the object's section classes as the tier states them
/// (llvm-size -A over the object; the maps' brackets read them exactly),
/// the roots the host's initialization contract names beyond the two the
/// kernel's default entry references, and the family's linker words.
const RT_MERGED_O: &str = "goldens/rt/rthello-merged.o";
const RT_DEMAND_TXT: &str = "goldens/rt/rthello-merged.undef.txt";
const RT_MANAGED_BYTES: u64 = 389_414;
const RT_UNBOX_BYTES: u64 = 766;
const RT_HYDRATED_BYTES: u64 = 77_204;
const RT_TDATA_BYTES: u64 = 152;
const RT_INIT_ARRAY_BYTES: u64 = 24;
const RT_MODULES_BYTES: u64 = 4;

/// The section classes one frozen merged runtime object carries at the
/// merge, as the tier states them (llvm-size -A over the object; the maps'
/// brackets read them exactly): one record per frozen program, so a second
/// program's image is checked against its own sizes.
pub(crate) struct RtBrackets {
    pub(crate) managed: u64,
    pub(crate) unbox: u64,
    pub(crate) hydrated: u64,
    pub(crate) tdata: u64,
    pub(crate) init_array: u64,
    pub(crate) modules: u64,
}
pub(crate) const RT_HELLO_BRACKETS: RtBrackets = RtBrackets {
    managed: RT_MANAGED_BYTES,
    unbox: RT_UNBOX_BYTES,
    hydrated: RT_HYDRATED_BYTES,
    tdata: RT_TDATA_BYTES,
    init_array: RT_INIT_ARRAY_BYTES,
    modules: RT_MODULES_BYTES,
};
const RT_LD_WORDS: [&str; 5] = ["-u", "RhInitialize", "-u", "RhRegisterOSModule", "--warn-unresolved-symbols"];

/// The frozen minirt objects' own managed code: the brackets on every
/// payload-family link read these exactly.
const MANAGED_BYTES_RTSMOKE: u64 = 5_534;
const MANAGED_BYTES_GCTEST: u64 = 5_190;

/// The gate's shared an500 staticlib (the payload-feature build the gc and
/// shim gates link against too) and the an500 payload map of record.
const AN500_LIB: &str = "src/kernel/target/thumbv7em-none-eabihf/release/librhu_kernel.a";
const AN500_MAP: &str = "src/kernel/payload-mps2-an500.x";

/// PT_GNU_RELRO: the read-only-after-relocation segment the linker emits
/// over the GOT, the constructor arrays and the TLS template.
const PT_GNU_RELRO: u32 = 0x6474_e552;

/// One board of the rt-link family: its payload map, cargo target and
/// feature set (the QEMU boards with `qemu-exit`, as the batteries build
/// them). The an500 link reuses the gate's shared staticlib; the others
/// build into target dirs of their own.
struct RtBoard {
    board: &'static str,
    target: &'static str,
    features: &'static str,
}

const RT_BOARDS: [RtBoard; 6] = [
    RtBoard { board: "mps2-an500", target: "thumbv7em-none-eabihf", features: "qemu-exit,board-mps2-an500,payload" },
    RtBoard { board: "mps2-an505", target: "thumbv8m.main-none-eabihf", features: "qemu-exit,board-mps2-an505,payload" },
    RtBoard { board: "nucleo-u5a5zj-q", target: "thumbv8m.main-none-eabihf", features: "board-nucleo-u5a5zj-q,payload" },
    RtBoard { board: "stm32f769i-disco", target: "thumbv7em-none-eabihf", features: "board-stm32f769i-disco,payload" },
    RtBoard { board: "pico-2", target: "thumbv8m.main-none-eabihf", features: "board-pico-2,payload" },
    RtBoard { board: "pico-plus-2", target: "thumbv8m.main-none-eabihf", features: "board-pico-plus-2,payload" },
];

/// What the rt-link checker reads off a linked artifact.
pub(crate) struct RtFacts {
    pub(crate) managed: u64,
    pub(crate) unbox: u64,
    pub(crate) hydrated: u64,
    pub(crate) tdata: u64,
    pub(crate) init_array: u64,
    pub(crate) modules: u64,
    pub(crate) exidx: u64,
    pub(crate) relro: u64,
    pub(crate) unresolved: usize,
    pub(crate) code_free: u64,
    pub(crate) arena_window: u64,
    pub(crate) arena_floor: u64,
}

/// One module-0 TLS template probe: the assembler text (one `module asm`
/// line each -- the integrated assembler stamps .tdata/.tbss thread-local
/// by name, the same flag a C object's thread-local sections carry), the
/// bytes the template must show in the load image, and the offset each
/// thread-local symbol must resolve to.
struct TlsProbe {
    case: &'static str,
    asm: &'static [&'static str],
    template: &'static [u8],
    syms: &'static [(&'static str, u64)],
}

const TLS_PROBES: [TlsProbe; 3] = [
    TlsProbe {
        case: "tls-tdata",
        asm: &[
            ".section .tdata,\"awT\",%progbits",
            ".p2align 2",
            ".global w1",
            "w1: .word 0x1234",
            ".text",
            ".global f",
            "f: bx lr",
        ],
        template: &[0x34, 0x12, 0x00, 0x00],
        syms: &[("w1", 0)],
    },
    TlsProbe {
        case: "tls-tbss",
        asm: &[
            ".section .tbss,\"awT\",%nobits",
            ".p2align 2",
            ".global w2",
            "w2: .space 4",
            ".text",
            ".global f",
            "f: bx lr",
        ],
        template: &[0x00, 0x00, 0x00, 0x00],
        syms: &[("w2", 0)],
    },
    TlsProbe {
        case: "tls-mixed",
        asm: &[
            ".section .tdata,\"aw\",%progbits",
            ".p2align 2",
            ".global w1",
            "w1: .word 0x1234",
            ".section .tbss,\"aw\",%nobits",
            ".p2align 2",
            ".global w2",
            "w2: .space 4",
            ".text",
            ".global f",
            "f: bx lr",
        ],
        template: &[0x34, 0x12, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
        syms: &[("w1", 0), ("w2", 4)],
    },
];

/// The kernel's own constructor input, assembled at gate time: one
/// pre-init constructor, two init constructors listed AGAINST their
/// priority order (200 before 100 in the source, so the transcript
/// order proves the sort), one destructor that must never run. Each
/// writes its marker through rhu_console_write; the markers carry the
/// kernel's bracket prefix, so the payload-stream filter drops them and
/// the rtsmoke golden stands.
const CTOR_MARK_PRE: &str = "[ctor] pre-init constructor ran";
const CTOR_MARK_100: &str = "[ctor] init constructor 100 ran";
const CTOR_MARK_200: &str = "[ctor] init constructor 200 ran";
const CTOR_MARK_FINI: &str = "[ctor] fini destructor ran";
/// The kernel's constructors line for the ctor image: one pre-init,
/// two init, the one rtsmoke module.
const CTOR_MODULES_LINE: &str =
    "[payload] constructors ran: 1 pre-init, 2 init; modules to initialize: 1";
/// The strong entry's marker and the code it returns (the machine's
/// exit code, reported by the kernel's unchanged returned line).
const ENTRY_MARK: &str = "[entry] payload-provided entry ran; the kernel default was displaced";
const ENTRY_RETURN: i32 = 7;
/// The kernel's refusal of a provided entry that states no generation.
const UNSTATED_SENTENCE: &str = "[payload] entry REFUSED: the payload provides rhu_payload_entry but states no hosting-API contract generation (rhu_payload_abi)";

/// One marker-writing function in assembler: `name` writes `msg` plus a
/// newline through rhu_console_write and returns.
fn marker_fn_asm(name: &str, msg: &str) -> Vec<String> {
    let len = msg.len() + 1;
    vec![
        format!(".section .rodata.{name}_msg,\"a\",%progbits"),
        format!("{name}_msg: .ascii \"{msg}\\n\""),
        format!(".section .text.{name},\"ax\",%progbits"),
        format!(".global {name}"),
        format!(".type {name},%function"),
        ".thumb_func".to_string(),
        format!("{name}:"),
        "  push {r7, lr}".to_string(),
        format!("  movw r0, #:lower16:{name}_msg"),
        format!("  movt r0, #:upper16:{name}_msg"),
        format!("  movw r1, #{len}"),
        "  bl rhu_console_write".to_string(),
        "  pop {r7, pc}".to_string(),
        format!(".size {name}, . - {name}"),
    ]
}

/// The constructor input's assembler text.
fn ctor_input_asm() -> Vec<String> {
    let mut a: Vec<String> = [
        ".syntax unified",
        ".thumb",
        ".section .preinit_array,\"aw\",%preinit_array",
        ".p2align 2",
        ".word ctor_pre",
        ".section .init_array.00200,\"aw\",%init_array",
        ".p2align 2",
        ".word ctor_200",
        ".section .init_array.00100,\"aw\",%init_array",
        ".p2align 2",
        ".word ctor_100",
        ".section .fini_array,\"aw\",%fini_array",
        ".p2align 2",
        ".word dtor_fini",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    a.extend(marker_fn_asm("ctor_pre", CTOR_MARK_PRE));
    a.extend(marker_fn_asm("ctor_100", CTOR_MARK_100));
    a.extend(marker_fn_asm("ctor_200", CTOR_MARK_200));
    a.extend(marker_fn_asm("dtor_fini", CTOR_MARK_FINI));
    a
}

/// The strong input's generation statement (the six lines the unstated
/// control removes).
fn strong_abi_block(abi: u32) -> String {
    format!(
        ".section .rodata.strong_abi,\"a\",%progbits\n.global rhu_payload_abi\n.type rhu_payload_abi,%object\n.p2align 2\nrhu_payload_abi: .word {abi}\n.size rhu_payload_abi, 4\n"
    )
}

/// A payload that provides its own entry: a strong `rhu_payload_entry`
/// that writes its marker and returns ENTRY_RETURN, and the contract
/// generation it was built against, stated strongly. The two controls
/// are generated from this text by one exact edit each.
fn strong_entry_asm(abi: u32) -> Vec<String> {
    let len = ENTRY_MARK.len() + 1;
    let mut a: Vec<String> = vec![
        ".syntax unified".to_string(),
        ".thumb".to_string(),
        ".section .rodata.entry_msg,\"a\",%progbits".to_string(),
        format!("entry_msg: .ascii \"{ENTRY_MARK}\\n\""),
        ".section .text.strong_entry,\"ax\",%progbits".to_string(),
        ".global rhu_payload_entry".to_string(),
        ".type rhu_payload_entry,%function".to_string(),
        ".thumb_func".to_string(),
        "rhu_payload_entry:".to_string(),
        "  push {r7, lr}".to_string(),
        "  movw r0, #:lower16:entry_msg".to_string(),
        "  movt r0, #:upper16:entry_msg".to_string(),
        format!("  movw r1, #{len}"),
        "  bl rhu_console_write".to_string(),
        format!("  movs r0, #{ENTRY_RETURN}"),
        "  pop {r7, pc}".to_string(),
        ".size rhu_payload_entry, . - rhu_payload_entry".to_string(),
    ];
    a.extend(strong_abi_block(abi).lines().map(String::from));
    a
}

/// What the ctor artifact must show for one array: its brackets and
/// the functions its entries name, in order.
struct CtorExpect {
    section: &'static str,
    lo_sym: &'static str,
    hi_sym: &'static str,
    entries: &'static [&'static str],
}

const CTOR_EXPECT: [CtorExpect; 3] = [
    CtorExpect {
        section: ".preinit_array",
        lo_sym: "__preinit_array_start",
        hi_sym: "__preinit_array_end",
        entries: &["ctor_pre"],
    },
    CtorExpect {
        section: ".init_array",
        lo_sym: "__init_array_start",
        hi_sym: "__init_array_end",
        entries: &["ctor_100", "ctor_200"],
    },
    CtorExpect {
        section: ".fini_array",
        lo_sym: "__fini_array_start",
        hi_sym: "__fini_array_end",
        entries: &["dtor_fini"],
    },
];

/// The thread-local section's segment alignment every board map declares.
const TLS_ALIGN: u64 = 8;
/// What a passing template check reports (report-only stats).
struct TlsFacts {
    lo: u64,
    hi: u64,
    phdrs: usize,
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// The template contract of one linked image against a probe: the
/// bracket pair opens by exactly the template's size inside the boot
/// copy window [__sdata, __edata); exactly one PT_TLS starts at the
/// bracket, sized and filed exactly (zero-fill input is filed, never
/// left to a loader), at the declared alignment, at the copy window's
/// load/run offset; the PT_LOAD carrying the template shares that
/// offset; the load image holds the initializer bytes at the template;
/// each thread-local symbol resolves to its offset from the bracket.
fn check_tls_image(dump: &[u8], elf: &[u8], probe: &TlsProbe) -> Result<TlsFacts, String> {
    let sym = |s: &str| {
        word_sym_value(dump, s)
            .ok_or_else(|| format!("symbol '{s}' not value-parseable from the linked artifact"))
    };
    let (lo, hi) = (sym("__rhu_tdata_lo")?, sym("__rhu_tdata_hi")?);
    let (sdata, edata, sidata) = (sym("__sdata")?, sym("__edata")?, sym("__sidata")?);
    let want = probe.template.len() as u64;
    if hi <= lo {
        return Err(format!("tls-template bracket did not open ({lo:#x} .. {hi:#x})"));
    }
    if hi - lo != want {
        return Err(format!("tls-template bracket spans {} bytes, wanted {want}", hi - lo));
    }
    if lo < sdata || hi > edata {
        return Err(format!(
            "template [{lo:#x}, {hi:#x}) outside the boot copy window [{sdata:#x}, {edata:#x})"
        ));
    }
    let phdrs = elf32_phdrs(elf)
        .ok_or_else(|| "linked artifact is not an ELF32 little-endian image".to_string())?;
    let tls_hdrs: Vec<&Phdr> = phdrs.iter().filter(|p| p.p_type == PT_TLS).collect();
    let [tls] = tls_hdrs.as_slice() else {
        return Err(format!("{} PT_TLS program headers, wanted exactly one", tls_hdrs.len()));
    };
    if tls.p_vaddr != lo {
        return Err(format!("PT_TLS starts at {:#x}, the bracket at {lo:#x}", tls.p_vaddr));
    }
    if tls.p_memsz != hi - lo || tls.p_filesz != tls.p_memsz {
        return Err(format!(
            "PT_TLS memsz {} filesz {} against a {}-byte template (zero-fill must be filed)",
            tls.p_memsz,
            tls.p_filesz,
            hi - lo
        ));
    }
    if tls.p_align != TLS_ALIGN {
        return Err(format!("PT_TLS align {}, the maps declare {TLS_ALIGN}", tls.p_align));
    }
    let offset = tls.p_paddr.wrapping_sub(tls.p_vaddr) & 0xffff_ffff;
    let window = sidata.wrapping_sub(sdata) & 0xffff_ffff;
    if offset != window {
        return Err(format!(
            "PT_TLS load/run offset {offset:#x} differs from the copy window's {window:#x}"
        ));
    }
    let holder = phdrs
        .iter()
        .find(|p| p.p_type == PT_LOAD && p.p_vaddr <= lo && hi <= p.p_vaddr + p.p_filesz)
        .ok_or_else(|| {
            "no PT_LOAD carries the template's bytes (zero-fill input took no load space)".to_string()
        })?;
    if holder.p_paddr.wrapping_sub(holder.p_vaddr) & 0xffff_ffff != offset {
        return Err(
            "the PT_LOAD carrying the template sits at a different load/run offset than PT_TLS"
                .to_string(),
        );
    }
    let at = (holder.p_offset + (lo - holder.p_vaddr)) as usize;
    let got = elf
        .get(at..at + probe.template.len())
        .ok_or_else(|| "template file range past the end of the artifact".to_string())?;
    if got != probe.template {
        return Err(format!(
            "load-image template bytes {} != initializer {}",
            hex(got),
            hex(probe.template)
        ));
    }
    for (name, off) in probe.syms {
        let v = sym(name)?;
        if v != *off {
            return Err(format!(
                "thread-local symbol {name} resolves to {v:#x}, wanted offset {off:#x}"
            ));
        }
    }
    Ok(TlsFacts { lo, hi, phdrs: phdrs.len() })
}

/// The constructor-array contract of one linked image: each bracket
/// pair parses and spans exactly its entries, inside the boot copy
/// window; the PT_LOAD carrying the bytes sits at the window's load/run
/// offset (an array left at load == run is the defect this map shape
/// closes); every entry word equals its function's symbol value, with
/// the Thumb bit set. Returns the three spans.
fn check_ctor_image(dump: &[u8], elf: &[u8], expect: &[CtorExpect]) -> Result<Vec<(u64, u64)>, String> {
    let sym = |s: &str| {
        word_sym_value(dump, s)
            .ok_or_else(|| format!("symbol '{s}' not value-parseable from the linked artifact"))
    };
    let (sdata, edata, sidata) = (sym("__sdata")?, sym("__edata")?, sym("__sidata")?);
    let window = sidata.wrapping_sub(sdata) & 0xffff_ffff;
    let phdrs = elf32_phdrs(elf)
        .ok_or_else(|| "linked artifact is not an ELF32 little-endian image".to_string())?;
    let mut spans = Vec::new();
    for x in expect {
        let (lo, hi) = (sym(x.lo_sym)?, sym(x.hi_sym)?);
        if hi < lo {
            return Err(format!("{} brackets inverted ({lo:#x} > {hi:#x})", x.section));
        }
        let want = 4 * x.entries.len() as u64;
        if hi - lo != want {
            return Err(format!("{} spans {} bytes, wanted {want}", x.section, hi - lo));
        }
        if lo < sdata || hi > edata {
            return Err(format!(
                "{} [{lo:#x}, {hi:#x}) outside the boot copy window [{sdata:#x}, {edata:#x})",
                x.section
            ));
        }
        let holder = phdrs
            .iter()
            .find(|p| p.p_type == PT_LOAD && p.p_vaddr <= lo && hi <= p.p_vaddr + p.p_filesz)
            .ok_or_else(|| format!("no PT_LOAD carries the bytes of {}", x.section))?;
        let off = holder.p_paddr.wrapping_sub(holder.p_vaddr) & 0xffff_ffff;
        if off != window {
            return Err(format!(
                "{} is carried at load/run offset {off:#x}, the copy window's is {window:#x}",
                x.section
            ));
        }
        for (i, name) in x.entries.iter().enumerate() {
            let at = (holder.p_offset + (lo - holder.p_vaddr)) as usize + 4 * i;
            let w = elf
                .get(at..at + 4)
                .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as u64)
                .ok_or_else(|| format!("{} entry {i} past the end of the artifact", x.section))?;
            let v = sym(name)?;
            if (w | 1) != (v | 1) {
                return Err(format!("{} entry {i} reads {w:#x}, {name} is at {v:#x}", x.section));
            }
            if w & 1 == 0 {
                return Err(format!("{} entry {i} ({name}) lacks the Thumb bit", x.section));
            }
        }
        spans.push((lo, hi));
    }
    Ok(spans)
}

/// First byte offset of a fixed needle in a byte string.
fn find_fixed(hay: &[u8], needle: &str) -> Option<usize> {
    let n = needle.as_bytes();
    if n.is_empty() || n.len() > hay.len() {
        return None;
    }
    (0..=hay.len() - n.len()).find(|&i| &hay[i..i + n.len()] == n)
}

/// The transcript order of the ctor image: every marker present,
/// strictly in the given order, all before `before` (the kernel's
/// constructors line, printed after the walks and before the module
/// list); `absent` (the destructor's marker) nowhere.
fn check_ctor_order(raw: &[u8], markers: &[&str], before: &str, absent: &str) -> Result<(), String> {
    let mut last = 0usize;
    for m in markers {
        let at = find_fixed(raw, m).ok_or_else(|| format!("marker '{m}' missing"))?;
        if at < last {
            return Err(format!("marker '{m}' printed out of order"));
        }
        last = at;
    }
    let b = find_fixed(raw, before).ok_or_else(|| format!("line '{before}' missing"))?;
    if b < last {
        return Err(format!("a constructor marker printed after '{before}'"));
    }
    if contains_fixed(raw, absent) {
        return Err(format!("'{absent}' printed: a destructor ran"));
    }
    Ok(())
}

/// The kernel's handshake line when no payload provides an entry or
/// states a generation: the line every default-path image prints.
fn default_entry_line(abi: u32) -> String {
    format!("[payload] entry: kernel default (hosting-API contract generation {abi})")
}

/// The kernel's handshake line for a payload-provided entry at the
/// kernel's own generation.
fn provided_entry_line(abi: u32) -> String {
    format!(
        "[payload] entry: payload-provided, built for hosting-API contract generation {abi} (this kernel: {abi})"
    )
}

/// The kernel's refusal of a stated generation that is not its own.
fn mismatch_sentence(stated: u32, kernel: u32) -> String {
    format!(
        "[payload] entry REFUSED: the payload was built for hosting-API contract generation {stated}, this kernel serves {kernel}"
    )
}

/// The strong-entry transcript: the kernel's handshake names the
/// payload's entry at the header's generation, the entry's marker
/// printed, the returned code reported, the default's constructors line
/// ABSENT (the default never ran), and no payload output at all
/// (managed Main never ran).
fn check_entry_strong(raw: &[u8], abi: u32) -> Result<(), String> {
    if !contains_fixed(raw, &provided_entry_line(abi)) {
        return Err("the payload-provided handshake line is missing".to_string());
    }
    if !contains_fixed(raw, ENTRY_MARK) {
        return Err("the strong entry's marker is missing".to_string());
    }
    if !contains_fixed(raw, &format!("[payload] managed Main returned {ENTRY_RETURN}")) {
        return Err(format!("the returned code {ENTRY_RETURN} is not reported"));
    }
    if contains_fixed(raw, "[payload] constructors ran:") {
        return Err("the kernel default ran beside the strong entry".to_string());
    }
    if !payload_stream(raw).is_empty() {
        return Err("payload output present: managed Main ran".to_string());
    }
    Ok(())
}

/// A refused entry: the sentence printed; managed Main never reported,
/// no payload output, no entry marker (the entry never ran).
fn check_refusal(raw: &[u8], sentence: &str) -> Result<(), String> {
    if !contains_fixed(raw, sentence) {
        return Err("the refusal sentence is missing".to_string());
    }
    if contains_fixed(raw, "[payload] managed Main returned") {
        return Err("managed Main reported after a refusal".to_string());
    }
    if contains_fixed(raw, ENTRY_MARK) {
        return Err("the refused entry ran".to_string());
    }
    if !payload_stream(raw).is_empty() {
        return Err("payload output after a refusal".to_string());
    }
    Ok(())
}

/// The placement refusal: the kernel's panic names the window, no
/// constructor marker printed (the walk never started), managed Main
/// never reported.
fn check_window_refusal(raw: &[u8]) -> Result<(), String> {
    if !contains_fixed(raw, "outside the boot copy window") {
        return Err("the placement refusal is missing".to_string());
    }
    if !contains_fixed(raw, "KERNEL PANIC") {
        return Err("the refusal did not go through the funnel".to_string());
    }
    if contains_fixed(raw, "[ctor]") {
        return Err("a constructor ran from a misplaced array".to_string());
    }
    if contains_fixed(raw, "[payload] managed Main returned") {
        return Err("managed Main reported after the refusal".to_string());
    }
    Ok(())
}

/// The payload linker as a command in words plus its reported version
/// (first line of `<command> --version`, CR-trimmed).
struct PayloadLd {
    words: Vec<String>,
    version: String,
}

/// A memory region line of a payload map (`NAME (..) : ORIGIN = X,
/// LENGTH = Y`): (origin, length) in bytes.
pub(crate) fn map_region(map: &str, name: &str) -> Option<(u64, u64)> {
    for line in map.lines() {
        let t = line.trim_end_matches('\r').trim();
        if t.starts_with(name) && t.contains("ORIGIN =") && t.contains("LENGTH =") {
            let origin = t.split("ORIGIN =").nth(1)?.split(',').next()?.trim();
            let length = t.split("LENGTH =").nth(1)?.trim();
            return Some((parse_size(origin)?, parse_size(length)?));
        }
    }
    None
}

/// A linker-script size token: `0x08000000`, `3M`, `1536K`, `64`.
fn parse_size(t: &str) -> Option<u64> {
    let t = t.trim().trim_end_matches(';').trim();
    if let Some(h) = t.strip_prefix("0x") {
        return u64::from_str_radix(h, 16).ok();
    }
    if let Some(m) = t.strip_suffix('M') {
        return m.parse::<u64>().ok().map(|v| v << 20);
    }
    if let Some(k) = t.strip_suffix('K') {
        return k.parse::<u64>().ok().map(|v| v << 10);
    }
    t.parse::<u64>().ok()
}

/// An rt-link's linker log split: the unresolved names (one warning each
/// under --warn-unresolved-symbols, their reference lines dropped) and
/// every other diagnostic line.
pub(crate) fn split_ld_log(text: &str) -> (Vec<String>, Vec<String>) {
    let mut names = Vec::new();
    let mut other = Vec::new();
    for raw in text.lines() {
        let l = raw.trim_end_matches('\r');
        if l.trim().is_empty() || l.starts_with(">>>") || l.contains("GNU-stack") || l.contains("deprecated") {
            continue;
        }
        if let Some(rest) = l.split("undefined symbol: ").nth(1) {
            names.push(rest.split_whitespace().next().unwrap_or("").to_string());
        } else {
            other.push(l.to_string());
        }
    }
    names.sort();
    names.dedup();
    (names, other)
}

/// A bracket pair from the nm dump, ordered.
fn bracket(dump: &[u8], lo: &str, hi: &str) -> Result<(u64, u64), String> {
    match (word_sym_value(dump, lo), word_sym_value(dump, hi)) {
        (Some(a), Some(b)) if a <= b => Ok((a, b)),
        (Some(a), Some(b)) => Err(format!("bracket {lo}..{hi} inverted ({a:#x} > {b:#x})")),
        _ => Err(format!("bracket pair {lo}/{hi} not value-parseable from the artifact")),
    }
}

/// The rt-link placement check over parsed inputs (the nm dump, the
/// program headers, the unresolved names, the frozen demand list sorted,
/// the map's CODE region): every class the frozen object carries at the
/// bracket the map declares, exact; the code brackets inside module 0's,
/// in order; the rehydrated region inside the zero-fill window; the
/// read-only-after-relocation run inside the copy window; the TLS segment
/// at the template; every unresolved name in the frozen demand list; the
/// flash image inside CODE; the arena window over the floor.
pub(crate) fn check_rt_placement(
    dump: &[u8],
    phdrs: &[Phdr],
    unresolved: &[String],
    demand: &[String],
    code: (u64, u64),
    want_sizes: &RtBrackets,
) -> Result<RtFacts, String> {
    let (m_lo, m_hi) = bracket(dump, "__rhu_managedcode_lo", "__rhu_managedcode_hi")?;
    let (u_lo, u_hi) = bracket(dump, "__rhu_unbox_lo", "__rhu_unbox_hi")?;
    let (h_lo, h_hi) = bracket(dump, "__rhu_hydrated_lo", "__rhu_hydrated_hi")?;
    let (t_lo, t_hi) = bracket(dump, "__rhu_tdata_lo", "__rhu_tdata_hi")?;
    let (i_lo, i_hi) = bracket(dump, "__init_array_start", "__init_array_end")?;
    let (mo_lo, mo_hi) = bracket(dump, "__start___modules", "__stop___modules")?;
    let (x_lo, x_hi) = bracket(dump, "__exidx_start", "__exidx_end")?;
    let _frozen = bracket(dump, "__rhu_frozen_lo", "__rhu_frozen_hi")?;
    let (stext, etext) = bracket(dump, "__stext", "__etext")?;
    let (sdata, edata) = bracket(dump, "__sdata", "__edata")?;
    let (sbss, ebss) = bracket(dump, "__sbss", "__ebss")?;
    let (a_lo, a_hi) = bracket(dump, "__rhu_arena_lo", "__rhu_arena_hi")?;
    let floor = word_sym_value(dump, "RHU_ARENA_MIN")
        .ok_or_else(|| "RHU_ARENA_MIN not value-parseable from the artifact".to_string())?;
    let want = [
        ("managed-code", m_hi - m_lo, want_sizes.managed),
        ("unboxing-stub", u_hi - u_lo, want_sizes.unbox),
        ("rehydrated-data", h_hi - h_lo, want_sizes.hydrated),
        ("TLS template", t_hi - t_lo, want_sizes.tdata),
        (".init_array", i_hi - i_lo, want_sizes.init_array),
        ("module table", mo_hi - mo_lo, want_sizes.modules),
    ];
    for (what, got, exp) in want {
        if got != exp {
            return Err(format!("{what} bracket reads {got} bytes, the frozen object carries {exp}"));
        }
    }
    if x_lo >= x_hi {
        return Err(format!("exidx bracket not strictly nonzero ({x_lo:#x} .. {x_hi:#x})"));
    }
    if m_lo < stext || m_hi > u_lo || u_hi > etext {
        return Err(format!(
            "managed code [{m_lo:#x}, {m_hi:#x}) and unboxing stubs [{u_lo:#x}, {u_hi:#x}) not inside module 0's code bracket [{stext:#x}, {etext:#x}) in order"
        ));
    }
    if h_lo < sbss || h_hi > ebss {
        return Err(format!(
            "rehydrated-data region [{h_lo:#x}, {h_hi:#x}) outside the zero-fill window [{sbss:#x}, {ebss:#x})"
        ));
    }
    let relro = phdrs
        .iter()
        .find(|p| p.p_type == PT_GNU_RELRO)
        .ok_or_else(|| "no read-only-after-relocation segment in the artifact".to_string())?;
    if relro.p_vaddr < sdata || relro.p_vaddr + relro.p_memsz > edata {
        return Err(format!(
            "read-only-after-relocation segment [{:#x}, +{:#x}) outside the copy window [{sdata:#x}, {edata:#x})",
            relro.p_vaddr, relro.p_memsz
        ));
    }
    let tls = phdrs
        .iter()
        .find(|p| p.p_type == PT_TLS)
        .ok_or_else(|| "no PT_TLS in the artifact".to_string())?;
    if tls.p_vaddr != t_lo || tls.p_memsz != want_sizes.tdata {
        return Err(format!(
            "PT_TLS at {:#x} sized {} (want the template bracket {t_lo:#x}, {} bytes)",
            tls.p_vaddr, tls.p_memsz, want_sizes.tdata
        ));
    }
    for n in unresolved {
        if demand.binary_search(n).is_err() {
            return Err(format!("unresolved name '{n}' outside the frozen demand list"));
        }
    }
    let (c_org, c_len) = code;
    let code_end = phdrs
        .iter()
        .filter(|p| p.p_type == PT_LOAD && p.p_paddr >= c_org && p.p_paddr < c_org + c_len)
        .map(|p| p.p_paddr + p.p_filesz)
        .max()
        .unwrap_or(c_org);
    if code_end > c_org + c_len {
        return Err(format!("flash image ends at {code_end:#x}, past the CODE region"));
    }
    if a_hi - a_lo < floor {
        return Err(format!("arena window {} bytes under the floor {floor}", a_hi - a_lo));
    }
    Ok(RtFacts {
        managed: m_hi - m_lo,
        unbox: u_hi - u_lo,
        hydrated: h_hi - h_lo,
        tdata: t_hi - t_lo,
        init_array: i_hi - i_lo,
        modules: mo_hi - mo_lo,
        exidx: x_hi - x_lo,
        relro: relro.p_memsz,
        unresolved: unresolved.len(),
        code_free: c_org + c_len - code_end,
        arena_window: a_hi - a_lo,
        arena_floor: floor,
    })
}

/// The rt-world brackets on a minirt payload-family artifact: managed
/// code exactly the frozen object's own, no unboxing stubs, no rehydrated
/// data.
fn check_minirt_brackets(dump: &[u8], managed_want: u64) -> Result<(), String> {
    let (m_lo, m_hi) = bracket(dump, "__rhu_managedcode_lo", "__rhu_managedcode_hi")?;
    let (u_lo, u_hi) = bracket(dump, "__rhu_unbox_lo", "__rhu_unbox_hi")?;
    let (h_lo, h_hi) = bracket(dump, "__rhu_hydrated_lo", "__rhu_hydrated_hi")?;
    if m_hi - m_lo != managed_want {
        return Err(format!(
            "managed-code bracket reads {} bytes, the frozen object carries {managed_want}",
            m_hi - m_lo
        ));
    }
    if u_hi != u_lo || h_hi != h_lo {
        return Err(format!(
            "unboxing-stub bracket {} bytes / rehydrated-data bracket {} bytes on an object that carries neither",
            u_hi - u_lo,
            h_hi - h_lo
        ));
    }
    Ok(())
}

fn resolve_payload_ld(repo: &Path, facts: &RustcFacts) -> Result<PayloadLd, String> {
    let words: Vec<String> = match std::env::var("RHU_PAYLOAD_LD") {
        Ok(v) if !v.trim().is_empty() => {
            command_words(&v).map_err(|e| format!("{e} (RHU_PAYLOAD_LD)"))?
        }
        _ => {
            let lld = llvm_tool(facts, "rust-lld").map_err(|e| {
                format!(
                    "no payload linker resolved: set RHU_PAYLOAD_LD (a command; a double-quoted word keeps its spaces) -- the default rust-lld is unavailable: {e}"
                )
            })?;
            vec![
                lld.to_string_lossy().into_owned(),
                "-flavor".to_string(),
                "gnu".to_string(),
            ]
        }
    };
    let _ = repo;
    let probe_out = std::env::temp_dir().join(format!("rhu-gate-ld-probe-{}.txt", std::process::id()));
    let probe_err = std::env::temp_dir().join(format!("rhu-gate-ld-probe-{}.stderr", std::process::id()));
    let mut probe_args: Vec<&str> = words[1..].iter().map(String::as_str).collect();
    probe_args.push("--version");
    let outcome = run_captured(
        Path::new(&words[0]),
        &probe_args,
        &probe_out,
        &probe_err,
        Duration::from_secs(20),
    );
    let bytes = std::fs::read(&probe_out).unwrap_or_default();
    let _ = std::fs::remove_file(&probe_out);
    let _ = std::fs::remove_file(&probe_err);
    let first = String::from_utf8_lossy(&bytes)
        .lines()
        .next()
        .unwrap_or("")
        .trim_end_matches('\r')
        .to_string();
    if !matches!(outcome, Ok(ref o) if o.exit == 0) || first.is_empty() {
        return Err(format!(
            "the payload linker '{}' did not answer --version (the command: {}; set RHU_PAYLOAD_LD to a working linker command; a probe that hangs past 20 s is killed and counts as not runnable)",
            words[0],
            words.join(" ")
        ));
    }
    Ok(PayloadLd { words, version: first })
}

/// A Command for the linker: repo-relative arguments from the repo root,
/// MSYS path conversion disabled (inert for native and WSL linkers,
/// load-bearing for MSYS-shimmed ones).
fn ld_command(repo: &Path, words: &[String]) -> Command {
    let mut cmd = Command::new(&words[0]);
    cmd.args(&words[1..])
        .current_dir(repo)
        .env("MSYS_NO_PATHCONV", "1")
        .env("MSYS2_ARG_CONV_EXCL", "*");
    cmd
}

struct MPayload {
    ctx: GateCtx,
    facts: RustcFacts,
    ld: PayloadLd,
    gold_dir: PathBuf,
    /// RHU_ABI as include/rhu.h states it, read at gate start.
    abi: u32,
    /// The frozen merged runtime object's demand list (sorted), read at
    /// the frozen-inputs case; every unresolved name of an rt-link must be
    /// in it.
    rt_demand: Vec<String>,
}

/// One linked-and-run payload case: staged artifact identity + the run.
struct PayloadRun {
    sha: String,
    path_rel: String,
    raw: Vec<u8>,
    exit: i32,
}

fn setup(repo: &Path) -> (RustcFacts, PathBuf, String) {
    let facts = match rustc_facts() {
        Ok(f) => f,
        Err(e) => {
            println!("rhu-gate: {e}");
            exit(1);
        }
    };
    let (qemu, qemu_v) = match resolve_qemu(repo) {
        Ok(q) => q,
        Err(e) => {
            println!("rhu-gate: {e}");
            exit(1);
        }
    };
    (facts, qemu, qemu_v)
}

impl MPayload {
    /// Case line in the results schema's exact shape: exit/exit_want are
    /// pre-rendered (this gate emits `null` exits on link/tool failures),
    /// image is null when no staged artifact exists yet, stats is an
    /// optional tail. `timing` (an additive, report-only field) renders
    /// between control and image; None keeps the line in its
    /// established shape.
    fn emit_case(
        &mut self,
        case: &str,
        verdict: &str,
        exit_s: &str,
        want_s: &str,
        sha: &str,
        path_rel: &str,
        golden: &str,
        timing: Option<&str>,
        stats: Option<&str>,
    ) {
        let img = if sha.is_empty() {
            "null".to_string()
        } else {
            format!("{{\"path\":\"{path_rel}\",\"sha256\":\"{sha}\"}}")
        };
        let clk = match timing {
            Some(t) => format!(",\"timing\":\"{t}\""),
            None => String::new(),
        };
        let tail = match stats {
            Some(st) => format!(",\"stats\":{st}"),
            None => String::new(),
        };
        self.ctx.results.emit(&format!(
            "{{\"schema\":1,\"type\":\"case\",\"gate\":\"payload\",\"case\":\"{case}\",\"verdict\":\"{verdict}\",\"exit\":{exit_s},\"exit_want\":{want_s},\"control\":\"positive\"{clk},\"image\":{img},\"golden\":{golden}{tail}}}"
        ));
        self.ctx.results.tally(verdict, "positive");
    }

    fn fail_case(
        &mut self,
        case: &str,
        exit_s: &str,
        want_s: &str,
        sha: &str,
        path_rel: &str,
        timing: Option<&str>,
        msg: &str,
    ) -> ! {
        self.emit_case(case, "fail", exit_s, want_s, sha, path_rel, "null", timing, None);
        self.ctx.gate_fail(&format!("{case}: {msg}"))
    }

    /// Case line for a negative control: no exit (a refused link has
    /// none), no artifact, the breakage named in `broke`.
    fn emit_control_case(&mut self, case: &str, verdict: &str, broke: &str) {
        self.ctx.results.emit(&format!(
            "{{\"schema\":1,\"type\":\"case\",\"gate\":\"payload\",\"case\":\"{case}\",\"verdict\":\"{verdict}\",\"exit\":null,\"exit_want\":null,\"control\":\"negative\",\"image\":null,\"broke\":\"{broke}\"}}"
        ));
        self.ctx.results.tally(verdict, "negative");
    }

    /// Case line for a negative control that RAN (a refusal at run
    /// time): the exit and the artifact recorded, the breakage named.
    fn emit_control_run(
        &mut self,
        case: &str,
        verdict: &str,
        code: i32,
        want: i32,
        sha: &str,
        path_rel: &str,
        broke: &str,
    ) {
        self.ctx.results.emit(&format!(
            "{{\"schema\":1,\"type\":\"case\",\"gate\":\"payload\",\"case\":\"{case}\",\"verdict\":\"{verdict}\",\"exit\":{code},\"exit_want\":{want},\"control\":\"negative\",\"image\":{{\"path\":\"{path_rel}\",\"sha256\":\"{sha}\"}},\"broke\":\"{broke}\"}}"
        ));
        self.ctx.results.tally(verdict, "negative");
    }

    fn verify_frozen_inputs(&mut self) {
        println!("== frozen inputs: sha256 vs goldens/payload/MANIFEST.md ==");
        for f in [
            "rtsmoke.o",
            "gctest.o",
            "rtsmoke-mps2.golden.txt",
            "gctest-mps2.golden.txt",
        ] {
            if !self.gold_dir.join(f).is_file() {
                self.fail_case(
                    "frozen-inputs",
                    "null",
                    "null",
                    "",
                    "",
                    None,
                    &format!("{f} missing from goldens/payload/"),
                );
            }
        }
        let pins = [
            ("rtsmoke.o", SHA_RTSMOKE_O, "rtsmoke.o sha256 mismatch (frozen artifact moved)"),
            ("gctest.o", SHA_GCTEST_O, "gctest.o sha256 mismatch (frozen artifact moved)"),
            ("rtsmoke-mps2.golden.txt", SHA_RTSMOKE_G, "rtsmoke golden sha256 mismatch"),
            ("gctest-mps2.golden.txt", SHA_GCTEST_G, "gctest golden sha256 mismatch"),
        ];
        for (f, want, msg) in pins {
            let got = sha256_file(&self.gold_dir.join(f)).unwrap_or_default();
            if got != want {
                self.fail_case("frozen-inputs", "null", "null", "", "", None, msg);
            }
        }
        let mpath = self.ctx.repo.join("goldens").join("rt").join("MANIFEST.md");
        let msha = sha256_file(&mpath).unwrap_or_default();
        if msha != crate::gates::rt::SHA_FROZEN_MANIFEST {
            self.fail_case(
                "frozen-inputs",
                "null",
                "null",
                "",
                "",
                None,
                "goldens/rt/MANIFEST.md sha256 does not match the rt tier's pin (the frozen record moved; this gate links its merged object)",
            );
        }
        let mtext = std::fs::read_to_string(&mpath).unwrap_or_default();
        let records = crate::gates::gc::parse_vendored_manifest(&mtext);
        for want in [RT_MERGED_O, RT_DEMAND_TXT] {
            let Some((_, sha)) = records.iter().find(|(p, _)| p.as_str() == want) else {
                self.fail_case(
                    "frozen-inputs",
                    "null",
                    "null",
                    "",
                    "",
                    None,
                    &format!("{want} has no record in goldens/rt/MANIFEST.md"),
                );
            };
            let got = sha256_file(&self.ctx.repo.join(want.replace('/', std::path::MAIN_SEPARATOR_STR)))
                .unwrap_or_default();
            if &got != sha {
                self.fail_case(
                    "frozen-inputs",
                    "null",
                    "null",
                    "",
                    "",
                    None,
                    &format!("{want} sha256 mismatch against goldens/rt/MANIFEST.md (frozen artifact moved)"),
                );
            }
        }
        let demand = std::fs::read_to_string(
            self.ctx.repo.join(RT_DEMAND_TXT.replace('/', std::path::MAIN_SEPARATOR_STR)),
        )
        .unwrap_or_default();
        let mut names: Vec<String> = demand
            .lines()
            .map(|l| l.trim_end_matches('\r').trim().to_string())
            .filter(|l| !l.is_empty())
            .collect();
        names.sort();
        names.dedup();
        if names.is_empty() {
            self.fail_case("frozen-inputs", "null", "null", "", "", None, "the frozen demand list is empty");
        }
        self.rt_demand = names;
        let stats = format!(
            "{{\"rtsmoke_o\":\"{SHA_RTSMOKE_O}\",\"gctest_o\":\"{SHA_GCTEST_O}\",\"rtsmoke_golden\":\"{SHA_RTSMOKE_G}\",\"gctest_golden\":\"{SHA_GCTEST_G}\",\"rt_manifest\":\"{msha}\",\"rt_demand_names\":{}}}",
            self.rt_demand.len()
        );
        self.emit_case("frozen-inputs", "pass", "null", "null", "", "", "null", None, Some(&stats));
        println!(
            "== frozen inputs: PASS (4/4 hashes match the MANIFEST; the rt tier's merged object and demand list tie goldens/rt/MANIFEST.md, {} names) ==",
            self.rt_demand.len()
        );
    }

    fn build_staticlib(&mut self) {
        println!("== build kernel staticlib (--lib --features payload) ==");
        let log = self.ctx.out.join("build-staticlib.log");
        let ok = match crate::tools::cargo_build_kernel(
            &self.ctx.repo,
            &["--lib", "--features", "payload"],
            &log,
        ) {
            Ok(ok) => ok,
            Err(e) => {
                println!("rhu-gate: {e}");
                false
            }
        };
        if !ok {
            echo_file(&log, "");
            self.fail_case("staticlib-build", "null", "null", "", "", None, "cargo build error");
        }
        let lib = self.ctx.repo.join(AN500_LIB);
        if !lib.is_file() {
            self.fail_case("staticlib-build", "null", "null", "", "", None, "librhu_kernel.a not produced");
        }
    }

    /// One bounded linker invocation under `map_rel`: the object
    /// `obj_rel` (a frozen payload object, or the rt tier's merged runtime
    /// object), any extra objects after it, then the staticlib `lib_rel`;
    /// `ld_extra` are the family's own linker words (none for the minirt
    /// links). Returns (linked, linker log, artifact path); the caller
    /// decides what a failed link means (a case fails on it, a control
    /// expects it).
    fn link_raw(
        &mut self,
        name: &str,
        obj_rel: &str,
        extra: &[&str],
        lib_rel: &str,
        map_rel: &str,
        ld_extra: &[&str],
    ) -> (bool, PathBuf, String) {
        println!("== link {name} (RHU_PAYLOAD_LD) ==");
        let path_rel = format!("src/kernel/target/payload-gate/{name}-rhu.elf");
        let ld_log = self.ctx.out.join(format!("{name}-ld.log"));
        let ld_stdout = self.ctx.out.join(format!("{name}-ld.stdout"));
        let mapfile_rel = format!("src/kernel/target/payload-gate/{name}-rhu.map");
        let obj_rel = obj_rel.to_string();
        let status = {
            let log_f = std::fs::File::create(&ld_log);
            let out_f = std::fs::File::create(&ld_stdout);
            match (log_f, out_f) {
                (Ok(log_f), Ok(out_f)) => {
                    let words = self.ld.words.clone();
                    let mut cmd = ld_command(&self.ctx.repo, &words);
                    let mut args: Vec<String> = vec![
                        "-T".to_string(),
                        map_rel.to_string(),
                        "--gc-sections".to_string(),
                        format!("-Map={mapfile_rel}"),
                    ];
                    args.extend(ld_extra.iter().map(|s| s.to_string()));
                    args.push(obj_rel.clone());
                    args.extend(extra.iter().map(|s| s.to_string()));
                    args.extend([lib_rel.to_string(), "-o".to_string(), path_rel.clone()]);
                    cmd.args(&args)
                        .stdin(Stdio::null())
                    .stdout(Stdio::from(out_f))
                    .stderr(Stdio::from(log_f));
                    status_bounded(&mut cmd, Duration::from_secs(LINK_TIMEOUT_S))
                }
                _ => {
                    self.ctx.gate_fail(&format!("{name}: cannot create linker log files"));
                }
            }
        };
        let ok = match status {
            Ok(Some(ref s)) => s.success(),
            Ok(None) => {
                println!(
                    "  (link killed at {LINK_TIMEOUT_S} s -- a wedged linker fails the gate, never parks it)"
                );
                false
            }
            Err(ref e) => {
                println!("  (link spawn: {e})");
                false
            }
        };
        (ok, ld_log, path_rel)
    }

    /// Link one frozen payload object (plus any extra objects) against
    /// the staticlib under the payload board map; assert the artifact's
    /// symbol contract by nm.
    fn link_image(&mut self, name: &str, frozen: &str, extra: &[&str]) -> (String, String) {
        let (ok, ld_log, path_rel) =
            self.link_raw(name, &format!("goldens/payload/{frozen}.o"), extra, AN500_LIB, AN500_MAP, &[]);
        let elf = self.ctx.repo.join(&path_rel);
        if !ok {
            echo_file(&ld_log, "");
            self.emit_case(name, "fail", "null", "0", "", &path_rel, "null", None, None);
            self.ctx.gate_fail(&format!("{name}: link failed (see {name}-ld.log)"));
        }
        let chatter = std::fs::read(&ld_log).unwrap_or_default();
        let text = String::from_utf8_lossy(&chatter);
        for line in text
            .lines()
            .filter(|l| !l.contains("GNU-stack") && !l.contains("deprecated"))
            .take(10)
        {
            println!("{}", line.trim_end_matches('\r'));
        }
        let sha = match sha256_file(&elf) {
            Ok(s) => s,
            Err(e) => self.ctx.gate_fail(&format!("{name}: cannot hash linked artifact: {e}")),
        };
        let nm = match llvm_tool(&self.facts, "llvm-nm") {
            Ok(p) => p,
            Err(e) => self.ctx.gate_fail(&format!("{name}: {e}")),
        };
        let elf_arg = elf.to_string_lossy().into_owned();
        let sym = self.ctx.out.join(format!("{name}.nm"));
        let scratch = self.ctx.out.join(format!("{name}.tool.stderr"));
        let nm_run = run_captured(&nm, &[&elf_arg], &sym, &scratch, Duration::from_secs(60));
        if !matches!(nm_run, Ok(ref o) if o.exit == 0) {
            self.fail_case(name, "null", "0", &sha, &path_rel, None, "llvm-nm failed on the linked ELF");
        }
        let dump = std::fs::read(&sym).unwrap_or_default();
        for s in [
            "__managed__Main",
            "__rhu_debug_layout",
            "__rhu_sched",
            "__rhu_ticks",
            "rhu_payload_entry",
            "__rhu_payload_entry_weak",
            "rhu_payload_abi",
        ] {
            if !word_sym_present(&dump, s) {
                self.fail_case(
                    name,
                    "null",
                    "0",
                    &sha,
                    &path_rel,
                    None,
                    &format!("symbol '{s}' absent from the linked artifact"),
                );
            }
        }
        let elf_bytes = std::fs::read(&elf).unwrap_or_default();
        if let Err(e) = check_records(&dump, &elf_bytes, self.abi) {
            self.fail_case(name, "null", "0", &sha, &path_rel, None, &format!("image records: {e}"));
        }
        let (Some(flo), Some(fhi)) = (
            word_sym_value(&dump, "__rhu_frozen_lo"),
            word_sym_value(&dump, "__rhu_frozen_hi"),
        ) else {
            self.ctx.gate_fail(&format!(
                "{name}: frozen bracket pair not value-parseable from the linked artifact"
            ));
        };
        if flo > fhi {
            self.ctx.gate_fail(&format!(
                "{name}: frozen bracket inverted ({flo:#x} > {fhi:#x})"
            ));
        }
        let (Some(xlo), Some(xhi)) = (
            word_sym_value(&dump, "__exidx_start"),
            word_sym_value(&dump, "__exidx_end"),
        ) else {
            self.ctx.gate_fail(&format!(
                "{name}: exidx bracket pair not value-parseable from the linked artifact"
            ));
        };
        if xlo >= xhi {
            self.ctx.gate_fail(&format!(
                "{name}: exidx bracket not strictly nonzero ({xlo:#x} .. {xhi:#x}) -- managed exidx input missed its bracket"
            ));
        }
        let (Some(tlo), Some(thi)) = (
            word_sym_value(&dump, "__rhu_tdata_lo"),
            word_sym_value(&dump, "__rhu_tdata_hi"),
        ) else {
            self.ctx.gate_fail(&format!(
                "{name}: tls-template bracket pair not value-parseable from the linked artifact"
            ));
        };
        if tlo > thi {
            self.ctx.gate_fail(&format!(
                "{name}: tls-template bracket inverted ({tlo:#x} > {thi:#x})"
            ));
        }
        let managed_want = if frozen == "gctest" { MANAGED_BYTES_GCTEST } else { MANAGED_BYTES_RTSMOKE };
        if let Err(e) = check_minirt_brackets(&dump, managed_want) {
            self.ctx.gate_fail(&format!("{name}: {e}"));
        }
        if let Ok(size) = llvm_tool(&self.facts, "llvm-size") {
            let size_out = self.ctx.out.join(format!("{name}.size"));
            let _ = run_captured(&size, &[&elf_arg], &size_out, &scratch, Duration::from_secs(60));
            let bytes = std::fs::read(&size_out).unwrap_or_default();
            if let Some(last) = String::from_utf8_lossy(&bytes).lines().rev().find(|l| !l.trim().is_empty()) {
                println!("  size: {}", last.trim_end_matches('\r'));
            }
        }
        (sha, path_rel)
    }

    /// Link one frozen payload object alone (the rtsmoke and gctest
    /// cases).
    fn link_payload(&mut self, name: &str) -> (String, String) {
        self.link_image(name, name, &[])
    }

    /// Assemble one module-asm input with the pinned toolchain's llc:
    /// the source is written under the gate's target dir (`sub`) and
    /// the object lands beside it (repo-relative, for the link argv).
    /// Every line becomes one `module asm` string with the two
    /// characters LLVM's string syntax escapes.
    fn assemble_module_asm(&mut self, sub: &str, name: &str, lines: &[String]) -> String {
        let dir = self.ctx.out.join(sub);
        let _ = std::fs::create_dir_all(&dir);
        let mut ll = String::from("target triple = \"thumbv7em-none-eabihf\"\n");
        for line in lines {
            ll.push_str(&format!(
                "module asm \"{}\"\n",
                line.replace('\\', "\\5C").replace('"', "\\22")
            ));
        }
        let ll_path = dir.join(format!("{name}.ll"));
        write_fixture(&ll_path, ll.as_bytes());
        let obj_rel = format!("src/kernel/target/payload-gate/{sub}/{name}.o");
        let llc = match llvm_tool(&self.facts, "llc") {
            Ok(p) => p,
            Err(e) => self.ctx.gate_fail(&format!("{name}: {e}")),
        };
        let log = dir.join(format!("{name}.llc.log"));
        let err = dir.join(format!("{name}.llc.stderr"));
        let ll_arg = ll_path.to_string_lossy().into_owned();
        let obj_arg = self.ctx.repo.join(&obj_rel).to_string_lossy().into_owned();
        let run = run_captured(
            &llc,
            &["-mtriple=thumbv7em-none-eabihf", "-filetype=obj", &ll_arg, "-o", &obj_arg],
            &log,
            &err,
            Duration::from_secs(60),
        );
        if !matches!(run, Ok(ref o) if o.exit == 0) || !self.ctx.repo.join(&obj_rel).is_file() {
            echo_file(&err, "  | ");
            self.fail_case(name, "null", "0", "", "", None, "llc did not assemble the input");
        }
        obj_rel
    }

    /// One module-0 TLS template probe, assembled under `tls/`.
    fn assemble_tls_probe(&mut self, probe: &TlsProbe) -> String {
        let lines: Vec<String> = probe.asm.iter().map(|s| s.to_string()).collect();
        self.assemble_module_asm("tls", probe.case, &lines)
    }

    /// The rtsmoke run checks, shared by the rtsmoke case and the
    /// template cases (whose payload IS rtsmoke): flavor line, managed
    /// Main return, no panic/exception, CR bytes present, exit 0, and
    /// the payload stream byte-identical to the golden. Returns the exit
    /// code rendered for the case line.
    fn check_rtsmoke_run(&mut self, case: &str, run: &PayloadRun) -> String {
        let (sha, path_rel, code) = (run.sha.clone(), run.path_rel.clone(), run.exit);
        let code_s = code.to_string();
        if !contains_fixed(&run.raw, "flavor: payload") {
            self.fail_case(case, &code_s, "0", &sha, &path_rel, None, "kernel flavor line missing");
        }
        if !contains_fixed(&run.raw, "[payload] managed Main returned 0") {
            self.fail_case(case, &code_s, "0", &sha, &path_rel, None, "managed Main return marker missing");
        }
        if !contains_fixed(&run.raw, &default_entry_line(self.abi)) {
            self.fail_case(case, &code_s, "0", &sha, &path_rel, None, "the kernel-default handshake line is missing");
        }
        for m in ["KERNEL PANIC", "EXCEPTION:"] {
            if contains_fixed(&run.raw, m) {
                self.fail_case(case, &code_s, "0", &sha, &path_rel, None, &format!("forbidden pattern '{m}' present"));
            }
        }
        if cr_count(&run.raw) == 0 {
            self.fail_case(case, &code_s, "0", &sha, &path_rel, None, "no CR bytes in raw capture");
        }
        if code != 0 {
            self.fail_case(case, &code_s, "0", &sha, &path_rel, None, &format!("exit {code}, wanted 0"));
        }
        let golden = std::fs::read(self.gold_dir.join("rtsmoke-mps2.golden.txt")).unwrap_or_default();
        let stream = payload_stream(&run.raw);
        let glf = lf_normalize(&golden);
        if stream != glf {
            println!("--- payload stream vs golden (diff) ---");
            print_first_divergence(&glf, &stream);
            self.fail_case(case, &code_s, "0", &sha, &path_rel, None, "payload stream differs from golden");
        }
        code_s
    }

    /// The module-0 TLS template cases (TLS_PROBES): assemble, link
    /// beside rtsmoke under the map of record, check the template
    /// contract on the artifact, then run the image with the rtsmoke
    /// checks -- a live template changes nothing the payload prints.
    fn run_tls_probes(&mut self) {
        let gjson = golden_json("goldens/payload/rtsmoke-mps2.golden.txt", SHA_RTSMOKE_G, "bytes");
        for probe in &TLS_PROBES {
            let obj_rel = self.assemble_tls_probe(probe);
            let (sha, path_rel) = self.link_image(probe.case, "rtsmoke", &[obj_rel.as_str()]);
            let dump = std::fs::read(self.ctx.out.join(format!("{}.nm", probe.case))).unwrap_or_default();
            let elf = std::fs::read(self.ctx.repo.join(&path_rel)).unwrap_or_default();
            let facts = match check_tls_image(&dump, &elf, probe) {
                Ok(f) => f,
                Err(e) => self.fail_case(probe.case, "null", "0", &sha, &path_rel, None, &e),
            };
            let bytes = facts.hi - facts.lo;
            println!(
                "  template: [{:#x}, {:#x}) {bytes} bytes; PT_TLS at the bracket inside the copy window; {} program headers",
                facts.lo, facts.hi, facts.phdrs
            );
            let run = self.run_payload_qemu(probe.case, &sha, &path_rel, ExecProfile::Parity);
            let code_s = self.check_rtsmoke_run(probe.case, &run);
            let stats = format!("{{\"template_bytes\":{bytes},\"program_headers\":{}}}", facts.phdrs);
            self.emit_case(probe.case, "pass", &code_s, "0", &sha, &path_rel, &gjson, None, Some(&stats));
            println!(
                "== case {}: PASS (exit 0, {bytes}-byte template at the bracket inside the copy window, payload stream byte-identical) ==",
                probe.case
            );
        }
    }

    /// Negative control: the template brackets inside .data -- the shape
    /// the linker refuses for thread-local input. The map of record is
    /// doctored by one exact insertion (the KEEP patterns after .data's
    /// own input line, so thread-local input matches .data first); the
    /// mixed probe must then be REFUSED with the section-flags error.
    fn run_tls_shape_control(&mut self) {
        const CASE: &str = "tls-shape-control";
        const BROKE: &str = "thread-local template brackets inside .data (the shape the linker refuses)";
        let map_of_record = self.ctx.repo.join("src/kernel/payload-mps2-an500.x");
        let map = std::fs::read_to_string(&map_of_record).unwrap_or_default();
        let data_line = "    *(.data .data.*);\n";
        if map.matches(data_line).count() != 1 {
            self.ctx.gate_fail(&format!(
                "{CASE}: cannot arm the control -- the map's .data input line is not unique"
            ));
        }
        let doctored = map.replacen(
            data_line,
            "    *(.data .data.*);\n    KEEP(*(.tdata .tdata.*));\n    KEEP(*(.tbss .tbss.*));\n",
            1,
        );
        let map_rel = "src/kernel/target/payload-gate/tls/shape-control.x";
        write_fixture(&self.ctx.repo.join(map_rel), doctored.as_bytes());
        let obj_rel = self.assemble_tls_probe(&TLS_PROBES[2]);
        let (ok, ld_log, _) =
            self.link_raw(CASE, "goldens/payload/rtsmoke.o", &[obj_rel.as_str()], AN500_LIB, map_rel, &[]);
        let text = String::from_utf8_lossy(&std::fs::read(&ld_log).unwrap_or_default()).into_owned();
        let refusal = text
            .lines()
            .find(|l| l.contains("incompatible section flags"))
            .map(|l| l.trim_end_matches('\r').to_string());
        if ok || refusal.is_none() {
            echo_file(&ld_log, "  | ");
            self.emit_control_case(CASE, "fail", BROKE);
            self.ctx.gate_fail(&format!(
                "{CASE}: the doctored shape was NOT refused with the section-flags error"
            ));
        }
        println!("  refused as predicted: {}", refusal.unwrap_or_default());
        self.emit_control_case(CASE, "pass", BROKE);
        println!("== control {CASE}: PASS (linker refused thread-local input inside .data: incompatible section flags) ==");
    }

    /// The ctor case: the kernel's own constructor input beside rtsmoke
    /// under the map of record. The artifact places the three arrays
    /// inside the boot copy window at its load/run offset with every
    /// entry naming its function; the transcript shows the markers in
    /// priority order before the kernel's constructors line, the
    /// destructor silent, and the rtsmoke stream byte-identical.
    fn run_ctor(&mut self) {
        const CASE: &str = "ctor";
        let gjson = golden_json("goldens/payload/rtsmoke-mps2.golden.txt", SHA_RTSMOKE_G, "bytes");
        let obj_rel = self.assemble_module_asm("ctor", "ctor", &ctor_input_asm());
        let (sha, path_rel) = self.link_image(CASE, "rtsmoke", &[obj_rel.as_str()]);
        let dump = std::fs::read(self.ctx.out.join(format!("{CASE}.nm"))).unwrap_or_default();
        let elf = std::fs::read(self.ctx.repo.join(&path_rel)).unwrap_or_default();
        let spans = match check_ctor_image(&dump, &elf, &CTOR_EXPECT) {
            Ok(s) => s,
            Err(e) => self.fail_case(CASE, "null", "0", &sha, &path_rel, None, &e),
        };
        for (x, (lo, hi)) in CTOR_EXPECT.iter().zip(spans.iter()) {
            println!(
                "  {}: [{lo:#x}, {hi:#x}) {} entries, inside the copy window at its load/run offset",
                x.section,
                x.entries.len()
            );
        }
        let run = self.run_payload_qemu(CASE, &sha, &path_rel, ExecProfile::Parity);
        let code_s = self.check_rtsmoke_run(CASE, &run);
        if let Err(e) = check_ctor_order(
            &run.raw,
            &[CTOR_MARK_PRE, CTOR_MARK_100, CTOR_MARK_200],
            CTOR_MODULES_LINE,
            CTOR_MARK_FINI,
        ) {
            self.fail_case(CASE, &code_s, "0", &sha, &path_rel, None, &e);
        }
        let stats = format!(
            "{{\"preinit_bytes\":{},\"init_bytes\":{},\"fini_bytes\":{}}}",
            spans[0].1 - spans[0].0,
            spans[1].1 - spans[1].0,
            spans[2].1 - spans[2].0
        );
        self.emit_case(CASE, "pass", &code_s, "0", &sha, &path_rel, &gjson, None, Some(&stats));
        println!("== case {CASE}: PASS (exit 0, three arrays inside the copy window, constructors ran in priority order before the module list, destructor silent, payload stream byte-identical) ==");
    }

    /// The strong-entry case: a payload that provides `rhu_payload_entry`
    /// and states its generation displaces the kernel's default -- the
    /// artifact resolves the entry away from the trampoline, the kernel
    /// names the payload's entry, the entry's marker prints, its return
    /// code becomes the exit, and neither the default nor managed Main
    /// runs.
    fn run_entry_strong(&mut self) {
        const CASE: &str = "entry-strong";
        let abi = self.abi;
        let want = ENTRY_RETURN.to_string();
        let obj_rel = self.assemble_module_asm("ctor", "entry-strong", &strong_entry_asm(abi));
        let (sha, path_rel) = self.link_image(CASE, "rtsmoke", &[obj_rel.as_str()]);
        let dump = std::fs::read(self.ctx.out.join(format!("{CASE}.nm"))).unwrap_or_default();
        let (Some(entry), Some(weak)) = (
            word_sym_value(&dump, "rhu_payload_entry"),
            word_sym_value(&dump, "__rhu_payload_entry_weak"),
        ) else {
            self.fail_case(CASE, "null", &want, &sha, &path_rel, None, "entry or trampoline symbol not value-parseable from the linked artifact");
        };
        if entry == weak {
            self.fail_case(CASE, "null", &want, &sha, &path_rel, None, "the strong entry did not displace the weak trampoline (same address)");
        }
        println!("  entry {entry:#x} resolved away from the trampoline {weak:#x}");
        let run = self.run_payload_qemu(CASE, &sha, &path_rel, ExecProfile::Parity);
        let code_s = run.exit.to_string();
        if !contains_fixed(&run.raw, "flavor: payload") {
            self.fail_case(CASE, &code_s, &want, &sha, &path_rel, None, "kernel flavor line missing");
        }
        for m in ["KERNEL PANIC", "EXCEPTION:"] {
            if contains_fixed(&run.raw, m) {
                self.fail_case(CASE, &code_s, &want, &sha, &path_rel, None, &format!("forbidden pattern '{m}' present"));
            }
        }
        if cr_count(&run.raw) == 0 {
            self.fail_case(CASE, &code_s, &want, &sha, &path_rel, None, "no CR bytes in raw capture");
        }
        if let Err(e) = check_entry_strong(&run.raw, abi) {
            self.fail_case(CASE, &code_s, &want, &sha, &path_rel, None, &e);
        }
        if run.exit != ENTRY_RETURN {
            self.fail_case(CASE, &code_s, &want, &sha, &path_rel, None, &format!("exit {}, wanted {ENTRY_RETURN} (the strong entry's return)", run.exit));
        }
        self.emit_case(CASE, "pass", &code_s, &want, &sha, &path_rel, "null", None, None);
        println!("== case {CASE}: PASS (exit {ENTRY_RETURN}: the payload's entry displaced the kernel default, ran alone, and its return became the exit) ==");
    }

    /// Negative control (link): the map of record with `.init_array`'s
    /// load region moved off CODE by one substitution -- the array's
    /// load/run offset is no longer .data's, the shape the map's ASSERT
    /// refuses.
    fn run_ctor_shape_control(&mut self) {
        const CASE: &str = "ctor-shape-control";
        const BROKE: &str = "the .init_array section loading from DATA instead of CODE (its load/run offset off .data's)";
        let map = std::fs::read_to_string(self.ctx.repo.join("src/kernel/payload-mps2-an500.x")).unwrap_or_default();
        let anchor = "    __init_array_end = .;\n    . = ALIGN(8);\n  } > DATA AT> CODE\n";
        if map.matches(anchor).count() != 1 {
            self.ctx.gate_fail(&format!("{CASE}: cannot arm the control -- the map's .init_array tail is not unique"));
        }
        let doctored = map.replacen(anchor, "    __init_array_end = .;\n    . = ALIGN(8);\n  } > DATA AT> DATA\n", 1);
        let map_rel = "src/kernel/target/payload-gate/ctor/shape-control.x";
        write_fixture(&self.ctx.repo.join(map_rel), doctored.as_bytes());
        let obj_rel = self.assemble_module_asm("ctor", "ctor", &ctor_input_asm());
        let (ok, ld_log, _) =
            self.link_raw(CASE, "goldens/payload/rtsmoke.o", &[obj_rel.as_str()], AN500_LIB, map_rel, &[]);
        let text = String::from_utf8_lossy(&std::fs::read(&ld_log).unwrap_or_default()).into_owned();
        let refusal = text
            .lines()
            .find(|l| l.contains(".init_array outside the boot copy's single load/run offset"))
            .map(|l| l.trim_end_matches('\r').to_string());
        if ok || refusal.is_none() {
            echo_file(&ld_log, "  | ");
            self.emit_control_case(CASE, "fail", BROKE);
            self.ctx.gate_fail(&format!("{CASE}: the doctored map was NOT refused by the .init_array offset ASSERT"));
        }
        println!("  refused as predicted: {}", refusal.unwrap_or_default());
        self.emit_control_case(CASE, "pass", BROKE);
        println!("== control {CASE}: PASS (the linker refused .init_array off .data's load/run offset) ==");
    }

    /// Negative control (run): the map of record with the three array
    /// sections moved past the TLS template, so they follow `__edata`
    /// at .data's load/run offset -- the ASSERTs hold, the arrays lie
    /// outside the boot copy window, and the kernel's placement check
    /// must refuse before any entry is read.
    fn run_ctor_window_control(&mut self) {
        const CASE: &str = "ctor-window-control";
        const BROKE: &str = "the constructor arrays placed after the boot copy window (the map's offset asserts hold, the placement check must refuse)";
        let map = std::fs::read_to_string(self.ctx.repo.join("src/kernel/payload-mps2-an500.x")).unwrap_or_default();
        let block_start = "  /* The constructor and destructor arrays, inside the boot copy window:";
        let block_end = "  /* Module 0's TLS template, in its own output section:";
        let tdata_tail = "    __edata = .;\n  } > DATA AT> CODE\n";
        let (Some(s), Some(e)) = (map.find(block_start), map.find(block_end)) else {
            self.ctx.gate_fail(&format!("{CASE}: cannot arm the control -- the map's array block anchors are missing"));
        };
        if s >= e || map.matches(block_start).count() != 1 || map.matches(tdata_tail).count() != 1 {
            self.ctx.gate_fail(&format!("{CASE}: cannot arm the control -- the map's array block or TLS tail is not unique"));
        }
        let block = map[s..e].to_string();
        let without = format!("{}{}", &map[..s], &map[e..]);
        let doctored = without.replacen(tdata_tail, &format!("{tdata_tail}\n{block}"), 1);
        let map_rel = "src/kernel/target/payload-gate/ctor/window-control.x";
        write_fixture(&self.ctx.repo.join(map_rel), doctored.as_bytes());
        let obj_rel = self.assemble_module_asm("ctor", "ctor", &ctor_input_asm());
        let (ok, ld_log, path_rel) =
            self.link_raw(CASE, "goldens/payload/rtsmoke.o", &[obj_rel.as_str()], AN500_LIB, map_rel, &[]);
        if !ok {
            echo_file(&ld_log, "  | ");
            self.emit_control_case(CASE, "fail", BROKE);
            self.ctx.gate_fail(&format!("{CASE}: the moved arrays did not link (the offset asserts were expected to hold)"));
        }
        let sha = sha256_file(&self.ctx.repo.join(&path_rel)).unwrap_or_default();
        let run = self.run_payload_qemu(CASE, &sha, &path_rel, ExecProfile::Parity);
        let handshake = default_entry_line(self.abi);
        let verdict = check_window_refusal(&run.raw).and_then(|_| {
            if contains_fixed(&run.raw, &handshake) {
                Ok(())
            } else {
                Err("the handshake line did not print before the refusal".to_string())
            }
        });
        if let Err(e) = verdict {
            self.emit_control_run(CASE, "fail", run.exit, 1, &sha, &path_rel, BROKE);
            self.ctx.gate_fail(&format!("{CASE}: {e}"));
        }
        if run.exit != 1 {
            self.emit_control_run(CASE, "fail", run.exit, 1, &sha, &path_rel, BROKE);
            self.ctx.gate_fail(&format!("{CASE}: exit {}, wanted 1 (the funnel's)", run.exit));
        }
        self.emit_control_run(CASE, "pass", run.exit, 1, &sha, &path_rel, BROKE);
        println!("== control {CASE}: PASS (the kernel refused the misplaced arrays before any entry was read: exit 1) ==");
    }

    /// Negative controls (run): the strong input built for the NEXT
    /// contract generation, and the strong input stating none -- the
    /// kernel refuses each with its sentence and exit 4 before the entry
    /// runs. Both inputs come from the strong input by one exact edit.
    fn run_abi_controls(&mut self) {
        let abi = self.abi;
        let strong = format!("{}\n", strong_entry_asm(abi).join("\n"));
        let word = format!("rhu_payload_abi: .word {abi}");
        let block = strong_abi_block(abi);
        if strong.matches(&word).count() != 1 || strong.matches(&block).count() != 1 {
            self.ctx.gate_fail("abi controls: cannot arm -- the strong input's generation statement is not unique");
        }
        let mismatch = strong.replacen(&word, &format!("rhu_payload_abi: .word {}", abi + 1), 1);
        let unstated = strong.replacen(&block, "", 1);
        let cases: [(&str, String, String, &str); 2] = [
            (
                "abi-mismatch-control",
                mismatch,
                mismatch_sentence(abi + 1, abi),
                "a payload entry built for the next hosting-API contract generation",
            ),
            (
                "abi-unstated-control",
                unstated,
                UNSTATED_SENTENCE.to_string(),
                "a payload entry stating no hosting-API contract generation",
            ),
        ];
        for (case, text, sentence, broke) in cases {
            let lines: Vec<String> = text.lines().map(String::from).collect();
            let obj_rel = self.assemble_module_asm("ctor", case, &lines);
            let (sha, path_rel) = self.link_image(case, "rtsmoke", &[obj_rel.as_str()]);
            let run = self.run_payload_qemu(case, &sha, &path_rel, ExecProfile::Parity);
            if let Err(e) = check_refusal(&run.raw, &sentence) {
                self.emit_control_run(case, "fail", run.exit, 4, &sha, &path_rel, broke);
                self.ctx.gate_fail(&format!("{case}: {e}"));
            }
            if run.exit != 4 {
                self.emit_control_run(case, "fail", run.exit, 4, &sha, &path_rel, broke);
                self.ctx.gate_fail(&format!("{case}: exit {}, wanted 4 (the refusal's)", run.exit));
            }
            self.emit_control_run(case, "pass", run.exit, 4, &sha, &path_rel, broke);
            println!("== control {case}: PASS (refused with its sentence, exit 4, the entry never ran) ==");
        }
    }

    /// Run a linked payload under QEMU on the given execution profile
    /// (Deterministic inserts the fixed icount argv before -kernel;
    /// the gctest case is the one that opts in).
    fn run_payload_qemu(
        &mut self,
        name: &str,
        sha: &str,
        path_rel: &str,
        profile: ExecProfile,
    ) -> PayloadRun {
        println!("== run {name} ==");
        let elf = self.ctx.repo.join(path_rel);
        let raw_path = self.ctx.out.join(format!("{name}.raw"));
        let stderr_path = self.ctx.out.join(format!("{name}.stderr"));
        let elf_arg = elf.to_string_lossy().into_owned();
        let mut args: Vec<&str> = vec![
            "-M",
            "mps2-an500",
            "-display",
            "none",
            "-monitor",
            "none",
            "-serial",
            "stdio",
            "-semihosting",
        ];
        args.extend_from_slice(profile.qemu_args());
        args.push("-kernel");
        args.push(&elf_arg);
        let outcome = match run_captured(
            &self.ctx.qemu,
            &args,
            &raw_path,
            &stderr_path,
            self.ctx.timeout,
        ) {
            Ok(o) => o,
            Err(e) => self.ctx.gate_fail(&format!("{name}: {e}")),
        };
        if outcome.timed_out {
            println!("  (run killed at its timeout; exit recorded as 124 -- a hang is observed, never a pass)");
        }
        let raw = std::fs::read(&raw_path).unwrap_or_default();
        echo_file(&raw_path, "  | ");
        PayloadRun {
            sha: sha.to_string(),
            path_rel: path_rel.to_string(),
            raw,
            exit: outcome.exit,
        }
    }

    fn run_rtsmoke(&mut self) {
        let gjson = golden_json("goldens/payload/rtsmoke-mps2.golden.txt", SHA_RTSMOKE_G, "bytes");
        let (sha, path_rel) = self.link_payload("rtsmoke");
        let run = self.run_payload_qemu("rtsmoke", &sha, &path_rel, ExecProfile::Parity);
        let code_s = self.check_rtsmoke_run("rtsmoke", &run);
        self.emit_case("rtsmoke", "pass", &code_s, "0", &run.sha, &run.path_rel, &gjson, None, None);
        println!("== case rtsmoke: PASS (exit 0, payload stream byte-identical) ==");
    }

    /// gctest runs on the DETERMINISTIC profile: the one timing-variant
    /// case, its counts pinnable exact only on this clock. Every post-run
    /// emission carries the timing field.
    fn run_gctest(&mut self) {
        let gjson = golden_json("goldens/payload/gctest-mps2.golden.txt", SHA_GCTEST_G, "pins");
        let (sha, path_rel) = self.link_payload("gctest");
        let run = self.run_payload_qemu("gctest", &sha, &path_rel, ExecProfile::Deterministic);
        let (sha, path_rel, code) = (run.sha.clone(), run.path_rel.clone(), run.exit);
        let code_s = code.to_string();
        let golden = std::fs::read(self.gold_dir.join("gctest-mps2.golden.txt")).unwrap_or_default();
        let stream = payload_stream(&run.raw);
        let glf = lf_normalize(&golden);
        let _ = std::fs::write(self.ctx.out.join("gctest.stream"), &stream);
        let _ = std::fs::write(self.ctx.out.join("gctest.golden.lf"), &glf);
        if !contains_fixed(&run.raw, "flavor: payload") {
            self.fail_case("gctest", &code_s, "0", &sha, &path_rel, Some(TIMING_DET), "kernel flavor line missing");
        }
        if !contains_fixed(&run.raw, "[payload] managed Main returned 0") {
            self.fail_case("gctest", &code_s, "0", &sha, &path_rel, Some(TIMING_DET), "managed Main return marker missing");
        }
        if !contains_fixed(&run.raw, &default_entry_line(self.abi)) {
            self.fail_case("gctest", &code_s, "0", &sha, &path_rel, Some(TIMING_DET), "the kernel-default handshake line is missing");
        }
        for m in [
            "PASS list-survives",
            "PASS boxes-survive",
            "PASS explicit-collect",
            "PASS post-churn-alloc",
            "PASS did-collect",
        ] {
            if !contains_fixed(&stream, m) {
                self.fail_case("gctest", &code_s, "0", &sha, &path_rel, Some(TIMING_DET), &format!("pin '{m}' not found in payload stream"));
            }
        }
        if !final_verdict_has(&stream, "ALL GOOD") {
            self.fail_case("gctest", &code_s, "0", &sha, &path_rel, Some(TIMING_DET), "verdict 'ALL GOOD' absent from the final summary line");
        }
        for m in ["KERNEL PANIC", "EXCEPTION:"] {
            if contains_fixed(&run.raw, m) {
                self.fail_case("gctest", &code_s, "0", &sha, &path_rel, Some(TIMING_DET), &format!("forbidden pattern '{m}' present"));
            }
        }
        if cr_count(&run.raw) == 0 {
            self.fail_case("gctest", &code_s, "0", &sha, &path_rel, Some(TIMING_DET), "no CR bytes in raw capture");
        }
        if code != 0 {
            self.fail_case("gctest", &code_s, "0", &sha, &path_rel, Some(TIMING_DET), &format!("exit {code}, wanted 0"));
        }
        let want_free = final_free(&glf);
        let got_free = final_free(&stream);
        let free_ok = matches!((&got_free, &want_free), (Some(a), Some(b)) if a == b);
        if !free_ok {
            let got = got_free.clone().unwrap_or_default();
            let want = want_free.clone().unwrap_or_default();
            self.fail_case("gctest", &code_s, "0", &sha, &path_rel, Some(TIMING_DET), &format!("final free={got} != golden's {want}"));
        }
        let got_gcs = final_gcs(&stream);
        if got_gcs.as_deref() != Some(COLLECTIONS_EXACT) {
            let got = got_gcs.clone().unwrap_or_default();
            self.fail_case("gctest", &code_s, "0", &sha, &path_rel, Some(TIMING_DET), &format!("collections={got} != pinned {COLLECTIONS_EXACT}"));
        }
        let free_v = got_free.unwrap_or_default();
        let gcs_v = got_gcs.unwrap_or_default();
        let stats = format!("{{\"collections\":{gcs_v},\"final_free\":{free_v}}}");
        self.emit_case("gctest", "pass", &code_s, "0", &sha, &path_rel, &gjson, Some(TIMING_DET), Some(&stats));
        println!("== case gctest: PASS (exit 0, 5 pins + ALL GOOD, final free={free_v}, collections={gcs_v}) ==");
    }

    /// The staticlib an rt-link case links: the gate's shared an500 one,
    /// or the board's own under the gate's target dir.
    fn rt_staticlib_rel(b: &RtBoard) -> String {
        if b.board == "mps2-an500" {
            AN500_LIB.to_string()
        } else {
            format!("src/kernel/target/payload-gate/rt/{}/{}/release/librhu_kernel.a", b.board, b.target)
        }
    }

    /// One board's payload staticlib into a target dir of its own (the
    /// shared per-target path stays the an500 flavor other gates link).
    fn build_rt_staticlib(&mut self, b: &RtBoard) {
        println!(
            "== build kernel staticlib {} (--lib --target {} --no-default-features --features {}) ==",
            b.board, b.target, b.features
        );
        let case = format!("rt-link-{}", b.board);
        let dir = self.ctx.repo.join("src/kernel/target/payload-gate/rt").join(b.board);
        let log = self.ctx.out.join(format!("rt-{}-staticlib.log", b.board));
        let ok = match cargo_build_kernel_at(
            &self.ctx.repo,
            &dir,
            &["--lib", "--target", b.target, "--no-default-features", "--features", b.features],
            &log,
        ) {
            Ok(ok) => ok,
            Err(e) => {
                println!("rhu-gate: {e}");
                false
            }
        };
        if !ok {
            echo_file(&log, "");
            self.fail_case(&case, "null", "null", "", "", None, "staticlib build error");
        }
        if !self.ctx.repo.join(Self::rt_staticlib_rel(b)).is_file() {
            self.fail_case(&case, "null", "null", "", "", None, "librhu_kernel.a not produced for the board");
        }
    }

    /// One linked rt image read off the artifact: the nm dump, the ELF
    /// bytes, the identity, and the linker log split into the unresolved
    /// names and every other diagnostic.
    fn rt_artifact(&mut self, case: &str, ld_log: &Path, path_rel: &str) -> (Vec<u8>, Vec<u8>, String, Vec<String>, Vec<String>) {
        let text = String::from_utf8_lossy(&std::fs::read(ld_log).unwrap_or_default()).into_owned();
        let (unresolved, other) = split_ld_log(&text);
        let elf = self.ctx.repo.join(path_rel);
        let sha = match sha256_file(&elf) {
            Ok(s) => s,
            Err(e) => self.ctx.gate_fail(&format!("{case}: cannot hash linked artifact: {e}")),
        };
        let nm = match llvm_tool(&self.facts, "llvm-nm") {
            Ok(p) => p,
            Err(e) => self.ctx.gate_fail(&format!("{case}: {e}")),
        };
        let elf_arg = elf.to_string_lossy().into_owned();
        let sym = self.ctx.out.join(format!("{case}.nm"));
        let scratch = self.ctx.out.join(format!("{case}.tool.stderr"));
        let nm_run = run_captured(&nm, &[&elf_arg], &sym, &scratch, Duration::from_secs(60));
        if !matches!(nm_run, Ok(ref o) if o.exit == 0) {
            self.fail_case(case, "null", "null", &sha, path_rel, None, "llvm-nm failed on the linked ELF");
        }
        let dump = std::fs::read(&sym).unwrap_or_default();
        let bytes = std::fs::read(&elf).unwrap_or_default();
        (dump, bytes, sha, unresolved, other)
    }

    /// The rt-link family: the five other boards' staticlibs, the frozen
    /// merged runtime object under each payload map, then the orphan
    /// control on the an500 map.
    fn run_rt_links(&mut self) {
        let _ = std::fs::create_dir_all(self.ctx.repo.join("src/kernel/target/payload-gate/rt"));
        for b in RT_BOARDS.iter().filter(|b| b.board != "mps2-an500") {
            self.build_rt_staticlib(b);
        }
        for b in RT_BOARDS.iter() {
            self.run_rt_link(b);
        }
        self.run_rt_link_control();
    }

    /// One board: the frozen merged runtime object linked under the
    /// board's payload map, link-only, rooted as the host's initialization
    /// contract roots it; the placement read off the artifact.
    fn run_rt_link(&mut self, b: &RtBoard) {
        let case = format!("rt-link-{}", b.board);
        let map_rel = format!("src/kernel/payload-{}.x", b.board);
        let lib_rel = Self::rt_staticlib_rel(b);
        let (ok, ld_log, path_rel) = self.link_raw(&case, RT_MERGED_O, &[], &lib_rel, &map_rel, &RT_LD_WORDS);
        if !ok {
            let text = String::from_utf8_lossy(&std::fs::read(&ld_log).unwrap_or_default()).into_owned();
            let (_, other) = split_ld_log(&text);
            for l in other.iter().take(12) {
                println!("  | {l}");
            }
            self.emit_case(&case, "fail", "null", "null", "", &path_rel, "null", None, None);
            self.ctx.gate_fail(&format!("{case}: link failed (see {case}-ld.log)"));
        }
        let (dump, elf_bytes, sha, unresolved, other) = self.rt_artifact(&case, &ld_log, &path_rel);
        if !other.is_empty() {
            for l in other.iter().take(10) {
                println!("  | {l}");
            }
            self.fail_case(&case, "null", "null", &sha, &path_rel, None, "linker diagnostics beyond the unresolved warnings");
        }
        for s in [
            "__managed__Main",
            "__rhu_debug_layout",
            "__rhu_sched",
            "__rhu_ticks",
            "rhu_payload_entry",
            "__rhu_payload_entry_weak",
            "rhu_payload_abi",
        ] {
            if !word_sym_present(&dump, s) {
                self.fail_case(&case, "null", "null", &sha, &path_rel, None, &format!("symbol '{s}' absent from the linked artifact"));
            }
        }
        if let Err(e) = check_records(&dump, &elf_bytes, self.abi) {
            self.fail_case(&case, "null", "null", &sha, &path_rel, None, &format!("image records: {e}"));
        }
        let Some(phdrs) = elf32_phdrs(&elf_bytes) else {
            self.fail_case(&case, "null", "null", &sha, &path_rel, None, "program headers not parseable");
        };
        let map_text = std::fs::read_to_string(self.ctx.repo.join(&map_rel)).unwrap_or_default();
        let Some(code) = map_region(&map_text, "CODE") else {
            self.ctx.gate_fail(&format!("{case}: {map_rel} states no CODE region"));
        };
        let facts = match check_rt_placement(&dump, &phdrs, &unresolved, &self.rt_demand, code, &RT_HELLO_BRACKETS) {
            Ok(f) => f,
            Err(e) => self.fail_case(&case, "null", "null", &sha, &path_rel, None, &format!("placement: {e}")),
        };
        println!(
            "  {case}: managed {} unbox {} hydrated {} tdata {} init {} modules {} exidx {}; unresolved {}, all in the frozen demand list; relro {} B inside the copy window; code free {} B; arena window {} B over the {} B floor",
            facts.managed, facts.unbox, facts.hydrated, facts.tdata, facts.init_array, facts.modules, facts.exidx,
            facts.unresolved, facts.relro, facts.code_free, facts.arena_window, facts.arena_floor
        );
        let stats = format!(
            "{{\"managed\":{},\"unbox\":{},\"hydrated\":{},\"tdata\":{},\"init_array\":{},\"modules\":{},\"exidx\":{},\"unresolved\":{},\"relro\":{},\"code_free\":{},\"arena_window\":{},\"arena_floor\":{}}}",
            facts.managed, facts.unbox, facts.hydrated, facts.tdata, facts.init_array, facts.modules, facts.exidx,
            facts.unresolved, facts.relro, facts.code_free, facts.arena_window, facts.arena_floor
        );
        self.emit_case(&case, "pass", "null", "null", &sha, &path_rel, "null", None, Some(&stats));
        println!(
            "== case {case}: PASS (the frozen runtime linked under payload-{}.x, placement asserted off the artifact) ==",
            b.board
        );
    }

    /// Negative control: the an500 map with its managed-code KEEP line
    /// gone. The linker places the input as an orphan section of its own,
    /// the bracket pair reads empty beside the managed code, and the
    /// placement checker must refuse.
    fn run_rt_link_control(&mut self) {
        const CASE: &str = "rt-link-control";
        const BROKE: &str = "the managed-code KEEP line removed from the an500 map (the section lands as an orphan; the bracket pair reads empty)";
        let map = std::fs::read_to_string(self.ctx.repo.join(AN500_MAP)).unwrap_or_default();
        let anchor = "    KEEP(*(__managedcode*));\n";
        if map.matches(anchor).count() != 1 {
            self.ctx.gate_fail(&format!(
                "{CASE}: cannot arm the control -- the map's managed-code KEEP line is not unique"
            ));
        }
        let doctored = map.replacen(anchor, "", 1);
        let map_rel = "src/kernel/target/payload-gate/rt/control.x";
        write_fixture(&self.ctx.repo.join(map_rel), doctored.as_bytes());
        let (ok, ld_log, path_rel) = self.link_raw(CASE, RT_MERGED_O, &[], AN500_LIB, map_rel, &RT_LD_WORDS);
        if !ok {
            echo_file(&ld_log, "  | ");
            self.emit_control_case(CASE, "fail", BROKE);
            self.ctx.gate_fail(&format!(
                "{CASE}: the doctored map did not link (the linker was expected to place the orphan)"
            ));
        }
        let (dump, elf_bytes, _sha, unresolved, _other) = self.rt_artifact(CASE, &ld_log, &path_rel);
        let phdrs = elf32_phdrs(&elf_bytes).unwrap_or_default();
        let map_text = std::fs::read_to_string(self.ctx.repo.join(AN500_MAP)).unwrap_or_default();
        let code = map_region(&map_text, "CODE").unwrap_or((0, 0));
        match check_rt_placement(&dump, &phdrs, &unresolved, &self.rt_demand, code, &RT_HELLO_BRACKETS) {
            Ok(_) => {
                self.emit_control_case(CASE, "fail", BROKE);
                self.ctx.gate_fail(&format!(
                    "{CASE}: the orphaned managed code was NOT refused by the bracket checker"
                ));
            }
            Err(e) => {
                println!("  refused as predicted: {e}");
                self.emit_control_case(CASE, "pass", BROKE);
                println!("== control {CASE}: PASS (the bracket checker refused the orphaned managed code) ==");
            }
        }
    }
}

/// Run the payload gate.
pub fn run(repo: &Path) -> ! {
    let (facts, qemu, qemu_v) = setup(repo);
    let ld = match resolve_payload_ld(repo, &facts) {
        Ok(l) => l,
        Err(e) => {
            println!("rhu-gate: {e}");
            exit(1);
        }
    };
    let gold_dir = repo.join("goldens").join("payload");
    let out = repo.join("src").join("kernel").join("target").join("payload-gate");
    if std::fs::create_dir_all(&out).is_err() {
        println!("rhu-gate: cannot create {}", out.display());
        exit(1);
    }
    let results = match Results::create(&out.join("payload.results.jsonl")) {
        Ok(r) => r,
        Err(e) => {
            println!("rhu-gate: cannot create results file: {e}");
            exit(1);
        }
    };
    let rustc_v = facts.version.clone();
    let ld_v = ld.version.clone();
    let header = std::fs::read_to_string(repo.join("src/kernel/include/rhu.h")).unwrap_or_default();
    let abi = match header_abi(&header) {
        Some(n) => n,
        None => {
            println!("rhu-gate: src/kernel/include/rhu.h states no RHU_ABI (the contract generation the entry handshake pins)");
            exit(1);
        }
    };
    let mut g = MPayload {
        ctx: GateCtx {
            repo: repo.to_path_buf(),
            gate: GATE.to_string(),
            display: DISPLAY.to_string(),
            out,
            out_rel: "src/kernel/target/payload-gate".to_string(),
            results,
            qemu,
            timeout: Duration::from_secs(TIMEOUT_S),
        },
        facts,
        ld,
        gold_dir,
        abi,
        rt_demand: Vec::new(),
    };
    g.ctx.results.emit(&format!(
        "{{\"schema\":1,\"type\":\"run\",\"gate\":\"payload\",\"run\":\"payload-{}\",\"host\":\"{HOST}\",\"runner\":\"{RUNNER}\",\"chain\":{},\"tools\":{{\"qemu\":\"{}\",\"rustc\":\"{}\",\"ld\":\"{}\"}}}}",
        std::process::id(),
        chain_json_from_env(),
        qemu_v,
        rustc_v,
        ld_v
    ));
    g.verify_frozen_inputs();
    g.build_staticlib();
    g.run_rtsmoke();
    g.run_gctest();
    g.run_tls_probes();
    g.run_tls_shape_control();
    g.run_ctor();
    g.run_entry_strong();
    g.run_ctor_shape_control();
    g.run_ctor_window_control();
    g.run_abi_controls();
    g.run_rt_links();
    g.ctx.results.emit_summary(GATE, "pass", 0);
    if !validate_results(repo, g.ctx.results.path()) {
        println!("{DISPLAY}: FAIL -- results file failed schema validation");
        exit(1);
    }
    println!("{DISPLAY}: PASS (18/18 cases incl. 6 negative controls)  [{qemu_v} | {rustc_v} | {ld_v}]");
    exit(0);
}

/// Negative-control the checkers this gate adds (sha armor, the payload
/// stream filter + byte compare, the gctest pins, the managed-entry nm
/// check, the module-0 template contract, the constructor-array
/// contract and transcript order, the strong-entry and refusal
/// transcripts, the header's generation), plus one accept-direction
/// probe per reject-direction family.
pub fn selfcheck(repo: &Path) -> ! {
    let (facts, _qemu, qemu_v) = setup(repo);
    let ld = match resolve_payload_ld(repo, &facts) {
        Ok(l) => l,
        Err(e) => {
            println!("rhu-gate: {e}");
            exit(1);
        }
    };
    let rustc_v = facts.version.clone();
    let gold_dir = repo.join("goldens").join("payload");
    if !gold_dir.join("rtsmoke-mps2.golden.txt").is_file() {
        println!("rhu-gate payload selfcheck: goldens missing");
        exit(1);
    }
    let out = repo.join("src").join("kernel").join("target").join("payload-gate");
    let tmp = out.join("selfcheck");
    let _ = std::fs::create_dir_all(&tmp);
    let results = match Results::create(&out.join("payload-selfcheck.results.jsonl")) {
        Ok(r) => r,
        Err(e) => {
            println!("rhu-gate: cannot create results file: {e}");
            exit(1);
        }
    };
    let mut sc = Selfcheck {
        repo: repo.to_path_buf(),
        gate: "payload-selfcheck".to_string(),
        display: "PAYLOAD SELFCHECK".to_string(),
        results,
        n: 0,
        npass: 0,
        nneg: 0,
    };
    sc.results.emit(&format!(
        "{{\"schema\":1,\"type\":\"run\",\"gate\":\"payload-selfcheck\",\"run\":\"payload-selfcheck-{}\",\"host\":\"{HOST}\",\"runner\":\"{RUNNER}\",\"tools\":{{\"qemu\":\"{}\",\"rustc\":\"{}\",\"ld\":\"{}\"}}}}",
        std::process::id(),
        qemu_v,
        rustc_v,
        ld.version
    ));

    let mut doctored = std::fs::read(gold_dir.join("rtsmoke.o")).unwrap_or_default();
    doctored.push(b'x');
    let doctored_path = tmp.join("doctored.o");
    write_fixture(&doctored_path, &doctored);
    sc.probe(
        "sha-doctored",
        "frozen object with one appended byte",
        sha256_file(&doctored_path).map(|h| h == SHA_RTSMOKE_O).unwrap_or(false),
    );
    sc.accept_probe(
        "sha-genuine",
        "recorded hash accepted for the untouched frozen object",
        sha256_file(&gold_dir.join("rtsmoke.o")).map(|h| h == SHA_RTSMOKE_O).unwrap_or(false),
    );

    let golden = std::fs::read(gold_dir.join("rtsmoke-mps2.golden.txt")).unwrap_or_default();
    let glf = lf_normalize(&golden);
    let mut genuine: Vec<u8> = Vec::new();
    genuine.extend_from_slice(b"rhu 0.2.0 -- kernel banner\r\n");
    genuine.extend_from_slice(b"[boot] kernel line\r\n");
    genuine.extend_from_slice(&crlf_of(&glf));
    genuine.extend_from_slice(b"[payload] managed Main returned 0\r\n");
    let genuine_path = tmp.join("genuine.raw");
    write_fixture(&genuine_path, &genuine);
    sc.accept_probe(
        "stream-genuine",
        "golden payload stream inside kernel noise accepted by the byte compare",
        payload_stream(&genuine) == glf,
    );
    let flipped = replace_all(&genuine, b"17 pass", b"18 pass");
    if flipped == genuine {
        println!("PAYLOAD SELFCHECK: FAIL -- flip probe edit was a no-op");
        exit(1);
    }
    write_fixture(&tmp.join("flipped.raw"), &flipped);
    sc.probe(
        "stream-flipped",
        "payload stream with one deviated byte",
        payload_stream(&flipped) == glf,
    );
    let unfiltered = b"PASS div32\n".to_vec();
    write_fixture(&tmp.join("unfiltered.raw"), &unfiltered);
    sc.probe(
        "stream-truncated",
        "payload stream missing all but one line",
        payload_stream(&unfiltered) == glf,
    );

    let goodfinal = format!("gctest: collections={COLLECTIONS_EXACT} free=89016 x ALL GOOD\n").into_bytes();
    let badfree = format!("gctest: collections={COLLECTIONS_EXACT} free=89017 x ALL GOOD\n").into_bytes();
    let badgcs = b"gctest: collections=9 free=89016 x ALL GOOD\n".to_vec();
    let nofinal = b"no final line here\n".to_vec();
    let golden_gc = std::fs::read(gold_dir.join("gctest-mps2.golden.txt")).unwrap_or_default();
    let gc_glf = lf_normalize(&golden_gc);
    write_fixture(&tmp.join("goodfinal.stream"), &goodfinal);
    write_fixture(&tmp.join("badfree.stream"), &badfree);
    write_fixture(&tmp.join("badgcs.stream"), &badgcs);
    write_fixture(&tmp.join("nofinal.stream"), &nofinal);
    write_fixture(&tmp.join("golden.lf"), &gc_glf);
    sc.probe(
        "final-free-drift",
        "final free one byte off the golden's value",
        free_matches(&badfree, &gc_glf),
    );
    sc.probe(
        "final-line-missing",
        "stream with no final summary line",
        free_matches(&nofinal, &gc_glf),
    );
    sc.probe(
        "collections-drift",
        "collections count off the derived pin",
        final_gcs(&badgcs).as_deref() == Some(COLLECTIONS_EXACT),
    );
    sc.accept_probe(
        "pins-genuine",
        "golden final state accepted by the free pin",
        free_matches(&goodfinal, &gc_glf),
    );
    sc.accept_probe(
        "pins-genuine-gcs",
        "golden collections count accepted by the exact pin",
        final_gcs(&goodfinal).as_deref() == Some(COLLECTIONS_EXACT),
    );
    let planted = format!("note: ALL GOOD is what the verdict would say\ngctest: collections={COLLECTIONS_EXACT} free=89016  not yet\n").into_bytes();
    write_fixture(&tmp.join("planted.stream"), &planted);
    sc.probe(
        "verdict-planted",
        "verdict words on a non-verdict line, the summary line without them",
        final_verdict_has(&planted, "ALL GOOD"),
    );
    sc.accept_probe(
        "verdict-genuine",
        "the golden's own final line carries the verdict after its fields",
        final_verdict_has(&gc_glf, "ALL GOOD"),
    );

    let noentry = b"00001568 T InitializeModules\n".to_vec();
    write_fixture(&tmp.join("noentry.nm"), &noentry);
    sc.probe(
        "entry-sym-gone",
        "linked-artifact symbol dump without __managed__Main",
        word_sym_present(&noentry, "__managed__Main"),
    );

    let nofrozen = b"00012000 R __rhu_frozen_lo\n0000da48 T __exidx_start\n0000db00 T __exidx_end\n".to_vec();
    write_fixture(&tmp.join("nofrozen.nm"), &nofrozen);
    sc.probe(
        "bracket-sym-gone",
        "linked-artifact symbol dump without __rhu_frozen_hi (value parse must refuse)",
        matches!(
            (word_sym_value(&nofrozen, "__rhu_frozen_lo"), word_sym_value(&nofrozen, "__rhu_frozen_hi")),
            (Some(a), Some(b)) if a <= b
        ),
    );
    let emptyexidx = b"00012000 R __rhu_frozen_lo\n00012000 R __rhu_frozen_hi\n0000da48 T __exidx_start\n0000da48 T __exidx_end\n".to_vec();
    write_fixture(&tmp.join("emptyexidx.nm"), &emptyexidx);
    sc.probe(
        "exidx-empty",
        "payload-family dump with an empty exidx bracket (managed input missed the bracket)",
        matches!(
            (word_sym_value(&emptyexidx, "__exidx_start"), word_sym_value(&emptyexidx, "__exidx_end")),
            (Some(a), Some(b)) if a < b
        ),
    );
    let goodbrackets = b"00012000 R __rhu_frozen_lo\n00012000 R __rhu_frozen_hi\n0000da48 T __exidx_start\n0000db00 T __exidx_end\n".to_vec();
    write_fixture(&tmp.join("goodbrackets.nm"), &goodbrackets);
    sc.accept_probe(
        "brackets-well-formed",
        "genuine payload-family bracket dump (frozen equal-ordered, exidx nonzero) accepted",
        matches!(
            (word_sym_value(&goodbrackets, "__rhu_frozen_lo"), word_sym_value(&goodbrackets, "__rhu_frozen_hi")),
            (Some(a), Some(b)) if a <= b
        ) && matches!(
            (word_sym_value(&goodbrackets, "__exidx_start"), word_sym_value(&goodbrackets, "__exidx_end")),
            (Some(a), Some(b)) if a < b
        ),
    );
    let invtdata = b"20000010 D __rhu_tdata_lo\n20000000 D __rhu_tdata_hi\n".to_vec();
    write_fixture(&tmp.join("invtdata.nm"), &invtdata);
    sc.probe(
        "tdata-inverted",
        "payload-family dump with an inverted tls-template bracket (lo above hi)",
        matches!(
            (word_sym_value(&invtdata, "__rhu_tdata_lo"), word_sym_value(&invtdata, "__rhu_tdata_hi")),
            (Some(a), Some(b)) if a <= b
        ),
    );

    let probe = &TLS_PROBES[2];
    let good_dump = b"20000000 D __sdata\n20001010 D __edata\n00009000 A __sidata\n20001008 D __rhu_tdata_lo\n20001010 D __rhu_tdata_hi\n00000000 D w1\n00000004 D w2\n".to_vec();
    let load = (PT_LOAD, 0x1000, 0x2000_0000, 0x9000, 0x1010, 0x1010, 8);
    let tls = (PT_TLS, 0x2008, 0x2000_1008, 0xa008, 8, 8, 8);
    let good_elf = synth_elf32(&[load, tls], 0x2008, probe.template);
    write_fixture(&tmp.join("tls-good.nm"), &good_dump);
    write_fixture(&tmp.join("tls-good.elf"), &good_elf);
    sc.accept_probe(
        "tls-genuine",
        "genuine template image (PT_TLS at the bracket, zero-fill filed, bytes and offsets exact) accepted",
        check_tls_image(&good_dump, &good_elf, probe).is_ok(),
    );
    let no_tls = synth_elf32(&[load], 0x2008, probe.template);
    write_fixture(&tmp.join("tls-no-phdr.elf"), &no_tls);
    sc.probe(
        "tls-phdr-missing",
        "template image without a PT_TLS program header",
        check_tls_image(&good_dump, &no_tls, probe).is_ok(),
    );
    let tls_off = synth_elf32(&[load, (PT_TLS, 0x200c, 0x2000_100c, 0xa00c, 8, 8, 8)], 0x2008, probe.template);
    write_fixture(&tmp.join("tls-vaddr-off.elf"), &tls_off);
    sc.probe(
        "tls-phdr-vaddr-off",
        "PT_TLS starting four bytes past the bracket",
        check_tls_image(&good_dump, &tls_off, probe).is_ok(),
    );
    let escaped = replace_all(&good_dump, b"20001010 D __edata", b"20001008 D __edata");
    write_fixture(&tmp.join("tls-window-escape.nm"), &escaped);
    sc.probe(
        "tls-window-escape",
        "template past the boot copy window (__edata at the bracket's low end)",
        check_tls_image(&escaped, &good_elf, probe).is_ok(),
    );
    let mut drift = good_elf.clone();
    drift[0x2008] ^= 0x01;
    write_fixture(&tmp.join("tls-bytes-drift.elf"), &drift);
    sc.probe(
        "tls-bytes-drift",
        "load image with one template byte off the initializer",
        check_tls_image(&good_dump, &drift, probe).is_ok(),
    );
    let off = replace_all(&good_dump, b"00000004 D w2", b"00000008 D w2");
    write_fixture(&tmp.join("tls-offset-drift.nm"), &off);
    sc.probe(
        "tls-offset-drift",
        "thread-local symbol resolved four bytes off its offset",
        check_tls_image(&off, &good_elf, probe).is_ok(),
    );

    let header = std::fs::read_to_string(repo.join("src/kernel/include/rhu.h")).unwrap_or_default();
    sc.accept_probe(
        "abi-header-genuine",
        "include/rhu.h states an RHU_ABI the gate can read",
        header_abi(&header).is_some(),
    );
    let abi = header_abi(&header).unwrap_or(0);
    sc.probe(
        "abi-header-missing",
        "a header text without the RHU_ABI define (the gate must refuse to run without it)",
        header_abi("#define RHU_ABI_VERSION 9\n#define RHU_EAGAIN (-11)\n").is_some(),
    );

    let ctor_dump = b"20000000 D __sdata\n200010f0 D __edata\n00009b00 A __sidata\n200010d8 D __preinit_array_start\n200010dc D __preinit_array_end\n200010e0 D __init_array_start\n200010e8 D __init_array_end\n200010e8 D __fini_array_start\n200010ec D __fini_array_end\n000017e2 T ctor_pre\n000017f6 T ctor_100\n0000180a T ctor_200\n0000181e T dtor_fini\n".to_vec();
    let words = |w: [u32; 5]| -> Vec<u8> { w.iter().flat_map(|v| v.to_le_bytes()).collect() };
    let ctor_words = words([0x17e3, 0, 0x17f7, 0x180b, 0x181f]);
    let copy_load = (PT_LOAD, 0x1000, 0x2000_0000, 0x9b00, 0x10f0, 0x10f0, 8);
    let ctor_elf = synth_elf32(&[copy_load], 0x1000 + 0x10d8, &ctor_words);
    write_fixture(&tmp.join("ctor-good.nm"), &ctor_dump);
    write_fixture(&tmp.join("ctor-good.elf"), &ctor_elf);
    sc.accept_probe(
        "ctor-image-genuine",
        "genuine constructor artifact (three arrays inside the window at its offset, entries naming their functions with the Thumb bit) accepted",
        check_ctor_image(&ctor_dump, &ctor_elf, &CTOR_EXPECT).is_ok(),
    );
    let missing = replace_all(&ctor_dump, b"200010e8 D __init_array_end\n", b"");
    sc.probe(
        "ctor-bracket-missing",
        "constructor artifact dump without __init_array_end (the parse must refuse)",
        check_ctor_image(&missing, &ctor_elf, &CTOR_EXPECT).is_ok(),
    );
    let inverted = replace_all(
        &replace_all(&ctor_dump, b"200010e0 D __init_array_start", b"200010e8 D __init_array_start"),
        b"200010e8 D __init_array_end",
        b"200010e0 D __init_array_end",
    );
    sc.probe(
        "ctor-bracket-inverted",
        "constructor artifact with the .init_array brackets inverted",
        check_ctor_image(&inverted, &ctor_elf, &CTOR_EXPECT).is_ok(),
    );
    let short = replace_all(&ctor_dump, b"200010e8 D __init_array_end", b"200010e4 D __init_array_end");
    sc.probe(
        "ctor-span-wrong",
        "constructor artifact whose .init_array spans one entry where two were linked",
        check_ctor_image(&short, &ctor_elf, &CTOR_EXPECT).is_ok(),
    );
    let escaped = replace_all(&ctor_dump, b"200010f0 D __edata", b"200010e0 D __edata");
    sc.probe(
        "ctor-window-escape",
        "constructor artifact with __edata below the arrays (arrays outside the boot copy window)",
        check_ctor_image(&escaped, &ctor_elf, &CTOR_EXPECT).is_ok(),
    );
    let orphan_elf = synth_elf32(&[(PT_LOAD, 0x1000, 0x2000_0000, 0x2000_0000, 0x10f0, 0x10f0, 8)], 0x1000 + 0x10d8, &ctor_words);
    write_fixture(&tmp.join("ctor-orphan.elf"), &orphan_elf);
    sc.probe(
        "ctor-segment-offset",
        "constructor artifact whose carrying segment loads at its run address (load == run, the orphan shape)",
        check_ctor_image(&ctor_dump, &orphan_elf, &CTOR_EXPECT).is_ok(),
    );
    let swapped_elf = synth_elf32(&[copy_load], 0x1000 + 0x10d8, &words([0x17e3, 0, 0x180b, 0x17f7, 0x181f]));
    sc.probe(
        "ctor-entry-names-wrong",
        "constructor artifact whose .init_array words name the functions in the wrong order",
        check_ctor_image(&ctor_dump, &swapped_elf, &CTOR_EXPECT).is_ok(),
    );
    let even_elf = synth_elf32(&[copy_load], 0x1000 + 0x10d8, &words([0x17e2, 0, 0x17f7, 0x180b, 0x181f]));
    write_fixture(&tmp.join("ctor-even.elf"), &even_elf);
    sc.probe(
        "ctor-thumb-bit-clear",
        "constructor artifact whose pre-init entry word lacks the Thumb bit",
        check_ctor_image(&ctor_dump, &even_elf, &CTOR_EXPECT).is_ok(),
    );

    let markers = [CTOR_MARK_PRE, CTOR_MARK_100, CTOR_MARK_200];
    let order_ok = format!(
        "{CTOR_MARK_PRE}\r\n{CTOR_MARK_100}\r\n{CTOR_MARK_200}\r\n{CTOR_MODULES_LINE}\r\n=== rtsmoke: minirt v2 (alloc/div/casts) ===\r\n"
    );
    write_fixture(&tmp.join("ctor-order.raw"), order_ok.as_bytes());
    sc.accept_probe(
        "ctor-order-genuine",
        "genuine constructor transcript (pre-init, 100, 200, then the constructors line; no destructor) accepted",
        check_ctor_order(order_ok.as_bytes(), &markers, CTOR_MODULES_LINE, CTOR_MARK_FINI).is_ok(),
    );
    let no200 = order_ok.replace(&format!("{CTOR_MARK_200}\r\n"), "");
    sc.probe(
        "ctor-marker-missing",
        "constructor transcript missing the priority-200 marker",
        check_ctor_order(no200.as_bytes(), &markers, CTOR_MODULES_LINE, CTOR_MARK_FINI).is_ok(),
    );
    let swapped = format!("{CTOR_MARK_PRE}\r\n{CTOR_MARK_200}\r\n{CTOR_MARK_100}\r\n{CTOR_MODULES_LINE}\r\n");
    sc.probe(
        "ctor-order-swapped",
        "constructor transcript with 200 before 100 (the priority sort ignored)",
        check_ctor_order(swapped.as_bytes(), &markers, CTOR_MODULES_LINE, CTOR_MARK_FINI).is_ok(),
    );
    let late = format!("{CTOR_MARK_PRE}\r\n{CTOR_MARK_100}\r\n{CTOR_MODULES_LINE}\r\n{CTOR_MARK_200}\r\n");
    sc.probe(
        "ctor-marker-after-modules",
        "constructor transcript with a marker after the constructors line (a constructor ran after the module list)",
        check_ctor_order(late.as_bytes(), &markers, CTOR_MODULES_LINE, CTOR_MARK_FINI).is_ok(),
    );
    let fini_ran = format!("{order_ok}{CTOR_MARK_FINI}\r\n");
    sc.probe(
        "ctor-fini-ran",
        "constructor transcript where the destructor's marker printed",
        check_ctor_order(fini_ran.as_bytes(), &markers, CTOR_MODULES_LINE, CTOR_MARK_FINI).is_ok(),
    );

    let strong_ok = format!(
        "[boot] x | y | flavor: payload\r\n{}\r\n{ENTRY_MARK}\r\n[payload] managed Main returned {ENTRY_RETURN}\r\n",
        provided_entry_line(abi)
    );
    write_fixture(&tmp.join("entry-strong.raw"), strong_ok.as_bytes());
    sc.accept_probe(
        "entry-strong-genuine",
        "genuine strong-entry transcript accepted",
        check_entry_strong(strong_ok.as_bytes(), abi).is_ok(),
    );
    let default_ran = format!("{strong_ok}{CTOR_MODULES_LINE}\r\n");
    sc.probe(
        "entry-default-ran",
        "strong-entry transcript where the kernel default's constructors line printed too",
        check_entry_strong(default_ran.as_bytes(), abi).is_ok(),
    );
    let main_ran = format!("{strong_ok}PASS div32\r\n");
    sc.probe(
        "entry-stream-not-empty",
        "strong-entry transcript carrying payload output (managed Main ran)",
        check_entry_strong(main_ran.as_bytes(), abi).is_ok(),
    );
    let wrong_code = strong_ok.replace(&format!("returned {ENTRY_RETURN}"), "returned 0");
    sc.probe(
        "entry-returned-wrong",
        "strong-entry transcript reporting a return other than the entry's",
        check_entry_strong(wrong_code.as_bytes(), abi).is_ok(),
    );

    let sentence = mismatch_sentence(abi + 1, abi);
    let refused_ok = format!("[boot] x | flavor: payload\r\n{sentence}\r\n");
    write_fixture(&tmp.join("abi-refused.raw"), refused_ok.as_bytes());
    sc.accept_probe(
        "refusal-genuine",
        "genuine refusal transcript (the sentence, nothing after) accepted",
        check_refusal(refused_ok.as_bytes(), &sentence).is_ok(),
    );
    let no_sentence = format!("[boot] x | flavor: payload\r\n{}\r\n", default_entry_line(abi));
    sc.probe(
        "refusal-sentence-missing",
        "transcript without the refusal sentence (the kernel accepted the mismatch)",
        check_refusal(no_sentence.as_bytes(), &sentence).is_ok(),
    );
    let main_after = format!("{refused_ok}[payload] managed Main returned 0\r\n");
    sc.probe(
        "refusal-main-ran",
        "refusal transcript where managed Main reported anyway",
        check_refusal(main_after.as_bytes(), &sentence).is_ok(),
    );

    let window_ok = format!(
        "{}\r\n[panic] *** KERNEL PANIC ***\r\n[panic] .preinit_array outside the boot copy window\r\n[panic] *** halted ***\r\n",
        default_entry_line(abi)
    );
    write_fixture(&tmp.join("ctor-window.raw"), window_ok.as_bytes());
    sc.accept_probe(
        "window-refusal-genuine",
        "genuine placement-refusal transcript accepted",
        check_window_refusal(window_ok.as_bytes()).is_ok(),
    );
    let no_msg = window_ok.replace(".preinit_array outside the boot copy window", "some other panic");
    sc.probe(
        "window-message-missing",
        "panic transcript without the placement refusal",
        check_window_refusal(no_msg.as_bytes()).is_ok(),
    );

    let rt_dump = |m: (u64, u64), u: (u64, u64), h: (u64, u64), t: (u64, u64), i: (u64, u64), arena: (u64, u64)| -> Vec<u8> {
        let syms: [(&str, u64); 25] = [
            ("__stext", 0x440),
            ("__etext", u.1 + 2),
            ("__rhu_managedcode_lo", m.0),
            ("__rhu_managedcode_hi", m.1),
            ("__rhu_unbox_lo", u.0),
            ("__rhu_unbox_hi", u.1),
            ("__start___modules", u.1 + 2),
            ("__stop___modules", u.1 + 6),
            ("__rhu_frozen_lo", 0xc70c0),
            ("__rhu_frozen_hi", 0xc70c0),
            ("__exidx_start", 0xd5024),
            ("__exidx_end", 0xd95d4),
            ("__sdata", 0x2000_0000),
            ("__edata", 0x2000_2408),
            ("__init_array_start", i.0),
            ("__init_array_end", i.1),
            ("__rhu_tdata_lo", t.0),
            ("__rhu_tdata_hi", t.1),
            ("__sbss", 0x2000_2408),
            ("__ebss", h.1),
            ("__rhu_hydrated_lo", h.0),
            ("__rhu_hydrated_hi", h.1),
            ("__rhu_arena_lo", arena.0),
            ("__rhu_arena_hi", arena.1),
            ("RHU_ARENA_MIN", 0x20_0000),
        ];
        let mut out = String::new();
        for (n, v) in syms {
            out.push_str(&format!("{v:08x} T {n}\n"));
        }
        out.into_bytes()
    };
    let ph = |relro_vaddr: u64| -> Vec<Phdr> {
        vec![
            Phdr { p_type: PT_LOAD, p_offset: 0, p_vaddr: 0, p_paddr: 0, p_filesz: 0x440, p_memsz: 0x440, p_align: 0x10000 },
            Phdr { p_type: PT_LOAD, p_offset: 0, p_vaddr: 0x2000_0000, p_paddr: 0xd9460, p_filesz: 0x2408, p_memsz: 0x2408, p_align: 0x10000 },
            Phdr { p_type: PT_TLS, p_offset: 0, p_vaddr: 0x2000_2368, p_paddr: 0xdb7c8, p_filesz: RT_TDATA_BYTES, p_memsz: RT_TDATA_BYTES, p_align: 8 },
            Phdr { p_type: PT_GNU_RELRO, p_offset: 0, p_vaddr: relro_vaddr, p_paddr: 0xdb638, p_filesz: 0x230, p_memsz: 0x230, p_align: 1 },
        ]
    };
    let g_m = (0x45c9cu64, 0x45c9c + RT_MANAGED_BYTES);
    let g_u = (g_m.1 + 2, g_m.1 + 2 + RT_UNBOX_BYTES);
    let g_h = (0x2000_96b8u64, 0x2000_96b8 + RT_HYDRATED_BYTES);
    let g_t = (0x2000_2368u64, 0x2000_2368 + RT_TDATA_BYTES);
    let g_i = (0x2000_2350u64, 0x2000_2368u64);
    let g_a = (0x2002_c408u64, 0x203f_0000u64);
    let demand: Vec<String> = ["SystemNative_Abort", "malloc", "pthread_self"].iter().map(|s| s.to_string()).collect();
    let unresolved: Vec<String> = vec!["malloc".to_string()];
    let code = (0u64, 3u64 << 20);
    let check = |d: &[u8], p: &[Phdr], u: &[String]| check_rt_placement(d, p, u, &demand, code, &RT_HELLO_BRACKETS).is_ok();
    sc.accept_probe(
        "rt-placement-genuine",
        "the frozen runtime's placement as the an500 link states it accepted",
        check(&rt_dump(g_m, g_u, g_h, g_t, g_i, g_a), &ph(0x2000_21d8), &unresolved),
    );
    sc.probe(
        "rt-managed-empty",
        "managed-code bracket empty (the input orphaned)",
        check(&rt_dump((0x45c9c, 0x45c9c), g_u, g_h, g_t, g_i, g_a), &ph(0x2000_21d8), &unresolved),
    );
    sc.probe(
        "rt-unbox-off",
        "unboxing-stub bracket one byte off the object's section",
        check(&rt_dump(g_m, (g_u.0, g_u.0 + RT_UNBOX_BYTES + 1), g_h, g_t, g_i, g_a), &ph(0x2000_21d8), &unresolved),
    );
    sc.probe(
        "rt-hydrated-size",
        "rehydrated-data bracket four bytes wide of the object's region",
        check(&rt_dump(g_m, g_u, (0x2000_96b8, 0x2000_96b8 + RT_HYDRATED_BYTES + 4), g_t, g_i, g_a), &ph(0x2000_21d8), &unresolved),
    );
    sc.probe(
        "rt-tdata-size",
        "TLS template bracket short of the object's thread-locals",
        check(&rt_dump(g_m, g_u, g_h, (0x2000_2368, 0x2000_2368 + RT_TDATA_BYTES - 4), g_i, g_a), &ph(0x2000_21d8), &unresolved),
    );
    sc.probe(
        "rt-init-array",
        ".init_array bracket missing a constructor",
        check(&rt_dump(g_m, g_u, g_h, g_t, (0x2000_2350, 0x2000_2364), g_a), &ph(0x2000_21d8), &unresolved),
    );
    sc.probe(
        "rt-relro-outside",
        "read-only-after-relocation segment outside the copy window",
        check(&rt_dump(g_m, g_u, g_h, g_t, g_i, g_a), &ph(0xc70c0), &unresolved),
    );
    sc.probe(
        "rt-unresolved-foreign",
        "an unresolved name outside the frozen demand list",
        check(&rt_dump(g_m, g_u, g_h, g_t, g_i, g_a), &ph(0x2000_21d8), &["SystemNative_Zzz".to_string()]),
    );
    sc.probe(
        "rt-arena-under-floor",
        "arena window under the board's floor",
        check(&rt_dump(g_m, g_u, g_h, g_t, g_i, (0x2002_c408, 0x2002_c408 + 0x10_0000)), &ph(0x2000_21d8), &unresolved),
    );

    sc.finish("genuine inputs accepted");
}

/// True when both streams carry a parsable final free= value and the
/// values are equal.
fn free_matches(stream: &[u8], golden_lf: &[u8]) -> bool {
    matches!(
        (final_free(stream), final_free(golden_lf)),
        (Some(a), Some(b)) if a == b
    )
}

/// LF bytes rendered CRLF, with a bare trailing CR when the input does
/// not end in a newline (the fixture builder's `sed 's/$/\r/'` shape).
fn crlf_of(lf: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(lf.len() + lf.len() / 16);
    for &b in lf {
        if b == b'\n' {
            out.push(b'\r');
        }
        out.push(b);
    }
    if !lf.is_empty() && !lf.ends_with(b"\n") {
        out.push(b'\r');
    }
    out
}

fn replace_all(hay: &[u8], from: &[u8], to: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(hay.len());
    let mut i = 0;
    while i < hay.len() {
        if i + from.len() <= hay.len() && &hay[i..i + from.len()] == from {
            out.extend_from_slice(to);
            i += from.len();
        } else {
            out.push(hay[i]);
            i += 1;
        }
    }
    out
}

fn print_first_divergence(golden_lf: &[u8], stream: &[u8]) {
    let a_lines: Vec<&[u8]> = golden_lf.split(|&c| c == b'\n').collect();
    let b_lines: Vec<&[u8]> = stream.split(|&c| c == b'\n').collect();
    for i in 0..a_lines.len().max(b_lines.len()) {
        let ga = a_lines.get(i).copied().unwrap_or(b"<absent>");
        let gb = b_lines.get(i).copied().unwrap_or(b"<absent>");
        if ga != gb {
            println!("golden {}: {}", i + 1, String::from_utf8_lossy(ga));
            println!("stream {}: {}", i + 1, String::from_utf8_lossy(gb));
            break;
        }
    }
}
