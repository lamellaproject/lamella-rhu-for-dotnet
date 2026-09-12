//! Gate scaffolding: the case machinery every gate definition composes,
//! and the selfcheck probe scaffold that keeps each gate's checkers
//! negative-controlled on every invocation.

use crate::results::{validate_results, Results};
use crate::sha256::sha256_file;
use crate::tools::{cargo_build_kernel, echo_file, run_captured, ExecProfile};
use std::path::{Path, PathBuf};
use std::process::exit;
use std::time::Duration;

/// One staged-and-run case: the artifact identity and the observed run.
pub struct CaseRun {
    pub sha: String,
    pub path_rel: String,
    pub raw: Vec<u8>,
    pub exit: i32,
}

/// Shared per-gate context: identity, tool facts, output home, results.
pub struct GateCtx {
    pub repo: PathBuf,
    pub gate: String,
    pub display: String,
    pub out: PathBuf,
    pub out_rel: String,
    pub results: Results,
    pub qemu: PathBuf,
    pub timeout: Duration,
}

impl GateCtx {
    /// Build the kernel with the given cargo feature arguments, stage the
    /// ELF under the case name, run it under QEMU, capture raw, echo.
    /// A build error emits the case's fail line and fails the gate.
    /// Runs on the Parity profile: the argv is byte-identical to the
    /// established gates' invocation.
    pub fn build_and_run(&mut self, name: &str, features: &[&str]) -> CaseRun {
        self.build_and_run_with(name, features, ExecProfile::Parity)
    }

    /// build_and_run under an explicit execution profile. Deterministic
    /// inserts the fixed icount argv before `-kernel`; count-class
    /// cases opt in per case, deliberately -- never by default.
    pub fn build_and_run_with(
        &mut self,
        name: &str,
        features: &[&str],
        profile: ExecProfile,
    ) -> CaseRun {
        println!("== case {name} ==");
        let log = self.out.join(format!("build-{name}.log"));
        let mut feats: Vec<&str> = features.to_vec();
        feats.extend_from_slice(&["--features", "entropy-accept-software"]);
        let ok = match cargo_build_kernel(&self.repo, &feats, &log) {
            Ok(ok) => ok,
            Err(e) => {
                println!("rhu-gate: {e}");
                false
            }
        };
        if !ok {
            echo_file(&log, "");
            self.results.emit(&format!(
                "{{\"schema\":1,\"type\":\"case\",\"gate\":\"{}\",\"case\":\"{name}\",\"verdict\":\"fail\",\"exit\":null,\"exit_want\":null,\"control\":\"positive\",\"image\":null}}",
                self.gate
            ));
            self.results.tally("fail", "positive");
            self.gate_fail(&format!("{name}: build error"));
        }
        let built = self
            .repo
            .join("src/kernel/target/thumbv7em-none-eabihf/release/rhu-kernel");
        let staged = self.out.join(format!("rhu-kernel-{name}.elf"));
        if std::fs::copy(&built, &staged).is_err() {
            self.gate_fail(&format!("{name}: built kernel not found at {}", built.display()));
        }
        let sha = match sha256_file(&staged) {
            Ok(s) => s,
            Err(e) => self.gate_fail(&format!("{name}: cannot hash staged artifact: {e}")),
        };
        let path_rel = format!("{}/rhu-kernel-{name}.elf", self.out_rel);
        let raw_path = self.out.join(format!("{name}.raw"));
        let stderr_path = self.out.join(format!("{name}.stderr"));
        let staged_arg = staged.to_string_lossy().into_owned();
        let mut args: Vec<&str> = vec![
            "-M",
            "mps2-an500",
            "-display",
            "none",
            "-monitor",
            "none",
            "-serial",
            "stdio",
            "-semihosting",
        ];
        args.extend_from_slice(profile.qemu_args());
        args.push("-kernel");
        args.push(&staged_arg);
        let outcome = match run_captured(
            &self.qemu,
            &args,
            &raw_path,
            &stderr_path,
            self.timeout,
        ) {
            Ok(o) => o,
            Err(e) => self.gate_fail(&format!("{name}: {e}")),
        };
        if outcome.timed_out {
            println!("  (run killed at its timeout; exit recorded as 124 -- a hang is observed, never a pass)");
        }
        let raw = std::fs::read(&raw_path).unwrap_or_default();
        echo_file(&raw_path, "  | ");
        CaseRun {
            sha,
            path_rel,
            raw,
            exit: outcome.exit,
        }
    }

    /// The gate's loud failure: verdict line, summary, validated results,
    /// nonzero exit. The failing case line is the caller's to have emitted.
    pub fn gate_fail(&mut self, msg: &str) -> ! {
        println!("{}: FAIL -- {msg}", self.display);
        self.results.emit_summary(&self.gate, "fail", 1);
        let _ = validate_results(&self.repo, self.results.path());
        exit(1);
    }
}

/// Selfcheck scaffold: reject-direction probes feed each checker a doctored
/// input and fail LOUD if it is accepted; accept-direction probes feed a
/// genuine input and fail loud if it is rejected (a checker broken toward
/// reject-everything is as silent as one broken toward accept-everything).
pub struct Selfcheck {
    pub repo: PathBuf,
    pub gate: String,
    pub display: String,
    pub results: Results,
    pub n: u32,
    pub npass: u32,
    pub nneg: u32,
}

