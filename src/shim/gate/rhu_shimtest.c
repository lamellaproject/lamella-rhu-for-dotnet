/* The shim gate's tenant: a C program hosted exactly like a managed
 * payload (the payload board map, the kernel's managed entry sequence,
 * the machine's exit code), exercising the shim's C-runtime surface and
 * the PAL from the caller's side. Eleven ladders, one per build:
 *
 *   RHU_SHIMTEST_LADDER_CORE        the glibc personality and the PAL's
 *                                   threading, memory, time and random verbs
 *   RHU_SHIMTEST_LADDER_FDMAP       the descriptor bands and the file surface
 *   RHU_SHIMTEST_LADDER_ACTIVATION  the activation costume (the signal
 *                                   surface over the kernel's delivery), the
 *                                   kernel's null fault delivered as SIGSEGV
 *                                   and the PAL's signal rows
 *   RHU_SHIMTEST_LADDER_TLS         the image's thread-local template: the
 *                                   kernel's per-thread copy of module 0
 *   RHU_SHIMTEST_LADDER_PWMFS       the PWM device files (/sys/class/pwm/pwmchip0)
 *                                   from a native caller's side, over the kernel's PWM doors
 *   RHU_SHIMTEST_LADDER_IIOFS       the analog input device files (/sys/bus/iio/devices)
 *                                   from a native caller's side, over the kernel's ADC doors
 *   RHU_SHIMTEST_LADDER_TTYDEV      the serial device files (/dev/ttyS<n>) from a
 *                                   native caller's side, over the kernel's UART doors,
 *                                   with the stock serial layer through its own entry points
 *   RHU_SHIMTEST_LADDER_BUSDEV      the bus device nodes (/dev/i2c-<bus>,
 *                                   /dev/spidev<bus>.<select>) from a native
 *                                   caller's side, on the row's bus models
 *   RHU_SHIMTEST_LADDER_GPIOD       the GPIO library surface from a native
 *                                   caller's side, on the row's virtual lines
 *   RHU_SHIMTEST_LADDER_HOST        the initialization contract the host runs
 *                                   before managed code, in the host's own
 *                                   entry: its order, the module handle,
 *                                   the table by the runtime's export names
 *   RHU_SHIMTEST_LADDER_FROZEN      the frozen object heap on a small board's
 *                                   row: a pool smaller than the runtime's
 *                                   commit, and what memory backs it
 *
 * Every check prints one PASS line; the first failed check prints one
 * FAIL line and ends the run with exit 1. Workers never print, no line
 * carries a timing value or an address, and the checks run in a fixed
 * order, so the payload stream is the same bytes on every run.
 *
 * Two build defines here break one check each, so the test runner can be
 * shown to report a failing program:
 *   RHU_SHIMTEST_BREAK_ERRNO   a worker writes through the main thread's
 *                              errno pointer (the isolation check fails)
 *   RHU_SHIMTEST_BREAK_CANARY  a block's header is corrupted before it is
 *                              freed (the allocator dies loud, exit 134)
 * and the shim objects' own defines place the file band at the console's
 * base (the descriptor ladder refuses it), name a foreign process in the
 * activation's signal information, fill the context's program counter
 * from the link register (the activation ladder refuses both), drop the
 * offset from a thread-local access, hand every thread the first block
 * seen (the thread-local ladder refuses both), and answer every page of
 * the frozen object heap's reservation as backed (the frozen ladder
 * refuses it). */
#include <stddef.h>
#include <stdint.h>

#include "rhu_shim.h"

#include "rhu.h"
#include "rhu_glibc_arm32.h"
#include "rhu_pal_errno_net11_rc2.h"
#include "rhu_pal_io_net11_rc2.h"
#include "rhu_pal_memory_net11_rc2.h"
#include "rhu_pal_random_net11_rc2.h"
#include "rhu_pal_signal_net11_rc2.h"
#include "rhu_pal_threading_net11_rc2.h"
#include "rhu_pal_time_net11_rc2.h"

#if !defined(RHU_SHIMTEST_LADDER_CORE) && !defined(RHU_SHIMTEST_LADDER_FDMAP) \
    && !defined(RHU_SHIMTEST_LADDER_ACTIVATION) && !defined(RHU_SHIMTEST_LADDER_TLS) \
    && !defined(RHU_SHIMTEST_LADDER_HOST) && !defined(RHU_SHIMTEST_LADDER_BUSDEV) \
    && !defined(RHU_SHIMTEST_LADDER_GPIOD) && !defined(RHU_SHIMTEST_LADDER_FROZEN) && !defined(RHU_SHIMTEST_LADDER_PWMFS) && !defined(RHU_SHIMTEST_LADDER_TTYDEV) && !defined(RHU_SHIMTEST_LADDER_IIOFS)
#error "a tenant build names its ladder (RHU_SHIMTEST_LADDER_CORE, RHU_SHIMTEST_LADDER_FDMAP, RHU_SHIMTEST_LADDER_ACTIVATION, RHU_SHIMTEST_LADDER_TLS, RHU_SHIMTEST_LADDER_HOST, RHU_SHIMTEST_LADDER_BUSDEV, RHU_SHIMTEST_LADDER_GPIOD, RHU_SHIMTEST_LADDER_PWMFS, RHU_SHIMTEST_LADDER_IIOFS, RHU_SHIMTEST_LADDER_TTYDEV or RHU_SHIMTEST_LADDER_FROZEN)"
#endif

/* --- the C-runtime surface, declared as the caller sees it --- */
extern int *__errno_location(void);
void *malloc(size_t n);
void free(void *p);
void *calloc(size_t n, size_t sz);
void *realloc(void *p, size_t n);
void *memalign(size_t align, size_t n);
int posix_memalign(void **out, size_t align, size_t n);
void *memset(void *dst, int c, size_t n);
void *memcpy(void *dst, const void *src, size_t n);
int memcmp(const void *a, const void *b, size_t n);
size_t strlen(const char *s);
int strcmp(const char *a, const char *b);
unsigned long getauxval(unsigned long type);
char *realpath(const char *path, char *resolved);
char *getenv(const char *name);
void *mmap64(void *addr, size_t len, int prot, int flags, int fd, int64_t off);
int mincore(void *addr, size_t length, unsigned char *vec);
long sysconf(int name);
int getpid(void);
long syscall(long number, ...);
int getentropy(void *buf, size_t len);
long getrandom(void *buf, size_t len, unsigned flags);
int fork(void);
int open(const char *path, int flags, ...);
int close(int fd);
int dup2(int oldfd, int newfd);
int ioctl(int fd, unsigned long request, ...);
int sched_yield(void);
void *opendir(const char *path);

struct ts64 {
    int64_t sec;
    int32_t nsec;
    int32_t pad;
};
int __clock_gettime64(int id, struct ts64 *ts);
int __clock_nanosleep_time64(int id, int flags, const struct ts64 *rq, struct ts64 *rm);
int __nanosleep64(const struct ts64 *rq, struct ts64 *rm);
int64_t __time64(int64_t *out);

typedef struct mutex_slot {
    unsigned char b[24];
} __attribute__((aligned(4))) mutex_slot;
typedef struct cond_slot {
    unsigned char b[48];
} __attribute__((aligned(4))) cond_slot;
typedef struct attr_slot {
    unsigned char b[36];
} __attribute__((aligned(4))) attr_slot;

int pthread_mutexattr_init(void *a);
int pthread_mutexattr_settype(void *a, int kind);
int pthread_mutex_init(void *slot, const void *attr);
int pthread_mutex_lock(void *slot);
int pthread_mutex_unlock(void *slot);
int pthread_condattr_init(void *a);
int pthread_condattr_setclock(void *a, int clock);
int pthread_cond_init(void *slot, const void *attr);
int pthread_cond_wait(void *cs, void *ms);
int pthread_cond_signal(void *cs);
int __pthread_cond_timedwait64(void *cs, void *ms, const struct ts64 *abs);
int pthread_attr_init(void *a);
int pthread_attr_setstacksize(void *a, size_t bytes);
int pthread_attr_getstack(void *a, void **lo, size_t *bytes);
int pthread_getattr_np(uint32_t tid, void *a);
int pthread_create(uint32_t *out, const void *attr, void *(*fn)(void *), void *arg);
uint32_t pthread_self(void);
int pthread_equal(uint32_t a, uint32_t b);
int pthread_key_create(uint32_t *key, void (*dtor)(void *));
int pthread_setspecific(uint32_t key, const void *value);
void *pthread_getspecific(uint32_t key);

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
int dl_iterate_phdr(int (*cb)(dl_phdr_info *, size_t, void *), void *data);

/* --- output: unprefixed payload lines, EACH ONE console write: the
 * kernel's door writes a call whole, and a second thread's log line can
 * land between two calls of one thread, tearing a verdict line the gate
 * matches as fixed text. --- */
#define LINE_MAX 512u
static uint32_t put(char *line, uint32_t n, const char *s)
{
    while (*s != 0 && n + 1u < LINE_MAX)
        line[n++] = *s++;
    return n;
}

static void out_line(const char *a, const char *b, const char *c, const char *d)
{
    char line[LINE_MAX];
    uint32_t n = put(line, 0u, a);
    n = put(line, n, b);
    n = put(line, n, c);
    n = put(line, n, d);
    line[n++] = '\n';
    rhu_console_write(line, n);
}

static void out(const char *s)
{
    out_line(s, "", "", "");
}

static void out2(const char *a, const char *b, const char *c)
{
    out_line(a, b, c, "");
}

static void pass(const char *name, const char *note)
{
    out2("PASS ", name, note);
}

static int fail(const char *name, const char *detail)
{
    out_line("FAIL ", name, " ", detail);
    return 1;
}

#define CHECK(cond, name, detail)      \
    do {                               \
        if (!(cond))                   \
            return fail(name, detail); \
    } while (0)

/* The file-store groups use this form. In the PROBE build (never
 * frozen) a failed check prints its FAIL line and the ladder continues,
 * so one run over a store reports every clause it gets wrong; in every
 * other build it is CHECK. */
#ifdef RHU_SHIMTEST_STORE_PROBE
#define SCHECK(cond, name, detail) \
    do {                           \
        if (!(cond))               \
            (void)fail(name, detail); \
    } while (0)
#else
#define SCHECK(cond, name, detail) CHECK(cond, name, detail)
#endif

#if defined(RHU_SHIMTEST_LADDER_CORE)
/* The thread pool's idle-worker timeout link word, read as the hosting
 * layer reads it (a weak absolute symbol: no word in this image reads
 * NULL), so the expectation for the row is computed from the same word
 * the row is built from, and the gate links this ladder both ways. */
extern const char __rhu_threadpool_thread_timeout_ms[] __attribute__((weak));

/* A value in decimal into the buffer given (at least eleven bytes);
 * returns the buffer. */
static const char *dec_u32(char *buf, uint32_t v)
{
    char tmp[12];
    uint32_t n = 0;
    do {
        tmp[n++] = (char)('0' + v % 10u);
        v /= 10u;
    } while (v != 0);
    uint32_t i = 0;
    while (n != 0)
        buf[i++] = tmp[--n];
    buf[i] = 0;
    return buf;
}
#endif

#if defined(RHU_SHIMTEST_LADDER_CORE) || defined(RHU_SHIMTEST_LADDER_FROZEN)
/* The link map's rehydrated-data brackets, read as the hosting layer
 * reads them (weak references: an image whose map defines none reads
 * NULL). */
extern char __rhu_hydrated_lo[] __attribute__((weak));
extern char __rhu_hydrated_hi[] __attribute__((weak));

/* The managed heap's window as the hosting layer chooses it: window 1
 * when the image serves two ranges and the brackets lie inside range 1's
 * physical range, window 0 otherwise. The chosen window's record is
 * filled at the call (zeroed should the range door refuse). Returns 1
 * for window 1, 0 for window 0. */
static int heap_window_read(rhu_ram_desc *w)
{
    rhu_ram_desc r;
    if (rhu_ram_count() >= 2 && rhu_ram_range(1u, &r) == 0 && __rhu_hydrated_lo != NULL && __rhu_hydrated_hi != NULL) {
        uint32_t lo = (uint32_t)(uintptr_t)__rhu_hydrated_lo;
        uint32_t hi = (uint32_t)(uintptr_t)__rhu_hydrated_hi;
        if (lo <= hi && lo >= r.base && hi - r.base <= r.bytes) {
            *w = r;
            return 1;
        }
    }
    if (rhu_ram_range(0u, w) != 0) {
        w->base = 0u;
        w->bytes = 0u;
        w->lo = 0u;
        w->hi = 0u;
        w->free = 0u;
        w->flags = 0u;
    }
    return 0;
}

/* The runtime's hard-limit row as a number: "0x" and hexadecimal digits,
 * nothing else. A row that is absent or malformed reads as all ones, a
 * value no board's memory reaches. */
static uint32_t hard_limit_row(void)
{
    const char *v = getenv("DOTNET_GCHeapHardLimit");
    if (v == NULL || v[0] != '0' || v[1] != 'x' || v[2] == 0)
        return 0xFFFFFFFFu;
    uint32_t n = 0;
    for (v += 2; *v != 0; v++) {
        uint32_t d = *v >= '0' && *v <= '9'   ? (uint32_t)(*v - '0')
                     : *v >= 'a' && *v <= 'f' ? (uint32_t)(*v - 'a') + 10u
                     : *v >= 'A' && *v <= 'F' ? (uint32_t)(*v - 'A') + 10u
                                              : 16u;
        if (d == 16u || n > 0x0FFFFFFFu)
            return 0xFFFFFFFFu;
        n = n * 16u + d;
    }
    return n;
}
#endif

/* Named positively: only the ladders that measure elapsed time carry
 * this; a ladder that does not name itself here does not get it. */
#if defined(RHU_SHIMTEST_LADDER_CORE) || defined(RHU_SHIMTEST_LADDER_FDMAP)
static uint64_t now_ns(void)
{
    struct ts64 t;
    __clock_gettime64(1, &t);
    return (uint64_t)t.sec * 1000000000ull + (uint64_t)t.nsec;
}
#endif

#if defined(RHU_SHIMTEST_LADDER_CORE) || defined(RHU_SHIMTEST_LADDER_ACTIVATION) || defined(RHU_SHIMTEST_LADDER_FDMAP) \
    || defined(RHU_SHIMTEST_LADDER_TLS) || defined(RHU_SHIMTEST_LADDER_BUSDEV) || defined(RHU_SHIMTEST_LADDER_GPIOD) \
    || defined(RHU_SHIMTEST_LADDER_TTYDEV)

static void msleep(int ms)
{
    struct ts64 t;
    t.sec = ms / 1000;
    t.nsec = (ms % 1000) * 1000000;
    t.pad = 0;
    __nanosleep64(&t, NULL);
}

static void wait_done(volatile int *done, int want)
{
    while (__atomic_load_n(done, __ATOMIC_ACQUIRE) < want)
        msleep(5);
    msleep(20);
}

/* Workers take a small stack. The board's 64 KB stack pool serves the
 * hosting thread (16 KB) and the idle thread (1 KB) first, and a retired
 * stack is reused only by a request of the same size, so the ladder's
 * distinct sizes must fit the rest: two of these, the row's default
 * (16 KB) once, and the row's clamp (24 KB) once. The worker bodies are
 * shallow: a lock, a wait, a sleep, a store. */
#define WORKER_STACK 2048u

/* The ladders whose workers run level with the ladder's thread (the
 * bus, GPIO and serial ladders' second threads run below it: spawn_below). */
#if defined(RHU_SHIMTEST_LADDER_CORE) || defined(RHU_SHIMTEST_LADDER_ACTIVATION) || defined(RHU_SHIMTEST_LADDER_FDMAP) || defined(RHU_SHIMTEST_LADDER_TLS)
static int spawn(uint32_t *tid, void *(*fn)(void *), void *arg, uint32_t stack)
{
    attr_slot at;
    pthread_attr_init(&at);
    if (stack != 0)
        pthread_attr_setstacksize(&at, stack);
    return pthread_create(tid, stack != 0 ? (const void *)&at : NULL, fn, arg);
}
#endif

#if defined(RHU_SHIMTEST_LADDER_CORE) || defined(RHU_SHIMTEST_LADDER_BUSDEV) || defined(RHU_SHIMTEST_LADDER_GPIOD) || defined(RHU_SHIMTEST_LADDER_TTYDEV)
/* A second thread BELOW the ladder's: spawned through the kernel's door one
 * step less urgent than the flat hosted level every shim thread runs at,
 * so it runs only while the ladder's thread is blocked and never takes a
 * step of its own before the ladder's thread has stopped to wait for it --
 * the order a race control's held-open window relies on. The kernel's
 * spawn prints no line; the stack is the row's default, as the shim carves
 * it for an unsized ask; two such threads at most in one ladder. */
#define BELOW_STACK 16384u
typedef struct below_start {
    void *(*fn)(void *);
    void *arg;
} below_start;
static below_start g_below[2];
static unsigned g_below_used;

static void below_entry(void *p)
{
    below_start s = *(below_start *)p;
    s.fn(s.arg);
}

static int spawn_below(uint32_t *tid, void *(*fn)(void *), void *arg)
{
    if (g_below_used >= 2u)
        return -1;
    below_start *s = &g_below[g_below_used++];
    s->fn = fn;
    s->arg = arg;
    int32_t t = rhu_thread_spawn(below_entry, s, BELOW_STACK, (uint32_t)RHU_HOSTED_PRIO_FLAT + 1u, "below");
    if (t < 0)
        return -1;
    if (tid != NULL)
        *tid = (uint32_t)t;
    return 0;
}
#endif

#endif /* RHU_SHIMTEST_LADDER_CORE || RHU_SHIMTEST_LADDER_ACTIVATION || _TLS */

/* The module-list initializer of the managed entry sequence, spelled as
 * the runtime's own bootstrapper declares it: no modules to initialize in
 * any tenant image. The host ladder records the call for its order
 * check. */
#ifdef RHU_SHIMTEST_LADDER_HOST
static void host_record_modules(void *os_module, int count);
#endif
void InitializeModules(void *osModule, void **modules, int count, void **pClasslibFunctions, int nClasslibFunctions)
{
    (void)modules;
    (void)pClasslibFunctions;
    (void)nClasslibFunctions;
#ifdef RHU_SHIMTEST_LADDER_HOST
    host_record_modules(osModule, count);
#else
    (void)osModule;
    (void)count;
#endif
}

#ifdef RHU_SHIMTEST_LADDER_CORE

static mutex_slot g_m;
static volatile int g_counter;
static volatile int g_done;

static void *locker(void *a)
{
    (void)a;
    for (int i = 0; i < 300; i++) {
        pthread_mutex_lock(&g_m);
        g_counter++;
        pthread_mutex_unlock(&g_m);
    }
    __atomic_fetch_add(&g_done, 1, __ATOMIC_RELEASE);
    return NULL;
}

static int *g_main_errno;
static volatile int g_err_ok;

static void *errno_worker(void *a)
{
    (void)a;
#ifdef RHU_SHIMTEST_BREAK_ERRNO
    int *e = g_main_errno;
#else
    int *e = __errno_location();
#endif
    *e = 42;
    msleep(15);
    if (*e == 42)
        g_err_ok = 1;
    __atomic_fetch_add(&g_done, 1, __ATOMIC_RELEASE);
    return NULL;
}

static uint32_t g_key;
static uint32_t g_key_rearm;
static void *g_dtor_saw;
static volatile int g_rearm_rounds;

static void key_dtor(void *v)
{
    g_dtor_saw = v;
}

/* A destructor that re-arms its key twice: the run count proves the
 * rounds happen and stay bounded. */
static void rearm_dtor(void *v)
{
    (void)v;
    g_rearm_rounds++;
    if (g_rearm_rounds < 3)
        pthread_setspecific(g_key_rearm, (void *)0xBEEF);
}

static void *key_worker(void *a)
{
    (void)a;
    pthread_setspecific(g_key, (void *)0xCAFE);
    pthread_setspecific(g_key_rearm, (void *)0xBEEF);
    __atomic_fetch_add(&g_done, 1, __ATOMIC_RELEASE);
    return NULL;
}

static volatile int g_clamp_ran;

static void *clamp_worker(void *a)
{
    (void)a;
    g_clamp_ran = 1;
    __atomic_fetch_add(&g_done, 1, __ATOMIC_RELEASE);
    return NULL;
}

static void *sleeper(void *a)
{
    (void)a;
    msleep(40);
    __atomic_fetch_add(&g_done, 1, __ATOMIC_RELEASE);
    return NULL;
}

static mutex_slot g_cm;
static cond_slot g_cv;
static volatile int g_cond_ready;
static volatile int g_cond_saw;

static void *cond_worker(void *a)
{
    (void)a;
    pthread_mutex_lock(&g_cm);
    while (!g_cond_ready)
        pthread_cond_wait(&g_cv, &g_cm);
    g_cond_saw = 1;
    pthread_mutex_unlock(&g_cm);
    __atomic_fetch_add(&g_done, 1, __ATOMIC_RELEASE);
    return NULL;
}

/* The real-time waits: a worker sleeps or waits against the real-time
 * clock while the main thread watches it on the boot clock, so a wait
 * that does not end is observed within a bound, never waited for. */
static mutex_slot g_rt_m;
static cond_slot g_rt_cv;
static struct ts64 g_rt_abs;
static volatile int g_rt_kind; /* 0 the absolute sleep, 1 the timed wait */
static volatile int g_rt_done;
static volatile int g_rt_rc;
static volatile uint64_t g_rt_end;

static void *realtime_waiter(void *a)
{
    (void)a;
    int rc;
    if (g_rt_kind == 0) {
        rc = __clock_nanosleep_time64(0, 1, &g_rt_abs, NULL);
    } else {
        pthread_mutex_lock(&g_rt_m);
        rc = __pthread_cond_timedwait64(&g_rt_cv, &g_rt_m, &g_rt_abs);
        pthread_mutex_unlock(&g_rt_m);
    }
    g_rt_rc = rc;
    g_rt_end = now_ns();
    __atomic_fetch_add(&g_rt_done, 1, __ATOMIC_RELEASE);
    return NULL;
}

/* The bounded watch: 1 once the count reaches `want` within `bound_ms` of
 * the boot clock (the worker's exit then given its moment, as wait_done
 * gives it); 0 at the bound with the count still short. */
static int done_within(volatile int *done, int want, int bound_ms)
{
    uint64_t t0 = now_ns();
    while (__atomic_load_n(done, __ATOMIC_ACQUIRE) < want) {
        if (now_ns() - t0 > (uint64_t)bound_ms * 1000000ull)
            return 0;
        msleep(5);
    }
    msleep(20);
    return 1;
}

/* The real-time clock now plus `ms` (negative for a past timestamp). */
static void realtime_after(struct ts64 *t, int ms)
{
    __clock_gettime64(0, t);
    int64_t ns = (int64_t)t->nsec + (int64_t)ms * 1000000ll;
    t->sec += ns / 1000000000ll;
    ns %= 1000000000ll;
    if (ns < 0) {
        t->sec -= 1;
        ns += 1000000000ll;
    }
    t->nsec = (int32_t)ns;
}

/* A timestamp's Unix count as calendar ticks since the year 1, for the
 * calendar door. */
static uint64_t calendar_ticks_of(const struct ts64 *t)
{
    return (uint64_t)((int64_t)t->sec * 10000000ll + (int64_t)(t->nsec / 100) + RHU_SHIM_UNIX_EPOCH_TICKS);
}

static int start_realtime_waiter(uint32_t *tid, int kind, const struct ts64 *abs)
{
    g_rt_kind = kind;
    g_rt_abs = *abs;
    g_rt_done = 0;
    g_rt_rc = -1;
    g_rt_end = 0;
    return spawn(tid, realtime_waiter, NULL, WORKER_STACK);
}

static int32_t g_futex_cell;
static volatile int g_futex_saw;

static void *futex_waiter(void *a)
{
    (void)a;
    while (__atomic_load_n(&g_futex_cell, __ATOMIC_ACQUIRE) == 0)
        SystemNative_LowLevelFutex_WaitOnAddress(&g_futex_cell, 0);
    g_futex_saw = 1;
    __atomic_fetch_add(&g_done, 1, __ATOMIC_RELEASE);
    return NULL;
}

static void *g_mon;
static volatile int g_mon_ready;
static volatile int g_mon_saw;

static void *mon_worker(void *a)
{
    (void)a;
    SystemNative_LowLevelMonitor_Acquire(g_mon);
    while (!g_mon_ready)
        SystemNative_LowLevelMonitor_Wait(g_mon);
    g_mon_saw = 1;
    SystemNative_LowLevelMonitor_Release(g_mon);
    __atomic_fetch_add(&g_done, 1, __ATOMIC_RELEASE);
    return NULL;
}

static int g_phdr_load;
static int g_phdr_exidx;
static int g_phdr_modules;
static int g_phdr_qspi;

/* One call per module of the table: the module counted; a load segment
 * holding this code and an exception-index segment noted; a load segment
 * inside the quad-SPI region (0x90000000 to 0x9FFFFFFF, where the
 * STM32F769's execute-in-place window lives; no emulator map brackets
 * one) noted too. */
static int phdr_cb(dl_phdr_info *info, size_t size, void *data)
{
    (void)size;
    (void)data;
    uintptr_t me = (uintptr_t)phdr_cb & ~1u;
    g_phdr_modules++;
    for (int i = 0; i < info->dlpi_phnum; i++) {
        const elf32_phdr *p = &info->dlpi_phdr[i];
        if (p->p_type == 0x70000001u)
            g_phdr_exidx = 1;
        if (p->p_type == 1u && me >= p->p_vaddr && me < p->p_vaddr + p->p_memsz)
            g_phdr_load = 1;
        if (p->p_type == 1u && p->p_vaddr >= 0x90000000u && p->p_vaddr < 0xA0000000u)
            g_phdr_qspi = 1;
    }
    return 0;
}

/* Two first users of one zero-filled mutex: the second thread's lock,
 * taken while the first is inside its own first lock. */
static mutex_slot g_adopt_slot;
static volatile int g_adopt_go;
static volatile int g_adopt_rc = -1;
static volatile int g_adopt_done;

static void *adopt_second(void *arg)
{
    (void)arg;
    while (__atomic_load_n(&g_adopt_go, __ATOMIC_ACQUIRE) == 0)
        sched_yield();
    int rc = pthread_mutex_lock(&g_adopt_slot);
    if (rc == 0)
        pthread_mutex_unlock(&g_adopt_slot);
    g_adopt_rc = rc;
    __atomic_store_n(&g_adopt_done, 1, __ATOMIC_RELEASE);
    return NULL;
}

