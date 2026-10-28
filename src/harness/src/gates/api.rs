//! The api gate on the runner: hosting-API conformance on QEMU
//! mps2-an500, cumulative.
//!
//!
//!
//!
//!

use crate::gate::{class_flag_sets_tie, write_fixture, CaseRun, Class, GateCtx, Selfcheck, Staged};
use crate::gates::gc::{resolve_clang, run_tool_bounded, ToolCmd};
use crate::image::{check_records, header_abi, synth_records, HEADER_MAGIC, LAYOUT_MAGIC};
use crate::results::{chain_json_from_env, validate_results, Results};
use crate::text::{contains_fixed, cr_count, word_sym_present, word_sym_value};
use crate::sha256::sha256_file;
use crate::tools::{
    budget, cargo_build_kernel_at, cargo_build_kernel_env, echo_file, host_unit, llvm_tool, resolve_qemu, run_captured,
    run_counted, run_guest, rustc_facts, say_counted, Echo, ExecProfile, Guest, Limits, RustcFacts, Stop,
};
use crate::gates::echo;
use std::path::Path;
use std::process::exit;
use std::time::Duration;

const GATE: &str = "api";
const DISPLAY: &str = "API GATE";
const HOST: &str = "qemu-mps2-an500";
const RUNNER: &str = "src/harness";
const TIMEOUT_S: u64 = 60;

const MARKER_QUIESCENT: &str =
    "[api] PASS tick-quiescent (solo busy 30 ms: 0 switches, clock advanced)";
/// The eleven [api] boot markers, boot order (check 1's cost line right
/// after its PASS, the far-deadline marker after futex-timeout, the
/// wake-priority marker after futex-wake-all).

const MARKERS_A: [&str; 11] = [
    "[api] PASS ns-monotonic (5000 paired reads + cs wrap + saturated pend windows)",
    "[api] ns-monotonic: its two designed windows each hold the tick masked across two SysTick reloads; where the witness counts, its 2 lost / 2 late are theirs by construction",
    "[api] PASS sleep-ns-floor (25 ms request: never early)",
    "[api] PASS futex-eagain (changed value refused: no sleep, 0 switches)",
    "[api] PASS futex-timeout (10 ms deadline honored: never early)",
    "[api] PASS far-deadline (a futex wait 2^32 ticks out held three ticks and woke with 0 on the wake, a sleep 2^31+1 ticks out held three ticks: never early past the 32-bit tick window)",
    "[api] PASS futex-wake-one (3 blocked: readied exactly 1, ran 0)",
    "[api] PASS futex-wake-all (released remaining 2, count exact, all ran after)",
    "[api] PASS wake-priority (wake(1) readied the more urgent of two waiters though the less urgent one's slot came first in the rotation, the rest released after)",
    "[api] PASS futex-mutex (4x300 stress: counter 1200 exact, overlap-free, slow path fired)",
    MARKER_QUIESCENT,
];
/// The six arena markers, boot order (the exhaustion check's marker
/// is last).
const MARKERS_B: [&str; 6] = [
    "[api] PASS arena-carve (window bracketed, disjoint from kstacks + statics)",
    "[api] PASS arena-zero-birth (pages read all-zero through the boot poison)",
    "[api] PASS arena-rezero (range re-zeroed in place: same address, zero VA burned, oob refused)",
    "[api] PASS arena-rezero-band (a tick landed inside the rezero's memset: the zeroing runs with the kernel band live, the wait bounded by the kernel's own tick count)",
    "[api] PASS arena-audit (two sites attributed exactly: rows, counts, bytes)",
    "[api] PASS arena-headroom (exact fit taken, further alloc refused NULL, headroom 0)",
];
/// The nine kheap/TLS/thread/registry markers, boot order (this
/// family appends after the arena family's exhaustion check; the
/// kheap-bound marker after kheap-coalesce, the stack-min marker after
/// thread-abi).
const MARKERS_C: [&str; 9] = [
    "[api] PASS kheap-basic (freed block reused at the same address, alignment honored)",
    "[api] PASS kheap-coalesce (adjacent frees merged whole: the summed span reallocated at the first block)",
    "[api] PASS kheap-bound (a request that wraps the rounding and a request over the heap refused NULL and said so, the heap intact after: a later alloc and free succeeded)",
    "[api] PASS tls-blocks (per-thread blocks distinct, init image + zero fill honored, no cross-thread bleed)",
    "[api] PASS tls-dtors (dtors ran LIFO exactly once on the exiting thread, TLS live inside, cap refused honest)",
    "[api] PASS thread-abi (round trip clean, tids unique, exact-fit stack reused, the name a kernel copy cut at the buffer, slot exhaustion refused honest)",
    "[api] PASS stack-min (0, 1, 8, 64 and 511 refused EINVAL, the sizes whose round-up wraps refused EINVAL, the largest non-wrapping size refused EAGAIN by the pool, nothing moved by a refusal, the floor itself spawned, parked and joined)",
    "[api] PASS thread-bounds (own and peer locals bracketed, windows inside kstacks, reaped tid refused ESRCH)",
    "[api] PASS registry-basic (handles distinct, stored desc survives caller clobber, full table refused honest)",
];
/// The ten suspension/activation markers, boot order (this family
/// appends after registry-basic).
const MARKERS_D: [&str; 10] = [
    "[api] PASS susp-pick (suspended spinner stayed ready with zero progress, woken waiter held until resume)",
    "[api] PASS susp-ctx (planted registers exact in ctx, sp inside the target stack, pc inside kernel text)",
    "[api] PASS susp-setctx (sp edit refused exact, the edited pc ran the pad with the planted witness)",
    "[api] PASS resume-urgent (resume preempted the caller before its next line)",
    "[api] PASS act-refuse (pre-registration send, null/second registration, self/idle suspends, idle send, unknown tids, misaligned contexts, unsuspended resume/set-context all refused exact)",
    "[api] PASS act-deliver (handler ran once per send on the target thread and stack, both interrupted flows continued)",
    "[api] PASS act-coalesce (three sends delivered exactly one handler run)",
    "[api] PASS act-redeliver (the handler re-entered before the interrupted flow resumed, then the flow continued)",
    "[api] PASS act-eintr (the wait returned RHU_EINTR after the handler ran, the re-wait then woke clean)",
    "[api] PASS act-sleep-rearm (the handler ran early, the sleep still returned not-early)",
];
/// The two module-query markers, boot order (this pair appends after
/// act-sleep-rearm).
const MARKERS_E: [&str; 2] = [
    "[api] PASS mod-find (kernel text resolved to module 0 half-open exact, arena address refused ESRCH, miss wrote nothing)",
    "[api] PASS mod-roots (module 0 ranges tie the link brackets, frozen, exidx, and tls empty in this family, walk ends ESRCH at the table count)",
];
/// The exit-door line: the machine's own exit routed through the door, the
/// LAST transcript line of a run that reached it; a run on either emulator
/// row is judged by it together with the run's exit status.
const MARKER_EXIT_DOOR: &str = "[api] exit-door: handing the machine exit to rhu_exit(0)";
/// The four hosting-door markers, boot order: the console-door line is
/// the write door's own wire output (the door IS the path that emits
/// it); the exit-door line is the LAST transcript line -- the machine
/// exit itself then rides rhu_exit(0), asserted via the exit status.
const MARKERS_F: [&str; 4] = [
    "[api] console-door: this line reached the wire through rhu_console_write",
    "[api] PASS console-pair (door write returned exact and hit the wire, no-input read returned 0 writing nothing, null, oversize and over-bound refused)",
    "[api] PASS entropy-honesty (draws filled and distinct, null refused, zero-length trivial, hardware query answers 0 on this class)",
    MARKER_EXIT_DOOR,
];
/// The two debug-facing markers, boot order: the thread-local-base door
/// check (after tls-blocks: the layout record walked from memory against
/// the doors) and the thread-id-space check (the LAST check, before the
/// exit-door line: the last two ids handed out, the spawn past the space
/// refused).
const MARKERS_G: [&str; 2] = [
    "[api] PASS tls-base-door (record walked from memory: module-0 base null on every live thread beside empty template brackets, registered-module bases equal to the door's answers on kmain and both workers, non-live slots null)",
    "[api] PASS tid-wrap (ids 0x7ffffffe and 0x7fffffff handed out positive, the spawn past the id space refused EAGAIN twice with the counter unmoved, no negative id)",
];
/// The host-band marker: the thread that hosts a payload
/// joins the hosted band for the hosting's life -- a door-spawned child
/// at the flat priority started through yield alone at one level with
/// its host, joining re-ran the pick, leaving restored kernel urgency.
/// Runs after entropy-honesty and before tid-wrap.
const MARKERS_H: [&str; 1] = [
    "[api] PASS host-band (the host joined the band and its door-spawned child at the flat priority started through yield alone at one level, joining re-ran the pick, leaving restored kernel urgency and held a band thread until the host blocked)",
];
/// The lazy floating-point marker: the activation resume
/// completes the state the handler armed before it pops the target's
/// frames -- a basic-frame target yields eight times after its resume
/// and the switch-entry witness counted no abandoned state. Runs after
/// act-sleep-rearm.
const MARKERS_I: [&str; 1] = [
    "[api] PASS act-lazy-fp (the handler ran one floating-point instruction on a target whose interrupted frame was basic, the target resumed and yielded eight times after it -- eight switch entries with its own frame basic -- and the witness counted no abandoned lazy floating-point state)",
];
/// The two sources markers: the cryptographic door under the emulator
/// row's build opt-in (after entropy-honesty) and the calendar doors
/// (after host-band, before tid-wrap).
const MARKERS_J: [&str; 2] = [
    "[api] PASS entropy-secure (the software stream served two distinct cryptographic draws under the build opt-in, null refused, zero-length and over-bound refused, the hardware query 0)",
    "[api] PASS calendar-door (the epoch at boot, the explicit set read back with its source over the elapsed clock, the replace rule, source and range refusals, no real-time clock on this row)",
];
/// The observation of record for the low-address question: pinned as a
/// required line, counted among no PASS -- red by design, not a check.
/// The double-precision forms as instructions (the check's armed line, its
/// ten digest lines over results and flags -- the values the mps2-an500
/// row's hardware printed, pinned -- and the hardware row's PASS line).
const MARKERS_K: [&str; 12] = [
    "[api] fp-double: the double-precision forms armed as instructions (the edge table under six FPSCR settings; the digests over results and flags)",
    "[api] fp-double: binary 43008 vectors digest 41ec5e6b",
    "[api] fp-double: accumulate 32768 vectors digest f8b6643d",
    "[api] fp-double: unary 2112 vectors digest d0b34f72",
    "[api] fp-double: compare 4096 vectors digest 79b37f44",
    "[api] fp-double: to-single 2688 vectors digest b56d5bdc",
    "[api] fp-double: from-single 560 vectors digest f8df090e",
    "[api] fp-double: fixed 256 vectors digest 9f33a71f",
    "[api] fp-double: select-imm 24 vectors digest fe549b57",
    "[api] fp-double: live 48 vectors digest 1bcd7647",
    "[api] fp-double: conditional 2 vectors digest f3a2ed28",
    "[api] PASS fp-double (85562 vectors computed by the hardware: the reference row)",
];
const MARKER_OBSERVED_NULL: &str = "[api] OBSERVED null-read: the record of address 0 on this row follows (the observation of record: red by design, not a check)";
/// The emulator row's record: the first page is no-access under the
/// protection unit, the read never attempted (the part's own map at 0 is
/// the row's vector table, readable -- what the `null-guard-off` control
/// image still reads).
const MARKER_OBS_ROW: &str = "[obs] address 0 on this row: no-access -- the first page is denied by the protection unit, so the read is not attempted (the part's own map at 0: readable memory); a hosted null dereference traps into the funnel here";
/// The boot line of the protection unit's posture on the Cortex-M7 row
/// (PMSAv7: the null page and the guard's slot over the background map)
/// and on the Cortex-M33 row (PMSAv8: the row's table, the background off).
const MARKER_MPU_AN500: &str = "[boot] MPU: 16 regions, 3 programmed, first page no-access, background on";
const MARKER_MPU_AN505: &str = "[boot] MPU: 8 regions, 4 programmed, first page no-access, background off";
const CONTRACT_SYMS: [&str; 8] = [
    "__rhu_debug_layout",
    "__rhu_sched",
    "__rhu_ticks",
    "__rhu_switches",
    "__rhu_lazy_fp_witness",
    "__rhu_entropy_witness",
    "__rhu_calendar",
    "__rhu_calendar_push",
];
/// The module-root bracket pairs, pinned BY VALUE in the api (base-family)
/// artifact: each pair must be present AND equal (the zero-length
/// claim; the payload gate owns the payload-family half).
const BRACKET_PAIRS: [(&str, &str); 3] = [
    ("__rhu_frozen_lo", "__rhu_frozen_hi"),
    ("__exidx_start", "__exidx_end"),
    ("__rhu_tdata_lo", "__rhu_tdata_hi"),
];
/// The GPIO doors' three checks (the device seam's first peripheral): the
/// armed lines and the PASS lines, pinned as prefixes.
const MARKERS_L: [&str; 6] = [
    "[api] gpio-claim: the line space stated;",
    "[api] PASS gpio-claim (the line space stated, a claim taken, the same program's second claim a re-configuration,",
    "[api] gpio-refuse: the board's line, a line past the space,",
    "[api] PASS gpio-refuse (the board's line EBUSY, the space's end and a far line EINVAL,",
    "[api] gpio-edge: an armed input counted eight spaced edges once each,",
    "[api] PASS gpio-edge (eight spaced edges counted once each, a parked waiter woken by one edge through the futex,",
];

/// The band-interrupt control (the double-precision emulation's fault at the
/// floor): the armed line, the binary group's digest under preemption (the
/// hardware row's value, pinned in full), the landing line (whole: the armed
/// group repeated until a tick landed, the wait bounded by the kernel's own
/// tick count -- a class, never a count) and the hardware class's PASS line,
/// pinned as prefixes.
const MARKERS_M: [&str; 4] = [
    "[api] fp-band: a band-priority exception firing every tick",
    "[api] fp-band: binary 43008 vectors digest 41ec5e6b under preemption",
    "[api] fp-band: a tick landed while the group ran armed (the group repeated until one did, the wait bounded by the kernel's own tick count)",
    "[api] PASS fp-band (the band exception fired every tick through the hardware block",
];

/// The module-code check (a double-precision form executed from a registered
/// module's range outside the primary code range): the armed line and the
/// PASS line, whole -- the same text on both emulator rows.
const MARKERS_N: [&str; 2] = [
    "[api] module-code: a form executed from a registered module's range outside the primary code range armed",
    "[api] PASS module-code (the range predicate true inside the registered range and false before it and past its end; the form from that range computed 3.0 -- through the fault funnel on the single-precision row, by the hardware on the double-precision row)",
];

/// The bus doors' three checks (the device seam's second peripheral, on the
/// rows whose facts carry a bus): the armed lines and the PASS lines, pinned
/// as prefixes.
const MARKERS_O: [&str; 6] = [
    "[api] bus-i2c: the bus count stated; a bus claimed at 100 kHz and claimed again;",
    "[api] PASS bus-i2c (one bus, claimed once and again, the identity 0x60 and the eight-byte burst read,",
    "[api] bus-spi: the bus count stated; a bus claimed and claimed again;",
    "[api] PASS bus-spi (one bus, claimed once and again, the identity EF 40 18 at 1000000 Hz,",
    "[api] bus-tenancy: a worker's long transfer on the bus,",
    "[api] PASS bus-tenancy (the worker's transfer of 4096 bytes intact,",
];

/// The PWM and ADC doors' three checks (the device seam's third and fourth
/// peripherals, on the rows whose facts carry the virtual models): the armed
/// lines and the PASS lines, pinned as prefixes.
const MARKERS_P: [&str; 6] = [
    "[api] pwm-claim: the count and the lines stated;",
    "[api] PASS pwm-claim (3 lines on 2 timers stated and enumerated,",
    "[api] adc-claim: the count and the channels' facts stated;",
    "[api] PASS adc-claim (6 channels on 2 blocks stated with their facts,",
    "[api] pwm-adc-loop: three duty cycles set on the first PWM line",
    "[api] PASS pwm-adc-loop (10, 50 and 90 percent read back as",
];

/// The RAM-range doors' check (every row): the armed
/// line and the PASS line, pinned as prefixes (the count differs by row:
/// one range on the emulator rows, two on a board with an external RAM).
const MARKERS_Q: [&str; 2] = [
    "[api] ram-range: the count and every range's record against the board's facts;",
    "[api] PASS ram-range (",
];

/// The console's two checks (after console-pair): the
/// memory console as a tee (a line through the log door read back from the
/// mirror) and the log doors in a verbose image (the level 3, a verbose and
/// a fatal-level line both on the wire, the refusals) -- the armed lines,
/// the doors' own wire lines and the PASS lines, whole.
const MARKERS_R: [&str; 7] = [
    "[api] console-mirror: a line through the log stream lands in the 16 KB memory console whole",
    "[api] console-mirror: this line is read back from the memory console",
    "[api] PASS console-mirror (the header's magic and size, the count moved by the line's wire length, the bytes equal)",
    "[api] log-level: the level door and the log write door in a verbose image",
    "[api] log-door: this verbose line reached the wire through rhu_log_write",
    "[api] log-door: this fatal-level line reached the wire through rhu_log_write",
    "[api] PASS log-level (the level 3 in this image, a verbose and a fatal-level line through the log door with exact counts, levels 0 and 4, a null buffer and an over-bound write refused, a zero write 0)",
];

/// The six UART checks of the api flavor on the emulator rows (after
/// log-level): the armed line and the PASS line of each, constant text;
/// the rows' own numbers (the rates achieved, the writes and interrupts
/// counted, the wait's milliseconds) are on lines this gate does not pin.
const MARKERS_S: [&str; 12] = [
    "[api] uart-enumerate: the count and every row's facts; the console's block refused at the claim, a block the row has not refused",
    "[api] PASS uart-enumerate (2 rows stated and enumerated with their facts, an index past the count and a null pointer EINVAL, the console's block EBUSY at the claim, a block the row has not EINVAL)",
    "[api] uart-claim-twice: a claim taken, a second claim refused, the event word given and bumped at the release, bad handles refused",
    "[api] PASS uart-claim-twice (the claim answered the row's handle, a second claim EBUSY, the event word given for the held handle and null for a bad one, the info door's facts, the release bumped the word once, a second release and the doors on the released handle EINVAL, claimed again)",
    "[api] uart-frame: the line discipline on this row's part -- 8N1 at the rates the divider reaches, every other frame and an unreachable rate refused",
    "[api] PASS uart-frame (8N1 at 115200, 9600 and 921600 with the achieved rates as the divider gives them, a rate past the ceiling, a rate the divider misses by more than 2 percent and a zero rate EINVAL, every frame but 8N1 EINVAL on this part, a null pointer and a bad handle EINVAL, the break door EINVAL on a part without break control)",
    "[api] uart-loopback: 1500 bytes through the port and back over the wire's echo, byte-equal, nothing lost",
    "[api] PASS uart-loopback (a probe byte back through the wire's echo, 1500 bytes written in partial writes and read back byte-equal, the status settled to nothing pending, the witness's overrun, framing, parity and drop counts unmoved and its byte counts exact)",
    "[api] uart-status-and-flush: the three counts and the flush selectors",
    "[api] PASS uart-status-and-flush (the idle counts, 8 bytes written and the 8 back counted by the status door with nothing pending to transmit, the receive flush emptying the ring, the flush of both, bad selectors, handles and null buffers EINVAL, zero reads and writes 0)",
    "[api] uart-timeout: a wait on the event word of a silent port returns at its deadline, never early",
    "[api] PASS uart-timeout (the event word's wait answered ETIMEDOUT at its 300 ms deadline, never early, within the tick's lateness)",
];


/// The network doors' eight checks (after uart-timeout, on the
/// emulator rows with the `net` feature over the virtual medium and its
/// scripted peer): the armed line and the PASS line of each, constant text;
/// the rows' own numbers (the pump's passes and milliseconds, the exchange
/// counts) are on lines this gate does not pin.
/// The exit word's marker (the check before tid-wrap, on both rows).
const MARKERS_U: [&str; 2] = [
    "[api] PASS exit-word (the magic at offset 0 and the code at offset 12 by symbol: RLOG and 0 while running; RRET beside 42, 70000 and 0 for returned, RABT beside 134, -1 and -2147483648 for aborted, each read back whole and decoded; a magic outside the three refused; RLOG and 0 restored)",
    "[api] PASS handler-stack (the high-water ",
];
const MARKERS_T: [&str; 16] = [
    "[api] net-pump: the network thread's statistics after the boot's exchange -- passes counted, every pass and every deadline wake under the 400 ms bound",
    "[api] PASS net-pump (the network thread ran through the boot's exchange with every pass and every deadline wake under the 400 ms bound; the counts above)",
    "[api] net-medium: the count, the medium's facts, the lease by DHCP with the record exact, the limits, the medium word, the refusals",
    "[api] PASS net-medium (1 virtual medium, link up, its hardware address; the lease by DHCP with the record exact: 192.0.2.2/24, the gateway 192.0.2.1, the name servers 192.0.2.1 and 192.0.2.3; the limits 4 streams, 2 datagrams, 2048 and 2048 byte stream buffers, a 1536 byte datagram buffer, 4 resolver slots; the medium word moved; a medium past the count and null pointers EINVAL)",
    "[api] net-retry: the first discover unanswered by the peer, the lease on the retry after one second; the medium's rings never full",
    "[api] PASS net-retry (the first discover dropped by the peer, the retry answered, the lease after the one-second retransmit; no frame dropped for want of a slot)",
    "[api] net-refuse: every refusal by its cause -- IPv6, a type the row has not, the pool spent, a second bind, the unconnected stream, bad handles and words, the peer's reset, the connect budget, a static address and the route",
    "[api] PASS net-refuse (IPv6 EAFNOSUPPORT, a bad type and protocol EPROTONOSUPPORT, the fifth stream EMFILE, a second bind EADDRINUSE, a bound, foreign or zero-port bind EINVAL, the unconnected stream ENOTCONN, a listen or accept on a datagram socket EINVAL, unsupported options ENOTSUP with the read-back form proven, bad words and three bad handles EINVAL at every door, a null and an IPv6 peer refused, the peer's reset ECONNREFUSED read once, the unowned address EHOSTUNREACH at the connect budget, a static address without a gateway with the off-segment connect ENETUNREACH and a bad mask EINVAL, the DHCP client armed again with its lease)",
    "[api] net-resolve: a scripted answer read back as an address record, a second name, a name the server has not, the slots exhausted, the refusals",
    "[api] PASS net-resolve (two names answered with their records through the resolver's word, a name the server has not EHOSTUNREACH, four slots taken and the fifth EAGAIN with every answer read, a bad name and bad handles EINVAL, the released slot EINVAL, the witness exact)",
    "[api] net-stream: a listener and a client over the loopback -- the connect edge, the risen bits, the accept, 26 then 3000 bytes echoed byte-exact, the peek, the half-close, the closes",
    "[api] PASS net-stream (a listener and a client over the loopback: the connect started with no readiness and a second connect EALREADY, writable at establishment with no error pending and a third connect EISCONN, the risen bits taken once and empty after, the accept with the peer's address and the client's own port, 26 bytes then 3000 patterned bytes echoed byte-exact through the peek and the available door, the peer's half-close read as hang-up and a zero read with EPIPE on the shut side, the three closed and the closed handle refused)",
    "[api] net-datagram: two bound datagram sockets over the loopback -- a datagram with its address round-tripped, the peek and the available door, an oversize datagram refused, the default peer, an unbound sender's own port",
    "[api] PASS net-datagram (two bound datagram sockets over the loopback: a 64-byte datagram with the sender's address round-tripped through the peek and the available door, an oversize datagram EMSGSIZE, a buffer smaller than the datagram EMSGSIZE with the datagram discarded, the default peer set and used, an unbound sender given a port of the row's choosing and its name answered, the three closed)",
    "[api] net-port: the bump door moves the global word by one and wakes a parked waiter; an idle connected stream makes no bump across two of the thread's safety deadlines (1100 ms)",
    "[api] PASS net-port (an idle connected stream moved the global word not once across two of the thread's safety deadlines (1100 ms), the waiter sleeping to its deadline; the bump door moved the word by exactly one and woke the parked waiter; the pair closed)",
];

