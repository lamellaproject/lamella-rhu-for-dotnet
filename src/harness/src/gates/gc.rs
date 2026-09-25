//! The gc gate on the runner: the mgc world's gate family, running on
//! FROZEN world objects (goldens/gc) -- the consumer story is Rust +
//! QEMU only; the image-tier C++ compile happens once, on the
//! maintainer's re-derivation leg ('rhu-gate gc rederive'), and its
//! objects are committed.

use crate::gate::{class_flag_sets_tie, write_fixture, Class, Selfcheck};
use crate::objfile::{distinct_stamps, producer_stamp_of, section_span, section_span_prefixed, sha256_without_stamp};
use crate::results::{chain_json_from_env, validate_results, Results};
use crate::sha256::{sha256_file, sha256_hex};
use crate::text::{contains_fixed, cr_count, word_sym_present};
use crate::tools::{
    command_words, echo_file, llvm_tool, resolve_qemu, run_captured, rustc_facts, status_bounded,
    ExecProfile, RustcFacts,
};
use std::path::{Path, PathBuf};
use std::process::{exit, Command, Stdio};
use std::time::Duration;

const GATE: &str = "gc";
const DISPLAY: &str = "GC GATE";
const HOST: &str = "qemu-mps2-an500";
const RUNNER: &str = "src/harness";
const RUN_TIMEOUT_S: u64 = 120;

/// Deadline for one C++ compile or one external link (both may cross a
/// VM boundary; a tool still running at this bound is wedged, not slow).
const TOOL_TIMEOUT_S: u64 = 120;

/// The pinned handshake constants (the vendored header's values, as
/// src/VENDORED.md records them). An independent anchor beside the
/// manifest: doctoring the manifest and
/// the tree together still cannot move these without failing here.
const GC_INTERFACE_MAJOR: u32 = 5;
const GC_INTERFACE_MINOR: u32 = 8;
const EE_INTERFACE_MAJOR: u32 = 5;

/// The compile flag set of record for image-tier C++: clang, the compiler
/// of record (the version the runtime archives at the pin
/// were built with; one LLVM lineage end to end), targeting the image ABI
/// half -- v7E-M, hard-float FPv5-D16, to match thumbv7em-none-eabihf --
/// with the kit's C-library headers as the sysroot the resolver appends, no
/// C++ standard library headers (the collector is freestanding C++ over the
/// C headers alone; clang's own would shadow them), and the include
/// closure. The 32-bit fixed-width types are spelled the archives' way
/// (`int`, `unsigned int`) by the target itself, so the mangled names the
/// collector calls the runtime's flat interface by need no override.
const CXX_FLAGS: [&str; 20] = [
    "-x",
    "c++",
    "-c",
    "-O2",
    "--target=arm-none-eabi",
    "-mcpu=cortex-m7",
    "-mthumb",
    "-mfloat-abi=hard",
    "-mfpu=fpv5-d16",
    "-nostdinc++",
    "-fno-exceptions",
    "-fno-rtti",
    "-fno-threadsafe-statics",
    "-fno-use-cxa-atexit",
    "-ffunction-sections",
    "-fdata-sections",
    "-Isrc/gc",
    "-Isrc/vendor/gcinterface",
    "-Isrc/vendor/gcenv",
    "-Isrc/kernel/include",
];

/// The same flag set for the Cortex-M33 (the mps2-an505 row's objects):
/// one list apart from the core and FPU words, which the selfcheck asserts
/// against `CXX_FLAGS` through the shared class record.
const CXX_FLAGS_CORTEX_M33: [&str; 20] = [
    "-x",
    "c++",
    "-c",
    "-O2",
    "--target=arm-none-eabi",
    "-mcpu=cortex-m33",
    "-mthumb",
    "-mfloat-abi=hard",
    "-mfpu=fpv5-sp-d16",
    "-nostdinc++",
    "-fno-exceptions",
    "-fno-rtti",
    "-fno-threadsafe-statics",
    "-fno-use-cxa-atexit",
    "-ffunction-sections",
    "-fdata-sections",
    "-Isrc/gc",
    "-Isrc/vendor/gcinterface",
    "-Isrc/vendor/gcenv",
    "-Isrc/kernel/include",
];

/// The flag set of record for a class.
fn cxx_flags(class: Class) -> &'static [&'static str] {
    match class {
        Class::CortexM7 => &CXX_FLAGS,
        Class::CortexM33 => &CXX_FLAGS_CORTEX_M33,
    }
}

/// The bare skeleton object's EXACT import closure with the compiler of record:
/// the five flat EE methods the init path calls, the vtable's sized
/// operator delete, and the console C runtime the activation host
/// provides. A name appearing here that the image world does not
/// satisfy, or a NEW import arriving with a header or source change, is
/// a world change and must fail loudly, never resolve by accident.
const IMPORTS_BARE: [&str; 17] = [
    "_ZN15GCToEEInterface15GetAllocContextEv",
    "_ZN15GCToEEInterface21GetBooleanConfigValueEPKcS1_Pb",
    "_ZN15GCToEEInterface24GetFreeObjectMethodTableEv",
    "_ZN15GCToEEInterface25GetCurrentProcessCpuCountEv",
    "_ZN15GCToEEInterface9GetThreadEv",
    "_ZdlPvj",
    "__aeabi_memclr4",
    "_impure_ptr",
    "abort",
    "fflush",
    "printf",
    "puts",
    "rhu_futex_wait",
    "rhu_futex_wake",
    "sched_yield",
    "strcmp",
    "strstr",
];

/// The activation host object's exact import closure: the costume seam
/// pair (skeleton-defined) and the kernel console export.
const IMPORTS_HOST: [&str; 4] = [
    "__aeabi_memset",
    "__mgc_dlopen_hook",
    "__mgc_dlsym_hook",
    "sh_write0",
];

/// The skeleton compiled MGC_HEAP_LIVE: the bare 15 plus the seven
/// allocator-core entry points it delegates to (satisfied in-image by
/// the heap object).
const IMPORTS_SKEL_HEAP: [&str; 24] = [
    "_ZN15GCToEEInterface15GetAllocContextEv",
    "_ZN15GCToEEInterface21GetBooleanConfigValueEPKcS1_Pb",
    "_ZN15GCToEEInterface24GetFreeObjectMethodTableEv",
    "_ZN15GCToEEInterface25GetCurrentProcessCpuCountEv",
    "_ZN15GCToEEInterface9GetThreadEv",
    "_ZdlPvj",
    "__aeabi_memclr4",
    "_impure_ptr",
    "abort",
    "fflush",
    "mgc_heap_alloc",
    "mgc_heap_census",
    "mgc_heap_contains",
    "mgc_heap_init",
    "mgc_heap_range",
    "mgc_heap_total_alloc_bytes",
    "mgc_heap_used_bytes",
    "printf",
    "puts",
    "rhu_futex_wait",
    "rhu_futex_wake",
    "sched_yield",
    "strcmp",
    "strstr",
];

/// The allocator core's exact import closure at MGC_HEAP_LIVE: four
/// flat EE methods (the stomp joins the init-path five; the two
/// context getters back the in-call identity probe), the glibc-shaped
/// region mmap, the skeleton's census recap, and the console runtime
/// (`_impure_ptr` is the same stdio reentrancy-pointer artifact the
/// skeleton carries, satisfied by the host's documented stub).
const IMPORTS_HEAP: [&str; 11] = [
    "_ZN15GCToEEInterface15GetAllocContextEv",
    "_ZN15GCToEEInterface17StompWriteBarrierEP22WriteBarrierParameters",
    "_ZN15GCToEEInterface24GetFreeObjectMethodTableEv",
    "_ZN15GCToEEInterface9GetThreadEv",
    "__aeabi_memclr4",
    "_impure_ptr",
    "abort",
    "fflush",
    "mgc_skel_census",
    "mmap64",
    "printf",
];

/// The driving host object's exact import closure: the costume seam
/// pair, the kernel console export, and the two arena verbs behind the
/// mmap bridge.
const IMPORTS_HOST_DRIVE: [&str; 6] = [
    "__aeabi_memset",
    "__mgc_dlopen_hook",
    "__mgc_dlsym_hook",
    "rhu_arena_alloc_at",
    "rhu_arena_headroom",
    "sh_write0",
];

/// The flat GCToEEInterface surface at the pinned tag: 53 methods (the
/// IGCToCLR virtual count). The gc.B artifact
/// check ties this three ways -- header declarations, probe REF sites,
/// probe-object undefined symbols -- at the NAME level.
const EE_SURFACE: usize = 53;

/// The init-time call inventory of the activation boot (the union of
/// the skeleton's logged calls over gc.A's positive run -- and over the
/// gc.B control's run, which dies at the first allocation before any
/// further surface call).
const INVENTORY_ACTIVATION: [&str; 3] = [
    "IGCHandleManager::GetGlobalHandleStore",
    "IGCHandleManager::Initialize",
    "IGCHeap::Initialize",
];

/// The allocator boot's call inventory: the activation set plus the
/// four BCL-surface queries the drive makes. IGCHeap::Alloc never logs
/// a call line under MGC_HEAP_LIVE (it delegates; the heap's own lines
/// are not inventory) -- a call line for it appearing here would mean
/// the bare fail-fast path compiled into a live image.
const INVENTORY_DRIVE: [&str; 7] = [
    "IGCHandleManager::GetGlobalHandleStore",
    "IGCHandleManager::Initialize",
    "IGCHeap::GetTotalAllocatedBytes",
    "IGCHeap::GetTotalBytesInUse",
    "IGCHeap::Initialize",
    "IGCHeap::IsHeapPointer",
    "IGCHeap::WhichGeneration",
];

/// The skeleton compiled MGC_HEAP_LIVE + MGC_COLLECT_LIVE: the
/// heap-live 22 MINUS mgc_heap_census (GarbageCollect now routes to
/// the collector instead of the census-then-fail-fast path) PLUS the
/// four collector entry points it delegates to.
const IMPORTS_SKEL_COLLECT: [&str; 27] = [
    "_ZN15GCToEEInterface15GetAllocContextEv",
    "_ZN15GCToEEInterface21GetBooleanConfigValueEPKcS1_Pb",
    "_ZN15GCToEEInterface24GetFreeObjectMethodTableEv",
    "_ZN15GCToEEInterface25GetCurrentProcessCpuCountEv",
    "_ZN15GCToEEInterface9GetThreadEv",
    "_ZdlPvj",
    "__aeabi_memclr4",
    "_impure_ptr",
    "abort",
    "fflush",
    "mgc_collect_count",
    "mgc_collect_init",
    "mgc_collect_total",
    "mgc_heap_alloc",
    "mgc_heap_collect",
    "mgc_heap_contains",
    "mgc_heap_init",
    "mgc_heap_range",
    "mgc_heap_total_alloc_bytes",
    "mgc_heap_used_bytes",
    "printf",
    "puts",
    "rhu_futex_wait",
    "rhu_futex_wake",
    "sched_yield",
    "strcmp",
    "strstr",
];

/// The mark stage's exact import closure: the twelve collection-era
/// flat EE methods, the allocator/skeleton surfaces it reads, the
/// shim mmap (bitmap + mark stack), the console runtime, and the two
/// libgcc-shaped 64-bit INTEGER division helpers the census ratio
/// arithmetic lowers to (satisfied by the staticlib's
/// compiler-builtins; the FLOAT `__aeabi_f/d` families stay
/// forbidden).
const IMPORTS_COLLECT: [&str; 29] = [
    "_ZN15GCToEEInterface11DiagGCStartEib",
    "_ZN15GCToEEInterface11GcScanRootsEPFvPP6ObjectP11ScanContextjEiiS4_",
    "_ZN15GCToEEInterface11GcStartWorkEii",
    "_ZN15GCToEEInterface16AfterGcScanRootsEiiP11ScanContext",
    "_ZN15GCToEEInterface17BeforeGcScanRootsEibb",
    "_ZN15GCToEEInterface18EnablePreemptiveGCEv",
    "_ZN15GCToEEInterface19DisablePreemptiveGCEv",
    "_ZN15GCToEEInterface19GcEnumAllocContextsEPFvP16gc_alloc_contextPvES2_",
    "_ZN15GCToEEInterface6GcDoneEi",
    "_ZN15GCToEEInterface9DiagGCEndEjiib",
    "_ZN15GCToEEInterface9RestartEEEb",
    "_ZN15GCToEEInterface9SuspendEEE14SUSPEND_REASON",
    "__aeabi_memclr",
    "__aeabi_memclr4",
    "__aeabi_uldivmod",
    "_impure_ptr",
    "abort",
    "fflush",
    "mgc_heap_census",
    "mgc_heap_counts",
    "mgc_heap_finq_snapshot",
    "mgc_heap_range",
    "mgc_heap_used_bytes",
    "mgc_skel_frozen_get",
    "mgc_skel_handles_foreach",
    "mmap64",
    "printf",
    "puts",
    "sched_yield",
];

/// The collector under MGC_BREAK_ROOTS: the root phases are compiled
/// out, so GcScanRoots and the two skeleton root surfaces
/// (handles_foreach / frozen_get) and the finq snapshot leave the
/// closure with them -- the suppression is visible in the OBJECT, not
/// just the transcript. (The source's `(void)&` anchors do not survive
/// -O2; the drop is the honest shape.)
const IMPORTS_COLLECT_BREAK: [&str; 25] = [
    "_ZN15GCToEEInterface11DiagGCStartEib",
    "_ZN15GCToEEInterface11GcStartWorkEii",
    "_ZN15GCToEEInterface16AfterGcScanRootsEiiP11ScanContext",
    "_ZN15GCToEEInterface17BeforeGcScanRootsEibb",
    "_ZN15GCToEEInterface18EnablePreemptiveGCEv",
    "_ZN15GCToEEInterface19DisablePreemptiveGCEv",
    "_ZN15GCToEEInterface19GcEnumAllocContextsEPFvP16gc_alloc_contextPvES2_",
    "_ZN15GCToEEInterface6GcDoneEi",
    "_ZN15GCToEEInterface9DiagGCEndEjiib",
    "_ZN15GCToEEInterface9RestartEEEb",
    "_ZN15GCToEEInterface9SuspendEEE14SUSPEND_REASON",
    "__aeabi_memclr",
    "__aeabi_memclr4",
    "__aeabi_uldivmod",
    "_impure_ptr",
    "abort",
    "fflush",
    "mgc_heap_census",
    "mgc_heap_counts",
    "mgc_heap_range",
    "mgc_heap_used_bytes",
    "mmap64",
    "printf",
    "puts",
    "sched_yield",
];

/// The collect-world host object's exact import closure: the gc.B
/// drive five plus the six kernel verbs of the suspension world
/// (spawn/suspend/resume/stack-bounds from the thread family, yield
/// behind sched_yield, sleep for the deterministic windows).
const IMPORTS_HOST_COLLECT: [&str; 12] = [
    "__aeabi_memset",
    "__mgc_dlopen_hook",
    "__mgc_dlsym_hook",
    "rhu_arena_alloc_at",
    "rhu_arena_headroom",
    "rhu_sleep_ns",
    "rhu_thread_resume",
    "rhu_thread_spawn",
    "rhu_thread_stack_bounds",
    "rhu_thread_suspend",
    "rhu_thread_yield",
    "sh_write0",
];

/// The collection boot's call inventory: the activation set plus the
/// drive's BCL-surface queries, the handle/frozen creation calls, and
/// the collection entries. The RegisterFrozenSegment head carries the
/// skeleton's slot index exactly as printed. IGCHeap::Alloc still
/// never logs a call line.
const INVENTORY_COLLECT: [&str; 13] = [
    "IGCHandleManager::GetGlobalHandleStore",
    "IGCHandleManager::Initialize",
    "IGCHandleStore::CreateDependentHandle",
    "IGCHandleStore::CreateHandleOfType",
    "IGCHeap::CollectionCount",
    "IGCHeap::GarbageCollect",
    "IGCHeap::GetGcCount",
    "IGCHeap::GetTotalAllocatedBytes",
    "IGCHeap::GetTotalBytesInUse",
    "IGCHeap::Initialize",
    "IGCHeap::IsHeapPointer",
    "IGCHeap::RegisterFrozenSegment[0]",
    "IGCHeap::WhichGeneration",
];

/// The positive gc.C census lines, pinned exact per collection (the
/// derivation is the gc.C registration's; the same numbers must
/// repeat at collection #2 -- the bitmap-reset proof). The break-roots
/// control proves these very pins ABSENT (the detector bites).
const CENSUS_ROOTS_LINE: &str = "[mgc-coll] roots: stacks=2 handles=1 (weak_skipped=1) dep_seen=1 dep_marked=1 finq=1 frozen_objs=2 frozen_edges=1";
const CENSUS_QUALS_LINE: &str = "[mgc-coll] quals: interior=1 (unresolved=0) pinned=1 nulls=1 out_of_heap=1 re_marks=3 ctx_seen=2 ctx_nonzero=0";
const CENSUS_MARK_1: &str = "[mgc-coll] mark census #1: roots=6 marked=7 marked_bytes=128 used_bytes=144 live_ratio~88% stack_hw=2 of 8192";
const CENSUS_MARK_2: &str = "[mgc-coll] mark census #2: roots=6 marked=7 marked_bytes=128 used_bytes=144 live_ratio~88% stack_hw=2 of 8192";

/// The sweep-stage skeleton: the collect set plus the four
/// f-reachable surfaces the finalization-era methods delegate to.
const IMPORTS_SKEL_SWEEP: [&str; 31] = [
    "_ZN15GCToEEInterface15GetAllocContextEv",
    "_ZN15GCToEEInterface21GetBooleanConfigValueEPKcS1_Pb",
    "_ZN15GCToEEInterface24GetFreeObjectMethodTableEv",
    "_ZN15GCToEEInterface25GetCurrentProcessCpuCountEv",
    "_ZN15GCToEEInterface9GetThreadEv",
    "_ZdlPvj",
    "__aeabi_memclr4",
    "_impure_ptr",
    "abort",
    "fflush",
    "mgc_collect_count",
    "mgc_collect_init",
    "mgc_collect_total",
    "mgc_finq_append",
    "mgc_freach_count",
    "mgc_freach_pop",
    "mgc_heap_alloc",
    "mgc_heap_collect",
    "mgc_heap_contains",
    "mgc_heap_init",
    "mgc_heap_live_bytes",
    "mgc_heap_range",
    "mgc_heap_total_alloc_bytes",
    "mgc_heap_used_bytes",
    "printf",
    "puts",
    "rhu_futex_wait",
    "rhu_futex_wake",
    "sched_yield",
    "strcmp",
    "strstr",
];

/// The windows-world skeleton: `sched_yield` leaves the import set
/// (WaitUntilGCComplete sleeps on the kernel futex), and the futex
/// pair plus the window-retire entry join. The first image-tier
/// consumption of rhu_futex_wait/rhu_futex_wake from src/gc.
const IMPORTS_SKEL_TUNE: [&str; 31] = [
    "_ZN15GCToEEInterface15GetAllocContextEv",
    "_ZN15GCToEEInterface21GetBooleanConfigValueEPKcS1_Pb",
    "_ZN15GCToEEInterface24GetFreeObjectMethodTableEv",
    "_ZN15GCToEEInterface25GetCurrentProcessCpuCountEv",
    "_ZN15GCToEEInterface9GetThreadEv",
    "_ZdlPvj",
    "__aeabi_memclr4",
    "_impure_ptr",
    "abort",
    "fflush",
    "mgc_collect_count",
    "mgc_collect_init",
    "mgc_collect_total",
    "mgc_finq_append",
    "mgc_freach_count",
    "mgc_freach_pop",
    "mgc_heap_alloc",
    "mgc_heap_collect",
    "mgc_heap_contains",
    "mgc_heap_init",
    "mgc_heap_live_bytes",
    "mgc_heap_range",
    "mgc_heap_total_alloc_bytes",
    "mgc_heap_used_bytes",
    "mgc_heap_window_fix",
    "printf",
    "puts",
    "rhu_futex_wait",
    "rhu_futex_wake",
    "strcmp",
    "strstr",
];

/// The sweep-stage allocator: the wall's collect-and-retry pulls in
/// the collector entry, and the alloc lock's preemptive spin brings
/// the EE mode switches plus yield. The tune-stage object is
/// IDENTICAL (the window machinery imports nothing).
const IMPORTS_HEAP_SWEEP: [&str; 16] = [
    "_ZN15GCToEEInterface15GetAllocContextEv",
    "_ZN15GCToEEInterface17StompWriteBarrierEP22WriteBarrierParameters",
    "_ZN15GCToEEInterface18EnablePreemptiveGCEv",
    "_ZN15GCToEEInterface19DisablePreemptiveGCEv",
    "_ZN15GCToEEInterface24GetFreeObjectMethodTableEv",
    "_ZN15GCToEEInterface9GetThreadEv",
    "__aeabi_memclr",
    "__aeabi_memclr4",
    "_impure_ptr",
    "abort",
    "fflush",
    "mgc_heap_collect",
    "mgc_skel_census",
    "mmap64",
    "printf",
    "sched_yield",
];

/// The BREAK_WINDOW_FIX heap object: identical but for `puts` -- the
/// compiler folds the control's no-argument banner printf into puts.
const IMPORTS_HEAP_TUNE_BREAK: [&str; 17] = [
    "_ZN15GCToEEInterface15GetAllocContextEv",
    "_ZN15GCToEEInterface17StompWriteBarrierEP22WriteBarrierParameters",
    "_ZN15GCToEEInterface18EnablePreemptiveGCEv",
    "_ZN15GCToEEInterface19DisablePreemptiveGCEv",
    "_ZN15GCToEEInterface24GetFreeObjectMethodTableEv",
    "_ZN15GCToEEInterface9GetThreadEv",
    "__aeabi_memclr",
    "__aeabi_memclr4",
    "_impure_ptr",
    "abort",
    "fflush",
    "mgc_heap_collect",
    "mgc_skel_census",
    "mmap64",
    "printf",
    "puts",
    "sched_yield",
];

/// The death-pipeline collector: the finq-snapshot blanket leaves with
/// the mark-only branch; the finq/free-list surfaces and the four
/// death-era EE methods join. The BREAK_WEAK control compiles to the
/// SAME closure -- the suppression is behavior-visible, not
/// closure-visible -- and the gate asserts that equality deliberately
/// (this control's honesty is in the transcript, unlike BREAK_ROOTS
/// whose suppression drops whole root surfaces from the object).
const IMPORTS_COLLECT_SWEEP: [&str; 41] = [
    "_ZN15GCToEEInterface11DiagGCStartEib",
    "_ZN15GCToEEInterface11GcScanRootsEPFvPP6ObjectP11ScanContextjEiiS4_",
    "_ZN15GCToEEInterface11GcStartWorkEii",
    "_ZN15GCToEEInterface14EagerFinalizedEP6Object",
    "_ZN15GCToEEInterface16AfterGcScanRootsEiiP11ScanContext",
    "_ZN15GCToEEInterface17BeforeGcScanRootsEibb",
    "_ZN15GCToEEInterface18EnableFinalizationEb",
    "_ZN15GCToEEInterface18EnablePreemptiveGCEv",
    "_ZN15GCToEEInterface19DisablePreemptiveGCEv",
    "_ZN15GCToEEInterface19GcEnumAllocContextsEPFvP16gc_alloc_contextPvES2_",
    "_ZN15GCToEEInterface25DiagWalkFReachableObjectsEPv",
    "_ZN15GCToEEInterface25SyncBlockCacheWeakPtrScanEPFvPP6ObjectPjjjEjj",
    "_ZN15GCToEEInterface6GcDoneEi",
    "_ZN15GCToEEInterface9DiagGCEndEjiib",
    "_ZN15GCToEEInterface9RestartEEEb",
    "_ZN15GCToEEInterface9SuspendEEE14SUSPEND_REASON",
    "__aeabi_memclr",
    "__aeabi_memclr4",
    "__aeabi_uldivmod",
    "_impure_ptr",
    "abort",
    "fflush",
    "mgc_finq_at",
    "mgc_finq_bound",
    "mgc_finq_set_state",
    "mgc_freach_count",
    "mgc_heap_census",
    "mgc_heap_counts",
    "mgc_heap_free_add",
    "mgc_heap_free_bytes",
    "mgc_heap_free_done",
    "mgc_heap_free_reset",
    "mgc_heap_range",
    "mgc_heap_used_bytes",
    "mgc_heap_walk_expected",
    "mgc_skel_frozen_get",
    "mgc_skel_handles_foreach",
    "mmap64",
    "printf",
    "puts",
    "sched_yield",
];

/// The tune-stage collector: the enum-time window retire joins.
const IMPORTS_COLLECT_TUNE: [&str; 42] = [
    "_ZN15GCToEEInterface11DiagGCStartEib",
    "_ZN15GCToEEInterface11GcScanRootsEPFvPP6ObjectP11ScanContextjEiiS4_",
    "_ZN15GCToEEInterface11GcStartWorkEii",
    "_ZN15GCToEEInterface14EagerFinalizedEP6Object",
    "_ZN15GCToEEInterface16AfterGcScanRootsEiiP11ScanContext",
    "_ZN15GCToEEInterface17BeforeGcScanRootsEibb",
    "_ZN15GCToEEInterface18EnableFinalizationEb",
    "_ZN15GCToEEInterface18EnablePreemptiveGCEv",
    "_ZN15GCToEEInterface19DisablePreemptiveGCEv",
    "_ZN15GCToEEInterface19GcEnumAllocContextsEPFvP16gc_alloc_contextPvES2_",
    "_ZN15GCToEEInterface25DiagWalkFReachableObjectsEPv",
    "_ZN15GCToEEInterface25SyncBlockCacheWeakPtrScanEPFvPP6ObjectPjjjEjj",
    "_ZN15GCToEEInterface6GcDoneEi",
    "_ZN15GCToEEInterface9DiagGCEndEjiib",
    "_ZN15GCToEEInterface9RestartEEEb",
    "_ZN15GCToEEInterface9SuspendEEE14SUSPEND_REASON",
    "__aeabi_memclr",
    "__aeabi_memclr4",
    "__aeabi_uldivmod",
    "_impure_ptr",
    "abort",
    "fflush",
    "mgc_finq_at",
    "mgc_finq_bound",
    "mgc_finq_set_state",
    "mgc_freach_count",
    "mgc_heap_census",
    "mgc_heap_counts",
    "mgc_heap_free_add",
    "mgc_heap_free_bytes",
    "mgc_heap_free_done",
    "mgc_heap_free_reset",
    "mgc_heap_range",
    "mgc_heap_used_bytes",
    "mgc_heap_walk_expected",
    "mgc_heap_window_fix",
    "mgc_skel_frozen_get",
    "mgc_skel_handles_foreach",
    "mmap64",
    "printf",
    "puts",
    "sched_yield",
];

/// The sweep host consumes ZERO new kernel verbs -- its closure is the
/// collect host's 11 names verbatim (IMPORTS_HOST_COLLECT is reused,
/// and that reuse IS the registered "no new kernel verbs" fact). The
/// tune host is the first image-tier C++ consumer of the futex AND
/// activation pairs.
const IMPORTS_HOST_TUNE: [&str; 16] = [
    "__aeabi_memset",
    "__mgc_dlopen_hook",
    "__mgc_dlsym_hook",
    "rhu_activation_register",
    "rhu_activation_send",
    "rhu_arena_alloc_at",
    "rhu_arena_headroom",
    "rhu_futex_wait",
    "rhu_futex_wake",
    "rhu_sleep_ns",
    "rhu_thread_resume",
    "rhu_thread_spawn",
    "rhu_thread_stack_bounds",
    "rhu_thread_suspend",
    "rhu_thread_yield",
    "sh_write0",
];

/// The sweep boot's call inventory: the collect set plus the
/// finalization-era heads (the eager branch's handle destroy, the
/// drain pair, the suppression verb).
/// The free-pad drive's pinned lines (gc.free-pad): the banner, the
/// layout, the live header word read after each sweep, the 12-byte reuse,
/// the verdict.
const FP_BANNER: &str = "[gc-host] gc.free-pad drive: a dead 12-byte object before a live object whose header word is in use, then two sweeps";
const FP_LAYOUT: &str = "[gc-host] free-pad: layout #1 dead 12 B | #2 live 16 B (header word set to 0x08abcdef) | #3 dead 24 B | #4 live 16 B";
const FP_INTACT_1: &str = "[gc-host] free-pad: the live object's header word after sweep #1 reads 0x08abcdef (INTACT)";
const FP_REUSE: &str = "[gc-host] free-pad: a 12-byte ask reused the 24-byte run at alloc #3's address (the run consumed whole, its 12-byte remainder a pad)";
const FP_INTACT_2: &str = "[gc-host] free-pad: the live object's header word after sweep #2 reads 0x08abcdef (INTACT)";
const FP_DONE: &str = "[gc-host] GC FREE-PAD EXERCISE OK (a 12-byte dead space before a live object left its header word intact across two sweeps; the pads walked and counted; a 12-byte first-fit remainder left as a pad)";
/// The free-pad boot's call inventory: the init-time calls and the two
/// collections; the drive reaches the heap through no other vtable method.
const INVENTORY_FREEPAD: [&str; 4] = [
    "IGCHandleManager::GetGlobalHandleStore",
    "IGCHandleManager::Initialize",
    "IGCHeap::GarbageCollect",
    "IGCHeap::Initialize",
];

/// The handle-pool drive's pinned lines (gc.handle-pool): the banner, the
/// counter worker's and the handle workers' spawn lines, the workers' books
/// and wall time, the pool's census after the storm, the verdict; and the
/// control's knob banner and exhaustion fail-fast. The case runs on the
/// deterministic profile, so every one of these is exact, the counts
/// included.
const HP_BANNER: &str = "[gc-host] gc.handle-pool drive: two hosted threads at one priority create and destroy handles at once through the handle manager's own entries (100000 iterations of 8 handles each)";
const HP_COUNTER_SPAWN: &str = "[gc-host] worker spawned: tid=3 prio=0xE0 stack=4096 (runs only while kmain sleeps)";
const HP_SPAWN: &str = "[gc-host] handle workers spawned: tid=4 and tid=5 prio=0xC0 stack=4096 (rotation at one level above the counter; the host parks until both finish)";
const HP_WORKER_0: &str = "[gc-host] handle worker 0 (tid 4): 800000 creates, 800000 destroys, 0 aliases (every handle read its own object, twice)";
const HP_WORKER_1: &str = "[gc-host] handle worker 1 (tid 5): 800000 creates, 800000 destroys, 0 aliases (every handle read its own object, twice)";
const HP_RAN_0: &str = "[gc-host] handle worker 0 ran 340 ms";
const HP_RAN_1: &str = "[gc-host] handle worker 1 ran 340 ms";
const HP_CENSUS: &str = "[mgc-skel] handles: created=1600200 live=0 of 64 (reused=1600184 live_hw=16) (types: ws=0 wl=0 strong=16 pinned=0 var=0 rc=0 dep=0 ap=0 sr=0 com=0 wip=0 xref=0)";
const HP_DONE: &str = "[gc-host] GC HANDLE POOL EXERCISE OK (two threads, 3200000 pool operations at once, no slot aliased, the workers' books exact; the census above carries the pool's)";
const HP_KNOB: &str = "[mgc-skel] BREAK_HANDLE_LOCK: the pool lock is compiled out (the negative control's knob)";
const HP_EXHAUSTED: &str = "[mgc-skel] FAIL-FAST: handle slot pool exhausted (raise MGC_HANDLE_SLOTS)";
/// The handle-pool boot's call inventory: the init-time calls, the two
/// handle entries the workers drive, and the census collection with its
/// event pair.
const INVENTORY_HANDLES: [&str; 8] = [
    "IGCHandleManager::CreateGlobalHandleOfType",
    "IGCHandleManager::DestroyHandleOfType",
    "IGCHandleManager::GetGlobalHandleStore",
    "IGCHandleManager::Initialize",
    "IGCHeap::GarbageCollect",
    "IGCHeap::Initialize",
    "IGCHeap::ResetWaitForGCEvent",
    "IGCHeap::SetWaitForGCEvent",
];
/// The control dies inside the storm: the heads seen before the pool's
/// exhaustion (no census collection follows).
const INVENTORY_HANDLES_BREAK: [&str; 5] = [
    "IGCHandleManager::CreateGlobalHandleOfType",
    "IGCHandleManager::DestroyHandleOfType",
    "IGCHandleManager::GetGlobalHandleStore",
    "IGCHandleManager::Initialize",
    "IGCHeap::Initialize",
];

/// The no-GC drive host's EXACT import closure: the windows host's verbs
/// less the worker's (no thread is spawned, no futex waited on, no
/// activation sent) -- the arena, the sleep, the resume and yield the
/// activation host keeps, the two hooks, memset and the console.
const IMPORTS_HOST_NOGC: [&str; 9] = [
    "__aeabi_memset",
    "__mgc_dlopen_hook",
    "__mgc_dlsym_hook",
    "rhu_arena_alloc_at",
    "rhu_arena_headroom",
    "rhu_sleep_ns",
    "rhu_thread_resume",
    "rhu_thread_yield",
    "sh_write0",
];
/// The no-GC region drive's pinned lines (gc.no-gc-region): the banner,
/// the three entries refused in size order, the end's answer, the
/// verdict; the control's knob line, its claimed entry and the drive's
/// loud check.
const NG_BANNER: &str = "[gc-host] gc.no-gc-region drive: the no-GC region's entry and end through the interface, the statuses read against the pinned enums (a byte, a kilobyte, the heap)";
const NG_START_1: &str = "[mgc-skel] call IGCHeap::StartNoGCRegion(size=1) -> no_memory (no region is provided; refused, nothing entered)";
const NG_START_1024: &str = "[mgc-skel] call IGCHeap::StartNoGCRegion(size=1024) -> no_memory (no region is provided; refused, nothing entered)";
const NG_START_HEAP: &str = "[mgc-skel] call IGCHeap::StartNoGCRegion(size=2097152) -> no_memory (no region is provided; refused, nothing entered)";
const NG_END: &str = "[mgc-skel] call IGCHeap::EndNoGCRegion -> not_in_progress (no region is ever entered)";
const NG_DONE: &str = "[gc-host] GC NO-GC REGION EXERCISE OK (three entries refused with no_memory -- a byte, a kilobyte, the heap -- each end and a bare end answering not_in_progress; nothing claimed)";
const NG_KNOB: &str = "[mgc-skel] BREAK_NOGC_CLAIM: the entry claims success with nothing entered (the negative control's knob)";
const NG_CLAIMED: &str = "[mgc-skel] call IGCHeap::StartNoGCRegion(size=1) -> success (claimed; nothing entered)";
const NG_CHECK_FAILED: &str = "[gc-host] ALLOC DRIVE CHECK FAILED: StartNoGCRegion claimed success with nothing entered";
/// The no-GC boot's call inventory: the init-time calls and the two region
/// entries; the drive reaches the heap through no other vtable method.
const INVENTORY_NOGC: [&str; 5] = [
    "IGCHandleManager::GetGlobalHandleStore",
    "IGCHandleManager::Initialize",
    "IGCHeap::EndNoGCRegion",
    "IGCHeap::Initialize",
    "IGCHeap::StartNoGCRegion",
];
/// The control dies at the first entry's check: no end is ever reached.
const INVENTORY_NOGC_BREAK: [&str; 4] = [
    "IGCHandleManager::GetGlobalHandleStore",
    "IGCHandleManager::Initialize",
    "IGCHeap::Initialize",
    "IGCHeap::StartNoGCRegion",
];

const INVENTORY_SWEEP: [&str; 17] = [
    "IGCHandleManager::DestroyHandleOfType",
    "IGCHandleManager::GetGlobalHandleStore",
    "IGCHandleManager::Initialize",
    "IGCHandleStore::CreateDependentHandle",
    "IGCHandleStore::CreateHandleOfType",
    "IGCHeap::CollectionCount",
    "IGCHeap::GarbageCollect",
    "IGCHeap::GetGcCount",
    "IGCHeap::GetNextFinalizable",
    "IGCHeap::GetNumberOfFinalizable",
    "IGCHeap::GetTotalAllocatedBytes",
    "IGCHeap::GetTotalBytesInUse",
    "IGCHeap::Initialize",
    "IGCHeap::IsHeapPointer",
    "IGCHeap::RegisterFrozenSegment[0]",
    "IGCHeap::SetFinalizationRun",
    "IGCHeap::WhichGeneration",
];