impl Selfcheck {
    /// Reject-direction probe: `accepted` is the checker's answer to the
    /// doctored input; true means the breakage went unnoticed.
    pub fn probe(&mut self, id: &str, broke: &str, accepted: bool) {
        self.n += 1;
        self.nneg += 1;
        if accepted {
            self.results.emit(&format!(
                "{{\"schema\":1,\"type\":\"case\",\"gate\":\"{}\",\"case\":\"{id}\",\"verdict\":\"fail\",\"exit\":null,\"exit_want\":null,\"control\":\"negative\",\"image\":null,\"broke\":\"{broke}\"}}",
                self.gate
            ));
            self.results.tally("fail", "negative");
            self.results.emit_summary(&self.gate, "fail", 1);
            println!("{}: FAIL -- doctored input ACCEPTED: {broke}", self.display);
            exit(1);
        }
        self.npass += 1;
        self.results.emit(&format!(
            "{{\"schema\":1,\"type\":\"case\",\"gate\":\"{}\",\"case\":\"{id}\",\"verdict\":\"pass\",\"exit\":null,\"exit_want\":null,\"control\":\"negative\",\"image\":null,\"broke\":\"{broke}\"}}",
            self.gate
        ));
        self.results.tally("pass", "negative");
        println!("  reject OK: {broke}");
    }

    /// Accept-direction probe: `ok` is the checker's answer to a genuine
    /// input; false means a working input was refused.
    pub fn accept_probe(&mut self, id: &str, desc: &str, ok: bool) {
        self.n += 1;
        if ok {
            self.npass += 1;
            self.results.emit(&format!(
                "{{\"schema\":1,\"type\":\"case\",\"gate\":\"{}\",\"case\":\"{id}\",\"verdict\":\"pass\",\"exit\":null,\"exit_want\":null,\"control\":\"positive\",\"image\":null}}",
                self.gate
            ));
            self.results.tally("pass", "positive");
            println!("  accept OK: {desc}");
        } else {
            self.results.emit(&format!(
                "{{\"schema\":1,\"type\":\"case\",\"gate\":\"{}\",\"case\":\"{id}\",\"verdict\":\"fail\",\"exit\":null,\"exit_want\":null,\"control\":\"positive\",\"image\":null}}",
                self.gate
            ));
            self.results.tally("fail", "positive");
            self.results.emit_summary(&self.gate, "fail", 1);
            println!("{}: FAIL -- genuine input REJECTED: {desc}", self.display);
            exit(1);
        }
    }

    /// Summary + schema validation + the PASS verdict line.
    /// `accepted_noun` names the accept-direction tally in the verdict
    /// ("genuine input accepted" / "genuine inputs accepted").
    pub fn finish(&mut self, accepted_noun: &str) -> ! {
        self.results.emit_summary(&self.gate, "pass", 0);
        if !validate_results(&self.repo, self.results.path()) {
            println!("{}: FAIL -- results file failed schema validation", self.display);
            exit(1);
        }
        println!(
            "{}: PASS ({}/{} probes held: {} doctored inputs rejected, {} {accepted_noun})",
            self.display,
            self.npass,
            self.n,
            self.nneg,
            self.n - self.nneg
        );
        exit(0);
    }
}

/// Write a probe fixture file, exact bytes.
pub fn write_fixture(path: &Path, bytes: &[u8]) {
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(path, bytes);
}

/// The processor class a frozen object is compiled for: the two words of
/// the compiler's flag set that select the core and its floating-point
/// unit. Every other flag is shared between the classes, which each gate's
/// selfcheck asserts over its two flag sets (`class_flag_sets_tie`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Class {
    /// The Cortex-M7 with the double-precision FPv5: the objects of every
    /// row of that class, the flag set of record.
    CortexM7,
    /// The Cortex-M33 with the single-precision FPv5: the objects of every
    /// row of that class, the same flag set apart from the core and FPU words.
    CortexM33,
}

impl Class {
    /// The two flag words that select this class.
    pub fn cpu_words(self) -> [&'static str; 2] {
        match self {
            Class::CortexM7 => ["-mcpu=cortex-m7", "-mfpu=fpv5-d16"],
            Class::CortexM33 => ["-mcpu=cortex-m33", "-mfpu=fpv5-sp-d16"],
        }
    }
}

/// Two flag sets are one list apart from the class's two words: the same
/// length, the Cortex-M7 words in the first where the Cortex-M33 words sit
/// in the second, every other position equal, and both class words
/// present. The complaint names the first position that breaks the rule.
pub fn class_flag_sets_tie(cortex_m7: &[&str], cortex_m33: &[&str]) -> Result<(), String> {
    if cortex_m7.len() != cortex_m33.len() {
        return Err(format!("the flag sets differ in length ({} and {})", cortex_m7.len(), cortex_m33.len()));
    }
    let w7 = Class::CortexM7.cpu_words();
    let w33 = Class::CortexM33.cpu_words();
    let mut class_positions = 0usize;
    for (i, (a, b)) in cortex_m7.iter().zip(cortex_m33.iter()).enumerate() {
        if let Some(k) = w7.iter().position(|w| w == a) {
            if *b != w33[k] {
                return Err(format!("position {i}: {a} pairs with {b}, not with {}", w33[k]));
            }
            class_positions += 1;
        } else if a != b {
            return Err(format!("position {i}: {a} and {b} differ outside the class's two words"));
        }
    }
    if class_positions != 2 {
        return Err(format!("the Cortex-M7 flag set carries {class_positions} of its two class words"));
    }
    Ok(())
}