/// The futex + nanosecond-time + arena + kheap/TLS/thread/registry +
/// suspension/activation + module-query + console/exit/entropy + GPIO +
/// bus + PWM + ADC + RAM-range C-ABI surface, pinned in the artifact (the
/// waistline exists and survives every build of this flavor); the table
/// names every export.
const API_SYMS: [&str; 112] = [
    "rhu_now_ns",
    "rhu_sleep_ns",
    "rhu_futex_wait",
    "rhu_futex_wake",
    "rhu_arena_alloc_at",
    "rhu_arena_rezero",
    "rhu_arena_headroom",
    "rhu_arena_report",
    "rhu_kheap_alloc",
    "rhu_kheap_free",
    "rhu_module_register",
    "rhu_tls_get",
    "rhu_thread_atexit",
    "rhu_thread_spawn",
    "rhu_thread_exit",
    "rhu_thread_self",
    "rhu_thread_yield",
    "rhu_thread_stack_bounds",
    "rhu_thread_suspend",
    "rhu_thread_set_context",
    "rhu_thread_resume",
    "rhu_activation_register",
    "rhu_activation_send",
    "rhu_module_find",
    "rhu_module_roots",
    "rhu_console_write",
    "rhu_console_read",
    "rhu_exit",
    "rhu_abort",
    "rhu_entropy",
    "rhu_entropy_is_hardware",
    "rhu_entropy_secure",
    "rhu_calendar_get",
    "rhu_calendar_set",
    "rhu_gpio_count",
    "rhu_gpio_claim",
    "rhu_gpio_release",
    "rhu_gpio_write",
    "rhu_gpio_read",
    "rhu_gpio_edge",
    "rhu_gpio_event_word",
    "rhu_i2c_count",
    "rhu_i2c_bus",
    "rhu_i2c_claim",
    "rhu_i2c_transfer",
    "rhu_spi_count",
    "rhu_spi_bus",
    "rhu_spi_claim",
    "rhu_spi_transfer",
    "rhu_pwm_count",
    "rhu_pwm_line",
    "rhu_pwm_claim",
    "rhu_pwm_set",
    "rhu_pwm_period",
    "rhu_pwm_enable",
    "rhu_pwm_release",
    "rhu_pwm_info",
    "rhu_adc_count",
    "rhu_adc_channel",
    "rhu_adc_claim",
    "rhu_adc_read",
    "rhu_adc_reference",
    "rhu_adc_release",
    "rhu_fault_register",
    "rhu_ram_count",
    "rhu_ram_range",
    "rhu_arena_alloc_from",
    "rhu_blk_count",
    "rhu_blk_device",
    "rhu_blk_read",
    "rhu_blk_write",
    "rhu_blk_sync",
    "rhu_log_level",
    "rhu_log_write",
    "rhu_uart_count",
    "rhu_uart_port",
    "rhu_uart_claim",
    "rhu_uart_configure",
    "rhu_uart_write",
    "rhu_uart_read",
    "rhu_uart_status",
    "rhu_uart_event_word",
    "rhu_uart_flush",
    "rhu_uart_break",
    "rhu_uart_release",
    "rhu_uart_info",
    "rhu_net_count",
    "rhu_net_medium",
    "rhu_net_medium_word",
    "rhu_net_configure",
    "rhu_net_lease",
    "rhu_net_limits",
    "rhu_net_resolve",
    "rhu_net_resolve_result",
    "rhu_net_resolve_word",
    "rhu_net_socket",
    "rhu_net_bind",
    "rhu_net_listen",
    "rhu_net_accept",
    "rhu_net_connect",
    "rhu_net_send",
    "rhu_net_recv",
    "rhu_net_available",
    "rhu_net_shutdown",
    "rhu_net_close",
    "rhu_net_readiness",
    "rhu_net_error",
    "rhu_net_event_word",
    "rhu_net_events_word",
    "rhu_net_events_bump",
    "rhu_net_name",
    "rhu_net_option",
];

/// One emulator row's boot images: the machine, the build's extra
/// arguments, the built image's path under the row's boot target dir and
/// the quiet image's banner clause (the board, its chip and its clock).
struct BootRow {
    row: &'static str,
    machine: &'static str,
    extra: &'static [&'static str],
    built: &'static str,
    banner: &'static str,
}

const BOOT_ROWS: [BootRow; 2] = [
    BootRow {
        row: "an500",
        machine: "mps2-an500",
        extra: &["--features", "entropy-accept-software"],
        built: "thumbv7em-none-eabihf/release/rhu-kernel",
        banner: "QEMU mps2-an500 (Cortex-M7, 25 MHz)",
    },
    BootRow {
        row: "an505",
        machine: "mps2-an505",
        extra: &["--target", "thumbv8m.main-none-eabihf", "--no-default-features", "--features", "qemu-exit,board-mps2-an505,entropy-accept-software"],
        built: "thumbv8m.main-none-eabihf/release/rhu-kernel",
        banner: "QEMU mps2-an505 (Cortex-M33, 20 MHz)",
    },
];

/// The three routes of the boot flavor and whether the wire carries the
/// banner: the level quiet (the one banner line), the level silent
/// (nothing), both streams to the memory console (nothing on the wire; the
/// image's exit 0 is its own assertion that the mirror holds the banner).
const BOOT_ROUTES: [(&str, &[(&str, &str)], bool); 3] = [
    ("boot-quiet", &[("RHU_LOG_LEVEL", "quiet")], true),
    ("boot-silent", &[("RHU_LOG_LEVEL", "silent")], false),
    ("boot-memory", &[("RHU_LOG_CHANNEL", "memory"), ("RHU_CONSOLE_CHANNEL", "memory")], false),
];

/// The crate's version as its manifest states it: the banner's number.
fn kernel_version(repo: &Path) -> Result<String, String> {
    let text = std::fs::read_to_string(repo.join("src/kernel/Cargo.toml")).map_err(|e| format!("src/kernel/Cargo.toml: {e}"))?;
    text.lines()
        .find_map(|l| l.trim().strip_prefix("version = \"").and_then(|r| r.strip_suffix('"')).map(|v| v.to_string()))
        .ok_or_else(|| "src/kernel/Cargo.toml states no version".to_string())
}

/// The boot images' judge: the wire carries exactly `want` -- the quiet
/// image's one line with its CRLF, the silent and memory images' nothing.
fn boot_transcript_fault(raw: &[u8], want: &[u8]) -> Option<String> {
    if raw == want {
        return None;
    }
    Some(format!(
        "the wire carried {} bytes where {} were expected: {:?}",
        raw.len(),
        want.len(),
        String::from_utf8_lossy(raw)
    ))
}

struct API {
    ctx: GateCtx,
    facts: RustcFacts,
    /// The api case's raw transcript: the double-precision row's fp-double block for the parity case.
    api_raw: Vec<u8>,
}

fn setup(repo: &Path) -> (RustcFacts, std::path::PathBuf, String) {
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
    (facts, qemu, qemu_v)
}

impl API {
    fn emit_case(&mut self, case: &str, verdict: &str, code: i32, want: i32, sha: &str, path_rel: &str) {
        self.ctx.results.emit(&format!(
            "{{\"schema\":1,\"type\":\"case\",\"gate\":\"api\",\"case\":\"{case}\",\"verdict\":\"{verdict}\",\"exit\":{code},\"exit_want\":{want},\"control\":\"positive\",\"image\":{{\"path\":\"{path_rel}\",\"sha256\":\"{sha}\"}},\"golden\":null}}"
        ));
        self.ctx.results.tally(verdict, "positive");
    }

    fn fail_case(&mut self, case: &str, code: i32, want: i32, sha: &str, path_rel: &str, msg: &str) -> ! {
        self.emit_case(case, "fail", code, want, sha, path_rel);
        self.ctx.gate_fail(&format!("{case}: {msg}"))
    }

    fn common_asserts(&mut self, case: &str, run: &CaseRun, want: i32, markers: &[&str], forbidden: &[&str]) {
        let (sha, path_rel, code) = (run.sha.clone(), run.path_rel.clone(), run.exit);
        for m in markers {
            if !contains_fixed(&run.raw, m) {
                self.fail_case(case, code, want, &sha, &path_rel, &format!("marker '{m}' not found"));
            }
        }
        for m in forbidden {
            if contains_fixed(&run.raw, m) {
                self.fail_case(case, code, want, &sha, &path_rel, &format!("forbidden pattern '{m}' present"));
            }
        }
        if cr_count(&run.raw) == 0 {
            self.fail_case(case, code, want, &sha, &path_rel, "no CR bytes in raw capture (wire discipline lost)");
        }
        if code != want {
            self.fail_case(case, code, want, &sha, &path_rel, &format!("exit {code}, wanted {want}"));
        }
    }

    fn run_api(&mut self) {
        let run = self.build_and_run_echo("api", &["--features", "api-smoke,net"]);
        let (sha, path_rel, code) = (run.sha.clone(), run.path_rel.clone(), run.exit);
        let mut markers: Vec<&str> = vec![
            "flavor: api-smoke",
            MARKER_MPU_AN500,
            "[base] ALL GOOD (boot + console + panic/fault funnel armed)",
            "[sched] ALL GOOD (threads + timers + preemption online)",
        ];
        markers.extend(MARKERS_A);
        markers.extend(MARKERS_B);
        markers.extend(MARKERS_C);
        markers.extend(MARKERS_D);
        markers.extend(MARKERS_E);
        markers.extend(MARKERS_F);
        markers.extend(MARKERS_G);
        markers.extend(MARKERS_H);
        markers.extend(MARKERS_I);
        markers.extend(MARKERS_J);
        markers.extend(MARKERS_K);
        markers.extend(MARKERS_L);
        markers.extend(MARKERS_M);
        markers.extend(MARKERS_N);
        markers.extend(MARKERS_O);
        markers.extend(MARKERS_P);
        markers.extend(MARKERS_Q);
        markers.extend(MARKERS_R);
        markers.extend(MARKERS_S);
        markers.extend(MARKERS_T);
        markers.extend(MARKERS_U);
        markers.push(MARKER_OBSERVED_NULL);
        markers.push(MARKER_OBS_ROW);
        self.common_asserts("api", &run, 0, &markers, &["KERNEL PANIC", "EXCEPTION:"]);
        self.handler_stack_reading("api", "mps2-an500", &run.raw);
        self.api_raw = run.raw.clone();
        // Pin the witnesses in the artifact: the switch counter beside
        // the debug-layout contract, and the four first-group API symbols.
        let nm = match llvm_tool(&self.facts, "llvm-nm") {
            Ok(p) => p,
            Err(e) => self.ctx.gate_fail(&format!("api: {e}")),
        };
        let elf = self.ctx.out.join("rhu-kernel-api.elf");
        let elf_arg = elf.to_string_lossy().into_owned();
        let sym = self.ctx.out.join("api.sym");
        let scratch = self.ctx.out.join("api.tool.stderr");
        let nm_run = run_captured(&nm, &[&elf_arg], &sym, &scratch, Duration::from_secs(60));
        if !matches!(nm_run, Ok(ref o) if o.exit == 0) {
            self.fail_case("api", code, 0, &sha, &path_rel, "llvm-nm failed on the api ELF");
        }
        let dump = std::fs::read(&sym).unwrap_or_default();
        for s in CONTRACT_SYMS.iter().chain(API_SYMS.iter()) {
            if !word_sym_present(&dump, s) {
                self.fail_case("api", code, 0, &sha, &path_rel, &format!("contract symbol '{s}' absent from the artifact"));
            }
        }
        // The payload entry seam belongs to the payload family: a
        // base-family image carries no such symbol, because nothing in
        // it could provide one.
        if word_sym_present(&dump, "rhu_payload_entry") {
            self.fail_case("api", code, 0, &sha, &path_rel, "rhu_payload_entry present in a base-family artifact (the entry seam belongs to the payload family)");
        }
        for (lo, hi) in BRACKET_PAIRS.iter() {
            let (Some(vlo), Some(vhi)) = (word_sym_value(&dump, lo), word_sym_value(&dump, hi))
            else {
                self.fail_case("api", code, 0, &sha, &path_rel, &format!("bracket pair {lo}/{hi} not value-parseable from the artifact"));
            };
            if vlo != vhi {
                self.fail_case("api", code, 0, &sha, &path_rel, &format!("bracket pair {lo}/{hi} not zero-length in a base-family image ({vlo:#x} != {vhi:#x})"));
            }
        }
        // The two records the image states about itself, read off the
        // artifact by symbol through the load segments and tied: the
        // in-image header at its stated load address + 0x400 (the load
        // address the lowest load segment's), the debug-visible layout
        // record at the door's version or later and equal to the
        // header's statement, the generation equal to include/rhu.h.
        let header_text = std::fs::read_to_string(self.ctx.repo.join("src/kernel/include/rhu.h")).unwrap_or_default();
        let Some(abi) = header_abi(&header_text) else {
            self.ctx.gate_fail("api: src/kernel/include/rhu.h states no RHU_ABI (the contract generation the image header carries)")
        };
        let elf_bytes = std::fs::read(&elf).unwrap_or_default();
        let rec = match check_records(&dump, &elf_bytes, abi) {
            Ok(r) => r,
            Err(e) => self.fail_case("api", code, 0, &sha, &path_rel, &format!("image records: {e}")),
        };
        self.emit_case("api", "pass", code, 0, &sha, &path_rel);
        println!(
            "== case api: PASS (exit 0, 53 hosting-API checks + the null-read observation (address 0 no-access by the protection unit, the read never attempted: the observation of record, not a check), the protection unit's boot line, contract + API symbols (the lazy floating-point witness among them) + zero-length module-root brackets in-artifact, no payload-entry seam in this family, the image header at {:#x} stating load address {:#010x} and generation {} and the layout record at {:#x} at version {} for {} threads with {} tls slots of {} bytes at offset {} in a {}-byte TCB, read off the artifact and tied) ==",
            rec.header_at, rec.load_addr, rec.abi, rec.layout_at, rec.layout_version, rec.max_threads, rec.tls_slots, rec.tls_slot_bytes, rec.tcb_off_tls, rec.tcb_stride
        );
    }


    /// The double-precision digests on the single-precision row: the api
    /// flavor built for the mps2-an505 row (the Cortex-M33, the emulation)
    /// must print the fp-double block the mps2-an500 run printed from its
    /// hardware -- one program, two classes, one block -- and its own PASS
    /// lines for the emulated run and the frame promotion. Then the negative
    /// control: the decoder with the add and subtract operations swapped
    /// (the feature `test-fpemul-doctored`) prints a different block.
    fn run_fp_parity(&mut self) {
        let hw = fp_block(&self.api_raw);
        if hw.is_empty() {
            self.ctx.gate_fail("fp-parity: the double-precision row printed no fp-double block");
        }
        let run = self.build_and_run_an505("fp-parity", "qemu-exit,board-mps2-an505,entropy-accept-software,api-smoke,net");
        let (sha, path_rel, code) = (run.sha.clone(), run.path_rel.clone(), run.exit);
        // The ending first: exit 0 through the exit door, no timeout, no death.
        if let Err(why) = run_ending(code, 0, &run.raw) {
            self.fail_case("fp-parity", code, 0, &sha, &path_rel, &why);
        }
        self.handler_stack_reading("fp-parity", "mps2-an505", &run.raw);
        let mut fp_markers: Vec<&str> = MARKERS_S.to_vec();
        fp_markers.extend_from_slice(&MARKERS_T);
        fp_markers.extend_from_slice(&MARKERS_U);
        fp_markers.extend_from_slice(&[
                MARKER_EXIT_DOOR,
                MARKER_MPU_AN505,
                "[api] PASS fp-double (",
                "every form emulated through the fault funnel, none refused, no frame promoted",
                "[api] PASS fp-frames (the basic frame promoted once",
                "[api] fp-band: a tick landed while the group ran armed (the group repeated until one did, the wait bounded by the kernel's own tick count)",
                "[api] PASS fp-band (the band exception fired every tick and preempted emulated forms",
        ]);
        self.common_asserts("fp-parity", &run, 0, &fp_markers, &["EXCEPTION:", "KERNEL PANIC"]);
        let em = fp_block(&run.raw);
        if em != hw {
            let first = hw.iter().zip(em.iter()).position(|(a, b)| a != b).unwrap_or(hw.len().min(em.len()));
            self.fail_case("fp-parity", code, 0, &sha, &path_rel, &format!("the fp-double block differs between the classes at line {} of {} (hardware: {:?}; emulated: {:?})", first + 1, hw.len(), hw.get(first), em.get(first)));
        }
        self.emit_case("fp-parity", "pass", code, 0, &sha, &path_rel);
        println!("== case fp-parity: PASS (the fp-double block of {} lines byte-equal across the classes -- the hardware row and the emulated row; the emulated row's own PASS lines present; exit 0 through the exit door) ==", hw.len());

        let run2 = self.build_and_run_an505("fp-doctored-control", "qemu-exit,board-mps2-an505,entropy-accept-software,api-smoke,test-fpemul-doctored");
        let (sha2, path_rel2, code2) = (run2.sha.clone(), run2.path_rel.clone(), run2.exit);
        let doc = fp_block(&run2.raw);
        if doc == hw {
            self.emit_control("fp-doctored-control", "fail", FP_DOCTORED_BROKE, code2, 2, &sha2, &path_rel2);
            self.ctx.gate_fail("fp-doctored-control: the doctored decoder's fp-double block equals the hardware row's (the control did not bite)");
        }
        // The terminal half: the image's last act is a form from a range no module
        // registers; the funnel must refuse it by name and the exit door must never print.
        let refused = contains_fixed(&run2.raw, "[api] module-code-control: a form from an unregistered range follows")
            && contains_fixed(&run2.raw, "not emulated -- the stacked pc outside every registered code range")
            && !contains_fixed(&run2.raw, "[api] exit-door:");
        if !refused {
            self.emit_control("fp-doctored-control", "fail", FP_DOCTORED_BROKE, code2, 2, &sha2, &path_rel2);
            self.ctx.gate_fail("fp-doctored-control: the form from an unregistered range was not refused by the funnel, or the image reached the exit door (the control did not bite)");
        }
        // The ending: the funnel's fault path is the machine's exit 2, never a hang.
        if let Err(why) = run_ending(code2, 2, &run2.raw) {
            self.emit_control("fp-doctored-control", "fail", FP_DOCTORED_BROKE, code2, 2, &sha2, &path_rel2);
            self.ctx.gate_fail(&format!("fp-doctored-control: {why} (the control did not die as designed)"));
        }
        self.emit_control("fp-doctored-control", "pass", FP_DOCTORED_BROKE, code2, 2, &sha2, &path_rel2);
        println!("== control fp-doctored-control: PASS (the decoder with add and subtract swapped prints a different fp-double block: the parity case would refuse it; the form from an unregistered range refused by the funnel, the exit door never reached; the image died through the funnel's fault path, exit 2) ==");
    }


    /// The emulator rows' GPIO control: a
    /// virtual backend whose loopback latch fires the input's event twice
    /// per change; the api flavor's edge check must refuse it -- two counted
    /// for one edge, no PASS line -- so the control shows the check reads
    /// the mechanism. The silicon control (the removed pending-mask clear)
    /// is a hardware-side control.
    fn run_gpio_latch_control(&mut self) {
        let run = self.build_and_run_echo("gpio-latch-control", &["--features", "api-smoke,test-gpio-doctored-latch"]);
        let (sha, path_rel, code) = (run.sha.clone(), run.path_rel.clone(), run.exit);
        let refused = !contains_fixed(&run.raw, "[api] PASS gpio-edge (") && contains_fixed(&run.raw, "gpio-edge: edge 0 counted 2 times");
        if !refused {
            self.emit_control("gpio-latch-control", "fail", GPIO_LATCH_BROKE, code, 1, &sha, &path_rel);
            self.ctx.gate_fail("gpio-latch-control: the doctored latch's edge check passed, or died elsewhere than the count (the control did not bite)");
        }
        if let Err(why) = run_ending(code, 1, &run.raw) {
            self.emit_control("gpio-latch-control", "fail", GPIO_LATCH_BROKE, code, 1, &sha, &path_rel);
            self.ctx.gate_fail(&format!("gpio-latch-control: {why} (the control did not die as designed)"));
        }
        self.emit_control("gpio-latch-control", "pass", GPIO_LATCH_BROKE, code, 1, &sha, &path_rel);
        println!("== control gpio-latch-control: PASS (the virtual latch firing twice per change is refused by the edge check: two counted for one edge, no PASS line; the image died by its panic, exit 1)");
    }

