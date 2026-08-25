//! The Lamella RHU kernel: boot into `kmain`, bring the scheduler
//! online, prove the machine with selftests, and arm the panic/fault
//! funnel. The whole kernel lives in this lib so one source tree feeds
//! both artifacts: the gate binary (rlib into `src/main.rs`) and the
//! payload-hosting staticlib (external `ld -T` merge with a frozen ILC
//! object).

#![no_std]

#[macro_use]
mod console;
mod api;
mod board;
mod boot;
mod critical;
mod funnel;
#[cfg(feature = "payload")]
mod payload;
mod sched;
mod semihost;

use core::ptr::{addr_of, addr_of_mut, read_volatile, write_volatile};

/// Compiled-in image flavor, stated on the boot line: the artifact
/// itself says what it is, and the gate asserts it per case.
const FLAVOR: &str = if cfg!(feature = "test-panic") {
    "test-panic"
} else if cfg!(feature = "test-fault") {
    "test-fault"
} else if cfg!(feature = "sched-smoke") {
    "sched-smoke"
} else if cfg!(feature = "api-smoke") {
    "api-smoke"
} else if cfg!(feature = "payload") {
    "payload"
} else {
    "boot"
};

/// Lands in .data -- proves the Reset LMA-to-VMA copy ran.
static mut DATA_CANARY: u32 = 0xDA7A_C0DE;
/// Lands in .bss -- proves the Reset zero-fill ran.
static mut BSS_CANARY: u32 = 0;

static F32_A: f32 = 2.5;
static F32_B: f32 = 1.5;
static F64_A: f64 = 3.5;
static F64_B: f64 = 1.5;

const SCB_CPUID: *const u32 = 0xE000_ED00 as *const u32;
const SCB_VTOR: *const u32 = 0xE000_ED08 as *const u32;

#[unsafe(no_mangle)]
pub extern "C" fn kmain() -> ! {
    let cpuid = unsafe { read_volatile(SCB_CPUID) };
    let vtor = unsafe { read_volatile(SCB_VTOR) };

    kprintln!(
        "rhu {} -- Lamella RHU: runtime-hosting unikernel for .NET on Cortex-M",
        env!("CARGO_PKG_VERSION")
    );
    kprintln!("[boot] {} | {} | flavor: {}", board::NAME, board::CONSOLE, FLAVOR);
    kprintln!(
        "[boot] VTOR={:#010x} CPUID={:#010x} ({} r{}p{})",
        vtor,
        cpuid,
        cpu_name(cpuid),
        (cpuid >> 20) & 0xF,
        cpuid & 0xF
    );

    if let Some(ck) = board::clock_readback() {
        kprintln!(
            "[boot] clock: {} | SYSCLK {} Hz (SWS={}, {}/{} x{} /{}) | flash WS {} | over-drive {}",
            ck.posture,
            ck.sysclk_hz,
            ck.sws,
            ck.src,
            ck.pllm,
            ck.plln,
            ck.pllp,
            ck.flash_ws,
            if ck.overdrive { "on" } else { "off" }
        );
        assert!(
            ck.sysclk_hz == board::SYSCLK_HZ,
            "clock posture mismatch: registers say {} Hz, build says {} Hz",
            ck.sysclk_hz,
            board::SYSCLK_HZ
        );
    }

    sched::init();
    let main_slot = sched::spawn("kmain", kthread_main, 0, 3, board::KMAIN_STACK_BYTES);
    kprintln!(
        "[sched] online: {} slots | SysTick {} Hz | BASEPRI ceiling {:#04x} | zero-latency band below {:#04x} never masked",
        sched::MAX_THREADS,
        board::TICK_HZ,
        critical::BASEPRI_CEILING,
        critical::BASEPRI_CEILING
    );
    sched::start(main_slot)
}

