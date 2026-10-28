/* The Lamella RHU hosting API -- the C ABI for hosted payloads.
 *
 * Mirrors src/api.rs exactly; the two files change together or not at
 * all. Minimal on purpose: a futex pair plus a nanosecond clock and
 * sleep is the entire blocking surface of the kernel, and every richer
 * primitive (mutex, event, semaphore, condition variable) is a recipe
 * over these four in the layer above. No signals exist.
 */

#ifndef RHU_H
#define RHU_H

#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/* Deadline value meaning "wait forever". Deadlines are ABSOLUTE
 * nanoseconds on the rhu_now_ns clock, never relative. */
#define RHU_DEADLINE_NONE UINT64_MAX

/* The hosting-API contract generation this header describes, and the
 * window of generations a kernel built from it serves.
 *
 * RHU_ABI is the generation. A door ADDED at the end of this header
 * leaves it where it stands: a program links against the kernel
 * statically, so a program that calls a door its kernel lacks fails at
 * the link with an undefined symbol, and no generation check is needed
 * to refuse it. The generation moves when a door's shape or meaning
 * changes -- a signature, a struct layout, an error value, what an
 * argument or a result means -- since a program compiled against the
 * older header links all the same and would call the door wrongly, and
 * when a door is removed. A change an older program still runs under
 * raises RHU_ABI by one and leaves RHU_ABI_COMPAT where it stands; one
 * an older program cannot run under raises both to the same new number.
 *
 * A payload that provides its own entry states the RHU_ABI of the header
 * it compiled against and nothing else (rhu_payload_abi, at the end of
 * this header); it never states the window. The kernel runs a stated
 * generation from RHU_ABI_COMPAT through RHU_ABI and refuses any other
 * before the entry runs, with one sentence on the console that names
 * the stated generation and the window:
 *     [payload] entry REFUSED: the payload was built for hosting-API
 *     contract generation N, outside this kernel's window C..A
 * where C is RHU_ABI_COMPAT and A is RHU_ABI. */
#define RHU_ABI 41
#define RHU_ABI_COMPAT 31

/* Error surface (RHU-owned values, errno-mirroring by convention). */
#define RHU_EAGAIN (-11)    /* *addr != expected at entry; no sleep    */
#define RHU_ETIMEDOUT (-110) /* the deadline passed                    */
#define RHU_EINVAL (-22)    /* address null or not 4-aligned           */

/* Monotonic nanoseconds since scheduler init, 64-bit. Sub-millisecond
 * resolution by hardware-counter interpolation; never steps backward,
 * including across the tick-wrap window. */
uint64_t rhu_now_ns(void);

/* --- The calendar: the date and time as a BASE plus the monotonic
 * clock above. The unit is the calendar tick -- 100 nanoseconds since
 * 0001-01-01T00:00:00Z (the hosted runtime's own DateTime tick; the
 * value 0 is that epoch, RHU_CALENDAR_MAX the last instant of the year
 * 9999). The calendar at any moment is the base plus the elapsed
 * monotonic clock, so setting the base never moves rhu_now_ns. The base
 * is the epoch until a source sets it. The sources: a board's real-time
 * clock read once at boot where the part has one and it was ever set
 * (RHU_CALENDAR_RTC); the developer's tooling writing the push mailbox
 * over the debug probe (RHU_CALENDAR_PUSH); network time by an EXPLICIT
 * call (RHU_CALENDAR_NETWORK, never automatic); the application
 * (RHU_CALENDAR_APP). The first source establishes the base; any later
 * explicit set replaces it; the query says which is current. Where a
 * part has a real-time clock, an explicit set is written back to it so
 * the next boot starts true across a reset, and across a power cycle on
 * a board whose clock has backup power; the boot banner states each
 * row's posture. */
#define RHU_CALENDAR_MAX 3155378975999999999ull
#define RHU_CALENDAR_UNSET 0
#define RHU_CALENDAR_RTC 1
#define RHU_CALENDAR_PUSH 2
#define RHU_CALENDAR_NETWORK 3
#define RHU_CALENDAR_APP 4

/* The calendar now and the source that set the base: 0; RHU_EINVAL for
 * a null or misaligned pointer, writing nothing. */
int32_t rhu_calendar_get(uint64_t *ticks, int32_t *source);

/* The explicit set: the calendar reads ticks from now on, the source
 * recorded (RHU_CALENDAR_NETWORK or RHU_CALENDAR_APP only -- the kernel
 * records its own two sources itself), the real-time clock written back
 * where the part has one. Returns 0; RHU_EINVAL for another source or
 * ticks past RHU_CALENDAR_MAX. Never blocks. */
int32_t rhu_calendar_set(uint64_t ticks, int32_t source);

/* Sleep at least ns nanoseconds (relative request). NEVER returns
 * early, however long the request (a target beyond the scheduler's
 * 32-bit tick window is re-armed at the window's edge until it lies
 * inside); may return up to ~2 ms late at the 1 kHz tick. */
void rhu_sleep_ns(uint64_t ns);

/* Atomically (with respect to rhu_futex_wake) compare *addr against
 * expected and block while they match. Returns 0 when woken by
 * rhu_futex_wake, RHU_EAGAIN when *addr != expected at entry (no sleep
 * happened -- the caller re-reads its state and decides), RHU_ETIMEDOUT
 * when deadline_ns passed (never early -- a deadline beyond the
 * scheduler's 32-bit tick window is honored to the tick, re-armed at the
 * window's edge until it lies inside), RHU_EINVAL on a bad address.
 * addr must be 4-aligned, non-null, and mapped for the life of the
 * wait. RHU_EINTR means an activation was delivered during the wait;
 * the wait may not have been satisfied -- re-evaluate the predicate
 * and re-wait, exactly as for a spurious wake. */
int32_t rhu_futex_wait(const uint32_t *addr, uint32_t expected,
                       uint64_t deadline_ns);

/* Ready up to n threads blocked on addr; returns the count readied
 * (0 when no thread waits there). Selection is priority-first among
 * the waiters on addr (the most urgent readied first; ties in scheduler
 * slot rotation order), never FIFO. A readied thread that is equal or
 * more urgent than the caller preempts immediately. */
int32_t rhu_futex_wake(const uint32_t *addr, uint32_t n);

/* --- The arena: the GC's memory substrate. One arena per
 * image; bump-only (individual frees never exist); rezero is the
 * recycling story. Every byte ever handed out was zero at the moment
 * of hand-out (the zero-birth invariant: zeroed at boot or at the
 * hand-out by the image's init mode; the door returns after). All verbs
 * run under the kernel critical section: single-core, short, never
 * blocking. */

/* Aligned bump allocation attributed to `site` (an opaque caller key;
 * C callers use the rhu_arena_alloc macro and get their return address
 * for free). Returns NULL on honest EXHAUSTION only. A wild argument
 * (align not a power of two or over the window's size, or bytes == 0)
 * is a caller BUG and dies loud through the funnel -- never a quiet
 * NULL. Any power of two up to the window's size is honoured. Sizes
 * round up to 8; the returned block satisfies `align`. */
void *rhu_arena_alloc_at(uint32_t bytes, uint32_t align, uint32_t site);
#define rhu_arena_alloc(b, a) \
    rhu_arena_alloc_at((b), (a), (uint32_t)__builtin_return_address(0))

/* Re-establish the zero-birth invariant over [p, p+bytes): after a 0
 * return the range reads zero, the address stays the caller's, and no
 * new address space was consumed (headroom and the next hand-out
 * address are untouched). The zeroing runs with the kernel's interrupt
 * band live: no critical section is held across it, however long the
 * range. The range must lie inside handed-out arena space; anything
 * else returns RHU_EINVAL and touches nothing. */
int32_t rhu_arena_rezero(void *p, uint32_t bytes);

/* Bytes an immediately following 8-aligned allocation could still get,
 * exactly. 0 means exhausted. */
uint32_t rhu_arena_headroom(void);

/* Dump the per-site allocation audit table to the console. Diagnostic:
 * noisy by design, never called by a shipped image. */
void rhu_arena_report(void);

/* --- Kheap, N-module TLS, the thread C-ABI, the module
 * registry. Same laws as the arena: every verb runs under the kernel
 * critical section, none blocks, NULL/RHU_EAGAIN are honest refusals,
 * wild ARGUMENTS die loud through the funnel. Join is a RECIPE, not a
 * verb: futex-wait on a word the exiting thread wakes -- the futex
 * pair stays the kernel's only blocking surface, permanently. */

#define RHU_ESRCH (-3)      /* no such thread (tid unknown or dead)    */

/* malloc-class kernel heap, carved from the arena at boot (KHEAP_BYTES,
 * a board const). Address-ordered first-fit with coalescing on free;
 * block headers carry a state word and a canary, and a corrupt or
 * double-freed header dies LOUD through the funnel -- heap corruption
 * is never a quiet survivor. No realloc/calloc here: shim recipes over
 * this pair. NULL = honest exhaustion, or a request the heap could never
 * hold (its rounding wraps, or its span with the header exceeds the
 * heap's own size) -- refused with a console line, nothing touched;
 * align must be a power of two <= 4096 and bytes nonzero (else: caller
 * bug, loud). */
void *rhu_kheap_alloc(uint32_t bytes, uint32_t align);
void rhu_kheap_free(void *p);   /* NULL is a no-op */

/* One module's registration record. The STRUCT is copied at
 * registration (the caller's may die); the two POINTERS are kept as
 * pointers -- their referents must outlive the module. Unused statics
 * ranges stay zero. frozen/exidx ranges AND the TLS template
 * (tls_bytes/tls_init) come from the link brackets (module 0 wires
 * all three at boot; empty ranges are honest -- base-family images
 * define the brackets zero-length, and an image whose payload carries
 * no thread-local input registers tls_bytes 0 the same way). */