static int core_ladder(void)
{
    uint32_t t1;
    uint32_t t2;
    /* The system memory free before the shim initializes: the environment
     * check reads the runtime's hard-limit row against it. */
    uint32_t free_at_start = rhu_arena_headroom();
    /* The managed heap's window and its record before the shim
     * initializes, by the hosting layer's rule (heap_window_read): the
     * hard-limit row and the page counts are checked against it. */
    rhu_ram_desc heap_w;
    int heap_ext = heap_window_read(&heap_w);

    out("shimtest: the glibc personality and the PAL on the kernel's hosting API");

    /* init: once, then a no-op */
    CHECK(rhu_shim_init() == 0 && rhu_shim_init() == 0, "shim-init", "(initialization refused)");
    pass("shim-init", " (per-thread block registered once; a second call is a no-op)");

    /* errno identities both ways, from the generated list */
    {
        int pairs = 0;
        int ok = 1;
#define X(pal, plat, name)                                                    \
        pairs++;                                                              \
        if (SystemNative_ConvertErrorPlatformToPal(plat) != (pal) ||          \
            SystemNative_ConvertErrorPalToPlatform(pal) != (plat))            \
            ok = 0;
        RHU_PAL_ERRNO_MAP(X)
#undef X
        CHECK(ok && pairs == 80, "shim-errno-pal", "(a mapped pair does not round-trip)");
        CHECK(SystemNative_ConvertErrorPlatformToPal(9999) == RHU_PAL_ENONSTANDARD, "shim-errno-pal", "(an unknown platform value did not map to the nonstandard code)");
        CHECK(SystemNative_ConvertErrorPalToPlatform(RHU_PAL_ENONSTANDARD) == RHU_GERR_EINVAL, "shim-errno-pal", "(the nonstandard code did not map back to EINVAL)");
        SystemNative_SetErrNo(RHU_GERR_EAGAIN);
        CHECK(*__errno_location() == RHU_GERR_EAGAIN && SystemNative_GetErrNo() == RHU_GERR_EAGAIN, "shim-errno-pal", "(SetErrNo/GetErrNo did not carry the raw platform errno)");
        char msg[32];
        const char *m = SystemNative_StrErrorR(RHU_GERR_EAGAIN, msg, (int32_t)sizeof msg);
        CHECK(m != NULL && strcmp(m, "EAGAIN (11)") == 0, "shim-errno-pal", "(StrErrorR did not name the errno)");
        pass("shim-errno-pal", " (80 pairs round-trip; unknown values map to the nonstandard code and back to EINVAL; the errno calls carry the raw platform errno)");
    }

    /* the auxiliary vector: absent on this platform, answered by contract */
    {
        *__errno_location() = 0;
        CHECK(getauxval(16) == 0 && *__errno_location() == RHU_GERR_ENOENT, "shim-auxv", "(the capability entry did not answer 0 with ENOENT)");
        CHECK(getauxval(31) == 0, "shim-auxv", "(the executable-path entry did not answer 0)");
        pass("shim-auxv", " (no auxiliary vector: the capability and path entries absent, 0 with ENOENT)");
    }

    /* per-thread errno */
    g_done = 0;
    g_main_errno = __errno_location();
    *g_main_errno = 7;
    CHECK(spawn(&t1, errno_worker, NULL, WORKER_STACK) == 0, "shim-errno-thread", "(spawn refused)");
    wait_done(&g_done, 1);
    CHECK(g_err_ok == 1, "shim-errno-thread", "(the worker's errno did not hold its value)");
    CHECK(*__errno_location() == 7, "shim-errno-thread", "(errno leaked across threads)");
    pass("shim-errno-thread", " (errno isolated across threads)");

    /* contended mutex, exact count */
    pthread_mutex_init(&g_m, NULL);
    g_done = 0;
    CHECK(spawn(&t1, locker, NULL, WORKER_STACK) == 0 && spawn(&t2, locker, NULL, WORKER_STACK) == 0, "shim-mutex", "(spawn refused)");
    locker(NULL);
    wait_done(&g_done, 3);
    CHECK(g_counter == 900, "shim-mutex", "(the count is not exact)");
    pass("shim-mutex", " (3 threads x 300 under contention, count exact)");

    /* recursive kind */
    {
        int kind = 0;
        mutex_slot rm;
        pthread_mutexattr_init(&kind);
        pthread_mutexattr_settype(&kind, 1);
        pthread_mutex_init(&rm, &kind);
        CHECK(pthread_mutex_lock(&rm) == 0 && pthread_mutex_lock(&rm) == 0, "shim-mutex-recursive", "(nested lock refused)");
        CHECK(pthread_mutex_unlock(&rm) == 0 && pthread_mutex_unlock(&rm) == 0, "shim-mutex-recursive", "(nested unlock refused)");
        pass("shim-mutex-recursive", " (nested lock and unlock clean)");
    }

    /* the static initializer: an all-zero slot is adopted */
    {
        mutex_slot zm;
        memset(&zm, 0, sizeof zm);
        CHECK(pthread_mutex_lock(&zm) == 0 && pthread_mutex_unlock(&zm) == 0, "shim-mutex-initializer", "(an all-zero slot was refused)");
        pass("shim-mutex-initializer", " (an all-zero slot adopted on first use)");
    }

    /* a slot with a corrupted magic is refused */
    {
        mutex_slot bad;
        memset(&bad, 0x5A, sizeof bad);
        CHECK(pthread_mutex_lock(&bad) == RHU_GERR_EINVAL, "shim-mutex-bad-slot", "(a corrupted slot was not refused)");
        pass("shim-mutex-bad-slot", " (a corrupted slot refused EINVAL)");
    }

    /* condition variable: the deadline, then a real signal */
    {
        int clock = 0;
        pthread_condattr_init(&clock);
        pthread_condattr_setclock(&clock, 1);
        pthread_cond_init(&g_cv, &clock);
        pthread_mutex_init(&g_cm, NULL);
        struct ts64 abs;
        __clock_gettime64(1, &abs);
        uint64_t t0 = now_ns();
        abs.nsec += 40 * 1000000;
        if (abs.nsec >= 1000000000) {
            abs.sec++;
            abs.nsec -= 1000000000;
        }
        pthread_mutex_lock(&g_cm);
        int rc = __pthread_cond_timedwait64(&g_cv, &g_cm, &abs);
        pthread_mutex_unlock(&g_cm);
        uint64_t dt = now_ns() - t0;
        CHECK(rc == RHU_GERR_ETIMEDOUT && dt >= 40000000ull, "shim-cond-timeout", "(the deadline was not honored)");
        pass("shim-cond-timeout", " (ETIMEDOUT after the deadline)");

        g_done = 0;
        CHECK(spawn(&t1, cond_worker, NULL, WORKER_STACK) == 0, "shim-cond-signal", "(spawn refused)");
        msleep(20);
        pthread_mutex_lock(&g_cm);
        g_cond_ready = 1;
        pthread_cond_signal(&g_cv);
        pthread_mutex_unlock(&g_cm);
        wait_done(&g_done, 1);
        CHECK(g_cond_saw == 1, "shim-cond-signal", "(the waiter was not woken)");
        pass("shim-cond-signal", " (a waiter woken by a signal)");
    }

    /* thread identity and stack bounds */
    {
        attr_slot at;
        void *lo = NULL;
        size_t bytes = 0;
        CHECK(pthread_equal(pthread_self(), pthread_self()), "shim-thread", "(self is not equal to self)");
        pthread_getattr_np(pthread_self(), &at);
        CHECK(pthread_attr_getstack(&at, &lo, &bytes) == 0 && lo != NULL && bytes >= 8u * 1024u, "shim-thread", "(own stack bounds unavailable)");
        uintptr_t sp = (uintptr_t)&at;
        CHECK(sp > (uintptr_t)lo && sp <= (uintptr_t)lo + bytes, "shim-thread", "(the live SP is outside the reported bounds)");
        CHECK(syscall(224) == (long)pthread_self(), "shim-thread", "(gettid is not self)");
        g_done = 0;
        CHECK(spawn(&t1, sleeper, NULL, 0) == 0, "shim-thread", "(spawn refused)");
        attr_slot wat;
        void *wlo = NULL;
        size_t wbytes = 0;
        pthread_getattr_np(t1, &wat);
        CHECK(pthread_attr_getstack(&wat, &wlo, &wbytes) == 0 && wlo != NULL && wbytes == 16u * 1024u, "shim-thread", "(a worker's default carve is not the row default)");
        wait_done(&g_done, 1);
        pass("shim-thread", " (bounds contain the live SP; gettid is self; a worker's default carve is the row default)");
    }

    /* an oversized stack request is clamped and the thread still runs */
    g_done = 0;
    CHECK(spawn(&t1, clamp_worker, NULL, 96u * 1024u) == 0, "shim-thread-clamp", "(spawn refused)");
    wait_done(&g_done, 1);
    CHECK(g_clamp_ran == 1, "shim-thread-clamp", "(the clamped thread did not run)");
    pass("shim-thread-clamp", " (a 96 KB request clamped to the row's clamp; the thread ran)");

    /* key destructors at thread exit */
    g_done = 0;
    CHECK(pthread_key_create(&g_key, key_dtor) == 0 && pthread_key_create(&g_key_rearm, rearm_dtor) == 0, "shim-key-dtor", "(key creation refused)");
    CHECK(spawn(&t1, key_worker, NULL, WORKER_STACK) == 0, "shim-key-dtor", "(spawn refused)");
    wait_done(&g_done, 1);
    CHECK(g_dtor_saw == (void *)0xCAFE, "shim-key-dtor", "(the destructor did not see the stored value)");
    CHECK(g_rearm_rounds == 3, "shim-key-dtor", "(a re-armed key did not run its rounds)");
    CHECK(pthread_getspecific(g_key) == NULL, "shim-key-dtor", "(a value leaked into another thread)");
    pass("shim-key-dtor", " (the value reached its destructor in-thread; a re-armed key ran three rounds)");

    /* memory mapping on the arena */
    {
        uint8_t *p1 = (uint8_t *)mmap64(NULL, 8192, 3, 0x22, -1, 0);
        uint8_t *p2 = (uint8_t *)mmap64(NULL, 4096, 3, 0x22, -1, 0);
        CHECK(p1 != (uint8_t *)-1 && p2 != (uint8_t *)-1 && p1 != p2, "shim-mmap", "(anonymous mapping refused)");
        CHECK(((uintptr_t)p1 & 0xFFFu) == 0 && ((uintptr_t)p2 & 0xFFFu) == 0, "shim-mmap", "(not page-aligned)");
        int zero = 1;
        for (int i = 0; i < 8192; i += 64)
            zero &= p1[i] == 0;
        CHECK(zero, "shim-mmap", "(a fresh mapping is not zero)");
        long before = sysconf(86);
        memset(p1, 0xAA, 8192);
        void *r = mmap64(p1, 8192, 0, 0x10, -1, 0);
        CHECK(r == p1 && p1[0] == 0 && p1[8191] == 0, "shim-mmap", "(the fixed re-map did not re-zero in place)");
        CHECK(sysconf(86) == before, "shim-mmap", "(the re-map consumed address space)");
        CHECK(mmap64(NULL, 4096, 3, 0x02, 5, 0) == (void *)-1 && *__errno_location() == RHU_GERR_ENOSYS, "shim-mmap", "(a file-backed mapping was not refused)");
        pass("shim-mmap", " (zero-born pages; a fixed re-map re-zeroes in place with no address space consumed; file-backed refused)");
    }

    /* the frozen-object reservation and its commit guard */
    {
        uint8_t *base = (uint8_t *)SystemNative_MMap(NULL, 4u * 1024u * 1024u, RHU_PAL_PROT_NONE,
                                                     RHU_PAL_MAP_PRIVATE | RHU_PAL_MAP_ANONYMOUS, -1, 0);
        CHECK(base != NULL, "shim-foh", "(the reservation was refused)");
        CHECK(SystemNative_MProtect(base, 64u * 1024u, RHU_PAL_PROT_READ | RHU_PAL_PROT_WRITE) == 0, "shim-foh", "(a commit inside the pool was refused)");
        CHECK(SystemNative_MProtect(base + 64u * 1024u, 64u * 1024u, RHU_PAL_PROT_READ | RHU_PAL_PROT_WRITE) == -1 && SystemNative_GetErrNo() == RHU_GERR_ENOMEM, "shim-foh", "(a commit beyond the pool was not refused)");
        CHECK(SystemNative_MMap(NULL, 4u * 1024u * 1024u, RHU_PAL_PROT_NONE, RHU_PAL_MAP_PRIVATE | RHU_PAL_MAP_ANONYMOUS, -1, 0) == NULL, "shim-foh", "(a second reservation was not refused)");
        CHECK(SystemNative_MUnmap(base, 4u * 1024u * 1024u) == 0, "shim-foh", "(the release was refused)");
        /* what memory backs the reservation: the row's pool of 64 KB, sixteen pages, and nothing past it */
        unsigned char backed[17];
        int pool_backed = 1;
        CHECK(mincore(base, 68u * 1024u, backed) == 0, "shim-foh", "(the question of what memory backs the reservation was refused)");
        for (int i = 0; i < 16; i++)
            pool_backed &= backed[i] == 1;
        CHECK(pool_backed && backed[16] == 0, "shim-foh", "(the pool's pages are not answered as backed, or the page past the pool is)");
        pass("shim-foh", " (the reservation served from the row's pool; a commit beyond it refused; a second reservation NULL; the pool's pages answered as backed, the page past it not)");
    }

    /* time */
    {
        uint64_t t0 = now_ns();
        msleep(25);
        CHECK(now_ns() - t0 >= 25000000ull, "shim-time", "(the sleep returned early)");
        struct ts64 abs;
        __clock_gettime64(1, &abs);
        abs.nsec += 10 * 1000000;
        if (abs.nsec >= 1000000000) {
            abs.sec++;
            abs.nsec -= 1000000000;
        }
        uint64_t want = (uint64_t)abs.sec * 1000000000ull + (uint64_t)abs.nsec;
        CHECK(__clock_nanosleep_time64(1, 1, &abs, NULL) == 0 && now_ns() >= want, "shim-time", "(the absolute sleep returned early)");
        pass("shim-time", " (monotonic clock; a relative sleep honors its floor; an absolute sleep honors its deadline)");
    }

    /* the calendar: every wall-clock answer is the kernel's calendar */
    {
        uint64_t c0;
        uint64_t c1;
        int32_t s0;
        int32_t s1;
        CHECK(rhu_calendar_get(&c0, &s0) == 0, "shim-calendar", "(the kernel's calendar call refused)");
        int64_t pal = SystemNative_GetSystemTimeAsTicks();
        struct ts64 rt;
        CHECK(__clock_gettime64(0, &rt) == 0, "shim-calendar", "(the real-time clock refused)");
        int64_t secs = __time64(NULL);
        CHECK(rhu_calendar_get(&c1, &s1) == 0 && c1 >= c0, "shim-calendar", "(the calendar went backwards)");
        int64_t lo = (int64_t)c0 - RHU_SHIM_UNIX_EPOCH_TICKS;
        int64_t hi = (int64_t)c1 - RHU_SHIM_UNIX_EPOCH_TICKS;
        CHECK(pal >= lo && pal <= hi, "shim-calendar", "(the system time is not the kernel's calendar)");
        int64_t rtt = rt.sec * 10000000ll + rt.nsec / 100;
        CHECK(rt.nsec >= 0 && rt.nsec < 1000000000 && rtt >= lo && rtt <= hi, "shim-calendar", "(the real-time clock is not the kernel's calendar)");
        CHECK(secs * 10000000ll <= hi && (secs + 1) * 10000000ll > lo, "shim-calendar", "(time() is not the calendar's whole second)");
        CHECK(s0 >= RHU_CALENDAR_UNSET && s0 <= RHU_CALENDAR_APP && s1 == s0, "shim-calendar", "(the source is not one the kernel names)");
        pass("shim-calendar", " (the system time, the real-time clock and time() read the kernel's calendar; the source named on the wire)");
    }

    /* the real-time clock's absolute sleep follows the calendar, a set included */
    {
        uint32_t w;
        struct ts64 abs;
        uint64_t c;
        int32_t src;
        CHECK(rhu_calendar_set(639028224000000000ull, RHU_CALENDAR_APP) == 0, "shim-time-realtime", "(the calendar call refused the set)");
        __clock_gettime64(0, &abs);
        CHECK(abs.sec >= 1767225600ll && abs.sec < 1767225600ll + 60ll, "shim-time-realtime", "(the real-time clock does not read the set calendar)");
        realtime_after(&abs, 40);
        uint64_t t0 = now_ns();
        CHECK(start_realtime_waiter(&w, 0, &abs) == 0, "shim-time-realtime", "(spawn refused)");
        CHECK(done_within(&g_rt_done, 1, 2000), "shim-time-realtime", "(the absolute sleep on the real-time clock did not end within its bound)");
        CHECK(g_rt_rc == 0 && g_rt_end - t0 >= 40000000ull, "shim-time-realtime", "(the absolute sleep on the real-time clock returned early or refused)");
        realtime_after(&abs, -1000);
        t0 = now_ns();
        CHECK(__clock_nanosleep_time64(0, 1, &abs, NULL) == 0 && now_ns() - t0 < 200000000ull, "shim-time-realtime", "(an expired real-time timestamp did not return at once)");
        abs.sec = -1;
        abs.nsec = 0;
        t0 = now_ns();
        CHECK(__clock_nanosleep_time64(0, 1, &abs, NULL) == 0 && now_ns() - t0 < 200000000ull, "shim-time-realtime", "(a pre-epoch timestamp did not return at once)");
        realtime_after(&abs, 40);
        abs.nsec = 1000000000;
        CHECK(__clock_nanosleep_time64(0, 1, &abs, NULL) == RHU_GERR_EINVAL, "shim-time-realtime", "(a nanosecond field outside its second was not refused)");
        abs.sec = -1;
        abs.nsec = 0;
        CHECK(__clock_nanosleep_time64(0, 0, &abs, NULL) == RHU_GERR_EINVAL, "shim-time-realtime", "(a negative relative request was not refused)");
        realtime_after(&abs, 60);
        CHECK(start_realtime_waiter(&w, 0, &abs) == 0, "shim-time-realtime", "(spawn refused)");
        msleep(20);
        CHECK(rhu_calendar_get(&c, &src) == 0 && rhu_calendar_set(c - 36000000000ull, RHU_CALENDAR_APP) == 0, "shim-time-realtime", "(the calendar call refused the set back)");
        CHECK(!done_within(&g_rt_done, 1, 400), "shim-time-realtime", "(a calendar set back an hour did not hold the sleep)");
        CHECK(rhu_calendar_set(calendar_ticks_of(&abs) + 10000000ull, RHU_CALENDAR_APP) == 0, "shim-time-realtime", "(the calendar call refused the set forward)");
        t0 = now_ns();
        CHECK(done_within(&g_rt_done, 1, 2000), "shim-time-realtime", "(the sleep did not end after the calendar was set past its timestamp)");
        CHECK(g_rt_rc == 0 && g_rt_end - t0 < 500000000ull, "shim-time-realtime", "(the sleep ended later than a slice after the set)");
        pass("shim-time-realtime", " (an absolute sleep on the real-time clock follows the calendar: a future timestamp waited for and never early, an expired or pre-epoch one returned at once, a nanosecond field outside its second and a negative relative request refused; a set back holds it, a set past its timestamp ends it within a slice)");
    }

    /* the real-time clock's timed wait: a condition variable with no attribute */
    {
        uint32_t w;
        struct ts64 abs;
        uint64_t c;
        int32_t src;
        pthread_cond_init(&g_rt_cv, NULL);
        pthread_mutex_init(&g_rt_m, NULL);
        realtime_after(&abs, 40);
        uint64_t t0 = now_ns();
        CHECK(start_realtime_waiter(&w, 1, &abs) == 0, "shim-cond-realtime", "(spawn refused)");
        CHECK(done_within(&g_rt_done, 1, 2000), "shim-cond-realtime", "(the timed wait on the real-time clock did not end within its bound)");
        CHECK(g_rt_rc == RHU_GERR_ETIMEDOUT && g_rt_end - t0 >= 40000000ull, "shim-cond-realtime", "(the timed wait on the real-time clock returned early or without ETIMEDOUT)");
        realtime_after(&abs, -1000);
        pthread_mutex_lock(&g_rt_m);
        t0 = now_ns();
        int rc = __pthread_cond_timedwait64(&g_rt_cv, &g_rt_m, &abs);
        uint64_t dt = now_ns() - t0;
        abs.nsec = 1000000000;
        int rc2 = __pthread_cond_timedwait64(&g_rt_cv, &g_rt_m, &abs);
        pthread_mutex_unlock(&g_rt_m);
        CHECK(rc == RHU_GERR_ETIMEDOUT && dt < 200000000ull, "shim-cond-realtime", "(an expired real-time timestamp did not time out at once)");
        CHECK(rc2 == RHU_GERR_EINVAL, "shim-cond-realtime", "(a nanosecond field outside its second was not refused)");
        realtime_after(&abs, 5000);
        CHECK(start_realtime_waiter(&w, 1, &abs) == 0, "shim-cond-realtime", "(spawn refused)");
        msleep(30);
        pthread_mutex_lock(&g_rt_m);
        pthread_cond_signal(&g_rt_cv);
        pthread_mutex_unlock(&g_rt_m);
        CHECK(done_within(&g_rt_done, 1, 2000) && g_rt_rc == 0, "shim-cond-realtime", "(a signal did not wake a timed wait on the real-time clock)");
        realtime_after(&abs, 60);
        CHECK(start_realtime_waiter(&w, 1, &abs) == 0, "shim-cond-realtime", "(spawn refused)");
        msleep(20);
        CHECK(rhu_calendar_get(&c, &src) == 0 && rhu_calendar_set(c - 36000000000ull, RHU_CALENDAR_APP) == 0, "shim-cond-realtime", "(the calendar call refused the set back)");
        CHECK(!done_within(&g_rt_done, 1, 400), "shim-cond-realtime", "(a calendar set back an hour did not hold the timed wait)");
        CHECK(rhu_calendar_set(calendar_ticks_of(&abs) + 10000000ull, RHU_CALENDAR_APP) == 0, "shim-cond-realtime", "(the calendar call refused the set forward)");
        t0 = now_ns();
        CHECK(done_within(&g_rt_done, 1, 2000), "shim-cond-realtime", "(the timed wait did not end after the calendar was set past its timestamp)");
        CHECK(g_rt_rc == RHU_GERR_ETIMEDOUT && g_rt_end - t0 < 500000000ull, "shim-cond-realtime", "(the timed wait ended later than a slice after the set, or without ETIMEDOUT)");
        pass("shim-cond-realtime", " (a condition variable with no attribute times out on the real-time clock and follows the calendar: a future timestamp timed out and never early, an expired one at once, a nanosecond field outside its second refused, a signal woke it; a set back holds it, a set past its timestamp ends it within a slice)");
    }

    /* the allocation family */
    {
        char *p = (char *)malloc(32);
        CHECK(p != NULL, "shim-alloc", "(malloc refused)");
        memcpy(p, "0123456789", 10);
        p = (char *)realloc(p, 4096);
        CHECK(p != NULL && memcmp(p, "0123456789", 10) == 0, "shim-alloc", "(realloc lost bytes)");
        uint32_t *z = (uint32_t *)calloc(4, 8);
        CHECK(z != NULL && z[0] == 0 && z[7] == 0, "shim-alloc", "(calloc is not zero)");
        CHECK(calloc(0x40000000u, 8) == NULL, "shim-alloc", "(an overflowing calloc did not refuse)");
        void *a64 = NULL;
        CHECK(posix_memalign(&a64, 64, 100) == 0 && ((uintptr_t)a64 & 63u) == 0, "shim-alloc", "(posix_memalign not aligned)");
        void *a256 = memalign(256, 10);
        CHECK(a256 != NULL && ((uintptr_t)a256 & 255u) == 0, "shim-alloc", "(memalign not aligned)");
        free(a256);
        free(a64);
        free(z);
        free(NULL);
#ifdef RHU_SHIMTEST_BREAK_CANARY
        ((uint32_t *)(void *)p)[-1] = 0;
#endif
        free(p);
        pass("shim-alloc", " (realloc keeps bytes; calloc zero and overflow-checked; aligned forms aligned)");
    }

    /* entropy: the two doors */
    {
        uint8_t buf[16];
        uint8_t again[16];
        uint8_t big[600] = {0};
        CHECK(getentropy(buf, sizeof buf) == 0, "shim-entropy", "(getentropy refused)");
        CHECK(getrandom(buf, sizeof buf, 0) == (long)sizeof buf, "shim-entropy", "(getrandom short)");
        CHECK(SystemNative_GetCryptographicallySecureRandomBytes(buf, (int32_t)sizeof buf) == 0, "shim-entropy", "(the cryptographic draw refused)");
        CHECK(SystemNative_GetCryptographicallySecureRandomBytes(again, (int32_t)sizeof again) == 0 && memcmp(buf, again, sizeof buf) != 0, "shim-entropy", "(two cryptographic draws equal)");
        CHECK(SystemNative_GetCryptographicallySecureRandomBytes(big, (int32_t)sizeof big) == 0, "shim-entropy", "(a draw past the call's bound refused)");
        {
            int nonzero = 0;
            for (size_t i = 0; i < sizeof big; i++)
                nonzero += big[i] != 0;
            CHECK(nonzero > 0, "shim-entropy", "(a draw past the call's bound left the buffer zero)");
        }
        CHECK(SystemNative_GetCryptographicallySecureRandomBytes(buf, 0) == 0, "shim-entropy", "(a zero-length draw failed)");
        CHECK(SystemNative_GetCryptographicallySecureRandomBytes(NULL, (int32_t)sizeof buf) == -1, "shim-entropy", "(a refused draw reported as served)");
        CHECK(SystemNative_GetCryptographicallySecureRandomBytes(buf, -1) == -1, "shim-entropy", "(a negative length served)");
        SystemNative_GetNonCryptographicallySecureRandomBytes(buf, (int32_t)sizeof buf);
        pass("shim-entropy", " (cryptographic draws through the kernel's secure entropy call in bounded pieces, all or nothing, a refusal honored; the source declared on the wire)");
    }

    /* the low-level monitor */
    {
        g_mon = SystemNative_LowLevelMonitor_Create();
        CHECK(g_mon != NULL, "shim-llm", "(monitor creation refused)");
        g_done = 0;
        CHECK(spawn(&t1, mon_worker, NULL, WORKER_STACK) == 0, "shim-llm", "(spawn refused)");
        msleep(20);
        SystemNative_LowLevelMonitor_Acquire(g_mon);
        g_mon_ready = 1;
        SystemNative_LowLevelMonitor_Signal_Release(g_mon);
        wait_done(&g_done, 1);
        CHECK(g_mon_saw == 1, "shim-llm", "(the waiter was not woken)");
        SystemNative_LowLevelMonitor_Acquire(g_mon);
        uint64_t t0 = now_ns();
        int32_t rc = SystemNative_LowLevelMonitor_TimedWait(g_mon, 40);
        SystemNative_LowLevelMonitor_Release(g_mon);
        CHECK(rc == 0 && now_ns() - t0 >= 40000000ull, "shim-llm", "(the timed wait did not report its timeout)");
        SystemNative_LowLevelMonitor_Destroy(g_mon);
        pass("shim-llm", " (signal round trip; a timed wait false only on timeout)");
    }

    /* the low-level futex */
    {
        g_done = 0;
        CHECK(spawn(&t1, futex_waiter, NULL, WORKER_STACK) == 0, "shim-futex-pal", "(spawn refused)");
        msleep(20);
        CHECK(g_futex_saw == 0, "shim-futex-pal", "(the waiter did not block)");
        __atomic_store_n(&g_futex_cell, 1, __ATOMIC_RELEASE);
        SystemNative_LowLevelFutex_WakeByAddressSingle(&g_futex_cell);
        wait_done(&g_done, 1);
        CHECK(g_futex_saw == 1, "shim-futex-pal", "(the waiter was not woken)");
        int32_t cell2 = 0;
        uint64_t t0 = now_ns();
        int32_t rc = SystemNative_LowLevelFutex_WaitOnAddressTimeout(&cell2, 0, 50);
        CHECK(rc == 0 && now_ns() - t0 >= 50000000ull, "shim-futex-pal", "(the timed wait did not report its timeout)");
        pass("shim-futex-pal", " (a wait blocks until its wake; a timed wait false only on timeout)");
    }

    /* the program-header walk: the image's own module alone under this map
     * (the window entry the shim appends under the STM32F769's execute-in-place
     * map is absent here, its brackets undefined) and nothing in the quad-SPI region */
    dl_iterate_phdr(phdr_cb, NULL);
    CHECK(g_phdr_load == 1 && g_phdr_exidx == 1, "shim-dlphdr", "(this code is not inside a load segment, or no exception index)");
    CHECK(g_phdr_modules == 1 && g_phdr_qspi == 0, "shim-dlphdr", "(the table does not hold exactly one module under this map, or a load segment lies in the quad-SPI region)");
    pass("shim-dlphdr", " (a load segment spans this code; the exception index segment present; one module in the table under this map, none in the quad-SPI region)");

    /* the environment table */
    {
        const char *v = getenv("DOTNET_gcServer");
        CHECK(v != NULL && strcmp(v, "0") == 0, "shim-env", "(gcServer)");
        v = getenv("DOTNET_gcConcurrent");
        CHECK(v != NULL && strcmp(v, "0") == 0, "shim-env", "(gcConcurrent)");
        /* The thread pool's idle-worker timeout row: the row's default of
         * 100 ms when the image carries no link word, the word's value in
         * decimal when it does (a negative word reads -1). */
        char want[12];
        const char *how;
        if (__rhu_threadpool_thread_timeout_ms == NULL) {
            dec_u32(want, 100u);
            how = " ms, the row's default";
        } else if ((int32_t)(uintptr_t)__rhu_threadpool_thread_timeout_ms < 0) {
            want[0] = '-';
            want[1] = '1';
            want[2] = 0;
            how = " ms, the link word";
        } else {
            dec_u32(want, (uint32_t)(uintptr_t)__rhu_threadpool_thread_timeout_ms);
            how = " ms, the link word";
        }
        v = getenv("DOTNET_ThreadPool_ThreadTimeoutMs");
        CHECK(v != NULL && strcmp(v, want) == 0, "shim-env", "(ThreadPool_ThreadTimeoutMs)");
        v = getenv("HOME");
        CHECK(v != NULL && strcmp(v, "/tmp") == 0, "shim-env", "(HOME)");
        /* The hard limit is the managed heap window's free memory at
         * initialization, in whole pages, less what the row keeps for
         * later draws when that window is the internal one (on this row
         * the frozen object heap's 64 KB and 64 KB left unallocated);
         * window 1 serves the heap alone. */
        CHECK(hard_limit_row() == (heap_ext ? heap_w.free & ~4095u : (free_at_start - 131072u) & ~4095u), "shim-env", "(the hard limit is not the managed heap window's free memory at the start, less what the row keeps from the internal window)");
        CHECK(getenv("NO_SUCH_ROW") == NULL, "shim-env", "(a missing row was answered)");
        {
            char note[LINE_MAX];
            uint32_t n = put(note, 0u, " (the five rows; the thread pool's idle timeout ");
            n = put(note, n, want);
            n = put(note, n, how);
            n = put(note, n, "; the hard limit is the managed heap window's free memory at the start, less what the row keeps when that window is the internal one; a miss NULL)");
            note[n] = 0;
            pass("shim-env", note);
        }
    }

    /* the collector's mapping entry and the managed heap's window */
    {
        rhu_ram_desc w;
        uint32_t which = heap_ext ? 1u : 0u;
        CHECK(rhu_ram_range(which, &w) == 0 && w.lo == heap_w.lo && w.hi == heap_w.hi, "shim-collector-mmap", "(the managed heap window's record)");
        CHECK(sysconf(85) == (long)((w.hi - w.lo) / 4096u) && sysconf(86) == (long)(w.free / 4096u), "shim-collector-mmap", "(the page counts are not the managed heap window's)");
        uint8_t *p = (uint8_t *)rhu_shim_collector_mmap(5000u);
        CHECK(p != (uint8_t *)-1 && p != NULL && ((uintptr_t)p & 4095u) == 0u, "shim-collector-mmap", "(the entry did not answer a page-aligned mapping)");
        CHECK((uint32_t)(uintptr_t)p >= w.lo && (uint32_t)(uintptr_t)p + 8192u <= w.hi, "shim-collector-mmap", "(the mapping lies outside the managed heap's window)");
        uint32_t nonzero = 0u;
        for (uint32_t i = 0u; i < 8192u; i++)
            nonzero |= p[i];
        CHECK(nonzero == 0u, "shim-collector-mmap", "(the mapping is not zero-born)");
        p[0] = 1u;
        p[8191] = 1u;
        rhu_ram_desc after;
        CHECK(rhu_ram_range(which, &after) == 0 && after.free + 8192u <= w.free, "shim-collector-mmap", "(the window's free memory did not fall by the mapping)");
        CHECK(sysconf(86) == (long)(after.free / 4096u), "shim-collector-mmap", "(the available page count did not follow the window)");
        *__errno_location() = 0;
        CHECK(rhu_shim_collector_mmap(0u) == (void *)-1 && *__errno_location() == RHU_GERR_EINVAL, "shim-collector-mmap", "(a zero-length ask was not refused with EINVAL)");
        pass("shim-collector-mmap", " (a page-aligned zero-born mapping inside the managed heap's window; the window's free memory and the page counts follow it; a zero-length ask EINVAL)");
    }

    /* the small identities */
    {
        CHECK(getpid() == 1, "shim-misc", "(pid)");
        CHECK(sysconf(30) == 4096 && sysconf(84) == 1 && sysconf(86) > 0 && sysconf(85) >= sysconf(86), "shim-misc", "(sysconf)");
        CHECK(SystemNative_SysConf(RHU_PAL_SC_PAGESIZE) == 4096 && SystemNative_SysConf(RHU_PAL_SC_CLK_TCK) == 100, "shim-misc", "(PAL SysConf)");
        int64_t a = SystemNative_GetTimestamp();
        msleep(2);
        CHECK(SystemNative_GetTimestamp() > a, "shim-misc", "(the timestamp did not advance)");
        CHECK(SystemNative_GetLowResolutionTimestamp() >= 0, "shim-misc", "(the low-resolution clock)");
        rhu_pal_processcpuinformation cpu;
        CHECK(SystemNative_GetCpuUtilization(&cpu) == 0.0 && cpu.lastRecordedCurrentTime != 0, "shim-misc", "(cpu utilization)");
        CHECK(SystemNative_GetUInt64OSThreadId() == (uint64_t)pthread_self() && SystemNative_SchedGetCpu() == 0, "shim-misc", "(thread id)");
        pass("shim-misc", " (pid 1; page size 4096; one processor; the PAL clocks advance)");
    }

    /* each refused with the right errno */
    {
        CHECK(fork() == -1 && *__errno_location() == RHU_GERR_ENOSYS, "shim-honest", "(fork)");
        CHECK(syscall(999) == -1 && *__errno_location() == RHU_GERR_ENOSYS, "shim-honest", "(unknown syscall)");
        CHECK(open("/nonexistent", 0) == -1 && *__errno_location() == RHU_GERR_ENOENT, "shim-honest", "(open)");
        CHECK(dup2(5, 0) == -1 && *__errno_location() == RHU_GERR_EBADF, "shim-honest", "(dup2)");
        CHECK(opendir("/nonexistent") == NULL && *__errno_location() == RHU_GERR_ENOENT, "shim-honest", "(opendir)");
        CHECK(close(7) == -1 && *__errno_location() == RHU_GERR_EBADF, "shim-honest", "(close)");
        int32_t fds[2];
        CHECK(SystemNative_Pipe(fds, 0) == -1 && SystemNative_GetErrNo() == RHU_GERR_ENOSYS, "shim-honest", "(pipe)");
        pass("shim-honest", " (fork, an unknown syscall, a path, a foreign descriptor, a pipe: each refused with its errno)");
    }

    {
        uint32_t adopt_tid = 0;
        CHECK(spawn_below(&adopt_tid, adopt_second, NULL) == 0, "shim-mutex-adopt", "(the second thread did not spawn)");
        __atomic_store_n(&g_adopt_go, 1, __ATOMIC_RELEASE);
        /* Both checks carry ONE message: which of the two first users a
         * broken adoption refuses is not fixed, and the line on the wire
         * must be the same bytes on every run. */
        CHECK(pthread_mutex_lock(&g_adopt_slot) == 0, "shim-mutex-adopt", "(one of two first users of a zero-filled mutex was refused)");
        pthread_mutex_unlock(&g_adopt_slot);
        wait_done(&g_adopt_done, 1);
        CHECK(g_adopt_rc == 0, "shim-mutex-adopt", "(one of two first users of a zero-filled mutex was refused)");
        pass("shim-mutex-adopt", " (a zero-filled mutex adopted once under two first users)");
    }

    out("SHIMTEST: ALL GOOD (glibc personality + PAL on the kernel API)");
    return 0;
}

#endif /* RHU_SHIMTEST_LADDER_CORE */

#ifdef RHU_SHIMTEST_LADDER_FDMAP

/* Two of these churn the store's tables at once, each through its own
 * file: create with truncation, write a tag, close, reopen, read the tag
 * back, close, unlink; a mismatch or a refused step counts against the
 * worker. */
#define CHURN_ROUNDS 400u
#define CHURN_STACK 4096u
static volatile int g_churn_done;
static uint32_t g_churn_bad[2];
static void *churn_worker(void *a)
{
    unsigned id = (unsigned)(uintptr_t)a;
    const char *path = id == 0 ? "/tmp/churn-0.txt" : "/tmp/churn-1.txt";
    uint8_t out[64];
    uint8_t in[64];
    memset(out, id == 0 ? 0x11 : 0x22, sizeof out);
    for (uint32_t r = 0; r < CHURN_ROUNDS; r++) {
        intptr_t fd = SystemNative_Open(path, RHU_PAL_O_RDWR | RHU_PAL_O_CREAT | RHU_PAL_O_TRUNC, 0x1A4);
        if (fd < 0 || SystemNative_Write(fd, out, 64) != 64 || SystemNative_Close(fd) != 0) {
            g_churn_bad[id]++;
            continue;
        }
        fd = SystemNative_Open(path, RHU_PAL_O_RDONLY, 0);
        if (fd < 0) {
            g_churn_bad[id]++;
            continue;
        }
        memset(in, 0, sizeof in);
        if (SystemNative_Read(fd, in, 64) != 64 || memcmp(in, out, 64) != 0)
            g_churn_bad[id]++;
        SystemNative_Close(fd);
        if (SystemNative_Unlink(path) != 0)
            g_churn_bad[id]++;
    }
    __atomic_add_fetch(&g_churn_done, 1, __ATOMIC_ACQ_REL);
    return NULL;
}

static int is_type(const rhu_pal_filestatus *st, int32_t type)
{
    return (st->Mode & RHU_PAL_S_IFMT) == type;
}

