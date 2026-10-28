/* The glibc-personality core of the hosting shim: the C-runtime surface
 * the .NET runtime archives import, defined under the archives' own import
 * names on the kernel's hosting verbs. There is no C library underneath to
 * collide with, so nothing is renamed. errno lives in a per-thread block
 * held by the kernel's module registry; memory mapping is the kernel
 * arena; time is the kernel clock; locks are futex recipes held inline in
 * the glibc-sized slots; the 64-bit atomic operations the runtime's
 * compiler names as library calls hold one such lock; threads are kernel
 * threads at one flat hosted
 * priority; thread-local storage is the kernel's per-module block; and
 * everything this platform does not have answers with the errno a caller
 * can act on and a diagnostic line, never a silent success.
 *
 * Two families of constants appear below. The glibc ABI facts (slot
 * sizes, clock ids, the sysconf and mmap flag numbers, the syscall number
 * for the thread id, the structure layouts) belong to the glibc arm32
 * personality and do not change with the runtime's version. The runtime-side
 * facts (errno identities, the board row's values) come from the generated
 * PAL headers and the board rows header and change with theirs. */
#include <stdarg.h>
#include <stddef.h>
#include <stdint.h>

#include "rhu_shim.h"

#include "mgc_board_rows.h"
#include "rhu.h"
#include "rhu_glibc_arm32.h"
#include "rhu_pal_errno_net11_rc2.h"

void *memcpy(void *dst, const void *src, size_t n);
void *memset(void *dst, int c, size_t n);
size_t strlen(const char *s);
int strncmp(const char *a, const char *b, size_t n);

/* ------------------------------------------------------------------ */
/* the diagnostic line writer                                          */
/* ------------------------------------------------------------------ */

#define LOG_LINE_BYTES 256u

/* A formatting sink: the caller's buffer and its capacity (the terminator
 * included; 0 measures without writing), and the count of bytes the text
 * takes whether written or not -- the vsnprintf contract the PAL's
 * formatters state. The diagnostic line is one sink over its own line. */
typedef struct fmt_sink {
    char *buf;
    uint32_t cap;
    uint32_t n;
} fmt_sink;

static void line_put(fmt_sink *l, char c)
{
    if (l->cap != 0 && l->n < l->cap - 1u)
        l->buf[l->n] = c;
    l->n++;
}

static void line_puts(fmt_sink *l, const char *s)
{
    while (*s != 0)
        line_put(l, *s++);
}

/* An unsigned value in base 10 or 16, at least `width` digits, zero-padded
 * when `pad0`; lowercase hex digits. */
static void line_num(fmt_sink *l, unsigned long long v, unsigned base, unsigned width, int pad0)
{
    char tmp[24];
    unsigned n = 0;
    do {
        unsigned d = (unsigned)(v % base);
        tmp[n++] = (char)(d < 10u ? '0' + d : 'a' + (d - 10u));
        v /= base;
    } while (v != 0 && n < sizeof tmp);
    while (n < width && n < sizeof tmp)
        tmp[n++] = pad0 ? '0' : ' ';
    while (n != 0)
        line_put(l, tmp[--n]);
}

static void line_format(fmt_sink *l, const char *fmt, va_list ap)
{
    for (const char *p = fmt; *p != 0; p++) {
        if (*p != '%') {
            line_put(l, *p);
            continue;
        }
        p++;
        int pad0 = 0;
        unsigned width = 0;
        if (*p == '0') {
            pad0 = 1;
            p++;
        }
        while (*p >= '0' && *p <= '9') {
            width = width * 10u + (unsigned)(*p - '0');
            p++;
        }
        int longs = 0;
        while (*p == 'l') {
            longs++;
            p++;
        }
        switch (*p) {
        case '%':
            line_put(l, '%');
            break;
        case 'c':
            line_put(l, (char)va_arg(ap, int));
            break;
        case 's': {
            const char *s = va_arg(ap, const char *);
            line_puts(l, s != NULL ? s : "(null)");
            break;
        }
        case 'd':
        case 'i': {
            long long v = longs >= 2 ? va_arg(ap, long long)
                        : longs == 1 ? (long long)va_arg(ap, long)
                                     : (long long)va_arg(ap, int);
            if (v < 0) {
                line_put(l, '-');
                line_num(l, (unsigned long long)(-v), 10u, width, pad0);
            } else {
                line_num(l, (unsigned long long)v, 10u, width, pad0);
            }
            break;
        }
        case 'u':
        case 'x': {
            unsigned long long v = longs >= 2 ? va_arg(ap, unsigned long long)
                                 : longs == 1 ? (unsigned long long)va_arg(ap, unsigned long)
                                              : (unsigned long long)va_arg(ap, unsigned);
            line_num(l, v, *p == 'x' ? 16u : 10u, width, pad0);
            break;
        }
        case 'p': {
            void *v = va_arg(ap, void *);
            line_puts(l, "0x");
            line_num(l, (unsigned long long)(uintptr_t)v, 16u, 8u, 1);
            break;
        }
        default: {
            const char *msg = "[shim] diagnostic line: unsupported conversion in ";
            rhu_log_write(RHU_LOG_SILENT, msg, (uint32_t)strlen(msg));
            rhu_log_write(RHU_LOG_SILENT, fmt, (uint32_t)strlen(fmt));
            rhu_log_write(RHU_LOG_SILENT, "\n", 1u);
            rhu_abort(134);
        }
        }
    }
}

/* One diagnostic line of this layer on the kernel's LOG stream at a level
 * (the log-door stage): "[shim] ", the formatted text and the newline as
 * ONE write through rhu_log_write, which prints the line when the image's
 * output level reaches the line's and drops it otherwise. */
static void shim_line(uint32_t level, const char *fmt, va_list ap)
{
    char line[LOG_LINE_BYTES];
    fmt_sink l;
    l.buf = line;
    l.cap = LOG_LINE_BYTES;
    l.n = 0;
    line_puts(&l, "[shim] ");
    line_format(&l, fmt, ap);
    line_put(&l, '\n');
    rhu_log_write(level, line, l.n < LOG_LINE_BYTES - 1u ? l.n : LOG_LINE_BYTES - 1u);
}

void rhu_shim_log(const char *fmt, ...)
{
    va_list ap;
    va_start(ap, fmt);
    shim_line(RHU_LOG_VERBOSE, fmt, ap);
    va_end(ap);
}

void rhu_shim_last_words(const char *fmt, ...)
{
    va_list ap;
    va_start(ap, fmt);
    shim_line(RHU_LOG_SILENT, fmt, ap);
    va_end(ap);
}

int32_t rhu_shim_vformat(char *buf, uint32_t cap, const char *fmt, va_list ap)
{
    fmt_sink l;
    l.buf = buf;
    l.cap = cap;
    l.n = 0;
    line_format(&l, fmt, ap);
    if (cap != 0)
        buf[l.n < cap - 1u ? l.n : cap - 1u] = 0;
    return (int32_t)l.n;
}

void rhu_shim_exit(int code)
{
    rhu_shim_log("exit(%d) from thread %d", code, rhu_thread_self());
    rhu_exit(code);
    __builtin_unreachable();
}

/* ------------------------------------------------------------------ */
/* the per-thread block: errno, the key values, the flags word, the    */
/* signal mask                                                          */
/* ------------------------------------------------------------------ */

/* The block is a module in the kernel's registry with no text and no
 * statics, only thread-local bytes: registering it gives every live
 * thread a zeroed block, and every later spawn allocates one before the
 * thread exists, so no path here can fail for want of a block. The
 * signal mask is the thread's own bookkeeping word (signals 1..64); it
 * masks nothing on this platform and is reported back wherever a caller
 * asks for it. */
#define SHIM_KEYS 8u
#define SHIM_FLAG_DTORS_ARMED 1u

typedef struct shim_block {
    int err;
    void *keys[SHIM_KEYS];
    uint32_t flags;
    uint64_t sigmask;
} shim_block;

static int32_t g_block_module = -1;

static shim_block *block(void)
{
    if (g_block_module < 0) {
        rhu_shim_last_words("the per-thread block is not registered: rhu_shim_init must run first");
        rhu_exit(120);
    }
    return (shim_block *)rhu_tls_get(g_block_module);
}

int *__errno_location(void)
{
    return &block()->err;
}

#define SET_ERRNO(e) (block()->err = (e))

/* ------------------------------------------------------------------ */
/* the stack protector                                                 */
/* ------------------------------------------------------------------ */

/* The guard word every protected frame in the runtime archives stores at
 * entry and compares at exit (the archives were built with the stack
 * protector on, and the glibc personality reads the word through this
 * global). It starts as a fixed pattern and is re-seeded from the entropy
 * door at initialization, before any protected frame exists; a mismatch
 * at a frame's exit ends the run through the loud path below, never by
 * returning into an overwritten frame. Answering both names here keeps
 * the C library's own stack-protector member, and the signal member it
 * pulls, out of the image. */
uintptr_t __stack_chk_guard = 0x52485521u;

void __stack_chk_fail(void) __attribute__((noreturn));
void __stack_chk_fail(void)
{
    rhu_shim_log("stack protector: a frame's guard word was overwritten on thread %d", rhu_thread_self());
    rhu_abort(134);
    __builtin_unreachable();
}

/* ------------------------------------------------------------------ */
/* the managed heap's window                                           */
/* ------------------------------------------------------------------ */

/* The window of the kernel's system memory the managed heap lives in,
 * read once at initialization. The kernel serves one range of RAM on
 * every row -- range 0, the chip's own SRAM, whose arena is window 0,
 * the memory every mapping below draws from -- and a second where the
 * board has an external RAM the image brought up and proved at boot
 * (range 1, whose arena is window 1). The heap goes where the runtime's
 * rehydrated type data went: the link map places that data between the
 * brackets __rhu_hydrated_lo and __rhu_hydrated_hi, and when the
 * brackets lie inside range 1's physical range the managed heap, the
 * collector's mark bitmap and its mark stack are served from window 1
 * through the collector's own entry (rhu_shim_collector_mmap below);
 * when they lie elsewhere (inside the SRAM's statics, or in an image
 * whose map defines no brackets), or the image serves one range, the
 * heap is window 0's as every other mapping. The brackets are weak
 * references: an image whose map leaves them undefined reads them as
 * NULL. The page counts sysconf answers and the runtime's hard-limit
 * row are the heap window's. */
extern char __rhu_hydrated_lo[] __attribute__((weak));
extern char __rhu_hydrated_hi[] __attribute__((weak));
extern char __rhu_arena_lo[];
extern char __rhu_arena_hi[];

static uint32_t g_heap_window;

static void heap_window_choose(void)
{
    g_heap_window = 0u;
    if (rhu_ram_count() < 2)
        return;
    rhu_ram_desc r;
    if (rhu_ram_range(1u, &r) != 0)
        return;
    if (__rhu_hydrated_lo == NULL || __rhu_hydrated_hi == NULL)
        return;
    uint32_t lo = (uint32_t)(uintptr_t)__rhu_hydrated_lo;
    uint32_t hi = (uint32_t)(uintptr_t)__rhu_hydrated_hi;
    if (lo <= hi && lo >= r.base && hi - r.base <= r.bytes)
        g_heap_window = 1u;
}

/* The heap window's size in bytes, and its headroom at the call: window
 * 1 by the kernel's range record, window 0 by the arena's link brackets
 * and its headroom. */
static uint32_t heap_window_bytes(void)
{
    if (g_heap_window == 1u) {
        rhu_ram_desc r;
        if (rhu_ram_range(1u, &r) == 0)
            return r.hi - r.lo;
    }
    return (uint32_t)((uintptr_t)__rhu_arena_hi - (uintptr_t)__rhu_arena_lo);
}

static uint32_t heap_window_free(void)
{
    if (g_heap_window == 1u) {
        rhu_ram_desc r;
        if (rhu_ram_range(1u, &r) == 0)
            return r.free;
    }
    return rhu_arena_headroom();
}

/* ------------------------------------------------------------------ */
/* the environment table                                               */
/* ------------------------------------------------------------------ */

/* The rows the runtime reads. The collector's aggregate hard limit is
 * generated at initialization: the managed heap window's free memory at
 * that moment, less what the board row keeps for later draws (the frozen
 * object heap and the system memory left unallocated) when that window
 * is window 0, where those draws are made (window 1 serves the heap and
 * the collector's tables alone), in whole pages. It is the most
 * any collector on this board can be given, and it is a bound for a stock
 * collector only: this image's own collector sizes the managed heap at
 * its start and takes less than the row says, by the collector tables and
 * by whatever the runtime maps first. The rest are constants of this
 * hosting. The two collector rows name this image's own collector: the
 * runtime loads a collector by the path row directly, through the loader
 * seam below, and the name row is stated beside it because the runtime
 * reads both. */
