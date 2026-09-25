/* mgc_host.cpp -- the GC gate's activation host (gate case gc.A).
 *
 * Plays the execution engine's INITIALIZATION role against the mgc
 * interface skeleton on a bare QEMU image: drives the standalone-GC
 * loader protocol (the dlopen/dlsym costume seam, the GC_VersionInfo
 * handshake, GC_Initialize, the first init-time vtable calls) and
 * provides the minimal EE surface the bare skeleton links against --
 * the five flat GCToEEInterface methods its initialization path calls,
 * plus the small console C runtime its logging uses.
 *
 * Under -DMGC_HOST_DRIVE_ALLOC (gate case gc.B) the same host also
 * plays the EE's allocation-era role for a bounded drive: it serves
 * the allocator's import surface -- the glibc-shaped anonymous mmap
 * served from the kernel arena, the write-barrier stomp sink, and
 * mutator thread/context getters at the EE's thread layout -- and
 * drives a deterministic allocation schedule through IGCHeap::Alloc
 * after activation succeeds. Compiled without the knob this file is
 * byte-for-byte the activation-only host, so the activation case's
 * objects do not move when the drive world changes.
 *
 * Under -DMGC_HOST_DRIVE_COLLECT (gate case gc.C; extends the drive
 * world above) the host additionally plays the EE's COLLECTION-era
 * role: the twelve flat methods the collector's mark calls around a
 * stop-the-world collection (SuspendEE ... RestartEE in the EE's
 * bracket order), with the stop-the-world mapped onto the KERNEL's
 * suspension verbs -- one spawned worker thread counts on its own
 * stack and SuspendEE really suspends it (context and stack bounds
 * asserted, counter verified frozen), RestartEE really resumes it
 * (counter verified advancing). The mark's root sources are all
 * host-built and deterministic: static slots, a slot on the worker's
 * stack, the skeleton's real handle store, a registered frozen
 * segment carrying a frozen->heap edge, and one heap object born
 * with a host-built ContainsGCPointers MethodTable whose GCDesc
 * series the tracer walks for real. The safe-point protocol of the
 * real EE (cooperative-mode parking) is policy above these verbs and
 * is NOT modeled here: the worker holds no heap invariants -- it
 * never allocates -- so suspend-anywhere is safe by construction; the
 * policy layer belongs to the real EE.
 *
 * Under -DMGC_HOST_DRIVE_SWEEP (gate case gc.C-sweep; extends the
 * collection world) the host plays the EE's DEATH-PIPELINE-era role:
 * the four flat methods the pipeline calls (EagerFinalized -- true for
 * the host's WR-mirror MethodTable, destroying the mirrored
 * "underlying" handle through the real manager exactly as the
 * runtime's WeakReference eager finalizer does; the two no-op parity
 * callbacks; EnableFinalization), a death cast whose every object
 * exercises one partition branch or one weak-nulling class, the
 * finalizer thread's drain role (GetNextFinalizable until null,
 * FINALIZER_RUN honored EE-side: clear + skip), and the allocator's
 * reclaim story end-to-end -- free-list reuse with zero-on-reuse, the
 * collect-and-retry wall that replaces a mark-only build's loud
 * region fail-fast, and the honest-NULL OOM protocol with the
 * headroom-reserve grace round trip. Weak-death observations are
 * PRINTS, never asserts: the same host must run the collector's
 * MGC_BREAK_WEAK control to exit 0, and the gate is the detector.
 *
 * Under -DMGC_HOST_DRIVE_TUNE (gate case gc.D; extends the sweep
 * world) the host plays the EE's WINDOW-era role: the fast path
 * (serving small allocations from the context's bump window exactly
 * as the EE's inline allocator would, so window consumption and the
 * fix-time object walk are real), the third fix site (a detach-probe
 * context retired through FixAllocContext), the GC-done event wiring
 * (Reset/SetWaitForGCEvent vcalled inside SuspendEE/RestartEE -- the
 * EE's order), and a second spawned thread: the WAITER, parked
 * between exercises on a host futex word, which sleeps on the
 * skeleton's futex-backed GC-done event and is woken once by a real
 * collection's RestartEE and once by an ACTIVATION delivered while
 * genuinely futex-blocked -- the kernel's RHU_EINTR contract, proven
 * by elimination on the wire (no wake, no EAGAIN window, no
 * deadline; the handler-ran line is the contract's "after the
 * handler has run" clause).
 *
 * This file is gate-world machinery, NOT the product shim: the real
 * execution engine arrives with the runtime archives, and when those
 * are linked this file must LEAVE the image (its EE statics and its
 * C++ operator delete would collide with the archives' own; the
 * archives' C++ runtime shim defines operator delete, and the real EE
 * defines the flat interface). It hosts exactly one world: the
 * skeleton-tier activation image.
 *
 * Exit taxonomy of this host (travels through the kernel's
 * "managed Main returned N" line and the semihosting exit):
 *   0   activation succeeded (every positive assertion held)
 *   3   version handshake REFUSED (the refusal class: a loud,
 *       attributed no -- the gate's doctored-version control expects
 *       exactly this)
 *   134 abort(): a structurally broken path (never expected; the
 *       value mirrors the conventional SIGABRT shell code so it can
 *       never be confused with the kernel's 0-3 verdict codes)
 *
 * Console output goes through the kernel's sh_write0 export
 * (unprefixed payload-stream lines, CRLF wire discipline at the
 * console layer). The printf here implements an ENUMERATED format
 * subset; an unsupported conversion is a loud death, never a silent
 * wrong answer, and the driver prints a format self-test line the
 * gate pins byte-exact -- the formatter is verified in-image on every
 * run.
 */

#include "mgc_env.h"
#include "gcinterface.h"
#include "gcenv.ee.h"
#include "gcenv.object.h"

#include <stdarg.h>

#ifdef MGC_HOST_DRIVE_ALLOC
#include "rhu.h" /* the kernel arena + headroom verbs behind the mmap bridge */
#endif

#ifdef MGC_HOST_DRIVE_COLLECT
#ifndef MGC_HOST_DRIVE_ALLOC
#error "MGC_HOST_DRIVE_COLLECT extends the allocator-drive world (pass MGC_HOST_DRIVE_ALLOC too)"
#endif
#include "gcdesc.h" /* the pair MethodTable's series layout (mirror-built) */
#endif

#ifdef MGC_HOST_DRIVE_SWEEP
#ifndef MGC_HOST_DRIVE_COLLECT
#error "MGC_HOST_DRIVE_SWEEP extends the collection-drive world (pass MGC_HOST_DRIVE_COLLECT too)"
#endif
#endif

#ifdef MGC_HOST_DRIVE_FREEPAD
#ifndef MGC_HOST_DRIVE_SWEEP
#error "MGC_HOST_DRIVE_FREEPAD is a sweep-world drive (needs MGC_HOST_DRIVE_SWEEP)"
#endif
#endif
#ifdef MGC_HOST_DRIVE_NOGC
#ifndef MGC_HOST_DRIVE_TUNE
#error "MGC_HOST_DRIVE_NOGC is a windows-world drive (needs MGC_HOST_DRIVE_TUNE)"
#endif
#endif
#ifdef MGC_HOST_DRIVE_HANDLES
#ifndef MGC_HOST_DRIVE_TUNE
#error "MGC_HOST_DRIVE_HANDLES is a windows-world drive (needs MGC_HOST_DRIVE_TUNE)"
#endif
#endif
#ifdef MGC_HOST_DRIVE_TUNE
#ifndef MGC_HOST_DRIVE_SWEEP
#error "MGC_HOST_DRIVE_TUNE extends the sweep-drive world (pass MGC_HOST_DRIVE_SWEEP too)"
#endif
#endif

#ifdef MGC_HOST_ROW_AN500
#ifndef MGC_HOST_DRIVE_TUNE
#error "a row world extends the windows-drive world (pass MGC_HOST_DRIVE_TUNE too)"
#endif
#include "mgc_board_rows.h"
/* The map's layout symbols, consumed by the containment and
 * window-fit verdicts: the statics window brackets and the arena
 * window the map derives above the statics. Addresses, never
 * contents. */
extern "C" char __sdata[], __edata[], __sbss[], __ebss[];
extern "C" char __rhu_arena_lo[], __rhu_arena_hi[];
/* Bytes actually drawn from the kernel arena through the mmap
 * bridge (region + mark bitmap + mark stack), summed for the
 * window-fit verdict. */
static unsigned long g_row_drawn;
#endif

/* ------------------------------------------------------------------ */
/* kernel exports consumed                                            */
/* ------------------------------------------------------------------ */

/* Console write, NUL-terminated, unprefixed, under the console lock
 * (the payload-feature kernel's export; lines must not tear). */
extern "C" void sh_write0(const char* p);

/* ------------------------------------------------------------------ */
/* the console C runtime (the skeleton's import closure, satisfied     */
/* here so no third-party C library enters the image)                  */
/* ------------------------------------------------------------------ */

extern "C" void abort(void);

/* One line under construction. The activation world is single-threaded
 * (the kmain thread hosts the driver), so one static buffer is the
 * whole story; a line that overflows it is a bug and dies loudly. */
static char g_line[512];
static unsigned g_line_n;

static void line_put(char c)
{
    if (g_line_n >= sizeof(g_line) - 1) {
        g_line[sizeof(g_line) - 1] = 0;
        sh_write0(g_line);
        sh_write0("\n[gc-host] FMT OVERFLOW (line exceeded its buffer)\n");
        abort();
    }
    g_line[g_line_n++] = c;
}

static void line_puts(const char* s)
{
    while (*s)
        line_put(*s++);
}

/* Unsigned render in the given base, minimum width, zero-padded when
 * pad0 (the %08lx family); base is 10 or 16, lowercase digits. */
static void line_num(unsigned long v, unsigned base, unsigned width, bool pad0)
{
    char tmp[16];
    unsigned n = 0;
    do {
        unsigned d = (unsigned)(v % base);
        tmp[n++] = (char)(d < 10 ? '0' + d : 'a' + (d - 10));
        v /= base;
    } while (v && n < sizeof(tmp));
    while (n < width && n < sizeof(tmp))
        tmp[n++] = pad0 ? '0' : ' ';
    while (n)
        line_put(tmp[--n]);
}

/* The enumerated conversion set: %s %c %d %i %u %x %p %% with an
 * optional 'l' length and an optional zero-padded width on the integer
 * conversions. Anything else is a loud death: a NEW conversion arriving
 * with a later gate case must be added here deliberately (and lands
 * pinned in that case's transcript), never guessed at. */
static void vformat(const char* fmt, va_list ap)
{
    for (const char* p = fmt; *p; p++) {
        if (*p != '%') {
            line_put(*p);
            continue;
        }
        p++;
        bool pad0 = false;
        unsigned width = 0;
        if (*p == '0') {
            pad0 = true;
            p++;
        }
        while (*p >= '0' && *p <= '9') {
            width = width * 10 + (unsigned)(*p - '0');
            p++;
        }
        bool l = false;
        if (*p == 'l') {
            l = true;
            p++;
        }
        switch (*p) {
        case '%':
            line_put('%');
            break;
        case 'c':
            line_put((char)va_arg(ap, int));
            break;
        case 's': {
            const char* s = va_arg(ap, const char*);
            line_puts(s ? s : "(null)");
            break;
        }
        case 'd':
        case 'i': {
            long v = l ? va_arg(ap, long) : (long)va_arg(ap, int);
            if (v < 0) {
                line_put('-');
                line_num((unsigned long)(-v), 10, width, pad0);
            } else {
                line_num((unsigned long)v, 10, width, pad0);
            }
            break;
        }
        case 'u': {
            unsigned long v =
                l ? va_arg(ap, unsigned long) : (unsigned long)va_arg(ap, unsigned);
            line_num(v, 10, width, pad0);
            break;
        }
        case 'x': {
            unsigned long v =
                l ? va_arg(ap, unsigned long) : (unsigned long)va_arg(ap, unsigned);
            line_num(v, 16, width, pad0);
            break;
        }
        case 'p': {
            void* v = va_arg(ap, void*);
            line_puts("0x");
            line_num((unsigned long)(uintptr_t)v, 16, 0, false);
            break;
        }
        default:
            g_line[g_line_n] = 0;
            sh_write0(g_line);
            sh_write0("\n[gc-host] FMT UNSUPPORTED conversion in: ");
            sh_write0(fmt);
            sh_write0("\n");
            abort();
        }
    }
}

extern "C" int printf(const char* fmt, ...)
{
    g_line_n = 0;
    va_list ap;
    va_start(ap, fmt);
    vformat(fmt, ap);
    va_end(ap);
    g_line[g_line_n] = 0;
    sh_write0(g_line);
    return (int)g_line_n;
}

