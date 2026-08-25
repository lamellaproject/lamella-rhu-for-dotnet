//! The scheduler: fixed-priority preemptive with round-robin inside a
//! priority, PendSV context switch (lazy-FP honest), SysTick timebase,
//! BASEPRI-only masking throughout.

use core::arch::global_asm;
use core::mem::offset_of;
use core::ptr::{self, addr_of, addr_of_mut, read_volatile, write_volatile};
use core::sync::atomic::{AtomicU32, Ordering};

use crate::board;
use crate::critical::with_cs;

/// TCB slots. Sized for the hosted runtime's thread census plus kernel
/// threads; the slot masks are u32, so this must stay below 32.
pub const MAX_THREADS: usize = 16;
const _: () = assert!(MAX_THREADS <= 31, "slot masks are u32");

const ICSR: *mut u32 = 0xE000_ED04 as *mut u32;
const ICSR_PENDSVSET: u32 = 1 << 28;
const ICSR_PENDSTSET: u32 = 1 << 26;
const SHPR2: *mut u32 = 0xE000_ED1C as *mut u32;
const SHPR3: *mut u32 = 0xE000_ED20 as *mut u32;
const SYST_CSR: *mut u32 = 0xE000_E010 as *mut u32;
const SYST_RVR: *mut u32 = 0xE000_E014 as *mut u32;
const SYST_CVR: *mut u32 = 0xE000_E018 as *mut u32;

/// Nanosecond clock geometry. The tick divisions must be exact; the
/// sub-tick interpolation is the exact reduced fraction
/// `NS_NUM/NS_DEN = 1e9/SYSCLK_HZ`, so a SYSCLK that does not divide
/// 1e9 (silicon PLL rates) still yields an honest clock: at every tick
/// boundary `CYCLES_PER_TICK * NS_NUM / NS_DEN == NS_PER_TICK`
/// identically (the boundary-identity assert below is the tripwire),
/// and within a tick the floor of a monotone product stays monotone.
const CYCLES_PER_TICK: u32 = board::SYSCLK_HZ / board::TICK_HZ;
const NS_PER_TICK: u64 = 1_000_000_000 / board::TICK_HZ as u64;
const NS_GCD: u64 = gcd(1_000_000_000, board::SYSCLK_HZ as u64);
const NS_NUM: u64 = 1_000_000_000 / NS_GCD;
const NS_DEN: u64 = board::SYSCLK_HZ as u64 / NS_GCD;
const _: () = assert!(board::SYSCLK_HZ % board::TICK_HZ == 0, "tick not a whole cycle count");
const _: () = assert!(CYCLES_PER_TICK - 1 <= 0x00FF_FFFF, "reload exceeds SysTick's 24 bits");
const _: () = assert!(1_000_000_000 % board::TICK_HZ as u64 == 0, "tick not a whole ns count");
const _: () = assert!(
    CYCLES_PER_TICK as u64 * NS_NUM / NS_DEN == NS_PER_TICK,
    "sub-tick fraction disagrees with the tick clock at the boundary"
);

const fn gcd(mut a: u64, mut b: u64) -> u64 {
    while b != 0 {
        let t = a % b;
        a = b;
        b = t;
    }
    a
}

/// Thread state. `repr(u8)` with stated values: part of the
/// debug-visible layout contract.
#[derive(Clone, Copy, PartialEq)]
#[repr(u8)]
enum State {
    Free = 0,
    Ready = 1,
    Sleeping = 2,
    Exited = 3,
}

/// One thread's control block. `repr(C)`: the field layout is part of
/// the debug-visible layout contract (offsets exported below).
#[repr(C)]
struct Tcb {
    sp: u32,
    stack_base: u32,
    stack_top: u32,
    state: State,
    prio: u8,
    wake_tick: u32,
    tid: i32,
    entry: Option<extern "C" fn(usize)>,
    arg: usize,
    name: &'static str,
    /// Futex wait channel: the address this thread blocks on (0 = not a
    /// futex wait -- a futex waiter is `Sleeping` with this set, so the
    /// debug-layout state values stay v1). The WAKER clears it before
    /// readying; a timer wake leaves it set -- that asymmetry is how the
    /// resumed thread tells woken from timed out.
    wait_addr: usize,
    /// True iff `wake_tick` is armed for this futex wait (an infinite
    /// wait must not join the timer walk).
    wait_deadline: bool,
}