static int fdmap_ladder(void)
{
    rhu_pal_filestatus st;
    char buf[32];

    out("shimtest-fdmap: the descriptor bands and the file surface");
    CHECK(rhu_shim_init() == 0, "shim-fdmap-init", "(initialization refused)");

    /* the console band */
    for (intptr_t fd = 0; fd < 3; fd++) {
        CHECK(SystemNative_FStat(fd, &st) == 0 && is_type(&st, RHU_PAL_S_IFCHR), "shim-fdmap-console", "(a console descriptor is not a character device)");
    }
    CHECK(SystemNative_FStat(3, &st) == -1 && SystemNative_GetErrNo() == RHU_GERR_EBADF, "shim-fdmap-console", "(an unopened descriptor answered)");
    pass("shim-fdmap-console", " (0, 1, 2 are the console; 3 is not open)");

    /* the first file lands in the file band and answers as a file */
    {
        intptr_t fd = SystemNative_Open("/tmp/a.txt", RHU_PAL_O_RDWR | RHU_PAL_O_CREAT | RHU_PAL_O_TRUNC, 0x1A4);
        CHECK(fd >= 0, "shim-fdmap-open-fstat", "(open refused)");
        CHECK(SystemNative_Write(fd, "0123456789", 10) == 10, "shim-fdmap-open-fstat", "(write short)");
        CHECK(SystemNative_FStat(fd, &st) == 0 && is_type(&st, RHU_PAL_S_IFREG), "shim-fdmap-open-fstat", "(the console shape answered for a file)");
        CHECK(fd == 3, "shim-fdmap-open-fstat", "(the first file did not land at the file band's base)");
        CHECK(st.Size == 10 && st.HardLinkCount == 1 && (st.Mode & 0x1FF) == 0x1A4, "shim-fdmap-open-fstat", "(size, link count or mode wrong)");
        pass("shim-fdmap-open-fstat", " (the first file lands at 3; a regular file of 10 bytes, one link, its mode kept)");

        /* end-relative seeks are arithmetic; positional reads leave the position */
        CHECK(SystemNative_LSeek(fd, -4, RHU_PAL_SEEK_END) == 6, "shim-fdmap-seek", "(end-relative seek wrong)");
        CHECK(SystemNative_PRead(fd, buf, 4, 6) == 4 && memcmp(buf, "6789", 4) == 0, "shim-fdmap-seek", "(positional read wrong)");
        CHECK(SystemNative_LSeek(fd, 0, RHU_PAL_SEEK_CUR) == 6, "shim-fdmap-seek", "(a positional read moved the position)");
        CHECK(SystemNative_Close(fd) == 0, "shim-fdmap-seek", "(close refused)");
        pass("shim-fdmap-seek", " (end-relative seek exact; a positional read leaves the position)");
    }

    /* reopen: the descriptor is reused; reads follow the position */
    {
        intptr_t fd = SystemNative_Open("/tmp/a.txt", RHU_PAL_O_RDONLY, 0);
        CHECK(fd == 3, "shim-fdmap-reopen", "(the closed descriptor was not reused)");
        CHECK(SystemNative_Read(fd, buf, 4) == 4 && memcmp(buf, "0123", 4) == 0, "shim-fdmap-reopen", "(first read wrong)");
        CHECK(SystemNative_LSeek(fd, 2, RHU_PAL_SEEK_CUR) == 6, "shim-fdmap-reopen", "(relative seek wrong)");
        CHECK(SystemNative_Read(fd, buf, 8) == 4 && memcmp(buf, "6789", 4) == 0, "shim-fdmap-reopen", "(second read wrong)");
        CHECK(SystemNative_Read(fd, buf, 8) == 0, "shim-fdmap-reopen", "(no end of file)");
        CHECK(SystemNative_Write(fd, "x", 1) == -1 && SystemNative_GetErrNo() == RHU_GERR_EBADF, "shim-fdmap-reopen", "(a write to a read-only descriptor succeeded)");
        CHECK(SystemNative_Close(fd) == 0, "shim-fdmap-reopen", "(close refused)");
        pass("shim-fdmap-reopen", " (the descriptor reused; reads follow the position; end of file 0; read-only refuses writes)");
    }

    /* a closed descriptor is invalid, never a console */
    CHECK(SystemNative_FStat(3, &st) == -1 && SystemNative_GetErrNo() == RHU_GERR_EBADF, "shim-fdmap-closed", "(a closed descriptor answered)");
    CHECK(SystemNative_Read(3, buf, 4) == -1 && SystemNative_GetErrNo() == RHU_GERR_EBADF, "shim-fdmap-closed", "(a closed descriptor read)");
    pass("shim-fdmap-closed", " (a closed descriptor is invalid, never the console shape)");

    /* paths: stat, access, rename, unlink */
    {
        CHECK(SystemNative_Stat("/tmp/a.txt", &st) == 0 && is_type(&st, RHU_PAL_S_IFREG) && st.Size == 10, "shim-fdmap-paths", "(stat)");
        CHECK(SystemNative_Access("/tmp/a.txt", RHU_PAL_R_OK) == 0, "shim-fdmap-paths", "(access)");
        CHECK(SystemNative_Stat("/tmp", &st) == 0 && is_type(&st, RHU_PAL_S_IFDIR), "shim-fdmap-paths", "(the root is not a directory)");
        {
            char canon[64];
            char *r = realpath("/tmp/a.txt", canon);
            CHECK(r == canon && strcmp(canon, "/tmp/a.txt") == 0, "shim-fdmap-paths", "(a present file did not resolve to itself)");
            char *h = realpath("/tmp/a.txt", NULL);
            CHECK(h != NULL && strcmp(h, "/tmp/a.txt") == 0, "shim-fdmap-paths", "(the heap copy did not carry the path)");
            free(h);
            r = realpath("/tmp", canon);
            CHECK(r == canon && strcmp(canon, "/tmp") == 0, "shim-fdmap-paths", "(a directory did not resolve to itself)");
            *__errno_location() = 0;
            CHECK(realpath("/tmp/nothere.txt", canon) == NULL && *__errno_location() == RHU_GERR_ENOENT, "shim-fdmap-paths", "(an absent path did not answer ENOENT)");
        }
        CHECK(SystemNative_Rename("/tmp/a.txt", "/tmp/b.txt") == 0, "shim-fdmap-paths", "(rename)");
        CHECK(SystemNative_Stat("/tmp/a.txt", &st) == -1 && SystemNative_GetErrNo() == RHU_GERR_ENOENT, "shim-fdmap-paths", "(the old name survives)");
        CHECK(SystemNative_Unlink("/tmp/b.txt") == 0, "shim-fdmap-paths", "(unlink)");
        CHECK(SystemNative_Stat("/tmp/b.txt", &st) == -1 && SystemNative_GetErrNo() == RHU_GERR_ENOENT, "shim-fdmap-paths", "(the file survives its unlink)");
        CHECK(SystemNative_Unlink("/tmp/b.txt") == -1 && SystemNative_GetErrNo() == RHU_GERR_ENOENT, "shim-fdmap-paths", "(a second unlink succeeded)");
        pass("shim-fdmap-paths", " (stat, access, rename, unlink; a missing path ENOENT)");
    }

    /* directories */
    {
        CHECK(SystemNative_MkDir("/tmp/d", 0x1ED) == 0, "shim-fdmap-dirs", "(mkdir)");
        CHECK(SystemNative_MkDir("/tmp/d", 0x1ED) == -1 && SystemNative_GetErrNo() == RHU_GERR_EEXIST, "shim-fdmap-dirs", "(a second mkdir succeeded)");
        intptr_t fx = SystemNative_Open("/tmp/d/x.txt", RHU_PAL_O_WRONLY | RHU_PAL_O_CREAT, 0x1A4);
        intptr_t fy = SystemNative_Open("/tmp/d/y.txt", RHU_PAL_O_WRONLY | RHU_PAL_O_CREAT, 0x1A4);
        CHECK(fx >= 0 && fy >= 0 && fx != fy, "shim-fdmap-dirs", "(files in a directory)");
        SystemNative_Close(fx);
        SystemNative_Close(fy);
        CHECK(SystemNative_MkDir("/tmp/d/sub", 0x1ED) == 0, "shim-fdmap-dirs", "(a nested directory)");
        CHECK(SystemNative_Open("/tmp/nodir/z.txt", RHU_PAL_O_WRONLY | RHU_PAL_O_CREAT, 0x1A4) == -1 && SystemNative_GetErrNo() == RHU_GERR_ENOENT, "shim-fdmap-dirs", "(a file under a missing directory was made)");
        void *dir = SystemNative_OpenDir("/tmp/d");
        CHECK(dir != NULL, "shim-fdmap-dirs", "(opendir)");
        rhu_pal_directoryentry e;
        int seen_x = 0;
        int seen_y = 0;
        int seen_sub = 0;
        int n = 0;
        while (SystemNative_ReadDir(dir, &e) == 0) {
            n++;
            if (strcmp(e.Name, "x.txt") == 0 && e.InodeType == RHU_PAL_DT_REG)
                seen_x = 1;
            if (strcmp(e.Name, "y.txt") == 0 && e.InodeType == RHU_PAL_DT_REG)
                seen_y = 1;
            if (strcmp(e.Name, "sub") == 0 && e.InodeType == RHU_PAL_DT_DIR)
                seen_sub = 1;
        }
        CHECK(n == 3 && seen_x && seen_y && seen_sub, "shim-fdmap-dirs", "(the listing is not the three entries)");
        CHECK(SystemNative_CloseDir(dir) == 0, "shim-fdmap-dirs", "(closedir)");
        CHECK(SystemNative_RmDir("/tmp/d") == -1 && SystemNative_GetErrNo() == RHU_GERR_ENOTEMPTY, "shim-fdmap-dirs", "(a non-empty directory was removed)");
        CHECK(SystemNative_Unlink("/tmp/d/x.txt") == 0 && SystemNative_Unlink("/tmp/d/y.txt") == 0, "shim-fdmap-dirs", "(unlink)");
        CHECK(SystemNative_RmDir("/tmp/d/sub") == 0 && SystemNative_RmDir("/tmp/d") == 0, "shim-fdmap-dirs", "(rmdir)");
        CHECK(SystemNative_OpenDir("/tmp/d") == NULL && SystemNative_GetErrNo() == RHU_GERR_ENOENT, "shim-fdmap-dirs", "(a removed directory opens)");
        pass("shim-fdmap-dirs", " (mkdir, a listing of three entries with their types, rmdir refuses non-empty)");
    }

    /* vectored transfers */
    {
        intptr_t fd = SystemNative_Open("/tmp/v.txt", RHU_PAL_O_RDWR | RHU_PAL_O_CREAT | RHU_PAL_O_TRUNC, 0x1A4);
        CHECK(fd >= 0, "shim-fdmap-vectored", "(open)");
        rhu_pal_iovector wv[2] = {{(uint8_t *)"abc", 3}, {(uint8_t *)"defg", 4}};
        CHECK(SystemNative_WriteV(fd, wv, 2) == 7, "shim-fdmap-vectored", "(writev)");
        CHECK(SystemNative_LSeek(fd, 0, RHU_PAL_SEEK_SET) == 0, "shim-fdmap-vectored", "(seek)");
        uint8_t r1[3];
        uint8_t r2[4];
        rhu_pal_iovector rv[2] = {{r1, 3}, {r2, 4}};
        CHECK(SystemNative_ReadV(fd, rv, 2) == 7 && memcmp(r1, "abc", 3) == 0 && memcmp(r2, "defg", 4) == 0, "shim-fdmap-vectored", "(readv)");
        rhu_pal_iovector pw[1] = {{(uint8_t *)"HIJ", 3}};
        CHECK(SystemNative_PWriteV(fd, pw, 1, 7) == 3, "shim-fdmap-vectored", "(pwritev)");
        uint8_t r3[8];
        rhu_pal_iovector pr[1] = {{r3, 7}};
        CHECK(SystemNative_PReadV(fd, pr, 1, 3) == 7 && memcmp(r3, "defgHIJ", 7) == 0, "shim-fdmap-vectored", "(preadv)");
        CHECK(SystemNative_Close(fd) == 0, "shim-fdmap-vectored", "(close)");
        pass("shim-fdmap-vectored", " (gathered writes and reads, positional and not)");
    }

    /* truncate both ways */
    {
        intptr_t fd = SystemNative_Open("/tmp/v.txt", RHU_PAL_O_RDWR, 0);
        CHECK(fd >= 0, "shim-fdmap-truncate", "(open)");
        CHECK(SystemNative_FTruncate(fd, 3) == 0 && SystemNative_FStat(fd, &st) == 0 && st.Size == 3, "shim-fdmap-truncate", "(shrink)");
        CHECK(SystemNative_FTruncate(fd, 8) == 0 && SystemNative_FStat(fd, &st) == 0 && st.Size == 8, "shim-fdmap-truncate", "(grow)");
        CHECK(SystemNative_PRead(fd, buf, 8, 0) == 8 && memcmp(buf, "abc\0\0\0\0\0", 8) == 0, "shim-fdmap-truncate", "(the grown tail is not zero)");
        CHECK(SystemNative_Close(fd) == 0, "shim-fdmap-truncate", "(close)");
        pass("shim-fdmap-truncate", " (shrink keeps the head; grow zero-fills)");
    }

    /* file times: a new object's are the calendar's; a set pair sticks */
    {
        uint64_t c0;
        uint64_t c1;
        int32_t s;
        CHECK(rhu_calendar_get(&c0, &s) == 0, "shim-fdmap-times", "(the calendar call refused)");
        intptr_t fd = SystemNative_Open("/tmp/w.txt", RHU_PAL_O_RDWR | RHU_PAL_O_CREAT | RHU_PAL_O_TRUNC, 0x1A4);
        CHECK(fd >= 0 && SystemNative_FStat(fd, &st) == 0 && SystemNative_Close(fd) == 0 && SystemNative_Unlink("/tmp/w.txt") == 0, "shim-fdmap-times", "(a new file)");
        CHECK(rhu_calendar_get(&c1, &s) == 0, "shim-fdmap-times", "(the calendar call refused)");
        int64_t lo = (int64_t)c0 - RHU_SHIM_UNIX_EPOCH_TICKS;
        int64_t hi = (int64_t)c1 - RHU_SHIM_UNIX_EPOCH_TICKS;
        int64_t mt = st.MTime * 10000000ll + st.MTimeNsec / 100;
        CHECK(st.MTimeNsec >= 0 && st.MTimeNsec < 1000000000 && mt >= lo && mt <= hi && st.ATime == st.MTime && st.ATimeNsec == st.MTimeNsec, "shim-fdmap-times", "(a new file's times are not the calendar's)");
        rhu_pal_timespec times[2] = {{1000, 5}, {2000, 7}};
        CHECK(SystemNative_UTimensat("/tmp/v.txt", times) == 0, "shim-fdmap-times", "(utimensat)");
        CHECK(SystemNative_Stat("/tmp/v.txt", &st) == 0 && st.ATime == 1000 && st.ATimeNsec == 5 && st.MTime == 2000 && st.MTimeNsec == 7, "shim-fdmap-times", "(the times did not stick)");
        pass("shim-fdmap-times", " (a new file's times are the calendar's; access and modification times set and read back)");
    }

    /* the file surface's refusals, each with its errno */
    {
        intptr_t fd = SystemNative_Open("/tmp/v.txt", RHU_PAL_O_RDONLY, 0);
        CHECK(fd >= 0, "shim-fdmap-refusals", "(open)");
        CHECK(SystemNative_Dup(fd) == -1 && SystemNative_GetErrNo() == RHU_GERR_ENOTSUP, "shim-fdmap-refusals", "(dup of a file succeeded)");
        CHECK(SystemNative_Dup(1) == 1, "shim-fdmap-refusals", "(dup of the console)");
        int32_t fds[2];
        CHECK(SystemNative_Pipe(fds, 0) == -1 && SystemNative_GetErrNo() == RHU_GERR_ENOSYS, "shim-fdmap-refusals", "(pipe)");
        CHECK(SystemNative_IsMemfdSupported() == 0, "shim-fdmap-refusals", "(memfd claimed)");
        CHECK(SystemNative_MemfdCreate("m", 0) == -1 && SystemNative_GetErrNo() == RHU_GERR_ENOTSUP, "shim-fdmap-refusals", "(memfd)");
        CHECK(SystemNative_ShmOpen("/s", 0, 0) == -1 && SystemNative_GetErrNo() == RHU_GERR_ENOTSUP, "shim-fdmap-refusals", "(shm open)");
        CHECK(SystemNative_ShmUnlink("/s") == -1 && SystemNative_GetErrNo() == RHU_GERR_ENOENT, "shim-fdmap-refusals", "(shm unlink)");
        CHECK(SystemNative_Close(fd) == 0, "shim-fdmap-refusals", "(close)");
        CHECK(SystemNative_Close(fd) == -1 && SystemNative_GetErrNo() == RHU_GERR_EBADF, "shim-fdmap-refusals", "(a second close succeeded)");
        pass("shim-fdmap-refusals", " (dup of a file, pipe, memfd and shm refused with their errno)");
    }

    /* poll */
    {
        intptr_t fd = SystemNative_Open("/tmp/v.txt", RHU_PAL_O_RDWR, 0);
        CHECK(fd >= 0, "shim-fdmap-poll", "(open)");
        rhu_pal_pollevent ev[3];
        uint32_t triggered = 0;
        ev[0].FileDescriptor = 1;
        ev[0].Events = RHU_PAL_POLLOUT;
        ev[1].FileDescriptor = (int32_t)fd;
        ev[1].Events = RHU_PAL_POLLIN | RHU_PAL_POLLOUT;
        ev[2].FileDescriptor = 0;
        ev[2].Events = RHU_PAL_POLLIN;
        CHECK(SystemNative_Poll(ev, 3, 0, &triggered) == RHU_PAL_SUCCESS && triggered == 2, "shim-fdmap-poll", "(ready set wrong)");
        CHECK(ev[0].TriggeredEvents == RHU_PAL_POLLOUT && ev[1].TriggeredEvents == (RHU_PAL_POLLIN | RHU_PAL_POLLOUT) && ev[2].TriggeredEvents == 0, "shim-fdmap-poll", "(triggered flags wrong)");
        uint64_t t0 = now_ns();
        CHECK(SystemNative_Poll(&ev[2], 1, 20, &triggered) == RHU_PAL_SUCCESS && triggered == 0 && now_ns() - t0 >= 20000000ull, "shim-fdmap-poll", "(the timed wait on the console input returned early)");
        ev[0].FileDescriptor = 50;
        ev[0].Events = RHU_PAL_POLLIN;
        CHECK(SystemNative_Poll(ev, 1, 0, &triggered) == RHU_PAL_SUCCESS && triggered == 1 && ev[0].TriggeredEvents == RHU_PAL_POLLNVAL, "shim-fdmap-poll", "(a descriptor from no band was not invalid)");
        CHECK(SystemNative_Close(fd) == 0, "shim-fdmap-poll", "(close)");
        pass("shim-fdmap-poll", " (console output and files ready at once; the console input waits out its timeout; a foreign descriptor invalid)");
    }

    /* the file store's contract: an object's name and its descriptors are separate */
    {
        intptr_t fa = SystemNative_Open("/tmp/life-a.txt", RHU_PAL_O_RDWR | RHU_PAL_O_CREAT | RHU_PAL_O_TRUNC, 0x1A4);
        SCHECK(fa >= 0 && SystemNative_Write(fa, "AAAA", 4) == 4, "shim-fdmap-lifetime", "(open, write)");
        SCHECK(SystemNative_Unlink("/tmp/life-a.txt") == 0, "shim-fdmap-lifetime", "(unlink of an open file refused)");
        SCHECK(SystemNative_Stat("/tmp/life-a.txt", &st) == -1 && SystemNative_GetErrNo() == RHU_GERR_ENOENT, "shim-fdmap-lifetime", "(the unlinked name still resolves)");
        intptr_t fb = SystemNative_Open("/tmp/life-b.txt", RHU_PAL_O_RDWR | RHU_PAL_O_CREAT | RHU_PAL_O_TRUNC, 0x1A4);
        SCHECK(fb >= 0 && SystemNative_Write(fb, "BBBB", 4) == 4, "shim-fdmap-lifetime", "(a second file)");
        SCHECK(SystemNative_PRead(fa, buf, 4, 0) == 4 && memcmp(buf, "AAAA", 4) == 0, "shim-fdmap-lifetime", "(the unlinked object's descriptor reads another file)");
        SCHECK(SystemNative_FStat(fa, &st) == 0 && st.Size == 4 && st.HardLinkCount == 0, "shim-fdmap-lifetime", "(the unlinked object's status is not four bytes with no link)");
        SCHECK(SystemNative_Close(fa) == 0, "shim-fdmap-lifetime", "(close of the unlinked object)");
        intptr_t fc = SystemNative_Open("/tmp/life-c.txt", RHU_PAL_O_RDWR | RHU_PAL_O_CREAT | RHU_PAL_O_TRUNC, 0x1A4);
        SCHECK(fc >= 0 && SystemNative_Write(fc, "CCCC", 4) == 4, "shim-fdmap-lifetime", "(a third file)");
        SCHECK(SystemNative_Rename("/tmp/life-b.txt", "/tmp/life-c.txt") == 0, "shim-fdmap-lifetime", "(rename over an open target refused)");
        SCHECK(SystemNative_PRead(fc, buf, 4, 0) == 4 && memcmp(buf, "CCCC", 4) == 0, "shim-fdmap-lifetime", "(the replaced object's descriptor reads the replacement)");
        intptr_t fd2 = SystemNative_Open("/tmp/life-c.txt", RHU_PAL_O_RDONLY, 0);
        SCHECK(fd2 >= 0 && SystemNative_Read(fd2, buf, 4) == 4 && memcmp(buf, "BBBB", 4) == 0, "shim-fdmap-lifetime", "(the new name does not read the renamed bytes)");
        SCHECK(SystemNative_Close(fc) == 0 && SystemNative_Close(fd2) == 0 && SystemNative_Close(fb) == 0, "shim-fdmap-lifetime", "(closes)");
        SCHECK(SystemNative_Unlink("/tmp/life-c.txt") == 0, "shim-fdmap-lifetime", "(unlink)");
        pass("shim-fdmap-lifetime", " (an unlinked or replaced object stays with its descriptors until the last close; its name is gone at once)");
    }

    /* a refused open changes nothing: the descriptor comes before the object is created or truncated */
    {
        intptr_t fd = SystemNative_Open("/tmp/order.txt", RHU_PAL_O_RDWR | RHU_PAL_O_CREAT | RHU_PAL_O_TRUNC, 0x1A4);
        SCHECK(fd >= 0 && SystemNative_Write(fd, "KEEP", 4) == 4 && SystemNative_Close(fd) == 0, "shim-fdmap-open-order", "(the file to keep)");
        intptr_t held[RHU_SHIM_FD_FILE_COUNT];
        int n = 0;
        for (; n < RHU_SHIM_FD_FILE_COUNT; n++) {
            held[n] = SystemNative_Open("/tmp/order.txt", RHU_PAL_O_RDONLY, 0);
            if (held[n] < 0)
                break;
        }
        SCHECK(n == RHU_SHIM_FD_FILE_COUNT, "shim-fdmap-open-order", "(the descriptor band did not fill)");
        SCHECK(SystemNative_Open("/tmp/order.txt", RHU_PAL_O_WRONLY | RHU_PAL_O_TRUNC, 0) == -1 && SystemNative_GetErrNo() == RHU_GERR_EMFILE, "shim-fdmap-open-order", "(an open past the band succeeded)");
        SCHECK(SystemNative_Stat("/tmp/order.txt", &st) == 0 && st.Size == 4, "shim-fdmap-open-order", "(a refused open truncated the file)");
        SCHECK(SystemNative_Open("/tmp/order-new.txt", RHU_PAL_O_WRONLY | RHU_PAL_O_CREAT, 0x1A4) == -1 && SystemNative_GetErrNo() == RHU_GERR_EMFILE, "shim-fdmap-open-order", "(an open with create past the band succeeded)");
        SCHECK(SystemNative_Stat("/tmp/order-new.txt", &st) == -1 && SystemNative_GetErrNo() == RHU_GERR_ENOENT, "shim-fdmap-open-order", "(a refused open created the file)");
        for (int i = 0; i < n; i++)
            SystemNative_Close(held[i]);
        SCHECK(SystemNative_Unlink("/tmp/order.txt") == 0, "shim-fdmap-open-order", "(unlink)");
        pass("shim-fdmap-open-order", " (with the descriptor band full an open is refused with EMFILE before it truncates or creates)");
    }

    /* the access mode at the read boundary */
    {
        intptr_t fd = SystemNative_Open("/tmp/wo.txt", RHU_PAL_O_WRONLY | RHU_PAL_O_CREAT | RHU_PAL_O_TRUNC, 0x1A4);
        SCHECK(fd >= 0 && SystemNative_Write(fd, "WO", 2) == 2, "shim-fdmap-access", "(open write-only, write)");
        SCHECK(SystemNative_Read(fd, buf, 2) == -1 && SystemNative_GetErrNo() == RHU_GERR_EBADF, "shim-fdmap-access", "(a write-only descriptor read)");
        SCHECK(SystemNative_PRead(fd, buf, 2, 0) == -1 && SystemNative_GetErrNo() == RHU_GERR_EBADF, "shim-fdmap-access", "(a write-only descriptor read positionally)");
        SCHECK(SystemNative_Close(fd) == 0 && SystemNative_Unlink("/tmp/wo.txt") == 0, "shim-fdmap-access", "(close, unlink)");
        pass("shim-fdmap-access", " (a write-only descriptor refuses reads with EBADF, as a read-only one refuses writes)");
    }

    /* advisory locks per object, held per descriptor */
    {
        intptr_t f1 = SystemNative_Open("/tmp/lock.txt", RHU_PAL_O_RDWR | RHU_PAL_O_CREAT | RHU_PAL_O_TRUNC, 0x1A4);
        intptr_t f2 = SystemNative_Open("/tmp/lock.txt", RHU_PAL_O_RDWR, 0);
        SCHECK(f1 >= 0 && f2 >= 0, "shim-fdmap-flock", "(two descriptors)");
        SCHECK(SystemNative_FLock(f1, RHU_PAL_LOCK_EX | RHU_PAL_LOCK_NB) == 0, "shim-fdmap-flock", "(an exclusive lock refused)");
        SCHECK(SystemNative_FLock(f2, RHU_PAL_LOCK_EX | RHU_PAL_LOCK_NB) == -1 && SystemNative_GetErrNo() == RHU_GERR_EAGAIN, "shim-fdmap-flock", "(a second exclusive lock succeeded)");
        SCHECK(SystemNative_FLock(f2, RHU_PAL_LOCK_SH | RHU_PAL_LOCK_NB) == -1 && SystemNative_GetErrNo() == RHU_GERR_EAGAIN, "shim-fdmap-flock", "(a shared lock under an exclusive one succeeded)");
        SCHECK(SystemNative_FLock(f1, RHU_PAL_LOCK_UN) == 0, "shim-fdmap-flock", "(unlock refused)");
        SCHECK(SystemNative_FLock(f2, RHU_PAL_LOCK_SH | RHU_PAL_LOCK_NB) == 0 && SystemNative_FLock(f1, RHU_PAL_LOCK_SH | RHU_PAL_LOCK_NB) == 0, "shim-fdmap-flock", "(shared locks do not coexist)");
        SCHECK(SystemNative_FLock(f1, RHU_PAL_LOCK_EX | RHU_PAL_LOCK_NB) == -1 && SystemNative_GetErrNo() == RHU_GERR_EAGAIN, "shim-fdmap-flock", "(an exclusive lock over another's shared one succeeded)");
        SCHECK(SystemNative_Close(f2) == 0 && SystemNative_FLock(f1, RHU_PAL_LOCK_EX | RHU_PAL_LOCK_NB) == 0, "shim-fdmap-flock", "(a close did not release its lock)");
        SCHECK(SystemNative_FLock(f1, 16) == -1 && SystemNative_GetErrNo() == RHU_GERR_EINVAL, "shim-fdmap-flock", "(an unknown operation accepted)");
        SCHECK(SystemNative_Close(f1) == 0 && SystemNative_Unlink("/tmp/lock.txt") == 0, "shim-fdmap-flock", "(close, unlink)");
        pass("shim-fdmap-flock", " (exclusive and shared advisory locks per object; a conflicting request is refused with EWOULDBLOCK; a close releases)");
    }

    /* a trailing slash names the same directory */
    {
        SCHECK(SystemNative_MkDir("/tmp/slash", 0x1ED) == 0, "shim-fdmap-dir-slash", "(mkdir)");
        intptr_t fd = SystemNative_Open("/tmp/slash/one.txt", RHU_PAL_O_WRONLY | RHU_PAL_O_CREAT, 0x1A4);
        SCHECK(fd >= 0 && SystemNative_Close(fd) == 0, "shim-fdmap-dir-slash", "(a file in it)");
        SCHECK(SystemNative_Stat("/tmp/slash/", &st) == 0 && is_type(&st, RHU_PAL_S_IFDIR), "shim-fdmap-dir-slash", "(stat with a trailing slash)");
        void *dir = SystemNative_OpenDir("/tmp/slash/");
        SCHECK(dir != NULL, "shim-fdmap-dir-slash", "(opendir with a trailing slash refused)");
        rhu_pal_directoryentry e;
        int n = 0;
        int seen = 0;
        while (dir != NULL && SystemNative_ReadDir(dir, &e) == 0) {
            n++;
            if (strcmp(e.Name, "one.txt") == 0)
                seen = 1;
        }
        SCHECK(n == 1 && seen, "shim-fdmap-dir-slash", "(the listing through a trailing slash is not the one entry)");
        if (dir != NULL)
            SystemNative_CloseDir(dir);
        SCHECK(SystemNative_Unlink("/tmp/slash/one.txt") == 0 && SystemNative_RmDir("/tmp/slash/") == 0, "shim-fdmap-dir-slash", "(unlink, rmdir with a trailing slash)");
        pass("shim-fdmap-dir-slash", " (a trailing slash names the same directory for stat, opendir and rmdir; the listing is whole)");
    }

    /* file times: a nanosecond field outside its range is refused */
    {
        intptr_t fd = SystemNative_Open("/tmp/t.txt", RHU_PAL_O_RDWR | RHU_PAL_O_CREAT | RHU_PAL_O_TRUNC, 0x1A4);
        SCHECK(fd >= 0, "shim-fdmap-times-range", "(open)");
        rhu_pal_timespec bad[2] = {{1000, 1000000000}, {2000, 7}};
        SCHECK(SystemNative_FUTimens(fd, bad) == -1 && SystemNative_GetErrNo() == RHU_GERR_EINVAL, "shim-fdmap-times-range", "(a nanosecond field of one second accepted)");
        rhu_pal_timespec neg[2] = {{1000, 5}, {2000, -1}};
        SCHECK(SystemNative_UTimensat("/tmp/t.txt", neg) == -1 && SystemNative_GetErrNo() == RHU_GERR_EINVAL, "shim-fdmap-times-range", "(a negative nanosecond field accepted)");
        rhu_pal_timespec good[2] = {{1000, 999999999}, {2000, 0}};
        SCHECK(SystemNative_FUTimens(fd, good) == 0 && SystemNative_FStat(fd, &st) == 0 && st.ATimeNsec == 999999999 && st.MTime == 2000 && st.MTimeNsec == 0, "shim-fdmap-times-range", "(the edge values did not stick)");
        SCHECK(SystemNative_Close(fd) == 0 && SystemNative_Unlink("/tmp/t.txt") == 0, "shim-fdmap-times-range", "(close, unlink)");
        pass("shim-fdmap-times-range", " (a nanosecond field outside 0..999999999 is EINVAL; the edges stick)");
    }

    /* the store under two threads: each worker churns its own file through the tables */
    {
        g_churn_bad[0] = 0;
        g_churn_bad[1] = 0;
        uint32_t t1 = 0;
        uint32_t t2 = 0;
        SCHECK(spawn(&t1, churn_worker, (void *)0, CHURN_STACK) == 0 && spawn(&t2, churn_worker, (void *)1, CHURN_STACK) == 0, "shim-fdmap-lock", "(spawn refused)");
        wait_done(&g_churn_done, 2);
        SCHECK(g_churn_bad[0] == 0 && g_churn_bad[1] == 0, "shim-fdmap-lock", "(a worker read bytes it did not write, or lost its file)");
        SCHECK(SystemNative_Stat("/tmp/churn-0.txt", &st) == -1 && SystemNative_Stat("/tmp/churn-1.txt", &st) == -1, "shim-fdmap-lock", "(a churned file survived)");
        pass("shim-fdmap-lock", " (two threads churning the tables: every read returned its writer's bytes and every name came and went)");
    }

    /* the store's bounds: positions, counts and lengths checked in 64 bits against the stated maximum */
    {
        intptr_t fd = SystemNative_Open("/tmp/bounds.txt", RHU_PAL_O_RDWR | RHU_PAL_O_CREAT | RHU_PAL_O_TRUNC, 0x1A4);
        SCHECK(fd >= 0 && SystemNative_Write(fd, "0123", 4) == 4, "shim-fdmap-bounds", "(open, write)");
#if !defined(RHU_SHIMTEST_STORE_PROBE) || defined(RHU_SHIMTEST_STORE_PROBE_GROW)
        SCHECK(SystemNative_FTruncate(fd, 2147483649ll) == -1 && SystemNative_GetErrNo() == RHU_GERR_EFBIG, "shim-fdmap-bounds", "(a truncation past the maximum was accepted)");
#endif
        SCHECK(SystemNative_PWrite(fd, "XYZW", 4, 4294967296ll) == -1 && SystemNative_GetErrNo() == RHU_GERR_EFBIG, "shim-fdmap-bounds", "(a write at 4 GiB was not refused with EFBIG)");
        SCHECK(SystemNative_PRead(fd, buf, 4, 0) == 4 && memcmp(buf, "0123", 4) == 0, "shim-fdmap-bounds", "(a write at 4 GiB reached byte 0)");
        SCHECK(SystemNative_PWrite(fd, "XYZW", 4, -4294967296ll) == -1 && SystemNative_GetErrNo() == RHU_GERR_EINVAL, "shim-fdmap-bounds", "(a negative write offset was not refused)");
        SCHECK(SystemNative_PRead(fd, buf, 4, -1) == -1 && SystemNative_GetErrNo() == RHU_GERR_EINVAL, "shim-fdmap-bounds", "(a negative read offset was not refused)");
        SCHECK(SystemNative_FTruncate(fd, -1) == -1 && SystemNative_GetErrNo() == RHU_GERR_EINVAL, "shim-fdmap-bounds", "(a negative truncation was accepted)");
        SCHECK(SystemNative_LSeek(fd, 1048576, RHU_PAL_SEEK_SET) == 1048576, "shim-fdmap-bounds", "(a seek to the maximum was refused)");
        SCHECK(SystemNative_LSeek(fd, 0xFFFFFFFFll, RHU_PAL_SEEK_SET) == -1 && SystemNative_GetErrNo() == RHU_GERR_EINVAL, "shim-fdmap-bounds", "(a seek past the maximum was accepted)");
        SCHECK(SystemNative_Write(fd, "x", 1) == -1 && SystemNative_GetErrNo() == RHU_GERR_EFBIG, "shim-fdmap-bounds", "(a write past the maximum was accepted)");
        SCHECK(SystemNative_FStat(fd, &st) == 0 && st.Size == 4, "shim-fdmap-bounds", "(the size moved on a refused write)");
        /* the offset's own bound before any sum: near the top of the range, at the maximum and one past it, whole and empty, direct and vectored */
        int64_t mt0 = st.MTime;
        SCHECK(SystemNative_PWrite(fd, "XYZW", 4, 9223372036854775805ll) == -1 && SystemNative_GetErrNo() == RHU_GERR_EFBIG, "shim-fdmap-bounds", "(a write near the top of the offset range was not refused with EFBIG)");
        SCHECK(SystemNative_PWrite(fd, "XYZW", 0, 9223372036854775807ll) == -1 && SystemNative_GetErrNo() == RHU_GERR_EFBIG, "shim-fdmap-bounds", "(an empty write at the top of the offset range was accepted)");
        SCHECK(SystemNative_PWrite(fd, "XYZW", 0, 1048577ll) == -1 && SystemNative_GetErrNo() == RHU_GERR_EFBIG, "shim-fdmap-bounds", "(an empty write one past the maximum was accepted)");
        SCHECK(SystemNative_PWrite(fd, "XYZW", 1, 1048576ll) == -1 && SystemNative_GetErrNo() == RHU_GERR_EFBIG, "shim-fdmap-bounds", "(a one-byte write at the maximum was accepted)");
        SCHECK(SystemNative_PWrite(fd, "XYZW", 0, 1048576ll) == 0, "shim-fdmap-bounds", "(an empty write at the maximum was refused)");
        rhu_pal_iovector top[1] = {{(uint8_t *)"XYZW", 4}};
        SCHECK(SystemNative_PWriteV(fd, top, 1, 9223372036854775805ll) == -1 && SystemNative_GetErrNo() == RHU_GERR_EFBIG, "shim-fdmap-bounds", "(a vectored write near the top of the offset range was not refused with EFBIG)");
        SCHECK(SystemNative_PWriteV(fd, top, 1, -1) == -1 && SystemNative_GetErrNo() == RHU_GERR_EINVAL, "shim-fdmap-bounds", "(a vectored write at a negative offset was not refused)");
        SCHECK(SystemNative_PRead(fd, buf, 4, 9223372036854775807ll) == 0, "shim-fdmap-bounds", "(a read at the top of the offset range did not answer end of file)");
        SCHECK(SystemNative_FStat(fd, &st) == 0 && st.Size == 4 && st.MTime == mt0, "shim-fdmap-bounds", "(the size or the modification time moved on a refused write near the top of the range)");
        {
            /* the last valid byte: admitted by the bound; the row's heap decides whether the object can grow to the maximum */
            int32_t last = SystemNative_PWrite(fd, "Z", 1, 1048575ll);
            SCHECK(last == 1 || (last == -1 && SystemNative_GetErrNo() == RHU_GERR_ENOSPC), "shim-fdmap-bounds", "(the last valid byte was refused by the bound)");
            if (last == 1)
                SCHECK(SystemNative_FStat(fd, &st) == 0 && st.Size == 1048576 && SystemNative_FTruncate(fd, 4) == 0, "shim-fdmap-bounds", "(the size did not follow the last valid byte, or the truncation back failed)");
            else
                SCHECK(SystemNative_FStat(fd, &st) == 0 && st.Size == 4, "shim-fdmap-bounds", "(the size moved on a write refused for want of space)");
        }
        SCHECK(SystemNative_Close(fd) == 0 && SystemNative_Unlink("/tmp/bounds.txt") == 0, "shim-fdmap-bounds", "(close, unlink)");
        pass("shim-fdmap-bounds", " (offsets, counts and lengths checked in 64 bits against the stated maximum: EFBIG past it, EINVAL below zero, the bytes and the size untouched)");
    }

    /* A refused open of the process identity file gives back the object slot it took */
    {
        char ib[16];
        intptr_t held[RHU_SHIM_FD_FILE_COUNT];
        held[0] = SystemNative_Open("/tmp/ident.txt", RHU_PAL_O_RDWR | RHU_PAL_O_CREAT | RHU_PAL_O_TRUNC, 0x1A4);
        SCHECK(held[0] >= 0, "shim-fdmap-identity-leak", "(the file to hold)");
        int n = 1;
        for (; n < RHU_SHIM_FD_FILE_COUNT; n++) {
            held[n] = SystemNative_Open("/tmp/ident.txt", RHU_PAL_O_RDONLY, 0);
            if (held[n] < 0)
                break;
        }
        SCHECK(n == RHU_SHIM_FD_FILE_COUNT, "shim-fdmap-identity-leak", "(the descriptor band did not fill)");
        /* more refused identity opens than the store has object slots: each must give back the slot it took */
        for (int i = 0; i < 16; i++)
            SCHECK(SystemNative_Open("/proc/self/stat", RHU_PAL_O_RDONLY, 0) == -1, "shim-fdmap-identity-leak", "(an identity open past the band succeeded)");
        SCHECK(SystemNative_Close(held[n - 1]) == 0, "shim-fdmap-identity-leak", "(close)");
        intptr_t id = SystemNative_Open("/proc/self/stat", RHU_PAL_O_RDONLY, 0);
        SCHECK(id >= 0, "shim-fdmap-identity-leak", "(the identity file did not open once a descriptor was free: a refused open kept the slot it took)");
        SCHECK(SystemNative_Read(id, ib, 11) == 11 && memcmp(ib, "1 (program)", 11) == 0, "shim-fdmap-identity-leak", "(the identity line)");
        SCHECK(SystemNative_Close(id) == 0, "shim-fdmap-identity-leak", "(close)");
        for (int i = 0; i < n - 1; i++)
            SystemNative_Close(held[i]);
        SCHECK(SystemNative_Unlink("/tmp/ident.txt") == 0, "shim-fdmap-identity-leak", "(unlink)");
        pass("shim-fdmap-identity-leak", " (sixteen refused opens of the identity file past the band, each giving back the slot it took; the file opens and reads once a descriptor is free)");
    }

    out("SHIMTEST-FDMAP: ALL GOOD (explicit descriptor bands; the console never answers for a file)");
    return 0;
}