typedef struct rhu_module_desc {
    uint32_t text_lo, text_hi;
    uint32_t statics_lo[4], statics_hi[4];
    uint32_t frozen_lo, frozen_hi;
    uint32_t exidx_lo, exidx_hi;
    uint32_t tls_bytes;         /* per-thread TLS block size (0 = none) */
    const void *tls_init;       /* init image, tls_bytes long; NULL = zero-filled */
    const char *name;
} rhu_module_desc;

/* Register a module -> handle 0..RHU_MODULE_MAX-1, or RHU_EAGAIN (table
 * full, or a TLS block for a live thread was refused -- the
 * registration unwinds wholesale and holds nothing). Module 0 is the
 * merged image itself, kernel-self-registered at boot. TLS is EAGER in
 * both directions: registering allocates this module's block for every
 * live thread; spawning allocates every registered module's block for
 * the new thread -- so a lazy-allocation surprise never exists. There
 * is no unregister: modules never unload. A registered module's text
 * range is also where the single-precision rows' double-precision
 * emulation accepts a faulting instruction's address: a form outside
 * every registered range is refused and the fault is terminal. */
#define RHU_MODULE_MAX 4
int32_t rhu_module_register(const rhu_module_desc *d);

/* The calling thread's TLS block for a module handle. A valid handle
 * with tls_bytes > 0 never returns NULL (eager allocation, by
 * construction). An invalid handle, or one registered with tls_bytes
 * == 0, is a caller BUG and dies loud. */
void *rhu_tls_get(int32_t module);

/* Push a thread-exit destructor (LIFO, RHU_ATEXIT_SLOTS per thread;
 * full -> RHU_EAGAIN, loudly honest). Dtors run on the exiting thread
 * after `entry` returns or rhu_thread_exit is called, most-recent
 * first, BEFORE the thread's TLS is freed -- a dtor may still read its
 * own TLS. */
#define RHU_ATEXIT_SLOTS 8
int32_t rhu_thread_atexit(void (*fn)(void *), void *arg);

/* The hosted priority band (numerically lower is more urgent): a spawn's
 * `prio` clamps into [RHU_HOSTED_PRIO_LO, RHU_HOSTED_PRIO_HI]; the flat
 * value is the one level every hosted thread shares, the payload host
 * thread included while it runs the entry. Spelled here, once, and
 * mirrored by the kernel; a shim adopts these names rather than a
 * literal of its own. */
#define RHU_HOSTED_PRIO_LO 0x10
#define RHU_HOSTED_PRIO_HI 0xE0
#define RHU_HOSTED_PRIO_FLAT 0x80

/* The smallest stack a hosted thread may be spawned with, bytes: the
 * frames the birth writes (32 + 36), an extended floating-point frame's
 * extra 104 and an activation delivery's 140 sum to 312, rounded up to
 * the next power of two. A floor for the kernel's own frames, not a
 * budget for the thread's code: a caller sizes above it for what the
 * thread runs. */
#define RHU_THREAD_STACK_MIN 512

/* Spawn a hosted thread -> tid >= 1, RHU_EINVAL (stack_bytes below
 * RHU_THREAD_STACK_MIN, or a size whose 8-byte round-up wraps -- refused
 * before anything moves), or RHU_EAGAIN (no TCB slot, stack
 * pool exhausted, or a TLS block refused -- an honest refusal; hosted
 * callers own their retry story, kernel-internal birth errors stay
 * loud panics). `prio` clamps into the hosted band [RHU_HOSTED_PRIO_LO,
 * RHU_HOSTED_PRIO_HI] (the kernel's urgent band and idle stay
 * unreachable from C; the managed-priority mapping INTO the band is the
 * hosting shim's). RHU_HOSTED_PRIO_FLAT is the one priority every
 * hosted thread runs at: the thread that runs the payload entry moves
 * to it for the entry's life (rhu_payload_entry, below), so a shim that
 * spawns at this value lands its threads level with their host --
 * equal-priority round-robin, and a start waited for by yield alone
 * completes. `name` is copied at the door (at most 15 bytes kept, cut
 * at a character boundary), so the caller's buffer may go away once the
 * door returns; NULL reads as "hosted".
 * Stacks come from the .kstacks pool with exact-fit reuse of retired
 * stacks. Tid numbering in a hosted image: the idle thread holds tid 1
 * and the payload host thread (the kernel's main thread) tid 2, so the
 * first hosted spawn lands tid 3. Tids are
 * process-unique and monotonic, never reused within a run: a stale tid
 * resolves RHU_ESRCH and never to a newer thread. The id space is the
 * positive int32_t range: a spawn that would exceed it is refused
 * RHU_EAGAIN with nothing committed, never handed a negative id (the
 * counter does not wrap). A refused spawn prints one kernel line naming
 * the reason and, when the stack pool is short, the request, the pool's
 * size, its remainder and its retired stacks. */
int32_t rhu_thread_spawn(void (*entry)(void *), void *arg,
                         uint32_t stack_bytes, uint32_t prio,
                         const char *name);
void rhu_thread_exit(void);     /* runs dtors, frees TLS, never returns */
int32_t rhu_thread_self(void);  /* the calling thread's tid */
/* Give the CPU to every equal-or-more-urgent Ready peer: the switch is
 * taken before this call returns (a synchronous scheduling point). */
void rhu_thread_yield(void);

/* A live thread's stack carve [*lo, *lo + *bytes), by tid, exactly as
 * spawned -- the GC's stack-scan input, answered
 * natively. Unknown or dead tid -> RHU_ESRCH; null out-pointers ->
 * RHU_EINVAL (the futex null convention). */
int32_t rhu_thread_stack_bounds(int32_t tid, uint32_t *lo,
                                uint32_t *bytes);

/* --- Suspension + activation -- THE stop-the-world seam,
 * consumed in full by the GC. Suspension is a FLAG
 * orthogonal to thread state: a suspended thread keeps its state and
 * its waits; it is simply never picked to run. Activation is the
 * scheduler-owned, pending-once, materialized-context mechanism --
 * NOT a signal: no queue, no mask, no per-thread handler table;
 * sends coalesce. Neither family blocks; the futex pair remains the
 * kernel's entire blocking surface. */

#define RHU_EINTR (-4)      /* futex wait broken by activation delivery */

/* A thread's integer context, as captured at suspension and as handed
 * to the activation handler. r[n] is rn; sp is the post-pop stack
 * pointer of the interrupted context and is a SNAPSHOT, never
 * writable (an sp edit is refused RHU_EINVAL by
 * rhu_thread_set_context and ignored at activation return -- frame
 * geometry never moves); xpsr is informational (writes ignored,
 * except that a pc edit clears the restored ITSTATE bits -- resuming
 * an edited pc into a live IT block would be unpredictable, so the
 * safe form is the only form); bit 0 of a written pc is cleared (the
 * Thumb bit lives in xpsr.T). flags bit 0 = FP-extended frame
 * present; FP state stays in-frame untouched either way (references
 * never live in FP registers). flags bit 8 = the context is a delivered
 * fault context (rhu_fault_ctx, below: the activation handler is also
 * the fault funnel's delivery entry once a hosting layer has armed it
 * through rhu_fault_register), its kind in bits 15:12. */
typedef struct rhu_ctx {
    uint32_t r[13];         /* r0-r12 */
    uint32_t sp, lr, pc, xpsr;
    uint32_t flags;         /* bit 0: FP-extended frame */
} rhu_ctx;

/* Suspend a thread and fill *out_ctx from its saved context. The
 * target keeps its state (Ready stays Ready; a futex waiter keeps
 * its wait; a sleeper keeps its deadline) -- it is simply never
 * picked until resumed. A thread inside the kernel critical section
 * can never be caught by this (it is never switched out mid-CS), so
 * the suspend-vs-kernel-lock deadlock class is closed by
 * construction. Refusals: RHU_EINVAL null or not 4-aligned out_ctx,
 * self, or idle;
 * RHU_ESRCH unknown or dead tid; RHU_EAGAIN already suspended (the
 * caller's bookkeeping is broken and hears so). */
int32_t rhu_thread_suspend(int32_t tid, rhu_ctx *out_ctx);

/* Edit a SUSPENDED thread's context in place (registers and pc, never
 * geometry). ctx->sp must equal the suspension snapshot's
 * sp or the whole edit is refused RHU_EINVAL and nothing is written.
 * The restore is always an exception return -- no path in this
 * kernel restores a context inline. Refusals: RHU_EINVAL null or not
 * 4-aligned ctx, or sp mismatch; RHU_ESRCH unknown or dead tid;
 * RHU_EAGAIN not
 * suspended. */
int32_t rhu_thread_set_context(int32_t tid, const rhu_ctx *ctx);

/* Clear the suspension flag. A pending activation deferred by
 * suspension becomes deliverable again: a suspended-then-resumed
 * blocked target has its wait broken NOW (delivery itself always
 * happens at the target's next scheduling-in). Preempts the caller
 * iff the target is now pickable and equal-or-more urgent. Refusals:
 * RHU_ESRCH unknown or dead tid; RHU_EAGAIN not suspended. */
int32_t rhu_thread_resume(int32_t tid);

/* Register THE process-global activation handler, once (the runtime
 * owns it; per-thread or per-send variants are recipes above the
 * kernel, never kernel shape). The handler runs ON the target thread
 * with a pointer to the interrupted context, which it may edit in
 * place under the set-context rules; on return the (possibly edited)
 * context is resumed by exception return. The handler may use the
 * floating-point unit: lazy floating-point state its instructions
 * leave pending is completed before the interrupted context resumes,
 * whatever that context's own frame type. Refusals: RHU_EINVAL null
 * handler or second registration. */
