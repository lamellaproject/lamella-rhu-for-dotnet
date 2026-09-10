/* The shim gate's tenant: a C program hosted exactly like a managed
 * payload (the payload board map, the kernel's managed entry sequence,
 * the machine's exit code), exercising the shim's C-runtime surface and
 * the PAL from the caller's side. Five ladders, one per build:
 *
 *   RHU_SHIMTEST_LADDER_CORE        the glibc personality and the PAL's
 *                                   threading, memory, time and random verbs
 *   RHU_SHIMTEST_LADDER_FDMAP       the descriptor bands and the file surface
 *   RHU_SHIMTEST_LADDER_ACTIVATION  the activation costume (the signal
 *                                   surface over the kernel's delivery) and
 *                                   the PAL's signal rows
 *   RHU_SHIMTEST_LADDER_TLS         the image's thread-local template: the
 *                                   kernel's per-thread copy of module 0
 *   RHU_SHIMTEST_LADDER_HOST        the initialization contract the host runs
 *                                   before managed code, in the host's own
 *                                   entry: its order, the module handle,
 *                                   the table by the runtime's export names
 *
 * Every check prints one PASS line; the first failed check prints one
 * FAIL line and ends the run with exit 1. Workers never print, no line
 * carries a timing value or an address, and the checks run in a fixed
 * order, so the payload stream is the same bytes on every run.
 *
 * Two build knobs here break one check each, so the gate can prove it
 * detects a failing tenant:
 *   RHU_SHIMTEST_BREAK_ERRNO   a worker writes through the main thread's
 *                              errno pointer (the isolation check fails)
 *   RHU_SHIMTEST_BREAK_CANARY  a block's header is corrupted before it is
 *                              freed (the allocator dies loud, exit 134)
 * and the shim objects' own knobs place the file band at the console's
 * base (the descriptor ladder refuses it), name a foreign process in the
 * activation's signal information, fill the context's program counter
 * from the link register (the activation ladder refuses both), drop the
 * offset from a thread-local access, and hand every thread the first
 * block seen (the thread-local ladder refuses both). */
#include <stddef.h>
#include <stdint.h>

#include "rhu_shim.h"

#include "rhu.h"
#include "rhu_glibc_arm32.h"
#include "rhu_pal_errno_net11_rc1.h"
#include "rhu_pal_io_net11_rc1.h"
#include "rhu_pal_memory_net11_rc1.h"
#include "rhu_pal_random_net11_rc1.h"
#include "rhu_pal_signal_net11_rc1.h"
#include "rhu_pal_threading_net11_rc1.h"
#include "rhu_pal_time_net11_rc1.h"

#if !defined(RHU_SHIMTEST_LADDER_CORE) && !defined(RHU_SHIMTEST_LADDER_FDMAP) \
    && !defined(RHU_SHIMTEST_LADDER_ACTIVATION) && !defined(RHU_SHIMTEST_LADDER_TLS) \
    && !defined(RHU_SHIMTEST_LADDER_HOST)
#error "a tenant build names its ladder (RHU_SHIMTEST_LADDER_CORE, RHU_SHIMTEST_LADDER_FDMAP, RHU_SHIMTEST_LADDER_ACTIVATION, RHU_SHIMTEST_LADDER_TLS or RHU_SHIMTEST_LADDER_HOST)"
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
long sysconf(int name);
int getpid(void);
long syscall(long number, ...);
int getentropy(void *buf, size_t len);
long getrandom(void *buf, size_t len, unsigned flags);
int fork(void);
int open(const char *path, int flags, ...);
int close(int fd);
int dup2(int oldfd, int newfd);
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

/* --- output: unprefixed payload lines, one console write each --- */
static void out(const char *s)
{
    rhu_console_write(s, (uint32_t)strlen(s));
    rhu_console_write("\n", 1u);
}

static void out2(const char *a, const char *b, const char *c)
{
    rhu_console_write(a, (uint32_t)strlen(a));
    rhu_console_write(b, (uint32_t)strlen(b));
    rhu_console_write(c, (uint32_t)strlen(c));
    rhu_console_write("\n", 1u);
}