#endif /* RHU_SHIMTEST_LADDER_FDMAP */

#ifdef RHU_SHIMTEST_LADDER_ACTIVATION

/* --- the signal surface, declared as the caller sees it --- */
int sigaction(int signo, const rhu_glibc_sigaction *act, rhu_glibc_sigaction *oldact);
void *signal(int signo, void *handler);
int sigemptyset(rhu_glibc_sigset *set);
int sigfillset(rhu_glibc_sigset *set);
int sigaddset(rhu_glibc_sigset *set, int signo);
int sigdelset(rhu_glibc_sigset *set, int signo);
int sigismember(const rhu_glibc_sigset *set, int signo);
int pthread_sigmask(int how, const rhu_glibc_sigset *set, rhu_glibc_sigset *old);
int pthread_kill(uint32_t tid, int sig);
int __libc_current_sigrtmin(void);
int __libc_current_sigrtmax(void);

/* Workers take 8 KB: the activation's two records and the handler's own
 * frames ride on the interrupted thread's stack. One worker lives at a
 * time, so every spawn reuses the retired carve exactly. */
#define ACT_WORKER_STACK 8192u

static int g_actsig;
static volatile int g_act_done;

/* The spin window: an assembly loop whose first and last addresses are
 * exported as symbols, so the interrupted program counter has an exact
 * range to be tested against. */
extern char rhu_act_spin_lo[];
extern char rhu_act_spin_hi[];
static volatile uint32_t g_spin_release;

__attribute__((noinline)) static void spin_in_window(void)
{
    __asm__ volatile(
        ".globl rhu_act_spin_lo\n\t"
        "rhu_act_spin_lo:\n\t"
        "1:\n\t"
        "ldr r3, [%0]\n\t"
        "cmp r3, #0\n\t"
        "beq 1b\n\t"
        ".globl rhu_act_spin_hi\n\t"
        "rhu_act_spin_hi:\n\t"
        :
        : "r"(&g_spin_release)
        : "r3", "memory", "cc");
}

enum act_mode { ACT_DELIVER = 1, ACT_SETCTX = 2, ACT_BLOCK = 3, ACT_COUNT = 4 };
static volatile int g_act_mode;
static volatile uint32_t g_act_count;
static volatile int g_act_signo;
static volatile int g_act_code;
static volatile int g_act_pid;
static volatile uint32_t g_act_uid;
static volatile uint32_t g_act_tid;
static volatile uint32_t g_act_local;
static volatile uint32_t g_act_ctx_sp;
static volatile uint32_t g_act_ctx_pc;
static volatile uint32_t g_act_mask1;
static volatile uint32_t g_act_regspace0;
static volatile uint32_t g_act_ucflags;
static volatile uint32_t g_act_sssp;
static volatile int g_block_inside;
static int32_t g_block_release;

static volatile uint32_t g_pad_magic;
static volatile uint32_t g_pad_tid;
static volatile int g_pad_ran;

#define ACT_MAGIC 0xC0DEF00Du

/* The landing function a redirected thread resumes into: r0 carries the
 * magic the handler planted; the thread then ends. */
__attribute__((noinline)) static void landing_pad(uint32_t magic)
{
    g_pad_magic = magic;
    g_pad_tid = (uint32_t)syscall(224);
    g_pad_ran = 1;
    rhu_thread_exit();
}

static void act_handler(int signo, rhu_glibc_siginfo *si, void *ctx)
{
    rhu_glibc_ucontext *uc = (rhu_glibc_ucontext *)ctx;
    uint32_t local = 0;
    g_act_count = g_act_count + 1;
    g_act_signo = signo;
    g_act_code = si->si_code;
    g_act_pid = si->si_pid;
    g_act_uid = si->si_uid;
    g_act_tid = (uint32_t)syscall(224);
    g_act_local = (uint32_t)(uintptr_t)&local;
    g_act_ctx_sp = uc->uc_mcontext.arm_sp;
    g_act_ctx_pc = uc->uc_mcontext.arm_pc;
    g_act_mask1 = uc->uc_sigmask.word[1];
    g_act_regspace0 = uc->uc_regspace[0];
    g_act_ucflags = uc->uc_flags;
    g_act_sssp = (uint32_t)(uintptr_t)uc->uc_stack.ss_sp;
    if (g_act_mode == ACT_SETCTX) {
        uc->uc_mcontext.arm_r0 = ACT_MAGIC;
        uc->uc_mcontext.arm_pc = (uint32_t)(uintptr_t)landing_pad;
        uc->uc_mcontext.arm_sp += 8;
    } else if (g_act_mode == ACT_BLOCK) {
        g_block_inside = 1;
        while (__atomic_load_n(&g_block_release, __ATOMIC_ACQUIRE) == 0)
            SystemNative_LowLevelFutex_WaitOnAddress(&g_block_release, 0);
    }
}

static void plain_handler(int signo)
{
    (void)signo;
}

static int32_t posix_signal_cb(int32_t code, rhu_pal_posixsignal sig)
{
    (void)code;
    (void)sig;
    return 0;
}

static int32_t sigchld_cb(int32_t reapAll, int32_t configureConsole)
{
    (void)reapAll;
    (void)configureConsole;
    return 0;
}

static void void_cb(void)
{
}

static volatile uint32_t g_w_tid;
static volatile uint32_t g_w_lo;
static volatile uint32_t g_w_len;
static volatile int g_w_spinning;

/* The spinning worker: blocks the activation signal in its own mask
 * (bookkeeping the context must report), publishes its identity and its
 * stack carve, then spins in the window until released. */
static void *act_spin_worker(void *a)
{
    (void)a;
    rhu_glibc_sigset s;
    attr_slot at;
    void *lo = NULL;
    size_t len = 0;
    sigemptyset(&s);
    sigaddset(&s, g_actsig);
    pthread_sigmask(RHU_GLIBC_SIG_BLOCK, &s, NULL);
    pthread_getattr_np(pthread_self(), &at);
    pthread_attr_getstack(&at, &lo, &len);
    g_w_lo = (uint32_t)(uintptr_t)lo;
    g_w_len = (uint32_t)len;
    g_w_tid = pthread_self();
    g_w_spinning = 1;
    spin_in_window();
    __atomic_fetch_add(&g_act_done, 1, __ATOMIC_RELEASE);
    return NULL;
}

static volatile uint32_t g_c_progress;
static volatile uint32_t g_c_release;

/* The counting worker: spins on a counter the main thread samples. */
static void *act_count_worker(void *a)
{
    (void)a;
    g_w_tid = pthread_self();
    while (g_c_release == 0)
        g_c_progress = g_c_progress + 1;
    __atomic_fetch_add(&g_act_done, 1, __ATOMIC_RELEASE);
    return NULL;
}

static mutex_slot g_e_m;
static cond_slot g_e_cv;
static volatile int g_e_ready;
static volatile int g_e_parking;
static volatile uint32_t g_e_returns;

/* The waiting worker: parks in a condition wait on a false predicate,
 * counting every return of the wait. */
static void *act_wait_worker(void *a)
{
    (void)a;
    g_w_tid = pthread_self();
    pthread_mutex_lock(&g_e_m);
    g_e_parking = 1;
    while (!g_e_ready) {
        pthread_cond_wait(&g_e_cv, &g_e_m);
        g_e_returns = g_e_returns + 1;
    }
    pthread_mutex_unlock(&g_e_m);
    __atomic_fetch_add(&g_act_done, 1, __ATOMIC_RELEASE);
    return NULL;
}

/* Poll a counter up to a bounded number of 5 ms slices. */
static int poll_until(volatile uint32_t *p, uint32_t want, int slices)
{
    for (int i = 0; i < slices; i++) {
        if (*p >= want)
            return 1;
        msleep(5);
    }
    return *p >= want;
}

static int poll_flag(volatile int *p, int slices)
{
    for (int i = 0; i < slices; i++) {
        if (*p != 0)
            return 1;
        msleep(5);
    }
    return *p != 0;
}

static int in_window(uint32_t v, uint32_t lo, uint32_t len)
{
    return v >= lo && v < lo + len;
}

/* --- The kernel's null fault delivered as SIGSEGV --- */

/* r4 to r11 hold FAULT_BASE + 4 .. FAULT_BASE + 11 at the faulting load, so
 * the handler can tell the context's registers are the worker's own. */
#define FAULT_BASE 0x40000000u
static volatile uint32_t g_f_count;
static volatile int g_f_signo;
static volatile int g_f_code;
static volatile uint32_t g_f_addr;
static volatile uint32_t g_f_pc;
static volatile uint32_t g_f_sp;
static volatile int g_f_regs_ok;
static volatile uint32_t g_f_tid;
static volatile uint32_t g_f_sp_edit;   /* the case: 0 = a plain redirect; 8 = the stack pointer moved up by 8 */
static volatile uint32_t g_fl_sp;       /* the stack pointer the landing saw at its entry */
static volatile uint32_t g_fl_expected; /* the handler's prediction: the fault's stack pointer plus the edit */
static volatile uint32_t g_fl_tid;
static volatile int g_fl_ran;
static volatile uint32_t g_f_word;

/* The faulting load: a label in the worker's assembly, so the handler can
 * assert the context's pc is the instruction itself. */
extern const uint8_t rhu_shimtest_fault_site[];

/* The landing's C half (external linkage: the stub below names it). The
 * thread ends here, as a redirected activation's landing does. */
void rhu_shimtest_fault_land(uint32_t sp_at_entry, uint32_t expected);
void rhu_shimtest_fault_land(uint32_t sp_at_entry, uint32_t expected)
{
    g_fl_sp = sp_at_entry;
    g_fl_expected = expected;
    g_fl_tid = (uint32_t)syscall(224);
    g_fl_ran = 1;
    rhu_thread_exit();
}

/* The landing a redirected fault resumes into: the stack pointer AT ENTRY
 * in r0 (read before any frame is pushed), the handler's prediction in r1
 * (planted through the context; the costume's trampoline keeps r0 and r1). */
__attribute__((naked, noinline)) static void fault_landing(void)
{
    __asm__ volatile("mov r0, sp\n\tb rhu_shimtest_fault_land");
}

/* The SA_SIGINFO handler for SIGSEGV: records the Linux-shaped record and the
 * context, then redirects the thread to the landing; in the second case it
 * also moves the stack pointer up by 8, which the shim's trampoline must
 * honour because the kernel never writes a thread's stack pointer back. */
static void fault_handler(int signo, rhu_glibc_siginfo *si, void *ctx)
{
    rhu_glibc_ucontext *uc = (rhu_glibc_ucontext *)ctx;
    g_f_count = g_f_count + 1;
    g_f_signo = signo;
    g_f_code = si->si_code;
    g_f_addr = si->si_addr;
    g_f_pc = uc->uc_mcontext.arm_pc;
    g_f_sp = uc->uc_mcontext.arm_sp;
    g_f_tid = (uint32_t)syscall(224);
    g_f_regs_ok = uc->uc_mcontext.arm_r4 == FAULT_BASE + 4 && uc->uc_mcontext.arm_r5 == FAULT_BASE + 5 &&
                  uc->uc_mcontext.arm_r6 == FAULT_BASE + 6 && uc->uc_mcontext.arm_r7 == FAULT_BASE + 7 &&
                  uc->uc_mcontext.arm_r8 == FAULT_BASE + 8 && uc->uc_mcontext.arm_r9 == FAULT_BASE + 9 &&
                  uc->uc_mcontext.arm_r10 == FAULT_BASE + 10 && uc->uc_mcontext.arm_fp == FAULT_BASE + 11;
    uc->uc_mcontext.arm_r1 = uc->uc_mcontext.arm_sp + g_f_sp_edit;
    uc->uc_mcontext.arm_pc = (uint32_t)(uintptr_t)fault_landing;
    uc->uc_mcontext.arm_sp += g_f_sp_edit;
}

/* The faulting worker: publishes its identity and its stack carve, loads
 * r4 to r11 with the known words and reads one word through null at offset
 * 8 at the global label. The read faults (the first page is no-access on
 * every row), the kernel delivers it through the shim's costume, and the
 * handler redirects the thread to the landing, so the code after the load
 * never runs. The null pointer comes through a volatile word so the
 * compiler cannot see it and fold the load away. */
static void *act_fault_worker(void *a)
{
    (void)a;
    volatile uint32_t zero = 0;
    uint32_t nul = zero;
    uint32_t word;
    attr_slot at;
    void *lo = NULL;
    size_t len = 0;
    pthread_getattr_np(pthread_self(), &at);
    pthread_attr_getstack(&at, &lo, &len);
    g_w_lo = (uint32_t)(uintptr_t)lo;
    g_w_len = (uint32_t)len;
    g_w_tid = pthread_self();
    __asm__ volatile(
        "movw r4, #4\n\tmovt r4, #0x4000\n\t"
        "movw r5, #5\n\tmovt r5, #0x4000\n\t"
        "movw r6, #6\n\tmovt r6, #0x4000\n\t"
        "movw r7, #7\n\tmovt r7, #0x4000\n\t"
        "movw r8, #8\n\tmovt r8, #0x4000\n\t"
        "movw r9, #9\n\tmovt r9, #0x4000\n\t"
        "movw r10, #10\n\tmovt r10, #0x4000\n\t"
        "movw r11, #11\n\tmovt r11, #0x4000\n\t"
        "mov r1, %[nul]\n\t"
        ".globl rhu_shimtest_fault_site\n"
        "rhu_shimtest_fault_site:\n\t"
        "ldr %[w], [r1, #8]\n\t"
        : [w] "=r"(word)
        : [nul] "r"(nul)
        : "r1", "r4", "r5", "r6", "r7", "r8", "r9", "r10", "r11", "memory", "cc");
    g_f_word = word;
    return NULL;
}

static int activation_ladder(void)
{
    uint32_t t1;
    uint32_t base_count;
    rhu_glibc_sigset s;
    rhu_glibc_sigaction act;
    rhu_glibc_sigaction old;
    uint32_t spin_lo = (uint32_t)(uintptr_t)rhu_act_spin_lo;
    uint32_t spin_hi = (uint32_t)(uintptr_t)rhu_act_spin_hi;

    out("shimtest-activation: the activation compatibility adapter on the kernel's delivery, and the signal rows");

    /* init: the realtime floor, the set operations, the mask bookkeeping */
    CHECK(rhu_shim_init() == 0, "shim-act-init", "(initialization refused)");
    g_actsig = __libc_current_sigrtmin();
    CHECK(g_actsig == RHU_GLIBC_SIGRTMIN_CURRENT && __libc_current_sigrtmax() == RHU_GLIBC_SIGRTMAX, "shim-act-init", "(the realtime range)");
    CHECK(sigemptyset(&s) == 0, "shim-act-init", "(sigemptyset)");
    {
        int zero = 1;
        for (int i = 0; i < 32; i++)
            zero &= s.word[i] == 0;
        CHECK(zero, "shim-act-init", "(an emptied set is not zero)");
    }
    CHECK(sigaddset(&s, g_actsig) == 0 && s.word[1] == 2u && s.word[0] == 0, "shim-act-init", "(sigaddset did not set bit 33)");
    CHECK(sigismember(&s, g_actsig) == 1 && sigismember(&s, g_actsig + 1) == 0, "shim-act-init", "(sigismember)");
    CHECK(sigdelset(&s, g_actsig) == 0 && s.word[1] == 0, "shim-act-init", "(sigdelset)");
    CHECK(sigfillset(&s) == 0 && s.word[0] == 0xFFFFFFFFu && s.word[31] == 0xFFFFFFFFu, "shim-act-init", "(sigfillset)");
    CHECK(sigaddset(&s, 0) == -1 && *__errno_location() == RHU_GERR_EINVAL, "shim-act-init", "(sigaddset 0 was not refused)");
    CHECK(sigaddset(&s, 65) == -1 && *__errno_location() == RHU_GERR_EINVAL, "shim-act-init", "(sigaddset 65 was not refused)");
    sigemptyset(&s);
    sigaddset(&s, g_actsig);
    CHECK(pthread_sigmask(RHU_GLIBC_SIG_BLOCK, &s, NULL) == 0, "shim-act-init", "(block)");
    {
        rhu_glibc_sigset q;
        CHECK(pthread_sigmask(RHU_GLIBC_SIG_SETMASK, NULL, &q) == 0 && q.word[1] == 2u && q.word[0] == 0, "shim-act-init", "(the mask did not read back the blocked signal)");
        CHECK(pthread_sigmask(RHU_GLIBC_SIG_UNBLOCK, &s, &q) == 0 && q.word[1] == 2u, "shim-act-init", "(unblock)");
        CHECK(pthread_sigmask(RHU_GLIBC_SIG_SETMASK, NULL, &q) == 0 && q.word[1] == 0, "shim-act-init", "(the mask did not clear)");
        CHECK(pthread_sigmask(7, &s, NULL) == RHU_GERR_EINVAL, "shim-act-init", "(an unknown operation was not refused)");
    }
    pass("shim-act-init", " (the realtime floor; the set operations; the mask blocks, reports and clears)");

    /* refusals before any registration */
    CHECK(pthread_kill(pthread_self(), g_actsig) == RHU_GERR_EINVAL, "shim-act-refuse", "(a send with no handler recorded was not refused)");
    CHECK(pthread_kill(pthread_self(), RHU_GSIG_SIGUSR1) == RHU_GERR_EINVAL, "shim-act-refuse", "(a non-activation signal was not refused)");
    CHECK(pthread_kill(4095u, 0) == RHU_GERR_ESRCH, "shim-act-refuse", "(an unknown thread was not refused)");
    CHECK(pthread_kill(pthread_self(), 0) == 0, "shim-act-refuse", "(the existence probe of self failed)");
    memset(&act, 0, sizeof act);
    act.sa_handler = (void *)plain_handler;
    CHECK(sigaction(RHU_GSIG_SIGKILL, &act, NULL) == -1 && *__errno_location() == RHU_GERR_EINVAL, "shim-act-refuse", "(SIGKILL was recorded)");
    CHECK(sigaction(0, &act, NULL) == -1 && sigaction(65, &act, NULL) == -1, "shim-act-refuse", "(a signal outside the range was recorded)");
    pass("shim-act-refuse", " (a send with no handler, a non-activation signal, an unknown thread, an unrecordable signal: each refused with its errno)");

    /* registration: the query form, the install, the read-back */
    CHECK(sigaction(g_actsig, NULL, &old) == 0 && old.sa_handler == RHU_GLIBC_SIG_DFL && old.sa_flags == 0, "shim-act-register", "(the query form did not read the default disposition)");
    memset(&act, 0, sizeof act);
    act.sa_handler = (void *)act_handler;
    act.sa_flags = RHU_GLIBC_SA_SIGINFO | RHU_GLIBC_SA_RESTART;
    sigemptyset(&act.sa_mask);
    CHECK(sigaction(g_actsig, &act, &old) == 0 && old.sa_handler == RHU_GLIBC_SIG_DFL, "shim-act-register", "(the install refused or the old disposition moved)");
    CHECK(sigaction(g_actsig, NULL, &old) == 0 && old.sa_handler == (void *)act_handler && old.sa_flags == (RHU_GLIBC_SA_SIGINFO | RHU_GLIBC_SA_RESTART), "shim-act-register", "(the recorded disposition did not read back)");
    CHECK(signal(RHU_GSIG_SIGFPE, (void *)plain_handler) == RHU_GLIBC_SIG_DFL, "shim-act-register", "(signal did not answer the default disposition)");
    CHECK(signal(RHU_GSIG_SIGFPE, (void *)plain_handler) == (void *)plain_handler, "shim-act-register", "(signal did not answer the previous handler)");
    memset(&act, 0, sizeof act);
    act.sa_handler = (void *)act_handler;
    act.sa_flags = RHU_GLIBC_SA_SIGINFO | RHU_GLIBC_SA_RESTART;
    CHECK(sigaction(RHU_GSIG_SIGSEGV, &act, NULL) == 0, "shim-act-register", "(a fault signal's handler was not recorded)");
    pass("shim-act-register", " (the query form reads the default; the install keeps the old disposition; the read-back matches; a one-argument disposition answers its predecessor)");

    /* The kernel's null fault delivered as SIGSEGV. The
     * SIGSEGV recording above armed the kernel's delivery (the shim's line
     * says so); this handler replaces it as the disposition. Two cases: a
     * plain redirect, then a redirect with the stack pointer moved up by 8,
     * which the landing sees as exactly 8 (the shim's trampoline). */
    memset(&act, 0, sizeof act);
    act.sa_handler = (void *)fault_handler;
    act.sa_flags = RHU_GLIBC_SA_SIGINFO;
    CHECK(sigaction(RHU_GSIG_SIGSEGV, &act, &old) == 0 && old.sa_handler == (void *)act_handler, "shim-act-fault", "(the fault handler's recording refused or the old disposition moved)");
    for (int leg = 0; leg < 2; leg++) {
        g_f_sp_edit = leg == 0 ? 0u : 8u;
        g_f_count = 0;
        g_fl_ran = 0;
        g_fl_sp = 0;
        g_fl_expected = 1;
        CHECK(spawn(&t1, act_fault_worker, NULL, ACT_WORKER_STACK) == 0, "shim-act-fault", "(spawn refused)");
        CHECK(poll_flag(&g_fl_ran, 200), "shim-act-fault", leg == 0 ? "(the landing never ran after the plain redirect)" : "(the landing never ran after the stack-pointer edit)");
        CHECK(g_f_count == 1, "shim-act-fault", "(the handler did not run exactly once)");
        CHECK(g_f_signo == RHU_GSIG_SIGSEGV, "shim-act-fault", "(the signal number is not SIGSEGV)");
        CHECK(g_f_code == 1, "shim-act-fault", "(the origin code is not SEGV_MAPERR)");
        CHECK(g_f_addr == 8u, "shim-act-fault", "(the faulting address is not 8)");
        CHECK(g_f_pc == (uint32_t)(uintptr_t)rhu_shimtest_fault_site, "shim-act-fault", "(the context's pc is not the faulting load)");
        CHECK(g_f_regs_ok, "shim-act-fault", "(r4 to r11 in the context are not the worker's)");
        CHECK(g_f_tid == g_w_tid && g_w_tid == t1 && g_fl_tid == t1, "shim-act-fault", "(the handler or the landing ran on another thread)");
        CHECK(in_window(g_f_sp, g_w_lo, g_w_len), "shim-act-fault", "(the faulting stack pointer is outside the worker's stack)");
        CHECK(g_fl_sp == g_fl_expected && g_fl_sp == g_f_sp + g_f_sp_edit, "shim-act-fault", leg == 0 ? "(the landing's stack pointer is not the fault's)" : "(the landing's stack pointer did not advance by exactly 8)");
        msleep(20);
    }
    pass("shim-act-fault", " (the kernel's null fault delivered as SIGSEGV with the address, the registers and the pc; a redirect landed; an upward stack-pointer edit honoured by the trampoline)");

    /* delivery: the handler on the target's own stack, the records exact */
    g_act_mode = ACT_DELIVER;
    g_act_done = 0;
    g_w_spinning = 0;
    g_spin_release = 0;
    CHECK(spawn(&t1, act_spin_worker, NULL, ACT_WORKER_STACK) == 0, "shim-act-deliver", "(spawn refused)");
    CHECK(poll_flag(&g_w_spinning, 200), "shim-act-deliver", "(the worker never reached the window)");
    msleep(5);
    CHECK(pthread_kill(t1, g_actsig) == 0, "shim-act-deliver", "(the send was refused)");
    CHECK(poll_until(&g_act_count, 1, 100), "shim-act-deliver", "(the handler did not run)");
    CHECK(g_act_count == 1, "shim-act-deliver", "(the handler ran more than once)");
    CHECK(g_act_pid == getpid(), "shim-act-deliver", "(the activation named a foreign process)");
    CHECK(in_window(g_act_ctx_pc, spin_lo, spin_hi - spin_lo), "shim-act-deliver", "(the interrupted pc is outside the worker's window)");
    CHECK(g_act_signo == g_actsig, "shim-act-deliver", "(the signal number)");
    CHECK(g_act_code == RHU_GLIBC_SI_TKILL, "shim-act-deliver", "(the origin code is not a thread-directed send)");
    CHECK(g_act_uid == 0, "shim-act-deliver", "(the user id)");
    CHECK(g_act_tid == g_w_tid && g_w_tid == t1, "shim-act-deliver", "(the handler did not run on the target thread)");
    CHECK(in_window(g_act_local, g_w_lo, g_w_len), "shim-act-deliver", "(the handler's frame is not on the target's stack)");
    CHECK(in_window(g_act_ctx_sp, g_w_lo, g_w_len), "shim-act-deliver", "(the interrupted stack pointer is outside the target's stack)");
    CHECK(g_act_mask1 == 2u, "shim-act-deliver", "(the context does not carry the thread's mask)");
    CHECK(g_act_regspace0 == 0 && g_act_ucflags == 0 && g_act_sssp == 0, "shim-act-deliver", "(the context's empty fields are not empty)");
    g_spin_release = 1;
    wait_done(&g_act_done, 1);
    pass("shim-act-deliver", " (the handler ran on the target's own stack with the process id, the thread's own id, the origin code, and the interrupted registers inside the target's window; the recorded mask rides the context)");

    /* set-context: r0 and pc edited through the context land on the thread */
    g_act_mode = ACT_SETCTX;
    g_w_spinning = 0;
    g_spin_release = 0;
    g_pad_ran = 0;
    base_count = g_act_count;
    CHECK(spawn(&t1, act_spin_worker, NULL, ACT_WORKER_STACK) == 0, "shim-act-setctx", "(spawn refused)");
    CHECK(poll_flag(&g_w_spinning, 200), "shim-act-setctx", "(the worker never reached the window)");
    msleep(5);
    CHECK(pthread_kill(t1, g_actsig) == 0, "shim-act-setctx", "(the send was refused)");
    CHECK(poll_flag(&g_pad_ran, 100), "shim-act-setctx", "(the landing function never ran)");
    CHECK(g_act_count == base_count + 1, "shim-act-setctx", "(the handler count)");
    CHECK(g_pad_magic == ACT_MAGIC, "shim-act-setctx", "(r0 did not carry the planted magic)");
    CHECK(g_pad_tid == t1, "shim-act-setctx", "(the landing function ran on another thread)");
    msleep(20);
    pass("shim-act-setctx", " (a pc and r0 edit through the signal context landed on the interrupted thread; a stack-pointer edit was ignored and reported)");

    /* a handler that blocks: the interrupted flow waits, then resumes */
    g_act_mode = ACT_BLOCK;
    g_act_done = 0;
    g_block_inside = 0;
    g_block_release = 0;
    g_c_release = 0;
    g_c_progress = 0;
    base_count = g_act_count;
    CHECK(spawn(&t1, act_count_worker, NULL, ACT_WORKER_STACK) == 0, "shim-act-block", "(spawn refused)");
    CHECK(poll_until(&g_c_progress, 1, 200), "shim-act-block", "(the worker never counted)");
    CHECK(pthread_kill(t1, g_actsig) == 0, "shim-act-block", "(the send was refused)");
    CHECK(poll_flag(&g_block_inside, 100), "shim-act-block", "(the handler never entered its wait)");
    {
        uint32_t p1;
        uint32_t p2;
        msleep(20);
        p1 = g_c_progress;
        msleep(20);
        p2 = g_c_progress;
        CHECK(p1 == p2, "shim-act-block", "(the interrupted flow advanced while the handler was blocked)");
        __atomic_store_n(&g_block_release, 1, __ATOMIC_RELEASE);
        SystemNative_LowLevelFutex_WakeByAddressSingle(&g_block_release);
        CHECK(poll_until(&g_c_progress, p2 + 1, 100), "shim-act-block", "(the interrupted flow did not resume after the handler's release)");
    }
    CHECK(g_act_count == base_count + 1, "shim-act-block", "(the handler count)");
    g_c_release = 1;
    wait_done(&g_act_done, 1);
    pass("shim-act-block", " (the handler blocked on a futex inside the activation; the interrupted flow stood still and resumed after the release)");

    /* a blocked condition wait breaks after the handler and re-waits */
    g_act_mode = ACT_COUNT;
    g_act_done = 0;
    g_e_ready = 0;
    g_e_parking = 0;
    g_e_returns = 0;
    base_count = g_act_count;
    pthread_mutex_init(&g_e_m, NULL);
    pthread_cond_init(&g_e_cv, NULL);
    CHECK(spawn(&t1, act_wait_worker, NULL, ACT_WORKER_STACK) == 0, "shim-act-eintr", "(spawn refused)");
    CHECK(poll_flag(&g_e_parking, 200), "shim-act-eintr", "(the worker never parked)");
    msleep(20);
    CHECK(g_e_returns == 0, "shim-act-eintr", "(the wait returned before any send)");
    CHECK(pthread_kill(t1, g_actsig) == 0, "shim-act-eintr", "(the send was refused)");
    CHECK(poll_until(&g_e_returns, 1, 100), "shim-act-eintr", "(the blocked wait did not return after the send)");
    CHECK(g_act_count == base_count + 1 && g_act_tid == t1, "shim-act-eintr", "(the handler did not run on the waiter)");
    msleep(20);
    CHECK(g_e_returns == 1 && g_act_done == 0, "shim-act-eintr", "(the loop did not re-wait on the false predicate)");
    pthread_mutex_lock(&g_e_m);
    g_e_ready = 1;
    pthread_cond_signal(&g_e_cv);
    pthread_mutex_unlock(&g_e_m);
    wait_done(&g_act_done, 1);
    pass("shim-act-eintr", " (a blocked condition wait returned after the handler ran on the waiter, re-waited on the false predicate, and a real signal released it)");

    /* the PAL's signal rows */
    CHECK(SystemNative_InitializeTerminalAndSignalHandling() == 1 && SystemNative_InitializeTerminalAndSignalHandling() == 1, "shim-act-pal", "(initialization did not report success)");
    CHECK(SystemNative_GetPlatformSignalNumber(RHU_PosixSignalSIGINT) == RHU_GSIG_SIGINT, "shim-act-pal", "(SIGINT)");
    CHECK(SystemNative_GetPlatformSignalNumber(RHU_PosixSignalSIGTERM) == RHU_GSIG_SIGTERM, "shim-act-pal", "(SIGTERM)");
    CHECK(SystemNative_GetPlatformSignalNumber(RHU_PosixSignalSIGCHLD) == RHU_GSIG_SIGCHLD, "shim-act-pal", "(SIGCHLD)");
    CHECK(SystemNative_GetPlatformSignalNumber(RHU_PosixSignalSIGWINCH) == RHU_GSIG_SIGWINCH, "shim-act-pal", "(SIGWINCH)");
    CHECK(SystemNative_GetPlatformSignalNumber(RHU_PosixSignalSIGKILL) == RHU_GSIG_SIGKILL, "shim-act-pal", "(SIGKILL)");
    CHECK(SystemNative_GetPlatformSignalNumber(40) == 40 && SystemNative_GetPlatformSignalNumber(0) == 0 && SystemNative_GetPlatformSignalNumber(99) == 0, "shim-act-pal", "(a raw code)");
    CHECK(SystemNative_GetPlatformSIGSTOP() == RHU_GSIG_SIGSTOP, "shim-act-pal", "(SIGSTOP)");
    SystemNative_SetPosixSignalHandler(posix_signal_cb);
    CHECK(SystemNative_EnablePosixSignalHandling(RHU_GSIG_SIGINT) == 1 && SystemNative_EnablePosixSignalHandling(0) == 0, "shim-act-pal", "(enable)");
    SystemNative_DisablePosixSignalHandling(RHU_GSIG_SIGINT);
    SystemNative_HandleNonCanceledPosixSignal(RHU_GSIG_SIGCONT);
    SystemNative_HandleNonCanceledPosixSignal(RHU_GSIG_SIGWINCH);
    SystemNative_SetTerminalInvalidationHandler(void_cb);
    SystemNative_RegisterForSigChld(sigchld_cb);
    SystemNative_SetDelayedSigChildConsoleConfigurationHandler(void_cb);
    pass("shim-act-pal", " (initialization reports success; the posix signal numbers map; enable and disable record; the continue and window signals take no action)");

    out("SHIMTEST-ACTIVATION: ALL GOOD (the activation adapter on the kernel's delivery; the signal rows recorded, none deliverable)");
    return 0;
}

