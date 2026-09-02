//! The api gate on the runner: hosting-API conformance on QEMU
//! mps2-an500, cumulative.

use crate::gate::{write_fixture, CaseRun, GateCtx, Selfcheck};
use crate::results::{chain_json_from_env, validate_results, Results};
use crate::text::{contains_fixed, cr_count, word_sym_present, word_sym_value};
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
/// The five arena markers, boot order (the exhaustion check's marker
/// is last).
const MARKERS_B: [&str; 5] = [
    "[api] PASS arena-carve (window bracketed, disjoint from kstacks + statics)",
    "[api] PASS arena-zero-birth (pages read all-zero through the boot poison)",
    "[api] PASS arena-rezero (range re-zeroed in place: same address, zero VA burned, oob refused)",
    "[api] PASS arena-audit (two sites attributed exactly: rows, counts, bytes)",
    "[api] PASS arena-headroom (exact fit taken, further alloc refused NULL, headroom 0)",
];
/// The seven kheap/TLS/thread/registry markers, boot order (this
/// family appends after the arena family's exhaustion check).
const MARKERS_C: [&str; 7] = [
    "[api] PASS kheap-basic (freed block reused at the same address, alignment honored)",
    "[api] PASS kheap-coalesce (adjacent frees merged whole: the summed span reallocated at the first block)",
    "[api] PASS tls-blocks (per-thread blocks distinct, init image + zero fill honored, no cross-thread bleed)",
    "[api] PASS tls-dtors (dtors ran LIFO exactly once on the exiting thread, TLS live inside, cap refused honest)",
    "[api] PASS thread-abi (round trip clean, tids unique, exact-fit stack reused, slot exhaustion refused honest)",
    "[api] PASS thread-bounds (own and peer locals bracketed, windows inside kstacks, reaped tid refused ESRCH)",
    "[api] PASS registry-basic (handles distinct, stored desc survives caller clobber, full table refused honest)",
];
/// The ten suspension/activation markers, boot order (this family
/// appends after registry-basic).
const MARKERS_D: [&str; 10] = [
    "[api] PASS susp-pick (suspended spinner stayed ready with zero progress, woken waiter held until resume)",
    "[api] PASS susp-ctx (planted registers exact in ctx, sp inside the target stack, pc inside kernel text)",
    "[api] PASS susp-hijack (sp edit refused exact, hijacked pc ran the pad with the planted witness)",
    "[api] PASS resume-urgent (resume preempted the caller before its next line)",
    "[api] PASS act-refuse (pre-registration send, null/second registration, self/idle suspends, idle send, unknown tids, unsuspended resume/set-context all refused exact)",
    "[api] PASS act-deliver (handler ran once per send on the target thread and stack, both interrupted flows continued)",
    "[api] PASS act-coalesce (three sends delivered exactly one handler run)",
    "[api] PASS act-redeliver (the handler re-entered before the interrupted flow resumed, then the flow continued)",
    "[api] PASS act-eintr (the wait returned RHU_EINTR after the handler ran, the re-wait then woke clean)",
    "[api] PASS act-sleep-rearm (the handler ran early, the sleep still returned not-early)",
];
/// The two module-query markers, boot order (this pair appends after
/// act-sleep-rearm).
const MARKERS_E: [&str; 2] = [
    "[api] PASS mod-find (kernel text resolved to module 0 half-open exact, arena address refused ESRCH, miss wrote nothing)",
    "[api] PASS mod-roots (module 0 ranges tie the link brackets, frozen, exidx, and tls empty in this family, walk ends ESRCH at the table count)",
];
/// The four hosting-door markers, boot order: the console-door line is
/// the write door's own wire output (the door IS the path that emits
/// it); the exit-door line is the LAST transcript line -- the machine
/// exit itself then rides rhu_exit(0), asserted via the exit status.
const MARKERS_F: [&str; 4] = [
    "[api] console-door: this line reached the wire through rhu_console_write",
    "[api] PASS console-pair (door write returned exact and hit the wire, no-input read returned 0 writing nothing, null and oversize refused)",
    "[api] PASS entropy-honesty (draws filled and distinct, null refused, zero-length trivial, hardware query answers 0 on this class)",
    "[api] exit-door: handing the machine exit to rhu_exit(0)",
];
const CONTRACT_SYMS: [&str; 4] = [
    "__rhu_debug_layout",
    "__rhu_sched",
    "__rhu_ticks",
    "__rhu_switches",
];
/// The module-root bracket pairs, pinned BY VALUE in the api (base-family)
/// artifact: each pair must be present AND equal (the zero-length
/// claim; the payload gate owns the payload-family half).
const BRACKET_PAIRS: [(&str, &str); 3] = [
    ("__rhu_frozen_lo", "__rhu_frozen_hi"),
    ("__exidx_start", "__exidx_end"),
    ("__rhu_tdata_lo", "__rhu_tdata_hi"),
];
/// The futex + nanosecond-time + arena + kheap/TLS/thread/registry +
/// suspension/activation + module-query + console/exit/entropy C-ABI
/// surface, pinned in the artifact (the waistline exists and survives
/// every build of this flavor). 30 exports.
const API_SYMS: [&str; 30] = [
    "rhu_now_ns",
    "rhu_sleep_ns",
    "rhu_futex_wait",
    "rhu_futex_wake",
    "rhu_arena_alloc_at",
    "rhu_arena_rezero",
    "rhu_arena_headroom",
    "rhu_arena_report",
    "rhu_kheap_alloc",
    "rhu_kheap_free",
    "rhu_module_register",
    "rhu_tls_get",
    "rhu_thread_atexit",
    "rhu_thread_spawn",
    "rhu_thread_exit",
    "rhu_thread_self",
    "rhu_thread_yield",
    "rhu_thread_stack_bounds",
    "rhu_thread_suspend",
    "rhu_thread_set_context",
    "rhu_thread_resume",
    "rhu_activation_register",
    "rhu_activation_send",
    "rhu_module_find",
    "rhu_module_roots",
    "rhu_console_write",
    "rhu_console_read",
    "rhu_exit",
    "rhu_entropy",
    "rhu_entropy_is_hardware",
];

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
        markers.extend(MARKERS_B);
        markers.extend(MARKERS_C);
        markers.extend(MARKERS_D);
        markers.extend(MARKERS_E);
        markers.extend(MARKERS_F);
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
        for (lo, hi) in BRACKET_PAIRS.iter() {
            let (Some(vlo), Some(vhi)) = (word_sym_value(&dump, lo), word_sym_value(&dump, hi))
            else {
                self.fail_case("api", code, 0, &sha, &path_rel, &format!("bracket pair {lo}/{hi} not value-parseable from the artifact"));
            };
            if vlo != vhi {
                self.fail_case("api", code, 0, &sha, &path_rel, &format!("bracket pair {lo}/{hi} not zero-length in a base-family image ({vlo:#x} != {vhi:#x})"));
            }
        }
        self.emit_case("api", "pass", code, 0, &sha, &path_rel);
        println!("== case api: PASS (exit 0, 35 hosting-API checks, contract + API symbols + zero-length module-root brackets in-artifact) ==");
    }

    /// The kheap-loud designed death: the corrupted
    /// free-block canary must kill the very next walk through the
    /// funnel. Exit 1, constant panic text; the UNREACHABLE line is
    /// the survived red shape (the C-NC2 control's expected trip).
    fn run_kheap_loud(&mut self) {
        let run = self.ctx.build_and_run("kheap-loud", &["--features", "test-kheap-loud"]);
        let (sha, path_rel, code) = (run.sha.clone(), run.path_rel.clone(), run.exit);
        self.common_asserts(
            "kheap-loud",
            &run,
            1,
            &[
                "flavor: test-kheap-loud",
                "[kheap-loud] corrupting a free block header, then allocating -- expect a KERNEL PANIC",
                "[panic] *** KERNEL PANIC ***",
                "kheap: corrupt block header",
                "[panic] *** halted ***",
            ],
            &["UNREACHABLE", "EXCEPTION:"],
        );
        self.emit_case("kheap-loud", "pass", code, 1, &sha, &path_rel);
        println!("== case kheap-loud: PASS (exit 1, corrupt free header died loud through the funnel) ==");
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
    api.run_kheap_loud();
    api.ctx.results.emit_summary(GATE, "pass", 0);
    if !validate_results(repo, api.ctx.results.path()) {
        println!("{DISPLAY}: FAIL -- results file failed schema validation");
        exit(1);
    }
    println!("{DISPLAY}: PASS (2/2)  [{qemu_v} | {rustc_v}]");
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

    let bpartial = tmp.join("four-of-five-arena.raw");
    let mut bfour = Vec::new();
    for m in MARKERS_B.iter().filter(|m| !m.contains("arena-headroom")) {
        bfour.extend_from_slice(m.as_bytes());
        bfour.extend_from_slice(b"\r\n");
    }
    write_fixture(&bpartial, &bfour);
    let bpartial_bytes = std::fs::read(&bpartial).unwrap_or_default();
    sc.probe(
        "arena-marker-gone",
        "four-of-five transcript missing the arena-headroom marker",
        MARKERS_B.iter().all(|m| contains_fixed(&bpartial_bytes, m)),
    );

    let carve = tmp.join("carve-red.raw");
    write_fixture(
        &carve,
        b"[api] arena-carve: window + disjointness asserts armed\r\n[panic] *** KERNEL PANIC ***\r\n[panic] arena window overlaps .kstacks\r\n[panic] *** halted ***\r\n",
    );
    sc.probe(
        "carve-shape",
        "forbidden-pattern sweep fed the carve-panic transcript",
        !contains_fixed(&std::fs::read(&carve).unwrap_or_default(), "KERNEL PANIC"),
    );

    let noarena = tmp.join("noarena.nm");
    write_fixture(
        &noarena,
        b"00000000 T rhu_now_ns\n00000010 T rhu_sleep_ns\n00000020 T rhu_futex_wait\n00000030 T rhu_futex_wake\n00000040 T rhu_arena_alloc_at\n00000050 T rhu_arena_headroom\n00000060 T rhu_arena_report\n",
    );
    sc.probe(
        "arena-sym-gone",
        "symbol dump missing rhu_arena_rezero (arena surface half-linked)",
        word_sym_present(&std::fs::read(&noarena).unwrap_or_default(), "rhu_arena_rezero"),
    );

    let cpartial = tmp.join("six-of-seven-c.raw");
    let mut csix = Vec::new();
    for m in MARKERS_C.iter().filter(|m| !m.contains("registry-basic")) {
        csix.extend_from_slice(m.as_bytes());
        csix.extend_from_slice(b"\r\n");
    }
    write_fixture(&cpartial, &csix);
    let cpartial_bytes = std::fs::read(&cpartial).unwrap_or_default();
    sc.probe(
        "c-marker-gone",
        "six-of-seven transcript missing the registry-basic marker",
        MARKERS_C.iter().all(|m| contains_fixed(&cpartial_bytes, m)),
    );

    let survived = tmp.join("kheaploud-survived.raw");
    write_fixture(
        &survived,
        b"[kheap-loud] corrupting a free block header, then allocating -- expect a KERNEL PANIC\r\n[kheap-loud] UNREACHABLE: the corrupt walk survived\r\n",
    );
    sc.probe(
        "kheaploud-survived-shape",
        "forbidden-pattern sweep fed the survived kheap-loud transcript",
        !contains_fixed(&std::fs::read(&survived).unwrap_or_default(), "UNREACHABLE"),
    );

    let noc = tmp.join("noc.nm");
    let mut seventeen = Vec::new();
    for (i, s) in API_SYMS.iter().enumerate() {
        if *s == "rhu_tls_get" {
            continue;
        }
        seventeen.extend_from_slice(format!("{:08x} T {}\n", i * 16, s).as_bytes());
    }
    write_fixture(&noc, &seventeen);
    sc.probe(
        "c-sym-gone",
        "symbol dump missing rhu_tls_get (C surface half-linked)",
        word_sym_present(&std::fs::read(&noc).unwrap_or_default(), "rhu_tls_get"),
    );

    let dpartial = tmp.join("nine-of-ten-d.raw");
    let mut dnine = Vec::new();
    for m in MARKERS_D.iter().filter(|m| !m.contains("act-sleep-rearm")) {
        dnine.extend_from_slice(m.as_bytes());
        dnine.extend_from_slice(b"\r\n");
    }
    write_fixture(&dpartial, &dnine);
    let dpartial_bytes = std::fs::read(&dpartial).unwrap_or_default();
    sc.probe(
        "d-marker-gone",
        "nine-of-ten transcript missing the act-sleep-rearm marker",
        MARKERS_D.iter().all(|m| contains_fixed(&dpartial_bytes, m)),
    );

    let nod = tmp.join("nod.nm");
    let mut twentytwo = Vec::new();
    for (i, s) in API_SYMS.iter().enumerate() {
        if *s == "rhu_activation_send" {
            continue;
        }
        twentytwo.extend_from_slice(format!("{:08x} T {}\n", i * 16, s).as_bytes());
    }
    write_fixture(&nod, &twentytwo);
    sc.probe(
        "d-sym-gone",
        "symbol dump missing rhu_activation_send (D surface half-linked)",
        word_sym_present(&std::fs::read(&nod).unwrap_or_default(), "rhu_activation_send"),
    );

    let epartial = tmp.join("one-of-two-e.raw");
    let mut eone = Vec::new();
    for m in MARKERS_E.iter().filter(|m| !m.contains("mod-roots")) {
        eone.extend_from_slice(m.as_bytes());
        eone.extend_from_slice(b"\r\n");
    }
    write_fixture(&epartial, &eone);
    let epartial_bytes = std::fs::read(&epartial).unwrap_or_default();
    sc.probe(
        "e-marker-gone",
        "one-of-two transcript missing the mod-roots marker",
        MARKERS_E.iter().all(|m| contains_fixed(&epartial_bytes, m)),
    );

    let noe = tmp.join("noe.nm");
    let mut twentyfour = Vec::new();
    for (i, s) in API_SYMS.iter().enumerate() {
        if *s == "rhu_module_roots" {
            continue;
        }
        twentyfour.extend_from_slice(format!("{:08x} T {}\n", i * 16, s).as_bytes());
    }
    write_fixture(&noe, &twentyfour);
    sc.probe(
        "e-sym-gone",
        "symbol dump missing rhu_module_roots (E surface half-linked)",
        word_sym_present(&std::fs::read(&noe).unwrap_or_default(), "rhu_module_roots"),
    );

    let unequal = tmp.join("bracket-unequal.nm");
    write_fixture(
        &unequal,
        b"00012000 R __rhu_frozen_lo\n00012040 R __rhu_frozen_hi\n0000da48 T __exidx_start\n0000da48 T __exidx_end\n",
    );
    let unequal_bytes = std::fs::read(&unequal).unwrap_or_default();
    sc.probe(
        "bracket-unequal",
        "bracket-value checker fed a frozen pair 0x40 apart (base family must be zero-length)",
        matches!(
            (word_sym_value(&unequal_bytes, "__rhu_frozen_lo"), word_sym_value(&unequal_bytes, "__rhu_frozen_hi")),
            (Some(a), Some(b)) if a == b
        ),
    );

    let fpartial = tmp.join("three-of-four-doors.raw");
    let mut fthree = Vec::new();
    for m in MARKERS_F.iter().filter(|m| !m.contains("entropy-honesty")) {
        fthree.extend_from_slice(m.as_bytes());
        fthree.extend_from_slice(b"\r\n");
    }
    write_fixture(&fpartial, &fthree);
    let fpartial_bytes = std::fs::read(&fpartial).unwrap_or_default();
    sc.probe(
        "door-marker-gone",
        "three-of-four transcript missing the entropy-honesty marker",
        MARKERS_F.iter().all(|m| contains_fixed(&fpartial_bytes, m)),
    );

    let nof = tmp.join("nodoors.nm");
    let mut twentynine = Vec::new();
    for (i, s) in API_SYMS.iter().enumerate() {
        if *s == "rhu_entropy" {
            continue;
        }
        twentynine.extend_from_slice(format!("{:08x} T {}\n", i * 16, s).as_bytes());
    }
    write_fixture(&nof, &twentynine);
    sc.probe(
        "door-sym-gone",
        "symbol dump missing rhu_entropy beside a surviving rhu_entropy_is_hardware (door surface half-linked)",
        word_sym_present(&std::fs::read(&nof).unwrap_or_default(), "rhu_entropy"),
    );

    let tunequal = tmp.join("tdata-unequal.nm");
    write_fixture(
        &tunequal,
        b"20000000 D __rhu_tdata_lo\n20000010 D __rhu_tdata_hi\n",
    );
    let tunequal_bytes = std::fs::read(&tunequal).unwrap_or_default();
    sc.probe(
        "tdata-bracket-unequal",
        "bracket-value checker fed a tls pair 0x10 apart (base family must be zero-length)",
        matches!(
            (word_sym_value(&tunequal_bytes, "__rhu_tdata_lo"), word_sym_value(&tunequal_bytes, "__rhu_tdata_hi")),
            (Some(a), Some(b)) if a == b
        ),
    );

    let nobracket = tmp.join("bracket-missing.nm");
    write_fixture(
        &nobracket,
        b"00012000 R __rhu_frozen_lo\n00012000 R __rhu_frozen_hi\n0000da48 T __exidx_start\n",
    );
    let nobracket_bytes = std::fs::read(&nobracket).unwrap_or_default();
    sc.probe(
        "bracket-sym-gone",
        "bracket-value checker fed a dump missing __exidx_end (parse must refuse, not default)",
        matches!(
            (word_sym_value(&nobracket_bytes, "__exidx_start"), word_sym_value(&nobracket_bytes, "__exidx_end")),
            (Some(a), Some(b)) if a == b
        ),
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
    let full = tmp.join("thirtytwo-of-thirtytwo.raw");
    let mut thirtytwo = Vec::new();
    for m in MARKERS_A
        .iter()
        .chain(MARKERS_B.iter())
        .chain(MARKERS_C.iter())
        .chain(MARKERS_D.iter())
        .chain(MARKERS_E.iter())
    {
        thirtytwo.extend_from_slice(m.as_bytes());
        thirtytwo.extend_from_slice(b"\r\n");
    }
    write_fixture(&full, &thirtytwo);
    let full_bytes = std::fs::read(&full).unwrap_or_default();
    sc.accept_probe(
        "a1-markers-present",
        "genuine thirty-two-marker transcript accepted by the A sweep",
        MARKERS_A.iter().all(|m| contains_fixed(&full_bytes, m)),
    );
    sc.accept_probe(
        "arena-markers-present",
        "genuine thirty-two-marker transcript accepted by the B sweep",
        MARKERS_B.iter().all(|m| contains_fixed(&full_bytes, m)),
    );
    sc.accept_probe(
        "c-markers-present",
        "genuine thirty-two-marker transcript accepted by the C sweep",
        MARKERS_C.iter().all(|m| contains_fixed(&full_bytes, m)),
    );
    sc.accept_probe(
        "d-markers-present",
        "genuine thirty-two-marker transcript accepted by the D sweep",
        MARKERS_D.iter().all(|m| contains_fixed(&full_bytes, m)),
    );
    sc.accept_probe(
        "e-markers-present",
        "genuine thirty-two-marker transcript accepted by the E sweep",
        MARKERS_E.iter().all(|m| contains_fixed(&full_bytes, m)),
    );
    let deathgreen = tmp.join("kheaploud-death.raw");
    write_fixture(
        &deathgreen,
        b"[kheap-loud] corrupting a free block header, then allocating -- expect a KERNEL PANIC\r\n[panic] *** KERNEL PANIC ***\r\n[panic] at src\\kheap.rs:99:5\r\n[panic] kheap: corrupt block header\r\n[panic] *** halted ***\r\n",
    );
    let deathgreen_bytes = std::fs::read(&deathgreen).unwrap_or_default();
    sc.accept_probe(
        "kheaploud-death-present",
        "genuine kheap-loud death transcript accepted by its marker sweep",
        contains_fixed(&deathgreen_bytes, "kheap: corrupt block header")
            && !contains_fixed(&deathgreen_bytes, "UNREACHABLE"),
    );
    let goodsym = tmp.join("allsyms.nm");
    let mut allsyms = Vec::new();
    for (i, s) in CONTRACT_SYMS.iter().chain(API_SYMS.iter()).enumerate() {
        allsyms.extend_from_slice(format!("{:08x} T {}\n", i * 16, s).as_bytes());
    }
    write_fixture(&goodsym, &allsyms);
    let goodsym_bytes = std::fs::read(&goodsym).unwrap_or_default();
    sc.accept_probe(
        "contract-syms-present",
        "full contract + API symbol dump accepted by the word-bounded sweep",
        CONTRACT_SYMS
            .iter()
            .chain(API_SYMS.iter())
            .all(|s| word_sym_present(&goodsym_bytes, s)),
    );
    let goodbrackets = tmp.join("brackets-equal.nm");
    write_fixture(
        &goodbrackets,
        b"00012d1c R __rhu_frozen_lo\n00012d1c R __rhu_frozen_hi\n0000da48 T __exidx_start\n0000da48 T __exidx_end\n20000000 D __rhu_tdata_lo\n20000000 D __rhu_tdata_hi\n",
    );
    let goodbrackets_bytes = std::fs::read(&goodbrackets).unwrap_or_default();
    sc.accept_probe(
        "brackets-zero-length-accepted",
        "genuine base-family bracket dump (every pair equal) accepted by the value checker",
        BRACKET_PAIRS.iter().all(|(lo, hi)| {
            matches!(
                (word_sym_value(&goodbrackets_bytes, lo), word_sym_value(&goodbrackets_bytes, hi)),
                (Some(a), Some(b)) if a == b
            )
        }),
    );

    sc.finish("genuine inputs accepted");
}