const TCB_INIT: Tcb = Tcb {
    sp: 0,
    stack_base: 0,
    stack_top: 0,
    state: State::Free,
    prio: 0xFF,
    wake_tick: 0,
    tid: 0,
    entry: None,
    arg: 0,
    name: "",
    wait_addr: 0,
    wait_deadline: false,
};

static NEXT_TID: AtomicU32 = AtomicU32::new(1);

struct Sched {
    tcbs: [Tcb; MAX_THREADS],
    current: usize,
    pool_used: usize,
    started: bool,
    /// Slot bitmasks mirroring `tcbs[i]`, written ONLY by `sync_bits`
    /// so they can never drift from the TCBs: bit i set iff the slot is
    /// pickable / has an armed wake timer. They turn the SysTick and
    /// pick() sweeps into set-bit iteration.
    ready_mask: u32,
    timer_mask: u32,
    futex_mask: u32,
    /// Exited-but-unreaped threads; reap() runs only when nonzero.
    exited_n: u32,
}

/// The one scheduler instance. All access happens under the kernel
/// critical section or from SysTick/PendSV, which the BASEPRI ceiling
/// masks -- single-core discipline. Exported by name for the
/// debug-visible layout contract (a debugger resolves the symbol and
/// applies `__rhu_debug_layout`'s offsets; no target code runs).
#[unsafe(export_name = "__rhu_sched")]
static mut SCHED: Sched = Sched {
    tcbs: [TCB_INIT; MAX_THREADS],
    current: 0,
    pool_used: 0,
    started: false,
    ready_mask: 0,
    timer_mask: 0,
    futex_mask: 0,
    exited_n: 0,
};

/// Thread stacks live in .kstacks -- outside the __sdata..__ebss window
/// (the board map places the section; spawn zeroes each carved region).
#[repr(align(8))]
struct StackPool([u8; board::STACK_POOL_BYTES]);
#[unsafe(link_section = ".kstacks")]
static mut STACK_POOL: StackPool = StackPool([0; board::STACK_POOL_BYTES]);

/// Milliseconds since scheduler init (wrapping; all comparisons are
/// wrapping). Only SysTick writes. Exported for the layout contract.
#[unsafe(export_name = "__rhu_ticks")]
pub static TICKS: AtomicU32 = AtomicU32::new(0);

/// High word of the 64-bit tick count: incremented by SysTick alone,
/// when `TICKS` wraps (~49.7 days at 1 kHz). Not part of the debug
/// contract; `now_ns`'s stability loop is the only reader.
static TICKS_HI: AtomicU32 = AtomicU32::new(0);

/// `now_ns` monotonicity watermark: the last value returned, split into
/// halves (this core has no 64-bit atomics). Read and written ONLY
/// inside the kernel CS in `now_ns` -- the CS, not the atomics, is the
/// pair's consistency.
static NOW_LAST_LO: AtomicU32 = AtomicU32::new(0);
static NOW_LAST_HI: AtomicU32 = AtomicU32::new(0);

/// Context switches taken since boot (PendSV entries; a switch back to
/// the same thread counts). Only the switch path writes. Exported
/// beside `__rhu_ticks` for the debug-visible contract; a
/// quiescence check reads the delta across a window to assert that no
/// switch happened inside it.
#[unsafe(export_name = "__rhu_switches")]
pub static SWITCHES: AtomicU32 = AtomicU32::new(0);

