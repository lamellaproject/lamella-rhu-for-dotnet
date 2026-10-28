/* mgc_collect.cpp -- the mgc collector: suspension + mark, and behind
 * MGC_SWEEP_LIVE the death pipeline, the sweep, and the compactor.
 *
 * The collection behind the allocator: a full-heap, stop-the-world
 * MARK over a side bitmap. Without MGC_SWEEP_LIVE nothing is swept and
 * no finalizer runs -- nothing dies, so a payload's retained-integrity
 * checks hold trivially; what a mark-only build proves is SUSPENSION
 * CORRECTNESS and ROOT COMPLETENESS, with the census as the evidence.
 *
 * Contract facts this file is built against (the runtime's own
 * collector and EE bodies; the gate asserts the order on the wire):
 *
 *  - CALLBACK ORDER around a non-concurrent collection (the runtime's
 *    GarbageCollectGeneration -> garbage_collect -> gc1 -> mark_phase
 *    -> do_post_gc):
 *      SuspendEE(SUSPEND_FOR_GC)
 *        GcEnumAllocContexts(fix)         [garbage_collect, first]
 *        update collection counts         [before the work]
 *        DiagGCStart(condemned, induced)  [no-op body in this build]
 *        GcStartWork(condemned, max_gen)  [RestrictedCallouts kind 0]
 *        BeforeGcScanRoots(c, bgc, conc)  [no-op body in this build]
 *        mark_ro_segments                 [frozen segs AS ROOT SOURCES]
 *        GCScan::GcScanRoots              [stacks + statics, precise]
 *        GcScanHandles                    [the GC's own handle table]
 *        dependent-handle fixpoint        [GcDhInitialScan/GcDhReScan]
 *        AfterGcScanRoots(c, max, sc)     [RestrictedCallouts kind 2]
 *        (ShortWeakPtrScan / ScanForFinalization / WeakPtrScan -- the
 *         death pipeline, MGC_SWEEP_LIVE only; nothing dies without it)
 *        GcDone(condemned)                [RestrictedCallouts kind 1]
 *        DiagGCEnd(index, gen, reason, fConcurrent=false)
 *      RestartEE(true)
 *      EnableFinalization(b) -- only when f-reachable work exists; a
 *      mark-only build has none, so it is honestly not called.
 *
 *  - SUSPENSION IS THE HEAP LOCK. RhCollect vcalls GarbageCollect in
 *    COOPERATIVE mode (Thread::DisablePreemptiveMode first, a deferred
 *    frame parked on the Thread); allocation (RhpGcAlloc/
 *    GcAllocInternal) also runs cooperative with only a DEFERRED frame
 *    -- so SuspendEE WAITS for any thread mid-allocation to finish and
 *    park at a safe point. After SuspendEE returns there are no
 *    half-born objects and no concurrent Alloc: the bump heap is
 *    exclusively the collector's.
 *
 *  - CONCURRENT GarbageCollect entries are real (worker threads call
 *    GC.Collect while another thread collects). The runtime's own
 *    collector parks the loser in PREEMPTIVE mode while it waits, so
 *    the winner's SuspendEE does not deadlock on it. Here that is the
 *    g_gc_busy guard: contenders go preemptive, yield until the
 *    collector is free, re-enter cooperative, retry.
 *
 *  - GcEnumAllocContexts (EE body): iterates the ThreadStore, hands
 *    the callback each Thread+4 (= gc_alloc_context), and AFTER the
 *    callback zeroes combined_limit itself when alloc_limit==0.
 *    Without MGC_TUNE_LIVE no windows are ever handed out, so contexts
 *    stay {0,0} -- the call is made for order parity and evidence.
 *
 *  - EE callback bodies in this build: BeforeGcScanRoots and
 *    DiagGCStart are literal no-ops; GcStartWork/AfterGcScanRoots/
 *    GcDone invoke RestrictedCallouts kinds 0/2/1; DiagGCEnd runs
 *    ETW::GCLog::WalkHeap when fConcurrent=false (benign: the
 *    runtime's own collector drives it on every workstation GC);
 *    EnableFinalization(b) sets the finalizer event only for b!=0.
 *
 *  - GcScanRoots (EE body): per non-IsGCSpecial thread it vcalls
 *    IGCHeap::IsThreadUsingAllocationContextHeap(thread+4,
 *    sc->thread_number) -- the skeleton answers true -- then walks
 *    inlined thread statics and the stack precisely, calling
 *    promote_func with GC_CALL_INTERIOR/GC_CALL_PINNED qualifiers.
 *    ScanContext.promotion=true for a mark pass (never let the EE
 *    think a non-moving GC is relocating).
 *
 *  - GCDesc tracing (vendored gcdesc.h): series table BACKWARD from
 *    the MT, count at MT-4; series sizes biased by the total object
 *    size (add back base+components); NEGATIVE count = valuetype-array
 *    repeating {nptrs,skip} u16 pairs -- walked here per-component
 *    exactly NumComponents times (the GetNumPointers recipe), which
 *    sidesteps any end-bound question.
 *
 *  - MARK STORAGE is a SIDE BITMAP (region/32 bytes, 1 bit per 4-byte
 *    granule) + an explicit mark stack, both drawn through the shim's
 *    audited mmap (arena-audit accounting; zero static RAM). MT words
 *    are NEVER mutated -- RawGetMethodTable stays valid everywhere, no
 *    clear-before-RestartEE obligation, and the runtime's own
 *    mark-bit-in-MT trick is deliberately not used.
 *
 *  - Post-stomp, NO barrier records frozen->heap stores (the checked
 *    barrier exits on the [lowest,highest) bounds test before card
 *    marking; frozen segments live OUTSIDE the stomped bounds), so
 *    every collection walks EVERY frozen object's ref fields as a
 *    root source -- the mark_ro_segments position in the order above.
 *
 *  - The EE does NOT retry allocation after a NULL Alloc
 *    (GcAllocInternal has exactly three vcalls: GetLOHThreshold, Alloc,
 *    PublishObject -- no GarbageCollect). OOM remains the bounded
 *    managed-OutOfMemoryException protocol.
 *
 * Built with -DMGC_BREAK_ROOTS, all root scanning is suppressed (callback
 * bracket, bitmap, census all still run), so the census prints
 * "roots=0 ... marked=0" and the collector gate's roots>0 check fails on
 * that build; under a mark-only collection nothing dies either way, and
 * the outcome is printed, not asserted.
 *
 * -DMGC_SWEEP_LIVE -- the death pipeline, in the runtime's own
 * mark_phase position and order:
 *
 *   AfterGcScanRoots
 *   GcShortWeakPtrScan            | null ws slots w/ unmarked referents
 *   CFinalize::ScanForFinalization| the finq partition (below)
 *   DiagWalkFReachableObjects     | EE body: literal no-op
 *   GcDhReScan fixpoint           | 2nd dependent pass (freach-promoted
 *                                 | primaries pull secondaries)
 *   GcWeakPtrScan                 | null wl slots + DEAD-PRIMARY
 *                                 | dependent clearing (null BOTH)
 *   GcWeakPtrScanBySingleThread   | SyncBlockCacheWeakPtrScan -- EE
 *                                 | body: literal no-op
 *   [sweep -- GC-internal, no EE callbacks]
 *   GcDone / DiagGCEnd / RestartEE
 *   EnableFinalization(true)      | only when f-reachable work was
 *                                 | promoted (the runtime also sets the
 *                                 | event INSIDE ScanForFinalization --
 *                                 | idempotent, parity-irrelevant: the
 *                                 | finalizer cannot run while
 *                                 | suspended)
 *
 * The ScanForFinalization partition, branch order per the runtime's
 * own collector: per registered entry: live (marked) -> stays
 * registered (registered finalizables are NOT roots -- they die by
 * unreachability); dead -> (1) GCToEEInterface::EagerFinalized(obj)
 * FIRST -- true for HasEagerFinalizer types (WeakReference: MT
 * extended-flags bit 0 with HasComponentSize clear); the EE destroys
 * the WR's underlying handle through THIS collector's
 * IGCHandleManager::DestroyHandleOfType and the entry is dropped,
 * never promoted, no finalizer; (2) else FINALIZER_RUN (ObjHeader bit
 * 0x40000000) set -> drop + ClrBit (the SuppressFinalize path:
 * RhSuppressFinalize vcalls SetFinalizationRun = SetBit;
 * ReRegisterForFinalize vcalls RegisterForFinalization = ClrBit-if-set
 * else re-append); (3) else -> move to f-reachable, CRITICAL
 * finalizers (MT flags & 0x80000002 == 2) to their own class drained
 * AFTER all normal ones. Then ONE promote pass marks the WHOLE
 * f-reachable set (old undrained + new) transitively -- position:
 * after the moves, inside ScanForFinalization, i.e. AFTER short-weak
 * nulling (a ws referent kept alive only by a pending finalizer
 * already reads null -- the runtime's semantics: weak handles do not
 * track resurrection).
 *
 * The drain side: RhpGetNextFinalizableObject loops vcalling THIS
 * collector's GetNextFinalizable and EE-side honors FINALIZER_RUN (set
 * -> ClrBit + skip); it runs COOPERATIVE (no transition in the body),
 * so SuspendEE serializes the drain against this pipeline and the pop
 * needs no extra guard beyond the alloc lock (vs concurrent appends).
 *
 * Built with -DMGC_BREAK_WEAK, weak nulling alone is suppressed (both
 * phases and the dependent clearing); partition, promote and sweep all
 * run, and a weak reference to a dead object reads alive where the
 * unbroken build reads it dead.
 */

#include "mgc_env.h"
#include "gcinterface.h"
#include "gcenv.ee.h"
#include "gcenv.object.h"
#include "gcdesc.h"

#include <stdarg.h>
#include <stdio.h>
#include <string.h>

extern "C" void abort(void);
extern "C" int  sched_yield(void);
extern "C" void* mmap64(void* addr, size_t len, int prot, int flags,
                        int fd, uint64_t off);
#define GLIBC_PROT_RW       0x3
#define GLIBC_MAP_ANON_PRIV 0x22
/* The collector's own mapping entry (the shim's rhu_shim_collector_mmap):
 * the mark bitmap and the mark stack are served from the window the
 * managed heap lives in, as the heap's region is; the two-range posture's
 * second bitmap keeps mmap64. */
extern "C" void* rhu_shim_collector_mmap(size_t len);

/* the allocator (mgc_heap.cpp) */
extern "C" void     mgc_heap_range(uint8_t** lo, uint8_t** hi);
extern "C" uint32_t mgc_heap_used_bytes(void);
extern "C" void     mgc_heap_census(const char* why);
extern "C" void     mgc_heap_counts(uint32_t* allocs, uint32_t* pads);
extern "C" uint32_t mgc_heap_finq_snapshot(Object*** out);
#ifdef MGC_FRAG_CENSUS
extern "C" void     mgc_heap_frag_line(unsigned long index); /* frag observability */
#endif

#ifdef MGC_SWEEP_LIVE
/* the free-list allocator's surface (mgc_heap.cpp) */
extern "C" uint32_t mgc_finq_bound(void);
extern "C" Object*  mgc_finq_at(uint32_t i, int* state);
extern "C" void     mgc_finq_set_state(uint32_t i, int state);
extern "C" uint32_t mgc_freach_count(void);
extern "C" void     mgc_heap_free_reset(void);
extern "C" void     mgc_heap_free_add(uint8_t* q, uint32_t bytes);
extern "C" void     mgc_heap_free_done(uint32_t live_objs);
extern "C" uint32_t mgc_heap_walk_expected(void);
extern "C" uint32_t mgc_heap_free_bytes(void);
#ifdef MGC_COMPACT
/* Compaction surface (mgc_heap.cpp, all knob'd) */
extern "C" uint8_t*     mgc_heap_bump(void);
extern "C" void         mgc_heap_bump_set(uint8_t* p);
extern "C" MethodTable* mgc_heap_freemt(void);
extern "C" void         mgc_finq_fix(uint32_t i, Object* o);
#endif
/* finq states (mirror of mgc_heap.cpp) */
#define MGC_FQ_FREE        0
#define MGC_FQ_REG         1
#define MGC_FQ_FREACH      2
#define MGC_FQ_FREACH_CRIT 3
#ifdef MGC_TUNE_LIVE
extern "C" void mgc_heap_window_fix(gc_alloc_context* acontext);
#endif
#endif

/* the skeleton (mgc_skeleton.cpp): handle pool + frozen segs */
extern "C" void mgc_skel_handles_foreach(
    void (*fn)(unsigned idx, Object** pslot, Object** psecondary,
               int type, void* arg), void* arg);
/* returns 1=filled, 0=dead slot (skip), -1=past the last segment */
extern "C" int mgc_skel_frozen_get(unsigned i, uint8_t** base,
                                   size_t* first, size_t* allocated);

/* The log-door stage: one line of the collector's log on the kernel's LOG
 * stream (mgc_env.h): the image's level read once, the line formatted only
 * when it will print, ONE write through the door. */
static void mgc_log(uint32_t level, const char *fmt, ...) __attribute__((format(printf, 2, 3)));
static void mgc_log(uint32_t level, const char *fmt, ...)
{
    static int32_t image_level = 0;
    if (image_level == 0)
        image_level = rhu_log_level();
    if ((int32_t)level > image_level)
        return;
    char line[256];
    va_list ap;
    va_start(ap, fmt);
    int n = vsnprintf(line, sizeof line, fmt, ap);
    va_end(ap);
    if (n <= 0)
        return;
    if ((size_t)n >= sizeof line)
        n = (int)sizeof line - 1;
    rhu_log_write(level, line, (uint32_t)n);
}

#define CLLOG(fmt, ...) mgc_log(MGC_LOG_VERBOSE, "[mgc-coll] " fmt "\n", ##__VA_ARGS__)

static void coll_failfast(const char* what)
{
    mgc_log(MGC_LOG_SILENT, "[mgc-coll] FAIL-FAST: %s\n", what);
    mgc_heap_census("collector fail-fast");
    abort();
}

/* ------------------------------------------------------------------ */
/* knobs                                                              */
/* ------------------------------------------------------------------ */

#ifndef MGC_MARK_STACK_ENTRIES  /* explicit mark stack; overflow is a
                                 * loud fail-fast, high-water in census */
#define MGC_MARK_STACK_ENTRIES 8192u
#endif

#if defined(MGC_COMPACT_ALWAYS) && !defined(MGC_COMPACT)
#error "MGC_COMPACT_ALWAYS is a test mode of MGC_COMPACT (pass both)"
#endif
#if defined(MGC_COMPACT) && !defined(MGC_SWEEP_LIVE)
#error "MGC_COMPACT requires MGC_SWEEP_LIVE (it replaces the sweep)"
#endif
#if defined(MGC_COMPACT) && !defined(MGC_HEAP_BYTES)
#error "MGC_COMPACT sizes its anchor tables from MGC_HEAP_BYTES (pass it)"
#endif
#ifdef MGC_COMPACT
#ifndef MGC_COMPACT_MAX_PINS    /* pinned plugs per compaction; overflow
                                 * = loud fallback to the plain sweep */
