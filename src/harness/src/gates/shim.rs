//! The shim gate on the runner: the hosting shim's gate family, running
//! on FROZEN world objects and goldens (goldens/shim) -- the consumer
//! story is Rust + QEMU only; the shim's C compile happens once, on the
//! maintainer's re-derivation leg ('rhu-gate shim rederive'), and its
//! objects are committed.

use crate::gate::{class_flag_sets_tie, write_fixture, Class, Selfcheck};
use crate::gates::gc::{parse_undefined, parse_vendored_manifest, resolve_clang, resolve_ld, run_tool_bounded, ToolCmd};
use crate::image::{header_abi, header_abi_compat};
use crate::objfile::{distinct_stamps, producer_stamp_of, section_span, section_span_prefixed, sha256_without_stamp, symbol_word};
use crate::results::{chain_json_from_env, validate_results, Results};
use crate::sha256::{sha256_file, sha256_hex};
use crate::text::{contains_fixed, cr_count, eq_lf_normalized, payload_stream, word_sym_present, word_sym_value};
use crate::tools::{echo_file, llvm_tool, resolve_qemu, run_captured, rustc_facts, ExecProfile, RustcFacts};
use std::path::{Path, PathBuf};
use std::process::exit;
use std::time::Duration;

const GATE: &str = "shim";
const DISPLAY: &str = "SHIM GATE";
const HOST: &str = "qemu-mps2-an500";
const RUNNER: &str = "src/harness";
const RUN_TIMEOUT_S: u64 = 120;

/// The compile flag set of record for the shim's C: clang, the compiler of
/// record (the version the runtime archives at the pin
/// were built with), running the C front end for the image ABI half --
/// v7E-M, hard-float FPv5-D16, to match thumbv7em-none-eabihf -- with no
/// implicit floating-point use (the activation costume carries no
/// floating-point instruction, and the tier's census pins every object's
/// count), no C library assumed, every diagnostic an error, and the
/// include closure (the shim, the board rows, rhu.h); the kit's C-library
/// headers are the sysroot the resolver appends.
const CC_FLAGS: [&str; 21] = [
    "-x",
    "c",
    "-c",
    "-O2",
    "--target=arm-none-eabi",
    "-mcpu=cortex-m7",
    "-mthumb",
    "-mfloat-abi=hard",
    "-mfpu=fpv5-d16",
    "-mno-implicit-float",
    "-ffunction-sections",
    "-fdata-sections",
    "-ffreestanding",
    "-fno-common",
    "-std=gnu11",
    "-Wall",
    "-Wextra",
    "-Werror",
    "-Isrc/shim",
    "-Isrc/gc/host",
    "-Isrc/kernel/include",
];

/// The same flag set for the Cortex-M33 (the mps2-an505 row's objects):
/// one list apart from the core and FPU words, which the selfcheck asserts
/// against `CC_FLAGS` through the shared class record.
const CC_FLAGS_CORTEX_M33: [&str; 21] = [
    "-x",
    "c",
    "-c",
    "-O2",
    "--target=arm-none-eabi",
    "-mcpu=cortex-m33",
    "-mthumb",
    "-mfloat-abi=hard",
    "-mfpu=fpv5-sp-d16",
    "-mno-implicit-float",
    "-ffunction-sections",
    "-fdata-sections",
    "-ffreestanding",
    "-fno-common",
    "-std=gnu11",
    "-Wall",
    "-Wextra",
    "-Werror",
    "-Isrc/shim",
    "-Isrc/gc/host",
    "-Isrc/kernel/include",
];

/// The flag set of record for a class.
fn cc_flags(class: Class) -> &'static [&'static str] {
    match class {
        Class::CortexM7 => &CC_FLAGS,
        Class::CortexM33 => &CC_FLAGS_CORTEX_M33,
    }
}

/// goldens/shim/MANIFEST.md's own sha256: the in-code anchor beside the
/// manifest. A re-freeze updates objects + goldens + manifest + this pin
/// together in one reviewed commit.
const SHA_FROZEN_MANIFEST: &str = "ff352b6225db86e593eece3ac4c97023f31375db49537dbf62ff4805cf8a7acd";

/// The frozen-object plan of record: file name under goldens/shim/,
/// source TU, extra defines beyond the class's flag set, and the class
/// (the processor class whose flag set compiles the row's objects). The
/// manifest's records must tie this table by
/// name both directions; 'rhu-gate shim rederive' recompiles every row
/// under its class's flag set and byte-asserts the output against the
/// frozen bytes.
const FROZEN_PLAN: [(&str, &str, &[&str], Class); 36] = [
    ("shim-core.o", "src/shim/rhu_shim.c", &["-DMGC_HOST_ROW_AN500"], Class::CortexM7),
    ("shim-core-break-sipid.o", "src/shim/rhu_shim.c", &["-DMGC_HOST_ROW_AN500", "-DRHU_SHIM_BREAK_SIPID"], Class::CortexM7),
    ("shim-core-break-ctxpc.o", "src/shim/rhu_shim.c", &["-DMGC_HOST_ROW_AN500", "-DRHU_SHIM_BREAK_CTXPC"], Class::CortexM7),
    ("shim-core-break-tlsoff.o", "src/shim/rhu_shim.c", &["-DMGC_HOST_ROW_AN500", "-DRHU_SHIM_BREAK_TLSOFF"], Class::CortexM7),
    ("shim-core-break-tlscache.o", "src/shim/rhu_shim.c", &["-DMGC_HOST_ROW_AN500", "-DRHU_SHIM_BREAK_TLSCACHE"], Class::CortexM7),
    ("shim-sysnative.o", "src/shim/rhu_sysnative.c", &["-DMGC_HOST_ROW_AN500"], Class::CortexM7),
    ("shim-sysnative-break-fdbase.o", "src/shim/rhu_sysnative.c", &["-DMGC_HOST_ROW_AN500", "-DRHU_SHIM_BREAK_FD_BASE"], Class::CortexM7),
    ("shim-sysnative-break-store-bounds.o", "src/shim/rhu_sysnative.c", &["-DMGC_HOST_ROW_AN500", "-DRHU_SHIM_BREAK_STORE_BOUNDS"], Class::CortexM7),
    ("shim-sysnative-break-store-lifetime.o", "src/shim/rhu_sysnative.c", &["-DMGC_HOST_ROW_AN500", "-DRHU_SHIM_BREAK_STORE_LIFETIME"], Class::CortexM7),
    ("shim-sysnative-break-store-open-order.o", "src/shim/rhu_sysnative.c", &["-DMGC_HOST_ROW_AN500", "-DRHU_SHIM_BREAK_STORE_OPEN_ORDER"], Class::CortexM7),
    ("shim-sysnative-break-store-access.o", "src/shim/rhu_sysnative.c", &["-DMGC_HOST_ROW_AN500", "-DRHU_SHIM_BREAK_STORE_ACCESS"], Class::CortexM7),
    ("shim-sysnative-break-store-flock.o", "src/shim/rhu_sysnative.c", &["-DMGC_HOST_ROW_AN500", "-DRHU_SHIM_BREAK_STORE_FLOCK"], Class::CortexM7),
    ("shim-sysnative-break-store-slash.o", "src/shim/rhu_sysnative.c", &["-DMGC_HOST_ROW_AN500", "-DRHU_SHIM_BREAK_STORE_SLASH"], Class::CortexM7),
    ("shim-sysnative-break-store-times.o", "src/shim/rhu_sysnative.c", &["-DMGC_HOST_ROW_AN500", "-DRHU_SHIM_BREAK_STORE_TIMES"], Class::CortexM7),
    ("shim-sysnative-break-store-lock.o", "src/shim/rhu_sysnative.c", &["-DMGC_HOST_ROW_AN500", "-DRHU_SHIM_BREAK_STORE_LOCK"], Class::CortexM7),
    ("shim-sysnative-break-entropy-refusal.o", "src/shim/rhu_sysnative.c", &["-DMGC_HOST_ROW_AN500", "-DRHU_SHIM_BREAK_ENTROPY_REFUSAL"], Class::CortexM7),
    ("shim-sysnative-break-calendar-clock.o", "src/shim/rhu_sysnative.c", &["-DMGC_HOST_ROW_AN500", "-DRHU_SHIM_BREAK_CALENDAR_CLOCK"], Class::CortexM7),
    ("shim-libc-glue.o", "src/shim/rhu_libc_glue.c", &[], Class::CortexM7),
    ("shim-host.o", "src/shim/rhu_host.c", &["-DMGC_HOST_ROW_AN500"], Class::CortexM7),
    ("shim-host-break-dataverify.o", "src/shim/rhu_host.c", &["-DMGC_HOST_ROW_AN500", "-DRHU_SHIM_BREAK_DATAVERIFY"], Class::CortexM7),
    ("shim-stdio-sym.o", "src/shim/rhu_stdio_sym.c", &[], Class::CortexM7),
    ("shimtest-core.o", "src/shim/gate/rhu_shimtest.c", &["-DRHU_SHIMTEST_LADDER_CORE"], Class::CortexM7),
    ("shimtest-core-break-errno.o", "src/shim/gate/rhu_shimtest.c", &["-DRHU_SHIMTEST_LADDER_CORE", "-DRHU_SHIMTEST_BREAK_ERRNO"], Class::CortexM7),
    ("shimtest-core-break-canary.o", "src/shim/gate/rhu_shimtest.c", &["-DRHU_SHIMTEST_LADDER_CORE", "-DRHU_SHIMTEST_BREAK_CANARY"], Class::CortexM7),
    ("shimtest-fdmap.o", "src/shim/gate/rhu_shimtest.c", &["-DRHU_SHIMTEST_LADDER_FDMAP"], Class::CortexM7),
    ("shimtest-activation.o", "src/shim/gate/rhu_shimtest.c", &["-DRHU_SHIMTEST_LADDER_ACTIVATION"], Class::CortexM7),
    ("shimtest-tls.o", "src/shim/gate/rhu_shimtest.c", &["-DRHU_SHIMTEST_LADDER_TLS"], Class::CortexM7),
    ("shimtest-host.o", "src/shim/gate/rhu_shimtest.c", &["-DRHU_SHIMTEST_LADDER_HOST"], Class::CortexM7),
    ("shim-core-f769.o", "src/shim/rhu_shim.c", &["-DMGC_HOST_ROW_F769"], Class::CortexM7),
    ("shim-core-an505.o", "src/shim/rhu_shim.c", &["-DMGC_HOST_ROW_AN505"], Class::CortexM33),
    ("shim-sysnative-an505.o", "src/shim/rhu_sysnative.c", &["-DMGC_HOST_ROW_AN505"], Class::CortexM33),
    ("shim-libc-glue-an505.o", "src/shim/rhu_libc_glue.c", &[], Class::CortexM33),
    ("shim-host-an505.o", "src/shim/rhu_host.c", &["-DMGC_HOST_ROW_AN505"], Class::CortexM33),
    ("shim-stdio-sym-an505.o", "src/shim/rhu_stdio_sym.c", &[], Class::CortexM33),
    ("shim-pal-surface.o", "src/shim/rhu_pal_surface_net11_rc1.c", &["-DMGC_HOST_ROW_AN500"], Class::CortexM7),
    ("shim-pal-surface-an505.o", "src/shim/rhu_pal_surface_net11_rc1.c", &["-DMGC_HOST_ROW_AN505"], Class::CortexM33),
];

/// The tier's text records beside the objects and goldens: the surface
/// expectation the shim.pal-surface case reads (GENERATED from the pinned
/// pack's export tables; regenerated at a re-pin, never edited). Recorded
/// in the manifest like a golden, verified like one.
const FROZEN_RECORDS: [&str; 1] = ["pal-surface.txt"];

/// The frozen goldens: the payload stream of each positive image, class
/// bytes. Recorded in the manifest beside the objects.
const GOLDENS: [&str; 5] = [
    "shimtest-core.golden.txt",
    "shimtest-fdmap.golden.txt",
    "shimtest-activation.golden.txt",
    "shimtest-tls.golden.txt",
    "shimtest-host.golden.txt",
];

/// The core object's EXACT import closure: the kernel verbs it consumes
/// (the activation pair since the costume, the module-find door since
/// the address answer), the two arena-window link symbols, the in-image
/// header the address answer reads, the compiler builtins (the zero-fill
/// helper the compiler of record emits among them) and the 64-bit
/// division helper (the loader-seam hooks are weak references, not
/// imports, and stay outside the pin).
const IMPORTS_CORE: [&str; 35] = [
    "__aeabi_ldivmod",
    "__aeabi_memclr4",
    "__aeabi_uldivmod",
    "__rhu_arena_hi",
    "__rhu_arena_lo",
    "__rhu_image_header",
    "memcmp",
    "memset",
    "rhu_activation_register",
    "rhu_activation_send",
    "rhu_arena_alloc_at",
    "rhu_arena_headroom",
    "rhu_arena_rezero",
    "rhu_calendar_get",
    "rhu_console_write",
    "rhu_entropy",
    "rhu_entropy_secure",
    "rhu_exit",
    "rhu_futex_wait",
    "rhu_futex_wake",
    "rhu_kheap_alloc",
    "rhu_kheap_free",
    "rhu_module_find",
    "rhu_module_register",
    "rhu_module_roots",
    "rhu_now_ns",
    "rhu_sleep_ns",
    "rhu_thread_atexit",
    "rhu_thread_self",
    "rhu_thread_spawn",
    "rhu_thread_stack_bounds",
    "rhu_thread_yield",
    "rhu_tls_get",
    "strlen",
    "strncmp",
];

/// The PAL object's EXACT import closure: the kernel verbs, the glue's
/// allocator, the core's errno, environment table, diagnostics, formatter
/// core, library-load path, thread and mmap paths, the mutex recipe, the
/// string functions, the division helpers.
const IMPORTS_SYSNATIVE: [&str; 41] = [
    "__aeabi_ldivmod",
    "__aeabi_uldivmod",
    "__errno_location",
    "dlopen",
    "dlsym",
    "free",
    "getenv",
    "malloc",
    "malloc_usable_size",
    "memalign",
    "memcpy",
    "memset",
    "mmap64",
    "pthread_mutex_init",
    "pthread_mutex_lock",
    "pthread_mutex_unlock",
    "realloc",
    "rhu_arena_alloc_at",
    "rhu_console_read",
    "rhu_console_write",
    "rhu_entropy",
    "rhu_entropy_is_hardware",
    "rhu_entropy_secure",
    "rhu_exit",
    "rhu_futex_wait",
    "rhu_futex_wake",
    "rhu_now_ns",
    "rhu_shim_calendar_unix_ticks",
    "rhu_shim_create_thread",
    "rhu_shim_environ",
    "rhu_shim_exit",
    "rhu_shim_log",
    "rhu_shim_vformat",
    "rhu_sleep_ns",
    "rhu_thread_self",
    "strchr",
    "strcmp",
    "strcpy",
    "strlen",
    "strncmp",
    "strrchr",
];

/// The surface object's EXACT import closure: a refusal sets errno and
/// returns, and reaches nothing else.
const IMPORTS_PAL_SURFACE: [&str; 1] = ["__errno_location"];

/// The glue object's EXACT import closure: the kernel heap and console
/// doors, the exit door, the core's errno and diagnostics, the builtins.
const IMPORTS_GLUE: [&str; 13] = [
    "__errno_location",
    "memcpy",
    "memset",
    "rhu_console_read",
    "rhu_console_write",
    "rhu_exit",
    "rhu_futex_wait",
    "rhu_futex_wake",
    "rhu_kheap_alloc",
    "rhu_kheap_free",
    "rhu_shim_exit",
    "rhu_shim_log",
    "rhu_thread_self",
];

/// The host object's EXACT import closure: the runtime's two entry points,
/// the module-list initializer, the managed entry and the eight
/// class-library exports by the bootstrapper's names, the map's and the
/// linker's brackets it reads, the in-image header, the kernel's console,
/// exit and arena doors, the shim's initialization and stream bridge.
const IMPORTS_HOST: [&str; 36] = [
    "AppendExceptionStackFrame",
    "GetRuntimeException",
    "GetSystemArrayEEType",
    "InitializeModules",
    "OnFirstChanceException",
    "OnUnhandledException",
    "ResolveDispatch",
    "RhInitialize",
    "RhRegisterOSModule",
    "RuntimeFailFast",
    "ThreadEntryPoint",
    "__edata",
    "__fini_array_end",
    "__fini_array_start",
    "__init_array_end",
    "__init_array_start",
    "__managed__Main",
    "__preinit_array_end",
    "__preinit_array_start",
    "__rhu_image_header",
    "__rhu_managedcode_hi",
    "__rhu_managedcode_lo",
    "__rhu_unbox_hi",
    "__rhu_unbox_lo",
    "__sdata",
    "__sidata",
    "__start___modules",
    "__stop___modules",
    "rhu_arena_headroom",
    "rhu_arena_report",
    "rhu_console_write",
    "rhu_exit",
    "rhu_shim_activation_count",
    "rhu_shim_init",
    "rhu_stdio_bind_stderr",
    "rhu_stdio_line_buffer_stdout",
];

/// The stream bridge's EXACT import closure: the C library's two parsers
/// its forwarders call by the plain names (its task-record accessors are
/// weak references, not imports).
const IMPORTS_STDIO: [&str; 2] = ["vfscanf", "vsscanf"];

/// The one named group of C-library imports the every-import-resolves
/// check accepts as resolved elsewhere: the stream bridge's parsers,
/// defined by the C library at the hosted runtime's link (where the rt
/// gate asserts every import resolved) and by nothing in these images,
/// whose linker discards the unreferenced forwarders. A C-library name
/// outside this group is a residue, as before.
const LIBC_IMPORTS: [&str; 2] = ["vfscanf", "vsscanf"];

/// Symbols no object and no staticlib member carries, because they are
/// the LINK's own. Two sources, and the difference matters when one of
/// them is wrong: the board map declares the first group deliberately,
/// while the second is synthesized by the linker for any image that
/// mentions it -- which is why the host checks where the constructor
/// array actually landed rather than trusting that it exists.
const MAP_SYMBOLS: [&str; 19] = [
    "__rhu_arena_lo",
    "__rhu_arena_hi",
    "__rhu_tdata_lo",
    "__rhu_tdata_hi",
    "__rhu_managedcode_lo",
    "__rhu_managedcode_hi",
    "__rhu_unbox_lo",
    "__rhu_unbox_hi",
    "__start___modules",
    "__stop___modules",
    "__sdata",
    "__edata",
    "__sidata",
    "__preinit_array_start",
    "__preinit_array_end",
    "__init_array_start",
    "__init_array_end",
    "__fini_array_start",
    "__fini_array_end",
];

/// Import families that must never appear in a shim object: C++ runtime
/// and unwinding, soft-float helpers, emulated thread-local storage.
const FORBIDDEN_IMPORT_FAMILIES: [&str; 6] = ["_Unwind", "__cxa", "_ZSt", "__aeabi_f", "__aeabi_d", "__emutls"];

/// The diagnostic lines the core run must carry (fixed strings).
const PINNED_CORE_LINES: [&str; 11] = [
    "[shim] init: per-thread block registered as module 1 (48 bytes)",
    "[shim] pthread_create: stack 96 KB clamped to 24 KB (the row's clamp)",
    "[shim] mmap: MAP_FIXED re-zero in place (decommit; further requests counted silently)",
    "[shim] frozen-object reserve 4096 KB -> pool 64 KB (commit guard armed)",
    "[shim] frozen-object commit refused beyond the pool (managed allocation failure, never a wild write)",
    "[shim] entropy: cryptographic draws through the kernel's secure door; hardware source: no",
    "[shim] entropy: the secure door refused a cryptographic draw (an invalid argument); the draw fails and nothing is served (further refusals counted silently)",
    "[shim] calendar: the kernel's calendar serves the wall clock; source: unset (the epoch plus the elapsed clock)",
    "[shim] fork: not available (ENOSYS)",
    "[shim] syscall 999: not available (ENOSYS)",
    "[shim] open(/nonexistent): no such path (ENOENT)",
];

const CORE_ALL_GOOD: &str = "SHIMTEST: ALL GOOD (glibc personality + PAL on the kernel doors)";
const FDMAP_ALL_GOOD: &str = "SHIMTEST-FDMAP: ALL GOOD (explicit descriptor bands; the console never answers for a file)";
const CANARY_LINE: &str = "[shim] free: corrupt block header (canary)";

