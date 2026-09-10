//! The api gate on the runner: hosting-API conformance on QEMU
//! mps2-an500, cumulative.

use crate::gate::{write_fixture, CaseRun, GateCtx, Selfcheck};
use crate::image::{check_records, header_abi, synth_records, HEADER_MAGIC, LAYOUT_MAGIC};
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
    "[api] PASS susp-setctx (sp edit refused exact, the edited pc ran the pad with the planted witness)",
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
/// The two debug-facing markers, boot order: the thread-local-base door
/// check (after tls-blocks: the layout record walked from memory against
/// the doors) and the thread-id-space check (the LAST check, before the
/// exit-door line: the last two ids handed out, the spawn past the space
/// refused).
const MARKERS_G: [&str; 2] = [
    "[api] PASS tls-base-door (record walked from memory: module-0 base null on every live thread beside empty template brackets, registered-module bases equal to the door's answers on kmain and both workers, non-live slots null)",
    "[api] PASS tid-wrap (ids 0x7ffffffe and 0x7fffffff handed out positive, the spawn past the id space refused EAGAIN twice with the counter unmoved, no negative id)",
];
/// The host-band marker: the thread that hosts a payload
/// joins the hosted band for the hosting's life -- a door-spawned child
/// at the flat priority started through yield alone at one level with
/// its host, joining re-ran the pick, leaving restored kernel urgency.
/// Runs after entropy-honesty and before tid-wrap.
const MARKERS_H: [&str; 1] = [
    "[api] PASS host-band (the host joined the band and its door-spawned child at the flat priority started through yield alone at one level, joining re-ran the pick, leaving restored kernel urgency and held a band thread until the host blocked)",
];
/// The lazy floating-point marker: the activation resume
/// completes the state the handler armed before it pops the target's
/// frames -- a basic-frame target yields eight times after its resume
/// and the switch-entry witness counted no abandoned state. Runs after
/// act-sleep-rearm.
const MARKERS_I: [&str; 1] = [
    "[api] PASS act-lazy-fp (the handler ran one floating-point instruction on a target whose interrupted frame was basic, the target resumed and yielded eight times after it -- eight switch entries with its own frame basic -- and the witness counted no abandoned lazy floating-point state)",
];
const CONTRACT_SYMS: [&str; 5] = [
    "__rhu_debug_layout",
    "__rhu_sched",
    "__rhu_ticks",
    "__rhu_switches",
    "__rhu_lazy_fp_witness",
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

fn setup(repo: &Path) -> (RustcFacts, std::path::PathBuf, String) {
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
        markers.extend(MARKERS_G);
        markers.extend(MARKERS_H);
        markers.extend(MARKERS_I);
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
        if word_sym_present(&dump, "rhu_payload_entry") {
            self.fail_case("api", code, 0, &sha, &path_rel, "rhu_payload_entry present in a base-family artifact (the entry seam belongs to the payload family)");
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
        let header_text = std::fs::read_to_string(self.ctx.repo.join("src/kernel/include/rhu.h")).unwrap_or_default();
        let Some(abi) = header_abi(&header_text) else {
            self.ctx.gate_fail("api: src/kernel/include/rhu.h states no RHU_ABI (the contract generation the image header carries)")
        };
        let elf_bytes = std::fs::read(&elf).unwrap_or_default();
        let rec = match check_records(&dump, &elf_bytes, abi) {
            Ok(r) => r,
            Err(e) => self.fail_case("api", code, 0, &sha, &path_rel, &format!("image records: {e}")),
        };
        self.emit_case("api", "pass", code, 0, &sha, &path_rel);
        println!(
            "== case api: PASS (exit 0, 39 hosting-API checks, contract + API symbols (the lazy floating-point witness among them) + zero-length module-root brackets in-artifact, no payload-entry seam in this family, the image header at {:#x} stating load address {:#010x} and generation {} and the layout record at {:#x} at version {} for {} threads with {} tls slots of {} bytes at offset {} in a {}-byte TCB, read off the artifact and tied) ==",
            rec.header_at, rec.load_addr, rec.abi, rec.layout_at, rec.layout_version, rec.max_threads, rec.tls_slots, rec.tls_slot_bytes, rec.tcb_off_tls, rec.tcb_stride
        );
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
    let (facts, qemu, qemu_v) = setup(repo);
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
    let (facts, _qemu, qemu_v) = setup(repo);
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

    let seam = tmp.join("seam-in-base.nm");
    write_fixture(
        &seam,
        b"00000000 T rhu_now_ns\n00000010 T rhu_payload_entry\n00000020 T rhu_sleep_ns\n",
    );
    sc.probe(
        "entry-seam-in-base",
        "base-family symbol dump carrying rhu_payload_entry (the seam belongs to the payload family)",
        !word_sym_present(&std::fs::read(&seam).unwrap_or_default(), "rhu_payload_entry"),
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
    sc.accept_probe(
        "entry-seam-absent-accepted",
        "genuine base-family dump without rhu_payload_entry accepted by the absence pin",
        !word_sym_present(&goodsym_bytes, "rhu_payload_entry"),
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

    let gdoor = tmp.join("one-of-two-g-door.raw");
    write_fixture(&gdoor, format!("{}\r\n", MARKERS_G[1]).as_bytes());
    sc.probe(
        "g-door-marker-gone",
        "one-of-two transcript missing the tls-base-door marker",
        MARKERS_G.iter().all(|m| contains_fixed(&std::fs::read(&gdoor).unwrap_or_default(), m)),
    );
    let gwrap = tmp.join("one-of-two-g-wrap.raw");
    write_fixture(&gwrap, format!("{}\r\n", MARKERS_G[0]).as_bytes());
    sc.probe(
        "g-wrap-marker-gone",
        "one-of-two transcript missing the tid-wrap marker",
        MARKERS_G.iter().all(|m| contains_fixed(&std::fs::read(&gwrap).unwrap_or_default(), m)),
    );

    let hgone = tmp.join("h-marker-gone.raw");
    write_fixture(&hgone, format!("{}\r\n{}\r\n", MARKERS_G[0], MARKERS_G[1]).as_bytes());
    sc.probe(
        "h-marker-gone",
        "transcript with the two debug-facing markers and no host-band marker",
        MARKERS_H.iter().all(|m| contains_fixed(&std::fs::read(&hgone).unwrap_or_default(), m)),
    );
    let hgreen = tmp.join("h-marker-present.raw");
    write_fixture(&hgreen, format!("{}\r\n", MARKERS_H[0]).as_bytes());
    sc.accept_probe(
        "h-marker-present",
        "genuine host-band PASS marker accepted by the H sweep",
        MARKERS_H.iter().all(|m| contains_fixed(&std::fs::read(&hgreen).unwrap_or_default(), m)),
    );

    let igone = tmp.join("i-marker-gone.raw");
    write_fixture(&igone, format!("{}\r\n", MARKERS_H[0]).as_bytes());
    sc.probe(
        "i-marker-gone",
        "transcript with the host-band marker and no act-lazy-fp marker",
        MARKERS_I.iter().all(|m| contains_fixed(&std::fs::read(&igone).unwrap_or_default(), m)),
    );
    let igreen = tmp.join("i-marker-present.raw");
    write_fixture(&igreen, format!("{}\r\n", MARKERS_I[0]).as_bytes());
    sc.accept_probe(
        "i-marker-present",
        "genuine act-lazy-fp PASS marker accepted by the I sweep",
        MARKERS_I.iter().all(|m| contains_fixed(&std::fs::read(&igreen).unwrap_or_default(), m)),
    );

    let header_text = std::fs::read_to_string(repo.join("src/kernel/include/rhu.h")).unwrap_or_default();
    let abi = header_abi(&header_text).unwrap_or(0);
    let load: u32 = 0x0800_0000;
    let good_h: [u32; 16] = [HEADER_MAGIC, 1, 64, load, abi, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
    let good_l: [u32; 16] = [LAYOUT_MAGIC, 2, 16, 100, 64, 140, 0, 12, 13, 20, 32, 4, 8, 48, 4, 4];
    let (gd, ge) = synth_records(load, load, &good_h, &good_l);
    sc.accept_probe(
        "records-genuine",
        "synthesized image with the header at load + 0x400 and a version-2 layout record accepted, every tie holding",
        check_records(&gd, &ge, abi).is_ok(),
    );
    let nohdr = format!("{:08x} R __rhu_debug_layout\n", load + 0x800).into_bytes();
    sc.probe(
        "records-header-absent",
        "symbol dump without __rhu_image_header (an image that states nothing)",
        check_records(&nohdr, &ge, abi).is_ok(),
    );
    let mut h = good_h;
    h[0] = 0x4948_5552;
    let (d, e) = synth_records(load, load, &h, &good_l);
    sc.probe("records-magic", "image header with a wrong magic word", check_records(&d, &e, abi).is_ok());
    let mut h = good_h;
    h[3] = load + 4;
    let (d, e) = synth_records(load, load, &h, &good_l);
    sc.probe(
        "records-offset",
        "image header stating a load address whose + 0x400 is not where the header sits",
        check_records(&d, &e, abi).is_ok(),
    );
    let (d, e) = synth_records(load, load - 0x1000, &good_h, &good_l);
    sc.probe(
        "records-load-addr",
        "image whose lowest load segment sits below the header's stated load address",
        check_records(&d, &e, abi).is_ok(),
    );
    let mut h = good_h;
    h[9] = 1;
    let (d, e) = synth_records(load, load, &h, &good_l);
    sc.probe("records-reserved", "image header with a reserved word set", check_records(&d, &e, abi).is_ok());
    let mut h = good_h;
    h[4] = abi + 1;
    let (d, e) = synth_records(load, load, &h, &good_l);
    sc.probe(
        "records-abi",
        "image header stating the next contract generation beside include/rhu.h's",
        check_records(&d, &e, abi).is_ok(),
    );
    let mut l = good_l;
    l[1] = 3;
    let (d, e) = synth_records(load, load, &good_h, &l);
    sc.probe(
        "records-version-tie",
        "layout record at version 3 beside a header stating version 2",
        check_records(&d, &e, abi).is_ok(),
    );
    let mut h = good_h;
    h[5] = 1;
    let mut l = good_l;
    l[1] = 1;
    let (d, e) = synth_records(load, load, &h, &l);
    sc.probe(
        "records-predates-door",
        "version-1 layout record beside a header stating version 1 (no thread-local-base door to read)",
        check_records(&d, &e, abi).is_ok(),
    );
    let mut l = good_l;
    l[13] = 132;
    let (d, e) = synth_records(load, load, &good_h, &l);
    sc.probe(
        "records-tls-overrun",
        "layout record whose tls slots run past the TCB stride",
        check_records(&d, &e, abi).is_ok(),
    );

    sc.finish("genuine inputs accepted");
}