#[cfg_attr(
    any(feature = "test-panic", feature = "test-fault", feature = "payload"),
    allow(unreachable_code)
)]
extern "C" fn kthread_main(_: usize) {
    selftest();

    #[cfg(feature = "sched-smoke")]
    smoke::run();

    #[cfg(feature = "api-smoke")]
    api_smoke::run();

    #[cfg(feature = "payload")]
    payload::run();

    #[cfg(feature = "test-panic")]
    panic!("deliberate panic (flavor test-panic)");

    #[cfg(feature = "test-fault")]
    {
        kprintln!("[test-fault] executing udf #0 -- expect a UsageFault dump");
        unsafe { core::arch::asm!("udf #0") };
    }

    kprintln!("[base] ALL GOOD (boot + console + panic/fault funnel armed)");
    kprintln!("[sched] ALL GOOD (threads + timers + preemption online)");
    semihost::exit(0)
}

fn cpu_name(cpuid: u32) -> &'static str {
    match (cpuid >> 4) & 0xFFF {
        0xC27 => "Cortex-M7",
        0xC24 => "Cortex-M4",
        0xC23 => "Cortex-M3",
        0xD21 => "Cortex-M33",
        _ => "unrecognized part",
    }
}

/// A failed check panics, which routes through the funnel: a broken
/// mechanism produces a loud KERNEL PANIC transcript and exit code 1,
/// never a silent pass.
fn selftest() {
    let d = unsafe { read_volatile(addr_of!(DATA_CANARY)) };
    assert!(d == 0xDA7A_C0DE, ".data canary: {:#010x}", d);
    kprintln!("[selftest] .data copied from code-region LMA: OK");

    let b = unsafe { read_volatile(addr_of!(BSS_CANARY)) };
    assert!(b == 0, ".bss canary: {:#010x}", b);
    unsafe {
        write_volatile(addr_of_mut!(BSS_CANARY), 0xA5A5_5A5A);
        assert!(read_volatile(addr_of!(BSS_CANARY)) == 0xA5A5_5A5A, "RAM readback");
    }
    kprintln!("[selftest] .bss zeroed, RAM readback: OK");

    let control: u32;
    let sp: u32;
    unsafe {
        core::arch::asm!("mrs {}, CONTROL", out(reg) control, options(nomem, nostack));
        core::arch::asm!("mov {}, sp", out(reg) sp, options(nomem, nostack));
    }
    assert!(control & 2 != 0, "kmain not on PSP (CONTROL={:#x})", control);
    let (lo, hi) = sched::kstacks_window();
    assert!(
        (lo as u32..hi as u32).contains(&sp),
        "kmain SP {:#010x} outside the .kstacks pool",
        sp
    );
    kprintln!("[selftest] kmain on PSP inside the .kstacks pool: OK");

    let p = unsafe { read_volatile(&F32_A) * read_volatile(&F32_B) };
    assert!(p == 3.75f32, "f32 product wrong");
    kprintln!("[selftest] f32 multiply on hardware FPU: OK");

    let q = unsafe { read_volatile(&F64_A) * read_volatile(&F64_B) };
    assert!(q == 5.25f64, "f64 product wrong");
    kprintln!("[selftest] f64 multiply on soft-float path: OK");

    let t0 = sched::now_ms();
    sched::sleep_ms(25);
    let dt = sched::now_ms().wrapping_sub(t0);
    assert!(dt >= 25, "sleep_ms(25) returned after {} ms", dt);
    kprintln!("[selftest] SysTick advances + sleep(25 ms) never early: OK");
}


#[cfg(feature = "sched-smoke")]
mod smoke {
    use core::sync::atomic::{AtomicU32, Ordering};

    use crate::sched;

    static WOKE: AtomicU32 = AtomicU32::new(0);
    static SPINS: AtomicU32 = AtomicU32::new(0);
    static FP_RELEASE: AtomicU32 = AtomicU32::new(0);
    static FP_STOP: AtomicU32 = AtomicU32::new(0);
    static FP_DONE: AtomicU32 = AtomicU32::new(0);
    static FP_OUT: [AtomicU32; 2] = [AtomicU32::new(0), AtomicU32::new(0)];

    unsafe extern "C" {
        fn __rhu_fp_probe(release: *const u32, out: *const u32);
        fn __rhu_fp_clobber(stop: *const u32);
    }