#define MGC_COMPACT_MAX_PINS 64u
#endif
#define CP_CHUNK 512u           /* forwarding anchor granularity */
#ifndef MGC_COMPACT_MAX_ZONES   /* slide zones = immobile objects + 1
                                 * (immobile = transient pins + POH);
                                 * overflow = loud plain-sweep fallback */
#define MGC_COMPACT_MAX_ZONES 512u
#endif
#ifndef MGC_OOM_RESERVE         /* mirror of mgc_heap.cpp's default -- the
                                 * evac dest-cap honors the reserve */
#define MGC_OOM_RESERVE (64u * 1024u)
#endif
#endif

/* ------------------------------------------------------------------ */
/* state                                                              */
/* ------------------------------------------------------------------ */

static uint8_t* g_lo;           /* heap region bounds (from mgc_heap) */
static uint8_t* g_hi;
static uint8_t* g_bm;           /* mark bitmap: 1 bit / 4 bytes */
static uint32_t g_bm_bytes;

#ifdef MGC_MULTIRANGE
/* Multi-range: the range table's collector-side mirror (bases/ends are
 * init-fixed; bumps are fetched per use, world stopped). g_lo/g_hi and
 * g_bm stay RANGE 0's cells; range 1 gets its own bitmap. */
extern "C" uint32_t mgc_heap_ranges(void);
extern "C" void     mgc_heap_range_get(uint32_t, uint8_t**, uint8_t**, uint8_t**);
extern "C" void     mgc_heap_gap_verify(const char*);
#ifdef MGC_MR_SPLIT_STOMP
extern "C" void     mgc_heap_stomp_gc(int enter);
#endif
static uint8_t* g_r1base;
static uint8_t* g_r1end;
static uint8_t* g_r1bm;         /* range 1 mark bitmap */
static uint32_t g_r1bm_bytes;
static uint32_t g_walk_r[2];    /* per-range walk counts (census) */

/* Walk end for range i = its bump, fetched live. Built with
 * MGC_BREAK_RANGE_TABLE, the table under-reports range 1's end by 4096 B,
 * and the walk cross-check (the count against the allocator model, or a
 * mid-object stop) fails. */
static uint8_t* range_walk_end(uint32_t i)
{
    uint8_t *b, *e, *p;
    mgc_heap_range_get(i, &b, &e, &p);
#ifdef MGC_BREAK_RANGE_TABLE
    if (i == 1 && (uintptr_t)(p - b) > 4100u) {
        static int armed;
        if (!armed++)
            CLLOG("BREAK_RANGE_TABLE: range 1 walk end UNDER-REPORTED by 4096 B (negative control)");
        p -= 4096;
    }
#endif
    return p;
}
#endif /* MGC_MULTIRANGE */
static Object** g_stk;          /* explicit mark stack */
static uint32_t g_stk_cap, g_sp, g_sp_hw;

/* THE OVERFLOW PROTOCOL: a full mark stack DEFERS the push. The object is marked (its bit
 * was set by the caller) and its references wait for the rescan of the
 * window [g_ovf_lo, g_ovf_hi) the deferred objects span; the drain
 * rescans the window's marked objects -- the mark bitmap is the index of
 * marked object starts -- round by round until nothing is deferred.
 * Nothing grows; the counters are the census's and reset per
 * collection. MGC_BREAK_MARK_OVERFLOW compiles the recovery out and the
 * fail-fast back in (the negative control's knob). */
static unsigned char g_ovf_flag;
static uint8_t* g_ovf_lo;
static uint8_t* g_ovf_hi;
static uint32_t g_ovf_deferred, g_ovf_rounds, g_ovf_rescanned;

static volatile unsigned char g_gc_busy;   /* concurrent-entry guard */

static uint32_t g_count[3];     /* per-generation collection counts */
static uint32_t g_total;        /* total collections */

/* per-collection census */
struct MarkCensus {
    uint32_t roots_stack;       /* promote calls inside GcScanRoots */
    uint32_t roots_handle;      /* strong-class handle slots marked */
    uint32_t roots_handle_weak; /* weak-class slots skipped (not roots) */
    uint32_t roots_dep_marked;  /* dependent secondaries marked (fixpoint) */
    uint32_t dep_seen;          /* live dependent handles examined */
    uint32_t roots_finq;        /* finalize-registration entries marked */
    uint32_t frozen_objs;       /* frozen objects walked */
    uint32_t frozen_edges;      /* frozen->heap edges marked */
    uint32_t interior;          /* GC_CALL_INTERIOR qualifiers */
    uint32_t interior_unres;    /* interior that resolved to no object */
    uint32_t pinned;            /* GC_CALL_PINNED qualifiers */
    uint32_t nulls;             /* null root slots */
    uint32_t out_of_heap;       /* non-null targets outside the region */
    uint32_t already;           /* re-marks (idempotent hits) */
    uint32_t marked;            /* objects marked */
    uint32_t marked_bytes;      /* their summed (aligned) sizes */
    uint32_t ctx_seen;          /* alloc contexts enumerated */
    uint32_t ctx_nonzero;       /* contexts with a live window (expect 0) */
};
static MarkCensus g_mc;
static uint32_t g_roots_total;  /* all root-phase marks, the gate number */

/* phase tag so promote/edge accounting lands in the right census slot */
enum Phase { PH_STACK, PH_HANDLE, PH_DEP, PH_FINQ, PH_FROZEN, PH_TRACE };
static Phase g_phase;

#ifdef MGC_COMPACT
/* --- Compaction state (forwarding lives HERE, never in the -4
 * header word -- that word carries live hash/thin-lock/finrun state
 * and moves WITH the object) --- */
static int      g_compact_req;    /* one-shot: set by the alloc OOM path */
static int      g_compact_this;   /* this collection compacts */
static Object*  g_pins[MGC_COMPACT_MAX_PINS];
static uint32_t g_pins_n;
static int      g_pins_over;      /* cap blown -> loud plain-sweep fallback */
/* forwarding anchors: first live object STARTING in each CP_CHUNK of
 * old-address space, as region-relative offsets (+1 so 0 = none), plus
 * the zone index at that anchor. A lookup replays the plan rules from
 * the anchor -- O(objs per chunk). */
#if defined(MGC_MULTIRANGE) && (MGC_HEAP2_BYTES > MGC_HEAP_BYTES)
#define CP_ANCH_SPAN MGC_HEAP2_BYTES    /* evac source can be either range */
#else
#define CP_ANCH_SPAN MGC_HEAP_BYTES
#endif
static uint32_t g_anch_old[CP_ANCH_SPAN / CP_CHUNK];
static uint32_t g_anch_new[CP_ANCH_SPAN / CP_CHUNK];
static uint16_t g_anch_zidx[CP_ANCH_SPAN / CP_CHUNK];
#ifdef MGC_MULTIRANGE
static uint32_t g_anch_run[CP_ANCH_SPAN / CP_CHUNK];  /* evac dest-run cursor */
#endif
/* POH bitmap (mgc_heap.cpp): allocation-time immobility contract */
extern "C" uint8_t* mgc_heap_poh_bitmap(void);
static uint8_t* g_poh;
/* compact census (printed on the compact line) */
static uint32_t g_cp_moved, g_cp_moved_bytes, g_cp_pads, g_cp_zones,
                g_cp_zones_ident, g_cp_holes, g_cp_fix_heap, g_cp_fix_root,
                g_cp_fix_handle, g_cp_fix_finq, g_cp_fix_interior;

extern "C" void mgc_compact_request(void) { g_compact_req = 1; }

static void pin_add(Object* o)
{
    if (!o || !g_compact_this) return;
    if (g_pins_n >= MGC_COMPACT_MAX_PINS) { g_pins_over = 1; return; }
    g_pins[g_pins_n++] = o;
}

static void pins_sort_unique(void)
{
    for (uint32_t i = 1; i < g_pins_n; i++) {      /* insertion sort */
        Object* v = g_pins[i];
        uint32_t j = i;
        while (j && g_pins[j - 1] > v) { g_pins[j] = g_pins[j - 1]; j--; }
        g_pins[j] = v;
    }
    uint32_t w = 0;
    for (uint32_t i = 0; i < g_pins_n; i++)
        if (!w || g_pins[w - 1] != g_pins[i]) g_pins[w++] = g_pins[i];
    g_pins_n = w;
}

static int is_pinned_obj(Object* o)                 /* binary search */
{
    uint32_t lo = 0, hi = g_pins_n;
    while (lo < hi) {
        uint32_t m = (lo + hi) >> 1;
        if (g_pins[m] < o) lo = m + 1;
        else if (g_pins[m] > o) hi = m;
        else return 1;
    }
    return 0;
}

#ifdef MGC_MULTIRANGE
extern "C" uint8_t* mgc_heap_poh_bitmap2(void);
extern "C" uint8_t* mgc_heap_bump_r(uint32_t r);
extern "C" void     mgc_heap_bump_set_r(uint32_t r, uint8_t* p);
static uint8_t* g_poh2;
#endif

static inline int poh_test(Object* o)
{
#ifdef MGC_MULTIRANGE
    if ((uint8_t*)o < g_hi) {
        uint32_t idx = (uint32_t)((uint8_t*)o - g_lo) >> 2;
        return (g_poh[idx >> 3] >> (idx & 7u)) & 1u;
    } else {
        uint32_t idx = (uint32_t)((uint8_t*)o - g_r1base) >> 2;
        return (g_poh2[idx >> 3] >> (idx & 7u)) & 1u;
    }
#else
    uint32_t idx = (uint32_t)((uint8_t*)o - g_lo) >> 2;
    return (g_poh[idx >> 3] >> (idx & 7u)) & 1u;
#endif
}

static inline void poh_clear(Object* o)
{
#ifdef MGC_MULTIRANGE
    if ((uint8_t*)o < g_hi) {
        uint32_t idx = (uint32_t)((uint8_t*)o - g_lo) >> 2;
        g_poh[idx >> 3] = (uint8_t)(g_poh[idx >> 3] & ~(1u << (idx & 7u)));
    } else {
        uint32_t idx = (uint32_t)((uint8_t*)o - g_r1base) >> 2;
        g_poh2[idx >> 3] = (uint8_t)(g_poh2[idx >> 3] & ~(1u << (idx & 7u)));
    }
#else
    uint32_t idx = (uint32_t)((uint8_t*)o - g_lo) >> 2;
    g_poh[idx >> 3] = (uint8_t)(g_poh[idx >> 3] & ~(1u << (idx & 7u)));
#endif
}

/* immobile = zone boundary: transient pin (stack/handle/frozen-edge)
 * or POH-allocated (permanent contract) */
static inline int cp_immobile(Object* o)
{
    return poh_test(o) || is_pinned_obj(o);
}
#endif /* MGC_COMPACT */

/* ------------------------------------------------------------------ */
/* object geometry (vendored mirrors; MT words are never dirtied)     */
/* ------------------------------------------------------------------ */

static inline uint32_t obj_exact_size(Object* o)
{
    MethodTable* mt = o->RawGetMethodTable();
    uint32_t s = mt->GetBaseSize();
    if (mt->HasComponentSize())
        s += ((ArrayBase*)o)->GetNumComponents()
             * (uint32_t)mt->RawGetComponentSize();
    return s;
}

static inline uint32_t obj_step(Object* o)
{
    return (obj_exact_size(o) + 3u) & ~3u;
}

static inline int in_heap(const void* p)
{
#ifdef MGC_MULTIRANGE
    /* exact per-range membership: the gap between ranges is NOT heap
     * (a gap pointer passing the EE's union-bounds filter dies here) */
    return ((const uint8_t*)p >= g_lo + 4 && (const uint8_t*)p < g_hi)
        || ((const uint8_t*)p >= g_r1base + 4 && (const uint8_t*)p < g_r1end);
#else
    return (const uint8_t*)p >= g_lo + 4 && (const uint8_t*)p < g_hi;
#endif
}

/* mark bit for a 4-aligned object start; returns prior state */
static inline int mark_test_set(Object* o)
{
#ifdef MGC_MULTIRANGE
    /* callers guarantee in_heap(o): a two-way branch picks the range */
    uint8_t* bm;
    uint32_t idx;
    if ((uint8_t*)o < g_hi) { bm = g_bm;   idx = (uint32_t)((uint8_t*)o - g_lo) >> 2; }
    else                    { bm = g_r1bm; idx = (uint32_t)((uint8_t*)o - g_r1base) >> 2; }
    uint8_t* byte = bm + (idx >> 3);
    uint8_t  bit  = (uint8_t)(1u << (idx & 7u));
    if (*byte & bit) return 1;
    *byte = (uint8_t)(*byte | bit);
    return 0;
#else
    uint32_t idx  = (uint32_t)((uint8_t*)o - g_lo) >> 2;
    uint8_t* byte = g_bm + (idx >> 3);
    uint8_t  bit  = (uint8_t)(1u << (idx & 7u));
    if (*byte & bit) return 1;
    *byte = (uint8_t)(*byte | bit);
    return 0;
#endif
}

static inline int is_marked(Object* o)
{
#ifdef MGC_MULTIRANGE
    if ((uint8_t*)o < g_hi) {
        uint32_t idx = (uint32_t)((uint8_t*)o - g_lo) >> 2;
        return (g_bm[idx >> 3] >> (idx & 7u)) & 1u;
    } else {
        uint32_t idx = (uint32_t)((uint8_t*)o - g_r1base) >> 2;
        return (g_r1bm[idx >> 3] >> (idx & 7u)) & 1u;
    }
#else
    uint32_t idx = (uint32_t)((uint8_t*)o - g_lo) >> 2;
    return (g_bm[idx >> 3] >> (idx & 7u)) & 1u;
#endif
}

/* ------------------------------------------------------------------ */
/* mark machinery                                                     */
/* ------------------------------------------------------------------ */

static void mark_push(Object* o)
{
    if (g_sp == g_stk_cap) {
#ifdef MGC_BREAK_MARK_OVERFLOW
        CLLOG("mark stack OVERFLOW at %lu entries (raise MGC_MARK_STACK_ENTRIES)",
              (unsigned long)g_stk_cap);
        coll_failfast("mark stack overflow");
#else
        /* Deferred (the overflow protocol): the caller set the object's mark bit; the
         * window widens to cover the object and the drain rescans the
         * window's marked objects. */
        uint8_t* lo = (uint8_t*)o;
        uint8_t* hi = lo + obj_step(o);
        if (!g_ovf_flag || lo < g_ovf_lo) g_ovf_lo = lo;
        if (!g_ovf_flag || hi > g_ovf_hi) g_ovf_hi = hi;
        g_ovf_flag = 1;
        g_ovf_deferred++;
        return;
#endif
    }
    g_stk[g_sp++] = o;
    if (g_sp > g_sp_hw) g_sp_hw = g_sp;
}

