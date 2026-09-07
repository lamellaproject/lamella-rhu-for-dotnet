/* The glibc-personality core of the hosting shim: the C-runtime surface
 * the .NET runtime archives import, defined under the archives' own import
 * names on the kernel's hosting verbs. There is no C library underneath to
 * collide with, so nothing is renamed. errno lives in a per-thread block
 * held by the kernel's module registry; memory mapping is the kernel
 * arena; time is the kernel clock; locks are futex recipes held inline in
 * the glibc-sized slots; threads are kernel threads at one flat hosted
 * priority; thread-local storage is the kernel's per-module block; and
 * everything this platform does not have answers with the errno a caller
 * can act on and a diagnostic line, never a silent success.
 *
 * Two families of constants appear below. The glibc ABI facts (slot
 * sizes, clock ids, the sysconf and mmap flag numbers, the syscall number
 * for the thread id, the structure layouts) belong to the glibc arm32
 * personality and do not move with a runtime re-pin. The runtime-side
 * facts (errno identities, the board row's values) come from the generated
 * PAL headers and the board rows header and move with theirs. */
#include <stdarg.h>
#include <stddef.h>
#include <stdint.h>

#include "rhu_shim.h"

#include "mgc_board_rows.h"
#include "rhu.h"
#include "rhu_glibc_arm32.h"
#include "rhu_pal_errno_p7.h"

void *memcpy(void *dst, const void *src, size_t n);
void *memset(void *dst, int c, size_t n);
size_t strlen(const char *s);
int strncmp(const char *a, const char *b, size_t n);

/* ------------------------------------------------------------------ */
/* the diagnostic line writer                                          */
/* ------------------------------------------------------------------ */

#define LOG_LINE_BYTES 256u

typedef struct log_line {
    char buf[LOG_LINE_BYTES];
    uint32_t n;
} log_line;

static void line_put(log_line *l, char c)
{
    if (l->n < LOG_LINE_BYTES - 1u)
        l->buf[l->n++] = c;
}

static void line_puts(log_line *l, const char *s)
{
    while (*s != 0)
        line_put(l, *s++);
}

/* An unsigned value in base 10 or 16, at least `width` digits, zero-padded
 * when `pad0`; lowercase hex digits. */
static void line_num(log_line *l, unsigned long long v, unsigned base, unsigned width, int pad0)
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

static void line_format(log_line *l, const char *fmt, va_list ap)
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
            rhu_console_write(msg, (uint32_t)strlen(msg));
            rhu_console_write(fmt, (uint32_t)strlen(fmt));
            rhu_console_write("\n", 1u);
            rhu_exit(134);
        }
        }
    }
}

void rhu_shim_log(const char *fmt, ...)
{
    log_line l;
    l.n = 0;
    line_puts(&l, "[shim] ");
    va_list ap;
    va_start(ap, fmt);
    line_format(&l, fmt, ap);
    va_end(ap);
    line_put(&l, '\n');
    rhu_console_write(l.buf, l.n);
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
        rhu_shim_log("the per-thread block is not registered: rhu_shim_init must run first");
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
    rhu_shim_exit(134);
}

/* ------------------------------------------------------------------ */
/* the environment table                                               */
/* ------------------------------------------------------------------ */

/* The rows the runtime reads. The collector's aggregate hard limit is
 * generated from the board row's managed region at initialization, so the
 * row header is the one declaration site; the rest are constants of this
 * hosting. The two collector rows name this image's own collector: the
 * runtime loads a collector by the path row directly, through the loader
 * seam below, and the name row is stated beside it because the runtime
 * reads both. */
static char g_env_hard_limit[48];
static const char *g_env[] = {
    g_env_hard_limit,
    "DOTNET_gcServer=0",
    "DOTNET_gcConcurrent=0",
    "DOTNET_GCName=rhu-mgc",
    "DOTNET_GCPath=rhu-mgc",
    "HOME=/tmp",
    NULL,
};