    /// The exit word's control: the store into the console mirror's header
    /// doctored to swap the word's halves; the api flavor's check must refuse
    /// it by its read-back -- the word for returned 42 reading 0x0001002a, no
    /// PASS line -- so the control shows the check reads the header's word by
    /// symbol against the contract's numbers, not the store's argument.
    fn run_exit_word_control(&mut self) {
        let run = self.build_and_run_echo("exit-word-control", &["--features", "api-smoke,test-exit-word-doctored"]);
        let (sha, path_rel, code) = (run.sha.clone(), run.path_rel.clone(), run.exit);
        let refused = !contains_fixed(&run.raw, "[api] PASS exit-word (")
            && contains_fixed(&run.raw, "exit-word: the code for returned 70000 read back 4464, wanted 70000");
        if !refused {
            self.emit_control("exit-word-control", "fail", EXIT_WORD_BROKE, code, 1, &sha, &path_rel);
            self.ctx.gate_fail("exit-word-control: the doctored store's check passed, or died elsewhere than the read-back (the control did not bite)");
        }
        if let Err(why) = run_ending(code, 1, &run.raw) {
            self.emit_control("exit-word-control", "fail", EXIT_WORD_BROKE, code, 1, &sha, &path_rel);
            self.ctx.gate_fail(&format!("exit-word-control: {why} (the control did not die as designed)"));
        }
        self.emit_control("exit-word-control", "pass", EXIT_WORD_BROKE, code, 1, &sha, &path_rel);
        println!("== control exit-word-control: PASS (the store keeping the code's low 16 bits is refused by the exit-word check's read-back: 4464 for returned 70000, no PASS line; the image died by its panic, exit 1)");
    }

    /// The emulator rows' second GPIO control: a virtual backend that makes
    /// ONE event of its own beside the tenth counted edge -- an edge the
    /// program did not make. The api flavor's edge check must refuse it by
    /// its total -- thirteen where twelve were made, no PASS line -- so the
    /// control shows the count refuses an edge the check did not make,
    /// wherever it lands (on a board: an input nothing holds).
    fn run_gpio_edge_control(&mut self) {
        let run = self.build_and_run_echo("gpio-edge-control", &["--features", "api-smoke,test-gpio-doctored-edge"]);
        let (sha, path_rel, code) = (run.sha.clone(), run.path_rel.clone(), run.exit);
        let refused = !contains_fixed(&run.raw, "[api] PASS gpio-edge (") && contains_fixed(&run.raw, "gpio-edge: 13 edges in all, want 12");
        if !refused {
            self.emit_control("gpio-edge-control", "fail", GPIO_EDGE_BROKE, code, 1, &sha, &path_rel);
            self.ctx.gate_fail("gpio-edge-control: the doctored backend's edge check passed, or died elsewhere than the total (the control did not bite)");
        }
        if let Err(why) = run_ending(code, 1, &run.raw) {
            self.emit_control("gpio-edge-control", "fail", GPIO_EDGE_BROKE, code, 1, &sha, &path_rel);
            self.ctx.gate_fail(&format!("gpio-edge-control: {why} (the control did not die as designed)"));
        }
        self.emit_control("gpio-edge-control", "pass", GPIO_EDGE_BROKE, code, 1, &sha, &path_rel);
        println!("== control gpio-edge-control: PASS (one event the program did not make, beside the tenth edge, is refused by the edge check's total: thirteen where twelve were made, no PASS line; the image died by its panic, exit 1)");
    }

    /// The set-context check's control: a landing pad that stores its witness
    /// word before its thread id and sleeps between the two. The check waits
    /// on the witness and then reads the id, so it must refuse at its
    /// off-thread assert -- the control shows the check's two words are read
    /// in the order the pad must write them.
    fn run_susp_order_control(&mut self) {
        let run = self.build_and_run_echo("susp-order-control", &["--features", "api-smoke,test-susp-doctored-order"]);
        let (sha, path_rel, code) = (run.sha.clone(), run.path_rel.clone(), run.exit);
        let refused = !contains_fixed(&run.raw, "[api] PASS susp-setctx (") && contains_fixed(&run.raw, "susp-setctx: pad ran off-thread");
        if !refused {
            self.emit_control("susp-order-control", "fail", SUSP_ORDER_BROKE, code, 1, &sha, &path_rel);
            self.ctx.gate_fail("susp-order-control: the doctored pad's check passed, or died elsewhere than the off-thread assert (the control did not bite)");
        }
        if let Err(why) = run_ending(code, 1, &run.raw) {
            self.emit_control("susp-order-control", "fail", SUSP_ORDER_BROKE, code, 1, &sha, &path_rel);
            self.ctx.gate_fail(&format!("susp-order-control: {why} (the control did not die as designed)"));
        }
        self.emit_control("susp-order-control", "pass", SUSP_ORDER_BROKE, code, 1, &sha, &path_rel);
        println!("== control susp-order-control: PASS (the pad storing its witness before its thread id is refused by the set-context check's off-thread assert, no PASS line; the image died by its panic, exit 1)");
    }

    /// The RP2350's bus drivers on the Cortex-M33 emulator row: under the
    /// test feature `test-bus-rp2350` the row carries register-level models
    /// of that chip's I2C and SPI blocks in place of the ST blocks', and the
    /// api flavor's three model-backed bus checks run over the chip's own
    /// drivers, unchanged in their lines. Then the same image with the
    /// doctored models and the unbounded pump: the three refusals, no bus
    /// check passing, the image walking on to its exit door. The doctored
    /// run is instruction-counted, as the ST drivers' control is.
    fn run_bus_rp2350(&mut self) {
        let run = self.build_and_run_an505("bus-rp2350", "qemu-exit,board-mps2-an505,entropy-accept-software,api-smoke,test-bus-rp2350");
        let (sha, path_rel, code) = (run.sha.clone(), run.path_rel.clone(), run.exit);
        if let Err(why) = run_ending(code, 0, &run.raw) {
            self.fail_case("bus-rp2350", code, 0, &sha, &path_rel, &why);
        }
        self.common_asserts(
            "bus-rp2350",
            &run,
            0,
            &[MARKER_EXIT_DOOR, "[api] PASS bus-i2c (", "[api] PASS bus-spi (", "[api] PASS bus-tenancy ("],
            &["EXCEPTION:", "KERNEL PANIC", "[bus] "],
        );
        self.emit_case("bus-rp2350", "pass", code, 0, &sha, &path_rel);
        println!("== case bus-rp2350: PASS (the three bus checks passed over the RP2350's I2C and SPI drivers against the register-level models of its blocks, no kernel bus line; exit 0 through the exit door) ==");

        let CaseRun { sha, path_rel, raw, exit: code } =
            self.build_and_run_an505_with("bus-rp2350-doctored-control", "qemu-exit,board-mps2-an505,entropy-accept-software,api-smoke,test-bus-rp2350,test-bus-doctored", ExecProfile::Deterministic, BUS_CONTROL_WORK);
        let bit = contains_fixed(&raw, "[api] bus-doctored: the data byte not acknowledged answered -5 at message 0 byte 1")
            && contains_fixed(&raw, "[bus] i2c 0: the wire wait passed its budget at message 0; the block reset")
            && contains_fixed(&raw, "[bus] i2c 0: the block did not come to rest at the claim (its reset or its disable did not complete); while it stays so, a transfer on this bus times out instead of hanging")
            && contains_fixed(&raw, "[api] bus-doctored: the claim of a block that had not come to rest answered 0 and was told; the first transfer found the claim's registers lost and wrote them again")
            && contains_fixed(&raw, "[api] bus-doctored: the fast-mode claim at 400000 Hz programmed the fast counts (low 15, high 7, spike 1 at 12000000 Hz; 387096 Hz achieved)")
            && contains_fixed(&raw, "[api] bus-doctored: an opening whose disable did not complete answered -110 at once through the reset path")
            && contains_fixed(&raw, "[api] bus-doctored: an abort whose stop never came answered -110 at message 0 byte 1 inside the budget")
            && contains_fixed(&raw, "[api] bus-doctored: a clock held from the first of twenty bytes, with a byte nobody asked for in the receive queue, answered -110 inside the budget")
            && contains_fixed(&raw, "[api] bus-doctored: a reserved address answered -22 at message 0 before the wire")
            && contains_fixed(&raw, "[bus] spi 0: the block's reset did not complete at the claim; while it stays so, a transfer on this bus times out instead of hanging")
            && contains_fixed(&raw, "[api] bus-doctored: the SPI claim of a block whose reset did not complete answered 0 and was told")
            && contains_fixed(&raw, "[bus] spi 0: the wire wait passed its budget at byte 0; the block reset")
            && contains_fixed(&raw, "[bus] spi 0: the wire wait passed its budget at byte 3; the block reset")
            && contains_fixed(&raw, "[api] bus-doctored: an SPI clock that never came to rest answered -110 at byte 0 at once; an overrun at a burst's end answered it at byte 3 at once")
            && contains_fixed(&raw, "[bus] i2c 0: the bus recovered after the timeout (a device held the data line low; clocked free, then a stop)")
            && contains_fixed(&raw, "[bus] i2c 0: the bus recovered after the timeout (a device held the data line low through nine clocks; the clock held low 35 ms, then a stop)")
            && contains_fixed(&raw, "[bus] i2c 0: the data line is still held low after the timeout's recovery (nine clocks, the clock held low 35 ms, a stop); while it stays so, every transfer on this bus fails")
            && contains_fixed(&raw, "[api] bus-doctored: a data line held after the timeout came free by the clocks; the bus recovered and was counted")
            && contains_fixed(&raw, "[api] bus-doctored: a data line held through nine clocks came free by the clock held low 35 ms; the bus recovered and was counted")
            && contains_fixed(&raw, "[api] bus-doctored: a data line held through the whole recovery was told and not counted as recovered")
            && contains_fixed(&raw, "[api] bus-doctored: a device that held the clock low 25 ms in one byte was served")
            && contains_fixed(&raw, "[api] bus-doctored: the clock that never completed answered -110 inside the budget; the block reset once")
            && contains_fixed(&raw, "[api] bus-doctored: the unbounded pump lost ")
            && contains_fixed(&raw, " of 64 bytes to the receive queue's overflow; the transfer answered -110")
            && !contains_fixed(&raw, "[api] PASS bus-");
        if !bit {
            self.emit_control_timed("bus-rp2350-doctored-control", "fail", BUS_RP2350_DOCTORED_BROKE, code, 0, &sha, &path_rel, Some("icount-shift0"));
            self.ctx.gate_fail("bus-rp2350-doctored-control: the doctored models' pinned lines were not all read, or a bus check passed (the control did not bite)");
        }
        // The ending: the refusals are counted and the image walks on to its exit door.
        if let Err(why) = run_ending(code, 0, &raw) {
            self.emit_control_timed("bus-rp2350-doctored-control", "fail", BUS_RP2350_DOCTORED_BROKE, code, 0, &sha, &path_rel, Some("icount-shift0"));
            self.ctx.gate_fail(&format!("bus-rp2350-doctored-control: {why} (the image must walk on to the exit door)"));
        }
        self.emit_control_timed("bus-rp2350-doctored-control", "pass", BUS_RP2350_DOCTORED_BROKE, code, 0, &sha, &path_rel, Some("icount-shift0"));
        println!("== control bus-rp2350-doctored-control: PASS (the data byte refused answered EIO at message 0 byte 1, the clock that never completed answered ETIMEDOUT inside the budget with the block reset once, the unbounded pump lost bytes to the receive queue's overrun and timed out; no bus check passed; exit 0 through the exit door)");
    }

    /// The emulator rows' bus control: the api flavor's doctored image on the
    /// Cortex-M33 twin, where the sensor model refuses the second data byte of
    /// a write (the door must answer EIO and name the byte), the block model
    /// never completes a non-last message (the door must answer ETIMEDOUT
    /// inside its budget and reset the block once) and the drivers' pumps run
    /// unbounded (the receive queue's honest overflow then loses bytes and the
    /// exchange times out), and the SPI driver's first transfer after the claim
    /// takes the wrong write order (the block model must fall out of
    /// master mode and the call must answer EIO at once with its own line): the five
    /// lines present, no bus check's PASS line.
    ///
    /// The control runs on the Deterministic execution profile: its
    /// judge in the image counts ticks of the guest's clock across two door
    /// calls ("at once", "inside the budget"), a count-class claim the plain
    /// profile exposes to the host's timing -- a parked emulator thread reads
    /// as ticks inside a few-hundred-instruction refusal. Under the
    /// instruction-counting clock the counts are the guest's own. The doctored
    /// inputs are unchanged; every positive case keeps the Parity profile.
    fn run_bus_doctored_control(&mut self) {
        let CaseRun { sha, path_rel, raw, exit: code } = self.build_and_run_an505_with("bus-doctored-control", "qemu-exit,board-mps2-an505,entropy-accept-software,api-smoke,test-bus-doctored", ExecProfile::Deterministic, BUS_CONTROL_WORK);
        let bit = contains_fixed(&raw, "[api] bus-doctored: the data byte not acknowledged answered -5 at message 0 byte 1")
            && contains_fixed(&raw, "[api] bus-doctored: the clock that never completed answered -110 inside the budget; the block reset once")
            && contains_fixed(&raw, "[bus] spi 1: the block fell out of master mode at its configuration; the fault cleared, nothing clocked")
            && contains_fixed(&raw, "[api] bus-doctored: the wrong write order answered -5 at once; the block fell out of master mode and the fault was cleared")
            && contains_fixed(&raw, "[bus] i2c 1: the bus recovered after the timeout (a device held the data line low; clocked free, then a stop)")
            && contains_fixed(&raw, "[bus] i2c 1: the bus recovered after the timeout (a device held the data line low through nine clocks; the clock held low 35 ms, then a stop)")
            && contains_fixed(&raw, "[bus] i2c 1: the data line is still held low after the timeout's recovery (nine clocks, the clock held low 35 ms, a stop); while it stays so, every transfer on this bus fails")
            && contains_fixed(&raw, "[api] bus-doctored: a data line held after the timeout came free by the clocks; the bus recovered and was counted")
            && contains_fixed(&raw, "[api] bus-doctored: a data line held through nine clocks came free by the clock held low 35 ms; the bus recovered and was counted")
            && contains_fixed(&raw, "[api] bus-doctored: a data line held through the whole recovery was told and not counted as recovered")
            && contains_fixed(&raw, "[api] bus-doctored: a device that held the clock low 25 ms in one byte was served")
            && contains_fixed(&raw, "[api] bus-doctored: the unbounded pump lost ")
            && contains_fixed(&raw, " of 64 bytes to the receive queue's overflow; the transfer answered -110")
            && !contains_fixed(&raw, "[api] PASS bus-");
        if !bit {
            self.emit_control_timed("bus-doctored-control", "fail", BUS_DOCTORED_BROKE, code, 0, &sha, &path_rel, Some("icount-shift0"));
            self.ctx.gate_fail("bus-doctored-control: the doctored models' and the wrong write order's pinned lines were not all read, or a bus check passed (the control did not bite)");
        }
        // The ending: the refusals are counted and the image walks on to its exit door.
        if let Err(why) = run_ending(code, 0, &raw) {
            self.emit_control_timed("bus-doctored-control", "fail", BUS_DOCTORED_BROKE, code, 0, &sha, &path_rel, Some("icount-shift0"));
            self.ctx.gate_fail(&format!("bus-doctored-control: {why} (the image must walk on to the exit door)"));
        }
        self.emit_control_timed("bus-doctored-control", "pass", BUS_DOCTORED_BROKE, code, 0, &sha, &path_rel, Some("icount-shift0"));
        println!("== control bus-doctored-control: PASS (the data byte refused answered EIO at message 0 byte 1, the clock that never completed answered ETIMEDOUT inside the budget with the block reset once, the wrong write order answered EIO at once with the block out of master mode, the unbounded pump lost bytes to the receive queue's overflow and timed out; no bus check's PASS line; on the deterministic profile; the image reached the exit door, exit 0)");
    }

    /// The emulator rows' PWM control: the virtual timer's own reading of
    /// the period it runs made one prescaler step long; the api flavor's
    /// claim check ties the door's achieved period to the timer's own and
    /// must refuse -- the refusal's words, no PASS line, the image dead by
    /// its panic.
    fn run_pwm_doctored_control(&mut self) {
        let run = self.build_and_run_echo("pwm-doctored-control", &["--features", "api-smoke,test-pwm-doctored"]);
        let (sha, path_rel, code) = (run.sha.clone(), run.path_rel.clone(), run.exit);
        let refused = !contains_fixed(&run.raw, "[api] PASS pwm-claim (") && contains_fixed(&run.raw, "pwm-claim: the door reports");
        if !refused {
            self.emit_control("pwm-doctored-control", "fail", PWM_DOCTORED_BROKE, code, 1, &sha, &path_rel);
            self.ctx.gate_fail("pwm-doctored-control: the doctored timer's claim check passed, or died elsewhere than the tie (the control did not bite)");
        }
        if let Err(why) = run_ending(code, 1, &run.raw) {
            self.emit_control("pwm-doctored-control", "fail", PWM_DOCTORED_BROKE, code, 1, &sha, &path_rel);
            self.ctx.gate_fail(&format!("pwm-doctored-control: {why} (the control did not die as designed)"));
        }
        self.emit_control("pwm-doctored-control", "pass", PWM_DOCTORED_BROKE, code, 1, &sha, &path_rel);
        println!("== control pwm-doctored-control: PASS (the virtual timer running its prescaler one step long is refused by the claim check's tie of the door's period to the timer's own: the refusal's words, no PASS line; the image died by its panic, exit 1)");
    }

    /// The emulator rows' ADC control: the virtual loop channel answering
    /// the previous conversion's sample; the api flavor's loop check reads
    /// a stale sample at its second duty cycle and must refuse -- the
    /// refusal's words, no PASS line, the image dead by its panic.
    fn run_adc_doctored_control(&mut self) {
        let run = self.build_and_run_echo("adc-doctored-control", &["--features", "api-smoke,test-adc-doctored"]);
        let (sha, path_rel, code) = (run.sha.clone(), run.path_rel.clone(), run.exit);
        let refused = !contains_fixed(&run.raw, "[api] PASS pwm-adc-loop (") && contains_fixed(&run.raw, "pwm-adc-loop: the loop read");
        if !refused {
            self.emit_control("adc-doctored-control", "fail", ADC_DOCTORED_BROKE, code, 1, &sha, &path_rel);
            self.ctx.gate_fail("adc-doctored-control: the doctored converter's loop check passed, or died elsewhere than the reading (the control did not bite)");
        }
        if let Err(why) = run_ending(code, 1, &run.raw) {
            self.emit_control("adc-doctored-control", "fail", ADC_DOCTORED_BROKE, code, 1, &sha, &path_rel);
            self.ctx.gate_fail(&format!("adc-doctored-control: {why} (the control did not die as designed)"));
        }
        self.emit_control("adc-doctored-control", "pass", ADC_DOCTORED_BROKE, code, 1, &sha, &path_rel);
        println!("== control adc-doctored-control: PASS (the virtual loop channel answering a stale sample is refused by the loop check at its second reading: the refusal's words, no PASS line; the image died by its panic, exit 1)");
    }

    /// The converter's conversion-error path on the emulator row's virtual
    /// converter (`test-adc-flagged`): the api flavor's adc-flagged check must
    /// PASS inside a walk that ends through the exit door -- the flagged first
    /// conversion of one channel discarded and retried, the channel flagged on
    /// every conversion refused with the input/output code after the bound
    /// with no sample, the first channel read again after it.
    fn run_adc_flagged_case(&mut self) {
        let name = "adc-flagged";
        let run = self.build_and_run_echo(name, &["--features", "api-smoke,test-adc-flagged"]);
        let (sha, path_rel, code) = (run.sha.clone(), run.path_rel.clone(), run.exit);
        if let Err(why) = run_ending(code, 0, &run.raw) {
            self.fail_case(name, code, 0, &sha, &path_rel, &why);
        }
        let mut markers: Vec<&str> = vec!["flavor: api-smoke", MARKER_EXIT_DOOR, "[api] PASS adc-flagged (", "[api] PASS pwm-adc-loop ("];
        markers.extend(MARKERS_B);
        self.common_asserts(name, &run, 0, &markers, &["EXCEPTION:", "KERNEL PANIC"]);
        self.emit_case(name, "pass", code, 0, &sha, &path_rel);
        println!("== case {name}: PASS (the flagged first conversion discarded and retried, the channel flagged on every conversion refused EIO after the bound with no sample, the first channel read again after it; exit 0 through the exit door)");
    }

    /// The doctored converter answering a flagged conversion's undefined
    /// result as a sample (`test-adc-flagged-doctored`): the adc-flagged check
    /// must refuse at its first reading -- the refusal's words, no PASS line,
    /// the image dead by its panic.
    fn run_adc_flagged_doctored_control(&mut self) {
        let name = "adc-flagged-doctored-control";
        let run = self.build_and_run_echo(name, &["--features", "api-smoke,test-adc-flagged,test-adc-flagged-doctored"]);
        let (sha, path_rel, code) = (run.sha.clone(), run.path_rel.clone(), run.exit);
        let refused = !contains_fixed(&run.raw, "[api] PASS adc-flagged (") && contains_fixed(&run.raw, "adc-flagged: channel");
        if !refused {
            self.emit_control(name, "fail", ADC_FLAGGED_DOCTORED_BROKE, code, 1, &sha, &path_rel);
            self.ctx.gate_fail("adc-flagged-doctored-control: the doctored converter's flagged conversion passed the check as a sample, or the image died elsewhere than the reading (the control did not bite)");
        }
        if let Err(why) = run_ending(code, 1, &run.raw) {
            self.emit_control(name, "fail", ADC_FLAGGED_DOCTORED_BROKE, code, 1, &sha, &path_rel);
            self.ctx.gate_fail(&format!("adc-flagged-doctored-control: {why} (the control did not die as designed)"));
        }
        self.emit_control(name, "pass", ADC_FLAGGED_DOCTORED_BROKE, code, 1, &sha, &path_rel);
        println!("== control adc-flagged-doctored-control: PASS (the doctored converter answering a flagged conversion's undefined result as a sample is refused by the adc-flagged check at its first reading: the refusal's words, no PASS line; the image died by its panic) ==");
    }