/// The debug-visible layout contract, v1: how external tooling reads
/// kernel state from memory with no target-resident agent. A debugger
/// resolves three symbols -- `__rhu_debug_layout` (this record),
/// `__rhu_sched`, `__rhu_ticks` -- then walks the TCB table using the
/// offsets recorded here. Offsets are computed by the compiler, so they
/// cannot drift from the structs. State values: 0 Free, 1 Ready,
/// 2 Sleeping, 3 Exited. `name` is a (pointer, length) string pair.
#[repr(C)]
pub struct DebugLayout {
    pub magic: u32,
    pub version: u32,
    pub max_threads: u32,
    pub sched_off_tcbs: u32,
    pub sched_off_current: u32,
    pub tcb_stride: u32,
    pub tcb_off_sp: u32,
    pub tcb_off_state: u32,
    pub tcb_off_prio: u32,
    pub tcb_off_tid: u32,
    pub tcb_off_name: u32,
    pub tcb_off_stack_base: u32,
    pub tcb_off_stack_top: u32,
}

#[used]
#[unsafe(no_mangle)]
#[unsafe(link_section = ".rhu_debug")]
pub static __rhu_debug_layout: DebugLayout = DebugLayout {
    magic: 0x4448_5552,
    version: 1,
    max_threads: MAX_THREADS as u32,
    sched_off_tcbs: offset_of!(Sched, tcbs) as u32,
    sched_off_current: offset_of!(Sched, current) as u32,
    tcb_stride: core::mem::size_of::<Tcb>() as u32,
    tcb_off_sp: offset_of!(Tcb, sp) as u32,
    tcb_off_state: offset_of!(Tcb, state) as u32,
    tcb_off_prio: offset_of!(Tcb, prio) as u32,
    tcb_off_tid: offset_of!(Tcb, tid) as u32,
    tcb_off_name: offset_of!(Tcb, name) as u32,
    tcb_off_stack_base: offset_of!(Tcb, stack_base) as u32,
    tcb_off_stack_top: offset_of!(Tcb, stack_top) as u32,
};

#[allow(clippy::mut_from_ref)]
unsafe fn sched() -> &'static mut Sched { unsafe {
    &mut *addr_of_mut!(SCHED)
}}

fn pend_switch() {
    unsafe { write_volatile(ICSR, ICSR_PENDSVSET) };
}

/// Recompute slot i's mask bits from its TCB -- the ONLY writer of the
/// ready/timer/futex masks. Every state mutation goes through
/// `set_state`, which calls this; that single-writer rule is what keeps
/// the masks and the TCBs from ever disagreeing. A futex waiter is
/// Sleeping with `wait_addr` set: it joins the futex walk always and
/// the timer walk only when a deadline is armed; a plain sleeper
/// (`wait_addr` 0) joins the timer walk alone.
fn sync_bits(s: &mut Sched, i: usize) {
    let bit = 1u32 << i;
    let t = &s.tcbs[i];
    if t.state == State::Ready {
        s.ready_mask |= bit;
    } else {
        s.ready_mask &= !bit;
    }
    if t.state == State::Sleeping && (t.wait_addr == 0 || t.wait_deadline) {
        s.timer_mask |= bit;
    } else {
        s.timer_mask &= !bit;
    }
    if t.state == State::Sleeping && t.wait_addr != 0 {
        s.futex_mask |= bit;
    } else {
        s.futex_mask &= !bit;
    }
}

/// State transition + mask maintenance in one step. Set every other
/// TCB field (wake_tick, ...) BEFORE calling this.
fn set_state(s: &mut Sched, i: usize, st: State) {
    s.tcbs[i].state = st;
    sync_bits(s, i);
}

pub fn now_ms() -> u32 {
    TICKS.load(Ordering::Relaxed)
}

/// Context switches taken so far (see `SWITCHES`).
#[allow(dead_code)]
pub fn switch_count() -> u32 {
    SWITCHES.load(Ordering::Relaxed)
}

/// The .kstacks pool window (for the boot selftest's placement check).
pub fn kstacks_window() -> (usize, usize) {
    let base = addr_of!(STACK_POOL) as usize;
    (base, base + board::STACK_POOL_BYTES)
}