    extern "C" fn worker(arg: usize) {
        sched::sleep_ms(30 * arg as u32);
        kprintln!("[smoke] worker {} awake", arg);
        WOKE.fetch_add(1, Ordering::Relaxed);
    }

    extern "C" fn spinner(_: usize) {
        loop {
            SPINS.fetch_add(1, Ordering::Relaxed);
            sched::yield_now();
        }
    }

    extern "C" fn fp_parker(_: usize) {
        unsafe { __rhu_fp_probe(FP_RELEASE.as_ptr(), FP_OUT[0].as_ptr()) };
        FP_DONE.store(1, Ordering::Release);
    }

    extern "C" fn fp_clobber(_: usize) {
        unsafe { __rhu_fp_clobber(FP_STOP.as_ptr()) };
    }

    fn busy_wait_ms(ms: u32) {
        let t0 = sched::now_ms();
        while sched::now_ms().wrapping_sub(t0) < ms {
            core::hint::spin_loop();
        }
    }

    pub fn run() {
        kprintln!("[smoke] 3 sleep-ordered workers + 1 equal-priority spinner");
        sched::spawn("w1", worker, 1, 3, 4 * 1024);
        sched::spawn("w2", worker, 2, 3, 4 * 1024);
        sched::spawn("w3", worker, 3, 3, 4 * 1024);
        sched::spawn("spin", spinner, 0, 3, 2 * 1024);

        sched::sleep_ms(150);
        let woke = WOKE.load(Ordering::Relaxed);
        assert!(woke == 3, "workers woke: {}", woke);
        kprintln!("[smoke] PASS sleep-order (3 workers woke in order)");
        assert!(SPINS.load(Ordering::Relaxed) > 0, "spinner never ran");
        kprintln!("[smoke] PASS rr-preempt (spinner ran while kmain slept)");

        let s0 = SPINS.load(Ordering::Relaxed);
        busy_wait_ms(30);
        assert!(SPINS.load(Ordering::Relaxed) != s0, "no preemption during busy loop");
        kprintln!("[smoke] PASS tick-preempt (equal-priority peer advanced during busy loop)");

        sched::spawn("fppark", fp_parker, 0, 3, 4 * 1024);
        sched::spawn("fpclob", fp_clobber, 0, 3, 2 * 1024);
        busy_wait_ms(40);
        FP_RELEASE.store(1, Ordering::Release);
        let t0 = sched::now_ms();
        while FP_DONE.load(Ordering::Acquire) == 0 {
            assert!(
                sched::now_ms().wrapping_sub(t0) < 1000,
                "fp parker never finished"
            );
            sched::sleep_ms(5);
        }
        FP_STOP.store(1, Ordering::Release);
        let (a, b) = (
            FP_OUT[0].load(Ordering::Relaxed),
            FP_OUT[1].load(Ordering::Relaxed),
        );
        assert!(
            a == 0xAAAA_1111 && b == 0xBBBB_2222,
            "FP state lost across preemption: s16={:#010x} s17={:#010x}",
            a,
            b
        );
        kprintln!("[smoke] PASS fp-preserved (s16/s17 survived preemption beside a hostile peer)");
    }

