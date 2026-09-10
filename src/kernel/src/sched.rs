//! The scheduler: fixed-priority preemptive with round-robin inside a
//! priority, PendSV context switch (lazy-FP honest), SysTick timebase,
//! BASEPRI-only masking throughout.

use core::arch::global_asm;
use core::ffi::c_void;
use core::mem::offset_of;
use core::ptr::{self, addr_of, addr_of_mut, read_volatile, write_volatile};
use core::sync::atomic::{AtomicU32, AtomicUsize, Ordering};

use crate::board;
use crate::critical::with_cs;
use crate::{kheap, registry};

/// TCB slots. Sized for the hosted runtime's thread census plus kernel
/// threads; the slot masks are u32, so this must stay below 32.
pub const MAX_THREADS: usize = 16;
const _: () = assert!(MAX_THREADS <= 31, "slot masks are u32");

/// Per-thread exit-destructor stack depth (rhu.h RHU_ATEXIT_SLOTS --
/// the two change together or not at all).
pub const ATEXIT_SLOTS: usize = 8;

/// Retired-stack list depth: exact-fit reuse covers the repeating-size
/// case (thread-pool workers); overflow lands in the audited leak
/// counters, visible and bounded, never a panic. (pub(crate): the api
/// smoke's leak-audit algebra reads it.)
pub(crate) const RETIRED_CAP: usize = 8;

/// A thread-exit destructor as the C ABI hands it over.
type AtexitFn = extern "C" fn(*mut c_void);

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
/// The wake-target arithmetic (pure; the host tier compiles it from the
/// same file).
#[path = "wake.rs"]
pub(crate) mod wake;

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

/// The thread stack floor, bytes: the frames the birth writes (32 + 36),
/// an extended floating-point frame's extra 104 and an activation
/// delivery's 140 sum to 312, rounded up to the next power of two.
/// Mirrored by `include/rhu.h` (RHU_THREAD_STACK_MIN) for the C door,
/// which refuses below it; a kernel-internal spawn below it is a bug.
pub const THREAD_STACK_MIN: usize = 512;

/// The name buffer a control block carries: at most `NAME_BYTES - 1`
/// bytes of the spawn name, cut at a character boundary, NUL after.
pub const NAME_BYTES: usize = 16;

/// The name as the layout record reads it: a (pointer, length) pair
/// pointing into the control block's own `name_buf` -- the caller's
/// bytes are copied at birth and may go away once the spawn returns.
#[repr(C)]
#[derive(Clone, Copy)]
#[allow(dead_code)]
struct NameRef {
    ptr: *const u8,
    len: usize,
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
    name: NameRef,
    /// Futex wait channel: the address this thread blocks on (0 = not a
    /// futex wait -- a futex waiter is `Sleeping` with this set, so the
    /// debug-layout state values stay v1). The WAKER clears it before
    /// readying; a timer wake leaves it set -- that asymmetry is how the
    /// resumed thread tells woken from timed out.
    wait_addr: usize,
    /// True iff `wake_tick` is armed for this futex wait (an infinite
    /// wait must not join the timer walk).
    wait_deadline: bool,
    /// Per-module TLS blocks, kheap-backed, eagerly allocated (null =
    /// module absent or zero-sized). Cleared under the CS before their
    /// blocks are freed at exit, so the teardown is once-only even if
    /// a dtor exits again.
    tls: [*mut u8; registry::MAX_MODULES],
    /// LIFO thread-exit destructor stack; runs on the exiting thread
    /// before TLS teardown.
    atexit: [(Option<AtexitFn>, *mut c_void); ATEXIT_SLOTS],
    atexit_n: u8,
    /// Suspension flag, ORTHOGONAL to `state`: a suspended thread
    /// keeps its state and its waits; it is simply never in the
    /// pickable set (sync_bits's ready arm). Only suspend/resume
    /// write it, under the CS.
    suspended: bool,
    /// Activation pending -- the flag IS the whole queue (sends
    /// coalesce). Consumed by the delivery transform at scheduling-in
    /// or by the SVC #1 re-delivery check.
    act_pending: bool,
    /// True from the delivery transform until the final SVC #1
    /// restore: gates re-transformation of a preempted handler.
    act_in_handler: bool,
    /// The futex-wait interruption latch: set by the break arm when a
    /// send readies a futex waiter; read-and-cleared by the wait's
    /// exit path (the third exit beside woken/timed-out).
    act_interrupted: bool,
    /// The interrupted context's saved-frame sp, held across the
    /// handler run so SVC #1 can rebuild the restore from it.
    act_saved_sp: u32,
    /// The 64-bit tick a sleep or a timed wait truly targets (0 = none).
    /// The armed `wake_tick` is this target inside the timer walk's
    /// 32-bit window, or the window's far edge with a re-arm at expiry
    /// -- a deadline past the window is never early.
    wake_far: u64,
    /// The spawn name's bytes (appended, the Tcb append rule; `name`
    /// points here), NUL after the kept bytes.
    #[allow(dead_code)]
    name_buf: [u8; NAME_BYTES],
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
    name: NameRef { ptr: ptr::null(), len: 0 },
    wait_addr: 0,
    wait_deadline: false,
    tls: [ptr::null_mut(); registry::MAX_MODULES],
    atexit: [(None, ptr::null_mut()); ATEXIT_SLOTS],
    atexit_n: 0,
    suspended: false,
    act_pending: false,
    act_in_handler: false,
    act_interrupted: false,
    act_saved_sp: 0,
    wake_far: 0,
    name_buf: [0; NAME_BYTES],
};

static NEXT_TID: AtomicU32 = AtomicU32::new(1);

/// THE process-global activation handler (0 = unregistered); written
/// once by `act_register` under the CS, read at delivery.
static ACT_HANDLER: AtomicUsize = AtomicUsize::new(0);

/// Idle's slot, recorded once at `init()` -- the refusal arms resolve
/// idle by slot, never by name or priority.
static IDLE_SLOT: AtomicU32 = AtomicU32::new(u32::MAX);

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
    /// Retired stacks (base, len), pushed at reap, taken by exact-fit
    /// at spawn -- the pool's recycling story for the repeating-size
    /// case.
    retired: [(u32, u32); RETIRED_CAP],
    retired_n: u32,
    /// Retire-list overflow audit: entries dropped and bytes leaked.
    /// Bounded by construction, loud by visibility, never a panic.
    leak_drops: u32,
    leak_bytes: u32,
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
    retired: [(0, 0); RETIRED_CAP],
    retired_n: 0,
    leak_drops: 0,
    leak_bytes: 0,
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

