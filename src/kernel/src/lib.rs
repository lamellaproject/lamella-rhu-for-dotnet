//! The Lamella RHU kernel: boot into `kmain`, bring the scheduler
//! online, prove the machine with selftests, and arm the panic/fault
//! funnel. The whole kernel lives in this lib so one source tree feeds
//! both artifacts: the gate binary (rlib into `src/main.rs`) and the
//! payload-hosting staticlib (external `ld -T` merge with a frozen ILC
//! object).
//!
//! Console discipline: functional lines are deterministic -- stable
//! prefixes, no timestamps -- so transcripts diff byte-for-byte against
//! goldens and against a second hosting of the same payload.

#![no_std]

#[macro_use]
mod console;
mod adc;
mod adc_math;
mod api;
mod arena;
mod board;
mod boot;
mod bus;
mod bus_math;
mod calendar;
mod calendar_math;
mod claims;
mod critical;
mod entropy;
mod exit_word;
mod funnel;
mod gpio;
mod mpu;
mod gpio_math;
#[cfg(feature = "fpemul")]
mod fpemul;
mod header;
mod kheap;
#[cfg(feature = "payload")]
mod abi_window;
#[cfg(feature = "payload")]
mod payload;
mod pwm;
mod pwm_math;
mod ram;
mod registry;
mod sched;
mod uart;
/// The UART pin words' arithmetic (the pin override's kernel half): the effective pair of a block from the row's pair, the chip's
/// legal sets and the project's two words, pure, so the host tier proves what every claim runs.
#[cfg_attr(not(rhu_uart_rows), allow(dead_code))]
mod uart_math;
/// The block-device doors: the device table and the five verbs on
/// every row; the one device the STM32F769 row's card socket.
mod blk;
/// The SD host trait and the card layer over it, compiled
/// where a row has a socket: the STM32F769's SDMMC2 host implements the trait.
#[cfg(rhu_sd_socket)]
mod sdhost;
#[cfg(rhu_sd_socket)]
mod sdcard;
/// The FAT filesystem layer over the card, compiled for its one caller, the
/// api checks of the socket row.
#[cfg(all(rhu_sd_socket, feature = "api-smoke", not(feature = "test-sd-doctored")))]
mod fat;

/// The api flavor's band-interrupt control line and its handler, folded into
/// the vector table beside the board's own lines; empty in every other image.
#[cfg(feature = "api-smoke")]
pub const TEST_IRQS: &[(usize, unsafe extern "C" fn())] = &[(api_smoke::FP_BAND_LINE, api_smoke::__rhu_fp_band_irq)];
/// The fault-in-handler flavor's line and its handler: the same spare line,
/// the two flavors never in one image.
#[cfg(all(feature = "test-fault-in-handler", not(feature = "api-smoke")))]
pub const TEST_IRQS: &[(usize, unsafe extern "C" fn())] = &[(board::SPARE_IRQ, fault_in_handler::__rhu_fault_in_handler_irq)];
#[cfg(not(any(feature = "api-smoke", feature = "test-fault-in-handler")))]
pub const TEST_IRQS: &[(usize, unsafe extern "C" fn())] = &[];
mod semihost;
/// The instruction cache's boot step on the STM32F769 row: on under the
/// quad-SPI window alone.
#[cfg(rhu_qspi_window)]
mod cache;
/// The handler stack's watermark: the boot stack reserve's pattern fill and its read-out.
mod stack_watermark;

/// The floating-point posture of this row, one line at boot: the Cortex-M7 class
/// computes double precision in hardware; the M33 class (the single-precision
/// FPU) has the double-precision forms computed by the kernel's emulation.
#[cfg(feature = "fpemul")]
const FPU_LINE: &str = "single precision in hardware; the double-precision forms are emulated by the kernel";
#[cfg(not(feature = "fpemul"))]
const FPU_LINE: &str = "double precision in hardware";

use core::ptr::{addr_of, addr_of_mut, read_volatile, write_volatile};

/// Compiled-in image flavor, stated on the boot line: the artifact
/// itself says what it is, and the gate asserts it per case.
const FLAVOR: &str = if cfg!(feature = "test-panic") {
    "test-panic"
} else if cfg!(feature = "test-fault") {
    "test-fault"
} else if cfg!(feature = "test-fault-in-handler") {
    "test-fault-in-handler"
} else if cfg!(feature = "test-h7-pmic-read") {
    "test-h7-pmic-read"
} else if cfg!(feature = "test-kheap-loud") {
    "test-kheap-loud"
} else if cfg!(feature = "test-tickcost") { "test-tickcost"
} else if cfg!(feature = "test-stackovfl") { "test-stackovfl"
} else if cfg!(feature = "test-nullpage") { "test-nullpage"
} else if cfg!(feature = "sched-smoke") {
    "sched-smoke"
} else if cfg!(feature = "api-smoke") {
    "api-smoke"
} else if cfg!(feature = "payload") {
    "payload"
} else if cfg!(feature = "unhosted-main") {
    "unhosted-main"
} else {
    "boot"
};

/// Lands in .data -- proves the Reset LMA-to-VMA copy ran.
static mut DATA_CANARY: u32 = 0xDA7A_C0DE;
/// Lands in .bss -- proves the Reset zero-fill ran.
static mut BSS_CANARY: u32 = 0;

