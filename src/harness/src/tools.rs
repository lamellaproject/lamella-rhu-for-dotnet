//! Tool resolution and process machinery.
//!
//! Every external tool resolves environment override first, then PATH, then
//! an untracked per-machine pointer file (never a path written into the
//! source) -- and a tool that resolves nowhere fails LOUD naming every place
//! it looked;
//! the runner never silently skips a missing tool. A command-valued
//! override splits into words on whitespace, and a double-quoted word keeps
//! its spaces. Captures are byte-true: a case's console stream goes to its file
//! through a direct handle, never through a text layer, so the wire's CR
//! bytes reach the transcript checks untouched. A run that outlives its
//! timeout is killed and recorded as exit 124, which never satisfies any
//! expected-exit check.


use std::env;
use std::fs::File;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
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

/// The limits of an instruction-counted run. Such a run is a fixed amount
/// of emulator work whatever the host's load: its wall time follows the
/// share of a processor the host gives the emulator, so the wall clock
/// alone cannot tell a run that works on a busy host from a run that
/// never ends. The run is judged by the processor time the host has
/// accounted to the emulator.
#[derive(Clone, Copy)]
pub struct Limits {
    /// The longest stretch of wall time a live run may go without working.
    pub deadline: Duration,
    /// The most processor time the run may use.
    pub budget: Duration,
}

/// The rule that stopped a run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stop {
    /// The emulator's processor time reached the budget: a guest that
    /// works without end, or a guest that waits without end in an
    /// emulator that keeps a thread of its own busy meanwhile.
    Work,
    /// A whole deadline passed in which the emulator did no work: an
    /// emulator that stands still, its guest waiting for an event that
    /// does not come.
    NoWork,
    /// The host gives no reading of the emulator's processor time, and
    /// the wall clock passed the deadline.
    Deadline,
}

/// The judge of a run in progress: a pure function of the wall time since
/// the run began and of the host's reading of the emulator's processor
/// time, fed at every poll. A window of wall time opens at the start and
/// opens again whenever the reading has advanced by one sixtieth of the
/// deadline since it opened; a window that reaches the deadline's length
/// stops the run. With no reading the first window stays open, so the
/// wall clock alone stops the run at the deadline. A launcher standing in
/// front of the emulator reads as a process that does no work, and its
/// run is judged the same way.
pub struct Watch {
    limits: Limits,
    opened_wall: Duration,
    opened_spent: Duration,
    read: bool,
}

impl Watch {
    pub fn new(limits: Limits) -> Watch {
        Watch {
            limits,
            opened_wall: Duration::ZERO,
            opened_spent: Duration::ZERO,
            read: false,
        }
    }

    /// One poll: the wall time since the run began and the host's reading,
    /// when it gave one. The answer is the rule that stops the run, or
    /// `None` while the run may go on.
    pub fn observe(&mut self, wall: Duration, spent: Option<Duration>) -> Option<Stop> {
        if let Some(spent) = spent {
            self.read = true;
            if spent >= self.limits.budget {
                return Some(Stop::Work);
            }
            if spent.saturating_sub(self.opened_spent) >= self.limits.deadline / 60 {
                self.opened_wall = wall;
                self.opened_spent = spent;
            }
        }
        if wall.saturating_sub(self.opened_wall) >= self.limits.deadline {
            return Some(if self.read { Stop::NoWork } else { Stop::Deadline });
        }
        None
    }
}

/// The processor time a Linux `stat` line states: its fourteenth and
/// fifteenth fields, the user and kernel time in clock ticks. The fields
/// are counted from the end of the process name, the one field that may
/// hold spaces and parentheses; the name ends at the line's last closing
/// parenthesis.
pub fn stat_line_time(line: &str, ticks_per_second: u64) -> Option<Duration> {
    if ticks_per_second == 0 {
        return None;
    }
    let after_name = &line[line.rfind(')')? + 1..];
    let mut fields = after_name.split_ascii_whitespace();
    let user: u64 = fields.nth(11)?.parse().ok()?;
    let kernel: u64 = fields.next()?.parse().ok()?;
    let ticks = u128::from(user.checked_add(kernel)?);
    let nanos = ticks * 1_000_000_000 / u128::from(ticks_per_second);
    Some(Duration::from_nanos(u64::try_from(nanos).ok()?))
}

#[cfg(windows)]
mod host {
    use std::os::windows::io::AsRawHandle;
    use std::process::Child;
    use std::time::Duration;

