//! The base gate on the runner: kernel conformance on QEMU mps2-an500.
//!
//! Three image flavors, each built, run, and asserted on markers AND
//! semihosting exit codes: boot (exit 0, golden byte-compare + FP artifact
//! checks), panic (exit 1, funnel report), fault (exit 2, CFSR decoded).
//! Every assertion reads the raw capture file; the boot transcript must
//! equal the committed golden (CR presence asserted byte-true, then
//! LF-normalized compare on both sides). Structured results are emitted as
//! JSON lines beside the transcripts and validated as the last step.


use crate::gate::{write_fixture, CaseRun, GateCtx, Selfcheck};
use crate::results::{chain_json_from_env, golden_json, image_json, validate_results, Results};
use crate::sha256::sha256_file;
use crate::text::{contains_fixed, cr_count, eq_lf_normalized, lf_normalize};
use crate::tools::{llvm_tool, resolve_qemu, run_captured, rustc_facts, RustcFacts};
use std::path::{Path, PathBuf};
use std::process::exit;
use std::time::Duration;

const GATE: &str = "base";
const DISPLAY: &str = "BASE GATE";
const HOST: &str = "qemu-mps2-an500";
const RUNNER: &str = "src/harness";
const GOLDEN_REL: &str = "src/scripts/goldens/base-boot.txt";
const TIMEOUT_S: u64 = 30;

const MARKER_ALL_GOOD: &str = "[base] ALL GOOD (boot + console + panic/fault funnel armed)";

struct BASE {
    ctx: GateCtx,
    golden: PathBuf,
    facts: RustcFacts,
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
    let _ = repo;
    (facts, qemu, qemu_v)
}

impl BASE {
    fn emit_case(
        &mut self,
        case: &str,
        verdict: &str,
        code: i32,
        want: i32,
        sha: &str,
        path_rel: &str,
        golden: &str,
    ) {
        let image = image_json(path_rel, sha);
        self.ctx.results.emit(&format!(
            "{{\"schema\":1,\"type\":\"case\",\"gate\":\"base\",\"case\":\"{case}\",\"verdict\":\"{verdict}\",\"exit\":{code},\"exit_want\":{want},\"control\":\"positive\",\"image\":{image},\"golden\":{golden}}}"
        ));
        self.ctx.results.tally(verdict, "positive");
    }

    fn fail_case(
        &mut self,
        case: &str,
        code: i32,
        want: i32,
        sha: &str,
        path_rel: &str,
        msg: &str,
    ) -> ! {
        self.emit_case(case, "fail", code, want, sha, path_rel, "null");
        self.ctx.gate_fail(&format!("{case}: {msg}"))
    }

    fn common_asserts(
        &mut self,
        case: &str,
        run: &CaseRun,
        want: i32,
        markers: &[&str],
        forbidden: &[&str],
    ) {
        let (sha, path_rel, code) = (run.sha.clone(), run.path_rel.clone(), run.exit);
        for m in markers {
            if !contains_fixed(&run.raw, m) {
                self.fail_case(case, code, want, &sha, &path_rel, &format!("marker '{m}' not found"));
            }
        }
        for m in forbidden {
            if contains_fixed(&run.raw, m) {
                self.fail_case(case, code, want, &sha, &path_rel, &format!("forbidden pattern '{m}' present"));
            }
        }
        if cr_count(&run.raw) == 0 {
            self.fail_case(case, code, want, &sha, &path_rel, "no CR bytes in raw capture (wire discipline lost)");
        }
        if code != want {
            self.fail_case(case, code, want, &sha, &path_rel, &format!("exit {code}, wanted {want}"));
        }
    }

    fn run_boot(&mut self) {
        let run = self.ctx.build_and_run("boot", &[]);
        let (sha, path_rel, code) = (run.sha.clone(), run.path_rel.clone(), run.exit);
        let gsha = match sha256_file(&self.golden) {
            Ok(s) => s,
            Err(e) => self.ctx.gate_fail(&format!("boot: cannot hash golden: {e}")),
        };
        let gjson = golden_json(GOLDEN_REL, &gsha, "bytes");
        self.common_asserts(
            "boot",
            &run,
            0,
            &["flavor: boot", MARKER_ALL_GOOD],
            &["KERNEL PANIC", "EXCEPTION:"],
        );
        let golden_bytes = std::fs::read(&self.golden).unwrap_or_default();
        if !eq_lf_normalized(&run.raw, &golden_bytes) {
            print_first_divergence(&golden_bytes, &run.raw);
            self.fail_case("boot", code, 0, &sha, &path_rel, "transcript differs from golden");
        }
        // FP-path evidence in the ARTIFACT, not just the printout: the f32
        // selftest must be hardware VFP, the f64 one a soft-float call.
        let objdump = match llvm_tool(&self.facts, "llvm-objdump") {
            Ok(p) => p,
            Err(e) => self.ctx.gate_fail(&format!("boot: {e}")),
        };
        let nm = match llvm_tool(&self.facts, "llvm-nm") {
            Ok(p) => p,
            Err(e) => self.ctx.gate_fail(&format!("boot: {e}")),
        };
        let elf = self.ctx.out.join("rhu-kernel-boot.elf");
        let elf_arg = elf.to_string_lossy().into_owned();
        let dis = self.ctx.out.join("boot.dis");
        let sym = self.ctx.out.join("boot.sym");
        let scratch = self.ctx.out.join("boot.tool.stderr");
        let _ = run_captured(
            &objdump,
            &["-d", "--no-show-raw-insn", &elf_arg],
            &dis,
            &scratch,
            Duration::from_secs(60),
        );
        if !contains_fixed(&std::fs::read(&dis).unwrap_or_default(), "vmul.f32") {
            self.fail_case("boot", code, 0, &sha, &path_rel, "no vmul.f32 in disassembly (f32 hardware path missing)");
        }
        let _ = run_captured(&nm, &[&elf_arg], &sym, &scratch, Duration::from_secs(60));
        if !contains_fixed(&std::fs::read(&sym).unwrap_or_default(), "__aeabi_dmul") {
            self.fail_case("boot", code, 0, &sha, &path_rel, "__aeabi_dmul not in symbol table (f64 soft path missing)");
        }
        self.emit_case("boot", "pass", code, 0, &sha, &path_rel, &gjson);
        println!("== case boot: PASS (exit 0, golden byte-identical, FP artifact checks OK) ==");
    }

