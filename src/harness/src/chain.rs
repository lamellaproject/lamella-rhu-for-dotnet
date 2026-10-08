//! The chain owner: every gate in order, one owner process, and a
//! verdict line that DECLARES any tier it could not run -- stated loudly,
//! never skipped silently.

use std::io::Write as _;
use std::path::Path;
use std::process::{exit, Command};

/// The chain verdict line: gate count, gate names, and the declared
/// absence of every frozen-input tier whose inputs are not present
/// (payload, gc, shim, rt-m7 and rt-m33), in leg order. Byte-identical to the chain of
/// record's line -- the release pipeline parses it into the manifest --
/// and byte-identical to the pre-generalization rendering for the
/// zero- and payload-declared shapes (the core selfcheck pins all
/// three shapes).
pub fn chain_verdict(gates: &[&str], declared_absent: &[(&str, &str)]) -> String {
    let declared: String = declared_absent
        .iter()
        .map(|(tier, inputs)| format!(" + {tier} tier NOT RUN (frozen inputs absent: {inputs})"))
        .collect();
    format!(
        "CHAIN: PASS ({} gates: {}){}",
        gates.len(),
        gates.join(" "),
        declared
    )
}

/// Print one owner line and flush: child legs inherit the same stdout,
/// so an unflushed parent banner would batch AFTER a child's output in a
/// captured transcript and break the interleaving the console shows.
fn say(line: &str) {
    println!("{line}");
    let _ = std::io::stdout().flush();
}

pub fn run(repo: &Path) -> ! {
    let exe = match std::env::current_exe() {
        Ok(p) => p,
        Err(e) => {
            say(&format!("rhu-gate chain: cannot resolve own binary path: {e}"));
            exit(1);
        }
    };

    let mut legs: Vec<&str> = vec!["base", "sched"];
    let mut declared: Vec<(&str, &str)> = Vec::new();
    if repo.join("goldens").join("payload").is_dir() {
        legs.push("payload");
    } else {
        declared.push(("payload", "goldens/payload"));
        say("==== chain: payload tier frozen inputs absent (goldens/payload) -- tier DECLARED, not run ====");
    }
    legs.push("api");
    legs.push("host");
    if repo.join("goldens").join("gc").is_dir() {
        legs.push("gc");
    } else {
        declared.push(("gc", "goldens/gc"));
        say("==== chain: gc tier frozen inputs absent (goldens/gc) -- tier DECLARED, not run ====");
    }
    if repo.join("goldens").join("shim").is_dir() {
        legs.push("shim");
    } else {
        declared.push(("shim", "goldens/shim"));
        say("==== chain: shim tier frozen inputs absent (goldens/shim) -- tier DECLARED, not run ====");
    }
    if repo.join("goldens").join("rt-m7").is_dir() {
        legs.push("rt-m7");
    } else {
        declared.push(("rt-m7", "goldens/rt-m7"));
        say("==== chain: rt-m7 tier frozen inputs absent (goldens/rt-m7) -- tier DECLARED, not run ====");
    }
    if repo.join("goldens").join("rt-m33").is_dir() {
        legs.push("rt-m33");
    } else {
        declared.push(("rt-m33", "goldens/rt-m33"));
        say("==== chain: rt-m33 tier frozen inputs absent (goldens/rt-m33) -- tier DECLARED, not run ====");
    }

    let chain_id = format!("chain-{}", std::process::id());
    let mut names: Vec<String> = Vec::new();
    for (i, leg) in legs.iter().enumerate() {
        let name = format!("rhu-gate-{leg}");
        say(&format!("==== chain [{i}] {name} ===="));
        let status = Command::new(&exe)
            .arg(leg)
            .env("RHU_CHAIN_ID", &chain_id)
            .env("RHU_CHAIN_INDEX", i.to_string())
            .status();
        match status {
            Ok(s) if s.success() => names.push(name),
            Ok(s) => {
                match s.code() {
                    Some(_) => say(&format!("CHAIN: FAIL at {name}")),
                    None => say(&format!("CHAIN: FAIL at {name} (terminated by signal)")),
                }
                exit(1);
            }
            Err(e) => {
                say(&format!("CHAIN: FAIL at {name} (spawn error: {e})"));
                exit(1);
            }
        }
    }

    let name_refs: Vec<&str> = names.iter().map(String::as_str).collect();
    say(&chain_verdict(&name_refs, &declared));
    exit(0);
}
