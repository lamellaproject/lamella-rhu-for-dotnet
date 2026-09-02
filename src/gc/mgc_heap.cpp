/* mgc_heap.cpp -- the mgc allocator: the slow path, then the sweep's
 * free list and the alloc-context windows behind two build knobs.
 *
 * The collector core behind the interface skeleton, first layer: a
 * bump allocator over ONE region drawn through the shim's audited
 * mmap surface (arena-audit accounting stays unified -- the built-in
 * GC's multi-megabyte reservations never happen on the standalone
 * route). Without MGC_COLLECT_LIVE no collection exists:
 * IGCHeap::GarbageCollect stays a loud fail-fast and region
 * exhaustion returns NULL, which the EE turns into a managed
 * OutOfMemoryException (a bounded shape: exit 134).
 *
 * Contract facts this file is built against (the runtime's own
 * allocation path; the gate asserts every one on the wire):
 *  - Slow chain: RhpNewFast(miss)/RhpNew*Align8/RhpNewFinalizable ->
 *    RhpGcAlloc -> GcAllocInternal -> IGCHeap::Alloc(acontext,size,
 *    flags). GcAllocInternal STORES THE MethodTable* AND numComponents
 *    ITSELF after Alloc returns -- Alloc's only job is zeroed, aligned,
 *    header-slot-preceded memory of `size` bytes.
 *  - acontext == pThread+4 (Thread's first member is ee_alloc_context
 *    {combined_limit@0, gc_alloc_context@4}); GetAllocContext() also
 *    returns tls_CurrentThread+4 -- the identity probe below checks
 *    this live on every call.
 *  - combined_limit is refreshed BY GcAllocInternal after EVERY Alloc
 *    return: = alloc_limit (sampling off) or alloc_ptr+min(window,
 *    sampling distance). So windows written into acontext DO engage
 *    the EE fast path -- the MGC_PROBE_WINDOW build watches it happen.
 *  - Object birth: result = old alloc_ptr, MT at [result], bump by
 *    size; the -4 ObjHeader is never touched -> region PRE-ZEROED
 *    (shim mmap delivers zeroed arena pages) with 4-byte plug skew
 *    (first object at region+4); each object's size-quantum tail
 *    hosts the NEXT object's header.
 *  - ALIGN8 (flags 8) / ALIGN8_BIAS (flags 4: MT slot at 4 mod 8 so
 *    the f64 payload lands 8-aligned) are ALWAYS slow-path on ARM32:
 *    honored here by parity pads = 12-byte free objects (12 mod 8 = 4
 *    flips parity AND keeps the heap walkable -- a bare 4-byte gap
 *    could hold no free object).
 *  - GC_ALLOC_FINALIZE registration is the GC's duty (GcAllocInternal
 *    does none) -> the registration list here; without MGC_SWEEP_LIVE
 *    no finalizer ever runs.
 *  - THE WRITE-BARRIER STOMP: StompWriteBarrier(Initialize) writes all
 *    five globals in one call (card_table, lowest, highest,
 *    ephemeral_low, ephemeral_high). ephemeral_low=~0 silences
 *    RhpAssignRef/RhpCheckedAssignRef, BUT RhBulkMoveWithWriteBarrier
 *    (Array.Copy/List growth) card-marks the WHOLE destination range
 *    gated ONLY by the bounds test -- no ephemeral test. The card
 *    table therefore must be a REAL writable buffer, pre-biased the
 *    way the barriers index it: effective byte = card_table[dst>>10].
 *    The buffer is biased against the arena floor and sized to cover
 *    the whole arena, so every possible in-bounds (and hypothetical
 *    unfiltered) mark lands harmlessly inside it.
 *
 * -DMGC_SWEEP_LIVE -- the free-list allocator behind the sweep:
 *  - FREE LIST: mgc_collect's sweep coalesces unmarked runs into free
 *    objects (free MT, numComponents = run-12) and hands them here;
 *    the list threads through the runs themselves ([q+8] = next, the
 *    first payload word) in address order. Alloc = first-fit from the
 *    list, bump-tail fallback. A run is taken only when the remainder
 *    is 0 or >= 12 (a 4/8-byte gap could hold no free object and
 *    would break the heap walk).
 *  - ZERO-ON-REUSE IS MANDATORY: a handed-out object's footprint is
 *    [result-4, result+size-4) (the -4 ObjHeader slot + MT slot +
 *    fields; the word at result+size-4 is the NEXT object's header
 *    and is never touched -- when the successor is live it may carry
 *    FINALIZER_RUN state). Dead bytes are garbage; the birth protocol
 *    (GcAllocInternal stores MT into pre-zeroed memory) needs zero.
 *    The bump tail stays virgin-zeroed: sweep NEVER retracts g_ptr
 *    (retracting over dead garbage would break the bump path's
 *    pre-zero assumption).
 *  - HEADROOM RESERVE: the unhandled-OOM path is not allocation-free
 *    -- at a 100%-full wall the dispatch itself allocates, every alloc
 *    returns NULL, the thread never parks and the next SuspendEE
 *    waits forever. So the bump path keeps the last MGC_OOM_RESERVE
 *    bytes untouchable in normal mode: when even a collect-and-retry
 *    cannot fit, OOM is reported (NULL, the managed protocol) while
 *    the reserve still exists, and the reserve OPENS (grace mode) to
 *    feed exception-dispatch allocations. Grace closes when a sweep
 *    recovers a reserve's worth of slack; a reserve exhausted
 *    mid-dispatch fail-fasts.
 *  - COLLECT-THEN-RETRY lives HERE: GcAllocInternal does NOT retry a
 *    NULL Alloc (it makes exactly 3 vcalls), so the wall collects and
 *    retries before reporting OOM. Calling mgc_heap_collect from
 *    inside Alloc is safe: RhpGcAlloc parked a deferred frame (the
 *    same shape as RhCollect), and the alloc lock is RELEASED around
 *    the collection.
 *  - THE ALLOC LOCK: slow-path-only means concurrent Alloc entries are
 *    real (worker threads allocate); the runtime's own collector
 *    serializes inside its Alloc, so mgc must too -- suspension is the
 *    heap lock for alloc-vs-collector, not for alloc-vs-alloc.
 *    Contended waiters spin in PREEMPTIVE mode (EnablePreemptiveGC ->
 *    yield -> Disable), so a spinner can never deadlock a SuspendEE;
 *    consequently the lock is provably FREE while the world is
 *    stopped and the collector's sweep/partition never needs it.
 *  - finq: the registration list is CFinalize-shaped -- one array,
 *    per-slot state {FREE, REG, FREACH, FREACH_CRIT}. mgc_collect's
 *    ScanForFinalization partition moves dead entries (EagerFinalized
 *    first, then FINALIZER_RUN -> drop+ClrBit, else promote; critical
 *    finalizers drain AFTER normal ones). GetNextFinalizable pops here
 *    (suspension-serialized against the partition: the drain runs
 *    cooperative).
 *
 * -DMGC_TUNE_LIVE -- alloc-context bump WINDOWS, the runtime's own
 * fix_allocation_context shape:
 *  - THE +12 PLUG RESERVE: the plug writes the free MT at alloc_ptr
 *    with numComponents = (alloc_limit - alloc_ptr) -- a free object
 *    spanning [ptr, limit+12). alloc_limit is therefore UNDER-reported
 *    by exactly one min-object: the true reservation extends 12 bytes
 *    past the limit, so ANY remainder (including 0) is pluggable and
 *    the heap stays walkable. Every window reserves size+window+12.
 *  - SERVE-FROM-WINDOW-HEAD: an eligible slow-path call (flags other
 *    than CONTAINS_REF all clear) is served at the window base;
 *    alloc_ptr = result+size, alloc_limit = result+size+window.
 *    GcAllocInternal refreshes combined_limit from alloc_limit after
 *    Alloc returns, so subsequent small allocations take RhpNewFast
 *    and never reach this file until the window misses.
 *  - WINDOWS RIDE BOTH SOURCES: bump-tail windows are virgin
 *    (pre-zeroed, no memset); free-run windows are adaptively sized
 *    (floor MGC_WINDOW_FLOOR, remainder rule 0-or->=12 with sub-12
 *    remainders absorbed into the window) and ZEROED at handout --
 *    the fast path stores MTs into what must be zero. Without the
 *    free-list source, windows would die after the first wall
 *    (g_ptr never retracts; steady-state allocation is free-list).
 *  - ACCOUNTING: the full window span is charged to alloc_bytes at
 *    handout; the unused remainder is REFUNDED at fix time.
 *  - FIX SITES: GcEnumAllocContexts (every collection, world
 *    stopped), IGCHeap::FixAllocContext (thread detach), and the alloc
 *    path itself when a context with an outstanding window comes back
 *    for a slow call (the EE noticed a miss; the old remainder becomes
 *    a plug). Fix counts the objects born in [window base, alloc_ptr)
 *    into the walk model and plugs [alloc_ptr, limit+12).
 *  - LOCKING: the fix mutates heap state, so it takes the alloc lock
 *    with a COOPERATIVE spin (plain yield, no EnablePreemptiveGC --
 *    the detach caller's transition-frame status is unknown, and a
 *    preemptive transition over a stale frame hands GcScanRoots a
 *    garbage walk). Deadlock-free: no holder ever blocks on
 *    suspension while holding (the wall path releases around its
 *    collection), so a cooperative spinner always sees the lock
 *    freed. During a collection the lock is provably free, so the
 *    GcEnumAllocContexts site never spins at all.
 *  - Negative control (-DMGC_BREAK_WINDOW_FIX): the plug+count are
 *    suppressed (contexts still cleared). The un-plugged window gap
 *    makes the very next heap walk read a NULL MethodTable -> the
 *    'heap walk CORRUPT' fail-fast is the deterministic detector --
 *    proving the walk cross-check genuinely guards the window seam.
 */

#include "mgc_env.h"
#include "gcinterface.h"
#include "gcenv.ee.h"     /* flat GCToEEInterface -- StompWriteBarrier lives here */
#include "gcenv.object.h" /* MethodTable mirror for the free-object gap fill */

#include <stdio.h>
#include <string.h>

extern "C" void abort(void);
/* The shim's glibc-ABI mmap surface (the host shim's mmap64 ->
 * the kernel arena): anonymous mappings come from the kernel arena,
 * page-aligned and ZEROED, and are counted in the arena RA-audit. */
extern "C" void* mmap64(void* addr, size_t len, int prot, int flags,
                        int fd, uint64_t off);
#define GLIBC_PROT_RW       0x3  /* PROT_READ|PROT_WRITE */
#define GLIBC_MAP_ANON_PRIV 0x22 /* MAP_PRIVATE|MAP_ANONYMOUS */

/* Handle-pool / frozen-segment recap, implemented by the skeleton TU
 * (only ever linked together with it in MGC_HEAP_LIVE images). */
extern "C" void mgc_skel_census(void);