/* one edge: target must be a heap object start (or null / foreign) */
static void mark_edge(Object* t)
{
    if (!t) { g_mc.nulls++; return; }
    if (!in_heap(t)) { g_mc.out_of_heap++; return; }
#ifdef MGC_COMPACT
    /* frozen->heap edges live in flash slots a mover cannot rewrite:
     * the target must never move (capture BEFORE the already-marked
     * early-out -- a re-marked target still pins) */
    if (g_phase == PH_FROZEN) pin_add(t);
#endif
    if (mark_test_set(t)) { g_mc.already++; return; }
    g_mc.marked++;
    g_mc.marked_bytes += obj_step(t);
    switch (g_phase) {
    case PH_STACK:  g_mc.roots_stack++;      g_roots_total++; break;
    case PH_HANDLE: g_mc.roots_handle++;     g_roots_total++; break;
    case PH_DEP:    g_mc.roots_dep_marked++; g_roots_total++; break;
    case PH_FINQ:   g_mc.roots_finq++;       g_roots_total++; break;
    case PH_FROZEN: g_mc.frozen_edges++;     g_roots_total++; break;
    case PH_TRACE:  break;
    }
    mark_push(t);
}

/* GCDesc reference walk (the vendored gcdesc.h recipe: GetNumPointers
 * / the series-bias comment; negative count walked per-component
 * exactly NumComponents times). */
static void scan_refs(Object* o)
{
    MethodTable* mt = o->RawGetMethodTable();
    if (!mt->ContainsGCPointers()) return;

    uint32_t  size = obj_exact_size(o);
    CGCDesc*  map  = CGCDesc::GetCGCDescFromMT(mt);
    ptrdiff_t cnt  = (ptrdiff_t)map->GetNumSeries();

    if (cnt >= 0) {
        CGCDescSeries* cur  = map->GetHighestSeries();
        CGCDescSeries* last = map->GetLowestSeries();
        do {
            uint8_t* parm = (uint8_t*)o + cur->GetSeriesOffset();
            uint8_t* stop = parm + cur->GetSeriesSize() + size;
            while (parm < stop) {
                mark_edge(*(Object**)parm);
                parm += sizeof(void*);
            }
            cur--;
        } while (cur >= last);
    } else {
        /* array of valuetypes-with-refs (e.g. Dictionary.Entry[]) */
        CGCDescSeries* cur   = map->GetHighestSeries();
        uint32_t       ncomp = ((ArrayBase*)o)->GetNumComponents();
        uint8_t*       parm  = (uint8_t*)o + cur->GetSeriesOffset();
        for (uint32_t c = 0; c < ncomp; c++) {
            for (ptrdiff_t i = 0; i > cnt; i--) {
                val_serie_item* it = cur->val_serie + i;
                for (uint32_t n = 0; n < it->nptrs; n++) {
                    mark_edge(*(Object**)parm);
                    parm += sizeof(void*);
                }
                parm += it->skip;
            }
        }
    }
}

#ifndef MGC_BREAK_MARK_OVERFLOW
/* One rescan round over one range's bitmap: every set bit inside [lo, hi)
 * is a marked object start (mark_test_set has one caller, mark_edge, and
 * every edge is an object start); its references are scanned -- an
 * already-marked child returns at mark_test_set, an unmarked one is
 * marked and pushed, or deferred into the next window. */
static void rescan_bitmap(const uint8_t* bm, uint8_t* base, uint8_t* end,
                          uint8_t* lo, uint8_t* hi)
{
    if (lo < base) lo = base;
    if (hi > end) hi = end;
    if (lo >= hi) return;
    uint32_t i0 = (uint32_t)(lo - base) >> 2;
    uint32_t i1 = (uint32_t)(hi - base + 3u) >> 2;     /* exclusive */
    for (uint32_t byte = i0 >> 3; byte < ((i1 + 7u) >> 3); byte++) {
        uint8_t bits = bm[byte];
        if (!bits) continue;
        for (uint32_t b = 0; b < 8u; b++) {
            if (!(bits & (uint8_t)(1u << b))) continue;
            uint32_t idx = (byte << 3) | b;
            if (idx < i0 || idx >= i1) continue;
            g_ovf_rescanned++;
            scan_refs((Object*)(base + (idx << 2)));
        }
    }
}

static void rescan_window(uint8_t* lo, uint8_t* hi)
{
    rescan_bitmap(g_bm, g_lo, g_hi, lo, hi);
#ifdef MGC_MULTIRANGE
    rescan_bitmap(g_r1bm, g_r1base, g_r1end, lo, hi);
#endif
}
#endif /* !MGC_BREAK_MARK_OVERFLOW */

static void mark_drain(void)
{
    Phase saved = g_phase;
    g_phase = PH_TRACE;
    for (;;) {
        while (g_sp)
            scan_refs(g_stk[--g_sp]);
#ifdef MGC_BREAK_MARK_OVERFLOW
        break;
#else
        if (!g_ovf_flag)
            break;
        /* A round (the overflow protocol): the window taken and reset, its marked objects
         * rescanned (pushing, or deferring into the next window), then
         * the stack drained again. Every round marks new objects or
         * defers none, so the rounds end. */
        uint8_t* lo = g_ovf_lo;
        uint8_t* hi = g_ovf_hi;
        g_ovf_flag = 0;
        g_ovf_lo = g_ovf_hi = nullptr;
        g_ovf_rounds++;
        rescan_window(lo, hi);
#endif
    }
    g_phase = saved;
}

/* ------------------------------------------------------------------ */
/* heap walk: the structural cross-check + interior resolution        */
/* ------------------------------------------------------------------ */

/* Walk the bump heap [lo+4, bump_end) by MT-derived sizes. The object
 * count must equal allocs+pads from the allocator's own census -- this
 * proves the size formula and pad discipline agree with the real
 * layout BEFORE the tracer relies on them. Fail-fast on divergence:
 * tracing with a wrong size formula is silent corruption. */
#ifdef MGC_MULTIRANGE
static uint32_t heap_walk_verify(void)
{
    uint32_t n = 0;
    for (uint32_t r = 0; r < mgc_heap_ranges(); r++) {
        uint8_t *base, *tend, *bump;
        mgc_heap_range_get(r, &base, &tend, &bump);
        uint8_t* end = range_walk_end(r);
        uint8_t* p   = base + 4;
        uint32_t rn  = 0;
        while (p < end) {
            Object*  o    = (Object*)p;
            uint32_t step = obj_step(o);
            if (step < 12u || step > (uint32_t)(end - p)) {
                CLLOG("heap walk CORRUPT at %p (range %lu, object #%lu, step=%lu, MT=%p)",
                      (void*)p, (unsigned long)r, (unsigned long)(rn + 1),
                      (unsigned long)step, (void*)o->RawGetMethodTable());
                coll_failfast("heap walk corrupt (size formula vs layout)");
            }
            rn++;
            p += step;
        }
        g_walk_r[r] = rn;
        n += rn;
    }
    return n;
}

/* interior pointer -> containing object (GC_CALL_INTERIOR): find the
 * range whose live span holds p, then walk THAT range only */
static Object* interior_resolve(const void* p)
{
    for (uint32_t r = 0; r < mgc_heap_ranges(); r++) {
        uint8_t *base, *tend, *bump;
        mgc_heap_range_get(r, &base, &tend, &bump);
        uint8_t* end = range_walk_end(r);
        if ((const uint8_t*)p < base + 4 || (const uint8_t*)p >= end)
            continue;
        uint8_t* q = base + 4;
        while (q < end) {
            Object*  o    = (Object*)q;
            uint32_t step = obj_step(o);
            if ((const uint8_t*)p < q + step)
                return o;
            q += step;
        }
        return nullptr;
    }
    return nullptr;
}
#else
static uint32_t heap_walk_verify(void)
{
    uint8_t* end = g_lo + mgc_heap_used_bytes();
    uint8_t* p   = g_lo + 4;
    uint32_t n   = 0;
    while (p < end) {
        Object*  o    = (Object*)p;
        uint32_t step = obj_step(o);
        if (step < 12u || step > (uint32_t)(end - p)) {
            CLLOG("heap walk CORRUPT at %p (object #%lu, step=%lu, MT=%p)",
                  (void*)p, (unsigned long)(n + 1), (unsigned long)step,
                  (void*)o->RawGetMethodTable());
            coll_failfast("heap walk corrupt (size formula vs layout)");
        }
        n++;
        p += step;
    }
    return n;
}

/* interior pointer -> containing object (GC_CALL_INTERIOR), linear */
static Object* interior_resolve(const void* p)
{
    if ((const uint8_t*)p < g_lo + 4 || (const uint8_t*)p >= g_lo + mgc_heap_used_bytes())
        return nullptr;
    uint8_t* end = g_lo + mgc_heap_used_bytes();
    uint8_t* q   = g_lo + 4;
    while (q < end) {
        Object*  o    = (Object*)q;
        uint32_t step = obj_step(o);
        if ((const uint8_t*)p < q + step)
            return o;
        q += step;
    }
    return nullptr;
}
#endif /* MGC_MULTIRANGE */

/* ------------------------------------------------------------------ */
/* the EE-facing promote callback (GcScanRoots)                       */
/* ------------------------------------------------------------------ */

static void mgc_promote(Object** ppObject, ScanContext* sc, uint32_t flags)
{
    (void)sc;
    Object* o = ppObject ? *ppObject : nullptr;
    if (!o) { g_mc.nulls++; return; }
    if (flags & GC_CALL_PINNED) g_mc.pinned++;   /* non-moving: no-op */
    if (flags & GC_CALL_INTERIOR) {
        g_mc.interior++;
        o = interior_resolve(o);
        if (!o) { g_mc.interior_unres++; return; }
    }
#ifdef MGC_COMPACT
    if (flags & GC_CALL_PINNED) pin_add(o);      /* the mover honors pins;
                                                  * interior resolved first
                                                  * so the CONTAINER pins */
#endif
    mark_edge(o);
}

/* alloc-context fix callback (GcEnumAllocContexts): a build without
 * MGC_TUNE_LIVE hands out no windows, so contexts must be {0,0}; the
 * EE-side wrapper zeroes combined_limit after this callback returns.
 * Counted as evidence. */
static void mgc_fix_alloc_context(gc_alloc_context* acontext, void* param)
{
    (void)param;
    g_mc.ctx_seen++;
    if (acontext->alloc_ptr || acontext->alloc_limit) {
        g_mc.ctx_nonzero++;
#ifdef MGC_TUNE_LIVE
        /* Retire the window properly -- count its births into
         * the walk model, plug the remainder, refund the charge
         * (mgc_heap.cpp; world is stopped, the lock is free) */
        mgc_heap_window_fix(acontext);
#else
        /* a window exists on a build that never hands one out: retire it */
        acontext->alloc_ptr = nullptr;
        acontext->alloc_limit = nullptr;
#endif
    }
}

/* ------------------------------------------------------------------ */
/* root phases                                                        */
/* ------------------------------------------------------------------ */

static void handle_visit(unsigned idx, Object** pslot, Object** psecondary,
                         int type, void* arg)
{
    (void)idx; (void)psecondary; (void)arg;
    /* HandleType (the interface's numbering): 0=weak-short 1=weak-long
     * 2=strong 3=pinned 4=variable 5=refcounted 6=dependent
     * 7=async-pinned 8=sizedref 9=weak-native-com 10=weak-interior
     * 11=crossreference. Weak classes are NOT roots (and, without
     * MGC_SWEEP_LIVE, not nulled either -- nothing dies). Dependent is
     * the fixpoint phase. Everything else holds its target live;
     * unexpected types
     * get a loud first-sight line and conservative strong treatment
     * (always mark-safe). */
    switch (type) {
    case 0: case 1: case 9: case 10:
        g_mc.roots_handle_weak++;
        return;
    case 6:
        g_mc.dep_seen++;
        return; /* fixpoint phase */
#ifdef MGC_COMPACT
    case 3: case 7:      /* pinned classes: strong AND immovable */
        if (*pslot && in_heap(*pslot)) pin_add(*pslot);
        break;
    case 2: case 8:
        break;
#else
    case 2: case 3: case 7: case 8:
        break;
#endif
    default: {
        static uint32_t warned;
        if (warned++ < 4)
            CLLOG("handle type %d UNEXPECTED on this profile -- treated as strong",
                  type);
        break;
    }
    }
    if (*pslot)
        mark_edge(*pslot);
}

struct DepFix { int changed; };

static void dep_visit(unsigned idx, Object** pslot, Object** psecondary,
                      int type, void* arg)
{
    (void)idx;
    DepFix* fx = (DepFix*)arg;
    if (type != 6) return;
    Object* primary   = *pslot;
    Object* secondary = *psecondary;
    if (!primary || !secondary) return;
    /* primary reachable => secondary reachable. A frozen/foreign
     * primary is immortal, hence reachable. */
    int primary_live = in_heap(primary) ? is_marked(primary) : 1;
    if (primary_live && in_heap(secondary) && !is_marked(secondary)) {
        mark_edge(secondary);
        fx->changed = 1;
    }
}

static void scan_frozen_roots(void)
{
    for (unsigned s = 0;; s++) {
        uint8_t* base; size_t first, allocated;
        int rc = mgc_skel_frozen_get(s, &base, &first, &allocated);
        if (rc < 0)
            break;
        if (rc == 0)
            continue;
        uint8_t* p   = base + first;
        uint8_t* end = base + allocated;
        while (p < end) {
            Object* o = (Object*)p;
            g_mc.frozen_objs++;
            scan_refs(o);        /* heap targets become roots; frozen->
                                  * frozen edges count as out_of_heap */
            p += obj_step(o);
        }
        mark_drain();
    }
}

/* ------------------------------------------------------------------ */
/* the death pipeline (-DMGC_SWEEP_LIVE)                              */
/* ------------------------------------------------------------------ */

#ifdef MGC_SWEEP_LIVE
struct DeathCensus {
    uint32_t ws_nulled;         /* short-weak slots nulled */
    uint32_t wl_nulled;         /* long-weak slots nulled */
    uint32_t dep_cleared;       /* dead-primary dependents (both nulled) */
    uint32_t weak_odd_nulled;   /* types 9/10 -- unexpected on profile */
    uint32_t finq_kept;         /* registered, still live */
    uint32_t finq_eager;        /* dead, EagerFinalized (WR handle freed) */
    uint32_t finq_suppressed;   /* dead, FINALIZER_RUN -> dropped+ClrBit */
    uint32_t promoted;          /* dead -> f-reachable (normal) */
    uint32_t promoted_crit;     /* dead -> f-reachable (critical) */
    uint32_t dead_objs;         /* unmarked walkable objects swept over */
    uint32_t swept_bytes;       /* bytes returned to the free list */
    uint32_t runs;              /* coalesced free runs emitted */
    uint32_t live_objs;         /* marked objects surviving */
    uint32_t live_bytes;
};
static DeathCensus g_dc;

struct WeakArg { int long_phase; };