/// The calling thread's stack top. A hosted payload's conservative GC
/// scans [current SP, stack top) of the thread it runs on -- the boot
/// image's "top of RAM" assumption is wrong under a scheduler.
#[cfg(feature = "payload")]
pub fn current_stack_top() -> u32 {
    with_cs(|| unsafe {
        let s = sched();
        s.tcbs[s.current].stack_top
    })
}

/// Kernel-thread spawn from the .kstacks pool. Returns the slot index.
/// The pool is bump-only: exited threads free their SLOT, not their
/// stack bytes -- kernel threads are long-lived by design here, and
/// stack recycling arrives with arena-backed stacks.
pub fn spawn(
    name: &'static str,
    entry: extern "C" fn(usize),
    arg: usize,
    prio: u8,
    stack_bytes: usize,
) -> usize {
    let stack_bytes = (stack_bytes + 7) & !7;
    let (slot, urgent) = with_cs(|| unsafe {
        let s = sched();
        let slot = (0..MAX_THREADS)
            .find(|&i| s.tcbs[i].state == State::Free)
            .expect("out of TCB slots");
        assert!(
            s.pool_used + stack_bytes <= board::STACK_POOL_BYTES,
            "stack pool exhausted"
        );
        let base = addr_of!(STACK_POOL.0) as usize + s.pool_used;
        s.pool_used += stack_bytes;
        let top = base + stack_bytes;
        ptr::write_bytes(base as *mut u8, 0, stack_bytes);

        let hw = (top - 32) as *mut u32;
        hw.add(0).write(slot as u32);
        hw.add(1).write(0);
        hw.add(2).write(0);
        hw.add(3).write(0);
        hw.add(4).write(0);
        hw.add(5).write(0);
        let tramp: extern "C" fn(u32) -> ! = thread_tramp;
        hw.add(6).write(tramp as usize as u32);
        hw.add(7).write(0x0100_0000);
        let sw = (top - 32 - 36) as *mut u32;
        for i in 0..8 {
            sw.add(i).write(0);
        }
        sw.add(8).write(0xFFFF_FFFD);

        s.tcbs[slot] = Tcb {
            sp: sw as u32,
            stack_base: base as u32,
            stack_top: top as u32,
            state: State::Ready,
            prio,
            wake_tick: 0,
            tid: NEXT_TID.fetch_add(1, Ordering::Relaxed) as i32,
            entry: Some(entry),
            arg,
            name,
            wait_addr: 0,
            wait_deadline: false,
        };
        sync_bits(s, slot);
        (slot, s.started && prio < s.tcbs[s.current].prio)
    });
    if urgent {
        pend_switch();
    }
    slot
}

extern "C" fn thread_tramp(slot: u32) -> ! {
    let (entry, arg) = with_cs(|| unsafe {
        let t = &sched().tcbs[slot as usize];
        (t.entry.expect("thread without entry"), t.arg)
    });
    entry(arg);
    exit_current()
}

pub fn exit_current() -> ! {
    with_cs(|| unsafe {
        let s = sched();
        let cur = s.current;
        set_state(s, cur, State::Exited);
        s.exited_n += 1;
    });
    pend_switch();
    loop {
        unsafe { core::arch::asm!("wfi", options(nomem, nostack)) };
    }
}

/// Reap finished threads: free the slots. Runs inside the switch path
/// with the CS held, so a reaped thread can never still be live -- it
/// has been switched away from for good.
unsafe fn reap(s: &mut Sched) {
    for i in 0..MAX_THREADS {
        if s.tcbs[i].state == State::Exited && i != s.current {
            s.tcbs[i] = TCB_INIT;
            sync_bits(s, i);
            s.exited_n -= 1;
        }
    }
}

#[allow(dead_code)]
pub fn yield_now() {
    pend_switch();
}