#define ROW_KEEP_BYTES ((uint32_t)MGC_ROW_FOH_POOL_BYTES + (uint32_t)MGC_ROW_SYSTEM_MEMORY_UNALLOCATED_BYTES)

/* The thread pool's idle-worker timeout row. The runtime's pool worker
 * threads exit after this many milliseconds without work, and the kernel
 * takes each exited worker's stack back into the row's stack pool; the
 * runtime reads the row as DOTNET_ThreadPool_ThreadTimeoutMs (its own
 * default, 20000 ms, keeps an idle worker's stack for twenty seconds on a
 * pool of a few stacks). The row's default here is 100 ms: an idle worker
 * returns its stack after 100 ms, inside the window in which a refused
 * thread creation is asked again (SPAWN_RETRY_TRIES tries of
 * SPAWN_RETRY_STEP_NS: 100 tries of 1 ms, in spawn_thread below), so a
 * program's thread refused while idle workers still hold their stacks is
 * served once they go; the timeout must stay under the window, and the
 * assertion keeps it there. The runtime reads this environment row before
 * its own runtimeconfig switch (the pool's reader takes the environment
 * variable first and falls back to the switch), so a program's
 * runtimeconfig value never overrides the row: a program sets its own
 * value through the link word __rhu_threadpool_thread_timeout_ms, an
 * absolute symbol whose address IS the value, defined by the SDK's link
 * line from the project's property as __rhu_thread_stack_default is --
 * a positive word is the timeout in milliseconds, a negative word is the
 * runtime's own never (-1), and no word is the row's default. */
#define ROW_POOL_THREAD_TIMEOUT_MS 100u
#define SPAWN_RETRY_TRIES 100u
#define SPAWN_RETRY_STEP_NS 1000000ull
_Static_assert((unsigned long long)ROW_POOL_THREAD_TIMEOUT_MS * 1000000ull <= (unsigned long long)SPAWN_RETRY_TRIES * SPAWN_RETRY_STEP_NS,
               "the pool's idle-worker timeout must stay inside the thread-creation retry window");
extern const char __rhu_threadpool_thread_timeout_ms[] __attribute__((weak));

static char g_env_hard_limit[48];
static char g_env_pool_timeout[48];
static char *g_env[] = {
    g_env_hard_limit,
    "DOTNET_gcServer=0",
    "DOTNET_gcConcurrent=0",
    g_env_pool_timeout,
    "DOTNET_GCName=rhu-mgc",
    "DOTNET_GCPath=rhu-mgc",
    "HOME=/tmp",
    NULL,
};

static void env_build(void)
{
    uint32_t free_now = heap_window_free();
    uint32_t keep = g_heap_window == 1u ? 0u : ROW_KEEP_BYTES;
    uint32_t limit = free_now > keep ? (free_now - keep) & ~4095u : 0u;
    fmt_sink l;
    l.buf = g_env_hard_limit;
    l.cap = (uint32_t)sizeof g_env_hard_limit;
    l.n = 0;
    line_puts(&l, "DOTNET_GCHeapHardLimit=0x");
    line_num(&l, (unsigned long long)limit, 16u, 0u, 0);
    g_env_hard_limit[l.n < sizeof g_env_hard_limit - 1u ? l.n : sizeof g_env_hard_limit - 1u] = 0;
    l.buf = g_env_pool_timeout;
    l.cap = (uint32_t)sizeof g_env_pool_timeout;
    l.n = 0;
    line_puts(&l, "DOTNET_ThreadPool_ThreadTimeoutMs=");
    if (__rhu_threadpool_thread_timeout_ms == NULL)
        line_num(&l, (unsigned long long)ROW_POOL_THREAD_TIMEOUT_MS, 10u, 0u, 0);
    else if ((int32_t)(uintptr_t)__rhu_threadpool_thread_timeout_ms < 0)
        line_puts(&l, "-1");
    else
        line_num(&l, (unsigned long long)(uintptr_t)__rhu_threadpool_thread_timeout_ms, 10u, 0u, 0);
    g_env_pool_timeout[l.n < sizeof g_env_pool_timeout - 1u ? l.n : sizeof g_env_pool_timeout - 1u] = 0;
}

char **rhu_shim_environ(void)
{
    return g_env;
}

char *getenv(const char *name)
{
    size_t n = strlen(name);
    for (int i = 0; g_env[i] != NULL; i++) {
        if (strncmp(g_env[i], name, n) == 0 && g_env[i][n] == '=')
            return g_env[i] + n + 1;
    }
    return NULL;
}

int rhu_shim_init(void)
{
    if (g_block_module >= 0)
        return 0;
    rhu_module_desc d = {0};
    d.tls_bytes = (uint32_t)sizeof(shim_block);
    d.tls_init = NULL;
    d.name = "shim";
    int32_t h = rhu_module_register(&d);
    if (h < 0) {
        rhu_shim_last_words("init: the kernel refused the per-thread block module (%d)", h);
        rhu_exit(120);
    }
    g_block_module = h;
    heap_window_choose();
    env_build();
    if (rhu_entropy(&__stack_chk_guard, (uint32_t)sizeof __stack_chk_guard) != 0) {
        rhu_shim_last_words("init: the kernel's entropy call refused the stack guard's seed");
        rhu_exit(120);
    }
    rhu_shim_log("init: per-thread block registered as module %d (%u bytes)", h,
                 (unsigned)sizeof(shim_block));
    rhu_shim_log("init: the managed heap's window is %u (%s)", (unsigned)g_heap_window,
                 g_heap_window == 1u ? "external: the rehydrated-data brackets lie inside range 1"
                                     : "internal: one range served, or the brackets lie outside range 1");
    return 0;
}

/* ------------------------------------------------------------------ */
/* memory mapping: the kernel arena                                     */
/* ------------------------------------------------------------------ */

#define GLIBC_MAP_FIXED 0x10
#define MMAP_FAILED ((void *)-1)
#define PAGE_BYTES 4096u

static uint32_t g_fixed_rezeros;

/* Anonymous mappings are window 0 arena allocations attributed to the
 * caller (the collector's region and tables are asked for through the
 * entry below, and go where the managed heap lives);
 * a fixed mapping over owned space is the decommit contract, delivered as
 * an in-place re-zero at the same address with no address space consumed;
 * file-backed mappings do not exist on this platform. */
void *mmap64(void *addr, size_t len, int prot, int flags, int fd, int64_t off)
{
    (void)prot;
    (void)off;
    uint32_t site = (uint32_t)(uintptr_t)__builtin_return_address(0);
    if (fd != -1) {
        rhu_shim_log("mmap: file-backed mapping not available (ENOSYS)");
        SET_ERRNO(RHU_GERR_ENOSYS);
        return MMAP_FAILED;
    }
    if (len == 0) {
        SET_ERRNO(RHU_GERR_EINVAL);
        return MMAP_FAILED;
    }
    if ((flags & GLIBC_MAP_FIXED) != 0 && addr != NULL) {
        if (rhu_arena_rezero(addr, (uint32_t)len) == 0) {
            if (g_fixed_rezeros++ == 0)
                rhu_shim_log("mmap: MAP_FIXED re-zero in place (decommit; further requests counted silently)");
            return addr;
        }
        rhu_shim_log("mmap: MAP_FIXED outside the arena (EINVAL)");
        SET_ERRNO(RHU_GERR_EINVAL);
        return MMAP_FAILED;
    }
    uint32_t rounded = ((uint32_t)len + PAGE_BYTES - 1u) & ~(PAGE_BYTES - 1u);
    void *p = rhu_arena_alloc_at(rounded, PAGE_BYTES, site);
    if (p == NULL) {
        rhu_shim_log("mmap: the arena is exhausted (ENOMEM)");
        SET_ERRNO(RHU_GERR_ENOMEM);
        return MMAP_FAILED;
    }
    rhu_shim_log("mmap: anonymous %u KB served from the kernel arena (page-aligned, zero-born)",
                 (unsigned)(rounded / 1024u));
    return p;
}

/* The collector's own mapping entry: the managed heap's one region, its
 * mark bitmap and its mark stack are asked for through this name and
 * served from the managed heap's window -- window 1 through the kernel's
 * named-range door when the heap lives in the external RAM, window 0 as
 * mmap64 above otherwise. Page-rounded, page-aligned, zero-born and
 * attributed to the caller as mmap64's mappings are; (void *)-1 with
 * ENOMEM when that window is exhausted, so a heap that does not fit is
 * refused at the collector's start without a byte drawn from the other
 * window; EINVAL for a zero length. Every other mapping of the runtime
 * -- the handle pages, the frozen object heap's pool, the small
 * anonymous mappings -- keeps mmap64 and window 0. */
void *rhu_shim_collector_mmap(size_t len)
{
    uint32_t site = (uint32_t)(uintptr_t)__builtin_return_address(0);
    if (len == 0u || len > (size_t)(0xFFFFFFFFu - PAGE_BYTES)) {
        SET_ERRNO(RHU_GERR_EINVAL);
        return MMAP_FAILED;
    }
    uint32_t rounded = ((uint32_t)len + PAGE_BYTES - 1u) & ~(PAGE_BYTES - 1u);
    void *p = g_heap_window == 1u ? rhu_arena_alloc_from(1u, rounded, PAGE_BYTES, site)
                                  : rhu_arena_alloc_at(rounded, PAGE_BYTES, site);
    if (p == NULL) {
        rhu_shim_log("collector mmap: window %u is exhausted (ENOMEM)", (unsigned)g_heap_window);
        SET_ERRNO(RHU_GERR_ENOMEM);
        return MMAP_FAILED;
    }
    rhu_shim_log("collector mmap: %u KB served from window %u (page-aligned, zero-born)",
                 (unsigned)(rounded / 1024u), (unsigned)g_heap_window);
    return p;
}

int munmap(void *a, size_t l)
{
    (void)a;
    (void)l;
    return 0;
}

int mprotect(void *a, size_t l, int p)
{
    (void)a;
    (void)l;
    (void)p;
    return 0;
}

int madvise(void *a, size_t l, int v)
{
    (void)a;
    (void)l;
    (void)v;
    return 0;
}

int mlock(const void *a, size_t l)
{
    (void)a;
    (void)l;
    return 0;
}

int munlock(const void *a, size_t l)
{
    (void)a;
    (void)l;
    return 0;
}

/* ------------------------------------------------------------------ */
/* time: the 64-bit time surface on the kernel clock and its calendar   */
/* ------------------------------------------------------------------ */

/* The 64-bit timespec on a 32-bit little-endian target: seconds, then the
 * nanoseconds, then padding. The two real-time clock ids read the kernel's
 * calendar as Unix time; every other id reads the boot-relative nanosecond
 * clock. A timed wait honors the clock its timestamp names (below). */
struct shim_ts64 {
    int64_t sec;
    int32_t nsec;
    int32_t pad;
};

#define NS_PER_S 1000000000ull
#define GLIBC_TIMER_ABSTIME 1

static void ts64_from_ns(struct shim_ts64 *ts, uint64_t ns)
{
    ts->sec = (int64_t)(ns / NS_PER_S);
    ts->nsec = (int32_t)(ns % NS_PER_S);
    ts->pad = 0;
}

/* The Linux clock ids of the glibc personality: the two real-time clocks
 * read the kernel's calendar, every other id the monotonic clock. */
#define SHIM_CLOCK_REALTIME 0
#define SHIM_CLOCK_REALTIME_COARSE 5

static int clock_is_realtime(int id)
{
    return id == SHIM_CLOCK_REALTIME || id == SHIM_CLOCK_REALTIME_COARSE;
}

/* The clock a timed wait honors. The real-time ids name the calendar, so
 * a real-time timestamp is translated into the boot-clock domain the
 * kernel's deadlines live in (below); every other id names the boot
 * clock, so the timestamp is the deadline itself. */
static int wait_clock_is_realtime(int id)
{
#ifdef RHU_SHIM_BREAK_REALTIME_WAIT
    /* Built with this define, the clock a timed wait names is discarded and a
     * real-time timestamp is read as a boot-relative one. Never set in a
     * hosted image. */
    (void)id;
    return 0;
#else
    return clock_is_realtime(id);
#endif
}

/* A timestamp's nanosecond field within its second. */
static int ts64_valid(const struct shim_ts64 *ts)
{
    return ts->nsec >= 0 && ts->nsec < (int32_t)NS_PER_S;
}

/* The boot-clock instant `ns` past now, saturated below the no-deadline
 * word: a deadline the clock cannot reach is a deadline still. */
static uint64_t deadline_after(uint64_t ns)
{
    uint64_t now = rhu_now_ns();
    return ns > RHU_DEADLINE_NONE - 1u - now ? RHU_DEADLINE_NONE - 1u : now + ns;
}

