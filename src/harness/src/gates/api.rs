//! The api gate on the runner: hosting-API conformance on QEMU
//! mps2-an500, cumulative.

use crate::gate::{write_fixture, CaseRun, GateCtx, Selfcheck};
use crate::results::{chain_json_from_env, validate_results, Results};
use crate::text::{contains_fixed, cr_count, word_sym_present};
use crate::tools::{llvm_tool, resolve_qemu, run_captured, rustc_facts, RustcFacts};
use std::path::Path;
use std::process::exit;
use std::time::Duration;

const GATE: &str = "api";
const DISPLAY: &str = "API GATE";
const HOST: &str = "qemu-mps2-an500";
const RUNNER: &str = "src/harness";
const TIMEOUT_S: u64 = 60;

const MARKER_QUIESCENT: &str =
    "[api] PASS tick-quiescent (solo busy 30 ms: 0 switches, clock advanced)";
/// The eight [api] boot markers, boot order.
const MARKERS_A: [&str; 8] = [
    "[api] PASS ns-monotonic (5000 paired reads + cs wrap + saturated pend windows)",
    "[api] PASS sleep-ns-floor (25 ms request: never early)",
    "[api] PASS futex-eagain (changed value refused: no sleep, 0 switches)",
    "[api] PASS futex-timeout (10 ms deadline honored: never early)",
    "[api] PASS futex-wake-one (3 blocked: readied exactly 1, ran 0)",
    "[api] PASS futex-wake-all (released remaining 2, count exact, all ran after)",
    "[api] PASS futex-mutex (4x300 stress: counter 1200 exact, overlap-free, slow path fired)",
    MARKER_QUIESCENT,
];
const CONTRACT_SYMS: [&str; 4] = [
    "__rhu_debug_layout",
    "__rhu_sched",
    "__rhu_ticks",
    "__rhu_switches",
];
/// The futex + nanosecond-time C-ABI surface, pinned in the artifact (the
/// waistline exists and survives every build of this flavor).
const API_SYMS: [&str; 4] = ["rhu_now_ns", "rhu_sleep_ns", "rhu_futex_wait", "rhu_futex_wake"];

struct API {
    ctx: GateCtx,
    facts: RustcFacts,
}

