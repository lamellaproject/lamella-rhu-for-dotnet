//! The runner core's own break-it-once: every helper the gates compose --
//! digests, transcript checks, stream filters, pin parsers, verdict-line
//! builders, the JSON escaper -- probed in both directions with embedded
//! fixtures, and the emission validated by the shared results checker.

use crate::chain::chain_verdict;
use crate::gate::Selfcheck;
use crate::release;
use crate::results::{json_escape, validate_results, Results};
use crate::sha256::sha256_hex;
use crate::text::{
    contains_fixed, cr_count, eq_lf_normalized, final_free, final_gcs, payload_stream,
    wake_order, word_sym_present,
};
use crate::tools::{
    budget, command_words, guest_image, qemu_ladder, rustc_facts, stat_line_time, ExecProfile, Guest,
    Limits, QemuSource, Stop, Watch, QEMU_POINTER,
};
use std::path::{Path, PathBuf};
use std::process::exit;
use std::time::Duration;

const GATE: &str = "rhu-gate-selfcheck";
const DISPLAY: &str = "RHU-GATE SELFCHECK";

const SHA_EMPTY: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
const SHA_ABC: &str = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
const SHA_TWO_BLOCK: &str = "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1";
/// The release module's archive of its two fixture members, as GNU tar
/// 1.35 writes the same members in the ustar format with single-block
/// records, sorted names, owner and group 0, mode 0644 and modification
/// time 0 (3584 bytes).
const SHA_TAR_FIXTURE: &str = "0aa5407ceceaa0782a43a1f2f2d5bf50e1e44834a7b1ba96ca4ab7bf15871c9a";

