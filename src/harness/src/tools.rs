//! Tool resolution and process machinery.

use std::env;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// Repo root: the nearest ancestor of the working directory that contains
/// src/kernel/Cargo.toml. Every emitted path is repo-relative from here.
pub fn find_repo_root() -> Result<PathBuf, String> {
    let mut dir = env::current_dir().map_err(|e| format!("cannot read working directory: {e}"))?;
    loop {
        if dir.join("src").join("kernel").join("Cargo.toml").is_file() {
            return Ok(dir);
        }
        if !dir.pop() {
            return Err(
                "not inside an RHU tree (no ancestor contains src/kernel/Cargo.toml)".to_string(),
            );
        }
    }
}

fn path_lookup(name: &str) -> Option<PathBuf> {
    let path_var = env::var_os("PATH")?;
    for dir in env::split_paths(&path_var) {
        for cand in [dir.join(name), dir.join(format!("{name}.exe"))] {
            if cand.is_file() {
                return Some(cand);
            }
        }
    }
    None
}

/// Deadline for a --version probe. Probes normally answer in well under a
/// second; a probe still running at 20 s is wedged, not slow.
const VERSION_PROBE_TIMEOUT_S: u64 = 20;

/// Bounded, file-captured stdout of a short tool QUERY (`--version`,
/// `rustc --print sysroot`, `rustc -vV`). The probe is part of the gate,
/// so it gets the same discipline as a case run: capture rides a FILE
/// handle, never a pipe (a pipe read returns only when every inherited
/// write handle closes -- a handle leaked to a concurrent process blocks
/// the read past the child's own death), and the wait is BOUNDED, with
/// one loud retry; a probe that hangs twice is a resolution FAILURE.
/// Compiler, cargo, and validator INVOCATIONS deliberately stay
/// unbounded: they ride file or inherited handles (no pipe exposure),
/// their durations legitimately vary. Only probes and the
/// external-linker merge get deadlines.
pub fn probe_stdout_bounded(
    program: &Path,
    args: &[&str],
    timeout: Duration,
) -> Result<Vec<u8>, String> {
    let shown = format!("{} {}", program.display(), args.join(" "));
    for attempt in 0..2u8 {
        let tmp = env::temp_dir().join(format!("rhu-gate-probe-{}.txt", std::process::id()));
        let out_f = File::create(&tmp)
            .map_err(|e| format!("cannot create probe capture file for '{shown}': {e}"))?;
        let mut child = match Command::new(program)
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::from(out_f))
            .stderr(Stdio::null())
            .spawn()
        {
            Ok(c) => c,
            Err(e) => {
                let _ = std::fs::remove_file(&tmp);
                return Err(format!("failed to spawn '{shown}': {e}"));
            }
        };
        let deadline = Instant::now() + timeout;
        let status = loop {
            match child.try_wait() {
                Ok(Some(s)) => break Some(s),
                Ok(None) => {
                    if Instant::now() >= deadline {
                        let _ = child.kill();
                        let _ = child.wait();
                        break None;
                    }
                    std::thread::sleep(Duration::from_millis(50));
                }
                Err(e) => {
                    let _ = std::fs::remove_file(&tmp);
                    return Err(format!("wait on '{shown}' failed: {e}"));
                }
            }
        };
        match status {
            None => {
                let _ = std::fs::remove_file(&tmp);
                println!(
                    "rhu-gate: probe '{shown}' hung and was killed at {} s{}",
                    timeout.as_secs(),
                    if attempt == 0 { " -- retrying once" } else { "" }
                );
            }
            Some(s) => {
                let bytes = std::fs::read(&tmp).unwrap_or_default();
                let _ = std::fs::remove_file(&tmp);
                if !s.success() {
                    return Err(format!("'{shown}' exited nonzero"));
                }
                return Ok(bytes);
            }
        }
    }
    Err(format!(
        "probe '{shown}' hung twice (killed at {} s each) -- resolution failure",
        timeout.as_secs()
    ))
}