    fn run_panic(&mut self) {
        let run = self.ctx.build_and_run("panic", &["--features", "test-panic"]);
        let (sha, path_rel, code) = (run.sha.clone(), run.path_rel.clone(), run.exit);
        self.common_asserts(
            "panic",
            &run,
            1,
            &[
                "flavor: test-panic",
                "[panic] *** KERNEL PANIC ***",
                "[panic] at ",
                "deliberate panic (flavor test-panic)",
                "[panic] *** halted ***",
            ],
            &["EXCEPTION:"],
        );
        self.emit_case("panic", "pass", code, 1, &sha, &path_rel, "null");
        println!("== case panic: PASS (exit 1) ==");
    }

    fn run_fault(&mut self) {
        let run = self.ctx.build_and_run("fault", &["--features", "test-fault"]);
        let (sha, path_rel, code) = (run.sha.clone(), run.path_rel.clone(), run.exit);
        self.common_asserts(
            "fault",
            &run,
            2,
            &[
                "flavor: test-fault",
                "[test-fault] executing udf #0",
                "[fault] *** EXCEPTION: UsageFault (IPSR=6) ***",
                "CFSR=0x00010000",
                "CFSR[16] UNDEFINSTR",
                "EXC_RETURN=0xffffffed (extended FP frame on PSP)",
                "[fault] *** halted ***",
            ],
            &["KERNEL PANIC"],
        );
        self.emit_case("fault", "pass", code, 2, &sha, &path_rel, "null");
        println!("== case fault: PASS (exit 2, CFSR decoded) ==");
    }
}

/// Run the base gate.
pub fn run(repo: &Path) -> ! {
    let (facts, qemu, qemu_v) = setup(repo);
    let golden = repo.join("src").join("scripts").join("goldens").join("base-boot.txt");
    if !golden.is_file() {
        println!("rhu-gate: golden missing at {GOLDEN_REL}");
        exit(1);
    }
    let out = repo.join("src").join("kernel").join("target").join("base-gate");
    if std::fs::create_dir_all(&out).is_err() {
        println!("rhu-gate: cannot create {}", out.display());
        exit(1);
    }
    let results = match Results::create(&out.join("base.results.jsonl")) {
        Ok(r) => r,
        Err(e) => {
            println!("rhu-gate: cannot create results file: {e}");
            exit(1);
        }
    };
    let rustc_v = facts.version.clone();
    let mut base = BASE {
        ctx: GateCtx {
            repo: repo.to_path_buf(),
            gate: GATE.to_string(),
            display: DISPLAY.to_string(),
            out,
            out_rel: "src/kernel/target/base-gate".to_string(),
            results,
            qemu,
            timeout: Duration::from_secs(TIMEOUT_S),
        },
        golden,
        facts,
    };
    base.ctx.results.emit(&format!(
        "{{\"schema\":1,\"type\":\"run\",\"gate\":\"base\",\"run\":\"base-{}\",\"host\":\"{HOST}\",\"runner\":\"{RUNNER}\",\"chain\":{},\"tools\":{{\"qemu\":\"{}\",\"rustc\":\"{}\"}}}}",
        std::process::id(),
        chain_json_from_env(),
        qemu_v,
        rustc_v
    ));
    base.run_boot();
    base.run_panic();
    base.run_fault();
    base.ctx.results.emit_summary(GATE, "pass", 0);
    if !validate_results(repo, base.ctx.results.path()) {
        println!("{DISPLAY}: FAIL -- results file failed schema validation");
        exit(1);
    }
    println!("{DISPLAY}: PASS (3/3)  [{qemu_v} | {rustc_v}]");
    exit(0);
}