fn setup() -> (RustcFacts, std::path::PathBuf, String) {
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

impl API {
    fn emit_case(&mut self, case: &str, verdict: &str, code: i32, want: i32, sha: &str, path_rel: &str) {
        self.ctx.results.emit(&format!(
            "{{\"schema\":1,\"type\":\"case\",\"gate\":\"api\",\"case\":\"{case}\",\"verdict\":\"{verdict}\",\"exit\":{code},\"exit_want\":{want},\"control\":\"positive\",\"image\":{{\"path\":\"{path_rel}\",\"sha256\":\"{sha}\"}},\"golden\":null}}"
        ));
        self.ctx.results.tally(verdict, "positive");
    }

    fn fail_case(&mut self, case: &str, code: i32, want: i32, sha: &str, path_rel: &str, msg: &str) -> ! {
        self.emit_case(case, "fail", code, want, sha, path_rel);
        self.ctx.gate_fail(&format!("{case}: {msg}"))
    }

    fn common_asserts(&mut self, case: &str, run: &CaseRun, want: i32, markers: &[&str], forbidden: &[&str]) {
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

    fn run_api(&mut self) {
        let run = self.ctx.build_and_run("api", &["--features", "api-smoke"]);
        let (sha, path_rel, code) = (run.sha.clone(), run.path_rel.clone(), run.exit);
        let mut markers: Vec<&str> = vec![
            "flavor: api-smoke",
            "[base] ALL GOOD (boot + console + panic/fault funnel armed)",
            "[sched] ALL GOOD (threads + timers + preemption online)",
        ];
        markers.extend(MARKERS_A);
        self.common_asserts("api", &run, 0, &markers, &["KERNEL PANIC", "EXCEPTION:"]);
        let nm = match llvm_tool(&self.facts, "llvm-nm") {
            Ok(p) => p,
            Err(e) => self.ctx.gate_fail(&format!("api: {e}")),
        };
        let elf = self.ctx.out.join("rhu-kernel-api.elf");
        let elf_arg = elf.to_string_lossy().into_owned();
        let sym = self.ctx.out.join("api.sym");
        let scratch = self.ctx.out.join("api.tool.stderr");
        let nm_run = run_captured(&nm, &[&elf_arg], &sym, &scratch, Duration::from_secs(60));
        if !matches!(nm_run, Ok(ref o) if o.exit == 0) {
            self.fail_case("api", code, 0, &sha, &path_rel, "llvm-nm failed on the api ELF");
        }
        let dump = std::fs::read(&sym).unwrap_or_default();
        for s in CONTRACT_SYMS.iter().chain(API_SYMS.iter()) {
            if !word_sym_present(&dump, s) {
                self.fail_case("api", code, 0, &sha, &path_rel, &format!("contract symbol '{s}' absent from the artifact"));
            }
        }
        self.emit_case("api", "pass", code, 0, &sha, &path_rel);
        println!("== case api: PASS (exit 0, 8 hosting-API checks, contract + API symbols in-artifact) ==");
    }
}

/// Run the api gate.
pub fn run(repo: &Path) -> ! {
    let (facts, qemu, qemu_v) = setup();
    let out = repo.join("src").join("kernel").join("target").join("api-gate");
    if std::fs::create_dir_all(&out).is_err() {
        println!("rhu-gate: cannot create {}", out.display());
        exit(1);
    }
    let results = match Results::create(&out.join("api.results.jsonl")) {
        Ok(r) => r,
        Err(e) => {
            println!("rhu-gate: cannot create results file: {e}");
            exit(1);
        }
    };
    let rustc_v = facts.version.clone();
    let mut api = API {
        ctx: GateCtx {
            repo: repo.to_path_buf(),
            gate: GATE.to_string(),
            display: DISPLAY.to_string(),
            out,
            out_rel: "src/kernel/target/api-gate".to_string(),
            results,
            qemu,
            timeout: Duration::from_secs(TIMEOUT_S),
        },
        facts,
    };
    api.ctx.results.emit(&format!(
        "{{\"schema\":1,\"type\":\"run\",\"gate\":\"api\",\"run\":\"api-{}\",\"host\":\"{HOST}\",\"runner\":\"{RUNNER}\",\"chain\":{},\"tools\":{{\"qemu\":\"{}\",\"rustc\":\"{}\"}}}}",
        std::process::id(),
        chain_json_from_env(),
        qemu_v,
        rustc_v
    ));
    api.run_api();
    api.ctx.results.emit_summary(GATE, "pass", 0);
    if !validate_results(repo, api.ctx.results.path()) {
        println!("{DISPLAY}: FAIL -- results file failed schema validation");
        exit(1);
    }
    println!("{DISPLAY}: PASS (1/1)  [{qemu_v} | {rustc_v}]");
    exit(0);
}

/// Negative-control the checkers this gate adds beyond the family:
/// the quiescence marker, the api flavor pin, the contract-symbol sweep,
/// and the forbidden-pattern sweep against the unconditional-pend red
/// shape -- each must REJECT its doctored input, and the accept side of
/// the new checkers is pinned too.
pub fn selfcheck(repo: &Path) -> ! {
    let (facts, _qemu, qemu_v) = setup();
    let rustc_v = facts.version.clone();
    let out = repo.join("src").join("kernel").join("target").join("api-gate");
    let tmp = out.join("selfcheck");
    let _ = std::fs::create_dir_all(&tmp);
    let results = match Results::create(&out.join("api-selfcheck.results.jsonl")) {
        Ok(r) => r,
        Err(e) => {
            println!("rhu-gate: cannot create results file: {e}");
            exit(1);
        }
    };
    let mut sc = Selfcheck {
        repo: repo.to_path_buf(),
        gate: "api-selfcheck".to_string(),
        display: "API SELFCHECK".to_string(),
        results,
        n: 0,
        npass: 0,
        nneg: 0,
    };
    sc.results.emit(&format!(
        "{{\"schema\":1,\"type\":\"run\",\"gate\":\"api-selfcheck\",\"run\":\"api-selfcheck-{}\",\"host\":\"{HOST}\",\"runner\":\"{RUNNER}\",\"tools\":{{\"qemu\":\"{}\",\"rustc\":\"{}\"}}}}",
        std::process::id(),
        qemu_v,
        rustc_v
    ));

    let red = tmp.join("unconditional.raw");
    write_fixture(
        &red,
        b"[api] tick-quiescent: solo busy 30 ms, switch delta armed\r\n[panic] *** KERNEL PANIC ***\r\n[panic] context switches during solo busy loop: 30\r\n[panic] *** halted ***\r\n",
    );
    let red_bytes = std::fs::read(&red).unwrap_or_default();
    sc.probe(
        "quiescent-marker-gone",
        "unconditional-pend transcript without the tick-quiescent PASS marker",
        contains_fixed(&red_bytes, MARKER_QUIESCENT),
    );
    sc.probe(
        "panic-shape",
        "forbidden-pattern sweep fed the unconditional-pend panic transcript",
        !contains_fixed(&red_bytes, "KERNEL PANIC"),
    );

    let wrongflavor = tmp.join("wrongflavor.raw");
    write_fixture(
        &wrongflavor,
        b"[boot] QEMU mps2-an500 | CMSDK UART0 | flavor: sched-smoke\r\n",
    );
    sc.probe(
        "wrong-flavor",
        "api-case flavor assertion fed a sched-smoke transcript",
        contains_fixed(&std::fs::read(&wrongflavor).unwrap_or_default(), "flavor: api-smoke"),
    );

    let nosym = tmp.join("noswitches.nm");
    write_fixture(
        &nosym,
        b"00000000 R __rhu_debug_layout\n20000008 D __rhu_sched\n20000040 B __rhu_ticks\n",
    );
    sc.probe(
        "switches-sym-gone",
        "symbol dump missing __rhu_switches (the counter LTO-eliminated shape)",
        word_sym_present(&std::fs::read(&nosym).unwrap_or_default(), "__rhu_switches"),
    );

    let partial = tmp.join("seven-of-eight.raw");
    let mut seven = Vec::new();
    for m in MARKERS_A.iter().filter(|m| !m.contains("futex-mutex")) {
        seven.extend_from_slice(m.as_bytes());
        seven.extend_from_slice(b"\r\n");
    }
    write_fixture(&partial, &seven);
    let partial_bytes = std::fs::read(&partial).unwrap_or_default();
    sc.probe(
        "a1-marker-gone",
        "seven-of-eight transcript missing the futex-mutex marker",
        MARKERS_A.iter().all(|m| contains_fixed(&partial_bytes, m)),
    );

    let overlap = tmp.join("overlap.raw");
    write_fixture(
        &overlap,
        b"[api] futex-mutex: 4 workers x 300 armed\r\n[panic] *** KERNEL PANIC ***\r\n[panic] mutex overlap: two threads inside the critical section\r\n[panic] *** halted ***\r\n",
    );
    sc.probe(
        "overlap-shape",
        "forbidden-pattern sweep fed the mutex-overlap panic transcript",
        !contains_fixed(&std::fs::read(&overlap).unwrap_or_default(), "KERNEL PANIC"),
    );

    let noapi = tmp.join("noapi.nm");
    write_fixture(
        &noapi,
        b"00000000 T rhu_now_ns\n00000010 T rhu_sleep_ns\n00000020 T rhu_futex_wake\n",
    );
    sc.probe(
        "api-sym-gone",
        "symbol dump missing rhu_futex_wait (API surface half-linked)",
        word_sym_present(&std::fs::read(&noapi).unwrap_or_default(), "rhu_futex_wait"),
    );

    let green = tmp.join("green.raw");
    write_fixture(
        &green,
        b"[api] PASS tick-quiescent (solo busy 30 ms: 0 switches, clock advanced)\r\n",
    );
    sc.accept_probe(
        "quiescent-marker-present",
        "genuine tick-quiescent PASS marker accepted",
        contains_fixed(&std::fs::read(&green).unwrap_or_default(), MARKER_QUIESCENT),
    );
    let full = tmp.join("eight-of-eight.raw");
    let mut eight = Vec::new();
    for m in MARKERS_A {
        eight.extend_from_slice(m.as_bytes());
        eight.extend_from_slice(b"\r\n");
    }
    write_fixture(&full, &eight);
    let full_bytes = std::fs::read(&full).unwrap_or_default();
    sc.accept_probe(
        "a1-markers-present",
        "genuine eight-marker transcript accepted in full",
        MARKERS_A.iter().all(|m| contains_fixed(&full_bytes, m)),
    );
    let goodsym = tmp.join("allsyms.nm");
    write_fixture(
        &goodsym,
        b"00000000 R __rhu_debug_layout\n20000008 D __rhu_sched\n20000040 B __rhu_ticks\n20000044 B __rhu_switches\n00000100 T rhu_now_ns\n00000110 T rhu_sleep_ns\n00000120 T rhu_futex_wait\n00000130 T rhu_futex_wake\n",
    );
    let goodsym_bytes = std::fs::read(&goodsym).unwrap_or_default();
    sc.accept_probe(
        "contract-syms-present",
        "full contract + API symbol dump accepted by the word-bounded sweep",
        CONTRACT_SYMS
            .iter()
            .chain(API_SYMS.iter())
            .all(|s| word_sym_present(&goodsym_bytes, s)),
    );

    sc.finish("genuine inputs accepted");
}