int32_t rhu_activation_register(void (*handler)(rhu_ctx *));

/* Mark an activation pending on a thread: delivered ONCE at the
 * target's next scheduling-in, on the target's own stack (cost: one
 * ctx + one frame pair = 140 bytes, plus the handler's own need --
 * budget it when sizing spawn stacks). SENDS COALESCE (the flag is
 * the whole queue); never blocks. A send during the handler's own
 * run re-enters the handler once more before the interrupted flow
 * resumes (re-delivery reuses the same reservation -- the stack cost
 * never nests). A blocked target is broken out promptly: a futex
 * waiter's wait returns RHU_EINTR after the handler has run; a plain
 * sleeper re-arms its remaining time and its caller sees nothing. A
 * suspended target latches the flag and delivery waits for resume. A
 * pending activation on a thread that exits is discarded. Self-send
 * is delivered at the sender's own next scheduling point. Refusals:
 * RHU_EINVAL no handler registered, or idle target; RHU_ESRCH
 * unknown or dead tid. */
int32_t rhu_activation_send(int32_t tid);

/* --- Modules/exidx plumbing -- the registry's query doors,
 * completing module 0 (frozen + exidx wired at boot from the link
 * brackets; every board map defines the bracket symbols:
 * section-exact in payload maps, zero-length in base maps -- one
 * registration code path, both families). Queries copy the stored
 * desc under the kernel critical section and never block; no new
 * error values (RHU_ESRCH is the miss surface). */

/* Containing-module lookup by text range: returns the handle (>= 0)
 * of the FIRST registered module whose half-open [text_lo, text_hi)
 * contains addr, and fills *out from the stored desc. Ranges may
 * overlap (registration order is the tiebreak; module 0 registers
 * first). Miss -> RHU_ESRCH with *out untouched; null out ->
 * RHU_EINVAL (the futex null convention). */
int32_t rhu_module_find(uint32_t addr, rhu_module_desc *out);

/* The GC-facing enumeration: fill *out with the stored desc for a
 * handle; 0 on success. Frozen ranges are ROOT SOURCES every
 * collection -- no write barrier ever records frozen->heap stores,
 * so this query is the collector's only honest path to them.
 * Statics ranges serve audit/diagnostics and the conservative
 * fallback; PRECISE statics roots arrive via the EE's own machinery
 * at the payload world (stated so nobody mistakes the registry for
 * the precise-root story). Walk handles from 0 until RHU_ESRCH
 * (unknown handle; *out untouched). Null out -> RHU_EINVAL. */
int32_t rhu_module_roots(int32_t handle, rhu_module_desc *out);

/* --- The hosting doors: console, exit, entropy. The C runtime
 * layer's plumbing verbs. Same laws as everything above: none of
 * these blocks (the futex pair stays the kernel's entire blocking
 * surface), NULL arguments are refused RHU_EINVAL, wild pointers
 * fault loud through the funnel. */

/* Write n bytes to the kernel console as ONE atomic write (concurrent
 * writers never tear it; size lines accordingly). Bytes pass through
 * the kernel's wire grammar: every '\n' goes out as "\r\n". The call
 * returns when the last byte entered the kernel's transmit path: on a
 * board whose console is driven by a transmit ring that is the ring,
 * and the call first waits -- outside the kernel's critical section,
 * with interrupts open -- until the ring has room for the write's wire
 * length; on a polled board it is the transmitter itself. Either way
 * the wait is bounded by hardware drain, never by the scheduler.
 * Returns n. Refusals: RHU_EINVAL for NULL buf with n > 0, or n beyond
 * RHU_CONSOLE_MAX_WRITE (one write is bounded; a larger buffer is
 * written in bounded pieces by the caller); n == 0 writes nothing and
 * returns 0. */
#define RHU_CONSOLE_MAX_WRITE 4096
int32_t rhu_console_write(const void *buf, uint32_t n);

/* Drain up to n pending console input bytes into buf and return the
 * count -- 0 means no input, and the call NEVER waits for any (a
 * blocking read is a recipe: poll; no readiness word is exported
 * for a futex wait). Bytes are raw wire
 * bytes: no echo, no line editing, no CR/LF translation. Refusals:
 * RHU_EINVAL for NULL buf with n > 0; n is not bounded (the drain
 * stops at the pending bytes, at most the receive ring's depth). */
int32_t rhu_console_read(void *buf, uint32_t n);

/* --- The two streams, their channels and the kernel's output level.
 * The kernel's console carries TWO STREAMS: the
 * LOG stream (the kernel's diagnostic lines, and a hosting layer's through
 * rhu_log_write) and the CONSOLE stream (the program's standard output and
 * standard error through rhu_console_write, its input through
 * rhu_console_read). Each stream is routed at BUILD TIME to a channel by a
 * link-time word the prebuilt kernel reads at boot (PROVIDE(... = 0) in
 * every link and payload script = the row's default; the SDK passes
 * --defsym from the project's properties):
 *   __rhu_log_channel, __rhu_console_channel:
 *     0        the row's default (the facts file's `channels` row)
 *     1        off     the stream discarded whole
 *     2        memory  no wire: the 16 KB memory console read over the
 *                      debug port (the Pico rows' default; no MCU pin taken)
 *     3        probe   the ST-Link boards' virtual COM port (their default)
 *     0x10 + n uart n  a UART of the row's facts table, held by the kernel
 *                      (the emulator rows' default is uart 0, QEMU's stdio)
 *   The 16 KB memory console `__rhu_console_log` is a TEE: every line of
 *   either stream lands in it on every row whatever the channel says, and
 *   the channel names the wire. Its header carries the program's end: the
 *   fourth word (offset 12) is the exit code, 0 while the program runs and
 *   the door's code whole at the end, stored first; then, after a barrier,
 *   the magic word (offset 0) turns from "RLOG" into "RRET" (returned) or
 *   "RABT" (aborted) in one write by the exit doors and the fault funnel
 *   after the transcript's last byte. A value illegal for the row is refused at
 *   the SDK's build; an image that reaches the kernel with one boots on the
 *   row's default and says so in one line.
 *   __rhu_uart<n>_tx, __rhu_uart<n>_rx: a project's pin override for a UART
 *     row's pre-defined pair (the UART doors below): 0x100 | gpio when a pin
 *     is named (bit 8 the valid flag, the low byte the GPIO number), 0 = the
 *     facts' pair, any other value malformed and refused as an illegal pair;
 *     validated against the chip's legal pin sets (the facts' uartPairs: any
 *     TX-capable pin with any RX-capable pin of the block, the two
 *     different) at the SDK's build and again at the claim.
 *   __rhu_stack_watermark: 0 = off (every map's default) | 1 = the boot
 *     stack reserve (the main stack: the boot's, then every handler's) is
 *     filled with a pattern at reset and its deepest use is printed at the
 *     program's end as one verbose line of the log stream, beside the
 *     reserve's size. The kernel feature `stack-watermark` turns it on in
 *     the kernel's own test images.
 *   __rhu_log_level: 1 silent | 2 quiet | 3 verbose (0 = unset = verbose).
 *     A line of the log stream carries the level of the quietest image
 *     that still prints it: silent prints nothing but a fatal error's last
 *     words (the funnel's reports); quiet, the SDK's default, prints one
 *     line naming the version, the board and its clock; verbose prints
 *     everything (the boot report, the flavors' lines, a hosting layer's).
 *     The console stream is never filtered. */
#define RHU_LOG_SILENT 1
#define RHU_LOG_QUIET 2
#define RHU_LOG_VERBOSE 3

/* The image's output level, 1 to 3: a hosting layer skips formatting a
 * line it would drop. */
int32_t rhu_log_level(void);

/* Write n bytes of the LOG stream at a level, as ONE atomic write through
 * the wire grammar (the lock, the bound and the back-pressure of
 * rhu_console_write): printed when level <= rhu_log_level(), dropped
 * otherwise; returns n either way. Refusals: RHU_EINVAL for a level
 * outside 1..3, for NULL buf with n > 0, or n beyond RHU_CONSOLE_MAX_WRITE;
 * n == 0 writes nothing and returns 0. */
int32_t rhu_log_write(uint32_t level, const void *buf, uint32_t n);

/* The program's end, two doors. rhu_exit: the program returned or
 * exited with code. rhu_abort: the program ended without returning (an
 * exception nothing caught, a fail-fast); code is the ending path's.
 * Either door stores the program's end into the console mirror's header
 * (the exit code whole, then the end magic; the channels' note above)
 * and ends the machine:
 * on a hosted-emulator world the code reaches the host's exit status;
 * on silicon the machine parks in low-power idle (the terminal state --
 * there is nothing to return the code to). Neither returns; neither
 * runs destructors or atexit machinery of its own (the layer above owns
 * shutdown ceremony). */
void rhu_exit(int32_t code);
void rhu_abort(int32_t code);

/* Random bytes, two doors. rhu_entropy fills buf[0..n) from the
 * kernel's software stream -- well mixed, entirely predictable from its
 * seed, seeded at its first draw from the hardware source where one
 * serves -- for everything that does not need secrecy; returns 0, or
 * refuses RHU_EINVAL (NULL buf with n > 0) writing nothing; n == 0
 * returns 0 trivially; never blocks; the draw is all-or-nothing.
 * rhu_entropy_secure fills buf[0..n) with bytes for cryptography, from a
 * hardware noise source ONLY, with the source's own health flags read at
 * this draw: 0 when served; RHU_ENODEV when no hardware source serves on
 * this row or clock posture (or the source was retired); RHU_EIO when
 * the source failed its health check at this draw -- it is then retired
 * for the boot's life, rhu_entropy_is_hardware answers 0 and every later
 * secure draw refuses RHU_ENODEV; RHU_EAGAIN when the source is healthy
 * but the bytes were not ready within the door's bound (retry), or a
 * draw was already in flight; RHU_EINVAL for NULL with n > 0 or n past
 * RHU_ENTROPY_SECURE_MAX (a larger buffer is filled in bounded pieces by
 * the caller). A refusal writes nothing. The build opt-in named
 * entropy-accept-software, off by default and implied by no board, lets
 * an image -- the emulator rows' gate images -- serve the software
 * stream through this door; the boot banner says so on every image
 * that carries it. QUALITY IS DECLARED, never implied:
 * rhu_entropy_is_hardware answers 1 while a hardware source serves and
 * its last check passed, 0 otherwise -- 0 under the opt-in too. */
