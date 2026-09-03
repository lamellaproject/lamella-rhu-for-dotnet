//! The payload gate on the runner: hosting the version-frozen minirt
//! payloads on QEMU mps2-an500.

use crate::gate::{write_fixture, GateCtx, Selfcheck};
use crate::results::{chain_json_from_env, golden_json, validate_results, Results};
use crate::sha256::sha256_file;
use crate::text::{
    contains_fixed, cr_count, final_free, final_gcs, lf_normalize, payload_stream,
    word_sym_present, word_sym_value,
};
use crate::tools::{
    echo_file, llvm_tool, resolve_qemu, run_captured, rustc_facts, status_bounded, ExecProfile,
    RustcFacts,
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
const COLLECTIONS_EXACT: &str = "61";

/// The deterministic profile's name in the results surface (additive,
/// report-only, between "control" and "image"). Emitted only on case
/// lines at or after a run under the profile: no run, no clock claim.
const TIMING_DET: &str = "icount-shift0";

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

/// The thread-local section's segment alignment every board map declares.
const TLS_ALIGN: u64 = 8;
const PT_LOAD: u32 = 1;
const PT_TLS: u32 = 7;

#[derive(Clone, Copy)]
struct Phdr {
    p_type: u32,
    p_offset: u64,
    p_vaddr: u64,
    p_paddr: u64,
    p_filesz: u64,
    p_memsz: u64,
    p_align: u64,
}

/// Program headers of an ELF32 little-endian image (the artifact class
/// every board map produces). None when the bytes are not that shape.
fn elf32_phdrs(elf: &[u8]) -> Option<Vec<Phdr>> {
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

/// A minimal ELF32 little-endian image for the selfcheck's template
/// probes: the given program headers (type, offset, vaddr, paddr,
/// filesz, memsz, align) and `data` placed at file offset `data_at`.
fn synth_elf32(phdrs: &[(u32, u32, u32, u32, u32, u32, u32)], data_at: usize, data: &[u8]) -> Vec<u8> {
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

/// The payload linker as a word-split command plus its reported version
/// (first line of `<command> --version`, CR-trimmed).
struct PayloadLd {
    words: Vec<String>,
    version: String,
}

fn resolve_payload_ld(repo: &Path, facts: &RustcFacts) -> Result<PayloadLd, String> {
    let words: Vec<String> = match std::env::var("RHU_PAYLOAD_LD") {
        Ok(v) if !v.trim().is_empty() => v.split_whitespace().map(str::to_string).collect(),
        _ => {
            let lld = llvm_tool(facts, "rust-lld").map_err(|e| {
                format!(
                    "no payload linker resolved: set RHU_PAYLOAD_LD (a command; word-split) -- the default rust-lld is unavailable: {e}"
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
            "payload linker not runnable via '{} --version' (set RHU_PAYLOAD_LD to a working linker command; a probe that hangs past 20 s is killed and counts as not runnable)",
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
}

/// One linked-and-run payload case: staged artifact identity + the run.
struct PayloadRun {
    sha: String,
    path_rel: String,
    raw: Vec<u8>,
    exit: i32,
}

fn setup(_repo: &Path) -> (RustcFacts, PathBuf, String) {
    let facts = match rustc_facts() {
        Ok(f) => f,
        Err(e) => {
            println!("rhu-gate: {e}");
            exit(1);
        }
    };
    let (qemu, qemu_v) = match resolve_qemu() {
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
        let stats = format!(
            "{{\"rtsmoke_o\":\"{SHA_RTSMOKE_O}\",\"gctest_o\":\"{SHA_GCTEST_O}\",\"rtsmoke_golden\":\"{SHA_RTSMOKE_G}\",\"gctest_golden\":\"{SHA_GCTEST_G}\"}}"
        );
        self.emit_case("frozen-inputs", "pass", "null", "null", "", "", "null", None, Some(&stats));
        println!("== frozen inputs: PASS (4/4 hashes match the MANIFEST) ==");
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
        let lib = self
            .ctx
            .repo
            .join("src/kernel/target/thumbv7em-none-eabihf/release/librhu_kernel.a");
        if !lib.is_file() {
            self.fail_case("staticlib-build", "null", "null", "", "", None, "librhu_kernel.a not produced");
        }
    }

    /// One bounded linker invocation under `map_rel`: the frozen payload
    /// object, any extra objects after it, then the staticlib. Returns
    /// (linked, linker log, artifact path); the caller decides what a
    /// failed link means (a case fails on it, a control expects it).
    fn link_raw(&mut self, name: &str, frozen: &str, extra: &[&str], map_rel: &str) -> (bool, PathBuf, String) {
        println!("== link {name} (RHU_PAYLOAD_LD) ==");
        let path_rel = format!("src/kernel/target/payload-gate/{name}-rhu.elf");
        let ld_log = self.ctx.out.join(format!("{name}-ld.log"));
        let ld_stdout = self.ctx.out.join(format!("{name}-ld.stdout"));
        let mapfile_rel = format!("src/kernel/target/payload-gate/{name}-rhu.map");
        let obj_rel = format!("goldens/payload/{frozen}.o");
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
                        obj_rel.clone(),
                    ];
                    args.extend(extra.iter().map(|s| s.to_string()));
                    args.extend([
                        "src/kernel/target/thumbv7em-none-eabihf/release/librhu_kernel.a".to_string(),
                        "-o".to_string(),
                        path_rel.clone(),
                    ]);
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
        let (ok, ld_log, path_rel) = self.link_raw(name, frozen, extra, "src/kernel/payload-mps2-an500.x");
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
        for s in ["__managed__Main", "__rhu_debug_layout", "__rhu_sched", "__rhu_ticks"] {
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

    /// Assemble one template probe with the pinned toolchain's llc: the
    /// module-asm source is written under the gate's target dir and the
    /// object lands beside it (repo-relative, for the link argv).
    fn assemble_tls_probe(&mut self, probe: &TlsProbe) -> String {
        let dir = self.ctx.out.join("tls");
        let _ = std::fs::create_dir_all(&dir);
        let mut ll = String::from("target triple = \"thumbv7em-none-eabihf\"\n");
        for line in probe.asm {
            ll.push_str(&format!("module asm \"{}\"\n", line.replace('"', "\\22")));
        }
        let ll_path = dir.join(format!("{}.ll", probe.case));
        write_fixture(&ll_path, ll.as_bytes());
        let obj_rel = format!("src/kernel/target/payload-gate/tls/{}.o", probe.case);
        let llc = match llvm_tool(&self.facts, "llc") {
            Ok(p) => p,
            Err(e) => self.ctx.gate_fail(&format!("{}: {e}", probe.case)),
        };
        let log = dir.join(format!("{}.llc.log", probe.case));
        let err = dir.join(format!("{}.llc.stderr", probe.case));
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
            self.fail_case(probe.case, "null", "0", "", "", None, "llc did not assemble the probe");
        }
        obj_rel
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
    /// hydrated input, so thread-local input matches .data first); the
    /// mixed probe must then be REFUSED with the section-flags error.
    fn run_tls_shape_control(&mut self) {
        const CASE: &str = "tls-shape-control";
        const BROKE: &str = "thread-local template brackets inside .data (the shape the linker refuses)";
        let map_of_record = self.ctx.repo.join("src/kernel/payload-mps2-an500.x");
        let map = std::fs::read_to_string(&map_of_record).unwrap_or_default();
        let hydrated = "    *(.hydrated*);\n";
        if map.matches(hydrated).count() != 1 {
            self.ctx.gate_fail(&format!(
                "{CASE}: cannot arm the control -- the map's hydrated-input line is not unique"
            ));
        }
        let doctored = map.replacen(
            hydrated,
            "    *(.hydrated*);\n    KEEP(*(.tdata .tdata.*));\n    KEEP(*(.tbss .tbss.*));\n",
            1,
        );
        let map_rel = "src/kernel/target/payload-gate/tls/shape-control.x";
        write_fixture(&self.ctx.repo.join(map_rel), doctored.as_bytes());
        let obj_rel = self.assemble_tls_probe(&TLS_PROBES[2]);
        let (ok, ld_log, _) = self.link_raw(CASE, "rtsmoke", &[obj_rel.as_str()], map_rel);
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
        for m in [
            "PASS list-survives",
            "PASS boxes-survive",
            "PASS explicit-collect",
            "PASS post-churn-alloc",
            "PASS did-collect",
            "ALL GOOD",
        ] {
            if !contains_fixed(&stream, m) {
                self.fail_case("gctest", &code_s, "0", &sha, &path_rel, Some(TIMING_DET), &format!("pin '{m}' not found in payload stream"));
            }
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
    g.ctx.results.emit_summary(GATE, "pass", 0);
    if !validate_results(repo, g.ctx.results.path()) {
        println!("{DISPLAY}: FAIL -- results file failed schema validation");
        exit(1);
    }
    println!("{DISPLAY}: PASS (7/7 cases incl. 1 negative control)  [{qemu_v} | {rustc_v} | {ld_v}]");
    exit(0);
}

/// Negative-control the checkers this gate adds (sha armor, the payload
/// stream filter + byte compare, the gctest pins, the managed-entry nm
/// check, the module-0 template contract), plus one accept-direction
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

    let goodfinal = b"gctest: collections=61 free=89016 x ALL GOOD\n".to_vec();
    let badfree = b"gctest: collections=61 free=89017 x ALL GOOD\n".to_vec();
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