/// The diagnostic lines the activation run must carry (fixed strings):
/// the per-thread block, the two refusals before any registration, the
/// arming line, the two recorded fault-signal dispositions, the ignored
/// stack-pointer edit, and the signal rows' honesty line.
const PINNED_ACT_LINES: [&str; 8] = [
    "[shim] init: per-thread block registered as module 1 (48 bytes)",
    "[shim] pthread_kill(2, 34): no activation handler recorded (EINVAL)",
    "[shim] pthread_kill(2, 10): only the activation signal is deliverable on this platform (EINVAL)",
    "[shim] activation: signal 34 handler recorded; the kernel's activation delivery is armed",
    "[shim] sigaction(8): handler recorded; this platform delivers no signal of that number",
    "[shim] sigaction(11): handler recorded; this platform delivers no signal of that number",
    "[shim] activation: a stack-pointer edit in the signal context is ignored (frame geometry never moves)",
    "[shim] signals: no signal source on this platform; registrations are recorded, none can arrive",
];
const ACT_ALL_GOOD: &str = "SHIMTEST-ACTIVATION: ALL GOOD (the activation costume on the kernel's delivery; the signal rows recorded, none deliverable)";
const ACT_FAIL_SIPID: &str = "FAIL shim-act-deliver (the activation named a foreign process)";
const ACT_FAIL_CTXPC: &str = "FAIL shim-act-deliver (the interrupted pc is outside the worker's window)";

/// Symbols the activation image must carry beyond the common contract.
const ACT_SYMBOLS: [&str; 5] = ["sigaction", "pthread_kill", "__libc_current_sigrtmin", "SystemNative_InitializeTerminalAndSignalHandling", "__errno_location"];

const TLS_ALL_GOOD: &str = "SHIMTEST-TLS: ALL GOOD (the image's thread-local template on module 0; a fresh copy per thread, isolated)";
const TLS_FAIL_OFF: &str = "FAIL shim-tls-initial (the zero-fill word is not zero)";
const TLS_FAIL_CACHE: &str = "FAIL shim-tls-fresh (a worker did not start from the template)";

/// Symbols the thread-local image must carry beyond the common contract.
const TLS_SYMBOLS: [&str; 4] = ["__tls_get_addr", "__rhu_tdata_lo", "__rhu_tdata_hi", "__errno_location"];

/// The bytes the thread-local image's template must span: the tenant's
/// one initialized word and one zero-fill word. Every other image links
/// no thread-local input at all, so its bracket pair is empty.
const TLS_TEMPLATE_SPAN: u64 = 8;
const NO_TEMPLATE: u64 = 0;

/// The host's markers, in the ONE order the contract allows. The case
/// asserts this sequence, not merely that each line is somewhere in the
/// capture: a host that bound its streams after registering its module
/// would satisfy presence and violate the contract.
const HOST_MARKERS: [&str; 10] = [
    "[host] stdio: standard error bound to the C library's stream",
    "[host] stdio: standard output line buffered",
    "[host] data: initialized region verified against its load image",
    "[host] ctors: none (this image carries no static constructor)",
    "[host] runtime: initialized",
    "[host] runtime: managed code 0 bytes, unboxing stubs 0 bytes",
    "[host] runtime: module registered, class-library slots: 12",
    "[host] runtime: modules initialized: 0",
    "[host] argv: 1",
    "[host] entering managed entry",
];

/// The markers that must FOLLOW managed entry: the audit is printed
/// before the exit line, so a run that ends normally still reports what
/// it left behind.
const HOST_TAIL_MARKERS: [&str; 2] = ["[host] arena headroom at exit: ", "[host] exit: 0"];

const HOST_ALL_GOOD: &str = "SHIMTEST-HOST: ALL GOOD (the initialization contract in order; the class-library table as the pin declares it)";
const HOST_FAIL_VERIFY: &str = "[host] FAIL initialized data does not match the image it was loaded from";

/// Symbols the host image must carry beyond the common contract.
const HOST_SYMBOLS: [&str; 6] = ["rhu_host_main", "rhu_payload_entry", "rhu_payload_abi", "RhInitialize", "RhRegisterOSModule", "InitializeModules"];

/// The C-library members the retarget must keep OUT of a linked image:
/// the allocator member's own imports -- the heap break it needs to start
/// and the lock pair it takes around every call. Their arrival would mean
/// two heaps in one image. (The allocator's own entry points are spelled
/// as the reentrant names this image defines strongly, so they are not
/// what an arrival would look like.)
const RETARGET_FORBIDDEN: [&str; 3] = ["_sbrk_r", "__malloc_lock", "__malloc_unlock"];

/// The names this image's glue defines and the library also carries a
/// member for: the allocation family and its reentrant spellings, the
/// heap break, the descriptor-level calls and the process exit. The
/// library's version of any of them arriving would mean two heaps or two
/// consoles; the glue's strong definitions keep the members out, and the
/// retarget control asserts that none arrived.
const RETARGET_LAYER: [&str; 19] = [
    "_calloc_r", "_close", "_exit", "_free_r", "_fstat", "_getpid", "_isatty", "_kill", "_lseek",
    "_malloc_r", "_memalign_r", "_read", "_realloc_r", "_sbrk", "_write", "calloc", "free", "malloc", "realloc",
];

/// The compile-tier probe for the generated C-library layout header: the
/// kit's own stream and task-record headers beside the generated one, and
/// one static assertion per generated value against the kit's macro,
/// offset or size. Compiled under the nano configuration the image's link
/// uses. The genuine header compiles; a header with one value moved is
/// refused at the assertion that names it.
const NEWLIB_PROBE_SOURCE: &[u8] = br#"#include <stddef.h>
#include <stdio.h>
#include <sys/reent.h>
#include "rhu_newlib_arm32.h"
_Static_assert(__NEWLIB__ == RHU_NEWLIB_VERSION_MAJOR, "the major version ties the kit");
_Static_assert(__NEWLIB_MINOR__ == RHU_NEWLIB_VERSION_MINOR, "the minor version ties the kit");
#ifdef _REENT_SMALL
_Static_assert(RHU_NEWLIB_REENT_SMALL == 1, "the small-reent switch ties the kit");
#else
_Static_assert(RHU_NEWLIB_REENT_SMALL == 0, "the small-reent switch ties the kit");
#endif
#ifdef __DYNAMIC_REENT__
_Static_assert(RHU_NEWLIB_DYNAMIC_REENT == 1, "the dynamic-reent switch ties the kit");
#else
_Static_assert(RHU_NEWLIB_DYNAMIC_REENT == 0, "the dynamic-reent switch ties the kit");
#endif
_Static_assert(__SLBF == RHU_NEWLIB_STREAM_LINEBUF, "the line-buffered flag ties the kit");
_Static_assert(__SNBF == RHU_NEWLIB_STREAM_NOBUF, "the unbuffered flag ties the kit");
_Static_assert(__SRD == RHU_NEWLIB_STREAM_READ, "the read flag ties the kit");
_Static_assert(__SSTR == RHU_NEWLIB_STREAM_STRING, "the string-stream flag ties the kit");
_Static_assert(offsetof(struct __sFILE, _flags) == RHU_NEWLIB_STREAM_FLAGS_OFFSET, "the flag word's offset ties the kit");
_Static_assert(sizeof(((struct __sFILE *)0)->_flags) == RHU_NEWLIB_STREAM_FLAGS_SIZE, "the flag word's width ties the kit");
_Static_assert(offsetof(struct __sFILE, _file) == RHU_NEWLIB_STREAM_FILE_OFFSET, "the descriptor's offset ties the kit");
_Static_assert(offsetof(struct _reent, _errno) == RHU_NEWLIB_TASK_ERRNO_OFFSET, "the errno offset ties the kit");
_Static_assert(offsetof(struct _reent, _stdin) == RHU_NEWLIB_TASK_STDIN_OFFSET, "the standard input offset ties the kit");
_Static_assert(offsetof(struct _reent, _stdout) == RHU_NEWLIB_TASK_STDOUT_OFFSET, "the standard output offset ties the kit");
_Static_assert(offsetof(struct _reent, _stderr) == RHU_NEWLIB_TASK_STDERR_OFFSET, "the standard error offset ties the kit");
int rhu_newlib_header_probe;
"#;

/// The line of the generated header the control moves for its doctored
/// copy, and what it moves it to: the line-buffered flag set to the
/// string-stream value, exactly the mistake the header exists to refuse.
const NEWLIB_DOCTOR_FROM: &str = "#define RHU_NEWLIB_STREAM_LINEBUF 0x0001";
const NEWLIB_DOCTOR_TO: &str = "#define RHU_NEWLIB_STREAM_LINEBUF 0x0200";

/// The class-library order control: a doctored copy of the generated
/// runtime-entry header whose unhandled-exception slot reads 6 where the
/// header says 7 -- the index the slot would have in a table laid out
/// without the dispatch resolver, one slot shifted -- must be refused.
const RT_ENTRY_DOCTOR_FROM: &str = "#define RHU_RT_CLASSLIB_SLOT_OnUnhandledException 7";
const RT_ENTRY_DOCTOR_TO: &str = "#define RHU_RT_CLASSLIB_SLOT_OnUnhandledException 6";

/// A translation unit that genuinely consumes the toolchain's C library,
/// so the retarget case runs against a link that really reaches it.
const PROBE_SOURCE: &[u8] = b"#include <stdio.h>\nvoid rhu_newlib_consumer(void);\nvoid rhu_newlib_consumer(void) { printf(\"%d\\n\", 1); }\n";

/// Patterns a positive run must not carry.
const FORBIDDEN_PATTERNS: [&str; 6] = ["FAIL ", "KERNEL PANIC", "EXCEPTION:", "REFUSED", "abort()", "unsupported conversion"];

/// Listed, never counted and never vacuously green. Empty since the
/// thread-local template check armed; the mechanism stays for whatever
/// the gate grows next.
const PENDING: [&str; 0] = [];

/// The floating-point instruction census of the frozen tier, pinned per
/// object (the FPU-free costume hardening). The activation
/// costume's object carries none: a floating-point instruction on the
/// activation path is what armed the lazy floating-point state the kernel
/// now completes, and the compiler of record under
/// -mno-implicit-float emits none the source does not ask for. The tier's
/// only floating-point sites are the PAL's CPU-utilization function, which
/// returns a double by its own signature, and the core tenant's check of
/// it. The mps2-an505 row's Cortex-M33 objects carry the same sites and
/// no instruction outside the Cortex-M7 decoder's reach, so one decoder
/// censuses the whole tier. A moved count means the source, a flag or the
/// compiler changed.
const FP_CENSUS: [(&str, u32); 36] = [
    ("shim-core.o", 0),
    ("shim-core-break-sipid.o", 0),
    ("shim-core-break-ctxpc.o", 0),
    ("shim-core-break-tlsoff.o", 0),
    ("shim-core-break-tlscache.o", 0),
    ("shim-sysnative.o", 1),
    ("shim-sysnative-break-fdbase.o", 1),
    ("shim-sysnative-break-store-bounds.o", 1),
    ("shim-sysnative-break-store-lifetime.o", 1),
    ("shim-sysnative-break-store-open-order.o", 1),
    ("shim-sysnative-break-store-access.o", 1),
    ("shim-sysnative-break-store-flock.o", 1),
    ("shim-sysnative-break-store-slash.o", 1),
    ("shim-sysnative-break-store-times.o", 1),
    ("shim-sysnative-break-store-lock.o", 1),
    ("shim-sysnative-break-entropy-refusal.o", 1),
    ("shim-sysnative-break-calendar-clock.o", 1),
    ("shim-libc-glue.o", 0),
    ("shim-host.o", 0),
    ("shim-host-break-dataverify.o", 0),
    ("shim-stdio-sym.o", 0),
    ("shimtest-core.o", 2),
    ("shimtest-core-break-errno.o", 2),
    ("shimtest-core-break-canary.o", 2),
    ("shimtest-fdmap.o", 0),
    ("shimtest-activation.o", 0),
    ("shimtest-tls.o", 0),
    ("shimtest-host.o", 0),
    ("shim-core-f769.o", 0),
    ("shim-core-an505.o", 0),
    ("shim-sysnative-an505.o", 1),
    ("shim-libc-glue-an505.o", 0),
    ("shim-host-an505.o", 0),
    ("shim-stdio-sym-an505.o", 0),
    ("shim-pal-surface.o", 0),
    ("shim-pal-surface-an505.o", 0),
];

/// Count the floating-point instructions in a decoded listing: every line whose
/// mnemonic (the first token after the first tab) begins with `v` followed
/// by a letter -- the VFP mnemonics on this profile all do (vldr, vstr,
/// vmov, vpush, vpop, vcmp, vmrs, vcvt, vmul, ...) and no other Thumb-2
/// mnemonic does. The decoder must be the Cortex-M7's: without it the
/// double-precision forms print as unknown and the count is blind.
pub(crate) fn fp_instruction_count(text: &str) -> u32 {
    let mut n = 0u32;
    for line in text.lines() {
        let Some((_, rest)) = line.split_once('\t') else {
            continue;
        };
        let mnemonic = rest.trim_start().split(|c: char| c == '\t' || c == ' ').next().unwrap_or("");
        let mut chars = mnemonic.chars();
        if chars.next() == Some('v') && chars.next().is_some_and(|c| c.is_ascii_alphabetic()) {
            n += 1;
        }
    }
    n
}

/// The census against its pin: the first object whose count differs, named.
pub(crate) fn fp_census_fault(got: &[(String, u32)], want: &[(&str, u32)]) -> Option<String> {
    for (name, w) in want {
        match got.iter().find(|(n, _)| n == name) {
            Some((_, g)) if g == w => {}
            Some((_, g)) => return Some(format!("{name}: {g} floating-point instruction(s), pinned {w}")),
            None => return Some(format!("{name}: not censused")),
        }
    }
    None
}

/// The hosting-API contract generation a frozen object STATES: the word
/// of its `rhu_payload_abi` symbol read off the object's own bytes, with
/// the word's file offset; None for an object that carries no such symbol.
pub(crate) fn stated_generation(obj: &[u8]) -> Option<(u32, usize)> {
    symbol_word(obj, "rhu_payload_abi")
}

/// The rederive leg's diagnosis before a byte comparison: the line naming
/// a frozen object's stated hosting-API contract generation against the
/// header's when they differ; None when the object states none, or the
/// header's own.
pub(crate) fn generation_fault(name: &str, frozen: &[u8], header_abi: u32) -> Option<String> {
    let (stated, _) = stated_generation(frozen)?;
    if stated == header_abi {
        return None;
    }
    Some(format!(
        "{name}: the frozen object states hosting-API contract generation {stated}; include/rhu.h is at {header_abi}: the tier was frozen against another generation and needs a re-freeze"
    ))
}

const INTERNAL_CASES: [&str; 8] = [
    "shim.foreign-errno-refused -- compile-tier; runs under 'rhu-gate shim rederive' with the compiler of record",
    "shim.newlib-header-ties-kit -- compile-tier; runs under 'rhu-gate shim rederive' with the compiler of record: the generated C-library layout header against the kit's own headers",
    "shim.classlib-order-ties-pin -- compile-tier; runs under 'rhu-gate shim rederive' with the compiler of record: the host's copy tying the frozen host object apart from the producer stamp, then the same compile against a copy of the generated runtime-entry header with one class-library slot shifted, refused at the assertion that names the slot",
    "shim.newlib-retarget -- link-tier; runs under 'rhu-gate shim rederive' when the kit's C library is resolved",
    "shim.f769-shared-ties-an500 -- compile-tier; runs under 'rhu-gate shim rederive' with the compiler of record: the sysnative and host objects under the F769 selector tie the frozen an500 objects apart from the producer stamp, the F769 core differs from the an500 core",
    "shim.fp-census-instrument -- compile-tier; runs under 'rhu-gate shim rederive': a compiled double multiply-add must be counted by the census the shim.fp-census case runs over the frozen tier",
    "shim.pal-surface-survey -- survey-tier; runs under 'rhu-gate shim rederive' when the pinned runtime pack's native directory is resolved (RHU_NET_PACK or tools/toolchains/netpack.local): the pack's five class-library archives re-read for their sha256s and exported family names against the frozen expectation goldens/shim/pal-surface.txt",
    "shim.an505-row-ties-an500 -- compile-tier; runs under 'rhu-gate shim rederive' with the compiler of record: the core, sysnative and host sources under the an505 selector with the Cortex-M7 flag set tie the frozen an500 objects apart from the producer stamp, so the class flag set is the one variable behind the row's Cortex-M33 objects",
];

/// The module-root template brackets, read from a linked artifact: the
/// pair must parse and must span exactly the bytes this image's inputs
/// contribute (zero for every image that links no thread-local input).
/// Returns the complaint, or None when the pair is right.
fn template_span_fault(lo: Option<u64>, hi: Option<u64>, want: u64) -> Option<String> {
    let (Some(lo), Some(hi)) = (lo, hi) else {
        return Some("the __rhu_tdata bracket pair is not value-parseable from the artifact".to_string());
    };
    if hi < lo {
        return Some(format!("the __rhu_tdata bracket pair is inverted ({lo:#x} > {hi:#x})"));
    }
    if hi - lo != want {
        return Some(format!(
            "the __rhu_tdata bracket pair spans {} bytes, wanted {want} ({lo:#x} .. {hi:#x})",
            hi - lo
        ));
    }
    None
}

/// The first marker that is absent from `raw`, or out of order relative
/// to the ones before it. Returns None when every marker appears exactly
/// in the order given. Presence is not order: a contract whose steps
/// depend on each other is only proven by the sequence.
pub(crate) fn markers_out_of_order(raw: &[u8], markers: &[&str]) -> Option<String> {
    let mut from = 0usize;
    for m in markers {
        match find_fixed_from(raw, m, from) {
            Some(at) => from = at + m.len(),
            None => {
                return Some(if contains_fixed(raw, m) {
                    format!("marker '{m}' appears, but before a marker that must precede it")
                } else {
                    format!("marker '{m}' is absent")
                })
            }
        }
    }
    None
}

/// The offset of `needle` in `raw` at or after `from`.
fn find_fixed_from(raw: &[u8], needle: &str, from: usize) -> Option<usize> {
    let n = needle.as_bytes();
    if n.is_empty() || from > raw.len() {
        return None;
    }
    raw[from..]
        .windows(n.len())
        .position(|w| w == n)
        .map(|p| p + from)
}

