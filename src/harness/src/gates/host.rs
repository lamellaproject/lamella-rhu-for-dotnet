//! The host gate: the kernel's arithmetic modules and the handshake's judge compiled from their own
//! source files by the host crate (src/kernel-host) and exercised on the
//! host toolchain -- the boundary cases a bounded emulator run cannot
//! reach (a request that wraps the heap's arithmetic, a wake target past
//! the 32-bit tick window, the arena's own overflow refusals). The gate
//! runs the crate's tests with cargo under a target dir of its own, reads
//! the run's own output, pins the case list and prints one line per case
//! (cargo's timing line never reaches the transcript); the selfcheck
//! feeds the reader doctored outputs.

use crate::gate::Selfcheck;
use crate::results::{chain_json_from_env, validate_results, Results};
use crate::tools::rustc_facts;
use std::path::Path;
use std::process::{exit, Command, Stdio};
use std::time::{Duration, Instant};

const GATE: &str = "host";
const DISPLAY: &str = "HOST GATE";
const HOST: &str = "host-cargo-test";
const RUNNER: &str = "src/harness";
const TIMEOUT_S: u64 = 900;

/// The cases pinned, in the order cargo's single-threaded runner reports
/// them (alphabetical by name). A case added to the host crate joins this
/// list in the same change, or the gate refuses the run.
pub const CASES: [&str; 20] = [
    "tests::abi_window_judges_the_window",
    "tests::arena_overflow_refused_headroom_intact",
    "tests::calendar_bcd_packs_both_ways",
    "tests::calendar_century_bound_refuses_1999_and_2100",
    "tests::calendar_civil_from_days_at_the_maximum",
    "tests::calendar_days_from_civil_round_trips_the_constants",
    "tests::calendar_set_saturates_at_the_epoch",
    "tests::fpemul_arithmetic_matches_the_host_in_round_to_nearest",
    "tests::fpemul_compare_max_min_match_the_host",
    "tests::fpemul_conversions_match_the_host",
    "tests::fpemul_decoder_matches_the_assembler",
    "tests::fpemul_flags_and_directed_rounding_by_hand",
    "tests::fpemul_fma_is_fused_and_matches_the_host",
    "tests::kheap_largest_fits_and_one_more_byte_does_not",
    "tests::kheap_rounding_wraps_refused",
    "tests::kheap_wrapping_request_refused_null_heap_intact",
    "tests::wake_arm_across_the_32bit_wrap",
    "tests::wake_arm_beyond_window_clamps_and_rearms",
    "tests::wake_arm_inside_window_is_exact",
    "tests::wake_tick_for_deadline_ceils_and_saturates",
];

/// Read a cargo test run: the passed cases in order and the summary
/// judged. Err names the first thing wrong -- a failed or missing case, a
/// summary that is not `ok`, a count that is not the pinned list's.
pub fn parse_run(text: &str) -> Result<Vec<String>, String> {
    let mut passed = Vec::new();
    let mut summary: Option<&str> = None;
    for line in text.lines() {
        let line = line.trim_end_matches('\r');
        if line.starts_with("test result: ") {
            if summary.is_none() {
                summary = Some(line);
            }
        } else if let Some(rest) = line.strip_prefix("test ") {
            if let Some(name) = rest.strip_suffix(" ... ok") {
                passed.push(name.to_string());
            } else if rest.ends_with(" ... FAILED") || rest.ends_with(" ... ignored") {
                return Err(format!("a case did not pass: {line}"));
            }
        }
    }
    let Some(summary) = summary else {
        return Err("no `test result:` summary in the run".to_string());
    };
    if !summary.starts_with("test result: ok. ") {
        return Err(format!("the summary is not ok: {summary}"));
    }
    let count: usize = summary
        .trim_start_matches("test result: ok. ")
        .split(" passed")
        .next()
        .and_then(|n| n.trim().parse().ok())
        .ok_or_else(|| format!("unreadable summary: {summary}"))?;
    if count != CASES.len() || passed.len() != CASES.len() {
        return Err(format!("{} cases passed ({} in the summary), the pinned list has {}", passed.len(), count, CASES.len()));
    }
    for want in CASES {
        if !passed.iter().any(|p| p == want) {
            return Err(format!("the pinned case {want} did not run"));
        }
    }
    Ok(passed)
}