#define RHU_ENODEV (-19)    /* no hardware entropy source serves          */
#define RHU_EIO (-5)        /* the source failed its check; retired       */
#define RHU_ENTROPY_SECURE_MAX 256
int32_t rhu_entropy(void *buf, uint32_t n);
int32_t rhu_entropy_secure(void *buf, uint32_t n);

/* The GPIO doors: a program's GPIO driver over the kernel's lines. A line
 * is the chip's own number (the RP2350's GPIO index; ST's port letter
 * index times sixteen plus the bit: PA0 = 0, PB0 = 16). rhu_gpio_count is
 * the line SPACE (the highest line plus one); a line inside it with no pad
 * is refused RHU_EINVAL at the claim. rhu_gpio_claim takes a line as an
 * input with its pull (RHU_GPIO_INPUT with RHU_GPIO_PULL_*) or as an
 * output (push-pull or open-drain) at an initial level, written to the
 * data register before the mode so the pad never shows the wrong level at
 * open: 0 when taken; RHU_EBUSY when the board or another claim holds the
 * line; RHU_EINVAL past the space, on an unpopulated line or a bad
 * selector; RHU_EACCES for an output mode on a line the board's facts mark
 * input-only; RHU_ENODEV on a row with no lines. A claim on a line this
 * program already holds RE-CONFIGURES it (idempotent: the mode and the pull
 * re-applied, an output's initial level re-applied, an armed edge dropped
 * when an input becomes an output). A refusal programs nothing.
 * rhu_gpio_release gives the line back (RHU_EINVAL when the program does
 * not hold it). rhu_gpio_write drives a claimed output (RHU_EACCES on an
 * input, RHU_EINVAL on an unclaimed line); rhu_gpio_read returns the pad,
 * 0 or 1, on any claimed line (an output reads what the pad sees).
 * rhu_gpio_edge arms an event kind on a claimed line: RHU_GPIO_EDGE_NONE
 * disarms; RISING, FALLING and BOTH are served; the level-triggered kinds
 * and the wakeup variants are refused RHU_ENOTSUP (an edge block cannot
 * serve a level contract honestly); RHU_EBUSY when the family's routing
 * cannot serve the line (ST's sixteen interrupt lines are shared by line
 * number across the ports). rhu_gpio_event_word returns the line's event
 * counter, typed as the futex's word (NULL on an unclaimed line): the
 * kernel increments it once per detected edge and wakes its waiters, so
 * the wait recipe is -- read the word, call rhu_futex_wait(word, the value
 * read, deadline), re-read: an edge between the read and the wait returns
 * RHU_EAGAIN at once, an edge during the wait wakes it, a burst coalesces
 * into one wake and a count of two. With BOTH armed, a pulse shorter than
 * the handler's latency counts once on a family that latches one pending
 * bit per line (the STM32F7, which then reads the direction off the pad)
 * and twice on a family that latches each edge (the STM32U5, the RP2350).
 * No callback and no blocking verb: the program's own thread waits. */
#define RHU_EBUSY (-16)     /* the line, or its interrupt routing, is held   */
#define RHU_EACCES (-13)    /* a write on an input; an output on input-only  */
#define RHU_ENOTSUP (-95)   /* a level-triggered or wakeup event kind        */
#define RHU_GPIO_INPUT 0
#define RHU_GPIO_OUTPUT 1
#define RHU_GPIO_OUTPUT_OPEN_DRAIN 2
#define RHU_GPIO_PULL_NONE 0
#define RHU_GPIO_PULL_UP 1
#define RHU_GPIO_PULL_DOWN 2
#define RHU_GPIO_EDGE_NONE 0
#define RHU_GPIO_EDGE_RISING 1
#define RHU_GPIO_EDGE_FALLING 2
#define RHU_GPIO_EDGE_BOTH 3
#define RHU_GPIO_LEVEL_LOW 4
#define RHU_GPIO_LEVEL_HIGH 5
#define RHU_GPIO_WAKE_RISING 6
#define RHU_GPIO_WAKE_FALLING 7
#define RHU_GPIO_WAKE_BOTH 8
int32_t rhu_gpio_count(void);
int32_t rhu_gpio_claim(uint32_t line, uint32_t mode, uint32_t pull, uint32_t initial);
int32_t rhu_gpio_release(uint32_t line);
int32_t rhu_gpio_write(uint32_t line, uint32_t value);
int32_t rhu_gpio_read(uint32_t line);
int32_t rhu_gpio_edge(uint32_t line, uint32_t kind);
const uint32_t *rhu_gpio_event_word(uint32_t line);
int32_t rhu_entropy_is_hardware(void);

/* The I2C doors: a program's I2C driver over one of the kernel's buses. A bus is named
 * by its NUMBER -- the chip's own block number (I2C1 is bus 1), the number a Linux
 * device node carries (/dev/i2c-1) -- never by an index: rhu_i2c_count answers how
 * many buses the board has (0 on a row with none: every other door of this family then
 * answers RHU_ENODEV) and rhu_i2c_bus answers the number of the index-th of them (the
 * index from 0 below the count; RHU_EINVAL past it), so a program enumerates without
 * guessing. A number the board has no bus of answers RHU_ENODEV at every door below.
 * rhu_i2c_claim takes a bus for the program's whole run at a rate in Hz, the bus's
 * mode: its two pins become the board's (a program holding one refuses the claim
 * RHU_EBUSY, nothing programmed), the bus is recovered if a device holds the data line
 * low (up to nine clocks, then a stop), the block is enabled -- 100000 is served on
 * every board; 400000 and 1000000 where the board's block makes them from its clock,
 * the clock then at the fastest rate at or below the request that keeps the mode's
 * minimum times; another rate, or one the block cannot make, is refused RHU_ENOTSUP,
 * judged before the bus's state and so on a claimed bus too; a bus the board has none
 * of RHU_ENODEV. A claimed bus lives for the boot: there is no release
 * call; a second claim of the same bus at its rate answers 0 and changes nothing, at
 * another rate RHU_EBUSY. rhu_i2c_transfer runs one combined transaction over a list
 * of message records (the layout below: the address, the flags with bit 0 the read
 * flag, the length, a reserved word that must read 0, the buffer -- twelve bytes,
 * 4-aligned), a repeated start between the messages and a stop after the last: 0 when
 * every byte moved; RHU_ENXIO when the address was not acknowledged; RHU_EIO when a
 * data byte was not acknowledged or the block reported a bus error or arbitration
 * lost; RHU_ETIMEDOUT when the deadline passed while the bus was busy (nothing reset),
 * or when a wire wait passed its budget (two bytes' time plus SMBus's 25 ms clock-low
 * limit) or the deadline, or the block did not stop: then the block is reset, a held
 * data line gets the claim's clocks, then 35 ms of clock low (40 ms more at most);
 * RHU_EINVAL for a flag other than the read flag, a reserved word not 0, a length over
 * RHU_I2C_LEN_MAX, a count of 0 or over RHU_I2C_MSG_MAX, a NULL buffer with a length,
 * an address over 0x7F, a NULL list, a bus not claimed and, where the board's I2C
 * block addresses one device a transaction and has no address-only frame, a list
 * naming two addresses, a message of length 0 or an address the block reserves (under
 * 0x08 or over 0x77); RHU_ENODEV for a bus the board has none of. The bus is shared: a
 * transaction that finds it busy WAITS (bounded by the deadline), never RHU_EBUSY. No
 * transaction runs under the kernel's critical section. A refusal moves no byte. */
#define RHU_ENXIO (-6)      /* the address was not acknowledged            */
#define RHU_I2C_MSG_READ 1
#define RHU_I2C_MSG_MAX 8
#define RHU_I2C_LEN_MAX 255
typedef struct rhu_i2c_msg {
    uint16_t addr;
    uint16_t flags;
    uint16_t len;
    uint16_t reserved;
    uint8_t *buf;
} rhu_i2c_msg;
int32_t rhu_i2c_count(void);
int32_t rhu_i2c_bus(uint32_t index);
int32_t rhu_i2c_claim(uint32_t bus, uint32_t hz);
int32_t rhu_i2c_transfer(uint32_t bus, const rhu_i2c_msg *msgs, uint32_t count, uint64_t deadline_ns);