static void pass(const char *name, const char *note)
{
    out2("PASS ", name, note);
}

static int fail(const char *name, const char *detail)
{
    rhu_console_write("FAIL ", 5u);
    rhu_console_write(name, (uint32_t)strlen(name));
    rhu_console_write(" ", 1u);
    out(detail);
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

/* Named positively: only the ladders that measure elapsed time carry
 * this, and a ladder added later must say so rather than inherit it. */
#if defined(RHU_SHIMTEST_LADDER_CORE) || defined(RHU_SHIMTEST_LADDER_FDMAP)
static uint64_t now_ns(void)
{
    struct ts64 t;
    __clock_gettime64(1, &t);
    return (uint64_t)t.sec * 1000000000ull + (uint64_t)t.nsec;
}
#endif

#if defined(RHU_SHIMTEST_LADDER_CORE) || defined(RHU_SHIMTEST_LADDER_ACTIVATION) || defined(RHU_SHIMTEST_LADDER_FDMAP) \
    || defined(RHU_SHIMTEST_LADDER_TLS)

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

static int spawn(uint32_t *tid, void *(*fn)(void *), void *arg, uint32_t stack)
{
    attr_slot at;
    pthread_attr_init(&at);
    if (stack != 0)
        pthread_attr_setstacksize(&at, stack);
    return pthread_create(tid, stack != 0 ? (const void *)&at : NULL, fn, arg);
}

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

static int phdr_cb(dl_phdr_info *info, size_t size, void *data)
{
    (void)size;
    (void)data;
    uintptr_t me = (uintptr_t)phdr_cb & ~1u;
    for (int i = 0; i < info->dlpi_phnum; i++) {
        const elf32_phdr *p = &info->dlpi_phdr[i];
        if (p->p_type == 0x70000001u)
            g_phdr_exidx = 1;
        if (p->p_type == 1u && me >= p->p_vaddr && me < p->p_vaddr + p->p_memsz)
            g_phdr_load = 1;
    }
    return 0;
}

static int core_ladder(void)
{
    uint32_t t1;
    uint32_t t2;

    out("shimtest: the glibc personality and the PAL on the kernel's hosting doors");

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
        SystemNative_SetErrNo(RHU_PAL_EAGAIN);
        CHECK(*__errno_location() == RHU_GERR_EAGAIN && SystemNative_GetErrNo() == RHU_PAL_EAGAIN, "shim-errno-pal", "(SetErrNo/GetErrNo did not round-trip)");
        char msg[32];
        const char *m = SystemNative_StrErrorR(RHU_GERR_EAGAIN, msg, (int32_t)sizeof msg);
        CHECK(m != NULL && strcmp(m, "EAGAIN (11)") == 0, "shim-errno-pal", "(StrErrorR did not name the errno)");
        pass("shim-errno-pal", " (80 pairs round-trip; unknown values map to the nonstandard code and back to EINVAL)");
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
        CHECK(SystemNative_MProtect(base + 64u * 1024u, 64u * 1024u, RHU_PAL_PROT_READ | RHU_PAL_PROT_WRITE) == -1 && SystemNative_GetErrNo() == RHU_PAL_ENOMEM, "shim-foh", "(a commit beyond the pool was not refused)");
        CHECK(SystemNative_MMap(NULL, 4u * 1024u * 1024u, RHU_PAL_PROT_NONE, RHU_PAL_MAP_PRIVATE | RHU_PAL_MAP_ANONYMOUS, -1, 0) == NULL, "shim-foh", "(a second reservation was not refused)");
        CHECK(SystemNative_MUnmap(base, 4u * 1024u * 1024u) == 0, "shim-foh", "(the release was refused)");
        pass("shim-foh", " (the reservation served from the row's pool; a commit beyond it refused; a second reservation NULL)");
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
        int64_t secs = __time64(NULL);
        CHECK(secs >= 0 && (uint64_t)secs <= now_ns() / 1000000000ull, "shim-time", "(seconds since boot inconsistent)");
        pass("shim-time", " (monotonic clock; a relative sleep honors its floor; an absolute sleep honors its deadline)");
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

    /* entropy */
    {
        uint8_t buf[16];
        CHECK(getentropy(buf, sizeof buf) == 0, "shim-entropy", "(getentropy refused)");
        CHECK(getrandom(buf, sizeof buf, 0) == (long)sizeof buf, "shim-entropy", "(getrandom short)");
        CHECK(SystemNative_GetCryptographicallySecureRandomBytes(buf, (int32_t)sizeof buf) == 0, "shim-entropy", "(the PAL draw refused)");
        SystemNative_GetNonCryptographicallySecureRandomBytes(buf, (int32_t)sizeof buf);
        pass("shim-entropy", " (every draw all-or-nothing; the source declared on the wire)");
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

    /* the program-header walk */
    dl_iterate_phdr(phdr_cb, NULL);
    CHECK(g_phdr_load == 1 && g_phdr_exidx == 1, "shim-dlphdr", "(this code is not inside a load segment, or no exception index)");
    pass("shim-dlphdr", " (a load segment spans this code; the exception index segment present)");

    /* the environment table */
    {
        const char *v = getenv("DOTNET_gcServer");
        CHECK(v != NULL && strcmp(v, "0") == 0, "shim-env", "(gcServer)");
        v = getenv("DOTNET_gcConcurrent");
        CHECK(v != NULL && strcmp(v, "0") == 0, "shim-env", "(gcConcurrent)");
        v = getenv("HOME");
        CHECK(v != NULL && strcmp(v, "/tmp") == 0, "shim-env", "(HOME)");
        v = getenv("DOTNET_GCHeapHardLimit");
        CHECK(v != NULL && strcmp(v, "0x200000") == 0, "shim-env", "(the hard limit is not the row's region)");
        CHECK(getenv("NO_SUCH_ROW") == NULL, "shim-env", "(a missing row was answered)");
        pass("shim-env", " (the four rows; the hard limit generated from the row; a miss NULL)");
    }

    /* the small identities */
    {
        CHECK(getpid() == 1, "shim-misc", "(pid)");
        CHECK(sysconf(30) == 4096 && sysconf(84) == 1 && sysconf(86) > 0 && sysconf(85) >= sysconf(86), "shim-misc", "(sysconf)");
        CHECK(SystemNative_SysConf(RHU_PAL_SC_PAGESIZE) == 4096 && SystemNative_SysConf(RHU_PAL_SC_CLK_TCK) == 100, "shim-misc", "(PAL SysConf)");
        int64_t a = SystemNative_GetTimestamp();
        msleep(2);
        CHECK(SystemNative_GetTimestamp() > a, "shim-misc", "(the timestamp did not advance)");
        CHECK(SystemNative_GetSystemTimeAsTicks() > 0 && SystemNative_GetLowResolutionTimestamp() >= 0, "shim-misc", "(the clocks)");
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
        CHECK(SystemNative_Pipe(fds, 0) == -1 && SystemNative_GetErrNo() == RHU_PAL_ENOSYS, "shim-honest", "(pipe)");
        pass("shim-honest", " (fork, an unknown syscall, a path, a foreign descriptor, a pipe: each refused with its errno)");
    }

    out("SHIMTEST: ALL GOOD (glibc personality + PAL on the kernel doors)");
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
    CHECK(SystemNative_FStat(3, &st) == -1 && SystemNative_GetErrNo() == RHU_PAL_EBADF, "shim-fdmap-console", "(an unopened descriptor answered)");
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
        CHECK(SystemNative_Write(fd, "x", 1) == -1 && SystemNative_GetErrNo() == RHU_PAL_EBADF, "shim-fdmap-reopen", "(a write to a read-only descriptor succeeded)");
        CHECK(SystemNative_Close(fd) == 0, "shim-fdmap-reopen", "(close refused)");
        pass("shim-fdmap-reopen", " (the descriptor reused; reads follow the position; end of file 0; read-only refuses writes)");
    }

    /* a closed descriptor is invalid, never a console */
    CHECK(SystemNative_FStat(3, &st) == -1 && SystemNative_GetErrNo() == RHU_PAL_EBADF, "shim-fdmap-closed", "(a closed descriptor answered)");
    CHECK(SystemNative_Read(3, buf, 4) == -1 && SystemNative_GetErrNo() == RHU_PAL_EBADF, "shim-fdmap-closed", "(a closed descriptor read)");
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
        CHECK(SystemNative_Stat("/tmp/a.txt", &st) == -1 && SystemNative_GetErrNo() == RHU_PAL_ENOENT, "shim-fdmap-paths", "(the old name survives)");
        CHECK(SystemNative_Unlink("/tmp/b.txt") == 0, "shim-fdmap-paths", "(unlink)");
        CHECK(SystemNative_Stat("/tmp/b.txt", &st) == -1 && SystemNative_GetErrNo() == RHU_PAL_ENOENT, "shim-fdmap-paths", "(the file survives its unlink)");
        CHECK(SystemNative_Unlink("/tmp/b.txt") == -1 && SystemNative_GetErrNo() == RHU_PAL_ENOENT, "shim-fdmap-paths", "(a second unlink succeeded)");
        pass("shim-fdmap-paths", " (stat, access, rename, unlink; a missing path ENOENT)");
    }

    /* directories */
    {
        CHECK(SystemNative_MkDir("/tmp/d", 0x1ED) == 0, "shim-fdmap-dirs", "(mkdir)");
        CHECK(SystemNative_MkDir("/tmp/d", 0x1ED) == -1 && SystemNative_GetErrNo() == RHU_PAL_EEXIST, "shim-fdmap-dirs", "(a second mkdir succeeded)");
        intptr_t fx = SystemNative_Open("/tmp/d/x.txt", RHU_PAL_O_WRONLY | RHU_PAL_O_CREAT, 0x1A4);
        intptr_t fy = SystemNative_Open("/tmp/d/y.txt", RHU_PAL_O_WRONLY | RHU_PAL_O_CREAT, 0x1A4);
        CHECK(fx >= 0 && fy >= 0 && fx != fy, "shim-fdmap-dirs", "(files in a directory)");
        SystemNative_Close(fx);
        SystemNative_Close(fy);
        CHECK(SystemNative_MkDir("/tmp/d/sub", 0x1ED) == 0, "shim-fdmap-dirs", "(a nested directory)");
        CHECK(SystemNative_Open("/tmp/nodir/z.txt", RHU_PAL_O_WRONLY | RHU_PAL_O_CREAT, 0x1A4) == -1 && SystemNative_GetErrNo() == RHU_PAL_ENOENT, "shim-fdmap-dirs", "(a file under a missing directory was made)");
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
        CHECK(SystemNative_RmDir("/tmp/d") == -1 && SystemNative_GetErrNo() == RHU_PAL_ENOTEMPTY, "shim-fdmap-dirs", "(a non-empty directory was removed)");
        CHECK(SystemNative_Unlink("/tmp/d/x.txt") == 0 && SystemNative_Unlink("/tmp/d/y.txt") == 0, "shim-fdmap-dirs", "(unlink)");
        CHECK(SystemNative_RmDir("/tmp/d/sub") == 0 && SystemNative_RmDir("/tmp/d") == 0, "shim-fdmap-dirs", "(rmdir)");
        CHECK(SystemNative_OpenDir("/tmp/d") == NULL && SystemNative_GetErrNo() == RHU_PAL_ENOENT, "shim-fdmap-dirs", "(a removed directory opens)");
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

    /* file times */
    {
        rhu_pal_timespec times[2] = {{1000, 5}, {2000, 7}};
        CHECK(SystemNative_UTimensat("/tmp/v.txt", times) == 0, "shim-fdmap-times", "(utimensat)");
        CHECK(SystemNative_Stat("/tmp/v.txt", &st) == 0 && st.ATime == 1000 && st.ATimeNsec == 5 && st.MTime == 2000 && st.MTimeNsec == 7, "shim-fdmap-times", "(the times did not stick)");
        pass("shim-fdmap-times", " (access and modification times set and read back)");
    }

    /* the file surface's refusals, each with its errno */
    {
        intptr_t fd = SystemNative_Open("/tmp/v.txt", RHU_PAL_O_RDONLY, 0);
        CHECK(fd >= 0, "shim-fdmap-refusals", "(open)");
        CHECK(SystemNative_Dup(fd) == -1 && SystemNative_GetErrNo() == RHU_PAL_ENOTSUP, "shim-fdmap-refusals", "(dup of a file succeeded)");
        CHECK(SystemNative_Dup(1) == 1, "shim-fdmap-refusals", "(dup of the console)");
        int32_t fds[2];
        CHECK(SystemNative_Pipe(fds, 0) == -1 && SystemNative_GetErrNo() == RHU_PAL_ENOSYS, "shim-fdmap-refusals", "(pipe)");
        CHECK(SystemNative_IsMemfdSupported() == 0, "shim-fdmap-refusals", "(memfd claimed)");
        CHECK(SystemNative_MemfdCreate("m", 0) == -1 && SystemNative_GetErrNo() == RHU_PAL_ENOTSUP, "shim-fdmap-refusals", "(memfd)");
        CHECK(SystemNative_ShmOpen("/s", 0, 0) == -1 && SystemNative_GetErrNo() == RHU_PAL_ENOTSUP, "shim-fdmap-refusals", "(shm open)");
        CHECK(SystemNative_ShmUnlink("/s") == -1 && SystemNative_GetErrNo() == RHU_PAL_ENOENT, "shim-fdmap-refusals", "(shm unlink)");
        CHECK(SystemNative_Close(fd) == 0, "shim-fdmap-refusals", "(close)");
        CHECK(SystemNative_Close(fd) == -1 && SystemNative_GetErrNo() == RHU_PAL_EBADF, "shim-fdmap-refusals", "(a second close succeeded)");
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
        SCHECK(SystemNative_Stat("/tmp/life-a.txt", &st) == -1 && SystemNative_GetErrNo() == RHU_PAL_ENOENT, "shim-fdmap-lifetime", "(the unlinked name still resolves)");
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
        SCHECK(SystemNative_Open("/tmp/order.txt", RHU_PAL_O_WRONLY | RHU_PAL_O_TRUNC, 0) == -1 && SystemNative_GetErrNo() == RHU_PAL_EMFILE, "shim-fdmap-open-order", "(an open past the band succeeded)");
        SCHECK(SystemNative_Stat("/tmp/order.txt", &st) == 0 && st.Size == 4, "shim-fdmap-open-order", "(a refused open truncated the file)");
        SCHECK(SystemNative_Open("/tmp/order-new.txt", RHU_PAL_O_WRONLY | RHU_PAL_O_CREAT, 0x1A4) == -1 && SystemNative_GetErrNo() == RHU_PAL_EMFILE, "shim-fdmap-open-order", "(an open with create past the band succeeded)");
        SCHECK(SystemNative_Stat("/tmp/order-new.txt", &st) == -1 && SystemNative_GetErrNo() == RHU_PAL_ENOENT, "shim-fdmap-open-order", "(a refused open created the file)");
        for (int i = 0; i < n; i++)
            SystemNative_Close(held[i]);
        SCHECK(SystemNative_Unlink("/tmp/order.txt") == 0, "shim-fdmap-open-order", "(unlink)");
        pass("shim-fdmap-open-order", " (with the descriptor band full an open is refused with EMFILE before it truncates or creates)");
    }

    /* the access mode at the read boundary */
    {
        intptr_t fd = SystemNative_Open("/tmp/wo.txt", RHU_PAL_O_WRONLY | RHU_PAL_O_CREAT | RHU_PAL_O_TRUNC, 0x1A4);
        SCHECK(fd >= 0 && SystemNative_Write(fd, "WO", 2) == 2, "shim-fdmap-access", "(open write-only, write)");
        SCHECK(SystemNative_Read(fd, buf, 2) == -1 && SystemNative_GetErrNo() == RHU_PAL_EBADF, "shim-fdmap-access", "(a write-only descriptor read)");
        SCHECK(SystemNative_PRead(fd, buf, 2, 0) == -1 && SystemNative_GetErrNo() == RHU_PAL_EBADF, "shim-fdmap-access", "(a write-only descriptor read positionally)");
        SCHECK(SystemNative_Close(fd) == 0 && SystemNative_Unlink("/tmp/wo.txt") == 0, "shim-fdmap-access", "(close, unlink)");
        pass("shim-fdmap-access", " (a write-only descriptor refuses reads with EBADF, as a read-only one refuses writes)");
    }

    /* advisory locks per object, held per descriptor */
    {
        intptr_t f1 = SystemNative_Open("/tmp/lock.txt", RHU_PAL_O_RDWR | RHU_PAL_O_CREAT | RHU_PAL_O_TRUNC, 0x1A4);
        intptr_t f2 = SystemNative_Open("/tmp/lock.txt", RHU_PAL_O_RDWR, 0);
        SCHECK(f1 >= 0 && f2 >= 0, "shim-fdmap-flock", "(two descriptors)");
        SCHECK(SystemNative_FLock(f1, RHU_PAL_LOCK_EX | RHU_PAL_LOCK_NB) == 0, "shim-fdmap-flock", "(an exclusive lock refused)");
        SCHECK(SystemNative_FLock(f2, RHU_PAL_LOCK_EX | RHU_PAL_LOCK_NB) == -1 && SystemNative_GetErrNo() == RHU_PAL_EWOULDBLOCK, "shim-fdmap-flock", "(a second exclusive lock succeeded)");
        SCHECK(SystemNative_FLock(f2, RHU_PAL_LOCK_SH | RHU_PAL_LOCK_NB) == -1 && SystemNative_GetErrNo() == RHU_PAL_EWOULDBLOCK, "shim-fdmap-flock", "(a shared lock under an exclusive one succeeded)");
        SCHECK(SystemNative_FLock(f1, RHU_PAL_LOCK_UN) == 0, "shim-fdmap-flock", "(unlock refused)");
        SCHECK(SystemNative_FLock(f2, RHU_PAL_LOCK_SH | RHU_PAL_LOCK_NB) == 0 && SystemNative_FLock(f1, RHU_PAL_LOCK_SH | RHU_PAL_LOCK_NB) == 0, "shim-fdmap-flock", "(shared locks do not coexist)");
        SCHECK(SystemNative_FLock(f1, RHU_PAL_LOCK_EX | RHU_PAL_LOCK_NB) == -1 && SystemNative_GetErrNo() == RHU_PAL_EWOULDBLOCK, "shim-fdmap-flock", "(an exclusive lock over another's shared one succeeded)");
        SCHECK(SystemNative_Close(f2) == 0 && SystemNative_FLock(f1, RHU_PAL_LOCK_EX | RHU_PAL_LOCK_NB) == 0, "shim-fdmap-flock", "(a close did not release its lock)");
        SCHECK(SystemNative_FLock(f1, 16) == -1 && SystemNative_GetErrNo() == RHU_PAL_EINVAL, "shim-fdmap-flock", "(an unknown operation accepted)");
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
        SCHECK(SystemNative_FUTimens(fd, bad) == -1 && SystemNative_GetErrNo() == RHU_PAL_EINVAL, "shim-fdmap-times-range", "(a nanosecond field of one second accepted)");
        rhu_pal_timespec neg[2] = {{1000, 5}, {2000, -1}};
        SCHECK(SystemNative_UTimensat("/tmp/t.txt", neg) == -1 && SystemNative_GetErrNo() == RHU_PAL_EINVAL, "shim-fdmap-times-range", "(a negative nanosecond field accepted)");
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
        SCHECK(SystemNative_FTruncate(fd, 2147483649ll) == -1 && SystemNative_GetErrNo() == RHU_PAL_EFBIG, "shim-fdmap-bounds", "(a truncation past the maximum was accepted)");
#endif
        SCHECK(SystemNative_PWrite(fd, "XYZW", 4, 4294967296ll) == -1 && SystemNative_GetErrNo() == RHU_PAL_EFBIG, "shim-fdmap-bounds", "(a write at 4 GiB was not refused with EFBIG)");
        SCHECK(SystemNative_PRead(fd, buf, 4, 0) == 4 && memcmp(buf, "0123", 4) == 0, "shim-fdmap-bounds", "(a write at 4 GiB reached byte 0)");
        SCHECK(SystemNative_PWrite(fd, "XYZW", 4, -4294967296ll) == -1 && SystemNative_GetErrNo() == RHU_PAL_EINVAL, "shim-fdmap-bounds", "(a negative write offset was not refused)");
        SCHECK(SystemNative_PRead(fd, buf, 4, -1) == -1 && SystemNative_GetErrNo() == RHU_PAL_EINVAL, "shim-fdmap-bounds", "(a negative read offset was not refused)");
        SCHECK(SystemNative_FTruncate(fd, -1) == -1 && SystemNative_GetErrNo() == RHU_PAL_EINVAL, "shim-fdmap-bounds", "(a negative truncation was accepted)");
        SCHECK(SystemNative_LSeek(fd, 1048576, RHU_PAL_SEEK_SET) == 1048576, "shim-fdmap-bounds", "(a seek to the maximum was refused)");
        SCHECK(SystemNative_LSeek(fd, 0xFFFFFFFFll, RHU_PAL_SEEK_SET) == -1 && SystemNative_GetErrNo() == RHU_PAL_EINVAL, "shim-fdmap-bounds", "(a seek past the maximum was accepted)");
        SCHECK(SystemNative_Write(fd, "x", 1) == -1 && SystemNative_GetErrNo() == RHU_PAL_EFBIG, "shim-fdmap-bounds", "(a write past the maximum was accepted)");
        SCHECK(SystemNative_FStat(fd, &st) == 0 && st.Size == 4, "shim-fdmap-bounds", "(the size moved on a refused write)");
        SCHECK(SystemNative_Close(fd) == 0 && SystemNative_Unlink("/tmp/bounds.txt") == 0, "shim-fdmap-bounds", "(close, unlink)");
        pass("shim-fdmap-bounds", " (offsets, counts and lengths checked in 64 bits against the stated maximum: EFBIG past it, EINVAL below zero, the bytes and the size untouched)");
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

static int activation_ladder(void)
{
    uint32_t t1;
    uint32_t base_count;
    rhu_glibc_sigset s;
    rhu_glibc_sigaction act;
    rhu_glibc_sigaction old;
    uint32_t spin_lo = (uint32_t)(uintptr_t)rhu_act_spin_lo;
    uint32_t spin_hi = (uint32_t)(uintptr_t)rhu_act_spin_hi;

    out("shimtest-activation: the activation costume on the kernel's delivery, and the signal rows");

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

    out("SHIMTEST-ACTIVATION: ALL GOOD (the activation costume on the kernel's delivery; the signal rows recorded, none deliverable)");
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
#include "rhu_rt_entry_net11_rc1.h"

_Static_assert(RHU_RT_CLASSLIB_SLOTS > RHU_RT_CLASSLIB_SLOT_ResolveDispatch, "the table holds the dispatch slot");
_Static_assert(RHU_RT_CLASSLIB_EXPORTS == 8, "eight slots carry an export at this pin");

/* The in-image header the kernel states: the module handle the host passes
 * is the load address it carries. */
extern const uint32_t __rhu_image_header[16];

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

    CHECK(g_registered_module == (void *)(uintptr_t)__rhu_image_header[3], "shim-host-module", "(the module handle is not the image's stated load address)");
    CHECK(g_modules_module == g_registered_module, "shim-host-module", "(the module list was initialized under another handle)");
    CHECK(g_managed_bytes == 0u && g_unbox_bytes == 0u, "shim-host-module", "(this tenant carries no managed code, yet a range was registered)");
    CHECK(g_modules_count == 0, "shim-host-module", "(this tenant carries no module table, yet modules were counted)");
    CHECK(argc == 1, "shim-host-module", "(the argument vector does not carry the one program name)");
    pass("shim-host-module", " (the handle is the image's load address; empty ranges; no module; one argument)");

    out("SHIMTEST-HOST: ALL GOOD (the initialization contract in order; the class-library table as the pin declares it)");
    return 0;
}

#endif /* RHU_SHIMTEST_LADDER_HOST */

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
#else
    return host_checks(argc);
#endif
}