static void weak_visit(unsigned idx, Object** pslot, Object** psecondary,
                       int type, void* arg)
{
    (void)idx;
    WeakArg* wa = (WeakArg*)arg;
    Object* t = *pslot;
    if (!wa->long_phase) {
        /* GcShortWeakPtrScan: ws only (Ref_CheckAlive) */
        if (type == 0 && t && in_heap(t) && !is_marked(t)) {
            *pslot = nullptr;
            g_dc.ws_nulled++;
        }
        return;
    }
    /* GcWeakPtrScan: wl + dependent clearing (+ the never-seen weak
     * classes, nulled loudly if they ever appear) */
    switch (type) {
    case 1:
        if (t && in_heap(t) && !is_marked(t)) {
            *pslot = nullptr;
            g_dc.wl_nulled++;
        }
        return;
    case 6:
        /* dead primary -> null BOTH primary and secondary
         * (Ref_ScanDependentHandlesForClearing's semantics) */
        if (t && in_heap(t) && !is_marked(t)) {
            *pslot = nullptr;
            *psecondary = nullptr;
            g_dc.dep_cleared++;
        }
        return;
    case 9: case 10:
        if (t && in_heap(t) && !is_marked(t)) {
            static uint32_t warned;
            if (warned++ < 4)
                CLLOG("weak handle type %d nulled -- UNEXPECTED on this profile", type);
            *pslot = nullptr;
            g_dc.weak_odd_nulled++;
        }
        return;
    default:
        return;
    }
}

/* The ScanForFinalization partition -- the runtime's branch order
 * (EagerFinalized FIRST, then FINALIZER_RUN, else promote; critical
 * finalizers to their own drain class). */
static void death_scan_for_finalization(void)
{
    uint32_t bound = mgc_finq_bound();
    for (uint32_t i = 0; i < bound; i++) {
        int st;
        Object* o = mgc_finq_at(i, &st);
        if (st != MGC_FQ_REG || !o)
            continue;
        if (!in_heap(o) || is_marked(o)) {
            g_dc.finq_kept++;
            continue;
        }
        /* dead */
        if (GCToEEInterface::EagerFinalized(o)) {
            mgc_finq_set_state(i, MGC_FQ_FREE);
            g_dc.finq_eager++;
            continue;
        }
        ObjHeader* h = (ObjHeader*)o - 1;
        if (h->GetBits() & BIT_SBLK_FINALIZER_RUN) {
            h->ClrBit(BIT_SBLK_FINALIZER_RUN);
            mgc_finq_set_state(i, MGC_FQ_FREE);
            g_dc.finq_suppressed++;
            continue;
        }
        MethodTable* mt = o->RawGetMethodTable();
        int crit = (mt->m_flags
                    & (MTFlag_HasComponentSize | MTFlag_HasCriticalFinalizer))
                   == MTFlag_HasCriticalFinalizer;
        if (crit) {
            mgc_finq_set_state(i, MGC_FQ_FREACH_CRIT);
            g_dc.promoted_crit++;
        } else {
            mgc_finq_set_state(i, MGC_FQ_FREACH);
            g_dc.promoted++;
        }
    }
    /* promote pass: the WHOLE f-reachable set (old undrained + newly
     * moved) is marked transitively -- it must survive this cycle's
     * sweep so its finalizers can run (the runtime's own collector
     * promotes the full range at the end of ScanForFinalization) */
    g_phase = PH_FINQ;
    for (uint32_t i = 0; i < bound; i++) {
        int st;
        Object* o = mgc_finq_at(i, &st);
        if ((st == MGC_FQ_FREACH || st == MGC_FQ_FREACH_CRIT) && o)
            mark_edge(o);
    }
    mark_drain();
}

extern "C" void mgc_heap_bump_set(uint8_t* p);

/* Sweep: one pass over the walkable heap; maximal runs of unmarked
 * objects (dead allocations, old pads, old free runs -- pads and runs
 * are never marked, so stale free space re-coalesces every cycle)
 * become single free objects on the allocator's address-ordered list.
 * Runs are always >=12 (min object) and ==0 mod 4. The one run the bump
 * mark is set back over is the dead run at the top, which is zeroed
 * first -- the virgin bump tail stays pre-zeroed. */
static void death_sweep(void)
{
    mgc_heap_free_reset();
#ifdef MGC_MULTIRANGE
    /* per-range sweep, ranges ascending: the ONE address-ordered free
     * list stays ordered across ranges, and a run can never span the
     * untabled gap (each range resets its own run) */
    for (uint32_t r = 0; r < mgc_heap_ranges(); r++) {
        uint8_t *base, *tend, *bump;
        mgc_heap_range_get(r, &base, &tend, &bump);
        uint8_t* end = range_walk_end(r);
        uint8_t* p   = base + 4;
        uint8_t* run = nullptr;
        while (p < end) {
            Object*  o    = (Object*)p;
            uint32_t step = obj_step(o);
            if (is_marked(o)) {
                if (run) {
                    mgc_heap_free_add(run, (uint32_t)(p - run));
                    g_dc.runs++;
                    g_dc.swept_bytes += (uint32_t)(p - run);
                    run = nullptr;
                }
                g_dc.live_objs++;
                g_dc.live_bytes += step;
            } else {
                g_dc.dead_objs++;
                if (!run)
                    run = p;
            }
            p += step;
        }
        if (run) {
#ifdef MGC_COMPACT
            /* compose: the trailing run RETRACTS into virgin
             * tail (re-zeroed, header word incl.) instead of joining
             * the free list -- tails self-restore as top objects die,
             * which is what lets tail-destination evacuation
             * bootstrap out of a packed heap. */
            memset(run - 4, 0, (size_t)(end - run) + 4u);
            mgc_heap_bump_set_r(r, run);
            g_dc.swept_bytes += (uint32_t)(end - run);
#else
            mgc_heap_free_add(run, (uint32_t)(end - run));
            g_dc.runs++;
            g_dc.swept_bytes += (uint32_t)(end - run);
#endif
        }
    }
#else
    uint8_t* end = g_lo + mgc_heap_used_bytes();
    uint8_t* p   = g_lo + 4;
    uint8_t* run = nullptr;
    while (p < end) {
        Object*  o    = (Object*)p;
        uint32_t step = obj_step(o);
        if (is_marked(o)) {
            if (run) {
                mgc_heap_free_add(run, (uint32_t)(p - run));
                g_dc.runs++;
                g_dc.swept_bytes += (uint32_t)(p - run);
                run = nullptr;
            }
            g_dc.live_objs++;
            g_dc.live_bytes += step;
        } else {
            g_dc.dead_objs++;
            if (!run)
                run = p;
        }
        p += step;
    }
    if (run) {
#ifndef MGC_BREAK_TAIL_REJOIN
        /* The dead run at the top of the used part rejoins the untouched
         * tail: zeroed with the header word before it (the tail's
         * zero-born promise) and the bump mark set back to its start.
         * Every thread's window was retired before the sweep, so no
         * allocation context points into it. A request that fits only
         * across this run and the tail is then served after one
         * collection. */
        memset(run - 4, 0, (size_t)(end - run) + 4u);
        mgc_heap_bump_set(run);
        g_dc.swept_bytes += (uint32_t)(end - run);
        CLLOG("sweep: the dead run at the top (%lu B) rejoined the untouched tail of the managed heap (zeroed, the bump mark set back)",
              (unsigned long)(end - run));
#else
        mgc_heap_free_add(run, (uint32_t)(end - run));
        g_dc.runs++;
        g_dc.swept_bytes += (uint32_t)(end - run);
#endif
    }
#endif /* MGC_MULTIRANGE */
    mgc_heap_free_done(g_dc.live_objs);
}

#ifdef MGC_COMPACT
/* ================================================================== */
/* Sliding compaction within the region.                              */
/* Replaces death_sweep on compacting collections. All passes run STW */
/* on the fresh mark bitmap; forwarding = deterministic replay of the */
/* plan rules from per-chunk anchors (never the -4 header word -- it  */
/* carries live hash/thin-lock/finalizer state and moves WITH the     */
/* object, header-inclusive copies [o-4, o+step-4)).                  */
/*                                                                    */
/* Zones: pinned objects bound slide zones; dest resets to pin_end at */
/* every pin (bounded residual fragmentation around pins, by design). */
/* A zone whose slide would leave a 4/8-byte hole before its pin      */
/* (unwalkable: min free object is 12) falls back to IDENTITY -- its  */
/* objects keep their addresses and original gaps become runs, which  */
/* are always 0 or >=12. "Immobile" = pinned OR in an identity zone;  */
/* plan, forward and move share that one rule. ALIGN8 is preserved by */
/* reproducing the SOURCE parity (dest%8 == src%8) via a 12-byte pad  */
/* -- inherits the allocator's decision, no MT flag interpretation.   */
/* ================================================================== */

static uint8_t g_zone_ident[MGC_COMPACT_MAX_ZONES];

static inline int cp_needs_pad(uint8_t* dest, uint8_t* src, MethodTable* mt)
{
    return mt->RequiresAlign8()
        && (((uintptr_t)dest ^ (uintptr_t)src) & 7u) != 0;
}

#ifdef MGC_MULTIRANGE
/* ================================================================== */
/* THE EVACUATION COMPOSE: on a two-range heap a compacting           */
/* collection EVACUATES one range's movable survivors into the other  */
/* range's fresh bump tail. Structurally simpler than sliding: NO     */
/* zones (source gaps are sums of >=12-byte object steps -- always    */
/* walkable), the dest is virgin (ALIGN8 source-parity pads emitted   */
/* as free objects), and the source bump RETRACTS to its last kept    */
/* immobile with the vacated span re-zeroed (the sliding pass's own   */
/* rule) -- so the ping-pong stays sustainable under COMPACT_ALWAYS.  */
/* Forwarding = anchor replay over the SOURCE range: anch_old =       */
/* source offset+1; anch_new = the RAW running dest pointer at the    */
/* anchor (+1) -- immobile objects do not advance dest, so the running */
/* value is the thing to record, never the object's own dest.         */
/* ================================================================== */

static uint32_t g_ev_src, g_ev_dst;
static uint8_t* g_ev_sbase;
static uint8_t* g_ev_send;      /* source live span end (old bump) */
static uint8_t* g_ev_dbase;
static uint8_t* g_ev_dend;      /* dest tabled end */
static uint8_t* g_ev_dbump;     /* dest bump at plan time */
static uint8_t* g_ev_dcap;      /* dest tail hard cap (reserve honored) */
extern "C" uint8_t* mgc_heap_free_head(void);

/* Destination cursor: dest allocations walk the dest range's free
 * RUNS (an address-ordered sublist of the ONE free list, headers
 * cached before any write) and finish on the virgin tail.
 * Deterministic given (free-list snapshot, source walk order), so
 * anchor replay reproduces it exactly. */
struct EvCur {
    uint8_t* dp;
    uint8_t* run;       /* current run start; null = tail mode */
    uint8_t* run_end;   /* current run end, or the tail cap */
    uint8_t* next_run;  /* cached BEFORE any write into this run */
};

static void ev_cur_load(EvCur* c, uint8_t* run)
{
    while (run && run < g_ev_dbase)
        run = *(uint8_t**)(run + 8);
    if (run && run >= g_ev_dend)
        run = nullptr;
    if (run) {
        c->run      = run;
        c->run_end  = run + 12u + *(uint32_t*)(run + 4);
        c->next_run = *(uint8_t**)(run + 8);
        c->dp       = run;
    } else {
        c->run      = nullptr;
        c->run_end  = g_ev_dcap;
        c->next_run = nullptr;
        c->dp       = g_ev_dbump;
    }
}

static void ev_cur_init(EvCur* c) { ev_cur_load(c, mgc_heap_free_head()); }

/* advance to the next run (or the tail); returns 0 when exhausted.
 * mv!=null = the MOVE pass: close the current position by writing a
 * walkable remainder free object (rem is 0 or >=12 by the placement
 * rule) before leaving it; virgin tail is never scribbled. */
static int ev_cur_advance(EvCur* c, MethodTable* mv)
{
    if (mv && c->run && c->dp < c->run_end) {
        uint32_t rem = (uint32_t)(c->run_end - c->dp);
        if (rem >= 12u) {
            *(MethodTable**)c->dp   = mv;
            *(uint32_t*)(c->dp + 4) = rem - 12u;
        }
    }
    if (!c->run) return 0;                    /* tail exhausted = done */
    uint8_t* nr = c->next_run;
    if (nr && nr >= g_ev_dbase && nr < g_ev_dend) {
        c->run      = nr;
        c->run_end  = nr + 12u + *(uint32_t*)(nr + 4);
        c->next_run = *(uint8_t**)(nr + 8);
        c->dp       = nr;
    } else {
        c->run      = nullptr;
        c->run_end  = g_ev_dcap;
        c->next_run = nullptr;
        c->dp       = g_ev_dbump;
    }
    return 1;
}

/* one placement: returns the object's dest or null (capacity out).
 * The remainder rule (0 or >=12) keeps every consumed run walkable;
 * the tail (run==null) has no remainder constraint. */
static uint8_t* ev_place(EvCur* c, uint8_t* src, MethodTable* mt,
                         uint32_t step, MethodTable* mv)
{
    for (;;) {
        uint32_t pad  = cp_needs_pad(c->dp, src, mt) ? 12u : 0u;
        uint32_t need = pad + step;
        if (c->dp + need <= c->run_end) {
            uint32_t rem = (uint32_t)(c->run_end - (c->dp + need));
            if (!c->run || rem == 0u || rem >= 12u) {
                if (mv && pad) {
                    *(MethodTable**)c->dp   = mv;
                    *(uint32_t*)(c->dp + 4) = 0;
                }
                uint8_t* obj = c->dp + pad;
                c->dp += need;
                return obj;
            }
        }
        if (!ev_cur_advance(c, mv)) return nullptr;
    }
}

/* sizing walk: movable live bytes + worst-case ALIGN8 pads */
static uint32_t evac_sizing(uint32_t r)
{
    uint8_t *base, *tend, *bump;
    mgc_heap_range_get(r, &base, &tend, &bump);
    uint8_t* p = base + 4;
    uint32_t need = 0;
    while (p < bump) {
        Object*  o    = (Object*)p;
        uint32_t step = obj_step(o);
        if (is_marked(o) && !cp_immobile(o)) {
            need += step;
            if (o->RawGetMethodTable()->RequiresAlign8()) need += 12u;
        }
        p += step;
    }
    return need;
}

/* dest free capacity: in-range free runs + the virgin tail */
static uint32_t evac_capacity(uint8_t* base, uint8_t* tend,
                              uint8_t* bump, uint8_t* cap)
{
    uint32_t total = (cap > bump) ? (uint32_t)(cap - bump) : 0u;
    for (uint8_t* run = mgc_heap_free_head(); run;
         run = *(uint8_t**)(run + 8)) {
        if (run >= tend) break;
        if (run >= base) total += 12u + *(uint32_t*)(run + 4);
    }
    return total;
}

/* plan: drive the cursor once, record per-chunk anchors over the
 * SOURCE: (source offset+1, raw dp+1, raw run+1; run 0 = tail).
 * Returns 0 if dest capacity ran out (caller sweeps, loudly). */