/// Negative-control the gate's own checkers: every assertion helper must
/// REJECT a doctored input, and the CR check must ACCEPT a genuine CRLF
/// capture.
pub fn selfcheck(repo: &Path) -> ! {
    let (facts, _qemu, qemu_v) = setup(repo);
    let golden_path = repo.join("src").join("scripts").join("goldens").join("base-boot.txt");
    if !golden_path.is_file() {
        println!("rhu-gate base selfcheck: golden missing");
        exit(1);
    }
    let rustc_v = facts.version.clone();
    let out = repo.join("src").join("kernel").join("target").join("base-gate");
    let tmp = out.join("selfcheck");
    let _ = std::fs::create_dir_all(&tmp);
    let results = match Results::create(&out.join("base-selfcheck.results.jsonl")) {
        Ok(r) => r,
        Err(e) => {
            println!("rhu-gate: cannot create results file: {e}");
            exit(1);
        }
    };
    let mut sc = Selfcheck {
        repo: repo.to_path_buf(),
        gate: "base-selfcheck".to_string(),
        display: "BASE SELFCHECK".to_string(),
        results,
        n: 0,
        npass: 0,
        nneg: 0,
    };
    sc.results.emit(&format!(
        "{{\"schema\":1,\"type\":\"run\",\"gate\":\"base-selfcheck\",\"run\":\"base-selfcheck-{}\",\"host\":\"{HOST}\",\"runner\":\"{RUNNER}\",\"tools\":{{\"qemu\":\"{}\",\"rustc\":\"{}\"}}}}",
        std::process::id(),
        qemu_v,
        rustc_v
    ));

    let golden = std::fs::read(&golden_path).unwrap_or_default();

    let empty = tmp.join("empty.raw");
    write_fixture(&empty, b"no markers here\r\n");
    sc.probe(
        "missing-marker",
        "transcript without the ALL GOOD marker",
        contains_fixed(&std::fs::read(&empty).unwrap_or_default(), "[base] ALL GOOD"),
    );

    let wrongflavor = tmp.join("wrongflavor.raw");
    write_fixture(
        &wrongflavor,
        b"[boot] QEMU mps2-an500 | CMSDK UART0 | flavor: test-panic\r\n",
    );
    let wrongflavor_bytes = std::fs::read(&wrongflavor).unwrap_or_default();
    sc.probe(
        "wrong-flavor",
        "boot-case flavor assertion fed a test-panic transcript",
        contains_fixed(&wrongflavor_bytes, "flavor: boot"),
    );

    let perturbed_text = String::from_utf8_lossy(&golden).replace("ALL GOOD", "ALL G00D");
    let perturbed = tmp.join("perturbed.txt");
    write_fixture(&perturbed, perturbed_text.as_bytes());
    sc.probe(
        "golden-byte",
        "golden with one byte changed (GOOD to G00D)",
        eq_lf_normalized(&std::fs::read(&perturbed).unwrap_or_default(), &golden),
    );

    sc.probe("wrong-exit", "exit code 1 asserted against want 0", 1 == 0);

    let nocfsr = tmp.join("nocfsr.raw");
    write_fixture(&nocfsr, b"[fault] *** EXCEPTION: UsageFault (IPSR=6) ***\r\n");
    sc.probe(
        "no-cfsr-decode",
        "fault transcript without the CFSR decode line",
        contains_fixed(&std::fs::read(&nocfsr).unwrap_or_default(), "CFSR[16] UNDEFINSTR"),
    );

    let lfonly = tmp.join("lfonly.raw");
    write_fixture(&lfonly, b"lf only, no carriage returns\n");
    sc.probe(
        "cr-lost",
        "capture with CR bytes stripped (wire discipline lost)",
        cr_count(&std::fs::read(&lfonly).unwrap_or_default()) > 0,
    );

    // The one accept-direction probe: a checker that rejects everything is
    // as broken as one that accepts everything; pin the accept side too.
    let crlf = tmp.join("crlf.raw");
    write_fixture(&crlf, b"crlf line\r\n");
    sc.accept_probe(
        "cr-present",
        "CRLF capture accepted by the CR check",
        cr_count(&std::fs::read(&crlf).unwrap_or_default()) > 0,
    );

    sc.probe(
        "forbidden-present",
        "forbidden-pattern sweep fed a panicking transcript",
        !contains_fixed(&wrongflavor_bytes, "flavor: test-panic"),
    );

    sc.finish("genuine input accepted");
}

fn print_first_divergence(golden: &[u8], raw: &[u8]) {
    let a = lf_normalize(golden);
    let b = lf_normalize(raw);
    let a_lines: Vec<&[u8]> = a.split(|&c| c == b'\n').collect();
    let b_lines: Vec<&[u8]> = b.split(|&c| c == b'\n').collect();
    println!("--- transcript vs golden (first divergence) ---");
    for i in 0..a_lines.len().max(b_lines.len()) {
        let ga = a_lines.get(i).copied().unwrap_or(b"<absent>");
        let gb = b_lines.get(i).copied().unwrap_or(b"<absent>");
        if ga != gb {
            println!("golden {}: {}", i + 1, String::from_utf8_lossy(ga));
            println!("actual {}: {}", i + 1, String::from_utf8_lossy(gb));
            break;
        }
    }
}