#define HPLOG(fmt, ...) \
    do { printf("[mgc-heap] " fmt "\n", ##__VA_ARGS__); fflush(stdout); } while (0)
#define PRLOG(fmt, ...) \
    do { printf("[mgc-probe] " fmt "\n", ##__VA_ARGS__); fflush(stdout); } while (0)

static void heap_failfast(const char* what)
{
    printf("[mgc-heap] FAIL-FAST: %s\n", what);
    fflush(stdout);
    abort();
}

/* ------------------------------------------------------------------ */
/* knobs                                                              */
/* ------------------------------------------------------------------ */

#ifndef MGC_HEAP_BYTES          /* region size: large enough for a real
                                 * payload's allocations, small enough to
                                 * hit a wall inside the 16 MB PSRAM budget */
#define MGC_HEAP_BYTES (8u * 1024u * 1024u)
#endif

#ifndef MGC_FINQ_CAP            /* finalize-registration list capacity */
#define MGC_FINQ_CAP 2048u
#endif

#ifndef MGC_PROBE_WINDOW_BYTES  /* -DMGC_PROBE_WINDOW experiment size */
#define MGC_PROBE_WINDOW_BYTES 512u
#endif

#ifndef MGC_OOM_RESERVE         /* headroom: report OOM while this much
                                 * virgin tail can still feed exception
                                 * dispatch */
#define MGC_OOM_RESERVE (64u * 1024u)
#endif

#ifndef MGC_WINDOW_BYTES        /* bump-window size handed to the EE
                                 * fast path (MGC_TUNE_LIVE) */
#define MGC_WINDOW_BYTES 4096u
#endif

#ifndef MGC_WINDOW_FLOOR        /* smallest window worth handing out
                                 * from a free run */
#define MGC_WINDOW_FLOOR 256u
#endif

#ifndef MGC_WALL_RETRIES        /* consecutive collect+fit failures
                                 * before the alloc path declares OOM
                                 * (the concurrent-wall race) */
#define MGC_WALL_RETRIES 4u
#endif

#ifndef MGC_ARENA_COVER_BYTES   /* span the card buffer covers from the
                                 * arena floor (power of two; the floor
                                 * mask derives from it). Default = the
                                 * QEMU 16 MB PSRAM arena; boards with
                                 * KB-class arenas shrink it (a knob-
                                 * table row) -- the init-time coverage
                                 * check makes an undersized value a
                                 * LOUD boot failfast, never a silent
                                 * out-of-buffer card write. */
#define MGC_ARENA_COVER_BYTES (16u * 1024u * 1024u)
#endif
static_assert((MGC_ARENA_COVER_BYTES & (MGC_ARENA_COVER_BYTES - 1u)) == 0u,
              "MGC_ARENA_COVER_BYTES must be a power of two (floor mask)");

#if defined(MGC_TUNE_LIVE) && !defined(MGC_SWEEP_LIVE)
#error "MGC_TUNE_LIVE requires MGC_SWEEP_LIVE (stage defines stack)"
#endif

#if defined(MGC_FRAG_CENSUS) && !defined(MGC_SWEEP_LIVE)
#error "MGC_FRAG_CENSUS requires MGC_SWEEP_LIVE (it reports free-list shape)"
#endif

#if defined(MGC_COMPACT) && !defined(MGC_SWEEP_LIVE)
#error "MGC_COMPACT requires MGC_SWEEP_LIVE (it replaces the sweep)"
#endif

/* ---- Multi-range coverage: the stomped bounds are the UNION of the
 * ranges (the runtime's conservative enumeration filters candidates
 * by g_lowest/g_highest, so the bounds must cover every range).
 * Range 0 = the g_region cells (primary); range 1 is additive. The
 * gap between them is UNTABLED poisoned memory, verified every
 * collection. ---- */
#ifdef MGC_MULTIRANGE
#ifndef MGC_HEAP2_BYTES
#error "MGC_MULTIRANGE needs MGC_HEAP2_BYTES (the secondary range's size)"
#endif
#ifndef MGC_RANGE_GAP_BYTES     /* deliberately untabled inter-range gap */
#define MGC_RANGE_GAP_BYTES (64u * 1024u)
#endif
#define MGC_GAP_POISON 0xA5u
#ifndef MGC_SWEEP_LIVE
#error "MGC_MULTIRANGE requires MGC_SWEEP_LIVE"
#endif
/* MGC_COMPACT x MGC_MULTIRANGE = the EVACUATION COMPOSE:
 * compacting collections EVACUATE one range's movable survivors into
 * the other range's fresh bump tail (order-preserving, anchor-replay
 * forwarding -- no zones needed: source gaps are sums of >=12-byte
 * object steps, dest is virgin). Under the split-stomp posture the
 * GC-time bounds are the union, conservative sightings arrive as
 * pins, and a pin-honoring mover is safe by construction. */
#endif /* MGC_MULTIRANGE */
#if defined(MGC_MR_SPLIT_STOMP) && !defined(MGC_MULTIRANGE)
#error "MGC_MR_SPLIT_STOMP is a multi-range posture (needs MGC_MULTIRANGE)"
#endif

/* ------------------------------------------------------------------ */
/* state                                                              */
/* ------------------------------------------------------------------ */

static uint8_t* g_region;       /* mmap base (page-aligned, zeroed) */
static uint8_t* g_region_end;
static uint8_t* g_ptr;          /* next object address; starts region+4 */
static MethodTable* g_free_mt;  /* identified by POINTER */
static int      g_stomped;
static int      g_oom;

#ifdef MGC_MULTIRANGE
/* Range 1 (secondary) + the untabled gap. g_region/g_region_end/g_ptr
 * stay RANGE 0's truth -- every single-range code path keeps its exact
 * meaning; range-1 handling is added beside it, never woven through. */
static uint8_t* g_r1;           /* range 1 base */
static uint8_t* g_r1_end;
static uint8_t* g_r1_ptr;       /* range 1 bump; starts base+4 */
static uint8_t* g_gap_lo;       /* poisoned, untabled, verified */
static uint8_t* g_gap_hi;
#ifdef MGC_MR_SPLIT_STOMP
static uint32_t* g_mr_card_biased;  /* saved for the GC-entry re-stomp */
#endif
#endif

/* Card buffer: one byte per 1 KB card across MGC_ARENA_COVER_BYTES
 * (16 K cards at the QEMU default) + slack for the bulk barrier's
 * end-rounding. Biased pointer computed at init; coverage is
 * machine-checked there (the coverage guard). */
static uint8_t  g_card_buf[MGC_ARENA_COVER_BYTES / 1024u + 64u];

/* census */
static uint32_t g_allocs, g_pads, g_null_returns;
static uint32_t g_bytes;        /* bump-consumed bytes incl. pads */
static uint32_t g_max_size, g_max_size_flags, g_max_size_alloc;
static uint32_t g_fl_finalize, g_fl_containsref, g_fl_align8,
                g_fl_bias, g_fl_zopt, g_fl_loh, g_fl_poh;
static uint32_t g_seen_flags[16]; static uint32_t g_seen_flags_n;

/* finalize registration. Without MGC_SWEEP_LIVE no finalizer runs
 * and the list is a census plus a root class; under MGC_SWEEP_LIVE the
 * same array is CFinalize-shaped: per-slot states, partitioned by the
 * death pipeline, drained by the finalizer thread. */
static Object*  g_finq[MGC_FINQ_CAP];
static uint32_t g_finq_n, g_finq_lost;

#ifdef MGC_SWEEP_LIVE
#define FQ_FREE        0u
#define FQ_REG         1u
#define FQ_FREACH      2u   /* dead, finalizer pending (normal) */
#define FQ_FREACH_CRIT 3u   /* dead, critical finalizer -- drained after
                             * ALL normal ones (the runtime's guarantee) */
static uint8_t  g_finq_state[MGC_FINQ_CAP];
static uint32_t g_finq_live;        /* REG slots */
static uint32_t g_freach_n, g_freach_crit_n;
static uint32_t g_finq_reg_total;   /* monotonic (census) */
static uint32_t g_finq_rover;       /* append scan start */

/* free list (sweep-installed): address-ordered, threaded through the
 * free objects themselves -- MT@q, numComponents@q+4 (= run-12),
 * next@q+8. Runs are >=12 and ==0 (mod 4). */
static uint8_t* g_free_head;
static uint32_t g_free_runs, g_free_bytes;
static uint32_t g_walk_expected;    /* walkable objects the heap holds
                                     * (live + pads + free runs) -- the
                                     * collector's cross-check number */
static int      g_grace;            /* OOM grace: reserve open */
static uint32_t g_reuse_allocs, g_reuse_bytes, g_exact_fits, g_splits,
                g_grace_allocs, g_collect_retries, g_oom_reports;

/* the alloc lock (see header comment: alloc-vs-alloc serialization;
 * contended waiters spin PREEMPTIVE so SuspendEE never waits on them,
 * hence the lock is free whenever the world is stopped) */
static volatile unsigned char g_alloc_lock;
extern "C" int  sched_yield(void);
extern "C" int  mgc_heap_collect(int generation, int low_memory_p, int mode);
#ifdef MGC_COMPACT
extern "C" void mgc_compact_request(void);  /* compaction: next collect compacts */
#endif

static void alloc_lock_acquire(void)
{
    while (__atomic_test_and_set((void*)&g_alloc_lock, __ATOMIC_ACQUIRE)) {
        GCToEEInterface::EnablePreemptiveGC();
        while (__atomic_load_n(&g_alloc_lock, __ATOMIC_RELAXED))
            sched_yield();
        GCToEEInterface::DisablePreemptiveGC();
    }
}

static void alloc_lock_release(void)
{
    __atomic_clear((void*)&g_alloc_lock, __ATOMIC_RELEASE);
}

#ifdef MGC_TUNE_LIVE
/* Cooperative acquire for callers whose transition-frame status is
 * unknown (the detach-path FixAllocContext): a preemptive transition
 * over a stale frame would hand GcScanRoots a garbage walk, so the
 * preemptive spin is forbidden there. Deadlock-free: no holder ever
 * blocks on suspension while holding. */
static void alloc_lock_acquire_coop(void)
{
    while (__atomic_test_and_set((void*)&g_alloc_lock, __ATOMIC_ACQUIRE))
        sched_yield();
}

/* outstanding windows: one per context, small linear table */
struct WinRec {
    gc_alloc_context* ctx;
    uint8_t*          base;     /* first fast-path object address */
};
static WinRec  g_win_tab[16];
static uint32_t g_win_handed, g_win_from_runs, g_win_fixed,
                g_win_objs, g_win_consumed, g_win_refund,
                g_win_plugs, g_win_hw;   /* hw = peak outstanding */

static WinRec* win_lookup(gc_alloc_context* ctx)
{
    for (unsigned i = 0; i < 16; i++)
        if (g_win_tab[i].ctx == ctx)
            return &g_win_tab[i];
    return nullptr;
}
#endif /* MGC_TUNE_LIVE */
#endif /* MGC_SWEEP_LIVE */

/* identity probe: GetAllocContext() vs the acontext parameter,
 * compared INSIDE Alloc */
static gc_alloc_context* g_ctx_seen[16];
static uint32_t g_ctx_seen_n;
static uint32_t g_id_match, g_id_mismatch;

/* window probe, -DMGC_PROBE_WINDOW only */
#ifdef MGC_PROBE_WINDOW
static gc_alloc_context* g_win_ctx;
static uint8_t* g_win_base;     /* value written into alloc_ptr */
static uint8_t* g_win_limit;
static uint32_t g_win_alloc_no;
static int      g_win_state;    /* 0=not handed 1=handed 2=verdict logged */
#endif

/* ------------------------------------------------------------------ */
/* init + stomp                                                       */
/* ------------------------------------------------------------------ */

extern "C" void mgc_heap_init(void)
{
    if (g_region)
        heap_failfast("mgc_heap_init called twice");

#if defined(MGC_MULTIRANGE) && defined(MGC_HEAP2_BASE)
    /* The silicon shape: range 1 is a FIXED second bank (a second SRAM
     * domain on parts that have one) -- only range 0 is carved; the
     * inter-bank address space is not the heap's to poison (membership
     * stays exact per-range). */
    void* p = mmap64(nullptr, MGC_HEAP_BYTES, GLIBC_PROT_RW,
                     GLIBC_MAP_ANON_PRIV, -1, 0);
#elif defined(MGC_MULTIRANGE)
    /* ONE carve holds range0 + gap + range1 (the QEMU two-range model:
     * a deliberate address discontinuity inside one arena mapping; a
     * silicon map would table two real banks the same way). */
    void* p = mmap64(nullptr,
                     MGC_HEAP_BYTES + MGC_RANGE_GAP_BYTES + MGC_HEAP2_BYTES,
                     GLIBC_PROT_RW, GLIBC_MAP_ANON_PRIV, -1, 0);
#else
    void* p = mmap64(nullptr, MGC_HEAP_BYTES, GLIBC_PROT_RW,
                     GLIBC_MAP_ANON_PRIV, -1, 0);
#endif
    if (p == (void*)-1 || p == nullptr)
        heap_failfast("region mmap failed (see the arena audit above)");

    g_region     = (uint8_t*)p;
    g_region_end = g_region + MGC_HEAP_BYTES;
    g_ptr        = g_region + 4; /* plug skew: first object's -4 header
                                  * slot lives at region+0 */
    g_free_mt    = GCToEEInterface::GetFreeObjectMethodTable();

    HPLOG("region [%p,%p) %lu KB via shim mmap (pre-zeroed, plug skew +4, free MT=%p)",
          (void*)g_region, (void*)g_region_end,
          (unsigned long)(MGC_HEAP_BYTES / 1024u), (void*)g_free_mt);

#if defined(MGC_MULTIRANGE) && defined(MGC_HEAP2_BASE)
#ifdef MGC_HEAP2_CLKEN
    /* the bank's bus clock may be gated at reset (some parts enable
     * only one SRAM domain out of reset) */
    *(volatile uint32_t*)(uintptr_t)MGC_HEAP2_CLKEN |=
        (uint32_t)MGC_HEAP2_CLKEN_BITS;
#endif
    g_gap_lo = g_gap_hi = nullptr;   /* no carved gap on this shape */
    g_r1     = (uint8_t*)(uintptr_t)MGC_HEAP2_BASE;
    g_r1_end = g_r1 + MGC_HEAP2_BYTES;
    g_r1_ptr = g_r1 + 4;
    memset(g_r1, 0, MGC_HEAP2_BYTES); /* power-up SRAM is garbage; the
                                       * allocator needs pre-zeroed memory */
    HPLOG("range 1 [%p,%p) %lu KB FIXED BANK (zeroed at init; inter-bank space untabled)",
          (void*)g_r1, (void*)g_r1_end,
          (unsigned long)(MGC_HEAP2_BYTES / 1024u));
#elif defined(MGC_MULTIRANGE)
    g_gap_lo = g_region_end;
    g_gap_hi = g_gap_lo + MGC_RANGE_GAP_BYTES;
    g_r1     = g_gap_hi;
    g_r1_end = g_r1 + MGC_HEAP2_BYTES;
    g_r1_ptr = g_r1 + 4;             /* same plug skew as range 0 */
    memset(g_gap_lo, MGC_GAP_POISON, MGC_RANGE_GAP_BYTES);
    HPLOG("range 1 [%p,%p) %lu KB; gap [%p,%p) %lu KB UNTABLED (poison 0x%02x, verified per collection)",
          (void*)g_r1, (void*)g_r1_end,
          (unsigned long)(MGC_HEAP2_BYTES / 1024u),
          (void*)g_gap_lo, (void*)g_gap_hi,
          (unsigned long)(MGC_RANGE_GAP_BYTES / 1024u),
          (unsigned)MGC_GAP_POISON);
#endif

#ifdef MGC_BREAK_STOMP
    /* Negative control: the write-barrier stomp is SUPPRESSED. The
     * gate's ordering check (stomp marker before the first Alloc-return
     * marker) must FAIL against this transcript --
     * marker absence is the deterministic detector; whatever the
     * unstomped barriers then do to the image is recorded, not
     * asserted. */
    HPLOG("BREAK_STOMP: write-barrier stomp SUPPRESSED (negative control)");
#else
    /* THE STOMP: one Initialize op writes all five globals. Card
     * table = a real biased buffer covering the arena --
     * RhBulkMoveWithWriteBarrier marks cards with NO ephemeral test. */
    uintptr_t arena_floor =
        (uintptr_t)g_region & ~((uintptr_t)MGC_ARENA_COVER_BYTES - 1u);
#ifdef MGC_MULTIRANGE
    /* The union rule: bounds cover the UNION [range0.base, range1.end) -- the
     * conservative enum pair filters candidates by these globals, and
     * a superset is the only answer safe against every filter-shaped
     * reader (gap pointers passing the EE filter die at the
     * collector's exact per-range membership test). The cost lands here:
     * RhBulkMoveWithWriteBarrier's card writes for range-1 (and
     * gap-indexed) dsts are in bounds, so the cover must span through
     * range 1's end -- an unaffordable cover dies at boot, loudly.
     *
     * THE SPLIT STOMP (MGC_MR_SPLIT_STOMP), for parts whose second
     * bank sits far from the first: the union rule's hazards are
     * temporally DISJOINT. Card writes happen only at MUTATOR time
     * (barriers); the conservative filter reads only at GC time
     * (world stopped: the pair's ONLY caller is
     * Thread::GcScanRootsWorker). So the mutator posture stomps
     * PRIMARY bounds (cover spans just range 0 -- affordable even when
     * the inter-range span is hundreds of megabytes); the collection
     * re-stomps UNION on entry and restores PRIMARY before
     * RestartEE. Secondary-dst bulk moves fail the mutator bounds
     * test and skip the card write -- nothing reads cards. */
#ifdef MGC_MR_SPLIT_STOMP
    uint8_t* mr_hi = g_region_end;      /* mutator posture: PRIMARY */
#else
    uint8_t* mr_hi = g_r1_end;          /* union-always */
#endif
    uintptr_t cards_needed = (((uintptr_t)mr_hi - arena_floor) >> 10) + 64u;
    HPLOG("card cover: floor=0x%08lx span=%lu KB cards=%lu of %lu (%s)",
          (unsigned long)arena_floor,
          (unsigned long)(((uintptr_t)mr_hi - arena_floor) / 1024u),
          (unsigned long)cards_needed, (unsigned long)sizeof(g_card_buf),
#ifdef MGC_MR_SPLIT_STOMP
          "SPLIT: PRIMARY at mutator time"
#else
          "UNION through range 1"
#endif
          );
    if (cards_needed > sizeof(g_card_buf))
        heap_failfast("card cover too small for the stomped span (raise MGC_ARENA_COVER_BYTES)");
    uint32_t* card_biased =
        (uint32_t*)((uintptr_t)g_card_buf - (arena_floor >> 10));

    WriteBarrierParameters wbp;
    memset(&wbp, 0, sizeof(wbp));
    wbp.operation            = WriteBarrierOp::Initialize;
    wbp.is_runtime_suspended = true;
    wbp.card_table           = card_biased;
    wbp.lowest_address       = g_region;
    wbp.highest_address      = mr_hi;
    wbp.ephemeral_low        = (uint8_t*)~(uintptr_t)0;
    wbp.ephemeral_high       = (uint8_t*)~(uintptr_t)0;
    GCToEEInterface::StompWriteBarrier(&wbp);
    g_stomped = 1;
#ifdef MGC_MR_SPLIT_STOMP
    g_mr_card_biased = card_biased;
    HPLOG("STOMP write barrier: Initialize bounds=[%p,%p) SPLIT (mutator=PRIMARY, GC=union [%p,%p)) eph=~0 card_table=%p",
          (void*)g_region, (void*)mr_hi, (void*)g_region, (void*)g_r1_end,
          (void*)card_biased);
#else
    HPLOG("STOMP write barrier: Initialize bounds=[%p,%p) UNION eph_low=~0 eph_high=~0 card_table=%p (buf=%p biased vs arena floor 0x%08lx)",
          (void*)g_region, (void*)mr_hi, (void*)card_biased,
          (void*)g_card_buf, (unsigned long)arena_floor);
#endif
#else
    /* Coverage guard: every in-bounds dst must index INSIDE the
     * buffer -- an undersized cover is out-of-buffer card writes into
     * whatever static neighbors the buffer has (silent corruption
     * class). Checked here so a bad board knob dies at boot, loudly. */
    uintptr_t cards_needed = (((uintptr_t)g_region_end - arena_floor) >> 10) + 64u;
    HPLOG("card cover: floor=0x%08lx span=%lu KB cards=%lu of %lu",
          (unsigned long)arena_floor,
          (unsigned long)(((uintptr_t)g_region_end - arena_floor) / 1024u),
          (unsigned long)cards_needed, (unsigned long)sizeof(g_card_buf));
    if (cards_needed > sizeof(g_card_buf))
        heap_failfast("card cover too small for region placement (raise MGC_ARENA_COVER_BYTES)");
    uint32_t* card_biased =
        (uint32_t*)((uintptr_t)g_card_buf - (arena_floor >> 10));

    WriteBarrierParameters wbp;
    memset(&wbp, 0, sizeof(wbp));
    wbp.operation            = WriteBarrierOp::Initialize;
    wbp.is_runtime_suspended = true;
    wbp.card_table           = card_biased;
    wbp.lowest_address       = g_region;
    wbp.highest_address      = g_region_end;
    wbp.ephemeral_low        = (uint8_t*)~(uintptr_t)0;
    wbp.ephemeral_high       = (uint8_t*)~(uintptr_t)0;
    GCToEEInterface::StompWriteBarrier(&wbp);
    g_stomped = 1;

    HPLOG("STOMP write barrier: Initialize bounds=[%p,%p) eph_low=~0 eph_high=~0 card_table=%p (buf=%p biased vs arena floor 0x%08lx)",
          (void*)g_region, (void*)g_region_end, (void*)card_biased,
          (void*)g_card_buf, (unsigned long)arena_floor);
#endif /* MGC_MULTIRANGE */
#endif
}

#ifdef MGC_FRAG_CENSUS
/* Fragmentation observability. One O(runs) pass over the free list;
 * every caller holds the alloc lock or runs with the world stopped.
 * Transcript-affecting, so the whole feature sits behind the knob: an
 * image adopts it deliberately, with its own goldens. */
static uint32_t frag_walk(uint32_t* largest, uint32_t hist[8])
{
    uint32_t runs = 0;
    for (uint8_t* q = g_free_head; q; q = *(uint8_t**)(q + 8)) {
        uint32_t rb = 12u + *(uint32_t*)(q + 4);
        if (rb > *largest) *largest = rb;
        hist[rb < 16u ? 0 : rb < 64u ? 1 : rb < 256u ? 2 :
             rb < 1024u ? 3 : rb < 4096u ? 4 : rb < 16384u ? 5 :
             rb < 65536u ? 6 : 7]++;
        runs++;
    }
    return runs;
}

/* per-collection line -- mgc_collect calls this under its census_log
 * gate; runs= is the walked recount (the census line's runs= prints
 * the allocator's own counter, so a divergence is VISIBLE) */
extern "C" void mgc_heap_frag_line(unsigned long index)
{
    uint32_t largest = 0, hist[8] = {0, 0, 0, 0, 0, 0, 0, 0};
    uint32_t runs = frag_walk(&largest, hist);
#ifdef MGC_MULTIRANGE
    HPLOG("frag #%lu: free=%lu B runs=%lu largest=%lu B tail0=%lu B tail1=%lu B hist<16/64/256/1K/4K/16K/64K/+>=%lu/%lu/%lu/%lu/%lu/%lu/%lu/%lu",
          index, (unsigned long)g_free_bytes, (unsigned long)runs,
          (unsigned long)largest,
          (unsigned long)(uint32_t)(g_region_end - g_ptr),
          (unsigned long)(uint32_t)(g_r1_end - g_r1_ptr),
          (unsigned long)hist[0], (unsigned long)hist[1],
          (unsigned long)hist[2], (unsigned long)hist[3],
          (unsigned long)hist[4], (unsigned long)hist[5],
          (unsigned long)hist[6], (unsigned long)hist[7]);
#else
    HPLOG("frag #%lu: free=%lu B runs=%lu largest=%lu B tail=%lu B hist<16/64/256/1K/4K/16K/64K/+>=%lu/%lu/%lu/%lu/%lu/%lu/%lu/%lu",
          index, (unsigned long)g_free_bytes, (unsigned long)runs,
          (unsigned long)largest,
          (unsigned long)(g_region ? (uint32_t)(g_region_end - g_ptr) : 0),
          (unsigned long)hist[0], (unsigned long)hist[1],
          (unsigned long)hist[2], (unsigned long)hist[3],
          (unsigned long)hist[4], (unsigned long)hist[5],
          (unsigned long)hist[6], (unsigned long)hist[7]);
#endif
}
#endif /* MGC_FRAG_CENSUS */

/* ------------------------------------------------------------------ */
/* census                                                             */
/* ------------------------------------------------------------------ */

extern "C" void mgc_heap_census(const char* why)
{
    HPLOG("=== census (%s) ===", why);
    HPLOG("region [%p,%p) used %lu of %lu KB (ptr=%p stomped=%d oom=%d)",
          (void*)g_region, (void*)g_region_end,
          (unsigned long)((uint32_t)(g_ptr - g_region) / 1024u),
          (unsigned long)(MGC_HEAP_BYTES / 1024u), (void*)g_ptr,
          g_stomped, g_oom);
#ifdef MGC_MULTIRANGE
    HPLOG("range 1 [%p,%p) used %lu of %lu KB (ptr=%p); gap [%p,%p) untabled",
          (void*)g_r1, (void*)g_r1_end,
          (unsigned long)((uint32_t)(g_r1_ptr - g_r1) / 1024u),
          (unsigned long)(MGC_HEAP2_BYTES / 1024u), (void*)g_r1_ptr,
          (void*)g_gap_lo, (void*)g_gap_hi);
#endif
    HPLOG("allocs=%lu bytes=%lu pads=%lu null_returns=%lu max_size=%lu (flags=0x%lx at alloc #%lu)",
          (unsigned long)g_allocs, (unsigned long)g_bytes,
          (unsigned long)g_pads, (unsigned long)g_null_returns,
          (unsigned long)g_max_size, (unsigned long)g_max_size_flags,
          (unsigned long)g_max_size_alloc);
    HPLOG("flags: finalize=%lu contains_ref=%lu align8=%lu align8_bias=%lu zero_opt=%lu loh=%lu poh=%lu",
          (unsigned long)g_fl_finalize, (unsigned long)g_fl_containsref,
          (unsigned long)g_fl_align8, (unsigned long)g_fl_bias,
          (unsigned long)g_fl_zopt, (unsigned long)g_fl_loh,
          (unsigned long)g_fl_poh);
#ifdef MGC_SWEEP_LIVE
    HPLOG("finq: registered_total=%lu live=%lu freach=%lu(+%lu crit) lost=%lu (cap=%lu, bound=%lu)",
          (unsigned long)g_finq_reg_total, (unsigned long)g_finq_live,
          (unsigned long)g_freach_n, (unsigned long)g_freach_crit_n,
          (unsigned long)g_finq_lost, (unsigned long)MGC_FINQ_CAP,
          (unsigned long)g_finq_n);
    HPLOG("free list: runs=%lu free=%lu KB; reuse: allocs=%lu bytes=%lu KB (exact=%lu split=%lu); walk_expected=%lu",
          (unsigned long)g_free_runs, (unsigned long)(g_free_bytes / 1024u),
          (unsigned long)g_reuse_allocs,
          (unsigned long)(g_reuse_bytes / 1024u),
          (unsigned long)g_exact_fits, (unsigned long)g_splits,
          (unsigned long)g_walk_expected);
#ifdef MGC_FRAG_CENSUS
    {   /* the fragmentation question, answered at every census (the first-OOM
         * dump above all): is the free TOTAL reachable by one ask? */
        uint32_t largest = 0, hist[8] = {0, 0, 0, 0, 0, 0, 0, 0};
        uint32_t runs = frag_walk(&largest, hist);
#ifdef MGC_MULTIRANGE
        HPLOG("frag: free=%lu B runs=%lu largest=%lu B tail0=%lu B tail1=%lu B hist<16/64/256/1K/4K/16K/64K/+>=%lu/%lu/%lu/%lu/%lu/%lu/%lu/%lu",
              (unsigned long)g_free_bytes, (unsigned long)runs,
              (unsigned long)largest,
              (unsigned long)(uint32_t)(g_region_end - g_ptr),
              (unsigned long)(uint32_t)(g_r1_end - g_r1_ptr),
              (unsigned long)hist[0], (unsigned long)hist[1],
              (unsigned long)hist[2], (unsigned long)hist[3],
              (unsigned long)hist[4], (unsigned long)hist[5],
              (unsigned long)hist[6], (unsigned long)hist[7]);
#else
        HPLOG("frag: free=%lu B runs=%lu largest=%lu B tail=%lu B hist<16/64/256/1K/4K/16K/64K/+>=%lu/%lu/%lu/%lu/%lu/%lu/%lu/%lu",
              (unsigned long)g_free_bytes, (unsigned long)runs,
              (unsigned long)largest,
              (unsigned long)(g_region ? (uint32_t)(g_region_end - g_ptr) : 0),
              (unsigned long)hist[0], (unsigned long)hist[1],
              (unsigned long)hist[2], (unsigned long)hist[3],
              (unsigned long)hist[4], (unsigned long)hist[5],
              (unsigned long)hist[6], (unsigned long)hist[7]);
#endif
    }
#endif
    HPLOG("oom: reserve=%lu KB grace=%d grace_allocs=%lu collect_retries=%lu oom_reports=%lu",
          (unsigned long)(MGC_OOM_RESERVE / 1024u), g_grace,
          (unsigned long)g_grace_allocs, (unsigned long)g_collect_retries,
          (unsigned long)g_oom_reports);
#ifdef MGC_TUNE_LIVE
    HPLOG("windows: handed=%lu (from_runs=%lu) fixed=%lu plugs=%lu objs=%lu consumed=%lu KB refunded=%lu KB outstanding_hw=%lu",
          (unsigned long)g_win_handed, (unsigned long)g_win_from_runs,
          (unsigned long)g_win_fixed, (unsigned long)g_win_plugs,
          (unsigned long)g_win_objs,
          (unsigned long)(g_win_consumed / 1024u),
          (unsigned long)(g_win_refund / 1024u),
          (unsigned long)g_win_hw);
#endif
#else
    HPLOG("finq: registered=%lu lost=%lu (cap=%lu)",
          (unsigned long)g_finq_n, (unsigned long)g_finq_lost,
          (unsigned long)MGC_FINQ_CAP);
#endif
    HPLOG("alloc contexts seen=%lu identity: match=%lu mismatch=%lu",
          (unsigned long)g_ctx_seen_n, (unsigned long)g_id_match,
          (unsigned long)g_id_mismatch);
#ifdef MGC_PROBE_WINDOW
    if (g_win_state == 0)
        PRLOG("window: never handed out");
    else
        PRLOG("window: handed at alloc #%lu [%p,%p) ctx=%p state=%s",
              (unsigned long)g_win_alloc_no, (void*)g_win_base,
              (void*)g_win_limit, (void*)g_win_ctx,
              g_win_state == 2 ? "verdict logged" : "NO VERDICT (never consumed)");
#endif
    mgc_skel_census();
}

/* ------------------------------------------------------------------ */
/* finq (registration side; the partition lives in mgc_collect.cpp)   */
/* ------------------------------------------------------------------ */

/* Append a finalize registration. Without MGC_SWEEP_LIVE: monotonic.
 * With it: a first-FREE-slot rover scan (drain/suppression free
 * slots). Called from Alloc's tail (alloc lock held) and -- under
 * MGC_SWEEP_LIVE -- from the skeleton's RegisterForFinalization. */
static int finq_append_unlocked(Object* obj, size_t size)
{
#ifdef MGC_SWEEP_LIVE
    /* LOCK-FREE by design, not convenience: this is reached from
     * RhReRegisterForFinalize (via the skeleton), whose caller parks
     * NO transition frame -- so it must never enter the preemptive
     * spin (EnablePreemptiveGC over a stale deferred frame hands
     * GcScanRoots a garbage walk). CAS claims the slot; appenders and
     * the drain touch
     * disjoint state values (FREE->REG here, FREACH->FREE there);
     * the partition is suspension-serialized against both. */
    uint32_t rover = __atomic_load_n(&g_finq_rover, __ATOMIC_RELAXED);
    for (uint32_t k = 0; k < MGC_FINQ_CAP; k++) {
        uint32_t i = (rover + k) % MGC_FINQ_CAP;
        uint8_t expect = FQ_FREE;
        if (!__atomic_compare_exchange_n(&g_finq_state[i], &expect,
                                         (uint8_t)FQ_REG, false,
                                         __ATOMIC_ACQUIRE, __ATOMIC_RELAXED))
            continue;
        /* the slot is ours; the partition cannot run concurrently and
         * no other appender will touch a non-FREE slot */
        g_finq[i] = obj;
        __atomic_thread_fence(__ATOMIC_RELEASE);
        __atomic_add_fetch(&g_finq_live, 1u, __ATOMIC_RELAXED);
        uint32_t total = __atomic_add_fetch(&g_finq_reg_total, 1u,
                                            __ATOMIC_RELAXED);
        __atomic_store_n(&g_finq_rover, (i + 1) % MGC_FINQ_CAP,
                         __ATOMIC_RELAXED);
        if (i >= g_finq_n) g_finq_n = i + 1;   /* census high-water */
        if (total <= 4 || (total & 63u) == 0)
            HPLOG("finalizable #%lu registered: obj=%p size=%lu (slot %lu)",
                  (unsigned long)total, (void*)obj,
                  (unsigned long)size, (unsigned long)i);
        return 1;
    }
    if (g_finq_lost++ == 0)
        HPLOG("finalize list FULL at %lu -- counting only from here",
              (unsigned long)MGC_FINQ_CAP);
    return 0;
#else
    if (g_finq_n < MGC_FINQ_CAP) {
        g_finq[g_finq_n++] = obj;
        if (g_finq_n <= 4 || (g_finq_n & 63u) == 0)
            HPLOG("finalizable #%lu registered: obj=%p size=%lu",
                  (unsigned long)g_finq_n, (void*)obj,
                  (unsigned long)size);
        return 1;
    }
    if (g_finq_lost++ == 0)
        HPLOG("finalize list FULL at %lu -- counting only from here",
              (unsigned long)MGC_FINQ_CAP);
    return 0;
#endif
}

#ifdef MGC_SWEEP_LIVE
/* skeleton's RegisterForFinalization (ReRegisterForFinalize path) --
 * lock-free, see finq_append_unlocked */
extern "C" int mgc_finq_append(Object* obj)
{
    return finq_append_unlocked(obj, 0);
}

/* partition surface (collector context only -- world stopped; scans
 * the full cap, states gate everything) */
extern "C" uint32_t mgc_finq_bound(void) { return MGC_FINQ_CAP; }

extern "C" Object* mgc_finq_at(uint32_t i, int* state)
{
    if (state) *state = (int)g_finq_state[i];
    return g_finq[i];
}

extern "C" void mgc_finq_set_state(uint32_t i, int state)
{
    uint8_t old = g_finq_state[i];
    if (old == (uint8_t)state)
        return;
    if (old == FQ_REG)         g_finq_live--;
    if (old == FQ_FREACH)      g_freach_n--;
    if (old == FQ_FREACH_CRIT) g_freach_crit_n--;
    if (state == FQ_REG)          g_finq_live++;
    if (state == FQ_FREACH)       g_freach_n++;
    if (state == FQ_FREACH_CRIT)  g_freach_crit_n++;
    g_finq_state[i] = (uint8_t)state;
    if (state == FQ_FREE)
        g_finq[i] = nullptr;
}

/* drain surface (finalizer thread, cooperative -- suspension-serialized
 * against the partition; the alloc lock serializes vs appends) */
extern "C" uint32_t mgc_freach_count(void)
{
    return __atomic_load_n(&g_freach_n, __ATOMIC_RELAXED)
         + __atomic_load_n(&g_freach_crit_n, __ATOMIC_RELAXED);
}

extern "C" Object* mgc_freach_pop(void)
{
    /* LOCK-FREE -- the drain (RhpGetNextFinalizableObject) runs
     * cooperative with NO parked transition frame, so it must never
     * enter a preemptive spin. There is exactly one popper (the
     * finalizer thread); the partition is suspension-serialized;
     * appenders only touch FREE slots. ALL normal finalizers drain
     * before ANY critical one (the .NET guarantee; the runtime's own
     * collector implements it with two segments). */
    for (int pass = 0; pass < 2; pass++) {
        uint8_t want = pass ? (uint8_t)FQ_FREACH_CRIT : (uint8_t)FQ_FREACH;
        for (uint32_t i = 0; i < MGC_FINQ_CAP; i++) {
            if (__atomic_load_n(&g_finq_state[i], __ATOMIC_ACQUIRE) != want)
                continue;
            Object* obj = g_finq[i];
            g_finq[i] = nullptr;
            if (want == FQ_FREACH)
                __atomic_sub_fetch(&g_freach_n, 1u, __ATOMIC_RELAXED);
            else
                __atomic_sub_fetch(&g_freach_crit_n, 1u, __ATOMIC_RELAXED);
            __atomic_store_n(&g_finq_state[i], (uint8_t)FQ_FREE,
                             __ATOMIC_RELEASE);
            return obj;
        }
    }
    return nullptr;
}

/* ------------------------------------------------------------------ */
/* free list installation (sweep side, collector context)             */
/* ------------------------------------------------------------------ */

static uint8_t** g_free_tail;   /* sweep appends in address order */

extern "C" void mgc_heap_free_reset(void)
{
    g_free_head  = nullptr;
    g_free_tail  = &g_free_head;
    g_free_runs  = 0;
    g_free_bytes = 0;
}

extern "C" void mgc_heap_free_add(uint8_t* q, uint32_t bytes)
{
    /* write the free object: walkable (free MT + numComponents) and a
     * list node ([q+8] = next). The -4 header slot is dead garbage and
     * is deliberately not touched -- free-object headers are never read;
     * zeroing happens at handout, where the birth protocol requires it. */
    *(MethodTable**)q   = g_free_mt;
    *(uint32_t*)(q + 4) = bytes - 12u;
    *(uint8_t**)(q + 8) = nullptr;
    *g_free_tail = q;
    g_free_tail  = (uint8_t**)(q + 8);
    g_free_runs++;
    g_free_bytes += bytes;
}

extern "C" void mgc_heap_free_done(uint32_t live_objs)
{
    g_walk_expected = live_objs + g_free_runs;
    /* grace closes when the heap again has a reserve's worth of slack */
    if (g_grace && g_free_bytes >= MGC_OOM_RESERVE) {
        g_grace = 0;
        HPLOG("grace CLOSED: sweep recovered %lu KB free (reserve %lu KB)",
              (unsigned long)(g_free_bytes / 1024u),
              (unsigned long)(MGC_OOM_RESERVE / 1024u));
    }
}

extern "C" uint32_t mgc_heap_walk_expected(void) { return g_walk_expected; }

extern "C" uint32_t mgc_heap_free_bytes(void) { return g_free_bytes; }

extern "C" uint32_t mgc_heap_live_bytes(void)
{
#ifdef MGC_MULTIRANGE
    uint32_t bump = (uint32_t)((g_ptr - g_region) + (g_r1_ptr - g_r1));
#else
    uint32_t bump = g_region ? (uint32_t)(g_ptr - g_region) : 0;
#endif
    return bump - g_free_bytes;
}

#ifdef MGC_COMPACT
/* Compaction surface: the compactor (mgc_collect.cpp) resets the bump and
 * fixes finq slots after a slide; all callers run STW. */
extern "C" uint8_t*     mgc_heap_bump(void)            { return g_ptr; }
extern "C" void         mgc_heap_bump_set(uint8_t* p)  { g_ptr = p; }
extern "C" MethodTable* mgc_heap_freemt(void)          { return g_free_mt; }
extern "C" void         mgc_finq_fix(uint32_t i, Object* o) { g_finq[i] = o; }

/* POH = allocation-time immobility CONTRACT: the EE
 * caches these objects' addresses in non-GC memory (the __GCSTATICS
 * region cells among others) and never reports those cells as root
 * slots -- liveness rides the spanning-array strong handles instead.
 * A mover must therefore treat every GC_ALLOC_PINNED_OBJECT_HEAP
 * allocation as permanently immobile. One bit per 4-byte granule,
 * set at allocation, cleared when the object is reclaimed. */
static uint8_t g_poh_bm[MGC_HEAP_BYTES / 32u + 4u];
extern "C" uint8_t* mgc_heap_poh_bitmap(void) { return g_poh_bm; }
#ifdef MGC_MULTIRANGE
/* range 1's POH bitmap (the compose: POH allocations land in either
 * range; single-bitmap indexing off range 0 would write out of
 * bounds for a range-1 address) */
static uint8_t g_poh_bm2[MGC_HEAP2_BYTES / 32u + 4u];
extern "C" uint8_t* mgc_heap_poh_bitmap2(void) { return g_poh_bm2; }

extern "C" uint8_t* mgc_heap_free_head(void) { return g_free_head; }

/* per-range bump surface for the evacuation's move pass */
extern "C" uint8_t* mgc_heap_bump_r(uint32_t r)
{
    return r ? g_r1_ptr : g_ptr;
}
extern "C" void mgc_heap_bump_set_r(uint32_t r, uint8_t* p)
{
    if (r) g_r1_ptr = p; else g_ptr = p;
}
#endif
#endif
#endif /* MGC_SWEEP_LIVE */

/* ------------------------------------------------------------------ */
/* the allocator                                                      */
/* ------------------------------------------------------------------ */

#ifdef MGC_SWEEP_LIVE
/* One fit attempt: free-list first-fit, then bump tail. Alignment pads
 * (12-byte free objects) come from either source. Returns the object
 * address or null; all heap-state mutation is under the alloc lock. */
static uint8_t* fit_alloc(size_t size, uint32_t align_flags)
{
    /* 1) free list first-fit (reuse before virgin growth) */
    uint8_t** pprev = &g_free_head;
    for (uint8_t* q = g_free_head; q; ) {
        uint32_t rb  = 12u + *(uint32_t*)(q + 4);
        uint32_t pad = 0;
        if (align_flags) {
            uintptr_t want = (align_flags & GC_ALLOC_ALIGN8_BIAS) ? 4u : 0u;
            if (((uintptr_t)q & 7u) != want)
                pad = 12;
        }
        uint32_t need = pad + (uint32_t)size;
        uint32_t rem;
        if (rb >= need && ((rem = rb - need) == 0 || rem >= 12u)) {
            uint8_t* next = *(uint8_t**)(q + 8);
            if (rem) {
                /* shrink in place: successor run keeps address order */
                uint8_t* q2 = q + need;
                *(MethodTable**)q2   = g_free_mt;
                *(uint32_t*)(q2 + 4) = rem - 12u;
                *(uint8_t**)(q2 + 8) = next;
                *pprev = q2;
                if (!next) g_free_tail = (uint8_t**)(q2 + 8);
                g_splits++;
            } else {
                *pprev = next;
                if (!next) g_free_tail = pprev;
                g_free_runs--;
                g_walk_expected--;      /* the run object is consumed */
                g_exact_fits++;
            }
            if (pad) {
                /* walkable 12-byte filler; dead garbage until the next
                 * sweep coalesces it */
                *(MethodTable**)q   = g_free_mt;
                *(uint32_t*)(q + 4) = 0;
                g_pads++;
                g_walk_expected++;
            }
            uint8_t* r = q + pad;
            /* ZERO-ON-REUSE: the new object's footprint is
             * [r-4, r+size-4) -- header slot + MT slot + fields. The
             * word at r+size-4 is the NEXT object's header (possibly a
             * LIVE object's, carrying FINALIZER_RUN state) and must
             * not be touched. */
            memset(r - 4, 0, size);
            g_free_bytes -= need;
            g_walk_expected++;          /* the new object */
            g_reuse_allocs++;
            g_reuse_bytes += (uint32_t)size;
            return r;
        }
        pprev = (uint8_t**)(q + 8);
        q = *pprev;
    }

    /* 2) bump tail (virgin, pre-zeroed). The last MGC_OOM_RESERVE
     * bytes are untouchable until an OOM report opened the grace
     * window. */
#ifdef MGC_MULTIRANGE
    /* Primary-first bump across the range table; the OOM reserve is
     * carried by the LAST range's virgin tail (same grace semantics --
     * OOM is reported while the final tail still holds dispatch
     * headroom). A bump span never crosses the gap by construction. */
    for (int rr = 0; rr < 2; rr++) {
        uint8_t* bptr  = rr ? g_r1_ptr : g_ptr;
        uint8_t* limit = rr ? (g_grace ? g_r1_end : g_r1_end - MGC_OOM_RESERVE)
                            : g_region_end;
        uint32_t pad = 0;
        if (align_flags) {
            uintptr_t want = (align_flags & GC_ALLOC_ALIGN8_BIAS) ? 4u : 0u;
            if (((uintptr_t)bptr & 7u) != want)
                pad = 12;
        }
        if (limit > bptr
            && (uintptr_t)(limit - bptr) >= (uintptr_t)(pad + size)) {
            if (pad) {
                *(MethodTable**)bptr = g_free_mt;   /* +4 already zero */
                bptr += 12;
                g_pads++;
                g_walk_expected++;
            }
            uint8_t* r = bptr;
            bptr += size;
            if (rr) g_r1_ptr = bptr; else g_ptr = bptr;
            g_walk_expected++;
            if (g_grace) g_grace_allocs++;
            return r;
        }
    }
#else
    {
        uint8_t* limit = g_grace ? g_region_end
                                 : g_region_end - MGC_OOM_RESERVE;
        uint32_t pad = 0;
        if (align_flags) {
            uintptr_t want = (align_flags & GC_ALLOC_ALIGN8_BIAS) ? 4u : 0u;
            if (((uintptr_t)g_ptr & 7u) != want)
                pad = 12;
        }
        if (limit > g_ptr
            && (uintptr_t)(limit - g_ptr) >= (uintptr_t)(pad + size)) {
            if (pad) {
                *(MethodTable**)g_ptr = g_free_mt;  /* +4 already zero */
                g_ptr += 12;
                g_pads++;
                g_walk_expected++;
            }
            uint8_t* r = g_ptr;
            g_ptr += size;
            g_walk_expected++;
            if (g_grace) g_grace_allocs++;
            return r;
        }
    }
#endif /* MGC_MULTIRANGE */
    return nullptr;
}

#ifdef MGC_TUNE_LIVE
/* Serve-from-window-head fit: returns the served object address and
 * sets *out_limit to the window's alloc_limit (the true reservation
 * extends 12 bytes further -- the plug reserve). NULL = no window
 * possible (caller falls back to the plain fit). */
static uint8_t* fit_alloc_window(size_t size, uint8_t** out_limit)
{
    /* 1) free list: adaptive window, ZERO at handout (dead bytes) */
    uint8_t** pprev = &g_free_head;
    for (uint8_t* q = g_free_head; q; ) {
        uint32_t rb = 12u + *(uint32_t*)(q + 4);
        uint32_t win, rem;
        if (rb >= (uint32_t)size + MGC_WINDOW_BYTES + 12u) {
            win = MGC_WINDOW_BYTES;
            rem = rb - ((uint32_t)size + win + 12u);
            if (rem && rem < 12u) { win += rem; rem = 0; }
        } else if (rb >= (uint32_t)size + MGC_WINDOW_FLOOR + 12u) {
            win = rb - (uint32_t)size - 12u;   /* whole run */
            rem = 0;
        } else {
            pprev = (uint8_t**)(q + 8);
            q = *pprev;
            continue;
        }
        uint8_t* next = *(uint8_t**)(q + 8);
        if (rem) {
            uint8_t* q2 = q + (uint32_t)size + win + 12u;
            *(MethodTable**)q2   = g_free_mt;
            *(uint32_t*)(q2 + 4) = rem - 12u;
            *(uint8_t**)(q2 + 8) = next;
            *pprev = q2;
            if (!next) g_free_tail = (uint8_t**)(q2 + 8);
            g_splits++;
        } else {
            *pprev = next;
            if (!next) g_free_tail = pprev;
            g_free_runs--;
            g_walk_expected--;
        }
        /* zero served object + window span + the last successor-header
         * slot inside it: [q-4, q+size+win+4). Never reaches the plug
         * reserve's own tail word (the live successor's header). */
        memset(q - 4, 0, (uint32_t)size + win + 4u);
        g_free_bytes -= (rb - rem);
        g_walk_expected++;              /* the served object */
        g_reuse_allocs++;
        g_reuse_bytes += (uint32_t)size;
        g_win_from_runs++;
        *out_limit = q + (uint32_t)size + win;
        return q;
    }
    /* 2) bump tail: virgin, no memset */
#ifdef MGC_MULTIRANGE
    /* primary-first; a window never spans the gap (whole reservation
     * from one range's tail); reserve carried by the last range */
    for (int rr = 0; rr < 2; rr++) {
        uint8_t* bptr  = rr ? g_r1_ptr : g_ptr;
        uint8_t* limit = rr ? (g_grace ? g_r1_end : g_r1_end - MGC_OOM_RESERVE)
                            : g_region_end;
        uint32_t need = (uint32_t)size + MGC_WINDOW_BYTES + 12u;
        if (limit > bptr && (uintptr_t)(limit - bptr) >= need) {
            uint8_t* r = bptr;
            if (rr) g_r1_ptr = bptr + need; else g_ptr = bptr + need;
            g_walk_expected++;          /* the served object */
            if (g_grace) g_grace_allocs++;
            *out_limit = r + (uint32_t)size + MGC_WINDOW_BYTES;
            return r;
        }
    }
#else
    {
        uint8_t* limit = g_grace ? g_region_end
                                 : g_region_end - MGC_OOM_RESERVE;
        uint32_t need = (uint32_t)size + MGC_WINDOW_BYTES + 12u;
        if (limit > g_ptr && (uintptr_t)(limit - g_ptr) >= need) {
            uint8_t* r = g_ptr;
            g_ptr += need;
            g_walk_expected++;          /* the served object */
            if (g_grace) g_grace_allocs++;
            *out_limit = r + (uint32_t)size + MGC_WINDOW_BYTES;
            return r;
        }
    }
#endif /* MGC_MULTIRANGE */
    return nullptr;
}

/* Retire a context's outstanding window: count the objects born in
 * [base, alloc_ptr) into the walk model, plug [alloc_ptr, limit+12)
 * as a free object (numComponents = limit-ptr), refund the
 * unused span from the handout charge, clear the context. MUST be
 * called with the alloc lock held (or from collector context where
 * the lock is provably free and world-stopped). */
static void window_fix_locked(gc_alloc_context* acontext)
{
    WinRec* w = win_lookup(acontext);
    uint8_t* ptr   = (uint8_t*)acontext->alloc_ptr;
    uint8_t* limit = (uint8_t*)acontext->alloc_limit;
    if (!w) {
        if (ptr || limit)
            heap_failfast("alloc context carries a window this table never handed out");
        return;
    }
    if (!(ptr >= w->base && ptr <= limit))
        heap_failfast("window fix geometry (alloc_ptr outside [base,limit])");
#ifdef MGC_BREAK_WINDOW_FIX
    /* Negative control: no plug, no count -- the walk model and heap
     * parseability are deliberately broken. The next heap walk reads
     * a NULL MethodTable in the gap -> 'heap walk CORRUPT' fail-fast
     * is the deterministic detector. */
    {
        static int armed;
        if (!armed++)
            HPLOG("BREAK_WINDOW_FIX: window plug+count SUPPRESSED (negative control)");
    }
#else
    /* count the fast-path births */
    uint32_t objs = 0;
    for (uint8_t* p = w->base; p < ptr; ) {
        MethodTable* mt = *(MethodTable**)p;
        uint32_t s = mt ? mt->GetBaseSize() : 0;
        if (mt && mt->HasComponentSize())
            s += *(uint32_t*)(p + 4) * (uint32_t)mt->RawGetComponentSize();
        s = (s + 3u) & ~3u;
        if (!mt || s < 12u || s > (uint32_t)(ptr - p)) {
            HPLOG("window walk CORRUPT at %p (MT=%p step=%lu span=[%p,%p))",
                  (void*)p, (void*)mt, (unsigned long)s,
                  (void*)w->base, (void*)ptr);
            heap_failfast("window object walk corrupt");
        }
        objs++;
        p += s;
    }
    /* plug: free object spanning [ptr, limit+12) -- the +12 reserve
     * makes any remainder (including 0) legal */
    *(MethodTable**)ptr   = g_free_mt;
    *(uint32_t*)(ptr + 4) = (uint32_t)(limit - ptr);
    g_walk_expected += objs + 1u;
    g_win_objs += objs;
    g_win_plugs++;
    /* refund the unused span (charged in full at handout) */
    uint32_t rem = (uint32_t)(limit - ptr);
    acontext->alloc_bytes -= (int64_t)rem;
    g_bytes -= rem;
    g_win_refund += rem;
    g_win_consumed += (uint32_t)(ptr - w->base);
#endif
    g_win_fixed++;
    acontext->alloc_ptr   = nullptr;
    acontext->alloc_limit = nullptr;
    w->ctx = nullptr;
}

/* collector + detach entry (takes the lock cooperatively: the detach
 * caller's frame status is unknown) */
extern "C" void mgc_heap_window_fix(gc_alloc_context* acontext)
{
    if (!acontext)
        return;
    alloc_lock_acquire_coop();
    if (win_lookup(acontext) || acontext->alloc_ptr || acontext->alloc_limit)
        window_fix_locked(acontext);
    alloc_lock_release();
}
#endif /* MGC_TUNE_LIVE */
#endif /* MGC_SWEEP_LIVE */

extern "C" void* mgc_heap_alloc(gc_alloc_context* acontext, size_t size,
                                uint32_t flags)
{
    if (!g_region)
        heap_failfast("Alloc before mgc_heap_init (init-order contract broken)");

#ifdef MGC_SWEEP_LIVE
    alloc_lock_acquire();
#else
    if (g_oom) {
        if (g_null_returns < 3)
            HPLOG("post-wall Alloc(size=%lu flags=0x%lx) -> NULL",
                  (unsigned long)size, (unsigned long)flags);
        g_null_returns++;
        return nullptr;
    }
#endif

    /* --- identity probe: GetAllocContext() vs acontext, in-call --- */
    {
        gc_alloc_context* ac2 = GCToEEInterface::GetAllocContext();
        if (ac2 == acontext) g_id_match++; else g_id_mismatch++;
        unsigned i;
        for (i = 0; i < g_ctx_seen_n; i++)
            if (g_ctx_seen[i] == acontext) break;
        if (i == g_ctx_seen_n && g_ctx_seen_n < 16) {
            g_ctx_seen[g_ctx_seen_n++] = acontext;
            HPLOG("ctx[%u] first sight: acontext=%p GetAllocContext=%p GetThread=%p -> %s (alloc #%lu)",
                  i, (void*)acontext, (void*)ac2,
                  (void*)GCToEEInterface::GetThread(),
                  ac2 == acontext ? "IDENTICAL" : "DIFFERENT",
                  (unsigned long)(g_allocs + 1));
        } else if (ac2 != acontext && g_id_mismatch <= 3) {
            HPLOG("identity MISMATCH at alloc #%lu: acontext=%p GetAllocContext=%p",
                  (unsigned long)(g_allocs + 1), (void*)acontext, (void*)ac2);
        }
    }

#ifdef MGC_PROBE_WINDOW
    /* --- window probe verdict: did the EE fast path consume it? --- */
    if (g_win_state == 1 && acontext == g_win_ctx) {
        if (acontext->alloc_ptr != g_win_base) {
            PRLOG("window CONSUMED: alloc_ptr moved %p -> %p (limit %p) by alloc #%lu -- EE fast path ENGAGED (combined_limit refreshed EE-side)",
                  (void*)g_win_base, (void*)acontext->alloc_ptr,
                  (void*)acontext->alloc_limit, (unsigned long)(g_allocs + 1));
            g_win_state = 2;
        }
    }
#endif

    /* --- flags census + first-sight log per distinct flag value --- */
    if (flags & GC_ALLOC_FINALIZE)           g_fl_finalize++;
    if (flags & GC_ALLOC_CONTAINS_REF)       g_fl_containsref++;
    if (flags & GC_ALLOC_ALIGN8)             g_fl_align8++;
    if (flags & GC_ALLOC_ALIGN8_BIAS)        g_fl_bias++;
    if (flags & GC_ALLOC_ZEROING_OPTIONAL)   g_fl_zopt++;
    if (flags & GC_ALLOC_LARGE_OBJECT_HEAP)  g_fl_loh++;
    if (flags & GC_ALLOC_PINNED_OBJECT_HEAP) g_fl_poh++;
    {
        unsigned i;
        for (i = 0; i < g_seen_flags_n; i++)
            if (g_seen_flags[i] == flags) break;
        if (i == g_seen_flags_n && g_seen_flags_n < 16) {
            g_seen_flags[g_seen_flags_n++] = flags;
            HPLOG("first sight of flags=0x%lx (alloc #%lu size=%lu ctx=%p)",
                  (unsigned long)flags, (unsigned long)(g_allocs + 1),
                  (unsigned long)size, (void*)acontext);
        }
    }

    /* --- size hygiene: the contract delivers 4-aligned sizes --- */
    if (size & 3u) {
        HPLOG("UNALIGNED size %lu at alloc #%lu (flags=0x%lx) -- aligning up",
              (unsigned long)size, (unsigned long)(g_allocs + 1),
              (unsigned long)flags);
        size = (size + 3u) & ~(size_t)3u;
    }

#ifdef MGC_SWEEP_LIVE
    /* --- fit: free list first, bump tail behind the reserve wall;
     * on a miss, collect once and retry once (GcAllocInternal never
     * retries, so the retry must live here) --- */
    uint32_t align_flags = flags & (GC_ALLOC_ALIGN8 | GC_ALLOC_ALIGN8_BIAS);
    uint32_t pad_bytes = 0;
    uint8_t* result = nullptr;
#ifdef MGC_TUNE_LIVE
    /* An outstanding window on this context means the EE fast path
     * missed (request too big for the remainder): retire it first --
     * the remainder becomes a plug. */
    if (acontext && (win_lookup(acontext)
                     || acontext->alloc_ptr || acontext->alloc_limit))
        window_fix_locked(acontext);
    /* window-eligible request: serve from the window head and hand
     * the rest to the EE fast path (GcAllocInternal refreshes
     * combined_limit from alloc_limit after Alloc returns) */
    if (acontext && (flags & ~GC_ALLOC_CONTAINS_REF) == 0) {
        uint8_t* wlimit;
        result = fit_alloc_window(size, &wlimit);
        if (result) {
            WinRec* w = nullptr;
            for (unsigned i = 0; i < 16; i++)
                if (!g_win_tab[i].ctx) { w = &g_win_tab[i]; break; }
            if (!w) {
                /* table full: serve without a window. An untracked
                 * window could never be plugged later (no record), so
                 * the reserved span is plugged NOW as a free object. */
                uint8_t* wbase = result + size;
                *(MethodTable**)wbase   = g_free_mt;
                *(uint32_t*)(wbase + 4) = (uint32_t)(wlimit - wbase);
                g_walk_expected++;
            } else {
                w->ctx  = acontext;
                w->base = result + size;
                acontext->alloc_ptr   = (uint8_t*)result + size;
                acontext->alloc_limit = (uint8_t*)wlimit;
                /* charge the full window span now; the fix refunds
                 * the unused part */
                uint32_t span = (uint32_t)(wlimit - (result + size));
                acontext->alloc_bytes += (int64_t)span;
                g_bytes += span;
                g_win_handed++;
                uint32_t out = 0;
                for (unsigned i = 0; i < 16; i++)
                    if (g_win_tab[i].ctx) out++;
                if (out > g_win_hw) g_win_hw = out;
                if (g_win_handed <= 4 || (g_win_handed & 255u) == 0
#ifdef MGC_DIAG_ALLOC_LOG /* diagnostic knob */
                    || 1
#endif
                    )
                    HPLOG("window #%lu handed: ctx=%p [%p,%p) (%lu B) at alloc #%lu",
                          (unsigned long)g_win_handed, (void*)acontext,
                          (void*)(result + size), (void*)wlimit,
                          (unsigned long)span,
                          (unsigned long)(g_allocs + 1));
            }
        }
    }
    if (!result)
#endif /* MGC_TUNE_LIVE */
    result = fit_alloc(size, align_flags);
    /* The concurrent-wall race: between THIS thread's collection
     * completing and its re-acquired fit, other threads' window
     * handouts can drain the freshly swept list -- a single retry
     * could then declare a spurious OOM while the heap is healthy.
     * The answer is bounded escalation: OOM is declared only after
     * MGC_WALL_RETRIES consecutive own-collection+fit failures. Green
     * paths are unchanged (the loop iterates only on failure). */
    for (uint32_t attempt = 1; !result && attempt <= MGC_WALL_RETRIES; attempt++) {
        g_collect_retries++;
        HPLOG("wall: collect-and-retry (alloc #%lu size=%lu flags=0x%lx free=%lu KB grace=%d attempt=%lu/%lu)",
              (unsigned long)(g_allocs + 1), (unsigned long)size,
              (unsigned long)flags, (unsigned long)(g_free_bytes / 1024u),
              g_grace, (unsigned long)attempt,
              (unsigned long)MGC_WALL_RETRIES);
        /* release around the collection: SuspendEE must never find the
         * lock held by the collecting thread (RhpGcAlloc parked its
         * deferred frame -- the same shape as RhCollect) */
        alloc_lock_release();
        mgc_heap_collect(2, /*low_memory_p*/ 1, /*mode*/ 0);
        alloc_lock_acquire();
        result = fit_alloc(size, align_flags);
    }
#ifdef MGC_COMPACT
    if (!result) {
        /* Compaction trigger: still failing AFTER completed full GCs -- one
         * compacting collection, one more fit, only then the OOM
         * protocol. */
        g_collect_retries++;
        HPLOG("wall: compact-and-retry (alloc #%lu size=%lu flags=0x%lx free=%lu KB)",
              (unsigned long)(g_allocs + 1), (unsigned long)size,
              (unsigned long)flags, (unsigned long)(g_free_bytes / 1024u));
        mgc_compact_request();
        alloc_lock_release();
        mgc_heap_collect(2, /*low_memory_p*/ 1, /*mode*/ 0);
        alloc_lock_acquire();
        result = fit_alloc(size, align_flags);
    }
#endif
    if (!result) {
        if (size <= 4096u && g_grace) {
            /* a dispatch-sized allocation missing even the open
             * reserve is the livelock precondition -- die loudly
             * instead of hanging the next SuspendEE */
            HPLOG("OOM WALL: reserve EXHAUSTED by dispatch-sized alloc #%lu (size=%lu) -- raise MGC_OOM_RESERVE",
                  (unsigned long)(g_allocs + 1), (unsigned long)size);
            mgc_heap_census("reserve exhausted");
            heap_failfast("headroom reserve exhausted during OOM dispatch (raise MGC_OOM_RESERVE)");
        }
        g_oom_reports++;
        g_null_returns++;
        if (!g_grace) {
            g_grace = 1;
            HPLOG("OOM at alloc #%lu: size=%lu flags=0x%lx -> NULL (managed protocol); headroom reserve OPEN (%lu KB) for exception dispatch",
                  (unsigned long)(g_allocs + 1), (unsigned long)size,
                  (unsigned long)flags,
                  (unsigned long)(MGC_OOM_RESERVE / 1024u));
        } else if (g_oom_reports <= 6 || (g_oom_reports & 31u) == 0) {
            HPLOG("OOM at alloc #%lu: size=%lu -> NULL (grace already open, report #%lu)",
                  (unsigned long)(g_allocs + 1), (unsigned long)size,
                  (unsigned long)g_oom_reports);
        }
        if (g_oom_reports == 1)
            mgc_heap_census("first OOM report");
        alloc_lock_release();
        return nullptr;
    }
#else
    /* --- alignment: ALIGN8 wants result%8==0, +BIAS wants ==4 --- */
    uint32_t pad = 0;
    if (flags & (GC_ALLOC_ALIGN8 | GC_ALLOC_ALIGN8_BIAS)) {
        uintptr_t want = (flags & GC_ALLOC_ALIGN8_BIAS) ? 4u : 0u;
        if (((uintptr_t)g_ptr & 7u) != want)
            pad = 12; /* min free object: 12 mod 8 = 4 flips parity and
                       * keeps the heap walkable */
    }

    /* --- the wall --- */
    if ((uintptr_t)(g_region_end - g_ptr) < (uintptr_t)(pad + size)) {
        g_oom = 1;
#ifdef MGC_COLLECT_LIVE
        /* A mark-only build reclaims nothing, so region exhaustion is
         * a hard wall -- and it must be loud, not a NULL: returning
         * NULL here livelocks (the unhandled-OutOfMemoryException
         * machinery itself allocates, every allocation is NULL at a
         * hard wall, and the next collector's SuspendEE waits forever
         * on the never-parking thread). Mark-only images die loudly
         * instead; the sweep build has the reserve and the retry
         * above. */
        HPLOG("OOM WALL at alloc #%lu: size=%lu(+pad %lu) flags=0x%lx exceeds region (used %lu of %lu KB) -- sweep is the next stage",
              (unsigned long)(g_allocs + 1), (unsigned long)size,
              (unsigned long)pad, (unsigned long)flags,
              (unsigned long)((uint32_t)(g_ptr - g_region) / 1024u),
              (unsigned long)(MGC_HEAP_BYTES / 1024u));
        mgc_heap_census("OOM wall");
        heap_failfast("region exhausted under mark-only collector (sweep-stage entry evidence)");
#else
        HPLOG("OOM WALL at alloc #%lu: size=%lu(+pad %lu) flags=0x%lx exceeds region (used %lu of %lu KB) -> NULL (managed OutOfMemoryException protocol)",
              (unsigned long)(g_allocs + 1), (unsigned long)size,
              (unsigned long)pad, (unsigned long)flags,
              (unsigned long)((uint32_t)(g_ptr - g_region) / 1024u),
              (unsigned long)(MGC_HEAP_BYTES / 1024u));
        mgc_heap_census("OOM wall");
        return nullptr;
#endif
    }

    if (pad) {
        /* gap fill: a 12-byte free object (header slot at g_ptr-4 is
         * already zero; numComponents at +4 already zero) */
        *(MethodTable**)g_ptr = g_free_mt;
        g_ptr += 12;
        g_pads++;
    }

    uint8_t* result = g_ptr;
    g_ptr += size;
    uint32_t pad_bytes = pad;
#endif /* MGC_SWEEP_LIVE */

    g_allocs++;
    g_bytes += (uint32_t)size + pad_bytes;
    if ((uint32_t)size > g_max_size) {
        g_max_size = (uint32_t)size;
        g_max_size_flags = flags;
        g_max_size_alloc = g_allocs;
    }

    /* per-thread accounting the EE reads (GcAllocInternal maintains
     * combined_limit only; alloc_bytes is the GC's book) */
    if (acontext) {
        acontext->alloc_bytes += (int64_t)size;
        acontext->alloc_count++;
    }

    /* --- finalize registration (the GC's duty; EE does none) --- */
    if (flags & GC_ALLOC_FINALIZE)
        finq_append_unlocked((Object*)result, size);

#ifdef MGC_COMPACT
    if (flags & GC_ALLOC_PINNED_OBJECT_HEAP) {
#ifdef MGC_MULTIRANGE
        if ((uint8_t*)result < g_region_end) {
            uint32_t pidx_ = (uint32_t)((uint8_t*)result - g_region) >> 2;
            g_poh_bm[pidx_ >> 3] |= (uint8_t)(1u << (pidx_ & 7u));
        } else {
            uint32_t pidx_ = (uint32_t)((uint8_t*)result - g_r1) >> 2;
            g_poh_bm2[pidx_ >> 3] |= (uint8_t)(1u << (pidx_ & 7u));
        }
#else
        uint32_t pidx_ = (uint32_t)((uint8_t*)result - g_region) >> 2;
        g_poh_bm[pidx_ >> 3] |= (uint8_t)(1u << (pidx_ & 7u));
#endif
    }
#endif
    if (g_allocs <= 12
#ifdef MGC_DIAG_ALLOC_LOG /* diagnostic knob */
        || 1
#endif
        )
        HPLOG("Alloc #%lu: ctx=%p size=%lu flags=0x%lx -> %p",
              (unsigned long)g_allocs, (void*)acontext,
              (unsigned long)size, (unsigned long)flags, (void*)result);
    if (g_allocs == 1)
        HPLOG("first Alloc returned %p (size=%lu flags=0x%lx, region use begins)",
              (void*)result, (unsigned long)size, (unsigned long)flags);
    if ((g_allocs & 16383u) == 0)
        HPLOG("progress: %lu allocs, %lu KB used of %lu KB",
              (unsigned long)g_allocs,
              (unsigned long)((uint32_t)(g_ptr - g_region) / 1024u),
              (unsigned long)(MGC_HEAP_BYTES / 1024u));

#ifdef MGC_PROBE_WINDOW
    /* --- hand out exactly ONE small window, early, on a plain
     * small allocation, then watch whether Alloc traffic for that
     * context stops advancing through us --- */
    if (g_win_state == 0 && flags == 0 && acontext && g_allocs >= 8
        && (uintptr_t)(g_region_end - g_ptr) > MGC_PROBE_WINDOW_BYTES + 64u) {
        g_win_ctx   = acontext;
        g_win_base  = g_ptr;
        g_win_limit = g_ptr + MGC_PROBE_WINDOW_BYTES;
        acontext->alloc_ptr   = g_win_base;
        acontext->alloc_limit = g_win_limit;
        g_ptr = g_win_limit; /* reserve; pre-zeroed; skew carried by the
                              * just-returned object's quantum tail */
        g_win_alloc_no = g_allocs;
        g_win_state = 1;
        PRLOG("window handed at alloc #%lu: alloc_ptr=%p alloc_limit=%p (%lu B) ctx=%p -- watching combined_limit behavior",
              (unsigned long)g_allocs, (void*)g_win_base,
              (void*)g_win_limit, (unsigned long)MGC_PROBE_WINDOW_BYTES,
              (void*)acontext);
    }
#endif

#ifdef MGC_SWEEP_LIVE
    alloc_lock_release();
#endif
    return result;
}

/* ------------------------------------------------------------------ */
/* BCL-surface backing (skeleton delegates under MGC_HEAP_LIVE)       */
/* ------------------------------------------------------------------ */

extern "C" uint32_t mgc_heap_used_bytes(void)
{
#ifdef MGC_MULTIRANGE
    /* SUM of bumps (BCL/GetTotalMemory surface); not a walk bound --
     * the collector walks per range via mgc_heap_range_get. */
    return (uint32_t)((g_ptr - g_region) + (g_r1_ptr - g_r1));
#else
    return g_region ? (uint32_t)(g_ptr - g_region) : 0;
#endif
}

/* collector surface: allocator census counts (the heap-walk
 * cross-check) and the finalize-registration list (a root class until
 * finalizers run). */
extern "C" void mgc_heap_counts(uint32_t* allocs, uint32_t* pads)
{
    if (allocs) *allocs = g_allocs;
    if (pads)   *pads = g_pads;
}

extern "C" uint32_t mgc_heap_finq_snapshot(Object*** out)
{
    if (out) *out = g_finq;
    return g_finq_n;
}

extern "C" uint32_t mgc_heap_total_alloc_bytes(void)
{
    return g_bytes;
}

extern "C" int mgc_heap_contains(const void* p)
{
#ifdef MGC_MULTIRANGE
    return (g_region && (const uint8_t*)p >= g_region
                     && (const uint8_t*)p < g_region_end)
        || (g_r1 && (const uint8_t*)p >= g_r1
                 && (const uint8_t*)p < g_r1_end);
#else
    return g_region && (const uint8_t*)p >= g_region
                    && (const uint8_t*)p < g_region_end;
#endif
}

extern "C" void mgc_heap_range(uint8_t** lo, uint8_t** hi)
{
#ifdef MGC_MULTIRANGE
    /* UNION bounds: anything outside is definitely not heap;
     * inside may be the gap -- exact membership is mgc_heap_contains /
     * the per-range API below. */
    if (lo) *lo = g_region;
    if (hi) *hi = g_r1_end;
#else
    if (lo) *lo = g_region;
    if (hi) *hi = g_region_end;
#endif
}

#ifdef MGC_MULTIRANGE
/* The range table surface (multi-range): the collector and the skeleton
 * enumerate ranges instead of assuming one span. */
extern "C" uint32_t mgc_heap_ranges(void) { return 2u; }

extern "C" void mgc_heap_range_get(uint32_t i, uint8_t** base,
                                   uint8_t** end, uint8_t** bump)
{
    if (i == 0) {
        if (base) *base = g_region;
        if (end)  *end  = g_region_end;
        if (bump) *bump = g_ptr;
    } else {
        if (base) *base = g_r1;
        if (end)  *end  = g_r1_end;
        if (bump) *bump = g_r1_ptr;
    }
}

/* Gap-poison verify: the untabled gap must never be written. One
 * O(gap) pass; callers run with the world stopped. A violated byte is
 * an out-of-range write (mis-ranged alloc, wild barrier, table bug) --
 * loud, attributed, never silent. */
#ifdef MGC_MR_SPLIT_STOMP
/* Split stomp: the collection swings the EE bounds -- UNION on
 * entry (the conservative stack scan must see every range), PRIMARY
 * on exit (bounded card cover for mutator-time bulk barriers). Both
 * calls run with the world stopped; Initialize writes all five
 * globals, so each swing is a complete posture. */
extern "C" void mgc_heap_stomp_gc(int enter)
{
    WriteBarrierParameters wbp;
    memset(&wbp, 0, sizeof(wbp));
    wbp.operation            = WriteBarrierOp::Initialize;
    wbp.is_runtime_suspended = true;
    wbp.card_table           = g_mr_card_biased;
    wbp.lowest_address       = g_region;
    wbp.highest_address      = enter ? g_r1_end : g_region_end;
    wbp.ephemeral_low        = (uint8_t*)~(uintptr_t)0;
    wbp.ephemeral_high       = (uint8_t*)~(uintptr_t)0;
    GCToEEInterface::StompWriteBarrier(&wbp);
}
#endif /* MGC_MR_SPLIT_STOMP */

extern "C" void mgc_heap_gap_verify(const char* when)
{
    for (uint8_t* p = g_gap_lo; p < g_gap_hi; p++) {
        if (*p != (uint8_t)MGC_GAP_POISON) {
            HPLOG("RANGE GAP POISON VIOLATED at %p (=0x%02x, %s) -- something wrote through the untabled gap",
                  (void*)p, (unsigned)*p, when);
            mgc_heap_census("gap poison violated");
            heap_failfast("range gap poison violated (out-of-range write)");
        }
    }
}
#endif /* MGC_MULTIRANGE */