fn emit_case(results: &mut Results, case: &str, verdict: &str) {
    results.emit(&format!(
        "{{\"schema\":1,\"type\":\"case\",\"gate\":\"host\",\"case\":\"{case}\",\"verdict\":\"{verdict}\",\"exit\":null,\"exit_want\":null,\"control\":\"positive\",\"image\":null}}"
    ));
    results.tally(verdict, "positive");
}

pub fn run(repo: &Path) -> ! {
    let facts = match rustc_facts() {
        Ok(f) => f,
        Err(e) => {
            println!("rhu-gate: {e}");
            exit(1);
        }
    };
    let crate_dir = repo.join("src").join("kernel-host");
    if !crate_dir.join("Cargo.toml").is_file() {
        println!("{DISPLAY}: FAIL -- the host crate is missing at src/kernel-host");
        exit(1);
    }
    let out = repo.join("src").join("kernel").join("target").join("host-gate");
    if std::fs::create_dir_all(&out).is_err() {
        println!("rhu-gate: cannot create {}", out.display());
        exit(1);
    }
    let mut results = match Results::create(&out.join("host.results.jsonl")) {
        Ok(r) => r,
        Err(e) => {
            println!("rhu-gate: cannot create results file: {e}");
            exit(1);
        }
    };
    results.emit(&format!(
        "{{\"schema\":1,\"type\":\"run\",\"gate\":\"host\",\"run\":\"host-{}\",\"host\":\"{HOST}\",\"runner\":\"{RUNNER}\",\"chain\":{},\"tools\":{{\"rustc\":\"{}\"}}}}",
        std::process::id(),
        chain_json_from_env(),
        facts.version
    ));
    let log_path = out.join("cargo-test.log");
    let err_path = out.join("cargo-test.stderr");
    let (log, err) = match (std::fs::File::create(&log_path), std::fs::File::create(&err_path)) {
        (Ok(a), Ok(b)) => (a, b),
        _ => {
            println!("rhu-gate: cannot create the host gate's logs");
            exit(1);
        }
    };
    let mut child = match Command::new("cargo")
        .args(["test", "--release", "--lib", "--", "--test-threads=1"])
        .current_dir(&crate_dir)
        .env("CARGO_TARGET_DIR", crate_dir.join("target"))
        .stdin(Stdio::null())
        .stdout(Stdio::from(log))
        .stderr(Stdio::from(err))
        .spawn()
    {
        Ok(c) => c,
        Err(e) => {
            println!("{DISPLAY}: FAIL -- cannot spawn cargo: {e}");
            exit(1);
        }
    };
    let deadline = Instant::now() + Duration::from_secs(TIMEOUT_S);
    let status = loop {
        match child.try_wait() {
            Ok(Some(s)) => break Some(s),
            Ok(None) if Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                break None;
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(100)),
            Err(e) => {
                println!("{DISPLAY}: FAIL -- waiting on cargo: {e}");
                exit(1);
            }
        }
    };
    let text = std::fs::read_to_string(&log_path).unwrap_or_default();
    let errtext = std::fs::read_to_string(&err_path).unwrap_or_default();
    let verdict = match status {
        None => Err(format!("the run did not finish inside {TIMEOUT_S} s")),
        Some(s) if !s.success() => Err(format!("cargo test exited {}", s.code().unwrap_or(1))),
        Some(_) => parse_run(&text),
    };
    match verdict {
        Ok(passed) => {
            for case in &passed {
                let short = case.trim_start_matches("tests::");
                println!("== case host.{short}: PASS");
                emit_case(&mut results, &format!("host.{short}"), "pass");
            }
            results.emit_summary(GATE, "pass", 0);
            if !validate_results(repo, results.path()) {
                println!("{DISPLAY}: FAIL -- results file failed schema validation");
                exit(1);
            }
            println!("{DISPLAY}: PASS ({}/{} cases on the host toolchain)  [{}]", passed.len(), CASES.len(), facts.version);
            exit(0);
        }
        Err(e) => {
            for line in text.lines().chain(errtext.lines()) {
                let line = line.trim_end_matches('\r');
                if line.starts_with("test ") || line.starts_with("test result") || line.contains("panicked") || line.starts_with("error") || line.contains("assertion") {
                    println!("  | {line}");
                }
            }
            emit_case(&mut results, "host.run", "fail");
            results.emit_summary(GATE, "fail", 1);
            let _ = validate_results(repo, results.path());
            println!("{DISPLAY}: FAIL -- {e}");
            exit(1);
        }
    }
}