    /// The RAM-range doors' control (every row; run on the default row): the
    /// count door answering one range more than the board's facts state, with
    /// a record the range door fills for it; the api flavor's ram-range check
    /// must refuse the count against the facts -- the refusal's words, no
    /// PASS line, the image dead by its panic.
    fn run_ram_doctored_control(&mut self) {
        let run = self.build_and_run_echo("ram-doctored-control", &["--features", "api-smoke,test-ram-doctored"]);
        let (sha, path_rel, code) = (run.sha.clone(), run.path_rel.clone(), run.exit);
        let refused = !contains_fixed(&run.raw, "[api] PASS ram-range (") && contains_fixed(&run.raw, "ram-range: the count reads 3 against the 2 this image serves by its facts");
        if !refused {
            self.emit_control("ram-doctored-control", "fail", RAM_DOCTORED_BROKE, code, 1, &sha, &path_rel);
            self.ctx.gate_fail("ram-doctored-control: the doctored count's ram-range check passed, or died elsewhere than the count (the control did not bite)");
        }
        if let Err(why) = run_ending(code, 1, &run.raw) {
            self.emit_control("ram-doctored-control", "fail", RAM_DOCTORED_BROKE, code, 1, &sha, &path_rel);
            self.ctx.gate_fail(&format!("ram-doctored-control: {why} (the control did not die as designed)"));
        }
        self.emit_control("ram-doctored-control", "pass", RAM_DOCTORED_BROKE, code, 1, &sha, &path_rel);
        println!("== control ram-doctored-control: PASS (the count door answering a range the facts do not state is refused by the ram-range check at its first assert: the refusal's words, no PASS line; the image died by its panic, exit 1)");
    }

    /// The lazy init modes on one emulator row (window 0 follows the mode
    /// under `test-ram-init-window0`, the boot poison kept): the api flavor
    /// under `ram-init-first-use` must pass its arena family and its
    /// ram-range check over the lazy window and leave through the exit door;
    /// under `ram-init-background` the same, plus the ram-init-frontier
    /// check; the doctored control (the hand-out's zeroing skipped) must be
    /// refused by the zero-birth check at its first scan -- the refusal's
    /// words, no PASS line, the image dead by its panic.
    fn run_ram_init(&mut self, an505: bool) {
        let suffix = if an505 { "-an505" } else { "" };
        let mpu = if an505 { MARKER_MPU_AN505 } else { MARKER_MPU_AN500 };
        let mut markers: Vec<&str> = vec!["flavor: api-smoke", mpu, MARKER_EXIT_DOOR, "[api] PASS ram-range ("];
        markers.extend(MARKERS_B);

        let first = format!("api-ram-init-first-use{suffix}");
        let run = self.build_and_run_row(an505, &first, "api-smoke,test-ram-init-window0,ram-init-first-use");
        let (sha, path_rel, code) = (run.sha.clone(), run.path_rel.clone(), run.exit);
        if let Err(why) = run_ending(code, 0, &run.raw) {
            self.fail_case(&first, code, 0, &sha, &path_rel, &why);
        }
        self.common_asserts(&first, &run, 0, &markers, &["EXCEPTION:", "KERNEL PANIC"]);
        self.emit_case(&first, "pass", code, 0, &sha, &path_rel);
        println!("== case {first}: PASS (window 0 lazy under the first-use mode: the arena family and the ram-range check PASS over it through the boot poison -- the hand-out's own zeroing; exit 0 through the exit door)");

        let bg = format!("api-ram-init-background{suffix}");
        let mut markers_bg = markers.clone();
        markers_bg.push("[api] PASS ram-init-frontier (");
        let run = self.build_and_run_row(an505, &bg, "api-smoke,test-ram-init-window0,ram-init-background");
        let (sha, path_rel, code) = (run.sha.clone(), run.path_rel.clone(), run.exit);
        if let Err(why) = run_ending(code, 0, &run.raw) {
            self.fail_case(&bg, code, 0, &sha, &path_rel, &why);
        }
        self.common_asserts(&bg, &run, 0, &markers_bg, &["EXCEPTION:", "KERNEL PANIC"]);
        self.emit_case(&bg, "pass", code, 0, &sha, &path_rel);
        println!("== case {bg}: PASS (window 0 lazy under the background mode: the arena family and the ram-range check PASS over it; the frontier check -- the straddling page zero-born by the hand-out, the frontier carried to the window's end by the idle steps, the page below it zero-born by the idle step alone; exit 0 through the exit door)");

        let ctl = format!("ram-init-doctored-control{suffix}");
        let run = self.build_and_run_row(an505, &ctl, "api-smoke,test-ram-init-window0,ram-init-first-use,test-ram-init-doctored");
        let (sha, path_rel, code) = (run.sha.clone(), run.path_rel.clone(), run.exit);
        let refused = !contains_fixed(&run.raw, "[api] PASS arena-zero-birth (") && contains_fixed(&run.raw, "zero-birth: a handed-out page reads nonzero");
        if !refused {
            self.emit_control(&ctl, "fail", RAM_INIT_DOCTORED_BROKE, code, 1, &sha, &path_rel);
            self.ctx.gate_fail(&format!("{ctl}: the unzeroed hand-out's zero-birth check passed, or the image died elsewhere than that check (the control did not bite)"));
        }
        if let Err(why) = run_ending(code, 1, &run.raw) {
            self.emit_control(&ctl, "fail", RAM_INIT_DOCTORED_BROKE, code, 1, &sha, &path_rel);
            self.ctx.gate_fail(&format!("{ctl}: {why} (the control did not die as designed)"));
        }
        self.emit_control(&ctl, "pass", RAM_INIT_DOCTORED_BROKE, code, 1, &sha, &path_rel);
        println!("== control {ctl}: PASS (the lazy hand-out's zeroing skipped is refused by the zero-birth check at its first scan: the refusal's words, no PASS line; the image died by its panic, exit 1)");

        let ctl2 = format!("ram-init-doctored-section-control{suffix}");
        let run = self.build_and_run_row(an505, &ctl2, "api-smoke,test-ram-init-window0,ram-init-background,test-ram-init-doctored-section");
        let (sha, path_rel, code) = (run.sha.clone(), run.path_rel.clone(), run.exit);
        let refused = !contains_fixed(&run.raw, "[api] PASS ram-init-frontier (") && contains_fixed(&run.raw, "ram-init-frontier: no tick served while a piece was zeroing");
        if !refused {
            self.emit_control(&ctl2, "fail", RAM_INIT_DOCTORED_SECTION_BROKE, code, 1, &sha, &path_rel);
            self.ctx.gate_fail(&format!("{ctl2}: the frontier check's witness passed with the memset under the section, or the image died elsewhere than the witness (the control did not bite)"));
        }
        if let Err(why) = run_ending(code, 1, &run.raw) {
            self.emit_control(&ctl2, "fail", RAM_INIT_DOCTORED_SECTION_BROKE, code, 1, &sha, &path_rel);
            self.ctx.gate_fail(&format!("{ctl2}: {why} (the control did not die as designed)"));
        }
        self.emit_control(&ctl2, "pass", RAM_INIT_DOCTORED_SECTION_BROKE, code, 1, &sha, &path_rel);
        println!("== control {ctl2}: PASS (the idle step's zeroing held under the critical section is refused by the frontier check's witness: no tick served mid-piece, the refusal's words, no PASS line; the image died by its panic, exit 1)");
    }

    /// The band-interrupt control's own control: the tick hook pends nothing
    /// while the fp-band check runs, so the armed group's repetitions end at
    /// the check's bound of the kernel's ticks with no hit, and the check must
    /// refuse with its own words -- no PASS line, no landing line, the image
    /// dead by its panic. Runs on the plain profile: the outcome is fixed in
    /// guest time, the bound being the kernel's own tick count.
    fn run_fp_band_doctored_control(&mut self) {
        let run = self.build_and_run_echo("fp-band-doctored-control", &["--features", "api-smoke,test-fp-band-doctored"]);
        let (sha, path_rel, code) = (run.sha.clone(), run.path_rel.clone(), run.exit);
        let refused = !contains_fixed(&run.raw, "[api] PASS fp-band (")
            && !contains_fixed(&run.raw, MARKERS_M[2])
            && contains_fixed(&run.raw, "fp-band: the band exception never fired");
        if !refused {
            self.emit_control("fp-band-doctored-control", "fail", FP_BAND_DOCTORED_BROKE, code, 1, &sha, &path_rel);
            self.ctx.gate_fail("fp-band-doctored-control: the silenced tick hook's fp-band check passed, printed a landing, or died elsewhere than its own assert (the control did not bite)");
        }
        if let Err(why) = run_ending(code, 1, &run.raw) {
            self.emit_control("fp-band-doctored-control", "fail", FP_BAND_DOCTORED_BROKE, code, 1, &sha, &path_rel);
            self.ctx.gate_fail(&format!("fp-band-doctored-control: {why} (the control did not die as designed)"));
        }
        self.emit_control("fp-band-doctored-control", "pass", FP_BAND_DOCTORED_BROKE, code, 1, &sha, &path_rel);
        println!("== control fp-band-doctored-control: PASS (the tick hook pending nothing is refused by the fp-band check at its bound of the kernel's ticks: the never-fired words, no PASS line, no landing line; the image died by its panic, exit 1)");
    }

    /// The rezero band's control: the rezero's memset under the critical
    /// section (`test-rezero-doctored`), so no tick is served while
    /// a memset is in flight and the arena-rezero-band check must refuse at
    /// its bound of the kernel's ticks with its own words -- no PASS line, the
    /// image dead by its panic. Runs on the plain profile: the outcome is
    /// fixed in guest time, the bound being the kernel's own tick count.
    fn run_rezero_doctored_control(&mut self) {
        let run = self.build_and_run_echo("rezero-doctored-control", &["--features", "api-smoke,test-rezero-doctored"]);
        let (sha, path_rel, code) = (run.sha.clone(), run.path_rel.clone(), run.exit);
        let refused = !contains_fixed(&run.raw, "[api] PASS arena-rezero-band (")
            && contains_fixed(&run.raw, "arena-rezero-band: no tick landed inside the rezero's memset");
        if !refused {
            self.emit_control("rezero-doctored-control", "fail", REZERO_DOCTORED_BROKE, code, 1, &sha, &path_rel);
            self.ctx.gate_fail("rezero-doctored-control: the memset held under the section passed the arena-rezero-band check, or the image died elsewhere than its own assert (the control did not bite)");
        }
        if let Err(why) = run_ending(code, 1, &run.raw) {
            self.emit_control("rezero-doctored-control", "fail", REZERO_DOCTORED_BROKE, code, 1, &sha, &path_rel);
            self.ctx.gate_fail(&format!("rezero-doctored-control: {why} (the control did not die as designed)"));
        }
        self.emit_control("rezero-doctored-control", "pass", REZERO_DOCTORED_BROKE, code, 1, &sha, &path_rel);
        println!("== control rezero-doctored-control: PASS (the memset held under the critical section is refused by the arena-rezero-band check at its bound of the kernel's ticks: no tick landed inside it, no PASS line; the image died by its panic, exit 1)");
    }

    /// The case for the gpio-refuse check's skip path (the shape of the
    /// Pimoroni rows): the api
    /// flavor built with the emulator row's facts claiming NO line for the
    /// board (`test-no-board-claims`, the Pico rows' shape under the UART contract's memory
    /// console), so the check skips its board-line assertion and says so, its
    /// witness bound holds at a floor of 0 EBUSY, the pwm-claim check's
    /// board-line assertion is skipped the same way, and the image walks on
    /// to the exit door. A bound of 1 EBUSY where the assertion is skipped
    /// would kill the image on every Pico row, and the emulator rows alone would
    /// never show it, because their facts claim the virtual console's line. A positive case, not a
    /// doctored control: the row's shape is the one under test.
    fn run_no_board_claim(&mut self) {
        let run = self.build_and_run_echo("api-no-board-claim", &["--features", "api-smoke,test-no-board-claims"]);
        let (sha, path_rel, code) = (run.sha.clone(), run.path_rel.clone(), run.exit);
        if let Err(why) = run_ending(code, 0, &run.raw) {
            self.fail_case("api-no-board-claim", code, 0, &sha, &path_rel, &why);
        }
        let markers: [&str; 7] = [
            "flavor: api-smoke",
            MARKER_MPU_AN500,
            "[boot] gpio: 6 virtual lines (5 populated), 0 claimed by the board (the control's shape: test-no-board-claims)",
            "[api] gpio-refuse: no line is claimed by the board on this row by default (the memory console takes no UART pin); the board-line assertion is skipped",
            "[api] PASS gpio-refuse (",
            "[api] PASS pwm-claim (",
            MARKER_EXIT_DOOR,
        ];
        self.common_asserts("api-no-board-claim", &run, 0, &markers, &["EXCEPTION:", "KERNEL PANIC", "gpio-refuse: the witness counts"]);
        self.emit_case("api-no-board-claim", "pass", code, 0, &sha, &path_rel);
        println!("== case api-no-board-claim: PASS (the emulator row's facts claiming no line for the board -- the Pico rows' shape: the gpio-refuse check skipped its board-line assertion and said so, its witness bound held at the floor of 0 EBUSY, the pwm-claim check skipped its own; the api flavor whole to the exit door, exit 0)");
    }


    /// The four network controls on the established row (the api flavor
    /// with the `net` feature over the virtual medium): (1) the scripted peer
    /// that never answers an ARP request -- the refuse check's connect to the
    /// peer's one port, reset on the wire in the honest image, ends at the
    /// connect budget with RHU_EHOSTUNREACH pending and the check refuses the
    /// code (the words, no PASS line, the image dead by its panic); (2) the
    /// network thread that bumps the global word every pass without a mask
    /// change -- the port check's idle wait wakes early and refuses the storm;
    /// (3) the slow pump, half a second of busy work per pass -- the pump
    /// check's longest pass passes the bound; (4) the doctored budget table,
    /// a stream buffer a megabyte wide -- the compile-time assertion against
    /// the row's RAM window refuses the build with its words (no image).
    fn run_net_controls(&mut self) {
        let run = self.build_and_run_echo("net-arp-silent-control", &["--features", "api-smoke,net,test-net-doctored-arp"]);
        let (sha, path_rel, code) = (run.sha.clone(), run.path_rel.clone(), run.exit);
        let refused = !contains_fixed(&run.raw, "[api] PASS net-refuse (") && contains_fixed(&run.raw, "net-refuse: the connect to the peer's port read -113 where ECONNREFUSED was expected");
        if !refused {
            self.emit_control("net-arp-silent-control", "fail", NET_ARP_SILENT_BROKE, code, 1, &sha, &path_rel);
            self.ctx.gate_fail("net-arp-silent-control: the refuse check passed against the silent peer, or died elsewhere than the pending code (the control did not bite)");
        }
        if let Err(why) = run_ending(code, 1, &run.raw) {
            self.emit_control("net-arp-silent-control", "fail", NET_ARP_SILENT_BROKE, code, 1, &sha, &path_rel);
            self.ctx.gate_fail(&format!("net-arp-silent-control: {why} (the control did not die as designed)"));
        }
        self.emit_control("net-arp-silent-control", "pass", NET_ARP_SILENT_BROKE, code, 1, &sha, &path_rel);
        println!("== control net-arp-silent-control: PASS (the peer that never answers an ARP request: the connect to its port ended at the budget with EHOSTUNREACH pending where the honest image reads the peer's reset, and the refuse check refused the code -- the words, no PASS line; the image died by its panic, exit 1)");

        let run = self.build_and_run_echo("net-bump-doctored-control", &["--features", "api-smoke,net,test-net-doctored-bump"]);
        let (sha, path_rel, code) = (run.sha.clone(), run.path_rel.clone(), run.exit);
        let refused = !contains_fixed(&run.raw, "[api] PASS net-port (") && contains_fixed(&run.raw, "net-port: the global word moved ");
        if !refused {
            self.emit_control("net-bump-doctored-control", "fail", NET_BUMP_DOCTORED_BROKE, code, 1, &sha, &path_rel);
            self.ctx.gate_fail("net-bump-doctored-control: the port check passed against the storming thread, or died elsewhere than the storm (the control did not bite)");
        }
        if let Err(why) = run_ending(code, 1, &run.raw) {
            self.emit_control("net-bump-doctored-control", "fail", NET_BUMP_DOCTORED_BROKE, code, 1, &sha, &path_rel);
            self.ctx.gate_fail(&format!("net-bump-doctored-control: {why} (the control did not die as designed)"));
        }
        self.emit_control("net-bump-doctored-control", "pass", NET_BUMP_DOCTORED_BROKE, code, 1, &sha, &path_rel);
        println!("== control net-bump-doctored-control: PASS (the network thread bumping the global word every pass without a mask change: the port check's idle wait woke early and refused the storm -- the words, no PASS line; the image died by its panic, exit 1)");

        let run = self.build_and_run_echo("net-slow-pump-control", &["--features", "api-smoke,net,test-net-doctored-slow"]);
        let (sha, path_rel, code) = (run.sha.clone(), run.path_rel.clone(), run.exit);
        let refused = !contains_fixed(&run.raw, "[api] PASS net-pump (") && contains_fixed(&run.raw, "net-pump: the longest pass took ");
        if !refused {
            self.emit_control("net-slow-pump-control", "fail", NET_SLOW_PUMP_BROKE, code, 1, &sha, &path_rel);
            self.ctx.gate_fail("net-slow-pump-control: the pump check passed against the slow pump, or died elsewhere than the pass bound (the control did not bite)");
        }
        if let Err(why) = run_ending(code, 1, &run.raw) {
            self.emit_control("net-slow-pump-control", "fail", NET_SLOW_PUMP_BROKE, code, 1, &sha, &path_rel);
            self.ctx.gate_fail(&format!("net-slow-pump-control: {why} (the control did not die as designed)"));
        }
        self.emit_control("net-slow-pump-control", "pass", NET_SLOW_PUMP_BROKE, code, 1, &sha, &path_rel);
        println!("== control net-slow-pump-control: PASS (the network thread spending half a second per pass: the pump check's longest pass passed the 400 ms bound and the check refused -- the words, no PASS line; the image died by its panic, exit 1)");

        // The budget control is a build, not a run: the doctored table must be refused by the compile-time assertion.
        println!("== control net-budget-doctored-control ==");
        let target_dir = self.ctx.repo.join("src").join("kernel").join("target").join("api-gate-net-budget");
        let log = self.ctx.out.join("build-net-budget-doctored-control.log");
        let ok = cargo_build_kernel_env(&self.ctx.repo, &target_dir, &["--features", "api-smoke,net,test-net-doctored-budget,entropy-accept-software"], &[], &log);
        let text = std::fs::read_to_string(&log).unwrap_or_default();
        let refused = matches!(ok, Ok(false)) && text.contains("the network pool exceeds a quarter of the row's RAM window");
        // A build refused at compile time has no image: the artifact of record is the build log with the assertion's
        // words, pinned by its sha256 like an image would be (the results schema wants a real artifact in every record).
        let log_sha = crate::sha256::sha256_file(&log).unwrap_or_default();
        let log_rel = log.strip_prefix(&self.ctx.repo).map(|p| p.to_string_lossy().replace('\\', "/")).unwrap_or_else(|_| log.to_string_lossy().replace('\\', "/"));
        if !refused {
            echo_file(&log, "  | ");
            self.emit_control("net-budget-doctored-control", "fail", NET_BUDGET_DOCTORED_BROKE, if matches!(ok, Ok(true)) { 0 } else { 1 }, 1, &log_sha, &log_rel);
            self.ctx.gate_fail("net-budget-doctored-control: the doctored budget table built, or the build failed without the assertion's words (the control did not bite)");
        }
        self.emit_control("net-budget-doctored-control", "pass", NET_BUDGET_DOCTORED_BROKE, 1, 1, &log_sha, &log_rel);
        println!("== control net-budget-doctored-control: PASS (the budget table with a stream buffer a megabyte wide refused at compile time by the assertion against the row's RAM window, with its words in the build log; no image)");
    }

    /// A case record for a negative control of this gate, naming what the
    /// control broke and carrying the run's observed exit beside the wanted one.
    fn emit_control(&mut self, case: &str, verdict: &str, broke: &str, code: i32, want: i32, sha: &str, path_rel: &str) {
        self.emit_control_timed(case, verdict, broke, code, want, sha, path_rel, None);
    }

    /// The same record with the additive, report-only `timing` field the
    /// payload gate's count-class records carry, naming the execution profile
    /// the control ran on (the bus control on the Deterministic profile).
    fn emit_control_timed(&mut self, case: &str, verdict: &str, broke: &str, code: i32, want: i32, sha: &str, path_rel: &str, timing: Option<&str>) {
        let clk = match timing {
            Some(t) => format!(",\"timing\":\"{t}\""),
            None => String::new(),
        };
        self.ctx.results.emit(&format!(
            "{{\"schema\":1,\"type\":\"case\",\"gate\":\"api\",\"case\":\"{case}\",\"verdict\":\"{verdict}\",\"exit\":{code},\"exit_want\":{want},\"control\":\"negative\",\"broke\":\"{broke}\"{clk},\"image\":{{\"path\":\"{path_rel}\",\"sha256\":\"{sha}\"}},\"golden\":null}}"
        ));
        self.ctx.results.tally(verdict, "negative");
    }

    /// An api-smoke run of the established row with the wire's echo on
    /// the second CMSDK UART: a free loopback port and the echo thread
    /// taken after the build and before the run, the thread joined after
    /// it, the bytes echoed printed. The api flavor reaches its UART
    /// checks unless it dies before them, so every api-smoke run carries
    /// the echo; an image that never claims the port leaves the echo idle.
    fn build_and_run_echo(&mut self, name: &str, features: &[&str]) -> CaseRun {
        let staged = self.ctx.build_case(name, features);
        let (chardev, echo) = self.start_echo(name);
        let run = self.ctx.run_staged(name, &staged, ExecProfile::Parity, Some(&chardev));
        self.end_echo(name, echo);
        run
    }