// Volatile-loaded operands so the float math executes at run time.
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

    // A quiet image prints ONE line -- the version, the board, its chip
    // and its clock -- and nothing else of the boot report (a verbose image
    // prints the report below instead). THE FORM:
    // `Lamella RHU v<version>, <board> (<chip>, <clock> MHz)`.
    if console::level() == console::LEVEL_QUIET {
        klog!(
            console::LEVEL_QUIET,
            "Lamella RHU v{}, {} ({}, {} MHz)",
            env!("CARGO_PKG_VERSION"),
            board::NAME,
            board::CHIP,
            board::SYSCLK_HZ / 1_000_000
        );
    }
    console::say_refusals();
    kprintln!(
        "rhu {} -- Lamella RHU: runtime-hosting unikernel for .NET on Cortex-M",
        env!("CARGO_PKG_VERSION")
    );
    kprintln!("[boot] {} | {} | flavor: {}", board::NAME, console::clause(), FLAVOR);
    kprintln!(
        "[boot] VTOR={:#010x} CPUID={:#010x} ({} r{}p{})",
        vtor,
        cpuid,
        cpu_name(cpuid),
        (cpuid >> 20) & 0xF,
        cpuid & 0xF
    );

    // The memory protection unit: the first page no-access on every row
    // and, on the v7-M rows, the slot for the running thread's stack guard
    // (the null-page guard and the stack floor); the posture read back on its own boot line, pinned
    // by the harness. Before the scheduler, so no thread ever runs with
    // the page readable.
    mpu::banner(&mpu::init());

    // Gated clock postures announce themselves from register read-back
    // (boards on their boring default return None and the recorded
    // transcript surface stands). The assert is the in-image tripwire:
    // a silently-failed posture prints its wrong line above, then dies
    // loudly here -- never a silent pass at the wrong clock.
    if let Some(ck) = board::clock_readback() {
        kprintln!(
            "[boot] clock: {} | SYSCLK {} Hz (SWS={}, {}/{} x{} /{}) | flash WS {} | {}",
            ck.posture,
            ck.sysclk_hz,
            ck.sws,
            ck.src,
            ck.pllm,
            ck.plln,
            ck.pllp,
            ck.flash_ws,
            ck.boost
        );
        assert!(
            ck.sysclk_hz == board::SYSCLK_HZ,
            "clock posture mismatch: registers say {} Hz, build says {} Hz",
            ck.sysclk_hz,
            board::SYSCLK_HZ
        );
    }

    // The board's RAM ranges, one line on every row: the internal
    // SRAM everywhere; on the rows that carry the part, the PSRAM brought
    // up through the flash interface's second chip select and proved by
    // read-back -- here, with the clock final and before any
    // interrupt source is enabled, the crate's preconditions met by
    // position. A refusal is a loud death inside the call.
    #[cfg(rhu_psram_window)]
    {
        let w = ram::psram_bring_up();
        kprintln!(
            "[boot] ram: 2 ranges -- {}; {} (the flash interface's second select on GPIO {}; KGD {:#04x}, read-back held; {})",
            ram::Words(0),
            ram::Words(1),
            board::facts::PSRAM_SELECT_LINE,
            w.id.kgd,
            arena::INIT_MODE_WORDS
        );
    }
    // On the STM32F769 row, the board's quad-SPI flash: brought up and
    // mapped at 0x90000000 when the map this image was linked under declares
    // the window (its brackets), proved by the two-path read-back and the
    // window header's matched-set words, before the SDRAM and before any
    // payload address is touched; the block stays disabled under the other
    // maps. Then the instruction cache, on under the window alone (or under
    // the flight-only measurement control test-icache).
    #[cfg(rhu_qspi_window)]
    {
        let w = board::stm32f7_qspi::boot();
        kprintln!("[boot] flash: {}", board::stm32f7_qspi::Banner(w));
        cache::boot(w.is_some() || cfg!(feature = "test-icache"));
        kprintln!("[boot] icache: {}", cache::Banner);
    }
    // On the STM32F769 row, the SDRAM behind the flexible memory controller,
    // brought up and proved by read-back at the same position.
    #[cfg(rhu_sdram_window)]
    {
        ram::sdram_bring_up();
        kprintln!(
            "[boot] ram: 2 ranges -- {}; {} (the flexible memory controller's bank 1 at {} MHz, CL {}; read-back held; {})",
            ram::Words(0),
            ram::Words(1),
            board::stm32f7_sdram::WORDS.sdclk_hz / 1_000_000,
            board::stm32f7_sdram::WORDS.cas,
            arena::INIT_MODE_WORDS
        );
    }
    // On the two emulator rows, the machines' PSRAM bank, proved by
    // read-back at the same position (nothing to program).
    #[cfg(rhu_mps2_window)]
    {
        ram::bank_bring_up();
        kprintln!(
            "[boot] ram: 2 ranges -- {}; {} (the machine's bank; read-back held; {})",
            ram::Words(0),
            ram::Words(1),
            arena::INIT_MODE_WORDS
        );
    }
    #[cfg(not(rhu_extram_window))]
    kprintln!("[boot] ram: 1 range -- {}", ram::Words(0));
    // The hosted runtime's rehydrated-data region, placed at the head of
    // the external RAM region by the rows' maps: zeroed here, with the
    // external RAM proved and before the arena takes its window past it.
    #[cfg(rhu_extram_window)]
    boot::zero_hydrated();
    // On the STM32F769 row, the microSD socket: the card identified where
    // one is seated and the banner's sd line either way; a refused card is
    // not a boot death (the doors answer their codes).
    #[cfg(rhu_sd_socket)]
    blk::boot();

    // The entropy posture and the calendar posture, one line each,
    // constant per row and clock posture: the hardware source started
    // where one serves, the real-time clock read where one was set.
    entropy::init();
    kprintln!("[boot] entropy: {}", entropy::banner());
    calendar::init_at_boot();
    kprintln!("[boot] calendar: {}", board::RTC_LINE);
    kprintln!("[boot] fpu: {}", FPU_LINE);
    // The GPIO posture, constant per image: the line space and the board's
    // own claims (the device seam).
    gpio::init();
    bus::init();
    kprintln!("[boot] gpio: {}", gpio::banner());
    // The PWM and ADC posture, constant per image: the lines a timer can
    // drive and the converter channels (the device seam).
    pwm::init();
    adc::init();
    uart::init();
    kprintln!("[boot] pwm: {} lines on {} timers; adc: {} channels on {} blocks", pwm::LINES, pwm::TIMERS, adc::CHANNELS, adc::BLOCKS);
    // The network posture, constant per image, on a row with a medium:
    // the stack built over the medium, the pool's figure stated; no
    // address on the banner (the transcript-surface rule: the hardware
    // address goes to the witness and the info door).
    #[cfg(rhu_net_rows)]
    {
        net::init();
        kprintln!("[boot] net: {} medium, pool {} B, {} stream + {} datagram sockets", net::KIND_WORD, net::POOL_BYTES, net::STREAMS, net::DATAGRAMS);
    }

    arena::init();
    kheap::init();
    registry::init_module0();
    sched::init();
    let main_slot = sched::spawn("kmain", kthread_main, 0, 3, board::KMAIN_STACK_BYTES);
    // The network thread: one notch more urgent than the hosted flat
    // level, 8 KB of the pool; it runs only once the scheduler starts.
    #[cfg(rhu_net_rows)]
    net::start();
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
    any(feature = "test-panic", feature = "test-fault", feature = "test-fault-in-handler", feature = "test-h7-pmic-read", feature = "payload", feature = "test-kheap-loud", feature = "test-tickcost", feature = "test-stackovfl", feature = "test-nullpage", feature = "api-smoke", feature = "unhosted-main"),
    allow(unreachable_code)
)]
extern "C" fn kthread_main(_: usize) {
    selftest();

    #[cfg(feature = "test-h7-pmic-read")]
    pmic_read::run();

    #[cfg(feature = "sched-smoke")]
    smoke::run();

    #[cfg(all(feature = "api-smoke", feature = "test-gpio-button"))]
    api_smoke::button_flight();
    #[cfg(all(feature = "api-smoke", any(feature = "test-uart-loopback", feature = "test-uart-jumper")))]
    api_smoke::uart_flight();
    #[cfg(all(feature = "api-smoke", not(any(feature = "test-gpio-button", feature = "test-uart-loopback", feature = "test-uart-jumper"))))]
    api_smoke::run();

    // The payload flavor hands this thread to the hosted image and
    // never returns; the machine exits with the payload's own verdict.
    #[cfg(feature = "payload")]
    payload::run();

    #[cfg(feature = "test-panic")]
    panic!("deliberate panic (flavor test-panic)");

    #[cfg(feature = "test-fault")]
    {
        kprintln!("[test-fault] executing udf #0 -- expect a UsageFault dump");
        unsafe { core::arch::asm!("udf #0") };
    }

    // The designed death inside an interrupt handler (the deepest path of the
    // handler stack: the handler's frame, the fault's stacking, the funnel's
    // report): the row's spare interrupt line, enabled at the lowest urgency
    // and pended from this thread; its handler executes an undefined
    // instruction, so the UsageFault raised inside it escalates to the
    // HardFault the funnel names from handler mode, and the watermark line at
    // the end reads the depth.
    #[cfg(feature = "test-fault-in-handler")]
    fault_in_handler::run();

    // The stack-overflow designed death (the v8-M rows' stack limit): a
    // thread descends past its floor; the limit register must raise the
    // UsageFault with the overflow line. The survived shape prints the
    // UNREACHABLE line and exits 3.
    #[cfg(feature = "test-stackovfl")]
    stackovfl::run();

    #[cfg(feature = "test-nullpage")]
    nullpage::run();

    #[cfg(feature = "test-kheap-loud")]
    {
        kprintln!("[kheap-loud] corrupting a free block header, then allocating -- expect a KERNEL PANIC");
        let a = api::rhu_kheap_alloc(64, 8);
        assert!(!a.is_null(), "kheap-loud: seed alloc failed");
        let g = api::rhu_kheap_alloc(64, 8);
        assert!(!g.is_null(), "kheap-loud: guard alloc failed");
        api::rhu_kheap_free(a); // a free block now heads the list
        kheap::probe_corrupt_first_free();
        let _ = api::rhu_kheap_alloc(64, 8); // this walk must die loud
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
    // The api world's machine exit takes the hosting door -- the
    // genuine-path exercise of rhu_exit: the gate's exit-status assert
    // proves the door terminated the machine with the handed code.
    #[cfg(feature = "api-smoke")]
    {
        // The doctored control's terminal half: a form from a range no module
        // registers, so the emulation's funnel must refuse it and this image
        // must never reach the exit door.
        #[cfg(feature = "test-fpemul-doctored")]
        {
            kprintln!("[api] module-code-control: a form from an unregistered range follows; the funnel must refuse it and this image must not reach the exit door");
            let _ = api_smoke::module_code::run_unregistered();
        }
        kprintln!("[api] exit-door: handing the machine exit to rhu_exit(0)");
        api::rhu_exit(0);
    }
    // The unhosted-main flavor: the board's tenancy for the program registered
    // first (the wireless module's interrupt bit and wake word, where the board
    // has one; a refusal is one console line), then the kmain thread handed to
    // the main the program defines -- declared, never defined -- and its code the machine's exit.
    #[cfg(feature = "unhosted-main")]
    {
        unsafe extern "C" { fn rhu_unhosted_main() -> i32; }
        let tenancy = board::unhosted_tenant_init();
        if tenancy != 0 { kprintln!("[unhosted-main] the wireless tenancy was refused: {}", tenancy); }
        let code = unsafe { rhu_unhosted_main() };
        kprintln!("[unhosted-main] entry returned {}", code);
        semihost::end(exit_word::STATE_RETURNED, code)
    }
    // A boot image whose two streams both name the memory
    // console (no wire) asserts before it exits that the mirror holds the
    // banner's first line -- a mismatch exits 1 (the api gate pins the exit
    // of its boot-memory image).
    if console::channel(console::Stream::Log) == console::CH_MEMORY
        && console::channel(console::Stream::Console) == console::CH_MEMORY
        && !console::mirror_holds(concat!("rhu ", env!("CARGO_PKG_VERSION"), " -- Lamella RHU").as_bytes())
    {
        semihost::exit(1)
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

    // kmain runs as a thread: process stack active, carved from the
    // scheduler's pool. (The printed line is constant text -- the pool
    // address is a layout rider; the checks read the live values.)
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

    // The tick clock advances and sleep never returns early -- the
    // sleeper's own clock is the authority.
    let t0 = sched::now_ms();
    sched::sleep_ms(25);
    let dt = sched::now_ms().wrapping_sub(t0);
    assert!(dt >= 25, "sleep_ms(25) returned after {} ms", dt);
    kprintln!("[selftest] SysTick advances + sleep(25 ms) never early: OK");
}

// ---------------- scheduler smoke (feature sched-smoke) ----------------

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
        // Equal priority on purpose: a correct fixed-priority scheduler
        // STARVES a lower-priority spinner while kmain busy-loops, so
        // the preemption proofs need an equal-priority peer sharing the
        // slice via round-robin.
        sched::spawn("spin", spinner, 0, 3, 2 * 1024);

        sched::sleep_ms(150);
        let woke = WOKE.load(Ordering::Relaxed);
        assert!(woke == 3, "workers woke: {}", woke);
        kprintln!("[smoke] PASS sleep-order (3 workers woke in order)");
        assert!(SPINS.load(Ordering::Relaxed) > 0, "spinner never ran");
        kprintln!("[smoke] PASS rr-preempt (spinner ran while kmain slept)");

        // Tick-driven timeslicing: no voluntary yield here, so the
        // spinner only advances if SysTick preempts this busy loop and
        // round-robin rotates to the equal-priority peer.
        let s0 = SPINS.load(Ordering::Relaxed);
        busy_wait_ms(30);
        assert!(SPINS.load(Ordering::Relaxed) != s0, "no preemption during busy loop");
        kprintln!("[smoke] PASS tick-preempt (equal-priority peer advanced during busy loop)");

        // Callee-saved FP state across preemption: the parker arms its
        // FP context and parks distinctive words in s16/s17; the
        // clobber thread deliberately dirties the same registers every
        // slice; tick rotation interleaves all three of us. If PendSV's
        // extended-frame save/restore is broken, the parker reads back
        // the clobber's values -- the silent-corruption failure mode
        // made loud.
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

// ---------------- hosting-API smoke (feature api-smoke) ----------------

#[cfg(feature = "api-smoke")]
#[cfg_attr(any(feature = "test-gpio-button", feature = "test-uart-loopback", feature = "test-uart-jumper"), allow(dead_code))]
mod api_smoke {
    use core::ffi::c_void;
    use core::ptr::{addr_of, addr_of_mut, read_volatile, write_volatile};
    use core::sync::atomic::{AtomicI32, AtomicU32, AtomicUsize, Ordering};

    use crate::critical::with_cs;
    use crate::{api, arena, board, calendar, calendar_math, entropy, gpio, gpio_math, kheap, ram, registry, sched};
    #[cfg(rhu_sd_socket)]
    use crate::blk;
    #[cfg(all(rhu_sd_socket, not(feature = "test-sd-doctored")))]
    use crate::{fat, sdcard, sdhost};

    /// The network doors' eight checks, on the rows with a medium.
    #[cfg(rhu_net_rows)]
    mod net_checks;

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
    /// position-independent. (Without the positioning the floored
    /// control can pass when the arm
    /// happens to sit early in its tick; the positioning below
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
        check_far_deadline();
        check_wake();
        check_wake_priority();
        check_mutex();
        check_quiescent();
        check_arena_carve();
        check_arena_zero_birth();
        check_arena_rezero();
        check_arena_rezero_band();
        check_arena_audit();
        check_ram_range();
        #[cfg(rhu_extram_window)]
        check_hydrated_home();
        #[cfg(armv8m)]
        check_excl_posture();
        #[cfg(rhu_extram_window)]
        check_excl_window1();
        #[cfg(rhu_qspi_window)]
        check_qspi_window();
        #[cfg(rhu_qspi_window)]
        check_xip_posture();
        #[cfg(rhu_ram_init_background)]
        check_ram_init_frontier();
        #[cfg(rhu_sd_socket)]
        check_sd_identify();
        #[cfg(all(rhu_sd_socket, not(feature = "test-sd-doctored")))]
        {
            check_sd_boot_sector();
            check_sd_sdram_buffer();
            check_sd_write_back();
            check_fs_mount();
            check_fs_walk();
            check_fs_roundtrip();
        }
        check_arena_headroom();
        check_kheap_basic();
        check_kheap_coalesce();
        check_kheap_bound();
        check_tls_blocks();
        check_tls_base_door();
        check_tls_dtors();
        check_thread_abi();
        check_stack_min();
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
        check_console_mirror();
        check_log_level();
        check_uart_enumerate();
        check_uart_claim_twice();
        check_uart_frame();
        check_uart_loopback();
        check_uart_status_and_flush();
        check_uart_timeout();
        #[cfg(rhu_net_rows)]
        net_checks::run();
        check_entropy();
        check_entropy_secure();
        check_host_band();
        check_calendar();
        check_gpio_claim();
        check_gpio_refuse();
        check_gpio_edge();
        #[cfg(not(any(feature = "board-mps2-an500", feature = "board-mps2-an505")))]
        check_gpio_edge_one_kind();
        #[cfg(all(rhu_bus_models, not(feature = "test-bus-doctored")))]
        check_bus_i2c();
        #[cfg(all(rhu_bus_models, not(feature = "test-bus-doctored")))]
        check_bus_spi();
        #[cfg(all(rhu_bus_models, not(feature = "test-bus-doctored")))]
        check_bus_tenancy();
        #[cfg(all(rhu_bus_models, feature = "test-bus-doctored"))]
        check_bus_doctored();
        #[cfg(all(rhu_bus_rows, not(rhu_bus_models)))]
        check_bus_pads();
        #[cfg(rhu_pwm_adc_rows)]
        check_pwm_claim();
        #[cfg(rhu_pwm_adc_rows)]
        check_adc_claim();
        #[cfg(rhu_pwm_adc_rows)]
        check_pwm_adc_loop();
        #[cfg(all(rhu_pwm_adc_models, feature = "test-adc-flagged"))]
        check_adc_flagged();
        #[cfg(not(rhu_pwm_adc_rows))]
        check_pwm_adc_none();
        check_fp_double();
        check_fp_band();
        #[cfg(feature = "fpemul")]
        check_fp_frames();
        #[cfg(feature = "fp-cost")]
        check_fp_cost();
        check_module_code();
        check_exit_word();
        check_tid_wrap();
        check_null_read();
        check_handler_stack();
    }

    // Check 39: the exit word -- the console mirror's header word a reader
    // over the debug port reads to learn how the program ended: 0 while the
    // program runs; the state and the code written by the exit paths in one
    // store. The check pins the word's offset by symbol, writes through the
    // exit paths' own store and reads the word back by symbol against the
    // contract's numbers, refuses the words outside the contract, then
    // restores 0 (the machine's own exit writes the final word after it).
    fn check_exit_word() {
        use crate::board::ring;
        use crate::exit_word::{decode, magic, name, CODE_OFFSET, MAGIC_OFFSET, MAGIC_RETURNED, MAGIC_RUNNING, MAGIC_ABORTED, STATE_RETURNED, STATE_RUNNING, STATE_ABORTED};
        kprintln!("[api] exit-word: the header's magic and exit code read by symbol against the contract's words");
        let log = addr_of!(ring::__rhu_console_log);
        let (magic_off, code_off) = unsafe { (addr_of!((*log).magic) as usize - log as usize, addr_of!((*log).exit_code) as usize - log as usize) };
        assert!(magic_off == MAGIC_OFFSET && code_off == CODE_OFFSET, "exit-word: the magic sits at offset {} and the code at {}, wanted {} and {}", magic_off, code_off, MAGIC_OFFSET, CODE_OFFSET);
        let (magic0, code0) = ring::end_words();
        assert!(magic0 == MAGIC_RUNNING && code0 == 0, "exit-word: the header reads {:#010x} and {} while the program runs, wanted RLOG ({:#010x}) and 0", magic0, code0, MAGIC_RUNNING);
        assert!(decode(magic0, code0) == Some((STATE_RUNNING, 0)), "exit-word: RLOG did not decode as running");
        assert!(MAGIC_RETURNED.to_le_bytes() == *b"RRET" && MAGIC_ABORTED.to_le_bytes() == *b"RABT", "exit-word: the end magics are not RRET and RABT");
        for (state, code) in [
            (STATE_RETURNED, 42),
            (STATE_ABORTED, 134),
            (STATE_RETURNED, 70000),
            (STATE_ABORTED, -1),
            (STATE_ABORTED, i32::MIN),
            (STATE_RETURNED, 0),
        ] {
            ring::end_mark(state, code);
            let (magic_now, code_now) = ring::end_words();
            assert!(magic_now == magic(state), "exit-word: the magic for {} {} read back {:#010x}, wanted {:#010x}", name(state), code, magic_now, magic(state));
            assert!(code_now == code, "exit-word: the code for {} {} read back {}, wanted {}", name(state), code, code_now, code);
            assert!(decode(magic_now, code_now) == Some((state, code)), "exit-word: the words {:#010x} and {} did not decode as {} {}", magic_now, code_now, name(state), code);
        }
        assert!(
            decode(0, 0).is_none() && decode(u32::from_le_bytes(*b"RREt"), 42).is_none() && decode(u32::from_le_bytes(*b"RLOX"), 0).is_none(),
            "exit-word: a magic outside the contract decoded"
        );
        ring::mark_running();
        let (magic_after, code_after) = ring::end_words();
        assert!(magic_after == MAGIC_RUNNING && code_after == 0, "exit-word: the header reads {:#010x} and {} after the restore, wanted RLOG and 0", magic_after, code_after);
        kprintln!("[api] PASS exit-word (the magic at offset 0 and the code at offset 12 by symbol: RLOG and 0 while running; RRET beside 42, 70000 and 0 for returned, RABT beside 134, -1 and -2147483648 for aborted, each read back whole and decoded; a magic outside the three refused; RLOG and 0 restored)");
    }

    // Check 40: the handler stack's watermark -- the boot stack reserve's
    // deepest use so far, read back from the pattern fill at reset (every
    // check above ran its interrupts, its faults and its deliveries on it);
    // the reserve a whole number of 4 KiB pages by the map symbols, the use
    // above zero and under the reserve. The doctored control skips the fill,
    // so the scan finds the reserve used whole and the check dies here.
    fn check_handler_stack() {
        kprintln!("[api] handler-stack: the boot stack reserve's high-water read back from the pattern fill at reset");
        let (used, reserve) = crate::stack_watermark::high_water().expect("handler-stack: the api flavor measures its handler stack");
        assert!(reserve >= 4096 && reserve % 4096 == 0, "handler-stack: the reserve reads {} bytes, not a whole number of 4 KiB pages", reserve);
        assert!(used > 0, "handler-stack: the high-water reads 0 -- no word of the reserve was used (the fill ran after the boot's use, or the scan read the wrong region)");
        assert!(used < reserve, "handler-stack: the high-water reads {} of {} bytes -- the reserve used whole (the fill missing or the floor crossed)", used, reserve);
        kprintln!("[api] PASS handler-stack (the high-water {} of {} bytes, {} bytes of the reserve never touched; read back from the pattern fill at reset)", used, reserve, reserve - used);
    }

    // Check 1: the ns clock always advances. Three shapes: (1)
    // thread-level paired reads, STRICT -- distinct calls never return
    // equal values; (2) the wrapped-but-undelivered hazard EXERCISED:
    // inside the kernel CS (SysTick masked) the counter is positioned
    // late in its tick, then spun across two reloads, so the
    // pending-tick handling is what keeps the second read ahead (with
    // the handling removed this window reads backward deterministically
    // -- the registered control's red shape); (3) the saturated-pend
    // worst case CONSTRUCTED: cross a reload inside the CS (the pend
    // bit latches), burn to late in the post-wrap generation, read (the
    // pending +1 composes with a LATE interpolation), cross one more
    // reload (the single bit saturates), read again early in the new
    // generation. Both reads share a tick term, so ordering is the
    // watermark's to carry -- without it this window reads backward by
    // most of a tick, every run.
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
        kprintln!("[api] ns-monotonic: its two designed windows each hold the tick masked across two SysTick reloads; where the witness counts, its 2 lost / 2 late are theirs by construction");
    }

    // Check 2: the never-early sleep contract on the ns clock (the
    // sleeper's own clock is the authority; the upper bound is
    // deliberately unpinned -- timing-coupled).
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

    // Check 4: an absolute-ns deadline honored, never early.
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

    // Check 4b: a deadline past the scheduler's 32-bit tick window is
    // never early. A waiter blocks with a deadline 2^32 ticks out and a
    // sleeper sleeps 2^31 + 1 ticks; three ticks later neither has moved
    // (a truncated arm would have fired both at once), and the waiter
    // wakes with 0 on the wake. The sleeper stays asleep for the boot's
    // life (its slot is one of the live ones later counts see).
    static FAR_WORD: AtomicU32 = AtomicU32::new(0);
    static FAR_RESULT: AtomicU32 = AtomicU32::new(u32::MAX);
    static FAR_SLEPT: AtomicU32 = AtomicU32::new(0);

    extern "C" fn far_waiter(_: usize) {
        let deadline = api::rhu_now_ns() + (1u64 << 32) * 1_000_000;
        let r = api::rhu_futex_wait(FAR_WORD.as_ptr(), 0, deadline);
        FAR_RESULT.store(r as u32, Ordering::Release);
    }

    extern "C" fn far_sleeper(_: usize) {
        api::rhu_sleep_ns(((1u64 << 31) + 1) * 1_000_000);
        FAR_SLEPT.store(1, Ordering::Release);
    }

    fn check_far_deadline() {
        kprintln!("[api] far-deadline: a futex wait and a sleep past the 32-bit tick window armed");
        sched::spawn("farw", far_waiter, 0, 4, 4 * 1024);
        // Parked OR already returned: a wait that times out at once
        // is caught by the assert below with its own
        // words, never by the parking wait's bound.
        wait_until(
            || sched::probe_futex_waiting(FAR_WORD.as_ptr()) == 1 || FAR_RESULT.load(Ordering::Acquire) != u32::MAX,
            "far waiter parked or returned",
        );
        api::rhu_sleep_ns(3_000_000);
        assert!(
            sched::probe_futex_waiting(FAR_WORD.as_ptr()) == 1 && FAR_RESULT.load(Ordering::Acquire) == u32::MAX,
            "far-deadline: the far futex wait timed out at once"
        );
        sched::spawn("fars", far_sleeper, 0, 4, 4 * 1024);
        api::rhu_sleep_ns(3_000_000);
        assert!(FAR_SLEPT.load(Ordering::Acquire) == 0, "far-deadline: the far sleep returned at once");
        assert!(api::rhu_futex_wake(FAR_WORD.as_ptr(), 1) == 1, "far-deadline: the far waiter was not woken");
        wait_until(|| FAR_RESULT.load(Ordering::Acquire) == 0, "far waiter woken with 0");
        kprintln!("[api] PASS far-deadline (a futex wait 2^32 ticks out held three ticks and woke with 0 on the wake, a sleep 2^31+1 ticks out held three ticks: never early past the 32-bit tick window)");
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

    // Check 6b: the wake is priority-first. Two waiters on one word at
    // different priorities, the LESS urgent one spawned first so its slot
    // comes first in the rotation: wake(1) must ready the more urgent one
    // (kernel-policy state read under the CS, as the wake checks above).
    static PRIO_WORD: AtomicU32 = AtomicU32::new(0);
    static PRIO_RAN: AtomicU32 = AtomicU32::new(0);

    extern "C" fn prio_waiter(_: usize) {
        let r = api::rhu_futex_wait(PRIO_WORD.as_ptr(), 0, sched::DEADLINE_NONE);
        assert!(r == 0, "prio waiter woke with {}", r);
        PRIO_RAN.fetch_add(1, Ordering::AcqRel);
    }

    fn check_wake_priority() {
        kprintln!("[api] wake-priority: two waiters at different priorities, the less urgent first in the rotation armed");
        let less = sched::spawn("wp6", prio_waiter, 0, 6, 4 * 1024);
        let more = sched::spawn("wp5", prio_waiter, 0, 5, 4 * 1024);
        assert!(less < more, "wake-priority: the less urgent waiter's slot is not first in the rotation");
        while sched::probe_futex_waiting(PRIO_WORD.as_ptr()) != 2 {
            api::rhu_sleep_ns(1_000_000);
        }
        let woken = api::rhu_futex_wake(PRIO_WORD.as_ptr(), 1);
        assert!(woken == 1, "wake-priority: wake(1) readied {}", woken);
        assert!(sched::probe_ready_among(&[more]) == 1, "wake-priority: the less urgent waiter was woken first");
        assert!(sched::probe_ready_among(&[less]) == 0, "wake-priority: both waiters readied by wake(1)");
        let rest = api::rhu_futex_wake(PRIO_WORD.as_ptr(), u32::MAX);
        assert!(rest == 1, "wake-priority: the rest readied {}", rest);
        while PRIO_RAN.load(Ordering::Acquire) != 2 {
            api::rhu_sleep_ns(1_000_000);
        }
        kprintln!("[api] PASS wake-priority (wake(1) readied the more urgent of two waiters though the less urgent one's slot came first in the rotation, the rest released after)");
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
                return; // took a free lock, conservatively marked contended
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
            // Non-atomic on purpose: mutual exclusion is what protects
            // this read-modify-write, and nothing else may.
            unsafe {
                let c = read_volatile(addr_of!(COUNTER));
                write_volatile(addr_of_mut!(COUNTER), c + 1);
            }
            if i % 50 == 0 {
                busy_wait_ms(2); // hold across a tick: forced contention
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

    // Check 9: the window is what the maps bracket, and it overlaps
    // nothing. Every fact read independently: the link brackets by
    // extern symbol, the kstacks window from the scheduler, the statics
    // window from the boot brackets -- disjointness is computed
    // interval overlap, never inferred from placement order.
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
        assert!(stk - hi >= 16 * 1024 && (stk - hi) % 4096 == 0, "arena window: boot-stack reserve under 16K or not whole 4 KiB pages");
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

    // Check 11: rezero re-establishes zero IN PLACE -- same address,
    // headroom and the bump address byte-identical before and after
    // (the zero-VA-burned contract) -- and refuses null + a range
    // inside the window that was never handed out, touching nothing.
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

    fn check_arena_rezero_band() {
        kprintln!("[api] arena-rezero-band: a large range re-zeroed with the band-interrupt control armed; a tick landing inside the memset is the proof no critical section is held across it");
        let site = check_arena_rezero_band as *const () as usize as u32;
        let room = api::rhu_arena_headroom();
        const SPARE: u32 = 256 * 1024;
        let bytes = if room > SPARE + 64 * 1024 { core::cmp::min(1024 * 1024, room - SPARE) } else { room / 4 } & !4095;
        assert!(bytes >= 16 * 1024, "arena-rezero-band: {} bytes of headroom leave no 16 KB range for the check", room);
        let q = api::rhu_arena_alloc_at(bytes, 8, site);
        assert!(!q.is_null(), "arena-rezero-band: the alloc of {} bytes failed", bytes);
        let h0 = api::rhu_arena_headroom();
        FP_BAND.hits.store(0, Ordering::Release);
        REZERO_BAND_HITS.store(0, Ordering::Release);
        let first_tick = sched::now_ms();
        fp_band_arm(true);
        let mut passes = 0u32;
        loop {
            let r = api::rhu_arena_rezero(q, bytes);
            assert!(r == 0, "arena-rezero-band: the rezero answered {}", r);
            passes += 1;
            if REZERO_BAND_HITS.load(Ordering::Acquire) >= 1 || sched::now_ms().wrapping_sub(first_tick) >= FP_BAND_TICK_BOUND {
                break;
            }
        }
        fp_band_arm(false);
        let inside = REZERO_BAND_HITS.load(Ordering::Acquire);
        let hits = FP_BAND.hits.load(Ordering::Acquire);
        kprintln!("[api] arena-rezero-band: {} control hit(s) landed inside the memset ({} control hits in all) over {} pass(es) of {} bytes within {} tick(s)", inside, hits, passes, bytes, sched::now_ms().wrapping_sub(first_tick));
        assert!(scan_zero(q as *const u8, bytes as usize), "arena-rezero-band: the range reads nonzero after the rezero");
        assert!(api::rhu_arena_headroom() == h0, "arena-rezero-band: the rezero burned VA");
        assert!(
            inside >= 1,
            "arena-rezero-band: no tick landed inside the rezero's memset within {} ticks ({} control hits, {} passes over {} bytes): the memset runs under the critical section",
            FP_BAND_TICK_BOUND,
            hits,
            passes,
            bytes
        );
        kprintln!("[api] PASS arena-rezero-band (a tick landed inside the rezero's memset: the zeroing runs with the kernel band live, the wait bounded by the kernel's own tick count)");
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

    fn check_ram_range() {
        kprintln!("[api] ram-range: the count and every range's record against the board's facts; the window inside its range and the headroom the arena's own; a page drawn from each range inside its window and zero-born, re-zeroed in place; a page at an alignment above the page honoured from each range; the index past the count, a NULL and a misaligned record refused; the SRAM door unchanged");
        let n = api::rhu_ram_count();
        assert!(n == ram::SERVED as i32, "ram-range: the count reads {} against the {} this image serves by its facts", n, ram::SERVED);
        let site = check_ram_range as *const () as usize as u32;
        let mut r = ram::RamRange { base: 0, bytes: 0, lo: 0, hi: 0, free: 0, flags: 0 };
        for i in 0..n as u32 {
            assert!(api::rhu_ram_range(i, &mut r) == 0, "ram-range: range {}'s record refused", i);
            let f = &board::facts::RAM_RANGES[i as usize];
            assert!(r.base as usize == f.base && r.bytes as usize == f.bytes, "ram-range: range {} reads base {:#010x} and {} bytes against the facts' {:#010x} and {}", i, r.base, r.bytes, f.base, f.bytes);
            assert!(r.lo >= r.base && r.hi <= r.base + r.bytes && r.lo < r.hi, "ram-range: range {}'s window [{:#010x}, {:#010x}) outside its range", i, r.lo, r.hi);
            // The flags as the facts state them, the exec bit per range (the
            // STM32F769's SDRAM is execute-never; every other range executable).
            let want = (if f.external { ram::FLAG_EXTERNAL } else { 0 }) | (if f.cached { ram::FLAG_CACHED } else { 0 }) | (if f.exec { ram::FLAG_EXEC } else { 0 });
            assert!(r.flags == want, "ram-range: range {}'s flags read {:#x} against {:#x}", i, r.flags, want);
            assert!(r.free % 8 == 0 && r.free <= r.hi - r.lo, "ram-range: range {}'s headroom {} is not an 8-multiple inside its window", i, r.free);
            if i == 0 {
                assert!(r.free == api::rhu_arena_headroom(), "ram-range: range 0's headroom {} differs from the arena door's {}", r.free, api::rhu_arena_headroom());
            }
            // On a row with an external RAM range, range 1's window starts at
            // the rehydrated-data region's end, 8-aligned: the region lies at
            // the head of the range and outside the window, and the headroom
            // at this first read is the window whole.
            #[cfg(rhu_extram_window)]
            if i == 1 {
                let (h_lo, h_hi) = crate::boot::hydrated_bracket();
                assert!(h_lo == r.base as usize && h_hi <= r.lo as usize, "ram-range: the rehydrated-data region [{:#010x}, {:#010x}) is not at the head of range 1 ({:#010x}) before its window ({:#010x})", h_lo, h_hi, r.base, r.lo);
                assert!(r.lo as usize == (h_hi + 7) & !7, "ram-range: range 1's window starts at {:#010x}, not at the rehydrated-data region's end {:#010x} rounded to 8", r.lo, h_hi);
                assert!(r.free == r.hi - r.lo, "ram-range: range 1's headroom {} at the first read is not its window's {} bytes", r.free, r.hi - r.lo);
            }
            let p = api::rhu_arena_alloc_from(i, 4096, 8, site);
            assert!(!p.is_null(), "ram-range: the page from range {} refused NULL", i);
            assert!(p as u32 >= r.lo && p as u32 + 4096 <= r.hi, "ram-range: the page {:#010x} from range {} lies outside its window", p as u32, i);
            assert!(scan_zero(p as *const u8, 4096), "ram-range: the page from range {} reads nonzero", i);
            let mut after = ram::RamRange { base: 0, bytes: 0, lo: 0, hi: 0, free: 0, flags: 0 };
            assert!(api::rhu_ram_range(i, &mut after) == 0, "ram-range: range {}'s second record refused", i);
            assert!(after.free + 4096 == r.free && after.lo == r.lo && after.hi == r.hi, "ram-range: range {}'s headroom moved from {} to {} for a 4096-byte page", i, r.free, after.free);
            if i == 0 {
                assert!(api::rhu_arena_headroom() == after.free, "ram-range: the SRAM door's headroom {} did not follow range 0's {}", api::rhu_arena_headroom(), after.free);
            }
            for k in 0..4096 {
                unsafe { write_volatile(p.add(k), 0x3C) };
            }
            assert!(api::rhu_arena_rezero(p, 4096) == 0, "ram-range: the rezero of range {}'s page refused", i);
            assert!(scan_zero(p as *const u8, 4096), "ram-range: range {}'s page reads nonzero after the rezero", i);
            let big_align: u32 = if i == 0 { 16384 } else { 1 << 20 };
            let big = api::rhu_arena_alloc_from(i, 4096, big_align, site);
            assert!(!big.is_null(), "ram-range: the {}-aligned page from range {} refused NULL", big_align, i);
            assert!(big as u32 % big_align == 0 && big as u32 >= r.lo && big as u32 + 4096 <= r.hi, "ram-range: the {}-aligned page {:#010x} from range {} misaligned or outside its window", big_align, big as u32, i);
            assert!(scan_zero(big as *const u8, 4096), "ram-range: the {}-aligned page from range {} reads nonzero", big_align, i);
        }
        assert!(api::rhu_ram_range(n as u32, &mut r) == sched::ERR_INVAL, "ram-range: an index past the count not refused EINVAL");
        assert!(api::rhu_ram_range(0, core::ptr::null_mut()) == sched::ERR_INVAL, "ram-range: a NULL record not refused EINVAL");
        let misaligned = (addr_of_mut!(r) as usize + 2) as *mut ram::RamRange;
        assert!(api::rhu_ram_range(0, misaligned) == sched::ERR_INVAL, "ram-range: a misaligned record not refused EINVAL");
        kprintln!("[api] PASS ram-range ({} range(s) served and read back as the facts state them, each window inside its range with the arena's own headroom, a page drawn from each range inside its window and zero-born with the headroom moved by its size, re-zeroed in place, a page at 16 KB alignment from the SRAM range and at 1 MB from an external one honoured, the index past the count, a NULL and a misaligned record EINVAL, the SRAM door's headroom equal to range 0's)", n);
    }

    /// The quad-SPI window (the STM32F769 row; the api image's own map
    /// declares the window with the header alone): the boot's bring-up
    /// record printed, the window's first 64 words read twice through the
    /// window and equal, the header's words equal to the boot's read, the
    /// protection unit's region for the window read back as programmed
    /// (read-only for every privilege, executable, Normal write-through, 64
    /// MB at the window's base). Under the control test-xip-store a word is
    /// stored into the window after the checks: the region must refuse it,
    /// a MemManage death the funnel names (never a shipped image).
    #[cfg(rhu_qspi_window)]
    fn check_qspi_window() {
        use crate::board::stm32f7_qspi as qspi;
        kprintln!("[api] qspi-window: the boot's bring-up record, the window's first 64 words read twice and equal, the window header's words as the boot read them, the protection region's words read back");
        let w = qspi::record().expect("qspi-window: the api image's map declares no window");
        kprintln!(
            "[api] qspi-window: {} {} id {:02x} {:02x} {:02x} at {} MHz, {} dummy clocks ({} driven as the alternate byte {:#04x}, {} left undriven), the sample shift {}; the window's image {} bytes past the header; {} cycles over the identification, the register fields and the indirect read, {} over the mode switch, the read-back and the header",
            w.part.maker,
            w.part.name,
            w.id[0],
            w.id[1],
            w.id[2],
            w.clk_hz / 1_000_000,
            w.part.dummy_total,
            2 * w.part.alt_bytes,
            w.part.alt_value,
            w.part.dummy_cycles,
            if qspi::SAMPLE_SHIFT { "set" } else { "clear" },
            w.image_bytes,
            w.cycles_indirect,
            w.cycles_window
        );
        let base = qspi::WINDOW.base as usize;
        let mut first = [0u32; 64];
        for (k, v) in first.iter_mut().enumerate() {
            *v = unsafe { read_volatile((base + 4 * k) as *const u32) };
        }
        for (k, v) in first.iter().enumerate() {
            let again = unsafe { read_volatile((base + 4 * k) as *const u32) };
            assert!(again == *v, "qspi-window: word {} reads {:#010x} then {:#010x} through the window", k, v, again);
        }
        for k in 0..8 {
            assert!(first[k] == w.header[k], "qspi-window: header word {} reads {:#010x} where the boot read {:#010x}", k, first[k], w.header[k]);
        }
        kprintln!("[api] qspi-window: the window's first words {:#010x} {:#010x} {:#010x} {:#010x} {:#010x} {:#010x} {:#010x} {:#010x}", first[0], first[1], first[2], first[3], first[4], first[5], first[6], first[7]);
        let (rbar, rasr) = crate::mpu::region_readback(crate::mpu::REGION_QSPI);
        assert!(rbar & !0x1F == base as u32, "qspi-window: region {}'s base reads {:#010x}, not the window's {:#010x}", crate::mpu::REGION_QSPI, rbar & !0x1F, base);
        assert!(rasr == crate::mpu::RASR_QSPI, "qspi-window: region {}'s attributes read {:#010x}, not {:#010x} (read-only, executable, Normal write-through, 64 MB)", crate::mpu::REGION_QSPI, rasr, crate::mpu::RASR_QSPI);
        #[cfg(feature = "test-xip-store")]
        {
            kprintln!("[api] qspi-window: THE CONTROL test-xip-store -- a word store into the window follows; the protection unit must refuse it (a MemManage death the funnel names)");
            unsafe { write_volatile(base as *mut u32, 0) };
            panic!("qspi-window: the store into the window went through: the region did not refuse it");
        }
        #[cfg(not(feature = "test-xip-store"))]
        kprintln!(
            "[api] PASS qspi-window (the bring-up record; 64 words read twice and equal; the header as the boot read it; region {} at {:#010x} with RASR {:#010x}: read-only for every privilege, executable, Normal write-through, {} MB)",
            crate::mpu::REGION_QSPI,
            base,
            rasr,
            qspi::WINDOW.bytes >> 20
        );
    }

    /// The execute-in-place posture (the STM32F769 row): the instruction
    /// cache on and the data cache off read back from the core under the
    /// window, and the cycle counter over one 4 KB sequential read of the
    /// window and one strided read of 256 words 1 KB apart, printed as a
    /// record.
    #[cfg(rhu_qspi_window)]
    fn check_xip_posture() {
        use crate::board::stm32f7_qspi as qspi;
        kprintln!("[api] xip-posture: CCR.IC and CCR.DC read back under the window; the cycle counter over a 4 KB sequential read and a 256-word strided read (1 KB apart) of the window");
        let (ic, dc, bp) = crate::cache::posture();
        assert!(ic, "xip-posture: the instruction cache reads off under the window");
        assert!(!dc, "xip-posture: the data cache reads on (the row's law keeps it off)");
        let base = qspi::WINDOW.base as usize;
        let mut sum = 0u32;
        let t0 = qspi::cycles();
        for k in 0..1024 {
            sum = sum.wrapping_add(unsafe { read_volatile((base + 4 * k) as *const u32) });
        }
        let t1 = qspi::cycles();
        for k in 0..256 {
            sum ^= unsafe { read_volatile((base + 1024 * k) as *const u32) };
        }
        let t2 = qspi::cycles();
        core::hint::black_box(sum);
        kprintln!("[api] xip-posture: {} cycles over the 4 KB sequential read, {} over the 256-word strided read, at {} MHz on the flash clock", t1.wrapping_sub(t0), t2.wrapping_sub(t1), qspi::CLK_HZ / 1_000_000);
        kprintln!("[api] PASS xip-posture (CCR.IC 1, CCR.DC 0, CCR.BP {}; the two reads timed on the cycle counter)", bp as u32);
    }

    /// The api flavor's own tenant of the hosted runtime's rehydrated-data
    /// section (hydrated-home): 256 bytes, zero-initialized, placed by the
    /// row's map at the head of the external RAM region.
    #[cfg(rhu_extram_window)]
    #[unsafe(link_section = ".hydrated.test")]
    static mut HYDRATED_TEST: [u8; 256] = [0; 256];

    /// The rehydrated-data region's home: the api flavor's static in the
    /// section the row's map places at the head of the external RAM region,
    /// before the arena's window 1. It reads
    /// zero at the check through the poison the boot's bracket memset
    /// cleared (boot.rs; a zero read is the memset's, never the memory's
    /// reset state); a pattern written through the program's view reads
    /// back, and on the RP2350 rows reads back through the uncached mirror
    /// after the cache is cleaned, so the write reached the part.
    #[cfg(rhu_extram_window)]
    fn check_hydrated_home() {
        kprintln!("[api] hydrated-home: the api flavor's 256-byte static in the rehydrated-data section -- zero through the boot poison, at the head of the external range before window 1, a pattern written and read back{}", if cfg!(rhu_psram_window) { " through the cached window and the uncached mirror" } else { "" });
        let p = addr_of_mut!(HYDRATED_TEST) as *mut u8;
        let (h_lo, h_hi) = crate::boot::hydrated_bracket();
        assert!(p as usize >= h_lo && p as usize + 256 <= h_hi, "hydrated-home: the static {:#010x} lies outside the rehydrated-data bracket [{:#010x}, {:#010x})", p as usize, h_lo, h_hi);
        let mut r = ram::RamRange { base: 0, bytes: 0, lo: 0, hi: 0, free: 0, flags: 0 };
        assert!(api::rhu_ram_range(1, &mut r) == 0, "hydrated-home: range 1's record refused");
        assert!(h_lo == r.base as usize && h_hi <= r.lo as usize, "hydrated-home: the bracket [{:#010x}, {:#010x}) is not at the head of range 1 ({:#010x}) before its window ({:#010x})", h_lo, h_hi, r.base, r.lo);
        assert!(scan_zero(p as *const u8, 256), "hydrated-home: the static reads nonzero (the bracket's boot memset did not reach it)");
        for k in 0..256 {
            unsafe { write_volatile(p.add(k), (k as u8) ^ 0x5A) };
        }
        for k in 0..256 {
            let v = unsafe { read_volatile(p.add(k)) };
            assert!(v == (k as u8) ^ 0x5A, "hydrated-home: byte {} reads {:#04x} after the write", k, v);
        }
        #[cfg(rhu_psram_window)]
        {
            use lamella_psram_qspi::proof::Memory;
            use lamella_psram_qspi::rp2350::{Mmio, WINDOW1_BASE, WINDOW1_UNCACHED_BASE};
            unsafe { Mmio.flush() };
            let mirror = (p as usize - WINDOW1_BASE + WINDOW1_UNCACHED_BASE) as *const u8;
            for k in 0..256 {
                let v = unsafe { read_volatile(mirror.add(k)) };
                assert!(v == (k as u8) ^ 0x5A, "hydrated-home: byte {} reads {:#04x} through the uncached mirror (the write did not reach the part)", k, v);
            }
        }
        for k in 0..256 {
            unsafe { write_volatile(p.add(k), 0) };
        }
        kprintln!("[api] PASS hydrated-home (the static zero through the boot poison, at the head of the external range before window 1, 256 bytes written and read back{})", if cfg!(rhu_psram_window) { " through both views" } else { "" });
    }

    /// The posture the exclusive pairs rest on, read back on the PMSAv8 rows:
    /// every enabled MPU region's shareability bits and ACTLR.EXTEXCLALL. A
    /// non-shareable
    /// region keeps its ldrex/strex pairs on the core's local monitor; a
    /// Shareable region or EXTEXCLALL sends them to the fabric's global
    /// monitor, which on the RP2350 answers for the SRAM alone. The shipped
    /// posture reads SH = 00 on every region and EXTEXCLALL 0; the control
    /// test-excl-shareable reads its Normal regions Shareable.
    #[cfg(armv8m)]
    fn check_excl_posture() {
        kprintln!("[api] excl-posture: every enabled MPU region's shareability bits and ACTLR.EXTEXCLALL read back (a non-shareable region keeps its exclusive pairs on the core's local monitor)");
        let (enabled, nonshare, share, extexclall) = crate::mpu::exclusive_posture();
        assert!(enabled >= 1, "excl-posture: no MPU region reads enabled");
        #[cfg(not(feature = "test-excl-shareable"))]
        assert!(share == 0, "excl-posture: {} of {} enabled regions read Shareable (their exclusive pairs go to the fabric's global monitor)", share, enabled);
        #[cfg(feature = "test-excl-shareable")]
        assert!(share >= 1, "excl-posture: the control's Normal regions did not read Shareable ({} of {} enabled)", share, enabled);
        assert!(extexclall == 0, "excl-posture: ACTLR.EXTEXCLALL reads 1 (every exclusive sent to the fabric's global monitor)");
        kprintln!("[api] PASS excl-posture ({} regions enabled: {} non-shareable, {} Shareable; ACTLR.EXTEXCLALL {})", enabled, nonshare, share, extexclall);
    }

    /// The exclusive-access proof: a bounded load-exclusive and
    /// store-exclusive pair on a word
    /// drawn from window 1 (the external RAM) and on an SRAM word as the
    /// control. The RP2350 datasheet (section 2.1.6.2) says its fabric reports
    /// exclusive failure for every exclusive access outside the main SRAM, does
    /// not suppress the store, and such loops run forever; the Cortex-M7
    /// decides exclusives to non-shareable memory by its local monitor alone.
    /// The pair is tried at most 64 times on each word and the outcome printed
    /// as a fact (HELD at attempt n, or REFUSED after 64 with the word's value);
    /// the check judges only the SRAM control and the bound: the record decides
    /// the design, and the case never hangs. On the emulator rows QEMU's
    /// monitor holds every pair: the case's mechanics proved there, not the
    /// silicon's answer.
    #[cfg(rhu_extram_window)]
    fn check_excl_window1() {
        kprintln!("[api] excl-window1: a bounded ldrex/strex pair (64 attempts) on a window-1 word and on an SRAM word armed");
        #[cfg(feature = "test-excl-shareable")]
        kprintln!("[api] excl-window1: THE CONTROL test-excl-shareable -- every Normal region programmed Shareable, each pair sent to the fabric's global monitor: the SRAM word expected to hold there, the window-1 word expected REFUSED after 64 with the stores gone through");
        let site = check_excl_window1 as *const () as usize as u32;
        let ext = api::rhu_arena_alloc_from(1, 8, 8, site) as *mut u32;
        assert!(!ext.is_null(), "excl-window1: the window-1 word refused NULL");
        let sram = api::rhu_arena_alloc_from(0, 8, 8, site) as *mut u32;
        assert!(!sram.is_null(), "excl-window1: the SRAM word refused NULL");
        // The attempt the store held at (0 when none in 64) and the last strex result.
        fn pair(p: *mut u32) -> (u32, u32) {
            let mut last = 1u32;
            for n in 1..=64u32 {
                let fail: u32;
                unsafe {
                    core::arch::asm!(
                        "ldrex {t}, [{p}]",
                        "adds {t}, {t}, #1",
                        "strex {f}, {t}, [{p}]",
                        p = in(reg) p,
                        t = out(reg) _,
                        f = out(reg) fail,
                        options(nostack)
                    );
                }
                last = fail;
                if fail == 0 {
                    return (n, 0);
                }
            }
            unsafe {
                core::arch::asm!("clrex", options(nomem, nostack));
            }
            (0, last)
        }
        let (n_ext, f_ext) = pair(ext);
        let v_ext = unsafe { core::ptr::read_volatile(ext) };
        let (n_sram, f_sram) = pair(sram);
        let v_sram = unsafe { core::ptr::read_volatile(sram) };
        assert!(n_sram == 1 && v_sram == 1, "excl-window1: the SRAM control's pair did not hold at the first attempt (held at {}, strex {}, the word {})", n_sram, f_sram, v_sram);
        if n_ext != 0 {
            kprintln!("[api] excl-window1: window-1 word {:#010x} HELD at attempt {} (the word reads {}); the SRAM word {:#010x} held at attempt 1", ext as u32, n_ext, v_ext, sram as u32);
        } else {
            kprintln!("[api] excl-window1: window-1 word {:#010x} REFUSED after 64 attempts (strex returned {} every time; the word reads {}: the stores {}); the SRAM word {:#010x} held at attempt 1", ext as u32, f_ext, v_ext, if v_ext == 0 { "suppressed" } else { "went through" }, sram as u32);
        }
        kprintln!("[api] PASS excl-window1 (the pair bounded on both words; the SRAM control held at the first attempt; the window-1 outcome printed above as the record's fact)");
    }


    /// The core's cycle counter (armed from boot on this row) and its reading in microseconds.
    #[cfg(all(rhu_sd_socket, not(feature = "test-sd-doctored")))]
    fn cyc() -> u32 {
        unsafe { read_volatile(0xE000_1004 as *const u32) }
    }
    #[cfg(all(rhu_sd_socket, not(feature = "test-sd-doctored")))]
    fn us(cycles: u32) -> u32 {
        (cycles as u64 * 1_000_000 / board::SYSCLK_HZ as u64) as u32
    }

    #[cfg(rhu_sd_socket)]
    fn check_sd_identify() {
        kprintln!("[api] sd-identify: the device count, the record of mmcblk0 read at the call (the detect line and the identification) against the card's own registers; the index past the count, a NULL and a misaligned record refused");
        let n = api::rhu_blk_count();
        assert!(n == 1, "sd-identify: the count reads {} against the 1 socket this row has", n);
        let mut d = blk::BlkDesc { block_bytes: 0, blocks: 0, flags: 0, name: [0; 16] };
        assert!(api::rhu_blk_device(0, &mut d) == 0, "sd-identify: the record refused");
        assert!(d.block_bytes == 512 && &d.name[..8] == b"mmcblk0\0", "sd-identify: the record's block size {} or name wrong", d.block_bytes);
        assert!(d.flags & blk::FLAG_REMOVABLE != 0, "sd-identify: the socket's record not removable");
        assert!(d.flags & blk::FLAG_PRESENT != 0, "sd-identify: no card in the socket (the flight needs one seated)");
        #[cfg(not(feature = "test-sd-doctored"))]
        {
            let facts = blk::with_card(|c| (c.capacity_blocks, c.high_capacity, c.write_protected, c.width == sdhost::Width::Four, sdcard::cid_manufacturer(&c.cid), sdcard::cid_product(&c.cid), sdcard::cid_serial(&c.cid), c.version2));
            let (cap, hc, wp, four, maker, product, serial, v2) = match facts {
                Some(f) => f,
                None => {
                    let (step, why) = blk::refusal_words();
                    panic!("sd-identify: the card refused at {} ({})", step, why);
                }
            };
            kprintln!(
                "[api] sd-identify: the card -- maker {:#04x}, product {}{}{}{}{}, serial {:#010x}, version 2 {}, high capacity {}, {} blocks of 512 ({} MB), write-protected {}, {} line(s)",
                maker,
                product[0] as char,
                product[1] as char,
                product[2] as char,
                product[3] as char,
                product[4] as char,
                serial,
                v2,
                hc,
                cap,
                cap / 2048,
                wp,
                if four { 4 } else { 1 }
            );
            assert!(d.blocks == cap, "sd-identify: the record's {} blocks against the card's {}", d.blocks, cap);
            kprintln!("[api] sd-identify: the identification took {} cycles = {} us at boot (the host spinning before the scheduler); {} yields inside the host's waits so far at {} Hz", blk::ident_cycles(), us(blk::ident_cycles()), blk::yields(), board::SYSCLK_HZ);
            assert!((d.flags & blk::FLAG_HIGH_CAPACITY != 0) == hc && (d.flags & blk::FLAG_WRITE_PROTECTED != 0) == wp, "sd-identify: the record's flags {:#x} against the card's class", d.flags);
        }
        #[cfg(feature = "test-sd-doctored")]
        {
            let (step, why) = blk::refusal_words();
            assert!(d.blocks == 0, "sd-doctored-control: the doctored identification passed with {} blocks (the control did not bite)", d.blocks);
            #[repr(C, align(16))]
            struct Buf([u8; 512]);
            static mut B: Buf = Buf([0; 512]);
            let r = api::rhu_blk_read(0, 0, 1, addr_of_mut!(B) as *mut c_void);
            assert!(r == sched::ERR_IO || r == sched::ERR_TIMEDOUT, "sd-doctored-control: the read answered {} with the card refused", r);
            kprintln!("[api] sd-doctored-control: the card refused at {} ({}); the record present with 0 blocks; the read refused {} -- the control bit", step, why, r);
        }
        assert!(api::rhu_blk_device(1, &mut d) == sched::ERR_INVAL, "sd-identify: the index past the count not refused EINVAL");
        assert!(api::rhu_blk_device(0, core::ptr::null_mut()) == sched::ERR_INVAL, "sd-identify: a NULL record not refused EINVAL");
        let misaligned = (addr_of_mut!(d) as usize + 2) as *mut blk::BlkDesc;
        assert!(api::rhu_blk_device(0, misaligned) == sched::ERR_INVAL, "sd-identify: a misaligned record not refused EINVAL");
        #[cfg(not(feature = "test-sd-doctored"))]
        kprintln!("[api] PASS sd-identify (1 device, mmcblk0 at 512-byte blocks, present and removable, the record's capacity and flags the card's own; the index 1, a NULL and a misaligned record EINVAL)");
    }

    #[cfg(all(rhu_sd_socket, not(feature = "test-sd-doctored")))]
    fn check_sd_boot_sector() {
        kprintln!("[api] sd-boot-sector: block 0 read twice into two buffers and equal, the boot signature present; an 8-block read whose first block equals the single read; the sync door; the argument refusals -- a count of 0 and of 129, a range at and past the end, a NULL and a misaligned buffer EINVAL, device 1 ENODEV");
        #[repr(C, align(16))]
        struct Buf([u8; 4096]);
        static mut A: Buf = Buf([0; 4096]);
        static mut B: Buf = Buf([0; 4096]);
        let a = addr_of_mut!(A) as *mut u8;
        let b = addr_of_mut!(B) as *mut u8;
        assert!(api::rhu_blk_read(0, 0, 1, a as *mut c_void) == 1, "sd-boot-sector: the first read of block 0 refused");
        let t0 = cyc();
        assert!(api::rhu_blk_read(0, 0, 1, b as *mut c_void) == 1, "sd-boot-sector: the second read of block 0 refused");
        let one = cyc().wrapping_sub(t0);
        let (sa, sb) = unsafe { (core::slice::from_raw_parts(a, 512), core::slice::from_raw_parts(b, 512)) };
        assert!(sa == sb, "sd-boot-sector: the two reads of block 0 differ");
        assert!(sa[510] == 0x55 && sa[511] == 0xAA, "sd-boot-sector: no boot signature in block 0 (bytes 510 and 511 read {:#04x} {:#04x}): the card is not formatted", sa[510], sa[511]);
        let nonzero = sa.iter().filter(|&&x| x != 0).count();
        kprintln!("[api] sd-boot-sector: block 0 -- bytes 0 to 3 {:02x} {:02x} {:02x} {:02x}, {} nonzero bytes, the signature 55 AA", sa[0], sa[1], sa[2], sa[3], nonzero);
        let t0 = cyc();
        assert!(api::rhu_blk_read(0, 0, 8, b as *mut c_void) == 8, "sd-boot-sector: the 8-block read refused");
        let eight = cyc().wrapping_sub(t0);
        kprintln!("[api] sd-boot-sector: one block read in {} cycles = {} us; 8 blocks in {} cycles = {} us; {} yields so far at {} Hz", one, us(one), eight, us(eight), blk::yields(), board::SYSCLK_HZ);
        assert!(sa == unsafe { core::slice::from_raw_parts(b, 512) }, "sd-boot-sector: the 8-block read's first block differs from the single read");
        assert!(api::rhu_blk_sync(0) == 0, "sd-boot-sector: the sync door refused");
        assert!(api::rhu_blk_sync(1) == sched::ERR_NODEV, "sd-boot-sector: the sync of device 1 not refused ENODEV");
        let cap = blk::with_card(|c| c.capacity_blocks).unwrap_or(0);
        assert!(api::rhu_blk_read(0, 0, 0, a as *mut c_void) == sched::ERR_INVAL, "sd-boot-sector: a count of 0 not refused EINVAL");
        assert!(api::rhu_blk_read(0, 0, 129, a as *mut c_void) == sched::ERR_INVAL, "sd-boot-sector: a count of 129 not refused EINVAL");
        assert!(api::rhu_blk_read(0, cap, 1, a as *mut c_void) == sched::ERR_INVAL, "sd-boot-sector: a block at the capacity not refused EINVAL");
        assert!(api::rhu_blk_read(0, cap - 1, 2, a as *mut c_void) == sched::ERR_INVAL, "sd-boot-sector: a range past the end not refused EINVAL");
        assert!(api::rhu_blk_read(0, 0, 1, core::ptr::null_mut()) == sched::ERR_INVAL, "sd-boot-sector: a NULL buffer not refused EINVAL");
        assert!(api::rhu_blk_read(0, 0, 1, unsafe { a.add(2) } as *mut c_void) == sched::ERR_INVAL, "sd-boot-sector: a misaligned buffer not refused EINVAL");
        assert!(api::rhu_blk_read(1, 0, 1, a as *mut c_void) == sched::ERR_NODEV, "sd-boot-sector: device 1 not refused ENODEV");
        assert!(api::rhu_blk_write(1, 0, 1, a as *const c_void) == sched::ERR_NODEV, "sd-boot-sector: a write to device 1 not refused ENODEV");
        assert!(api::rhu_blk_write(0, 0, 0, a as *const c_void) == sched::ERR_INVAL, "sd-boot-sector: a write of 0 blocks not refused EINVAL");
        kprintln!("[api] PASS sd-boot-sector (block 0 read twice and equal with the signature 55 AA, the 8-block read's first block equal to the single read, the sync door 0 and ENODEV for device 1; the counts 0 and 129, the ranges at and past the end, a NULL and a misaligned buffer EINVAL, device 1 ENODEV for a read and a write, a write of 0 blocks EINVAL)");
    }

    #[cfg(all(rhu_sd_socket, not(feature = "test-sd-doctored")))]
    fn check_sd_sdram_buffer() {
        kprintln!("[api] sd-sdram-buffer: block 1 and an 8-block run read by the controller's DMA stream straight into a page of the SDRAM range (the bus matrix's path to the flexible memory controller), equal to the same blocks read into the internal SRAM");
        let site = check_sd_sdram_buffer as *const () as usize as u32;
        let p = api::rhu_arena_alloc_from(1, 4096, 16, site);
        assert!(!p.is_null(), "sd-sdram-buffer: the SDRAM page refused NULL");
        #[repr(C, align(16))]
        struct Buf([u8; 4096]);
        static mut S: Buf = Buf([0; 4096]);
        let s = addr_of_mut!(S) as *mut u8;
        assert!(api::rhu_blk_read(0, 1, 1, s as *mut c_void) == 1, "sd-sdram-buffer: the SRAM read of block 1 refused");
        assert!(api::rhu_blk_read(0, 1, 1, p as *mut c_void) == 1, "sd-sdram-buffer: the SDRAM read of block 1 refused");
        let (ss, sp) = unsafe { (core::slice::from_raw_parts(s, 512), core::slice::from_raw_parts(p as *const u8, 512)) };
        assert!(ss == sp, "sd-sdram-buffer: block 1 differs between the SRAM and the SDRAM buffer");
        assert!(api::rhu_blk_read(0, 8, 8, s as *mut c_void) == 8, "sd-sdram-buffer: the SRAM 8-block read refused");
        assert!(api::rhu_blk_read(0, 8, 8, p as *mut c_void) == 8, "sd-sdram-buffer: the SDRAM 8-block read refused");
        let (ss, sp) = unsafe { (core::slice::from_raw_parts(s, 4096), core::slice::from_raw_parts(p as *const u8, 4096)) };
        assert!(ss == sp, "sd-sdram-buffer: the 8-block run differs between the SRAM and the SDRAM buffer");
        kprintln!("[api] PASS sd-sdram-buffer (block 1 and blocks 8 to 15 read into the SDRAM page at {:#010x} by the DMA stream, byte for byte the SRAM reads)", p as u32);
    }

    #[cfg(all(rhu_sd_socket, not(feature = "test-sd-doctored")))]
    fn check_sd_write_back() {
        kprintln!("[api] sd-write-back: 128 blocks (64 KB) at the card's middle read into an SDRAM buffer, WRITTEN BACK byte for byte to the same blocks, read again into a second buffer and compared; one block the same way; the wall times by the cycle counter and the yields the host's waits took (5.12's predictions (b) and (c)); a write-protected card refuses EROFS instead");
        let site = check_sd_write_back as *const () as usize as u32;
        let (cap, protected) = blk::with_card(|c| (c.capacity_blocks, c.write_protected)).unwrap_or((0, true));
        let p1 = api::rhu_arena_alloc_from(1, 65536, 16, site);
        let p2 = api::rhu_arena_alloc_from(1, 65536, 16, site);
        assert!(!p1.is_null() && !p2.is_null(), "sd-write-back: the SDRAM buffers refused NULL");
        if protected {
            assert!(api::rhu_blk_write(0, 0, 1, p1 as *const c_void) == -30, "sd-write-back: a write on the protected card not refused EROFS");
            kprintln!("[api] PASS sd-write-back (the card is write-protected: a write refused EROFS; the measurement skipped)");
            return;
        }
        let lba = (cap / 2) & !127;
        let y0 = blk::yields();
        let t0 = cyc();
        assert!(api::rhu_blk_read(0, lba, 128, p1 as *mut c_void) == 128, "sd-write-back: the 128-block read refused");
        let r128 = cyc().wrapping_sub(t0);
        let t0 = cyc();
        assert!(api::rhu_blk_write(0, lba, 128, p1 as *const c_void) == 128, "sd-write-back: the 128-block write refused");
        let w128 = cyc().wrapping_sub(t0);
        let t0 = cyc();
        assert!(api::rhu_blk_read(0, lba, 128, p2 as *mut c_void) == 128, "sd-write-back: the second 128-block read refused");
        let r128b = cyc().wrapping_sub(t0);
        let (s1, s2) = unsafe { (core::slice::from_raw_parts(p1 as *const u8, 65536), core::slice::from_raw_parts(p2 as *const u8, 65536)) };
        assert!(s1 == s2, "sd-write-back: the 64 KB read back differs from what was written");
        let t0 = cyc();
        assert!(api::rhu_blk_write(0, lba, 1, p1 as *const c_void) == 1, "sd-write-back: the one-block write refused");
        let w1 = cyc().wrapping_sub(t0);
        assert!(api::rhu_blk_read(0, lba, 1, p2 as *mut c_void) == 1, "sd-write-back: the one-block read after the write refused");
        assert!(s1[..512] == s2[..512], "sd-write-back: the one block read back differs");
        let y = blk::yields().wrapping_sub(y0);
        kprintln!(
            "[api] sd-write-back: blocks {} to {} -- 64 KB read {} cycles = {} us, written {} cycles = {} us, read again {} cycles = {} us; one block written {} cycles = {} us; {} yields across the five transfers at {} Hz",
            lba,
            lba + 127,
            r128,
            us(r128),
            w128,
            us(w128),
            r128b,
            us(r128b),
            w1,
            us(w1),
            y,
            board::SYSCLK_HZ
        );
        kprintln!("[api] PASS sd-write-back (64 KB and one block at the card's middle written back byte for byte and read equal; the times above)");
    }

    /// The filesystem layer's volume over the socket for the three fs checks: a
    /// static under the cfg, about 4.5 KB of .bss on the STM32F769 api image alone.
    #[cfg(all(rhu_sd_socket, not(feature = "test-sd-doctored")))]
    static mut FS_VOLUME: Option<fat::Volume<blk::SocketDevice>> = None;

    /// The layer's clock seam: the calendar's reading when a source set it,
    /// else None (the layer's FAT floor, 1 January 1980).
    #[cfg(all(rhu_sd_socket, not(feature = "test-sd-doctored")))]
    fn fs_now() -> Option<calendar_math::Civil> {
        let (ticks, source) = calendar::get();
        if source == calendar::SOURCE_UNSET {
            None
        } else {
            Some(calendar_math::ticks_to_civil(ticks))
        }
    }

    #[cfg(all(rhu_sd_socket, not(feature = "test-sd-doctored")))]
    fn fs_volume() -> &'static mut fat::Volume<blk::SocketDevice> {
        unsafe { (*addr_of_mut!(FS_VOLUME)).as_mut() }.expect("fs: the volume fs-mount mounted")
    }

    #[cfg(all(rhu_sd_socket, not(feature = "test-sd-doctored")))]
    fn check_fs_mount() {
        kprintln!("[api] fs-mount: the card's volume mounted through the filesystem layer with a 64 KB SDRAM scratch page from the arena's range 1 -- the type by the cluster count alone, the label, the cluster size, the counts and the free count, with the mount's cycles and whether the FSInfo's count served or one pass over the FAT ran");
        let site = check_fs_mount as *const () as usize as u32;
        let p = api::rhu_arena_alloc_from(1, 65536, 16, site);
        assert!(!p.is_null(), "fs-mount: the SDRAM scratch page refused NULL");
        let scratch = unsafe { core::slice::from_raw_parts_mut(p, 65536) };
        let t0 = cyc();
        let r = fat::Volume::mount_in(unsafe { &mut *addr_of_mut!(FS_VOLUME) }, blk::SocketDevice, scratch);
        let dt = cyc().wrapping_sub(t0);
        if let Err(r) = r {
            panic!("fs-mount: the mount refused -- {}", r.words());
        }
        let v = fs_volume();
        v.set_clock(fs_now);
        let g = *v.geometry();
        let kind = match g.kind {
            fat::Kind::Fat12 => "FAT12",
            fat::Kind::Fat16 => "FAT16",
            fat::Kind::Fat32 => "FAT32",
        };
        let free = v.free_clusters();
        kprintln!(
            "[api] fs-mount: {} -- label {}, {} bytes per cluster ({} sectors), {} clusters, {} free, {} reserved sectors, {} FATs of {} sectors, the root {}, the volume at sector {} ({}) -- {} cycles = {} us; {}",
            kind,
            core::str::from_utf8(v.label()).unwrap_or("?"),
            g.cluster_bytes,
            g.sec_per_clus,
            g.count_of_clusters,
            free,
            g.rsvd,
            g.num_fats,
            g.fat_sz,
            if g.kind == fat::Kind::Fat32 { "a cluster chain" } else { "a fixed region" },
            g.base,
            if g.base == 0 { "the boot sector at block 0, no partition table" } else { "behind the partition table's first FAT entry" },
            dt,
            us(dt),
            if v.hint_served() { "the FSInfo's count served (no pass)" } else { "one pass over the FAT ran" }
        );
        if !v.hint_served() {
            kprintln!("[api] fs-mount: the pass read the FAT in {} calls through the scratch page", v.pass_calls());
        }
        assert!(g.count_of_clusters >= 1 && free <= g.count_of_clusters, "fs-mount: the counts are not consistent");
        kprintln!("[api] PASS fs-mount ({} with {} clusters of {} bytes, {} free, at sector {}, mounted in {} us)", kind, g.count_of_clusters, g.cluster_bytes, free, g.base, us(dt));
    }

    #[cfg(all(rhu_sd_socket, not(feature = "test-sd-doctored")))]
    fn check_fs_walk() {
        kprintln!("[api] fs-walk: the root listed through the layer, up to 32 entries with their long names, kinds and sizes, to compare with a PC's listing");
        let v = fs_volume();
        let t0 = cyc();
        let mut d = v.opendir(b"/").unwrap_or_else(|e| panic!("fs-walk: opendir refused {}", e.code()));
        let mut e = fat::Entry::EMPTY;
        let (mut n, mut dirs, mut files) = (0u32, 0u32, 0u32);
        while n < 32 {
            match v.readdir(&mut d, &mut e) {
                Ok(true) => {}
                Ok(false) => break,
                Err(err) => panic!("fs-walk: readdir refused {} after {} entries", err.code(), n),
            }
            n += 1;
            if e.is_dir() {
                dirs += 1;
            } else {
                files += 1;
            }
            kprintln!("[api] fs-walk:   {} {:>10} {}{}", if e.is_dir() { "d" } else { "-" }, e.size, core::str::from_utf8(e.name()).unwrap_or("?"), if e.long { "" } else { "  (a short name)" });
        }
        v.closedir(&mut d).unwrap_or_else(|e| panic!("fs-walk: closedir refused {}", e.code()));
        let dt = cyc().wrapping_sub(t0);
        kprintln!("[api] PASS fs-walk ({} entries listed -- {} directories, {} files -- in {} cycles = {} us)", n, dirs, files, dt, us(dt));
    }

    #[cfg(all(rhu_sd_socket, not(feature = "test-sd-doctored")))]
    fn check_fs_roundtrip() {
        kprintln!("[api] fs-roundtrip: THE ONE WRITE OF THE FLIGHT -- a file rhu-fs-roundtrip.tmp and a directory rhu-fs-roundtrip.dir the check creates and removes at the card's root, REFUSED before any write if either name exists; 1 MB of a cycle pattern written from an SDRAM buffer, synced, closed, re-read into a second SDRAM buffer and compared, truncated to 4096 bytes and re-read, unlinked; the directory made, listed empty and removed; the free count equal before and after; the times by the cycle counter");
        const FILE: &[u8] = b"/rhu-fs-roundtrip.tmp";
        const DIR: &[u8] = b"/rhu-fs-roundtrip.dir";
        const LEN: usize = 1 << 20;
        let v = fs_volume();
        for name in [FILE, DIR] {
            match v.stat(name) {
                Err(fat::Error::NotFound) => {}
                Ok(_) => panic!("fs-roundtrip: REFUSED before any write -- {} already exists on the card (no existing file is opened for writing)", core::str::from_utf8(name).unwrap_or("?")),
                Err(e) => panic!("fs-roundtrip: the guard's stat refused {}", e.code()),
            }
        }
        let protected = blk::with_card(|c| c.write_protected).unwrap_or(true);
        if protected {
            kprintln!("[api] PASS fs-roundtrip (the card is write-protected: nothing written, the roundtrip skipped)");
            return;
        }
        let site = check_fs_roundtrip as *const () as usize as u32;
        let p1 = api::rhu_arena_alloc_from(1, LEN as u32, 16, site);
        let p2 = api::rhu_arena_alloc_from(1, LEN as u32, 16, site);
        assert!(!p1.is_null() && !p2.is_null(), "fs-roundtrip: the SDRAM buffers refused NULL");
        let (a, b) = unsafe { (core::slice::from_raw_parts_mut(p1, LEN), core::slice::from_raw_parts_mut(p2, LEN)) };
        for (i, x) in a.iter_mut().enumerate() {
            *x = (i as u8).wrapping_mul(7).wrapping_add((i >> 8) as u8) ^ ((i >> 16) as u8).wrapping_mul(31);
        }
        let free0 = v.free_clusters();
        let t0 = cyc();
        let mut h = v.open(FILE, fat::O_WRITE | fat::O_CREATE | fat::O_EXCL).unwrap_or_else(|e| panic!("fs-roundtrip: the create refused {}", e.code()));
        let t_create = cyc().wrapping_sub(t0);
        let t0 = cyc();
        let mut done = 0;
        while done < LEN {
            let n = v.write(&mut h, &a[done..]).unwrap_or_else(|e| {
                let w = blk::witness_snapshot();
                panic!("fs-roundtrip: the {} refused {} after {} bytes; the sd witness -- refusals {}, last step {}, last why {}, writes {}, blocks written {}, the host status {:#010x}, the stream flags {:#010x}", "write", e.code(), done, w.0, w.1, w.2, w.3, w.4, w.5, w.6)
            });
            assert!(n > 0, "fs-roundtrip: a write moved nothing");
            done += n;
        }
        let t_write = cyc().wrapping_sub(t0);
        let t0 = cyc();
        v.sync_file(&mut h).unwrap_or_else(|e| panic!("fs-roundtrip: the sync refused {}", e.code()));
        let t_sync = cyc().wrapping_sub(t0);
        v.close(&mut h).unwrap_or_else(|e| panic!("fs-roundtrip: the close refused {}", e.code()));
        let mut h = v.open(FILE, fat::O_READ).unwrap_or_else(|e| panic!("fs-roundtrip: the re-open refused {}", e.code()));
        assert!(v.fstat(&h).map(|s| s.size) == Ok(LEN as u32), "fs-roundtrip: the size after the close is not 1 MB");
        let t0 = cyc();
        let mut done = 0;
        while done < LEN {
            let n = v.read(&mut h, &mut b[done..]).unwrap_or_else(|e| {
                let w = blk::witness_snapshot();
                panic!("fs-roundtrip: the {} refused {} after {} bytes; the sd witness -- refusals {}, last step {}, last why {}, writes {}, blocks written {}, the host status {:#010x}, the stream flags {:#010x}", "read", e.code(), done, w.0, w.1, w.2, w.3, w.4, w.5, w.6)
            });
            assert!(n > 0, "fs-roundtrip: the read ended early at {} bytes", done);
            done += n;
        }
        let t_read = cyc().wrapping_sub(t0);
        assert!(a == b, "fs-roundtrip: the 1 MB read back differs from what was written");
        v.close(&mut h).unwrap_or_else(|e| panic!("fs-roundtrip: the close after the read refused {}", e.code()));
        let mut h = v.open(FILE, fat::O_READ | fat::O_WRITE).unwrap_or_else(|e| panic!("fs-roundtrip: the open for the truncate refused {}", e.code()));
        v.truncate(&mut h, 4096).unwrap_or_else(|e| panic!("fs-roundtrip: the truncate refused {}", e.code()));
        v.sync_file(&mut h).unwrap_or_else(|e| panic!("fs-roundtrip: the sync after the truncate refused {}", e.code()));
        v.seek(&mut h, 0, fat::SEEK_SET).unwrap_or_else(|e| panic!("fs-roundtrip: the seek refused {}", e.code()));
        b[..8192].fill(0);
        let n = v.read(&mut h, &mut b[..8192]).unwrap_or_else(|e| panic!("fs-roundtrip: the read after the truncate refused {}", e.code()));
        let after = v.read(&mut h, &mut b[4096..8192]).unwrap_or(1);
        assert!(n == 4096 && after == 0, "fs-roundtrip: the truncated file reads {} then {} bytes, not 4096 then the end", n, after);
        assert!(a[..4096] == b[..4096], "fs-roundtrip: the truncated file's bytes differ");
        v.close(&mut h).unwrap_or_else(|e| panic!("fs-roundtrip: the close after the truncate refused {}", e.code()));
        assert!(v.stat(FILE).map(|s| s.size) == Ok(4096), "fs-roundtrip: the size after the truncate is not 4096");
        let t0 = cyc();
        v.unlink(FILE).unwrap_or_else(|e| panic!("fs-roundtrip: the unlink refused {}", e.code()));
        let t_unlink = cyc().wrapping_sub(t0);
        v.mkdir(DIR).unwrap_or_else(|e| panic!("fs-roundtrip: the mkdir refused {}", e.code()));
        let mut d = v.opendir(DIR).unwrap_or_else(|e| panic!("fs-roundtrip: the opendir refused {}", e.code()));
        let mut e = fat::Entry::EMPTY;
        let mut n = 0;
        while v.readdir(&mut d, &mut e).unwrap_or_else(|e| panic!("fs-roundtrip: readdir refused {}", e.code())) {
            assert!(e.name() == b"." || e.name() == b"..", "fs-roundtrip: the new directory lists an entry of its own");
            n += 1;
        }
        v.closedir(&mut d).unwrap_or_else(|e| panic!("fs-roundtrip: closedir refused {}", e.code()));
        assert!(n == 2, "fs-roundtrip: the new directory lists {} entries, not the two dots", n);
        v.rmdir(DIR).unwrap_or_else(|e| panic!("fs-roundtrip: the rmdir refused {}", e.code()));
        for name in [FILE, DIR] {
            assert!(v.stat(name) == Err(fat::Error::NotFound), "fs-roundtrip: {} still present after", core::str::from_utf8(name).unwrap_or("?"));
        }
        v.sync().unwrap_or_else(|e| panic!("fs-roundtrip: the final sync refused {}", e.code()));
        let free1 = v.free_clusters();
        assert!(free1 == free0, "fs-roundtrip: the free count moved from {} to {}", free0, free1);
        kprintln!(
            "[api] fs-roundtrip: the create {} cycles = {} us, the 1 MB write {} = {} us, the sync {} = {} us, the 1 MB read {} = {} us, the unlink {} = {} us; {} clusters free before and after",
            t_create,
            us(t_create),
            t_write,
            us(t_write),
            t_sync,
            us(t_sync),
            t_read,
            us(t_read),
            t_unlink,
            us(t_unlink),
            free0
        );
        kprintln!("[api] PASS fs-roundtrip (1 MB written from the SDRAM, read back equal into the SDRAM, truncated to 4096 and re-read equal, unlinked; the directory made, empty, removed; the two names absent and the free count unchanged)");
    }

    // Check 13, LAST in the family: headroom is honest to the byte. An
    // alloc of exactly headroom() succeeds, a further 8-byte alloc is
    // refused with a quiet NULL (the one honest NULL -- exhaustion),
    // and headroom reads 0. Also the audit probe's refusal direction:
    // a never-used site reads (0, 0) against the now-final table.
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

    #[cfg(rhu_ram_init_background)]
    fn check_ram_init_frontier() {
        kprintln!("[api] ram-init-frontier: the background mode's frontier armed (64 KB re-poisoned above the lazy window's hand-out edge)");
        let (index, lo, hi, bump0, clean0) = arena::probe_frontier();
        assert!(bump0 >= lo && clean0 >= bump0 && clean0 <= hi, "ram-init-frontier: the frontier {:#010x} outside [bump {:#010x}, hi {:#010x}]", clean0, bump0, hi);
        let poison_end = arena::probe_frontier_rewind(64 * 1024);
        assert!(poison_end >= bump0 + 8192, "ram-init-frontier: under 8 KB left above the lazy window's hand-out edge");
        let (_, _, _, _, clean1) = arena::probe_frontier();
        assert!(clean1 == bump0, "ram-init-frontier: the frontier {:#010x} did not rewind to the hand-out edge {:#010x}", clean1, bump0);
        let site = check_ram_init_frontier as *const () as usize as u32;
        let p = api::rhu_arena_alloc_from(index as u32, 4096, 8, site);
        assert!(!p.is_null(), "ram-init-frontier: the straddling page refused NULL");
        assert!(p as usize == bump0, "ram-init-frontier: the straddling page {:#010x} is not at the hand-out edge {:#010x}", p as usize, bump0);
        assert!(scan_zero(p as *const u8, 4096), "ram-init-frontier: the straddling page reads nonzero (the hand-out did not zero its part above the frontier)");
        let (_, _, _, bump2, clean2) = arena::probe_frontier();
        assert!(bump2 == bump0 + 4096 && clean2 == bump2, "ram-init-frontier: the frontier {:#010x} did not jump to the hand-out's end {:#010x}", clean2, bump2);
        let mut ticks = 0u32;
        loop {
            let (_, _, _, _, c) = arena::probe_frontier();
            if c >= hi {
                break;
            }
            assert!(ticks < 20_000, "ram-init-frontier: the frontier {:#010x} short of the window's end {:#010x} after {} ticks of idle", c, hi, ticks);
            api::rhu_sleep_ns(1_000_000);
            ticks += 1;
        }
        let (mid, takeovers, retries, abandoned) = arena::probe_ram_init_witness();
        kprintln!("[api] ram-init-frontier: {} tick(s) served mid-piece since boot (the zeroing outside the critical section); {} takeover(s) of the piece in flight, {} strex retr(y/ies), {} piece(s) abandoned", mid, takeovers, retries, abandoned);
        assert!(mid > 0, "ram-init-frontier: no tick served while a piece was zeroing -- the background zeroing ran under the critical section");
        let q = api::rhu_arena_alloc_from(index as u32, 4096, 8, site);
        assert!(!q.is_null(), "ram-init-frontier: the page below the frontier refused NULL");
        assert!(q as usize == bump2 && q as usize + 4096 <= poison_end, "ram-init-frontier: the page {:#010x} is not the next page inside the poisoned span", q as usize);
        assert!(scan_zero(q as *const u8, 4096), "ram-init-frontier: the page below the frontier reads nonzero (the idle step did not zero the poison)");
        let (_, _, _, bump3, clean3) = arena::probe_frontier();
        assert!(clean3 == hi && bump3 == bump2 + 4096, "ram-init-frontier: the frontier {:#010x} moved for a hand-out below it (bump {:#010x})", clean3, bump3);
        kprintln!("[api] PASS ram-init-frontier (the straddling page zero-born by the hand-out with the frontier at its end; the frontier at the window's end after {} tick(s) of idle; the page below it zero-born by the idle step alone)", ticks);
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

    // Check 14: kheap alloc/write/free/re-alloc + mixed alignment. The
    // heap is pristine here (module 0 carries no TLS and nothing else
    // draws), so the freed head block is the first fit for its exact
    // size -- address equality is the reuse proof. All blocks freed at
    // the end: coalescing folds the heap back to one block, which the
    // next check's adjacency asserts then verify from a cold start.
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

    // Check 15: coalescing, proven on a guarded triple. The guard keeps
    // z off the wilderness block so the 3-way merge -- island, forward,
    // backward -- is exactly what the summed re-allocation proves; the
    // sum is computed from the kheap's own header const, never a
    // golden'd absolute.
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
        api::rhu_kheap_free(y); // island
        api::rhu_kheap_free(z); // merges backward into y's block
        api::rhu_kheap_free(x); // merges forward: one x..z block
        let sum = 48 + 80 + 112 + 2 * kheap::HDR_BYTES;
        let w = api::rhu_kheap_alloc(sum, 8);
        assert!(w == x, "kheap-coalesce: the summed span did not reallocate at the first block");
        api::rhu_kheap_free(w);
        api::rhu_kheap_free(g);
        kprintln!("[api] PASS kheap-coalesce (adjacent frees merged whole: the summed span reallocated at the first block)");
    }

    // Check 15b: the heap's size arithmetic is bounded. A request whose
    // rounding wraps and a request over any row's heap are both refused
    // NULL with the refusal line, and the heap is intact after (a plain
    // alloc and free succeed). The requests are constants, so the two
    // emulator rows print the same lines.
    fn check_kheap_bound() {
        kprintln!("[api] kheap-bound: a wrapping request and an over-heap request armed");
        assert!(api::rhu_kheap_alloc(0xFFFF_FFF7, 8).is_null(), "kheap-bound: a wrapping request was not refused");
        assert!(api::rhu_kheap_alloc(0x7000_0000, 8).is_null(), "kheap-bound: an over-heap request was not refused");
        let p = api::rhu_kheap_alloc(40, 8);
        assert!(!p.is_null(), "kheap-bound: a plain alloc failed after the refusals");
        api::rhu_kheap_free(p);
        kprintln!("[api] PASS kheap-bound (a request that wraps the rounding and a request over the heap refused NULL and said so, the heap intact after: a later alloc and free succeeded)");
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
        let me = arg as usize; // 0 or 1
        let h1 = TLS_H1.load(Ordering::Acquire);
        let h2 = TLS_H2.load(Ordering::Acquire);
        let b1 = api::rhu_tls_get(h1);
        let b2 = api::rhu_tls_get(h2);
        // Spawn-side eager init: image honored in module 1, zero fill
        // in module 2 -- checked in-thread, before any write.
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

    // Check 16: N-module TLS. Registration-side eager allocation gives
    // kmain (and idle) their blocks at register time; spawn-side eager
    // gives the two workers theirs at birth. Blocks are per (thread,
    // module): six distinct addresses, init image and zero fill
    // honored in every copy, and a thread's writes invisible outside
    // its own blocks.
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

        // Registration-side eager: this thread's blocks exist NOW.
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
        // Cross-thread visibility: each writer's byte is where it wrote
        // it and nowhere else; kmain's own byte survived both workers.
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

    // Check 16b: the thread-local-base door. The debug-visible layout
    // record is read FROM MEMORY at its exported symbols exactly as a
    // debugger reads it -- raw words at `__rhu_debug_layout`, then
    // `__rhu_sched` walked with the recorded offsets, no kernel verb in
    // the walk -- and judged against the doors: every live thread's
    // module-0 pointer is null while the template brackets are empty
    // (this family carries no thread-local template); the two
    // registered modules' blocks read through the record equal what
    // rhu_tls_get answers on kmain and what each of two parked workers
    // answered for itself; every non-live slot reads null in every
    // module column; the running thread's slot is the recorded current.
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
        // The record, as words at its symbol.
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

        // The doors' answers, taken once; the walk below never calls one.
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
            DTOR_BAD.fetch_or(1, Ordering::AcqRel); // ran off-thread
        }
        let p = api::rhu_tls_get(TLS_H1.load(Ordering::Acquire));
        if unsafe { read_volatile(p) } != 0xD7 {
            DTOR_BAD.fetch_or(2, Ordering::AcqRel); // TLS not live in a dtor
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
        // Exit by returning: the entry-return path runs the dtors too.
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
        api::rhu_sleep_ns(2_000_000); // grace: nothing may run a ninth
        assert!(DTOR_SEQ.load(Ordering::Acquire) == 8, "tls-dtors: more than eight dtor runs");
        for (call, slot) in DTOR_ORDER.iter().enumerate() {
            let want = 8 - call as u32; // LIFO: call 0 sees arg 7 (+1 encoding)
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

    // Checks 18-19 helpers: parked probes over one release word.
    static ABI_PARK: AtomicU32 = AtomicU32::new(0);
    static ABI_SELF_SEEN: AtomicI32 = AtomicI32::new(0);

    extern "C" fn probe_return(_: *mut c_void) {
        while ABI_PARK.load(Ordering::Acquire) == 0 {
            let _ = api::rhu_futex_wait(ABI_PARK.as_ptr(), 0, sched::DEADLINE_NONE);
        }
        // Exit by returning.
    }

    extern "C" fn probe_exit_verb(_: *mut c_void) {
        ABI_SELF_SEEN.store(api::rhu_thread_self(), Ordering::Release);
        api::rhu_thread_yield();
        while ABI_PARK.load(Ordering::Acquire) == 0 {
            let _ = api::rhu_futex_wait(ABI_PARK.as_ptr(), 0, sched::DEADLINE_NONE);
        }
        api::rhu_thread_exit(); // the explicit exit path
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
        api::rhu_sleep_ns(2_000_000); // reap grace
    }

    fn check_thread_abi() {
        kprintln!("[api] thread-abi: C-door round trip + exact-fit stack reuse armed");
        // (a) round trip + reuse at the unique 2K size.
        let t1 = spawn_probe(probe_exit_verb, 2048);
        assert!(t1 >= 1, "thread-abi: spawn returned {}", t1);
        wait_until(|| sched::probe_futex_waiting(ABI_PARK.as_ptr()) == 1, "probe 1 parked");
        assert!(ABI_SELF_SEEN.load(Ordering::Acquire) == t1, "thread-abi: self() != spawn's tid");
        let (mut lo1, mut b1) = (0u32, 0u32);
        assert!(api::rhu_thread_stack_bounds(t1, &mut lo1, &mut b1) == 0, "thread-abi: bounds(t1) refused");
        release(&ABI_PARK);
        join_tid(t1);
        ABI_PARK.store(0, Ordering::Release); // re-arm for the next probe

        let t2 = spawn_probe(probe_exit_verb, 2048);
        assert!(t2 >= 1 && t2 != t1, "thread-abi: tid not unique across spawn-exit-spawn");
        wait_until(|| sched::probe_futex_waiting(ABI_PARK.as_ptr()) == 1, "probe 2 parked");
        let (mut lo2, mut b2) = (0u32, 0u32);
        assert!(api::rhu_thread_stack_bounds(t2, &mut lo2, &mut b2) == 0, "thread-abi: bounds(t2) refused");
        assert!(lo2 == lo1 && b2 == b1, "thread-abi: exact-fit reuse missed (bounds differ)");
        release(&ABI_PARK);
        join_tid(t2);
        ABI_PARK.store(0, Ordering::Release);

        // (c) the name is the kernel's copy: spawned with names in local
        // buffers that are clobbered once the door returns, each thread's
        // record still reads the bytes the door saw -- cut to the buffer,
        // at a character boundary -- and points into its own block. The
        // stacks are a size unique in the image's spawn history.
        let mut local = *b"clobbered-name-longer-than-15\0";
        let tn = api::rhu_thread_spawn(
            Some(probe_return),
            core::ptr::null_mut(),
            1536,
            0x40,
            local.as_ptr() as *const core::ffi::c_char,
        );
        assert!(tn >= 1, "thread-abi: the named spawn returned {}", tn);
        local.fill(b'#');
        let mut got = [0u8; sched::NAME_BYTES];
        let n = sched::probe_thread_name(tn, &mut got)
            .expect("thread-abi: the named thread's record does not point into its own block");
        assert!(&got[..n] == b"clobbered-name-" && got[n] == 0, "thread-abi: the copied name reads {:?}", &got[..n]);
        wait_until(|| sched::probe_futex_waiting(ABI_PARK.as_ptr()) == 1, "the named probe parked");
        release(&ABI_PARK);
        join_tid(tn);
        ABI_PARK.store(0, Ordering::Release);
        // 14 ASCII bytes then one two-byte character straddling the cut.
        let mut local2 = *b"abcdefghijklmn\xc3\xa9\0";
        let tn2 = api::rhu_thread_spawn(
            Some(probe_return),
            core::ptr::null_mut(),
            1536,
            0x40,
            local2.as_ptr() as *const core::ffi::c_char,
        );
        assert!(tn2 >= 1, "thread-abi: the second named spawn returned {}", tn2);
        local2.fill(b'#');
        let n2 = sched::probe_thread_name(tn2, &mut got)
            .expect("thread-abi: the second named thread's record does not point into its own block");
        assert!(&got[..n2] == b"abcdefghijklmn" && got[n2] == 0, "thread-abi: the cut name reads {:?}", &got[..n2]);
        wait_until(|| sched::probe_futex_waiting(ABI_PARK.as_ptr()) == 1, "the second named probe parked");
        release(&ABI_PARK);
        join_tid(tn2);
        ABI_PARK.store(0, Ordering::Release);

        // (b) slot exhaustion, honest refusal, algorithmic count.
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
            assert!((spawned as usize) < tids.len(), "thread-abi: spawn never refused");
            tids[spawned as usize] = t;
            spawned += 1;
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
        // The probes' 1K stacks matched nothing retired (idle never
        // exits, every other historical size differs), so the list only
        // gained: accepted up to capacity, the rest audited as leaked.
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
        kprintln!("[api] PASS thread-abi (round trip clean, tids unique, exact-fit stack reused, the name a kernel copy cut at the buffer, slot exhaustion refused honest)");
    }

    // Check 18b: the spawn door's stack floor. Sizes the frames cannot
    // fit (0, 1, 8, 64, one below the floor) and sizes whose 8-byte
    // round-up wraps are refused RHU_EINVAL with nothing committed -- the
    // slot census, the retired list and the pool's bump unchanged; the
    // largest size that does not wrap is refused by the pool, RHU_EAGAIN;
    // the floor itself spawns, parks and joins.
    fn check_stack_min() {
        kprintln!("[api] stack-min: sizes below the floor, sizes whose round-up wraps, the floor itself armed");
        let floor = sched::THREAD_STACK_MIN as u32;
        let census0 = (sched::probe_slot_states(), sched::probe_retired().0, sched::probe_pool_used());
        for bytes in [0u32, 1, 8, 64, floor - 1, 0xFFFF_FFF9, 0xFFFF_FFFE, u32::MAX] {
            assert!(
                spawn_probe(probe_return, bytes) == sched::ERR_INVAL,
                "stack-min: a stack of {} bytes was not refused EINVAL",
                bytes
            );
        }
        assert!(
            spawn_probe(probe_return, 0xFFFF_FFF8) == sched::ERR_AGAIN,
            "stack-min: the largest non-wrapping size was not refused EAGAIN by the pool"
        );
        let census1 = (sched::probe_slot_states(), sched::probe_retired().0, sched::probe_pool_used());
        assert!(census1 == census0, "stack-min: a refusal moved state");
        let t = spawn_probe(probe_return, floor);
        assert!(t >= 1, "stack-min: the floor itself refused {}", t);
        wait_until(|| sched::probe_futex_waiting(ABI_PARK.as_ptr()) == 1, "the floor-stack probe parked");
        release(&ABI_PARK);
        join_tid(t);
        ABI_PARK.store(0, Ordering::Release);
        kprintln!("[api] PASS stack-min (0, 1, 8, 64 and 511 refused EINVAL, the sizes whose round-up wraps refused EINVAL, the largest non-wrapping size refused EAGAIN by the pool, nothing moved by a refusal, the floor itself spawned, parked and joined)");
    }

    static HB_STARTED: AtomicU32 = AtomicU32::new(0);
    static HB_CHILD_PRIO: AtomicU32 = AtomicU32::new(0xFFFF);
    static HB_CHILD_TID: AtomicI32 = AtomicI32::new(0);
    static HB_PEER_RAN: AtomicU32 = AtomicU32::new(0);
    static HB_LATE_RAN: AtomicU32 = AtomicU32::new(0);

    extern "C" fn band_child(_: *mut c_void) {
        HB_CHILD_PRIO.store(sched::probe_current_prio() as u32, Ordering::Release);
        HB_CHILD_TID.store(api::rhu_thread_self(), Ordering::Release);
        HB_STARTED.store(1, Ordering::Release); // the started signal; exit by returning
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

        // (b) Joining re-runs the pick: back at kernel urgency, ready a
        // band thread at the band's most urgent value; fifty yields hold
        // it; the join runs it to completion before returning.
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

        // (c) Leaving restores kernel urgency: a band thread spawned
        // after it waits through fifty yields and runs once the host
        // blocks (the join below sleeps).
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

    // Check 37b: the calendar doors, late in the flavor (a board's
    // low-speed crystal is ready by now, so on a row with a real-time
    // clock the write-back is a measured fact). The base at boot is the
    // epoch or a carried one (a warm board with a set clock, a pushed
    // mailbox); an explicit set reads back with its source and the
    // elapsed clock beneath it; a later explicit set replaces it; the
    // source and range refusals; the states per class.
    fn check_calendar() {
        kprintln!("[api] calendar-door: the calendar base's doors armed (get, two explicit sets, the replace rule, the refusals, the record)");
        const T1: u64 = 631_167_699_060_000_000;
        const T2: u64 = T1 + 864_000_000_000;
        let mut t = 0u64;
        let mut s = 0i32;
        assert!(api::rhu_calendar_get(&mut t, &mut s) == 0, "calendar: get refused");
        assert!(s == 0 || s == 1 || s == 2, "calendar: the source at boot reads {}", s);
        assert!(api::rhu_calendar_set(T1, 4) == 0, "calendar: the application's set refused");
        assert!(api::rhu_calendar_get(&mut t, &mut s) == 0, "calendar: get refused after the set");
        assert!(s == 4, "calendar: the source after the set reads {}", s);
        assert!(t >= T1 && t < T1 + 500_000, "calendar: read {} after setting {}", t, T1);
        api::rhu_sleep_ns(25_000_000);
        assert!(api::rhu_calendar_get(&mut t, &mut s) == 0, "calendar: get refused after the sleep");
        assert!(t - T1 >= 250_000, "calendar: the elapsed clock did not carry the calendar ({} ticks)", t - T1);
        assert!(api::rhu_calendar_set(T2, 3) == 0, "calendar: the network set refused");
        assert!(api::rhu_calendar_get(&mut t, &mut s) == 0, "calendar: get refused after the replace");
        assert!(s == 3 && t >= T2, "calendar: the replace did not take (source {}, ticks {})", s, t);
        assert!(api::rhu_calendar_set(T1, 7) == sched::ERR_INVAL, "calendar: source 7 not refused EINVAL");
        assert!(api::rhu_calendar_set(T1, 1) == sched::ERR_INVAL, "calendar: a hosted set claiming the clock's source not refused");
        assert!(
            api::rhu_calendar_set(calendar_math::TICKS_MAX + 1, 4) == sched::ERR_INVAL,
            "calendar: an instant past the maximum not refused EINVAL"
        );
        assert!(api::rhu_calendar_get(core::ptr::null_mut(), &mut s) == sched::ERR_INVAL, "calendar: a null ticks pointer not refused");
        assert!(api::rhu_calendar_get(&mut t, core::ptr::null_mut()) == sched::ERR_INVAL, "calendar: a null source pointer not refused");
        let (rtc, lse, _pushes) = calendar::record_states();
        if board::RTC_PRESENT {
            assert!(rtc == calendar::RTC_WRITTEN, "calendar: the record's clock state reads {} after the sets", rtc);
            assert!(lse == calendar::LSE_READY, "calendar: the record's oscillator state reads {}", lse);
            kprintln!("[api] PASS calendar-door (the epoch or a carried base at boot, the explicit set read back with its source over the elapsed clock, the replace rule, source and range refusals, the real-time clock written back with the crystal ready)");
        } else {
            assert!(rtc == calendar::RTC_NONE, "calendar: the record claims a clock state {} on a row without one", rtc);
            kprintln!("[api] PASS calendar-door (the epoch at boot, the explicit set read back with its source over the elapsed clock, the replace rule, source and range refusals, no real-time clock on this row)");
        }
    }

    fn check_gpio_claim() {
        kprintln!("[api] gpio-claim: the line space stated; a free line claimed, re-configured by the same program, released, claimed again (an output at its initial level read back before a write where the row has one)");
        let n = api::rhu_gpio_count();
        assert!(n == board::facts::LINE_COUNT as i32, "gpio-claim: the line space reads {}", n);
        let l = board::facts::CHECK_FREE;
        if let Some(o) = board::facts::CHECK_OUT {
            assert!(api::rhu_gpio_claim(o, gpio_math::MODE_OUTPUT, gpio_math::PULL_NONE, 1) == 0, "gpio-claim: the output claim refused");
            assert!(api::rhu_gpio_read(o) == 1, "gpio-claim: the output's initial level did not read back before a write");
            assert!(api::rhu_gpio_claim(o, gpio_math::MODE_OUTPUT, gpio_math::PULL_NONE, 0) == 0, "gpio-claim: the same program's second claim (a re-configuration) refused");
            assert!(api::rhu_gpio_read(o) == 0, "gpio-claim: the re-configured initial level did not read back");
            assert!(api::rhu_gpio_write(o, 1) == 0 && api::rhu_gpio_read(o) == 1, "gpio-claim: a write did not read back");
            assert!(api::rhu_gpio_write(o, 0) == 0 && api::rhu_gpio_read(o) == 0, "gpio-claim: a second write did not read back");
            assert!(api::rhu_gpio_release(o) == 0, "gpio-claim: the output's release refused");
        }
        assert!(api::rhu_gpio_claim(l, gpio_math::MODE_INPUT, gpio_math::PULL_UP, 0) == 0, "gpio-claim: the input claim refused");
        assert!(api::rhu_gpio_claim(l, gpio_math::MODE_INPUT, gpio_math::PULL_DOWN, 0) == 0, "gpio-claim: the input's re-configuration refused");
        let level = api::rhu_gpio_read(l);
        assert!(level == 0 || level == 1, "gpio-claim: the input read {}", level);
        assert!(api::rhu_gpio_release(l) == 0, "gpio-claim: the release refused");
        assert!(api::rhu_gpio_read(l) == sched::ERR_INVAL, "gpio-claim: a read on a released line not refused EINVAL");
        assert!(api::rhu_gpio_claim(l, gpio_math::MODE_INPUT, gpio_math::PULL_NONE, 0) == 0, "gpio-claim: the claim after the release refused");
        assert!(api::rhu_gpio_release(l) == 0, "gpio-claim: the second release refused");
        let (taken, re) = unsafe { ((*addr_of!(gpio::WITNESS)).claims_taken, (*addr_of!(gpio::WITNESS)).reconfigured) };
        assert!(taken >= 2 && re >= 1, "gpio-claim: the witness counts {} claims and {} re-configurations", taken, re);
        kprintln!("[api] PASS gpio-claim (the line space stated, a claim taken, the same program's second claim a re-configuration, a released line refused a read, claimed again; the output's initial level read back before a write where the row has an output)");
    }

    fn check_gpio_refuse() {
        kprintln!("[api] gpio-refuse: the board's line, a line past the space, an unpopulated line, an input-only output claim, a write on an input, an unclaimed read and release, a bad selector -- each refused with its code");
        let n = api::rhu_gpio_count() as u32;
        // A row with no board-claimed line by default (the Pico rows:
        // the memory console takes no pin) skips the board-line assertion
        // and says so; every other row asserts it.
        if board::facts::BOARD_CLAIMS.is_empty() {
            kprintln!("[api] gpio-refuse: no line is claimed by the board on this row by default (the memory console takes no UART pin); the board-line assertion is skipped");
        } else {
            assert!(api::rhu_gpio_claim(board::facts::CHECK_BOARD, gpio_math::MODE_INPUT, gpio_math::PULL_NONE, 0) == gpio::ERR_BUSY, "gpio-refuse: the board's line not refused EBUSY");
        }
        assert!(api::rhu_gpio_claim(n, gpio_math::MODE_INPUT, gpio_math::PULL_NONE, 0) == sched::ERR_INVAL, "gpio-refuse: the line at the space's end not refused EINVAL");
        assert!(api::rhu_gpio_claim(n + 1000, gpio_math::MODE_INPUT, gpio_math::PULL_NONE, 0) == sched::ERR_INVAL, "gpio-refuse: a line far past the space not refused EINVAL");
        if let Some(u) = board::facts::CHECK_UNPOPULATED {
            assert!(api::rhu_gpio_claim(u, gpio_math::MODE_INPUT, gpio_math::PULL_NONE, 0) == sched::ERR_INVAL, "gpio-refuse: an unpopulated line not refused EINVAL");
        }
        if let Some(io) = board::facts::CHECK_INPUT_ONLY {
            assert!(api::rhu_gpio_claim(io, gpio_math::MODE_OUTPUT, gpio_math::PULL_NONE, 0) == gpio::ERR_ACCES, "gpio-refuse: an output claim on an input-only line not refused EACCES");
            assert!(api::rhu_gpio_claim(io, gpio_math::MODE_INPUT, gpio_math::PULL_DOWN, 0) == 0, "gpio-refuse: the input claim on the input-only line refused");
            assert!(api::rhu_gpio_release(io) == 0, "gpio-refuse: the input-only line's release refused");
        }
        let l = board::facts::CHECK_FREE;
        assert!(api::rhu_gpio_claim(l, gpio_math::MODE_INPUT, gpio_math::PULL_NONE, 0) == 0, "gpio-refuse: the input claim refused");
        assert!(api::rhu_gpio_write(l, 1) == gpio::ERR_ACCES, "gpio-refuse: a write on an input not refused EACCES");
        assert!(api::rhu_gpio_release(l) == 0, "gpio-refuse: the release refused");
        assert!(api::rhu_gpio_read(l) == sched::ERR_INVAL, "gpio-refuse: a read on an unclaimed line not refused EINVAL");
        assert!(api::rhu_gpio_write(l, 0) == sched::ERR_INVAL, "gpio-refuse: a write on an unclaimed line not refused EINVAL");
        assert!(api::rhu_gpio_release(l) == sched::ERR_INVAL, "gpio-refuse: a release of an unclaimed line not refused EINVAL");
        assert!(api::rhu_gpio_edge(l, gpio_math::KIND_RISING) == sched::ERR_INVAL, "gpio-refuse: an edge on an unclaimed line not refused EINVAL");
        assert!(api::rhu_gpio_event_word(l).is_null(), "gpio-refuse: an unclaimed line's event word not NULL");
        assert!(api::rhu_gpio_claim(l, 7, gpio_math::PULL_NONE, 0) == sched::ERR_INVAL, "gpio-refuse: a bad mode not refused EINVAL");
        assert!(api::rhu_gpio_claim(l, gpio_math::MODE_INPUT, 5, 0) == sched::ERR_INVAL, "gpio-refuse: a bad pull not refused EINVAL");
        assert!(api::rhu_gpio_claim(l, gpio_math::MODE_OUTPUT, gpio_math::PULL_NONE, 2) == sched::ERR_INVAL, "gpio-refuse: a bad initial level not refused EINVAL");
        let (busy, inval, acces, last) = unsafe {
            let p = addr_of!(gpio::WITNESS);
            (read_volatile(addr_of!((*p).refused_busy)), read_volatile(addr_of!((*p).refused_inval)), read_volatile(addr_of!((*p).refused_acces)), read_volatile(addr_of!((*p).last_refused_code)))
        };
        let busy_floor: u32 = if board::facts::BOARD_CLAIMS.is_empty() { 0 } else { 1 };
        assert!(busy >= busy_floor && inval >= 8 && acces >= 1, "gpio-refuse: the witness counts busy {} (floor {}) inval {} acces {}", busy, busy_floor, inval, acces);
        assert!(last == sched::ERR_INVAL, "gpio-refuse: the last refusal's code reads {}", last);
        kprintln!("[api] PASS gpio-refuse (the board's line EBUSY, the space's end and a far line EINVAL, an unpopulated line EINVAL and an input-only output claim EACCES where the row has them, a write on an input EACCES, an unclaimed read, write, release and edge EINVAL with a NULL event word, three bad selectors EINVAL, the witness's counts and last code as expected)");
    }

    static EDGE_WAIT_WORD: AtomicUsize = AtomicUsize::new(0);
    static EDGE_WAIT_RESULT: AtomicI32 = AtomicI32::new(-1);
    static EDGE_WAIT_DONE: AtomicU32 = AtomicU32::new(0);

    extern "C" fn edge_waiter(_arg: *mut c_void) {
        let w = EDGE_WAIT_WORD.load(Ordering::Acquire) as *const u32;
        let v = unsafe { *w };
        let r = api::rhu_futex_wait(w, v, u64::MAX);
        EDGE_WAIT_RESULT.store(r, Ordering::Release);
        EDGE_WAIT_DONE.store(1, Ordering::Release);
        api::rhu_thread_exit();
    }

    /// The one-kind arm on the boards' software trigger (a wire-only check:
    /// the virtual backend has no trigger): RISING armed alone, the trigger
    /// fired, the word moves by exactly one. The trigger sets the rising
    /// pending bit on the STM32U5 and the RP2350, and the STM32F7's one
    /// pending bit is reported as the armed kind, never as a later look at
    /// the pad: a rising-only arm is the kind the trigger satisfies on each.
    #[cfg(not(any(feature = "board-mps2-an500", feature = "board-mps2-an505")))]
    fn check_gpio_edge_one_kind() {
        kprintln!("[api] gpio-edge-one-kind: a rising-only arm on the software trigger, the word moved by one");
        let inp = board::facts::CHECK_IN;
        // The input is pulled up: an input nothing holds is no premise of a
        // count (a pad nothing drives floats on the chip revisions whose pads
        // do not leak).
        assert!(api::rhu_gpio_claim(inp, gpio_math::MODE_INPUT, gpio_math::PULL_UP, 0) == 0, "gpio-edge-one-kind: the input claim refused");
        assert!(api::rhu_gpio_edge(inp, gpio_math::KIND_RISING) == 0, "gpio-edge-one-kind: the arm refused");
        let w = api::rhu_gpio_event_word(inp);
        assert!(!w.is_null(), "gpio-edge-one-kind: the armed line's event word is NULL");
        let start = unsafe { *w };
        assert!(gpio::software_edge(inp), "gpio-edge-one-kind: the software-triggered edge refused");
        let _ = gpio_wait_count(w, start, 1);
        api::rhu_sleep_ns(5_000_000);
        let d = gpio_math::edges_since(unsafe { *w }, start);
        assert!(d == 1, "gpio-edge-one-kind: the trigger counted {} times, want 1", d);
        assert!(api::rhu_gpio_edge(inp, gpio_math::KIND_NONE) == 0, "gpio-edge-one-kind: the disarm refused");
        assert!(api::rhu_gpio_release(inp) == 0, "gpio-edge-one-kind: the release refused");
        kprintln!("[api] PASS gpio-edge-one-kind (a rising-only arm counted the software-triggered edge exactly once)");
    }

    /// THE BUTTON FLIGHT (the feature test-gpio-button, never a shipped
    /// image; the boards with a button row): the LED row blinks five times
    /// at 250 ms for the eye, then the button row is claimed with its pull
    /// and both edges armed, and a ten-second wait on its event word lights
    /// the LED on the first event; the outcome is printed either way -- an
    /// interactive step that says "no press" honestly.
    #[cfg(all(feature = "test-gpio-button", any(feature = "board-adafruit-feather-rp2350-hstx", feature = "board-adafruit-feather-rp2350-hstx-psram", feature = "board-pimoroni-pico-lipo-2", feature = "board-pimoroni-pico-plus-2", feature = "board-rpi-pico2", feature = "board-st-nucleo-l4r5zi", feature = "board-st-nucleo-u5a5zj-q", feature = "board-st-stm32f746g-disco", feature = "board-st-stm32f769i-disco")))]
    pub fn button_flight() {
        let led = board::facts::LED0_LINE;
        let off = board::facts::LED0_ACTIVE_LOW as u32;
        let on = 1 - off;
        assert!(api::rhu_gpio_claim(led, gpio_math::MODE_OUTPUT, gpio_math::PULL_NONE, off) == 0, "flight: the LED claim refused");
        kprintln!("[flight] blinky: the LED row toggled five times at 250 ms");
        for _ in 0..5 {
            assert!(api::rhu_gpio_write(led, on) == 0, "flight: the LED write refused");
            api::rhu_sleep_ns(250_000_000);
            assert!(api::rhu_gpio_write(led, off) == 0, "flight: the LED write refused");
            api::rhu_sleep_ns(250_000_000);
        }
        let b = board::facts::BUTTON0_LINE;
        assert!(api::rhu_gpio_claim(b, gpio_math::MODE_INPUT, board::facts::BUTTON0_PULL, 0) == 0, "flight: the button claim refused");
        assert!(api::rhu_gpio_edge(b, gpio_math::KIND_BOTH) == 0, "flight: the arm refused");
        let w = api::rhu_gpio_event_word(b);
        assert!(!w.is_null(), "flight: the button's event word is NULL");
        let start = unsafe { *w };
        let t0 = api::rhu_now_ns();
        kprintln!("[flight] button: waiting up to 10 s for a press on the button row (an interactive step)");
        let r = api::rhu_futex_wait(w, start, t0 + 10_000_000_000);
        if gpio_math::edges_since(unsafe { *w }, start) > 0 {
            assert!(api::rhu_gpio_write(led, on) == 0, "flight: the LED write refused");
            kprintln!("[flight] button: pressed after {} ms; the LED lit", (api::rhu_now_ns() - t0) / 1_000_000);
        } else {
            kprintln!("[flight] button: no press within 10 s (an interactive step; the wait returned {})", r);
        }
    }

    /// THE UART FLAVORS (the features test-uart-loopback and test-uart-jumper,
    /// never a shipped image; the Pico rows): the row's port 0 -- or its port
    /// 1 under test-uart-port1 -- is claimed through the doors and set to
    /// 115200 8N1; the loopback image turns the block's loopback bit on, so
    /// the transmitter feeds the receiver inside the chip, and the jumper
    /// image expects the port's pair wired together by hand (GP0 to GP1 on
    /// port 0, the facts' pair; on port 1 the pair the project's pin words
    /// name, since UART1 has no default pins on the Pico rows). A
    /// 256-byte pattern is written in partial writes and read back within
    /// two seconds, compared byte for byte; the witness's counts and PASS
    /// are printed, the port released, and the image ends as the api flavor
    /// does (the memory console holds the transcript for the debugger).
    #[cfg(all(any(feature = "test-uart-loopback", feature = "test-uart-jumper"), rhu_uart_pl011))]
    pub fn uart_flight() {
        let port: u32 = if cfg!(feature = "test-uart-port1") { 1 } else { 0 };
        let (jtx, jrx, _) = crate::uart::pins_for(port as usize);
        let loopback = cfg!(feature = "test-uart-loopback");
        if loopback {
            kprintln!("[flight] uart: UART{} claimed through the doors at 115200 8N1, 256 bytes out and back over the block's loopback bit", port);
        } else {
            kprintln!("[flight] uart: UART{} claimed through the doors at 115200 8N1, 256 bytes out and back over a jumper from GP{} to GP{}", port, jtx, jrx);
        }
        let h = api::rhu_uart_claim(port);
        assert!(h >= 0, "flight: the UART0 claim answered {}", h);
        let mut ach = 0u32;
        assert!(api::rhu_uart_configure(h, 115_200, 8, crate::uart::PARITY_NONE, 1, &mut ach) == 0, "flight: the configure refused");
        kprintln!("[flight] uart: 115200 Bd asked, {} Bd achieved", ach);
        #[cfg(feature = "test-uart-loopback")]
        assert!(crate::uart::loopback(h, true), "flight: the loopback bit refused");
        let mut pat = [0u8; 256];
        for (i, b) in pat.iter_mut().enumerate() {
            *b = (i as u8) ^ 0x5A;
        }
        let w0 = unsafe { core::ptr::read_volatile(core::ptr::addr_of!(crate::uart::WITNESS)) };
        let mut back = [0u8; 256];
        let (mut sent, mut recv, mut writes) = (0usize, 0usize, 0u32);
        let deadline = api::rhu_now_ns() + 2_000_000_000;
        while recv < 256 {
            if sent < 256 {
                let t = api::rhu_uart_write(h, pat[sent..].as_ptr(), (256 - sent) as u32);
                assert!(t >= 0, "flight: the write answered {}", t);
                sent += t as usize;
                writes += 1;
            }
            let n = api::rhu_uart_read(h, back[recv..].as_mut_ptr(), (256 - recv) as u32);
            assert!(n >= 0, "flight: the read answered {}", n);
            recv += n as usize;
            if recv < 256 && n == 0 {
                assert!(api::rhu_now_ns() < deadline, "flight: {} of 256 bytes back within two seconds (is the wire there?)", recv);
                api::rhu_sleep_ns(1_000_000);
            }
        }
        for i in 0..256 {
            assert!(back[i] == pat[i], "flight: byte {} differs: {:#04x} sent, {:#04x} back", i, pat[i], back[i]);
        }
        let w = unsafe { core::ptr::read_volatile(core::ptr::addr_of!(crate::uart::WITNESS)) };
        kprintln!(
            "[flight] uart: 256 bytes in {} writes; the witness {} out, {} in, {} interrupts, {} overruns, {} framing, {} parity, {} dropped",
            writes,
            w.bytes_out.wrapping_sub(w0.bytes_out),
            w.bytes_in.wrapping_sub(w0.bytes_in),
            w.irqs.wrapping_sub(w0.irqs),
            w.overruns,
            w.framing,
            w.parity,
            w.rx_dropped
        );
        #[cfg(feature = "test-uart-loopback")]
        assert!(crate::uart::loopback(h, false), "flight: the loopback bit refused off");
        assert!(api::rhu_uart_release(h) == 0, "flight: the release refused");
        if loopback {
            kprintln!("[flight] PASS uart-loopback (256 bytes out and back byte-equal over the block's loopback bit, the port released)");
        } else {
            kprintln!("[flight] PASS uart-jumper (256 bytes out and back byte-equal over a jumper from GP{} to GP{}, the port released)", jtx, jrx);
        }
    }

    /// One edge on the input: on a row whose output loops back to the input
    /// (the virtual backend) the toggle IS the edge; on a board the toggle
    /// drives the LED row for the eye and the family's software trigger
    /// makes the edge on the input.
    fn gpio_fire_one(out: Option<u32>, inp: u32, i: u32) {
        if let Some(o) = out {
            assert!(api::rhu_gpio_write(o, (i % 2 == 0) as u32) == 0, "gpio-edge: the toggle {} refused", i);
        }
        if out.is_none() || !board::facts::LOOPBACK {
            assert!(gpio::software_edge(inp), "gpio-edge: the software-triggered edge {} refused", i);
        }
    }

    /// The word read until it has moved by at least `want` since `v`, within a bound.
    fn gpio_wait_count(w: *const u32, v: u32, want: u32) -> u32 {
        let mut spins = 0u32;
        loop {
            let now = unsafe { *w };
            if gpio_math::edges_since(now, v) >= want {
                return now;
            }
            spins += 1;
            assert!(spins < 200, "gpio-edge: the word did not move by {} within the bound (at {} since {})", want, gpio_math::edges_since(now, v), v);
            api::rhu_sleep_ns(1_000_000);
        }
    }

    fn check_gpio_edge() {
        kprintln!("[api] gpio-edge: an armed input counted eight spaced edges once each, a parked waiter woken by one edge through the futex, the compare-and-wait, a burst coalesced, the level and wakeup kinds refused");
        let inp = board::facts::CHECK_IN;
        let out = board::facts::CHECK_OUT;
        // The input is pulled up: an input nothing holds is no premise of a
        // count (a pad nothing drives floats on the chip revisions whose pads
        // do not leak).
        assert!(api::rhu_gpio_claim(inp, gpio_math::MODE_INPUT, gpio_math::PULL_UP, 0) == 0, "gpio-edge: the input claim refused");
        assert!(api::rhu_gpio_edge(inp, gpio_math::KIND_LEVEL_HIGH) == gpio::ERR_NOTSUP, "gpio-edge: a level kind not refused ENOTSUP");
        assert!(api::rhu_gpio_edge(inp, gpio_math::KIND_WAKE_BOTH) == gpio::ERR_NOTSUP, "gpio-edge: a wakeup kind not refused ENOTSUP");
        assert!(api::rhu_gpio_edge(inp, 9) == sched::ERR_INVAL, "gpio-edge: an unknown kind not refused EINVAL");
        if let Some(o) = out {
            // The loop's output opens LOW, so the first toggle (high) is a
            // change and every toggle after it alternates -- an open is not
            // an edge, and a write at the level already driven is not one.
            assert!(api::rhu_gpio_claim(o, gpio_math::MODE_OUTPUT, gpio_math::PULL_NONE, 0) == 0, "gpio-edge: the output claim refused");
        }
        assert!(api::rhu_gpio_edge(inp, gpio_math::KIND_BOTH) == 0, "gpio-edge: the arm refused");
        let w = api::rhu_gpio_event_word(inp);
        assert!(!w.is_null(), "gpio-edge: the armed line's event word is NULL");
        let start = unsafe { *w };
        let mut v = start;
        for i in 0..8u32 {
            gpio_fire_one(out, inp, i);
            let now = gpio_wait_count(w, v, 1);
            let d = gpio_math::edges_since(now, v);
            assert!(d == 1, "gpio-edge: edge {} counted {} times", i, d);
            v = now;
        }
        EDGE_WAIT_WORD.store(w as usize, Ordering::Release);
        EDGE_WAIT_RESULT.store(-1, Ordering::Release);
        EDGE_WAIT_DONE.store(0, Ordering::Release);
        let t = api::rhu_thread_spawn(Some(edge_waiter), core::ptr::null_mut(), 4 * 1024, 0x40, c"gpiow".as_ptr());
        assert!(t > 0, "gpio-edge: the waiter's spawn refused {}", t);
        let mut spins = 0u32;
        while sched::probe_futex_waiting(w) != 1 {
            api::rhu_sleep_ns(1_000_000);
            spins += 1;
            assert!(spins < 200, "gpio-edge: the waiter never parked on the event word");
        }
        gpio_fire_one(out, inp, 8);
        spins = 0;
        while EDGE_WAIT_DONE.load(Ordering::Acquire) == 0 {
            api::rhu_sleep_ns(1_000_000);
            spins += 1;
            assert!(spins < 200, "gpio-edge: the waiter was not woken by the edge");
        }
        assert!(EDGE_WAIT_RESULT.load(Ordering::Acquire) == 0, "gpio-edge: the waiter returned {}", EDGE_WAIT_RESULT.load(Ordering::Acquire));
        v = gpio_wait_count(w, v, 1);
        // The compare-and-wait: an edge between the snapshot and the wait returns at once.
        let snap = v;
        gpio_fire_one(out, inp, 9);
        v = gpio_wait_count(w, snap, 1);
        assert!(api::rhu_futex_wait(w, snap, u64::MAX) == sched::ERR_AGAIN, "gpio-edge: a wait on a moved word did not return EAGAIN at once");
        // A burst: two edges without a read between, one count of two.
        let snap2 = v;
        gpio_fire_one(out, inp, 10);
        gpio_fire_one(out, inp, 11);
        v = gpio_wait_count(w, snap2, 2);
        assert!(gpio_math::edges_since(v, snap2) == 2, "gpio-edge: the burst counted {}", gpio_math::edges_since(v, snap2));
        let total = gpio_math::edges_since(v, start);
        assert!(total == 12, "gpio-edge: {} edges in all, want 12", total);
        let counted = unsafe { (*addr_of!(gpio::WITNESS)).edges_counted };
        assert!(counted == 12, "gpio-edge: the witness counted {} edges, the word 12", counted);
        assert!(api::rhu_gpio_edge(inp, gpio_math::KIND_NONE) == 0, "gpio-edge: the disarm refused");
        // After the disarm: the toggle where the row has an output (on the
        // loopback row the toggle IS the edge, and a disarmed line must count
        // nothing; toggle 12 drives high after toggle 11's low); where the
        // edge would come from the software trigger, the trigger is REFUSED
        // on a disarmed line by contract, and that refusal is the check. The
        // word is then read unmoved either way.
        if let Some(o) = out {
            assert!(api::rhu_gpio_write(o, 1) == 0, "gpio-edge: the toggle 12 refused");
        }
        if out.is_none() || !board::facts::LOOPBACK {
            assert!(!gpio::software_edge(inp), "gpio-edge: the software trigger on a disarmed line was not refused");
        }
        api::rhu_sleep_ns(5_000_000);
        assert!(gpio_math::edges_since(unsafe { *w }, v) == 0, "gpio-edge: a disarmed line counted an edge");
        if let Some(o) = out {
            assert!(api::rhu_gpio_release(o) == 0, "gpio-edge: the output's release refused");
        }
        assert!(api::rhu_gpio_release(inp) == 0, "gpio-edge: the input's release refused");
        kprintln!("[api] PASS gpio-edge (eight spaced edges counted once each, a parked waiter woken by one edge through the futex, an edge between the snapshot and the wait returned EAGAIN at once, a burst of two coalesced into one count of two, the witness's count equal to the word's, a disarmed line counting nothing, the level and wakeup kinds refused)");
    }

    /// An I2C message record for the checks.
    #[cfg(rhu_bus_rows)]
    fn imsg(addr: u16, flags: u16, buf: &mut [u8]) -> bus_math::I2cMsg {
        bus_math::I2cMsg { addr, flags, len: buf.len() as u16, reserved: 0, buf: buf.as_mut_ptr() }
    }

    #[cfg(rhu_bus_rows)]
    fn bus_witness() -> bus::BusWitness {
        unsafe { core::ptr::read_volatile(addr_of!(bus::WITNESS)) }
    }

    /// The I2C doors against the emulator row's model of the ST block and
    /// the sensor behind it.
    #[cfg(all(rhu_bus_models, not(feature = "test-bus-doctored")))]
    fn check_bus_i2c() {
        kprintln!("[api] bus-i2c: the bus count stated; a bus claimed at 100 kHz and claimed again; its number read through the enumeration door (the chip's own block number, as a Linux node is named); the sensor model's identity and burst read through combined transactions and its reset written; an unanswered address, nine bad arguments, an unserved rate and a bus the board has none of each refused with its code");
        assert!(api::rhu_i2c_count() == 1, "bus-i2c: the count reads {}", api::rhu_i2c_count());
        let n = api::rhu_i2c_bus(0) as u32;
        assert!(api::rhu_i2c_bus(1) == sched::ERR_INVAL, "bus-i2c: an index past the count not refused EINVAL at the enumeration door");
        assert!(api::rhu_i2c_claim(n, 50_000) == bus::ERR_NOTSUP, "bus-i2c: an unserved rate not refused ENOTSUP");
        assert!(api::rhu_i2c_claim(99, 100_000) == sched::ERR_NODEV, "bus-i2c: a bus the board has none of not refused ENODEV at the claim");
        let mut reg = [0xD0u8];
        let mut id = [0u8; 1];
        let early = [imsg(0x76, 0, &mut reg), imsg(0x76, bus_math::MSG_READ, &mut id)];
        assert!(api::rhu_i2c_transfer(n, early.as_ptr(), 2, u64::MAX) == sched::ERR_INVAL, "bus-i2c: a transfer before the claim not refused EINVAL");
        assert!(api::rhu_i2c_claim(n, 100_000) == 0, "bus-i2c: the claim refused");
        assert!(api::rhu_i2c_claim(n, 100_000) == 0, "bus-i2c: the second claim did not answer 0");
        #[cfg(feature = "test-bus-rp2350")]
        assert!(api::rhu_i2c_claim(n, 400_000) == gpio::ERR_BUSY, "bus-i2c: a second claim at another rate not refused EBUSY");
        #[cfg(not(feature = "test-bus-rp2350"))]
        assert!(api::rhu_i2c_claim(n, 400_000) == bus::ERR_NOTSUP, "bus-i2c: a second claim at a rate this row does not serve not refused ENOTSUP");
        let msgs = [imsg(0x76, 0, &mut reg), imsg(0x76, bus_math::MSG_READ, &mut id)];
        let r = api::rhu_i2c_transfer(n, msgs.as_ptr(), 2, u64::MAX);
        assert!(r == 0, "bus-i2c: the identity read answered {}", r);
        assert!(id[0] == 0x60, "bus-i2c: the identity register read {:#04x}", id[0]);
        let mut reg2 = [0xF7u8];
        let mut burst = [0u8; 8];
        let msgs = [imsg(0x76, 0, &mut reg2), imsg(0x76, bus_math::MSG_READ, &mut burst)];
        let r = api::rhu_i2c_transfer(n, msgs.as_ptr(), 2, api::rhu_now_ns() + 100_000_000);
        assert!(r == 0, "bus-i2c: the burst read answered {}", r);
        assert!(burst == [0x80, 0x00, 0x00, 0x80, 0x00, 0x00, 0x80, 0x00], "bus-i2c: the burst read {:?}", burst);
        let mut reset = [0xE0u8, 0xB6];
        let msgs = [imsg(0x76, 0, &mut reset)];
        let r = api::rhu_i2c_transfer(n, msgs.as_ptr(), 1, u64::MAX);
        assert!(r == 0, "bus-i2c: the reset write answered {}", r);
        let mut probe = [0u8; 1];
        let msgs = [imsg(0x77, bus_math::MSG_READ, &mut probe)];
        let r = api::rhu_i2c_transfer(n, msgs.as_ptr(), 1, u64::MAX);
        assert!(r == bus::ERR_NXIO, "bus-i2c: an unanswered address answered {}, want ENXIO", r);
        let mut one = [0u8; 1];
        let mut big = [0u8; 256];
        let bad_flag = [imsg(0x76, 2, &mut one)];
        assert!(api::rhu_i2c_transfer(n, bad_flag.as_ptr(), 1, u64::MAX) == sched::ERR_INVAL, "bus-i2c: a bad flag not refused EINVAL");
        let mut reserved = imsg(0x76, 0, &mut one);
        reserved.reserved = 1;
        assert!(api::rhu_i2c_transfer(n, [reserved].as_ptr(), 1, u64::MAX) == sched::ERR_INVAL, "bus-i2c: a reserved word not refused EINVAL");
        let long = [imsg(0x76, 0, &mut big)];
        assert!(api::rhu_i2c_transfer(n, long.as_ptr(), 1, u64::MAX) == sched::ERR_INVAL, "bus-i2c: a 256-byte message not refused EINVAL");
        let ok = [imsg(0x76, 0, &mut one)];
        assert!(api::rhu_i2c_transfer(n, ok.as_ptr(), 0, u64::MAX) == sched::ERR_INVAL, "bus-i2c: a count of 0 not refused EINVAL");
        assert!(api::rhu_i2c_transfer(n, ok.as_ptr(), 9, u64::MAX) == sched::ERR_INVAL, "bus-i2c: a count of 9 not refused EINVAL");
        let null = bus_math::I2cMsg { addr: 0x76, flags: 0, len: 1, reserved: 0, buf: core::ptr::null_mut() };
        assert!(api::rhu_i2c_transfer(n, [null].as_ptr(), 1, u64::MAX) == sched::ERR_INVAL, "bus-i2c: a NULL buffer with a length not refused EINVAL");
        let far = [imsg(0x80, 0, &mut one)];
        assert!(api::rhu_i2c_transfer(n, far.as_ptr(), 1, u64::MAX) == sched::ERR_INVAL, "bus-i2c: an address over 0x7F not refused EINVAL");
        assert!(api::rhu_i2c_transfer(99, ok.as_ptr(), 1, u64::MAX) == sched::ERR_NODEV, "bus-i2c: a bus the board has none of not refused ENODEV at the transfer");
        assert!(api::rhu_i2c_transfer(n, core::ptr::null(), 1, u64::MAX) == sched::ERR_INVAL, "bus-i2c: a NULL message list not refused EINVAL");
        let w = bus_witness();
        assert!(w.i2c_buses == 1 && w.i2c_claims == 1 && w.i2c_transfers == 3 && w.i2c_bytes == 13, "bus-i2c: the witness counts {} buses, {} claims, {} transfers, {} bytes", w.i2c_buses, w.i2c_claims, w.i2c_transfers, w.i2c_bytes);
        assert!(w.recoveries == 0 && w.resets == 0 && w.last_refused_code == sched::ERR_INVAL, "bus-i2c: the witness counts {} recoveries, {} resets, the last code {}", w.recoveries, w.resets, w.last_refused_code);
        kprintln!("[api] PASS bus-i2c (one bus, claimed once and again, the identity 0x60 and the eight-byte burst read, the reset acknowledged, an unanswered address ENXIO, nine bad arguments EINVAL, a bus the board has none of ENODEV, the rate ENOTSUP, the witness's counts as expected)");
    }

    /// The SPI doors against the emulator row's model of the ST block and
    /// the memory behind it: the settings ride each transfer, the select is
    /// the device's.
    #[cfg(all(rhu_bus_models, not(feature = "test-bus-doctored")))]
    fn check_bus_spi() {
        kprintln!("[api] bus-spi: the bus count stated; a bus claimed and claimed again; its number read through the enumeration door; the memory model's identity read under the first select at the achieved rate, echoes of four and five hundred and twelve bytes through the pump, a NULL transmit buffer sending ones, another mode and rate on a later transfer, a transfer with no select; seven bad arguments and a bus the board has none of refused with their codes");
        assert!(api::rhu_spi_count() == 1, "bus-spi: the count reads {}", api::rhu_spi_count());
        let n = api::rhu_spi_bus(0) as u32;
        assert!(api::rhu_spi_bus(1) == sched::ERR_INVAL, "bus-spi: an index past the count not refused EINVAL at the enumeration door");
        assert!(api::rhu_spi_claim(99) == sched::ERR_NODEV, "bus-spi: a bus the board has none of not refused ENODEV at the claim");
        let tx = [0x9Fu8, 0, 0, 0];
        let mut rx = [0u8; 4];
        assert!(api::rhu_spi_transfer(n, 0, 0, 1_000_000, tx.as_ptr(), rx.as_mut_ptr(), 4) == sched::ERR_INVAL, "bus-spi: a transfer before the claim not refused EINVAL");
        assert!(api::rhu_spi_claim(n) == 0, "bus-spi: the claim refused");
        assert!(api::rhu_spi_claim(n) == 0, "bus-spi: the second claim did not answer 0");
        let r = api::rhu_spi_transfer(n, 0, 0, 1_000_000, tx.as_ptr(), rx.as_mut_ptr(), 4);
        assert!(r == 1_000_000, "bus-spi: the identity transfer answered {}, want the achieved rate 1000000", r);
        assert!(rx[1..4] == [0xEF, 0x40, 0x18], "bus-spi: the identity read {:?}", rx);
        let tx = [0x03u8, 1, 2, 3, 4];
        let mut rx = [0u8; 5];
        assert!(api::rhu_spi_transfer(n, 0, 0, 1_000_000, tx.as_ptr(), rx.as_mut_ptr(), 5) == 1_000_000, "bus-spi: the four-byte echo refused");
        assert!(rx[1..5] == [1, 2, 3, 4], "bus-spi: the four-byte echo read {:?}", rx);
        let mut big_tx = [0u8; 512];
        let mut big_rx = [0u8; 512];
        big_tx[0] = 0x03;
        for (i, b) in big_tx.iter_mut().enumerate().skip(1) {
            *b = i as u8;
        }
        assert!(api::rhu_spi_transfer(n, 0, 0, 1_000_000, big_tx.as_ptr(), big_rx.as_mut_ptr(), 512) == 1_000_000, "bus-spi: the 512-byte echo refused");
        assert!(big_rx[1..] == big_tx[1..], "bus-spi: the 512-byte echo differs from what was sent");
        let mut ones = [0u8; 4];
        assert!(api::rhu_spi_transfer(n, 0, 0, 1_000_000, core::ptr::null(), ones.as_mut_ptr(), 4) == 1_000_000, "bus-spi: the NULL transmit transfer refused");
        assert!(ones == [0xFF; 4], "bus-spi: a NULL transmit buffer did not send ones ({:?})", ones);
        let tx = [0x03u8, 9, 8];
        let mut rx = [0u8; 3];
        let r = api::rhu_spi_transfer(n, 0, 3, 500_000, tx.as_ptr(), rx.as_mut_ptr(), 3);
        assert!(r == 500_000 && rx[1..3] == [9, 8], "bus-spi: mode 3 at 500 kHz answered {} with {:?}", r, rx);
        let mut none = [0u8; 3];
        let r = api::rhu_spi_transfer(n, bus_math::SELECT_NONE, 0, 1_000_000, tx.as_ptr(), none.as_mut_ptr(), 3);
        assert!(r == 1_000_000 && none == [0xFF; 3], "bus-spi: the no-select transfer answered {} with {:?}", r, none);
        assert!(api::rhu_spi_transfer(n, 2, 0, 1_000_000, tx.as_ptr(), rx.as_mut_ptr(), 3) == sched::ERR_INVAL, "bus-spi: a select past the bus's two not refused EINVAL");
        assert!(api::rhu_spi_transfer(n, 0, 4, 1_000_000, tx.as_ptr(), rx.as_mut_ptr(), 3) == sched::ERR_INVAL, "bus-spi: a mode over 3 not refused EINVAL");
        assert!(api::rhu_spi_transfer(n, 0, 0, 1_000_000, tx.as_ptr(), rx.as_mut_ptr(), 0) == sched::ERR_INVAL, "bus-spi: a length of 0 not refused EINVAL");
        assert!(api::rhu_spi_transfer(n, 0, 0, 1_000_000, tx.as_ptr(), rx.as_mut_ptr(), 65536) == sched::ERR_INVAL, "bus-spi: a length over 65535 not refused EINVAL");
        assert!(api::rhu_spi_transfer(n, 0, 0, 100, tx.as_ptr(), rx.as_mut_ptr(), 3) == sched::ERR_INVAL, "bus-spi: a rate below the ladder's floor not refused EINVAL");
        assert!(api::rhu_spi_transfer(99, 0, 0, 1_000_000, tx.as_ptr(), rx.as_mut_ptr(), 3) == sched::ERR_NODEV, "bus-spi: a bus the board has none of not refused ENODEV at the transfer");
        assert!(api::rhu_spi_transfer(n, 0, 0, 1_000_000, core::ptr::null(), core::ptr::null_mut(), 3) == sched::ERR_INVAL, "bus-spi: NULL transmit and receive buffers not refused EINVAL");
        let w = bus_witness();
        assert!(w.spi_buses == 1 && w.spi_claims == 1 && w.spi_transfers == 6 && w.spi_bytes == 4 + 5 + 512 + 4 + 3 + 3, "bus-spi: the witness counts {} buses, {} claims, {} transfers, {} bytes", w.spi_buses, w.spi_claims, w.spi_transfers, w.spi_bytes);
        assert!(w.spi_last_hz == 1_000_000 && w.resets == 0 && w.last_refused_code == sched::ERR_INVAL, "bus-spi: the witness's last rate {}, resets {}, last code {}", w.spi_last_hz, w.resets, w.last_refused_code);
        kprintln!("[api] PASS bus-spi (one bus, claimed once and again, the identity EF 40 18 at 1000000 Hz, the echoes of four and five hundred and twelve bytes intact, ones sent for a NULL buffer, mode 3 at 500000 Hz, the no-select transfer reading ones, seven bad arguments EINVAL, a bus the board has none of ENODEV, the witness's counts as expected)");
    }

    #[cfg(all(rhu_bus_models, not(feature = "test-bus-doctored")))]
    static TENANCY_DONE: AtomicU32 = AtomicU32::new(0);
    #[cfg(all(rhu_bus_models, not(feature = "test-bus-doctored")))]
    static TENANCY_RESULT: AtomicI32 = AtomicI32::new(0);
    #[cfg(all(rhu_bus_models, not(feature = "test-bus-doctored")))]
    static mut TENANCY_TX: [u8; 4096] = [0; 4096];
    #[cfg(all(rhu_bus_models, not(feature = "test-bus-doctored")))]
    static mut TENANCY_RX: [u8; 4096] = [0; 4096];

    /// The worker's long transfer: an echo of four thousand and ninety-six
    /// bytes, about thirty-three milliseconds on the paced wire.
    #[cfg(all(rhu_bus_models, not(feature = "test-bus-doctored")))]
    extern "C" fn tenancy_worker(_arg: *mut c_void) {
        let (tx, rx) = (addr_of_mut!(TENANCY_TX), addr_of_mut!(TENANCY_RX));
        let r = api::rhu_spi_transfer(api::rhu_spi_bus(0) as u32, 0, 0, 1_000_000, tx as *const u8, rx as *mut u8, 4096);
        let mut intact = r == 1_000_000;
        for i in 1..4096usize {
            if unsafe { (*tx)[i] != (*rx)[i] } {
                intact = false;
            }
        }
        TENANCY_RESULT.store(if intact { r } else { -1 }, Ordering::Release);
        TENANCY_DONE.store(1, Ordering::Release);
        api::rhu_thread_exit();
    }

    /// The tenancy and the masking consequence: a worker's long transfer,
    /// this thread's transfer on the same bus waiting on the tenancy word
    /// until it ends, and a band-priority counter through both.
    #[cfg(all(rhu_bus_models, not(feature = "test-bus-doctored")))]
    fn check_bus_tenancy() {
        kprintln!("[api] bus-tenancy: a worker's long transfer on the bus, this thread's transfer on the same bus sleeping on the tenancy word until it ends; a band-priority counter through both, counting every tick");
        unsafe {
            let tx = addr_of_mut!(TENANCY_TX);
            (*tx)[0] = 0x03;
            for i in 1..4096usize {
                (*tx)[i] = (i * 7) as u8;
            }
        }
        TENANCY_DONE.store(0, Ordering::Release);
        let waits_before = bus_witness().tenancy_waits;
        FP_BAND.hits.store(0, Ordering::Release);
        fp_band_arm(true);
        let t = api::rhu_thread_spawn(Some(tenancy_worker), core::ptr::null_mut(), 16 * 1024, 0x40, c"busw".as_ptr());
        assert!(t > 0, "bus-tenancy: the worker's spawn refused {}", t);
        api::rhu_sleep_ns(2_000_000);
        let tx = [0x03u8, 5, 6, 7];
        let mut rx = [0u8; 4];
        let t0 = api::rhu_now_ns();
        let r = api::rhu_spi_transfer(api::rhu_spi_bus(0) as u32, 0, 0, 1_000_000, tx.as_ptr(), rx.as_mut_ptr(), 4);
        let waited_ns = api::rhu_now_ns() - t0;
        let mut spins = 0u32;
        while TENANCY_DONE.load(Ordering::Acquire) == 0 {
            api::rhu_sleep_ns(1_000_000);
            spins += 1;
            assert!(spins < 500, "bus-tenancy: the worker never finished");
        }
        fp_band_arm(false);
        let hits = FP_BAND.hits.load(Ordering::Acquire);
        assert!(r == 1_000_000 && rx[1..4] == [5, 6, 7], "bus-tenancy: this thread's transfer answered {} with {:?}", r, rx);
        assert!(TENANCY_RESULT.load(Ordering::Acquire) == 1_000_000, "bus-tenancy: the worker's transfer answered {}", TENANCY_RESULT.load(Ordering::Acquire));
        assert!(waited_ns >= 20_000_000, "bus-tenancy: this thread waited {} ns for the bus, want at least 20 ms", waited_ns);
        let w = bus_witness();
        assert!(w.tenancy_waits == waits_before + 1, "bus-tenancy: the witness counts {} waits, want {}", w.tenancy_waits, waits_before + 1);
        assert!(hits >= 20, "bus-tenancy: the band counter counted {} ticks through the transfers, want at least 20", hits);
        kprintln!("[api] PASS bus-tenancy (the worker's transfer of 4096 bytes intact, this thread's transfer served after it with a wait of at least 20 ms on the tenancy word, the wait counted by the witness, the band counter counting at least 20 ticks through both: the transactions masked nothing)");
    }

    /// The bus kernel calls on a board's real blocks, with nothing asked of
    /// any device: the pads' ownership through the claims table, an address
    /// nobody answers, transfers of ones. The text is the same on every row
    /// (no bus number and no rate is printed; the rate is asserted against the
    /// ladder's arithmetic from the row's facts). The address probed is the
    /// lowest one the I2C-bus specification does not reserve.
    #[cfg(all(rhu_bus_rows, not(rhu_bus_models)))]
    fn check_bus_pads() {
        kprintln!("[api] bus-pads: the bus counts and numbers stated; each bus's claim refused whole while the program holds one of its pads, taken once the pad is released, its pads the board's from then on; an address nobody answers; a chip select the program holds refused at its first use, taken at the next; transfers of ones under that select and under none");
        let input = |l: u32| api::rhu_gpio_claim(l, gpio_math::MODE_INPUT, gpio_math::PULL_UP, 0);
        let (i2cs, spis) = (board::facts::I2C_BUSES, board::facts::SPI_BUSES);
        assert!(!i2cs.is_empty() && !spis.is_empty() && api::rhu_i2c_count() as usize == i2cs.len() && api::rhu_spi_count() as usize == spis.len(), "bus-pads: the counts read {} and {}", api::rhu_i2c_count(), api::rhu_spi_count());
        let line_of = |pads: &[bus_math::Pad], role: bus_math::PadRole| pads.iter().find(|p| p.role == role).map_or(u32::MAX, |p| p.line as u32);
        // Every bus of the row is walked, in the tables' order.
        for (k, i2c) in i2cs.iter().enumerate() {
            let ni = api::rhu_i2c_bus(k as u32) as u32;
            assert!(ni == i2c.number, "bus-pads: the enumeration answered {} for the I2C bus at index {}, the facts state {}", ni, k, i2c.number);
            let (scl, sda) = (line_of(i2c.pads, bus_math::PadRole::I2cClock), line_of(i2c.pads, bus_math::PadRole::I2cData));
            assert!(input(scl) == 0, "bus-pads: the program's claim on the I2C clock pad refused");
            assert!(api::rhu_i2c_claim(ni, 100_000) == gpio::ERR_BUSY, "bus-pads: the I2C claim not refused EBUSY while the program holds a pad");
            assert!(input(sda) == 0, "bus-pads: the refused claim took the data pad");
            assert!(api::rhu_gpio_release(sda) == 0 && api::rhu_gpio_release(scl) == 0, "bus-pads: the I2C pads' release refused");
            assert!(api::rhu_i2c_claim(ni, 100_000) == 0, "bus-pads: the I2C claim refused");
            assert!(api::rhu_i2c_claim(ni, 100_000) == 0, "bus-pads: the second I2C claim did not answer 0");
            assert!(input(sda) == gpio::ERR_BUSY && input(scl) == gpio::ERR_BUSY, "bus-pads: the program's claim on a claimed bus's pad not refused EBUSY");
            // A one-byte read: every family's block can ask an address that way.
            let mut probe = [0u8; 1];
            let msgs = [imsg(0x08, bus_math::MSG_READ, &mut probe)];
            let r = api::rhu_i2c_transfer(ni, msgs.as_ptr(), 1, api::rhu_now_ns() + 100_000_000);
            assert!(r == bus::ERR_NXIO, "bus-pads: an address nobody answers answered {}, want ENXIO", r);
        }
        let mut want = 0i32;
        for (k, spi) in spis.iter().enumerate() {
            let ns = api::rhu_spi_bus(k as u32) as u32;
            assert!(ns == spi.number, "bus-pads: the enumeration answered {} for the SPI bus at index {}, the facts state {}", ns, k, spi.number);
            let sck = line_of(spi.pads, bus_math::PadRole::SpiClock);
            assert!(input(sck) == 0, "bus-pads: the program's claim on the SPI clock pad refused");
            assert!(api::rhu_spi_claim(ns) == gpio::ERR_BUSY, "bus-pads: the SPI claim not refused EBUSY while the program holds a pad");
            assert!(api::rhu_gpio_release(sck) == 0, "bus-pads: the SPI clock pad's release refused");
            assert!(api::rhu_spi_claim(ns) == 0, "bus-pads: the SPI claim refused");
            assert!(api::rhu_spi_claim(ns) == 0, "bus-pads: the second SPI claim did not answer 0");
            assert!(input(sck) == gpio::ERR_BUSY, "bus-pads: the program's claim on a claimed bus's pad not refused EBUSY");
            want = match board::spi_driver::divisor(spi.kernel_hz, 1_000_000) {
                Some((_, achieved)) => achieved as i32,
                None => 0,
            };
            let sel = spi.selects.first().map_or(u32::MAX, |&l| l as u32);
            let mut under = [0u8; 4];
            assert!(input(sel) == 0, "bus-pads: the program's claim on the first select refused");
            assert!(api::rhu_spi_transfer(ns, 0, 0, 1_000_000, core::ptr::null(), under.as_mut_ptr(), 4) == gpio::ERR_BUSY, "bus-pads: a transfer under a select the program holds not refused EBUSY");
            assert!(api::rhu_gpio_release(sel) == 0, "bus-pads: the select's release refused");
            let r = api::rhu_spi_transfer(ns, 0, 0, 1_000_000, core::ptr::null(), under.as_mut_ptr(), 4);
            assert!(r == want && want > 0, "bus-pads: the transfer under the first select answered {}, want the achieved rate {}", r, want);
            assert!(input(sel) == gpio::ERR_BUSY, "bus-pads: the program's claim on a claimed select not refused EBUSY");
            let mut none = [0u8; 4];
            let r = api::rhu_spi_transfer(ns, bus_math::SELECT_NONE, 0, 1_000_000, core::ptr::null(), none.as_mut_ptr(), 4);
            assert!(r == want && none == [0xFF; 4], "bus-pads: the no-select transfer answered {} with {:?}", r, none);
        }
        let w = bus_witness();
        let (ni, ns) = (i2cs.len() as u32, spis.len() as u32);
        assert!(w.i2c_buses == ni && w.spi_buses == ns && w.i2c_claims == ni && w.spi_claims == ns, "bus-pads: the witness counts {} and {} buses, {} and {} claims", w.i2c_buses, w.spi_buses, w.i2c_claims, w.spi_claims);
        assert!(w.spi_transfers == 2 * ns && w.spi_bytes == 8 * ns && w.spi_last_hz == want as u32 && w.resets == 0, "bus-pads: the witness counts {} transfers, {} bytes, the last rate {}, {} resets", w.spi_transfers, w.spi_bytes, w.spi_last_hz, w.resets);
        kprintln!("[api] PASS bus-pads (each bus's claim EBUSY while the program held a pad and nothing taken, 0 once released and 0 again, its pads EBUSY to the program from then on; an address nobody answers ENXIO; the held select EBUSY at its first use, the transfer under it at the achieved rate once released, the select EBUSY to the program from then on; the no-select transfer reading ones; the witness's counts as expected)");
    }

    /// The doctored image's run in place of the three checks. On both
    /// models: the sensor refuses the second data byte of a write (the door
    /// answers EIO and names the byte); the block never completes a non-last
    /// message (the door answers ETIMEDOUT no sooner than its budget less a
    /// tick and resets the block); the same with a device left holding the
    /// data line -- freed by the clocks, freed by the clock held low 35 ms,
    /// not freed -- each told in its line and counted or not; a device that
    /// holds the clock low 25 ms in one byte is SERVED; the unbounded pump
    /// loses bytes to the receive queue's honest overflow. On the register
    /// file that keeps the master bit and the internal select in two
    /// registers, the first transfer after the claim takes the wrong write
    /// order: the block falls out of master mode and the call answers
    /// EIO at once, never after a budget. On the RP2350's models, the paths
    /// no honest run reaches: a claim on a block that has not come to rest
    /// and the first transfer after it (the claim's registers written again
    /// at the opening), an opening whose disable does not complete, an abort
    /// whose stop never comes, a clock held while the pump waits with a byte
    /// nobody asked for in the receive queue, a reserved address, an SPI
    /// clock that never rests and an overrun at a burst's end.
    #[cfg(all(rhu_bus_models, feature = "test-bus-doctored"))]
    fn check_bus_doctored() {
        use board::bus_backend::{doctor, probe_i2c_resets, probe_spi_drops, Doctor};
        #[cfg(feature = "test-bus-rp2350")]
        use board::bus_backend::probe_i2c_counts;
        /// The transfer the doctored block never completes, timed in ticks.
        fn stuck(ni: u32) -> (i32, u32) {
            let mut reg = [0xD0u8];
            let mut id = [0u8; 1];
            let msgs = [imsg(0x76, 0, &mut reg), imsg(0x76, bus_math::MSG_READ, &mut id)];
            let t0 = sched::now_ms();
            let r = api::rhu_i2c_transfer(ni, msgs.as_ptr(), 2, u64::MAX);
            (r, sched::now_ms().wrapping_sub(t0))
        }
        kprintln!("[api] bus-doctored: the doctored models and the unbounded pump armed (no bus check passes on this image)");
        let budget = bus_math::i2c_wait_ticks(100_000, board::TICK_HZ);
        let ni = api::rhu_i2c_bus(0) as u32;
        #[cfg(feature = "test-bus-rp2350")]
        doctor(Doctor::ClaimUnrested);
        #[cfg(feature = "test-bus-rp2350")]
        let hz = 400_000;
        #[cfg(not(feature = "test-bus-rp2350"))]
        let hz = 100_000;
        assert!(api::rhu_i2c_claim(ni, hz) == 0, "bus-doctored: the I2C claim refused");
        #[cfg(feature = "test-bus-rp2350")]
        {
            assert!(bus_witness().resets == 1, "bus-doctored: the claim of a block that had not come to rest counted {} resets", bus_witness().resets);
            sched::sleep_ms(2);
        }
        let mut reset = [0xE0u8, 0xB6];
        let msgs = [imsg(0x76, 0, &mut reset)];
        let r = api::rhu_i2c_transfer(ni, msgs.as_ptr(), 1, u64::MAX);
        let w = bus_witness();
        kprintln!("[api] bus-doctored: the data byte not acknowledged answered {} at message {} byte {}", r, w.last_message, w.last_byte);
        #[cfg(feature = "test-bus-rp2350")]
        {
            assert!(r == sched::ERR_IO && probe_i2c_resets() == 0, "bus-doctored: the first transfer after the claim that had not rested answered {} with {} resets of the block", r, probe_i2c_resets());
            kprintln!("[api] bus-doctored: the claim of a block that had not come to rest answered 0 and was told; the first transfer found the claim's registers lost and wrote them again");
            let clk = board::facts::I2C_BUSES[0].kernel_hz;
            let Some(c) = crate::rp2350_bus_math::i2c_counts(clk, hz) else {
                panic!("bus-doctored: no counts for the fast-mode claim at {} Hz", clk);
            };
            let (l, h, s, pair) = probe_i2c_counts();
            assert!(c.speed == 2 && pair == 2 && (l, h, s) == (c.lcnt, c.hcnt, c.spklen), "bus-doctored: the fast-mode claim programmed low {} high {} spike {} to register pair {}, want low {} high {} spike {} to pair 2", l, h, s, pair, c.lcnt, c.hcnt, c.spklen);
            kprintln!("[api] bus-doctored: the fast-mode claim at {} Hz programmed the fast counts (low {}, high {}, spike {} at {} Hz; {} Hz achieved)", hz, c.lcnt, c.hcnt, c.spklen, clk, c.achieved_hz);
            // The two shapes below rest on the doctored claim's 1 ms of
            // un-rest (the model's UNRESTED_NS) having passed at the next
            // opening -- the 2 ms sleep above -- deterministic under icount.
            doctor(Doctor::StatusStuck);
            let t0 = sched::now_ms();
            let r = api::rhu_i2c_transfer(ni, msgs.as_ptr(), 1, u64::MAX);
            let dt = sched::now_ms().wrapping_sub(t0);
            assert!(r == sched::ERR_TIMEDOUT && dt <= 1 && probe_i2c_resets() == 1 && bus_witness().resets == 2, "bus-doctored: the opening whose disable did not complete answered {} after {} ticks, {} resets by the model, {} by the witness", r, dt, probe_i2c_resets(), bus_witness().resets);
            kprintln!("[api] bus-doctored: an opening whose disable did not complete answered {} at once through the reset path", r);
            doctor(Doctor::AbortNoStop);
            let t0 = sched::now_ms();
            let r = api::rhu_i2c_transfer(ni, msgs.as_ptr(), 1, u64::MAX);
            let dt = sched::now_ms().wrapping_sub(t0);
            let w = bus_witness();
            assert!(r == sched::ERR_TIMEDOUT && w.last_message == 0 && w.last_byte == 1 && dt + 1 >= budget && dt <= budget + 4, "bus-doctored: the abort whose stop never came answered {} at message {} byte {} after {} ticks against a wait budget of {}", r, w.last_message, w.last_byte, dt, budget);
            kprintln!("[api] bus-doctored: an abort whose stop never came answered {} at message 0 byte 1 inside the budget", r);
            let mut twenty = [0u8; 20];
            let long = [imsg(0x76, 0, &mut twenty)];
            doctor(Doctor::StallNext);
            let t0 = sched::now_ms();
            let r = api::rhu_i2c_transfer(ni, long.as_ptr(), 1, u64::MAX);
            let dt = sched::now_ms().wrapping_sub(t0);
            assert!(r == sched::ERR_TIMEDOUT && dt + 1 >= budget && dt <= budget + 4, "bus-doctored: the clock held while the pump waited answered {} after {} ticks against a wait budget of {}", r, dt, budget);
            kprintln!("[api] bus-doctored: a clock held from the first of twenty bytes, with a byte nobody asked for in the receive queue, answered {} inside the budget", r);
            let mut one = [0u8; 1];
            for addr in [0x03u16, 0x78] {
                let reserved = [imsg(addr, 0, &mut one)];
                let r = api::rhu_i2c_transfer(ni, reserved.as_ptr(), 1, u64::MAX);
                assert!(r == sched::ERR_INVAL && bus_witness().last_message == 0, "bus-doctored: the reserved address {:#04x} answered {}", addr, r);
            }
            kprintln!("[api] bus-doctored: a reserved address answered {} at message 0 before the wire", sched::ERR_INVAL);
        }
        let (resets0, counted0) = (probe_i2c_resets(), bus_witness().resets);
        let (r, dt) = stuck(ni);
        assert!(r == sched::ERR_TIMEDOUT && dt + 1 >= budget && dt <= budget + 4, "bus-doctored: the stuck clock's refusal answered {} after {} ticks against a wait budget of {}", r, dt, budget);
        let w = bus_witness();
        assert!(probe_i2c_resets() == resets0 + 1 && w.resets == counted0 + 1 && w.recoveries == 0, "bus-doctored: the block reset {} times by the model, {} by the witness, {} recoveries", probe_i2c_resets() - resets0, w.resets - counted0, w.recoveries);
        kprintln!("[api] bus-doctored: the clock that never completed answered {} inside the budget; the block reset once", r);
        doctor(Doctor::HeldClocks);
        let (r, dt) = stuck(ni);
        assert!(r == sched::ERR_TIMEDOUT && dt + 1 >= budget && dt <= budget + 4 && bus_witness().recoveries == 1, "bus-doctored: the data line freed by the clocks answered {} after {} ticks, {} recoveries", r, dt, bus_witness().recoveries);
        kprintln!("[api] bus-doctored: a data line held after the timeout came free by the clocks; the bus recovered and was counted");
        let hold = bus_math::I2C_CLOCK_HOLD_MS;
        doctor(Doctor::HeldLong);
        let (r, dt) = stuck(ni);
        assert!(r == sched::ERR_TIMEDOUT && dt + 1 >= budget + hold && dt <= budget + hold + 7 && bus_witness().recoveries == 2, "bus-doctored: the data line freed by the clock held low answered {} after {} ticks, {} recoveries", r, dt, bus_witness().recoveries);
        kprintln!("[api] bus-doctored: a data line held through nine clocks came free by the clock held low 35 ms; the bus recovered and was counted");
        doctor(Doctor::HeldStuck);
        let (r, dt) = stuck(ni);
        assert!(r == sched::ERR_TIMEDOUT && dt + 1 >= budget + hold && dt <= budget + hold + 7 && bus_witness().recoveries == 2, "bus-doctored: the data line never freed answered {} after {} ticks, {} recoveries", r, dt, bus_witness().recoveries);
        kprintln!("[api] bus-doctored: a data line held through the whole recovery was told and not counted as recovered");
        doctor(Doctor::Free);
        doctor(Doctor::Stretch);
        let mut reg = [0xD0u8];
        let one = [imsg(0x76, 0, &mut reg)];
        let t0 = sched::now_ms();
        let r = api::rhu_i2c_transfer(ni, one.as_ptr(), 1, u64::MAX);
        let dt = sched::now_ms().wrapping_sub(t0);
        assert!(r == 0 && dt + 1 >= bus_math::I2C_CLOCK_LOW_MS && dt < budget, "bus-doctored: the device that held the clock low 25 ms in one byte answered {} after {} ticks", r, dt);
        kprintln!("[api] bus-doctored: a device that held the clock low 25 ms in one byte was served");
        let ns = api::rhu_spi_bus(0) as u32;
        let counted0 = bus_witness().resets;
        assert!(api::rhu_spi_claim(ns) == 0, "bus-doctored: the SPI claim refused");
        #[cfg(feature = "test-bus-rp2350")]
        {
            assert!(bus_witness().resets == counted0 + 1, "bus-doctored: the SPI claim of a block whose reset did not complete counted {} resets", bus_witness().resets - counted0);
            kprintln!("[api] bus-doctored: the SPI claim of a block whose reset did not complete answered 0 and was told");
            let mut four = [0u8; 4];
            for (what, byte) in [(Doctor::SpiClockNever, 0u32), (Doctor::SpiOverrun, 3)] {
                doctor(what);
                let t0 = sched::now_ms();
                let r = api::rhu_spi_transfer(ns, 0, 0, 1_000_000, core::ptr::null(), four.as_mut_ptr(), 4);
                let dt = sched::now_ms().wrapping_sub(t0);
                let w = bus_witness();
                assert!(r == sched::ERR_TIMEDOUT && dt <= 1 && w.last_byte == byte, "bus-doctored: the SPI transfer answered {} at byte {} after {} ticks, want byte {}", r, w.last_byte, dt, byte);
            }
            kprintln!("[api] bus-doctored: an SPI clock that never came to rest answered {} at byte 0 at once; an overrun at a burst's end answered it at byte 3 at once", sched::ERR_TIMEDOUT);
        }
        let _ = counted0;
        if board::facts::SPI_BUSES[0].file == bus_math::SpiFile::U5 {
            let mut four = [0u8; 4];
            let t0 = sched::now_ms();
            let r = api::rhu_spi_transfer(ns, 0, 0, 1_000_000, core::ptr::null(), four.as_mut_ptr(), 4);
            let dt = sched::now_ms().wrapping_sub(t0);
            assert!(r == sched::ERR_IO && dt <= 1, "bus-doctored: the wrong write order answered {} after {} ticks (the control did not bite)", r, dt);
            kprintln!("[api] bus-doctored: the wrong write order answered {} at once; the block fell out of master mode and the fault was cleared", r);
        }
        let mut tx = [0u8; 64];
        let mut rx = [0u8; 64];
        tx[0] = 0x03;
        for (i, b) in tx.iter_mut().enumerate().skip(1) {
            *b = i as u8;
        }
        let r = api::rhu_spi_transfer(ns, 0, 0, 1_000_000, tx.as_ptr(), rx.as_mut_ptr(), 64);
        let drops = probe_spi_drops();
        assert!(drops >= 1 && r == sched::ERR_TIMEDOUT, "bus-doctored: the unbounded pump lost {} bytes and answered {} (the control did not bite)", drops, r);
        kprintln!("[api] bus-doctored: the unbounded pump lost {} of 64 bytes to the receive queue's overflow; the transfer answered {}", drops, r);
    }

    #[cfg(rhu_pwm_adc_rows)]
    fn pwm_witness() -> crate::pwm::PwmWitness {
        unsafe { core::ptr::read_volatile(addr_of!(crate::pwm::WITNESS)) }
    }

    #[cfg(rhu_pwm_adc_rows)]
    fn adc_witness() -> crate::adc::AdcWitness {
        unsafe { core::ptr::read_volatile(addr_of!(crate::adc::WITNESS)) }
    }

    /// The PWM doors against the emulator row's virtual timers: the
    /// quantization measured, the door's period tied to the timer's own
    /// reading of its registers (the doctored timer breaks the tie), the
    /// shared-period rule and the refusals.
    #[cfg(rhu_pwm_adc_models)]
    fn check_pwm_claim() {
        kprintln!("[api] pwm-claim: the count and the lines stated; a line claimed, a period the clock divides set exact and one it does not quantized, the door's period tied to the timer's own both times, the timer's other channel refused on a different period and served on the same; a second claim, a line without a timer function, a bad polarity, a duty past the period, a zero period, a period past the range and bad handles each refused with its code; the output driven and parked, released and claimed again");
        let n = api::rhu_pwm_count();
        assert!(n == crate::pwm::LINES as i32 && n == 3, "pwm-claim: the count reads {}", n);
        for (i, l) in board::facts::PWM_LINES.iter().enumerate() {
            assert!(api::rhu_pwm_line(i as u32) == l.line as i32, "pwm-claim: line {} reads {}", i, api::rhu_pwm_line(i as u32));
        }
        assert!(api::rhu_pwm_line(n as u32) == sched::ERR_INVAL, "pwm-claim: an index past the count not refused EINVAL");
        let l0 = board::facts::PWM_LINES[0].line as u32;
        let l1 = board::facts::PWM_LINES[1].line as u32;
        let h = api::rhu_pwm_claim(l0);
        assert!(h == 0, "pwm-claim: the claim answered {}", h);
        assert!(api::rhu_pwm_claim(l0) == crate::pwm::ERR_BUSY, "pwm-claim: a second claim on the line not refused EBUSY");
        assert!(api::rhu_pwm_claim(board::facts::CHECK_BOARD) == sched::ERR_INVAL, "pwm-claim: a line without a timer function not refused EINVAL");
        let mut ns = 0u64;
        assert!(api::rhu_pwm_period(h, &mut ns) == 0 && ns == 0, "pwm-claim: the period before any set reads {}", ns);
        assert!(api::rhu_pwm_enable(h, 1) == sched::ERR_INVAL, "pwm-claim: an enable before any set not refused EINVAL");
        assert!(api::rhu_pwm_set(h, 1_000_000, 250_000, crate::pwm_math::POLARITY_NORMAL) == 0, "pwm-claim: the exact set refused");
        assert!(api::rhu_pwm_period(h, &mut ns) == 0 && ns == 1_000_000, "pwm-claim: the exact period reads {}", ns);
        let own = board::pwm_backend::probe_period_ns(0);
        assert!(own == ns, "pwm-claim: the door reports {} ns but the timer runs {} ns (the prescaler arithmetic disagrees)", ns, own);
        let (mut t, mut c, mut hz) = (0u32, 0u32, 0u32);
        assert!(api::rhu_pwm_info(h, &mut t, &mut c, &mut hz) == 0 && t == 1 && c == 0 && hz == 1_000_000, "pwm-claim: the info reads timer {} channel {} clock {}", t, c, hz);
        assert!(api::rhu_pwm_set(h, 100_000_000_123, 0, crate::pwm_math::POLARITY_NORMAL) == 0, "pwm-claim: the quantized set refused");
        assert!(api::rhu_pwm_period(h, &mut ns) == 0 && ns == 100_000_306_000, "pwm-claim: the quantized period reads {}", ns);
        let own = board::pwm_backend::probe_period_ns(0);
        assert!(own == ns, "pwm-claim: the door reports {} ns but the timer runs {} ns (the prescaler arithmetic disagrees)", ns, own);
        let h1 = api::rhu_pwm_claim(l1);
        assert!(h1 == 1, "pwm-claim: the second line's claim answered {}", h1);
        assert!(api::rhu_pwm_set(h1, 2_000_000, 1_000_000, crate::pwm_math::POLARITY_NORMAL) == crate::pwm::ERR_BUSY, "pwm-claim: the timer's other channel on a different period not refused EBUSY");
        assert!(pwm_witness().last_holder_line == l0, "pwm-claim: the holder in the witness reads {}", pwm_witness().last_holder_line);
        assert!(api::rhu_pwm_set(h1, 100_000_000_123, 50_000_000_000, crate::pwm_math::POLARITY_NORMAL) == 0, "pwm-claim: the timer's other channel on the same period refused");
        assert!(api::rhu_pwm_set(h, 1_000, 2_000, crate::pwm_math::POLARITY_NORMAL) == sched::ERR_INVAL, "pwm-claim: a duty past the period not refused EINVAL");
        assert!(api::rhu_pwm_set(h, 0, 0, crate::pwm_math::POLARITY_NORMAL) == sched::ERR_INVAL, "pwm-claim: a zero period not refused EINVAL");
        assert!(api::rhu_pwm_set(h, 4_294_967_297_000, 0, crate::pwm_math::POLARITY_NORMAL) == sched::ERR_INVAL, "pwm-claim: a period past the range not refused EINVAL");
        assert!(api::rhu_pwm_set(h, 1_000_000, 0, 2) == sched::ERR_INVAL, "pwm-claim: a bad polarity not refused EINVAL");
        assert!(api::rhu_pwm_set(99, 1_000_000, 0, 0) == sched::ERR_INVAL, "pwm-claim: a set on a bad handle not refused EINVAL");
        assert!(api::rhu_pwm_period(99, &mut ns) == sched::ERR_INVAL, "pwm-claim: a period on a bad handle not refused EINVAL");
        assert!(api::rhu_pwm_period(h, core::ptr::null_mut()) == sched::ERR_INVAL, "pwm-claim: a NULL period pointer not refused EINVAL");
        assert!(api::rhu_pwm_enable(99, 1) == sched::ERR_INVAL, "pwm-claim: an enable on a bad handle not refused EINVAL");
        assert!(api::rhu_pwm_enable(h, 2) == sched::ERR_INVAL, "pwm-claim: a bad enable value not refused EINVAL");
        assert!(api::rhu_pwm_info(99, &mut t, &mut c, &mut hz) == sched::ERR_INVAL, "pwm-claim: an info on a bad handle not refused EINVAL");
        assert!(api::rhu_pwm_release(99) == sched::ERR_INVAL, "pwm-claim: a release of a bad handle not refused EINVAL");
        assert!(api::rhu_pwm_enable(h, 1) == 0 && api::rhu_pwm_enable(h, 0) == 0, "pwm-claim: the output could not be driven and parked");
        assert!(api::rhu_pwm_release(h) == 0, "pwm-claim: the release refused");
        assert!(api::rhu_pwm_release(h) == sched::ERR_INVAL, "pwm-claim: a second release not refused EINVAL");
        assert!(api::rhu_pwm_claim(l0) == 0, "pwm-claim: the claim after the release refused");
        assert!(api::rhu_pwm_release(0) == 0 && api::rhu_pwm_release(h1) == 0, "pwm-claim: the final releases refused");
        let w = pwm_witness();
        assert!(w.lines == 3 && w.timers == 2 && w.claims_taken == 3 && w.releases == 3 && w.sets == 3 && w.enables == 1, "pwm-claim: the witness counts {} lines, {} timers, {} claims, {} releases, {} sets, {} enables", w.lines, w.timers, w.claims_taken, w.releases, w.sets, w.enables);
        assert!(w.refused_busy == 2 && w.refused_inval == 15 && w.last_refused_code == sched::ERR_INVAL, "pwm-claim: the witness counts busy {} inval {}, the last code {}", w.refused_busy, w.refused_inval, w.last_refused_code);
        kprintln!("[api] PASS pwm-claim (3 lines on 2 timers stated and enumerated, a claim taken, a period of 1000000 ns exact and a period of 100000000123 ns quantized to 100000306000 ns with the timer's own period equal to the door's both times, the timer's other channel EBUSY on a different period with the holder named and served on the same, a second claim EBUSY, a line without a timer function, a bad polarity, a duty past the period, a zero period, a period past the range and bad handles EINVAL, the output driven and parked, released and claimed again, the witness's counts as expected)");
    }

    /// The ADC doors against the emulator row's virtual converters: the
    /// facts enumerated, the samples as the model states them, the
    /// references, the claims table shared with the GPIO doors, the refusals.
    #[cfg(rhu_pwm_adc_models)]
    fn check_adc_claim() {
        kprintln!("[api] adc-claim: the count and the channels' facts stated; a channel claimed and read as the model states, the internal reference channel read against its block's reference, the two blocks' references answered; a second claim, a line a program holds, an index past the count, a NULL pointer, bad handles, a released handle and a block the board has none of each refused with its code");
        let n = api::rhu_adc_count();
        assert!(n == crate::adc::CHANNELS as i32 && n == 6, "adc-claim: the count reads {}", n);
        let (mut b, mut c, mut l, mut bits) = (0u32, 0u32, 0u32, 0u32);
        for (i, ch) in board::facts::ADC_CHANNELS.iter().enumerate() {
            assert!(api::rhu_adc_channel(i as u32, &mut b, &mut c, &mut l, &mut bits) == 0, "adc-claim: channel {}'s facts refused", i);
            let blk = &board::facts::ADC_BLOCKS[ch.block as usize];
            let want_line = if ch.line == crate::adc_math::NO_LINE { u32::MAX } else { ch.line as u32 };
            assert!(b == blk.number && c == ch.channel as u32 && l == want_line && bits == blk.bits, "adc-claim: channel {} reads block {} channel {} line {} bits {}", i, b, c, l, bits);
        }
        assert!(api::rhu_adc_channel(6, &mut b, &mut c, &mut l, &mut bits) == sched::ERR_INVAL, "adc-claim: an index past the count not refused EINVAL");
        assert!(api::rhu_adc_channel(0, core::ptr::null_mut(), &mut c, &mut l, &mut bits) == sched::ERR_INVAL, "adc-claim: a NULL pointer not refused EINVAL");
        let h = api::rhu_adc_claim(0);
        assert!(h == 0, "adc-claim: the claim answered {}", h);
        assert!(api::rhu_adc_claim(0) == crate::adc::ERR_BUSY, "adc-claim: a second claim not refused EBUSY");
        let s = api::rhu_adc_read(h);
        assert!(s == 819, "adc-claim: channel 0 read {}, the model states 819", s);
        let r = api::rhu_adc_claim(board::facts::ADC_REFERENCE as u32);
        assert!(r == board::facts::ADC_REFERENCE as i32, "adc-claim: the reference channel's claim answered {}", r);
        let s = api::rhu_adc_read(r);
        assert!(s == 6619, "adc-claim: the reference channel read {}, the model states 6619", s);
        let mut mv = 0u32;
        assert!(api::rhu_adc_reference(1, &mut mv) == 0 && mv == 3300, "adc-claim: block 1's reference reads {}", mv);
        assert!(api::rhu_adc_reference(2, &mut mv) == 0 && mv == 3000, "adc-claim: block 2's reference reads {}", mv);
        assert!(api::rhu_adc_reference(3, &mut mv) == sched::ERR_NODEV, "adc-claim: a block the board has none of not refused ENODEV");
        assert!(api::rhu_adc_reference(1, core::ptr::null_mut()) == sched::ERR_INVAL, "adc-claim: a NULL reference pointer not refused EINVAL");
        let line1 = board::facts::ADC_CHANNELS[1].line as u32;
        assert!(api::rhu_gpio_claim(line1, gpio_math::MODE_INPUT, gpio_math::PULL_NONE, 0) == 0, "adc-claim: the GPIO claim of channel 1's line refused");
        assert!(api::rhu_adc_claim(1) == crate::adc::ERR_BUSY, "adc-claim: a channel whose line a program holds not refused EBUSY");
        assert!(api::rhu_gpio_release(line1) == 0, "adc-claim: the GPIO release refused");
        let h1 = api::rhu_adc_claim(1);
        assert!(h1 == 1 && api::rhu_adc_read(h1) == 1638, "adc-claim: channel 1 after the line's release: handle {}", h1);
        assert!(api::rhu_gpio_claim(line1, gpio_math::MODE_INPUT, gpio_math::PULL_NONE, 0) == crate::adc::ERR_BUSY, "adc-claim: the GPIO claim of a held channel's line not refused EBUSY");
        assert!(api::rhu_adc_claim(6) == sched::ERR_INVAL, "adc-claim: a claim past the count not refused EINVAL");
        assert!(api::rhu_adc_read(99) == sched::ERR_INVAL, "adc-claim: a read on a bad handle not refused EINVAL");
        assert!(api::rhu_adc_release(99) == sched::ERR_INVAL, "adc-claim: a release of a bad handle not refused EINVAL");
        assert!(api::rhu_adc_release(h) == 0, "adc-claim: the release refused");
        assert!(api::rhu_adc_read(h) == sched::ERR_INVAL, "adc-claim: a read on a released handle not refused EINVAL");
        assert!(api::rhu_adc_release(h) == sched::ERR_INVAL, "adc-claim: a second release not refused EINVAL");
        assert!(api::rhu_adc_release(r) == 0 && api::rhu_adc_release(h1) == 0, "adc-claim: the final releases refused");
        let w = adc_witness();
        assert!(w.channels == 6 && w.blocks == 2 && w.claims_taken == 3 && w.releases == 3 && w.conversions == 3 && w.timeouts == 0 && w.resets == 0, "adc-claim: the witness counts {} channels, {} blocks, {} claims, {} releases, {} conversions, {} timeouts, {} resets", w.channels, w.blocks, w.claims_taken, w.releases, w.conversions, w.timeouts, w.resets);
        assert!(w.refused_busy == 2 && w.refused_inval == 9 && w.last_sample == 1638 && w.last_refused_code == sched::ERR_INVAL, "adc-claim: the witness counts busy {} inval {}, the last sample {}, the last code {}", w.refused_busy, w.refused_inval, w.last_sample, w.last_refused_code);
        kprintln!("[api] PASS adc-claim (6 channels on 2 blocks stated with their facts, a claim taken and its conversion 819 of 4095 as the model states, the internal reference 6619 of 16383, the references 3300 and 3000 mV, a second claim and a line a program holds EBUSY, an index past the count, a NULL pointer, bad handles, a released handle's read and release EINVAL, a block the board has none of ENODEV, the witness's counts as expected)");
    }

    /// The loop: the first PWM line's duty cycle read back on the loop
    /// channel as the fixture's network will read it (the doctored converter
    /// answers a stale sample at the second reading).
    #[cfg(rhu_pwm_adc_models)]
    fn check_pwm_adc_loop() {
        kprintln!("[api] pwm-adc-loop: three duty cycles set on the first PWM line and read back on the loop channel within two percent of full scale, the parked output read, the inversed polarity read as the complement");
        let h = api::rhu_pwm_claim(board::facts::PWM_LINES[board::facts::ADC_LOOP_SOURCE].line as u32);
        assert!(h == board::facts::ADC_LOOP_SOURCE as i32, "pwm-adc-loop: the PWM claim answered {}", h);
        let a = api::rhu_adc_claim(board::facts::ADC_LOOP as u32);
        assert!(a == board::facts::ADC_LOOP as i32, "pwm-adc-loop: the loop channel's claim answered {}", a);
        let full = crate::adc_math::full_scale(board::facts::ADC_BLOCKS[board::facts::ADC_CHANNELS[board::facts::ADC_LOOP].block as usize].bits);
        let band = full * 2 / 100;
        assert!(api::rhu_pwm_set(h, 1_000_000, 100_000, crate::pwm_math::POLARITY_NORMAL) == 0 && api::rhu_pwm_enable(h, 1) == 0, "pwm-adc-loop: the first set or the enable refused");
        let mut got = [0i32; 3];
        for (i, percent) in [10u32, 50, 90].iter().enumerate() {
            assert!(api::rhu_pwm_set(h, 1_000_000, *percent as u64 * 10_000, crate::pwm_math::POLARITY_NORMAL) == 0, "pwm-adc-loop: the set at {} percent refused", percent);
            let s = api::rhu_adc_read(a);
            let want = full * percent / 100;
            assert!(s >= 0 && crate::adc_math::within_band(s as u32, want, band), "pwm-adc-loop: the loop read {} for a duty of {} percent, outside the band of {} around {}", s, percent, band, want);
            got[i] = s;
        }
        assert!(api::rhu_pwm_enable(h, 0) == 0, "pwm-adc-loop: the park refused");
        let parked = api::rhu_adc_read(a);
        assert!(parked == 0, "pwm-adc-loop: the parked output read {}", parked);
        assert!(api::rhu_pwm_set(h, 1_000_000, 100_000, crate::pwm_math::POLARITY_INVERSED) == 0 && api::rhu_pwm_enable(h, 1) == 0, "pwm-adc-loop: the inversed set or the enable refused");
        let inv = api::rhu_adc_read(a);
        assert!(inv >= 0 && crate::adc_math::within_band(inv as u32, full - full / 10, band), "pwm-adc-loop: the inversed 10 percent read {}", inv);
        assert!(api::rhu_pwm_release(h) == 0 && api::rhu_adc_release(a) == 0, "pwm-adc-loop: the releases refused");
        kprintln!("[api] PASS pwm-adc-loop (10, 50 and 90 percent read back as {}, {} and {} of {} within a band of {}, the parked output 0, the inversed 10 percent {})", got[0], got[1], got[2], full, band, inv);
    }

    /// The converter's conversion-error path on the virtual model under
    /// `test-adc-flagged`: the channel whose first conversion the model flags
    /// reads as the model states after one discard and retry; the channel the
    /// model flags on every conversion is refused with the input/output code
    /// after the attempts' bound, never a sample, the retries counted; the
    /// first channel reads again after it, so the refused read left nothing
    /// held. The doctored converter answers the flagged conversion's
    /// undefined result as a sample, which this check refuses at its first
    /// reading.
    #[cfg(all(rhu_pwm_adc_models, feature = "test-adc-flagged"))]
    fn check_adc_flagged() {
        let once = crate::board::adc_backend::FLAGGED_ONCE;
        let always = crate::board::adc_backend::FLAGGED_ALWAYS;
        let attempts = crate::adc::CONVERSION_ATTEMPTS;
        kprintln!("[api] adc-flagged: channel {}'s flagged first conversion discarded and retried, channel {} flagged on every conversion refused EIO after {} attempts with no sample, channel {} read again after it", once, always, attempts, once);
        let c = &board::facts::ADC_CHANNELS[once];
        let full = crate::adc_math::full_scale(board::facts::ADC_BLOCKS[c.block as usize].bits);
        let want = ((c.channel as u32 + 1) * full / 5) as i32;
        let w0 = adc_witness();
        let h = api::rhu_adc_claim(once as u32);
        assert!(h == once as i32, "adc-flagged: the claim of channel {} answered {}", once, h);
        let s = api::rhu_adc_read(h);
        assert!(s == want, "adc-flagged: channel {} read {} after its flagged first conversion, the model states {}", once, s, want);
        let w1 = adc_witness();
        assert!(w1.retries == w0.retries + 1 && w1.conversion_errors == w0.conversion_errors && w1.conversions == w0.conversions + 1, "adc-flagged: after the retried read the witness counts {} retries, {} conversion errors and {} conversions (before: {}, {} and {})", w1.retries, w1.conversion_errors, w1.conversions, w0.retries, w0.conversion_errors, w0.conversions);
        let h2 = api::rhu_adc_claim(always as u32);
        assert!(h2 == always as i32, "adc-flagged: the claim of channel {} answered {}", always, h2);
        let r = api::rhu_adc_read(h2);
        assert!(r == crate::adc::ERR_IO, "adc-flagged: channel {} flagged on every conversion answered {} (EIO expected, never a sample)", always, r);
        let w2 = adc_witness();
        assert!(w2.retries == w1.retries + attempts - 1 && w2.conversion_errors == w1.conversion_errors + 1 && w2.conversions == w1.conversions && w2.timeouts == w1.timeouts && w2.resets == w1.resets && w2.last_refused_code == crate::adc::ERR_IO && w2.last_refused_index == always as u32, "adc-flagged: after the refused read the witness counts {} retries, {} conversion errors, {} conversions, {} timeouts, {} resets, the last code {} at index {}", w2.retries, w2.conversion_errors, w2.conversions, w2.timeouts, w2.resets, w2.last_refused_code, w2.last_refused_index);
        let s2 = api::rhu_adc_read(h);
        assert!(s2 == want, "adc-flagged: channel {} read {} after the refused read on channel {}, the model states {}", once, s2, always, want);
        let w3 = adc_witness();
        assert!(w3.retries == w2.retries && w3.conversions == w2.conversions + 1, "adc-flagged: the read after the refusal counted {} retries and {} conversions (before: {} and {})", w3.retries, w3.conversions, w2.retries, w2.conversions);
        assert!(api::rhu_adc_release(h) == 0 && api::rhu_adc_release(h2) == 0, "adc-flagged: the releases refused");
        kprintln!("[api] PASS adc-flagged (channel {} read {} after one flagged conversion discarded and retried, channel {} refused EIO after {} flagged conversions with {} retries counted and no sample, channel {} read {} again after it)", once, s, always, attempts, attempts - 1, once, s2);
    }

    /// The rows whose facts carry no PWM line and no ADC channel: the
    /// honest posture of every door.
    #[cfg(not(rhu_pwm_adc_rows))]
    fn check_pwm_adc_none() {
        kprintln!("[api] pwm-adc-none: no PWM line and no ADC channel on this row -- the counts 0, the enumeration doors EINVAL, every other door ENODEV");
        assert!(api::rhu_pwm_count() == 0 && api::rhu_adc_count() == 0, "pwm-adc-none: the counts read {} and {}", api::rhu_pwm_count(), api::rhu_adc_count());
        let (mut a, mut b, mut c, mut d) = (0u32, 0u32, 0u32, 0u32);
        let mut ns = 0u64;
        assert!(api::rhu_pwm_line(0) == sched::ERR_INVAL && api::rhu_adc_channel(0, &mut a, &mut b, &mut c, &mut d) == sched::ERR_INVAL, "pwm-adc-none: the enumeration doors not refused EINVAL");
        let codes = [
            api::rhu_pwm_claim(0),
            api::rhu_pwm_set(0, 1_000_000, 0, 0),
            api::rhu_pwm_period(0, &mut ns),
            api::rhu_pwm_enable(0, 1),
            api::rhu_pwm_release(0),
            api::rhu_pwm_info(0, &mut a, &mut b, &mut c),
            api::rhu_adc_claim(0),
            api::rhu_adc_read(0),
            api::rhu_adc_reference(1, &mut a),
            api::rhu_adc_release(0),
        ];
        for (i, code) in codes.iter().enumerate() {
            assert!(*code == sched::ERR_NODEV, "pwm-adc-none: door {} answered {}, want ENODEV", i, code);
        }
        kprintln!("[api] PASS pwm-adc-none (the counts 0, the two enumeration doors EINVAL, the ten other doors ENODEV)");
    }

    // Check 36: the thread-id space (runs LAST in the smoke: nothing
    // spawns after it). The counter is sought to the last two positive
    // ids -- the refusal cannot be reached by spawning -- and both
    // spawns land exactly those ids; the spawn past the space is
    // refused RHU_EAGAIN with the counter unmoved, twice; no negative id
    // is ever handed out.
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

    // Check 19: stack bounds. Own and peer locals bracketed by their
    // own windows and only those, windows inside the kstacks pool,
    // null out-pointers refused EINVAL, and a reaped tid refused ESRCH
    // by tid lookup (slot reuse cannot alias).
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
        let (a_lo, a_hi) = module_code::range_a();
        assert!(!registry::code_holds(a_lo, 4), "registry-basic: the module-code range held before its registration");
        let mut d3 = registry::ModuleDesc {
            text_lo: a_lo,
            text_hi: a_hi,
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
        // The clobber: the caller's struct dies; the stored copy must not.
        unsafe {
            core::ptr::write_bytes(addr_of_mut!(d3), 0xFF, 1);
        }
        let stored = registry::probe_module(h3);
        assert!(
            stored.text_lo == a_lo
                && stored.text_hi == a_hi
                && stored.statics_lo[0] == 0x3000
                && stored.statics_hi[0] == 0x3800
                && stored.tls_bytes == 0
                && stored.name == keep_name,
            "registry-basic: the stored desc changed with its caller's struct"
        );
        assert!(
            registry::code_holds(a_lo, 4) && registry::code_holds(a_hi - 4, 4) && !registry::code_holds(a_hi - 2, 4),
            "registry-basic: the module-code range not held whole after its registration"
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

    /// The module-code check's form and buffers: an assembled double-precision
    /// form (a multiply, then the return) copied into two RAM buffers, one
    /// registered as the third module's text range by registry-basic and one
    /// never registered; the form is called with its operands in d0 and d1.
    pub mod module_code {
        #[repr(C, align(8))]
        struct Buf([u16; 8]);
        static mut A: Buf = Buf([0; 8]);
        static mut B: Buf = Buf([0; 8]);

        core::arch::global_asm!(
            ".pushsection .text.rhu_api_form_src, \"ax\", %progbits",
            ".thumb",
            ".fpu fpv5-d16",
            ".global __rhu_api_form_src",
            ".type __rhu_api_form_src, %function",
            ".thumb_func",
            "__rhu_api_form_src:",
            "vmul.f64 d0, d0, d1",
            "bx lr",
            ".global __rhu_api_form_src_end",
            "__rhu_api_form_src_end:",
            ".popsection",
        );
        unsafe extern "C" {
            static __rhu_api_form_src: u8;
            static __rhu_api_form_src_end: u8;
        }

        /// Buffer A's half-open range (registered).
        pub fn range_a() -> (u32, u32) {
            let lo = core::ptr::addr_of!(A) as u32;
            (lo, lo + 16)
        }
        /// Buffer B's half-open range (never registered).
        pub fn range_b() -> (u32, u32) {
            let lo = core::ptr::addr_of!(B) as u32;
            (lo, lo + 16)
        }
        /// The assembled form copied into both buffers byte for byte, then the
        /// barriers so the copy is the instruction stream the calls fetch.
        pub fn plant() {
            let src = core::ptr::addr_of!(__rhu_api_form_src) as usize & !1;
            let end = core::ptr::addr_of!(__rhu_api_form_src_end) as usize & !1;
            let n = end - src;
            assert!(n == 6, "module-code: the assembled form is {} bytes, not 6", n);
            unsafe {
                core::ptr::copy_nonoverlapping(src as *const u8, core::ptr::addr_of_mut!(A) as *mut u8, n);
                core::ptr::copy_nonoverlapping(src as *const u8, core::ptr::addr_of_mut!(B) as *mut u8, n);
                core::arch::asm!("dsb sy", "isb sy", options(nostack));
            }
        }
        /// The form at `addr` called with `a` in d0 and `b` in d1; d0 back. The
        /// form touches d0 and lr alone (the fault's handler restores what it
        /// stacks), so those are the call's clobbers, the crate's form macros' shape.
        pub fn run(addr: usize, a: f64, b: f64) -> f64 {
            let (ab, bb) = (a.to_bits(), b.to_bits());
            let (lo, hi): (u32, u32);
            unsafe {
                core::arch::asm!(
                    ".fpu fpv5-d16",
                    "vmov d0, {alo}, {ahi}",
                    "vmov d1, {blo}, {bhi}",
                    "blx {f}",
                    "vmov {lo}, {hi}, d0",
                    f = in(reg) addr | 1,
                    alo = in(reg) ab as u32, ahi = in(reg) (ab >> 32) as u32,
                    blo = in(reg) bb as u32, bhi = in(reg) (bb >> 32) as u32,
                    lo = out(reg) lo, hi = out(reg) hi,
                    out("lr") _, out("d0") _, out("d1") _,
                    options(nostack),
                );
            }
            f64::from_bits(((hi as u64) << 32) | lo as u64)
        }
        /// The doctored control's terminal half: the form from the buffer no
        /// module registers -- the funnel must refuse it.
        #[cfg(feature = "test-fpemul-doctored")]
        pub fn run_unregistered() -> f64 {
            run(range_b().0 as usize, 1.5, 2.0)
        }
    }

    // The module-code check, run after the emulation's checks (whose trap
    // counts are absolutes since boot): a double-precision form executed from
    // a registered module's range OUTSIDE the primary code range (a module
    // placed elsewhere, the registry's case) -- the single-precision row's
    // fault funnel accepts the faulting pc by the registry and emulates the
    // form, the double-precision row's hardware computes it; the range
    // predicate holds inside the registered range and fails past its end and
    // on the unregistered buffer (registry-basic proved it false before the
    // registration). The lines are the same on both rows; the funnel's counts
    // are asserted, not printed.
    fn check_module_code() {
        kprintln!("[api] module-code: a form executed from a registered module's range outside the primary code range armed");
        unsafe extern "C" {
            static __stext: u32;
        }
        let (a_lo, a_hi) = module_code::range_a();
        let (b_lo, _) = module_code::range_b();
        assert!(registry::code_holds(addr_of!(__stext) as u32, 4), "module-code: the primary range not held by module 0");
        assert!(registry::code_holds(a_lo, 4) && registry::code_holds(a_hi - 4, 4), "module-code: the registered range not held");
        assert!(!registry::code_holds(a_hi - 2, 4), "module-code: a read past the registered range held");
        assert!(!registry::code_holds(b_lo, 4), "module-code: the unregistered range held");
        module_code::plant();
        #[cfg(feature = "fpemul")]
        let (t0, _, r0) = crate::fpemul::counts();
        let got = module_code::run(a_lo as usize, 1.5, 2.0);
        assert!(got == 3.0, "module-code: the form from the registered range computed {} not 3.0", got);
        #[cfg(feature = "fpemul")]
        {
            let (t1, _, r1) = crate::fpemul::counts();
            assert!(t1 == t0 + 1 && r1 == r0, "module-code: the funnel counted {} traps and {} refusals over the form, want 1 and 0", t1 - t0, r1 - r0);
        }
        kprintln!("[api] PASS module-code (the range predicate true inside the registered range and false before it and past its end; the form from that range computed 3.0 -- through the fault funnel on the single-precision row, by the hardware on the double-precision row)");
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
        api::rhu_sleep_ns(2_000_000); // equal prio: it spins while we sleep
        assert!(SUSP_PROG.load(Ordering::Acquire) > 0, "susp-pick: spinner never ran");
        let mut ctx = zctx();
        assert!(api::rhu_thread_suspend(tid, &mut ctx) == 0, "susp-pick: suspend refused");
        let p0 = SUSP_PROG.load(Ordering::Acquire);
        for _ in 0..50 {
            api::rhu_thread_yield(); // every chance to rotate in
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

    // Check 22: the suspension snapshot is the thread's real saved
    // context. The plant is an asm spin (calling into a park would
    // leave callee frames, not the pattern, in the saved sw frame):
    // tick preemption switches the planter out with the pattern live
    // in r4/r5/r8/r9/r10/r11 (the asm!-plantable subset -- r6 and r7 are
    // LLVM-reserved on this target).
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
        api::rhu_sleep_ns(2_000_000); // deep in the spin by now
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
        let _ = ctx.flags; // read and recorded, not pinned
        assert!(api::rhu_thread_resume(tid) == 0, "susp-ctx: resume refused");
        CTX_REL.store(1, Ordering::Release);
        join_tid(tid);
        kprintln!("[api] PASS susp-ctx (planted registers exact in ctx, sp inside the target stack, pc inside kernel text)");
    }

    // Check 23: set-context -- registers and pc through the door,
    // geometry refused. The pad runs by exception return with
    // the planted r0 and exits by the C door (ctx.lr is not edited,
    // so returning is not an option for it).
    static SC_WITNESS: AtomicU32 = AtomicU32::new(0);
    static SC_TID: AtomicI32 = AtomicI32::new(0);

    /// The pad stores its thread id FIRST and the witness word last: the check
    /// waits on the witness, so the id is there when the witness is.
    #[cfg(not(feature = "test-susp-doctored-order"))]
    extern "C" fn setctx_pad(magic: usize) -> ! {
        SC_TID.store(api::rhu_thread_self(), Ordering::Release);
        SC_WITNESS.store(magic as u32, Ordering::Release);
        api::rhu_thread_exit()
    }

    /// The doctored control: the witness first, a sleep, then the id -- the
    /// check must refuse at its off-thread assert.
    #[cfg(feature = "test-susp-doctored-order")]
    extern "C" fn setctx_pad(magic: usize) -> ! {
        SC_WITNESS.store(magic as u32, Ordering::Release);
        api::rhu_sleep_ns(5_000_000);
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

    // Check 24: resume of a more-urgent suspended thread preempts the
    // caller before its next line (the spawn-preemption idiom on the
    // resume path).
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

    // Checks 25-30 share ONE process-global handler (registered once
    // in act-refuse, the first act check) dispatching on a MODE
    // static; MODE changes only between checks, workers joined.
    static ACT_MODE: AtomicU32 = AtomicU32::new(0); // 0 count, 1 redeliver, 2 rearm, 3 fp-costume
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
                // redeliver: first run snapshots + sends to self; the
                // second run snapshots again (equality = the flow
                // never ran between the two handler runs).
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
                // act-lazy-fp: ONE floating-point instruction inside the
                // handler (the hosted runtime's activation shape), then
                // the interrupted frame's type and the ctx address.
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
        // A context is a word array: a pointer that is not 4-aligned is
        // refused as the futex doors refuse a misaligned word, before any
        // access through it (the target stays unsuspended: the door's
        // refusal comes first).
        let mis = (addr_of_mut!(c) as usize + 2) as *mut sched::RhuCtx;
        assert!(
            api::rhu_thread_suspend(tp, mis) == sched::ERR_INVAL,
            "act-refuse: misaligned out_ctx not EINVAL"
        );
        assert!(
            api::rhu_thread_set_context(tp, mis) == sched::ERR_INVAL,
            "act-refuse: misaligned ctx not EINVAL"
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
        kprintln!("[api] PASS act-refuse (pre-registration send, null/second registration, self/idle suspends, idle send, unknown tids, misaligned contexts, unsuspended resume/set-context all refused exact)");
    }

    // Checks 26-28 target: a less-urgent spinner (prio 4) -- starved
    // while kmain holds the CPU, so sends coalesce deterministically;
    // its progress word is the interrupted-flow witness.
    static D_PROG: AtomicU32 = AtomicU32::new(0);
    static D_REL: AtomicU32 = AtomicU32::new(0);

    extern "C" fn act_spinner(_: usize) {
        while D_REL.load(Ordering::Acquire) == 0 {
            D_PROG.fetch_add(1, Ordering::Relaxed);
        }
    }

    // Check 26: delivery -- peer arm (handler on the TARGET's thread
    // and stack at its next scheduling-in) + self arm (the same-slot
    // path: no pend inside send, delivery at the sender's own yield).
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
        api::rhu_sleep_ns(2_000_000); // the starved target runs: delivery first
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

    // Check 27: sends coalesce -- three sends before the starved
    // target next runs deliver exactly one handler run.
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
        api::rhu_sleep_ns(2_000_000); // grace: nothing may deliver again
        assert!(
            ACT_COUNT.load(Ordering::Acquire) == c0 + 1,
            "act-coalesce: more than one delivery"
        );
        wait_until(|| D_PROG.load(Ordering::Acquire) > 0, "flow to continue");
        D_REL.store(1, Ordering::Release);
        join_tid(tid);
        kprintln!("[api] PASS act-coalesce (three sends delivered exactly one handler run)");
    }

    // Check 28: re-delivery -- a send from inside the handler re-enters
    // the handler once more BEFORE the interrupted flow resumes (the
    // SVC #1 re-check; same ctx storage, the cost never nests).
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
        api::rhu_sleep_ns(2_000_000); // grace: no third run
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

    // Check 30: a plain sleeper takes the delivery EARLY (ns-clock
    // witness) and the sleep still returns not-early -- the re-arm
    // (the sleep's recheck loop) is what reconciles the
    // two; the never-early assert runs IN the sleeping thread.
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
        api::rhu_sleep_ns(20_000_000); // it is provably IN the 200 ms sleep now
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

    // Check 38: the activation resume completes the lazy floating-point
    // state the handler armed. The hosted runtime's activation handler
    // executes a floating-point instruction, so the trampoline's resume
    // SVC arrives with lazy stacking pending in the SVC's own extended
    // frame; a target whose interrupted frame is BASIC (a thread that
    // never touched the FPU) would otherwise resume with that save still
    // aimed under its stack pointer, and the next floating-point
    // instruction anywhere -- the context switch's own restore of an
    // extended thread -- would write 68 bytes of register images there.
    // The observable is the switch-entry witness, not a crash: a fresh
    // target (never any floating-point use: basic frames) takes one
    // delivery whose handler runs one floating-point instruction, then
    // yields eight times after its resume -- eight switch entries with
    // its own frame basic -- and the witness must have counted nothing.
    // A resume that leaves the state armed is counted at every one of
    // those entries, the pending save's address equal to the handler's
    // ctx address minus 80 (the trampoline's push, then the SVC frame's
    // floating-point area 32 bytes into its 104).
    static LZ_FLAGS: AtomicU32 = AtomicU32::new(0xFFFF);
    static LZ_CTX: AtomicU32 = AtomicU32::new(0);
    static LZ_SWITCHES: AtomicU32 = AtomicU32::new(0);
    static LZ_DONE: AtomicU32 = AtomicU32::new(0);

    extern "C" fn lazy_target(_: usize) {
        // The send lands before this thread is first picked, so the
        // delivery -- and the handler's floating-point instruction --
        // precede this first instruction of the thread's own code.
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
        api::rhu_sleep_ns(2_000_000); // the starved target runs: delivery first, then its yields
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
        // The TLS template joins the bracket ties: size equals the
        // bracket span (empty in this family), and the init pointer
        // follows the emptiness (NULL iff zero -- the honest pair).
        let tlo = core::hint::black_box(addr_of!(__rhu_tdata_lo) as u32);
        let thi = core::hint::black_box(addr_of!(__rhu_tdata_hi) as u32);
        assert!(
            out.tls_bytes == thi - tlo,
            "mod-roots: tls size is not the tdata bracket span"
        );
        assert!(out.tls_bytes == 0, "mod-roots: tls not empty in a base-family image");
        assert!(out.tls_init.is_null(), "mod-roots: empty tls carries a non-null init image");
        assert!(!out.name.is_null(), "mod-roots: module 0 name wrong");
        // One storage, two doors: find's desc for a text address must
        // byte-match roots(0)'s.
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

    // ---------- the hosting doors (the console pair + entropy; the
    // exit door runs LAST as the machine's own exit -- kthread_main
    // hands the final code through it, and the gate's exit-status
    // assert is the proof it terminated the machine). ----------

    // The console doors. The write door pushes a whole known line
    // through the kernel wire (the gate pins that exact line arriving
    // with CRLF discipline -- the door IS the path under test),
    // returns the count exact, and refuses null, oversize and over-bound
    // (the door's stated maximum) honestly.
    // The read door on this world answers 0 -- the gate serial feeds
    // no input, so no-input honesty is the deterministic observable
    // -- writes NOTHING into the buffer, and never waits (this check
    // returning is the proof).
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
        // The over-bound request carries a buffer that is REAL for its
        // whole length, so the refusal is judged on the count alone and
        // a door that accepted the request would write bytes that exist
        // (a control's transcript stays a clean panic, never undefined).
        static OVER_BOUND: [u8; api::CONSOLE_MAX_WRITE as usize + 1] = [b'x'; api::CONSOLE_MAX_WRITE as usize + 1];
        assert!(
            api::rhu_console_write(OVER_BOUND.as_ptr(), OVER_BOUND.len() as u32) == sched::ERR_INVAL,
            "console-pair: over-bound write not refused EINVAL"
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
        kprintln!("[api] PASS console-pair (door write returned exact and hit the wire, no-input read returned 0 writing nothing, null, oversize and over-bound refused)");
    }

    fn check_console_mirror() {
        kprintln!("[api] console-mirror: a line through the log stream lands in the 16 KB memory console whole");
        let log = core::ptr::addr_of!(board::ring::__rhu_console_log);
        let magic = unsafe { core::ptr::read_volatile(core::ptr::addr_of!((*log).magic)) };
        assert!(magic == board::ring::CONSOLE_LOG_MAGIC, "console-mirror: the header's magic is {:#010x}, not RLOG", magic);
        let size = unsafe { core::ptr::read_volatile(core::ptr::addr_of!((*log).size)) };
        assert!(size == board::ring::CONSOLE_LOG_BYTES as u32, "console-mirror: the header's size is {}, not 16 KB", size);
        let before = unsafe { core::ptr::read_volatile(core::ptr::addr_of!((*log).written)) };
        let line = b"[api] console-mirror: this line is read back from the memory console\n";
        assert!(
            api::rhu_log_write(crate::console::LEVEL_VERBOSE, line.as_ptr(), line.len() as u32) == line.len() as i32,
            "console-mirror: the log door returned the wrong count"
        );
        let after = unsafe { core::ptr::read_volatile(core::ptr::addr_of!((*log).written)) };
        let wire = line.len() as u32 + 1;
        assert!(after.wrapping_sub(before) == wire, "console-mirror: the count moved by {} for a wire length of {}", after.wrapping_sub(before), wire);
        let bytes = unsafe { core::ptr::addr_of!((*log).bytes) as *const u8 };
        for i in 0..wire as usize {
            let want = if i + 1 < line.len() { line[i] } else if i + 1 == line.len() { b'\r' } else { b'\n' };
            let got = unsafe { core::ptr::read_volatile(bytes.add((before as usize + i) % board::ring::CONSOLE_LOG_BYTES)) };
            assert!(got == want, "console-mirror: byte {} of the line reads {:#04x}, not {:#04x}", i, got, want);
        }
        kprintln!("[api] PASS console-mirror (the header's magic and size, the count moved by the line's wire length, the bytes equal)");
    }

    fn check_log_level() {
        kprintln!("[api] log-level: the level door and the log write door in a verbose image");
        assert!(api::rhu_log_level() == 3, "log-level: a gate image's level is not verbose (3)");
        let l3 = b"[api] log-door: this verbose line reached the wire through rhu_log_write\n";
        assert!(api::rhu_log_write(3, l3.as_ptr(), l3.len() as u32) == l3.len() as i32, "log-level: the verbose write returned the wrong count");
        let l1 = b"[api] log-door: this fatal-level line reached the wire through rhu_log_write\n";
        assert!(api::rhu_log_write(1, l1.as_ptr(), l1.len() as u32) == l1.len() as i32, "log-level: the fatal-level write returned the wrong count");
        assert!(api::rhu_log_write(0, l3.as_ptr(), 4) == sched::ERR_INVAL, "log-level: level 0 not refused EINVAL");
        assert!(api::rhu_log_write(4, l3.as_ptr(), 4) == sched::ERR_INVAL, "log-level: level 4 not refused EINVAL");
        assert!(api::rhu_log_write(3, core::ptr::null(), 4) == sched::ERR_INVAL, "log-level: null write not refused EINVAL");
        assert!(api::rhu_log_write(3, l3.as_ptr(), api::CONSOLE_MAX_WRITE + 1) == sched::ERR_INVAL, "log-level: over-bound write not refused EINVAL");
        assert!(api::rhu_log_write(3, l3.as_ptr(), 0) == 0, "log-level: zero write not 0");
        kprintln!("[api] PASS log-level (the level 3 in this image, a verbose and a fatal-level line through the log door with exact counts, levels 0 and 4, a null buffer and an over-bound write refused, a zero write 0)");
    }

    #[cfg(rhu_uart_cmsdk)]
    fn uart_witness() -> crate::uart::UartWitness {
        unsafe { core::ptr::read_volatile(core::ptr::addr_of!(crate::uart::WITNESS)) }
    }

    /// A wait on a port's event word with a deadline, the futex recipe: the
    /// value read first, the wait answering woken, EAGAIN (the word moved) or
    /// ETIMEDOUT.
    #[cfg(rhu_uart_cmsdk)]
    fn uart_wait(word: *const u32, deadline_ns: u64) -> i32 {
        let v = unsafe { core::ptr::read_volatile(word) };
        api::rhu_futex_wait(word, v, deadline_ns)
    }

    #[cfg(rhu_uart_cmsdk)]
    fn check_uart_enumerate() {
        kprintln!("[api] uart-enumerate: the count and every row's facts; the console's block refused at the claim, a block the row has not refused");
        let n = api::rhu_uart_count();
        assert!(n == 2 && n == crate::uart::PORTS as i32, "uart-enumerate: the count reads {}", n);
        for (i, row) in board::facts::UART_ROWS.iter().enumerate() {
            let (mut b, mut tx, mut rx, mut hz) = (9u32, 9u32, 9u32, 9u32);
            assert!(api::rhu_uart_port(i as u32, &mut b, &mut tx, &mut rx, &mut hz) == 0, "uart-enumerate: row {} refused", i);
            assert!(
                b == row.block as u32 && tx == crate::uart::NO_PIN && rx == crate::uart::NO_PIN && hz == board::SYSCLK_HZ,
                "uart-enumerate: row {} reads block {} pins {} {} clock {}", i, b, tx, rx, hz
            );
            kprintln!("[api] uart-enumerate: row {}: UART{} (no pins on this row) at {} Hz", i, b, hz);
        }
        let mut scratch = 0u32;
        let sp = &mut scratch as *mut u32;
        assert!(api::rhu_uart_port(2, sp, sp, sp, sp) == sched::ERR_INVAL, "uart-enumerate: an index past the count not refused EINVAL");
        assert!(api::rhu_uart_port(0, core::ptr::null_mut(), sp, sp, sp) == sched::ERR_INVAL, "uart-enumerate: a null pointer not refused EINVAL");
        assert!(api::rhu_uart_claim(0) == crate::uart::ERR_BUSY, "uart-enumerate: the console's block not refused EBUSY at the claim");
        assert!(api::rhu_uart_claim(7) == sched::ERR_INVAL, "uart-enumerate: a block the row has not, not refused EINVAL");
        let w = uart_witness();
        assert!(w.ports == 2 && w.refused_busy >= 1 && w.last_refused_block == 7 && w.last_refused_code == sched::ERR_INVAL, "uart-enumerate: the witness reads ports {} busy {} last {} code {}", w.ports, w.refused_busy, w.last_refused_block, w.last_refused_code);
        kprintln!("[api] PASS uart-enumerate (2 rows stated and enumerated with their facts, an index past the count and a null pointer EINVAL, the console's block EBUSY at the claim, a block the row has not EINVAL)");
    }

    #[cfg(rhu_uart_cmsdk)]
    fn check_uart_claim_twice() {
        kprintln!("[api] uart-claim-twice: a claim taken, a second claim refused, the event word given and bumped at the release, bad handles refused");
        let h = api::rhu_uart_claim(1);
        assert!(h == 1, "uart-claim-twice: the claim answered {}", h);
        assert!(api::rhu_uart_claim(1) == crate::uart::ERR_BUSY, "uart-claim-twice: a second claim not refused EBUSY");
        let word = api::rhu_uart_event_word(h);
        assert!(!word.is_null(), "uart-claim-twice: the event word is null for the held handle");
        assert!(api::rhu_uart_event_word(5).is_null(), "uart-claim-twice: the event word of a bad handle is not null");
        let (mut b, mut tx, mut rx, mut hz) = (9u32, 9u32, 9u32, 9u32);
        assert!(api::rhu_uart_info(h, &mut b, &mut tx, &mut rx, &mut hz) == 0 && b == 1 && tx == crate::uart::NO_PIN && rx == crate::uart::NO_PIN && hz == board::SYSCLK_HZ, "uart-claim-twice: the info reads block {} pins {} {} clock {}", b, tx, rx, hz);
        assert!(api::rhu_uart_info(5, &mut b, &mut tx, &mut rx, &mut hz) == sched::ERR_INVAL, "uart-claim-twice: an info on a bad handle not refused EINVAL");
        let before = unsafe { core::ptr::read_volatile(word) };
        assert!(api::rhu_uart_release(h) == 0, "uart-claim-twice: the release refused");
        let after = unsafe { core::ptr::read_volatile(word) };
        assert!(after == before.wrapping_add(1), "uart-claim-twice: the event word moved by {} at the release", after.wrapping_sub(before));
        assert!(api::rhu_uart_release(h) == sched::ERR_INVAL, "uart-claim-twice: a second release not refused EINVAL");
        let x = [0u8; 1];
        assert!(api::rhu_uart_write(h, x.as_ptr(), 1) == sched::ERR_INVAL && api::rhu_uart_read(h, core::ptr::null_mut(), 0) == sched::ERR_INVAL, "uart-claim-twice: the doors on the released handle not refused EINVAL");
        assert!(api::rhu_uart_claim(1) == 1 && api::rhu_uart_release(1) == 0, "uart-claim-twice: the claim after the release refused");
        kprintln!("[api] PASS uart-claim-twice (the claim answered the row's handle, a second claim EBUSY, the event word given for the held handle and null for a bad one, the info door's facts, the release bumped the word once, a second release and the doors on the released handle EINVAL, claimed again)");
    }

    #[cfg(rhu_uart_cmsdk)]
    fn check_uart_frame() {
        kprintln!("[api] uart-frame: the line discipline on this row's part -- 8N1 at the rates the divider reaches, every other frame and an unreachable rate refused");
        let h = api::rhu_uart_claim(1);
        assert!(h == 1, "uart-frame: the claim answered {}", h);
        let clock = board::SYSCLK_HZ;
        let want = |baud: u32| clock / ((clock + baud / 2) / baud);
        let mut ach = 0u32;
        for baud in [115_200u32, 9_600, 921_600] {
            assert!(api::rhu_uart_configure(h, baud, 8, crate::uart::PARITY_NONE, 1, &mut ach) == 0, "uart-frame: {} 8N1 refused", baud);
            assert!(ach == want(baud), "uart-frame: {} Bd asked, {} achieved, the divider gives {}", baud, ach, want(baud));
            kprintln!("[api] uart-frame: {} Bd asked, {} Bd achieved at {} Hz", baud, ach, clock);
        }
        assert!(api::rhu_uart_configure(h, 2_000_000, 8, 0, 1, &mut ach) == sched::ERR_INVAL, "uart-frame: a rate past the ceiling not refused EINVAL");
        assert!(api::rhu_uart_configure(h, 1_520_000, 8, 0, 1, &mut ach) == sched::ERR_INVAL, "uart-frame: a rate the divider misses by more than 2 percent not refused EINVAL");
        assert!(api::rhu_uart_configure(h, 0, 8, 0, 1, &mut ach) == sched::ERR_INVAL, "uart-frame: a zero rate not refused EINVAL");
        for (bits, par, stop) in [(7u32, 2u32, 1u32), (8, 0, 2), (8, 1, 1), (5, 0, 1), (9, 0, 1), (8, 5, 1), (8, 0, 3)] {
            assert!(api::rhu_uart_configure(h, 115_200, bits, par, stop, &mut ach) == sched::ERR_INVAL, "uart-frame: the frame {} {} {} not refused EINVAL on this part", bits, par, stop);
        }
        assert!(api::rhu_uart_configure(h, 115_200, 8, 0, 1, core::ptr::null_mut()) == sched::ERR_INVAL, "uart-frame: a null pointer not refused EINVAL");
        assert!(api::rhu_uart_configure(5, 115_200, 8, 0, 1, &mut ach) == sched::ERR_INVAL, "uart-frame: a bad handle not refused EINVAL");
        assert!(api::rhu_uart_break(h, 1) == sched::ERR_INVAL && api::rhu_uart_break(h, 2) == sched::ERR_INVAL, "uart-frame: the break door not refused EINVAL on a part without break control");
        assert!(api::rhu_uart_configure(h, 115_200, 8, 0, 1, &mut ach) == 0, "uart-frame: back to 115200 refused");
        assert!(api::rhu_uart_release(h) == 0, "uart-frame: the release refused");
        kprintln!("[api] PASS uart-frame (8N1 at 115200, 9600 and 921600 with the achieved rates as the divider gives them, a rate past the ceiling, a rate the divider misses by more than 2 percent and a zero rate EINVAL, every frame but 8N1 EINVAL on this part, a null pointer and a bad handle EINVAL, the break door EINVAL on a part without break control)");
    }

    #[cfg(rhu_uart_cmsdk)]
    fn check_uart_loopback() {
        kprintln!("[api] uart-loopback: 1500 bytes through the port and back over the wire's echo, byte-equal, nothing lost");
        let h = api::rhu_uart_claim(1);
        assert!(h == 1, "uart-loopback: the claim answered {}", h);
        let word = api::rhu_uart_event_word(h);
        let w0 = uart_witness();
        // The echo's connection window: one probe byte, waited for up to ten
        // seconds (the harness connects its echo after the emulator starts).
        let probe = [0xA5u8];
        assert!(api::rhu_uart_write(h, probe.as_ptr(), 1) == 1, "uart-loopback: the probe write refused");
        let mut got = [0u8; 4];
        let deadline = api::rhu_now_ns() + 10_000_000_000;
        loop {
            let n = api::rhu_uart_read(h, got.as_mut_ptr(), 4);
            if n == 1 {
                assert!(got[0] == 0xA5, "uart-loopback: the probe byte came back as {:#04x}", got[0]);
                break;
            }
            assert!(n == 0, "uart-loopback: the probe read answered {}", n);
            assert!(api::rhu_now_ns() < deadline, "uart-loopback: the probe byte did not come back within ten seconds (the wire's echo is not connected)");
            let _ = uart_wait(word, deadline);
        }
        let mut pat = [0u8; 1500];
        for (i, b) in pat.iter_mut().enumerate() {
            *b = ((i * 7 + 3) & 0xFF) as u8;
        }
        let mut sent = 0usize;
        let mut writes = 0u32;
        let deadline = api::rhu_now_ns() + 5_000_000_000;
        while sent < pat.len() {
            let chunk = (pat.len() - sent).min(123) as u32;
            let t = api::rhu_uart_write(h, pat[sent..].as_ptr(), chunk);
            assert!(t >= 0 && t as u32 <= chunk, "uart-loopback: the write answered {}", t);
            if t == 0 {
                assert!(api::rhu_now_ns() < deadline, "uart-loopback: no room for five seconds");
                let _ = uart_wait(word, deadline);
            }
            sent += t as usize;
            writes += 1;
        }
        let mut back = [0u8; 1500];
        let mut recv = 0usize;
        while recv < back.len() {
            let v = unsafe { core::ptr::read_volatile(word) };
            let n = api::rhu_uart_read(h, back[recv..].as_mut_ptr(), (back.len() - recv) as u32);
            assert!(n >= 0, "uart-loopback: the read answered {}", n);
            recv += n as usize;
            if recv < back.len() && n == 0 {
                assert!(api::rhu_now_ns() < deadline, "uart-loopback: {} of 1500 bytes back within five seconds", recv);
                let _ = api::rhu_futex_wait(word, v, deadline);
            }
        }
        if let Some(i) = (0..pat.len()).find(|&i| pat[i] != back[i]) {
            panic!("uart-loopback: byte {} differs: {:#04x} sent, {:#04x} back", i, pat[i], back[i]);
        }
        let (mut a, mut p, mut r) = (1u32, 1u32, 0u32);
        let settle = api::rhu_now_ns() + 1_000_000_000;
        loop {
            assert!(api::rhu_uart_status(h, &mut a, &mut p, &mut r) == 0, "uart-loopback: the status refused");
            if p == 0 {
                break;
            }
            assert!(api::rhu_now_ns() < settle, "uart-loopback: {} bytes still pending to transmit a second after the echo", p);
            api::rhu_sleep_ns(1_000_000);
        }
        assert!(a == 0 && r == crate::uart::TX_BYTES as u32, "uart-loopback: the status reads {} to read and {} room after the echo", a, r);
        let w = uart_witness();
        assert!(w.overruns == w0.overruns && w.rx_dropped == w0.rx_dropped && w.framing == w0.framing && w.parity == w0.parity, "uart-loopback: the witness counts overruns {} drops {} framing {} parity {}", w.overruns, w.rx_dropped, w.framing, w.parity);
        assert!(w.bytes_out.wrapping_sub(w0.bytes_out) == 1501 && w.bytes_in.wrapping_sub(w0.bytes_in) == 1501 && w.irqs > w0.irqs, "uart-loopback: the witness counts {} out, {} in, {} interrupts", w.bytes_out.wrapping_sub(w0.bytes_out), w.bytes_in.wrapping_sub(w0.bytes_in), w.irqs.wrapping_sub(w0.irqs));
        kprintln!("[api] uart-loopback: 1500 bytes in {} writes, {} interrupts taken", writes, w.irqs.wrapping_sub(w0.irqs));
        assert!(api::rhu_uart_release(h) == 0, "uart-loopback: the release refused");
        kprintln!("[api] PASS uart-loopback (a probe byte back through the wire's echo, 1500 bytes written in partial writes and read back byte-equal, the status settled to nothing pending, the witness's overrun, framing, parity and drop counts unmoved and its byte counts exact)");
    }

    #[cfg(rhu_uart_cmsdk)]
    fn check_uart_status_and_flush() {
        kprintln!("[api] uart-status-and-flush: the three counts and the flush selectors");
        let h = api::rhu_uart_claim(1);
        assert!(h == 1, "uart-status-and-flush: the claim answered {}", h);
        let word = api::rhu_uart_event_word(h);
        let (mut a, mut p, mut r) = (1u32, 1u32, 1u32);
        assert!(api::rhu_uart_status(h, &mut a, &mut p, &mut r) == 0 && a == 0 && p == 0 && r == crate::uart::TX_BYTES as u32, "uart-status-and-flush: the idle status reads {} {} {}", a, p, r);
        assert!(api::rhu_uart_status(h, core::ptr::null_mut(), &mut p, &mut r) == sched::ERR_INVAL, "uart-status-and-flush: a null pointer not refused EINVAL");
        assert!(api::rhu_uart_status(5, &mut a, &mut p, &mut r) == sched::ERR_INVAL, "uart-status-and-flush: a bad handle not refused EINVAL");
        let eight = *b"12345678";
        assert!(api::rhu_uart_write(h, eight.as_ptr(), 8) == 8, "uart-status-and-flush: the write took fewer than 8");
        let deadline = api::rhu_now_ns() + 3_000_000_000;
        loop {
            assert!(api::rhu_uart_status(h, &mut a, &mut p, &mut r) == 0, "uart-status-and-flush: the status refused");
            if a == 8 {
                break;
            }
            assert!(a < 8, "uart-status-and-flush: {} bytes to read after 8 written", a);
            assert!(api::rhu_now_ns() < deadline, "uart-status-and-flush: {} of 8 bytes back within three seconds", a);
            let _ = uart_wait(word, deadline);
        }
        assert!(p == 0, "uart-status-and-flush: {} bytes still pending to transmit after the echo", p);
        assert!(api::rhu_uart_flush(h, 0) == sched::ERR_INVAL && api::rhu_uart_flush(h, 4) == sched::ERR_INVAL && api::rhu_uart_flush(5, 1) == sched::ERR_INVAL, "uart-status-and-flush: a bad selector or handle not refused EINVAL");
        assert!(api::rhu_uart_flush(h, crate::uart::FLUSH_RX) == 0, "uart-status-and-flush: the receive flush refused");
        assert!(api::rhu_uart_status(h, &mut a, &mut p, &mut r) == 0 && a == 0, "uart-status-and-flush: {} bytes to read after the receive flush", a);
        assert!(api::rhu_uart_write(h, eight.as_ptr(), 8) == 8 && api::rhu_uart_flush(h, crate::uart::FLUSH_BOTH) == 0, "uart-status-and-flush: the write or the flush of both refused");
        // The bytes the transmitter had already taken echo back later: the
        // receive side flushed once more after they can have arrived.
        api::rhu_sleep_ns(300_000_000);
        assert!(api::rhu_uart_flush(h, crate::uart::FLUSH_RX) == 0, "uart-status-and-flush: the second receive flush refused");
        assert!(api::rhu_uart_status(h, &mut a, &mut p, &mut r) == 0 && a == 0 && p == 0 && r == crate::uart::TX_BYTES as u32, "uart-status-and-flush: the status after the flushes reads {} {} {}", a, p, r);
        let mut buf = [0u8; 4];
        assert!(api::rhu_uart_read(h, core::ptr::null_mut(), 4) == sched::ERR_INVAL && api::rhu_uart_write(h, core::ptr::null(), 4) == sched::ERR_INVAL, "uart-status-and-flush: a null buffer not refused EINVAL");
        assert!(api::rhu_uart_read(h, buf.as_mut_ptr(), 0) == 0 && api::rhu_uart_write(h, buf.as_ptr(), 0) == 0, "uart-status-and-flush: a zero read or write not 0");
        assert!(api::rhu_uart_release(h) == 0, "uart-status-and-flush: the release refused");
        kprintln!("[api] PASS uart-status-and-flush (the idle counts, 8 bytes written and the 8 back counted by the status door with nothing pending to transmit, the receive flush emptying the ring, the flush of both, bad selectors, handles and null buffers EINVAL, zero reads and writes 0)");
    }

    #[cfg(rhu_uart_cmsdk)]
    fn check_uart_timeout() {
        kprintln!("[api] uart-timeout: a wait on the event word of a silent port returns at its deadline, never early");
        let h = api::rhu_uart_claim(1);
        assert!(h == 1, "uart-timeout: the claim answered {}", h);
        let word = api::rhu_uart_event_word(h);
        let v = unsafe { core::ptr::read_volatile(word) };
        let t0 = api::rhu_now_ns();
        let r = api::rhu_futex_wait(word, v, t0 + 300_000_000);
        let t1 = api::rhu_now_ns();
        assert!(r == sched::ERR_TIMEDOUT, "uart-timeout: the wait answered {}", r);
        let ms = (t1 - t0) / 1_000_000;
        assert!((300..=400).contains(&ms), "uart-timeout: the wait took {} ms", ms);
        kprintln!("[api] uart-timeout: the wait took {} ms", ms);
        assert!(api::rhu_uart_release(h) == 0, "uart-timeout: the release refused");
        kprintln!("[api] PASS uart-timeout (the event word's wait answered ETIMEDOUT at its 300 ms deadline, never early, within the tick's lateness)");
    }

    #[cfg(not(rhu_uart_cmsdk))]
    fn check_uart_enumerate() {}
    #[cfg(not(rhu_uart_cmsdk))]
    fn check_uart_claim_twice() {}
    #[cfg(not(rhu_uart_cmsdk))]
    fn check_uart_frame() {}
    #[cfg(not(rhu_uart_cmsdk))]
    fn check_uart_loopback() {}
    #[cfg(not(rhu_uart_cmsdk))]
    fn check_uart_status_and_flush() {}
    #[cfg(not(rhu_uart_cmsdk))]
    fn check_uart_timeout() {}

    // Check 34: the software-stream door and the declared-quality query.
    // The query's answer is the row's posture, never a constant: 1 where
    // a hardware source serves and its last check passed, 0 otherwise
    // (0 under the build opt-in too). One PASS text per class.
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
        let hw = api::rhu_entropy_is_hardware();
        let serving = matches!(board::ENTROPY, board::EntropyPosture::Source { .. })
            && entropy::state() == entropy::STATE_SERVING;
        assert!(hw == serving as i32, "entropy: the hardware query {} disagrees with the posture", hw);
        if hw == 1 {
            kprintln!("[api] PASS entropy-honesty (draws filled and distinct, null refused, zero-length trivial, hardware query answers 1 on this class ({}))", board::ENTROPY.name());
        } else {
            kprintln!("[api] PASS entropy-honesty (draws filled and distinct, null refused, zero-length trivial, hardware query answers 0 on this class)");
        }
    }

    // Check 34b: the cryptographic door. Three classes, one PASS text
    // each: the build opt-in (the emulator rows' gate images) serves the
    // software stream; a row with a serving hardware source serves it
    // with its health flags clean and the witness's counts moving as
    // expected; a row with neither refuses RHU_ENODEV and writes nothing.
    fn check_entropy_secure() {
        kprintln!("[api] entropy-secure: the cryptographic door armed (two draws, a null, a zero-length)");
        let mut a = [0u8; 16];
        let mut b = [0u8; 64];
        let (draws0, errors0, refusals0) = entropy::counts();
        let ra = api::rhu_entropy_secure(a.as_mut_ptr(), 16);
        let rb = api::rhu_entropy_secure(b.as_mut_ptr(), 64);
        assert!(
            api::rhu_entropy_secure(core::ptr::null_mut(), 16) == sched::ERR_INVAL,
            "entropy-secure: null draw not refused EINVAL"
        );
        assert!(api::rhu_entropy_secure(a.as_mut_ptr(), 0) == 0, "entropy-secure: zero-length draw not 0");
        assert!(
            api::rhu_entropy_secure(a.as_mut_ptr(), entropy::SECURE_MAX_DRAW + 1) == sched::ERR_INVAL,
            "entropy-secure: an over-bound draw not refused EINVAL"
        );
        let hw = api::rhu_entropy_is_hardware();
        let (draws1, errors1, refusals1) = entropy::counts();
        if cfg!(feature = "entropy-accept-software") {
            assert!(ra == 0 && rb == 0, "entropy-secure: a draw refused under the opt-in ({} {})", ra, rb);
            assert!(a[..] != b[..16], "entropy-secure: two draws returned identical bytes");
            assert!(a.iter().any(|&x| x != 0), "entropy-secure: draw left the buffer all-zero");
            assert!(hw == 0, "entropy-secure: the hardware query answers {} under the opt-in", hw);
            assert!(draws1 == draws0 + 2, "entropy-secure: the witness counted {} draws, expected {}", draws1, draws0 + 2);
            kprintln!("[api] PASS entropy-secure (the software stream served two distinct cryptographic draws under the build opt-in, null refused, zero-length and over-bound refused, the hardware query 0)");
        } else if hw == 1 {
            assert!(ra == 0 && rb == 0, "entropy-secure: the hardware source refused a draw ({} {})", ra, rb);
            assert!(a[..] != b[..16], "entropy-secure: two draws returned identical bytes");
            assert!(a.iter().any(|&x| x != 0), "entropy-secure: draw left the buffer all-zero");
            assert!(draws1 == draws0 + 2, "entropy-secure: the witness counted {} draws, expected {}", draws1, draws0 + 2);
            assert!(errors1 == errors0, "entropy-secure: the witness saw {} health flags during the draws", errors1 - errors0);
            kprintln!("[api] PASS entropy-secure (the hardware source served two distinct cryptographic draws with its health flags clean, null refused, zero-length and over-bound refused, the hardware query 1)");
        } else {
            assert!(ra == sched::ERR_NODEV && rb == sched::ERR_NODEV, "entropy-secure: expected RHU_ENODEV twice, got {} {}", ra, rb);
            assert!(a.iter().all(|&x| x == 0) && b.iter().all(|&x| x == 0), "entropy-secure: a refused draw wrote bytes");
            assert!(refusals1 == refusals0 + 2, "entropy-secure: the witness counted {} refusals, expected {}", refusals1, refusals0 + 2);
            kprintln!("[api] PASS entropy-secure (no hardware source on this row and no opt-in: the cryptographic door refused RHU_ENODEV twice and wrote nothing, the query 0, null refused, zero-length and over-bound refused)");
        }
    }



    /// The edge table: every class and boundary a rounding or a NaN rule can
    /// turn on (zeros of both signs, small integers, thirds, powers, the
    /// extremes, the integer boundaries, infinities, quiet and signalling
    /// NaNs, denormals, the neighbours of one).
    const FP_EDGES: [u64; 32] = [
        0x0000_0000_0000_0000, 0x8000_0000_0000_0000, 0x3FF0_0000_0000_0000, 0xBFF0_0000_0000_0000,
        0x4000_0000_0000_0000, 0x3FE0_0000_0000_0000, 0x4008_0000_0000_0000, 0x3FD5_5555_5555_5555,
        0x4024_0000_0000_0000, 0x4202_A05F_2000_0000, 0x3DDB_7CDF_D9D7_BDBB, 0x7E37_E43C_8800_759C,
        0x01A5_6E1F_C2F8_F359, 0x7FEF_FFFF_FFFF_FFFF, 0x0010_0000_0000_0000, 0x3CB0_0000_0000_0000,
        0x4330_0000_0000_0000, 0x4330_0000_0000_0001, 0x4340_0000_0000_0000, 0x41DF_FFFF_FFC0_0000,
        0x41EF_FFFF_FFE0_0000, 0xC1E0_0000_0020_0000, 0x7FF0_0000_0000_0000, 0xFFF0_0000_0000_0000,
        0x7FF8_0000_0000_0000, 0x7FF8_0000_0000_0001, 0xFFF8_0000_0000_0002, 0x7FF0_0000_0000_0001,
        0x0000_0000_0000_0001, 0x000F_FFFF_FFFF_FFFF, 0x3FF0_0000_0000_0001, 0x3FEF_FFFF_FFFF_FFFF,
    ];
    /// The FPSCR values the arithmetic runs under: the four rounding modes, then default-NaN and flush-to-zero.
    const FP_MODES: [u32; 6] = [0, 1 << 22, 2 << 22, 3 << 22, 1 << 25, 1 << 24];
    /// The flag bits a digest takes from FPSCR: the condition flags and the cumulative exception flags.
    const FP_FLAGS: u32 = 0xF000_0000 | 0x9F;

    /// A 32-bit digest over words (FNV-1a over their little-endian bytes).
    struct Fnv(u32);
    impl Fnv {
        fn new() -> Fnv {
            Fnv(0x811C_9DC5)
        }
        fn word(&mut self, w: u32) {
            for b in w.to_le_bytes() {
                self.0 ^= b as u32;
                self.0 = self.0.wrapping_mul(0x0100_0193);
            }
        }
        fn pair(&mut self, r: u64, f: u32) {
            self.word(r as u32);
            self.word((r >> 32) as u32);
            self.word(f & FP_FLAGS);
        }
    }

    macro_rules! fp_asm_bin {
        ($ins:literal, $a:expr, $b:expr, $fpscr:expr) => {{
            let (lo, hi, f): (u32, u32, u32);
            unsafe {
                core::arch::asm!(
                    ".fpu fpv5-d16",
                    "vmsr fpscr, {fpscr}",
                    "vmov d0, {alo}, {ahi}",
                    "vmov d1, {blo}, {bhi}",
                    concat!($ins, " d2, d0, d1"),
                    "vmov {lo}, {hi}, d2",
                    "vmrs {f}, fpscr",
                    "vmsr fpscr, {zero}",
                    fpscr = in(reg) $fpscr, zero = in(reg) 0u32,
                    alo = in(reg) $a as u32, ahi = in(reg) ($a >> 32) as u32,
                    blo = in(reg) $b as u32, bhi = in(reg) ($b >> 32) as u32,
                    lo = out(reg) lo, hi = out(reg) hi, f = out(reg) f,
                    out("d0") _, out("d1") _, out("d2") _,
                    options(nostack)
                )
            };
            ((lo as u64) | ((hi as u64) << 32), f)
        }};
    }
    macro_rules! fp_asm_tri {
        ($ins:literal, $d:expr, $a:expr, $b:expr, $fpscr:expr) => {{
            let (lo, hi, f): (u32, u32, u32);
            unsafe {
                core::arch::asm!(
                    ".fpu fpv5-d16",
                    "vmsr fpscr, {fpscr}",
                    "vmov d2, {dlo}, {dhi}",
                    "vmov d0, {alo}, {ahi}",
                    "vmov d1, {blo}, {bhi}",
                    concat!($ins, " d2, d0, d1"),
                    "vmov {lo}, {hi}, d2",
                    "vmrs {f}, fpscr",
                    "vmsr fpscr, {zero}",
                    fpscr = in(reg) $fpscr, zero = in(reg) 0u32,
                    dlo = in(reg) $d as u32, dhi = in(reg) ($d >> 32) as u32,
                    alo = in(reg) $a as u32, ahi = in(reg) ($a >> 32) as u32,
                    blo = in(reg) $b as u32, bhi = in(reg) ($b >> 32) as u32,
                    lo = out(reg) lo, hi = out(reg) hi, f = out(reg) f,
                    out("d0") _, out("d1") _, out("d2") _,
                    options(nostack)
                )
            };
            ((lo as u64) | ((hi as u64) << 32), f)
        }};
    }
    macro_rules! fp_asm_un {
        ($ins:literal, $a:expr, $fpscr:expr) => {{
            let (lo, hi, f): (u32, u32, u32);
            unsafe {
                core::arch::asm!(
                    ".fpu fpv5-d16",
                    "vmsr fpscr, {fpscr}",
                    "vmov d0, {alo}, {ahi}",
                    concat!($ins, " d2, d0"),
                    "vmov {lo}, {hi}, d2",
                    "vmrs {f}, fpscr",
                    "vmsr fpscr, {zero}",
                    fpscr = in(reg) $fpscr, zero = in(reg) 0u32,
                    alo = in(reg) $a as u32, ahi = in(reg) ($a >> 32) as u32,
                    lo = out(reg) lo, hi = out(reg) hi, f = out(reg) f,
                    out("d0") _, out("d2") _,
                    options(nostack)
                )
            };
            ((lo as u64) | ((hi as u64) << 32), f)
        }};
    }
    /// A form whose result is one single-precision register (a conversion out of a double).
    macro_rules! fp_asm_to_s {
        ($ins:literal, $a:expr, $fpscr:expr) => {{
            let (r, f): (u32, u32);
            unsafe {
                core::arch::asm!(
                    ".fpu fpv5-d16",
                    "vmsr fpscr, {fpscr}",
                    "vmov d0, {alo}, {ahi}",
                    "vmov s4, {zero}",
                    concat!($ins, " s4, d0"),
                    "vmov {r}, s4",
                    "vmrs {f}, fpscr",
                    "vmsr fpscr, {zero}",
                    fpscr = in(reg) $fpscr, zero = in(reg) 0u32,
                    alo = in(reg) $a as u32, ahi = in(reg) ($a >> 32) as u32,
                    r = out(reg) r, f = out(reg) f,
                    out("d0") _, out("s4") _,
                    options(nostack)
                )
            };
            (r, f)
        }};
    }
    /// A form whose source is one single-precision register (a conversion into a double).
    macro_rules! fp_asm_from_s {
        ($ins:literal, $s:expr, $fpscr:expr) => {{
            let (lo, hi, f): (u32, u32, u32);
            unsafe {
                core::arch::asm!(
                    ".fpu fpv5-d16",
                    "vmsr fpscr, {fpscr}",
                    "vmov s8, {s}",
                    concat!($ins, " d2, s8"),
                    "vmov {lo}, {hi}, d2",
                    "vmrs {f}, fpscr",
                    "vmsr fpscr, {zero}",
                    fpscr = in(reg) $fpscr, zero = in(reg) 0u32, s = in(reg) $s,
                    lo = out(reg) lo, hi = out(reg) hi, f = out(reg) f,
                    out("s8") _, out("d2") _,
                    options(nostack)
                )
            };
            ((lo as u64) | ((hi as u64) << 32), f)
        }};
    }
    /// The in-place fixed-point conversions (the register is source and destination).
    macro_rules! fp_asm_fixed {
        ($ins:literal, $a:expr, $fpscr:expr) => {{
            let (lo, hi, f): (u32, u32, u32);
            unsafe {
                core::arch::asm!(
                    ".fpu fpv5-d16",
                    "vmsr fpscr, {fpscr}",
                    "vmov d2, {alo}, {ahi}",
                    $ins,
                    "vmov {lo}, {hi}, d2",
                    "vmrs {f}, fpscr",
                    "vmsr fpscr, {zero}",
                    fpscr = in(reg) $fpscr, zero = in(reg) 0u32,
                    alo = in(reg) $a as u32, ahi = in(reg) ($a >> 32) as u32,
                    lo = out(reg) lo, hi = out(reg) hi, f = out(reg) f,
                    out("d2") _,
                    options(nostack)
                )
            };
            ((lo as u64) | ((hi as u64) << 32), f)
        }};
    }
    /// A compare: the FPSCR after it (the condition flags and the invalid flag).
    macro_rules! fp_asm_cmp {
        ($ins:literal, $a:expr, $b:expr, $fpscr:expr) => {{
            let f: u32;
            unsafe {
                core::arch::asm!(
                    ".fpu fpv5-d16",
                    "vmsr fpscr, {fpscr}",
                    "vmov d0, {alo}, {ahi}",
                    "vmov d1, {blo}, {bhi}",
                    $ins,
                    "vmrs {f}, fpscr",
                    "vmsr fpscr, {zero}",
                    fpscr = in(reg) $fpscr, zero = in(reg) 0u32,
                    alo = in(reg) $a as u32, ahi = in(reg) ($a >> 32) as u32,
                    blo = in(reg) $b as u32, bhi = in(reg) ($b >> 32) as u32,
                    f = out(reg) f,
                    out("d0") _, out("d1") _,
                    options(nostack)
                )
            };
            f
        }};
    }
    /// A select under the condition the compare of two core values leaves in the APSR.
    macro_rules! fp_asm_sel {
        ($ins:literal, $x:expr, $y:expr, $a:expr, $b:expr) => {{
            let (lo, hi): (u32, u32);
            unsafe {
                core::arch::asm!(
                    ".fpu fpv5-d16",
                    "vmov d0, {alo}, {ahi}",
                    "vmov d1, {blo}, {bhi}",
                    "cmp {x}, {y}",
                    concat!($ins, " d2, d0, d1"),
                    "vmov {lo}, {hi}, d2",
                    x = in(reg) $x, y = in(reg) $y,
                    alo = in(reg) $a as u32, ahi = in(reg) ($a >> 32) as u32,
                    blo = in(reg) $b as u32, bhi = in(reg) ($b >> 32) as u32,
                    lo = out(reg) lo, hi = out(reg) hi,
                    out("d0") _, out("d1") _, out("d2") _,
                    options(nostack)
                )
            };
            (lo as u64) | ((hi as u64) << 32)
        }};
    }
    macro_rules! fp_asm_imm {
        ($imm:literal) => {{
            let (lo, hi): (u32, u32);
            unsafe {
                core::arch::asm!(
                    ".fpu fpv5-d16",
                    concat!("vmov.f64 d2, #", $imm),
                    "vmov {lo}, {hi}, d2",
                    lo = out(reg) lo, hi = out(reg) hi,
                    out("d2") _,
                    options(nomem, nostack)
                )
            };
            (lo as u64) | ((hi as u64) << 32)
        }};
    }

    /// A form inside an IT block: the result register preset, the block's
    /// condition from a compare of two core values (NE: the form runs when
    /// they differ and is a NOP when they are equal).
    /// A form whose destination and operands lie in D8-D15 -- the registers no frame carries,
    /// read and written LIVE by the emulation -- the destination preset before the form so a lost
    /// write shows in the read-back; the check's own asm declares the eight registers it uses.
    macro_rules! fp_asm_live {
        ($ins:literal, $d:literal, $n:literal, $m:literal, $a:expr, $b:expr, $preset:expr) => {{
            let (lo, hi, f): (u32, u32, u32);
            unsafe {
                core::arch::asm!(
                    ".fpu fpv5-d16",
                    "vmsr fpscr, {zero}",
                    concat!("vmov ", $n, ", {alo}, {ahi}"),
                    concat!("vmov ", $m, ", {blo}, {bhi}"),
                    concat!("vmov ", $d, ", {plo}, {phi}"),
                    $ins,
                    concat!("vmov {lo}, {hi}, ", $d),
                    "vmrs {f}, fpscr",
                    "vmsr fpscr, {zero}",
                    zero = in(reg) 0u32,
                    alo = in(reg) $a as u32, ahi = in(reg) ($a >> 32) as u32,
                    blo = in(reg) $b as u32, bhi = in(reg) ($b >> 32) as u32,
                    plo = in(reg) $preset as u32, phi = in(reg) ($preset >> 32) as u32,
                    lo = out(reg) lo, hi = out(reg) hi, f = out(reg) f,
                    out("d8") _, out("d9") _, out("d10") _, out("d11") _,
                    out("d12") _, out("d13") _, out("d14") _, out("d15") _,
                    options(nostack)
                )
            };
            ((lo as u64) | ((hi as u64) << 32), f)
        }};
    }
    /// The twelve live-register forms: every doubleword of D8-D15 a destination once, the
    /// operands beside it, the fused form's accumulator itself in the live range (preset 1.0 so
    /// a lost write differs from the sum), the two conversions with a single-precision live
    /// register as the destination and as the source.
    fn fp_live(i: u32, a: u64, b: u64, preset: u64) -> (u64, u32) {
        match i {
            0 => fp_asm_live!("vadd.f64 d8, d9, d10", "d8", "d9", "d10", a, b, preset),
            1 => fp_asm_live!("vsub.f64 d9, d10, d11", "d9", "d10", "d11", a, b, preset),
            2 => fp_asm_live!("vmul.f64 d10, d11, d12", "d10", "d11", "d12", a, b, preset),
            3 => fp_asm_live!("vadd.f64 d11, d12, d13", "d11", "d12", "d13", a, b, preset),
            4 => fp_asm_live!("vsub.f64 d12, d13, d14", "d12", "d13", "d14", a, b, preset),
            5 => fp_asm_live!("vmul.f64 d13, d14, d15", "d13", "d14", "d15", a, b, preset),
            6 => fp_asm_live!("vadd.f64 d14, d15, d8", "d14", "d15", "d8", a, b, preset),
            7 => fp_asm_live!("vsub.f64 d15, d8, d9", "d15", "d8", "d9", a, b, preset),
            8 => fp_asm_live!("vfma.f64 d8, d9, d10", "d8", "d9", "d10", a, b, 0x3FF0_0000_0000_0000u64),
            9 => fp_asm_live!("vsqrt.f64 d15, d8", "d15", "d8", "d8", a, b, preset),
            10 => fp_asm_live!("vcvt.f32.f64 s16, d9", "d8", "d9", "d9", a, b, preset),
            11 => fp_asm_live!("vcvt.f64.f32 d15, s18", "d15", "d9", "d9", a, b, preset),
            _ => (0, 0),
        }
    }
    macro_rules! fp_asm_cond {
        ($ins:literal, $x:expr, $y:expr, $a:expr, $b:expr, $preset:expr) => {{
            let (lo, hi, f): (u32, u32, u32);
            unsafe {
                core::arch::asm!(
                    ".fpu fpv5-d16",
                    "vmsr fpscr, {zero}",
                    "vmov d0, {alo}, {ahi}",
                    "vmov d1, {blo}, {bhi}",
                    "vmov d2, {plo}, {phi}",
                    "cmp {x}, {y}",
                    "it ne",
                    $ins,
                    "vmov {lo}, {hi}, d2",
                    "vmrs {f}, fpscr",
                    "vmsr fpscr, {zero}",
                    zero = in(reg) 0u32, x = in(reg) $x, y = in(reg) $y,
                    alo = in(reg) $a as u32, ahi = in(reg) ($a >> 32) as u32,
                    blo = in(reg) $b as u32, bhi = in(reg) ($b >> 32) as u32,
                    plo = in(reg) $preset as u32, phi = in(reg) ($preset >> 32) as u32,
                    lo = out(reg) lo, hi = out(reg) hi, f = out(reg) f,
                    out("d0") _, out("d1") _, out("d2") _,
                    options(nostack)
                )
            };
            ((lo as u64) | ((hi as u64) << 32), f)
        }};
    }

    fn fp_bin(op: u32, a: u64, b: u64, fpscr: u32) -> (u64, u32) {
        match op {
            0 => fp_asm_bin!("vadd.f64", a, b, fpscr),
            1 => fp_asm_bin!("vsub.f64", a, b, fpscr),
            2 => fp_asm_bin!("vmul.f64", a, b, fpscr),
            3 => fp_asm_bin!("vdiv.f64", a, b, fpscr),
            4 => fp_asm_bin!("vnmul.f64", a, b, fpscr),
            5 => fp_asm_bin!("vmaxnm.f64", a, b, fpscr),
            _ => fp_asm_bin!("vminnm.f64", a, b, fpscr),
        }
    }
    fn fp_tri(op: u32, d: u64, a: u64, b: u64, fpscr: u32) -> (u64, u32) {
        match op {
            0 => fp_asm_tri!("vmla.f64", d, a, b, fpscr),
            1 => fp_asm_tri!("vmls.f64", d, a, b, fpscr),
            2 => fp_asm_tri!("vnmla.f64", d, a, b, fpscr),
            3 => fp_asm_tri!("vnmls.f64", d, a, b, fpscr),
            4 => fp_asm_tri!("vfma.f64", d, a, b, fpscr),
            5 => fp_asm_tri!("vfms.f64", d, a, b, fpscr),
            6 => fp_asm_tri!("vfnma.f64", d, a, b, fpscr),
            _ => fp_asm_tri!("vfnms.f64", d, a, b, fpscr),
        }
    }
    fn fp_un(op: u32, a: u64, fpscr: u32) -> (u64, u32) {
        match op {
            0 => fp_asm_un!("vsqrt.f64", a, fpscr),
            1 => fp_asm_un!("vneg.f64", a, fpscr),
            2 => fp_asm_un!("vabs.f64", a, fpscr),
            3 => fp_asm_un!("vmov.f64", a, fpscr),
            4 => fp_asm_un!("vrintr.f64", a, fpscr),
            5 => fp_asm_un!("vrintx.f64", a, fpscr),
            6 => fp_asm_un!("vrintz.f64", a, fpscr),
            7 => fp_asm_un!("vrinta.f64", a, fpscr),
            8 => fp_asm_un!("vrintn.f64", a, fpscr),
            9 => fp_asm_un!("vrintp.f64", a, fpscr),
            _ => fp_asm_un!("vrintm.f64", a, fpscr),
        }
    }
    fn fp_cmp(op: u32, a: u64, b: u64, fpscr: u32) -> u32 {
        match op {
            0 => fp_asm_cmp!("vcmp.f64 d0, d1", a, b, fpscr),
            1 => fp_asm_cmp!("vcmpe.f64 d0, d1", a, b, fpscr),
            2 => fp_asm_cmp!("vcmp.f64 d0, #0.0", a, b, fpscr),
            _ => fp_asm_cmp!("vcmpe.f64 d0, #0.0", a, b, fpscr),
        }
    }
    fn fp_to_s(op: u32, a: u64, fpscr: u32) -> (u32, u32) {
        match op {
            0 => fp_asm_to_s!("vcvt.f32.f64", a, fpscr),
            1 => fp_asm_to_s!("vcvt.s32.f64", a, fpscr),
            2 => fp_asm_to_s!("vcvt.u32.f64", a, fpscr),
            3 => fp_asm_to_s!("vcvtr.s32.f64", a, fpscr),
            4 => fp_asm_to_s!("vcvtr.u32.f64", a, fpscr),
            5 => fp_asm_to_s!("vcvta.s32.f64", a, fpscr),
            6 => fp_asm_to_s!("vcvtn.s32.f64", a, fpscr),
            7 => fp_asm_to_s!("vcvtp.s32.f64", a, fpscr),
            8 => fp_asm_to_s!("vcvtm.s32.f64", a, fpscr),
            9 => fp_asm_to_s!("vcvta.u32.f64", a, fpscr),
            10 => fp_asm_to_s!("vcvtb.f16.f64", a, fpscr),
            _ => fp_asm_to_s!("vcvtt.f16.f64", a, fpscr),
        }
    }
    fn fp_from_s(op: u32, s: u32, fpscr: u32) -> (u64, u32) {
        match op {
            0 => fp_asm_from_s!("vcvt.f64.f32", s, fpscr),
            1 => fp_asm_from_s!("vcvt.f64.s32", s, fpscr),
            2 => fp_asm_from_s!("vcvt.f64.u32", s, fpscr),
            3 => fp_asm_from_s!("vcvtb.f64.f16", s, fpscr),
            _ => fp_asm_from_s!("vcvtt.f64.f16", s, fpscr),
        }
    }
    fn fp_fixed(op: u32, a: u64, fpscr: u32) -> (u64, u32) {
        match op {
            0 => fp_asm_fixed!("vcvt.s32.f64 d2, d2, #8", a, fpscr),
            1 => fp_asm_fixed!("vcvt.u32.f64 d2, d2, #8", a, fpscr),
            2 => fp_asm_fixed!("vcvt.s16.f64 d2, d2, #4", a, fpscr),
            3 => fp_asm_fixed!("vcvt.u16.f64 d2, d2, #4", a, fpscr),
            4 => fp_asm_fixed!("vcvt.f64.s32 d2, d2, #8", a, fpscr),
            5 => fp_asm_fixed!("vcvt.f64.u32 d2, d2, #8", a, fpscr),
            6 => fp_asm_fixed!("vcvt.f64.s16 d2, d2, #4", a, fpscr),
            _ => fp_asm_fixed!("vcvt.f64.u16 d2, d2, #4", a, fpscr),
        }
    }
    fn fp_sel(op: u32, x: u32, y: u32, a: u64, b: u64) -> u64 {
        match op {
            0 => fp_asm_sel!("vseleq.f64", x, y, a, b),
            1 => fp_asm_sel!("vselvs.f64", x, y, a, b),
            2 => fp_asm_sel!("vselge.f64", x, y, a, b),
            _ => fp_asm_sel!("vselgt.f64", x, y, a, b),
        }
    }

    /// The double-precision forms executed as INSTRUCTIONS over the edge
    /// table under the six FPSCR settings, one digest per group over the
    /// results and the flags: the hardware computes on the double-precision
    /// rows, the fault funnel's emulation on the single-precision rows, and
    /// the digests must agree (the harness pins the hardware row's).
    fn check_fp_double() {
        kprintln!("[api] fp-double: the double-precision forms armed as instructions (the edge table under six FPSCR settings; the digests over results and flags)");
        let e = &FP_EDGES;
        let mut vectors = 0u32;
        // the two-operand arithmetic and the max/min forms
        let mut h = Fnv::new();
        for &m in &FP_MODES {
            for &a in e {
                for &b in e {
                    for op in 0..7 {
                        let (r, f) = fp_bin(op, a, b, m);
                        h.pair(r, f);
                        vectors += 1;
                    }
                }
            }
        }
        kprintln!("[api] fp-double: binary {} vectors digest {:08x}", 7 * 32 * 32 * 6, h.0);
        // the accumulating forms: the edge table squared over four accumulators, round to nearest
        let mut h = Fnv::new();
        for &d in &[e[0], e[2], e[13], e[24]] {
            for &a in e {
                for &b in e {
                    for op in 0..8 {
                        let (r, f) = fp_tri(op, d, a, b, 0);
                        h.pair(r, f);
                        vectors += 1;
                    }
                }
            }
        }
        kprintln!("[api] fp-double: accumulate {} vectors digest {:08x}", 8 * 32 * 32 * 4, h.0);
        // the one-operand forms under the six settings
        let mut h = Fnv::new();
        for &m in &FP_MODES {
            for &a in e {
                for op in 0..11 {
                    let (r, f) = fp_un(op, a, m);
                    h.pair(r, f);
                    vectors += 1;
                }
            }
        }
        kprintln!("[api] fp-double: unary {} vectors digest {:08x}", 11 * 32 * 6, h.0);
        // the compares
        let mut h = Fnv::new();
        for &a in e {
            for &b in e {
                for op in 0..4 {
                    let f = fp_cmp(op, a, b, 0);
                    h.word(f & FP_FLAGS);
                    vectors += 1;
                }
            }
        }
        kprintln!("[api] fp-double: compare {} vectors digest {:08x}", 4 * 32 * 32, h.0);
        // the conversions out of a double, under the six settings (the alternative half format as the seventh)
        let mut h = Fnv::new();
        for &m in FP_MODES.iter().chain([1u32 << 26].iter()) {
            for &a in e {
                for op in 0..12 {
                    let (r, f) = fp_to_s(op, a, m);
                    h.word(r);
                    h.word(f & FP_FLAGS);
                    vectors += 1;
                }
            }
        }
        kprintln!("[api] fp-double: to-single {} vectors digest {:08x}", 12 * 32 * 7, h.0);
        // the conversions into a double: the edge table's low words as singles, integers and halves
        let mut h = Fnv::new();
        let words: [u32; 16] = [
            0, 0x8000_0000, 0x3F80_0000, 0xBF80_0000, 0x7F80_0000, 0xFF80_0000, 0x7FC0_0000, 0x7F80_0001,
            0x0000_0001, 0x007F_FFFF, 0x7FFF_FFFF, 0xFFFF_FFFF, 0x0000_3C00, 0x0000_7C00, 0x0000_7E01, 0x7BFF_FBFF,
        ];
        for &m in FP_MODES.iter().chain([1u32 << 26].iter()) {
            for &w in &words {
                for op in 0..5 {
                    let (r, f) = fp_from_s(op, w, m);
                    h.pair(r, f);
                    vectors += 1;
                }
            }
        }
        kprintln!("[api] fp-double: from-single {} vectors digest {:08x}", 5 * 16 * 7, h.0);
        // the fixed-point conversions, round to nearest (the to-fixed direction rounds toward zero by rule)
        let mut h = Fnv::new();
        for &a in e {
            for op in 0..8 {
                let (r, f) = fp_fixed(op, a, 0);
                h.pair(r, f);
                vectors += 1;
            }
        }
        kprintln!("[api] fp-double: fixed {} vectors digest {:08x}", 8 * 32, h.0);
        // the selects under four compare outcomes, and the immediates
        let mut h = Fnv::new();
        for &(x, y) in &[(1u32, 1u32), (1, 2), (2, 1), (0x8000_0000u32, 1)] {
            for op in 0..4 {
                let r = fp_sel(op, x, y, e[2], e[3]);
                h.word(r as u32);
                h.word((r >> 32) as u32);
                vectors += 1;
            }
        }
        for r in [fp_asm_imm!(1.0), fp_asm_imm!(-1.0), fp_asm_imm!(2.0), fp_asm_imm!(1.75), fp_asm_imm!(31.0), fp_asm_imm!(0.125), fp_asm_imm!(-0.5), fp_asm_imm!(10.0)] {
            h.word(r as u32);
            h.word((r >> 32) as u32);
            vectors += 1;
        }
        kprintln!("[api] fp-double: select-imm {} vectors digest {:08x}", 16 + 8, h.0);
        // the live registers: destinations and operands in D8-D15, the destination preset to a
        // quiet NaN so a write the handler lost shows in the read-back (the fused form's
        // accumulator preset 1.0); four operand pairs of finite normals over the twelve forms
        let mut h = Fnv::new();
        for &(a, b) in &[(e[2], e[4]), (e[6], e[7]), (e[8], e[5]), (e[13], e[15])] {
            for i in 0..12 {
                let (r, f) = fp_live(i, a, b, e[24]);
                h.pair(r, f);
                vectors += 1;
            }
        }
        kprintln!("[api] fp-double: live {} vectors digest {:08x}", 12 * 4, h.0);
        // a form inside an IT block under a failing and a passing condition: the result register
        // preset (3.0), unchanged under the failure and the sum 1.5 under the pass -- the same on
        // every row; on the emulated row the witness says whether the failed form faulted at all
        let mut h = Fnv::new();
        #[cfg(feature = "fpemul")]
        let (t_before, _, _) = crate::fpemul::counts();
        for &(x, y) in &[(1u32, 1u32), (1, 2)] {
            let (r, f) = fp_asm_cond!("vaddne.f64 d2, d0, d1", x, y, e[2], e[5], e[6]);
            h.pair(r, f);
            vectors += 1;
        }
        kprintln!("[api] fp-double: conditional 2 vectors digest {:08x}", h.0);
        #[cfg(feature = "fpemul")]
        let failed_faulted = {
            let (t_after, _, _) = crate::fpemul::counts();
            let raised = t_after - t_before;
            assert!(raised == 1 || raised == 2, "fp-double: the conditional pair raised {} traps, wanted 1 or 2", raised);
            raised == 2
        };
        #[cfg(feature = "fpemul")]
        {
            let (traps, promotions, refusals) = crate::fpemul::counts();
            let skipped = crate::fpemul::skipped();
            // the failed conditional form is a vector executed; it came through the funnel only if it faulted
            let expected = vectors - if failed_faulted { 0 } else { 1 };
            assert!(refusals == 0, "fp-double: {} forms refused by the emulation", refusals);
            assert!(traps == expected, "fp-double: {} forms through the funnel, {} expected of {} vectors", traps, expected, vectors);
            assert!(skipped == failed_faulted as u32, "fp-double: {} forms skipped on a failed condition, wanted {}", skipped, failed_faulted as u32);
            assert!(promotions == 0, "fp-double: {} frames promoted on a thread that holds a floating-point context", promotions);
            if failed_faulted {
                kprintln!("[api] PASS fp-double ({} vectors: every form emulated through the fault funnel, none refused, no frame promoted; the failed condition faulted and was skipped)", vectors);
            } else {
                kprintln!("[api] PASS fp-double ({} vectors: every form emulated through the fault funnel, none refused, no frame promoted; the failed condition did not fault)", vectors);
            }
        }
        #[cfg(not(feature = "fpemul"))]
        kprintln!("[api] PASS fp-double ({} vectors computed by the hardware: the reference row)", vectors);
    }

    /// The band-interrupt control (the double-precision emulation's fault at
    /// the floor): a band-priority exception on the row's last vector line,
    /// pended by the tick while armed, does single-precision math on the
    /// caller-saved and the callee-saved registers and counts how often it
    /// landed inside an emulated form; the binary double-precision group's
    /// digest is computed unarmed and armed and must not move; the
    /// switch-entry witness stays clean; the emulation's counts move by the
    /// forms alone. WHEN a tick lands is the emulator host's business, not the
    /// guest's: on the plain execution profile the tick is a host-clock timer
    /// delivered by the emulator's own thread, and a host that delays that
    /// thread can let a whole armed group run before one tick reaches the
    /// guest. So the armed group is repeated, back to back, until a hit has
    /// landed where the check needs it -- on the emulated row inside an
    /// emulated form -- or until the kernel's own tick count has advanced by
    /// `FP_BAND_TICK_BOUND` since the first repetition began: a bound in the
    /// guest's ticks, never in the host's time. A tick that advanced the count
    /// ran the tick handler, which pended the line, which ran before this
    /// thread resumed; so a bound reached with no hit inside the repeated
    /// group is a broken band and the refusal is right, while a slow host only
    /// makes the loop run longer. The counts are asserted, never printed, so
    /// the transcript stays constant per class; the armed digest is printed,
    /// a constant the harness pins; the landing is printed as a class, never
    /// as a count of repetitions or hits.
    fn check_fp_band() {
        kprintln!("[api] fp-band: a band-priority exception firing every tick through the binary double-precision group, doing single-precision math on the caller-saved and the callee-saved registers; the digest, the switch-entry witness and the emulation's counts armed");
        let (w0, _, _) = sched::probe_lazy_fp_witness();
        assert!(w0 == 0, "fp-band: {} abandoned lazy floating-point states before the check", w0);
        #[cfg(feature = "fpemul")]
        let (t0, p0, r0) = crate::fpemul::counts();
        let unarmed = fp_band_group();
        #[cfg(feature = "fpemul")]
        let (t1, _, _) = crate::fpemul::counts();
        FP_BAND.hits.store(0, Ordering::Release);
        FP_BAND.in_form.store(0, Ordering::Release);
        FP_BAND.lspact.store(0, Ordering::Release);
        #[cfg(feature = "test-fp-band-doctored")]
        {
            FP_BAND_SILENT.store(1, Ordering::Release);
        }
        let first_tick = sched::now_ms();
        fp_band_arm(true);
        let armed = loop {
            #[cfg(feature = "fpemul")]
            let (ta, pa, ra) = crate::fpemul::counts();
            let digest = fp_band_group();
            assert!(digest == unarmed, "fp-band: the binary group's digest moved under preemption ({:08x} armed, {:08x} unarmed)", digest, unarmed);
            #[cfg(feature = "fpemul")]
            {
                let (tb, pb, rb) = crate::fpemul::counts();
                assert!(tb - ta == t1 - t0, "fp-band: the armed group raised {} traps, the unarmed {}", tb - ta, t1 - t0);
                assert!(pb == pa && rb == ra, "fp-band: a promotion or a refusal during the groups ({} promoted, {} refused)", pb - pa, rb - ra);
            }
            if fp_band_landed() || sched::now_ms().wrapping_sub(first_tick) >= FP_BAND_TICK_BOUND {
                break digest;
            }
        };
        fp_band_arm(false);
        let hits = FP_BAND.hits.load(Ordering::Acquire);
        let in_form = FP_BAND.in_form.load(Ordering::Acquire);
        let lspact = FP_BAND.lspact.load(Ordering::Acquire);
        assert!(hits >= 1, "fp-band: the band exception never fired");
        // the lazy save pending at the body's first read is a report, not an assertion: this handler's
        // own prologue saves d8 before the body runs, and that push is the first floating-point
        // instruction of the context -- it completes the pending save itself, so the word reads 0
        // on a handler that saves a callee-saved register, which is the mechanism under test
        let _ = lspact;
        #[cfg(feature = "fpemul")]
        {
            let (_, p2, r2) = crate::fpemul::counts();
            assert!(p2 == p0 && r2 == r0, "fp-band: a promotion or a refusal during the groups ({} promoted, {} refused)", p2 - p0, r2 - r0);
            assert!(in_form >= 1, "fp-band: no hit landed inside an emulated form ({} hits)", hits);
        }
        #[cfg(not(feature = "fpemul"))]
        assert!(in_form == 0, "fp-band: {} hits found the fault active on a hardware row", in_form);
        let (w1, fpcar, psp) = sched::probe_lazy_fp_witness();
        assert!(w1 == 0, "fp-band: {} abandoned lazy floating-point states after the armed group (fpcar {:#010x}, psp {:#010x})", w1, fpcar, psp);
        kprintln!("[api] fp-band: binary {} vectors digest {:08x} under preemption", 7 * 32 * 32 * 6, armed);
        kprintln!("[api] fp-band: a tick landed while the group ran armed (the group repeated until one did, the wait bounded by the kernel's own tick count)");
        // The doctored flavor's silence ends with the check: were its refusal
        // ever missed, the image would not walk on with the tick hook silent.
        #[cfg(feature = "test-fp-band-doctored")]
        FP_BAND_SILENT.store(0, Ordering::Release);
        #[cfg(feature = "fpemul")]
        kprintln!("[api] PASS fp-band (the band exception fired every tick and preempted emulated forms; the digest held under preemption; the switch-entry witness clean; the emulation's counts moved by the forms alone)");
        #[cfg(not(feature = "fpemul"))]
        kprintln!("[api] PASS fp-band (the band exception fired every tick through the hardware block; the digest held under preemption; the switch-entry witness clean)");
    }

    /// The bound on the armed group's repetitions, in the kernel's own ticks
    /// since the first repetition began. Eight: a tick that advanced the
    /// count pended the line, and the line's handler ran before this thread
    /// resumed, so eight ticks with no hit where the check needs one is a
    /// broken band, however slow the host; one tick is not enough only when
    /// it lands in the few instructions between two repetitions.
    const FP_BAND_TICK_BOUND: u32 = 8;

    /// Whether a hit has landed where the check needs it: on the emulated row
    /// inside an emulated form (the mechanism under test there); on the
    /// hardware row anywhere in the armed span.
    fn fp_band_landed() -> bool {
        if cfg!(feature = "fpemul") {
            FP_BAND.in_form.load(Ordering::Acquire) >= 1
        } else {
            FP_BAND.hits.load(Ordering::Acquire) >= 1
        }
    }

    /// The doctored control's silence (the feature test-fp-band-doctored,
    /// built into no board image): set by the fp-band check before it arms, read
    /// by the tick hook, which then pends nothing -- the check must refuse at
    /// its own assert once the bound's ticks were observed. Scoped to the
    /// check, so the band's other consumer keeps its pend.
    #[cfg(feature = "test-fp-band-doctored")]
    static FP_BAND_SILENT: AtomicU32 = AtomicU32::new(0);

    /// The binary group of the double-precision block, silent: the digest alone.
    fn fp_band_group() -> u32 {
        let e = &FP_EDGES;
        let mut h = Fnv::new();
        for &m in &FP_MODES {
            for &a in e {
                for &b in e {
                    for op in 0..7 {
                        let (r, f) = fp_bin(op, a, b, m);
                        h.pair(r, f);
                    }
                }
            }
        }
        h.0
    }

    /// The control's record, debug-facing and exported by symbol (four words):
    /// armed; the hits; the hits taken while the UsageFault was active (inside
    /// an emulated form); the hits whose body still found a lazy floating-point
    /// save pending (0 when the handler's own prologue completed it first).
    #[repr(C)]
    pub struct FpBandWitness {
        pub armed: AtomicU32,
        pub hits: AtomicU32,
        pub in_form: AtomicU32,
        pub lspact: AtomicU32,
    }

    /// The rezero band's control: the control line's hits that landed while a
    /// rezero's memset was in flight (the kernel band served inside it).
    static REZERO_BAND_HITS: AtomicU32 = AtomicU32::new(0);

    #[unsafe(export_name = "__rhu_fp_band_witness")]
    pub static FP_BAND: FpBandWitness = FpBandWitness {
        armed: AtomicU32::new(0),
        hits: AtomicU32::new(0),
        in_form: AtomicU32::new(0),
        lspact: AtomicU32::new(0),
    };

    /// The control's line: the row's spare vector line -- one the part
    /// implements and no handler of the kernel's claims (the table refuses
    /// a double claim at compile time); the arming reads the line's priority
    /// byte back, so a line the part does not implement refuses loudly
    /// instead of never firing.
    pub const FP_BAND_LINE: usize = crate::board::SPARE_IRQ;
    const NVIC_ISER: usize = 0xE000_E100;
    const NVIC_ICER: usize = 0xE000_E180;
    const NVIC_ISPR: usize = 0xE000_E200;
    const NVIC_ICPR: usize = 0xE000_E280;
    const NVIC_IPR: usize = 0xE000_E400;
    const SCB_SHCSR: *const u32 = 0xE000_ED24 as *const u32;
    const SHCSR_USGFAULTACT: u32 = 1 << 3;
    const FPCCR: *const u32 = 0xE000_EF34 as *const u32;
    const FPCCR_LSPACT: u32 = 1 << 0;
    /// The priority the control's line takes: inside the zero-latency band.
    const FP_BAND_PRIO: u8 = 0x20;
    const FP_BAND_BIT: u32 = 1 << ((FP_BAND_LINE % 32) as u32);

    fn fp_band_reg(base: usize) -> *mut u32 {
        (base + 4 * (FP_BAND_LINE / 32)) as *mut u32
    }

    /// The line armed: its priority in the band, read back (a priority byte
    /// that does not take the write is a line the part does not implement:
    /// the arming refuses, naming the line), any stale pend cleared, the
    /// line enabled, the record's flag set; disarmed: the flag cleared, the
    /// line disabled, the pend cleared, and the pend bit asserted clear after.
    fn fp_band_arm(on: bool) {
        unsafe {
            if on {
                write_volatile((NVIC_IPR + FP_BAND_LINE) as *mut u8, FP_BAND_PRIO);
                let got = read_volatile((NVIC_IPR + FP_BAND_LINE) as *const u8);
                assert!(got == FP_BAND_PRIO, "fp-band: the control line {} is not implemented on this row (its priority byte does not take a write)", FP_BAND_LINE);
                write_volatile(fp_band_reg(NVIC_ICPR), FP_BAND_BIT);
                write_volatile(fp_band_reg(NVIC_ISER), FP_BAND_BIT);
                FP_BAND.armed.store(1, Ordering::Release);
            } else {
                FP_BAND.armed.store(0, Ordering::Release);
                write_volatile(fp_band_reg(NVIC_ICER), FP_BAND_BIT);
                write_volatile(fp_band_reg(NVIC_ICPR), FP_BAND_BIT);
                assert!(read_volatile(fp_band_reg(NVIC_ISPR)) & FP_BAND_BIT == 0, "fp-band: the line still pending after the disarm");
            }
        }
    }

    /// The tick's hook: one pend per tick while armed (the api flavor only);
    /// under the doctored feature, none while the fp-band check has asked
    /// for silence.
    pub fn fp_band_tick() {
        #[cfg(feature = "test-fp-band-doctored")]
        {
            if FP_BAND_SILENT.load(Ordering::Relaxed) != 0 {
                return;
            }
        }
        if FP_BAND.armed.load(Ordering::Relaxed) != 0 {
            unsafe { write_volatile(fp_band_reg(NVIC_ISPR), FP_BAND_BIT) };
        }
    }

    /// The control's handler: the record first (the fault active means the
    /// hit landed inside an emulated form; the lazy-save word is what the body
    /// still sees after the prologue's own save of d8 ran, the interrupted
    /// context's floating-point state stacked by that first instruction), then single-precision
    /// math through the caller-saved s0-s2 and the callee-saved s16-s17 (d8):
    /// the registers declared, so this handler's own prologue and epilogue
    /// carry d8 as the procedure-call standard requires of every handler.
    pub unsafe extern "C" fn __rhu_fp_band_irq() {
        unsafe {
            let shcsr = read_volatile(SCB_SHCSR);
            let fpccr = read_volatile(FPCCR);
            FP_BAND.hits.fetch_add(1, Ordering::AcqRel);
            if arena::rezero_in_flight() != 0 {
                REZERO_BAND_HITS.fetch_add(1, Ordering::AcqRel);
            }
            if shcsr & SHCSR_USGFAULTACT != 0 {
                FP_BAND.in_form.fetch_add(1, Ordering::AcqRel);
            }
            if fpccr & FPCCR_LSPACT != 0 {
                FP_BAND.lspact.fetch_add(1, Ordering::AcqRel);
            }
            core::arch::asm!(
                "vmov s1, {a}",
                "vmov s2, {b}",
                "vdiv.f32 s0, s1, s2",
                "vmov s16, {a}",
                "vmov s17, {b}",
                "vmul.f32 s16, s16, s17",
                a = in(reg) 0x3F80_0000u32,
                b = in(reg) 0x4040_0000u32,
                out("s0") _, out("s1") _, out("s2") _, out("s16") _, out("s17") _,
                options(nomem, nostack),
            );
        }
    }

    /// The basic-frame promotion: a thread whose FIRST floating-point
    /// instruction is a double-precision immediate move (no context yet) must
    /// come away with the value and a context, and the switch-entry witness
    /// must stay clean. On the double-precision rows the hardware creates
    /// the context itself; the words are the same.
    #[cfg(feature = "fpemul")]
    fn check_fp_frames() {
        kprintln!("[api] fp-frames: a fresh thread's first floating-point instruction is a double-precision immediate; the promotion and the switch-entry witness armed");
        let (w0, _, _) = sched::probe_lazy_fp_witness();
        assert!(w0 == 0, "fp-frames: {} abandoned lazy floating-point states before the check", w0);
        let (_, p0, r0) = crate::fpemul::counts();
        FP_FRAME_LO.store(0, Ordering::Release);
        FP_FRAME_HI.store(0, Ordering::Release);
        FP_FRAME_DONE.store(0, Ordering::Release);
        let (_slot, tid) = sched::try_spawn("dpimm", fp_frame_target, 0, 4, 4096).ok().expect("fp-frames: spawn");
        wait_until(|| FP_FRAME_DONE.load(Ordering::Acquire) == 1, "the fresh thread's double-precision immediate");
        join_tid(tid);
        let (lo, hi) = (FP_FRAME_LO.load(Ordering::Acquire), FP_FRAME_HI.load(Ordering::Acquire));
        assert!(lo == 0 && hi == 0x3FF8_0000, "fp-frames: the immediate 1.5 read back as {:#010x}:{:#010x}", hi, lo);
        let (_, p1, r1) = crate::fpemul::counts();
        assert!(p1 == p0 + 1, "fp-frames: {} frames promoted, wanted 1", p1 - p0);
        assert!(r1 == r0, "fp-frames: {} refusals", r1 - r0);
        let (count, fpcar, psp) = sched::probe_lazy_fp_witness();
        assert!(count == 0, "fp-frames: {} abandoned lazy floating-point states after the promotion (fpcar {:#010x}, psp {:#010x})", count, fpcar, psp);
        kprintln!("[api] PASS fp-frames (the basic frame promoted once; the value 1.5 read back through the new context; the switch-entry witness clean)");
    }

    #[cfg(feature = "fpemul")]
    static FP_FRAME_LO: AtomicU32 = AtomicU32::new(0);
    #[cfg(feature = "fpemul")]
    static FP_FRAME_HI: AtomicU32 = AtomicU32::new(0);
    #[cfg(feature = "fpemul")]
    static FP_FRAME_DONE: AtomicU32 = AtomicU32::new(0);

    /// The fresh thread: its first floating-point instruction is the
    /// double-precision immediate, then the value goes out through the core
    /// registers, then it yields a few times so the witness can count.
    #[cfg(feature = "fpemul")]
    extern "C" fn fp_frame_target(_: usize) {
        let (lo, hi): (u32, u32);
        unsafe {
            core::arch::asm!(
                ".fpu fpv5-d16",
                "vmov.f64 d0, #1.5",
                "vmov {lo}, {hi}, d0",
                lo = out(reg) lo, hi = out(reg) hi,
                out("d0") _,
                options(nomem, nostack)
            )
        };
        FP_FRAME_LO.store(lo, Ordering::Release);
        FP_FRAME_HI.store(hi, Ordering::Release);
        for _ in 0..4 {
            api::rhu_thread_yield();
        }
        FP_FRAME_DONE.store(1, Ordering::Release);
    }

    // ---- the cost of one double-precision form on a row with a cycle counter (the feature fp-cost) ----

    /// The cycle counter's word read before and after ONE instruction, the two
    /// reads fenced so nothing else sits between them. On the single-precision
    /// class the instruction is the trap and everything the emulation does
    /// before the program resumes; on the double-precision class the hardware's
    /// own cost; the same bracket around a nop is the bracket's own cost. The
    /// first operand's low word also lands in s8 for the forms with a
    /// single-precision source; d2 starts as the second operand for the
    /// accumulating and in-place forms.
    #[cfg(feature = "fp-cost")]
    macro_rules! fp_cost_of {
        ($ins:literal, $a:expr, $b:expr) => {{
            let (c0, c1): (u32, u32);
            unsafe {
                core::arch::asm!(
                    ".fpu fpv5-d16",
                    "vmov d0, {alo}, {ahi}",
                    "vmov d1, {blo}, {bhi}",
                    "vmov d2, {blo}, {bhi}",
                    "vmov s8, {alo}",
                    "dsb",
                    "isb",
                    "ldr {c0}, [{dwt}]",
                    $ins,
                    "isb",
                    "ldr {c1}, [{dwt}]",
                    dwt = in(reg) 0xE000_1004u32,
                    alo = in(reg) $a as u32, ahi = in(reg) ($a >> 32) as u32,
                    blo = in(reg) $b as u32, bhi = in(reg) ($b >> 32) as u32,
                    c0 = out(reg) c0, c1 = out(reg) c1,
                    out("d0") _, out("d1") _, out("d2") _, out("s8") _,
                    options(nostack)
                )
            };
            c1.wrapping_sub(c0)
        }};
    }

    /// The cost per form, sixteen runs each, the smallest and the middle count
    /// printed: a measurement record, never a golden (the
    /// counts move with the flash's wait states and whatever interrupt lands
    /// inside a run; the smallest of sixteen is the form's own cost). A row
    /// without a cycle counter says so and measures nothing.
    #[cfg(feature = "fp-cost")]
    fn check_fp_cost() {
        const A: u64 = 0x3FF8_0000_0000_0000; // 1.5
        const B: u64 = 0x4002_0000_0000_0000; // 2.25
        const A_F32: u64 = 0x3FC0_0000; // 1.5 as a single, in the low word
        const A_F16: u64 = 0x3E00; // 1.5 as a half, in the low word
        const A_INT: u64 = 12345; // an integer in the low word
        if !board::CYCLE_COUNTER {
            kprintln!("[api] fp-cost: not counted (no cycle counter on this row)");
            return;
        }
        let (t0, t1) = unsafe { (read_volatile(0xE000_1004 as *const u32), read_volatile(0xE000_1004 as *const u32)) };
        if t0 == t1 {
            kprintln!("[api] fp-cost: not counted (the cycle counter did not advance)");
            return;
        }
        kprintln!("[api] fp-cost: the cycle counter around one instruction, sixteen runs per form; the smallest and the middle count");
        let forms: [(&str, fn() -> u32); 23] = [
            ("nop (the bracket itself)", || fp_cost_of!("nop", A, B)),
            ("vadd.f64", || fp_cost_of!("vadd.f64 d2, d0, d1", A, B)),
            ("vsub.f64", || fp_cost_of!("vsub.f64 d2, d0, d1", A, B)),
            ("vmul.f64", || fp_cost_of!("vmul.f64 d2, d0, d1", A, B)),
            ("vdiv.f64", || fp_cost_of!("vdiv.f64 d2, d0, d1", A, B)),
            ("vsqrt.f64", || fp_cost_of!("vsqrt.f64 d2, d0", B, B)),
            ("vneg.f64", || fp_cost_of!("vneg.f64 d2, d0", A, B)),
            ("vabs.f64", || fp_cost_of!("vabs.f64 d2, d0", A, B)),
            ("vmov.f64 (register)", || fp_cost_of!("vmov.f64 d2, d0", A, B)),
            ("vmov.f64 #imm", || fp_cost_of!("vmov.f64 d2, #1.5", A, B)),
            ("vcmp.f64", || fp_cost_of!("vcmp.f64 d0, d1", A, B)),
            ("vcmpe.f64", || fp_cost_of!("vcmpe.f64 d0, d1", A, B)),
            ("vfma.f64", || fp_cost_of!("vfma.f64 d2, d0, d1", A, B)),
            ("vmla.f64", || fp_cost_of!("vmla.f64 d2, d0, d1", A, B)),
            ("vmaxnm.f64", || fp_cost_of!("vmaxnm.f64 d2, d0, d1", A, B)),
            ("vrintn.f64", || fp_cost_of!("vrintn.f64 d2, d0", A, B)),
            ("vcvt.f64.s32", || fp_cost_of!("vcvt.f64.s32 d2, s8", A_INT, B)),
            ("vcvt.f64.u32", || fp_cost_of!("vcvt.f64.u32 d2, s8", A_INT, B)),
            ("vcvt.s32.f64", || fp_cost_of!("vcvt.s32.f64 s8, d0", A, B)),
            ("vcvt.f64.f32", || fp_cost_of!("vcvt.f64.f32 d2, s8", A_F32, B)),
            ("vcvt.f32.f64", || fp_cost_of!("vcvt.f32.f64 s8, d0", A, B)),
            ("vcvtb.f64.f16", || fp_cost_of!("vcvtb.f64.f16 d2, s8", A_F16, B)),
            ("vcvt.s32.f64 #8 (fixed, in place)", || fp_cost_of!("vcvt.s32.f64 d2, d2, #8", A, B)),
        ];
        for (name, run) in forms.iter() {
            let mut counts = [0u32; 16];
            for c in counts.iter_mut() {
                *c = run();
            }
            // an insertion sort: the smallest first, the middle at index eight
            for i in 1..counts.len() {
                let mut j = i;
                while j > 0 && counts[j - 1] > counts[j] {
                    counts.swap(j - 1, j);
                    j -= 1;
                }
            }
            kprintln!("[api] fp-cost: {} min {} mid {} cycles", name, counts[0], counts[8]);
        }
        kprintln!("[api] PASS fp-cost ({} forms measured on the cycle counter; the nop line is the bracket's own cost)", forms.len() - 1);
    }

    fn check_null_read() {
        kprintln!("[api] OBSERVED null-read: the record of address 0 on this row follows (the observation of record: red by design, not a check)");
        #[cfg(not(feature = "null-guard-off"))]
        kprintln!(
            "[obs] address 0 on this row: no-access -- the first page is denied by the protection unit, so the read is not attempted (the part's own map at 0: {}); a hosted null dereference traps into the funnel here",
            if board::ADDRESS_ZERO_READABLE { "readable memory" } else { "a bus fault" }
        );
        #[cfg(feature = "null-guard-off")]
        if board::ADDRESS_ZERO_READABLE {
            let zero = core::hint::black_box(0usize) as *const u32;
            let word = unsafe { core::ptr::read_volatile(zero) };
            core::hint::black_box(word);
            kprintln!("[obs] address 0 on this row: readable -- the guard is off (null-guard-off, a control image) and the read returned without a fault; a hosted null dereference does not trap here");
        } else {
            kprintln!("[obs] address 0 on this row: a bus fault -- the board states it (measured on the silicon) and the read is not attempted; a hosted null dereference traps into the funnel here");
        }
    }

    /// The PWM doors against the RP2350's slices, every number from the row's
    /// own tables: the count and the lines enumerated against the table; a
    /// line claimed, the line that shares its slice output and the board's
    /// line refused; a period set and tied to the arithmetic AND to the
    /// slice's own registers; the slice's other output refused on a
    /// different period and served on the same; the refusals; the drive and
    /// the park; the releases.
    #[cfg(all(rhu_pwm_adc_rows, not(rhu_pwm_adc_models)))]
    fn check_pwm_claim() {
        kprintln!("[api] pwm-claim: the count and the lines stated against the table; a line claimed, its twin pad and the board's line refused, a 20 ms period set and tied to the slice's registers, the sibling output refused on a different period and served on the same, the refusals, the drive and the park, the releases");
        let lines = board::facts::PWM_LINES;
        let n = api::rhu_pwm_count();
        assert!(n == lines.len() as i32, "pwm-claim: the count reads {}, the table has {}", n, lines.len());
        for (i, l) in lines.iter().enumerate() {
            assert!(api::rhu_pwm_line(i as u32) == l.line as i32, "pwm-claim: line {} reads {}", i, api::rhu_pwm_line(i as u32));
        }
        assert!(api::rhu_pwm_line(n as u32) == sched::ERR_INVAL, "pwm-claim: an index past the count not refused EINVAL");
        let l0 = board::facts::PWM_CHECK_LINE;
        let ix = lines.iter().position(|l| l.line as u32 == l0).unwrap_or(usize::MAX);
        assert!(ix != usize::MAX, "pwm-claim: the check line {} is not in the table", l0);
        let h = api::rhu_pwm_claim(l0);
        assert!(h == ix as i32, "pwm-claim: the claim of line {} answered {}, its index is {}", l0, h, ix);
        assert!(api::rhu_pwm_claim(l0) == crate::pwm::ERR_BUSY, "pwm-claim: a second claim on the line not refused EBUSY");
        assert!(api::rhu_pwm_claim(board::facts::PWM_CHECK_TWIN) == crate::pwm::ERR_BUSY, "pwm-claim: the line sharing the slice output not refused EBUSY");
        if !board::facts::BOARD_CLAIMS.is_empty() {
            assert!(api::rhu_pwm_claim(board::facts::CHECK_BOARD) == crate::pwm::ERR_BUSY, "pwm-claim: the board's line not refused EBUSY");
        } else {
            kprintln!("[api] pwm-claim: no line is claimed by the board on this row by default (the memory console takes no UART pin); the board-line assertion skipped, three busy refusals owed over the walk, not four");
        }
        assert!(api::rhu_pwm_claim(board::facts::LINE_COUNT as u32) == sched::ERR_INVAL, "pwm-claim: a line past the package not refused EINVAL");
        let mut ns = 0u64;
        assert!(api::rhu_pwm_period(h, &mut ns) == 0 && ns == 0, "pwm-claim: the period before any set reads {}", ns);
        assert!(api::rhu_pwm_enable(h, 1) == sched::ERR_INVAL, "pwm-claim: an enable before any set not refused EINVAL");
        let tix = lines[ix].timer as usize;
        let t = &board::facts::PWM_TIMERS[tix];
        let want = match crate::pwm_math::quantize(20_000_000, t) {
            Some((p, a)) => crate::pwm_math::achieved_ns(p, a, t.clock_hz),
            None => 0,
        };
        assert!(want != 0, "pwm-claim: 20 ms does not quantize on a clock of {} Hz", t.clock_hz);
        assert!(api::rhu_pwm_set(h, 20_000_000, 5_000_000, crate::pwm_math::POLARITY_NORMAL) == 0, "pwm-claim: the 20 ms set refused");
        assert!(api::rhu_pwm_period(h, &mut ns) == 0 && ns == want, "pwm-claim: the period reads {} ns, the arithmetic says {}", ns, want);
        let own = board::pwm_backend::probe_period_ns(tix);
        assert!(own == ns, "pwm-claim: the door reports {} ns but the slice runs {} ns (the registers disagree)", ns, own);
        let (mut tn, mut c, mut hz) = (0u32, 0u32, 0u32);
        assert!(api::rhu_pwm_info(h, &mut tn, &mut c, &mut hz) == 0 && tn == t.number && c == lines[ix].channel as u32 && hz == t.clock_hz, "pwm-claim: the info reads slice {} output {} clock {}", tn, c, hz);
        assert!(api::rhu_pwm_set(h, 2_000_000_000, 0, crate::pwm_math::POLARITY_NORMAL) == sched::ERR_INVAL, "pwm-claim: a period past the range not refused EINVAL");
        let l1 = board::facts::PWM_CHECK_SIBLING;
        let ix1 = lines.iter().position(|l| l.line as u32 == l1).unwrap_or(usize::MAX);
        assert!(ix1 != usize::MAX && lines[ix1].timer as usize == tix, "pwm-claim: the sibling line {} is not on slice {}", l1, t.number);
        let h1 = api::rhu_pwm_claim(l1);
        assert!(h1 == ix1 as i32, "pwm-claim: the sibling's claim answered {}", h1);
        assert!(api::rhu_pwm_set(h1, 10_000_000, 5_000_000, crate::pwm_math::POLARITY_NORMAL) == crate::pwm::ERR_BUSY, "pwm-claim: the slice's other output on a different period not refused EBUSY");
        assert!(pwm_witness().last_holder_line == l0, "pwm-claim: the holder in the witness reads {}", pwm_witness().last_holder_line);
        assert!(api::rhu_pwm_set(h1, 20_000_000, 10_000_000, crate::pwm_math::POLARITY_NORMAL) == 0, "pwm-claim: the slice's other output on the same period refused");
        assert!(api::rhu_pwm_period(h1, &mut ns) == 0 && ns == want, "pwm-claim: the sibling's period reads {}", ns);
        assert!(api::rhu_pwm_set(h, 1_000, 2_000, crate::pwm_math::POLARITY_NORMAL) == sched::ERR_INVAL, "pwm-claim: a duty past the period not refused EINVAL");
        assert!(api::rhu_pwm_set(h, 0, 0, crate::pwm_math::POLARITY_NORMAL) == sched::ERR_INVAL, "pwm-claim: a zero period not refused EINVAL");
        assert!(api::rhu_pwm_set(h, 20_000_000, 0, 2) == sched::ERR_INVAL, "pwm-claim: a bad polarity not refused EINVAL");
        assert!(api::rhu_pwm_set(99, 20_000_000, 0, 0) == sched::ERR_INVAL, "pwm-claim: a set on a bad handle not refused EINVAL");
        assert!(api::rhu_pwm_period(99, &mut ns) == sched::ERR_INVAL, "pwm-claim: a period on a bad handle not refused EINVAL");
        assert!(api::rhu_pwm_period(h, core::ptr::null_mut()) == sched::ERR_INVAL, "pwm-claim: a NULL period pointer not refused EINVAL");
        assert!(api::rhu_pwm_enable(99, 1) == sched::ERR_INVAL, "pwm-claim: an enable on a bad handle not refused EINVAL");
        assert!(api::rhu_pwm_enable(h, 2) == sched::ERR_INVAL, "pwm-claim: a bad enable value not refused EINVAL");
        assert!(api::rhu_pwm_info(99, &mut tn, &mut c, &mut hz) == sched::ERR_INVAL, "pwm-claim: an info on a bad handle not refused EINVAL");
        assert!(api::rhu_pwm_release(99) == sched::ERR_INVAL, "pwm-claim: a release of a bad handle not refused EINVAL");
        assert!(api::rhu_pwm_enable(h, 1) == 0 && api::rhu_pwm_enable(h, 0) == 0, "pwm-claim: the output could not be driven and parked");
        assert!(api::rhu_pwm_release(h) == 0, "pwm-claim: the release refused");
        assert!(api::rhu_pwm_release(h) == sched::ERR_INVAL, "pwm-claim: a second release not refused EINVAL");
        assert!(api::rhu_pwm_claim(l0) == ix as i32, "pwm-claim: the claim after the release refused");
        assert!(api::rhu_pwm_release(ix as i32) == 0 && api::rhu_pwm_release(h1) == 0, "pwm-claim: the final releases refused");
        let w = pwm_witness();
        assert!(w.lines == lines.len() as u32 && w.timers == crate::pwm::TIMERS as u32 && w.claims_taken == 3 && w.releases == 3 && w.sets == 2 && w.enables == 1, "pwm-claim: the witness counts {} lines, {} slices, {} claims, {} releases, {} sets, {} enables", w.lines, w.timers, w.claims_taken, w.releases, w.sets, w.enables);
        let busy_want: u32 = if board::facts::BOARD_CLAIMS.is_empty() { 3 } else { 4 };
        assert!(w.refused_busy == busy_want && w.refused_inval == 15 && w.last_refused_code == sched::ERR_INVAL, "pwm-claim: the witness counts busy {} (want {}) inval {}, the last code {}", w.refused_busy, busy_want, w.refused_inval, w.last_refused_code);
        kprintln!("[api] PASS pwm-claim ({} lines on {} slices stated and enumerated, line {} claimed and the line sharing its slice output refused, a period of 20000000 ns run as {} ns and tied to the slice's registers, output {} of the slice refused on a different period and served on the same, {} busy and 15 invalid refusals counted, driven and parked, released)", lines.len(), crate::pwm::TIMERS, l0, want, l1, busy_want);
    }

    /// The ADC doors against the RP2350's converter: the count and the
    /// channels' facts against the table; the temperature sensor's channel
    /// claimed and read inside the die's band; a pin channel claimed and read
    /// (a header line with nothing on it: the sample printed, not judged);
    /// the reference door; a channel whose line a program holds refused; the
    /// refusals; the releases.
    #[cfg(all(rhu_pwm_adc_rows, not(rhu_pwm_adc_models)))]
    fn check_adc_claim() {
        kprintln!("[api] adc-claim: the count and the channels' facts stated against the table; the temperature sensor's channel claimed and read inside the die's band, a pin channel claimed and read, the reference door, a channel whose line a program holds refused, the refusals, the releases");
        let chans = board::facts::ADC_CHANNELS;
        let blocks = board::facts::ADC_BLOCKS;
        let n = api::rhu_adc_count();
        assert!(n == chans.len() as i32, "adc-claim: the count reads {}, the table has {}", n, chans.len());
        let (mut b, mut c, mut l, mut bits) = (0u32, 0u32, 0u32, 0u32);
        for (i, ch) in chans.iter().enumerate() {
            assert!(api::rhu_adc_channel(i as u32, &mut b, &mut c, &mut l, &mut bits) == 0, "adc-claim: channel {}'s facts refused", i);
            let blk = &blocks[ch.block as usize];
            let want_line = if ch.line == crate::adc_math::NO_LINE { u32::MAX } else { ch.line as u32 };
            assert!(b == blk.number && c == ch.channel as u32 && l == want_line && bits == blk.bits, "adc-claim: channel {} reads block {} channel {} line {} bits {}", i, b, c, l, bits);
        }
        assert!(api::rhu_adc_channel(n as u32, &mut b, &mut c, &mut l, &mut bits) == sched::ERR_INVAL, "adc-claim: an index past the count not refused EINVAL");
        assert!(api::rhu_adc_channel(0, core::ptr::null_mut(), &mut c, &mut l, &mut bits) == sched::ERR_INVAL, "adc-claim: a NULL pointer not refused EINVAL");
        let s_ix = board::facts::ADC_SENSOR;
        let hs = api::rhu_adc_claim(s_ix as u32);
        assert!(hs == s_ix as i32, "adc-claim: the sensor channel's claim answered {}", hs);
        assert!(api::rhu_adc_claim(s_ix as u32) == crate::adc::ERR_BUSY, "adc-claim: a second claim not refused EBUSY");
        let s = api::rhu_adc_read(hs);
        assert!(s >= 0, "adc-claim: the sensor's conversion answered {}", s);
        let full = crate::adc_math::full_scale(blocks[chans[s_ix].block as usize].bits) as i64;
        let mv100 = s as i64 * blocks[chans[s_ix].block as usize].reference_mv as i64 * 100 / full;
        let t10 = 270 - (mv100 - 70600) * 100 / 1721;
        assert!((760..=1000).contains(&s), "adc-claim: the sensor read {} counts ({} tenths of a degree), outside the die's band", s, t10);
        let p_ix = board::facts::ADC_CHECK;
        let hp = api::rhu_adc_claim(p_ix as u32);
        assert!(hp == p_ix as i32, "adc-claim: the pin channel's claim answered {}", hp);
        let v = api::rhu_adc_read(hp);
        assert!(v >= 0 && v <= full as i32, "adc-claim: the pin channel read {}", v);
        let mut mv = 0u32;
        assert!(api::rhu_adc_reference(blocks[0].number, &mut mv) == 0 && mv == blocks[0].reference_mv, "adc-claim: the block's reference reads {}", mv);
        assert!(api::rhu_adc_reference(blocks[0].number + 1, &mut mv) == sched::ERR_NODEV, "adc-claim: a block the board has none of not refused ENODEV");
        assert!(api::rhu_adc_reference(blocks[0].number, core::ptr::null_mut()) == sched::ERR_INVAL, "adc-claim: a NULL reference pointer not refused EINVAL");
        assert!(api::rhu_adc_release(hp) == 0, "adc-claim: the pin channel's release refused");
        let line = chans[p_ix].line as u32;
        assert!(api::rhu_gpio_claim(line, gpio_math::MODE_INPUT, gpio_math::PULL_NONE, 0) == 0, "adc-claim: the GPIO claim of the pin channel's line refused");
        assert!(api::rhu_adc_claim(p_ix as u32) == crate::adc::ERR_BUSY, "adc-claim: a channel whose line a program holds not refused EBUSY");
        assert!(api::rhu_gpio_release(line) == 0, "adc-claim: the GPIO release refused");
        let hp2 = api::rhu_adc_claim(p_ix as u32);
        assert!(hp2 == p_ix as i32, "adc-claim: the pin channel after the line's release: handle {}", hp2);
        assert!(api::rhu_adc_claim(n as u32) == sched::ERR_INVAL, "adc-claim: a claim past the count not refused EINVAL");
        assert!(api::rhu_adc_read(99) == sched::ERR_INVAL, "adc-claim: a read on a bad handle not refused EINVAL");
        assert!(api::rhu_adc_release(99) == sched::ERR_INVAL, "adc-claim: a release of a bad handle not refused EINVAL");
        assert!(api::rhu_adc_release(hs) == 0, "adc-claim: the sensor channel's release refused");
        assert!(api::rhu_adc_read(hs) == sched::ERR_INVAL, "adc-claim: a read on a released handle not refused EINVAL");
        assert!(api::rhu_adc_release(hs) == sched::ERR_INVAL, "adc-claim: a second release not refused EINVAL");
        assert!(api::rhu_adc_release(hp2) == 0, "adc-claim: the final release refused");
        let w = adc_witness();
        assert!(w.channels == n as u32 && w.blocks == blocks.len() as u32 && w.claims_taken == 3 && w.releases == 3 && w.conversions == 2 && w.timeouts == 0 && w.resets == 0, "adc-claim: the witness counts {} channels, {} blocks, {} claims, {} releases, {} conversions, {} timeouts, {} resets", w.channels, w.blocks, w.claims_taken, w.releases, w.conversions, w.timeouts, w.resets);
        assert!(w.refused_busy == 2 && w.refused_inval == 9 && w.last_sample == v as u32 && w.last_refused_code == sched::ERR_INVAL, "adc-claim: the witness counts busy {} inval {}, the last sample {}, the last code {}", w.refused_busy, w.refused_inval, w.last_sample, w.last_refused_code);
        kprintln!("[api] PASS adc-claim ({} channels on {} block stated with their facts, the temperature sensor read {} of {} = {} tenths of a degree C, pin channel {} on line {} read {}, the reference {} mV, a channel whose line a program held refused, 2 busy and 9 invalid refusals counted, released)", n, blocks.len(), s, full, t10, p_ix, line, v, mv);
    }

    /// The loop between a PWM line and a channel through the fixture's
    /// resistor-capacitor network: three duty cycles read back within two
    /// percent of full scale, the parked output read 0, the inversed duty
    /// read as its complement -- run where the row's facts state the pair;
    /// where they state none, said so, and no verdict claimed.
    #[cfg(all(rhu_pwm_adc_rows, not(rhu_pwm_adc_models)))]
    fn check_pwm_adc_loop() {
        let Some((src, chan)) = board::facts::ADC_LOOP else {
            kprintln!("[api] pwm-adc-loop: no loop fixture stated for this row; the check is not run (the fixture's two passive parts and its pair of lines become the row's facts once the loop is wired on the board)");
            return;
        };
        kprintln!("[api] pwm-adc-loop: three duty cycles set on PWM line {} and read on channel {} within two percent of full scale, the parked output read 0, the inversed duty read as its complement", board::facts::PWM_LINES[src].line, chan);
        let h = api::rhu_pwm_claim(board::facts::PWM_LINES[src].line as u32);
        assert!(h == src as i32, "pwm-adc-loop: the PWM claim answered {}", h);
        let a = api::rhu_adc_claim(chan as u32);
        assert!(a == chan as i32, "pwm-adc-loop: the channel's claim answered {}", a);
        let full = crate::adc_math::full_scale(board::facts::ADC_BLOCKS[board::facts::ADC_CHANNELS[chan].block as usize].bits);
        let band = full / 50;
        assert!(api::rhu_pwm_set(h, 1_000_000, 100_000, crate::pwm_math::POLARITY_NORMAL) == 0 && api::rhu_pwm_enable(h, 1) == 0, "pwm-adc-loop: the first set or the enable refused");
        let mut got = [0i32; 3];
        for (k, percent) in [10u32, 50, 90].iter().enumerate() {
            assert!(api::rhu_pwm_set(h, 1_000_000, *percent as u64 * 10_000, crate::pwm_math::POLARITY_NORMAL) == 0, "pwm-adc-loop: the set at {} percent refused", percent);
            sched::sleep_ms(20);
            let s = api::rhu_adc_read(a);
            let want = full * percent / 100;
            assert!(s >= 0 && crate::adc_math::within_band(s as u32, want, band), "pwm-adc-loop: the loop read {} for a duty of {} percent, outside the band of {} around {}", s, percent, band, want);
            got[k] = s;
        }
        assert!(api::rhu_pwm_enable(h, 0) == 0, "pwm-adc-loop: the park refused");
        sched::sleep_ms(20);
        let parked = api::rhu_adc_read(a);
        assert!(parked >= 0 && crate::adc_math::within_band(parked as u32, 0, band), "pwm-adc-loop: the parked output read {}", parked);
        assert!(api::rhu_pwm_set(h, 1_000_000, 100_000, crate::pwm_math::POLARITY_INVERSED) == 0 && api::rhu_pwm_enable(h, 1) == 0, "pwm-adc-loop: the inversed set or the enable refused");
        sched::sleep_ms(20);
        let inv = api::rhu_adc_read(a);
        assert!(inv >= 0 && crate::adc_math::within_band(inv as u32, full - full / 10, band), "pwm-adc-loop: the inversed 10 percent read {}", inv);
        assert!(api::rhu_pwm_release(h) == 0 && api::rhu_adc_release(a) == 0, "pwm-adc-loop: the releases refused");
        kprintln!("[api] PASS pwm-adc-loop (10, 50 and 90 percent read back as {}, {} and {} of {} within a band of {}, the parked output {}, the inversed 10 percent {})", got[0], got[1], got[2], full, band, parked, inv);
    }

    // The bus checks' modules, on the rows that have a bus alone: an unused
    // import is a warning on every other row.
    #[cfg(rhu_bus_rows)]
    use crate::{bus, bus_math};
}

/// The stack-overflow designed death (the v8-M rows' stack limit; the
/// v7-M rows' guard block).
#[cfg(feature = "test-stackovfl")]
mod stackovfl;

/// The null-page guard's designed death and its deliveries.
#[cfg(feature = "test-nullpage")]
mod nullpage;

/// The designed death inside an interrupt handler.
#[cfg(feature = "test-fault-in-handler")]
mod fault_in_handler;
#[cfg(all(feature = "test-fault-in-handler", feature = "api-smoke"))]
compile_error!("the fault-in-handler flavor and the api flavor share the row's spare interrupt line -- name one of them");

/// The power-management chip's registers read over the family I2C driver
/// (the power chip's register reading; a flavor of the Portenta rows).
#[cfg(feature = "test-h7-pmic-read")]
mod pmic_read;
#[cfg(all(feature = "test-uart-loopback", feature = "test-uart-jumper"))]
compile_error!("the UART flight runs one wire per image -- name one of test-uart-loopback and test-uart-jumper");
#[cfg(all(any(feature = "test-uart-loopback", feature = "test-uart-jumper"), not(feature = "api-smoke")))]
compile_error!("the UART flights are legs of the api-smoke flavor -- name the flavor beside the leg");
#[cfg(all(
    not(feature = "test-nullpage"),
    any(feature = "test-nullpage-deliver", feature = "test-nullpage-cs", feature = "test-nullpage-fetch", feature = "test-nullpage-header")
))]
compile_error!("the nullpage legs are features of the test-nullpage flavor -- name the flavor beside the leg");
#[cfg(any(
    all(feature = "test-nullpage-deliver", feature = "test-nullpage-cs"),
    all(feature = "test-nullpage-deliver", feature = "test-nullpage-fetch"),
    all(feature = "test-nullpage-deliver", feature = "test-nullpage-header"),
    all(feature = "test-nullpage-cs", feature = "test-nullpage-fetch"),
    all(feature = "test-nullpage-cs", feature = "test-nullpage-header"),
    all(feature = "test-nullpage-fetch", feature = "test-nullpage-header")
))]
compile_error!("the nullpage flavor runs one leg per image -- name one of deliver, cs, fetch and header");

/// The RP2350's bus arithmetic and bus rows: no hardware access, covered by the host tests.
#[cfg(rhu_bus_rp2350)]
mod rp2350_bus_math;

// The network stack and its arithmetic (net_math.rs), behind the `net`
// feature alone; declared at the end of this file so that, with the feature
// off, no line of this file moves and no panic location's line number with
// it.
#[cfg(feature = "net")]
#[allow(dead_code)]
mod net_math;

// The network doors' bodies (every row: the count 0 and RHU_ENODEV
// where no medium is), the scripted peer behind the emulator rows' virtual
// medium (the rows with a medium alone).
mod net;
#[cfg(rhu_net_rows)]
mod net_peer;