/// Feed the judge of an instruction-counted run one poll a second for
/// `seconds`, the reading of each second from `spent_at`: the rule that
/// stopped the run and its second, or `None` for a run that went on to the
/// end.
fn watch_run(limits: Limits, seconds: u64, spent_at: impl Fn(u64) -> Option<Duration>) -> Option<(Stop, u64)> {
    let mut watch = Watch::new(limits);
    for second in 0..=seconds {
        if let Some(stop) = watch.observe(Duration::from_secs(second), spent_at(second)) {
            return Some((stop, second));
        }
    }
    None
}

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
        "nine-gate chain verdict line matches the chain of record",
        chain_verdict(&["rhu-gate-base", "rhu-gate-sched", "rhu-gate-payload", "rhu-gate-api", "rhu-gate-host", "rhu-gate-gc", "rhu-gate-shim", "rhu-gate-rt-m7", "rhu-gate-rt-m33"], &[])
            == "CHAIN: PASS (9 gates: rhu-gate-base rhu-gate-sched rhu-gate-payload rhu-gate-api rhu-gate-host rhu-gate-gc rhu-gate-shim rhu-gate-rt-m7 rhu-gate-rt-m33)",
    );
    sc.accept_probe(
        "chain-line-declared",
        "declared-tier chain verdict line matches the chain of record",
        chain_verdict(&["rhu-gate-base", "rhu-gate-sched", "rhu-gate-api", "rhu-gate-host", "rhu-gate-gc", "rhu-gate-shim", "rhu-gate-rt-m7", "rhu-gate-rt-m33"], &[("payload", "goldens/payload")])
            == "CHAIN: PASS (8 gates: rhu-gate-base rhu-gate-sched rhu-gate-api rhu-gate-host rhu-gate-gc rhu-gate-shim rhu-gate-rt-m7 rhu-gate-rt-m33) + payload tier NOT RUN (frozen inputs absent: goldens/payload)",
    );
    sc.accept_probe(
        "chain-line-both-declared",
        "drop-shaped chain verdict line (payload + gc + shim + rt-m7 + rt-m33 tiers declared) matches",
        chain_verdict(
            &["rhu-gate-base", "rhu-gate-sched", "rhu-gate-api", "rhu-gate-host"],
            &[("payload", "goldens/payload"), ("gc", "goldens/gc"), ("shim", "goldens/shim"), ("rt-m7", "goldens/rt-m7"), ("rt-m33", "goldens/rt-m33")],
        ) == "CHAIN: PASS (4 gates: rhu-gate-base rhu-gate-sched rhu-gate-api rhu-gate-host) + payload tier NOT RUN (frozen inputs absent: goldens/payload) + gc tier NOT RUN (frozen inputs absent: goldens/gc) + shim tier NOT RUN (frozen inputs absent: goldens/shim) + rt-m7 tier NOT RUN (frozen inputs absent: goldens/rt-m7) + rt-m33 tier NOT RUN (frozen inputs absent: goldens/rt-m33)",
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

    let on_path = Some(PathBuf::from("path-dir/qemu-system-arm"));
    sc.accept_probe(
        "qemu-variable-first",
        "the QEMU variable wins over a PATH hit and a pointer line",
        matches!(
            qemu_ladder(Some("var/qemu"), on_path.clone(), Some("ptr/qemu"), 3),
            Ok((ref p, QemuSource::Variable)) if p.as_path() == Path::new("var/qemu")
        ),
    );
    sc.accept_probe(
        "qemu-path-second",
        "an empty variable falls through to the PATH hit ahead of the pointer",
        matches!(
            qemu_ladder(Some("  "), on_path.clone(), Some("ptr/qemu"), 3),
            Ok((ref p, QemuSource::Path)) if p.as_path() == Path::new("path-dir/qemu-system-arm")
        ),
    );
    sc.accept_probe(
        "qemu-pointer-third",
        "no variable and no PATH hit resolve through the pointer file's line, trimmed",
        matches!(
            qemu_ladder(None, None, Some("  ptr/qemu  "), 3),
            Ok((ref p, QemuSource::Pointer)) if p.as_path() == Path::new("ptr/qemu")
        ),
    );
    sc.probe(
        "qemu-nothing-resolves",
        "no variable, no PATH hit, no pointer line (the refusal must name the variable, the directory count and the pointer)",
        match qemu_ladder(None, None, None, 7) {
            Ok(_) => true,
            Err(e) => !(e.contains("QEMU") && e.contains("7 directories") && e.contains(QEMU_POINTER)),
        },
    );
    sc.accept_probe(
        "command-quoted-word",
        "a double-quoted word with spaces stays one word; the rest split on whitespace",
        command_words("\"tool dir/clang.exe\" --target=arm-none-eabi  -x c")
            == Ok(vec!["tool dir/clang.exe".to_string(), "--target=arm-none-eabi".to_string(), "-x".to_string(), "c".to_string()]),
    );
    sc.accept_probe(
        "command-plain-words",
        "an unquoted command splits exactly on whitespace",
        command_words("wsl.exe -d Ubuntu -- kit/bin/clang")
            == Ok(vec!["wsl.exe".to_string(), "-d".to_string(), "Ubuntu".to_string(), "--".to_string(), "kit/bin/clang".to_string()]),
    );
    sc.probe(
        "command-unbalanced-quote",
        "a command with an unbalanced double quote",
        command_words("\"tool dir/clang.exe --version").is_ok(),
    );

    let minute = Limits { deadline: Duration::from_secs(60), budget: Duration::from_secs(60) };
    sc.accept_probe(
        "watch-busy-host",
        "a run given a fifth of a processor goes on past its wall deadline, its budget unspent",
        watch_run(minute, 250, |s| Some(Duration::from_secs(s) / 5)).is_none(),
    );
    sc.accept_probe(
        "watch-slow-worker",
        "a run that shows a sixtieth of the deadline in work inside every window goes on",
        watch_run(minute, 600, |s| Some(Duration::from_secs(s) / 59)).is_none(),
    );
    sc.probe(
        "watch-works-without-end",
        "a run that works without end (it must be stopped for work when its budget is spent)",
        watch_run(minute, 100, |s| Some(Duration::from_secs(s))) != Some((Stop::Work, 60)),
    );
    sc.probe(
        "watch-stands-still",
        "a run that worked ten seconds and then stands still (it must be stopped for no work one deadline later)",
        watch_run(minute, 100, |s| Some(Duration::from_secs(s.min(10)))) != Some((Stop::NoWork, 70)),
    );
    sc.probe(
        "watch-launcher",
        "a process whose reading never moves (it must be stopped for no work at the deadline)",
        watch_run(minute, 100, |_| Some(Duration::from_millis(10))) != Some((Stop::NoWork, 60)),
    );
    sc.probe(
        "watch-no-reading",
        "a run on a host that gives no reading (the wall clock must stop it at the deadline)",
        watch_run(minute, 100, |_| None) != Some((Stop::Deadline, 60)),
    );
    sc.accept_probe(
        "budget-floor-and-unit",
        "the budget is the work times the unit, the floor when that is less, and the floor with no unit",
        budget(Duration::from_secs(60), 80, Some(Duration::from_secs(1))) == Duration::from_secs(80)
            && budget(Duration::from_secs(60), 80, Some(Duration::from_millis(500))) == Duration::from_secs(60)
            && budget(Duration::from_secs(60), 80, None) == Duration::from_secs(60),
    );
    let stat_fields = "S 1 77 77 0 -1 4194560 100 0 0 0 250 50 0 0 20 0 5 0 100 1000000 200 18446744073709551615";
    sc.accept_probe(
        "stat-line-genuine",
        "a process's stat line read for its user and kernel time, in the host's clock ticks",
        stat_line_time(&format!("77 (qemu-system-arm) {stat_fields}"), 100) == Some(Duration::from_secs(3)),
    );
    sc.accept_probe(
        "stat-line-name",
        "a process name that holds spaces and parentheses leaves the fields where they are",
        stat_line_time(&format!("77 (a (b) c) {stat_fields}"), 100) == Some(Duration::from_secs(3)),
    );
    sc.probe(
        "stat-line-cut-short",
        "a stat line cut short of its time fields",
        stat_line_time("77 (qemu-system-arm) S 1 77 77 0 -1 4194560 100 0 0 0 250", 100).is_some(),
    );
    sc.accept_probe(
        "guest-images",
        "the three generated guests, byte for byte",
        guest_image(Guest::Work(1))
            == [
                0x00, 0x10, 0x00, 0x20, 0x09, 0x00, 0x00, 0x00, 0x03, 0x48, 0x01, 0x38, 0xFD, 0xD1, 0x18, 0x20,
                0x02, 0x49, 0xAB, 0xBE, 0xFE, 0xE7, 0x00, 0xBF, 0x01, 0x00, 0x00, 0x00, 0x26, 0x00, 0x02, 0x00,
            ]
            && guest_image(Guest::Spin) == [0x00, 0x10, 0x00, 0x20, 0x09, 0x00, 0x00, 0x00, 0xFE, 0xE7, 0x00, 0xBF]
            && guest_image(Guest::Wait) == [0x00, 0x10, 0x00, 0x20, 0x09, 0x00, 0x00, 0x00, 0x30, 0xBF, 0xFD, 0xE7],
    );

    let tree = release::FsTree { root: repo.to_path_buf() };
    let sets = release::resolve_sets(repo);
    let record = std::fs::read_to_string(repo.join("goldens").join("MANIFEST.md")).unwrap_or_default();
    let real = release::model(&tree, [sets[0].as_ref().map(|t| t as &dyn release::Tree), sets[1].as_ref().map(|t| t as &dyn release::Tree)])
        .and_then(|m| release::check(&record, &m));
    match &real {
        Ok(lines) => {
            for l in lines {
                println!("    {l}");
            }
        }
        Err(e) => println!("    {e}"),
    }
    sc.accept_probe(
        "release-record-tree",
        "goldens/MANIFEST.md ties this checkout (the tiers it holds verified, the absent ones declared)",
        real.is_ok(),
    );
    let (fx, set_m7, set_m33) = release::fixture();
    let fx_model = |t: &release::MapTree| release::model(t, [Some(&set_m7 as &dyn release::Tree), Some(&set_m33 as &dyn release::Tree)]);
    let fixture_model = fx_model(&fx).ok();
    let fixture_text = fixture_model.as_ref().and_then(|m| release::render(m).ok()).unwrap_or_default();
    let fixture_ok = |text: &str, t: &release::MapTree| fx_model(t).and_then(|m| release::check(text, &m)).is_ok();
    sc.accept_probe(
        "release-fixture-roundtrip",
        "a fixture tree's generated record checks clean against the same tree",
        !fixture_text.is_empty() && fixture_ok(&fixture_text, &fx),
    );
    let manifest_ok = fixture_model
        .as_ref()
        .map(|m| {
            !m.assets.is_empty()
                && m.assets.iter().all(|a| {
                    a.facts
                        .as_ref()
                        .map(|f| {
                            let first = f.members.first().map(|(p, s)| p == release::ARCHIVE_MANIFEST && *s == sha256_hex(f.manifest.as_bytes())).unwrap_or(false);
                            let others = f.members.len() > 1 && f.members.iter().skip(1).all(|(p, s)| f.manifest.lines().any(|l| l == format!("{s}  {p}")));
                            let not_itself = !f.manifest.lines().any(|l| l.ends_with(&format!("  {}", release::ARCHIVE_MANIFEST)));
                            let embedded = fixture_text.contains(&format!("{}  {}\n{}", sha256_hex(f.manifest.as_bytes()), release::ARCHIVE_MANIFEST, f.manifest));
                            first && others && not_itself && embedded
                        })
                        .unwrap_or(false)
                })
        })
        .unwrap_or(false);
    sc.accept_probe(
        "release-archive-manifest",
        "each fixture archive's first member is its MANIFEST.txt, listing every other member's sha256 and not itself, and the record embeds that text under its digest",
        manifest_ok,
    );
    sc.accept_probe(
        "release-tar-pinned",
        "the tar writer's bytes for the fixture members reproduce the pinned sha256",
        release::tar(&release::tar_fixture_members()).map(|b| sha256_hex(&b)).ok().as_deref() == Some(SHA_TAR_FIXTURE),
    );
    let hex_run = |line: &str| -> Option<(usize, usize)> {
        let b = line.as_bytes();
        let mut i = 0;
        while i < b.len() {
            let mut j = i;
            while j < b.len() && b[j].is_ascii_hexdigit() && !b[j].is_ascii_uppercase() {
                j += 1;
            }
            if j - i == 64 {
                return Some((i, j));
            }
            i = if j > i { j } else { i + 1 };
        }
        None
    };
    let flip = |line: &str| -> String {
        match hex_run(line) {
            Some((s, _)) => {
                let c = line.as_bytes()[s] as char;
                let n = if c == 'f' { 'e' } else { char::from_digit(c.to_digit(16).unwrap_or(0) + 1, 16).unwrap_or('0') };
                format!("{}{n}{}", &line[..s], &line[s + 1..])
            }
            None => line.to_string(),
        }
    };
    let shorten = |line: &str| -> String {
        match hex_run(line) {
            Some((s, _)) => format!("{}{}", &line[..s], &line[s + 1..]),
            None => line.to_string(),
        }
    };
    let doctor = |needle: &str, how: &dyn Fn(&str) -> Option<String>| -> String {
        let mut hit = false;
        let mut out = String::new();
        for l in fixture_text.lines() {
            if !hit && l.contains(needle) {
                hit = true;
                if let Some(nl) = how(l) {
                    out.push_str(&nl);
                    out.push('\n');
                }
            } else {
                out.push_str(l);
                out.push('\n');
            }
        }
        out
    };
    let names = release::fixture_asset_names();
    let first_asset = names.first().cloned().unwrap_or_default();
    let last_asset = names.last().cloned().unwrap_or_default();
    let object_line = format!("{}  goldens/rt-m7/object.o", sha256_hex(b"rt-m7 object bytes\n"));
    sc.probe(
        "release-doctored-tier-sha",
        "the record's sha256 of a tier manifest with one digit flipped",
        fixture_ok(&doctor("| `goldens/gc/MANIFEST.md` | `", &|l| Some(flip(l))), &fx),
    );
    sc.probe(
        "release-doctored-asset-sha",
        "the record's sha256 of an archive with one digit flipped",
        fixture_ok(&doctor(&format!("| `{first_asset}` |"), &|l| Some(flip(l))), &fx),
    );
    sc.probe(
        "release-doctored-member-sha",
        "the record's sha256 of an archive member with one digit flipped",
        fixture_ok(&doctor(&object_line,&|l| Some(flip(l))), &fx),
    );
    sc.probe(
        "release-doctored-member-missing",
        "the record with an archive member's line removed",
        fixture_ok(&doctor(&object_line,&|_| None), &fx),
    );
    sc.probe(
        "release-doctored-archive-manifest",
        "the record's copy of an archive's MANIFEST.txt with one comment line changed",
        fixture_ok(&doctor(&format!("# {first_asset}"), &|l| Some(format!("{l} (changed)"))), &fx),
    );
    let mut extra = fx.0.clone();
    extra.insert("goldens/rt-m7/extra.o".to_string(), b"a file no record names\n".to_vec());
    sc.probe(
        "release-doctored-extra-file",
        "a file added to a tier directory that no record names",
        fixture_ok(&fixture_text, &release::MapTree(extra)),
    );
    sc.probe(
        "release-malformed",
        "a record whose archive sha256 has sixty-three digits",
        fixture_ok(&doctor(&format!("| `{last_asset}` |"), &|l| Some(shorten(l))), &fx),
    );

    sc.finish("genuine inputs accepted");
}