    /// The echo's start, through the shared module: the chardev argument
    /// and the thread.
    fn start_echo(&mut self, name: &str) -> (String, Echo) {
        match echo::start_echo(self.ctx.timeout) {
            Ok(x) => x,
            Err(e) => self.ctx.gate_fail(&format!("{name}: {e}")),
        }
    }

    /// The echo's end: the thread joined, its count printed, its complaint
    /// the gate's failure.
    fn end_echo(&mut self, name: &str, wire: Echo) {
        match echo::end_echo(wire) {
            Ok(n) => println!("  (the wire's echo on the second UART: {n} bytes echoed)"),
            Err(e) => self.ctx.gate_fail(&format!("{name}: {e}")),
        }
    }

    /// The six boot cases: the boot flavor routed through the environment
    /// words at the build (the crate's build script reads them into the
    /// image's link-time words), one target dir per row so the routed
    /// builds never move the gate's plain images; each run's wire judged
    /// exactly against the route's expectation and its exit 0.
    fn run_boot_images(&mut self) {
        let version = match kernel_version(&self.ctx.repo) {
            Ok(v) => v,
            Err(e) => self.ctx.gate_fail(&format!("boot images: {e}")),
        };
        for row in BOOT_ROWS.iter() {
            let line = format!("Lamella RHU v{version}, {}\r\n", row.banner);
            for (case, env, prints) in BOOT_ROUTES.iter() {
                let name = format!("{case}-{}", row.row);
                let want: &[u8] = if *prints { line.as_bytes() } else { b"" };
                let run = self.build_and_run_boot(&name, row, env);
                let (sha, path_rel, code) = (run.sha.clone(), run.path_rel.clone(), run.exit);
                if let Some(f) = boot_transcript_fault(&run.raw, want) {
                    self.fail_case(&name, code, 0, &sha, &path_rel, &f);
                }
                if code != 0 {
                    self.fail_case(&name, code, 0, &sha, &path_rel, &format!("exit {code}, wanted 0"));
                }
                self.emit_case(&name, "pass", code, 0, &sha, &path_rel);
                let what = match *case {
                    "boot-quiet" => format!("the quiet image's wire is the one banner line exactly, {} bytes with its CRLF", want.len()),
                    "boot-silent" => "the silent image's wire is empty".to_string(),
                    _ => "the memory-routed image's wire is empty, and its exit 0 says its mirror holds the banner".to_string(),
                };
                println!("== case {name}: PASS ({what}; exit 0) ==");
            }
        }
    }

    /// Build the boot flavor with environment words into the row's own
    /// boot target dir, stage it, run it on the row's machine and return
    /// the case record (no second chardev: the boot flavor claims no UART).
    fn build_and_run_boot(&mut self, name: &str, row: &BootRow, env: &[(&str, &str)]) -> CaseRun {
        println!("== case {name} ==");
        let target_dir = self.ctx.repo.join("src").join("kernel").join("target").join(format!("api-gate-boot-{}", row.row));
        let log = self.ctx.out.join(format!("build-{name}.log"));
        let ok = cargo_build_kernel_env(&self.ctx.repo, &target_dir, row.extra, env, &log);
        if !matches!(ok, Ok(true)) {
            echo_file(&log, "");
            self.ctx.gate_fail(&format!("{name}: build error"));
        }
        let built = target_dir.join(row.built);
        let staged = self.ctx.out.join(format!("rhu-kernel-{name}.elf"));
        if std::fs::copy(&built, &staged).is_err() {
            self.ctx.gate_fail(&format!("{name}: built kernel not found at {}", built.display()));
        }
        let sha = match sha256_file(&staged) {
            Ok(s) => s,
            Err(e) => self.ctx.gate_fail(&format!("{name}: cannot hash staged artifact: {e}")),
        };
        let path_rel = format!("{}/rhu-kernel-{name}.elf", self.ctx.out_rel);
        let raw_path = self.ctx.out.join(format!("{name}.raw"));
        let stderr_path = self.ctx.out.join(format!("{name}.stderr"));
        let staged_arg = staged.to_string_lossy().into_owned();
        let args: Vec<&str> = vec!["-M", row.machine, "-display", "none", "-monitor", "none", "-serial", "stdio", "-semihosting", "-kernel", staged_arg.as_str()];
        let outcome = match run_captured(&self.ctx.qemu, &args, &raw_path, &stderr_path, self.ctx.timeout) {
            Ok(o) => o,
            Err(e) => self.ctx.gate_fail(&format!("{name}: {e}")),
        };
        if outcome.timed_out {
            println!("  (run killed at its timeout; a hang is observed, never a pass)");
        }
        let raw = std::fs::read(&raw_path).unwrap_or_default();
        echo_file(&raw_path, "  | ");
        CaseRun { sha, path_rel, raw, exit: outcome.exit }
    }

    /// Build the kernel for the mps2-an505 row into the gate's own second
    /// target directory, stage it, run it on the an505 machine, and return
    /// the family's case record: the raw transcript, the staged image's
    /// identity and the run's exit (124 when the run was killed at its
    /// deadline, the family's convention), judged by the case like any
    /// run of the established row.
    fn build_and_run_an505(&mut self, name: &str, feats: &str) -> CaseRun {
        self.build_and_run_an505_with(name, feats, ExecProfile::Parity, 0)
    }

    /// build_and_run_an505 under an explicit execution profile: the
    /// Deterministic profile's argv is inserted immediately before `-kernel`,
    /// as `GateCtx::build_and_run_with` inserts it; the Parity profile inserts
    /// nothing. A count-class case opts in per case, deliberately -- never by
    /// default. An instruction-counted run is judged by the processor time
    /// the host gave the emulator: its budget is `work` units of the host's
    /// unit of emulator work, never under the gate's deadline. A run on the
    /// plain profile is judged by the wall clock and takes no `work`. Every
    /// case of this row is an api-smoke image, so every run carries the
    /// wire's echo on the second CMSDK UART.
    fn build_and_run_an505_with(&mut self, name: &str, feats: &str, profile: ExecProfile, work: u32) -> CaseRun {
        let staged = self.build_an505(name, feats);
        self.run_an505_staged(name, &staged, profile, work)
    }

    /// The build half of the an505 row: the crate built for the Cortex-M33
    /// target into the gate's second target directory, the image staged
    /// under the case name and hashed.
    fn build_an505(&mut self, name: &str, feats: &str) -> Staged {
        println!("== case {name} ==");
        let target_dir = self.ctx.repo.join("src").join("kernel").join("target").join("api-gate-an505");
        let log = self.ctx.out.join(format!("build-{name}.log"));
        let ok = cargo_build_kernel_at(&self.ctx.repo, &target_dir, &["--target", "thumbv8m.main-none-eabihf", "--no-default-features", "--features", feats], &log);
        if !matches!(ok, Ok(true)) {
            echo_file(&log, "");
            self.ctx.gate_fail(&format!("{name}: build error"));
        }
        let built = target_dir.join("thumbv8m.main-none-eabihf").join("release").join("rhu-kernel");
        let staged = self.ctx.out.join(format!("rhu-kernel-{name}.elf"));
        if std::fs::copy(&built, &staged).is_err() {
            self.ctx.gate_fail(&format!("{name}: built kernel not found at {}", built.display()));
        }
        let sha = match sha256_file(&staged) {
            Ok(s) => s,
            Err(e) => self.ctx.gate_fail(&format!("{name}: cannot hash staged artifact: {e}")),
        };
        let path_rel = format!("{}/rhu-kernel-{name}.elf", self.ctx.out_rel);
        Staged { staged_arg: staged.to_string_lossy().into_owned(), sha, path_rel }
    }

    /// The run half of the an505 row: the host's unit of emulator work
    /// measured first where the profile counts instructions, then the echo
    /// started, the staged image run on the an505 machine with the second
    /// chardev after the console's, the echo finished.
    fn run_an505_staged(&mut self, name: &str, staged: &Staged, profile: ExecProfile, work: u32) -> CaseRun {
        let raw_path = self.ctx.out.join(format!("{name}.raw"));
        let stderr_path = self.ctx.out.join(format!("{name}.stderr"));
        let limits = match profile {
            ExecProfile::Parity => None,
            ExecProfile::Deterministic => {
                let unit = host_unit(&self.ctx.qemu, &self.ctx.out, self.ctx.timeout);
                Some(Limits { deadline: self.ctx.timeout, budget: budget(self.ctx.timeout, work, unit) })
            }
        };
        let (chardev, echo) = self.start_echo(name);
        let mut args: Vec<&str> = vec!["-M", "mps2-an505", "-display", "none", "-monitor", "none", "-serial", "stdio", "-serial", chardev.as_str(), "-semihosting"];
        args.extend_from_slice(profile.qemu_args());
        args.extend_from_slice(&["-kernel", &staged.staged_arg]);
        let exit = match limits {
            None => {
                let outcome = match run_captured(&self.ctx.qemu, &args, &raw_path, &stderr_path, self.ctx.timeout) {
                    Ok(o) => o,
                    Err(e) => self.ctx.gate_fail(&format!("{name}: {e}")),
                };
                if outcome.timed_out {
                    println!("  (run killed at its timeout; a hang is observed, never a pass)");
                }
                outcome.exit
            }
            Some(limits) => {
                let run = match run_counted(&self.ctx.qemu, &args, &raw_path, &stderr_path, limits) {
                    Ok(r) => r,
                    Err(e) => self.ctx.gate_fail(&format!("{name}: {e}")),
                };
                say_counted(&run);
                run.exit
            }
        };
        self.end_echo(name, echo);
        let raw = std::fs::read(&raw_path).unwrap_or_default();
        echo_file(&raw_path, "  | ");
        CaseRun { sha: staged.sha.clone(), path_rel: staged.path_rel.clone(), raw, exit }
    }

    /// The two controls of the instruction-counted runs' limits: a guest
    /// that never ends must be stopped. Each is a generated guest of the
    /// mps2-an500 machine, no kernel image: one never leaves its loop and
    /// is stopped when the emulator has spent the run's budget of
    /// processor time; one waits for an interrupt nothing raises and is
    /// stopped after a whole deadline in which the emulator did no work --
    /// or at its budget, where the emulator keeps a thread of its own busy
    /// while its guest waits. On a host that gives no reading of the
    /// emulator's processor time the wall clock stops both at the
    /// deadline. The budgets are one unit of the host's emulator work over
    /// small floors, so a slow host's emulator is given the time it needs
    /// to start.
    fn run_hang_controls(&mut self) {
        let unit = host_unit(&self.ctx.qemu, &self.ctx.out, self.ctx.timeout);
        let deadline = Duration::from_secs(HANG_DEADLINE_S);
        let spin = Limits { deadline, budget: budget(Duration::from_secs(1), 1, unit) };
        let wait = Limits { deadline, budget: budget(Duration::from_secs(2), 1, unit) };
        self.run_hang_control("hang-spin-control", Guest::Spin, true, HANG_SPIN_BROKE, spin);
        self.run_hang_control("hang-wait-control", Guest::Wait, false, HANG_WAIT_BROKE, wait);
    }

    fn run_hang_control(&mut self, name: &str, guest: Guest, works: bool, broke: &str, limits: Limits) {
        println!("== case {name} ==");
        let run = match run_guest(&self.ctx.qemu, &self.ctx.out, name, guest, limits) {
            Ok(r) => r,
            Err(e) => self.ctx.gate_fail(&format!("{name}: {e}")),
        };
        say_counted(&run);
        let sha = match sha256_file(&self.ctx.out.join(format!("{name}.bin"))) {
            Ok(s) => s,
            Err(e) => self.ctx.gate_fail(&format!("{name}: cannot hash the generated image: {e}")),
        };
        let path_rel = format!("{}/{name}.bin", self.ctx.out_rel);
        let raw = std::fs::read(self.ctx.out.join(format!("{name}.raw"))).unwrap_or_default();
        if let Err(why) = hang_refused(run.exit, run.stop, works, &raw) {
            self.emit_control_timed(name, "fail", broke, run.exit, 124, &sha, &path_rel, Some("icount-shift0"));
            self.ctx.gate_fail(&format!("{name}: {why} (a guest that never ends must be stopped)"));
        }
        self.emit_control_timed(name, "pass", broke, run.exit, 124, &sha, &path_rel, Some("icount-shift0"));
        let how = match run.stop {
            Some(Stop::Work) => "stopped when the emulator had spent the run's budget of processor time",
            Some(Stop::NoWork) => "stopped after a whole deadline in which the emulator did no work",
            _ => "stopped by the wall clock at the deadline, this host giving no reading of the emulator's processor time",
        };
        println!("== control {name}: PASS (the guest never ended and was {how}; nothing printed; recorded as exit 124) ==");
    }

    // ---------------- the header's own compile ----------------

    fn run_header_compile(&mut self) {
        const CASE: &str = "header-compile";
        const CONTROL: &str = "header-doctored-control";
        println!("== case {CASE} ==");
        let clang = match resolve_clang(&self.ctx.repo) {
            Ok(c) => c,
            Err(e) => self.ctx.gate_fail(&format!("{CASE}: {e}")),
        };
        let header = self.ctx.repo.join("src").join("kernel").join("include").join("rhu.h");
        let header_sha = match sha256_file(&header) {
            Ok(s) => s,
            Err(e) => self.ctx.gate_fail(&format!("{CASE}: cannot hash {HEADER_REL}: {e}")),
        };
        let header_text = match std::fs::read_to_string(&header) {
            Ok(t) => t,
            Err(e) => self.ctx.gate_fail(&format!("{CASE}: cannot read {HEADER_REL}: {e}")),
        };
        if !header_text.contains("extern \"C\"") {
            self.ctx.gate_fail(&format!("{CASE}: {HEADER_REL} carries no extern \"C\" guard, which the C++ compile of this case assumes"));
        }
        let unit_rel = format!("{}/{CASE}.c", self.ctx.out_rel);
        write_fixture(&self.ctx.out.join(format!("{CASE}.c")), HEADER_UNIT);
        let mut read = Vec::new();
        for (lang, stem_lang, words) in HEADER_LANGS {
            for class in [Class::CortexM7, Class::CortexM33] {
                let stem = format!("{CASE}-{stem_lang}-{}", class_stem(class));
                let (ok, _) = self.compile_header(&clang, &stem, &words, class, "-Isrc/kernel/include", &unit_rel);
                if !ok {
                    echo_file(&self.ctx.out.join(format!("{stem}.cc.stderr")), "  | ");
                    self.emit_case(CASE, "fail", 1, 0, &header_sha, HEADER_REL);
                    self.ctx.gate_fail(&format!(
                        "{CASE}: {HEADER_REL} did not read clean as {lang} for the {} class (the compiler's words above, kept as {stem}.cc.stderr)",
                        class_name(class)
                    ));
                }
                read.push(format!("{lang} {}", class_name(class)));
            }
        }
        self.emit_case(CASE, "pass", 0, 0, &header_sha, HEADER_REL);
        println!(
            "== case {CASE}: PASS ({HEADER_REL} read clean by {} at -ffreestanding -fsyntax-only -Wall -Wextra -Werror as {}) ==",
            clang.version,
            read.join(", ")
        );

        println!("== control {CONTROL} ==");
        if header_text.matches(HEADER_DOCTOR_ANCHOR).count() != 1 {
            self.ctx.gate_fail(&format!("{CONTROL}: the doctor's anchor '{HEADER_DOCTOR_ANCHOR}' is not found once in {HEADER_REL}"));
        }
        let doctored = header_text.replacen(HEADER_DOCTOR_ANCHOR, &format!("{HEADER_DOCTOR_LINE}\n{HEADER_DOCTOR_ANCHOR}"), 1);
        let dir = self.ctx.out.join("header-doctored");
        write_fixture(&dir.join("rhu.h"), doctored.as_bytes());
        let doctored_rel = format!("{}/header-doctored/rhu.h", self.ctx.out_rel);
        let doctored_sha = match sha256_file(&dir.join("rhu.h")) {
            Ok(s) => s,
            Err(e) => self.ctx.gate_fail(&format!("{CONTROL}: cannot hash the doctored copy: {e}")),
        };
        let include = format!("-I{}/header-doctored", self.ctx.out_rel);
        for class in [Class::CortexM7, Class::CortexM33] {
            let stem = format!("{CONTROL}-{}", class_stem(class));
            let (ok, stderr_text) = self.compile_header(&clang, &stem, &HEADER_LANGS[0].2, class, &include, &unit_rel);
            if let Err(why) = header_refusal(ok, &stderr_text) {
                echo_file(&self.ctx.out.join(format!("{stem}.cc.stderr")), "  | ");
                self.emit_control(CONTROL, "fail", HEADER_DOCTORED_BROKE, if ok { 0 } else { 1 }, 1, &doctored_sha, &doctored_rel);
                self.ctx.gate_fail(&format!("{CONTROL}: {why} for the {} class (the control did not bite)", class_name(class)));
            }
        }
        self.emit_control(CONTROL, "pass", HEADER_DOCTORED_BROKE, 1, 1, &doctored_sha, &doctored_rel);
        println!("== control {CONTROL}: PASS (the copy with rhu_net_count declared as a type as well refused on both classes with the words '{HEADER_REFUSAL_WORDS}') ==");
    }

    /// One compile of the header's translation unit by the compiler of
    /// record: the language words, the class's flag set, one include
    /// directory and the unit, bounded, the streams captured beside the
    /// gate's transcripts. Answers whether the compiler accepted the unit
    /// and what it wrote on its error stream.
    fn compile_header(&mut self, clang: &ToolCmd, stem: &str, lang_words: &[&str], class: Class, include: &str, unit_rel: &str) -> (bool, String) {
        let stdout_p = self.ctx.out.join(format!("{stem}.cc.stdout"));
        let stderr_p = self.ctx.out.join(format!("{stem}.cc.stderr"));
        let mut args: Vec<String> = lang_words.iter().map(|s| s.to_string()).collect();
        args.extend(header_flags(class).iter().map(|s| s.to_string()));
        args.push(include.to_string());
        args.push(unit_rel.to_string());
        let ok = match run_tool_bounded(&self.ctx.repo, clang, &args, &stdout_p, &stderr_p) {
            Ok(b) => b,
            Err(e) => self.ctx.gate_fail(&format!("{stem}: {e}")),
        };
        (ok, std::fs::read_to_string(&stderr_p).unwrap_or_default())
    }

    // ---------------- the null-page guard and the stack floor ----------------

    /// A case built and run on either emulator row, every run carrying the
    /// wire's echo on the row's second CMSDK UART: the established row
    /// through `build_and_run_echo`, the Cortex-M33 row through its second
    /// target directory and machine, whose every run carries the echo. An
    /// api-smoke image reaches its UART checks on either row, so a case
    /// routed here runs over the wire; an image that never claims the port
    /// leaves the echo idle.
    fn build_and_run_row(&mut self, an505: bool, name: &str, feats: &str) -> CaseRun {
        if an505 {
            let all = format!("qemu-exit,board-mps2-an505,entropy-accept-software,{feats}");
            self.build_and_run_an505(name, &all)
        } else {
            self.build_and_run_echo(name, &["--features", feats])
        }
    }

    /// The symbol dump of a staged case artifact (llvm-nm), for a value
    /// read off the image.
    fn nm_dump(&mut self, case: &str) -> Vec<u8> {
        let nm = match llvm_tool(&self.facts, "llvm-nm") {
            Ok(p) => p,
            Err(e) => self.ctx.gate_fail(&format!("{case}: {e}")),
        };
        let elf = self.ctx.out.join(format!("rhu-kernel-{case}.elf"));
        let elf_arg = elf.to_string_lossy().into_owned();
        let sym = self.ctx.out.join(format!("{case}.sym"));
        let scratch = self.ctx.out.join(format!("{case}.tool.stderr"));
        let nm_run = run_captured(&nm, &[&elf_arg], &sym, &scratch, Duration::from_secs(60));
        if !matches!(nm_run, Ok(ref o) if o.exit == 0) {
            self.ctx.gate_fail(&format!("{case}: llvm-nm failed on the staged ELF"));
        }
        std::fs::read(&sym).unwrap_or_default()
    }