extern "C" int puts(const char* s)
{
    g_line_n = 0;
    line_puts(s);
    line_put('\n');
    g_line[g_line_n] = 0;
    sh_write0(g_line);
    return (int)g_line_n;
}

/* The console path is synchronous and unbuffered: flush is complete
 * by construction, whatever stream cookie arrives. */
extern "C" int fflush(void*)
{
    return 0;
}

/* The toolchain's stdio.h renders `stdout` as a field load through the
 * C library's reentrancy pointer, so the skeleton's object imports the
 * pointer SYMBOL and loads one pointer-sized field at a small offset
 * from it. No C library is linked here: the symbol points at a zeroed
 * block large enough for any early field, the loaded value (0) goes to
 * fflush above, and fflush ignores its argument entirely. Nothing else
 * may ever consume this symbol -- linking a real C library replaces it. */
static char g_reent_stub[64];
extern "C" {
void* _impure_ptr = g_reent_stub;
}

extern "C" int strcmp(const char* a, const char* b)
{
    while (*a && *a == *b) {
        a++;
        b++;
    }
    return (int)(unsigned char)*a - (int)(unsigned char)*b;
}

extern "C" char* strstr(const char* hay, const char* needle)
{
    if (!*needle)
        return (char*)hay;
    for (; *hay; hay++) {
        const char* h = hay;
        const char* n = needle;
        while (*h && *n && *h == *n) {
            h++;
            n++;
        }
        if (!*n)
            return (char*)hay;
    }
    return nullptr;
}

/* Semihosting exit, the kernel's mechanism restated in C++ (SYS_EXIT
 * carries the code via the EXTENDED form; QEMU honors it, silicon
 * would need a debugger -- this image class is QEMU-only). */
__attribute__((noreturn)) static void semihost_exit(unsigned code)
{
    unsigned block[2] = { 0x20026u, code }; /* ADP_Stopped_ApplicationExit */
    register unsigned r0 asm("r0");
    register unsigned r1 asm("r1");
    r0 = 0x20; /* SYS_EXIT_EXTENDED */
    r1 = (unsigned)(uintptr_t)block;
    asm volatile("bkpt #0xab" : "+r"(r0) : "r"(r1) : "memory");
    r0 = 0x18; /* SYS_EXIT, for semihosting hosts without the extended form */
    r1 = 0x20026u;
    asm volatile("bkpt #0xab" : "+r"(r0) : "r"(r1) : "memory");
    for (;;) {
        asm volatile("wfi");
    }
}

extern "C" void abort(void)
{
    sh_write0("[gc-host] abort() -- structurally broken path, exiting 134\n");
    semihost_exit(134);
}

#ifdef MGC_HOST_DRIVE_COLLECT
/* A worker thread exists in this world, so yield is real. The only
 * caller is the collector's busy-guard loser path, which the
 * single-collector drive never takes -- the first-call line makes any
 * surprise contention visible (and the gate forbids it). */
extern "C" int sched_yield(void)
{
    static bool logged;
    if (!logged) {
        logged = true;
        sh_write0("[gc-host] sched_yield -> rhu_thread_yield (contended path reached)\n");
    }
    rhu_thread_yield();
    return 0;
}
#else
/* Single-threaded host: a yield with no other runnable thread returns
 * immediately. Logged once so a spin that lands here is attributable. */
extern "C" int sched_yield(void)
{
    static bool logged;
    if (!logged) {
        logged = true;
        sh_write0("[gc-host] sched_yield (single-thread activation host: no-op)\n");
    }
    return 0;
}
#endif

/* The IGCHandleStore vtable's deleting destructor references sized
 * operator delete. Nothing may ever heap-delete the skeleton's
 * placement-new'd singletons; reaching this is a broken world. (When
 * the runtime archives join the link, their C++ runtime shim owns this
 * operator and this definition must leave the image with the rest of
 * the file.) */
void operator delete(void*, size_t)
{
    sh_write0("[gc-host] operator delete reached -- nothing may heap-delete GC singletons\n");
    abort();
}

/* ------------------------------------------------------------------ */
/* the EE-for-initialization surface: the five flat GCToEEInterface    */
/* methods the bare skeleton's init path calls                         */
/* ------------------------------------------------------------------ */

/* The free-object MethodTable, built by the vendored mirror's own
 * recipe (InitializeFreeObject) so the skeleton's shape probe checks
 * the mirror's accessors against the mirror's initializer -- a
 * self-consistency proof; with the runtime linked, its own table
 * replaces this one. */
static MethodTable g_free_mt;

MethodTable* GCToEEInterface::GetFreeObjectMethodTable()
{
    return &g_free_mt;
}

/* No configuration store exists in this world: every lookup honestly
 * reports "not configured" and leaves the caller's default in place. */
bool GCToEEInterface::GetBooleanConfigValue(const char* privateKey,
                                            const char* publicKey, bool* value)
{
    (void)privateKey;
    (void)publicKey;
    (void)value;
    return false;
}

#ifdef MGC_HOST_DRIVE_ALLOC
/* The drive world has exactly one mutator: the kmain thread hosting
 * this driver. Its allocation context sits behind a combined_limit
 * word inside a synthetic thread block -- the EE thread layout's ORDER
 * (Thread's first member is ee_alloc_context: combined_limit first,
 * gc_alloc_context behind it) -- so the allocator's in-call identity
 * probe, GetAllocContext() compared against the acontext parameter,
 * holds here the way it holds on the real EE. Mirror-tier
 * construction: natural alignment places the context at +8 where the
 * real EE packs it at +4; nothing here computes that offset, nothing
 * dereferences Thread fields, and the exact packing is the real EE's,
 * not this host's. */
struct HostMutator {
    uint32_t         combined_limit;
    gc_alloc_context acx;
};
static HostMutator g_mutator;

#ifdef MGC_HOST_DRIVE_TUNE
/* The detach-probe context: enumerated at every collection like a
 * real thread's, given one window through the slow path, then retired
 * through FixAllocContext exactly as a dying thread's would be. */
static HostMutator g_detach_mutator;

/* The EE's per-thread alloc-context answer, host-modeled: the drive
 * points this at whichever context it is allocating on, so the
 * allocator's in-call identity probe holds for the detach probe too. */
static gc_alloc_context* volatile g_cur_acx = &g_mutator.acx;
#endif
#endif

/* During bare activation no EE threads exist and NULL is the honest
 * answer (the skeleton logs, and must not dereference, these); the
 * drive world answers with its one mutator instead. */
Thread* GCToEEInterface::GetThread()
{
#ifdef MGC_HOST_DRIVE_ALLOC
    return (Thread*)(void*)&g_mutator;
#else
    return nullptr;
#endif
}

gc_alloc_context* GCToEEInterface::GetAllocContext()
{
#ifdef MGC_HOST_DRIVE_TUNE
    /* The per-thread answer, host-modeled: the drive points g_cur_acx
     * at whichever context it is allocating on (the detach probe
     * included), so the allocator's in-call identity probe holds the
     * way it holds on the real EE. */
    return g_cur_acx;
#elif defined(MGC_HOST_DRIVE_ALLOC)
    return &g_mutator.acx;
#else
    return nullptr;
#endif
}

/* Single-core platform fact. */
uint32_t GCToEEInterface::GetCurrentProcessCpuCount()
{
    return 1;
}

#ifdef MGC_HOST_DRIVE_ALLOC
/* ------------------------------------------------------------------ */
/* the allocator-drive surface (gate case gc.B): the allocator's       */
/* imports, satisfied the EE-for-initialization way                    */
/* ------------------------------------------------------------------ */

/* The allocator initializes the write-barrier globals through this
 * method -- one Initialize operation writing all five values, the
 * runtime's stomp shape. No barrier code is compiled into this world,
 * so there is nothing to repoint: the parameter shape is asserted (a
 * surprise here is a contract change and must die loud) and the call
 * is recorded on the wire. */
void GCToEEInterface::StompWriteBarrier(WriteBarrierParameters* args)
{
    if (!args || args->operation != WriteBarrierOp::Initialize
        || !args->is_runtime_suspended
        || args->ephemeral_low != (uint8_t*)~(uintptr_t)0
        || args->ephemeral_high != (uint8_t*)~(uintptr_t)0
        || !args->card_table
        || !(args->lowest_address < args->highest_address)) {
        sh_write0("[gc-host] EE StompWriteBarrier: unexpected parameter shape\n");
        abort();
    }
    printf("[gc-host] EE StompWriteBarrier: op=Initialize suspended=1 eph_low=~0 eph_high=~0 card+bounds present (no compiled barriers in this world; shape asserted)\n");
}

/* The heap draws its region through the glibc-shaped anonymous-mapping
 * entry point. This world serves the one ask that surface carries --
 * anonymous private read-write, no fd, no offset, no address hint --
 * from the kernel arena at page alignment; the arena's zero-birth
 * invariant is exactly the pre-zeroed-pages contract the allocator's
 * birth protocol depends on. Any other argument shape is a contract
 * change and dies loud. Exhaustion stays honest: MAP_FAILED, which the
 * heap turns into its own loud init refusal. */
#define HOST_PROT_RW       0x3  /* PROT_READ|PROT_WRITE */
#define HOST_MAP_ANON_PRIV 0x22 /* MAP_PRIVATE|MAP_ANONYMOUS */

extern "C" void* mmap64(void* addr, size_t len, int prot, int flags,
                        int fd, uint64_t off)
{
    if (addr || !len || prot != HOST_PROT_RW || flags != HOST_MAP_ANON_PRIV
        || fd != -1 || off != 0) {
        printf("[gc-host] mmap64: unexpected argument shape (addr=%p len=%lu prot=0x%x flags=0x%x fd=%d)\n",
               addr, (unsigned long)len, (unsigned)prot, (unsigned)flags,
               fd);
        abort();
    }
    uint32_t rounded = ((uint32_t)len + 4095u) & ~4095u;
    uint32_t before = rhu_arena_headroom();
    void* p = rhu_arena_alloc(rounded, 4096u);
    if (!p)
        return (void*)-1;
    uint32_t after = rhu_arena_headroom();
    uint32_t delta = before - after;
    if (delta < rounded || delta >= rounded + 4096u) {
        printf("[gc-host] mmap64: arena headroom delta %lu outside [%lu,%lu)\n",
               (unsigned long)delta, (unsigned long)rounded,
               (unsigned long)(rounded + 4096u));
        abort();
    }
    printf("[gc-host] mmap64: anon RW len=%lu -> served from the kernel arena (page-aligned, zero-born; headroom delta OK)\n",
           (unsigned long)len);
#ifdef MGC_HOST_ROW_AN500
    g_row_drawn += (unsigned long)rounded;
#endif
    return p;
}
#endif /* MGC_HOST_DRIVE_ALLOC */

#ifdef MGC_HOST_DRIVE_COLLECT
/* ------------------------------------------------------------------ */
/* the collection-drive surface (gate case gc.C): the collector's      */
/* EE imports, and the one real mutator the world stops               */
/* ------------------------------------------------------------------ */

static void coll_host_die(const char* what)
{
    printf("[gc-host] COLLECT SURFACE CHECK FAILED: %s\n", what);
    abort();
}

/* The drive's two hosted threads sit at fixed levels below the host,
 * a strict order every observation window below relies on. The host
 * drives at the flat hosted level (RHU_HOSTED_PRIO_FLAT, the level the
 * payload host thread holds for the entry's life); the waiter sits at
 * a level less urgent than that; the worker sits at the band's least
 * urgent level. So the worker runs only while the host sleeps and
 * nothing more urgent is runnable, and the waiter, whenever it is
 * runnable, runs before the worker and blocks quickly. Each value is
 * printed from the same definition the spawn uses, and the order is
 * asserted against the kernel's band at compile time. */
#define GC_HOST_WAITER_PRIO 0xC0
#define GC_HOST_WORKER_PRIO RHU_HOSTED_PRIO_HI
#define GC_HOST_STR_(x) #x
#define GC_HOST_STR(x) GC_HOST_STR_(x)
static_assert(RHU_HOSTED_PRIO_FLAT < GC_HOST_WAITER_PRIO,
              "the waiter must be less urgent than the host's flat level");
static_assert(GC_HOST_WAITER_PRIO < GC_HOST_WORKER_PRIO,
              "the waiter must be more urgent than the worker");
static_assert(GC_HOST_WORKER_PRIO <= RHU_HOSTED_PRIO_HI,
              "the worker must lie inside the hosted band");

/* The worker: the one thread the stop-the-world actually stops. It
 * parks a root slot on its own stack (published so GcScanRoots can
 * report the SLOT ADDRESS -- the precise-scan shape), then counts a
 * volatile word forever. It never allocates, never prints, and never
 * touches GC state, so suspending it at any instruction is safe --
 * which is exactly what lets the kernel verb's own contract (stop =
 * never picked; context snapshot; resume = exception return) be the
 * thing under test rather than EE parking policy. Spawned at the
 * band's least urgent level (GC_HOST_WORKER_PRIO) under a host that
 * drives at the flat level: the worker runs ONLY while kmain sleeps
 * and nothing more urgent is runnable, so every observation window
 * below is deterministic. */