/// The directory holding the toolchain's own C library, resolved the way
/// the image-tier compiler already is: RHU_NEWLIB_M7, else the first line of
/// an untracked machine-local pointer file, else absent. Absent is not a
/// failure -- the retarget case runs on the maintainer's leg and the main gate never needs
/// it -- so this returns an Option and the caller declares the case.
///
/// The pointer file names a directory in the form the LINKER can open,
/// which on a desk whose compiler lives behind a translation layer is not
/// the form the compiler prints; that is why it is a recorded
/// machine-local fact and not a derivation.
pub(crate) fn resolve_newlib_dir(repo: &Path) -> Option<String> {
    if let Ok(v) = std::env::var("RHU_NEWLIB_M7") {
        if !v.trim().is_empty() {
            return Some(v.trim().to_string());
        }
    }
    let local = repo.join("tools").join("toolchains").join("newlib-m7.local");
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

/// The pinned runtime pack's native directory (runtimes/linux-arm/native),
/// the survey control's input: RHU_NET_PACK, else the first line of the
/// untracked pointer tools/toolchains/netpack.local; None when neither
/// names it (the control is then declared, not run).
pub(crate) fn resolve_netpack_dir(repo: &Path) -> Option<String> {
    if let Ok(v) = std::env::var("RHU_NET_PACK") {
        if !v.trim().is_empty() {
            return Some(v.trim().to_string());
        }
    }
    let local = repo.join("tools").join("toolchains").join("netpack.local");
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

/// The five P/Invoke-facing families of the runtime pack's class-library
/// native libraries: a defined name belongs to the surface when it carries
/// one of these prefixes.
const SURFACE_FAMILIES: [&str; 5] = ["SystemNative_", "GlobalizationNative_", "CompressionNative_", "NetSecurityNative_", "CryptoNative_"];

/// The frozen surface expectation, parsed: the pin, the tag, the archives
/// (name, sha256, family count) and the sorted family names.
struct SurfaceExpectation {
    pin: String,
    tag: String,
    archives: Vec<(String, String, u32)>,
    names: Vec<String>,
}

/// goldens/shim/pal-surface.txt, or the first line of no known shape.
fn parse_surface_expectation(text: &str) -> Result<SurfaceExpectation, String> {
    let mut pin = None;
    let mut tag = None;
    let mut archives = Vec::new();
    let mut names: Vec<String> = Vec::new();
    for line in text.lines() {
        let l = line.trim();
        if l.is_empty() || l.starts_with('#') || l.starts_with("commit: ") || l.starts_with("family: ") || l.starts_with("outside: ") {
            continue;
        }
        if let Some(v) = l.strip_prefix("pin: ") {
            pin = Some(v.trim().to_string());
            continue;
        }
        if let Some(v) = l.strip_prefix("tag: ") {
            tag = Some(v.trim().to_string());
            continue;
        }
        if let Some(v) = l.strip_prefix("archive: ") {
            let p: Vec<&str> = v.split_whitespace().collect();
            let count = if p.len() == 3 { p[2].parse::<u32>().ok() } else { None };
            let Some(count) = count else {
                return Err(format!("an archive line of no known shape: {l}"));
            };
            archives.push((p[0].to_string(), p[1].to_string(), count));
            continue;
        }
        let p: Vec<&str> = l.split_whitespace().collect();
        if p.len() == 3 && SURFACE_FAMILIES.contains(&p[1]) && p[0].starts_with(p[1]) && ["defined", "served", "refused"].contains(&p[2]) {
            names.push(p[0].to_string());
            continue;
        }
        return Err(format!("an expectation line of no known shape: {l}"));
    }
    let pin = pin.ok_or_else(|| "the expectation names no pin".to_string())?;
    let tag = tag.ok_or_else(|| "the expectation names no tag".to_string())?;
    if archives.len() != SURFACE_FAMILIES.len() {
        return Err(format!("the expectation names {} archives, not {}", archives.len(), SURFACE_FAMILIES.len()));
    }
    if names.is_empty() {
        return Err("the expectation names no surface name".to_string());
    }
    names.sort();
    if names.windows(2).any(|w| w[0] == w[1]) {
        return Err("the expectation names a surface name twice".to_string());
    }
    Ok(SurfaceExpectation { pin, tag, archives, names })
}

/// The shim's defined names against the expectation's sorted set: the
/// first expected name the shim does not define, else the first defined
/// family name the pack's tables do not export; None when the family
/// names are exactly the expectation's (names outside the families are
/// the shim's own and stay out of the comparison).
fn surface_fault(defined: &[String], expected: &[String]) -> Option<String> {
    let mut have: Vec<&String> = defined.iter().filter(|n| SURFACE_FAMILIES.iter().any(|f| n.starts_with(f))).collect();
    have.sort();
    have.dedup();
    for e in expected {
        if have.binary_search(&e).is_err() {
            return Some(format!(
                "the pinned pack exports {e} and the shim does not define it: regenerate the surface at the pin (tools/extract-pal/gen_pal_surface.py) and answer it"
            ));
        }
    }
    for h in have {
        if expected.binary_search(h).is_err() {
            return Some(format!("the shim defines {h}, a family name the pinned pack's tables do not export: the surface is not the pack's"));
        }
    }
    None
}

/// The EXTERNAL definitions of an nm dump by the generator's rule (the
/// text, data, read-only, zero-fill and weak definitions: T, D, R, B, W).
/// A local symbol -- a lowercase type; a function's static variable, which
/// the compiler names function.variable -- is not an export and stays out
/// (the survey's first run counted one: the terminal initializer's
/// `initialized` flag in the pack's libSystem.Native.a).
fn external_names(dump: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in dump.lines() {
        let mut it = line.split_whitespace();
        if let (Some(_), Some(t), Some(name)) = (it.next(), it.next(), it.next()) {
            if ["T", "D", "R", "B", "W"].contains(&t) && it.next().is_none() {
                out.push(name.to_string());
            }
        }
    }
    out
}

/// The pin tie: the expectation was generated at the SDK's pin and the
/// surface header states the same tag, or the survey is stale.
fn surface_pin_fault(expectation_pin: &str, sdk_pin: &str, header_tag: &str) -> Option<String> {
    if expectation_pin != sdk_pin {
        return Some(format!(
            "the surface expectation was generated at pin {expectation_pin}; the SDK pins {sdk_pin} (RhuRuntimeVersion): regenerate the survey at the new pin and answer what it adds"
        ));
    }
    let want = format!("v{expectation_pin}");
    if header_tag != want {
        return Some(format!("the generated surface header states tag {header_tag}, the expectation's pin wants {want}: the header and the expectation were not generated together"));
    }
    None
}

/// The SDK's runtime pin, read off Sdk.props (the one RhuRuntimeVersion default).
fn read_sdk_runtime_pin(repo: &Path) -> Result<String, String> {
    let p = repo.join("src").join("sdk").join("Lamella.Rhu.Sdk").join("Sdk").join("Sdk.props");
    let text = std::fs::read_to_string(&p).map_err(|e| format!("src/sdk/Lamella.Rhu.Sdk/Sdk/Sdk.props unreadable: {e}"))?;
    for line in text.lines() {
        let l = line.trim();
        if l.starts_with("<RhuRuntimeVersion") {
            if let (Some(a), Some(b)) = (l.find('>'), l.find("</RhuRuntimeVersion>")) {
                if a < b {
                    return Ok(l[a + 1..b].trim().to_string());
                }
            }
        }
    }
    Err("Sdk.props carries no RhuRuntimeVersion default".to_string())
}

/// The tag the generated surface header states in its provenance comment.
fn read_surface_header_tag(repo: &Path) -> Result<String, String> {
    let p = repo.join("src").join("shim").join("rhu_pal_surface_net11_rc1.h");
    let text = std::fs::read_to_string(&p).map_err(|e| format!("src/shim/rhu_pal_surface_net11_rc1.h unreadable: {e}"))?;
    let Some(i) = text.find("at tag ") else {
        return Err("the surface header states no tag".to_string());
    };
    let rest = &text[i + 7..];
    let end = rest.find(|c: char| c.is_whitespace()).unwrap_or(rest.len());
    Ok(rest[..end].to_string())
}

/// The six link objects of a row: the five the images link and the surface object.
const ROW_AN500_LINK: [&str; 6] = [
    "goldens/shim/shim-core.o",
    "goldens/shim/shim-sysnative.o",
    "goldens/shim/shim-libc-glue.o",
    "goldens/shim/shim-stdio-sym.o",
    "goldens/shim/shim-host.o",
    "goldens/shim/shim-pal-surface.o",
];
const ROW_AN505_LINK: [&str; 6] = [
    "goldens/shim/shim-core-an505.o",
    "goldens/shim/shim-sysnative-an505.o",
    "goldens/shim/shim-libc-glue-an505.o",
    "goldens/shim/shim-stdio-sym-an505.o",
    "goldens/shim/shim-host-an505.o",
    "goldens/shim/shim-pal-surface-an505.o",
];

/// True when the sorted import list equals the pinned set exactly.
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

/// Strong (non-weak, non-local) definitions in an nm dump: the names of
/// lines whose type letter is T, D, B or R.
fn strong_defined(dump: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in dump.lines() {
        let mut it = line.split_whitespace();
        if let (Some(_), Some(t), Some(name)) = (it.next(), it.next(), it.next()) {
            if matches!(t, "T" | "D" | "B" | "R") && it.next().is_none() {
                out.push(name.to_string());
            }
        }
    }
    out
}

/// Every defined name in an nm dump, whatever its type letter (strong
/// and weak alike; archive member headers and undefined lines skipped).
fn defined_names(dump: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in dump.lines() {
        let mut it = line.split_whitespace();
        if let (Some(_), Some(t), Some(name)) = (it.next(), it.next(), it.next()) {
            if t != "U" && t != "w" && it.next().is_none() {
                out.push(name.to_string());
            }
        }
    }
    out
}

/// Imports of the linked objects that neither the objects themselves, the
/// kernel staticlib, the board map nor the named C-library group define. The linker's section garbage collection
/// never reports a reference from a function nothing references, so a
/// missing definition would otherwise surface only in the image that
/// first calls it.
fn unresolved_imports(imports: &[Vec<String>], defined: &[Vec<String>], lib_defined: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for list in imports {
        for name in list {
            let here = defined.iter().any(|d| d.contains(name));
            let by_map = MAP_SYMBOLS.contains(&name.as_str()) || LIBC_IMPORTS.contains(&name.as_str());
            if !here && !by_map && !lib_defined.contains(name) && !out.contains(name) {
                out.push(name.clone());
            }
        }
    }
    out.sort();
    out
}

/// The definitions the C library adds to an image that reaches it only
/// through the stream bridge (no consumer forced in): every one the
/// library's own, none of the allocator's, the heap break's, the lock
/// pair's or the descriptor layer's -- the reentrancy record and the
/// stream objects the bridge's weak references bind are what the design
/// expects -- and nothing of the image's own gone.
fn retarget_added_fault(added: &[String], removed: &[String], lib_defined: &[String]) -> Option<String> {
    if let Some(r) = removed.first() {
        return Some(format!("the library's arrival removed a definition of this image's own: '{r}'"));
    }
    if let Some(a) = added.iter().find(|a| !lib_defined.contains(a)) {
        return Some(format!("the library's arrival added '{a}', which the library does not define"));
    }
    if let Some(a) = added.iter().find(|a| RETARGET_FORBIDDEN.contains(&a.as_str()) || RETARGET_LAYER.contains(&a.as_str())) {
        return Some(format!("the library's own '{a}' arrived: two heaps or two consoles would answer"));
    }
    None
}

/// Names defined strongly in more than one dump (sorted, deduplicated).
fn duplicate_definitions(dumps: &[&str]) -> Vec<String> {
    let mut seen: Vec<String> = Vec::new();
    let mut dups: Vec<String> = Vec::new();
    for d in dumps {
        for n in strong_defined(d) {
            if seen.contains(&n) {
                if !dups.contains(&n) {
                    dups.push(n);
                }
            } else {
                seen.push(n);
            }
        }
    }
    dups.sort();
    dups
}

/// The frozen record set against the plan and the goldens, both
/// directions, plus the on-disk sweep: every plan object and golden has a
/// record; every record names a plan object or a golden under
/// goldens/shim/; every `.o` on disk is in the plan.
fn frozen_consistency(records: &[(String, String)], on_disk: &[String]) -> Result<(), String> {
    let expected = FROZEN_PLAN.len() + GOLDENS.len() + FROZEN_RECORDS.len();
    if records.len() != expected {
        return Err(format!(
            "manifest parsed to {} records, the frozen plan and goldens hold {} -- a record set that drifted",
            records.len(),
            expected
        ));
    }
    for (name, _, _, _) in FROZEN_PLAN {
        if !records.iter().any(|(p, _)| p == &format!("goldens/shim/{name}")) {
            return Err(format!("plan object {name} has no manifest record"));
        }
    }
    for g in GOLDENS {
        if !records.iter().any(|(p, _)| p == &format!("goldens/shim/{g}")) {
            return Err(format!("golden {g} has no manifest record"));
        }
    }
    for r in FROZEN_RECORDS {
        if !records.iter().any(|(p, _)| p == &format!("goldens/shim/{r}")) {
            return Err(format!("record {r} has no manifest record"));
        }
    }
    for (path, _) in records {
        let Some(name) = path.strip_prefix("goldens/shim/") else {
            return Err(format!("manifest record '{path}' is outside goldens/shim/"));
        };
        if !FROZEN_PLAN.iter().any(|(n, _, _, _)| n == &name) && !GOLDENS.contains(&name) && !FROZEN_RECORDS.contains(&name) {
            return Err(format!("manifest record '{path}' has no frozen-plan or golden entry"));
        }
    }
    for name in on_disk {
        if !FROZEN_PLAN.iter().any(|(n, _, _, _)| *n == name.as_str()) {
            return Err(format!("goldens/shim/{name} has NO manifest record (a file without a record)"));
        }
    }
    Ok(())
}

struct Shim {
    repo: PathBuf,
    out: PathBuf,
    results: Results,
    gate_id: &'static str,
    display: &'static str,
    /// The verified manifest records (repo-relative path, sha256).
    records: Vec<(String, String)>,
}

impl Shim {
    fn fail(&mut self, case: &str, msg: &str) -> ! {
        self.results.emit(&format!(
            "{{\"schema\":1,\"type\":\"case\",\"gate\":\"{}\",\"case\":\"{case}\",\"verdict\":\"fail\",\"exit\":null,\"exit_want\":null,\"control\":\"positive\",\"image\":null}}",
            self.gate_id
        ));
        self.results.tally("fail", "positive");
        self.results.emit_summary(self.gate_id, "fail", 1);
        let _ = validate_results(&self.repo, self.results.path());
        println!("{}: FAIL -- {case}: {msg}", self.display);
        exit(1);
    }

    #[allow(clippy::too_many_arguments)]
    fn fail_run(&mut self, case: &str, exit_got: i32, exit_want: i32, control: &str, broke: &str, sha: &str, path_rel: &str, msg: &str) -> ! {
        let broke_frag = if control == "negative" { format!(",\"broke\":\"{broke}\"") } else { String::new() };
        self.results.emit(&format!(
            "{{\"schema\":1,\"type\":\"case\",\"gate\":\"{}\",\"case\":\"{case}\",\"verdict\":\"fail\",\"exit\":{exit_got},\"exit_want\":{exit_want},\"control\":\"{control}\",\"image\":{{\"path\":\"{path_rel}\",\"sha256\":\"{sha}\"}}{broke_frag}}}",
            self.gate_id
        ));
        self.results.tally("fail", control);
        self.results.emit_summary(self.gate_id, "fail", 1);
        let _ = validate_results(&self.repo, self.results.path());
        println!("{}: FAIL -- {case}: {msg}", self.display);
        exit(1);
    }

    fn golden_sha(&self, name: &str) -> String {
        let path = format!("goldens/shim/{name}");
        self.records
            .iter()
            .find(|(p, _)| p == &path)
            .map(|(_, s)| s.clone())
            .unwrap_or_default()
    }

    fn verify_frozen(&mut self) {
        println!("== frozen inputs: sha256 vs goldens/shim/MANIFEST.md ==");
        let dir = self.repo.join("goldens").join("shim");
        let mpath = dir.join("MANIFEST.md");
        let msha = match sha256_file(&mpath) {
            Ok(s) => s,
            Err(e) => self.fail(
                "frozen-inputs",
                &format!("goldens/shim/MANIFEST.md unreadable: {e} (the frozen tier's record; a drop copy without goldens/shim runs the chain with this tier DECLARED, never this gate)"),
            ),
        };
        if msha != SHA_FROZEN_MANIFEST {
            self.fail(
                "frozen-inputs",
                &format!("goldens/shim/MANIFEST.md sha256 {msha} != pinned {SHA_FROZEN_MANIFEST} -- the frozen record moved without a registered re-freeze"),
            );
        }
        let text = match std::fs::read_to_string(&mpath) {
            Ok(t) => t,
            Err(e) => self.fail("frozen-inputs", &format!("goldens/shim/MANIFEST.md unreadable: {e}")),
        };
        let records = parse_vendored_manifest(&text);
        let mut on_disk: Vec<String> = match std::fs::read_dir(&dir) {
            Ok(rd) => rd
                .flatten()
                .filter_map(|e| e.file_name().to_str().map(str::to_string))
                .filter(|n| n.ends_with(".o"))
                .collect(),
            Err(e) => self.fail("frozen-inputs", &format!("cannot list goldens/shim: {e}")),
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
            if &got != want {
                self.fail(
                    "frozen-inputs",
                    &format!("{path}: sha256 {got} != recorded {want} (frozen artifact moved -- WRONG, never rebuilt; derivation is 'rhu-gate shim rederive')"),
                );
            }
        }
        self.records = records;
        self.results.emit(&format!(
            "{{\"schema\":1,\"type\":\"case\",\"gate\":\"{}\",\"case\":\"frozen-inputs\",\"verdict\":\"pass\",\"exit\":null,\"exit_want\":null,\"control\":\"positive\",\"image\":null,\"stats\":{{\"manifest_sha\":\"{SHA_FROZEN_MANIFEST}\",\"objects\":{},\"goldens\":{}}}}}",
            self.gate_id,
            FROZEN_PLAN.len(),
            GOLDENS.len()
        ));
        self.results.tally("pass", "positive");
        println!(
            "== frozen inputs: PASS ({}/{} objects and {}/{} goldens tie the manifest both directions; the manifest ties the in-code pin) ==",
            FROZEN_PLAN.len(),
            FROZEN_PLAN.len(),
            GOLDENS.len(),
            GOLDENS.len()
        );
    }

    /// nm dump of an object (all symbols) or its undefined set.
    fn nm_dump(&mut self, facts: &RustcFacts, case: &str, obj_rel: &str, undefined_only: bool) -> String {
        let obj_abs = self.repo.join(obj_rel.replace('/', std::path::MAIN_SEPARATOR_STR));
        self.nm_dump_path(facts, case, &obj_abs, obj_rel, undefined_only)
    }

    /// nm dump of a file named by its own path (the survey's archives).
    fn nm_dump_path(&mut self, facts: &RustcFacts, case: &str, obj_abs: &Path, obj_rel: &str, undefined_only: bool) -> String {
        let nm = match llvm_tool(facts, "llvm-nm") {
            Ok(p) => p,
            Err(e) => self.fail(case, &e),
        };
        let stem = Path::new(obj_rel).file_stem().unwrap().to_string_lossy().into_owned();
        let dump_p = self.out.join(format!("{stem}.{}", if undefined_only { "nm-u" } else { "nm" }));
        let scratch = self.out.join("nm.stderr");
        let obj_arg = obj_abs.to_string_lossy().into_owned();
        let args: Vec<&str> = if undefined_only { vec!["-u", &obj_arg] } else { vec!["--defined-only", &obj_arg] };
        let run = run_captured(&nm, &args, &dump_p, &scratch, Duration::from_secs(60));
        if !matches!(run, Ok(ref o) if o.exit == 0) {
            self.fail(case, &format!("llvm-nm failed on {obj_rel}"));
        }
        std::fs::read_to_string(&dump_p).unwrap_or_default()
    }

    fn pin_closure(&mut self, facts: &RustcFacts, case: &str, obj: &str, want: &[&str]) -> Vec<String> {
        let got = parse_undefined(&self.nm_dump(facts, case, &format!("goldens/shim/{obj}"), true));
        if !imports_match(&got, want) {
            self.fail(
                case,
                &format!(
                    "{obj} import closure moved: got [{}] (pinned {} names) -- a new or vanished import is a world change",
                    got.join(" "),
                    want.len()
                ),
            );
        }
        if let Some(hit) = forbidden_import(&got) {
            self.fail(case, &format!("{obj} imports forbidden family symbol '{hit}'"));
        }
        got
    }

    /// The one-strong-symbol sweep over the objects an image links, and
    /// the every-import-resolves check against the objects' own
    /// definitions plus the kernel staticlib's.
    fn sweep_strong(&mut self, facts: &RustcFacts, case: &str, objs: &[&str]) {
        let dumps: Vec<String> = objs.iter().map(|o| self.nm_dump(facts, case, o, false)).collect();
        let refs: Vec<&str> = dumps.iter().map(String::as_str).collect();
        let dups = duplicate_definitions(&refs);
        if !dups.is_empty() {
            self.fail(case, &format!("symbols defined strongly in more than one object: {}", dups.join(" ")));
        }
        let imports: Vec<Vec<String>> = objs.iter().map(|o| parse_undefined(&self.nm_dump(facts, case, o, true))).collect();
        let defined: Vec<Vec<String>> = dumps.iter().map(|d| defined_names(d)).collect();
        let lib = defined_names(&self.nm_dump(facts, case, "src/kernel/target/thumbv7em-none-eabihf/release/librhu_kernel.a", false));
        let missing = unresolved_imports(&imports, &defined, &lib);
        if !missing.is_empty() {
            self.fail(case, &format!("imports defined by nothing in the image (hidden from the link by section GC): {}", missing.join(" ")));
        }
    }

    /// Link objects + the payload-feature staticlib under the payload
    /// board map; returns (sha, repo-relative path) of the ELF.
    fn link_image(&mut self, ld: &ToolCmd, case: &str, objs: &[&str], name: &str) -> (String, String) {
        self.link_image_extra(ld, case, objs, name, &[], &[])
    }

    /// As link_image, with extra arguments after the objects (libraries)
    /// and before them (link-line options such as an undefined-symbol
    /// request).
    fn link_image_extra(&mut self, ld: &ToolCmd, case: &str, objs: &[&str], name: &str, after: &[String], before: &[&str]) -> (String, String) {
        let path_rel = format!("src/kernel/target/shim-gate/{name}.elf");
        let map_rel = format!("src/kernel/target/shim-gate/{name}.map");
        let ld_log = self.out.join(format!("{name}-ld.log"));
        let ld_out = self.out.join(format!("{name}-ld.stdout"));
        let mut args: Vec<String> = vec![
            "-T".to_string(),
            "src/kernel/payload-mps2-an500.x".to_string(),
            "--gc-sections".to_string(),
            format!("-Map={map_rel}"),
        ];
        args.extend(before.iter().map(|s| s.to_string()));
        args.extend(objs.iter().map(|s| s.to_string()));
        args.push("src/kernel/target/thumbv7em-none-eabihf/release/librhu_kernel.a".to_string());
        args.extend(after.iter().cloned());
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

    /// Run a linked image under QEMU (Parity profile); (raw, exit).
    fn run_qemu(&mut self, qemu: &Path, case: &str, path_rel: &str) -> (Vec<u8>, i32) {
        let elf = self.repo.join(path_rel.replace('/', std::path::MAIN_SEPARATOR_STR));
        let raw_path = self.out.join(format!("{case}.raw"));
        let stderr_path = self.out.join(format!("{case}.stderr"));
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
        (raw, outcome.exit)
    }

    /// The artifact contract of a linked shim image by nm: the common
    /// symbol set plus the case's own `extra` names.
    fn artifact_contract(&mut self, facts: &RustcFacts, case: &str, path_rel: &str, extra: &[&str], want_span: u64) {
        let nm = match llvm_tool(facts, "llvm-nm") {
            Ok(p) => p,
            Err(e) => self.fail(case, &e),
        };
        let elf_abs = self.repo.join(path_rel.replace('/', std::path::MAIN_SEPARATOR_STR));
        let elf_arg = elf_abs.to_string_lossy().into_owned();
        let sym = self.out.join(format!("{case}.elf.nm"));
        let scratch = self.out.join(format!("{case}.elf.nm.stderr"));
        let nm_run = run_captured(&nm, &[&elf_arg], &sym, &scratch, Duration::from_secs(60));
        if !matches!(nm_run, Ok(ref o) if o.exit == 0) {
            self.fail(case, "llvm-nm failed on the linked ELF");
        }
        let dump = std::fs::read(&sym).unwrap_or_default();
        for s in ["__managed__Main", "__rhu_debug_layout", "__rhu_sched", "rhu_shim_init"] {
            if !word_sym_present(&dump, s) {
                self.fail(case, &format!("symbol '{s}' absent from the linked artifact"));
            }
        }
        for s in extra {
            if !word_sym_present(&dump, s) {
                self.fail(case, &format!("symbol '{s}' absent from the linked artifact"));
            }
        }
        if let Some(fault) = template_span_fault(
            word_sym_value(&dump, "__rhu_tdata_lo"),
            word_sym_value(&dump, "__rhu_tdata_hi"),
            want_span,
        ) {
            self.fail(case, &fault);
        }
    }

    /// A positive run: exit 0, CR discipline, the golden byte for byte,
    /// pinned lines present, forbidden patterns absent, the verdict line
    /// the payload stream's last.
    #[allow(clippy::too_many_arguments)]
    fn assert_positive(&mut self, case: &str, raw: &[u8], code: i32, sha: &str, rel: &str, golden: &str, pinned: &[&str], all_good: &str) {
        if cr_count(raw) == 0 {
            self.fail_run(case, code, 0, "positive", "", sha, rel, "no CR bytes in raw capture (wire discipline lost)");
        }
        let gpath = self.repo.join("goldens").join("shim").join(golden);
        let gbytes = match std::fs::read(&gpath) {
            Ok(b) => b,
            Err(e) => self.fail_run(case, code, 0, "positive", "", sha, rel, &format!("golden unreadable: {e}")),
        };
        let stream = payload_stream(raw);
        if !eq_lf_normalized(&stream, &gbytes) {
            self.fail_run(case, code, 0, "positive", "", sha, rel, &format!("payload stream differs from goldens/shim/{golden} (byte class)"));
        }
        for m in pinned {
            if !contains_fixed(raw, m) {
                self.fail_run(case, code, 0, "positive", "", sha, rel, &format!("pinned line '{m}' not found"));
            }
        }
        for m in FORBIDDEN_PATTERNS {
            if contains_fixed(raw, m) {
                self.fail_run(case, code, 0, "positive", "", sha, rel, &format!("forbidden pattern '{m}' present"));
            }
        }
        let last = String::from_utf8_lossy(&stream).lines().last().unwrap_or("").to_string();
        if last != all_good {
            self.fail_run(case, code, 0, "positive", "", sha, rel, &format!("the payload stream's last line is not the verdict line (got '{last}')"));
        }
        if code != 0 {
            self.fail_run(case, code, 0, "positive", "", sha, rel, &format!("exit {code}, wanted 0"));
        }
    }

    /// A negative control: the expected exit, the expected line, no
    /// verdict line, and the positive golden compare FAILING against it
    /// (the detector bites).
    #[allow(clippy::too_many_arguments)]
    fn assert_control(&mut self, case: &str, raw: &[u8], code: i32, want: i32, broke: &str, sha: &str, rel: &str, golden: &str, expect_line: &str, all_good: &str) {
        if cr_count(raw) == 0 {
            self.fail_run(case, code, want, "negative", broke, sha, rel, "no CR bytes in raw capture");
        }
        if !contains_fixed(raw, expect_line) {
            self.fail_run(case, code, want, "negative", broke, sha, rel, &format!("expected line '{expect_line}' not found"));
        }
        if contains_fixed(raw, all_good) {
            self.fail_run(case, code, want, "negative", broke, sha, rel, "the verdict line appeared on a broken run");
        }
        let gpath = self.repo.join("goldens").join("shim").join(golden);
        let gbytes = std::fs::read(&gpath).unwrap_or_default();
        if eq_lf_normalized(&payload_stream(raw), &gbytes) {
            self.fail_run(case, code, want, "negative", broke, sha, rel, "the positive golden compare ACCEPTED the broken transcript (the detector is blind)");
        }
        if code != want {
            self.fail_run(case, code, want, "negative", broke, sha, rel, &format!("exit {code}, wanted {want}"));
        }
    }

    fn emit_pass(&mut self, case: &str, code: i32, control: &str, broke: &str, sha: &str, rel: &str, golden: Option<&str>) {
        let broke_frag = if control == "negative" { format!(",\"broke\":\"{broke}\"") } else { String::new() };
        let golden_frag = match golden {
            Some(g) => format!(",\"golden\":{{\"path\":\"goldens/shim/{g}\",\"sha256\":\"{}\",\"compare\":\"bytes\"}}", self.golden_sha(g)),
            None => String::new(),
        };
        self.results.emit(&format!(
            "{{\"schema\":1,\"type\":\"case\",\"gate\":\"{}\",\"case\":\"{case}\",\"verdict\":\"pass\",\"exit\":{code},\"exit_want\":{code},\"control\":\"{control}\",\"image\":{{\"path\":\"{rel}\",\"sha256\":\"{sha}\"}}{golden_frag}{broke_frag}}}",
            self.gate_id
        ));
        self.results.tally("pass", control);
    }

    fn build_staticlib(&mut self) {
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
            self.fail("shim.core", "kernel staticlib build error");
        }
    }

    fn run_core(&mut self, facts: &RustcFacts, ld: &ToolCmd, qemu: &Path) {
        println!("== case shim.core ==");
        let core = self.pin_closure(facts, "shim.core", "shim-core.o", &IMPORTS_CORE);
        let sysn = self.pin_closure(facts, "shim.core", "shim-sysnative.o", &IMPORTS_SYSNATIVE);
        let glue = self.pin_closure(facts, "shim.core", "shim-libc-glue.o", &IMPORTS_GLUE);
        let tenant = parse_undefined(&self.nm_dump(facts, "shim.core", "goldens/shim/shimtest-core.o", true));
        if let Some(hit) = forbidden_import(&tenant) {
            self.fail("shim.core", &format!("the tenant imports forbidden family symbol '{hit}'"));
        }
        println!(
            "   import closures: core {}/{}, PAL {}/{}, glue {}/{} pinned; tenant {} names; forbidden families absent",
            core.len(),
            IMPORTS_CORE.len(),
            sysn.len(),
            IMPORTS_SYSNATIVE.len(),
            glue.len(),
            IMPORTS_GLUE.len(),
            tenant.len()
        );
        let surface = self.pin_closure(facts, "shim.core", "shim-pal-surface.o", &IMPORTS_PAL_SURFACE);
        println!("   shim-pal-surface.o: import closure pinned exactly ({} names: {})", IMPORTS_PAL_SURFACE.len(), surface.join(" "));
        let core_an505 = self.pin_closure(facts, "shim.core", "shim-core-an505.o", &IMPORTS_CORE);
        let sysn_an505 = self.pin_closure(facts, "shim.core", "shim-sysnative-an505.o", &IMPORTS_SYSNATIVE);
        let glue_an505 = self.pin_closure(facts, "shim.core", "shim-libc-glue-an505.o", &IMPORTS_GLUE);
        let surface_an505 = self.pin_closure(facts, "shim.core", "shim-pal-surface-an505.o", &IMPORTS_PAL_SURFACE);
        if surface_an505 != surface {
            self.fail("shim.core", "shim-pal-surface-an505.o's closure differs from shim-pal-surface.o's: the class moved an import");
        }
        println!(
            "   the mps2-an505 row's Cortex-M33 objects: core {}/{}, PAL {}/{}, glue {}/{} pinned against the same closures",
            core_an505.len(),
            IMPORTS_CORE.len(),
            sysn_an505.len(),
            IMPORTS_SYSNATIVE.len(),
            glue_an505.len(),
            IMPORTS_GLUE.len()
        );
        let world = ["goldens/shim/shim-core.o", "goldens/shim/shim-sysnative.o", "goldens/shim/shim-libc-glue.o"];
        self.sweep_strong(facts, "shim.core", &[world[0], world[1], world[2], "goldens/shim/shimtest-core.o"]);
        println!("   one strong definition per symbol across the four linked objects; every import resolves (the objects, the kernel staticlib, and the link's own symbols: the map's arena window, data and template brackets, and the linker's constructor-array brackets)");

        let (sha, rel) = self.link_image(ld, "shim.core", &[world[0], world[1], world[2], "goldens/shim/shimtest-core.o"], "shim-core");
        self.artifact_contract(facts, "shim.core", &rel, &["SystemNative_GetErrNo", "__errno_location"], NO_TEMPLATE);
        let (raw, code) = self.run_qemu(qemu, "shim.core", &rel);
        self.assert_positive("shim.core", &raw, code, &sha, &rel, GOLDENS[0], &PINNED_CORE_LINES, CORE_ALL_GOOD);
        self.emit_pass("shim.core", 0, "positive", "", &sha, &rel, Some(GOLDENS[0]));
        println!("== case shim.core: PASS (exit 0; the payload stream equals its golden; {} pinned diagnostic lines; the verdict line last) ==", PINNED_CORE_LINES.len());

        println!("== case shim.core-break-errno (negative control) ==");
        let broke = "a worker writes through the main thread's errno pointer (RHU_SHIMTEST_BREAK_ERRNO)";
        let (sha_b, rel_b) = self.link_image(ld, "shim.core-break-errno", &[world[0], world[1], world[2], "goldens/shim/shimtest-core-break-errno.o"], "shim-core-break-errno");
        let (raw_b, code_b) = self.run_qemu(qemu, "shim.core-break-errno", &rel_b);
        self.assert_control("shim.core-break-errno", &raw_b, code_b, 1, broke, &sha_b, &rel_b, GOLDENS[0], "FAIL shim-errno-thread", CORE_ALL_GOOD);
        self.emit_pass("shim.core-break-errno", 1, "negative", broke, &sha_b, &rel_b, None);
        println!("== case shim.core-break-errno: PASS (exit 1; the isolation check failed loudly; the golden compare rejected the transcript) ==");

        println!("== case shim.core-break-canary (negative control) ==");
        let broke = "a block header corrupted before free (RHU_SHIMTEST_BREAK_CANARY)";
        let (sha_c, rel_c) = self.link_image(ld, "shim.core-break-canary", &[world[0], world[1], world[2], "goldens/shim/shimtest-core-break-canary.o"], "shim-core-break-canary");
        let (raw_c, code_c) = self.run_qemu(qemu, "shim.core-break-canary", &rel_c);
        self.assert_control("shim.core-break-canary", &raw_c, code_c, 134, broke, &sha_c, &rel_c, GOLDENS[0], CANARY_LINE, CORE_ALL_GOOD);
        self.emit_pass("shim.core-break-canary", 134, "negative", broke, &sha_c, &rel_c, None);
        println!("== case shim.core-break-canary: PASS (exit 134; the allocator died loud on the corrupted header) ==");

        println!("== case shim.core-break-entropy-refusal (negative control) ==");
        let broke = "the cryptographic entry reporting a refused draw as served (RHU_SHIM_BREAK_ENTROPY_REFUSAL)";
        let (sha_e, rel_e) = self.link_image(ld, "shim.core-break-entropy-refusal", &[world[0], "goldens/shim/shim-sysnative-break-entropy-refusal.o", world[2], "goldens/shim/shimtest-core.o"], "shim-core-break-entropy-refusal");
        let (raw_e, code_e) = self.run_qemu(qemu, "shim.core-break-entropy-refusal", &rel_e);
        self.assert_control("shim.core-break-entropy-refusal", &raw_e, code_e, 1, broke, &sha_e, &rel_e, GOLDENS[0], "FAIL shim-entropy (a refused draw reported as served)", CORE_ALL_GOOD);
        self.emit_pass("shim.core-break-entropy-refusal", 1, "negative", broke, &sha_e, &rel_e, None);
        println!("== case shim.core-break-entropy-refusal: PASS (exit 1; the tenant refused a draw the door had refused and the entry called served) ==");

        println!("== case shim.core-break-calendar-clock (negative control) ==");
        let broke = "the system time answered from the boot clock instead of the kernel's calendar (RHU_SHIM_BREAK_CALENDAR_CLOCK)";
        let (sha_k, rel_k) = self.link_image(ld, "shim.core-break-calendar-clock", &[world[0], "goldens/shim/shim-sysnative-break-calendar-clock.o", world[2], "goldens/shim/shimtest-core.o"], "shim-core-break-calendar-clock");
        let (raw_k, code_k) = self.run_qemu(qemu, "shim.core-break-calendar-clock", &rel_k);
        self.assert_control("shim.core-break-calendar-clock", &raw_k, code_k, 1, broke, &sha_k, &rel_k, GOLDENS[0], "FAIL shim-calendar (the system time is not the kernel's calendar)", CORE_ALL_GOOD);
        self.emit_pass("shim.core-break-calendar-clock", 1, "negative", broke, &sha_k, &rel_k, None);
        println!("== case shim.core-break-calendar-clock: PASS (exit 1; the tenant refused a system time that was not the calendar) ==");
    }

    fn run_fdmap(&mut self, facts: &RustcFacts, ld: &ToolCmd, qemu: &Path) {
        println!("== case shim.fdmap ==");
        let tenant = parse_undefined(&self.nm_dump(facts, "shim.fdmap", "goldens/shim/shimtest-fdmap.o", true));
        if let Some(hit) = forbidden_import(&tenant) {
            self.fail("shim.fdmap", &format!("the tenant imports forbidden family symbol '{hit}'"));
        }
        let world = ["goldens/shim/shim-core.o", "goldens/shim/shim-sysnative.o", "goldens/shim/shim-libc-glue.o"];
        self.sweep_strong(facts, "shim.fdmap", &[world[0], world[1], world[2], "goldens/shim/shimtest-fdmap.o"]);
        let (sha, rel) = self.link_image(ld, "shim.fdmap", &[world[0], world[1], world[2], "goldens/shim/shimtest-fdmap.o"], "shim-fdmap");
        self.artifact_contract(facts, "shim.fdmap", &rel, &["SystemNative_GetErrNo", "__errno_location"], NO_TEMPLATE);
        let (raw, code) = self.run_qemu(qemu, "shim.fdmap", &rel);
        self.assert_positive("shim.fdmap", &raw, code, &sha, &rel, GOLDENS[1], &[PINNED_CORE_LINES[0]], FDMAP_ALL_GOOD);
        self.emit_pass("shim.fdmap", 0, "positive", "", &sha, &rel, Some(GOLDENS[1]));
        println!("== case shim.fdmap: PASS (exit 0; the payload stream equals its golden; the verdict line last) ==");

        println!("== case shim.fdmap-break-fdbase (negative control) ==");
        let broke = "the file band placed at the console's base (RHU_SHIM_BREAK_FD_BASE)";
        let (sha_b, rel_b) = self.link_image(ld, "shim.fdmap-break-fdbase", &[world[0], "goldens/shim/shim-sysnative-break-fdbase.o", world[2], "goldens/shim/shimtest-fdmap.o"], "shim-fdmap-break-fdbase");
        let (raw_b, code_b) = self.run_qemu(qemu, "shim.fdmap-break-fdbase", &rel_b);
        self.assert_control("shim.fdmap-break-fdbase", &raw_b, code_b, 1, broke, &sha_b, &rel_b, GOLDENS[1], "FAIL shim-fdmap-open-fstat", FDMAP_ALL_GOOD);
        self.emit_pass("shim.fdmap-break-fdbase", 1, "negative", broke, &sha_b, &rel_b, None);
        println!("== case shim.fdmap-break-fdbase: PASS (exit 1; the console answered for a file and the ladder refused it) ==");

        const STORE_CONTROLS: [(&str, &str, &str, &str); 7] = [
            ("shim.fdmap-break-store-bounds", "shim-sysnative-break-store-bounds.o", "a positioned transfer's offset cut to 32 bits (RHU_SHIM_BREAK_STORE_BOUNDS)", "FAIL shim-fdmap-bounds"),
            ("shim.fdmap-break-store-lifetime", "shim-sysnative-break-store-lifetime.o", "an unlink releasing an open object (RHU_SHIM_BREAK_STORE_LIFETIME)", "FAIL shim-fdmap-lifetime"),
            ("shim.fdmap-break-store-open-order", "shim-sysnative-break-store-open-order.o", "an open creating and truncating before the descriptor search (RHU_SHIM_BREAK_STORE_OPEN_ORDER)", "FAIL shim-fdmap-open-order"),
            ("shim.fdmap-break-store-access", "shim-sysnative-break-store-access.o", "a write-only descriptor readable (RHU_SHIM_BREAK_STORE_ACCESS)", "FAIL shim-fdmap-access"),
            ("shim.fdmap-break-store-flock", "shim-sysnative-break-store-flock.o", "every lock request granted (RHU_SHIM_BREAK_STORE_FLOCK)", "FAIL shim-fdmap-flock"),
            ("shim.fdmap-break-store-slash", "shim-sysnative-break-store-slash.o", "a trailing slash kept in a path (RHU_SHIM_BREAK_STORE_SLASH)", "FAIL shim-fdmap-dir-slash"),
            ("shim.fdmap-break-store-times", "shim-sysnative-break-store-times.o", "a nanosecond field unchecked (RHU_SHIM_BREAK_STORE_TIMES)", "FAIL shim-fdmap-times-range"),
        ];
        for (case, obj, broke, expect) in STORE_CONTROLS {
            println!("== case {case} (negative control) ==");
            let obj_rel = format!("goldens/shim/{obj}");
            let image = case.replace('.', "-");
            let (sha_s, rel_s) = self.link_image(ld, case, &[world[0], obj_rel.as_str(), world[2], "goldens/shim/shimtest-fdmap.o"], &image);
            let (raw_s, code_s) = self.run_qemu(qemu, case, &rel_s);
            self.assert_control(case, &raw_s, code_s, 1, broke, &sha_s, &rel_s, GOLDENS[1], expect, FDMAP_ALL_GOOD);
            self.emit_pass(case, 1, "negative", broke, &sha_s, &rel_s, None);
            println!("== case {case}: PASS (exit 1; the ladder refused the restored defect at its own check) ==");
        }
    }

    fn run_activation(&mut self, facts: &RustcFacts, ld: &ToolCmd, qemu: &Path) {
        println!("== case shim.activation ==");
        let tenant = parse_undefined(&self.nm_dump(facts, "shim.activation", "goldens/shim/shimtest-activation.o", true));
        if let Some(hit) = forbidden_import(&tenant) {
            self.fail("shim.activation", &format!("the tenant imports forbidden family symbol '{hit}'"));
        }
        let world = ["goldens/shim/shim-core.o", "goldens/shim/shim-sysnative.o", "goldens/shim/shim-libc-glue.o"];
        self.sweep_strong(facts, "shim.activation", &[world[0], world[1], world[2], "goldens/shim/shimtest-activation.o"]);
        let (sha, rel) = self.link_image(ld, "shim.activation", &[world[0], world[1], world[2], "goldens/shim/shimtest-activation.o"], "shim-activation");
        self.artifact_contract(facts, "shim.activation", &rel, &ACT_SYMBOLS, NO_TEMPLATE);
        let (raw, code) = self.run_qemu(qemu, "shim.activation", &rel);
        self.assert_positive("shim.activation", &raw, code, &sha, &rel, GOLDENS[2], &PINNED_ACT_LINES, ACT_ALL_GOOD);
        self.emit_pass("shim.activation", 0, "positive", "", &sha, &rel, Some(GOLDENS[2]));
        println!("== case shim.activation: PASS (exit 0; the payload stream equals its golden; {} pinned diagnostic lines; the verdict line last) ==", PINNED_ACT_LINES.len());

        println!("== case shim.activation-break-sipid (negative control) ==");
        let broke = "the activation's signal information names a foreign process (RHU_SHIM_BREAK_SIPID)";
        let (sha_b, rel_b) = self.link_image(ld, "shim.activation-break-sipid", &["goldens/shim/shim-core-break-sipid.o", world[1], world[2], "goldens/shim/shimtest-activation.o"], "shim-activation-break-sipid");
        let (raw_b, code_b) = self.run_qemu(qemu, "shim.activation-break-sipid", &rel_b);
        self.assert_control("shim.activation-break-sipid", &raw_b, code_b, 1, broke, &sha_b, &rel_b, GOLDENS[2], ACT_FAIL_SIPID, ACT_ALL_GOOD);
        self.emit_pass("shim.activation-break-sipid", 1, "negative", broke, &sha_b, &rel_b, None);
        println!("== case shim.activation-break-sipid: PASS (exit 1; the delivery check refused a foreign process id; the golden compare rejected the transcript) ==");

        println!("== case shim.activation-break-ctxpc (negative control) ==");
        let broke = "the context's program counter filled from the link register (RHU_SHIM_BREAK_CTXPC)";
        let (sha_c, rel_c) = self.link_image(ld, "shim.activation-break-ctxpc", &["goldens/shim/shim-core-break-ctxpc.o", world[1], world[2], "goldens/shim/shimtest-activation.o"], "shim-activation-break-ctxpc");
        let (raw_c, code_c) = self.run_qemu(qemu, "shim.activation-break-ctxpc", &rel_c);
        self.assert_control("shim.activation-break-ctxpc", &raw_c, code_c, 1, broke, &sha_c, &rel_c, GOLDENS[2], ACT_FAIL_CTXPC, ACT_ALL_GOOD);
        self.emit_pass("shim.activation-break-ctxpc", 1, "negative", broke, &sha_c, &rel_c, None);
        println!("== case shim.activation-break-ctxpc: PASS (exit 1; the delivery check refused a program counter outside the worker's window) ==");
    }

    fn run_tls(&mut self, facts: &RustcFacts, ld: &ToolCmd, qemu: &Path) {
        println!("== case shim.tls ==");
        let tenant = parse_undefined(&self.nm_dump(facts, "shim.tls", "goldens/shim/shimtest-tls.o", true));
        if let Some(hit) = forbidden_import(&tenant) {
            self.fail("shim.tls", &format!("the tenant imports forbidden family symbol '{hit}'"));
        }
        let world = ["goldens/shim/shim-core.o", "goldens/shim/shim-sysnative.o", "goldens/shim/shim-libc-glue.o"];
        self.sweep_strong(facts, "shim.tls", &[world[0], world[1], world[2], "goldens/shim/shimtest-tls.o"]);
        let (sha, rel) = self.link_image(ld, "shim.tls", &[world[0], world[1], world[2], "goldens/shim/shimtest-tls.o"], "shim-tls");
        self.artifact_contract(facts, "shim.tls", &rel, &TLS_SYMBOLS, TLS_TEMPLATE_SPAN);
        let (raw, code) = self.run_qemu(qemu, "shim.tls", &rel);
        self.assert_positive("shim.tls", &raw, code, &sha, &rel, GOLDENS[3], &[PINNED_CORE_LINES[0]], TLS_ALL_GOOD);
        self.emit_pass("shim.tls", 0, "positive", "", &sha, &rel, Some(GOLDENS[3]));
        println!("== case shim.tls: PASS (exit 0; the bracket pair spans {TLS_TEMPLATE_SPAN} bytes; the payload stream equals its golden; the verdict line last) ==");

        println!("== case shim.tls-break-tlsoff (negative control) ==");
        let broke = "a thread-local access resolved without its offset (RHU_SHIM_BREAK_TLSOFF)";
        let (sha_b, rel_b) = self.link_image(ld, "shim.tls-break-tlsoff", &["goldens/shim/shim-core-break-tlsoff.o", world[1], world[2], "goldens/shim/shimtest-tls.o"], "shim-tls-break-tlsoff");
        let (raw_b, code_b) = self.run_qemu(qemu, "shim.tls-break-tlsoff", &rel_b);
        self.assert_control("shim.tls-break-tlsoff", &raw_b, code_b, 1, broke, &sha_b, &rel_b, GOLDENS[3], TLS_FAIL_OFF, TLS_ALL_GOOD);
        self.emit_pass("shim.tls-break-tlsoff", 1, "negative", broke, &sha_b, &rel_b, None);
        println!("== case shim.tls-break-tlsoff: PASS (exit 1; the zero-fill word read the initialized one) ==");

        println!("== case shim.tls-break-tlscache (negative control) ==");
        let broke = "the first thread-local block seen handed to every thread after it (RHU_SHIM_BREAK_TLSCACHE)";
        let (sha_c, rel_c) = self.link_image(ld, "shim.tls-break-tlscache", &["goldens/shim/shim-core-break-tlscache.o", world[1], world[2], "goldens/shim/shimtest-tls.o"], "shim-tls-break-tlscache");
        let (raw_c, code_c) = self.run_qemu(qemu, "shim.tls-break-tlscache", &rel_c);
        self.assert_control("shim.tls-break-tlscache", &raw_c, code_c, 1, broke, &sha_c, &rel_c, GOLDENS[3], TLS_FAIL_CACHE, TLS_ALL_GOOD);
        self.emit_pass("shim.tls-break-tlscache", 1, "negative", broke, &sha_c, &rel_c, None);
        println!("== case shim.tls-break-tlscache: PASS (exit 1; a worker read the hosting thread's block) ==");
    }

    fn run_host(&mut self, facts: &RustcFacts, ld: &ToolCmd, qemu: &Path) {
        println!("== case shim.host ==");
        let tenant = parse_undefined(&self.nm_dump(facts, "shim.host", "goldens/shim/shimtest-host.o", true));
        if let Some(hit) = forbidden_import(&tenant) {
            self.fail("shim.host", &format!("the tenant imports forbidden family symbol '{hit}'"));
        }
        let host = self.pin_closure(facts, "shim.host", "shim-host.o", &IMPORTS_HOST);
        let bridge = self.pin_closure(facts, "shim.host", "shim-stdio-sym.o", &IMPORTS_STDIO);
        println!(
            "   import closures: host {}/{}, stream bridge {}/{} pinned (the bridge's parsers resolve at the hosted runtime's link); tenant {} names; forbidden families absent",
            host.len(),
            IMPORTS_HOST.len(),
            bridge.len(),
            IMPORTS_STDIO.len(),
            tenant.len()
        );
        let host_an505 = self.pin_closure(facts, "shim.host", "shim-host-an505.o", &IMPORTS_HOST);
        let bridge_an505 = self.pin_closure(facts, "shim.host", "shim-stdio-sym-an505.o", &IMPORTS_STDIO);
        println!(
            "   the mps2-an505 row's Cortex-M33 objects: host {}/{}, stream bridge {}/{} pinned against the same closures",
            host_an505.len(),
            IMPORTS_HOST.len(),
            bridge_an505.len(),
            IMPORTS_STDIO.len()
        );
        let world = [
            "goldens/shim/shim-core.o",
            "goldens/shim/shim-sysnative.o",
            "goldens/shim/shim-libc-glue.o",
            "goldens/shim/shim-host.o",
            "goldens/shim/shim-stdio-sym.o",
            "goldens/shim/shimtest-host.o",
        ];
        self.sweep_strong(facts, "shim.host", &world);
        let (sha, rel) = self.link_image(ld, "shim.host", &world, "shim-host");
        self.artifact_contract(facts, "shim.host", &rel, &HOST_SYMBOLS, NO_TEMPLATE);
        let (raw, code) = self.run_qemu(qemu, "shim.host", &rel);
        self.assert_positive("shim.host", &raw, code, &sha, &rel, GOLDENS[4], &[], HOST_ALL_GOOD);
        if let Some(fault) = markers_out_of_order(&raw, &HOST_MARKERS) {
            self.fail("shim.host", &format!("the initialization contract's order: {fault}"));
        }
        let mut tail: Vec<&str> = vec![HOST_MARKERS[HOST_MARKERS.len() - 1]];
        tail.extend_from_slice(&HOST_TAIL_MARKERS);
        if let Some(fault) = markers_out_of_order(&raw, &tail) {
            self.fail("shim.host", &format!("the accounting after managed entry: {fault}"));
        }
        self.emit_pass("shim.host", 0, "positive", "", &sha, &rel, Some(GOLDENS[4]));
        println!("== case shim.host: PASS (exit 0; {} contract markers IN ORDER, then the audit before the exit line; the payload stream equals its golden) ==", HOST_MARKERS.len());

        println!("== case shim.host-break-dataverify (negative control) ==");
        let broke = "one byte of the initialized data region damaged before the host verifies it (RHU_SHIM_BREAK_DATAVERIFY)";
        let broken_world = [
            world[0],
            world[1],
            world[2],
            "goldens/shim/shim-host-break-dataverify.o",
            world[4],
            world[5],
        ];
        let (sha_b, rel_b) = self.link_image(ld, "shim.host-break-dataverify", &broken_world, "shim-host-break-dataverify");
        let (raw_b, code_b) = self.run_qemu(qemu, "shim.host-break-dataverify", &rel_b);
        self.assert_control("shim.host-break-dataverify", &raw_b, code_b, 70, broke, &sha_b, &rel_b, GOLDENS[4], HOST_FAIL_VERIFY, HOST_ALL_GOOD);
        if contains_fixed(&raw_b, HOST_MARKERS[3]) {
            self.fail("shim.host-break-dataverify", "the constructor step ran after the data check failed");
        }
        self.emit_pass("shim.host-break-dataverify", 70, "negative", broke, &sha_b, &rel_b, None);
        println!("== case shim.host-break-dataverify: PASS (exit 70; the data check refused, and the constructor step never ran) ==");
    }

    /// Compile one source with the compiler of record into the gate's obj dir;
    /// on failure the stderr log is echoed and the gate fails.
    fn compile(&mut self, gxx: &ToolCmd, case: &str, src_rel: &str, extra: &[&str], out_rel: &str, class: Class) {
        let stem = Path::new(out_rel).file_stem().unwrap().to_string_lossy().into_owned();
        let stdout_p = self.out.join(format!("{stem}.cc.stdout"));
        let stderr_p = self.out.join(format!("{stem}.cc.stderr"));
        let mut args: Vec<String> = cc_flags(class).iter().map(|s| s.to_string()).collect();
        args.extend(extra.iter().map(|s| s.to_string()));
        args.push(src_rel.to_string());
        args.push("-o".to_string());
        args.push(out_rel.to_string());
        match run_tool_bounded(&self.repo, gxx, &args, &stdout_p, &stderr_p) {
            Ok(true) => {}
            Ok(false) => {
                echo_file(&stderr_p, "");
                self.fail(case, &format!("C compile failed for {src_rel} (see the compiler stderr log)"));
            }
            Err(e) => self.fail(case, &e),
        }
    }

    /// The link-tier control: with the toolchain's own C library on the
    /// link line, one heap and one console must still serve the whole
    /// image. Two measurements, because either alone can be satisfied by
    /// an accident:
    ///
    ///   (a) the image linked WITH the library on the line differs from
    ///       the image linked without it by the library's own definitions
    ///       only -- the reentrancy record and the stream objects the
    ///       stream bridge's weak references bind once the library's
    ///       parser members its forwarders name are fetched -- and by
    ///       none of the allocator's, the heap break's, the lock pair's or
    ///       the descriptor layer's, with nothing of the image's own gone
    ///       (an image that mentions no library name at all comes out
    ///       byte-identical, the stronger reading of the same fact);
    ///   (b) with a genuine consumer of that library FORCED into the
    ///       link, the library's own members do arrive -- and the
    ///       allocator that answers is still this image's, the heap break
    ///       is still the one that refuses, and none of the library's
    ///       allocator internals is present.
    ///
    /// (a) without (b) would pass on a link line that never reached the
    /// library at all; (b) without (a) would not show that the ordinary
    /// image takes only what the bridge is built to take.
    fn run_newlib_retarget(&mut self, gxx: &ToolCmd, facts: &RustcFacts, ld: &ToolCmd, dir: &str) {
        const CASE: &str = "shim.newlib-retarget";
        println!("== case {CASE} (link-tier control) ==");
        let world = [
            "goldens/shim/shim-core.o",
            "goldens/shim/shim-sysnative.o",
            "goldens/shim/shim-libc-glue.o",
            "goldens/shim/shim-host.o",
            "goldens/shim/shim-stdio-sym.o",
            "goldens/shim/shimtest-host.o",
        ];
        let (bare_sha, rel_bare) = self.link_image(ld, CASE, &world, "shim-newlib-bare");
        let lib_args = [format!("-L{dir}"), "-lc_nano".to_string()];
        let (with_sha, rel_with) = self.link_image_extra(ld, CASE, &world, "shim-newlib-linked", &lib_args, &[]);
        let bare_def = defined_names(&self.nm_dump(facts, CASE, &rel_bare, false));
        let with_def = defined_names(&self.nm_dump(facts, CASE, &rel_with, false));
        let lib_def = defined_names(&self.nm_dump(facts, CASE, &format!("{dir}/libc_nano.a"), false));
        let mut added: Vec<String> = with_def.iter().filter(|n| !bare_def.contains(n)).cloned().collect();
        added.sort();
        added.dedup();
        let mut removed: Vec<String> = bare_def.iter().filter(|n| !with_def.contains(n)).cloned().collect();
        removed.sort();
        removed.dedup();
        if let Some(f) = retarget_added_fault(&added, &removed, &lib_def) {
            self.fail(CASE, &f);
        }
        if bare_sha == with_sha {
            println!("   (a) the image is byte-identical with and without the library on the link line ({})", &bare_sha[..16]);
        } else {
            println!(
                "   (a) with the library on the link line the image gains the library's own definitions only, {} of them, none of its allocator, heap break, lock pair or descriptor layer, nothing of its own gone: [{}] ({} -> {})",
                added.len(),
                added.join(" "),
                &bare_sha[..16],
                &with_sha[..16]
            );
        }

        let probe_rel = "src/kernel/target/shim-gate/obj/newlib-consumer.c";
        write_fixture(
            &self.repo.join(probe_rel.replace('/', std::path::MAIN_SEPARATOR_STR)),
            PROBE_SOURCE,
        );
        let probe_obj = "src/kernel/target/shim-gate/obj/newlib-consumer.o";
        self.compile(gxx, CASE, probe_rel, &[], probe_obj, Class::CortexM7);
        let mut forced: Vec<&str> = world.to_vec();
        forced.push(probe_obj);
        let (forced_sha, rel_f) = self.link_image_extra(ld, CASE, &forced, "shim-newlib-forced", &lib_args, &["-u", "rhu_newlib_consumer"]);
        let dump = self.nm_dump(facts, CASE, &rel_f, false);
        let defined = strong_defined(&dump);
        if !defined.iter().any(|d| d == "_write") {
            self.fail(CASE, "with the library forced in, the writer it demands is not defined by this image's own runtime");
        }
        for f in RETARGET_FORBIDDEN {
            if word_sym_present(dump.as_bytes(), f) || defined.iter().any(|d| d == f) {
                self.fail(CASE, &format!("the library's own allocator arrived: '{f}' is in the image, so two heaps would answer"));
            }
        }
        let dangling = parse_undefined(&self.nm_dump(facts, CASE, &rel_f, true));
        if !dangling.is_empty() {
            self.fail(CASE, &format!("the image has undefined symbols after the library joined the link: {}", dangling.join(" ")));
        }
        println!("   (b) with a library consumer forced in, the writer it demanded is this image's, none of the library's allocator internals arrived, and nothing is left undefined");
        self.emit_pass(CASE, 0, "negative", "the toolchain's C library on the link line", &forced_sha, &rel_f, None);
        println!("== case {CASE}: PASS (one heap and one console with the toolchain's C library linked) ==");
    }

    /// The floating-point census over the frozen tier (host-side,
    /// image:null): every object decoded with the Cortex-M7 decoder, its
    /// count against the pin.
    fn run_fp_census(&mut self, facts: &RustcFacts) {
        const CASE: &str = "shim.fp-census";
        println!("== case {CASE} (the frozen tier's floating-point instruction census, pinned per object) ==");
        let objdump = match llvm_tool(facts, "llvm-objdump") {
            Ok(p) => p,
            Err(e) => self.fail(CASE, &e),
        };
        let mut got: Vec<(String, u32)> = Vec::new();
        for (name, _) in FP_CENSUS {
            let obj_abs = self.repo.join("goldens").join("shim").join(name);
            let obj_arg = obj_abs.to_string_lossy().into_owned();
            let dump_p = self.out.join(format!("{name}.objdump"));
            let scratch = self.out.join("objdump.stderr");
            let run = run_captured(&objdump, &["-d", "--mcpu=cortex-m7", &obj_arg], &dump_p, &scratch, Duration::from_secs(60));
            if !matches!(run, Ok(ref o) if o.exit == 0) {
                self.fail(CASE, &format!("llvm-objdump failed on goldens/shim/{name}"));
            }
            let text = std::fs::read_to_string(&dump_p).unwrap_or_default();
            if !text.lines().any(|l| l.split_once('\t').is_some()) {
                self.fail(CASE, &format!("llvm-objdump produced no instruction lines for goldens/shim/{name} (a broken instrument, never a zero)"));
            }
            got.push((name.to_string(), fp_instruction_count(&text)));
        }
        if let Some(f) = fp_census_fault(&got, &FP_CENSUS) {
            self.fail(
                CASE,
                &format!("the tier's floating-point census moved: {f} -- a moved count is a registered change (a source, flag or compiler change), never a drift"),
            );
        }
        let total: u32 = got.iter().map(|(_, n)| n).sum();
        let sites: Vec<String> = got.iter().filter(|(_, n)| *n > 0).map(|(name, n)| format!("{name} {n}")).collect();
        println!(
            "   {} objects censused with the Cortex-M7 decoder: {} floating-point instruction(s) in all, every count at its pin; the costume's core object 0; the sites: {}",
            got.len(),
            total,
            sites.join(", ")
        );
        self.results.emit(&format!(
            "{{\"schema\":1,\"type\":\"case\",\"gate\":\"{}\",\"case\":\"{CASE}\",\"verdict\":\"pass\",\"exit\":null,\"exit_want\":null,\"control\":\"positive\",\"image\":null,\"stats\":{{\"objects\":{},\"fp_instructions\":{total}}}}}",
            self.gate_id,
            got.len()
        ));
        self.results.tally("pass", "positive");
        println!("== case {CASE}: PASS (the activation costume's object carries no floating-point instruction; the tier's counts at their pins) ==");
    }

    /// The shim's P/Invoke-facing surface against the pinned pack's export
    /// tables (F135): the frozen expectation's pin ties the SDK's pin and
    /// the generated header's tag, then each row's six link objects define
    /// the expectation's family names exactly.
    fn run_pal_surface(&mut self, facts: &RustcFacts) {
        const CASE: &str = "shim.pal-surface";
        println!("== case {CASE} (the shim's P/Invoke-facing surface against the pinned runtime pack's export tables, the expectation goldens/shim/pal-surface.txt) ==");
        let text = match std::fs::read_to_string(self.repo.join("goldens").join("shim").join("pal-surface.txt")) {
            Ok(t) => t,
            Err(e) => self.fail(CASE, &format!("goldens/shim/pal-surface.txt unreadable: {e}")),
        };
        let exp = match parse_surface_expectation(&text) {
            Ok(x) => x,
            Err(e) => self.fail(CASE, &format!("goldens/shim/pal-surface.txt: {e}")),
        };
        let sdk_pin = match read_sdk_runtime_pin(&self.repo) {
            Ok(p) => p,
            Err(e) => self.fail(CASE, &e),
        };
        let tag = match read_surface_header_tag(&self.repo) {
            Ok(t) => t,
            Err(e) => self.fail(CASE, &e),
        };
        if let Some(f) = surface_pin_fault(&exp.pin, &sdk_pin, &tag) {
            self.fail(CASE, &f);
        }
        if exp.tag != format!("v{}", exp.pin) {
            self.fail(CASE, &format!("the expectation states tag {} for pin {}: the survey's tag and pin were not written together", exp.tag, exp.pin));
        }
        println!("   the expectation's pin {} (its tag {}) is the SDK's RhuRuntimeVersion and the surface header's tag {}; {} archives, {} family names", exp.pin, exp.tag, tag, exp.archives.len(), exp.names.len());
        for (row, objs) in [("an500", &ROW_AN500_LINK), ("an505", &ROW_AN505_LINK)] {
            let mut defined: Vec<String> = Vec::new();
            for o in objs.iter() {
                defined.extend(external_names(&self.nm_dump(facts, CASE, o, false)));
            }
            if let Some(f) = surface_fault(&defined, &exp.names) {
                self.fail(CASE, &format!("the {row} row: {f}"));
            }
            println!("   the {row} row's six link objects define the expectation's {} family names exactly", exp.names.len());
        }
        self.results.emit(&format!(
            "{{\"schema\":1,\"type\":\"case\",\"gate\":\"{}\",\"case\":\"{CASE}\",\"verdict\":\"pass\",\"exit\":null,\"exit_want\":null,\"control\":\"positive\",\"image\":null,\"stats\":{{\"names\":{},\"archives\":{},\"pin\":\"{}\"}}}}",
            self.gate_id,
            exp.names.len(),
            exp.archives.len(),
            exp.pin
        ));
        self.results.tally("pass", "positive");
        println!("== case {CASE}: PASS (the shim answers the pinned pack's whole P/Invoke-facing surface, {} names over the five families, on both rows; the pin tied) ==", exp.names.len());
    }

    /// The survey control (the rederive leg): the pack's five archives
    /// re-read for their sha256s and exported family names against the
    /// frozen expectation -- a moved pack or a moved set refuses naming the
    /// first difference, so the expectation is proven to be what the pack
    /// says at every run of the leg.
    fn run_pal_surface_survey(&mut self, facts: &RustcFacts, pack: &str) {
        const CASE: &str = "shim.pal-surface-survey";
        println!("== case {CASE} (survey-tier control: the pinned pack's archives at {pack} against goldens/shim/pal-surface.txt) ==");
        let text = match std::fs::read_to_string(self.repo.join("goldens").join("shim").join("pal-surface.txt")) {
            Ok(t) => t,
            Err(e) => self.fail(CASE, &format!("goldens/shim/pal-surface.txt unreadable: {e}")),
        };
        let exp = match parse_surface_expectation(&text) {
            Ok(x) => x,
            Err(e) => self.fail(CASE, &format!("goldens/shim/pal-surface.txt: {e}")),
        };
        let mut names: Vec<String> = Vec::new();
        for (archive, sha, count) in &exp.archives {
            let path = Path::new(pack).join(archive);
            let got = match sha256_file(&path) {
                Ok(s) => s,
                Err(e) => self.fail(CASE, &format!("the pack at {pack} carries no readable {archive}: {e}")),
            };
            if &got != sha {
                self.fail(CASE, &format!("{archive}: sha256 {got} != the expectation's {sha} -- the pack moved (a re-pin?): regenerate the survey"));
            }
            let dump = self.nm_dump_path(facts, CASE, &path, archive, false);
            let fam: Vec<String> = external_names(&dump).into_iter().filter(|n| SURFACE_FAMILIES.iter().any(|f| n.starts_with(f))).collect();
            let mut distinct = fam.clone();
            distinct.sort();
            distinct.dedup();
            if distinct.len() as u32 != *count {
                self.fail(CASE, &format!("{archive}: {} family names exported, the expectation records {count}", distinct.len()));
            }
            println!("   {archive}: sha256 ties the expectation; {} family names", distinct.len());
            names.extend(distinct);
        }
        names.sort();
        names.dedup();
        if names != exp.names {
            let first = names.iter().zip(exp.names.iter()).find(|(a, b)| a != b).map(|(a, b)| format!("{a} in the pack, {b} in the expectation")).unwrap_or_else(|| format!("{} names in the pack, {} in the expectation", names.len(), exp.names.len()));
            self.fail(CASE, &format!("the pack's family names differ from the expectation's at the first difference: {first} -- regenerate the survey"));
        }
        self.results.emit(&format!(
            "{{\"schema\":1,\"type\":\"case\",\"gate\":\"{}\",\"case\":\"{CASE}\",\"verdict\":\"pass\",\"exit\":null,\"exit_want\":null,\"control\":\"positive\",\"image\":null,\"stats\":{{\"names\":{},\"archives\":{}}}}}",
            self.gate_id,
            names.len(),
            exp.archives.len()
        ));
        self.results.tally("pass", "positive");
        println!("== case {CASE}: PASS (the pack's five archives tie the expectation's sha256s and its {} family names exactly) ==", names.len());
    }

    /// The census instrument's control (compile-tier): a compiled double
    /// multiply-add must be counted, or the census's zeros mean nothing.
    fn run_fp_census_instrument_control(&mut self, gxx: &ToolCmd, facts: &RustcFacts) {
        const CASE: &str = "shim.fp-census-instrument";
        println!("== case {CASE} (compile-tier control) ==");
        let probe_rel = "src/kernel/target/shim-gate/obj/fp-probe.c";
        write_fixture(
            &self.repo.join(probe_rel.replace('/', std::path::MAIN_SEPARATOR_STR)),
            b"double rhu_fp_probe(double a, double b);\ndouble rhu_fp_probe(double a, double b) { return a * b + 1.5; }\n",
        );
        let out_rel = "src/kernel/target/shim-gate/obj/fp-probe.o";
        self.compile(gxx, CASE, probe_rel, &[], out_rel, Class::CortexM7);
        let objdump = match llvm_tool(facts, "llvm-objdump") {
            Ok(p) => p,
            Err(e) => self.fail(CASE, &e),
        };
        let obj_abs = self.repo.join(out_rel.replace('/', std::path::MAIN_SEPARATOR_STR));
        let obj_arg = obj_abs.to_string_lossy().into_owned();
        let dump_p = self.out.join("fp-probe.objdump");
        let scratch = self.out.join("objdump.stderr");
        let run = run_captured(&objdump, &["-d", "--mcpu=cortex-m7", &obj_arg], &dump_p, &scratch, Duration::from_secs(60));
        if !matches!(run, Ok(ref o) if o.exit == 0) {
            self.fail(CASE, "llvm-objdump failed on the probe object");
        }
        let n = fp_instruction_count(&std::fs::read_to_string(&dump_p).unwrap_or_default());
        if n == 0 {
            self.fail(CASE, "the census counted no floating-point instruction in a compiled double multiply-add -- the instrument is blind");
        }
        println!("   a compiled double multiply-add is counted: {n} floating-point instruction(s)");
        self.results.emit(&format!(
            "{{\"schema\":1,\"type\":\"case\",\"gate\":\"{}\",\"case\":\"{CASE}\",\"verdict\":\"pass\",\"exit\":null,\"exit_want\":null,\"control\":\"negative\",\"image\":null,\"broke\":\"a compiled double multiply-add the census must count\"}}",
            self.gate_id
        ));
        self.results.tally("pass", "negative");
        println!("== case {CASE}: PASS (the census instrument counts a compiled double multiply-add) ==");
    }

    fn run_rederive(&mut self, gxx: &ToolCmd) {
        println!("== rederive: {} frozen-plan objects from src/shim with the compiler of record, compared apart from the producer stamp (each sha256 the object's with its comment section removed) ==", FROZEN_PLAN.len());
        let obj_dir = self.out.join("obj");
        if std::fs::create_dir_all(&obj_dir).is_err() {
            self.fail("rederive", "cannot create the object directory");
        }
        let header_text = std::fs::read_to_string(self.repo.join("src").join("kernel").join("include").join("rhu.h")).unwrap_or_default();
        let Some(abi) = header_abi(&header_text) else {
            self.fail("rederive", "src/kernel/include/rhu.h states no RHU_ABI (the hosting-API contract generation a frozen host object states)")
        };
        let facts = match rustc_facts() {
            Ok(f) => f,
            Err(e) => self.fail("rederive", &e),
        };
        let stripped = obj_dir.join("stripped");
        let mut stamps_frozen: Vec<String> = Vec::new();
        let mut stamps_fresh: Vec<String> = Vec::new();
        for (name, src, extra, class) in FROZEN_PLAN {
            let out_rel = format!("src/kernel/target/shim-gate/obj/{name}");
            let out_abs = self.repo.join(out_rel.replace('/', std::path::MAIN_SEPARATOR_STR));
            let _ = std::fs::remove_file(&out_abs);
            self.compile(gxx, "rederive", src, extra, &out_rel, class);
            let frozen_abs = self.repo.join("goldens").join("shim").join(name);
            let got = match sha256_without_stamp(&facts, &out_abs, &stripped.join(format!("fresh-{name}"))) {
                Ok(s) => s,
                Err(e) => self.fail("rederive", &format!("{name}: the compile produced no object, or its copy failed ({e})")),
            };
            let want = match sha256_without_stamp(&facts, &frozen_abs, &stripped.join(format!("frozen-{name}"))) {
                Ok(s) => s,
                Err(e) => self.fail("rederive", &format!("goldens/shim/{name} unreadable, or its copy failed: {e}")),
            };
            stamps_frozen.push(producer_stamp_of(&frozen_abs));
            stamps_fresh.push(producer_stamp_of(&out_abs));
            let frozen = match std::fs::read(&frozen_abs) {
                Ok(b) => b,
                Err(e) => self.fail("rederive", &format!("goldens/shim/{name} unreadable: {e}")),
            };
            if let Some(line) = generation_fault(name, &frozen, abi) {
                println!("   {line}");
            }
            if got != want {
                self.fail("rederive", &format!("{name}: recompiled sha256 {got} != frozen {want}, apart from the producer stamp -- the source, the flags or the kit moved: a re-freeze is a registered change"));
            }
            println!("   {name} OK ({})", &got[..16]);
        }
        println!("   the frozen tier's producer stamp (report-only): {}", distinct_stamps(&stamps_frozen));
        println!("   the rebuild's producer stamp (report-only): {}", distinct_stamps(&stamps_fresh));
        self.results.emit(&format!(
            "{{\"schema\":1,\"type\":\"case\",\"gate\":\"{}\",\"case\":\"rederive\",\"verdict\":\"pass\",\"exit\":null,\"exit_want\":null,\"control\":\"positive\",\"image\":null,\"stats\":{{\"objects\":{}}}}}",
            self.gate_id,
            FROZEN_PLAN.len()
        ));
        self.results.tally("pass", "positive");
        println!("== rederive: {}/{} objects byte-identical to goldens/shim apart from the producer stamp ==", FROZEN_PLAN.len(), FROZEN_PLAN.len());

        println!("== case shim.foreign-errno-refused (negative control) ==");
        let probe_rel = "src/kernel/target/shim-gate/obj/foreign-errno-probe.c";
        write_fixture(
            &self.repo.join(probe_rel.replace('/', std::path::MAIN_SEPARATOR_STR)),
            b"#include <errno.h>\n#include \"rhu_pal_errno_net11_rc1.h\"\nint rhu_probe_value = RHU_GERR_ETIMEDOUT;\n",
        );
        let out_rel = "src/kernel/target/shim-gate/obj/foreign-errno-probe.o";
        let stdout_p = self.out.join("foreign-errno-probe.cc.stdout");
        let stderr_p = self.out.join("foreign-errno-probe.cc.stderr");
        let mut args: Vec<String> = CC_FLAGS.iter().map(|s| s.to_string()).collect();
        args.push(probe_rel.to_string());
        args.push("-o".to_string());
        args.push(out_rel.to_string());
        let want_err = "a foreign errno.h is visible";
        match run_tool_bounded(&self.repo, gxx, &args, &stdout_p, &stderr_p) {
            Ok(true) => self.fail("shim.foreign-errno-refused", "the probe COMPILED -- the generated header's guard did not fire"),
            Ok(false) => {
                let err = std::fs::read_to_string(&stderr_p).unwrap_or_default();
                if !err.contains(want_err) {
                    echo_file(&stderr_p, "");
                    self.fail("shim.foreign-errno-refused", &format!("the probe refused for the WRONG reason (stderr lacks '{want_err}')"));
                }
            }
            Err(e) => self.fail("shim.foreign-errno-refused", &e),
        }
        self.results.emit(&format!(
            "{{\"schema\":1,\"type\":\"case\",\"gate\":\"{}\",\"case\":\"shim.foreign-errno-refused\",\"verdict\":\"pass\",\"exit\":null,\"exit_want\":null,\"control\":\"negative\",\"image\":null,\"broke\":\"the kit's errno.h included before the generated errno header\"}}",
            self.gate_id
        ));
        self.results.tally("pass", "negative");
        println!("== case shim.foreign-errno-refused: PASS (the generated errno header refused a foreign errno.h at compile with the designed message) ==");

        self.run_newlib_header_control(gxx);
        self.run_classlib_order_control(gxx);
        self.run_f769_shared_control(gxx);
        self.run_an505_row_control(gxx);
    }

    /// The compile-tier control for the F769 rider's shared objects: the
    /// sysnative and host translation units compiled under the F769 row
    /// selector must tie the frozen an500 objects apart from the producer
    /// stamp, through the leg's helper (neither
    /// consumes a field that differs between the rows), which is the fact
    /// that lets the rider link them from the an500 line; and the frozen
    /// F769 core object must differ from the an500 core (the core consumes
    /// the row's region), which is why the tier carries it as its own
    /// object rather than a second copy of the same bytes.
    fn run_f769_shared_control(&mut self, gxx: &ToolCmd) {
        const CASE: &str = "shim.f769-shared-ties-an500";
        println!("== case {CASE} (compile-tier control) ==");
        let facts = match rustc_facts() {
            Ok(f) => f,
            Err(e) => self.fail(CASE, &e),
        };
        let stripped = self.out.join("obj").join("stripped");
        for (src, frozen, tag) in [
            ("src/shim/rhu_sysnative.c", "shim-sysnative.o", "sysnative"),
            ("src/shim/rhu_host.c", "shim-host.o", "host"),
            ("src/shim/rhu_pal_surface_net11_rc1.c", "shim-pal-surface.o", "pal-surface"),
        ] {
            let out_rel = format!("src/kernel/target/shim-gate/obj/{tag}-f769-control.o");
            let out_abs = self.repo.join(out_rel.replace('/', std::path::MAIN_SEPARATOR_STR));
            let _ = std::fs::remove_file(&out_abs);
            self.compile(gxx, CASE, src, &["-DMGC_HOST_ROW_F769"], &out_rel, Class::CortexM7);
            let got = match sha256_without_stamp(&facts, &out_abs, &stripped.join(format!("f769-shared-{tag}.o"))) {
                Ok(s) => s,
                Err(e) => self.fail(CASE, &format!("{src} under the F769 selector produced no object, or its copy failed ({e})")),
            };
            let frozen_abs = self.repo.join("goldens").join("shim").join(frozen);
            let want = match sha256_without_stamp(&facts, &frozen_abs, &stripped.join(format!("f769-shared-frozen-{tag}.o"))) {
                Ok(s) => s,
                Err(e) => self.fail(CASE, &format!("goldens/shim/{frozen} unreadable, or its copy failed: {e}")),
            };
            if got != want {
                self.fail(CASE, &format!("{src} under the F769 selector compiled to {got}, not the frozen an500 {frozen} {want} (apart from the producer stamp): the rows no longer share this object, so the F769 image needs its own frozen copy"));
            }
            println!("   {frozen}: the compile under the F769 selector ties the frozen an500 object apart from the producer stamp ({})", &got[..16]);
        }
        let core = match sha256_without_stamp(&facts, &self.repo.join("goldens").join("shim").join("shim-core.o"), &stripped.join("f769-shared-core.o")) {
            Ok(s) => s,
            Err(e) => self.fail(CASE, &format!("goldens/shim/shim-core.o unreadable, or its copy failed: {e}")),
        };
        let core_f769 = match sha256_without_stamp(&facts, &self.repo.join("goldens").join("shim").join("shim-core-f769.o"), &stripped.join("f769-shared-core-f769.o")) {
            Ok(s) => s,
            Err(e) => self.fail(CASE, &format!("goldens/shim/shim-core-f769.o unreadable, or its copy failed: {e}")),
        };
        if core == core_f769 {
            self.fail(CASE, "shim-core-f769.o ties shim-core.o apart from the producer stamp: the F769 row moves nothing in the core object, so a separate frozen object is unfounded");
        }
        println!("   shim-core.o ({}) and shim-core-f769.o ({}) differ apart from the producer stamp: the row's region reaches the core, and the tier carries both", &core[..16], &core_f769[..16]);
        self.results.emit(&format!(
            "{{\"schema\":1,\"type\":\"case\",\"gate\":\"{}\",\"case\":\"{CASE}\",\"verdict\":\"pass\",\"exit\":null,\"exit_want\":null,\"control\":\"positive\",\"image\":null,\"stats\":{{\"shared\":2}}}}",
            self.gate_id
        ));
        self.results.tally("pass", "positive");
        println!("== case {CASE}: PASS (two objects shared with the an500 line tie under the F769 selector; the core object differs and is frozen for the row) ==");
    }

    /// The compile-tier control for the mps2-an505 row: its three
    /// row-consuming sources compiled under the an505 selector WITH THE
    /// CORTEX-M7 FLAG SET must tie the frozen an500 objects apart from the
    /// producer stamp (the row's fields equal the an500's and the shim
    /// consumes no field that differs), so the class flag set is the one
    /// variable between the an500 objects and the row's Cortex-M33 objects.
    /// A row edit that lets the an505 row diverge from the an500 is named
    /// here.
    fn run_an505_row_control(&mut self, gxx: &ToolCmd) {
        const CASE: &str = "shim.an505-row-ties-an500";
        println!("== case {CASE} (compile-tier control) ==");
        let facts = match rustc_facts() {
            Ok(f) => f,
            Err(e) => self.fail(CASE, &e),
        };
        let stripped = self.out.join("obj").join("stripped");
        for (src, frozen, tag) in [
            ("src/shim/rhu_shim.c", "shim-core.o", "core"),
            ("src/shim/rhu_sysnative.c", "shim-sysnative.o", "sysnative"),
            ("src/shim/rhu_host.c", "shim-host.o", "host"),
            ("src/shim/rhu_pal_surface_net11_rc1.c", "shim-pal-surface.o", "pal-surface"),
        ] {
            let out_rel = format!("src/kernel/target/shim-gate/obj/{tag}-an505-row-control.o");
            let out_abs = self.repo.join(out_rel.replace('/', std::path::MAIN_SEPARATOR_STR));
            let _ = std::fs::remove_file(&out_abs);
            self.compile(gxx, CASE, src, &["-DMGC_HOST_ROW_AN505"], &out_rel, Class::CortexM7);
            let got = match sha256_without_stamp(&facts, &out_abs, &stripped.join(format!("an505-row-{tag}.o"))) {
                Ok(s) => s,
                Err(e) => self.fail(CASE, &format!("{src} under the an505 selector produced no object, or its copy failed ({e})")),
            };
            let frozen_abs = self.repo.join("goldens").join("shim").join(frozen);
            let want = match sha256_without_stamp(&facts, &frozen_abs, &stripped.join(format!("an505-row-frozen-{tag}.o"))) {
                Ok(s) => s,
                Err(e) => self.fail(CASE, &format!("goldens/shim/{frozen} unreadable, or its copy failed: {e}")),
            };
            if got != want {
                self.fail(CASE, &format!("{src} under the an505 selector with the Cortex-M7 flag set compiled to {got}, not the frozen an500 {frozen} {want} (apart from the producer stamp): the an505 row no longer ties the an500 row, so the class flag set is no longer the one variable behind the row's Cortex-M33 objects"));
            }
            println!("   {frozen}: the compile under the an505 selector with the Cortex-M7 flag set ties the frozen an500 object apart from the producer stamp ({})", &got[..16]);
        }
        self.results.emit(&format!(
            "{{\"schema\":1,\"type\":\"case\",\"gate\":\"{}\",\"case\":\"{CASE}\",\"verdict\":\"pass\",\"exit\":null,\"exit_want\":null,\"control\":\"positive\",\"image\":null,\"stats\":{{\"shared\":3}}}}",
            self.gate_id
        ));
        self.results.tally("pass", "positive");
        println!("== case {CASE}: PASS (the three row-consuming sources under the an505 selector with the Cortex-M7 flag set tie the frozen an500 objects apart from the producer stamp: the class flag set is the one variable behind the row's Cortex-M33 objects) ==");
    }

    /// The compile-tier control for the generated C-library layout header
    /// (`src/shim/rhu_newlib_arm32.h`): the probe above, compiled with the
    /// kit's own headers under the nano configuration, must COMPILE against
    /// the genuine header and must be REFUSED against a copy with one value
    /// moved -- both sides, because a control that only accepts cannot be
    /// told from no control. The nano include directory comes from the
    /// compiler's own `-print-sysroot`, so no machine-local pointer is
    /// added; the doctored copy is written beside the probe, where a quoted
    /// include finds it first, and removed afterwards.
    fn run_newlib_header_control(&mut self, gxx: &ToolCmd) {
        const CASE: &str = "shim.newlib-header-ties-kit";
        println!("== case {CASE} (compile-tier control) ==");
        let sysroot = gxx.sysroot.clone();
        if sysroot.is_empty() {
            self.fail(CASE, "the resolver carries no C-library sysroot");
        }
        let nano = format!("{sysroot}/include/newlib-nano");
        let obj_dir = self.out.join("obj");
        let doctored = obj_dir.join("rhu_newlib_arm32.h");
        let _ = std::fs::remove_file(&doctored);
        let probe_rel = "src/kernel/target/shim-gate/obj/newlib-header-probe.c";
        write_fixture(&self.repo.join(probe_rel.replace('/', std::path::MAIN_SEPARATOR_STR)), NEWLIB_PROBE_SOURCE);
        let out_rel = "src/kernel/target/shim-gate/obj/newlib-header-probe.o";
        self.compile(gxx, CASE, probe_rel, &["-isystem", &nano], out_rel, Class::CortexM7);
        println!("   (a) the generated header compiles against the kit's own headers under the nano configuration");

        let genuine = std::fs::read_to_string(self.repo.join("src").join("shim").join("rhu_newlib_arm32.h")).unwrap_or_default();
        if !genuine.contains(NEWLIB_DOCTOR_FROM) {
            self.fail(CASE, "the doctored copy cannot be made: the line-buffered value is not where the control expects it");
        }
        let moved = genuine.replace(NEWLIB_DOCTOR_FROM, NEWLIB_DOCTOR_TO);
        write_fixture(&doctored, moved.as_bytes());
        let stdout_p = self.out.join("newlib-header-doctored.cc.stdout");
        let stderr_p = self.out.join("newlib-header-doctored.cc.stderr");
        let mut args: Vec<String> = CC_FLAGS.iter().map(|s| s.to_string()).collect();
        args.push("-isystem".to_string());
        args.push(nano.clone());
        args.push(probe_rel.to_string());
        args.push("-o".to_string());
        args.push("src/kernel/target/shim-gate/obj/newlib-header-doctored.o".to_string());
        let want_err = "the line-buffered flag ties the kit";
        let outcome = run_tool_bounded(&self.repo, gxx, &args, &stdout_p, &stderr_p);
        let _ = std::fs::remove_file(&doctored);
        match outcome {
            Ok(true) => self.fail(CASE, "the doctored header COMPILED -- the assertion did not bite on a moved value"),
            Ok(false) => {
                let err = std::fs::read_to_string(&stderr_p).unwrap_or_default();
                if !err.contains(want_err) {
                    echo_file(&stderr_p, "");
                    self.fail(CASE, &format!("the doctored header refused for the WRONG reason (stderr lacks '{want_err}')"));
                }
            }
            Err(e) => self.fail(CASE, &e),
        }
        println!("   (b) a copy with the line-buffered flag moved to the string-stream value is refused at the assertion that names it");
        self.results.emit(&format!(
            "{{\"schema\":1,\"type\":\"case\",\"gate\":\"{}\",\"case\":\"{CASE}\",\"verdict\":\"pass\",\"exit\":null,\"exit_want\":null,\"control\":\"negative\",\"image\":null,\"broke\":\"one value of the generated C-library layout header moved\"}}",
            self.gate_id
        ));
        self.results.tally("pass", "negative");
        println!("== case {CASE}: PASS (the generated C-library layout header ties the kit's own headers; a moved value refused) ==");
    }

    /// The compile-tier control for the class-library table's order: the
    /// host itself, compiled as a copy placed where a header written beside
    /// it is what its quoted include finds first. Both halves, because a
    /// refusal alone cannot be told from a compile that fails for another
    /// reason: (a) with nothing beside it the copy reads the genuine
    /// generated header through the include path, compiles, and its object
    /// ties the frozen host object apart from the producer stamp -- the
    /// copy IS the host;
    /// (b) with a copy of the generated runtime-entry header beside it in
    /// which one slot is shifted, the same compile must be REFUSED at the
    /// assertion that names that slot. The doctored copy is removed
    /// afterwards.
    fn run_classlib_order_control(&mut self, gxx: &ToolCmd) {
        const CASE: &str = "shim.classlib-order-ties-pin";
        println!("== case {CASE} (compile-tier control) ==");
        let facts = match rustc_facts() {
            Ok(f) => f,
            Err(e) => self.fail(CASE, &e),
        };
        let stripped = self.out.join("obj").join("stripped");
        let dir = self.out.join("obj").join("classlib-order");
        if std::fs::create_dir_all(&dir).is_err() {
            self.fail(CASE, "cannot create the control's directory");
        }
        let doctored = dir.join("rhu_rt_entry_net11_rc1.h");
        let _ = std::fs::remove_file(&doctored);
        let host = match std::fs::read(self.repo.join("src").join("shim").join("rhu_host.c")) {
            Ok(b) => b,
            Err(e) => self.fail(CASE, &format!("src/shim/rhu_host.c unreadable: {e}")),
        };
        let copy_rel = "src/kernel/target/shim-gate/obj/classlib-order/rhu_host.c";
        write_fixture(&self.repo.join(copy_rel.replace('/', std::path::MAIN_SEPARATOR_STR)), &host);
        let out_rel = "src/kernel/target/shim-gate/obj/classlib-order/rhu_host.o";
        self.compile(gxx, CASE, copy_rel, &["-DMGC_HOST_ROW_AN500"], out_rel, Class::CortexM7);
        let got = match sha256_without_stamp(&facts, &self.repo.join(out_rel.replace('/', std::path::MAIN_SEPARATOR_STR)), &stripped.join("classlib-order-host.o")) {
            Ok(s) => s,
            Err(e) => self.fail(CASE, &format!("the copy's compile produced no object, or its copy failed ({e})")),
        };
        let want = match sha256_without_stamp(&facts, &self.repo.join("goldens").join("shim").join("shim-host.o"), &stripped.join("classlib-order-frozen-host.o")) {
            Ok(s) => s,
            Err(e) => self.fail(CASE, &format!("goldens/shim/shim-host.o unreadable, or its copy failed: {e}")),
        };
        if got != want {
            self.fail(CASE, &format!("the host's copy compiled to {got}, not the frozen host object {want} (apart from the producer stamp): the copy is not the host, so a refusal would prove nothing about it"));
        }
        println!("   (a) the host's copy compiles against the genuine generated header and ties the frozen host object apart from the producer stamp ({})", &got[..16]);

        let genuine = std::fs::read_to_string(self.repo.join("src").join("shim").join("rhu_rt_entry_net11_rc1.h")).unwrap_or_default();
        if !genuine.contains(RT_ENTRY_DOCTOR_FROM) {
            self.fail(CASE, "the doctored copy cannot be made: the unhandled-exception slot is not where the control expects it");
        }
        let moved = genuine.replace(RT_ENTRY_DOCTOR_FROM, RT_ENTRY_DOCTOR_TO);
        write_fixture(&doctored, moved.as_bytes());
        let stdout_p = self.out.join("classlib-order-doctored.cc.stdout");
        let stderr_p = self.out.join("classlib-order-doctored.cc.stderr");
        let mut args: Vec<String> = CC_FLAGS.iter().map(|s| s.to_string()).collect();
        args.push("-DMGC_HOST_ROW_AN500".to_string());
        args.push(copy_rel.to_string());
        args.push("-o".to_string());
        args.push("src/kernel/target/shim-gate/obj/classlib-order/rhu_host-doctored.o".to_string());
        let want_err = "the unhandled-exception slot ties the pin";
        let outcome = run_tool_bounded(&self.repo, gxx, &args, &stdout_p, &stderr_p);
        let _ = std::fs::remove_file(&doctored);
        match outcome {
            Ok(true) => self.fail(CASE, "the host COMPILED against a header with a shifted slot -- the shape assertion did not bite"),
            Ok(false) => {
                let err = std::fs::read_to_string(&stderr_p).unwrap_or_default();
                if !err.contains(want_err) {
                    echo_file(&stderr_p, "");
                    self.fail(CASE, &format!("the shifted header refused for the WRONG reason (stderr lacks '{want_err}')"));
                }
            }
            Err(e) => self.fail(CASE, &e),
        }
        println!("   (b) a copy of the generated runtime-entry header with the unhandled-exception slot shifted by one is refused at the assertion that names it");
        self.results.emit(&format!(
            "{{\"schema\":1,\"type\":\"case\",\"gate\":\"{}\",\"case\":\"{CASE}\",\"verdict\":\"pass\",\"exit\":null,\"exit_want\":null,\"control\":\"negative\",\"image\":null,\"broke\":\"one class-library slot of the generated runtime-entry header shifted\"}}",
            self.gate_id
        ));
        self.results.tally("pass", "negative");
        println!("== case {CASE}: PASS (the host pins the class-library table's shape against the generated header; a shifted slot refused) ==");
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
    let out = repo.join("src").join("kernel").join("target").join("shim-gate");
    if std::fs::create_dir_all(&out).is_err() {
        println!("rhu-gate: cannot create {}", out.display());
        exit(1);
    }
    let results = match Results::create(&out.join("shim.results.jsonl")) {
        Ok(r) => r,
        Err(e) => {
            println!("rhu-gate: cannot create results file: {e}");
            exit(1);
        }
    };
    let mut shim = Shim { repo: repo.to_path_buf(), out, results, gate_id: GATE, display: DISPLAY, records: Vec::new() };
    shim.results.emit(&format!(
        "{{\"schema\":1,\"type\":\"run\",\"gate\":\"shim\",\"run\":\"shim-{}\",\"host\":\"{HOST}\",\"runner\":\"{RUNNER}\",\"chain\":{},\"tools\":{{\"qemu\":\"{}\",\"rustc\":\"{}\",\"ld\":\"{}\"}}}}",
        std::process::id(),
        chain_json_from_env(),
        qemu_v,
        facts.version,
        ld.version
    ));
    shim.verify_frozen();
    shim.build_staticlib();
    shim.run_core(&facts, &ld, &qemu);
    shim.run_fdmap(&facts, &ld, &qemu);
    shim.run_activation(&facts, &ld, &qemu);
    shim.run_tls(&facts, &ld, &qemu);
    shim.run_host(&facts, &ld, &qemu);
    shim.run_fp_census(&facts);
    shim.run_pal_surface(&facts);
    for p in PENDING {
        println!("   pending (unarmed, not counted): {p}");
    }
    for c in INTERNAL_CASES {
        println!("   internal (rederive leg, not counted): {c}");
    }
    shim.results.emit_summary(GATE, "pass", 0);
    if !validate_results(repo, shim.results.path()) {
        println!("{DISPLAY}: FAIL -- results file failed schema validation");
        exit(1);
    }
    println!(
        "{DISPLAY}: PASS (25/25 cases incl. 17 negative controls; {} pending unarmed; {} internal on the rederive leg)  [{} | {} | {}]",
        PENDING.len(),
        INTERNAL_CASES.len(),
        qemu_v,
        facts.version,
        ld.version
    );
    exit(0);
}

/// The maintainer's re-derivation leg: the only place the C compiler is needed.
/// Recompiles the frozen plan, byte-asserts it against goldens/shim apart
/// from the producer stamp each object carries (a frozen object stating
/// another hosting-API contract generation than include/rhu.h is named
/// before its byte comparison), then runs the compile-tier controls. Needs the compiler of record (clang
/// 22.1.8, resolved by RHU_CLANG or tools/toolchains/clang.local);
/// consumers never need it -- the chain never calls this.
pub fn rederive(repo: &Path) -> ! {
    let gxx = match resolve_clang(repo) {
        Ok(g) => g,
        Err(e) => {
            println!("rhu-gate: {e}");
            exit(1);
        }
    };
    let out = repo.join("src").join("kernel").join("target").join("shim-gate");
    if std::fs::create_dir_all(&out).is_err() {
        println!("rhu-gate: cannot create {}", out.display());
        exit(1);
    }
    let results = match Results::create(&out.join("shim-rederive.results.jsonl")) {
        Ok(r) => r,
        Err(e) => {
            println!("rhu-gate: cannot create results file: {e}");
            exit(1);
        }
    };
    let mut shim = Shim { repo: repo.to_path_buf(), out, results, gate_id: "shim-rederive", display: "SHIM REDERIVE", records: Vec::new() };
    shim.results.emit(&format!(
        "{{\"schema\":1,\"type\":\"run\",\"gate\":\"shim-rederive\",\"run\":\"shim-rederive-{}\",\"host\":\"compile-tier\",\"runner\":\"{RUNNER}\",\"tools\":{{\"clang\":\"{}\"}}}}",
        std::process::id(),
        gxx.version
    ));
    let mpath = repo.join("goldens").join("shim").join("MANIFEST.md");
    match sha256_file(&mpath) {
        Ok(s) if s == SHA_FROZEN_MANIFEST => {}
        Ok(s) => {
            println!("SHIM REDERIVE: FAIL -- goldens/shim/MANIFEST.md sha256 {s} != pinned {SHA_FROZEN_MANIFEST}");
            exit(1);
        }
        Err(e) => {
            println!("SHIM REDERIVE: FAIL -- goldens/shim/MANIFEST.md unreadable: {e}");
            exit(1);
        }
    }
    shim.run_rederive(&gxx);
    match rustc_facts() {
        Ok(facts) => shim.run_fp_census_instrument_control(&gxx, &facts),
        Err(e) => {
            println!("SHIM REDERIVE: FAIL -- {e}");
            exit(1);
        }
    }
    shim.build_staticlib();
    let retarget = match (resolve_newlib_dir(repo), rustc_facts()) {
        (Some(dir), Ok(facts)) => match resolve_ld(&facts) {
            Ok(ld) => {
                shim.run_newlib_retarget(&gxx, &facts, &ld, &dir);
                "run"
            }
            Err(e) => {
                println!("   declared (not run): shim.newlib-retarget -- {e}");
                "declared"
            }
        },
        (None, _) => {
            println!("   declared (not run): shim.newlib-retarget -- the toolchain's C library is not resolved (set RHU_NEWLIB_M7 or write tools/toolchains/newlib-m7.local)");
            "declared"
        }
        (_, Err(e)) => {
            println!("   declared (not run): shim.newlib-retarget -- {e}");
            "declared"
        }
    };
    let survey = match (resolve_netpack_dir(repo), rustc_facts()) {
        (Some(dir), Ok(facts)) => {
            shim.run_pal_surface_survey(&facts, &dir);
            "run"
        }
        (None, _) => {
            println!("   declared (not run): shim.pal-surface-survey -- the pinned runtime pack's native directory is not resolved (set RHU_NET_PACK or write tools/toolchains/netpack.local)");
            "declared"
        }
        (_, Err(e)) => {
            println!("   declared (not run): shim.pal-surface-survey -- {e}");
            "declared"
        }
    };
    shim.results.emit_summary("shim-rederive", "pass", 0);
    if !validate_results(repo, shim.results.path()) {
        println!("SHIM REDERIVE: FAIL -- results file failed schema validation");
        exit(1);
    }
    println!(
        "SHIM REDERIVE: PASS ({}/{} frozen objects re-derived byte-identical apart from the producer stamp; foreign-errno control refused at compile; C-library layout header ties the kit; class-library order ties the pin; the F769 row's shared objects tie the an500 bytes; the an505 row's objects tie the an500 bytes under the Cortex-M7 flag set apart from the producer stamp; the floating-point census instrument counts a compiled double multiply-add; C-library retarget {}); the surface survey {})  [{}]",
        FROZEN_PLAN.len(),
        FROZEN_PLAN.len(),
        retarget,
        survey,
        gxx.version
    );
    exit(0);
}

/// Negative-control the shim gate's checkers: the manifest pin and record
/// consistency, the frozen-object and golden compares, the import-closure
/// pin, the one-strong-symbol sweep, the forbidden-family sweep, the
/// failure detector, the exit-code and verdict-line checks -- each must
/// REJECT its doctored input, with the accept side pinned on genuine
/// inputs.
/// A frozen input of the selfcheck's own probes, read whole from the shim
/// tier -- or a loud refusal naming the file. The probes doctor REAL frozen
/// bytes; an empty default in their place would let a probe report the
/// checker broken when it is the tier that is absent.
fn selfcheck_frozen(repo: &Path, name: &str) -> Vec<u8> {
    match std::fs::read(repo.join("goldens").join("shim").join(name)) {
        Ok(b) => b,
        Err(e) => {
            println!("rhu-gate shim selfcheck: goldens/shim/{name} unreadable: {e}");
            exit(1);
        }
    }
}

pub fn selfcheck(repo: &Path) -> ! {
    let facts = match rustc_facts() {
        Ok(f) => f,
        Err(e) => {
            println!("rhu-gate: {e}");
            exit(1);
        }
    };
    let rustc_v = facts.version.clone();
    if !repo.join("goldens").join("shim").join("MANIFEST.md").is_file() {
        println!("rhu-gate shim selfcheck: frozen inputs missing (goldens/shim)");
        exit(1);
    }
    let out = repo.join("src").join("kernel").join("target").join("shim-gate");
    if std::fs::create_dir_all(&out).is_err() {
        println!("rhu-gate: cannot create {}", out.display());
        exit(1);
    }
    let results = match Results::create(&out.join("shim-selfcheck.results.jsonl")) {
        Ok(r) => r,
        Err(e) => {
            println!("rhu-gate: cannot create results file: {e}");
            exit(1);
        }
    };
    let mut sc = Selfcheck {
        repo: repo.to_path_buf(),
        gate: "shim-selfcheck".to_string(),
        display: "SHIM SELFCHECK".to_string(),
        results,
        n: 0,
        npass: 0,
        nneg: 0,
    };
    sc.results.emit(&format!(
        "{{\"schema\":1,\"type\":\"run\",\"gate\":\"shim-selfcheck\",\"run\":\"shim-selfcheck-{}\",\"host\":\"{HOST}\",\"runner\":\"{RUNNER}\",\"tools\":{{\"rustc\":\"{}\"}}}}",
        std::process::id(),
        rustc_v
    ));

    let ordered: Vec<u8> = HOST_MARKERS.join("\r\n").into_bytes();
    sc.accept_probe("host-order-genuine", "a capture carrying the contract's markers in order", markers_out_of_order(&ordered, &HOST_MARKERS).is_none());
    let mut noise: Vec<u8> = b"rhu 0.2.0 -- banner\r\n[boot] a kernel line\r\n".to_vec();
    noise.extend_from_slice(&ordered);
    noise.extend_from_slice(b"\r\n[payload] managed Main returned 0\r\n");
    sc.accept_probe("host-order-in-noise", "the markers in order inside the surrounding kernel lines", markers_out_of_order(&noise, &HOST_MARKERS).is_none());
    let mut swapped: Vec<&str> = HOST_MARKERS.to_vec();
    swapped.swap(1, 4);
    let swapped_capture: Vec<u8> = swapped.join("\r\n").into_bytes();
    sc.probe("host-order-swapped", "every marker present, two of them in the wrong order", markers_out_of_order(&swapped_capture, &HOST_MARKERS).is_none());
    let dropped: Vec<u8> = HOST_MARKERS[1..].join("\r\n").into_bytes();
    sc.probe("host-order-missing", "a capture whose first contract marker never printed", markers_out_of_order(&dropped, &HOST_MARKERS).is_none());
    let once: Vec<u8> = HOST_MARKERS[0].as_bytes().to_vec();
    sc.probe("host-order-truncated", "a capture that stops after the first marker", markers_out_of_order(&once, &HOST_MARKERS).is_none());
    let fault = markers_out_of_order(&swapped_capture, &HOST_MARKERS).unwrap_or_default();
    sc.accept_probe("host-order-names-the-kind", "the out-of-order complaint says the marker appears, rather than that it is absent", fault.contains("before a marker that must precede it"));

    sc.accept_probe("tls-span-empty", "an empty bracket pair where no template is wanted", template_span_fault(Some(0x2000_1000), Some(0x2000_1000), NO_TEMPLATE).is_none());
    sc.accept_probe("tls-span-eight", "an eight-byte bracket pair where the template is wanted", template_span_fault(Some(0x2000_1000), Some(0x2000_1008), TLS_TEMPLATE_SPAN).is_none());
    sc.probe("tls-span-unwanted", "an eight-byte bracket pair in an image that links no template", template_span_fault(Some(0x2000_1000), Some(0x2000_1008), NO_TEMPLATE).is_none());
    sc.probe("tls-span-missing", "an empty bracket pair in the image whose template must span", template_span_fault(Some(0x2000_1000), Some(0x2000_1000), TLS_TEMPLATE_SPAN).is_none());
    sc.probe("tls-span-short", "a four-byte bracket pair where eight are wanted", template_span_fault(Some(0x2000_1000), Some(0x2000_1004), TLS_TEMPLATE_SPAN).is_none());
    sc.probe("tls-span-inverted", "a bracket pair whose low end is above its high end", template_span_fault(Some(0x2000_1008), Some(0x2000_1000), NO_TEMPLATE).is_none());
    sc.probe("tls-span-unparsed", "a dump the bracket pair cannot be read from", template_span_fault(None, Some(0x2000_1008), TLS_TEMPLATE_SPAN).is_none());

    let mpath = repo.join("goldens").join("shim").join("MANIFEST.md");
    let msha = match sha256_file(&mpath) {
        Ok(s) => s,
        Err(e) => {
            println!("rhu-gate shim selfcheck: goldens/shim/MANIFEST.md unreadable: {e}");
            exit(1);
        }
    };
    sc.accept_probe("manifest-pin", "goldens/shim/MANIFEST.md ties the in-code pin", msha == SHA_FROZEN_MANIFEST);
    let doctored_pin = format!("{}{}", &SHA_FROZEN_MANIFEST[..63], if SHA_FROZEN_MANIFEST.ends_with('0') { "1" } else { "0" });
    sc.probe("manifest-pin-doctored", "manifest sha compared against a pin with one flipped digit", msha == doctored_pin);

    let text = String::from_utf8_lossy(&selfcheck_frozen(repo, "MANIFEST.md")).into_owned();
    let records = parse_vendored_manifest(&text);
    let on_disk: Vec<String> = FROZEN_PLAN.iter().map(|(n, _, _, _)| n.to_string()).collect();
    sc.accept_probe("records-genuine", "the manifest's records tie the plan and goldens both directions", frozen_consistency(&records, &on_disk).is_ok());
    let mut outside = records.clone();
    if let Some(r) = outside.first_mut() {
        r.0 = "goldens/other/shim-core.o".to_string();
    }
    sc.probe("record-outside-tier", "a manifest record naming a path outside goldens/shim", frozen_consistency(&outside, &on_disk).is_ok());
    let mut extra = on_disk.clone();
    extra.push("stray.o".to_string());
    sc.probe("object-unrecorded", "an object on disk with no manifest record", frozen_consistency(&records, &extra).is_ok());

    let want = records.iter().find(|(p, _)| p == "goldens/shim/shim-core.o").map(|(_, s)| s.clone()).unwrap_or_default();
    let genuine = selfcheck_frozen(repo, "shim-core.o");
    let mut doctored = genuine.clone();
    if let Some(b) = doctored.get_mut(64) {
        *b ^= 0x01;
    }
    let dpath = out.join("doctored-shim-core.o");
    write_fixture(&dpath, &doctored);
    sc.accept_probe("object-genuine", "the frozen core object ties its record", sha256_file(&repo.join("goldens").join("shim").join("shim-core.o")).map(|s| s == want).unwrap_or(false));
    sc.probe("object-doctored", "frozen object with one flipped byte", sha256_file(&dpath).map(|s| s == want).unwrap_or(false));

    let golden = selfcheck_frozen(repo, GOLDENS[0]);
    let mut wrapped: Vec<u8> = b"rhu 0.2.0 -- kernel banner\r\n[boot] kernel line\r\n".to_vec();
    wrapped.extend_from_slice(&golden);
    wrapped.extend_from_slice(b"[payload] managed Main returned 0\r\n");
    sc.accept_probe("golden-genuine", "the genuine payload stream inside kernel lines equals the golden", eq_lf_normalized(&payload_stream(&wrapped), &golden));
    let flipped: Vec<u8> = String::from_utf8_lossy(&wrapped).replace("ALL GOOD", "ALL G00D").into_bytes();
    sc.probe("golden-flipped", "payload stream with the verdict line's GOOD changed to G00D", eq_lf_normalized(&payload_stream(&flipped), &golden));

    let act_golden = selfcheck_frozen(repo, GOLDENS[2]);
    sc.accept_probe("activation-verdict-last", "the activation golden ends on its verdict line", String::from_utf8_lossy(&act_golden).lines().last() == Some(ACT_ALL_GOOD));
    let act_flipped: Vec<u8> = String::from_utf8_lossy(&act_golden).replace("ALL GOOD", "ALL G00D").into_bytes();
    sc.probe("activation-golden-flipped", "the activation golden with its verdict line's GOOD changed to G00D", eq_lf_normalized(&act_flipped, &act_golden));

    let mut pinned: Vec<String> = IMPORTS_GLUE.iter().map(|s| s.to_string()).collect();
    pinned.sort();
    sc.accept_probe("closure-genuine", "the pinned glue closure accepted exactly", imports_match(&pinned, &IMPORTS_GLUE));
    let mut extra_name = pinned.clone();
    extra_name.push("__emutls_get_address".to_string());
    extra_name.sort();
    sc.probe("closure-extra", "a closure with one extra import", imports_match(&extra_name, &IMPORTS_GLUE));
    let missing: Vec<String> = pinned.iter().skip(1).cloned().collect();
    sc.probe("closure-missing", "a closure with one import vanished", imports_match(&missing, &IMPORTS_GLUE));

    sc.accept_probe("forbidden-clean", "the pinned closure carries no forbidden family", forbidden_import(&pinned).is_none());
    sc.probe("forbidden-emutls", "a closure importing __emutls_get_address", forbidden_import(&extra_name).is_none());

    let clean_a = "00000000 T malloc\n00000010 T free\n00000000 W weak_one\n";
    let clean_b = "00000000 T pthread_self\n00000000 W weak_one\n";
    let dup_b = "00000000 T malloc\n00000000 T pthread_self\n";
    sc.accept_probe("strong-clean", "two objects with disjoint strong definitions (a shared weak allowed)", duplicate_definitions(&[clean_a, clean_b]).is_empty());
    sc.probe("strong-duplicate", "two objects both defining malloc strongly", duplicate_definitions(&[clean_a, dup_b]).is_empty());

    let imports = vec![vec!["rhu_now_ns".to_string(), "malloc".to_string()]];
    let defined = vec![vec!["malloc".to_string()]];
    let lib = vec!["rhu_now_ns".to_string()];
    sc.accept_probe("resolve-clean", "imports defined by the objects or the staticlib resolve", unresolved_imports(&imports, &defined, &lib).is_empty());
    let orphan = vec![vec!["rhu_now_ns".to_string(), "malloc_usable_size".to_string()]];
    sc.probe("resolve-orphan", "an import that neither the objects nor the staticlib define", unresolved_imports(&orphan, &defined, &lib).is_empty());
    let libc_group = vec![vec!["rhu_now_ns".to_string(), "vfscanf".to_string()]];
    sc.accept_probe("resolve-libc-group", "the stream bridge's C-library parser, in its named group, accepted as resolved at the hosted runtime's link", unresolved_imports(&libc_group, &defined, &lib).is_empty());
    let libc_stray = vec![vec!["rhu_now_ns".to_string(), "fprintf".to_string()]];
    sc.probe("resolve-libc-stray", "a C-library name outside the named group", unresolved_imports(&libc_stray, &defined, &lib).is_empty());

    let lib_def: Vec<String> = ["__sf", "_impure_data", "_impure_ptr", "_malloc_r", "_write"].iter().map(|s| s.to_string()).collect();
    let bridge_added: Vec<String> = ["__sf", "_impure_data", "_impure_ptr"].iter().map(|s| s.to_string()).collect();
    sc.accept_probe("retarget-added-genuine", "the library's reentrancy record and stream objects added through the bridge", retarget_added_fault(&bridge_added, &[], &lib_def).is_none());
    sc.probe("retarget-added-foreign", "an added definition the library does not define", retarget_added_fault(&["stray_symbol".to_string()], &[], &lib_def).is_none());
    sc.probe("retarget-added-allocator", "the library's own allocator entry arriving", retarget_added_fault(&["_malloc_r".to_string()], &[], &lib_def).is_none());
    sc.probe("retarget-removed", "a definition of the image's own vanishing when the library joins", retarget_added_fault(&[], &["rhu_host_main".to_string()], &lib_def).is_none());

    let failing: &[u8] = b"shimtest: ladder\r\nPASS shim-init (x)\r\nFAIL shim-errno-thread (errno leaked across threads)\r\n[payload] managed Main returned 1\r\n";
    sc.accept_probe("fail-absent", "the genuine golden carries no FAIL line", !contains_fixed(&golden, "FAIL "));
    sc.probe("fail-present", "a transcript carrying a FAIL line", !contains_fixed(failing, "FAIL "));
    sc.accept_probe("verdict-last", "the genuine golden ends on the verdict line", String::from_utf8_lossy(&golden).lines().last() == Some(CORE_ALL_GOOD));
    sc.probe("verdict-missing", "a broken transcript without the verdict line", contains_fixed(failing, CORE_ALL_GOOD));
    sc.probe("exit-timeout", "a run recorded as exit 124 (a hang) offered as exit 0", 124 == 0);

    sc.accept_probe(
        "fp-census-counts-vfp",
        "a disassembly line with a VFP mnemonic is counted once",
        fp_instruction_count("      56: ed95 7b0a    \tvldr\td7, [r5, #40]\n      5a: 68a5         \tldr\tr5, [r4, #0x8]\n") == 1,
    );
    sc.accept_probe(
        "fp-census-ignores-integer",
        "a disassembly with no VFP mnemonic counts zero",
        fp_instruction_count("      54: 6a23         \tldr\tr3, [r4, #0x20]\n      58: 4607         \tmov\tr7, r0\n") == 0,
    );
    sc.probe(
        "fp-census-moved-count",
        "a census reading one instruction where the pin says zero offered as at the pin",
        fp_census_fault(&[("shim-core.o".to_string(), 1)], &[("shim-core.o", 0)]).is_none(),
    );

    let header_text = std::fs::read_to_string(repo.join("src").join("kernel").join("include").join("rhu.h")).unwrap_or_default();
    let abi = header_abi(&header_text).unwrap_or(0);
    let compat = header_abi_compat(&header_text).unwrap_or(0);
    let host_path = repo.join("goldens").join("shim").join("shim-host.o");
    let host_obj = selfcheck_frozen(repo, "shim-host.o");
    let stated = stated_generation(&host_obj);
    sc.accept_probe(
        "generation-genuine",
        "the frozen host object states a contract generation inside include/rhu.h's window (RHU_ABI_COMPAT through RHU_ABI); the leg's line fires exactly when it is not the header's own",
        stated.map(|(g, _)| compat != 0 && abi != 0 && compat <= g && g <= abi) == Some(true)
            && generation_fault("shim-host.o", &host_obj, abi).is_none() == (stated.map(|(g, _)| g) == Some(abi)),
    );
    let mut moved = host_obj.clone();
    if let Some((_, at)) = stated {
        moved[at..at + 4].copy_from_slice(&abi.wrapping_sub(1).to_le_bytes());
    }
    let fault = generation_fault("shim-host.o", &moved, abi).unwrap_or_default();
    sc.probe("generation-moved", "a copy of the frozen host object stating the previous generation", fault.is_empty());
    sc.accept_probe(
        "generation-names-both",
        "the diagnosis names the stated generation and the header's",
        fault.contains(&format!("states hosting-API contract generation {}", abi.wrapping_sub(1))) && fault.contains(&format!("include/rhu.h is at {abi}")),
    );
    sc.accept_probe(
        "generation-absent",
        "an object stating no generation (the core object) is not diagnosed",
        stated_generation(&genuine).is_none() && generation_fault("shim-core.o", &genuine, abi).is_none(),
    );

    let host_an505 = selfcheck_frozen(repo, "shim-host-an505.o");
    let stated_an505 = stated_generation(&host_an505).map(|(g, _)| g);
    sc.accept_probe(
        "generation-genuine-an505",
        "the frozen an505 host object states a contract generation inside include/rhu.h's window, the one the an500 host object states; the leg's line fires exactly when it is not the header's own",
        stated_an505.map(|g| compat != 0 && abi != 0 && compat <= g && g <= abi) == Some(true)
            && stated_an505 == stated.map(|(g, _)| g)
            && generation_fault("shim-host-an505.o", &host_an505, abi).is_none() == (stated_an505 == Some(abi)),
    );
    sc.accept_probe(
        "class-flags-two-words",
        "the Cortex-M33 flag set is the Cortex-M7 set apart from the class's two words",
        class_flag_sets_tie(&CC_FLAGS, &CC_FLAGS_CORTEX_M33).is_ok(),
    );
    let mut doctored: Vec<&str> = CC_FLAGS_CORTEX_M33.to_vec();
    if let Some(slot) = doctored.iter().position(|f| *f == "-O2") {
        doctored[slot] = "-O1";
    }
    sc.probe("class-flags-doctored", "the Cortex-M33 flag set with a third word moved offered as the class variant", class_flag_sets_tie(&CC_FLAGS, &doctored).is_ok());

    let stripped = out.join("stripped");
    let stamp_base = sha256_without_stamp(&facts, &host_path, &stripped.join("selfcheck-genuine.o")).unwrap_or_default();
    let code_at = section_span_prefixed(&host_obj, ".text").map(|(o, _)| o);
    let mut code_moved = host_obj.clone();
    if let Some(at) = code_at {
        if let Some(b) = code_moved.get_mut(at) {
            *b ^= 0x01;
        }
    }
    write_fixture(&out.join("doctored-host-code.o"), &code_moved);
    let code_sha = sha256_without_stamp(&facts, &out.join("doctored-host-code.o"), &stripped.join("selfcheck-code-moved.o")).unwrap_or_default();
    sc.probe(
        "stamp-compare-code-byte",
        "a copy of the frozen host object with one code byte moved, compared apart from the stamp",
        code_at.is_none() || stamp_base.is_empty() || code_sha.is_empty() || code_sha == stamp_base,
    );
    let stamp_at = section_span(&host_obj, ".comment");
    let mut stamp_moved = host_obj.clone();
    if let Some((o, s)) = stamp_at {
        if s > 2 {
            if let Some(b) = stamp_moved.get_mut(o + 1) {
                *b ^= 0x01;
            }
        }
    }
    write_fixture(&out.join("doctored-host-stamp.o"), &stamp_moved);
    let stamp_sha = sha256_without_stamp(&facts, &out.join("doctored-host-stamp.o"), &stripped.join("selfcheck-stamp-moved.o")).unwrap_or_default();
    sc.accept_probe(
        "stamp-compare-stamp-only",
        "a copy of the frozen host object whose producer stamp alone is changed ties apart from the stamp while the files differ",
        stamp_at.is_some() && !stamp_base.is_empty() && stamp_sha == stamp_base && sha256_hex(&stamp_moved) != sha256_hex(&host_obj),
    );

    let surface_text = String::from_utf8_lossy(&selfcheck_frozen(repo, "pal-surface.txt")).into_owned();
    let surface = parse_surface_expectation(&surface_text);
    sc.accept_probe("surface-expectation-genuine", "goldens/shim/pal-surface.txt parses: a pin, five archives, the family names", surface.is_ok());
    let names: Vec<String> = surface.as_ref().map(|s| s.names.clone()).unwrap_or_default();
    let pin: String = surface.as_ref().map(|s| s.pin.clone()).unwrap_or_default();
    let mut with_own: Vec<String> = names.clone();
    with_own.push("rhu_shim_init".to_string());
    sc.accept_probe("surface-tie-genuine", "the expectation's names plus the shim's own names accepted as the surface exactly", surface_fault(&with_own, &names).is_none());
    let missing: Vec<String> = names.iter().skip(1).cloned().collect();
    sc.probe("surface-tie-missing", "a defined set lacking one expected name", surface_fault(&missing, &names).is_none());
    let mut foreign: Vec<String> = names.clone();
    foreign.push("SystemNative_NotInThePack".to_string());
    sc.probe("surface-tie-foreign", "a defined family name the pack's tables do not export", surface_fault(&foreign, &names).is_none());
    sc.accept_probe("surface-pin-genuine", "the expectation's pin as the SDK's, the header's tag as v + the pin", surface_pin_fault(&pin, &pin, &format!("v{pin}")).is_none());
    sc.probe("surface-pin-moved", "the SDK re-pinned to another version", surface_pin_fault(&pin, "11.0.0-rc.2.00000.0", &format!("v{pin}")).is_none());
    sc.probe("surface-header-tag-apart", "a surface header generated at another tag", surface_pin_fault(&pin, &pin, "v11.0.0-rc.2.00000.0").is_none());
    let doctored_text = surface_text.replacen("\npin: ", "\npin-was: ", 1);
    sc.probe("surface-expectation-doctored", "an expectation file whose pin line is renamed", parse_surface_expectation(&doctored_text).is_ok());
    let dump = "00000000 T SystemNative_Probe\n0000000c b SystemNative_Probe.initialized\n         U __errno_location\n";
    sc.accept_probe("surface-external-names", "an external text definition read as an export, its local static and an undefined name not", external_names(dump) == vec!["SystemNative_Probe".to_string()]);
    sc.probe("surface-local-symbol-excluded", "a function's static variable, a local symbol with a family prefix, counted as an export", external_names(dump).len() == 2);

    sc.finish("genuine inputs accepted");
}
