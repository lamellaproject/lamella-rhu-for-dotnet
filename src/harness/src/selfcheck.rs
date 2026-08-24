//! The runner core's own break-it-once: every helper the gates compose --
//! digests, transcript checks, stream filters, pin parsers, verdict-line
//! builders, the JSON escaper -- probed in both directions with embedded
//! fixtures, and the emission validated by the shared results checker.

use crate::chain::chain_verdict;
use crate::gate::Selfcheck;
use crate::results::{json_escape, validate_results, Results};
use crate::sha256::sha256_hex;
use crate::text::{
    contains_fixed, cr_count, eq_lf_normalized, final_free, final_gcs, payload_stream,
    wake_order, word_sym_present,
};
use crate::tools::{rustc_facts, ExecProfile};
use std::path::Path;
use std::process::exit;

const GATE: &str = "rhu-gate-selfcheck";
const DISPLAY: &str = "RHU-GATE SELFCHECK";

const SHA_EMPTY: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
const SHA_ABC: &str = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
const SHA_TWO_BLOCK: &str = "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1";

pub fn run(repo: &Path) -> ! {
    let rustc_v = match rustc_facts() {
        Ok(f) => f.version,
        Err(e) => {
            println!("rhu-gate: {e}");
            exit(1);
        }
    };
    let out = repo.join("src").join("kernel").join("target").join("rhu-gate");
    if std::fs::create_dir_all(&out).is_err() {
        println!("rhu-gate: cannot create {}", out.display());
        exit(1);
    }
    let results = match Results::create(&out.join("rhu-gate-selfcheck.results.jsonl")) {
        Ok(r) => r,
        Err(e) => {
            println!("rhu-gate: cannot create results file: {e}");
            exit(1);
        }
    };
    let mut sc = Selfcheck {
        repo: repo.to_path_buf(),
        gate: GATE.to_string(),
        display: DISPLAY.to_string(),
        results,
        n: 0,
        npass: 0,
        nneg: 0,
    };
    sc.results.emit(&format!(
        "{{\"schema\":1,\"type\":\"run\",\"gate\":\"{GATE}\",\"run\":\"{GATE}-{}\",\"host\":\"qemu-mps2-an500\",\"runner\":\"src/harness\",\"tools\":{{\"rustc\":\"{}\"}}}}",
        std::process::id(),
        rustc_v
    ));

    sc.accept_probe("sha-empty", "SHA-256 empty-message vector reproduced", sha256_hex(b"") == SHA_EMPTY);
    sc.accept_probe("sha-abc", "SHA-256 'abc' vector reproduced", sha256_hex(b"abc") == SHA_ABC);
    sc.accept_probe(
        "sha-two-block",
        "SHA-256 two-block vector reproduced",
        sha256_hex(b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq") == SHA_TWO_BLOCK,
    );
    sc.probe(
        "sha-flipped",
        "digest compared against a vector with one flipped digit",
        sha256_hex(b"abc") == SHA_ABC.replace("ba78", "ba79"),
    );

    sc.accept_probe("cr-present", "CRLF capture accepted by the CR check", cr_count(b"crlf line\r\n") > 0);
    sc.probe(
        "cr-lost",
        "capture with CR bytes stripped (wire discipline lost)",
        cr_count(b"lf only\n") > 0,
    );

    sc.accept_probe(
        "marker-present",
        "present marker found by the fixed-string check",
        contains_fixed(b"x\r\n[base] ALL GOOD (ok)\r\n", "[base] ALL GOOD"),
    );
    sc.probe(
        "marker-missing",
        "transcript without the asserted marker",
        contains_fixed(b"no markers here\r\n", "[base] ALL GOOD"),
    );

    sc.accept_probe(
        "bytediff-crlf-lf",
        "same content under CRLF and LF accepted by the normalized compare",
        eq_lf_normalized(b"one\r\ntwo\r\n", b"one\ntwo\n"),
    );
    sc.probe(
        "bytediff-flip",
        "golden with one byte changed (GOOD to G00D)",
        eq_lf_normalized(b"ALL GOOD\r\n", b"ALL G00D\n"),
    );

    sc.accept_probe(
        "sym-present",
        "contract symbol accepted from a full dump",
        word_sym_present(
            b"00000000 R __rhu_debug_layout\n20000008 D __rhu_sched\n",
            "__rhu_debug_layout",
        ),
    );
    sc.probe(
        "sym-suffixed",
        "underscore-suffixed relative offered for the contract symbol",
        word_sym_present(b"00000000 R __rhu_debug_layout_ext\n", "__rhu_debug_layout"),
    );

    let wrapped: &[u8] = b"rhu 0.2.0 -- kernel banner\r\n[boot] kernel line\r\nPASS div32\r\nresult: k=7\r\n\r\n[payload] managed Main returned 0\r\n";
    let expect: &[u8] = b"PASS div32\nresult: k=7\n";
    sc.accept_probe(
        "stream-genuine",
        "payload bytes inside kernel noise accepted by the filter compare",
        payload_stream(wrapped) == expect,
    );
    sc.probe(
        "stream-flipped",
        "payload stream with one deviated byte",
        payload_stream(b"rhu banner\r\nPASS div33\r\nresult: k=7\r\n") == expect,
    );
    sc.probe(
        "stream-truncated",
        "payload stream missing all but one line",
        payload_stream(b"PASS div32\r\n") == expect,
    );

    sc.accept_probe(
        "wake-order-good",
        "correct wake order 123 accepted by the order check",
        wake_order(b"[smoke] worker 1 awake\r\n[smoke] worker 2 awake\r\n[smoke] worker 3 awake\r\n") == "123",
    );
    sc.probe(
        "wake-order-swapped",
        "workers waking 1,3,2 instead of 1,2,3",
        wake_order(b"worker 1 awake\r\nworker 3 awake\r\nworker 2 awake\r\n") == "123",
    );
    sc.probe(
        "wake-missing",
        "a worker never waking (order 12)",
        wake_order(b"worker 1 awake\r\nworker 2 awake\r\n") == "123",
    );

    let good_stream: &[u8] = b"gctest: collections=61 free=1\ngctest: collections=62 free=89016 x ALL GOOD\n";
    sc.accept_probe(
        "pins-genuine",
        "final free value parsed from the last summary line",
        final_free(good_stream).as_deref() == Some("89016"),
    );
    sc.accept_probe(
        "pins-genuine-gcs",
        "final collections count parsed from the last summary line",
        final_gcs(good_stream).as_deref() == Some("62"),
    );
    sc.probe(
        "final-free-drift",
        "final free one off the expected value",
        final_free(b"gctest: collections=62 free=89017 x\n").as_deref() == Some("89016"),
    );
    sc.probe(
        "final-line-missing",
        "stream with no final summary line",
        final_free(b"no final line here\n").is_some(),
    );

    sc.accept_probe(
        "chain-line-full",
        "four-gate chain verdict line matches the chain of record",
        chain_verdict(&["rhu-gate-base", "rhu-gate-sched", "rhu-gate-payload", "rhu-gate-api"], None)
            == "CHAIN: PASS (4 gates: rhu-gate-base rhu-gate-sched rhu-gate-payload rhu-gate-api)",
    );
    sc.accept_probe(
        "chain-line-declared",
        "declared-tier chain verdict line matches the chain of record",
        chain_verdict(&["rhu-gate-base", "rhu-gate-sched", "rhu-gate-api"], Some("goldens/payload"))
            == "CHAIN: PASS (3 gates: rhu-gate-base rhu-gate-sched rhu-gate-api) + payload tier NOT RUN (frozen inputs absent: goldens/payload)",
    );

    sc.accept_probe(
        "profile-deterministic",
        "deterministic profile argv equals the fixed icount contract",
        ExecProfile::Deterministic.qemu_args() == ["-icount", "shift=0,sleep=off"],
    );
    sc.accept_probe(
        "profile-parity-empty",
        "parity profile inserts nothing into the invocation",
        ExecProfile::Parity.qemu_args().is_empty(),
    );
    sc.probe(
        "profile-doctored",
        "deterministic argv compared against a doctored contract (shift=auto)",
        ExecProfile::Deterministic.qemu_args() == ["-icount", "shift=auto,sleep=off"],
    );

    let timing_fixture = |token: &str| {
        format!(
            "{{\"schema\":1,\"type\":\"run\",\"gate\":\"probe-timing\",\"run\":\"probe-timing-1\",\"host\":\"qemu-mps2-an500\",\"runner\":\"src/harness\",\"tools\":{{\"rustc\":\"pinned\"}}}}\n{{\"schema\":1,\"type\":\"case\",\"gate\":\"probe-timing\",\"case\":\"det-case\",\"verdict\":\"pass\",\"exit\":0,\"exit_want\":0,\"control\":\"positive\",\"timing\":\"{token}\",\"image\":null}}\n{{\"schema\":1,\"type\":\"summary\",\"gate\":\"probe-timing\",\"verdict\":\"pass\",\"cases\":1,\"pass\":1,\"fail\":0,\"skip\":0,\"neg_controls\":0,\"exit\":0}}\n"
        )
    };
    let t_ok = out.join("timing-known.results.jsonl");
    let t_bad = out.join("timing-doctored.results.jsonl");
    let fixtures_written = std::fs::write(&t_ok, timing_fixture("icount-shift0")).is_ok()
        && std::fs::write(&t_bad, timing_fixture("wall-clock")).is_ok();
    sc.accept_probe(
        "timing-known-token",
        "case line carrying the known profile token validates clean",
        fixtures_written && validate_results(repo, &t_ok),
    );
    sc.probe(
        "timing-doctored-token",
        "timing value outside the known profile set",
        validate_results(repo, &t_bad),
    );

    sc.accept_probe(
        "json-escape",
        "quote and backslash escaped; clean ASCII untouched",
        json_escape("a\"b\\c") == "a\\\"b\\\\c" && json_escape("clean text") == "clean text",
    );

    sc.finish("genuine inputs accepted");
}