    fn run_nullpage(&mut self, an505: bool) {
        let row = if an505 { "-an505" } else { "" };
        let mpu_line = if an505 { MARKER_MPU_AN505 } else { MARKER_MPU_AN500 };

        // The designed death.
        let case = format!("nullpage{row}");
        let run = self.build_and_run_row(an505, &case, "test-nullpage");
        let (sha, path_rel, code) = (run.sha.clone(), run.path_rel.clone(), run.exit);
        self.common_asserts(
            &case,
            &run,
            2,
            &[
                "flavor: test-nullpage",
                mpu_line,
                "[nullpage] a thread named reader reads a word through address 4 with nothing armed -- expect the null-access death",
                "[fault] *** EXCEPTION: MemManage (IPSR=4) ***",
                "CFSR=0x00000082",
                "MMFAR=0x00000004",
                "CFSR[1] DACCVIOL",
                "CFSR[7] MMARVALID",
                "[fault] null access: read at 0x00000004 from thread 3 reader (the first page is no-access; not delivered: no hosting layer has armed the delivery)",
                "[fault] thread 3 reader: sp 0x",
                "): inside",
                "[fault] *** halted ***",
            ],
            &["UNREACHABLE", "KERNEL PANIC", "[nullpage] delivered"],
        );
        self.emit_case(&case, "pass", code, 2, &sha, &path_rel);
        println!("== case {case}: PASS (exit 2, the read through null died loud through the funnel with the null-access line naming the reader) ==");

        // The control: the guard off, the part's own word at 4 read and
        // printed -- the reset vector, tied to the artifact's Reset symbol.
        let case = format!("nullpage-control{row}");
        let broke = "the null-page guard removed (null-guard-off): the read through null returns the row's reset-vector word and the flavor's UNREACHABLE line fires";
        let run = self.build_and_run_row(an505, &case, "test-nullpage,null-guard-off");
        let (sha, path_rel, code) = (run.sha.clone(), run.path_rel.clone(), run.exit);
        let survived = contains_fixed(&run.raw, "first page OPEN (null-guard-off, a control image)")
            && contains_fixed(&run.raw, "[nullpage] UNREACHABLE: the read through null returned 0x")
            && !contains_fixed(&run.raw, "EXCEPTION:")
            && code == 3;
        if !survived {
            self.emit_control(&case, "fail", broke, code, 3, &sha, &path_rel);
            self.ctx.gate_fail(&format!("{case}: the control did not survive the read as designed (exit {code}, wanted 3 with the UNREACHABLE line)"));
        }
        let dump = self.nm_dump(&case);
        let Some(reset) = word_sym_value(&dump, "Reset") else {
            self.emit_control(&case, "fail", broke, code, 3, &sha, &path_rel);
            self.ctx.gate_fail(&format!("{case}: the artifact's Reset symbol is not value-parseable"));
        };
        let printed = hex_after(&run.raw, "[nullpage] UNREACHABLE: the read through null returned 0x");
        if printed != Some(reset | 1) {
            self.emit_control(&case, "fail", broke, code, 3, &sha, &path_rel);
            self.ctx.gate_fail(&format!("{case}: the word read at 4 ({printed:#x?}) is not the row's reset vector ({:#010x} with the Thumb bit)", reset));
        }
        self.emit_control(&case, "pass", broke, code, 3, &sha, &path_rel);
        println!("== control {case}: PASS (the guard off, the read through null returned the row's reset vector {:#010x}: the survived shape the guarded flavor refuses; exit 3) ==", reset | 1);

        let case = format!("nullpage-deliver{row}");
        let run = self.build_and_run_row(an505, &case, "test-nullpage,test-nullpage-deliver");
        let (sha, path_rel, code) = (run.sha.clone(), run.path_rel.clone(), run.exit);
        self.common_asserts(
            &case,
            &run,
            0,
            &[
                "flavor: test-nullpage",
                mpu_line,
                "[nullpage] deliver: the test handler registered and the null kind armed through the door; the reader reads [4] then writes [0x10] -- expect two deliveries through the activation costume and two landings",
                "[nullpage] delivered: kind 1 read at 0x00000004 from the read site, the registers as the thread held them; pc redirected to the landing",
                "[nullpage] landed from the read with marker 0xde110001 (address 0x00000004); the write follows",
                "[nullpage] delivered: kind 1 write at 0x00000010 from the write site, the registers as the thread held them; pc redirected to the landing",
                "[nullpage] landed from the write with marker 0xde110002 (address 0x00000010)",
                "[nullpage] PASS (two null accesses delivered through the activation costume and landed: the read and the write)",
            ],
            &["EXCEPTION:", "KERNEL PANIC", "UNREACHABLE"],
        );
        self.emit_case(&case, "pass", code, 0, &sha, &path_rel);
        println!("== case {case}: PASS (exit 0, a null read and a null write delivered through the activation costume with the thread's registers intact, the handler's redirects landed) ==");

        let case = format!("nullpage-cs{row}");
        let run = self.build_and_run_row(an505, &case, "test-nullpage,test-nullpage-cs");
        let (sha, path_rel, code) = (run.sha.clone(), run.path_rel.clone(), run.exit);
        self.common_asserts(
            &case,
            &run,
            2,
            &[
                "flavor: test-nullpage",
                "[nullpage] cs: the reader reads a word through address 4 inside the kernel's critical section -- expect the forced HardFault's naming",
                "[fault] *** EXCEPTION: HardFault (IPSR=3) ***",
                "HFSR=0x40000000",
                "MMFAR=0x00000004",
                "CFSR[1] DACCVIOL",
                "[fault] null access: read at 0x00000004 from thread 3 reader (the first page is no-access; not delivered: raised with the kernel's interrupt mask set or inside a handler of equal or higher urgency, escalated to HardFault)",
                "[fault] *** halted ***",
            ],
            &["UNREACHABLE", "KERNEL PANIC", "[nullpage] delivered"],
        );
        self.emit_case(&case, "pass", code, 2, &sha, &path_rel);
        println!("== case {case}: PASS (exit 2, the read through null inside the critical section escalated to a forced HardFault and died named) ==");

        let case = format!("nullpage-fetch{row}");
        let run = self.build_and_run_row(an505, &case, "test-nullpage,test-nullpage-fetch");
        let (sha, path_rel, code) = (run.sha.clone(), run.path_rel.clone(), run.exit);
        self.common_asserts(
            &case,
            &run,
            2,
            &[
                "flavor: test-nullpage",
                "[nullpage] fetch: the test handler registered and the null kind armed; the reader jumps to address 0 -- expect the terminal fetch line (an instruction fetch is never delivered)",
                "[fault] *** EXCEPTION: MemManage (IPSR=4) ***",
                "CFSR[0] IACCVIOL",
                "[fault] null access: fetch at 0x00000000 from thread 3 reader (the first page is no-access; an instruction fetch below the page is never delivered: no managed frame to throw from)",
                "[fault] *** halted ***",
            ],
            &["UNREACHABLE", "KERNEL PANIC", "was delivered"],
        );
        self.emit_case(&case, "pass", code, 2, &sha, &path_rel);
        println!("== case {case}: PASS (exit 2, the jump to address 0 died through the funnel with the fetch line, undelivered with the delivery armed) ==");

        if !an505 {
            let case = "nullpage-header";
            let run = self.build_and_run_row(false, case, "test-nullpage,test-nullpage-header");
            let (sha, path_rel, code) = (run.sha.clone(), run.path_rel.clone(), run.exit);
            self.common_asserts(
                case,
                &run,
                2,
                &[
                    "flavor: test-nullpage",
                    MARKER_MPU_AN500,
                    "[nullpage] header: the reader reads the image header's first word through the linker symbol, at 0x400 on this row and inside the page, with nothing armed -- expect the null-access death",
                    "[fault] *** EXCEPTION: MemManage (IPSR=4) ***",
                    "CFSR=0x00000082",
                    "MMFAR=0x00000400",
                    "CFSR[1] DACCVIOL",
                    "CFSR[7] MMARVALID",
                    "[fault] null access: read at 0x00000400 from thread 3 reader (the first page is no-access; not delivered: no hosting layer has armed the delivery)",
                    "[fault] *** halted ***",
                ],
                &["UNREACHABLE", "KERNEL PANIC", "[nullpage] delivered", "readable"],
            );
            self.emit_case(case, "pass", code, 2, &sha, &path_rel);
            println!("== case {case}: PASS (exit 2, the read of the image header's first word at 0x400 died through the funnel like any null access) ==");
        }
    }

    /// The handler stack's reading of an api-flavor run: the check's PASS
    /// line and the exit path's watermark line parsed for their numbers, the
    /// use asserted under the reserve on this side too and non-decreasing
    /// between the two, both printed.
    fn handler_stack_reading(&mut self, case: &str, row: &str, raw: &[u8]) {
        let (used, reserve) = match parse_high_water(raw, "[api] PASS handler-stack (the high-water ") {
            Some(v) => v,
            None => self.ctx.gate_fail(&format!("{case}: the handler-stack PASS line carries no readable numbers")),
        };
        let (used_end, reserve_end) = match parse_high_water(raw, "[stack] handler stack high-water: ") {
            Some(v) => v,
            None => self.ctx.gate_fail(&format!("{case}: the exit path's watermark line is missing or carries no readable numbers")),
        };
        if used == 0 || used >= reserve || used_end == 0 || used_end >= reserve_end || reserve != reserve_end || used_end < used {
            self.ctx.gate_fail(&format!("{case}: the handler stack's high-water {used} then {used_end} of {reserve} and {reserve_end} bytes is not under the reserve and non-decreasing"));
        }
        println!("== handler-stack on {row} ({case}): the high-water {used} of {reserve} bytes at the check, {used_end} at the exit path ({} bytes of the reserve never touched) ==", reserve - used_end);
    }

    /// The watermark's doctored control: the fill at reset skipped, so the
    /// api flavor's scan finds the reserve used whole and the check must
    /// refuse at its own assert (exit 1, the panic's text, no PASS line).
    fn run_handler_stack_control(&mut self) {
        let run = self.build_and_run_echo("handler-stack-control", &["--features", "api-smoke,test-stack-watermark-doctored"]);
        let (sha, path_rel, code) = (run.sha.clone(), run.path_rel.clone(), run.exit);
        let broke = "the fill at reset skipped (test-stack-watermark-doctored): the scan finds the reserve used whole and the handler-stack check must refuse";
        let refused = !contains_fixed(&run.raw, "[api] PASS handler-stack (")
            && contains_fixed(&run.raw, "handler-stack: the high-water reads ")
            && contains_fixed(&run.raw, "the reserve used whole (the fill missing or the floor crossed)");
        if !refused {
            self.emit_control("handler-stack-control", "fail", broke, code, 1, &sha, &path_rel);
            self.ctx.gate_fail("handler-stack-control: the check passed with the fill skipped, or died elsewhere than its own assert (the control did not bite)");
        }
        if let Err(why) = run_ending(code, 1, &run.raw) {
            self.emit_control("handler-stack-control", "fail", broke, code, 1, &sha, &path_rel);
            self.ctx.gate_fail(&format!("handler-stack-control: {why} (the control did not die as designed)"));
        }
        self.emit_control("handler-stack-control", "pass", broke, code, 1, &sha, &path_rel);
        println!("== control handler-stack-control: PASS (the fill skipped, the scan read the reserve used whole and the check refused by its own assert; no PASS line; exit 1) ==");
    }

    /// The designed death inside an interrupt handler on an emulator row:
    /// the spare line's handler executes an undefined instruction, the
    /// UsageFault escalates to the HardFault the funnel names from handler
    /// mode (exit 2), and the watermark line at the end reads the handler
    /// stack's deepest use -- the reserve's deepest path, printed.
    fn run_fault_in_handler(&mut self, an505: bool) {
        let case = if an505 { "fault-in-handler-an505" } else { "fault-in-handler" };
        let run = if an505 {
            self.build_and_run_an505(case, "qemu-exit,board-mps2-an505,entropy-accept-software,test-fault-in-handler")
        } else {
            self.ctx.build_and_run(case, &["--features", "test-fault-in-handler"])
        };
        let (sha, path_rel, code) = (run.sha.clone(), run.path_rel.clone(), run.exit);
        self.common_asserts(
            case,
            &run,
            2,
            &[
                "flavor: test-fault-in-handler",
                "[fault-in-handler] the spare interrupt line ",
                "[fault] *** EXCEPTION: HardFault (IPSR=3) ***",
                "HFSR=0x40000000",
                "CFSR[16] UNDEFINSTR",
                "frame on MSP)",
                "[fault] *** halted ***",
                "[stack] handler stack high-water: ",
            ],
            &["UNREACHABLE", "KERNEL PANIC"],
        );
        let (used, reserve) = match parse_high_water(&run.raw, "[stack] handler stack high-water: ") {
            Some(v) => v,
            None => self.ctx.gate_fail(&format!("{case}: the watermark line carries no readable numbers")),
        };
        if used == 0 || used >= reserve {
            self.ctx.gate_fail(&format!("{case}: the handler stack's high-water {used} of {reserve} bytes is not under the reserve"));
        }
        self.emit_case(case, "pass", code, 2, &sha, &path_rel);
        println!("== case {case}: PASS (exit 2, the undefined instruction inside the spare line's handler died through the funnel from handler mode; the handler stack's high-water {used} of {reserve} bytes) ==");
    }

    /// The stack-overflow flavor on the Cortex-M7 row: the guard
    /// block at the eater's floor raises the MemManage fault the funnel
    /// names with the overflow line (exit 2); its control, the guard off,
    /// survives the descent into the victim's parked frames and exits 3
    /// through the UNREACHABLE line.
    fn run_stackovfl_v7m(&mut self) {
        let run = self.ctx.build_and_run("stackovfl", &["--features", "test-stackovfl"]);
        let (sha, path_rel, code) = (run.sha.clone(), run.path_rel.clone(), run.exit);
        self.common_asserts(
            "stackovfl",
            &run,
            2,
            &[
                "flavor: test-stackovfl",
                MARKER_MPU_AN500,
                "[stackovfl] a 4 KB victim parked below a 1 KB eater; the eater descends twelve 256-byte frames past its floor -- expect a MemManage fault with the stack-overflow line (the guard block at the floor)",
                "[fault] *** EXCEPTION: MemManage (IPSR=4) ***",
                "[fault] stack overflow: thread 4 eater's stack [",
                ") at its floor was crossed by ",
                "[fault] thread 4 eater: sp 0x",
                "[fault] *** halted ***",
            ],
            &["UNREACHABLE", "KERNEL PANIC", "null access"],
        );
        self.emit_case("stackovfl", "pass", code, 2, &sha, &path_rel);
        println!("== case stackovfl: PASS (exit 2, the descent past the floor died loud at the guard block with the overflow line naming the eater) ==");

        let broke = "the stack guard removed (stack-guard-off): the descent survives into the victim's parked frames and the flavor's UNREACHABLE line fires";
        let run = self.ctx.build_and_run("stackovfl-control", &["--features", "test-stackovfl,stack-guard-off"]);
        let (sha, path_rel, code) = (run.sha.clone(), run.path_rel.clone(), run.exit);
        let survived = contains_fixed(&run.raw, "stack guard OFF (stack-guard-off, a control image)")
            && contains_fixed(&run.raw, "[stackovfl] UNREACHABLE: the descent survived the floor")
            && !contains_fixed(&run.raw, "EXCEPTION:")
            && code == 3;
        if !survived {
            self.emit_control("stackovfl-control", "fail", broke, code, 3, &sha, &path_rel);
            self.ctx.gate_fail(&format!("stackovfl-control: the control did not survive the descent as designed (exit {code}, wanted 3 with the UNREACHABLE line)"));
        }
        self.emit_control("stackovfl-control", "pass", broke, code, 3, &sha, &path_rel);
        println!("== control stackovfl-control: PASS (the guard off, the descent survived the floor: the survived shape the guarded flavor refuses; exit 3) ==");
    }

    /// The kheap-loud designed death: the corrupted
    /// free-block canary must kill the very next walk through the
    /// funnel. Exit 1, constant panic text; the UNREACHABLE line is
    /// the survived red shape (the control's expected trip).
    fn run_kheap_loud(&mut self) {
        let run = self.ctx.build_and_run("kheap-loud", &["--features", "test-kheap-loud"]);
        let (sha, path_rel, code) = (run.sha.clone(), run.path_rel.clone(), run.exit);
        self.common_asserts(
            "kheap-loud",
            &run,
            1,
            &[
                "flavor: test-kheap-loud",
                "[kheap-loud] corrupting a free block header, then allocating -- expect a KERNEL PANIC",
                "[panic] *** KERNEL PANIC ***",
                "kheap: corrupt block header",
                "[panic] *** halted ***",
            ],
            &["UNREACHABLE", "EXCEPTION:"],
        );
        self.emit_case("kheap-loud", "pass", code, 1, &sha, &path_rel);
        println!("== case kheap-loud: PASS (exit 1, corrupt free header died loud through the funnel) ==");
    }
}

/// Run the api gate.

/// What the api gate's negative control breaks, for its case record: the
/// emulation's decoder with the add and subtract operations swapped (the
/// feature `test-fpemul-doctored`).
const NET_ARP_SILENT_BROKE: &str = "the virtual medium's scripted peer never answering an ARP request, so the connect to its port ends at the kernel's budget with EHOSTUNREACH pending where the honest peer's reset reads ECONNREFUSED (test-net-doctored-arp)";
const NET_BUMP_DOCTORED_BROKE: &str = "the network thread bumping the global event word every pass without a mask change, the storm the event port's recipe must never see (test-net-doctored-bump)";
const NET_SLOW_PUMP_BROKE: &str = "the network thread spending half a second of busy work per pass, so its longest pass passes the pump check's bound (test-net-doctored-slow)";
const NET_BUDGET_DOCTORED_BROKE: &str = "the network pool's table with a stream receive buffer a megabyte wide, which the compile-time assertion against the row's RAM window must refuse (test-net-doctored-budget)";
const HEADER_DOCTORED_BROKE: &str = "a copy of include/rhu.h with one door's name declared as a type as well (a typedef struct rhu_net_count inserted before the prototype of rhu_net_count), which the compiler of record must refuse as a redefinition of the name as a different kind of symbol";

/// The header the case compiles, repo-relative: the artifact of record.
const HEADER_REL: &str = "src/kernel/include/rhu.h";
/// The translation unit: the header and nothing else.
const HEADER_UNIT: &[u8] = b"#include <rhu.h>\n";
/// The languages the header is compiled as (the display name, the stem of
/// the transcript's file name, the compiler's words): C11 and, since the
/// header carries extern "C" guards, C++17.
const HEADER_LANGS: [(&str, &str, [&str; 3]); 2] = [
    ("C11", "c11", ["-x", "c", "-std=c11"]),
    ("C++17", "cxx17", ["-x", "c++", "-std=c++17"]),
];
/// The flag set of record for the header's compile on the Cortex-M7
/// class: the compiler of record's front end at the image ABI half (v7E-M,
/// hard-float FPv5-D16, as the shim tier's flag set of record), freestanding,
/// syntax only, every diagnostic an error with the extra warnings on (the
/// header reads clean under them). The language words go in front.
const HEADER_FLAGS: [&str; 10] = [
    "--target=arm-none-eabi",
    "-mcpu=cortex-m7",
    "-mthumb",
    "-mfloat-abi=hard",
    "-mfpu=fpv5-d16",
    "-ffreestanding",
    "-fsyntax-only",
    "-Wall",
    "-Wextra",
    "-Werror",
];
/// The same flag set for the Cortex-M33 class: one list apart from the
/// core and FPU words, which the selfcheck asserts through the shared class
/// record.
const HEADER_FLAGS_CORTEX_M33: [&str; 10] = [
    "--target=arm-none-eabi",
    "-mcpu=cortex-m33",
    "-mthumb",
    "-mfloat-abi=hard",
    "-mfpu=fpv5-sp-d16",
    "-ffreestanding",
    "-fsyntax-only",
    "-Wall",
    "-Wextra",
    "-Werror",
];
/// The control's doctor: the prototype the typedef is inserted before
/// (found once in the header), the typedef line, and the compiler's words
/// for the collision the control must read.
const HEADER_DOCTOR_ANCHOR: &str = "int32_t rhu_net_count(void);";
const HEADER_DOCTOR_LINE: &str = "typedef struct rhu_net_count { uint32_t n; } rhu_net_count;";
const HEADER_REFUSAL_WORDS: &str = "redefinition of 'rhu_net_count' as different kind of symbol";

/// The header compile's flag set of record for a class.
fn header_flags(class: Class) -> &'static [&'static str] {
    match class {
        Class::CortexM7 => &HEADER_FLAGS,
        Class::CortexM33 => &HEADER_FLAGS_CORTEX_M33,
    }
}

fn class_name(class: Class) -> &'static str {
    match class {
        Class::CortexM7 => "Cortex-M7",
        Class::CortexM33 => "Cortex-M33",
    }
}

fn class_stem(class: Class) -> &'static str {
    match class {
        Class::CortexM7 => concat!("m", "7"),
        Class::CortexM33 => "m33",
    }
}

/// The control's judge: the doctored copy must be refused, and refused
/// with the compiler's words for the collision -- a copy that compiled, or
/// one refused for another reason, is a control that did not bite.
fn header_refusal(ok: bool, stderr_text: &str) -> Result<(), String> {
    if ok {
        return Err("the doctored header compiled".to_string());
    }
    if !stderr_text.contains(HEADER_REFUSAL_WORDS) {
        return Err(format!("the doctored header was refused without the words '{HEADER_REFUSAL_WORDS}'"));
    }
    Ok(())
}
const PWM_DOCTORED_BROKE: &str = "the virtual timer's own reading of the period it runs made one prescaler step long, so the door's achieved period and the timer's disagree (test-pwm-doctored)";
const ADC_DOCTORED_BROKE: &str = "the virtual loop channel answering the previous conversion's sample, a stale conversion at the second reading (test-adc-doctored)";
const ADC_FLAGGED_DOCTORED_BROKE: &str = "the virtual converter answering a flagged conversion's undefined result as a sample instead of discarding it (test-adc-flagged-doctored)";
const RAM_DOCTORED_BROKE: &str = "the RAM-range count door answering one range more than the board's facts state, with a record for the phantom (test-ram-doctored)";
const RAM_INIT_DOCTORED_BROKE: &str = "the lazy init mode's hand-out skipping its zeroing, so the span above the frontier is handed out as the boot poison left it (test-ram-init-doctored)";
const RAM_INIT_DOCTORED_SECTION_BROKE: &str = "the background init mode's idle step zeroing its piece under the critical section, so the kernel band's tick is never served while a piece is in flight (test-ram-init-doctored-section)";
const FP_BAND_DOCTORED_BROKE: &str = "the tick hook pending nothing while the fp-band check runs, so no band exception fires however many of the kernel's ticks pass (test-fp-band-doctored)";
const REZERO_DOCTORED_BROKE: &str = "the rezero's memset run under the critical section, so no kernel-band interrupt is served while a memset is in flight and the arena-rezero-band check finds no tick landing inside it (test-rezero-doctored)";
const EXIT_WORD_BROKE: &str = "the store of the exit code into the console mirror's header keeps its low 16 bits alone, sign-extended, so a code past 16 bits reads back truncated (test-exit-word-doctored)";
const GPIO_LATCH_BROKE: &str = "the virtual backend's loopback latch fires the input's event twice per change (test-gpio-doctored-latch)";
const GPIO_EDGE_BROKE: &str = "the virtual backend makes one event of its own beside the tenth counted edge, an edge the program did not make (test-gpio-doctored-edge)";
const SUSP_ORDER_BROKE: &str = "the set-context landing pad storing its witness word before its thread id, with a sleep between the two (test-susp-doctored-order)";
const BUS_RP2350_DOCTORED_BROKE: &str = "over the RP2350's bus drivers: the sensor model refusing the second data byte of a write, the I2C block model never performing a repeated start, and the SPI driver's pump unbounded against the receive queue's honest overrun (test-bus-rp2350 with test-bus-doctored); and the events its check arms one at a time through the models' doctor call (a clock held low 25 ms and released, a data line held after a reset, the paths no honest run reaches)";
const BUS_DOCTORED_BROKE: &str = "the sensor model refusing the second data byte of a write, the I2C block model never completing a non-last message, the drivers' pumps unbounded against the receive queue's honest overflow, and the SPI driver's first transfer after the claim taking the wrong write order (test-bus-doctored); and the events its check arms one at a time through the models' doctor call (a clock held low 25 ms and released, a data line held after a reset, the paths no honest run reaches)";
const FP_DOCTORED_BROKE: &str = "the emulation decoder with the add and subtract operations swapped, and a form executed from an unregistered code range as the image's last act (the feature test-fpemul-doctored)";