static int evac_plan(void)
{
    memset(g_anch_old, 0, sizeof(g_anch_old));
    memset(g_anch_new, 0, sizeof(g_anch_new));
    memset(g_anch_run, 0, sizeof(g_anch_run));
    EvCur c; ev_cur_init(&c);
    uint8_t* p = g_ev_sbase + 4;
    while (p < g_ev_send) {
        Object*  o    = (Object*)p;
        uint32_t step = obj_step(o);
        if (is_marked(o)) {
            uint32_t ci = (uint32_t)(p - g_ev_sbase) / CP_CHUNK;
            if (!g_anch_old[ci]) {
                g_anch_old[ci] = (uint32_t)(p - g_ev_sbase) + 1u;
                g_anch_new[ci] = (uint32_t)(uintptr_t)c.dp + 1u;
                g_anch_run[ci] = (uint32_t)(uintptr_t)c.run + 1u;
            }
            if (!cp_immobile(o)) {
                if (!ev_place(&c, p, o->RawGetMethodTable(), step, nullptr))
                    return 0;
            }
        }
        p += step;
    }
    return 1;
}

static uint8_t* evac_forward(Object* obj)
{
    uint8_t* p = (uint8_t*)obj;
    if (p < g_ev_sbase + 4 || p >= g_ev_send)
        return p;                    /* dest range / frozen / foreign */
    uint32_t ci = (uint32_t)(p - g_ev_sbase) / CP_CHUNK;
    if (!g_anch_old[ci])
        coll_failfast("evac forward: empty anchor chunk for a live target");
    uint8_t* cur = g_ev_sbase + (g_anch_old[ci] - 1u);
    EvCur c;
    {
        uint8_t* arun = (uint8_t*)(uintptr_t)(g_anch_run[ci] - 1u);
        if (arun) {
            c.run      = arun;
            c.run_end  = arun + 12u + *(uint32_t*)(arun + 4);
            c.next_run = *(uint8_t**)(arun + 8);
        } else {
            c.run      = nullptr;
            c.run_end  = g_ev_dcap;
            c.next_run = nullptr;
        }
        c.dp = (uint8_t*)(uintptr_t)(g_anch_new[ci] - 1u);
    }
    for (;;) {
        Object*  co   = (Object*)cur;
        uint32_t step = obj_step(co);
        if (is_marked(co)) {
            if (cp_immobile(co)) {
                if (cur == p) return cur;
            } else {
                uint8_t* nd = ev_place(&c, cur, co->RawGetMethodTable(),
                                       step, nullptr);
                if (!nd)
                    coll_failfast("evac forward: cursor ran out (plan/replay skew)");
                if (cur == p) return nd;
            }
        }
        cur += step;
        if (cur >= g_ev_send)
            coll_failfast("evac forward: target not reached (layout/bitmap skew)");
    }
}

/* range-aware raw mark-bit ops for the transplant */
static inline void ev_bit_set(uint8_t* a)
{
    if (a < g_hi) {
        uint32_t idx = (uint32_t)(a - g_lo) >> 2;
        g_bm[idx >> 3] |= (uint8_t)(1u << (idx & 7u));
    } else {
        uint32_t idx = (uint32_t)(a - g_r1base) >> 2;
        g_r1bm[idx >> 3] |= (uint8_t)(1u << (idx & 7u));
    }
}
static inline void ev_bit_clear(uint8_t* a)
{
    if (a < g_hi) {
        uint32_t idx = (uint32_t)(a - g_lo) >> 2;
        g_bm[idx >> 3] = (uint8_t)(g_bm[idx >> 3] & ~(1u << (idx & 7u)));
    } else {
        uint32_t idx = (uint32_t)(a - g_r1base) >> 2;
        g_r1bm[idx >> 3] = (uint8_t)(g_r1bm[idx >> 3] & ~(1u << (idx & 7u)));
    }
}

/* move: replay the plan, copy header-inclusive, TRANSPLANT the mark
 * bit old->new, write pads and run remainders as walkable free
 * objects. The rebuilt free list, walk model and (the compose's rule)
 * trailing-run retraction all come from the normal per-range
 * death_sweep run AFTER this: moved objects are marked at their NEW
 * addresses, vacated and consumed space is unmarked, and every byte
 * stays walkable. */
static void evac_move(void)
{
    MethodTable* freemt = mgc_heap_freemt();
    EvCur c; ev_cur_init(&c);
    uint8_t* p = g_ev_sbase + 4;
    while (p < g_ev_send) {
        Object*  o    = (Object*)p;
        uint32_t step = obj_step(o);
        if (is_marked(o) && !cp_immobile(o)) {
            uint8_t* nd = ev_place(&c, p, o->RawGetMethodTable(),
                                   step, freemt);
            if (!nd)
                coll_failfast("evac move: cursor ran out (plan/replay skew)");
            memmove(nd - 4, p - 4, step);
            ev_bit_clear(p);
            ev_bit_set(nd);
            g_cp_moved++;
            g_cp_moved_bytes += step;
        }
        p += step;
    }
    /* close the final position (writes the last run remainder; a tail
     * position writes nothing) and publish the new dest bump */
    if (!c.run) {
        if (c.dp > g_ev_dbump)
            mgc_heap_bump_set_r(g_ev_dst, c.dp);
    } else {
        ev_cur_advance(&c, freemt);
    }
}
#endif /* MGC_MULTIRANGE */

#ifndef MGC_MULTIRANGE
/* Pass A: per-zone slide simulation; flags zones whose terminating
 * hole would be 4 or 8 bytes (identity fallback -- unwalkable gap).
 * Zone boundaries = immobile objects in address order; the tail zone
 * has no terminating boundary (no constraint). Returns 0 on zone
 * overflow (caller falls back to the plain sweep, loudly). */
static int cp_sim_zones(uint8_t* end)
{
    memset(g_zone_ident, 0, sizeof(g_zone_ident));
    uint8_t* p     = g_lo + 4;
    uint8_t* dest  = g_lo + 4;
    uint32_t zidx  = 0;
    while (p < end) {
        Object*  o    = (Object*)p;
        uint32_t step = obj_step(o);
        if (is_marked(o)) {
            if (cp_immobile(o)) {
                uint32_t hole = (uint32_t)(p - dest);
                if (hole == 4u || hole == 8u) g_zone_ident[zidx] = 1;
                zidx++;
                if (zidx >= MGC_COMPACT_MAX_ZONES - 1u) return 0;
                dest = p + step;
            } else {
                if (cp_needs_pad(dest, p, o->RawGetMethodTable()))
                    dest += 12u;
                dest += step;
            }
        }
        p += step;
    }
    g_cp_zones = zidx + 1u;
    return 1;
}

/* Pass B: assign destinations, record per-chunk anchors (first live
 * START in each CP_CHUNK, with its true dest and zone index; offsets
 * +1, 0=empty). */
static void cp_plan(uint8_t* end)
{
    memset(g_anch_old, 0, sizeof(g_anch_old));
    memset(g_anch_new, 0, sizeof(g_anch_new));
    uint8_t* p    = g_lo + 4;
    uint8_t* dest = g_lo + 4;
    uint32_t zidx = 0;
    while (p < end) {
        Object*  o    = (Object*)p;
        uint32_t step = obj_step(o);
        if (is_marked(o)) {
            int      imm = cp_immobile(o);
            uint8_t* nd;
            if (imm || g_zone_ident[zidx]) {
                nd   = p;                    /* immobile / identity zone */
                dest = p + step;
            } else {
                if (cp_needs_pad(dest, p, o->RawGetMethodTable()))
                    dest += 12u;
                nd    = dest;
                dest += step;
            }
            uint32_t c = (uint32_t)(p - g_lo) / CP_CHUNK;
            if (!g_anch_old[c]) {
                g_anch_old[c]  = (uint32_t)(p - g_lo) + 1u;
                g_anch_new[c]  = (uint32_t)(nd - g_lo) + 1u;
                g_anch_zidx[c] = (uint16_t)zidx;
            }
            if (imm) zidx++;
        }
        p += step;
    }
}

#endif /* !MGC_MULTIRANGE (sliding-only passes) */

/* forwarding: replay the plan rules from the chunk anchor. Only valid
 * between plan and move (bitmap + old layout intact). Under the
 * compose this dispatches to the evacuation replay -- every fixup
 * caller (slots, roots, handles, finq) stays shared. */
static uint8_t* cp_forward(Object* obj)
{
#ifdef MGC_MULTIRANGE
    return evac_forward(obj);
#else
    uint8_t* p   = (uint8_t*)obj;
    uint8_t* end = g_lo + mgc_heap_used_bytes();
    if (p < g_lo + 4 || p >= end) return p;      /* frozen/foreign */
    uint32_t c = (uint32_t)(p - g_lo) / CP_CHUNK;
    if (!g_anch_old[c])
        coll_failfast("compact forward: empty anchor chunk for a live target");
    uint8_t* cur    = g_lo + (g_anch_old[c] - 1u);
    uint8_t* curnew = g_lo + (g_anch_new[c] - 1u);
    uint32_t zidx   = g_anch_zidx[c];
    for (;;) {
        Object*  co   = (Object*)cur;
        uint32_t step = obj_step(co);
        if (is_marked(co)) {
            int imm = cp_immobile(co);
            if (imm || g_zone_ident[zidx]) {
                if (cur == p) return cur;        /* immobile */
                curnew = cur + step;
                if (imm) zidx++;
            } else {
                if (cp_needs_pad(curnew, cur, co->RawGetMethodTable()))
                    curnew += 12u;
                if (cur == p) return curnew;
                curnew += step;
            }
        }
        cur += step;
        if (cur >= end)
            coll_failfast("compact forward: target not reached (layout/bitmap skew)");
    }
#endif /* MGC_MULTIRANGE */
}

/* ---- fixup: rewrite every reference BEFORE anything moves ---- */

static void fix_slot(Object** slot)
{
    Object* t = *slot;
    if (!t || !in_heap(t)) return;
    uint8_t* n = cp_forward(t);
#ifdef MGC_BREAK_COMPACT_FWD
    /* Built with this define, the 7th rewritten heap slot gets a
     * misaligned forward (+2), and the stale-scan's alignment test fails
     * at this same collection. */
    if (n != (uint8_t*)t && g_cp_fix_heap == 6u) {
        CLLOG("BREAK_COMPACT_FWD: slot %p forwarded to %p+2 (negative control)",
              (void*)slot, (void*)n);
        n += 2;
    }
#endif
    if (n != (uint8_t*)t) { *slot = (Object*)n; g_cp_fix_heap++; }
}

/* scan_refs clone with slot rewrite (a separate copy, so the hot mark
 * path is the same code when the compaction define is off) */
static void fixup_refs(Object* o)
{
    MethodTable* mt = o->RawGetMethodTable();
    if (!mt->ContainsGCPointers()) return;
    uint32_t  size = obj_exact_size(o);
    CGCDesc*  map  = CGCDesc::GetCGCDescFromMT(mt);
    ptrdiff_t cnt  = (ptrdiff_t)map->GetNumSeries();
    if (cnt >= 0) {
        CGCDescSeries* cur  = map->GetHighestSeries();
        CGCDescSeries* last = map->GetLowestSeries();
        do {
            uint8_t* parm = (uint8_t*)o + cur->GetSeriesOffset();
            uint8_t* stop = parm + cur->GetSeriesSize() + size;
            while (parm < stop) {
                fix_slot((Object**)parm);
                parm += sizeof(void*);
            }
            cur--;
        } while (cur >= last);
    } else {
        CGCDescSeries* cur   = map->GetHighestSeries();
        uint32_t       ncomp = ((ArrayBase*)o)->GetNumComponents();
        uint8_t*       parm  = (uint8_t*)o + cur->GetSeriesOffset();
        for (uint32_t c = 0; c < ncomp; c++) {
            for (ptrdiff_t i = 0; i > cnt; i--) {
                val_serie_item* it = cur->val_serie + i;
                for (uint32_t n = 0; n < it->nptrs; n++) {
                    fix_slot((Object**)parm);
                    parm += sizeof(void*);
                }
                parm += it->skip;
            }
        }
    }
}

/* EE root relocation callback (sc.promotion=false -- the EE's
 * EnumGcRefsCallback then SKIPS pinned slots and reports no
 * conservative ranges) */
static void mgc_relocate(Object** ppObject, ScanContext* sc, uint32_t flags)
{
    (void)sc;
    if (!ppObject || !*ppObject) return;
    uint8_t* v = (uint8_t*)*ppObject;
    if (flags & GC_CALL_INTERIOR) {
        Object* c = interior_resolve(v);
        if (!c) return;                          /* stack/foreign byref */
        uint8_t* n = cp_forward(c);
        if (n != (uint8_t*)c)
            *ppObject = (Object*)(v + (n - (uint8_t*)c));
        g_cp_fix_interior++;
        return;
    }
#ifdef MGC_MULTIRANGE
    if (in_heap(v)) {
        uint8_t* n = cp_forward((Object*)v);
        if (n != v) *ppObject = (Object*)n;
        g_cp_fix_root++;
    }
#else
    if (v >= g_lo + 4 && v < g_lo + mgc_heap_used_bytes()) {
        uint8_t* n = cp_forward((Object*)v);
        if (n != v) *ppObject = (Object*)n;
        g_cp_fix_root++;
    }
#endif
}

/* handle fixup: EVERY type's primary + dependent secondary (weak
 * slots too -- survivors' weak targets moved; dead ones were already
 * nulled by the death pipeline) */
static void handle_fix_visit(unsigned idx, Object** pslot,
                             Object** psecondary, int type, void* arg)
{
    (void)idx; (void)type; (void)arg;
    if (pslot && *pslot && in_heap(*pslot)) {
        uint8_t* n = cp_forward(*pslot);
        if (n != (uint8_t*)*pslot) { *pslot = (Object*)n; g_cp_fix_handle++; }
    }
#ifdef MGC_BREAK_COMPACT_DEPSEC
    /* Built with this define, the dependent handles' secondary rewrites
     * are skipped, and the stale-scan's handle pass fails on a moved
     * secondary. */
    if (type == 6) {
        static uint32_t warned;
        if (warned++ == 0)
            CLLOG("BREAK_COMPACT_DEPSEC: dependent SECONDARY rewrite SKIPPED (negative control)");
        return;
    }
#endif
    if (psecondary && *psecondary && in_heap(*psecondary)) {
        uint8_t* n = cp_forward(*psecondary);
        if (n != (uint8_t*)*psecondary) { *psecondary = (Object*)n; g_cp_fix_handle++; }
    }
}

#ifdef MGC_COMPACT_VERIFY
/* Content checksum (the direct silent-corruption detector): per
 * object, a commutative position-weighted sum over the copy span
 * [o-4, o+step-4) with GCDesc ref-slot words SUBTRACTED out (their
 * values legitimately change under relocation). Aggregated as a sum
 * over all live objects -- address-independent, so pre==post iff the
 * moved multiset of non-pointer bytes is intact. */
static uint32_t cp_h32(uint32_t i, uint8_t b)
{
    return (b + 0x9Eu) * (i * 2654435761u + 0x100001b3u);
}