/* A boot-clock absolute timestamp as a kernel deadline: a negative second
 * count is already past; a count past the clock's range saturates below
 * the no-deadline word. */
static uint64_t monotonic_deadline(const struct shim_ts64 *ts)
{
    if (ts->sec < 0)
        return 0u;
    uint64_t sec = (uint64_t)ts->sec;
    if (sec > (RHU_DEADLINE_NONE - 1u - (uint64_t)ts->nsec) / NS_PER_S)
        return RHU_DEADLINE_NONE - 1u;
    return sec * NS_PER_S + (uint64_t)ts->nsec;
}

/* A real-time absolute wait re-reads the calendar this often while it is
 * pending. The calendar is a base plus the boot clock, and a set moves
 * the base without waking anyone, so the wait follows a set within one
 * slice: set past the timestamp, the wait ends; set back, the wait lasts
 * until the calendar reaches the timestamp again. The runtime's own timed
 * waits name the boot clock and never take a slice. */
#define SHIM_REALTIME_SLICE_NS (100u * 1000u * 1000u)

/* The nanoseconds a real-time absolute timestamp is still ahead of the
 * calendar: 0 once the calendar has reached it (a pre-epoch or otherwise
 * past timestamp is past), saturated below the no-deadline word for a
 * timestamp the calendar cannot reach. Every step is checked: the second
 * count is bounded before the multiply into calendar ticks, the
 * nanoseconds round up to the tick so the wait is never early, the
 * difference is taken in unsigned arithmetic where it is known positive,
 * and the ticks are bounded before the multiply into nanoseconds. */
static uint64_t realtime_remaining_ns(const struct shim_ts64 *ts)
{
    const int64_t sec_bound = 900000000000ll; /* 9e11 s of Unix time: 9e18 ticks, inside the signed range */
    if (ts->sec < -sec_bound)
        return 0u;
    if (ts->sec > sec_bound)
        return RHU_DEADLINE_NONE - 1u;
    int64_t target = ts->sec * 10000000ll + (int64_t)((ts->nsec + 99) / 100);
    int64_t now = rhu_shim_calendar_unix_ticks();
    if (target <= now)
        return 0u;
    uint64_t ticks = (uint64_t)target - (uint64_t)now; /* exact: 0 < target - now < 2^64 */
    if (ticks > (RHU_DEADLINE_NONE - 1u) / 100u)
        return RHU_DEADLINE_NONE - 1u;
    return ticks * 100u;
}

int __clock_gettime64(int id, struct shim_ts64 *ts)
{
    if (clock_is_realtime(id)) {
        int64_t sec;
        int32_t nsec;
        rhu_shim_unix_split(rhu_shim_calendar_unix_ticks(), &sec, &nsec);
        ts->sec = sec;
        ts->nsec = nsec;
        ts->pad = 0;
        return 0;
    }
    ts64_from_ns(ts, rhu_now_ns());
    return 0;
}

/* The sleep honors the clock id: a relative request is a boot-clock
 * interval (a negative second count refused); an absolute boot-clock
 * timestamp is slept to as a deadline; an absolute real-time timestamp is
 * slept to against the calendar, one slice at a time, so a calendar set
 * while the sleep is pending moves it. A nanosecond field outside its
 * second is refused. No signal interrupts a sleep here, so the remaining
 * time reported is always zero. */
int __clock_nanosleep_time64(int id, int flags, const struct shim_ts64 *rq, struct shim_ts64 *rm)
{
    if (!ts64_valid(rq))
        return RHU_GERR_EINVAL;
    if ((flags & GLIBC_TIMER_ABSTIME) == 0) {
        if (rq->sec < 0)
            return RHU_GERR_EINVAL;
        uint64_t sec = (uint64_t)rq->sec;
        uint64_t ns = sec > (UINT64_MAX - (uint64_t)rq->nsec) / NS_PER_S ? UINT64_MAX : sec * NS_PER_S + (uint64_t)rq->nsec;
        if (ns != 0)
            rhu_sleep_ns(ns);
    } else if (wait_clock_is_realtime(id)) {
        for (;;) {
            uint64_t left = realtime_remaining_ns(rq);
            if (left == 0)
                break;
            rhu_sleep_ns(left < SHIM_REALTIME_SLICE_NS ? left : SHIM_REALTIME_SLICE_NS);
        }
    } else {
        uint64_t deadline = monotonic_deadline(rq);
        uint64_t now = rhu_now_ns();
        if (deadline > now)
            rhu_sleep_ns(deadline - now);
    }
    if (rm != NULL)
        ts64_from_ns(rm, 0u);
    return 0;
}

int __nanosleep64(const struct shim_ts64 *rq, struct shim_ts64 *rm)
{
    return __clock_nanosleep_time64(0, 0, rq, rm);
}

int64_t __time64(int64_t *out)
{
    int64_t t;
    int32_t nsec;
    rhu_shim_unix_split(rhu_shim_calendar_unix_ticks(), &t, &nsec);
    if (out != NULL)
        *out = t;
    return t;
}

/* The calendar's source, in the kernel's words, for the one line the
 * first wall-clock read prints. */
static const char *calendar_source_text(int32_t source)
{
    switch (source) {
    case RHU_CALENDAR_UNSET:
        return "unset (the epoch plus the elapsed clock)";
    case RHU_CALENDAR_RTC:
        return "the board's real-time clock, read at boot";
    case RHU_CALENDAR_PUSH:
        return "the debug channel's push";
    case RHU_CALENDAR_NETWORK:
        return "network time, set explicitly";
    case RHU_CALENDAR_APP:
        return "the application, set explicitly";
    default:
        return "a source the kernel does not name";
    }
}

int64_t rhu_shim_calendar_unix_ticks(void)
{
    static uint32_t announced;
    uint64_t ticks = 0;
    int32_t source = 0;
    int32_t r = rhu_calendar_get(&ticks, &source);
    if (r != 0) {
        rhu_shim_log("calendar: the kernel refused the calendar read (%d)", r);
        rhu_shim_exit(120);
    }
    if (announced++ == 0)
        rhu_shim_log("calendar: the kernel's calendar serves the wall clock; source: %s", calendar_source_text(source));
    return (int64_t)ticks - RHU_SHIM_UNIX_EPOCH_TICKS;
}

/* ------------------------------------------------------------------ */
/* locks: futex recipes held inline in the glibc-sized slots            */
/* ------------------------------------------------------------------ */

/* The mutex word is 0 free, 1 locked, 2 locked with waiters: an
 * uncontended lock or unlock is one atomic operation and never enters the
 * kernel. The condition variable is a sequence word: a waiter captures it
 * under the mutex and waits for it to move after releasing the mutex, so
 * a signal between the release and the wait makes the wait return at
 * once. The objects live inline in the slots the archives hand over, at
 * the glibc arm32 sizes; a slot that arrives all zero is the static
 * initializer and is adopted on first use. */

typedef struct shim_mutex {
    volatile uint32_t word;
    int32_t owner;
    uint32_t count;
    uint32_t recursive;
} shim_mutex;

typedef struct shim_cond {
    volatile uint32_t seq;
    int32_t clock;
} shim_cond;

#define MAGIC_MUTEX 0x4D555458u
#define MAGIC_COND 0x434F4E44u
#define MAGIC_ATTR 0x41545452u
#define GLIBC_PTHREAD_MUTEX_RECURSIVE 1

typedef struct mutex_slot {
    uint32_t magic;
    shim_mutex m;
} mutex_slot;

typedef struct cond_slot {
    uint32_t magic;
    shim_cond c;
} cond_slot;

typedef struct attr_slot {
    uint32_t magic;
    uint32_t stack_bytes;
    uint32_t detached;
    uint32_t tid;
} attr_slot;

_Static_assert(sizeof(mutex_slot) <= 24, "the mutex must fit the glibc arm32 slot (24 bytes)");
_Static_assert(sizeof(cond_slot) <= 48, "the condition variable must fit the glibc arm32 slot (48 bytes)");
_Static_assert(sizeof(attr_slot) <= 36, "the thread attributes must fit the glibc arm32 slot (36 bytes)");

static void word_lock(volatile uint32_t *w)
{
    uint32_t zero = 0;
    if (__atomic_compare_exchange_n(w, &zero, 1u, 0, __ATOMIC_ACQUIRE, __ATOMIC_RELAXED))
        return;
    uint32_t c = __atomic_exchange_n(w, 2u, __ATOMIC_ACQUIRE);
    while (c != 0) {
        rhu_futex_wait((const uint32_t *)w, 2u, RHU_DEADLINE_NONE);
        c = __atomic_exchange_n(w, 2u, __ATOMIC_ACQUIRE);
    }
}

static void word_unlock(volatile uint32_t *w)
{
    if (__atomic_fetch_sub(w, 1u, __ATOMIC_RELEASE) != 1u) {
        __atomic_store_n(w, 0u, __ATOMIC_RELEASE);
        rhu_futex_wake((const uint32_t *)w, 1u);
    }
}

/* ------------------------------------------------------------------ */
/* the 64-bit atomics: the library calls the compiler names            */
/* ------------------------------------------------------------------ */

/* The runtime archives are compiled for the core they run on, and no
 * Cortex-M has a 64-bit exclusive-access instruction, so the compiler
 * lowers every 64-bit atomic operation in them to a call of this family
 * (the compare-and-swap, the exchange and its release, the
 * fetch-and-operate forms); the managed interlocked operations on a
 * 64-bit value reach the runtime's own helper, which calls the
 * compare-and-swap. Each call holds one lock word of the futex family
 * for the operation's few instructions: an uncontended take is one
 * 32-bit exclusive operation and a contended one waits in the kernel, so
 * every 64-bit atomic is atomic against every other, no interrupt is
 * masked, and the recipe holds on a second core. A full barrier on each
 * side keeps the family's ordering contract. The symbol names are the
 * compiler's; the C names are this file's own. */

static volatile uint32_t atomic64_word;

static void atomic64_enter(void)
{
    __atomic_thread_fence(__ATOMIC_SEQ_CST);
    word_lock(&atomic64_word);
}

static void atomic64_leave(void)
{
    word_unlock(&atomic64_word);
    __atomic_thread_fence(__ATOMIC_SEQ_CST);
}

uint64_t shim_sync_val_compare_and_swap_8(volatile uint64_t *p, uint64_t expected, uint64_t desired) __asm__("__sync_val_compare_and_swap_8");
int shim_sync_bool_compare_and_swap_8(volatile uint64_t *p, uint64_t expected, uint64_t desired) __asm__("__sync_bool_compare_and_swap_8");
uint64_t shim_sync_lock_test_and_set_8(volatile uint64_t *p, uint64_t value) __asm__("__sync_lock_test_and_set_8");
void shim_sync_lock_release_8(volatile uint64_t *p) __asm__("__sync_lock_release_8");

uint64_t shim_sync_val_compare_and_swap_8(volatile uint64_t *p, uint64_t expected, uint64_t desired)
{
    atomic64_enter();
    uint64_t old = *p;
    if (old == expected)
        *p = desired;
    atomic64_leave();
    return old;
}

int shim_sync_bool_compare_and_swap_8(volatile uint64_t *p, uint64_t expected, uint64_t desired)
{
    return shim_sync_val_compare_and_swap_8(p, expected, desired) == expected;
}

uint64_t shim_sync_lock_test_and_set_8(volatile uint64_t *p, uint64_t value)
{
    atomic64_enter();
    uint64_t old = *p;
    *p = value;
    atomic64_leave();
    return old;
}

void shim_sync_lock_release_8(volatile uint64_t *p)
{
    atomic64_enter();
    *p = 0u;
    atomic64_leave();
}

/* The fetch-and-operate forms: the old value returned, the new one
 * stored, under the same lock. */
#define ATOMIC64_FETCH_AND(cname, symbol, type, expr)                       \
    type cname(volatile type *p, type value) __asm__(symbol);               \
    type cname(volatile type *p, type value)                                \
    {                                                                       \
        atomic64_enter();                                                   \
        type old = *p;                                                      \
        *p = (expr);                                                        \
        atomic64_leave();                                                   \
        return old;                                                         \
    }