#endif /* RHU_SHIMTEST_LADDER_ACTIVATION */

#ifdef RHU_SHIMTEST_LADDER_TLS

/* The image's own thread-local template, built here in assembly so this
 * ladder owns both halves of it and depends on no compiler choice of
 * access model: one initialized word in .tdata and one zero-fill word in
 * .tbss, each 4-aligned. The payload map keeps .tdata input ahead of
 * .tbss input inside one 8-aligned output section, so the two words are
 * at template offsets 0 and 4 and the bracketed span is 8 bytes. The
 * assembler marks a section named .tdata or .tbss thread-local whatever
 * flags are written, which is the whole reason the template needs an
 * output section of its own. */
__asm__(".section .tdata,\"awT\",%progbits\n\t"
        ".p2align 2\n\t"
        ".word 0x1234\n\t"
        ".section .tbss,\"awT\",%nobits\n\t"
        ".p2align 2\n\t"
        ".space 4\n\t"
        ".text");

#define TLS_TDATA_WORD 0x1234u
#define TLS_WORKER_STACK 2048u

/* The map's brackets around the template. */
extern char __rhu_tdata_lo[];
extern char __rhu_tdata_hi[];

/* An address the compiler may not reason about. Two distinct extern
 * objects are exactly the shape whose difference the distinct-objects
 * axiom is allowed to fold, so both bracket reads pass through here
 * before any arithmetic. */
static uintptr_t opaque_addr(const void *p)
{
    uintptr_t v = (uintptr_t)p;
    __asm__("" : "+r"(v));
    return v;
}

/* A global-dynamic access as the compiler would emit it: the module the
 * symbol lives in, and its offset inside that module's template. Module 1
 * is the image's own, which the shim maps onto the kernel's module 0. */
typedef struct {
    unsigned long ti_module;
    unsigned long ti_offset;
} shimtest_tls_index;

void *__tls_get_addr(shimtest_tls_index *ti);

static shimtest_tls_index g_ti_data = {1ul, 0ul};
static shimtest_tls_index g_ti_bss = {1ul, 4ul};

static volatile uint32_t *tls_data_word(void)
{
    return (volatile uint32_t *)__tls_get_addr(&g_ti_data);
}

static volatile uint32_t *tls_bss_word(void)
{
    return (volatile uint32_t *)__tls_get_addr(&g_ti_bss);
}

/* What one worker saw and where its block was. Workers never print. */
struct tls_worker {
    uint32_t saw_data;      /* the initialized word as the worker found it */
    uint32_t saw_bss;       /* the zero-fill word as the worker found it */
    uint32_t read_data;     /* its own store, read back after the other ran */
    uint32_t read_bss;
    uintptr_t base;
    uint32_t mark;          /* what this worker stores */
};

static struct tls_worker g_w[2];
static volatile int g_written;
static volatile int g_tls_done;

static void *tls_worker(void *arg)
{
    struct tls_worker *w = (struct tls_worker *)arg;

    w->base = opaque_addr((const void *)tls_data_word());
    w->saw_data = *tls_data_word();
    w->saw_bss = *tls_bss_word();

    *tls_data_word() = w->mark;
    *tls_bss_word() = w->mark + 1u;
    __atomic_add_fetch(&g_written, 1, __ATOMIC_ACQ_REL);

    /* Both workers write before either reads back, so a shared block
     * shows up as the other worker's mark rather than as a race. */
    while (__atomic_load_n(&g_written, __ATOMIC_ACQUIRE) < 2)
        msleep(5);

    w->read_data = *tls_data_word();
    w->read_bss = *tls_bss_word();
    __atomic_add_fetch(&g_tls_done, 1, __ATOMIC_ACQ_REL);
    return NULL;
}

static int tls_ladder(void)
{
    uint32_t tid0 = 0;
    uint32_t tid1 = 0;
    uintptr_t host_base;

    out("shimtest: the image's thread-local template through the kernel's module 0");
    rhu_shim_init();

    {
        uintptr_t lo = opaque_addr(__rhu_tdata_lo);
        uintptr_t hi = opaque_addr(__rhu_tdata_hi);
        CHECK(hi - lo == 8u, "shim-tls-template", "(the bracketed span is not the ladder's two words)");
        host_base = opaque_addr(rhu_tls_get(0));
        CHECK(host_base != 0u, "shim-tls-template", "(no block for module 0)");
        CHECK((host_base & 7u) == 0u, "shim-tls-template", "(the block is not 8-aligned)");
        pass("shim-tls-template", " (the map brackets 8 bytes; the block is present and 8-aligned)");
    }

    CHECK(*tls_data_word() == TLS_TDATA_WORD, "shim-tls-initial", "(the initialized word did not arrive)");
    CHECK(*tls_bss_word() == 0u, "shim-tls-initial", "(the zero-fill word is not zero)");
    pass("shim-tls-initial", " (the initialized half arrives; the zero-fill half is zero)");

    *tls_data_word() = 0xC0DEu;
    *tls_bss_word() = 0xBEEFu;
    CHECK(*tls_data_word() == 0xC0DEu && *tls_bss_word() == 0xBEEFu, "shim-tls-write", "(a store did not stick)");
    CHECK(*(const volatile uint32_t *)(const void *)__rhu_tdata_lo == TLS_TDATA_WORD, "shim-tls-write", "(the store reached the template, not the copy)");
    pass("shim-tls-write", " (both words take a store; the template itself is untouched)");

    g_w[0].mark = 0xA1A1u;
    g_w[1].mark = 0xB2B2u;
    CHECK(spawn(&tid0, tls_worker, &g_w[0], TLS_WORKER_STACK) == 0, "shim-tls-fresh", "(spawn)");
    CHECK(spawn(&tid1, tls_worker, &g_w[1], TLS_WORKER_STACK) == 0, "shim-tls-fresh", "(spawn)");
    wait_done(&g_tls_done, 2);

    CHECK(g_w[0].saw_data == TLS_TDATA_WORD && g_w[1].saw_data == TLS_TDATA_WORD, "shim-tls-fresh", "(a worker did not start from the template)");
    CHECK(g_w[0].saw_bss == 0u && g_w[1].saw_bss == 0u, "shim-tls-fresh", "(a worker's zero-fill word was not zero)");
    CHECK(g_w[0].base != host_base && g_w[1].base != host_base, "shim-tls-fresh", "(a worker shares the hosting thread's block)");
    pass("shim-tls-fresh", " (each worker starts from a fresh copy of the template, not the hosting thread's)");

    CHECK(g_w[0].base != g_w[1].base, "shim-tls-isolation", "(the two workers share one block)");
    CHECK(g_w[0].read_data == g_w[0].mark && g_w[0].read_bss == g_w[0].mark + 1u, "shim-tls-isolation", "(the first worker did not read back its own pair)");
    CHECK(g_w[1].read_data == g_w[1].mark && g_w[1].read_bss == g_w[1].mark + 1u, "shim-tls-isolation", "(the second worker did not read back its own pair)");
    CHECK(*tls_data_word() == 0xC0DEu && *tls_bss_word() == 0xBEEFu, "shim-tls-isolation", "(a worker reached the hosting thread's copy)");
    pass("shim-tls-isolation", " (two blocks at once, each thread reads back only its own pair)");

    {
        uintptr_t errno_at = opaque_addr((const void *)__errno_location());
        CHECK(errno_at < host_base || errno_at >= host_base + 8u, "shim-tls-block", "(the shim's per-thread block overlaps the template's)");
        pass("shim-tls-block", " (the shim's own per-thread block does not alias the image's template)");
    }

    out("SHIMTEST-TLS: ALL GOOD (the image's thread-local template on module 0; a fresh copy per thread, isolated)");
    return 0;
}

#endif /* RHU_SHIMTEST_LADDER_TLS */

#ifdef RHU_SHIMTEST_LADDER_HOST

/* The host contract's counterparties, standing in for the runtime so the
 * ORDER can be proven without the runtime present, and the class-library
 * exports standing in for the managed program's, so the table the host
 * installs can be read from the callee's side by the names the runtime's
 * bootstrapper installs. Each stand-in records that it was called and
 * what it was given; the managed entry then reports the record, so the
 * transcript proves the sequence rather than just its outcome. The
 * stand-ins carry the prototypes the generated header declares and read
 * the table by the slot names and export names it declares: a stand-in
 * that disagreed with the pin would prove the wrong contract. */
#include "rhu_rt_entry_net11_rc2.h"

_Static_assert(RHU_RT_CLASSLIB_SLOTS > RHU_RT_CLASSLIB_SLOT_ResolveDispatch, "the table holds the dispatch slot");
_Static_assert(RHU_RT_CLASSLIB_EXPORTS == 8, "eight slots carry an export at this pin");

/* The module handle the host passes is the image's load address: the
 * vector table's address (the header's load-address word repeats it; the
 * host reads the symbol, not the word). */
extern const uint32_t __vector_table[];

static int g_step;
static int g_rt_init_at;
static int g_rt_register_at;
static int g_modules_at;
static int g_managed_at;
static void *g_registered_module;
static void *g_modules_module;
static int g_modules_count;
static uint32_t g_managed_bytes;
static uint32_t g_unbox_bytes;
static uint32_t g_classlib_slots;
static void *g_classlib[RHU_RT_CLASSLIB_SLOTS];

/* The class-library exports, by the names the bootstrapper installs. */
void GetRuntimeException(void)
{
}

void RuntimeFailFast(void)
{
}

size_t ThreadEntryPoint(void *pContext)
{
    (void)pContext;
    return 0;
}

void AppendExceptionStackFrame(void)
{
}

void ResolveDispatch(void)
{
}

void GetSystemArrayEEType(void)
{
}

void OnFirstChanceException(void)
{
}

void OnUnhandledException(void)
{
}

bool RhInitialize(bool isDll)
{
    (void)isDll;
    g_rt_init_at = ++g_step;
    return true;
}

bool RhRegisterOSModule(void *pModule,
                        void *pvManagedCodeStartRange, uint32_t cbManagedCodeRange,
                        void *pvUnboxingStubsStartRange, uint32_t cbUnboxingStubsRange,
                        void **pClasslibFunctions, uint32_t nClasslibFunctions)
{
    uint32_t i;
    (void)pvManagedCodeStartRange;
    (void)pvUnboxingStubsStartRange;
    g_rt_register_at = ++g_step;
    g_registered_module = pModule;
    g_managed_bytes = cbManagedCodeRange;
    g_unbox_bytes = cbUnboxingStubsRange;
    g_classlib_slots = nClasslibFunctions;
    for (i = 0; i < nClasslibFunctions && i < (uint32_t)RHU_RT_CLASSLIB_SLOTS; i++)
        g_classlib[i] = pClasslibFunctions != NULL ? pClasslibFunctions[i] : NULL;
    return true;
}

static void host_record_modules(void *os_module, int count)
{
    g_modules_at = ++g_step;
    g_modules_module = os_module;
    g_modules_count = count;
}

/* The managed program. It runs last, so every record above is complete by
 * the time it checks them. */
static int32_t host_checks(int argc)
{
    int i;

    out("shimtest: the host's initialization contract, in order");
    g_managed_at = ++g_step;

    CHECK(g_rt_init_at == 1, "shim-host-order", "(the runtime was not initialized first)");
    CHECK(g_rt_register_at == 2, "shim-host-order", "(the module was not registered after initialization)");
    CHECK(g_modules_at == 3, "shim-host-order", "(the module list was not initialized after the registration)");
    CHECK(g_managed_at == 4, "shim-host-order", "(managed entry did not come last)");
    pass("shim-host-order", " (initialize, then register, then the module list, then managed entry)");

    CHECK(g_classlib_slots == RHU_RT_CLASSLIB_SLOTS, "shim-host-classlib", "(the table is not twelve slots)");
    CHECK(g_classlib[RHU_RT_CLASSLIB_SLOT_GetRuntimeException] == (void *)(uintptr_t)&GetRuntimeException, "shim-host-classlib", "(slot 0 is not the runtime-exception export)");
    CHECK(g_classlib[RHU_RT_CLASSLIB_SLOT_FailFast] == (void *)(uintptr_t)&RuntimeFailFast, "shim-host-classlib", "(slot 1 is not the fail-fast export)");
    CHECK(g_classlib[RHU_RT_CLASSLIB_SLOT_ThreadEntryPoint] == (void *)(uintptr_t)&ThreadEntryPoint, "shim-host-classlib", "(slot 2 is not the thread entry point)");
    CHECK(g_classlib[RHU_RT_CLASSLIB_SLOT_AppendExceptionStackFrame] == (void *)(uintptr_t)&AppendExceptionStackFrame, "shim-host-classlib", "(slot 3 is not the stack-frame export)");
    CHECK(g_classlib[RHU_RT_CLASSLIB_SLOT_ResolveDispatch] == (void *)(uintptr_t)&ResolveDispatch, "shim-host-classlib", "(slot 4 is not the dispatch resolver)");
    CHECK(g_classlib[RHU_RT_CLASSLIB_SLOT_GetSystemArrayEEType] == (void *)(uintptr_t)&GetSystemArrayEEType, "shim-host-classlib", "(slot 5 is not the array-type export)");
    CHECK(g_classlib[RHU_RT_CLASSLIB_SLOT_OnFirstChanceException] == (void *)(uintptr_t)&OnFirstChanceException, "shim-host-classlib", "(slot 6 is not the first-chance export)");
    CHECK(g_classlib[RHU_RT_CLASSLIB_SLOT_OnUnhandledException] == (void *)(uintptr_t)&OnUnhandledException, "shim-host-classlib", "(slot 7 is not the unhandled-exception export)");
    for (i = RHU_RT_CLASSLIB_EXPORTS; i < RHU_RT_CLASSLIB_SLOTS; i++)
        CHECK(g_classlib[i] == NULL, "shim-host-classlib", "(a slot past the exports is not null)");
    pass("shim-host-classlib", " (twelve slots; eight exports by the bootstrapper's names, four null)");

    CHECK(g_registered_module == (void *)(uintptr_t)__vector_table, "shim-host-module", "(the module handle is not the image's load address, the vector table's)");
    CHECK(g_modules_module == g_registered_module, "shim-host-module", "(the module list was initialized under another handle)");
    CHECK(g_managed_bytes == 0u && g_unbox_bytes == 0u, "shim-host-module", "(this tenant carries no managed code, yet a range was registered)");
    CHECK(g_modules_count == 0, "shim-host-module", "(this tenant carries no module table, yet modules were counted)");
    CHECK(argc == 1, "shim-host-module", "(the argument vector does not carry the one program name)");
    pass("shim-host-module", " (the handle is the image's load address; empty ranges; no module; one argument)");

    out("SHIMTEST-HOST: ALL GOOD (the initialization contract in order; the class-library table as the pin declares it)");
    return 0;
}

#endif /* RHU_SHIMTEST_LADDER_HOST */

#ifdef RHU_SHIMTEST_LADDER_BUSDEV

/* --- the bus device nodes, from a native caller's side: the two device
 * files through the C library's open, ioctl and close, on the emulator
 * row's bus models (a sensor at 0x76 on the I2C bus; a memory behind the
 * SPI bus's first select, whose identity request answers EF 40 18). The
 * binding's numbers and record layouts, as the unit states them. --- */
#define BD_O_RDWR 2
#define BD_I2C_SLAVE 0x0703ul
#define BD_I2C_FUNCS 0x0705ul
#define BD_I2C_RDWR 0x0707ul
#define BD_I2C_RATE 0x0740ul
#define BD_SPI_WR_MODE 0x40016b01ul
#define BD_SPI_RD_MODE 0x80016b01ul
#define BD_SPI_WR_BITS 0x40016b03ul
#define BD_SPI_WR_SPEED 0x40046b04ul
#define BD_SPI_RD_SPEED 0x80046b04ul
#define BD_SPI_MESSAGE(n) (0x40006b00ul | (((unsigned long)(n) * 32ul) << 16))

struct bd_i2c_msg {
    uint16_t addr;
    uint16_t flags;
    uint16_t len;
    uint8_t *buf;
};
struct bd_i2c_rdwr {
    struct bd_i2c_msg *msgs;
    uint32_t nmsgs;
};
struct bd_spi {
    uint32_t tx_lo, tx_hi, rx_lo, rx_hi;
    uint32_t len;
    uint32_t speed_hz;
    uint16_t delay_usecs;
    uint8_t bits_per_word, cs_change, tx_nbits, rx_nbits;
    uint16_t pad;
};

#define BD_ERRNO_IS(e) (*__errno_location() == (e))

static char g_bd_name[40];

/* "/dev/i2c-<bus>" or "/dev/spidev<bus>.<select>", from the row's own bus
 * numbers. */
static const char *bd_name(const char *stem, uint32_t bus, int with_select, uint32_t select)
{
    char *p = g_bd_name;
    while (*stem != 0)
        *p++ = *stem++;
    for (int part = 0; part < (with_select ? 2 : 1); part++) {
        uint32_t v = part == 0 ? bus : select;
        char d[10];
        int n = 0;
        if (part == 1)
            *p++ = '.';
        do {
            d[n++] = (char)('0' + v % 10u);
            v /= 10u;
        } while (v != 0);
        while (n > 0)
            *p++ = d[--n];
    }
    *p = 0;
    return g_bd_name;
}

/* The request running at a close: one long transfer on a descriptor. */
static uint8_t g_bd_long_tx[65535];
static volatile int g_bd_fd;
static volatile int g_bd_in;
static volatile int g_bd_rc;
static volatile int g_bd_done;

static void *bd_long_transfer(void *arg)
{
    (void)arg;
    struct bd_spi t;
    memset(&t, 0, sizeof t);
    t.tx_lo = (uint32_t)(uintptr_t)g_bd_long_tx;
    t.len = sizeof g_bd_long_tx;
    /* The block's slowest rate over the longest record: 8.4 s of the
     * kernel's clock on the wire, so the request is still running at the
     * ladder's close by seconds. */
    t.speed_hz = 62500u;
    __atomic_store_n(&g_bd_in, 1, __ATOMIC_RELEASE);
    g_bd_rc = ioctl(g_bd_fd, BD_SPI_MESSAGE(1), &t);
    __atomic_store_n(&g_bd_done, 1, __ATOMIC_RELEASE);
    return NULL;
}

static int busdev_ladder(void)
{
    out("shimtest-busdev: the bus device nodes from a native caller's side, on the row's bus models");
    CHECK(rhu_shim_init() == 0, "shim-busdev-init", "(initialization refused)");
    int32_t i2c_bus = rhu_i2c_bus(0);
    int32_t spi_bus = rhu_spi_bus(0);
    CHECK(i2c_bus >= 0 && spi_bus >= 0, "shim-busdev-init", "(the row states no I2C or no SPI bus)");
    pass("shim-busdev-init", " (the row's buses by their own numbers)");

    /* The names. */
    CHECK(open("/dev/i2c-", BD_O_RDWR) == -1 && BD_ERRNO_IS(RHU_GERR_ENOENT), "shim-busdev-names", "(a name with no number opened)");
    CHECK(open("/dev/i2c-01", BD_O_RDWR) == -1 && BD_ERRNO_IS(RHU_GERR_ENOENT), "shim-busdev-names", "(a number with a leading zero opened)");
    CHECK(open("/dev/spidev2", BD_O_RDWR) == -1 && BD_ERRNO_IS(RHU_GERR_ENOENT), "shim-busdev-names", "(an SPI name with no select opened)");
    CHECK(open(bd_name("/dev/spidev", (uint32_t)spi_bus, 1, 4294967295u), BD_O_RDWR) == -1 && BD_ERRNO_IS(RHU_GERR_ENOENT), "shim-busdev-names",
          "(the no-select value opened as a select's name)");
    CHECK(open("/dev/spidev2.4294967296", BD_O_RDWR) == -1 && BD_ERRNO_IS(RHU_GERR_ENOENT), "shim-busdev-names", "(a select past 32 bits opened)");
    CHECK(open("/dev/i2c-9", BD_O_RDWR) == -1 && BD_ERRNO_IS(RHU_GERR_ENOENT), "shim-busdev-names", "(a bus the row lacks opened)");
    pass("shim-busdev-names", " (malformed names, the no-select value as a select's name and a bus the row lacks refused ENOENT)");

    /* The I2C node. */
    int fi = open(bd_name("/dev/i2c-", (uint32_t)i2c_bus, 0, 0), BD_O_RDWR);
    CHECK(fi >= RHU_SHIM_FD_DEVICE_LO, "shim-busdev-funcs", "(the I2C node did not open in the device band)");
    {
        uint32_t word[2] = { 0xAAAAAAAAu, 0xC0FFEE00u };
        CHECK(ioctl(fi, BD_I2C_FUNCS, &word[0]) == 0 && word[0] == 1u, "shim-busdev-funcs", "(the functionality word is not plain combined transactions)");
        CHECK(word[1] == 0xC0FFEE00u, "shim-busdev-funcs", "(the functionality request stored past its four-byte output)");
        CHECK(ioctl(fi, BD_I2C_FUNCS, NULL) == -1 && BD_ERRNO_IS(RHU_GERR_EFAULT), "shim-busdev-funcs", "(a NULL output was not refused EFAULT)");
        pass("shim-busdev-funcs", " (one four-byte word, the word after it untouched; NULL refused EFAULT)");
    }
    /* The one RHU request on the I2C node: the bus's rate for the program's
     * run (the rate request). This row's emulated block serves 100 kHz alone (the
     * generic timing arm's one pair), so the served rate is the default, and
     * the refusal of a second served rate (EBUSY with the running rate read
     * back) can be tested only on a board whose block serves two rates. The
     * request on a fresh
     * node of the same bus: the claim is the bus's, not the node's, and the
     * transaction then runs through the first node. */
    {
        int fr = open(bd_name("/dev/i2c-", (uint32_t)i2c_bus, 0, 0), BD_O_RDWR);
        CHECK(fr >= RHU_SHIM_FD_DEVICE_LO && fr != fi, "shim-busdev-rate", "(a second I2C node of the bus did not open apart)");
        uint32_t hz = 0;
        CHECK(ioctl(fr, BD_I2C_RATE, NULL) == -1 && BD_ERRNO_IS(RHU_GERR_EFAULT), "shim-busdev-rate", "(a NULL word was not refused EFAULT)");
        CHECK(ioctl(fr, BD_I2C_RATE, &hz) == -1 && BD_ERRNO_IS(RHU_GERR_EINVAL), "shim-busdev-rate", "(a rate of 0 Hz was not refused EINVAL)");
        int ft = open(bd_name("/dev/spidev", (uint32_t)spi_bus, 1, 0), BD_O_RDWR);
        hz = 100000u;
        CHECK(ft >= RHU_SHIM_FD_DEVICE_LO && ioctl(ft, BD_I2C_RATE, &hz) == -1 && BD_ERRNO_IS(RHU_GERR_ENOTTY) && close(ft) == 0, "shim-busdev-rate", "(the request on the SPI node was not refused ENOTTY)");
        CHECK(ioctl(fr, BD_I2C_RATE, &hz) == 0 && hz == 100000u, "shim-busdev-rate", "(the served rate was not claimed with the rate read back)");
        uint8_t reg = 0xD0;
        uint8_t id = 0;
        struct bd_i2c_msg two[2] = { { 0x76, 0, 1, &reg }, { 0x76, 1, 1, &id } };
        struct bd_i2c_rdwr set = { two, 2 };
        CHECK(ioctl(fi, BD_I2C_RDWR, &set) == 2 && id == 0x60, "shim-busdev-rate", "(a transaction on the claimed bus did not read the sensor's identity)");
        hz = 100000u;
        CHECK(ioctl(fr, BD_I2C_RATE, &hz) == 0 && hz == 100000u, "shim-busdev-rate", "(the same rate again did not answer 0)");
        hz = 400000u;
        CHECK(ioctl(fr, BD_I2C_RATE, &hz) == -1 && BD_ERRNO_IS(RHU_GERR_ENOTSUP) && hz == 400000u, "shim-busdev-rate", "(an unserved rate was not refused ENOTSUP with the word untouched)");
        hz = 250000u;
        CHECK(ioctl(fr, BD_I2C_RATE, &hz) == -1 && BD_ERRNO_IS(RHU_GERR_ENOTSUP) && hz == 250000u, "shim-busdev-rate", "(a rate outside the set was not refused ENOTSUP with the word untouched)");
        CHECK(close(fr) == 0, "shim-busdev-rate", "(the second node's close refused)");
        pass("shim-busdev-rate", " (the one RHU request on the I2C node: a served rate claims the bus for the run and reads back, the same rate again 0, an unserved rate ENOTSUP before the state with the word untouched, NULL EFAULT, 0 EINVAL, the SPI node ENOTTY; this row serves one rate, so a second served rate's EBUSY with the running rate read back is the bench's, by the kernel's contract)");
    }
    {
        uint8_t reg = 0xD0;
        uint8_t id = 0;
        struct bd_i2c_msg two[2] = { { 0x76, 0, 1, &reg }, { 0x76, 1, 1, &id } };
        struct bd_i2c_rdwr set = { two, 2 };
        CHECK(ioctl(fi, BD_I2C_RDWR, &set) == 2 && id == 0x60, "shim-busdev-i2c", "(the sensor's identity did not read 0x60 through one combined transaction)");
        /* The same header at an odd address: records are copied, so none
         * need be aligned. */
        uint8_t odd[1 + sizeof set];
        memcpy(&odd[1], &set, sizeof set);
        id = 0;
        CHECK(ioctl(fi, BD_I2C_RDWR, &odd[1]) == 2 && id == 0x60, "shim-busdev-i2c", "(a transaction header at an odd address was not served)");
        set.nmsgs = 0;
        CHECK(ioctl(fi, BD_I2C_RDWR, &set) == -1 && BD_ERRNO_IS(RHU_GERR_EINVAL), "shim-busdev-i2c", "(a transaction of no messages was not refused EINVAL)");
        set.nmsgs = 9;
        CHECK(ioctl(fi, BD_I2C_RDWR, &set) == -1 && BD_ERRNO_IS(RHU_GERR_EINVAL), "shim-busdev-i2c", "(nine messages were not refused EINVAL)");
        set.nmsgs = 2;
        set.msgs = NULL;
        CHECK(ioctl(fi, BD_I2C_RDWR, &set) == -1 && BD_ERRNO_IS(RHU_GERR_EINVAL), "shim-busdev-i2c", "(a NULL message list was not refused EINVAL)");
        set.msgs = two;
        two[1].buf = NULL;
        CHECK(ioctl(fi, BD_I2C_RDWR, &set) == -1 && BD_ERRNO_IS(RHU_GERR_EFAULT), "shim-busdev-i2c", "(a NULL buffer with a length was not refused EFAULT)");
        two[1].buf = &id;
        two[1].flags = 0x0011;
        CHECK(ioctl(fi, BD_I2C_RDWR, &set) == -1 && BD_ERRNO_IS(RHU_GERR_EINVAL), "shim-busdev-i2c", "(a flag other than the read flag was not refused EINVAL)");
        two[1].flags = 1;
        two[1].len = 256;
        CHECK(ioctl(fi, BD_I2C_RDWR, &set) == -1 && BD_ERRNO_IS(RHU_GERR_EINVAL), "shim-busdev-i2c", "(a message of 256 bytes was not refused EINVAL)");
        CHECK(ioctl(fi, BD_I2C_SLAVE, NULL) == -1 && BD_ERRNO_IS(RHU_GERR_ENOTTY), "shim-busdev-i2c", "(a request not served did not answer ENOTTY with a zero argument)");
        CHECK(ioctl(fi, BD_I2C_RDWR, NULL) == -1 && BD_ERRNO_IS(RHU_GERR_EFAULT), "shim-busdev-i2c", "(a NULL transaction was not refused EFAULT)");
        pass("shim-busdev-i2c", " (the identity through one combined transaction, a header at an odd address served, the limits and the nested NULLs by their errnos, a request not served ENOTTY whatever its argument)");
    }
    CHECK(close(fi) == 0 && close(fi) == -1 && BD_ERRNO_IS(RHU_GERR_EBADF), "shim-busdev-i2c", "(the close, or a second close, answered wrong)");

    /* The SPI node's settings. */
    int fs = open(bd_name("/dev/spidev", (uint32_t)spi_bus, 1, 0), BD_O_RDWR);
    CHECK(fs >= RHU_SHIM_FD_DEVICE_LO, "shim-busdev-spi-settings", "(the SPI node did not open in the device band)");
    {
        uint8_t m = 0x43, back = 0, bits = 8;
        uint32_t hz = 1000000u, hz_back = 0, zero = 0;
        CHECK(ioctl(fs, BD_SPI_WR_MODE, &m) == 0 && ioctl(fs, BD_SPI_RD_MODE, &back) == 0 && back == 0x43, "shim-busdev-spi-settings",
              "(mode 3 with the no-select bit did not read back as one byte)");
        m = 0x04;
        CHECK(ioctl(fs, BD_SPI_WR_MODE, &m) == -1 && BD_ERRNO_IS(RHU_GERR_EINVAL), "shim-busdev-spi-settings", "(an active-high select was not refused EINVAL)");
        m = 0x08;
        CHECK(ioctl(fs, BD_SPI_WR_MODE, &m) == -1 && BD_ERRNO_IS(RHU_GERR_EINVAL), "shim-busdev-spi-settings", "(least-significant-bit-first was not refused EINVAL)");
        CHECK(ioctl(fs, BD_SPI_RD_MODE, &back) == 0 && back == 0x43, "shim-busdev-spi-settings", "(a refused mode changed the node's mode)");
        CHECK(ioctl(fs, BD_SPI_WR_BITS, &bits) == 0, "shim-busdev-spi-settings", "(eight-bit words refused)");
        bits = 16;
        CHECK(ioctl(fs, BD_SPI_WR_BITS, &bits) == -1 && BD_ERRNO_IS(RHU_GERR_EINVAL), "shim-busdev-spi-settings", "(sixteen-bit words were not refused EINVAL)");
        CHECK(ioctl(fs, BD_SPI_WR_SPEED, &zero) == -1 && BD_ERRNO_IS(RHU_GERR_EINVAL), "shim-busdev-spi-settings", "(a rate of 0 Hz was not refused EINVAL)");
        CHECK(ioctl(fs, BD_SPI_WR_SPEED, &hz) == 0 && ioctl(fs, BD_SPI_RD_SPEED, &hz_back) == 0 && hz_back == 1000000u, "shim-busdev-spi-settings",
              "(the rate did not read back)");
        CHECK(ioctl(fs, BD_SPI_WR_MODE, NULL) == -1 && BD_ERRNO_IS(RHU_GERR_EFAULT), "shim-busdev-spi-settings", "(a NULL setting was not refused EFAULT)");
        pass("shim-busdev-spi-settings", " (the mode and the no-select bit one byte, the refused bits and sizes EINVAL with the node unchanged, the rate read back, NULL refused EFAULT)");
    }

    /* The transfers. */
    {
        static const uint8_t ask[4] = { 0x9F, 0x00, 0x00, 0x00 };
        uint8_t got[4] = { 0, 0, 0, 0 };
        uint8_t m = 0x43;
        struct bd_spi t;
        memset(&t, 0, sizeof t);
        t.tx_lo = (uint32_t)(uintptr_t)ask;
        t.rx_lo = (uint32_t)(uintptr_t)got;
        t.len = 4;
        /* The node is at mode 3 with the no-select bit: no select asserted,
         * the memory answers ones. */
        CHECK(ioctl(fs, BD_SPI_MESSAGE(1), &t) == 4 && got[0] == 0xFF && got[1] == 0xFF && got[2] == 0xFF && got[3] == 0xFF, "shim-busdev-spi-transfer",
              "(a transfer under the no-select mode did not answer ones)");
        m = 0x00;
        CHECK(ioctl(fs, BD_SPI_WR_MODE, &m) == 0, "shim-busdev-spi-transfer", "(mode 0 refused)");
        CHECK(ioctl(fs, BD_SPI_MESSAGE(1), &t) == 4 && got[0] == 0xFF && got[1] == 0xEF && got[2] == 0x40 && got[3] == 0x18, "shim-busdev-spi-transfer",
              "(the memory's identity did not read FF EF 40 18 under select 0)");
        CHECK(ioctl(fs, BD_SPI_MESSAGE(0), NULL) == 0, "shim-busdev-spi-transfer", "(an empty message did not answer 0)");
        CHECK(ioctl(fs, BD_SPI_MESSAGE(2), &t) == -1 && BD_ERRNO_IS(RHU_GERR_EINVAL), "shim-busdev-spi-transfer", "(two records in one request were not refused EINVAL)");
        CHECK(ioctl(fs, BD_SPI_MESSAGE(1), NULL) == -1 && BD_ERRNO_IS(RHU_GERR_EFAULT), "shim-busdev-spi-transfer", "(a NULL record was not refused EFAULT)");
        t.tx_hi = 1;
        CHECK(ioctl(fs, BD_SPI_MESSAGE(1), &t) == -1 && BD_ERRNO_IS(RHU_GERR_EINVAL), "shim-busdev-spi-transfer", "(a buffer address above 32 bits was not refused EINVAL)");
        pass("shim-busdev-spi-transfer", " (ones with no select asserted, the memory's identity under select 0, an empty message 0, the refusals by their errnos)");
    }
    {
        /* A receive buffer over its own record's length word: the echoed
         * bytes land on the caller's record, and the answer is still the
         * count asked. */
        static const uint8_t echo[4] = { 0x00, 0x11, 0x22, 0x33 };
        struct bd_spi t;
        memset(&t, 0, sizeof t);
        t.tx_lo = (uint32_t)(uintptr_t)echo;
        t.rx_lo = (uint32_t)(uintptr_t)&t.len;
        t.len = 4;
        CHECK(ioctl(fs, BD_SPI_MESSAGE(1), &t) == 4, "shim-busdev-spi-overlay", "(a receive buffer over the record's length word changed the count returned)");
        CHECK(t.len == 0x332211FFu, "shim-busdev-spi-overlay", "(the echoed bytes did not land on the record: the case proved nothing)");
        pass("shim-busdev-spi-overlay", " (the record read once: the count asked, whatever the receive buffer overwrote)");
    }

    /* A close under a running request. */
    {
        uint32_t tid = 0;
        uint8_t m = 0;
        g_bd_fd = fs;
        CHECK(spawn_below(&tid, bd_long_transfer, NULL) == 0, "shim-busdev-close-under-request", "(the worker did not spawn)");
        /* The worker runs below this thread: it enters its request while
         * this thread sleeps (a thread below is never reached by a yield),
         * and the verdict on the reopen waits for the worker's end, so the
         * worker's line on the wire comes before the verdict's on every
         * run. */
        while (__atomic_load_n(&g_bd_in, __ATOMIC_ACQUIRE) == 0)
            msleep(1);
        msleep(20);
        CHECK(__atomic_load_n(&g_bd_done, __ATOMIC_ACQUIRE) == 0, "shim-busdev-close-under-request", "(the long transfer ended before the close: the case proved nothing)");
        CHECK(close(fs) == 0, "shim-busdev-close-under-request", "(the close under a running request did not answer 0)");
        CHECK(ioctl(fs, BD_SPI_RD_MODE, &m) == -1 && BD_ERRNO_IS(RHU_GERR_EBADF), "shim-busdev-close-under-request", "(a closed descriptor still served a request)");
        int f1 = open(bd_name("/dev/spidev", (uint32_t)spi_bus, 1, 1), BD_O_RDWR);
        int reopen_apart = f1 >= RHU_SHIM_FD_DEVICE_LO && f1 != fs;
        wait_done(&g_bd_done, 1);
        CHECK(reopen_apart, "shim-busdev-close-under-request", "(the reopen received the entry a request still ran on)");
        CHECK(g_bd_rc == (int)sizeof g_bd_long_tx, "shim-busdev-close-under-request", "(the request running at the close did not complete with its count)");
        int f2 = open(bd_name("/dev/spidev", (uint32_t)spi_bus, 1, 0), BD_O_RDWR);
        CHECK(f2 == fs, "shim-busdev-close-under-request", "(the entry was not given back when its last request ended)");
        CHECK(close(f1) == 0 && close(f2) == 0, "shim-busdev-close-under-request", "(the closes refused)");
        pass("shim-busdev-close-under-request", " (the close answers at once and the descriptor is closed; the request completes with its count; the entry goes to no one until then, and back after)");
    }

    out("SHIMTEST-BUSDEV: ALL GOOD (the bus device nodes from a native caller's side)");
    return 0;
}

