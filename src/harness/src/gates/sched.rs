//! The sched gate on the runner: scheduler conformance on QEMU mps2-an500.

use crate::gate::{write_fixture, CaseRun, GateCtx, Selfcheck};
use crate::results::{chain_json_from_env, golden_json, image_json, validate_results, Results};
use crate::sha256::sha256_file;
use crate::text::{contains_fixed, cr_count, eq_lf_normalized, lf_normalize, wake_order, word_sym_present};
use crate::tools::{llvm_tool, resolve_qemu, run_captured, rustc_facts, RustcFacts};
use std::path::{Path, PathBuf};
use std::process::exit;
use std::time::Duration;

const GATE: &str = "sched";
const DISPLAY: &str = "SCHED GATE";
const HOST: &str = "qemu-mps2-an500";
const RUNNER: &str = "src/harness";
const GOLDEN_REL: &str = "src/scripts/goldens/base-boot.txt";
const TIMEOUT_S: u64 = 60;

const MARKER_SCHED_ALL_GOOD: &str = "[sched] ALL GOOD (threads + timers + preemption online)";

/// The debug-layout contract symbols, asserted word-bounded in the
/// ARTIFACT: a contract asserted nowhere in the artifact is not a
/// contract (the linker silently eliminates an unreferenced record,
/// so presence is asserted here, in the artifact itself).
const CONTRACT_SYMS: [&str; 3] = ["__rhu_debug_layout", "__rhu_sched", "__rhu_ticks"];

struct SCHED {
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
    (facts, qemu, qemu_v)
}

impl SCHED {
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
            "{{\"schema\":1,\"type\":\"case\",\"gate\":\"sched\",\"case\":\"{case}\",\"verdict\":\"{verdict}\",\"exit\":{code},\"exit_want\":{want},\"control\":\"positive\",\"image\":{image},\"golden\":{golden}}}"
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

    /// Marker presence in boot order; each case then appends its own
    /// checks in a fixed sequence.
    fn assert_markers(&mut self, case: &str, run: &CaseRun, want: i32, markers: &[&str]) {
        let (sha, path_rel, code) = (run.sha.clone(), run.path_rel.clone(), run.exit);
        for m in markers {
            if !contains_fixed(&run.raw, m) {
                self.fail_case(case, code, want, &sha, &path_rel, &format!("marker '{m}' not found"));
            }
        }
    }

    fn assert_forbidden_cr_exit(
        &mut self,
        case: &str,
        run: &CaseRun,
        want: i32,
        forbidden: &[&str],
    ) {
        let (sha, path_rel, code) = (run.sha.clone(), run.path_rel.clone(), run.exit);
        for m in forbidden {
            if contains_fixed(&run.raw, m) {
                self.fail_case(case, code, want, &sha, &path_rel, &format!("forbidden pattern '{m}' present"));
            }
        }
        if cr_count(&run.raw) == 0 {
            self.fail_case(case, code, want, &sha, &path_rel, "no CR bytes in raw capture");
        }
        if code != want {
            self.fail_case(case, code, want, &sha, &path_rel, &format!("exit {code}, wanted {want}"));
        }
    }

    fn run_core(&mut self) {
        let run = self.ctx.build_and_run("core", &[]);
        let (sha, path_rel, code) = (run.sha.clone(), run.path_rel.clone(), run.exit);
        let gsha = match sha256_file(&self.golden) {
            Ok(s) => s,
            Err(e) => self.ctx.gate_fail(&format!("core: cannot hash golden: {e}")),
        };
        let gjson = golden_json(GOLDEN_REL, &gsha, "bytes");
        self.assert_markers(
            "core",
            &run,
            0,
            &[
                "flavor: boot",
                "[sched] online:",
                "[base] ALL GOOD (boot + console + panic/fault funnel armed)",
                MARKER_SCHED_ALL_GOOD,
            ],
        );
        self.assert_forbidden_cr_exit("core", &run, 0, &["KERNEL PANIC", "EXCEPTION:"]);
        let golden_bytes = std::fs::read(&self.golden).unwrap_or_default();
        if !eq_lf_normalized(&run.raw, &golden_bytes) {
            print_first_divergence(&golden_bytes, &run.raw);
            self.fail_case("core", code, 0, &sha, &path_rel, "transcript differs from golden");
        }
        let nm = match llvm_tool(&self.facts, "llvm-nm") {
            Ok(p) => p,
            Err(e) => self.ctx.gate_fail(&format!("core: {e}")),
        };
        let elf = self.ctx.out.join("rhu-kernel-core.elf");
        let elf_arg = elf.to_string_lossy().into_owned();
        let sym = self.ctx.out.join("core.nm");
        let scratch = self.ctx.out.join("core.tool.stderr");
        let nm_run = run_captured(&nm, &[&elf_arg], &sym, &scratch, Duration::from_secs(60));
        if !matches!(nm_run, Ok(ref o) if o.exit == 0) {
            self.fail_case("core", code, 0, &sha, &path_rel, "llvm-nm failed on the core ELF");
        }
        let dump = std::fs::read(&sym).unwrap_or_default();
        for s in CONTRACT_SYMS {
            if !word_sym_present(&dump, s) {
                self.fail_case(
                    "core",
                    code,
                    0,
                    &sha,
                    &path_rel,
                    &format!("debug-layout contract symbol '{s}' absent from the artifact"),
                );
            }
        }
        self.emit_case("core", "pass", code, 0, &sha, &path_rel, &gjson);
        println!("== case core: PASS (exit 0, golden byte-identical, layout contract in-artifact) ==");
    }