/* The SPI doors: a program's SPI driver over one of the kernel's buses, shared
 * by any number of devices, each with its own chip select, mode and clock. A
 * bus is named by its NUMBER, the chip's own block number (SPI2 is bus 2), as
 * /dev/spidev2.0 names it; rhu_spi_count is the board's bus count (0 on a row
 * with none: every other door of this family then answers RHU_ENODEV) and
 * rhu_spi_bus the number of the index-th bus (the index from 0 below the
 * count; RHU_EINVAL past it); a number the board has no bus of answers
 * RHU_ENODEV at every door below. rhu_spi_claim takes a bus for the program's
 * whole run: its three pins become the board's (a program holding one refuses
 * the claim RHU_EBUSY, nothing programmed), the block is enabled; 0 when
 * taken, RHU_ENODEV for a bus the board has none of; a claimed bus lives for
 * the boot (no release verb) and a second claim answers 0.
 * rhu_spi_transfer runs one full-duplex exchange with THE DEVICE'S OWN
 * SETTINGS: the mode (RHU_SPI_MODE_0 to RHU_SPI_MODE_3) and the rate in Hz are
 * applied to the block for this transfer, the clock at the fastest rate the
 * block makes at or below the request; the select is one of the bus's
 * chip-select lines by its index from 0, claimed for the board at its first
 * use (RHU_EBUSY when the program holds it as a GPIO line), or
 * RHU_SPI_SELECT_NONE for a program that drives its own chip select through
 * the GPIO doors around the transfer; the select is asserted low before the
 * first clock and released after the last, len bytes are clocked as eight-bit
 * frames, most significant bit first (tx NULL sends 0xFF; rx NULL discards).
 * THE ANSWER IS THE RATE ACHIEVED in Hz, never the request. RHU_ETIMEDOUT when
 * a wait on the wire passed its budget -- twice a byte's time at the achieved
 * rate plus two ticks, never fewer than three, so a stuck wire fails within
 * milliseconds and a transfer of n bytes is bounded by n such waits -- or,
 * where the block shows it, when the clock line did not come to rest within
 * 100 microseconds of the enable or the receiver overran (answered at once),
 * the block reset; RHU_EIO when the block refused its configuration (it fell
 * out of master mode): answered at once, nothing clocked, the fault flag
 * cleared; RHU_EINVAL for a mode over 3, a rate below the block's slowest, a
 * select the bus does not have, a length of 0 or over RHU_SPI_LEN_MAX, tx and
 * rx both NULL, a bus not claimed (RHU_ENODEV for a bus the board has none
 * of). A transfer that finds the bus busy WAITS for it (every holder's
 * exchange is bounded wait by wait, so the wait is), never RHU_EBUSY. No
 * transaction runs under the kernel's critical section. A refusal moves no
 * byte. */
#define RHU_SPI_MODE_0 0
#define RHU_SPI_MODE_1 1
#define RHU_SPI_MODE_2 2
#define RHU_SPI_MODE_3 3
#define RHU_SPI_SELECT_NONE 0xFFFFFFFFu
#define RHU_SPI_LEN_MAX 65535
int32_t rhu_spi_count(void);
int32_t rhu_spi_bus(uint32_t index);
int32_t rhu_spi_claim(uint32_t bus);
int32_t rhu_spi_transfer(uint32_t bus, uint32_t select, uint32_t mode, uint32_t hz, const void *tx, void *rx, uint32_t len);

/* The PWM doors: a program drives a line as a timer's output. A row's
 * PWM-capable lines are a TABLE in the board's facts (every pin with a
 * timer output function, its timer and its channel): rhu_pwm_count answers
 * how many, rhu_pwm_line the index-th line's number (RHU_EINVAL past the
 * count), so a program enumerates without guessing; a row with no such
 * line answers 0 and RHU_ENODEV at every other door. rhu_pwm_claim takes
 * the line in the claims table AND its timer channel (a channel serves one
 * line at a time) for the program's run and answers a handle: RHU_EBUSY
 * when the line or the channel is held (by the board, or by a program
 * through any door family); RHU_EINVAL for a line no timer drives.
 * rhu_pwm_set takes the period and the duty in NANOSECONDS (the Linux PWM
 * class's units) and the polarity (the output high for the duty, or low
 * for it): the timer's clock and its prescaler quantize the period, and
 * rhu_pwm_period answers the period the timer runs, exact from its clock
 * and prescaler (0 before any set). On a timer whose channels share one
 * period -- every timer served -- a period that disagrees with the period
 * another held channel of the same timer already runs is refused
 * RHU_EBUSY; RHU_EINVAL for a duty past the period, a period of 0 or past
 * the timer's range, a polarity past RHU_PWM_POLARITY_INVERSED, a NULL
 * pointer, or a handle the program does not hold. rhu_pwm_enable drives
 * the output (1) or parks it at its inactive level (0): RHU_EINVAL before
 * any set. rhu_pwm_release parks the output and frees the channel and the
 * line. rhu_pwm_info answers the timer's number and the channel's index
 * within that timer, and the timer's input clock in Hz. The device files
 * a hosting layer presents (the Linux PWM class's /sys/class/pwm) number
 * a channel by the LINE NUMBER: pwmchip0/pwm<N> drives GPIO N, and the
 * timer's channel index above is never the channel number on that
 * surface. A refusal programs nothing. */
#define RHU_PWM_POLARITY_NORMAL 0
#define RHU_PWM_POLARITY_INVERSED 1
int32_t rhu_pwm_count(void);
int32_t rhu_pwm_line(uint32_t index);
int32_t rhu_pwm_claim(uint32_t line);
int32_t rhu_pwm_set(int32_t h, uint64_t period_ns, uint64_t duty_ns, uint32_t polarity);
int32_t rhu_pwm_period(int32_t h, uint64_t *achieved_ns);
int32_t rhu_pwm_enable(int32_t h, uint32_t on);
int32_t rhu_pwm_release(int32_t h);
int32_t rhu_pwm_info(int32_t h, uint32_t *timer, uint32_t *channel, uint32_t *clock_hz);

/* The ADC doors: a program reads a line as a voltage. A row's channels are
 * a TABLE in the board's facts (every converter block, every channel, the
 * pin it reads or the internal source it measures, the sample's width in
 * bits): rhu_adc_count answers how many across the blocks, rhu_adc_channel
 * the index-th channel's facts -- the block's number, the channel's
 * number, the line (RHU_ADC_NO_LINE for an internal source) and the width
 * (RHU_EINVAL past the count or for a NULL pointer); a row with no channel
 * answers 0 and RHU_ENODEV at every other door. rhu_adc_claim takes the
 * channel's line (analog mode) and the channel for the program's run and
 * answers a handle: RHU_EBUSY when either is held; RHU_EINVAL past the
 * count. rhu_adc_read runs ONE conversion and answers the raw sample, 0 to
 * 2^bits - 1, polled against a budget from the block's clock and never
 * waiting longer: RHU_ETIMEDOUT past the budget with the block reset;
 * a conversion the block flags as erroneous is discarded (its result is
 * undefined) and another started, at most eight in one read -- RHU_EIO
 * when every one of the eight was flagged, with no sample answered;
 * RHU_EINVAL for a handle the program does not hold. rhu_adc_reference
 * answers a block's reference voltage in millivolts, a fact of the board
 * (RHU_ENODEV for a block the board has none of; RHU_EINVAL for a NULL
 * pointer). rhu_adc_release frees the channel and the line. No conversion
 * runs under the kernel's critical section; a refusal converts nothing. */
#define RHU_ADC_NO_LINE 0xFFFFFFFFu
int32_t rhu_adc_count(void);
int32_t rhu_adc_channel(uint32_t index, uint32_t *block, uint32_t *channel, uint32_t *line, uint32_t *bits);
int32_t rhu_adc_claim(uint32_t index);
int32_t rhu_adc_read(int32_t h);
int32_t rhu_adc_reference(uint32_t block, uint32_t *millivolts);
int32_t rhu_adc_release(int32_t h);

/* The UART doors: a program's serial port on a UART block of the
 * board's facts table -- every UART the board pre-defines, numbered by the
 * chip's own block number (UART0 is port 0) with its two pins where the row
 * has pins. rhu_uart_count answers how many rows, rhu_uart_port the index-th
 * row's facts: the block, the TX and RX lines a claim would take -- the
 * project's pin words' pair when they name a legal pair for the block, else
 * the row's pre-defined pair (RHU_UART_NO_PIN on a row
 * whose UARTs have no pins, the emulator rows, and for a block with no
 * default pair -- the Pico rows' UART1, which has no maker-given pair and
 * is claimable once the project's pin words name a legal pair of the chip)
 * and the UART's input clock in Hz (RHU_EINVAL past the count or for a NULL
 * pointer); a row with no
 * UART row answers 0 and RHU_ENODEV at every other door (the ST-Link boards).
 * A block the kernel's console holds as a stream's
 * wire (the channel words above) is IN the table and REFUSED at the claim,
 * so a program enumerates every UART and learns which one it may open.
 * rhu_uart_claim takes the block for the program's run and answers a handle:
 * RHU_EBUSY when the block is the console's or already claimed (its two
 * lines are taken in the claims table under one critical section, all or
 * nothing, so a GPIO claim of either after it is refused); RHU_EINVAL for a
 * block the row has not, for a project pin override that is not a legal
 * pair of the chip, or for a block with no default pair when the project's
 * words name no legal pair. The claim's state is 115200 8N1, the FIFOs on, the
 * receiver on, no break. rhu_uart_configure sets the line discipline live
 * -- the rate, 5 to 8 data bits, the parity (RHU_UART_PARITY_*), 1 or 2 stop
 * bits -- once the pending transmit has left at the old rate (the door
 * discards nothing): the rate rule is the part's divisor rounded to nearest,
 * and a rate the divisor misses by more than 2 percent, or that needs a
 * divisor the part cannot hold, is refused RHU_EINVAL with nothing moved (on
 * the Picos' 12 MHz peripheral clock the ceiling is 750,000 Bd: 921,600 is
 * refused, 460,800 lands 0.16 percent high and is taken); the achieved rate
 * is answered; the emulator rows take 8N1 alone and refuse every other
 * frame. rhu_uart_write takes the bytes the transmit ring has room for, 0 to
 * n, and primes the transmitter -- it NEVER blocks (a blocking write is a
 * recipe on the event word); rhu_uart_read answers the bytes pending in the
 * receive ring, 0 to n, never waiting -- raw bytes, no echo, no translation.
 * rhu_uart_status answers the bytes pending to read, the bytes pending to
 * transmit (the ring AND the transmitter: 0 means the wire is idle) and the
 * transmit ring's room. rhu_uart_event_word answers the port's futex word
 * (NULL for a handle not held): bumped once per interrupt pass that moved a
 * byte in or made room out, and once more at the release so a waiter on a
 * closed port wakes (the word's storage stays readable); a thread waits on
 * it through rhu_futex_wait with the value it read. rhu_uart_flush discards
 * the receive ring and FIFO (RHU_UART_FLUSH_RX), the transmit ring (the
 * transmitter's FIFO finishes on the wire; RHU_UART_FLUSH_TX) or both.
 * rhu_uart_break holds (1) or releases (0) the break condition (RHU_EINVAL
 * for on > 1, or on a part without break control: the emulator rows).
 * rhu_uart_release disables the block and holds it in reset, drops the
 * rings (pending transmit discarded: drain first), returns the two pads and
 * the two lines, and bumps the event word. rhu_uart_info answers the held
 * port's block, lines and clock. Every refusal moves no byte. The error
 * counters (overruns, framing, parity, breaks, the rings' drops, the bytes
 * in and out, the interrupts) are the witness record __rhu_uart_witness,
 * read by symbol. */