static volatile uint32_t   g_counter;
/* Both levels volatile: the VARIABLE is written by the worker and read
 * by kmain (an unqualified static here lets the compiler prove kmain
 * only ever sees NULL and plant a udf trap), and the SLOT it points
 * at lives on the worker's stack. */
static Object* volatile* volatile g_worker_slot;
static volatile int        g_worker_ready;
static int32_t             g_worker_tid;
static volatile int        g_world_suspended;
static HostMutator         g_worker_mutator; /* zeroed; handed at enum */
static IGCHandleManager*   g_handle_mgr;     /* stashed for the drive's
                                              * handle-store use */

#ifdef MGC_HOST_DRIVE_TUNE
/* The waiter's epoch machinery. go is a futex word the waiter parks
 * on between exercises (a blocked idle thread -- a spinning one would
 * starve the counter inside kmain's sleep windows); done is the
 * completion witness the drive asserts; the heap pointer is published
 * for the waiter's vcalls and the EE-side event wiring (both levels
 * of every cross-thread pointer volatile -- the compiler otherwise
 * proves a never-written NULL and plants a trap). */
static volatile uint32_t   g_waiter_go;
static volatile uint32_t   g_waiter_done;
static volatile uint32_t   g_activations;
static int32_t             g_waiter_tid;
static IGCHeap* volatile   g_heap_ee;
#endif

static void gc_worker(void* arg)
{
    Object* volatile local = (Object*)arg;   /* the stack root slot */
    g_worker_slot  = (Object* volatile*)&local;
    g_worker_ready = 1;
    for (;;)
        g_counter = g_counter + 1;
}

#ifdef MGC_HOST_DRIVE_TUNE
/* The process activation handler (one per image, kernel contract).
 * Runs on the target's own stack at its next scheduling-in; the
 * console lock keeps the print whole. */
static void gc_activation_handler(rhu_ctx* ctx)
{
    (void)ctx;
    g_activations = g_activations + 1;
    printf("[gc-host] activation handler ran (delivered on the target's own stack)\n");
}

/* The waiter: the EE's trapped-mutator role, one park per epoch. It
 * blocks on the go futex between epochs (never spins -- kmain's sleep
 * windows belong to the counter), and inside an epoch it sleeps on
 * the skeleton's futex-backed GC-done event through the same vtable
 * call the EE's trapped-thread loop makes. EAGAIN/EINTR on the go
 * wait re-check the epoch word, the standard recipe. Spawned at
 * GC_HOST_WAITER_PRIO -- less urgent than the host, more urgent than
 * the counter -- so whenever it is runnable it runs first in a window
 * and blocks quickly. */
static void gc_waiter(void* arg)
{
    (void)arg;
    uint32_t seen = 0;
    for (;;) {
        rhu_futex_wait((const uint32_t*)&g_waiter_go, seen, RHU_DEADLINE_NONE);
        if (g_waiter_go == seen)
            continue;
        seen = g_waiter_go;
        printf("[gc-host] waiter: parking on the GC-done event (epoch %u)\n", seen);
        g_heap_ee->WaitUntilGCComplete(false);
        printf("[gc-host] waiter: woken (epoch %u)\n", seen);
        g_waiter_done = seen;
    }
}
#endif

/* The mark's root schedule, reported by GcScanRoots below. */
static Object* g_root_static;   /* -> alloc #1 */
static Object* g_root_null;     /* stays null (null-slot class) */
static Object* g_root_oob;      /* -> a host static (out-of-heap class) */
static Object* g_root_pair;     /* -> alloc #6 (the traced-edge source) */

#ifdef MGC_HOST_DRIVE_SWEEP
/* Two more root slots for the death world: the kept finalizable (the
 * partition's live branch needs a specimen that never dies) and the
 * big-object slot the wall exercise roots and then drops. */
static Object* g_root_fin;      /* -> alloc #17, rooted for the whole run */
static Object* g_root_big;      /* -> the 1.9 MB object, dropped mid-drive */

/* Two more mirror-built MethodTables. The critical-finalizer table
 * carries the REAL flag the partition tests (MT flags & (component |
 * critical) == critical); the WR-mirror table needs no
 * flag -- EagerFinalized answers by identity, playing the EE's
 * has-eager-finalizer test for exactly one type. Neither contains GC
 * pointers, so neither carries a GCDesc. */
static struct { MethodTable mt; } g_crit_block;
static struct { MethodTable mt; } g_eager_block;

/* The WR-mirror's "underlying" handle: created before the cast dies,
 * destroyed BY the host's EagerFinalized through the real manager --
 * the runtime's WeakReference eager-finalizer behavior. */
static OBJECTHANDLE g_h_und;
#endif


/* A ContainsGCPointers MethodTable, mirror-built like g_free_mt: one
 * GCDesc series laid out backward before the MT per the vendored
 * gcdesc.h (series count at MT-4, the series record below it), so the
 * tracer walks a REAL series in this world. baseSize 16 = header +
 * MT slot + two fields; the ref field is at object+8 (startoffset 8),
 * series size stored biased by -baseSize per the vendored recipe (one
 * slot: 4 - 16). The vendored accessors are the self-check. */
static struct PairMTBlock {
    CGCDescSeries series;
    size_t        nseries;
    MethodTable   mt;
} g_pair_block;
static_assert(sizeof(CGCDescSeries) == 8 && sizeof(size_t) == 4,
              "pair GCDesc layout assumes the 32-bit vendored shapes");

static void pair_mt_init(void)
{
    g_pair_block.series.SetSeriesSize((size_t)4 - (size_t)16);
    g_pair_block.series.SetSeriesOffset(8);
    g_pair_block.nseries = 1;
    g_pair_block.mt.m_flags = MTFlag_ContainsGCPointers;
    g_pair_block.mt.m_baseSize = 16;
    g_pair_block.mt.m_pRelatedType = nullptr;
    CGCDesc* d = CGCDesc::GetCGCDescFromMT(&g_pair_block.mt);
    if (!g_pair_block.mt.ContainsGCPointers()
        || d->GetNumSeries() != 1
        || d->GetHighestSeries() != &g_pair_block.series
        || d->GetLowestSeries() != &g_pair_block.series
        || d->GetHighestSeries()->GetSeriesOffset() != 8)
        coll_host_die("pair MethodTable GCDesc did not read back through the vendored accessors");
    printf("[gc-host] pair MT built: ContainsGCPointers baseSize=16, 1 series (offset 8, one slot) -- vendored-accessor readback OK\n");
}

/* The frozen segment: a host static registered through the real
 * interface. Layout: [0,4) header pad; a 16-byte pair object at +4
 * whose ref field (+12) points INTO the heap; a 12-byte free object
 * at +20. The edge store is a plain write -- NO barrier records
 * frozen->heap stores, which is exactly why every collection walks
 * frozen objects as a root source (the mark_ro_segments position). */
alignas(8) static uint8_t g_frozen_buf[32];

static void frozen_build_and_register(IGCHeap* heap, Object* edge_target)
{
    *(MethodTable**)(void*)(g_frozen_buf + 4)  = &g_pair_block.mt;
    *(Object**)(void*)(g_frozen_buf + 12)      = edge_target;
    *(MethodTable**)(void*)(g_frozen_buf + 20) =
        GCToEEInterface::GetFreeObjectMethodTable();
    segment_info si;
    si.pvMem         = g_frozen_buf;
    si.ibFirstObject = 4;
    si.ibAllocated   = 28;
    si.ibCommit      = 32;
    si.ibReserved    = 32;
    if (!heap->RegisterFrozenSegment(&si))
        coll_host_die("RegisterFrozenSegment refused the host segment");
    printf("[gc-host] frozen segment registered: 2 objects, one frozen->heap ref edge (plain store -- no barrier covers frozen memory)\n");
}

/* --- the twelve collection-era flat EE methods ------------------- */

/* The stop-the-world bracket over the kernel suspension verbs. The
 * counter verdicts never print raw values (icount-class fragility);
 * frozen/advanced are the pinned facts. */
void GCToEEInterface::SuspendEE(SUSPEND_REASON reason)
{
    if (reason != SUSPEND_FOR_GC)
        coll_host_die("SuspendEE reason is not SUSPEND_FOR_GC");
    if (g_worker_tid < 1 || g_world_suspended)
        coll_host_die("SuspendEE before the worker exists, or double suspend");
    rhu_ctx ctx;
    if (rhu_thread_suspend(g_worker_tid, &ctx) != 0)
        coll_host_die("rhu_thread_suspend refused the worker");
    uint32_t lo = 0, bytes = 0;
    if (rhu_thread_stack_bounds(g_worker_tid, &lo, &bytes) != 0)
        coll_host_die("rhu_thread_stack_bounds refused the worker");
    if (bytes != 4096u)
        coll_host_die("stack bounds are not the spawned 4096 bytes");
    if (!(ctx.sp >= lo && ctx.sp < lo + bytes))
        coll_host_die("suspended ctx.sp outside the worker's stack bounds");
    uint32_t slot = (uint32_t)(uintptr_t)g_worker_slot;
    if (!(slot >= lo && slot < lo + bytes))
        coll_host_die("the published root slot lies outside the worker's stack bounds");
#ifdef MGC_HOST_DRIVE_TUNE
    /* The EE arms the GC-done event INSIDE its suspension path.
     * Armed before the frozen-check window below, so
     * a waiter scheduled in that window finds the event down and
     * genuinely sleeps. */
    g_heap_ee->ResetWaitForGCEvent();
#endif
    uint32_t c1 = g_counter;
    rhu_sleep_ns(2000000ull);   /* a window the worker WOULD use */
    if (g_counter != c1)
        coll_host_die("counter advanced while suspended (the world did not stop)");
    g_world_suspended = 1;
    printf("[gc-host] EE SuspendEE(reason=%d): worker tid=%d suspended; ctx.sp + published slot inside stack bounds (4096 B); counter FROZEN across a sleep window\n",
           (int)reason, (int)g_worker_tid);
}

void GCToEEInterface::RestartEE(bool bFinishedGC)
{
    if (!bFinishedGC || !g_world_suspended)
        coll_host_die("RestartEE without a finished, suspended collection");
#ifdef MGC_HOST_DRIVE_TUNE
    /* The EE releases the GC-done sleepers from its restart path --
     * the Set stores 1 before waking, so there is no lost-wakeup
     * window against a re-checking waiter. */
    g_heap_ee->SetWaitForGCEvent();
#endif
    if (rhu_thread_resume(g_worker_tid) != 0)
        coll_host_die("rhu_thread_resume refused the worker");
    g_world_suspended = 0;
    uint32_t c1 = g_counter;
    unsigned tries = 0;
    for (; tries < 50; tries++) {
        rhu_sleep_ns(2000000ull);
        if (g_counter != c1)
            break;
    }
    if (tries == 50)
        coll_host_die("counter did not advance after resume (worker never ran again)");
    printf("[gc-host] EE RestartEE: worker resumed; counter ADVANCED within the bounded window\n");
}

/* Hand every mutator context, EE-parity: the wrapper zeroes
 * combined_limit itself when alloc_limit reads zero after the
 * callback (the EE's GcEnumAllocContexts body). */
void GCToEEInterface::GcEnumAllocContexts(enum_alloc_context_func* fn, void* param)
{
    fn(&g_mutator.acx, param);
    if (g_mutator.acx.alloc_limit == nullptr)
        g_mutator.combined_limit = 0;
    fn(&g_worker_mutator.acx, param);
    if (g_worker_mutator.acx.alloc_limit == nullptr)
        g_worker_mutator.combined_limit = 0;
#ifdef MGC_HOST_DRIVE_TUNE
    fn(&g_detach_mutator.acx, param);
    if (g_detach_mutator.acx.alloc_limit == nullptr)
        g_detach_mutator.combined_limit = 0;
    printf("[gc-host] EE GcEnumAllocContexts: 3 contexts handed (kmain books, worker zeroed, detach probe; combined_limit EE-zeroed when limit==0)\n");
#else
    printf("[gc-host] EE GcEnumAllocContexts: 2 contexts handed (kmain books, worker zeroed; combined_limit EE-zeroed when limit==0)\n");
#endif
}

/* The fixed 7-slot root report: the precise-scan shape (slot
 * ADDRESSES handed, qualifiers on two of them), covering the null,
 * out-of-heap, duplicate, interior, and pinned classes on top of the
 * two live static roots and the worker's stack slot. */
