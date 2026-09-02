/* mgc_skeleton.cpp -- the mgc interface skeleton.
 *
 * A COMPLETE IGCHeap + IGCHandleManager + IGCHandleStore vtable
 * implementation, built against the VENDORED pinned-tag headers
 * (src/vendor/gcinterface/, the pinned commit recorded in
 * src/VENDORED.md), with NO collector behind it:
 *
 *  - GC_VersionInfo prints BEFORE it reports (a gate requirement: a
 *    version mismatch on bare metal must die VISIBLY, and the print
 *    proves the seam was reached and shows what it reported).
 *  - It reports the pinned header's own constants
 *    {GC_INTERFACE_MAJOR_VERSION, GC_INTERFACE_MINOR_VERSION, 0} =
 *    {5,8,0} (BuildVersion has no header constant; the runtime's own
 *    collector reports 0 there).
 *  - GC_Initialize returns real vtable objects whose init-time methods
 *    SUCCEED and LOG (`[mgc-skel] call <Iface>::<Method>` -- the union
 *    of these lines over a boot IS the init-time call inventory the
 *    gate pins).
 *  - The FIRST OBJECT ALLOCATION fail-fasts LOUDLY with a distinct
 *    marker: a skeleton must never limp into wrong answers. Handles
 *    are the one surface implemented for real (static slot pool),
 *    because the EE READS handles by direct dereference -- an opaque
 *    fake would be a silent wrong answer, the thing the gate forbids.
 *  - GcDacVars: pointers zeroed, VAL fields carry the values the
 *    runtime's own collector writes through the seam:
 *    major=2 minor=8 generation_size=136 total_generation_count=5.
 *
 * The interface shim is C++: the vtable ABI comes from the compiler
 * instead of hand-laid function-pointer structs, `override` makes
 * every signature drift a compile error, and arm-none-eabi-g++ 12.2
 * compiles it clean under -fno-exceptions -fno-rtti
 * -fno-threadsafe-statics -fno-use-cxa-atexit (no libstdc++/libsupc++/
 * _Unwind imports -- the gate asserts this on the object file). The
 * collector CORE behind this shim may be C; the shim is the ABI
 * boundary.
 *
 * Freestanding C++ discipline (each rule exists because breaking it
 * drags runtime baggage into the payload link):
 *  - singletons are placement-new'd into static storage at
 *    GC_Initialize time (no .init_array reliance -- the payload link
 *    runs no C++ global constructors);
 *  - operator delete is never called, but the one virtual destructor
 *    in IGCHandleStore makes the compiler emit a deleting-destructor
 *    vtable slot that references it, so the link must supply one (the
 *    gate host supplies a fail-fast; the runtime archives supply their
 *    own -- see the note below the fail-fast helper);
 *  - no exceptions, no RTTI, no thread-safe-static guards, no atexit.
 *
 * Linked ONLY into skeleton-tier gate builds: it provides the strong
 * __mgc_dlopen_hook / __mgc_dlsym_hook pair the host shim's weak seam
 * expects.
 *
 * -DMGC_HEAP_LIVE (linked with mgc_heap.cpp): Alloc routes to the real
 * slow-path allocator, IGCHeap::Initialize brings up the region +
 * write-barrier stomp, GarbageCollect dumps the allocation census
 * before its fail-fast when no collector is linked, and the
 * heap-membership queries answer for the real region. Without the
 * define this file is byte-for-byte the bare skeleton -- the skeleton
 * gate case keeps gating it.
 */

#include "mgc_env.h"
#include "gcinterface.h"
#include "gcenv.ee.h"     /* the FLAT GCToEEInterface the collector calls the EE through */
#include "gcenv.object.h" /* ObjHeader/MethodTable/Object mirrors */
#include "gcdesc.h"       /* CGCDesc series format (compile-proof for the collector) */

#include <stdio.h>
#include <string.h>

extern "C" void abort(void);
extern "C" int  sched_yield(void);

#ifdef MGC_HEAP_LIVE
/* the allocator core (mgc_heap.cpp) */
extern "C" void     mgc_heap_init(void);
extern "C" void*    mgc_heap_alloc(gc_alloc_context* acontext, size_t size,
                                   uint32_t flags);
extern "C" void     mgc_heap_census(const char* why);
extern "C" uint32_t mgc_heap_used_bytes(void);
extern "C" uint32_t mgc_heap_total_alloc_bytes(void);
extern "C" int      mgc_heap_contains(const void* p);
extern "C" void     mgc_heap_range(uint8_t** lo, uint8_t** hi);
#ifdef MGC_MULTIRANGE
extern "C" uint32_t mgc_heap_ranges(void);
extern "C" void     mgc_heap_range_get(uint32_t, uint8_t**, uint8_t**, uint8_t**);
#endif
#endif

#ifdef MGC_COLLECT_LIVE
/* the suspension+mark collector (mgc_collect.cpp); only ever
 * defined together with MGC_HEAP_LIVE */
#ifdef MGC_SWEEP_LIVE
/* the sweep's finalization surface (mgc_heap.cpp): the f-reachable
 * drain + re-registration */
extern "C" uint32_t mgc_freach_count(void);
extern "C" Object*  mgc_freach_pop(void);
extern "C" int      mgc_finq_append(Object* obj);
extern "C" uint32_t mgc_heap_live_bytes(void);
#endif
#ifdef MGC_TUNE_LIVE
/* the windows surface: window retire (thread-detach path) + the
 * kernel futex that backs the GC-done event (the host ABI's ONE
 * blocking primitive; deadline is absolute monotonic ns). The futex
 * verbs are the RHU hosting API's (rhu.h). */
extern "C" void    mgc_heap_window_fix(gc_alloc_context* acontext);
#include "rhu.h"
/* GC-done event: 1 = no GC in progress (initial), 0 = GC running.
 * Reset/Set are vcalled by the EE inside SuspendEE/RestartEE; waiters
 * sleep on the word. */
static volatile int32_t g_gcdone = 1;
#endif
extern "C" void     mgc_collect_init(void);
extern "C" int      mgc_heap_collect(int generation, int low_memory_p,
                                     int mode);
extern "C" uint32_t mgc_collect_count(int gen);
extern "C" uint32_t mgc_collect_total(void);
#endif

/* ------------------------------------------------------------------ */
/* logging + fail-fast                                                */
/* ------------------------------------------------------------------ */