/// The tune boot adds the event pair, the trapped-mutator wait, and
/// the detach fix.
const INVENTORY_TUNE: [&str; 21] = [
    "IGCHandleManager::DestroyHandleOfType",
    "IGCHandleManager::GetGlobalHandleStore",
    "IGCHandleManager::Initialize",
    "IGCHandleStore::CreateDependentHandle",
    "IGCHandleStore::CreateHandleOfType",
    "IGCHeap::CollectionCount",
    "IGCHeap::FixAllocContext",
    "IGCHeap::GarbageCollect",
    "IGCHeap::GetGcCount",
    "IGCHeap::GetNextFinalizable",
    "IGCHeap::GetNumberOfFinalizable",
    "IGCHeap::GetTotalAllocatedBytes",
    "IGCHeap::GetTotalBytesInUse",
    "IGCHeap::Initialize",
    "IGCHeap::IsHeapPointer",
    "IGCHeap::RegisterFrozenSegment[0]",
    "IGCHeap::ResetWaitForGCEvent",
    "IGCHeap::SetFinalizationRun",
    "IGCHeap::SetWaitForGCEvent",
    "IGCHeap::WaitUntilGCComplete",
    "IGCHeap::WhichGeneration",
];

/// The tune CONTROL dies at collection #1's heap walk: the heads seen
/// before the death (no CollectionCount/GetGcCount, no drain, no
/// eager destroy -- those all come after the walk).
const INVENTORY_TUNE_BREAK: [&str; 14] = [
    "IGCHandleManager::GetGlobalHandleStore",
    "IGCHandleManager::Initialize",
    "IGCHandleStore::CreateDependentHandle",
    "IGCHandleStore::CreateHandleOfType",
    "IGCHeap::GarbageCollect",
    "IGCHeap::GetTotalAllocatedBytes",
    "IGCHeap::GetTotalBytesInUse",
    "IGCHeap::Initialize",
    "IGCHeap::IsHeapPointer",
    "IGCHeap::RegisterFrozenSegment[0]",
    "IGCHeap::ResetWaitForGCEvent",
    "IGCHeap::SetFinalizationRun",
    "IGCHeap::WaitUntilGCComplete",
    "IGCHeap::WhichGeneration",
];

/// gc.C-sweep census pins, derived on paper from the schedule + the
/// root graph + the collector's own arithmetic (throttle indices
/// included: census lines print for #1-6 and every 8th; full logs
/// for #1-4 and every 16th).
const SW_ROOTS_1: &str = "[mgc-coll] roots: stacks=3 handles=1 (weak_skipped=4) dep_seen=2 dep_marked=1 finq=3 frozen_objs=2 frozen_edges=1";
const SW_QUALS_1: &str = "[mgc-coll] quals: interior=1 (unresolved=0) pinned=1 nulls=2 out_of_heap=1 re_marks=3 ctx_seen=2 ctx_nonzero=0";
const SW_MARK_1: &str = "[mgc-coll] mark census #1: roots=9 marked=10 marked_bytes=176 used_bytes=320 live_ratio~55% stack_hw=3 of 8192";
const SW_DEATH_1: &str = "[mgc-coll] death #1: ws_null=1 wl_null=1 dep_clear=1 odd=0 | finq kept=1 eager=1 suppressed=1 promoted=2(+1 crit) freach_pending=3";
const SW_SWEEP_1: &str = "[mgc-coll] sweep #1: live=10 objs 0 KB | reclaimed 8 objs -> 4 runs 0 KB free";
const SW_ROOTS_2: &str = "[mgc-coll] roots: stacks=3 handles=1 (weak_skipped=3) dep_seen=2 dep_marked=1 finq=0 frozen_objs=2 frozen_edges=1";
const SW_MARK_2: &str = "[mgc-coll] mark census #2: roots=6 marked=7 marked_bytes=132 used_bytes=320 live_ratio~41% stack_hw=3 of 8192";
const SW_DEATH_2: &str = "[mgc-coll] death #2: ws_null=0 wl_null=1 dep_clear=0 odd=0 | finq kept=1 eager=0 suppressed=0 promoted=0(+0 crit) freach_pending=0";
const SW_SWEEP_2: &str = "[mgc-coll] sweep #2: live=7 objs 0 KB | reclaimed 7 objs -> 3 runs 0 KB free";
const SW_MARK_3: &str = "[mgc-coll] mark census #3: roots=7 marked=8 marked_bytes=1900132 used_bytes=1900320 live_ratio~99% stack_hw=4 of 8192";
const SW_SWEEP_3: &str = "[mgc-coll] sweep #3: live=8 objs 1855 KB | reclaimed 4 objs -> 3 runs 0 KB free";
const SW_MARK_4: &str = "[mgc-coll] mark census #4: roots=6 marked=7 marked_bytes=132 used_bytes=1900320 live_ratio~0% stack_hw=3 of 8192";
const SW_SWEEP_4: &str = "[mgc-coll] sweep #4: live=7 objs 0 KB | reclaimed 4 objs -> 4 runs 1855 KB free";
const SW_MARK_5: &str = "[mgc-coll] mark census #5: roots=6 marked=7 marked_bytes=132 used_bytes=1900320 live_ratio~0% stack_hw=3 of 8192";
const SW_DEATH_1_BREAK: &str = "[mgc-coll] death #1: ws_null=0 wl_null=0 dep_clear=0 odd=0 | finq kept=1 eager=1 suppressed=1 promoted=2(+1 crit) freach_pending=3";
const SW_DEATH_2_BREAK: &str = "[mgc-coll] death #2: ws_null=0 wl_null=0 dep_clear=0 odd=0 | finq kept=1 eager=0 suppressed=0 promoted=0(+0 crit) freach_pending=0";

/// gc.D census pins. The death line at collection #1 is BYTE-EQUAL to
/// the sweep world's (SW_DEATH_1 is reused) -- the death pipeline is
/// posture-invariant, and that cross-pin is deliberate.
const TN_QUALS_1: &str = "[mgc-coll] quals: interior=1 (unresolved=0) pinned=1 nulls=2 out_of_heap=1 re_marks=3 ctx_seen=3 ctx_nonzero=1";
const TN_MARK_1: &str = "[mgc-coll] mark census #1: roots=9 marked=10 marked_bytes=176 used_bytes=16652 live_ratio~1% stack_hw=3 of 8192";
const TN_SWEEP_1: &str = "[mgc-coll] sweep #1: live=10 objs 0 KB | reclaimed 13 objs -> 6 runs 16 KB free";
const TN_MARK_2: &str = "[mgc-coll] mark census #2: roots=6 marked=7 marked_bytes=132 used_bytes=16652 live_ratio~0% stack_hw=3 of 8192";
const TN_SWEEP_2: &str = "[mgc-coll] sweep #2: live=7 objs 0 KB | reclaimed 9 objs -> 5 runs 16 KB free";
const TN_MARK_3: &str = "[mgc-coll] mark census #3: roots=6 marked=7 marked_bytes=132 used_bytes=16652 live_ratio~0% stack_hw=3 of 8192";
const TN_DEATH_3: &str = "[mgc-coll] death #3: ws_null=0 wl_null=0 dep_clear=0 odd=0 | finq kept=1 eager=0 suppressed=0 promoted=0(+0 crit) freach_pending=0";
const TN_SWEEP_3: &str = "[mgc-coll] sweep #3: live=7 objs 0 KB | reclaimed 9 objs -> 5 runs 16 KB free";
const TN_BOOKS: &str = "[gc-host] posture books: alloc_count=11 alloc_bytes=304; in_use=12528 allocated=304 (window charges minus refunds = the fast-born bytes)";
const TN_BOOKS_BREAK: &str = "[gc-host] posture books: alloc_count=11 alloc_bytes=12476; in_use=12528 allocated=12476 (window charges minus refunds = the fast-born bytes)";
const TN_WINDOWS_1: &str = "[mgc-heap] windows: handed=4 (from_runs=0) fixed=3 plugs=3 objs=6 consumed=0 KB refunded=11 KB outstanding_hw=1";
const TN_HANDLES_1: &str = "[mgc-skel] handles: created=207 live=7 of 64 (reused=199 live_hw=8) (types: ws=2 wl=2 strong=2 pinned=0 var=0 rc=0 dep=2 ap=0 sr=0 com=0 wip=0 xref=0)";
const TN_SELFTEST: &str = "[mgc-skel] handle reuse self-test: 200 create/destroy cycles in 8 fresh slots (reused=192 live back to 0) OK";

/// The region size the gate compiles into the allocator boot (a
/// gate-image knob on the QEMU an500 payload map, comfortably inside
/// the arena window; per-board knob rows are the gc.E deliverable).
const GATE_HEAP_BYTES: u32 = 2 * 1024 * 1024;

/// gc.E row pins. The row world is the tune world under the DECLARED
/// an500 row (src/gc/host/mgc_board_rows.h): every address-free tune
/// pin carries verbatim (TUNE_DRIVE_PINS below is shared by both
/// cases) and the row adds its census, the containment verdict, and
/// the window-fit verdict. drawn=2195456 is the three arena asks'
/// exact sum (region 2M + mark bitmap 64K + mark stack 32K); the
/// cover-line buffer totals are cover/1024 + 64 (4160 for the row's
/// 4M, 2112 for the control's doctored 2M).
const ROW_CENSUS: &str = "[gc-row] board=mps2-an500 profile=suite basis=link region=2097152 cover=4194304 kheap=65536 stack=16384/24576 handles=64 foh=65536";
const ROW_SIZING: &str = "[gc-row] sizing: markstack=8192 finq=2048 reserve=65536 window=4096/256 retries=4 frozen=8";
const ROW_CONTAIN_PREFIX: &str = "[gc-row] containment: floor=0x20000000 cover_end=0x20400000 statics=[";
const ROW_CONTAIN_VERDICT: &str = "[gc-row] containment VERDICT: INSIDE (statics + arena window inside the cover granule -- every legal bulk-move destination covered)";
const ROW_FIT_PREFIX: &str = "[gc-row] window fit: drawn=2195456 kheap=65536 window=";
const ROW_FIT_VERDICT: &str = "[gc-row] window VERDICT: FITS (row region + aux + kheap inside the link window)";
const ROW_REFUSAL_TEXT: &str = "evidence postures never ride a row image";
const ROW_COVER_FAILFAST: &str = "[mgc-heap] FAIL-FAST: card cover too small for region placement (raise MGC_ARENA_COVER_BYTES)";

/// The tune-world drive pins (address-free), shared verbatim by gc.D
/// and gc.E: the row changes bookkeeping geometry, never behavior, so
/// one list is the single truth for both worlds' wire.
const TUNE_DRIVE_PINS: &[&str] = &[
    "[gc-host] GC ACTIVATION OK (heap + handle manager live; the allocator drive follows)",
    "[gc-host] gc.D windows+event drive: the same payload under window posture, the GC-done futex, and the activation break-out",
    TN_SELFTEST,
    "[gc-host] alloc #2 size=28 FAST-PATH from the window: zero-born OK, free-object birth stored",
    "[gc-host] alloc #2 adjacent to #1: the bump advanced exactly 24 bytes",
    " (4096 B) at alloc #1",
    " (4096 B) at alloc #5",
    " (4096 B) at alloc #10",
    " (4096 B) at alloc #12",
    "[gc-host] alloc #8 size=24 FAST-PATH from the window: zero-born OK, free-object birth stored",
    TN_BOOKS,
    "[gc-host] waiter spawned: tid=4 prio=0xC0 stack=4096 (parks on a go futex between epochs)",
    "[gc-host] epoch signal: woken 1 waiter (it was genuinely futex-blocked)",
    "[mgc-heap] allocs=12 bytes=4416 pads=1 null_returns=0 max_size=24 (flags=0x0 at alloc #1)",
    "[mgc-heap] finq: registered_total=6 live=6 freach=0(+0 crit) lost=0 (cap=2048, bound=6)",
    "[mgc-heap] free list: runs=0 free=0 KB; reuse: allocs=0 bytes=0 KB (exact=0 split=0); walk_expected=22",
    TN_WINDOWS_1,
    TN_HANDLES_1,
    "[mgc-skel] call IGCHeap::ResetWaitForGCEvent",
    "[gc-host] waiter: parking on the GC-done event (epoch 1)",
    "[mgc-skel] call IGCHeap::WaitUntilGCComplete -> 0",
    "[gc-host] EE SuspendEE(reason=1): worker tid=3 suspended; ctx.sp + published slot inside stack bounds (4096 B); counter FROZEN across a sleep window",
    "[gc-host] EE GcEnumAllocContexts: 3 contexts handed (kmain books, worker zeroed, detach probe; combined_limit EE-zeroed when limit==0)",
    "[mgc-coll] heap walk: objects=23 (model=23: live+pads+runs) MATCH",
    SW_ROOTS_1,
    TN_QUALS_1,
    TN_MARK_1,
    SW_DEATH_1,
    TN_SWEEP_1,
    "[mgc-skel] call IGCHeap::SetWaitForGCEvent",
    "[gc-host] waiter: woken (epoch 1)",
    "[gc-host] EE RestartEE: worker resumed; counter ADVANCED within the bounded window",
    "[mgc-coll] EnableFinalization(true): 2(+1 crit) promoted this collection",
    "[gc-host] weak probe ws(#13): slot NULL (dead=True)",
    "[gc-host] drain: finalizer ran for #11 (the critical class drains LAST)",
    SW_ROOTS_2,
    TN_MARK_2,
    SW_DEATH_2,
    TN_SWEEP_2,
    "[mgc-coll] heap walk: objects=16 (model=16: live+pads+runs) MATCH",
    "[gc-host] weak probe wl(#5) after #2: slot NULL (dead=True)",
    "[gc-host] slot reuse: the post-destroy create landed on the released slot (handle address identical)",
    "[gc-host] waiter: parking on the GC-done event (epoch 2)",
    "[gc-host] activation sent to the futex-blocked waiter (tid=4)",
    "[gc-host] activation handler ran (delivered on the target's own stack)",
    "[gc-host] waiter: woken (epoch 2)",
    "[gc-host] EINTR by elimination: no Set/wake between parking and return, the event was armed before parking (no EAGAIN), no deadline -- the wait returned through the RHU_EINTR contract after the handler ran",
    "[gc-host] alloc #19 served from a FREE-RUN window (adaptive size, zeroed at handout)",
    "[gc-host] alloc #20 size=24 FAST-PATH from the window: zero-born OK, free-object birth stored",
    "[mgc-skel] call IGCHeap::FixAllocContext(ctx=",
    "[gc-host] detach probe: FixAllocContext retired the context's window (zero births counted, remainder plugged)",
    TN_MARK_3,
    TN_DEATH_3,
    TN_SWEEP_3,
    "[mgc-coll] counters: gen0=3 gen1=3 gen2=3 total=3",
    "[gc-host] final books: kmain alloc_count=13 alloc_bytes=360, detach alloc_count=1; in_use=160 (132 live + 4 skew + 2 pads x 12) allocated=376 OK",
    "[gc-host] GC WINDOWS+EVENT EXERCISE OK (windows both sources, three fix sites, refunds exact; the GC-done event slept and woke over the kernel futex; an activation broke a blocked wait -- the RHU_EINTR contract; handle slots reused)",
    "[payload] managed Main returned 0",
];

/// The row host's exact import closure: the tune host's 15 names plus
/// the six map-defined layout symbols the containment and window-fit
/// verdicts consume (satisfied at link by the payload map, never by
/// code).
/// The handle-pool host object's exact import closure: the windows host's
/// verbs less the activation pair (the drive that used them is compiled out
/// under the handle-pool drive) plus the clock, the thread exit and the
/// 64-bit division the workers' wall-time line needs.
const IMPORTS_HOST_HANDLES: [&str; 17] = [
    "__aeabi_memset",
    "__aeabi_uldivmod",
    "__mgc_dlopen_hook",
    "__mgc_dlsym_hook",
    "rhu_arena_alloc_at",
    "rhu_arena_headroom",
    "rhu_futex_wait",
    "rhu_futex_wake",
    "rhu_now_ns",
    "rhu_sleep_ns",
    "rhu_thread_exit",
    "rhu_thread_resume",
    "rhu_thread_spawn",
    "rhu_thread_stack_bounds",
    "rhu_thread_suspend",
    "rhu_thread_yield",
    "sh_write0",
];

const IMPORTS_HOST_ROW: [&str; 22] = [
    "__aeabi_memset",
    "__ebss",
    "__edata",
    "__mgc_dlopen_hook",
    "__mgc_dlsym_hook",
    "__rhu_arena_hi",
    "__rhu_arena_lo",
    "__sbss",
    "__sdata",
    "rhu_activation_register",
    "rhu_activation_send",
    "rhu_arena_alloc_at",
    "rhu_arena_headroom",
    "rhu_futex_wait",
    "rhu_futex_wake",
    "rhu_sleep_ns",
    "rhu_thread_resume",
    "rhu_thread_spawn",
    "rhu_thread_stack_bounds",
    "rhu_thread_suspend",
    "rhu_thread_yield",
    "sh_write0",
];

/// Import-name families that may never appear in image-tier C++ objects:
/// C++ runtime/unwind machinery (the -fno-exceptions/-fno-rtti contract)
/// and soft-float helpers (the hard-float ABI contract).
const FORBIDDEN_IMPORT_FAMILIES: [&str; 5] = ["_Unwind", "__cxa", "_ZSt", "__aeabi_f", "__aeabi_d"];

/// Cases registered by the gate design, not yet armed. Reported as
/// pending on every run so the ladder's state is visible where the gate
/// runs -- a pending case contributes nothing to the verdict.
const PENDING: [&str; 1] = [
    "gc.C remainder: managed value-set battery + doctored-golden control (rt world)",
];

/// The compile-tier posture control: listed in every consumer run (the
/// pending-list idiom -- stated loudly, never counted), armed on the
/// maintainer's re-derivation leg, where the compiler is.
const INTERNAL_CASES: [&str; 2] = [
    "gc.E-posture-refused -- compile-tier; runs under 'rhu-gate gc rederive' with the compiler of record",
    "gc.sdk-rows-tie-their-row -- compile-tier; runs under 'rhu-gate gc rederive' with the compiler of record: every row of the SDK's board table names collector objects that tie their sources compiled under that row's own declaration apart from the producer stamp; the table with the U5A5 row's heap object put back to the emulator row's is refused",
];

/// goldens/gc/MANIFEST.md's own sha256: the in-code anchor beside the
/// manifest (the payload-gate idiom, one level up -- doctoring the
/// manifest and the objects together still cannot pass without editing
/// this reviewed surface). A re-freeze updates objects + manifest + this
/// pin together in one reviewed commit.
const SHA_FROZEN_MANIFEST: &str = "9516c07bbebeae8a00268c9c52b7e5ef0fc0b884da08d4331222a0ae8931c3d9";

/// The frozen-object plan of record: file name under goldens/gc/, source
/// TU, extra defines beyond the class's flag set, and the class (the
/// Cortex-M7 for every row but the mps2-an505 row's, the Cortex-M33 for
/// those). The manifest's records must tie this table by name both
/// directions; 'rhu-gate gc rederive' recompiles every row under its
/// class's flag set and byte-asserts the output against the frozen bytes.
/// Worlds stage these files where they once compiled them -- the bytes
/// are frozen inputs, produced by ONE recorded build, never rebuilt by
/// a consumer.
const FROZEN_PLAN: [(&str, &str, &[&str], Class); 39] = [
    ("skel-bare.o", "src/gc/mgc_skeleton.cpp", &[], Class::CortexM7),
    ("skel-bad.o", "src/gc/mgc_skeleton.cpp", &["-DMGC_SKEL_BAD_VERSION"], Class::CortexM7),
    ("host.o", "src/gc/host/mgc_host.cpp", &[], Class::CortexM7),
    ("probe.o", "src/gc/mgc_ee_probe.cpp", &[], Class::CortexM7),
    ("skel-heap.o", "src/gc/mgc_skeleton.cpp", &["-DMGC_HEAP_LIVE"], Class::CortexM7),
    ("heap.o", "src/gc/mgc_heap.cpp", &["-DMGC_HEAP_LIVE", "-DMGC_HEAP_BYTES=2097152u"], Class::CortexM7),
    ("host-drive.o", "src/gc/host/mgc_host.cpp", &["-DMGC_HOST_DRIVE_ALLOC"], Class::CortexM7),
    ("skel-collect.o", "src/gc/mgc_skeleton.cpp", &["-DMGC_HEAP_LIVE", "-DMGC_COLLECT_LIVE"], Class::CortexM7),
    ("heap-collect.o", "src/gc/mgc_heap.cpp", &["-DMGC_HEAP_LIVE", "-DMGC_COLLECT_LIVE", "-DMGC_HEAP_BYTES=2097152u"], Class::CortexM7),
    ("collect.o", "src/gc/mgc_collect.cpp", &["-DMGC_HEAP_LIVE", "-DMGC_COLLECT_LIVE"], Class::CortexM7),
    ("collect-break.o", "src/gc/mgc_collect.cpp", &["-DMGC_HEAP_LIVE", "-DMGC_COLLECT_LIVE", "-DMGC_BREAK_ROOTS"], Class::CortexM7),
    ("host-collect.o", "src/gc/host/mgc_host.cpp", &["-DMGC_HOST_DRIVE_ALLOC", "-DMGC_HOST_DRIVE_COLLECT"], Class::CortexM7),
    ("skel-sweep.o", "src/gc/mgc_skeleton.cpp", &["-DMGC_HEAP_LIVE", "-DMGC_COLLECT_LIVE", "-DMGC_SWEEP_LIVE"], Class::CortexM7),
    ("heap-sweep.o", "src/gc/mgc_heap.cpp", &["-DMGC_HEAP_LIVE", "-DMGC_COLLECT_LIVE", "-DMGC_SWEEP_LIVE", "-DMGC_HEAP_BYTES=2097152u"], Class::CortexM7),
    ("collect-sweep.o", "src/gc/mgc_collect.cpp", &["-DMGC_HEAP_LIVE", "-DMGC_COLLECT_LIVE", "-DMGC_SWEEP_LIVE"], Class::CortexM7),
    ("collect-sweep-break.o", "src/gc/mgc_collect.cpp", &["-DMGC_HEAP_LIVE", "-DMGC_COLLECT_LIVE", "-DMGC_SWEEP_LIVE", "-DMGC_BREAK_WEAK"], Class::CortexM7),
    ("host-sweep.o", "src/gc/host/mgc_host.cpp", &["-DMGC_HOST_DRIVE_ALLOC", "-DMGC_HOST_DRIVE_COLLECT", "-DMGC_HOST_DRIVE_SWEEP"], Class::CortexM7),
    ("skel-tune.o", "src/gc/mgc_skeleton.cpp", &["-DMGC_HEAP_LIVE", "-DMGC_COLLECT_LIVE", "-DMGC_SWEEP_LIVE", "-DMGC_TUNE_LIVE", "-DMGC_HANDLE_SLOTS=64"], Class::CortexM7),
    ("heap-tune.o", "src/gc/mgc_heap.cpp", &["-DMGC_HEAP_LIVE", "-DMGC_COLLECT_LIVE", "-DMGC_SWEEP_LIVE", "-DMGC_TUNE_LIVE", "-DMGC_HEAP_BYTES=2097152u"], Class::CortexM7),
    ("heap-tune-break.o", "src/gc/mgc_heap.cpp", &["-DMGC_HEAP_LIVE", "-DMGC_COLLECT_LIVE", "-DMGC_SWEEP_LIVE", "-DMGC_TUNE_LIVE", "-DMGC_HEAP_BYTES=2097152u", "-DMGC_BREAK_WINDOW_FIX"], Class::CortexM7),
    ("collect-tune.o", "src/gc/mgc_collect.cpp", &["-DMGC_HEAP_LIVE", "-DMGC_COLLECT_LIVE", "-DMGC_SWEEP_LIVE", "-DMGC_TUNE_LIVE"], Class::CortexM7),
    ("host-tune.o", "src/gc/host/mgc_host.cpp", &["-DMGC_HOST_DRIVE_ALLOC", "-DMGC_HOST_DRIVE_COLLECT", "-DMGC_HOST_DRIVE_SWEEP", "-DMGC_HOST_DRIVE_TUNE"], Class::CortexM7),
    ("skel-row.o", "src/gc/mgc_skeleton.cpp", &["-DMGC_HEAP_LIVE", "-DMGC_COLLECT_LIVE", "-DMGC_SWEEP_LIVE", "-DMGC_TUNE_LIVE", "-DMGC_HANDLE_SLOTS=64"], Class::CortexM7),
    ("heap-row.o", "src/gc/mgc_heap.cpp", &["-DMGC_HEAP_LIVE", "-DMGC_COLLECT_LIVE", "-DMGC_SWEEP_LIVE", "-DMGC_TUNE_LIVE", "-DMGC_HEAP_BYTES=2097152u", "-DMGC_ARENA_COVER_BYTES=4194304u"], Class::CortexM7),
    ("heap-row-break-cover.o", "src/gc/mgc_heap.cpp", &["-DMGC_HEAP_LIVE", "-DMGC_COLLECT_LIVE", "-DMGC_SWEEP_LIVE", "-DMGC_TUNE_LIVE", "-DMGC_HEAP_BYTES=2097152u", "-DMGC_ARENA_COVER_BYTES=2097152u"], Class::CortexM7),
    ("collect-row.o", "src/gc/mgc_collect.cpp", &["-DMGC_HEAP_LIVE", "-DMGC_COLLECT_LIVE", "-DMGC_SWEEP_LIVE", "-DMGC_TUNE_LIVE"], Class::CortexM7),
    ("host-row.o", "src/gc/host/mgc_host.cpp", &["-DMGC_HOST_DRIVE_ALLOC", "-DMGC_HOST_DRIVE_COLLECT", "-DMGC_HOST_DRIVE_SWEEP", "-DMGC_HOST_DRIVE_TUNE", "-DMGC_HOST_ROW_AN500"], Class::CortexM7),
    ("heap-row-f769.o", "src/gc/mgc_heap.cpp", &["-DMGC_HEAP_LIVE", "-DMGC_COLLECT_LIVE", "-DMGC_SWEEP_LIVE", "-DMGC_TUNE_LIVE", "-DMGC_HEAP_BYTES=98304u", "-DMGC_ARENA_COVER_BYTES=524288u", "-DMGC_FINQ_CAP=256u", "-DMGC_OOM_RESERVE=8192u", "-DMGC_WINDOW_BYTES=4096u", "-DMGC_WINDOW_FLOOR=256u", "-DMGC_WALL_RETRIES=4u"], Class::CortexM7),
    ("collect-row-f769.o", "src/gc/mgc_collect.cpp", &["-DMGC_HEAP_LIVE", "-DMGC_COLLECT_LIVE", "-DMGC_SWEEP_LIVE", "-DMGC_TUNE_LIVE", "-DMGC_MARK_STACK_ENTRIES=2048u", "-DMGC_OOM_RESERVE=8192u"], Class::CortexM7),
    ("host-freepad.o", "src/gc/host/mgc_host.cpp", &["-DMGC_HOST_DRIVE_ALLOC", "-DMGC_HOST_DRIVE_COLLECT", "-DMGC_HOST_DRIVE_SWEEP", "-DMGC_HOST_DRIVE_FREEPAD"], Class::CortexM7),
    ("heap-sweep-break-freepad.o", "src/gc/mgc_heap.cpp", &["-DMGC_HEAP_LIVE", "-DMGC_COLLECT_LIVE", "-DMGC_SWEEP_LIVE", "-DMGC_HEAP_BYTES=2097152u", "-DMGC_BREAK_FREE_PAD"], Class::CortexM7),
    ("host-handles.o", "src/gc/host/mgc_host.cpp", &["-DMGC_HOST_DRIVE_ALLOC", "-DMGC_HOST_DRIVE_COLLECT", "-DMGC_HOST_DRIVE_SWEEP", "-DMGC_HOST_DRIVE_TUNE", "-DMGC_HOST_DRIVE_HANDLES", "-DMGC_HANDLES_ITER=100000u"], Class::CortexM7),
    ("skel-tune-break-lock.o", "src/gc/mgc_skeleton.cpp", &["-DMGC_HEAP_LIVE", "-DMGC_COLLECT_LIVE", "-DMGC_SWEEP_LIVE", "-DMGC_TUNE_LIVE", "-DMGC_HANDLE_SLOTS=64", "-DMGC_BREAK_HANDLE_LOCK"], Class::CortexM7),
    ("host-nogc.o", "src/gc/host/mgc_host.cpp", &["-DMGC_HOST_DRIVE_ALLOC", "-DMGC_HOST_DRIVE_COLLECT", "-DMGC_HOST_DRIVE_SWEEP", "-DMGC_HOST_DRIVE_TUNE", "-DMGC_HOST_DRIVE_NOGC"], Class::CortexM7),
    ("skel-tune-break-nogc.o", "src/gc/mgc_skeleton.cpp", &["-DMGC_HEAP_LIVE", "-DMGC_COLLECT_LIVE", "-DMGC_SWEEP_LIVE", "-DMGC_TUNE_LIVE", "-DMGC_HANDLE_SLOTS=64", "-DMGC_BREAK_NOGC_CLAIM"], Class::CortexM7),
    ("skel-row-an505.o", "src/gc/mgc_skeleton.cpp", &["-DMGC_HEAP_LIVE", "-DMGC_COLLECT_LIVE", "-DMGC_SWEEP_LIVE", "-DMGC_TUNE_LIVE", "-DMGC_HANDLE_SLOTS=64"], Class::CortexM33),
    ("heap-row-an505.o", "src/gc/mgc_heap.cpp", &["-DMGC_HEAP_LIVE", "-DMGC_COLLECT_LIVE", "-DMGC_SWEEP_LIVE", "-DMGC_TUNE_LIVE", "-DMGC_HEAP_BYTES=2097152u", "-DMGC_ARENA_COVER_BYTES=4194304u"], Class::CortexM33),
    ("collect-row-an505.o", "src/gc/mgc_collect.cpp", &["-DMGC_HEAP_LIVE", "-DMGC_COLLECT_LIVE", "-DMGC_SWEEP_LIVE", "-DMGC_TUNE_LIVE"], Class::CortexM33),
    ("heap-row-u5a5.o", "src/gc/mgc_heap.cpp", &["-DMGC_HEAP_LIVE", "-DMGC_COLLECT_LIVE", "-DMGC_SWEEP_LIVE", "-DMGC_TUNE_LIVE", "-DMGC_HEAP_BYTES=1048576u", "-DMGC_ARENA_COVER_BYTES=4194304u"], Class::CortexM33),
];

/// Parse VENDORED.md's per-file sha bullets: a line whose trimmed form is
/// "- `<relpath>`" followed by a line whose trimmed form is
/// "`<64 lowercase hex>`". Returns (relpath, sha) pairs in file order.
/// A relpath is one relative path token -- non-empty, no spaces, no
/// leading or trailing '/'; root-level names (no '/' at all) are valid
/// records. The caller treats an empty parse as a broken instrument,
/// never as "nothing to verify".
pub(crate) fn parse_vendored_manifest(text: &str) -> Vec<(String, String)> {
    let lines: Vec<&str> = text.lines().collect();
    let mut out = Vec::new();
    for i in 0..lines.len().saturating_sub(1) {
        let a = lines[i].trim();
        let b = lines[i + 1].trim();
        let Some(rest) = a.strip_prefix("- `") else {
            continue;
        };
        let Some((path, _)) = rest.split_once('`') else {
            continue;
        };
        if path.is_empty() || path.contains(' ') || path.starts_with('/') || path.ends_with('/') {
            continue;
        }
        let Some(sha) = b.strip_prefix('`').and_then(|r| r.strip_suffix('`')) else {
            continue;
        };
        if sha.len() == 64 && sha.bytes().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()) {
            out.push((path.to_string(), sha.to_string()));
        }
    }
    out
}

/// Extract `#define <name> <decimal>` from header text.
fn extract_define(text: &str, name: &str) -> Option<u32> {
    for line in text.lines() {
        let t = line.trim();
        if let Some(rest) = t.strip_prefix("#define ") {
            let mut it = rest.split_whitespace();
            if it.next() == Some(name) {
                return it.next().and_then(|v| v.parse().ok());
            }
        }
    }
    None
}

/// Walk src/vendor recursively, returning paths relative to it with '/'
/// separators, sorted.
fn walk_vendor(root: &Path) -> Vec<String> {
    fn rec(dir: &Path, root: &Path, out: &mut Vec<String>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                rec(&p, root, out);
            } else if let Ok(rel) = p.strip_prefix(root) {
                out.push(rel.to_string_lossy().replace('\\', "/"));
            }
        }
    }
    let mut v = Vec::new();
    rec(root, root, &mut v);
    v.sort();
    v
}

/// Undefined-symbol names from an `nm -u`-shaped dump (lines whose
/// symbol-type column is U), sorted.
pub(crate) fn parse_undefined(dump: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for line in dump.lines() {
        let mut it = line.split_whitespace();
        match (it.next(), it.next(), it.next()) {
            (Some("U"), Some(name), None) => out.push(name.to_string()),
            (Some(_), Some("U"), Some(name)) => out.push(name.to_string()),
            _ => {}
        }
    }
    out.sort();
    out
}

/// True when the sorted import list equals the pinned set exactly.
/// One parsed board row from src/gc/host/mgc_board_rows.h (parsed as
/// TEXT -- unselected preprocessor blocks included -- so every declared
/// row is checked, not just the compiled one).
#[derive(Debug, Clone)]
struct BoardRow {
    board: String,
    profile: String,
    basis: String,
    region: u64,
    cover: u64,
    kheap: u64,
    stack_default: u64,
    stack_clamp: u64,
    handles: u64,
    foh: u64,
    markstack: u64,
    finq: u64,
    reserve: u64,
    window: u64,
    wfloor: u64,
    retries: u64,
    frozen: u64,
}

/// Parse the row header's per-board blocks: each opens with
/// `defined(MGC_HOST_ROW_<NAME>)` and carries one `#define MGC_ROW_*`
/// per field. An empty parse or a block missing a field is a broken
/// instrument, never "nothing to check".
fn parse_board_rows(text: &str) -> Result<Vec<BoardRow>, String> {
    let mut blocks: Vec<Vec<&str>> = Vec::new();
    for line in text.lines() {
        let t = line.trim();
        if (t.starts_with("#if ") || t.starts_with("#elif ")) && t.contains("defined(MGC_HOST_ROW_") {
            blocks.push(Vec::new());
        } else if t.starts_with("#else") || (t.starts_with("#endif") && !blocks.is_empty() && t.contains("MGC_BOARD_ROWS_H")) {
        } else if let Some(b) = blocks.last_mut() {
            b.push(t);
        }
    }
    if blocks.is_empty() {
        return Err("no MGC_HOST_ROW_ blocks parsed (broken instrument or gutted header)".to_string());
    }
    let mut rows = Vec::new();
    for (i, b) in blocks.iter().enumerate() {
        let s = |key: &str| -> Result<String, String> {
            for l in b {
                if let Some(rest) = l.strip_prefix(&format!("#define {key} ")) {
                    let v = rest.trim().trim_matches('"');
                    return Ok(v.to_string());
                }
                if let Some(rest) = l.strip_prefix(&format!("#define {key}\t")) {
                    return Ok(rest.trim().trim_matches('"').to_string());
                }
                if l.starts_with("#define ") {
                    let mut it = l["#define ".len()..].split_whitespace();
                    if it.next() == Some(key) {
                        if let Some(v) = it.next() {
                            return Ok(v.trim_matches('"').to_string());
                        }
                    }
                }
            }
            Err(format!("row block {} missing {key}", i + 1))
        };
        let n = |v: String| -> Result<u64, String> {
            v.trim_end_matches('u')
                .parse::<u64>()
                .map_err(|_| format!("row block {}: unparsable number '{v}'", i + 1))
        };
        rows.push(BoardRow {
            board: s("MGC_ROW_BOARD")?,
            profile: s("MGC_ROW_PROFILE")?,
            basis: s("MGC_ROW_BASIS")?,
            region: n(s("MGC_ROW_REGION_BYTES")?)?,
            cover: n(s("MGC_ROW_COVER_BYTES")?)?,
            kheap: n(s("MGC_ROW_KHEAP_BYTES")?)?,
            stack_default: n(s("MGC_ROW_STACK_DEFAULT")?)?,
            stack_clamp: n(s("MGC_ROW_STACK_CLAMP")?)?,
            handles: n(s("MGC_ROW_HANDLE_SLOTS")?)?,
            foh: n(s("MGC_ROW_FOH_POOL_BYTES")?)?,
            markstack: n(s("MGC_ROW_MARKSTACK_ENTRIES")?)?,
            finq: n(s("MGC_ROW_FINQ_CAP")?)?,
            reserve: n(s("MGC_ROW_OOM_RESERVE")?)?,
            window: n(s("MGC_ROW_WINDOW_BYTES")?)?,
            wfloor: n(s("MGC_ROW_WINDOW_FLOOR")?)?,
            retries: n(s("MGC_ROW_WALL_RETRIES")?)?,
            frozen: n(s("MGC_ROW_FROZEN_SEGS")?)?,
        });
    }
    Ok(rows)
}

/// The SDK's board table, where the row tie of the two rederive legs reads
/// the rows from.
pub(crate) const SDK_BOARD_TABLE: &str = "src/sdk/Lamella.Rhu.Sdk/Sdk/boards.props";