void GCToEEInterface::GcScanRoots(promote_func* fn, int condemned, int max_gen,
                                  ScanContext* sc)
{
    (void)condemned;
    (void)max_gen;
    fn(&g_root_static, sc, 0);
    fn(&g_root_null, sc, 0);
    fn(&g_root_oob, sc, 0);
    fn((Object**)g_worker_slot, sc, 0);
    Object* interior = (Object*)((uint8_t*)(void*)g_root_static + 8);
    fn(&interior, sc, GC_CALL_INTERIOR);
    fn(&g_root_static, sc, GC_CALL_PINNED);
    fn(&g_root_pair, sc, 0);
#ifdef MGC_HOST_DRIVE_SWEEP
    fn(&g_root_fin, sc, 0);
    fn(&g_root_big, sc, 0);
    printf("[gc-host] EE GcScanRoots: reported 9 slots (static, null, out-of-heap, worker-stack, interior, pinned-dup, pair-static, fin-static, big-static)\n");
#else
    printf("[gc-host] EE GcScanRoots: reported 7 slots (static, null, out-of-heap, worker-stack, interior, pinned-dup, pair-static)\n");
#endif
}

/* The callout brackets. No RestrictedCallouts are registered in this
 * world (the real EE's kinds 0/2/1 dispatch is rt machinery); the
 * order of these lines on the wire IS the EE's callback order,
 * asserted by the gate. */
void GCToEEInterface::GcStartWork(int condemned, int max_gen)
{
    printf("[gc-host] EE GcStartWork(condemned=%d max_gen=%d) (no RestrictedCallouts exist in this world)\n",
           condemned, max_gen);
}

void GCToEEInterface::BeforeGcScanRoots(int condemned, bool is_bgc,
                                        bool is_concurrent)
{
    if (is_bgc || is_concurrent)
        coll_host_die("BeforeGcScanRoots claimed a concurrent flavor");
    printf("[gc-host] EE BeforeGcScanRoots(condemned=%d) non-concurrent\n",
           condemned);
}

void GCToEEInterface::AfterGcScanRoots(int condemned, int max_gen,
                                       ScanContext* sc)
{
    (void)sc;
    printf("[gc-host] EE AfterGcScanRoots(condemned=%d max_gen=%d)\n",
           condemned, max_gen);
}

void GCToEEInterface::GcDone(int condemned)
{
    printf("[gc-host] EE GcDone(condemned=%d)\n", condemned);
}

void GCToEEInterface::DiagGCStart(int gen, bool isInduced)
{
    printf("[gc-host] EE DiagGCStart(gen=%d induced=%d)\n", gen, (int)isInduced);
}

void GCToEEInterface::DiagGCEnd(size_t index, int gen, int reason,
                                bool fConcurrent)
{
    if (fConcurrent)
        coll_host_die("DiagGCEnd claimed a concurrent collection");
    printf("[gc-host] EE DiagGCEnd(index=%lu gen=%d reason=%d) non-concurrent\n",
           (unsigned long)index, gen, reason);
}

#ifdef MGC_HOST_DRIVE_SWEEP
/* --- the death-pipeline-era flat EE methods (gate case gc.C-sweep) - */

/* The partition's FIRST question about every dead registered entry.
 * True only for the WR-mirror type, and the true path carries the
 * EE-side duty: destroy the WeakReference's underlying handle through
 * the GC's own manager; the entry is then
 * dropped, never promoted, no finalizer. The common false path stays
 * silent (it answers for every dead entry). */
bool GCToEEInterface::EagerFinalized(Object* obj)
{
    if (obj->RawGetMethodTable() == &g_eager_block.mt) {
        printf("[gc-host] EE EagerFinalized(the WR-mirror) -> true; underlying handle destroyed via the manager\n");
        g_handle_mgr->DestroyHandleOfType(g_h_und, HNDTYPE_WEAK_SHORT);
        return true;
    }
    return false;
}

/* A literal no-op in the EE body; the call is the parity fact, and
 * its position in the bracket is order-asserted. */
void GCToEEInterface::DiagWalkFReachableObjects(void* gcContext)
{
    (void)gcContext;
    printf("[gc-host] EE DiagWalkFReachableObjects (no-op parity)\n");
}

/* SyncBlockCacheWeakPtrScan -- also a no-op EE-side. */
void GCToEEInterface::SyncBlockCacheWeakPtrScan(HANDLESCANPROC scanProc,
                                                uintptr_t lp1, uintptr_t lp2)
{
    (void)scanProc;
    (void)lp1;
    (void)lp2;
    printf("[gc-host] EE SyncBlockCacheWeakPtrScan (no-op parity)\n");
}

/* The finalizer-thread event. In this world the drive IS the
 * finalizer thread: it drains GetNextFinalizable after the collection
 * returns, so the method just witnesses the only-when-promoted call. */
void GCToEEInterface::EnableFinalization(bool gcHasWorkForFinalizerThread)
{
    printf("[gc-host] EE EnableFinalization(work=%d)\n",
           (int)gcHasWorkForFinalizerThread);
}
#endif /* MGC_HOST_DRIVE_SWEEP */

/* The busy-guard loser dance. The single-collector drive never
 * contends, so these firing means a world change -- the lines exist to
 * make that loud, and the gate forbids them on the positive wire. */
bool GCToEEInterface::EnablePreemptiveGC()
{
    printf("[gc-host] EE EnablePreemptiveGC (busy-guard loser path -- must never fire in the single-collector drive)\n");
    return true;
}

void GCToEEInterface::DisablePreemptiveGC()
{
    printf("[gc-host] EE DisablePreemptiveGC (busy-guard loser path -- must never fire in the single-collector drive)\n");
}
#endif /* MGC_HOST_DRIVE_COLLECT */

/* ------------------------------------------------------------------ */
/* the activation driver (the payload-feature kernel's managed entry)  */
/* ------------------------------------------------------------------ */

/* The costume seam the skeleton exports (two entry points; a third
 * is a contract change the hook refuses). */
extern "C" void* __mgc_dlopen_hook(const char* f);
extern "C" void* __mgc_dlsym_hook(void* h, const char* n);

/* The module-initializer walk the kernel entry sequence performs. The
 * gc.A image links no managed module, so the bracketed table is empty
 * by construction; a non-empty table here is a foreign object in the
 * image and dies loudly. */
extern "C" void InitializeModules(uintptr_t os_module, const uintptr_t* headers,
                                  int32_t count, const uintptr_t* classlib,
                                  int32_t n_classlib)
{
    (void)os_module;
    (void)headers;
    (void)classlib;
    (void)n_classlib;
    printf("[gc-host] InitializeModules(count=%d) -- no managed modules in this world\n",
           (int)count);
    if (count != 0)
        abort();
}

#ifdef MGC_HOST_DRIVE_ALLOC
/* ------------------------------------------------------------------ */
/* the allocator drive (gate case gc.B): a bounded, deterministic      */
/* schedule through IGCHeap::Alloc -- the discriminator between the    */
/* live-allocator image (every step verified, exit 0) and the bare-    */
/* skeleton control (the first allocation must fail-fast).             */
/* ------------------------------------------------------------------ */

static void drive_check(bool ok, const char* what)
{
    if (!ok) {
        printf("[gc-host] ALLOC DRIVE CHECK FAILED: %s\n", what);
        abort();
    }
}

/* One allocation through the interface. The returned footprint
 * [obj-4, obj+size) is verified all-zero BEFORE anything is stored --
 * the allocator's job is zeroed, aligned, header-slot-preceded memory;
 * the store afterward is the EE side's birth-protocol duty (the caller
 * writes the MethodTable into pre-zeroed memory), mimicked here with
 * the free-object table + numComponents = size-12, which keeps the
 * region walk-valid (free-object baseSize 12, componentSize 1). */
static Object* drive_alloc(IGCHeap* heap, gc_alloc_context* acx,
                           size_t size, uint32_t flags, unsigned no)
{
    Object* o = heap->Alloc(acx, size, flags);
    drive_check(o != nullptr, "allocation returned NULL on the live region");
    const unsigned char* fp = (const unsigned char*)(const void*)o - 4;
    for (size_t i = 0; i < size + 4; i++)
        drive_check(fp[i] == 0, "a handed-out byte was not zero-born");
    *(MethodTable**)(void*)o = GCToEEInterface::GetFreeObjectMethodTable();
    *(uint32_t*)((unsigned char*)(void*)o + 4) = (uint32_t)size - 12u;
    printf("[gc-host] alloc #%u size=%lu flags=0x%lx: zero-born OK (%lu bytes incl. header slot), free-object birth stored\n",
           no, (unsigned long)size, (unsigned long)flags,
           (unsigned long)(size + 4));
    return o;
}

#ifdef MGC_HOST_DRIVE_COLLECT
/* One pair-object allocation: the birth-protocol store writes the
 * host-built ContainsGCPointers MethodTable and the ref field the
 * tracer must walk (GC_ALLOC_CONTAINS_REF passed honestly -- the real
 * EE flags ref-carrying allocations exactly so). */
static Object* drive_alloc_pair(IGCHeap* heap, gc_alloc_context* acx,
                                Object* ref, unsigned no)
{
    Object* o = heap->Alloc(acx, 16, GC_ALLOC_CONTAINS_REF);
    drive_check(o != nullptr, "pair allocation returned NULL on the live region");
    const unsigned char* fp = (const unsigned char*)(const void*)o - 4;
    for (size_t i = 0; i < 16 + 4; i++)
        drive_check(fp[i] == 0, "a handed-out pair byte was not zero-born");
    *(MethodTable**)(void*)o = &g_pair_block.mt;
    *(Object**)((unsigned char*)(void*)o + 8) = ref;
    printf("[gc-host] alloc #%u size=16 flags=0x2: zero-born OK (20 bytes incl. header slot), pair birth stored (ContainsGCPointers; ref field -> alloc #4)\n",
           no);
    return o;
}
#endif /* MGC_HOST_DRIVE_COLLECT */

#ifdef MGC_HOST_DRIVE_SWEEP
/* One allocation born with a caller-supplied no-component MethodTable
 * (the crit / WR-mirror types); baseSize must equal the ask so the
 * heap stays walkable. Virgin-serve zero check, then the birth store. */
static Object* drive_alloc_mt(IGCHeap* heap, gc_alloc_context* acx,
                              MethodTable* mt, const char* kind, unsigned no)
{
    Object* o = heap->Alloc(acx, 16, GC_ALLOC_FINALIZE);
    drive_check(o != nullptr, "MT-cast allocation returned NULL on the live region");
    const unsigned char* fp = (const unsigned char*)(const void*)o - 4;
    for (size_t i = 0; i < 16 + 4; i++)
        drive_check(fp[i] == 0, "a handed-out cast byte was not zero-born");
    *(MethodTable**)(void*)o = mt;
    printf("[gc-host] alloc #%u size=16 flags=0x1: zero-born OK, %s birth stored\n",
           no, kind);
    return o;
}

/* One allocation that may be served from the free list. The zero
 * check covers exactly the zero-on-reuse contract span [obj-4,
 * obj+size-4): the word beyond it is the successor's header slot,
 * explicitly OUTSIDE the guarantee (a live successor may carry
 * FINALIZER_RUN state there). */
static Object* drive_alloc_reuse(IGCHeap* heap, gc_alloc_context* acx,
                                 size_t size, uint32_t flags, unsigned no)
{
    Object* o = heap->Alloc(acx, size, flags);
    drive_check(o != nullptr, "reuse-era allocation returned NULL");
    const unsigned char* fp = (const unsigned char*)(const void*)o - 4;
    for (size_t i = 0; i < size; i++)
        drive_check(fp[i] == 0, "a reused byte violated zero-on-reuse");
    *(MethodTable**)(void*)o = GCToEEInterface::GetFreeObjectMethodTable();
    *(uint32_t*)((unsigned char*)(void*)o + 4) = (uint32_t)size - 12u;
    printf("[gc-host] alloc #%u size=%lu flags=0x%lx: zero-on-reuse OK ([obj-4, obj+size-4) all zero), free-object birth stored\n",
           no, (unsigned long)size, (unsigned long)flags);
    return o;
}
#endif /* MGC_HOST_DRIVE_SWEEP */

#ifdef MGC_HOST_DRIVE_TUNE
/* The EE fast path's role: serve a small allocation from the
 * context's outstanding bump window -- no vtable call, birth store
 * into pre-zeroed window memory, alloc_ptr advanced. Consumed windows
 * are what makes the fix-time object walk real. */
static Object* drive_alloc_win(gc_alloc_context* acx, size_t size, unsigned no)
{
    uint8_t* p     = (uint8_t*)acx->alloc_ptr;
    uint8_t* limit = (uint8_t*)acx->alloc_limit;
    drive_check(p != nullptr && limit != nullptr && (size_t)(limit - p) >= size,
                "fast path asked with no window room (posture drift)");
    const unsigned char* fp = (const unsigned char*)p - 4;
    for (size_t i = 0; i < size; i++)
        drive_check(fp[i] == 0, "a window byte was not zero at fast birth");
    *(MethodTable**)(void*)p = GCToEEInterface::GetFreeObjectMethodTable();
    *(uint32_t*)(p + 4) = (uint32_t)size - 12u;
    acx->alloc_ptr = p + size;
    printf("[gc-host] alloc #%u size=%lu FAST-PATH from the window: zero-born OK, free-object birth stored\n",
           no, (unsigned long)size);
    return (Object*)(void*)p;
}
#endif /* MGC_HOST_DRIVE_TUNE */