/// First line of a command's --version output, CR-trimmed (recorded in the
/// results file exactly as the tool reports it). Rides the bounded probe;
/// a hang or spawn failure surfaces as None after the probe's own loud
/// line, and the caller's ladder message names the override variable.
pub fn version_first_line(program: &Path) -> Option<String> {
    let bytes = match probe_stdout_bounded(
        program,
        &["--version"],
        Duration::from_secs(VERSION_PROBE_TIMEOUT_S),
    ) {
        Ok(b) => b,
        Err(e) => {
            println!("rhu-gate: {e}");
            return None;
        }
    };
    let text = String::from_utf8_lossy(&bytes);
    let first = text
        .lines()
        .next()
        .unwrap_or("")
        .trim_end_matches('\r')
        .to_string();
    if first.is_empty() {
        None
    } else {
        Some(first)
    }
}

/// Where a resolved emulator came from: the `QEMU` variable, a PATH
/// lookup, or the untracked per-machine pointer file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QemuSource {
    Variable,
    Path,
    Pointer,
}

/// The emulator's pointer file, repo-relative: its first non-empty,
/// non-comment line names the `qemu-system-arm` executable on this machine
/// (the same untracked per-machine idiom as the compiler and archive
/// pointers under that directory).
pub const QEMU_POINTER: &str = "tools/toolchains/qemu.local";

/// The emulator's resolution ladder as a pure decision: the `QEMU` variable
/// when set and non-empty, else the PATH hit, else the pointer file's line,
/// else a refusal naming all three. No path is built in: a machine's own
/// layout reaches the runner only through the variable, PATH or the pointer.
pub fn qemu_ladder(
    variable: Option<&str>,
    on_path: Option<PathBuf>,
    pointer_line: Option<&str>,
    path_dirs: usize,
) -> Result<(PathBuf, QemuSource), String> {
    if let Some(v) = variable {
        if !v.trim().is_empty() {
            return Ok((PathBuf::from(v.trim()), QemuSource::Variable));
        }
    }
    if let Some(p) = on_path {
        return Ok((p, QemuSource::Path));
    }
    if let Some(l) = pointer_line {
        if !l.trim().is_empty() {
            return Ok((PathBuf::from(l.trim()), QemuSource::Pointer));
        }
    }
    Err(format!(
        "QEMU not resolved: the QEMU variable is unset, qemu-system-arm is on none of PATH's {path_dirs} directories, and {QEMU_POINTER} is absent or empty -- set QEMU=<path to qemu-system-arm>, put it on PATH, or write that file (line 1 = the path)"
    ))
}

/// QEMU, resolved `QEMU` -> PATH -> the untracked per-machine pointer file,
/// then verified runnable by its version line. Fails loud naming every
/// place it looked; a candidate that does not answer is refused naming
/// where it came from.
pub fn resolve_qemu(repo: &Path) -> Result<(PathBuf, String), String> {
    let variable = env::var("QEMU").ok();
    let path_dirs = env::var_os("PATH").map(|p| env::split_paths(&p).count()).unwrap_or(0);
    let pointer_text = std::fs::read_to_string(repo.join(QEMU_POINTER)).ok();
    let pointer_line = pointer_text
        .as_deref()
        .and_then(|t| t.lines().map(str::trim).find(|l| !l.is_empty() && !l.starts_with('#')));
    let (qemu, source) = qemu_ladder(
        variable.as_deref(),
        path_lookup("qemu-system-arm"),
        pointer_line,
        path_dirs,
    )?;
    match version_first_line(&qemu) {
        Some(v) => Ok((qemu, v)),
        None => {
            let from = match source {
                QemuSource::Variable => "the QEMU variable",
                QemuSource::Path => "PATH",
                QemuSource::Pointer => QEMU_POINTER,
            };
            Err(format!(
                "QEMU at '{}' (from {from}) did not answer --version -- set QEMU=<path to qemu-system-arm>, or fix {QEMU_POINTER}",
                qemu.display()
            ))
        }
    }
}