static void env_build(void)
{
    log_line l;
    l.n = 0;
    line_puts(&l, "DOTNET_GCHeapHardLimit=0x");
    line_num(&l, (unsigned long long)MGC_ROW_REGION_BYTES, 16u, 0u, 0);
    l.buf[l.n] = 0;
    memcpy(g_env_hard_limit, l.buf, l.n + 1u);
}

char *getenv(const char *name)
{
    size_t n = strlen(name);
    for (int i = 0; g_env[i] != NULL; i++) {
        if (strncmp(g_env[i], name, n) == 0 && g_env[i][n] == '=')
            return (char *)g_env[i] + n + 1;
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
        rhu_shim_log("init: the kernel refused the per-thread block module (%d)", h);
        rhu_exit(120);
    }
    g_block_module = h;
    env_build();
    if (rhu_entropy(&__stack_chk_guard, (uint32_t)sizeof __stack_chk_guard) != 0) {
        rhu_shim_log("init: the entropy door refused the stack guard's seed");
        rhu_exit(120);
    }
    rhu_shim_log("init: per-thread block registered as module %d (%u bytes)", h,
                 (unsigned)sizeof(shim_block));
    return 0;
}

/* ------------------------------------------------------------------ */
/* memory mapping: the kernel arena                                     */
/* ------------------------------------------------------------------ */

#define GLIBC_MAP_FIXED 0x10
#define MMAP_FAILED ((void *)-1)
#define PAGE_BYTES 4096u

static uint32_t g_fixed_rezeros;

/* Anonymous mappings are arena allocations attributed to the caller;
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
/* time: the 64-bit time surface on the kernel clock                    */
/* ------------------------------------------------------------------ */

/* The 64-bit timespec on a 32-bit little-endian target: seconds, then the
 * nanoseconds, then padding. Both clock ids share the boot-relative
 * nanosecond clock; the wall clock has no epoch source on this platform,
 * so it reports time since boot. */
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

int __clock_gettime64(int id, struct shim_ts64 *ts)
{
    (void)id;
    ts64_from_ns(ts, rhu_now_ns());
    return 0;
}