#define RHU_UART_NO_PIN 0xFFFFFFFFu
#define RHU_UART_PARITY_NONE 0
#define RHU_UART_PARITY_ODD 1
#define RHU_UART_PARITY_EVEN 2
#define RHU_UART_PARITY_MARK 3
#define RHU_UART_PARITY_SPACE 4
#define RHU_UART_FLUSH_RX 1
#define RHU_UART_FLUSH_TX 2
#define RHU_UART_FLUSH_BOTH 3
int32_t rhu_uart_count(void);
int32_t rhu_uart_port(uint32_t index, uint32_t *block, uint32_t *tx_line, uint32_t *rx_line, uint32_t *clock_hz);
int32_t rhu_uart_claim(uint32_t block);
int32_t rhu_uart_configure(int32_t h, uint32_t baud, uint32_t data_bits, uint32_t parity, uint32_t stop_bits, uint32_t *achieved_baud);
int32_t rhu_uart_write(int32_t h, const void *buf, uint32_t n);
int32_t rhu_uart_read(int32_t h, void *buf, uint32_t n);
int32_t rhu_uart_status(int32_t h, uint32_t *rx_avail, uint32_t *tx_pending, uint32_t *tx_room);
const uint32_t *rhu_uart_event_word(int32_t h);
int32_t rhu_uart_flush(int32_t h, uint32_t which);
int32_t rhu_uart_break(int32_t h, uint32_t on);
int32_t rhu_uart_release(int32_t h);
int32_t rhu_uart_info(int32_t h, uint32_t *block, uint32_t *tx_line, uint32_t *rx_line, uint32_t *clock_hz);

/* --- The network doors: a program's sockets over the kernel's own
 * TCP/IP stack on the medium the board has (the STM32F769's wired controller,
 * the Pico W rows' wireless module -- each with its driver -- and a
 * VIRTUAL medium in kernel memory on the emulator rows, where the whole
 * family needs no network). Handles are small
 * non-negative integers from a per-row pool; every door answers 0 or a
 * positive count on success and a negative errno-mirroring code otherwise.
 * NO DOOR BLOCKS: every wait is a hosting layer's recipe over
 * rhu_futex_wait on a word the kernel bumps -- a socket's event word
 * (moved when the socket's readiness mask changes, and at its close), the
 * one global word for all sockets (moved after any socket's mask changed,
 * and by rhu_net_events_bump), the medium's word (the link and the lease),
 * a resolution's word (its answer). The blocking recipe: call the door;
 * on RHU_EAGAIN snapshot the word, call the door again, then
 * rhu_futex_wait(word, snapshot, deadline) and loop. A row with no medium
 * answers 0 from rhu_net_count and RHU_ENODEV from every other door
 * (rhu_net_events_word and rhu_net_events_bump stand on every row).
 *
 * The codes joining the surface for this family: */
#define RHU_EMFILE (-24)          /* the socket pool is full                             */
#define RHU_EPIPE (-32)           /* send after the stream closed                        */
#define RHU_EMSGSIZE (-90)        /* a datagram larger than the socket's buffer          */
#define RHU_EPROTONOSUPPORT (-93) /* a socket type or protocol the row does not serve    */
#define RHU_EAFNOSUPPORT (-97)    /* an address family the row does not serve (IPv6)     */
#define RHU_EADDRINUSE (-98)      /* the local port is bound already                     */
#define RHU_ENETDOWN (-100)       /* the medium's link is down                           */
#define RHU_ENETUNREACH (-101)    /* no route: no address yet, or off the segment with no gateway, or no name server */
#define RHU_ECONNABORTED (-103)   /* the connection was aborted                          */
#define RHU_ECONNRESET (-104)     /* the peer reset the connection                       */
#define RHU_EISCONN (-106)        /* connect on a connected stream                       */
#define RHU_ENOTCONN (-107)       /* send or receive on a stream that is not connected   */
#define RHU_ECONNREFUSED (-111)   /* the peer refused the connection with a reset        */
#define RHU_EHOSTUNREACH (-113)   /* the connect's budget passed with no answer from the peer (its hardware address, or its reply) */
#define RHU_EALREADY (-114)       /* connect on a stream already connecting              */
#define RHU_EINPROGRESS (-115)    /* the handshake started; wait on the event word       */
/* (RHU_EAGAIN, RHU_EINVAL, RHU_ETIMEDOUT, RHU_ENODEV, RHU_ENOTSUP as they stand.) */

/* The records. rhu_net_addr is sized for both families: family 2 is
 * IPv4 (the Linux value) with the four octets in the first bytes of addr
 * in the wire's order and the port in the host's order; family 0 is no
 * address (an unconnected peer half); IPv6 (family 10) is refused at
 * every door. rhu_net_lease_desc carries each address as
 * its four octets in the wire's order read as one word (the first octet
 * in the word's lowest byte, as sin_addr.s_addr holds it). */
typedef struct rhu_net_addr   { uint32_t family; uint32_t port; uint8_t addr[16]; uint32_t scope; } rhu_net_addr;
typedef struct rhu_net_medium { uint32_t kind; uint32_t link; uint8_t hw[6]; uint16_t reserved; } rhu_net_medium_desc; /* kind: RHU_NET_MEDIUM_*; link 0 down / 1 up */
typedef struct rhu_net_lease  { uint32_t ip4, mask, gateway, dns[2]; uint32_t source; } rhu_net_lease_desc;        /* source: RHU_NET_LEASE_* */
typedef struct rhu_net_limits { uint32_t streams, datagrams, stream_rx, stream_tx, datagram_buf, resolve_slots; } rhu_net_limits_desc;
#define RHU_NET_FAMILY_IPV4 2u
#define RHU_NET_FAMILY_IPV6 10u
#define RHU_NET_STREAM 1u
#define RHU_NET_DATAGRAM 2u
#define RHU_NET_MEDIUM_NONE 0u
#define RHU_NET_MEDIUM_WIRELESS 1u
#define RHU_NET_MEDIUM_WIRED 2u
#define RHU_NET_MEDIUM_VIRTUAL 3u
#define RHU_NET_LEASE_NONE 0u
#define RHU_NET_LEASE_DHCP 1u
#define RHU_NET_LEASE_STATIC 2u
#define RHU_NET_READABLE 1
#define RHU_NET_WRITABLE 2
#define RHU_NET_ERROR 4
#define RHU_NET_HANGUP 8
#define RHU_NET_PEEK 1u
#define RHU_NET_SHUT_RD 0u
#define RHU_NET_SHUT_WR 1u
#define RHU_NET_SHUT_RDWR 2u
#define RHU_NET_OPT_NODELAY 1u
#define RHU_NET_OPT_KEEPALIVE 2u
#define RHU_NET_OPT_REUSEADDR 3u

/* The media and the address. rhu_net_count answers how many media the row
 * has (0 or 1). rhu_net_medium answers a medium's
 * kind, link state and hardware address (RHU_EINVAL past the count or for
 * a NULL or misaligned pointer); rhu_net_medium_word its futex word, moved
 * at every link change and lease change (NULL past the count).
 * rhu_net_configure with NULL arms the DHCP client (the default: the
 * lease arrives on the network thread and moves the medium's word; a
 * program never waits for a lease before it starts); with a record it
 * sets a static address, mask, gateway (0 = none) and name servers
 * (0 = none) and stops the client (RHU_EINVAL for an address of 0 or a
 * mask that is not a prefix). rhu_net_lease answers the current record
 * with its source; RHU_EAGAIN while none. rhu_net_limits answers the row's
 * pool as numbers: the stream and datagram socket counts, the stream
 * receive and transmit buffer bytes, the datagram buffer bytes, the
 * resolver's slots (RHU_EINVAL for a bad pointer). */
int32_t rhu_net_count(void);
int32_t rhu_net_medium(uint32_t medium, rhu_net_medium_desc *out);
const uint32_t *rhu_net_medium_word(uint32_t medium);
int32_t rhu_net_configure(uint32_t medium, const rhu_net_lease_desc *fixed);
int32_t rhu_net_lease(uint32_t medium, rhu_net_lease_desc *out);
int32_t rhu_net_limits(rhu_net_limits_desc *out);