/// A command-valued override split into words: whitespace separates words,
/// a double-quoted run is one word with its quotes removed (an executable
/// under a path with a space travels that way), no other escape exists, and
/// an unbalanced quote is refused. The rule for every `RHU_*` command
/// variable and for the first line of a pointer file that holds a command.
pub fn command_words(text: &str) -> Result<Vec<String>, String> {
    let mut words = Vec::new();
    let mut cur = String::new();
    let mut in_word = false;
    let mut quoted = false;
    for c in text.chars() {
        if quoted {
            if c == '"' {
                quoted = false;
            } else {
                cur.push(c);
            }
        } else if c == '"' {
            quoted = true;
            in_word = true;
        } else if c.is_whitespace() {
            if in_word {
                words.push(std::mem::take(&mut cur));
                in_word = false;
            }
        } else {
            cur.push(c);
            in_word = true;
        }
    }
    if quoted {
        return Err(format!("unbalanced double quote in the command '{text}'"));
    }
    if in_word {
        words.push(cur);
    }
    if words.is_empty() {
        return Err("empty command".to_string());
    }
    Ok(words)
}

/// Facts of the pinned Rust toolchain: version line, sysroot, host triple,
/// and the llvm-tools bin directory (the path travels with the pin).
pub struct RustcFacts {
    pub version: String,
    pub llvm_bin: PathBuf,
}

pub fn rustc_facts() -> Result<RustcFacts, String> {
    let version = version_first_line(Path::new("rustc"))
        .ok_or_else(|| "rustc not runnable (the pinned toolchain resolves via rustup and rust-toolchain.toml)".to_string())?;
    let sysroot_bytes = probe_stdout_bounded(
        Path::new("rustc"),
        &["--print", "sysroot"],
        Duration::from_secs(VERSION_PROBE_TIMEOUT_S),
    )?;
    let sysroot = String::from_utf8_lossy(&sysroot_bytes).trim().to_string();
    if sysroot.is_empty() {
        return Err("rustc --print sysroot reported nothing".to_string());
    }
    let vv = probe_stdout_bounded(
        Path::new("rustc"),
        &["-vV"],
        Duration::from_secs(VERSION_PROBE_TIMEOUT_S),
    )?;
    let host = String::from_utf8_lossy(&vv)
        .lines()
        .find_map(|l| l.strip_prefix("host: ").map(|h| h.trim_end_matches('\r').to_string()))
        .ok_or_else(|| "rustc -vV reported no host triple".to_string())?;
    let llvm_bin = PathBuf::from(&sysroot)
        .join("lib")
        .join("rustlib")
        .join(&host)
        .join("bin");
    Ok(RustcFacts { version, llvm_bin })
}

/// An llvm-tools binary from the pinned toolchain's component directory,
/// verified runnable (missing component fails loud, never skips).
pub fn llvm_tool(facts: &RustcFacts, name: &str) -> Result<PathBuf, String> {
    for cand in [facts.llvm_bin.join(name), facts.llvm_bin.join(format!("{name}.exe"))] {
        if cand.is_file() {
            return Ok(cand);
        }
    }
    Err(format!(
        "{name} not present under '{}' (llvm-tools component of the pinned toolchain)",
        facts.llvm_bin.display()
    ))
}

/// Execution profile for a QEMU case run. Parity is the default: it
/// inserts nothing, so a parity case's argv is exactly the plain QEMU
/// invocation. Deterministic inserts a
/// fixed icount contract -- fixed shift, sleep off -- making the
/// guest's timer-vs-execution interleaving a pure function of the image;
/// count-class pins are pinnable exact only there. The shift value is
/// part of every such pin's identity: changing it re-clocks every count
/// pin, a reviewed migration, never a tweak.
#[derive(Clone, Copy)]
pub enum ExecProfile {
    Parity,
    Deterministic,
}

impl ExecProfile {
    /// The argv this profile inserts immediately before `-kernel`.
    pub fn qemu_args(self) -> &'static [&'static str] {
        match self {
            ExecProfile::Parity => &[],
            ExecProfile::Deterministic => &["-icount", "shift=0,sleep=off"],
        }
    }
}

/// Outcome of a captured run: the observed exit code, with 124 recorded for
/// a run that was killed at its timeout (a hang is observed, never a pass).
pub struct RunOutcome {
    pub exit: i32,
    pub timed_out: bool,
}