static uint32_t cp_obj_hash(Object* o)
{
    uint32_t step = obj_step(o);
    uint8_t* base = (uint8_t*)o - 4;
    uint32_t h    = 0;
    /* the -4 header word is EXCLUDED: it is legitimately mutable
     * state (identity hash / thin locks / FINALIZER_RUN -- mutable by
     * design) and, when the object follows an exactly-12-byte free
     * run, the free list's next-pointer write at run+8 IS this slot.
     * Payload integrity starts at the MT word. */
    for (uint32_t i = 4; i < step; i++)
        h += cp_h32(i, base[i]);
    MethodTable* mt = o->RawGetMethodTable();
    if (mt->ContainsGCPointers()) {
        uint32_t  size = obj_exact_size(o);
        CGCDesc*  map  = CGCDesc::GetCGCDescFromMT(mt);
        ptrdiff_t cnt  = (ptrdiff_t)map->GetNumSeries();
        if (cnt >= 0) {
            CGCDescSeries* cur  = map->GetHighestSeries();
            CGCDescSeries* last = map->GetLowestSeries();
            do {
                uint8_t* parm = (uint8_t*)o + cur->GetSeriesOffset();
                uint8_t* stop = parm + cur->GetSeriesSize() + size;
                while (parm < stop) {
                    uint32_t off = (uint32_t)(parm - base);
                    for (uint32_t k = 0; k < 4u; k++)
                        h -= cp_h32(off + k, parm[k]);
                    parm += sizeof(void*);
                }
                cur--;
            } while (cur >= last);
        } else {
            CGCDescSeries* cur   = map->GetHighestSeries();
            uint32_t       ncomp = ((ArrayBase*)o)->GetNumComponents();
            uint8_t*       parm  = (uint8_t*)o + cur->GetSeriesOffset();
            for (uint32_t c = 0; c < ncomp; c++) {
                for (ptrdiff_t i2 = 0; i2 > cnt; i2--) {
                    val_serie_item* it = cur->val_serie + i2;
                    for (uint32_t n = 0; n < it->nptrs; n++) {
                        uint32_t off = (uint32_t)(parm - base);
                        for (uint32_t k = 0; k < 4u; k++)
                            h -= cp_h32(off + k, parm[k]);
                        parm += sizeof(void*);
                    }
                    parm += it->skip;
                }
            }
        }
    }
    return h;
}

/* live-heap aggregate; usable pre-move (old layout, via bitmap) and
 * post-move (new layout, every walked object below the new bump) */
static void cp_hash_heap(uint8_t* end, int marked_only,
                         uint32_t* cnt, uint32_t* sum)
{
#ifdef MGC_MULTIRANGE
    (void)end;                       /* per-range spans, not one end */
    *cnt = 0; *sum = 0;
    for (uint32_t r = 0; r < mgc_heap_ranges(); r++) {
        uint8_t *b, *e, *bp;
        mgc_heap_range_get(r, &b, &e, &bp);
        uint8_t* p = b + 4;
        while (p < bp) {
            Object*  o    = (Object*)p;
            uint32_t step = obj_step(o);
            int live = marked_only ? is_marked(o)
                     : (o->RawGetMethodTable() != mgc_heap_freemt());
            if (live) { *cnt += 1; *sum += cp_obj_hash(o); }
            p += step;
        }
    }
#else
    uint8_t* p = g_lo + 4;
    *cnt = 0; *sum = 0;
    while (p < end) {
        Object*  o    = (Object*)p;
        uint32_t step = obj_step(o);
        int live = marked_only ? is_marked(o)
                 : (o->RawGetMethodTable() != mgc_heap_freemt());
        if (live) { *cnt += 1; *sum += cp_obj_hash(o); }
        p += step;
    }
#endif
}
#endif /* MGC_COMPACT_VERIFY */

#ifdef MGC_COMPACT_VERIFY
/* Post-move stale-scan: every ref slot in the NEW heap must target
 * null, a non-heap address, or a live object (MT word non-null and
 * 4-aligned inside the new span). A pointer into vacated/virgin
 * space reads MT=0 -- the silent-corruption class made LOUD at the
 * compaction that creates it, with the HOLDER identified. */
static uint32_t g_stale_found;

static void stale_check_slot(Object* holder, Object** slot)
{
    Object* t = *slot;
    if (!t) return;
    uint8_t* p = (uint8_t*)t;
#ifdef MGC_MULTIRANGE
    if (!in_heap(p)) return;
#else
    if (p < g_lo + 4 || p >= g_lo + mgc_heap_used_bytes()) return;
#endif
    if (((uintptr_t)p & 3u) == 0 && t->RawGetMethodTable() != nullptr)
        return;
    if (g_stale_found < 8u)
        CLLOG("STALE SLOT: holder=%p (MT=%p) slot=%p -> %p (target MT reads %p)",
              (void*)holder,
              holder ? (void*)holder->RawGetMethodTable() : (void*)0,
              (void*)slot, (void*)t,
              ((uintptr_t)p & 3u) ? (void*)~0u : (void*)t->RawGetMethodTable());
    g_stale_found++;
}

static void stale_scan_refs(Object* o)
{
    MethodTable* mt = o->RawGetMethodTable();
    if (!mt->ContainsGCPointers()) return;
    uint32_t  size = obj_exact_size(o);
    CGCDesc*  map  = CGCDesc::GetCGCDescFromMT(mt);
    ptrdiff_t cnt  = (ptrdiff_t)map->GetNumSeries();
    if (cnt >= 0) {
        CGCDescSeries* cur  = map->GetHighestSeries();
        CGCDescSeries* last = map->GetLowestSeries();
        do {
            uint8_t* parm = (uint8_t*)o + cur->GetSeriesOffset();
            uint8_t* stop = parm + cur->GetSeriesSize() + size;
            while (parm < stop) {
                stale_check_slot(o, (Object**)parm);
                parm += sizeof(void*);
            }
            cur--;
        } while (cur >= last);
    } else {
        CGCDescSeries* cur   = map->GetHighestSeries();
        uint32_t       ncomp = ((ArrayBase*)o)->GetNumComponents();
        uint8_t*       parm  = (uint8_t*)o + cur->GetSeriesOffset();
        for (uint32_t c = 0; c < ncomp; c++) {
            for (ptrdiff_t i2 = 0; i2 > cnt; i2--) {
                val_serie_item* it = cur->val_serie + i2;
                for (uint32_t n = 0; n < it->nptrs; n++) {
                    stale_check_slot(o, (Object**)parm);
                    parm += sizeof(void*);
                }
                parm += it->skip;
            }
        }
    }
}

static void stale_handle_visit(unsigned idx, Object** pslot,
                               Object** psecondary, int type, void* arg)
{
    (void)idx; (void)type; (void)arg;
    if (pslot) stale_check_slot(nullptr, pslot);
    if (psecondary) stale_check_slot(nullptr, psecondary);
}

static void cp_stale_scan(unsigned long index)
{
    g_stale_found = 0;
    MethodTable* freemt = mgc_heap_freemt();
#ifdef MGC_MULTIRANGE
    for (uint32_t r = 0; r < mgc_heap_ranges(); r++) {
        uint8_t *b, *e, *bp;
        mgc_heap_range_get(r, &b, &e, &bp);
        uint8_t* p = b + 4;
        while (p < bp) {
            Object* o = (Object*)p;
            if (o->RawGetMethodTable() != freemt) stale_scan_refs(o);
            p += obj_step(o);
        }
    }
#else
    uint8_t* p   = g_lo + 4;
    uint8_t* end = g_lo + mgc_heap_used_bytes();
    while (p < end) {
        Object* o = (Object*)p;
        if (o->RawGetMethodTable() != freemt) stale_scan_refs(o);
        p += obj_step(o);
    }
#endif
    mgc_skel_handles_foreach(&stale_handle_visit, nullptr);
    if (g_stale_found) {
        CLLOG("compact #%lu STALE-SCAN: %lu stale slots (first 8 above)",
              index, (unsigned long)g_stale_found);
        coll_failfast("post-compact stale reference(s) -- a fixup pass missed a slot class");
    }
}
#endif /* MGC_COMPACT_VERIFY */

#ifndef MGC_MULTIRANGE
/* Pass D: slide (ascending -- dest<=src per object, disjoint ascending
 * dests: forward memmove is safe), install pads and hole runs, reset
 * the bump, zero the vacated tail (future allocations expect virgin
 * zeros, including the charge-tail header word). */
static void cp_move(uint8_t* end)
{
    MethodTable* freemt = mgc_heap_freemt();
    mgc_heap_free_reset();
    uint8_t* p    = g_lo + 4;
    uint8_t* dest = g_lo + 4;
    uint32_t zidx = 0;
    uint8_t* run  = nullptr;    /* identity-zone dead-run tracking */
    while (p < end) {
        Object*  o    = (Object*)p;
        uint32_t step = obj_step(o);
        if (is_marked(o)) {
            int imm = cp_immobile(o);
            if (imm || g_zone_ident[zidx]) {
                if (run) {      /* identity zone: original gap -> run */
                    mgc_heap_free_add(run, (uint32_t)(p - run));
                    g_dc.runs++; g_cp_holes++;
                    g_dc.swept_bytes += (uint32_t)(p - run);
                    run = nullptr;
                } else if (dest < p) {
                    /* slide-zone hole before a boundary (0 or >=12 by sim) */
                    mgc_heap_free_add(dest, (uint32_t)(p - dest));
                    g_dc.runs++; g_cp_holes++;
                    g_dc.swept_bytes += (uint32_t)(p - dest);
                }
                dest = p + step;
                if (imm) zidx++;
            } else {
                if (cp_needs_pad(dest, p, o->RawGetMethodTable())) {
                    /* 12-byte pad free object (walkable, off-list) */
                    *(MethodTable**)dest   = freemt;
                    *(uint32_t*)(dest + 4) = 0;
                    dest += 12u;
                    g_cp_pads++;
                }
                if (dest != p) {
                    memmove(dest - 4, p - 4, step);
                    g_cp_moved++;
                    g_cp_moved_bytes += step;
                }
                dest += step;
            }
            g_dc.live_objs++;
            g_dc.live_bytes += step;
        } else {
            g_dc.dead_objs++;
            if (poh_test(o)) poh_clear(o);   /* reclaimed POH: bit dies */
            if (g_zone_ident[zidx] && !run) run = p;
        }
        p += step;
    }
    if (run) {                  /* identity tail-zone trailing gap */
        mgc_heap_free_add(run, (uint32_t)(end - run));
        g_dc.runs++; g_cp_holes++;
        g_dc.swept_bytes += (uint32_t)(end - run);
    }
    /* new bump + zero the vacated tail, incl. the last charge-tail
     * header word at dest-4 (holds pre-move garbage otherwise) */
    if (dest < end)
        memset(dest - 4, 0, (size_t)(end - dest) + 4u);
    mgc_heap_bump_set(dest);
    mgc_heap_free_done(g_dc.live_objs + g_cp_pads);
}
#endif /* !MGC_MULTIRANGE (sliding move) */

#ifdef MGC_MULTIRANGE
/* The compose's compacting collection (dispatch target). Same
 * prologue/fate rules as the sliding path: pin-cap overflow falls
 * back to the plain sweep, loudly. */
static void evac_heap(unsigned long index)
{
    if (g_pins_over) {
        CLLOG("evac #%lu ABORTED: pin cap %u exceeded -- plain sweep (raise MGC_COMPACT_MAX_PINS)",
              index, (unsigned)MGC_COMPACT_MAX_PINS);
        death_sweep();
        return;
    }
    pins_sort_unique();
    if (!g_poh)  g_poh  = mgc_heap_poh_bitmap();
    if (!g_poh2) g_poh2 = mgc_heap_poh_bitmap2();
    g_cp_moved = g_cp_moved_bytes = g_cp_pads = g_cp_holes = 0;
    g_cp_fix_heap = g_cp_fix_root = g_cp_fix_handle = 0;
    g_cp_fix_finq = g_cp_fix_interior = 0;

    /* direction: the fitting evacuation that frees the most; dest
     * space = the dest range's free runs + its virgin tail (the last
     * range's OOM reserve honored). Plan can still fail on placement
     * fragmentation -- then the plain sweep, loudly. */
    uint32_t need[2], capc[2];
    uint8_t* rb[2]; uint8_t* re[2]; uint8_t* rbp[2]; uint8_t* rcap[2];
    for (uint32_t r = 0; r < 2u; r++) {
        mgc_heap_range_get(r, &rb[r], &re[r], &rbp[r]);
        rcap[r] = (r == 1u) ? re[r] - MGC_OOM_RESERVE : re[r];
        need[r] = evac_sizing(r);
        capc[r] = evac_capacity(rb[r], re[r], rbp[r], rcap[r]);
    }
    int fit0 = (capc[1] >= need[0] + 64u);
    int fit1 = (capc[0] >= need[1] + 64u);
    int src;
    if (fit0 && fit1) src = (need[0] >= need[1]) ? 0 : 1;
    else if (fit0)    src = 0;
    else if (fit1)    src = 1;
    else {
        CLLOG("evac #%lu: neither direction fits (need r0=%lu r1=%lu B, cap r0=%lu r1=%lu B) -- plain sweep",
              index, (unsigned long)need[0], (unsigned long)need[1],
              (unsigned long)capc[0], (unsigned long)capc[1]);
        death_sweep();
        return;
    }
    g_ev_src = (uint32_t)src;
    g_ev_dst = 1u - (uint32_t)src;
    g_ev_sbase = rb[g_ev_src];  g_ev_send  = rbp[g_ev_src];
    g_ev_dbase = rb[g_ev_dst];  g_ev_dend  = re[g_ev_dst];
    g_ev_dbump = rbp[g_ev_dst]; g_ev_dcap  = rcap[g_ev_dst];

    if (!evac_plan()) {
        CLLOG("evac #%lu: r%lu->r%lu plan ran out of dest placements -- plain sweep",
              index, (unsigned long)g_ev_src, (unsigned long)g_ev_dst);
        death_sweep();
        return;
    }

#ifdef MGC_COMPACT_VERIFY
    uint32_t pre_cnt, pre_sum;
    cp_hash_heap(nullptr, /*marked_only*/ 1, &pre_cnt, &pre_sum);
#endif

    /* ---- fixup, all BEFORE the move (old layout + bitmap valid);
     * BOTH ranges' live objects -- dest objects point into the source */
    for (uint32_t r = 0; r < 2u; r++) {
        uint8_t *b, *e, *bp;
        mgc_heap_range_get(r, &b, &e, &bp);
        uint8_t* pfx = b + 4;
        while (pfx < bp) {
            Object*  o    = (Object*)pfx;
            uint32_t step = obj_step(o);
            if (is_marked(o)) fixup_refs(o);
            pfx += step;
        }
    }
    mgc_skel_handles_foreach(&handle_fix_visit, nullptr);
    {
        uint32_t bound = mgc_finq_bound();
        for (uint32_t i = 0; i < bound; i++) {
            int st; Object* o = mgc_finq_at(i, &st);
            if (st != MGC_FQ_FREE && o && in_heap(o)) {
                uint8_t* n = cp_forward(o);
                if (n != (uint8_t*)o) { mgc_finq_fix(i, (Object*)n); g_cp_fix_finq++; }
            }
        }
    }
#ifdef MGC_BREAK_COMPACT_ROOTS
    if (index == 1)
        CLLOG("BREAK_COMPACT_ROOTS: EE root relocation SKIPPED (negative control)");
#else
    {
        ScanContext sc2;
        sc2.thread_under_crawl = nullptr;
        sc2.thread_number      = 0;
        sc2.thread_count       = 1;
        sc2.stack_limit        = 0;
        sc2.promotion          = false;  /* THE relocate semantics */
        sc2.concurrent         = false;
        GCToEEInterface::GcScanRoots(&mgc_relocate, 2, 2, &sc2);
    }
#endif

    evac_move();
    /* the normal per-range sweep rebuilds the free list, the walk
     * model and (the compose's rule) retracts trailing runs -- driven by the
     * TRANSPLANTED bits: moved objects are marked at their new homes,
     * everything vacated or consumed is not. */
    death_sweep();

    {
        uint8_t *sb2, *se2, *sbp2, *db2, *de2, *dbp2;
        mgc_heap_range_get(g_ev_src, &sb2, &se2, &sbp2);
        mgc_heap_range_get(g_ev_dst, &db2, &de2, &dbp2);
        CLLOG("evac #%lu: r%lu->r%lu moved=%lu objs %lu KB | pins=%lu | fix: heap=%lu roots=%lu(+%lu int) handles=%lu finq=%lu | src bump -> %lu KB, dst bump %lu -> %lu KB",
              index, (unsigned long)g_ev_src, (unsigned long)g_ev_dst,
              (unsigned long)g_cp_moved,
              (unsigned long)(g_cp_moved_bytes / 1024u),
              (unsigned long)g_pins_n,
              (unsigned long)g_cp_fix_heap,
              (unsigned long)g_cp_fix_root, (unsigned long)g_cp_fix_interior,
              (unsigned long)g_cp_fix_handle, (unsigned long)g_cp_fix_finq,
              (unsigned long)((uint32_t)(sbp2 - sb2) / 1024u),
              (unsigned long)((uint32_t)(g_ev_dbump - db2) / 1024u),
              (unsigned long)((uint32_t)(dbp2 - db2) / 1024u));
    }

    /* ---- self-audit: the walk must still tile perfectly ---- */
    {
        uint32_t post = heap_walk_verify();
        uint32_t model = mgc_heap_walk_expected();
        if (post != model) {
            CLLOG("evac #%lu AUDIT: walk=%lu model=%lu MISMATCH",
                  index, (unsigned long)post, (unsigned long)model);
            coll_failfast("post-evac walk != live+pads+runs");
        }
    }
#ifdef MGC_COMPACT_VERIFY
    {
        uint32_t post_cnt, post_sum;
        cp_hash_heap(nullptr, /*marked_only*/ 0, &post_cnt, &post_sum);
        if (post_cnt != pre_cnt || post_sum != pre_sum) {
            CLLOG("evac #%lu VERIFY: pre cnt=%lu sum=%08lx post cnt=%lu sum=%08lx MISMATCH",
                  index, (unsigned long)pre_cnt, (unsigned long)pre_sum,
                  (unsigned long)post_cnt, (unsigned long)post_sum);
            coll_failfast("evac content checksum mismatch (silent corruption)");
        }
        CLLOG("evac #%lu VERIFY: %lu objects, content sum %08lx pre==post",
              index, (unsigned long)post_cnt, (unsigned long)post_sum);
    }
    cp_stale_scan(index);
#endif
}
#endif /* MGC_MULTIRANGE (evac) */