#ifdef MGC_HOST_DRIVE_FREEPAD
/* ------------------------------------------------------------------ */
/* the free-pad drive (gate case gc.free-pad): a dead space of exactly */
/* 12 bytes before a live object whose header word is in use.          */
/* ------------------------------------------------------------------ */

/* The header-word state the runtime keeps in the word before an object:
 * a hash code (the is-hash-code bit with 26 bits of hash), the pattern a
 * program that hashed the object would have left there. */
#define FREEPAD_HEADER_STATE 0x08ABCDEFu

static int32_t drive_free_pad(IGCHeap* heap)
{
    printf("[gc-host] gc.free-pad drive: a dead 12-byte object before a live object whose header word is in use, then two sweeps\n");
    gc_alloc_context* acx = &g_mutator.acx;

    /* #1 the 12-byte object that dies; #2 the live object right after it,
     * its header word carrying a hash code; #3 a second dead object (the
     * next run the sweep installs); #4 a live object bounding that run. */
    Object* o1 = drive_alloc(heap, acx, 12, 0, 1);
    Object* o2 = drive_alloc(heap, acx, 16, 0, 2);
    drive_check((unsigned char*)o2 == (unsigned char*)o1 + 12,
                "the live object is not adjacent to the 12-byte object");
    Object* o3 = drive_alloc(heap, acx, 24, 0, 3);
    Object* o4 = drive_alloc(heap, acx, 16, 0, 4);
    drive_check((unsigned char*)o3 == (unsigned char*)o2 + 16
                && (unsigned char*)o4 == (unsigned char*)o3 + 24,
                "the four objects are not adjacent in address order");
    *(volatile uint32_t*)((unsigned char*)(void*)o2 - 4) = FREEPAD_HEADER_STATE;
    printf("[gc-host] free-pad: layout #1 dead 12 B | #2 live 16 B (header word set to 0x%08lx) | #3 dead 24 B | #4 live 16 B\n",
           (unsigned long)FREEPAD_HEADER_STATE);

    g_root_static = o2;
    g_root_null   = nullptr;
    g_root_oob    = (Object*)(void*)&g_mutator;
    g_root_pair   = o4;
    g_root_fin    = nullptr;
    g_root_big    = nullptr;

    g_worker_tid = rhu_thread_spawn(&gc_worker, (void*)o4, 4096u,
                                    GC_HOST_WORKER_PRIO,
                                    "gcmut");
    drive_check(g_worker_tid >= 1, "worker spawn refused");
    printf("[gc-host] worker spawned: tid=%d prio=" GC_HOST_STR(GC_HOST_WORKER_PRIO)
           " stack=4096 (runs only while kmain sleeps)\n",
           (int)g_worker_tid);
    {
        unsigned tries = 0;
        for (; tries < 100 && !g_worker_ready; tries++)
            rhu_sleep_ns(2000000ull);
    }
    drive_check(g_worker_ready == 1, "worker never published its stack slot");
    drive_check(*g_worker_slot == o4,
                "the worker stack slot does not hold alloc #4");
    printf("[gc-host] worker ready: stack slot published (alloc #4 parked on the worker stack)\n");

    /* Sweep #1: #1 becomes the 12-byte dead space, #3 the first free run
     * the sweep links. The live header word is read back afterwards. */
    drive_check(heap->GarbageCollect(2, false, 0) == 0,
                "GarbageCollect #1 rc != S_OK");
    uint32_t hw = *(volatile uint32_t*)((unsigned char*)(void*)o2 - 4);
    printf("[gc-host] free-pad: the live object's header word after sweep #1 reads 0x%08lx (%s)\n",
           (unsigned long)hw,
           hw == FREEPAD_HEADER_STATE ? "INTACT" : "CLOBBERED");
    drive_check(hw == FREEPAD_HEADER_STATE,
                "the sweep wrote a free-list link into the live successor's header word");

    /* The first fit's remainder rule: a 12-byte ask is served from the
     * 24-byte run at #3's address; the run is consumed whole and its
     * 12-byte remainder is left as a pad, never as a list node. */
    Object* o5 = drive_alloc_reuse(heap, acx, 12, 0, 5);
    drive_check(o5 == o3,
                "the 12-byte ask was not served at the dead 24-byte object's address");
    printf("[gc-host] free-pad: a 12-byte ask reused the 24-byte run at alloc #3's address (the run consumed whole, its 12-byte remainder a pad)\n");

    /* Sweep #2: the pads walk (the collector's walk model counts them)
     * and the live header word still reads as set. */
    drive_check(heap->GarbageCollect(2, false, 0) == 0,
                "GarbageCollect #2 rc != S_OK");
    hw = *(volatile uint32_t*)((unsigned char*)(void*)o2 - 4);
    printf("[gc-host] free-pad: the live object's header word after sweep #2 reads 0x%08lx (%s)\n",
           (unsigned long)hw,
           hw == FREEPAD_HEADER_STATE ? "INTACT" : "CLOBBERED");
    drive_check(hw == FREEPAD_HEADER_STATE,
                "the second sweep disturbed the live successor's header word");
    printf("[gc-host] GC FREE-PAD EXERCISE OK (a 12-byte dead space before a live object left its header word intact across two sweeps; the pads walked and counted; a 12-byte first-fit remainder left as a pad)\n");
    return 0;
}
#endif /* MGC_HOST_DRIVE_FREEPAD */

#ifdef MGC_HOST_DRIVE_HANDLES
/* ------------------------------------------------------------------ */
/* the handle-pool drive (gate case gc.handle-pool): two hosted threads */
/* at one priority create and destroy handles at once through the       */
/* handle manager's own entries, for a bounded count.                    */
/* ------------------------------------------------------------------ */

/* Iterations per worker; each iteration creates HD_BURST handles on the
 * worker's own object, reads every one back at once, reads them back
 * again and destroys them. The count is a plan fact, sized so a boot
 * spans hundreds of scheduler ticks: on one core the tick is the only
 * preemption, and a pool race needs one inside the pool's
 * read-modify-write. Both workers sit at one level above the counter
 * worker and below the host, so they share the core by rotation while
 * the host parks and the counter waits its turn. */
#ifndef MGC_HANDLES_ITER
#define MGC_HANDLES_ITER 20000u
#endif
#define HD_BURST 8u
#define HD_WORKER_PRIO GC_HOST_WAITER_PRIO

struct HdWorker {
    Object*      obj;          /* the worker's own object */
    uint32_t     creates;
    uint32_t     destroys;
    uint32_t     aliases;      /* a handle that read another object */
    OBJECTHANDLE first_alias;  /* the first such handle, null = none */
    Object*      first_seen;   /* what it read */
    uint64_t     ns;           /* the worker's own elapsed time */
    int32_t      tid;
};
static HdWorker            g_hd[2];
static volatile uint32_t   g_hd_done;

static void hd_worker(void* arg)
{
    HdWorker* w = (HdWorker*)arg;
    OBJECTHANDLE h[HD_BURST];
    uint64_t t0 = rhu_now_ns();
    for (uint32_t it = 0; it < MGC_HANDLES_ITER; it++) {
        for (uint32_t j = 0; j < HD_BURST; j++) {
            h[j] = g_handle_mgr->CreateGlobalHandleOfType(w->obj, HNDTYPE_STRONG);
            w->creates++;
            Object* seen = *(Object* volatile*)h[j];
            if (seen != w->obj) {
                w->aliases++;
                if (!w->first_alias) { w->first_alias = h[j]; w->first_seen = seen; }
            }
        }
        for (uint32_t j = 0; j < HD_BURST; j++) {
            Object* seen = *(Object* volatile*)h[j];
            if (seen != w->obj) {
                w->aliases++;
                if (!w->first_alias) { w->first_alias = h[j]; w->first_seen = seen; }
            }
            g_handle_mgr->DestroyHandleOfType(h[j], HNDTYPE_STRONG);
            w->destroys++;
        }
    }
    w->ns = rhu_now_ns() - t0;
    __atomic_add_fetch(&g_hd_done, 1u, __ATOMIC_SEQ_CST);
    (void)rhu_futex_wake((const uint32_t*)&g_hd_done, 1);
    rhu_thread_exit();
}

static int32_t drive_handles(IGCHeap* heap)
{
    printf("[gc-host] gc.handle-pool drive: two hosted threads at one priority create and destroy handles at once through the handle manager's own entries (%u iterations of %u handles each)\n",
           (unsigned)MGC_HANDLES_ITER, (unsigned)HD_BURST);
    gc_alloc_context* acx = &g_mutator.acx;
    /* The heap pointer the EE-side event wiring reads inside the
     * stop-the-world (the GC-done event's reset and set), published the
     * way every windows-world drive publishes it before its first
     * collection. */
    g_heap_ee = heap;
    Object* o1 = drive_alloc(heap, acx, 16, 0, 1);
    Object* o2 = drive_alloc(heap, acx, 16, 0, 2);
    Object* o3 = drive_alloc(heap, acx, 16, 0, 3);
    g_root_static = o1;
    g_root_null   = nullptr;
    g_root_oob    = (Object*)(void*)&g_mutator;
    g_root_pair   = o2;
    g_root_fin    = nullptr;
    g_root_big    = nullptr;

    /* The counter worker every collection drive spawns, so the census
     * collection at the end keeps its standard shape (one thread the
     * stop-the-world stops); it runs only while nothing above it is
     * runnable. */
    g_worker_tid = rhu_thread_spawn(&gc_worker, (void*)o3, 4096u,
                                    GC_HOST_WORKER_PRIO,
                                    "gcmut");
    drive_check(g_worker_tid >= 1, "worker spawn refused");
    printf("[gc-host] worker spawned: tid=%d prio=" GC_HOST_STR(GC_HOST_WORKER_PRIO)
           " stack=4096 (runs only while kmain sleeps)\n",
           (int)g_worker_tid);
    {
        unsigned tries = 0;
        for (; tries < 100 && !g_worker_ready; tries++)
            rhu_sleep_ns(2000000ull);
    }
    drive_check(g_worker_ready == 1, "worker never published its stack slot");

    g_hd[0].obj = o1;
    g_hd[1].obj = o2;
    g_hd[0].tid = rhu_thread_spawn(&hd_worker, &g_hd[0], 4096u, HD_WORKER_PRIO, "hdwk0");
    g_hd[1].tid = rhu_thread_spawn(&hd_worker, &g_hd[1], 4096u, HD_WORKER_PRIO, "hdwk1");
    drive_check(g_hd[0].tid >= 1 && g_hd[1].tid >= 1, "handle worker spawn refused");
    printf("[gc-host] handle workers spawned: tid=%d and tid=%d prio=" GC_HOST_STR(HD_WORKER_PRIO)
           " stack=4096 (rotation at one level above the counter; the host parks until both finish)\n",
           (int)g_hd[0].tid, (int)g_hd[1].tid);
    for (;;) {
        uint32_t v = g_hd_done;
        if (v >= 2u)
            break;
        (void)rhu_futex_wait((const uint32_t*)&g_hd_done, v, RHU_DEADLINE_NONE);
    }
    for (unsigned k = 0; k < 2; k++) {
        HdWorker* w = &g_hd[k];
        printf("[gc-host] handle worker %u (tid %d): %u creates, %u destroys, %u aliases%s\n",
               k, (int)w->tid, w->creates, w->destroys, w->aliases,
               w->aliases ? "" : " (every handle read its own object, twice)");
        if (w->aliases)
            printf("[gc-host] handle worker %u: the first aliased handle %p read %p; its own object is %p\n",
                   k, (void*)w->first_alias, (void*)w->first_seen, (void*)w->obj);
        printf("[gc-host] handle worker %u ran %lu ms\n", k, (unsigned long)(w->ns / 1000000ull));
    }
    drive_check(g_hd[0].aliases == 0 && g_hd[1].aliases == 0,
                "a handle read another thread's object (the pool aliased a slot)");
    drive_check(g_hd[0].creates == MGC_HANDLES_ITER * HD_BURST
                && g_hd[1].creates == MGC_HANDLES_ITER * HD_BURST
                && g_hd[0].destroys == MGC_HANDLES_ITER * HD_BURST
                && g_hd[1].destroys == MGC_HANDLES_ITER * HD_BURST,
                "a worker's operation count is not the plan's");
    /* The pool's books after the storm: the census collection walks the
     * live slots, and a slot a lost free-list entry left marked live
     * shows there. */
    drive_check(heap->GarbageCollect(2, false, 0) == 0,
                "GarbageCollect rc != S_OK");
    printf("[gc-host] GC HANDLE POOL EXERCISE OK (two threads, %u pool operations at once, no slot aliased, the workers' books exact; the census above carries the pool's)\n",
           (unsigned)(4u * MGC_HANDLES_ITER * HD_BURST));
    return 0;
}
#endif /* MGC_HOST_DRIVE_HANDLES */