/* Name resolution: a request with a handle, never a wait. rhu_net_resolve
 * files a query for a name of len bytes (letters, digits, hyphens and
 * dots; RHU_EINVAL otherwise or past 253 bytes) through the stack's
 * resolver into one of the row's slots and answers the slot's handle at
 * once: RHU_EAGAIN when no slot is free, RHU_ENETUNREACH when the row
 * has no name server (no lease yet, or a static record naming none).
 * rhu_net_resolve_result answers the address records (family 2, the port
 * 0) written into out, at most max: the count written; RHU_EAGAIN while
 * the answer is pending; RHU_EHOSTUNREACH for a name the server does not
 * have; RHU_ETIMEDOUT when the stack's retry budget passed with no
 * answer; RHU_EINVAL for a handle not in use. THE SLOT's LIFETIME: a slot
 * is taken by rhu_net_resolve and released by the first
 * rhu_net_resolve_result that answers anything but RHU_EAGAIN -- a
 * program that files a query and never reads its result holds the slot.
 * rhu_net_resolve_word answers the request's futex word (moved once when
 * the answer or the failure arrives; NULL for a handle not in use). */
int32_t rhu_net_resolve(const char *name, uint32_t len);
int32_t rhu_net_resolve_result(int32_t handle, rhu_net_addr *out, uint32_t max);
const uint32_t *rhu_net_resolve_word(int32_t handle);

/* Sockets. rhu_net_socket answers a handle for a stream (RHU_NET_STREAM,
 * TCP; protocol 0 or 6) or a datagram socket (RHU_NET_DATAGRAM, UDP;
 * protocol 0 or 17) of family RHU_NET_FAMILY_IPV4, its receive and
 * transmit buffers carved from the row's pool here: RHU_EMFILE when the
 * pool of handles or of sockets of that kind is spent, RHU_EAFNOSUPPORT
 * for another family (IPv6 included, which turns a hosted runtime's
 * dual-mode default off), RHU_EPROTONOSUPPORT for another type or
 * protocol. rhu_net_bind records a stream's local port or binds a
 * datagram socket (the address 0.0.0.0 or the row's own; the port not 0):
 * RHU_EADDRINUSE when another socket of the kind holds the port (unless
 * both set reuse-address), RHU_EINVAL for a bound, connecting or connected
 * socket or a foreign address. rhu_net_listen puts a bound stream to
 * listening with up to backlog connections pre-listening behind the one
 * handle, bounded by the pool (fewer sockets than asked is not a
 * refusal): RHU_EINVAL for an unbound stream, a datagram socket or a
 * backlog of 0. rhu_net_accept moves the first completed connection
 * behind a listener to a new handle, writes the peer's address when peer
 * is not NULL, and arms a fresh socket in its place where the pool has
 * one: the new handle; RHU_EAGAIN when none completed; RHU_EMFILE when no
 * handle is free. rhu_net_connect on a stream starts the handshake and
 * answers RHU_EINPROGRESS on every medium (the loopback included; the
 * readiness word reports its end), with RHU_EALREADY while one is in
 * flight, RHU_EISCONN after one ended, RHU_ENETDOWN with the link down and
 * RHU_ENETUNREACH with no address, a port of 0 or a destination off the
 * segment with no gateway; on a datagram socket it sets the default peer
 * (binding an unbound socket to a port of the row's choosing) and answers
 * 0. rhu_net_send takes the bytes a stream's transmit buffer has room for
 * (0 to n, never waiting; a room of 0 answers RHU_EAGAIN) or a datagram
 * whole (to the record, or the default peer with to NULL; RHU_EMSGSIZE
 * past the buffer, RHU_ENOTCONN with neither address, RHU_ENETUNREACH off
 * the segment): a PENDING ERROR is answered first and consumed; a stream
 * that is not connected answers RHU_ENOTCONN (RHU_EAGAIN while
 * connecting), one whose peer or own side closed RHU_EPIPE. rhu_net_recv
 * answers the bytes read, 0 to n, from a stream (0 = the peer closed) or
 * one datagram whole (RHU_EMSGSIZE when the buffer is smaller than the
 * datagram, which is discarded), with the peer's address into from when
 * it is not NULL; flags RHU_NET_PEEK reads without consuming (the health
 * peek a hosted runtime's HTTP handler takes); RHU_EAGAIN while nothing
 * is queued; a pending error first; RHU_EINVAL for another flag.
 * rhu_net_available answers a stream's queued receive bytes or a datagram
 * socket's next datagram length, 0 when none. rhu_net_shutdown with
 * RHU_NET_SHUT_RD makes later reads answer 0 (a slot flag: the stack has
 * no read half-close), with RHU_NET_SHUT_WR sends the stream's close
 * (FIN), RHU_NET_SHUT_RDWR both; 0 on any valid handle, connected or not.
 * rhu_net_close releases the handle at once and wakes its waiters; a
 * stream's close handshake and its buffer pair stay on the network thread
 * until the stack removes the connection (the pool counts the pair until
 * then); a listener's sockets are aborted. rhu_net_readiness answers the
 * poll-shaped mask -- RHU_NET_READABLE, RHU_NET_WRITABLE, RHU_NET_ERROR,
 * RHU_NET_HANGUP -- as the LEVEL (take 0) or as THE RISEN BITS since the
 * last take (take 1; cleared), the edge an event port reports: a
 * connecting stream reports no bit until the handshake ends; at
 * establishment WRITABLE rises; a failed connect raises WRITABLE, ERROR
 * and HANGUP with the code pending (RHU_ECONNREFUSED on the peer's reset,
 * RHU_EHOSTUNREACH at the budget); the peer's close raises READABLE and
 * HANGUP and a read answers 0; a reset raises ERROR with RHU_ECONNRESET
 * pending; a listener with a completed connection reports READABLE; a
 * datagram socket reports READABLE with a datagram queued and WRITABLE
 * when bound with room. rhu_net_error answers the pending code, read once
 * and cleared, 0 when none. rhu_net_event_word answers the socket's futex
 * word (NULL for a handle not in use); rhu_net_events_word the one word
 * for all sockets; rhu_net_events_bump moves that word and wakes its
 * waiters (a hosting layer's self-wake: an event port's registration
 * changed under a waiter, or the port closed) -- the one bump that is not
 * a mask change. rhu_net_name writes the local half always (a bound or
 * connected socket's address and port, the port of the row's choosing
 * included; 0.0.0.0 and 0 before any) and the peer half when peer is not
 * NULL (family 0 when unconnected). rhu_net_option with set 1 writes an
 * option's value and answers 0; with set 0 answers the value: no-delay
 * (1), keep-alive (2; its interval a row fact) and reuse-address (3);
 * RHU_ENOTSUP for any other option, and for the stream options on a
 * datagram socket (the timeouts and linger are a hosting layer's own
 * records). Every refusal moves no byte; the counters are the witness
 * record __rhu_net_witness, read by symbol. */
int32_t rhu_net_socket(uint32_t family, uint32_t type, uint32_t protocol);
int32_t rhu_net_bind(int32_t h, const rhu_net_addr *local);
int32_t rhu_net_listen(int32_t h, uint32_t backlog);
int32_t rhu_net_accept(int32_t h, rhu_net_addr *peer);
int32_t rhu_net_connect(int32_t h, const rhu_net_addr *peer);
int32_t rhu_net_send(int32_t h, const void *buf, uint32_t n, const rhu_net_addr *to);
int32_t rhu_net_recv(int32_t h, void *buf, uint32_t n, uint32_t flags, rhu_net_addr *from);
int32_t rhu_net_available(int32_t h);
int32_t rhu_net_shutdown(int32_t h, uint32_t how);
int32_t rhu_net_close(int32_t h);
int32_t rhu_net_readiness(int32_t h, uint32_t take);
int32_t rhu_net_error(int32_t h);
const uint32_t *rhu_net_event_word(int32_t h);
const uint32_t *rhu_net_events_word(void);
void    rhu_net_events_bump(void);
int32_t rhu_net_name(int32_t h, rhu_net_addr *local, rhu_net_addr *peer);
int32_t rhu_net_option(int32_t h, uint32_t option, uint32_t set, int32_t value);

/* The RAM-range doors: the board's RAM as this image offers it to the
 * hosted runtime, so a heap can span more than the chip's own SRAM. A
 * row's ranges are a TABLE in the board's facts (the SDK's per-board file
 * and the kernel's mirror of it, kept equal by the verification chain):
 * range 0 is ALWAYS the internal SRAM -- the arena every door above draws
 * from, the behaviour unchanged -- and range 1 an external RAM where a
 * board has one and the image brought it up and proved it by read-back at
 * boot (the Pimoroni Pico Plus 2 boards' 8 MB PSRAM behind the flash
 * interface's second chip select; a boot whose part refuses dies loud
 * before any door answers). rhu_ram_count answers how many ranges THIS
 * IMAGE serves (at least 1; the emulator rows and the boards without an
 * external part answer 1; a clock posture at which the part's limits
 * cannot be met builds without the window and answers 1 too).
 * rhu_ram_range fills the index-th range's record: `base` and `bytes` the
 * physical range as the facts state it; `lo` and `hi` the arena window
 * inside it the runtime may be handed (the first byte and one past the
 * last); `free` that window's headroom at the call: the bytes an
 * allocation at any alignment up to 8 could still take from it (hi minus
 * the next hand-out address; the available memory a hosting layer
 * answers is the sum over the ranges; for range 0 the same number
 * rhu_arena_headroom answers); `flags` the bits below. RHU_EINVAL
 * past the count or for a NULL or misaligned record, writing nothing.
 * rhu_arena_alloc_from is rhu_arena_alloc_at drawn from the named range,
 * under the arena's laws: NULL on THAT range's honest exhaustion only;
 * a wild argument -- the align, the size, or a range past the count --
 * is a caller BUG and dies loud through the funnel; any power-of-two
 * align up to the range's window is honoured (a collector's region
 * alignment among them). Range 0 behaves exactly as
 * rhu_arena_alloc_at. The zero-birth invariant holds per
 * range (zeroed at boot or at the hand-out by the image's init mode;
 * the door returns after), and rhu_arena_rezero accepts a pointer
 * inside any range's handed-out space.
 *
 * The flags: EXTERNAL, behind a memory controller rather than the chip's
 * own SRAM; CACHED, reached through a cache the kernel manages (the
 * RP2350's flash-interface cache for its PSRAM); EXEC, the memory
 * protection lets code run from it; DMA, a bus master reaches it
 * coherently (clear on every range: no door drives a bus master). */