/// Negative-control the reader: a failed case, a summary that is not ok,
/// a run with no summary and a run one case short are refused; the
/// genuine run is accepted.
pub fn selfcheck(repo: &Path) -> ! {
    let facts = match rustc_facts() {
        Ok(f) => f,
        Err(e) => {
            println!("rhu-gate: {e}");
            exit(1);
        }
    };
    let out = repo.join("src").join("kernel").join("target").join("host-gate");
    let _ = std::fs::create_dir_all(&out);
    let results = match Results::create(&out.join("host-selfcheck.results.jsonl")) {
        Ok(r) => r,
        Err(e) => {
            println!("rhu-gate: cannot create results file: {e}");
            exit(1);
        }
    };
    let mut sc = Selfcheck {
        repo: repo.to_path_buf(),
        gate: "host-selfcheck".to_string(),
        display: "HOST SELFCHECK".to_string(),
        results,
        n: 0,
        npass: 0,
        nneg: 0,
    };
    sc.results.emit(&format!(
        "{{\"schema\":1,\"type\":\"run\",\"gate\":\"host-selfcheck\",\"run\":\"host-selfcheck-{}\",\"host\":\"{HOST}\",\"runner\":\"{RUNNER}\",\"chain\":null,\"tools\":{{\"rustc\":\"{}\"}}}}",
        std::process::id(),
        facts.version
    ));
    let genuine: String = {
        let mut s = format!("\nrunning {} tests\n", CASES.len());
        for c in CASES {
            s.push_str(&format!("test {c} ... ok\n"));
        }
        s.push_str(&format!("\ntest result: ok. {} passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s\n", CASES.len()));
        s
    };
    let failed = genuine.replace(&format!("test {} ... ok", CASES[2]), &format!("test {} ... FAILED", CASES[2]))
        .replace(&format!("test result: ok. {} passed; 0 failed", CASES.len()), &format!("test result: FAILED. {} passed; 1 failed", CASES.len() - 1));
    sc.probe("case-failed", "a run with one FAILED case", parse_run(&failed).is_ok());
    let short = genuine.replace(&format!("test {} ... ok\n", CASES[0]), "")
        .replace(&format!("test result: ok. {} passed", CASES.len()), &format!("test result: ok. {} passed", CASES.len() - 1));
    sc.probe("case-short", "a run one case short of the pinned list", parse_run(&short).is_ok());
    let nosummary = genuine.replace("test result: ok.", "test outcome: ok.");
    sc.probe("summary-missing", "a run with no test result summary", parse_run(&nosummary).is_ok());
    let renamed = genuine.replace(&format!("test {} ... ok", CASES[1]), "test tests::some_other_case ... ok");
    sc.probe("case-renamed", "a run whose count matches but a pinned case never ran", parse_run(&renamed).is_ok());
    sc.accept_probe(
        "genuine-run",
        "the genuine run of the pinned cases accepted, in order",
        parse_run(&genuine).map(|p| p.len() == CASES.len()).unwrap_or(false),
    );
    sc.finish("genuine input accepted");
}