    fn run_smoke(&mut self) {
        let run = self.ctx.build_and_run("smoke", &["--features", "sched-smoke"]);
        let (sha, path_rel, code) = (run.sha.clone(), run.path_rel.clone(), run.exit);
        self.assert_markers(
            "smoke",
            &run,
            0,
            &[
                "flavor: sched-smoke",
                "[smoke] PASS sleep-order",
                "[smoke] PASS rr-preempt",
                "[smoke] PASS tick-preempt",
                "[smoke] PASS fp-preserved",
                MARKER_SCHED_ALL_GOOD,
            ],
        );
        let order = wake_order(&run.raw);
        if order != "123" {
            self.fail_case(
                "smoke",
                code,
                0,
                &sha,
                &path_rel,
                &format!("worker wake order '{order}' != '123'"),
            );
        }
        self.assert_forbidden_cr_exit("smoke", &run, 0, &["KERNEL PANIC", "EXCEPTION:"]);
        self.emit_case("smoke", "pass", code, 0, &sha, &path_rel, "null");
        println!("== case smoke: PASS (exit 0, order 123, preemption + FP survival proven) ==");
    }

    fn run_panic(&mut self) {
        let run = self.ctx.build_and_run("panic", &["--features", "test-panic"]);
        let (sha, path_rel, code) = (run.sha.clone(), run.path_rel.clone(), run.exit);
        self.assert_markers(
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
        );
        self.assert_forbidden_cr_exit("panic", &run, 1, &["EXCEPTION:"]);
        self.emit_case("panic", "pass", code, 1, &sha, &path_rel, "null");
        println!("== case panic: PASS (exit 1, thread context) ==");
    }

    fn run_fault(&mut self) {
        let run = self.ctx.build_and_run("fault", &["--features", "test-fault"]);
        let (sha, path_rel, code) = (run.sha.clone(), run.path_rel.clone(), run.exit);
        self.assert_markers(
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
        );
        self.assert_forbidden_cr_exit("fault", &run, 2, &["KERNEL PANIC"]);
        self.emit_case("fault", "pass", code, 2, &sha, &path_rel, "null");
        println!("== case fault: PASS (exit 2, PSP extended frame reported) ==");
    }
}

/// Run the sched gate.
pub fn run(repo: &Path) -> ! {
    let (facts, qemu, qemu_v) = setup(repo);
    let golden = repo.join("src").join("scripts").join("goldens").join("base-boot.txt");
    if !golden.is_file() {
        println!("rhu-gate: golden missing at {GOLDEN_REL}");
        exit(1);
    }
    let out = repo.join("src").join("kernel").join("target").join("sched-gate");
    if std::fs::create_dir_all(&out).is_err() {
        println!("rhu-gate: cannot create {}", out.display());
        exit(1);
    }
    let results = match Results::create(&out.join("sched.results.jsonl")) {
        Ok(r) => r,
        Err(e) => {
            println!("rhu-gate: cannot create results file: {e}");
            exit(1);
        }
    };
    let rustc_v = facts.version.clone();
    let mut sched = SCHED {
        ctx: GateCtx {
            repo: repo.to_path_buf(),
            gate: GATE.to_string(),
            display: DISPLAY.to_string(),
            out,
            out_rel: "src/kernel/target/sched-gate".to_string(),
            results,
            qemu,
            timeout: Duration::from_secs(TIMEOUT_S),
        },
        golden,
        facts,
    };
    sched.ctx.results.emit(&format!(
        "{{\"schema\":1,\"type\":\"run\",\"gate\":\"sched\",\"run\":\"sched-{}\",\"host\":\"{HOST}\",\"runner\":\"{RUNNER}\",\"chain\":{},\"tools\":{{\"qemu\":\"{}\",\"rustc\":\"{}\"}}}}",
        std::process::id(),
        chain_json_from_env(),
        qemu_v,
        rustc_v
    ));
    sched.run_core();
    sched.run_smoke();
    sched.run_panic();
    sched.run_fault();
    sched.ctx.results.emit_summary(GATE, "pass", 0);
    if !validate_results(repo, sched.ctx.results.path()) {
        println!("{DISPLAY}: FAIL -- results file failed schema validation");
        exit(1);
    }
    println!("{DISPLAY}: PASS (4/4)  [{qemu_v} | {rustc_v}]");
    exit(0);
}