/// Tick-granular sleep: never returns early (the selftest pins that
/// contract); may return up to one tick late. Wrapping arithmetic
/// throughout. Spurious-wake tolerant: the deadline is re-checked on
/// every wake and the sleep re-armed if it has not arrived.
pub fn sleep_ms(ms: u32) {
    let deadline = now_ms().wrapping_add(ms);
    loop {
        let expired = with_cs(|| unsafe {
            if (now_ms().wrapping_sub(deadline) as i32) >= 0 {
                true
            } else {
                let s = sched();
                let cur = s.current;
                s.tcbs[cur].wake_tick = deadline;
                set_state(s, cur, State::Sleeping);
                false
            }
        });
        if expired {
            return;
        }
        pend_switch();
    }
}


/// Wait forever (`rhu.h` RHU_DEADLINE_NONE). Deadlines are absolute
/// nanoseconds on the `now_ns` clock.
pub const DEADLINE_NONE: u64 = u64::MAX;
/// RHU-owned error surface (errno-mirroring, defined in include/rhu.h).
pub const ERR_AGAIN: i32 = -11;
pub const ERR_TIMEDOUT: i32 = -110;
pub const ERR_INVAL: i32 = -22;

/// 64-bit monotonic nanoseconds since scheduler init. The four
/// recorded hazards, each handled in order:
///   1. torn 64-bit tick read -- hi/lo stability loop (re-read brackets;
///      the handler runs atomically w.r.t. thread level, so a bracketed
///      equal pair is a consistent pair);
///   2. sub-millisecond resolution -- SysTick CVR interpolation (CVR
///      counts DOWN from the reload value);
///   3. the wrapped-but-undelivered window -- the counter reloaded but
///      the tick interrupt has not run (masked under a kernel CS, or
///      raced): ICSR.PENDSTSET says so, and the read must add the
///      pending tick or time steps backward across the wrap. CVR is
///      resampled AFTER seeing the bit so the interpolation describes
///      the post-wrap countdown the +1 belongs to;
///   4. the saturated pend bit -- the bit is SINGLE, so a CS held
///      across more than one reload loses the extras, and a read that
///      consumed the +1 EARLY in such a CS shares its tick term with
///      every later read inside it: ordering would fall to CVR
///      interpolations from different countdown generations, which can
///      regress by a few cycles (or compare equal under an emulator's
///      lazily-advanced counter). The returned-value watermark below
///      carries ordering structurally: a computed value that fails to
///      exceed the last value returned becomes last+1 instead, so
///      distinct calls always advance -- a 1 ns floor can never outrun
///      a call's real cost. Accuracy across such a CS stays what it
///      was: the lost ticks stay lost, bounded by the kernel's
///      short-CS discipline; the watermark repairs ORDER, not
///      accuracy.
pub fn now_ns() -> u64 {
    let computed = loop {
        let h1 = TICKS_HI.load(Ordering::Relaxed);
        let l1 = TICKS.load(Ordering::Relaxed);
        let mut cvr = unsafe { read_volatile(SYST_CVR) };
        let pending = unsafe { read_volatile(ICSR) } & ICSR_PENDSTSET != 0;
        if pending {
            cvr = unsafe { read_volatile(SYST_CVR) };
        }
        let l2 = TICKS.load(Ordering::Relaxed);
        let h2 = TICKS_HI.load(Ordering::Relaxed);
        if h1 == h2 && l1 == l2 {
            let ticks = (((h1 as u64) << 32) | l1 as u64) + pending as u64;
            let elapsed = (CYCLES_PER_TICK - 1).saturating_sub(cvr) as u64;
            break ticks * NS_PER_TICK + elapsed * NS_NUM / NS_DEN;
        }
    };
    with_cs(|| {
        let last = ((NOW_LAST_HI.load(Ordering::Relaxed) as u64) << 32)
            | NOW_LAST_LO.load(Ordering::Relaxed) as u64;
        let v = if computed > last { computed } else { last + 1 };
        NOW_LAST_HI.store((v >> 32) as u32, Ordering::Relaxed);
        NOW_LAST_LO.store(v as u32, Ordering::Relaxed);
        v
    })
}