/// One row of the SDK's board table: its name, its processor class and the
/// tier objects its image links, by file name.
#[derive(Debug, Clone)]
pub(crate) struct SdkRow {
    pub(crate) board: String,
    pub(crate) class: Class,
    pub(crate) objects: Vec<String>,
}

/// An attribute's value inside one row element of the board table.
fn sdk_row_attr(row: &str, name: &str) -> Option<String> {
    let key = format!("{name}=\"");
    let start = row.find(&key)? + key.len();
    let end = row[start..].find('"')? + start;
    Some(row[start..end].to_string())
}

/// Parse the board table's rows. An empty parse, a row without a name, a
/// class this harness does not compile for or an empty object list is a
/// broken instrument, never "nothing to check".
pub(crate) fn parse_sdk_rows(text: &str) -> Result<Vec<SdkRow>, String> {
    let mut rows = Vec::new();
    for (i, _) in text.match_indices("<RhuBoardRow Include=\"") {
        let end = text[i..].find("/>").ok_or("a board row without its end")? + i;
        let row = &text[i..end];
        let board = sdk_row_attr(row, "Include").ok_or("a board row without a name")?;
        let class = match sdk_row_attr(row, "Class").as_deref() {
            Some("cortex-m7") => Class::CortexM7,
            Some("cortex-m33") => Class::CortexM33,
            other => return Err(format!("{board}: the class {other:?} is not one this harness compiles for")),
        };
        let objects: Vec<String> = sdk_row_attr(row, "Objects")
            .ok_or(format!("{board}: no Objects"))?
            .split_whitespace()
            .map(|p| p.rsplit('/').next().unwrap_or(p).to_string())
            .collect();
        if objects.is_empty() {
            return Err(format!("{board}: an empty object list"));
        }
        rows.push(SdkRow { board, class, objects });
    }
    if rows.is_empty() {
        return Err("the board table has no rows (broken instrument or a gutted table)".to_string());
    }
    Ok(rows)
}

/// The selector macro of the row-header block whose MGC_ROW_BOARD names
/// the board: the SDK's name, or that name without its `arm-` prefix (the
/// two emulator rows' blocks carry the machine's name).
pub(crate) fn row_selector_of(header: &str, sdk_board: &str) -> Option<String> {
    let name = sdk_board.strip_prefix("arm-").unwrap_or(sdk_board);
    let quoted = format!("\"{name}\"");
    let mut current: Option<String> = None;
    for line in header.lines() {
        let t = line.trim();
        if (t.starts_with("#if ") || t.starts_with("#elif ")) && t.contains("defined(MGC_HOST_ROW_") {
            let at = t.find("defined(")? + "defined(".len();
            let end = t[at..].find(')')? + at;
            current = Some(t[at..end].to_string());
        } else if t.starts_with("#define MGC_ROW_BOARD") && t.contains(&quoted) {
            return current;
        }
    }
    None
}

/// Parse a size token of the linker-script form: `4M`, `384K`, `2496K`
/// or a plain byte count.
fn parse_size_token(t: &str) -> Option<u64> {
    let t = t.trim();
    if let Some(m) = t.strip_suffix('M') {
        return m.parse::<u64>().ok().map(|v| v * 1024 * 1024);
    }
    if let Some(k) = t.strip_suffix('K') {
        return k.parse::<u64>().ok().map(|v| v * 1024);
    }
    t.parse::<u64>().ok()
}

/// Parse a payload map's DATA window and its guaranteed arena floor:
/// (data_base, data_len, arena_min).
fn parse_map_geometry(text: &str) -> Result<(u64, u64, u64), String> {
    let mut data: Option<(u64, u64)> = None;
    let mut amin: Option<u64> = None;
    for line in text.lines() {
        let t = line.trim();
        if t.starts_with("DATA") && t.contains("ORIGIN =") && t.contains("LENGTH =") {
            let origin = t
                .split("ORIGIN =")
                .nth(1)
                .and_then(|r| r.split(',').next())
                .map(str::trim)
                .and_then(|h| h.strip_prefix("0x"))
                .and_then(|h| u64::from_str_radix(h, 16).ok());
            let length = t
                .split("LENGTH =")
                .nth(1)
                .map(str::trim)
                .and_then(parse_size_token);
            if let (Some(o), Some(l)) = (origin, length) {
                data = Some((o, l));
            }
        }
        if let Some(rest) = t.strip_prefix("RHU_ARENA_MIN = ") {
            amin = parse_size_token(rest.trim_end_matches(';'));
        }
    }
    match (data, amin) {
        (Some((b, l)), Some(a)) => Ok((b, l, a)),
        _ => Err("DATA window or RHU_ARENA_MIN not parsed (broken instrument or a map shape change)".to_string()),
    }
}

/// Parse a board module's `pub const KHEAP_BYTES: usize = <a> * <b>;`
/// (or a plain value).
fn parse_kheap_const(text: &str) -> Result<u64, String> {
    for line in text.lines() {
        let t = line.trim();
        if let Some(rest) = t.strip_prefix("pub const KHEAP_BYTES: usize = ") {
            let expr = rest.trim_end_matches(';').trim();
            let parts: Vec<&str> = expr.split('*').map(str::trim).collect();
            let mut v: u64 = 1;
            for p in &parts {
                v = v
                    .checked_mul(p.parse::<u64>().map_err(|_| format!("unparsable KHEAP_BYTES factor '{p}'"))?)
                    .ok_or("KHEAP_BYTES overflow")?;
            }
            return Ok(v);
        }
    }
    Err("KHEAP_BYTES not found in the board module".to_string())
}

/// The minimal power-of-two cover whose aligned granule holds the whole
/// DATA window: smallest P with (base & !(P-1)) + P >= base + len.
fn minimal_cover(base: u64, len: u64) -> u64 {
    let mut p: u64 = 1024;
    while (base & !(p - 1)) + p < base + len {
        p <<= 1;
        if p > (1u64 << 40) {
            break;
        }
    }
    p
}

/// One row against its map geometry + board module: the table-check
/// rules. Returns the violated rule as the error. The FOH pool is a
/// per-row arena budget: it counts inside a floor-basis row's
/// inequality and must be whole pages (the mmap bridge draws in
/// 4096-byte pages, so a smaller unit would round up past the budget).
fn row_check(row: &BoardRow, data_base: u64, data_len: u64, arena_min: u64, kheap_const: u64) -> Result<(), String> {
    if row.kheap != kheap_const {
        return Err(format!(
            "row {}: kheap_carve {} != the board module's KHEAP_BYTES {}",
            row.board, row.kheap, kheap_const
        ));
    }
    if row.foh % 4096 != 0 {
        return Err(format!(
            "row {}: foh_pool {} is not whole pages (the mmap bridge draws in 4096-byte pages)",
            row.board, row.foh
        ));
    }
    let mini = row.region < 256 * 1024;
    if mini != (row.profile == "mini") {
        return Err(format!(
            "row {}: profile '{}' does not match the region class ({} bytes)",
            row.board, row.profile, row.region
        ));
    }
    let want_cover = minimal_cover(data_base, data_len);
    if row.cover != want_cover {
        return Err(format!(
            "row {}: cover {} != minimal granule {} for DATA [0x{:x},+0x{:x})",
            row.board, row.cover, want_cover, data_base, data_len
        ));
    }
    match row.basis.as_str() {
        "floor" => {
            let ask = row.kheap + row.region + row.region / 32 + row.markstack * 4 + row.foh;
            if ask > arena_min {
                return Err(format!(
                    "row {}: kheap+region+bitmap+markstack+foh = {} overflows the map's guaranteed floor {}",
                    row.board, ask, arena_min
                ));
            }
        }
        "link" => {
            if row.region != arena_min {
                return Err(format!(
                    "row {}: link-basis region {} != the map floor {} (the flown roomy posture is region == floor)",
                    row.board, row.region, arena_min
                ));
            }
        }
        other => return Err(format!("row {}: unknown basis '{other}'", row.board)),
    }
    Ok(())
}

/// Declared-uniform fields must be uniform across every row. The FOH
/// pool is NOT one of them: it is a per-row budget inside each row's
/// floor inequality (a mini row funds a smaller pool than a suite row).
fn rows_uniform(rows: &[BoardRow]) -> Result<(), String> {
    let f = &rows[0];
    for r in &rows[1..] {
        if r.window != f.window
            || r.wfloor != f.wfloor
            || r.retries != f.retries
            || r.frozen != f.frozen
            || r.handles != f.handles
            || r.stack_default != f.stack_default
            || r.stack_clamp != f.stack_clamp
        {
            return Err(format!("row {}: a declared-uniform field differs from row {}", r.board, f.board));
        }
    }
    Ok(())
}

/// Parse the heap's card-cover line
/// (`[mgc-heap] card cover: floor=0x%08lx span=%lu KB cards=%lu of %lu`)
/// out of a raw transcript: (floor_hex, span_kb, cards, of).
fn parse_cover_line(raw: &[u8]) -> Result<(String, u64, u64, u64), String> {
    let text = String::from_utf8_lossy(raw);
    for line in text.lines() {
        if let Some(rest) = line.trim().strip_prefix("[mgc-heap] card cover: floor=") {
            let mut it = rest.split_whitespace();
            let floor = it.next().unwrap_or("").to_string();
            let mut span = None;
            let mut cards = None;
            let mut of = None;
            let toks: Vec<&str> = rest.split_whitespace().collect();
            for (i, t) in toks.iter().enumerate() {
                if let Some(v) = t.strip_prefix("span=") {
                    span = v.parse::<u64>().ok();
                }
                if let Some(v) = t.strip_prefix("cards=") {
                    cards = v.parse::<u64>().ok();
                }
                if *t == "of" {
                    of = toks.get(i + 1).and_then(|v| v.trim_end_matches(|c: char| !c.is_ascii_digit()).parse::<u64>().ok());
                }
            }
            return match (span, cards, of) {
                (Some(s), Some(c), Some(o)) => Ok((floor, s, c, o)),
                _ => Err(format!("cover line parsed incompletely: '{line}'")),
            };
        }
    }
    Err("no card-cover line in the transcript".to_string())
}

fn imports_match(got: &[String], want: &[&str]) -> bool {
    let mut w: Vec<&str> = want.to_vec();
    w.sort_unstable();
    got.len() == w.len() && got.iter().zip(w.iter()).all(|(g, w)| g == w)
}

/// The first forbidden-family hit in an import list, if any.
fn forbidden_import(got: &[String]) -> Option<String> {
    got.iter()
        .find(|n| FORBIDDEN_IMPORT_FAMILIES.iter().any(|f| n.starts_with(f)))
        .cloned()
}

/// A boot's call-inventory union: every `[mgc-skel] call <head>` line
/// contributes its head -- the `<Iface>::<Method>` token, cut at the
/// first '(' or space -- deduplicated and sorted. The union over a boot
/// IS the init-time call inventory the gc.B case pins (the skeleton
/// logs every vtable method the EE side reaches).
fn call_union(raw: &[u8]) -> Vec<String> {
    let text = String::from_utf8_lossy(raw);
    let mut out: Vec<String> = Vec::new();
    for line in text.lines() {
        let t = line.trim_end_matches('\r');
        let Some(rest) = t.strip_prefix("[mgc-skel] call ") else {
            continue;
        };
        let head: String = rest.chars().take_while(|c| *c != '(' && *c != ' ').collect();
        if !head.is_empty() && !out.contains(&head) {
            out.push(head);
        }
    }
    out.sort();
    out
}

/// Method names declared on the flat GCToEEInterface class in the
/// vendored header: inside the class block, every trimmed
/// `static <ret> <Name>(...)` line contributes `<Name>` (the token
/// immediately before the first parenthesis). One leg of the gc.B
/// three-way surface tie.
fn gctoee_decl_names(text: &str) -> Vec<String> {
    let mut names = Vec::new();
    let mut in_class = false;
    for line in text.lines() {
        let t = line.trim();
        if t.starts_with("class GCToEEInterface") {
            in_class = true;
            continue;
        }
        if in_class && t.starts_with("};") {
            break;
        }
        if !in_class || !t.starts_with("static ") {
            continue;
        }
        let Some(paren) = t.find('(') else {
            continue;
        };
        if let Some(name) = t[..paren]
            .rsplit(|c: char| c.is_whitespace() || c == '*' || c == '&')
            .next()
        {
            if !name.is_empty() {
                names.push(name.to_string());
            }
        }
    }
    names.sort();
    names
}

/// REF(<name>) rows in the probe source (each materializes one flat
/// interface method's address as a relocation). The macro's own
/// `#define REF(...)` line does not match -- counting it is exactly the
/// off-by-one this parser exists to avoid.
fn probe_ref_names(text: &str) -> Vec<String> {
    let mut names = Vec::new();
    for line in text.lines() {
        let t = line.trim();
        let Some(rest) = t.strip_prefix("REF(") else {
            continue;
        };
        let Some((name, _)) = rest.split_once(')') else {
            continue;
        };
        names.push(name.to_string());
    }
    names.sort();
    names
}

/// The method name inside a flat-interface mangled symbol
/// (`_ZN15GCToEEInterface<len><Name>E...`): parse the length prefix,
/// take that many characters. None for any other shape -- an alien
/// import in the probe object must fail the tie, never blend in.
fn mangled_method_name(sym: &str) -> Option<String> {
    let rest = sym.strip_prefix("_ZN15GCToEEInterface")?;
    let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
    let n: usize = digits.parse().ok()?;
    let tail = &rest[digits.len()..];
    if tail.len() < n {
        return None;
    }
    Some(tail[..n].to_string())
}

/// Defined external names from a `--defined-only --extern-only` dump
/// (`<value> <type> <name>` lines), sorted and deduplicated.
fn parse_defined_extern(dump: &str) -> Vec<String> {
    let mut out: Vec<String> = dump
        .lines()
        .filter_map(|l| {
            let mut it = l.split_whitespace();
            match (it.next(), it.next(), it.next(), it.next()) {
                (Some(_), Some(_), Some(name), None) => Some(name.to_string()),
                _ => None,
            }
        })
        .collect();
    out.sort();
    out.dedup();
    out
}

/// The first demanded spelling the merged runtime object does not
/// define, or None. A demand the archive spells otherwise -- the 32-bit
/// fixed-width types are the known case -- is a link refusal waiting at
/// the hosted-runtime image, caught here on the frozen objects.
fn seam_spelling_fault(demands: &[String], defined: &[String]) -> Option<String> {
    demands
        .iter()
        .find(|d| !defined.iter().any(|x| x == *d))
        .map(|d| format!("flat-interface demand '{d}' is not defined in the frozen merged runtime object by that spelling (the archive spells a parameter type differently, or the method is absent)"))
}

/// Byte offset of the first occurrence of `needle` in `hay`.
fn find_pos(hay: &[u8], needle: &str) -> Option<usize> {
    let n = needle.as_bytes();
    if n.is_empty() || hay.len() < n.len() {
        return None;
    }
    (0..=hay.len() - n.len()).find(|&i| &hay[i..i + n.len()] == n)
}

/// True when every marker appears and their first occurrences are in
/// strictly increasing transcript order (the prints-before-it-reports
/// property is an ORDER claim, not a presence claim).
fn in_order(raw: &[u8], markers: &[&str]) -> bool {
    let mut last: Option<usize> = None;
    for m in markers {
        match find_pos(raw, m) {
            Some(p) => {
                if let Some(l) = last {
                    if p <= l {
                        return false;
                    }
                }
                last = Some(p);
            }
            None => return false,
        }
    }
    true
}

/// An external tool command as words (a double-quoted word keeps its
/// spaces) plus its reported version line.
pub(crate) struct ToolCmd {
    pub(crate) words: Vec<String>,
    pub(crate) version: String,
    /// The C-library sysroot the compiler was resolved with (empty for a
    /// tool that has none, such as the linker).
    pub(crate) sysroot: String,
}

/// Probe `<command> --version` bounded; first stdout line.
fn probe_version(words: &[String]) -> Result<String, String> {
    let probe_out = std::env::temp_dir().join(format!("rhu-gate-gxx-probe-{}.txt", std::process::id()));
    let probe_err = std::env::temp_dir().join(format!("rhu-gate-gxx-probe-{}.stderr", std::process::id()));
    let mut args: Vec<&str> = words[1..].iter().map(String::as_str).collect();
    args.push("--version");
    let outcome = run_captured(
        Path::new(&words[0]),
        &args,
        &probe_out,
        &probe_err,
        Duration::from_secs(20),
    );
    let bytes = std::fs::read(&probe_out).unwrap_or_default();
    let _ = std::fs::remove_file(&probe_out);
    let _ = std::fs::remove_file(&probe_err);
    let first = String::from_utf8_lossy(&bytes)
        .lines()
        .next()
        .unwrap_or("")
        .trim_end_matches('\r')
        .to_string();
    if !matches!(outcome, Ok(ref o) if o.exit == 0) || first.is_empty() {
        return Err(format!(
            "the tool '{}' did not answer --version (the command: {})",
            words[0],
            words.join(" ")
        ));
    }
    Ok(first)
}

/// The pinned compiler of record: the version string the resolved compiler
/// must report. The runtime archives at the pin carry the same clang.
pub(crate) const CLANG_VERSION_PIN: &str = "clang version 22.1.8";

/// The image-tier compiler: clang, the compiler of record.
/// RHU_CLANG (a command: whitespace splits it and a double-quoted word
/// keeps its spaces), else the untracked per-machine file
/// tools/toolchains/clang.local (first line = the command, second line =
/// the C-library sysroot in the form the compiler opens), else a loud
/// refusal naming both; RHU_CLANG_SYSROOT overrides the sysroot. The
/// resolved compiler must report the pinned version -- a different
/// compiler is a world change, a gated migration, never a drift. The
/// sysroot rides the command as `--sysroot=`, so every compile the gates
/// run finds the kit's C headers, and the gates that need the sysroot as a
/// path (the C-library layout control) read it off the resolved command.
pub(crate) fn resolve_clang(repo: &Path) -> Result<ToolCmd, String> {
    let local = repo.join("tools").join("toolchains").join("clang.local");
    let local_text = std::fs::read_to_string(&local).unwrap_or_default();
    let mut local_lines = local_text.lines().map(|l| l.trim().to_string());
    let local_cmd = local_lines.next().unwrap_or_default();
    let local_sysroot = local_lines.next().unwrap_or_default();
    let cmd = match std::env::var("RHU_CLANG") {
        Ok(v) if !v.trim().is_empty() => v,
        _ => local_cmd,
    };
    if cmd.trim().is_empty() {
        return Err(
            "no image-tier compiler resolved: set RHU_CLANG (a command; a double-quoted word keeps its spaces) or put the command in tools/toolchains/clang.local (line 1 the command, line 2 the C-library sysroot; the pinned clang 22.1.8)"
                .to_string(),
        );
    }
    let sysroot = match std::env::var("RHU_CLANG_SYSROOT") {
        Ok(v) if !v.trim().is_empty() => v.trim().to_string(),
        _ => local_sysroot,
    };
    if sysroot.is_empty() {
        return Err(
            "no C-library sysroot resolved for the image-tier compiler: set RHU_CLANG_SYSROOT or write it as the second line of tools/toolchains/clang.local (the kit's arm-none-eabi directory)"
                .to_string(),
        );
    }
    let mut words = command_words(&cmd)
        .map_err(|e| format!("{e} (RHU_CLANG or tools/toolchains/clang.local)"))?;
    words.push(format!("--sysroot={sysroot}"));
    let version = probe_version(&words).map_err(|e| format!("{e} (set RHU_CLANG or fix tools/toolchains/clang.local)"))?;
    if !version.contains(CLANG_VERSION_PIN) {
        return Err(format!(
            "resolved compiler reports '{version}' -- the image-tier pin is {CLANG_VERSION_PIN}; a compiler bump is a gated migration, not an override"
        ));
    }
    Ok(ToolCmd { words, version, sysroot })
}

/// The image link driver, shared with the payload gate's world:
/// RHU_PAYLOAD_LD (a command; a double-quoted word keeps its spaces)
/// overrides; otherwise rust-lld
/// from the pinned toolchain's sysroot with `-flavor gnu`.
pub(crate) fn resolve_ld(facts: &RustcFacts) -> Result<ToolCmd, String> {
    let words: Vec<String> = match std::env::var("RHU_PAYLOAD_LD") {
        Ok(v) if !v.trim().is_empty() => {
            command_words(&v).map_err(|e| format!("{e} (RHU_PAYLOAD_LD)"))?
        }
        _ => {
            let lld = llvm_tool(facts, "rust-lld").map_err(|e| {
                format!(
                    "no image linker resolved: set RHU_PAYLOAD_LD (a command; a double-quoted word keeps its spaces) -- the default rust-lld is unavailable: {e}"
                )
            })?;
            vec![
                lld.to_string_lossy().into_owned(),
                "-flavor".to_string(),
                "gnu".to_string(),
            ]
        }
    };
    let version = probe_version(&words)
        .map_err(|e| format!("{e} (set RHU_PAYLOAD_LD to a working linker command)"))?;
    Ok(ToolCmd { words, version, sysroot: String::new() })
}

/// Run one external tool with repo-relative arguments, bounded, streams
/// captured to files; MSYS path conversion disabled (inert for native
/// and WSL tools, load-bearing for MSYS-shimmed overrides).
pub(crate) fn run_tool_bounded(
    repo: &Path,
    tool: &ToolCmd,
    args: &[String],
    stdout_path: &Path,
    stderr_path: &Path,
) -> Result<bool, String> {
    let out_f = std::fs::File::create(stdout_path).map_err(|e| format!("cannot create tool log: {e}"))?;
    let err_f = std::fs::File::create(stderr_path).map_err(|e| format!("cannot create tool log: {e}"))?;
    let mut cmd = Command::new(&tool.words[0]);
    cmd.args(&tool.words[1..])
        .args(args)
        .current_dir(repo)
        .env("MSYS_NO_PATHCONV", "1")
        .env("MSYS2_ARG_CONV_EXCL", "*")
        .stdin(Stdio::null())
        .stdout(Stdio::from(out_f))
        .stderr(Stdio::from(err_f));
    match status_bounded(&mut cmd, Duration::from_secs(TOOL_TIMEOUT_S)) {
        Ok(Some(s)) => Ok(s.success()),
        Ok(None) => Ok(false),
        Err(e) => Err(format!("tool spawn: {e}")),
    }
}

struct Gc {
    repo: PathBuf,
    out: PathBuf,
    results: Results,
    /// The results-surface identity of this invocation: "gc" for the
    /// consumer gate, "gc-rederive" for the maintainer's leg (the shared
    /// checker requires every case line to match the run line's gate).
    gate_id: &'static str,
    display: &'static str,
    /// gc.A's positive raw transcript, kept for gc.B's inventory pin
    /// (the activation boot's union is the init-time call inventory).
    raw_gca: Vec<u8>,
}

impl Gc {
    fn fail(&mut self, case: &str, msg: &str) -> ! {
        self.results.emit(&format!(
            "{{\"schema\":1,\"type\":\"case\",\"gate\":\"{}\",\"case\":\"{case}\",\"verdict\":\"fail\",\"exit\":null,\"exit_want\":null,\"control\":\"positive\",\"image\":null}}",
            self.gate_id
        ));
        self.results.tally("fail", "positive");
        println!("{}: FAIL -- {case}: {msg}", self.display);
        self.results.emit_summary(self.gate_id, "fail", 1);
        let _ = validate_results(&self.repo, self.results.path());
        exit(1);
    }

    /// A run-shaped case failure carrying the observed exit and image.
    fn fail_run(
        &mut self,
        case: &str,
        exit_got: i32,
        exit_want: i32,
        control: &str,
        broke: &str,
        sha: &str,
        path_rel: &str,
        msg: &str,
    ) -> ! {
        let broke_frag = if control == "negative" {
            format!(",\"broke\":\"{broke}\"")
        } else {
            String::new()
        };
        self.results.emit(&format!(
            "{{\"schema\":1,\"type\":\"case\",\"gate\":\"{}\",\"case\":\"{case}\",\"verdict\":\"fail\",\"exit\":{exit_got},\"exit_want\":{exit_want},\"control\":\"{control}\"{broke_frag},\"image\":{{\"path\":\"{path_rel}\",\"sha256\":\"{sha}\"}}}}",
            self.gate_id
        ));
        self.results.tally("fail", control);
        println!("{}: FAIL -- {case}: {msg}", self.display);
        self.results.emit_summary(self.gate_id, "fail", 1);
        let _ = validate_results(&self.repo, self.results.path());
        exit(1);
    }

    /// The frozen tier verified BEFORE the first world (the payload
    /// gate's discipline): the manifest's own bytes tie the
    /// in-code pin, its records tie the in-code plan by name both
    /// directions, every frozen object's sha256 ties its record, and no
    /// unrecorded object hides in the directory. A mismatch anywhere is
    /// a moved frozen artifact -- WRONG, never "rebuilt".
    fn verify_frozen(&mut self) {
        println!("== frozen inputs: sha256 vs goldens/gc/MANIFEST.md ==");
        let dir = self.repo.join("goldens").join("gc");
        let mpath = dir.join("MANIFEST.md");
        let msha = match sha256_file(&mpath) {
            Ok(s) => s,
            Err(e) => self.fail(
                "frozen-inputs",
                &format!("goldens/gc/MANIFEST.md unreadable: {e} (the frozen tier's record; a drop copy without goldens/gc runs the chain with this tier DECLARED, never this gate)"),
            ),
        };
        if msha != SHA_FROZEN_MANIFEST {
            self.fail(
                "frozen-inputs",
                &format!("goldens/gc/MANIFEST.md sha256 {msha} != pinned {SHA_FROZEN_MANIFEST} -- the frozen record moved without a registered re-freeze"),
            );
        }
        let text = match std::fs::read_to_string(&mpath) {
            Ok(t) => t,
            Err(e) => self.fail("frozen-inputs", &format!("goldens/gc/MANIFEST.md unreadable: {e}")),
        };
        let records = parse_vendored_manifest(&text);
        if records.len() != FROZEN_PLAN.len() {
            self.fail(
                "frozen-inputs",
                &format!("manifest parsed to {} records, the frozen plan holds {} -- a record set that drifted from the plan", records.len(), FROZEN_PLAN.len()),
            );
        }
        for (name, _, _, _) in FROZEN_PLAN {
            let want_path = format!("goldens/gc/{name}");
            if !records.iter().any(|(p, _)| p == &want_path) {
                self.fail("frozen-inputs", &format!("plan object {name} has no manifest record"));
            }
        }
        for (path, want) in &records {
            let Some(name) = path.strip_prefix("goldens/gc/") else {
                self.fail("frozen-inputs", &format!("manifest record '{path}' is outside goldens/gc/"));
            };
            if !FROZEN_PLAN.iter().any(|(n, _, _, _)| n == &name) {
                self.fail("frozen-inputs", &format!("manifest record '{path}' has no frozen-plan entry"));
            }
            let got = match sha256_file(&self.repo.join(path.replace('/', std::path::MAIN_SEPARATOR_STR))) {
                Ok(s) => s,
                Err(e) => self.fail("frozen-inputs", &format!("{path} unreadable: {e} (a record without a file)")),
            };
            if &got != want {
                self.fail(
                    "frozen-inputs",
                    &format!("{path}: sha256 {got} != recorded {want} (frozen artifact moved -- WRONG, never rebuilt; derivation is 'rhu-gate gc rederive')"),
                );
            }
        }
        let mut on_disk: Vec<String> = match std::fs::read_dir(&dir) {
            Ok(rd) => rd
                .flatten()
                .filter_map(|e| e.file_name().to_str().map(str::to_string))
                .filter(|n| n.ends_with(".o"))
                .collect(),
            Err(e) => self.fail("frozen-inputs", &format!("cannot list goldens/gc: {e}")),
        };
        on_disk.sort();
        for name in &on_disk {
            if !FROZEN_PLAN.iter().any(|(n, _, _, _)| *n == name.as_str()) {
                self.fail("frozen-inputs", &format!("goldens/gc/{name} has NO manifest record (a file without a record)"));
            }
        }
        self.results.emit(&format!(
            "{{\"schema\":1,\"type\":\"case\",\"gate\":\"gc\",\"case\":\"frozen-inputs\",\"verdict\":\"pass\",\"exit\":null,\"exit_want\":null,\"control\":\"positive\",\"image\":null,\"stats\":{{\"manifest_sha\":\"{SHA_FROZEN_MANIFEST}\",\"objects\":{}}}}}",
            FROZEN_PLAN.len()
        ));
        self.results.tally("pass", "positive");
        println!(
            "== frozen inputs: PASS ({}/{} objects tie the manifest both directions; the manifest ties the in-code pin) ==",
            FROZEN_PLAN.len(),
            FROZEN_PLAN.len()
        );
    }

    fn run_vendor(&mut self) {
        println!("== case gc.vendor ==");
        let manifest_path = self.repo.join("src").join("VENDORED.md");
        let text = match std::fs::read_to_string(&manifest_path) {
            Ok(t) => t,
            Err(e) => self.fail("gc.vendor", &format!("cannot read src/VENDORED.md: {e}")),
        };
        let records = parse_vendored_manifest(&text);
        if records.is_empty() {
            self.fail(
                "gc.vendor",
                "src/VENDORED.md parsed to ZERO file records -- a broken manifest instrument, not an empty obligation",
            );
        }
        let vendor_root = self.repo.join("src").join("vendor");
        for (rel, want) in &records {
            let path = vendor_root.join(rel.replace('/', std::path::MAIN_SEPARATOR_STR));
            let got = match sha256_file(&path) {
                Ok(s) => s,
                Err(e) => self.fail(
                    "gc.vendor",
                    &format!("recorded file src/vendor/{rel} unreadable: {e} (a record without a file)"),
                ),
            };
            if &got != want {
                self.fail(
                    "gc.vendor",
                    &format!("src/vendor/{rel}: sha256 {got} != recorded {want} (vendored bytes moved)"),
                );
            }
        }
        let on_disk = walk_vendor(&vendor_root);
        for f in &on_disk {
            if !records.iter().any(|(rel, _)| rel == f) {
                self.fail(
                    "gc.vendor",
                    &format!("src/vendor/{f} has NO src/VENDORED.md record (a file without a record)"),
                );
            }
        }
        let header = match std::fs::read_to_string(vendor_root.join("gcinterface").join("gcinterface.h")) {
            Ok(t) => t,
            Err(e) => self.fail("gc.vendor", &format!("cannot read vendored gcinterface.h: {e}")),
        };
        for (name, want) in [
            ("GC_INTERFACE_MAJOR_VERSION", GC_INTERFACE_MAJOR),
            ("GC_INTERFACE_MINOR_VERSION", GC_INTERFACE_MINOR),
            ("EE_INTERFACE_MAJOR_VERSION", EE_INTERFACE_MAJOR),
        ] {
            match extract_define(&header, name) {
                Some(got) if got == want => {}
                Some(got) => self.fail(
                    "gc.vendor",
                    &format!("{name} = {got} in the vendored header, pinned {want} -- the interface tuple moved without a re-pin"),
                ),
                None => self.fail("gc.vendor", &format!("{name} not found in the vendored header")),
            }
        }
        self.results.emit(
            "{\"schema\":1,\"type\":\"case\",\"gate\":\"gc\",\"case\":\"gc.vendor\",\"verdict\":\"pass\",\"exit\":null,\"exit_want\":null,\"control\":\"positive\",\"image\":null}",
        );
        self.results.tally("pass", "positive");
        println!(
            "== case gc.vendor: PASS ({} vendored files sha-verified both directions; version tuple {{{GC_INTERFACE_MAJOR},{GC_INTERFACE_MINOR}}} + EE major {EE_INTERFACE_MAJOR} in-header) ==",
            records.len()
        );
    }

    /// Compile one source with the compiler of record into the gate's obj dir;
    /// on failure the stderr log is echoed and the gate fails.
    fn compile(&mut self, gxx: &ToolCmd, case: &str, src_rel: &str, extra: &[&str], out_rel: &str, class: Class) {
        let stdout_p = self.out.join(format!("{}.gxx.stdout", Path::new(out_rel).file_stem().unwrap().to_string_lossy()));
        let stderr_p = self.out.join(format!("{}.gxx.stderr", Path::new(out_rel).file_stem().unwrap().to_string_lossy()));
        let mut args: Vec<String> = cxx_flags(class).iter().map(|s| s.to_string()).collect();
        args.extend(extra.iter().map(|s| s.to_string()));
        args.push(src_rel.to_string());
        args.push("-o".to_string());
        args.push(out_rel.to_string());
        match run_tool_bounded(&self.repo, gxx, &args, &stdout_p, &stderr_p) {
            Ok(true) => {}
            Ok(false) => {
                echo_file(&stderr_p, "");
                self.fail(case, &format!("C++ compile failed for {src_rel} (see the gxx stderr log)"));
            }
            Err(e) => self.fail(case, &e),
        }
    }

    /// Compile a build that must REFUSE: the compile must fail AND the
    /// stderr must carry the designed refusal text (a refusal for any
    /// other reason is a different defect, not the control).
    fn compile_expect_fail(&mut self, gxx: &ToolCmd, case: &str, src_rel: &str, extra: &[&str], out_rel: &str, want_err: &str) {
        let stem = Path::new(out_rel).file_stem().unwrap().to_string_lossy().into_owned();
        let stdout_p = self.out.join(format!("{stem}.gxx.stdout"));
        let stderr_p = self.out.join(format!("{stem}.gxx.stderr"));
        let mut args: Vec<String> = CXX_FLAGS.iter().map(|s| s.to_string()).collect();
        args.extend(extra.iter().map(|s| s.to_string()));
        args.push(src_rel.to_string());
        args.push("-o".to_string());
        args.push(out_rel.to_string());
        match run_tool_bounded(&self.repo, gxx, &args, &stdout_p, &stderr_p) {
            Ok(true) => self.fail(case, "the doctored build COMPILED -- the refusal guard did not fire"),
            Ok(false) => {
                let err = std::fs::read_to_string(&stderr_p).unwrap_or_default();
                if !err.contains(want_err) {
                    echo_file(&stderr_p, "");
                    self.fail(case, &format!("the build refused for the WRONG reason (stderr lacks '{want_err}')"));
                }
            }
            Err(e) => self.fail(case, &e),
        }
    }

    /// nm -u an object with the pinned toolchain's llvm-nm; sorted names.
    fn undefined_of(&mut self, facts: &RustcFacts, case: &str, obj_rel: &str) -> Vec<String> {
        let nm = match llvm_tool(facts, "llvm-nm") {
            Ok(p) => p,
            Err(e) => self.fail(case, &e),
        };
        let dump_p = self.out.join(format!("{}.nm-u", Path::new(obj_rel).file_stem().unwrap().to_string_lossy()));
        let scratch = self.out.join("nm.stderr");
        let obj_abs = self.repo.join(obj_rel.replace('/', std::path::MAIN_SEPARATOR_STR));
        let obj_arg = obj_abs.to_string_lossy().into_owned();
        let run = run_captured(&nm, &["-u", &obj_arg], &dump_p, &scratch, Duration::from_secs(60));
        if !matches!(run, Ok(ref o) if o.exit == 0) {
            self.fail(case, &format!("llvm-nm -u failed on {obj_rel}"));
        }
        let dump = std::fs::read_to_string(&dump_p).unwrap_or_default();
        parse_undefined(&dump)
    }

    /// The object's defined external names, as the symbol tool prints
    /// them with `--defined-only --extern-only`.
    fn defined_extern_of(&mut self, facts: &RustcFacts, case: &str, obj_rel: &str) -> Vec<String> {
        let nm = match llvm_tool(facts, "llvm-nm") {
            Ok(p) => p,
            Err(e) => self.fail(case, &e),
        };
        let dump_p = self.out.join(format!("{}.nm-def", Path::new(obj_rel).file_stem().unwrap().to_string_lossy()));
        let scratch = self.out.join("nm.stderr");
        let obj_abs = self.repo.join(obj_rel.replace('/', std::path::MAIN_SEPARATOR_STR));
        let obj_arg = obj_abs.to_string_lossy().into_owned();
        let run = run_captured(&nm, &["--defined-only", "--extern-only", &obj_arg], &dump_p, &scratch, Duration::from_secs(60));
        if !matches!(run, Ok(ref o) if o.exit == 0) {
            self.fail(case, &format!("llvm-nm --defined-only failed on {obj_rel}"));
        }
        let dump = std::fs::read_to_string(&dump_p).unwrap_or_default();
        parse_defined_extern(&dump)
    }