#endif /* RHU_SHIMTEST_LADDER_BUSDEV */

#ifdef RHU_SHIMTEST_LADDER_PWMFS

/* --- the PWM device files, from a native caller's side: the small tree
 * under /sys the standard package's PWM class reads and writes, driven
 * through the runtime's portability verbs the class reaches (the store
 * hands a /sys path and every sysfs-band descriptor to the unit) and the
 * C library's access (the package's own check of a channel directory), on
 * the emulator row's two virtual timers: the lines 0 and 2 on timer 0, the
 * line 3 on timer 1, the line 1 a hole no timer drives. The kernel's PWM
 * witness is read by symbol after the verbs, in the kernel's layout. --- */
struct pf_witness {
    uint32_t lines, timers, claims_taken, releases, sets, enables, refused_busy, refused_inval;
    uint32_t last_refused_line;
    int32_t last_refused_code;
    uint32_t last_holder_line, reserved;
    uint64_t last_period_ns;
};
extern struct pf_witness __rhu_pwm_witness;
int access(const char *path, int mode);

#define PF_CHIP "/sys/class/pwm/pwmchip0"
#define PF_ERRNO_IS(e) (SystemNative_GetErrNo() == (e))

/* One value written to a node through the store, as the class writes one:
 * the count, or -1 with the write's errno kept past the close. */
static int32_t pf_write(const char *path, const char *text)
{
    intptr_t fd = SystemNative_Open(path, RHU_PAL_O_WRONLY, 0);
    if (fd < 0)
        return -1;
    int32_t n = SystemNative_Write(fd, text, (int32_t)strlen(text));
    int32_t e = *__errno_location();
    SystemNative_Close(fd);
    if (n < 0)
        *__errno_location() = e;
    return n;
}

/* A node's text read through the store: the count with the text
 * terminated, or -1 with the errno kept past the close. */
static int32_t pf_read(const char *path, char *buf, int32_t cap)
{
    intptr_t fd = SystemNative_Open(path, RHU_PAL_O_RDONLY, 0);
    if (fd < 0)
        return -1;
    int32_t n = SystemNative_Read(fd, buf, cap - 1);
    int32_t e = *__errno_location();
    SystemNative_Close(fd);
    if (n >= 0)
        buf[n] = 0;
    else
        *__errno_location() = e;
    return n;
}

static int pf_is(const char *path, int32_t type)
{
    rhu_pal_filestatus st;
    return SystemNative_Stat(path, &st) == 0 && (st.Mode & 0xF000) == type;
}

static int pf_reads(const char *path, const char *want)
{
    char t[32];
    int32_t n = pf_read(path, t, (int32_t)sizeof t);
    return n == (int32_t)strlen(want) && strcmp(t, want) == 0;
}

static int pwmfs_ladder(void)
{
    char t[32];
    rhu_pal_filestatus st;
    const volatile struct pf_witness *w = &__rhu_pwm_witness;
    out("shimtest-pwmfs: the PWM device files from a native caller's side, on the row's virtual timers");
    CHECK(rhu_shim_init() == 0, "shim-pwmfs-init", "(initialization refused)");
    CHECK(rhu_pwm_count() == 3 && w->lines == 3 && w->timers == 2, "shim-pwmfs-init", "(the row does not state three PWM lines over two timers)");
    pass("shim-pwmfs-init", " (three PWM lines over two virtual timers, by the kernel's enumeration and its witness)");

    /* The tree. */
    CHECK(pf_is("/sys", RHU_PAL_S_IFDIR) && pf_is("/sys/class", RHU_PAL_S_IFDIR) && pf_is("/sys/class/pwm", RHU_PAL_S_IFDIR) && pf_is(PF_CHIP, RHU_PAL_S_IFDIR) && pf_is(PF_CHIP "/", RHU_PAL_S_IFDIR),
          "shim-pwmfs-tree", "(a directory of the tree is missing)");
    CHECK(pf_is(PF_CHIP "/npwm", RHU_PAL_S_IFREG) && pf_is(PF_CHIP "/export", RHU_PAL_S_IFREG) && pf_is(PF_CHIP "/unexport", RHU_PAL_S_IFREG), "shim-pwmfs-tree", "(npwm, export or unexport is not a regular file)");
    CHECK(SystemNative_Stat("/sys/class/pwm/pwmchip1", &st) == -1 && PF_ERRNO_IS(RHU_GERR_ENOENT), "shim-pwmfs-tree", "(a second chip answered)");
    CHECK(SystemNative_Stat(PF_CHIP "/pwm0", &st) == -1 && PF_ERRNO_IS(RHU_GERR_ENOENT), "shim-pwmfs-tree", "(an unexported channel's directory answered)");
    CHECK(SystemNative_Open(PF_CHIP, RHU_PAL_O_RDONLY, 0) == -1 && PF_ERRNO_IS(RHU_GERR_EISDIR), "shim-pwmfs-tree", "(a directory opened as a file)");
    pass("shim-pwmfs-tree", " (/sys, /sys/class, /sys/class/pwm and pwmchip0 directories, with or without a trailing slash; the three chip files regular; pwmchip1 and an unexported channel ENOENT; a directory EISDIR)");

    /* npwm. */
    CHECK(pf_reads(PF_CHIP "/npwm", "4\n"), "shim-pwmfs-npwm", "(npwm did not read 4)");
    CHECK(pf_write(PF_CHIP "/npwm", "4") == -1 && PF_ERRNO_IS(RHU_GERR_EACCES), "shim-pwmfs-npwm", "(a write to npwm was not refused EACCES)");
    pass("shim-pwmfs-npwm", " (the highest PWM-capable line plus one, read-only)");

    /* export. */
    CHECK(pf_write(PF_CHIP "/export", "0\n") == 2 && w->claims_taken == 1, "shim-pwmfs-export", "(the export of line 0 did not claim it)");
    CHECK(pf_write(PF_CHIP "/export", "0") == -1 && PF_ERRNO_IS(RHU_GERR_EBUSY), "shim-pwmfs-export", "(a second export of line 0 was not refused EBUSY)");
    CHECK(pf_write(PF_CHIP "/export", "1") == -1 && PF_ERRNO_IS(RHU_GERR_ENODEV), "shim-pwmfs-export", "(the hole's export was not refused ENODEV)");
    CHECK(pf_write(PF_CHIP "/export", "2") == 1 && pf_write(PF_CHIP "/export", "3") == 1 && w->claims_taken == 3, "shim-pwmfs-export", "(lines 2 and 3 did not export)");
    CHECK(pf_write(PF_CHIP "/export", "4") == -1 && PF_ERRNO_IS(RHU_GERR_ENODEV), "shim-pwmfs-export", "(a line past npwm was not refused ENODEV)");
    CHECK(pf_write(PF_CHIP "/export", "x") == -1 && PF_ERRNO_IS(RHU_GERR_EINVAL), "shim-pwmfs-export", "(a non-number was not refused EINVAL)");
    CHECK(pf_is(PF_CHIP "/pwm0", RHU_PAL_S_IFDIR) && pf_is(PF_CHIP "/pwm0/period", RHU_PAL_S_IFREG) && pf_is(PF_CHIP "/pwm3/enable", RHU_PAL_S_IFREG), "shim-pwmfs-export", "(an exported channel's directory or a file of it is missing)");
    CHECK(pf_read(PF_CHIP "/export", t, (int32_t)sizeof t) == -1 && PF_ERRNO_IS(RHU_GERR_EACCES), "shim-pwmfs-export", "(a read of export was not refused EACCES)");
    pass("shim-pwmfs-export", " (the channel number is the line number: lines 0, 2 and 3 claimed, the second export EBUSY, the hole and a line past npwm ENODEV, a non-number EINVAL; export write-only)");

    /* period: the period the timer runs read back, the shared period. */
    CHECK(pf_reads(PF_CHIP "/pwm0/period", "0\n"), "shim-pwmfs-period", "(the period before any set did not read 0)");
    CHECK(pf_write(PF_CHIP "/pwm0/period", "0") == -1 && PF_ERRNO_IS(RHU_GERR_EINVAL), "shim-pwmfs-period", "(a period of 0 was not refused EINVAL)");
    CHECK(pf_write(PF_CHIP "/pwm0/duty_cycle", "1") == -1 && PF_ERRNO_IS(RHU_GERR_EINVAL), "shim-pwmfs-period", "(a duty before any period was not refused EINVAL)");
    CHECK(pf_write(PF_CHIP "/pwm3/enable", "1") == -1 && PF_ERRNO_IS(RHU_GERR_EINVAL), "shim-pwmfs-period", "(an enable before any period was not refused EINVAL)");
    CHECK(pf_write(PF_CHIP "/pwm0/period", "1000000") == 7 && w->sets == 1 && w->last_period_ns == 1000000ull, "shim-pwmfs-period", "(the period of 1 ms did not reach the timer)");
    CHECK(pf_reads(PF_CHIP "/pwm0/period", "1000000\n"), "shim-pwmfs-period", "(the period did not read back the timer's 1000000)");
    CHECK(pf_write(PF_CHIP "/pwm3/period", "333333333") == 9 && w->last_period_ns == 333336000ull, "shim-pwmfs-period", "(the 3 Hz period did not quantize to 333336000 ns on the timer)");
    CHECK(pf_reads(PF_CHIP "/pwm3/period", "333336000\n"), "shim-pwmfs-period", "(the period file did not read the period the timer runs: the quantized truth)");
    CHECK(pf_write(PF_CHIP "/pwm2/period", "500000") == -1 && PF_ERRNO_IS(RHU_GERR_EBUSY) && w->last_holder_line == 0, "shim-pwmfs-period", "(another period on the shared timer was not refused EBUSY with the holder named)");
    CHECK(pf_write(PF_CHIP "/pwm2/period", "1000000") == 7, "shim-pwmfs-period", "(the same period on the shared timer refused)");
    CHECK(pf_write(PF_CHIP "/pwm0/period", "x") == -1 && PF_ERRNO_IS(RHU_GERR_EINVAL), "shim-pwmfs-period", "(a non-number period was not refused EINVAL)");
    pass("shim-pwmfs-period", " (0 before any set; 0, a non-number, a duty or an enable before the period EINVAL; 1 ms exact; 3 Hz asked reads the timer's 333336000: the quantized truth; another period on the shared timer EBUSY with the holder named, the same period accepted)");

    /* duty_cycle. */
    CHECK(pf_write(PF_CHIP "/pwm0/duty_cycle", "250000") == 6 && pf_reads(PF_CHIP "/pwm0/duty_cycle", "250000\n"), "shim-pwmfs-duty", "(the duty of 250000 ns did not set and read back)");
    CHECK(pf_write(PF_CHIP "/pwm0/duty_cycle", "2000000") == -1 && PF_ERRNO_IS(RHU_GERR_EINVAL), "shim-pwmfs-duty", "(a duty past the period was not refused EINVAL)");
    CHECK(pf_write(PF_CHIP "/pwm0/duty_cycle", "x") == -1 && PF_ERRNO_IS(RHU_GERR_EINVAL) && pf_reads(PF_CHIP "/pwm0/duty_cycle", "250000\n"), "shim-pwmfs-duty", "(a non-number duty was not refused EINVAL with the duty kept)");
    pass("shim-pwmfs-duty", " (the duty asked read back; past the period or a non-number EINVAL with the duty kept)");

    /* polarity. */
    CHECK(pf_reads(PF_CHIP "/pwm0/polarity", "normal\n"), "shim-pwmfs-polarity", "(the polarity did not read normal)");
    CHECK(pf_write(PF_CHIP "/pwm0/polarity", "inversed\n") == 9 && pf_reads(PF_CHIP "/pwm0/polarity", "inversed\n"), "shim-pwmfs-polarity", "(inversed did not set and read back)");
    CHECK(pf_write(PF_CHIP "/pwm0/polarity", "x") == -1 && PF_ERRNO_IS(RHU_GERR_EINVAL) && pf_reads(PF_CHIP "/pwm0/polarity", "inversed\n"), "shim-pwmfs-polarity", "(a third word was not refused EINVAL with the polarity kept)");
    CHECK(pf_write(PF_CHIP "/pwm0/polarity", "normal") == 6 && pf_reads(PF_CHIP "/pwm0/polarity", "normal\n"), "shim-pwmfs-polarity", "(normal did not set again)");
    pass("shim-pwmfs-polarity", " (normal, inversed, normal again, each read back; a third word EINVAL)");

    /* enable. */
    CHECK(pf_write(PF_CHIP "/pwm0/enable", "1") == 1 && w->enables == 1 && pf_reads(PF_CHIP "/pwm0/enable", "1\n"), "shim-pwmfs-enable", "(enable 1 did not drive the output and read back)");
    CHECK(pf_write(PF_CHIP "/pwm0/enable", "2") == -1 && PF_ERRNO_IS(RHU_GERR_EINVAL), "shim-pwmfs-enable", "(a value past 1 was not refused EINVAL)");
    CHECK(pf_write(PF_CHIP "/pwm0/enable", "0\n") == 2 && pf_reads(PF_CHIP "/pwm0/enable", "0\n"), "shim-pwmfs-enable", "(enable 0 did not park the output and read back)");
    pass("shim-pwmfs-enable", " (1 drives, 0 parks, each read back; 2 EINVAL)");

    /* The descriptor verbs on a node. */
    {
        intptr_t fd = SystemNative_Open(PF_CHIP "/pwm0/period", RHU_PAL_O_RDWR, 0);
        CHECK(fd >= RHU_SHIM_FD_SYSFS_LO && fd < RHU_SHIM_FD_SYSFS_LO + RHU_SHIM_FD_SYSFS_COUNT, "shim-pwmfs-descriptors", "(the node did not open in the sysfs band)");
        CHECK(SystemNative_FStat(fd, &st) == 0 && (st.Mode & 0xF000) == RHU_PAL_S_IFREG && st.Size == 4096, "shim-pwmfs-descriptors", "(fstat is not a regular file of 4096 bytes)");
        CHECK(SystemNative_FTruncate(fd, 0) == 0 && SystemNative_FLock(fd, 1) == 0 && SystemNative_FSync(fd) == 0, "shim-pwmfs-descriptors", "(truncate, lock or sync refused)");
        CHECK(SystemNative_LSeek(fd, 2, RHU_PAL_SEEK_SET) == 2 && SystemNative_Read(fd, t, 31) == 6 && memcmp(t, "00000\n", 6) == 0, "shim-pwmfs-descriptors", "(a read after a seek did not continue the text)");
        CHECK(SystemNative_Read(fd, t, 31) == 0, "shim-pwmfs-descriptors", "(a read at the text's end did not answer end of file)");
        CHECK(SystemNative_PRead(fd, t, 31, 5) == 3 && memcmp(t, "00\n", 3) == 0, "shim-pwmfs-descriptors", "(a positional read at 5 wrong)");
        uint32_t sets_before = w->sets;
        CHECK(SystemNative_PWrite(fd, "1000000", 7, 100) == 7 && w->sets == sets_before + 1, "shim-pwmfs-descriptors", "(a positional write was not taken as one value whatever its offset)");
        CHECK(SystemNative_Dup(fd) == -1 && PF_ERRNO_IS(RHU_GERR_ENOTSUP), "shim-pwmfs-descriptors", "(dup was not refused ENOTSUP)");
        CHECK(SystemNative_Close(fd) == 0 && SystemNative_FStat(fd, &st) == -1 && PF_ERRNO_IS(RHU_GERR_EBADF) && SystemNative_Close(fd) == -1 && PF_ERRNO_IS(RHU_GERR_EBADF), "shim-pwmfs-descriptors", "(a closed descriptor still answered)");
        pass("shim-pwmfs-descriptors", " (the sysfs band; a regular file of 4096 bytes; truncate, lock and sync 0; seek, read to the end and a positional read on the text; a positional write one value; dup ENOTSUP; a closed descriptor EBADF)");
    }

    /* The band's exhaustion and recovery. */
    {
        intptr_t fds[RHU_SHIM_FD_SYSFS_COUNT];
        int ok = 1;
        for (int i = 0; i < RHU_SHIM_FD_SYSFS_COUNT; i++) {
            fds[i] = SystemNative_Open(PF_CHIP "/npwm", RHU_PAL_O_RDONLY, 0);
            if (fds[i] < 0)
                ok = 0;
        }
        CHECK(ok, "shim-pwmfs-band", "(the band did not open 32 descriptors)");
        CHECK(SystemNative_Open(PF_CHIP "/npwm", RHU_PAL_O_RDONLY, 0) == -1 && PF_ERRNO_IS(RHU_GERR_EMFILE), "shim-pwmfs-band", "(the 33rd open was not refused EMFILE)");
        for (int i = 0; i < RHU_SHIM_FD_SYSFS_COUNT; i++) {
            if (SystemNative_Close(fds[i]) != 0)
                ok = 0;
        }
        CHECK(ok && pf_reads(PF_CHIP "/npwm", "4\n"), "shim-pwmfs-band", "(the band did not recover after the closes)");
        pass("shim-pwmfs-band", " (32 descriptors, the 33rd EMFILE, every one given back)");
    }

    /* The C library's access. */
    CHECK(access(PF_CHIP "/pwm0", RHU_PAL_R_OK | RHU_PAL_W_OK) == 0 && access(PF_CHIP "/pwm0/period", RHU_PAL_R_OK | RHU_PAL_W_OK) == 0, "shim-pwmfs-access", "(the channel directory or its period file did not pass R_OK|W_OK)");
    CHECK(access(PF_CHIP "/export", RHU_PAL_R_OK) == -1 && *__errno_location() == RHU_GERR_EACCES, "shim-pwmfs-access", "(R_OK on the write-only export was not refused EACCES)");
    CHECK(access(PF_CHIP "/pwm1", 0) == -1 && *__errno_location() == RHU_GERR_ENOENT, "shim-pwmfs-access", "(an unexported channel directory did not answer ENOENT)");
    CHECK(access("/tmp", 0) == -1 && *__errno_location() == RHU_GERR_ENOENT, "shim-pwmfs-access", "(a path outside /sys did not answer ENOENT)");
    pass("shim-pwmfs-access", " (the package's own check: a channel directory and its file R_OK|W_OK, the write-only export EACCES for R_OK, an unexported channel and a path outside /sys ENOENT)");

    /* unexport. */
    CHECK(pf_write(PF_CHIP "/unexport", "1") == -1 && PF_ERRNO_IS(RHU_GERR_ENODEV), "shim-pwmfs-unexport", "(an unexported line's unexport was not refused ENODEV)");
    CHECK(pf_write(PF_CHIP "/unexport", "0") == 1 && w->releases == 1 && SystemNative_Stat(PF_CHIP "/pwm0", &st) == -1 && PF_ERRNO_IS(RHU_GERR_ENOENT), "shim-pwmfs-unexport", "(the unexport of line 0 did not release it)");
    CHECK(pf_write(PF_CHIP "/export", "0") == 1 && w->claims_taken == 4 && pf_reads(PF_CHIP "/pwm0/period", "0\n"), "shim-pwmfs-unexport", "(a re-export after the unexport was refused, or the channel kept its period)");
    CHECK(pf_write(PF_CHIP "/unexport", "0") == 1 && pf_write(PF_CHIP "/unexport", "2") == 1 && pf_write(PF_CHIP "/unexport", "3") == 1 && w->releases == 4, "shim-pwmfs-unexport", "(the unexports of 0, 2 and 3 did not release them)");
    CHECK(pf_write(PF_CHIP "/unexport", "0") == -1 && PF_ERRNO_IS(RHU_GERR_ENODEV), "shim-pwmfs-unexport", "(a second unexport was not refused ENODEV)");
    pass("shim-pwmfs-unexport", " (an unexported line ENODEV; the release gives the line back and a re-export starts from 0; every channel released)");

    out("SHIMTEST-PWMFS: ALL GOOD (the PWM device files from a native caller's side)");
    return 0;
}

#endif /* RHU_SHIMTEST_LADDER_PWMFS */

#ifdef RHU_SHIMTEST_LADDER_IIOFS

/* --- the analog input device files, from a native caller's side: the
 * tree under /sys/bus/iio/devices the board package's analog input class
 * reads, driven through the runtime's portability verbs the class
 * reaches (the store hands a /sys/bus path and every analog-band
 * descriptor to the unit) and the C library's access, on the emulator
 * row's two virtual converter blocks: block 1 (twelve bits, 3300 mV) with
 * channels 0 to 3 on lines 0 to 3, block 2 (fourteen bits, 3000 mV) with
 * channel 0 the internal reference and channel 1 the loop channel that
 * reads the first PWM line's duty cycle. The kernel's ADC witness is read
 * by symbol after the verbs, in the kernel's layout. --- */
struct ai_witness {
    uint32_t channels, blocks, claims_taken, releases, conversions, timeouts, resets, refused_busy, refused_inval;
    uint32_t last_sample, last_refused_index;
    int32_t last_refused_code;
};
extern struct ai_witness __rhu_adc_witness;
int access(const char *path, int mode);
int32_t SystemNative_FcntlSetFD(intptr_t fd, int32_t flags);

#define AI_ROOT "/sys/bus/iio/devices"
#define AI_DEV0 AI_ROOT "/iio:device0"
#define AI_DEV1 AI_ROOT "/iio:device1"
#define AI_PWM "/sys/class/pwm/pwmchip0"
#define AI_ERRNO_IS(e) (SystemNative_GetErrNo() == (e))

/* One value written to a PWM node through the store, as the PWM class
 * writes one (the loop step drives the first PWM line through the PWM
 * device files): the count, or -1 with the write's errno kept past the
 * close. */
static int32_t ai_pwm_write(const char *path, const char *text)
{
    intptr_t fd = SystemNative_Open(path, RHU_PAL_O_WRONLY, 0);
    if (fd < 0)
        return -1;
    int32_t n = SystemNative_Write(fd, text, (int32_t)strlen(text));
    int32_t e = *__errno_location();
    SystemNative_Close(fd);
    if (n < 0)
        *__errno_location() = e;
    return n;
}

/* A file's text read through the store by an open, a read and a close (the
 * one-shot read a stock File.ReadAllText makes): the count with the text
 * terminated, or -1 with the errno kept past the close. */
static int32_t ai_read(const char *path, char *buf, int32_t cap)
{
    intptr_t fd = SystemNative_Open(path, RHU_PAL_O_RDONLY, 0);
    if (fd < 0)
        return -1;
    int32_t n = SystemNative_Read(fd, buf, cap - 1);
    int32_t e = *__errno_location();
    SystemNative_Close(fd);
    if (n >= 0)
        buf[n] = 0;
    else
        *__errno_location() = e;
    return n;
}

static int ai_reads(const char *path, const char *want)
{
    char t[32];
    int32_t n = ai_read(path, t, (int32_t)sizeof t);
    return n == (int32_t)strlen(want) && strcmp(t, want) == 0;
}

/* A held descriptor's text by a positional read at 0 (the class's read of
 * a held channel). */
static int ai_reads_fd(intptr_t fd, const char *want)
{
    char t[32];
    int32_t n = SystemNative_PRead(fd, t, (int32_t)sizeof t - 1, 0);
    if (n < 0)
        return 0;
    t[n] = 0;
    return n == (int32_t)strlen(want) && strcmp(t, want) == 0;
}

static int ai_is(const char *path, int32_t type)
{
    rhu_pal_filestatus st;
    return SystemNative_Stat(path, &st) == 0 && (st.Mode & 0xF000) == type;
}

static int ai_in_band(intptr_t fd)
{
    return fd >= RHU_SHIM_FD_IIO_LO && fd < RHU_SHIM_FD_IIO_LO + RHU_SHIM_FD_IIO_COUNT;
}