/// The tick at which `now_ns() >= deadline_ns` is guaranteed: ceil to
/// the next tick boundary, +1 for the arm-race margin (a tick firing
/// between the deadline computation and the arm). Ticks and `now_ns`
/// share the SysTick clock, so ceil alone is already never-early; the
/// negative control that reds the never-early checks is FLOORING this
/// conversion. Truncation to u32 is the wrapping
/// tick arithmetic.
fn tick_for_deadline(deadline_ns: u64) -> u32 {
    (deadline_ns.div_ceil(NS_PER_TICK) as u32).wrapping_add(1)
}

/// Nanosecond-request sleep: never early (gate check 2's contract), up
/// to ~2 ms late at the 1 kHz tick. Arm-once, no recheck loop: nothing
/// wakes a plain sleeper early today -- no signals by futex rule 2; a
/// future activation mechanism must revisit this.
pub fn sleep_ns(ns: u64) {
    let target = now_ns().saturating_add(ns);
    let expired = with_cs(|| unsafe {
        if now_ns() >= target {
            return true;
        }
        let s = sched();
        let cur = s.current;
        s.tcbs[cur].wait_addr = 0;
        s.tcbs[cur].wait_deadline = false;
        s.tcbs[cur].wake_tick = tick_for_deadline(target);
        set_state(s, cur, State::Sleeping);
        false
    });
    if !expired {
        pend_switch();
    }
}

/// Block until `*addr` is signaled, the value stops matching, or the
/// deadline passes. The compare-and-sleep is atomic w.r.t. wakers: both
/// sides run under the kernel CS (single core -- the CS IS the
/// atomicity), so the lost-wakeup window closes by construction.
/// Returns 0 woken, ERR_AGAIN when `*addr != expected` at entry,
/// ERR_TIMEDOUT when the deadline passed, ERR_INVAL on a null or
/// misaligned address (a wild-but-plausible pointer instead faults
/// through the funnel -- loud, never silent).
pub fn futex_wait(addr: *const u32, expected: u32, deadline_ns: u64) -> i32 {
    if addr.is_null() || (addr as usize) & 3 != 0 {
        return ERR_INVAL;
    }
    enum Armed {
        Value,
        Timeout,
        Slept,
    }
    let armed = with_cs(|| unsafe {
        if read_volatile(addr) != expected {
            return Armed::Value;
        }
        if deadline_ns != DEADLINE_NONE && now_ns() >= deadline_ns {
            return Armed::Timeout;
        }
        let s = sched();
        let cur = s.current;
        s.tcbs[cur].wait_addr = addr as usize;
        s.tcbs[cur].wait_deadline = deadline_ns != DEADLINE_NONE;
        if s.tcbs[cur].wait_deadline {
            s.tcbs[cur].wake_tick = tick_for_deadline(deadline_ns);
        }
        set_state(s, cur, State::Sleeping);
        Armed::Slept
    });
    match armed {
        Armed::Value => return ERR_AGAIN,
        Armed::Timeout => return ERR_TIMEDOUT,
        Armed::Slept => {}
    }
    pend_switch();
    let woken = with_cs(|| unsafe {
        let s = sched();
        let cur = s.current;
        if s.tcbs[cur].wait_addr == 0 {
            true
        } else {
            s.tcbs[cur].wait_addr = 0;
            s.tcbs[cur].wait_deadline = false;
            false
        }
    });
    if woken {
        0
    } else {
        ERR_TIMEDOUT
    }
}

/// Ready up to `n` waiters blocked on `addr`; returns the count.
/// Rotation-fair in slot order from the caller's slot (pick's split
/// walk -- allocation-free, no per-address queue state; slot rotation,
/// never FIFO). Every wake path pends explicitly, per the
/// conditional-tick policy: an equal-or-more-urgent woken thread runs
/// now, not at the next tick.
pub fn futex_wake(addr: *const u32, n: u32) -> i32 {
    if addr.is_null() || (addr as usize) & 3 != 0 {
        return ERR_INVAL;
    }
    let (count, urgent) = with_cs(|| unsafe {
        let s = sched();
        let split = (1u32 << (s.current + 1)).wrapping_sub(1);
        let mut count = 0u32;
        let mut urgent = false;
        'outer: for part in [s.futex_mask & !split, s.futex_mask & split] {
            let mut m = part;
            while m != 0 {
                if count == n {
                    break 'outer;
                }
                let i = m.trailing_zeros() as usize;
                m &= m - 1;
                if s.tcbs[i].wait_addr == addr as usize {
                    s.tcbs[i].wait_addr = 0;
                    s.tcbs[i].wait_deadline = false;
                    set_state(s, i, State::Ready);
                    if s.tcbs[i].prio <= s.tcbs[s.current].prio {
                        urgent = true;
                    }
                    count += 1;
                }
            }
        }
        (count, urgent)
    });
    if urgent {
        pend_switch();
    }
    count as i32
}