    /// Link objects + the payload-feature staticlib under the payload
    /// board map; returns (sha, repo-relative path) of the ELF.
    fn link_image(&mut self, ld: &ToolCmd, case: &str, objs: &[&str], name: &str) -> (String, String) {
        let path_rel = format!("src/kernel/target/gc-gate/{name}.elf");
        let map_rel = format!("src/kernel/target/gc-gate/{name}.map");
        let ld_log = self.out.join(format!("{name}-ld.log"));
        let ld_out = self.out.join(format!("{name}-ld.stdout"));
        let mut args: Vec<String> = vec![
            "-T".to_string(),
            "src/kernel/payload-mps2-an500.x".to_string(),
            "--gc-sections".to_string(),
            format!("-Map={map_rel}"),
        ];
        args.extend(objs.iter().map(|s| s.to_string()));
        args.push("src/kernel/target/thumbv7em-none-eabihf/release/librhu_kernel.a".to_string());
        args.push("-o".to_string());
        args.push(path_rel.clone());
        match run_tool_bounded(&self.repo, ld, &args, &ld_out, &ld_log) {
            Ok(true) => {}
            Ok(false) => {
                echo_file(&ld_log, "");
                self.fail(case, &format!("image link failed for {name} (see {name}-ld.log)"));
            }
            Err(e) => self.fail(case, &e),
        }
        let elf = self.repo.join(path_rel.replace('/', std::path::MAIN_SEPARATOR_STR));
        let sha = match sha256_file(&elf) {
            Ok(s) => s,
            Err(e) => self.fail(case, &format!("cannot hash linked artifact: {e}")),
        };
        (sha, path_rel)
    }

    /// Run a linked image under QEMU on the Parity profile (nothing the
    /// parity cases pin is a count); returns (raw transcript, exit code).
    fn run_qemu(&mut self, qemu: &Path, case: &str, path_rel: &str) -> (Vec<u8>, i32) {
        self.run_qemu_profile(qemu, case, path_rel, ExecProfile::Parity)
    }

    /// Run a linked image under QEMU on the given profile. The handle-pool
    /// case runs on the Deterministic profile: its boot is a pure function
    /// of the image there, and its pins are counts.
    fn run_qemu_profile(&mut self, qemu: &Path, case: &str, path_rel: &str, profile: ExecProfile) -> (Vec<u8>, i32) {
        let elf = self.repo.join(path_rel.replace('/', std::path::MAIN_SEPARATOR_STR));
        let raw_path = self.out.join(format!("{case}.raw"));
        let stderr_path = self.out.join(format!("{case}.stderr"));
        let elf_arg = elf.to_string_lossy().into_owned();
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
        args.extend_from_slice(profile.qemu_args());
        args.push("-kernel");
        args.push(&elf_arg);
        let outcome = match run_captured(qemu, &args, &raw_path, &stderr_path, Duration::from_secs(RUN_TIMEOUT_S)) {
            Ok(o) => o,
            Err(e) => self.fail(case, &format!("{e}")),
        };
        if outcome.timed_out {
            println!("  (run killed at its timeout; exit recorded as 124 -- a hang is observed, never a pass)");
        }
        let raw = std::fs::read(&raw_path).unwrap_or_default();
        echo_file(&raw_path, "  | ");
        (raw, outcome.exit)
    }

    /// gc.A: the activation case + its doctored-version negative control.
    fn run_activation(&mut self, facts: &RustcFacts, ld: &ToolCmd, qemu: &Path) {
        println!("== case gc.A (activation) ==");

        let log = self.out.join("build-staticlib.log");
        let ok = match crate::tools::cargo_build_kernel(&self.repo, &["--lib", "--features", "payload,entropy-accept-software"], &log) {
            Ok(ok) => ok,
            Err(e) => {
                println!("rhu-gate: {e}");
                false
            }
        };
        if !ok {
            echo_file(&log, "");
            self.fail("gc.A", "kernel staticlib build error");
        }

        let bare = self.undefined_of(facts, "gc.A", "goldens/gc/skel-bare.o");
        if !imports_match(&bare, &IMPORTS_BARE) {
            self.fail(
                "gc.A",
                &format!(
                    "bare skeleton import closure moved: got [{}] (pinned {} names) -- a new or vanished import is a world change",
                    bare.join(" "),
                    IMPORTS_BARE.len()
                ),
            );
        }
        let bad = self.undefined_of(facts, "gc.A", "goldens/gc/skel-bad.o");
        if !imports_match(&bad, &IMPORTS_BARE) {
            self.fail("gc.A", "doctored skeleton import closure differs from the bare pin (the knob must change ONE constant, nothing else)");
        }
        let host = self.undefined_of(facts, "gc.A", "goldens/gc/host.o");
        if !imports_match(&host, &IMPORTS_HOST) {
            self.fail(
                "gc.A",
                &format!("activation host import closure moved: got [{}]", host.join(" ")),
            );
        }
        for (name, list) in [("skel-bare.o", &bare), ("skel-bad.o", &bad), ("host.o", &host)] {
            if let Some(hit) = forbidden_import(list) {
                self.fail("gc.A", &format!("{name} imports forbidden family symbol '{hit}'"));
            }
        }
        println!(
            "   import closure: skel {}/{} pinned, host {}/{} pinned, forbidden families absent",
            bare.len(),
            IMPORTS_BARE.len(),
            host.len(),
            IMPORTS_HOST.len()
        );

        let (sha_ok, rel_ok) = self.link_image(
            ld,
            "gc.A",
            &["goldens/gc/skel-bare.o", "goldens/gc/host.o"],
            "gcA-rhu",
        );
        let (sha_bad, rel_bad) = self.link_image(
            ld,
            "gc.A",
            &["goldens/gc/skel-bad.o", "goldens/gc/host.o"],
            "gcA-doctored-rhu",
        );

        let nm = match llvm_tool(facts, "llvm-nm") {
            Ok(p) => p,
            Err(e) => self.fail("gc.A", &e),
        };
        let elf_abs = self.repo.join(rel_ok.replace('/', std::path::MAIN_SEPARATOR_STR));
        let elf_arg = elf_abs.to_string_lossy().into_owned();
        let sym = self.out.join("gcA-rhu.nm");
        let scratch = self.out.join("gcA-rhu.tool.stderr");
        let nm_run = run_captured(&nm, &[&elf_arg], &sym, &scratch, Duration::from_secs(60));
        if !matches!(nm_run, Ok(ref o) if o.exit == 0) {
            self.fail("gc.A", "llvm-nm failed on the linked ELF");
        }
        let dump = std::fs::read(&sym).unwrap_or_default();
        for s in [
            "__managed__Main",
            "__mgc_dlopen_hook",
            "__mgc_dlsym_hook",
            "__rhu_debug_layout",
            "__rhu_sched",
            "__rhu_ticks",
            "_ZN15GCToEEInterface24GetFreeObjectMethodTableEv",
            "_ZN15GCToEEInterface9GetThreadEv",
        ] {
            if !word_sym_present(&dump, s) {
                self.fail("gc.A", &format!("symbol '{s}' absent from the linked artifact"));
            }
        }

        let (raw, code) = self.run_qemu(qemu, "gcA", &rel_ok);
        let enter_line = "[mgc-skel] GC_VersionInfo ENTER (EE passed major=0 minor=0 build=0 name=0x0)";
        let report_line = format!(
            "[mgc-skel] GC_VersionInfo reporting major={GC_INTERFACE_MAJOR} minor={GC_INTERFACE_MINOR} build=0 name=\"mgc-skeleton\""
        );
        let accept_line = format!(
            "[gc-host] handshake accepted: GC {{{GC_INTERFACE_MAJOR},{GC_INTERFACE_MINOR}}} == the pinned header constants"
        );
        let fmt_line = "[gc-host] fmt d=-42 u=3000000000 x=deadbeef lx=00123abc s=ok p=0x20000000 c=Q pct=%";
        let ok_line = "[gc-host] GC ACTIVATION OK (heap + handle manager live; first allocation deliberately not attempted)";
        if cr_count(&raw) == 0 {
            self.fail_run("gc.A", code, 0, "positive", "", &sha_ok, &rel_ok, "no CR bytes in raw capture");
        }
        for m in [
            "flavor: payload",
            fmt_line,
            enter_line,
            report_line.as_str(),
            accept_line.as_str(),
            "[mgc-skel] GC_Initialize EXIT rc=0",
            "[mgc-skel] EE probe: free-object MT shape OK",
            "[mgc-skel] call IGCHeap::Initialize -> S_OK",
            "[mgc-skel] call IGCHandleManager::Initialize -> true",
            ok_line,
            "[payload] managed Main returned 0",
        ] {
            if !contains_fixed(&raw, m) {
                self.fail_run("gc.A", code, 0, "positive", "", &sha_ok, &rel_ok, &format!("pin '{m}' not found"));
            }
        }
        if !in_order(
            &raw,
            &[enter_line, &report_line, "[mgc-skel] GC_Initialize ENTER", ok_line],
        ) {
            self.fail_run(
                "gc.A",
                code,
                0,
                "positive",
                "",
                &sha_ok,
                &rel_ok,
                "handshake order violated (the version print must precede the report, initialization, and the verdict)",
            );
        }
        for m in ["KERNEL PANIC", "EXCEPTION:", "REFUSED", "FAIL-FAST", "shape MISMATCH", "abort()"] {
            if contains_fixed(&raw, m) {
                self.fail_run("gc.A", code, 0, "positive", "", &sha_ok, &rel_ok, &format!("forbidden pattern '{m}' present"));
            }
        }
        if code != 0 {
            self.fail_run("gc.A", code, 0, "positive", "", &sha_ok, &rel_ok, &format!("exit {code}, wanted 0"));
        }
        self.raw_gca = raw;
        self.results.emit(&format!(
            "{{\"schema\":1,\"type\":\"case\",\"gate\":\"gc\",\"case\":\"gc.A\",\"verdict\":\"pass\",\"exit\":0,\"exit_want\":0,\"control\":\"positive\",\"image\":{{\"path\":\"{rel_ok}\",\"sha256\":\"{sha_ok}\"}}}}"
        ));
        self.results.tally("pass", "positive");
        println!("== case gc.A: PASS (exit 0; handshake printed before it reported; tuple == pinned constants; init-time calls live) ==");

        println!("== case gc.A-doctored-version (negative control) ==");
        let broke = "doctored GC interface major version (MGC_SKEL_BAD_VERSION)";
        let (rawb, codeb) = self.run_qemu(qemu, "gcA-doctored", &rel_bad);
        let bad_report = format!(
            "[mgc-skel] GC_VersionInfo reporting major={} minor={GC_INTERFACE_MINOR} build=0",
            GC_INTERFACE_MAJOR - 1
        );
        let refused_line = format!(
            "[gc-host] GC ACTIVATION REFUSED: reported {{{},{GC_INTERFACE_MINOR}}}, this EE requires major {GC_INTERFACE_MAJOR} minor >= {GC_INTERFACE_MINOR}",
            GC_INTERFACE_MAJOR - 1
        );
        if cr_count(&rawb) == 0 {
            self.fail_run("gc.A-doctored-version", codeb, 3, "negative", broke, &sha_bad, &rel_bad, "no CR bytes in raw capture");
        }
        for m in [enter_line, bad_report.as_str(), refused_line.as_str(), "[payload] managed Main returned 3"] {
            if !contains_fixed(&rawb, m) {
                self.fail_run(
                    "gc.A-doctored-version",
                    codeb,
                    3,
                    "negative",
                    broke,
                    &sha_bad,
                    &rel_bad,
                    &format!("control pin '{m}' not found (the refusal must be loud and attributed)"),
                );
            }
        }
        if !in_order(&rawb, &[enter_line, &refused_line]) {
            self.fail_run(
                "gc.A-doctored-version",
                codeb,
                3,
                "negative",
                broke,
                &sha_bad,
                &rel_bad,
                "the pre-handshake print must be on the wire BEFORE the refusal",
            );
        }
        for m in ["GC_Initialize ENTER", "GC ACTIVATION OK"] {
            if contains_fixed(&rawb, m) {
                self.fail_run(
                    "gc.A-doctored-version",
                    codeb,
                    3,
                    "negative",
                    broke,
                    &sha_bad,
                    &rel_bad,
                    &format!("'{m}' present -- the refusal did not stop activation"),
                );
            }
        }
        if codeb != 3 {
            self.fail_run(
                "gc.A-doctored-version",
                codeb,
                3,
                "negative",
                broke,
                &sha_bad,
                &rel_bad,
                &format!("exit {codeb}, wanted 3 (the refusal class)"),
            );
        }
        self.results.emit(&format!(
            "{{\"schema\":1,\"type\":\"case\",\"gate\":\"gc\",\"case\":\"gc.A-doctored-version\",\"verdict\":\"pass\",\"exit\":3,\"exit_want\":3,\"control\":\"negative\",\"broke\":\"{broke}\",\"image\":{{\"path\":\"{rel_bad}\",\"sha256\":\"{sha_bad}\"}}}}"
        ));
        self.results.tally("pass", "negative");
        println!("== case gc.A-doctored-version: PASS (refused loudly, exit 3, ENTER print preceded the refusal) ==");
    }

    /// gc.B: the mangle-parity probe as a standing artifact check, the
    /// init-time call-inventory pins, and the allocator live behind the
    /// interface -- plus the registered negative control (first object
    /// allocation on the bare skeleton fail-fasts).
    fn run_skeleton_alloc(&mut self, facts: &RustcFacts, ld: &ToolCmd, qemu: &Path) {
        println!("== case gc.B (skeleton inventory + probe + the allocator) ==");

        let probe_undef = self.undefined_of(facts, "gc.B", "goldens/gc/probe.o");
        if probe_undef.len() != EE_SURFACE {
            self.fail(
                "gc.B",
                &format!(
                    "probe object undefined-symbol count {} != the pinned surface {EE_SURFACE}",
                    probe_undef.len()
                ),
            );
        }
        let mut probe_names: Vec<String> = Vec::new();
        for sym in &probe_undef {
            match mangled_method_name(sym) {
                Some(n) => probe_names.push(n),
                None => self.fail(
                    "gc.B",
                    &format!("probe object import '{sym}' is not a flat GCToEEInterface method -- an alien import in the parity probe"),
                ),
            }
        }
        probe_names.sort();
        let header_text = match std::fs::read_to_string(
            self.repo.join("src").join("vendor").join("gcenv").join("gcenv.ee.h"),
        ) {
            Ok(t) => t,
            Err(e) => self.fail("gc.B", &format!("cannot read vendored gcenv.ee.h: {e}")),
        };
        let decl_names = gctoee_decl_names(&header_text);
        let probe_src = match std::fs::read_to_string(self.repo.join("src").join("gc").join("mgc_ee_probe.cpp")) {
            Ok(t) => t,
            Err(e) => self.fail("gc.B", &format!("cannot read the probe source: {e}")),
        };
        let ref_names = probe_ref_names(&probe_src);
        if decl_names != probe_names || ref_names != probe_names {
            self.fail(
                "gc.B",
                &format!(
                    "surface tie broken: header declares {} names, probe REFs {} names, object imports {} -- the three sets must be identical",
                    decl_names.len(),
                    ref_names.len(),
                    probe_names.len()
                ),
            );
        }
        for known in [
            "GetAllocContext",
            "GetBooleanConfigValue",
            "GetCurrentProcessCpuCount",
            "GetFreeObjectMethodTable",
            "GetThread",
            "StompWriteBarrier",
            "SuspendEE",
            "RestartEE",
            "GcEnumAllocContexts",
            "GcScanRoots",
            "GcStartWork",
            "BeforeGcScanRoots",
            "AfterGcScanRoots",
            "GcDone",
            "DiagGCStart",
            "DiagGCEnd",
            "EnablePreemptiveGC",
            "DisablePreemptiveGC",
        ] {
            if !probe_names.iter().any(|n| n == known) {
                self.fail("gc.B", &format!("host-served method '{known}' absent from the probe surface"));
            }
        }
        println!(
            "   probe surface: {EE_SURFACE} == header declarations == REF sites == object imports (name-level tie; the eighteen host-served methods present)"
        );

        let merged_rel = "goldens/rt-m7/rthello-merged.o";
        if self.repo.join(merged_rel.replace('/', std::path::MAIN_SEPARATOR_STR)).is_file() {
            let merged_defined = self.defined_extern_of(facts, "gc.B", merged_rel);
            if let Some(f) = seam_spelling_fault(&probe_undef, &merged_defined) {
                self.fail("gc.B", &f);
            }
            println!(
                "   the seam's spellings: {}/{EE_SURFACE} flat-interface demands defined in {merged_rel} by the probe's exact spelling",
                probe_undef.len()
            );
        } else {
            println!("   the seam's spellings: NOT TIED (the rt-m7 tier's frozen merged object is absent: {merged_rel})");
        }

        let skel_heap = self.undefined_of(facts, "gc.B", "goldens/gc/skel-heap.o");
        if !imports_match(&skel_heap, &IMPORTS_SKEL_HEAP) {
            self.fail(
                "gc.B",
                &format!(
                    "heap-live skeleton import closure moved: got [{}] (pinned {} names)",
                    skel_heap.join(" "),
                    IMPORTS_SKEL_HEAP.len()
                ),
            );
        }
        let heap = self.undefined_of(facts, "gc.B", "goldens/gc/heap.o");
        if !imports_match(&heap, &IMPORTS_HEAP) {
            self.fail(
                "gc.B",
                &format!(
                    "allocator import closure moved: got [{}] (pinned {} names)",
                    heap.join(" "),
                    IMPORTS_HEAP.len()
                ),
            );
        }
        let host_drive = self.undefined_of(facts, "gc.B", "goldens/gc/host-drive.o");
        if !imports_match(&host_drive, &IMPORTS_HOST_DRIVE) {
            self.fail(
                "gc.B",
                &format!("driving host import closure moved: got [{}]", host_drive.join(" ")),
            );
        }
        for (name, list) in [
            ("probe.o", &probe_undef),
            ("skel-heap.o", &skel_heap),
            ("heap.o", &heap),
            ("host-drive.o", &host_drive),
        ] {
            if let Some(hit) = forbidden_import(list) {
                self.fail("gc.B", &format!("{name} imports forbidden family symbol '{hit}'"));
            }
        }
        println!(
            "   import closures: skel-heap {}/{}, heap {}/{}, host-drive {}/{} pinned; forbidden families absent",
            skel_heap.len(),
            IMPORTS_SKEL_HEAP.len(),
            heap.len(),
            IMPORTS_HEAP.len(),
            host_drive.len(),
            IMPORTS_HOST_DRIVE.len()
        );

        let (sha_ok, rel_ok) = self.link_image(
            ld,
            "gc.B",
            &[
                "goldens/gc/skel-heap.o",
                "goldens/gc/heap.o",
                "goldens/gc/host-drive.o",
            ],
            "gcB-rhu",
        );
        let (sha_ctl, rel_ctl) = self.link_image(
            ld,
            "gc.B",
            &[
                "goldens/gc/skel-bare.o",
                "goldens/gc/host-drive.o",
            ],
            "gcB-bare-alloc-rhu",
        );

        let (raw, code) = self.run_qemu(qemu, "gcB", &rel_ok);
        let ok_line = "[gc-host] GC ACTIVATION OK (heap + handle manager live; the allocator drive follows)";
        let banner = "[gc-host] gc.B allocator drive: exercising IGCHeap::Alloc over the live region";
        let done_line = "[gc-host] GC ALLOCATOR EXERCISE OK (5 allocations zero-born on the live region; honest NULL at the wall)";
        let wall_line = "[mgc-heap] OOM WALL at alloc #6: size=8388608(+pad 0) flags=0x0 exceeds region (used 0 of 2048 KB) -> NULL (managed OutOfMemoryException protocol)";
        if cr_count(&raw) == 0 {
            self.fail_run("gc.B", code, 0, "positive", "", &sha_ok, &rel_ok, "no CR bytes in raw capture");
        }
        for m in [
            "flavor: payload",
            "[mgc-skel] GC_VersionInfo ENTER (EE passed major=0 minor=0 build=0 name=0x0)",
            "[gc-host] handshake accepted: GC {5,8} == the pinned header constants",
            "[mgc-skel] GC_Initialize EXIT rc=0",
            "[mgc-skel] EE probe: free-object MT shape OK",
            "[gc-host] mmap64: anon RW len=2097152 -> served from the kernel arena (page-aligned, zero-born; headroom delta OK)",
            "2048 KB via shim mmap (pre-zeroed, plug skew +4",
            "[gc-host] EE StompWriteBarrier: op=Initialize suspended=1 eph_low=~0 eph_high=~0 card+bounds present (no compiled barriers in this world; shape asserted)",
            "[mgc-heap] STOMP write barrier: Initialize bounds=[",
            ok_line,
            banner,
            "[mgc-heap] ctx[0] first sight:",
            "-> IDENTICAL (alloc #1)",
            "(size=24 flags=0x0, region use begins)",
            "[mgc-heap] first sight of flags=0x0 (alloc #1 size=24",
            "[mgc-heap] first sight of flags=0x8 (alloc #3 size=16",
            "[mgc-heap] first sight of flags=0x4 (alloc #4 size=20",
            "[mgc-heap] first sight of flags=0x1 (alloc #5 size=12",
            "[gc-host] alloc #1 size=24 flags=0x0: zero-born OK (28 bytes incl. header slot), free-object birth stored",
            "[gc-host] alloc #2 size=28 flags=0x0: zero-born OK (32 bytes incl. header slot), free-object birth stored",
            "[gc-host] alloc #2 adjacent to #1: the bump advanced exactly 24 bytes",
            "[gc-host] alloc #3 size=16 flags=0x8: zero-born OK (20 bytes incl. header slot), free-object birth stored",
            "[gc-host] alloc #3 ALIGN8: address parity 0 mod 8 OK (no pad needed at this point)",
            "[gc-host] alloc #4 size=20 flags=0x4: zero-born OK (24 bytes incl. header slot), free-object birth stored",
            "[gc-host] alloc #4 ALIGN8_BIAS: address parity 4 mod 8 OK via one 12-byte parity pad",
            "[mgc-heap] finalizable #1 registered:",
            "[gc-host] alloc #5 size=12 flags=0x1: zero-born OK (16 bytes incl. header slot), free-object birth stored",
            "[gc-host] alloc #5 GC_ALLOC_FINALIZE: registration is the allocator's duty (the census carries it)",
            "-> 2147483647 (not on heap)",
            "[gc-host] containment: heap object -> gen 0 + IsHeapPointer true; host static -> not on heap",
            "[mgc-skel] call IGCHeap::GetTotalBytesInUse -> 116",
            "[mgc-skel] call IGCHeap::GetTotalAllocatedBytes -> 112",
            "[gc-host] context books: alloc_count=5 alloc_bytes=100; heap books: in_use=116 allocated=112 OK",
            wall_line,
            "[mgc-heap] === census (OOM wall) ===",
            "[mgc-heap] allocs=5 bytes=112 pads=1 null_returns=0 max_size=28 (flags=0x0 at alloc #2)",
            "[mgc-heap] flags: finalize=1 contains_ref=0 align8=1 align8_bias=1 zero_opt=0 loh=0 poh=0",
            "[mgc-heap] finq: registered=1 lost=0 (cap=2048)",
            "[mgc-heap] alloc contexts seen=1 identity: match=6 mismatch=0",
            "[mgc-skel] handles: created=0 live=0 of 4096 (types: ws=0 wl=0 strong=0 pinned=0 var=0 rc=0 dep=0 ap=0 sr=0 com=0 wip=0 xref=0)",
            "[mgc-heap] post-wall Alloc(size=24 flags=0x0) -> NULL",
            "[gc-host] wall probe: oversize Alloc -> NULL OK; post-wall Alloc -> NULL OK",
            done_line,
            "[payload] managed Main returned 0",
        ] {
            if !contains_fixed(&raw, m) {
                self.fail_run("gc.B", code, 0, "positive", "", &sha_ok, &rel_ok, &format!("pin '{m}' not found"));
            }
        }
        if !in_order(
            &raw,
            &[
                "[mgc-skel] GC_VersionInfo ENTER",
                "[mgc-skel] GC_VersionInfo reporting",
                "[mgc-skel] GC_Initialize ENTER",
                ok_line,
            ],
        ) {
            self.fail_run("gc.B", code, 0, "positive", "", &sha_ok, &rel_ok, "activation order violated in the allocator boot");
        }
        if !in_order(
            &raw,
            &[
                "KB via shim mmap",
                "[gc-host] EE StompWriteBarrier: op=Initialize",
                "[mgc-heap] STOMP write barrier: Initialize bounds=[",
                "(size=24 flags=0x0, region use begins)",
                "[mgc-heap] OOM WALL at alloc #6",
                "[mgc-heap] === census (OOM wall) ===",
                done_line,
            ],
        ) {
            self.fail_run(
                "gc.B",
                code,
                0,
                "positive",
                "",
                &sha_ok,
                &rel_ok,
                "allocator order violated (region -> stomp -> first alloc -> wall -> census -> verdict is an ORDER claim)",
            );
        }
        for m in [
            "KERNEL PANIC",
            "EXCEPTION:",
            "REFUSED",
            "FAIL-FAST",
            "shape MISMATCH",
            "identity MISMATCH",
            "abort()",
            "DRIVE CHECK FAILED",
            "FMT OVERFLOW",
            "FMT UNSUPPORTED",
        ] {
            if contains_fixed(&raw, m) {
                self.fail_run("gc.B", code, 0, "positive", "", &sha_ok, &rel_ok, &format!("forbidden pattern '{m}' present"));
            }
        }
        let drive_union = call_union(&raw);
        if !imports_match(&drive_union, &INVENTORY_DRIVE) {
            self.fail_run(
                "gc.B",
                code,
                0,
                "positive",
                "",
                &sha_ok,
                &rel_ok,
                &format!(
                    "allocator-boot call inventory moved: got [{}] (banked {} heads)",
                    drive_union.join(" "),
                    INVENTORY_DRIVE.len()
                ),
            );
        }
        let act_union = call_union(&self.raw_gca);
        if !imports_match(&act_union, &INVENTORY_ACTIVATION) {
            self.fail_run(
                "gc.B",
                code,
                0,
                "positive",
                "",
                &sha_ok,
                &rel_ok,
                &format!(
                    "activation-boot call inventory moved: got [{}] (banked {} heads)",
                    act_union.join(" "),
                    INVENTORY_ACTIVATION.len()
                ),
            );
        }
        if code != 0 {
            self.fail_run("gc.B", code, 0, "positive", "", &sha_ok, &rel_ok, &format!("exit {code}, wanted 0"));
        }
        self.results.emit(&format!(
            "{{\"schema\":1,\"type\":\"case\",\"gate\":\"gc\",\"case\":\"gc.B\",\"verdict\":\"pass\",\"exit\":0,\"exit_want\":0,\"control\":\"positive\",\"image\":{{\"path\":\"{rel_ok}\",\"sha256\":\"{sha_ok}\"}}}}"
        ));
        self.results.tally("pass", "positive");
        println!(
            "== case gc.B: PASS (exit 0; surface tie {EE_SURFACE} three ways; inventories {} activation / {} drive; 5 allocations zero-born; honest NULL at the wall) ==",
            INVENTORY_ACTIVATION.len(),
            INVENTORY_DRIVE.len()
        );

        println!("== case gc.B-bare-alloc (negative control) ==");
        let broke = "no allocator behind the interface (bare skeleton): the first object allocation must fail-fast";
        let (rawc, codec) = self.run_qemu(qemu, "gcB-bare-alloc", &rel_ctl);
        let attempt = "[mgc-skel] IGCHeap::Alloc(ctx=0x";
        let failfast = "[mgc-skel] FAIL-FAST: first object allocation (IGCHeap::Alloc) -- bare skeleton by design: no collector behind the interface";
        let abort_line = "[gc-host] abort() -- structurally broken path, exiting 134";
        if cr_count(&rawc) == 0 {
            self.fail_run("gc.B-bare-alloc", codec, 134, "negative", broke, &sha_ctl, &rel_ctl, "no CR bytes in raw capture");
        }
        for m in [ok_line, banner, attempt, "size=24 flags=0x0)", failfast, abort_line] {
            if !contains_fixed(&rawc, m) {
                self.fail_run(
                    "gc.B-bare-alloc",
                    codec,
                    134,
                    "negative",
                    broke,
                    &sha_ctl,
                    &rel_ctl,
                    &format!("control pin '{m}' not found (the fail-fast must be loud and attributed)"),
                );
            }
        }
        if !in_order(&rawc, &[ok_line, banner, attempt, "FAIL-FAST: first object allocation", abort_line]) {
            self.fail_run(
                "gc.B-bare-alloc",
                codec,
                134,
                "negative",
                broke,
                &sha_ctl,
                &rel_ctl,
                "control order violated (activation green, then the drive, then the first-alloc fail-fast, then abort)",
            );
        }
        for m in ["[mgc-heap]", "EXERCISE OK", "managed Main returned"] {
            if contains_fixed(&rawc, m) {
                self.fail_run(
                    "gc.B-bare-alloc",
                    codec,
                    134,
                    "negative",
                    broke,
                    &sha_ctl,
                    &rel_ctl,
                    &format!("'{m}' present -- the bare skeleton must die at the first allocation, not limp on"),
                );
            }
        }
        let ctl_union = call_union(&rawc);
        if !imports_match(&ctl_union, &INVENTORY_ACTIVATION) {
            self.fail_run(
                "gc.B-bare-alloc",
                codec,
                134,
                "negative",
                broke,
                &sha_ctl,
                &rel_ctl,
                &format!("control call inventory moved: got [{}]", ctl_union.join(" ")),
            );
        }
        if codec != 134 {
            self.fail_run(
                "gc.B-bare-alloc",
                codec,
                134,
                "negative",
                broke,
                &sha_ctl,
                &rel_ctl,
                &format!("exit {codec}, wanted 134 (the abort class)"),
            );
        }
        self.results.emit(&format!(
            "{{\"schema\":1,\"type\":\"case\",\"gate\":\"gc\",\"case\":\"gc.B-bare-alloc\",\"verdict\":\"pass\",\"exit\":134,\"exit_want\":134,\"control\":\"negative\",\"broke\":\"{broke}\",\"image\":{{\"path\":\"{rel_ctl}\",\"sha256\":\"{sha_ctl}\"}}}}"
        ));
        self.results.tally("pass", "negative");
        println!("== case gc.B-bare-alloc: PASS (first allocation fail-fasted with the distinct marker, exit 134, activation green first) ==");
    }