int __clock_nanosleep_time64(int id, int flags, const struct shim_ts64 *rq, struct shim_ts64 *rm)
{
    (void)id;
    uint64_t ns = (uint64_t)rq->sec * NS_PER_S + (uint64_t)rq->nsec;
    if ((flags & GLIBC_TIMER_ABSTIME) != 0) {
        uint64_t now = rhu_now_ns();
        ns = ns > now ? ns - now : 0u;
    }
    if (ns != 0)
        rhu_sleep_ns(ns);
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
    int64_t t = (int64_t)(rhu_now_ns() / NS_PER_S);
    if (out != NULL)
        *out = t;
    return t;
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

static shim_mutex *mutex_of(void *slot)
{
    mutex_slot *s = (mutex_slot *)slot;
    if (s->magic != MAGIC_MUTEX) {
        static const mutex_slot zero;
        if (__builtin_memcmp(s, &zero, sizeof zero) != 0)
            return NULL;
        s->magic = MAGIC_MUTEX;
    }
    return &s->m;
}

static shim_cond *cond_of(void *slot)
{
    cond_slot *s = (cond_slot *)slot;
    if (s->magic != MAGIC_COND) {
        static const cond_slot zero;
        if (__builtin_memcmp(s, &zero, sizeof zero) != 0)
            return NULL;
        s->magic = MAGIC_COND;
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

/* Both clock ids share the boot-relative timebase, so an absolute timespec
 * converts directly to a kernel deadline. */
int __pthread_cond_timedwait64(void *cs, void *ms, const struct shim_ts64 *abs)
{
    shim_cond *c = cond_of(cs);
    shim_mutex *m = mutex_of(ms);
    if (c == NULL || m == NULL)
        return RHU_GERR_EINVAL;
    uint64_t deadline = (uint64_t)abs->sec * NS_PER_S + (uint64_t)abs->nsec;
    return cond_wait(c, m, deadline);
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

int pthread_attr_init(void *a)
{
    attr_slot *s = (attr_slot *)a;
    s->magic = MAGIC_ATTR;
    s->stack_bytes = MGC_ROW_STACK_DEFAULT;
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
        stack_bytes = MGC_ROW_STACK_DEFAULT;
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
     * the caller's error. */
    int32_t tid = rhu_thread_spawn(thread_entry, s, stack_bytes, RHU_SHIM_PRIO_FLAT, NULL);
    unsigned retries = 0u;
    while (tid == RHU_EAGAIN && retries < 100u) {
        rhu_sleep_ns(1000000ull);
        retries++;
        tid = rhu_thread_spawn(thread_entry, s, stack_bytes, RHU_SHIM_PRIO_FLAT, NULL);
    }
    if (tid < 0) {
        rhu_kheap_free(s);
        rhu_shim_log("pthread_create: the kernel refused the spawn (%d) -> EAGAIN", tid);
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
    uint32_t stack_bytes = (s != NULL && s->magic == MAGIC_ATTR) ? s->stack_bytes : MGC_ROW_STACK_DEFAULT;
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
        rhu_shim_log("TLS module %lu is not wired (only the image's own module 1 exists)", ti->ti_module);
        rhu_exit(121);
    }
#if defined(RHU_SHIM_BREAK_TLSOFF)
    /* Gate control: the offset dropped, so every thread-local symbol
     * resolves to the front of the block. */
    return rhu_tls_get(0);
#elif defined(RHU_SHIM_BREAK_TLSCACHE)
    /* Gate control: the first block seen is remembered and handed to
     * every thread after it -- the shape a "look it up once" change
     * would take, and the reason the per-thread copy is checked from a
     * second thread rather than from the caller alone. */
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
 * reads -- answered from the arena window and its live headroom. */
#define GLIBC_SC_PAGESIZE 30
#define GLIBC_SC_NPROCESSORS_CONF 83
#define GLIBC_SC_NPROCESSORS_ONLN 84
#define GLIBC_SC_PHYS_PAGES 85
#define GLIBC_SC_AVPHYS_PAGES 86

extern char __rhu_arena_lo[];
extern char __rhu_arena_hi[];

long sysconf(int name)
{
    switch (name) {
    case GLIBC_SC_PAGESIZE:
        return (long)PAGE_BYTES;
    case GLIBC_SC_NPROCESSORS_CONF:
    case GLIBC_SC_NPROCESSORS_ONLN:
        return 1;
    case GLIBC_SC_PHYS_PAGES:
        return (long)((uintptr_t)__rhu_arena_hi - (uintptr_t)__rhu_arena_lo) / (long)PAGE_BYTES;
    case GLIBC_SC_AVPHYS_PAGES:
        return (long)(rhu_arena_headroom() / PAGE_BYTES);
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

/* The kernel's activation handler: the costume proper. */
static void act_costume(rhu_ctx *k)
{
    rhu_glibc_ucontext uc;
    rhu_glibc_siginfo si;
    shim_block *b = block();
    g_activation_count++;
    memset(&uc, 0, sizeof uc);
    memset(&si, 0, sizeof si);
    si.si_signo = SHIM_ACTIVATION_SIGNAL;
    si.si_code = RHU_GLIBC_SI_TKILL;
#ifdef RHU_SHIM_BREAK_SIPID
    /* A test knob: the record names a foreign process, the shape the runtime
     * drops. Never set in a hosted image. */
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
    /* A test knob: the program counter slot carries the link register, the
     * plumbing slip a layout assert cannot see. Never set in a hosted image. */
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
/* entropy: never a short draw reported as a full one                   */
/* ------------------------------------------------------------------ */

int getentropy(void *buf, size_t len)
{
    if (rhu_entropy(buf, (uint32_t)len) != 0) {
        SET_ERRNO(RHU_GERR_EIO);
        return -1;
    }
    return 0;
}

long getrandom(void *buf, size_t len, unsigned flags)
{
    (void)flags;
    if (rhu_entropy(buf, (uint32_t)len) != 0) {
        SET_ERRNO(RHU_GERR_EIO);
        return -1;
    }
    return (long)len;
}

/* No error channel by contract: callers make keys from these bytes, so a
 * failed draw ends the process rather than returning weak bytes. */
void arc4random_buf(void *buf, size_t len)
{
    if (rhu_entropy(buf, (uint32_t)len) != 0) {
        rhu_shim_log("arc4random_buf: the entropy source failed; refusing to return weak bytes");
        rhu_exit(134);
    }
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
 * its unwind index as the ARM exception-index segment. A program counter
 * belongs to the smallest module containing it, so a module's load
 * segments are emitted as its span minus every more specific module's
 * span -- an unwinder then never consults a foreign index for it. */
int dl_iterate_phdr(int (*cb)(dl_phdr_info *, size_t, void *), void *data)
{
    dl_mod m[RHU_MODULE_MAX];
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
    for (int i = 0; i < n; i++) {
        elf32_phdr ph[RHU_MODULE_MAX + 2];
        int np = 0;
        uint32_t cur = m[i].lo;
        memset(ph, 0, sizeof ph);
        while (cur < m[i].hi && np < RHU_MODULE_MAX + 1) {
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

/* The in-image header the kernel states at the image base: the module
 * base every reader of this record shares (the host's registration, a
 * debugger, this answer). Sixteen words; the magic, the layout version
 * and the load address are the ones read here. */
extern const uint32_t __rhu_image_header[16];
#define RHU_SHIM_IMAGE_MAGIC 0x49554852u
#define RHU_SHIM_IMAGE_LAYOUT 1u

/* An address inside a registered module's code is answered with the
 * image's own name and the load address its header states -- the one
 * number the host registered the module under. This image has no symbol
 * table to consult, so the nearest-symbol fields are null. Any other
 * address is not inside a module this hosting knows. */
int dladdr(const void *addr, void *info)
{
    rhu_module_desc d;
    rhu_glibc_dl_info *out = (rhu_glibc_dl_info *)info;
    if (out == NULL || rhu_module_find((uint32_t)(uintptr_t)addr, &d) < 0)
        return 0;
    if (__rhu_image_header[0] != RHU_SHIM_IMAGE_MAGIC || __rhu_image_header[1] != RHU_SHIM_IMAGE_LAYOUT) {
        rhu_shim_log("dladdr: the in-image header does not read as layout version 1");
        return 0;
    }
    out->dli_fname = "/rhu";
    out->dli_fbase = (void *)(uintptr_t)__rhu_image_header[3];
    out->dli_sname = NULL;
    out->dli_saddr = NULL;
    return 1;
}

/* ------------------------------------------------------------------ */
/* the instruction-stream sync after code is written                   */
/* ------------------------------------------------------------------ */

/* The runtime calls this after writing a fresh block of call thunks into
 * memory it then executes. The processor cores this hosting runs on
 * keep their caches OFF on both silicon boards (the kernel's board
 * modules state that posture; the emulator models no cache), so nothing
 * is cleaned or invalidated here: the two barriers order the writes
 * before any fetch of the new instructions and drain the pipeline of
 * anything fetched earlier. The day a board's posture turns a cache on,
 * the kernel's cache-sync door lands beside it and this row calls it. */
void __clear_cache(void *begin, void *end)
{
    (void)begin;
    (void)end;
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
    rhu_shim_exit(134);
}

/* ------------------------------------------------------------------ */
/* file descriptors at the C-runtime level: the console, and every    */
/* path refused with its errno -- the file surface is behind the PAL  */
/* ------------------------------------------------------------------ */

static int fd_is_console(int fd)
{
    return fd >= RHU_SHIM_FD_CONSOLE_LO && fd < RHU_SHIM_FD_CONSOLE_HI;
}

int open(const char *path, int flags, ...)
{
    (void)flags;
    rhu_shim_log("open(%s): no such path (ENOENT)", path != NULL ? path : "(null)");
    SET_ERRNO(RHU_GERR_ENOENT);
    return -1;
}

int open64(const char *path, int flags, ...)
{
    (void)flags;
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
    SET_ERRNO(RHU_GERR_EBADF);
    return -1;
}

int ioctl(int fd, unsigned long request, ...)
{
    (void)fd;
    (void)request;
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