/// Kernel-policy observability for the api smoke ONLY (the
/// readied-vs-ran audit fold): counts read under the CS,
/// never shipped in the default build.
#[cfg(feature = "api-smoke")]
pub fn probe_futex_waiting(addr: *const u32) -> u32 {
    with_cs(|| unsafe {
        let s = sched();
        let mut m = s.futex_mask;
        let mut count = 0;
        while m != 0 {
            let i = m.trailing_zeros() as usize;
            m &= m - 1;
            if s.tcbs[i].wait_addr == addr as usize {
                count += 1;
            }
        }
        count
    })
}

/// Ready-state count among the given slots (see `probe_futex_waiting`).
#[cfg(feature = "api-smoke")]
pub fn probe_ready_among(slots: &[usize]) -> u32 {
    with_cs(|| unsafe {
        let s = sched();
        slots.iter().filter(|&&i| s.tcbs[i].state == State::Ready).count() as u32
    })
}

/// Round-robin pick of the most urgent Ready thread: walk the ready
/// mask's set bits in rotation order current+1..wrap (high half first,
/// then low), strict `<` so the first equal-best in rotation order wins
/// and `current` competes last. The idle thread is always Ready, so the
/// mask is never empty.
fn pick(s: &mut Sched) -> usize {
    let split = (1u32 << (s.current + 1)).wrapping_sub(1);
    let mut best = s.current;
    let mut best_prio = 0x1FFu32;
    for part in [s.ready_mask & !split, s.ready_mask & split] {
        let mut m = part;
        while m != 0 {
            let i = m.trailing_zeros() as usize;
            m &= m - 1;
            if (s.tcbs[i].prio as u32) < best_prio {
                best = i;
                best_prio = s.tcbs[i].prio as u32;
            }
        }
    }
    best
}

#[unsafe(no_mangle)]
extern "C" fn __rhu_switch(sp: u32) -> u32 {
    SWITCHES.fetch_add(1, Ordering::Relaxed);
    with_cs(|| unsafe {
        let s = sched();
        s.tcbs[s.current].sp = sp;
        let next = pick(s);
        s.current = next;
        if s.exited_n > 0 {
            reap(s);
        }
        s.tcbs[next].sp
    })
}

#[unsafe(no_mangle)]
extern "C" fn __rhu_first_sp() -> u32 {
    with_cs(|| unsafe {
        let s = sched();
        s.started = true;
        s.tcbs[s.current].sp
    })
}

#[unsafe(no_mangle)]
extern "C" fn __rhu_msp_top() -> u32 {
    unsafe extern "C" {
        static __stack_start: u32;
    }
    addr_of!(__stack_start) as usize as u32
}

/// True iff a Ready thread OTHER than current is equal-or-more urgent
/// (numerically `<=`; the equality arm is what rotates equal-priority
/// peers on the tick). Mask-derived, read-only.
fn ready_peer_at_or_above(s: &Sched) -> bool {
    let cur_prio = s.tcbs[s.current].prio;
    let mut m = s.ready_mask & !(1u32 << s.current);
    while m != 0 {
        let i = m.trailing_zeros() as usize;
        m &= m - 1;
        if s.tcbs[i].prio <= cur_prio {
            return true;
        }
    }
    false
}