    fn run_collect(&mut self, facts: &RustcFacts, ld: &ToolCmd, qemu: &Path) {
        println!("== case gc.C (suspension + mark over the kernel verbs) ==");

        for (name, obj_rel, want) in [
            ("skel-collect.o", "goldens/gc/skel-collect.o", &IMPORTS_SKEL_COLLECT[..]),
            ("heap-collect.o", "goldens/gc/heap-collect.o", &IMPORTS_HEAP[..]),
            ("collect.o", "goldens/gc/collect.o", &IMPORTS_COLLECT[..]),
            ("collect-break.o", "goldens/gc/collect-break.o", &IMPORTS_COLLECT_BREAK[..]),
            ("host-collect.o", "goldens/gc/host-collect.o", &IMPORTS_HOST_COLLECT[..]),
        ] {
            let got = self.undefined_of(facts, "gc.C", obj_rel);
            if !imports_match(&got, want) {
                self.fail(
                    "gc.C",
                    &format!(
                        "{name} import closure moved: got [{}] (pinned {} names)",
                        got.join(" "),
                        want.len()
                    ),
                );
            }
            if let Some(hit) = forbidden_import(&got) {
                self.fail("gc.C", &format!("{name} imports forbidden family symbol '{hit}'"));
            }
        }
        println!(
            "   import closures: skel-collect {}, heap-collect {}, collect {}, collect-break {}, host-collect {} pinned; forbidden families absent",
            IMPORTS_SKEL_COLLECT.len(),
            IMPORTS_HEAP.len(),
            IMPORTS_COLLECT.len(),
            IMPORTS_COLLECT_BREAK.len(),
            IMPORTS_HOST_COLLECT.len()
        );

        let (sha_ok, rel_ok) = self.link_image(
            ld,
            "gc.C",
            &[
                "goldens/gc/skel-collect.o",
                "goldens/gc/heap-collect.o",
                "goldens/gc/collect.o",
                "goldens/gc/host-collect.o",
            ],
            "gcC-rhu",
        );
        let (sha_ctl, rel_ctl) = self.link_image(
            ld,
            "gc.C",
            &[
                "goldens/gc/skel-collect.o",
                "goldens/gc/heap-collect.o",
                "goldens/gc/collect-break.o",
                "goldens/gc/host-collect.o",
            ],
            "gcC-break-roots-rhu",
        );

        let (raw, code) = self.run_qemu(qemu, "gcC", &rel_ok);
        let ok_line = "[gc-host] GC ACTIVATION OK (heap + handle manager live; the allocator drive follows)";
        let banner = "[gc-host] gc.C suspension+mark drive: allocation schedule, then two collections over the real interface";
        let done_line = "[gc-host] GC SUSPENSION+MARK EXERCISE OK (worker suspended/resumed x2 over the kernel verbs; census exact x2; 7 marked of 8 walked -- the pad stayed unmarked)";
        let suspend_line = "[gc-host] EE SuspendEE(reason=1): worker tid=3 suspended; ctx.sp + published slot inside stack bounds (4096 B); counter FROZEN across a sleep window";
        let restart_line = "[gc-host] EE RestartEE: worker resumed; counter ADVANCED within the bounded window";
        let scan_line = "[gc-host] EE GcScanRoots: reported 7 slots (static, null, out-of-heap, worker-stack, interior, pinned-dup, pair-static)";
        if cr_count(&raw) == 0 {
            self.fail_run("gc.C", code, 0, "positive", "", &sha_ok, &rel_ok, "no CR bytes in raw capture");
        }
        for m in [
            "flavor: payload",
            "[gc-host] handshake accepted: GC {5,8} == the pinned header constants",
            "[mgc-skel] GC_Initialize EXIT rc=0",
            "[gc-host] mmap64: anon RW len=2097152 -> served from the kernel arena (page-aligned, zero-born; headroom delta OK)",
            "2048 KB via shim mmap (pre-zeroed, plug skew +4",
            "[gc-host] mmap64: anon RW len=65536 -> served from the kernel arena (page-aligned, zero-born; headroom delta OK)",
            "[gc-host] mmap64: anon RW len=32768 -> served from the kernel arena (page-aligned, zero-born; headroom delta OK)",
            "init: mark bitmap 64 KB @",
            "mark stack 8192 entries (32 KB) @",
            ok_line,
            banner,
            "[gc-host] alloc #1 size=24 flags=0x0: zero-born OK (28 bytes incl. header slot), free-object birth stored",
            "[gc-host] alloc #2 adjacent to #1: the bump advanced exactly 24 bytes",
            "[gc-host] alloc #4 ALIGN8_BIAS: address parity 4 mod 8 OK via one 12-byte parity pad",
            "[mgc-heap] finalizable #1 registered:",
            "[gc-host] pair MT built: ContainsGCPointers baseSize=16, 1 series (offset 8, one slot) -- vendored-accessor readback OK",
            "[mgc-heap] first sight of flags=0x2 (alloc #6 size=16",
            "[gc-host] alloc #6 size=16 flags=0x2: zero-born OK (20 bytes incl. header slot), pair birth stored (ContainsGCPointers; ref field -> alloc #4)",
            "[gc-host] alloc #7 size=12 flags=0x0: zero-born OK (16 bytes incl. header slot), free-object birth stored",
            "[gc-host] containment: heap object -> gen 0 + IsHeapPointer true; host static -> not on heap",
            "[mgc-skel] call IGCHeap::GetTotalBytesInUse -> 144",
            "[mgc-skel] call IGCHeap::GetTotalAllocatedBytes -> 140",
            "[gc-host] context books: alloc_count=7 alloc_bytes=128; heap books: in_use=144 allocated=140 OK",
            "[mgc-skel] call IGCHeap::RegisterFrozenSegment[0](mem=",
            "firstObj=+4 allocated=28 commit=32 reserved=32)",
            "[gc-host] frozen segment registered: 2 objects, one frozen->heap ref edge (plain store -- no barrier covers frozen memory)",
            "[mgc-skel] handle[0] created via store: type=2 obj=0x",
            "[mgc-skel] handle[1] created via store: type=1 obj=0x",
            "[mgc-skel] handle[2] created via store/dependent: type=6 obj=0x",
            "[gc-host] handles: strong(#3) weak-long(#5) dependent(#3 -> #7) created; strong slot dereferences to alloc #3",
            "[gc-host] worker spawned: tid=3 prio=0xE0 stack=4096 (runs only while kmain sleeps)",
            "[gc-host] worker ready: stack slot published (alloc #2 parked on the worker stack)",
            "[mgc-skel] call IGCHeap::GarbageCollect(gen=2 low_mem=0 mode=0)",
            "[mgc-coll] collect #1 ENTER gen=2 (condemned=2) low_mem=0 mode=0",
            "[mgc-heap] === census (GarbageCollect #1) ===",
            "[mgc-heap] allocs=7 bytes=140 pads=1 null_returns=0 max_size=28 (flags=0x0 at alloc #2)",
            "[mgc-heap] flags: finalize=1 contains_ref=1 align8=1 align8_bias=1 zero_opt=0 loh=0 poh=0",
            "[mgc-heap] finq: registered=1 lost=0 (cap=2048)",
            "[mgc-heap] alloc contexts seen=1 identity: match=7 mismatch=0",
            "[mgc-skel] handles: created=3 live=3 of 4096 (types: ws=0 wl=1 strong=1 pinned=0 var=0 rc=0 dep=1 ap=0 sr=0 com=0 wip=0 xref=0)",
            "firstObj=+4 allocated=28 committed=32 reserved=32 live=1",
            suspend_line,
            "[mgc-coll] world SUSPENDED (SuspendEE returned; heap is exclusively ours)",
            "[gc-host] EE GcEnumAllocContexts: 2 contexts handed (kmain books, worker zeroed; combined_limit EE-zeroed when limit==0)",
            "[gc-host] EE DiagGCStart(gen=2 induced=1)",
            "[gc-host] EE GcStartWork(condemned=2 max_gen=2) (no RestrictedCallouts exist in this world)",
            "[gc-host] EE BeforeGcScanRoots(condemned=2) non-concurrent",
            "[mgc-coll] heap walk: objects=8 (allocs=7 pads=1) MATCH",
            scan_line,
            "[gc-host] EE AfterGcScanRoots(condemned=2 max_gen=2)",
            "[gc-host] EE GcDone(condemned=2)",
            "[gc-host] EE DiagGCEnd(index=1 gen=2 reason=0) non-concurrent",
            CENSUS_ROOTS_LINE,
            CENSUS_QUALS_LINE,
            CENSUS_MARK_1,
            "[mgc-coll] counters: gen0=1 gen1=1 gen2=1 total=1",
            restart_line,
            "[mgc-coll] world RESTARTED (RestartEE returned) -- collect #1 S_OK",
            "[mgc-skel] call IGCHeap::CollectionCount(gen=2) -> 1",
            "[gc-host] EE DiagGCEnd(index=2 gen=2 reason=0) non-concurrent",
            CENSUS_MARK_2,
            "[mgc-coll] counters: gen0=2 gen1=2 gen2=2 total=2",
            "world RESTARTED (RestartEE returned) -- collect #2 S_OK",
            "[mgc-skel] call IGCHeap::GetGcCount -> 2",
            done_line,
            "[payload] managed Main returned 0",
        ] {
            if !contains_fixed(&raw, m) {
                self.fail_run("gc.C", code, 0, "positive", "", &sha_ok, &rel_ok, &format!("pin '{m}' not found"));
            }
        }
        if !in_order(
            &raw,
            &[
                suspend_line,
                "[mgc-coll] world SUSPENDED",
                "[gc-host] EE GcEnumAllocContexts",
                "[gc-host] EE DiagGCStart(gen=2",
                "[gc-host] EE GcStartWork(condemned=2",
                "[gc-host] EE BeforeGcScanRoots(condemned=2",
                scan_line,
                "[gc-host] EE AfterGcScanRoots(condemned=2",
                "[gc-host] EE GcDone(condemned=2)",
                "[gc-host] EE DiagGCEnd(index=1",
                CENSUS_MARK_1,
                "[mgc-coll] counters: gen0=1",
                restart_line,
                "-- collect #1 S_OK",
            ],
        ) {
            self.fail_run(
                "gc.C",
                code,
                0,
                "positive",
                "",
                &sha_ok,
                &rel_ok,
                "callback bracket order violated (SuspendEE .. RestartEE is the required ORDER)",
            );
        }
        if !in_order(
            &raw,
            &[
                banner,
                "[gc-host] pair MT built",
                "[gc-host] alloc #6 size=16 flags=0x2",
                "in_use=144 allocated=140 OK",
                "[gc-host] frozen segment registered",
                "[gc-host] worker ready",
                "[mgc-heap] === census (GarbageCollect #1) ===",
                "-- collect #1 S_OK",
                "CollectionCount(gen=2) -> 1",
                CENSUS_MARK_2,
                "-- collect #2 S_OK",
                "GetGcCount -> 2",
                done_line,
            ],
        ) {
            self.fail_run(
                "gc.C",
                code,
                0,
                "positive",
                "",
                &sha_ok,
                &rel_ok,
                "drive order violated (schedule -> roots -> worker -> collect x2 -> verdict is an ORDER claim)",
            );
        }
        for m in [
            "KERNEL PANIC",
            "EXCEPTION:",
            "REFUSED",
            "FAIL-FAST",
            "MISMATCH",
            "abort()",
            "DRIVE CHECK FAILED",
            "COLLECT SURFACE CHECK FAILED",
            "FMT OVERFLOW",
            "FMT UNSUPPORTED",
            "EE EnablePreemptiveGC",
            "EE DisablePreemptiveGC",
            "sched_yield -> rhu_thread_yield",
            "BREAK_ROOTS",
            "OOM",
            "collect-and-retry",
            "mark stack OVERFLOW",
            "heap walk CORRUPT",
        ] {
            if contains_fixed(&raw, m) {
                self.fail_run("gc.C", code, 0, "positive", "", &sha_ok, &rel_ok, &format!("forbidden pattern '{m}' present"));
            }
        }
        let coll_union = call_union(&raw);
        if !imports_match(&coll_union, &INVENTORY_COLLECT) {
            self.fail_run(
                "gc.C",
                code,
                0,
                "positive",
                "",
                &sha_ok,
                &rel_ok,
                &format!(
                    "collection-boot call inventory moved: got [{}] (banked {} heads)",
                    coll_union.join(" "),
                    INVENTORY_COLLECT.len()
                ),
            );
        }
        if code != 0 {
            self.fail_run("gc.C", code, 0, "positive", "", &sha_ok, &rel_ok, &format!("exit {code}, wanted 0"));
        }
        self.results.emit(&format!(
            "{{\"schema\":1,\"type\":\"case\",\"gate\":\"gc\",\"case\":\"gc.C\",\"verdict\":\"pass\",\"exit\":0,\"exit_want\":0,\"control\":\"positive\",\"image\":{{\"path\":\"{rel_ok}\",\"sha256\":\"{sha_ok}\"}}}}"
        ));
        self.results.tally("pass", "positive");
        println!(
            "== case gc.C: PASS (exit 0; suspend/resume x2 over the kernel verbs; census exact x2 incl. the traced edge and the unmarked pad; bracket order held; inventory {} heads) ==",
            INVENTORY_COLLECT.len()
        );

        println!("== case gc.C-break-roots (negative control) ==");
        let broke = "ALL root scanning suppressed (MGC_BREAK_ROOTS): the census detector must bite";
        let (rawc, codec) = self.run_qemu(qemu, "gcC-break-roots", &rel_ctl);
        if cr_count(&rawc) == 0 {
            self.fail_run("gc.C-break-roots", codec, 0, "negative", broke, &sha_ctl, &rel_ctl, "no CR bytes in raw capture");
        }
        for m in [
            ok_line,
            banner,
            "[mgc-coll] BREAK_ROOTS: ALL root scanning SUPPRESSED (negative control)",
            "[mgc-coll] roots: stacks=0 handles=0 (weak_skipped=0) dep_seen=0 dep_marked=0 finq=0 frozen_objs=0 frozen_edges=0",
            "[mgc-coll] quals: interior=0 (unresolved=0) pinned=0 nulls=0 out_of_heap=0 re_marks=0 ctx_seen=2 ctx_nonzero=0",
            "[mgc-coll] mark census #1: roots=0 marked=0 marked_bytes=0 used_bytes=144 live_ratio~0% stack_hw=0 of 8192",
            "[mgc-coll] heap walk: objects=8 (allocs=7 pads=1) MATCH",
            "[mgc-heap] allocs=7 bytes=140 pads=1 null_returns=0 max_size=28 (flags=0x0 at alloc #2)",
            suspend_line,
            restart_line,
            "[mgc-coll] counters: gen0=2 gen1=2 gen2=2 total=2",
            done_line,
            "[payload] managed Main returned 0",
        ] {
            if !contains_fixed(&rawc, m) {
                self.fail_run(
                    "gc.C-break-roots",
                    codec,
                    0,
                    "negative",
                    broke,
                    &sha_ctl,
                    &rel_ctl,
                    &format!("control pin '{m}' not found (suppression must be loud, suspension must stay real)"),
                );
            }
        }
        for m in [CENSUS_ROOTS_LINE, CENSUS_QUALS_LINE, CENSUS_MARK_1, CENSUS_MARK_2] {
            if contains_fixed(&rawc, m) {
                self.fail_run(
                    "gc.C-break-roots",
                    codec,
                    0,
                    "negative",
                    broke,
                    &sha_ctl,
                    &rel_ctl,
                    &format!("positive census pin '{m}' PRESENT under BREAK_ROOTS -- the detector does not bite"),
                );
            }
        }
        let ctl_union = call_union(&rawc);
        if !imports_match(&ctl_union, &INVENTORY_COLLECT) {
            self.fail_run(
                "gc.C-break-roots",
                codec,
                0,
                "negative",
                broke,
                &sha_ctl,
                &rel_ctl,
                &format!("control call inventory moved: got [{}]", ctl_union.join(" ")),
            );
        }
        if codec != 0 {
            self.fail_run(
                "gc.C-break-roots",
                codec,
                0,
                "negative",
                broke,
                &sha_ctl,
                &rel_ctl,
                &format!("exit {codec}, wanted 0 (mark-only: nothing dies either way; the census is the detector)"),
            );
        }
        self.results.emit(&format!(
            "{{\"schema\":1,\"type\":\"case\",\"gate\":\"gc\",\"case\":\"gc.C-break-roots\",\"verdict\":\"pass\",\"exit\":0,\"exit_want\":0,\"control\":\"negative\",\"broke\":\"{broke}\",\"image\":{{\"path\":\"{rel_ctl}\",\"sha256\":\"{sha_ctl}\"}}}}"
        ));
        self.results.tally("pass", "negative");
        println!("== case gc.C-break-roots: PASS (census zeroed loudly, the positive detector bites, suspension real, exit 0) ==");
    }

    fn run_sweep(&mut self, facts: &RustcFacts, ld: &ToolCmd, qemu: &Path) {
        println!("== case gc.C-sweep (the death pipeline; the mark-only wall retired) ==");

        for (name, obj_rel, want) in [
            ("skel-sweep.o", "goldens/gc/skel-sweep.o", &IMPORTS_SKEL_SWEEP[..]),
            ("heap-sweep.o", "goldens/gc/heap-sweep.o", &IMPORTS_HEAP_SWEEP[..]),
            ("collect-sweep.o", "goldens/gc/collect-sweep.o", &IMPORTS_COLLECT_SWEEP[..]),
            ("collect-sweep-break.o", "goldens/gc/collect-sweep-break.o", &IMPORTS_COLLECT_SWEEP[..]),
            ("host-sweep.o", "goldens/gc/host-sweep.o", &IMPORTS_HOST_COLLECT[..]),
        ] {
            let got = self.undefined_of(facts, "gc.C-sweep", obj_rel);
            if !imports_match(&got, want) {
                self.fail(
                    "gc.C-sweep",
                    &format!(
                        "{name} import closure moved: got [{}] (pinned {} names)",
                        got.join(" "),
                        want.len()
                    ),
                );
            }
            if let Some(hit) = forbidden_import(&got) {
                self.fail("gc.C-sweep", &format!("{name} imports forbidden family symbol '{hit}'"));
            }
        }
        println!(
            "   import closures: skel-sweep {}, heap-sweep {}, collect-sweep {} == collect-sweep-break (EQUALITY pinned), host-sweep {} (the collect host's -- ZERO new kernel verbs); forbidden families absent",
            IMPORTS_SKEL_SWEEP.len(),
            IMPORTS_HEAP_SWEEP.len(),
            IMPORTS_COLLECT_SWEEP.len(),
            IMPORTS_HOST_COLLECT.len()
        );

        let (sha_ok, rel_ok) = self.link_image(
            ld,
            "gc.C-sweep",
            &[
                "goldens/gc/skel-sweep.o",
                "goldens/gc/heap-sweep.o",
                "goldens/gc/collect-sweep.o",
                "goldens/gc/host-sweep.o",
            ],
            "gcC-sweep-rhu",
        );
        let (sha_ctl, rel_ctl) = self.link_image(
            ld,
            "gc.C-sweep",
            &[
                "goldens/gc/skel-sweep.o",
                "goldens/gc/heap-sweep.o",
                "goldens/gc/collect-sweep-break.o",
                "goldens/gc/host-sweep.o",
            ],
            "gcC-break-weak-rhu",
        );

        let ok_line = "[gc-host] GC ACTIVATION OK (heap + handle manager live; the allocator drive follows)";
        let banner = "[gc-host] gc.C-sweep death-pipeline drive: the schedule + a death cast, then collections that actually reclaim";
        let done_line = "[gc-host] GC DEATH-PIPELINE EXERCISE OK (four partition branches + three weak classes + resurrection; drain normals-then-critical with FINALIZER_RUN honored; zero-on-reuse; the mark-only wall retired; honest NULL + grace round trip)";
        let suspend_line = "[gc-host] EE SuspendEE(reason=1): worker tid=3 suspended; ctx.sp + published slot inside stack bounds (4096 B); counter FROZEN across a sleep window";
        let restart_line = "[gc-host] EE RestartEE: worker resumed; counter ADVANCED within the bounded window";
        let scan_line = "[gc-host] EE GcScanRoots: reported 9 slots (static, null, out-of-heap, worker-stack, interior, pinned-dup, pair-static, fin-static, big-static)";

        let (raw, code) = self.run_qemu(qemu, "gcC-sweep", &rel_ok);
        if cr_count(&raw) == 0 {
            self.fail_run("gc.C-sweep", code, 0, "positive", "", &sha_ok, &rel_ok, "no CR bytes in raw capture");
        }
        for m in [
            "flavor: payload",
            "[gc-host] handshake accepted: GC {5,8} == the pinned header constants",
            "[mgc-skel] GC_Initialize EXIT rc=0",
            ok_line,
            banner,
            "[gc-host] alloc #2 adjacent to #1: the bump advanced exactly 24 bytes",
            "[gc-host] alloc #4 ALIGN8_BIAS: address parity 4 mod 8 OK via one 12-byte parity pad",
            "[gc-host] context books: alloc_count=7 alloc_bytes=128; heap books: in_use=144 allocated=128 (pads fit-internal under the sweep allocator) OK",
            "[gc-host] crit MT built (HasCriticalFinalizer, baseSize=16); WR-mirror MT built (identity-typed, baseSize=16)",
            "[gc-host] alloc #10 suppressed pre-death (SetFinalizationRun sets the FINALIZER_RUN header bit)",
            "[mgc-skel] call IGCHeap::SetFinalizationRun(0x",
            "[gc-host] alloc #11 size=16 flags=0x1: zero-born OK, critical-finalizer birth stored",
            "[gc-host] alloc #12 size=16 flags=0x1: zero-born OK, WR-mirror birth stored",
            "[gc-host] cast handles: underlying-ws(#1) ws(#13) wl(#14) dependent(#15 -> #16) created",
            "[mgc-skel] handle[3] created via store: type=0 obj=0x",
            "[mgc-skel] handle[6] created via store/dependent: type=6 obj=0x",
            scan_line,
            "[gc-host] worker spawned: tid=3 prio=0xE0 stack=4096 (runs only while kmain sleeps)",
            "[mgc-coll] collect #1 ENTER gen=2 (condemned=2) low_mem=0 mode=0",
            "[mgc-heap] === census (GarbageCollect #1) ===",
            "[mgc-heap] allocs=17 bytes=304 pads=1 null_returns=0 max_size=28 (flags=0x0 at alloc #2)",
            "[mgc-heap] flags: finalize=6 contains_ref=1 align8=1 align8_bias=1 zero_opt=0 loh=0 poh=0",
            "[mgc-heap] finq: registered_total=6 live=6 freach=0(+0 crit) lost=0 (cap=2048, bound=6)",
            "[mgc-heap] free list: runs=0 free=0 KB; reuse: allocs=0 bytes=0 KB (exact=0 split=0); walk_expected=18",
            "[mgc-heap] oom: reserve=64 KB grace=0 grace_allocs=0 collect_retries=0 oom_reports=0",
            "[mgc-heap] alloc contexts seen=1 identity: match=17 mismatch=0",
            "[mgc-skel] handles: created=7 live=7 of 4096 (types: ws=2 wl=2 strong=1 pinned=0 var=0 rc=0 dep=2 ap=0 sr=0 com=0 wip=0 xref=0)",
            suspend_line,
            "[mgc-coll] heap walk: objects=18 (model=18: live+pads+runs) MATCH",
            "[gc-host] EE EagerFinalized(the WR-mirror) -> true; underlying handle destroyed via the manager",
            "[mgc-skel] call IGCHandleManager::DestroyHandleOfType(handle[3] type=0)",
            "[gc-host] EE DiagWalkFReachableObjects (no-op parity)",
            "[gc-host] EE SyncBlockCacheWeakPtrScan (no-op parity)",
            SW_ROOTS_1,
            SW_QUALS_1,
            SW_MARK_1,
            SW_DEATH_1,
            SW_SWEEP_1,
            "[mgc-coll] counters: gen0=1 gen1=1 gen2=1 total=1",
            restart_line,
            "[mgc-coll] EnableFinalization(true): 2(+1 crit) promoted this collection",
            "[gc-host] EE EnableFinalization(work=1)",
            "[gc-host] weak probe ws(#13): slot NULL (dead=True)",
            "[gc-host] weak probe wl(#14): slot NULL (dead=True)",
            "[gc-host] dependent probe (#15 -> #16): primary NULL (cleared)",
            "[gc-host] weak-long(#5) SURVIVED collection #1 (promoted by the partition -- long weaks track resurrection)",
            "[mgc-skel] call IGCHeap::GetNumberOfFinalizable -> 3",
            "[gc-host] drain: finalizer ran for #5 (normals first)",
            "[gc-host] drain: FINALIZER_RUN honored EE-side -- popped and SKIPPED (bit cleared)",
            "[gc-host] drain: finalizer ran for #11 (the critical class drains LAST)",
            "[mgc-skel] call IGCHeap::GetNextFinalizable -> 0x0",
            "[mgc-skel] call IGCHeap::GetNumberOfFinalizable -> 0",
            SW_ROOTS_2,
            SW_MARK_2,
            SW_DEATH_2,
            SW_SWEEP_2,
            "[mgc-coll] heap walk: objects=14 (model=14: live+pads+runs) MATCH",
            "[gc-host] weak probe wl(#5) after #2: slot NULL (dead=True)",
            "[gc-host] alloc #18 size=16 flags=0x0: zero-on-reuse OK ([obj-4, obj+size-4) all zero), free-object birth stored",
            "[gc-host] reuse landed at alloc #8's old address (first-fit, split)",
            SW_MARK_3,
            SW_SWEEP_3,
            "[mgc-heap] wall: collect-and-retry (alloc #20 size=1900000 flags=0x0 free=0 KB grace=0 attempt=1/4)",
            "[mgc-coll] collect #4 ENTER gen=2 (condemned=2) low_mem=1 mode=0",
            SW_MARK_4,
            SW_SWEEP_4,
            "[gc-host] alloc #20 size=1900000 flags=0x0: zero-on-reuse OK ([obj-4, obj+size-4) all zero), free-object birth stored",
            "[gc-host] THE MARK-ONLY WALL IS RETIRED: the 1.9 MB ask collected-and-retried to an exact-fit serve at the dead object's address",
            "[mgc-heap] wall: collect-and-retry (alloc #21 size=4194304 flags=0x0 free=0 KB grace=0 attempt=1/4)",
            "[mgc-heap] wall: collect-and-retry (alloc #21 size=4194304 flags=0x0 free=1855 KB grace=0 attempt=2/4)",
            SW_MARK_5,
            "[mgc-coll] counters: gen0=8 gen1=8 gen2=8 total=8",
            "[mgc-heap] OOM at alloc #21: size=4194304 flags=0x0 -> NULL (managed protocol); headroom reserve OPEN (64 KB) for exception dispatch",
            "[mgc-heap] === census (first OOM report) ===",
            "[mgc-heap] oom: reserve=64 KB grace=1 grace_allocs=0 collect_retries=5 oom_reports=1",
            "[gc-host] oversize ask -> NULL after bounded retries (managed protocol; grace open)",
            "[mgc-heap] grace CLOSED: sweep recovered 1855 KB free (reserve 64 KB)",
            "[gc-host] final books: alloc_count=21 alloc_bytes=3800344; in_use=160 (132 live + 4 skew + 2 pads x 12) allocated=3800344 OK",
            done_line,
            "[payload] managed Main returned 0",
        ] {
            if !contains_fixed(&raw, m) {
                self.fail_run("gc.C-sweep", code, 0, "positive", "", &sha_ok, &rel_ok, &format!("pin '{m}' not found"));
            }
        }
        if !in_order(
            &raw,
            &[
                suspend_line,
                "[gc-host] EE AfterGcScanRoots(condemned=2",
                "[gc-host] EE EagerFinalized(the WR-mirror)",
                "[gc-host] EE DiagWalkFReachableObjects",
                "[gc-host] EE SyncBlockCacheWeakPtrScan",
                "[gc-host] EE GcDone(condemned=2)",
                "[gc-host] EE DiagGCEnd(index=1",
                SW_DEATH_1,
                SW_SWEEP_1,
                restart_line,
                "[gc-host] EE EnableFinalization(work=1)",
            ],
        ) {
            self.fail_run(
                "gc.C-sweep",
                code,
                0,
                "positive",
                "",
                &sha_ok,
                &rel_ok,
                "death-pipeline order violated (short-weak .. partition .. GcDhReScan .. long-weak .. sweep .. RestartEE .. EnableFinalization is the required ORDER)",
            );
        }
        if !in_order(
            &raw,
            &[
                banner,
                "[gc-host] cast handles: underlying-ws(#1)",
                "-- collect #1 S_OK",
                "[gc-host] weak probe ws(#13): slot NULL (dead=True)",
                "[gc-host] drain: finalizer ran for #11",
                "-- collect #2 S_OK",
                "[gc-host] reuse landed at alloc #8's old address",
                "[gc-host] THE MARK-ONLY WALL IS RETIRED",
                "[mgc-heap] OOM at alloc #21",
                "[mgc-heap] grace CLOSED",
                done_line,
            ],
        ) {
            self.fail_run(
                "gc.C-sweep",
                code,
                0,
                "positive",
                "",
                &sha_ok,
                &rel_ok,
                "drive order violated (cast -> collect -> death probes -> drain -> reclaim -> wall retirement -> OOM/grace is an ORDER claim)",
            );
        }
        for m in [
            "KERNEL PANIC",
            "EXCEPTION:",
            "REFUSED",
            "FAIL-FAST",
            "MISMATCH",
            "abort()",
            "DRIVE CHECK FAILED",
            "COLLECT SURFACE CHECK FAILED",
            "FMT OVERFLOW",
            "FMT UNSUPPORTED",
            "EE EnablePreemptiveGC",
            "sched_yield -> rhu_thread_yield",
            "BREAK_ROOTS",
            "BREAK_WEAK",
            "BREAK_WINDOW_FIX",
            "mark stack OVERFLOW",
            "heap walk CORRUPT",
            "reserve EXHAUSTED",
            "finalize list FULL",
            "UNEXPECTED",
            "handle reuse self-test",
        ] {
            if contains_fixed(&raw, m) {
                self.fail_run("gc.C-sweep", code, 0, "positive", "", &sha_ok, &rel_ok, &format!("forbidden pattern '{m}' present"));
            }
        }
        let sweep_union = call_union(&raw);
        if !imports_match(&sweep_union, &INVENTORY_SWEEP) {
            self.fail_run(
                "gc.C-sweep",
                code,
                0,
                "positive",
                "",
                &sha_ok,
                &rel_ok,
                &format!(
                    "sweep-boot call inventory moved: got [{}] (banked {} heads)",
                    sweep_union.join(" "),
                    INVENTORY_SWEEP.len()
                ),
            );
        }
        if code != 0 {
            self.fail_run("gc.C-sweep", code, 0, "positive", "", &sha_ok, &rel_ok, &format!("exit {code}, wanted 0"));
        }
        self.results.emit(&format!(
            "{{\"schema\":1,\"type\":\"case\",\"gate\":\"gc\",\"case\":\"gc.C-sweep\",\"verdict\":\"pass\",\"exit\":0,\"exit_want\":0,\"control\":\"positive\",\"image\":{{\"path\":\"{rel_ok}\",\"sha256\":\"{sha_ok}\"}}}}"
        ));
        self.results.tally("pass", "positive");
        println!(
            "== case gc.C-sweep: PASS (exit 0; 9 collections; deaths exact; drain ordered; wall retired; OOM+grace honest; inventory {} heads) ==",
            INVENTORY_SWEEP.len()
        );

        println!("== case gc.C-break-weak (negative control) ==");
        let broke = "weak nulling suppressed (MGC_BREAK_WEAK): the weak-visibility detectors must bite";
        let (rawc, codec) = self.run_qemu(qemu, "gcC-break-weak", &rel_ctl);
        if cr_count(&rawc) == 0 {
            self.fail_run("gc.C-break-weak", codec, 0, "negative", broke, &sha_ctl, &rel_ctl, "no CR bytes in raw capture");
        }
        for m in [
            ok_line,
            banner,
            "[mgc-coll] BREAK_WEAK: weak nulling SUPPRESSED (negative control)",
            SW_DEATH_1_BREAK,
            SW_DEATH_2_BREAK,
            SW_MARK_1,
            SW_SWEEP_1,
            SW_SWEEP_2,
            "[gc-host] weak probe ws(#13): slot STALE non-null (dead=False)",
            "[gc-host] weak probe wl(#14): slot STALE non-null (dead=False)",
            "[gc-host] dependent probe (#15 -> #16): primary STALE non-null (dead=False)",
            "[gc-host] weak probe wl(#5) after #2: slot STALE non-null (dead=False)",
            "[gc-host] drain: finalizer ran for #11 (the critical class drains LAST)",
            "[gc-host] THE MARK-ONLY WALL IS RETIRED: the 1.9 MB ask collected-and-retried to an exact-fit serve at the dead object's address",
            "[mgc-heap] grace CLOSED: sweep recovered 1855 KB free (reserve 64 KB)",
            done_line,
            "[payload] managed Main returned 0",
        ] {
            if !contains_fixed(&rawc, m) {
                self.fail_run(
                    "gc.C-break-weak",
                    codec,
                    0,
                    "negative",
                    broke,
                    &sha_ctl,
                    &rel_ctl,
                    &format!("control pin '{m}' not found (the suppression must be loud, everything else must still hold)"),
                );
            }
        }
        for m in [
            SW_DEATH_1,
            SW_DEATH_2,
            "[gc-host] weak probe ws(#13): slot NULL (dead=True)",
            "[gc-host] weak probe wl(#14): slot NULL (dead=True)",
            "[gc-host] dependent probe (#15 -> #16): primary NULL (cleared)",
            "[gc-host] weak probe wl(#5) after #2: slot NULL (dead=True)",
        ] {
            if contains_fixed(&rawc, m) {
                self.fail_run(
                    "gc.C-break-weak",
                    codec,
                    0,
                    "negative",
                    broke,
                    &sha_ctl,
                    &rel_ctl,
                    &format!("positive weak pin '{m}' PRESENT under BREAK_WEAK -- the detector does not bite"),
                );
            }
        }
        let ctl_union = call_union(&rawc);
        if !imports_match(&ctl_union, &INVENTORY_SWEEP) {
            self.fail_run(
                "gc.C-break-weak",
                codec,
                0,
                "negative",
                broke,
                &sha_ctl,
                &rel_ctl,
                &format!("control call inventory moved: got [{}]", ctl_union.join(" ")),
            );
        }
        if codec != 0 {
            self.fail_run(
                "gc.C-break-weak",
                codec,
                0,
                "negative",
                broke,
                &sha_ctl,
                &rel_ctl,
                &format!("exit {codec}, wanted 0 (nulling-only suppression: nothing else may change)"),
            );
        }
        self.results.emit(&format!(
            "{{\"schema\":1,\"type\":\"case\",\"gate\":\"gc\",\"case\":\"gc.C-break-weak\",\"verdict\":\"pass\",\"exit\":0,\"exit_want\":0,\"control\":\"negative\",\"broke\":\"{broke}\",\"image\":{{\"path\":\"{rel_ctl}\",\"sha256\":\"{sha_ctl}\"}}}}"
        ));
        self.results.tally("pass", "negative");
        println!("== case gc.C-break-weak: PASS (stale slots visible loudly, the positive detectors bite, partition/sweep unchanged, exit 0) ==");
    }

    /// gc.free-pad: a dead space of exactly 12 bytes before a live
    /// object whose header word is in use. The sweep world's objects with a
    /// host drive that lays out a 12-byte object that dies, a live 16-byte
    /// object right after it carrying a hash code in its header word, a
    /// second dead object and a live one, then two sweeps: the allocator
    /// keeps the 12-byte space as a PAD (walkable, never a list node -- its
    /// link word would be the live successor's header word), the header
    /// word reads intact after each sweep, the walk model counts the pad,
    /// and a 12-byte first fit on the second run leaves its remainder as a
    /// pad. The negative control links the heap object built with
    /// MGC_BREAK_FREE_PAD (the run linked as before): the live header word
    /// reads as the second run's address and the drive dies at its check.
    fn run_free_pad(&mut self, facts: &RustcFacts, ld: &ToolCmd, qemu: &Path) {
        println!("== case gc.free-pad (a 12-byte dead space before a live object's header word; the sweep's pad rule) ==");
        for (name, obj_rel, want) in [
            ("host-freepad.o", "goldens/gc/host-freepad.o", &IMPORTS_HOST_COLLECT[..]),
            ("heap-sweep-break-freepad.o", "goldens/gc/heap-sweep-break-freepad.o", &IMPORTS_HEAP_TUNE_BREAK[..]),
        ] {
            let got = self.undefined_of(facts, "gc.free-pad", obj_rel);
            if !imports_match(&got, want) {
                self.fail(
                    "gc.free-pad",
                    &format!("{name} import closure moved: got [{}] (pinned {} names)", got.join(" "), want.len()),
                );
            }
            if let Some(hit) = forbidden_import(&got) {
                self.fail("gc.free-pad", &format!("{name} imports forbidden family symbol '{hit}'"));
            }
        }
        println!(
            "   import closures: host-freepad {} == the collect host's (the drive consumes no new kernel verb); heap-sweep-break-freepad {} == the windows-world break heap's (the sweep heap's plus the knob's banner call); forbidden families absent",
            IMPORTS_HOST_COLLECT.len(),
            IMPORTS_HEAP_TUNE_BREAK.len()
        );
        let (sha_ok, rel_ok) = self.link_image(
            ld,
            "gc.free-pad",
            &[
                "goldens/gc/skel-sweep.o",
                "goldens/gc/heap-sweep.o",
                "goldens/gc/collect-sweep.o",
                "goldens/gc/host-freepad.o",
            ],
            "gc-freepad-rhu",
        );
        let (sha_ctl, rel_ctl) = self.link_image(
            ld,
            "gc.free-pad",
            &[
                "goldens/gc/skel-sweep.o",
                "goldens/gc/heap-sweep-break-freepad.o",
                "goldens/gc/collect-sweep.o",
                "goldens/gc/host-freepad.o",
            ],
            "gc-freepad-break-rhu",
        );
        let (raw, code) = self.run_qemu(qemu, "gc-freepad", &rel_ok);
        if cr_count(&raw) == 0 {
            self.fail_run("gc.free-pad", code, 0, "positive", "", &sha_ok, &rel_ok, "no CR bytes in raw capture");
        }
        for m in [
            "flavor: payload",
            "[gc-host] GC ACTIVATION OK (heap + handle manager live; the allocator drive follows)",
            FP_BANNER,
            FP_LAYOUT,
            "[gc-host] worker spawned: tid=3 prio=0xE0 stack=4096 (runs only while kmain sleeps)",
            "[gc-host] worker ready: stack slot published (alloc #4 parked on the worker stack)",
            "[mgc-heap] free list: runs=0 free=0 KB; reuse: allocs=0 bytes=0 KB (exact=0 split=0); walk_expected=4",
            "[mgc-heap] sweep pads: 1 (dead spaces of 12 bytes left unlinked: walkable, unusable until the next sweep coalesces them)",
            "[mgc-coll] heap walk: objects=4 (model=4: live+pads+runs) MATCH",
            "[mgc-coll] counters: gen0=1 gen1=1 gen2=1 total=1",
            FP_INTACT_1,
            FP_REUSE,
            "[mgc-coll] heap walk: objects=5 (model=5: live+pads+runs) MATCH",
            "[mgc-coll] counters: gen0=2 gen1=2 gen2=2 total=2",
            FP_INTACT_2,
            FP_DONE,
            "[payload] managed Main returned 0",
        ] {
            if !contains_fixed(&raw, m) {
                self.fail_run("gc.free-pad", code, 0, "positive", "", &sha_ok, &rel_ok, &format!("pin '{m}' not found"));
            }
        }
        if !in_order(&raw, &[FP_BANNER, FP_LAYOUT, FP_INTACT_1, FP_REUSE, FP_INTACT_2, FP_DONE]) {
            self.fail_run(
                "gc.free-pad",
                code,
                0,
                "positive",
                "",
                &sha_ok,
                &rel_ok,
                "drive order violated (layout -> the first sweep's intact reading -> the 12-byte reuse -> the second sweep's intact reading -> verdict is an ORDER claim)",
            );
        }
        for m in [
            "(CLOBBERED)",
            "DRIVE CHECK FAILED",
            "BREAK_FREE_PAD",
            "KERNEL PANIC",
            "EXCEPTION:",
            "REFUSED",
            "FAIL-FAST",
            "MISMATCH",
            "abort()",
            "heap walk CORRUPT",
            "COLLECT SURFACE CHECK FAILED",
        ] {
            if contains_fixed(&raw, m) {
                self.fail_run("gc.free-pad", code, 0, "positive", "", &sha_ok, &rel_ok, &format!("forbidden pattern '{m}' present"));
            }
        }
        let union = call_union(&raw);
        if !imports_match(&union, &INVENTORY_FREEPAD) {
            self.fail_run(
                "gc.free-pad",
                code,
                0,
                "positive",
                "",
                &sha_ok,
                &rel_ok,
                &format!("free-pad boot call inventory moved: got [{}] (banked {} heads)", union.join(" "), INVENTORY_FREEPAD.len()),
            );
        }
        if code != 0 {
            self.fail_run("gc.free-pad", code, 0, "positive", "", &sha_ok, &rel_ok, &format!("exit {code}, wanted 0"));
        }
        self.results.emit(&format!(
            "{{\"schema\":1,\"type\":\"case\",\"gate\":\"gc\",\"case\":\"gc.free-pad\",\"verdict\":\"pass\",\"exit\":0,\"exit_want\":0,\"control\":\"positive\",\"image\":{{\"path\":\"{rel_ok}\",\"sha256\":\"{sha_ok}\"}}}}"
        ));
        self.results.tally("pass", "positive");
        println!(
            "== case gc.free-pad: PASS (exit 0; the live header word intact after both sweeps; the pad counted by the walk; the 12-byte first-fit remainder a pad; inventory {} heads) ==",
            INVENTORY_FREEPAD.len()
        );

        println!("== case gc.free-pad-break (negative control) ==");
        let broke = "12-byte runs linked through the successor's header word (MGC_BREAK_FREE_PAD): the live header word must read as the next run's address and the drive must die at its check";
        let (rawc, codec) = self.run_qemu(qemu, "gc-freepad-break", &rel_ctl);
        if cr_count(&rawc) == 0 {
            self.fail_run("gc.free-pad-break", codec, 134, "negative", broke, &sha_ctl, &rel_ctl, "no CR bytes in raw capture");
        }
        for m in [
            FP_BANNER,
            FP_LAYOUT,
            "[mgc-heap] BREAK_FREE_PAD: 12-byte runs LINKED through the successor's header word (negative control)",
            "[gc-host] free-pad: the live object's header word after sweep #1 reads 0x",
            "(CLOBBERED)",
            "[gc-host] ALLOC DRIVE CHECK FAILED: the sweep wrote a free-list link into the live successor's header word",
            "abort()",
        ] {
            if !contains_fixed(&rawc, m) {
                self.fail_run(
                    "gc.free-pad-break",
                    codec,
                    134,
                    "negative",
                    broke,
                    &sha_ctl,
                    &rel_ctl,
                    &format!("control pin '{m}' not found (the knob must be loud and the clobber must be read)"),
                );
            }
        }
        for m in ["(INTACT)", FP_DONE, "sweep pads:"] {
            if contains_fixed(&rawc, m) {
                self.fail_run(
                    "gc.free-pad-break",
                    codec,
                    134,
                    "negative",
                    broke,
                    &sha_ctl,
                    &rel_ctl,
                    &format!("positive pin '{m}' PRESENT under BREAK_FREE_PAD -- the detector does not bite"),
                );
            }
        }
        let ctl_union = call_union(&rawc);
        if !imports_match(&ctl_union, &INVENTORY_FREEPAD) {
            self.fail_run(
                "gc.free-pad-break",
                codec,
                134,
                "negative",
                broke,
                &sha_ctl,
                &rel_ctl,
                &format!("control call inventory moved: got [{}]", ctl_union.join(" ")),
            );
        }
        if codec != 134 {
            self.fail_run(
                "gc.free-pad-break",
                codec,
                134,
                "negative",
                broke,
                &sha_ctl,
                &rel_ctl,
                &format!("exit {codec}, wanted 134 (the drive check's abort)"),
            );
        }
        self.results.emit(&format!(
            "{{\"schema\":1,\"type\":\"case\",\"gate\":\"gc\",\"case\":\"gc.free-pad-break\",\"verdict\":\"pass\",\"exit\":134,\"exit_want\":134,\"control\":\"negative\",\"broke\":\"{broke}\",\"image\":{{\"path\":\"{rel_ctl}\",\"sha256\":\"{sha_ctl}\"}}}}"
        ));
        self.results.tally("pass", "negative");
        println!("== case gc.free-pad-break: PASS (the live header word clobbered by the linked run's address, the drive died at its check, exit 134) ==");
    }