static int iiofs_ladder(void)
{
    char t[32];
    rhu_pal_filestatus st;
    const volatile struct ai_witness *w = &__rhu_adc_witness;
    out("shimtest-iiofs: the analog input device files from a native caller's side, on the row's virtual converters");
    CHECK(rhu_shim_init() == 0, "shim-iiofs-init", "(initialization refused)");
    CHECK(rhu_adc_count() == 6 && w->channels == 6 && w->blocks == 2, "shim-iiofs-init", "(the row does not state six channels over two converter blocks)");
    pass("shim-iiofs-init", " (six channels over two virtual converter blocks, by the kernel's enumeration and its witness)");

    /* The tree. */
    CHECK(ai_is("/sys/bus", RHU_PAL_S_IFDIR) && ai_is("/sys/bus/iio", RHU_PAL_S_IFDIR) && ai_is(AI_ROOT, RHU_PAL_S_IFDIR) && ai_is(AI_ROOT "/", RHU_PAL_S_IFDIR), "shim-iiofs-tree", "(a directory of the tree is missing)");
    CHECK(ai_is(AI_DEV0, RHU_PAL_S_IFDIR) && ai_is(AI_DEV0 "/", RHU_PAL_S_IFDIR) && ai_is(AI_DEV1, RHU_PAL_S_IFDIR), "shim-iiofs-tree", "(a device directory is missing)");
    CHECK(SystemNative_Stat(AI_ROOT "/iio:device2", &st) == -1 && AI_ERRNO_IS(RHU_GERR_ENOENT), "shim-iiofs-tree", "(a third device answered)");
    CHECK(ai_is(AI_DEV0 "/name", RHU_PAL_S_IFREG) && ai_is(AI_DEV0 "/in_voltage_scale", RHU_PAL_S_IFREG) && ai_is(AI_DEV0 "/in_voltage0_raw", RHU_PAL_S_IFREG) && ai_is(AI_DEV0 "/in_voltage3_raw", RHU_PAL_S_IFREG) && ai_is(AI_DEV1 "/in_voltage1_raw", RHU_PAL_S_IFREG),
          "shim-iiofs-tree", "(name, in_voltage_scale or a channel's raw file is not a regular file)");
    CHECK(SystemNative_Stat(AI_DEV0 "/in_voltage4_raw", &st) == -1 && AI_ERRNO_IS(RHU_GERR_ENOENT) && SystemNative_Stat(AI_DEV1 "/in_voltage2_raw", &st) == -1 && AI_ERRNO_IS(RHU_GERR_ENOENT), "shim-iiofs-tree", "(a channel the block has not answered)");
    CHECK(SystemNative_Stat(AI_DEV0 "/in_voltage0_offset", &st) == -1 && AI_ERRNO_IS(RHU_GERR_ENOENT) && SystemNative_Stat("/sys/bus/i2c", &st) == -1 && AI_ERRNO_IS(RHU_GERR_ENOENT), "shim-iiofs-tree", "(a file the tree has not answered)");
    CHECK(SystemNative_Open(AI_DEV0, RHU_PAL_O_RDONLY, 0) == -1 && AI_ERRNO_IS(RHU_GERR_EISDIR), "shim-iiofs-tree", "(a directory opened as a file)");
    CHECK(ai_is("/sys/class/pwm", RHU_PAL_S_IFDIR) && ai_is("/sys", RHU_PAL_S_IFDIR), "shim-iiofs-tree", "(the PWM tree's directories no longer answer beside the analog tree)");
    pass("shim-iiofs-tree", " (/sys/bus, /sys/bus/iio, the devices directory and the two device directories, with or without a trailing slash; a third device, a channel a block has not and a file the tree has not ENOENT; a directory EISDIR; the PWM tree beside it)");

    /* The name files. */
    CHECK(ai_reads(AI_DEV0 "/name", "adc1\n") && ai_reads(AI_DEV1 "/name", "adc2\n"), "shim-iiofs-name", "(the name files did not read adc1 and adc2)");
    CHECK(SystemNative_Open(AI_DEV0 "/name", RHU_PAL_O_WRONLY, 0) == -1 && AI_ERRNO_IS(RHU_GERR_EACCES), "shim-iiofs-name", "(an open of name for writing was not refused EACCES)");
    pass("shim-iiofs-name", " (the kernel's block numbers as adc1 and adc2; read-only)");

    /* The scale files. */
    CHECK(ai_reads(AI_DEV0 "/in_voltage_scale", "0.805664062\n"), "shim-iiofs-scale", "(device 0's scale did not read 0.805664062)");
    CHECK(ai_reads(AI_DEV1 "/in_voltage_scale", "0.183105468\n"), "shim-iiofs-scale", "(device 1's scale did not read 0.183105468)");
    CHECK(SystemNative_Open(AI_DEV1 "/in_voltage_scale", RHU_PAL_O_RDWR, 0) == -1 && AI_ERRNO_IS(RHU_GERR_EACCES), "shim-iiofs-scale", "(an open of the scale for writing was not refused EACCES)");
    pass("shim-iiofs-scale", " (3300 mV over twelve bits as 0.805664062, 3000 mV over fourteen bits as 0.183105468: millivolts per count with nine digits; read-only)");

    /* A channel: the open claims, the read converts, the close releases. */
    {
        uint32_t claims = w->claims_taken, conv = w->conversions, busy = w->refused_busy;
        intptr_t fd = SystemNative_Open(AI_DEV0 "/in_voltage0_raw", RHU_PAL_O_RDONLY, 0);
        CHECK(ai_in_band(fd) && w->claims_taken == claims + 1, "shim-iiofs-read", "(the channel's file did not open in the analog band with the channel claimed)");
        CHECK(SystemNative_Open(AI_DEV0 "/in_voltage0_raw", RHU_PAL_O_RDONLY, 0) == -1 && AI_ERRNO_IS(RHU_GERR_EBUSY) && w->refused_busy == busy + 1, "shim-iiofs-read", "(a second open of the held channel was not refused EBUSY)");
        CHECK(SystemNative_Read(fd, t, 31) == 4 && memcmp(t, "819\n", 4) == 0 && w->conversions == conv + 1, "shim-iiofs-read", "(the first read did not convert once and answer 819)");
        CHECK(SystemNative_Read(fd, t, 31) == 0 && w->conversions == conv + 1, "shim-iiofs-read", "(a read at the text's end did not answer end of file without a conversion)");
        CHECK(ai_reads_fd(fd, "819\n") && w->conversions == conv + 2, "shim-iiofs-read", "(a positional read at 0 did not convert again and answer 819)");
        CHECK(SystemNative_LSeek(fd, 0, RHU_PAL_SEEK_SET) == 0 && SystemNative_Read(fd, t, 31) == 4 && memcmp(t, "819\n", 4) == 0 && w->conversions == conv + 3, "shim-iiofs-read", "(a read after a seek to 0 did not convert again)");
        CHECK(SystemNative_Open(AI_DEV0 "/in_voltage9_raw", RHU_PAL_O_RDONLY, 0) == -1 && AI_ERRNO_IS(RHU_GERR_ENOENT), "shim-iiofs-read", "(a channel the block has not was not refused ENOENT)");
        CHECK(SystemNative_Open(AI_DEV0 "/in_voltage2_raw", RHU_PAL_O_WRONLY, 0) == -1 && AI_ERRNO_IS(RHU_GERR_EACCES) && SystemNative_Open(AI_DEV0 "/in_voltage2_raw", RHU_PAL_O_RDWR, 0) == -1 && AI_ERRNO_IS(RHU_GERR_EACCES) && w->claims_taken == claims + 1,
              "shim-iiofs-read", "(an open of a channel for writing was not refused EACCES with nothing claimed)");
        uint32_t releases = w->releases;
        CHECK(SystemNative_Close(fd) == 0 && w->releases == releases + 1, "shim-iiofs-read", "(the close did not release the channel)");
        fd = SystemNative_Open(AI_DEV0 "/in_voltage0_raw", RHU_PAL_O_RDONLY, 0);
        CHECK(ai_in_band(fd), "shim-iiofs-read", "(the re-open after the close was refused: the channel is still held)");
        CHECK(ai_reads_fd(fd, "819\n") && SystemNative_Close(fd) == 0 && w->releases == releases + 2, "shim-iiofs-read", "(the re-opened channel did not read 819 and release at its close)");
        CHECK(ai_reads(AI_DEV0 "/in_voltage2_raw", "2457\n") && ai_reads(AI_DEV0 "/in_voltage3_raw", "3276\n") && w->claims_taken == claims + 4 && w->releases == releases + 4, "shim-iiofs-read", "(the one-shot reads of channels 2 and 3 did not claim, convert and release)");
        pass("shim-iiofs-read", " (the open claims the channel and a second open is EBUSY; a read at 0 converts once and answers 819, a read at the end answers end of file, a positional read at 0 and a read after a seek convert again; a channel the block has not ENOENT; an open for writing EACCES with nothing claimed; the close releases and a re-open succeeds; one-shot reads of channels 2 and 3 answer 2457 and 3276)");
    }

    /* A line a program holds as a GPIO line refuses the channel until the line is released, and a held channel refuses the line until the channel is closed. */
    {
        uint32_t busy = w->refused_busy;
        CHECK(rhu_gpio_claim(1, RHU_GPIO_INPUT, RHU_GPIO_PULL_NONE, 0) == 0, "shim-iiofs-pad-law", "(the kernel's claim of line 1 as a GPIO line refused)");
        CHECK(SystemNative_Open(AI_DEV0 "/in_voltage1_raw", RHU_PAL_O_RDONLY, 0) == -1 && AI_ERRNO_IS(RHU_GERR_EBUSY), "shim-iiofs-pad-law", "(the held line's open was not refused EBUSY)");
        CHECK(w->refused_busy == busy + 1 && rhu_gpio_release(1) == 0, "shim-iiofs-pad-law", "(the refusal was not the kernel's, or the line's release refused)");
        intptr_t fd = SystemNative_Open(AI_DEV0 "/in_voltage1_raw", RHU_PAL_O_RDONLY, 0);
        CHECK(ai_in_band(fd) && ai_reads_fd(fd, "1638\n"), "shim-iiofs-pad-law", "(the channel did not open and read 1638 after the line's release)");
        CHECK(rhu_gpio_claim(1, RHU_GPIO_INPUT, RHU_GPIO_PULL_NONE, 0) == RHU_EBUSY, "shim-iiofs-pad-law", "(the kernel's claim of the held channel's line as a GPIO line was not refused EBUSY)");
        CHECK(SystemNative_Close(fd) == 0 && rhu_gpio_claim(1, RHU_GPIO_INPUT, RHU_GPIO_PULL_NONE, 0) == 0 && rhu_gpio_release(1) == 0, "shim-iiofs-pad-law", "(the line did not come back to the GPIO side after the channel's close)");
        pass("shim-iiofs-pad-law", " (line 1 held as a GPIO line: the channel's open EBUSY by the kernel's refusal; released: the channel opens and reads 1638 and the line's GPIO claim is EBUSY; closed: the line is a GPIO line again)");
    }

    /* The internal reference. */
    CHECK(ai_reads(AI_DEV1 "/in_voltage0_raw", "6619\n"), "shim-iiofs-reference", "(the internal reference did not read 6619)");
    pass("shim-iiofs-reference", " (block 2's channel 0, the 1212 mV reference over 3000 mV at fourteen bits: 6619)");

    /* The loop channel: the first PWM line's duty cycle, driven through the PWM device files beside this tree. */
    {
        intptr_t fd = SystemNative_Open(AI_DEV1 "/in_voltage1_raw", RHU_PAL_O_RDONLY, 0);
        CHECK(ai_in_band(fd) && ai_reads_fd(fd, "0\n"), "shim-iiofs-loop", "(the loop channel did not read 0 with the PWM output parked)");
        CHECK(ai_pwm_write(AI_PWM "/export", "0") == 1 && ai_pwm_write(AI_PWM "/pwm0/period", "1000000") == 7 && ai_pwm_write(AI_PWM "/pwm0/duty_cycle", "250000") == 6 && ai_pwm_write(AI_PWM "/pwm0/enable", "1") == 1,
              "shim-iiofs-loop", "(the PWM line 0 did not export and start at 25 percent through the PWM device files)");
        CHECK(ai_reads_fd(fd, "4096\n"), "shim-iiofs-loop", "(the loop channel did not read 4096 at 25 percent)");
        CHECK(ai_pwm_write(AI_PWM "/pwm0/duty_cycle", "750000") == 6 && ai_reads_fd(fd, "12287\n"), "shim-iiofs-loop", "(the loop channel did not read 12287 at 75 percent)");
        CHECK(ai_pwm_write(AI_PWM "/pwm0/duty_cycle", "250000") == 6 && ai_pwm_write(AI_PWM "/pwm0/polarity", "inversed") == 8 && ai_reads_fd(fd, "12287\n"), "shim-iiofs-loop", "(the loop channel did not read 12287 at 25 percent inversed)");
        CHECK(ai_pwm_write(AI_PWM "/pwm0/enable", "0") == 1 && ai_reads_fd(fd, "0\n"), "shim-iiofs-loop", "(the loop channel did not read 0 after the output stopped)");
        CHECK(ai_pwm_write(AI_PWM "/unexport", "0") == 1 && SystemNative_Close(fd) == 0, "shim-iiofs-loop", "(the PWM line's unexport or the loop channel's close refused)");
        pass("shim-iiofs-loop", " (0 parked; 4096 at 25 percent, 12287 at 75 percent and at 25 percent inversed, 0 after the stop: every reading a fresh conversion of the first PWM line's duty cycle through the two trees in one image)");
    }

    /* The descriptor verbs on a channel file. */
    {
        intptr_t fd = SystemNative_Open(AI_DEV0 "/in_voltage0_raw", RHU_PAL_O_RDONLY, 0);
        CHECK(ai_in_band(fd), "shim-iiofs-descriptors", "(the channel file did not open in the analog band)");
        CHECK(SystemNative_FStat(fd, &st) == 0 && (st.Mode & 0xF000) == RHU_PAL_S_IFREG && st.Size == 4096, "shim-iiofs-descriptors", "(fstat is not a regular file of 4096 bytes)");
        CHECK(SystemNative_FTruncate(fd, 0) == 0 && SystemNative_FLock(fd, 1) == 0 && SystemNative_FSync(fd) == 0 && SystemNative_FcntlSetFD(fd, 1) == 0, "shim-iiofs-descriptors", "(truncate, lock, sync or the descriptor flag refused)");
        uint32_t conv = w->conversions;
        CHECK(SystemNative_PRead(fd, t, 31, 0) == 4 && SystemNative_LSeek(fd, 2, RHU_PAL_SEEK_SET) == 2 && SystemNative_Read(fd, t, 31) == 2 && memcmp(t, "9\n", 2) == 0 && w->conversions == conv + 1, "shim-iiofs-descriptors", "(a read after a seek did not continue the last text without a conversion)");
        CHECK(SystemNative_Read(fd, t, 31) == 0 && SystemNative_PRead(fd, t, 31, 5) == 0 && w->conversions == conv + 1, "shim-iiofs-descriptors", "(a read at or past the text's end did not answer end of file)");
        CHECK(SystemNative_PWrite(fd, "1", 1, 0) == -1 && AI_ERRNO_IS(RHU_GERR_EBADF) && SystemNative_Write(fd, "1", 1) == -1 && AI_ERRNO_IS(RHU_GERR_EBADF), "shim-iiofs-descriptors", "(a write on the read-only descriptor was not refused EBADF)");
        CHECK(SystemNative_Dup(fd) == -1 && AI_ERRNO_IS(RHU_GERR_ENOTSUP), "shim-iiofs-descriptors", "(dup was not refused ENOTSUP)");
        CHECK(SystemNative_Close(fd) == 0 && SystemNative_FStat(fd, &st) == -1 && AI_ERRNO_IS(RHU_GERR_EBADF) && SystemNative_Close(fd) == -1 && AI_ERRNO_IS(RHU_GERR_EBADF), "shim-iiofs-descriptors", "(a closed descriptor still answered)");
        pass("shim-iiofs-descriptors", " (the analog band; a regular file of 4096 bytes; truncate, lock, sync and the descriptor flag 0; a read after a seek continues the last text, a read at the end answers end of file; a write EBADF; dup ENOTSUP; a closed descriptor EBADF)");
    }

    /* The band's exhaustion and recovery, on the name file (no claim). */
    {
        intptr_t fds[RHU_SHIM_FD_IIO_COUNT];
        int ok = 1;
        for (int i = 0; i < RHU_SHIM_FD_IIO_COUNT; i++) {
            fds[i] = SystemNative_Open(AI_DEV0 "/name", RHU_PAL_O_RDONLY, 0);
            if (!ai_in_band(fds[i]))
                ok = 0;
        }
        CHECK(ok, "shim-iiofs-band", "(the band did not open 16 descriptors)");
        CHECK(SystemNative_Open(AI_DEV0 "/name", RHU_PAL_O_RDONLY, 0) == -1 && AI_ERRNO_IS(RHU_GERR_EMFILE), "shim-iiofs-band", "(the 17th open was not refused EMFILE)");
        CHECK(SystemNative_Open(AI_DEV0 "/in_voltage0_raw", RHU_PAL_O_RDONLY, 0) == -1 && AI_ERRNO_IS(RHU_GERR_EMFILE) && w->claims_taken == w->releases, "shim-iiofs-band", "(a channel's open with the band full was not refused EMFILE before any claim)");
        for (int i = 0; i < RHU_SHIM_FD_IIO_COUNT; i++) {
            if (SystemNative_Close(fds[i]) != 0)
                ok = 0;
        }
        CHECK(ok && ai_reads(AI_DEV0 "/name", "adc1\n"), "shim-iiofs-band", "(the band did not recover after the closes)");
        pass("shim-iiofs-band", " (16 descriptors, the 17th EMFILE, a channel's open with the band full EMFILE before any claim, every one given back)");
    }

    /* The C library's access. */
    CHECK(access(AI_DEV0 "/in_voltage0_raw", RHU_PAL_R_OK) == 0 && access(AI_DEV0, RHU_PAL_R_OK | RHU_PAL_X_OK) == 0, "shim-iiofs-access", "(a channel file or its device directory did not pass R_OK)");
    CHECK(access(AI_DEV0 "/in_voltage0_raw", RHU_PAL_W_OK) == -1 && *__errno_location() == RHU_GERR_EACCES, "shim-iiofs-access", "(W_OK on a read-only channel file was not refused EACCES)");
    CHECK(access(AI_ROOT "/iio:device2", 0) == -1 && *__errno_location() == RHU_GERR_ENOENT, "shim-iiofs-access", "(a device the row has not did not answer ENOENT)");
    CHECK(access("/sys/class/pwm", 0) == 0 && access("/tmp", 0) == -1 && *__errno_location() == RHU_GERR_ENOENT, "shim-iiofs-access", "(the PWM tree's root or a path outside /sys did not answer as before)");
    pass("shim-iiofs-access", " (a channel file and its device directory R_OK, W_OK on a channel file EACCES, a device the row has not and a path outside /sys ENOENT, the PWM tree's root beside it)");

    /* Every channel given back. */
    CHECK(w->claims_taken == w->releases && w->claims_taken >= 8, "shim-iiofs-closed", "(a claim was not released)");
    CHECK(w->timeouts == 0 && w->resets == 0, "shim-iiofs-closed", "(a conversion passed its budget)");
    pass("shim-iiofs-closed", " (every claim released: the witness's claims equal its releases; no conversion passed its budget)");

    out("SHIMTEST-IIOFS: ALL GOOD (the analog input device files from a native caller's side)");
    return 0;
}

#endif /* RHU_SHIMTEST_LADDER_IIOFS */

#ifdef RHU_SHIMTEST_LADDER_GPIOD

/* --- the GPIO unit's library surface, from a native caller's side, on the
 * emulator row's virtual lines: line 2 an output wired to line 3, line 0
 * free. The binding's signatures and values, as the unit states them. --- */
struct gpiod_chip;
struct gpiod_line;
struct gpiod_chip_iter;
struct gd_timespec {
    int32_t sec;
    int32_t nsec;
};
struct gd_event {
    struct gd_timespec ts;
    int32_t event_type;
};
struct gpiod_chip *gpiod_chip_open_by_number(int number);
void gpiod_chip_close(struct gpiod_chip *chip);
int gpiod_chip_num_lines(struct gpiod_chip *chip);
struct gpiod_chip_iter *gpiod_chip_iter_new(void);
struct gpiod_chip *gpiod_chip_iter_next_noclose(struct gpiod_chip_iter *iter);
void gpiod_chip_iter_free_noclose(struct gpiod_chip_iter *iter);
struct gpiod_line *gpiod_chip_get_line(struct gpiod_chip *chip, int offset);
int gpiod_line_request_input(struct gpiod_line *line, const char *consumer);
int gpiod_line_request_input_flags(struct gpiod_line *line, const char *consumer, int flags);
int gpiod_line_request_output(struct gpiod_line *line, const char *consumer, int default_val);
int gpiod_line_request_both_edges_events(struct gpiod_line *line, const char *consumer);
void gpiod_line_release(struct gpiod_line *line);
int gpiod_line_set_value(struct gpiod_line *line, int value);
int gpiod_line_get_value(struct gpiod_line *line);
int gpiod_line_bias(struct gpiod_line *line);
int gpiod_line_event_wait(struct gpiod_line *line, const struct gd_timespec *timeout);
int gpiod_line_event_read(struct gpiod_line *line, struct gd_event *event);

#define GD_FLAG_PULL_DOWN (1 << 4)
#define GD_FLAG_PULL_UP (1 << 5)
#define GD_BIAS_AS_IS 1
#define GD_BIAS_PULL_UP 3
#define GD_RISING 1
#define GD_FALLING 2
#define GD_LOOP_OUT 2
#define GD_LOOP_IN 3
#define GD_ERRNO_IS(e) (*__errno_location() == (e))

/* A second thread's acts, each started by a flag and ended by a flag. */
enum { GD_ACT_NONE = 0, GD_ACT_LATE_EDGE = 1, GD_ACT_SIX_EDGES = 2, GD_ACT_DRIVE_LOW = 3, GD_ACT_OPEN = 4 };
static volatile int g_gd_act;
static volatile int g_gd_done;
static volatile int g_gd_ready;
static struct gpiod_chip *volatile g_gd_chip;

static void *gd_second(void *arg)
{
    (void)arg;
    /* In the loop from here on: the ladder waits for this before its first
     * two-thread case, so the second thread is never still unstarted. */
    __atomic_store_n(&g_gd_ready, 1, __ATOMIC_RELEASE);
    for (;;) {
        int act = __atomic_load_n(&g_gd_act, __ATOMIC_ACQUIRE);
        if (act == GD_ACT_NONE) {
            sched_yield();
            continue;
        }
        if (act == GD_ACT_LATE_EDGE) {
            msleep(50);
            rhu_gpio_write(GD_LOOP_OUT, 1);
        } else if (act == GD_ACT_SIX_EDGES) {
            for (uint32_t i = 0; i < 6; i++) {
                rhu_gpio_write(GD_LOOP_OUT, (i & 1u) == 0 ? 1u : 0u);
                sched_yield();
            }
        } else if (act == GD_ACT_DRIVE_LOW) {
            rhu_gpio_write(GD_LOOP_OUT, 0);
        } else {
            g_gd_chip = gpiod_chip_open_by_number(0);
        }
        __atomic_store_n(&g_gd_act, GD_ACT_NONE, __ATOMIC_RELEASE);
        __atomic_store_n(&g_gd_done, 1, __ATOMIC_RELEASE);
    }
    return NULL;
}

static void gd_start(int act)
{
    __atomic_store_n(&g_gd_done, 0, __ATOMIC_RELEASE);
    __atomic_store_n(&g_gd_act, act, __ATOMIC_RELEASE);
}

static void gd_join(void)
{
    wait_done(&g_gd_done, 1);
}

/* Poll, then read: the unit's read waits for an edge. */
static int gd_drain(struct gpiod_line *in)
{
    static const struct gd_timespec zero = { 0, 0 };
    struct gd_event ev;
    int n = 0;
    while (gpiod_line_event_wait(in, &zero) == 1 && gpiod_line_event_read(in, &ev) == 0)
        n++;
    return n;
}

static int gpiod_ladder(void)
{
    out("shimtest-gpiod: the GPIO library surface from a native caller's side, on the row's virtual lines");
    CHECK(rhu_shim_init() == 0, "shim-gpiod-init", "(initialization refused)");
    uint32_t tid = 0;
    CHECK(spawn_below(&tid, gd_second, NULL) == 0, "shim-gpiod-init", "(the second thread did not spawn)");
    wait_done(&g_gd_ready, 1);
    pass("shim-gpiod-init", "");

    /* Two opens in turn, then two opens at once. The opens in turn come
     * first: a close that gave nothing back would otherwise be met at the
     * two-thread case before its own check. */
    {
        struct gpiod_chip *a = gpiod_chip_open_by_number(0);
        struct gpiod_chip *b = gpiod_chip_open_by_number(0);
        struct gpiod_line *la = gpiod_chip_get_line(a, 0);
        struct gpiod_line *lb = gpiod_chip_get_line(b, 0);
        CHECK(a != NULL && b != NULL && a != b && la != NULL && lb != NULL && la != lb, "shim-gpiod-opens", "(two opens share one chip or one line object)");
        CHECK(gpiod_line_request_output(la, "first", 0) == 0, "shim-gpiod-opens", "(the request through the first chip refused)");
        CHECK(gpiod_line_request_output(lb, "second", 0) == -1 && GD_ERRNO_IS(RHU_GERR_EBUSY), "shim-gpiod-opens", "(a line held through one chip was granted through another)");
        gpiod_chip_close(a);
        CHECK(gpiod_line_request_output(lb, "second", 0) == 0, "shim-gpiod-opens", "(a closed chip's line is still held)");
        gpiod_line_release(lb);
        gpiod_chip_close(b);
        pass("shim-gpiod-opens", " (each open its own chip and lines; one holder per line across them; a close gives back its own chip's lines)");
    }
    {
        /* One pair of opens, one on each thread. Whether the two overlap is
         * the scheduler's business here; the build with the list unlocked
         * holds its window open until they do. */
        gd_start(GD_ACT_OPEN);
        struct gpiod_chip *mine = gpiod_chip_open_by_number(0);
        gd_join();
        struct gpiod_chip *theirs = g_gd_chip;
        CHECK(mine != NULL && theirs != NULL && mine != theirs, "shim-gpiod-open-race", "(two opens at once did not answer two chip objects)");
        CHECK(gpiod_chip_num_lines(mine) == rhu_gpio_count() && gpiod_chip_num_lines(theirs) == rhu_gpio_count(), "shim-gpiod-open-race",
              "(a chip opened while another thread opened is not a live chip)");
        gpiod_chip_close(theirs);
        gpiod_chip_close(mine);
        CHECK(gpiod_chip_num_lines(mine) == 0 && gpiod_chip_get_line(mine, 0) == NULL && GD_ERRNO_IS(RHU_GERR_EINVAL), "shim-gpiod-open-race",
              "(a closed chip's handle was followed)");
        pass("shim-gpiod-open-race", " (two opens at once are two live chips; a closed chip's handle is refused, never followed)");
    }
    {
        struct gpiod_chip_iter *i1 = gpiod_chip_iter_new();
        struct gpiod_chip *c1 = i1 != NULL ? gpiod_chip_iter_next_noclose(i1) : NULL;
        struct gpiod_chip_iter *i2 = gpiod_chip_iter_new();
        CHECK(c1 != NULL && i2 != NULL && i1 != i2, "shim-gpiod-iterators", "(two iterators are one object, or yielded no chip)");
        CHECK(gpiod_chip_iter_next_noclose(i1) == NULL, "shim-gpiod-iterators", "(an iterator at its end yielded again after another was made)");
        struct gpiod_chip *c2 = gpiod_chip_iter_next_noclose(i2);
        CHECK(c2 != NULL && c2 != c1 && gpiod_chip_iter_next_noclose(i2) == NULL, "shim-gpiod-iterators", "(the second iterator did not yield its own chip once)");
        gpiod_chip_iter_free_noclose(i1);
        gpiod_chip_iter_free_noclose(i2);
        CHECK(gpiod_chip_num_lines(c1) == rhu_gpio_count(), "shim-gpiod-iterators", "(a yielded chip did not outlive its iterator)");
        gpiod_chip_close(c1);
        gpiod_chip_close(c2);
        pass("shim-gpiod-iterators", " (each iterator its own progress and chip; a yielded chip is the caller's)");
    }

    struct gpiod_chip *chip = gpiod_chip_open_by_number(0);
    struct gpiod_line *l0 = gpiod_chip_get_line(chip, 0);
    struct gpiod_line *lo = gpiod_chip_get_line(chip, GD_LOOP_OUT);
    struct gpiod_line *li = gpiod_chip_get_line(chip, GD_LOOP_IN);
    CHECK(chip != NULL && l0 != NULL && lo != NULL && li != NULL, "shim-gpiod-bias", "(the chip or its lines did not open)");

    /* The bias: the flag's reading, "as is" otherwise, the pad as it is. */
    CHECK(gpiod_line_request_input_flags(l0, "t", GD_FLAG_PULL_UP | GD_FLAG_PULL_DOWN) == -1 && GD_ERRNO_IS(RHU_GERR_EINVAL), "shim-gpiod-bias",
          "(pull-up and pull-down together were accepted)");
    CHECK(gpiod_line_request_input_flags(l0, "t", GD_FLAG_PULL_UP) == 0 && gpiod_line_bias(l0) == GD_BIAS_PULL_UP && gpiod_line_get_value(l0) == 1, "shim-gpiod-bias",
          "(a pull-up input does not read pull-up with the pad high)");
    gpiod_line_release(l0);
    CHECK(gpiod_line_request_input(l0, "t") == 0 && gpiod_line_bias(l0) == GD_BIAS_AS_IS && gpiod_line_get_value(l0) == 1, "shim-gpiod-bias",
          "(a flagless input does not read \"as is\" with the pad's pull left as it was)");
    gpiod_line_release(l0);
    CHECK(gpiod_line_request_output(l0, "t", 0) == 0 && gpiod_line_bias(l0) == GD_BIAS_AS_IS, "shim-gpiod-bias", "(a plain output reads an earlier request's bias)");
    gpiod_line_release(l0);
    CHECK(gpiod_line_request_input(l0, "t") == 0 && gpiod_line_bias(l0) == GD_BIAS_AS_IS && gpiod_line_get_value(l0) == 0, "shim-gpiod-bias",
          "(a flagless input after an output does not find the pad without a pull)");
    gpiod_line_release(l0);
    pass("shim-gpiod-bias", " (two bias flags refused EINVAL; the reading the living request's flag, \"as is\" otherwise; a flagless request leaves the pad's pull as it is)");

    /* The values. */
    CHECK(gpiod_line_set_value(lo, 1) == -1 && GD_ERRNO_IS(RHU_GERR_EPERM) && gpiod_line_get_value(lo) == -1 && GD_ERRNO_IS(RHU_GERR_EPERM), "shim-gpiod-values",
          "(a line not held was written or read)");
    CHECK(gpiod_line_request_output(lo, "t", 0) == 0 && gpiod_line_request_input(li, "t") == 0, "shim-gpiod-values", "(the loop's two lines refused)");
    CHECK(gpiod_line_set_value(li, 1) == -1 && GD_ERRNO_IS(RHU_GERR_EPERM), "shim-gpiod-values", "(a write on an input was not refused EPERM)");
    CHECK(gpiod_line_set_value(lo, 1) == 0 && gpiod_line_get_value(li) == 1 && gpiod_line_set_value(lo, 0) == 0 && gpiod_line_get_value(li) == 0, "shim-gpiod-values",
          "(the loop's input does not follow its output)");
    gpiod_line_release(li);
    pass("shim-gpiod-values", " (EPERM on a line not held and on an input's write; the loop's input follows its output)");

    /* The events. */
    {
        static const struct gd_timespec second = { 1, 0 };
        static const struct gd_timespec zero = { 0, 0 };
        struct gd_event ev;
        CHECK(gpiod_line_event_wait(li, &zero) == -1 && GD_ERRNO_IS(RHU_GERR_EPERM), "shim-gpiod-events", "(an event wait on a line with no event request was not refused EPERM)");
        CHECK(gpiod_line_request_both_edges_events(li, "t") == 0, "shim-gpiod-events", "(the both-edges request refused)");
        CHECK(gpiod_line_event_wait(li, &zero) == 0, "shim-gpiod-events", "(a wait with nothing counted did not time out)");
        gpiod_line_set_value(lo, 1);
        CHECK(gpiod_line_event_wait(li, &second) == 1 && gpiod_line_event_read(li, &ev) == 0 && ev.event_type == GD_RISING, "shim-gpiod-events", "(a rising edge was not delivered as rising)");
        gpiod_line_set_value(lo, 0);
        CHECK(gpiod_line_event_wait(li, &second) == 1 && gpiod_line_event_read(li, &ev) == 0 && ev.event_type == GD_FALLING, "shim-gpiod-events", "(a falling edge was not delivered as falling)");
        gpiod_line_set_value(lo, 1);
        gpiod_line_set_value(lo, 0);
        CHECK(gpiod_line_event_read(li, &ev) == 0 && ev.event_type == GD_RISING && gpiod_line_event_read(li, &ev) == 0 && ev.event_type == GD_FALLING && gd_drain(li) == 0,
              "shim-gpiod-events", "(a burst of two edges was not delivered as rising then falling, once each)");
        pass("shim-gpiod-events", " (EPERM without an event request; each edge delivered once with its kind; a burst alternates backwards from the last)");
    }
    {
        static const struct gd_timespec neg_ns = { 0, -1 };
        static const struct gd_timespec neg_s = { -1, 0 };
        static const struct gd_timespec big_ns = { 0, 1000000000 };
        CHECK(gpiod_line_event_wait(li, &neg_ns) == -1 && GD_ERRNO_IS(RHU_GERR_EINVAL) && gpiod_line_event_wait(li, &neg_s) == -1 && GD_ERRNO_IS(RHU_GERR_EINVAL)
                  && gpiod_line_event_wait(li, &big_ns) == -1 && GD_ERRNO_IS(RHU_GERR_EINVAL),
              "shim-gpiod-timeouts", "(a timespec field outside its range became a deadline)");
        gd_start(GD_ACT_LATE_EDGE);
        CHECK(gpiod_line_event_wait(li, NULL) == 1, "shim-gpiod-timeouts", "(a wait with no timespec did not wait for the edge)");
        gd_join();
        CHECK(gd_drain(li) == 1, "shim-gpiod-timeouts", "(the late edge was not delivered once)");
        rhu_gpio_write(GD_LOOP_OUT, 0);
        CHECK(gd_drain(li) == 1, "shim-gpiod-timeouts", "(the edge back was not delivered once)");
        pass("shim-gpiod-timeouts", " (a field outside its range refused EINVAL; no timespec waits without limit)");
    }

    /* The arm: every edge the kernel counts from the arm's first instant
     * is delivered. The kernel's word for the line outlives a release, so
     * its count is read here across one. */
    {
        const volatile uint32_t *w = (const volatile uint32_t *)rhu_gpio_event_word(GD_LOOP_IN);
        CHECK(w != NULL, "shim-gpiod-arm", "(the kernel states no event word for the claimed line)");
        gpiod_line_release(li);
        uint32_t before = *w;
        gd_start(GD_ACT_SIX_EDGES);
        CHECK(gpiod_line_request_both_edges_events(li, "t") == 0, "shim-gpiod-arm", "(the both-edges request refused)");
        gd_join();
        uint32_t counted = *w - before;
        uint32_t delivered = (uint32_t)gd_drain(li);
        CHECK(counted >= 1u, "shim-gpiod-arm", "(no edge landed after the arm: the case proved nothing)");
        CHECK(delivered == counted, "shim-gpiod-arm", "(an edge the kernel counted after the arm was never delivered)");
        pass("shim-gpiod-arm", " (every edge counted from the arm on is delivered)");
    }

    /* The kind: the first event after a known rising edge is rising,
     * whatever a second thread does to the line meanwhile. */
    {
        struct gd_event ev;
        rhu_gpio_write(GD_LOOP_OUT, 0);
        gd_drain(li);
        rhu_gpio_write(GD_LOOP_OUT, 1);
        gd_start(GD_ACT_DRIVE_LOW);
        CHECK(gpiod_line_event_read(li, &ev) == 0 && ev.event_type == GD_RISING, "shim-gpiod-kind", "(the first event after a rising edge was labelled falling)");
        gd_join();
        gd_drain(li);
        pass("shim-gpiod-kind", " (the count and the level read together)");
    }

    gpiod_line_release(li);
    gpiod_line_release(lo);
    gpiod_chip_close(chip);
    out("SHIMTEST-GPIOD: ALL GOOD (the GPIO library surface from a native caller's side)");
    return 0;
}

#endif /* RHU_SHIMTEST_LADDER_GPIOD */

#ifdef RHU_SHIMTEST_LADDER_TTYDEV

/* --- the serial device files, from a native caller's side: the file
 * system's view of /dev through the runtime's portability verbs, then one
 * port through the C library's open, ioctl, close, read, write, poll,
 * fcntl and the termios family, on the emulator row's second UART, whose
 * far end echoes every byte (the test runner's echo peer on the emulator's
 * second serial port); then the stock serial layer driven through its own
 * entry points on the same port, so its object is linked and tested here as
 * well as through the managed test program. The first UART is the kernel's
 * console,
 * so its device file is held (EBUSY) and the second is the ladder's. Every
 * wait is bounded, so a run without the echo ends in a FAIL line. One
 * step runs a second thread below this one, blocked in a read while
 * the descriptor is closed under it. --- */
#include <fcntl.h>
#include "ports/termios.h"
#include "ports/sys/ioctl.h"
#include "ports/poll.h"

long read(int fd, void *buf, size_t n);
long write(int fd, const void *buf, size_t n);
int fcntl(int fd, int cmd, ...);
int shutdown(int fd, int how);

/* The stock layer's entry points, as its headers declare them. */
typedef struct td_poll_event {
    int32_t FileDescriptor;
    int16_t Events;
    int16_t TriggeredEvents;
} td_poll_event;
intptr_t SystemIoPortsNative_SerialPortOpen(const char *name);
int SystemIoPortsNative_SerialPortClose(intptr_t fd);
int32_t SystemIoPortsNative_Read(intptr_t fd, void *buffer, int32_t bufferSize);
int32_t SystemIoPortsNative_Write(intptr_t fd, const void *buffer, int32_t bufferSize);
int32_t SystemIoPortsNative_Poll(td_poll_event *pollEvents, uint32_t eventCount, int32_t milliseconds, uint32_t *triggered);
int32_t SystemIoPortsNative_Shutdown(intptr_t socket, int32_t socketShutdown);
int32_t SystemIoPortsNative_TermiosReset(intptr_t fd, int32_t speed, int32_t dataBits, int32_t stopBits, int32_t parity, int32_t handshake);
int32_t SystemIoPortsNative_TermiosGetSpeed(intptr_t fd);
int32_t SystemIoPortsNative_TermiosAvailableBytes(intptr_t fd, int32_t readBuffer);
int32_t SystemIoPortsNative_TermiosGetAllSignals(intptr_t fd);
int32_t SystemIoPortsNative_TermiosSendBreak(intptr_t fd, int32_t duration);

#define TD_ERRNO_IS(e) (*__errno_location() == (e))
#define TD_FLAGS (O_RDWR | O_NOCTTY | O_CLOEXEC | O_NONBLOCK)
#define TD_PORT "/dev/ttyS1"
#define TD_ALIAS "/dev/ttyAMA1"
#define TD_ECHO_BYTES 1500
#define TD_IN_BAND(fd) ((fd) >= RHU_SHIM_FD_TTY_LO && (fd) < RHU_SHIM_FD_TTY_LO + RHU_SHIM_FD_TTY_COUNT)
#define TD_SIG_DSR (1 << 1)
#define TD_SIG_CTS (1 << 3)
#define TD_SIG_DCD (1 << 4)

static uint8_t g_td_out[TD_ECHO_BYTES];
static uint8_t g_td_in[TD_ECHO_BYTES];
/* The exchange's count sent and the errno of a refused write or read,
 * for the bytes step's line. */
static int g_td_sent, g_td_werr, g_td_rerr;

/* A count in decimal. */
static const char *td_dec(long v, char *buf)
{
    char tmp[24];
    int n = 0, i = 0;
    unsigned long u = v < 0 ? 0ul - (unsigned long)v : (unsigned long)v;
    do {
        tmp[n++] = (char)('0' + u % 10ul);
        u /= 10ul;
    } while (u != 0ul);
    if (v < 0)
        buf[i++] = '-';
    while (n > 0)
        buf[i++] = tmp[--n];
    buf[i] = 0;
    return buf;
}

/* The bytes step's counts on one line: what the ladder sent and got,
 * the port's two queues after the exchange, a refused write or read. */
static void td_bytes_line(int got, int pending, int received)
{
    char line[LINE_MAX], d[24];
    uint32_t n = put(line, 0u, "shimtest-ttydev: bytes: sent ");
    n = put(line, n, td_dec(g_td_sent, d));
    n = put(line, n, ", got ");
    n = put(line, n, td_dec(got, d));
    n = put(line, n, ", pending ");
    n = put(line, n, td_dec(pending, d));
    n = put(line, n, ", received ");
    n = put(line, n, td_dec(received, d));
    if (g_td_werr != 0) {
        n = put(line, n, ", a write refused with errno ");
        n = put(line, n, td_dec(g_td_werr, d));
    }
    if (g_td_rerr != 0) {
        n = put(line, n, ", a read refused with errno ");
        n = put(line, n, td_dec(g_td_rerr, d));
    }
    line[n] = 0;
    out(line);
}

static int64_t td_now_ms(void)
{
    struct ts64 t;
    __clock_gettime64(1, &t);
    return t.sec * 1000 + t.nsec / 1000000;
}

/* Wait until the port has `want` bytes pending, or the bound passes; the
 * count pending at the end. */