/// Negative-control the checkers sched adds beyond base's family (wake order,
/// the smoke/fault verdict pins, the word-bounded layout-contract symbol
/// check), plus one accept-direction probe per reject-direction family.
pub fn selfcheck(repo: &Path) -> ! {
    let (facts, _qemu, qemu_v) = setup(repo);
    let rustc_v = facts.version.clone();
    let out = repo.join("src").join("kernel").join("target").join("sched-gate");
    let tmp = out.join("selfcheck");
    let _ = std::fs::create_dir_all(&tmp);
    let results = match Results::create(&out.join("sched-selfcheck.results.jsonl")) {
        Ok(r) => r,
        Err(e) => {
            println!("rhu-gate: cannot create results file: {e}");
            exit(1);
        }
    };
    let mut sc = Selfcheck {
        repo: repo.to_path_buf(),
        gate: "sched-selfcheck".to_string(),
        display: "SCHED SELFCHECK".to_string(),
        results,
        n: 0,
        npass: 0,
        nneg: 0,
    };
    sc.results.emit(&format!(
        "{{\"schema\":1,\"type\":\"run\",\"gate\":\"sched-selfcheck\",\"run\":\"sched-selfcheck-{}\",\"host\":\"{HOST}\",\"runner\":\"{RUNNER}\",\"chain\":null,\"tools\":{{\"qemu\":\"{}\",\"rustc\":\"{}\"}}}}",
        std::process::id(),
        qemu_v,
        rustc_v
    ));

    let badorder = tmp.join("badorder.raw");
    write_fixture(
        &badorder,
        b"[smoke] worker 1 awake\r\n[smoke] worker 3 awake\r\n[smoke] worker 2 awake\r\n",
    );
    sc.probe(
        "wake-order",
        "workers waking 1,3,2 instead of 1,2,3",
        wake_order(&std::fs::read(&badorder).unwrap_or_default()) == "123",
    );

    let missing3 = tmp.join("missing3.raw");
    write_fixture(&missing3, b"[smoke] worker 1 awake\r\n[smoke] worker 2 awake\r\n");
    sc.probe(
        "wake-missing",
        "a worker never waking (order 12)",
        wake_order(&std::fs::read(&missing3).unwrap_or_default()) == "123",
    );

    let nofp = tmp.join("nofp.raw");
    write_fixture(&nofp, b"[smoke] PASS rr-preempt\r\n");
    sc.probe(
        "fp-marker",
        "smoke transcript without the fp-preserved verdict",
        contains_fixed(&std::fs::read(&nofp).unwrap_or_default(), "[smoke] PASS fp-preserved"),
    );

    let nopsp = tmp.join("nopsp.raw");
    write_fixture(&nopsp, b"x\r\n");
    sc.probe(
        "psp-pin",
        "fault transcript without the PSP extended-frame pin",
        contains_fixed(
            &std::fs::read(&nopsp).unwrap_or_default(),
            "EXC_RETURN=0xffffffed (extended FP frame on PSP)",
        ),
    );

    let nolayout = tmp.join("nolayout.nm");
    write_fixture(&nolayout, b"20000008 D __rhu_sched\n20000040 B __rhu_ticks\n");
    sc.probe(
        "layout-sym-gone",
        "symbol dump missing __rhu_debug_layout (the LTO-eliminated shape)",
        word_sym_present(&std::fs::read(&nolayout).unwrap_or_default(), "__rhu_debug_layout"),
    );

    let goodorder = tmp.join("goodorder.raw");
    write_fixture(
        &goodorder,
        b"[smoke] worker 1 awake\r\n[smoke] worker 2 awake\r\n[smoke] worker 3 awake\r\n",
    );
    sc.accept_probe(
        "wake-order-good",
        "correct wake order 123 accepted by the order check",
        wake_order(&std::fs::read(&goodorder).unwrap_or_default()) == "123",
    );

    let goodlayout = tmp.join("goodlayout.nm");
    write_fixture(
        &goodlayout,
        b"00000000 R __rhu_debug_layout\n20000008 D __rhu_sched\n20000040 B __rhu_ticks\n",
    );
    sc.accept_probe(
        "layout-sym-present",
        "full contract symbol dump accepted by the nm check",
        word_sym_present(&std::fs::read(&goodlayout).unwrap_or_default(), "__rhu_debug_layout"),
    );

    sc.finish("genuine inputs accepted");
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