#define SKLOG(fmt, ...) \
    do { printf("[mgc-skel] " fmt "\n", ##__VA_ARGS__); fflush(stdout); } while (0)

/* One call-inventory line shape, greppable: "call <Iface>::<Method>" */
#define SKCALL(fmt, ...) SKLOG("call " fmt, ##__VA_ARGS__)

/* Once a live allocator is behind the interface, a handful of BCL
 * surface methods can be called at allocation-era frequency; their
 * logs throttle (first 8 in full, then every 1024th -- the periodic
 * line carries the running count) so a run stays transcript-bounded.
 * In skeleton-only builds nothing ever reaches the throttle (the
 * first allocation dies). */
#define SKCALL_T(ctr, fmt, ...) \
    do { static uint32_t ctr##_n; uint32_t v_ = ++ctr##_n; \
         if (v_ <= 8) SKCALL(fmt, ##__VA_ARGS__); \
         else if ((v_ & 1023u) == 0) \
             SKCALL(fmt " (call #%lu)", ##__VA_ARGS__, (unsigned long)v_); \
    } while (0)

static void mgc_failfast(const char* what)
{
    printf("[mgc-skel] FAIL-FAST: %s -- bare skeleton by design: no collector behind the interface\n",
           what);
    fflush(stdout);
    abort();
}

/* The deleting-destructor slot in IGCHandleStore's vtable references
 * operator delete. Deliberately NOT defined here: the runtime archives
 * carry NativeAOT's minimal C++ runtime shim (Bootstrap/stdcppshim.cpp:
 * operator delete/delete[] in plain AND sized forms plus NOTHROW-only
 * operator new/new[]), and a second definition collides at link time.
 * Nothing ever heap-deletes these singletons, so the slot's backing
 * implementation is unreachable either way -- it must merely link.
 * The nothrow-only new surface also keeps any accidental heap `new`
 * in mgc code a LINK ERROR, which is the intended discipline. */

/* ------------------------------------------------------------------ */
/* handle slots -- the one genuinely functional surface               */
/*                                                                    */
/* The EE reads a handle by dereferencing the OBJECTHANDLE as an      */
/* Object** (that is why OBJECTHANDLE__ is a pointer-to-slot type),   */
/* so handles must be REAL slots that store real object pointers.     */
/* Creation/store/read work fully; only GC lifetime semantics are     */
/* absent, which is fine -- the first allocation dies before any      */
/* collection could matter. The bare skeleton hands slots out         */
/* monotonically (slot reuse belongs to the windows surface,          */
/* MGC_TUNE_LIVE); exhaustion fails fast rather than wrapping.        */
/* ------------------------------------------------------------------ */

/* 4096 by default: a managed payload's statics spines, weak
 * references and task-era handles land here once a live allocator
 * lets the run proceed, and the census records the real high-water
 * for collector sizing. The board rows make it a knob (the default's
 * slot arrays cost 57,344 B; the drive worlds run at 64 with a
 * measured live high-water of 8) and MGC_TUNE_LIVE adds SLOT REUSE --
 * without reuse any fixed pool is lifetime-bounded, which no
 * long-running application can accept. */
#ifndef MGC_HANDLE_SLOTS
#define MGC_HANDLE_SLOTS 4096
#endif

static Object*  g_slot[MGC_HANDLE_SLOTS];       /* the slot the EE dereferences */
static Object*  g_slot_secondary[MGC_HANDLE_SLOTS]; /* dependent-handle secondary */
static void*    g_slot_extra[MGC_HANDLE_SLOTS];
static uint8_t  g_slot_type[MGC_HANDLE_SLOTS];
static uint8_t  g_slot_live[MGC_HANDLE_SLOTS];
static unsigned g_slot_next;
#ifdef MGC_TUNE_LIVE
static unsigned g_slot_free;        /* free list: index+1, 0 = empty
                                     * (threaded through g_slot_extra
                                     * of dead slots) */
static unsigned g_slot_created;     /* monotonic (census) */
static unsigned g_slot_reused;
static unsigned g_slot_live_now, g_slot_live_hw;  /* the knob-table sizing number */
static int      g_slot_selftest;    /* suppress per-create logs during
                                     * the init reuse self-test */

static void slot_release(unsigned i)
{
    g_slot_live[i] = 0;
    g_slot[i] = nullptr;
    g_slot_secondary[i] = nullptr;
    g_slot_extra[i] = (void*)(uintptr_t)g_slot_free;
    g_slot_free = i + 1;
    g_slot_live_now--;
}
#endif

static unsigned slot_of(OBJECTHANDLE h, const char* via)
{
    Object** p = (Object**)h;
    if (p < &g_slot[0] || p >= &g_slot[MGC_HANDLE_SLOTS]) {
        SKLOG("%s: foreign OBJECTHANDLE %p (outside the slot pool)", via, (void*)h);
        mgc_failfast("foreign OBJECTHANDLE");
    }
    return (unsigned)(p - &g_slot[0]);
}

static OBJECTHANDLE mk_handle(Object* obj, int type, void* extra,
                              Object* secondary, const char* via)
{
#ifdef MGC_TUNE_LIVE
    unsigned i;
    if (g_slot_free) {
        i = g_slot_free - 1;
        g_slot_free = (unsigned)(uintptr_t)g_slot_extra[i];
        g_slot_reused++;
    } else {
        if (g_slot_next >= MGC_HANDLE_SLOTS)
            mgc_failfast("handle slot pool exhausted (raise MGC_HANDLE_SLOTS)");
        i = g_slot_next++;
    }
    g_slot_created++;
    g_slot_live_now++;
    if (g_slot_live_now > g_slot_live_hw) g_slot_live_hw = g_slot_live_now;
#else
    if (g_slot_next >= MGC_HANDLE_SLOTS)
        mgc_failfast("handle slot pool exhausted (raise MGC_HANDLE_SLOTS)");
    unsigned i = g_slot_next++;
#endif
    g_slot[i]           = obj;
    g_slot_secondary[i] = secondary;
    g_slot_extra[i]     = extra;
    g_slot_type[i]      = (uint8_t)type;
    g_slot_live[i]      = 1;
    /* full detail for the first 16 (init-era evidence), then every
     * 256th -- allocation-era handle traffic would otherwise flood the
     * transcript; the census carries the totals */
#ifdef MGC_TUNE_LIVE
    if (!g_slot_selftest
        && (g_slot_created <= 24 || (g_slot_created & 255u) == 0))
        SKLOG("handle[%u] created via %s: type=%d obj=%p secondary=%p extra=%p (create #%u)",
              i, via, type, (void*)obj, (void*)secondary, extra,
              g_slot_created);
#else
    if (i < 16 || ((i + 1) & 255u) == 0)
        SKLOG("handle[%u] created via %s: type=%d obj=%p secondary=%p extra=%p",
              i, via, type, (void*)obj, (void*)secondary, extra);
#endif
    return (OBJECTHANDLE)&g_slot[i];
}

/* ------------------------------------------------------------------ */
/* frozen segment registry -- must be real: module init registers the */
/* payload's frozen-object segment and later asks membership          */
/* questions about it (its geometry is logged in full: a frozen       */
/* segment may live in flash).                                        */
/* ------------------------------------------------------------------ */

struct FrozenSeg {
    uint8_t* base;      /* pvMem */
    size_t   first;     /* ibFirstObject */
    size_t   allocated; /* ibAllocated */
    size_t   committed; /* ibCommit */
    size_t   reserved;  /* ibReserved */
    uint8_t  live;
};

#define MGC_FROZEN_SEGS 8
static FrozenSeg g_frozen[MGC_FROZEN_SEGS];
static unsigned  g_frozen_next;

static FrozenSeg* frozen_of(segment_handle h, const char* via)
{
    FrozenSeg* s = (FrozenSeg*)h;
    if (s < &g_frozen[0] || s >= &g_frozen[MGC_FROZEN_SEGS]) {
        SKLOG("%s: foreign segment_handle %p", via, (void*)h);
        mgc_failfast("foreign segment_handle");
    }
    return s;
}

static bool in_frozen(const void* p)
{
    for (unsigned i = 0; i < g_frozen_next; i++) {
        const FrozenSeg* s = &g_frozen[i];
        if (s->live && (const uint8_t*)p >= s->base
                    && (const uint8_t*)p <  s->base + s->reserved)
            return true;
    }
    return false;
}

/* Handle-pool + frozen-segment recap for the allocator census (called by
 * mgc_heap_census; unreferenced -- and gc-sections-dropped -- in
 * skeleton-only builds). */
extern "C" void mgc_skel_census(void)
{
    unsigned live = 0, by_type[12] = {0};
    for (unsigned i = 0; i < g_slot_next; i++) {
        if (g_slot_live[i]) live++;
        if (g_slot_type[i] < 12) by_type[g_slot_type[i]]++;
    }
#ifdef MGC_TUNE_LIVE
    SKLOG("handles: created=%u live=%u of %u (reused=%u live_hw=%u) (types: ws=%u wl=%u strong=%u pinned=%u var=%u rc=%u dep=%u ap=%u sr=%u com=%u wip=%u xref=%u)",
          g_slot_created, live, (unsigned)MGC_HANDLE_SLOTS,
          g_slot_reused, g_slot_live_hw,
          by_type[0], by_type[1], by_type[2], by_type[3], by_type[4],
          by_type[5], by_type[6], by_type[7], by_type[8], by_type[9],
          by_type[10], by_type[11]);
#else
    SKLOG("handles: created=%u live=%u of %u (types: ws=%u wl=%u strong=%u pinned=%u var=%u rc=%u dep=%u ap=%u sr=%u com=%u wip=%u xref=%u)",
          g_slot_next, live, (unsigned)MGC_HANDLE_SLOTS,
          by_type[0], by_type[1], by_type[2], by_type[3], by_type[4],
          by_type[5], by_type[6], by_type[7], by_type[8], by_type[9],
          by_type[10], by_type[11]);
#endif
    for (unsigned i = 0; i < g_frozen_next; i++)
        SKLOG("frozen[%u]: base=%p firstObj=+%lu allocated=%lu committed=%lu reserved=%lu live=%d",
              i, (void*)g_frozen[i].base,
              (unsigned long)g_frozen[i].first,
              (unsigned long)g_frozen[i].allocated,
              (unsigned long)g_frozen[i].committed,
              (unsigned long)g_frozen[i].reserved, (int)g_frozen[i].live);
}

/* collector surface: iterate live handle slots (root scanning +
 * the dependent-handle fixpoint) and expose frozen-segment geometry
 * (frozen objects are walked as root sources every collection -- no
 * barrier records frozen->heap stores). Unreferenced (and
 * gc-sections-dropped) below collector-tier images. */
extern "C" void mgc_skel_handles_foreach(
    void (*fn)(unsigned idx, Object** pslot, Object** psecondary,
               int type, void* arg), void* arg)
{
    for (unsigned i = 0; i < g_slot_next; i++) {
        if (!g_slot_live[i])
            continue;
        fn(i, &g_slot[i], &g_slot_secondary[i], (int)g_slot_type[i], arg);
    }
}

extern "C" int mgc_skel_frozen_get(unsigned i, uint8_t** base,
                                   size_t* first, size_t* allocated)
{
    if (i >= g_frozen_next)
        return -1;              /* past the last registered segment */
    if (!g_frozen[i].live)
        return 0;               /* unregistered: skip */
    if (base)      *base = g_frozen[i].base;
    if (first)     *first = g_frozen[i].first;
    if (allocated) *allocated = g_frozen[i].allocated;
    return 1;
}

/* ------------------------------------------------------------------ */
/* IGCHandleStore                                                     */
/* ------------------------------------------------------------------ */

class MgcSkelHandleStore final : public IGCHandleStore {
public:
    void Uproot() override
    {
        SKCALL("IGCHandleStore::Uproot");
    }

    bool ContainsHandle(OBJECTHANDLE handle) override
    {
        Object** p = (Object**)handle;
        bool in = (p >= &g_slot[0] && p < &g_slot[MGC_HANDLE_SLOTS]);
        SKCALL("IGCHandleStore::ContainsHandle(%p) -> %d", (void*)handle, (int)in);
        return in;
    }

    OBJECTHANDLE CreateHandleOfType(Object* object, HandleType type) override
    {
        SKCALL("IGCHandleStore::CreateHandleOfType(obj=%p type=%d)",
               (void*)object, (int)type);
        return mk_handle(object, (int)type, nullptr, nullptr, "store");
    }

    OBJECTHANDLE CreateHandleOfType(Object* object, HandleType type,
                                    int heapToAffinitizeTo) override
    {
        SKCALL("IGCHandleStore::CreateHandleOfType(obj=%p type=%d heap=%d)",
               (void*)object, (int)type, heapToAffinitizeTo);
        return mk_handle(object, (int)type, nullptr, nullptr, "store/affinitized");
    }

    OBJECTHANDLE CreateHandleWithExtraInfo(Object* object, HandleType type,
                                           void* pExtraInfo) override
    {
        SKCALL("IGCHandleStore::CreateHandleWithExtraInfo(obj=%p type=%d extra=%p)",
               (void*)object, (int)type, pExtraInfo);
        return mk_handle(object, (int)type, pExtraInfo, nullptr, "store/extra");
    }

    OBJECTHANDLE CreateDependentHandle(Object* primary, Object* secondary) override
    {
        SKCALL("IGCHandleStore::CreateDependentHandle(primary=%p secondary=%p)",
               (void*)primary, (void*)secondary);
        return mk_handle(primary, (int)HNDTYPE_DEPENDENT, nullptr, secondary,
                         "store/dependent");
    }
};

/* ------------------------------------------------------------------ */
/* IGCHandleManager                                                   */
/* ------------------------------------------------------------------ */

static MgcSkelHandleStore* g_store; /* placement-new'd in GC_Initialize */

class MgcSkelHandleManager final : public IGCHandleManager {
public:
    bool Initialize() override
    {
        SKCALL("IGCHandleManager::Initialize -> true");
        return true;
    }

    void Shutdown() override
    {
        SKCALL("IGCHandleManager::Shutdown");
    }

    IGCHandleStore* GetGlobalHandleStore() override
    {
        SKCALL("IGCHandleManager::GetGlobalHandleStore -> %p", (void*)g_store);
        return g_store;
    }

    IGCHandleStore* CreateHandleStore() override
    {
        /* Multiple stores are CoreCLR appdomain machinery; NativeAOT is
         * not expected to ask. Loud, and served with the global store
         * so a surprise caller still functions. */
        SKCALL("IGCHandleManager::CreateHandleStore (UNEXPECTED on this profile) -> global store");
        return g_store;
    }

    void DestroyHandleStore(IGCHandleStore* store) override
    {
        SKCALL("IGCHandleManager::DestroyHandleStore(%p)", (void*)store);
    }

    OBJECTHANDLE CreateGlobalHandleOfType(Object* object, HandleType type) override
    {
        SKCALL("IGCHandleManager::CreateGlobalHandleOfType(obj=%p type=%d)",
               (void*)object, (int)type);
        return mk_handle(object, (int)type, nullptr, nullptr, "manager/global");
    }

    OBJECTHANDLE CreateDuplicateHandle(OBJECTHANDLE handle) override
    {
        unsigned i = slot_of(handle, "CreateDuplicateHandle");
        SKCALL("IGCHandleManager::CreateDuplicateHandle(handle[%u])", i);
        return mk_handle(g_slot[i], (int)g_slot_type[i], g_slot_extra[i],
                         g_slot_secondary[i], "manager/duplicate");
    }

    void DestroyHandleOfType(OBJECTHANDLE handle, HandleType type) override
    {
        unsigned i = slot_of(handle, "DestroyHandleOfType");
        SKCALL("IGCHandleManager::DestroyHandleOfType(handle[%u] type=%d)",
               i, (int)type);
#ifdef MGC_TUNE_LIVE
        if (!g_slot_live[i]) {
            SKLOG("DestroyHandleOfType: handle[%u] already dead (double destroy ignored)", i);
            return;
        }
        slot_release(i);
#else
        g_slot_live[i] = 0;
        g_slot[i] = nullptr;
#endif
    }

    void DestroyHandleOfUnknownType(OBJECTHANDLE handle) override
    {
        unsigned i = slot_of(handle, "DestroyHandleOfUnknownType");
        SKCALL("IGCHandleManager::DestroyHandleOfUnknownType(handle[%u])", i);
#ifdef MGC_TUNE_LIVE
        if (!g_slot_live[i]) {
            SKLOG("DestroyHandleOfUnknownType: handle[%u] already dead (double destroy ignored)", i);
            return;
        }
        slot_release(i);
#else
        g_slot_live[i] = 0;
        g_slot[i] = nullptr;
#endif
    }

    void SetExtraInfoForHandle(OBJECTHANDLE handle, HandleType type,
                               void* pExtraInfo) override
    {
        unsigned i = slot_of(handle, "SetExtraInfoForHandle");
        SKCALL("IGCHandleManager::SetExtraInfoForHandle(handle[%u] type=%d extra=%p)",
               i, (int)type, pExtraInfo);
        g_slot_extra[i] = pExtraInfo;
    }

    void* GetExtraInfoFromHandle(OBJECTHANDLE handle) override
    {
        unsigned i = slot_of(handle, "GetExtraInfoFromHandle");
        SKCALL("IGCHandleManager::GetExtraInfoFromHandle(handle[%u]) -> %p",
               i, g_slot_extra[i]);
        return g_slot_extra[i];
    }

    void StoreObjectInHandle(OBJECTHANDLE handle, Object* object) override
    {
        unsigned i = slot_of(handle, "StoreObjectInHandle");
        SKCALL("IGCHandleManager::StoreObjectInHandle(handle[%u] obj=%p)",
               i, (void*)object);
        g_slot[i] = object;
    }

    bool StoreObjectInHandleIfNull(OBJECTHANDLE handle, Object* object) override
    {
        unsigned i = slot_of(handle, "StoreObjectInHandleIfNull");
        Object* expected = nullptr;
        bool stored = __atomic_compare_exchange_n(&g_slot[i], &expected, object,
                                                  false, __ATOMIC_SEQ_CST,
                                                  __ATOMIC_SEQ_CST);
        SKCALL("IGCHandleManager::StoreObjectInHandleIfNull(handle[%u] obj=%p) -> %d",
               i, (void*)object, (int)stored);
        return stored;
    }

    void SetDependentHandleSecondary(OBJECTHANDLE handle, Object* object) override
    {
        unsigned i = slot_of(handle, "SetDependentHandleSecondary");
        SKCALL("IGCHandleManager::SetDependentHandleSecondary(handle[%u] obj=%p)",
               i, (void*)object);
        g_slot_secondary[i] = object;
    }

    Object* GetDependentHandleSecondary(OBJECTHANDLE handle) override
    {
        unsigned i = slot_of(handle, "GetDependentHandleSecondary");
        SKCALL("IGCHandleManager::GetDependentHandleSecondary(handle[%u]) -> %p",
               i, (void*)g_slot_secondary[i]);
        return g_slot_secondary[i];
    }

    Object* InterlockedCompareExchangeObjectInHandle(OBJECTHANDLE handle,
                                                     Object* object,
                                                     Object* comparandObject) override
    {
        unsigned i = slot_of(handle, "InterlockedCompareExchangeObjectInHandle");
        Object* expected = comparandObject;
        __atomic_compare_exchange_n(&g_slot[i], &expected, object, false,
                                    __ATOMIC_SEQ_CST, __ATOMIC_SEQ_CST);
        SKCALL("IGCHandleManager::InterlockedCompareExchangeObjectInHandle(handle[%u]) -> %p",
               i, (void*)expected);
        return expected; /* prior value, CAS convention */
    }

    HandleType HandleFetchType(OBJECTHANDLE handle) override
    {
        unsigned i = slot_of(handle, "HandleFetchType");
        SKCALL("IGCHandleManager::HandleFetchType(handle[%u]) -> %d",
               i, (int)g_slot_type[i]);
        return (HandleType)g_slot_type[i];
    }

    void TraceRefCountedHandles(HANDLESCANPROC callback, uintptr_t param1,
                                uintptr_t param2) override
    {
        (void)callback; (void)param1; (void)param2;
        SKCALL("IGCHandleManager::TraceRefCountedHandles (none exist)");
    }
};

/* ------------------------------------------------------------------ */
/* IGCHeap -- 89 virtuals; the vendored header's declaration order   */
/* is the vtable order. Init-time queries succeed with benign truths  */
/* about an empty heap; anything that would require an actual         */
/* collector fails fast.                                              */
/* ------------------------------------------------------------------ */

class MgcSkelHeap final : public IGCHeap {
public:
    /* --- hosting APIs --- */

    bool IsValidSegmentSize(size_t size) override
    {
        SKCALL("IGCHeap::IsValidSegmentSize(%lu) -> true", (unsigned long)size);
        return true;
    }

    bool IsValidGen0MaxSize(size_t size) override
    {
        SKCALL("IGCHeap::IsValidGen0MaxSize(%lu) -> true", (unsigned long)size);
        return true;
    }

    size_t GetValidSegmentSize(bool large_seg) override
    {
        SKCALL("IGCHeap::GetValidSegmentSize(large=%d) -> 1 MiB", (int)large_seg);
        return 1024 * 1024;
    }

    void SetReservedVMLimit(size_t vmlimit) override
    {
        SKCALL("IGCHeap::SetReservedVMLimit(%lu)", (unsigned long)vmlimit);
    }

    /* --- concurrent GC (none exists) --- */

    void WaitUntilConcurrentGCComplete() override
    {
        SKCALL("IGCHeap::WaitUntilConcurrentGCComplete (none)");
    }

    bool IsConcurrentGCInProgress() override
    {
        SKCALL("IGCHeap::IsConcurrentGCInProgress -> false");
        return false;
    }

    void TemporaryEnableConcurrentGC() override
    {
        SKCALL("IGCHeap::TemporaryEnableConcurrentGC (none)");
    }

    void TemporaryDisableConcurrentGC() override
    {
        SKCALL("IGCHeap::TemporaryDisableConcurrentGC (none)");
    }

    bool IsConcurrentGCEnabled() override
    {
        SKCALL("IGCHeap::IsConcurrentGCEnabled -> false");
        return false;
    }

    HRESULT WaitUntilConcurrentGCCompleteAsync(int millisecondsTimeout) override
    {
        SKCALL("IGCHeap::WaitUntilConcurrentGCCompleteAsync(%d) -> S_OK",
               millisecondsTimeout);
        return 0;
    }

    /* --- finalization --- */

    size_t GetNumberOfFinalizable() override
    {
#ifdef MGC_SWEEP_LIVE
        size_t n = mgc_freach_count();
        SKCALL_T(numfin, "IGCHeap::GetNumberOfFinalizable -> %lu",
                 (unsigned long)n);
        return n;
#else
        SKCALL("IGCHeap::GetNumberOfFinalizable -> 0");
        return 0;
#endif
    }

    Object* GetNextFinalizable() override
    {
#ifdef MGC_SWEEP_LIVE
        /* The finalizer thread's drain: RhpGetNextFinalizableObject
         * loops this slot until NULL and honors FINALIZER_RUN EE-side.
         * It runs cooperative, so it is suspension-serialized against
         * the partition; the pop's alloc lock covers concurrent
         * appends. */
        Object* o = mgc_freach_pop();
        SKCALL_T(nextfin, "IGCHeap::GetNextFinalizable -> %p", (void*)o);
        return o;
#else
        SKCALL("IGCHeap::GetNextFinalizable -> null");
        return nullptr;
#endif
    }

    /* --- BCL / System.GC surface --- */

    void GetMemoryInfo(uint64_t* highMemLoadThresholdBytes,
                       uint64_t* totalAvailableMemoryBytes,
                       uint64_t* lastRecordedMemLoadBytes,
                       uint64_t* lastRecordedHeapSizeBytes,
                       uint64_t* lastRecordedFragmentationBytes,
                       uint64_t* totalCommittedBytes,
                       uint64_t* promotedBytes,
                       uint64_t* pinnedObjectCount,
                       uint64_t* finalizationPendingCount,
                       uint64_t* index,
                       uint32_t* generation,
                       uint32_t* pauseTimePct,
                       bool* isCompaction,
                       bool* isConcurrent,
                       uint64_t* genInfoRaw,
                       uint64_t* pauseInfoRaw,
                       int kind) override
    {
        SKCALL_T(memflags_gmi, "IGCHeap::GetMemoryInfo(kind=%d) -> zeros", kind);
        if (highMemLoadThresholdBytes)   *highMemLoadThresholdBytes = 0;
        if (totalAvailableMemoryBytes)   *totalAvailableMemoryBytes = 0;
        if (lastRecordedMemLoadBytes)    *lastRecordedMemLoadBytes = 0;
        if (lastRecordedHeapSizeBytes)   *lastRecordedHeapSizeBytes = 0;
        if (lastRecordedFragmentationBytes) *lastRecordedFragmentationBytes = 0;
        if (totalCommittedBytes)         *totalCommittedBytes = 0;
        if (promotedBytes)               *promotedBytes = 0;
        if (pinnedObjectCount)           *pinnedObjectCount = 0;
        if (finalizationPendingCount)    *finalizationPendingCount = 0;
        if (index)                       *index = 0;
        if (generation)                  *generation = 0;
        if (pauseTimePct)                *pauseTimePct = 0;
        if (isCompaction)                *isCompaction = false;
        if (isConcurrent)                *isConcurrent = false;
        if (genInfoRaw)                  *genInfoRaw = 0;
        if (pauseInfoRaw)                *pauseInfoRaw = 0;
    }

    uint32_t GetMemoryLoad() override
    {
        SKCALL_T(memload, "IGCHeap::GetMemoryLoad -> 0");
        return 0;
    }

    int GetGcLatencyMode() override
    {
        SKCALL("IGCHeap::GetGcLatencyMode -> 1 (Interactive)");
        return 1;
    }

    int SetGcLatencyMode(int newLatencyMode) override
    {
        SKCALL("IGCHeap::SetGcLatencyMode(%d) -> 0", newLatencyMode);
        return 0;
    }

    int GetLOHCompactionMode() override
    {
        SKCALL("IGCHeap::GetLOHCompactionMode -> 1 (Default)");
        return 1;
    }

    void SetLOHCompactionMode(int newLOHCompactionMode) override
    {
        SKCALL("IGCHeap::SetLOHCompactionMode(%d)", newLOHCompactionMode);
    }

    bool RegisterForFullGCNotification(uint32_t gen2Percentage,
                                       uint32_t lohPercentage) override
    {
        SKCALL("IGCHeap::RegisterForFullGCNotification(%lu,%lu) -> true",
               (unsigned long)gen2Percentage, (unsigned long)lohPercentage);
        return true;
    }

    bool CancelFullGCNotification() override
    {
        SKCALL("IGCHeap::CancelFullGCNotification -> true");
        return true;
    }

    int WaitForFullGCApproach(int millisecondsTimeout) override
    {
        SKCALL("IGCHeap::WaitForFullGCApproach(%d) -> wait_full_gc_na",
               millisecondsTimeout);
        return (int)wait_full_gc_na;
    }

    int WaitForFullGCComplete(int millisecondsTimeout) override
    {
        SKCALL("IGCHeap::WaitForFullGCComplete(%d) -> wait_full_gc_na",
               millisecondsTimeout);
        return (int)wait_full_gc_na;
    }

    unsigned WhichGeneration(Object* obj) override
    {
        /* Frozen objects answer like the mature generation; live-heap
         * objects are generation 0 (the only one mgc has); anything
         * else is not on this heap -- INT32_MAX per the interface
         * comment for non-GC-heap objects. */
        unsigned g;
        const char* kind;
#ifdef MGC_HEAP_LIVE
        if (mgc_heap_contains(obj)) { g = 0; kind = " (heap)"; }
        else
#endif
        if (in_frozen(obj)) { g = 2u; kind = " (frozen)"; }
        else { g = (unsigned)INT32_MAX; kind = " (not on heap)"; }
        SKCALL_T(whichgen, "IGCHeap::WhichGeneration(%p) -> %u%s",
                 (void*)obj, g, kind);
        return g;
    }

    int CollectionCount(int generation, int get_bgc_fgc_coutn) override
    {
        (void)get_bgc_fgc_coutn;
#ifdef MGC_COLLECT_LIVE
        /* Real counters. Load-bearing beyond System.GC: the runtime's
         * RhWaitForPendingFinalizers loop compares
         * CollectionCount(GetMaxGeneration()) against the count the
         * finalizer thread reports back through
         * RhpSignalFinalizationComplete. */
        int v = (int)mgc_collect_count(generation);
        SKCALL_T(collcount, "IGCHeap::CollectionCount(gen=%d) -> %d",
                 generation, v);
        return v;
#else
        SKCALL_T(collcount, "IGCHeap::CollectionCount(gen=%d) -> 0", generation);
        return 0;
#endif
    }

    int StartNoGCRegion(uint64_t totalSize, bool lohSizeKnown, uint64_t lohSize,
                        bool disallowFullBlockingGC) override
    {
        (void)lohSizeKnown; (void)lohSize; (void)disallowFullBlockingGC;
        SKCALL("IGCHeap::StartNoGCRegion(size=%lu) -> success (vacuous: no GC exists)",
               (unsigned long)totalSize);
        return (int)start_no_gc_success;
    }

    int EndNoGCRegion() override
    {
        SKCALL("IGCHeap::EndNoGCRegion -> not_in_progress");
        return (int)end_no_gc_not_in_progress;
    }

    size_t GetTotalBytesInUse() override
    {
#if defined(MGC_SWEEP_LIVE)
        size_t v = mgc_heap_live_bytes();  /* bump extent - free list */
#elif defined(MGC_HEAP_LIVE)
        size_t v = mgc_heap_used_bytes();
#else
        size_t v = 0;
#endif
        SKCALL_T(gtbiu, "IGCHeap::GetTotalBytesInUse -> %lu", (unsigned long)v);
        return v;
    }

    uint64_t GetTotalAllocatedBytes() override
    {
#ifdef MGC_HEAP_LIVE
        uint64_t v = mgc_heap_total_alloc_bytes();
#else
        uint64_t v = 0;
#endif
        SKCALL_T(gtab, "IGCHeap::GetTotalAllocatedBytes -> %lu",
                 (unsigned long)v);
        return v;
    }

    HRESULT GarbageCollect(int generation, bool low_memory_p, int mode) override
    {
        SKCALL("IGCHeap::GarbageCollect(gen=%d low_mem=%d mode=%d)",
               generation, (int)low_memory_p, mode);
#if defined(MGC_COLLECT_LIVE)
        /* The real collection: suspension + mark, and under
         * MGC_SWEEP_LIVE the death pipeline and the sweep. */
        return mgc_heap_collect(generation, low_memory_p ? 1 : 0, mode);
#else
#ifdef MGC_HEAP_LIVE
        /* Collection-entry evidence: what the world looked like when the
         * first collection was demanded. */
        mgc_heap_census("GarbageCollect");
#endif
        mgc_failfast("collection requested (IGCHeap::GarbageCollect)");
        return 0; /* unreachable */
#endif
    }

    unsigned GetMaxGeneration() override
    {
        SKCALL("IGCHeap::GetMaxGeneration -> 2");
        return 2;
    }

    void SetFinalizationRun(Object* obj) override
    {
        SKCALL_T(setfinrun, "IGCHeap::SetFinalizationRun(%p)", (void*)obj);
#ifdef MGC_SWEEP_LIVE
        /* GC.SuppressFinalize routes HERE (RhSuppressFinalize vcalls
         * this slot after the MT HasFinalizer check). The contract:
         * set the FINALIZER_RUN header bit and leave the queue entry;
         * the partition drops-and-clears it when the object dies, the
         * drain skips-and-clears it if already promoted. */
        (((ObjHeader*)obj) - 1)->SetBit(BIT_SBLK_FINALIZER_RUN);
#endif
    }

    bool RegisterForFinalization(int gen, Object* obj) override
    {
        SKCALL_T(regfin, "IGCHeap::RegisterForFinalization(gen=%d obj=%p) -> true",
                 gen, (void*)obj);
#ifdef MGC_SWEEP_LIVE
        /* GC.ReRegisterForFinalize routes here (gen=-1). The contract:
         * FINALIZER_RUN set -> just clear it (the entry never left the
         * queue); else genuinely (re-)append, which covers
         * re-registration after a finalizer already ran. */
        ObjHeader* h = ((ObjHeader*)obj) - 1;
        if (h->GetBits() & BIT_SBLK_FINALIZER_RUN) {
            h->ClrBit(BIT_SBLK_FINALIZER_RUN);
            return true;
        }
        return mgc_finq_append(obj) != 0;
#else
        return true;
#endif
    }

    int GetLastGCPercentTimeInGC() override
    {
        SKCALL("IGCHeap::GetLastGCPercentTimeInGC -> 0");
        return 0;
    }

    size_t GetLastGCGenerationSize(int gen) override
    {
        SKCALL("IGCHeap::GetLastGCGenerationSize(gen=%d) -> 0", gen);
        return 0;
    }

    /* --- miscellaneous VM entry points --- */

    HRESULT Initialize() override
    {
        SKCALL("IGCHeap::Initialize -> S_OK");
#ifdef MGC_HEAP_LIVE
        /* Region + write-barrier stomp at the same init point the
         * runtime's own collector uses (gc_heap::initialize). The first
         * allocation arrives right after IGCHandleManager::Initialize
         * returns, so everything must be live here. */
        mgc_heap_init();
#endif
#ifdef MGC_COLLECT_LIVE
        /* mark bitmap + mark stack (shim mmap; eager so the arena audit
         * shows them at boot and no allocation happens under
         * suspension) */
        mgc_collect_init();
#endif
        return 0;
    }

    bool IsPromoted(Object* object) override
    {
        SKCALL("IGCHeap::IsPromoted(%p) -> false", (void*)object);
        return false;
    }

    bool IsHeapPointer(void* object, bool small_heap_only) override
    {
        bool in = in_frozen(object) && !small_heap_only;
#ifdef MGC_HEAP_LIVE
        /* the region IS the small object heap */
        if (mgc_heap_contains(object)) in = true;
#endif
        SKCALL_T(isheapptr, "IGCHeap::IsHeapPointer(%p small_only=%d) -> %d",
                 object, (int)small_heap_only, (int)in);
        return in;
    }

    unsigned GetCondemnedGeneration() override
    {
        SKCALL("IGCHeap::GetCondemnedGeneration -> 0");
        return 0;
    }

    bool IsGCInProgressHelper(bool bConsiderGCStart) override
    {
        (void)bConsiderGCStart;
        SKCALL("IGCHeap::IsGCInProgressHelper -> false");
        return false;
    }

    unsigned GetGcCount() override
    {
#ifdef MGC_COLLECT_LIVE
        unsigned v = mgc_collect_total();
        SKCALL_T(gccount, "IGCHeap::GetGcCount -> %u", v);
        return v;
#else
        SKCALL_T(gccount, "IGCHeap::GetGcCount -> 0");
        return 0;
#endif
    }

    bool IsThreadUsingAllocationContextHeap(gc_alloc_context* acontext,
                                            int thread_number) override
    {
        SKCALL("IGCHeap::IsThreadUsingAllocationContextHeap(ctx=%p tn=%d) -> true",
               (void*)acontext, thread_number);
        return true;
    }

    bool IsEphemeral(Object* object) override
    {
        SKCALL("IGCHeap::IsEphemeral(%p) -> false", (void*)object);
        return false;
    }

    uint32_t WaitUntilGCComplete(bool bConsiderGCStart) override
    {
        (void)bConsiderGCStart;
        SKCALL_T(waitgc, "IGCHeap::WaitUntilGCComplete -> 0");
#ifdef MGC_TUNE_LIVE
        /* A real sleep on the GC-done event. EAGAIN (already done),
         * EINTR (an activation broke the wait), or a wake all return
         * to the EE's loop, which re-checks its trap flag -- the
         * standard futex recipe, no lost-wakeup window (the Set stores
         * 1 before waking). RHU_DEADLINE_NONE = no deadline. */
        rhu_futex_wait((const uint32_t*)&g_gcdone, 0, RHU_DEADLINE_NONE);
#else
        /* The EE's trapped-thread loop re-polls this until its trap
         * flag clears; with a no-op WaitForGCEvent the loop is a busy
         * poll, so the minimal honest wait is a yield (it donates the
         * timeslice to the collector on one CPU) behind a throttled
         * log. The futex-backed event above is the refinement; a yield
         * is already correct. */
        sched_yield();
#endif
        return 0;
    }

    void FixAllocContext(gc_alloc_context* acontext, void* arg, void* heap) override
    {
        SKCALL("IGCHeap::FixAllocContext(ctx=%p ptr=%p limit=%p arg=%p heap=%p)",
               (void*)acontext,
               acontext ? (void*)acontext->alloc_ptr : nullptr,
               acontext ? (void*)acontext->alloc_limit : nullptr,
               arg, heap);
#ifdef MGC_TUNE_LIVE
        /* The thread-detach path: a dying thread's outstanding window
         * must be retired or its span is unwalkable garbage. */
        mgc_heap_window_fix(acontext);
#else
        /* Nothing to fix: this skeleton never hands out a bump window,
         * so every alloc context stays {0,0}. */
#endif
    }

    size_t GetCurrentObjSize() override
    {
        SKCALL("IGCHeap::GetCurrentObjSize -> 0");
        return 0;
    }

    void SetGCInProgress(bool fInProgress) override
    {
        SKCALL("IGCHeap::SetGCInProgress(%d)", (int)fInProgress);
    }

    bool RuntimeStructuresValid() override
    {
        SKCALL("IGCHeap::RuntimeStructuresValid -> true");
        return true;
    }

    void SetSuspensionPending(bool fSuspensionPending) override
    {
        SKCALL("IGCHeap::SetSuspensionPending(%d)", (int)fSuspensionPending);
    }

    void SetYieldProcessorScalingFactor(float yieldProcessorScalingFactor) override
    {
        SKCALL("IGCHeap::SetYieldProcessorScalingFactor(~%d/100)",
               (int)(yieldProcessorScalingFactor * 100.0f));
    }

    void Shutdown() override
    {
        SKCALL("IGCHeap::Shutdown");
    }

    /* --- memory-pressure timestamps --- */

    size_t GetLastGCStartTime(int generation) override
    {
        SKCALL_T(lastgcstart, "IGCHeap::GetLastGCStartTime(gen=%d) -> 0", generation);
        return 0;
    }

    size_t GetLastGCDuration(int generation) override
    {
        SKCALL_T(lastgcdur, "IGCHeap::GetLastGCDuration(gen=%d) -> 0", generation);
        return 0;
    }

    size_t GetNow() override
    {
        SKCALL_T(getnow, "IGCHeap::GetNow -> 0");
        return 0;
    }

#ifdef MGC_HEAP_LIVE
    /* --- allocation: the live slow-path allocator --- */

    Object* Alloc(gc_alloc_context* acontext, size_t size, uint32_t flags) override
    {
        /* all logging/census lives in mgc_heap_alloc; NULL propagates
         * as the managed OutOfMemoryException protocol */
        return (Object*)mgc_heap_alloc(acontext, size, flags);
    }
#else
    /* --- allocation: THE fail-fast (bare skeleton) --- */

    Object* Alloc(gc_alloc_context* acontext, size_t size, uint32_t flags) override
    {
        printf("[mgc-skel] IGCHeap::Alloc(ctx=%p size=%lu flags=0x%lx)\n",
               (void*)acontext, (unsigned long)size, (unsigned long)flags);
        fflush(stdout);
        mgc_failfast("first object allocation (IGCHeap::Alloc)");
        return nullptr; /* unreachable */
    }
#endif

    void PublishObject(uint8_t* obj) override
    {
        /* LOH/POH allocations only: GcAllocInternal vcalls it when
         * uFlags & 0x60 */
        SKCALL_T(publish, "IGCHeap::PublishObject(%p)", (void*)obj);
    }

    void SetWaitForGCEvent() override
    {
        SKCALL("IGCHeap::SetWaitForGCEvent");
#ifdef MGC_TUNE_LIVE
        /* RestartEE position: GC done -- release the sleepers */
        __atomic_store_n(&g_gcdone, 1, __ATOMIC_RELEASE);
        rhu_futex_wake((const uint32_t*)&g_gcdone, INT32_MAX);
#endif
    }

    void ResetWaitForGCEvent() override
    {
        SKCALL("IGCHeap::ResetWaitForGCEvent");
#ifdef MGC_TUNE_LIVE
        /* SuspendEE position: GC starting -- arm the event */
        __atomic_store_n(&g_gcdone, 0, __ATOMIC_RELEASE);
#endif
    }

    /* --- heap verification --- */

    bool IsLargeObject(Object* pObj) override
    {
        SKCALL("IGCHeap::IsLargeObject(%p) -> false", (void*)pObj);
        return false;
    }

    void ValidateObjectMember(Object* obj) override
    {
        SKCALL("IGCHeap::ValidateObjectMember(%p)", (void*)obj);
    }

    Object* NextObj(Object* object) override
    {
        SKCALL("IGCHeap::NextObj(%p) -> null", (void*)object);
        return nullptr;
    }

    Object* GetContainingObject(void* pInteriorPtr, bool fCollectedGenOnly) override
    {
        (void)fCollectedGenOnly;
        SKCALL("IGCHeap::GetContainingObject(%p) -> null", pInteriorPtr);
        return nullptr;
    }

    /* --- profiling walkers (no heap to walk) --- */

    void DiagWalkObject(Object* obj, walk_fn fn, void* context) override
    {
        (void)fn; (void)context;
        SKCALL("IGCHeap::DiagWalkObject(%p)", (void*)obj);
    }

    void DiagWalkObject2(Object* obj, walk_fn2 fn, void* context) override
    {
        (void)fn; (void)context;
        SKCALL("IGCHeap::DiagWalkObject2(%p)", (void*)obj);
    }

    void DiagWalkHeap(walk_fn fn, void* context, int gen_number,
                      bool walk_large_object_heap_p) override
    {
        (void)fn; (void)context;
        SKCALL("IGCHeap::DiagWalkHeap(gen=%d large=%d)", gen_number,
               (int)walk_large_object_heap_p);
    }

    void DiagWalkSurvivorsWithType(void* gc_context, record_surv_fn fn,
                                   void* diag_context, walk_surv_type type,
                                   int gen_number) override
    {
        (void)gc_context; (void)fn; (void)diag_context;
        SKCALL("IGCHeap::DiagWalkSurvivorsWithType(type=%d gen=%d)",
               (int)type, gen_number);
    }

    void DiagWalkFinalizeQueue(void* gc_context, fq_walk_fn fn) override
    {
        (void)gc_context; (void)fn;
        SKCALL("IGCHeap::DiagWalkFinalizeQueue");
    }

    void DiagScanFinalizeQueue(fq_scan_fn fn, ScanContext* context) override
    {
        (void)fn; (void)context;
        SKCALL("IGCHeap::DiagScanFinalizeQueue");
    }

    void DiagScanHandles(handle_scan_fn fn, int gen_number,
                         ScanContext* context) override
    {
        (void)fn; (void)context;
        SKCALL("IGCHeap::DiagScanHandles(gen=%d)", gen_number);
    }

    void DiagScanDependentHandles(handle_scan_fn fn, int gen_number,
                                  ScanContext* context) override
    {
        (void)fn; (void)context;
        SKCALL("IGCHeap::DiagScanDependentHandles(gen=%d)", gen_number);
    }

    void DiagDescrGenerations(gen_walk_fn fn, void* context) override
    {
        (void)fn; (void)context;
        SKCALL("IGCHeap::DiagDescrGenerations");
    }

    void DiagTraceGCSegments() override
    {
        SKCALL("IGCHeap::DiagTraceGCSegments");
    }

    void DiagGetGCSettings(EtwGCSettingsInfo* settings) override
    {
        SKCALL("IGCHeap::DiagGetGCSettings -> zeros");
        if (settings)
            memset(settings, 0, sizeof(*settings));
    }

    /* --- GC stress --- */

    bool StressHeap(gc_alloc_context* acontext) override
    {
        SKCALL("IGCHeap::StressHeap(ctx=%p) -> false", (void*)acontext);
        return false;
    }

    /* --- frozen segments (a real registry) --- */

    segment_handle RegisterFrozenSegment(segment_info* pseginfo) override
    {
        if (!pseginfo)
            mgc_failfast("RegisterFrozenSegment(NULL)");
        /* Seam guard: a failed upstream reservation must come back as a
         * null HANDLE, never a live segment. The runtime's own collector
         * can reject here, and the frozen-segment manager's null-handle
         * path unwinds to a graceful managed OOM; accepting a bogus
         * base instead turns an honest OOM into writes through
         * 0xffffffff. */
        if (!pseginfo->pvMem || pseginfo->pvMem == (void*)-1) {
            SKCALL("IGCHeap::RegisterFrozenSegment REJECTED (mem=%p) -> null",
                   pseginfo->pvMem);
            return nullptr;
        }
        if (g_frozen_next >= MGC_FROZEN_SEGS)
            mgc_failfast("frozen segment registry full");
        unsigned i = g_frozen_next++;
        g_frozen[i].base      = (uint8_t*)pseginfo->pvMem;
        g_frozen[i].first     = pseginfo->ibFirstObject;
        g_frozen[i].allocated = pseginfo->ibAllocated;
        g_frozen[i].committed = pseginfo->ibCommit;
        g_frozen[i].reserved  = pseginfo->ibReserved;
        g_frozen[i].live      = 1;
        SKCALL("IGCHeap::RegisterFrozenSegment[%u](mem=%p firstObj=+%lu allocated=%lu commit=%lu reserved=%lu)",
               i, pseginfo->pvMem,
               (unsigned long)pseginfo->ibFirstObject,
               (unsigned long)pseginfo->ibAllocated,
               (unsigned long)pseginfo->ibCommit,
               (unsigned long)pseginfo->ibReserved);
        return (segment_handle)&g_frozen[i];
    }

    void UnregisterFrozenSegment(segment_handle seg) override
    {
        FrozenSeg* s = frozen_of(seg, "UnregisterFrozenSegment");
        SKCALL("IGCHeap::UnregisterFrozenSegment[%u]",
               (unsigned)(s - &g_frozen[0]));
        s->live = 0;
    }

    bool IsInFrozenSegment(Object* object) override
    {
        bool in = in_frozen(object);
        SKCALL_T(isinfroz, "IGCHeap::IsInFrozenSegment(%p) -> %d", (void*)object, (int)in);
        return in;
    }

    /* --- eventing --- */

    void ControlEvents(GCEventKeyword keyword, GCEventLevel level) override
    {
        SKCALL("IGCHeap::ControlEvents(keyword=0x%lx level=%d)",
               (unsigned long)keyword, (int)level);
    }

    void ControlPrivateEvents(GCEventKeyword keyword, GCEventLevel level) override
    {
        SKCALL("IGCHeap::ControlPrivateEvents(keyword=0x%lx level=%d)",
               (unsigned long)keyword, (int)level);
    }

    unsigned int GetGenerationWithRange(Object* object, uint8_t** ppStart,
                                        uint8_t** ppAllocated,
                                        uint8_t** ppReserved) override
    {
        for (unsigned i = 0; i < g_frozen_next; i++) {
            FrozenSeg* s = &g_frozen[i];
            if (s->live && (uint8_t*)object >= s->base
                        && (uint8_t*)object < s->base + s->reserved) {
                if (ppStart)     *ppStart = s->base;
                if (ppAllocated) *ppAllocated = s->base + s->allocated;
                if (ppReserved)  *ppReserved = s->base + s->reserved;
                SKCALL("IGCHeap::GetGenerationWithRange(%p) -> 2 (frozen[%u])",
                       (void*)object, i);
                return 2;
            }
        }
#ifdef MGC_HEAP_LIVE
        if (mgc_heap_contains(object)) {
#ifdef MGC_MULTIRANGE
            /* per-range truth: the range CONTAINING the object answers
             * (start/allocated/reserved are that range's, never the
             * union -- the gap must not read as heap) */
            for (uint32_t r = 0; r < mgc_heap_ranges(); r++) {
                uint8_t *b, *e, *bp;
                mgc_heap_range_get(r, &b, &e, &bp);
                if ((uint8_t*)object >= b && (uint8_t*)object < e) {
                    if (ppStart)     *ppStart = b;
                    if (ppAllocated) *ppAllocated = bp;
                    if (ppReserved)  *ppReserved = e;
                    SKCALL_T(genrange, "IGCHeap::GetGenerationWithRange(%p) -> 0 (heap range %lu)",
                             (void*)object, (unsigned long)r);
                    return 0;
                }
            }
#else
            uint8_t *lo, *hi;
            mgc_heap_range(&lo, &hi);
            if (ppStart)     *ppStart = lo;
            if (ppAllocated) *ppAllocated = lo + mgc_heap_used_bytes();
            if (ppReserved)  *ppReserved = hi;
            SKCALL_T(genrange, "IGCHeap::GetGenerationWithRange(%p) -> 0 (heap)",
                     (void*)object);
            return 0;
#endif
        }
#endif
        if (ppStart)     *ppStart = nullptr;
        if (ppAllocated) *ppAllocated = nullptr;
        if (ppReserved)  *ppReserved = nullptr;
        SKCALL("IGCHeap::GetGenerationWithRange(%p) -> 0 (not found)",
               (void*)object);
        return 0;
    }

    /* --- post-5.0 tail (order matters: these sit AFTER the omitted
     *     destructor block in the header, still declaration order) --- */

    int64_t GetTotalPauseDuration() override
    {
        SKCALL("IGCHeap::GetTotalPauseDuration -> 0");
        return 0;
    }

    void EnumerateConfigurationValues(void* context,
                                      ConfigurationValueFunc configurationValueFunc) override
    {
        (void)context; (void)configurationValueFunc;
        SKCALL("IGCHeap::EnumerateConfigurationValues (none reported)");
    }

    void UpdateFrozenSegment(segment_handle seg, uint8_t* allocated,
                             uint8_t* committed) override
    {
        FrozenSeg* s = frozen_of(seg, "UpdateFrozenSegment");
        SKCALL("IGCHeap::UpdateFrozenSegment[%u](allocated=%p committed=%p)",
               (unsigned)(s - &g_frozen[0]), (void*)allocated, (void*)committed);
        s->allocated = (size_t)(allocated - s->base);
        s->committed = (size_t)(committed - s->base);
    }

    int RefreshMemoryLimit() override
    {
        SKCALL("IGCHeap::RefreshMemoryLimit -> refresh_success");
        return (int)refresh_success;
    }

    enable_no_gc_region_callback_status
    EnableNoGCRegionCallback(NoGCRegionCallbackFinalizerWorkItem* callback,
                             uint64_t callback_threshold) override
    {
        SKCALL("IGCHeap::EnableNoGCRegionCallback(cb=%p threshold=%lu) -> not_started",
               (void*)callback, (unsigned long)callback_threshold);
        return not_started;
    }

    FinalizerWorkItem* GetExtraWorkForFinalization() override
    {
        SKCALL("IGCHeap::GetExtraWorkForFinalization -> null");
        return nullptr;
    }

    uint64_t GetGenerationBudget(int generation) override
    {
        SKCALL("IGCHeap::GetGenerationBudget(gen=%d) -> 0", generation);
        return 0;
    }

    size_t GetLOHThreshold() override
    {
        SKCALL_T(lohthresh, "IGCHeap::GetLOHThreshold -> %lu", (unsigned long)LARGE_OBJECT_SIZE);
        return LARGE_OBJECT_SIZE;
    }

    void DiagWalkHeapWithACHandling(walk_fn fn, void* context, int gen_number,
                                    bool walk_large_object_heap_p) override
    {
        (void)fn; (void)context;
        SKCALL("IGCHeap::DiagWalkHeapWithACHandling(gen=%d large=%d)",
               gen_number, (int)walk_large_object_heap_p);
    }

    void NullBridgeObjectsWeakRefs(size_t length, void* unreachableObjectHandles) override
    {
        SKCALL("IGCHeap::NullBridgeObjectsWeakRefs(len=%lu handles=%p)",
               (unsigned long)length, unreachableObjectHandles);
    }
};

/* ------------------------------------------------------------------ */
/* singleton storage + the two seam entry points                      */
/* ------------------------------------------------------------------ */

alignas(MgcSkelHeap)          static unsigned char g_heap_mem[sizeof(MgcSkelHeap)];
alignas(MgcSkelHandleManager) static unsigned char g_mgr_mem[sizeof(MgcSkelHandleManager)];
alignas(MgcSkelHandleStore)   static unsigned char g_store_mem[sizeof(MgcSkelHandleStore)];

/* GcDacVars VAL fields: the values the runtime's own collector writes
 * through the DAC seam (the pointer fields stay zero -- nothing
 * in-process consumes them; they exist for out-of-process debuggers). */
#define MGC_DAC_MAJOR      2u   /* the runtime's DAC major */
#define MGC_DAC_MINOR      8u   /* the runtime's DAC minor */
#define MGC_DAC_GEN_SIZE   136u /* sizeof the collector's dac_generation */
#define MGC_DAC_TOTAL_GENS 5u   /* gen0,1,2 + LOH + POH */

static void mgc_skel_version_info(VersionInfo* v)
{
    /* PRE-HANDSHAKE PRINT -- must be the first observable act of the
     * mgc library, so a version-mismatch death is attributable. */
    printf("[mgc-skel] GC_VersionInfo ENTER (EE passed major=%lu minor=%lu build=%lu name=%p)\n",
           (unsigned long)v->MajorVersion, (unsigned long)v->MinorVersion,
           (unsigned long)v->BuildVersion, (const void*)v->Name);
    fflush(stdout);

#ifdef MGC_SKEL_BAD_VERSION
    /* Negative control: report a wrong major version.
     * The EE's check must kill the boot VISIBLY -- and the ENTER print
     * above must already be on the wire when it does. */
    v->MajorVersion = GC_INTERFACE_MAJOR_VERSION - 1;
#else
    v->MajorVersion = GC_INTERFACE_MAJOR_VERSION;
#endif
    v->MinorVersion = GC_INTERFACE_MINOR_VERSION;
    v->BuildVersion = 0;
    v->Name         = "mgc-skeleton";

    printf("[mgc-skel] GC_VersionInfo reporting major=%lu minor=%lu build=%lu name=\"%s\"\n",
           (unsigned long)v->MajorVersion, (unsigned long)v->MinorVersion,
           (unsigned long)v->BuildVersion, v->Name);
    fflush(stdout);
}

static HRESULT mgc_skel_initialize(IGCToCLR* clrToGC, IGCHeap** ppHeap,
                                   IGCHandleManager** ppMgr, GcDacVars* dac)
{
    /* clrToGC is the EE's IGCToCLR vtable wrapper (non-null by
     * contract). Logged for diagnostics; mgc reaches the EE through
     * the statically linked flat GCToEEInterface symbols, and nothing
     * is called through either route here. */
    SKLOG("GC_Initialize ENTER (clrToGC=%p gcDacVars=%p)", (void*)clrToGC, (void*)dac);

    g_store = new (g_store_mem) MgcSkelHandleStore();
    MgcSkelHandleManager* mgr = new (g_mgr_mem) MgcSkelHandleManager();
    MgcSkelHeap* heap = new (g_heap_mem) MgcSkelHeap();

#ifdef MGC_TUNE_LIVE
    /* Handle-pool reuse self-test: a payload's lifetime handle
     * traffic is far too small to exercise reuse against any sane
     * capacity, so prove it here through the REAL Create/Destroy API:
     * 25 rounds x 8 handles must recycle the same 8 fresh slots. Runs
     * before any real handle exists; per-create logs suppressed; the
     * live high-water it leaves (8) is below a real payload's peak. */
    {
        g_slot_selftest = 1;
        unsigned pre_next = g_slot_next, pre_live = g_slot_live_now;
        OBJECTHANDLE hs[8];
        for (unsigned round = 0; round < 25; round++) {
            for (unsigned k = 0; k < 8; k++)
                hs[k] = mk_handle((Object*)(void*)g_store_mem,
                                  (int)HNDTYPE_STRONG, nullptr, nullptr,
                                  "selftest");
            for (unsigned k = 0; k < 8; k++)
                slot_release(slot_of(hs[k], "selftest"));
        }
        g_slot_selftest = 0;
        unsigned fresh = g_slot_next - pre_next;
        if (fresh <= 8 && g_slot_live_now == pre_live
            && g_slot_reused >= 192)
            SKLOG("handle reuse self-test: 200 create/destroy cycles in %u fresh slots (reused=%u live back to %u) OK",
                  fresh, g_slot_reused, g_slot_live_now);
        else {
            SKLOG("handle reuse self-test FAILED: fresh=%u reused=%u live=%u (pre=%u)",
                  fresh, g_slot_reused, g_slot_live_now, pre_live);
            mgc_failfast("handle reuse self-test failed");
        }
    }
#endif

    if (dac) {
        memset(dac, 0, sizeof(*dac));
        dac->major_version_number   = MGC_DAC_MAJOR;
        dac->minor_version_number   = MGC_DAC_MINOR;
        dac->generation_size        = MGC_DAC_GEN_SIZE;
        dac->total_generation_count = MGC_DAC_TOTAL_GENS;
        SKLOG("GC_Initialize dac VALs written (major=%u minor=%u gen_size=%u total_gens=%u, pointers zeroed)",
              MGC_DAC_MAJOR, MGC_DAC_MINOR, MGC_DAC_GEN_SIZE, MGC_DAC_TOTAL_GENS);
    } else {
        SKLOG("GC_Initialize gcDacVars=NULL (unexpected on this profile)");
    }

    /* EE probe: the first LIVE calls through the statically linked
     * flat GCToEEInterface symbols (the gate proves mangle parity on
     * the object; this proves the route at runtime). The free-object
     * MethodTable doubles as a layout cross-check: when the vendored MT
     * mirror reads it as {HasComponentSize|IsArray, baseSize=12,
     * componentSize=1} -- the InitializeFreeObject recipe -- the
     * mirror's flag values are verified against the live runtime, not
     * just the header. */
    {
        MethodTable* freeMT = GCToEEInterface::GetFreeObjectMethodTable();
        SKLOG("EE probe: GetFreeObjectMethodTable=%p flags=0x%lx baseSize=%lu compSize=%u",
              (void*)freeMT,
              (unsigned long)freeMT->m_flags,
              (unsigned long)freeMT->GetBaseSize(),
              (unsigned)freeMT->RawGetComponentSize());
        bool shape_ok = freeMT->HasComponentSize()
                     && !freeMT->ContainsGCPointers()
                     && freeMT->GetBaseSize() == 3 * sizeof(void*)
                     && freeMT->RawGetComponentSize() == 1;
        SKLOG("EE probe: free-object MT shape %s", shape_ok ? "OK" : "MISMATCH");

        bool bval = true;
        bool have = GCToEEInterface::GetBooleanConfigValue(
                        "gcConcurrent", "System.GC.Concurrent", &bval);
        SKLOG("EE probe: GetBooleanConfigValue(gcConcurrent) have=%d value=%d",
              (int)have, (int)bval);

        Thread* self = GCToEEInterface::GetThread();
        gc_alloc_context* ac = GCToEEInterface::GetAllocContext();
        SKLOG("EE probe: GetThread=%p GetAllocContext=%p (ptr=%p limit=%p)",
              (void*)self, (void*)ac,
              ac ? (void*)ac->alloc_ptr : nullptr,
              ac ? (void*)ac->alloc_limit : nullptr);
        SKLOG("EE probe: GetCurrentProcessCpuCount=%lu",
              (unsigned long)GCToEEInterface::GetCurrentProcessCpuCount());
    }

    *ppHeap = heap;
    *ppMgr  = mgr;
    SKLOG("GC_Initialize EXIT rc=0 heap=%p handleMgr=%p store=%p",
          (void*)heap, (void*)mgr, (void*)g_store);
    return 0;
}

/* ------------------------------------------------------------------ */
/* the shim seam costume (strong defs over the host shim's weak pair) */
/* ------------------------------------------------------------------ */

static int g_mgc_handle_cookie;

extern "C" void* __mgc_dlopen_hook(const char* f)
{
    if (!f || !strstr(f, "mgc"))
        return nullptr; /* not ours -- the shim's honest stub handles it */
    printf("[mgc] dlopen(\"%s\") -> skeleton costume handle\n", f);
    fflush(stdout);
    return &g_mgc_handle_cookie;
}

extern "C" void* __mgc_dlsym_hook(void* h, const char* n)
{
    if (h != (void*)&g_mgc_handle_cookie || !n)
        return nullptr;
    printf("[mgc] dlsym(\"%s\")\n", n);
    fflush(stdout);
    if (strcmp(n, "GC_VersionInfo") == 0)
        return (void*)&mgc_skel_version_info;
    if (strcmp(n, "GC_Initialize") == 0)
        return (void*)&mgc_skel_initialize;
    /* The loader seam has exactly two entry points; a third is a
     * contract change and must fail the gate loudly. */
    printf("[mgc] dlsym: UNHANDLED entry point \"%s\"\n", n);
    fflush(stdout);
    return nullptr;
}