static void compact_heap(unsigned long index)
{
#ifdef MGC_MULTIRANGE
    evac_heap(index);
}
#else
    /* NOTE: g_dc already carries the death-pipeline counters of THIS
     * collection (promoted/finq/weak nulls) -- never reset it here:
     * EnableFinalization gates on g_dc.promoted after the call returns. Only
     * the sweep-side fields (live/dead/runs/swept) are ours to fill,
     * and they are still at the pipeline-entry zeros. */
    if (g_pins_over) {
        CLLOG("compact #%lu ABORTED: pin cap %u exceeded -- plain sweep (raise MGC_COMPACT_MAX_PINS)",
              index, (unsigned)MGC_COMPACT_MAX_PINS);
        death_sweep();
        return;
    }
    pins_sort_unique();
    if (!g_poh) g_poh = mgc_heap_poh_bitmap();
    g_cp_moved = g_cp_moved_bytes = g_cp_pads = g_cp_holes = 0;
    g_cp_fix_heap = g_cp_fix_root = g_cp_fix_handle = 0;
    g_cp_fix_finq = g_cp_fix_interior = 0;

    uint8_t* end = g_lo + mgc_heap_used_bytes();

    if (!cp_sim_zones(end)) {
        CLLOG("compact #%lu ABORTED: zone cap %u exceeded -- plain sweep (raise MGC_COMPACT_MAX_ZONES)",
              index, (unsigned)MGC_COMPACT_MAX_ZONES);
        death_sweep();
        return;
    }
    g_cp_zones_ident = 0;
    for (uint32_t z = 0; z < MGC_COMPACT_MAX_ZONES; z++)
        if (g_zone_ident[z]) g_cp_zones_ident++;

    cp_plan(end);

#ifdef MGC_COMPACT_VERIFY
    uint32_t pre_cnt, pre_sum;
    cp_hash_heap(end, /*marked_only*/ 1, &pre_cnt, &pre_sum);
#endif

    /* ---- fixup, all BEFORE the slide (old layout + bitmap valid) ---- */
    uint8_t* pfx = g_lo + 4;                    /* heap-interior slots */
    while (pfx < end) {
        Object*  o    = (Object*)pfx;
        uint32_t step = obj_step(o);
        if (is_marked(o)) fixup_refs(o);
        pfx += step;
    }
    mgc_skel_handles_foreach(&handle_fix_visit, nullptr);
    {                                           /* finq slots (all states) */
        uint32_t bound = mgc_finq_bound();
        for (uint32_t i = 0; i < bound; i++) {
            int st; Object* o = mgc_finq_at(i, &st);
            if (st != MGC_FQ_FREE && o && in_heap(o)) {
                uint8_t* n = cp_forward(o);
                if (n != (uint8_t*)o) { mgc_finq_fix(i, (Object*)n); g_cp_fix_finq++; }
            }
        }
    }
#ifdef MGC_BREAK_COMPACT_ROOTS
    /* Built with this define, the EE root re-scan is skipped: every moved
     * object referenced only from a stack or register slot keeps its
     * stale address, which the program's own checks (and usually the
     * stale references' later reads) catch; the outcome is printed. */
    if (index == 1)
        CLLOG("BREAK_COMPACT_ROOTS: EE root relocation SKIPPED (negative control)");
#else
    {                                           /* EE roots, relocate flavor */
        ScanContext sc2;
        sc2.thread_under_crawl = nullptr;
        sc2.thread_number      = 0;
        sc2.thread_count       = 1;
        sc2.stack_limit        = 0;
        sc2.promotion          = false;  /* THE relocate semantics */
        sc2.concurrent         = false;
        GCToEEInterface::GcScanRoots(&mgc_relocate, 2, 2, &sc2);
    }
#endif

    cp_move(end);

    CLLOG("compact #%lu: moved=%lu objs %lu KB | pins=%lu zones=%lu(ident=%lu) pads=%lu holes=%lu | fix: heap=%lu roots=%lu(+%lu int) handles=%lu finq=%lu | bump %lu -> %lu KB",
          index,
          (unsigned long)g_cp_moved,
          (unsigned long)(g_cp_moved_bytes / 1024u),
          (unsigned long)g_pins_n,
          (unsigned long)g_cp_zones, (unsigned long)g_cp_zones_ident,
          (unsigned long)g_cp_pads, (unsigned long)g_cp_holes,
          (unsigned long)g_cp_fix_heap,
          (unsigned long)g_cp_fix_root, (unsigned long)g_cp_fix_interior,
          (unsigned long)g_cp_fix_handle, (unsigned long)g_cp_fix_finq,
          (unsigned long)((uint32_t)(end - g_lo) / 1024u),
          (unsigned long)(mgc_heap_used_bytes() / 1024u));

    /* ---- self-audit: the walk must still tile perfectly ---- */
    {
        uint32_t post = heap_walk_verify();
        uint32_t model = mgc_heap_walk_expected();
        if (post != model) {
            CLLOG("compact #%lu AUDIT: walk=%lu model=%lu MISMATCH",
                  index, (unsigned long)post, (unsigned long)model);
            coll_failfast("post-compact walk != live+pads+runs");
        }
    }
#ifdef MGC_COMPACT_VERIFY
    {
        uint32_t post_cnt, post_sum;
        cp_hash_heap(g_lo + mgc_heap_used_bytes(), /*marked_only*/ 0,
                     &post_cnt, &post_sum);
        /* post walk counts live objects as non-free-MT entities:
         * pads and hole runs carry the free MT, live objects never do */
        if (post_cnt != pre_cnt || post_sum != pre_sum) {
            CLLOG("compact #%lu VERIFY: pre cnt=%lu sum=%08lx post cnt=%lu sum=%08lx MISMATCH",
                  index, (unsigned long)pre_cnt, (unsigned long)pre_sum,
                  (unsigned long)post_cnt, (unsigned long)post_sum);
            coll_failfast("compact content checksum mismatch (silent corruption)");
        }
        CLLOG("compact #%lu VERIFY: %lu objects, content sum %08lx pre==post",
              index, (unsigned long)post_cnt, (unsigned long)post_sum);
    }
    cp_stale_scan(index);
#endif
}
#endif /* MGC_MULTIRANGE dispatch */
#endif /* MGC_COMPACT */
#endif /* MGC_SWEEP_LIVE */

/* ------------------------------------------------------------------ */
/* init                                                               */
/* ------------------------------------------------------------------ */

extern "C" void mgc_collect_init(void)
{
    if (g_bm)
        coll_failfast("mgc_collect_init called twice");

#ifdef MGC_MULTIRANGE
    /* g_lo/g_hi = RANGE 0 exactly (never the union: in_heap and the
     * mark index derive from them); range 1 mirrors beside them. */
    {
        uint8_t *b, *e, *p2;
        mgc_heap_range_get(0, &b, &e, &p2);
        g_lo = b; g_hi = e;
        mgc_heap_range_get(1, &b, &e, &p2);
        g_r1base = b; g_r1end = e;
    }
#else
    mgc_heap_range(&g_lo, &g_hi);
#endif
    if (!g_lo)
        coll_failfast("mgc_collect_init before mgc_heap_init");

    g_bm_bytes = (uint32_t)(g_hi - g_lo) / 32u;
    void* bm = rhu_shim_collector_mmap(g_bm_bytes);
    if (bm == (void*)-1 || bm == nullptr)
        coll_failfast("mark bitmap mmap failed");
    g_bm = (uint8_t*)bm;

#ifdef MGC_MULTIRANGE
    g_r1bm_bytes = (uint32_t)(g_r1end - g_r1base) / 32u;
    void* bm1 = mmap64(nullptr, g_r1bm_bytes, GLIBC_PROT_RW,
                       GLIBC_MAP_ANON_PRIV, -1, 0);
    if (bm1 == (void*)-1 || bm1 == nullptr)
        coll_failfast("range 1 mark bitmap mmap failed");
    g_r1bm = (uint8_t*)bm1;
    CLLOG("init: range 1 [%p,%p) bitmap %lu KB @%p (PER-RANGE bitmaps -- no bits for the gap)",
          (void*)g_r1base, (void*)g_r1end,
          (unsigned long)(g_r1bm_bytes / 1024u), (void*)g_r1bm);
#endif

    g_stk_cap = MGC_MARK_STACK_ENTRIES;
    void* stk = rhu_shim_collector_mmap(g_stk_cap * sizeof(Object*));
    if (stk == (void*)-1 || stk == nullptr)
        coll_failfast("mark stack mmap failed");
    g_stk = (Object**)stk;

#ifdef MGC_BREAK_TAIL_REJOIN
    CLLOG("BREAK_TAIL_REJOIN: a dead run at the top joins the free list and the bump mark stays (the negative control)");
#endif
    CLLOG("init: mark bitmap %lu KB @%p (region/32), mark stack %lu entries (%lu KB) @%p -- both shim mmap, zero static RAM",
          (unsigned long)(g_bm_bytes / 1024u), (void*)g_bm,
          (unsigned long)g_stk_cap,
          (unsigned long)(g_stk_cap * sizeof(Object*) / 1024u), (void*)g_stk);
}

#ifdef MGC_TUNE_LIVE
/* What the collector tables cost in system memory for a managed heap of
 * the given size, each draw rounded up to the page the bridge draws in:
 * the mark bitmap (the heap / 32) and the mark stack -- the same two
 * expressions as the init's draws above. The heap's rule at boot sizes
 * itself with this, so the two sizes are stated in this file alone. */
extern "C" uint32_t mgc_collect_tables_bytes(uint32_t heap_bytes,
                                             uint32_t page_bytes)
{
    uint32_t mask = page_bytes - 1u;
    uint32_t bm   = (heap_bytes / 32u + mask) & ~mask;
    uint32_t stk  = ((uint32_t)(MGC_MARK_STACK_ENTRIES * sizeof(Object*)) + mask) & ~mask;
    return bm + stk;
}
#endif

/* counters for the skeleton's BCL surface */
extern "C" uint32_t mgc_collect_count(int gen)
{
    if (gen < 0 || gen > 2) gen = 2;
    return g_count[gen];
}

extern "C" uint32_t mgc_collect_total(void)
{
    return g_total;
}

/* ------------------------------------------------------------------ */
/* the collection                                                     */
/* ------------------------------------------------------------------ */