static int td_wait_pending(int fd, int want, int bound_ms)
{
    int64_t start = td_now_ms();
    for (;;) {
        int n = 0;
        if (ioctl(fd, FIONREAD, &n) != 0)
            return -1;
        if (n >= want || td_now_ms() - start > bound_ms)
            return n;
        struct pollfd p = {fd, POLLIN, 0};
        (void)poll(&p, 1, 20);
    }
}

/* Write `n` bytes whole through partial writes on a non-blocking
 * descriptor, reading the echo back as it arrives so no ring fills; the
 * bytes read land in `in`. Answers the count read within the bound. */
static int td_exchange(int fd, const uint8_t *out, uint8_t *in, int n, int bound_ms)
{
    int sent = 0, got = 0;
    int64_t start = td_now_ms();
    g_td_sent = 0;
    g_td_werr = 0;
    g_td_rerr = 0;
    while ((sent < n || got < n) && td_now_ms() - start <= bound_ms) {
        if (sent < n) {
            int chunk = n - sent > 64 ? 64 : n - sent;
            long w = write(fd, out + sent, (size_t)chunk);
            if (w > 0) {
                sent += (int)w;
                g_td_sent = sent;
            } else if (w < 0 && !TD_ERRNO_IS(RHU_GERR_EAGAIN)) {
                g_td_werr = *__errno_location();
                return -1;
            }
        }
        struct pollfd p = {fd, POLLIN, 0};
        (void)poll(&p, 1, sent < n ? 0 : 20);
        long r = read(fd, in + got, (size_t)(n - got));
        if (r > 0) {
            got += (int)r;
        } else if (r < 0 && !TD_ERRNO_IS(RHU_GERR_EAGAIN)) {
            g_td_rerr = *__errno_location();
            return -1;
        }
    }
    return got;
}

static int td_same_attrs(const struct termios *a, const struct termios *b)
{
    return a->c_iflag == b->c_iflag && a->c_oflag == b->c_oflag && a->c_cflag == b->c_cflag && a->c_lflag == b->c_lflag
        && memcmp(a->c_cc, b->c_cc, sizeof a->c_cc) == 0 && a->c_ispeed == b->c_ispeed && a->c_ospeed == b->c_ospeed;
}

/* The worker of the close-under-read case: one blocking read on the
 * descriptor the case names, its answer and errno kept for the verdict. */
static int g_td_fd;
static volatile int g_td_entered, g_td_finished;
static long g_td_rc;
static int g_td_errno;

static void *td_blocked_read(void *arg)
{
    (void)arg;
    uint8_t b[4];
    __atomic_store_n(&g_td_entered, 1, __ATOMIC_RELEASE);
    g_td_rc = read(g_td_fd, b, sizeof b);
    g_td_errno = *__errno_location();
    __atomic_store_n(&g_td_finished, 1, __ATOMIC_RELEASE);
    return NULL;
}
static int ttydev_ladder(void)
{
    out("shimtest-ttydev: the serial device files from a native caller's side, on the row's UART blocks");
    CHECK(rhu_shim_init() == 0, "shim-ttydev-init", "(initialization refused)");
    pass("shim-ttydev-init", " (the shim initialized)");
    /* the enumeration: no /sys/class/tty, so the standard class lists /dev and keeps the tty names */
    {
        rhu_pal_filestatus st;
        CHECK(SystemNative_Stat("/sys/class/tty", &st) == -1 && SystemNative_GetErrNo() == RHU_GERR_ENOENT, "shim-ttydev-enumeration", "(/sys/class/tty is not absent)");
        void *dir = SystemNative_OpenDir("/dev");
        CHECK(dir != NULL, "shim-ttydev-enumeration", "(/dev did not open as a directory)");
        rhu_pal_directoryentry e;
        int seen = 0, chr = 0, order_ok = 1;
        while (SystemNative_ReadDir(dir, &e) == 0) {
            if (strlen(e.Name) < 4 || memcmp(e.Name, "tty", 3) != 0)
                continue;
            if (e.InodeType == RHU_PAL_DT_CHR)
                chr++;
            if ((seen == 0 && strcmp(e.Name, "ttyS0") != 0) || (seen == 1 && strcmp(e.Name, "ttyS1") != 0))
                order_ok = 0;
            seen++;
        }
        CHECK(SystemNative_CloseDir(dir) == 0, "shim-ttydev-enumeration", "(closedir)");
        CHECK(seen == 2 && chr == 2 && order_ok, "shim-ttydev-enumeration", "(the listing of /dev did not show ttyS0 then ttyS1 as character devices and no other tty name)");
        CHECK(SystemNative_Stat(TD_PORT, &st) == 0 && (st.Mode & 0xF000) == RHU_PAL_S_IFCHR, "shim-ttydev-enumeration", "(ttyS1's status is not a character device)");
        CHECK(SystemNative_LStat(TD_PORT, &st) == 0 && (st.Mode & 0xF000) == RHU_PAL_S_IFCHR, "shim-ttydev-enumeration", "(ttyS1's link status is not a character device)");
        CHECK(SystemNative_Stat("/dev/ttyS9", &st) == -1 && SystemNative_GetErrNo() == RHU_GERR_ENOENT, "shim-ttydev-enumeration", "(ttyS9 is not absent)");
        CHECK(SystemNative_Access(TD_PORT, RHU_PAL_R_OK | RHU_PAL_W_OK) == 0, "shim-ttydev-enumeration", "(ttyS1 is not readable and writable)");
        CHECK(SystemNative_Access("/dev/ttyS9", RHU_PAL_F_OK) == -1, "shim-ttydev-enumeration", "(ttyS9 is present to access)");
        pass("shim-ttydev-enumeration", " (/sys/class/tty absent; /dev lists ttyS0 and ttyS1 as character devices in the rows' order; ttyS1's status a character device, readable and writable; ttyS9 absent)");
    }

    /* the open: the stock layer's flags, the descriptor in the tty band */
    int fd = open(TD_PORT, TD_FLAGS);
    CHECK(TD_IN_BAND(fd), "shim-ttydev-open", "(the open did not answer a descriptor of the tty band)");
    pass("shim-ttydev-open", " (/dev/ttyS1 opened read-write, no controlling terminal, close-on-exec, non-blocking; the descriptor in the tty band)");

    /* the refusals: the console's port, a second opener, the alias of a held port, a missing port, a malformed name */
    CHECK(open("/dev/ttyS0", TD_FLAGS) == -1 && TD_ERRNO_IS(RHU_GERR_EBUSY), "shim-ttydev-refusals", "(the console's port did not refuse EBUSY)");
    CHECK(open(TD_PORT, TD_FLAGS) == -1 && TD_ERRNO_IS(RHU_GERR_EBUSY), "shim-ttydev-refusals", "(a second opener was not refused EBUSY)");
    CHECK(open(TD_ALIAS, TD_FLAGS) == -1 && TD_ERRNO_IS(RHU_GERR_EBUSY), "shim-ttydev-refusals", "(the alias of a held port was not refused EBUSY)");
    CHECK(open("/dev/ttyS9", TD_FLAGS) == -1 && TD_ERRNO_IS(RHU_GERR_ENOENT), "shim-ttydev-refusals", "(a port the board has not was not refused ENOENT)");
    CHECK(open("/dev/ttyS01", TD_FLAGS) == -1 && TD_ERRNO_IS(RHU_GERR_ENOENT), "shim-ttydev-refusals", "(a malformed name was not refused ENOENT)");
    pass("shim-ttydev-refusals", " (ttyS0 EBUSY, the kernel's console holding it; a second opener EBUSY; the alias of the held port EBUSY; ttyS9 ENOENT; a name with a leading zero ENOENT)");

    /* the termios round trip: the claim's state read; the raw image with a two-byte minimum and a timer set and read back equal */
    struct termios t, u;
    {
        CHECK(tcgetattr(fd, &t) == 0, "shim-ttydev-termios", "(tcgetattr refused)");
        CHECK(cfgetospeed(&t) == B115200 && cfgetispeed(&t) == B115200 && (t.c_cflag & CSIZE) == CS8 && t.c_cc[VMIN] == 1 && t.c_cc[VTIME] == 0, "shim-ttydev-termios", "(the claim's state is not 115200 8N1 with one byte to satisfy a read and no timer)");
        cfmakeraw(&t);
        CHECK(cfsetspeed(&t, B115200) == 0, "shim-ttydev-termios", "(cfsetspeed refused)");
        t.c_cc[VMIN] = 2;
        t.c_cc[VTIME] = 1;
        CHECK(tcsetattr(fd, TCSANOW, &t) == 0, "shim-ttydev-termios", "(tcsetattr refused the raw image)");
        memset(&u, 0, sizeof u);
        CHECK(tcgetattr(fd, &u) == 0, "shim-ttydev-termios", "(tcgetattr refused after the set)");
        CHECK(td_same_attrs(&t, &u), "shim-ttydev-termios", "(the attributes read back are not the set attributes)");
        t.c_cc[VMIN] = 1;
        t.c_cc[VTIME] = 0;
        CHECK(tcsetattr(fd, TCSANOW, &t) == 0, "shim-ttydev-termios", "(tcsetattr refused the one-byte minimum)");
        pass("shim-ttydev-termios", " (the claim's state 115200 8N1, one byte, no timer; the raw image with a two-byte minimum and a one-tenth timer set and read back equal; then one byte, no timer)");
    }

    /* the frame refusal: seven data bits refused by the port, the image unchanged */
    {
        struct termios f = t;
        f.c_cflag = (f.c_cflag & ~(tcflag_t)CSIZE) | CS7;
        CHECK(tcsetattr(fd, TCSANOW, &f) == -1 && TD_ERRNO_IS(RHU_GERR_EINVAL), "shim-ttydev-frame", "(seven data bits were not refused EINVAL)");
        CHECK(tcgetattr(fd, &u) == 0 && (u.c_cflag & CSIZE) == CS8, "shim-ttydev-frame", "(the image moved under a refused set)");
        pass("shim-ttydev-frame", " (CS7 refused EINVAL: the port takes 8N1 alone; the image unchanged)");
    }

    /* the rate: past the divider's floor refused, 921600 accepted with its achieved rate, 115200 again */
    {
        struct termios r = t;
        CHECK(cfsetspeed(&r, B2000000) == 0, "shim-ttydev-rate", "(cfsetspeed B2000000 refused)");
        CHECK(tcsetattr(fd, TCSANOW, &r) == -1 && TD_ERRNO_IS(RHU_GERR_EINVAL), "shim-ttydev-rate", "(2000000 was not refused EINVAL)");
        CHECK(tcgetattr(fd, &u) == 0 && cfgetospeed(&u) == B115200, "shim-ttydev-rate", "(the rate moved under a refused set)");
        CHECK(cfsetspeed(&r, B921600) == 0 && tcsetattr(fd, TCSANOW, &r) == 0, "shim-ttydev-rate", "(921600 was refused)");
        CHECK(tcgetattr(fd, &u) == 0 && cfgetospeed(&u) == B921600 && cfgetispeed(&u) == B921600, "shim-ttydev-rate", "(921600 did not read back)");
        CHECK(tcsetattr(fd, TCSANOW, &t) == 0, "shim-ttydev-rate", "(the return to 115200 was refused)");
        pass("shim-ttydev-rate", " (2000000 refused EINVAL, past the divider's floor; 921600 accepted with the achieved rate in the port's line and read back; 115200 again)");
    }

    /* the bytes: 1,500 through partial writes and non-blocking reads with poll, byte-equal */
    {
        for (int i = 0; i < TD_ECHO_BYTES; i++)
            g_td_out[i] = (uint8_t)(i * 7 + 3);
        int got = td_exchange(fd, g_td_out, g_td_in, TD_ECHO_BYTES, 10000);
        int pending = -1, received = -1;
        (void)ioctl(fd, TIOCOUTQ, &pending);
        (void)ioctl(fd, FIONREAD, &received);
        td_bytes_line(got, pending, received);
        CHECK(got == TD_ECHO_BYTES, "shim-ttydev-bytes", "(the echo did not return every byte within the bound)");
        CHECK(memcmp(g_td_out, g_td_in, TD_ECHO_BYTES) == 0, "shim-ttydev-bytes", "(the bytes read back differ from the bytes written)");
        pass("shim-ttydev-bytes", " (1500 bytes written in partial writes and read back non-blocking with poll, byte-equal)");
    }

    /* the empty non-blocking read: EAGAIN, never 0 */
    {
        uint8_t b;
        long r = read(fd, &b, 1);
        CHECK(r == -1 && TD_ERRNO_IS(RHU_GERR_EAGAIN), "shim-ttydev-nonblock", "(an empty non-blocking read did not answer EAGAIN)");
        pass("shim-ttydev-nonblock", " (an empty non-blocking read answers EAGAIN)");
    }

    /* poll on the idle port: writable, not readable, no error; a poll for input times out */
    {
        struct pollfd p = {fd, POLLIN | POLLOUT, 0};
        int n = poll(&p, 1, 0);
        CHECK(n == 1 && (p.revents & POLLOUT) != 0 && (p.revents & POLLIN) == 0 && (p.revents & (POLLERR | POLLHUP | POLLNVAL)) == 0, "shim-ttydev-poll", "(poll reported an error on a live port)");
        struct pollfd q = {fd, POLLIN, 0};
        int64_t t0 = td_now_ms();
        CHECK(poll(&q, 1, 100) == 0 && td_now_ms() - t0 >= 80, "shim-ttydev-poll", "(a poll for input on the idle port did not time out)");
        pass("shim-ttydev-poll", " (the idle port polls writable, not readable, with no error; a 100 ms poll for input times out)");
    }

    /* the counts: TIOCOUTQ 0 after tcdrain; FIONREAD exactly 8 after an eight-byte write and its echo */
    {
        uint8_t eight[8] = {1, 2, 3, 4, 5, 6, 7, 8};
        CHECK(write(fd, eight, 8) == 8, "shim-ttydev-counts", "(the eight-byte write was partial)");
        CHECK(tcdrain(fd) == 0, "shim-ttydev-counts", "(tcdrain refused)");
        int outq = -1;
        CHECK(ioctl(fd, TIOCOUTQ, &outq) == 0 && outq == 0, "shim-ttydev-counts", "(TIOCOUTQ is not 0 after tcdrain)");
        CHECK(td_wait_pending(fd, 8, 2000) == 8, "shim-ttydev-counts", "(FIONREAD did not reach exactly 8 after the echo)");
        uint8_t back[8];
        CHECK(read(fd, back, 8) == 8 && memcmp(eight, back, 8) == 0, "shim-ttydev-counts", "(the eight bytes did not read back)");
        pass("shim-ttydev-counts", " (TIOCOUTQ 0 after tcdrain; FIONREAD exactly 8 after an eight-byte write and its echo)");
    }

    /* the modem lines: CTS, DSR and DCD always; DTR and RTS latched, moved by MBIC and MBIS */
    {
        int bits = 0;
        CHECK(ioctl(fd, TIOCMGET, &bits) == 0 && (bits & (TIOCM_CTS | TIOCM_DSR | TIOCM_CAR)) == (TIOCM_CTS | TIOCM_DSR | TIOCM_CAR) && (bits & (TIOCM_DTR | TIOCM_RTS)) == (TIOCM_DTR | TIOCM_RTS), "shim-ttydev-modem", "(TIOCMGET does not read CTS, DSR and DCD with DTR and RTS latched)");
        int dtr = TIOCM_DTR;
        CHECK(ioctl(fd, TIOCMBIC, &dtr) == 0 && ioctl(fd, TIOCMGET, &bits) == 0 && (bits & TIOCM_DTR) == 0 && (bits & TIOCM_RTS) != 0, "shim-ttydev-modem", "(TIOCMBIC did not clear DTR alone)");
        CHECK(ioctl(fd, TIOCMBIS, &dtr) == 0 && ioctl(fd, TIOCMGET, &bits) == 0 && (bits & TIOCM_DTR) != 0, "shim-ttydev-modem", "(TIOCMBIS did not set DTR back)");
        pass("shim-ttydev-modem", " (CTS, DSR and DCD read asserted; DTR and RTS latched at the open; MBIC clears DTR alone and MBIS sets it back, the port's line once)");
    }

    /* tcflush discards the pending input; tcsendbreak is a no-op on this port */
    {
        uint8_t four[4] = {9, 8, 7, 6};
        CHECK(write(fd, four, 4) == 4 && td_wait_pending(fd, 4, 2000) == 4, "shim-ttydev-flush", "(the four bytes did not arrive)");
        int n = -1;
        CHECK(tcflush(fd, TCIOFLUSH) == 0 && ioctl(fd, FIONREAD, &n) == 0 && n == 0, "shim-ttydev-flush", "(tcflush did not discard the pending input)");
        CHECK(tcsendbreak(fd, 0) == 0, "shim-ttydev-flush", "(tcsendbreak refused)");
        pass("shim-ttydev-flush", " (tcflush discards four pending bytes; tcsendbreak answers 0 with the port's no-break line)");
    }

    /* fcntl and the four blocking cases of VMIN and VTIME */
    {
        int fl = fcntl(fd, F_GETFL);
        CHECK((fl & O_NONBLOCK) != 0 && (fl & O_ACCMODE) == O_RDWR, "shim-ttydev-blocking", "(F_GETFL does not read O_NONBLOCK and O_RDWR)");
        CHECK(fcntl(fd, F_SETFL, fl & ~O_NONBLOCK) == 0 && (fcntl(fd, F_GETFL) & O_NONBLOCK) == 0, "shim-ttydev-blocking", "(F_SETFL did not clear O_NONBLOCK)");
        struct termios v = t;
        uint8_t b[4];
        v.c_cc[VMIN] = 0;
        v.c_cc[VTIME] = 0;
        CHECK(tcsetattr(fd, TCSANOW, &v) == 0 && read(fd, b, 4) == 0, "shim-ttydev-blocking", "(VMIN 0 VTIME 0 did not answer 0 at once)");
        v.c_cc[VMIN] = 1;
        v.c_cc[VTIME] = 0;
        CHECK(tcsetattr(fd, TCSANOW, &v) == 0 && write(fd, "A", 1) == 1 && read(fd, b, 4) == 1 && b[0] == 'A', "shim-ttydev-blocking", "(VMIN 1 VTIME 0 did not answer the one echoed byte)");
        v.c_cc[VMIN] = 0;
        v.c_cc[VTIME] = 2;
        int64_t t0 = td_now_ms();
        CHECK(tcsetattr(fd, TCSANOW, &v) == 0 && read(fd, b, 4) == 0 && td_now_ms() - t0 >= 150, "shim-ttydev-blocking", "(VMIN 0 VTIME 2 did not answer 0 after the timer on a silent line)");
        v.c_cc[VMIN] = 2;
        v.c_cc[VTIME] = 1;
        CHECK(tcsetattr(fd, TCSANOW, &v) == 0 && write(fd, "BC", 2) == 2 && read(fd, b, 4) == 2 && b[0] == 'B' && b[1] == 'C', "shim-ttydev-blocking", "(VMIN 2 VTIME 1 did not answer the two echoed bytes)");
        CHECK(tcsetattr(fd, TCSANOW, &t) == 0 && fcntl(fd, F_SETFL, fl) == 0 && (fcntl(fd, F_GETFL) & O_NONBLOCK) != 0, "shim-ttydev-blocking", "(the one-byte minimum and O_NONBLOCK were not restored)");
        pass("shim-ttydev-blocking", " (F_GETFL and F_SETFL move O_NONBLOCK; blocking reads: 0/0 answers 0 at once, 1/0 waits for the one echoed byte, 0/2 answers 0 after the timer on a silent line, 2/1 waits for two then the inter-byte time)");
    }

    /* the close, then the alias opens the same block */
    CHECK(close(fd) == 0, "shim-ttydev-reopen", "(the close refused)");
    {
        int fa = open(TD_ALIAS, TD_FLAGS);
        CHECK(TD_IN_BAND(fa), "shim-ttydev-reopen", "(the alias did not open after the close: the port is still held)");
        CHECK(close(fa) == 0, "shim-ttydev-reopen", "(the alias's close refused)");
        pass("shim-ttydev-reopen", " (closed, then /dev/ttyAMA1 opens the same block and closes)");
    }

    /* a close under a blocked read on a second thread: the read answers EBADF at the close; the block goes to
     * no new opener while the read still runs on the closed descriptor; the same descriptor comes back after */
    {
        int fb = open(TD_PORT, TD_FLAGS);
        CHECK(TD_IN_BAND(fb), "shim-ttydev-close-under-read", "(the port did not open for the case)");
        int fl = fcntl(fb, F_GETFL);
        CHECK(fcntl(fb, F_SETFL, fl & ~O_NONBLOCK) == 0, "shim-ttydev-close-under-read", "(F_SETFL did not clear O_NONBLOCK)");
        uint32_t tid = 0;
        g_td_fd = fb;
        CHECK(spawn_below(&tid, td_blocked_read, NULL) == 0, "shim-ttydev-close-under-read", "(the worker did not spawn)");
        /* The worker runs below this thread: it enters its read while
         * this thread sleeps and blocks there with nothing on the line;
         * it runs again only while this thread waits for its end, so the
         * open right after the close always finds the read still on the
         * closed descriptor. */
        while (__atomic_load_n(&g_td_entered, __ATOMIC_ACQUIRE) == 0)
            msleep(1);
        msleep(20);
        CHECK(__atomic_load_n(&g_td_finished, __ATOMIC_ACQUIRE) == 0, "shim-ttydev-close-under-read", "(the read ended before the close: the case proved nothing)");
        CHECK(close(fb) == 0, "shim-ttydev-close-under-read", "(the close under a blocked read did not answer 0)");
        CHECK(open(TD_PORT, TD_FLAGS) == -1 && TD_ERRNO_IS(RHU_GERR_EBUSY), "shim-ttydev-close-under-read", "(the block was reopened while a request still ran on its closed descriptor)");
        wait_done(&g_td_finished, 1);
        CHECK(g_td_rc == -1 && g_td_errno == RHU_GERR_EBADF, "shim-ttydev-close-under-read", "(the blocked read did not answer EBADF at the close)");
        int fc = open(TD_PORT, TD_FLAGS);
        CHECK(fc == fb, "shim-ttydev-close-under-read", "(the descriptor was not given back when its last request left)");
        CHECK(close(fc) == 0, "shim-ttydev-close-under-read", "(the close refused)");
        pass("shim-ttydev-close-under-read", " (the close under a blocked read answers 0 at once and the read answers EBADF; a new opener is refused EBUSY while the read still runs on the closed descriptor; the same descriptor comes back after)");
    }

    /* the stock layer through its own entry points on the same port */
    {
        intptr_t s = SystemIoPortsNative_SerialPortOpen(TD_PORT);
        CHECK(TD_IN_BAND(s), "shim-ttydev-stock", "(the layer's open did not answer a descriptor of the tty band)");
        CHECK(SystemIoPortsNative_TermiosReset(s, 115200, 8, 1, 0, 0) == 0, "shim-ttydev-stock", "(the layer's reset to 115200 8N1 without flow control refused)");
        CHECK(SystemIoPortsNative_TermiosGetSpeed(s) == 115200, "shim-ttydev-stock", "(the layer did not read 115200)");
        CHECK(SystemIoPortsNative_TermiosReset(s, 115200, 8, 1, 0, 2) != 0, "shim-ttydev-stock", "(the layer's reset with hardware flow control was not refused)");
        CHECK(SystemIoPortsNative_TermiosGetSpeed(s) == 115200, "shim-ttydev-stock", "(the refused reset moved the rate)");
        static const uint8_t hello[5] = {'h', 'e', 'l', 'l', 'o'};
        int sent = 0;
        int64_t t0 = td_now_ms();
        while (sent < 5 && td_now_ms() - t0 < 2000) {
            int32_t w = SystemIoPortsNative_Write(s, hello + sent, 5 - sent);
            if (w > 0)
                sent += w;
        }
        CHECK(sent == 5, "shim-ttydev-stock", "(the layer's write did not send five bytes)");
        td_poll_event ev;
        ev.FileDescriptor = (int32_t)s;
        ev.Events = 0x0001;
        ev.TriggeredEvents = 0;
        uint32_t triggered = 0;
        uint8_t back[5];
        int got = 0;
        t0 = td_now_ms();
        while (got < 5 && td_now_ms() - t0 < 2000) {
            ev.TriggeredEvents = 0;
            if (SystemIoPortsNative_Poll(&ev, 1, 200, &triggered) != 0)
                break;
            if (triggered == 0)
                continue;
            int32_t r = SystemIoPortsNative_Read(s, back + got, 5 - got);
            if (r > 0)
                got += r;
        }
        CHECK(got == 5 && memcmp(hello, back, 5) == 0, "shim-ttydev-stock", "(the layer's poll and read did not return the five echoed bytes)");
        CHECK(SystemIoPortsNative_TermiosAvailableBytes(s, 1) == 0, "shim-ttydev-stock", "(the layer counts pending input on the drained port)");
        int32_t sig = SystemIoPortsNative_TermiosGetAllSignals(s);
        CHECK(sig >= 0 && (sig & (TD_SIG_CTS | TD_SIG_DSR | TD_SIG_DCD)) == (TD_SIG_CTS | TD_SIG_DSR | TD_SIG_DCD), "shim-ttydev-stock", "(the layer does not read CTS, DSR and DCD)");
        CHECK(SystemIoPortsNative_TermiosSendBreak(s, 0) == 0, "shim-ttydev-stock", "(the layer's break refused)");
        CHECK(SystemIoPortsNative_Shutdown(s, 2) != 0, "shim-ttydev-stock", "(the layer's shutdown of a port answered success)");
        CHECK(SystemIoPortsNative_SerialPortClose(s) == 0, "shim-ttydev-stock", "(the layer's close refused)");
        pass("shim-ttydev-stock", " (the stock serial layer through its own entry points: open, the reset to 115200 8N1, the speed read, hardware flow control refused, five bytes written and read back through its poll, no pending input, the signals, the break, shutdown refused on a port, close)");
    }

    out("SHIMTEST-TTYDEV: ALL GOOD (the serial device files from a native caller's side)");
    return 0;
}

#endif /* RHU_SHIMTEST_LADDER_TTYDEV */

#ifdef RHU_SHIMTEST_LADDER_FROZEN
/* The frozen object heap on a small board's row, linked with that row's
 * shim objects: a pool of 16 KB under the runtime's reservation of 4 MB
 * and its commits of 64 KB, and a row that keeps 24 KB of system memory
 * for later draws (the pool and 8 KB left unallocated). */
#define FROZEN_POOL 16384u
#define FROZEN_KEEP 24576u
#define FROZEN_RESERVE (4u * 1024u * 1024u)
#define FROZEN_COMMIT (64u * 1024u)
#define FROZEN_RW (RHU_PAL_PROT_READ | RHU_PAL_PROT_WRITE)

/* The answers for the first commit's span and the page after it: the
 * pool's four pages backed, the thirteen pages past the pool not. Returns
 * 0 when they are so, 1 when a page inside the pool is not backed, 2
 * when a page past it is, 3 when the question was refused. */
static int frozen_backing(uint8_t *base)
{
    unsigned char backed[17];
    if (mincore(base, FROZEN_COMMIT + 4096u, backed) != 0)
        return 3;
    for (int i = 0; i < 4; i++)
        if (backed[i] != 1)
            return 1;
    for (int i = 4; i < 17; i++)
        if (backed[i] != 0)
            return 2;
    return 0;
}

static int frozen_ladder(void)
{
    uint32_t free_at_start = rhu_arena_headroom();
    /* The managed heap's window before the shim initializes, by the
     * hosting layer's rule: the hard-limit row is checked against it. */
    rhu_ram_desc heap_w;
    int heap_ext = heap_window_read(&heap_w);

    out("shimtest-frozen: the frozen object heap on a small board's pool");

    CHECK(rhu_shim_init() == 0, "shim-frozen-init", "(initialization refused)");
    CHECK(rhu_arena_headroom() == free_at_start, "shim-frozen-init", "(the initialization drew system memory)");
    pass("shim-frozen-init", " (per-thread block registered; no system memory drawn)");

    CHECK(hard_limit_row() == (heap_ext ? heap_w.free & ~4095u : (free_at_start - FROZEN_KEEP) & ~4095u), "shim-frozen-env", "(the hard limit is not the managed heap window's free memory at the start, less what the row keeps from the internal window)");
    pass("shim-frozen-env", " (the hard limit is the managed heap window's free memory at the start, less the 24 KB the row keeps when that window is the internal one)");

    /* An ordinary mapping first: its page is memory, and it leaves the
     * arena's next draw on a page boundary, so the reservation's cost
     * below is exact. */
    unsigned char one = 0;
    uint8_t *plain = (uint8_t *)mmap64(NULL, 4096, 3, 0x22, -1, 0);
    CHECK(plain != (uint8_t *)-1 && mincore(plain, 4096, &one) == 0 && one == 1, "shim-frozen-plain", "(an ordinary mapping's page is not answered as backed)");
    pass("shim-frozen-plain", " (an ordinary mapping's page is answered as backed)");

    uint32_t before = rhu_arena_headroom();
    uint8_t *base = (uint8_t *)SystemNative_MMap(NULL, FROZEN_RESERVE, RHU_PAL_PROT_NONE, RHU_PAL_MAP_PRIVATE | RHU_PAL_MAP_ANONYMOUS, -1, 0);
    CHECK(base != NULL, "shim-frozen-reserve", "(the reservation was refused)");
    CHECK(before - rhu_arena_headroom() == FROZEN_POOL, "shim-frozen-reserve", "(the reservation did not draw exactly the row's pool)");
    pass("shim-frozen-reserve", " (a reservation of 4 MB drew the row's pool of 16 KB and nothing more)");

    int answer = frozen_backing(base);
    CHECK(answer != 3, "shim-frozen-backing", "(the question of what memory backs the reservation was refused)");
    CHECK(answer != 1, "shim-frozen-backing", "(a page inside the pool was not answered as backed)");
    CHECK(answer != 2, "shim-frozen-backing", "(a page past the pool was answered as backed)");
    pass("shim-frozen-backing", " (the pool's four pages answered as backed; the twelve past it in the first commit's span and the page after that span not)");

    /* The runtime's first commit covers 64 KB from the base: it starts
     * inside the pool and is accepted, and it backs nothing new. */
    volatile uint8_t *pool = base;
    CHECK(SystemNative_MProtect(base, FROZEN_COMMIT, FROZEN_RW) == 0, "shim-frozen-commit", "(the runtime's first commit, which starts inside the pool, was refused)");
    CHECK(SystemNative_MProtect(base + 8192u, FROZEN_COMMIT, FROZEN_RW) == 0, "shim-frozen-commit", "(a commit that starts inside the pool off its base was refused)");
    CHECK(frozen_backing(base) == 0, "shim-frozen-commit", "(a commit changed what memory backs the reservation)");
    CHECK(pool[0] == 0 && pool[FROZEN_POOL - 1u] == 0, "shim-frozen-commit", "(the pool is not zero-born)");
    pool[0] = 0x5A;
    pool[FROZEN_POOL - 1u] = 0xA5;
    CHECK(pool[0] == 0x5A && pool[FROZEN_POOL - 1u] == 0xA5, "shim-frozen-commit", "(the pool's first and last bytes do not hold what was written)");
    pass("shim-frozen-commit", " (a commit that starts inside the pool accepted; the answers unchanged; the pool's first and last bytes hold what is written)");

    CHECK(SystemNative_MProtect(base + FROZEN_POOL, FROZEN_COMMIT, FROZEN_RW) == -1 && SystemNative_GetErrNo() == RHU_GERR_ENOMEM, "shim-frozen-refusals", "(a commit that starts at the pool's end was not refused)");
    CHECK(SystemNative_MProtect(base + FROZEN_COMMIT, FROZEN_COMMIT, FROZEN_RW) == -1 && SystemNative_GetErrNo() == RHU_GERR_ENOMEM, "shim-frozen-refusals", "(the runtime's second commit was not refused)");
    CHECK(SystemNative_MMap(NULL, FROZEN_RESERVE, RHU_PAL_PROT_NONE, RHU_PAL_MAP_PRIVATE | RHU_PAL_MAP_ANONYMOUS, -1, 0) == NULL, "shim-frozen-refusals", "(a second reservation was not refused)");
    CHECK(before - rhu_arena_headroom() == FROZEN_POOL, "shim-frozen-refusals", "(a refusal drew system memory)");
    pass("shim-frozen-refusals", " (a commit that starts at or past the pool's end refused with no memory; a second reservation NULL; nothing more drawn)");

    CHECK(mincore(base + 1, 4096, &one) == -1 && *__errno_location() == RHU_GERR_EINVAL, "shim-frozen-arguments", "(an address off a page boundary was answered)");
    CHECK(mincore(base, 4096, NULL) == -1 && *__errno_location() == RHU_GERR_EFAULT, "shim-frozen-arguments", "(a missing answer was accepted)");
    pass("shim-frozen-arguments", " (an address off a page boundary and a missing answer refused, each with its errno)");

    out("SHIMTEST-FROZEN: ALL GOOD (the frozen object heap on a small board's pool)");
    return 0;
}
#endif /* RHU_SHIMTEST_LADDER_FROZEN */

/* The managed entry, spelled as the runtime's own bootstrapper declares
 * it: the kernel's default entry calls it in every ladder but the host's,
 * where the host does. */
int __managed__Main(int argc, char *argv[])
{
    (void)argv;
#if defined(RHU_SHIMTEST_LADDER_CORE)
    (void)argc;
    return core_ladder();
#elif defined(RHU_SHIMTEST_LADDER_FDMAP)
    (void)argc;
    return fdmap_ladder();
#elif defined(RHU_SHIMTEST_LADDER_ACTIVATION)
    (void)argc;
    return activation_ladder();
#elif defined(RHU_SHIMTEST_LADDER_TLS)
    (void)argc;
    return tls_ladder();
#elif defined(RHU_SHIMTEST_LADDER_BUSDEV)
    (void)argc;
    return busdev_ladder();
#elif defined(RHU_SHIMTEST_LADDER_GPIOD)
    (void)argc;
    return gpiod_ladder();
#elif defined(RHU_SHIMTEST_LADDER_PWMFS)
    (void)argc;
    return pwmfs_ladder();
#elif defined(RHU_SHIMTEST_LADDER_IIOFS)
    (void)argc;
    return iiofs_ladder();
#elif defined(RHU_SHIMTEST_LADDER_TTYDEV)
    (void)argc;
    return ttydev_ladder();
#elif defined(RHU_SHIMTEST_LADDER_FROZEN)
    (void)argc;
    return frozen_ladder();
#else
    return host_checks(argc);
#endif
}