ATOMIC64_FETCH_AND(shim_sync_fetch_and_add_8, "__sync_fetch_and_add_8", uint64_t, old + value)
ATOMIC64_FETCH_AND(shim_sync_fetch_and_sub_8, "__sync_fetch_and_sub_8", uint64_t, old - value)
ATOMIC64_FETCH_AND(shim_sync_fetch_and_and_8, "__sync_fetch_and_and_8", uint64_t, old & value)
ATOMIC64_FETCH_AND(shim_sync_fetch_and_or_8, "__sync_fetch_and_or_8", uint64_t, old | value)
ATOMIC64_FETCH_AND(shim_sync_fetch_and_xor_8, "__sync_fetch_and_xor_8", uint64_t, old ^ value)
ATOMIC64_FETCH_AND(shim_sync_fetch_and_nand_8, "__sync_fetch_and_nand_8", uint64_t, ~(old & value))
ATOMIC64_FETCH_AND(shim_sync_fetch_and_max_8, "__sync_fetch_and_max_8", int64_t, old > value ? old : value)
ATOMIC64_FETCH_AND(shim_sync_fetch_and_min_8, "__sync_fetch_and_min_8", int64_t, old < value ? old : value)
ATOMIC64_FETCH_AND(shim_sync_fetch_and_umax_8, "__sync_fetch_and_umax_8", uint64_t, old > value ? old : value)
ATOMIC64_FETCH_AND(shim_sync_fetch_and_umin_8, "__sync_fetch_and_umin_8", uint64_t, old < value ? old : value)

static void mutex_lock(shim_mutex *m)
{
    int32_t self = rhu_thread_self();
    if (m->recursive != 0 && m->owner == self) {
        m->count++;
        return;
    }
    word_lock(&m->word);
    m->owner = self;
    m->count = 1;
}

static void mutex_unlock(shim_mutex *m)
{
    if (m->recursive != 0 && --m->count > 0)
        return;
    m->owner = 0;
    m->count = 0;
    word_unlock(&m->word);
}

/* Wait for the sequence word to move after releasing the mutex; a wake,
 * a spurious wake, or an activation returns 0 (the caller rechecks its
 * predicate); only the deadline returns ETIMEDOUT. */
static int cond_wait(shim_cond *c, shim_mutex *m, uint64_t deadline_ns)
{
    uint32_t s = __atomic_load_n(&c->seq, __ATOMIC_ACQUIRE);
    mutex_unlock(m);
    int32_t rc = rhu_futex_wait((const uint32_t *)&c->seq, s, deadline_ns);
    mutex_lock(m);
    return rc == RHU_ETIMEDOUT ? RHU_GERR_ETIMEDOUT : 0;
}

static void cond_wake(shim_cond *c, uint32_t n)
{
    __atomic_fetch_add(&c->seq, 1u, __ATOMIC_RELEASE);
    rhu_futex_wake((const uint32_t *)&c->seq, n);
}

/* A slot in the static-initializer form (all zero) is adopted at its first
 * use, and two threads may arrive at once: the adoption is ONE
 * compare-and-swap of the magic word from zero, and a thread that finds
 * the slot no longer zero reads the magic again before it refuses -- the
 * other thread's adoption, and perhaps its lock, is what it found. */
#ifdef RHU_SHIM_BREAK_MUTEX_ADOPT
/* The window of the build below: held open, blocked in the kernel's sleep,
 * until another first user has adopted the slot; a first user alone gives
 * up at this deadline of the kernel's clock, the only way the deadline is
 * reached (the core section of gate/rhu_shimtest.c runs its second first
 * user one step below its own thread, so it runs only while the first is
 * blocked here). */
#define SHIM_CONTROL_WINDOW_NS 500000000ull
static uint32_t g_control_inside;
#endif

static shim_mutex *mutex_of(void *slot)
{
    mutex_slot *s = (mutex_slot *)slot;
#ifdef RHU_SHIM_BREAK_MUTEX_ADOPT
    /* Built with this define, the adoption is a read, a check and a store,
     * the window between the read and the check held open, blocked, until
     * another first user has adopted the slot (the first user inside holds
     * the window; a second one, arriving while it waits, adopts at once).
     * With two first users inside together the second adopts and the
     * first is refused, every time, and the two-thread adoption check in
     * gate/rhu_shimtest.c fails. */
    if (s->magic != MAGIC_MUTEX) {
        static const mutex_slot zero;
        if (__atomic_fetch_add(&g_control_inside, 1u, __ATOMIC_ACQ_REL) == 0u) {
            uint64_t deadline = rhu_now_ns() + SHIM_CONTROL_WINDOW_NS;
            while (__builtin_memcmp(s, &zero, sizeof zero) == 0 && rhu_now_ns() < deadline)
                rhu_sleep_ns(1000000ull);
        }
        __atomic_fetch_sub(&g_control_inside, 1u, __ATOMIC_ACQ_REL);
        if (__builtin_memcmp(s, &zero, sizeof zero) != 0)
            return NULL;
        s->magic = MAGIC_MUTEX;
    }
    return &s->m;
#else
    if (__atomic_load_n(&s->magic, __ATOMIC_ACQUIRE) != MAGIC_MUTEX) {
        static const mutex_slot zero;
        if (__builtin_memcmp(s, &zero, sizeof zero) != 0)
            return __atomic_load_n(&s->magic, __ATOMIC_ACQUIRE) == MAGIC_MUTEX ? &s->m : NULL;
        uint32_t expected = 0;
        if (!__atomic_compare_exchange_n(&s->magic, &expected, MAGIC_MUTEX, 0, __ATOMIC_RELEASE, __ATOMIC_ACQUIRE)
            && expected != MAGIC_MUTEX)
            return NULL;
    }
    return &s->m;
#endif
}

static shim_cond *cond_of(void *slot)
{
    /* Adopted as a mutex slot is: one compare-and-swap, and the magic read
     * again before a slot found non-zero is refused. */
    cond_slot *s = (cond_slot *)slot;
    if (__atomic_load_n(&s->magic, __ATOMIC_ACQUIRE) != MAGIC_COND) {
        static const cond_slot zero;
        if (__builtin_memcmp(s, &zero, sizeof zero) != 0)
            return __atomic_load_n(&s->magic, __ATOMIC_ACQUIRE) == MAGIC_COND ? &s->c : NULL;
        uint32_t expected = 0;
        if (!__atomic_compare_exchange_n(&s->magic, &expected, MAGIC_COND, 0, __ATOMIC_RELEASE, __ATOMIC_ACQUIRE)
            && expected != MAGIC_COND)
            return NULL;
    }
    return &s->c;
}

int pthread_mutexattr_init(void *a)
{
    *(int *)a = 0;
    return 0;
}

int pthread_mutexattr_destroy(void *a)
{
    (void)a;
    return 0;
}

int pthread_mutexattr_settype(void *a, int kind)
{
    *(int *)a = kind;
    return 0;
}

int pthread_mutex_init(void *slot, const void *attr)
{
    mutex_slot *s = (mutex_slot *)slot;
    memset(s, 0, sizeof *s);
    s->magic = MAGIC_MUTEX;
    if (attr != NULL && (*(const int *)attr & GLIBC_PTHREAD_MUTEX_RECURSIVE) != 0)
        s->m.recursive = 1;
    return 0;
}

int pthread_mutex_lock(void *slot)
{
    shim_mutex *m = mutex_of(slot);
    if (m == NULL) {
        rhu_shim_log("mutex: the slot is not a mutex (EINVAL)");
        return RHU_GERR_EINVAL;
    }
    mutex_lock(m);
    return 0;
}

int pthread_mutex_unlock(void *slot)
{
    shim_mutex *m = mutex_of(slot);
    if (m == NULL)
        return RHU_GERR_EINVAL;
    mutex_unlock(m);
    return 0;
}

int pthread_mutex_destroy(void *slot)
{
    ((mutex_slot *)slot)->magic = 0;
    return 0;
}

int pthread_condattr_init(void *a)
{
    *(int *)a = 0;
    return 0;
}

int pthread_condattr_destroy(void *a)
{
    (void)a;
    return 0;
}

int pthread_condattr_setclock(void *a, int clock)
{
    *(int *)a = clock;
    return 0;
}

int pthread_cond_init(void *slot, const void *attr)
{
    cond_slot *s = (cond_slot *)slot;
    memset(s, 0, sizeof *s);
    s->magic = MAGIC_COND;
    s->c.clock = attr != NULL ? *(const int *)attr : 0;
    return 0;
}

int pthread_cond_wait(void *cs, void *ms)
{
    shim_cond *c = cond_of(cs);
    shim_mutex *m = mutex_of(ms);
    if (c == NULL || m == NULL)
        return RHU_GERR_EINVAL;
    return cond_wait(c, m, RHU_DEADLINE_NONE);
}

/* A real-time timed wait: the calendar re-read every slice on the one
 * captured sequence word, so a signal between slices ends the wait at
 * once (the word no longer matches when the wait re-arms), a wake
 * inside a slice ends it as any wait's does, and a calendar set while the
 * wait is pending moves it. Only the timestamp's arrival answers
 * ETIMEDOUT. */
static int cond_wait_realtime(shim_cond *c, shim_mutex *m, const struct shim_ts64 *abs)
{
    uint32_t s = __atomic_load_n(&c->seq, __ATOMIC_ACQUIRE);
    mutex_unlock(m);
    int32_t rc;
    for (;;) {
        uint64_t left = realtime_remaining_ns(abs);
        uint64_t slice = left < SHIM_REALTIME_SLICE_NS ? left : SHIM_REALTIME_SLICE_NS;
        rc = rhu_futex_wait((const uint32_t *)&c->seq, s, deadline_after(slice));
        if (rc != RHU_ETIMEDOUT || left == 0)
            break;
    }
    mutex_lock(m);
    return rc == RHU_ETIMEDOUT ? RHU_GERR_ETIMEDOUT : 0;
}

/* A timed wait honors the clock the condition variable was initialized
 * with (the default the real-time clock): a boot-clock timestamp is the
 * kernel deadline itself; a real-time timestamp is waited for against the
 * calendar. A nanosecond field outside its second is refused. */
int __pthread_cond_timedwait64(void *cs, void *ms, const struct shim_ts64 *abs)
{
    shim_cond *c = cond_of(cs);
    shim_mutex *m = mutex_of(ms);
    if (c == NULL || m == NULL || !ts64_valid(abs))
        return RHU_GERR_EINVAL;
    if (wait_clock_is_realtime(c->clock))
        return cond_wait_realtime(c, m, abs);
    return cond_wait(c, m, monotonic_deadline(abs));
}

int pthread_cond_signal(void *cs)
{
    shim_cond *c = cond_of(cs);
    if (c == NULL)
        return RHU_GERR_EINVAL;
    cond_wake(c, 1u);
    return 0;
}

int pthread_cond_broadcast(void *cs)
{
    shim_cond *c = cond_of(cs);
    if (c == NULL)
        return RHU_GERR_EINVAL;
    cond_wake(c, 0x7FFFFFFFu);
    return 0;
}

int pthread_cond_destroy(void *cs)
{
    ((cond_slot *)cs)->magic = 0;
    return 0;
}

/* ------------------------------------------------------------------ */
/* threads: kernel threads at the flat hosted priority                  */
/* ------------------------------------------------------------------ */

/* The default managed stack size: the value the SDK's link line defines
 * from the program's RhuThreadStack knob as an absolute symbol (its
 * address IS the value; the SDK refuses a value outside the kernel's
 * minimum and the row's clamp, or one that leaves fewer than three
 * stacks in the pool), or the row's constant when no such symbol was
 * given -- the gate images, and every program that leaves the knob
 * alone. The runtime's own default-stack knob cannot carry a value
 * inside a row's range (its reader keeps a 64 KB floor), so this is the
 * one way a program sizes its unsized threads on this system. */
extern const char __rhu_thread_stack_default[] __attribute__((weak));

static uint32_t row_stack_default(void)
{
    return __rhu_thread_stack_default != NULL ? (uint32_t)(uintptr_t)__rhu_thread_stack_default : (uint32_t)MGC_ROW_STACK_DEFAULT;
}

int pthread_attr_init(void *a)
{
    attr_slot *s = (attr_slot *)a;
    s->magic = MAGIC_ATTR;
    s->stack_bytes = row_stack_default();
    s->detached = 0;
    s->tid = 0;
    return 0;
}

int pthread_attr_destroy(void *a)
{
    (void)a;
    return 0;
}

int pthread_attr_setstacksize(void *a, size_t bytes)
{
    ((attr_slot *)a)->stack_bytes = (uint32_t)bytes;
    return 0;
}

int pthread_attr_setdetachstate(void *a, int detached)
{
    ((attr_slot *)a)->detached = (uint32_t)detached;
    return 0;
}

int pthread_getattr_np(uint32_t tid, void *a)
{
    pthread_attr_init(a);
    ((attr_slot *)a)->tid = tid;
    return 0;
}

/* The stack carve of the thread the attributes name, or of the calling
 * thread, answered by the kernel from its own records. */