/// The lost-tick witness: in the tick handler, the cycle counter's
/// reading against the previous entry's, so a tick that fired while
/// the kernel critical section masked it -- coalesced into one pending
/// bit -- becomes a number the transcript carries. An entry is LATE when
/// more than a period and a half elapsed since the last (a normal
/// entry's jitter never counts, a coalesced tick always does); the ticks
/// it LOST are the whole periods beyond the first. Counting, never
/// crediting: the tick is the scheduler's quantum, and a kernel with a
/// masked tick must not look healthy by its own clock. Seven words,
/// exported for a probe beside `__rhu_ticks`: the source (0 = no cycle
/// counter on this row, 1 = counting, 2 = expected but the counter did
/// not advance), the period in cycles, the entries, the late entries,
/// the lost ticks, the maximum elapsed, the previous reading. Written
/// by SysTick alone; the census line every gated transcript ends with
/// prints the two counts (`tick_witness_print`, from the exit funnel).
#[repr(C)]
pub struct TickWitness {
    pub source: u32,
    pub period: u32,
    pub entries: u32,
    pub late: u32,
    pub lost: u32,
    pub max: u32,
    pub prev: u32,
}

#[unsafe(export_name = "__rhu_tick_witness")]
pub static mut TICK_WITNESS: TickWitness = TickWitness {
    source: WITNESS_NONE,
    period: 0,
    entries: 0,
    late: 0,
    lost: 0,
    max: 0,
    prev: 0,
};

const WITNESS_NONE: u32 = 0;
const WITNESS_DWT: u32 = 1;
const WITNESS_STATIC: u32 = 2;

/// The debug architecture's cycle counter (the data watchpoint and trace
/// unit, ARMv7-M and ARMv8-M): the trace enable in the debug exception
/// and monitor control register, the unit's lock register where a core
/// implements it, its control word and the counter.
const DEMCR: *mut u32 = 0xE000_EDFC as *mut u32;
const DEMCR_TRCENA: u32 = 1 << 24;
const DWT_CTRL: *mut u32 = 0xE000_1000 as *mut u32;
const DWT_CYCCNT: *mut u32 = 0xE000_1004 as *mut u32;
const DWT_LAR: *mut u32 = 0xE000_1FB0 as *mut u32;
const DWT_LAR_UNLOCK: u32 = 0xC5AC_CE55;
const DWT_CTRL_CYCCNTENA: u32 = 1;

/// The census line every gated transcript ends with (the exit funnel
/// calls it): the two counts on a counting row, or why there are none.
pub fn tick_witness_print() {
    let (source, period, late, lost) = unsafe {
        let w = &*core::ptr::addr_of!(TICK_WITNESS);
        (
            read_volatile(&w.source),
            read_volatile(&w.period),
            read_volatile(&w.late),
            read_volatile(&w.lost),
        )
    };
    match source {
        WITNESS_DWT => kprintln!("[tick] witness: {} lost, {} late (> 1/2 period), cycle counter DWT", lost, late),
        WITNESS_STATIC => kprintln!("[tick] witness: not counted (the cycle counter did not advance)"),
        _ if board::CYCLE_COUNTER && period == 0 => kprintln!("[tick] witness: not counted (the tick never started)"),
        _ => kprintln!("[tick] witness: not counted (no cycle counter on this row)"),
    }
}

/// The switch-entry witness of the lazy floating-point discipline (the
/// frame-type note above the suspension + activation seam): how many
/// PendSV entries found lazy floating-point state still pending while
/// their OWN frame was basic -- the outgoing thread held no
/// floating-point context, so the pending save belongs to no live frame
/// and is aimed at whatever now occupies the address an abandoned frame
/// held -- with the pending save's address and the outgoing thread's
/// stack pointer from the last such entry. Zero in a correct kernel.
/// Only the PendSV entry writes it; the api smoke reads it.
#[repr(C)]
pub struct LazyFpWitness {
    pub count: AtomicU32,
    pub fpcar: AtomicU32,
    pub psp: AtomicU32,
}
#[unsafe(export_name = "__rhu_lazy_fp_witness")]
pub static LAZY_FP_WITNESS: LazyFpWitness = LazyFpWitness {
    count: AtomicU32::new(0),
    fpcar: AtomicU32::new(0),
    psp: AtomicU32::new(0),
};

/// The version of the debug-visible layout record this kernel emits.
/// Fields never move or change meaning across versions; later versions
/// append, and a reader accepts any record whose version is at least
/// the one it knows.
pub const DEBUG_LAYOUT_VERSION: u32 = 2;

/// The debug-visible layout contract: how external tooling reads
/// kernel state from memory with no target-resident agent. A debugger
/// resolves three symbols -- `__rhu_debug_layout` (this record),
/// `__rhu_sched`, `__rhu_ticks` -- then walks the TCB table using the
/// offsets recorded here. Offsets are computed by the compiler, so they
/// cannot drift from the structs. State values: 0 Free, 1 Ready,
/// 2 Sleeping, 3 Exited. `name` is a (pointer, length) string pair
/// pointing into the control block's own buffer (appended past the
/// version-2 fields: at most 15 bytes, NUL after), so a name never
/// outlives its thread's block.
///
/// Version 2 appends the thread-local-base door: `tcb_off_tls` is the
/// offset of a thread's per-module block pointer array, `tls_slots` its
/// length and `tls_slot_bytes` one entry, so module `m`'s block for the
/// thread whose control block starts at `tcb` is the pointer-sized word
/// at `tcb + tcb_off_tls + m * tls_slot_bytes` -- 0 when the module has
/// no thread-local state or the slot holds no live thread. Module 0 is
/// the merged image's own thread-local template (the `.tdata` brackets):
/// a thread-static at template offset X lives at that thread's module-0
/// block plus X, the same arithmetic the hosting shim's
/// `__tls_get_addr` performs for the running thread.
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
    pub tcb_off_tls: u32,
    pub tls_slots: u32,
    pub tls_slot_bytes: u32,
}

#[used]
#[unsafe(no_mangle)]
#[unsafe(link_section = ".rhu_debug")]
pub static __rhu_debug_layout: DebugLayout = DebugLayout {
    magic: 0x4448_5552,
    version: DEBUG_LAYOUT_VERSION,
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
    tcb_off_tls: offset_of!(Tcb, tls) as u32,
    tls_slots: registry::MAX_MODULES as u32,
    tls_slot_bytes: core::mem::size_of::<*mut u8>() as u32,
};

#[allow(clippy::mut_from_ref)]
unsafe fn sched() -> &'static mut Sched { unsafe {
    &mut *addr_of_mut!(SCHED)
}}

