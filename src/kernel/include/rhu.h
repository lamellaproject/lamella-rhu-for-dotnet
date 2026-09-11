/* The Lamella RHU hosting API -- the C ABI for hosted payloads.
 *
 * Mirrors src/api.rs exactly; the two files change together or not at
 * all. Minimal on purpose: a futex pair plus a nanosecond clock and
 * sleep is the entire blocking surface of the kernel, and every richer
 * primitive (mutex, event, semaphore, condition variable) is a recipe
 * over these four in the layer above. No signals exist or ever will.
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
 * RHU_ABI is the generation. An ADDITIVE change -- a function added at
 * the end of this header, nothing a compiled consumer could feel moved
 * -- raises RHU_ABI by one and leaves RHU_ABI_COMPAT where it stands. A
 * BREAKING change -- a signature, a struct layout, an error value, a
 * function removed or reordered -- raises both to the same new number.
 * (Through generation 31 the value was the header's function count; it
 * is a generation number from 32 on.)
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
#define RHU_ABI 32
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
 * of hand-out (the zero-birth invariant). All verbs run under the
 * kernel critical section: single-core, short, never blocking. */

/* Aligned bump allocation attributed to `site` (an opaque caller key;
 * C callers use the rhu_arena_alloc macro and get their return address
 * for free). Returns NULL on honest EXHAUSTION only. A wild argument
 * (align not a power of two, align > 4096, or bytes == 0) is a caller
 * BUG and dies loud through the funnel -- never a quiet NULL. Sizes
 * round up to 8; the returned block satisfies `align`. */
void *rhu_arena_alloc_at(uint32_t bytes, uint32_t align, uint32_t site);
#define rhu_arena_alloc(b, a) \
    rhu_arena_alloc_at((b), (a), (uint32_t)__builtin_return_address(0))

/* Re-establish the zero-birth invariant over [p, p+bytes): after a 0
 * return the range reads zero, the address stays the caller's, and no
 * new address space was consumed (headroom and the next hand-out
 * address are untouched). The range must lie inside handed-out arena
 * space; anything else returns RHU_EINVAL and touches nothing. */
int32_t rhu_arena_rezero(void *p, uint32_t bytes);

/* Bytes an immediately following 8-aligned allocation could still get,
 * exactly. 0 means exhausted. */
uint32_t rhu_arena_headroom(void);

/* Dump the per-site allocation audit table to the console. Diagnostic:
 * noisy by design, never called by ship flavors or gated images. */
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
 * is no unregister: modules never unload. */
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
 * counter does not wrap). */
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
 * never live in FP registers). */
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
 * brackets; every board map now defines the bracket symbols:
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
 * bytes: no echo, no line editing, no CR/LF translation. Refusals
 * mirror rhu_console_write. */
int32_t rhu_console_read(void *buf, uint32_t n);

/* Terminate the machine with an exit code. On a hosted-emulator
 * world the code reaches the host's exit status; on silicon the
 * machine parks in low-power idle (the terminal state -- there is
 * nothing to return the code to). Never returns; runs NO destructors
 * or atexit machinery of its own (the layer above owns shutdown
 * ceremony). */
void rhu_exit(int32_t code);

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
int32_t rhu_entropy_is_hardware(void);

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