/// The deadline of the two hang controls' runs, in seconds.
const HANG_DEADLINE_S: u64 = 3;

/// The bus controls' work budget, in the host's units of emulator work:
/// eight times the larger of the two measured shares of the api gate (on a
/// free host and under a shared load), rounded up. The Work stop guards
/// against a spinning guest and times nothing of the guest's work (the
/// instruction-counting clock does that); the Deadline and NoWork rules keep
/// their own bounds, so the larger budget weakens no judgement, and its only
/// cost is a later stop of a true spin.
const BUS_CONTROL_WORK: u32 = 500;

const HANG_SPIN_BROKE: &str = "a generated guest that never leaves its loop, where a run must end (hang-spin-control)";
const HANG_WAIT_BROKE: &str = "a generated guest that waits for an interrupt nothing in the machine raises, where a run must end (hang-wait-control)";

/// The judge of a hang control's run: the runner must have stopped it
/// (the exit recorded as 124) and the guest must have printed nothing. A
/// guest that works without end must have been stopped for its work; a
/// guest that waits may have been stopped for no work, or for work where
/// the emulator keeps a thread of its own busy while its guest waits. On a
/// host that gives no reading of the emulator's processor time the wall
/// clock's stop is the one to expect of both.
fn hang_refused(exit: i32, stop: Option<Stop>, works: bool, raw: &[u8]) -> Result<(), String> {
    if exit != 124 {
        return Err(format!("the run ended by itself with exit {exit}"));
    }
    if !raw.is_empty() {
        return Err("the generated guest printed".to_string());
    }
    match stop {
        None => Err("the run is recorded as stopped and no rule is named".to_string()),
        Some(Stop::NoWork) if works => Err("a guest that works without end read as doing no work".to_string()),
        Some(_) => Ok(()),
    }
}

/// The fp-double block of a transcript: every line that starts with the
/// check's tag, in order, without the line ending (the PASS line is not in
/// it: it names the row's class).
fn fp_block(raw: &[u8]) -> Vec<String> {
    String::from_utf8_lossy(raw)
        .lines()
        .map(|l| l.trim_end_matches('\r'))
        .filter(|l| l.starts_with("[api] fp-double:"))
        .map(String::from)
        .collect()
}

/// The judge of an emulated run's ending, beside the transcript's lines: a
/// run killed at the harness's deadline (recorded as exit 124) is a hang and
/// never a pass; the exit must be the wanted one; a run wanted to exit clean
/// must have printed the exit-door line (the machine left by the door, not by
/// another path); a run wanted to die must not have printed it.
/// The hexadecimal number printed right after `prefix` in a capture (the
/// first occurrence): the nullpage control's word read at 4.
fn hex_after(raw: &[u8], prefix: &str) -> Option<u64> {
    let p = prefix.as_bytes();
    let at = raw.windows(p.len()).position(|w| w == p)? + p.len();
    let digits: Vec<u8> = raw[at..].iter().take_while(|b| b.is_ascii_hexdigit()).copied().collect();
    if digits.is_empty() {
        return None;
    }
    u64::from_str_radix(core::str::from_utf8(&digits).ok()?, 16).ok()
}

fn run_ending(exit: i32, want: i32, raw: &[u8]) -> Result<(), String> {
    if exit == 124 {
        return Err("the run was killed at its timeout (a hang is observed, never a pass)".to_string());
    }
    if exit != want {
        return Err(format!("exit {exit}, wanted {want}"));
    }
    let door = contains_fixed(raw, MARKER_EXIT_DOOR);
    if want == 0 && !door {
        return Err("exit 0 without the exit-door line (the machine left by another path)".to_string());
    }
    if want != 0 && door {
        return Err("the exit-door line printed on a run that must die".to_string());
    }
    Ok(())
}

pub fn run(repo: &Path) -> ! {
    let (facts, qemu, qemu_v) = setup(repo);
    let out = repo.join("src").join("kernel").join("target").join("api-gate");
    if std::fs::create_dir_all(&out).is_err() {
        println!("rhu-gate: cannot create {}", out.display());
        exit(1);
    }
    let results = match Results::create(&out.join("api.results.jsonl")) {
        Ok(r) => r,
        Err(e) => {
            println!("rhu-gate: cannot create results file: {e}");
            exit(1);
        }
    };
    let rustc_v = facts.version.clone();
    let mut api = API {
        ctx: GateCtx {
            repo: repo.to_path_buf(),
            gate: GATE.to_string(),
            display: DISPLAY.to_string(),
            out,
            out_rel: "src/kernel/target/api-gate".to_string(),
            results,
            qemu,
            timeout: Duration::from_secs(TIMEOUT_S),
        },
        facts,
        api_raw: Vec::new(),
    };
    api.ctx.results.emit(&format!(
        "{{\"schema\":1,\"type\":\"run\",\"gate\":\"api\",\"run\":\"api-{}\",\"host\":\"{HOST}\",\"runner\":\"{RUNNER}\",\"chain\":{},\"tools\":{{\"qemu\":\"{}\",\"rustc\":\"{}\"}}}}",
        std::process::id(),
        chain_json_from_env(),
        qemu_v,
        rustc_v
    ));
    api.run_header_compile();
    api.run_api();
    api.run_boot_images();
    api.run_kheap_loud();
    api.run_nullpage(false);
    api.run_nullpage(true);
    api.run_stackovfl_v7m();
    api.run_fault_in_handler(false);
    api.run_fault_in_handler(true);
    api.run_fp_parity();
    api.run_gpio_latch_control();
    api.run_gpio_edge_control();
    api.run_susp_order_control();
    api.run_exit_word_control();
    api.run_handler_stack_control();
    api.run_bus_doctored_control();
    api.run_bus_rp2350();
    api.run_pwm_doctored_control();
    api.run_adc_doctored_control();
    api.run_adc_flagged_case();
    api.run_adc_flagged_doctored_control();
    api.run_ram_doctored_control();
    api.run_ram_init(false);
    api.run_ram_init(true);
    api.run_fp_band_doctored_control();
    api.run_rezero_doctored_control();
    api.run_no_board_claim();
    api.run_net_controls();
    api.run_hang_controls();
    api.ctx.results.emit_summary(GATE, "pass", 0);
    if !validate_results(repo, api.ctx.results.path()) {
        println!("{DISPLAY}: FAIL -- results file failed schema validation");
        exit(1);
    }
    println!("{DISPLAY}: PASS (56/56)  [{qemu_v} | {rustc_v}]");
    exit(0);
}