    /// gc.handle-pool: the handle pool under two hosted threads at once --
    /// the windows world's objects, the handle-pool host drive, the
    /// deterministic profile. The negative control links the skeleton built
    /// with MGC_BREAK_HANDLE_LOCK: the pool unlocked, the free list losing
    /// entries under two threads until the pool is exhausted and the
    /// skeleton fail-fasts.
    fn run_handle_pool(&mut self, facts: &RustcFacts, ld: &ToolCmd, qemu: &Path) {
        println!("== case gc.handle-pool (two hosted threads create and destroy handles at once; the pool's lock; the deterministic profile) ==");
        for (name, obj_rel, want) in [
            ("host-handles.o", "goldens/gc/host-handles.o", &IMPORTS_HOST_HANDLES[..]),
            ("skel-tune-break-lock.o", "goldens/gc/skel-tune-break-lock.o", &IMPORTS_SKEL_TUNE[..]),
        ] {
            let got = self.undefined_of(facts, "gc.handle-pool", obj_rel);
            if !imports_match(&got, want) {
                self.fail(
                    "gc.handle-pool",
                    &format!("{name} import closure moved: got [{}] (pinned {} names)", got.join(" "), want.len()),
                );
            }
            if let Some(hit) = forbidden_import(&got) {
                self.fail("gc.handle-pool", &format!("{name} imports forbidden family symbol '{hit}'"));
            }
        }
        println!(
            "   import closures: host-handles {} (the windows host's verbs less the activation pair, plus the clock, the thread exit and the 64-bit division); skel-tune-break-lock {} == the windows skeleton's (the knob moves no import); forbidden families absent",
            IMPORTS_HOST_HANDLES.len(),
            IMPORTS_SKEL_TUNE.len()
        );
        let (sha_ok, rel_ok) = self.link_image(
            ld,
            "gc.handle-pool",
            &[
                "goldens/gc/skel-tune.o",
                "goldens/gc/heap-tune.o",
                "goldens/gc/collect-tune.o",
                "goldens/gc/host-handles.o",
            ],
            "gc-handles-rhu",
        );
        let (sha_ctl, rel_ctl) = self.link_image(
            ld,
            "gc.handle-pool",
            &[
                "goldens/gc/skel-tune-break-lock.o",
                "goldens/gc/heap-tune.o",
                "goldens/gc/collect-tune.o",
                "goldens/gc/host-handles.o",
            ],
            "gc-handles-break-rhu",
        );
        let (raw, code) = self.run_qemu_profile(qemu, "gc-handles", &rel_ok, ExecProfile::Deterministic);
        if cr_count(&raw) == 0 {
            self.fail_run("gc.handle-pool", code, 0, "positive", "", &sha_ok, &rel_ok, "no CR bytes in raw capture");
        }
        for m in [
            "flavor: payload",
            "[gc-host] GC ACTIVATION OK (heap + handle manager live; the allocator drive follows)",
            HP_BANNER,
            HP_COUNTER_SPAWN,
            HP_SPAWN,
            HP_WORKER_0,
            HP_RAN_0,
            HP_WORKER_1,
            HP_RAN_1,
            HP_CENSUS,
            "[gc-host] EE SuspendEE(reason=1): worker tid=3 suspended; ctx.sp + published slot inside stack bounds (4096 B); counter FROZEN across a sleep window",
            "[mgc-coll] heap walk: objects=6 (model=6: live+pads+runs) MATCH",
            "[mgc-coll] counters: gen0=1 gen1=1 gen2=1 total=1",
            "[gc-host] EE RestartEE: worker resumed; counter ADVANCED within the bounded window",
            HP_DONE,
            "[payload] managed Main returned 0",
        ] {
            if !contains_fixed(&raw, m) {
                self.fail_run("gc.handle-pool", code, 0, "positive", "", &sha_ok, &rel_ok, &format!("pin '{m}' not found"));
            }
        }
        if !in_order(&raw, &[HP_BANNER, HP_SPAWN, HP_WORKER_0, HP_WORKER_1, HP_CENSUS, HP_DONE]) {
            self.fail_run(
                "gc.handle-pool",
                code,
                0,
                "positive",
                "",
                &sha_ok,
                &rel_ok,
                "drive order violated (banner -> the workers spawned -> the workers' books -> the pool's census -> verdict is an ORDER claim)",
            );
        }
        for m in [
            "aliased handle",
            "DRIVE CHECK FAILED",
            "pool exhausted",
            "BREAK_HANDLE_LOCK",
            "KERNEL PANIC",
            "EXCEPTION:",
            "REFUSED",
            "FAIL-FAST",
            "MISMATCH",
            "abort()",
            "heap walk CORRUPT",
            "COLLECT SURFACE CHECK FAILED",
        ] {
            if contains_fixed(&raw, m) {
                self.fail_run("gc.handle-pool", code, 0, "positive", "", &sha_ok, &rel_ok, &format!("forbidden pattern '{m}' present"));
            }
        }
        let union = call_union(&raw);
        if !imports_match(&union, &INVENTORY_HANDLES) {
            self.fail_run(
                "gc.handle-pool",
                code,
                0,
                "positive",
                "",
                &sha_ok,
                &rel_ok,
                &format!("handle-pool boot call inventory moved: got [{}] (banked {} heads)", union.join(" "), INVENTORY_HANDLES.len()),
            );
        }
        if code != 0 {
            self.fail_run("gc.handle-pool", code, 0, "positive", "", &sha_ok, &rel_ok, &format!("exit {code}, wanted 0"));
        }
        self.results.emit(&format!(
            "{{\"schema\":1,\"type\":\"case\",\"gate\":\"gc\",\"case\":\"gc.handle-pool\",\"verdict\":\"pass\",\"exit\":0,\"exit_want\":0,\"control\":\"positive\",\"image\":{{\"path\":\"{rel_ok}\",\"sha256\":\"{sha_ok}\"}}}}"
        ));
        self.results.tally("pass", "positive");
        println!(
            "== case gc.handle-pool: PASS (exit 0; 3200000 pool operations from two threads at once, every handle read its own object twice, the workers' books exact, the pool's census exact; the census collection's shape standard; inventory {} heads) ==",
            INVENTORY_HANDLES.len()
        );

        println!("== case gc.handle-pool-break (negative control) ==");
        let broke = "the pool lock compiled out (MGC_BREAK_HANDLE_LOCK): two threads must lose free-list entries until the pool is exhausted";
        let (rawc, codec) = self.run_qemu_profile(qemu, "gc-handles-break", &rel_ctl, ExecProfile::Deterministic);
        if cr_count(&rawc) == 0 {
            self.fail_run("gc.handle-pool-break", codec, 134, "negative", broke, &sha_ctl, &rel_ctl, "no CR bytes in raw capture");
        }
        for m in [HP_KNOB, HP_BANNER, HP_SPAWN, HP_EXHAUSTED] {
            if !contains_fixed(&rawc, m) {
                self.fail_run(
                    "gc.handle-pool-break",
                    codec,
                    134,
                    "negative",
                    broke,
                    &sha_ctl,
                    &rel_ctl,
                    &format!("control pin '{m}' not found (the knob must be loud and the pool must run dry)"),
                );
            }
        }
        for m in [HP_WORKER_0, HP_WORKER_1, HP_DONE, "(every handle read its own object, twice)"] {
            if contains_fixed(&rawc, m) {
                self.fail_run(
                    "gc.handle-pool-break",
                    codec,
                    134,
                    "negative",
                    broke,
                    &sha_ctl,
                    &rel_ctl,
                    &format!("positive pin '{m}' PRESENT under BREAK_HANDLE_LOCK -- the detector does not bite"),
                );
            }
        }
        let ctl_union = call_union(&rawc);
        if !imports_match(&ctl_union, &INVENTORY_HANDLES_BREAK) {
            self.fail_run(
                "gc.handle-pool-break",
                codec,
                134,
                "negative",
                broke,
                &sha_ctl,
                &rel_ctl,
                &format!("control call inventory moved: got [{}]", ctl_union.join(" ")),
            );
        }
        if codec != 134 {
            self.fail_run(
                "gc.handle-pool-break",
                codec,
                134,
                "negative",
                broke,
                &sha_ctl,
                &rel_ctl,
                &format!("exit {codec}, wanted 134 (the skeleton's fail-fast)"),
            );
        }
        self.results.emit(&format!(
            "{{\"schema\":1,\"type\":\"case\",\"gate\":\"gc\",\"case\":\"gc.handle-pool-break\",\"verdict\":\"pass\",\"exit\":134,\"exit_want\":134,\"control\":\"negative\",\"broke\":\"{broke}\",\"image\":{{\"path\":\"{rel_ctl}\",\"sha256\":\"{sha_ctl}\"}}}}"
        ));
        self.results.tally("pass", "negative");
        println!("== case gc.handle-pool-break: PASS (the unlocked pool lost free-list entries under two threads until it was exhausted; the skeleton fail-fasted, exit 134) ==");
    }

    /// The no-GC region's contract at the interface (gc.no-gc-region):
    /// the windows world's objects with the drive host that asks for a
    /// region of a byte, a kilobyte and the heap, reading each start's
    /// status as the refusal no_memory (never success: no budget and no
    /// suppression state exist, so nothing is claimed) and each end's,
    /// and a bare end's, as not_in_progress. The negative control links
    /// the skeleton built with MGC_BREAK_NOGC_CLAIM (the entry claims
    /// success with nothing entered): the drive's first check fails
    /// loudly and the host aborts (exit 134).
    fn run_no_gc_region(&mut self, facts: &RustcFacts, ld: &ToolCmd, qemu: &Path) {
        println!("== case gc.no-gc-region (the no-GC region's entry refused honestly, its end answering no region; the statuses read against the pinned enums) ==");
        for (name, obj_rel, want) in [
            ("host-nogc.o", "goldens/gc/host-nogc.o", &IMPORTS_HOST_NOGC[..]),
            ("skel-tune-break-nogc.o", "goldens/gc/skel-tune-break-nogc.o", &IMPORTS_SKEL_TUNE[..]),
        ] {
            let got = self.undefined_of(facts, "gc.no-gc-region", obj_rel);
            if !imports_match(&got, want) {
                self.fail(
                    "gc.no-gc-region",
                    &format!("{name} import closure moved: got [{}] (pinned {} names)", got.join(" "), want.len()),
                );
            }
            if let Some(hit) = forbidden_import(&got) {
                self.fail("gc.no-gc-region", &format!("{name} imports forbidden family symbol '{hit}'"));
            }
        }
        println!(
            "   import closures: host-nogc {} (the windows host's verbs less the worker's: no thread is spawned); skel-tune-break-nogc {} == the windows skeleton's (the knob moves no import)",
            IMPORTS_HOST_NOGC.len(),
            IMPORTS_SKEL_TUNE.len()
        );
        let (sha_ok, rel_ok) = self.link_image(
            ld,
            "gc.no-gc-region",
            &[
                "goldens/gc/skel-tune.o",
                "goldens/gc/heap-tune.o",
                "goldens/gc/collect-tune.o",
                "goldens/gc/host-nogc.o",
            ],
            "gc-nogc-rhu",
        );
        let (sha_ctl, rel_ctl) = self.link_image(
            ld,
            "gc.no-gc-region",
            &[
                "goldens/gc/skel-tune-break-nogc.o",
                "goldens/gc/heap-tune.o",
                "goldens/gc/collect-tune.o",
                "goldens/gc/host-nogc.o",
            ],
            "gc-nogc-break-rhu",
        );
        let (raw, code) = self.run_qemu(qemu, "gc-nogc", &rel_ok);
        if cr_count(&raw) == 0 {
            self.fail_run("gc.no-gc-region", code, 0, "positive", "", &sha_ok, &rel_ok, "no CR bytes in raw capture");
        }
        for m in [
            "flavor: payload",
            "[gc-host] GC ACTIVATION OK (heap + handle manager live; the allocator drive follows)",
            NG_BANNER,
            NG_START_1,
            NG_START_1024,
            NG_START_HEAP,
            NG_END,
            NG_DONE,
            "[payload] managed Main returned 0",
        ] {
            if !contains_fixed(&raw, m) {
                self.fail_run("gc.no-gc-region", code, 0, "positive", "", &sha_ok, &rel_ok, &format!("pin '{m}' not found"));
            }
        }
        if !in_order(&raw, &[NG_BANNER, NG_START_1, NG_START_1024, NG_START_HEAP, NG_DONE]) {
            self.fail_run(
                "gc.no-gc-region",
                code,
                0,
                "positive",
                "",
                &sha_ok,
                &rel_ok,
                "drive order violated (banner -> the three entries in size order -> verdict is an ORDER claim)",
            );
        }
        for m in [
            "DRIVE CHECK FAILED",
            "BREAK_NOGC_CLAIM",
            "-> success",
            "KERNEL PANIC",
            "EXCEPTION:",
            "REFUSED",
            "FAIL-FAST",
            "MISMATCH",
            "abort()",
            "heap walk CORRUPT",
        ] {
            if contains_fixed(&raw, m) {
                self.fail_run("gc.no-gc-region", code, 0, "positive", "", &sha_ok, &rel_ok, &format!("forbidden pattern '{m}' present"));
            }
        }
        let union = call_union(&raw);
        if !imports_match(&union, &INVENTORY_NOGC) {
            self.fail_run(
                "gc.no-gc-region",
                code,
                0,
                "positive",
                "",
                &sha_ok,
                &rel_ok,
                &format!("no-gc-region boot call inventory moved: got [{}] (banked {} heads)", union.join(" "), INVENTORY_NOGC.len()),
            );
        }
        if code != 0 {
            self.fail_run("gc.no-gc-region", code, 0, "positive", "", &sha_ok, &rel_ok, &format!("exit {code}, wanted 0"));
        }
        self.results.emit(&format!(
            "{{\"schema\":1,\"type\":\"case\",\"gate\":\"gc\",\"case\":\"gc.no-gc-region\",\"verdict\":\"pass\",\"exit\":0,\"exit_want\":0,\"control\":\"positive\",\"image\":{{\"path\":\"{rel_ok}\",\"sha256\":\"{sha_ok}\"}}}}"
        ));
        self.results.tally("pass", "positive");
        println!(
            "== case gc.no-gc-region: PASS (exit 0; three entries refused with no_memory in size order, each end and a bare end not_in_progress, nothing claimed; the boot's call inventory the banked {} heads) ==",
            INVENTORY_NOGC.len()
        );

        println!("== case gc.no-gc-region-break (negative control) ==");
        let broke = "the entry claims success with nothing entered (MGC_BREAK_NOGC_CLAIM): the drive's first check must fail loudly";
        let (rawc, codec) = self.run_qemu(qemu, "gc-nogc-break", &rel_ctl);
        if cr_count(&rawc) == 0 {
            self.fail_run("gc.no-gc-region-break", codec, 134, "negative", broke, &sha_ctl, &rel_ctl, "no CR bytes in raw capture");
        }
        for m in [NG_KNOB, NG_BANNER, NG_CLAIMED, NG_CHECK_FAILED] {
            if !contains_fixed(&rawc, m) {
                self.fail_run(
                    "gc.no-gc-region-break",
                    codec,
                    134,
                    "negative",
                    broke,
                    &sha_ctl,
                    &rel_ctl,
                    &format!("control pin '{m}' not found (the knob must be loud and the claim must be caught)"),
                );
            }
        }
        for m in [NG_START_1, NG_END, NG_DONE] {
            if contains_fixed(&rawc, m) {
                self.fail_run(
                    "gc.no-gc-region-break",
                    codec,
                    134,
                    "negative",
                    broke,
                    &sha_ctl,
                    &rel_ctl,
                    &format!("positive pin '{m}' PRESENT under BREAK_NOGC_CLAIM -- the detector does not bite"),
                );
            }
        }
        let ctl_union = call_union(&rawc);
        if !imports_match(&ctl_union, &INVENTORY_NOGC_BREAK) {
            self.fail_run(
                "gc.no-gc-region-break",
                codec,
                134,
                "negative",
                broke,
                &sha_ctl,
                &rel_ctl,
                &format!("control call inventory moved: got [{}]", ctl_union.join(" ")),
            );
        }
        if codec != 134 {
            self.fail_run(
                "gc.no-gc-region-break",
                codec,
                134,
                "negative",
                broke,
                &sha_ctl,
                &rel_ctl,
                &format!("exit {codec}, wanted 134 (the host's abort)"),
            );
        }
        self.results.emit(&format!(
            "{{\"schema\":1,\"type\":\"case\",\"gate\":\"gc\",\"case\":\"gc.no-gc-region-break\",\"verdict\":\"pass\",\"exit\":134,\"exit_want\":134,\"control\":\"negative\",\"broke\":\"{broke}\",\"image\":{{\"path\":\"{rel_ctl}\",\"sha256\":\"{sha_ctl}\"}}}}"
        ));
        self.results.tally("pass", "negative");
        println!("== case gc.no-gc-region-break: PASS (the claimed entry was caught by the drive's first check; the host aborted, exit 134) ==");
    }

    fn run_tune(&mut self, facts: &RustcFacts, ld: &ToolCmd, qemu: &Path) {
        println!("== case gc.D (alloc-context windows + the futex-backed GC-done event) ==");

        for (name, obj_rel, want) in [
            ("skel-tune.o", "goldens/gc/skel-tune.o", &IMPORTS_SKEL_TUNE[..]),
            ("heap-tune.o", "goldens/gc/heap-tune.o", &IMPORTS_HEAP_SWEEP[..]),
            ("heap-tune-break.o", "goldens/gc/heap-tune-break.o", &IMPORTS_HEAP_TUNE_BREAK[..]),
            ("collect-tune.o", "goldens/gc/collect-tune.o", &IMPORTS_COLLECT_TUNE[..]),
            ("host-tune.o", "goldens/gc/host-tune.o", &IMPORTS_HOST_TUNE[..]),
        ] {
            let got = self.undefined_of(facts, "gc.D", obj_rel);
            if !imports_match(&got, want) {
                self.fail(
                    "gc.D",
                    &format!(
                        "{name} import closure moved: got [{}] (pinned {} names)",
                        got.join(" "),
                        want.len()
                    ),
                );
            }
            if let Some(hit) = forbidden_import(&got) {
                self.fail("gc.D", &format!("{name} imports forbidden family symbol '{hit}'"));
            }
        }
        println!(
            "   import closures: skel-tune {} (sched_yield left with the retired yield-poll; the futex pair joined), heap-tune {} (== heap-sweep), heap-tune-break {} (+puts, the banner fold), collect-tune {}, host-tune {} (first image-tier consumption of the futex AND activation pairs); forbidden families absent",
            IMPORTS_SKEL_TUNE.len(),
            IMPORTS_HEAP_SWEEP.len(),
            IMPORTS_HEAP_TUNE_BREAK.len(),
            IMPORTS_COLLECT_TUNE.len(),
            IMPORTS_HOST_TUNE.len()
        );

        let (sha_ok, rel_ok) = self.link_image(
            ld,
            "gc.D",
            &[
                "goldens/gc/skel-tune.o",
                "goldens/gc/heap-tune.o",
                "goldens/gc/collect-tune.o",
                "goldens/gc/host-tune.o",
            ],
            "gcD-rhu",
        );
        let (sha_ctl, rel_ctl) = self.link_image(
            ld,
            "gc.D",
            &[
                "goldens/gc/skel-tune.o",
                "goldens/gc/heap-tune-break.o",
                "goldens/gc/collect-tune.o",
                "goldens/gc/host-tune.o",
            ],
            "gcD-break-window-fix-rhu",
        );

        let ok_line = "[gc-host] GC ACTIVATION OK (heap + handle manager live; the allocator drive follows)";
        let banner = "[gc-host] gc.D windows+event drive: the same payload under window posture, the GC-done futex, and the activation break-out";
        let done_line = "[gc-host] GC WINDOWS+EVENT EXERCISE OK (windows both sources, three fix sites, refunds exact; the GC-done event slept and woke over the kernel futex; an activation broke a blocked wait -- the RHU_EINTR contract; handle slots reused)";
        let suspend_line = "[gc-host] EE SuspendEE(reason=1): worker tid=3 suspended; ctx.sp + published slot inside stack bounds (4096 B); counter FROZEN across a sleep window";
        let restart_line = "[gc-host] EE RestartEE: worker resumed; counter ADVANCED within the bounded window";
        let parking1 = "[gc-host] waiter: parking on the GC-done event (epoch 1)";
        let woken1 = "[gc-host] waiter: woken (epoch 1)";
        let parking2 = "[gc-host] waiter: parking on the GC-done event (epoch 2)";
        let woken2 = "[gc-host] waiter: woken (epoch 2)";

        let (raw, code) = self.run_qemu(qemu, "gcD", &rel_ok);
        if cr_count(&raw) == 0 {
            self.fail_run("gc.D", code, 0, "positive", "", &sha_ok, &rel_ok, "no CR bytes in raw capture");
        }
        for &m in TUNE_DRIVE_PINS {
            if !contains_fixed(&raw, m) {
                self.fail_run("gc.D", code, 0, "positive", "", &sha_ok, &rel_ok, &format!("pin '{m}' not found"));
            }
        }
        if !in_order(
            &raw,
            &[
                "[gc-host] epoch signal: woken 1 waiter",
                "[mgc-coll] collect #1 ENTER gen=2",
                "[mgc-skel] call IGCHeap::ResetWaitForGCEvent",
                parking1,
                suspend_line,
                SW_DEATH_1,
                "[mgc-skel] call IGCHeap::SetWaitForGCEvent",
                woken1,
                restart_line,
                "-- collect #1 S_OK",
            ],
        ) {
            self.fail_run(
                "gc.D",
                code,
                0,
                "positive",
                "",
                &sha_ok,
                &rel_ok,
                "epoch-1 order violated (arm inside SuspendEE -> park -> death pipeline -> Set+wake inside RestartEE -> woken is the ORDER claim)",
            );
        }
        if !in_order(
            &raw,
            &[
                parking2,
                "[gc-host] activation sent to the futex-blocked waiter (tid=4)",
                "[gc-host] activation handler ran (delivered on the target's own stack)",
                woken2,
                "[gc-host] EINTR by elimination",
            ],
        ) {
            self.fail_run(
                "gc.D",
                code,
                0,
                "positive",
                "",
                &sha_ok,
                &rel_ok,
                "epoch-2 order violated (park -> activation send -> handler -> woken is the EINTR contract's order)",
            );
        }
        match (find_pos(&raw, parking2), find_pos(&raw, woken2)) {
            (Some(p1), Some(p2)) if p1 < p2 => {
                let slice = &raw[p1..p2];
                if contains_fixed(slice, "SetWaitForGCEvent") || contains_fixed(slice, "futex_wake") {
                    self.fail_run(
                        "gc.D",
                        code,
                        0,
                        "positive",
                        "",
                        &sha_ok,
                        &rel_ok,
                        "a Set/wake appears between epoch-2 parking and waking -- the EINTR elimination does not hold",
                    );
                }
            }
            _ => self.fail_run("gc.D", code, 0, "positive", "", &sha_ok, &rel_ok, "epoch-2 markers missing for the elimination slice"),
        }
        for m in [
            "KERNEL PANIC",
            "EXCEPTION:",
            "REFUSED",
            "FAIL-FAST",
            "MISMATCH",
            "abort()",
            "DRIVE CHECK FAILED",
            "COLLECT SURFACE CHECK FAILED",
            "FMT OVERFLOW",
            "FMT UNSUPPORTED",
            "EE EnablePreemptiveGC",
            "sched_yield -> rhu_thread_yield",
            "BREAK_ROOTS",
            "BREAK_WEAK",
            "BREAK_WINDOW_FIX",
            "mark stack OVERFLOW",
            "heap walk CORRUPT",
            "window walk CORRUPT",
            "reserve EXHAUSTED",
            "finalize list FULL",
            "UNEXPECTED",
            "OOM",
            "collect-and-retry",
            "grace CLOSED",
        ] {
            if contains_fixed(&raw, m) {
                self.fail_run("gc.D", code, 0, "positive", "", &sha_ok, &rel_ok, &format!("forbidden pattern '{m}' present"));
            }
        }
        let tune_union = call_union(&raw);
        if !imports_match(&tune_union, &INVENTORY_TUNE) {
            self.fail_run(
                "gc.D",
                code,
                0,
                "positive",
                "",
                &sha_ok,
                &rel_ok,
                &format!(
                    "tune-boot call inventory moved: got [{}] (banked {} heads)",
                    tune_union.join(" "),
                    INVENTORY_TUNE.len()
                ),
            );
        }
        if code != 0 {
            self.fail_run("gc.D", code, 0, "positive", "", &sha_ok, &rel_ok, &format!("exit {code}, wanted 0"));
        }
        self.results.emit(&format!(
            "{{\"schema\":1,\"type\":\"case\",\"gate\":\"gc\",\"case\":\"gc.D\",\"verdict\":\"pass\",\"exit\":0,\"exit_want\":0,\"control\":\"positive\",\"image\":{{\"path\":\"{rel_ok}\",\"sha256\":\"{sha_ok}\"}}}}"
        ));
        self.results.tally("pass", "positive");
        println!(
            "== case gc.D: PASS (exit 0; windows both sources with three fix sites; the GC-done futex slept/woke; EINTR by elimination; death line posture-invariant; inventory {} heads) ==",
            INVENTORY_TUNE.len()
        );

        println!("== case gc.D-break-window-fix (negative control) ==");
        let broke = "window plug+count suppressed (MGC_BREAK_WINDOW_FIX): the heap-walk cross-check must die loudly";
        let (rawc, codec) = self.run_qemu(qemu, "gcD-break-window-fix", &rel_ctl);
        if cr_count(&rawc) == 0 {
            self.fail_run("gc.D-break-window-fix", codec, 134, "negative", broke, &sha_ctl, &rel_ctl, "no CR bytes in raw capture");
        }
        for m in [
            ok_line,
            banner,
            "[mgc-heap] BREAK_WINDOW_FIX: window plug+count SUPPRESSED (negative control)",
            TN_BOOKS_BREAK,
            "[gc-host] waiter spawned: tid=4 prio=0xC0 stack=4096 (parks on a go futex between epochs)",
            "[mgc-skel] call IGCHeap::ResetWaitForGCEvent",
            parking1,
            suspend_line,
            "[mgc-coll] heap walk CORRUPT at 0x",
            "[mgc-coll] FAIL-FAST: heap walk corrupt (size formula vs layout)",
            "[mgc-heap] === census (collector fail-fast) ===",
            "[gc-host] abort() -- structurally broken path, exiting 134",
        ] {
            if !contains_fixed(&rawc, m) {
                self.fail_run(
                    "gc.D-break-window-fix",
                    codec,
                    134,
                    "negative",
                    broke,
                    &sha_ctl,
                    &rel_ctl,
                    &format!("control pin '{m}' not found (the corruption must be caught loudly AFTER real suspension + event-arm)"),
                );
            }
        }
        for m in [
            TN_BOOKS,
            TN_MARK_1,
            SW_DEATH_1,
            TN_SWEEP_1,
            woken1,
            restart_line,
            done_line,
        ] {
            if contains_fixed(&rawc, m) {
                self.fail_run(
                    "gc.D-break-window-fix",
                    codec,
                    134,
                    "negative",
                    broke,
                    &sha_ctl,
                    &rel_ctl,
                    &format!("positive pin '{m}' PRESENT under BREAK_WINDOW_FIX -- the detector does not bite"),
                );
            }
        }
        let ctl_union = call_union(&rawc);
        if !imports_match(&ctl_union, &INVENTORY_TUNE_BREAK) {
            self.fail_run(
                "gc.D-break-window-fix",
                codec,
                134,
                "negative",
                broke,
                &sha_ctl,
                &rel_ctl,
                &format!("control call inventory moved: got [{}] (banked {} heads)", ctl_union.join(" "), INVENTORY_TUNE_BREAK.len()),
            );
        }
        if codec != 134 {
            self.fail_run(
                "gc.D-break-window-fix",
                codec,
                134,
                "negative",
                broke,
                &sha_ctl,
                &rel_ctl,
                &format!("exit {codec}, wanted 134 (the abort class: the walk cross-check is the deterministic detector)"),
            );
        }
        self.results.emit(&format!(
            "{{\"schema\":1,\"type\":\"case\",\"gate\":\"gc\",\"case\":\"gc.D-break-window-fix\",\"verdict\":\"pass\",\"exit\":134,\"exit_want\":134,\"control\":\"negative\",\"broke\":\"{broke}\",\"image\":{{\"path\":\"{rel_ctl}\",\"sha256\":\"{sha_ctl}\"}}}}"
        ));
        self.results.tally("pass", "negative");
        println!("== case gc.D-break-window-fix: PASS (heap walk died loudly at the un-plugged gap, exit 134, suspension + event-arm real first, the positive detector bites) ==");
    }