#ifdef MGC_HOST_DRIVE_NOGC
/* ------------------------------------------------------------------ */
/* the no-GC region drive (gate case gc.no-gc-region): the entry and   */
/* the end through the interface, each status read against the pinned */
/* enums -- an entry is refused with no_memory, never claimed, since   */
/* no budget exists to admit; an end answers not_in_progress.          */
/* ------------------------------------------------------------------ */

static int32_t drive_no_gc_region(IGCHeap* heap)
{
    printf("[gc-host] gc.no-gc-region drive: the no-GC region's entry and end through the interface, the statuses read against the pinned enums (a byte, a kilobyte, the heap)\n");
    static const uint64_t sizes[3] = { 1u, 1024u, 2097152u };
    for (int i = 0; i < 3; i++) {
        int s = heap->StartNoGCRegion(sizes[i], false, 0, i == 1);
        drive_check(s != (int)start_no_gc_success, "StartNoGCRegion claimed success with nothing entered");
        drive_check(s == (int)start_no_gc_no_memory, "StartNoGCRegion refused with a status other than no_memory");
        int e = heap->EndNoGCRegion();
        drive_check(e == (int)end_no_gc_not_in_progress, "EndNoGCRegion did not answer not_in_progress after a refused entry");
    }
    int e0 = heap->EndNoGCRegion();
    drive_check(e0 == (int)end_no_gc_not_in_progress, "a bare EndNoGCRegion did not answer not_in_progress");
    printf("[gc-host] GC NO-GC REGION EXERCISE OK (three entries refused with no_memory -- a byte, a kilobyte, the heap -- each end and a bare end answering not_in_progress; nothing claimed)\n");
    return 0;
}
#endif /* MGC_HOST_DRIVE_NOGC */