extern "C" int mgc_heap_collect(int generation, int low_memory_p, int mode)
{
    /* --- concurrent-entry guard: loser waits in PREEMPTIVE mode so
     * the winner's SuspendEE never deadlocks on it (the runtime's
     * entry-dance parity) --- */
    while (__atomic_test_and_set((void*)&g_gc_busy, __ATOMIC_ACQUIRE)) {
        GCToEEInterface::EnablePreemptiveGC();
        while (__atomic_load_n(&g_gc_busy, __ATOMIC_RELAXED))
            sched_yield();
        GCToEEInterface::DisablePreemptiveGC();
    }

    if (!g_bm)
        coll_failfast("collection before mgc_collect_init");

    int condemned = (generation < 0 || generation > 2) ? 2 : generation;
    g_total++;
    uint32_t index = g_total;
    int full_log = (index <= 4) || ((index & 15u) == 0);
#ifdef MGC_COMPACT
    /* decide BEFORE the mark so pin capture runs on compacting marks */
#ifdef MGC_COMPACT_ALWAYS
    g_compact_this = 1;
#else
    g_compact_this = g_compact_req;
#endif
    g_compact_req = 0;
    g_pins_n = 0; g_pins_over = 0;
#endif

    if (full_log)
        CLLOG("collect #%lu ENTER gen=%d (condemned=%d) low_mem=%d mode=%d",
              (unsigned long)index, generation, condemned, low_memory_p, mode);
    if (index == 1)
        mgc_heap_census("GarbageCollect #1");

    /* ---------------- the world stops ---------------- */
    GCToEEInterface::SuspendEE(SUSPEND_FOR_GC);
    if (full_log)
        CLLOG("world SUSPENDED (SuspendEE returned; heap is exclusively ours)");
#ifdef MGC_MR_SPLIT_STOMP
    /* Split stomp: swing the EE bounds to the UNION for the conservative
     * stack scan; restored to PRIMARY before RestartEE below */
    mgc_heap_stomp_gc(1);
    if (index == 1)
        CLLOG("split stomp: bounds -> UNION for the collection (restored to PRIMARY at restart)");
#endif

    /* counts update BEFORE the work (the runtime's order); a gen-N collection
     * collects every generation <= N */
    for (int g = 0; g <= condemned; g++) g_count[g]++;

    memset(&g_mc, 0, sizeof(g_mc));
    g_roots_total = 0;
    g_sp = 0; g_sp_hw = 0;
    g_ovf_flag = 0; g_ovf_lo = g_ovf_hi = nullptr;
    g_ovf_deferred = g_ovf_rounds = g_ovf_rescanned = 0;
#ifdef MGC_BREAK_MARK_OVERFLOW
    if (index == 1)
        CLLOG("BREAK_MARK_OVERFLOW: the overflow recovery is compiled out -- a full mark stack fail-fasts (negative control)");
#endif

    GCToEEInterface::GcEnumAllocContexts(&mgc_fix_alloc_context, nullptr);
    GCToEEInterface::DiagGCStart(condemned, /*isInduced*/ true);
    GCToEEInterface::GcStartWork(condemned, /*max_gen*/ 2);
    GCToEEInterface::BeforeGcScanRoots(condemned, /*is_bgc*/ false,
                                       /*is_concurrent*/ false);

    memset(g_bm, 0, g_bm_bytes);
#ifdef MGC_MULTIRANGE
    memset(g_r1bm, 0, g_r1bm_bytes);
#endif

    /* structural precondition: size formula == real layout */
    uint32_t walk_objs, allocs, pads;
    walk_objs = heap_walk_verify();
    mgc_heap_counts(&allocs, &pads);
#ifdef MGC_SWEEP_LIVE
    /* post-sweep heaps are not monotonic: the model is live objects +
     * pads + free runs, maintained by the allocator and rebuilt by
     * every sweep. Captured HERE -- the sweep below rebuilds it, so
     * the census print must use the walk-time value. */
    uint32_t walk_model = mgc_heap_walk_expected();
    if (walk_objs != walk_model) {
        CLLOG("heap walk: objects=%lu but walk_expected=%lu MISMATCH",
              (unsigned long)walk_objs, (unsigned long)walk_model);
        coll_failfast("heap walk count != allocator model");
    }
#else
    if (walk_objs != allocs + pads) {
        CLLOG("heap walk: objects=%lu but allocs+pads=%lu MISMATCH",
              (unsigned long)walk_objs, (unsigned long)(allocs + pads));
        coll_failfast("heap walk count != allocator census");
    }
#endif

    ScanContext sc;
    sc.thread_under_crawl = nullptr;
    sc.thread_number      = 0;
    sc.thread_count       = 1;
    sc.stack_limit        = 0;
    sc.promotion          = true;   /* mark pass -- NEVER a relocation */
    sc.concurrent         = false;

#ifdef MGC_BREAK_ROOTS
    /* Built with this define, no root is ever reported; the census below
     * then prints roots=0 marked=0, and the collector gate's roots>0 check
     * fails on this build. What follows is printed, not asserted (a
     * mark-only collection kills nothing either way). */
    CLLOG("BREAK_ROOTS: ALL root scanning SUPPRESSED (negative control)");
    (void)sc;
    (void)&mgc_promote; (void)&handle_visit; (void)&dep_visit;
    (void)&scan_frozen_roots; (void)&mgc_heap_finq_snapshot;
#else
    /* root order per the runtime's mark_phase: frozen
     * (mark_ro_segments position), then stacks+statics, then handles,
     * then the dependent-handle fixpoint; without MGC_SWEEP_LIVE no
     * finalizer ever runs, so finq objects are a root class too. */
    g_phase = PH_FROZEN;
    scan_frozen_roots();

    g_phase = PH_STACK;
    GCToEEInterface::GcScanRoots(&mgc_promote, condemned, /*max_gen*/ 2, &sc);
    mark_drain();

    g_phase = PH_HANDLE;
    mgc_skel_handles_foreach(&handle_visit, nullptr);
    mark_drain();

#ifndef MGC_SWEEP_LIVE
    /* Mark-only build: with no death pipeline the finq is a blanket root class
     * (nothing may die). Under MGC_SWEEP_LIVE registered finalizables are NOT roots --
     * they die by unreachability and the partition promotes them. */
    g_phase = PH_FINQ;
    {
        Object** finq;
        uint32_t n = mgc_heap_finq_snapshot(&finq);
        for (uint32_t i = 0; i < n; i++)
            mark_edge(finq[i]);
        mark_drain();
    }
#endif

    g_phase = PH_DEP;
    {
        DepFix fx;
        do {
            fx.changed = 0;
            mgc_skel_handles_foreach(&dep_visit, &fx);
            mark_drain();
        } while (fx.changed);
    }
#endif

#ifndef MGC_BREAK_MARK_OVERFLOW
    /* Every root phase drains; a window still set here is a structural
     * break (a deferred push whose references were never rescanned). */
    if (g_ovf_flag)
        coll_failfast("a deferred push was never rescanned (the mark phase ended with the window set)");
#endif
    GCToEEInterface::AfterGcScanRoots(condemned, /*max_gen*/ 2, &sc);

#ifdef MGC_SWEEP_LIVE
    /* ------------ the death pipeline (the runtime's order) ------------ */
    memset(&g_dc, 0, sizeof(g_dc));
#ifdef MGC_BREAK_WEAK
    /* Built with this define, weak nulling is suppressed (both phases and
     * the dependent clearing); partition, promote and sweep are unchanged,
     * and a weak reference to a dead object reads alive. */
    if (index == 1)
        CLLOG("BREAK_WEAK: weak nulling SUPPRESSED (negative control)");
#else
    {   /* GcShortWeakPtrScan position: ws referents that did not
         * survive the mark read null from here on */
        WeakArg wa; wa.long_phase = 0;
        mgc_skel_handles_foreach(&weak_visit, &wa);
    }
#endif
    death_scan_for_finalization();
    GCToEEInterface::DiagWalkFReachableObjects(nullptr); /* EE no-op -- parity */
    g_phase = PH_DEP;
    {   /* 2nd dependent fixpoint: freach-promoted primaries pull their
         * secondaries (GcDhReScan position) */
        DepFix fx;
        do {
            fx.changed = 0;
            mgc_skel_handles_foreach(&dep_visit, &fx);
            mark_drain();
        } while (fx.changed);
    }
#ifndef MGC_BREAK_WEAK
    {   /* GcWeakPtrScan position: wl nulling + dead-primary dependent
         * clearing */
        WeakArg wa; wa.long_phase = 1;
        mgc_skel_handles_foreach(&weak_visit, &wa);
    }
#endif
    GCToEEInterface::SyncBlockCacheWeakPtrScan(nullptr, 0, 0); /* EE no-op -- parity */
#ifdef MGC_COMPACT
    if (g_compact_this) compact_heap((unsigned long)index);
    else death_sweep();
#else
    death_sweep();
#endif
#ifdef MGC_MULTIRANGE
    /* the untabled gap must still be all-poison after a full cycle of
     * marking, weak processing and sweeping -- every collection */
    mgc_heap_gap_verify("post-collection");
#endif
    /* ---------------------------------------------------------------- */
#endif /* MGC_SWEEP_LIVE */

    GCToEEInterface::GcDone(condemned);
    GCToEEInterface::DiagGCEnd((size_t)index, condemned, /*reason*/ 0,
                               /*fConcurrent*/ false);

    uint32_t used = mgc_heap_used_bytes();
#ifdef MGC_SWEEP_LIVE
    /* alloc-triggered collections make about 20 to 40 per run: census lines
     * print for the first 6 + every 8th (#1-#4 stay full via full_log:
     * the verification gate reads those lines) */
    int census_log = (index <= 6) || ((index & 7u) == 0);
#else
    int census_log = 1;
#endif
    if (full_log) {
        CLLOG("roots: stacks=%lu handles=%lu (weak_skipped=%lu) dep_seen=%lu dep_marked=%lu finq=%lu frozen_objs=%lu frozen_edges=%lu",
              (unsigned long)g_mc.roots_stack,
              (unsigned long)g_mc.roots_handle,
              (unsigned long)g_mc.roots_handle_weak,
              (unsigned long)g_mc.dep_seen,
              (unsigned long)g_mc.roots_dep_marked,
              (unsigned long)g_mc.roots_finq,
              (unsigned long)g_mc.frozen_objs,
              (unsigned long)g_mc.frozen_edges);
        CLLOG("quals: interior=%lu (unresolved=%lu) pinned=%lu nulls=%lu out_of_heap=%lu re_marks=%lu ctx_seen=%lu ctx_nonzero=%lu",
              (unsigned long)g_mc.interior,
              (unsigned long)g_mc.interior_unres,
              (unsigned long)g_mc.pinned,
              (unsigned long)g_mc.nulls,
              (unsigned long)g_mc.out_of_heap,
              (unsigned long)g_mc.already,
              (unsigned long)g_mc.ctx_seen,
              (unsigned long)g_mc.ctx_nonzero);
#ifdef MGC_SWEEP_LIVE
        CLLOG("heap walk: objects=%lu (model=%lu: live+pads+runs) MATCH",
              (unsigned long)walk_objs, (unsigned long)walk_model);
#else
        CLLOG("heap walk: objects=%lu (allocs=%lu pads=%lu) MATCH",
              (unsigned long)walk_objs, (unsigned long)allocs,
              (unsigned long)pads);
#endif
    }
#ifdef MGC_MULTIRANGE
    if (census_log) {
        uint8_t *b0, *e0, *p0, *b1, *e1, *p1;
        mgc_heap_range_get(0, &b0, &e0, &p0);
        mgc_heap_range_get(1, &b1, &e1, &p1);
        CLLOG("ranges #%lu: r0=%lu objs (bump %lu KB) r1=%lu objs (bump %lu KB) gap=OK",
              (unsigned long)index,
              (unsigned long)g_walk_r[0],
              (unsigned long)((uint32_t)(p0 - b0) / 1024u),
              (unsigned long)g_walk_r[1],
              (unsigned long)((uint32_t)(p1 - b1) / 1024u));
    }
#endif
    if (census_log) {
        CLLOG("mark census #%lu: roots=%lu marked=%lu marked_bytes=%lu used_bytes=%lu live_ratio~%lu%% stack_hw=%lu of %lu",
              (unsigned long)index,
              (unsigned long)g_roots_total,
              (unsigned long)g_mc.marked,
              (unsigned long)g_mc.marked_bytes,
              (unsigned long)used,
              (unsigned long)(used ? (uint64_t)g_mc.marked_bytes * 100u / used : 0),
              (unsigned long)g_sp_hw, (unsigned long)g_stk_cap);
        if (g_ovf_rounds)
            CLLOG("mark stack full: %lu pushes deferred at cap %lu; %lu rescan round(s) over the window; %lu objects re-scanned",
                  (unsigned long)g_ovf_deferred, (unsigned long)g_stk_cap,
                  (unsigned long)g_ovf_rounds, (unsigned long)g_ovf_rescanned);
    }
    if (g_mc.marked_bytes > used)
        coll_failfast("marked_bytes exceeds used_bytes (tracer escaped the heap)");
#ifdef MGC_SWEEP_LIVE
    if (census_log) {
        CLLOG("death #%lu: ws_null=%lu wl_null=%lu dep_clear=%lu odd=%lu | finq kept=%lu eager=%lu suppressed=%lu promoted=%lu(+%lu crit) freach_pending=%lu",
              (unsigned long)index,
              (unsigned long)g_dc.ws_nulled, (unsigned long)g_dc.wl_nulled,
              (unsigned long)g_dc.dep_cleared,
              (unsigned long)g_dc.weak_odd_nulled,
              (unsigned long)g_dc.finq_kept, (unsigned long)g_dc.finq_eager,
              (unsigned long)g_dc.finq_suppressed,
              (unsigned long)g_dc.promoted, (unsigned long)g_dc.promoted_crit,
              (unsigned long)mgc_freach_count());
        CLLOG("sweep #%lu: live=%lu objs %lu KB | reclaimed %lu objs -> %lu runs %lu KB free",
              (unsigned long)index,
              (unsigned long)g_dc.live_objs,
              (unsigned long)(g_dc.live_bytes / 1024u),
              (unsigned long)g_dc.dead_objs,
              (unsigned long)g_dc.runs,
              (unsigned long)(mgc_heap_free_bytes() / 1024u));
#ifdef MGC_FRAG_CENSUS
        mgc_heap_frag_line((unsigned long)index);
#endif
    }
#endif
    if (census_log)
        CLLOG("counters: gen0=%lu gen1=%lu gen2=%lu total=%lu",
              (unsigned long)g_count[0], (unsigned long)g_count[1],
              (unsigned long)g_count[2], (unsigned long)g_total);

    /* ---------------- the world resumes ---------------- */
#ifdef MGC_MR_SPLIT_STOMP
    mgc_heap_stomp_gc(0);   /* PRIMARY bounds back before mutators run */
#endif
    GCToEEInterface::RestartEE(true);
    if (full_log)
        CLLOG("world RESTARTED (RestartEE returned) -- collect #%lu S_OK",
              (unsigned long)index);
#ifdef MGC_SWEEP_LIVE
    /* AFTER RestartEE, only when this collection promoted f-reachable
     * work (the woken finalizer drains GetNextFinalizable and reports
     * back through RhpSignalFinalizationComplete). The runtime's own
     * collector also sets the event inside ScanForFinalization --
     * idempotent, and the finalizer cannot run while suspended, so one
     * post-restart site is behavior-equal. */
    if (g_dc.promoted + g_dc.promoted_crit) {
        GCToEEInterface::EnableFinalization(true);
        if (census_log)
            CLLOG("EnableFinalization(true): %lu(+%lu crit) promoted this collection",
                  (unsigned long)g_dc.promoted,
                  (unsigned long)g_dc.promoted_crit);
    }
#else
    /* EnableFinalization: honestly NOT called -- a mark-only build
     * promotes no f-reachable work (ScanForFinalization exists only
     * under MGC_SWEEP_LIVE). */
#endif

    __atomic_clear((void*)&g_gc_busy, __ATOMIC_RELEASE);
    return 0; /* S_OK */
}