/// SysTick: advance the tick clock, wake due sleepers, and pend a
/// switch iff a timer fired this tick or an equal-or-more-urgent Ready
/// peer exists (a solo thread runs tick-free). Runs above PendSV and
/// below the zero-latency band; kernel CS holders delay it wholesale,
/// so scheduler state is never seen torn.
pub unsafe extern "C" fn systick_vector() { unsafe {
    let now = TICKS.fetch_add(1, Ordering::Release).wrapping_add(1);
    if now == 0 {
        TICKS_HI.fetch_add(1, Ordering::Release);
    }
    let s = sched();
    if !s.started {
        return;
    }
    let mut fired = false;
    let mut m = s.timer_mask;
    while m != 0 {
        let i = m.trailing_zeros() as usize;
        m &= m - 1;
        if (now.wrapping_sub(s.tcbs[i].wake_tick) as i32) >= 0 {
            set_state(s, i, State::Ready);
            fired = true;
        }
    }
    if fired || ready_peer_at_or_above(s) {
        pend_switch();
    }
}}

extern "C" fn idle_entry(_: usize) {
    loop {
        unsafe { core::arch::asm!("wfi", options(nomem, nostack)) };
    }
}

/// Configure exception priorities + SysTick and create the idle thread.
pub fn init() {
    unsafe {
        write_volatile(SHPR2, 0xF0 << 24);
        write_volatile(SHPR3, (0x80 << 24) | (0xF0 << 16));
        write_volatile(SYST_RVR, board::SYSCLK_HZ / board::TICK_HZ - 1);
        write_volatile(SYST_CVR, 0);
        write_volatile(SYST_CSR, 0b111);
    }
    spawn("idle", idle_entry, 0, 0xFF, 1024);
}

/// Hand the CPU to the scheduler; the given slot runs first. The boot
/// thread's stack is abandoned -- MSP becomes the handler stack.
pub fn start(main_slot: usize) -> ! {
    with_cs(|| unsafe {
        sched().current = main_slot;
    });
    unsafe { core::arch::asm!("svc #0", options(noreturn)) }
}

global_asm!(
    r#"
    .fpu fpv4-sp-d16

    .section .text.SvcHandler, "ax", %progbits
    .global SvcHandler
    .type SvcHandler, %function
    .thumb_func
SvcHandler:
    /* Dispatch on the SVC immediate (read out of the instruction below
       the stacked PC; SVC is the 16-bit 0xDFxx encoding):
         #0  scheduler first start
       Anything else is unexpected here and takes the terminal funnel. */
    tst   lr, #4
    ite   eq
    mrseq r0, msp
    mrsne r0, psp
    ldr   r1, [r0, #24]       /* stacked PC */
    subs  r1, r1, #2
    ldrb  r1, [r1]
    cbz   r1, 1f
    b     ExceptionTrampoline

1:  /* First start: reset MSP to the top (the boot-thread frame below is
       abandoned; MSP is the handler stack from here on), then
       exception-return into the first thread via its prebuilt frames. */
    bl    __rhu_msp_top
    msr   msp, r0
    bl    __rhu_first_sp
    ldmia r0!, {{r4-r11, lr}}
    msr   psp, r0
    bx    lr
    .size SvcHandler, . - SvcHandler

    .section .text.PendSvHandler, "ax", %progbits
    .global PendSvHandler
    .type PendSvHandler, %function
    .thumb_func
PendSvHandler:
    /* Save the outgoing thread: s16-s31 only if its frame is extended
       (EXC_RETURN bit 4 clear; the FP store itself completes any
       pending lazy s0-s15 stacking first, so no frame is ever abandoned
       with lazy state armed), then r4-r11 + EXC_RETURN. */
    mrs   r0, psp
    tst   lr, #0x10
    it    eq
    vstmdbeq r0!, {{s16-s31}}
    stmdb r0!, {{r4-r11, lr}}
    bl    __rhu_switch
    ldmia r0!, {{r4-r11, lr}}
    tst   lr, #0x10
    it    eq
    vldmiaeq r0!, {{s16-s31}}
    msr   psp, r0
    bx    lr
    .size PendSvHandler, . - PendSvHandler
"#
);