static int32_t drive_allocator(IGCHeap* heap)
{
#ifdef MGC_HOST_DRIVE_FREEPAD
    return drive_free_pad(heap);
#endif
#ifdef MGC_HOST_DRIVE_HANDLES
    return drive_handles(heap);
#endif
#ifdef MGC_HOST_DRIVE_NOGC
    return drive_no_gc_region(heap);
#endif
#if defined(MGC_HOST_DRIVE_TUNE)
    printf("[gc-host] gc.D windows+event drive: the same payload under window posture, the GC-done futex, and the activation break-out\n");
#elif defined(MGC_HOST_DRIVE_SWEEP)
    printf("[gc-host] gc.C-sweep death-pipeline drive: the schedule + a death cast, then collections that actually reclaim\n");
#elif defined(MGC_HOST_DRIVE_COLLECT)
    printf("[gc-host] gc.C suspension+mark drive: allocation schedule, then two collections over the real interface\n");
#else
    printf("[gc-host] gc.B allocator drive: exercising IGCHeap::Alloc over the live region\n");
#endif

    gc_alloc_context* acx = &g_mutator.acx;

    /* No pre-allocation probing here: on the bare skeleton the FIRST
     * interface call of this drive must be the allocation itself, so
     * the control image dies through the skeleton's own first-alloc
     * fail-fast and nothing else. The plug-skew fact rides the
     * post-schedule book check (116 = the +4 skew + 112 allocated). */
    Object* o1 = drive_alloc(heap, acx, 24, 0, 1);
#ifdef MGC_HOST_DRIVE_TUNE
    /* Under window posture the first slow serve opened a bump window
     * on the context; the second allocation is the EE fast path's --
     * and it lands at the window head, which IS first + 24. */
    Object* o2 = drive_alloc_win(acx, 28, 2);
#else
    Object* o2 = drive_alloc(heap, acx, 28, 0, 2);
#endif
    drive_check((unsigned char*)o2 == (unsigned char*)o1 + 24,
                "bump adjacency: the second object is not first + 24");
    printf("[gc-host] alloc #2 adjacent to #1: the bump advanced exactly 24 bytes\n");

    Object* o3 = drive_alloc(heap, acx, 16, GC_ALLOC_ALIGN8, 3);
    drive_check(((uintptr_t)o3 & 7u) == 0, "ALIGN8 result not 8-aligned");
    printf("[gc-host] alloc #3 ALIGN8: address parity 0 mod 8 OK (no pad needed at this point)\n");

    Object* o4 = drive_alloc(heap, acx, 20, GC_ALLOC_ALIGN8_BIAS, 4);
    drive_check(((uintptr_t)o4 & 7u) == 4, "ALIGN8_BIAS result not at 4 mod 8");
    drive_check((unsigned char*)o4 == (unsigned char*)o3 + 16 + 12,
                "parity flip did not cost exactly one 12-byte pad object");
    printf("[gc-host] alloc #4 ALIGN8_BIAS: address parity 4 mod 8 OK via one 12-byte parity pad\n");

    Object* o5 = drive_alloc(heap, acx, 12, GC_ALLOC_FINALIZE, 5);
    (void)o5;
    printf("[gc-host] alloc #5 GC_ALLOC_FINALIZE: registration is the allocator's duty (the census carries it)\n");

    /* containment answers must flip for a live-region object and stay
     * honest for a host static */
    drive_check(heap->WhichGeneration(o1) == 0, "a live object did not read as generation 0");
    drive_check(heap->IsHeapPointer((void*)o1, false), "a live object failed IsHeapPointer");
    drive_check(heap->WhichGeneration((Object*)(void*)&g_mutator) == 2147483647u,
                "a host static read as on-heap");
    drive_check(!heap->IsHeapPointer((void*)&g_mutator, false),
                "a host static passed IsHeapPointer");
    printf("[gc-host] containment: heap object -> gen 0 + IsHeapPointer true; host static -> not on heap\n");

#ifndef MGC_HOST_DRIVE_COLLECT
    drive_check(heap->GetTotalBytesInUse() == 116,
                "GetTotalBytesInUse after the schedule is not 116");
    drive_check(heap->GetTotalAllocatedBytes() == 112,
                "GetTotalAllocatedBytes after the schedule is not 112");
    drive_check(acx->alloc_count == 5, "context alloc_count is not 5");
    drive_check(acx->alloc_bytes == 100, "context alloc_bytes is not 100");
    printf("[gc-host] context books: alloc_count=5 alloc_bytes=100; heap books: in_use=116 allocated=112 OK\n");

    /* The wall, honestly and LAST (it latches the plain bump allocator):
     * an oversize ask must come back NULL -- the managed
     * OutOfMemoryException protocol -- never a quiet wrong answer, and
     * every later ask stays NULL. */
    drive_check(heap->Alloc(acx, 8u * 1024u * 1024u, 0) == nullptr,
                "an oversize allocation did not return NULL");
    drive_check(heap->Alloc(acx, 24, 0) == nullptr,
                "a post-wall allocation did not return NULL");
    printf("[gc-host] wall probe: oversize Alloc -> NULL OK; post-wall Alloc -> NULL OK\n");

    printf("[gc-host] GC ALLOCATOR EXERCISE OK (5 allocations zero-born on the live region; honest NULL at the wall)\n");
    return 0;
#else
    /* The extended schedule: a traced-edge source and a dependent
     * secondary. alloc #4 is deliberately reachable ONLY through the
     * pair object's ref field (the tracer must walk a real GCDesc
     * series to find it); alloc #7 ONLY through the dependent
     * handle's fixpoint. NO wall probe in this world: under a
     * mark-only collector, region exhaustion is the collector's LOUD
     * fail-fast -- the NULL protocol stays the allocator drive's
     * witness. */
    pair_mt_init();
    Object* o6 = drive_alloc_pair(heap, acx, o4, 6);
    g_root_pair = o6;
#ifdef MGC_HOST_DRIVE_TUNE
    Object* o7 = drive_alloc_win(acx, 12, 7);
#else
    Object* o7 = drive_alloc(heap, acx, 12, 0, 7);
#endif

#ifndef MGC_HOST_DRIVE_TUNE
    drive_check(heap->GetTotalBytesInUse() == 144,
                "GetTotalBytesInUse after the schedule is not 144");
#ifdef MGC_HOST_DRIVE_SWEEP
    /* The free-list allocator accounts pads fit-internally: the
     * allocated-bytes census carries OBJECT bytes only (128), where
     * the plain bump path charges the pad too (140). */
    drive_check(heap->GetTotalAllocatedBytes() == 128,
                "GetTotalAllocatedBytes after the schedule is not 128");
#else
    drive_check(heap->GetTotalAllocatedBytes() == 140,
                "GetTotalAllocatedBytes after the schedule is not 140");
#endif
    drive_check(acx->alloc_count == 7, "context alloc_count is not 7");
    drive_check(acx->alloc_bytes == 128, "context alloc_bytes is not 128");
#ifdef MGC_HOST_DRIVE_SWEEP
    printf("[gc-host] context books: alloc_count=7 alloc_bytes=128; heap books: in_use=144 allocated=128 (pads fit-internal under the sweep allocator) OK\n");
#else
    printf("[gc-host] context books: alloc_count=7 alloc_bytes=128; heap books: in_use=144 allocated=140 OK\n");
#endif
#endif

    /* The root world: statics, the frozen segment (edge -> alloc #2),
     * the skeleton's real handle store (strong #3; weak-long #5 --
     * skipped, not nulled, under a mark-only collector; dependent
     * #3 -> #7), then the worker with alloc #2 parked on its stack. */
    g_root_static = o1;
    g_root_null   = nullptr;
    g_root_oob    = (Object*)(void*)&g_mutator;
    frozen_build_and_register(heap, o2);

    IGCHandleStore* store = g_handle_mgr->GetGlobalHandleStore();
    drive_check(store != nullptr, "global handle store missing at the drive");
    OBJECTHANDLE hs = store->CreateHandleOfType(o3, HNDTYPE_STRONG);
    OBJECTHANDLE hw = store->CreateHandleOfType(o5, HNDTYPE_WEAK_LONG);
    OBJECTHANDLE hd = store->CreateDependentHandle(o3, o7);
    drive_check(hs != nullptr && hw != nullptr && hd != nullptr,
                "handle creation failed");
    drive_check(*(Object**)hs == o3,
                "a handle is not a real dereferenceable slot holding its object");
    printf("[gc-host] handles: strong(#3) weak-long(#5) dependent(#3 -> #7) created; strong slot dereferences to alloc #3\n");

#ifdef MGC_HOST_DRIVE_SWEEP
    /* --- the death cast: one specimen per partition branch and per
     * weak-nulling class. Fates are the gate's census pins; nothing
     * here asserts a death (the BREAK_WEAK control runs this same
     * code and must exit 0 -- weak visibility is printed, below). --- */
    g_crit_block.mt.m_flags        = MTFlag_HasCriticalFinalizer;
    g_crit_block.mt.m_baseSize     = 16;
    g_crit_block.mt.m_pRelatedType = nullptr;
    g_eager_block.mt.m_flags        = 0;
    g_eager_block.mt.m_baseSize     = 16;
    g_eager_block.mt.m_pRelatedType = nullptr;
    printf("[gc-host] crit MT built (HasCriticalFinalizer, baseSize=16); WR-mirror MT built (identity-typed, baseSize=16)\n");

#ifdef MGC_HOST_DRIVE_TUNE
    Object* o8 = drive_alloc_win(acx, 24, 8);
#else
    Object* o8 = drive_alloc(heap, acx, 24, 0, 8);
#endif
    Object* o9  = drive_alloc(heap, acx, 16, GC_ALLOC_FINALIZE, 9);
    Object* o10 = drive_alloc(heap, acx, 16, GC_ALLOC_FINALIZE, 10);
    heap->SetFinalizationRun(o10);
    printf("[gc-host] alloc #10 suppressed pre-death (SetFinalizationRun sets the FINALIZER_RUN header bit)\n");
    Object* o11 = drive_alloc_mt(heap, acx, &g_crit_block.mt, "critical-finalizer", 11);
    Object* o12 = drive_alloc_mt(heap, acx, &g_eager_block.mt, "WR-mirror", 12);
    (void)o12;
    Object* o13 = drive_alloc(heap, acx, 20, 0, 13);
#ifdef MGC_HOST_DRIVE_TUNE
    Object* o14 = drive_alloc_win(acx, 24, 14);
    Object* o15 = drive_alloc_win(acx, 16, 15);
    Object* o16 = drive_alloc_win(acx, 12, 16);
#else
    Object* o14 = drive_alloc(heap, acx, 24, 0, 14);
    Object* o15 = drive_alloc(heap, acx, 16, 0, 15);
    Object* o16 = drive_alloc(heap, acx, 12, 0, 16);
#endif
    Object* o17 = drive_alloc(heap, acx, 16, GC_ALLOC_FINALIZE, 17);
    g_root_fin = o17;
    g_root_big = nullptr;

    g_h_und = store->CreateHandleOfType(o1, HNDTYPE_WEAK_SHORT);
    OBJECTHANDLE h_ws   = store->CreateHandleOfType(o13, HNDTYPE_WEAK_SHORT);
    OBJECTHANDLE h_wl2  = store->CreateHandleOfType(o14, HNDTYPE_WEAK_LONG);
    OBJECTHANDLE h_dep2 = store->CreateDependentHandle(o15, o16);
    drive_check(g_h_und != nullptr && h_ws != nullptr && h_wl2 != nullptr
                && h_dep2 != nullptr, "cast handle creation failed");
    printf("[gc-host] cast handles: underlying-ws(#1) ws(#13) wl(#14) dependent(#15 -> #16) created\n");

#ifdef MGC_HOST_DRIVE_TUNE
    /* Printed, not asserted: the BREAK_WINDOW_FIX control suppresses
     * the fix-time refunds, so these numbers are exactly where that
     * suppression first shows -- the gate pins the positive values and
     * lets the control walk on to its own detector. */
    printf("[gc-host] posture books: alloc_count=%d alloc_bytes=%d; in_use=%u allocated=%u (window charges minus refunds = the fast-born bytes)\n",
           (int)acx->alloc_count, (int)acx->alloc_bytes,
           (unsigned)heap->GetTotalBytesInUse(),
           (unsigned)heap->GetTotalAllocatedBytes());
#endif
#endif /* MGC_HOST_DRIVE_SWEEP */

    g_worker_tid = rhu_thread_spawn(&gc_worker, (void*)o2, 4096u,
                                    GC_HOST_WORKER_PRIO,
                                    "gcmut");
    drive_check(g_worker_tid >= 1, "worker spawn refused");
    printf("[gc-host] worker spawned: tid=%d prio=" GC_HOST_STR(GC_HOST_WORKER_PRIO)
           " stack=4096 (runs only while kmain sleeps)\n",
           (int)g_worker_tid);
    {
        unsigned tries = 0;
        for (; tries < 100 && !g_worker_ready; tries++)
            rhu_sleep_ns(2000000ull);
    }
    drive_check(g_worker_ready == 1, "worker never published its stack slot");
    drive_check(*g_worker_slot == o2,
                "the worker stack slot does not hold alloc #2");
    printf("[gc-host] worker ready: stack slot published (alloc #2 parked on the worker stack)\n");

#ifndef MGC_HOST_DRIVE_SWEEP
    /* Two collections through the interface: the second proves the
     * mark bitmap resets and the suspend/resume round trip repeats
     * with identical census numbers. */
    drive_check(heap->GarbageCollect(2, false, 0) == 0,
                "GarbageCollect #1 rc != S_OK");
    drive_check(heap->CollectionCount(2, 0) == 1,
                "CollectionCount after #1 is not 1");
    drive_check(heap->GarbageCollect(2, false, 0) == 0,
                "GarbageCollect #2 rc != S_OK");
    drive_check(heap->GetGcCount() == 2, "GetGcCount after #2 is not 2");

    printf("[gc-host] GC SUSPENSION+MARK EXERCISE OK (worker suspended/resumed x2 over the kernel verbs; census exact x2; 7 marked of 8 walked -- the pad stayed unmarked)\n");
    return 0;
#else /* MGC_HOST_DRIVE_SWEEP */

#ifdef MGC_HOST_DRIVE_TUNE
    /* One live window across the collection so the enum-time fix site
     * fires (retired inside the world-stopped bracket, zero births --
     * any remainder is pluggable), then the waiter's first epoch: it
     * parks INSIDE SuspendEE's frozen-check window (the event is
     * armed by then) and RestartEE's Set+wake releases it. */
    Object* o18t = drive_alloc(heap, acx, 16, 0, 18);
    (void)o18t;
    drive_check(acx->alloc_ptr != nullptr,
                "no window outstanding before the collection (posture drift)");
    drive_check(rhu_activation_register(&gc_activation_handler) == 0,
                "activation handler registration refused");
    g_heap_ee = heap;
    g_waiter_tid = rhu_thread_spawn(&gc_waiter, nullptr, 4096u,
                                    GC_HOST_WAITER_PRIO,
                                    "gcwait");
    drive_check(g_waiter_tid >= 1, "waiter spawn refused");
    printf("[gc-host] waiter spawned: tid=%d prio=" GC_HOST_STR(GC_HOST_WAITER_PRIO)
           " stack=4096 (parks on a go futex between epochs)\n",
           (int)g_waiter_tid);
    rhu_sleep_ns(2000000ull);   /* a window: the waiter parks on the go word */
    g_waiter_go = 1;
    drive_check(rhu_futex_wake((const uint32_t*)&g_waiter_go, 1) == 1,
                "the waiter was not futex-blocked on the go word");
    printf("[gc-host] epoch signal: woken 1 waiter (it was genuinely futex-blocked)\n");
#endif

    /* Collection #1: the death pipeline runs for the first time. */
    drive_check(heap->GarbageCollect(2, false, 0) == 0,
                "GarbageCollect #1 rc != S_OK");
    drive_check(heap->CollectionCount(2, 0) == 1,
                "CollectionCount after #1 is not 1");
#ifdef MGC_HOST_DRIVE_TUNE
    drive_check(g_waiter_done == 1,
                "the waiter did not complete epoch 1 (RestartEE's Set+wake)");
#endif

    /* Weak visibility after #1 -- PRINTED, the gate is the detector
     * (the BREAK_WEAK control runs this same code and must exit 0). */
    printf("[gc-host] weak probe ws(#13): %s\n",
           *(Object**)h_ws ? "slot STALE non-null (dead=False)"
                           : "slot NULL (dead=True)");
    printf("[gc-host] weak probe wl(#14): %s\n",
           *(Object**)h_wl2 ? "slot STALE non-null (dead=False)"
                            : "slot NULL (dead=True)");
    printf("[gc-host] dependent probe (#15 -> #16): primary %s\n",
           *(Object**)h_dep2 ? "STALE non-null (dead=False)"
                             : "NULL (cleared)");
    drive_check(*(Object**)hd == o3, "the live dependent's primary slot moved");
    drive_check(*(Object**)hw == o5,
                "the resurrected finalizable's weak-long slot did not survive #1");
    printf("[gc-host] weak-long(#5) SURVIVED collection #1 (promoted by the partition -- long weaks track resurrection)\n");

    /* The finalizer thread's drain: normals before the critical, with
     * the FINALIZER_RUN bit honored EE-side (clear + skip) on a
     * specimen suppressed AFTER promotion. */
    drive_check(heap->GetNumberOfFinalizable() == 3,
                "f-reachable count after #1 is not 3");
    heap->SetFinalizationRun(o9);
    {
        unsigned ran = 0, skipped = 0;
        for (;;) {
            Object* f = heap->GetNextFinalizable();
            if (!f)
                break;
            ObjHeader* hh = ((ObjHeader*)f) - 1;
            if (hh->GetBits() & BIT_SBLK_FINALIZER_RUN) {
                hh->ClrBit(BIT_SBLK_FINALIZER_RUN);
                drive_check(f == o9, "the skipped pop is not the FINALIZER_RUN specimen");
                skipped++;
                printf("[gc-host] drain: FINALIZER_RUN honored EE-side -- popped and SKIPPED (bit cleared)\n");
                continue;
            }
            ran++;
            if (ran == 1) {
                drive_check(f == o5, "first drained finalizable is not the resurrected #5");
                printf("[gc-host] drain: finalizer ran for #5 (normals first)\n");
            } else {
                drive_check(f == o11, "second-run finalizable is not the critical #11");
                drive_check(skipped == 1, "the critical drained before every normal was popped");
                printf("[gc-host] drain: finalizer ran for #11 (the critical class drains LAST)\n");
            }
        }
        drive_check(ran == 2 && skipped == 1, "drain census is not 2 ran + 1 skipped");
    }
    drive_check(heap->GetNumberOfFinalizable() == 0,
                "f-reachable count after the drain is not 0");

    /* Collection #2: the drained trio dies for real; the resurrected
     * #5's weak-long slot goes the way of the dead this time. */
    drive_check(heap->GarbageCollect(2, false, 0) == 0,
                "GarbageCollect #2 rc != S_OK");
    printf("[gc-host] weak probe wl(#5) after #2: %s\n",
           *(Object**)hw ? "slot STALE non-null (dead=False)"
                         : "slot NULL (dead=True)");

#ifdef MGC_HOST_DRIVE_TUNE
    /* Slot reuse at the drive level: the eager branch destroyed the
     * underlying handle during #1's partition; the very next create
     * must be handed that released slot (handles are slot pointers,
     * so address equality IS the reuse proof -- per-create logs are
     * throttled behind the init self-test's 200). */
    {
        OBJECTHANDLE h_new = store->CreateHandleOfType(o1, HNDTYPE_STRONG);
        drive_check(h_new == g_h_und,
                    "the create after the eager destroy did not reuse the released slot");
        printf("[gc-host] slot reuse: the post-destroy create landed on the released slot (handle address identical)\n");
    }
#endif

#ifndef MGC_HOST_DRIVE_TUNE
    /* --- the sweep world's tail: reclaim, the retired wall, the
     * honest-NULL OOM protocol and the grace round trip --- */

    /* Reuse: first-fit must land at the first fitting dead run --
     * alloc #8's address (the 40-byte coalesced run), split serve. */
    Object* o18 = drive_alloc_reuse(heap, acx, 16, 0, 18);
    drive_check((void*)o18 == (void*)o8,
                "reuse did not land at the first fitting run (address-ordered first-fit)");
    printf("[gc-host] reuse landed at alloc #8's old address (first-fit, split)\n");

    /* The retired wall: a rooted 1.9 MB object is marked through a
     * collection, dropped, and the SAME ask that fail-fasts a
     * mark-only build now collects-and-retries to an exact-fit serve
     * at the dead object's own address. */
    Object* bigA = drive_alloc(heap, acx, 1900000, 0, 19);
    g_root_big = bigA;
    drive_check(heap->GarbageCollect(2, false, 0) == 0,
                "GarbageCollect #3 rc != S_OK");
    g_root_big = nullptr;
    Object* bigB = drive_alloc_reuse(heap, acx, 1900000, 0, 20);
    drive_check((void*)bigB == (void*)bigA,
                "the wall retry did not serve the freed big run exact-fit");
    printf("[gc-host] THE MARK-ONLY WALL IS RETIRED: the 1.9 MB ask collected-and-retried to an exact-fit serve at the dead object's address\n");

    /* The honest NULL: an unfittable ask exhausts the bounded retries
     * and reports OOM with the reserve intact (grace opens); a small
     * allocation still succeeds; the next sweep closes grace. */
    drive_check(heap->Alloc(acx, 4u * 1024u * 1024u, 0) == nullptr,
                "an unfittable ask did not return NULL through the retry path");
    printf("[gc-host] oversize ask -> NULL after bounded retries (managed protocol; grace open)\n");
    Object* o21 = drive_alloc_reuse(heap, acx, 24, 0, 21);
    (void)o21;
    drive_check(heap->GarbageCollect(2, false, 0) == 0,
                "GarbageCollect #9 rc != S_OK");
    drive_check(heap->GetGcCount() == 9, "GetGcCount at the drive's end is not 9");
    /* GetTotalBytesInUse under the free-list allocator answers the bump
     * extent minus the free list -- GC.GetTotalMemory honesty: 132 B of
     * surviving objects + the 4-byte region skew + the two dead spaces
     * of exactly 12 bytes this schedule leaves between live objects,
     * which the sweep keeps as pads (walkable, never on the list: a
     * 12-byte run has no room for its link). */
    drive_check(heap->GetTotalBytesInUse() == 160, "final in_use is not 160 (live bytes + two 12-byte pads)");
    drive_check(heap->GetTotalAllocatedBytes() == 3800344, "final allocated is not 3800344");
    drive_check(acx->alloc_count == 21, "final alloc_count is not 21");
    drive_check(acx->alloc_bytes == 3800344, "final alloc_bytes is not 3800344");
    printf("[gc-host] final books: alloc_count=21 alloc_bytes=3800344; in_use=160 (132 live + 4 skew + 2 pads x 12) allocated=3800344 OK\n");

    printf("[gc-host] GC DEATH-PIPELINE EXERCISE OK (four partition branches + three weak classes + resurrection; drain normals-then-critical with FINALIZER_RUN honored; zero-on-reuse; the mark-only wall retired; honest NULL + grace round trip)\n");
    return 0;
#else /* MGC_HOST_DRIVE_TUNE */
    /* --- the tune world's tail: the EINTR epoch, the free-run
     * window, the detach fix site, and the enum fix closing it --- */

    /* Epoch 2: the event is armed BY HAND (the EE's own verb), the
     * waiter genuinely blocks on it, and an ACTIVATION -- not a wake --
     * brings it back: the kernel futex contract's RHU_EINTR path. */
    heap->ResetWaitForGCEvent();
    g_waiter_go = 2;
    drive_check(rhu_futex_wake((const uint32_t*)&g_waiter_go, 1) == 1,
                "the waiter was not parked on the go word for epoch 2");
    rhu_sleep_ns(2000000ull);   /* the waiter parks on the armed event */
    drive_check(g_waiter_done == 1,
                "the waiter returned without any wake (it never blocked)");
    drive_check(rhu_activation_send(g_waiter_tid) == 0,
                "activation send refused the blocked waiter");
    printf("[gc-host] activation sent to the futex-blocked waiter (tid=%d)\n",
           (int)g_waiter_tid);
    rhu_sleep_ns(2000000ull);   /* delivery, then the broken wait returns */
    drive_check(g_waiter_done == 2, "the waiter did not return from the broken wait");
    drive_check(g_activations == 1, "the activation handler did not run exactly once");
    printf("[gc-host] EINTR by elimination: no Set/wake between parking and return, the event was armed before parking (no EAGAIN), no deadline -- the wait returned through the RHU_EINTR contract after the handler ran\n");
    heap->SetWaitForGCEvent();  /* restore; nobody sleeps, the wake readies nothing */

    /* A window from a FREE RUN (adaptive whole-run, zeroed at
     * handout) and its fast consumption. */
    Object* o19t = drive_alloc(heap, acx, 16, 0, 19);
    drive_check((unsigned char*)o19t == (unsigned char*)o2 + 28,
                "the free-run window did not land at the first fitting run (alloc #2's end)");
    printf("[gc-host] alloc #19 served from a FREE-RUN window (adaptive size, zeroed at handout)\n");
    Object* o20t = drive_alloc_win(acx, 24, 20);
    (void)o20t;

    /* The detach fix site: one window on the detach-probe context,
     * retired through FixAllocContext as a dying thread's would be. */
    g_cur_acx = &g_detach_mutator.acx;
    Object* o21t = drive_alloc(heap, &g_detach_mutator.acx, 16, 0, 21);
    g_cur_acx = &g_mutator.acx;
    drive_check((void*)o21t == (void*)o8,
                "the detach window did not land at the next fitting run (alloc #8's address)");
    heap->FixAllocContext(&g_detach_mutator.acx, nullptr, nullptr);
    drive_check(g_detach_mutator.acx.alloc_ptr == nullptr
                && g_detach_mutator.acx.alloc_limit == nullptr,
                "FixAllocContext did not clear the detach context");
    printf("[gc-host] detach probe: FixAllocContext retired the context's window (zero births counted, remainder plugged)\n");

    /* Collection #3: the enum fix retires kmain's outstanding window
     * with its one fast birth counted into the walk model. */
    drive_check(heap->GarbageCollect(2, false, 0) == 0,
                "GarbageCollect #3 rc != S_OK");
    drive_check(heap->GetGcCount() == 3, "GetGcCount at the drive's end is not 3");
    /* the same two 12-byte dead spaces as the sweep world's: pads, in use */
    drive_check(heap->GetTotalBytesInUse() == 160, "final in_use is not 160 (live bytes + two 12-byte pads)");
    drive_check(heap->GetTotalAllocatedBytes() == 376, "final allocated is not 376");
    drive_check(acx->alloc_count == 13, "final kmain alloc_count is not 13");
    drive_check(acx->alloc_bytes == 360, "final kmain alloc_bytes is not 360");
    drive_check(g_detach_mutator.acx.alloc_count == 1,
                "the detach context's alloc_count is not 1");
    printf("[gc-host] final books: kmain alloc_count=13 alloc_bytes=360, detach alloc_count=1; in_use=160 (132 live + 4 skew + 2 pads x 12) allocated=376 OK\n");

    printf("[gc-host] GC WINDOWS+EVENT EXERCISE OK (windows both sources, three fix sites, refunds exact; the GC-done event slept and woke over the kernel futex; an activation broke a blocked wait -- the RHU_EINTR contract; handle slots reused)\n");
    return 0;
#endif /* MGC_HOST_DRIVE_TUNE */
#endif /* MGC_HOST_DRIVE_SWEEP */
#endif /* MGC_HOST_DRIVE_COLLECT */
}
#endif /* MGC_HOST_DRIVE_ALLOC */