#define RHU_RAM_EXTERNAL 1u
#define RHU_RAM_CACHED 2u
#define RHU_RAM_EXEC 4u
#define RHU_RAM_DMA 8u
typedef struct rhu_ram_desc {
    uint32_t base, bytes;       /* the physical range, as the facts state it */
    uint32_t lo, hi;            /* the arena window inside it, half-open      */
    uint32_t free;              /* that window's headroom at the call         */
    uint32_t flags;             /* RHU_RAM_* bits                             */
} rhu_ram_desc;
int32_t rhu_ram_count(void);
int32_t rhu_ram_range(uint32_t index, rhu_ram_desc *out);
void *rhu_arena_alloc_from(uint32_t range, uint32_t bytes, uint32_t align, uint32_t site);

/* The block-device doors: a board's block devices as this
 * image serves them, enumerated by index -- the one device is the
 * STM32F769 Discovery board's microSD socket on the chip's SDMMC2
 * controller, named "mmcblk0" as Linux names the first MMC/SD block
 * device; every other row serves none (the count 0, RHU_ENODEV at every
 * other door). rhu_blk_count answers how many devices THIS IMAGE serves.
 * rhu_blk_device fills the index-th device's record, read AT THE CALL (the
 * detect line and the card's identification: a program may poll it for an
 * insertion): block_bytes 512; blocks the card's capacity in blocks, 0 with
 * no card seated or a card refused; flags the bits below -- REMOVABLE on a
 * socket, PRESENT with a card seated (identified or not), WRITE_PROTECTED
 * from the card's own register, HIGH_CAPACITY when the card itself
 * addresses by block (every card is addressed by block NUMBER at these
 * doors either way); name the device's name, NUL-terminated. RHU_EINVAL
 * past the count or for a NULL or misaligned record, writing nothing.
 * rhu_blk_read and rhu_blk_write move whole blocks by number -- `blocks`
 * blocks from block `lba` -- and answer the count moved, or a code with
 * nothing moved: RHU_ENODEV for a device past the count or no card seated;
 * RHU_EINVAL for a NULL or misaligned buffer (4-byte alignment), a count of
 * 0 or over RHU_BLK_MAX, a range past the card's end; RHU_EROFS for a write
 * on a write-protected card; RHU_EIO for a card that refused its
 * identification or a transfer (after the kernel's one retry of a transfer
 * refused by its CRC); RHU_ETIMEDOUT for a transfer past its budget. The
 * buffer is the caller's, in RAM the controller's engine reaches (every RAM
 * range the ram doors state on this row). A card is identified at the first
 * door call after its insertion (that call carries the identification's
 * cost); a card removed drops the identification and the next door call
 * reads the socket anew (the hot-swap policy). A
 * door that finds the device busy with another thread's transfer waits for
 * it, never RHU_EBUSY. No door runs under the kernel's critical section.
 * rhu_blk_sync waits for the card's programming of the last write to end
 * (every write at these doors is synchronous already; the door is the seam
 * for an engine that queues) and answers 0, or RHU_ENODEV.
 * RHU_EROFS is this family's code; RHU_ENOENT is declared here for the
 * filesystem doors that follow (Linux's values, both). */
#define RHU_BLK_MAX 128u                 /* blocks per call: 64 KB            */
#define RHU_BLK_REMOVABLE 1u
#define RHU_BLK_PRESENT 2u
#define RHU_BLK_WRITE_PROTECTED 4u
#define RHU_BLK_HIGH_CAPACITY 8u
#define RHU_EROFS (-30)     /* a write on a write-protected card         */
#define RHU_ENOENT (-2)     /* no such path (the filesystem doors')      */
typedef struct rhu_blk_desc {
    uint32_t block_bytes;
    uint32_t blocks;
    uint32_t flags;
    char name[16];
} rhu_blk_desc;
int32_t rhu_blk_count(void);
int32_t rhu_blk_device(uint32_t index, rhu_blk_desc *out);
int32_t rhu_blk_read(uint32_t dev, uint32_t lba, uint32_t blocks, void *buf);
int32_t rhu_blk_write(uint32_t dev, uint32_t lba, uint32_t blocks, const void *buf);
int32_t rhu_blk_sync(uint32_t dev);

/* --- The payload entry seam: the one member of this surface the
 * kernel CALLS rather than serves. After the platform is up (the
 * initialized data copied, the arena, the kernel heap, module 0, the
 * scheduler and the kmain thread, the console) the payload flavor
 * calls exactly one function, on the kmain thread, and exits the
 * machine with the code it returns. For the entry's life that thread
 * runs at RHU_HOSTED_PRIO_FLAT -- inside the hosted band, level with
 * every thread the payload starts at that value -- and it takes the
 * kernel's own urgency back when the entry returns; one console line
 * ("[payload] host thread: ...") states both values at every boot. */

/* --- The fault funnel's delivery (the null-page guard). The kernel
 * programs the core's memory protection unit so the first 4 KB of the
 * address space is no-access on every row: a data read or write through
 * a null reference (any offset below 0x1000, the window the compiler
 * leaves unchecked) raises a MemManage fault instead of returning a
 * word. Until a hosting layer arms the delivery, that fault is a designed
 * death naming the access. A hosting layer that owns the runtime's
 * signal handler arms it here, once, and the kernel then delivers a null
 * DATA access taken from a program's thread through the activation
 * costume: the thread resumes on its own stack inside the registered
 * activation handler with a pointer to an rhu_fault_ctx -- the rhu_ctx of
 * the faulting instruction (pc = the instruction, sp = the fault site's)
 * with flags bit 8 (RHU_CTX_FAULT) set and the kind in bits 15:12
 * (RHU_FAULT_KIND(flags) == RHU_FAULT_NULL), followed by the faulting
 * address and the fault status word (the MemManage status bits). The two
 * trailing words exist only in a delivered fault context: rhu_ctx stays
 * 72 bytes and rhu_thread_suspend's out_ctx does not change. The handler
 * edits r0-r12, lr and pc under the activation rules (sp is never
 * written back: an upward stack-pointer edit is the hosting layer's to
 * honour on the thread's own stack) and returns; the thread resumes at
 * the edited pc. A handler that returns with the pc UNCHANGED has
 * declined the fault: the access would only fault again, so the kernel
 * ends the program through its terminal path with the null-access line
 * -- never a loop. Never delivered, each a designed death with one line:
 * an instruction fetch below the page (no managed frame to throw from), a
 * null access from idle, from handler mode, under the kernel's interrupt
 * mask (a forced HardFault: the kernel's own bug), inside the activation
 * handler itself, or from a thread with too little stack above its floor
 * for the delivery. An activation send that lands while the fault handler
 * runs is re-delivered after it, on the edited context, as a plain
 * activation. The cost on the faulting thread's stack: 80 (the fault
 * context) + 68 (the frame pair) = 148 bytes, plus the handler's own
 * need; the handler runs on the faulting thread's stack, as the runtime's
 * signal handler does on Linux without an alternate stack. Refusals:
 * RHU_EINVAL for a bit outside the kinds below. Sticky for the process's
 * life; arming twice is fine. */
#define RHU_FAULT_NULL 1u                      /* kinds bit 0: a data access below 0x1000 */
#define RHU_CTX_FAULT (1u << 8)                /* rhu_ctx.flags: the context is an rhu_fault_ctx */
#define RHU_FAULT_KIND(flags) (((flags) >> 12) & 0xFu)
typedef struct rhu_fault_ctx {
    rhu_ctx ctx;
    uint32_t fault_addr;    /* the faulting data address */
    uint32_t fault_status;  /* the MemManage status bits (CFSR[7:0]) */
} rhu_fault_ctx;
int32_t rhu_fault_register(uint32_t kinds);

/* The entry. The kernel provides a WEAK default that runs, in order:
 * the placement check on the constructor and destructor arrays (the
 * payload maps place .preinit_array, .init_array and .fini_array
 * inside the boot copy window under the standard bracket names
 * __preinit_array_start/end, __init_array_start/end and
 * __fini_array_start/end; an array outside the window refuses loudly),
 * the .preinit_array walk, the .init_array walk in priority order, the
 * managed module list, then managed Main, whose code it returns.
 * Nothing in the kernel runs the destructor array: the machine exit
 * runs no destructors. A payload that defines this name strongly
 * displaces the default and owns the whole order -- constructors,
 * runtime, module list, arguments, the managed program -- and the
 * default never runs. */
int32_t rhu_payload_entry(void);

/* The generation handshake. A payload that provides its own
 * rhu_payload_entry also defines this object, strongly, as the RHU_ABI
 * of the header it compiled against:
 *     const uint32_t rhu_payload_abi = RHU_ABI;
 * The kernel reads it before calling the entry. A stated generation
 * inside the kernel's window (RHU_ABI_COMPAT through RHU_ABI) runs; one
 * outside it, or a provided entry with no stated generation, is refused
 * with one sentence on the console and exit code 4 (a park on silicon):
 * the entry never runs, and a mismatched peer reports instead of
 * hanging. A payload that keeps the default entry may leave this
 * undefined (the kernel's weak sentinel reads 0, "no generation
 * stated"). */
extern const uint32_t rhu_payload_abi;

#ifdef __cplusplus
}
#endif

#endif /* RHU_H */
