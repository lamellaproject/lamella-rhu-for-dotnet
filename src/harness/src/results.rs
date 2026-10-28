//! Structured-results emission (JSON-lines, schema 1) and the validator
//! hookup.
//!
//! One results file per gate invocation: exactly one run line first, one
//! case line per case in execution order, exactly one summary line last.
//! The results file records outcomes; it never decides them. As its last
//! step, a gate validates its own file with the shared results-check tool,
//! built from source with the pinned toolchain -- a missing or unbuildable
//! validator is a gate FAILURE, not a skip.


use std::fmt::Write as _;
use std::fs::{self, File, OpenOptions};
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command;

/// JSON string-escape (backslash, quote, control bytes). The values gates
/// emit are clean ASCII, so on real inputs this is the identity -- it
/// exists so the emitter cannot produce an invalid line on a hostile
/// tool-version string.
pub fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out
}

/// The chain-membership JSON fragment: `null` outside a chain, or
/// `{"id":...,"index":N}` from the chain owner's environment
/// (RHU_CHAIN_ID / RHU_CHAIN_INDEX), one owner process driving every
/// gate alike.
pub fn chain_json_from_env() -> String {
    match std::env::var("RHU_CHAIN_ID") {
        Ok(id) if !id.is_empty() => {
            let index = std::env::var("RHU_CHAIN_INDEX").unwrap_or_else(|_| "0".to_string());
            format!("{{\"id\":\"{}\",\"index\":{}}}", json_escape(&id), index)
        }
        _ => "null".to_string(),
    }
}

/// Append-only writer for one results file, tracking the summary tallies.
pub struct Results {
    path: PathBuf,
    pub n_pass: u32,
    pub n_fail: u32,
    pub n_skip: u32,
    pub n_neg: u32,
}

impl Results {
    /// Create (truncating) the results file.
    pub fn create(path: &Path) -> std::io::Result<Results> {
        File::create(path)?;
        Ok(Results {
            path: path.to_path_buf(),
            n_pass: 0,
            n_fail: 0,
            n_skip: 0,
            n_neg: 0,
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Append one pre-rendered JSON line.
    pub fn emit(&self, line: &str) {
        if let Ok(mut f) = OpenOptions::new().append(true).open(&self.path) {
            let _ = writeln!(f, "{line}");
        }
    }

    /// Tally a case verdict (the summary line must match the case lines
    /// exactly; the validator refuses a lying tally).
    pub fn tally(&mut self, verdict: &str, control: &str) {
        match verdict {
            "pass" => self.n_pass += 1,
            "fail" => self.n_fail += 1,
            "skip" => self.n_skip += 1,
            _ => {}
        }
        if control == "negative" {
            self.n_neg += 1;
        }
    }

    /// The summary line (field order fixed by the schema's emitters).
    pub fn emit_summary(&self, gate: &str, verdict: &str, exit: i32) {
        self.emit(&format!(
            "{{\"schema\":1,\"type\":\"summary\",\"gate\":\"{}\",\"verdict\":\"{}\",\"cases\":{},\"pass\":{},\"fail\":{},\"skip\":{},\"neg_controls\":{},\"exit\":{}}}",
            gate,
            verdict,
            self.n_pass + self.n_fail + self.n_skip,
            self.n_pass,
            self.n_fail,
            self.n_skip,
            self.n_neg,
            exit
        ));
    }
}

/// The image JSON fragment for a case line.
pub fn image_json(path_rel: &str, sha: &str) -> String {
    format!("{{\"path\":\"{}\",\"sha256\":\"{}\"}}", path_rel, sha)
}

/// The golden JSON fragment for a case line.
pub fn golden_json(path_rel: &str, sha: &str, compare: &str) -> String {
    format!(
        "{{\"path\":\"{}\",\"sha256\":\"{}\",\"compare\":\"{}\"}}",
        path_rel, sha, compare
    )
}

/// Validate a results file with the shared checker: built from
/// src/scripts/results_check.rs when missing or stale, with the pinned
/// toolchain, to src/kernel/target/tools/results-check -- one shared checker
/// for every results consumer. Returns false (a FAILURE, never a skip) when the
/// source is missing, the build fails, or the file is rejected.
pub fn validate_results(repo: &Path, file: &Path) -> bool {
    let vsrc = repo.join("src").join("scripts").join("results_check.rs");
    if !vsrc.is_file() {
        println!(
            "rhu-gate: results validator source missing at src/scripts/results_check.rs -- a missing checker is a FAILURE, not a skip"
        );
        return false;
    }
    let vdir = repo.join("src").join("kernel").join("target").join("tools");
    let mut vbin = vdir.join("results-check");
    if !vbin.is_file() {
        vbin = vdir.join("results-check.exe");
    }
    let stale = match (fs::metadata(&vsrc), fs::metadata(&vbin)) {
        (Ok(src), Ok(bin)) => match (src.modified(), bin.modified()) {
            (Ok(s), Ok(b)) => s > b,
            _ => true,
        },
        _ => true,
    };
    if !vbin.is_file() || stale {
        if fs::create_dir_all(&vdir).is_err() {
            println!("rhu-gate: cannot create {} -- a broken checker is a FAILURE, not a skip", vdir.display());
            return false;
        }
        let built = Command::new("rustc")
            .args(["-O", "--edition", "2021", "-o"])
            .arg(vdir.join("results-check"))
            .arg(&vsrc)
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if !built {
            println!(
                "rhu-gate: results validator failed to build -- a broken checker is a FAILURE, not a skip"
            );
            return false;
        }
        vbin = vdir.join("results-check");
        if !vbin.is_file() {
            vbin = vdir.join("results-check.exe");
        }
    }
    Command::new(&vbin)
        .arg(file)
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}