extern "C" int32_t __managed__Main(int32_t argc, const uint8_t* const* argv)
{
    (void)argc;
    (void)argv;

    /* Formatter self-test, pinned byte-exact by the gate: every
     * supported conversion with fixed values. */
    printf("[gc-host] fmt d=%d u=%u x=%x lx=%08lx s=%s p=%p c=%c pct=%%\n",
           -42, 3000000000u, 0xdeadbeefu, 0x123abcul, "ok", (void*)0x20000000, 'Q');

#ifdef MGC_HOST_ROW_AN500
    /* The board row, declared before the activation it configures.
     * These are the compiled constants themselves; the knob ties are
     * on the wire (the cover line's buffer total, the census region
     * size, the handle census's slot count). */
    printf("[gc-row] board=%s profile=%s basis=%s region=%u cover=%u kheap=%u stack=%u/%u handles=%u foh=%u\n",
           MGC_ROW_BOARD, MGC_ROW_PROFILE, MGC_ROW_BASIS,
           (unsigned)MGC_ROW_REGION_BYTES, (unsigned)MGC_ROW_COVER_BYTES,
           (unsigned)MGC_ROW_KHEAP_BYTES, (unsigned)MGC_ROW_STACK_DEFAULT,
           (unsigned)MGC_ROW_STACK_CLAMP, (unsigned)MGC_ROW_HANDLE_SLOTS,
           (unsigned)MGC_ROW_FOH_POOL_BYTES);
    printf("[gc-row] sizing: markstack=%u finq=%u reserve=%u window=%u/%u retries=%u frozen=%u\n",
           (unsigned)MGC_ROW_MARKSTACK_ENTRIES, (unsigned)MGC_ROW_FINQ_CAP,
           (unsigned)MGC_ROW_OOM_RESERVE, (unsigned)MGC_ROW_WINDOW_BYTES,
           (unsigned)MGC_ROW_WINDOW_FLOOR, (unsigned)MGC_ROW_WALL_RETRIES,
           (unsigned)MGC_ROW_FROZEN_SEGS);
#endif

    printf("[gc-host] gc.A activation host: driving the standalone-GC loader protocol\n");

    g_free_mt.InitializeFreeObject();

    void* h = __mgc_dlopen_hook("rhu-mgc");
    if (!h) {
        sh_write0("[gc-host] dlopen costume returned no handle\n");
        abort();
    }
    GC_VersionInfoFunction version_fn =
        (GC_VersionInfoFunction)__mgc_dlsym_hook(h, "GC_VersionInfo");
    GC_InitializeFunction init_fn =
        (GC_InitializeFunction)__mgc_dlsym_hook(h, "GC_Initialize");
    if (!version_fn || !init_fn) {
        sh_write0("[gc-host] loader protocol entry point missing\n");
        abort();
    }

    /* The handshake. The struct is an out-parameter; a careful caller
     * hands it zeroed, so the skeleton's ENTER diagnostic line is
     * deterministic. The acceptance check compares against the SAME
     * vendored-header constants the skeleton reports from -- one truth
     * source, and the gate's gc.vendor case anchors those constants to
     * the recorded interface tuple independently. */
    VersionInfo v;
    v.MajorVersion = 0;
    v.MinorVersion = 0;
    v.BuildVersion = 0;
    v.Name = nullptr;
    version_fn(&v);
    if (v.MajorVersion != GC_INTERFACE_MAJOR_VERSION
        || v.MinorVersion < GC_INTERFACE_MINOR_VERSION) {
        printf("[gc-host] GC ACTIVATION REFUSED: reported {%u,%u}, this EE requires major %u minor >= %u\n",
               (unsigned)v.MajorVersion, (unsigned)v.MinorVersion,
               (unsigned)GC_INTERFACE_MAJOR_VERSION,
               (unsigned)GC_INTERFACE_MINOR_VERSION);
        return 3;
    }
    printf("[gc-host] handshake accepted: GC {%u,%u} == the pinned header constants\n",
           (unsigned)v.MajorVersion, (unsigned)v.MinorVersion);

    /* GC_Initialize. The EE-side interface pointer is non-null by
     * contract; nothing is ever called through it here (the flat
     * statics above are the route), so a cookie address satisfies the
     * contract honestly. */
    static int clr_cookie;
    IGCHeap* heap = nullptr;
    IGCHandleManager* mgr = nullptr;
    GcDacVars dac;
    HRESULT rc = init_fn((IGCToCLR*)(void*)&clr_cookie, &heap, &mgr, &dac);
    if (rc != 0 || !heap || !mgr) {
        printf("[gc-host] GC_Initialize failed (rc=%d heap=%p mgr=%p)\n",
               (int)rc, (void*)heap, (void*)mgr);
        abort();
    }

    /* The first init-time vtable calls, in the EE's order: heap init,
     * handle-manager init, the global handle store. */
    rc = heap->Initialize();
    if (rc != 0) {
        printf("[gc-host] IGCHeap::Initialize rc=%d\n", (int)rc);
        abort();
    }
    if (!mgr->Initialize()) {
        sh_write0("[gc-host] IGCHandleManager::Initialize returned false\n");
        abort();
    }
    if (!mgr->GetGlobalHandleStore()) {
        sh_write0("[gc-host] global handle store missing\n");
        abort();
    }

#ifdef MGC_HOST_ROW_AN500
    /* The row's in-artifact verdicts, after init (the arena draws are
     * complete) and before the drive. Both refuse arms die loudly --
     * the same abort class as every structurally broken path.
     *
     * Containment: the cover granule is anchored by masking any
     * covered address down to the cover's power-of-two boundary (the
     * heap's guard masks the region base; the statics window and the
     * region are served from the same DATA window, so masking the
     * statics base lands on the same floor whenever the verdict
     * holds -- the gate cross-ties the two printed floors). Every
     * legal bulk-move destination -- the statics window and the
     * whole arena window any heap range is served from -- must sit
     * inside the granule, or the cover is a lie and this image
     * refuses to exist. */
    {
        uintptr_t floor_ = (uintptr_t)__sdata
            & ~((uintptr_t)MGC_ROW_COVER_BYTES - 1u);
        uintptr_t cend = floor_ + (uintptr_t)MGC_ROW_COVER_BYTES;
        printf("[gc-row] containment: floor=0x%08lx cover_end=0x%08lx statics=[0x%08lx,0x%08lx) arena=[0x%08lx,0x%08lx)\n",
               (unsigned long)floor_, (unsigned long)cend,
               (unsigned long)(uintptr_t)__sdata,
               (unsigned long)(uintptr_t)__ebss,
               (unsigned long)(uintptr_t)__rhu_arena_lo,
               (unsigned long)(uintptr_t)__rhu_arena_hi);
        if ((uintptr_t)__edata > cend || (uintptr_t)__ebss > cend
            || (uintptr_t)__sbss < floor_
            || (uintptr_t)__rhu_arena_lo < floor_
            || (uintptr_t)__rhu_arena_hi > cend) {
            printf("[gc-row] containment REFUSED: a legal bulk-move destination lies outside the cover granule\n");
            abort();
        }
        printf("[gc-row] containment VERDICT: INSIDE (statics + arena window inside the cover granule -- every legal bulk-move destination covered)\n");

        unsigned long win = (unsigned long)
            ((uintptr_t)__rhu_arena_hi - (uintptr_t)__rhu_arena_lo);
        printf("[gc-row] window fit: drawn=%lu kheap=%lu window=%lu\n",
               g_row_drawn, (unsigned long)MGC_ROW_KHEAP_BYTES, win);
        if (g_row_drawn + (unsigned long)MGC_ROW_KHEAP_BYTES > win) {
            printf("[gc-row] window REFUSED: the row's region + aux + kheap overflow the link window\n");
            abort();
        }
        printf("[gc-row] window VERDICT: FITS (row region + aux + kheap inside the link window)\n");
    }
#endif

#ifdef MGC_HOST_DRIVE_ALLOC
#ifdef MGC_HOST_DRIVE_COLLECT
    g_handle_mgr = mgr;
#endif
    printf("[gc-host] GC ACTIVATION OK (heap + handle manager live; the allocator drive follows)\n");
    return drive_allocator(heap);
#else
    printf("[gc-host] GC ACTIVATION OK (heap + handle manager live; first allocation deliberately not attempted)\n");
    return 0;
#endif
}