    /// A count of 100 ns steps in two halves, the layout the host writes.
    #[repr(C)]
    #[derive(Clone, Copy, Default)]
    struct Steps {
        low: u32,
        high: u32,
    }

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetProcessTimes(
            process: *mut core::ffi::c_void,
            creation: *mut Steps,
            exit: *mut Steps,
            kernel: *mut Steps,
            user: *mut Steps,
        ) -> i32;
    }

    pub fn processor_time(child: &Child) -> Option<Duration> {
        let mut creation = Steps::default();
        let mut exit = Steps::default();
        let mut kernel = Steps::default();
        let mut user = Steps::default();
        // SAFETY: the handle is the child's own, held open by the standard
        // library for the child's life; the four pointers name live values
        // of the layout the call writes.
        let answered = unsafe {
            GetProcessTimes(child.as_raw_handle(), &mut creation, &mut exit, &mut kernel, &mut user)
        };
        if answered == 0 {
            return None;
        }
        let count = |s: Steps| (u64::from(s.high) << 32) | u64::from(s.low);
        Some(Duration::from_nanos(count(kernel).saturating_add(count(user)).saturating_mul(100)))
    }

    /// The handle answers after the child's exit: the reading at the end.
    pub fn processor_time_at_end(child: &Child) -> Option<Duration> {
        processor_time(child)
    }
}

#[cfg(target_os = "linux")]
mod host {
    use std::process::Child;
    use std::time::Duration;

    /// The clock tick's name in the host's configuration.
    const CLOCK_TICK: core::ffi::c_int = 2;

    unsafe extern "C" {
        fn sysconf(name: core::ffi::c_int) -> core::ffi::c_long;
    }

    pub fn processor_time(child: &Child) -> Option<Duration> {
        let line = std::fs::read_to_string(format!("/proc/{}/stat", child.id())).ok()?;
        // SAFETY: the call reads one value of the host's configuration and
        // touches no memory of this program's.
        let tick = unsafe { sysconf(CLOCK_TICK) };
        super::stat_line_time(&line, u64::try_from(tick).ok()?)
    }

    /// The process's entry is gone once the child is reaped: the last
    /// poll's reading stands.
    pub fn processor_time_at_end(_child: &Child) -> Option<Duration> {
        None
    }
}

#[cfg(not(any(windows, target_os = "linux")))]
mod host {
    use std::process::Child;
    use std::time::Duration;

    pub fn processor_time(_child: &Child) -> Option<Duration> {
        None
    }

    pub fn processor_time_at_end(_child: &Child) -> Option<Duration> {
        None
    }
}

/// The host's reading of the processor time a child has used, kernel and
/// user together, or `None` on a host the runner takes no reading from.
/// Windows answers through `GetProcessTimes` on the child's own handle;
/// Linux through the process's `stat` line under `/proc`, whose time
/// fields count clock ticks of the length `sysconf` states. The crate
/// takes no dependency, so the two foreign functions are declared here.
pub fn processor_time(child: &std::process::Child) -> Option<Duration> {
    host::processor_time(child)
}

/// The outcome of an instruction-counted run.
pub struct CountedRun {
    /// The observed exit code; 124 for a run the runner stopped.
    pub exit: i32,
    /// The rule that stopped the run, when one did.
    pub stop: Option<Stop>,
    /// The last reading of the emulator's processor time.
    pub spent: Option<Duration>,
    /// The run ended by itself after more wall time than its deadline.
    pub outlived: bool,
    /// The run ended by itself with more than half of its budget spent.
    pub past_half: bool,
    /// The run's budget of processor time, so the record carries the
    /// numbers beside the reading.
    pub budget: Duration,
}