/// Spawn `program args...` with stdout and stderr redirected to files by
/// direct handle, wait up to `timeout`, kill on expiry.
pub fn run_captured(
    program: &Path,
    args: &[&str],
    stdout_path: &Path,
    stderr_path: &Path,
    timeout: Duration,
) -> Result<RunOutcome, String> {
    let stdout = File::create(stdout_path)
        .map_err(|e| format!("cannot create {}: {e}", stdout_path.display()))?;
    let stderr = File::create(stderr_path)
        .map_err(|e| format!("cannot create {}: {e}", stderr_path.display()))?;
    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::from(stderr))
        .spawn()
        .map_err(|e| format!("failed to spawn {}: {e}", program.display()))?;
    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                return Ok(RunOutcome {
                    exit: status.code().unwrap_or(1),
                    timed_out: false,
                });
            }
            Ok(None) => {
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Ok(RunOutcome {
                        exit: 124,
                        timed_out: true,
                    });
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(e) => return Err(format!("wait on {} failed: {e}", program.display())),
        }
    }
}

/// Spawn a pre-configured command (the caller sets stdio -- file handles,
/// never pipes) and wait with a deadline: Ok(Some(status)) when the child
/// exits, Ok(None) when the deadline expired and the child was killed,
/// Err on spawn or wait failure. The external-linker merge rides this: a
/// linker can cross a VM boundary (for example a WSL-hosted GNU ld), and an
/// unbounded .status() on a wedged linker would park the gate exactly
/// like an unbounded probe.
pub fn status_bounded(
    cmd: &mut Command,
    timeout: Duration,
) -> Result<Option<std::process::ExitStatus>, String> {
    let mut child = cmd.spawn().map_err(|e| format!("failed to spawn: {e}"))?;
    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(s)) => return Ok(Some(s)),
            Ok(None) => {
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Ok(None);
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(e) => return Err(format!("wait failed: {e}")),
        }
    }
}

/// `cargo build --release [extra args]` in src/kernel with a crate-local
/// CARGO_TARGET_DIR (a machine-global redirect would send every checkout's
/// build into one shared tree -- the contention class one-owner chains
/// exist to prevent). Combined output goes to `log_path`; the caller
/// surfaces the log on failure.
pub fn cargo_build_kernel(repo: &Path, extra: &[&str], log_path: &Path) -> Result<bool, String> {
    let target_dir = repo.join("src").join("kernel").join("target");
    cargo_build_kernel_at(repo, &target_dir, extra, log_path)
}

/// The kernel crate built into a target dir of the caller's choosing: a
/// board's own staticlib beside the shared per-target one other gates
/// link against, never over it.
pub fn cargo_build_kernel_at(
    repo: &Path,
    target_dir: &Path,
    extra: &[&str],
    log_path: &Path,
) -> Result<bool, String> {
    let crate_dir = repo.join("src").join("kernel");
    let log = File::create(log_path).map_err(|e| format!("cannot create build log: {e}"))?;
    let log2 = log
        .try_clone()
        .map_err(|e| format!("cannot clone build log handle: {e}"))?;
    let remap = format!("--remap-path-prefix={}=/rhu", repo.display());
    let remap_m7 = format!("target.thumbv7em-none-eabihf.rustflags=['{remap}']");
    let remap_m33 = format!("target.thumbv8m.main-none-eabihf.rustflags=['{remap}']");
    let status = Command::new("cargo")
        .arg("build")
        .arg("--release")
        .arg("--config")
        .arg(&remap_m7)
        .arg("--config")
        .arg(&remap_m33)
        .args(extra)
        .current_dir(&crate_dir)
        .env("CARGO_TARGET_DIR", target_dir)
        .stdin(Stdio::null())
        .stdout(Stdio::from(log))
        .stderr(Stdio::from(log2))
        .status()
        .map_err(|e| format!("failed to spawn cargo: {e}"))?;
    Ok(status.success())
}

/// Print a file to stdout (used to surface build logs and echo transcripts;
/// the echo is never the comparison input -- every check reads the file).
pub fn echo_file(path: &Path, prefix: &str) {
    let mut bytes = Vec::new();
    if let Ok(mut f) = File::open(path) {
        let _ = f.read_to_end(&mut bytes);
    }
    let text = String::from_utf8_lossy(&bytes);
    for line in text.lines() {
        println!("{prefix}{}", line.trim_end_matches('\r'));
    }
}
