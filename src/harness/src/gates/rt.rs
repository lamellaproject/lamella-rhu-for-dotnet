//! The rt gate on the runner: the hosted-runtime world's frozen tier
//! (goldens/rt-m7) -- the class-library program's compiler output merged
//! with the runtime archives at the pin into ONE relocatable object, the
//! C and math libraries' closure merged into another, the archive member
//! whose stack-cookie section the merge takes renamed, and the program's
//! boot golden -- verified host-side, then linked into the hosted image
//! and booted on the emulator. Nothing here compiles: the consumer story
//! is Rust, the pinned toolchain's own object tools and linker, and the
//! emulator; the merges are produced once, on the maintainer's
//! re-derivation leg ('rhu-gate rt-m7 rederive'), and committed. The module
//! runs one world per processor class: the Cortex-M7 world is the gate
//! 'rt-m7' on goldens/rt-m7 and the mps2-an500 machine, the Cortex-M33 world
//! the gate 'rt-m33' on goldens/rt-m33 and the mps2-an505 machine,
//! consuming the same two program objects from goldens/rt-m7 by sha; every
//! class-bound fact is a field of the world record.
//!

use crate::debugport::{doctor_event_bkpt, event_run, read_event_image, ArmImage, EventProgram, EventRun};
use crate::gate::{write_fixture, Selfcheck};
use crate::objfile::{producer_stamp_of, section_span, section_span_prefixed, sha256_without_stamp};
use crate::gates::gc::{parse_undefined, parse_vendored_manifest, resolve_ld, run_tool_bounded, ToolCmd};
use crate::gates::payload::{check_rt_placement, map_external_region, map_region, split_ld_log, RtBrackets, RT_HELLO_BRACKETS};
use crate::gates::echo;
use crate::gates::shim::markers_out_of_order;
use crate::image::{check_records, elf32_phdrs, header_abi, header_abi_compat};
use crate::results::{chain_json_from_env, validate_results, Results};
use crate::sha256::{sha256_file, sha256_hex};
use crate::text::{contains_fixed, cr_count, eq_lf_normalized, payload_stream, word_sym_present, word_sym_value};
use crate::tools::{cargo_build_kernel, cargo_build_kernel_at, command_words, echo_file, llvm_tool, resolve_qemu, run_captured, rustc_facts, Echo, ExecProfile, RustcFacts};
use std::path::{Path, PathBuf};
use std::process::exit;
use std::time::Duration;

const GATE: &str = "rt-m7";
const DISPLAY: &str = "RT-M7 GATE";
const HOST: &str = "qemu-mps2-an500";
const RUNNER: &str = "src/harness";
const RUN_TIMEOUT_S: u64 = 300;

/// The world a hosted-runtime gate runs: every fact the processor class
/// binds -- the gate's identity and file names, the frozen
/// tier and the tier the two program objects live in, the manifest pin,
/// the archive set, the donor and the CPU architecture the merges must
/// carry, the board map, the emulator machine, the kernel staticlib and
/// its build, the shim and collector objects the images link, the pointers
/// that resolve the archive set and the C library, whether the world
/// carries the STM32F769 rider, and the verdict's case count.
pub(crate) struct RtWorld {
    pub(crate) gate: &'static str,
    pub(crate) display: &'static str,
    pub(crate) host: &'static str,
    pub(crate) results: &'static str,
    pub(crate) rederive_gate: &'static str,
    pub(crate) rederive_display: &'static str,
    pub(crate) rederive_results: &'static str,
    pub(crate) selfcheck_gate: &'static str,
    pub(crate) selfcheck_display: &'static str,
    pub(crate) selfcheck_results: &'static str,
    /// The gate's output directory under src/kernel/target.
    pub(crate) out_dir: &'static str,
    /// The frozen tier's path, repo-relative.
    pub(crate) tier: &'static str,
    /// Where the two program objects live: the rt-m7 tier for every world
    /// (a world other than the Cortex-M7's consumes them by sha, never a
    /// copy, so a republish of the programs refuses that world's gate
    /// until its own re-freeze).
    pub(crate) program_tier: &'static str,
    pub(crate) pin: &'static str,
    pub(crate) archive_set: &'static str,
    pub(crate) set_file: &'static str,
    pub(crate) donor: &'static str,
    /// The CPU_arch build attribute the merged objects must carry.
    pub(crate) cpu_arch: &'static str,
    /// The THUMB_ISA_use build attribute the class's objects carry: the
    /// v7 encoding names the ISA level ("Thumb-2"); a v8-M object states
    /// the permission ("Permitted") and leaves the level to CPU_arch.
    pub(crate) thumb_isa: &'static str,
    pub(crate) map: &'static str,
    pub(crate) machine: &'static str,
    pub(crate) lib: &'static str,
    /// The staticlib's build: None builds the kernel's default target under
    /// the kernel's own target directory; Some((target dir, target,
    /// features)) builds under a directory of the gate's own.
    pub(crate) lib_build: Option<(&'static str, &'static str, &'static str)>,
    pub(crate) shim_objects: [&'static str; 15],
    /// The suite's markers in the one order the contract allows for this world.
    pub(crate) suite_markers: &'static [&'static str],
    pub(crate) gc_objects: [&'static str; 3],
    pub(crate) archives_env: &'static str,
    pub(crate) archives_local: &'static str,
    pub(crate) newlib_env: &'static str,
    pub(crate) newlib_local: &'static str,
    /// Whether the world links the STM32F769 rider case.
    pub(crate) rider: bool,
    /// The verdict's case count, as the line prints it.
    pub(crate) cases: &'static str,
    pub(crate) sdk_board: &'static str,
    pub(crate) sdk_gate: &'static str,
    pub(crate) sdk_display: &'static str,
    pub(crate) sdk_results: &'static str,
}

/// The Cortex-M7 world: the gate as frozen, every field a constant of
/// record.
pub(crate) const WORLD_M7: RtWorld = RtWorld {
    gate: GATE,
    display: DISPLAY,
    host: HOST,
    results: "rt-m7.results.jsonl",
    rederive_gate: "rt-m7-rederive",
    rederive_display: "RT-M7 REDERIVE",
    rederive_results: "rt-m7-rederive.results.jsonl",
    selfcheck_gate: "rt-m7-selfcheck",
    selfcheck_display: "RT-M7 SELFCHECK",
    selfcheck_results: "rt-m7-selfcheck.results.jsonl",
    out_dir: "rt-m7-gate",
    tier: "goldens/rt-m7",
    program_tier: "goldens/rt-m7",
    pin: SHA_FROZEN_MANIFEST_M7,
    archive_set: ARCHIVE_SET,
    set_file: SET_FILE,
    donor: DONOR,
    cpu_arch: "ARM v7E-M",
    thumb_isa: "Thumb-2",
    map: AN500_MAP,
    machine: "mps2-an500",
    lib: AN500_LIB,
    lib_build: None,
    shim_objects: SHIM_OBJECTS,
    gc_objects: GC_OBJECTS,
    suite_markers: &SUITE_MARKERS_M7,
    archives_env: "RHU_RT_ARCHIVES_M7",
    archives_local: "rtarchives-m7.local",
    newlib_env: "RHU_NEWLIB_M7",
    newlib_local: "newlib-m7.local",
    rider: true,
    cases: "29/29 cases incl. 21 negative controls",
    sdk_board: "arm-mps2-an500",
    sdk_gate: "rt-m7-sdk-parity",
    sdk_display: "RT-M7 SDK PARITY",
    sdk_results: "rt-m7-sdk-parity.results.jsonl",
};

/// The Cortex-M33 world: the gate `rt-m33` on the class's own frozen tier
/// and the mps2-an505 machine. It consumes the two program objects from
/// the Cortex-M7 tier by sha (the same compiler output, never copied),
/// merges them with the class's single-precision archive set through the
/// v8-M donor, links under the an505 map with the an505 row's shim and
/// collector objects and the class's staticlib, and closes over the
/// kit's v8-M C and math libraries. No board rider: the class's boards
/// are the payload gate's rows and the kernel's hardware runs.
pub(crate) const WORLD_M33: RtWorld = RtWorld {
    gate: "rt-m33",
    display: "RT-M33 GATE",
    host: "qemu-mps2-an505",
    results: "rt-m33.results.jsonl",
    rederive_gate: "rt-m33-rederive",
    rederive_display: "RT-M33 REDERIVE",
    rederive_results: "rt-m33-rederive.results.jsonl",
    selfcheck_gate: "rt-m33-selfcheck",
    selfcheck_display: "RT-M33 SELFCHECK",
    selfcheck_results: "rt-m33-selfcheck.results.jsonl",
    out_dir: "rt-m33-gate",
    tier: "goldens/rt-m33",
    program_tier: "goldens/rt-m7",
    pin: SHA_FROZEN_MANIFEST_M33,
    archive_set: "net11.0.0-rhu-cortex-m33-sgc-enabled",
    set_file: "archives-net11.0.0-rhu-cortex-m33-sgc-enabled.txt",
    donor: "attr-donor-v8m.bin",
    cpu_arch: "ARM v8-M Mainline",
    thumb_isa: "Permitted",
    map: "src/kernel/payload-mps2-an505.x",
    machine: "mps2-an505",
    lib: "src/kernel/target/rt-m33-gate/kernel/thumbv8m.main-none-eabihf/release/librhu_kernel.a",
    lib_build: Some(("src/kernel/target/rt-m33-gate/kernel", "thumbv8m.main-none-eabihf", "qemu-exit,board-mps2-an505,payload,entropy-accept-software,stack-watermark,net")),
    shim_objects: [
        "goldens/shim/shim-core-an505.o",
        "goldens/shim/shim-sysnative-an505.o",
        "goldens/shim/shim-libc-glue-an505.o",
        "goldens/shim/shim-stdio-sym-an505.o",
        "goldens/shim/shim-host-an505.o",
        "goldens/shim/shim-pal-surface-an505.o",
        "goldens/shim/shim-sockaddr-an505.o",
        "goldens/shim/shim-socket-an505.o",
        "goldens/shim/shim-netinfo-an505.o",
        "goldens/shim/shim-gpiod-an505.o",
        "goldens/shim/shim-busdev-an505.o",
        "goldens/shim/shim-pwmfs-an505.o",
        "goldens/shim/shim-iiofs-an505.o",
        "goldens/shim/shim-ttydev-an505.o",
        "goldens/shim/shim-ports-native-an505.o",
    ],
    gc_objects: ["goldens/gc/skel-row-an505.o", "goldens/gc/heap-row-an505.o", "goldens/gc/collect-row-an505.o"],
    suite_markers: &SUITE_MARKERS_M33,
    archives_env: "RHU_RT_ARCHIVES_M33",
    archives_local: "rtarchives-m33.local",
    newlib_env: "RHU_NEWLIB_M33",
    newlib_local: "newlib-m33.local",
    rider: false,
    cases: "28/28 cases incl. 21 negative controls",
    sdk_board: "arm-mps2-an505",
    sdk_gate: "rt-m33-sdk-parity",
    sdk_display: "RT-M33 SDK PARITY",
    sdk_results: "rt-m33-sdk-parity.results.jsonl",
};

/// The tier a frozen file lives in: the program objects' tier for the two
/// program objects, the world's own tier for everything else.
fn tier_of(w: &RtWorld, name: &str) -> &'static str {
    if name == PAYLOAD || name == SUITE_PAYLOAD {
        w.program_tier
    } else {
        w.tier
    }
}

/// The frozen tier's file list for a world: the names of record with the
/// world's own donor and set file.
fn frozen_files(w: &RtWorld) -> [(&'static str, &'static str); 16] {
    let mut files = FROZEN_FILES;
    for f in files.iter_mut() {
        if f.0 == DONOR {
            f.0 = w.donor;
        } else if f.0 == SET_FILE {
            f.0 = w.set_file;
        }
    }
    files
}

/// The kit's C-library directory for a world: its environment variable,
/// else the first line of its untracked machine-local pointer file under
/// tools/toolchains/, else None (the shim resolver's shape).
fn resolve_kit_dir(repo: &Path, w: &RtWorld) -> Option<String> {
    if let Ok(v) = std::env::var(w.newlib_env) {
        if !v.trim().is_empty() {
            return Some(v.trim().to_string());
        }
    }
    let local = repo.join("tools").join("toolchains").join(w.newlib_local);
    let line = std::fs::read_to_string(&local)
        .ok()
        .and_then(|t| t.lines().next().map(str::to_string))
        .unwrap_or_default();
    if line.trim().is_empty() {
        None
    } else {
        Some(line.trim().to_string())
    }
}

/// goldens/rt-m7/MANIFEST.md's own sha256: the in-code anchor beside the
/// manifest.
pub(crate) const SHA_FROZEN_MANIFEST_M7: &str = "bc6b7469f40310affb5fe9874df34daaebfc4f2ab07c9030f73e210e4c51aebc";

/// goldens/rt-m33/MANIFEST.md's own sha256: the Cortex-M33 world's pin,
/// beside its manifest the way the Cortex-M7 world's is; a re-freeze of
/// that tier moves this line and nothing of the other world's.
pub(crate) const SHA_FROZEN_MANIFEST_M33: &str = "a53a5905404ec007e9ec5ae66f91ceaa2891c96fceba0ceabd6cdd4f240e0127";

/// The archive set the frozen merge consumed. The manifest's `archives:`
/// line and the frozen set file must both name it; a merge recorded under
/// another set is refused before its bytes are trusted.
const ARCHIVE_SET: &str = "net11.0.0-rhu-cortex-m7-sgc-enabled";

/// The frozen tier: the file names under goldens/rt-m7/ and what each is.
const FROZEN_FILES: [(&str, &str); 16] = [
    ("rthello.o", "the program object the merge consumed (the compiler's output of record)"),
    ("startup-cookie-copied.o", "the runtime archive's startup member with its stack-cookie section renamed into copied data"),
    ("rthello-merged.o", "the merged object: the program object, the surgery object and the runtime archives, the descriptor rooted, attributes neutralized"),
    ("attr-donor-v7em.bin", "the build-attributes blob the merges apply (a donor compiled at the image ABI)"),
    ("control-attr-a-profile.bin", "the archives' own merged attributes (application profile), the negative control's input"),
    ("archives-net11.0.0-rhu-cortex-m7-sgc-enabled.txt", "the archive set's identity: what built it, each archive by size and sha256, the set's demand and surface shas"),
    ("rthello-merged.undef.txt", "the merged object's undefined names, sorted: the demand surface pinned exactly"),
    ("rt-libc.o", "the C and math libraries' closure merged into one relocatable, attributes neutralized"),
    ("rt-libc.members.txt", "the closure's member list by archive and name, with the kit's two libraries by size and sha256"),
    ("rthello.golden.txt", "the payload stream of the hosted program's boot: the rt.boot golden"),
    ("rtsuite.o", "the suite's program object (the compiler's output of record for src/rt/gate/rtsuite)"),
    ("rtsuite-merged.o", "the suite's merged object: the same recipe from the suite's program object"),
    ("rtsuite-merged.undef.txt", "the suite's merged object's undefined names, sorted: its demand surface pinned exactly"),
    ("rtsuite.golden.txt", "the payload stream of the suite's boot: the rt.suite golden"),
    ("control-d32-site.o", "one function naming double registers above d15 (the application profile's 32-double register file): the census control's input"),
    ("control-ldrexd-site.o", "one 64-bit compare-and-swap compiled for the application profile, carrying the 64-bit exclusive-access forms: the census control's input"),
];
const PAYLOAD: &str = "rthello.o";
const SURGERY: &str = "startup-cookie-copied.o";
pub(crate) const MERGED: &str = "rthello-merged.o";
const DONOR: &str = "attr-donor-v7em.bin";
const CONTROL_ATTRS: &str = "control-attr-a-profile.bin";
const SET_FILE: &str = "archives-net11.0.0-rhu-cortex-m7-sgc-enabled.txt";
pub(crate) const UNDEF_FILE: &str = "rthello-merged.undef.txt";
const LIBC: &str = "rt-libc.o";
const LIBC_MEMBERS: &str = "rt-libc.members.txt";
const GOLDEN: &str = "rthello.golden.txt";
const SUITE_PAYLOAD: &str = "rtsuite.o";
const SUITE_MERGED: &str = "rtsuite-merged.o";
const SUITE_UNDEF_FILE: &str = "rtsuite-merged.undef.txt";
const SUITE_GOLDEN: &str = "rtsuite.golden.txt";
const CONTROL_D32: &str = "control-d32-site.o";
/// The exclusive-double census control: one 64-bit compare-and-swap
/// compiled for an application-profile core, which carries the two
/// 64-bit exclusive-access forms by construction.
const CONTROL_LDREXD: &str = "control-ldrexd-site.o";
/// The ARM-state code the Cortex-M7 class archive set still carries: libunwind's
/// context save (`unw_getcontext`, its private alias beside it) and its
/// resume jump, hand-written assembly that takes its instruction set from
/// the A-profile target the archives are built for (the source selects
/// Thumb only where the ARM instruction set is absent). Both are merged
/// and linked into the images and reached by nothing the images run (the
/// runtime unwinds through its own helpers; the suite's exception section
/// passes on the Cortex-M7). The census pins them by name and by count, so a
/// third ARM-state function is loud.
const ARM_MODE_PINNED: [&str; 3] = ["_ZN9libunwind13Registers_arm20restoreCoreAndJumpToEv", "__unw_getcontext", "unw_getcontext"];
/// The ARM-state mapping symbols the merged objects, the images and the
/// archive set carry: the two functions above.
const ARM_MODE_PINNED_COUNT: usize = 2;
/// The hand-written assembly the Cortex-M7 class set still carries that names a
/// double register above d15: libunwind's VFPv3 register save and restore
/// (one `vstmia`/`vldmia` of d16-d31 each), the neighbours of the ARM-state
/// pair in the same section, reached by nothing the images run. The
/// compiler's flag cannot touch assembly; the census pins these two by
/// name with one site each, so any COMPILED function naming d16-d31 is
/// loud.
const D32_PINNED: [&str; 2] = ["_ZN9libunwind13Registers_arm9saveVFPv3EPv", "_ZN9libunwind13Registers_arm12restoreVFPv3EPv"];
/// The A-profile decode of record for the census: the mapping symbols
/// keep literal pools out of the count, the register file is the full one
/// so a name above d15 decodes as itself.
const CENSUS_TRIPLE: &str = "--triple=thumbv7a-none-eabihf";
/// The ARM-state decode for the regions the mapping symbols mark `$a`: the
/// object reader's Thumb decode does not switch state inside a merged
/// object's section, so those regions are decoded again in ARM state and
/// the census reads each region with its own decode.
const CENSUS_TRIPLE_ARM: &str = "--triple=armv7a-none-eabihf";
const CENSUS_MATTR: &str = "--mattr=+vfp4,+d32,+neon,+fp-armv8";

/// The surgery: the archive member, the archive it lives in, and the
/// section rename that moves its read-only stack cookie into copied data
/// (the runtime writes the cookie once at initialization; on
/// flash-resident silicon that write faults).
const SURGERY_ARCHIVE: &str = "libRuntime.WorkstationGC.a";
const SURGERY_MEMBER: &str = "startup.cpp.o";
const SURGERY_RENAME: &str = ".rodata=.data.__security_cookie,alloc,load,data,contents";
const COOKIE: &str = "__security_cookie";
const COOKIE_SECTION: &str = ".data.__security_cookie";

/// The merge's inputs beyond the program object and the surgery object,
/// in the order the linker takes them: the workstation runtime, the
/// disabled event pipe, the standalone-GC selector, the minimal platform
/// layer, the C++ shim.
const MERGE_ARCHIVES: [&str; 5] = [
    "libRuntime.WorkstationGC.a",
    "libeventpipe-disabled.a",
    "libstandalonegc-enabled.a",
    "libaotminipal.a",
    "libstdc++compat.a",
];
/// The other selector: the rederive leg's control merges with it in the
/// selector's place and must not reproduce the frozen object.
const SELECTOR_CONTROL: &str = "libstandalonegc-disabled.a";
/// A name the standalone-GC-enabled selector defines and the disabled
/// one does not: the control object must lack it.
const SELECTOR_WITNESS: &str = "_Z22InitializeStandaloneGCv";

/// The symbols forced undefined at the merge so the archive members
/// defining them are taken in (group resolution pulls only what an
/// undefined name asks for): the registration the host calls but the
/// program object does not reference, and the data-contract descriptor
/// the debug adapter reads by symbol.
const MERGE_UNDEFINED: [&str; 2] = ["RhRegisterOSModule", "DotNetRuntimeContractDescriptor"];
const DESCRIPTOR: &str = "DotNetRuntimeContractDescriptor";

/// The build attributes the merged object must carry, tag by tag, as the
/// pinned toolchain's object reader prints them: the microcontroller
/// profile at the world's architecture, the world's Thumb ISA
/// description, hard-float VFP argument passing.
fn required_attrs(w: &RtWorld) -> [(&'static str, &'static str); 4] {
    [("CPU_arch_profile", "Microcontroller"), ("CPU_arch", w.cpu_arch), ("THUMB_ISA_use", w.thumb_isa), ("ABI_VFP_args", "AAPCS VFP")]
}

/// Defined names the host relies on, by their EXPORTED spelling: the
/// runtime's two entry points and its module-list initializer, the
/// managed entry, the eight class-library exports the host's table names,
/// the allocation fast and slow paths, the stack cookie the runtime
/// archive defines, and the data-contract descriptor's two symbols.
const SPOT_DEFINED: [&str; 17] = [
    "RhInitialize",
    "RhRegisterOSModule",
    "InitializeModules",
    "__managed__Main",
    "GetRuntimeException",
    "RuntimeFailFast",
    "ThreadEntryPoint",
    "AppendExceptionStackFrame",
    "ResolveDispatch",
    "GetSystemArrayEEType",
    "OnFirstChanceException",
    "OnUnhandledException",
    "RhpNewFast",
    "RhpGcAlloc",
    "__security_cookie",
    "DotNetRuntimeContractDescriptor",
    "DotNetRuntimeContractDescriptorPointerData",
];

/// The class-library exports of the build feature this runtime lacks:
/// the host leaves their four table slots null, and the merged object
/// must define none of them (a runtime that did would take a slot the
/// host does not fill).
const ABSENT_DEFINED: [&str; 4] = [
    "ObjectiveCMarshalTryGetTaggedMemory",
    "ObjectiveCMarshalGetIsTrackedReferenceCallback",
    "ObjectiveCMarshalGetOnEnteredFinalizerQueueCallback",
    "ObjectiveCMarshalGetUnhandledExceptionPropagationHandler",
];

/// Names that must NOT be defined in the merged object: the platform
/// layer is the shim's and the C runtime is the glue's.
const FORBIDDEN_DEFINED_PREFIX: &str = "SystemNative_";
const FORBIDDEN_DEFINED_EXACT: [&str; 2] = ["malloc", "pthread_create"];

/// The kit's two libraries the closure is drawn from.
const LIBC_ARCHIVES: [&str; 2] = ["libc_nano.a", "libm.a"];

/// Names the closure must NOT define: the image's glue owns them, and the
/// library's own version of any would mean two heaps or two consoles --
/// the allocation family and its reentrant spellings, the heap break, the
/// lock pair, the descriptor-level calls and the process exit.
const LIBC_FORBIDDEN: [&str; 24] = [
    "_calloc_r", "_close", "_exit", "_free_r", "_fstat", "_getpid", "_isatty", "_kill", "_lseek", "_malloc_r",
    "_memalign_r", "_open", "_read", "_realloc_r", "_sbrk", "_sbrk_r", "_write", "__malloc_lock", "__malloc_unlock",
    "calloc", "free", "malloc", "memalign", "realloc",
];

/// The rt.link line beyond the map and `--gc-sections`: the three roots
/// the initialization contract names -- the runtime's two entry points
/// and the descriptor nothing in the image references. No
/// read-only-after-relocation flag: the payload map places the GOT, the
/// constructor arrays and the TLS template as one run inside the copy
/// window, so the linker's segment over them is contiguous and the
/// checker reads it there, exactly as the payload gate's link-only cases
/// do.
/// The kernel's output level word the SDK's link always carries (the
/// property half of the log word: `--defsym=__rhu_log_level=<code>`, the SDK's default
/// quiet differing from the kernel's PROVIDE), at the gate project's own
/// value -- rthello says `verbose` in its project file, so 3. The link of
/// record carries the same word, so the parity tie proves the SDK's link
/// is the gate's line plus the level the project named.
const SDK_LOG_LEVEL_WORD: &str = "--defsym=__rhu_log_level=3";
const RT_LINK_WORDS: [&str; 6] = ["-u", "RhInitialize", "-u", "RhRegisterOSModule", "-u", "DotNetRuntimeContractDescriptor"];
const AN500_MAP: &str = "src/kernel/payload-mps2-an500.x";
const AN500_LIB: &str = "src/kernel/target/thumbv7em-none-eabihf/release/librhu_kernel.a";
/// The feature words of the Cortex-M7 world's kernel over the crate's defaults: the payload seam, the software
/// entropy the emulator row accepts, the stack watermark and the network.
const AN500_FEATURES: &str = "payload,entropy-accept-software,stack-watermark,net";
/// The frozen shim objects and collector row objects the image links,
/// in the order the line takes them.
const SHIM_OBJECTS: [&str; 15] = [
    "goldens/shim/shim-core.o",
    "goldens/shim/shim-sysnative.o",
    "goldens/shim/shim-libc-glue.o",
    "goldens/shim/shim-stdio-sym.o",
    "goldens/shim/shim-host.o",
    "goldens/shim/shim-pal-surface.o",
    "goldens/shim/shim-sockaddr.o",
    "goldens/shim/shim-socket.o",
    "goldens/shim/shim-netinfo.o",
    "goldens/shim/shim-gpiod.o",
    "goldens/shim/shim-busdev.o",
    "goldens/shim/shim-pwmfs.o",
    "goldens/shim/shim-iiofs.o",
    "goldens/shim/shim-ttydev.o",
    "goldens/shim/shim-ports-native.o",
];
const GC_OBJECTS: [&str; 3] = ["goldens/gc/skel-row.o", "goldens/gc/heap-row.o", "goldens/gc/collect-row.o"];

/// The diag axis's resource sentence: every rich-flavour class library
/// carries it as a UTF-16 literal, and a min-flavour object carries none
/// (the publish wrapper asserts the same both ways at artifact birth).
const FLAVOUR_MARK: &str = "Object reference not set to an instance";

/// The kernel's handshake for the frozen payload in the PLAIN form: the shim
/// tier is frozen at the header's generation, so the stated and the served
/// generations agree and the kernel prints neither the window's floor nor a
/// second number. The placeholder is filled at the run from include/rhu.h
/// (RHU_ABI), as the payload gate's entry line derives the same text -- an
/// ABI bump moves the header alone, never a literal here; a tier frozen
/// behind the header puts the window form back, with the tier's generation
/// as the one literal, until its re-freeze.
const ENTRY_MARKER: &str = "[payload] entry: payload-provided, built for hosting-API contract generation {abi} (this kernel: {abi})";

/// The markers with the handshake's placeholders filled from the header.
fn filled_markers(markers: &[&str], abi: u32, compat: u32) -> Vec<String> {
    markers.iter().map(|m| m.replace("{abi}", &abi.to_string()).replace("{compat}", &compat.to_string())).collect()
}

/// The boot's markers, in the ONE order the contract allows: the kernel's
/// handshake, the shim, the host's steps, the collector through the
/// dynamic-loading seam, the registration, the module list, managed
/// entry, the program's start, one collection, the verdict, the host's
/// exit line, the kernel's return.
const BOOT_MARKERS: [&str; 25] = [
    ENTRY_MARKER,
    "[shim] init: per-thread block registered as module 1 (48 bytes)",
    "[host] stdio: standard error bound to the C library's stream",
    "[host] stdio: standard output line buffered",
    "[host] data: initialized region verified against its load image",
    "[host] ctors run: 0 early, 6 main",
    "[shim] sigaction(11): handler recorded; the kernel's null-fault delivery is armed",
    "[shim] activation: signal 34 handler recorded; the kernel's activation delivery is armed",
    "[shim] pthread_create: stack 16384 B asked, 16384 B spawned (tid 4)",
    "[mgc] dlopen(\"rhu-mgc\") -> skeleton costume handle",
    "[mgc] dlsym(\"GC_VersionInfo\")",
    "[mgc] dlsym(\"GC_Initialize\")",
    "[mgc-skel] call IGCHeap::Initialize -> S_OK",
    "[host] runtime: initialized",
    "[host] runtime: managed code 351298 bytes, unboxing stubs 742 bytes",
    "[host] runtime: module registered, class-library slots: 12",
    "[host] runtime: modules initialized: 1",
    "[host] argv: 1",
    "[host] entering managed entry",
    "RTHELLO: start (args 0)",
    "[mgc-coll] world SUSPENDED (SuspendEE returned; heap is exclusively ours)",
    "[mgc-coll] world RESTARTED (RestartEE returned) -- collect #1 S_OK",
    "RTHELLO: ALL GOOD",
    "[host] activations delivered: 0",
    "[host] exit: 0",
];
const BOOT_RETURN_LINE: &str = "[payload] managed Main returned 0";
const BOOT_ALL_GOOD: &str = "RTHELLO: ALL GOOD";
/// Patterns a passing boot must not carry.
const FORBIDDEN_PATTERNS: [&str; 7] = ["FAIL ", "RTHELLO: FAIL", "KERNEL PANIC", "EXCEPTION:", "REFUSED", "abort()", "unsupported conversion"];

const PENDING: [&str; 0] = [];
/// The parity legs' cases that stand ahead of the tiers' re-freeze: the
/// program and its expected lines are in the tree and a desk run is their
/// proof; a leg prints each as pending and does not count it.
const SDK_PARITY_PENDING: [&str; 0] = [];

/// The suite's boot: the platform's lines around the program's (the
/// runtime's own thread spawned at its 16 KB ask, the threading section's
/// worker and the activation section's worker at the row's default),
/// every section's verdict line, the summary last.
const SUITE_MARKERS_M7: [&str; 82] = [
    ENTRY_MARKER,
    "[shim] sigaction(11): handler recorded; the kernel's null-fault delivery is armed",
    "[shim] activation: signal 34 handler recorded; the kernel's activation delivery is armed",
    "[shim] pthread_create: stack 16384 B asked, 16384 B spawned (tid 4)",
    "[host] entering managed entry",
    "RTSUITE: start",
    "PASS rt-smoke (array sort and string join)",
    "PASS rt-collections (hash map of 64, list of 128 filtered to 64)",
    "PASS rt-text (builder of 86, formats, parse, trim, compare)",
    "PASS rt-exceptions (nested finally, divide by zero, invalid cast, array bound)",
    "PASS rt-gc (4000 dead objects, two collections, weak nulled, strong kept)",
    "PASS rt-time (tick count monotonic across a spin, processor count)",
    "PASS rt-dispatch (interface dispatch, delegate, generic comparison)",
    "[shim] pthread_create: stack 0 B asked, 16384 B spawned (tid 5)",
    "PASS rt-threads (one worker thread, 10000 locked increments)",
    "[shim] pthread_create: stack 0 B asked, 16384 B spawned (tid 6)",
    "PASS rt-activation (a running worker suspended and resumed across three collections)",
    "[shim] pthread_create: stack 0 B asked, 16384 B spawned (tid 7)",
    "PASS rt-parked (a worker parked on a held monitor across two collections, released and joined)",
    "[shim] pthread_create: stack 0 B asked, 16384 B spawned (tid 8)",
    "PASS rt-interlocked (a long compared, exchanged, added and read above 32 bits; two threads, 10000 increments)",
    "PASS rt-math (truncate and round away from zero through the C library; pow, log2, cbrt, remainder)",
    "[shim] pthread_create: stack 0 B asked, 16384 B spawned (tid 9)",
    "[shim] pthread_create: stack 256 KB clamped to 24 KB (the row's clamp)",
    "[shim] pthread_create: stack 262144 B asked, 24576 B spawned (tid 10)",
    "[shim] gpiod: line 4 claim refused (EBUSY: the board or another claim holds it)",
    "[shim] gpiod: line 5 claim refused (EINVAL: past the space, unpopulated, unclaimed or a bad selector)",
    "PASS rt-gpio (the standard class over the doors: six lines, an output looped to an input read back, one edge to a callback and one to a waiter, two collections during the wait, the board's and an unpopulated line refused)",
    "[shim] /dev/i2c-1: a message of 256 bytes refused (EINVAL: the limit is 255)",
    "[shim] /dev/i2c-9: no such bus on this board (ENOENT)",
    "PASS rt-i2c (the standard class over the doors: the sensor model's identity and burst read in combined transactions, its reset written, a second descriptor on the bus; an unanswered address ENXIO, an oversize message EINVAL, a missing bus ENOENT)",
    "[shim] /dev/spidev2.0: mode 0, 8 bits, 1000000 Hz asked, 1000000 Hz achieved",
    "[shim] /dev/spidev2.1: mode 3, 8 bits, 500000 Hz asked, 500000 Hz achieved",
    "[shim] /dev/spidev2.0: mode 0, 8 bits, 1000000 Hz asked, 1000000 Hz achieved",
    "[shim] /dev/spidev2.0: least-significant-bit-first refused (EINVAL: the block sends the most significant bit first)",
    "[shim] /dev/spidev2.0: mode 0, 8 bits, 1000000 Hz asked, 1000000 Hz achieved",
    "[shim] /dev/spidev2.2: the transfer refused by the kernel (EINVAL: a select the bus does not have, or a rate below the block's slowest)",
    "[shim] /dev/spidev2.0: a word of 16 bits refused (EINVAL: the kernel clocks eight-bit frames)",
    "[shim] /dev/spidev2.0: an active-high select refused (EINVAL: the kernel drives the select low)",
    "[shim] /dev/spidev2.0: an active-high select refused (EINVAL: the kernel drives the select low)",
    "[shim] /dev/spidev9.0: no such bus on this board (ENOENT)",
    "PASS rt-spi (the standard class over the doors: the memory model's identity at the achieved rate; echoes of 4 and 512 bytes; a read shifting out ones; a second device, mode and rate; four refusals; LSB-first by the class's reversal)",
    // The PWM section's lines as the Cortex-M7 world's probe measured them (the shim's device-file lines and the verdict).
    "[shim] /sys/class/pwm/pwmchip0/export: line 0 exported (timer 1 channel 0, the timer's clock 1000000 Hz)",
    "[shim] /sys/class/pwm/pwmchip0/export: line 3 exported (timer 2 channel 0, the timer's clock 1000000 Hz)",
    "[shim] /sys/class/pwm/pwmchip0/export: line 1 is not a PWM line of this row (ENODEV)",
    "[shim] /sys/class/pwm/pwmchip0/export: line 0 exported (timer 1 channel 0, the timer's clock 1000000 Hz)",
    "[shim] /sys/class/pwm/pwmchip0/export: line 2 exported (timer 1 channel 1, the timer's clock 1000000 Hz)",
    "[shim] /sys/class/pwm/pwmchip0/pwm2/period: 500000 ns refused (EBUSY: another held channel of the same timer runs another period)",
    "[shim] /sys/class/pwm/pwmchip0/pwm2/period: 500000 ns refused (EBUSY: another held channel of the same timer runs another period)",
    "[shim] /sys/class/pwm/pwmchip0/export: line 2 exported (timer 1 channel 1, the timer's clock 1000000 Hz)",
    "[shim] /sys/class/pwm/pwmchip0/export: line 3 is held (EBUSY: as a GPIO line, by the board, or its timer output through another line)",
    "[shim] /sys/class/pwm/pwmchip0/export: line 3 exported (timer 2 channel 0, the timer's clock 1000000 Hz)",
    "PASS rt-pwm (the standard class over the sysfs files on the doors: the channel number is the line number; Start, duty, frequency and Stop; the quantized period; the refusals; the shared period EBUSY; a GPIO holder refuses and gives back)",
    // The null section's lines as the probe measured them: the six delivered faults -- every case once; the compiler's explicit null check is a read of the object's first byte -- then the verdict.
    "[shim] SIGSEGV: null access at 0x00000000 delivered to the runtime's handler",
    "[shim] SIGSEGV: null access at 0x00000004 delivered to the runtime's handler",
    "[shim] SIGSEGV: null access at 0x00000004 delivered to the runtime's handler",
    "[shim] SIGSEGV: null access at 0x00000000 delivered to the runtime's handler",
    "[shim] SIGSEGV: null access at 0x00000000 delivered to the runtime's handler",
    "[shim] SIGSEGV: null access at 0x00000000 delivered to the runtime's handler",
    "PASS rt-null (a null reference as the managed exception: a field read and write, an array element, an instance and an interface call each caught through the guard; a field past the unchecked window and a read at offset 0 caught the same)",
    // The serial section's lines as the freeze captured them on this world's wire (the shim's lines of the serial
    // device file -- the achieved-rate words are this row's -- then the verdict).
    "[shim] /dev/ttyS1: DTR and RTS are not wired on this port; the state is kept",
    "[shim] /dev/ttyS1: the port is held (EBUSY: the kernel's console or another opener)",
    "[shim] /dev/ttyS9: no such serial port on this board (ENOENT)",
    "[shim] /dev/ttyS1: RTS/CTS flow control is not available on this port (EINVAL)",
    "[shim] /dev/ttyS1: 115200 7N1 refused by the port (EINVAL: a rate the part misses or a frame it lacks; the door answered -22)",
    "[shim] /dev/ttyS1: 2000000 8N1 refused by the port (EINVAL: a rate the part misses or a frame it lacks; the door answered -22)",
    "[shim] /dev/ttyS1: 250000 8N1 (250000 Bd achieved)",
    "[shim] /dev/ttyS1: 921600 8N1 (925925 Bd achieved)",
    "[shim] /dev/ttyS1: 115200 8N1 (115207 Bd achieved)",
    "[shim] pthread_create: stack 0 B asked, 16384 B spawned (tid 12)",
    "[shim] pthread_create: stack 0 B asked, 16384 B spawned (tid 13)",
    "[shim] /dev/ttyS1: this port has no break control; the break is a no-op",
    "[shim] /dev/ttyS1: DTR and RTS are not wired on this port; the state is kept",
    "PASS rt-serial (the standard serial port class over the device files on the doors: two ports; 115200 8N1; the refusals by their exceptions; a silent read TimeoutException; 8 and 64 bytes echoed with DataReceived; the break a no-op; reopened)",
    // The analog section's lines as the freeze captured them on this world's wire (the shim's lines of the
    // IIO device files, then the verdict).
    "[shim] open(/sys/bus/iio/devices/iio:device0/in_voltage0_raw): the analog input is held (EBUSY: a program holds GPIO line 0 as a GPIO line, or this program already holds the channel)",
    "[shim] open(/sys/bus/iio/devices/iio:device0/in_voltage1_raw): the analog input is held (EBUSY: a program holds GPIO line 1 as a GPIO line, or this program already holds the channel)",
    "[shim] gpiod: line 1 claim refused (EBUSY: the board or another claim holds it)",
    "[shim] /sys/class/pwm/pwmchip0/export: line 0 exported (timer 1 channel 0, the timer's clock 1000000 Hz)",
    "PASS rt-adc (the standard analog controller over the IIO files: 819 at 12 bits on line 0; the voltage members not supported; a held line refused on both controllers; a held line or channel refuses the other; the disposal gives back; converter 1 by file)",
    "PASS rt-crypto (two draws of 32 secure random bytes served, not all zero and differing; a hash call throws CryptographicException on each call)",
    "PASS rt-named-mutex (a name is created once, found while a handle stands and gone with the last; two handles of a name are one mutex; a name that is not there is WaitHandleCannotBeOpenedException; a second thread has the mutex after the release; an owner thread that ended leaves it to the next waiter with AbandonedMutexException; a hundred rounds on one name are each created new; a full temporary folder is IOException and the name is made once there is room; Local is the plain name and Global another)",
    "RTSUITE: ALL GOOD",
];
const SUITE_MARKERS_M33: [&str; 82] = [
    ENTRY_MARKER,
    "[shim] sigaction(11): handler recorded; the kernel's null-fault delivery is armed",
    "[shim] activation: signal 34 handler recorded; the kernel's activation delivery is armed",
    "[shim] pthread_create: stack 16384 B asked, 16384 B spawned (tid 4)",
    "[host] entering managed entry",
    "RTSUITE: start",
    "PASS rt-smoke (array sort and string join)",
    "PASS rt-collections (hash map of 64, list of 128 filtered to 64)",
    "PASS rt-text (builder of 86, formats, parse, trim, compare)",
    "PASS rt-exceptions (nested finally, divide by zero, invalid cast, array bound)",
    "PASS rt-gc (4000 dead objects, two collections, weak nulled, strong kept)",
    "PASS rt-time (tick count monotonic across a spin, processor count)",
    "PASS rt-dispatch (interface dispatch, delegate, generic comparison)",
    "[shim] pthread_create: stack 0 B asked, 16384 B spawned (tid 5)",
    "PASS rt-threads (one worker thread, 10000 locked increments)",
    "[shim] pthread_create: stack 0 B asked, 16384 B spawned (tid 6)",
    "PASS rt-activation (a running worker suspended and resumed across three collections)",
    "[shim] pthread_create: stack 0 B asked, 16384 B spawned (tid 7)",
    "PASS rt-parked (a worker parked on a held monitor across two collections, released and joined)",
    "[shim] pthread_create: stack 0 B asked, 16384 B spawned (tid 8)",
    "PASS rt-interlocked (a long compared, exchanged, added and read above 32 bits; two threads, 10000 increments)",
    "PASS rt-math (truncate and round away from zero through the C library; pow, log2, cbrt, remainder)",
    // The GPIO section's lines as the Cortex-M7 world's freeze measured them (the section's shim lines and its verdict).
    "[shim] pthread_create: stack 0 B asked, 16384 B spawned (tid 9)",
    "[shim] pthread_create: stack 256 KB clamped to 24 KB (the row's clamp)",
    "[shim] pthread_create: stack 262144 B asked, 24576 B spawned (tid 10)",
    "[shim] gpiod: line 4 claim refused (EBUSY: the board or another claim holds it)",
    "[shim] gpiod: line 5 claim refused (EINVAL: past the space, unpopulated, unclaimed or a bad selector)",
    "PASS rt-gpio (the standard class over the doors: six lines, an output looped to an input read back, one edge to a callback and one to a waiter, two collections during the wait, the board's and an unpopulated line refused)",
    "[shim] /dev/i2c-1: a message of 256 bytes refused (EINVAL: the limit is 255)",
    "[shim] /dev/i2c-9: no such bus on this board (ENOENT)",
    "PASS rt-i2c (the standard class over the doors: the sensor model's identity and burst read in combined transactions, its reset written, a second descriptor on the bus; an unanswered address ENXIO, an oversize message EINVAL, a missing bus ENOENT)",
    "[shim] /dev/spidev1.0: mode 0, 8 bits, 1000000 Hz asked, 1000000 Hz achieved",
    "[shim] /dev/spidev1.1: mode 3, 8 bits, 500000 Hz asked, 500000 Hz achieved",
    "[shim] /dev/spidev1.0: mode 0, 8 bits, 1000000 Hz asked, 1000000 Hz achieved",
    "[shim] /dev/spidev1.0: least-significant-bit-first refused (EINVAL: the block sends the most significant bit first)",
    "[shim] /dev/spidev1.0: mode 0, 8 bits, 1000000 Hz asked, 1000000 Hz achieved",
    "[shim] /dev/spidev1.2: the transfer refused by the kernel (EINVAL: a select the bus does not have, or a rate below the block's slowest)",
    "[shim] /dev/spidev1.0: a word of 16 bits refused (EINVAL: the kernel clocks eight-bit frames)",
    "[shim] /dev/spidev1.0: an active-high select refused (EINVAL: the kernel drives the select low)",
    "[shim] /dev/spidev1.0: an active-high select refused (EINVAL: the kernel drives the select low)",
    "[shim] /dev/spidev9.0: no such bus on this board (ENOENT)",
    "PASS rt-spi (the standard class over the doors: the memory model's identity at the achieved rate; echoes of 4 and 512 bytes; a read shifting out ones; a second device, mode and rate; four refusals; LSB-first by the class's reversal)",
    // The PWM section's lines as the Cortex-M33 world's probe measured them (the same timers, the same lines).
    "[shim] /sys/class/pwm/pwmchip0/export: line 0 exported (timer 1 channel 0, the timer's clock 1000000 Hz)",
    "[shim] /sys/class/pwm/pwmchip0/export: line 3 exported (timer 2 channel 0, the timer's clock 1000000 Hz)",
    "[shim] /sys/class/pwm/pwmchip0/export: line 1 is not a PWM line of this row (ENODEV)",
    "[shim] /sys/class/pwm/pwmchip0/export: line 0 exported (timer 1 channel 0, the timer's clock 1000000 Hz)",
    "[shim] /sys/class/pwm/pwmchip0/export: line 2 exported (timer 1 channel 1, the timer's clock 1000000 Hz)",
    "[shim] /sys/class/pwm/pwmchip0/pwm2/period: 500000 ns refused (EBUSY: another held channel of the same timer runs another period)",
    "[shim] /sys/class/pwm/pwmchip0/pwm2/period: 500000 ns refused (EBUSY: another held channel of the same timer runs another period)",
    "[shim] /sys/class/pwm/pwmchip0/export: line 2 exported (timer 1 channel 1, the timer's clock 1000000 Hz)",
    "[shim] /sys/class/pwm/pwmchip0/export: line 3 is held (EBUSY: as a GPIO line, by the board, or its timer output through another line)",
    "[shim] /sys/class/pwm/pwmchip0/export: line 3 exported (timer 2 channel 0, the timer's clock 1000000 Hz)",
    "PASS rt-pwm (the standard class over the sysfs files on the doors: the channel number is the line number; Start, duty, frequency and Stop; the quantized period; the refusals; the shared period EBUSY; a GPIO holder refuses and gives back)",
    // The null section's lines as the probe measured them: the six delivered faults -- every case once; the compiler's explicit null check is a read of the object's first byte -- then the verdict.
    "[shim] SIGSEGV: null access at 0x00000000 delivered to the runtime's handler",
    "[shim] SIGSEGV: null access at 0x00000004 delivered to the runtime's handler",
    "[shim] SIGSEGV: null access at 0x00000004 delivered to the runtime's handler",
    "[shim] SIGSEGV: null access at 0x00000000 delivered to the runtime's handler",
    "[shim] SIGSEGV: null access at 0x00000000 delivered to the runtime's handler",
    "[shim] SIGSEGV: null access at 0x00000000 delivered to the runtime's handler",
    "PASS rt-null (a null reference as the managed exception: a field read and write, an array element, an instance and an interface call each caught through the guard; a field past the unchecked window and a read at offset 0 caught the same)",
    // The serial section's lines as the freeze captured them on this world's wire (the shim's lines of the serial
    // device file -- the achieved-rate words are this row's -- then the verdict).
    "[shim] /dev/ttyS1: DTR and RTS are not wired on this port; the state is kept",
    "[shim] /dev/ttyS1: the port is held (EBUSY: the kernel's console or another opener)",
    "[shim] /dev/ttyS9: no such serial port on this board (ENOENT)",
    "[shim] /dev/ttyS1: RTS/CTS flow control is not available on this port (EINVAL)",
    "[shim] /dev/ttyS1: 115200 7N1 refused by the port (EINVAL: a rate the part misses or a frame it lacks; the door answered -22)",
    "[shim] /dev/ttyS1: 2000000 8N1 refused by the port (EINVAL: a rate the part misses or a frame it lacks; the door answered -22)",
    "[shim] /dev/ttyS1: 250000 8N1 (250000 Bd achieved)",
    "[shim] /dev/ttyS1: 921600 8N1 (909090 Bd achieved)",
    "[shim] /dev/ttyS1: 115200 8N1 (114942 Bd achieved)",
    "[shim] pthread_create: stack 0 B asked, 16384 B spawned (tid 12)",
    "[shim] pthread_create: stack 0 B asked, 16384 B spawned (tid 13)",
    "[shim] /dev/ttyS1: this port has no break control; the break is a no-op",
    "[shim] /dev/ttyS1: DTR and RTS are not wired on this port; the state is kept",
    "PASS rt-serial (the standard serial port class over the device files on the doors: two ports; 115200 8N1; the refusals by their exceptions; a silent read TimeoutException; 8 and 64 bytes echoed with DataReceived; the break a no-op; reopened)",
    // The analog section's lines as the freeze captured them on this world's wire (the shim's lines of the
    // IIO device files, then the verdict).
    "[shim] open(/sys/bus/iio/devices/iio:device0/in_voltage0_raw): the analog input is held (EBUSY: a program holds GPIO line 0 as a GPIO line, or this program already holds the channel)",
    "[shim] open(/sys/bus/iio/devices/iio:device0/in_voltage1_raw): the analog input is held (EBUSY: a program holds GPIO line 1 as a GPIO line, or this program already holds the channel)",
    "[shim] gpiod: line 1 claim refused (EBUSY: the board or another claim holds it)",
    "[shim] /sys/class/pwm/pwmchip0/export: line 0 exported (timer 1 channel 0, the timer's clock 1000000 Hz)",
    "PASS rt-adc (the standard analog controller over the IIO files: 819 at 12 bits on line 0; the voltage members not supported; a held line refused on both controllers; a held line or channel refuses the other; the disposal gives back; converter 1 by file)",
    "PASS rt-crypto (two draws of 32 secure random bytes served, not all zero and differing; a hash call throws CryptographicException on each call)",
    "PASS rt-named-mutex (a name is created once, found while a handle stands and gone with the last; two handles of a name are one mutex; a name that is not there is WaitHandleCannotBeOpenedException; a second thread has the mutex after the release; an owner thread that ended leaves it to the next waiter with AbandonedMutexException; a hundred rounds on one name are each created new; a full temporary folder is IOException and the name is made once there is room; Local is the plain name and Global another)",
    "RTSUITE: ALL GOOD",
];
const SUITE_ALL_GOOD: &str = "RTSUITE: ALL GOOD";
/// Patterns a passing suite boot must not carry (the runtime's own
/// unhandled-exception report among them).
const SUITE_FORBIDDEN: [&str; 8] = ["FAIL ", "RTSUITE: FAIL", "Unhandled exception", "KERNEL PANIC", "EXCEPTION:", "REFUSED", "abort()", "unsupported conversion"];
/// The activation count the host prints at exit: the line must be present
/// and parse, and the count must reach the floor. The activation section
/// collects three times while a worker runs managed code, and each
/// collection suspends that worker through the shim's costume, so a
/// passing boot delivers at least three activations (the other threads
/// the runtime suspends add to the count and vary run to run; the floor
/// is the invariant). The kernel's activation resume completes the lazy
/// floating-point state a handler leaves pending before it pops a target
/// whose own frame is basic, which is what lets this section run.
const ACTIVATIONS_LINE: &str = "[host] activations delivered: ";
const ACTIVATIONS_FLOOR: u32 = 3;

/// The STM32F769 rider's world: the board's payload map and staticlib (built
/// as the payload gate builds it), and the row's tier objects -- the
/// shim's core and system-call objects under the STM32F769 selector, the
/// collector's heap and collect objects under the row's defines; the
/// rest shared with the an500 line.
const F769_BOARD: &str = "st-stm32f769i-disco";
const F769_TARGET: &str = "thumbv7em-none-eabihf";
/// The row's kernel is the one built with the network (the board table's
/// Kernel for the row; the second set, built without it, is the SDK's).
const F769_FEATURES: &str = "board-st-stm32f769i-disco,payload,net";
const F769_MAP: &str = "src/kernel/payload-st-stm32f769i-disco.x";
const F769_LIB: &str = "src/kernel/target/payload-gate/rt/st-stm32f769i-disco/thumbv7em-none-eabihf/release/librhu_kernel.a";
const F769_SHIM_OBJECTS: [&str; 15] = [
    "goldens/shim/shim-core-f769.o",
    "goldens/shim/shim-sysnative-f769.o",
    "goldens/shim/shim-libc-glue.o",
    "goldens/shim/shim-stdio-sym.o",
    "goldens/shim/shim-host.o",
    "goldens/shim/shim-pal-surface.o",
    "goldens/shim/shim-sockaddr.o",
    "goldens/shim/shim-socket.o",
    "goldens/shim/shim-netinfo.o",
    "goldens/shim/shim-gpiod.o",
    "goldens/shim/shim-busdev.o",
    "goldens/shim/shim-pwmfs.o",
    "goldens/shim/shim-iiofs.o",
    "goldens/shim/shim-ttydev.o",
    "goldens/shim/shim-ports-native.o",
];
const F769_GC_OBJECTS: [&str; 3] = ["goldens/gc/skel-row.o", "goldens/gc/heap-row-f769.o", "goldens/gc/collect-row-f769.o"];

/// The section classes the suite's frozen merged object carries at the
/// merge (the maps' brackets read them exactly).
// The suite's brackets as the tier's freeze measured them in both worlds (re-stamped at each re-freeze).
const SUITE_BRACKETS: RtBrackets = RtBrackets { managed: 1298926, unbox: 2054, hydrated: 342472, tdata: 152, init_array: 24, modules: 4 };
fn internal_cases(w: &RtWorld) -> [String; 2] {
    [
        format!("rt.rederive-selector-control -- link-tier; runs under 'rhu-gate {} rederive' when the archive set is resolved: the same merge with the standalone-GC-disabled selector must not tie the frozen object", w.gate),
        format!("rt.rederive-census -- link-tier; runs under 'rhu-gate {} rederive' when the archive set is resolved: every member of the five archives decoded, no compiled function naming a double register above d15 (the pinned assembly pair the only ones), the ARM-state code exactly the pinned pair, no 64-bit exclusive-access instruction in any member", w.gate),
    ]
}

/// The file header, as the object reader prints it: a 32-bit ARM
/// relocatable. Anything else is not a mergeable payload object.
fn header_fault(text: &str) -> Option<String> {
    let has = |needle: &str| text.lines().any(|l| l.trim().starts_with(needle));
    if !has("Class: 32-bit") {
        return Some("the object is not 32-bit (Class)".to_string());
    }
    if !has("Type: Relocatable") {
        return Some("the object is not relocatable (Type)".to_string());
    }
    if !has("Machine: EM_ARM") {
        return Some("the object is not ARM (Machine)".to_string());
    }
    None
}

/// The build attributes, read as (TagName, Description) pairs from the
/// object reader's output: a TagName line whose next line is a
/// Description line. Tags without a description (a CPU name) carry none.
fn parse_attrs(text: &str) -> Vec<(String, String)> {
    let lines: Vec<&str> = text.lines().map(str::trim).collect();
    let mut out = Vec::new();
    for i in 0..lines.len() {
        if let Some(tag) = lines[i].strip_prefix("TagName: ") {
            if let Some(desc) = lines.get(i + 1).and_then(|l| l.strip_prefix("Description: ")) {
                out.push((tag.to_string(), desc.to_string()));
            }
        }
    }
    out
}

/// Every required tag present with the required description; the first
/// miss names the tag and what was read (or its absence).
fn attrs_fault(text: &str, w: &RtWorld) -> Option<String> {
    let attrs = parse_attrs(text);
    for (tag, want) in required_attrs(w) {
        match attrs.iter().find(|(t, _)| t == tag) {
            None => return Some(format!("build attribute {tag} absent (wanted '{want}')")),
            Some((_, got)) if got != want => {
                return Some(format!("build attribute {tag} reads '{got}', wanted '{want}'"))
            }
            Some(_) => {}
        }
    }
    None
}

/// The build-attributes update as the runner's own byte operation --
/// the ELF32 relocatable's `.ARM.attributes` section contents replaced by
/// the donor blob, appended at the object's end (4-byte aligned) with the
/// section header re-pointed, every other byte of the object untouched.
/// Deterministic by construction: the same input bytes give the same
/// output bytes on every run and every desk.
pub(crate) fn attrs_update(pre: &[u8], donor: &[u8]) -> Result<Vec<u8>, String> {
    let (hdr, _, _) = elf_attrs_header(pre)?;
    let mut out = pre.to_vec();
    while out.len() % 4 != 0 {
        out.push(0);
    }
    let new_off = out.len() as u32;
    out.extend_from_slice(donor);
    out[hdr + 16..hdr + 20].copy_from_slice(&new_off.to_le_bytes());
    out[hdr + 20..hdr + 24].copy_from_slice(&(donor.len() as u32).to_le_bytes());
    Ok(out)
}

/// The `.ARM.attributes` section of a 32-bit little-endian ARM relocatable:
/// (its header's file offset, its contents' offset, its size).
fn elf_attrs_header(obj: &[u8]) -> Result<(usize, usize, usize), String> {
    let rd16 = |o: usize| -> Result<u16, String> {
        obj.get(o..o + 2).map(|b| u16::from_le_bytes([b[0], b[1]])).ok_or_else(|| format!("object truncated at offset {o}"))
    };
    let rd32 = |o: usize| -> Result<u32, String> {
        obj.get(o..o + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]])).ok_or_else(|| format!("object truncated at offset {o}"))
    };
    if obj.len() < 52 || &obj[0..4] != b"\x7fELF" {
        return Err("not an ELF object".to_string());
    }
    if obj[4] != 1 {
        return Err("not a 32-bit ELF object".to_string());
    }
    if obj[5] != 1 {
        return Err("not a little-endian ELF object".to_string());
    }
    if rd16(16)? != 1 {
        return Err("not a relocatable (ET_REL) object".to_string());
    }
    if rd16(18)? != 40 {
        return Err("not an ARM object".to_string());
    }
    let shoff = rd32(32)? as usize;
    let shentsize = rd16(46)? as usize;
    let shnum = rd16(48)? as usize;
    let shstrndx = rd16(50)? as usize;
    if shentsize < 40 || shnum == 0 || shstrndx >= shnum {
        return Err("section header table malformed".to_string());
    }
    let sh = |i: usize, field: usize| rd32(shoff + i * shentsize + field);
    let str_off = sh(shstrndx, 16)? as usize;
    let str_size = sh(shstrndx, 20)? as usize;
    let strtab = obj.get(str_off..str_off + str_size).ok_or_else(|| "section name table outside the file".to_string())?;
    let mut found: Option<usize> = None;
    for i in 0..shnum {
        let name_off = sh(i, 0)? as usize;
        let name: &[u8] = strtab.get(name_off..).map(|s| s.split(|&c| c == 0).next().unwrap_or(&[])).unwrap_or(&[]);
        if name == b".ARM.attributes" {
            if sh(i, 4)? != 0x7000_0003 {
                return Err("the .ARM.attributes section's type is not SHT_ARM_ATTRIBUTES".to_string());
            }
            if found.is_some() {
                return Err("two .ARM.attributes sections".to_string());
            }
            found = Some(i);
        }
    }
    let i = found.ok_or_else(|| "no .ARM.attributes section".to_string())?;
    Ok((shoff + i * shentsize, sh(i, 16)? as usize, sh(i, 20)? as usize))
}

/// The `.ARM.attributes` contents of an object, for the selfcheck.
fn attrs_section(obj: &[u8]) -> Option<Vec<u8>> {
    let (_, off, size) = elf_attrs_header(obj).ok()?;
    obj.get(off..off + size).map(|b| b.to_vec())
}

/// The file form: `post` written from `pre` with the donor applied; the
/// pre-object's sha256 and the donor's length returned for the transcript.
fn attrs_update_file(pre: &Path, donor: &Path, post: &Path) -> Result<(String, usize), String> {
    let pre_bytes = std::fs::read(pre).map_err(|e| format!("{} unreadable: {e}", pre.display()))?;
    let donor_bytes = std::fs::read(donor).map_err(|e| format!("{} unreadable: {e}", donor.display()))?;
    if donor_bytes.is_empty() {
        return Err(format!("{} is empty", donor.display()));
    }
    let out = attrs_update(&pre_bytes, &donor_bytes)?;
    let _ = std::fs::remove_file(post);
    std::fs::write(post, &out).map_err(|e| format!("{} unwritable: {e}", post.display()))?;
    Ok((sha256_hex(&pre_bytes), donor_bytes.len()))
}

/// True when the operand text names a double register above d15: a `d`
/// followed by 16..31, delimited by non-word characters.
fn names_high_double(operands: &str) -> bool {
    let b = operands.as_bytes();
    let is_word = |c: u8| c.is_ascii_alphanumeric() || c == b'_';
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'd' && (i == 0 || !is_word(b[i - 1])) {
            let mut j = i + 1;
            let mut n: u32 = 0;
            let mut digits = 0;
            while j < b.len() && b[j].is_ascii_digit() && digits < 3 {
                n = n * 10 + u32::from(b[j] - b'0');
                j += 1;
                digits += 1;
            }
            if digits > 0 && (j >= b.len() || !is_word(b[j])) && (16..=31).contains(&n) {
                return true;
            }
        }
        i += 1;
    }
    false
}

/// The d32 census over a disassembly (the A-profile decode with the
/// mapping symbols honoured): every function naming a double register
/// above d15 in an instruction's OPERANDS, with its site count. A decode
/// line is `<address>: <bytes><TAB><mnemonic><TAB><operands>`; an address
/// or a byte column never counts.
fn d32_census(dis: &str) -> Vec<(String, usize)> {
    let mut out: Vec<(String, usize)> = Vec::new();
    let mut cur = String::from("<?>");
    for line in dis.lines() {
        let t = line.trim_end_matches('\r');
        if t.ends_with(">:") && t.starts_with(|c: char| c.is_ascii_hexdigit()) {
            if let Some(p) = t.find(" <") {
                cur = t[p + 1..].to_string();
                continue;
            }
        }
        let mut fields = t.split('\t');
        let _addr_bytes = fields.next();
        let _mnemonic = fields.next();
        let operands = fields.next().unwrap_or("");
        if names_high_double(operands) {
            match out.iter_mut().find(|(f, _)| f == &cur) {
                Some(e) => e.1 += 1,
                None => out.push((cur.clone(), 1)),
            }
        }
    }
    out
}

/// The section names of a GNU-style section table dump
/// (`llvm-readobj --elf-output-style=GNU --sections`), by index.
fn section_names(sections: &str) -> Vec<(usize, String)> {
    let mut out = Vec::new();
    for line in sections.lines() {
        let t = line.trim_start();
        if !t.starts_with('[') {
            continue;
        }
        let Some(close) = t.find(']') else {
            continue;
        };
        let idx: usize = match t[1..close].trim().parse() {
            Ok(i) => i,
            Err(_) => continue,
        };
        let rest = t[close + 1..].trim_start();
        let name = rest.split_whitespace().next().unwrap_or("");
        if !name.is_empty() && name != "NULL" {
            out.push((idx, name.to_string()));
        }
    }
    out
}

/// The ARM-state regions of an object, by section name: from each `$a`
/// mapping symbol to the next mapping symbol of its section (or the
/// section's end). The Thumb decode of those bytes is meaningless, so the
/// census walks them with the ARM decode and everything else with the
/// Thumb decode.
fn arm_regions(syms: &str, sections: &str) -> Vec<(String, u64, u64)> {
    let names = section_names(sections);
    let mut mapping: Vec<(usize, u64, char)> = Vec::new();
    for line in syms.lines() {
        let f: Vec<&str> = line.split_whitespace().collect();
        if f.len() >= 8 && (f[7] == "$a" || f[7] == "$t" || f[7] == "$d") {
            if let (Ok(v), Ok(n)) = (u64::from_str_radix(f[1], 16), f[6].parse::<usize>()) {
                mapping.push((n, v, f[7].as_bytes()[1] as char));
            }
        }
    }
    mapping.sort();
    let mut out = Vec::new();
    for (i, (n, v, k)) in mapping.iter().enumerate() {
        if *k != 'a' {
            continue;
        }
        let end = mapping[i + 1..].iter().find(|(n2, _, _)| n2 == n).map(|(_, v2, _)| *v2).unwrap_or(u64::MAX);
        let name = names.iter().find(|(idx, _)| idx == n).map(|(_, s)| s.clone()).unwrap_or_default();
        out.push((name, *v, end));
    }
    out
}

/// One decode line's address and its region membership: the line is
/// `<address>: <bytes><TAB><mnemonic><TAB><operands>` under a
/// section header line the reader prints (its name the last word).
fn in_arm_region(section: &str, addr_bytes: &str, regions: &[(String, u64, u64)]) -> Option<bool> {
    let addr = u64::from_str_radix(addr_bytes.trim_start().split(':').next().unwrap_or(""), 16).ok()?;
    Some(regions.iter().any(|(s, lo, hi)| s == section && addr >= *lo && addr < *hi))
}

/// The d32 census over a disassembly restricted by the ARM-state regions:
/// `inside` counts only the instructions inside them (the ARM decode),
/// otherwise only the instructions outside them (the Thumb decode).
fn d32_census_regions(dis: &str, regions: &[(String, u64, u64)], inside: bool) -> Vec<(String, usize)> {
    let mut out: Vec<(String, usize)> = Vec::new();
    let mut cur = String::from("<?>");
    let mut section = String::new();
    for line in dis.lines() {
        let t = line.trim_end_matches('\r');
        if t.starts_with("Disassembly") && t.contains(" of section ") {
            section = t.rsplit(' ').next().unwrap_or("").trim_end_matches(':').to_string();
            continue;
        }
        if t.ends_with(">:") && t.starts_with(|c: char| c.is_ascii_hexdigit()) {
            if let Some(p) = t.find(" <") {
                cur = t[p + 1..].to_string();
                continue;
            }
        }
        let mut fields = t.split('\t');
        let addr_bytes = fields.next().unwrap_or("");
        let _mnemonic = fields.next();
        let operands = fields.next().unwrap_or("");
        let Some(in_arm) = in_arm_region(&section, addr_bytes, regions) else {
            continue;
        };
        if in_arm != inside {
            continue;
        }
        if names_high_double(operands) {
            match out.iter_mut().find(|(f, _)| f == &cur) {
                Some(e) => e.1 += 1,
                None => out.push((cur.clone(), 1)),
            }
        }
    }
    out
}

/// The double-precision sites of a disassembly outside the ARM-state
/// regions (the Thumb decode; the regions are hand-written assembly).
fn f64_census_regions(dis: &str, regions: &[(String, u64, u64)]) -> usize {
    let mut n = 0usize;
    let mut section = String::new();
    for line in dis.lines() {
        let t = line.trim_end_matches('\r');
        if t.starts_with("Disassembly") && t.contains(" of section ") {
            section = t.rsplit(' ').next().unwrap_or("").trim_end_matches(':').to_string();
            continue;
        }
        let mut fields = t.split('\t');
        let addr_bytes = fields.next().unwrap_or("");
        let mnemonic = fields.next().unwrap_or("");
        if in_arm_region(&section, addr_bytes, regions) == Some(false) && mnemonic.contains(".f64") {
            n += 1;
        }
    }
    n
}

/// One decode line's bytes as two Thumb halfwords: the reader prints a
/// 32-bit Thumb instruction as `<hw1> <hw2>` under a Thumb triple and as
/// four little-endian bytes under an ARM one; a 16-bit instruction has
/// one halfword and is never a 64-bit exclusive form.
fn thumb_halfwords(addr_bytes: &str) -> Option<(u32, u32)> {
    let bytes = addr_bytes.split(':').nth(1)?.trim();
    let toks: Vec<&str> = bytes.split_whitespace().collect();
    if toks.len() >= 2 && toks[0].len() == 4 && toks[1].len() == 4 {
        return Some((u32::from_str_radix(toks[0], 16).ok()?, u32::from_str_radix(toks[1], 16).ok()?));
    }
    if toks.len() >= 4 && toks.iter().take(4).all(|x| x.len() == 2) {
        let b: Vec<u32> = toks.iter().take(4).map(|x| u32::from_str_radix(x, 16).unwrap_or(0)).collect();
        return Some(((b[1] << 8) | b[0], (b[3] << 8) | b[2]));
    }
    None
}

/// Whether two Thumb halfwords encode a 64-bit exclusive-access
/// instruction: LDREXD (the first halfword's top twelve bits e8d, the
/// second's low byte 7f) or STREXD (e8c, the second's bits 4-7 the value
/// 7). The class's own decoder prints the forms as unknown with their
/// bytes, so the bytes are the instrument.
fn is_exclusive_double(hw1: u32, hw2: u32) -> bool {
    ((hw1 & 0xfff0) == 0xe8d0 && (hw2 & 0x00ff) == 0x007f) || ((hw1 & 0xfff0) == 0xe8c0 && (hw2 & 0x00f0) == 0x0070)
}

/// The 64-bit exclusive-access census over a disassembly restricted by
/// the ARM-state regions, by enclosing function: outside them (the Thumb
/// decode) by the instruction's bytes; inside them (the ARM decode) by
/// the mnemonic, since the ARM encodings differ.
fn xdouble_census_regions(dis: &str, regions: &[(String, u64, u64)], inside: bool) -> Vec<(String, usize)> {
    let mut out: Vec<(String, usize)> = Vec::new();
    let mut cur = String::from("<?>");
    let mut section = String::new();
    for line in dis.lines() {
        let t = line.trim_end_matches('\r');
        if t.starts_with("Disassembly") && t.contains(" of section ") {
            section = t.rsplit(' ').next().unwrap_or("").trim_end_matches(':').to_string();
            continue;
        }
        if t.ends_with(">:") && t.starts_with(|c: char| c.is_ascii_hexdigit()) {
            if let Some(p) = t.find(" <") {
                cur = t[p + 1..].to_string();
                continue;
            }
        }
        let mut fields = t.split('\t');
        let addr_bytes = fields.next().unwrap_or("");
        let mnemonic = fields.next().unwrap_or("").trim();
        let Some(in_arm) = in_arm_region(&section, addr_bytes, regions) else {
            continue;
        };
        if in_arm != inside {
            continue;
        }
        let hit = if inside { mnemonic == "ldrexd" || mnemonic == "strexd" } else { thumb_halfwords(addr_bytes).map_or(false, |(a, b)| is_exclusive_double(a, b)) };
        if hit {
            match out.iter_mut().find(|(f, _)| f == &cur) {
                Some(e) => e.1 += 1,
                None => out.push((cur.clone(), 1)),
            }
        }
    }
    out
}

/// The 64-bit exclusive-access census of an object: the Thumb decode
/// outside the ARM-state regions and the ARM decode inside them, merged
/// by function.
fn xdouble_total(dis_t: &str, dis_a: &str, regions: &[(String, u64, u64)]) -> Vec<(String, usize)> {
    let mut out = xdouble_census_regions(dis_t, regions, false);
    if !regions.is_empty() {
        for (f, n) in xdouble_census_regions(dis_a, regions, true) {
            match out.iter_mut().find(|(g, _)| g == &f) {
                Some(e) => e.1 += n,
                None => out.push((f, n)),
            }
        }
    }
    out
}

/// The 64-bit exclusive-access census of a decode with no ARM-state
/// region (the control object, the probes).
fn xdouble_census(dis: &str) -> Vec<(String, usize)> {
    xdouble_census_regions(dis, &[], false)
}

/// The d32 census against its pin: every function counted must be one of
/// the pinned assembly functions with exactly one site, and the pinned
/// pair must be whole when `expect_pinned` (the merged objects and the
/// archive set carry the pair; the closure carries none).
fn d32_fault(census: &[(String, usize)], expect_pinned: bool) -> Option<String> {
    for (f, n) in census {
        let name = f.trim_start_matches('<').trim_end_matches(':').trim_end_matches('>');
        if !D32_PINNED.contains(&name) {
            return Some(format!("{f} names a double register above d15 ({n} site(s)) and is not the pinned assembly pair -- the Cortex-M7's FPU has sixteen"));
        }
        if *n != 1 {
            return Some(format!("{f} names d16-d31 at {n} sites, the pin is one"));
        }
    }
    let want = if expect_pinned { D32_PINNED.len() } else { 0 };
    if census.len() != want {
        return Some(format!("{} function(s) naming d16-d31, the pin is {want}", census.len()));
    }
    None
}

/// The double-precision sites of a disassembly: every instruction whose
/// mnemonic carries the `.f64` type.
fn f64_census(dis: &str) -> usize {
    dis.lines().filter(|l| l.split('\t').nth(1).map_or(false, |m| m.contains(".f64"))).count()
}

/// The ARM-state mapping symbols of a GNU-style symbol table dump
/// (`llvm-readobj --elf-output-style=GNU --symbols`): (value, section
/// index) of every `$a`.
fn arm_mode_symbols(syms: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for line in syms.lines() {
        let f: Vec<&str> = line.split_whitespace().collect();
        if f.len() >= 8 && f[7] == "$a" {
            out.push((f[1].to_string(), f[6].to_string()));
        }
    }
    out
}

/// The function symbols at a (value, section index) of the same dump.
fn funcs_at(syms: &str, value: &str, ndx: &str) -> Vec<String> {
    syms.lines()
        .filter_map(|line| {
            let f: Vec<&str> = line.split_whitespace().collect();
            if f.len() >= 8 && f[3] == "FUNC" && f[1] == value && f[6] == ndx {
                Some(f[7].to_string())
            } else {
                None
            }
        })
        .collect()
}

/// The ARM-state census against the pin: exactly `want` mapping symbols,
/// each at a pinned function's address.
fn arm_mode_fault(arm: &[(String, String)], syms: &str, want: usize) -> Option<String> {
    if arm.len() != want {
        return Some(format!("ARM-state mapping symbols {}, the pin is {want} (Cortex-M executes Thumb only)", arm.len()));
    }
    for (v, n) in arm {
        let fs = funcs_at(syms, v, n);
        if !fs.iter().any(|f| ARM_MODE_PINNED.contains(&f.as_str())) {
            return Some(format!("an ARM-state mapping symbol at {v} (section {n}) with no pinned function there: [{}]", fs.join(" ")));
        }
    }
    None
}

/// The suite's capture with the host's activation count set to zero (the
/// delivery-suppressed control's input).
fn zero_delivery(raw: &[u8]) -> Vec<u8> {
    let text = String::from_utf8_lossy(raw);
    let Some(p) = text.find(ACTIVATIONS_LINE) else {
        return raw.to_vec();
    };
    let start = p + ACTIVATIONS_LINE.len();
    let end = start + text[start..].chars().take_while(|c| c.is_ascii_digit()).count();
    format!("{}0{}", &text[..start], &text[end..]).into_bytes()
}

/// The sorted undefined-name list against the frozen list, exactly.
fn undef_fault(got: &[String], want: &[String]) -> Option<String> {
    if let Some(extra) = got.iter().find(|n| !want.contains(n)) {
        return Some(format!("undefined name '{extra}' is not in the frozen demand list (a NEW demand)"));
    }
    if let Some(missing) = want.iter().find(|n| !got.contains(n)) {
        return Some(format!("frozen demand '{missing}' is no longer undefined (a VANISHED demand)"));
    }
    if got.len() != want.len() {
        return Some(format!("undefined count {} != frozen {}", got.len(), want.len()));
    }
    None
}

/// The manifest's `archives:` line (set id + archives sha256) against the
/// expected set id and the frozen set file's own `set-id:` and
/// `archives-sha256:` lines.
fn set_fault(manifest: &str, set_file: &str, want_id: &str) -> Option<String> {
    let line = manifest.lines().map(str::trim).find(|l| l.starts_with("archives: "));
    let Some(line) = line else {
        return Some("the manifest carries no 'archives:' line".to_string());
    };
    let mut it = line["archives: ".len()..].split_whitespace();
    let (Some(id), Some(sha)) = (it.next(), it.next()) else {
        return Some("the manifest's 'archives:' line lacks a set id or a sha".to_string());
    };
    if id != want_id {
        return Some(format!("the manifest names archive set '{id}', this gate consumes '{want_id}'"));
    }
    let file_id = set_file.lines().map(str::trim).find_map(|l| l.strip_prefix("set-id: "));
    let file_sha = set_file.lines().map(str::trim).find_map(|l| l.strip_prefix("archives-sha256: "));
    match (file_id, file_sha) {
        (Some(fid), _) if fid != want_id => Some(format!("the frozen set file names '{fid}', the gate consumes '{want_id}'")),
        (_, Some(fsha)) if fsha != sha => Some(format!("the frozen set file's archives sha256 {fsha} != the manifest's {sha}")),
        (None, _) | (_, None) => Some("the frozen set file lacks its set-id or archives-sha256 line".to_string()),
        _ => None,
    }
}

/// The manifest's records against the frozen file list, both directions,
/// and the on-disk files against the records.
fn frozen_consistency(w: &RtWorld, records: &[(String, String)], on_disk: &[String]) -> Result<(), String> {
    let files = frozen_files(w);
    if records.len() != files.len() {
        return Err(format!(
            "manifest parsed to {} records, the frozen tier holds {} -- a record set that drifted",
            records.len(),
            files.len()
        ));
    }
    for (name, _) in files {
        if !records.iter().any(|(p, _)| p == &format!("{}/{name}", tier_of(w, name))) {
            return Err(format!("frozen file {name} has no manifest record"));
        }
    }
    let own = format!("{}/", w.tier);
    let programs = format!("{}/", w.program_tier);
    for (path, _) in records {
        // A record lives in the world's tier, or -- for the two program
        // objects -- in the program objects' tier, consumed by sha.
        let name = match path.strip_prefix(&own) {
            Some(n) => n,
            None => match path.strip_prefix(&programs) {
                Some(n) if n == PAYLOAD || n == SUITE_PAYLOAD => n,
                _ => return Err(format!("manifest record '{path}' is outside {own}")),
            },
        };
        if !files.iter().any(|(n, _)| n == &name) {
            return Err(format!("manifest record '{path}' has no frozen-tier entry"));
        }
    }
    for name in on_disk {
        if !files.iter().any(|(n, _)| *n == name.as_str()) {
            return Err(format!("{}/{name} has NO manifest record (a file without a record)", w.tier));
        }
    }
    Ok(())
}

/// The activation count the host prints at exit, against the floor a boot
/// that suspended a running worker must reach: the count, or the complaint.
fn activation_count(raw: &[u8], floor: u32) -> Result<u32, String> {
    let text = String::from_utf8_lossy(raw);
    let Some(line) = text.lines().map(str::trim_end).find(|l| l.starts_with(ACTIVATIONS_LINE)) else {
        return Err(format!("the host's '{ACTIVATIONS_LINE}' line is absent"));
    };
    let n: u32 = match line[ACTIVATIONS_LINE.len()..].trim().parse() {
        Ok(n) => n,
        Err(_) => return Err(format!("the activation line carries no count: '{line}'")),
    };
    if n < floor {
        return Err(format!("{n} activations delivered, fewer than the {floor} a running worker suspended {floor} times must have received"));
    }
    Ok(n)
}

/// One record against the bytes on disk: the recorded sha must equal the
/// file's. A record that disagrees is a frozen artifact that moved.
fn record_fault(path: &str, recorded: &str, actual: &str) -> Option<String> {
    if recorded != actual {
        return Some(format!(
            "{path}: sha256 {actual} != recorded {recorded} (frozen artifact moved -- WRONG, never rebuilt; derivation is the world's rederive leg)"
        ));
    }
    None
}

/// Every spot name defined.
fn spot_fault(defined: &[String]) -> Option<String> {
    SPOT_DEFINED
        .iter()
        .find(|n| !defined.iter().any(|d| d == *n))
        .map(|n| format!("'{n}' is not defined in the merged object (the host references it by that name)"))
}

/// None of the absent feature's exports defined.
fn absent_fault(defined: &[String]) -> Option<String> {
    ABSENT_DEFINED
        .iter()
        .find(|n| defined.iter().any(|d| d == *n))
        .map(|n| format!("'{n}' is defined in the merged object: the runtime carries a build feature whose table slot the host leaves null"))
}

/// No platform-layer or C-runtime definition merged in. The compiler's
/// own direct-call thunks carry the platform verb's name with a `$thunk`
/// suffix and are the program object's, not a platform layer's: a name
/// with a `$` in it is never a platform definition.

fn forbidden_fault(defined: &[String]) -> Option<String> {
    defined
        .iter()
        .filter(|d| !d.contains('$'))
        .find(|d| d.starts_with(FORBIDDEN_DEFINED_PREFIX) || FORBIDDEN_DEFINED_EXACT.contains(&d.as_str()))
        .map(|d| format!("'{d}' is defined in the merged object: a platform layer or C runtime was merged (a world change)"))
}

/// The cookie's section in an object-dump symbol table: the line whose
/// last token is the cookie's name (the program's `$thunk` of the same
/// stem is another line) must name the renamed data section among its
/// tokens and no read-only one.
fn cookie_section_fault(dump: &str) -> Option<String> {
    let line = dump.lines().find(|l| l.split_whitespace().last() == Some(COOKIE));
    let Some(line) = line else {
        return Some(format!("'{COOKIE}' is not in the merged object's symbol table"));
    };
    let toks: Vec<&str> = line.split_whitespace().collect();
    if toks.iter().any(|t| *t == ".rodata" || t.starts_with(".rodata.")) {
        return Some(format!("'{COOKIE}' still sits in a read-only section: the surgery did not hold through the merge"));
    }
    if !toks.iter().any(|t| *t == COOKIE_SECTION) {
        return Some(format!("'{COOKIE}' is not in {COOKIE_SECTION} (the surgery's section)"));
    }
    None
}

/// The closure's defined names carry none of the names the glue owns.
fn libc_forbidden_fault(defined: &[String]) -> Option<String> {
    defined
        .iter()
        .find(|d| LIBC_FORBIDDEN.contains(&d.as_str()))
        .map(|d| format!("'{d}' is defined in the closure: the library's own heap, break, lock or descriptor layer would answer beside the image's"))
}

/// The closure's member list: the kit's archives by name, size and
/// sha256, and every member by archive and name.
struct LibcMembers {
    archives: Vec<(String, u64, String)>,
    members: Vec<(String, String)>,
}

/// Parse the member list. Every `archive` line names one of the kit's two
/// libraries with a size and a sha; every `member` line names one of the
/// listed archives; at least one member.
fn parse_members(text: &str) -> Result<LibcMembers, String> {
    let mut archives = Vec::new();
    let mut members = Vec::new();
    for raw in text.lines() {
        let l = raw.trim();
        if l.is_empty() || l.starts_with('#') {
            continue;
        }
        let toks: Vec<&str> = l.split_whitespace().collect();
        match toks.as_slice() {
            ["archive", name, size, sha] => {
                if !LIBC_ARCHIVES.contains(name) {
                    return Err(format!("the member list names an archive the closure is not drawn from: '{name}'"));
                }
                let size: u64 = size.parse().map_err(|_| format!("the member list's size for {name} is not a number"))?;
                if sha.len() != 64 || !sha.bytes().all(|b| b.is_ascii_hexdigit()) {
                    return Err(format!("the member list's sha for {name} is not 64 hex digits"));
                }
                archives.push((name.to_string(), size, sha.to_string()));
            }
            ["member", archive, name] => {
                if !archives.iter().any(|(a, _, _)| a == archive) {
                    return Err(format!("the member '{name}' names an archive the list does not identify: '{archive}'"));
                }
                members.push((archive.to_string(), name.to_string()));
            }
            _ => return Err(format!("a member-list line the reader does not handle: '{l}'")),
        }
    }
    if archives.len() != LIBC_ARCHIVES.len() {
        return Err(format!("the member list identifies {} archives, the closure is drawn from {}", archives.len(), LIBC_ARCHIVES.len()));
    }
    if members.is_empty() {
        return Err("the member list names no member".to_string());
    }
    Ok(LibcMembers { archives, members })
}

/// A symbol's address inside a half-open window.
fn window_fault(what: &str, at: u64, lo: u64, hi: u64, window: &str) -> Option<String> {
    if at < lo || at >= hi {
        return Some(format!("{what} at {at:#x} lies outside the {window} [{lo:#x}, {hi:#x})"));
    }
    None
}

/// The UTF-16 little-endian bytes of an ASCII string.
fn utf16le(s: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(s.len() * 2);
    for b in s.bytes() {
        out.push(b);
        out.push(0);
    }
    out
}

/// The diag axis's resource sentence must be absent from a min-flavour
/// image's bytes.
fn flavour_fault(elf: &[u8]) -> Option<String> {
    let needle = utf16le(FLAVOUR_MARK);
    if elf.windows(needle.len()).any(|w| w == needle.as_slice()) {
        return Some(format!("the resource sentence '{FLAVOUR_MARK}' is PRESENT in the image: a rich-flavour object, not the min flavour of record"));
    }
    None
}

/// Defined global names from a `--defined-only --extern-only` dump:
/// `<value> <type> <name>` lines.
fn parse_defined(dump: &str) -> Vec<String> {
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

/// The frozen demand list: one name per line, sorted at the freeze.
fn parse_list(text: &str) -> Vec<String> {
    let mut v: Vec<String> = text.lines().map(str::trim).filter(|l| !l.is_empty()).map(str::to_string).collect();
    v.sort();
    v
}

/// Weak undefined names of a linked image (`w` lines of `nm -u`).
fn parse_weak_undefined(dump: &str) -> Vec<String> {
    let mut out: Vec<String> = dump
        .lines()
        .filter_map(|l| {
            let mut it = l.split_whitespace();
            match (it.next(), it.next(), it.next()) {
                (Some("w"), Some(name), None) => Some(name.to_string()),
                (Some(_), Some("w"), Some(name)) => Some(name.to_string()),
                _ => None,
            }
        })
        .collect();
    out.sort();
    out.dedup();
    out
}

struct Rt {
    w: &'static RtWorld,
    /// RHU_ABI and RHU_ABI_COMPAT as include/rhu.h states them: the handshake marker's values.
    abi: u32,
    compat: u32,
    repo: PathBuf,
    out: PathBuf,
    results: Results,
    gate_id: &'static str,
    display: &'static str,
    /// The verified manifest records (repo-relative path, sha256).
    records: Vec<(String, String)>,
    manifest_text: String,
}

impl Rt {
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

    /// A failing negative control: the doctored input was ACCEPTED.
    fn fail_control(&mut self, case: &str, broke: &str, msg: &str) -> ! {
        self.results.emit(&format!(
            "{{\"schema\":1,\"type\":\"case\",\"gate\":\"{}\",\"case\":\"{case}\",\"verdict\":\"fail\",\"exit\":null,\"exit_want\":null,\"control\":\"negative\",\"image\":null,\"broke\":\"{broke}\"}}",
            self.gate_id
        ));
        self.results.tally("fail", "negative");
        println!("{}: FAIL -- {case}: {msg}", self.display);
        self.results.emit_summary(self.gate_id, "fail", 1);
        let _ = validate_results(&self.repo, self.results.path());
        exit(1);
    }

    /// A passing case line; a negative control names what it broke (the
    /// shared results checker requires it).

    fn pass(&mut self, case: &str, control: &str, broke: &str, stats: &str) {
        self.results.emit(&format!(
            "{{\"schema\":1,\"type\":\"case\",\"gate\":\"{}\",\"case\":\"{case}\",\"verdict\":\"pass\",\"exit\":null,\"exit_want\":null,\"control\":\"{control}\",\"image\":null{}{}}}",
            self.gate_id,
            if broke.is_empty() { String::new() } else { format!(",\"broke\":\"{broke}\"") },
            if stats.is_empty() { String::new() } else { format!(",\"stats\":{stats}") }
        ));
        self.results.tally("pass", control);
    }

    /// A passing case that produced or ran an image: the artifact's path
    /// and sha, the exit observed and wanted, the golden compared.
    fn pass_image(&mut self, case: &str, exit_code: i32, rel: &str, sha: &str, golden: Option<&str>, stats: &str) {
        let golden_frag = match golden {
            Some(g) => format!(
                ",\"golden\":{{\"path\":\"{}/{g}\",\"sha256\":\"{}\",\"compare\":\"bytes\"}}",
                self.tier_of(g),
                self.record_sha(g)
            ),
            None => String::new(),
        };
        self.results.emit(&format!(
            "{{\"schema\":1,\"type\":\"case\",\"gate\":\"{}\",\"case\":\"{case}\",\"verdict\":\"pass\",\"exit\":{exit_code},\"exit_want\":{exit_code},\"control\":\"positive\",\"image\":{{\"path\":\"{rel}\",\"sha256\":\"{sha}\"}}{golden_frag}{}}}",
            self.gate_id,
            if stats.is_empty() { String::new() } else { format!(",\"stats\":{stats}") }
        ));
        self.results.tally("pass", "positive");
    }

    fn tier_of(&self, name: &str) -> &'static str {
        tier_of(self.w, name)
    }

    fn frozen(&self, name: &str) -> PathBuf {
        self.repo.join(self.tier_of(name).replace('/', std::path::MAIN_SEPARATOR_STR)).join(name)
    }

    fn record_sha(&mut self, name: &str) -> String {
        let path = format!("{}/{name}", self.tier_of(name));
        match self.records.iter().find(|(p, _)| p == &path) {
            Some((_, s)) => s.clone(),
            None => self.fail("frozen-inputs", &format!("no record for {name}")),
        }
    }

    fn verify_frozen(&mut self) {
        println!("== frozen inputs: sha256 vs {}/MANIFEST.md ==", self.w.tier);
        let dir = self.repo.join(self.w.tier.replace('/', std::path::MAIN_SEPARATOR_STR));
        let mpath = dir.join("MANIFEST.md");
        let msha = match sha256_file(&mpath) {
            Ok(s) => s,
            Err(e) => self.fail(
                "frozen-inputs",
                &format!("{}/MANIFEST.md unreadable: {e} (the frozen tier's record; an exported copy without {} runs the chain with this tier DECLARED, never this gate)", self.w.tier, self.w.tier),
            ),
        };
        if msha != self.w.pin {
            self.fail(
                "frozen-inputs",
                &format!("{}/MANIFEST.md sha256 {msha} != pinned {} -- the frozen record moved without a registered re-freeze", self.w.tier, self.w.pin),
            );
        }
        let text = match std::fs::read_to_string(&mpath) {
            Ok(t) => t,
            Err(e) => self.fail("frozen-inputs", &format!("{}/MANIFEST.md unreadable: {e}", self.w.tier)),
        };
        let records = parse_vendored_manifest(&text);
        let mut on_disk: Vec<String> = match std::fs::read_dir(&dir) {
            Ok(rd) => rd
                .flatten()
                .filter_map(|e| e.file_name().to_str().map(str::to_string))
                .filter(|n| n != "MANIFEST.md")
                .collect(),
            Err(e) => self.fail("frozen-inputs", &format!("cannot list {}: {e}", self.w.tier)),
        };
        on_disk.sort();
        if let Err(e) = frozen_consistency(self.w, &records, &on_disk) {
            self.fail("frozen-inputs", &e);
        }
        for (path, want) in &records {
            let got = match sha256_file(&self.repo.join(path.replace('/', std::path::MAIN_SEPARATOR_STR))) {
                Ok(s) => s,
                Err(e) => self.fail("frozen-inputs", &format!("{path} unreadable: {e} (a record without a file)")),
            };
            if let Some(f) = record_fault(path, want, &got) {
                self.fail("frozen-inputs", &f);
            }
        }
        self.records = records;
        self.manifest_text = text;
        let stats = format!("{{\"manifest_sha\":\"{}\",\"files\":{}}}", self.w.pin, frozen_files(self.w).len());
        self.pass("frozen-inputs", "positive", "", &stats);
        println!(
            "== frozen inputs: PASS ({}/{} files tie the manifest both directions; the manifest ties the in-code pin) ==",
            frozen_files(self.w).len(),
            frozen_files(self.w).len()
        );
    }

    /// One object-reader or symbol-tool run, stdout captured to a file
    /// under the gate's output dir and returned as text.
    fn tool_text(&mut self, tool: &Path, case: &str, args: &[&str], out_name: &str) -> String {
        let out_p = self.out.join(out_name);
        let err_p = self.out.join(format!("{out_name}.stderr"));
        let run = run_captured(tool, args, &out_p, &err_p, Duration::from_secs(120));
        if !matches!(run, Ok(ref o) if o.exit == 0) {
            self.fail(case, &format!("{} failed ({})", tool.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default(), out_name));
        }
        std::fs::read_to_string(&out_p).unwrap_or_default()
    }

    /// One tool run whose stdout is the BYTES wanted (an archive member
    /// printed out of its archive), captured to a file under the gate's
    /// output dir; the path is returned.
    fn tool_bytes(&mut self, tool: &Path, case: &str, args: &[&str], out_name: &str) -> PathBuf {
        let out_p = self.out.join(out_name);
        let err_p = self.out.join(format!("{out_name}.stderr"));
        let run = run_captured(tool, args, &out_p, &err_p, Duration::from_secs(120));
        if !matches!(run, Ok(ref o) if o.exit == 0) {
            self.fail(case, &format!("{} failed ({})", tool.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default(), out_name));
        }
        out_p
    }

    fn case_merge(&mut self, facts: &RustcFacts) {
        let archive_set = self.w.archive_set;
        let case = "rt.merge";
        println!("== case {case} ==");
        let readobj = match llvm_tool(facts, "llvm-readobj") {
            Ok(p) => p,
            Err(e) => self.fail(case, &e),
        };
        let nm = match llvm_tool(facts, "llvm-nm") {
            Ok(p) => p,
            Err(e) => self.fail(case, &e),
        };
        let objdump = match llvm_tool(facts, "llvm-objdump") {
            Ok(p) => p,
            Err(e) => self.fail(case, &e),
        };
        let obj = self.frozen(MERGED);
        let obj_arg = obj.to_string_lossy().into_owned();
        let header = self.tool_text(&readobj, case, &["--file-headers", &obj_arg], "merged.fileheaders.txt");
        if let Some(f) = header_fault(&header) {
            self.fail(case, &f);
        }
        println!("  file header: 32-bit ARM relocatable");
        let attrs = self.tool_text(&readobj, case, &["--arch-specific", &obj_arg], "merged.attrs.txt");
        if let Some(f) = attrs_fault(&attrs, self.w) {
            self.fail(case, &f);
        }
        println!(
            "  build attributes: {}",
            required_attrs(self.w).iter().map(|(t, v)| format!("{t}={v}")).collect::<Vec<_>>().join(", ")
        );
        let set_text = match std::fs::read_to_string(self.frozen(self.w.set_file)) {
            Ok(t) => t,
            Err(e) => self.fail(case, &format!("{} unreadable: {e}", self.w.set_file)),
        };
        let manifest = self.manifest_text.clone();
        if let Some(f) = set_fault(&manifest, &set_text, self.w.archive_set) {
            self.fail(case, &f);
        }
        let set_sha = manifest
            .lines()
            .map(str::trim)
            .find(|l| l.starts_with("archives: "))
            .and_then(|l| l.split_whitespace().nth(2))
            .unwrap_or("")
            .to_string();
        println!("  archive set: {} ({}...) named by the manifest and the frozen set file alike", self.w.archive_set, &set_sha[..set_sha.len().min(16)]);
        let undef_dump = self.tool_text(&nm, case, &["-u", &obj_arg], "merged.nm-u.txt");
        let got_undef = parse_undefined(&undef_dump);
        let want_undef = match std::fs::read_to_string(self.frozen(UNDEF_FILE)) {
            Ok(t) => parse_list(&t),
            Err(e) => self.fail(case, &format!("{UNDEF_FILE} unreadable: {e}")),
        };
        if let Some(f) = undef_fault(&got_undef, &want_undef) {
            self.fail(case, &f);
        }
        println!("  demand surface: {} undefined names, equal to the frozen list both directions", got_undef.len());
        let atomics: Vec<&str> = got_undef.iter().map(String::as_str).filter(|n| n.starts_with("__sync_") && n.ends_with("_8")).collect();
        println!("  the demand's 64-bit atomic library calls: {} ({}), the compiler's lowering for the core, answered by the shim's core object", atomics.len(), atomics.join(" "));
        let def_dump = self.tool_text(&nm, case, &["--defined-only", "--extern-only", &obj_arg], "merged.nm-def.txt");
        let defined = parse_defined(&def_dump);
        if let Some(f) = spot_fault(&defined) {
            self.fail(case, &f);
        }
        if let Some(f) = absent_fault(&defined) {
            self.fail(case, &f);
        }
        if let Some(f) = forbidden_fault(&defined) {
            self.fail(case, &f);
        }
        println!(
            "  defined surface: {} global names; the {} spot names present by their exported spelling (the descriptor's two among them); the absent feature's {} exports absent; no platform layer merged",
            defined.len(),
            SPOT_DEFINED.len(),
            ABSENT_DEFINED.len()
        );
        let sym_dump = self.tool_text(&objdump, case, &["-t", &obj_arg], "merged.objdump-t.txt");
        if let Some(f) = cookie_section_fault(&sym_dump) {
            self.fail(case, &f);
        }
        println!("  the stack cookie: a data definition in {COOKIE_SECTION} (the surgery held through the merge)");
        let merged_sha = self.record_sha(MERGED);
        let stats = format!(
            "{{\"merged_sha\":\"{merged_sha}\",\"undefined\":{},\"defined\":{},\"archive_set\":\"{archive_set}\"}}",
            got_undef.len(),
            defined.len()
        );
        self.pass(case, "positive", "", &stats);
        println!("  PASS");
    }

    /// The A-profile control: the frozen object's copy with the archives'
    /// own attributes applied must refuse at the attribute check.
    fn control_attrs(&mut self, facts: &RustcFacts) {
        let case = "rt.merge-attrs-control";
        println!("== case {case} (negative control) ==");
        let readobj = match llvm_tool(facts, "llvm-readobj") {
            Ok(p) => p,
            Err(e) => self.fail(case, &e),
        };
        let copy = self.out.join("merged-attrs-control.o");
        let blob = self.frozen(CONTROL_ATTRS);
        let src = self.frozen(MERGED);
        if let Err(e) = attrs_update_file(&src, &blob, &copy) {
            self.fail(case, &format!("the control's attributes update failed: {e}"));
        }
        let dst = copy.to_string_lossy().into_owned();
        let attrs = self.tool_text(&readobj, case, &["--arch-specific", &dst], "attrs-control.attrs.txt");
        let broke = "the archives' application-profile attributes applied to a copy of the merged object";
        match attrs_fault(&attrs, self.w) {
            Some(f) => {
                println!("  refused as designed: {f}");
                self.pass(case, "negative", broke, "");
                println!("  PASS (control)");
            }
            None => self.fail_control(case, broke, "the application-profile attributes were ACCEPTED"),
        }
    }

    /// The set control: a manifest naming another archive set must refuse.
    fn control_set(&mut self) {
        let case = "rt.merge-set-control";
        println!("== case {case} (negative control) ==");
        let archive_set = self.w.archive_set;
        let other_set = format!("{}-disabled", archive_set.trim_end_matches("-enabled"));
        let doctored = self.manifest_text.replace(&format!("archives: {archive_set} "), &format!("archives: {other_set} "));
        if doctored == self.manifest_text {
            self.fail(case, "the control could not doctor the manifest's archives line");
        }
        let set_text = std::fs::read_to_string(self.frozen(self.w.set_file)).unwrap_or_default();
        let broke = "the manifest's archives line naming another set";
        match set_fault(&doctored, &set_text, self.w.archive_set) {
            Some(f) => {
                println!("  refused as designed: {f}");
                self.pass(case, "negative", broke, "");
                println!("  PASS (control)");
            }
            None => self.fail_control(case, broke, "a manifest naming another archive set was ACCEPTED"),
        }
    }

    /// The closure's identity, and none of the names the image's glue owns
    /// (one heap, one console) defined in it: the check over its
    /// definitions.
    fn case_libc(&mut self, facts: &RustcFacts) {
        let case = "rt.libc";
        println!("== case {case} ==");
        let readobj = match llvm_tool(facts, "llvm-readobj") {
            Ok(p) => p,
            Err(e) => self.fail(case, &e),
        };
        let nm = match llvm_tool(facts, "llvm-nm") {
            Ok(p) => p,
            Err(e) => self.fail(case, &e),
        };
        let obj = self.frozen(LIBC);
        let obj_arg = obj.to_string_lossy().into_owned();
        let header = self.tool_text(&readobj, case, &["--file-headers", &obj_arg], "libc.fileheaders.txt");
        if let Some(f) = header_fault(&header) {
            self.fail(case, &f);
        }
        let attrs = self.tool_text(&readobj, case, &["--arch-specific", &obj_arg], "libc.attrs.txt");
        if let Some(f) = attrs_fault(&attrs, self.w) {
            self.fail(case, &f);
        }
        println!("  file header: 32-bit ARM relocatable; build attributes at the image profile");
        let members_text = match std::fs::read_to_string(self.frozen(LIBC_MEMBERS)) {
            Ok(t) => t,
            Err(e) => self.fail(case, &format!("{LIBC_MEMBERS} unreadable: {e}")),
        };
        let members = match parse_members(&members_text) {
            Ok(m) => m,
            Err(e) => self.fail(case, &e),
        };
        let per_archive: Vec<String> = members
            .archives
            .iter()
            .map(|(a, size, sha)| format!("{a} ({size} B, {}...) {} members", &sha[..16], members.members.iter().filter(|(x, _)| x == a).count()))
            .collect();
        println!("  member list: {} members off the linker's trace: {}", members.members.len(), per_archive.join("; "));
        let def_dump = self.tool_text(&nm, case, &["--defined-only", "--extern-only", &obj_arg], "libc.nm-def.txt");
        let defined = parse_defined(&def_dump);
        if let Some(f) = libc_forbidden_fault(&defined) {
            self.fail(case, &f);
        }
        let undef_dump = self.tool_text(&nm, case, &["-u", &obj_arg], "libc.nm-u.txt");
        let undefined = parse_undefined(&undef_dump);
        println!(
            "  defined surface: {} global names, none of the {} the image's glue owns (the allocation family, the heap break, the lock pair, the descriptor layer); {} names the closure itself imports (resolved at rt.link)",
            defined.len(),
            LIBC_FORBIDDEN.len(),
            undefined.len()
        );
        let libc_sha = self.record_sha(LIBC);
        let stats = format!(
            "{{\"libc_sha\":\"{libc_sha}\",\"members\":{},\"defined\":{},\"undefined\":{}}}",
            members.members.len(),
            defined.len(),
            undefined.len()
        );
        self.pass(case, "positive", "", &stats);
        println!("  PASS");
    }

    /// The forbidden-name control: a defined list carrying the library's
    /// allocator entry must refuse.
    fn control_libc_forbidden(&mut self) {
        let case = "rt.libc-forbidden-control";
        println!("== case {case} (negative control) ==");
        let doctored: Vec<String> = ["log", "vfscanf", "_malloc_r"].iter().map(|s| s.to_string()).collect();
        let broke = "a closure defining the library's own allocator entry";
        match libc_forbidden_fault(&doctored) {
            Some(f) => {
                println!("  refused as designed: {f}");
                self.pass(case, "negative", broke, "");
                println!("  PASS (control)");
            }
            None => self.fail_control(case, broke, "the library's allocator entry was ACCEPTED in the closure"),
        }
    }

    /// The world's kernel staticlib: the kernel's default target under its
    /// own target directory, or the world's board target under a directory
    /// of the gate's own.
    fn build_lib(&self, log: &Path) -> Result<bool, String> {
        match self.w.lib_build {
            None => cargo_build_kernel(&self.repo, &["--lib", "--features", AN500_FEATURES], log),
            Some((dir, target, feats)) => {
                let target_dir = self.repo.join(dir.replace('/', std::path::MAIN_SEPARATOR_STR));
                let _ = std::fs::create_dir_all(&target_dir);
                cargo_build_kernel_at(&self.repo, &target_dir, &["--lib", "--target", target, "--no-default-features", "--features", feats], log)
            }
        }
    }

    /// The hosted image linked under the world's payload map: the link line
    /// of record, the artifact read.
    fn case_link(&mut self, facts: &RustcFacts, ld: &ToolCmd, abi: u32) -> (String, String) {
        let case = "rt.link";
        println!("== case {case} ==");
        let log = self.out.join("staticlib-build.log");
        let ok = match self.build_lib(&log) {
            Ok(ok) => ok,
            Err(e) => {
                println!("rhu-gate: {e}");
                false
            }
        };
        if !ok {
            echo_file(&log, "");
            self.fail(case, "kernel staticlib build error");
        }
        let path_rel = format!("src/kernel/target/{}/rt-link.elf", self.w.out_dir);
        let map_rel = format!("src/kernel/target/{}/rt-link.map", self.w.out_dir);
        let mut args: Vec<String> = vec!["-T".to_string(), self.w.map.to_string(), "--gc-sections".to_string(), format!("-Map={map_rel}")];
        args.extend(RT_LINK_WORDS.iter().map(|s| s.to_string()));
        args.push(format!("{}/{MERGED}", self.w.tier));
        args.extend(self.w.shim_objects.iter().map(|s| s.to_string()));
        args.extend(self.w.gc_objects.iter().map(|s| s.to_string()));
        args.push(format!("{}/{LIBC}", self.w.tier));
        args.push(self.w.lib.to_string());
        args.push("-o".to_string());
        args.push(path_rel.clone());
        let ld_out = self.out.join("rt-link.ld.stdout");
        let ld_log = self.out.join("rt-link.ld.stderr");
        match run_tool_bounded(&self.repo, ld, &args, &ld_out, &ld_log) {
            Ok(true) => {}
            Ok(false) => {
                echo_file(&ld_log, "  | ");
                self.fail(case, "the image did not link (see rt-link.ld.stderr)");
            }
            Err(e) => self.fail(case, &e),
        }
        let text = String::from_utf8_lossy(&std::fs::read(&ld_log).unwrap_or_default()).into_owned();
        let (names, other) = split_ld_log(&text);
        if !names.is_empty() || !other.is_empty() {
            for l in text.lines().take(12) {
                println!("  | {l}");
            }
            self.fail(case, &format!("the linker printed {} undefined-name and {} other diagnostic lines; the image of record links silently", names.len(), other.len()));
        }
        let elf = self.repo.join(path_rel.replace('/', std::path::MAIN_SEPARATOR_STR));
        let sha = match sha256_file(&elf) {
            Ok(s) => s,
            Err(e) => self.fail(case, &format!("cannot hash the linked artifact: {e}")),
        };
        let nm = match llvm_tool(facts, "llvm-nm") {
            Ok(p) => p,
            Err(e) => self.fail(case, &e),
        };
        let elf_arg = elf.to_string_lossy().into_owned();
        let undef_dump = self.tool_text(&nm, case, &["-u", &elf_arg], "rt-link.nm-u.txt");
        let undefined = parse_undefined(&undef_dump);
        if !undefined.is_empty() {
            self.fail(case, &format!("{} undefined name(s) remain in the linked image: {}", undefined.len(), undefined.join(" ")));
        }
        let weak = parse_weak_undefined(&undef_dump);
        println!("  every import resolved: no undefined name in the image; weak undefined (never referenced strongly): [{}]", weak.join(" "));
        let dump_text = self.tool_text(&nm, case, &[&elf_arg], "rt-link.nm.txt");
        let dump = dump_text.as_bytes();
        let elf_bytes = std::fs::read(&elf).unwrap_or_default();
        for s in ["__managed__Main", "__rhu_debug_layout", "__rhu_sched", "rhu_payload_entry", "rhu_payload_abi", "rhu_host_main", "rhu_shim_init", "__stack_chk_guard", DESCRIPTOR, COOKIE] {
            if !word_sym_present(dump, s) {
                self.fail(case, &format!("symbol '{s}' absent from the linked artifact"));
            }
        }
        let records = match check_records(dump, &elf_bytes, abi) {
            Ok(r) => r,
            Err(e) => self.fail(case, &format!("image records: {e}")),
        };
        println!(
            "  records: the header at {:#x} states load address {:#010x} and contract generation {}; the debug-layout record version {} at {:#x}",
            records.header_at, records.load_addr, records.abi, records.layout_version, records.layout_at
        );
        let Some(phdrs) = elf32_phdrs(&elf_bytes) else {
            self.fail(case, "program headers not parseable");
        };
        let map_text = std::fs::read_to_string(self.repo.join(self.w.map)).unwrap_or_default();
        let Some(code) = map_region(&map_text, "CODE") else {
            self.fail(case, &format!("{} states no CODE region", self.w.map));
        };
        let demand = match std::fs::read_to_string(self.frozen(UNDEF_FILE)) {
            Ok(t) => parse_list(&t),
            Err(e) => self.fail(case, &format!("{UNDEF_FILE} unreadable: {e}")),
        };
        let external = map_external_region(&map_text);
        let facts_p = match check_rt_placement(dump, &phdrs, &[], &demand, code, external, &RT_HELLO_BRACKETS) {
            Ok(f) => f,
            Err(e) => self.fail(case, &format!("placement: {e}")),
        };
        println!(
            "  placement: managed {} unbox {} hydrated {} tdata {} init {} modules {} exidx {} (each exact at its bracket); relro {} B inside the copy window; code free {} B; arena window {} B over the {} B floor",
            facts_p.managed, facts_p.unbox, facts_p.hydrated, facts_p.tdata, facts_p.init_array, facts_p.modules, facts_p.exidx,
            facts_p.relro, facts_p.code_free, facts_p.arena_window, facts_p.arena_floor
        );
        let sdata = word_sym_value(dump, "__sdata").unwrap_or(0);
        let edata = word_sym_value(dump, "__edata").unwrap_or(0);
        let desc_at = word_sym_value(dump, DESCRIPTOR).unwrap_or(0);
        let cookie_at = word_sym_value(dump, COOKIE).unwrap_or(0);
        if let Some(f) = window_fault("the data-contract descriptor", desc_at, sdata, edata, "copy window") {
            self.fail(case, &f);
        }
        if let Some(f) = window_fault("the stack cookie", cookie_at, sdata, edata, "copy window") {
            self.fail(case, &f);
        }
        println!("  the descriptor at {desc_at:#x} and the cookie at {cookie_at:#x} lie inside the copy window [{sdata:#x}, {edata:#x}); the stack guard present");
        if let Some(f) = flavour_fault(&elf_bytes) {
            self.fail(case, &f);
        }
        println!("  the diag axis: the resource sentence absent (a min-flavour image)");
        let size_tool = match llvm_tool(facts, "llvm-size") {
            Ok(p) => p,
            Err(e) => self.fail(case, &e),
        };
        let size_text = self.tool_text(&size_tool, case, &["-B", &elf_arg], "rt-link.size.txt");
        let footprint: Vec<u64> = size_text
            .lines()
            .last()
            .map(|l| l.split_whitespace().take(3).filter_map(|t| t.parse().ok()).collect())
            .unwrap_or_default();
        if footprint.len() != 3 {
            self.fail(case, "the footprint row could not be read off the size tool's output");
        }
        println!("  footprint: text {} data {} bss {} ({})", footprint[0], footprint[1], footprint[2], &sha[..16]);
        let stats = format!(
            "{{\"text\":{},\"data\":{},\"bss\":{},\"managed\":{},\"unbox\":{},\"hydrated\":{},\"weak_undefined\":{}}}",
            footprint[0], footprint[1], footprint[2], facts_p.managed, facts_p.unbox, facts_p.hydrated, weak.len()
        );
        self.pass_image(case, 0, &path_rel, &sha, None, &stats);
        println!("  PASS");
        (path_rel, sha)
    }

    /// The frozen-record control: a manifest record with one flipped
    /// digit must refuse at the compare the frozen-inputs case runs.
    fn control_link_frozen(&mut self) {
        let case = "rt.link-frozen-control";
        println!("== case {case} (negative control) ==");
        let want = self.record_sha(MERGED);
        let flipped = format!("{}{}", &want[..63], if want.ends_with('0') { "1" } else { "0" });
        let broke = "the merged object's manifest record with one flipped digit";
        match record_fault(&format!("{}/{MERGED}", self.w.tier), &flipped, &want) {
            Some(f) => {
                println!("  refused as designed: {}", f.split(" (frozen artifact").next().unwrap_or(&f));
                self.pass(case, "negative", broke, "");
                println!("  PASS (control)");
            }
            None => self.fail_control(case, broke, "a record with a flipped digit was ACCEPTED"),
        }
    }

    /// The boot: the image on the emulator, the golden, the markers.
    fn case_boot(&mut self, qemu: &Path, path_rel: &str, sha: &str) {
        let case = "rt.boot";
        println!("== case {case} ==");
        let elf = self.repo.join(path_rel.replace('/', std::path::MAIN_SEPARATOR_STR));
        let raw_path = self.out.join("rt-boot.raw");
        let stderr_path = self.out.join("rt-boot.stderr");
        let elf_arg = elf.to_string_lossy().into_owned();
        let mut args: Vec<&str> = vec!["-M", self.w.machine, "-display", "none", "-monitor", "none", "-serial", "stdio", "-semihosting"];
        args.extend_from_slice(ExecProfile::Parity.qemu_args());
        args.push("-kernel");
        args.push(&elf_arg);
        let outcome = match run_captured(qemu, &args, &raw_path, &stderr_path, Duration::from_secs(RUN_TIMEOUT_S)) {
            Ok(o) => o,
            Err(e) => self.fail(case, &e),
        };
        if outcome.timed_out {
            println!("  (run killed at its timeout; exit recorded as 124 -- a hang is observed, never a pass)");
        }
        let raw = std::fs::read(&raw_path).unwrap_or_default();
        echo_file(&raw_path, "  | ");
        let code = outcome.exit;
        if cr_count(&raw) == 0 {
            self.fail(case, "no CR bytes in raw capture (wire discipline lost)");
        }
        let golden = match std::fs::read(self.frozen(GOLDEN)) {
            Ok(b) => b,
            Err(e) => self.fail(case, &format!("golden unreadable: {e}")),
        };
        let stream = payload_stream(&raw);
        if !eq_lf_normalized(&stream, &golden) {
            self.fail(case, &format!("payload stream differs from {}/{GOLDEN} (byte class)", self.w.tier));
        }
        let boot_markers = filled_markers(&BOOT_MARKERS, self.abi, self.compat);
        let boot_marker_refs: Vec<&str> = boot_markers.iter().map(String::as_str).collect();
        if let Some(fault) = markers_out_of_order(&raw, &boot_marker_refs) {
            self.fail(case, &format!("the contract's order: {fault}"));
        }
        if !contains_fixed(&raw, BOOT_RETURN_LINE) {
            self.fail(case, &format!("the kernel's return line '{BOOT_RETURN_LINE}' not found"));
        }
        for m in FORBIDDEN_PATTERNS {
            if contains_fixed(&raw, m) {
                self.fail(case, &format!("forbidden pattern '{m}' present"));
            }
        }
        let last = String::from_utf8_lossy(&stream).lines().last().unwrap_or("").to_string();
        if last != BOOT_ALL_GOOD {
            self.fail(case, &format!("the payload stream's last line is not the verdict line (got '{last}')"));
        }
        if code != 0 {
            self.fail(case, &format!("exit {code}, wanted 0"));
        }
        let lines = String::from_utf8_lossy(&stream).lines().count();
        let stats = format!("{{\"stream_lines\":{lines},\"markers\":{}}}", BOOT_MARKERS.len());
        self.pass_image(case, 0, path_rel, sha, Some(GOLDEN), &stats);
        println!(
            "== case {case}: PASS (exit 0; the payload stream equals its golden, {lines} lines with the verdict last; {} contract markers IN ORDER, then the kernel's return line) ==",
            BOOT_MARKERS.len()
        );
    }

    /// The golden control: the golden with its verdict's GOOD changed to
    /// G00D must refuse the compare.
    fn control_boot_golden(&mut self) {
        let case = "rt.boot-golden-control";
        println!("== case {case} (negative control) ==");
        let golden = std::fs::read(self.frozen(GOLDEN)).unwrap_or_default();
        let flipped: Vec<u8> = String::from_utf8_lossy(&golden).replace("ALL GOOD", "ALL G00D").into_bytes();
        let broke = "the boot golden with its verdict line's GOOD changed to G00D";
        if flipped == golden {
            self.fail(case, "the control could not doctor the golden's verdict line");
        }
        if eq_lf_normalized(&flipped, &golden) {
            self.fail_control(case, broke, "the doctored golden was ACCEPTED by the compare");
        }
        println!("  refused as designed: the compare rejects the doctored verdict line");
        self.pass(case, "negative", broke, "");
        println!("  PASS (control)");
    }

    /// The STM32F769 rider: the frozen merged runtime object linked under the
    /// STM32F769 payload map with the STM32F769 payload staticlib and the row's own
    /// tier objects (the shim's core object under the STM32F769 selector, the
    /// collector's heap and collect objects under the STM32F769 row's defines;
    /// every other object shared with the an500 line, which the rederive
    /// legs prove row-independent). Link-only here: this artifact is what
    /// the board boots.
    fn case_link_board(&mut self, facts: &RustcFacts, ld: &ToolCmd, abi: u32) {
        let case = "rt.link-st-stm32f769i-disco";
        println!("== case {case} ==");
        let dir = self.repo.join("src/kernel/target/payload-gate/rt").join(F769_BOARD);
        let _ = std::fs::create_dir_all(&dir);
        let log = self.out.join(format!("rt-{F769_BOARD}-staticlib.log"));
        let ok = match cargo_build_kernel_at(
            &self.repo,
            &dir,
            &["--lib", "--target", F769_TARGET, "--no-default-features", "--features", F769_FEATURES],
            &log,
        ) {
            Ok(ok) => ok,
            Err(e) => {
                println!("rhu-gate: {e}");
                false
            }
        };
        if !ok {
            echo_file(&log, "");
            self.fail(case, "the board's kernel staticlib build error");
        }
        let path_rel = format!("src/kernel/target/{}/rt-link-{F769_BOARD}.elf", self.w.out_dir);
        let map_rel = format!("src/kernel/target/{}/rt-link-{F769_BOARD}.map", self.w.out_dir);
        let mut args: Vec<String> = vec!["-T".to_string(), F769_MAP.to_string(), "--gc-sections".to_string(), format!("-Map={map_rel}")];
        args.extend(RT_LINK_WORDS.iter().map(|s| s.to_string()));
        args.push(format!("{}/{MERGED}", self.w.tier));
        args.extend(F769_SHIM_OBJECTS.iter().map(|s| s.to_string()));
        args.extend(F769_GC_OBJECTS.iter().map(|s| s.to_string()));
        args.push(format!("{}/{LIBC}", self.w.tier));
        args.push(F769_LIB.to_string());
        args.push("-o".to_string());
        args.push(path_rel.clone());
        let ld_out = self.out.join(format!("rt-link-{F769_BOARD}.ld.stdout"));
        let ld_log = self.out.join(format!("rt-link-{F769_BOARD}.ld.stderr"));
        match run_tool_bounded(&self.repo, ld, &args, &ld_out, &ld_log) {
            Ok(true) => {}
            Ok(false) => {
                echo_file(&ld_log, "  | ");
                self.fail(case, "the board's image did not link (see the linker stderr log)");
            }
            Err(e) => self.fail(case, &e),
        }
        let text = String::from_utf8_lossy(&std::fs::read(&ld_log).unwrap_or_default()).into_owned();
        let (names, other) = split_ld_log(&text);
        if !names.is_empty() || !other.is_empty() {
            for l in text.lines().take(12) {
                println!("  | {l}");
            }
            self.fail(case, &format!("the linker printed {} undefined-name and {} other diagnostic lines; the board's image links silently", names.len(), other.len()));
        }
        let elf = self.repo.join(path_rel.replace('/', std::path::MAIN_SEPARATOR_STR));
        let sha = match sha256_file(&elf) {
            Ok(s) => s,
            Err(e) => self.fail(case, &format!("cannot hash the linked artifact: {e}")),
        };
        let nm = match llvm_tool(facts, "llvm-nm") {
            Ok(p) => p,
            Err(e) => self.fail(case, &e),
        };
        let elf_arg = elf.to_string_lossy().into_owned();
        let undef_dump = self.tool_text(&nm, case, &["-u", &elf_arg], &format!("rt-link-{F769_BOARD}.nm-u.txt"));
        let undefined = parse_undefined(&undef_dump);
        if !undefined.is_empty() {
            self.fail(case, &format!("{} undefined name(s) remain in the board's image: {}", undefined.len(), undefined.join(" ")));
        }
        let weak = parse_weak_undefined(&undef_dump);
        println!("  every import resolved: no undefined name in the image; weak undefined (never referenced strongly): [{}]", weak.join(" "));
        let dump_text = self.tool_text(&nm, case, &[&elf_arg], &format!("rt-link-{F769_BOARD}.nm.txt"));
        let dump = dump_text.as_bytes();
        let elf_bytes = std::fs::read(&elf).unwrap_or_default();
        for s in ["__managed__Main", "__rhu_debug_layout", "__rhu_sched", "rhu_payload_entry", "rhu_payload_abi", "rhu_host_main", "rhu_shim_init", "__stack_chk_guard", DESCRIPTOR, COOKIE] {
            if !word_sym_present(dump, s) {
                self.fail(case, &format!("symbol '{s}' absent from the board's artifact"));
            }
        }
        let records = match check_records(dump, &elf_bytes, abi) {
            Ok(r) => r,
            Err(e) => self.fail(case, &format!("image records: {e}")),
        };
        let map_text = std::fs::read_to_string(self.repo.join(F769_MAP)).unwrap_or_default();
        let Some(code) = map_region(&map_text, "CODE") else {
            self.fail(case, &format!("{F769_MAP} states no CODE region"));
        };
        if u64::from(records.load_addr) != code.0 {
            self.fail(case, &format!("the header states load address {:#010x}, the board's CODE origin is {:#010x}", records.load_addr, code.0));
        }
        println!(
            "  records: the header at {:#x} states load address {:#010x} (the board's CODE origin) and contract generation {}; the debug-layout record version {} at {:#x}",
            records.header_at, records.load_addr, records.abi, records.layout_version, records.layout_at
        );
        let Some(phdrs) = elf32_phdrs(&elf_bytes) else {
            self.fail(case, "program headers not parseable");
        };
        let demand = match std::fs::read_to_string(self.frozen(UNDEF_FILE)) {
            Ok(t) => parse_list(&t),
            Err(e) => self.fail(case, &format!("{UNDEF_FILE} unreadable: {e}")),
        };
        let external = map_external_region(&map_text);
        let facts_p = match check_rt_placement(dump, &phdrs, &[], &demand, code, external, &RT_HELLO_BRACKETS) {
            Ok(f) => f,
            Err(e) => self.fail(case, &format!("placement: {e}")),
        };
        println!(
            "  placement: managed {} unbox {} hydrated {} tdata {} init {} modules {} exidx {} (each exact at its bracket); relro {} B inside the copy window; code free {} B; arena window {} B over the {} B floor",
            facts_p.managed, facts_p.unbox, facts_p.hydrated, facts_p.tdata, facts_p.init_array, facts_p.modules, facts_p.exidx,
            facts_p.relro, facts_p.code_free, facts_p.arena_window, facts_p.arena_floor
        );
        let sdata = word_sym_value(dump, "__sdata").unwrap_or(0);
        let edata = word_sym_value(dump, "__edata").unwrap_or(0);
        let desc_at = word_sym_value(dump, DESCRIPTOR).unwrap_or(0);
        let cookie_at = word_sym_value(dump, COOKIE).unwrap_or(0);
        if let Some(f) = window_fault("the data-contract descriptor", desc_at, sdata, edata, "copy window") {
            self.fail(case, &f);
        }
        if let Some(f) = window_fault("the stack cookie", cookie_at, sdata, edata, "copy window") {
            self.fail(case, &f);
        }
        println!("  the descriptor at {desc_at:#x} and the cookie at {cookie_at:#x} lie inside the copy window [{sdata:#x}, {edata:#x}); the stack guard present");
        let size_tool = match llvm_tool(facts, "llvm-size") {
            Ok(p) => p,
            Err(e) => self.fail(case, &e),
        };
        let size_text = self.tool_text(&size_tool, case, &["-B", &elf_arg], &format!("rt-link-{F769_BOARD}.size.txt"));
        let footprint: Vec<u64> = size_text
            .lines()
            .last()
            .map(|l| l.split_whitespace().take(3).filter_map(|t| t.parse().ok()).collect())
            .unwrap_or_default();
        if footprint.len() != 3 {
            self.fail(case, "the footprint row could not be read off the size tool's output");
        }
        println!("  footprint: text {} data {} bss {} ({})", footprint[0], footprint[1], footprint[2], &sha[..16]);
        let stats = format!(
            "{{\"board\":\"{F769_BOARD}\",\"text\":{},\"data\":{},\"bss\":{},\"managed\":{},\"code_free\":{},\"arena_window\":{}}}",
            footprint[0], footprint[1], footprint[2], facts_p.managed, facts_p.code_free, facts_p.arena_window
        );
        self.pass_image(case, 0, &path_rel, &sha, None, &stats);
        println!("  PASS");
    }

    /// The suite: the frozen suite merge linked by the rt.link line in the
    /// an500 image's place, then booted -- every section's verdict line in
    /// order, the summary last, the payload stream equal to its golden,
    /// the host's activation count at exit read against its floor (one
    /// delivery per collection of the activation section). Returns the raw
    /// capture for the break-class control.
    fn case_suite(&mut self, facts: &RustcFacts, ld: &ToolCmd, qemu: &Path, abi: u32) -> Vec<u8> {
        let case = "rt.suite";
        println!("== case {case} ==");
        let path_rel = format!("src/kernel/target/{}/rt-suite.elf", self.w.out_dir);
        let map_rel = format!("src/kernel/target/{}/rt-suite.map", self.w.out_dir);
        let mut args: Vec<String> = vec!["-T".to_string(), self.w.map.to_string(), "--gc-sections".to_string(), format!("-Map={map_rel}")];
        args.extend(RT_LINK_WORDS.iter().map(|s| s.to_string()));
        args.push(format!("{}/{SUITE_MERGED}", self.w.tier));
        args.extend(self.w.shim_objects.iter().map(|s| s.to_string()));
        args.extend(self.w.gc_objects.iter().map(|s| s.to_string()));
        args.push(format!("{}/{LIBC}", self.w.tier));
        args.push(self.w.lib.to_string());
        args.push("-o".to_string());
        args.push(path_rel.clone());
        let ld_out = self.out.join("rt-suite.ld.stdout");
        let ld_log = self.out.join("rt-suite.ld.stderr");
        match run_tool_bounded(&self.repo, ld, &args, &ld_out, &ld_log) {
            Ok(true) => {}
            Ok(false) => {
                echo_file(&ld_log, "  | ");
                self.fail(case, "the suite's image did not link (see rt-suite.ld.stderr)");
            }
            Err(e) => self.fail(case, &e),
        }
        let text = String::from_utf8_lossy(&std::fs::read(&ld_log).unwrap_or_default()).into_owned();
        let (names, other) = split_ld_log(&text);
        if !names.is_empty() || !other.is_empty() {
            for l in text.lines().take(12) {
                println!("  | {l}");
            }
            self.fail(case, &format!("the linker printed {} undefined-name and {} other diagnostic lines; the suite's image links silently", names.len(), other.len()));
        }
        let elf = self.repo.join(path_rel.replace('/', std::path::MAIN_SEPARATOR_STR));
        let sha = match sha256_file(&elf) {
            Ok(s) => s,
            Err(e) => self.fail(case, &format!("cannot hash the linked artifact: {e}")),
        };
        let nm = match llvm_tool(facts, "llvm-nm") {
            Ok(p) => p,
            Err(e) => self.fail(case, &e),
        };
        let elf_arg = elf.to_string_lossy().into_owned();
        let undef_dump = self.tool_text(&nm, case, &["-u", &elf_arg], "rt-suite.nm-u.txt");
        let undefined = parse_undefined(&undef_dump);
        if !undefined.is_empty() {
            self.fail(case, &format!("{} undefined name(s) remain in the suite's image: {}", undefined.len(), undefined.join(" ")));
        }
        let dump_text = self.tool_text(&nm, case, &[&elf_arg], "rt-suite.nm.txt");
        let dump = dump_text.as_bytes();
        let elf_bytes = std::fs::read(&elf).unwrap_or_default();
        for s in ["__managed__Main", "rhu_host_main", "rhu_shim_init", "__stack_chk_guard", DESCRIPTOR, COOKIE] {
            if !word_sym_present(dump, s) {
                self.fail(case, &format!("symbol '{s}' absent from the suite's artifact"));
            }
        }
        let records = match check_records(dump, &elf_bytes, abi) {
            Ok(r) => r,
            Err(e) => self.fail(case, &format!("image records: {e}")),
        };
        let map_text = std::fs::read_to_string(self.repo.join(self.w.map)).unwrap_or_default();
        let Some(code) = map_region(&map_text, "CODE") else {
            self.fail(case, &format!("{} states no CODE region", self.w.map));
        };
        let Some(phdrs) = elf32_phdrs(&elf_bytes) else {
            self.fail(case, "program headers not parseable");
        };
        let demand = match std::fs::read_to_string(self.frozen(SUITE_UNDEF_FILE)) {
            Ok(t) => parse_list(&t),
            Err(e) => self.fail(case, &format!("{SUITE_UNDEF_FILE} unreadable: {e}")),
        };
        let external = map_external_region(&map_text);
        let facts_p = match check_rt_placement(dump, &phdrs, &[], &demand, code, external, &SUITE_BRACKETS) {
            Ok(f) => f,
            Err(e) => self.fail(case, &format!("placement: {e}")),
        };
        println!(
            "  link: contract generation {}; managed {} unbox {} hydrated {} (each exact at its bracket); code free {} B; arena window {} B over the {} B floor; the suite's demand list {} names",
            records.abi, facts_p.managed, facts_p.unbox, facts_p.hydrated, facts_p.code_free, facts_p.arena_window, facts_p.arena_floor, demand.len()
        );
        if let Some(f) = flavour_fault(&elf_bytes) {
            self.fail(case, &f);
        }
        let raw_path = self.out.join("rt-suite.raw");
        let stderr_path = self.out.join("rt-suite.stderr");
        let (chardev, echo) = self.echo_start(case);
        let mut qargs: Vec<&str> = vec!["-M", self.w.machine, "-display", "none", "-monitor", "none", "-serial", "stdio", "-serial", &chardev, "-semihosting"];
        qargs.extend_from_slice(ExecProfile::Parity.qemu_args());
        qargs.push("-kernel");
        qargs.push(&elf_arg);
        let outcome = match run_captured(qemu, &qargs, &raw_path, &stderr_path, Duration::from_secs(RUN_TIMEOUT_S)) {
            Ok(o) => o,
            Err(e) => self.fail(case, &e),
        };
        if outcome.timed_out {
            println!("  (run killed at its timeout; exit recorded as 124 -- a hang is observed, never a pass)");
        }
        let raw = std::fs::read(&raw_path).unwrap_or_default();
        self.echo_end(case, echo);
        echo_file(&raw_path, "  | ");
        let code_exit = outcome.exit;
        if cr_count(&raw) == 0 {
            self.fail(case, "no CR bytes in raw capture (wire discipline lost)");
        }
        let golden = match std::fs::read(self.frozen(SUITE_GOLDEN)) {
            Ok(b) => b,
            Err(e) => self.fail(case, &format!("golden unreadable: {e}")),
        };
        let stream = payload_stream(&raw);
        if !eq_lf_normalized(&stream, &golden) {
            self.fail(case, &format!("payload stream differs from {}/{SUITE_GOLDEN} (byte class)", self.w.tier));
        }
        let suite_markers = filled_markers(self.w.suite_markers, self.abi, self.compat);
        let suite_marker_refs: Vec<&str> = suite_markers.iter().map(String::as_str).collect();
        if let Some(fault) = markers_out_of_order(&raw, &suite_marker_refs) {
            self.fail(case, &format!("the suite's order: {fault}"));
        }
        if !contains_fixed(&raw, BOOT_RETURN_LINE) {
            self.fail(case, &format!("the kernel's return line '{BOOT_RETURN_LINE}' not found"));
        }
        for m in SUITE_FORBIDDEN {
            if contains_fixed(&raw, m) {
                self.fail(case, &format!("forbidden pattern '{m}' present"));
            }
        }
        let last = String::from_utf8_lossy(&stream).lines().last().unwrap_or("").to_string();
        if last != SUITE_ALL_GOOD {
            self.fail(case, &format!("the payload stream's last line is not the verdict line (got '{last}')"));
        }
        let activations = match activation_count(&raw, ACTIVATIONS_FLOOR) {
            Ok(n) => n,
            Err(e) => self.fail(case, &e),
        };
        if code_exit != 0 {
            self.fail(case, &format!("exit {code_exit}, wanted 0"));
        }
        let lines = String::from_utf8_lossy(&stream).lines().count();
        let stats = format!("{{\"stream_lines\":{lines},\"markers\":{},\"activations\":{activations},\"activations_floor\":{ACTIVATIONS_FLOOR}}}", self.w.suite_markers.len());
        self.pass_image(case, 0, &path_rel, &sha, Some(SUITE_GOLDEN), &stats);
        println!(
            "== case {case}: PASS (exit 0; the payload stream equals its golden, {lines} lines with the verdict last; {} markers IN ORDER incl. the seventeen section verdicts; the host reports {activations} activations delivered (at least {ACTIVATIONS_FLOOR}: one per collection of the activation section); then the kernel's return line) ==",
            self.w.suite_markers.len()
        );
        raw
    }

    /// rt.census: the M-profile facts over the frozen merged objects, the
    /// closure and the linked images (the module doc's paragraph).
    fn case_census(&mut self, facts: &RustcFacts) {
        let case = "rt.census";
        println!("== case {case}: no compiled function naming a double register above d15 in the merged objects and the closure (the pinned assembly pair the only ones); the ARM-state code exactly the pinned pair; the double-precision sites counted; no 64-bit exclusive-access instruction in the merged objects, the closure, the shim and collector objects or the linked images ==");
        let objdump = match llvm_tool(facts, "llvm-objdump") {
            Ok(p) => p,
            Err(e) => self.fail(case, &e),
        };
        let readobj = match llvm_tool(facts, "llvm-readobj") {
            Ok(p) => p,
            Err(e) => self.fail(case, &e),
        };
        let mut f64_total = 0usize;
        let mut parts: Vec<String> = Vec::new();
        for (name, arm_want) in [(MERGED, ARM_MODE_PINNED_COUNT), (SUITE_MERGED, ARM_MODE_PINNED_COUNT), (LIBC, 0usize)] {
            let path = self.frozen(name).to_string_lossy().into_owned();
            let syms = self.tool_text(&readobj, case, &["--elf-output-style=GNU", "--symbols", &path], &format!("census-{name}.syms.txt"));
            let sections = self.tool_text(&readobj, case, &["--elf-output-style=GNU", "--sections", &path], &format!("census-{name}.sections.txt"));
            let regions = arm_regions(&syms, &sections);
            let dis_t = self.tool_text(&objdump, case, &["-d", CENSUS_TRIPLE, CENSUS_MATTR, &path], &format!("census-{name}.dis.txt"));
            let thumb = d32_census_regions(&dis_t, &regions, false);
            if !thumb.is_empty() {
                let sites: usize = thumb.iter().map(|(_, n)| n).sum();
                self.fail(
                    case,
                    &format!("{name}: {} compiled function(s) name a double register above d15 ({sites} sites; the first: {} with {}) -- the Cortex-M7's FPU has sixteen", thumb.len(), thumb[0].0, thumb[0].1),
                );
            }
            let dis_a = if regions.is_empty() { String::new() } else { self.tool_text(&objdump, case, &["-d", CENSUS_TRIPLE_ARM, CENSUS_MATTR, &path], &format!("census-{name}.dis-arm.txt")) };
            let asm = if regions.is_empty() { Vec::new() } else { d32_census_regions(&dis_a, &regions, true) };
            if let Some(f) = d32_fault(&asm, name != LIBC) {
                self.fail(case, &format!("{name}: {f}"));
            }
            let xd = xdouble_total(&dis_t, &dis_a, &regions);
            if let Some((f, n)) = xd.first() {
                let sites: usize = xd.iter().map(|(_, n)| n).sum();
                self.fail(case, &format!("{name}: {} function(s) carry a 64-bit exclusive-access instruction ({sites} sites; the first: {f} with {n}) -- no Cortex-M executes the form", xd.len()));
            }
            let f64 = f64_census_regions(&dis_t, &regions);
            f64_total += f64;
            let arm = arm_mode_symbols(&syms);
            if let Some(f) = arm_mode_fault(&arm, &syms, arm_want) {
                self.fail(case, &format!("{name}: {f}"));
            }
            println!(
                "  {name}: compiled functions naming d16-d31: 0; the ARM-state assembly naming them: {} (the pinned pair, one site each); double-precision sites: {f64}; ARM-state regions: {}; ARM-state mapping symbols: {} ({}); 64-bit exclusive-access forms: 0",
                asm.len(),
                regions.len(),
                arm.len(),
                if arm.is_empty() { "none" } else { "the pinned libunwind pair" }
            );
            parts.push(format!("\"{name}\":{{\"d32_compiled\":0,\"d32_pinned_asm\":{},\"f64_sites\":{f64},\"arm_mode\":{},\"xdouble\":0}}", asm.len(), arm.len()));
        }
        // The shim and collector objects the images link: the 64-bit
        // exclusive-access census alone (their other facts are their own
        // gates').
        let mut linked = 0usize;
        for obj in self.w.shim_objects.iter().chain(self.w.gc_objects.iter()) {
            let path = self.repo.join(obj.replace('/', std::path::MAIN_SEPARATOR_STR)).to_string_lossy().into_owned();
            let short = obj.rsplit('/').next().unwrap_or(obj).to_string();
            let syms = self.tool_text(&readobj, case, &["--elf-output-style=GNU", "--symbols", &path], &format!("census-{short}.syms.txt"));
            let sections = self.tool_text(&readobj, case, &["--elf-output-style=GNU", "--sections", &path], &format!("census-{short}.sections.txt"));
            let regions = arm_regions(&syms, &sections);
            let dis_t = self.tool_text(&objdump, case, &["-d", CENSUS_TRIPLE, CENSUS_MATTR, &path], &format!("census-{short}.dis.txt"));
            let dis_a = if regions.is_empty() { String::new() } else { self.tool_text(&objdump, case, &["-d", CENSUS_TRIPLE_ARM, CENSUS_MATTR, &path], &format!("census-{short}.dis-arm.txt")) };
            let xd = xdouble_total(&dis_t, &dis_a, &regions);
            if let Some((f, n)) = xd.first() {
                self.fail(case, &format!("{obj}: {f} carries a 64-bit exclusive-access instruction ({n} site(s)) -- no Cortex-M executes the form"));
            }
            linked += 1;
        }
        println!("  the {linked} shim and collector objects the images link: 64-bit exclusive-access forms: 0");
        for img in ["rt-link.elf", "rt-suite.elf"] {
            let p = self.out.join(img);
            if !p.is_file() {
                self.fail(case, &format!("{img} is not on disk (the link cases run first)"));
            }
            let ps = p.to_string_lossy().into_owned();
            let syms = self.tool_text(&readobj, case, &["--elf-output-style=GNU", "--symbols", &ps], &format!("census-{img}.syms.txt"));
            let arm = arm_mode_symbols(&syms);
            if let Some(f) = arm_mode_fault(&arm, &syms, ARM_MODE_PINNED_COUNT) {
                self.fail(case, &format!("{img}: {f}"));
            }
            let sections = self.tool_text(&readobj, case, &["--elf-output-style=GNU", "--sections", &ps], &format!("census-{img}.sections.txt"));
            let regions = arm_regions(&syms, &sections);
            let dis_t = self.tool_text(&objdump, case, &["-d", CENSUS_TRIPLE, CENSUS_MATTR, &ps], &format!("census-{img}.dis.txt"));
            let dis_a = if regions.is_empty() { String::new() } else { self.tool_text(&objdump, case, &["-d", CENSUS_TRIPLE_ARM, CENSUS_MATTR, &ps], &format!("census-{img}.dis-arm.txt")) };
            let xd = xdouble_total(&dis_t, &dis_a, &regions);
            if let Some((f, n)) = xd.first() {
                let sites: usize = xd.iter().map(|(_, n)| n).sum();
                self.fail(case, &format!("{img}: {} function(s) carry a 64-bit exclusive-access instruction ({sites} sites; the first: {f} with {n}) -- no Cortex-M executes the form", xd.len()));
            }
            println!("  {img}: ARM-state mapping symbols: {} (the pinned libunwind pair, linked in, reached by nothing the images run); 64-bit exclusive-access forms: 0", arm.len());
            parts.push(format!("\"{img}\":{{\"arm_mode\":{},\"xdouble\":0}}", arm.len()));
        }
        let stats = format!("{{{}}}", parts.join(","));
        self.pass(case, "positive", "", &stats);
        println!("== case {case}: PASS (no compiled function naming d16-d31 in the merged objects or the closure, the two pinned assembly functions the only ones; {f64_total} double-precision sites over the three; the ARM-state code exactly the pinned pair in the merged objects and both images; no 64-bit exclusive-access instruction in the merged objects, the closure, the {linked} linked shim and collector objects or the images) ==");
    }

    /// The census control: the frozen object carrying one function that
    /// names registers above d15 must be refused by the census.
    fn control_census(&mut self, facts: &RustcFacts) {
        let case = "rt.census-control";
        println!("== case {case} (negative control): the frozen object with one function naming d16-d31 offered to the census ==");
        let objdump = match llvm_tool(facts, "llvm-objdump") {
            Ok(p) => p,
            Err(e) => self.fail(case, &e),
        };
        let path = self.frozen(CONTROL_D32).to_string_lossy().into_owned();
        let dis = self.tool_text(&objdump, case, &["-d", CENSUS_TRIPLE, CENSUS_MATTR, &path], "census-control.dis.txt");
        let d32 = d32_census(&dis);
        let broke = "a function compiled for the 32-double register file, naming registers above d15";
        let fault = match d32_fault(&d32, false) {
            Some(f) => f,
            None => self.fail_control(case, broke, "the census read NO unpinned site naming d16-d31 in the control object"),
        };
        let sites: usize = d32.iter().map(|(_, n)| n).sum();
        println!("  refused as designed: {} function(s), {sites} sites naming d16-d31 -- {fault}", d32.len());
        self.pass(case, "negative", broke, "");
        println!("  PASS (control)");
    }

    /// The exclusive-double census control: the frozen object carrying
    /// one 64-bit compare-and-swap compiled for the application profile
    /// must be refused by the census.
    fn control_census_ldrexd(&mut self, facts: &RustcFacts) {
        let case = "rt.census-ldrexd-control";
        println!("== case {case} (negative control): the frozen object with one 64-bit compare-and-swap compiled for the application profile offered to the census ==");
        let objdump = match llvm_tool(facts, "llvm-objdump") {
            Ok(p) => p,
            Err(e) => self.fail(case, &e),
        };
        let path = self.frozen(CONTROL_LDREXD).to_string_lossy().into_owned();
        let dis = self.tool_text(&objdump, case, &["-d", CENSUS_TRIPLE, CENSUS_MATTR, &path], "census-ldrexd-control.dis.txt");
        let xd = xdouble_census(&dis);
        let broke = "a 64-bit compare-and-swap compiled for the application profile, carrying the 64-bit exclusive-access forms";
        let Some((f, n)) = xd.first() else {
            self.fail_control(case, broke, "the census read NO 64-bit exclusive-access instruction in the control object");
        };
        let sites: usize = xd.iter().map(|(_, n)| n).sum();
        if xd.len() != 1 || sites < 2 {
            self.fail_control(case, broke, &format!("the census read {} function(s) and {sites} sites, one function with at least two sites expected", xd.len()));
        }
        println!("  refused as designed: {f} carries {n} 64-bit exclusive-access instruction(s) -- no Cortex-M executes the form");
        self.pass(case, "negative", broke, "");
        println!("  PASS (control)");
    }

    /// The suite image linked with one input substituted and booted once: a
    /// control's doctored input must be refused by the section's own checks
    /// (the section's FAIL lines the control names on the wire, and no ALL GOOD).
    /// The wire's echo for the suite's image, which talks on the machine's
    /// second serial port in its serial section: the chardev argument and
    /// the running echo, through the shared module.
    fn echo_start(&mut self, case: &str) -> (String, Echo) {
        match echo::start_echo(Duration::from_secs(RUN_TIMEOUT_S)) {
            Ok(x) => x,
            Err(e) => self.fail(case, &e),
        }
    }

    /// The echo's end: the thread joined, its count printed, its complaint
    /// the case's failure.
    fn echo_end(&mut self, case: &str, wire: Echo) {
        match echo::end_echo(wire) {
            Ok(n) => println!("  (the wire's echo on the second UART: {n} bytes echoed)"),
            Err(e) => self.fail(case, &e),
        }
    }

    fn control_suite_variant(&mut self, case: &str, broke: &str, ld: &ToolCmd, qemu: &Path, shim_objects: &[String], lib: &str, tag: &str, fail_lines: &[&str]) {
        println!("== case {case} (negative control): {broke} ==");
        let path_rel = format!("src/kernel/target/{}/rt-suite-{tag}.elf", self.w.out_dir);
        let map_rel = format!("src/kernel/target/{}/rt-suite-{tag}.map", self.w.out_dir);
        let mut args: Vec<String> = vec!["-T".to_string(), self.w.map.to_string(), "--gc-sections".to_string(), format!("-Map={map_rel}")];
        args.extend(RT_LINK_WORDS.iter().map(|s| s.to_string()));
        args.push(format!("{}/{SUITE_MERGED}", self.w.tier));
        args.extend(shim_objects.iter().cloned());
        args.extend(self.w.gc_objects.iter().map(|s| s.to_string()));
        args.push(format!("{}/{LIBC}", self.w.tier));
        args.push(lib.to_string());
        args.push("-o".to_string());
        args.push(path_rel.clone());
        let ld_out = self.out.join(format!("rt-suite-{tag}.ld.stdout"));
        let ld_log = self.out.join(format!("rt-suite-{tag}.ld.stderr"));
        match run_tool_bounded(&self.repo, ld, &args, &ld_out, &ld_log) {
            Ok(true) => {}
            Ok(false) => {
                echo_file(&ld_log, "  | ");
                self.fail(case, "the control's image did not link (see its linker stderr log)");
            }
            Err(e) => self.fail(case, &e),
        }
        let elf = self.repo.join(path_rel.replace('/', std::path::MAIN_SEPARATOR_STR));
        let elf_arg = elf.to_string_lossy().into_owned();
        let raw_path = self.out.join(format!("rt-suite-{tag}.raw"));
        let stderr_path = self.out.join(format!("rt-suite-{tag}.stderr"));
        let (chardev, echo) = self.echo_start(case);
        let mut qargs: Vec<&str> = vec!["-M", self.w.machine, "-display", "none", "-monitor", "none", "-serial", "stdio", "-serial", &chardev, "-semihosting"];
        qargs.extend_from_slice(ExecProfile::Parity.qemu_args());
        qargs.push("-kernel");
        qargs.push(&elf_arg);
        if let Err(e) = run_captured(qemu, &qargs, &raw_path, &stderr_path, Duration::from_secs(RUN_TIMEOUT_S)) {
            self.fail(case, &e);
        }
        let raw = std::fs::read(&raw_path).unwrap_or_default();
        self.echo_end(case, echo);
        let failed = fail_lines.iter().all(|l| contains_fixed(&raw, l));
        let all_good = contains_fixed(&raw, SUITE_ALL_GOOD);
        if !failed || all_good {
            self.fail_control(case, broke, &format!("the doctored input was ACCEPTED: the section's FAIL line ({}) is not on the wire, or the suite reports ALL GOOD", fail_lines.join(", ")));
        }
        println!("  refused as designed: the section fails on the wire ({}) and the suite reports no ALL GOOD", fail_lines.join(", "));
        self.pass(case, "negative", broke, "");
        println!("  PASS (control)");
    }

    /// The GPIO section's controls: the version control substitutes the
    /// class's variant of the shim's GPIO API object (its version string an
    /// API generation below the bias flags) in every world; the latch control
    /// links the suite against the row's kernel built with the GPIO API's
    /// doctored latch (the virtual loop fires twice per change), in every
    /// world.
    fn control_suite_gpio(&mut self, ld: &ToolCmd, qemu: &Path) {
        let base: Vec<String> = self.w.shim_objects.iter().map(|s| s.to_string()).collect();
        let lib = self.w.lib;
        let version = substitute_object(&base, "goldens/shim/shim-gpiod.o", "goldens/shim/shim-gpiod-break-version.o", "goldens/shim/shim-gpiod-an505.o", "goldens/shim/shim-gpiod-break-version-an505.o");
        self.control_suite_variant(
            "rt.suite-gpiod-version-control",
            "the suite image linked with the GPIO API object whose version string states an API generation below the bias flags (the driver reads no pull modes)",
            ld, qemu, &version, lib, "gpiod-version", &["FAIL rt-gpio"],
        );
        let case = "rt.suite-gpio-latch-control";
        let dir = self.repo.join("src").join("kernel").join("target").join(self.w.out_dir).join("kernel-latch");
        let _ = std::fs::create_dir_all(&dir);
        let log = self.out.join("rt-suite-gpio-latch-staticlib.log");
        let (target, feats): (Option<&str>, String) = match self.w.lib_build {
            None => (None, "payload,entropy-accept-software,test-gpio-doctored-latch".to_string()),
            Some((_, target, feats)) => (Some(target), format!("{feats},test-gpio-doctored-latch")),
        };
        let mut bargs: Vec<&str> = vec!["--lib"];
        if let Some(tg) = target {
            bargs.extend_from_slice(&["--target", tg, "--no-default-features"]);
        }
        bargs.extend_from_slice(&["--features", feats.as_str()]);
        let ok = match cargo_build_kernel_at(&self.repo, &dir, &bargs, &log) {
            Ok(ok) => ok,
            Err(e) => {
                println!("rhu-gate: {e}");
                false
            }
        };
        if !ok {
            echo_file(&log, "");
            self.fail(case, "the doctored kernel's staticlib build error");
        }
        let triple = target.unwrap_or("thumbv7em-none-eabihf");
        let latch_lib = format!("src/kernel/target/{}/kernel-latch/{triple}/release/librhu_kernel.a", self.w.out_dir);
        self.control_suite_variant(
            case,
            "the suite image linked against the row's kernel built with the doors' doctored latch (the virtual loop fires twice per change; the section counts two events for one edge)",
            ld, qemu, &base, &latch_lib, "gpio-latch", &["FAIL rt-gpio"],
        );
    }

    /// The bus sections' controls: the functionality control substitutes the
    /// class's variant of the shim's bus device unit whose functionality
    /// answer names no combined transfer, so the driver takes its
    /// file-transfer path, which is not presented; the doctored control
    /// links the suite against the row's kernel built with the bus API's
    /// doctored models (a data byte not acknowledged, a clock that never
    /// completes, the pumps' bound removed). Both in every world.
    fn control_suite_bus(&mut self, ld: &ToolCmd, qemu: &Path) {
        let base: Vec<String> = self.w.shim_objects.iter().map(|s| s.to_string()).collect();
        let lib = self.w.lib;
        let funcs = substitute_object(&base, "goldens/shim/shim-busdev.o", "goldens/shim/shim-busdev-break-funcs.o", "goldens/shim/shim-busdev-an505.o", "goldens/shim/shim-busdev-break-funcs-an505.o");
        self.control_suite_variant(
            "rt.suite-busdev-funcs-control",
            "the suite image linked with the bus device unit whose functionality answer names no combined transfer (the driver takes its file-transfer path, which is not presented)",
            ld, qemu, &funcs, lib, "busdev-funcs", &["FAIL rt-i2c"],
        );
        let case = "rt.suite-bus-doctored-control";
        let dir = self.repo.join("src").join("kernel").join("target").join(self.w.out_dir).join("kernel-bus-doctored");
        let _ = std::fs::create_dir_all(&dir);
        let log = self.out.join("rt-suite-bus-doctored-staticlib.log");
        let (target, feats): (Option<&str>, String) = match self.w.lib_build {
            None => (None, "payload,entropy-accept-software,test-bus-doctored".to_string()),
            Some((_, target, feats)) => (Some(target), format!("{feats},test-bus-doctored")),
        };
        let mut bargs: Vec<&str> = vec!["--lib"];
        if let Some(tg) = target {
            bargs.extend_from_slice(&["--target", tg, "--no-default-features"]);
        }
        bargs.extend_from_slice(&["--features", feats.as_str()]);
        let ok = match cargo_build_kernel_at(&self.repo, &dir, &bargs, &log) {
            Ok(ok) => ok,
            Err(e) => {
                println!("rhu-gate: {e}");
                false
            }
        };
        if !ok {
            echo_file(&log, "");
            self.fail(case, "the doctored kernel's staticlib build error");
        }
        let triple = target.unwrap_or("thumbv7em-none-eabihf");
        let doctored_lib = format!("src/kernel/target/{}/kernel-bus-doctored/{triple}/release/librhu_kernel.a", self.w.out_dir);
        self.control_suite_variant(
            case,
            "the suite image linked against the row's kernel built with the bus doors' doctored models (a data byte not acknowledged, a clock that never completes, the pumps' bound removed; the I2C and SPI sections fail)",
            ld, qemu, &base, &doctored_lib, "bus-doctored", &["FAIL rt-i2c", "FAIL rt-spi"],
        );
    }

    /// The PWM section's control: the version of the shim's PWM device-file
    /// unit whose period file answers the value written, not the period the
    /// timer runs, substituted in every world (the Cortex-M33 twin in the an505
    /// world); the section's quantized-truth case refuses it.
    fn control_suite_pwm(&mut self, ld: &ToolCmd, qemu: &Path) {
        let base: Vec<String> = self.w.shim_objects.iter().map(|s| s.to_string()).collect();
        let lib = self.w.lib;
        let period = substitute_object(&base, "goldens/shim/shim-pwmfs.o", "goldens/shim/shim-pwmfs-break-period-asked.o", "goldens/shim/shim-pwmfs-an505.o", "goldens/shim/shim-pwmfs-break-period-asked-an505.o");
        self.control_suite_variant(
            "rt.suite-pwmfs-period-control",
            "the suite image linked with the PWM device-file unit whose period file answers the value written, not the period the timer runs (the quantized truth unread)",
            ld, qemu, &period, lib, "pwmfs-period", &["FAIL rt-pwm"],
        );
    }

    /// The serial section's control: the suite image linked with the
    /// serial device unit's variant whose close keeps the port's block (the
    /// kernel's claim never released), so the class's reopen after its
    /// close is refused; the Cortex-M33 twin serves the an505 world. The
    /// class caches the rate it was given and never reads it back, so the
    /// attributes-image control cannot bite here; the close can.
    fn control_suite_ttydev(&mut self, ld: &ToolCmd, qemu: &Path) {
        let base: Vec<String> = self.w.shim_objects.iter().map(|s| s.to_string()).collect();
        let lib = self.w.lib;
        let keeps = substitute_object(&base, "goldens/shim/shim-ttydev.o", "goldens/shim/shim-ttydev-break-close-keeps.o", "goldens/shim/shim-ttydev-an505.o", "goldens/shim/shim-ttydev-break-close-keeps-an505.o");
        self.control_suite_variant(
            "rt.suite-ttydev-close-keeps-control",
            "the suite image linked with the serial device unit whose close keeps the port's block (the kernel's claim never released: the class's reopen after its close is refused)",
            ld, qemu, &keeps, lib, "ttydev-close-keeps", &["FAIL rt-serial"],
        );
        // The names control: the suite image linked with the serial unit's low-power word naming block 1, as the SDK's
        // link line defines it for a row whose board table names a low-power UART, while the suite's board facts name
        // that port /dev/ttyS1. The unit then names the port /dev/ttyLP1 and serves the folder /sys/class/tty, so the
        // class's enumeration answers /dev/ttyS0 and /dev/ttyLP1 by its first search, and the section's names case
        // refuses the board class's /dev/ttyS1 with the enumeration's own names in its line.
        let mut word: Vec<String> = base.clone();
        word.push("--defsym=__rhu_uart_lowpower_blocks=2".to_string());
        self.control_suite_variant(
            "rt.suite-ttydev-lowpower-word-control",
            "the suite image linked with the serial unit's low-power word for block 1 under board facts that name the port /dev/ttyS1 (the board class names a device file the enumeration does not return)",
            ld, qemu, &word, lib, "ttydev-lowpower-word", &["FAIL rt-serial (the board class's port /dev/ttyS1 is not a name of the enumeration: /dev/ttyS0,/dev/ttyLP1)"],
        );
    }

    /// The analog section's control: the IIO device-file unit's variant
    /// whose read answers the previous sample, substituted in every world
    /// (the Cortex-M33 twin in the an505 world); the section's loop step
    /// reads the duty's change and refuses it.
    fn control_suite_iiofs(&mut self, ld: &ToolCmd, qemu: &Path) {
        let base: Vec<String> = self.w.shim_objects.iter().map(|s| s.to_string()).collect();
        let lib = self.w.lib;
        let stale = substitute_object(&base, "goldens/shim/shim-iiofs.o", "goldens/shim/shim-iiofs-break-read-stale.o", "goldens/shim/shim-iiofs-an505.o", "goldens/shim/shim-iiofs-break-read-stale-an505.o");
        self.control_suite_variant(
            "rt.suite-iiofs-read-stale-control",
            "the suite image linked with the analog input device-file unit whose read answers the previous sample (the loop step's second reading is the first's)",
            ld, qemu, &stale, lib, "iiofs-read-stale", &["FAIL rt-adc"],
        );
    }

    /// The null section's controls: the guard control
    /// links the suite against the row's kernel built with the null-page
    /// guard removed (a field read through null returns the row's
    /// reset-vector word and no exception is raised); the unarmed control
    /// substitutes the core's variant that never arms the kernel's delivery
    /// (the kernel's designed death at the first read through null); the
    /// address control substitutes the core's variant that reports the
    /// faulting address as one past the guarded page (the runtime ends the
    /// process on an access violation, never the managed null reference;
    /// its fail-fast line is read). The Cortex-M33
    /// twins serve the an505 world, as the PWM period control's does.
    fn control_suite_null(&mut self, ld: &ToolCmd, qemu: &Path) {
        let base: Vec<String> = self.w.shim_objects.iter().map(|s| s.to_string()).collect();
        let lib = self.w.lib;
        let case = "rt.suite-null-guard-off-control";
        let dir = self.repo.join("src").join("kernel").join("target").join(self.w.out_dir).join("kernel-null-guard-off");
        let _ = std::fs::create_dir_all(&dir);
        let log = self.out.join("rt-suite-null-guard-off-staticlib.log");
        let (target, feats): (Option<&str>, String) = match self.w.lib_build {
            None => (None, "payload,entropy-accept-software,null-guard-off".to_string()),
            Some((_, target, feats)) => (Some(target), format!("{feats},null-guard-off")),
        };
        let mut bargs: Vec<&str> = vec!["--lib"];
        if let Some(tg) = target {
            bargs.extend_from_slice(&["--target", tg, "--no-default-features"]);
        }
        bargs.extend_from_slice(&["--features", feats.as_str()]);
        let ok = match cargo_build_kernel_at(&self.repo, &dir, &bargs, &log) {
            Ok(ok) => ok,
            Err(e) => {
                println!("rhu-gate: {e}");
                false
            }
        };
        if !ok {
            echo_file(&log, "");
            self.fail(case, "the guard-off kernel's staticlib build error");
        }
        let triple = target.unwrap_or("thumbv7em-none-eabihf");
        let guard_off_lib = format!("src/kernel/target/{}/kernel-null-guard-off/{triple}/release/librhu_kernel.a", self.w.out_dir);
        self.control_suite_variant(
            case,
            "the suite image linked against the row's kernel built with the null-page guard removed (a field read through null returns the row's reset-vector word; nothing is raised)",
            ld, qemu, &base, &guard_off_lib, "null-guard-off", &["FAIL rt-null"],
        );
        let unarmed = substitute_object(&base, "goldens/shim/shim-core.o", "goldens/shim/shim-core-break-fault-unarmed.o", "goldens/shim/shim-core-an505.o", "goldens/shim/shim-core-break-fault-unarmed-an505.o");
        self.control_suite_variant(
            "rt.suite-fault-unarmed-control",
            "the suite image linked with the shim's core variant that records the runtime's SIGSEGV handler and never arms the kernel's null-fault delivery (the first read through null is the kernel's designed death)",
            ld, qemu, &unarmed, lib, "fault-unarmed", &["not delivered: no hosting layer has armed the delivery"],
        );
        let addr = substitute_object(&base, "goldens/shim/shim-core.o", "goldens/shim/shim-core-break-fault-addr.o", "goldens/shim/shim-core-an505.o", "goldens/shim/shim-core-break-fault-addr-an505.o");
        self.control_suite_variant(
            "rt.suite-fault-addr-control",
            "the suite image linked with the shim's core variant that reports the faulting address as one past the guarded page (the runtime ends the process on an access violation, never the managed null reference)",
            ld, qemu, &addr, lib, "fault-addr", &["Access Violation: Attempted to read or write protected memory"],
        );
    }

    /// The delivery control: the suite's capture with its activation count
    /// set to zero must refuse at the floor (a host that delivered no
    /// activation could not have suspended a running worker).
    fn control_suite_delivery(&mut self, raw: &[u8]) {
        let case = "rt.suite-delivery-control";
        println!("== case {case} (negative control): the suite's capture with its activation count set to zero ==");
        let broke = "the suite's capture with the host's activation count set to zero (delivery suppressed on the wire)";
        let doctored = zero_delivery(raw);
        if doctored == raw {
            self.fail(case, "the control could not doctor the capture (the activation line is absent)");
        }
        if activation_count(&doctored, ACTIVATIONS_FLOOR).is_ok() {
            self.fail_control(case, broke, "a capture reporting zero activations was ACCEPTED at the floor");
        }
        println!("  refused as designed: zero activations delivered is below the floor of {ACTIVATIONS_FLOOR}");
        self.pass(case, "negative", broke, "");
        println!("  PASS (control)");
    }

    /// The suite golden control: the golden with its verdict's GOOD changed
    /// to G00D must refuse the compare.
    fn control_suite_golden(&mut self) {
        let case = "rt.suite-golden-control";
        println!("== case {case} (negative control) ==");
        let golden = std::fs::read(self.frozen(SUITE_GOLDEN)).unwrap_or_default();
        let flipped: Vec<u8> = String::from_utf8_lossy(&golden).replace("ALL GOOD", "ALL G00D").into_bytes();
        let broke = "the suite golden with its verdict line's GOOD changed to G00D";
        if flipped == golden {
            self.fail(case, "the control could not doctor the golden's verdict line");
        }
        if eq_lf_normalized(&flipped, &golden) {
            self.fail_control(case, broke, "the doctored golden was ACCEPTED by the compare");
        }
        println!("  refused as designed: the compare rejects the doctored verdict line");
        self.pass(case, "negative", broke, "");
        println!("  PASS (control)");
    }

    /// The break-class control: the suite's own capture with one section's
    /// verdict turned into a failure must be refused by every checker that
    /// stands between it and a pass -- the golden compare, the forbidden
    /// sweep and the marker order alike.
    fn control_suite_break(&mut self, raw: &[u8]) {
        let case = "rt.suite-break-control";
        println!("== case {case} (negative control) ==");
        let broke = "the suite's capture with the threading section's verdict turned into a failure";
        let doctored: Vec<u8> = String::from_utf8_lossy(raw).replace("PASS rt-threads", "FAIL rt-threads").into_bytes();
        if doctored == raw {
            self.fail(case, "the control could not doctor the capture (the threading verdict is absent)");
        }
        let golden = std::fs::read(self.frozen(SUITE_GOLDEN)).unwrap_or_default();
        if eq_lf_normalized(&payload_stream(&doctored), &golden) {
            self.fail_control(case, broke, "the doctored stream was ACCEPTED by the golden compare");
        }
        if !SUITE_FORBIDDEN.iter().any(|m| contains_fixed(&doctored, m)) {
            self.fail_control(case, broke, "the doctored capture passed the forbidden sweep");
        }
        if markers_out_of_order(&doctored, self.w.suite_markers).is_none() {
            self.fail_control(case, broke, "the doctored capture carried every marker in order");
        }
        println!("  refused as designed: the golden compare, the forbidden sweep and the marker order each reject one failed section");
        self.pass(case, "negative", broke, "");
        println!("  PASS (control)");
    }
}

/// The world's link list with one frozen object replaced by its control
/// variant: the Cortex-M7 object or its Cortex-M33 twin, whichever the world
/// links (a world linking neither is a fault of the tables).
fn substitute_object(base: &[String], cortex_m7: &str, cortex_m7_variant: &str, cortex_m33: &str, cortex_m33_variant: &str) -> Vec<String> {
    if !base.iter().any(|s| s == cortex_m7 || s == cortex_m33) {
        println!("rhu-gate: the world links neither {cortex_m7} nor {cortex_m33}");
        exit(1);
    }
    base.iter()
        .map(|s| if s == cortex_m7 { cortex_m7_variant.to_string() } else if s == cortex_m33 { cortex_m33_variant.to_string() } else { s.clone() })
        .collect()
}

fn open(repo: &Path, w: &'static RtWorld, gate_id: &'static str, display: &'static str, file: &str) -> Rt {
    let out = repo.join("src").join("kernel").join("target").join(w.out_dir);
    if std::fs::create_dir_all(&out).is_err() {
        println!("rhu-gate: cannot create {}", out.display());
        exit(1);
    }
    let results = match Results::create(&out.join(file)) {
        Ok(r) => r,
        Err(e) => {
            println!("rhu-gate: cannot create results file: {e}");
            exit(1);
        }
    };
    let header = std::fs::read_to_string(repo.join("src").join("kernel").join("include").join("rhu.h")).unwrap_or_default();
    let (abi, compat) = match (header_abi(&header), header_abi_compat(&header)) {
        (Some(a), Some(c)) => (a, c),
        _ => {
            println!("rhu-gate: include/rhu.h states no RHU_ABI or no RHU_ABI_COMPAT (the handshake marker's values)");
            exit(1);
        }
    };
    Rt { w, abi, compat, repo: repo.to_path_buf(), out, results, gate_id, display, records: Vec::new(), manifest_text: String::new() }
}

/// RHU_ABI as include/rhu.h states it: the generation the records must
/// carry.
fn read_abi(repo: &Path) -> u32 {
    let text = std::fs::read_to_string(repo.join("src").join("kernel").join("include").join("rhu.h")).unwrap_or_default();
    match header_abi(&text) {
        Some(v) => v,
        None => {
            println!("rhu-gate: include/rhu.h states no RHU_ABI");
            exit(1);
        }
    }
}

pub fn run(repo: &Path, w: &'static RtWorld) -> ! {
    let facts = match rustc_facts() {
        Ok(f) => f,
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
    let (qemu, qemu_v) = match resolve_qemu(repo) {
        Ok(q) => q,
        Err(e) => {
            println!("rhu-gate: {e}");
            exit(1);
        }
    };
    let abi = read_abi(repo);
    let mut rt = open(repo, w, w.gate, w.display, w.results);
    rt.results.emit(&format!(
        "{{\"schema\":1,\"type\":\"run\",\"gate\":\"{}\",\"run\":\"{}-{}\",\"host\":\"{}\",\"runner\":\"{RUNNER}\",\"chain\":{},\"tools\":{{\"qemu\":\"{}\",\"rustc\":\"{}\",\"ld\":\"{}\"}}}}",
        w.gate,
        w.gate,
        std::process::id(),
        w.host,
        chain_json_from_env(),
        qemu_v,
        facts.version,
        ld.version
    ));
    rt.verify_frozen();
    rt.case_merge(&facts);
    rt.control_attrs(&facts);
    rt.control_set();
    rt.case_libc(&facts);
    rt.control_libc_forbidden();
    let (path_rel, sha) = rt.case_link(&facts, &ld, abi);
    rt.control_link_frozen();
    if w.rider {
        rt.case_link_board(&facts, &ld, abi);
    }
    rt.case_boot(&qemu, &path_rel, &sha);
    rt.control_boot_golden();
    let suite_raw = rt.case_suite(&facts, &ld, &qemu, abi);
    rt.control_suite_golden();
    rt.control_suite_break(&suite_raw);
    rt.control_suite_delivery(&suite_raw);
    rt.control_suite_gpio(&ld, &qemu);
    rt.control_suite_bus(&ld, &qemu);
    rt.control_suite_pwm(&ld, &qemu);
    rt.control_suite_ttydev(&ld, &qemu);
    rt.control_suite_iiofs(&ld, &qemu);
    rt.control_suite_null(&ld, &qemu);
    rt.case_census(&facts);
    rt.control_census(&facts);
    rt.control_census_ldrexd(&facts);
    for p in PENDING {
        println!("   pending (unarmed, not counted): {p}");
    }
    for c in internal_cases(w) {
        println!("   internal (rederive leg, not counted): {c}");
    }
    rt.results.emit_summary(w.gate, "pass", 0);
    if !validate_results(repo, rt.results.path()) {
        println!("{}: FAIL -- results file failed schema validation", w.display);
        exit(1);
    }
    println!(
        "{}: PASS ({}; {} pending unarmed; {} internal on the rederive leg)  [{} | {} | {}]",
        w.display,
        w.cases,
        PENDING.len(),
        internal_cases(w).len(),
        qemu_v,
        facts.version,
        ld.version
    );
    exit(0);
}

/// The archive set's directory for a world: its environment variable,
/// else the first line of its untracked machine-local pointer file under
/// tools/toolchains/ (the directory in the form the linker opens), else
/// None.
fn resolve_archives(repo: &Path, w: &RtWorld) -> Option<String> {
    if let Ok(v) = std::env::var(w.archives_env) {
        if !v.trim().is_empty() {
            return Some(v.trim().to_string());
        }
    }
    let local = repo.join("tools").join("toolchains").join(w.archives_local);
    std::fs::read_to_string(&local)
        .ok()
        .and_then(|t| t.lines().next().map(|l| l.trim().to_string()))
        .filter(|l| !l.is_empty())
}

/// The archive set's files against the frozen set file's `archive` lines
/// (name, size, sha256), every one, before any merge.
fn archives_fault(dir: &str, set_text: &str) -> Result<(), String> {
    for (name, _) in MERGE_ARCHIVES.iter().map(|a| (*a, ())) {
        let rec = set_text
            .lines()
            .map(str::trim)
            .filter_map(|l| l.strip_prefix("archive "))
            .map(|r| r.split_whitespace().collect::<Vec<_>>())
            .find(|f| f.first() == Some(&name));
        let Some(rec) = rec else {
            return Err(format!("the frozen set file carries no record for {name}"));
        };
        if rec.len() < 3 {
            return Err(format!("the frozen set file's record for {name} lacks a size or a sha"));
        }
        let path = format!("{dir}/{name}");
        let got = sha256_file(Path::new(&path)).map_err(|e| format!("{path} unreadable: {e}"))?;
        if got != rec[2] {
            return Err(format!("{name}: sha256 {got} != the frozen set's {} -- a different archive set", rec[2]));
        }
    }
    Ok(())
}

/// The kit's two libraries against the member list's identities.
fn kit_fault(dir: &str, members: &LibcMembers) -> Result<(), String> {
    for (name, size, sha) in &members.archives {
        let path = format!("{dir}/{name}");
        let meta = std::fs::metadata(&path).map_err(|e| format!("{path} unreadable: {e}"))?;
        if meta.len() != *size {
            return Err(format!("{name}: {} bytes at the kit, the member list records {size}", meta.len()));
        }
        let got = sha256_file(Path::new(&path)).map_err(|e| format!("{path} unreadable: {e}"))?;
        if &got != sha {
            return Err(format!("{name}: sha256 {got} != the member list's {sha} -- a different kit"));
        }
    }
    Ok(())
}

/// The comparison the leg makes for a re-derived object against its
/// frozen twin, and the method's name for the transcript: apart from the
/// producer stamp where the toolchain's object copier can remove the
/// comment section (a compiler's object), and WHOLE where the copier
/// refuses because a relocation targets that section -- a merged object's
/// debug ranges carry one -- so the stamp stays inside the identity there.
/// Any other refusal of the copier is the caller's failure.
fn stamp_compare(facts: &RustcFacts, out: &Path, fresh: &Path, frozen: &Path, tag: &str) -> Result<(String, String, &'static str), String> {
    let dir = out.join("stripped");
    let fresh_stripped = sha256_without_stamp(facts, fresh, &dir.join(format!("fresh-{tag}.o")));
    let frozen_stripped = sha256_without_stamp(facts, frozen, &dir.join(format!("frozen-{tag}.o")));
    match (fresh_stripped, frozen_stripped) {
        (Ok(got), Ok(want)) => Ok((got, want, "apart from the producer stamp")),
        (Err(e), _) | (_, Err(e)) if e.contains("cannot be removed") => {
            let got = sha256_file(fresh).map_err(|e| format!("{}: {e}", fresh.display()))?;
            let want = sha256_file(frozen).map_err(|e| format!("{}: {e}", frozen.display()))?;
            Ok((got, want, "whole: the object copier cannot remove this object's comment section, a relocation in its debug ranges targeting it"))
        }
        (Err(e), _) | (_, Err(e)) => Err(e),
    }
}

pub fn rederive(repo: &Path, w: &'static RtWorld) -> ! {
    let facts = match rustc_facts() {
        Ok(f) => f,
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
    let objcopy = match llvm_tool(&facts, "llvm-objcopy") {
        Ok(p) => p,
        Err(e) => {
            println!("rhu-gate: {e}");
            exit(1);
        }
    };
    let nm = match llvm_tool(&facts, "llvm-nm") {
        Ok(p) => p,
        Err(e) => {
            println!("rhu-gate: {e}");
            exit(1);
        }
    };
    let ar = match llvm_tool(&facts, "llvm-ar") {
        Ok(p) => p,
        Err(e) => {
            println!("rhu-gate: {e}");
            exit(1);
        }
    };
    let mut rt = open(repo, w, w.rederive_gate, w.rederive_display, w.rederive_results);
    rt.results.emit(&format!(
        "{{\"schema\":1,\"type\":\"run\",\"gate\":\"{}\",\"run\":\"{}-{}\",\"host\":\"host-native\",\"runner\":\"{RUNNER}\",\"tools\":{{\"rustc\":\"{}\",\"ld\":\"{}\"}}}}",
        w.rederive_gate,
        w.rederive_gate,
        std::process::id(),
        facts.version,
        ld.version
    ));
    rt.verify_frozen();
    let Some(dir) = resolve_archives(repo, w) else {
        println!("{}: NOT RUN (no archive set resolved: set {} to the runtime pack's native directory at the pin, or put that directory in tools/toolchains/{})", w.rederive_display, w.archives_env, w.archives_local);
        exit(1);
    };
    let Some(kit) = resolve_kit_dir(repo, w) else {
        println!("{}: NOT RUN (the toolchain's C library is not resolved: set {} or write tools/toolchains/{})", w.rederive_display, w.newlib_env, w.newlib_local);
        exit(1);
    };

    // 1. the surgery: the member out of the archive, its cookie section renamed.
    let case = "rt.rederive-surgery";
    println!("== case {case}: the startup member printed out of {SURGERY_ARCHIVE} at {dir} and its cookie section renamed ==");
    let set_text = std::fs::read_to_string(rt.frozen(rt.w.set_file)).unwrap_or_default();
    if let Err(e) = archives_fault(&dir, &set_text) {
        rt.fail(case, &e);
    }
    println!("  archive set: {}/{} archives tie the frozen set file's sha256s", MERGE_ARCHIVES.len(), MERGE_ARCHIVES.len());
    let archive_path = format!("{dir}/{SURGERY_ARCHIVE}");
    let member = rt.tool_bytes(&ar, case, &["p", &archive_path, SURGERY_MEMBER], "rederive-startup.cpp.o");
    let renamed = rt.out.join("rederive-startup-cookie-copied.o");
    let member_s = member.to_string_lossy().into_owned();
    let renamed_s = renamed.to_string_lossy().into_owned();
    let _ = rt.tool_text(&objcopy, case, &["--rename-section", SURGERY_RENAME, &member_s, &renamed_s], "rederive-surgery.objcopy.txt");
    let record = rt.record_sha(SURGERY);
    let frozen_path = rt.frozen(SURGERY);
    let (got, want, how) = match stamp_compare(&facts, &rt.out, &renamed, &frozen_path, "surgery") {
        Ok(p) => p,
        Err(e) => rt.fail(case, &e),
    };
    let how_surgery = how;
    if got != want {
        rt.fail(case, &format!("re-derived surgery object sha256 {got} != frozen {want} ({how}) -- the archive member or the rename moved; a moved object is a registered re-freeze"));
    }
    println!("  re-derived surgery object byte-identical to {}/{SURGERY} ({how}; {}...; the record's sha256 {}...)", rt.w.tier, &want[..16], &record[..16]);
    println!("  the frozen surgery object's producer stamp (report-only): '{}'; the re-derived one's: '{}'", producer_stamp_of(&frozen_path), producer_stamp_of(&renamed));
    rt.pass(case, "positive", "", &format!("{{\"surgery_sha\":\"{record}\"}}"));
    println!("  PASS");

    // 2. the merge, from the frozen program object, the frozen surgery object and the archives.
    let case = "rt.rederive-merge";
    println!("== case {case}: the merge re-derived from the frozen program object, the frozen surgery object and the archive set ==");
    let merge = |rt: &mut Rt, payload: &str, selector: &str, tag: &str| -> PathBuf {
        let pre = rt.out.join(format!("rederive-{tag}-pre.o"));
        let post = rt.out.join(format!("rederive-{tag}.o"));
        let mut args: Vec<String> = vec!["-r".to_string()];
        for u in MERGE_UNDEFINED {
            args.push("-u".to_string());
            args.push(u.to_string());
        }
        args.push(rt.frozen(payload).to_string_lossy().into_owned());
        args.push(rt.frozen(SURGERY).to_string_lossy().into_owned());
        args.push("--start-group".to_string());
        for a in MERGE_ARCHIVES {
            let name = if a == "libstandalonegc-enabled.a" { selector } else { a };
            args.push(format!("{dir}/{name}"));
        }
        args.push("--end-group".to_string());
        args.push("-o".to_string());
        args.push(pre.to_string_lossy().into_owned());
        let log_o = rt.out.join(format!("rederive-{tag}.ld.stdout"));
        let log_e = rt.out.join(format!("rederive-{tag}.ld.stderr"));
        match run_tool_bounded(&rt.repo, &ld, &args, &log_o, &log_e) {
            Ok(true) => {}
            Ok(false) => rt.fail(case, &format!("the merge ({tag}) failed (see {})", log_e.display())),
            Err(e) => rt.fail(case, &e),
        }
        let donor = rt.frozen(rt.w.donor);
        match attrs_update_file(&pre, &donor, &post) {
            Ok((pre_sha, n)) => println!("  {tag}: the linker's product (the pre-object) sha256 {pre_sha}; the attributes update applied by the runner ({n}-byte donor)"),
            Err(e) => rt.fail(case, &format!("the attributes update failed ({tag}): {e}")),
        }
        post
    };
    let post = merge(&mut rt, PAYLOAD, "libstandalonegc-enabled.a", "enabled");
    let record = rt.record_sha(MERGED);
    let frozen_path = rt.frozen(MERGED);
    let (got, want, how) = match stamp_compare(&facts, &rt.out, &post, &frozen_path, "merged") {
        Ok(p) => p,
        Err(e) => rt.fail(case, &e),
    };
    let how_merged = how;
    if got != want {
        rt.fail(
            case,
            &format!("re-derived merged object sha256 {got} != frozen {want} ({how}) -- the merge moved (a linker, an archive, the surgery object or the program object changed); a moved object is a registered re-freeze, never a silent rebuild"),
        );
    }
    println!("  re-derived object byte-identical to {}/{MERGED} ({how}; {}...; the record's sha256 {}...)", rt.w.tier, &want[..16], &record[..16]);
    println!("  the frozen merged object's producer stamp (report-only): '{}'; the re-derived one's: '{}'", producer_stamp_of(&frozen_path), producer_stamp_of(&post));
    rt.pass(case, "positive", "", &format!("{{\"merged_sha\":\"{record}\",\"archives\":{}}}", MERGE_ARCHIVES.len()));
    println!("  PASS");

    // 2b. the suite's merge: the same recipe from the frozen suite object.
    let case = "rt.rederive-merge-suite";
    println!("== case {case}: the suite's merge re-derived from the frozen suite object, the frozen surgery object and the archive set ==");
    let post_suite = merge(&mut rt, SUITE_PAYLOAD, "libstandalonegc-enabled.a", "enabled-suite");
    let record = rt.record_sha(SUITE_MERGED);
    let frozen_path = rt.frozen(SUITE_MERGED);
    let (got, want, how) = match stamp_compare(&facts, &rt.out, &post_suite, &frozen_path, "merged-suite") {
        Ok(p) => p,
        Err(e) => rt.fail(case, &e),
    };
    let how_suite = how;
    if got != want {
        rt.fail(case, &format!("re-derived suite merge sha256 {got} != frozen {want} ({how}) -- the merge moved (a linker, an archive, the surgery object or the suite's program object changed); a moved object is a registered re-freeze, never a silent rebuild"));
    }
    println!("  re-derived object byte-identical to {}/{SUITE_MERGED} ({how}; {}...; the record's sha256 {}...)", rt.w.tier, &want[..16], &record[..16]);
    println!("  the frozen suite merge's producer stamp (report-only): '{}'; the re-derived one's: '{}'", producer_stamp_of(&frozen_path), producer_stamp_of(&post_suite));
    rt.pass(case, "positive", "", &format!("{{\"merged_sha\":\"{record}\",\"archives\":{}}}", MERGE_ARCHIVES.len()));
    println!("  PASS");

    // 3. the closure, from the kit at the member list's identities.
    let case = "rt.rederive-libc";
    println!("== case {case}: the closure's members printed out of the kit's archives again at {kit} and re-merged ==");
    let members_text = std::fs::read_to_string(rt.frozen(LIBC_MEMBERS)).unwrap_or_default();
    let members = match parse_members(&members_text) {
        Ok(m) => m,
        Err(e) => rt.fail(case, &e),
    };
    if let Err(e) = kit_fault(&kit, &members) {
        rt.fail(case, &e);
    }
    println!("  the kit's {} libraries tie the member list's sizes and sha256s", members.archives.len());
    let mut args: Vec<String> = vec!["-r".to_string()];
    for (i, (archive, name)) in members.members.iter().enumerate() {
        let archive_path = format!("{kit}/{archive}");
        let out_name = format!("rederive-member-{i:03}-{name}");
        let p = rt.tool_bytes(&ar, case, &["p", &archive_path, name], &out_name);
        if std::fs::metadata(&p).map(|m| m.len()).unwrap_or(0) == 0 {
            rt.fail(case, &format!("{archive}: the member {name} printed empty"));
        }
        args.push(p.to_string_lossy().into_owned());
    }
    let pre = rt.out.join("rederive-libc-pre.o");
    let post = rt.out.join("rederive-libc.o");
    args.push("-o".to_string());
    args.push(pre.to_string_lossy().into_owned());
    let log_o = rt.out.join("rederive-libc.ld.stdout");
    let log_e = rt.out.join("rederive-libc.ld.stderr");
    match run_tool_bounded(&rt.repo, &ld, &args, &log_o, &log_e) {
        Ok(true) => {}
        Ok(false) => rt.fail(case, &format!("the closure merge failed (see {})", log_e.display())),
        Err(e) => rt.fail(case, &e),
    }
    let donor = rt.frozen(rt.w.donor);
    match attrs_update_file(&pre, &donor, &post) {
        Ok((pre_sha, n)) => println!("  the linker's product (the pre-object) sha256 {pre_sha}; the attributes update applied by the runner ({n}-byte donor)"),
        Err(e) => rt.fail(case, &format!("the attributes update failed (libc): {e}")),
    }
    let got = sha256_file(&post).unwrap_or_default();
    let want = rt.record_sha(LIBC);
    if got != want {
        rt.fail(case, &format!("re-derived closure sha256 {got} != frozen {want} -- the kit's members, their order or the linker moved; a moved object is a registered re-freeze"));
    }
    println!("  {} members printed out of the archives and re-merged in the list's order; byte-identical to {}/{LIBC} ({}...)", members.members.len(), rt.w.tier, &want[..16]);
    let closure_stamp = producer_stamp_of(&post);
    println!("  the closure's producer stamp: {closure_stamp} (compared whole)");
    rt.pass(case, "positive", "", &format!("{{\"libc_sha\":\"{want}\",\"members\":{}}}", members.members.len()));
    println!("  PASS");

    // 4. the census over the archive set at the pointer: every member decoded.
    let case = "rt.rederive-census";
    println!("== case {case}: every member of the five archives at {dir} decoded -- no compiled function naming a double register above d15 (the pinned assembly pair the only ones); the ARM-state code exactly the pinned pair; no 64-bit exclusive-access instruction in any member ==");
    let objdump = match llvm_tool(&facts, "llvm-objdump") {
        Ok(p) => p,
        Err(e) => rt.fail(case, &e),
    };
    let readobj = match llvm_tool(&facts, "llvm-readobj") {
        Ok(p) => p,
        Err(e) => rt.fail(case, &e),
    };
    let mut arm_all: Vec<String> = Vec::new();
    let mut d32_all: Vec<String> = Vec::new();
    let mut f64_total = 0usize;
    for a in MERGE_ARCHIVES {
        let archive_path = format!("{dir}/{a}");
        let listing = rt.tool_text(&ar, case, &["t", &archive_path], &format!("census-{a}.members.txt"));
        let members: Vec<String> = listing.lines().map(|l| l.trim().to_string()).filter(|l| !l.is_empty()).collect();
        let mut arm_here: Vec<String> = Vec::new();
        let mut f64_here = 0usize;
        for (i, m) in members.iter().enumerate() {
            let p = rt.tool_bytes(&ar, case, &["p", &archive_path, m], &format!("census-{a}-{i:03}.o"));
            let ps = p.to_string_lossy().into_owned();
            let syms = rt.tool_text(&readobj, case, &["--elf-output-style=GNU", "--symbols", &ps], &format!("census-{a}-{i:03}.syms.txt"));
            let sections = rt.tool_text(&readobj, case, &["--elf-output-style=GNU", "--sections", &ps], &format!("census-{a}-{i:03}.sections.txt"));
            let regions = arm_regions(&syms, &sections);
            let dis_t = rt.tool_text(&objdump, case, &["-d", CENSUS_TRIPLE, CENSUS_MATTR, &ps], &format!("census-{a}-{i:03}.dis.txt"));
            let thumb = d32_census_regions(&dis_t, &regions, false);
            if !thumb.is_empty() {
                let sites: usize = thumb.iter().map(|(_, n)| n).sum();
                rt.fail(case, &format!("{a}({m}): {} compiled function(s) name a double register above d15 ({sites} sites; the first: {} with {}) -- the Cortex-M7 class set carries none", thumb.len(), thumb[0].0, thumb[0].1));
            }
            let dis_a = if regions.is_empty() { String::new() } else { rt.tool_text(&objdump, case, &["-d", CENSUS_TRIPLE_ARM, CENSUS_MATTR, &ps], &format!("census-{a}-{i:03}.dis-arm.txt")) };
            let xd = xdouble_total(&dis_t, &dis_a, &regions);
            if let Some((f, n)) = xd.first() {
                rt.fail(case, &format!("{a}({m}): {f} carries a 64-bit exclusive-access instruction ({n} site(s)) -- no Cortex-M executes the form; the set is built for the core"));
            }
            if !regions.is_empty() {
                for (f, n) in d32_census_regions(&dis_a, &regions, true) {
                    let name = f.trim_start_matches('<').trim_end_matches(':').trim_end_matches('>').to_string();
                    if !D32_PINNED.contains(&name.as_str()) || n != 1 {
                        rt.fail(case, &format!("{a}({m}): {f} names a double register above d15 in ARM state ({n} site(s)) and is not the pinned assembly pair -- the Cortex-M7 class set carries no other"));
                    }
                    d32_all.push(format!("{m}: {name}"));
                }
            }
            f64_here += f64_census_regions(&dis_t, &regions);
            for (v, n) in arm_mode_symbols(&syms) {
                let fs = funcs_at(&syms, &v, &n);
                if !fs.iter().any(|f| ARM_MODE_PINNED.contains(&f.as_str())) {
                    rt.fail(case, &format!("{a}({m}): an ARM-state mapping symbol at {v} (section {n}) with no pinned function there: [{}]", fs.join(" ")));
                }
                arm_here.push(format!("{m}: {}", fs.join("/")));
            }
        }
        println!(
            "  {a}: {} members; compiled functions naming d16-d31: 0; double-precision sites: {f64_here}; ARM-state mapping symbols: {} [{}]; 64-bit exclusive-access forms: 0",
            members.len(),
            arm_here.len(),
            arm_here.join("; ")
        );
        f64_total += f64_here;
        arm_all.extend(arm_here);
    }
    if arm_all.len() != ARM_MODE_PINNED_COUNT {
        rt.fail(case, &format!("ARM-state mapping symbols over the set: {}, the pin is {ARM_MODE_PINNED_COUNT}", arm_all.len()));
    }
    if d32_all.len() != D32_PINNED.len() {
        rt.fail(case, &format!("functions naming d16-d31 over the set: {} [{}], the pin is the assembly pair", d32_all.len(), d32_all.join("; ")));
    }
    println!("  the pinned assembly pair naming d16-d31: [{}]", d32_all.join("; "));
    rt.pass(case, "positive", "", &format!("{{\"archives\":{},\"d32_compiled\":0,\"d32_pinned_asm\":{},\"f64_sites\":{f64_total},\"arm_mode\":{},\"xdouble\":0}}", MERGE_ARCHIVES.len(), d32_all.len(), arm_all.len()));
    println!("  PASS");

    // The selector control.
    let ccase = "rt.rederive-selector-control";
    println!("== case {ccase} (negative control): the same merge with {SELECTOR_CONTROL} in the selector's place ==");
    let cpost = merge(&mut rt, PAYLOAD, SELECTOR_CONTROL, "selector-control");
    let csha = sha256_file(&cpost).unwrap_or_default();
    let want_merged = rt.record_sha(MERGED);
    let cdef = {
        let arg = cpost.to_string_lossy().into_owned();
        let dump = rt.tool_text(&nm, ccase, &["--defined-only", "--extern-only", &arg], "selector-control.nm-def.txt");
        parse_defined(&dump)
    };
    let witness_absent = !cdef.iter().any(|d| d == SELECTOR_WITNESS);
    let broke = "the standalone-GC-disabled selector in the enabled one's place";
    if csha != want_merged && witness_absent {
        println!("  refused as designed: the control object ({}...) differs from the frozen one and lacks '{SELECTOR_WITNESS}'", &csha[..16]);
        rt.pass(ccase, "negative", broke, "");
        println!("  PASS (control)");
    } else {
        rt.fail_control(ccase, broke, "the other selector reproduced the frozen object or carries the witness");
    }
    rt.results.emit_summary(w.rederive_gate, "pass", 0);
    if !validate_results(repo, rt.results.path()) {
        println!("{}: FAIL -- results file failed schema validation", w.rederive_display);
        exit(1);
    }
    println!(
        "{}: PASS (4/4 frozen objects re-derived byte-identical -- the surgery object {how_surgery}; the merged object {how_merged}; the suite merge {how_suite}; the closure whole, its producer stamp {closure_stamp}; the archive set and the kit's libraries tie the frozen identities; the set censused: no compiled function naming d16-d31, the ARM-state code the pinned pair, no 64-bit exclusive-access instruction in any member; selector control refused)  [{}]",
        w.rederive_display,
        ld.version
    );
    exit(0);
}

/// `rhu-gate rt attrs <pre> <donor> <out>`: the runner's own attributes
/// update for the freeze -- the same bytes the rederive leg produces.
pub fn attrs(_repo: &Path, args: &[String]) -> ! {
    if args.len() != 3 {
        println!("usage: rhu-gate rt attrs <pre-object> <donor-blob> <out-object>");
        exit(2);
    }
    let (pre, donor, out) = (Path::new(&args[0]), Path::new(&args[1]), Path::new(&args[2]));
    match attrs_update_file(pre, donor, out) {
        Ok((pre_sha, n)) => {
            let out_sha = sha256_file(out).unwrap_or_default();
            println!(
                "rt attrs: {} written -- the .ARM.attributes section replaced by the {n}-byte donor {}; pre-object sha256 {pre_sha}; result sha256 {out_sha}",
                out.display(),
                donor.display()
            );
            exit(0);
        }
        Err(e) => {
            println!("rt attrs: REFUSED -- {e}");
            exit(1);
        }
    }
}

/// The parity legs' program for the debugger's unhandled-exception event
/// (src/rt/gate/unhandled), as far as the event's runs read it: one
/// exception nothing catches, thrown two calls below Main, with a handler
/// of the program's own on the unhandled-exception event.
const EVENT_PROGRAM: EventProgram = EventProgram {
    main_ending: "_Rhu_Hosted_Program__Main",
    thrower_ending: "_Rhu_Hosted_Program__ThrowIt",
    type_symbol: "_ZTV44S_P_CoreLib_System_InvalidOperationException",
    handler_line: "UNHANDLED: 3 the program's own handler ran",
    unhandled_line: "Unhandled exception. System.InvalidOperationException",
    before_line: "UNHANDLED: 2 throwing what nothing catches",
    exit_unhandled: 134,
    exit_fault: 2,
};

/// The unhandled event's reading of one image and, when the reading
/// holds, its four runs through the emulator's debug port: one line per
/// reading and per run, each `(true, what was seen)` or `(false, what did
/// not hold)`. A reading that is refused starts no run.
fn event_lines(qemu: &Path, machine: &str, image: &Path, out: &Path, tag: &str) -> Vec<(String, bool, String)> {
    let img = match std::fs::read(image).map_err(|e| format!("cannot read {}: {e}", image.display())).and_then(ArmImage::read) {
        Ok(i) => i,
        Err(e) => return vec![("READING".to_string(), false, e)],
    };
    let facts = match read_event_image(&img) {
        Ok(f) => f,
        Err(e) => return vec![("READING".to_string(), false, e)],
    };
    let mut lines = vec![(
        "READING".to_string(),
        true,
        format!(
            "the events word at {:#010x} inside the zero-filled window; the routine at {:#010x}, {} bytes, with an index-table entry of its own that unwinds by the link register; one BKPT #0x60, at {:#010x}",
            facts.word, facts.routine, facts.routine_size, facts.bkpt
        ),
    )];
    for run in EventRun::ALL {
        let name = format!("RUN {}", run.name());
        match event_run(qemu, machine, image, &img, &facts, &EVENT_PROGRAM, run, out, tag) {
            Ok(seen) => lines.push((name, true, seen)),
            Err(why) => lines.push((name, false, why)),
        }
    }
    lines
}

/// `rhu-gate rt debug-event <machine> <image> <out-dir>`: the unhandled
/// event's reading of one published image of the legs' program and its
/// four runs through the emulator's debug port, one line each, the
/// consoles and the packets left in the directory.
/// `rhu-gate rt debug-event doctor <image> <out-image>`: the control's
/// doctored copy, the routine's BKPT overwritten by a no-operation.
pub fn debug_event(repo: &Path, args: &[String]) -> ! {
    if args.len() != 3 {
        println!("usage: rhu-gate rt debug-event <machine> <image> <out-dir> | rhu-gate rt debug-event doctor <image> <out-image>");
        exit(2);
    }
    if args[0] == "doctor" {
        let doctored = std::fs::read(&args[1]).map_err(|e| format!("cannot read {}: {e}", args[1])).and_then(|b| doctor_event_bkpt(&b));
        match doctored.and_then(|d| std::fs::write(&args[2], &d).map_err(|e| format!("cannot write {}: {e}", args[2])).map(|_| sha256_hex(&d))) {
            Ok(sha) => {
                println!("debug-event: {} written -- the routine's BKPT #0x60 overwritten by a no-operation; sha256 {sha}", args[2]);
                exit(0);
            }
            Err(e) => {
                println!("debug-event: REFUSED -- {e}");
                exit(1);
            }
        }
    }
    let (machine, image, out) = (args[0].as_str(), Path::new(&args[1]), Path::new(&args[2]));
    if std::fs::create_dir_all(out).is_err() {
        println!("debug-event: REFUSED -- cannot create {}", out.display());
        exit(1);
    }
    let (qemu, qemu_version) = match resolve_qemu(repo) {
        Ok(q) => q,
        Err(e) => {
            println!("debug-event: REFUSED -- {e}");
            exit(1);
        }
    };
    let lines = event_lines(&qemu, machine, image, out, "debug-event");
    let held = lines.iter().filter(|l| l.1).count();
    for (name, ok, text) in &lines {
        println!("debug-event: {name} {} -- {text}", if *ok { "HOLDS" } else if name == "READING" { "REFUSED" } else { "FAILS" });
    }
    println!("debug-event: {held} of {} hold ({machine}; {qemu_version})", 1 + EventRun::ALL.len());
    exit(if held == 1 + EventRun::ALL.len() { 0 } else { 1 });
}

/// A frozen input of the selfcheck's own probes, read whole from the rt
/// tier -- or a loud refusal naming the file. The probes doctor REAL frozen
/// bytes; an empty default in their place would let a doctored input
/// compare equal to a genuine one and report the checker broken when it is
/// the tier that is absent.
fn selfcheck_frozen(repo: &Path, w: &RtWorld, name: &str) -> Vec<u8> {
    match std::fs::read(repo.join(tier_of(w, name).replace('/', std::path::MAIN_SEPARATOR_STR)).join(name)) {
        Ok(b) => b,
        Err(e) => {
            println!("rhu-gate {} selfcheck: {}/{name} unreadable: {e}", w.gate, tier_of(w, name));
            exit(1);
        }
    }
}

pub fn selfcheck(repo: &Path, w: &'static RtWorld) -> ! {
    let facts = match rustc_facts() {
        Ok(f) => f,
        Err(e) => {
            println!("rhu-gate: {e}");
            exit(1);
        }
    };
    if !repo.join(w.tier.replace('/', std::path::MAIN_SEPARATOR_STR)).join("MANIFEST.md").is_file() {
        println!("rhu-gate {} selfcheck: frozen inputs missing ({})", w.gate, w.tier);
        exit(1);
    }
    let out = repo.join("src").join("kernel").join("target").join(w.out_dir);
    let _ = std::fs::create_dir_all(&out);
    let results = match Results::create(&out.join(w.selfcheck_results)) {
        Ok(r) => r,
        Err(e) => {
            println!("rhu-gate: cannot create results file: {e}");
            exit(1);
        }
    };
    results.emit(&format!(
        "{{\"schema\":1,\"type\":\"run\",\"gate\":\"{}\",\"run\":\"{}-{}\",\"host\":\"{}\",\"runner\":\"{RUNNER}\",\"tools\":{{\"rustc\":\"{}\"}}}}",
        w.selfcheck_gate,
        w.selfcheck_gate,
        std::process::id(),
        w.host,
        facts.version
    ));
    let mut sc = Selfcheck { repo: repo.to_path_buf(), gate: w.selfcheck_gate.to_string(), display: w.selfcheck_display.to_string(), results, n: 0, npass: 0, nneg: 0 };

    let header_ok = "File: x\nFormat: elf32-littlearm\nElfHeader {\n  Class: 32-bit (0x1)\n  Type: Relocatable (0x1)\n  Machine: EM_ARM (0x28)\n}\n";
    sc.accept_probe("header-genuine", "a 32-bit ARM relocatable header accepted", header_fault(header_ok).is_none());
    sc.probe("header-executable", "an executable's header (Type: Executable) must be refused", header_fault(&header_ok.replace("Type: Relocatable (0x1)", "Type: Executable (0x2)")).is_none());
    sc.probe("header-x86", "a non-ARM header (Machine: EM_386) must be refused", header_fault(&header_ok.replace("Machine: EM_ARM (0x28)", "Machine: EM_386 (0x3)")).is_none());

    let attrs_m_fixture = "BuildAttributes {\n  Attribute {\n    Tag: 5\n    TagName: CPU_name\n    Value: 7E-M\n  }\n  Attribute {\n    Tag: 6\n    Value: 13\n    TagName: CPU_arch\n    Description: ARM v7E-M\n  }\n  Attribute {\n    Tag: 7\n    Value: 77\n    TagName: CPU_arch_profile\n    Description: Microcontroller\n  }\n  Attribute {\n    Tag: 9\n    Value: 2\n    TagName: THUMB_ISA_use\n    Description: Thumb-2\n  }\n  Attribute {\n    Tag: 28\n    Value: 1\n    TagName: ABI_VFP_args\n    Description: AAPCS VFP\n  }\n}\n";
    let attrs_m = attrs_m_fixture
        .replace("Description: ARM v7E-M", &format!("Description: {}", w.cpu_arch))
        .replace("Description: Thumb-2", &format!("Description: {}", w.thumb_isa));
    sc.accept_probe("attrs-genuine", "the microcontroller-profile hard-float attributes accepted", attrs_fault(&attrs_m, w).is_none());
    let attrs_a = attrs_m.replace(&format!("Description: {}", w.cpu_arch), "Description: ARM v7").replace("Description: Microcontroller", "Description: Application");
    sc.probe("attrs-application-profile", "the archives' own application-profile attributes must be refused", attrs_fault(&attrs_a, w).is_none());
    let attrs_lld_pre = "BuildAttributes {\n  Attribute {\n    Tag: 6\n    Value: 2\n    TagName: CPU_arch\n    Description: ARM v4T\n  }\n  Attribute {\n    Tag: 28\n    Value: 1\n    TagName: ABI_VFP_args\n    Description: AAPCS VFP\n  }\n}\n";
    sc.probe("attrs-profile-absent", "a merged object whose profile tag is absent (the linker's own minimal section) must be refused", attrs_fault(attrs_lld_pre, w).is_none());
    let attrs_soft = attrs_m.replace("Description: AAPCS VFP", "Description: AAPCS");
    sc.probe("attrs-soft-float", "microcontroller attributes with soft-float argument passing must be refused", attrs_fault(&attrs_soft, w).is_none());

    let want: Vec<String> = ["a", "b", "c"].iter().map(|s| s.to_string()).collect();
    sc.accept_probe("undef-genuine", "an undefined set equal to the frozen list accepted", undef_fault(&want, &want).is_none());
    let extra: Vec<String> = ["a", "b", "c", "d"].iter().map(|s| s.to_string()).collect();
    sc.probe("undef-extra", "an undefined set with one NEW name must be refused", undef_fault(&extra, &want).is_none());
    let missing: Vec<String> = ["a", "c"].iter().map(|s| s.to_string()).collect();
    sc.probe("undef-missing", "an undefined set with one VANISHED name must be refused", undef_fault(&missing, &want).is_none());

    let manifest_fixture = "# goldens/rt-m7\n\narchives: rc1-rhu-cortex-m7-sgc-enabled abcd\n\n- `goldens/rt-m7/x.o`\n  `0000000000000000000000000000000000000000000000000000000000000000`\n";
    let set_file_fixture = "set-id: rc1-rhu-cortex-m7-sgc-enabled\narchive libRuntime.WorkstationGC.a 1 ff\narchives-sha256: abcd\n";
    let manifest = manifest_fixture.replace("rc1-rhu-cortex-m7-sgc-enabled", w.archive_set);
    let set_file = set_file_fixture.replace("rc1-rhu-cortex-m7-sgc-enabled", w.archive_set);
    let other_set = format!("{}-disabled", w.archive_set.trim_end_matches("-enabled"));
    sc.accept_probe("set-genuine", "a manifest and set file naming the consumed set with equal shas accepted", set_fault(&manifest, &set_file, w.archive_set).is_none());
    sc.probe("set-other-id", "a manifest naming another archive set must be refused", set_fault(&manifest.replace(w.archive_set, &other_set), &set_file, w.archive_set).is_none());
    sc.probe("set-sha-disagree", "a set file whose archives sha disagrees with the manifest must be refused", set_fault(&manifest, &set_file.replace("archives-sha256: abcd", "archives-sha256: abce"), w.archive_set).is_none());

    let records: Vec<(String, String)> = frozen_files(w).iter().map(|(n, _)| (format!("{}/{n}", tier_of(w, n)), "0".repeat(64))).collect();
    let on_disk: Vec<String> = frozen_files(w).iter().map(|(n, _)| n.to_string()).collect();
    let mut extra_disk = on_disk.clone();
    extra_disk.push("stray.o".to_string());
    sc.probe("frozen-file-without-record", "an on-disk file without a manifest record must be refused", frozen_consistency(w, &records, &extra_disk).is_ok());
    let mut outside = records.clone();
    outside[0].0 = "goldens/shim/rthello.o".to_string();
    sc.probe("frozen-record-outside", "a manifest record outside the world's tier must be refused", frozen_consistency(w, &outside, &on_disk).is_ok());
    let sha_a = "a".repeat(64);
    sc.accept_probe("record-genuine", "a record equal to the file's sha accepted", record_fault("goldens/rt-m7/x.o", &sha_a, &sha_a).is_none());
    let sha_b = format!("{}b", "a".repeat(63));
    sc.probe("record-flipped", "a record with one flipped digit must be refused", record_fault("goldens/rt-m7/x.o", &sha_b, &sha_a).is_none());

    let defined: Vec<String> = SPOT_DEFINED.iter().map(|s| s.to_string()).collect();
    sc.accept_probe("spot-genuine", "a defined surface carrying every spot name accepted", spot_fault(&defined).is_none());
    let mut without: Vec<String> = defined.clone();
    without.retain(|d| d != "RhRegisterOSModule");
    sc.probe("spot-missing", "a defined surface missing RhRegisterOSModule must be refused", spot_fault(&without).is_none());
    let mut without_desc: Vec<String> = defined.clone();
    without_desc.retain(|d| d != DESCRIPTOR);
    sc.probe("spot-missing-descriptor", "a defined surface missing the data-contract descriptor must be refused", spot_fault(&without_desc).is_none());

    sc.accept_probe("absent-genuine", "a defined surface without the absent feature's exports accepted", absent_fault(&defined).is_none());
    let mut with_objc = defined.clone();
    with_objc.push(ABSENT_DEFINED[0].to_string());
    sc.probe("absent-present", "a defined surface carrying an export of the absent feature must be refused", absent_fault(&with_objc).is_none());

    sc.accept_probe("forbidden-genuine", "a defined surface without a platform layer accepted", forbidden_fault(&defined).is_none());
    let mut with_thunk = defined.clone();
    with_thunk.push("SystemNative_Write$thunk".to_string());
    sc.accept_probe("forbidden-thunk-genuine", "a defined surface carrying the compiler's own SystemNative_Write$thunk accepted (the program's thunk, not a platform layer)", forbidden_fault(&with_thunk).is_none());
    let mut with_pal = defined.clone();
    with_pal.push("SystemNative_Write".to_string());
    sc.probe("forbidden-pal", "a defined surface carrying SystemNative_Write (a merged PAL) must be refused", forbidden_fault(&with_pal).is_none());

    let cookie_ok = "00000238 g     F .text.thunks\t00000004 __security_cookie$thunk\n00000000 g     O .data.__security_cookie\t00000004 .hidden __security_cookie\n";
    sc.accept_probe("cookie-genuine", "the cookie as a data definition in its renamed section accepted", cookie_section_fault(cookie_ok).is_none());
    sc.probe("cookie-rodata", "the cookie in a read-only section (the surgery not held) must be refused", cookie_section_fault(&cookie_ok.replace(".data.__security_cookie", ".rodata")).is_none());
    sc.probe("cookie-absent", "a symbol table without the cookie must be refused", cookie_section_fault("00000238 g     F .text.thunks\t00000004 __security_cookie$thunk\n").is_none());

    let libc_ok: Vec<String> = ["log", "vfscanf", "_impure_ptr", "printf"].iter().map(|s| s.to_string()).collect();
    sc.accept_probe("libc-forbidden-genuine", "a closure without the glue's names accepted", libc_forbidden_fault(&libc_ok).is_none());
    let mut libc_bad = libc_ok.clone();
    libc_bad.push("_sbrk_r".to_string());
    sc.probe("libc-forbidden-break", "a closure defining the library's heap break must be refused", libc_forbidden_fault(&libc_bad).is_none());

    let members_ok = "# list\narchive libc_nano.a 10 0000000000000000000000000000000000000000000000000000000000000000\narchive libm.a 20 1111111111111111111111111111111111111111111111111111111111111111\nmember libc_nano.a libc_a-printf.o\nmember libm.a libm_a-log.o\n";
    sc.accept_probe("members-genuine", "a member list naming the two libraries and their members accepted", parse_members(members_ok).is_ok());
    sc.probe("members-foreign-archive", "a member list naming an archive the closure is not drawn from must be refused", parse_members(&members_ok.replace("archive libm.a", "archive libgcc.a")).is_ok());
    sc.probe("members-unlisted-archive", "a member naming an archive the list does not identify must be refused", parse_members(&members_ok.replace("member libm.a", "member libnosys.a")).is_ok());
    sc.probe("members-empty", "a member list with no member must be refused", parse_members("archive libc_nano.a 10 0000000000000000000000000000000000000000000000000000000000000000\narchive libm.a 20 1111111111111111111111111111111111111111111111111111111111111111\n").is_ok());

    sc.accept_probe("window-inside", "a symbol inside the copy window accepted", window_fault("x", 0x2000_1120, 0x2000_0000, 0x2000_24d8, "copy window").is_none());
    sc.probe("window-outside", "a symbol at the window's end must be refused", window_fault("x", 0x2000_24d8, 0x2000_0000, 0x2000_24d8, "copy window").is_none());

    let min_image: Vec<u8> = b"\x7fELFsome bytes of a min image".to_vec();
    sc.accept_probe("flavour-absent", "an image without the resource sentence accepted", flavour_fault(&min_image).is_none());
    let mut rich_image = min_image.clone();
    rich_image.extend_from_slice(&utf16le(FLAVOUR_MARK));
    sc.probe("flavour-present", "an image carrying the resource sentence as UTF-16 must be refused", flavour_fault(&rich_image).is_none());

    let golden = selfcheck_frozen(repo, w, GOLDEN);
    let mut wrapped: Vec<u8> = b"rhu 0.2.0 -- kernel banner\r\n[boot] kernel line\r\n[mgc] a collector line\r\n".to_vec();
    wrapped.extend_from_slice(&golden);
    wrapped.extend_from_slice(b"[payload] managed Main returned 0\r\n");
    sc.accept_probe("golden-genuine", "the genuine payload stream inside kernel and collector lines equals the golden", eq_lf_normalized(&payload_stream(&wrapped), &golden));
    let flipped: Vec<u8> = String::from_utf8_lossy(&wrapped).replace("ALL GOOD", "ALL G00D").into_bytes();
    sc.probe("golden-flipped", "payload stream with the verdict line's GOOD changed to G00D", eq_lf_normalized(&payload_stream(&flipped), &golden));
    sc.accept_probe("verdict-last", "the genuine golden ends on the verdict line", String::from_utf8_lossy(&golden).lines().last() == Some(BOOT_ALL_GOOD));

    let ordered: Vec<u8> = BOOT_MARKERS.join("\r\n").into_bytes();
    sc.accept_probe("boot-order-genuine", "a capture carrying the boot's markers in order", markers_out_of_order(&ordered, &BOOT_MARKERS).is_none());
    let mut swapped: Vec<&str> = BOOT_MARKERS.to_vec();
    swapped.swap(11, 16);
    let swapped_capture: Vec<u8> = swapped.join("\r\n").into_bytes();
    sc.probe("boot-order-swapped", "every marker present, the runtime initialized after managed entry", markers_out_of_order(&swapped_capture, &BOOT_MARKERS).is_none());

    let suite_ordered: Vec<u8> = SUITE_MARKERS_M7.join("\r\n").into_bytes();
    sc.accept_probe("suite-order-genuine", "a capture carrying the suite's markers in order", markers_out_of_order(&suite_ordered, &SUITE_MARKERS_M7).is_none());
    let mut suite_swapped: Vec<&str> = SUITE_MARKERS_M7.to_vec();
    suite_swapped.swap(5, 13);
    let suite_swapped_capture: Vec<u8> = suite_swapped.join("\r\n").into_bytes();
    sc.probe("suite-order-swapped", "the threading section's verdict before the first section's", markers_out_of_order(&suite_swapped_capture, &SUITE_MARKERS_M7).is_none());
    sc.accept_probe("activations-genuine", "an activation line at the floor accepted", activation_count(b"[host] activations delivered: 3\r\n", ACTIVATIONS_FLOOR).is_ok());
    sc.probe("activations-below-floor", "an activation line below the floor of three must be refused", activation_count(b"[host] activations delivered: 0\r\n", 3).is_ok());
    sc.probe("activations-absent", "a capture without the activation line must be refused", activation_count(b"[host] exit: 0\r\n", ACTIVATIONS_FLOOR).is_ok());
    let suite_broken: Vec<u8> = String::from_utf8_lossy(&suite_ordered).replace("PASS rt-threads", "FAIL rt-threads").into_bytes();
    sc.probe("suite-break-forbidden", "a capture with one section's verdict turned FAIL must trip the forbidden sweep", !SUITE_FORBIDDEN.iter().any(|m| contains_fixed(&suite_broken, m)));
    sc.probe("suite-break-order", "a capture with one section's verdict turned FAIL must lose that marker", markers_out_of_order(&suite_broken, &SUITE_MARKERS_M7).is_none());

    let manifest_short = manifest.replace("0000000000000000000000000000000000000000000000000000000000000000", "000000000000000000000000000000000000000000000000000000000000000");
    sc.probe("manifest-sha-malformed", "a manifest record whose sha is 63 hex digits must parse to fewer records (a drifted record set)", parse_vendored_manifest(&manifest_short).len() == parse_vendored_manifest(&manifest).len());

    // The census's detectors, the runner's attributes update, the delivery control's doctoring.
    let dis_hi = "00000000 <f>:\n       0: ec d1 7b 2b\tvldr\td17, [r1, #8]\n";
    sc.accept_probe("census-sees-d17", "a decode naming d17 in its operands is counted", d32_census(dis_hi).len() == 1);
    let dis_lo = "00000000 <f>:\n       0: ed 91 7b 02\tvldr\td7, [r1, #8]\n";
    sc.accept_probe("census-passes-d7", "a decode naming d7 is not counted", d32_census(dis_lo).is_empty());
    let dis_addr = "00000000 <f>:\n     d16: 2b 7b\tldr\tr3, [r7, #0x30]\n";
    sc.accept_probe("census-ignores-address", "an instruction at address d16 is not a register name", d32_census(dis_addr).is_empty());
    sc.probe("census-d17-as-clean", "a decode with a d16-d31 site offered as clean", d32_census(dis_hi).is_empty());
    sc.probe("census-unpinned-function", "a compiled function naming d17 must be refused against the pin", d32_fault(&d32_census(dis_hi), false).is_none());
    let dis_pinned = "00000000 <_ZN9libunwind13Registers_arm9saveVFPv3EPv>:\n       0: ec 80 0b 20\tvstmia\tr0, {d16, d17}\n00000004 <_ZN9libunwind13Registers_arm12restoreVFPv3EPv>:\n       4: ec 90 0b 20\tvldmia\tr0, {d16, d17}\n";
    sc.accept_probe("census-pinned-pair", "the two pinned assembly functions with one site each pass the pin", d32_fault(&d32_census(dis_pinned), true).is_none());
    let dis_pinned_twice = format!("{dis_pinned}       8: ec 90 0b 20\tvldmia\tr0, {{d18, d19}}\n");
    sc.probe("census-pinned-second-site", "a pinned assembly function with a second site must be refused", d32_fault(&d32_census(&dis_pinned_twice), true).is_none());
    let syms_map = "   Num:    Value  Size Type    Bind   Vis       Ndx Name\n     1: 00000010     0 NOTYPE  LOCAL  DEFAULT     1 $a\n     2: 00000020     0 NOTYPE  LOCAL  DEFAULT     1 $t\n     3: 00000000     0 NOTYPE  LOCAL  DEFAULT     1 $t\n";
    let sections_map = "  [Nr] Name              Type            Address  Off    Size   ES Flg Lk Inf Al\n  [ 0]                   NULL            00000000 000000 000000 00      0   0  0\n  [ 1] .text             PROGBITS        00000000 000034 000100 00  AX  0   0  4\n";
    let regions = arm_regions(syms_map, sections_map);
    sc.accept_probe("census-arm-region-read", "one ARM-state region from an $a at 0x10 to the $t at 0x20 in .text", regions == vec![(".text".to_string(), 0x10, 0x20)]);
    let dis_sec = format!("{} of section .text:\n\n00000000 <f>:\n       0: ec d1 7b 2b\tvldr\td17, [r1, #8]\n00000010 <g>:\n      10: ec d1 7b 2b\tvldr\td17, [r1, #8]\n", "Disassembly");
    sc.accept_probe("census-regions-outside", "the Thumb walk counts the function outside the ARM-state region only", d32_census_regions(&dis_sec, &regions, false) == vec![("<f>:".to_string(), 1)]);
    sc.accept_probe("census-regions-inside", "the ARM walk counts the function inside the ARM-state region only", d32_census_regions(&dis_sec, &regions, true) == vec![("<g>:".to_string(), 1)]);
    let dis_f64 = "00000000 <f>:\n       0: ee 21 0b 01\tvmul.f64\td0, d1, d1\n       4: ee 20 0a 01\tvmul.f32\ts0, s0, s2\n";
    sc.accept_probe("census-counts-f64", "a decode with one double-precision multiply counts one", f64_census(dis_f64) == 1);
    let dis_xd = "00000000 <f>:\n       0: e8d0 017f    \t<unknown>\n       4: e8cc 2375    \t<unknown>\n       8: e8dc 017f    \t<unknown>\n";
    sc.accept_probe("census-sees-exclusive-double", "a decode with the two 64-bit exclusive-access forms as unknown lines counts three sites in one function", xdouble_census(dis_xd) == vec![("<f>:".to_string(), 3)]);
    let dis_xd_bytes = "00000000 <f>:\n       0: d0 e8 7f 01\tldrexd\tr0, r1, [r0]\n";
    sc.accept_probe("census-exclusive-double-bytes", "the same form printed as four bytes by name is counted", xdouble_census(dis_xd_bytes) == vec![("<f>:".to_string(), 1)]);
    let dis_x32 = "00000000 <f>:\n       0: e850 0f00    \tldrex\tr0, [r0]\n       4: e840 1200    \tstrex\tr2, r1, [r0]\n       8: e8d0 0f4f    \tldrexb\tr0, [r0]\n       c: e8d0 0f5f    \tldrexh\tr0, [r0]\n      10: e8c0 1f42    \tstrexb\tr2, r1, [r0]\n      14: f3bf 8f2f    \tclrex\n";
    sc.accept_probe("census-passes-32-bit-exclusives", "the word, byte and halfword exclusives and clrex are not counted", xdouble_census(dis_x32).is_empty());
    let dis_half = "00000000 <f>:\n       0: e8d0 f000    \ttbb\t[r0, r0]\n       4: b5f0         \tpush\t{r4, r5, r6, r7, lr}\n";
    sc.accept_probe("census-second-halfword-checked", "a line whose first halfword matches the form and whose second does not, and a 16-bit instruction, are not counted", xdouble_census(dis_half).is_empty());
    sc.probe("census-exclusive-double-as-clean", "a decode carrying the forms offered as clean", xdouble_census(dis_xd).is_empty());
    let dis_xd_sec = format!("{} of section .text:\n\n00000000 <f>:\n       0: e8d0 017f    \t<unknown>\n00000010 <g>:\n      10: 9f 0f b0 e1\tldrexd\tr0, r1, [r0]\n", "Disassembly");
    sc.accept_probe("census-exclusive-double-regions", "the Thumb walk counts the function outside the ARM-state region and the ARM walk the one inside, merged by function", xdouble_total(&dis_xd_sec, &dis_xd_sec, &regions) == vec![("<f>:".to_string(), 1), ("<g>:".to_string(), 1)]);
    let syms_arm = "   Num:    Value  Size Type    Bind   Vis       Ndx Name\n     1: 00000000     0 NOTYPE  LOCAL  DEFAULT     1 $a\n     2: 00000000     0 FUNC    LOCAL  DEFAULT     1 __unw_getcontext\n";
    sc.accept_probe("arm-mode-seen", "a symbol table with one ARM-state mapping symbol at a pinned function reads one and passes the pin of one", arm_mode_fault(&arm_mode_symbols(syms_arm), syms_arm, 1).is_none());
    let syms_thumb = syms_arm.replace("$a", "$t");
    sc.accept_probe("arm-mode-none", "a symbol table with Thumb mapping symbols only reads zero", arm_mode_symbols(&syms_thumb).is_empty());
    let syms_stray = syms_arm.replace("__unw_getcontext", "SomeOtherFunction");
    sc.probe("arm-mode-unpinned", "an ARM-state mapping symbol at an unpinned function must be refused", arm_mode_fault(&arm_mode_symbols(&syms_stray), &syms_stray, 1).is_none());
    let merged = selfcheck_frozen(repo, w, MERGED);
    let donor = selfcheck_frozen(repo, w, w.donor);
    let once = attrs_update(&merged, &donor);
    let twice = attrs_update(&merged, &donor);
    sc.accept_probe("attrs-update-deterministic", "the runner's attributes update gives the same bytes twice on the frozen merged object", once.is_ok() && once == twice);
    sc.accept_probe("attrs-update-applied", "the updated object's .ARM.attributes section reads the donor's bytes", once.as_ref().ok().and_then(|o| attrs_section(o)) == Some(donor.clone()));
    sc.probe("attrs-update-not-elf", "the attributes update must refuse a non-ELF input", attrs_update(b"not an object", &donor).is_ok());
    sc.probe("delivery-zeroed", "the suite's capture with its activation count set to zero must be refused at the floor", activation_count(&zero_delivery(b"[host] activations delivered: 7\r\n"), ACTIVATIONS_FLOOR).is_ok());

    let surgery_path = repo.join(w.tier.replace('/', std::path::MAIN_SEPARATOR_STR)).join(SURGERY);
    let surgery_bytes = selfcheck_frozen(repo, w, SURGERY);
    let stripped = out.join("selfcheck-stripped");
    let stamp_base = sha256_without_stamp(&facts, &surgery_path, &stripped.join("genuine.o")).unwrap_or_default();
    let code_at = section_span_prefixed(&surgery_bytes, ".text").map(|(o, _)| o);
    let mut code_moved = surgery_bytes.clone();
    if let Some(at) = code_at {
        if let Some(b) = code_moved.get_mut(at) {
            *b ^= 0x01;
        }
    }
    write_fixture(&out.join("selfcheck-stamp-code-moved.o"), &code_moved);
    let code_sha = sha256_without_stamp(&facts, &out.join("selfcheck-stamp-code-moved.o"), &stripped.join("code-moved.o")).unwrap_or_default();
    sc.probe(
        "stamp-compare-code-byte",
        "a copy of the frozen surgery object with one code byte moved, compared apart from the stamp",
        code_at.is_none() || stamp_base.is_empty() || code_sha.is_empty() || code_sha == stamp_base,
    );
    let stamp_at = section_span(&surgery_bytes, ".comment");
    let mut stamp_moved = surgery_bytes.clone();
    if let Some((o, s)) = stamp_at {
        if s > 2 {
            if let Some(b) = stamp_moved.get_mut(o + 1) {
                *b ^= 0x01;
            }
        }
    }
    write_fixture(&out.join("selfcheck-stamp-stamp-moved.o"), &stamp_moved);
    let stamp_sha = sha256_without_stamp(&facts, &out.join("selfcheck-stamp-stamp-moved.o"), &stripped.join("stamp-moved.o")).unwrap_or_default();
    sc.accept_probe(
        "stamp-compare-stamp-only",
        "a copy of the frozen surgery object whose producer stamp alone is changed ties apart from the stamp while the files differ",
        stamp_at.is_some() && !stamp_base.is_empty() && stamp_sha == stamp_base && sha256_hex(&stamp_moved) != sha256_hex(&surgery_bytes),
    );

    // The board table's stack facts against the kernel's own text: the tie the parity leg's case
    // rt.sdk-parity-row-facts runs, probed here on the tree itself -- its facts accepted, and the
    // first row's StackKernel one byte off the kernel's own threads refused.
    let stack_facts = match read_stack_facts(repo) {
        Ok(f) => f,
        Err(e) => {
            println!("rhu-gate {} selfcheck: the board table's stack facts are unreadable: {e}", w.gate);
            exit(1);
        }
    };
    sc.accept_probe("stack-facts-genuine", "the board table's stack facts as the tree states them accepted", stack_facts_fault(&stack_facts).is_none());
    let mut stack_facts_off = stack_facts.clone();
    stack_facts_off.rows[0].kernel_own += 1;
    sc.probe("stack-facts-kernel-own", "a row's StackKernel one byte off the kernel's own threads must be refused", stack_facts_fault(&stack_facts_off).is_none());

    // The boards' facts files against the kernel's own text, the board class's half of the same
    // case, probed here on the tree itself for the net object: the facts files as the tree
    // states them accepted with a net object among them, and the first such row's stream
    // receive buffer one byte off the kernel's refused by the net object's own sentence.
    let board_facts = match read_board_facts(repo) {
        Ok(f) => f,
        Err(e) => {
            println!("rhu-gate {} selfcheck: the boards' facts files are unreadable: {e}", w.gate);
            exit(1);
        }
    };
    sc.accept_probe("net-facts-genuine", "the boards' facts files as the tree states them accepted, a net object among them", board_facts_fault(&board_facts).is_none() && board_facts.iter().any(|r| r.net.is_some()));
    let mut net_facts_off = board_facts.clone();
    if let Some(n) = net_facts_off.iter_mut().find_map(|r| r.net.as_mut()) {
        n.3 += 1;
    }
    sc.probe("net-facts-stream-rx", "a facts file's stream receive buffer one byte off the kernel's must be refused", !board_facts_fault(&net_facts_off).is_some_and(|f| f.contains("the facts file's net object")));

    // The reader of the kernel's network text under that tie, probed on the tree's own text for
    // the first row whose facts file carries a net object: the text as the tree states it read,
    // and the same text refused with STREAM_RX, then KIND_WORD, stated for the wired and the
    // virtual medium alone.
    let net_build = std::fs::read_to_string(repo.join("src").join("kernel").join("build.rs")).unwrap_or_default();
    let net_module = std::fs::read_to_string(repo.join("src").join("kernel").join("src").join("net.rs")).unwrap_or_default();
    let net_feature = board_facts.iter().find(|r| r.net.is_some()).map(|r| format!("board-{}", r.board.strip_prefix("arm-").unwrap_or(&r.board))).unwrap_or_default();
    let restated = |head: &str, line: &str| -> String { net_module.lines().map(|l| if l.trim().starts_with(head) { line } else { l }).collect::<Vec<&str>>().join("\n") };
    sc.accept_probe("net-reader-genuine", "the kernel's network text as the tree states it read for a row with a medium", matches!(kernel_net_facts(&net_build, &net_module, &net_feature), Some(Some(_))));
    let rx_two_media = restated("pub const STREAM_RX: usize = if cfg!(", "pub const STREAM_RX: usize = if cfg!(rhu_net_wired) { 4096 } else { 2048 };");
    sc.probe("net-reader-rx-two-media", "the stream receive buffer stated for the wired and the virtual medium alone must be refused", kernel_net_facts(&net_build, &rx_two_media, &net_feature).is_some());
    let word_two_media = restated("pub const KIND_WORD: &str = if cfg!(", "pub const KIND_WORD: &str = if cfg!(rhu_net_wired) { \"wired\" } else { \"virtual\" };");
    sc.probe("net-reader-word-two-media", "the medium's word stated for the wired and the virtual medium alone must be refused", kernel_net_facts(&net_build, &word_two_media, &net_feature).is_some());
    // The radio's block under the same reader, probed for the first row whose facts file says
    // wireless: the tree's own text read with the wireless word and the row's own figures; the
    // same text with the block's refusal line restated for another board alone, which must leave
    // the row without a medium; and the row's stream receive buffer one byte off the kernel's
    // under the tie.
    let wireless_row = board_facts.iter().position(|r| r.net.as_ref().is_some_and(|n| n.0 == "wireless"));
    let wireless_feature = wireless_row.map(|i| format!("board-{}", board_facts[i].board.strip_prefix("arm-").unwrap_or(&board_facts[i].board))).unwrap_or_default();
    sc.accept_probe("net-reader-wireless-genuine", "the kernel's network text as the tree states it read for a row with the radio: the wireless word and the row's own figures", wireless_row.is_some_and(|i| kernel_net_facts(&net_build, &net_module, &wireless_feature).flatten() == board_facts[i].net));
    let radio_refusal = net_build.lines().enumerate().skip_while(|(_, l)| l.trim() != "if cfg!(feature = \"wifi-cyw43\") {").find(|(_, l)| l.trim().starts_with("if !cfg!(any(")).map(|(i, _)| i);
    let radio_other_board = net_build.lines().enumerate().map(|(i, l)| if Some(i) == radio_refusal { "        if !cfg!(any(feature = \"board-other\")) {" } else { l }).collect::<Vec<&str>>().join("\n");
    sc.probe("net-reader-radio-other-board", "the radio's block restated for another board alone must leave the row without a medium", wireless_row.is_none() || !matches!(kernel_net_facts(&radio_other_board, &net_module, &wireless_feature), Some(None)));
    let mut wireless_facts_off = board_facts.clone();
    if let Some(n) = wireless_facts_off.iter_mut().find_map(|r| r.net.as_mut().filter(|n| n.0 == "wireless")) {
        n.3 += 1;
    }
    sc.probe("net-facts-wireless-stream-rx", "a wireless row's stream receive buffer one byte off the kernel's must be refused", wireless_row.is_none() || !board_facts_fault(&wireless_facts_off).is_some_and(|f| f.contains("the facts file's net object")));
    // The flash region's rule under the same tie: the rows as the tree states them, the row with the radio naming
    // its region and its kernel facts stating the corner; and that row's FlashRegions taken off, which must be refused.
    sc.accept_probe("region-rows-genuine", "the board table's flash regions as the tree states them accepted, a row with the radio naming one", board_facts_fault(&board_facts).is_none() && wireless_row.is_some_and(|i| board_facts[i].flash_regions.is_some() && board_facts[i].kernel_corner));
    let mut region_unnamed = board_facts.clone();
    if let Some(i) = wireless_row {
        region_unnamed[i].flash_regions = None;
    }
    sc.probe("region-wireless-row-unnamed", "a row with the radio whose table entry names no flash region must be refused", wireless_row.is_none() || !board_facts_fault(&region_unnamed).is_some_and(|f| f.contains("names no flash region")));
    // The low-power UART's kind under the same tie: the rows as the tree states them, one of them with a block under
    // the kernel's low-power law, its facts file and its table entry saying so; and that row's table entry taken off,
    // which must be refused.
    let lowpower_row = board_facts.iter().position(|r| !r.kernel_uart_lowpower.is_empty());
    sc.accept_probe("uart-kind-rows-genuine", "the rows' low-power UART blocks as the tree states them accepted, a row with such a block naming it in its facts file and its table entry", board_facts_fault(&board_facts).is_none() && lowpower_row.is_some_and(|i| board_facts[i].table_uart_lowpower != 0 && board_facts[i].uart_lowpower == board_facts[i].kernel_uart_lowpower));
    let mut kind_unnamed = board_facts.clone();
    if let Some(i) = lowpower_row {
        kind_unnamed[i].table_uart_lowpower = 0;
    }
    sc.probe("uart-kind-table-unnamed", "a row with a low-power UART whose table entry names no such block must be refused", lowpower_row.is_none() || !board_facts_fault(&kind_unnamed).is_some_and(|f| f.contains("the board table's UartLowPowerBlocks")));
    sc.accept_probe("bus-rows-genuine", "a row whose kernel facts write their bus rows out in full accepted, its buses tied", board_facts.iter().any(|r| r.kernel_buses.is_some() && r.ties.iter().any(|t| t.0.contains("select"))) && board_facts_fault(&board_facts).is_none());
    let unlisted = board_facts.iter().find_map(|r| r.kernel_buses.as_ref().filter(|_| !r.bus_lists.0.is_empty()).map(|k| literal_bus_ties(&r.board, &[], &r.bus_lists.1, &k.0, &k.1)));
    sc.probe("bus-rows-unlisted", "a facts file that lists no I2C bus beside the kernel's bus with pads must be refused", !unlisted.is_some_and(|t| t.is_err_and(|f| f.contains("lists no such bus"))));


    // The PWM table tie, probed on the tree's own rows: the unit's table and the tenants' tables as
    // the tree states them; a table one channel short of the most PWM lines a row states; a tenant's
    // table with its first two lines exchanged.
    let pwm_table = read_pwm_table(repo).unwrap_or(PwmTable { channels: 0, tenants: Vec::new() });
    sc.accept_probe("pwm-table-genuine", "the PWM unit's table and the PWM rows tenants' tables as the tree states them accepted", !pwm_table.tenants.is_empty() && pwm_table_fault(&board_facts, &pwm_table).is_none());
    let mut pwm_short = pwm_table.clone();
    pwm_short.channels = board_facts.iter().map(|r| r.kernel_pwm_lines.len() as u64).max().unwrap_or(0).saturating_sub(1);
    sc.probe("pwm-table-short", "a PWM unit's table one channel short of the most PWM lines a row states must be refused", !pwm_table_fault(&board_facts, &pwm_short).is_some_and(|f| f.contains("more PWM lines than the unit's table")));
    let mut pwm_exchanged = pwm_table.clone();
    if let Some(t) = pwm_exchanged.tenants.first_mut().filter(|t| t.1.len() > 1) {
        t.1.swap(0, 1);
    }
    sc.probe("pwm-table-tenant-order", "a PWM rows tenant's table with two lines exchanged must be refused", !pwm_table_fault(&board_facts, &pwm_exchanged).is_some_and(|f| f.contains("!= the kernel's PWM_LINES")));

    sc.finish("genuine inputs accepted");
}

/// The Linux environment for the publish that the parity leg runs the SDK in: `RHU_PUBLISH` (the
/// command prefix that runs a script there; a double-quoted word keeps
/// its spaces) with `RHU_PUBLISH_ROOT` (this checkout's root in that
/// world), else the two lines of the untracked machine-local
/// tools/toolchains/publish.local, else None.
pub(crate) fn resolve_publish(repo: &Path) -> Option<(String, String)> {
    let env_prefix = std::env::var("RHU_PUBLISH").ok().map(|v| v.trim().to_string()).filter(|v| !v.is_empty());
    let env_root = std::env::var("RHU_PUBLISH_ROOT").ok().map(|v| v.trim().to_string()).filter(|v| !v.is_empty());
    if let (Some(p), Some(r)) = (env_prefix.clone(), env_root.clone()) {
        return Some((p, r));
    }
    let local = repo.join("tools").join("toolchains").join("publish.local");
    let text = std::fs::read_to_string(&local).unwrap_or_default();
    let mut lines = text.lines().map(|l| l.trim().to_string()).filter(|l| !l.is_empty());
    let file_prefix = lines.next();
    let file_root = lines.next();
    match (env_prefix.or(file_prefix), env_root.or(file_root)) {
        (Some(p), Some(r)) => Some((p, r)),
        _ => None,
    }
}

/// The project directory a compiled program object embeds: the printable
/// run ending in the gate's project path as the SDK's map writes it
/// (/rhu/src/rthello), read off the object's bytes.
/// The compiler records the directory it ran in -- the SDK's path map
/// maps it to one fixed string, so an object published at any directory
/// under the map carries the same run, and one published with the map
/// off carries its own directory: the exact tie refuses that one.
fn embedded_project_path(obj: &[u8]) -> Option<String> {
    let needle = b"/rhu/src/rthello";
    let pos = obj.windows(needle.len()).position(|w| w == needle)?;
    let mut start = pos;
    while start > 0 && obj[start - 1] >= 0x20 && obj[start - 1] < 0x7f {
        start -= 1;
    }
    let end = pos + needle.len();
    Some(String::from_utf8_lossy(&obj[start..end]).into_owned())
}

/// One `control <name>: rc=<n> elf=<state> refuse=<text>` line of the
/// publish-side script's control summary.
fn control_line<'a>(text: &'a str, name: &str) -> Option<&'a str> {
    let head = format!("control {name}: ");
    text.lines().find(|l| l.starts_with(&head))
}

fn control_field<'a>(line: &'a str, key: &str) -> &'a str {
    let key_eq = format!("{key}=");
    line.split(' ').find_map(|w| w.strip_prefix(key_eq.as_str())).unwrap_or("")
}

/// A Debug publish's summary line as its seven facts: the publish's code, the
/// image's state, the response file's counts of -O, --Os and -g, and the two
/// keys the image's record names, how the framework's code was compiled and
/// how the program's own.
fn debug_fields(line: &str) -> (&str, &str, &str, &str, &str, &str, &str) {
    (control_field(line, "rc"), control_field(line, "elf"), control_field(line, "opt-O"), control_field(line, "opt-Os"), control_field(line, "opt-g"), control_field(line, "profile"), control_field(line, "program"))
}

/// The SDK's half of the stack-size tie: the board table's thread stack facts (bytes) per row as
/// the SDK ships them, beside the tree's constants they copy -- the kernel's
/// pool for the row, the hosting layer's default and clamp.
#[derive(Clone)]
struct StackRow {
    board: String,
    pool: u64,
    default: u64,
    clamp: u64,
    kernel_pool: u64,
    host_default: u64,
    host_clamp: u64,
    /// The table's StackKernel: the bytes of the row's pool the kernel's own threads hold.
    kernel_own: u64,
    /// The same as the kernel's text states it, thread by thread: the idle thread and the thread
    /// that runs a hosted program's Main. The kernel's network thread and its card watcher take
    /// their stacks from bytes the scheduler's array carries above the row's pool, which the
    /// tie reads in the scheduler's text.
    kernel_threads: [u64; 2],
}

#[derive(Clone)]
struct StackFacts {
    rows: Vec<StackRow>,
    /// The targets' copy of the kernel's minimum stack, and the header's.
    sdk_min: u64,
    kernel_min: u64,
}

/// The stack knob's value for the positive case: above the row's default,
/// so the moved spawn line proves the knob without shrinking the third
/// thread's stack under the program's own needs; below every row's clamp.
const STACK_KNOB: u64 = 20480;
/// The boot marker the knob moves: the hosting layer's third thread asks the
/// row's default through the shim's own thread attributes.
const STACK_KNOB_MARKER_DEFAULT: &str = "[shim] pthread_create: stack 16384 B asked, 16384 B spawned (tid 4)";
/// The portable symbol file's embedded-source record kind by its GUID as the
/// file stores it (0E8A571B-6926-466E-B4AD-8AB04611F5FE): in the file's GUID
/// heap exactly when a source is embedded.
const EMBEDDED_SOURCE_KIND: [u8; 16] = [0x1B, 0x57, 0x8A, 0x0E, 0x26, 0x69, 0x6E, 0x46, 0xB4, 0xAD, 0x8A, 0xB0, 0x46, 0x11, 0xF5, 0xFE];

fn embeds_source(pdb: &[u8]) -> bool {
    pdb.windows(EMBEDDED_SOURCE_KIND.len()).any(|w| w == EMBEDDED_SOURCE_KIND)
}

/// The external RAM init mode's row for the SDK parity cases: the
/// Pimoroni Pico Plus 2 (8 MB of PSRAM behind the flash interface's second
/// select, an external range), its shipped feature set and target (the
/// payload gate's row line), and the three modes as the SDK's property
/// names them, each with the kernel feature it adds (none for the boot-time
/// init, the default) and the banner words the kernel compiles in for it
/// (the arena's mode words). The row's archive per offered mode is built
/// once under RAM_INIT_DIR, shared by both worlds, and packed beside the
/// leg's own kernel under the fixed folder names.
const RAM_INIT_ROW: &str = "pimoroni-pico-plus-2";
const RAM_INIT_TARGET: &str = "thumbv8m.main-none-eabihf";
const RAM_INIT_FEATURES: &str = "board-pimoroni-pico-plus-2,payload";
const RAM_INIT_DIR: &str = "src/kernel/target/sdk-parity-ram-init";
/// The hosting layer's line at boot naming the managed heap's window: 1
/// when the rehydrated-data brackets lie inside the external RAM range
/// (the row's own map), 0 when they lie in the chip's own RAM (the row's
/// second map).
const HEAP_WINDOW_LINE_EXTERNAL: &str = "the managed heap's window is 1 (external";
const HEAP_WINDOW_LINE_INTERNAL: &str = "the managed heap's window is 0 (internal";
/// The STM32F769I-DISCO row, whose board carries a memory-mapped external
/// flash (the facts file's flash array: its external range mapped and
/// executable): its payload kernel is built once under XIP_DIR, shared by
/// both worlds, and packed beside the leg's own kernel, so the leg publishes
/// the row under the program's two homes. The row carries two kernel sets:
/// its kernel is built with the network (XIP_FEATURES), and the second
/// set's default archive, built without it (XIP_PLAIN_FEATURES, under
/// XIP_PLAIN_DIR), is named to the pack as the row's no-network kernel,
/// which a pack of the row requires.
const XIP_ROW: &str = "st-stm32f769i-disco";
const XIP_TARGET: &str = "thumbv7em-none-eabihf";
const XIP_FEATURES: &str = "board-st-stm32f769i-disco,payload,net";
const XIP_DIR: &str = "src/kernel/target/sdk-parity-external-flash";
const XIP_PLAIN_FEATURES: &str = "board-st-stm32f769i-disco,payload";
const XIP_PLAIN_DIR: &str = "src/kernel/target/sdk-parity-external-flash-no-network";
/// The row the Debug configuration's cases fly: a row whose board the board
/// table marks RequiresOptimization. The table's header states the rule
/// (src/sdk/Lamella.Rhu.Sdk/Sdk/boards.props): on such a board an image
/// compiled without the code optimization as a whole is refused at the
/// link. The row's payload kernel is built once under DEBUG_DIR, shared by
/// both worlds, and packed beside the leg's own kernel, so the leg
/// publishes the row in the Debug configuration (the program's own code
/// unoptimized, the framework's optimized, as on every row) and once more
/// compiled without the optimization as a whole, the control the link
/// refuses. The floor is read from the row's map in the tree.
const DEBUG_ROW: &str = "raspberrypi-pico-2";
/// The row for the image compiled without the code optimization as a whole:
/// a row of the board table whose board does not require the optimization,
/// packed by both worlds (the external-RAM row).
const DEBUG_PLAIN_ROW: &str = RAM_INIT_ROW;
/// A Debug publish's seven facts (debug_fields) as each build leaves them:
/// the usual Debug build (the optimization words and the debug word once
/// each; the framework's code optimized, the program's own unoptimized), the
/// image compiled without the optimization as a whole, and the build with
/// the optimization on for the program's own code too.
const DEBUG_USUAL: (&str, &str, &str, &str, &str, &str, &str) = ("0", "present", "1", "1", "1", "optimized", "unoptimized");
const DEBUG_WHOLE: (&str, &str, &str, &str, &str, &str, &str) = ("0", "present", "0", "0", "1", "unoptimized", "unoptimized");
const DEBUG_OPTIMIZED: (&str, &str, &str, &str, &str, &str, &str) = ("0", "present", "1", "1", "1", "optimized", "optimized");
const DEBUG_TARGET: &str = "thumbv8m.main-none-eabihf";
const DEBUG_FEATURES: &str = "board-raspberrypi-pico-2,payload";
const DEBUG_DIR: &str = "src/kernel/target/sdk-parity-debug";
const DEBUG_MAP: &str = "src/kernel/payload-raspberrypi-pico-2.x";
/// The analog voltage switch the SDK sets per program: the four members of
/// the standard analog classes whose reference turns it on, and the quantity
/// type the standard API's ReadVoltage returns, whose symbol names witness
/// the library an image carries.
const VOLTAGE_MEMBERS: [&str; 4] = ["ReadVoltage", "MinVoltage", "MaxVoltage", "VoltageReference"];
const VOLTAGE_TYPE: &str = "ElectricPotential";
/// The board table's rule for AnalogVoltageFits: a row whose own payload
/// map's code region is at least this long holds the library the voltage
/// members compute in (internal-flash); a shorter region with an
/// external-flash map holds it there alone (external-flash); a shorter
/// region without one does not hold it (no attribute). The floor lies
/// between the longest code region measured to refuse the library at the
/// link and the shortest measured to hold it beside a program.
const VOLTAGE_LIBRARY_FLOOR: u64 = 8 << 20;
const RAM_INIT_MODES: [(&str, &str, &str); 3] = [
    ("boot", "", "init at boot"),
    ("first-use", "ram-init-first-use", "init at first use"),
    ("background", "ram-init-background", "init in background"),
];
/// The SDK's withheld table (its targets file): one row per value a
/// release withholds, each naming its property, the value and the words
/// the refusal prints, with the row's own condition as its test; the
/// gate reads the rows of the init mode's property.
const WITHHELD_ROW: &str = "<_RhuFeatureWithheld ";
const WITHHELD_RAM_INIT: &str = "RhuExternalRamInit";
const SDK_TARGETS: &str = "src/sdk/Lamella.Rhu.Sdk/Sdk/Sdk.targets";
const SDK_BOARDS: &str = "src/sdk/Lamella.Rhu.Sdk/Sdk/boards.props";

/// The init mode values the SDK's withheld table withholds, read from the
/// targets as shipped (the rows outside XML comments whose Include names
/// the init mode's property, each row's Value), so the gate follows the
/// one table the refusal reads.
fn ram_init_withheld(repo: &Path) -> Result<Vec<String>, String> {
    let path = repo.join(SDK_TARGETS.replace('/', std::path::MAIN_SEPARATOR_STR));
    let text = std::fs::read_to_string(&path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    let mut live = String::new();
    let mut rest = text.as_str();
    while let Some(start) = rest.find("<!--") {
        live.push_str(&rest[..start]);
        let Some(end) = rest[start..].find("-->") else {
            return Err(format!("{}: an XML comment never closes", path.display()));
        };
        rest = &rest[start + end + 3..];
    }
    live.push_str(rest);
    let mut withheld = Vec::new();
    let mut at = 0;
    while let Some(i) = live[at..].find(WITHHELD_ROW) {
        let start = at + i;
        let Some(len) = live[start..].find("/>") else {
            return Err(format!("{}: a withheld row never closes", path.display()));
        };
        let row = &live[start..start + len];
        let property = row_attr_text(row, "Include").ok_or_else(|| format!("{}: a withheld row without an Include", path.display()))?;
        if property == WITHHELD_RAM_INIT {
            let value = row_attr_text(row, "Value").ok_or_else(|| format!("{}: a withheld row without a Value", path.display()))?;
            withheld.push(value);
        }
        at = start + len;
    }
    Ok(withheld)
}

/// The board table's init-mode kernels against the boards' facts files: a
/// row carries both lazy archives exactly when its facts file lists an
/// external RAM range, at the paths the SDK defines under the row's folder; no other
/// row carries either. Answers every row's name with whether it is such a
/// row.
fn ram_init_table_check(repo: &Path) -> Result<Vec<(String, bool)>, String> {
    let path = repo.join(SDK_BOARDS.replace('/', std::path::MAIN_SEPARATOR_STR));
    let text = std::fs::read_to_string(&path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    let mut rows = Vec::new();
    let mut rest = text.as_str();
    while let Some(i) = rest.find("<LamellaTargetBoardRow ") {
        let Some(len) = rest[i..].find("/>") else {
            return Err("boards.props: a row never closes".to_string());
        };
        let row = &rest[i..i + len];
        let name = row_attr_text(row, "Include").ok_or_else(|| "boards.props: a row without an Include".to_string())?;
        let facts_path = repo.join(format!("src/sdk/Lamella.Rhu.Sdk/boards/{name}.json").replace('/', std::path::MAIN_SEPARATOR_STR));
        let facts = std::fs::read_to_string(&facts_path).map_err(|e| format!("cannot read {}: {e}", facts_path.display()))?;
        let compact: String = facts.chars().filter(|c| !c.is_whitespace()).collect();
        let external = compact.contains("\"external\":true");
        match (row_attr_text(row, "KernelRamInitFirstUse"), row_attr_text(row, "KernelRamInitBackground")) {
            (Some(first), Some(background)) => {
                if !external {
                    return Err(format!("the row {name} names the lazy init-mode kernels but its facts file lists no external RAM range"));
                }
                let want_first = format!("kernels/{name}/ram-init-first-use/librhu_kernel.a");
                let want_background = format!("kernels/{name}/ram-init-background/librhu_kernel.a");
                if first != want_first || background != want_background {
                    return Err(format!("the row {name}'s lazy kernel paths are not the defined ones: '{first}', '{background}'"));
                }
            }
            (None, None) => {
                if external {
                    return Err(format!("the row {name}'s facts file lists an external RAM range but the table names no lazy init-mode kernels for it"));
                }
            }
            _ => return Err(format!("the row {name} names one lazy init-mode kernel without the other")),
        }
        rows.push((name, external));
        rest = &rest[i + len..];
    }
    if rows.is_empty() {
        return Err("boards.props: no rows".to_string());
    }
    Ok(rows)
}

/// The board table's second payload maps against the boards' facts files
/// and the tree: a row names a second map (MapInternal) exactly when its
/// facts file lists an external RAM range, the name the row's own map with
/// "-internal" before its extension, both files in the tree under
/// src/kernel; no other row names one. Answers every row's name with its
/// own map and its second map's package path.
fn managed_memory_table_check(repo: &Path) -> Result<Vec<(String, String, Option<String>)>, String> {
    let path = repo.join(SDK_BOARDS.replace('/', std::path::MAIN_SEPARATOR_STR));
    let text = std::fs::read_to_string(&path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    let mut rows = Vec::new();
    let mut rest = text.as_str();
    while let Some(i) = rest.find("<LamellaTargetBoardRow ") {
        let Some(len) = rest[i..].find("/>") else {
            return Err("boards.props: a row never closes".to_string());
        };
        let row = &rest[i..i + len];
        let name = row_attr_text(row, "Include").ok_or_else(|| "boards.props: a row without an Include".to_string())?;
        let map = row_attr_text(row, "Map").ok_or_else(|| format!("boards.props: the row {name} names no Map"))?;
        let facts_path = repo.join(format!("src/sdk/Lamella.Rhu.Sdk/boards/{name}.json").replace('/', std::path::MAIN_SEPARATOR_STR));
        let facts = std::fs::read_to_string(&facts_path).map_err(|e| format!("cannot read {}: {e}", facts_path.display()))?;
        let compact: String = facts.chars().filter(|c| !c.is_whitespace()).collect();
        let external = compact.contains("\"external\":true");
        let second = row_attr_text(row, "MapInternal");
        match (&second, external) {
            (Some(s), true) => {
                let want = map.strip_suffix(".x").map(|m| format!("{m}-internal.x")).unwrap_or_default();
                if *s != want {
                    return Err(format!("the row {name}'s second map is '{s}', not the defined {want}"));
                }
                for m in [&map, s] {
                    let Some(file) = m.strip_prefix("maps/") else {
                        return Err(format!("the row {name}'s map '{m}' is not under maps/"));
                    };
                    let tree = repo.join("src").join("kernel").join(file);
                    if !tree.is_file() {
                        return Err(format!("the row {name}'s map {m} is not in the tree at {}", tree.display()));
                    }
                }
            }
            (Some(s), false) => return Err(format!("the row {name} names a second map ({s}) but its facts file lists no external RAM range")),
            (None, true) => return Err(format!("the row {name}'s facts file lists an external RAM range but the table names no second map for it")),
            (None, false) => {}
        }
        rows.push((name, map, second));
        rest = &rest[i + len..];
    }
    if rows.is_empty() {
        return Err("boards.props: no rows".to_string());
    }
    Ok(rows)
}

/// The four window symbols of a published image from its llvm-nm dump: the
/// rehydrated-data brackets and the arena's second window; a missing one
/// is named, never a silent zero.
fn window_symbols(dump: &[u8]) -> Result<(u64, u64, u64, u64), String> {
    let mut v = [0u64; 4];
    for (i, s) in ["__rhu_hydrated_lo", "__rhu_hydrated_hi", "__rhu_arena1_lo", "__rhu_arena1_hi"].iter().enumerate() {
        v[i] = word_sym_value(dump, s).ok_or_else(|| format!("the image does not define {s}"))?;
    }
    Ok((v[0], v[1], v[2], v[3]))
}

/// A row's two RAM ranges from its facts file, each as (base, bytes): the
/// chip's own (the first range not marked external) and the external one.
fn facts_ram_ranges(f: &FactsFile) -> Option<((u64, u64), (u64, u64))> {
    let own = f.ram.iter().find(|r| !r.3)?;
    let ext = f.ram.iter().find(|r| r.3)?;
    Some(((own.1, own.2), (ext.1, ext.2)))
}

/// The board table's external-flash maps against the boards' facts files
/// and the tree: a row names an external-flash map (MapExternalFlash)
/// exactly when its facts file lists an external flash range that is mapped
/// and executable, the name the row's own map with "-xip" before its
/// extension, both files in the tree under src/kernel; no other row names
/// one. Answers every row's name with its own map and its external-flash
/// map's package path.
fn program_memory_table_check(repo: &Path) -> Result<Vec<(String, String, Option<String>)>, String> {
    let path = repo.join(SDK_BOARDS.replace('/', std::path::MAIN_SEPARATOR_STR));
    let text = std::fs::read_to_string(&path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    let mut rows = Vec::new();
    let mut rest = text.as_str();
    while let Some(i) = rest.find("<LamellaTargetBoardRow ") {
        let Some(len) = rest[i..].find("/>") else {
            return Err("boards.props: a row never closes".to_string());
        };
        let row = &rest[i..i + len];
        let name = row_attr_text(row, "Include").ok_or_else(|| "boards.props: a row without an Include".to_string())?;
        let map = row_attr_text(row, "Map").ok_or_else(|| format!("boards.props: the row {name} names no Map"))?;
        let facts = read_facts_file(&repo.join(format!("src/sdk/Lamella.Rhu.Sdk/boards/{name}.json").replace('/', std::path::MAIN_SEPARATOR_STR)))?;
        let window = facts.flash.iter().any(|f| f.3 && f.4 && f.5);
        let third = row_attr_text(row, "MapExternalFlash");
        match (&third, window) {
            (Some(s), true) => {
                let want = map.strip_suffix(".x").map(|m| format!("{m}-xip.x")).unwrap_or_default();
                if *s != want {
                    return Err(format!("the row {name}'s external-flash map is '{s}', not the defined {want}"));
                }
                for m in [&map, s] {
                    let Some(file) = m.strip_prefix("maps/") else {
                        return Err(format!("the row {name}'s map '{m}' is not under maps/"));
                    };
                    let tree = repo.join("src").join("kernel").join(file);
                    if !tree.is_file() {
                        return Err(format!("the row {name}'s map {m} is not in the tree at {}", tree.display()));
                    }
                }
            }
            (Some(s), false) => return Err(format!("the row {name} names an external-flash map ({s}) but its facts file lists no mapped, executable external flash range")),
            (None, true) => return Err(format!("the row {name}'s facts file lists a mapped, executable external flash range but the table names no external-flash map for it")),
            (None, false) => {}
        }
        rows.push((name, map, third));
        rest = &rest[i + len..];
    }
    if rows.is_empty() {
        return Err("boards.props: no rows".to_string());
    }
    Ok(rows)
}

/// The board table's AnalogVoltageFits against each row's maps: the rule of
/// VOLTAGE_LIBRARY_FLOOR over the code region of the row's own payload map
/// (internal-flash at or above the floor; external-flash below it with an
/// external-flash map; no attribute below it without one), the attribute
/// exactly the rule's word. Answers every row's name with its attribute.
fn analog_voltage_table_check(repo: &Path) -> Result<Vec<(String, Option<String>)>, String> {
    let path = repo.join(SDK_BOARDS.replace('/', std::path::MAIN_SEPARATOR_STR));
    let text = std::fs::read_to_string(&path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    let mut rows = Vec::new();
    let mut rest = text.as_str();
    while let Some(i) = rest.find("<LamellaTargetBoardRow ") {
        let Some(len) = rest[i..].find("/>") else {
            return Err("boards.props: a row never closes".to_string());
        };
        let row = &rest[i..i + len];
        let name = row_attr_text(row, "Include").ok_or_else(|| "boards.props: a row without an Include".to_string())?;
        let map = row_attr_text(row, "Map").ok_or_else(|| format!("boards.props: the row {name} names no Map"))?;
        let Some(file) = map.strip_prefix("maps/") else {
            return Err(format!("the row {name}'s map '{map}' is not under maps/"));
        };
        let map_path = repo.join("src").join("kernel").join(file);
        let map_text = std::fs::read_to_string(&map_path).map_err(|e| format!("cannot read {}: {e}", map_path.display()))?;
        let Some((_, code_len)) = crate::gates::payload::map_region(&map_text, "CODE") else {
            return Err(format!("the row {name}'s map {map} states no CODE region"));
        };
        let external_map = row_attr_text(row, "MapExternalFlash").is_some();
        let want = if code_len >= VOLTAGE_LIBRARY_FLOOR {
            Some("internal-flash".to_string())
        } else if external_map {
            Some("external-flash".to_string())
        } else {
            None
        };
        let got = row_attr_text(row, "AnalogVoltageFits");
        if got != want {
            return Err(format!(
                "the row {name}'s AnalogVoltageFits is {}, but its code region of {code_len} B{} calls for {}",
                got.as_deref().unwrap_or("absent"),
                if external_map { " with an external-flash map" } else { "" },
                want.as_deref().unwrap_or("no attribute")
            ));
        }
        rows.push((name, got));
        rest = &rest[i + len..];
    }
    if rows.is_empty() {
        return Err("boards.props: no rows".to_string());
    }
    Ok(rows)
}

/// A row's chip flash range and its mapped, executable external flash range
/// from its facts file, each as (base, bytes).
fn facts_flash_ranges(f: &FactsFile) -> Option<((u64, u64), (u64, u64))> {
    let chip = f.flash.iter().find(|r| !r.3)?;
    let window = f.flash.iter().find(|r| r.3 && r.4 && r.5)?;
    Some(((chip.1, chip.2), (window.1, window.2)))
}

/// The execute-in-place image's brackets from its llvm-nm dump: the window,
/// the managed code, the unboxing stubs' end, the window's index and the
/// native code; a missing one is named, never a silent zero.
struct XipSyms {
    lo: u64,
    hi: u64,
    code_lo: u64,
    code_hi: u64,
    unbox_hi: u64,
    exidx_start: u64,
    exidx_end: u64,
    stext: u64,
    etext: u64,
}

fn xip_symbols(dump: &[u8]) -> Result<XipSyms, String> {
    let mut v = [0u64; 9];
    for (i, s) in ["__rhu_xip_lo", "__rhu_xip_hi", "__rhu_managedcode_lo", "__rhu_managedcode_hi", "__rhu_unbox_hi", "__rhu_xip_exidx_start", "__rhu_xip_exidx_end", "__stext", "__etext"].iter().enumerate() {
        v[i] = word_sym_value(dump, s).ok_or_else(|| format!("the image does not define {s}"))?;
    }
    Ok(XipSyms { lo: v[0], hi: v[1], code_lo: v[2], code_hi: v[3], unbox_hi: v[4], exidx_start: v[5], exidx_end: v[6], stext: v[7], etext: v[8] })
}

/// The arena floor a payload map states (RHU_ARENA_MIN = <n>[K|M];), in
/// bytes, read from the map in the tree; a map without the line is named,
/// never a silent zero.
fn arena_floor(repo: &Path, map_rel: &str) -> Result<u64, String> {
    let text = std::fs::read_to_string(repo.join(map_rel.replace('/', std::path::MAIN_SEPARATOR_STR))).map_err(|e| format!("cannot read {map_rel}: {e}"))?;
    for l in text.lines() {
        if let Some(rest) = l.trim().strip_prefix("RHU_ARENA_MIN = ") {
            let v = rest.trim_end_matches(';').trim();
            let (digits, scale) = match v.strip_suffix('M') {
                Some(d) => (d, 1u64 << 20),
                None => match v.strip_suffix('K') {
                    Some(d) => (d, 1u64 << 10),
                    None => (v, 1u64),
                },
            };
            return digits.parse::<u64>().map(|n| n * scale).map_err(|_| format!("{map_rel}: RHU_ARENA_MIN '{v}' is not a number"));
        }
    }
    Err(format!("{map_rel} states no RHU_ARENA_MIN line"))
}

/// One section's type, address and size from a section table in GNU readelf's
/// layout (llvm-readobj --elf-output-style=GNU -S -W), by its exact name; None
/// when the table has no such section.
fn section_row(dump: &str, name: &str) -> Option<(String, u64, u64)> {
    for l in dump.lines() {
        let Some(j) = l.find(']') else { continue };
        let mut it = l[j + 1..].split_whitespace();
        if it.next() != Some(name) {
            continue;
        }
        let kind = it.next()?.to_string();
        let addr = u64::from_str_radix(it.next()?, 16).ok()?;
        let _offset = it.next()?;
        let size = u64::from_str_radix(it.next()?, 16).ok()?;
        return Some((kind, addr, size));
    }
    None
}

/// The switch's publish summary line, its publish run as registered (rc 0,
/// the image present, the row's facts generated once).
fn voltage_publish_line<'a>(rt: &mut Rt, case: &str, controls: &'a str, label: &str) -> &'a str {
    let Some(line) = control_line(controls, label) else {
        rt.fail(case, &format!("the publish-side script left no summary line for {label}"));
    };
    if control_field(line, "rc") != "0" || control_field(line, "elf") != "present" || control_field(line, "generated") != "1" {
        rt.fail(case, &format!("the publish did not run as registered (rc 0, the image present, the facts generated once): {line}"));
    }
    line
}

/// The switch's words after "analog voltage" in a publish summary line.
fn voltage_words(line: &str) -> &str {
    line.split_once(" words=").map(|(_, w)| w.trim()).unwrap_or("")
}

/// How many of an image's symbol names carry the quantity type's name.
fn voltage_type_names(rt: &mut Rt, case: &str, nm_tool: &Path, elf: &Path, out_name: &str) -> usize {
    let arg = elf.to_string_lossy().into_owned();
    let dump = rt.tool_text(nm_tool, case, &[&arg], out_name);
    dump.lines().filter(|l| l.contains(VOLTAGE_TYPE)).count()
}

/// One boot of a published image on the leg's emulator machine for the
/// managed memory's home: the raw capture banked under the gate's output
/// directory under the given name; the boot must end at the kernel's
/// return line with the program's stream equal to the golden, the hosting
/// layer's posture line as wanted and not the other, nothing forbidden,
/// exit 0. Answers the emulator's version for the verdict line.
#[allow(clippy::too_many_arguments)]
fn boot_for_heap_window(rt: &mut Rt, case: &str, repo: &Path, machine: &str, tier: &str, out: &Path, elf: &Path, raw_name: &str, want: &str, not: &str) -> String {
    let (qemu, qemu_version) = match resolve_qemu(repo) {
        Ok(q) => q,
        Err(e) => rt.fail(case, &e),
    };
    let raw_path = out.join(format!("{raw_name}.raw"));
    let stderr_path = out.join(format!("{raw_name}.stderr"));
    let elf_arg = elf.to_string_lossy().into_owned();
    let mut qargs: Vec<&str> = vec!["-M", machine, "-display", "none", "-monitor", "none", "-serial", "stdio", "-semihosting"];
    qargs.extend_from_slice(ExecProfile::Parity.qemu_args());
    qargs.push("-kernel");
    qargs.push(&elf_arg);
    let outcome = match run_captured(&qemu, &qargs, &raw_path, &stderr_path, Duration::from_secs(RUN_TIMEOUT_S)) {
        Ok(o) => o,
        Err(e) => rt.fail(case, &e),
    };
    let raw = std::fs::read(&raw_path).unwrap_or_default();
    echo_file(&raw_path, "  | ");
    if outcome.timed_out {
        rt.fail(case, "the boot hung (killed at its timeout)");
    }
    if cr_count(&raw) == 0 {
        rt.fail(case, "no CR bytes in raw capture (wire discipline lost)");
    }
    let golden = match std::fs::read(rt.frozen(GOLDEN)) {
        Ok(b) => b,
        Err(e) => rt.fail(case, &format!("golden unreadable: {e}")),
    };
    if !eq_lf_normalized(&payload_stream(&raw), &golden) {
        rt.fail(case, &format!("the payload stream differs from {tier}/{GOLDEN}"));
    }
    if !contains_fixed(&raw, BOOT_RETURN_LINE) {
        rt.fail(case, &format!("the kernel's return line '{BOOT_RETURN_LINE}' not found"));
    }
    for m in FORBIDDEN_PATTERNS {
        if contains_fixed(&raw, m) {
            rt.fail(case, &format!("forbidden pattern '{m}' present"));
        }
    }
    if !contains_fixed(&raw, want) {
        rt.fail(case, &format!("the hosting layer's posture line '{want}' not found"));
    }
    if contains_fixed(&raw, not) {
        rt.fail(case, &format!("the hosting layer's posture line reads '{not}'"));
    }
    if outcome.exit != 0 {
        rt.fail(case, &format!("exit {}, wanted 0", outcome.exit));
    }
    qemu_version
}

/// The board table's channel facts against the boards' facts files: every
/// row's two default channels and its legal set are copies of its facts
/// file's `channels` row (the two defaults equal, the set equal in order,
/// both defaults inside the set); a row without the three attributes, or a
/// facts file without the member, is a fault. Answers every row's name
/// with its two defaults and the size of its set.
fn channel_facts_check(repo: &Path) -> Result<Vec<(String, String, String, usize)>, String> {
    let path = repo.join(SDK_BOARDS.replace('/', std::path::MAIN_SEPARATOR_STR));
    let text = std::fs::read_to_string(&path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    let mut rows = Vec::new();
    let mut rest = text.as_str();
    while let Some(i) = rest.find("<LamellaTargetBoardRow ") {
        let Some(len) = rest[i..].find("/>") else {
            return Err("boards.props: a row never closes".to_string());
        };
        let row = &rest[i..i + len];
        let name = row_attr_text(row, "Include").ok_or_else(|| "boards.props: a row without an Include".to_string())?;
        let log = row_attr_text(row, "LogChannel").ok_or_else(|| format!("the row {name} names no LogChannel"))?;
        let console = row_attr_text(row, "ConsoleChannel").ok_or_else(|| format!("the row {name} names no ConsoleChannel"))?;
        let legal = row_attr_text(row, "Channels").ok_or_else(|| format!("the row {name} names no Channels"))?;
        let facts_path = repo.join(format!("src/sdk/Lamella.Rhu.Sdk/boards/{name}.json").replace('/', std::path::MAIN_SEPARATOR_STR));
        let facts = std::fs::read_to_string(&facts_path).map_err(|e| format!("cannot read {}: {e}", facts_path.display()))?;
        let compact: String = facts.chars().filter(|c| !c.is_whitespace()).collect();
        let Some(at) = compact.find("\"channels\":{") else {
            return Err(format!("{}: no channels member", facts_path.display()));
        };
        let body = &compact[at + "\"channels\":{".len()..];
        let Some(end) = body.find('}') else {
            return Err(format!("{}: the channels member never closes", facts_path.display()));
        };
        let body = &body[..end];
        let field = |key: &str| -> Option<String> {
            let k = format!("\"{key}\":\"");
            let s = body.find(&k)? + k.len();
            let e = body[s..].find('"')? + s;
            Some(body[s..e].to_string())
        };
        let f_log = field("log").ok_or_else(|| format!("{}: channels.log missing", facts_path.display()))?;
        let f_console = field("console").ok_or_else(|| format!("{}: channels.console missing", facts_path.display()))?;
        let f_legal = {
            let k = "\"legal\":[";
            let s = body.find(k).ok_or_else(|| format!("{}: channels.legal missing", facts_path.display()))? + k.len();
            let e = body[s..].find(']').ok_or_else(|| format!("{}: channels.legal never closes", facts_path.display()))? + s;
            body[s..e].split(',').map(|w| w.trim_matches('"').to_string()).collect::<Vec<_>>().join(" ")
        };
        if log != f_log {
            return Err(format!("{name}: LogChannel '{log}' != the facts file's channels.log '{f_log}'"));
        }
        if console != f_console {
            return Err(format!("{name}: ConsoleChannel '{console}' != the facts file's channels.console '{f_console}'"));
        }
        if legal != f_legal {
            return Err(format!("{name}: Channels '{legal}' != the facts file's channels.legal '{f_legal}'"));
        }
        if !legal.split(' ').any(|w| w == log) || !legal.split(' ').any(|w| w == console) {
            return Err(format!("{name}: a default channel ({log}, {console}) is outside the row's legal set '{legal}'"));
        }
        let n = legal.split(' ').count();
        rows.push((name, log, console, n));
        rest = &rest[i + len..];
    }
    if rows.is_empty() {
        return Err("boards.props: no rows".to_string());
    }
    Ok(rows)
}

/// The boot markers with the third thread's spawn line at the knob's value
/// and the handshake's values from the header.
fn knob_boot_markers(value: u64, abi: u32, compat: u32) -> Vec<String> {
    BOOT_MARKERS
        .iter()
        .map(|m| if *m == STACK_KNOB_MARKER_DEFAULT { format!("[shim] pthread_create: stack {value} B asked, {value} B spawned (tid 4)") } else { m.replace("{abi}", &abi.to_string()).replace("{compat}", &compat.to_string()) })
        .collect()
}

/// The text of `<name>="..."` in one row of the board table.
fn row_attr_text(row: &str, name: &str) -> Option<String> {
    let key = format!("{name}=\"");
    let start = row.find(&key)? + key.len();
    let end = row[start..].find('"')? + start;
    Some(row[start..end].to_string())
}

fn row_attr(row: &str, name: &str) -> Option<u64> {
    row_attr_text(row, name)?.parse().ok()
}

/// `pub const STACK_POOL_BYTES: usize = <n> * 1024;` (or a plain number) in
/// a kernel board file.
fn kernel_pool_bytes(text: &str) -> Option<u64> {
    let key = "pub const STACK_POOL_BYTES: usize = ";
    let start = text.find(key)? + key.len();
    let end = text[start..].find(';')? + start;
    let expr: String = text[start..end].chars().filter(|c| !c.is_whitespace()).collect();
    let mut product = 1u64;
    for factor in expr.split('*') {
        product = product.checked_mul(factor.parse::<u64>().ok()?)?;
    }
    Some(product)
}

/// A product of decimal factors (`16 * 1024`, or a plain number).
fn decimal_product(expr: &str) -> Option<u64> {
    let expr: String = expr.chars().filter(|c| !c.is_whitespace()).collect();
    let mut product = 1u64;
    for factor in expr.split('*') {
        product = product.checked_mul(factor.parse::<u64>().ok()?)?;
    }
    Some(product)
}

/// `pub const <name>: usize = <product>;` in a kernel source's text.
fn kernel_const(text: &str, name: &str) -> Option<u64> {
    let key = format!("pub const {name}: usize = ");
    let start = text.find(&key)? + key.len();
    let end = text[start..].find(';')? + start;
    decimal_product(&text[start..end])
}

/// The stack the kernel gives its idle thread: the last word of the idle
/// thread's spawn in the scheduler's text.
fn kernel_idle_stack(sched: &str) -> Option<u64> {
    let key = "spawn(\"idle\", idle_entry, 0, 0xFF, ";
    let start = sched.find(key)? + key.len();
    let end = sched[start..].find(')')? + start;
    decimal_product(&sched[start..end])
}

/// Whether the scheduler's text carries the kernel's service threads'
/// stacks above a row's pool: the pool's array is declared as the row's
/// pool plus the service stacks, so those threads take none of the bytes
/// a program's threads are given.
fn kernel_service_stacks_above_pool(sched: &str) -> bool {
    sched.lines().any(|l| l.trim() == "struct StackPool([u8; board::STACK_POOL_BYTES + SERVICE_STACK_BYTES]);")
}

/// `#define <name> <digits>` (a `u` suffix allowed) in a C header's text.
fn c_define(text: &str, name: &str) -> Option<u64> {
    for line in text.lines() {
        let mut words = line.split_whitespace();
        if words.next() == Some("#define") && words.next() == Some(name) {
            return words.next()?.trim_end_matches('u').parse().ok();
        }
    }
    None
}

/// The hosting layer's row block whose MGC_ROW_BOARD names the board: the
/// text from its `#if`/`#elif` line to the next preprocessor branch.
fn host_row_block<'a>(text: &'a str, board: &str) -> Option<&'a str> {
    let quoted = format!("\"{board}\"");
    let mut bounds: Vec<usize> = Vec::new();
    for key in ["#if defined(", "#elif defined(", "#else", "#endif"] {
        bounds.extend(text.match_indices(key).map(|(i, _)| i));
    }
    bounds.push(text.len());
    bounds.sort_unstable();
    bounds.dedup();
    bounds.windows(2).map(|p| &text[p[0]..p[1]]).find(|block| block.lines().any(|l| l.contains("MGC_ROW_BOARD") && l.contains(&quoted)))
}

/// `<name>value</name>` in the targets' text.
fn xml_value(text: &str, name: &str) -> Option<String> {
    let open = format!("<{name}>");
    let close = format!("</{name}>");
    let start = text.find(&open)? + open.len();
    let end = text[start..].find(&close)? + start;
    Some(text[start..end].trim().to_string())
}

/// A JSON value of the flat shape the facts files use: objects, arrays,
/// strings, whole numbers and null. The reader is the harness's own (the
/// harness carries no dependency); anything outside that shape -- a fraction,
/// an exponent, a boolean, a negative number, a \u escape, a trailing comma,
/// a member named twice, text after the value -- is refused with a sentence
/// naming the file and the byte.
#[derive(Clone, Debug, PartialEq)]
enum Json {
    Object(Vec<(String, Json)>),
    Array(Vec<Json>),
    Str(String),
    Int(u64),
    Bool(bool),
    Null,
}

struct JsonReader<'a> {
    b: &'a [u8],
    i: usize,
    where_: &'a str,
}

impl<'a> JsonReader<'a> {
    fn err(&self, what: &str) -> String {
        format!("{}: {what} at byte {}", self.where_, self.i)
    }
    fn skip_ws(&mut self) {
        while self.i < self.b.len() && matches!(self.b[self.i], b' ' | b'\t' | b'\n' | b'\r') {
            self.i += 1;
        }
    }
    fn peek(&self) -> Option<u8> {
        self.b.get(self.i).copied()
    }
    fn expect(&mut self, c: u8) -> Result<(), String> {
        if self.peek() == Some(c) {
            self.i += 1;
            Ok(())
        } else {
            Err(self.err(&format!("'{}' expected", c as char)))
        }
    }
    fn value(&mut self) -> Result<Json, String> {
        self.skip_ws();
        match self.peek() {
            Some(b'{') => self.object(),
            Some(b'[') => self.array(),
            Some(b'"') => Ok(Json::Str(self.string()?)),
            Some(b'n') if self.b[self.i..].starts_with(b"null") => {
                self.i += 4;
                Ok(Json::Null)
            }
            Some(c) if c.is_ascii_digit() => self.integer(),
            Some(b'-') => Err(self.err("a negative number is not in the facts' shape")),
            Some(b't') if self.b[self.i..].starts_with(b"true") => {
                self.i += 4;
                Ok(Json::Bool(true))
            }
            Some(b'f') if self.b[self.i..].starts_with(b"false") => {
                self.i += 5;
                Ok(Json::Bool(false))
            }
            Some(b't') | Some(b'f') => Err(self.err("'true' or 'false' expected")),
            Some(_) => Err(self.err("a value of the facts' shape expected (an object, an array, a string, a whole number, a boolean or null)")),
            None => Err(self.err("the text ends before a value")),
        }
    }
    fn object(&mut self) -> Result<Json, String> {
        self.expect(b'{')?;
        let mut members: Vec<(String, Json)> = Vec::new();
        self.skip_ws();
        if self.peek() == Some(b'}') {
            self.i += 1;
            return Ok(Json::Object(members));
        }
        loop {
            self.skip_ws();
            if self.peek() != Some(b'"') {
                return Err(self.err("a member's name (a string) expected"));
            }
            let key = self.string()?;
            if members.iter().any(|(k, _)| *k == key) {
                return Err(self.err(&format!("the member '{key}' appears twice")));
            }
            self.skip_ws();
            self.expect(b':')?;
            let v = self.value()?;
            members.push((key, v));
            self.skip_ws();
            match self.peek() {
                Some(b',') => {
                    self.i += 1;
                    self.skip_ws();
                    if self.peek() == Some(b'}') {
                        return Err(self.err("a trailing comma"));
                    }
                }
                Some(b'}') => {
                    self.i += 1;
                    return Ok(Json::Object(members));
                }
                _ => return Err(self.err("',' or '}' expected")),
            }
        }
    }
    fn array(&mut self) -> Result<Json, String> {
        self.expect(b'[')?;
        let mut items = Vec::new();
        self.skip_ws();
        if self.peek() == Some(b']') {
            self.i += 1;
            return Ok(Json::Array(items));
        }
        loop {
            items.push(self.value()?);
            self.skip_ws();
            match self.peek() {
                Some(b',') => {
                    self.i += 1;
                    self.skip_ws();
                    if self.peek() == Some(b']') {
                        return Err(self.err("a trailing comma"));
                    }
                }
                Some(b']') => {
                    self.i += 1;
                    return Ok(Json::Array(items));
                }
                _ => return Err(self.err("',' or ']' expected")),
            }
        }
    }
    fn string(&mut self) -> Result<String, String> {
        self.expect(b'"')?;
        let mut out = Vec::new();
        loop {
            let c = match self.peek() {
                Some(c) => c,
                None => return Err(self.err("the text ends inside a string")),
            };
            self.i += 1;
            match c {
                b'"' => break,
                b'\\' => {
                    let e = match self.peek() {
                        Some(e) => e,
                        None => return Err(self.err("the text ends inside an escape")),
                    };
                    self.i += 1;
                    out.push(match e {
                        b'"' => b'"',
                        b'\\' => b'\\',
                        b'/' => b'/',
                        b'b' => 8,
                        b'f' => 12,
                        b'n' => b'\n',
                        b'r' => b'\r',
                        b't' => b'\t',
                        b'u' => return Err(self.err("a \\u escape is not in the facts' shape (the files are ASCII)")),
                        _ => return Err(self.err("an unknown escape")),
                    });
                }
                c if c < 0x20 => return Err(self.err("a control byte inside a string")),
                c => out.push(c),
            }
        }
        String::from_utf8(out).map_err(|_| self.err("a string that is not UTF-8"))
    }
    fn integer(&mut self) -> Result<Json, String> {
        let start = self.i;
        while matches!(self.peek(), Some(c) if c.is_ascii_digit()) {
            self.i += 1;
        }
        if matches!(self.peek(), Some(b'.') | Some(b'e') | Some(b'E')) {
            return Err(self.err("a fraction or an exponent is not in the facts' shape"));
        }
        let text = std::str::from_utf8(&self.b[start..self.i]).unwrap_or("");
        if text.len() > 1 && text.starts_with('0') {
            return Err(self.err("a number with a leading zero"));
        }
        text.parse::<u64>().map(Json::Int).map_err(|_| self.err("a whole number too large"))
    }
}

/// The whole text as one value, nothing but whitespace after it.
fn json_parse(text: &[u8], where_: &str) -> Result<Json, String> {
    let mut r = JsonReader { b: text, i: 0, where_ };
    let v = r.value()?;
    r.skip_ws();
    if r.i != r.b.len() {
        return Err(r.err("text after the value"));
    }
    Ok(v)
}

impl Json {
    fn member<'a>(&'a self, key: &str, where_: &str) -> Result<&'a Json, String> {
        match self {
            Json::Object(m) => m.iter().find(|(k, _)| k == key).map(|(_, v)| v).ok_or(format!("{where_}: no '{key}' member")),
            _ => Err(format!("{where_}: not an object")),
        }
    }
    fn str(&self, key: &str, where_: &str) -> Result<String, String> {
        match self.member(key, where_)? {
            Json::Str(s) => Ok(s.clone()),
            _ => Err(format!("{where_}: '{key}' is not a string")),
        }
    }
    fn int(&self, key: &str, where_: &str) -> Result<u64, String> {
        match self.member(key, where_)? {
            Json::Int(n) => Ok(*n),
            _ => Err(format!("{where_}: '{key}' is not a whole number")),
        }
    }
    fn bool(&self, key: &str, where_: &str) -> Result<bool, String> {
        match self.member(key, where_)? {
            Json::Bool(b) => Ok(*b),
            _ => Err(format!("{where_}: '{key}' is not a boolean")),
        }
    }
    fn ints(&self, key: &str, where_: &str) -> Result<Vec<u64>, String> {
        match self.member(key, where_)? {
            Json::Array(a) => a
                .iter()
                .map(|v| match v {
                    Json::Int(n) => Ok(*n),
                    _ => Err(format!("{where_}: '{key}' holds something that is not a whole number")),
                })
                .collect(),
            _ => Err(format!("{where_}: '{key}' is not an array")),
        }
    }
    fn strs(&self, key: &str, where_: &str) -> Result<Vec<String>, String> {
        match self.member(key, where_)? {
            Json::Array(a) => a
                .iter()
                .map(|v| match v {
                    Json::Str(s) => Ok(s.clone()),
                    _ => Err(format!("{where_}: '{key}' holds something that is not a string")),
                })
                .collect(),
            _ => Err(format!("{where_}: '{key}' is not an array")),
        }
    }
    fn objects<'a>(&'a self, key: &str, where_: &str) -> Result<Vec<&'a Json>, String> {
        match self.member(key, where_)? {
            Json::Array(a) => a
                .iter()
                .map(|v| if matches!(v, Json::Object(_)) { Ok(v) } else { Err(format!("{where_}: '{key}' holds something that is not an object")) })
                .collect(),
            _ => Err(format!("{where_}: '{key}' is not an array")),
        }
    }
}

/// One boards/<row>.json file of the SDK package (the board class's
/// source of facts): the pin table's rules, the buses, the clock and the
/// posture -- the members the tie and the board class's markers read.
/// One analog converter as the facts file states it: its number, its width in bits, its reference in millivolts
/// and its channels (the chip's channel number and the pin, None for an internal source).
type AdcFacts = (u64, u64, u64, Vec<(u64, Option<u64>)>);
/// The network pool as a facts file states it for a row whose kernel serves a network: the medium's word, the
/// stream and the datagram sockets, a stream's receive and transmit buffers and a datagram's buffer in bytes,
/// and the resolver's slots.
type NetFacts = (String, u64, u64, u64, u64, u64, u64);

#[derive(Clone, Default)]
struct FactsFile {
    name: String,
    maker: String,
    product: String,
    chip: String,
    clock_hz: u64,
    posture: String,
    pins: u64,
    labels: String,
    i2c: Vec<(u64, u64, u64, Vec<u64>)>,
    spi: Vec<(u64, u64, u64, u64, Vec<u64>, u64, u64)>,
    /// The one PWM chip's number and the lines a timer drives (the channel number is the line number).
    pwm_chip: u64,
    pwm_lines: Vec<u64>,
    /// The board's RAM ranges -- the name, the base, the size, external, cached.
    ram: Vec<(String, u64, u64, bool, bool)>,
    /// The board's flash ranges where the row states them -- the name, the base, the size, external, mapped, executable (a row without the array carries none).
    flash: Vec<(String, u64, u64, bool, bool, bool)>,
    /// The board's UART rows -- the block and its pins (absent on a row whose UARTs have no pins).
    uart: Vec<(u64, Option<u64>, Option<u64>)>,
    /// The blocks whose UART row states the kind lowpower, in the rows' order.
    uart_lowpower: Vec<u64>,
    /// The two streams' default channels and the row's legal set.
    channels: (String, String, Vec<String>),
    /// The chip's legal pin sets per block for a project's pin override.
    uart_pairs: Vec<(u64, Vec<u64>, Vec<u64>)>,
    /// The analog converters: a converter's number is its index in the kernel's block table.
    adc: Vec<AdcFacts>,
    /// The network pool, on a row whose file carries the net object (a row whose kernel serves no network carries none).
    net: Option<NetFacts>,
}

fn read_facts_file(path: &Path) -> Result<FactsFile, String> {
    let text = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let where_ = path.display().to_string();
    let j = json_parse(&text, &where_)?;
    let w = where_.as_str();
    let mut f = FactsFile {
        name: j.str("name", w)?,
        maker: j.str("maker", w)?,
        product: j.str("product", w)?,
        chip: j.str("chip", w)?,
        clock_hz: j.int("clockHz", w)?,
        posture: j.str("posture", w)?,
        pins: j.int("pins", w)?,
        labels: j.str("labels", w)?,
        ..FactsFile::default()
    };
    for b in j.objects("i2c", w)? {
        f.i2c.push((b.int("block", w)?, b.int("sda", w)?, b.int("scl", w)?, b.ints("rates", w)?));
    }
    for b in j.objects("spi", w)? {
        f.spi.push((b.int("block", w)?, b.int("sck", w)?, b.int("mosi", w)?, b.int("miso", w)?, b.ints("cs", w)?, b.int("min", w)?, b.int("max", w)?));
    }
    let pwm = j.member("pwm", w)?;
    f.pwm_chip = pwm.int("chip", w)?;
    f.pwm_lines = pwm.ints("lines", w)?;
    for c in j.member("adc", w)?.objects("converters", w)? {
        let mut channels = Vec::new();
        for ch in c.objects("channels", w)? {
            let pin = match ch.member("pin", w)? {
                Json::Int(n) => Some(*n),
                Json::Null => None,
                _ => return Err(format!("{where_}: a channel's 'pin' is neither a whole number nor null")),
            };
            channels.push((ch.int("number", w)?, pin));
        }
        f.adc.push((c.int("number", w)?, c.int("bits", w)?, c.int("referenceMillivolts", w)?, channels));
    }
    for b in j.objects("ram", w)? {
        f.ram.push((b.str("name", w)?, b.int("base", w)?, b.int("bytes", w)?, b.bool("external", w)?, b.bool("cached", w)?));
    }
    if j.member("flash", w).is_ok() {
        for b in j.objects("flash", w)? {
            f.flash.push((b.str("name", w)?, b.int("base", w)?, b.int("bytes", w)?, b.bool("external", w)?, b.bool("mapped", w)?, b.bool("executable", w)?));
        }
    }
    for b in j.objects("uart", w)? {
        let tx = if b.member("tx", w).is_ok() { Some(b.int("tx", w)?) } else { None };
        let rx = if b.member("rx", w).is_ok() { Some(b.int("rx", w)?) } else { None };
        let block = b.int("block", w)?;
        if b.member("kind", w).is_ok() {
            let kind = b.str("kind", w)?;
            if kind != "lowpower" {
                return Err(format!("{w}: the uart row of block {block} states the kind '{kind}', not the word lowpower"));
            }
            f.uart_lowpower.push(block);
        }
        f.uart.push((block, tx, rx));
    }
    let ch = j.member("channels", w)?;
    f.channels = (ch.str("log", w)?, ch.str("console", w)?, ch.strs("legal", w)?);
    for b in j.objects("uartPairs", w)? {
        f.uart_pairs.push((b.int("block", w)?, b.ints("tx", w)?, b.ints("rx", w)?));
    }
    if j.member("net", w).is_ok() {
        let n = j.member("net", w)?;
        f.net = Some((n.str("medium", w)?, n.int("streams", w)?, n.int("datagrams", w)?, n.int("streamRx", w)?, n.int("streamTx", w)?, n.int("datagramBuf", w)?, n.int("resolveSlots", w)?));
    }
    if f.name.is_empty() || f.pins == 0 || f.clock_hz == 0 || f.posture.is_empty() {
        return Err(format!("{where_}: an empty name, pins, clockHz or posture"));
    }
    if f.ram.is_empty() || f.ram[0].3 || f.ram[0].4 {
        return Err(format!("{where_}: the ram array is empty or its first range is not the internal SRAM"));
    }
    Ok(f)
}

/// `pub const NAME: <type> = <value>;` in a kernel source: the value's text.
fn rust_const(text: &str, name: &str) -> Option<String> {
    let key = format!("pub const {name}: ");
    let start = text.find(&key)? + key.len();
    let eq = text[start..].find('=')? + start + 1;
    let end = text[eq..].find(';')? + eq;
    Some(text[eq..end].trim().to_string())
}

fn rust_number(s: &str) -> Option<u64> {
    s.trim().replace('_', "").parse().ok()
}

/// SYSCLK_HZ under a posture: the definition whose `#[cfg(...)]` line (the
/// nearest above it) says `not(` for the default posture, or names the
/// posture; a module with one definition and no cfg serves every posture.
fn kernel_sysclk(text: &str, posture: &str) -> Option<u64> {
    let key = "pub const SYSCLK_HZ: u32 = ";
    let mut best = None;
    for (i, _) in text.match_indices(key) {
        let rest = &text[i + key.len()..];
        let value = rust_number(&rest[..rest.find(';')?])?;
        let cfg = text[..i].lines().rev().take(4).find(|l| l.trim().starts_with("#[cfg(")).map(|l| l.to_string());
        let fits = match &cfg {
            None => true,
            Some(c) if posture == "default" => c.contains("not("),
            Some(c) => !c.contains("not(") && c.contains(posture),
        };
        if fits {
            best = Some(value);
        }
    }
    best
}

/// The `line: <n>` numbers of a pad row, or the numbers of a select list.
fn pad_lines(text: &str, name: &str) -> Option<Vec<u64>> {
    let key = format!("pub const {name}: ");
    let start = text.find(&key)?;
    let end = text[start..].find("];")? + start;
    let block = &text[start..end];
    let mut out = Vec::new();
    if block.contains("line:") {
        for (i, _) in block.match_indices("line:") {
            let rest = block[i + 5..].trim_start();
            let n: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
            out.push(n.parse().ok()?);
        }
    } else {
        let open = block.rfind('[')? + 1;
        if block[open..].trim().is_empty() {
            return Some(out);
        }
        for n in block[open..].split(',') {
            out.push(n.trim().parse().ok()?);
        }
    }
    Some(out)
}

/// A `0x` or decimal literal with underscores, as the kernel's facts write it.
fn rust_literal(s: &str) -> Option<u64> {
    let t = s.trim().replace('_', "");
    match t.strip_prefix("0x") {
        Some(h) => u64::from_str_radix(h, 16).ok(),
        None => t.parse().ok(),
    }
}

/// The `RAM_RANGES` rows of a kernel facts module for one board feature:
/// the definition whose `#[cfg(...)]` line (the nearest above it,
/// within four lines) names the feature, or carries no cfg at all; each
/// `RamFacts { name: "..", base: .., bytes: .., external: .., cached: .. }`
/// row read by its fields.
fn kernel_ram_ranges(text: &str, feature: &str) -> Option<Vec<(String, u64, u64, bool, bool)>> {
    let key = "pub const RAM_RANGES: ";
    for (i, _) in text.match_indices(key) {
        let cfg = text[..i].lines().rev().take(4).find(|l| l.trim().starts_with("#[cfg(")).map(|l| l.to_string());
        let fits = match &cfg {
            None => true,
            Some(c) => c.contains(&format!("\"{feature}\"")),
        };
        if !fits {
            continue;
        }
        let end = text[i..].find("];")? + i;
        let block = &text[i..end];
        let mut rows = Vec::new();
        for (j, _) in block.match_indices("RamFacts {") {
            let row_end = block[j..].find('}')? + j;
            let row = &block[j + "RamFacts {".len()..row_end];
            let field = |name: &str| -> Option<String> {
                let k = format!("{name}:");
                let at = row.find(&k)? + k.len();
                let rest = row[at..].trim_start();
                let stop = rest.find(',').unwrap_or(rest.len());
                Some(rest[..stop].trim().to_string())
            };
            let name = field("name")?.trim_matches('"').to_string();
            let base = rust_literal(&field("base")?)?;
            let bytes = rust_literal(&field("bytes")?)?;
            let external = field("external")? == "true";
            let cached = field("cached")? == "true";
            rows.push((name, base, bytes, external, cached));
        }
        return Some(rows);
    }
    None
}

/// The `FLASH_RANGES` rows of a kernel facts module, where the row states
/// them: each `FlashFacts { name: "..", base: .., bytes: .., external: ..,
/// mapped: .., executable: .. }` row read by its fields; None on a module
/// without the table (a row whose facts file carries no flash array).
fn kernel_flash_ranges(text: &str) -> Option<Vec<(String, u64, u64, bool, bool, bool)>> {
    let key = "pub const FLASH_RANGES: ";
    let i = text.find(key)?;
    let end = text[i..].find("];")? + i;
    let block = &text[i..end];
    let mut rows = Vec::new();
    for (j, _) in block.match_indices("FlashFacts {") {
        let row_end = block[j..].find('}')? + j;
        let row = &block[j + "FlashFacts {".len()..row_end];
        let field = |name: &str| -> Option<String> {
            let k = format!("{name}:");
            let at = row.find(&k)? + k.len();
            let rest = row[at..].trim_start();
            let stop = rest.find(',').unwrap_or(rest.len());
            Some(rest[..stop].trim().to_string())
        };
        let name = field("name")?.trim_matches('"').to_string();
        let base = rust_literal(&field("base")?)?;
        let bytes = rust_literal(&field("bytes")?)?;
        let external = field("external")? == "true";
        let mapped = field("mapped")? == "true";
        let executable = field("executable")? == "true";
        rows.push((name, base, bytes, external, mapped, executable));
    }
    Some(rows)
}

/// The kernel's `UART_ROWS` table of a facts module: each
/// `UartRow { block: .., tx: .., rx: .. }` row's block and pins, a pin
/// `NO_PIN` read as absent (the emulator rows' UARTs have no pins); an
/// empty table on a row with no UART row (the ST rows).
fn kernel_uart_rows(text: &str) -> Option<Vec<(u64, Option<u64>, Option<u64>)>> {
    let start = text.find("pub const UART_ROWS: ")?;
    let end = text[start..].find("];")? + start;
    let block = &text[start..end];
    let mut out = Vec::new();
    for row in block.split("UartRow {").skip(1) {
        let pin = |key: &str| -> Option<Option<u64>> {
            let i = row.find(key)? + key.len();
            if row[i..].trim_start().starts_with("crate::console::NO_PIN") {
                Some(None)
            } else {
                Some(Some(field_number(row, key)?))
            }
        };
        out.push((field_number(row, "block:")?, pin("tx:")?, pin("rx:")?));
    }
    Some(out)
}

/// The kernel's low-power UART blocks of a facts module, by number: the
/// `UART_BLOCKS` table holds one block for each row of `UART_ROWS`, in the
/// rows' order, and a block under `Law::LowPower` is a low-power UART; its
/// number is its row's. A module without the table (the rows of another
/// chip family) has none.
fn kernel_uart_lowpower(text: &str, rows: &[(u64, Option<u64>, Option<u64>)]) -> Option<Vec<u64>> {
    let Some(start) = text.find("pub const UART_BLOCKS: ") else {
        return Some(Vec::new());
    };
    let end = text[start..].find("];")? + start;
    let mut out = Vec::new();
    for (i, block) in text[start..end].split("Block {").skip(1).enumerate() {
        if block.contains("Law::LowPower") {
            out.push(rows.get(i)?.0);
        }
    }
    Some(out)
}

/// The word of a list of UART blocks: bit n set for block n, as the board
/// table's `UartLowPowerBlocks` and the link word state them.
fn uart_blocks_word(blocks: &[u64]) -> u64 {
    blocks.iter().fold(0u64, |word, b| word | 1u64.checked_shl(*b as u32).unwrap_or(0))
}

/// The kernel's `CHANNELS` facts of a row: the two streams'
/// default codes and the legal set, each `CH_*` name read as the SDK's word.
fn kernel_channels(text: &str) -> Option<(String, String, Vec<String>)> {
    let start = text.find("pub const CHANNELS: ")?;
    let end = text[start..].find("};")? + start;
    let decl = &text[start..end];
    let word = |key: &str| -> Option<String> {
        let i = decl.find(key)? + key.len();
        channel_word(decl[i..].trim_start())
    };
    let log = word(" log: ")?;
    let console = word(" console: ")?;
    let li = decl.find("legal:")? + "legal:".len();
    let open = decl[li..].find('[')? + li + 1;
    let close = decl[open..].find(']')? + open;
    let mut legal = Vec::new();
    for item in decl[open..close].split(',') {
        if item.trim().is_empty() {
            continue;
        }
        legal.push(channel_word(item.trim())?);
    }
    Some((log, console, legal))
}

/// A `CH_*` constant's name, as the kernel's facts write it, read as the
/// SDK's word for the channel.
fn channel_word(s: &str) -> Option<String> {
    let name: String = s
        .trim_start_matches("crate::console::")
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
        .collect();
    let word = match name.as_str() {
        "CH_MEMORY" => "memory",
        "CH_PROBE" => "probe",
        "CH_OFF" => "off",
        "CH_UART0" => "uart0",
        "CH_UART1" => "uart1",
        _ => return None,
    };
    Some(word.to_string())
}

/// The kernel's `UART_PAIRS` table of a facts module: per block, the
/// TX pins and the RX pins the UART function reaches; an empty table on the
/// emulator and ST rows.
fn kernel_uart_pairs(text: &str) -> Option<Vec<(u64, Vec<u64>, Vec<u64>)>> {
    let start = text.find("pub const UART_PAIRS: ")?;
    let end = text[start..].find("];")? + start;
    let block = &text[start..end];
    let mut out = Vec::new();
    for row in block.split("UartPairs {").skip(1) {
        let list = |key: &str| -> Option<Vec<u64>> {
            let i = row.find(key)? + key.len();
            let open = row[i..].find('[')? + i + 1;
            let close = row[open..].find(']')? + open;
            row[open..close].split(',').map(|n| n.trim().parse().ok()).collect()
        };
        out.push((field_number(row, "block:")?, list("tx:")?, list("rx:")?));
    }
    Some(out)
}

/// One row's facts file beside the kernel's facts.
#[derive(Clone)]
struct BoardFactsRow {
    board: String,
    lines: u64,
    kernel_lines: u64,
    clock_hz: u64,
    kernel_clock_hz: u64,
    ties: Vec<(String, u64, u64)>,
    /// The facts file's two bus lists as it states them, and the bus rows the kernel's facts write out in full
    /// for the row (None on a row whose buses the chip's module makes): the two sides of the second bus tie.
    bus_lists: (Vec<I2cFacts>, Vec<SpiFacts>),
    kernel_buses: Option<(Vec<LiteralBus>, Vec<LiteralBus>)>,
    /// The facts file's I2C rates and the kernel's modes whose floor the row's clock reaches, in order (the rate tie).
    rates: Vec<u64>,
    kernel_rates: Vec<u64>,
    /// Per SPI bus: the facts file's slowest and fastest rate, and the block's ladder's at its clock.
    ladder: Vec<(u64, u64, u64, u64)>,
    kernel_spi_hz: u64,
    /// The facts file's PWM chip and lines against the kernel's PWM_LINES table.
    pwm_chip: u64,
    pwm_lines: Vec<u64>,
    kernel_pwm_lines: Vec<u64>,
    /// The facts file's RAM ranges and the kernel's (`RAM_RANGES` in the row's facts module).
    ram: Vec<(String, u64, u64, bool, bool)>,
    kernel_ram: Vec<(String, u64, u64, bool, bool)>,
    /// The facts file's flash ranges and the kernel's (`FLASH_RANGES`), both empty on a row that states none.
    flash: Vec<(String, u64, u64, bool, bool, bool)>,
    kernel_flash: Vec<(String, u64, u64, bool, bool, bool)>,
    /// The facts file's UART rows and the kernel's (`UART_ROWS`).
    uart: Vec<(u64, Option<u64>, Option<u64>)>,
    kernel_uart: Vec<(u64, Option<u64>, Option<u64>)>,
    /// The low-power UART blocks: the facts file's (a UART row's kind), the kernel's (a block of `UART_BLOCKS`
    /// under `Law::LowPower`, by its row's number) and the board table's word (`UartLowPowerBlocks`, 0 without it).
    uart_lowpower: Vec<u64>,
    kernel_uart_lowpower: Vec<u64>,
    table_uart_lowpower: u64,
    /// The facts file's channels and the kernel's (`CHANNELS`).
    channels: (String, String, Vec<String>),
    kernel_channels: (String, String, Vec<String>),
    /// The facts file's legal pin sets and the kernel's (`UART_PAIRS`).
    uart_pairs: Vec<(u64, Vec<u64>, Vec<u64>)>,
    kernel_uart_pairs: Vec<(u64, Vec<u64>, Vec<u64>)>,
    /// The facts file's analog converters and the kernel's (`ADC_BLOCKS` and `ADC_CHANNELS`).
    adc: Vec<AdcFacts>,
    kernel_adc: Vec<AdcFacts>,
    /// The facts file's net object and the network pool the kernel gives the row (the build script's medium for
    /// the row's board feature and the network module's constants); None on either side where there is none.
    net: Option<NetFacts>,
    kernel_net: Option<NetFacts>,
    /// The flash regions the table's row names (FlashRegions), and whether the row's kernel facts state a radio
    /// corner (RADIO_CORNER_OFFSET): the firmware of a row whose kernels carry the radio lies in such a region of
    /// the board package, and the corner is the region's place.
    flash_regions: Option<String>,
    kernel_corner: bool,
}

/// Every row of the board table has a facts file naming it; its pin count,
/// its clock at the row's posture and the bus pads are read against the
/// kernel's facts (`src/kernel/src/board/facts/<row>.rs`, the emulator rows
/// sharing `mps2.rs`; the chip's bus rows in `rp2350_bus_math.rs`). Nothing
/// of LEDs or buttons is in the facts files: the board pages' matter.
fn read_board_facts(repo: &Path) -> Result<Vec<BoardFactsRow>, String> {
    let sdk = repo.join("src").join("sdk").join("Lamella.Rhu.Sdk");
    let boards = std::fs::read_to_string(sdk.join("Sdk").join("boards.props")).map_err(|e| format!("boards.props: {e}"))?;
    let kboard = repo.join("src").join("kernel").join("src").join("board");
    // The kernel's network medium per board feature and its pool, for the facts files' net objects.
    let net_build = std::fs::read_to_string(repo.join("src").join("kernel").join("build.rs")).map_err(|e| format!("build.rs: {e}"))?;
    let net_module = std::fs::read_to_string(repo.join("src").join("kernel").join("src").join("net.rs")).map_err(|e| format!("net.rs: {e}"))?;
    let mut rows = Vec::new();
    for (i, _) in boards.match_indices("<LamellaTargetBoardRow Include=\"") {
        let end = boards[i..].find("/>").ok_or("a board row without its end")? + i;
        let board = row_attr_text(&boards[i..end], "Include").ok_or("a board row without a name")?;
        let flash_regions = row_attr_text(&boards[i..end], "FlashRegions");
        let facts = read_facts_file(&sdk.join("boards").join(format!("{board}.json")))?;
        if facts.name != board {
            return Err(format!("boards/{board}.json names '{}', not its row", facts.name));
        }
        let name = board.strip_prefix("arm-").unwrap_or(&board).replace('-', "_");
        let mut kfile = kboard.join("facts").join(format!("{name}.rs"));
        if !kfile.is_file() {
            let shared = name.rsplit_once('_').map(|(a, _)| a.to_string()).unwrap_or(name.clone());
            kfile = kboard.join("facts").join(format!("{shared}.rs"));
        }
        let ktext = std::fs::read_to_string(&kfile).map_err(|e| format!("{}: {e}", kfile.display()))?;
        let kernel_lines = rust_const(&ktext, "LINE_COUNT").and_then(|v| rust_number(&v)).ok_or(format!("{}: no LINE_COUNT", kfile.display()))?;
        let bfile = kboard.join(format!("{name}.rs"));
        let btext = std::fs::read_to_string(&bfile).map_err(|e| format!("{}: {e}", bfile.display()))?;
        let kernel_clock_hz = match kernel_sysclk(&btext, &facts.posture) {
            Some(v) => v,
            None => {
                let chip = btext
                    .lines()
                    .find_map(|l| l.trim().strip_prefix("pub use super::").and_then(|r| r.strip_suffix("::*;")))
                    .ok_or(format!("{}: no SYSCLK_HZ at the posture '{}' and no chip module re-export", bfile.display(), facts.posture))?;
                let ctext = std::fs::read_to_string(kboard.join(format!("{chip}.rs"))).map_err(|e| format!("{chip}.rs: {e}"))?;
                kernel_sysclk(&ctext, &facts.posture).ok_or(format!("{chip}.rs: no SYSCLK_HZ at the posture '{}'", facts.posture))?
            }
        };
        let mut ties = Vec::new();
        let mut rates = Vec::new();
        let mut kernel_rates = Vec::new();
        let mut ladder = Vec::new();
        let mut kernel_spi_hz = 0;
        // A row's bus rows stand in one of two forms in its kernel facts. A chip's module makes them (the shared
        // rows rp2350_bus_math::i2c_buses, or the row's own through the constructors i2c_bus and spi_bus over the
        // facts file's I2C_PADS, SPI_PADS and SPI_SELECTS): the pads are read where the row declares them. Or the
        // facts file writes them out in full (bus_math::I2cBus and bus_math::SpiBus rows): every such bus that
        // has pads is a bus of the board's facts file and the reverse; the rates are those bus_math::i2c_setup
        // serves -- fast mode plus by the row's fact for it, at that fact's clock, and every other rate at the
        // bus's own clock, each where bus_math::i2c_timing has a word for the pair -- and the range is the
        // divisor ladder's at the bus's own clock.
        let own = ktext.contains("rp2350_bus_math::i2c_bus(") || ktext.contains("rp2350_bus_math::spi_bus(");
        let made_by_chip = ktext.contains("rp2350_bus_math::i2c_buses") || own;
        let mut kernel_buses = None;
        if !made_by_chip {
            let row_feature = format!("board-{}", board.strip_prefix("arm-").unwrap_or(&board));
            let k_i2c = kernel_literal_buses(&ktext, "I2C_BUSES", "I2cBus", &row_feature).ok_or(format!("{}: an I2C bus row the tie cannot read", kfile.display()))?;
            let k_spi = kernel_literal_buses(&ktext, "SPI_BUSES", "SpiBus", &row_feature).ok_or(format!("{}: an SPI bus row the tie cannot read", kfile.display()))?;
            ties = literal_bus_ties(&board, &facts.i2c, &facts.spi, &k_i2c, &k_spi)?;
            if !ties.is_empty() {
                let math = std::fs::read_to_string(repo.join("src").join("kernel").join("src").join("bus_math.rs")).map_err(|e| format!("bus_math.rs: {e}"))?;
                let timings = kernel_i2c_timings(&math).ok_or("bus_math.rs: i2c_timing is not a match over pairs of kernel_hz and rate_hz as the tie reads it")?;
                let rate_set = kernel_i2c_rate_set(&math).ok_or("bus_math.rs: I2C_RATES_HZ and the rule of i2c_setup are not there as the tie reads them")?;
                if !math.contains("while n < 8") || !math.contains("kernel_hz >> (n + 1)") {
                    return Err("bus_math.rs: the SPI divisor ladder is not the eight halvings of the kernel clock the tie reads".to_string());
                }
                let chip_text = btext.lines().find_map(|l| l.trim().strip_prefix("pub use super::").and_then(|r| r.strip_suffix("::*;"))).and_then(|chip| std::fs::read_to_string(kboard.join(format!("{chip}.rs"))).ok());
                let clock_of = |bus: &LiteralBus, what: &str| kernel_bus_clock(&bus.clock, chip_text.as_deref(), kernel_clock_hz).ok_or(format!("{board}: the kernel clock of {what}{} ({}) is not a number the tie can read", bus.number, bus.clock));
                let fast_plus_hz = match k_i2c.first() {
                    Some(first) => kernel_i2c_fast_plus(&ktext, &row_feature, clock_of(first, "I2C")?, chip_text.as_deref(), kernel_clock_hz).ok_or(format!("{}: the row's fact for fast mode plus (I2C_FAST_PLUS) is not stated in a form the tie reads", kfile.display()))?,
                    None => None,
                };
                for (block, _, _, r) in &facts.i2c {
                    if let Some(bus) = k_i2c.iter().find(|b| b.number == *block) {
                        let hz = clock_of(bus, "I2C")?;
                        let served = |rate: u64| (if rate == rate_set[2] { fast_plus_hz } else { Some(hz) }).is_some_and(|clock| timings.contains(&(clock, rate)));
                        kernel_rates = rate_set.iter().copied().filter(|rate| served(*rate)).collect();
                        rates = r.clone();
                    }
                }
                for (block, _, _, _, _, min, max) in &facts.spi {
                    if let Some(bus) = k_spi.iter().find(|b| b.number == *block) {
                        kernel_spi_hz = clock_of(bus, "SPI")?;
                        ladder.push((*min, *max, kernel_spi_hz >> 8, kernel_spi_hz >> 1));
                    }
                }
            }
            kernel_buses = Some((k_i2c, k_spi));
        } else if !facts.i2c.is_empty() || !facts.spi.is_empty() {
            let bus = std::fs::read_to_string(repo.join("src").join("kernel").join("src").join("rp2350_bus_math.rs")).map_err(|e| format!("rp2350_bus_math.rs: {e}"))?;
            let (i2c_pads, spi_pads, selects) = if own {
                (
                    pad_lines(&ktext, "I2C_PADS").ok_or(format!("{board}: the facts file names no I2C_PADS the tie can read"))?,
                    pad_lines(&ktext, "SPI_PADS").ok_or(format!("{board}: the facts file names no SPI_PADS the tie can read"))?,
                    pad_lines(&ktext, "SPI_SELECTS").ok_or(format!("{board}: the facts file names no SPI_SELECTS the tie can read"))?,
                )
            } else {
                (
                    pad_lines(&bus, "I2C0_PADS").ok_or("rp2350_bus_math.rs: no I2C0_PADS")?,
                    pad_lines(&bus, "SPI0_PADS").ok_or("rp2350_bus_math.rs: no SPI0_PADS")?,
                    pad_lines(&bus, "SPI0_SELECTS").ok_or("rp2350_bus_math.rs: no SPI0_SELECTS")?,
                )
            };
            // The rates the facts list are the kernel's modes whose floor is at or below the row's clock at its posture, in the set's order;
            // the SPI range is the block's ladder at the block's own clock (the crystal on the RP2350 rows).
            let modes = kernel_i2c_modes(&bus).ok_or("rp2350_bus_math.rs: no I2C_MODES rows with rate_hz and floor_hz")?;
            kernel_spi_hz = rust_const(&bus, "SPI_CLOCK_HZ").and_then(|v| rust_number(&v)).ok_or("rp2350_bus_math.rs: no SPI_CLOCK_HZ")?;
            kernel_rates = modes.iter().filter(|(_, floor)| *floor <= kernel_clock_hz).map(|(rate, _)| *rate).collect();
            for (block, sda, scl, r) in &facts.i2c {
                let named = if own { ktext.contains(&format!("rp2350_bus_math::i2c_bus({block}, ")) } else { *block == 0 };
                if !named || i2c_pads.len() != 2 {
                    return Err(format!("{board}: the kernel's bus rows carry no I2C block {block} with two pads"));
                }
                ties.push((format!("I2C{block}'s data pad"), *sda, i2c_pads[0]));
                ties.push((format!("I2C{block}'s clock pad"), *scl, i2c_pads[1]));
                rates = r.clone();
            }
            for (block, sck, mosi, miso, cs, min, max) in &facts.spi {
                let named = if own { ktext.contains(&format!("rp2350_bus_math::spi_bus({block}, ")) } else { *block == 0 };
                if !named || spi_pads.len() != 3 || cs.len() != selects.len() {
                    return Err(format!("{board}: the kernel's bus rows carry no SPI block {block} with three pads and {} selects", selects.len()));
                }
                ties.push((format!("SPI{block}'s clock pad"), *sck, spi_pads[0]));
                ties.push((format!("SPI{block}'s out pad"), *mosi, spi_pads[1]));
                ties.push((format!("SPI{block}'s in pad"), *miso, spi_pads[2]));
                for (k, c) in cs.iter().enumerate() {
                    ties.push((format!("SPI{block}'s select {k}"), *c, selects[k]));
                }
                ladder.push((*min, *max, kernel_spi_hz / (254 * 256), kernel_spi_hz / 2));
            }
        }
        let kernel_pwm_lines = kernel_pwm_lines(&ktext, kernel_lines).ok_or(format!("{}: no PWM_LINES table the tie can read", kfile.display()))?;
        let feature = format!("board-{}", board.strip_prefix("arm-").unwrap_or(&board));
        let kernel_ram = kernel_ram_ranges(&ktext, &feature).ok_or(format!("{}: no RAM_RANGES rows for the feature {feature}", kfile.display()))?;
        let kernel_flash = kernel_flash_ranges(&ktext).unwrap_or_default();
        let kernel_uart = kernel_uart_rows(&ktext).ok_or(format!("{}: no UART_ROWS table the tie can read", kfile.display()))?;
        let kernel_uart_lowpower = kernel_uart_lowpower(&ktext, &kernel_uart).ok_or(format!("{}: a UART_BLOCKS table the tie cannot read against its UART_ROWS", kfile.display()))?;
        let table_uart_lowpower = row_attr(&boards[i..end], "UartLowPowerBlocks").unwrap_or(0);
        let kernel_channels = kernel_channels(&ktext).ok_or(format!("{}: no CHANNELS facts the tie can read", kfile.display()))?;
        let kernel_uart_pairs = kernel_uart_pairs(&ktext).ok_or(format!("{}: no UART_PAIRS table the tie can read", kfile.display()))?;
        let kernel_adc = kernel_adc_tables(&ktext).ok_or(format!("{}: no ADC_BLOCKS and ADC_CHANNELS tables the tie can read", kfile.display()))?;
        let kernel_net = kernel_net_facts(&net_build, &net_module, &feature).ok_or(format!("{board}: the kernel's network medium and pool are not there as the tie reads them (build.rs, net.rs)"))?;
        rows.push(BoardFactsRow { board, lines: facts.pins, kernel_lines, clock_hz: facts.clock_hz, kernel_clock_hz, ties, bus_lists: (facts.i2c, facts.spi), kernel_buses, rates, kernel_rates, ladder, kernel_spi_hz, pwm_chip: facts.pwm_chip, pwm_lines: facts.pwm_lines.clone(), kernel_pwm_lines, ram: facts.ram, kernel_ram, flash: facts.flash, kernel_flash, uart: facts.uart, kernel_uart, uart_lowpower: facts.uart_lowpower, kernel_uart_lowpower, table_uart_lowpower,channels: facts.channels, kernel_channels, uart_pairs: facts.uart_pairs, kernel_uart_pairs, adc: facts.adc.clone(), kernel_adc, net: facts.net, kernel_net, flash_regions, kernel_corner: ktext.contains("pub const RADIO_CORNER_OFFSET: usize = ") });
    }
    Ok(rows)
}

/// The kernel's PWM_LINES table as the row's facts module states it: the
/// literal rows' `line:` numbers (the emulator rows), every line of the package
/// where the table is the chip's generated one (`rp2350_pwm::lines::<LINE_COUNT>()`),
/// none where the table is empty (the ST rows).
fn kernel_pwm_lines(text: &str, line_count: u64) -> Option<Vec<u64>> {
    let start = text.find("pub const PWM_LINES: ")?;
    let end = text[start..].find(';')? + start;
    let decl = &text[start..end];
    if decl.contains("rp2350_pwm::lines::<LINE_COUNT>()") {
        return Some((0..line_count).collect());
    }
    if decl.contains("= &[]") {
        return Some(Vec::new());
    }
    let mut out = Vec::new();
    for row in decl.split("PwmLine {").skip(1) {
        out.push(field_number(row, "line:")?);
    }
    if out.is_empty() {
        None
    } else {
        Some(out)
    }
}

/// The kernel's ADC_BLOCKS and ADC_CHANNELS tables as the row's facts module
/// states them, in the facts file's shape: a converter's number is its index in
/// the block table (the table's own `number:` is the chip's), its `bits:` and
/// `reference_mv:` as written, and its channels the channel rows whose `block:`
/// names it -- the chip's `channel:` number with its `line:`, None for
/// `NO_LINE` (an internal source); two empty tables give an empty list.
fn kernel_adc_tables(text: &str) -> Option<Vec<AdcFacts>> {
    let blocks = rust_const(text, "ADC_BLOCKS")?;
    let channels = rust_const(text, "ADC_CHANNELS")?;
    let mut out: Vec<AdcFacts> = Vec::new();
    if blocks.trim() != "&[]" {
        for row in blocks.split("AdcBlock {").skip(1) {
            out.push((out.len() as u64, field_number(row, "bits:")?, field_number(row, "reference_mv:")?, Vec::new()));
        }
    }
    if channels.trim() != "&[]" {
        for row in channels.split("AdcChannel {").skip(1) {
            let block = field_number(row, "block:")? as usize;
            let number = field_number(row, "channel:")?;
            let line = if row.contains("line: crate::adc_math::NO_LINE") { None } else { Some(field_number(row, "line:")?) };
            out.get_mut(block)?.3.push((number, line));
        }
    }
    Some(out)
}

/// The network pool the kernel gives a row, in the facts file's shape, read
/// as text where the kernel states it. The medium: the build script's blocks
/// that open `if cfg!(all(feature = "net", ...)) {` and set a medium's
/// configuration word (`rhu_net_wired` or `rhu_net_virtual`, beside
/// `rhu_net_rows`) for the board features they name, and the radio's block,
/// which opens `if cfg!(feature = "wifi-cyw43") {`, names its boards in its
/// refusal line (`if !cfg!(any(feature = ..., ...)) {`) and sets
/// `rhu_net_wireless` beside `rhu_net_rows` before its end: a row's kernels
/// built with that feature have the radio as their medium. The pool: the network
/// module's STREAMS, DATAGRAMS, STREAM_TX, DGRAM_BUF and RESOLVE_SLOTS, each
/// stated once as a plain number; STREAM_RX, stated once by two arms, the
/// first the wired and the wireless medium's and the second the virtual
/// medium's; and KIND_WORD, stated once by three arms, the wired, the
/// wireless and the virtual medium's in that order. The outer None says the
/// text is not there as read, a statement that names fewer media and a
/// feature given two media among the cases; the inner None that the kernel
/// gives the row's feature no medium.
fn kernel_net_facts(build: &str, net: &str, feature: &str) -> Option<Option<NetFacts>> {
    let lines: Vec<&str> = build.lines().collect();
    let names = |line: &str| -> bool { line.split("feature = \"").skip(1).filter_map(|p| p.split('"').next()).any(|name| name == feature) };
    // The row's medium as its arm of KIND_WORD: 0 the wired, 1 the wireless, 2 the virtual.
    let mut medium: Option<usize> = None;
    for (i, l) in lines.iter().enumerate() {
        let s = l.trim();
        let arm = if s == "if cfg!(feature = \"wifi-cyw43\") {" {
            // The radio's block, to the closing brace at its own depth.
            let close = format!("{}}}", &l[..l.len() - l.trim_start().len()]);
            let len = lines.iter().skip(i + 1).position(|b| *b == close)?;
            let body = &lines[i + 1..i + 1 + len];
            let refusal = body.iter().map(|b| b.trim()).find(|b| b.starts_with("if !cfg!(any(") && b.ends_with('{'))?;
            let words = body.join(" ");
            if !words.contains("cargo:rustc-cfg=rhu_net_rows") || !words.contains("cargo:rustc-cfg=rhu_net_wireless") {
                return None;
            }
            names(refusal).then_some(1)
        } else if s.starts_with("if cfg!(all(feature = \"net\",") && s.ends_with('{') {
            let body = lines.iter().skip(i + 1).take(3).copied().collect::<Vec<&str>>().join(" ");
            let arm = if body.contains("rhu_net_wired") {
                0
            } else if body.contains("rhu_net_virtual") {
                2
            } else {
                return None;
            };
            if !body.contains("rhu_net_rows") {
                return None;
            }
            names(s).then_some(arm)
        } else {
            None
        };
        if arm.is_some() {
            if medium.is_some() {
                return None;
            }
            medium = arm;
        }
    }
    let Some(medium) = medium else {
        return Some(None);
    };
    let plain = |name: &str| -> Option<u64> {
        let key = format!("pub const {name}: usize = ");
        let mut found = None;
        for l in net.lines() {
            if let Some(rest) = l.trim().strip_prefix(key.as_str()) {
                if found.is_some() {
                    return None;
                }
                found = Some(rest.strip_suffix(';')?.trim().parse::<u64>().ok()?);
            }
        }
        found
    };
    // One statement's arms in their order: the text behind `head`, cut at each of `joints` in turn and closed by ` };`.
    let arms = |head: &str, joints: &[&str]| -> Option<Vec<String>> {
        let mut found = None;
        for l in net.lines() {
            if let Some(mut rest) = l.trim().strip_prefix(head) {
                if found.is_some() {
                    return None;
                }
                let mut out = Vec::new();
                for joint in joints {
                    let (arm, behind) = rest.split_once(*joint)?;
                    out.push(arm.trim().to_string());
                    rest = behind;
                }
                out.push(rest.strip_suffix(" };")?.trim().to_string());
                if out.iter().any(|arm| arm.contains('{') || arm.contains('}')) {
                    return None;
                }
                found = Some(out);
            }
        }
        found
    };
    let rx = arms("pub const STREAM_RX: usize = if cfg!(any(rhu_net_wired, rhu_net_wireless)) { ", &[" } else { "])?;
    let rx = rx.iter().map(|arm| arm.parse::<u64>().ok()).collect::<Option<Vec<u64>>>()?;
    let words = arms("pub const KIND_WORD: &str = if cfg!(rhu_net_wired) { ", &[" } else if cfg!(rhu_net_wireless) { ", " } else { "])?;
    let words = words.iter().map(|arm| arm.strip_prefix('"').and_then(|w| w.strip_suffix('"')).map(str::to_string)).collect::<Option<Vec<String>>>()?;
    // The row's medium takes its arm of each statement: the wired and the wireless medium share STREAM_RX's first.
    let (word, rx) = (words[medium].clone(), if medium == 2 { rx[1] } else { rx[0] });
    Some(Some((word, plain("STREAMS")?, plain("DATAGRAMS")?, rx, plain("STREAM_TX")?, plain("DGRAM_BUF")?, plain("RESOLVE_SLOTS")?)))
}

/// A net object as words: `wired, 4 stream and 2 datagram sockets, stream buffers 4096 and 2048 B, a datagram
/// buffer of 1536 B, 4 resolver slots` (`none` where there is no object).
fn net_words(n: &Option<NetFacts>) -> String {
    match n {
        None => "none".to_string(),
        Some((medium, streams, datagrams, rx, tx, dgram, slots)) => {
            format!("{medium}, {streams} stream and {datagrams} datagram sockets, stream buffers {rx} and {tx} B, a datagram buffer of {dgram} B, {slots} resolver slots")
        }
    }
}

fn board_facts_fault(rows: &[BoardFactsRow]) -> Option<String> {
    if rows.is_empty() {
        return Some("the board table has no rows with facts files".to_string());
    }
    for r in rows {
        if r.lines != r.kernel_lines {
            return Some(format!("{}: the facts file's pins {} != the kernel's LINE_COUNT {}", r.board, r.lines, r.kernel_lines));
        }
        if r.clock_hz != r.kernel_clock_hz {
            return Some(format!("{}: the facts file's clockHz {} != the kernel's SYSCLK_HZ {} at the row's posture", r.board, r.clock_hz, r.kernel_clock_hz));
        }
        if r.pwm_chip != 0 {
            return Some(format!("{}: the facts file's PWM chip is {}; the one chip of a row is 0", r.board, r.pwm_chip));
        }
        if r.pwm_lines != r.kernel_pwm_lines {
            return Some(format!("{}: the facts file's PWM lines {} != the kernel's PWM_LINES {}", r.board, number_list(&r.pwm_lines), number_list(&r.kernel_pwm_lines)));
        }
        if r.adc != r.kernel_adc {
            return Some(format!("{}: the facts file's analog converters {} != the kernel's ADC_BLOCKS and ADC_CHANNELS {}", r.board, adc_list(&r.adc), adc_list(&r.kernel_adc)));
        }
        if r.rates != r.kernel_rates {
            return Some(format!("{}: the facts file's I2C rates {} != the kernel's modes at {} Hz: {}", r.board, number_list(&r.rates), r.kernel_clock_hz, number_list(&r.kernel_rates)));
        }
        for (min, max, kmin, kmax) in &r.ladder {
            if min != kmin || max != kmax {
                return Some(format!("{}: the facts file's SPI range {min} to {max} != the block's ladder {kmin} to {kmax} at {} Hz", r.board, r.kernel_spi_hz));
            }
        }
        for (what, ours, theirs) in &r.ties {
            if ours != theirs {
                return Some(format!("{}: {what} {ours} != the kernel's {theirs}", r.board));
            }
        }
        if r.ram != r.kernel_ram {
            return Some(format!("{}: the facts file's RAM ranges {} != the kernel's RAM_RANGES {}", r.board, ram_list(&r.ram), ram_list(&r.kernel_ram)));
        }
        if r.flash != r.kernel_flash {
            return Some(format!("{}: the facts file's flash ranges {} != the kernel's FLASH_RANGES {}", r.board, flash_list(&r.flash), flash_list(&r.kernel_flash)));
        }
        if r.uart != r.kernel_uart {
            return Some(format!("{}: the facts file's UART rows {} != the kernel's UART_ROWS {}", r.board, uart_list(&r.uart), uart_list(&r.kernel_uart)));
        }
        // A low-power UART's device file is /dev/ttyLP<block>: the facts file's kind names the board class's port,
        // the board table's word names the hosting layer's, and both are the kernel's law for the block.
        if r.uart_lowpower != r.kernel_uart_lowpower {
            return Some(format!("{}: the facts file's low-power UART blocks {:?} != the kernel's UART_BLOCKS under Law::LowPower {:?}", r.board, r.uart_lowpower, r.kernel_uart_lowpower));
        }
        if r.table_uart_lowpower != uart_blocks_word(&r.kernel_uart_lowpower) {
            return Some(format!("{}: the board table's UartLowPowerBlocks {} != the word of the kernel's low-power UART blocks {:?} ({})", r.board, r.table_uart_lowpower, r.kernel_uart_lowpower, uart_blocks_word(&r.kernel_uart_lowpower)));
        }
        if r.channels != r.kernel_channels {
            return Some(format!("{}: the facts file's channels {} != the kernel's CHANNELS {}", r.board, channels_words(&r.channels), channels_words(&r.kernel_channels)));
        }
        if r.uart_pairs != r.kernel_uart_pairs {
            return Some(format!("{}: the facts file's uartPairs {} != the kernel's UART_PAIRS {}", r.board, pairs_list(&r.uart_pairs), pairs_list(&r.kernel_uart_pairs)));
        }
        if r.net != r.kernel_net {
            return Some(format!("{}: the facts file's net object ({}) != the network pool the kernel gives the row ({})", r.board, net_words(&r.net), net_words(&r.kernel_net)));
        }
        if r.net.as_ref().is_some_and(|n| n.0 == "wireless") && r.flash_regions.is_none() {
            return Some(format!("{}: the row's kernels carry the radio and the board table names no flash region for it (FlashRegions): the radio's firmware lies in a region of the board package", r.board));
        }
        if r.flash_regions.is_some() && !r.kernel_corner {
            return Some(format!("{}: the board table names a flash region ({}) and the row's kernel facts state no radio corner (RADIO_CORNER_OFFSET)", r.board, r.flash_regions.as_deref().unwrap_or("")));
        }
    }
    None
}

/// Analog converters as words: `converter 0: 12 bits of 3300 mV, channels 0 on 26, 1 on 27, 4 on no line`
/// (`none` for an empty list).
fn adc_list(v: &[AdcFacts]) -> String {
    if v.is_empty() {
        return "none".to_string();
    }
    v.iter()
        .map(|(number, bits, mv, channels)| {
            let ch: Vec<String> = channels
                .iter()
                .map(|(n, pin)| match pin {
                    Some(p) => format!("{n} on {p}"),
                    None => format!("{n} on no line"),
                })
                .collect();
            format!("converter {number}: {bits} bits of {mv} mV, channels {}", if ch.is_empty() { "none".to_string() } else { ch.join(", ") })
        })
        .collect::<Vec<_>>()
        .join("; ")
}

/// UART rows as words: `UART0 on 0/1; UART1 on 8/9` (`UART0 (no pins)` on the emulator rows).
fn uart_list(v: &[(u64, Option<u64>, Option<u64>)]) -> String {
    if v.is_empty() {
        return "none".to_string();
    }
    v.iter()
        .map(|(block, tx, rx)| match (tx, rx) {
            (Some(t), Some(r)) => format!("UART{block} on {t}/{r}"),
            _ => format!("UART{block} (no pins)"),
        })
        .collect::<Vec<_>>()
        .join("; ")
}

/// The channels as words: `log memory, console memory, legal memory/off/uart0/uart1`.
fn channels_words(c: &(String, String, Vec<String>)) -> String {
    format!("log {}, console {}, legal {}", c.0, c.1, c.2.join("/"))
}

/// The legal pin sets as words: `UART0 tx 0/12/16/28 rx 1/13/17/29; ...`.
fn pairs_list(v: &[(u64, Vec<u64>, Vec<u64>)]) -> String {
    if v.is_empty() {
        return "none".to_string();
    }
    let nums = |l: &[u64]| l.iter().map(|n| n.to_string()).collect::<Vec<_>>().join("/");
    v.iter()
        .map(|(block, tx, rx)| format!("UART{block} tx {} rx {}", nums(tx), nums(rx)))
        .collect::<Vec<_>>()
        .join("; ")
}

/// RAM ranges as words: `SRAM 532480 B at 0x20000000 (internal, uncached)`.
fn ram_list(v: &[(String, u64, u64, bool, bool)]) -> String {
    if v.is_empty() {
        return "none".to_string();
    }
    v.iter()
        .map(|(name, base, bytes, external, cached)| {
            format!("{name} {bytes} B at {base:#010x} ({}, {})", if *external { "external" } else { "internal" }, if *cached { "cached" } else { "uncached" })
        })
        .collect::<Vec<_>>()
        .join("; ")
}

/// The flash ranges as words; "none" where the row states none.
fn flash_list(v: &[(String, u64, u64, bool, bool, bool)]) -> String {
    if v.is_empty() {
        return "none".to_string();
    }
    v.iter()
        .map(|(name, base, bytes, external, mapped, executable)| {
            format!("{name} {bytes} B at {base:#010x} ({}, {}, {})", if *external { "external" } else { "internal" }, if *mapped { "mapped" } else { "unmapped" }, if *executable { "executable" } else { "data" })
        })
        .collect::<Vec<_>>()
        .join("; ")
}

/// Numbers joined with commas; "none" for an empty list.
fn number_list(v: &[u64]) -> String {
    if v.is_empty() {
        "none".to_string()
    } else {
        v.iter().map(|n| n.to_string()).collect::<Vec<_>>().join(", ")
    }
}

/// The kernel's I2C modes as (rate, floor) pairs in the set's order: the
/// `rate_hz:` and `floor_hz:` numbers of each row of `pub const I2C_MODES`.
fn kernel_i2c_modes(text: &str) -> Option<Vec<(u64, u64)>> {
    let start = text.find("pub const I2C_MODES: ")?;
    let end = text[start..].find("];")? + start;
    let block = &text[start..end];
    let mut out = Vec::new();
    for row in block.split("I2cMode {").skip(1) {
        out.push((field_number(row, "rate_hz:")?, field_number(row, "floor_hz:")?));
    }
    if out.is_empty() {
        None
    } else {
        Some(out)
    }
}

/// An I2C bus and an SPI bus as a facts file lists them: the block; the data and the clock pin with the rates;
/// the clock, the out and the in pin with the selects and the range.
type I2cFacts = (u64, u64, u64, Vec<u64>);
type SpiFacts = (u64, u64, u64, u64, Vec<u64>, u64, u64);

/// One bus row a kernel facts file writes out in full (`bus_math::I2cBus { .. }`, `bus_math::SpiBus { .. }`):
/// the block's number, its kernel clock as the row writes it, its pads as line and role, its chip selects.
#[derive(Clone)]
struct LiteralBus {
    number: u64,
    clock: String,
    pads: Vec<(u64, String)>,
    selects: Vec<u64>,
}

/// A line number as a facts table writes it: a sum of products of decimal numbers (`7 * 16 + 6`).
fn line_number(expr: &str) -> Option<u64> {
    let mut total = 0u64;
    for term in expr.split('+') {
        let mut product = 1u64;
        for factor in term.split('*') {
            product = product.checked_mul(rust_number(factor)?)?;
        }
        total = total.checked_add(product)?;
    }
    Some(total)
}

/// The bus rows a facts file writes out in full under `name` (`I2C_BUSES`, `SPI_BUSES`), each a `row_type { .. }`
/// literal: the declarations with no `#[cfg(...)]` line above them and those whose line names the row's board
/// feature. A declaration a chip's constructor builds, or an empty one, yields no row.
fn kernel_literal_buses(text: &str, name: &str, row_type: &str, feature: &str) -> Option<Vec<LiteralBus>> {
    let key = format!("pub const {name}: ");
    let open = format!("{row_type} {{");
    let mut out = Vec::new();
    for (i, _) in text.match_indices(&key) {
        if let Some(above) = text[..i].lines().next_back() {
            if above.trim_start().starts_with("#[cfg(") && !above.contains(&format!("feature = \"{feature}\"")) {
                continue;
            }
        }
        let end = text[i..].find(';')? + i;
        for row in text[i..end].split(&open).skip(1) {
            let mut pads = Vec::new();
            for pad in row.split("Pad {").skip(1) {
                let line = pad.split("line:").nth(1)?.split(',').next()?;
                let role: String = pad.split("PadRole::").nth(1)?.chars().take_while(|c| c.is_ascii_alphanumeric()).collect();
                pads.push((line_number(line)?, role));
            }
            let mut selects = Vec::new();
            if let Some(list) = row.split("selects: &[").nth(1) {
                for select in list[..list.find(']')?].split(',').filter(|s| !s.trim().is_empty()) {
                    selects.push(line_number(select)?);
                }
                selects.truncate(field_number(row, "select_count:")? as usize);
            }
            let clock = row.split("kernel_hz:").nth(1)?.split(',').next()?.trim().to_string();
            out.push(LiteralBus { number: field_number(row, "number:")?, clock, pads, selects });
        }
    }
    Some(out)
}

/// A bus's kernel clock in Hz as its row writes it: a number, or a clock the chip's module names
/// (`crate::board::PCLK1_HZ`), read there at the clock configuration whose `SYSCLK_HZ` is the row's clock --
/// the definition under the same `#[cfg(...)]` line, `SYSCLK_HZ` itself or `SYSCLK_HZ / <n>`.
fn kernel_bus_clock(clock: &str, chip_text: Option<&str>, sysclk_hz: u64) -> Option<u64> {
    if let Some(n) = rust_number(clock) {
        return Some(n);
    }
    let name = clock.strip_prefix("crate::board::")?;
    let text = chip_text?;
    let cfg_above = |i: usize| text[..i].lines().rev().take(4).find(|l| l.trim().starts_with("#[cfg(")).map(|l| l.trim().to_string());
    let sys_key = "pub const SYSCLK_HZ: u32 = ";
    let key = format!("pub const {name}: u32 = ");
    for (i, _) in text.match_indices(sys_key) {
        let rest = &text[i + sys_key.len()..];
        if rust_number(&rest[..rest.find(';')?]) != Some(sysclk_hz) {
            continue;
        }
        for (j, _) in text.match_indices(&key) {
            if cfg_above(j) != cfg_above(i) {
                continue;
            }
            let rest = &text[j + key.len()..];
            let value = rest[..rest.find(';')?].trim();
            return match value.strip_prefix("SYSCLK_HZ") {
                Some("") => Some(sysclk_hz),
                Some(over) => Some(sysclk_hz / rust_number(over.trim().strip_prefix('/')?)?),
                None => rust_number(value),
            };
        }
    }
    None
}

/// The pairs of kernel clock and rate that `bus_math::i2c_timing` serves, as the arms of its match over the
/// pair write them (`(16_000_000, 100_000) => Some(0x3042_0F13),`), closed by the arm that serves no other pair
/// (`_ => None,`). None where the function is not that match or an arm has another form.
fn kernel_i2c_timings(text: &str) -> Option<Vec<(u64, u64)>> {
    let start = text.find("pub const fn i2c_timing(")?;
    let end = text[start..].find("\n}")? + start;
    let arms = text[start..end].split_once("match (kernel_hz, rate_hz) {")?.1;
    let mut out = Vec::new();
    let mut closed = false;
    for arm in arms.lines().map(str::trim).filter(|l| l.contains("=>")) {
        if closed {
            return None;
        }
        if arm == "_ => None," {
            closed = true;
            continue;
        }
        let (pair, word) = arm.strip_prefix('(')?.split_once(") => Some(0x")?;
        let (clock, rate) = pair.split_once(", ")?;
        if !word.strip_suffix("),")?.chars().all(|c| c.is_ascii_hexdigit() || c == '_') {
            return None;
        }
        out.push((rust_number(clock)?, rust_number(rate)?));
    }
    if out.is_empty() || !closed {
        None
    } else {
        Some(out)
    }
}

/// The rates a claim may name, in the set's order (`pub const I2C_RATES_HZ: [u32; 3] = [..];`), read with the
/// rule `bus_math::i2c_setup` states over the set: its last rate, fast mode plus, is served by the row's fact
/// for the mode alone, at that fact's clock, and every other rate at the bus's own clock. None where the set
/// or the rule's three lines are not there as read.
fn kernel_i2c_rate_set(text: &str) -> Option<Vec<u64>> {
    let start = text.find("pub const fn i2c_setup(")?;
    let end = text[start..].find("\n}")? + start;
    let rule = &text[start..end];
    if !rule.contains("if rate_hz == I2C_RATES_HZ[2] {") || !rule.contains("match i2c_timing(f.kernel_hz, rate_hz) {") || !rule.contains("match i2c_timing(bus_hz, rate_hz) {") {
        return None;
    }
    let set = rust_const(text, "I2C_RATES_HZ")?;
    let rates = set.strip_prefix('[')?.strip_suffix(']')?.split(',').map(rust_number).collect::<Option<Vec<u64>>>()?;
    if rates.len() == 3 {
        Some(rates)
    } else {
        None
    }
}

/// The kernel clock a row's I2C block takes for fast mode plus, as the row's facts state the fact
/// (`pub const I2C_FAST_PLUS`, the one declaration with no `#[cfg(...)]` line above it or with the row's board
/// feature on that line): the inner None where the row states none, or states the fact under a condition that
/// does not hold at the row's clock; the outer None where the statement is not one of the forms read here.
/// The forms: `None`; one `Some(..I2cFastPlus { mux_value: .., kernel_hz: <clock>, drive: .. })`; and that row
/// under a condition, `if <clock> as u64 * <a> / <b> > <n> { Some(..) } else { None }`. A clock is the first
/// bus's own (`I2C_BUSES[0].kernel_hz`, passed as `first_hz`), or one `kernel_bus_clock` reads.
fn kernel_i2c_fast_plus(text: &str, feature: &str, first_hz: u64, chip_text: Option<&str>, sysclk_hz: u64) -> Option<Option<u64>> {
    let key = "pub const I2C_FAST_PLUS: ";
    let mut stated = None;
    for (i, _) in text.match_indices(key) {
        if let Some(above) = text[..i].lines().next_back() {
            if above.trim_start().starts_with("#[cfg(") && !above.contains(&format!("feature = \"{feature}\"")) {
                continue;
            }
        }
        if stated.is_some() {
            return None;
        }
        let end = text[i..].find(';')? + i;
        stated = Some(text[i..end].split_once(" = ")?.1.split_whitespace().collect::<Vec<&str>>().join(" "));
    }
    let stated = stated?;
    let clock = |expr: &str| if expr == "I2C_BUSES[0].kernel_hz" { Some(first_hz) } else { kernel_bus_clock(expr, chip_text, sysclk_hz) };
    // The clock's text of one `Some(..I2cFastPlus { mux_value: .., kernel_hz: <clock>, drive: .. })`.
    let row_clock = |row: &str| -> Option<String> {
        let fields: Vec<&str> = row.strip_prefix("Some(")?.strip_suffix(" })")?.split_once("I2cFastPlus { ")?.1.split(", ").collect();
        if fields.len() != 3 || !fields[0].starts_with("mux_value: ") || !matches!(fields[2], "drive: true" | "drive: false") {
            return None;
        }
        fields[1].strip_prefix("kernel_hz: ").map(str::to_string)
    };
    if stated == "None" {
        return Some(None);
    }
    let Some(conditional) = stated.strip_prefix("if ") else {
        return Some(Some(clock(&row_clock(&stated)?)?));
    };
    let (condition, rest) = conditional.split_once(" { ")?;
    let (row, tail) = rest.split_once(" } else { ")?;
    if tail != "None }" {
        return None;
    }
    let (left, floor) = condition.split_once(" > ")?;
    let (name, scale) = left.split_once(" as u64 * ")?;
    let (a, b) = scale.split_once(" / ")?;
    let holds = clock(name)?.checked_mul(rust_number(a)?)?.checked_div(rust_number(b)?)? > rust_number(floor)?;
    let hz = clock(&row_clock(row)?)?;
    Some(if holds { Some(hz) } else { None })
}

/// The bus ties of a row whose kernel facts write their bus rows out in full: every kernel bus that has pads
/// is a bus of the facts file and the reverse, by the block's number; the pads by their role and the selects
/// in their order. A kernel bus without pads (an emulator row's model) is no row of a facts file.
fn literal_bus_ties(board: &str, i2c: &[I2cFacts], spi: &[SpiFacts], k_i2c: &[LiteralBus], k_spi: &[LiteralBus]) -> Result<Vec<(String, u64, u64)>, String> {
    let pad = |bus: &LiteralBus, role: &str| bus.pads.iter().find(|(_, r)| r == role).map(|(line, _)| *line);
    let mut ties = Vec::new();
    for (block, _, _, _) in i2c {
        if !k_i2c.iter().any(|b| b.number == *block && !b.pads.is_empty()) {
            return Err(format!("{board}: the facts file lists I2C block {block}; the kernel's facts carry no such bus with pads"));
        }
    }
    for bus in k_i2c.iter().filter(|b| !b.pads.is_empty()) {
        let n = bus.number;
        let Some((_, sda, scl, _)) = i2c.iter().find(|(block, _, _, _)| *block == n) else {
            return Err(format!("{board}: the kernel's facts carry I2C{n} with its pads and the facts file lists no such bus"));
        };
        let (Some(data), Some(clock)) = (pad(bus, "I2cData"), pad(bus, "I2cClock")) else {
            return Err(format!("{board}: the kernel's I2C{n} row names no data pad or no clock pad"));
        };
        ties.push((format!("I2C{n}'s data pad"), *sda, data));
        ties.push((format!("I2C{n}'s clock pad"), *scl, clock));
    }
    for (block, _, _, _, _, _, _) in spi {
        if !k_spi.iter().any(|b| b.number == *block && !b.pads.is_empty()) {
            return Err(format!("{board}: the facts file lists SPI block {block}; the kernel's facts carry no such bus with pads"));
        }
    }
    for bus in k_spi.iter().filter(|b| !b.pads.is_empty()) {
        let n = bus.number;
        let Some((_, sck, mosi, miso, cs, _, _)) = spi.iter().find(|(block, _, _, _, _, _, _)| *block == n) else {
            return Err(format!("{board}: the kernel's facts carry SPI{n} with its pads and the facts file lists no such bus"));
        };
        let (Some(clock), Some(out), Some(input)) = (pad(bus, "SpiClock"), pad(bus, "SpiOut"), pad(bus, "SpiIn")) else {
            return Err(format!("{board}: the kernel's SPI{n} row names no clock pad, no out pad or no in pad"));
        };
        if cs.len() != bus.selects.len() {
            return Err(format!("{board}: the facts file lists {} chip selects for SPI{n}; the kernel's row carries {}", cs.len(), bus.selects.len()));
        }
        ties.push((format!("SPI{n}'s clock pad"), *sck, clock));
        ties.push((format!("SPI{n}'s out pad"), *mosi, out));
        ties.push((format!("SPI{n}'s in pad"), *miso, input));
        for (k, c) in cs.iter().enumerate() {
            ties.push((format!("SPI{n}'s select {k}"), *c, bus.selects[k]));
        }
    }
    Ok(ties)
}

/// The number after `key` in a struct row (`rate_hz: 100_000`), underscores allowed.
fn field_number(row: &str, key: &str) -> Option<u64> {
    let i = row.find(key)? + key.len();
    let n: String = row[i..].trim_start().chars().take_while(|c| c.is_ascii_digit() || *c == '_').collect();
    rust_number(&n)
}

/// The one RHU request on the I2C node (the rate request): the shim's `#define BUSDEV_I2C_RATE 0x...u`
/// and the board class's private constant `BusRateRequest = 0x...` carry the same number,
/// read as text as the pads are; the number as the shim writes it.
/// The PWM unit's table size and the tables the shim gate's PWM rows tenants
/// answer with, as the two sources state them.
#[derive(Clone)]
struct PwmTable {
    channels: u64,
    tenants: Vec<(String, Vec<u64>)>,
}

/// Reads the unit's `#define PWMFS_CHANNELS_MAX` (src/shim/rhu_pwmfs.c) and the
/// tenants' tables (src/shim/gate/rhu_shimtest.c: every line of the form
/// `static const uint16_t pr_lines[] = { ... }; /* PWM_LINES of <row> */`).
fn read_pwm_table(repo: &Path) -> Result<PwmTable, String> {
    let unit = std::fs::read_to_string(repo.join("src").join("shim").join("rhu_pwmfs.c")).map_err(|e| format!("rhu_pwmfs.c: {e}"))?;
    let tenant = std::fs::read_to_string(repo.join("src").join("shim").join("gate").join("rhu_shimtest.c")).map_err(|e| format!("rhu_shimtest.c: {e}"))?;
    pwm_table_of(&unit, &tenant).ok_or("the PWM unit's '#define PWMFS_CHANNELS_MAX' or the PWM rows tenants' tables are not there as the tie reads them".to_string())
}

fn pwm_table_of(unit: &str, tenant: &str) -> Option<PwmTable> {
    let channels = unit.lines().find_map(|l| l.strip_prefix("#define PWMFS_CHANNELS_MAX "))?.trim().parse().ok()?;
    let mut tenants = Vec::new();
    for l in tenant.lines() {
        let Some(rest) = l.strip_prefix("static const uint16_t pr_lines[] = { ") else {
            continue;
        };
        let (list, tail) = rest.split_once(" }; /* PWM_LINES of ")?;
        let row = tail.strip_suffix(" */")?;
        let lines: Option<Vec<u64>> = list.split(", ").map(|n| n.parse().ok()).collect();
        tenants.push((row.to_string(), lines?));
    }
    if tenants.is_empty() {
        None
    } else {
        Some(PwmTable { channels, tenants })
    }
}

/// The PWM table tie's first fault, by the row's name: a row whose kernel
/// table states more PWM lines than the unit's table holds, a tenant's row
/// that the board table does not carry, or a tenant's table that is not the
/// kernel's for its row.
fn pwm_table_fault(rows: &[BoardFactsRow], t: &PwmTable) -> Option<String> {
    for r in rows {
        if r.kernel_pwm_lines.len() as u64 > t.channels {
            return Some(format!("{}: the kernel states {} PWM lines, more PWM lines than the unit's table of {} channels holds (src/shim/rhu_pwmfs.c, PWMFS_CHANNELS_MAX)", r.board, r.kernel_pwm_lines.len(), t.channels));
        }
    }
    for (row, lines) in &t.tenants {
        let Some(r) = rows.iter().find(|r| r.board == *row) else {
            return Some(format!("{row}: the shim gate's PWM rows tenant names a row that the board table does not carry"));
        };
        if r.kernel_pwm_lines != *lines {
            return Some(format!("{row}: the shim gate's PWM rows tenant answers with the lines {} != the kernel's PWM_LINES {}", number_list(lines), number_list(&r.kernel_pwm_lines)));
        }
    }
    None
}

fn request_tie(repo: &Path) -> Result<String, String> {
    let shim = std::fs::read_to_string(repo.join("src").join("shim").join("rhu_busdev.c")).map_err(|e| format!("rhu_busdev.c: {e}"))?;
    let class = std::fs::read_to_string(repo.join("src").join("sdk").join("Lamella.Hardware").join("TargetBoard.cs")).map_err(|e| format!("TargetBoard.cs: {e}"))?;
    let shim_n = hex_after(&shim, "#define BUSDEV_I2C_RATE 0x").ok_or("rhu_busdev.c: no '#define BUSDEV_I2C_RATE 0x...'")?;
    let class_n = hex_after(&class, "BusRateRequest = 0x").ok_or("TargetBoard.cs: no 'BusRateRequest = 0x...'")?;
    if shim_n != class_n {
        return Err(format!("the rate request's number differs: the shim's define 0x{shim_n}, the board class's constant 0x{class_n}"));
    }
    Ok(format!("0x{shim_n}"))
}

fn hex_after(text: &str, key: &str) -> Option<String> {
    let i = text.find(key)? + key.len();
    let n: String = text[i..].chars().take_while(|c| c.is_ascii_hexdigit()).map(|c| c.to_ascii_lowercase()).collect();
    if n.is_empty() {
        None
    } else {
        Some(n)
    }
}

/// The board class's six lines as the row's facts file predicts them, in order (the pin's PWM numbers third, the serial
/// ports fifth: the uart rows with the row's default channel words, which the gate's program leaves unset).
fn board_class_markers(repo: &Path, board: &str) -> Result<Vec<String>, String> {
    let f = read_facts_file(&repo.join("src").join("sdk").join("Lamella.Rhu.Sdk").join("boards").join(format!("{board}.json")))?;
    let list = |v: Vec<u64>| if v.is_empty() { "none".to_string() } else { v.iter().map(|n| n.to_string()).collect::<Vec<_>>().join(", ") };
    let i2c: Vec<u64> = f.i2c.iter().map(|b| b.0).collect();
    let spi: Vec<u64> = f.spi.iter().map(|b| b.0).collect();
    let uart: Vec<u64> = f.uart.iter().map(|u| u.0).collect();
    let who = format!("{} {} ({})", f.maker, f.product, f.name);
    let label0 = match f.labels.as_str() {
        "gp" => "GP0".to_string(),
        "port" => "PA0".to_string(),
        _ => "none".to_string(),
    };
    let third = if i2c.contains(&0) {
        "TBHELLO: I2C bus 0 at 100000 Hz: device created".to_string()
    } else if i2c.is_empty() {
        format!("TBHELLO: NotSupportedException: {who} has no I2C bus 0; the row carries no I2C bus")
    } else {
        format!("TBHELLO: NotSupportedException: {who} has no I2C bus 0; its buses: {}", list(i2c.clone()))
    };
    let mut ports = String::from("TBHELLO: serial ports:");
    if f.uart.is_empty() {
        ports.push_str(" none");
    }
    for (i, (block, tx, rx)) in f.uart.iter().enumerate() {
        ports.push_str(if i == 0 { " " } else { ", " });
        let leaf = if f.uart_lowpower.contains(block) { "ttyLP" } else { "ttyS" };
        ports.push_str(&format!("/dev/{leaf}{block} (UART{block}"));
        match (tx, rx) {
            (Some(t), Some(r)) => ports.push_str(&format!(", TX pin {t}, RX pin {r}")),
            _ => ports.push_str(", no lines"),
        }
        let word = format!("uart{block}");
        if f.channels.1 == word {
            ports.push_str(", the console");
        }
        if f.channels.0 == word {
            ports.push_str(", the log");
        }
        ports.push(')');
    }
    Ok(vec![
        format!("TBHELLO: {who}, {} at {} Hz, {} pins, I2C {}, SPI {}, UART {}", f.chip, f.clock_hz, f.pins, list(i2c), list(spi), list(uart)),
        format!("TBHELLO: pins {}; pin 0: labels {label0}", f.pins),
        format!("TBHELLO: pin 0: PWM {}", if f.pwm_lines.contains(&0) { format!("chip {}, channel 0", f.pwm_chip) } else { "none".to_string() }),
        third,
        ports,
        "TBHELLO: end".to_string(),
    ])
}

/// The board table's stack facts beside the tree's constants: a row's
/// kernel file is its name without the `arm-` prefix, hyphens as
/// underscores, under src/kernel/src/board (a file that carries no pool
/// of its own re-exports its chip module's, and the pool is read there);
/// its hosting-layer block names the same name in MGC_ROW_BOARD.
fn read_stack_facts(repo: &Path) -> Result<StackFacts, String> {
    let sdk = repo.join("src").join("sdk").join("Lamella.Rhu.Sdk").join("Sdk");
    let boards = std::fs::read_to_string(sdk.join("boards.props")).map_err(|e| format!("boards.props: {e}"))?;
    let targets = std::fs::read_to_string(sdk.join("Sdk.targets")).map_err(|e| format!("Sdk.targets: {e}"))?;
    let host = std::fs::read_to_string(repo.join("src").join("gc").join("host").join("mgc_board_rows.h")).map_err(|e| format!("mgc_board_rows.h: {e}"))?;
    let header = std::fs::read_to_string(repo.join("src").join("kernel").join("include").join("rhu.h")).map_err(|e| format!("rhu.h: {e}"))?;
    // The kernel's own threads in a row's pool, each stack read as text where the kernel states
    // it: the idle thread's spawn and, per row below, the stack of the thread that runs Main.
    // The network thread's and the card watcher's stacks are not the pool's: the scheduler's
    // array carries them above the row's pool, and the tie reads that the text says so.
    let ksrc = repo.join("src").join("kernel");
    let sched = std::fs::read_to_string(ksrc.join("src").join("sched.rs")).map_err(|e| format!("sched.rs: {e}"))?;
    let idle_stack = kernel_idle_stack(&sched).ok_or("sched.rs: the idle thread's spawn is not there as read")?;
    if !kernel_service_stacks_above_pool(&sched) {
        return Err("sched.rs: the pool's array is not the row's pool with the service threads' stacks above it, as read".to_string());
    }
    let mut rows = Vec::new();
    for (i, _) in boards.match_indices("<LamellaTargetBoardRow Include=\"") {
        let end = boards[i..].find("/>").ok_or("a board row without its end")? + i;
        let row = &boards[i..end];
        let board = row_attr_text(row, "Include").ok_or("a board row without a name")?;
        let pool = row_attr(row, "StackPool").ok_or(format!("{board}: no StackPool"))?;
        let default = row_attr(row, "StackDefault").ok_or(format!("{board}: no StackDefault"))?;
        let clamp = row_attr(row, "StackClamp").ok_or(format!("{board}: no StackClamp"))?;
        let kernel_own = row_attr(row, "StackKernel").ok_or(format!("{board}: no StackKernel"))?;
        let name = board.strip_prefix("arm-").unwrap_or(&board).to_string();
        let file = repo.join("src").join("kernel").join("src").join("board").join(format!("{}.rs", name.replace('-', "_")));
        let ktext = std::fs::read_to_string(&file).map_err(|e| format!("{}: {e}", file.display()))?;
        let kernel_pool = match kernel_pool_bytes(&ktext) {
            Some(v) => v,
            // A board module that carries no pool of its own re-exports its
            // chip module's (`pub use super::<chip>::*;`): the pool is read
            // there, and a module with neither is refused.
            None => {
                let chip = ktext
                    .lines()
                    .find_map(|l| l.trim().strip_prefix("pub use super::").and_then(|r| r.strip_suffix("::*;")))
                    .ok_or(format!("{}: no STACK_POOL_BYTES and no chip module re-export", file.display()))?;
                let cfile = repo.join("src").join("kernel").join("src").join("board").join(format!("{chip}.rs"));
                let ctext = std::fs::read_to_string(&cfile).map_err(|e| format!("{}: {e}", cfile.display()))?;
                kernel_pool_bytes(&ctext).ok_or(format!("{}: no STACK_POOL_BYTES (the chip module {} re-exports)", cfile.display(), file.display()))?
            }
        };
        let block = host_row_block(&host, &name).ok_or(format!("mgc_board_rows.h: no row block names {name}"))?;
        let host_default = c_define(block, "MGC_ROW_STACK_DEFAULT").ok_or(format!("mgc_board_rows.h: the {name} block has no MGC_ROW_STACK_DEFAULT"))?;
        let host_clamp = c_define(block, "MGC_ROW_STACK_CLAMP").ok_or(format!("mgc_board_rows.h: the {name} block has no MGC_ROW_STACK_CLAMP"))?;
        // The stack of the thread that runs a hosted program's Main: the board module's
        // constant, or the chip module's it re-exports, as the pool is read above.
        let kmain = match kernel_const(&ktext, "KMAIN_STACK_BYTES") {
            Some(v) => v,
            None => {
                let chip = ktext
                    .lines()
                    .find_map(|l| l.trim().strip_prefix("pub use super::").and_then(|r| r.strip_suffix("::*;")))
                    .ok_or(format!("{}: no KMAIN_STACK_BYTES and no chip module re-export", file.display()))?;
                let cfile = repo.join("src").join("kernel").join("src").join("board").join(format!("{chip}.rs"));
                let ctext = std::fs::read_to_string(&cfile).map_err(|e| format!("{}: {e}", cfile.display()))?;
                kernel_const(&ctext, "KMAIN_STACK_BYTES").ok_or(format!("{}: no KMAIN_STACK_BYTES (the chip module {} re-exports)", cfile.display(), file.display()))?
            }
        };
        rows.push(StackRow { board, pool, default, clamp, kernel_pool, host_default, host_clamp, kernel_own, kernel_threads: [idle_stack, kmain] });
    }
    let sdk_min = xml_value(&targets, "RhuThreadStackMin").and_then(|v| v.parse().ok()).ok_or("Sdk.targets: no RhuThreadStackMin")?;
    let kernel_min = c_define(&header, "RHU_THREAD_STACK_MIN").ok_or("rhu.h: no RHU_THREAD_STACK_MIN")?;
    Ok(StackFacts { rows, sdk_min, kernel_min })
}

/// The tie: every row's three facts equal the tree's, the default fits the
/// budget the targets enforce, the minimum equal to the header's.
fn stack_facts_fault(f: &StackFacts) -> Option<String> {
    if f.rows.is_empty() {
        return Some("the board table has no rows".to_string());
    }
    for r in &f.rows {
        if r.pool != r.kernel_pool {
            return Some(format!("{}: StackPool {} != the kernel's STACK_POOL_BYTES {}", r.board, r.pool, r.kernel_pool));
        }
        if r.default != r.host_default {
            return Some(format!("{}: StackDefault {} != the hosting layer's MGC_ROW_STACK_DEFAULT {}", r.board, r.default, r.host_default));
        }
        if r.clamp != r.host_clamp {
            return Some(format!("{}: StackClamp {} != the hosting layer's MGC_ROW_STACK_CLAMP {}", r.board, r.clamp, r.host_clamp));
        }
        if r.default > r.clamp || 3 * r.default + 1024 > r.pool {
            return Some(format!("{}: the default {} does not fit the budget (the clamp {}, the pool {})", r.board, r.default, r.clamp, r.pool));
        }
        let own: u64 = r.kernel_threads.iter().sum();
        if r.kernel_own != own {
            return Some(format!(
                "{}: StackKernel {} != the kernel's own threads' {} (the idle thread {}, the thread that runs Main {})",
                r.board, r.kernel_own, own, r.kernel_threads[0], r.kernel_threads[1]
            ));
        }
    }
    if f.sdk_min != f.kernel_min {
        return Some(format!("the targets' RhuThreadStackMin {} != RHU_THREAD_STACK_MIN {}", f.sdk_min, f.kernel_min));
    }
    None
}

/// THE PARITY LEG: the gate's own program published through the SDK
/// package from the local feed in the Linux environment for the publish, and its object, its
/// surgery object, its merged object and its image tied byte for byte to
/// the frozen tier and to this runner's own link of the tier's merged
/// object; the board table's stack facts tied to the tree; the stack
/// knob's image booted with the row's default moved; a real git checkout
/// at another path tying the record; then the SDK's refusals exercised
/// through the real tool on doctored inputs. A maintainer's leg beside
/// `rederive`: it needs the
/// Linux environment for the publish (the pinned .NET SDK, the archive sets and the LLVM
/// release on the Linux side), NOT RUN and exit 1 without it -- never a
/// pass. The exact tie of the object needs the SDK's path map (the same
/// project maps to the same string at any directory), so the leg refuses
/// a Linux environment whose published object does not carry the frozen object's mapped
/// path.
pub fn sdk_parity(repo: &Path, w: &'static RtWorld) -> ! {
    let facts = match rustc_facts() {
        Ok(f) => f,
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
    let Some((prefix, root)) = resolve_publish(repo) else {
        println!(
            "{}: NOT RUN (no Linux environment for the publish resolved: set RHU_PUBLISH to the command prefix that runs a script in the Linux environment for the publish and RHU_PUBLISH_ROOT to this checkout's root there, or write tools/toolchains/publish.local with those two lines)",
            w.sdk_display
        );
        exit(1);
    };
    if !repo.join("tools").join("rt-publish").join("sdk-parity.sh").is_file() {
        println!(
            "{}: NOT RUN (the Linux environment for the publish is named but this checkout carries no tools/rt-publish/sdk-parity.sh, the maintainers' script that drives the parity leg there)",
            w.sdk_display
        );
        exit(1);
    }
    let words = match command_words(&prefix) {
        Ok(v) if !v.is_empty() => v,
        Ok(_) => {
            println!("{}: NOT RUN (the publish command prefix is empty)", w.sdk_display);
            exit(1);
        }
        Err(e) => {
            println!("{}: NOT RUN ({e})", w.sdk_display);
            exit(1);
        }
    };
    let mut rt = open(repo, w, w.sdk_gate, w.sdk_display, w.sdk_results);
    rt.results.emit(&format!(
        "{{\"schema\":1,\"type\":\"run\",\"gate\":\"{}\",\"run\":\"{}-{}\",\"host\":\"host-native\",\"runner\":\"{RUNNER}\",\"tools\":{{\"rustc\":\"{}\",\"ld\":\"{}\"}}}}",
        w.sdk_gate,
        w.sdk_gate,
        std::process::id(),
        facts.version,
        ld.version
    ));
    rt.verify_frozen();

    // The staticlib as the gate builds it: the SDK package is packed with it.
    let case = "rt.sdk-parity-object";
    let log = rt.out.join("sdk-parity-staticlib-build.log");
    match rt.build_lib(&log) {
        Ok(true) => {}
        Ok(false) => {
            echo_file(&log, "");
            rt.fail(case, "kernel staticlib build error");
        }
        Err(e) => rt.fail(case, &e),
    }

    // The external RAM init mode's row: its kernel per mode the SDK's withheld
    // table leaves offered (the row's default and its lazy archives), built once
    // under a directory both worlds share and packed beside the leg's own
    // kernel. A withheld mode's kernel is neither built nor packed: its
    // publish below must refuse with the table's words.
    let withheld = match ram_init_withheld(repo) {
        Ok(v) => v,
        Err(e) => rt.fail(case, &e),
    };
    let mut ram_init_kernels: Vec<String> = Vec::new();
    for (mode, feature, _) in RAM_INIT_MODES {
        if withheld.iter().any(|v| v == mode) {
            println!("  the {mode} init mode is withheld by the SDK's withheld table: the row's kernel for it is neither built nor packed");
            continue;
        }
        let dir = repo.join(RAM_INIT_DIR.replace('/', std::path::MAIN_SEPARATOR_STR)).join(mode);
        let _ = std::fs::create_dir_all(&dir);
        let feats = if feature.is_empty() { RAM_INIT_FEATURES.to_string() } else { format!("{RAM_INIT_FEATURES},{feature}") };
        let log = rt.out.join(format!("sdk-parity-ram-init-{mode}-staticlib-build.log"));
        println!("== build kernel staticlib {RAM_INIT_ROW} in the {mode} init mode (--lib --target {RAM_INIT_TARGET} --no-default-features --features {feats}) ==");
        match cargo_build_kernel_at(repo, &dir, &["--lib", "--target", RAM_INIT_TARGET, "--no-default-features", "--features", feats.as_str()], &log) {
            Ok(true) => {}
            Ok(false) => {
                echo_file(&log, "");
                rt.fail(case, &format!("the {RAM_INIT_ROW} row's kernel staticlib build error in the {mode} init mode"));
            }
            Err(e) => rt.fail(case, &e),
        }
        let lib_rel = format!("{RAM_INIT_DIR}/{mode}/{RAM_INIT_TARGET}/release/librhu_kernel.a");
        if !repo.join(lib_rel.replace('/', std::path::MAIN_SEPARATOR_STR)).is_file() {
            rt.fail(case, &format!("no staticlib at {lib_rel} after the build"));
        }
        let name = if feature.is_empty() { RAM_INIT_ROW.to_string() } else { format!("{RAM_INIT_ROW}/{feature}") };
        ram_init_kernels.push(format!("{name}={root}/{lib_rel}"));
    }

    // The external-flash row's kernels: the row's payload staticlib, built
    // with the network, and the second set's default, built without it, each
    // built once under a directory both worlds share and packed beside the
    // leg's own kernel, so the leg publishes the row under the program's two
    // homes and with either kernel.
    for (name, dir_rel, features, tag) in [
        (XIP_ROW.to_string(), XIP_DIR, XIP_FEATURES, "external-flash"),
        (format!("{XIP_ROW}/no-network"), XIP_PLAIN_DIR, XIP_PLAIN_FEATURES, "external-flash-no-network"),
    ] {
        let dir = repo.join(dir_rel.replace('/', std::path::MAIN_SEPARATOR_STR));
        let _ = std::fs::create_dir_all(&dir);
        let log = rt.out.join(format!("sdk-parity-{tag}-staticlib-build.log"));
        println!("== build kernel staticlib {name} (--lib --target {XIP_TARGET} --no-default-features --features {features}) ==");
        match cargo_build_kernel_at(repo, &dir, &["--lib", "--target", XIP_TARGET, "--no-default-features", "--features", features], &log) {
            Ok(true) => {}
            Ok(false) => {
                echo_file(&log, "");
                rt.fail(case, &format!("the kernel staticlib {name} build error"));
            }
            Err(e) => rt.fail(case, &e),
        }
        let lib_rel = format!("{dir_rel}/{XIP_TARGET}/release/librhu_kernel.a");
        if !repo.join(lib_rel.replace('/', std::path::MAIN_SEPARATOR_STR)).is_file() {
            rt.fail(case, &format!("no staticlib at {lib_rel} after the build"));
        }
        ram_init_kernels.push(format!("{name}={root}/{lib_rel}"));
    }

    // The Debug configuration's row: the Raspberry Pi Pico 2's payload
    // kernel built once under a directory both worlds share and packed
    // beside the leg's own kernel, so the leg publishes the row in the
    // Debug configuration and compiled without the optimization as a whole.
    {
        let dir = repo.join(DEBUG_DIR.replace('/', std::path::MAIN_SEPARATOR_STR));
        let _ = std::fs::create_dir_all(&dir);
        let log = rt.out.join("sdk-parity-debug-staticlib-build.log");
        println!("== build kernel staticlib {DEBUG_ROW} (--lib --target {DEBUG_TARGET} --no-default-features --features {DEBUG_FEATURES}) ==");
        match cargo_build_kernel_at(repo, &dir, &["--lib", "--target", DEBUG_TARGET, "--no-default-features", "--features", DEBUG_FEATURES], &log) {
            Ok(true) => {}
            Ok(false) => {
                echo_file(&log, "");
                rt.fail(case, &format!("the {DEBUG_ROW} row's kernel staticlib build error"));
            }
            Err(e) => rt.fail(case, &e),
        }
        let lib_rel = format!("{DEBUG_DIR}/{DEBUG_TARGET}/release/librhu_kernel.a");
        if !repo.join(lib_rel.replace('/', std::path::MAIN_SEPARATOR_STR)).is_file() {
            rt.fail(case, &format!("no staticlib at {lib_rel} after the build"));
        }
        ram_init_kernels.push(format!("{DEBUG_ROW}={root}/{lib_rel}"));
    }

    // The publish-side script: the pack, the publish through the SDK, the
    // wrapper's assertions, the preprocessed project and the controls.
    let out_rel = format!("src/kernel/target/{}/sdk-parity", w.out_dir);
    let out = rt.out.join("sdk-parity");
    if std::fs::create_dir_all(&out).is_err() {
        rt.fail(case, &format!("cannot create {}", out.display()));
    }
    let script = format!("{root}/tools/rt-publish/sdk-parity.sh");
    let lib_there = format!("{root}/{}", w.lib);
    let out_there = format!("{root}/{out_rel}");
    let mut args: Vec<&str> = words[1..].iter().map(String::as_str).collect();
    args.push(&script);
    args.push(w.sdk_board);
    args.push(&lib_there);
    args.push(&out_there);
    for k in &ram_init_kernels {
        args.push(k);
    }
    println!("== the Linux environment for the publish: {} {} {} {} {} {} ==", words.join(" "), script, w.sdk_board, lib_there, out_there, ram_init_kernels.join(" "));
    let script_out = out.join("script.stdout");
    let script_err = out.join("script.stderr");
    let outcome = run_captured(Path::new(&words[0]), &args, &script_out, &script_err, Duration::from_secs(2400));
    let script_text = std::fs::read_to_string(&script_out).unwrap_or_default();
    for l in script_text.lines() {
        println!("  | {}", l.trim_end_matches('\r'));
    }
    match outcome {
        Ok(o) if o.exit == 0 => {}
        Ok(o) => {
            echo_file(&script_err, "  | ");
            rt.fail(case, &format!("the publish-side script failed (exit {}, timed out {})", o.exit, o.timed_out));
        }
        Err(e) => rt.fail(case, &format!("the publish-side script did not run: {e}")),
    }

    // 1. the object, byte for byte, after the path guard.
    println!("== case {case}: the object published through the SDK against the frozen program object ==");
    let fresh_obj = std::fs::read(out.join("rthello.o")).unwrap_or_default();
    let frozen_obj = std::fs::read(rt.frozen(PAYLOAD)).unwrap_or_default();
    if fresh_obj.is_empty() {
        rt.fail(case, "the publish left no object under sdk-parity/");
    }
    let frozen_path = embedded_project_path(&frozen_obj).unwrap_or_default();
    if frozen_path.is_empty() {
        rt.fail(case, "the frozen program object embeds no project path (the guard has nothing to read)");
    }
    let carries_record_path = |obj: &[u8]| obj.windows(frozen_path.len()).any(|w| w == frozen_path.as_bytes());
    if !carries_record_path(&fresh_obj) {
        println!(
            "{}: REFUSED -- the object published in the Linux environment for the publish does not carry the record's project path '{frozen_path}'; the exact tie needs the same project at the same path, and the tie apart from the embedded path strings is a separate instrument",
            w.sdk_display
        );
        exit(1);
    }
    println!("  the object carries the record's project path: {frozen_path}");
    let want = rt.record_sha(PAYLOAD);
    let got = sha256_hex(&fresh_obj);
    if got != want {
        rt.fail(case, &format!("the object's sha256 {got} != the frozen {want} -- the compiler's output moved (the response file, the pack or the SDK's posture)"));
    }
    println!("  the object byte-identical to {}/{PAYLOAD} ({}...; {} bytes)", rt.w.program_tier, &want[..16], fresh_obj.len());
    rt.pass(case, "positive", "", &format!("{{\"object_sha\":\"{want}\",\"bytes\":{}}}", fresh_obj.len()));
    println!("  PASS");

    // 2. the surgery object.
    let case = "rt.sdk-parity-surgery";
    println!("== case {case}: the startup member printed out of the shipped archive and its cookie section renamed, against the frozen surgery object ==");
    let got = sha256_file(&out.join("startup-cookie-copied.o")).unwrap_or_default();
    let want = rt.record_sha(SURGERY);
    if got != want {
        rt.fail(case, &format!("the surgery object's sha256 {got} != the frozen {want} -- the member, the archive or the copier moved"));
    }
    println!("  the surgery object byte-identical to {}/{SURGERY} ({}...)", rt.w.tier, &want[..16]);
    rt.pass(case, "positive", "", &format!("{{\"surgery_sha\":\"{want}\"}}"));
    println!("  PASS");

    // 3. the merged object, whole.
    let case = "rt.sdk-parity-merge";
    println!("== case {case}: the merge the SDK made with the tools package's linker and the task assembly's attributes update, against the frozen merged object ==");
    let got = sha256_file(&out.join("rthello-merged.o")).unwrap_or_default();
    let want = rt.record_sha(MERGED);
    if got != want {
        rt.fail(case, &format!("the merged object's sha256 {got} != the frozen {want} (whole) -- the merge moved (the linker, the archives, the surgery object or the attributes update)"));
    }
    println!("  the merged object byte-identical to {}/{MERGED} ({}...; whole)", rt.w.tier, &want[..16]);
    rt.pass(case, "positive", "", &format!("{{\"merged_sha\":\"{want}\"}}"));
    println!("  PASS");

    // 4. the image, against this runner's own link of the tier's merged object.
    let case = "rt.sdk-parity-link";
    println!("== case {case}: the image the SDK linked against this runner's link of the frozen merged object on the gate's line ==");
    let record_rel = format!("{out_rel}/rt-link-of-record.elf");
    let map_rel = format!("{out_rel}/rt-link-of-record.map");
    let mut largs: Vec<String> = vec!["-T".to_string(), w.map.to_string(), "--gc-sections".to_string(), format!("-Map={map_rel}")];
    largs.extend(RT_LINK_WORDS.iter().map(|s| s.to_string()));
    largs.push(format!("{}/{MERGED}", w.tier));
    largs.push(SDK_LOG_LEVEL_WORD.to_string());
    // The bound the build states to the runtime: the SDK's link line carries it as an absolute symbol on every
    // link, so the link of record carries the same word, the number this runner's own.
    let bound_row = match read_stack_facts(repo) {
        Ok(f) => match f.rows.into_iter().find(|r| r.board == w.sdk_board) {
            Some(r) => r,
            None => rt.fail(case, &format!("the board table has no row {}", w.sdk_board)),
        },
        Err(e) => rt.fail(case, &e),
    };
    largs.push(format!("--defsym=__rhu_threadpool_max_threads={}", pool_bound(&bound_row).0));
    largs.extend(w.shim_objects.iter().map(|s| s.to_string()));
    largs.extend(w.gc_objects.iter().map(|s| s.to_string()));
    largs.push(format!("{}/{LIBC}", w.tier));
    largs.push(w.lib.to_string());
    largs.push("-o".to_string());
    largs.push(record_rel.clone());
    let ld_out = out.join("rt-link-of-record.ld.stdout");
    let ld_log = out.join("rt-link-of-record.ld.stderr");
    match run_tool_bounded(repo, &ld, &largs, &ld_out, &ld_log) {
        Ok(true) => {}
        Ok(false) => {
            echo_file(&ld_log, "  | ");
            rt.fail(case, "the link of record did not link");
        }
        Err(e) => rt.fail(case, &e),
    }
    let record_elf = repo.join(record_rel.replace('/', std::path::MAIN_SEPARATOR_STR));
    let fresh_elf = out.join("rthello.elf");
    let bytes = std::fs::metadata(&fresh_elf).map(|m| m.len()).unwrap_or(0);
    let file_sha = sha256_file(&fresh_elf).unwrap_or_default();
    // The shipped kernels are stripped of their debug information and bitcode by the pack, so the SDK's image carries
    // no kernel DWARF while this runner's link of record (the whole unstripped archive) does: the two are compared on
    // their loadable bytes, each after the same strip plus its comment section (the pack's gate's comparison).
    let objcopy = match llvm_tool(&facts, "llvm-objcopy") {
        Ok(p) => p,
        Err(e) => rt.fail(case, &e),
    };
    let record_loadable = out.join("rt-link-of-record.loadable.elf");
    let fresh_loadable = out.join("rthello.loadable.elf");
    for (from, to, label) in [(&record_elf, &record_loadable, "record"), (&fresh_elf, &fresh_loadable, "fresh")] {
        let from_arg = from.to_string_lossy().into_owned();
        let to_arg = to.to_string_lossy().into_owned();
        let oargs = ["--strip-debug", "-R", ".llvmbc", "-R", ".llvmcmd", "-R", ".comment", from_arg.as_str(), to_arg.as_str()];
        match run_captured(&objcopy, &oargs, &out.join(format!("link-{label}.objcopy.stdout")), &out.join(format!("link-{label}.objcopy.stderr")), Duration::from_secs(120)) {
            Ok(o) if o.exit == 0 => {}
            Ok(o) => rt.fail(case, &format!("llvm-objcopy did not strip the {label} image for the comparison (exit {})", o.exit)),
            Err(e) => rt.fail(case, &e),
        }
    }
    let want = sha256_file(&record_loadable).unwrap_or_default();
    let got = sha256_file(&fresh_loadable).unwrap_or_default();
    if got.is_empty() || want.is_empty() || file_sha.is_empty() {
        rt.fail(case, "the image or the link of record is missing");
    }
    if got != want {
        rt.fail(case, &format!("the image's loadable bytes {got} != the link of record's {want} -- the SDK's link is not the gate's"));
    }
    println!("  the image's loadable bytes tie this runner's link of record ({}...; the image {} bytes, sha256 {}..., the kernel's debug information stripped by the pack; this runner's linker {})", &want[..16], bytes, &file_sha[..16], ld.version);
    // The link line's order as the shipped targets state it, read from the preprocessed project the publish-side
    // script banked (the build package's targets as restored): the row's kernel after the tier objects, then the
    // class's core archive, last before the output. The order moves the loadable bytes (measured at the pack),
    // so the tie above holds for this order alone.
    let pp = std::fs::read_to_string(out.join("rthello.pp.xml")).unwrap_or_default();
    let order = "&quot;$(RhuBoardRoot)$(RhuKernel)&quot; &quot;$(RhuRuntimeRoot)$(RhuCore)&quot; -o &quot;$(_RhuElf)&quot;";
    let seen = pp.matches(order).count();
    if seen != 1 {
        rt.fail(case, &format!("the preprocessed project states the link's order (the row's kernel, then the class's core archive, last before the output) {seen} times, not once"));
    }
    println!("  the link line names the row's kernel before the class's core archive, the core archive last before the output (the preprocessed project, once)");
    rt.pass(case, "positive", "", &format!("{{\"image_loadable_sha\":\"{want}\",\"image_sha\":\"{file_sha}\",\"bytes\":{bytes}}}"));
    println!("  PASS");

    // 5. the controls: the SDK's refusals through the real tool.
    let controls = std::fs::read_to_string(out.join("controls.txt")).unwrap_or_default();
    let facts_rel = format!("boards/{}.json (the board package)", w.sdk_board);
    let release = match sdk_release_version(repo) {
        Ok(v) => v,
        Err(e) => rt.fail("rt.sdk-parity-control-stamped-version-doctored", &e),
    };
    let doctored_board = format!("Lamella.Rhu.Board.{} at {release}", w.sdk_board);
    let bound_over = (pool_bound(&bound_row).0 + 1).to_string();
    let pool_max_words = pool_max_refusal(&bound_row, &bound_over);
    let pool_min_words = pool_min_refusal(&bound_row, &bound_over);
    let table: &[(&str, &str, &str, &[&str], &str)] = &[
        ("frozen", "frozen-byte", "RHU4012", &["does not tie its record: ", " (the board package)"], "one byte of a frozen tier object changed"),
        ("board-missing", "board-missing", "RHU1001", &["LamellaTargetBoard is not set"], "no board named"),
        ("board-unknown", "board-unknown", "RHU1002", &["is not in the table"], "a board not in the table"),
        ("tool", "tool-byte", "RHU4012", &["does not tie its record: bin/llvm-objcopy (the tools package)"], "one byte of the object copier changed"),
        ("posture", "posture", "RHU3004", &["InvariantGlobalization"], "the fixed posture moved"),
        ("pack-root", "pack-root", "RHU3009", &["NetCoreTargetingPackRoot"], "the reference assemblies' pack root moved back to the .NET SDK's own folder"),
        ("tools-host", "tools-host", "RHU4001", &["no tools package for the host osx-x64"], "a host runtime identifier this release packs no tools for (osx-x64 given): the targets' one sentence naming the identifier found and the identifiers served, before anything is compiled"),
        ("stack-clamp", "stack-clamp", "RHU2021", &["above the row's clamp"], "a thread stack above the row's clamp"),
        ("stack-min", "stack-min", "RHU2020", &["below the kernel's minimum"], "a thread stack below the kernel's minimum"),
        ("stack-nan", "stack-nan", "RHU2019", &["not a whole number"], "a thread stack that is not a whole number"),
        ("stack-budget", "stack-budget", "RHU2025", &["fewer than three stacks"], "a thread stack the row's pool cannot hold three of (the package's table in the cache doctored to a 64 KiB pool)"),
        ("pool-timeout-nan", "pool-timeout-nan", "RHU2022", &["is not a whole number of milliseconds or -1"], "a thread pool idle-worker timeout that is not a whole number of milliseconds or -1"),
        ("pool-timeout-zero", "pool-timeout-zero", "RHU2023", &["no link word is the row's default"], "a thread pool idle-worker timeout of 0 (the link cannot carry it: no word is the row's default)"),
        ("pool-timeout-big", "pool-timeout-big", "RHU2024", &["is above 2147483647"], "a thread pool idle-worker timeout above the 32-bit range the runtime reads"),
        ("facts-missing", "facts-missing", "RHU1005", &["no board facts for"], "the board class's program with the row's facts file deleted from the package in the cache (the generation must refuse before anything is compiled)"),
        ("facts-byte", "facts-byte", "RHU4012", &["does not tie its record: ", facts_rel.as_str()], "the board class's program with one byte of the row's facts file changed in the package in the cache (the frozen record must refuse at the link)"),
        ("reflection-off", "reflection-off", "RHU2009", &["RhuReflection=off with the package Lamella.Hardware"], "the board class's program with the compiler's reflection data stripped (the class needs it: refused before anything is compiled, naming the property and the package)"),
        ("ram-init-value", "ram-init-value", "RHU2003", &["is not one of: boot, first-use, background"], "an external RAM init mode value outside the three"),
        ("ram-init-no-external", "ram-init-no-external", "RHU2011", &["has no external RAM"], "a lazy init mode on the Raspberry Pi Pico 2 row, which has no external RAM range"),
        ("ram-init-frozen", "ram-init-frozen", "RHU4012", &["does not tie its record: kernels/pimoroni-pico-plus-2/ram-init-first-use/librhu_kernel.a"], "one byte of the row's lazy init-mode archive changed in the package in the cache (the frozen record must refuse at the link)"),
        ("ram-init-withheld", "ram-init-withheld", "RHU2010", &["withheld in this release"], "the targets' withheld table in the package in the cache doctored with a row withholding the background value (the refusal before anything is compiled, with the row's words)"),
        ("managed-memory-value", "managed-memory-value", "RHU2004", &["is not one of: external, internal"], "a managed memory home outside the two words"),
        ("managed-memory-no-external", "managed-memory-no-external", "RHU2013", &["has no external RAM"], "the internal home on the Raspberry Pi Pico 2 row, which has no external RAM range"),
        ("program-memory-value", "program-memory-value", "RHU2005", &["is not one of: internal, external"], "a program home outside the two words"),
        ("program-memory-no-external-flash", "program-memory-no-external-flash", "RHU2014", &["has no memory-mapped external flash"], "the external home on the Raspberry Pi Pico 2 row, which has no memory-mapped external flash"),
        ("program-memory-with-internal-heap", "program-memory-with-internal-heap", "RHU2015", &["RhuProgramMemory=external with RhuManagedMemory=internal"], "the external home asked together with the internal managed memory on the STM32F769I-DISCO row (the external-flash map keeps the heap in the external RAM)"),
        ("analog-voltage-by-hand-no-room", "analog-voltage-by-hand-no-room", "RHU2016", &["=true in the project, but the board does not hold the library"], "the switch's program with the option set true by hand on the Raspberry Pi Pico 2 row, which does not hold the library (refused before anything is compiled)"),
        ("log-value", "log-value", "RHU2006", &["is not one of: silent, quiet, verbose"], "a kernel output level outside the three words"),
        ("channel-value", "channel-value", "RHU2008", &["is not one of: memory, probe, uart0, uart1, off"], "a console channel outside the five words"),
        ("channel-row", "channel-row", "RHU2017", &["is not a channel of pimoroni-pico-plus-2 (its channels: memory, off, uart0, uart1)"], "a log channel the Pico Plus 2 row cannot serve (the probe), refused with the row's set"),
        ("channel-row-console", "channel-row-console", "RHU2018", &["(its channels: uart0, uart1, memory, off)"], "a console channel this leg's emulator row cannot serve (the probe), refused with the row's set"),
        ("channel-table", "channel-table", "RHU2018", &["(its channels: uart0, memory, off)"], "a console channel the row serves but struck from the table's set in the package in the cache (the set is the table's, never a literal)"),
        ("contract-mismatch", "contract-mismatch", "RHU4002", &["is not this build package's"], "the shell's contract number set to 2 by a global property against the build package's 1: the two sides' numbers differ, refused before anything is compiled"),
        ("stamped-version-doctored", "stamped-version", "RHU4014", &["not at this build package's version 9.9.9-doctored", doctored_board.as_str()], "the version stamp in the build package's properties in the cache doctored to another string: the shell's resolved packages are not at it, refused by name before any root is read"),
        ("rid-other", "rid-other", "RHU3002", &["RuntimeIdentifier 'linux-arm64' is not linux-arm"], "RuntimeIdentifier given as a global property with another value than the posture's (amendment 3: the posture's own value passes), refused by name without a restore"),
        ("roots-board", "roots-board", "RHU4005", &["is Lamella.Rhu.Board.raspberrypi-pico-2, not Lamella.Rhu.Board."], "the stub shell's board item in the cache doctored to another board's package id: the shell's board and the project's differ, refused before any root is read"),
        ("roots-runtime", "roots-runtime", "RHU4007", &["not Lamella.Rhu.Runtime."], "the stub shell's runtime item in the cache doctored to another class's package id: the shell's class and the board's differ, refused before any root is read"),
        ("debug-unoptimized", "debug-unoptimized", "RHU5001", &["arena window under this board", "the image's data leaves this board less than the arena window its map guarantees", "this board's memory requires the framework's code compiled with it"], "the board class's program published for the Raspberry Pi Pico 2 row in the Debug configuration and compiled without the code optimization as a whole, by the one build property that asks for it: that image leaves the row no arena window at its floor, refused at the link by name with the linker's words and what the board's memory requires"),
        ("boardless-publish", "boardless-publish", "RHU1001", &["LamellaTargetBoard is not set"],"a publish with no board named, after the boardless build of the same copy: refused by name at the first step, the assembly from the build untouched, no image"),
        ("pool-bound-over-row", "pool-bound-over-row", "RHU2026", &[pool_max_words.as_str()], "the gate's project with .NET's own ThreadPoolMaxThreads one above the most workers this runner computes for the row: refused before anything is compiled, in the sentence this runner composes from the board table"),
        ("pool-min-over-bound", "pool-min-over-bound", "RHU2027", &[pool_min_words.as_str()], "the gate's project with .NET's own ThreadPoolMinThreads one above the bound: refused before anything is compiled (the runtime drops both bounds when the fewest is above the most)"),
        ("network-value", "network-value", "RHU2028", &["RhuNetwork='maybe' is not one of: on, off"], "a network setting outside the two words"),
        ("network-one-kernel", "network-one-kernel", "RHU2029", &["RhuNetwork=off, but the board 'raspberrypi-pico-2' has one kernel; the network is a choice on boards whose packages carry a kernel with it and a kernel without it"], "the network turned off on the Raspberry Pi Pico 2 row, whose packages carry one kernel"),
        ("network-with-internal-heap", "network-with-internal-heap", "RHU2030", &["RhuNetwork=on with RhuManagedMemory=internal: with the managed memory in the chip's own RAM this board's kernel is the one without the network; leave RhuNetwork unset or the managed memory external (its default)"], "the network asked together with the managed memory in the chip's own RAM on the STM32F769I-DISCO row"),
        ("network-no-room", "network-no-room", "RHU5001", &["the link printed a diagnostic or failed", "arena window under this board's GC-substrate floor (RHU_ARENA_MIN)", "; the image's data leaves this board less than the arena window its map guarantees: this image links the board's kernel with the network, which holds part of the chip's own memory: RhuNetwork=off links the kernel without it"], "the voltage switch's program published for the STM32F769I-DISCO row with no property: beside the kernel with the network its data leaves the row no arena window at its floor, refused at the link by name with the setting that takes the other kernel"),
        ("network-kernel-internal", "network-kernel-internal", "RHU5001", &["arena window under this board's GC-substrate floor (RHU_ARENA_MIN)", "; the image's data leaves this board less than the arena window its map guarantees: "], "the board table in the package in the cache doctored so that the second kernel set's default names the row's kernel with the network, and the gate's project published with the managed memory in the chip's own RAM: that kernel leaves the internal map no arena window at its floor, refused at the link by name"),
        ("program-memory-internal-overflow", "program-memory-internal-overflow", "RHU5001", &["will not fit in region", "; the image is larger than a memory region of this board's map: RhuProgramMemory=internal places the program in the chip's own flash; without it the program lies in this board's external flash"], "the network suite published for the STM32F769I-DISCO row with the chip's own flash named, which does not hold it: refused at the link by name with the home that does"),
        ("flash-region-absent", "flash-region-absent", "RHU4015", &["names a flash region for", "that the board package does not carry with its record: regions/absent.bin"], "the board table in the package in the cache doctored so that the leg's row names a flash region its board package does not carry (the publish must refuse before the link, by name)"),
    ];
    for &(name, broke, code, needles, what) in table {
        let case = format!("rt.sdk-parity-control-{name}");
        println!("== case {case}: {what} -- the publish must refuse, nothing linked ==");
        let Some(line) = control_line(&controls, name) else {
            rt.fail_control(&case, broke, "the publish-side script left no summary line for this control");
        };
        let rc = control_field(line, "rc");
        let elf = control_field(line, "elf");
        let refuse = line.split_once("refuse=").map(|(_, r)| r).unwrap_or("");
        let code_read = control_field(line, "code");
        if rc == "0" || elf != "absent" || code_read != code || needles.iter().any(|n| !refuse.contains(n)) {
            rt.fail_control(&case, broke, &format!("the doctored input was not refused as registered (the code {code} and the words {needles:?} wanted): {line}"));
        }
        println!("  refused (rc {rc}, {code}, no image): {}", refuse.trim());
        rt.pass(&case, "negative", broke, "");
        println!("  PASS");
    }
    let case = "rt.sdk-parity-control-path-mapped";
    println!("== case {case}: the same project published through the SDK at another directory under the SDK's path map -- its object must be the record's, byte for byte ==");
    let copy_obj = std::fs::read(out.join("control-path").join("rthello.o")).unwrap_or_default();
    if copy_obj.is_empty() {
        rt.fail(case, "the publish at the other directory left no object");
    }
    if !carries_record_path(&copy_obj) {
        rt.fail(case, &format!("the mapped copy's object does not carry the record's project path '{frozen_path}' -- the SDK's path map did not apply"));
    }
    if copy_obj != frozen_obj {
        rt.fail(case, &format!("the mapped copy's object ({} bytes) is not the record's ({} bytes) -- the path map does not make the object path-independent", copy_obj.len(), frozen_obj.len()));
    }
    println!("  the mapped copy's object carries the record's path '{frozen_path}' and ties the record byte for byte ({} bytes)", copy_obj.len());
    rt.pass(case, "positive", "", &format!("{{\"bytes\":{}}}", copy_obj.len()));
    println!("  PASS");
    let case = "rt.sdk-parity-control-path-unmapped";
    println!("== case {case}: the same copy published with the SDK's path map disabled -- its object embeds its own directory and the guard refuses it ==");
    let unmapped_obj = std::fs::read(out.join("control-path-unmapped").join("rthello.o")).unwrap_or_default();
    if unmapped_obj.is_empty() {
        rt.fail_control(case, "project-path", "the unmapped publish left no object");
    }
    if carries_record_path(&unmapped_obj) {
        rt.fail_control(case, "project-path", &format!("the unmapped copy's object carries the record's project path '{frozen_path}' -- the map was not disabled, the control would not bite"));
    }
    if unmapped_obj == frozen_obj {
        rt.fail_control(case, "project-path", "the unmapped copy's object is the record's byte for byte -- the control does not distinguish");
    }
    let unmapped_run = control_line(&controls, "path-unmapped").and_then(|l| l.split_once("embedded=")).map(|(_, r)| r.trim().to_string()).unwrap_or_default();
    println!("  the unmapped copy's object does not carry the record's path '{frozen_path}' (the Linux environment for the publish read '{unmapped_run}' in it) and differs from the record: the guard refuses");
    rt.pass(case, "negative", "project-path", "");
    println!("  PASS");

    // 6. the board table's stack facts against the tree.
    let case = "rt.sdk-parity-row-facts";
    println!("== case {case}: the board table's stack facts (the pool, the default, the clamp per row) and the targets' minimum against the kernel's and the hosting layer's constants ==");
    let stack_facts = match read_stack_facts(repo) {
        Ok(f) => f,
        Err(e) => rt.fail(case, &e),
    };
    if let Some(f) = stack_facts_fault(&stack_facts) {
        rt.fail(case, &f);
    }
    for r in &stack_facts.rows {
        println!("  {}: pool {} B, default {} B, clamp {} B -- the kernel's pool {}, the hosting layer's default {} and clamp {}; the kernel's own threads hold {} B of the pool -- the idle thread {}, the thread that runs Main {}", r.board, r.pool, r.default, r.clamp, r.kernel_pool, r.host_default, r.host_clamp, r.kernel_own, r.kernel_threads[0], r.kernel_threads[1]);
    }
    println!("  the targets' minimum {} B = the header's RHU_THREAD_STACK_MIN {}", stack_facts.sdk_min, stack_facts.kernel_min);
    // The board class's half of the same case: the boards' facts files (the shared board
    // class's source of facts, boards/<row>.json in the SDK package) against
    // the kernel's facts -- every row's pin count, its clock at the row's
    // posture and the bus pads (nothing of LEDs or buttons: the board pages').
    let board_facts = match read_board_facts(repo) {
        Ok(f) => f,
        Err(e) => rt.fail(case, &e),
    };
    if let Some(f) = board_facts_fault(&board_facts) {
        rt.fail(case, &f);
    }
    for r in &board_facts {
        if r.ties.is_empty() {
            println!("  {}: the facts file's {} pins and {} Hz = the kernel's LINE_COUNT and SYSCLK_HZ at the row's posture; {} bus pins tied; {} PWM lines = the kernel's table; the RAM ranges {} = the kernel's RAM_RANGES; the flash ranges {} = the kernel's FLASH_RANGES; the UART rows {} and the channels {} = the kernel's UART_ROWS and CHANNELS; the legal pairs {} = the kernel's UART_PAIRS; the analog converters {} = the kernel's ADC_BLOCKS and ADC_CHANNELS; the net object ({}) = the network pool the kernel gives the row",r.board, r.lines, r.clock_hz, r.ties.len(), r.pwm_lines.len(), ram_list(&r.ram), flash_list(&r.flash), uart_list(&r.uart), channels_words(&r.channels), pairs_list(&r.uart_pairs), adc_list(&r.adc), net_words(&r.net));
        } else {
            let ranges: Vec<String> = r.ladder.iter().map(|(min, max, _, _)| format!("{min} to {max}")).collect();
            println!("  {}: the facts file's {} pins and {} Hz = the kernel's LINE_COUNT and SYSCLK_HZ at the row's posture; {} bus pins tied; the I2C rates {} = the kernel's modes at that clock; the SPI range {} = the block's ladder at {} Hz; the RAM ranges {} = the kernel's RAM_RANGES; the flash ranges {} = the kernel's FLASH_RANGES; the UART rows {} and the channels {} = the kernel's UART_ROWS and CHANNELS; the legal pairs {} = the kernel's UART_PAIRS; the analog converters {} = the kernel's ADC_BLOCKS and ADC_CHANNELS; the net object ({}) = the network pool the kernel gives the row",r.board, r.lines, r.clock_hz, r.ties.len(), number_list(&r.rates), ranges.join(" and "), r.kernel_spi_hz, ram_list(&r.ram), flash_list(&r.flash), uart_list(&r.uart), channels_words(&r.channels), pairs_list(&r.uart_pairs), adc_list(&r.adc), net_words(&r.net));
        }
    }
    match request_tie(repo) {
        Ok(n) => println!("  the rate request's number {n}: the shim's define and the board class's constant carry the same number"),
        Err(e) => rt.fail(case, &e),
    }
    // The PWM unit's table against every row: the unit holds one entry a PWM-capable line at the
    // kernel's index of it, so its size bounds the count of a row's lines and nothing bounds a
    // line's number; and the shim gate's PWM rows tenants answer as the kernel with their rows'
    // tables, which must be the kernel's.
    let pwm_table = match read_pwm_table(repo) {
        Ok(t) => t,
        Err(e) => rt.fail(case, &e),
    };
    if let Some(f) = pwm_table_fault(&board_facts, &pwm_table) {
        rt.fail(case, &f);
    }
    println!("  the PWM unit's table of {} channels holds every row's PWM lines (the most a row states: {}); the shim gate's PWM rows tenants answer with the kernel's PWM_LINES of {}", pwm_table.channels, board_facts.iter().map(|r| r.kernel_pwm_lines.len()).max().unwrap_or(0), pwm_table.tenants.iter().map(|t| t.0.as_str()).collect::<Vec<&str>>().join(" and "));
    rt.pass(case, "positive", "", &format!("{{\"rows\":{},\"facts_files\":{}}}", stack_facts.rows.len(), board_facts.len()));
    println!("  PASS");
    let case = "rt.sdk-parity-control-row-facts";
    println!("== case {case}: the same facts with the first row's pool moved by one byte, and the first facts file's line count moved by one -- the tie must refuse both ==");
    let mut doctored = stack_facts.clone();
    doctored.rows[0].pool += 1;
    match stack_facts_fault(&doctored) {
        Some(f) => println!("  refused: {f}"),
        None => rt.fail_control(case, "row-facts", "a doctored pool was accepted by the tie"),
    }
    // The kernel's own threads' control -- the first row's StackKernel one byte larger in the table.
    let mut doctored_own = stack_facts.clone();
    doctored_own.rows[0].kernel_own += 1;
    match stack_facts_fault(&doctored_own) {
        Some(f) if f.contains("StackKernel") => println!("  refused: {f}"),
        Some(f) => rt.fail_control(case, "row-facts", &format!("the tie refused the doctored StackKernel for another reason: {f}")),
        None => rt.fail_control(case, "row-facts", "a doctored StackKernel was accepted by the tie"),
    }
    let mut doctored_facts = board_facts.clone();
    doctored_facts[0].lines += 1;
    match board_facts_fault(&doctored_facts) {
        Some(f) => println!("  refused: {f}"),
        None => rt.fail_control(case, "row-facts", "a doctored line count was accepted by the tie"),
    }
    // The RAM ranges' control -- the first row's first range one byte larger in the facts file.
    let mut doctored_ram = board_facts.clone();
    doctored_ram[0].ram[0].2 += 1;
    match board_facts_fault(&doctored_ram) {
        Some(f) if f.contains("the facts file's RAM ranges") => println!("  refused: {f}"),
        Some(f) => rt.fail_control(case, "row-facts", &format!("the tie refused the doctored RAM range for another reason: {f}")),
        None => rt.fail_control(case, "row-facts", "a doctored RAM range was accepted by the tie"),
    }
    // The UART rows' and the channels' controls -- the first row's facts file with a
    // UART row the kernel has not, then with its log stream naming a channel the kernel does not.
    let mut doctored_uart = board_facts.clone();
    doctored_uart[0].uart.push((7, None, None));
    match board_facts_fault(&doctored_uart) {
        Some(f) if f.contains("the facts file's UART rows") => println!("  refused: {f}"),
        Some(f) => rt.fail_control(case, "row-facts", &format!("the tie refused the doctored UART rows for another reason: {f}")),
        None => rt.fail_control(case, "row-facts", "a doctored UART row was accepted by the tie"),
    }
    let mut doctored_channels = board_facts.clone();
    doctored_channels[0].channels.0 = "nowhere".to_string();
    match board_facts_fault(&doctored_channels) {
        Some(f) if f.contains("the facts file's channels") => println!("  refused: {f}"),
        Some(f) => rt.fail_control(case, "row-facts", &format!("the tie refused the doctored channels for another reason: {f}")),
        None => rt.fail_control(case, "row-facts", "a doctored channel was accepted by the tie"),
    }
    // The net object's control -- the first row whose facts file carries one, its stream
    // receive buffer one byte larger than the kernel's.
    let mut doctored_net = board_facts.clone();
    match doctored_net.iter_mut().find_map(|r| r.net.as_mut()) {
        Some(n) => n.3 += 1,
        None => rt.fail_control(case, "row-facts", "no row's facts file carries a net object for the control to doctor"),
    }
    match board_facts_fault(&doctored_net) {
        Some(f) if f.contains("the facts file's net object") => println!("  refused: {f}"),
        Some(f) => rt.fail_control(case, "row-facts", &format!("the tie refused the doctored net object for another reason: {f}")),
        None => rt.fail_control(case, "row-facts", "a doctored net object was accepted by the tie"),
    }
    // The second bus tie's controls -- the first row whose kernel facts write their bus rows out in full: a chip
    // select of its facts file moved by one line, and its facts file listing no I2C bus at all.
    let mut doctored_select = board_facts.clone();
    match doctored_select.iter_mut().filter(|r| r.kernel_buses.is_some()).find_map(|r| r.ties.iter_mut().find(|t| t.0.contains("select"))) {
        Some(t) => t.1 += 1,
        None => rt.fail_control(case, "row-facts", "no row with bus rows written out in full has a chip select for the control to doctor"),
    }
    match board_facts_fault(&doctored_select) {
        Some(f) if f.contains("select") && f.contains("!= the kernel's") => println!("  refused: {f}"),
        Some(f) => rt.fail_control(case, "row-facts", &format!("the tie refused the doctored chip select for another reason: {f}")),
        None => rt.fail_control(case, "row-facts", "a doctored chip select was accepted by the tie"),
    }
    let Some((unlisted_board, unlisted_spi, unlisted_buses)) = board_facts.iter().find_map(|r| r.kernel_buses.as_ref().filter(|_| !r.bus_lists.0.is_empty()).map(|k| (&r.board, &r.bus_lists.1, k))) else {
        rt.fail_control(case, "row-facts", "no row with bus rows written out in full lists an I2C bus for the control to drop");
    };
    match literal_bus_ties(unlisted_board, &[], unlisted_spi, &unlisted_buses.0, &unlisted_buses.1) {
        Err(f) if f.contains("lists no such bus") => println!("  refused: {f}"),
        Err(f) => rt.fail_control(case, "row-facts", &format!("the tie refused the dropped I2C bus for another reason: {f}")),
        Ok(_) => rt.fail_control(case, "row-facts", "a facts file that lists no I2C bus was accepted beside the kernel's bus with pads"),
    }
    rt.pass(case, "negative", "row-facts", "");
    println!("  PASS");
    // The rates tie's control -- the first row with buses, its facts file's
    // I2C rates doctored to list a rate the kernel's modes do not carry.
    let case = "rt.sdk-parity-control-rates";
    println!("== case {case}: the first row with buses, its facts file's I2C rates doctored to list 250000 among them -- the tie must refuse ==");
    let mut doctored_rates = board_facts.clone();
    let Some(row) = doctored_rates.iter_mut().find(|r| !r.rates.is_empty()) else {
        rt.fail_control(case, "rates", "no row with buses to doctor");
    };
    row.rates.push(250_000);
    row.rates.sort_unstable();
    match board_facts_fault(&doctored_rates) {
        Some(f) if f.contains("the facts file's I2C rates") => println!("  refused: {f}"),
        Some(f) => rt.fail_control(case, "rates", &format!("the tie refused for another reason: {f}")),
        None => rt.fail_control(case, "rates", "a doctored rates list was accepted by the tie"),
    }
    rt.pass(case, "negative", "rates", "");
    println!("  PASS");

    // The PWM lines tie's control -- the first row's facts file doctored to list line 1 (the emulator
    // rows' hole) among its PWM lines.
    let case = "rt.sdk-parity-control-pwm-lines";
    println!("== case {case}: the first row's facts file with its PWM lines doctored to list 1 -- the tie must refuse ==");
    let mut doctored_pwm = board_facts.clone();
    doctored_pwm[0].pwm_lines.push(1);
    doctored_pwm[0].pwm_lines.sort_unstable();
    match board_facts_fault(&doctored_pwm) {
        Some(f) if f.contains("PWM lines") => println!("  refused: {f}"),
        Some(f) => rt.fail_control(case, "pwm-lines", &format!("the tie refused for another reason: {f}")),
        None => rt.fail_control(case, "pwm-lines", "a doctored PWM line list was accepted by the tie"),
    }
    rt.pass(case, "negative", "pwm-lines", "");
    println!("  PASS");

    // The PWM table tie's control -- the unit's table doctored to one channel fewer than the most
    // PWM lines a row states.
    let case = "rt.sdk-parity-control-pwm-table";
    println!("== case {case}: the PWM unit's table doctored to one channel fewer than the most PWM lines a row states -- the tie must refuse ==");
    let mut doctored_table = pwm_table.clone();
    doctored_table.channels = board_facts.iter().map(|r| r.kernel_pwm_lines.len() as u64).max().unwrap_or(0).saturating_sub(1);
    match pwm_table_fault(&board_facts, &doctored_table) {
        Some(f) if f.contains("more PWM lines than the unit's table") => println!("  refused: {f}"),
        Some(f) => rt.fail_control(case, "pwm-table", &format!("the tie refused for another reason: {f}")),
        None => rt.fail_control(case, "pwm-table", "a table smaller than a row's PWM lines was accepted by the tie"),
    }
    rt.pass(case, "negative", "pwm-table", "");
    println!("  PASS");

    // The analog converters tie's control -- the first row with a channel on a pin, that channel's pin
    // moved to the next pin in the facts file; the tie must refuse it by the row's name.
    let case = "rt.sdk-parity-control-adc-pin";
    println!("== case {case}: the first row with an analog channel on a pin, its facts file's channel moved to the next pin -- the tie must refuse ==");
    let mut doctored_adc = board_facts.clone();
    let mut moved = None;
    'rows: for r in doctored_adc.iter_mut() {
        for c in r.adc.iter_mut() {
            if let Some(ch) = c.3.iter_mut().find(|ch| ch.1.is_some()) {
                ch.1 = ch.1.map(|p| p + 1);
                moved = Some((r.board.clone(), c.0, ch.0));
                break 'rows;
            }
        }
    }
    let Some((board, conv, chan)) = moved else {
        rt.fail_control(case, "adc-pin", "no row with an analog channel on a pin to doctor");
    };
    match board_facts_fault(&doctored_adc) {
        Some(f) if f.contains("analog converters") && f.contains(&board) => println!("  refused: {f}"),
        Some(f) => rt.fail_control(case, "adc-pin", &format!("the tie refused for another reason: {f}")),
        None => rt.fail_control(case, "adc-pin", &format!("a doctored channel pin ({board}: converter {conv}, channel {chan}) was accepted by the tie")),
    }
    rt.pass(case, "negative", "adc-pin", "");
    println!("  PASS");

    // 7. the thread stack knob: the gate's project published with
    // RhuThreadStack set -- the compiled object the record's (the knob is
    // link-time only), the response file free of the runtime's own knob, the
    // budget line printed, the image defining the absolute symbol at the
    // value, and the image booted on the emulator with the row's default
    // moved: the hosting layer's third thread asks the row default through
    // the shim's own thread attributes, so its spawn line reads the value.
    let case = "rt.sdk-parity-stack-knob";
    println!("== case {case}: the gate's project published with RhuThreadStack={STACK_KNOB} -- the object the record's, the response file free of the runtime's knob, the image defining __rhu_thread_stack_default={STACK_KNOB}, its boot with the third thread at that size ==");
    let Some(line) = control_line(&controls, "stack-knob") else {
        rt.fail(case, "the publish-side script left no summary line for the knob publish");
    };
    if control_field(line, "rc") != "0" || control_field(line, "elf") != "present" {
        rt.fail(case, &format!("the publish with the knob did not produce the image: {line}"));
    }
    let knob_obj = std::fs::read(out.join("control-stack-knob").join("rthello.o")).unwrap_or_default();
    if knob_obj != frozen_obj {
        rt.fail(case, &format!("the object published with the knob is not the record's ({} bytes against {}): the knob reached the compile, which it must not", knob_obj.len(), frozen_obj.len()));
    }
    if control_field(line, "runtime-knob-in-rsp") != "0" {
        rt.fail(case, &format!("the response file carries the runtime's own default-stack knob: {line}"));
    }
    let budget = line.split_once("budget=").map(|(_, b)| b.trim()).unwrap_or("");
    let budget_want = format!("the default managed stack {STACK_KNOB} B");
    if !budget.contains(&budget_want) {
        rt.fail(case, &format!("the build did not print the budget line with the knob's value: '{budget}'"));
    }
    let knob_elf = out.join("control-stack-knob").join("rthello.elf");
    let knob_elf_arg = knob_elf.to_string_lossy().into_owned();
    let nm = match llvm_tool(&facts, "llvm-nm") {
        Ok(p) => p,
        Err(e) => rt.fail(case, &e),
    };
    let dump = rt.tool_text(&nm, case, &[&knob_elf_arg], "sdk-parity-stack-knob.nm.txt");
    match word_sym_value(dump.as_bytes(), "__rhu_thread_stack_default") {
        Some(v) if v == STACK_KNOB => println!("  the image defines __rhu_thread_stack_default = {v}; the object the record's; the response file free of the runtime's knob; the build's line: {budget}"),
        Some(v) => rt.fail(case, &format!("the image defines __rhu_thread_stack_default = {v}, not {STACK_KNOB}")),
        None => rt.fail(case, "the image does not define __rhu_thread_stack_default"),
    }
    let (qemu, qemu_version) = match resolve_qemu(repo) {
        Ok(q) => q,
        Err(e) => rt.fail(case, &e),
    };
    let raw_path = out.join("stack-knob-boot.raw");
    let stderr_path = out.join("stack-knob-boot.stderr");
    let mut qargs: Vec<&str> = vec!["-M", w.machine, "-display", "none", "-monitor", "none", "-serial", "stdio", "-semihosting"];
    qargs.extend_from_slice(ExecProfile::Parity.qemu_args());
    qargs.push("-kernel");
    qargs.push(&knob_elf_arg);
    let outcome = match run_captured(&qemu, &qargs, &raw_path, &stderr_path, Duration::from_secs(RUN_TIMEOUT_S)) {
        Ok(o) => o,
        Err(e) => rt.fail(case, &e),
    };
    let raw = std::fs::read(&raw_path).unwrap_or_default();
    echo_file(&raw_path, "  | ");
    if outcome.timed_out {
        rt.fail(case, "the boot with the knob hung (killed at its timeout)");
    }
    if cr_count(&raw) == 0 {
        rt.fail(case, "no CR bytes in raw capture (wire discipline lost)");
    }
    let golden = match std::fs::read(rt.frozen(GOLDEN)) {
        Ok(b) => b,
        Err(e) => rt.fail(case, &format!("golden unreadable: {e}")),
    };
    if !eq_lf_normalized(&payload_stream(&raw), &golden) {
        rt.fail(case, &format!("the payload stream differs from {}/{GOLDEN} under the knob", w.tier));
    }
    let knob_markers = knob_boot_markers(STACK_KNOB, rt.abi, rt.compat);
    let knob_marker_refs: Vec<&str> = knob_markers.iter().map(String::as_str).collect();
    if let Some(fault) = markers_out_of_order(&raw, &knob_marker_refs) {
        rt.fail(case, &format!("the contract's order under the knob: {fault}"));
    }
    if !contains_fixed(&raw, BOOT_RETURN_LINE) {
        rt.fail(case, &format!("the kernel's return line '{BOOT_RETURN_LINE}' not found"));
    }
    for m in FORBIDDEN_PATTERNS {
        if contains_fixed(&raw, m) {
            rt.fail(case, &format!("forbidden pattern '{m}' present"));
        }
    }
    if outcome.exit != 0 {
        rt.fail(case, &format!("exit {}, wanted 0", outcome.exit));
    }
    let knob_sha = sha256_file(&knob_elf).unwrap_or_default();
    let knob_rel = format!("{out_rel}/control-stack-knob/rthello.elf");
    rt.pass_image(case, 0, &knob_rel, &knob_sha, Some(GOLDEN), &format!("{{\"knob\":{STACK_KNOB},\"markers\":{}}}", knob_markers.len()));
    println!("  PASS (the third thread's spawn line at {STACK_KNOB} B; the payload stream equals its golden; {} markers in order; {qemu_version})", knob_markers.len());

    // 7b. the thread pool's idle-worker timeout knob: the gate's project
    // published with RhuThreadPoolThreadTimeoutMs set, at 250 and at -1 --
    // the compiled object the record's (the knob is link-time only), the
    // image defining the absolute symbol at the value, the never as the
    // 32-bit word; the environment row the hosting layer builds from the
    // word is the shim gate's case.
    for (name, knob, want) in [("pool-timeout-knob", "250", 250u64), ("pool-timeout-never", "-1", 4294967295u64)] {
        let case = format!("rt.sdk-parity-{name}");
        println!("== case {case}: the gate's project published with RhuThreadPoolThreadTimeoutMs={knob} -- the object the record's, the image defining __rhu_threadpool_thread_timeout_ms = {want} ==");
        let Some(line) = control_line(&controls, name) else {
            rt.fail(&case, "the publish-side script left no summary line for the knob publish");
        };
        if control_field(line, "rc") != "0" || control_field(line, "elf") != "present" {
            rt.fail(&case, &format!("the publish with the knob did not produce the image: {line}"));
        }
        let obj = std::fs::read(out.join(format!("control-{name}")).join("rthello.o")).unwrap_or_default();
        if obj != frozen_obj {
            rt.fail(&case, &format!("the object published with the knob is not the record's ({} bytes against {}): the knob reached the compile, which it must not", obj.len(), frozen_obj.len()));
        }
        let elf = out.join(format!("control-{name}")).join("rthello.elf");
        let elf_arg = elf.to_string_lossy().into_owned();
        let dump = rt.tool_text(&nm, &case, &[&elf_arg], &format!("sdk-parity-{name}.nm.txt"));
        match word_sym_value(dump.as_bytes(), "__rhu_threadpool_thread_timeout_ms") {
            Some(v) if v == want => println!("  the image defines __rhu_threadpool_thread_timeout_ms = {v}; the object the record's; the build's line: {}", line.split_once("budget=").map(|(_, b)| b.trim()).unwrap_or("")),
            Some(v) => rt.fail(&case, &format!("the image defines __rhu_threadpool_thread_timeout_ms = {v}, not {want}")),
            None => rt.fail(&case, "the image does not define __rhu_threadpool_thread_timeout_ms"),
        }
        rt.pass(&case, "positive", "", &format!("{{\"knob\":\"{knob}\",\"word\":{want}}}"));
        println!("  PASS");
    }

    // 8. The path-map control: the same project published from a real
    // git checkout at another path -- the root three levels above the
    // project, the tree's ignore file covering the build directories, the
    // shape at which the .NET SDK's git tasks embed the generated attribute
    // sources into the symbol file as untracked files and move the
    // assembly's module version id -- ties the record byte for byte with no
    // source embedded; the same checkout with the source-control queries
    // turned back on moves and embeds, so the control bites.
    let case = "rt.sdk-parity-control-git-checkout";
    println!("== case {case}: the same project published from a real git checkout at another path, its ignore rules covering the build directory -- its object must be the record's byte for byte and its symbol file must embed no source ==");
    let Some(line) = control_line(&controls, "git-checkout") else {
        rt.fail(case, "the publish-side script left no summary line for the checkout's publish");
    };
    if control_field(line, "rc") != "0" || control_field(line, "object") != "present" || control_field(line, "ignore") != "present" || control_field(line, "git").is_empty() {
        rt.fail(case, &format!("the checkout's publish did not run as registered: {line}"));
    }
    let checkout_obj = std::fs::read(out.join("control-git-checkout").join("rthello.o")).unwrap_or_default();
    let checkout_pdb = std::fs::read(out.join("control-git-checkout").join("rthello.pdb")).unwrap_or_default();
    if checkout_pdb.is_empty() {
        rt.fail(case, "the checkout's publish left no symbol file");
    }
    if embeds_source(&checkout_pdb) {
        rt.fail(case, "the checkout's symbol file embeds a source: the source-control queries ran");
    }
    if checkout_obj != frozen_obj {
        rt.fail(case, &format!("the checkout's object ({} bytes) is not the record's ({} bytes): a source-control fact reached the build", checkout_obj.len(), frozen_obj.len()));
    }
    println!("  the checkout at {} ties the record byte for byte ({} bytes); its symbol file embeds no source", control_field(line, "git"), checkout_obj.len());
    rt.pass(case, "positive", "", &format!("{{\"bytes\":{}}}", checkout_obj.len()));
    println!("  PASS");
    let case = "rt.sdk-parity-control-git-embed";
    println!("== case {case}: the same checkout published with the source-control queries turned back on -- its symbol file embeds the generated sources and its object moves: the control bites ==");
    let Some(line) = control_line(&controls, "git-embed") else {
        rt.fail_control(case, "source-control-queries", "the publish-side script left no summary line for the publish with the queries on");
    };
    if control_field(line, "rc") != "0" || control_field(line, "object") != "present" {
        rt.fail_control(case, "source-control-queries", &format!("the publish with the queries on did not produce an object: {line}"));
    }
    let embed_obj = std::fs::read(out.join("control-git-embed").join("rthello.o")).unwrap_or_default();
    let embed_pdb = std::fs::read(out.join("control-git-embed").join("rthello.pdb")).unwrap_or_default();
    if !embeds_source(&embed_pdb) {
        rt.fail_control(case, "source-control-queries", "the symbol file embeds no source with the queries on -- the control would not bite");
    }
    if embed_obj == frozen_obj {
        rt.fail_control(case, "source-control-queries", "the object is the record's with the queries on -- the control does not distinguish");
    }
    println!("  with the queries on the symbol file embeds sources and the object moves ({}...): the control bites", control_field(line, "sha256"));
    rt.pass(case, "negative", "source-control-queries", "");
    println!("  PASS");

    // 9. The board class: the gate's second program, tbhello, published
    // through the SDK with its reference to Lamella.Hardware -- the SDK
    // generated the row's facts into it (a new program, tied to no record) --
    // and its image booted on the emulator: the class's six lines in the
    // order the facts file predicts, the kernel's return line, exit 0.
    let case = "rt.sdk-parity-board-class";
    println!("== case {case}: the board class's program published through the SDK with the row's facts generated into it, its image booted on the emulator -- the class's six lines in order, exit 0 ==");
    let Some(line) = control_line(&controls, "board-class") else {
        rt.fail(case, "the publish-side script left no summary line for the board class's publish");
    };
    if control_field(line, "rc") != "0" || control_field(line, "elf") != "present" || control_field(line, "generated") != "1" {
        rt.fail(case, &format!("the board class's publish did not run as registered (rc 0, the image present, the facts generated once): {line}"));
    }
    let class_markers = match board_class_markers(repo, w.sdk_board) {
        Ok(m) => m,
        Err(e) => rt.fail(case, &e),
    };
    let class_elf = out.join("control-board-class").join("tbhello.elf");
    let class_elf_arg = class_elf.to_string_lossy().into_owned();
    let raw_path = out.join("board-class-boot.raw");
    let stderr_path = out.join("board-class-boot.stderr");
    let mut qargs: Vec<&str> = vec!["-M", w.machine, "-display", "none", "-monitor", "none", "-serial", "stdio", "-semihosting"];
    qargs.extend_from_slice(ExecProfile::Parity.qemu_args());
    qargs.push("-kernel");
    qargs.push(&class_elf_arg);
    let outcome = match run_captured(&qemu, &qargs, &raw_path, &stderr_path, Duration::from_secs(RUN_TIMEOUT_S)) {
        Ok(o) => o,
        Err(e) => rt.fail(case, &e),
    };
    let raw = std::fs::read(&raw_path).unwrap_or_default();
    echo_file(&raw_path, "  | ");
    if outcome.timed_out {
        rt.fail(case, "the board class's boot hung (killed at its timeout)");
    }
    if cr_count(&raw) == 0 {
        rt.fail(case, "no CR bytes in raw capture (wire discipline lost)");
    }
    let class_marker_refs: Vec<&str> = class_markers.iter().map(String::as_str).collect();
    if let Some(fault) = markers_out_of_order(&raw, &class_marker_refs) {
        rt.fail(case, &format!("the board class's lines, as the row's facts file predicts them: {fault}"));
    }
    if !contains_fixed(&raw, BOOT_RETURN_LINE) {
        rt.fail(case, &format!("the kernel's return line '{BOOT_RETURN_LINE}' not found"));
    }
    for m in FORBIDDEN_PATTERNS {
        if contains_fixed(&raw, m) {
            rt.fail(case, &format!("forbidden pattern '{m}' present"));
        }
    }
    if outcome.exit != 0 {
        rt.fail(case, &format!("exit {}, wanted 0", outcome.exit));
    }
    let class_sha = sha256_file(&class_elf).unwrap_or_default();
    let class_rel = format!("{out_rel}/control-board-class/tbhello.elf");
    rt.pass_image(case, 0, &class_rel, &class_sha, None, &format!("{{\"markers\":{},\"board\":\"{}\"}}", class_markers.len(), w.sdk_board));
    println!("  PASS (the class's {} lines in the facts file's order; the image {}...; {qemu_version})", class_markers.len(), &class_sha[..16.min(class_sha.len())]);

    // 9b. The reflection knob without the board class: the gate's first
    // program, rthello, published with RhuReflection=off -- the knob reaches
    // the compiler (the response file carries its argument once), the object
    // moves off the record (the stripped data; a program tied to no record),
    // and the image boots on the emulator: the program's verdict line, the
    // kernel's return line, no forbidden pattern, exit 0. The knob stays
    // honoured where nothing needs the data; with the board class referenced
    // the control above shows it refused.
    let case = "rt.sdk-parity-reflection-off-plain";
    println!("== case {case}: the gate's program published with RhuReflection=off and no board class -- the knob honoured, the image booted on the emulator: the program's verdict, the kernel's return line, exit 0 ==");
    let Some(line) = control_line(&controls, "reflection-off-plain") else {
        rt.fail(case, "the publish-side script left no summary line for the publish with the reflection data stripped");
    };
    if control_field(line, "rc") != "0" || control_field(line, "elf") != "present" || control_field(line, "refuse") != "NONE" {
        rt.fail(case, &format!("the publish with the knob and no board class did not produce the image as registered (rc 0, the image present, no refusal): {line}"));
    }
    if control_field(line, "reflection-in-rsp") != "1" {
        rt.fail(case, &format!("the response file does not carry the knob's argument once: {line}"));
    }
    let plain_obj = std::fs::read(out.join("control-reflection-off-plain").join("rthello.o")).unwrap_or_default();
    if plain_obj.is_empty() {
        rt.fail(case, "the publish with the knob left no object");
    }
    if plain_obj == frozen_obj {
        rt.fail(case, "the object published with the reflection data stripped is the record's byte for byte: the knob did not reach the compile");
    }
    let plain_elf = out.join("control-reflection-off-plain").join("rthello.elf");
    let plain_elf_arg = plain_elf.to_string_lossy().into_owned();
    let raw_path = out.join("reflection-off-plain-boot.raw");
    let stderr_path = out.join("reflection-off-plain-boot.stderr");
    let mut qargs: Vec<&str> = vec!["-M", w.machine, "-display", "none", "-monitor", "none", "-serial", "stdio", "-semihosting"];
    qargs.extend_from_slice(ExecProfile::Parity.qemu_args());
    qargs.push("-kernel");
    qargs.push(&plain_elf_arg);
    let outcome = match run_captured(&qemu, &qargs, &raw_path, &stderr_path, Duration::from_secs(RUN_TIMEOUT_S)) {
        Ok(o) => o,
        Err(e) => rt.fail(case, &e),
    };
    let raw = std::fs::read(&raw_path).unwrap_or_default();
    echo_file(&raw_path, "  | ");
    if outcome.timed_out {
        rt.fail(case, "the boot with the reflection data stripped hung (killed at its timeout)");
    }
    if cr_count(&raw) == 0 {
        rt.fail(case, "no CR bytes in raw capture (wire discipline lost)");
    }
    if !contains_fixed(&raw, BOOT_ALL_GOOD) {
        rt.fail(case, &format!("the program's verdict line '{BOOT_ALL_GOOD}' not found"));
    }
    if !contains_fixed(&raw, BOOT_RETURN_LINE) {
        rt.fail(case, &format!("the kernel's return line '{BOOT_RETURN_LINE}' not found"));
    }
    for m in FORBIDDEN_PATTERNS {
        if contains_fixed(&raw, m) {
            rt.fail(case, &format!("forbidden pattern '{m}' present"));
        }
    }
    if outcome.exit != 0 {
        rt.fail(case, &format!("exit {}, wanted 0", outcome.exit));
    }
    let plain_sha = sha256_file(&plain_elf).unwrap_or_default();
    let plain_rel = format!("{out_rel}/control-reflection-off-plain/rthello.elf");
    rt.pass_image(case, 0, &plain_rel, &plain_sha, None, &format!("{{\"object_bytes\":{},\"record_bytes\":{}}}", plain_obj.len(), frozen_obj.len()));
    println!("  PASS (the knob's argument in the response file; the object {} bytes against the record's {}; the image {}... booted to the verdict line; {qemu_version})", plain_obj.len(), frozen_obj.len(), &plain_sha[..16.min(plain_sha.len())]);

    // 10. The external RAM init mode (the property's half of the lazy init modes): the
    // board table's lazy kernels tied to the boards' facts files; then, on
    // the external-RAM row, the publish per offered mode -- the object the
    // record's (the mode is a link-time pick), the board line naming the
    // mode's archive and the mode, the image's read-only data carrying the
    // mode's banner words and neither other mode's; the named default the
    // same image as none; a withheld mode's publish refused with the withheld
    // table's words.
    let case = "rt.sdk-parity-ram-init-table";
    println!("== case {case}: the board table's init-mode kernels against the boards' facts files -- a row names the two lazy archives exactly when its facts list an external RAM range, at the defined paths ==");
    match ram_init_table_check(repo) {
        Ok(rows) => {
            let lazy: Vec<&str> = rows.iter().filter(|(_, e)| *e).map(|(n, _)| n.as_str()).collect();
            println!("  {} rows; the rows with an external RAM range and the two lazy kernels: {}", rows.len(), lazy.join(", "));
            rt.pass(case, "positive", "", &format!("{{\"rows\":{},\"lazy_rows\":{}}}", rows.len(), lazy.len()));
            println!("  PASS");
        }
        Err(e) => rt.fail(case, &e),
    }
    let objcopy = match llvm_tool(&facts, "llvm-objcopy") {
        Ok(p) => p,
        Err(e) => rt.fail("rt.sdk-parity-ram-init", &e),
    };
    let mut modes_offered = 0usize;
    let mut modes_withheld = 0usize;
    for (mode, _feature, phrase) in RAM_INIT_MODES {
        let label = format!("ram-init-{mode}");
        let case = format!("rt.sdk-parity-{label}");
        let Some(line) = control_line(&controls, &label) else {
            rt.fail(&case, "the publish-side script left no summary line for this publish");
        };
        if withheld.iter().any(|v| v == mode) {
            println!("== case {case}: the {mode} value withheld by the SDK's withheld table -- the publish must refuse with the table's words, nothing linked ==");
            let refuse = line.split_once("refuse=").map(|(_, r)| r).unwrap_or("");
            if control_field(line, "rc") == "0" || control_field(line, "elf") != "absent" || !refuse.contains("withheld in this release") {
                rt.fail_control(&case, "ram-init-withheld-table", &format!("the withheld value was not refused as the table says: {line}"));
            }
            println!("  refused (rc {}, no image): {}", control_field(line, "rc"), refuse.trim());
            rt.pass(&case, "negative", "ram-init-withheld-table", "");
            modes_withheld += 1;
            println!("  PASS");
            continue;
        }
        let want_kernel = if mode == "boot" { format!("kernels/{RAM_INIT_ROW}/librhu_kernel.a") } else { format!("kernels/{RAM_INIT_ROW}/ram-init-{mode}/librhu_kernel.a") };
        println!("== case {case}: the gate's project published for {RAM_INIT_ROW} with RhuExternalRamInit={mode} -- the object the record's, the board line naming {want_kernel} and the mode, the image's rodata carrying \"{phrase}\" and neither other mode's words ==");
        if control_field(line, "rc") != "0" || control_field(line, "elf") != "present" {
            rt.fail(&case, &format!("the publish did not produce the image: {line}"));
        }
        let obj = std::fs::read(out.join(format!("control-{label}")).join("rthello.o")).unwrap_or_default();
        if obj != frozen_obj {
            rt.fail(&case, &format!("the object published for the row in the {mode} init mode is not the record's ({} bytes against {}): the mode reached the compile, which it must not", obj.len(), frozen_obj.len()));
        }
        if control_field(line, "kernel") != want_kernel {
            rt.fail(&case, &format!("the board line names the kernel '{}', not {want_kernel}", control_field(line, "kernel")));
        }
        if control_field(line, "mode") != mode {
            rt.fail(&case, &format!("the board line's mode words read '{}', not {mode}", control_field(line, "mode")));
        }
        let elf = out.join(format!("control-{label}")).join("rthello.elf");
        let rodata_path = out.join(format!("{label}.rodata.bin"));
        let elf_arg = elf.to_string_lossy().into_owned();
        let rodata_arg = rodata_path.to_string_lossy().into_owned();
        let oargs = ["-O", "binary", "--only-section=.rodata", elf_arg.as_str(), rodata_arg.as_str()];
        match run_captured(&objcopy, &oargs, &out.join(format!("{label}.objcopy.stdout")), &out.join(format!("{label}.objcopy.stderr")), Duration::from_secs(120)) {
            Ok(o) if o.exit == 0 => {}
            Ok(o) => rt.fail(&case, &format!("llvm-objcopy did not print the image's rodata out (exit {})", o.exit)),
            Err(e) => rt.fail(&case, &e),
        }
        let rodata = std::fs::read(&rodata_path).unwrap_or_default();
        if rodata.is_empty() {
            rt.fail(&case, "the image's rodata printed out empty");
        }
        let count = |needle: &str| rodata.windows(needle.len()).filter(|w| *w == needle.as_bytes()).count();
        for (_, _, p) in RAM_INIT_MODES {
            let n = count(p);
            if p == phrase && n == 0 {
                rt.fail(&case, &format!("the image's rodata does not carry \"{phrase}\""));
            }
            if p != phrase && n != 0 {
                rt.fail(&case, &format!("the image's rodata carries \"{p}\" ({n}) beside the mode's own words"));
            }
        }
        let sha = sha256_file(&elf).unwrap_or_default();
        rt.pass(&case, "positive", "", &format!("{{\"mode\":\"{mode}\",\"kernel\":\"{want_kernel}\",\"image_sha\":\"{sha}\",\"rodata_bytes\":{},\"phrase_count\":{}}}", rodata.len(), count(phrase)));
        modes_offered += 1;
        println!("  PASS (the object the record's; the kernel {want_kernel}; the rodata, {} bytes, carries \"{phrase}\" {} time(s) and neither other mode's words; the image {}...)", rodata.len(), count(phrase), &sha[..16.min(sha.len())]);
    }
    let case = "rt.sdk-parity-ram-init-default";
    println!("== case {case}: the row published with RhuExternalRamInit=boot named and with the property empty -- the same image byte for byte under the row's default kernel ==");
    let Some(line) = control_line(&controls, "ram-init-boot-named") else {
        rt.fail(case, "the publish-side script left no summary line for the named default's publish");
    };
    let default_kernel = format!("kernels/{RAM_INIT_ROW}/librhu_kernel.a");
    if control_field(line, "rc") != "0" || control_field(line, "elf") != "present" || control_field(line, "kernel") != default_kernel || control_field(line, "mode") != "boot" {
        rt.fail(case, &format!("the named default's publish did not run as registered (rc 0, the image present, the row's default kernel, the mode boot): {line}"));
    }
    let named = std::fs::read(out.join("control-ram-init-boot-named").join("rthello.elf")).unwrap_or_default();
    let plain = std::fs::read(out.join("control-ram-init-boot").join("rthello.elf")).unwrap_or_default();
    if named.is_empty() || named != plain {
        rt.fail(case, &format!("the named default's image ({} bytes) is not the empty property's ({} bytes)", named.len(), plain.len()));
    }
    rt.pass(case, "positive", "", &format!("{{\"bytes\":{}}}", named.len()));
    println!("  PASS (the two images byte-identical, {} bytes)", named.len());

    // 10b. The managed memory's home (the property's half of the external
    // heap): the board table's second maps tied to the boards' facts files
    // and to the tree; this leg's own emulator row published with the home
    // internal -- the object the record's (the home is a link-time pick),
    // the board line naming the row's second map and its default kernel,
    // the image's rehydrated-data brackets inside the chip's own RAM range
    // and its second window the whole external range, its boot printing
    // the hosting layer's posture line internal with the program's stream
    // equal to the golden -- and the row's default publish as the pair (the
    // brackets inside the external range with the second window after
    // them, the posture line external, the named default the same image as
    // none); then the Pimoroni Pico Plus 2 row published with the home
    // internal, its brackets read against its own facts (the image the
    // bench boot's).
    let case = "rt.sdk-parity-managed-memory-table";
    println!("== case {case}: the board table's second maps against the boards' facts files and the tree -- a row names its second map exactly when its facts list an external RAM range, both files in the tree ==");
    let home_rows = match managed_memory_table_check(repo) {
        Ok(rows) => {
            let named: Vec<&str> = rows.iter().filter(|(_, _, m)| m.is_some()).map(|(n, _, _)| n.as_str()).collect();
            println!("  {} rows; the rows with a second map: {}", rows.len(), named.join(", "));
            rt.pass(case, "positive", "", &format!("{{\"rows\":{},\"second_map_rows\":{}}}", rows.len(), named.len()));
            println!("  PASS");
            rows
        }
        Err(e) => rt.fail(case, &e),
    };
    let home_row = w.sdk_board;
    let case = "rt.sdk-parity-managed-memory-internal";
    let (home_map, home_second) = match home_rows.iter().find(|(n, _, _)| n == home_row) {
        Some((_, m, Some(s))) => (m.clone(), s.clone()),
        _ => rt.fail(case, &format!("the board table names no second map for this leg's row {home_row}")),
    };
    let home_facts = match read_facts_file(&repo.join(format!("src/sdk/Lamella.Rhu.Sdk/boards/{home_row}.json").replace('/', std::path::MAIN_SEPARATOR_STR))) {
        Ok(f) => f,
        Err(e) => rt.fail(case, &e),
    };
    let Some(((own_base, own_bytes), (ext_base, ext_bytes))) = facts_ram_ranges(&home_facts) else {
        rt.fail(case, &format!("the facts file of {home_row} lists no chip RAM range beside an external one"));
    };
    let default_kernel = format!("kernels/{home_row}/librhu_kernel.a");
    println!("== case {case}: the gate's project published for {home_row} with RhuManagedMemory=internal -- the object the record's, the board line naming {home_second} and {default_kernel}, the image's rehydrated-data brackets inside the chip's own RAM range and its second window the whole external range, its boot printing the hosting layer's posture line internal with the program's stream equal to the golden ==");
    let Some(line) = control_line(&controls, "managed-memory-internal") else {
        rt.fail(case, "the publish-side script left no summary line for the internal publish");
    };
    if control_field(line, "rc") != "0" || control_field(line, "elf") != "present" {
        rt.fail(case, &format!("the publish did not produce the image: {line}"));
    }
    let obj = std::fs::read(out.join("control-managed-memory-internal").join("rthello.o")).unwrap_or_default();
    if obj != frozen_obj {
        rt.fail(case, &format!("the object published with the home internal is not the record's ({} bytes against {}): the home reached the compile, which it must not", obj.len(), frozen_obj.len()));
    }
    if control_field(line, "map") != home_second {
        rt.fail(case, &format!("the board line names the map '{}', not {home_second}", control_field(line, "map")));
    }
    if control_field(line, "kernel") != default_kernel {
        rt.fail(case, &format!("the board line names the kernel '{}', not {default_kernel}", control_field(line, "kernel")));
    }
    if control_field(line, "home") != "internal" {
        rt.fail(case, &format!("the board line's home words read '{}', not internal", control_field(line, "home")));
    }
    let nm_tool = match llvm_tool(&facts, "llvm-nm") {
        Ok(p) => p,
        Err(e) => rt.fail(case, &e),
    };
    let internal_elf = out.join("control-managed-memory-internal").join("rthello.elf");
    let internal_elf_arg = internal_elf.to_string_lossy().into_owned();
    let dump = rt.tool_text(&nm_tool, case, &[&internal_elf_arg], "sdk-parity-managed-memory-internal.nm.txt");
    let (lo, hi, a1lo, a1hi) = match window_symbols(dump.as_bytes()) {
        Ok(s) => s,
        Err(e) => rt.fail(case, &e),
    };
    if !(lo <= hi && lo >= own_base && hi <= own_base + own_bytes) {
        rt.fail(case, &format!("the rehydrated-data brackets [{lo:#x}, {hi:#x}) lie outside the chip's own RAM range [{own_base:#x}, {:#x})", own_base + own_bytes));
    }
    if a1lo != ext_base || a1hi != ext_base + ext_bytes {
        rt.fail(case, &format!("the second window [{a1lo:#x}, {a1hi:#x}) is not the whole external range [{ext_base:#x}, {:#x})", ext_base + ext_bytes));
    }
    println!("  the brackets [{lo:#x}, {hi:#x}) inside the chip's own RAM; the second window [{a1lo:#x}, {a1hi:#x}) the whole external range; the object the record's; the board line: map {home_second}, kernel {default_kernel}, the home internal");
    let qemu_version = boot_for_heap_window(&mut rt, case, repo, w.machine, w.tier, &out, &internal_elf, "managed-memory-internal-boot", HEAP_WINDOW_LINE_INTERNAL, HEAP_WINDOW_LINE_EXTERNAL);
    let internal_sha = sha256_file(&internal_elf).unwrap_or_default();
    rt.pass_image(case, 0, &format!("{out_rel}/control-managed-memory-internal/rthello.elf"), &internal_sha, Some(GOLDEN), &format!("{{\"map\":\"{home_second}\",\"hydrated_lo\":{lo},\"hydrated_hi\":{hi},\"arena1_lo\":{a1lo},\"arena1_hi\":{a1hi}}}"));
    println!("  PASS (the posture line internal; the payload stream equals its golden; {qemu_version})");
    let case = "rt.sdk-parity-managed-memory-default";
    println!("== case {case}: the row published with RhuManagedMemory=external named and with the property empty -- the same image byte for byte under {home_map}, its rehydrated-data brackets inside the external range with the second window after them, its boot printing the posture line external ==");
    let Some(named_line) = control_line(&controls, "managed-memory-external-named") else {
        rt.fail(case, "the publish-side script left no summary line for the named default's publish");
    };
    let Some(plain_line) = control_line(&controls, "managed-memory-external") else {
        rt.fail(case, "the publish-side script left no summary line for the empty property's publish");
    };
    for (label, l) in [("named", named_line), ("plain", plain_line)] {
        if control_field(l, "rc") != "0" || control_field(l, "elf") != "present" || control_field(l, "map") != home_map || control_field(l, "kernel") != default_kernel || control_field(l, "home") != "external" {
            rt.fail(case, &format!("the {label} default's publish did not run as registered (rc 0, the image present, the row's own map, its default kernel, the home external): {l}"));
        }
    }
    let named = std::fs::read(out.join("control-managed-memory-external-named").join("rthello.elf")).unwrap_or_default();
    let plain_elf = out.join("control-managed-memory-external").join("rthello.elf");
    let plain = std::fs::read(&plain_elf).unwrap_or_default();
    if named.is_empty() || named != plain {
        rt.fail(case, &format!("the named default's image ({} bytes) is not the empty property's ({} bytes)", named.len(), plain.len()));
    }
    let plain_elf_arg = plain_elf.to_string_lossy().into_owned();
    let dump = rt.tool_text(&nm_tool, case, &[&plain_elf_arg], "sdk-parity-managed-memory-external.nm.txt");
    let (lo, hi, a1lo, a1hi) = match window_symbols(dump.as_bytes()) {
        Ok(s) => s,
        Err(e) => rt.fail(case, &e),
    };
    if !(lo <= hi && lo >= ext_base && hi <= ext_base + ext_bytes) {
        rt.fail(case, &format!("the rehydrated-data brackets [{lo:#x}, {hi:#x}) lie outside the external range [{ext_base:#x}, {:#x})", ext_base + ext_bytes));
    }
    if a1lo < hi || a1lo - hi >= 8 || a1hi != ext_base + ext_bytes {
        rt.fail(case, &format!("the second window [{a1lo:#x}, {a1hi:#x}) does not start at the brackets' end [{lo:#x}, {hi:#x}) 8-aligned and run to the external range's end {:#x}", ext_base + ext_bytes));
    }
    println!("  the two images byte-identical ({} bytes); the brackets [{lo:#x}, {hi:#x}) inside the external range; the second window [{a1lo:#x}, {a1hi:#x}) after them", named.len());
    let qemu_version = boot_for_heap_window(&mut rt, case, repo, w.machine, w.tier, &out, &plain_elf, "managed-memory-external-boot", HEAP_WINDOW_LINE_EXTERNAL, HEAP_WINDOW_LINE_INTERNAL);
    let plain_sha = sha256_file(&plain_elf).unwrap_or_default();
    rt.pass_image(case, 0, &format!("{out_rel}/control-managed-memory-external/rthello.elf"), &plain_sha, Some(GOLDEN), &format!("{{\"map\":\"{home_map}\",\"bytes\":{},\"hydrated_lo\":{lo},\"hydrated_hi\":{hi},\"arena1_lo\":{a1lo},\"arena1_hi\":{a1hi}}}", named.len()));
    println!("  PASS (the posture line external; the payload stream equals its golden; {qemu_version})");
    let case = "rt.sdk-parity-managed-memory-internal-plus2";
    let plus2_second = match home_rows.iter().find(|(n, _, _)| n == RAM_INIT_ROW) {
        Some((_, _, Some(s))) => s.clone(),
        _ => rt.fail(case, &format!("the board table names no second map for the {RAM_INIT_ROW} row")),
    };
    let plus2_facts = match read_facts_file(&repo.join(format!("src/sdk/Lamella.Rhu.Sdk/boards/{RAM_INIT_ROW}.json").replace('/', std::path::MAIN_SEPARATOR_STR))) {
        Ok(f) => f,
        Err(e) => rt.fail(case, &e),
    };
    let Some(((own_base, own_bytes), (ext_base, ext_bytes))) = facts_ram_ranges(&plus2_facts) else {
        rt.fail(case, &format!("the facts file of {RAM_INIT_ROW} lists no chip RAM range beside an external one"));
    };
    let plus2_kernel = format!("kernels/{RAM_INIT_ROW}/librhu_kernel.a");
    println!("== case {case}: the gate's project published for {RAM_INIT_ROW} with RhuManagedMemory=internal -- the object the record's, the board line naming {plus2_second} and {plus2_kernel}, the image's rehydrated-data brackets inside the chip's own RAM range and its second window the whole external range (the image the bench boot's) ==");
    let Some(line) = control_line(&controls, "managed-memory-internal-plus2") else {
        rt.fail(case, "the publish-side script left no summary line for the row's internal publish");
    };
    if control_field(line, "rc") != "0" || control_field(line, "elf") != "present" {
        rt.fail(case, &format!("the publish did not produce the image: {line}"));
    }
    let obj = std::fs::read(out.join("control-managed-memory-internal-plus2").join("rthello.o")).unwrap_or_default();
    if obj != frozen_obj {
        rt.fail(case, &format!("the object published for the row with the home internal is not the record's ({} bytes against {}): the home reached the compile, which it must not", obj.len(), frozen_obj.len()));
    }
    if control_field(line, "map") != plus2_second || control_field(line, "kernel") != plus2_kernel || control_field(line, "home") != "internal" {
        rt.fail(case, &format!("the board line did not name the row's second map, its default kernel and the home internal: {line}"));
    }
    let plus2_elf = out.join("control-managed-memory-internal-plus2").join("rthello.elf");
    let plus2_elf_arg = plus2_elf.to_string_lossy().into_owned();
    let dump = rt.tool_text(&nm_tool, case, &[&plus2_elf_arg], "sdk-parity-managed-memory-internal-plus2.nm.txt");
    let (lo, hi, a1lo, a1hi) = match window_symbols(dump.as_bytes()) {
        Ok(s) => s,
        Err(e) => rt.fail(case, &e),
    };
    if !(lo <= hi && lo >= own_base && hi <= own_base + own_bytes) {
        rt.fail(case, &format!("the rehydrated-data brackets [{lo:#x}, {hi:#x}) lie outside the chip's own RAM range [{own_base:#x}, {:#x})", own_base + own_bytes));
    }
    if a1lo != ext_base || a1hi != ext_base + ext_bytes {
        rt.fail(case, &format!("the second window [{a1lo:#x}, {a1hi:#x}) is not the whole external range [{ext_base:#x}, {:#x})", ext_base + ext_bytes));
    }
    let plus2_sha = sha256_file(&plus2_elf).unwrap_or_default();
    rt.pass(case, "positive", "", &format!("{{\"map\":\"{plus2_second}\",\"image_sha\":\"{plus2_sha}\",\"hydrated_lo\":{lo},\"hydrated_hi\":{hi},\"arena1_lo\":{a1lo},\"arena1_hi\":{a1hi}}}"));
    println!("  PASS (the brackets [{lo:#x}, {hi:#x}) inside the chip's own RAM; the second window [{a1lo:#x}, {a1hi:#x}) the whole external range; the object the record's; the image {}...)", &plus2_sha[..16.min(plus2_sha.len())]);

    // 10c. The program's home (the external-flash posture): the board table's
    // external-flash map tied to the boards' facts files and the tree, and
    // its fit column tied to the maps; the STM32F769I-DISCO row published
    // with RhuProgramMemory=external -- the object the record's (the home is
    // a link-time pick), the board line naming the row's external-flash map
    // and its default kernel, the image linked silent with no undefined
    // name, the window read off the artifact (the managed code, the
    // unboxing stubs and the index brackets inside the facts' external flash
    // range; the program object's read-only data and its retyped index as
    // window sections of the section table; the native code inside the
    // chip's flash with its own exception-index table) -- and the pair
    // beside it: no property is that same image, the external flash being
    // the row's default home, and the chip's own flash named holds the
    // whole image under the row's own map, no window declared.
    let case = "rt.sdk-parity-program-memory-table";
    println!("== case {case}: the board table's external-flash maps against the boards' facts files and the tree -- a row names its external-flash map exactly when its facts list a mapped, executable external flash range, both files in the tree ==");
    let xip_rows = match program_memory_table_check(repo) {
        Ok(rows) => {
            let named: Vec<&str> = rows.iter().filter(|(_, _, m)| m.is_some()).map(|(n, _, _)| n.as_str()).collect();
            println!("  {} rows; the rows with an external-flash map: {}", rows.len(), named.join(", "));
            rt.pass(case, "positive", "", &format!("{{\"rows\":{},\"external_flash_rows\":{}}}", rows.len(), named.len()));
            println!("  PASS");
            rows
        }
        Err(e) => rt.fail(case, &e),
    };
    let case = "rt.sdk-parity-analog-voltage-table";
    println!("== case {case}: the board table's AnalogVoltageFits against each row's maps -- internal-flash where the row's code region is at least {VOLTAGE_LIBRARY_FLOOR} B, external-flash where a shorter region has an external-flash map, no attribute elsewhere ==");
    match analog_voltage_table_check(repo) {
        Ok(rows) => {
            let words: Vec<String> = rows.iter().map(|(n, v)| format!("{n} {}", v.as_deref().unwrap_or("none"))).collect();
            println!("  {} rows: {}", rows.len(), words.join(", "));
            let internal = rows.iter().filter(|(_, v)| v.as_deref() == Some("internal-flash")).count();
            let external = rows.iter().filter(|(_, v)| v.as_deref() == Some("external-flash")).count();
            rt.pass(case, "positive", "", &format!("{{\"rows\":{},\"internal_flash_rows\":{internal},\"external_flash_rows\":{external}}}", rows.len()));
            println!("  PASS");
        }
        Err(e) => rt.fail(case, &e),
    }
    let case = "rt.sdk-parity-program-memory-external";
    let (xip_map, xip_second) = match xip_rows.iter().find(|(n, _, _)| n == XIP_ROW) {
        Some((_, m, Some(s))) => (m.clone(), s.clone()),
        _ => rt.fail(case, &format!("the board table names no external-flash map for the {XIP_ROW} row")),
    };
    let xip_facts = match read_facts_file(&repo.join(format!("src/sdk/Lamella.Rhu.Sdk/boards/{XIP_ROW}.json").replace('/', std::path::MAIN_SEPARATOR_STR))) {
        Ok(f) => f,
        Err(e) => rt.fail(case, &e),
    };
    let Some(((chip_base, chip_bytes), (win_base, win_bytes))) = facts_flash_ranges(&xip_facts) else {
        rt.fail(case, &format!("the facts file of {XIP_ROW} lists no chip flash range beside a mapped, executable external one"));
    };
    let win_end = win_base + win_bytes;
    let xip_kernel = format!("kernels/{XIP_ROW}/librhu_kernel.a");
    println!("== case {case}: the gate's project published for {XIP_ROW} with RhuProgramMemory=external -- the object the record's, the board line naming {xip_second} and {xip_kernel}, the image linked silent with no undefined name, the window's brackets inside the external flash range and its sections in the section table, the native code inside the chip's flash ==");
    let Some(line) = control_line(&controls, "program-memory-external") else {
        rt.fail(case, "the publish-side script left no summary line for the external publish");
    };
    if control_field(line, "rc") != "0" || control_field(line, "elf") != "present" {
        rt.fail(case, &format!("the publish did not produce the image: {line}"));
    }
    let obj = std::fs::read(out.join("control-program-memory-external").join("rthello.o")).unwrap_or_default();
    if obj != frozen_obj {
        rt.fail(case, &format!("the object published with the program's home external is not the record's ({} bytes against {}): the home reached the compile, which it must not", obj.len(), frozen_obj.len()));
    }
    if control_field(line, "map") != xip_second || control_field(line, "kernel") != xip_kernel || control_field(line, "program") != "external" || control_field(line, "window-notice") != "1" {
        rt.fail(case, &format!("the board line did not name the row's external-flash map, its default kernel and the home external with the window half's notice once: {line}"));
    }
    let xip_elf = out.join("control-program-memory-external").join("rthello.elf");
    let xip_elf_arg = xip_elf.to_string_lossy().into_owned();
    let undefined = rt.tool_text(&nm_tool, case, &["-u", &xip_elf_arg], "sdk-parity-program-memory-external.nm-u.txt");
    if let Some(l) = undefined.lines().find(|l| l.split_whitespace().next() == Some("U")) {
        rt.fail(case, &format!("the image carries a strong undefined name: {l}"));
    }
    let weak_undefined = undefined.lines().filter(|l| !l.trim().is_empty()).count();
    let dump = rt.tool_text(&nm_tool, case, &[&xip_elf_arg], "sdk-parity-program-memory-external.nm.txt");
    let xs = match xip_symbols(dump.as_bytes()) {
        Ok(s) => s,
        Err(e) => rt.fail(case, &e),
    };
    if !(xs.lo > win_base && xs.hi > xs.lo && xs.hi <= win_end) {
        rt.fail(case, &format!("the window's brackets [{:#x}, {:#x}) lie outside the external flash range [{win_base:#x}, {win_end:#x}) or are empty", xs.lo, xs.hi));
    }
    if !(xs.code_lo >= xs.lo && xs.code_hi > xs.code_lo && xs.unbox_hi <= xs.hi && xs.exidx_start >= xs.lo && xs.exidx_end > xs.exidx_start && xs.exidx_end <= xs.hi) {
        rt.fail(case, &format!("the managed code [{:#x}, {:#x}), the stubs' end {:#x} or the index [{:#x}, {:#x}) lie outside the window [{:#x}, {:#x}) or are empty", xs.code_lo, xs.code_hi, xs.unbox_hi, xs.exidx_start, xs.exidx_end, xs.lo, xs.hi));
    }
    if !(xs.stext >= chip_base && xs.etext <= chip_base + chip_bytes && xs.etext > xs.stext) {
        rt.fail(case, &format!("the native code [{:#x}, {:#x}) lies outside the chip's flash range [{chip_base:#x}, {:#x})", xs.stext, xs.etext, chip_base + chip_bytes));
    }
    let readobj = match llvm_tool(&facts, "llvm-readobj") {
        Ok(p) => p,
        Err(e) => rt.fail(case, &e),
    };
    let sections = rt.tool_text(&readobj, case, &["--elf-output-style=GNU", "-S", "-W", &xip_elf_arg], "sdk-parity-program-memory-external.sections.txt");
    let (Some((rodata_type, rodata_addr, rodata_size)), Some((exidx_type, exidx_addr, exidx_size)), Some((module0_type, _, _))) = (section_row(&sections, ".rhu_xip_rodata"), section_row(&sections, ".rhu_xip_exidx"), section_row(&sections, ".ARM.exidx")) else {
        rt.fail(case, "the section table lacks .rhu_xip_rodata, .rhu_xip_exidx or .ARM.exidx");
    };
    if rodata_type != "PROGBITS" || rodata_size == 0 || rodata_addr < win_base || rodata_addr + rodata_size > win_end {
        rt.fail(case, &format!(".rhu_xip_rodata is {rodata_type} of {rodata_size} B at {rodata_addr:#x}: not the program object's read-only data placed in the window"));
    }
    if exidx_type != "PROGBITS" || exidx_size == 0 || exidx_addr != xs.exidx_start {
        rt.fail(case, &format!(".rhu_xip_exidx is {exidx_type} of {exidx_size} B at {exidx_addr:#x}: not the retyped index at the bracket {:#x}", xs.exidx_start));
    }
    if module0_type != "ARM_EXIDX" {
        rt.fail(case, &format!(".ARM.exidx is {module0_type}: the native code's own index is not the exception-index table"));
    }
    println!("  the window [{:#x}, {:#x}) inside the external flash range: the managed code [{:#x}, {:#x}), the stubs to {:#x}, the read-only data {rodata_size} B at {rodata_addr:#x}, the index {exidx_size} B at {exidx_addr:#x} retyped PROGBITS; the native code [{:#x}, {:#x}) inside the chip's flash with its own ARM_EXIDX table; no strong undefined name ({weak_undefined} weak); the object the record's; the board line: map {xip_second}, kernel {xip_kernel}, the home external, the window half's notice", xs.lo, xs.hi, xs.code_lo, xs.code_hi, xs.unbox_hi, xs.stext, xs.etext);
    let xip_sha = sha256_file(&xip_elf).unwrap_or_default();
    rt.pass(case, "positive", "", &format!("{{\"map\":\"{xip_second}\",\"image_sha\":\"{xip_sha}\",\"xip_lo\":{},\"xip_hi\":{},\"managed_lo\":{},\"managed_hi\":{},\"exidx_start\":{},\"exidx_end\":{},\"rodata_bytes\":{rodata_size},\"exidx_bytes\":{exidx_size}}}", xs.lo, xs.hi, xs.code_lo, xs.code_hi, xs.exidx_start, xs.exidx_end));
    println!("  PASS (the image {}...)", &xip_sha[..16.min(xip_sha.len())]);
    let case = "rt.sdk-parity-program-memory-default";
    println!("== case {case}: the row published with no property and with RhuProgramMemory=internal named -- no property is the image of the external home named, byte for byte, under {xip_second} with the window half's notice; the chip's own flash named holds the whole image under {xip_map}, no window declared; the objects the record's ==");
    let Some(plain_line) = control_line(&controls, "program-memory-default") else {
        rt.fail(case, "the publish-side script left no summary line for the publish with no property");
    };
    let Some(named_line) = control_line(&controls, "program-memory-internal-named") else {
        rt.fail(case, "the publish-side script left no summary line for the publish with the chip's own flash named");
    };
    if control_field(plain_line, "rc") != "0" || control_field(plain_line, "elf") != "present" || control_field(plain_line, "map") != xip_second || control_field(plain_line, "kernel") != xip_kernel || control_field(plain_line, "program") != "external" || control_field(plain_line, "window-notice") != "1" {
        rt.fail(case, &format!("the publish with no property did not run as registered (rc 0, the image present, the row's external-flash map, its default kernel, the home external, the window half's notice once): {plain_line}"));
    }
    if control_field(named_line, "rc") != "0" || control_field(named_line, "elf") != "present" || control_field(named_line, "map") != xip_map || control_field(named_line, "kernel") != xip_kernel || control_field(named_line, "program") != "internal" || control_field(named_line, "window-notice") != "0" {
        rt.fail(case, &format!("the publish with the chip's own flash named did not run as registered (rc 0, the image present, the row's own map, its default kernel, the home internal, no window notice): {named_line}"));
    }
    for (label, folder) in [("no property", "control-program-memory-default"), ("the chip's own flash named", "control-program-memory-internal-named")] {
        let o = std::fs::read(out.join(folder).join("rthello.o")).unwrap_or_default();
        if o != frozen_obj {
            rt.fail(case, &format!("the object published with {label} is not the record's ({} bytes against {})", o.len(), frozen_obj.len()));
        }
    }
    let plain_elf = out.join("control-program-memory-default").join("rthello.elf");
    let plain = std::fs::read(&plain_elf).unwrap_or_default();
    let external = std::fs::read(&xip_elf).unwrap_or_default();
    if plain.is_empty() || plain != external {
        rt.fail(case, &format!("the image published with no property ({} bytes) is not the image of the external home named ({} bytes)", plain.len(), external.len()));
    }
    let named_elf = out.join("control-program-memory-internal-named").join("rthello.elf");
    let named_elf_arg = named_elf.to_string_lossy().into_owned();
    let dump = rt.tool_text(&nm_tool, case, &[&named_elf_arg], "sdk-parity-program-memory-internal.nm.txt");
    let (Some(lo), Some(hi)) = (word_sym_value(dump.as_bytes(), "__rhu_xip_lo"), word_sym_value(dump.as_bytes(), "__rhu_xip_hi")) else {
        rt.fail(case, "the image with the chip's own flash named does not define the window's brackets (the row's own map provides them as zero)");
    };
    if lo != 0 || hi != 0 {
        rt.fail(case, &format!("the image with the chip's own flash named declares a window [{lo:#x}, {hi:#x}); the row's own map provides none"));
    }
    let plain_sha = sha256_file(&plain_elf).unwrap_or_default();
    let named_sha = sha256_file(&named_elf).unwrap_or_default();
    rt.pass(case, "positive", "", &format!("{{\"map\":\"{xip_second}\",\"image_sha\":\"{plain_sha}\",\"bytes\":{},\"internal_map\":\"{xip_map}\",\"internal_image_sha\":\"{named_sha}\"}}", plain.len()));
    println!("  PASS (no property: the image of the external home named, {} bytes, {}...; the chip's own flash named: the whole image under the row's own map, no window declared, {}...; the objects the record's)", plain.len(), &plain_sha[..16.min(plain_sha.len())], &named_sha[..16.min(named_sha.len())]);

    // 10d. The analog voltage switch, set per program by the SDK: the switch's
    // program (tbvolts, naming the voltage members) published for the
    // Pimoroni Pico Plus 2 row -- the switch true with the scan's witness,
    // the compiler's feature argument and the knob once each in the response
    // file, the library's quantity type among the image's symbols; for this
    // leg's row -- false (the row does not hold the library), no feature
    // argument, no such symbol, the image booted on the emulator to the
    // program's lines; for the external-flash row under either home -- true
    // under external with the library in the window, false with the chip's
    // own flash named, the words naming the external home; the board class's
    // program (no voltage member) for the Plus 2 row -- false by the scan's
    // negative; the switch's program with the project's own false for the
    // Plus 2 row -- the project's value kept.
    let case = "rt.sdk-parity-analog-voltage-true";
    println!("== case {case}: the switch's program published for {RAM_INIT_ROW} -- the switch true with the scan's witness, the feature argument and the knob once each in the response file, the library's quantity type among the image's symbols ==");
    let true_line = voltage_publish_line(&mut rt, case, &controls, "analog-voltage-plus2");
    let true_words = voltage_words(true_line);
    if !true_words.starts_with("true (") || !VOLTAGE_MEMBERS.iter().any(|m| true_words.contains(&format!("({m} referenced by tbvolts)"))) {
        rt.fail(case, &format!("the switch's line does not read true with the scan's witness: {true_words}"));
    }
    if control_field(true_line, "feature") != "1" || control_field(true_line, "knob") != "1" {
        rt.fail(case, &format!("the response file does not carry the feature argument and the knob once each: {true_line}"));
    }
    let true_elf = out.join("control-analog-voltage-plus2").join("tbvolts.elf");
    let true_names = voltage_type_names(&mut rt, case, &nm_tool, &true_elf, "sdk-parity-analog-voltage-plus2.nm.txt");
    if true_names == 0 {
        rt.fail(case, "the image carries no symbol of the library's quantity type: the switch did not reach the compile");
    }
    let true_sha = sha256_file(&true_elf).unwrap_or_default();
    rt.pass(case, "positive", "", &format!("{{\"board\":\"{RAM_INIT_ROW}\",\"image_sha\":\"{true_sha}\",\"type_names\":{true_names}}}"));
    println!("  PASS (analog voltage {true_words}; {true_names} symbols of the quantity type; the image {}...)", &true_sha[..16.min(true_sha.len())]);
    let case = "rt.sdk-parity-analog-voltage-row";
    println!("== case {case}: the switch's program published for this leg's row {} -- the switch false (the row does not hold the library), the feature argument false once, no symbol of the quantity type, the image booted on the emulator to the program's lines ==", w.sdk_board);
    let row_line = voltage_publish_line(&mut rt, case, &controls, "analog-voltage-row");
    let row_words = voltage_words(row_line);
    if !row_words.starts_with("false (") || !row_words.contains("but the board does not hold the library") || !VOLTAGE_MEMBERS.iter().any(|m| row_words.contains(&format!("({m} referenced by tbvolts, but"))) {
        rt.fail(case, &format!("the switch's line does not read false with the scan's witness and the row's reason: {row_words}"));
    }
    if control_field(row_line, "feature") != "0" || control_field(row_line, "feature-false") != "1" || control_field(row_line, "knob") != "0" {
        rt.fail(case, &format!("the response file does not carry the feature argument false once and no knob for the switch: {row_line}"));
    }
    let row_elf = out.join("control-analog-voltage-row").join("tbvolts.elf");
    let row_names = voltage_type_names(&mut rt, case, &nm_tool, &row_elf, "sdk-parity-analog-voltage-row.nm.txt");
    if row_names != 0 {
        rt.fail(case, &format!("the image carries {row_names} symbols of the library's quantity type with the switch false"));
    }
    let (qemu, qemu_version) = match resolve_qemu(repo) {
        Ok(q) => q,
        Err(e) => rt.fail(case, &e),
    };
    let row_elf_arg = row_elf.to_string_lossy().into_owned();
    let raw_path = out.join("analog-voltage-row-boot.raw");
    let stderr_path = out.join("analog-voltage-row-boot.stderr");
    let mut qargs: Vec<&str> = vec!["-M", w.machine, "-display", "none", "-monitor", "none", "-serial", "stdio", "-semihosting"];
    qargs.extend_from_slice(ExecProfile::Parity.qemu_args());
    qargs.push("-kernel");
    qargs.push(&row_elf_arg);
    let outcome = match run_captured(&qemu, &qargs, &raw_path, &stderr_path, Duration::from_secs(RUN_TIMEOUT_S)) {
        Ok(o) => o,
        Err(e) => rt.fail(case, &e),
    };
    let raw = std::fs::read(&raw_path).unwrap_or_default();
    echo_file(&raw_path, "  | ");
    if outcome.timed_out {
        rt.fail(case, "the switch's program's boot hung (killed at its timeout)");
    }
    if cr_count(&raw) == 0 {
        rt.fail(case, "no CR bytes in raw capture (wire discipline lost)");
    }
    let switch_line = format!("TBVOLTS: {}; the switch false", w.sdk_board);
    let volts_markers: [&str; 4] = [&switch_line, "TBVOLTS: pin 0 reads ", "TBVOLTS: NotSupportedException: ", "TBVOLTS: end"];
    if let Some(fault) = markers_out_of_order(&raw, &volts_markers) {
        rt.fail(case, &format!("the switch's program's lines: {fault}"));
    }
    if !contains_fixed(&raw, " of 12 bits") {
        rt.fail(case, "the count line does not name the converter's 12 bits");
    }
    if !contains_fixed(&raw, BOOT_RETURN_LINE) {
        rt.fail(case, &format!("the kernel's return line '{BOOT_RETURN_LINE}' not found"));
    }
    for m in FORBIDDEN_PATTERNS {
        if contains_fixed(&raw, m) {
            rt.fail(case, &format!("forbidden pattern '{m}' present"));
        }
    }
    if outcome.exit != 0 {
        rt.fail(case, &format!("exit {}, wanted 0", outcome.exit));
    }
    let row_sha = sha256_file(&row_elf).unwrap_or_default();
    rt.pass_image(case, 0, &format!("{out_rel}/control-analog-voltage-row/tbvolts.elf"), &row_sha, None, &format!("{{\"board\":\"{}\",\"type_names\":0}}", w.sdk_board));
    println!("  PASS (analog voltage {row_words}; no symbol of the quantity type; the image {}... booted to the program's four lines; {qemu_version})", &row_sha[..16.min(row_sha.len())]);
    let case = "rt.sdk-parity-analog-voltage-unreferenced";
    println!("== case {case}: the board class's program (no voltage member) published for {RAM_INIT_ROW} -- the switch false by the scan's negative, the feature argument false once, no symbol of the quantity type ==");
    let un_line = voltage_publish_line(&mut rt, case, &controls, "analog-voltage-unreferenced");
    let un_words = voltage_words(un_line);
    if !un_words.starts_with("false (no voltage member referenced by the program)") {
        rt.fail(case, &format!("the switch's line does not read false by the scan's negative: {un_words}"));
    }
    if control_field(un_line, "feature") != "0" || control_field(un_line, "feature-false") != "1" || control_field(un_line, "knob") != "0" {
        rt.fail(case, &format!("the response file does not carry the feature argument false once and no knob for the switch: {un_line}"));
    }
    let un_elf = out.join("control-analog-voltage-unreferenced").join("tbhello.elf");
    let un_names = voltage_type_names(&mut rt, case, &nm_tool, &un_elf, "sdk-parity-analog-voltage-unreferenced.nm.txt");
    if un_names != 0 {
        rt.fail(case, &format!("the image carries {un_names} symbols of the library's quantity type with no voltage member referenced"));
    }
    let un_sha = sha256_file(&un_elf).unwrap_or_default();
    rt.pass(case, "positive", "", &format!("{{\"board\":\"{RAM_INIT_ROW}\",\"image_sha\":\"{un_sha}\",\"type_names\":0}}"));
    println!("  PASS (analog voltage {un_words}; no symbol of the quantity type; the image {}...)", &un_sha[..16.min(un_sha.len())]);
    let case = "rt.sdk-parity-analog-voltage-external-flash";
    println!("== case {case}: the switch's program published for {XIP_ROW} with RhuProgramMemory=external and RhuNetwork=off (beside the kernel with the network this program's data leaves no room, which its control shows) -- the switch true with the scan's witness, the feature argument and the knob once each, the library's quantity type among the image's symbols, the managed code in the window ==");
    let ext_line = voltage_publish_line(&mut rt, case, &controls, "analog-voltage-external-flash");
    let ext_words = voltage_words(ext_line);
    if !ext_words.starts_with("true (") || !VOLTAGE_MEMBERS.iter().any(|m| ext_words.contains(&format!("({m} referenced by tbvolts)"))) {
        rt.fail(case, &format!("the switch's line does not read true with the scan's witness: {ext_words}"));
    }
    if control_field(ext_line, "feature") != "1" || control_field(ext_line, "knob") != "1" {
        rt.fail(case, &format!("the response file does not carry the feature argument and the knob once each: {ext_line}"));
    }
    let ext_elf = out.join("control-analog-voltage-external-flash").join("tbvolts.elf");
    let ext_names = voltage_type_names(&mut rt, case, &nm_tool, &ext_elf, "sdk-parity-analog-voltage-external-flash.nm.txt");
    if ext_names == 0 {
        rt.fail(case, "the image carries no symbol of the library's quantity type: the switch did not reach the compile");
    }
    let ext_elf_arg = ext_elf.to_string_lossy().into_owned();
    let dump = rt.tool_text(&nm_tool, case, &[&ext_elf_arg], "sdk-parity-analog-voltage-external-flash.syms.txt");
    let es = match xip_symbols(dump.as_bytes()) {
        Ok(s) => s,
        Err(e) => rt.fail(case, &e),
    };
    if !(es.lo > win_base && es.code_lo >= es.lo && es.code_hi > es.code_lo && es.hi <= win_end && es.hi - es.lo > xs.hi - xs.lo) {
        rt.fail(case, &format!("the window [{:#x}, {:#x}) with the managed code [{:#x}, {:#x}) is not inside the external flash range, or not larger than the gate's program's window", es.lo, es.hi, es.code_lo, es.code_hi));
    }
    let ext_sha = sha256_file(&ext_elf).unwrap_or_default();
    rt.pass(case, "positive", "", &format!("{{\"board\":\"{XIP_ROW}\",\"image_sha\":\"{ext_sha}\",\"type_names\":{ext_names},\"xip_lo\":{},\"xip_hi\":{}}}", es.lo, es.hi));
    println!("  PASS (analog voltage {ext_words}; {ext_names} symbols of the quantity type; the window [{:#x}, {:#x}) in the external flash; the image {}...)", es.lo, es.hi, &ext_sha[..16.min(ext_sha.len())]);
    let case = "rt.sdk-parity-analog-voltage-external-flash-default";
    println!("== case {case}: the switch's program published for {XIP_ROW} with the chip's own flash named (RhuProgramMemory=internal) -- the switch false with the words naming RhuProgramMemory=external, the feature argument false once, no symbol of the quantity type ==");
    let def_line = voltage_publish_line(&mut rt, case, &controls, "analog-voltage-external-flash-default");
    let def_words = voltage_words(def_line);
    if !def_words.starts_with("false (") || !def_words.contains("RhuProgramMemory=external places the program in the board's external flash") {
        rt.fail(case, &format!("the switch's line does not read false with the external home named: {def_words}"));
    }
    if control_field(def_line, "feature") != "0" || control_field(def_line, "feature-false") != "1" || control_field(def_line, "knob") != "0" {
        rt.fail(case, &format!("the response file does not carry the feature argument false once and no knob for the switch: {def_line}"));
    }
    let def_elf = out.join("control-analog-voltage-external-flash-default").join("tbvolts.elf");
    let def_names = voltage_type_names(&mut rt, case, &nm_tool, &def_elf, "sdk-parity-analog-voltage-external-flash-default.nm.txt");
    if def_names != 0 {
        rt.fail(case, &format!("the image carries {def_names} symbols of the library's quantity type with the chip's own flash named"));
    }
    let def_sha = sha256_file(&def_elf).unwrap_or_default();
    rt.pass(case, "positive", "", &format!("{{\"board\":\"{XIP_ROW}\",\"image_sha\":\"{def_sha}\",\"type_names\":0}}"));
    println!("  PASS (analog voltage {def_words}; no symbol of the quantity type; the image {}...)", &def_sha[..16.min(def_sha.len())]);
    let case = "rt.sdk-parity-analog-voltage-own";
    println!("== case {case}: the switch's program published for {RAM_INIT_ROW} with the project's own option false -- the project's value kept, its feature argument false once and nothing of the SDK's, no symbol of the quantity type ==");
    let own_line = voltage_publish_line(&mut rt, case, &controls, "analog-voltage-own-false");
    let own_words = voltage_words(own_line);
    if !own_words.starts_with("false (the project's own option)") {
        rt.fail(case, &format!("the switch's line does not read false by the project's own option: {own_words}"));
    }
    if control_field(own_line, "feature") != "0" || control_field(own_line, "feature-false") != "1" || control_field(own_line, "knob") != "0" {
        rt.fail(case, &format!("the response file does not carry the project's own feature argument false once and nothing of the SDK's: {own_line}"));
    }
    let own_elf = out.join("control-analog-voltage-own-false").join("tbvolts.elf");
    let own_names = voltage_type_names(&mut rt, case, &nm_tool, &own_elf, "sdk-parity-analog-voltage-own-false.nm.txt");
    if own_names != 0 {
        rt.fail(case, &format!("the image carries {own_names} symbols of the library's quantity type with the project's own option false"));
    }
    let own_sha = sha256_file(&own_elf).unwrap_or_default();
    rt.pass(case, "positive", "", &format!("{{\"board\":\"{RAM_INIT_ROW}\",\"image_sha\":\"{own_sha}\",\"type_names\":0}}"));
    println!("  PASS (analog voltage {own_words}; no symbol of the quantity type; the image {}...)", &own_sha[..16.min(own_sha.len())]);

    // 11. The kernel's output level and the two streams' channels (the
    // log word's property half): the board table's channel facts tied to the boards'
    // facts files; then the three link-time words read from the images as
    // absolute symbols -- the leg's own image at verbose (the gate project's
    // own posture), silent named, the SDK's default from a project copy
    // without the line, one channel named with the other left to the
    // kernel's PROVIDE, both named, a channel named on the external-RAM row
    // -- each publish's object the record's (the words are link-time only)
    // and the board line's words the effective choice.
    let case = "rt.sdk-parity-channel-facts";
    println!("== case {case}: the board table's channel facts against the boards' facts files -- every row's two default channels and its legal set the facts file's channels row, in order ==");
    let channel_rows = match channel_facts_check(repo) {
        Ok(rows) => {
            let words: Vec<String> = rows.iter().map(|(n, l, c, k)| format!("{n} {l}/{c} ({k})")).collect();
            println!("  {} rows tie their facts files: {}", rows.len(), words.join(", "));
            rt.pass(case, "positive", "", &format!("{{\"rows\":{}}}", rows.len()));
            println!("  PASS");
            rows
        }
        Err(e) => rt.fail(case, &e),
    };
    let (board_log, board_console) = match channel_rows.iter().find(|r| r.0 == w.sdk_board) {
        Some(r) => (r.1.clone(), r.2.clone()),
        None => rt.fail(case, &format!("the board table has no row for this leg's board {}", w.sdk_board)),
    };
    let ram_log = match channel_rows.iter().find(|r| r.0 == RAM_INIT_ROW) {
        Some(r) => r.1.clone(),
        None => rt.fail(case, &format!("the board table has no row for {RAM_INIT_ROW}")),
    };
    let nm_tool = match llvm_tool(&facts, "llvm-nm") {
        Ok(p) => p,
        Err(e) => rt.fail("rt.sdk-parity-log-verbose", &e),
    };
    let word_cases: [(&str, &str, u64, u64, u64, &str, &str, &str); 6] = [
        ("log-verbose", "", 3, 0, 0, "verbose", &board_log, &board_console),
        ("log-silent", "control-log-silent", 1, 0, 0, "silent", &board_log, &board_console),
        ("log-default", "control-log-default", 2, 0, 0, "quiet", &board_log, &board_console),
        ("channel-off", "control-channel-off", 3, 1, 0, "verbose", "off", &board_console),
        ("channel-both-memory", "control-channel-both-memory", 3, 2, 2, "verbose", "memory", "memory"),
        ("channel-console-uart1", "control-channel-console-uart1", 3, 0, 17, "verbose", &ram_log, "uart1"),
    ];
    for (label, dir, level, log_code, console_code, level_word, log_word, console_word) in word_cases {
        let case = format!("rt.sdk-parity-{label}");
        println!("== case {case}: the image's three link-time words -- __rhu_log_level {level}, __rhu_log_channel {log_code}, __rhu_console_channel {console_code}; the board line 'log {level_word} to {log_word}; console to {console_word}'; the object the record's ==");
        let Some(line) = control_line(&controls, label) else {
            rt.fail(&case, "the publish-side script left no summary line for this publish");
        };
        if control_field(line, "rc") != "0" || control_field(line, "elf") != "present" {
            rt.fail(&case, &format!("the publish did not produce the image: {line}"));
        }
        if control_field(line, "log") != level_word || control_field(line, "logch") != log_word || control_field(line, "conch") != console_word {
            rt.fail(&case, &format!("the board line's words are not 'log {level_word} to {log_word}; console to {console_word}': {line}"));
        }
        let elf = if dir.is_empty() { out.join("rthello.elf") } else { out.join(dir).join("rthello.elf") };
        if !dir.is_empty() {
            let bytes = std::fs::read(out.join(dir).join("rthello.o")).unwrap_or_default();
            if bytes != frozen_obj {
                rt.fail(&case, &format!("the object published is not the record's ({} bytes against {}): the property reached the compile, which it must not", bytes.len(), frozen_obj.len()));
            }
        }
        let elf_arg = elf.to_string_lossy().into_owned();
        let dump = rt.tool_text(&nm_tool, &case, &[&elf_arg], &format!("sdk-parity-{label}.nm.txt"));
        for (sym, want) in [("__rhu_log_level", level), ("__rhu_log_channel", log_code), ("__rhu_console_channel", console_code)] {
            match word_sym_value(dump.as_bytes(), sym) {
                Some(v) if v == want => {}
                Some(v) => rt.fail(&case, &format!("the image defines {sym} = {v}, not {want}")),
                None => rt.fail(&case, &format!("the image does not define {sym}")),
            }
        }
        let sha = sha256_file(&elf).unwrap_or_default();
        rt.pass(&case, "positive", "", &format!("{{\"level\":{level},\"log_channel\":{log_code},\"console_channel\":{console_code},\"image_sha\":\"{sha}\"}}"));
        println!("  PASS (__rhu_log_level {level}, __rhu_log_channel {log_code}, __rhu_console_channel {console_code}; the words as registered; the object the record's; the image {}...)", &sha[..16.min(sha.len())]);
    }

    // 12. The run verb: the SDK's two run targets and the manifest,
    // driven through the real dotnet run in the Linux environment with a
    // stand-in tool and a stand-in emulator the script writes: the manifest
    // beside the image
    // byte for byte against the expectation composed here from the image's
    // own facts; the device list as .NET renders it; the run's command line
    // and its exit code returned unchanged; then the controls.
    let case = "rt.sdk-parity-manifest";
    println!("== case {case}: the manifest the publish wrote beside the image, byte for byte against the expectation composed from the image's own facts (its sha256 and size, the header's words, the board row's channels, the runtime pin) ==");
    let manifest_path = out.join("rthello.elf.manifest.json");
    let manifest = std::fs::read(&manifest_path).unwrap_or_default();
    if manifest.is_empty() {
        rt.fail(case, "the publish left no rthello.elf.manifest.json beside the image under sdk-parity/");
    }
    let elf_bytes = std::fs::read(&fresh_elf).unwrap_or_default();
    let fresh_elf_arg = fresh_elf.to_string_lossy().into_owned();
    let dump = rt.tool_text(&nm_tool, case, &[&fresh_elf_arg], "sdk-parity-manifest.nm.txt");
    let records = match check_records(dump.as_bytes(), &elf_bytes, rt.abi) {
        Ok(r) => r,
        Err(e) => rt.fail(case, &format!("the image's records: {e}")),
    };
    let runtime_pin = match sdk_runtime_pin(repo) {
        Ok(v) => v,
        Err(e) => rt.fail(case, &e),
    };
    let expected = manifest_text(w.sdk_board, &sha256_hex(&elf_bytes), elf_bytes.len(), records.abi, records.layout_version, &runtime_pin, &board_console, &board_log);
    let expected_path = out.join("manifest.expected.json");
    if std::fs::write(&expected_path, expected.as_bytes()).is_err() {
        rt.fail(case, &format!("cannot write {}", expected_path.display()));
    }
    if manifest != expected.as_bytes() {
        let got = String::from_utf8_lossy(&manifest).into_owned();
        let diff = got
            .lines()
            .zip(expected.lines())
            .enumerate()
            .find(|(_, (a, b))| a != b)
            .map(|(i, (a, b))| format!("line {}: '{a}' against '{b}'", i + 1))
            .unwrap_or_else(|| format!("{} bytes against {}", manifest.len(), expected.len()));
        rt.fail(case, &format!("the manifest is not the expectation byte for byte ({diff}); the expectation at {}", expected_path.display()));
    }
    println!("  the record ties the expectation byte for byte ({} bytes): the image's sha256 {}..., {} bytes; abi {}; debug layout {}; console {board_console}, log {board_log}; runtime {runtime_pin}, tested", manifest.len(), &sha256_hex(&elf_bytes)[..16], elf_bytes.len(), records.abi, records.layout_version);
    rt.pass(case, "positive", "", &format!("{{\"bytes\":{},\"abi\":{},\"debug_layout_version\":{}}}", manifest.len(), records.abi, records.layout_version));
    println!("  PASS");
    let case = "rt.sdk-parity-control-manifest-schema";
    println!("== case {case}: the record with its schema moved to 2 -- the comparison must refuse it, and the stand-in tool's run must refuse it before anything is opened ==");
    let doctored = expected.replacen("\"schema\": 1,", "\"schema\": 2,", 1);
    if doctored == expected || doctored.as_bytes() == manifest.as_slice() {
        rt.fail_control(case, "manifest-schema", "the doctored record is the expectation: the control would not bite");
    }
    let Some(line) = control_line(&controls, "manifest-schema") else {
        rt.fail_control(case, "manifest-schema", "the publish-side script left no summary line for the schema control");
    };
    let refuse = line.split_once("refuse=").map(|(_, r)| r.trim()).unwrap_or("");
    if control_field(line, "rc") != "1" || !refuse.contains("schema") {
        rt.fail_control(case, "manifest-schema", &format!("the stand-in's run did not refuse the schema-2 record as registered: {line}"));
    }
    println!("  the comparison refuses the moved schema; the stand-in's run refused it (rc 1): {refuse}");
    rt.pass(case, "negative", "manifest-schema", "");
    println!("  PASS");
    let case = "rt.sdk-parity-control-manifest-magic";
    println!("== case {case}: the manifest task run on a copy of the image with its header magic flipped -- it must refuse naming the magic, no record written ==");
    let Some(line) = control_line(&controls, "manifest-magic") else {
        rt.fail_control(case, "header-magic", "the publish-side script left no summary line for the magic control");
    };
    let refuse = line.split_once("refuse=").map(|(_, r)| r.trim()).unwrap_or("");
    if control_field(line, "rc") == "0" || control_field(line, "manifest") != "absent" || !refuse.contains("header magic") {
        rt.fail_control(case, "header-magic", &format!("the doctored image was not refused as registered: {line}"));
    }
    println!("  refused (rc {}, no record): {refuse}", control_field(line, "rc"));
    rt.pass(case, "negative", "header-magic", "");
    println!("  PASS");
    let case = "rt.sdk-parity-control-manifest-model-absent";
    println!("== case {case}: the record with its model line removed -- the stand-in tool's run must refuse it by name before anything is opened (a schema 1 record states the execution model its image runs under) ==");
    let Some(line) = control_line(&controls, "manifest-model-absent") else {
        rt.fail_control(case, "manifest-model", "the publish-side script left no summary line for the model control");
    };
    let refuse = line.split_once("refuse=").map(|(_, r)| r.trim()).unwrap_or("");
    if control_field(line, "rc") != "1" || control_field(line, "model-lines") != "0" || !refuse.contains("no model string") {
        rt.fail_control(case, "manifest-model", &format!("the stand-in's run did not refuse the record without a model as registered: {line}"));
    }
    println!("  refused (rc 1, nothing opened): {refuse}");
    rt.pass(case, "negative", "manifest-model", "");
    println!("  PASS");
    let case = "rt.sdk-parity-manifest-unknown-field";
    println!("== case {case}: the record with a field the reader does not know added after the model -- the stand-in's run accepts it and reads the model native (the shell's reader ignores a field it does not know) ==");
    let Some(line) = control_line(&controls, "manifest-unknown-field") else {
        rt.fail(case, "the publish-side script left no summary line for the unknown-field run");
    };
    if control_field(line, "rc") != "7" || control_field(line, "model") != "native" || control_field(line, "accepted") != "1" || control_field(line, "keys") != "22" {
        rt.fail(case, &format!("the stand-in's run did not accept the record with an unknown field as registered (rc 7, the model native, 22 keys): {line}"));
    }
    println!("  accepted (rc 7, the stand-in's code): the model native read, the unknown field ignored");
    rt.pass(case, "positive", "", "");
    println!("  PASS");
    let case = "rt.sdk-parity-debug-line";
    println!("== case {case}: the line tables of the published image -- every .debug_line unit read whole a second way with no backslash in a directory or file-name string (llvm-dwarfdump's reading beside it where the toolchain carries it), and the separator task run again over a copy: nothing to rewrite, the bytes identical ==");
    let Some(line) = control_line(&controls, "debug-line") else {
        rt.fail(case, "the publish-side script left no summary line for the line tables");
    };
    let units = control_field(line, "units");
    let dwarfdump = control_field(line, "dwarfdump");
    let dwarfdump_ok = dwarfdump == "absent" || dwarfdump == format!("rc0,units{units},errors0");
    if control_field(line, "rc") != "0" || units.is_empty() || units == "0" || control_field(line, "backslashes") != "0" || !dwarfdump_ok || control_field(line, "again-rc") != "0" || control_field(line, "again-units") != units || control_field(line, "again-rewritten") != "0" || control_field(line, "before").is_empty() || control_field(line, "before") != control_field(line, "after") {
        rt.fail(case, &format!("the line tables are not as registered (every unit read, no backslash, the task's second run rewriting nothing with the bytes identical): {line}"));
    }
    println!("  {units} units read whole, {} strings, no backslash; llvm-dwarfdump {dwarfdump}; the task again: {units} units, 0 rewritten, the image byte-identical ({}...)", control_field(line, "strings"), control_field(line, "after"));
    rt.pass(case, "positive", "", &format!("{{\"units\":{units}}}"));
    println!("  PASS");
    let case = "rt.sdk-parity-control-debug-line-doctored";
    println!("== case {case}: the separator task over a copy of the image with its first line-table unit's version set to 9 -- refused by RHU5002 by name, the copy untouched ==");
    let Some(line) = control_line(&controls, "debug-line-doctored") else {
        rt.fail_control(case, "line-table-version", "the publish-side script left no summary line for the doctored line table");
    };
    let refuse = line.split_once("refuse=").map(|(_, r)| r.trim()).unwrap_or("");
    if control_field(line, "rc") == "0" || control_field(line, "code") != "RHU5002" || control_field(line, "untouched") != "yes" || !refuse.contains("version 9") {
        rt.fail_control(case, "line-table-version", &format!("the doctored line table was not refused as registered: {line}"));
    }
    println!("  refused (rc {}, RHU5002, the copy untouched): {refuse}", control_field(line, "rc"));
    rt.pass(case, "negative", "line-table-version", "");
    println!("  PASS");

    let machine_word = w.machine[5..].to_ascii_uppercase();
    let case = "rt.sdk-parity-devices";
    println!("== case {case}: dotnet run --list-devices through the SDK with the stand-in tool's two probes and the stand-in emulator -- the list as .NET renders it, line for line ==");
    let Some(line) = control_line(&controls, "run-devices") else {
        rt.fail(case, "the publish-side script left no summary line for the device list");
    };
    if control_field(line, "rc") != "0" {
        rt.fail(case, &format!("the device list did not exit 0: {line}"));
    }
    let listing = std::fs::read_to_string(out.join("run-run-devices.stdout")).unwrap_or_default();
    let block = devices_block(&listing);
    let want = format!(
        "Available devices:\n\n  1. probe:{RUN_PROBE_A} - Debug Probe (CMSIS-DAP) (Device, Connected)\n  2. probe:{RUN_PROBE_B} - {RUN_PROBE_B_WORDS} (Device, Connected)\n  3. emulator:{} - Arm MPS2+ {machine_word} reference board under QEMU (Emulator, Installed)\n\nExample: dotnet run --device probe:{RUN_PROBE_A}\n",
        w.machine
    );
    if block != want {
        rt.fail(case, &format!("the list is not the expectation:\n--- read\n{block}--- wanted\n{want}"));
    }
    println!("  the three rows as .NET renders them (the two probes with the escaped description intact, the emulator row with its description); the example names the first id");
    rt.pass(case, "positive", "", "{\"rows\":3}");
    println!("  PASS");
    for (label, broke, what, rows, none, emulator, warn, phrase) in [
        // (the controls tool-absent and tool-version are retired: they asserted RHU's resolution of the
        // lamella tool, which the contract moves to the shell, whose own tool is hash-pinned; RHU names no tool)
        ("run-devices-no-emulator", "no-emulator", "the emulator's folder named empty -- the two probes and no emulator row", "2", "0", "0", false, ""),
    ] {
        let case = format!("rt.sdk-parity-control-{label}");
        println!("== case {case}: {what} ==");
        let Some(line) = control_line(&controls, label) else {
            rt.fail_control(&case, broke, "the publish-side script left no summary line for this control");
        };
        if control_field(line, "rc") != "0" || control_field(line, "rows") != rows || control_field(line, "emulator") != emulator || (none == "1") != (control_field(line, "none") != "0") || warn != (control_field(line, "warn") != "0") {
            rt.fail_control(&case, broke, &format!("the list did not come back as registered (rows {rows}, the no-devices sentence {none}, emulator rows {emulator}, a warning {warn}): {line}"));
        }
        if !phrase.is_empty() {
            let text = std::fs::read_to_string(out.join(format!("run-{label}.stdout"))).unwrap_or_default() + &std::fs::read_to_string(out.join(format!("run-{label}.stderr"))).unwrap_or_default();
            if !text.contains(phrase) {
                rt.fail_control(&case, broke, &format!("the warning does not carry '{phrase}'"));
            }
        }
        println!("  as registered: {line}");
        rt.pass(&case, "negative", broke, "");
        println!("  PASS");
    }

    let case = "rt.sdk-parity-first-list";
    println!("== case {case}: dotnet run --list-devices on a copy of the gate's project from an empty package folder of the case's own, the two packages from a feed of the case's own seeded by copy, the pinned cache's other entries through a fallback folder of the case's own, the tool the tools package's own -- the one command installs the two packages and lists whole: the emulator row, no warning that the tools package is not restored (a package the fallback folder lacks is fetched: the count is a figure in the summary line, never a condition) ==");
    let Some(line) = control_line(&controls, "first-list") else {
        rt.fail(case, "the publish-side script left no summary line for the first list");
    };
    if control_field(line, "feed") != "ok" {
        rt.fail(case, &format!("the case's feed was not seeded by copy (the publish-side script's own refusal names the package): {line}"));
    }
    if control_field(line, "rc") != "0" || control_field(line, "installed") != "4" || control_field(line, "emulator") != "1" || control_field(line, "warn") != "0" || control_field(line, "none") != "0" {
        rt.fail(case, &format!("the first list did not come back whole (RHU's four packages installed by the command beside the shell, the emulator row, no warning, no no-devices sentence): {line}"));
    }
    println!("  as registered: {line}");
    rt.pass(case, "positive", "", &format!("{{\"rows\":{}}}", control_field(line, "rows")));
    println!("  PASS");
    let case = "rt.sdk-parity-control-first-list-no-tools";
    println!("== case {case}: the same from a feed of the case's own that holds the SDK package alone and names no public source (the released packages on nuget.org answer the id at another version), over the same fallback folder (no Lamella package in it) -- the restore refuses by NuGet's own sentence naming the tools package, one restore and no second, nothing listed; a refusal naming the SDK package instead is a broken feed, not a pass ==");
    let Some(line) = control_line(&controls, "first-list-no-tools") else {
        rt.fail_control(case, "tools-package-absent", "the publish-side script left no summary line for the no-tools control");
    };
    let text = std::fs::read_to_string(out.join("run-first-list-no-tools.stdout")).unwrap_or_default() + &std::fs::read_to_string(out.join("run-first-list-no-tools.stderr")).unwrap_or_default();
    if control_field(line, "feed") != "ok" {
        rt.fail_control(case, "tools-package-absent", &format!("the control's feed was not seeded by copy (the publish-side script's own refusal names the package): {line}"));
    }
    if text.contains("MSB4236") || text.contains("Unable to find package Lamella.Sdk") || text.contains("Unable to find package Lamella.Rhu.Build") {
        rt.fail_control(case, "tools-package-absent", &format!("the control refused for the wrong reason: the shell or the build package itself was not found (MSB4236, or NuGet's sentence naming Lamella.Sdk or Lamella.Rhu.Build) -- a broken feed, not the registered refusal of the tools package: {line}"));
    }
    if control_field(line, "rows") != "0" || control_field(line, "emulator") != "0" || control_field(line, "refused") == "0" || control_field(line, "restores") != "1" || !text.contains("NU1101") || !text.contains("Unable to find package Lamella.Rhu.Tools.") {
        rt.fail_control(case, "tools-package-absent", &format!("the absent tools package was not refused as registered (NuGet's sentence naming it, one restore, nothing listed): {line}"));
    }
    println!("  refused by the restore (rc {}): {}", control_field(line, "rc"), text.lines().find(|l| l.contains("NU1101")).unwrap_or("").trim());
    rt.pass(case, "negative", "tools-package-absent", "");
    println!("  PASS");

    let publish_dir = format!("{root}/src/rt/gate/rthello/bin/Release/net11.0/linux-arm/publish");
    let case = "rt.sdk-parity-run-arguments";
    println!("== case {case}: dotnet run --device {RUN_PROBE_A} through the SDK -- the image published with its manifest, the stand-in tool started with exactly run --manifest <the manifest> --probe <the id> --, and its code 7 returned unchanged ==");
    let want_argv = format!("argv: [run] [--manifest] [{publish_dir}/rthello.elf.manifest.json] [--probe] [{RUN_PROBE_A}] [--]");
    let Some(line) = control_line(&controls, "run-arguments") else {
        rt.fail(case, "the publish-side script left no summary line for the run");
    };
    let text = std::fs::read_to_string(out.join("run-run-arguments.stdout")).unwrap_or_default();
    let argv = text.lines().find(|l| l.starts_with("argv:")).map(|l| l.trim_end().to_string()).unwrap_or_default();
    if control_field(line, "rc") != "7" || control_field(line, "manifest") != "present" {
        rt.fail(case, &format!("the run did not end with the stand-in's code 7 with the manifest published: {line}"));
    }
    if argv != want_argv {
        rt.fail(case, &format!("the stand-in was started with '{argv}', wanted '{want_argv}'"));
    }
    println!("  the manifest published beside the image; the command line exactly as registered; dotnet run returned 7, the stand-in's code, unchanged");
    rt.pass(case, "positive", "", "{\"exit\":7}");
    println!("  PASS");
    let case = "rt.sdk-parity-run-emulator";
    println!("== case {case}: dotnet run --device {} through the SDK -- the stand-in emulator started on the published image with the gate's own line ==", w.machine);
    let want_argv = format!("argv: [-M] [{}] [-display] [none] [-monitor] [none] [-serial] [stdio] [-semihosting] [-kernel] [{publish_dir}/rthello.elf]", w.machine);
    let Some(line) = control_line(&controls, "run-emulator") else {
        rt.fail(case, "the publish-side script left no summary line for the emulator run");
    };
    let text = std::fs::read_to_string(out.join("run-run-emulator.stdout")).unwrap_or_default();
    let argv = text.lines().find(|l| l.starts_with("argv:")).map(|l| l.trim_end().to_string()).unwrap_or_default();
    if control_field(line, "rc") != "0" || argv != want_argv {
        rt.fail(case, &format!("the emulator was not started as registered (rc {}): '{argv}', wanted '{want_argv}'", control_field(line, "rc")));
    }
    println!("  the emulator's line exactly as registered; the run returned the emulator's 0");
    rt.pass(case, "positive", "", "{\"exit\":0}");
    println!("  PASS");
    let case = "rt.sdk-parity-control-run-program-arguments";
    println!("== case {case}: dotnet run --device {RUN_PROBE_A} -- one \"two words\" -- the words reach the tool after the bare double dash and it refuses them in one sentence, exit 1 ==");
    let Some(line) = control_line(&controls, "run-program-arguments") else {
        rt.fail_control(case, "program-arguments", "the publish-side script left no summary line for the run with program arguments");
    };
    let err = std::fs::read_to_string(out.join("run-run-program-arguments.stderr")).unwrap_or_default();
    if control_field(line, "rc") != "1" || !err.contains("cannot take arguments yet") || !err.contains("(given: one two words)") {
        rt.fail_control(case, "program-arguments", &format!("the program's arguments were not refused as registered: {line}"));
    }
    println!("  refused (rc 1): {}", err.lines().find(|l| l.contains("cannot take arguments yet")).unwrap_or("").trim());
    rt.pass(case, "negative", "program-arguments", "");
    println!("  PASS");
    let case = "rt.sdk-parity-control-run-emulator";
    println!("== case {case}: RHU's targets in the build package in the cache doctored (the emulator line's semihosting word moved; the probe line is the shell's) -- the command line must not be the expectation's: the comparison refuses ==");
    let Some(line) = control_line(&controls, "run-emulator-moved") else {
        rt.fail_control(case, "run-emulator", "the publish-side script left no summary line for the moved run");
    };
    let text = std::fs::read_to_string(out.join("run-run-emulator-moved.stdout")).unwrap_or_default();
    let argv = text.lines().find(|l| l.starts_with("argv:")).map(|l| l.trim_end().to_string()).unwrap_or_default();
    if argv.is_empty() || !argv.contains("[-semihostin]") {
        rt.fail_control(case, "run-emulator", &format!("the doctored targets did not reach the stand-in with the moved word: {line}; '{argv}'"));
    }
    let want_argv = format!("argv: [-M] [{}] [-display] [none] [-monitor] [none] [-serial] [stdio] [-semihosting] [-kernel] [{publish_dir}/rthello.elf]", w.machine);
    if argv == want_argv {
        rt.fail_control(case, "run-emulator", "the moved command line read as the expectation: the control does not distinguish");
    }
    println!("  the moved line '{argv}' is not the expectation (rc {}): the comparison bites", control_field(line, "rc"));
    rt.pass(case, "negative", "run-emulator", "");
    println!("  PASS");

    // 13. Three fixes of the run verb, each a case here: the run verb's no-build
    // option (the publish at the end of the run verb's build, nothing from
    // ComputeRunArguments), no C compiler on the host, a class library through the
    // SDK; each read from the publish-side script's summary lines.
    let case = "rt.sdk-parity-run-no-build";
    println!("== case {case}: dotnet run --no-build --device {RUN_PROBE_A} after the runs above -- nothing published again (the image's bytes and its time unchanged), the last image run by the stand-in and its code 7 returned; then the same with the output level moved: the move ignored, nothing published, the same image run ==");
    for name in ["run-no-build", "run-no-build-word"] {
        let Some(line) = control_line(&controls, name) else {
            rt.fail(case, &format!("the publish-side script left no summary line for {name}"));
        };
        let before = control_field(line, "before");
        if control_field(line, "rc") != "7" || control_field(line, "argv") != "1" || before.is_empty() || before == "absent" || before != control_field(line, "after") || control_field(line, "time0") != control_field(line, "time1") {
            rt.fail(case, &format!("the no-build run did not run the last image unchanged as registered ({name}): {line}"));
        }
    }
    println!("  the last image run unchanged twice ({}, its time kept); the moved word ignored under the no-build option", control_field(control_line(&controls, "run-no-build").unwrap_or(""), "after"));
    rt.pass(case, "positive", "", "{\"exit\":7}");
    println!("  PASS");
    let case = "rt.sdk-parity-control-run-no-build";
    println!("== case {case}: the no-build run's two controls -- the same moved word with the build (dotnet run --device {RUN_PROBE_A}) publishes again, the image's bytes and time moving; and with the published image set aside the no-build run is refused naming the image, nothing run ==");
    let Some(built) = control_line(&controls, "run-word-built") else {
        rt.fail_control(case, "run-no-build", "the publish-side script left no summary line for the moved word's run with the build");
    };
    if control_field(built, "rc") != "7" || control_field(built, "argv") != "1" || control_field(built, "before") == "absent" || control_field(built, "before") == control_field(built, "after") || control_field(built, "time0") == control_field(built, "time1") {
        rt.fail_control(case, "run-no-build", &format!("the moved word with the build did not publish again: the control does not tell a publish that never runs from one the no-build option skips: {built}"));
    }
    let Some(absent) = control_line(&controls, "run-no-build-absent") else {
        rt.fail_control(case, "run-no-build", "the publish-side script left no summary line for the no-build run without an image");
    };
    let text = std::fs::read_to_string(out.join("run-run-no-build-absent.stdout")).unwrap_or_default() + &std::fs::read_to_string(out.join("run-run-no-build-absent.stderr")).unwrap_or_default();
    if control_field(absent, "rc") == "7" || control_field(absent, "rc") == "0" || control_field(absent, "argv") != "0" || control_field(absent, "refuse") == "0" || !text.contains(&format!("{publish_dir}/rthello.elf")) {
        rt.fail_control(case, "run-no-build", &format!("the no-build run without an image was not refused naming the image: {absent}"));
    }
    println!("  the moved word with the build published again ({} -> {}); without an image the no-build run was refused (rc {}) naming {publish_dir}/rthello.elf", control_field(built, "before"), control_field(built, "after"), control_field(absent, "rc"));
    rt.pass(case, "negative", "run-no-build", "");
    println!("  PASS");
    let case = "rt.sdk-parity-no-compiler";
    println!("== case {case}: the publish with the compiler pack's alternative compiler name pointed at a program that does not exist -- the pack's probe is satisfied by the shell the SDK names, the publish passes and the object is the record's ==");
    let Some(nc) = control_line(&controls, "no-compiler") else {
        rt.fail(case, "the publish-side script left no summary line for the no-compiler publish");
    };
    // The record's object: the one the leg's first publish banked (an earlier case compared the
    // hash with the device listing's text by a reused binding; this case reads the object itself).
    let record_object = sha256_file(&out.join("rthello.o")).unwrap_or_default();
    if control_field(nc, "rc") != "0" || control_field(nc, "elf") != "present" || control_field(nc, "probe") != "0" || control_field(nc, "object-sha256").is_empty() || !record_object.starts_with(control_field(nc, "object-sha256")) {
        rt.fail(case, &format!("the publish with no C compiler to be found did not pass with the record's object: {nc}"));
    }
    println!("  the publish passed with the pack's probe satisfied by the shell; the object the record's ({}...)", control_field(nc, "object-sha256"));
    rt.pass(case, "positive", "", "");
    println!("  PASS");
    let case = "rt.sdk-parity-control-no-compiler";
    println!("== case {case}: both compiler names the pack looks for pointed at programs that do not exist (the SDK's shell overridden) -- the pack's probe must refuse by its own sentence, no image ==");
    let Some(cp) = control_line(&controls, "compiler-probe") else {
        rt.fail_control(case, "compiler-probe", "the publish-side script left no summary line for the compiler-probe publish");
    };
    if control_field(cp, "rc") == "0" || control_field(cp, "elf") != "absent" || control_field(cp, "probe") == "0" {
        rt.fail_control(case, "compiler-probe", &format!("the pack's probe did not refuse the missing compilers: the control does not distinguish: {cp}"));
    }
    println!("  refused by the pack's sentence (rc {}), no image: the probe is the pack's and the SDK's name disarms it", control_field(cp, "rc"));
    rt.pass(case, "negative", "compiler-probe", "");
    println!("  PASS");
    let case = "rt.sdk-parity-library";
    println!("== case {case}: a class library (no OutputType: .NET's default) built through the SDK with the board named -- the build passes, the assembly written, no image ==");
    let Some(lb) = control_line(&controls, "library-build") else {
        rt.fail(case, "the publish-side script left no summary line for the library build");
    };
    if control_field(lb, "rc") != "0" || control_field(lb, "dll") != "present" || control_field(lb, "images") != "0" || control_field(lb, "outputtype") != "Library" {
        rt.fail(case, &format!("the class library did not build through the SDK as registered: {lb}"));
    }
    println!("  the library built (OutputType {}), no image", control_field(lb, "outputtype"));
    rt.pass(case, "positive", "", "");
    println!("  PASS");
    let case = "rt.sdk-parity-control-library";
    println!("== case {case}: the same library published -- refused by name before anything is compiled ==");
    let Some(lp) = control_line(&controls, "library-publish") else {
        rt.fail_control(case, "library-publish", "the publish-side script left no summary line for the library publish");
    };
    if control_field(lp, "rc") == "0" || control_field(lp, "code") != "RHU1004" || !lp.contains("REFUSE: OutputType is 'Library', not Exe") {
        rt.fail_control(case, "library-publish", &format!("the library's publish was not refused as registered (RHU1004 naming the output type): {lp}"));
    }
    println!("  refused (rc {}, RHU1004): OutputType is 'Library', not Exe", control_field(lp, "rc"));
    rt.pass(case, "negative", "library-publish", "");
    println!("  PASS");

    let case = "rt.sdk-parity-bus-names";
    println!("== case {case}: a class library that imports System.Device.Spi, System.Device.I2c and Lamella.Hardware together and names both of the board class's bus records, BoardI2cBus and BoardSpiBus -- the compiler passes it, the assembly written ==");
    let Some(bn) = control_line(&controls, "bus-names") else {
        rt.fail(case, "the publish-side script left no summary line for the build that names the two bus records");
    };
    if control_field(bn, "rc") != "0" || control_field(bn, "dll") != "present" || control_field(bn, "codes") != "NONE" {
        rt.fail(case, &format!("the library that names both bus records beside the two device namespaces did not build as registered: {bn}"));
    }
    println!("  built: BoardI2cBus and BoardSpiBus named beside System.Device.I2c and System.Device.Spi, no compiler error");
    rt.pass(case, "positive", "", "");
    println!("  PASS");
    let case = "rt.sdk-parity-control-bus-names";
    println!("== case {case}: the same source with the two names the records carried before, I2cBusInfo and SpiBusInfo -- the compiler must refuse both: no namespace of the three carries the first, and the second is the device bindings package's own static class, which takes no board's bus ==");
    let Some(bo) = control_line(&controls, "bus-names-old") else {
        rt.fail_control(case, "bus-names-old", "the publish-side script left no summary line for the build under the old names");
    };
    let count = |key: &str| control_field(bo, key).parse::<u32>().unwrap_or(0);
    if control_field(bo, "rc") == "0" || control_field(bo, "dll") != "absent" || control_field(bo, "codes") != "CS0029,CS0246,CS0723" || count("cs0246-i2c") == 0 || count("cs0029-spi") == 0 || control_field(bo, "cs0104") != "0" {
        rt.fail_control(case, "bus-names-old", &format!("the old names were not refused as registered (CS0246 for I2cBusInfo, CS0029 and CS0723 for the bindings package's SpiBusInfo, nothing ambiguous and no other code): the control does not distinguish: {bo}"));
    }
    println!("  refused (rc {}): I2cBusInfo stands in no namespace of the three (CS0246), SpiBusInfo is the bindings package's own static class: it takes no board's bus (CS0029) and no variable of it is declared (CS0723)", control_field(bo, "rc"));
    rt.pass(case, "negative", "bus-names-old", "");
    println!("  PASS");

    // 14. The pack split's cases at the new hashes: the version
    // stamp, amendment 3's RuntimeIdentifier, and the boardless listing with its control; each read from the
    // publish-side script's summary lines and banked files.
    let case = "rt.sdk-parity-stamped-version";
    println!("== case {case}: the build package's properties as restored carry the pack's own version unconditioned (the pack's stamp; the tree's line defaults it for the desk alone), read through the shell as RhuSdkVersion ==");
    let Some(line) = control_line(&controls, "stamped-version") else {
        rt.fail(case, "the publish-side script left no summary line for the stamped version");
    };
    if control_field(line, "rc") != "0" || control_field(line, "version") != release || control_field(line, "stamped") != "1" || control_field(line, "conditioned") != "0" {
        rt.fail(case, &format!("the stamped version did not read as registered (the release {release} unconditioned, no default line in the properties restored): {line}"));
    }
    println!("  RhuSdkVersion reads {release} through the shell; the properties file in the package folder carries the one unconditioned line and no default");
    rt.pass(case, "positive", "", &format!("{{\"version\":\"{release}\"}}"));
    println!("  PASS");
    let case = "rt.sdk-parity-rid-global";
    println!("== case {case}: the publish with RuntimeIdentifier given as a global property equal to the posture's value (amendment 3: the shell's run restores again with the chosen device and the build's own identifier) -- the publish passes and the object is the record's ==");
    let Some(line) = control_line(&controls, "rid-equal") else {
        rt.fail(case, "the publish-side script left no summary line for the publish with the global runtime identifier");
    };
    let record_object = sha256_file(&out.join("rthello.o")).unwrap_or_default();
    if control_field(line, "rc") != "0" || control_field(line, "elf") != "present" || control_field(line, "object-sha256").is_empty() || !record_object.starts_with(control_field(line, "object-sha256")) {
        rt.fail(case, &format!("the publish with the posture's own runtime identifier as a global property did not pass with the record's object: {line}"));
    }
    println!("  the publish passed with RuntimeIdentifier=linux-arm given globally; the object the record's ({}...)", control_field(line, "object-sha256"));
    rt.pass(case, "positive", "", "");
    println!("  PASS");
    let case = "rt.sdk-parity-boardless-build";
    println!("== case {case}: dotnet build with no board named through the shell, from an empty package folder of the case's own -- the managed assembly alone: rc 0, no image, no board and no runtime package restored ==");
    let Some(line) = control_line(&controls, "boardless-build") else {
        rt.fail(case, "the publish-side script left no summary line for the boardless build");
    };
    if control_field(line, "rc") != "0" || control_field(line, "assembly") != "present" || control_field(line, "images") != "0" || control_field(line, "board-packages") != "0" || control_field(line, "runtime-packages") != "0" {
        rt.fail(case, &format!("the boardless build did not pass as registered (rc 0, the assembly, no image, no board or runtime package in the case's package folder): {line}"));
    }
    println!("  the boardless build passed: the assembly present, no image, {} lamella entries restored and none of a board or a runtime", control_field(line, "restored"));
    rt.pass(case, "positive", "", "");
    println!("  PASS");
    let case = "rt.sdk-parity-routes-boardless";
    println!("== case {case}: RhuRunRoutes with no board named (dotnet msbuild -getTargetResult through the shell) -- every row of the table listed once, each route carrying Board, the emulator rows with their machine and the stand-in's answer (the contract's amendment 1) ==");
    let Some(line) = control_line(&controls, "routes-boardless") else {
        rt.fail(case, "the publish-side script left no summary line for the boardless listing");
    };
    let listing = std::fs::read_to_string(out.join("routes-routes-boardless.txt")).unwrap_or_default();
    let items: Vec<&str> = listing.lines().skip(1).collect();
    let mut boards: Vec<&str> = items.iter().map(|l| control_field(l, "board")).collect();
    boards.sort_unstable();
    let mut rows: Vec<&str> = stack_facts.rows.iter().map(|r| r.board.as_str()).collect();
    rows.sort_unstable();
    let emulator_ok = items.iter().any(|l| l.starts_with("emulator ") && control_field(l, "board") == w.sdk_board && control_field(l, "machine") == w.machine && control_field(l, "available") == "true");
    if control_field(line, "rc") != "0" || control_field(line, "result") != "Success" || boards != rows || boards.iter().any(|b| b.is_empty()) || !emulator_ok {
        rt.fail(case, &format!("the boardless listing is not the table's every row once with Board on each route and this leg's emulator row available (read {} routes for {} rows): {line}", items.len(), rows.len()));
    }
    println!("  {} routes, one per row of the table, each with Board; the emulator route of {} on {} available", items.len(), w.sdk_board, w.machine);
    rt.pass(case, "positive", "", &format!("{{\"routes\":{}}}", items.len()));
    println!("  PASS");
    let case = "rt.sdk-parity-control-routes-boardless";
    println!("== case {case}: the same listing with the board named -- its own route alone (the control: a board named lists one row, so the every-row listing above is the amendment's shape, not the target's indifference) ==");
    let Some(line) = control_line(&controls, "routes-board") else {
        rt.fail_control(case, "routes-board", "the publish-side script left no summary line for the listing with the board named");
    };
    let listing = std::fs::read_to_string(out.join("routes-routes-board.txt")).unwrap_or_default();
    let items: Vec<&str> = listing.lines().skip(1).collect();
    if control_field(line, "rc") != "0" || control_field(line, "result") != "Success" || items.len() != 1 || !items[0].starts_with("emulator ") || control_field(items[0], "board") != w.sdk_board || control_field(items[0], "machine") != w.machine {
        rt.fail_control(case, "routes-board", &format!("the listing with the board named is not its one emulator route: {line}; {items:?}"));
    }
    println!("  one route, the emulator's for {} on {}: the control distinguishes", w.sdk_board, w.machine);
    rt.pass(case, "negative", "routes-board", "");
    println!("  PASS");

    let case = "rt.sdk-parity-host-inputs";
    println!("== case {case}: the build's host inputs as the shell resolves them (dotnet msbuild -getProperty through the shell) -- PathMap with the intermediate directory mapped to /rhu/obj/<project> ahead of the project directory's /rhu/src/<project>, and NoWin32Manifest true ==");
    let Some(line) = control_line(&controls, "host-inputs") else {
        rt.fail(case, "the publish-side script left no summary line for the host inputs");
    };
    if control_field(line, "rc") != "0" || control_field(line, "entries") != "2" || control_field(line, "obj-first") != "yes" || control_field(line, "src-second") != "yes" || control_field(line, "nowin32manifest") != "true" {
        rt.fail(case, &format!("the host inputs are not as registered (two path-map entries, the intermediate directory's first, NoWin32Manifest true): {line}"));
    }
    println!("  the path map's two entries in their order; NoWin32Manifest true");
    rt.pass(case, "positive", "", "");
    println!("  PASS");
    let case = "rt.sdk-parity-line-endings";
    println!("== case {case}: the line-endings task over three sources -- CRLF under the directory rewritten to LF, LF under it left with its time, CRLF outside it left as it is ==");
    let Some(line) = control_line(&controls, "line-endings") else {
        rt.fail(case, "the publish-side script left no summary line for the line-endings task");
    };
    if control_field(line, "rc") != "0" || control_field(line, "rewritten") != "1" || control_field(line, "crlf-cr") != "0" || control_field(line, "lf-cr") != "0" || control_field(line, "lf-time") != "978307200" || control_field(line, "outside-cr") != "2" {
        rt.fail(case, &format!("the line-endings task did not act as registered (one file rewritten, the LF file's time kept, the file outside untouched): {line}"));
    }
    println!("  one file rewritten to LF; the LF file untouched (its time 978307200 kept); the file outside the directory untouched (2 carriage returns)");
    rt.pass(case, "positive", "", "");
    println!("  PASS");
    let case = "rt.sdk-parity-control-pathmap-empty";
    println!("== case {case}: PathMap given empty as a global property (the SDK's default cannot fill a global property) -- refused by RHU3011 by name before anything is compiled, no image ==");
    let Some(line) = control_line(&controls, "pathmap-empty") else {
        rt.fail_control(case, "path-map", "the publish-side script left no summary line for the empty path map");
    };
    let refuse = line.split_once("refuse=").map(|(_, r)| r.trim()).unwrap_or("");
    if control_field(line, "rc") == "0" || control_field(line, "elf") != "absent" || control_field(line, "code") != "RHU3011" || !refuse.contains("PathMap is empty") {
        rt.fail_control(case, "path-map", &format!("the empty path map was not refused as registered: {line}"));
    }
    println!("  refused (rc {}, RHU3011, no image): {refuse}", control_field(line, "rc"));
    rt.pass(case, "negative", "path-map", "");
    println!("  PASS");

    // 14. The Debug configuration: a plain dotnet run builds Debug, whose
    // .NET default turns the code optimization off. The SDK's targets keep
    // that for the program's own code alone -- the project's assembly and
    // the class libraries it references by project, handed to the native
    // compiler marked not to be optimized -- and compile the framework and
    // every package with the optimization on, on every row. On a row whose
    // board the board table marks RequiresOptimization the Debug image
    // links, where the image compiled without the optimization as a whole
    // is refused at the link (the control above); the case flies
    // DEBUG_ROW, one such row. The response file carries the
    // optimization words and the debug word, names one assembly under the
    // build's own-code folder, the program's (the board class's package is
    // handed over as it is), and sets the folding of identical bodies to
    // generic; the record names the framework's code optimized and the
    // program's own unoptimized. The image's window is read against the
    // floor the row's map states in the tree.
    let case = "rt.sdk-parity-debug-optimized";
    println!("== case {case}: the board class's program published for {DEBUG_ROW} in the Debug configuration -- the compiler's response file carrying the optimization words (-O, --Os) and the debug word (-g) once each, one assembly marked (the program's, the package's not), the folding of identical bodies at generic, the image linked, its record optimized and unoptimized, its arena window at or above the row's floor ==");
    let Some(line) = control_line(&controls, "debug-optimized") else {
        rt.fail(case, "the publish-side script left no summary line for the Debug publish");
    };
    if debug_fields(line) != DEBUG_USUAL || control_field(line, "own") != "tbhello.dll" || control_field(line, "fold") != "generic" {
        rt.fail(case, &format!("the Debug publish did not run as registered (rc 0, the image present, -O, --Os and -g in the response file once each, the record optimized and unoptimized, the one marked assembly tbhello.dll, the folding generic): {line}"));
    }
    let floor = match arena_floor(repo, DEBUG_MAP) {
        Ok(v) => v,
        Err(e) => rt.fail(case, &e),
    };
    let debug_elf = out.join("control-debug-optimized").join("tbhello.elf");
    let debug_elf_arg = debug_elf.to_string_lossy().into_owned();
    let dump = rt.tool_text(&nm_tool, case, &[&debug_elf_arg], "sdk-parity-debug-optimized.nm.txt");
    let (lo, hi) = match (word_sym_value(dump.as_bytes(), "__rhu_arena_lo"), word_sym_value(dump.as_bytes(), "__rhu_arena_hi")) {
        (Some(lo), Some(hi)) => (lo, hi),
        _ => rt.fail(case, "the Debug image does not define __rhu_arena_lo and __rhu_arena_hi"),
    };
    if hi <= lo || hi - lo < floor {
        rt.fail(case, &format!("the Debug image's arena window [{lo:#x}, {hi:#x}) is {} B, under the row's floor {floor} B", hi.saturating_sub(lo)));
    }
    println!("  -O, --Os and -g in the response file once each; one assembly marked, tbhello.dll; the folding generic; the record optimized and unoptimized; the image's arena window [{lo:#x}, {hi:#x}) = {} B at or above the row's floor {floor} B (the map {DEBUG_MAP})", hi - lo);
    rt.pass(case, "positive", "", &format!("{{\"window\":{},\"floor\":{floor},\"board\":\"{DEBUG_ROW}\"}}", hi - lo));
    println!("  PASS");

    // 15. The image compiled without the optimization as a whole, on a row
    // whose board holds it: one build property asks for it, the response
    // file carries no optimization word and keeps its debug word, nothing
    // is marked, the image links and its record names both unoptimized. The
    // row's Debug configuration by itself is the usual one. The same
    // publish with the optimization turned on for the program's own code by
    // a global property (Optimize=true) marks nothing and its record names
    // both optimized. Each publish's summary line is read by debug_fields.
    let case = "rt.sdk-parity-debug-unoptimized-row";
    println!("== case {case}: the board class's program published for {DEBUG_PLAIN_ROW} in the Debug configuration, and again compiled without the optimization as a whole -- the first the usual Debug build with one assembly marked, the second with no optimization word (-O, --Os) in the compiler's response file, the debug word (-g) once, nothing marked, the image linked, its record unoptimized and unoptimized ==");
    match control_line(&controls, "debug-plain") {
        Some(l) if debug_fields(l) == DEBUG_USUAL && control_field(l, "own") == "tbhello.dll" => {}
        other => rt.fail(case, &format!("the Debug publish did not run as registered (rc 0, the image present, -O, --Os and -g once each, the record optimized and unoptimized, the one marked assembly tbhello.dll): {other:?}")),
    }
    match control_line(&controls, "debug-plain-whole") {
        Some(l) if debug_fields(l) == DEBUG_WHOLE && control_field(l, "own") == "NONE" && control_field(l, "fold") == "none" => {}
        other => rt.fail(case, &format!("the publish compiled without the optimization as a whole did not run as registered (rc 0, the image present, no -O and no --Os in the response file, -g once, nothing marked, the record unoptimized and unoptimized): {other:?}")),
    }
    println!("  the Debug configuration: the usual build; compiled without the optimization as a whole: no -O and no --Os in the response file, -g once, nothing marked, the image linked, the record unoptimized and unoptimized");
    rt.pass(case, "positive", "", &format!("{{\"board\":\"{DEBUG_PLAIN_ROW}\",\"profile\":\"unoptimized\",\"program\":\"unoptimized\"}}"));
    println!("  PASS");
    let case = "rt.sdk-parity-debug-optimize-on";
    println!("== case {case}: the same publish with the optimization turned on for the program's own code by a global property (Optimize=true) -- the optimization words and the debug word once each, nothing marked, the image linked, its record optimized and optimized ==");
    match control_line(&controls, "debug-plain-optimize-on") {
        Some(l) if debug_fields(l) == DEBUG_OPTIMIZED && control_field(l, "own") == "NONE" => {}
        other => rt.fail(case, &format!("the Debug publish with the optimization turned on did not run as registered (rc 0, the image present, -O, --Os and -g in the response file once each, nothing marked, the record optimized and optimized): {other:?}")),
    }
    match control_line(&controls, "debug-plain-whole-optimize-on") {
        Some(l) if debug_fields(l) == DEBUG_WHOLE && control_field(l, "own") == "NONE" => {}
        other => rt.fail(case, &format!("the same publish compiled without the optimization as a whole did not run as registered (the native compiler has one setting for a compilation: rc 0, the image present, no -O and no --Os in the response file, -g once, nothing marked, the record unoptimized and unoptimized): {other:?}")),
    }
    println!("  -O, --Os and -g in the response file once each; nothing marked; the image linked; the record optimized and optimized; compiled without the optimization as a whole, the same publish is unoptimized and unoptimized");
    rt.pass(case, "positive", "", &format!("{{\"board\":\"{DEBUG_PLAIN_ROW}\",\"profile\":\"optimized\",\"program\":\"optimized\"}}"));
    println!("  PASS");

    // 16. The row with an external-flash map: its board requires the
    // optimization while the program's code lies in the chip's own flash.
    // With the chip's own flash named its Debug configuration is the usual build;
    // with the program's home external the image compiled without the
    // optimization as a whole links under the external-flash map.
    let case = "rt.sdk-parity-debug-program-memory";
    println!("== case {case}: the board class's program published for {XIP_ROW} in the Debug configuration with the chip's own flash named, and compiled without the optimization as a whole with the program's home external -- the usual Debug build under the first (the record optimized and unoptimized), no optimization word and the image linked under the second (the record unoptimized and unoptimized) ==");
    match control_line(&controls, "debug-program-internal").map(debug_fields) {
        Some(got) if got == DEBUG_USUAL => {}
        other => rt.fail(case, &format!("the Debug publish with the chip's own flash named did not run as registered (rc 0, the image present, -O, --Os and -g in the response file once each, the record optimized and unoptimized): {other:?}")),
    }
    match control_line(&controls, "debug-program-external").map(debug_fields) {
        Some(got) if got == DEBUG_WHOLE => {}
        other => rt.fail(case, &format!("the publish compiled without the optimization as a whole with the program's home external did not run as registered (rc 0, the image present, no -O and no --Os in the response file, -g once, the record unoptimized and unoptimized): {other:?}")),
    }
    println!("  the chip's own flash named: -O, --Os and -g once each, optimized and unoptimized; the external home, compiled without the optimization as a whole: no -O and no --Os, -g once, the image linked, unoptimized and unoptimized");
    rt.pass(case, "positive", "", &format!("{{\"board\":\"{XIP_ROW}\"}}"));
    println!("  PASS");

    // 17. Both images run: the board class's program published for this
    // leg's own emulator row in the Debug configuration, and again compiled
    // without the optimization as a whole, each booted on the emulator --
    // the class's lines in the facts file's order, the kernel's return
    // line, no forbidden pattern, exit 0.
    let case = "rt.sdk-parity-debug-boot";
    println!("== case {case}: the board class's program published for {} in the Debug configuration, and again compiled without the optimization as a whole, each image booted on the emulator -- the class's six lines in order, exit 0 ==", w.sdk_board);
    let class_markers = match board_class_markers(repo, w.sdk_board) {
        Ok(m) => m,
        Err(e) => rt.fail(case, &e),
    };
    let class_marker_refs: Vec<&str> = class_markers.iter().map(String::as_str).collect();
    let mut boots: Vec<(String, String)> = Vec::new();
    for (label, want, what) in [("debug-emulator", DEBUG_USUAL, "the Debug configuration's image"), ("debug-emulator-whole", DEBUG_WHOLE, "the image compiled without the optimization as a whole")] {
        match control_line(&controls, label).map(debug_fields) {
            Some(got) if got == want => {}
            other => rt.fail(case, &format!("the publish of {what} for the emulator row did not run as registered ({want:?}): {other:?}")),
        }
        let boot_elf = out.join(format!("control-{label}")).join("tbhello.elf");
        let boot_elf_arg = boot_elf.to_string_lossy().into_owned();
        let raw_path = out.join(format!("{label}-boot.raw"));
        let stderr_path = out.join(format!("{label}-boot.stderr"));
        let mut qargs: Vec<&str> = vec!["-M", w.machine, "-display", "none", "-monitor", "none", "-serial", "stdio", "-semihosting"];
        qargs.extend_from_slice(ExecProfile::Parity.qemu_args());
        qargs.push("-kernel");
        qargs.push(&boot_elf_arg);
        let outcome = match run_captured(&qemu, &qargs, &raw_path, &stderr_path, Duration::from_secs(RUN_TIMEOUT_S)) {
            Ok(o) => o,
            Err(e) => rt.fail(case, &e),
        };
        let raw = std::fs::read(&raw_path).unwrap_or_default();
        echo_file(&raw_path, "  | ");
        if outcome.timed_out {
            rt.fail(case, &format!("the boot of {what} hung (killed at its timeout)"));
        }
        if cr_count(&raw) == 0 {
            rt.fail(case, "no CR bytes in raw capture (wire discipline lost)");
        }
        if let Some(fault) = markers_out_of_order(&raw, &class_marker_refs) {
            rt.fail(case, &format!("the board class's lines in the boot of {what}, as the row's facts file predicts them: {fault}"));
        }
        if !contains_fixed(&raw, BOOT_RETURN_LINE) {
            rt.fail(case, &format!("the kernel's return line '{BOOT_RETURN_LINE}' not found in the boot of {what}"));
        }
        for m in FORBIDDEN_PATTERNS {
            if contains_fixed(&raw, m) {
                rt.fail(case, &format!("forbidden pattern '{m}' present in the boot of {what}"));
            }
        }
        if outcome.exit != 0 {
            rt.fail(case, &format!("the boot of {what}: exit {}, wanted 0", outcome.exit));
        }
        boots.push((format!("{out_rel}/control-{label}/tbhello.elf"), sha256_file(&boot_elf).unwrap_or_default()));
    }
    rt.pass_image(case, 0, &boots[0].0, &boots[0].1, None, &format!("{{\"markers\":{},\"board\":\"{}\",\"profile\":\"optimized\",\"program\":\"unoptimized\",\"whole\":\"{}\"}}", class_markers.len(), w.sdk_board, boots[1].1));
    println!("  PASS (the class's {} lines in the facts file's order, in both boots; the Debug image {}..., the image compiled without the optimization as a whole {}...; {qemu_version})", class_markers.len(), &boots[0].1[..16.min(boots[0].1.len())], &boots[1].1[..16.min(boots[1].1.len())]);

    // 18. What a debugger reads from a Debug image: the six-line program
    // published in the Debug configuration, its image read by
    // tools/rt-publish/debug-readings.py -- a statement row on every
    // statement with the local function a function of its own, a location
    // for each local over the whole of its scope, the record's two keys
    // (the framework's code optimized, the program's own unoptimized), the
    // file under the SDK's path map -- for a row whose board does not
    // require the optimization and for the Raspberry Pi Pico 2 row, whose
    // board does. The control is the same program published for the
    // Raspberry Pi Pico 2 row with the optimization turned on for the
    // program's own code (Optimize=true): the reader reports the statements
    // and the locals gone and the record's second key optimized, and the
    // path map stands.
    for (case, label, row, what) in [
        ("rt.sdk-parity-debug-readings", "debug-readings", DEBUG_PLAIN_ROW, "whose board does not require the optimization"),
        ("rt.sdk-parity-debug-readings-row", "debug-readings-row", DEBUG_ROW, "whose board requires the optimization"),
    ] {
        println!("== case {case}: the six-line program published for {row}, {what}, in the Debug configuration and its image read as a debugger reads it -- a statement row on every statement with the local function a function of its own, a location for each local over the whole of its scope, the record optimized and unoptimized, the file under the SDK's path map ==");
        let Some(line) = control_line(&controls, label) else {
            rt.fail(case, "the publish-side script left no summary line for the readings' publish");
        };
        let readings = std::fs::read_to_string(out.join(format!("control-{label}")).join("readings.txt")).unwrap_or_default();
        for l in readings.lines().filter(|l| l.starts_with("READING ") || l.starts_with("readings: ")) {
            println!("  | {l}");
        }
        if control_field(line, "rc") != "0" || control_field(line, "elf") != "present" || control_field(line, "readings-rc") != "0" || ["r1", "r2", "r3", "r4"].iter().any(|k| control_field(line, k) != "HOLDS") {
            rt.fail(case, &format!("the four readings did not hold as registered (rc 0, the image present, the reader's code 0, each reading HOLDS): {line}"));
        }
        rt.pass(case, "positive", "", &format!("{{\"board\":\"{row}\",\"readings\":4}}"));
        println!("  PASS");
    }
    let case = "rt.sdk-parity-control-debug-readings-optimized";
    println!("== case {case}: the same program published for {DEBUG_ROW} with the optimization turned on for the program's own code (Optimize=true) -- the reader must report the statements and the locals gone and the record's second key optimized, the path map standing ==");
    let Some(line) = control_line(&controls, "debug-readings-optimized") else {
        rt.fail_control(case, "debug-readings-optimized", "the publish-side script left no summary line for this control");
    };
    let readings = std::fs::read_to_string(out.join("control-debug-readings-optimized").join("readings.txt")).unwrap_or_default();
    for l in readings.lines().filter(|l| l.starts_with("READING ") || l.starts_with("readings: ")) {
        println!("  | {l}");
    }
    if control_field(line, "rc") != "0" || control_field(line, "elf") != "present" || control_field(line, "readings-rc") != "1" || ["r1", "r2", "r3"].iter().any(|k| control_field(line, k) != "FAILS") || control_field(line, "r4") != "HOLDS" {
        rt.fail_control(case, "debug-readings-optimized", &format!("the reader did not report the optimized image as registered (the image present, the reader's code 1, the statements, the locals and the record FAILS, the path HOLDS): {line}"));
    }
    println!("  the reader refused the optimized image at its statements, its locals and its record");
    rt.pass(case, "negative", "debug-readings-optimized", "");
    println!("  PASS");

    // 19. The program's own code with a class library referenced by project:
    // the program dbgown, whose two local functions are of one body, and its
    // class library dbgownlib, a plain class library, published in the Debug
    // configuration for this leg's own emulator row and read three ways by
    // the same reader. A project reference is the program's own code: the
    // response file names two assemblies under the own-code folder and the
    // library's lines read as the program's do. Two methods of one body are
    // two functions: the folding of identical bodies stands at generic.
    // And the library was compiled under the SDK's path map: its file is
    // named under the map, and no line of the image carries the tree's path
    // or the home directory's.
    let own_readings = |name: &str| std::fs::read_to_string(out.join("control-debug-own").join(format!("readings-{name}.txt"))).unwrap_or_default();
    let case = "rt.sdk-parity-debug-project-reference";
    println!("== case {case}: a program with a class library referenced by project, published for {} in the Debug configuration -- two assemblies marked, the program's and the library's, and the image read as a debugger reads it at the program's lines and at the library's: a statement row on every statement, a function of its own for the method each calls, a location for each local over the whole of its scope ==", w.sdk_board);
    let Some(line) = control_line(&controls, "debug-own") else {
        rt.fail(case, "the publish-side script left no summary line for the program with a class library");
    };
    for name in ["program", "library"] {
        println!("  the reader at the {name}'s lines:");
        for l in own_readings(name).lines().filter(|l| l.starts_with("READING ") || l.starts_with("readings: ")) {
            println!("  | {l}");
        }
    }
    if control_field(line, "rc") != "0" || control_field(line, "elf") != "present" || control_field(line, "program-rc") != "0" || control_field(line, "library-rc") != "0" || control_field(line, "own") != "dbgown.dll+dbgownlib.dll" {
        rt.fail(case, &format!("the program with a class library did not publish and read as registered (rc 0, the image present, the reader's code 0 at the program's lines and at the library's, the two marked assemblies dbgown.dll and dbgownlib.dll): {line}"));
    }
    rt.pass(case, "positive", "", &format!("{{\"board\":\"{}\",\"marked\":2}}", w.sdk_board));
    println!("  PASS");
    let case = "rt.sdk-parity-debug-one-body";
    println!("== case {case}: the same image read at its two local functions of one body -- each line in a function of its own, no inlined call, the local of the first located, the folding of identical bodies at generic in the compiler's response file ==");
    println!("  the reader at the two functions' lines:");
    for l in own_readings("one-body").lines().filter(|l| l.starts_with("READING ") || l.starts_with("readings: ")) {
        println!("  | {l}");
    }
    if control_field(line, "one-body-rc") != "0" || control_field(line, "fold") != "generic" {
        rt.fail(case, &format!("the two functions of one body did not read as two (the reader's code 0, the folding generic): {line}"));
    }
    rt.pass(case, "positive", "", "");
    println!("  PASS");
    let case = "rt.sdk-parity-project-reference-path-map";
    println!("== case {case}: the same image carries no path of the build machine -- the library's file named under the SDK's path map in the line table, and no line of the image carrying the tree's path or the home directory's ==");
    if control_field(line, "library-path") != "HOLDS" || control_field(line, "machine-paths") != "0" {
        rt.fail(case, &format!("the image of the program with a class library carries a path of the build machine (the library's path reading HOLDS and 0 lines with the tree's path or the home directory's wanted): {line}"));
    }
    println!("  the library's file under the map; 0 lines of the image carry the tree's path or the home directory's");
    rt.pass(case, "positive", "", "");
    println!("  PASS");
    let case = "rt.sdk-parity-control-mark-unreadable";
    println!("== case {case}: the marking task given a file that is not a .NET assembly -- refused by name, no copy written ==");
    let Some(line) = control_line(&controls, "mark-unreadable") else {
        rt.fail_control(case, "mark-unreadable", "the publish-side script left no summary line for this control");
    };
    let refuse = line.split_once("refuse=").map(|(_, r)| r.trim()).unwrap_or("");
    if control_field(line, "rc") == "0" || control_field(line, "copy") != "absent" || !refuse.contains("notanassembly.dll is not a .NET assembly") {
        rt.fail_control(case, "mark-unreadable", &format!("the file was not refused as registered (a failing code, no copy, the file named as not a .NET assembly): {line}"));
    }
    println!("  refused (rc {}, no copy): {refuse}", control_field(line, "rc"));
    rt.pass(case, "negative", "mark-unreadable", "");
    println!("  PASS");

    // 20. A compiled file is not the program's own code: the same class
    // library built by itself and referenced by its file from the program
    // dbgfile. One assembly is marked, the program's, whose lines read
    // whole; at the library's lines the reader must report the statements
    // not standing as a program's own do (its second method inlined into
    // its first), where the same library referenced by project reads whole
    // (the case above).
    let case = "rt.sdk-parity-control-debug-file-reference";
    println!("== case {case}: the same class library referenced as a compiled file, the program published for {} in the Debug configuration -- one assembly marked, the program's, whose lines read whole, and the reader must report the library's statements not standing as a program's own code does ==", w.sdk_board);
    let Some(line) = control_line(&controls, "debug-file") else {
        rt.fail_control(case, "debug-file-reference", "the publish-side script left no summary line for this control");
    };
    let file_readings = std::fs::read_to_string(out.join("control-debug-file").join("readings-library.txt")).unwrap_or_default();
    for l in file_readings.lines().filter(|l| l.starts_with("READING ") || l.starts_with("readings: ")) {
        println!("  | {l}");
    }
    if control_field(line, "rc") != "0" || control_field(line, "library-build-rc") != "0" || control_field(line, "elf") != "present" || control_field(line, "program-rc") != "0" || control_field(line, "own") != "dbgfile.dll" || control_field(line, "library-statements") != "FAILS" {
        rt.fail_control(case, "debug-file-reference", &format!("the compiled file's code did not read as registered (rc 0, the library built, the image present, the reader's code 0 at the program's lines, the one marked assembly dbgfile.dll, the statements reading at the library's lines FAILS): {line}"));
    }
    println!("  one assembly marked, dbgfile.dll; the program's lines read whole; the reader refused the library's statements");
    rt.pass(case, "negative", "debug-file-reference", "");
    println!("  PASS");

    // 21. A class library reached through another is the program's own
    // code: the program dbgvia references the class library dbgvialib by
    // project, which references dbgownlib by project, and a copy of the
    // three projects is published in the Debug configuration for this leg's
    // own emulator row. The response file names three assemblies under the
    // own-code folder, and at the far library's lines the reader holds as
    // it does for a library the program references directly: its
    // statements, its second method a function of its own, its local, the
    // record's two keys and its file under the SDK's path map, with no line
    // of the image carrying the tree's path or the home directory's.
    let via_readings = |name: &str| std::fs::read_to_string(out.join("control-debug-via").join(format!("readings-{name}.txt"))).unwrap_or_default();
    let case = "rt.sdk-parity-debug-through-reference";
    println!("== case {case}: a program that reaches a class library only through another class library, each referenced by project, published for {} in the Debug configuration -- three assemblies marked, and the image read as a debugger reads it at the far library's lines: a statement row on every statement, a function of its own for the method it calls, a location for its local over the whole of its scope, its file under the SDK's path map, no path of the build machine in the image ==", w.sdk_board);
    let Some(via) = control_line(&controls, "debug-via") else {
        rt.fail(case, "the publish-side script left no summary line for the program that reaches a class library through another");
    };
    println!("  the reader at the far library's lines:");
    for l in via_readings("far").lines().filter(|l| l.starts_with("READING ") || l.starts_with("readings: ")) {
        println!("  | {l}");
    }
    if control_field(via, "rc") != "0" || control_field(via, "elf") != "present" || control_field(via, "far-rc") != "0" || control_field(via, "far-path") != "HOLDS" || control_field(via, "own") != "dbgownlib.dll+dbgvia.dll+dbgvialib.dll" || control_field(via, "fold") != "generic" || control_field(via, "machine-paths") != "0" {
        rt.fail(case, &format!("the program that reaches a class library through another did not publish and read as registered (rc 0, the image present, the reader's code 0 at the far library's lines with its file under the map, the three marked assemblies dbgownlib.dll, dbgvia.dll and dbgvialib.dll, the folding generic, 0 lines with the tree's path or the home directory's): {via}"));
    }
    rt.pass(case, "positive", "", &format!("{{\"board\":\"{}\",\"marked\":3}}", w.sdk_board));
    println!("  PASS");

    // 22. A method the program itself marks AggressiveOptimization stays
    // optimized: the same image read at Kept.cs, whose two methods carry
    // the mark and have the shape of the far library's two. The reader
    // must report their statements not standing as the program's own code
    // does, while the file's rows are in the image under the SDK's path
    // map: the two methods were compiled, so the failing reading is not an
    // absent file's.
    let case = "rt.sdk-parity-control-debug-aggressive-optimization";
    println!("== case {case}: the same image read at two methods the program marks AggressiveOptimization -- the reader must report their statements not standing as the program's own code does, the file's rows in the image under the SDK's path map ==");
    for l in via_readings("kept").lines().filter(|l| l.starts_with("READING ") || l.starts_with("readings: ")) {
        println!("  | {l}");
    }
    if control_field(via, "kept-rc") != "1" || control_field(via, "kept-statements") != "FAILS" || control_field(via, "kept-path") != "HOLDS" {
        rt.fail_control(case, "debug-aggressive-optimization", &format!("the two marked methods did not read as registered (the reader's code 1, the statements reading FAILS, the path reading HOLDS): {via}"));
    }
    println!("  the reader refused the two marked methods' statements; their file's rows are in the image");
    rt.pass(case, "negative", "debug-aggressive-optimization", "");
    println!("  PASS");

    // 23. A build with nothing changed does not run the native compiler,
    // and a build after a changed statement does: the same copy published
    // a second time as it stands, and a third time after one statement of
    // its Program.cs is changed. Each publish's own output is read for the
    // compiler pack's line for its native compile step: once in the first,
    // not at all in the second, whose image is the first's byte for byte,
    // and once in the third, whose image moved.
    let case = "rt.sdk-parity-debug-unchanged-build";
    println!("== case {case}: the same program published again with nothing changed, and once more after one statement of its source is changed -- the native compiler's line once in the first publish's output, not at all in the second's with the image the same bytes, once in the third's with the image moved ==");
    let (Some(again), Some(moved)) = (control_line(&controls, "debug-via-again"), control_line(&controls, "debug-via-moved")) else {
        rt.fail(case, "the publish-side script left no summary line for the second publish or the third");
    };
    if control_field(via, "compiles") != "1" {
        rt.fail(case, &format!("the first publish's output does not carry the native compiler's line once: {via}"));
    }
    if control_field(again, "rc") != "0" || control_field(again, "elf") != "present" || control_field(again, "compiles") != "0" || control_field(again, "same") != "yes" {
        rt.fail(case, &format!("the publish with nothing changed did not run as registered (rc 0, the image present and the first publish's bytes, the native compiler's line absent from its output): {again}"));
    }
    if control_field(moved, "rc") != "0" || control_field(moved, "elf") != "present" || control_field(moved, "moved-lines") != "1" || control_field(moved, "compiles") != "1" || control_field(moved, "same") != "no" {
        rt.fail(case, &format!("the publish after a changed statement did not run as registered (rc 0, one line of the source changed, the native compiler's line once in its output, the image moved): {moved}"));
    }
    println!("  the native compiler ran in the first publish, not in the second (the image the same bytes), and again in the third (one statement changed, the image moved)");
    rt.pass(case, "positive", "", "");
    println!("  PASS");

    // 24. The Optimize property speaks for the program's own code in any
    // configuration: the six-line program published for the Raspberry Pi
    // Pico 2 row in the Release configuration with Optimize=false. The
    // program's assembly is marked, the folding stands at generic, and the
    // four readings hold as they do for a Debug image: the statements, the
    // locals, the record's two keys (the framework's code optimized, the
    // program's own unoptimized) and the file under the SDK's path map.
    let case = "rt.sdk-parity-release-optimize-off";
    println!("== case {case}: the six-line program published for {DEBUG_ROW} in the Release configuration with the optimization turned off for the program's own code (Optimize=false) -- one assembly marked, the folding generic, and the image read as a debugger reads it: a statement row on every statement with the local function a function of its own, a location for each local over the whole of its scope, the record optimized and unoptimized, the file under the SDK's path map ==");
    let Some(line) = control_line(&controls, "release-optimize-off") else {
        rt.fail(case, "the publish-side script left no summary line for the Release publish with the optimization off");
    };
    let readings = std::fs::read_to_string(out.join("control-release-optimize-off").join("readings.txt")).unwrap_or_default();
    for l in readings.lines().filter(|l| l.starts_with("READING ") || l.starts_with("readings: ")) {
        println!("  | {l}");
    }
    if control_field(line, "rc") != "0" || control_field(line, "elf") != "present" || control_field(line, "readings-rc") != "0" || ["r1", "r2", "r3", "r4"].iter().any(|k| control_field(line, k) != "HOLDS") || control_field(line, "own") != "dbgsample.dll" || control_field(line, "fold") != "generic" {
        rt.fail(case, &format!("the Release publish with the optimization off did not run and read as registered (rc 0, the image present, the reader's code 0, each reading HOLDS, the one marked assembly dbgsample.dll, the folding generic): {line}"));
    }
    rt.pass(case, "positive", "", &format!("{{\"board\":\"{DEBUG_ROW}\",\"readings\":4}}"));
    println!("  PASS");

    // 25. The network suite: the gate's program for the socket classes
    // published through the SDK for this leg's row and booted on the
    // emulator -- its nine lines in order, the kernel's return line, no
    // forbidden pattern, exit 0 -- and three of the socket functions read
    // by the image's symbols: each defined once, at the size its frozen
    // unit gives it, so none is the surface's generated refusal.
    let case = "rt.sdk-parity-net-suite";
    println!("== case {case}: the network suite published through the SDK for {} and booted on the emulator -- its nine lines in order, exit 0, and three of the socket functions each defined once in the image at the size of its frozen unit ==", w.sdk_board);
    let Some(line) = control_line(&controls, "net-suite") else {
        rt.fail(case, "the publish-side script left no summary line for the network suite's publish");
    };
    if control_field(line, "rc") != "0" || control_field(line, "elf") != "present" {
        rt.fail(case, &format!("the network suite's publish did not produce the image: {line}"));
    }
    let net_elf = out.join("control-net-suite").join("netsuite.elf");
    let (raw, exit_code) = parity_boot(&mut rt, case, &qemu, w.machine, &net_elf, &out.join("net-suite-boot.raw"), &out.join("net-suite-boot.stderr"));
    if let Some(fault) = markers_out_of_order(&raw, &NET_SUITE_LINES) {
        rt.fail(case, &format!("the network suite's lines: {fault}"));
    }
    if !contains_fixed(&raw, BOOT_RETURN_LINE) {
        rt.fail(case, &format!("the kernel's return line '{BOOT_RETURN_LINE}' not found"));
    }
    for m in NET_SUITE_FORBIDDEN {
        if contains_fixed(&raw, m) {
            rt.fail(case, &format!("forbidden pattern '{m}' present"));
        }
    }
    if String::from_utf8_lossy(&raw).lines().any(|l| l.starts_with("FAIL ")) {
        rt.fail(case, "a section's FAIL line is on the wire");
    }
    if exit_code != 0 {
        rt.fail(case, &format!("exit {exit_code}, wanted 0"));
    }
    let net_image = match std::fs::read(&net_elf).map_err(|e| format!("cannot read {}: {e}", net_elf.display())).and_then(ArmImage::read) {
        Ok(i) => i,
        Err(e) => rt.fail(case, &e),
    };
    let mut unit_sizes: Vec<(String, u64)> = Vec::new();
    for (i, obj) in w.shim_objects.iter().filter(|o| o.contains("/shim-socket") || o.contains("/shim-netinfo")).enumerate() {
        let obj_arg = repo.join(obj.replace('/', std::path::MAIN_SEPARATOR_STR)).to_string_lossy().into_owned();
        let text = rt.tool_text(&nm_tool, case, &["-S", "--defined-only", &obj_arg], &format!("sdk-parity-net-suite.unit-{i}.nm.txt"));
        for l in text.lines() {
            let f: Vec<&str> = l.split_whitespace().collect();
            if f.len() == 4 && f[2] == "T" {
                if let Ok(size) = u64::from_str_radix(f[1], 16) {
                    unit_sizes.push((f[3].to_string(), size));
                }
            }
        }
    }
    for name in NET_SUITE_SYMBOLS {
        let defs: Vec<u64> = net_image.symbols.iter().filter(|s| s.defined && s.kind == 2 && s.name == name).map(|s| u64::from(s.size)).collect();
        let unit: Vec<u64> = unit_sizes.iter().filter(|(n, _)| n == name).map(|(_, s)| *s).collect();
        if defs.len() != 1 || unit.len() != 1 || defs[0] != unit[0] {
            rt.fail(case, &format!("{name}: {} definition(s) in the image at the size(s) {defs:?} and {} in the frozen socket units at {unit:?} -- one of each at one size is wanted (a function of another size is not the unit's)", defs.len(), unit.len()));
        }
        println!("  {name}: defined once in the image at {} bytes, the frozen unit's size", defs[0]);
    }
    let net_sha = sha256_file(&net_elf).unwrap_or_default();
    let net_rel = format!("{out_rel}/control-net-suite/netsuite.elf");
    rt.pass_image(case, 0, &net_rel, &net_sha, None, &format!("{{\"lines\":{},\"board\":\"{}\"}}", NET_SUITE_LINES.len(), w.sdk_board));
    println!("  PASS (the suite's {} lines in order; the image {}...; {qemu_version})", NET_SUITE_LINES.len(), &net_sha[..16.min(net_sha.len())]);

    // The suite's control: the merged object the publish made, linked by
    // this runner on the SDK's line against this row's kernel built with
    // the kernel's own doctored name resolver, which keeps one record of
    // an answer. The lookup section must fail by its one line, exact and
    // whole; the six other sections pass; the summary counts one; exit 1.
    let case = "rt.sdk-parity-control-net-suite-doctored-resolver";
    let broke = "the network suite's merged object linked against this row's kernel built with the kernel's doctored name resolver (one record of an answer kept)";
    println!("== case {case} (negative control): {broke} -- the lookup section must fail by its one line, the six other sections pass, exit 1 ==");
    let net_dir = repo.join("src").join("kernel").join("target").join(w.out_dir).join("kernel-net-doctored-resolve");
    let _ = std::fs::create_dir_all(&net_dir);
    let net_log = rt.out.join("sdk-parity-net-doctored-staticlib-build.log");
    let (net_target, net_feats): (Option<&str>, String) = match w.lib_build {
        None => (None, format!("{AN500_FEATURES},test-net-doctored-resolve")),
        Some((_, target, feats)) => (Some(target), format!("{feats},test-net-doctored-resolve")),
    };
    let mut bargs: Vec<&str> = vec!["--lib"];
    if let Some(tg) = net_target {
        bargs.extend_from_slice(&["--target", tg, "--no-default-features"]);
    }
    bargs.extend_from_slice(&["--features", net_feats.as_str()]);
    match cargo_build_kernel_at(repo, &net_dir, &bargs, &net_log) {
        Ok(true) => {}
        Ok(false) => {
            echo_file(&net_log, "");
            rt.fail(case, "the doctored kernel's staticlib build error");
        }
        Err(e) => rt.fail(case, &e),
    }
    let net_triple = net_target.unwrap_or("thumbv7em-none-eabihf");
    let net_lib = format!("src/kernel/target/{}/kernel-net-doctored-resolve/{net_triple}/release/librhu_kernel.a", w.out_dir);
    let net_ctl_rel = format!("{out_rel}/control-net-suite/netsuite-doctored-resolver.elf");
    let mut nargs: Vec<String> = vec!["-T".to_string(), w.map.to_string(), "--gc-sections".to_string(), format!("-Map={out_rel}/control-net-suite/netsuite-doctored-resolver.map")];
    nargs.extend(RT_LINK_WORDS.iter().map(|s| s.to_string()));
    nargs.push(format!("{out_rel}/control-net-suite/netsuite-merged.o"));
    nargs.push(SDK_LOG_LEVEL_WORD.to_string());
    nargs.push(format!("--defsym=__rhu_threadpool_max_threads={}", pool_bound(&bound_row).0));
    nargs.extend(w.shim_objects.iter().map(|s| s.to_string()));
    nargs.extend(w.gc_objects.iter().map(|s| s.to_string()));
    nargs.push(format!("{}/{LIBC}", w.tier));
    nargs.push(net_lib.clone());
    nargs.push("-o".to_string());
    nargs.push(net_ctl_rel.clone());
    let net_ld_log = out.join("net-suite-doctored.ld.stderr");
    match run_tool_bounded(repo, &ld, &nargs, &out.join("net-suite-doctored.ld.stdout"), &net_ld_log) {
        Ok(true) => {}
        Ok(false) => {
            echo_file(&net_ld_log, "  | ");
            rt.fail(case, "the control's image did not link (see its linker stderr log)");
        }
        Err(e) => rt.fail(case, &e),
    }
    let net_ctl_elf = repo.join(net_ctl_rel.replace('/', std::path::MAIN_SEPARATOR_STR));
    let (raw, exit_code) = parity_boot(&mut rt, case, &qemu, w.machine, &net_ctl_elf, &out.join("net-suite-doctored-boot.raw"), &out.join("net-suite-doctored-boot.stderr"));
    let wire_text = String::from_utf8_lossy(&raw).into_owned();
    let wire: Vec<&str> = wire_text.lines().map(|l| l.trim_end_matches('\r')).collect();
    let fails: Vec<&str> = wire.iter().copied().filter(|l| l.starts_with("FAIL ")).collect();
    let others_pass = NET_SUITE_LINES.iter().filter(|l| l.starts_with("PASS ") && !l.starts_with("PASS net-lookup ")).all(|l| wire.contains(l));
    let lookup_passed = wire.iter().any(|l| l.starts_with("PASS net-lookup "));
    if fails != [NET_SUITE_CONTROL_LINE] || !others_pass || lookup_passed || !wire.contains(&NET_SUITE_CONTROL_SUMMARY) || !contains_fixed(&raw, NET_SUITE_RETURN_BAD) || exit_code != 1 {
        rt.fail_control(case, broke, &format!("the doctored kernel was not refused as registered (the one FAIL line '{NET_SUITE_CONTROL_LINE}', the six other sections' PASS lines, '{NET_SUITE_CONTROL_SUMMARY}', '{NET_SUITE_RETURN_BAD}', exit 1): the FAIL lines {fails:?}, the other sections pass {others_pass}, the lookup's PASS line present {lookup_passed}, exit {exit_code}"));
    }
    println!("  refused as designed: '{NET_SUITE_CONTROL_LINE}' alone fails, the six other sections pass, the summary counts one, exit 1");
    rt.pass(case, "negative", broke, "");
    println!("  PASS (control)");

    // 26. The thread pool's bound: the build states the most workers the
    // row holds. The number is this runner's own, computed from the board
    // table (the stack pool, less the kernel's own threads, less the
    // runtime's service threads, in whole default stacks): the build's
    // line in the gate's own publish says it, the image carries it as an
    // absolute symbol, and the bound's program, booted on the emulator,
    // prints the runtime's own answer, is refused one more worker, and
    // runs more work items than the bound to their end.
    let case = "rt.sdk-parity-pool-bound";
    let (most, _, _) = pool_bound(&bound_row);
    let bound_line = pool_bound_line(&bound_row);
    println!("== case {case}: the thread pool's bound for {} -- {most} by this runner's arithmetic over the board table: the build's line in the gate's own publish, the image's absolute symbol, and the bound's program booted on the emulator ==", w.sdk_board);
    let Some(line) = control_line(&controls, "pool-bound-line") else {
        rt.fail(case, "the publish-side script left no summary line for the bound's line");
    };
    let printed = line.split_once("line=").map(|(_, r)| r.trim()).unwrap_or("");
    if control_field(line, "most") != most.to_string() || printed != bound_line {
        rt.fail(case, &format!("the build's line is not this runner's ('{bound_line}'): {line}"));
    }
    let fresh_image = match std::fs::read(&fresh_elf).map_err(|e| format!("cannot read {}: {e}", fresh_elf.display())).and_then(ArmImage::read) {
        Ok(i) => i,
        Err(e) => rt.fail(case, &e),
    };
    match fresh_image.symbol("__rhu_threadpool_max_threads") {
        Some(s) if u64::from(s.value) == most => {}
        Some(s) => rt.fail(case, &format!("the image's word __rhu_threadpool_max_threads is {}, not {most}", s.value)),
        None => rt.fail(case, "the image defines no __rhu_threadpool_max_threads"),
    }
    let Some(line) = control_line(&controls, "pool-bound") else {
        rt.fail(case, "the publish-side script left no summary line for the bound's program");
    };
    if control_field(line, "rc") != "0" || control_field(line, "elf") != "present" {
        rt.fail(case, &format!("the bound's program was not published: {line}"));
    }
    let pool_elf = out.join("control-pool-bound").join("poolbound.elf");
    let (raw, exit_code) = parity_boot(&mut rt, case, &qemu, w.machine, &pool_elf, &out.join("pool-bound-boot.raw"), &out.join("pool-bound-boot.stderr"));
    let items = 2 * most + 2;
    let pool_lines = [
        format!("POOLBOUND: the pool runs at most {most} {}", if most == 1 { "worker" } else { "workers" }),
        format!("POOLBOUND: one more asked: refused; the most is {most}"),
        format!("POOLBOUND: {items} of {items} work items ran; never more at once than the bound: true"),
        "POOLBOUND: end".to_string(),
    ];
    let pool_refs: Vec<&str> = pool_lines.iter().map(String::as_str).collect();
    if let Some(fault) = markers_out_of_order(&raw, &pool_refs) {
        rt.fail(case, &format!("the bound's program's lines: {fault}"));
    }
    if !contains_fixed(&raw, BOOT_RETURN_LINE) {
        rt.fail(case, &format!("the kernel's return line '{BOOT_RETURN_LINE}' not found"));
    }
    for m in FORBIDDEN_PATTERNS {
        if contains_fixed(&raw, m) {
            rt.fail(case, &format!("forbidden pattern '{m}' present"));
        }
    }
    if exit_code != 0 {
        rt.fail(case, &format!("exit {exit_code}, wanted 0"));
    }
    let pool_sha = sha256_file(&pool_elf).unwrap_or_default();
    let pool_rel = format!("{out_rel}/control-pool-bound/poolbound.elf");
    rt.pass_image(case, 0, &pool_rel, &pool_sha, None, &format!("{{\"most\":{most},\"board\":\"{}\"}}", w.sdk_board));
    println!("  PASS (the build's line, the image's word and the runtime's answer all {most}; one more refused; {items} work items ran; the image {}...)", &pool_sha[..16.min(pool_sha.len())]);

    // 27. The network on the external-flash row: its packages carry a kernel
    // with the network and a kernel without it. With no property the image
    // links the first; with the managed memory in the chip's own RAM, and
    // with the network turned off, it links the second and the build says
    // so in a line of its own. Each image is read by its own headers: what
    // it loads in the external flash and in the chip's own, the section of
    // the Ethernet controller's rings and the kernel's network banner.
    let case = "rt.sdk-parity-network-kernel";
    println!("== case {case}: the gate's project published for {XIP_ROW} with no property, with RhuManagedMemory=internal and with RhuNetwork=off -- the kernel with the network by default, the kernel without it for the other two with the build's line saying so; each image read for the Ethernet rings' section and the kernel's network banner ==");
    let boards_text = std::fs::read_to_string(repo.join("src").join("sdk").join("Lamella.Rhu.Sdk").join("Sdk").join("boards.props")).unwrap_or_default();
    let xip_row_text = boards_text.find(&format!("<LamellaTargetBoardRow Include=\"{XIP_ROW}\"")).and_then(|i| boards_text[i..].find("/>").map(|e| &boards_text[i..i + e]));
    let (Some(internal_map), Some(plain_kernel)) = (xip_row_text.and_then(|r| row_attr_text(r, "MapInternal")), xip_row_text.and_then(|r| row_attr_text(r, "KernelNoNetwork"))) else {
        rt.fail(case, &format!("the board table's row {XIP_ROW} names no MapInternal or no KernelNoNetwork"));
    };
    let network_rows: [(&str, &str, &str, &str, &str, &str, bool, bool); 3] = [
        ("program-memory-default", xip_second.as_str(), xip_kernel.as_str(), "external", "on", "none", true, true),
        ("network-internal-heap", internal_map.as_str(), plain_kernel.as_str(), "internal", "off", "managed-memory-internal", false, false),
        ("network-off", xip_second.as_str(), plain_kernel.as_str(), "external", "off", "network-off", true, false),
    ];
    for (label, map, kernel, program, network, without, in_window, with_network) in network_rows {
        let Some(l) = control_line(&controls, label) else {
            rt.fail(case, &format!("the publish-side script left no summary line for {label}"));
        };
        let said = if without == "none" { "0" } else { "1" };
        if control_field(l, "rc") != "0" || control_field(l, "elf") != "present" || control_field(l, "map") != map || control_field(l, "kernel") != kernel || control_field(l, "program") != program || control_field(l, "network") != network || control_field(l, "without-network") != without || control_field(l, "without-network-lines") != said {
            rt.fail(case, &format!("{label} did not run as registered (rc 0, the image present, the map {map}, the kernel {kernel}, the home {program}, the network {network}, the build's line about a kernel without the network {said} time(s), its reason {without}): {l}"));
        }
        let o = std::fs::read(out.join(format!("control-{label}")).join("rthello.o")).unwrap_or_default();
        if o != frozen_obj {
            rt.fail(case, &format!("{label}: the object is not the record's ({} bytes against {}): the kernel is a pick of the link, which must not reach the compile", o.len(), frozen_obj.len()));
        }
        let image = std::fs::read(out.join(format!("control-{label}")).join("rthello.elf")).unwrap_or_default();
        let (loads_window, loads_chip, rings, banner) = match image_net_readings(&image, (win_base, win_end), (chip_base, chip_base + chip_bytes)) {
            Ok(r) => r,
            Err(e) => rt.fail(case, &format!("{label}: {e}")),
        };
        if (loads_window > 0) != in_window || loads_chip == 0 || (rings > 0) != with_network || (banner > 0) != with_network {
            rt.fail(case, &format!("{label}: the image loads {loads_window} B in the external flash and {loads_chip} B in the chip's own, its rings' section is {rings} B and it carries the network banner {banner} time(s) -- wanted: the external flash {}, the chip's own flash loaded, the rings and the banner {}", if in_window { "loaded" } else { "not loaded" }, if with_network { "present" } else { "absent" }));
        }
        println!("  {label}: map {map}, kernel {kernel}, the home {program}, network {network}; {loads_window} B in the external flash, {loads_chip} B in the chip's own; the rings' section {rings} B, the banner {banner} time(s)");
    }
    rt.pass(case, "positive", "", &format!("{{\"board\":\"{XIP_ROW}\",\"publishes\":{}}}", network_rows.len()));
    println!("  PASS");

    // 28. The debugger's exception entries: the program that throws what
    // nothing catches, published for this leg's row in the Debug and the
    // Release configuration; each image read for the entries a debugger
    // stops at when an exception is thrown -- the eight function symbols,
    // the thrown type's record by its symbol, the three fields the message
    // is read through. The control: each image's copy through the tools
    // package's object copier with its strip-all option, which the reader
    // must refuse by name at the missing symbol table.
    let exception_images = [("exception-debug", "Debug"), ("exception-release", "Release")];
    let case = "rt.sdk-parity-exception-entry-symbols";
    println!("== case {case}: the program that throws what nothing catches published for {} in the Debug and the Release configuration -- each image read for the exception entries a debugger stops at: the eight function symbols, the thrown type's record by its symbol, the three fields ==", w.sdk_board);
    for (label, conf) in exception_images {
        let Some(line) = control_line(&controls, label) else {
            rt.fail(case, &format!("the publish-side script left no summary line for the {conf} publish"));
        };
        if control_field(line, "rc") != "0" || control_field(line, "elf") != "present" || control_field(line, "warnings") != "0" {
            rt.fail(case, &format!("the {conf} publish did not run as registered (rc 0, the image present, no warning): {line}"));
        }
        let readings_label = format!("{label}-readings");
        let Some(rline) = control_line(&controls, &readings_label) else {
            rt.fail(case, &format!("the publish-side script left no summary line for the {conf} image's readings"));
        };
        let readings = std::fs::read_to_string(out.join(format!("control-{label}")).join("readings.txt")).unwrap_or_default();
        for l in readings.lines().filter(|l| l.starts_with("READING ") || l.starts_with("readings: ")) {
            println!("  | {conf}: {l}");
        }
        let want = [
            "READING 1 entries: HOLDS -- the 8 function symbols ".to_string(),
            format!("READING 2 type-record: HOLDS -- the symbol {} at 0x", EVENT_PROGRAM.type_symbol),
            "READING 3 fields: HOLDS -- S_P_CoreLib_System_Exception._message at 4; String._stringLength at 4; String._firstChar at 8".to_string(),
            "readings: 3 of 3 hold".to_string(),
        ];
        if control_field(rline, "readings-rc") != "0" || control_field(rline, "held") != "3-of-3" || want.iter().any(|n| !readings.lines().any(|l| l.starts_with(n.as_str()))) {
            rt.fail(case, &format!("the {conf} image's exception entries did not read as registered (the reader's code 0, three readings of three, each line in its registered words): {rline}"));
        }
    }
    rt.pass(case, "positive", "", &format!("{{\"board\":\"{}\",\"images\":2,\"readings\":3}}", w.sdk_board));
    println!("  PASS");
    let case = "rt.sdk-parity-control-exception-entry-stripped";
    let broke = "each image's copy through the tools package's object copier with its strip-all option";
    println!("== case {case} (negative control): {broke} -- the reader must refuse it by name at the missing symbol table, its code 2, no reading printed ==");
    for (label, conf) in exception_images {
        let readings_label = format!("{label}-readings");
        let Some(rline) = control_line(&controls, &readings_label) else {
            rt.fail_control(case, broke, "the publish-side script left no summary line for the stripped copy");
        };
        let image_bytes: u64 = control_field(rline, "image-bytes").parse().unwrap_or(0);
        let stripped_bytes: u64 = control_field(rline, "stripped-bytes").parse().unwrap_or(0);
        if control_field(rline, "stripped-rc") != "2" || control_field(rline, "stripped-refused") != "1" || control_field(rline, "stripped-readings") != "0" || stripped_bytes == 0 || stripped_bytes >= image_bytes {
            rt.fail_control(case, broke, &format!("the {conf} image's stripped copy was not refused as registered (the reader's code 2, its one line naming the missing symbol table, no reading printed, the copy smaller than the image): {rline}"));
        }
        println!("  the {conf} image's stripped copy ({stripped_bytes} of {image_bytes} bytes): refused by name at the missing symbol table, the reader's code 2");
    }
    rt.pass(case, "negative", broke, "");
    println!("  PASS (control)");

    // 29. The debugger's unhandled-exception event, seen through the
    // emulator's debug port in both images: the reading of the image (the
    // events word, the routine, its one BKPT) and four runs -- the word
    // never written, the word written before the startup, the word written
    // at Main with a breakpoint on the instruction, and the word written
    // with none. The control: each image's copy with the routine's BKPT
    // overwritten by a no-operation, refused at the reading by name, no
    // run started.
    let case = "rt.sdk-parity-unhandled-event";
    println!("== case {case}: the unhandled-exception event in the Debug and the Release image of the program that throws, on {} through the emulator's debug port -- the image's reading and four runs each ==", w.machine);
    for (label, conf) in exception_images {
        let image = out.join(format!("control-{label}")).join("unhandled.elf");
        let dir = out.join(format!("event-{label}"));
        let _ = std::fs::remove_dir_all(&dir);
        if std::fs::create_dir_all(&dir).is_err() {
            rt.fail(case, &format!("cannot create {}", dir.display()));
        }
        let lines = event_lines(&qemu, w.machine, &image, &dir, label);
        for (name, ok, text) in &lines {
            println!("  | {conf}: {name} {} -- {text}", if *ok { "HOLDS" } else if name == "READING" { "REFUSED" } else { "FAILS" });
        }
        let held = lines.iter().filter(|l| l.1).count();
        if lines.len() != 1 + EventRun::ALL.len() || held != lines.len() {
            rt.fail(case, &format!("the {conf} image: {held} of {} lines hold (the image's reading and the four runs through the emulator's debug port)", 1 + EventRun::ALL.len()));
        }
    }
    rt.pass(case, "positive", "", &format!("{{\"machine\":\"{}\",\"images\":2,\"runs\":{}}}", w.machine, EventRun::ALL.len()));
    println!("  PASS");
    let case = "rt.sdk-parity-control-unhandled-event-no-bkpt";
    let broke = "each image's copy with the routine's BKPT #0x60 overwritten by a no-operation";
    println!("== case {case} (negative control): {broke} -- refused at the reading by name, no run started ==");
    for (label, conf) in exception_images {
        let image = out.join(format!("control-{label}")).join("unhandled.elf");
        let doctored_path = out.join(format!("control-{label}")).join("unhandled-no-bkpt.elf");
        let verdict = std::fs::read(&image)
            .map_err(|e| format!("cannot read {}: {e}", image.display()))
            .and_then(|b| doctor_event_bkpt(&b))
            .and_then(|d| std::fs::write(&doctored_path, &d).map_err(|e| format!("cannot write {}: {e}", doctored_path.display())).map(|_| d))
            .and_then(ArmImage::read)
            .map(|img| read_event_image(&img).map(|_| ()));
        match verdict {
            Ok(Err(why)) if why.contains("the routine holds no BKPT #0x60") => println!("  the {conf} image's doctored copy: refused at the reading -- {why}"),
            Ok(Err(why)) => rt.fail_control(case, broke, &format!("the {conf} image's doctored copy was refused for another reason: {why}")),
            Ok(Ok(())) => rt.fail_control(case, broke, &format!("the {conf} image's doctored copy was ACCEPTED: its reading holds though the routine's BKPT is gone")),
            Err(e) => rt.fail_control(case, broke, &format!("the {conf} image's doctored copy could not be made or read: {e}")),
        }
    }
    rt.pass(case, "negative", broke, "");
    println!("  PASS (control)");

    // 30. The runtime's stack helper: the class library asks it before work
    // that may nest deeply, and on a board's stacks it must say yes while
    // there is room and no while enough is left to raise its exception. The
    // gate's program for it, published through the SDK for this leg's row
    // and booted on the emulator: a record's ToString() on the thread that
    // runs Main and on a spawned thread, a continuation asked to run
    // synchronously, and a descent that asks the helper at each frame --
    // every line of the program's own, whole and in order, the kernel's
    // return line, no forbidden pattern, exit 0.
    let case = "rt.sdk-parity-stack-helper";
    println!("== case {case}: the stack helper's program published through the SDK for {} and booted on the emulator -- a record's ToString() on two threads, a continuation run synchronously and a descent that asks the helper at each frame, every line of the program's own whole and in order, exit 0 ==", w.sdk_board);
    let Some(line) = control_line(&controls, "stack-helper") else {
        rt.fail(case, "the publish-side script left no summary line for the stack helper's publish");
    };
    if control_field(line, "rc") != "0" || control_field(line, "elf") != "present" {
        rt.fail(case, &format!("the stack helper's program was not published: {line}"));
    }
    let helper_elf = out.join("control-stack-helper").join("stackhelper.elf");
    let (raw, exit_code) = parity_boot(&mut rt, case, &qemu, w.machine, &helper_elf, &out.join("stack-helper-boot.raw"), &out.join("stack-helper-boot.stderr"));
    let frames = match stack_helper_lines(&raw, &STACK_HELPER_LINES) {
        Ok(n) => n,
        Err(fault) => rt.fail(case, &format!("the stack helper's program's lines: {fault}")),
    };
    if !contains_fixed(&raw, BOOT_RETURN_LINE) {
        rt.fail(case, &format!("the kernel's return line '{BOOT_RETURN_LINE}' not found"));
    }
    for m in FORBIDDEN_PATTERNS {
        if contains_fixed(&raw, m) {
            rt.fail(case, &format!("forbidden pattern '{m}' present"));
        }
    }
    if exit_code != 0 {
        rt.fail(case, &format!("exit {exit_code}, wanted 0"));
    }
    let helper_sha = sha256_file(&helper_elf).unwrap_or_default();
    let helper_rel = format!("{out_rel}/control-stack-helper/stackhelper.elf");
    rt.pass_image(case, 0, &helper_rel, &helper_sha, None, &format!("{{\"lines\":{},\"frames\":{frames},\"board\":\"{}\"}}", STACK_HELPER_LINES.len(), w.sdk_board));
    println!("  PASS (the program's {} lines whole and in order; the descent went {frames} frames down; the image {}...)", STACK_HELPER_LINES.len(), &helper_sha[..16.min(helper_sha.len())]);

    // The helper's control: the merged object the publish made, linked by
    // this runner on the SDK's line twice. With the world's own shim objects
    // the link must give the published image's loadable bytes, so the second
    // link differs from the build's by one object alone: the hosting layer's
    // core replaced by the tier's unit that reports a thread's true stack
    // bounds, on which the helper says no at once. The program must then say
    // so by its own lines, whole and in order -- the record throws on both
    // threads, the continuation runs on another thread, the descent goes no
    // frame down -- and end 1.
    let case = "rt.sdk-parity-control-stack-helper-true-bounds";
    let broke = "the stack helper's merged object linked with the shim tier's core unit that reports a thread's true stack bounds (the helper's limit stands above every frame)";
    println!("== case {case} (negative control): {broke} -- the record's ToString() must throw on both threads, the continuation run on another thread and the descent go no frame down, by the program's own lines whole and in order, exit 1 ==");
    let helper_shim: Vec<String> = w.shim_objects.iter().map(|s| s.to_string()).collect();
    let helper_broken = substitute_object(&helper_shim, "goldens/shim/shim-core.o", "goldens/shim/shim-core-break-stack-true-bounds.o", "goldens/shim/shim-core-an505.o", "goldens/shim/shim-core-break-stack-true-bounds-an505.o");
    let mut helper_images: Vec<PathBuf> = Vec::new();
    for (tag, objects) in [("linked-again", &helper_shim), ("true-bounds", &helper_broken)] {
        let rel = format!("{out_rel}/control-stack-helper/stackhelper-{tag}.elf");
        let mut hargs: Vec<String> = vec!["-T".to_string(), w.map.to_string(), "--gc-sections".to_string(), format!("-Map={out_rel}/control-stack-helper/stackhelper-{tag}.map")];
        hargs.extend(RT_LINK_WORDS.iter().map(|s| s.to_string()));
        hargs.push(format!("{out_rel}/control-stack-helper/stackhelper-merged.o"));
        hargs.push(SDK_LOG_LEVEL_WORD.to_string());
        hargs.push(format!("--defsym=__rhu_threadpool_max_threads={}", pool_bound(&bound_row).0));
        hargs.extend(objects.iter().cloned());
        hargs.extend(w.gc_objects.iter().map(|s| s.to_string()));
        hargs.push(format!("{}/{LIBC}", w.tier));
        hargs.push(w.lib.to_string());
        hargs.push("-o".to_string());
        hargs.push(rel.clone());
        let helper_ld_log = out.join(format!("stack-helper-{tag}.ld.stderr"));
        match run_tool_bounded(repo, &ld, &hargs, &out.join(format!("stack-helper-{tag}.ld.stdout")), &helper_ld_log) {
            Ok(true) => {}
            Ok(false) => {
                echo_file(&helper_ld_log, "  | ");
                rt.fail(case, &format!("the {tag} image did not link (see its linker stderr log)"));
            }
            Err(e) => rt.fail(case, &e),
        }
        helper_images.push(repo.join(rel.replace('/', std::path::MAIN_SEPARATOR_STR)));
    }
    // The pack strips the kernel's debug information, so the published
    // image and a link against the whole archive are compared on their
    // loadable bytes, each after the same strip (the link case's comparison).
    let mut loadable: Vec<String> = Vec::new();
    for (from, name) in [(&helper_elf, "stackhelper.loadable.elf"), (&helper_images[0], "stackhelper-linked-again.loadable.elf"), (&helper_images[1], "stackhelper-true-bounds.loadable.elf")] {
        let to = out.join("control-stack-helper").join(name);
        let from_arg = from.to_string_lossy().into_owned();
        let to_arg = to.to_string_lossy().into_owned();
        let oargs = ["--strip-debug", "-R", ".llvmbc", "-R", ".llvmcmd", "-R", ".comment", from_arg.as_str(), to_arg.as_str()];
        match run_captured(&objcopy, &oargs, &out.join(format!("stack-helper-{name}.objcopy.stdout")), &out.join(format!("stack-helper-{name}.objcopy.stderr")), Duration::from_secs(120)) {
            Ok(o) if o.exit == 0 => {}
            Ok(o) => rt.fail(case, &format!("llvm-objcopy did not strip {name} for the comparison (exit {})", o.exit)),
            Err(e) => rt.fail(case, &e),
        }
        loadable.push(sha256_file(&to).unwrap_or_default());
    }
    if loadable[1] != loadable[0] || loadable[0].is_empty() {
        rt.fail(case, &format!("this runner's link of the merged object with the world's own objects has the loadable bytes {}, the published image {} -- the control's link would differ from the build's by more than the one object", loadable[1], loadable[0]));
    }
    if loadable[2] == loadable[0] || loadable[2].is_empty() {
        rt.fail_control(case, broke, &format!("the image linked with the tier's unit has the loadable bytes {}, the published image {}: the unit was not linked in", loadable[2], loadable[0]));
    }
    println!("  the tie: this runner's link with the world's own objects has the published image's loadable bytes ({}...); with the one object replaced they are others ({}...)", &loadable[0][..16.min(loadable[0].len())], &loadable[2][..16.min(loadable[2].len())]);
    let (raw, exit_code) = parity_boot(&mut rt, case, &qemu, w.machine, &helper_images[1], &out.join("stack-helper-true-bounds-boot.raw"), &out.join("stack-helper-true-bounds-boot.stderr"));
    let listed = stack_helper_lines(&raw, &STACK_HELPER_CONTROL_LINES);
    let returned = contains_fixed(&raw, STACK_HELPER_CONTROL_RETURN);
    let carried: Vec<&str> = NET_SUITE_FORBIDDEN.iter().copied().filter(|m| contains_fixed(&raw, m)).collect();
    if listed.is_err() || !returned || exit_code != 1 || !carried.is_empty() {
        let lines_said = match &listed {
            Ok(_) => "as registered".to_string(),
            Err(fault) => fault.clone(),
        };
        rt.fail_control(case, broke, &format!("the true bounds were not refused as registered (the program's {} lines whole and in order, '{STACK_HELPER_CONTROL_RETURN}', exit 1, none of {NET_SUITE_FORBIDDEN:?}): the lines {lines_said}; the return line present {returned}; exit {exit_code}; the patterns found {carried:?}", STACK_HELPER_CONTROL_LINES.len()));
    }
    println!("  refused as designed: the program's {} lines whole and in order -- '{}', the same on a spawned thread, the continuation on another thread, the descent no frame down, '{}' -- and exit 1", STACK_HELPER_CONTROL_LINES.len(), STACK_HELPER_CONTROL_LINES[1], STACK_HELPER_CONTROL_LINES[5]);
    rt.pass(case, "negative", broke, "");
    println!("  PASS (control)");

    for p in SDK_PARITY_PENDING {
        println!("   pending (unarmed, not counted): {p}");
    }
    rt.results.emit_summary(w.sdk_gate, "pass", 0);
    if !validate_results(repo, rt.results.path()) {
        println!("{}: FAIL -- results file failed schema validation", w.sdk_display);
        exit(1);
    }
    // The counts as the results file tallies them, one line per case (the
    // validator refuses a summary that lies about its case lines).
    let negative = rt.results.n_neg as usize;
    let positive = rt.results.n_pass as usize - negative;
    let ties = 5 + modes_offered + 5 + 1 + 2 + 3;
    println!("{}: PASS ({positive} positive cases -- {ties} ties byte for byte, the fifth the same project at another directory under the SDK's path map the {modes_offered} after it the objects published for the external-RAM row in each offered init mode, {modes_withheld} withheld, two the objects published with the managed memory's home internal on this leg's row and the {RAM_INIT_ROW} row, three the objects published for the {XIP_ROW} row with no property, with the chip's own flash named and with the external home named, five the publishes with the output level or a channel named and the last the image's manifest against the expectation composed from its own facts; the board table's stack facts, the boards' facts files and the table's init-mode kernels tied to the tree; the stack knob's image booted with the row's default moved; a real git checkout at another path tying the record; the board class's program booted with the row's facts generated into it; the gate's program booted with the compiler's reflection data stripped; the init mode's kernel picked by the property on the {RAM_INIT_ROW} row and witnessed in the image's rodata, the named default the same image as none; the managed memory's home picked by the property on this leg's row and the {RAM_INIT_ROW} row and witnessed in the images' window symbols against the rows' facts, this leg's row booted under either map for the hosting layer's posture line, the named default the same image as none; the program's home picked by the property on the {XIP_ROW} row and witnessed in the image's window brackets and section table with no undefined name, no property the same image as the external home named, the whole image in the chip's own flash where that is named; the analog voltage switch set per program by the SDK's scan, true on the {RAM_INIT_ROW} row and on the {XIP_ROW} row under the external home beside the kernel without the network with the library's quantity type in the image, false on this leg's row (booted to the program's lines), on the {XIP_ROW} row with the chip's own flash named, for the board class's program and for a project's own false; the board table's external-flash map and its fit column tied to the facts files and the maps; the kernel's output level and the two streams' channels read from the images as the three absolute symbols with the board line's words, the board table's channel facts tied to the boards' facts files; the run verb's device list as .NET renders it and its two commands through dotnet run with the stand-in tool and emulator, the program's code returned unchanged, and the first list of a fresh project whole in the one command that restores it; the run's publish skipped under the no-build option and run again on a moved word, a publish with no C compiler to be found, and a class library built through the SDK and refused at publish; the image's manifest with the execution model native and the stand-in's acceptance of a field it does not know; the line tables of the image read whole with no host separator and the separator task rewriting nothing on a second run; the host inputs as the shell resolves them and the line-endings task's rewrite; the Debug configuration's image for the {DEBUG_ROW} row, the program's own code alone unoptimized in it, linked with its arena window at the row's floor, the image compiled without the optimization as a whole and the one with it on for the program's own code for the {DEBUG_PLAIN_ROW} row, for the {XIP_ROW} row the Debug configuration with the chip's own flash named and the image compiled without the optimization as a whole under the external one, both images booted on this leg's row, the six-line program's Debug image read as a debugger reads it for the {DEBUG_PLAIN_ROW} row and the {DEBUG_ROW} row, a program with a class library referenced by project read at the program's lines, at two functions of one body and at the library's, its image carrying no path of the build machine, a program that reaches a class library through another read at the far library's lines, the same program published again with nothing changed and after a changed statement, and the six-line program's Release image with the optimization off for the program's own code read as a debugger reads it; the network suite booted on this leg's row with three of the socket functions read by the image's symbols; the thread pool's bound in the build's line, the image's word and the booted program's answer, this runner's own number; the {XIP_ROW} row's kernel with the network by default and the kernel without it where the managed memory is in the chip's own RAM or the network is turned off; the exception entries read in the Debug and the Release image of the program that throws what nothing catches, and the unhandled-exception event seen through the emulator's debug port in both -- + {negative} negative controls through the real tool; the Linux environment's transcript under {out_rel})  [{} | {}]", w.sdk_display, facts.version, ld.version);
    exit(0);
}

/// The network suite's lines on the wire, in order; the one line its control must fail by, exact and whole, with
/// the summary and the kernel's return line of that run; and the patterns a passing boot must not carry.
const NET_SUITE_LINES: [&str; 9] = [
    "NETSUITE: start",
    "PASS net-lease (IPv4 alone; the board's own name rhu answered the lease's address 192.0.2.2)",
    "PASS net-interfaces (lo and eth0; eth0 up, an Ethernet interface at 020000000001 with 192.0.2.2 and the mask 255.255.255.0)",
    "PASS net-lookup (peer.rhu.test 192.0.2.1; pair.rhu.test 192.0.2.5 and 192.0.2.6 in the answer's order; second.rhu.test 192.0.2.3 through the asynchronous call; a name the server has not HostNotFound)",
    "PASS net-stream (a listener on port 7777 accepted the client from the board's own address; 26 bytes and 3000 bytes read byte-exact; the client's shutdown read as the end of the stream)",
    "PASS net-options (the peer's reset read ConnectionRefused by a blocking connect and as the pending error after a non-blocking one; the linger state and the two timeouts set and read back; a receive with nothing to read TimedOut at its bound)",
    "PASS net-datagram (two bound datagram sockets: 64 bytes each way with the sender's address; a 2000-byte datagram answered MessageSize; an IPv6 socket asked while both are held AddressFamilyNotSupported)",
    "PASS net-async (an accept, a connect, a send and a receive awaited: 26 bytes echoed)",
    "NETSUITE: ALL GOOD",
];
const NET_SUITE_CONTROL_LINE: &str = "FAIL net-lookup: pair.rhu.test answered 1 address where 2 were expected";
const NET_SUITE_CONTROL_SUMMARY: &str = "NETSUITE: FAIL (1 of 7 sections)";
const NET_SUITE_RETURN_BAD: &str = "[payload] managed Main returned 1";
const NET_SUITE_FORBIDDEN: [&str; 4] = ["KERNEL PANIC", "EXCEPTION:", "REFUSED", "abort()"];

/// The section of the Ethernet controller's rings in a kernel built with
/// the network, and the kernel's network banner words: an image that
/// links the kernel without the network carries neither.
const NET_RINGS_SECTION: &str = ".rhu_net_rings";
const NET_BANNER: &str = "[boot] net: ";

/// Three of the socket functions the class library imports by symbol, one
/// from each of the hosting layer's socket units' families: the image of a
/// program that reaches them must define each once, at its unit's size.
const NET_SUITE_SYMBOLS: [&str; 3] = ["SystemNative_Socket", "SystemNative_GetHostEntryForName", "SystemNative_GetNetworkInterfaces"];

/// The stack helper's program's own lines on the wire, whole and in order:
/// with the hosting layer's repair, and with the tier's unit that reports a
/// thread's true stack bounds in the core's place (the control's exact
/// refusal, with the kernel's return line of that run). Every line the
/// program writes begins with the head; <N> stands for a decimal number, 1
/// or more: the frames its descent went down.
const STACK_HELPER_HEAD: &str = "STACKHELPER: ";
const STACK_HELPER_LINES: [&str; 6] = [
    "STACKHELPER: start",
    "STACKHELPER: on the thread that runs Main a record's ToString() returned \"Reading { Name = abc, Number = 7 }\"",
    "STACKHELPER: on a spawned thread a record's ToString() returned \"Reading { Name = abc, Number = 7 }\"",
    "STACKHELPER: a continuation asked to run synchronously ran on the thread that completed its task",
    "STACKHELPER: a descent that asks the stack helper at each frame went <N> frames down, ended in InsufficientExecutionStackException and was caught",
    "STACKHELPER: end; 4 of 4 checks hold",
];
const STACK_HELPER_CONTROL_LINES: [&str; 6] = [
    "STACKHELPER: start",
    "STACKHELPER: on the thread that runs Main a record's ToString() threw InsufficientExecutionStackException",
    "STACKHELPER: on a spawned thread a record's ToString() threw InsufficientExecutionStackException",
    "STACKHELPER: a continuation asked to run synchronously ran on another thread",
    "STACKHELPER: a descent that asks the stack helper at each frame went 0 frames down, ended in InsufficientExecutionStackException and was caught",
    "STACKHELPER: end; 0 of 4 checks hold",
];
const STACK_HELPER_CONTROL_RETURN: &str = "[payload] managed Main returned 1";

/// The stack helper's program's own lines in a boot's capture against a
/// registered list: every line that begins with the program's head must be
/// the list's line at its place, and the list must be exhausted by them. A
/// registered line that holds <N> matches a decimal number, 1 or more, at
/// that place. Returns the number (0 when the list holds no such line), or
/// the first line that differs.
fn stack_helper_lines(raw: &[u8], want: &[&str]) -> Result<u64, String> {
    let text = String::from_utf8_lossy(raw);
    let own: Vec<&str> = text.lines().map(|l| l.trim_end_matches('\r')).filter(|l| l.starts_with(STACK_HELPER_HEAD)).collect();
    let mut frames = 0u64;
    for (i, w) in want.iter().enumerate() {
        let Some(got) = own.get(i) else {
            return Err(format!("line {} is absent: '{w}' ({} of {} lines printed)", i + 1, own.len(), want.len()));
        };
        match w.split_once("<N>") {
            None => {
                if got != w {
                    return Err(format!("line {}: printed '{got}', registered '{w}'", i + 1));
                }
            }
            Some((before, after)) => {
                let number = got.strip_prefix(before).and_then(|r| r.strip_suffix(after)).unwrap_or("");
                match number.parse::<u64>() {
                    Ok(n) if n >= 1 && n.to_string() == number => frames = n,
                    _ => return Err(format!("line {}: printed '{got}', registered '{w}' with a number of 1 or more for <N>", i + 1)),
                }
            }
        }
    }
    if own.len() != want.len() {
        return Err(format!("{} lines of the program's own were printed and {} are registered; the first beyond them: '{}'", own.len(), want.len(), own[want.len()]));
    }
    Ok(frames)
}

/// The thread pool's bound for a row at its default thread stack, by this
/// runner's own arithmetic over the board table: the stack pool, less the
/// kernel's own threads, less the runtime's service threads (three at the
/// default stack, the pool's gate thread at the row's clamp), in whole
/// default stacks and never fewer than one. Returns the bound, the service
/// threads' bytes and what the pool leaves for workers.
fn pool_bound(row: &StackRow) -> (u64, u64, i64) {
    let service = 3 * row.default + row.clamp;
    let left = row.pool as i64 - row.kernel_own as i64 - service as i64;
    let fit = if left > 0 { left as u64 / row.default } else { 0 };
    (fit.max(1), service, left)
}

/// The build's line for the bound, as the targets print it for a project
/// that names no bound of its own.
fn pool_bound_line(row: &StackRow) -> String {
    let (most, service, left) = pool_bound(row);
    if left >= row.default as i64 {
        format!(
            "Lamella RHU: the thread pool runs at most {most} {} (the board's: of the stack pool's {} B the kernel's own threads hold {} B and the runtime's service threads {service} B, which leaves {left} B for workers of {} B each)",
            if most == 1 { "worker" } else { "workers" },
            row.pool,
            row.kernel_own,
            row.default
        )
    } else {
        format!(
            "Lamella RHU: the thread pool runs at most 1 worker (the board's fewest: of the stack pool's {} B the kernel's own threads hold {} B and the runtime's service threads {service} B, which leaves {left} B, less than one worker's {} B: the one worker can be refused its stack while all four service threads run)",
            row.pool, row.kernel_own, row.default
        )
    }
}

/// The targets' refusal of a project's own most workers above the row's.
fn pool_max_refusal(row: &StackRow, value: &str) -> String {
    let (most, service, left) = pool_bound(row);
    format!(
        "REFUSE: ThreadPoolMaxThreads='{value}' is not a whole number from 1 to {most}, the most thread-pool workers that fit on {}: of the thread stack pool's {} B the kernel's own threads hold {} B and the runtime's service threads {service} B (the finalizer's, the timer's and the socket engine's at {} B each, the pool's gate thread at {} B), which leaves {left} B for workers of {} B each",
        row.board, row.pool, row.kernel_own, row.default, row.clamp, row.default
    )
}

/// The targets' refusal of a project's own fewest workers above the bound.
fn pool_min_refusal(row: &StackRow, value: &str) -> String {
    let (most, _, _) = pool_bound(row);
    format!(
        "REFUSE: ThreadPoolMinThreads='{value}' is not a whole number from 1 to {most}, the thread pool's most workers in this build for {}: the runtime drops both bounds when the fewest is above the most, and a pool without a bound ends the program when the board has no stack left for its next worker",
        row.board
    )
}

/// One published image booted on the leg's emulator as the leg boots every
/// image: the raw console echoed and returned with the emulator's exit
/// code. A hang, or a console without its carriage returns, fails the case.
fn parity_boot(rt: &mut Rt, case: &str, qemu: &Path, machine: &str, elf: &Path, raw_path: &Path, stderr_path: &Path) -> (Vec<u8>, i32) {
    let elf_arg = elf.to_string_lossy().into_owned();
    let mut qargs: Vec<&str> = vec!["-M", machine, "-display", "none", "-monitor", "none", "-serial", "stdio", "-semihosting"];
    qargs.extend_from_slice(ExecProfile::Parity.qemu_args());
    qargs.push("-kernel");
    qargs.push(&elf_arg);
    let outcome = match run_captured(qemu, &qargs, raw_path, stderr_path, Duration::from_secs(RUN_TIMEOUT_S)) {
        Ok(o) => o,
        Err(e) => rt.fail(case, &e),
    };
    let raw = std::fs::read(raw_path).unwrap_or_default();
    echo_file(raw_path, "  | ");
    if outcome.timed_out {
        rt.fail(case, "the boot hung (killed at its timeout)");
    }
    if cr_count(&raw) == 0 {
        rt.fail(case, "no CR bytes in raw capture (wire discipline lost)");
    }
    (raw, outcome.exit)
}

/// Four readings of a linked 32-bit Arm image by its own headers: the
/// bytes it loads inside the first address range and inside the second
/// (the loadable segments' file sizes, by their physical addresses), the
/// size of the section of the Ethernet controller's rings (zero when the
/// image has none) and how many times the file carries the kernel's
/// network banner words.
fn image_net_readings(d: &[u8], window: (u64, u64), chip: (u64, u64)) -> Result<(u64, u64, u64, usize), String> {
    let u16_at = |o: usize| d.get(o..o + 2).map(|b| u16::from_le_bytes([b[0], b[1]]) as usize);
    let u32_at = |o: usize| d.get(o..o + 4).map(|b| u64::from(u32::from_le_bytes([b[0], b[1], b[2], b[3]])));
    if d.len() < 0x34 || &d[..4] != b"\x7fELF" || d[4] != 1 || d[5] != 1 {
        return Err("not a 32-bit little-endian ELF image".to_string());
    }
    let bad = || "the image's headers do not read".to_string();
    let (phoff, shoff) = (u32_at(0x1c).ok_or_else(bad)? as usize, u32_at(0x20).ok_or_else(bad)? as usize);
    let (phentsize, phnum) = (u16_at(0x2a).ok_or_else(bad)?, u16_at(0x2c).ok_or_else(bad)?);
    let (shentsize, shnum, shstrndx) = (u16_at(0x2e).ok_or_else(bad)?, u16_at(0x30).ok_or_else(bad)?, u16_at(0x32).ok_or_else(bad)?);
    let (mut in_window, mut in_chip) = (0u64, 0u64);
    for i in 0..phnum {
        let at = phoff + i * phentsize;
        let (p_type, paddr, filesz) = (u32_at(at).ok_or_else(bad)?, u32_at(at + 12).ok_or_else(bad)?, u32_at(at + 16).ok_or_else(bad)?);
        if p_type != 1 {
            continue;
        }
        if paddr >= window.0 && paddr < window.1 {
            in_window += filesz;
        } else if paddr >= chip.0 && paddr < chip.1 {
            in_chip += filesz;
        }
    }
    let names = u32_at(shoff + shstrndx * shentsize + 16).ok_or_else(bad)? as usize;
    let mut rings = 0u64;
    for i in 0..shnum {
        let at = shoff + i * shentsize;
        let name_at = names + u32_at(at).ok_or_else(bad)? as usize;
        let name = d.get(name_at..).and_then(|t| t.split(|&b| b == 0).next()).ok_or_else(bad)?;
        if name == NET_RINGS_SECTION.as_bytes() {
            rings = u32_at(at + 20).ok_or_else(bad)?;
        }
    }
    let banner = d.windows(NET_BANNER.len()).filter(|w| *w == NET_BANNER.as_bytes()).count();
    Ok((in_window, in_chip, rings, banner))
}

/// The two probes the parity leg's stand-in tool lists, and the second one's description as .NET renders it once the tool's escapes are read back.
const RUN_PROBE_A: &str = "0A1B2C3D4E5F6071";
const RUN_PROBE_B: &str = "1F2E3D4C5B6A79880123ABCD";
const RUN_PROBE_B_WORDS: &str = "ST-LINK/V2-1 ; 50% $(not) @(this) 'single' \"double\" <lt> &amp ? *";

/// The release the feed's manifest names (lamella-rhu-release.json beside the feed the parity leg packs into, as the
/// pack wrote it): the version every package of the release carries and the build package's properties are stamped with.
fn sdk_release_version(repo: &Path) -> Result<String, String> {
    let path = repo.join("src").join("kernel").join("target").join("sdk-feed").join("lamella-rhu-release.json");
    let text = std::fs::read_to_string(&path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    let key = "\"release\":";
    let rest = text.find(key).map(|i| &text[i + key.len()..]).ok_or_else(|| format!("{} names no release", path.display()))?;
    let open = rest.find('"').ok_or_else(|| format!("{} names no release string", path.display()))?;
    let rest = &rest[open + 1..];
    let close = rest.find('"').ok_or_else(|| format!("{} names no release string", path.display()))?;
    Ok(rest[..close].to_string())
}

/// The runtime build the SDK pins, as Sdk.props states it.
fn sdk_runtime_pin(repo: &Path) -> Result<String, String> {
    let path = repo.join("src").join("sdk").join("Lamella.Rhu.Sdk").join("Sdk").join("Sdk.props");
    let text = std::fs::read_to_string(&path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    text.lines()
        .find_map(|l| l.trim().strip_prefix("<RhuRuntimeVersionTested>").and_then(|r| r.strip_suffix("</RhuRuntimeVersionTested>")).map(str::to_string))
        .ok_or_else(|| format!("{} states no RhuRuntimeVersionTested", path.display()))
}

/// The manifest's record as the SDK writes it for the gate's program: the contract's keys in order (the execution model native the third), two-space indentation, LF, a final newline.
fn manifest_text(board: &str, sha: &str, bytes: usize, abi: u32, debug_layout: u32, runtime: &str, console: &str, log: &str) -> String {
    format!(
        "{{\n  \"schema\": 1,\n  \"kind\": \"flash-image\",\n  \"model\": \"native\",\n  \"board\": \"{board}\",\n  \"format\": \"elf\",\n  \"base\": null,\n  \"sha256\": \"{sha}\",\n  \"bytes\": {bytes},\n  \"producer\": \"ilc+rhu-link\",\n  \"elf\": \"rthello.elf\",\n  \"map\": null,\n  \"image_kind\": \"monolith\",\n  \"layout_version\": 1,\n  \"abi\": {abi},\n  \"debug_layout_version\": {debug_layout},\n  \"codegen_profile\": \"optimized\",\n  \"program_codegen\": \"optimized\",\n  \"runtime\": \"{runtime}\",\n  \"runtime_tested\": true,\n  \"console_channel\": \"{console}\",\n  \"log_channel\": \"{log}\"\n}}\n"
    )
}

/// The device list as .NET prints it, cut out of a run's stdout: from the "Available devices:" line through the "Example:" line, LF, a final newline.
fn devices_block(text: &str) -> String {
    let mut out = String::new();
    let mut inside = false;
    for l in text.lines() {
        let l = l.trim_end_matches('\r');
        if l.starts_with("Available devices:") {
            inside = true;
        }
        if inside {
            out.push_str(l);
            out.push('\n');
            if l.starts_with("Example:") {
                break;
            }
        }
    }
    out
}