/// `run_captured` for an instruction-counted run: the same spawn and the
/// same byte-true capture, the run judged at every poll by `Watch` and
/// stopped by the rule it names. A stopped run is recorded as exit 124,
/// which never satisfies an expected-exit check of a run that must end.
pub fn run_counted(
    program: &Path,
    args: &[&str],
    stdout_path: &Path,
    stderr_path: &Path,
    limits: Limits,
) -> Result<CountedRun, String> {
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
    let began = Instant::now();
    let mut watch = Watch::new(limits);
    let mut spent: Option<Duration> = None;
    loop {
        if let Some(reading) = processor_time(&child) {
            spent = Some(reading);
        }
        match child.try_wait() {
            Ok(Some(status)) => {
                if let Some(reading) = host::processor_time_at_end(&child) {
                    spent = Some(reading);
                }
                return Ok(CountedRun {
                    exit: status.code().unwrap_or(1),
                    stop: None,
                    spent,
                    outlived: began.elapsed() > limits.deadline,
                    past_half: spent.is_some_and(|s| s > limits.budget / 2),
                    budget: limits.budget,
                });
            }
            Ok(None) => {
                if let Some(stop) = watch.observe(began.elapsed(), spent) {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Ok(CountedRun {
                        exit: 124,
                        stop: Some(stop),
                        spent,
                        outlived: false,
                        past_half: false,
                        budget: limits.budget,
                    });
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(e) => return Err(format!("wait on {} failed: {e}", program.display())),
        }
    }
}

/// The lines an instruction-counted run is reported with: the rule that
/// stopped it, or what its ending says of the host and of its budget. The
/// two notes are printed only when their condition holds. The budget's rule: every line
/// carries the numbers -- the processor time spent, the budget, the spent
/// share of it and the host's unit -- so the record holds the margin.
pub fn say_counted(run: &CountedRun) {
    let numbers = counted_numbers(run);
    match run.stop {
        Some(Stop::Work) => println!(
            "  (run stopped: its budget of processor time is spent; {numbers}; exit recorded as 124 -- a hang is observed, never a pass)"
        ),
        Some(Stop::NoWork) => println!(
            "  (run stopped: a whole deadline passed in which the emulator did no work; {numbers}; exit recorded as 124 -- a hang is observed, never a pass)"
        ),
        Some(Stop::Deadline) => println!(
            "  (run killed at its timeout; {numbers}; exit recorded as 124 -- a hang is observed, never a pass)"
        ),
        None => {
            if run.outlived {
                println!("  (the run outlived its wall deadline while the emulator kept working: the host is busy)");
            }
            if run.past_half {
                println!("  (the run spent more than half of its budget of processor time; {numbers})");
            } else {
                println!("  (the run's processor time: {numbers})");
            }
        }
    }
}

/// The numbers of a counted run as words -- the processor time spent
/// (the host's last reading), the budget, the spent share of it, and the
/// host's unit of emulator work the budget followed.
fn counted_numbers(run: &CountedRun) -> String {
    let unit = match UNIT.get() {
        Some(Some(u)) => format!("{} ms", u.as_millis()),
        _ => "not measured".to_string(),
    };
    match run.spent {
        Some(s) => format!(
            "spent {} ms of the budget {} ms ({:.2} of it); the host's unit {}",
            s.as_millis(),
            run.budget.as_millis(),
            s.as_secs_f64() / run.budget.as_secs_f64().max(0.001),
            unit
        ),
        None => format!("no reading of the processor time; the budget {} ms; the host's unit {}", run.budget.as_millis(), unit),
    }
}

/// A generated guest of the mps2-an500 machine: a raw image the emulator
/// loads at the machine's reset base. Its first eight bytes are the vector
/// table -- the initial stack pointer 0x20001000, then the reset handler's
/// address 8 with the Thumb bit set -- and the handler follows, in 16-bit
/// Thumb instructions stored low byte first.
#[derive(Clone, Copy)]
pub enum Guest {
    /// Counts the given turns of a two-instruction loop down to zero, then
    /// leaves through the machine's semihosting door with status 0:
    /// `ldr r0, [pc, #12]` (the turns), `subs r0, #1`, `bne` back to the
    /// `subs`, `movs r0, #0x18` (the exit call), `ldr r1, [pc, #8]` (the
    /// reason 0x20026, the program ended), `bkpt 0xAB`, `b .`, `nop`, then
    /// the two literal words. 32 bytes; the turns must not be zero.
    Work(u32),
    /// Never leaves its loop: `b .`, `nop`. 12 bytes.
    Spin,
    /// Waits for an interrupt, and nothing in the machine is set up to
    /// raise one: `wfi`, `b` back to the `wfi`. 12 bytes. An emulator that
    /// rests while its guest waits does no work; one that keeps a thread
    /// of its own busy meanwhile reads as working.
    Wait,
}

/// The bytes of a generated guest.
pub fn guest_image(guest: Guest) -> Vec<u8> {
    const WORK: [u16; 8] = [0x4803, 0x3801, 0xD1FD, 0x2018, 0x4902, 0xBEAB, 0xE7FE, 0xBF00];
    const SPIN: [u16; 2] = [0xE7FE, 0xBF00];
    const WAIT: [u16; 2] = [0xBF30, 0xE7FD];
    let (code, literals): (&[u16], Vec<u32>) = match guest {
        Guest::Work(turns) => (&WORK[..], vec![turns, 0x0002_0026]),
        Guest::Spin => (&SPIN[..], Vec::new()),
        Guest::Wait => (&WAIT[..], Vec::new()),
    };
    let mut image = Vec::new();
    for word in [0x2000_1000u32, 0x0000_0009] {
        image.extend_from_slice(&word.to_le_bytes());
    }
    for half in code {
        image.extend_from_slice(&half.to_le_bytes());
    }
    for word in literals {
        image.extend_from_slice(&word.to_le_bytes());
    }
    image
}

/// Run a generated guest on the mps2-an500 machine, on the
/// instruction-counted profile, under the given limits. The image is
/// written into `scratch` as `<name>.bin`, its transcript beside it as
/// `<name>.raw`.
pub fn run_guest(
    qemu: &Path,
    scratch: &Path,
    name: &str,
    guest: Guest,
    limits: Limits,
) -> Result<CountedRun, String> {
    let image = scratch.join(format!("{name}.bin"));
    std::fs::write(&image, guest_image(guest))
        .map_err(|e| format!("cannot write {}: {e}", image.display()))?;
    let image_arg = image.to_string_lossy().into_owned();
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
    args.extend_from_slice(ExecProfile::Deterministic.qemu_args());
    args.push("-kernel");
    args.push(&image_arg);
    run_counted(
        qemu,
        &args,
        &scratch.join(format!("{name}.raw")),
        &scratch.join(format!("{name}.stderr")),
        limits,
    )
}

/// The turns of the unit's guest: twice this many instructions are the
/// unit of emulator work.
pub const UNIT_TURNS: u32 = 0x1000_0000;

/// The least reading accepted as the unit. An emulator that ran the
/// unit's guest used many times this; a smaller reading belongs to a
/// process that did not do the work itself.
const UNIT_LEAST: Duration = Duration::from_millis(100);

static UNIT: std::sync::OnceLock<Option<Duration>> = std::sync::OnceLock::new();

/// The host's unit of emulator work: the processor time the emulator
/// uses on this host, as the host reads it, to run the unit's guest on the
/// instruction-counted profile and leave. It is measured once per process,
/// before the first instruction-counted case, under `floor` on both
/// clocks, and one line reports it. The measure is refused -- `None`, the
/// line saying why -- when the guest did not leave through its door with
/// status 0, when the host gives no reading, or when the reading is under
/// `UNIT_LEAST`.
pub fn host_unit(qemu: &Path, scratch: &Path, floor: Duration) -> Option<Duration> {
    *UNIT.get_or_init(|| match measure_unit(qemu, scratch, floor) {
        Ok(unit) => {
            println!("== the host's unit of emulator work is measured: the budgets of the instruction-counted runs follow it ==");
            Some(unit)
        }
        Err(why) => {
            println!("== the host's unit of emulator work is NOT measured ({why}): the budgets of the instruction-counted runs stay at their floors ==");
            None
        }
    })
}

fn measure_unit(qemu: &Path, scratch: &Path, floor: Duration) -> Result<Duration, String> {
    let limits = Limits { deadline: floor, budget: floor };
    let run = run_guest(qemu, scratch, "unit", Guest::Work(UNIT_TURNS), limits)?;
    if run.stop.is_some() {
        return Err("the unit's guest was stopped before it left".to_string());
    }
    if run.exit != 0 {
        return Err(format!("the unit's guest left with status {}", run.exit));
    }
    match run.spent {
        None => Err("this host gives no reading of the emulator's processor time".to_string()),
        Some(spent) if spent < UNIT_LEAST => {
            Err("the reading does not follow the emulator's work".to_string())
        }
        Some(spent) => Ok(spent),
    }
}

/// The budget of an instruction-counted case: the case's work, in units,
/// times the host's unit, and never under the case's floor. With no unit
/// the floor stands.
pub fn budget(floor: Duration, work: u32, unit: Option<Duration>) -> Duration {
    match unit {
        Some(unit) => floor.max(unit.saturating_mul(work)),
        None => floor,
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
    cargo_build_kernel_env(repo, target_dir, extra, &[], log_path)
}

/// `cargo_build_kernel_at` with environment words on the build: the
/// environment route of the console's channels and the log's level
/// (`RHU_LOG_LEVEL`, `RHU_LOG_CHANNEL`, `RHU_CONSOLE_CHANNEL`, the pin
/// overrides `RHU_UART<n>_TX` and `RHU_UART<n>_RX`), read by the crate's
/// build script into the image's link-time words. The build script re-runs
/// when such a word changes, so a routed image is built in a target dir of
/// its own, never in the shared per-target tree the gates' plain images
/// share -- their identities would move with every route.
pub fn cargo_build_kernel_env(
    repo: &Path,
    target_dir: &Path,
    extra: &[&str],
    env: &[(&str, &str)],
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
        .envs(env.iter().map(|(k, v)| (*k, *v)))
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

/// A free TCP port on the loopback interface: bound at 0, read, released.
/// The emulator's chardev binds it a moment later; a gate runs one case at
/// a time, and a port taken in between fails that run loudly (the chardev
/// cannot bind), never silently.
pub fn free_port() -> Result<u16, String> {
    let listener = TcpListener::bind(("127.0.0.1", 0)).map_err(|e| format!("cannot bind a loopback port: {e}"))?;
    let port = listener
        .local_addr()
        .map_err(|e| format!("cannot read the bound port: {e}"))?
        .port();
    drop(listener);
    Ok(port)
}

/// QEMU's second serial chardev for an api image's second UART: a TCP
/// server on the loopback interface the wire's echo connects to; the run
/// does not wait for the client, and the bytes go out without the Nagle
/// delay. The native Windows build of the emulator listens on the host's
/// own loopback interface, so the echo reaches it directly.
pub fn echo_chardev(port: u16) -> String {
    // wait=on: the emulator starts the machine once the gate's echo peer is connected, so the first bytes the
    // ladder sends reach the peer.
    format!("tcp:127.0.0.1:{port},server=on,wait=on,nodelay=on")
}

/// The wire's echo on an api image's second UART: a thread that connects
/// to the chardev with retries every 25 ms until it listens (or the
/// deadline passes, or the run ends first), then writes back every byte it
/// reads until the peer closes or the stop flag is raised -- a 250 ms read
/// timeout re-checks the flag. `finish` raises the flag, joins and answers
/// the bytes echoed. Spawned after the case's build and before its run, so
/// the deadline bounds the run alone.
pub struct Echo {
    stop: Arc<AtomicBool>,
    thread: JoinHandle<Result<u64, String>>,
}

pub fn spawn_echo(port: u16, deadline: Duration) -> Echo {
    let stop = Arc::new(AtomicBool::new(false));
    let flag = stop.clone();
    let thread = std::thread::spawn(move || echo_until_closed(port, deadline, &flag));
    Echo { stop, thread }
}

fn echo_until_closed(port: u16, deadline: Duration, stop: &AtomicBool) -> Result<u64, String> {
    let start = Instant::now();
    let mut stream = loop {
        if stop.load(Ordering::SeqCst) {
            // The run ended before the chardev listened (an early death):
            // nothing to echo, and nothing wrong with the echo.
            return Ok(0);
        }
        match TcpStream::connect(("127.0.0.1", port)) {
            Ok(s) => break s,
            Err(e) => {
                if start.elapsed() >= deadline {
                    return Err(format!(
                        "the wire's echo could not connect to the chardev on port {port} within {} s: {e}",
                        deadline.as_secs()
                    ));
                }
                std::thread::sleep(Duration::from_millis(25));
            }
        }
    };
    let _ = stream.set_nodelay(true);
    stream
        .set_read_timeout(Some(Duration::from_millis(250)))
        .map_err(|e| format!("the wire's echo cannot set its read timeout: {e}"))?;
    // A write back blocks only while the guest stops reading and the
    // socket's buffer is full: bounded, so a run that ends there ends
    // the thread too.
    stream
        .set_write_timeout(Some(Duration::from_secs(2)))
        .map_err(|e| format!("the wire's echo cannot set its write timeout: {e}"))?;
    let mut buf = [0u8; 4096];
    let mut echoed = 0u64;
    loop {
        match stream.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                if let Err(e) = stream.write_all(&buf[..n]) {
                    if stop.load(Ordering::SeqCst) {
                        break;
                    }
                    return Err(format!("the wire's echo could not write {n} bytes back: {e}"));
                }
                echoed += n as u64;
                if stop.load(Ordering::SeqCst) {
                    break;
                }
            }
            Err(e) if matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut) => {
                if stop.load(Ordering::SeqCst) {
                    break;
                }
            }
            // The peer is gone (the emulator exited): the echo's work is done.
            Err(_) => break,
        }
    }
    Ok(echoed)
}

impl Echo {
    /// Raise the stop flag, join the thread, answer the bytes echoed.
    pub fn finish(self) -> Result<u64, String> {
        self.stop.store(true, Ordering::SeqCst);
        match self.thread.join() {
            Ok(r) => r,
            Err(_) => Err("the wire's echo thread panicked".to_string()),
        }
    }
}