    core::arch::global_asm!(
        r#"
    .fpu fpv4-sp-d16

    /* (release_ptr r0, out_ptr r1): arm this thread's FP context, park
       distinctive words in the callee-saved bank, spin until *release
       != 0 (tick preemption rotates equal-priority peers meanwhile),
       then publish s16/s17. One assembly body on purpose: the compiler
       must never be free to reuse the parked registers in between. */
    .section .text.__rhu_fp_probe, "ax", %progbits
    .global __rhu_fp_probe
    .type __rhu_fp_probe, %function
    .thumb_func
__rhu_fp_probe:
    movw  r2, #0x1111
    movt  r2, #0xAAAA
    vmov  s16, r2
    movw  r2, #0x2222
    movt  r2, #0xBBBB
    vmov  s17, r2
1:  ldr   r3, [r0]
    cmp   r3, #0
    beq   1b
    vmov  r2, s16
    str   r2, [r1, #0]
    vmov  r2, s17
    str   r2, [r1, #4]
    bx    lr
    .size __rhu_fp_probe, . - __rhu_fp_probe

    /* (stop_ptr r0): repeatedly dirty s16/s17 until *stop != 0. */
    .section .text.__rhu_fp_clobber, "ax", %progbits
    .global __rhu_fp_clobber
    .type __rhu_fp_clobber, %function
    .thumb_func
__rhu_fp_clobber:
    movw  r2, #0xDEAD
    movt  r2, #0xDEAD
1:  vmov  s16, r2
    vmov  s17, r2
    ldr   r3, [r0]
    cmp   r3, #0
    beq   1b
    bx    lr
    .size __rhu_fp_clobber, . - __rhu_fp_clobber
"#
    );
}


#[cfg(feature = "api-smoke")]
mod api_smoke {
    use core::ptr::{addr_of, addr_of_mut, read_volatile, write_volatile};
    use core::sync::atomic::{AtomicU32, Ordering};

    use crate::critical::with_cs;
    use crate::{api, board, sched};

    fn busy_wait_ms(ms: u32) {
        let t0 = sched::now_ms();
        while sched::now_ms().wrapping_sub(t0) < ms {
            core::hint::spin_loop();
        }
    }

    /// Spin until the SysTick counter sits in the last eighth of its
    /// tick. The never-early checks (1, 2, 4) take their base timestamp
    /// from HERE so their negative controls red structurally, never by
    /// arm-phase luck: a floored deadline conversion is early by most
    /// of a millisecond from this position, and an ignored pending tick
    /// reads backward across the wrap -- while the correct paths are
    /// position-independent. (Discovered live: the check-4 floored
    /// control PASSED on its first pre-hardening run because the arm
    /// happened to sit early in its tick; the positioning below
    /// forces the case instead of hoping for it.)
    fn late_in_tick() {
        const SYST_CVR: *const u32 = 0xE000_E018 as *const u32;
        const LATE: u32 = (board::SYSCLK_HZ / board::TICK_HZ) / 8;
        unsafe { while read_volatile(SYST_CVR) > LATE {} }
    }

    /// Spin until SysTick reloads once: CVR counts DOWN, so the first
    /// sample reading HIGHER than the last crossed a reload. Callable
    /// inside the kernel CS -- the crossing still latches
    /// ICSR.PENDSTSET even while the interrupt is masked.
    fn wait_wrap() {
        const SYST_CVR: *const u32 = 0xE000_E018 as *const u32;
        let mut last = unsafe { read_volatile(SYST_CVR) };
        loop {
            let c = unsafe { read_volatile(SYST_CVR) };
            if c > last {
                return;
            }
            last = c;
        }
    }

    /// The eight API contract checks, in gate order, one boot,
    /// sequential; every marker is constant text the gate pins. Waiters
    /// and workers run LESS urgent than this thread (prio 4 vs 3), so the
    /// conditional-tick policy (a wake pends a switch only when one is
    /// owed) makes "readied but not run" a deterministic observable, not
    /// a race.
    pub fn run() {
        check_monotonic();
        check_sleep();
        check_eagain();
        check_timeout();
        check_wake();
        check_mutex();
        check_quiescent();
    }

    fn check_monotonic() {
        kprintln!("[api] ns-monotonic: 5000 paired reads + cs wrap + saturated-pend windows armed");
        let mut prev = 0u64;
        for _ in 0..5000 {
            let a = api::rhu_now_ns();
            let b = api::rhu_now_ns();
            assert!(a > prev && b > a, "ns clock failed to advance at thread level");
            prev = b;
        }
        const SYST_CVR: *const u32 = 0xE000_E018 as *const u32;
        let (t_a, t_b) = with_cs(|| unsafe {
            late_in_tick();
            let t_a = api::rhu_now_ns();
            let mut last = read_volatile(SYST_CVR);
            let mut wraps = 0u32;
            while wraps < 2 {
                let c = read_volatile(SYST_CVR);
                if c > last {
                    wraps += 1;
                }
                last = c;
            }
            let t_b = api::rhu_now_ns();
            (t_a, t_b)
        });
        assert!(t_b > t_a, "ns clock stepped backward across a masked tick wrap");
        let (t_c, t_d) = with_cs(|| {
            wait_wrap();
            late_in_tick();
            let t_c = api::rhu_now_ns();
            wait_wrap();
            let t_d = api::rhu_now_ns();
            (t_c, t_d)
        });
        assert!(t_d > t_c, "ns clock stepped backward across a saturated pend window");
        kprintln!("[api] PASS ns-monotonic (5000 paired reads + cs wrap + saturated pend windows)");
    }

    fn check_sleep() {
        kprintln!("[api] sleep-ns-floor: 25 ms request armed");
        late_in_tick();
        let t0 = api::rhu_now_ns();
        api::rhu_sleep_ns(25_000_000);
        let dt = api::rhu_now_ns() - t0;
        assert!(dt >= 25_000_000, "sleep_ns(25 ms) returned early");
        kprintln!("[api] PASS sleep-ns-floor (25 ms request: never early)");
    }

    fn check_eagain() {
        kprintln!("[api] futex-eagain: mismatch refusal armed");
        static WORD: AtomicU32 = AtomicU32::new(7);
        let s0 = sched::switch_count();
        let r = api::rhu_futex_wait(WORD.as_ptr(), 8, sched::DEADLINE_NONE);
        let ds = sched::switch_count().wrapping_sub(s0);
        assert!(r == sched::ERR_AGAIN, "mismatched wait returned {}", r);
        assert!(ds == 0, "mismatched wait slept: {} switches", ds);
        kprintln!("[api] PASS futex-eagain (changed value refused: no sleep, 0 switches)");
    }

    fn check_timeout() {
        kprintln!("[api] futex-timeout: 10 ms deadline armed");
        static WORD: AtomicU32 = AtomicU32::new(0);
        late_in_tick();
        let deadline = api::rhu_now_ns() + 10_000_000;
        let r = api::rhu_futex_wait(WORD.as_ptr(), 0, deadline);
        assert!(r == sched::ERR_TIMEDOUT, "timed wait returned {}", r);
        assert!(api::rhu_now_ns() >= deadline, "timeout fired before the deadline");
        kprintln!("[api] PASS futex-timeout (10 ms deadline honored: never early)");
    }

    static WAKE_WORD: AtomicU32 = AtomicU32::new(0);
    static RAN: [AtomicU32; 3] = [AtomicU32::new(0), AtomicU32::new(0), AtomicU32::new(0)];

    extern "C" fn waiter(i: usize) {
        let r = api::rhu_futex_wait(WAKE_WORD.as_ptr(), 0, sched::DEADLINE_NONE);
        assert!(r == 0, "waiter woke with {}", r);
        RAN[i].store(1, Ordering::Release);
    }

    fn check_wake() {
        kprintln!("[api] futex-wake: 3 waiters arming");
        let slots = [
            sched::spawn("fw0", waiter, 0, 4, 4 * 1024),
            sched::spawn("fw1", waiter, 1, 4, 4 * 1024),
            sched::spawn("fw2", waiter, 2, 4, 4 * 1024),
        ];
        while sched::probe_futex_waiting(WAKE_WORD.as_ptr()) != 3 {
            api::rhu_sleep_ns(1_000_000);
        }
        let woken = api::rhu_futex_wake(WAKE_WORD.as_ptr(), 1);
        assert!(woken == 1, "wake-one readied {}", woken);
        let readied = sched::probe_ready_among(&slots);
        let waiting = sched::probe_futex_waiting(WAKE_WORD.as_ptr());
        let ran: u32 = RAN.iter().map(|f| f.load(Ordering::Acquire)).sum();
        assert!(readied == 1, "wake-one: {} slots readied", readied);
        assert!(waiting == 2, "wake-one: {} slots still waiting", waiting);
        assert!(ran == 0, "wake-one: a waiter RAN before this thread yielded");
        kprintln!("[api] PASS futex-wake-one (3 blocked: readied exactly 1, ran 0)");

        let rest = api::rhu_futex_wake(WAKE_WORD.as_ptr(), u32::MAX);
        assert!(rest == 2, "wake-all readied {}", rest);
        assert!(sched::probe_ready_among(&slots) == 3, "wake-all left a waiter behind");
        while RAN.iter().map(|f| f.load(Ordering::Acquire)).sum::<u32>() != 3 {
            api::rhu_sleep_ns(1_000_000);
        }
        kprintln!("[api] PASS futex-wake-all (released remaining 2, count exact, all ran after)");
    }

    static LOCK: AtomicU32 = AtomicU32::new(0);
    static IN_CS: AtomicU32 = AtomicU32::new(0);
    static SLOWPATH: AtomicU32 = AtomicU32::new(0);
    static MUTEX_DONE: AtomicU32 = AtomicU32::new(0);
    static mut COUNTER: u32 = 0;

    fn lock() {
        if LOCK
            .compare_exchange(0, 1, Ordering::Acquire, Ordering::Relaxed)
            .is_ok()
        {
            return;
        }
        loop {
            if LOCK.swap(2, Ordering::Acquire) == 0 {
                return;
            }
            SLOWPATH.fetch_add(1, Ordering::Relaxed);
            let _ = api::rhu_futex_wait(LOCK.as_ptr(), 2, sched::DEADLINE_NONE);
        }
    }

    fn unlock() {
        if LOCK.swap(0, Ordering::Release) == 2 {
            api::rhu_futex_wake(LOCK.as_ptr(), 1);
        }
    }

    extern "C" fn mutex_worker(_: usize) {
        for i in 0..300u32 {
            lock();
            assert!(
                IN_CS
                    .compare_exchange(0, 1, Ordering::Acquire, Ordering::Relaxed)
                    .is_ok(),
                "mutex overlap: two threads inside the critical section"
            );
            unsafe {
                let c = read_volatile(addr_of!(COUNTER));
                write_volatile(addr_of_mut!(COUNTER), c + 1);
            }
            if i % 50 == 0 {
                busy_wait_ms(2);
            }
            IN_CS.store(0, Ordering::Release);
            unlock();
        }
        MUTEX_DONE.fetch_add(1, Ordering::Release);
        api::rhu_futex_wake(MUTEX_DONE.as_ptr(), u32::MAX);
    }

    fn check_mutex() {
        kprintln!("[api] futex-mutex: 4 workers x 300 armed");
        for name in ["mx0", "mx1", "mx2", "mx3"] {
            sched::spawn(name, mutex_worker, 0, 4, 4 * 1024);
        }
        loop {
            let d = MUTEX_DONE.load(Ordering::Acquire);
            if d == 4 {
                break;
            }
            let _ = api::rhu_futex_wait(MUTEX_DONE.as_ptr(), d, sched::DEADLINE_NONE);
        }
        let count = unsafe { read_volatile(addr_of!(COUNTER)) };
        assert!(count == 1200, "counter {} after 4 x 300", count);
        assert!(SLOWPATH.load(Ordering::Relaxed) >= 1, "slow path never fired");
        assert!(IN_CS.load(Ordering::Relaxed) == 0, "critical section flag stuck");
        kprintln!("[api] PASS futex-mutex (4x300 stress: counter 1200 exact, overlap-free, slow path fired)");
    }

    fn check_quiescent() {
        kprintln!("[api] tick-quiescent: solo busy 30 ms, switch delta armed");
        let t0 = sched::now_ms();
        let s0 = sched::switch_count();
        busy_wait_ms(30);
        let ds = sched::switch_count().wrapping_sub(s0);
        let dt = sched::now_ms().wrapping_sub(t0);
        assert!(dt >= 30, "tick clock failed to advance: {} ms", dt);
        assert!(ds == 0, "context switches during solo busy loop: {}", ds);
        kprintln!("[api] PASS tick-quiescent (solo busy 30 ms: 0 switches, clock advanced)");
    }
}