fn pend_switch() {
    unsafe {
        write_volatile(ICSR, ICSR_PENDSVSET);
        core::arch::asm!("dsb sy", "isb sy", options(nostack, preserves_flags));
    }
}

/// Recompute slot i's mask bits from its TCB -- the ONLY writer of the
/// ready/timer/futex masks. Every state mutation goes through
/// `set_state`, which calls this; that single-writer rule is what keeps
/// the masks and the TCBs from ever disagreeing. A futex waiter is
/// Sleeping with `wait_addr` set: it joins the futex walk always and
/// the timer walk only when a deadline is armed; a plain sleeper
/// (`wait_addr` 0) joins the timer walk alone. The ready mask is the
/// PICKABLE set: Ready AND not suspended (suspension is orthogonal to
/// state -- a suspended thread keeps its timer/futex membership and is
/// simply never picked).
fn sync_bits(s: &mut Sched, i: usize) {
    let bit = 1u32 << i;
    let t = &s.tcbs[i];
    if t.state == State::Ready && !t.suspended {
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

/// Why a spawn was refused. Kernel callers turn each into its loud
/// panic; the C door maps all four onto one honest RHU_EAGAIN.
pub enum SpawnErr {
    NoSlot,
    PoolExhausted,
    TlsExhausted,
    /// The thread-id space (the positive `i32` range) is spent: the
    /// counter never wraps, so a stale id can never alias a newer
    /// thread.
    TidExhausted,
}

/// The byte count of `name` a control block keeps: at most
/// `NAME_BYTES - 1`, cut back to a character boundary.
fn name_cut(name: &str) -> usize {
    let mut n = name.len().min(NAME_BYTES - 1);
    while !name.is_char_boundary(n) {
        n -= 1;
    }
    n
}

/// Fallible thread spawn from the .kstacks pool: `(slot, tid)` or the
/// refusal reason with NOTHING committed (a taken retired stack is put
/// back, a bump is un-bumped, partial TLS blocks are freed). Stacks
/// come from exact-fit reuse of the retired list first, the bump top
/// second; the new thread eagerly receives a TLS block for every
/// registered module (the other eager direction
/// lives in `tls_on_register`).
pub fn try_spawn(
    name: &str,
    entry: extern "C" fn(usize),
    arg: usize,
    prio: u8,
    stack_bytes: usize,
) -> Result<(usize, i32), SpawnErr> {
    assert!(
        (THREAD_STACK_MIN..=usize::MAX - 7).contains(&stack_bytes),
        "spawn: a stack of {} bytes is below the floor or wraps the rounding",
        stack_bytes
    );
    let stack_bytes = (stack_bytes + 7) & !7;
    let r = with_cs(|| unsafe {
        if NEXT_TID.load(Ordering::Relaxed) > i32::MAX as u32 {
            return Err(SpawnErr::TidExhausted);
        }
        let (demands, nmod) = registry::tls_demands();
        let s = sched();
        let Some(slot) = (0..MAX_THREADS).find(|&i| s.tcbs[i].state == State::Free) else {
            return Err(SpawnErr::NoSlot);
        };
        let mut from_retired = false;
        let base = if let Some(ri) =
            (0..s.retired_n as usize).find(|&i| s.retired[i].1 as usize == stack_bytes)
        {
            let (b, _) = s.retired[ri];
            s.retired[ri] = s.retired[s.retired_n as usize - 1];
            s.retired_n -= 1;
            from_retired = true;
            b as usize
        } else {
            if stack_bytes > board::STACK_POOL_BYTES - s.pool_used {
                return Err(SpawnErr::PoolExhausted);
            }
            let b = addr_of!(STACK_POOL.0) as usize + s.pool_used;
            s.pool_used += stack_bytes;
            b
        };
        let top = base + stack_bytes;
        ptr::write_bytes(base as *mut u8, 0, stack_bytes);

        let mut tls = [ptr::null_mut::<u8>(); registry::MAX_MODULES];
        for m in 0..nmod {
            let (bytes, init) = demands[m];
            if bytes == 0 {
                continue;
            }
            let block = kheap::alloc(bytes, 8);
            if block.is_null() {
                for b in tls.iter().take(m) {
                    if !b.is_null() {
                        kheap::free(*b);
                    }
                }
                if from_retired {
                    s.retired[s.retired_n as usize] = (base as u32, stack_bytes as u32);
                    s.retired_n += 1;
                } else {
                    s.pool_used -= stack_bytes;
                }
                return Err(SpawnErr::TlsExhausted);
            }
            if init.is_null() {
                ptr::write_bytes(block, 0, bytes as usize);
            } else {
                ptr::copy_nonoverlapping(init, block, bytes as usize);
            }
            tls[m] = block;
        }

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

        let mut name_buf = [0u8; NAME_BYTES];
        let name_len = name_cut(name);
        name_buf[..name_len].copy_from_slice(&name.as_bytes()[..name_len]);
        let name = NameRef { ptr: addr_of!(s.tcbs[slot].name_buf) as *const u8, len: name_len };
        let tid = NEXT_TID.fetch_add(1, Ordering::Relaxed) as i32;
        s.tcbs[slot] = Tcb {
            sp: sw as u32,
            stack_base: base as u32,
            stack_top: top as u32,
            state: State::Ready,
            prio,
            wake_tick: 0,
            tid,
            entry: Some(entry),
            arg,
            name,
            wait_addr: 0,
            wait_deadline: false,
            tls,
            atexit: [(None, ptr::null_mut()); ATEXIT_SLOTS],
            atexit_n: 0,
            suspended: false,
            act_pending: false,
            act_in_handler: false,
            act_interrupted: false,
            act_saved_sp: 0,
            wake_far: 0,
            name_buf,
        };
        sync_bits(s, slot);
        Ok((slot, tid, s.started && prio < s.tcbs[s.current].prio))
    });
    match r {
        Ok((slot, tid, urgent)) => {
            if urgent {
                pend_switch();
            }
            Ok((slot, tid))
        }
        Err(e) => Err(e),
    }
}

/// Kernel-thread spawn: birth errors are kernel bugs and stay loud
/// panics (hosted callers go through the C door, which refuses
/// honestly instead). Returns the slot index.
pub fn spawn(
    name: &str,
    entry: extern "C" fn(usize),
    arg: usize,
    prio: u8,
    stack_bytes: usize,
) -> usize {
    match try_spawn(name, entry, arg, prio, stack_bytes) {
        Ok((slot, _)) => slot,
        Err(SpawnErr::NoSlot) => panic!("out of TCB slots"),
        Err(SpawnErr::PoolExhausted) => panic!("stack pool exhausted"),
        Err(SpawnErr::TlsExhausted) => panic!("TLS exhausted at spawn"),
        Err(SpawnErr::TidExhausted) => panic!("thread ids exhausted"),
    }
}

extern "C" fn thread_tramp(slot: u32) -> ! {
    let (entry, arg) = with_cs(|| unsafe {
        let t = &sched().tcbs[slot as usize];
        (t.entry.expect("thread without entry"), t.arg)
    });
    entry(arg);
    exit_current()
}

/// Thread exit, in the registered order: dtors run LIFO on the exiting
/// thread (outside the CS -- they are arbitrary code), then TLS blocks
/// are freed clear-then-free (so a dtor that itself exits re-enters
/// this path with the remaining dtors still running exactly once and
/// the TLS freed exactly once), then the slot goes Exited and the reap
/// recycles the stack.
pub fn exit_current() -> ! {
    loop {
        let popped = with_cs(|| unsafe {
            let s = sched();
            let t = &mut s.tcbs[s.current];
            if t.atexit_n == 0 {
                None
            } else {
                t.atexit_n -= 1;
                let e = t.atexit[t.atexit_n as usize];
                t.atexit[t.atexit_n as usize] = (None, ptr::null_mut());
                Some(e)
            }
        });
        match popped {
            Some((Some(f), a)) => f(a),
            _ => break,
        }
    }
    let blocks = with_cs(|| unsafe {
        let s = sched();
        let mut b = [ptr::null_mut::<u8>(); registry::MAX_MODULES];
        core::mem::swap(&mut b, &mut s.tcbs[s.current].tls);
        b
    });
    for p in blocks {
        if !p.is_null() {
            kheap::free(p);
        }
    }
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

/// Reap finished threads: retire the stack for exact-fit reuse (a full
/// retired list drops into the audited leak counters -- visible,
/// bounded, never a panic), then free the slot. Runs inside the switch
/// path with the CS held, so a reaped thread can never still be live
/// -- it has been switched away from for good.
unsafe fn reap(s: &mut Sched) {
    for i in 0..MAX_THREADS {
        if s.tcbs[i].state == State::Exited && i != s.current {
            let len = s.tcbs[i].stack_top - s.tcbs[i].stack_base;
            if (s.retired_n as usize) < RETIRED_CAP {
                s.retired[s.retired_n as usize] = (s.tcbs[i].stack_base, len);
                s.retired_n += 1;
            } else {
                s.leak_drops += 1;
                s.leak_bytes = s.leak_bytes.saturating_add(len);
            }
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
                s.tcbs[cur].wake_far = 0;
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
/// No such thread (the first tid-addressed verb's refusal).
pub const ERR_SRCH: i32 = -3;
/// Futex wait broken by activation delivery (rhu.h RHU_EINTR).
pub const ERR_INTR: i32 = -4;

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

/// The 64-bit tick at which `now_ns() >= deadline_ns` is guaranteed: ceil
/// to the next tick boundary, +1 for the arm-race margin (a tick firing
/// between the deadline computation and the arm). Ticks and `now_ns`
/// share the SysTick clock, so ceil alone is already never-early; the
/// negative control that reds the never-early checks is FLOORING this
/// conversion. The wake module carries the arithmetic.
fn tick_for_deadline(deadline_ns: u64) -> u64 {
    wake::tick_for_deadline(deadline_ns, NS_PER_TICK)
}

/// The 64-bit tick now: the counter's two words, read under the kernel
/// critical section (the tick cannot move between them there).
fn now_tick64() -> u64 {
    ((TICKS_HI.load(Ordering::Relaxed) as u64) << 32) | TICKS.load(Ordering::Relaxed) as u64
}

/// Arm slot `i`'s timer at a 64-bit target tick: the target itself when
/// it lies inside the timer walk's window, else the window's far edge
/// with the target kept for the walk's re-arm at expiry.
fn arm_target(s: &mut Sched, i: usize, target: u64) {
    let (armed, _clamped) = wake::arm(now_tick64(), target);
    s.tcbs[i].wake_tick = armed;
    s.tcbs[i].wake_far = target;
}

/// Nanosecond-request sleep: never early (gate check 2's contract), up
/// to ~2 ms late at the 1 kHz tick. Recheck loop: an activation
/// delivery breaks
/// the sleep for the handler run, and the loop re-arms the REMAINING
/// time -- never-early survives and the caller sees nothing.
pub fn sleep_ns(ns: u64) {
    let target = now_ns().saturating_add(ns);
    loop {
        let expired = with_cs(|| unsafe {
            if now_ns() >= target {
                return true;
            }
            let s = sched();
            let cur = s.current;
            s.tcbs[cur].wait_addr = 0;
            s.tcbs[cur].wait_deadline = false;
            arm_target(s, cur, tick_for_deadline(target));
            set_state(s, cur, State::Sleeping);
            false
        });
        if expired {
            return;
        }
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
            arm_target(s, cur, tick_for_deadline(deadline_ns));
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
    with_cs(|| unsafe {
        let s = sched();
        let cur = s.current;
        let intr = s.tcbs[cur].act_interrupted;
        s.tcbs[cur].act_interrupted = false;
        if s.tcbs[cur].wait_addr == 0 {
            0
        } else {
            s.tcbs[cur].wait_addr = 0;
            s.tcbs[cur].wait_deadline = false;
            if intr { ERR_INTR } else { ERR_TIMEDOUT }
        }
    })
}

/// Ready up to `n` waiters blocked on `addr`; returns the count.
/// Priority first: each pick readies the most urgent waiter on the
/// word, ties in rotation order from the caller's slot (pick's split
/// walk -- allocation-free, no per-address queue state; never FIFO).
/// Every wake path pends explicitly, per the
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
        while count < n {
            let mut best: Option<usize> = None;
            for part in [s.futex_mask & !split, s.futex_mask & split] {
                let mut m = part;
                while m != 0 {
                    let i = m.trailing_zeros() as usize;
                    m &= m - 1;
                    if s.tcbs[i].wait_addr == addr as usize
                        && best.is_none_or(|b| s.tcbs[i].prio < s.tcbs[b].prio)
                    {
                        best = Some(i);
                    }
                }
            }
            let Some(i) = best else {
                break;
            };
            s.tcbs[i].wait_addr = 0;
            s.tcbs[i].wait_deadline = false;
            set_state(s, i, State::Ready);
            if !s.tcbs[i].suspended && s.tcbs[i].prio <= s.tcbs[s.current].prio {
                urgent = true;
            }
            count += 1;
        }
        (count, urgent)
    });
    if urgent {
        pend_switch();
    }
    count as i32
}


/// Registration-side eager TLS: give every LIVE thread a block for the
/// module being registered. All-or-nothing: a refused block unwinds
/// the walk and reports false, and the registration commits nothing.
/// Runs under the registry's CS (nesting is the CS's contract).
pub fn tls_on_register(module: usize, bytes: u32, init: *const u8) -> bool {
    if bytes == 0 {
        return true;
    }
    with_cs(|| unsafe {
        let s = sched();
        for i in 0..MAX_THREADS {
            let live = s.tcbs[i].state == State::Ready || s.tcbs[i].state == State::Sleeping;
            if !live {
                continue;
            }
            let block = kheap::alloc(bytes, 8);
            if block.is_null() {
                for j in 0..i {
                    let b = s.tcbs[j].tls[module];
                    if !b.is_null() {
                        s.tcbs[j].tls[module] = ptr::null_mut();
                        kheap::free(b);
                    }
                }
                return false;
            }
            if init.is_null() {
                ptr::write_bytes(block, 0, bytes as usize);
            } else {
                ptr::copy_nonoverlapping(init, block, bytes as usize);
            }
            s.tcbs[i].tls[module] = block;
        }
        true
    })
}

/// The calling thread's TLS block for a module handle. Eager
/// allocation makes a valid handle's block non-null by construction;
/// an invalid handle, or one registered with zero TLS, is a caller bug
/// and dies loud.
pub fn tls_get(module: i32) -> *mut u8 {
    with_cs(|| unsafe {
        assert!(
            module >= 0 && (module as usize) < registry::count(),
            "tls_get: invalid module handle (caller bug)"
        );
        let s = sched();
        let p = s.tcbs[s.current].tls[module as usize];
        assert!(!p.is_null(), "tls_get: module has no TLS (caller bug)");
        p
    })
}

/// Push a thread-exit destructor onto the calling thread's LIFO stack.
/// 0, or ERR_AGAIN when all `ATEXIT_SLOTS` are taken -- loudly honest.
pub fn atexit_push(f: AtexitFn, arg: *mut c_void) -> i32 {
    with_cs(|| unsafe {
        let s = sched();
        let t = &mut s.tcbs[s.current];
        if t.atexit_n as usize == ATEXIT_SLOTS {
            return ERR_AGAIN;
        }
        t.atexit[t.atexit_n as usize] = (Some(f), arg);
        t.atexit_n += 1;
        0
    })
}

/// The calling thread's tid.
pub fn self_tid() -> i32 {
    with_cs(|| unsafe {
        let s = sched();
        s.tcbs[s.current].tid
    })
}

/// A live thread's stack carve by tid: `(base, len)` exactly as
/// spawned. Lookup is by tid, never slot, so a reused slot cannot
/// alias a dead thread; Exited-unreaped threads are not live.
pub fn stack_bounds_by_tid(tid: i32) -> Option<(u32, u32)> {
    with_cs(|| unsafe {
        let s = sched();
        for t in s.tcbs.iter() {
            let live = t.state == State::Ready || t.state == State::Sleeping;
            if live && t.tid == tid {
                return Some((t.stack_base, t.stack_top - t.stack_base));
            }
        }
        None
    })
}



/// The C-ABI `rhu_ctx` (include/rhu.h) -- layout mirrored exactly.
/// sp is the post-pop stack pointer, a SNAPSHOT: refused on write by
/// set_context, ignored by the activation-return door.
#[repr(C)]
pub struct RhuCtx {
    pub r: [u32; 13],
    pub sp: u32,
    pub lr: u32,
    pub pc: u32,
    pub xpsr: u32,
    pub flags: u32,
}

/// EPSR ITSTATE bits inside xPSR ([26:25] + [15:10]): cleared when a
/// pc edit lands -- resuming an edited pc into a live IT block would
/// be unpredictable, so the safe form is the only form.
const ITSTATE_MASK: u32 = 0x0600_FC00;

/// Parse a switched-out thread's saved frames (sw: r4-r11 +
/// EXC_RETURN; optional s16-s31; hw: r0-r3, r12, lr, pc, xpsr) into a
/// ctx. The post-pop sp honors the frame type AND the stacked-aligner
/// bit (xpsr[9]). Sound for every non-current live thread: PendSV's
/// FP store completes lazy stacking before any frame is saved, so
/// frames are always fully materialized.
unsafe fn fill_ctx(sp: u32) -> RhuCtx { unsafe {
    let sw = sp as *const u32;
    let exc = sw.add(8).read();
    let ext = exc & 0x10 == 0;
    let hw_off = 36 + if ext { 64 } else { 0 };
    let hw = (sp + hw_off) as *const u32;
    let xpsr = hw.add(7).read();
    let post_pop =
        sp + hw_off + if ext { 104 } else { 32 } + if xpsr & (1 << 9) != 0 { 4 } else { 0 };
    let mut r = [0u32; 13];
    for k in 0..4 {
        r[k] = hw.add(k).read();
    }
    for k in 0..8 {
        r[4 + k] = sw.add(k).read();
    }
    r[12] = hw.add(4).read();
    RhuCtx {
        r,
        sp: post_pop,
        lr: hw.add(5).read(),
        pc: hw.add(6).read(),
        xpsr,
        flags: ext as u32,
    }
}}

/// Apply a ctx back into the saved frames -- ONE mechanism, two
/// entrances (set_context and the activation return), both landing in
/// the scheduler-owned context. Writes r0-r12 and lr; pc is written
/// bit-0-cleared (the Thumb bit lives in xPSR.T) and a pc EDIT clears
/// the restored ITSTATE; xpsr is otherwise never written; sp,
/// EXC_RETURN, and geometry never move.
unsafe fn apply_ctx(frames_sp: u32, c: &RhuCtx) { unsafe {
    let sw = frames_sp as *mut u32;
    let exc = sw.add(8).read();
    let ext = exc & 0x10 == 0;
    let hw = (frames_sp + 36 + if ext { 64 } else { 0 }) as *mut u32;
    for k in 0..8 {
        sw.add(k).write(c.r[4 + k]);
    }
    for k in 0..4 {
        hw.add(k).write(c.r[k]);
    }
    hw.add(4).write(c.r[12]);
    hw.add(5).write(c.lr);
    let new_pc = c.pc & !1;
    if new_pc != hw.add(6).read() & !1 {
        hw.add(6).write(new_pc);
        let x = hw.add(7).read();
        hw.add(7).write(x & !ITSTATE_MASK);
    }
}}

/// Live-slot lookup by tid (Ready|Sleeping -- suspension is a flag,
/// not a state, so suspended threads are still live and addressable).
fn find_live(s: &Sched, tid: i32) -> Option<usize> {
    (0..MAX_THREADS).find(|&i| {
        let t = &s.tcbs[i];
        (t.state == State::Ready || t.state == State::Sleeping) && t.tid == tid
    })
}

/// Suspend by tid and fill *out (non-null, checked at the C door).
/// Refusals per rhu.h: ESRCH unknown/dead, EINVAL self or idle,
/// EAGAIN already suspended. No pend: the target is switched out
/// already (single core), removing it from pickability changes
/// nothing for the running thread.
pub fn suspend_by_tid(tid: i32, out: *mut RhuCtx) -> i32 {
    with_cs(|| unsafe {
        let s = sched();
        let Some(i) = find_live(s, tid) else {
            return ERR_SRCH;
        };
        if i == s.current || i as u32 == IDLE_SLOT.load(Ordering::Relaxed) {
            return ERR_INVAL;
        }
        if s.tcbs[i].suspended {
            return ERR_AGAIN;
        }
        out.write(fill_ctx(s.tcbs[i].sp));
        s.tcbs[i].suspended = true;
        sync_bits(s, i);
        0
    })
}

/// Edit a SUSPENDED thread's saved context in place (registers and pc,
/// never geometry): ctx->sp must equal the snapshot's or the whole edit is
/// refused writing nothing.
pub fn set_context_by_tid(tid: i32, ctx: *const RhuCtx) -> i32 {
    with_cs(|| unsafe {
        let s = sched();
        let Some(i) = find_live(s, tid) else {
            return ERR_SRCH;
        };
        if !s.tcbs[i].suspended {
            return ERR_AGAIN;
        }
        let c = &*ctx;
        if c.sp != fill_ctx(s.tcbs[i].sp).sp {
            return ERR_INVAL;
        }
        apply_ctx(s.tcbs[i].sp, c);
        0
    })
}

/// The send/resume break arm: ready a Sleeping target for delivery,
/// latching the futex-interruption discriminant when it was a waiter
/// (wait_addr stays SET -- the exit path's three-way tell).
unsafe fn break_wait(s: &mut Sched, i: usize) {
    if s.tcbs[i].wait_addr != 0 {
        s.tcbs[i].act_interrupted = true;
    }
    set_state(s, i, State::Ready);
}

/// Resume by tid. A pending activation deferred by suspension makes a
/// still-Sleeping target's wait break NOW; pends iff the target is
/// now pickable and equal-or-more urgent than the caller.
pub fn resume_by_tid(tid: i32) -> i32 {
    let (r, urgent) = with_cs(|| unsafe {
        let s = sched();
        let Some(i) = find_live(s, tid) else {
            return (ERR_SRCH, false);
        };
        if !s.tcbs[i].suspended {
            return (ERR_AGAIN, false);
        }
        s.tcbs[i].suspended = false;
        sync_bits(s, i);
        if s.tcbs[i].act_pending && s.tcbs[i].state == State::Sleeping {
            break_wait(s, i);
        }
        let urgent =
            s.tcbs[i].state == State::Ready && s.tcbs[i].prio <= s.tcbs[s.current].prio;
        (0, urgent)
    });
    if urgent {
        pend_switch();
    }
    r
}

/// Register THE process-global activation handler, once (CAS is the
/// once-ness; a second registration is refused EINVAL).
pub fn act_register(handler: usize) -> i32 {
    if ACT_HANDLER
        .compare_exchange(0, handler, Ordering::AcqRel, Ordering::Acquire)
        .is_ok()
    {
        0
    } else {
        ERR_INVAL
    }
}

/// Mark an activation pending (the flag IS the queue -- sends
/// coalesce). Breaks an unsuspended Sleeping target's wait; a
/// suspended target latches only (delivery waits for resume). Pends
/// for PEER targets only: a self-send's delivery point is the
/// sender's own next scheduling-out, and pending a switch for it
/// would buy nothing.
pub fn act_send(tid: i32) -> i32 {
    let (r, urgent) = with_cs(|| unsafe {
        let s = sched();
        let Some(i) = find_live(s, tid) else {
            return (ERR_SRCH, false);
        };
        if i as u32 == IDLE_SLOT.load(Ordering::Relaxed) {
            return (ERR_INVAL, false);
        }
        if ACT_HANDLER.load(Ordering::Relaxed) == 0 {
            return (ERR_INVAL, false);
        }
        s.tcbs[i].act_pending = true;
        if !s.tcbs[i].suspended && s.tcbs[i].state == State::Sleeping {
            break_wait(s, i);
        }
        let urgent = i != s.current
            && !s.tcbs[i].suspended
            && s.tcbs[i].state == State::Ready
            && s.tcbs[i].prio <= s.tcbs[s.current].prio;
        (0, urgent)
    });
    if urgent {
        pend_switch();
    }
    r
}

/// The activation trampoline: entered by exception return on the
/// TARGET's stack with &ctx in r0; calls the handler, then SVC #1
/// hands the (possibly edited) ctx back to the kernel for the
/// restore. Never returns -- the kernel rebuilds or restores.
extern "C" fn act_tramp(ctx: *mut RhuCtx) -> ! {
    let h = ACT_HANDLER.load(Ordering::Relaxed);
    assert!(h != 0, "activation: delivery without a handler");
    let f: extern "C" fn(*mut RhuCtx) = unsafe { core::mem::transmute(h) };
    f(ctx);
    unsafe { core::arch::asm!("svc #1", options(noreturn)) }
}

/// Build the canonical trampoline frame pair below `ctx_base`: a
/// basic hw frame (r0 = &ctx, pc = act_tramp, Thumb xPSR) and a sw
/// frame (zeros + thread-mode/PSP/basic EXC_RETURN). Returns the new
/// sp. The trampoline starts clean-integer; the interrupted FP state
/// rides the ORIGINAL frames untouched.
unsafe fn build_tramp_frames(ctx_base: u32) -> u32 { unsafe {
    let fhw = (ctx_base - 32) as *mut u32;
    fhw.add(0).write(ctx_base);
    fhw.add(1).write(0);
    fhw.add(2).write(0);
    fhw.add(3).write(0);
    fhw.add(4).write(0);
    fhw.add(5).write(0);
    fhw.add(6).write(act_tramp as *const () as usize as u32 & !1);
    fhw.add(7).write(0x0100_0000);
    let new_sp = ctx_base - 68;
    let fsw = new_sp as *mut u32;
    for k in 0..8 {
        fsw.add(k).write(0);
    }
    fsw.add(8).write(0xFFFF_FFFD);
    new_sp
}}

/// The delivery transform, run in the switch path
/// on the PICKED thread: materialize the ctx onto the target's own
/// stack below its saved frames, then point its sp at a fresh frame
/// pair entering the trampoline. Cost: 72 (ctx) + 68 (frames) = 140
/// bytes, asserted against the stack base -- a budgeting bug is a
/// first-delivery fact, never silent corruption.
unsafe fn deliver_activation(s: &mut Sched, i: usize) { unsafe {
    let old_sp = s.tcbs[i].sp;
    let ctx_base = (old_sp - 72) & !7;
    let new_sp = ctx_base - 68;
    assert!(
        new_sp >= s.tcbs[i].stack_base,
        "activation: tid {} has {} bytes of stack below its saved frame, a delivery needs 140",
        s.tcbs[i].tid,
        old_sp.wrapping_sub(s.tcbs[i].stack_base)
    );
    (ctx_base as *mut RhuCtx).write(fill_ctx(old_sp));
    let built = build_tramp_frames(ctx_base);
    s.tcbs[i].act_saved_sp = old_sp;
    s.tcbs[i].act_in_handler = true;
    s.tcbs[i].act_pending = false;
    s.tcbs[i].sp = built;
}}

/// SVC #1 (the reserved activation-resume immediate):
/// re-deliver if a send landed during the handler (same ctx storage,
/// the cost never nests), else apply the possibly-edited ctx to the
/// original frames (activation door: ctx.sp ignored) and restore.
/// Returns the psp to pop from; the asm arm pops the PendSV shape. The
/// arm completes its own frame's pending lazy floating-point state
/// BEFORE calling here (the frame-type note above the seam).
#[unsafe(no_mangle)]
extern "C" fn __rhu_act_resume(_psp: u32) -> u32 {
    with_cs(|| unsafe {
        let s = sched();
        let i = s.current;
        assert!(s.tcbs[i].act_in_handler, "activation: stray SVC #1");
        let saved = s.tcbs[i].act_saved_sp;
        let ctx_base = (saved - 72) & !7;
        if s.tcbs[i].act_pending {
            s.tcbs[i].act_pending = false;
            let new_sp = build_tramp_frames(ctx_base);
            s.tcbs[i].sp = new_sp;
            new_sp
        } else {
            apply_ctx(saved, &*(ctx_base as *const RhuCtx));
            s.tcbs[i].act_in_handler = false;
            s.tcbs[i].sp = saved;
            saved
        }
    })
}

/// Idle's tid, for the api smoke's refusal matrix ONLY (see
/// `probe_futex_waiting` for the probe rules).
#[cfg(feature = "api-smoke")]
pub fn probe_idle_tid() -> i32 {
    with_cs(|| unsafe {
        let s = sched();
        s.tcbs[IDLE_SLOT.load(Ordering::Relaxed) as usize].tid
    })
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

/// (live, free) slot counts -- the thread-abi exhaustion arm derives
/// its expected spawn count from these and asserts no Exited residue
/// first (see `probe_futex_waiting` for the probe rules).
#[cfg(feature = "api-smoke")]
pub fn probe_slot_states() -> (u32, u32) {
    with_cs(|| unsafe {
        let s = sched();
        let mut live = 0;
        let mut free = 0;
        for t in s.tcbs.iter() {
            match t.state {
                State::Ready | State::Sleeping => live += 1,
                State::Free => free += 1,
                State::Exited => {}
            }
        }
        (live, free)
    })
}

/// (retired-list occupancy, leak drops, leaked bytes) -- the smoke's
/// algorithmic leak-audit reads.
#[cfg(feature = "api-smoke")]
pub fn probe_retired() -> (u32, u32, u32) {
    with_cs(|| unsafe {
        let s = sched();
        (s.retired_n, s.leak_drops, s.leak_bytes)
    })
}

/// The stack pool's bump, bytes -- the smoke's "a refusal moved
/// nothing" read.
#[cfg(feature = "api-smoke")]
pub fn probe_pool_used() -> u32 {
    with_cs(|| unsafe { sched().pool_used as u32 })
}

/// A live thread's name as the record reads it: the pair proven to
/// point into the thread's own control block (else None), its bytes
/// copied out under the CS, the kept count returned.
#[cfg(feature = "api-smoke")]
pub fn probe_thread_name(tid: i32, out: &mut [u8; NAME_BYTES]) -> Option<usize> {
    with_cs(|| unsafe {
        let s = sched();
        let i = find_live(s, tid)?;
        let t = &s.tcbs[i];
        if t.name.ptr != addr_of!(t.name_buf) as *const u8 || t.name.len >= NAME_BYTES {
            return None;
        }
        *out = t.name_buf;
        Some(t.name.len)
    })
}

/// Seek the thread-id counter to `next` (the id the next spawn takes),
/// for the api smoke's id-space arm ONLY: the refusal at the end of the
/// id space cannot be reached by spawning. Forward only -- the
/// monotonic law holds inside the smoke too.
#[cfg(feature = "api-smoke")]
pub fn probe_tid_seek(next: u32) {
    with_cs(|| {
        let cur = NEXT_TID.load(Ordering::Relaxed);
        assert!(next >= cur, "tid seek backwards (probe bug)");
        NEXT_TID.store(next, Ordering::Relaxed);
    })
}

/// The id the next spawn takes (see `probe_tid_seek`).
#[cfg(feature = "api-smoke")]
pub fn probe_next_tid() -> u32 {
    NEXT_TID.load(Ordering::Relaxed)
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
        let t = &s.tcbs[next];
        if t.act_pending
            && !t.act_in_handler
            && !t.suspended
            && next as u32 != IDLE_SLOT.load(Ordering::Relaxed)
        {
            deliver_activation(s, next);
        }
        #[cfg(armv8m)]
        core::arch::asm!("msr PSPLIM, {}", in(reg) s.tcbs[next].stack_base, options(nomem, nostack));
        s.tcbs[next].sp
    })
}

#[unsafe(no_mangle)]
extern "C" fn __rhu_first_sp() -> u32 {
    with_cs(|| unsafe {
        let s = sched();
        s.started = true;
        #[cfg(armv8m)]
        core::arch::asm!("msr PSPLIM, {}", in(reg) s.tcbs[s.current].stack_base, options(nomem, nostack));
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
    if board::CYCLE_COUNTER {
        let w = &mut *core::ptr::addr_of_mut!(TICK_WITNESS);
        if w.source == WITNESS_DWT {
            let cyc = read_volatile(DWT_CYCCNT);
            if w.entries != 0 {
                let elapsed = cyc.wrapping_sub(w.prev);
                if elapsed > w.max {
                    w.max = elapsed;
                }
                if elapsed > w.period + w.period / 2 {
                    w.late += 1;
                    w.lost += (elapsed + w.period / 2) / w.period - 1;
                }
            }
            w.prev = cyc;
            w.entries = w.entries.wrapping_add(1);
        }
    }
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
        if wake::expired(now, s.tcbs[i].wake_tick) {
            let now64 = ((TICKS_HI.load(Ordering::Relaxed) as u64) << 32) | now as u64;
            if s.tcbs[i].wake_far > now64 {
                s.tcbs[i].wake_tick = wake::arm(now64, s.tcbs[i].wake_far).0;
                continue;
            }
            set_state(s, i, State::Ready);
            if !s.tcbs[i].suspended {
                fired = true;
            }
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
        #[cfg(armv8m)]
        core::arch::asm!("msr MSPLIM, {}", in(reg) board::arena_window().1 as u32, options(nomem, nostack));
    }
    if board::CYCLE_COUNTER {
        unsafe {
            write_volatile(DEMCR, read_volatile(DEMCR) | DEMCR_TRCENA);
            write_volatile(DWT_LAR, DWT_LAR_UNLOCK);
            write_volatile(DWT_CYCCNT, 0);
            write_volatile(DWT_CTRL, read_volatile(DWT_CTRL) | DWT_CTRL_CYCCNTENA);
            let first = read_volatile(DWT_CYCCNT);
            let mut last = first;
            for _ in 0..16 {
                last = read_volatile(DWT_CYCCNT);
            }
            let w = &mut *core::ptr::addr_of_mut!(TICK_WITNESS);
            w.period = board::SYSCLK_HZ / board::TICK_HZ;
            w.source = if last != first { WITNESS_DWT } else { WITNESS_STATIC };
        }
    }
    let idle = spawn("idle", idle_entry, 0, 0xFF, 1024);
    IDLE_SLOT.store(idle as u32, Ordering::Relaxed);
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
         #1  activation resume (from the trampoline)
       Anything else is unexpected here and takes the terminal funnel. */
    tst   lr, #4
    ite   eq
    mrseq r0, msp
    mrsne r0, psp
    ldr   r1, [r0, #24]       /* stacked PC */
    subs  r1, r1, #2
    ldrb  r1, [r1]
    cbz   r1, 1f
    cmp   r1, #1
    beq   2f
    b     ExceptionTrampoline

1:  /* First start: reset MSP to the top (the boot-thread frame below is
       abandoned; MSP is the handler stack from here on), then
       exception-return into the first thread via its prebuilt frames.
       No delivery rides this path: nothing can be pending before the
       scheduler starts. */
    bl    __rhu_msp_top
    msr   msp, r0
    bl    __rhu_first_sp
    ldmia r0!, {{r4-r11, lr}}
    msr   psp, r0
    bx    lr

2:  /* Activation resume. The handler ran on the target's stack; when it
       executed a floating-point instruction this SVC's own frame is
       extended (EXC_RETURN bit 4 clear) with its lazy s0-s15 save still
       pending, aimed at the floating-point area of that frame -- dead
       space once the original frames are popped below. Complete it
       here, while the frame is live, so the pop never abandons a frame
       with lazy state armed (the discipline the context switch keeps):
       one floating-point instruction does it. The kernel half then
       decides re-delivery vs final restore and returns the psp to pop;
       the pop is the PendSV restore shape verbatim. */
    tst   lr, #0x10
    it    eq
    vmoveq.f32 s0, s0
    bl    __rhu_act_resume
    ldmia r0!, {{r4-r11, lr}}
    tst   lr, #0x10
    it    eq
    vldmiaeq r0!, {{s16-s31}}
    msr   psp, r0
    bx    lr
    .size SvcHandler, . - SvcHandler

    .section .text.PendSvHandler, "ax", %progbits
    .global PendSvHandler
    .type PendSvHandler, %function
    .thumb_func
PendSvHandler:
    /* The switch-entry witness (LazyFpWitness): an entry whose own frame
       is basic -- the outgoing thread holds no floating-point context --
       must find no lazy state pending (FPCCR.LSPACT clear); one that
       does records the count, the pending save's address (FPCAR) and
       the outgoing psp, and continues. r1-r3 are the entry's scratch
       registers (stacked by the hardware, restored at the return). */
    tst   lr, #0x10
    beq   9f
    movw  r1, #0xef34
    movt  r1, #0xe000
    ldr   r2, [r1]
    tst   r2, #1
    beq   9f
    movw  r3, #:lower16:__rhu_lazy_fp_witness
    movt  r3, #:upper16:__rhu_lazy_fp_witness
    ldr   r2, [r3]
    adds  r2, #1
    str   r2, [r3]
    ldr   r2, [r1, #4]
    str   r2, [r3, #4]
    mrs   r2, psp
    str   r2, [r3, #8]
9:  /* Save the outgoing thread: s16-s31 only if its frame is extended
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

/// Move the CALLING thread to `prio` and re-run the pick before
/// returning: a thread that became less urgent yields at once to every
/// Ready peer now equal or more urgent than it (round-robin puts the
/// caller last among equals), and a thread that became more urgent
/// simply continues. Returns the priority the caller held. The one
/// consumer is the hosting seam: the thread that runs a payload's entry
/// joins the hosted band for the entry's life and takes its own urgency
/// back when the entry returns (payload.rs); the api smoke exercises the
/// same door. No other kernel thread changes priority, and no hosted
/// thread reaches this -- there is no C door to it.
#[cfg(any(feature = "payload", feature = "api-smoke"))]
pub fn set_current_prio(prio: u8) -> u8 {
    let prev = with_cs(|| unsafe {
        let s = sched();
        let cur = s.current;
        let prev = s.tcbs[cur].prio;
        s.tcbs[cur].prio = prio;
        prev
    });
    pend_switch();
    prev
}

/// The calling thread's priority as the scheduler holds it, for the api
/// smoke's host-band check ONLY (see `probe_futex_waiting` for the probe
/// rules).
#[cfg(feature = "api-smoke")]
pub fn probe_current_prio() -> u8 {
    with_cs(|| unsafe {
        let s = sched();
        s.tcbs[s.current].prio
    })
}

/// The switch-entry witness's three words (count, the last pending
/// save's address, the last outgoing psp), for the api smoke's
/// act-lazy-fp check ONLY (see `probe_futex_waiting` for the probe
/// rules).
#[cfg(feature = "api-smoke")]
pub fn probe_lazy_fp_witness() -> (u32, u32, u32) {
    (
        LAZY_FP_WITNESS.count.load(Ordering::Acquire),
        LAZY_FP_WITNESS.fpcar.load(Ordering::Acquire),
        LAZY_FP_WITNESS.psp.load(Ordering::Acquire),
    )
}
