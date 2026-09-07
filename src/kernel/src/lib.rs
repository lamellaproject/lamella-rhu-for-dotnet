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
mod arena;
mod board;
mod boot;
mod critical;
mod entropy;
mod funnel;
mod header;
mod kheap;
#[cfg(feature = "payload")]
mod payload;
mod registry;
mod sched;
mod semihost;

use core::ptr::{addr_of, addr_of_mut, read_volatile, write_volatile};

/// Compiled-in image flavor, stated on the boot line: the artifact
/// itself says what it is, and the gate asserts it per case.
const FLAVOR: &str = if cfg!(feature = "test-panic") {
    "test-panic"
} else if cfg!(feature = "test-fault") {
    "test-fault"
} else if cfg!(feature = "test-kheap-loud") {
    "test-kheap-loud"
} else if cfg!(feature = "test-tickcost") {
    "test-tickcost"
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

    arena::init();
    kheap::init();
    registry::init_module0();
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
    any(feature = "test-panic", feature = "test-fault", feature = "payload", feature = "test-kheap-loud", feature = "test-tickcost", feature = "api-smoke"),
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

    #[cfg(feature = "test-kheap-loud")]
    {
        kprintln!("[kheap-loud] corrupting a free block header, then allocating -- expect a KERNEL PANIC");
        let a = api::rhu_kheap_alloc(64, 8);
        assert!(!a.is_null(), "kheap-loud: seed alloc failed");
        let g = api::rhu_kheap_alloc(64, 8);
        assert!(!g.is_null(), "kheap-loud: guard alloc failed");
        api::rhu_kheap_free(a);
        kheap::probe_corrupt_first_free();
        let _ = api::rhu_kheap_alloc(64, 8);
        kprintln!("[kheap-loud] UNREACHABLE: the corrupt walk survived");
        semihost::exit(3)
    }

    #[cfg(feature = "test-tickcost")]
    {
        const W: u32 = 25_000_000;
        const N: u32 = 100_000;
        kprintln!("[tickcost] phase 1: solo busy window armed (W iterations, 2 insns each)");
        let ms0 = sched::now_ms();
        let sw0 = sched::switch_count();
        let t0 = api::rhu_now_ns();
        unsafe {
            core::arch::asm!(
                "1:",
                "subs {i}, {i}, #1",
                "bne 1b",
                i = inout(reg) W => _,
                options(nomem, nostack)
            );
        }
        let t1 = api::rhu_now_ns();
        let dsw = sched::switch_count().wrapping_sub(sw0);
        let dms = sched::now_ms().wrapping_sub(ms0);
        assert!(dsw == 0, "tickcost: switches during the solo busy window");
        kprintln!("[tickcost] p1 dns={} dms={} dsw={} W={}", t1 - t0, dms, dsw, W);
        kprintln!("[tickcost] phase 2: solo yield round-trips armed (N calls)");
        let sw1 = sched::switch_count();
        let t2 = api::rhu_now_ns();
        for _ in 0..N {
            api::rhu_thread_yield();
        }
        let t3 = api::rhu_now_ns();
        let dsw2 = sched::switch_count().wrapping_sub(sw1);
        assert!(dsw2 == N, "tickcost: yield round trips missed PendSV");
        kprintln!("[tickcost] p2 dns={} dsw={} N={}", t3 - t2, dsw2, N);
        kprintln!("[tickcost] done");
        semihost::exit(0)
    }

    kprintln!("[base] ALL GOOD (boot + console + panic/fault funnel armed)");
    kprintln!("[sched] ALL GOOD (threads + timers + preemption online)");
    #[cfg(feature = "api-smoke")]
    {
        kprintln!("[api] exit-door: handing the machine exit to rhu_exit(0)");
        api::rhu_exit(0);
    }
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
    use core::ffi::c_void;
    use core::ptr::{addr_of, addr_of_mut, read_volatile, write_volatile};
    use core::sync::atomic::{AtomicI32, AtomicU32, AtomicUsize, Ordering};

    use crate::critical::with_cs;
    use crate::{api, arena, board, kheap, registry, sched};

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

    /// The API contract checks, in gate order, one boot,
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
        check_arena_carve();
        check_arena_zero_birth();
        check_arena_rezero();
        check_arena_audit();
        check_arena_headroom();
        check_kheap_basic();
        check_kheap_coalesce();
        check_tls_blocks();
        check_tls_base_door();
        check_tls_dtors();
        check_thread_abi();
        check_thread_bounds();
        check_registry_basic();
        check_susp_pick();
        check_susp_ctx();
        check_susp_setctx();
        check_resume_urgent();
        check_act_refuse();
        check_act_deliver();
        check_act_coalesce();
        check_act_redeliver();
        check_act_eintr();
        check_act_sleep_rearm();
        check_act_lazy_fp();
        check_module_find();
        check_module_roots();
        check_console_pair();
        check_entropy();
        check_host_band();
        check_tid_wrap();
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


    /// Volatile full scan: true iff every byte of [p, p+len) reads
    /// zero. Bool-returning on purpose -- the zero-birth check proves
    /// this scanner refuses a nonzero byte before trusting it (panics
    /// cannot be caught in-kernel, so refusal probes cannot ride
    /// assert).
    fn scan_zero(p: *const u8, len: usize) -> bool {
        for i in 0..len {
            if unsafe { read_volatile(p.add(i)) } != 0 {
                return false;
            }
        }
        true
    }

    fn check_arena_carve() {
        kprintln!("[api] arena-carve: window + disjointness asserts armed");
        unsafe extern "C" {
            static __ekstacks: u32;
            static __stack_start: u32;
            static __sdata: u32;
            static __ebss: u32;
        }
        let (lo, hi, bump) = arena::probe_window();
        let eks = addr_of!(__ekstacks) as usize;
        let stk = addr_of!(__stack_start) as usize;
        let sd = addr_of!(__sdata) as usize;
        let eb = addr_of!(__ebss) as usize;
        assert!(lo >= eks, "arena window: lo below __ekstacks");
        assert!(hi <= stk, "arena window: hi above __stack_start");
        assert!(stk - hi >= 64 * 1024, "arena window: boot-stack reserve under 64K");
        assert!(lo < hi, "arena window: empty or negative");
        assert!(bump >= lo && bump <= hi, "arena window: bump outside the window");
        let (klo, khi) = sched::kstacks_window();
        assert!(hi.min(khi) <= lo.max(klo), "arena window overlaps .kstacks");
        assert!(hi.min(eb) <= lo.max(sd), "arena window overlaps the statics window");
        kprintln!("[api] PASS arena-carve (window bracketed, disjoint from kstacks + statics)");
    }

    fn check_arena_zero_birth() {
        kprintln!("[api] arena-zero-birth: poisoned-boot hand-out scan armed");
        let mut fixture = [0u8; 16];
        fixture[9] = 1;
        assert!(
            !scan_zero(fixture.as_ptr(), fixture.len()),
            "zero scanner accepted a one-nonzero-byte page"
        );
        let site = check_arena_zero_birth as *const () as usize as u32;
        let p1 = api::rhu_arena_alloc_at(4096, 4096, site);
        assert!(!p1.is_null(), "zero-birth: probe 1 alloc failed");
        assert!(!api::rhu_arena_alloc_at(32 * 1024, 8, site).is_null(), "zero-birth: filler 1 failed");
        let p2 = api::rhu_arena_alloc_at(4096, 4096, site);
        assert!(!p2.is_null(), "zero-birth: probe 2 alloc failed");
        assert!(!api::rhu_arena_alloc_at(64 * 1024, 8, site).is_null(), "zero-birth: filler 2 failed");
        let p3 = api::rhu_arena_alloc_at(4096, 4096, site);
        assert!(!p3.is_null(), "zero-birth: probe 3 alloc failed");
        for p in [p1, p2, p3] {
            assert!(scan_zero(p as *const u8, 4096), "zero-birth: a handed-out page reads nonzero");
        }
        kprintln!("[api] PASS arena-zero-birth (pages read all-zero through the boot poison)");
    }

    fn check_arena_rezero() {
        kprintln!("[api] arena-rezero: pattern + rezero + VA-burn probe armed");
        let site = check_arena_rezero as *const () as usize as u32;
        let q = api::rhu_arena_alloc_at(4096, 8, site);
        assert!(!q.is_null(), "rezero: alloc failed");
        for i in 0..4096 {
            unsafe { write_volatile(q.add(i), 0x5A) };
        }
        let h0 = api::rhu_arena_headroom();
        let (_, _, b0) = arena::probe_window();
        let r = api::rhu_arena_rezero(q, 4096);
        assert!(r == 0, "rezero returned nonzero on a handed-out range");
        assert!(scan_zero(q as *const u8, 4096), "rezero: the pattern survived");
        let h1 = api::rhu_arena_headroom();
        let (_, _, b1) = arena::probe_window();
        assert!(h0 == h1 && b0 == b1, "rezero burned VA (headroom or bump moved)");
        assert!(
            api::rhu_arena_rezero(core::ptr::null_mut(), 8) == sched::ERR_INVAL,
            "null rezero not refused"
        );
        let oob = (b1 + 64) as *mut u8;
        assert!(
            api::rhu_arena_rezero(oob, 8) == sched::ERR_INVAL,
            "never-handed-out rezero not refused"
        );
        let h2 = api::rhu_arena_headroom();
        let (_, _, b2) = arena::probe_window();
        assert!(h1 == h2 && b1 == b2, "a refused rezero touched state");
        kprintln!("[api] PASS arena-rezero (range re-zeroed in place: same address, zero VA burned, oob refused)");
    }

    fn check_arena_audit() {
        kprintln!("[api] arena-audit: two-site attribution armed");
        let site_a = check_arena_audit as *const () as usize as u32;
        let site_b = check_arena_carve as *const () as usize as u32;
        assert!(site_a != site_b, "audit: the two sites are not distinct");
        for b in [8u32, 16, 24] {
            assert!(!api::rhu_arena_alloc_at(b, 8, site_a).is_null(), "audit: site-A alloc failed");
        }
        for b in [32u32, 40] {
            assert!(!api::rhu_arena_alloc_at(b, 8, site_b).is_null(), "audit: site-B alloc failed");
        }
        let (ca, ba) = arena::probe_site(site_a);
        let (cb, bb) = arena::probe_site(site_b);
        assert!(ca == 3 && ba == 8 + 16 + 24, "audit: site-A row wrong");
        assert!(cb == 2 && bb == 32 + 40, "audit: site-B row wrong");
        kprintln!("[api] PASS arena-audit (two sites attributed exactly: rows, counts, bytes)");
    }

    fn check_arena_headroom() {
        kprintln!("[api] arena-headroom: exact-fit exhaustion armed");
        let missing = check_quiescent as *const () as usize as u32;
        assert!(
            arena::probe_site(missing) == (0, 0),
            "audit probe invented a row for a never-used site"
        );
        let site = check_arena_headroom as *const () as usize as u32;
        let h = api::rhu_arena_headroom();
        assert!(h != 0 && h % 8 == 0, "headroom not a positive 8-multiple");
        let p = api::rhu_arena_alloc_at(h, 8, site);
        assert!(!p.is_null(), "exact-fit alloc of the full headroom failed");
        assert!(api::rhu_arena_headroom() == 0, "headroom nonzero after the exact fit");
        let q = api::rhu_arena_alloc_at(8, 8, site);
        assert!(q.is_null(), "alloc past exhaustion was not refused NULL");
        assert!(api::rhu_arena_headroom() == 0, "a refused alloc changed headroom");
        kprintln!("[api] PASS arena-headroom (exact fit taken, further alloc refused NULL, headroom 0)");
    }


    /// Bounded poll: the C checks join their threads by observable
    /// state, never by luck; a stuck condition dies loud at 2 s.
    fn wait_until(mut cond: impl FnMut() -> bool, what: &str) {
        let t0 = sched::now_ms();
        while !cond() {
            assert!(sched::now_ms().wrapping_sub(t0) < 2000, "timed out waiting: {}", what);
            api::rhu_sleep_ns(1_000_000);
        }
    }

    /// Release a park word: store-then-wake, so a late arrival sees the
    /// changed value and refuses the wait instead of sleeping forever.
    fn release(word: &AtomicU32) {
        word.store(1, Ordering::Release);
        api::rhu_futex_wake(word.as_ptr(), u32::MAX);
    }

    fn check_kheap_basic() {
        kprintln!("[api] kheap-basic: alloc/free/reuse + mixed-align armed");
        let a = api::rhu_kheap_alloc(40, 8);
        let b = api::rhu_kheap_alloc(64, 16);
        let c = api::rhu_kheap_alloc(24, 64);
        assert!(!a.is_null() && !b.is_null() && !c.is_null(), "kheap-basic: an alloc failed");
        for (p, al) in [(a, 8usize), (b, 16), (c, 64)] {
            assert!(p as usize & (al - 1) == 0, "kheap-basic: alignment violated");
        }
        unsafe {
            write_volatile(a, 0x11u8);
            write_volatile(b, 0x22);
            write_volatile(c, 0x33);
            assert!(
                read_volatile(a) == 0x11 && read_volatile(b) == 0x22 && read_volatile(c) == 0x33,
                "kheap-basic: writes did not stick"
            );
        }
        api::rhu_kheap_free(a);
        let a2 = api::rhu_kheap_alloc(40, 8);
        assert!(a2 == a, "kheap-basic: exact-fit free not reused at the same address");
        api::rhu_kheap_free(a2);
        api::rhu_kheap_free(b);
        api::rhu_kheap_free(c);
        kprintln!("[api] PASS kheap-basic (freed block reused at the same address, alignment honored)");
    }

    fn check_kheap_coalesce() {
        kprintln!("[api] kheap-coalesce: guarded three-block mixed-order free armed");
        let x = api::rhu_kheap_alloc(48, 8);
        let y = api::rhu_kheap_alloc(80, 8);
        let z = api::rhu_kheap_alloc(112, 8);
        let g = api::rhu_kheap_alloc(24, 8);
        assert!(
            !x.is_null() && !y.is_null() && !z.is_null() && !g.is_null(),
            "kheap-coalesce: an alloc failed"
        );
        assert!(
            y as usize == x as usize + 48 + kheap::HDR_BYTES as usize,
            "kheap-coalesce: x/y not adjacent (heap not pristine?)"
        );
        assert!(
            z as usize == y as usize + 80 + kheap::HDR_BYTES as usize,
            "kheap-coalesce: y/z not adjacent"
        );
        api::rhu_kheap_free(y);
        api::rhu_kheap_free(z);
        api::rhu_kheap_free(x);
        let sum = 48 + 80 + 112 + 2 * kheap::HDR_BYTES;
        let w = api::rhu_kheap_alloc(sum, 8);
        assert!(w == x, "kheap-coalesce: the summed span did not reallocate at the first block");
        api::rhu_kheap_free(w);
        api::rhu_kheap_free(g);
        kprintln!("[api] PASS kheap-coalesce (adjacent frees merged whole: the summed span reallocated at the first block)");
    }

    static TLS_H1: AtomicI32 = AtomicI32::new(-1);
    static TLS_H2: AtomicI32 = AtomicI32::new(-1);
    static TLS_INIT_IMAGE: [u8; 24] = [
        0xC1, 0xC2, 0xC3, 0xC4, 0xC5, 0xC6, 0xC7, 0xC8, 0xC9, 0xCA, 0xCB, 0xCC, 0xCD, 0xCE,
        0xCF, 0xD0, 0xD1, 0xD2, 0xD3, 0xD4, 0xD5, 0xD6, 0xD7, 0xD8,
    ];
    static TLS_ADDRS: [AtomicUsize; 4] = [
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
    ];
    static TLS_SEEN_BAD: AtomicU32 = AtomicU32::new(0);
    static TLS_READY: AtomicU32 = AtomicU32::new(0);
    static TLS_PARK: AtomicU32 = AtomicU32::new(0);
    static TLS_EXITED: AtomicU32 = AtomicU32::new(0);

    fn scan_eq(p: *const u8, want: &[u8]) -> bool {
        for (i, w) in want.iter().enumerate() {
            if unsafe { read_volatile(p.add(i)) } != *w {
                return false;
            }
        }
        true
    }

    extern "C" fn tls_worker(arg: *mut c_void) {
        let me = arg as usize;
        let h1 = TLS_H1.load(Ordering::Acquire);
        let h2 = TLS_H2.load(Ordering::Acquire);
        let b1 = api::rhu_tls_get(h1);
        let b2 = api::rhu_tls_get(h2);
        if !scan_eq(b1, &TLS_INIT_IMAGE) || !scan_zero(b2, 48) {
            TLS_SEEN_BAD.store(1, Ordering::Release);
        }
        unsafe {
            write_volatile(b1, 0x41 + me as u8);
            write_volatile(b2, 0x61 + me as u8);
        }
        TLS_ADDRS[me * 2].store(b1 as usize, Ordering::Release);
        TLS_ADDRS[me * 2 + 1].store(b2 as usize, Ordering::Release);
        TLS_READY.fetch_add(1, Ordering::Release);
        api::rhu_futex_wake(TLS_READY.as_ptr(), u32::MAX);
        while TLS_PARK.load(Ordering::Acquire) == 0 {
            let _ = api::rhu_futex_wait(TLS_PARK.as_ptr(), 0, sched::DEADLINE_NONE);
        }
        TLS_EXITED.fetch_add(1, Ordering::Release);
        api::rhu_futex_wake(TLS_EXITED.as_ptr(), u32::MAX);
    }

    fn check_tls_blocks() {
        kprintln!("[api] tls-blocks: two modules x two threads armed");
        let d1 = registry::ModuleDesc {
            text_lo: 0,
            text_hi: 0,
            statics_lo: [0; 4],
            statics_hi: [0; 4],
            frozen_lo: 0,
            frozen_hi: 0,
            exidx_lo: 0,
            exidx_hi: 0,
            tls_bytes: 24,
            tls_init: TLS_INIT_IMAGE.as_ptr(),
            name: c"smoke-tls-img".as_ptr() as *const u8,
        };
        let mut d2 = d1;
        d2.tls_bytes = 48;
        d2.tls_init = core::ptr::null();
        d2.name = c"smoke-tls-zero".as_ptr() as *const u8;
        let h1 = api::rhu_module_register(&d1);
        let h2 = api::rhu_module_register(&d2);
        assert!(h1 > 0 && h2 > 0 && h1 != h2, "tls-blocks: bad handles {} {}", h1, h2);
        TLS_H1.store(h1, Ordering::Release);
        TLS_H2.store(h2, Ordering::Release);

        let g1 = api::rhu_tls_get(h1);
        let g2 = api::rhu_tls_get(h2);
        assert!(scan_eq(g1, &TLS_INIT_IMAGE), "tls-blocks: kmain's init image wrong");
        assert!(scan_zero(g2, 48), "tls-blocks: kmain's zero-fill block dirty");
        unsafe { write_volatile(g1, 0xAA) };

        let t0 = api::rhu_thread_spawn(
            Some(tls_worker),
            0 as *mut c_void,
            4 * 1024,
            0x40,
            c"tlsw0".as_ptr(),
        );
        let t1 = api::rhu_thread_spawn(
            Some(tls_worker),
            1 as *mut c_void,
            4 * 1024,
            0x40,
            c"tlsw1".as_ptr(),
        );
        assert!(t0 >= 1 && t1 >= 1 && t0 != t1, "tls-blocks: spawn failed");
        wait_until(|| TLS_READY.load(Ordering::Acquire) == 2, "tls workers ready");

        let a1 = TLS_ADDRS[0].load(Ordering::Acquire) as *const u8;
        let a2 = TLS_ADDRS[1].load(Ordering::Acquire) as *const u8;
        let b1 = TLS_ADDRS[2].load(Ordering::Acquire) as *const u8;
        let b2 = TLS_ADDRS[3].load(Ordering::Acquire) as *const u8;
        assert!(TLS_SEEN_BAD.load(Ordering::Acquire) == 0, "tls-blocks: a worker saw a bad birth copy");
        let mut all = [g1 as usize, g2 as usize, a1 as usize, a2 as usize, b1 as usize, b2 as usize];
        all.sort_unstable();
        for w in all.windows(2) {
            assert!(w[0] != w[1], "tls-blocks: two blocks share an address");
        }
        unsafe {
            assert!(read_volatile(a1) == 0x41 && read_volatile(b1) == 0x42, "tls-blocks: module-1 writes wrong");
            assert!(read_volatile(a2) == 0x61 && read_volatile(b2) == 0x62, "tls-blocks: module-2 writes wrong");
            assert!(read_volatile(g1) == 0xAA, "tls-blocks: a worker's write bled into kmain's block");
            assert!(scan_eq(a1.add(1), &TLS_INIT_IMAGE[1..]), "tls-blocks: worker image tail damaged");
        }
        release(&TLS_PARK);
        wait_until(|| TLS_EXITED.load(Ordering::Acquire) == 2, "tls workers exited");
        kprintln!("[api] PASS tls-blocks (per-thread blocks distinct, init image + zero fill honored, no cross-thread bleed)");
    }

    static DOOR_PARK: AtomicU32 = AtomicU32::new(0);
    static DOOR_READY: AtomicU32 = AtomicU32::new(0);
    static DOOR_TID: [AtomicI32; 2] = [AtomicI32::new(0), AtomicI32::new(0)];
    static DOOR_BASE: [AtomicUsize; 2] = [AtomicUsize::new(0), AtomicUsize::new(0)];

    extern "C" fn door_worker(arg: *mut c_void) {
        let me = arg as usize;
        DOOR_TID[me].store(api::rhu_thread_self(), Ordering::Release);
        DOOR_BASE[me].store(api::rhu_tls_get(TLS_H1.load(Ordering::Acquire)) as usize, Ordering::Release);
        DOOR_READY.fetch_add(1, Ordering::Release);
        api::rhu_futex_wake(DOOR_READY.as_ptr(), u32::MAX);
        while DOOR_PARK.load(Ordering::Acquire) == 0 {
            let _ = api::rhu_futex_wait(DOOR_PARK.as_ptr(), 0, sched::DEADLINE_NONE);
        }
    }

    fn check_tls_base_door() {
        kprintln!("[api] tls-base-door: the layout record walked from memory against the doors armed");
        unsafe extern "C" {
            static __rhu_sched: u8;
            static __rhu_tdata_lo: u32;
            static __rhu_tdata_hi: u32;
        }
        let lay = addr_of!(sched::__rhu_debug_layout) as *const u32;
        let w = |i: usize| unsafe { read_volatile(lay.add(i)) };
        assert!(w(0) == 0x4448_5552, "tls-base-door: layout magic {:#x}", w(0));
        assert!(w(1) >= 2, "tls-base-door: layout version {} predates the door", w(1));
        let (max_threads, off_tcbs, off_current, stride) = (w(2), w(3), w(4), w(5));
        let (off_state, off_tid) = (w(7), w(9));
        let (off_tls, slots, slot_bytes) = (w(13), w(14), w(15));
        assert!(
            slots == registry::MAX_MODULES as u32 && slot_bytes == 4,
            "tls-base-door: {} slots of {} bytes recorded",
            slots,
            slot_bytes
        );
        assert!(off_tls + slots * slot_bytes <= stride, "tls-base-door: the slots overrun the TCB stride");
        let rd32 = |a: u32| unsafe { read_volatile(a as *const u32) };
        let rd8 = |a: u32| unsafe { read_volatile(a as *const u8) };

        let h1 = TLS_H1.load(Ordering::Acquire);
        let h2 = TLS_H2.load(Ordering::Acquire);
        assert!(h1 > 0 && h2 > 0 && (h1 as u32) < slots && (h2 as u32) < slots, "tls-base-door: no registered modules on record");
        let t0 = api::rhu_thread_spawn(Some(door_worker), 0 as *mut c_void, 4 * 1024, 0x40, c"doorw0".as_ptr());
        let t1 = api::rhu_thread_spawn(Some(door_worker), 1 as *mut c_void, 4 * 1024, 0x40, c"doorw1".as_ptr());
        assert!(t0 >= 1 && t1 >= 1 && t0 != t1, "tls-base-door: spawn failed");
        wait_until(|| DOOR_READY.load(Ordering::Acquire) == 2, "door workers ready");

        let me = api::rhu_thread_self();
        let g1 = api::rhu_tls_get(h1) as u32;
        let g2 = api::rhu_tls_get(h2) as u32;
        let tlo = core::hint::black_box(addr_of!(__rhu_tdata_lo) as u32);
        let thi = core::hint::black_box(addr_of!(__rhu_tdata_hi) as u32);
        assert!(thi == tlo, "tls-base-door: this family's template brackets are not empty");

        let base = addr_of!(__rhu_sched) as u32;
        let cur = rd32(base + off_current);
        let (mut live, mut seen_me, mut seen_w) = (0u32, false, [false; 2]);
        for i in 0..max_threads {
            let tcb = base + off_tcbs + i * stride;
            let state = rd8(tcb + off_state);
            let tid = rd32(tcb + off_tid) as i32;
            let slot = |m: u32| rd32(tcb + off_tls + m * slot_bytes);
            if state == 1 || state == 2 {
                live += 1;
                assert!(slot(0) == 0, "tls-base-door: a module-0 block on a live thread beside empty template brackets");
                if tid == me {
                    assert!(i == cur, "tls-base-door: the running thread is not the recorded current slot");
                    assert!(slot(h1 as u32) == g1 && slot(h2 as u32) == g2, "tls-base-door: kmain's blocks through the record differ from the door's answers");
                    seen_me = true;
                }
                for k in 0..2 {
                    if tid == DOOR_TID[k].load(Ordering::Acquire) {
                        assert!(slot(h1 as u32) == DOOR_BASE[k].load(Ordering::Acquire) as u32, "tls-base-door: a worker's block through the record differs from its own answer");
                        seen_w[k] = true;
                    }
                }
            } else {
                for m in 0..slots {
                    assert!(slot(m) == 0, "tls-base-door: a non-live slot holds a block pointer");
                }
            }
        }
        assert!(seen_me && seen_w[0] && seen_w[1], "tls-base-door: a thread on record was not found in the walk");
        assert!(live >= 4, "tls-base-door: {} live threads walked (want idle, kmain, two workers at least)", live);
        release(&DOOR_PARK);
        join_tid(t0);
        join_tid(t1);
        kprintln!("[api] PASS tls-base-door (record walked from memory: module-0 base null on every live thread beside empty template brackets, registered-module bases equal to the door's answers on kmain and both workers, non-live slots null)");
    }

    static DTOR_SEQ: AtomicU32 = AtomicU32::new(0);
    static DTOR_ORDER: [AtomicU32; 8] = [
        AtomicU32::new(0),
        AtomicU32::new(0),
        AtomicU32::new(0),
        AtomicU32::new(0),
        AtomicU32::new(0),
        AtomicU32::new(0),
        AtomicU32::new(0),
        AtomicU32::new(0),
    ];
    static DTOR_TID: AtomicI32 = AtomicI32::new(0);
    static DTOR_BAD: AtomicU32 = AtomicU32::new(0);

    extern "C" fn dtor_recorder(arg: *mut c_void) {
        let seq = DTOR_SEQ.fetch_add(1, Ordering::AcqRel);
        if (seq as usize) < DTOR_ORDER.len() {
            DTOR_ORDER[seq as usize].store(arg as u32 + 1, Ordering::Release);
        }
        if api::rhu_thread_self() != DTOR_TID.load(Ordering::Acquire) {
            DTOR_BAD.fetch_or(1, Ordering::AcqRel);
        }
        let p = api::rhu_tls_get(TLS_H1.load(Ordering::Acquire));
        if unsafe { read_volatile(p) } != 0xD7 {
            DTOR_BAD.fetch_or(2, Ordering::AcqRel);
        }
    }

    extern "C" fn dtor_thread(_: *mut c_void) {
        let p = api::rhu_tls_get(TLS_H1.load(Ordering::Acquire));
        unsafe { write_volatile(p, 0xD7) };
        DTOR_TID.store(api::rhu_thread_self(), Ordering::Release);
        for i in 0..8usize {
            let r = api::rhu_thread_atexit(Some(dtor_recorder), i as *mut c_void);
            assert!(r == 0, "tls-dtors: registration {} refused with {}", i, r);
        }
        let r = api::rhu_thread_atexit(Some(dtor_recorder), 8 as *mut c_void);
        assert!(r == sched::ERR_AGAIN, "tls-dtors: the ninth dtor was not refused ({})", r);
    }

    fn check_tls_dtors() {
        kprintln!("[api] tls-dtors: LIFO dtor stack + cap refusal armed");
        let t = api::rhu_thread_spawn(
            Some(dtor_thread),
            core::ptr::null_mut(),
            4 * 1024,
            0x40,
            c"dtor".as_ptr(),
        );
        assert!(t >= 1, "tls-dtors: spawn failed");
        wait_until(|| DTOR_SEQ.load(Ordering::Acquire) == 8, "eight dtors to run");
        api::rhu_sleep_ns(2_000_000);
        assert!(DTOR_SEQ.load(Ordering::Acquire) == 8, "tls-dtors: more than eight dtor runs");
        for (call, slot) in DTOR_ORDER.iter().enumerate() {
            let want = 8 - call as u32;
            assert!(
                slot.load(Ordering::Acquire) == want,
                "tls-dtors: call {} recorded {} (want {})",
                call,
                slot.load(Ordering::Acquire),
                want
            );
        }
        assert!(DTOR_BAD.load(Ordering::Acquire) == 0, "tls-dtors: off-thread run or dead TLS inside a dtor");
        kprintln!("[api] PASS tls-dtors (dtors ran LIFO exactly once on the exiting thread, TLS live inside, cap refused honest)");
    }

    static ABI_PARK: AtomicU32 = AtomicU32::new(0);
    static ABI_SELF_SEEN: AtomicI32 = AtomicI32::new(0);

    extern "C" fn probe_return(_: *mut c_void) {
        while ABI_PARK.load(Ordering::Acquire) == 0 {
            let _ = api::rhu_futex_wait(ABI_PARK.as_ptr(), 0, sched::DEADLINE_NONE);
        }
    }

    extern "C" fn probe_exit_verb(_: *mut c_void) {
        ABI_SELF_SEEN.store(api::rhu_thread_self(), Ordering::Release);
        api::rhu_thread_yield();
        while ABI_PARK.load(Ordering::Acquire) == 0 {
            let _ = api::rhu_futex_wait(ABI_PARK.as_ptr(), 0, sched::DEADLINE_NONE);
        }
        api::rhu_thread_exit();
    }

    /// Spawn one parked probe through the C door.
    fn spawn_probe(entry: extern "C" fn(*mut c_void), stack: u32) -> i32 {
        api::rhu_thread_spawn(Some(entry), core::ptr::null_mut(), stack, 0x40, c"probe".as_ptr())
    }

    /// Join a probe by its own verb: dead reads RHU_ESRCH; then one
    /// sleep so the reap (a switch-path event) has provably run.
    fn join_tid(tid: i32) {
        wait_until(
            || {
                let (mut lo, mut b) = (0u32, 0u32);
                api::rhu_thread_stack_bounds(tid, &mut lo, &mut b) == sched::ERR_SRCH
            },
            "probe exit",
        );
        api::rhu_sleep_ns(2_000_000);
    }

    fn check_thread_abi() {
        kprintln!("[api] thread-abi: C-door round trip + exact-fit stack reuse armed");
        let t1 = spawn_probe(probe_exit_verb, 2048);
        assert!(t1 >= 1, "thread-abi: spawn returned {}", t1);
        wait_until(|| sched::probe_futex_waiting(ABI_PARK.as_ptr()) == 1, "probe 1 parked");
        assert!(ABI_SELF_SEEN.load(Ordering::Acquire) == t1, "thread-abi: self() != spawn's tid");
        let (mut lo1, mut b1) = (0u32, 0u32);
        assert!(api::rhu_thread_stack_bounds(t1, &mut lo1, &mut b1) == 0, "thread-abi: bounds(t1) refused");
        release(&ABI_PARK);
        join_tid(t1);
        ABI_PARK.store(0, Ordering::Release);

        let t2 = spawn_probe(probe_exit_verb, 2048);
        assert!(t2 >= 1 && t2 != t1, "thread-abi: tid not unique across spawn-exit-spawn");
        wait_until(|| sched::probe_futex_waiting(ABI_PARK.as_ptr()) == 1, "probe 2 parked");
        let (mut lo2, mut b2) = (0u32, 0u32);
        assert!(api::rhu_thread_stack_bounds(t2, &mut lo2, &mut b2) == 0, "thread-abi: bounds(t2) refused");
        assert!(lo2 == lo1 && b2 == b1, "thread-abi: exact-fit reuse missed (bounds differ)");
        release(&ABI_PARK);
        join_tid(t2);
        ABI_PARK.store(0, Ordering::Release);

        let (live0, free0) = sched::probe_slot_states();
        assert!(
            live0 + free0 == sched::MAX_THREADS as u32,
            "thread-abi: exited residue before the exhaustion arm"
        );
        let (occ0, drops0, bytes0) = sched::probe_retired();
        let mut spawned = 0u32;
        let mut tids = [0i32; sched::MAX_THREADS];
        loop {
            let t = spawn_probe(probe_return, 1024);
            if t == sched::ERR_AGAIN {
                break;
            }
            assert!(t >= 1, "thread-abi: spawn returned {}", t);
            tids[spawned as usize] = t;
            spawned += 1;
            assert!(spawned <= sched::MAX_THREADS as u32, "thread-abi: spawn never refused");
        }
        assert!(spawned == free0, "thread-abi: refused at {} spawns, {} slots were free", spawned, free0);
        assert!(
            spawn_probe(probe_return, 1024) == sched::ERR_AGAIN,
            "thread-abi: refusal not stable"
        );
        release(&ABI_PARK);
        for i in 0..spawned {
            join_tid(tids[i as usize]);
        }
        ABI_PARK.store(0, Ordering::Release);
        let (occ1, drops1, bytes1) = sched::probe_retired();
        let accepted = spawned.min(sched::RETIRED_CAP as u32 - occ0);
        assert!(occ1 == occ0 + accepted, "thread-abi: retired occupancy {} (want {})", occ1, occ0 + accepted);
        assert!(
            drops1 - drops0 == spawned - accepted,
            "thread-abi: leak drops {} (want {})",
            drops1 - drops0,
            spawned - accepted
        );
        assert!(
            bytes1 - bytes0 == (spawned - accepted) * 1024,
            "thread-abi: leaked bytes {} (want {})",
            bytes1 - bytes0,
            (spawned - accepted) * 1024
        );
        kprintln!("[api] PASS thread-abi (round trip clean, tids unique, exact-fit stack reused, slot exhaustion refused honest)");
    }

    static HB_STARTED: AtomicU32 = AtomicU32::new(0);
    static HB_CHILD_PRIO: AtomicU32 = AtomicU32::new(0xFFFF);
    static HB_CHILD_TID: AtomicI32 = AtomicI32::new(0);
    static HB_PEER_RAN: AtomicU32 = AtomicU32::new(0);
    static HB_LATE_RAN: AtomicU32 = AtomicU32::new(0);

    extern "C" fn band_child(_: *mut c_void) {
        HB_CHILD_PRIO.store(sched::probe_current_prio() as u32, Ordering::Release);
        HB_CHILD_TID.store(api::rhu_thread_self(), Ordering::Release);
        HB_STARTED.store(1, Ordering::Release);
    }

    extern "C" fn band_peer(_: *mut c_void) {
        HB_PEER_RAN.store(1, Ordering::Release);
    }

    extern "C" fn band_late(_: *mut c_void) {
        HB_LATE_RAN.store(1, Ordering::Release);
    }

    /// Spawn one band thread through the C door at `prio`, 4K stack.
    fn spawn_band(entry: extern "C" fn(*mut c_void), prio: u32, name: &'static core::ffi::CStr) -> i32 {
        let t = api::rhu_thread_spawn(Some(entry), core::ptr::null_mut(), 4096, prio, name.as_ptr());
        assert!(t >= 1, "host-band: spawn returned {}", t);
        t
    }

    fn check_host_band() {
        kprintln!("[api] host-band: door-spawned child at the flat priority + yield-only start wait armed");
        let kernel_prio = sched::probe_current_prio();
        assert!(
            (kernel_prio as u32) < api::HOSTED_PRIO_LO,
            "host-band: this thread holds priority {} (not kernel urgency)",
            kernel_prio
        );
        let flat = api::HOSTED_PRIO_FLAT as u8;

        let prev = sched::set_current_prio(flat);
        assert!(prev == kernel_prio, "host-band: the join returned {} (not the priority held)", prev);
        let tc = spawn_band(band_child, api::HOSTED_PRIO_FLAT, c"bandchild");
        let t0 = sched::now_ms();
        while HB_STARTED.load(Ordering::Acquire) == 0 {
            assert!(
                sched::now_ms().wrapping_sub(t0) < 2000,
                "host-band: the started signal never arrived through yield alone (the host cannot release its child)"
            );
            api::rhu_thread_yield();
        }
        assert!(HB_CHILD_TID.load(Ordering::Acquire) == tc, "host-band: the started signal came from another thread");
        let mine = sched::probe_current_prio();
        assert!(mine == flat, "host-band: the host reads priority {} inside the band", mine);
        assert!(
            HB_CHILD_PRIO.load(Ordering::Acquire) == mine as u32,
            "host-band: the child ran at priority {} beside its host at {} (not one level)",
            HB_CHILD_PRIO.load(Ordering::Acquire),
            mine
        );
        join_tid(tc);

        assert!(sched::set_current_prio(kernel_prio) == flat, "host-band: the leave returned another priority");
        let tp = spawn_band(band_peer, api::HOSTED_PRIO_LO, c"bandpeer");
        for _ in 0..50 {
            api::rhu_thread_yield();
        }
        assert!(HB_PEER_RAN.load(Ordering::Acquire) == 0, "host-band: a band thread ran while its host held kernel urgency");
        let _ = sched::set_current_prio(flat);
        assert!(
            HB_PEER_RAN.load(Ordering::Acquire) == 1,
            "host-band: joining the band did not re-run the pick (the more urgent band thread still waits)"
        );
        join_tid(tp);

        assert!(sched::set_current_prio(kernel_prio) == flat, "host-band: the second leave returned another priority");
        assert!(sched::probe_current_prio() == kernel_prio, "host-band: kernel urgency not restored");
        let tl = spawn_band(band_late, api::HOSTED_PRIO_FLAT, c"bandlate");
        for _ in 0..50 {
            api::rhu_thread_yield();
        }
        assert!(HB_LATE_RAN.load(Ordering::Acquire) == 0, "host-band: a band thread ran after the host left the band");
        join_tid(tl);
        assert!(HB_LATE_RAN.load(Ordering::Acquire) == 1, "host-band: the band thread never ran once the host blocked");
        kprintln!("[api] PASS host-band (the host joined the band and its door-spawned child at the flat priority started through yield alone at one level, joining re-ran the pick, leaving restored kernel urgency and held a band thread until the host blocked)");
    }

    fn check_tid_wrap() {
        kprintln!("[api] tid-wrap: the id space's last two ids + the refusal past it armed");
        sched::probe_tid_seek(0x7FFF_FFFE);
        let a = spawn_probe(probe_return, 1024);
        let b = spawn_probe(probe_return, 1024);
        assert!(a == 0x7FFF_FFFE, "tid-wrap: the second-to-last id read {:#x}", a);
        assert!(b == 0x7FFF_FFFF, "tid-wrap: the last id read {:#x}", b);
        let c = spawn_probe(probe_return, 1024);
        assert!(c == sched::ERR_AGAIN, "tid-wrap: the spawn past the id space returned {}", c);
        assert!(sched::probe_next_tid() == 0x8000_0000, "tid-wrap: the counter moved on a refused spawn");
        assert!(spawn_probe(probe_return, 1024) == sched::ERR_AGAIN, "tid-wrap: the refusal is not stable");
        assert!(sched::probe_next_tid() == 0x8000_0000, "tid-wrap: the counter moved on the second refusal");
        release(&ABI_PARK);
        join_tid(a);
        join_tid(b);
        ABI_PARK.store(0, Ordering::Release);
        kprintln!("[api] PASS tid-wrap (ids 0x7ffffffe and 0x7fffffff handed out positive, the spawn past the id space refused EAGAIN twice with the counter unmoved, no negative id)");
    }

    static PEER_LOCAL: AtomicUsize = AtomicUsize::new(0);
    static PEER_PARK: AtomicU32 = AtomicU32::new(0);

    extern "C" fn bounds_peer(_: *mut c_void) {
        let planted = 0u32;
        PEER_LOCAL.store(addr_of!(planted) as usize, Ordering::Release);
        while PEER_PARK.load(Ordering::Acquire) == 0 {
            let _ = api::rhu_futex_wait(PEER_PARK.as_ptr(), 0, sched::DEADLINE_NONE);
        }
    }

    fn check_thread_bounds() {
        kprintln!("[api] thread-bounds: own + peer + reaped-tid probes armed");
        let my_local = 0u32;
        let me = api::rhu_thread_self();
        let (mut mlo, mut mb) = (0u32, 0u32);
        assert!(api::rhu_thread_stack_bounds(me, &mut mlo, &mut mb) == 0, "thread-bounds: own lookup refused");
        let mla = addr_of!(my_local) as usize;
        assert!(
            (mlo as usize..(mlo + mb) as usize).contains(&mla),
            "thread-bounds: own local outside own window"
        );
        let (klo, khi) = sched::kstacks_window();
        assert!(
            mlo as usize >= klo && (mlo + mb) as usize <= khi,
            "thread-bounds: own window outside the kstacks pool"
        );

        let tp = api::rhu_thread_spawn(
            Some(bounds_peer),
            core::ptr::null_mut(),
            4 * 1024,
            0x40,
            c"peer".as_ptr(),
        );
        assert!(tp >= 1, "thread-bounds: peer spawn failed");
        wait_until(|| PEER_LOCAL.load(Ordering::Acquire) != 0, "peer planted its local");
        let (mut plo, mut pb) = (0u32, 0u32);
        assert!(api::rhu_thread_stack_bounds(tp, &mut plo, &mut pb) == 0, "thread-bounds: peer lookup refused");
        let pla = PEER_LOCAL.load(Ordering::Acquire);
        assert!(
            (plo as usize..(plo + pb) as usize).contains(&pla),
            "thread-bounds: peer local outside peer window"
        );
        assert!(
            !((plo as usize..(plo + pb) as usize).contains(&mla)),
            "thread-bounds: own local inside the peer window"
        );
        assert!(
            plo as usize >= klo && (plo + pb) as usize <= khi,
            "thread-bounds: peer window outside the kstacks pool"
        );
        assert!(
            ((plo + pb).min(mlo + mb)) <= plo.max(mlo),
            "thread-bounds: the two windows overlap"
        );

        let mut scratch = 0u32;
        assert!(
            api::rhu_thread_stack_bounds(me, core::ptr::null_mut(), &mut scratch) == sched::ERR_INVAL,
            "thread-bounds: null lo not refused EINVAL"
        );
        assert!(
            api::rhu_thread_stack_bounds(me, &mut scratch, core::ptr::null_mut()) == sched::ERR_INVAL,
            "thread-bounds: null bytes not refused EINVAL"
        );
        let dead = DTOR_TID.load(Ordering::Acquire);
        assert!(dead >= 1, "thread-bounds: no reaped tid on record");
        assert!(
            api::rhu_thread_stack_bounds(dead, &mut mlo, &mut mb) == sched::ERR_SRCH,
            "thread-bounds: reaped tid not refused ESRCH"
        );
        release(&PEER_PARK);
        join_tid(tp);
        kprintln!("[api] PASS thread-bounds (own and peer locals bracketed, windows inside kstacks, reaped tid refused ESRCH)");
    }

    fn check_registry_basic() {
        kprintln!("[api] registry-basic: desc copy-proof + slots-full refusal armed");
        unsafe extern "C" {
            static __stext: u32;
            static __etext: u32;
            static __sdata: u32;
            static __edata: u32;
            static __sbss: u32;
            static __ebss: u32;
        }
        let mod0 = registry::probe_module(0);
        assert!(
            mod0.text_lo == addr_of!(__stext) as u32 && mod0.text_hi == addr_of!(__etext) as u32,
            "registry-basic: module 0 text bracket wrong"
        );
        assert!(
            mod0.statics_lo[0] == addr_of!(__sdata) as u32 && mod0.statics_hi[0] == addr_of!(__edata) as u32,
            "registry-basic: module 0 .data range wrong"
        );
        assert!(
            mod0.statics_lo[1] == addr_of!(__sbss) as u32 && mod0.statics_hi[1] == addr_of!(__ebss) as u32,
            "registry-basic: module 0 .bss range wrong"
        );
        assert!(mod0.tls_bytes == 0, "registry-basic: module 0 grew TLS");

        let keep_name = c"x3".as_ptr() as *const u8;
        let mut d3 = registry::ModuleDesc {
            text_lo: 0x1000,
            text_hi: 0x2000,
            statics_lo: [0x3000, 0, 0, 0],
            statics_hi: [0x3800, 0, 0, 0],
            frozen_lo: 0,
            frozen_hi: 0,
            exidx_lo: 0,
            exidx_hi: 0,
            tls_bytes: 0,
            tls_init: core::ptr::null(),
            name: keep_name,
        };
        let h3 = api::rhu_module_register(&d3);
        let h1 = TLS_H1.load(Ordering::Acquire);
        let h2 = TLS_H2.load(Ordering::Acquire);
        assert!(h3 > 0 && h3 != h1 && h3 != h2, "registry-basic: handle {} not distinct", h3);
        unsafe {
            core::ptr::write_bytes(addr_of_mut!(d3), 0xFF, 1);
        }
        let stored = registry::probe_module(h3);
        assert!(
            stored.text_lo == 0x1000
                && stored.text_hi == 0x2000
                && stored.statics_lo[0] == 0x3000
                && stored.statics_hi[0] == 0x3800
                && stored.tls_bytes == 0
                && stored.name == keep_name,
            "registry-basic: the stored desc changed with its caller's struct"
        );
        let d4 = registry::ModuleDesc { name: c"x4".as_ptr() as *const u8, ..stored };
        assert!(
            api::rhu_module_register(&d4) == sched::ERR_AGAIN,
            "registry-basic: full table not refused"
        );
        assert!(
            api::rhu_module_register(&d4) == sched::ERR_AGAIN,
            "registry-basic: refusal not stable"
        );
        kprintln!("[api] PASS registry-basic (handles distinct, stored desc survives caller clobber, full table refused honest)");
    }


    fn zctx() -> sched::RhuCtx {
        sched::RhuCtx { r: [0; 13], sp: 0, lr: 0, pc: 0, xpsr: 0, flags: 0 }
    }

    static SUSP_PROG: AtomicU32 = AtomicU32::new(0);
    static SUSP_REL: AtomicU32 = AtomicU32::new(0);
    static SUSP_RAN: AtomicU32 = AtomicU32::new(0);
    static SUSP_WWORD: AtomicU32 = AtomicU32::new(0);

    extern "C" fn susp_spinner(_: usize) {
        while SUSP_REL.load(Ordering::Acquire) == 0 {
            SUSP_PROG.fetch_add(1, Ordering::Relaxed);
        }
    }

    extern "C" fn susp_waiter(_: usize) {
        let r = api::rhu_futex_wait(SUSP_WWORD.as_ptr(), 0, sched::DEADLINE_NONE);
        assert!(r == 0, "susp-pick: waiter woke with {}", r);
        SUSP_RAN.store(1, Ordering::Release);
    }

    fn check_susp_pick() {
        kprintln!("[api] susp-pick: suspended-ready spinner + woken-waiter hold armed");
        let (slot, tid) =
            sched::try_spawn("dspin", susp_spinner, 0, 3, 4096).ok().expect("susp-pick: spawn");
        api::rhu_sleep_ns(2_000_000);
        assert!(SUSP_PROG.load(Ordering::Acquire) > 0, "susp-pick: spinner never ran");
        let mut ctx = zctx();
        assert!(api::rhu_thread_suspend(tid, &mut ctx) == 0, "susp-pick: suspend refused");
        let p0 = SUSP_PROG.load(Ordering::Acquire);
        for _ in 0..50 {
            api::rhu_thread_yield();
        }
        assert!(sched::probe_ready_among(&[slot]) == 1, "susp-pick: state left Ready");
        assert!(SUSP_PROG.load(Ordering::Acquire) == p0, "susp-pick: suspended spinner progressed");
        assert!(api::rhu_thread_resume(tid) == 0, "susp-pick: resume refused");
        wait_until(|| SUSP_PROG.load(Ordering::Acquire) > p0, "resumed spinner to progress");
        SUSP_REL.store(1, Ordering::Release);
        join_tid(tid);

        let (wslot, wtid) =
            sched::try_spawn("dwait", susp_waiter, 0, 3, 4096).ok().expect("susp-pick: spawn 2");
        wait_until(|| sched::probe_futex_waiting(SUSP_WWORD.as_ptr()) == 1, "waiter to park");
        let mut ctx2 = zctx();
        assert!(api::rhu_thread_suspend(wtid, &mut ctx2) == 0, "susp-pick: waiter suspend refused");
        let woken = api::rhu_futex_wake(SUSP_WWORD.as_ptr(), 1);
        assert!(woken == 1, "susp-pick: suspended waiter not counted readied ({})", woken);
        assert!(sched::probe_ready_among(&[wslot]) == 1, "susp-pick: woken waiter not Ready");
        for _ in 0..50 {
            api::rhu_thread_yield();
        }
        assert!(SUSP_RAN.load(Ordering::Acquire) == 0, "susp-pick: suspended waiter RAN");
        assert!(api::rhu_thread_resume(wtid) == 0, "susp-pick: waiter resume refused");
        wait_until(|| SUSP_RAN.load(Ordering::Acquire) == 1, "resumed waiter to run");
        join_tid(wtid);
        kprintln!("[api] PASS susp-pick (suspended spinner stayed ready with zero progress, woken waiter held until resume)");
    }

    static CTX_REL: AtomicU32 = AtomicU32::new(0);

    extern "C" fn ctx_planter(_: usize) {
        unsafe {
            core::arch::asm!(
                "2:",
                "ldr {t}, [{w}]",
                "cmp {t}, #0",
                "beq 2b",
                w = in(reg) CTX_REL.as_ptr(),
                t = out(reg) _,
                in("r4") 0xD400_04D4u32,
                in("r5") 0xD500_05D5u32,
                in("r9") 0xD900_09D9u32,
                in("r8") 0xD800_08D8u32,
                in("r10") 0xDA00_0ADAu32,
                in("r11") 0xDB00_0BDBu32,
                options(nostack, readonly)
            );
        }
    }

    fn check_susp_ctx() {
        kprintln!("[api] susp-ctx: planted-pattern context snapshot armed");
        unsafe extern "C" {
            static __stext: u32;
            static __etext: u32;
        }
        CTX_REL.store(0, Ordering::Release);
        let (_slot, tid) =
            sched::try_spawn("dctx", ctx_planter, 0, 3, 4096).ok().expect("susp-ctx: spawn");
        api::rhu_sleep_ns(2_000_000);
        let mut ctx = zctx();
        assert!(api::rhu_thread_suspend(tid, &mut ctx) == 0, "susp-ctx: suspend refused");
        for (reg, want) in [
            (4usize, 0xD400_04D4u32),
            (5, 0xD500_05D5),
            (9, 0xD900_09D9),
            (8, 0xD800_08D8),
            (10, 0xDA00_0ADA),
            (11, 0xDB00_0BDB),
        ] {
            assert!(
                ctx.r[reg] == want,
                "susp-ctx: r{} read {:#010x} (want {:#010x})",
                reg,
                ctx.r[reg],
                want
            );
        }
        let (mut lo, mut b) = (0u32, 0u32);
        assert!(api::rhu_thread_stack_bounds(tid, &mut lo, &mut b) == 0, "susp-ctx: bounds refused");
        assert!(ctx.sp > lo && ctx.sp <= lo + b, "susp-ctx: snapshot sp outside the target stack");
        let (tlo, thi) = (addr_of!(__stext) as u32, addr_of!(__etext) as u32);
        assert!(ctx.pc >= tlo && ctx.pc < thi, "susp-ctx: snapshot pc outside kernel text");
        let _ = ctx.flags;
        assert!(api::rhu_thread_resume(tid) == 0, "susp-ctx: resume refused");
        CTX_REL.store(1, Ordering::Release);
        join_tid(tid);
        kprintln!("[api] PASS susp-ctx (planted registers exact in ctx, sp inside the target stack, pc inside kernel text)");
    }

    static SC_WITNESS: AtomicU32 = AtomicU32::new(0);
    static SC_TID: AtomicI32 = AtomicI32::new(0);

    extern "C" fn setctx_pad(magic: usize) -> ! {
        SC_WITNESS.store(magic as u32, Ordering::Release);
        SC_TID.store(api::rhu_thread_self(), Ordering::Release);
        api::rhu_thread_exit()
    }

    fn check_susp_setctx() {
        kprintln!("[api] susp-setctx: landing pad + sp-refusal armed");
        CTX_REL.store(0, Ordering::Release);
        let (_slot, tid) =
            sched::try_spawn("dsctx", ctx_planter, 0, 3, 4096).ok().expect("susp-setctx: spawn");
        api::rhu_sleep_ns(2_000_000);
        let mut ctx = zctx();
        assert!(api::rhu_thread_suspend(tid, &mut ctx) == 0, "susp-setctx: suspend refused");
        let mut c2 = zctx();
        assert!(
            api::rhu_thread_suspend(tid, &mut c2) == sched::ERR_AGAIN,
            "susp-setctx: double suspend not EAGAIN"
        );
        let good_sp = ctx.sp;
        ctx.sp = good_sp.wrapping_add(8);
        assert!(
            api::rhu_thread_set_context(tid, &ctx) == sched::ERR_INVAL,
            "susp-setctx: sp edit not refused EINVAL"
        );
        ctx.sp = good_sp;
        ctx.pc = setctx_pad as *const () as usize as u32;
        ctx.r[0] = 0xC0DE_FACE;
        assert!(api::rhu_thread_set_context(tid, &ctx) == 0, "susp-setctx: the context edit refused");
        assert!(api::rhu_thread_resume(tid) == 0, "susp-setctx: resume refused");
        wait_until(|| SC_WITNESS.load(Ordering::Acquire) == 0xC0DE_FACE, "pad to run");
        assert!(SC_TID.load(Ordering::Acquire) == tid, "susp-setctx: pad ran off-thread");
        join_tid(tid);
        kprintln!("[api] PASS susp-setctx (sp edit refused exact, the edited pc ran the pad with the planted witness)");
    }

    static RU_WORD: AtomicU32 = AtomicU32::new(0);
    static RU_FLAG: AtomicU32 = AtomicU32::new(0);

    extern "C" fn urgent_worker(_: usize) {
        while RU_WORD.load(Ordering::Acquire) == 0 {
            let _ = api::rhu_futex_wait(RU_WORD.as_ptr(), 0, sched::DEADLINE_NONE);
        }
        RU_FLAG.store(1, Ordering::Release);
    }

    fn check_resume_urgent() {
        kprintln!("[api] resume-urgent: more-urgent suspended resume armed");
        let (_slot, tid) =
            sched::try_spawn("durg", urgent_worker, 0, 2, 4096).ok().expect("resume-urgent: spawn");
        wait_until(|| sched::probe_futex_waiting(RU_WORD.as_ptr()) == 1, "urgent worker to park");
        let mut ctx = zctx();
        assert!(api::rhu_thread_suspend(tid, &mut ctx) == 0, "resume-urgent: suspend refused");
        RU_WORD.store(1, Ordering::Release);
        let woken = api::rhu_futex_wake(RU_WORD.as_ptr(), u32::MAX);
        assert!(woken == 1, "resume-urgent: readied {}", woken);
        assert!(RU_FLAG.load(Ordering::Acquire) == 0, "resume-urgent: suspended thread ran");
        assert!(api::rhu_thread_resume(tid) == 0, "resume-urgent: resume refused");
        assert!(
            RU_FLAG.load(Ordering::Acquire) == 1,
            "resume-urgent: resume did not preempt the caller"
        );
        join_tid(tid);
        kprintln!("[api] PASS resume-urgent (resume preempted the caller before its next line)");
    }

    static ACT_MODE: AtomicU32 = AtomicU32::new(0);
    static ACT_COUNT: AtomicU32 = AtomicU32::new(0);
    static ACT_TID: AtomicI32 = AtomicI32::new(0);
    static ACT_SP: AtomicUsize = AtomicUsize::new(0);
    static ACT_SNAP: [AtomicU32; 2] = [AtomicU32::new(0), AtomicU32::new(0)];
    static H_MS: AtomicU32 = AtomicU32::new(0);

    extern "C" fn act_handler(ctx: *mut sched::RhuCtx) {
        let n = ACT_COUNT.fetch_add(1, Ordering::AcqRel);
        ACT_TID.store(api::rhu_thread_self(), Ordering::Release);
        let probe = 0u32;
        ACT_SP.store(addr_of!(probe) as usize, Ordering::Release);
        match ACT_MODE.load(Ordering::Acquire) {
            1 => {
                if n == 0 {
                    ACT_SNAP[0].store(D_PROG.load(Ordering::Acquire), Ordering::Release);
                    let r = api::rhu_activation_send(api::rhu_thread_self());
                    assert!(r == 0, "redeliver: in-handler send refused {}", r);
                } else {
                    ACT_SNAP[1].store(D_PROG.load(Ordering::Acquire), Ordering::Release);
                }
            }
            2 => {
                H_MS.store(sched::now_ms(), Ordering::Release);
            }
            3 => {
                unsafe { core::arch::asm!("vmov.f32 s0, s0", out("s0") _, options(nomem, nostack)) };
                LZ_FLAGS.store(unsafe { (*ctx).flags }, Ordering::Release);
                LZ_CTX.store(ctx as u32, Ordering::Release);
            }
            _ => {}
        }
    }

    fn check_act_refuse() {
        kprintln!("[api] act-refuse: registration door + refusal matrix armed");
        ABI_PARK.store(0, Ordering::Release);
        let tp = spawn_probe(probe_return, 4096);
        assert!(tp >= 1, "act-refuse: probe spawn failed");
        wait_until(|| sched::probe_futex_waiting(ABI_PARK.as_ptr()) == 1, "probe to park");
        let mut c = zctx();
        assert!(
            api::rhu_activation_send(tp) == sched::ERR_INVAL,
            "act-refuse: pre-registration send not EINVAL"
        );
        assert!(
            api::rhu_activation_register(None) == sched::ERR_INVAL,
            "act-refuse: null registration not EINVAL"
        );
        assert!(api::rhu_activation_register(Some(act_handler)) == 0, "act-refuse: registration refused");
        assert!(
            api::rhu_activation_register(Some(act_handler)) == sched::ERR_INVAL,
            "act-refuse: second registration not EINVAL"
        );
        let idle = sched::probe_idle_tid();
        assert!(
            api::rhu_activation_send(idle) == sched::ERR_INVAL,
            "act-refuse: send(idle) not EINVAL"
        );
        assert!(
            api::rhu_thread_suspend(idle, &mut c) == sched::ERR_INVAL,
            "act-refuse: suspend(idle) not EINVAL"
        );
        assert!(
            api::rhu_thread_suspend(api::rhu_thread_self(), &mut c) == sched::ERR_INVAL,
            "act-refuse: suspend(self) not EINVAL"
        );
        assert!(api::rhu_activation_send(4095) == sched::ERR_SRCH, "act-refuse: send(unknown) not ESRCH");
        assert!(
            api::rhu_thread_suspend(4095, &mut c) == sched::ERR_SRCH,
            "act-refuse: suspend(unknown) not ESRCH"
        );
        assert!(api::rhu_thread_resume(4095) == sched::ERR_SRCH, "act-refuse: resume(unknown) not ESRCH");
        assert!(
            api::rhu_thread_set_context(4095, &c) == sched::ERR_SRCH,
            "act-refuse: set_context(unknown) not ESRCH"
        );
        assert!(
            api::rhu_thread_suspend(tp, core::ptr::null_mut()) == sched::ERR_INVAL,
            "act-refuse: null out_ctx not EINVAL"
        );
        assert!(
            api::rhu_thread_set_context(tp, core::ptr::null()) == sched::ERR_INVAL,
            "act-refuse: null ctx not EINVAL"
        );
        assert!(
            api::rhu_thread_resume(tp) == sched::ERR_AGAIN,
            "act-refuse: resume(not-suspended) not EAGAIN"
        );
        assert!(
            api::rhu_thread_set_context(tp, &c) == sched::ERR_AGAIN,
            "act-refuse: set_context(not-suspended) not EAGAIN"
        );
        release(&ABI_PARK);
        join_tid(tp);
        ABI_PARK.store(0, Ordering::Release);
        assert!(
            api::rhu_thread_suspend(tp, &mut c) == sched::ERR_SRCH,
            "act-refuse: suspend(dead) not ESRCH"
        );
        assert!(
            api::rhu_activation_send(tp) == sched::ERR_SRCH,
            "act-refuse: send(dead) not ESRCH"
        );
        kprintln!("[api] PASS act-refuse (pre-registration send, null/second registration, self/idle suspends, idle send, unknown tids, unsuspended resume/set-context all refused exact)");
    }

    static D_PROG: AtomicU32 = AtomicU32::new(0);
    static D_REL: AtomicU32 = AtomicU32::new(0);

    extern "C" fn act_spinner(_: usize) {
        while D_REL.load(Ordering::Acquire) == 0 {
            D_PROG.fetch_add(1, Ordering::Relaxed);
        }
    }

    fn check_act_deliver() {
        kprintln!("[api] act-deliver: peer + self delivery armed");
        ACT_MODE.store(0, Ordering::Release);
        let c0 = ACT_COUNT.load(Ordering::Acquire);
        D_PROG.store(0, Ordering::Release);
        D_REL.store(0, Ordering::Release);
        let (_slot, tid) =
            sched::try_spawn("dact", act_spinner, 0, 4, 4096).ok().expect("act-deliver: spawn");
        assert!(api::rhu_activation_send(tid) == 0, "act-deliver: send refused");
        let snap = D_PROG.load(Ordering::Acquire);
        api::rhu_sleep_ns(2_000_000);
        wait_until(|| ACT_COUNT.load(Ordering::Acquire) == c0 + 1, "peer delivery");
        assert!(ACT_TID.load(Ordering::Acquire) == tid, "act-deliver: handler ran off-target");
        let (mut lo, mut b) = (0u32, 0u32);
        assert!(api::rhu_thread_stack_bounds(tid, &mut lo, &mut b) == 0, "act-deliver: bounds");
        let hsp = ACT_SP.load(Ordering::Acquire) as u32;
        assert!(hsp > lo && hsp < lo + b, "act-deliver: handler SP outside the target stack");
        wait_until(|| D_PROG.load(Ordering::Acquire) > snap, "interrupted flow to continue");

        let c1 = ACT_COUNT.load(Ordering::Acquire);
        let me = api::rhu_thread_self();
        assert!(api::rhu_activation_send(me) == 0, "act-deliver: self send refused");
        assert!(
            ACT_COUNT.load(Ordering::Acquire) == c1,
            "act-deliver: self delivery inside send (no scheduling point yet)"
        );
        api::rhu_thread_yield();
        assert!(ACT_COUNT.load(Ordering::Acquire) == c1 + 1, "act-deliver: self delivery missed");
        assert!(ACT_TID.load(Ordering::Acquire) == me, "act-deliver: self handler off-thread");
        let (mut klo, mut kb) = (0u32, 0u32);
        assert!(api::rhu_thread_stack_bounds(me, &mut klo, &mut kb) == 0, "act-deliver: own bounds");
        let ksp = ACT_SP.load(Ordering::Acquire) as u32;
        assert!(ksp > klo && ksp < klo + kb, "act-deliver: self handler SP outside own stack");
        D_REL.store(1, Ordering::Release);
        join_tid(tid);
        kprintln!("[api] PASS act-deliver (handler ran once per send on the target thread and stack, both interrupted flows continued)");
    }

    fn check_act_coalesce() {
        kprintln!("[api] act-coalesce: three sends before the target runs armed");
        ACT_MODE.store(0, Ordering::Release);
        let c0 = ACT_COUNT.load(Ordering::Acquire);
        D_PROG.store(0, Ordering::Release);
        D_REL.store(0, Ordering::Release);
        let (_slot, tid) =
            sched::try_spawn("dcoal", act_spinner, 0, 4, 4096).ok().expect("act-coalesce: spawn");
        for _ in 0..3 {
            assert!(api::rhu_activation_send(tid) == 0, "act-coalesce: send refused");
        }
        api::rhu_sleep_ns(2_000_000);
        wait_until(|| ACT_COUNT.load(Ordering::Acquire) == c0 + 1, "coalesced delivery");
        api::rhu_sleep_ns(2_000_000);
        assert!(
            ACT_COUNT.load(Ordering::Acquire) == c0 + 1,
            "act-coalesce: more than one delivery"
        );
        wait_until(|| D_PROG.load(Ordering::Acquire) > 0, "flow to continue");
        D_REL.store(1, Ordering::Release);
        join_tid(tid);
        kprintln!("[api] PASS act-coalesce (three sends delivered exactly one handler run)");
    }

    fn check_act_redeliver() {
        kprintln!("[api] act-redeliver: in-handler self-send armed");
        ACT_COUNT.store(0, Ordering::Release);
        ACT_MODE.store(1, Ordering::Release);
        D_PROG.store(0, Ordering::Release);
        D_REL.store(0, Ordering::Release);
        let (_slot, tid) =
            sched::try_spawn("dredl", act_spinner, 0, 4, 4096).ok().expect("act-redeliver: spawn");
        assert!(api::rhu_activation_send(tid) == 0, "act-redeliver: send refused");
        api::rhu_sleep_ns(2_000_000);
        wait_until(|| ACT_COUNT.load(Ordering::Acquire) == 2, "both handler runs");
        api::rhu_sleep_ns(2_000_000);
        assert!(ACT_COUNT.load(Ordering::Acquire) == 2, "act-redeliver: ran again");
        assert!(
            ACT_SNAP[0].load(Ordering::Acquire) == ACT_SNAP[1].load(Ordering::Acquire),
            "act-redeliver: the interrupted flow ran between the two handler runs"
        );
        wait_until(
            || D_PROG.load(Ordering::Acquire) > ACT_SNAP[1].load(Ordering::Acquire),
            "flow to continue after both",
        );
        D_REL.store(1, Ordering::Release);
        join_tid(tid);
        ACT_MODE.store(0, Ordering::Release);
        kprintln!("[api] PASS act-redeliver (the handler re-entered before the interrupted flow resumed, then the flow continued)");
    }

    static EI_WORD: AtomicU32 = AtomicU32::new(0);
    static EI_PROG: AtomicU32 = AtomicU32::new(0);
    static EI_R1: AtomicI32 = AtomicI32::new(99);
    static EI_C1: AtomicU32 = AtomicU32::new(99);
    static EI_R2: AtomicI32 = AtomicI32::new(99);

    extern "C" fn eintr_worker(_: usize) {
        let r = api::rhu_futex_wait(EI_WORD.as_ptr(), 0, sched::DEADLINE_NONE);
        EI_R1.store(r, Ordering::Release);
        EI_C1.store(ACT_COUNT.load(Ordering::Acquire), Ordering::Release);
        EI_PROG.store(1, Ordering::Release);
        api::rhu_futex_wake(EI_PROG.as_ptr(), u32::MAX);
        let r2 = api::rhu_futex_wait(EI_WORD.as_ptr(), 0, sched::DEADLINE_NONE);
        EI_R2.store(r2, Ordering::Release);
        EI_PROG.store(2, Ordering::Release);
        api::rhu_futex_wake(EI_PROG.as_ptr(), u32::MAX);
    }

    fn check_act_eintr() {
        kprintln!("[api] act-eintr: infinite-wait break by activation armed");
        ACT_MODE.store(0, Ordering::Release);
        let c0 = ACT_COUNT.load(Ordering::Acquire);
        let (_slot, tid) =
            sched::try_spawn("deintr", eintr_worker, 0, 4, 4096).ok().expect("act-eintr: spawn");
        wait_until(|| sched::probe_futex_waiting(EI_WORD.as_ptr()) == 1, "worker to park");
        assert!(api::rhu_activation_send(tid) == 0, "act-eintr: send refused");
        wait_until(|| EI_PROG.load(Ordering::Acquire) == 1, "broken wait to return");
        assert!(
            EI_R1.load(Ordering::Acquire) == sched::ERR_INTR,
            "act-eintr: broken wait returned {} (want RHU_EINTR)",
            EI_R1.load(Ordering::Acquire)
        );
        assert!(
            EI_C1.load(Ordering::Acquire) == c0 + 1,
            "act-eintr: the wait returned before the handler ran"
        );
        wait_until(|| sched::probe_futex_waiting(EI_WORD.as_ptr()) == 1, "worker to re-wait");
        assert!(api::rhu_futex_wake(EI_WORD.as_ptr(), 1) == 1, "act-eintr: real wake missed");
        wait_until(|| EI_PROG.load(Ordering::Acquire) == 2, "re-wait to wake clean");
        assert!(
            EI_R2.load(Ordering::Acquire) == 0,
            "act-eintr: the re-wait returned {} (want 0)",
            EI_R2.load(Ordering::Acquire)
        );
        join_tid(tid);
        kprintln!("[api] PASS act-eintr (the wait returned RHU_EINTR after the handler ran, the re-wait then woke clean)");
    }

    static RA_FLAG: AtomicU32 = AtomicU32::new(0);
    static RA_T0MS: AtomicU32 = AtomicU32::new(0);
    static RA_DONE: AtomicU32 = AtomicU32::new(0);

    extern "C" fn rearm_worker(_: usize) {
        let t0 = api::rhu_now_ns();
        RA_T0MS.store(sched::now_ms(), Ordering::Release);
        RA_FLAG.store(1, Ordering::Release);
        api::rhu_futex_wake(RA_FLAG.as_ptr(), u32::MAX);
        api::rhu_sleep_ns(200_000_000);
        let dt = api::rhu_now_ns() - t0;
        assert!(dt >= 200_000_000, "act-sleep-rearm: sleep under delivery returned early");
        RA_DONE.store(1, Ordering::Release);
        api::rhu_futex_wake(RA_DONE.as_ptr(), u32::MAX);
    }

    fn check_act_sleep_rearm() {
        kprintln!("[api] act-sleep-rearm: sleeping-target delivery + never-early armed");
        ACT_MODE.store(2, Ordering::Release);
        let c0 = ACT_COUNT.load(Ordering::Acquire);
        let (_slot, tid) =
            sched::try_spawn("drearm", rearm_worker, 0, 4, 4096).ok().expect("act-sleep-rearm: spawn");
        wait_until(|| RA_FLAG.load(Ordering::Acquire) == 1, "worker to reach its sleep");
        api::rhu_sleep_ns(20_000_000);
        assert!(api::rhu_activation_send(tid) == 0, "act-sleep-rearm: send refused");
        wait_until(|| RA_DONE.load(Ordering::Acquire) == 1, "sleep to finish not-early");
        assert!(
            ACT_COUNT.load(Ordering::Acquire) == c0 + 1,
            "act-sleep-rearm: handler run count wrong"
        );
        let early = H_MS.load(Ordering::Acquire).wrapping_sub(RA_T0MS.load(Ordering::Acquire));
        assert!(early < 200, "act-sleep-rearm: handler did not run early ({} ms in)", early);
        ACT_MODE.store(0, Ordering::Release);
        join_tid(tid);
        kprintln!("[api] PASS act-sleep-rearm (the handler ran early, the sleep still returned not-early)");
    }

    static LZ_FLAGS: AtomicU32 = AtomicU32::new(0xFFFF);
    static LZ_CTX: AtomicU32 = AtomicU32::new(0);
    static LZ_SWITCHES: AtomicU32 = AtomicU32::new(0);
    static LZ_DONE: AtomicU32 = AtomicU32::new(0);

    extern "C" fn lazy_target(_: usize) {
        let sw0 = sched::switch_count();
        for _ in 0..8 {
            api::rhu_thread_yield();
        }
        LZ_SWITCHES.store(sched::switch_count().wrapping_sub(sw0), Ordering::Release);
        LZ_DONE.store(1, Ordering::Release);
    }

    fn check_act_lazy_fp() {
        kprintln!("[api] act-lazy-fp: floating-point handler + basic-frame target + the switch-entry witness armed");
        let (w0, _, _) = sched::probe_lazy_fp_witness();
        assert!(w0 == 0, "act-lazy-fp: {} abandoned lazy floating-point states seen before the check", w0);
        ACT_MODE.store(3, Ordering::Release);
        let c0 = ACT_COUNT.load(Ordering::Acquire);
        LZ_DONE.store(0, Ordering::Release);
        let (_slot, tid) =
            sched::try_spawn("dlazy", lazy_target, 0, 4, 4096).ok().expect("act-lazy-fp: spawn");
        assert!(api::rhu_activation_send(tid) == 0, "act-lazy-fp: send refused");
        api::rhu_sleep_ns(2_000_000);
        wait_until(|| LZ_DONE.load(Ordering::Acquire) == 1, "the target's yields after its resume");
        join_tid(tid);
        ACT_MODE.store(0, Ordering::Release);
        assert!(ACT_COUNT.load(Ordering::Acquire) == c0 + 1, "act-lazy-fp: handler run count wrong");
        assert!(ACT_TID.load(Ordering::Acquire) == tid, "act-lazy-fp: handler ran off-target");
        let flags = LZ_FLAGS.load(Ordering::Acquire);
        assert!(flags == 0, "act-lazy-fp: the target's interrupted frame was not basic (ctx flags {:#x})", flags);
        let sw = LZ_SWITCHES.load(Ordering::Acquire);
        assert!(sw >= 8, "act-lazy-fp: the target switched out {} times after its resume (wanted at least 8)", sw);
        let (count, fpcar, psp) = sched::probe_lazy_fp_witness();
        assert!(
            count == 0,
            "act-lazy-fp: {} abandoned lazy floating-point states seen at basic-frame switch entries after the resume (the last pending save at {:#010x}, the outgoing psp {:#010x}; the handler's ctx at {:#010x})",
            count,
            fpcar,
            psp,
            LZ_CTX.load(Ordering::Acquire)
        );
        kprintln!("[api] PASS act-lazy-fp (the handler ran one floating-point instruction on a target whose interrupted frame was basic, the target resumed and yielded eight times after it -- eight switch entries with its own frame basic -- and the witness counted no abandoned lazy floating-point state)");
    }

    /// The whole desc as bytes for the miss-writes-nothing and
    /// one-storage-two-doors compares (repr(C), all-u32/pointer
    /// fields on a 32-bit target: no padding, so a byte compare is a
    /// field compare).
    fn desc_bytes(d: &registry::ModuleDesc) -> &[u8] {
        unsafe {
            core::slice::from_raw_parts(
                d as *const registry::ModuleDesc as *const u8,
                core::mem::size_of::<registry::ModuleDesc>(),
            )
        }
    }

    /// A 0xA5-filled desc: the sentinel a refused query must leave
    /// byte-intact (zero would be reproducible by a zeroing bug; the
    /// pointer fields hold the pattern as an address value and are
    /// never dereferenced).
    fn sentinel_desc() -> registry::ModuleDesc {
        unsafe {
            let mut d = core::mem::MaybeUninit::<registry::ModuleDesc>::uninit();
            core::ptr::write_bytes(
                d.as_mut_ptr() as *mut u8,
                0xA5,
                core::mem::size_of::<registry::ModuleDesc>(),
            );
            d.assume_init()
        }
    }

    fn check_module_find() {
        kprintln!("[api] mod-find: containing-module lookup armed");
        unsafe extern "C" {
            static __stext: u32;
            static __etext: u32;
            static __rhu_arena_lo: u32;
        }
        let stext = addr_of!(__stext) as u32;
        let etext = addr_of!(__etext) as u32;
        let arena_lo = addr_of!(__rhu_arena_lo) as u32;
        assert!(
            api::rhu_module_find(stext, core::ptr::null_mut()) == sched::ERR_INVAL,
            "mod-find: null out not refused EINVAL"
        );
        let mut out = sentinel_desc();
        assert!(api::rhu_module_find(stext, &mut out) == 0, "mod-find: __stext missed module 0");
        let door: extern "C" fn(u32, *mut registry::ModuleDesc) -> i32 = api::rhu_module_find;
        let probe = (door as *const () as u32) & !1;
        assert!(
            api::rhu_module_find(probe, &mut out) == 0,
            "mod-find: own door address missed module 0"
        );
        assert!(
            out.text_lo == stext && out.text_hi == etext,
            "mod-find: returned text range is not the link bracket"
        );
        let mut miss = sentinel_desc();
        let miss0 = sentinel_desc();
        assert!(
            api::rhu_module_find(etext, &mut miss) == sched::ERR_SRCH,
            "mod-find: __etext not refused (half-open hi violated)"
        );
        assert!(
            api::rhu_module_find(arena_lo, &mut miss) == sched::ERR_SRCH,
            "mod-find: arena address not refused ESRCH"
        );
        assert!(
            desc_bytes(&miss) == desc_bytes(&miss0),
            "mod-find: a miss wrote into the out desc"
        );
        kprintln!("[api] PASS mod-find (kernel text resolved to module 0 half-open exact, arena address refused ESRCH, miss wrote nothing)");
    }

    fn check_module_roots() {
        kprintln!("[api] mod-roots: module 0 root-source enumeration armed");
        unsafe extern "C" {
            static __stext: u32;
            static __etext: u32;
            static __sdata: u32;
            static __edata: u32;
            static __sbss: u32;
            static __ebss: u32;
            static __rhu_frozen_lo: u32;
            static __rhu_frozen_hi: u32;
            static __exidx_start: u32;
            static __exidx_end: u32;
            static __rhu_tdata_lo: u32;
            static __rhu_tdata_hi: u32;
        }
        assert!(
            api::rhu_module_roots(0, core::ptr::null_mut()) == sched::ERR_INVAL,
            "mod-roots: null out not refused EINVAL"
        );
        let mut miss = sentinel_desc();
        let miss0 = sentinel_desc();
        assert!(
            api::rhu_module_roots(-1, &mut miss) == sched::ERR_SRCH,
            "mod-roots: negative handle not ESRCH"
        );
        let mut out = sentinel_desc();
        assert!(api::rhu_module_roots(0, &mut out) == 0, "mod-roots: module 0 refused");
        assert!(
            out.text_lo == addr_of!(__stext) as u32 && out.text_hi == addr_of!(__etext) as u32,
            "mod-roots: text range is not the link bracket"
        );
        assert!(
            out.statics_lo[0] == addr_of!(__sdata) as u32
                && out.statics_hi[0] == addr_of!(__edata) as u32
                && out.statics_lo[1] == addr_of!(__sbss) as u32
                && out.statics_hi[1] == addr_of!(__ebss) as u32
                && out.statics_lo[2] == 0
                && out.statics_hi[2] == 0
                && out.statics_lo[3] == 0
                && out.statics_hi[3] == 0,
            "mod-roots: statics ranges are not the link windows"
        );
        let flo = core::hint::black_box(addr_of!(__rhu_frozen_lo) as u32);
        let fhi = core::hint::black_box(addr_of!(__rhu_frozen_hi) as u32);
        let xlo = core::hint::black_box(addr_of!(__exidx_start) as u32);
        let xhi = core::hint::black_box(addr_of!(__exidx_end) as u32);
        assert!(
            out.frozen_lo == flo && out.frozen_hi == fhi,
            "mod-roots: frozen range is not the frozen bracket"
        );
        assert!(
            out.frozen_lo == out.frozen_hi,
            "mod-roots: frozen range not empty in a base-family image"
        );
        assert!(
            out.exidx_lo == xlo && out.exidx_hi == xhi,
            "mod-roots: exidx range is not the exidx bracket"
        );
        assert!(
            out.exidx_lo == out.exidx_hi,
            "mod-roots: exidx range not empty in a base-family image"
        );
        let tlo = core::hint::black_box(addr_of!(__rhu_tdata_lo) as u32);
        let thi = core::hint::black_box(addr_of!(__rhu_tdata_hi) as u32);
        assert!(
            out.tls_bytes == thi - tlo,
            "mod-roots: tls size is not the tdata bracket span"
        );
        assert!(out.tls_bytes == 0, "mod-roots: tls not empty in a base-family image");
        assert!(out.tls_init.is_null(), "mod-roots: empty tls carries a non-null init image");
        assert!(!out.name.is_null(), "mod-roots: module 0 name wrong");
        let mut via_find = sentinel_desc();
        assert!(
            api::rhu_module_find(out.text_lo, &mut via_find) == 0,
            "mod-roots: find cross-check missed"
        );
        assert!(
            desc_bytes(&via_find) == desc_bytes(&out),
            "mod-roots: find and roots disagree about module 0"
        );
        let mut w = sentinel_desc();
        for h in 1..4 {
            assert!(api::rhu_module_roots(h, &mut w) == 0, "mod-roots: live handle {} refused", h);
        }
        assert!(
            api::rhu_module_roots(4, &mut miss) == sched::ERR_SRCH,
            "mod-roots: walk terminator not ESRCH"
        );
        assert!(
            desc_bytes(&miss) == desc_bytes(&miss0),
            "mod-roots: a refusal wrote into the out desc"
        );
        kprintln!("[api] PASS mod-roots (module 0 ranges tie the link brackets, frozen, exidx, and tls empty in this family, walk ends ESRCH at the table count)");
    }


    fn check_console_pair() {
        kprintln!("[api] console-pair: door write + no-input read armed");
        let line = b"[api] console-door: this line reached the wire through rhu_console_write\n";
        assert!(
            api::rhu_console_write(line.as_ptr(), line.len() as u32) == line.len() as i32,
            "console-pair: write door returned the wrong count"
        );
        assert!(
            api::rhu_console_write(core::ptr::null(), 4) == sched::ERR_INVAL,
            "console-pair: null write not refused EINVAL"
        );
        assert!(
            api::rhu_console_write(line.as_ptr(), (i32::MAX as u32) + 1) == sched::ERR_INVAL,
            "console-pair: oversize write not refused EINVAL"
        );
        assert!(api::rhu_console_write(line.as_ptr(), 0) == 0, "console-pair: zero write not 0");
        let mut buf = [0xA5u8; 16];
        assert!(
            api::rhu_console_read(buf.as_mut_ptr(), 16) == 0,
            "console-pair: no-input read not 0"
        );
        assert!(buf.iter().all(|&b| b == 0xA5), "console-pair: empty read wrote into the buffer");
        assert!(
            api::rhu_console_read(core::ptr::null_mut(), 4) == sched::ERR_INVAL,
            "console-pair: null read not refused EINVAL"
        );
        kprintln!("[api] PASS console-pair (door write returned exact and hit the wire, no-input read returned 0 writing nothing, null and oversize refused)");
    }

    fn check_entropy() {
        kprintln!("[api] entropy-honesty: software-stream doors armed");
        let mut a = [0u8; 16];
        let mut b = [0u8; 16];
        assert!(api::rhu_entropy(a.as_mut_ptr(), 16) == 0, "entropy: first draw refused");
        assert!(api::rhu_entropy(b.as_mut_ptr(), 16) == 0, "entropy: second draw refused");
        assert!(a != b, "entropy: two draws returned identical bytes");
        assert!(a.iter().any(|&x| x != 0), "entropy: draw left the buffer all-zero");
        assert!(
            api::rhu_entropy(core::ptr::null_mut(), 16) == sched::ERR_INVAL,
            "entropy: null draw not refused EINVAL"
        );
        assert!(api::rhu_entropy(a.as_mut_ptr(), 0) == 0, "entropy: zero-length draw not 0");
        assert!(api::rhu_entropy_is_hardware() == 0, "entropy: hardware claim on a software-stream board");
        kprintln!("[api] PASS entropy-honesty (draws filled and distinct, null refused, zero-length trivial, hardware query answers 0 on this class)");
    }
}