int pthread_attr_getstack(void *a, void **lo, size_t *bytes)
{
    attr_slot *s = (attr_slot *)a;
    int32_t tid = (s != NULL && s->magic == MAGIC_ATTR && s->tid != 0) ? (int32_t)s->tid : rhu_thread_self();
    uint32_t base = 0;
    uint32_t len = 0;
    if (rhu_thread_stack_bounds(tid, &base, &len) != 0) {
        rhu_shim_log("attr_getstack: no live thread %d (EINVAL)", tid);
        return RHU_GERR_EINVAL;
    }
    *lo = (void *)(uintptr_t)base;
    *bytes = (size_t)len;
    return 0;
}

typedef struct thread_start {
    void *(*fn)(void *);
    void *arg;
} thread_start;

/* The kernel entry: hand the start record back to the heap, run the
 * thread function, and return -- the kernel then runs this thread's exit
 * destructors and frees its blocks. */
static void thread_entry(void *p)
{
    thread_start s = *(thread_start *)p;
    rhu_kheap_free(p);
    s.fn(s.arg);
}

static int spawn_thread(uint32_t stack_bytes, void *(*fn)(void *), void *arg, uint32_t *tid_out)
{
    uint32_t asked = stack_bytes;
    if (stack_bytes == 0)
        stack_bytes = row_stack_default();
    if (stack_bytes > MGC_ROW_STACK_CLAMP) {
        rhu_shim_log("pthread_create: stack %u KB clamped to %u KB (the row's clamp)",
                     (unsigned)(stack_bytes / 1024u), (unsigned)(MGC_ROW_STACK_CLAMP / 1024u));
        stack_bytes = MGC_ROW_STACK_CLAMP;
    }
    thread_start *s = (thread_start *)rhu_kheap_alloc((uint32_t)sizeof *s, 8u);
    if (s == NULL)
        return RHU_GERR_EAGAIN;
    s->fn = fn;
    s->arg = arg;
    /* The kernel's refusal is honest and momentary when a thread that was
     * joined has not yet given its stack back (the join returns before the
     * thread's last instruction); the retry story is this caller's, so a
     * refused spawn is asked again for a bounded while before it becomes
     * the caller's error -- SPAWN_RETRY_TRIES tries of SPAWN_RETRY_STEP_NS,
     * the window the pool's idle-worker timeout row above sits inside, so
     * a stack an idle pool worker is about to give back is waited for. */
    int32_t tid = rhu_thread_spawn(thread_entry, s, stack_bytes, RHU_SHIM_PRIO_FLAT, NULL);
    /* A clamped ask the pool cannot serve (the kernel serves an ask from
     * its never-carved remainder or a retired stack of exactly that size)
     * is asked again at the row's default before the bounded retries: a
     * thread on the row's default runs managed code as every hosted
     * thread does, and the pool's retired default-sized stacks serve it. */
    if (tid == RHU_EAGAIN && stack_bytes > row_stack_default()) {
        rhu_shim_log("pthread_create: the pool cannot serve the clamped %u KB; asked again at the row's default %u KB",
                     (unsigned)(stack_bytes / 1024u), (unsigned)(row_stack_default() / 1024u));
        stack_bytes = row_stack_default();
        tid = rhu_thread_spawn(thread_entry, s, stack_bytes, RHU_SHIM_PRIO_FLAT, NULL);
    }
    unsigned retries = 0u;
    while (tid == RHU_EAGAIN && retries < SPAWN_RETRY_TRIES) {
        rhu_sleep_ns(SPAWN_RETRY_STEP_NS);
        retries++;
        tid = rhu_thread_spawn(thread_entry, s, stack_bytes, RHU_SHIM_PRIO_FLAT, NULL);
    }
    if (tid < 0) {
        rhu_kheap_free(s);
        /* The door answers one code for four reasons; the kernel's own
         * [sched] line above carries the pool's numbers and the reason,
         * this line the request's story in the same words. */
        rhu_shim_log("pthread_create: refused by the kernel after %u tries: a stack of %u B asked (the row's default %u B, the clamp %u B); the [sched] line above names the reason -> EAGAIN",
                     retries + 1u, (unsigned)stack_bytes, (unsigned)row_stack_default(), (unsigned)MGC_ROW_STACK_CLAMP);
        return RHU_GERR_EAGAIN;
    }
    if (retries != 0u)
        rhu_shim_log("pthread_create: the spawn was refused and then accepted after %u retries (a stack given back)", retries);
    rhu_shim_log("pthread_create: stack %u B asked, %u B spawned (tid %d)",
                 (unsigned)asked, (unsigned)stack_bytes, tid);
    if (tid_out != NULL)
        *tid_out = (uint32_t)tid;
    return 0;
}

int pthread_create(uint32_t *out, const void *attr, void *(*fn)(void *), void *arg)
{
    const attr_slot *s = (const attr_slot *)attr;
    uint32_t stack_bytes = (s != NULL && s->magic == MAGIC_ATTR) ? s->stack_bytes : row_stack_default();
    return spawn_thread(stack_bytes, fn, arg, out);
}

int rhu_shim_create_thread(size_t stack_bytes, void *(*fn)(void *), void *arg)
{
    return spawn_thread((uint32_t)stack_bytes, fn, arg, NULL);
}

uint32_t pthread_self(void)
{
    return (uint32_t)rhu_thread_self();
}

int pthread_equal(uint32_t a, uint32_t b)
{
    return a == b;
}

int pthread_setname_np(uint32_t tid, const char *name)
{
    (void)tid;
    (void)name;
    return 0;
}

/* --- thread-specific keys ---
 * Values live in the per-thread block; destructors run on the exiting
 * thread through the kernel's exit-destructor stack, in the glibc shape:
 * up to four rounds, each value cleared before its destructor runs. */
static void (*g_key_dtor[SHIM_KEYS])(void *);
static uint8_t g_key_used[SHIM_KEYS];

static void keys_run_dtors(void *unused)
{
    (void)unused;
    shim_block *b = block();
    for (int round = 0; round < 4; round++) {
        int ran = 0;
        for (uint32_t k = 0; k < SHIM_KEYS; k++) {
            if (g_key_used[k] != 0 && g_key_dtor[k] != NULL && b->keys[k] != NULL) {
                void *v = b->keys[k];
                b->keys[k] = NULL;
                g_key_dtor[k](v);
                ran = 1;
            }
        }
        if (ran == 0)
            break;
    }
}

int pthread_key_create(uint32_t *key, void (*dtor)(void *))
{
    for (uint32_t k = 0; k < SHIM_KEYS; k++) {
        if (g_key_used[k] == 0) {
            g_key_used[k] = 1;
            g_key_dtor[k] = dtor;
            *key = k;
            return 0;
        }
    }
    rhu_shim_log("pthread_key_create: all %u keys in use (EAGAIN)", (unsigned)SHIM_KEYS);
    return RHU_GERR_EAGAIN;
}

int pthread_setspecific(uint32_t key, const void *value)
{
    if (key >= SHIM_KEYS || g_key_used[key] == 0)
        return RHU_GERR_EINVAL;
    shim_block *b = block();
    if ((b->flags & SHIM_FLAG_DTORS_ARMED) == 0) {
        b->flags |= SHIM_FLAG_DTORS_ARMED;
        if (rhu_thread_atexit(keys_run_dtors, NULL) != 0) {
            rhu_shim_log("pthread_setspecific: no exit-destructor slot left (EAGAIN)");
            b->flags &= ~SHIM_FLAG_DTORS_ARMED;
            return RHU_GERR_EAGAIN;
        }
    }
    b->keys[key] = (void *)value;
    return 0;
}

void *pthread_getspecific(uint32_t key)
{
    if (key >= SHIM_KEYS || g_key_used[key] == 0)
        return NULL;
    return block()->keys[key];
}

/* ------------------------------------------------------------------ */
/* thread-local storage: the image's own module                         */
/* ------------------------------------------------------------------ */

/* A global-dynamic access names a module and an offset; module 1 is the
 * image's own thread-local template, which the kernel registers as its
 * module 0 from the link brackets and copies for every thread before the
 * thread runs. No other module exists at this hosting. */
typedef struct tls_index {
    unsigned long ti_module;
    unsigned long ti_offset;
} tls_index;

void *__tls_get_addr(tls_index *ti)
{
    if (ti->ti_module != 1u) {
        rhu_shim_last_words("TLS module %lu is not wired (only the image's own module 1 exists)", ti->ti_module);
        rhu_exit(121);
    }
#if defined(RHU_SHIM_BREAK_TLSOFF)
    /* Built with this define, the offset is dropped, so every thread-local
     * symbol resolves to the front of the block. */
    return rhu_tls_get(0);
#elif defined(RHU_SHIM_BREAK_TLSCACHE)
    /* Built with this define, the first block seen is remembered and handed
     * to every thread after it, which is why the per-thread copy is checked
     * from a second thread rather than from the caller alone. */
    static void *first;
    if (first == NULL)
        first = rhu_tls_get(0);
    return (uint8_t *)first + ti->ti_offset;
#else
    return (uint8_t *)rhu_tls_get(0) + ti->ti_offset;
#endif
}

/* ------------------------------------------------------------------ */
/* scheduling, sysconf, identities                                      */
/* ------------------------------------------------------------------ */

int sched_yield(void)
{
    rhu_thread_yield();
    return 0;
}

int sched_getcpu(void)
{
    return 0;
}

int sched_setaffinity(int pid, size_t bytes, const void *set)
{
    (void)pid;
    (void)bytes;
    (void)set;
    return 0;
}

int sched_getaffinity(int pid, size_t bytes, void *set)
{
    (void)pid;
    memset(set, 0, bytes);
    if (bytes != 0)
        ((uint8_t *)set)[0] = 1;
    return 0;
}

void *__sched_cpualloc(size_t count)
{
    (void)count;
    return rhu_kheap_alloc(16u, 8u);
}

void __sched_cpufree(void *set)
{
    rhu_kheap_free(set);
}

int __sched_cpucount(size_t bytes, const void *set)
{
    (void)bytes;
    (void)set;
    return 1;
}

/* glibc's sysconf names: the page size, the processor counts, and the
 * physical / available page counts the collector's memory-load probe
 * reads -- answered from the managed heap's window (chosen at
 * initialization above): its size and its live headroom. */
#define GLIBC_SC_PAGESIZE 30
#define GLIBC_SC_NPROCESSORS_CONF 83
#define GLIBC_SC_NPROCESSORS_ONLN 84
#define GLIBC_SC_PHYS_PAGES 85
#define GLIBC_SC_AVPHYS_PAGES 86

long sysconf(int name)
{
    switch (name) {
    case GLIBC_SC_PAGESIZE:
        return (long)PAGE_BYTES;
    case GLIBC_SC_NPROCESSORS_CONF:
    case GLIBC_SC_NPROCESSORS_ONLN:
        return 1;
    case GLIBC_SC_PHYS_PAGES:
        return (long)(heap_window_bytes() / PAGE_BYTES);
    case GLIBC_SC_AVPHYS_PAGES:
        return (long)(heap_window_free() / PAGE_BYTES);
    default:
        rhu_shim_log("sysconf(%d): not available (EINVAL)", name);
        SET_ERRNO(RHU_GERR_EINVAL);
        return -1;
    }
}

int getpid(void)
{
    return 1;
}

/* The ARM EABI system call table's number for the thread id: the runtime
 * keys its signal-safe thread map on it, so it must answer. */
#define ARM_EABI_NR_GETTID 224

long syscall(long number, ...)
{
    if (number == ARM_EABI_NR_GETTID)
        return (long)rhu_thread_self();
    rhu_shim_log("syscall %ld: not available (ENOSYS)", number);
    SET_ERRNO(RHU_GERR_ENOSYS);
    return -1;
}

/* ------------------------------------------------------------------ */
/* signals: the activation costume                                     */
/* ------------------------------------------------------------------ */

/* The runtime asks for its thread activations in the shape of a realtime
 * signal: it records a three-argument handler for the first application
 * realtime signal, sends that signal to a thread, and expects the handler
 * to run on that thread with a signal information record naming this
 * process and a user context carrying the interrupted registers, which it
 * may edit before returning. Below that surface the kernel's activation
 * delivery does the work: one process-global handler, entered on the
 * target's own stack with the interrupted integer context, resumed from
 * the possibly edited context on return. The costume here builds the two
 * records the handler expects on the target's stack, calls the recorded
 * disposition, and writes the integer registers back; the stack pointer
 * and the status word are never written back (frame geometry never
 * moves), and an edit to either is reported once.
 *
 * No signal of any other number can arrive on this platform: dispositions
 * for them are recorded and read back consistently, and sending one is
 * refused. The signal mask is per-thread bookkeeping: nothing masks the
 * kernel's activation, and the recorded mask rides the user context so a
 * caller sees its own bookkeeping.
 *
 * The activation's stack cost on the target thread is the two records
 * (the user context and the signal information, 872 bytes together) plus
 * the kernel's own frames and the recorded handler's need; spawn stacks
 * are sized with that in mind. No coprocessor frame is recorded in the
 * context: the interrupted floating-point state stays in the kernel's
 * frames untouched, and the runtime reads none of it. */