/// Negative-control the checkers this gate adds beyond the family:
/// the quiescence marker, the api flavor pin, the contract-symbol sweep,
/// and the forbidden-pattern sweep against the unconditional-pend red
/// shape -- each must REJECT its doctored input, and the accept side of
/// the new checkers is pinned too.
pub fn selfcheck(repo: &Path) -> ! {
    let (facts, _qemu, qemu_v) = setup(repo);
    let rustc_v = facts.version.clone();
    let out = repo.join("src").join("kernel").join("target").join("api-gate");
    let tmp = out.join("selfcheck");
    let _ = std::fs::create_dir_all(&tmp);
    let results = match Results::create(&out.join("api-selfcheck.results.jsonl")) {
        Ok(r) => r,
        Err(e) => {
            println!("rhu-gate: cannot create results file: {e}");
            exit(1);
        }
    };
    let mut sc = Selfcheck {
        repo: repo.to_path_buf(),
        gate: "api-selfcheck".to_string(),
        display: "API SELFCHECK".to_string(),
        results,
        n: 0,
        npass: 0,
        nneg: 0,
    };
    sc.results.emit(&format!(
        "{{\"schema\":1,\"type\":\"run\",\"gate\":\"api-selfcheck\",\"run\":\"api-selfcheck-{}\",\"host\":\"{HOST}\",\"runner\":\"{RUNNER}\",\"tools\":{{\"qemu\":\"{}\",\"rustc\":\"{}\"}}}}",
        std::process::id(),
        qemu_v,
        rustc_v
    ));

    // The unconditional-pend red shape: the case arms, the in-kernel
    // assert fires, the funnel reports -- no PASS marker.
    let red = tmp.join("unconditional.raw");
    write_fixture(
        &red,
        b"[api] tick-quiescent: solo busy 30 ms, switch delta armed\r\n[panic] *** KERNEL PANIC ***\r\n[panic] context switches during solo busy loop: 30\r\n[panic] *** halted ***\r\n",
    );
    let red_bytes = std::fs::read(&red).unwrap_or_default();
    sc.probe(
        "quiescent-marker-gone",
        "unconditional-pend transcript without the tick-quiescent PASS marker",
        contains_fixed(&red_bytes, MARKER_QUIESCENT),
    );
    sc.probe(
        "panic-shape",
        "forbidden-pattern sweep fed the unconditional-pend panic transcript",
        !contains_fixed(&red_bytes, "KERNEL PANIC"),
    );

    let wrongflavor = tmp.join("wrongflavor.raw");
    write_fixture(
        &wrongflavor,
        b"[boot] QEMU mps2-an500 | CMSDK UART0 | flavor: sched-smoke\r\n",
    );
    sc.probe(
        "wrong-flavor",
        "api-case flavor assertion fed a sched-smoke transcript",
        contains_fixed(&std::fs::read(&wrongflavor).unwrap_or_default(), "flavor: api-smoke"),
    );

    let nosym = tmp.join("noswitches.nm");
    write_fixture(
        &nosym,
        b"00000000 R __rhu_debug_layout\n20000008 D __rhu_sched\n20000040 B __rhu_ticks\n",
    );
    sc.probe(
        "switches-sym-gone",
        "symbol dump missing __rhu_switches (the counter LTO-eliminated shape)",
        word_sym_present(&std::fs::read(&nosym).unwrap_or_default(), "__rhu_switches"),
    );

    let partial = tmp.join("seven-of-eight.raw");
    let mut seven = Vec::new();
    for m in MARKERS_A.iter().filter(|m| !m.contains("futex-mutex")) {
        seven.extend_from_slice(m.as_bytes());
        seven.extend_from_slice(b"\r\n");
    }
    write_fixture(&partial, &seven);
    let partial_bytes = std::fs::read(&partial).unwrap_or_default();
    sc.probe(
        "a1-marker-gone",
        "seven-of-eight transcript missing the futex-mutex marker",
        MARKERS_A.iter().all(|m| contains_fixed(&partial_bytes, m)),
    );

    // The overlap detector's red shape: loud death through the funnel,
    // no mutex marker -- the forbidden sweep must trip on it.
    let overlap = tmp.join("overlap.raw");
    write_fixture(
        &overlap,
        b"[api] futex-mutex: 4 workers x 300 armed\r\n[panic] *** KERNEL PANIC ***\r\n[panic] mutex overlap: two threads inside the critical section\r\n[panic] *** halted ***\r\n",
    );
    sc.probe(
        "overlap-shape",
        "forbidden-pattern sweep fed the mutex-overlap panic transcript",
        !contains_fixed(&std::fs::read(&overlap).unwrap_or_default(), "KERNEL PANIC"),
    );

    // The API surface half-linked: rhu_futex_wait missing from the dump
    // (the symbol-eliminated shape) must fail the artifact pin.
    let noapi = tmp.join("noapi.nm");
    write_fixture(
        &noapi,
        b"00000000 T rhu_now_ns\n00000010 T rhu_sleep_ns\n00000020 T rhu_futex_wake\n",
    );
    sc.probe(
        "api-sym-gone",
        "symbol dump missing rhu_futex_wait (API surface half-linked)",
        word_sym_present(&std::fs::read(&noapi).unwrap_or_default(), "rhu_futex_wait"),
    );

    let bpartial = tmp.join("five-of-six-arena.raw");
    let mut bfour = Vec::new();
    for m in MARKERS_B.iter().filter(|m| !m.contains("arena-headroom")) {
        bfour.extend_from_slice(m.as_bytes());
        bfour.extend_from_slice(b"\r\n");
    }
    write_fixture(&bpartial, &bfour);
    let bpartial_bytes = std::fs::read(&bpartial).unwrap_or_default();
    sc.probe(
        "arena-marker-gone",
        "five-of-six transcript missing the arena-headroom marker",
        MARKERS_B.iter().all(|m| contains_fixed(&bpartial_bytes, m)),
    );

    let carve = tmp.join("carve-red.raw");
    write_fixture(
        &carve,
        b"[api] arena-carve: window + disjointness asserts armed\r\n[panic] *** KERNEL PANIC ***\r\n[panic] arena window overlaps .kstacks\r\n[panic] *** halted ***\r\n",
    );
    sc.probe(
        "carve-shape",
        "forbidden-pattern sweep fed the carve-panic transcript",
        !contains_fixed(&std::fs::read(&carve).unwrap_or_default(), "KERNEL PANIC"),
    );

    let noarena = tmp.join("noarena.nm");
    write_fixture(
        &noarena,
        b"00000000 T rhu_now_ns\n00000010 T rhu_sleep_ns\n00000020 T rhu_futex_wait\n00000030 T rhu_futex_wake\n00000040 T rhu_arena_alloc_at\n00000050 T rhu_arena_headroom\n00000060 T rhu_arena_report\n",
    );
    sc.probe(
        "arena-sym-gone",
        "symbol dump missing rhu_arena_rezero (arena surface half-linked)",
        word_sym_present(&std::fs::read(&noarena).unwrap_or_default(), "rhu_arena_rezero"),
    );

    let cpartial = tmp.join("six-of-seven-c.raw");
    let mut csix = Vec::new();
    for m in MARKERS_C.iter().filter(|m| !m.contains("registry-basic")) {
        csix.extend_from_slice(m.as_bytes());
        csix.extend_from_slice(b"\r\n");
    }
    write_fixture(&cpartial, &csix);
    let cpartial_bytes = std::fs::read(&cpartial).unwrap_or_default();
    sc.probe(
        "c-marker-gone",
        "six-of-seven transcript missing the registry-basic marker",
        MARKERS_C.iter().all(|m| contains_fixed(&cpartial_bytes, m)),
    );

    let survived = tmp.join("kheaploud-survived.raw");
    write_fixture(
        &survived,
        b"[kheap-loud] corrupting a free block header, then allocating -- expect a KERNEL PANIC\r\n[kheap-loud] UNREACHABLE: the corrupt walk survived\r\n",
    );
    sc.probe(
        "kheaploud-survived-shape",
        "forbidden-pattern sweep fed the survived kheap-loud transcript",
        !contains_fixed(&std::fs::read(&survived).unwrap_or_default(), "UNREACHABLE"),
    );

    let noc = tmp.join("noc.nm");
    let mut seventeen = Vec::new();
    for (i, s) in API_SYMS.iter().enumerate() {
        if *s == "rhu_tls_get" {
            continue;
        }
        seventeen.extend_from_slice(format!("{:08x} T {}\n", i * 16, s).as_bytes());
    }
    write_fixture(&noc, &seventeen);
    sc.probe(
        "c-sym-gone",
        "symbol dump missing rhu_tls_get (C surface half-linked)",
        word_sym_present(&std::fs::read(&noc).unwrap_or_default(), "rhu_tls_get"),
    );

    let dpartial = tmp.join("nine-of-ten-d.raw");
    let mut dnine = Vec::new();
    for m in MARKERS_D.iter().filter(|m| !m.contains("act-sleep-rearm")) {
        dnine.extend_from_slice(m.as_bytes());
        dnine.extend_from_slice(b"\r\n");
    }
    write_fixture(&dpartial, &dnine);
    let dpartial_bytes = std::fs::read(&dpartial).unwrap_or_default();
    sc.probe(
        "d-marker-gone",
        "nine-of-ten transcript missing the act-sleep-rearm marker",
        MARKERS_D.iter().all(|m| contains_fixed(&dpartial_bytes, m)),
    );

    let nod = tmp.join("nod.nm");
    let mut twentytwo = Vec::new();
    for (i, s) in API_SYMS.iter().enumerate() {
        if *s == "rhu_activation_send" {
            continue;
        }
        twentytwo.extend_from_slice(format!("{:08x} T {}\n", i * 16, s).as_bytes());
    }
    write_fixture(&nod, &twentytwo);
    sc.probe(
        "d-sym-gone",
        "symbol dump missing rhu_activation_send (D surface half-linked)",
        word_sym_present(&std::fs::read(&nod).unwrap_or_default(), "rhu_activation_send"),
    );

    let epartial = tmp.join("one-of-two-e.raw");
    let mut eone = Vec::new();
    for m in MARKERS_E.iter().filter(|m| !m.contains("mod-roots")) {
        eone.extend_from_slice(m.as_bytes());
        eone.extend_from_slice(b"\r\n");
    }
    write_fixture(&epartial, &eone);
    let epartial_bytes = std::fs::read(&epartial).unwrap_or_default();
    sc.probe(
        "e-marker-gone",
        "one-of-two transcript missing the mod-roots marker",
        MARKERS_E.iter().all(|m| contains_fixed(&epartial_bytes, m)),
    );

    let noe = tmp.join("noe.nm");
    let mut twentyfour = Vec::new();
    for (i, s) in API_SYMS.iter().enumerate() {
        if *s == "rhu_module_roots" {
            continue;
        }
        twentyfour.extend_from_slice(format!("{:08x} T {}\n", i * 16, s).as_bytes());
    }
    write_fixture(&noe, &twentyfour);
    sc.probe(
        "e-sym-gone",
        "symbol dump missing rhu_module_roots (E surface half-linked)",
        word_sym_present(&std::fs::read(&noe).unwrap_or_default(), "rhu_module_roots"),
    );

    let unequal = tmp.join("bracket-unequal.nm");
    write_fixture(
        &unequal,
        b"00012000 R __rhu_frozen_lo\n00012040 R __rhu_frozen_hi\n0000da48 T __exidx_start\n0000da48 T __exidx_end\n",
    );
    let unequal_bytes = std::fs::read(&unequal).unwrap_or_default();
    sc.probe(
        "bracket-unequal",
        "bracket-value checker fed a frozen pair 0x40 apart (base family must be zero-length)",
        matches!(
            (word_sym_value(&unequal_bytes, "__rhu_frozen_lo"), word_sym_value(&unequal_bytes, "__rhu_frozen_hi")),
            (Some(a), Some(b)) if a == b
        ),
    );

    // Hosting-door reject side 1: a transcript that lost exactly one
    // door marker (entropy-honesty) must not satisfy the door sweep.
    let fpartial = tmp.join("three-of-four-doors.raw");
    let mut fthree = Vec::new();
    for m in MARKERS_F.iter().filter(|m| !m.contains("entropy-honesty")) {
        fthree.extend_from_slice(m.as_bytes());
        fthree.extend_from_slice(b"\r\n");
    }
    write_fixture(&fpartial, &fthree);
    let fpartial_bytes = std::fs::read(&fpartial).unwrap_or_default();
    sc.probe(
        "door-marker-gone",
        "three-of-four transcript missing the entropy-honesty marker",
        MARKERS_F.iter().all(|m| contains_fixed(&fpartial_bytes, m)),
    );

    // Hosting-door reject side 2: the door surface half-linked --
    // rhu_entropy symbol-eliminated while its rhu_entropy_is_hardware
    // NEIGHBOR stays -- must fail the artifact pin (this fixture also
    // holds the word-BOUNDED matcher to its promise: the surviving
    // longer name must not satisfy the shorter prefix).
    let nof = tmp.join("nodoors.nm");
    let mut twentynine = Vec::new();
    for (i, s) in API_SYMS.iter().enumerate() {
        if *s == "rhu_entropy" {
            continue;
        }
        twentynine.extend_from_slice(format!("{:08x} T {}\n", i * 16, s).as_bytes());
    }
    write_fixture(&nof, &twentynine);
    sc.probe(
        "door-sym-gone",
        "symbol dump missing rhu_entropy beside a surviving rhu_entropy_is_hardware (door surface half-linked)",
        word_sym_present(&std::fs::read(&nof).unwrap_or_default(), "rhu_entropy"),
    );

    // The entry-seam absence pin: a base-family dump that carries
    // rhu_payload_entry must be refused (the seam belongs to the
    // payload family, where a payload can provide it).
    let seam = tmp.join("seam-in-base.nm");
    write_fixture(
        &seam,
        b"00000000 T rhu_now_ns\n00000010 T rhu_payload_entry\n00000020 T rhu_sleep_ns\n",
    );
    sc.probe(
        "entry-seam-in-base",
        "base-family symbol dump carrying rhu_payload_entry (the seam belongs to the payload family)",
        !word_sym_present(&std::fs::read(&seam).unwrap_or_default(), "rhu_payload_entry"),
    );

    // Hosting-door reject side 3: a base-family dump whose tls-template
    // pair is NOT zero-length must be refused by the bracket-value
    // checker.
    let tunequal = tmp.join("tdata-unequal.nm");
    write_fixture(
        &tunequal,
        b"20000000 D __rhu_tdata_lo\n20000010 D __rhu_tdata_hi\n",
    );
    let tunequal_bytes = std::fs::read(&tunequal).unwrap_or_default();
    sc.probe(
        "tdata-bracket-unequal",
        "bracket-value checker fed a tls pair 0x10 apart (base family must be zero-length)",
        matches!(
            (word_sym_value(&tunequal_bytes, "__rhu_tdata_lo"), word_sym_value(&tunequal_bytes, "__rhu_tdata_hi")),
            (Some(a), Some(b)) if a == b
        ),
    );

    let nobracket = tmp.join("bracket-missing.nm");
    write_fixture(
        &nobracket,
        b"00012000 R __rhu_frozen_lo\n00012000 R __rhu_frozen_hi\n0000da48 T __exidx_start\n",
    );
    let nobracket_bytes = std::fs::read(&nobracket).unwrap_or_default();
    sc.probe(
        "bracket-sym-gone",
        "bracket-value checker fed a dump missing __exidx_end (parse must refuse, not default)",
        matches!(
            (word_sym_value(&nobracket_bytes, "__exidx_start"), word_sym_value(&nobracket_bytes, "__exidx_end")),
            (Some(a), Some(b)) if a == b
        ),
    );

    // Accept side: the genuine green transcript and the full symbol dump.
    let green = tmp.join("green.raw");
    write_fixture(
        &green,
        b"[api] PASS tick-quiescent (solo busy 30 ms: 0 switches, clock advanced)\r\n",
    );
    sc.accept_probe(
        "quiescent-marker-present",
        "genuine tick-quiescent PASS marker accepted",
        contains_fixed(&std::fs::read(&green).unwrap_or_default(), MARKER_QUIESCENT),
    );
    let full = tmp.join("thirtytwo-of-thirtytwo.raw");
    let mut thirtytwo = Vec::new();
    for m in MARKERS_A
        .iter()
        .chain(MARKERS_B.iter())
        .chain(MARKERS_C.iter())
        .chain(MARKERS_D.iter())
        .chain(MARKERS_E.iter())
    {
        thirtytwo.extend_from_slice(m.as_bytes());
        thirtytwo.extend_from_slice(b"\r\n");
    }
    write_fixture(&full, &thirtytwo);
    let full_bytes = std::fs::read(&full).unwrap_or_default();
    sc.accept_probe(
        "a1-markers-present",
        "genuine thirty-two-marker transcript accepted by the A sweep",
        MARKERS_A.iter().all(|m| contains_fixed(&full_bytes, m)),
    );
    sc.accept_probe(
        "arena-markers-present",
        "genuine thirty-two-marker transcript accepted by the B sweep",
        MARKERS_B.iter().all(|m| contains_fixed(&full_bytes, m)),
    );
    sc.accept_probe(
        "c-markers-present",
        "genuine thirty-two-marker transcript accepted by the C sweep",
        MARKERS_C.iter().all(|m| contains_fixed(&full_bytes, m)),
    );
    sc.accept_probe(
        "d-markers-present",
        "genuine thirty-two-marker transcript accepted by the D sweep",
        MARKERS_D.iter().all(|m| contains_fixed(&full_bytes, m)),
    );
    sc.accept_probe(
        "e-markers-present",
        "genuine thirty-two-marker transcript accepted by the E sweep",
        MARKERS_E.iter().all(|m| contains_fixed(&full_bytes, m)),
    );
    let deathgreen = tmp.join("kheaploud-death.raw");
    write_fixture(
        &deathgreen,
        b"[kheap-loud] corrupting a free block header, then allocating -- expect a KERNEL PANIC\r\n[panic] *** KERNEL PANIC ***\r\n[panic] at src\\kheap.rs:99:5\r\n[panic] kheap: corrupt block header\r\n[panic] *** halted ***\r\n",
    );
    let deathgreen_bytes = std::fs::read(&deathgreen).unwrap_or_default();
    sc.accept_probe(
        "kheaploud-death-present",
        "genuine kheap-loud death transcript accepted by its marker sweep",
        contains_fixed(&deathgreen_bytes, "kheap: corrupt block header")
            && !contains_fixed(&deathgreen_bytes, "UNREACHABLE"),
    );
    let goodsym = tmp.join("allsyms.nm");
    let mut allsyms = Vec::new();
    for (i, s) in CONTRACT_SYMS.iter().chain(API_SYMS.iter()).enumerate() {
        allsyms.extend_from_slice(format!("{:08x} T {}\n", i * 16, s).as_bytes());
    }
    write_fixture(&goodsym, &allsyms);
    let goodsym_bytes = std::fs::read(&goodsym).unwrap_or_default();
    sc.accept_probe(
        "contract-syms-present",
        "full contract + API symbol dump accepted by the word-bounded sweep",
        CONTRACT_SYMS
            .iter()
            .chain(API_SYMS.iter())
            .all(|s| word_sym_present(&goodsym_bytes, s)),
    );
    sc.accept_probe(
        "entry-seam-absent-accepted",
        "genuine base-family dump without rhu_payload_entry accepted by the absence pin",
        !word_sym_present(&goodsym_bytes, "rhu_payload_entry"),
    );
    // The genuine base-family bracket shape: every pair present and
    // zero-length -- the value checker must accept it.
    let goodbrackets = tmp.join("brackets-equal.nm");
    write_fixture(
        &goodbrackets,
        b"00012d1c R __rhu_frozen_lo\n00012d1c R __rhu_frozen_hi\n0000da48 T __exidx_start\n0000da48 T __exidx_end\n20000000 D __rhu_tdata_lo\n20000000 D __rhu_tdata_hi\n",
    );
    let goodbrackets_bytes = std::fs::read(&goodbrackets).unwrap_or_default();
    sc.accept_probe(
        "brackets-zero-length-accepted",
        "genuine base-family bracket dump (every pair equal) accepted by the value checker",
        BRACKET_PAIRS.iter().all(|(lo, hi)| {
            matches!(
                (word_sym_value(&goodbrackets_bytes, lo), word_sym_value(&goodbrackets_bytes, hi)),
                (Some(a), Some(b)) if a == b
            )
        }),
    );

    let gdoor = tmp.join("one-of-two-g-door.raw");
    write_fixture(&gdoor, format!("{}\r\n", MARKERS_G[1]).as_bytes());
    sc.probe(
        "g-door-marker-gone",
        "one-of-two transcript missing the tls-base-door marker",
        MARKERS_G.iter().all(|m| contains_fixed(&std::fs::read(&gdoor).unwrap_or_default(), m)),
    );
    let gwrap = tmp.join("one-of-two-g-wrap.raw");
    write_fixture(&gwrap, format!("{}\r\n", MARKERS_G[0]).as_bytes());
    sc.probe(
        "g-wrap-marker-gone",
        "one-of-two transcript missing the tid-wrap marker",
        MARKERS_G.iter().all(|m| contains_fixed(&std::fs::read(&gwrap).unwrap_or_default(), m)),
    );

    let hgone = tmp.join("h-marker-gone.raw");
    write_fixture(&hgone, format!("{}\r\n{}\r\n", MARKERS_G[0], MARKERS_G[1]).as_bytes());
    sc.probe(
        "h-marker-gone",
        "transcript with the two debug-facing markers and no host-band marker",
        MARKERS_H.iter().all(|m| contains_fixed(&std::fs::read(&hgone).unwrap_or_default(), m)),
    );
    let hgreen = tmp.join("h-marker-present.raw");
    write_fixture(&hgreen, format!("{}\r\n", MARKERS_H[0]).as_bytes());
    sc.accept_probe(
        "h-marker-present",
        "genuine host-band PASS marker accepted by the H sweep",
        MARKERS_H.iter().all(|m| contains_fixed(&std::fs::read(&hgreen).unwrap_or_default(), m)),
    );

    let igone = tmp.join("i-marker-gone.raw");
    write_fixture(&igone, format!("{}\r\n", MARKERS_H[0]).as_bytes());
    sc.probe(
        "i-marker-gone",
        "transcript with the host-band marker and no act-lazy-fp marker",
        MARKERS_I.iter().all(|m| contains_fixed(&std::fs::read(&igone).unwrap_or_default(), m)),
    );
    let igreen = tmp.join("i-marker-present.raw");
    write_fixture(&igreen, format!("{}\r\n", MARKERS_I[0]).as_bytes());
    sc.accept_probe(
        "i-marker-present",
        "genuine act-lazy-fp PASS marker accepted by the I sweep",
        MARKERS_I.iter().all(|m| contains_fixed(&std::fs::read(&igreen).unwrap_or_default(), m)),
    );

    // The band-interrupt control's markers, reject side: a transcript with
    // the armed line, the digest line and the landing line but no PASS line
    // must not satisfy the M sweep; accept side: the four genuine lines.
    let mgone = tmp.join("m-marker-gone.raw");
    write_fixture(&mgone, format!("{}\r\n{}\r\n{}\r\n", MARKERS_M[0], MARKERS_M[1], MARKERS_M[2]).as_bytes());
    sc.probe(
        "m-marker-gone",
        "transcript with the fp-band armed, digest and landing lines and no PASS line",
        MARKERS_M.iter().all(|m| contains_fixed(&std::fs::read(&mgone).unwrap_or_default(), m)),
    );
    let mgreen = tmp.join("m-marker-present.raw");
    write_fixture(&mgreen, format!("{}\r\n{}\r\n{}\r\n{}\r\n", MARKERS_M[0], MARKERS_M[1], MARKERS_M[2], MARKERS_M[3]).as_bytes());
    sc.accept_probe(
        "m-marker-present",
        "genuine fp-band armed, digest, landing and PASS lines accepted by the M sweep",
        MARKERS_M.iter().all(|m| contains_fixed(&std::fs::read(&mgreen).unwrap_or_default(), m)),
    );

    // The sources markers, reject side: a transcript that carries the
    // lazy floating-point marker but neither sources marker must not
    // satisfy the J sweep; accept side: the two genuine markers.
    let jgone = tmp.join("j-marker-gone.raw");
    write_fixture(&jgone, format!("{}\r\n", MARKERS_I[0]).as_bytes());
    sc.probe(
        "j-marker-gone",
        "transcript with the act-lazy-fp marker and neither sources marker",
        MARKERS_J.iter().all(|m| contains_fixed(&std::fs::read(&jgone).unwrap_or_default(), m)),
    );
    let jgreen = tmp.join("j-marker-present.raw");
    write_fixture(&jgreen, format!("{}\r\n{}\r\n", MARKERS_J[0], MARKERS_J[1]).as_bytes());
    sc.accept_probe(
        "j-marker-present",
        "genuine entropy-secure and calendar-door PASS markers accepted by the J sweep",
        MARKERS_J.iter().all(|m| contains_fixed(&std::fs::read(&jgreen).unwrap_or_default(), m)),
    );

    // The PWM and ADC markers, reject side: a transcript that carries the
    // three armed lines and no PASS line must not satisfy the P sweep;
    // accept side: the six genuine prefixes.
    let pgone = tmp.join("p-marker-gone.raw");
    write_fixture(&pgone, format!("{}\r\n{}\r\n{}\r\n", MARKERS_P[0], MARKERS_P[2], MARKERS_P[4]).as_bytes());
    sc.probe(
        "p-marker-gone",
        "transcript with the three PWM and ADC armed lines and no PASS line",
        MARKERS_P.iter().all(|m| contains_fixed(&std::fs::read(&pgone).unwrap_or_default(), m)),
    );
    let pgreen = tmp.join("p-marker-present.raw");
    write_fixture(&pgreen, MARKERS_P.iter().map(|m| format!("{m} the rest of the line)\r\n")).collect::<String>().as_bytes());
    sc.accept_probe(
        "p-marker-present",
        "genuine PWM and ADC armed and PASS prefixes accepted by the P sweep",
        MARKERS_P.iter().all(|m| contains_fixed(&std::fs::read(&pgreen).unwrap_or_default(), m)),
    );

    // The image records: a synthesized image + symbol dump in the
    // genuine shape (the header at load + 0x400 stating that load
    // address, the generation include/rhu.h states, a version-2 layout
    // record with four 4-byte tls slots at offset 48 inside a 140-byte
    // TCB) accepted; then nine doctored shapes, one mechanism each,
    // refused.
    let header_text = std::fs::read_to_string(repo.join("src/kernel/include/rhu.h")).unwrap_or_default();
    let abi = header_abi(&header_text).unwrap_or(0);
    let load: u32 = 0x0800_0000;
    let good_h: [u32; 16] = [HEADER_MAGIC, 1, 64, load, abi, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
    let good_l: [u32; 16] = [LAYOUT_MAGIC, 2, 16, 100, 64, 140, 0, 12, 13, 20, 32, 4, 8, 48, 4, 4];
    let (gd, ge) = synth_records(load, load, &good_h, &good_l);
    sc.accept_probe(
        "records-genuine",
        "synthesized image with the header at load + 0x400 and a version-2 layout record accepted, every tie holding",
        check_records(&gd, &ge, abi).is_ok(),
    );
    let nohdr = format!("{:08x} R __rhu_debug_layout\n", load + 0x800).into_bytes();
    sc.probe(
        "records-header-absent",
        "symbol dump without __rhu_image_header (an image that states nothing)",
        check_records(&nohdr, &ge, abi).is_ok(),
    );
    let mut h = good_h;
    h[0] = 0x4948_5552;
    let (d, e) = synth_records(load, load, &h, &good_l);
    sc.probe("records-magic", "image header with a wrong magic word", check_records(&d, &e, abi).is_ok());
    let mut h = good_h;
    h[3] = load + 4;
    let (d, e) = synth_records(load, load, &h, &good_l);
    sc.probe(
        "records-offset",
        "image header stating a load address whose + 0x400 is not where the header sits",
        check_records(&d, &e, abi).is_ok(),
    );
    let (d, e) = synth_records(load, load - 0x1000, &good_h, &good_l);
    sc.probe(
        "records-load-addr",
        "image whose lowest load segment sits below the header's stated load address",
        check_records(&d, &e, abi).is_ok(),
    );
    let mut h = good_h;
    h[9] = 1;
    let (d, e) = synth_records(load, load, &h, &good_l);
    sc.probe("records-reserved", "image header with a reserved word set", check_records(&d, &e, abi).is_ok());
    let mut h = good_h;
    h[4] = abi + 1;
    let (d, e) = synth_records(load, load, &h, &good_l);
    sc.probe(
        "records-abi",
        "image header stating the next contract generation beside include/rhu.h's",
        check_records(&d, &e, abi).is_ok(),
    );
    let mut l = good_l;
    l[1] = 3;
    let (d, e) = synth_records(load, load, &good_h, &l);
    sc.probe(
        "records-version-tie",
        "layout record at version 3 beside a header stating version 2",
        check_records(&d, &e, abi).is_ok(),
    );
    let mut h = good_h;
    h[5] = 1;
    let mut l = good_l;
    l[1] = 1;
    let (d, e) = synth_records(load, load, &h, &l);
    sc.probe(
        "records-predates-door",
        "version-1 layout record beside a header stating version 1 (no thread-local-base door to read)",
        check_records(&d, &e, abi).is_ok(),
    );
    let mut l = good_l;
    l[13] = 132;
    let (d, e) = synth_records(load, load, &good_h, &l);
    sc.probe(
        "records-tls-overrun",
        "layout record whose tls slots run past the TCB stride",
        check_records(&d, &e, abi).is_ok(),
    );

    // The judge of an emulated run's ending (the Cortex-M33 row's cases and
    // every control): a hang, a wrong exit, an exit without the door and a
    // control that stayed alive are each refused; the genuine endings accepted.
    let complete = tmp.join("ending-complete.raw");
    write_fixture(
        &complete,
        b"[api] PASS fp-double (the block)\r\n[api] exit-door: handing the machine exit to rhu_exit(0)\r\n[tick] witness: not counted (no cycle counter on this row)\r\n",
    );
    let complete_bytes = std::fs::read(&complete).unwrap_or_default();
    let nodoor = tmp.join("ending-no-door.raw");
    write_fixture(
        &nodoor,
        b"[api] PASS fp-double (the block)\r\n[sched] ALL GOOD (threads + timers + preemption online)\r\n",
    );
    let nodoor_bytes = std::fs::read(&nodoor).unwrap_or_default();
    sc.probe(
        "ending-hang-after-the-lines",
        "a complete transcript whose run was killed at the harness's deadline (exit 124)",
        run_ending(124, 0, &complete_bytes).is_ok(),
    );
    sc.probe(
        "ending-wrong-exit",
        "a complete transcript whose machine exited 1 where 0 was wanted",
        run_ending(1, 0, &complete_bytes).is_ok(),
    );
    sc.probe(
        "ending-no-exit-door",
        "exit 0 on a transcript that never printed the exit-door line",
        run_ending(0, 0, &nodoor_bytes).is_ok(),
    );
    sc.probe(
        "ending-control-alive",
        "a control that must die (exit 1 wanted) whose machine exited 0 through the door",
        run_ending(0, 1, &complete_bytes).is_ok(),
    );
    sc.accept_probe(
        "ending-genuine",
        "exit 0 with the exit-door line accepted, and a designed death at exit 1 without it",
        run_ending(0, 0, &complete_bytes).is_ok() && run_ending(1, 1, &nodoor_bytes).is_ok(),
    );

    sc.probe(
        "hang-ended-by-itself",
        "a guest that must never end whose machine exited 0",
        hang_refused(0, None, true, b"").is_ok(),
    );
    sc.probe(
        "hang-spin-read-as-no-work",
        "a guest that never leaves its loop stopped for doing no work",
        hang_refused(124, Some(Stop::NoWork), true, b"").is_ok(),
    );
    sc.probe(
        "hang-printed",
        "a generated guest whose transcript is not empty",
        hang_refused(124, Some(Stop::Work), true, b"x").is_ok(),
    );
    sc.probe(
        "hang-no-rule",
        "a run recorded as stopped with no rule named",
        hang_refused(124, None, false, b"").is_ok(),
    );
    sc.accept_probe(
        "hang-genuine",
        "a working guest stopped for its work, a waiting guest stopped for no work or for work, and either stopped by the wall clock on a host with no reading",
        hang_refused(124, Some(Stop::Work), true, b"").is_ok()
            && hang_refused(124, Some(Stop::NoWork), false, b"").is_ok()
            && hang_refused(124, Some(Stop::Work), false, b"").is_ok()
            && hang_refused(124, Some(Stop::Deadline), true, b"").is_ok()
            && hang_refused(124, Some(Stop::Deadline), false, b"").is_ok(),
    );

    // The UART markers' sweep: a transcript that lost exactly one of the
    // twelve (the last PASS line) must not satisfy it.
    let s_partial = tmp.join("eleven-of-twelve-s.raw");
    let mut eleven = Vec::new();
    for m in MARKERS_S.iter().filter(|m| !m.contains("PASS uart-timeout")) {
        eleven.extend_from_slice(m.as_bytes());
        eleven.extend_from_slice(b"\r\n");
    }
    write_fixture(&s_partial, &eleven);
    let eleven_bytes = std::fs::read(&s_partial).unwrap_or_default();
    sc.probe(
        "uart-marker-gone",
        "a transcript that lost the uart-timeout PASS line (eleven of the twelve UART markers)",
        MARKERS_S.iter().all(|m| contains_fixed(&eleven_bytes, m)),
    );
    // The boot images' exact-wire judge: a second line, a bare LF and a
    // stray byte each rejected; the genuine shapes accepted.
    let banner = b"Lamella RHU v0.2.0, QEMU mps2-an500 (Cortex-M7, 25 MHz)\r\n";
    sc.probe(
        "boot-quiet-extra-line",
        "a quiet image's wire with a second line after the banner",
        boot_transcript_fault(&[&banner[..], &b"[base] ALL GOOD\r\n"[..]].concat(), banner).is_none(),
    );
    sc.probe(
        "boot-quiet-bare-lf",
        "a quiet image's banner ending in a bare LF (the wire discipline lost)",
        boot_transcript_fault(b"Lamella RHU v0.2.0, QEMU mps2-an500 (Cortex-M7, 25 MHz)\n", banner).is_none(),
    );
    sc.probe(
        "boot-silent-one-byte",
        "a silent image's wire carrying one byte",
        boot_transcript_fault(b"\r", b"").is_none(),
    );
    sc.accept_probe(
        "boot-wires-exact",
        "the quiet image's one banner line with its CRLF and the silent image's empty wire accepted by the exact judge",
        boot_transcript_fault(banner, banner).is_none() && boot_transcript_fault(b"", b"").is_none(),
    );

    sc.accept_probe(
        "header-flags-two-words",
        "the header compile's Cortex-M33 flag set is the Cortex-M7 set apart from the class's two words",
        class_flag_sets_tie(&HEADER_FLAGS, &HEADER_FLAGS_CORTEX_M33).is_ok(),
    );
    let mut doctored_flags: Vec<&str> = HEADER_FLAGS_CORTEX_M33.to_vec();
    if let Some(slot) = doctored_flags.iter().position(|f| *f == "-Wextra") {
        doctored_flags[slot] = "-Wno-extra";
    }
    sc.probe(
        "header-flags-doctored",
        "the header compile's Cortex-M33 flag set with a third word moved offered as the class variant",
        class_flag_sets_tie(&HEADER_FLAGS, &doctored_flags).is_ok(),
    );
    let header_text = std::fs::read_to_string(repo.join("src").join("kernel").join("include").join("rhu.h")).unwrap_or_default();
    sc.accept_probe(
        "header-doctor-anchor-once",
        "the control's doctor finds the prototype of rhu_net_count once in include/rhu.h",
        header_text.matches(HEADER_DOCTOR_ANCHOR).count() == 1,
    );
    sc.probe(
        "header-doctor-anchor-doubled",
        "a header copy carrying the prototype of rhu_net_count twice offered to the doctor's anchor count",
        format!("{header_text}{HEADER_DOCTOR_ANCHOR}\n").matches(HEADER_DOCTOR_ANCHOR).count() == 1,
    );
    sc.accept_probe(
        "header-refusal-words",
        "a refused compile whose error stream carries the collision's words accepted by the control's judge",
        header_refusal(false, &format!("rhu.h:904:9: error: {HEADER_REFUSAL_WORDS}\n")).is_ok(),
    );
    sc.probe(
        "header-refusal-compiled",
        "a doctored copy the compiler accepted offered to the control's judge",
        header_refusal(true, "").is_ok(),
    );
    sc.probe(
        "header-refusal-other-words",
        "a doctored copy refused for another reason (an unknown type name) offered to the control's judge",
        header_refusal(false, "rhu.h:12:10: error: unknown type name 'uint32_t'\n").is_ok(),
    );

    sc.finish("genuine inputs accepted");
}

/// The two numbers of a watermark line after `prefix` ("N of M bytes"): the
/// high-water and the reserve, or None when the line or its numbers are
/// missing.
fn parse_high_water(raw: &[u8], prefix: &str) -> Option<(u32, u32)> {
    let text = String::from_utf8_lossy(raw);
    let at = text.find(prefix)?;
    let rest = &text[at + prefix.len()..];
    let mut words = rest.split_whitespace();
    let used: u32 = words.next()?.parse().ok()?;
    if words.next()? != "of" {
        return None;
    }
    let reserve: u32 = words.next()?.parse().ok()?;
    Some((used, reserve))
}
