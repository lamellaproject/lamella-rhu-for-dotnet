//! The rt gate on the runner: the hosted-runtime world's frozen tier
//! (goldens/rt) -- the class-library program's compiler output merged
//! with the runtime archives at the pin into ONE relocatable object, the
//! C and math libraries' closure merged into another, the archive member
//! whose stack-cookie section the merge takes renamed, and the program's
//! boot golden -- verified host-side, then linked into the hosted image
//! and booted on the emulator. Nothing here compiles: the consumer story
//! is Rust, the pinned toolchain's own object tools and linker, and the
//! emulator; the merges live behind the frozen boundary on the internal
//! rederive leg.

use crate::gate::Selfcheck;
use crate::gates::gc::{parse_undefined, parse_vendored_manifest, resolve_ld, run_tool_bounded, ToolCmd};
use crate::gates::payload::{check_rt_placement, map_region, split_ld_log, RtBrackets, RT_HELLO_BRACKETS};
use crate::gates::shim::{markers_out_of_order, resolve_newlib_dir};
use crate::image::{check_records, elf32_phdrs, header_abi};
use crate::results::{chain_json_from_env, validate_results, Results};
use crate::sha256::sha256_file;
use crate::text::{contains_fixed, cr_count, eq_lf_normalized, payload_stream, word_sym_present, word_sym_value};
use crate::tools::{cargo_build_kernel, cargo_build_kernel_at, echo_file, llvm_tool, resolve_qemu, run_captured, rustc_facts, ExecProfile, RustcFacts};
use std::path::{Path, PathBuf};
use std::process::exit;
use std::time::Duration;

const GATE: &str = "rt";
const DISPLAY: &str = "RT GATE";
const HOST: &str = "qemu-mps2-an500";
const RUNNER: &str = "src/harness";
const RUN_TIMEOUT_S: u64 = 300;

/// goldens/rt/MANIFEST.md's own sha256: the in-code anchor beside the
/// manifest. A re-freeze updates the objects, the lists, the manifest
/// and this pin together in one reviewed commit.
pub(crate) const SHA_FROZEN_MANIFEST: &str = "6e73b465cd06fbc5f1aec32f762201a79243c0fa559b5b8e868d484c7505df0b";

/// The archive set the frozen merge consumed. The manifest's `archives:`
/// line and the frozen set file must both name it; a merge recorded under
/// another set is refused before its bytes are trusted.
const ARCHIVE_SET: &str = "p7-shipped-sgc-enabled";

/// The frozen tier: the file names under goldens/rt/ and what each is.
const FROZEN_FILES: [(&str, &str); 14] = [
    ("rthello.o", "the program object the merge consumed (the compiler's output of record)"),
    ("startup-cookie-copied.o", "the runtime archive's startup member with its stack-cookie section renamed into copied data"),
    ("rthello-merged.o", "the merged object: the program object, the surgery object and the runtime archives, the descriptor rooted, attributes neutralized"),
    ("attr-donor-v7em.bin", "the build-attributes blob the merges apply (a donor compiled at the image ABI)"),
    ("control-attr-a-profile.bin", "the archives' own merged attributes (application profile), the negative control's input"),
    ("archives-p7-shipped-sgc-enabled.txt", "the archive set's identity: each archive by size and sha256, the set's demand and surface shas"),
    ("rthello-merged.undef.txt", "the merged object's undefined names, sorted: the demand surface pinned exactly"),
    ("rt-libc.o", "the C and math libraries' closure merged into one relocatable, attributes neutralized"),
    ("rt-libc.members.txt", "the closure's member list by archive and name, with the kit's two libraries by size and sha256"),
    ("rthello.golden.txt", "the payload stream of the hosted program's boot: the rt.boot golden"),
    ("rtsuite.o", "the suite's program object (the compiler's output of record for src/rt/gate/rtsuite)"),
    ("rtsuite-merged.o", "the suite's merged object: the same recipe from the suite's program object"),
    ("rtsuite-merged.undef.txt", "the suite's merged object's undefined names, sorted: its demand surface pinned exactly"),
    ("rtsuite.golden.txt", "the payload stream of the suite's boot: the rt.suite golden"),
];
const PAYLOAD: &str = "rthello.o";
const SURGERY: &str = "startup-cookie-copied.o";
const MERGED: &str = "rthello-merged.o";
const DONOR: &str = "attr-donor-v7em.bin";
const CONTROL_ATTRS: &str = "control-attr-a-profile.bin";
const SET_FILE: &str = "archives-p7-shipped-sgc-enabled.txt";
const UNDEF_FILE: &str = "rthello-merged.undef.txt";
const LIBC: &str = "rt-libc.o";
const LIBC_MEMBERS: &str = "rt-libc.members.txt";
const GOLDEN: &str = "rthello.golden.txt";
const SUITE_PAYLOAD: &str = "rtsuite.o";
const SUITE_MERGED: &str = "rtsuite-merged.o";
const SUITE_UNDEF_FILE: &str = "rtsuite-merged.undef.txt";
const SUITE_GOLDEN: &str = "rtsuite.golden.txt";

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
/// profile at the image's architecture, Thumb-2, hard-float VFP argument
/// passing.
const REQUIRED_ATTRS: [(&str, &str); 4] = [
    ("CPU_arch_profile", "Microcontroller"),
    ("CPU_arch", "ARM v7E-M"),
    ("THUMB_ISA_use", "Thumb-2"),
    ("ABI_VFP_args", "AAPCS VFP"),
];

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
const RT_LINK_WORDS: [&str; 6] = ["-u", "RhInitialize", "-u", "RhRegisterOSModule", "-u", "DotNetRuntimeContractDescriptor"];
const AN500_MAP: &str = "src/kernel/payload-mps2-an500.x";
const AN500_LIB: &str = "src/kernel/target/thumbv7em-none-eabihf/release/librhu_kernel.a";
/// The frozen shim objects and collector row objects the image links,
/// in the order the line takes them.
const SHIM_OBJECTS: [&str; 5] = [
    "goldens/shim/shim-core.o",
    "goldens/shim/shim-sysnative.o",
    "goldens/shim/shim-libc-glue.o",
    "goldens/shim/shim-stdio-sym.o",
    "goldens/shim/shim-host.o",
];
const GC_OBJECTS: [&str; 3] = ["goldens/gc/skel-row.o", "goldens/gc/heap-row.o", "goldens/gc/collect-row.o"];