    fn run_row(&mut self, facts: &RustcFacts, ld: &ToolCmd, qemu: &Path) {
        println!("== case gc.E (the board knob table: rows declared per board, asserted in-artifact) ==");

        let hdr_rel = "src/gc/host/mgc_board_rows.h";
        let hdr_text = match std::fs::read_to_string(self.repo.join(hdr_rel.replace('/', std::path::MAIN_SEPARATOR_STR))) {
            Ok(t) => t,
            Err(e) => self.fail("gc.E", &format!("cannot read {hdr_rel}: {e}")),
        };
        let rows = match parse_board_rows(&hdr_text) {
            Ok(r) => r,
            Err(e) => self.fail("gc.E", &e),
        };
        if rows.len() != 6 {
            self.fail("gc.E", &format!("expected 6 declared rows, parsed {}", rows.len()));
        }
        if let Err(e) = rows_uniform(&rows) {
            self.fail("gc.E", &e);
        }
        println!("   knob table (census-derived; {} rows parsed from {hdr_rel}, each checked against its payload map + board module):", rows.len());
        for row in &rows {
            let map_rel = format!("src/kernel/payload-{}.x", row.board);
            let map_text = match std::fs::read_to_string(self.repo.join(map_rel.replace('/', std::path::MAIN_SEPARATOR_STR))) {
                Ok(t) => t,
                Err(e) => self.fail("gc.E", &format!("cannot read {map_rel}: {e}")),
            };
            let (base, len, amin) = match parse_map_geometry(&map_text) {
                Ok(g) => g,
                Err(e) => self.fail("gc.E", &format!("{map_rel}: {e}")),
            };
            let brd_rel = format!("src/kernel/src/board/{}.rs", row.board.replace('-', "_"));
            let brd_text = match std::fs::read_to_string(self.repo.join(brd_rel.replace('/', std::path::MAIN_SEPARATOR_STR))) {
                Ok(t) => t,
                Err(e) => self.fail("gc.E", &format!("cannot read {brd_rel}: {e}")),
            };
            let kheap = match parse_kheap_const(&brd_text) {
                Ok(v) => v,
                Err(e) => self.fail("gc.E", &format!("{brd_rel}: {e}")),
            };
            if let Err(e) = row_check(row, base, len, amin, kheap) {
                self.fail("gc.E", &e);
            }
            println!(
                "   row {}: {}/{} region={} cover={} kheap={} stack={}/{} handles={} foh={} markstack={} finq={} reserve={} (DATA 0x{:x}+{}K floor {}K)",
                row.board, row.profile, row.basis, row.region, row.cover, row.kheap,
                row.stack_default, row.stack_clamp, row.handles, row.foh,
                row.markstack, row.finq, row.reserve, base, len / 1024, amin / 1024
            );
        }

        for (name, obj_rel, want) in [
            ("skel-row.o", "goldens/gc/skel-row.o", &IMPORTS_SKEL_TUNE[..]),
            ("heap-row.o", "goldens/gc/heap-row.o", &IMPORTS_HEAP_SWEEP[..]),
            ("heap-row-break-cover.o", "goldens/gc/heap-row-break-cover.o", &IMPORTS_HEAP_SWEEP[..]),
            ("collect-row.o", "goldens/gc/collect-row.o", &IMPORTS_COLLECT_TUNE[..]),
            ("host-row.o", "goldens/gc/host-row.o", &IMPORTS_HOST_ROW[..]),
            ("skel-row-an505.o", "goldens/gc/skel-row-an505.o", &IMPORTS_SKEL_TUNE[..]),
            ("heap-row-an505.o", "goldens/gc/heap-row-an505.o", &IMPORTS_HEAP_SWEEP[..]),
            ("collect-row-an505.o", "goldens/gc/collect-row-an505.o", &IMPORTS_COLLECT_TUNE[..]),
            ("heap-row-u5a5.o", "goldens/gc/heap-row-u5a5.o", &IMPORTS_HEAP_SWEEP[..]),
        ] {
            let got = self.undefined_of(facts, "gc.E", obj_rel);
            if !imports_match(&got, want) {
                self.fail(
                    "gc.E",
                    &format!(
                        "{name} import closure moved: got [{}] (pinned {} names)",
                        got.join(" "),
                        want.len()
                    ),
                );
            }
            if let Some(hit) = forbidden_import(&got) {
                self.fail("gc.E", &format!("{name} imports forbidden family symbol '{hit}'"));
            }
        }
        println!(
            "   import closures: skel-row {} / heap-row {} / collect-row {} (each == its tune-world set), heap-row-break-cover {} (== heap-row: a CONSTANT-only doctor), host-row {} (the tune host + the six map layout symbols); forbidden families absent",
            IMPORTS_SKEL_TUNE.len(),
            IMPORTS_HEAP_SWEEP.len(),
            IMPORTS_COLLECT_TUNE.len(),
            IMPORTS_HEAP_SWEEP.len(),
            IMPORTS_HOST_ROW.len()
        );
        println!(
            "   the mps2-an505 row's Cortex-M33 objects: skel-row-an505 {} / heap-row-an505 {} / collect-row-an505 {} pinned against the same closures",
            IMPORTS_SKEL_TUNE.len(),
            IMPORTS_HEAP_SWEEP.len(),
            IMPORTS_COLLECT_TUNE.len()
        );
        println!("   the U5A5 row's own Cortex-M33 heap object: heap-row-u5a5 {} pinned against the same closure", IMPORTS_HEAP_SWEEP.len());

        let (sha_ok, rel_ok) = self.link_image(
            ld,
            "gc.E",
            &[
                "goldens/gc/skel-row.o",
                "goldens/gc/heap-row.o",
                "goldens/gc/collect-row.o",
                "goldens/gc/host-row.o",
            ],
            "gcE-rhu",
        );
        let (sha_ctl, rel_ctl) = self.link_image(
            ld,
            "gc.E",
            &[
                "goldens/gc/skel-row.o",
                "goldens/gc/heap-row-break-cover.o",
                "goldens/gc/collect-row.o",
                "goldens/gc/host-row.o",
            ],
            "gcE-break-cover-rhu",
        );

        let (raw, code) = self.run_qemu(qemu, "gcE", &rel_ok);
        if cr_count(&raw) == 0 {
            self.fail_run("gc.E", code, 0, "positive", "", &sha_ok, &rel_ok, "no CR bytes in raw capture");
        }
        for m in [
            ROW_CENSUS,
            ROW_SIZING,
            ROW_CONTAIN_PREFIX,
            ROW_CONTAIN_VERDICT,
            ROW_FIT_PREFIX,
            ROW_FIT_VERDICT,
        ] {
            if !contains_fixed(&raw, m) {
                self.fail_run("gc.E", code, 0, "positive", "", &sha_ok, &rel_ok, &format!("row pin '{m}' not found"));
            }
        }
        if !in_order(
            &raw,
            &[
                ROW_CENSUS,
                "[gc-host] handshake accepted",
                ROW_CONTAIN_VERDICT,
                ROW_FIT_VERDICT,
                "[gc-host] GC ACTIVATION OK",
            ],
        ) {
            self.fail_run(
                "gc.E",
                code,
                0,
                "positive",
                "",
                &sha_ok,
                &rel_ok,
                "row order violated (census -> handshake -> containment -> window fit -> activation OK is the declaration-then-proof order)",
            );
        }
        match parse_cover_line(&raw) {
            Ok((floor, span, cards, of)) => {
                if of != 4160 {
                    self.fail_run("gc.E", code, 0, "positive", "", &sha_ok, &rel_ok, &format!("cover buffer total {of}, wanted 4160 (the row cover did not reach the heap TU)"));
                }
                if cards != span + 64 || cards > of {
                    self.fail_run("gc.E", code, 0, "positive", "", &sha_ok, &rel_ok, &format!("cover arithmetic broken: span={span} cards={cards} of={of}"));
                }
                if floor != "0x20000000" {
                    self.fail_run("gc.E", code, 0, "positive", "", &sha_ok, &rel_ok, &format!("heap cover floor {floor} != the host containment floor 0x20000000"));
                }
            }
            Err(e) => self.fail_run("gc.E", code, 0, "positive", "", &sha_ok, &rel_ok, &e),
        }
        for &m in TUNE_DRIVE_PINS {
            if !contains_fixed(&raw, m) {
                self.fail_run("gc.E", code, 0, "positive", "", &sha_ok, &rel_ok, &format!("carried tune pin '{m}' not found"));
            }
        }
        if !in_order(
            &raw,
            &[
                "[gc-host] epoch signal: woken 1 waiter",
                "[mgc-coll] collect #1 ENTER gen=2",
                "[mgc-skel] call IGCHeap::ResetWaitForGCEvent",
                "[gc-host] waiter: parking on the GC-done event (epoch 1)",
                "[gc-host] EE SuspendEE(reason=1): worker tid=3 suspended; ctx.sp + published slot inside stack bounds (4096 B); counter FROZEN across a sleep window",
                SW_DEATH_1,
                "[mgc-skel] call IGCHeap::SetWaitForGCEvent",
                "[gc-host] waiter: woken (epoch 1)",
                "[gc-host] EE RestartEE: worker resumed; counter ADVANCED within the bounded window",
                "-- collect #1 S_OK",
            ],
        ) {
            self.fail_run("gc.E", code, 0, "positive", "", &sha_ok, &rel_ok, "epoch-1 order violated under the row (the carried weave)");
        }
        if !in_order(
            &raw,
            &[
                "[gc-host] waiter: parking on the GC-done event (epoch 2)",
                "[gc-host] activation sent to the futex-blocked waiter (tid=4)",
                "[gc-host] activation handler ran (delivered on the target's own stack)",
                "[gc-host] waiter: woken (epoch 2)",
                "[gc-host] EINTR by elimination",
            ],
        ) {
            self.fail_run("gc.E", code, 0, "positive", "", &sha_ok, &rel_ok, "epoch-2 order violated under the row (the carried weave)");
        }
        match (
            find_pos(&raw, "[gc-host] waiter: parking on the GC-done event (epoch 2)"),
            find_pos(&raw, "[gc-host] waiter: woken (epoch 2)"),
        ) {
            (Some(p1), Some(p2)) if p1 < p2 => {
                let slice = &raw[p1..p2];
                if contains_fixed(slice, "SetWaitForGCEvent") || contains_fixed(slice, "futex_wake") {
                    self.fail_run("gc.E", code, 0, "positive", "", &sha_ok, &rel_ok, "a Set/wake appears in the epoch-2 parked slice under the row");
                }
            }
            _ => self.fail_run("gc.E", code, 0, "positive", "", &sha_ok, &rel_ok, "epoch-2 markers missing for the elimination slice"),
        }
        for m in ["KERNEL PANIC", "EXCEPTION:", "REFUSED", "FAIL-FAST", "abort()", "DRIVE CHECK FAILED", "mark stack OVERFLOW", "heap walk CORRUPT", "OOM"] {
            if contains_fixed(&raw, m) {
                self.fail_run("gc.E", code, 0, "positive", "", &sha_ok, &rel_ok, &format!("forbidden pattern '{m}' present"));
            }
        }
        let row_union = call_union(&raw);
        if !imports_match(&row_union, &INVENTORY_TUNE) {
            self.fail_run(
                "gc.E",
                code,
                0,
                "positive",
                "",
                &sha_ok,
                &rel_ok,
                &format!("row-boot call inventory moved: got [{}] (the row adds no interface calls; banked {} heads)", row_union.join(" "), INVENTORY_TUNE.len()),
            );
        }
        if code != 0 {
            self.fail_run("gc.E", code, 0, "positive", "", &sha_ok, &rel_ok, &format!("exit {code}, wanted 0"));
        }
        self.results.emit(&format!(
            "{{\"schema\":1,\"type\":\"case\",\"gate\":\"gc\",\"case\":\"gc.E\",\"verdict\":\"pass\",\"exit\":0,\"exit_want\":0,\"control\":\"positive\",\"image\":{{\"path\":\"{rel_ok}\",\"sha256\":\"{sha_ok}\"}}}}"
        ));
        self.results.tally("pass", "positive");
        println!(
            "== case gc.E: PASS (4 rows checked against maps + board modules; the an500 row asserted in-artifact: census + containment INSIDE + window fit {}; the tune drive carried verbatim under the row) ==",
            "drawn=2195456"
        );

        println!("== case gc.E-break-cover (negative control) ==");
        let broke = "card cover doctored to 2 MiB (region placement spans past the granule): the heap's coverage guard must die loudly at boot";
        let (rawc, codec) = self.run_qemu(qemu, "gcE-break-cover", &rel_ctl);
        if cr_count(&rawc) == 0 {
            self.fail_run("gc.E-break-cover", codec, 134, "negative", broke, &sha_ctl, &rel_ctl, "no CR bytes in raw capture");
        }
        for m in [
            ROW_CENSUS,
            "[gc-host] handshake accepted",
            ROW_COVER_FAILFAST,
            "[gc-host] abort() -- structurally broken path, exiting 134",
        ] {
            if !contains_fixed(&rawc, m) {
                self.fail_run("gc.E-break-cover", codec, 134, "negative", broke, &sha_ctl, &rel_ctl, &format!("control pin '{m}' not found (the guard must die at boot with the declaration already on the wire)"));
            }
        }
        if !in_order(&rawc, &["[gc-host] handshake accepted", ROW_COVER_FAILFAST, "[gc-host] abort()"]) {
            self.fail_run("gc.E-break-cover", codec, 134, "negative", broke, &sha_ctl, &rel_ctl, "control order violated (accepted -> FAIL-FAST -> abort is the boot-death order)");
        }
        match parse_cover_line(&rawc) {
            Ok((_floor, span, cards, of)) => {
                if of != 2112 {
                    self.fail_run("gc.E-break-cover", codec, 134, "negative", broke, &sha_ctl, &rel_ctl, &format!("control cover buffer total {of}, wanted 2112"));
                }
                if cards <= of || span <= 2048 {
                    self.fail_run("gc.E-break-cover", codec, 134, "negative", broke, &sha_ctl, &rel_ctl, &format!("the doctored cover is not actually unaffordable (span={span} cards={cards} of={of})"));
                }
            }
            Err(e) => self.fail_run("gc.E-break-cover", codec, 134, "negative", broke, &sha_ctl, &rel_ctl, &e),
        }
        for m in [
            "[gc-host] GC ACTIVATION OK",
            ROW_CONTAIN_VERDICT,
            ROW_FIT_VERDICT,
            "FAST-PATH from the window",
            "[mgc-coll] collect #1 ENTER",
        ] {
            if contains_fixed(&rawc, m) {
                self.fail_run("gc.E-break-cover", codec, 134, "negative", broke, &sha_ctl, &rel_ctl, &format!("positive pin '{m}' PRESENT under the doctored cover -- the guard did not die at boot"));
            }
        }
        if codec != 134 {
            self.fail_run("gc.E-break-cover", codec, 134, "negative", broke, &sha_ctl, &rel_ctl, &format!("exit {codec}, wanted 134 (the abort class)"));
        }
        self.results.emit(&format!(
            "{{\"schema\":1,\"type\":\"case\",\"gate\":\"gc\",\"case\":\"gc.E-break-cover\",\"verdict\":\"pass\",\"exit\":134,\"exit_want\":134,\"control\":\"negative\",\"broke\":\"{broke}\",\"image\":{{\"path\":\"{rel_ctl}\",\"sha256\":\"{sha_ctl}\"}}}}"
        ));
        self.results.tally("pass", "negative");
        println!("== case gc.E-break-cover: PASS (the unaffordable cover died loudly at boot, exit 134, before any allocation; the guard's arithmetic recomputed) ==");

    }

    /// THE ROW TIE, driven by the SDK's board table: every row of the table
    /// names the collector objects its image links (the skeleton, the heap
    /// and the collect object), and each must be the object the row's OWN
    /// declaration compiles to -- its source under the fields of the header
    /// block that names the row, passed as the collector's knob macros (the
    /// form the F769 row's frozen objects are built in), with the row's
    /// class flag set, tying the frozen object the row names apart from the
    /// producer stamp. A row may share another row's object only where the
    /// bytes agree. The doctored table -- the U5A5 row naming the emulator
    /// row's heap object again, the defect this case was written after --
    /// must be refused by the same routine, naming the row and the object.
    fn run_sdk_row_tie(&mut self, gxx: &ToolCmd, facts: &RustcFacts, stripped: &Path) {
        const CASE: &str = "gc.sdk-rows-tie-their-row";
        const CONTROL: &str = "gc.sdk-rows-tie-control-borrowed-object";
        println!("== case {CASE} (compile-tier control) ==");
        let table = match std::fs::read_to_string(self.repo.join(SDK_BOARD_TABLE.replace('/', std::path::MAIN_SEPARATOR_STR))) {
            Ok(t) => t,
            Err(e) => self.fail(CASE, &format!("{SDK_BOARD_TABLE} unreadable: {e}")),
        };
        let header = match std::fs::read_to_string(self.repo.join("src").join("gc").join("host").join("mgc_board_rows.h")) {
            Ok(t) => t,
            Err(e) => self.fail(CASE, &format!("src/gc/host/mgc_board_rows.h unreadable: {e}")),
        };
        let mut memo: Vec<(String, String)> = Vec::new();
        let ties = match self.sdk_row_tie_pass(gxx, CASE, facts, stripped, &table, &header, &mut memo) {
            Ok(n) => n,
            Err(f) => self.fail(CASE, &f),
        };
        self.results.emit(&format!(
            "{{\"schema\":1,\"type\":\"case\",\"gate\":\"gc-rederive\",\"case\":\"{CASE}\",\"verdict\":\"pass\",\"exit\":null,\"exit_want\":null,\"control\":\"positive\",\"image\":null,\"stats\":{{\"ties\":{ties},\"compiles\":{}}}}}",
            memo.len()
        ));
        self.results.tally("pass", "positive");
        println!("== case {CASE}: PASS ({ties} collector objects over the board table's rows tie their own row's declaration apart from the producer stamp; {} compiles) ==", memo.len());

        println!("== case {CONTROL} (negative control) ==");
        let doctored = table.replace("heap-row-u5a5.o", "heap-row-an505.o");
        if doctored == table {
            self.fail(CONTROL, "the board table names no heap-row-u5a5.o: the doctored table cannot be built");
        }
        match self.sdk_row_tie_pass(gxx, CONTROL, facts, stripped, &doctored, &header, &mut memo) {
            Ok(_) => self.fail(CONTROL, "the board table with the U5A5 row's heap object put back to the emulator row's was ACCEPTED: the tie cannot see a row that links another row's object"),
            Err(f) => {
                if !(f.contains("st-nucleo-u5a5zj-q") && f.contains("heap-row-an505.o")) {
                    self.fail(CONTROL, &format!("the doctored table was refused for another reason: {f}"));
                }
                println!("   refused: {f}");
            }
        }
        self.results.emit(&format!(
            "{{\"schema\":1,\"type\":\"case\",\"gate\":\"gc-rederive\",\"case\":\"{CONTROL}\",\"verdict\":\"pass\",\"exit\":null,\"exit_want\":null,\"control\":\"negative\",\"broke\":\"the board table with the U5A5 row's heap object put back to the emulator row's\",\"image\":null}}"
        ));
        self.results.tally("pass", "negative");
        println!("== case {CONTROL}: PASS (the board table with the U5A5 row's heap object put back to the emulator row's refused, naming the row and the object) ==");
    }

    /// One pass of the row tie over a board table's text: the ties made, or
    /// the first fault. A compile is made once per source, knob set and
    /// class (`memo`), so the doctored pass costs none.
    #[allow(clippy::too_many_arguments)]
    fn sdk_row_tie_pass(&mut self, gxx: &ToolCmd, case: &str, facts: &RustcFacts, stripped: &Path, table: &str, header: &str, memo: &mut Vec<(String, String)>) -> Result<u32, String> {
        let sdk_rows = parse_sdk_rows(table)?;
        let host_rows = parse_board_rows(header)?;
        let mut ties = 0u32;
        for r in &sdk_rows {
            let name = r.board.strip_prefix("arm-").unwrap_or(&r.board);
            let Some(h) = host_rows.iter().find(|h| h.board == name) else {
                return Err(format!("{}: no row block of src/gc/host/mgc_board_rows.h names the board, so its image would link another board's collector objects", r.board));
            };
            let mut row_ties = 0u32;
            for o in &r.objects {
                let Some((_, src, _, plan_class)) = FROZEN_PLAN.iter().find(|(n, _, _, _)| n == o) else {
                    continue;
                };
                let knobs: Vec<String> = match *src {
                    "src/gc/mgc_skeleton.cpp" => vec![format!("-DMGC_HANDLE_SLOTS={}", h.handles)],
                    "src/gc/mgc_heap.cpp" => vec![
                        format!("-DMGC_HEAP_BYTES={}u", h.region),
                        format!("-DMGC_ARENA_COVER_BYTES={}u", h.cover),
                        format!("-DMGC_FINQ_CAP={}u", h.finq),
                        format!("-DMGC_OOM_RESERVE={}u", h.reserve),
                        format!("-DMGC_WINDOW_BYTES={}u", h.window),
                        format!("-DMGC_WINDOW_FLOOR={}u", h.wfloor),
                        format!("-DMGC_WALL_RETRIES={}u", h.retries),
                    ],
                    "src/gc/mgc_collect.cpp" => vec![format!("-DMGC_MARK_STACK_ENTRIES={}u", h.markstack), format!("-DMGC_OOM_RESERVE={}u", h.reserve)],
                    _ => continue,
                };
                if *plan_class != r.class {
                    return Err(format!("{}: the row's class is {:?}, the object {o} it names was frozen for {:?}", r.board, r.class, plan_class));
                }
                let key = format!("{src}|{}|{:?}", knobs.join(" "), r.class);
                let got = match memo.iter().find(|(k, _)| *k == key) {
                    Some((_, s)) => s.clone(),
                    None => {
                        let out_rel = format!("src/kernel/target/gc-gate/obj/rowtie-{}.o", memo.len());
                        let out_abs = self.repo.join(out_rel.replace('/', std::path::MAIN_SEPARATOR_STR));
                        let _ = std::fs::remove_file(&out_abs);
                        let mut defs: Vec<&str> = vec!["-DMGC_HEAP_LIVE", "-DMGC_COLLECT_LIVE", "-DMGC_SWEEP_LIVE", "-DMGC_TUNE_LIVE"];
                        defs.extend(knobs.iter().map(|s| s.as_str()));
                        self.compile(gxx, case, src, &defs, &out_rel, r.class);
                        let s = match sha256_without_stamp(facts, &out_abs, &stripped.join(format!("rowtie-{}.o", memo.len()))) {
                            Ok(s) => s,
                            Err(e) => self.fail(case, &format!("{src} under the {} row's declaration produced no object, or the copier refused it ({e})", r.board)),
                        };
                        memo.push((key, s.clone()));
                        s
                    }
                };
                let frozen_abs = self.repo.join("goldens").join("gc").join(o);
                let want = match sha256_without_stamp(facts, &frozen_abs, &stripped.join(format!("rowtie-frozen-{o}"))) {
                    Ok(s) => s,
                    Err(e) => self.fail(case, &format!("goldens/gc/{o} unreadable, or the copier refused it: {e}")),
                };
                if got != want {
                    return Err(format!(
                        "{}: the row names {o}; {src} compiled under the row's own declaration ({}) is {got}, not {want} apart from the producer stamp -- the row links a collector object built from another board's row declaration (freeze the row's own object and name it in the board table)",
                        r.board,
                        knobs.join(" ")
                    ));
                }
                println!("   {}: {o} ties {src} under the row's own declaration ({})", r.board, &got[..16]);
                ties += 1;
                row_ties += 1;
            }
            if row_ties != 3 {
                return Err(format!("{}: {row_ties} of the three collector row objects (the skeleton, the heap and the collect object) found in the row's list", r.board));
            }
        }
        Ok(ties)
    }

    /// The maintainer's re-derivation leg ('rhu-gate gc rederive'): recompile
    /// every frozen-plan object from src/gc with the compiler of record and
    /// byte-assert it against goldens/gc, then run the compile-tier
    /// posture control. This is the machinery consumers never run: a
    /// source, flag or kit change re-runs it and updates goldens +
    /// MANIFEST + the in-code pin together in one reviewed commit.
    fn run_rederive(&mut self, gxx: &ToolCmd) {
        println!("== rederive: {} frozen-plan objects from src/gc with the compiler of record, compared apart from the producer stamp (each sha256 the object's with its comment section removed) ==", FROZEN_PLAN.len());
        let obj_dir = self.out.join("obj");
        if std::fs::create_dir_all(&obj_dir).is_err() {
            self.fail("rederive.objects", "cannot create the rederive obj directory");
        }
        let facts = match rustc_facts() {
            Ok(f) => f,
            Err(e) => self.fail("rederive.objects", &e),
        };
        let stripped = obj_dir.join("stripped");
        let mut stamps_frozen: Vec<String> = Vec::new();
        let mut stamps_fresh: Vec<String> = Vec::new();
        let heap_bytes_flag = format!("-DMGC_HEAP_BYTES={GATE_HEAP_BYTES}u");
        if !FROZEN_PLAN.iter().any(|(_, _, defs, _)| defs.contains(&heap_bytes_flag.as_str())) {
            self.fail(
                "rederive.objects",
                &format!("no frozen-plan row carries {heap_bytes_flag} -- the plan literals drifted from GATE_HEAP_BYTES"),
            );
        }
        for (name, src_rel, defines, class) in FROZEN_PLAN {
            let out_rel = format!("src/kernel/target/gc-gate/obj/{name}");
            let out_abs = self.repo.join(out_rel.replace('/', std::path::MAIN_SEPARATOR_STR));
            let _ = std::fs::remove_file(&out_abs);
            self.compile(gxx, "rederive.objects", src_rel, defines, &out_rel, class);
            if !out_abs.is_file() {
                self.fail(
                    "rederive.objects",
                    &format!("{name}: compile reported success but produced no object (the compile-assert guard)"),
                );
            }
            let frozen_abs = self.repo.join("goldens").join("gc").join(name);
            let fresh = match sha256_without_stamp(&facts, &out_abs, &stripped.join(format!("fresh-{name}"))) {
                Ok(s) => s,
                Err(e) => self.fail("rederive.objects", &format!("{name}: cannot hash the fresh object apart from its stamp: {e}")),
            };
            let frozen = match sha256_without_stamp(&facts, &frozen_abs, &stripped.join(format!("frozen-{name}"))) {
                Ok(s) => s,
                Err(e) => self.fail("rederive.objects", &format!("{name}: cannot hash the frozen object apart from its stamp: {e}")),
            };
            stamps_frozen.push(producer_stamp_of(&frozen_abs));
            stamps_fresh.push(producer_stamp_of(&out_abs));
            if fresh != frozen {
                self.fail(
                    "rederive.objects",
                    &format!(
                        "{name}: fresh compile {fresh} != frozen {frozen}, apart from the producer stamp -- source, flags, or kit moved; a re-freeze is a registered, reviewed change (goldens/gc + MANIFEST.md + the in-code pin move together), never a silent refresh"
                    ),
                );
            }
            println!("   {name} OK ({})", &fresh[..16]);
        }
        println!("   the frozen tier's producer stamp (report-only): {}", distinct_stamps(&stamps_frozen));
        println!("   the rebuild's producer stamp (report-only): {}", distinct_stamps(&stamps_fresh));
        self.results.emit(&format!(
            "{{\"schema\":1,\"type\":\"case\",\"gate\":\"gc-rederive\",\"case\":\"rederive.objects\",\"verdict\":\"pass\",\"exit\":null,\"exit_want\":null,\"control\":\"positive\",\"image\":null,\"stats\":{{\"objects\":{}}}}}",
            FROZEN_PLAN.len()
        ));
        self.results.tally("pass", "positive");
        println!("== rederive: {}/{} objects byte-identical to goldens/gc apart from the producer stamp ==", FROZEN_PLAN.len(), FROZEN_PLAN.len());

        println!("== case gc.f769-shared-control (compile-tier control) ==");
        let shared_rel = "src/kernel/target/gc-gate/obj/skel-row-f769-control.o";
        let shared_abs = self.repo.join(shared_rel.replace('/', std::path::MAIN_SEPARATOR_STR));
        let _ = std::fs::remove_file(&shared_abs);
        self.compile(
            gxx,
            "gc.f769-shared-control",
            "src/gc/mgc_skeleton.cpp",
            &["-DMGC_HEAP_LIVE", "-DMGC_COLLECT_LIVE", "-DMGC_SWEEP_LIVE", "-DMGC_TUNE_LIVE", "-DMGC_HANDLE_SLOTS=64"],
            shared_rel,
            Class::CortexM7,
        );
        let shared_got = match sha256_without_stamp(&facts, &shared_abs, &stripped.join("f769-control.o")) {
            Ok(s) => s,
            Err(e) => self.fail("gc.f769-shared-control", &format!("the skeleton's compile under the F769 row produced no object, or the copier refused it ({e})")),
        };
        let frozen_abs = self.repo.join("goldens").join("gc").join("skel-row.o");
        let shared_want = match sha256_without_stamp(&facts, &frozen_abs, &stripped.join("skel-row.o")) {
            Ok(s) => s,
            Err(e) => self.fail("gc.f769-shared-control", &format!("goldens/gc/skel-row.o unreadable, or the copier refused it: {e}")),
        };
        println!("   the frozen skeleton's producer stamp (report-only): '{}'", producer_stamp_of(&frozen_abs));
        println!("   the F769 compile's producer stamp (report-only): '{}'", producer_stamp_of(&shared_abs));
        if shared_got != shared_want {
            self.fail(
                "gc.f769-shared-control",
                &format!("the skeleton under the F769 row's defines compiled to {shared_got}, not the frozen an500 skel-row.o {shared_want}, apart from the producer stamp: the rows no longer share the skeleton, so the F769 image needs its own frozen skeleton object"),
            );
        }
        let frozen_bytes = match std::fs::read(&frozen_abs) {
            Ok(b) => b,
            Err(e) => self.fail("gc.f769-shared-control", &format!("goldens/gc/skel-row.o unreadable: {e}")),
        };
        let Some((code_at, _)) = section_span_prefixed(&frozen_bytes, ".text") else {
            self.fail("gc.f769-shared-control", "goldens/gc/skel-row.o carries no code section to doctor: the control cannot fire");
        };
        let mut doctored = frozen_bytes.clone();
        match doctored.get_mut(code_at) {
            Some(b) => *b ^= 0x01,
            None => self.fail("gc.f769-shared-control", "goldens/gc/skel-row.o's code section lies outside the file: the control cannot fire"),
        }
        let doctored_abs = self.repo.join("src").join("kernel").join("target").join("gc-gate").join("obj").join("skel-row-doctored-control.o");
        if let Some(dir) = doctored_abs.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Err(e) = std::fs::write(&doctored_abs, &doctored) {
            self.fail("gc.f769-shared-control", &format!("the doctored copy could not be written: {e}"));
        }
        let doctored_sha = match sha256_without_stamp(&facts, &doctored_abs, &stripped.join("skel-row-doctored.o")) {
            Ok(s) => s,
            Err(e) => self.fail("gc.f769-shared-control", &format!("the doctored copy: the copier refused it ({e})")),
        };
        if doctored_sha == shared_want {
            self.fail("gc.f769-shared-control", "a copy of the frozen skeleton with one code byte moved ties apart from the stamp: the comparison cannot see code");
        }
        println!("   the doctored control: a copy of the frozen skeleton with one code byte moved is refused apart from the stamp ({} != {})", &doctored_sha[..16], &shared_want[..16]);
        self.results.emit(&format!(
            "{{\"schema\":1,\"type\":\"case\",\"gate\":\"gc-rederive\",\"case\":\"gc.f769-shared-control\",\"verdict\":\"pass\",\"exit\":null,\"exit_want\":null,\"control\":\"positive\",\"image\":null,\"stats\":{{\"shared_sha\":\"{shared_want}\"}}}}"
        ));
        self.results.tally("pass", "positive");
        println!("== case gc.f769-shared-control: PASS (the skeleton under the F769 row's defines ties the frozen an500 skel-row.o {} apart from the producer stamp: one skeleton object serves both rows; a copy with one code byte moved refused) ==", &shared_want[..16]);

        self.run_sdk_row_tie(gxx, &facts, &stripped);

        println!("== case gc.E-posture-refused (negative control) ==");
        self.compile_expect_fail(
            gxx,
            "gc.E-posture-refused",
            "src/gc/host/mgc_host.cpp",
            &["-DMGC_HOST_DRIVE_ALLOC", "-DMGC_HOST_DRIVE_COLLECT", "-DMGC_HOST_DRIVE_SWEEP", "-DMGC_HOST_DRIVE_TUNE", "-DMGC_HOST_ROW_AN500", "-DMGC_FRAG_CENSUS"],
            "src/kernel/target/gc-gate/obj/host-row-posture.o",
            ROW_REFUSAL_TEXT,
        );
        self.results.emit(&format!(
            "{{\"schema\":1,\"type\":\"case\",\"gate\":\"gc-rederive\",\"case\":\"gc.E-posture-refused\",\"verdict\":\"pass\",\"exit\":null,\"exit_want\":null,\"control\":\"negative\",\"broke\":\"MGC_FRAG_CENSUS on a row image: the header guard must refuse the compile\",\"image\":null}}"
        ));
        self.results.tally("pass", "negative");
        println!("== case gc.E-posture-refused: PASS (an evidence-posture knob on a row image refused at compile with the designed message) ==");
    }
}

/// Run the gc gate (armed cases; pending ladder listed, never counted).
pub fn run(repo: &Path) -> ! {
    let facts = match rustc_facts() {
        Ok(f) => f,
        Err(e) => {
            println!("rhu-gate: {e}");
            exit(1);
        }
    };
    let (qemu, qemu_v) = match resolve_qemu(repo) {
        Ok(q) => q,
        Err(e) => {
            println!("rhu-gate: {e}");
            exit(1);
        }
    };
    let ld = match resolve_ld(&facts) {
        Ok(l) => l,
        Err(e) => {
            println!("rhu-gate: {e}");
            exit(1);
        }
    };
    let out = repo.join("src").join("kernel").join("target").join("gc-gate");
    if std::fs::create_dir_all(&out).is_err() {
        println!("rhu-gate: cannot create {}", out.display());
        exit(1);
    }
    let results = match Results::create(&out.join("gc.results.jsonl")) {
        Ok(r) => r,
        Err(e) => {
            println!("rhu-gate: cannot create results file: {e}");
            exit(1);
        }
    };
    let mut gc = Gc {
        repo: repo.to_path_buf(),
        out,
        results,
        gate_id: GATE,
        display: DISPLAY,
        raw_gca: Vec::new(),
    };
    gc.results.emit(&format!(
        "{{\"schema\":1,\"type\":\"run\",\"gate\":\"gc\",\"run\":\"gc-{}\",\"host\":\"{HOST}\",\"runner\":\"{RUNNER}\",\"chain\":{},\"tools\":{{\"qemu\":\"{}\",\"rustc\":\"{}\",\"ld\":\"{}\"}}}}",
        std::process::id(),
        chain_json_from_env(),
        qemu_v,
        facts.version,
        ld.version
    ));
    gc.verify_frozen();
    gc.run_vendor();
    gc.run_activation(&facts, &ld, &qemu);
    gc.run_skeleton_alloc(&facts, &ld, &qemu);
    gc.run_collect(&facts, &ld, &qemu);
    gc.run_sweep(&facts, &ld, &qemu);
    gc.run_free_pad(&facts, &ld, &qemu);
    gc.run_handle_pool(&facts, &ld, &qemu);
    gc.run_no_gc_region(&facts, &ld, &qemu);
    gc.run_tune(&facts, &ld, &qemu);
    gc.run_row(&facts, &ld, &qemu);
    for p in PENDING {
        println!("   pending (unarmed, not counted): {p}");
    }
    for c in INTERNAL_CASES {
        println!("   internal (rederive leg, not counted): {c}");
    }
    gc.results.emit_summary(GATE, "pass", 0);
    if !validate_results(repo, gc.results.path()) {
        println!("{DISPLAY}: FAIL -- results file failed schema validation");
        exit(1);
    }
    println!(
        "{DISPLAY}: PASS (20/20 cases incl. 9 negative controls; {} pending unarmed; {} internal on the rederive leg)  [{} | {} | {}]",
        PENDING.len(),
        INTERNAL_CASES.len(),
        qemu_v,
        facts.version,
        ld.version
    );
    exit(0);
}

/// The maintainer's re-derivation leg: the only place the image-tier C++
/// compiler is needed since the objects were committed.
/// Recompiles the full frozen plan and byte-asserts it against
/// goldens/gc, then runs the compile-tier posture control. Needs the
/// compiler of record (RHU_CLANG -> tools/toolchains/clang.local -> refuse);
/// consumers never need it -- the chain never calls this.
pub fn rederive(repo: &Path) -> ! {
    let gxx = match resolve_clang(repo) {
        Ok(g) => g,
        Err(e) => {
            println!("rhu-gate: {e}");
            exit(1);
        }
    };
    let out = repo.join("src").join("kernel").join("target").join("gc-gate");
    if std::fs::create_dir_all(&out).is_err() {
        println!("rhu-gate: cannot create {}", out.display());
        exit(1);
    }
    let results = match Results::create(&out.join("gc-rederive.results.jsonl")) {
        Ok(r) => r,
        Err(e) => {
            println!("rhu-gate: cannot create results file: {e}");
            exit(1);
        }
    };
    let mut gc = Gc {
        repo: repo.to_path_buf(),
        out,
        results,
        gate_id: "gc-rederive",
        display: "GC REDERIVE",
        raw_gca: Vec::new(),
    };
    gc.results.emit(&format!(
        "{{\"schema\":1,\"type\":\"run\",\"gate\":\"gc-rederive\",\"run\":\"gc-rederive-{}\",\"host\":\"compile-tier\",\"runner\":\"{RUNNER}\",\"tools\":{{\"clang\":\"{}\"}}}}",
        std::process::id(),
        gxx.version
    ));
    let mpath = repo.join("goldens").join("gc").join("MANIFEST.md");
    match sha256_file(&mpath) {
        Ok(s) if s == SHA_FROZEN_MANIFEST => {}
        Ok(s) => {
            println!("GC REDERIVE: FAIL -- goldens/gc/MANIFEST.md sha256 {s} != pinned {SHA_FROZEN_MANIFEST}");
            exit(1);
        }
        Err(e) => {
            println!("GC REDERIVE: FAIL -- goldens/gc/MANIFEST.md unreadable: {e}");
            exit(1);
        }
    }
    gc.run_rederive(&gxx);
    gc.results.emit_summary("gc-rederive", "pass", 0);
    if !validate_results(repo, gc.results.path()) {
        println!("GC REDERIVE: FAIL -- results file failed schema validation");
        exit(1);
    }
    println!(
        "GC REDERIVE: PASS ({}/{} frozen objects re-derived byte-identical apart from the producer stamp; the F769 row's shared skeleton ties the an500 object apart from the stamp, a copy with one code byte moved refused; every row of the SDK's board table names collector objects that tie their own row's declaration, the table with a borrowed heap object refused; posture control refused at compile)  [{}]",
        FROZEN_PLAN.len(),
        FROZEN_PLAN.len(),
        gxx.version
    );
    exit(0);
}

