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

    /// Link one frozen payload object against the staticlib under the
    /// payload board map; assert the artifact's symbol contract by nm.
    fn link_payload(&mut self, name: &str) -> (String, String) {
        println!("== link {name} (RHU_PAYLOAD_LD) ==");
        let path_rel = format!("src/kernel/target/payload-gate/{name}-rhu.elf");
        let elf = self.ctx.repo.join(&path_rel);
        let ld_log = self.ctx.out.join(format!("{name}-ld.log"));
        let ld_stdout = self.ctx.out.join(format!("{name}-ld.stdout"));
        let map_rel = format!("src/kernel/target/payload-gate/{name}-rhu.map");
        let obj_rel = format!("goldens/payload/{name}.o");
        let status = {
            let log_f = std::fs::File::create(&ld_log);
            let out_f = std::fs::File::create(&ld_stdout);
            match (log_f, out_f) {
                (Ok(log_f), Ok(out_f)) => {
                    let words = self.ld.words.clone();
                    let mut cmd = ld_command(&self.ctx.repo, &words);
                    cmd.args([
                        "-T",
                        "src/kernel/payload-mps2-an500.x",
                        "--gc-sections",
                        &format!("-Map={map_rel}"),
                        &obj_rel,
                        "src/kernel/target/thumbv7em-none-eabihf/release/librhu_kernel.a",
                        "-o",
                        &path_rel,
                    ])
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
        let (sha, path_rel, code) = (run.sha.clone(), run.path_rel.clone(), run.exit);
        let code_s = code.to_string();
        if !contains_fixed(&run.raw, "flavor: payload") {
            self.fail_case("rtsmoke", &code_s, "0", &sha, &path_rel, None, "kernel flavor line missing");
        }
        if !contains_fixed(&run.raw, "[payload] managed Main returned 0") {
            self.fail_case("rtsmoke", &code_s, "0", &sha, &path_rel, None, "managed Main return marker missing");
        }
        for m in ["KERNEL PANIC", "EXCEPTION:"] {
            if contains_fixed(&run.raw, m) {
                self.fail_case("rtsmoke", &code_s, "0", &sha, &path_rel, None, &format!("forbidden pattern '{m}' present"));
            }
        }
        if cr_count(&run.raw) == 0 {
            self.fail_case("rtsmoke", &code_s, "0", &sha, &path_rel, None, "no CR bytes in raw capture");
        }
        if code != 0 {
            self.fail_case("rtsmoke", &code_s, "0", &sha, &path_rel, None, &format!("exit {code}, wanted 0"));
        }
        let golden = std::fs::read(self.gold_dir.join("rtsmoke-mps2.golden.txt")).unwrap_or_default();
        let stream = payload_stream(&run.raw);
        let glf = lf_normalize(&golden);
        if stream != glf {
            println!("--- payload stream vs golden (diff) ---");
            print_first_divergence(&glf, &stream);
            self.fail_case("rtsmoke", &code_s, "0", &sha, &path_rel, None, "payload stream differs from golden");
        }
        self.emit_case("rtsmoke", "pass", &code_s, "0", &sha, &path_rel, &gjson, None, None);
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
    g.ctx.results.emit_summary(GATE, "pass", 0);
    if !validate_results(repo, g.ctx.results.path()) {
        println!("{DISPLAY}: FAIL -- results file failed schema validation");
        exit(1);
    }
    println!("{DISPLAY}: PASS (3/3)  [{qemu_v} | {rustc_v} | {ld_v}]");
    exit(0);
}

/// Negative-control the checkers this gate adds (sha armor, the payload
/// stream filter + byte compare, the gctest pins, the managed-entry nm
/// check), plus one accept-direction probe per reject-direction family.
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
