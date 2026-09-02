/* mgc_ee_probe.cpp -- the mangle-parity probe.
 *
 * References the address of EVERY method the vendored gcenv.ee.h
 * declares on the flat GCToEEInterface class. Compiling this TU and
 * diffing its undefined symbols against the payload's defined symbols
 * machine-proves that the route by which mgc reaches the EE
 * (statically linked flat symbols) resolves for the WHOLE surface --
 * name mangling included -- before any collector code depends on it.
 * Never linked into images; the gate compiles it, nm-diffs it, and
 * throws it away.
 */

#include "mgc_env.h"
#include "gcinterface.h"
#include "gcenv.ee.h"

typedef void (*fn_t)(void);

#define REF(m) (fn_t)(void*)&GCToEEInterface::m

/* volatile so the compiler must materialize every address (each one a
 * relocation against the mangled symbol). */
fn_t volatile g_mgc_ee_surface[] = {
    REF(SuspendEE),
    REF(RestartEE),
    REF(GcScanRoots),
    REF(GcStartWork),
    REF(BeforeGcScanRoots),
    REF(AfterGcScanRoots),
    REF(GcDone),
    REF(RefCountedHandleCallbacks),
    REF(TriggerClientBridgeProcessing),
    REF(SyncBlockCacheWeakPtrScan),
    REF(SyncBlockCacheDemote),
    REF(SyncBlockCachePromotionsGranted),
    REF(GetActiveSyncBlockCount),
    REF(IsPreemptiveGCDisabled),
    REF(EnablePreemptiveGC),
    REF(DisablePreemptiveGC),
    REF(GetThread),
    REF(GetAllocContext),
    REF(GcEnumAllocContexts),
    REF(GetLoaderAllocatorObjectForGC),
    REF(DiagGCStart),
    REF(DiagUpdateGenerationBounds),
    REF(DiagGCEnd),
    REF(DiagWalkFReachableObjects),
    REF(DiagWalkSurvivors),
    REF(DiagWalkUOHSurvivors),
    REF(DiagWalkBGCSurvivors),
    REF(StompWriteBarrier),
    REF(EnableFinalization),
    REF(HandleFatalError),
    REF(EagerFinalized),
    REF(GetFreeObjectMethodTable),
    REF(GetBooleanConfigValue),
    REF(GetIntConfigValue),
    REF(GetStringConfigValue),
    REF(FreeStringConfigValue),
    REF(IsGCThread),
    REF(WasCurrentThreadCreatedByGC),
    REF(CreateThread),
    REF(WalkAsyncPinnedForPromotion),
    REF(WalkAsyncPinned),
    REF(EventSink),
    REF(GetTotalNumSizedRefHandles),
    REF(AnalyzeSurvivorsRequested),
    REF(AnalyzeSurvivorsFinished),
    REF(VerifySyncTableEntry),
    REF(UpdateGCEventStatus),
    REF(LogStressMsg),
    REF(GetCurrentProcessCpuCount),
    REF(DiagAddNewRegion),
    REF(LogErrorToHost),
    REF(GetThreadOSThreadId),
};

extern "C" unsigned mgc_ee_probe_count(void)
{
    return (unsigned)(sizeof(g_mgc_ee_surface) / sizeof(g_mgc_ee_surface[0]));
}