/// Negative-control the gc gate's checkers: the vendor identity chain
/// (sha compare, two-direction coverage, manifest-parser vacuity, the
/// version-macro read) and the activation checkers (import-closure
/// pin, forbidden-family sweep, the transcript order assert, the refusal
/// and tuple pins) -- each must REJECT its doctored input, with the
/// accept side pinned on genuine inputs.
pub fn selfcheck(repo: &Path) -> ! {
    let facts = match rustc_facts() {
        Ok(f) => f,
        Err(e) => {
            println!("rhu-gate: {e}");
            exit(1);
        }
    };
    if !repo.join("goldens").join("gc").join("MANIFEST.md").is_file() {
        println!("rhu-gate gc selfcheck: frozen inputs missing (goldens/gc)");
        exit(1);
    }
    let out = repo.join("src").join("kernel").join("target").join("gc-gate");
    let tmp = out.join("selfcheck");
    let _ = std::fs::create_dir_all(&tmp);
    let results = match Results::create(&out.join("gc-selfcheck.results.jsonl")) {
        Ok(r) => r,
        Err(e) => {
            println!("rhu-gate: cannot create results file: {e}");
            exit(1);
        }
    };
    let mut sc = Selfcheck {
        repo: repo.to_path_buf(),
        gate: "gc-selfcheck".to_string(),
        display: "GC SELFCHECK".to_string(),
        results,
        n: 0,
        npass: 0,
        nneg: 0,
    };
    sc.results.emit(&format!(
        "{{\"schema\":1,\"type\":\"run\",\"gate\":\"gc-selfcheck\",\"run\":\"gc-selfcheck-{}\",\"host\":\"{HOST}\",\"runner\":\"{RUNNER}\",\"tools\":{{\"rustc\":\"{}\"}}}}",
        std::process::id(),
        facts.version
    ));

    let real = repo
        .join("src")
        .join("vendor")
        .join("gcinterface")
        .join("gcinterface.h");
    let mut bytes = std::fs::read(&real).unwrap_or_default();
    let real_sha = sha256_hex(&bytes);
    if let Some(b) = bytes.first_mut() {
        *b ^= 0x01;
    }
    let doctored = tmp.join("doctored-gcinterface.h");
    write_fixture(&doctored, &bytes);
    let doctored_sha = match sha256_file(&doctored) {
        Ok(s) => s,
        Err(_) => String::new(),
    };
    sc.probe(
        "vendored-byte-flipped",
        "one flipped byte in a vendored file still matching its recorded sha",
        !doctored_sha.is_empty() && doctored_sha == real_sha,
    );

    let vacuous = parse_vendored_manifest("# nothing here\n\njust prose, no records\n");
    sc.probe(
        "parser-invents-records",
        "a record-free prose manifest parsing to invented records",
        !vacuous.is_empty(),
    );

    let tok_sha = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
    let root_level = parse_vendored_manifest(&format!("- `NOTICE`\n  `{tok_sha}`\n"));
    sc.accept_probe(
        "root-record-parses",
        "a root-level (slash-less) record path parses to its (path, sha) pair",
        root_level.len() == 1 && root_level[0].0 == "NOTICE" && root_level[0].1 == tok_sha,
    );
    for (id, broke, bullet) in [
        ("record-path-space", "a space-bearing path token parsing as a record", "- `not a path`"),
        ("record-path-empty", "an empty path token parsing as a record", "- ``"),
        ("record-path-lead-slash", "a leading-slash (absolute-shaped) path token parsing as a record", "- `/rooted/name`"),
        ("record-path-trail-slash", "a trailing-slash (directory-shaped) path token parsing as a record", "- `gcinterface/`"),
    ] {
        sc.probe(id, broke, !parse_vendored_manifest(&format!("{bullet}\n  `{tok_sha}`\n")).is_empty());
    }

    let fake = parse_vendored_manifest(
        "- `gcinterface/gcinterface.h`\n  `0000000000000000000000000000000000000000000000000000000000000000`\n",
    );
    let fake_matches = fake
        .first()
        .map(|(_, sha)| sha == &real_sha)
        .unwrap_or(true);
    sc.probe(
        "manifest-sha-doctored",
        "an all-zeros recorded sha matching the real vendored file",
        fake_matches,
    );

    let gc_manifest_path = repo.join("goldens").join("gc").join("MANIFEST.md");
    let gc_manifest_text = std::fs::read_to_string(&gc_manifest_path).unwrap_or_default();
    let gc_records = parse_vendored_manifest(&gc_manifest_text);
    let bare_row = gc_records
        .iter()
        .find(|(p, _)| p.as_str() == "goldens/gc/skel-bare.o")
        .map(|(_, s)| s.clone())
        .unwrap_or_default();
    let mut frozen_bytes = std::fs::read(repo.join("goldens").join("gc").join("skel-bare.o")).unwrap_or_default();
    let frozen_real_sha = sha256_hex(&frozen_bytes);
    frozen_bytes.push(b'x');
    let frozen_doctored = tmp.join("doctored-skel-bare.o");
    write_fixture(&frozen_doctored, &frozen_bytes);
    let frozen_doctored_sha = sha256_file(&frozen_doctored).unwrap_or_default();
    sc.probe(
        "frozen-obj-doctored",
        "a frozen world object with one appended byte still matching its manifest record",
        !frozen_doctored_sha.is_empty() && frozen_doctored_sha == bare_row,
    );
    sc.accept_probe(
        "frozen-obj-genuine",
        "the untouched frozen object accepted by its manifest record",
        !bare_row.is_empty() && frozen_real_sha == bare_row,
    );

    let mut gc_manifest_bytes = std::fs::read(&gc_manifest_path).unwrap_or_default();
    if let Some(b) = gc_manifest_bytes.first_mut() {
        *b ^= 0x01;
    }
    let manifest_doctored = tmp.join("doctored-gc-manifest.md");
    write_fixture(&manifest_doctored, &gc_manifest_bytes);
    let manifest_doctored_sha = sha256_file(&manifest_doctored).unwrap_or_default();
    sc.probe(
        "frozen-manifest-doctored",
        "a doctored goldens/gc/MANIFEST.md still matching the in-code pin",
        !manifest_doctored_sha.is_empty() && manifest_doctored_sha == SHA_FROZEN_MANIFEST,
    );
    sc.accept_probe(
        "frozen-manifest-genuine",
        "the real goldens/gc/MANIFEST.md accepted by the in-code pin",
        sha256_file(&gc_manifest_path).map(|h| h == SHA_FROZEN_MANIFEST).unwrap_or(false),
    );

    let manifest_text = std::fs::read_to_string(repo.join("src").join("VENDORED.md")).unwrap_or_default();
    let records = parse_vendored_manifest(&manifest_text);
    let mut with_extra: Vec<String> = records.iter().map(|(p, _)| p.clone()).collect();
    with_extra.push("gcinterface/uninvited.h".to_string());
    let extra_covered = with_extra.iter().all(|f| records.iter().any(|(rel, _)| rel == f));
    sc.probe(
        "file-without-record",
        "an unrecorded file under src/vendor passing the coverage sweep",
        extra_covered,
    );

    let doctored_hdr = "#define GC_INTERFACE_MAJOR_VERSION 4\n#define GC_INTERFACE_MINOR_VERSION 8\n";
    sc.probe(
        "version-major-doctored",
        "a doctored GC_INTERFACE_MAJOR_VERSION reading as the pinned value",
        extract_define(doctored_hdr, "GC_INTERFACE_MAJOR_VERSION") == Some(GC_INTERFACE_MAJOR),
    );

    let mut with_extra_import: Vec<String> = IMPORTS_BARE.iter().map(|s| s.to_string()).collect();
    with_extra_import.push("malloc".to_string());
    with_extra_import.sort();
    sc.probe(
        "import-extra",
        "an unexpected extra import satisfying the exact-set pin",
        imports_match(&with_extra_import, &IMPORTS_BARE),
    );
    let mut missing: Vec<String> = IMPORTS_BARE.iter().map(|s| s.to_string()).collect();
    missing.pop();
    missing.sort();
    sc.probe(
        "import-missing",
        "a vanished import satisfying the exact-set pin",
        imports_match(&missing, &IMPORTS_BARE),
    );

    let unwindy = vec!["_Unwind_Resume".to_string(), "abort".to_string()];
    sc.probe(
        "forbidden-family-unwind",
        "an _Unwind import passing the forbidden-family sweep",
        forbidden_import(&unwindy).is_none(),
    );

    let reversed = b"[mgc-skel] GC_VersionInfo reporting major=5 minor=8\n[mgc-skel] GC_VersionInfo ENTER (EE passed\n".to_vec();
    sc.probe(
        "handshake-order-reversed",
        "a report-before-print transcript satisfying the order assert",
        in_order(&reversed, &["GC_VersionInfo ENTER", "GC_VersionInfo reporting"]),
    );
    let missing_marker = b"[mgc-skel] GC_VersionInfo ENTER (EE passed\n".to_vec();
    sc.probe(
        "handshake-order-missing",
        "a transcript missing the report line satisfying the order assert",
        in_order(&missing_marker, &["GC_VersionInfo ENTER", "GC_VersionInfo reporting"]),
    );

    let refused = b"[gc-host] GC ACTIVATION REFUSED: reported {4,8}\n".to_vec();
    sc.probe(
        "positive-with-refusal",
        "a positive-case transcript carrying REFUSED passing the forbidden sweep",
        !contains_fixed(&refused, "REFUSED"),
    );

    let good_report = b"[mgc-skel] GC_VersionInfo reporting major=5 minor=8 build=0\n".to_vec();
    let bad_report = format!(
        "[mgc-skel] GC_VersionInfo reporting major={} minor={GC_INTERFACE_MINOR} build=0",
        GC_INTERFACE_MAJOR - 1
    );
    sc.probe(
        "control-report-doctored",
        "a good-tuple transcript satisfying the control's doctored-report pin",
        contains_fixed(&good_report, &bad_report),
    );

    sc.accept_probe(
        "manifest-parses",
        "the real src/VENDORED.md parses to a non-empty record set",
        !records.is_empty(),
    );
    let vendor_root = repo.join("src").join("vendor");
    let on_disk = walk_vendor(&vendor_root);
    let covered = !on_disk.is_empty()
        && on_disk.iter().all(|f| records.iter().any(|(rel, _)| rel == f))
        && records.iter().all(|(rel, _)| on_disk.iter().any(|f| f == rel));
    sc.accept_probe(
        "coverage-two-way",
        "records and on-disk vendored files cover each other exactly",
        covered,
    );
    let all_tie = records.iter().all(|(rel, want)| {
        let p = vendor_root.join(rel.replace('/', std::path::MAIN_SEPARATOR_STR));
        matches!(sha256_file(&p), Ok(got) if &got == want)
    });
    sc.accept_probe("shas-tie", "every recorded sha ties its working-tree file", all_tie);
    let header_text = std::fs::read_to_string(vendor_root.join("gcinterface").join("gcinterface.h")).unwrap_or_default();
    sc.accept_probe(
        "tuple-pinned",
        "the real vendored header yields the pinned version tuple",
        extract_define(&header_text, "GC_INTERFACE_MAJOR_VERSION") == Some(GC_INTERFACE_MAJOR)
            && extract_define(&header_text, "GC_INTERFACE_MINOR_VERSION") == Some(GC_INTERFACE_MINOR)
            && extract_define(&header_text, "EE_INTERFACE_MAJOR_VERSION") == Some(EE_INTERFACE_MAJOR),
    );
    let genuine_imports: Vec<String> = {
        let mut v: Vec<String> = IMPORTS_BARE.iter().map(|s| s.to_string()).collect();
        v.sort();
        v
    };
    sc.accept_probe(
        "imports-genuine",
        "the pinned bare-skeleton import set accepted by the exact-set pin",
        imports_match(&genuine_imports, &IMPORTS_BARE) && forbidden_import(&genuine_imports).is_none(),
    );
    let genuine_order = b"[mgc-skel] GC_VersionInfo ENTER (EE passed\n[mgc-skel] GC_VersionInfo reporting major=5\n[mgc-skel] GC_Initialize ENTER\n[gc-host] GC ACTIVATION OK\n".to_vec();
    sc.accept_probe(
        "order-genuine",
        "the genuine handshake order accepted by the order assert",
        in_order(
            &genuine_order,
            &["GC_VersionInfo ENTER", "GC_VersionInfo reporting", "GC_Initialize ENTER", "GC ACTIVATION OK"],
        ),
    );

    let mut inv_extra: Vec<String> = INVENTORY_DRIVE.iter().map(|s| s.to_string()).collect();
    inv_extra.push("IGCHeap::Alloc".to_string());
    inv_extra.sort();
    sc.probe(
        "inventory-extra",
        "an extra call head satisfying the exact-inventory pin",
        imports_match(&inv_extra, &INVENTORY_DRIVE),
    );
    let mut inv_missing: Vec<String> = INVENTORY_DRIVE.iter().map(|s| s.to_string()).collect();
    inv_missing.pop();
    inv_missing.sort();
    sc.probe(
        "inventory-missing",
        "a vanished call head satisfying the exact-inventory pin",
        imports_match(&inv_missing, &INVENTORY_DRIVE),
    );
    let inv_raw = b"[mgc-skel] call IGCHeap::Initialize -> S_OK\r\n[mgc-skel] call IGCHeap::WhichGeneration(0x20035004) -> 0 (heap)\r\n[mgc-skel] call IGCHeap::GetTotalBytesInUse -> 116 (call #1024)\r\n[mgc-skel] call IGCHeap::WhichGeneration(0x20014b08) -> 2147483647 (not on heap)\r\n[mgc-skel] IGCHeap::Alloc(ctx=0x0 size=24 flags=0x0)\r\n".to_vec();
    let inv_got = call_union(&inv_raw);
    sc.accept_probe(
        "inventory-parse-genuine",
        "the union parser dedupes heads, strips arguments and riders, and skips non-call lines",
        inv_got
            == vec![
                "IGCHeap::GetTotalBytesInUse".to_string(),
                "IGCHeap::Initialize".to_string(),
                "IGCHeap::WhichGeneration".to_string(),
            ],
    );

    let header_text_ee =
        std::fs::read_to_string(repo.join("src").join("vendor").join("gcenv").join("gcenv.ee.h")).unwrap_or_default();
    let real_decls = gctoee_decl_names(&header_text_ee);
    let doctored_ee: String = {
        let mut dropped = false;
        header_text_ee
            .lines()
            .filter(|l| {
                if !dropped && l.trim().starts_with("static ") && l.contains("SuspendEE") {
                    dropped = true;
                    return false;
                }
                true
            })
            .collect::<Vec<_>>()
            .join("\n")
    };
    sc.probe(
        "decl-census-doctored",
        "a header with a deleted declaration still counting the pinned surface",
        gctoee_decl_names(&doctored_ee).len() == EE_SURFACE,
    );
    let ref_fixture = "#define REF(m) (fn_t)(void*)&GCToEEInterface::m\nfn_t volatile g[] = {\n    REF(SuspendEE),\n    REF(RestartEE),\n};\n";
    sc.probe(
        "ref-macro-counted",
        "the REF census counting the macro's own define line (the off-by-one class)",
        probe_ref_names(ref_fixture).len() == 3,
    );
    let probe_src =
        std::fs::read_to_string(repo.join("src").join("gc").join("mgc_ee_probe.cpp")).unwrap_or_default();
    let real_refs = probe_ref_names(&probe_src);
    sc.accept_probe(
        "surface-tie-genuine",
        "the real header's declaration census ties the real probe's REF census at the pinned count",
        real_decls.len() == EE_SURFACE && real_decls == real_refs,
    );

    sc.probe(
        "mangled-alien",
        "a non-flat-interface import reading as a surface method",
        mangled_method_name("malloc").is_some(),
    );
    sc.probe(
        "mangled-len-doctored",
        "a length-doctored mangled symbol reading as a surface method",
        mangled_method_name("_ZN15GCToEEInterface99StompWriteBarrierE").is_some(),
    );
    sc.accept_probe(
        "mangled-genuine",
        "the genuine mangled shape extracting its method name exactly",
        mangled_method_name("_ZN15GCToEEInterface17StompWriteBarrierEP22WriteBarrierParameters").as_deref()
            == Some("StompWriteBarrier"),
    );

    let spelled: Vec<String> = [
        "_ZN15GCToEEInterface9GetThreadEv",
        "_ZN15GCToEEInterface11GcScanRootsEPFvPP6ObjectP11ScanContextjEiiS4_",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    sc.accept_probe(
        "seam-spelling-genuine",
        "the probe's demands each defined in the merged runtime object by the same spelling",
        seam_spelling_fault(&spelled, &spelled).is_none(),
    );
    let kit_spelled: Vec<String> = vec![
        spelled[0].clone(),
        "_ZN15GCToEEInterface11GcScanRootsEPFvPP6ObjectP11ScanContextmEiiS4_".to_string(),
    ];
    sc.probe(
        "seam-spelling-kit",
        "a demand spelling a 32-bit parameter type as unsigned long against the archive's unsigned int",
        seam_spelling_fault(&kit_spelled, &spelled).is_none(),
    );
    let absent: Vec<String> = vec![spelled[0].clone(), "_ZN15GCToEEInterface7MissingEv".to_string()];
    sc.probe(
        "seam-spelling-absent",
        "a demand the merged runtime object does not define at all",
        seam_spelling_fault(&absent, &spelled).is_none(),
    );

    let ctl_with_heap = b"[gc-host] GC ACTIVATION OK\n[mgc-heap] region [0x0,0x0) 2048 KB via shim mmap\n[mgc-skel] FAIL-FAST: first object allocation\n".to_vec();
    sc.probe(
        "control-heap-line",
        "an allocator line inside the bare-skeleton control passing the negative-space check",
        !contains_fixed(&ctl_with_heap, "[mgc-heap]"),
    );

    let census_doctored = b"[mgc-heap] allocs=6 bytes=112 pads=1 null_returns=0 max_size=28 (flags=0x0 at alloc #2)\n".to_vec();
    sc.probe(
        "census-counters-doctored",
        "a moved allocation counter satisfying the exact census pin",
        contains_fixed(
            &census_doctored,
            "allocs=5 bytes=112 pads=1 null_returns=0 max_size=28 (flags=0x0 at alloc #2)",
        ),
    );

    let frozen_raw =
        b"[mgc-skel] call IGCHeap::RegisterFrozenSegment[0](mem=0x20014000 firstObj=+4 allocated=28 commit=32 reserved=32)\r\n"
            .to_vec();
    sc.accept_probe(
        "union-bracketed-head",
        "the union parser preserves the bracketed RegisterFrozenSegment[0] head as printed",
        call_union(&frozen_raw) == vec!["IGCHeap::RegisterFrozenSegment[0]".to_string()],
    );
    let mut inv_bare_bracket: Vec<String> = INVENTORY_COLLECT.iter().map(|s| s.to_string()).collect();
    for h in inv_bare_bracket.iter_mut() {
        if h == "IGCHeap::RegisterFrozenSegment[0]" {
            *h = "IGCHeap::RegisterFrozenSegment".to_string();
        }
    }
    inv_bare_bracket.sort();
    sc.probe(
        "union-bracket-stripped",
        "a bracket-stripped RegisterFrozenSegment head satisfying the collection-inventory pin",
        imports_match(&inv_bare_bracket, &INVENTORY_COLLECT),
    );

    let break_fixture = b"[mgc-coll] BREAK_ROOTS: ALL root scanning SUPPRESSED (negative control)\r\n[mgc-coll] roots: stacks=0 handles=0 (weak_skipped=0) dep_seen=0 dep_marked=0 finq=0 frozen_objs=0 frozen_edges=0\r\n[mgc-coll] mark census #1: roots=0 marked=0 marked_bytes=0 used_bytes=144 live_ratio~0% stack_hw=0 of 8192\r\n".to_vec();
    sc.probe(
        "break-census-passes-positive",
        "the zeroed break-roots census satisfying a positive census pin (the detector failing to bite)",
        contains_fixed(&break_fixture, CENSUS_ROOTS_LINE)
            || contains_fixed(&break_fixture, CENSUS_MARK_1),
    );
    sc.accept_probe(
        "break-census-detected",
        "the break-roots markers read as themselves (suppression is loud, not silent)",
        contains_fixed(&break_fixture, "BREAK_ROOTS: ALL root scanning SUPPRESSED")
            && contains_fixed(&break_fixture, "mark census #1: roots=0 marked=0"),
    );

    let bracket_reversed =
        b"[gc-host] EE RestartEE: worker resumed; counter ADVANCED within the bounded window\r\n[gc-host] EE SuspendEE(reason=1): worker tid=2 suspended\r\n"
            .to_vec();
    sc.probe(
        "bracket-order-reversed",
        "a RestartEE-before-SuspendEE transcript satisfying the bracket order assert",
        in_order(&bracket_reversed, &["EE SuspendEE(reason=1)", "EE RestartEE:"]),
    );

    let no_verdict = b"[gc-host] EE SuspendEE(reason=1): worker tid=2 suspended\r\n".to_vec();
    sc.probe(
        "suspend-without-verdict",
        "a SuspendEE line without the counter-frozen verdict satisfying the suspension pin",
        contains_fixed(
            &no_verdict,
            "counter FROZEN across a sleep window",
        ),
    );

    let break_set: Vec<String> = IMPORTS_COLLECT_BREAK.iter().map(|s| s.to_string()).collect();
    sc.probe(
        "collect-break-closure-crossmatch",
        "the break-roots object's import closure satisfying the full mark-stage closure pin",
        imports_match(&break_set, &IMPORTS_COLLECT),
    );
    let full_set: Vec<String> = IMPORTS_COLLECT.iter().map(|s| s.to_string()).collect();
    sc.accept_probe(
        "collect-closures-genuine",
        "the pinned mark-stage and break closures each accepted by their own pin and free of forbidden families",
        imports_match(&full_set, &IMPORTS_COLLECT)
            && imports_match(&break_set, &IMPORTS_COLLECT_BREAK)
            && forbidden_import(&full_set).is_none()
            && forbidden_import(&break_set).is_none(),
    );


    let weak_fixture = b"[mgc-coll] BREAK_WEAK: weak nulling SUPPRESSED (negative control)\r\n[mgc-coll] death #1: ws_null=0 wl_null=0 dep_clear=0 odd=0 | finq kept=1 eager=1 suppressed=1 promoted=2(+1 crit) freach_pending=3\r\n[gc-host] weak probe ws(#13): slot STALE non-null (dead=False)\r\n".to_vec();
    sc.probe(
        "break-weak-passes-positive",
        "the nulling-suppressed death line or stale probe satisfying a positive weak pin (the detector failing to bite)",
        contains_fixed(&weak_fixture, SW_DEATH_1)
            || contains_fixed(&weak_fixture, "[gc-host] weak probe ws(#13): slot NULL (dead=True)"),
    );
    sc.accept_probe(
        "break-weak-detected",
        "the BREAK_WEAK markers read as themselves (partition unchanged, nulling loudly suppressed)",
        contains_fixed(&weak_fixture, SW_DEATH_1_BREAK)
            && contains_fixed(&weak_fixture, "BREAK_WEAK: weak nulling SUPPRESSED"),
    );

    let sweep_set: Vec<String> = IMPORTS_COLLECT_SWEEP.iter().map(|s| s.to_string()).collect();
    sc.accept_probe(
        "sweep-closure-equality-genuine",
        "the sweep collector's closure accepted as its own break twin's pin (the equality is the claim)",
        imports_match(&sweep_set, &IMPORTS_COLLECT_SWEEP) && forbidden_import(&sweep_set).is_none(),
    );
    let mut sweep_dropped = sweep_set.clone();
    sweep_dropped.retain(|n| n != "mgc_heap_free_add");
    sc.probe(
        "sweep-closure-equality-dropped",
        "a sweep closure missing the free-list installer satisfying the equality pin",
        imports_match(&sweep_dropped, &IMPORTS_COLLECT_SWEEP),
    );

    let corrupt_fixture = b"[mgc-heap] BREAK_WINDOW_FIX: window plug+count SUPPRESSED (negative control)\r\n[mgc-coll] heap walk CORRUPT at 0x2100a038 (object #3, step=536873432, MT=0x0)\r\n[mgc-coll] FAIL-FAST: heap walk corrupt (size formula vs layout)\r\n".to_vec();
    sc.probe(
        "positive-with-corrupt-walk",
        "a positive-case transcript carrying the corrupt-walk line passing the forbidden sweep",
        !contains_fixed(&corrupt_fixture, "heap walk CORRUPT"),
    );
    sc.accept_probe(
        "corrupt-walk-detected",
        "the BREAK_WINDOW_FIX fixture reads as itself (banner + corrupt walk + fail-fast)",
        contains_fixed(&corrupt_fixture, "[mgc-coll] heap walk CORRUPT at 0x")
            && contains_fixed(&corrupt_fixture, "FAIL-FAST: heap walk corrupt (size formula vs layout)"),
    );

    let parking2 = "[gc-host] waiter: parking on the GC-done event (epoch 2)";
    let woken2 = "[gc-host] waiter: woken (epoch 2)";
    let doctored_eintr = format!(
        "[mgc-skel] call IGCHeap::SetWaitForGCEvent\r\n{parking2}\r\n[mgc-skel] call IGCHeap::SetWaitForGCEvent\r\n{woken2}\r\n"
    )
    .into_bytes();
    let slice_caught = match (find_pos(&doctored_eintr, parking2), find_pos(&doctored_eintr, woken2)) {
        (Some(p1), Some(p2)) if p1 < p2 => contains_fixed(&doctored_eintr[p1..p2], "SetWaitForGCEvent"),
        _ => false,
    };
    sc.accept_probe(
        "eintr-slice-catches-set",
        "a Set planted between epoch-2 parking and waking is seen by the elimination slice",
        slice_caught,
    );
    let genuine_eintr = format!(
        "[mgc-skel] call IGCHeap::SetWaitForGCEvent\r\n{parking2}\r\n[gc-host] activation handler ran (delivered on the target's own stack)\r\n{woken2}\r\n"
    )
    .into_bytes();
    let genuine_clean = match (find_pos(&genuine_eintr, parking2), find_pos(&genuine_eintr, woken2)) {
        (Some(p1), Some(p2)) if p1 < p2 => !contains_fixed(&genuine_eintr[p1..p2], "SetWaitForGCEvent"),
        _ => false,
    };
    sc.accept_probe(
        "eintr-slice-genuine-clean",
        "the genuine epoch-2 slice (handler only, earlier Sets outside) passes the elimination",
        genuine_clean,
    );

    sc.probe(
        "break-books-pass-positive",
        "the refund-suppressed posture books satisfying the positive books pin",
        TN_BOOKS_BREAK == TN_BOOKS || contains_fixed(TN_BOOKS_BREAK.as_bytes(), TN_BOOKS),
    );

    let big_pool = TN_HANDLES_1.replace("of 64", "of 4096");
    sc.probe(
        "handle-pool-knob-doctored",
        "a default-pool handles census satisfying the 64-slot gc.D pin",
        contains_fixed(big_pool.as_bytes(), TN_HANDLES_1),
    );


    let hdr_text = std::fs::read_to_string(
        repo.join("src").join("gc").join("host").join("mgc_board_rows.h"),
    )
    .unwrap_or_default();
    sc.accept_probe(
        "row-parse-genuine",
        "the real board-rows header parses to exactly 6 rows",
        matches!(parse_board_rows(&hdr_text), Ok(ref r) if r.len() == 6),
    );

    let mk = |region: u64, cover: u64, kheap: u64, profile: &str, basis: &str| {
        format!(
            "#if defined(MGC_HOST_ROW_TEST)\n#define MGC_ROW_BOARD \"st-stm32f769i-disco\"\n#define MGC_ROW_PROFILE \"{profile}\"\n#define MGC_ROW_BASIS \"{basis}\"\n#define MGC_ROW_REGION_BYTES {region}u\n#define MGC_ROW_COVER_BYTES {cover}u\n#define MGC_ROW_KHEAP_BYTES {kheap}u\n#define MGC_ROW_STACK_DEFAULT 16384u\n#define MGC_ROW_STACK_CLAMP 24576u\n#define MGC_ROW_HANDLE_SLOTS 64u\n#define MGC_ROW_FOH_POOL_BYTES 4096u\n#define MGC_ROW_MARKSTACK_ENTRIES 2048u\n#define MGC_ROW_FINQ_CAP 256u\n#define MGC_ROW_OOM_RESERVE 8192u\n#define MGC_ROW_WINDOW_BYTES 4096u\n#define MGC_ROW_WINDOW_FLOOR 256u\n#define MGC_ROW_WALL_RETRIES 4u\n#define MGC_ROW_FROZEN_SEGS 8u\n#endif\n"
        )
    };
    let gutted = mk(98304, 524288, 16384, "mini", "floor").replace("#define MGC_ROW_REGION_BYTES 98304u\n", "");
    sc.probe(
        "row-parse-gutted",
        "a row block missing its region define parsing as a row",
        parse_board_rows(&gutted).is_ok(),
    );

    let parse_one = |t: &str| parse_board_rows(t).ok().and_then(|r| r.into_iter().next());
    let genuine = parse_one(&mk(98304, 524288, 16384, "mini", "floor"));
    sc.accept_probe(
        "row-check-genuine-mini",
        "the mini row passes the table check against its own map geometry",
        matches!(genuine, Some(ref r) if row_check(r, 0x2002_0000, 384 * 1024, 128 * 1024, 16384).is_ok()),
    );
    let fat = parse_one(&mk(131072, 524288, 16384, "mini", "floor"));
    sc.probe(
        "row-floor-overflow",
        "a region overflowing kheap+region+aux past the map floor passing the floor rule",
        matches!(fat, Some(ref r) if row_check(r, 0x2002_0000, 384 * 1024, 128 * 1024, 16384).is_ok()),
    );
    let unbudgeted = parse_one(
        &mk(98304, 524288, 16384, "mini", "floor").replace("MGC_ROW_FOH_POOL_BYTES 4096u", "MGC_ROW_FOH_POOL_BYTES 65536u"),
    );
    sc.probe(
        "row-foh-unbudgeted",
        "a mini row whose FOH pool overflows the floor inequality passing the floor rule",
        matches!(unbudgeted, Some(ref r) if row_check(r, 0x2002_0000, 384 * 1024, 128 * 1024, 16384).is_ok()),
    );
    let unpaged = parse_one(
        &mk(98304, 524288, 16384, "mini", "floor").replace("MGC_ROW_FOH_POOL_BYTES 4096u", "MGC_ROW_FOH_POOL_BYTES 5120u"),
    );
    sc.probe(
        "row-foh-not-paged",
        "a FOH pool that is not whole pages passing the page rule",
        matches!(unpaged, Some(ref r) if row_check(r, 0x2002_0000, 384 * 1024, 128 * 1024, 16384).is_ok()),
    );
    let two_rows = |second: &str| {
        let first = mk(98304, 524288, 16384, "mini", "floor");
        let second = second.replace("#if defined(MGC_HOST_ROW_TEST)", "#elif defined(MGC_HOST_ROW_TEST2)");
        parse_board_rows(&format!("{first}{second}")).ok()
    };
    let doctored_uniform = two_rows(
        &mk(98304, 524288, 16384, "mini", "floor").replace("MGC_ROW_HANDLE_SLOTS 64u", "MGC_ROW_HANDLE_SLOTS 128u"),
    );
    sc.probe(
        "rows-uniform-doctored",
        "two rows differing in a declared-uniform field passing the uniform check",
        matches!(doctored_uniform, Some(ref rows) if rows.len() == 2 && rows_uniform(rows).is_ok()),
    );
    let per_row_foh = two_rows(
        &mk(98304, 524288, 16384, "mini", "floor").replace("MGC_ROW_FOH_POOL_BYTES 4096u", "MGC_ROW_FOH_POOL_BYTES 65536u"),
    );
    sc.accept_probe(
        "rows-foh-per-row",
        "two rows differing only in their FOH pools accepted by the uniform check",
        matches!(per_row_foh, Some(ref rows) if rows.len() == 2 && rows_uniform(rows).is_ok()),
    );
    let small_cover = parse_one(&mk(98304, 262144, 16384, "mini", "floor"));
    sc.probe(
        "row-cover-not-minimal",
        "a non-minimal cover passing the granule rule",
        matches!(small_cover, Some(ref r) if row_check(r, 0x2002_0000, 384 * 1024, 128 * 1024, 16384).is_ok()),
    );
    let genuine2 = parse_one(&mk(98304, 524288, 16384, "mini", "floor"));
    sc.probe(
        "row-kheap-mismatch",
        "a row kheap disagreeing with the board module's const passing the tie",
        matches!(genuine2, Some(ref r) if row_check(r, 0x2002_0000, 384 * 1024, 128 * 1024, 32768).is_ok()),
    );
    let fake_link = parse_one(&mk(98304, 524288, 16384, "mini", "link"));
    sc.probe(
        "row-link-basis-doctored",
        "a link-basis row whose region is not the map floor passing the basis rule",
        matches!(fake_link, Some(ref r) if row_check(r, 0x2002_0000, 384 * 1024, 128 * 1024, 16384).is_ok()),
    );

    let cover_ok = b"[mgc-heap] card cover: floor=0x20000000 span=2196 KB cards=2260 of 4160\r\n".to_vec();
    sc.accept_probe(
        "cover-line-parse",
        "the genuine cover line parses to its exact tuple",
        parse_cover_line(&cover_ok).map(|t| t == ("0x20000000".to_string(), 2196, 2260, 4160)).unwrap_or(false),
    );
    let cover_break = b"[mgc-heap] card cover: floor=0x20000000 span=2196 KB cards=2260 of 2112\r\n".to_vec();
    sc.accept_probe(
        "cover-line-break-detected",
        "the doctored-cover shape reads as unaffordable (cards > buffer total)",
        parse_cover_line(&cover_break).map(|(_, _, c, o)| c > o).unwrap_or(false),
    );
    let cover_lie = b"[mgc-heap] card cover: floor=0x20000000 span=2196 KB cards=2000 of 4160\r\n".to_vec();
    sc.probe(
        "cover-line-arithmetic-doctored",
        "a cover line whose cards do not equal span+64 passing the recompute rule",
        parse_cover_line(&cover_lie).map(|(_, s, c, _)| c == s + 64).unwrap_or(true),
    );

    let refusal_err = "src/gc/host/mgc_board_rows.h:52:2: error: #error \"evidence postures never ride a row image (census/compact/multirange carry their own images and goldens)\"";
    sc.accept_probe(
        "posture-refusal-text",
        "the header guard's #error stderr recognized as the designed refusal",
        refusal_err.contains(ROW_REFUSAL_TEXT),
    );
    let clean_err = "src/gc/host/mgc_host.cpp: In function 'int main()':\nsome unrelated diagnostic\n";
    sc.probe(
        "posture-refusal-clean",
        "an unrelated compile diagnostic reading as the designed refusal",
        clean_err.contains(ROW_REFUSAL_TEXT),
    );

    let fp_ok = format!("{FP_INTACT_1}\r\n{FP_REUSE}\r\n{FP_INTACT_2}\r\n").into_bytes();
    sc.accept_probe(
        "free-pad-readings-in-order",
        "the genuine free-pad readings in drive order",
        in_order(&fp_ok, &[FP_INTACT_1, FP_REUSE, FP_INTACT_2]),
    );
    let fp_swapped = format!("{FP_INTACT_2}\r\n{FP_REUSE}\r\n{FP_INTACT_1}\r\n").into_bytes();
    sc.probe(
        "free-pad-readings-swapped",
        "the second sweep's reading before the first's",
        in_order(&fp_swapped, &[FP_INTACT_1, FP_REUSE, FP_INTACT_2]),
    );
    let fp_clobbered = b"[gc-host] free-pad: the live object's header word after sweep #1 reads 0x20036020 (CLOBBERED)\r\n".to_vec();
    sc.probe(
        "free-pad-clobbered-as-intact",
        "a CLOBBERED reading offered as the intact pin",
        contains_fixed(&fp_clobbered, FP_INTACT_1),
    );
    sc.accept_probe(
        "free-pad-clobbered-detected",
        "the forbidden sweep sees the CLOBBERED reading",
        contains_fixed(&fp_clobbered, "(CLOBBERED)"),
    );

    let hp_alias = b"[gc-host] handle worker 0 (tid 4): 800000 creates, 800000 destroys, 1 aliases\r\n".to_vec();
    sc.probe(
        "handle-pool-alias-as-clean",
        "a worker line carrying one alias offered as the clean pin",
        contains_fixed(&hp_alias, HP_WORKER_0),
    );
    sc.accept_probe(
        "handle-pool-alias-detected",
        "the forbidden sweep sees the aliased-handle line",
        contains_fixed(
            b"[gc-host] handle worker 0: the first aliased handle 0x20000ac0 read 0x20029020; its own object is 0x20028004\r\n",
            "aliased handle",
        ),
    );
    let hp_leak = HP_CENSUS.replace("live=0 of 64", "live=1 of 64").into_bytes();
    sc.probe(
        "handle-pool-census-leaked",
        "a census line with one slot left live after the storm",
        contains_fixed(&hp_leak, HP_CENSUS),
    );
    let hp_ok = format!("{HP_BANNER}\r\n{HP_SPAWN}\r\n{HP_WORKER_0}\r\n{HP_WORKER_1}\r\n{HP_CENSUS}\r\n{HP_DONE}\r\n").into_bytes();
    sc.accept_probe(
        "handle-pool-order",
        "the genuine handle-pool lines in drive order",
        in_order(&hp_ok, &[HP_BANNER, HP_SPAWN, HP_WORKER_0, HP_WORKER_1, HP_CENSUS, HP_DONE]),
    );
    let hp_swapped = format!("{HP_BANNER}\r\n{HP_SPAWN}\r\n{HP_CENSUS}\r\n{HP_WORKER_0}\r\n{HP_WORKER_1}\r\n{HP_DONE}\r\n").into_bytes();
    sc.probe(
        "handle-pool-order-swapped",
        "the pool's census before the workers' books",
        in_order(&hp_swapped, &[HP_BANNER, HP_SPAWN, HP_WORKER_0, HP_WORKER_1, HP_CENSUS, HP_DONE]),
    );
    let hp_short: Vec<String> = INVENTORY_HANDLES
        .iter()
        .filter(|h| **h != "IGCHeap::GarbageCollect")
        .map(|h| h.to_string())
        .collect();
    sc.probe(
        "handle-pool-inventory-short",
        "a call inventory missing the census collection's head",
        imports_match(&hp_short, &INVENTORY_HANDLES),
    );

    let stamp_obj = repo.join("goldens").join("gc").join("collect-row.o");
    let stamp_bytes = std::fs::read(&stamp_obj).unwrap_or_default();
    let stripped = tmp.join("stripped");
    let stamp_base = sha256_without_stamp(&facts, &stamp_obj, &stripped.join("genuine.o")).unwrap_or_default();
    let code_at = section_span_prefixed(&stamp_bytes, ".text").map(|(o, _)| o);
    let mut code_moved = stamp_bytes.clone();
    if let Some(at) = code_at {
        if let Some(b) = code_moved.get_mut(at) {
            *b ^= 0x01;
        }
    }
    write_fixture(&tmp.join("stamp-code-moved.o"), &code_moved);
    let code_sha = sha256_without_stamp(&facts, &tmp.join("stamp-code-moved.o"), &stripped.join("code-moved.o")).unwrap_or_default();
    sc.probe(
        "stamp-compare-code-byte",
        "a copy of a frozen object with one code byte moved, compared apart from the stamp",
        code_at.is_none() || stamp_base.is_empty() || code_sha.is_empty() || code_sha == stamp_base,
    );
    let stamp_at = section_span(&stamp_bytes, ".comment");
    let mut stamp_moved = stamp_bytes.clone();
    if let Some((o, s)) = stamp_at {
        if s > 2 {
            if let Some(b) = stamp_moved.get_mut(o + 1) {
                *b ^= 0x01;
            }
        }
    }
    write_fixture(&tmp.join("stamp-stamp-moved.o"), &stamp_moved);
    let stamp_sha = sha256_without_stamp(&facts, &tmp.join("stamp-stamp-moved.o"), &stripped.join("stamp-moved.o")).unwrap_or_default();
    sc.accept_probe(
        "stamp-compare-stamp-only",
        "a copy of a frozen object whose producer stamp alone is changed ties apart from the stamp while the files differ",
        stamp_at.is_some() && !stamp_base.is_empty() && stamp_sha == stamp_base && sha256_hex(&stamp_moved) != sha256_hex(&stamp_bytes),
    );

    sc.accept_probe(
        "class-flags-two-words",
        "the Cortex-M33 flag set is the Cortex-M7 set apart from the class's two words",
        class_flag_sets_tie(&CXX_FLAGS, &CXX_FLAGS_CORTEX_M33).is_ok(),
    );
    let mut doctored: Vec<&str> = CXX_FLAGS_CORTEX_M33.to_vec();
    if let Some(slot) = doctored.iter().position(|f| *f == "-O2") {
        doctored[slot] = "-O1";
    }
    sc.probe("class-flags-doctored", "the Cortex-M33 flag set with a third word moved offered as the class variant", class_flag_sets_tie(&CXX_FLAGS, &doctored).is_ok());

    sc.finish("genuine inputs accepted");
}