#define SHIM_ACTIVATION_SIGNAL RHU_GLIBC_SIGRTMIN_CURRENT

typedef struct sig_disposition {
    void *handler;
    int32_t flags;
    uint32_t noted;
} sig_disposition;

static sig_disposition g_sig[RHU_GLIBC_NSIG];
static uint32_t g_activation_armed;
static uint32_t g_activation_count;
#ifndef RHU_SHIM_BREAK_FAULT_UNARMED
static uint32_t g_fault_armed;   /* the kernel's null-fault delivery armed (sticky) */
#endif
static uint32_t g_fault_count;   /* faults delivered to the runtime's handler (never counted as activations) */
#define SHIM_FAULT_EXIT 139      /* the shim's own fault deaths: 128 + SIGSEGV, the Linux shell's word */

uint32_t rhu_shim_activation_count(void)
{
    return g_activation_count;
}
static uint32_t g_sp_edit_noted;
static uint32_t g_status_edit_noted;

static int sig_in_range(int signo)
{
    return signo >= 1 && signo < RHU_GLIBC_NSIG;
}

static int handler_is_recorded(void *h)
{
    return h != RHU_GLIBC_SIG_DFL && h != RHU_GLIBC_SIG_IGN;
}

/* The stack-pointer trampoline. The runtime's handler may
 * move the faulting thread's stack pointer UP before it throws (the
 * interface-dispatch helper's pop of its two pushed words); the kernel never
 * writes sp back, so the thread resumes here with r3 = the delta and r12 = the
 * handler's pc (both scratch at RhpThrowHwEx's entry, which reads r0, r1 and
 * sp and keeps r4 to r11 as the fault's). */
__attribute__((naked, noinline)) static void rhu_fault_sp_tramp(void)
{
    __asm__ volatile("add sp, sp, r3\n\tbx r12");
}

/* The fault costume: a delivered null data access,
 * dressed as the Linux record the runtime's handler reads -- SIGSEGV,
 * SEGV_MAPERR, the faulting address in si_addr, the registers as today.
 * The two deaths the rule names: no real handler recorded, or the
 * runtime's decline (the pc unchanged and the disposition restored to the
 * default: on Linux the access would run again and kill the process; here the
 * kernel would deliver again, so the costume ends the process itself, loud).
 * An upward stack-pointer edit within the thread's carve is honoured through
 * the trampoline; any other edit is the loud end. */
static void fault_costume(rhu_fault_ctx *f)
{
    rhu_ctx *k = &f->ctx;
    rhu_glibc_ucontext uc;
    rhu_glibc_siginfo si;
    shim_block *b = block();
    g_fault_count++;
    if (RHU_FAULT_KIND(k->flags) != RHU_FAULT_NULL) {
        rhu_shim_last_words("SIGSEGV: a fault of kind %u delivered; this layer arms the null kind alone; the process ends", RHU_FAULT_KIND(k->flags));
        rhu_exit(SHIM_FAULT_EXIT);
    }
    sig_disposition d = g_sig[RHU_GSIG_SIGSEGV];
    if (!handler_is_recorded(d.handler)) {
        rhu_shim_last_words("SIGSEGV: null access at 0x%08x from pc 0x%08x with the default disposition; the process ends", f->fault_addr, k->pc);
        rhu_exit(SHIM_FAULT_EXIT);
    }
    rhu_shim_log("SIGSEGV: null access at 0x%08x delivered to the runtime's handler", f->fault_addr);
    memset(&uc, 0, sizeof uc);
    memset(&si, 0, sizeof si);
    si.si_signo = RHU_GSIG_SIGSEGV;
    si.si_code = 1; /* SEGV_MAPERR: the zero page is unmapped on Linux */
#ifdef RHU_SHIM_BREAK_FAULT_ADDR
    /* Built with this define, the address is one past the page, which the
     * runtime maps to a plain access violation, not the managed null
     * reference. Never set in a hosted image. */
    si.si_addr = 0x1000u;
#else
    si.si_addr = f->fault_addr;
#endif
    uc.uc_mcontext.oldmask = (uint32_t)b->sigmask;
    uc.uc_mcontext.arm_r0 = k->r[0];
    uc.uc_mcontext.arm_r1 = k->r[1];
    uc.uc_mcontext.arm_r2 = k->r[2];
    uc.uc_mcontext.arm_r3 = k->r[3];
    uc.uc_mcontext.arm_r4 = k->r[4];
    uc.uc_mcontext.arm_r5 = k->r[5];
    uc.uc_mcontext.arm_r6 = k->r[6];
    uc.uc_mcontext.arm_r7 = k->r[7];
    uc.uc_mcontext.arm_r8 = k->r[8];
    uc.uc_mcontext.arm_r9 = k->r[9];
    uc.uc_mcontext.arm_r10 = k->r[10];
    uc.uc_mcontext.arm_fp = k->r[11];
    uc.uc_mcontext.arm_ip = k->r[12];
    uc.uc_mcontext.arm_sp = k->sp;
    uc.uc_mcontext.arm_lr = k->lr;
    uc.uc_mcontext.arm_pc = k->pc;
    uc.uc_mcontext.arm_cpsr = k->xpsr;
    uc.uc_sigmask.word[0] = (uint32_t)b->sigmask;
    uc.uc_sigmask.word[1] = (uint32_t)(b->sigmask >> 32);
    if ((d.flags & RHU_GLIBC_SA_SIGINFO) != 0)
        ((void (*)(int, rhu_glibc_siginfo *, void *))d.handler)(RHU_GSIG_SIGSEGV, &si, &uc);
    else
        ((void (*)(int))d.handler)(RHU_GSIG_SIGSEGV);
    if (uc.uc_mcontext.arm_pc == k->pc && !handler_is_recorded(g_sig[RHU_GSIG_SIGSEGV].handler)) {
        rhu_shim_last_words("SIGSEGV: null access at 0x%08x from pc 0x%08x declined by the runtime's handler (the faulting instruction is outside managed code); the process ends", f->fault_addr, k->pc);
        rhu_exit(SHIM_FAULT_EXIT);
    }
    k->r[0] = uc.uc_mcontext.arm_r0;
    k->r[1] = uc.uc_mcontext.arm_r1;
    k->r[2] = uc.uc_mcontext.arm_r2;
    k->r[3] = uc.uc_mcontext.arm_r3;
    k->r[4] = uc.uc_mcontext.arm_r4;
    k->r[5] = uc.uc_mcontext.arm_r5;
    k->r[6] = uc.uc_mcontext.arm_r6;
    k->r[7] = uc.uc_mcontext.arm_r7;
    k->r[8] = uc.uc_mcontext.arm_r8;
    k->r[9] = uc.uc_mcontext.arm_r9;
    k->r[10] = uc.uc_mcontext.arm_r10;
    k->r[11] = uc.uc_mcontext.arm_fp;
    k->r[12] = uc.uc_mcontext.arm_ip;
    k->lr = uc.uc_mcontext.arm_lr;
    k->pc = uc.uc_mcontext.arm_pc;
    if (uc.uc_mcontext.arm_cpsr != k->xpsr && g_status_edit_noted++ == 0)
        rhu_shim_log("activation: a status-register edit in the signal context is ignored");
    if (uc.uc_mcontext.arm_sp != k->sp) {
        int32_t delta = (int32_t)(uc.uc_mcontext.arm_sp - k->sp);
        uint32_t lo = 0;
        uint32_t len = 0;
        int in_carve = rhu_thread_stack_bounds(rhu_thread_self(), &lo, &len) == 0 && delta > 0 &&
                       k->sp >= lo && (uint32_t)delta <= lo + len - k->sp;
        if (!in_carve) {
            rhu_shim_last_words("SIGSEGV: the runtime's handler moved the stack pointer by %d bytes outside the thread's stack; the process ends", delta);
            rhu_exit(SHIM_FAULT_EXIT);
        }
        k->r[3] = (uint32_t)delta;
        k->r[12] = k->pc | 1u;
        k->pc = (uint32_t)(uintptr_t)rhu_fault_sp_tramp;
    }
}

/* The kernel's activation handler: the costume proper (the fault branch
 * first: a delivered fault context carries the flag). */
static void act_costume(rhu_ctx *k)
{
    rhu_glibc_ucontext uc;
    rhu_glibc_siginfo si;
    shim_block *b = block();
    if ((k->flags & RHU_CTX_FAULT) != 0) {
        fault_costume((rhu_fault_ctx *)k);
        return;
    }
    g_activation_count++;
    memset(&uc, 0, sizeof uc);
    memset(&si, 0, sizeof si);
    si.si_signo = SHIM_ACTIVATION_SIGNAL;
    si.si_code = RHU_GLIBC_SI_TKILL;
#ifdef RHU_SHIM_BREAK_SIPID
    /* Built with this define, the record names a foreign process, a record
     * the runtime drops. Never set in a hosted image. */
    si.si_pid = 0;
#else
    si.si_pid = getpid();
#endif
    uc.uc_mcontext.oldmask = (uint32_t)b->sigmask;
    uc.uc_mcontext.arm_r0 = k->r[0];
    uc.uc_mcontext.arm_r1 = k->r[1];
    uc.uc_mcontext.arm_r2 = k->r[2];
    uc.uc_mcontext.arm_r3 = k->r[3];
    uc.uc_mcontext.arm_r4 = k->r[4];
    uc.uc_mcontext.arm_r5 = k->r[5];
    uc.uc_mcontext.arm_r6 = k->r[6];
    uc.uc_mcontext.arm_r7 = k->r[7];
    uc.uc_mcontext.arm_r8 = k->r[8];
    uc.uc_mcontext.arm_r9 = k->r[9];
    uc.uc_mcontext.arm_r10 = k->r[10];
    uc.uc_mcontext.arm_fp = k->r[11];
    uc.uc_mcontext.arm_ip = k->r[12];
    uc.uc_mcontext.arm_sp = k->sp;
    uc.uc_mcontext.arm_lr = k->lr;
#ifdef RHU_SHIM_BREAK_CTXPC
    /* Built with this define, the program counter slot carries the link
     * register, a wrong-field copy a layout assert cannot see. Never set in a
     * hosted image. */
    uc.uc_mcontext.arm_pc = k->lr;
#else
    uc.uc_mcontext.arm_pc = k->pc;
#endif
    uc.uc_mcontext.arm_cpsr = k->xpsr;
    uc.uc_sigmask.word[0] = (uint32_t)b->sigmask;
    uc.uc_sigmask.word[1] = (uint32_t)(b->sigmask >> 32);
    sig_disposition d = g_sig[SHIM_ACTIVATION_SIGNAL];
    if (!handler_is_recorded(d.handler)) {
        rhu_shim_log("activation: delivered with no handler recorded for signal %d; ignored", SHIM_ACTIVATION_SIGNAL);
        return;
    }
    if ((d.flags & RHU_GLIBC_SA_SIGINFO) != 0)
        ((void (*)(int, rhu_glibc_siginfo *, void *))d.handler)(SHIM_ACTIVATION_SIGNAL, &si, &uc);
    else
        ((void (*)(int))d.handler)(SHIM_ACTIVATION_SIGNAL);
    k->r[0] = uc.uc_mcontext.arm_r0;
    k->r[1] = uc.uc_mcontext.arm_r1;
    k->r[2] = uc.uc_mcontext.arm_r2;
    k->r[3] = uc.uc_mcontext.arm_r3;
    k->r[4] = uc.uc_mcontext.arm_r4;
    k->r[5] = uc.uc_mcontext.arm_r5;
    k->r[6] = uc.uc_mcontext.arm_r6;
    k->r[7] = uc.uc_mcontext.arm_r7;
    k->r[8] = uc.uc_mcontext.arm_r8;
    k->r[9] = uc.uc_mcontext.arm_r9;
    k->r[10] = uc.uc_mcontext.arm_r10;
    k->r[11] = uc.uc_mcontext.arm_fp;
    k->r[12] = uc.uc_mcontext.arm_ip;
    k->lr = uc.uc_mcontext.arm_lr;
    k->pc = uc.uc_mcontext.arm_pc;
    if (uc.uc_mcontext.arm_sp != k->sp && g_sp_edit_noted++ == 0)
        rhu_shim_log("activation: a stack-pointer edit in the signal context is ignored (frame geometry never moves)");
    if (uc.uc_mcontext.arm_cpsr != k->xpsr && g_status_edit_noted++ == 0)
        rhu_shim_log("activation: a status-register edit in the signal context is ignored");
}

/* Record a disposition. The query form reports the recorded handler and
 * flags with an empty mask. The first recording of a real handler for the
 * activation signal arms the kernel's delivery; for any other signal it
 * states that nothing of that number can arrive. */