/// The diag axis's resource sentence: every rich-flavour class library
/// carries it as a UTF-16 literal, and a min-flavour object carries none
/// (the publish wrapper asserts the same both ways at artifact birth).
const FLAVOUR_MARK: &str = "Object reference not set to an instance";

/// The boot's markers, in the ONE order the contract allows: the kernel's
/// handshake, the shim, the host's steps, the collector through the
/// dynamic-loading seam, the registration, the module list, managed
/// entry, the program's start, one collection, the verdict, the host's
/// exit line, the kernel's return.
const BOOT_MARKERS: [&str; 24] = [
    "[payload] entry: payload-provided, built for hosting-API contract generation 31 (this kernel: 31)",
    "[shim] init: per-thread block registered as module 1 (48 bytes)",
    "[host] stdio: standard error bound to the C library's stream",
    "[host] stdio: standard output line buffered",
    "[host] data: initialized region verified against its load image",
    "[host] ctors run: 0 early, 6 main",
    "[shim] activation: signal 34 handler recorded; the kernel's activation delivery is armed",
    "[shim] pthread_create: stack 16384 B asked, 16384 B spawned (tid 3)",
    "[mgc] dlopen(\"rhu-mgc\") -> skeleton costume handle",
    "[mgc] dlsym(\"GC_VersionInfo\")",
    "[mgc] dlsym(\"GC_Initialize\")",
    "[mgc-skel] call IGCHeap::Initialize -> S_OK",
    "[host] runtime: initialized",
    "[host] runtime: managed code 385282 bytes, unboxing stubs 734 bytes",
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

/// The suite's boot: the platform's lines around the program's (the
/// runtime's own thread spawned at its 16 KB ask, the one worker at the
/// row's default), every section's verdict line, the summary last.
const SUITE_MARKERS: [&str; 15] = [
    "[payload] entry: payload-provided, built for hosting-API contract generation 31 (this kernel: 31)",
    "[shim] activation: signal 34 handler recorded; the kernel's activation delivery is armed",
    "[shim] pthread_create: stack 16384 B asked, 16384 B spawned (tid 3)",
    "[host] entering managed entry",
    "RTSUITE: start",
    "PASS rt-smoke (array sort and string join)",
    "PASS rt-collections (hash map of 64, list of 128 filtered to 64)",
    "PASS rt-text (builder of 86, formats, parse, trim, compare)",
    "PASS rt-exceptions (nested finally, divide by zero, invalid cast, array bound)",
    "PASS rt-gc (4000 dead objects, two collections, weak nulled, strong kept)",
    "PASS rt-time (tick count monotonic across a spin, processor count)",
    "PASS rt-dispatch (interface dispatch, delegate, generic comparison)",
    "[shim] pthread_create: stack 0 B asked, 16384 B spawned (tid 4)",
    "PASS rt-threads (one worker thread, 10000 interlocked increments)",
    "RTSUITE: ALL GOOD",
];
const SUITE_ALL_GOOD: &str = "RTSUITE: ALL GOOD";
/// Patterns a passing suite boot must not carry (the runtime's own
/// unhandled-exception report among them).
const SUITE_FORBIDDEN: [&str; 8] = ["FAIL ", "RTSUITE: FAIL", "Unhandled exception", "KERNEL PANIC", "EXCEPTION:", "REFUSED", "abort()", "unsupported conversion"];
/// The activation count the host prints at exit: the line must be present
/// and parse, and the count is reported. No floor while the activation
/// section is parked: the frozen suite predates the kernel's activation
/// resume completing pending lazy floating-point state, and under the
/// earlier kernel a suite that delivered activations faulted about once
/// in fifteen boots.
const ACTIVATIONS_LINE: &str = "[host] activations delivered: ";
const ACTIVATIONS_FLOOR: u32 = 0;

/// The F769 rider's world: the board's payload map and staticlib (built
/// as the payload gate builds it), and the row's tier objects -- the
/// shim's core object under the F769 selector, the collector's heap and
/// collect objects under the row's defines; the rest shared with the
/// an500 line.
const F769_BOARD: &str = "stm32f769i-disco";
const F769_TARGET: &str = "thumbv7em-none-eabihf";
const F769_FEATURES: &str = "board-stm32f769i-disco,payload";
const F769_MAP: &str = "src/kernel/payload-stm32f769i-disco.x";
const F769_LIB: &str = "src/kernel/target/payload-gate/rt/stm32f769i-disco/thumbv7em-none-eabihf/release/librhu_kernel.a";
const F769_SHIM_OBJECTS: [&str; 5] = [
    "goldens/shim/shim-core-f769.o",
    "goldens/shim/shim-sysnative.o",
    "goldens/shim/shim-libc-glue.o",
    "goldens/shim/shim-stdio-sym.o",
    "goldens/shim/shim-host.o",
];
const F769_GC_OBJECTS: [&str; 3] = ["goldens/gc/skel-row.o", "goldens/gc/heap-row-f769.o", "goldens/gc/collect-row-f769.o"];

/// The section classes the suite's frozen merged object carries at the
/// merge (read off its linked image's brackets at the freeze; the maps'
/// brackets read them exactly).
const SUITE_BRACKETS: RtBrackets = RtBrackets { managed: 421_074, unbox: 958, hydrated: 92_200, tdata: 160, init_array: 24, modules: 4 };
const INTERNAL_CASES: [&str; 1] = [
    "rt.rederive-selector-control -- link-tier; runs under 'rhu-gate rt rederive' when the archive set is resolved: the same merge with the standalone-GC-disabled selector must not tie the frozen object",
];

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
fn attrs_fault(text: &str) -> Option<String> {
    let attrs = parse_attrs(text);
    for (tag, want) in REQUIRED_ATTRS {
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
fn frozen_consistency(records: &[(String, String)], on_disk: &[String]) -> Result<(), String> {
    if records.len() != FROZEN_FILES.len() {
        return Err(format!(
            "manifest parsed to {} records, the frozen tier holds {} -- a record set that drifted",
            records.len(),
            FROZEN_FILES.len()
        ));
    }
    for (name, _) in FROZEN_FILES {
        if !records.iter().any(|(p, _)| p == &format!("goldens/rt/{name}")) {
            return Err(format!("frozen file {name} has no manifest record"));
        }
    }
    for (path, _) in records {
        let Some(name) = path.strip_prefix("goldens/rt/") else {
            return Err(format!("manifest record '{path}' is outside goldens/rt/"));
        };
        if !FROZEN_FILES.iter().any(|(n, _)| n == &name) {
            return Err(format!("manifest record '{path}' has no frozen-tier entry"));
        }
    }
    for name in on_disk {
        if !FROZEN_FILES.iter().any(|(n, _)| *n == name.as_str()) {
            return Err(format!("goldens/rt/{name} has NO manifest record (a file without a record)"));
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
            "{path}: sha256 {actual} != recorded {recorded} (frozen artifact moved -- WRONG, never rebuilt; derivation is 'rhu-gate rt rederive')"
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
                ",\"golden\":{{\"path\":\"goldens/rt/{g}\",\"sha256\":\"{}\",\"compare\":\"bytes\"}}",
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

    fn frozen(&self, name: &str) -> PathBuf {
        self.repo.join("goldens").join("rt").join(name)
    }

    fn record_sha(&mut self, name: &str) -> String {
        match self.records.iter().find(|(p, _)| p == &format!("goldens/rt/{name}")) {
            Some((_, s)) => s.clone(),
            None => self.fail("frozen-inputs", &format!("no record for {name}")),
        }
    }

    fn verify_frozen(&mut self) {
        println!("== frozen inputs: sha256 vs goldens/rt/MANIFEST.md ==");
        let dir = self.repo.join("goldens").join("rt");
        let mpath = dir.join("MANIFEST.md");
        let msha = match sha256_file(&mpath) {
            Ok(s) => s,
            Err(e) => self.fail(
                "frozen-inputs",
                &format!("goldens/rt/MANIFEST.md unreadable: {e} (the frozen tier's record; a drop copy without goldens/rt runs the chain with this tier DECLARED, never this gate)"),
            ),
        };
        if msha != SHA_FROZEN_MANIFEST {
            self.fail(
                "frozen-inputs",
                &format!("goldens/rt/MANIFEST.md sha256 {msha} != pinned {SHA_FROZEN_MANIFEST} -- the frozen record moved without a registered re-freeze"),
            );
        }
        let text = match std::fs::read_to_string(&mpath) {
            Ok(t) => t,
            Err(e) => self.fail("frozen-inputs", &format!("goldens/rt/MANIFEST.md unreadable: {e}")),
        };
        let records = parse_vendored_manifest(&text);
        let mut on_disk: Vec<String> = match std::fs::read_dir(&dir) {
            Ok(rd) => rd
                .flatten()
                .filter_map(|e| e.file_name().to_str().map(str::to_string))
                .filter(|n| n != "MANIFEST.md")
                .collect(),
            Err(e) => self.fail("frozen-inputs", &format!("cannot list goldens/rt: {e}")),
        };
        on_disk.sort();
        if let Err(e) = frozen_consistency(&records, &on_disk) {
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
        let stats = format!("{{\"manifest_sha\":\"{SHA_FROZEN_MANIFEST}\",\"files\":{}}}", FROZEN_FILES.len());
        self.pass("frozen-inputs", "positive", "", &stats);
        println!(
            "== frozen inputs: PASS ({}/{} files tie the manifest both directions; the manifest ties the in-code pin) ==",
            FROZEN_FILES.len(),
            FROZEN_FILES.len()
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
        if let Some(f) = attrs_fault(&attrs) {
            self.fail(case, &f);
        }
        println!(
            "  build attributes: {}",
            REQUIRED_ATTRS.iter().map(|(t, v)| format!("{t}={v}")).collect::<Vec<_>>().join(", ")
        );
        let set_text = match std::fs::read_to_string(self.frozen(SET_FILE)) {
            Ok(t) => t,
            Err(e) => self.fail(case, &format!("{SET_FILE} unreadable: {e}")),
        };
        let manifest = self.manifest_text.clone();
        if let Some(f) = set_fault(&manifest, &set_text, ARCHIVE_SET) {
            self.fail(case, &f);
        }
        let set_sha = manifest
            .lines()
            .map(str::trim)
            .find(|l| l.starts_with("archives: "))
            .and_then(|l| l.split_whitespace().nth(2))
            .unwrap_or("")
            .to_string();
        println!("  archive set: {ARCHIVE_SET} ({}...) named by the manifest and the frozen set file alike", &set_sha[..set_sha.len().min(16)]);
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
            "{{\"merged_sha\":\"{merged_sha}\",\"undefined\":{},\"defined\":{},\"archive_set\":\"{ARCHIVE_SET}\"}}",
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
        let objcopy = match llvm_tool(facts, "llvm-objcopy") {
            Ok(p) => p,
            Err(e) => self.fail(case, &e),
        };
        let readobj = match llvm_tool(facts, "llvm-readobj") {
            Ok(p) => p,
            Err(e) => self.fail(case, &e),
        };
        let copy = self.out.join("merged-attrs-control.o");
        let blob = self.frozen(CONTROL_ATTRS);
        let update = format!(".ARM.attributes={}", blob.to_string_lossy());
        let src = self.frozen(MERGED).to_string_lossy().into_owned();
        let dst = copy.to_string_lossy().into_owned();
        let _ = self.tool_text(&objcopy, case, &["--update-section", &update, &src, &dst], "attrs-control.objcopy.txt");
        let attrs = self.tool_text(&readobj, case, &["--arch-specific", &dst], "attrs-control.attrs.txt");
        let broke = "the archives' application-profile attributes applied to a copy of the merged object";
        match attrs_fault(&attrs) {
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
        let doctored = self.manifest_text.replace(&format!("archives: {ARCHIVE_SET} "), "archives: p7-shipped-sgc-disabled ");
        if doctored == self.manifest_text {
            self.fail(case, "the control could not doctor the manifest's archives line");
        }
        let set_text = std::fs::read_to_string(self.frozen(SET_FILE)).unwrap_or_default();
        let broke = "the manifest's archives line naming another set";
        match set_fault(&doctored, &set_text, ARCHIVE_SET) {
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
        if let Some(f) = attrs_fault(&attrs) {
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

    /// The hosted image linked under the an500 payload map: the link line
    /// of record, the artifact read.
    fn case_link(&mut self, facts: &RustcFacts, ld: &ToolCmd, abi: u32) -> (String, String) {
        let case = "rt.link";
        println!("== case {case} ==");
        let log = self.out.join("staticlib-build.log");
        let ok = match cargo_build_kernel(&self.repo, &["--lib", "--features", "payload"], &log) {
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
        let path_rel = "src/kernel/target/rt-gate/rt-link.elf".to_string();
        let map_rel = "src/kernel/target/rt-gate/rt-link.map".to_string();
        let mut args: Vec<String> = vec!["-T".to_string(), AN500_MAP.to_string(), "--gc-sections".to_string(), format!("-Map={map_rel}")];
        args.extend(RT_LINK_WORDS.iter().map(|s| s.to_string()));
        args.push(format!("goldens/rt/{MERGED}"));
        args.extend(SHIM_OBJECTS.iter().map(|s| s.to_string()));
        args.extend(GC_OBJECTS.iter().map(|s| s.to_string()));
        args.push(format!("goldens/rt/{LIBC}"));
        args.push(AN500_LIB.to_string());
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
        let map_text = std::fs::read_to_string(self.repo.join(AN500_MAP)).unwrap_or_default();
        let Some(code) = map_region(&map_text, "CODE") else {
            self.fail(case, &format!("{AN500_MAP} states no CODE region"));
        };
        let demand = match std::fs::read_to_string(self.frozen(UNDEF_FILE)) {
            Ok(t) => parse_list(&t),
            Err(e) => self.fail(case, &format!("{UNDEF_FILE} unreadable: {e}")),
        };
        let facts_p = match check_rt_placement(dump, &phdrs, &[], &demand, code, &RT_HELLO_BRACKETS) {
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
        match record_fault(&format!("goldens/rt/{MERGED}"), &flipped, &want) {
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
        let mut args: Vec<&str> = vec!["-M", "mps2-an500", "-display", "none", "-monitor", "none", "-serial", "stdio", "-semihosting"];
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
            self.fail(case, &format!("payload stream differs from goldens/rt/{GOLDEN} (byte class)"));
        }
        if let Some(fault) = markers_out_of_order(&raw, &BOOT_MARKERS) {
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

    /// The F769 rider: the frozen merged runtime object linked under the
    /// F769 payload map with the F769 payload staticlib and the row's own
    /// tier objects (the shim's core object under the F769 selector, the
    /// collector's heap and collect objects under the F769 row's defines;
    /// every other object shared with the an500 line, which the rederive
    /// legs prove row-independent). Link-only here: the board's boot is the
    /// kernel's flight, and this artifact is what it flies.
    fn case_link_board(&mut self, facts: &RustcFacts, ld: &ToolCmd, abi: u32) {
        let case = "rt.link-stm32f769i-disco";
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
        let path_rel = format!("src/kernel/target/rt-gate/rt-link-{F769_BOARD}.elf");
        let map_rel = format!("src/kernel/target/rt-gate/rt-link-{F769_BOARD}.map");
        let mut args: Vec<String> = vec!["-T".to_string(), F769_MAP.to_string(), "--gc-sections".to_string(), format!("-Map={map_rel}")];
        args.extend(RT_LINK_WORDS.iter().map(|s| s.to_string()));
        args.push(format!("goldens/rt/{MERGED}"));
        args.extend(F769_SHIM_OBJECTS.iter().map(|s| s.to_string()));
        args.extend(F769_GC_OBJECTS.iter().map(|s| s.to_string()));
        args.push(format!("goldens/rt/{LIBC}"));
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
        let facts_p = match check_rt_placement(dump, &phdrs, &[], &demand, code, &RT_HELLO_BRACKETS) {
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
    /// the host's activation count at exit read and reported (no floor
    /// while the activation section is parked). Returns the raw
    /// capture for the break-class control.
    fn case_suite(&mut self, facts: &RustcFacts, ld: &ToolCmd, qemu: &Path, abi: u32) -> Vec<u8> {
        let case = "rt.suite";
        println!("== case {case} ==");
        let path_rel = "src/kernel/target/rt-gate/rt-suite.elf".to_string();
        let map_rel = "src/kernel/target/rt-gate/rt-suite.map".to_string();
        let mut args: Vec<String> = vec!["-T".to_string(), AN500_MAP.to_string(), "--gc-sections".to_string(), format!("-Map={map_rel}")];
        args.extend(RT_LINK_WORDS.iter().map(|s| s.to_string()));
        args.push(format!("goldens/rt/{SUITE_MERGED}"));
        args.extend(SHIM_OBJECTS.iter().map(|s| s.to_string()));
        args.extend(GC_OBJECTS.iter().map(|s| s.to_string()));
        args.push(format!("goldens/rt/{LIBC}"));
        args.push(AN500_LIB.to_string());
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
        let map_text = std::fs::read_to_string(self.repo.join(AN500_MAP)).unwrap_or_default();
        let Some(code) = map_region(&map_text, "CODE") else {
            self.fail(case, &format!("{AN500_MAP} states no CODE region"));
        };
        let Some(phdrs) = elf32_phdrs(&elf_bytes) else {
            self.fail(case, "program headers not parseable");
        };
        let demand = match std::fs::read_to_string(self.frozen(SUITE_UNDEF_FILE)) {
            Ok(t) => parse_list(&t),
            Err(e) => self.fail(case, &format!("{SUITE_UNDEF_FILE} unreadable: {e}")),
        };
        let facts_p = match check_rt_placement(dump, &phdrs, &[], &demand, code, &SUITE_BRACKETS) {
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
        let mut qargs: Vec<&str> = vec!["-M", "mps2-an500", "-display", "none", "-monitor", "none", "-serial", "stdio", "-semihosting"];
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
            self.fail(case, &format!("payload stream differs from goldens/rt/{SUITE_GOLDEN} (byte class)"));
        }
        if let Some(fault) = markers_out_of_order(&raw, &SUITE_MARKERS) {
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
        let stats = format!("{{\"stream_lines\":{lines},\"markers\":{},\"activations\":{activations},\"activations_floor\":{ACTIVATIONS_FLOOR}}}", SUITE_MARKERS.len());
        self.pass_image(case, 0, &path_rel, &sha, Some(SUITE_GOLDEN), &stats);
        println!(
            "== case {case}: PASS (exit 0; the payload stream equals its golden, {lines} lines with the verdict last; {} markers IN ORDER incl. eight section verdicts; the host reports {activations} activations delivered (report-only at floor {ACTIVATIONS_FLOOR}: the activation section is parked); then the kernel's return line) ==",
            SUITE_MARKERS.len()
        );
        raw
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
        if markers_out_of_order(&doctored, &SUITE_MARKERS).is_none() {
            self.fail_control(case, broke, "the doctored capture carried every marker in order");
        }
        println!("  refused as designed: the golden compare, the forbidden sweep and the marker order each reject one failed section");
        self.pass(case, "negative", broke, "");
        println!("  PASS (control)");
    }
}

fn open(repo: &Path, gate_id: &'static str, display: &'static str, file: &str) -> Rt {
    let out = repo.join("src").join("kernel").join("target").join("rt-gate");
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
    Rt { repo: repo.to_path_buf(), out, results, gate_id, display, records: Vec::new(), manifest_text: String::new() }
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

pub fn run(repo: &Path) -> ! {
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
    let (qemu, qemu_v) = match resolve_qemu() {
        Ok(q) => q,
        Err(e) => {
            println!("rhu-gate: {e}");
            exit(1);
        }
    };
    let abi = read_abi(repo);
    let mut rt = open(repo, GATE, DISPLAY, "rt.results.jsonl");
    rt.results.emit(&format!(
        "{{\"schema\":1,\"type\":\"run\",\"gate\":\"rt\",\"run\":\"rt-{}\",\"host\":\"{HOST}\",\"runner\":\"{RUNNER}\",\"chain\":{},\"tools\":{{\"qemu\":\"{}\",\"rustc\":\"{}\",\"ld\":\"{}\"}}}}",
        std::process::id(),
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
    rt.case_link_board(&facts, &ld, abi);
    rt.case_boot(&qemu, &path_rel, &sha);
    rt.control_boot_golden();
    let suite_raw = rt.case_suite(&facts, &ld, &qemu, abi);
    rt.control_suite_golden();
    rt.control_suite_break(&suite_raw);
    for p in PENDING {
        println!("   pending (unarmed, not counted): {p}");
    }
    for c in INTERNAL_CASES {
        println!("   internal (rederive leg, not counted): {c}");
    }
    rt.results.emit_summary(GATE, "pass", 0);
    if !validate_results(repo, rt.results.path()) {
        println!("{DISPLAY}: FAIL -- results file failed schema validation");
        exit(1);
    }
    println!(
        "{DISPLAY}: PASS (14/14 cases incl. 7 negative controls; {} pending unarmed; {} internal on the rederive leg)  [{} | {} | {}]",
        PENDING.len(),
        INTERNAL_CASES.len(),
        qemu_v,
        facts.version,
        ld.version
    );
    exit(0);
}

/// The archive set's directory: RHU_RT_ARCHIVES, else the first line of
/// the untracked machine-local tools/toolchains/rtarchives.local (the
/// directory in the form the linker opens), else None.
fn resolve_archives(repo: &Path) -> Option<String> {
    if let Ok(v) = std::env::var("RHU_RT_ARCHIVES") {
        if !v.trim().is_empty() {
            return Some(v.trim().to_string());
        }
    }
    let local = repo.join("tools").join("toolchains").join("rtarchives.local");
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

pub fn rederive(repo: &Path) -> ! {
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
    let mut rt = open(repo, "rt-rederive", "RT REDERIVE", "rt-rederive.results.jsonl");
    rt.results.emit(&format!(
        "{{\"schema\":1,\"type\":\"run\",\"gate\":\"rt-rederive\",\"run\":\"rt-rederive-{}\",\"host\":\"host-native\",\"runner\":\"{RUNNER}\",\"tools\":{{\"rustc\":\"{}\",\"ld\":\"{}\"}}}}",
        std::process::id(),
        facts.version,
        ld.version
    ));
    rt.verify_frozen();
    let Some(dir) = resolve_archives(repo) else {
        println!("RT REDERIVE: NOT RUN (no archive set resolved: set RHU_RT_ARCHIVES to the runtime pack's native directory at the pin, or put that directory in tools/toolchains/rtarchives.local)");
        exit(1);
    };
    let Some(kit) = resolve_newlib_dir(repo) else {
        println!("RT REDERIVE: NOT RUN (the toolchain's C library is not resolved: set RHU_NEWLIB or write tools/toolchains/newlib.local)");
        exit(1);
    };
    let donor_update = format!(".ARM.attributes={}", rt.frozen(DONOR).to_string_lossy());

    let case = "rt.rederive-surgery";
    println!("== case {case}: the startup member printed out of {SURGERY_ARCHIVE} at {dir} and its cookie section renamed ==");
    let set_text = std::fs::read_to_string(rt.frozen(SET_FILE)).unwrap_or_default();
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
    let got = sha256_file(&renamed).unwrap_or_default();
    let want = rt.record_sha(SURGERY);
    if got != want {
        rt.fail(case, &format!("re-derived surgery object sha256 {got} != frozen {want} -- the archive member or the rename moved; a moved object is a registered re-freeze"));
    }
    println!("  re-derived surgery object byte-identical to goldens/rt/{SURGERY} ({}...)", &want[..16]);
    rt.pass(case, "positive", "", &format!("{{\"surgery_sha\":\"{want}\"}}"));
    println!("  PASS");

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
        let pre_s = pre.to_string_lossy().into_owned();
        let post_s = post.to_string_lossy().into_owned();
        let _ = rt.tool_text(&objcopy, case, &["--update-section", &donor_update, &pre_s, &post_s], &format!("rederive-{tag}.objcopy.txt"));
        post
    };
    let post = merge(&mut rt, PAYLOAD, "libstandalonegc-enabled.a", "enabled");
    let got = match sha256_file(&post) {
        Ok(s) => s,
        Err(e) => rt.fail(case, &format!("re-derived object unreadable: {e}")),
    };
    let want = rt.record_sha(MERGED);
    if got != want {
        rt.fail(
            case,
            &format!("re-derived merged object sha256 {got} != frozen {want} -- the merge moved (a linker, an archive, the surgery object or the program object changed); a moved object is a registered re-freeze, never a silent rebuild"),
        );
    }
    println!("  re-derived object byte-identical to goldens/rt/{MERGED} ({}...)", &want[..16]);
    rt.pass(case, "positive", "", &format!("{{\"merged_sha\":\"{want}\",\"archives\":{}}}", MERGE_ARCHIVES.len()));
    println!("  PASS");

    let case = "rt.rederive-merge-suite";
    println!("== case {case}: the suite's merge re-derived from the frozen suite object, the frozen surgery object and the archive set ==");
    let post_suite = merge(&mut rt, SUITE_PAYLOAD, "libstandalonegc-enabled.a", "enabled-suite");
    let got = match sha256_file(&post_suite) {
        Ok(s) => s,
        Err(e) => rt.fail(case, &format!("re-derived suite object unreadable: {e}")),
    };
    let want = rt.record_sha(SUITE_MERGED);
    if got != want {
        rt.fail(case, &format!("re-derived suite merge sha256 {got} != frozen {want} -- the merge moved (a linker, an archive, the surgery object or the suite's program object changed); a moved object is a registered re-freeze, never a silent rebuild"));
    }
    println!("  re-derived object byte-identical to goldens/rt/{SUITE_MERGED} ({}...)", &want[..16]);
    rt.pass(case, "positive", "", &format!("{{\"merged_sha\":\"{want}\",\"archives\":{}}}", MERGE_ARCHIVES.len()));
    println!("  PASS");

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
    let pre_s = pre.to_string_lossy().into_owned();
    let post_s = post.to_string_lossy().into_owned();
    let _ = rt.tool_text(&objcopy, case, &["--update-section", &donor_update, &pre_s, &post_s], "rederive-libc.objcopy.txt");
    let got = sha256_file(&post).unwrap_or_default();
    let want = rt.record_sha(LIBC);
    if got != want {
        rt.fail(case, &format!("re-derived closure sha256 {got} != frozen {want} -- the kit's members, their order or the linker moved; a moved object is a registered re-freeze"));
    }
    println!("  {} members printed out of the archives and re-merged in the list's order; byte-identical to goldens/rt/{LIBC} ({}...)", members.members.len(), &want[..16]);
    rt.pass(case, "positive", "", &format!("{{\"libc_sha\":\"{want}\",\"members\":{}}}", members.members.len()));
    println!("  PASS");

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
    rt.results.emit_summary("rt-rederive", "pass", 0);
    if !validate_results(repo, rt.results.path()) {
        println!("RT REDERIVE: FAIL -- results file failed schema validation");
        exit(1);
    }
    println!(
        "RT REDERIVE: PASS (4/4 frozen objects re-derived byte-identical: the surgery object, the two merged objects, the closure; the archive set and the kit's libraries tie the frozen identities; selector control refused)  [{}]",
        ld.version
    );
    exit(0);
}

pub fn selfcheck(repo: &Path) -> ! {
    let facts = match rustc_facts() {
        Ok(f) => f,
        Err(e) => {
            println!("rhu-gate: {e}");
            exit(1);
        }
    };
    let out = repo.join("src").join("kernel").join("target").join("rt-gate");
    let _ = std::fs::create_dir_all(&out);
    let results = match Results::create(&out.join("rt-selfcheck.results.jsonl")) {
        Ok(r) => r,
        Err(e) => {
            println!("rhu-gate: cannot create results file: {e}");
            exit(1);
        }
    };
    results.emit(&format!(
        "{{\"schema\":1,\"type\":\"run\",\"gate\":\"rt-selfcheck\",\"run\":\"rt-selfcheck-{}\",\"host\":\"{HOST}\",\"runner\":\"{RUNNER}\",\"tools\":{{\"rustc\":\"{}\"}}}}",
        std::process::id(),
        facts.version
    ));
    let mut sc = Selfcheck { repo: repo.to_path_buf(), gate: "rt-selfcheck".to_string(), display: "RT SELFCHECK".to_string(), results, n: 0, npass: 0, nneg: 0 };

    let header_ok = "File: x\nFormat: elf32-littlearm\nElfHeader {\n  Class: 32-bit (0x1)\n  Type: Relocatable (0x1)\n  Machine: EM_ARM (0x28)\n}\n";
    sc.accept_probe("header-genuine", "a 32-bit ARM relocatable header accepted", header_fault(header_ok).is_none());
    sc.probe("header-executable", "an executable's header (Type: Executable) must be refused", header_fault(&header_ok.replace("Type: Relocatable (0x1)", "Type: Executable (0x2)")).is_none());
    sc.probe("header-x86", "a non-ARM header (Machine: EM_386) must be refused", header_fault(&header_ok.replace("Machine: EM_ARM (0x28)", "Machine: EM_386 (0x3)")).is_none());

    let attrs_m = "BuildAttributes {\n  Attribute {\n    Tag: 5\n    TagName: CPU_name\n    Value: 7E-M\n  }\n  Attribute {\n    Tag: 6\n    Value: 13\n    TagName: CPU_arch\n    Description: ARM v7E-M\n  }\n  Attribute {\n    Tag: 7\n    Value: 77\n    TagName: CPU_arch_profile\n    Description: Microcontroller\n  }\n  Attribute {\n    Tag: 9\n    Value: 2\n    TagName: THUMB_ISA_use\n    Description: Thumb-2\n  }\n  Attribute {\n    Tag: 28\n    Value: 1\n    TagName: ABI_VFP_args\n    Description: AAPCS VFP\n  }\n}\n";
    sc.accept_probe("attrs-genuine", "the microcontroller-profile hard-float attributes accepted", attrs_fault(attrs_m).is_none());
    let attrs_a = attrs_m.replace("Description: ARM v7E-M", "Description: ARM v7").replace("Description: Microcontroller", "Description: Application");
    sc.probe("attrs-application-profile", "the archives' own application-profile attributes must be refused", attrs_fault(&attrs_a).is_none());
    let attrs_lld_pre = "BuildAttributes {\n  Attribute {\n    Tag: 6\n    Value: 2\n    TagName: CPU_arch\n    Description: ARM v4T\n  }\n  Attribute {\n    Tag: 28\n    Value: 1\n    TagName: ABI_VFP_args\n    Description: AAPCS VFP\n  }\n}\n";
    sc.probe("attrs-profile-absent", "a merged object whose profile tag is absent (the linker's own minimal section) must be refused", attrs_fault(attrs_lld_pre).is_none());
    let attrs_soft = attrs_m.replace("Description: AAPCS VFP", "Description: AAPCS");
    sc.probe("attrs-soft-float", "microcontroller attributes with soft-float argument passing must be refused", attrs_fault(&attrs_soft).is_none());

    let want: Vec<String> = ["a", "b", "c"].iter().map(|s| s.to_string()).collect();
    sc.accept_probe("undef-genuine", "an undefined set equal to the frozen list accepted", undef_fault(&want, &want).is_none());
    let extra: Vec<String> = ["a", "b", "c", "d"].iter().map(|s| s.to_string()).collect();
    sc.probe("undef-extra", "an undefined set with one NEW name must be refused", undef_fault(&extra, &want).is_none());
    let missing: Vec<String> = ["a", "c"].iter().map(|s| s.to_string()).collect();
    sc.probe("undef-missing", "an undefined set with one VANISHED name must be refused", undef_fault(&missing, &want).is_none());

    let manifest = "# goldens/rt\n\narchives: p7-shipped-sgc-enabled abcd\n\n- `goldens/rt/x.o`\n  `0000000000000000000000000000000000000000000000000000000000000000`\n";
    let set_file = "set-id: p7-shipped-sgc-enabled\narchive libRuntime.WorkstationGC.a 1 ff\narchives-sha256: abcd\n";
    sc.accept_probe("set-genuine", "a manifest and set file naming the consumed set with equal shas accepted", set_fault(manifest, set_file, ARCHIVE_SET).is_none());
    sc.probe("set-other-id", "a manifest naming another archive set must be refused", set_fault(&manifest.replace("p7-shipped-sgc-enabled", "p7-shipped-sgc-disabled"), set_file, ARCHIVE_SET).is_none());
    sc.probe("set-sha-disagree", "a set file whose archives sha disagrees with the manifest must be refused", set_fault(manifest, &set_file.replace("archives-sha256: abcd", "archives-sha256: abce"), ARCHIVE_SET).is_none());

    let records: Vec<(String, String)> = FROZEN_FILES.iter().map(|(n, _)| (format!("goldens/rt/{n}"), "0".repeat(64))).collect();
    let on_disk: Vec<String> = FROZEN_FILES.iter().map(|(n, _)| n.to_string()).collect();
    let mut extra_disk = on_disk.clone();
    extra_disk.push("stray.o".to_string());
    sc.probe("frozen-file-without-record", "an on-disk file without a manifest record must be refused", frozen_consistency(&records, &extra_disk).is_ok());
    let mut outside = records.clone();
    outside[0].0 = "goldens/shim/rthello.o".to_string();
    sc.probe("frozen-record-outside", "a manifest record outside goldens/rt/ must be refused", frozen_consistency(&outside, &on_disk).is_ok());
    let sha_a = "a".repeat(64);
    sc.accept_probe("record-genuine", "a record equal to the file's sha accepted", record_fault("goldens/rt/x.o", &sha_a, &sha_a).is_none());
    let sha_b = format!("{}b", "a".repeat(63));
    sc.probe("record-flipped", "a record with one flipped digit must be refused", record_fault("goldens/rt/x.o", &sha_b, &sha_a).is_none());

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

    let golden = std::fs::read(repo.join("goldens").join("rt").join(GOLDEN)).unwrap_or_default();
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

    let suite_ordered: Vec<u8> = SUITE_MARKERS.join("\r\n").into_bytes();
    sc.accept_probe("suite-order-genuine", "a capture carrying the suite's markers in order", markers_out_of_order(&suite_ordered, &SUITE_MARKERS).is_none());
    let mut suite_swapped: Vec<&str> = SUITE_MARKERS.to_vec();
    suite_swapped.swap(5, 13);
    let suite_swapped_capture: Vec<u8> = suite_swapped.join("\r\n").into_bytes();
    sc.probe("suite-order-swapped", "the threading section's verdict before the first section's", markers_out_of_order(&suite_swapped_capture, &SUITE_MARKERS).is_none());
    sc.accept_probe("activations-genuine", "an activation line at the floor accepted", activation_count(b"[host] activations delivered: 3\r\n", ACTIVATIONS_FLOOR).is_ok());
    sc.probe("activations-below-floor", "an activation line below a floor of three must be refused (the mechanism; the shipped floor is 0 while the activation section is parked)", activation_count(b"[host] activations delivered: 0\r\n", 3).is_ok());
    sc.probe("activations-absent", "a capture without the activation line must be refused", activation_count(b"[host] exit: 0\r\n", ACTIVATIONS_FLOOR).is_ok());
    let suite_broken: Vec<u8> = String::from_utf8_lossy(&suite_ordered).replace("PASS rt-threads", "FAIL rt-threads").into_bytes();
    sc.probe("suite-break-forbidden", "a capture with one section's verdict turned FAIL must trip the forbidden sweep", !SUITE_FORBIDDEN.iter().any(|m| contains_fixed(&suite_broken, m)));
    sc.probe("suite-break-order", "a capture with one section's verdict turned FAIL must lose that marker", markers_out_of_order(&suite_broken, &SUITE_MARKERS).is_none());

    let manifest_short = manifest.replace("0000000000000000000000000000000000000000000000000000000000000000", "000000000000000000000000000000000000000000000000000000000000000");
    sc.probe("manifest-sha-malformed", "a manifest record whose sha is 63 hex digits must parse to fewer records (a drifted record set)", parse_vendored_manifest(&manifest_short).len() == parse_vendored_manifest(manifest).len());

    sc.finish("genuine inputs accepted");
}