int sigaction(int signo, const rhu_glibc_sigaction *act, rhu_glibc_sigaction *oldact)
{
    if (!sig_in_range(signo) || signo == RHU_GSIG_SIGKILL || signo == RHU_GSIG_SIGSTOP) {
        SET_ERRNO(RHU_GERR_EINVAL);
        return -1;
    }
    if (oldact != NULL) {
        memset(oldact, 0, sizeof *oldact);
        oldact->sa_handler = g_sig[signo].handler;
        oldact->sa_flags = g_sig[signo].flags;
    }
    if (act == NULL)
        return 0;
    g_sig[signo].handler = act->sa_handler;
    g_sig[signo].flags = act->sa_flags;
    if (handler_is_recorded(act->sa_handler) && g_sig[signo].noted == 0) {
        g_sig[signo].noted = 1;
        if (signo == SHIM_ACTIVATION_SIGNAL) {
            if (g_activation_armed == 0) {
                int32_t rc = rhu_activation_register(act_costume);
                if (rc != 0) {
                    rhu_shim_log("activation: the kernel refused the handler registration (%d)", rc);
                    rhu_exit(122);
                }
                g_activation_armed = 1;
            }
            rhu_shim_log("activation: signal %d handler recorded; the kernel's activation delivery is armed", signo);
        } else if (signo == RHU_GSIG_SIGSEGV) {
            /* The activation entry first (the kernel refuses a
             * kind armed with no entry), then the null kind, once; sticky. */
            if (g_activation_armed == 0) {
                int32_t rc = rhu_activation_register(act_costume);
                if (rc != 0) {
                    rhu_shim_log("activation: the kernel refused the handler registration (%d)", rc);
                    rhu_exit(122);
                }
                g_activation_armed = 1;
            }
#ifdef RHU_SHIM_BREAK_FAULT_UNARMED
            /* Built with this define, the disposition is recorded and the kernel
             * call never made, so the first null access ends the program as the
             * kernel designs it. Never set in a hosted image. */
            rhu_shim_log("sigaction(%d): handler recorded; this platform delivers no signal of that number", signo);
#else
            if (g_fault_armed == 0) {
                int32_t rc = rhu_fault_register(RHU_FAULT_NULL);
                if (rc != 0) {
                    rhu_shim_log("SIGSEGV: the kernel refused the null-fault delivery's arming (%d)", rc);
                    rhu_exit(122);
                }
                g_fault_armed = 1;
            }
            rhu_shim_log("sigaction(%d): handler recorded; the kernel's null-fault delivery is armed", signo);
#endif
        } else {
            rhu_shim_log("sigaction(%d): handler recorded; this platform delivers no signal of that number", signo);
        }
    }
    return 0;
}

/* The one-argument form: a disposition with restartable calls, answering
 * the previous handler, or the error handler value on a refusal. */
void *signal(int signo, void *handler)
{
    rhu_glibc_sigaction act;
    rhu_glibc_sigaction old;
    memset(&act, 0, sizeof act);
    act.sa_handler = handler;
    act.sa_flags = RHU_GLIBC_SA_RESTART;
    if (sigaction(signo, &act, &old) != 0)
        return RHU_GLIBC_SIG_ERR;
    return old.sa_handler;
}

static int sig_bit(int signo, uint32_t *word, uint32_t *bit)
{
    if (!sig_in_range(signo))
        return 0;
    *word = (uint32_t)(signo - 1) / 32u;
    *bit = 1u << ((uint32_t)(signo - 1) % 32u);
    return 1;
}

int sigemptyset(rhu_glibc_sigset *set)
{
    memset(set, 0, sizeof *set);
    return 0;
}

int sigfillset(rhu_glibc_sigset *set)
{
    memset(set, 0xFF, sizeof *set);
    return 0;
}

int sigaddset(rhu_glibc_sigset *set, int signo)
{
    uint32_t w;
    uint32_t b;
    if (!sig_bit(signo, &w, &b)) {
        SET_ERRNO(RHU_GERR_EINVAL);
        return -1;
    }
    set->word[w] |= b;
    return 0;
}

int sigdelset(rhu_glibc_sigset *set, int signo)
{
    uint32_t w;
    uint32_t b;
    if (!sig_bit(signo, &w, &b)) {
        SET_ERRNO(RHU_GERR_EINVAL);
        return -1;
    }
    set->word[w] &= ~b;
    return 0;
}

int sigismember(const rhu_glibc_sigset *set, int signo)
{
    uint32_t w;
    uint32_t b;
    if (!sig_bit(signo, &w, &b)) {
        SET_ERRNO(RHU_GERR_EINVAL);
        return -1;
    }
    return (set->word[w] & b) != 0;
}

/* The calling thread's mask: bookkeeping over signals 1..64, reported back
 * on request. Returns the errno value for an unknown operation. */
int pthread_sigmask(int how, const rhu_glibc_sigset *set, rhu_glibc_sigset *old)
{
    shim_block *b = block();
    if (old != NULL) {
        memset(old, 0, sizeof *old);
        old->word[0] = (uint32_t)b->sigmask;
        old->word[1] = (uint32_t)(b->sigmask >> 32);
    }
    if (set == NULL)
        return 0;
    uint64_t m = (uint64_t)set->word[0] | ((uint64_t)set->word[1] << 32);
    switch (how) {
    case RHU_GLIBC_SIG_BLOCK:
        b->sigmask |= m;
        return 0;
    case RHU_GLIBC_SIG_UNBLOCK:
        b->sigmask &= ~m;
        return 0;
    case RHU_GLIBC_SIG_SETMASK:
        b->sigmask = m;
        return 0;
    default:
        rhu_shim_log("pthread_sigmask: unknown operation %d (EINVAL)", how);
        return RHU_GERR_EINVAL;
    }
}

/* Send a signal to a thread: signal 0 probes the thread's existence, the
 * activation signal is the kernel's activation send, and nothing else can
 * be delivered on this platform. Returns an errno value. */
int pthread_kill(uint32_t tid, int sig)
{
    if (sig == 0) {
        uint32_t base = 0;
        uint32_t len = 0;
        return rhu_thread_stack_bounds((int32_t)tid, &base, &len) == 0 ? 0 : RHU_GERR_ESRCH;
    }
    if (sig != SHIM_ACTIVATION_SIGNAL) {
        rhu_shim_log("pthread_kill(%u, %d): only the activation signal is deliverable on this platform (EINVAL)",
                     (unsigned)tid, sig);
        return RHU_GERR_EINVAL;
    }
    if (g_activation_armed == 0) {
        rhu_shim_log("pthread_kill(%u, %d): no activation handler recorded (EINVAL)", (unsigned)tid, sig);
        return RHU_GERR_EINVAL;
    }
    int32_t rc = rhu_activation_send((int32_t)tid);
    if (rc == 0)
        return 0;
    if (rc == RHU_ESRCH)
        return RHU_GERR_ESRCH;
    return RHU_GERR_EINVAL;
}

int __libc_current_sigrtmin(void)
{
    return RHU_GLIBC_SIGRTMIN_CURRENT;
}

int __libc_current_sigrtmax(void)
{
    return RHU_GLIBC_SIGRTMAX;
}

/* ------------------------------------------------------------------ */
/* entropy: the cryptographic sources on the kernel's secure door       */
/* ------------------------------------------------------------------ */

/* The kernel's secure door asked in pieces of at most its bound, all or
 * nothing: the door's first refusal ends the draw, clears the bytes
 * already served so the buffer never carries a partial draw, and is
 * returned. */
static int32_t secure_fill(void *buf, size_t len)
{
    uint8_t *p = (uint8_t *)buf;
    size_t off = 0;
    while (off < len) {
        size_t piece = len - off;
        if (piece > RHU_ENTROPY_SECURE_MAX)
            piece = RHU_ENTROPY_SECURE_MAX;
        int32_t r = rhu_entropy_secure(p == NULL ? NULL : p + off, (uint32_t)piece);
        if (r != 0) {
            if (p != NULL && off > 0)
                memset(p, 0, off);
            return r;
        }
        off += piece;
    }
    return 0;
}

/* The C library's cryptographic sources -- the runtime's portability
 * layer draws its cryptographic bytes through getrandom, then getentropy
 * -- take the secure door; a refusal is EIO for the two with an error
 * channel. getentropy serves at most 256 bytes per call by its own
 * contract. */
int getentropy(void *buf, size_t len)
{
    if (len > 256u || secure_fill(buf, len) != 0) {
        SET_ERRNO(RHU_GERR_EIO);
        return -1;
    }
    return 0;
}

long getrandom(void *buf, size_t len, unsigned flags)
{
    (void)flags;
    if (secure_fill(buf, len) != 0) {
        SET_ERRNO(RHU_GERR_EIO);
        return -1;
    }
    return (long)len;
}

/* No error channel by contract: callers make keys from these bytes, so a
 * failed draw ends the process rather than returning weak bytes. */
void arc4random_buf(void *buf, size_t len)
{
    if (secure_fill(buf, len) != 0) {
        rhu_shim_last_words("arc4random_buf: the kernel's secure entropy call refused; refusing to return weak bytes");
        rhu_abort(134);
    }
}

/* ------------------------------------------------------------------ */
/* the auxiliary vector: no entry exists on this platform              */
/* ------------------------------------------------------------------ */

/* The runtime's portability layer asks the auxiliary vector for the
 * hardware capability words and the executable's path. This platform
 * has no auxiliary vector, so every entry is absent, and the C library's
 * contract for an absent entry is 0 with ENOENT. */
unsigned long getauxval(unsigned long type)
{
    (void)type;
    SET_ERRNO(RHU_GERR_ENOENT);
    return 0;
}

/* ------------------------------------------------------------------ */
/* the dynamic-linking surface: the kernel's module registry            */
/* ------------------------------------------------------------------ */

typedef struct elf32_phdr {
    uint32_t p_type, p_offset, p_vaddr, p_paddr, p_filesz, p_memsz, p_flags, p_align;
} elf32_phdr;

typedef struct dl_phdr_info {
    uint32_t dlpi_addr;
    const char *dlpi_name;
    const elf32_phdr *dlpi_phdr;
    uint16_t dlpi_phnum;
    unsigned long long dlpi_adds;
    unsigned long long dlpi_subs;
    uint32_t dlpi_tls_modid;
    void *dlpi_tls_data;
} dl_phdr_info;

#define PT_LOAD 1u
#define PT_ARM_EXIDX 0x70000001u
#define PF_R 4u
#define PF_X 1u

/* The execute-in-place window of a board whose map places the hosted
 * program's code in a memory-mapped flash (the STM32F769 under its
 * execute-in-place map): the window's text between __rhu_xip_lo and
 * __rhu_xip_hi, its exception-index table between __rhu_xip_exidx_start
 * and __rhu_xip_exidx_end. The four are weak references: a map that
 * declares no window leaves them undefined (read as NULL), provides the
 * text pair as 0 or brackets an empty window, and the table below then
 * holds the registered modules alone. */
extern char __rhu_xip_lo[] __attribute__((weak));
extern char __rhu_xip_hi[] __attribute__((weak));
extern char __rhu_xip_exidx_start[] __attribute__((weak));
extern char __rhu_xip_exidx_end[] __attribute__((weak));

typedef struct dl_mod {
    uint32_t lo, hi, exidx_lo, exidx_hi;
} dl_mod;

/* Module j is strictly more specific than module i when its text span is
 * smaller (ties broken by the lower handle). */
static int dl_clips(const dl_mod *m, int i, int j)
{
    if (j == i)
        return 0;
    uint32_t si = m[i].hi - m[i].lo;
    uint32_t sj = m[j].hi - m[j].lo;
    return sj < si || (sj == si && j < i);
}

/* One entry per registered code module: its text as load segments, and
 * its unwind index as the ARM exception-index segment; then, when the
 * image's map brackets an execute-in-place window, one entry for the
 * window -- its text and its own exception-index table -- so an
 * unwinder finds the entries of the code that runs from it. A program
 * counter belongs to the smallest module containing it, so a module's
 * load segments are emitted as its span minus every more specific
 * module's span -- an unwinder then never consults a foreign index for
 * it. */
int dl_iterate_phdr(int (*cb)(dl_phdr_info *, size_t, void *), void *data)
{
    dl_mod m[RHU_MODULE_MAX + 1];
    int n = 0;
    for (int32_t h = 0; h < RHU_MODULE_MAX; h++) {
        rhu_module_desc d;
        if (rhu_module_roots(h, &d) != 0)
            break;
        if (d.text_hi <= d.text_lo)
            continue;
        m[n].lo = d.text_lo;
        m[n].hi = d.text_hi;
        m[n].exidx_lo = d.exidx_lo;
        m[n].exidx_hi = d.exidx_hi;
        n++;
    }
    uint32_t xlo = (uint32_t)(uintptr_t)__rhu_xip_lo;
    uint32_t xhi = (uint32_t)(uintptr_t)__rhu_xip_hi;
    if (xhi > xlo) {
        m[n].lo = xlo;
        m[n].hi = xhi;
        m[n].exidx_lo = (uint32_t)(uintptr_t)__rhu_xip_exidx_start;
        m[n].exidx_hi = (uint32_t)(uintptr_t)__rhu_xip_exidx_end;
        n++;
    }
    for (int i = 0; i < n; i++) {
        elf32_phdr ph[RHU_MODULE_MAX + 3];
        int np = 0;
        uint32_t cur = m[i].lo;
        memset(ph, 0, sizeof ph);
        while (cur < m[i].hi && np < RHU_MODULE_MAX + 2) {
            int moved = 1;
            while (moved != 0) {
                moved = 0;
                for (int j = 0; j < n; j++) {
                    if (dl_clips(m, i, j) && m[j].lo <= cur && cur < m[j].hi) {
                        cur = m[j].hi;
                        moved = 1;
                    }
                }
            }
            if (cur >= m[i].hi)
                break;
            uint32_t end = m[i].hi;
            for (int j = 0; j < n; j++) {
                if (dl_clips(m, i, j) && m[j].lo > cur && m[j].lo < end)
                    end = m[j].lo;
            }
            ph[np].p_type = PT_LOAD;
            ph[np].p_vaddr = cur;
            ph[np].p_paddr = cur;
            ph[np].p_filesz = end - cur;
            ph[np].p_memsz = end - cur;
            ph[np].p_flags = PF_R | PF_X;
            ph[np].p_align = 4;
            np++;
            cur = end;
        }
        ph[np].p_type = PT_ARM_EXIDX;
        ph[np].p_vaddr = m[i].exidx_lo;
        ph[np].p_paddr = m[i].exidx_lo;
        ph[np].p_filesz = m[i].exidx_hi - m[i].exidx_lo;
        ph[np].p_memsz = m[i].exidx_hi - m[i].exidx_lo;
        ph[np].p_flags = PF_R;
        ph[np].p_align = 4;
        np++;
        dl_phdr_info info;
        memset(&info, 0, sizeof info);
        info.dlpi_addr = 0;
        info.dlpi_name = "";
        info.dlpi_phdr = ph;
        info.dlpi_phnum = (uint16_t)np;
        int r = cb(&info, sizeof info, data);
        if (r != 0)
            return r;
    }
    return 0;
}

/* The loader seam: an image that carries a standalone collector links a
 * strong pair of these hooks; every other image keeps the null
 * answers below, and a request the hooks do not serve is printed. */
void *__mgc_dlopen_hook(const char *file) __attribute__((weak));
void *__mgc_dlsym_hook(void *handle, const char *name) __attribute__((weak));

void *dlopen(const char *file, int mode)
{
    (void)mode;
    if (__mgc_dlopen_hook != NULL) {
        void *h = __mgc_dlopen_hook(file);
        if (h != NULL)
            return h;
    }
    rhu_shim_log("dlopen(%s): no loadable library by that name", file != NULL ? file : "(null)");
    return NULL;
}

void *dlsym(void *handle, const char *name)
{
    if (__mgc_dlsym_hook != NULL) {
        void *p = __mgc_dlsym_hook(handle, name);
        if (p != NULL)
            return p;
    }
    rhu_shim_log("dlsym(%s): no such entry point", name != NULL ? name : "(null)");
    return NULL;
}

char *dlerror(void)
{
    return (char *)"dynamic loading is not available on this platform";
}

/* The glibc arm32 personality's address-information record: four
 * pointer-sized fields -- the file name and base of the module holding
 * the address, then the nearest symbol's name and address. */
typedef struct rhu_glibc_dl_info {
    const char *dli_fname;
    void *dli_fbase;
    const char *dli_sname;
    void *dli_saddr;
} rhu_glibc_dl_info;

/* The image's load address -- the module base every reader of this
 * record shares (the host's registration, a debugger, this answer): the
 * address of the vector table's symbol, which every board map places at
 * the image base and the in-image header's load-address word repeats.
 * Read from no memory (on a board whose image base
 * is 0 the header lies inside the guarded null page). */
extern const uint32_t __vector_table[];

/* An address inside a registered module's code is answered with the
 * image's own name and its load address -- the one number the host
 * registered the module under. This image has no symbol table to
 * consult, so the nearest-symbol fields are null. Any other address is
 * not inside a module this hosting knows. */
int dladdr(const void *addr, void *info)
{
    rhu_module_desc d;
    rhu_glibc_dl_info *out = (rhu_glibc_dl_info *)info;
    if (out == NULL || rhu_module_find((uint32_t)(uintptr_t)addr, &d) < 0)
        return 0;
    out->dli_fname = "/rhu";
    out->dli_fbase = (void *)(uintptr_t)__vector_table;
    out->dli_sname = NULL;
    out->dli_saddr = NULL;
    return 1;
}

/* ------------------------------------------------------------------ */
/* the instruction-stream sync after code is written                   */
/* ------------------------------------------------------------------ */

/* The runtime calls this after writing a fresh block of call thunks into
 * memory it then executes. The data cache is off on every board this
 * hosting runs on, so the written bytes are in memory already; the
 * instruction cache is on where the kernel turned it on at boot (the
 * STM32F769 under its execute-in-place map), and a line of it may still
 * hold what the written range held before. When the configuration and
 * control register reads the instruction cache on (its IC bit), the
 * writes are drained and the cache is invalidated by address over the
 * range through the ICIMVAU register, one 32-byte line at a time; then,
 * as on every board, the two barriers order the writes before any fetch
 * of the new instructions and drain the pipeline of anything fetched
 * earlier. The emulator models no cache and the Cortex-M33 rows' cores
 * have none: the register reads the cache off there and nothing is
 * written to it. */
void __clear_cache(void *begin, void *end)
{
    volatile uint32_t *const ccr = (volatile uint32_t *)0xE000ED14u;
    if ((*ccr & (1u << 17)) != 0u) {
        volatile uint32_t *const icimvau = (volatile uint32_t *)0xE000EF58u;
        uint32_t line = (uint32_t)(uintptr_t)begin & ~31u;
        uint32_t stop = (uint32_t)(uintptr_t)end;
        __asm__ volatile("dsb sy" ::: "memory");
        for (; line < stop; line += 32u)
            *icimvau = line;
    }
    __asm__ volatile("dsb sy\n\tisb sy" ::: "memory");
}

/* ------------------------------------------------------------------ */
/* process-shaped requests: refused with the right errno              */
/* ------------------------------------------------------------------ */

int fork(void)
{
    rhu_shim_log("fork: not available (ENOSYS)");
    SET_ERRNO(RHU_GERR_ENOSYS);
    return -1;
}

int execv(const char *path, char *const argv[])
{
    (void)argv;
    rhu_shim_log("execv(%s): not available (ENOSYS)", path != NULL ? path : "(null)");
    SET_ERRNO(RHU_GERR_ENOSYS);
    return -1;
}

int waitpid(int pid, int *status, int options)
{
    (void)pid;
    (void)status;
    (void)options;
    SET_ERRNO(RHU_GERR_ECHILD);
    return -1;
}

int pipe(int fds[2])
{
    (void)fds;
    rhu_shim_log("pipe: not available (ENOSYS)");
    SET_ERRNO(RHU_GERR_ENOSYS);
    return -1;
}

int sysinfo(void *info)
{
    (void)info;
    SET_ERRNO(RHU_GERR_ENOSYS);
    return -1;
}

/* Every limit reads as unlimited: the two 64-bit fields are all ones. */
int getrlimit64(int resource, void *limits)
{
    (void)resource;
    memset(limits, 0xFF, 16u);
    return 0;
}

int statfs64(const char *path, void *buf)
{
    (void)path;
    (void)buf;
    SET_ERRNO(RHU_GERR_ENOSYS);
    return -1;
}

/* A process-control request: none exists on this platform, and the one
 * the runtime makes is on its crash-dump path. The option is named so the
 * line says which request was refused. */
int __prctl_time64(int option, ...)
{
    rhu_shim_log("prctl(%d): not available on this platform (EINVAL)", option);
    SET_ERRNO(RHU_GERR_EINVAL);
    return -1;
}

/* The second unwinding personality routine the runtime's unwind tables
 * name (the kernel parks the first). This hosting never unwinds a frame
 * through these routines: a managed exception is dispatched by the
 * runtime's own tables, and a native fault ends the run. Being entered
 * means a table was consulted by an unwinder that does not exist here,
 * so the line says so and the run ends. */
void __aeabi_unwind_cpp_pr1(void)
{
    rhu_shim_log("unwind: personality routine 1 entered; this image never unwinds");
    rhu_abort(134);
}

/* ------------------------------------------------------------------ */
/* file descriptors at the C-runtime level: the console, the bus device */
/* nodes (the bus device unit's, by the path's prefix and the device    */
/* band), and every other path refused with its errno -- the file      */
/* surface is behind the PAL                                           */
/* ------------------------------------------------------------------ */

static int fd_is_console(int fd)
{
    return fd >= RHU_SHIM_FD_CONSOLE_LO && fd < RHU_SHIM_FD_CONSOLE_HI;
}

static int fd_is_device(int fd)
{
    return fd >= RHU_SHIM_FD_DEVICE_LO && fd < RHU_SHIM_FD_DEVICE_LO + RHU_SHIM_FD_DEVICE_COUNT;
}

static int fd_is_tty(int fd)
{
    return fd >= RHU_SHIM_FD_TTY_LO && fd < RHU_SHIM_FD_TTY_LO + RHU_SHIM_FD_TTY_COUNT;
}

int open(const char *path, int flags, ...)
{
    if (rhu_ttydev_owns_path(path))
        return rhu_ttydev_open(path, flags);
    if (path != NULL && (strncmp(path, "/dev/i2c-", 9) == 0 || strncmp(path, "/dev/spidev", 11) == 0))
        return rhu_busdev_open(path, flags);
    rhu_shim_log("open(%s): no such path (ENOENT)", path != NULL ? path : "(null)");
    SET_ERRNO(RHU_GERR_ENOENT);
    return -1;
}

int open64(const char *path, int flags, ...)
{
    if (rhu_ttydev_owns_path(path))
        return rhu_ttydev_open(path, flags);
    rhu_shim_log("open(%s): no such path (ENOENT)", path != NULL ? path : "(null)");
    SET_ERRNO(RHU_GERR_ENOENT);
    return -1;
}

void *fopen64(const char *path, const char *mode)
{
    (void)mode;
    rhu_shim_log("fopen(%s): no such path (ENOENT)", path != NULL ? path : "(null)");
    SET_ERRNO(RHU_GERR_ENOENT);
    return NULL;
}

int close(int fd)
{
    if (fd_is_console(fd))
        return 0;
    if (fd_is_device(fd))
        return rhu_busdev_close(fd);
    if (fd_is_tty(fd))
        return rhu_ttydev_close(fd);
    SET_ERRNO(RHU_GERR_EBADF);
    return -1;
}

/* Every request the bus device unit serves takes a pointer argument; the
 * one it refuses without reading (the file-transfer form's address by
 * value) is read as a pointer and never dereferenced. The serial device
 * unit's requests take a pointer or a value in the same word; it reads
 * the word as its request defines it. */
int ioctl(int fd, unsigned long request, ...)
{
    if (fd_is_device(fd) || fd_is_tty(fd)) {
        va_list ap;
        va_start(ap, request);
        void *arg = va_arg(ap, void *);
        va_end(ap);
        return fd_is_tty(fd) ? rhu_ttydev_ioctl(fd, request, arg) : rhu_busdev_ioctl(fd, request, arg);
    }
    SET_ERRNO(RHU_GERR_ENOTTY);
    return -1;
}

void *opendir(const char *path)
{
    rhu_shim_log("opendir(%s): no such path (ENOENT)", path != NULL ? path : "(null)");
    SET_ERRNO(RHU_GERR_ENOENT);
    return NULL;
}

void *readdir64(void *dir)
{
    (void)dir;
    return NULL;
}

int closedir(void *dir)
{
    (void)dir;
    return 0;
}

/* The console descriptors may be duplicated onto each other (the runtime
 * pins its error stream this way); nothing else has a second name. */
int dup2(int oldfd, int newfd)
{
    if (fd_is_console(oldfd) && fd_is_console(newfd))
        return newfd;
    rhu_shim_log("dup2(%d, %d): only the console descriptors (EBADF)", oldfd, newfd);
    SET_ERRNO(RHU_GERR_EBADF);
    return -1;
}
