/* The shim gate's tenant: a C program hosted exactly like a managed
 * payload (the payload board map, the kernel's managed entry sequence,
 * the machine's exit code), exercising the shim's C-runtime surface and
 * the PAL from the caller's side. Twenty-three ladders, one per build:
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
 *                                   from a native caller's side, over the kernel's PWM API
 *   RHU_SHIMTEST_LADDER_PWMROWS     the PWM device files over a board row's own line
 *                                   table, the kernel's PWM functions answered by the
 *                                   tenant (RHU_SHIMTEST_PWMROWS_F769 or
 *                                   RHU_SHIMTEST_PWMROWS_U5A5 names the row)
 *   RHU_SHIMTEST_LADDER_IIOFS       the analog input device files (/sys/bus/iio/devices)
 *   RHU_SHIMTEST_LADDER_SDFS        the card's files (/media/sd0) over a scripted kernel
 *                                   from a native caller's side, over the kernel's ADC API
 *   RHU_SHIMTEST_LADDER_TTYDEV      the serial device files (/dev/ttyS<n>) from a
 *                                   native caller's side, over the kernel's UART API,
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
 *   RHU_SHIMTEST_LADDER_SOCKADDR    the socket-address helpers from a native
 *                                   caller's side: the record sizes and every
 *                                   field of an address buffer
 *   RHU_SHIMTEST_LADDER_SOCKERRORS  the kernel's network codes as the runtime's
 *                                   errors
 *   RHU_SHIMTEST_LADDER_SOCKREFUSALS  the refused names of the socket surface,
 *                                   each read for the value its caller expects,
 *                                   and the socket band in an image that links
 *                                   no socket unit
 *   RHU_SHIMTEST_LADDER_SOCKSTREAM  a stream socket over a scripted kernel:
 *                                   connect, the data calls, the blocking
 *                                   calls and their timeouts, close
 *   RHU_SHIMTEST_LADDER_SOCKLISTEN  a listening socket over a scripted kernel:
 *                                   bind, listen, accept
 *   RHU_SHIMTEST_LADDER_SOCKDATAGRAM  a datagram socket over a scripted kernel:
 *                                   messages to and from an address
 *   RHU_SHIMTEST_LADDER_SOCKOPTIONS  the socket options over a scripted kernel
 *   RHU_SHIMTEST_LADDER_SOCKPOLL    Poll over the socket band and a file, over
 *                                   a scripted kernel
 *   RHU_SHIMTEST_LADDER_SOCKPORT    the socket event port over a scripted
 *                                   kernel: the level once, the risen bits, a
 *                                   registration and a close under a waiter
 *   RHU_SHIMTEST_LADDER_SOCKRESOLVE  name resolution over a scripted kernel
 *   RHU_SHIMTEST_LADDER_SOCKNETINFO  the network interface records over a
 *                                   scripted kernel
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
#include "rhu_pal_errno_net11.h"
#include "rhu_pal_io_net11.h"
#include "rhu_pal_memory_net11.h"
#include "rhu_pal_random_net11.h"
#include "rhu_pal_signal_net11.h"
#include "rhu_pal_threading_net11.h"
#include "rhu_pal_time_net11.h"

#if !defined(RHU_SHIMTEST_LADDER_CORE) && !defined(RHU_SHIMTEST_LADDER_FDMAP) \
    && !defined(RHU_SHIMTEST_LADDER_ACTIVATION) && !defined(RHU_SHIMTEST_LADDER_TLS) \
    && !defined(RHU_SHIMTEST_LADDER_HOST) && !defined(RHU_SHIMTEST_LADDER_BUSDEV) \
    && !defined(RHU_SHIMTEST_LADDER_GPIOD) && !defined(RHU_SHIMTEST_LADDER_FROZEN) && !defined(RHU_SHIMTEST_LADDER_PWMFS) && !defined(RHU_SHIMTEST_LADDER_PWMROWS) && !defined(RHU_SHIMTEST_LADDER_TTYDEV) && !defined(RHU_SHIMTEST_LADDER_IIOFS) && !defined(RHU_SHIMTEST_LADDER_SDFS) \
    && !defined(RHU_SHIMTEST_LADDER_SOCKADDR) && !defined(RHU_SHIMTEST_LADDER_SOCKERRORS) && !defined(RHU_SHIMTEST_LADDER_SOCKREFUSALS) \
    && !defined(RHU_SHIMTEST_LADDER_SOCKSTREAM) && !defined(RHU_SHIMTEST_LADDER_SOCKLISTEN) && !defined(RHU_SHIMTEST_LADDER_SOCKDATAGRAM) && !defined(RHU_SHIMTEST_LADDER_SOCKOPTIONS) \
    && !defined(RHU_SHIMTEST_LADDER_SOCKPOLL) && !defined(RHU_SHIMTEST_LADDER_SOCKPORT) && !defined(RHU_SHIMTEST_LADDER_SOCKRESOLVE) && !defined(RHU_SHIMTEST_LADDER_SOCKNETINFO)
#error "a tenant build names its ladder (RHU_SHIMTEST_LADDER_CORE, RHU_SHIMTEST_LADDER_FDMAP, RHU_SHIMTEST_LADDER_ACTIVATION, RHU_SHIMTEST_LADDER_TLS, RHU_SHIMTEST_LADDER_HOST, RHU_SHIMTEST_LADDER_BUSDEV, RHU_SHIMTEST_LADDER_GPIOD, RHU_SHIMTEST_LADDER_PWMFS, RHU_SHIMTEST_LADDER_PWMROWS, RHU_SHIMTEST_LADDER_IIOFS, RHU_SHIMTEST_LADDER_TTYDEV, RHU_SHIMTEST_LADDER_SDFS, RHU_SHIMTEST_LADDER_FROZEN, RHU_SHIMTEST_LADDER_SOCKADDR, RHU_SHIMTEST_LADDER_SOCKERRORS, RHU_SHIMTEST_LADDER_SOCKREFUSALS, RHU_SHIMTEST_LADDER_SOCKSTREAM, RHU_SHIMTEST_LADDER_SOCKLISTEN, RHU_SHIMTEST_LADDER_SOCKDATAGRAM, RHU_SHIMTEST_LADDER_SOCKOPTIONS, RHU_SHIMTEST_LADDER_SOCKPOLL, RHU_SHIMTEST_LADDER_SOCKPORT, RHU_SHIMTEST_LADDER_SOCKRESOLVE or RHU_SHIMTEST_LADDER_SOCKNETINFO)"
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
 * kernel function writes a call whole, and a second thread's log line can
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
/* The probe's line also carries the error number that stood when the
 * check failed, so one run over a store shows each clause's reason. */
int32_t SystemNative_GetErrNo(void);
static void probe_fail(const char *name, const char *detail)
{
    char tail[LINE_MAX];
    char dig[12];
    int32_t e = SystemNative_GetErrNo();
    uint32_t v = (uint32_t)(e < 0 ? -e : e);
    uint32_t k = sizeof dig;
    dig[--k] = 0;
    do {
        dig[--k] = (char)('0' + v % 10u);
        v /= 10u;
    } while (v != 0u);
    uint32_t n = put(tail, 0u, detail);
    n = put(tail, n, " [the error number that stood: ");
    n = put(tail, n, dig + k);
    n = put(tail, n, "]");
    tail[n] = 0;
    out_line("FAIL ", name, " ", tail);
}
#define SCHECK(cond, name, detail) \
    do {                           \
        if (!(cond))               \
            probe_fail(name, detail); \
    } while (0)
#else
#define SCHECK(cond, name, detail) CHECK(cond, name, detail)
#endif

#if defined(RHU_SHIMTEST_LADDER_CORE)
/* The cross-process mutex's entry points, declared by the surface header
 * as the pin exports them. */
#include "rhu_pal_surface_net11.h"

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

/* The thread pool's most-workers link word, read the same way: an image
 * linked without it carries no row, and one linked with it carries the
 * row DOTNET_ThreadPool_ForceMaxWorkerThreads=0x<the word in hexadecimal>,
 * the form the pool reads. */
extern const char __rhu_threadpool_max_threads[] __attribute__((weak));

/* "0x" and a value in lower-case hexadecimal into the buffer given (at
 * least eleven bytes); returns the buffer. */
static const char *hex_u32(char *buf, uint32_t v)
{
    char tmp[8];
    uint32_t n = 0;
    do {
        uint32_t d = v & 15u;
        tmp[n++] = (char)(d < 10u ? '0' + d : 'a' + (d - 10u));
        v >>= 4;
    } while (v != 0);
    uint32_t i = 0;
    buf[i++] = '0';
    buf[i++] = 'x';
    while (n != 0)
        buf[i++] = tmp[--n];
    buf[i] = 0;
    return buf;
}

/* What the hosting layer's stack-attribute call answers for a carve the
 * kernel records as (base, bytes): the upper bound kept, the lower bound
 * 65536 bytes less the reserve of 6144 under the base, so that the
 * runtime's stack-depth helper stops the reserve above the true base. */
#define STACK_TOLD_UNDER (65536u - 6144u)
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
 * filled at the call (zeroed should the range function refuse). Returns 1
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
/* A second thread BELOW the ladder's: spawned by a kernel call one
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
 * calendar function. */
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

/* The cross-process mutex's block: 64 bytes filled with a pattern, of
 * which the family owns as many as it reports; a waiter that takes the
 * mutex with no bound and releases it. */
static uint64_t g_xpm_store[8];
static volatile int g_xpm_saw;

static void *xpm_waiter(void *a)
{
    (void)a;
    LowLevelCrossProcessMutex *m = (LowLevelCrossProcessMutex *)g_xpm_store;
    if (SystemNative_LowLevelCrossProcessMutex_Acquire(m, -1) == RHU_PAL_SUCCESS) {
        g_xpm_saw = 1;
        SystemNative_LowLevelCrossProcessMutex_Release(m);
    }
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
        /* The carve is the kernel's answer; the bounds the attribute call
         * reports are the ones the runtime's stack-depth helper needs. */
        uint32_t kb = 0;
        uint32_t kl = 0;
        CHECK(rhu_thread_stack_bounds((int32_t)pthread_self(), &kb, &kl) == 0 && kb != 0 && kl >= 8u * 1024u, "shim-thread", "(own stack carve unavailable)");
        uintptr_t sp = (uintptr_t)&at;
        CHECK(sp > (uintptr_t)kb && sp <= (uintptr_t)kb + kl, "shim-thread", "(the live SP is outside the kernel's carve)");
        CHECK(pthread_attr_getstack(&at, &lo, &bytes) == 0 && (uintptr_t)lo == (uintptr_t)kb - STACK_TOLD_UNDER && (uintptr_t)lo + bytes == (uintptr_t)kb + kl, "shim-thread",
              "(the reported bounds do not begin 59392 bytes under the kernel's base with its upper bound kept)");
        CHECK(syscall(224) == (long)pthread_self(), "shim-thread", "(gettid is not self)");
        g_done = 0;
        CHECK(spawn(&t1, sleeper, NULL, 0) == 0, "shim-thread", "(spawn refused)");
        attr_slot wat;
        void *wlo = NULL;
        size_t wbytes = 0;
        pthread_getattr_np(t1, &wat);
        CHECK(rhu_thread_stack_bounds((int32_t)t1, &kb, &kl) == 0 && kl == 16u * 1024u, "shim-thread", "(a worker's default carve is not the row default)");
        CHECK(pthread_attr_getstack(&wat, &wlo, &wbytes) == 0 && (uintptr_t)wlo == (uintptr_t)kb - STACK_TOLD_UNDER && wbytes == 16u * 1024u + STACK_TOLD_UNDER, "shim-thread",
              "(a worker's default carve is not reported 59392 bytes under its base with its upper bound kept)");
        wait_done(&g_done, 1);
        g_done = 0;
        CHECK(spawn(&t1, sleeper, NULL, WORKER_STACK) == 0, "shim-thread", "(spawn of a small worker refused)");
        pthread_getattr_np(t1, &wat);
        CHECK(rhu_thread_stack_bounds((int32_t)t1, &kb, &kl) == 0 && kl == WORKER_STACK, "shim-thread", "(a small worker's carve is not the size asked)");
        CHECK(pthread_attr_getstack(&wat, &wlo, &wbytes) == 0 && (uintptr_t)wlo == (uintptr_t)kb && wbytes == WORKER_STACK, "shim-thread", "(a carve not larger than the reserve is not reported as it is)");
        wait_done(&g_done, 1);
        pass("shim-thread", " (the kernel's carve contains the live SP; the reported bounds keep the carve's upper bound and begin 59392 bytes under its base, 65536 less the reserve of 6144; gettid is self; a worker's default carve is the row default and is reported the same way; a worker's carve of 2048 bytes, not larger than the reserve, is reported as it is)");
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

    /* entropy: the kernel's two entropy functions */
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

    /* the cross-process mutex: the nine entry points over the block the
     * family sizes, driven as the class library drives them */
    {
        uint8_t *block = (uint8_t *)g_xpm_store;
        LowLevelCrossProcessMutex *m = (LowLevelCrossProcessMutex *)g_xpm_store;
        uint32_t pid = 0;
        uint64_t tid = 0;
        memset(block, 0xA5, sizeof g_xpm_store);
        int32_t size = SystemNative_LowLevelCrossProcessMutex_Size();
        CHECK(size >= 17 && size <= (int32_t)sizeof g_xpm_store - 8, "shim-crossprocess-mutex", "(the reported size does not hold a lock word, a process id, a 64-bit thread id and a flag)");
        CHECK(SystemNative_LowLevelCrossProcessMutex_Init(m) == RHU_PAL_SUCCESS && SystemNative_LowLevelCrossProcessMutex_IsAbandoned(m) == 0, "shim-crossprocess-mutex", "(a fresh block was refused or born abandoned)");
        SystemNative_LowLevelCrossProcessMutex_SetOwnerProcessAndThreadId(m, 1u, 0x0000000100000002ull);
        SystemNative_LowLevelCrossProcessMutex_GetOwnerProcessAndThreadId(m, &pid, &tid);
        CHECK(pid == 1u, "shim-crossprocess-mutex", "(the owner's process id did not come back)");
        CHECK(tid == 0x0000000100000002ull, "shim-crossprocess-mutex", "(the owner's thread id did not come back whole)");
        SystemNative_LowLevelCrossProcessMutex_SetAbandoned(m, 1);
        SystemNative_LowLevelCrossProcessMutex_GetOwnerProcessAndThreadId(m, &pid, &tid);
        CHECK(SystemNative_LowLevelCrossProcessMutex_IsAbandoned(m) == 1 && pid == 1u && tid == 0x0000000100000002ull, "shim-crossprocess-mutex", "(the abandoned flag was not kept apart from the owner)");
        SystemNative_LowLevelCrossProcessMutex_SetOwnerProcessAndThreadId(m, 0xFFFFFFFFu, 0xFFFFFFFFFFFFFFFFull);
        SystemNative_LowLevelCrossProcessMutex_GetOwnerProcessAndThreadId(m, &pid, &tid);
        CHECK(pid == 0xFFFFFFFFu && tid == 0xFFFFFFFFFFFFFFFFull && SystemNative_LowLevelCrossProcessMutex_IsAbandoned(m) == 1, "shim-crossprocess-mutex", "(the no-owner pair did not come back whole)");
        SystemNative_LowLevelCrossProcessMutex_SetAbandoned(m, 0);
        CHECK(SystemNative_LowLevelCrossProcessMutex_IsAbandoned(m) == 0, "shim-crossprocess-mutex", "(the abandoned flag was not cleared)");
        CHECK(SystemNative_LowLevelCrossProcessMutex_Acquire(m, 0) == RHU_PAL_SUCCESS, "shim-crossprocess-mutex", "(a try on a free mutex did not take it)");
        CHECK(SystemNative_LowLevelCrossProcessMutex_Acquire(m, 0) == RHU_PAL_EBUSY, "shim-crossprocess-mutex", "(a try on a held mutex was not EBUSY)");
        uint64_t t0 = now_ns();
        int32_t rc = SystemNative_LowLevelCrossProcessMutex_Acquire(m, 40);
        CHECK(rc == RHU_PAL_ETIMEDOUT && now_ns() - t0 >= 40000000ull, "shim-crossprocess-mutex", "(a bounded wait on a held mutex did not time out at its bound)");
        g_done = 0;
        g_xpm_saw = 0;
        CHECK(spawn(&t1, xpm_waiter, NULL, WORKER_STACK) == 0, "shim-crossprocess-mutex", "(spawn refused)");
        msleep(20);
        CHECK(g_xpm_saw == 0, "shim-crossprocess-mutex", "(a wait with no bound did not block on a held mutex)");
        CHECK(SystemNative_LowLevelCrossProcessMutex_Release(m) == RHU_PAL_SUCCESS, "shim-crossprocess-mutex", "(the release was refused)");
        wait_done(&g_done, 1);
        CHECK(g_xpm_saw == 1, "shim-crossprocess-mutex", "(the waiter did not take the released mutex)");
        CHECK(SystemNative_LowLevelCrossProcessMutex_Acquire(m, 0) == RHU_PAL_SUCCESS && SystemNative_LowLevelCrossProcessMutex_Release(m) == RHU_PAL_SUCCESS, "shim-crossprocess-mutex", "(the mutex was not free after the waiter's release)");
        CHECK(SystemNative_LowLevelCrossProcessMutex_Destroy(m) == RHU_PAL_SUCCESS, "shim-crossprocess-mutex", "(the destroy was refused)");
        for (uint32_t i = (uint32_t)size; i < (uint32_t)sizeof g_xpm_store; i++)
            CHECK(block[i] == 0xA5, "shim-crossprocess-mutex", "(the family wrote past the size it reports)");
        pass("shim-crossprocess-mutex", " (the nine entry points over the block the family sizes: a 64-bit owner thread id read back whole beside the process id; the abandoned flag kept apart; a held mutex busy for a try, timed out for a bounded wait and waited for with no bound until its release)");
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
        /* The thread pool's most-workers row: absent when the image
         * carries no link word, 0x and the word in hexadecimal when it
         * does (the pool reads this row as hexadecimal). */
        char most[12];
        v = getenv("DOTNET_ThreadPool_ForceMaxWorkerThreads");
        if (__rhu_threadpool_max_threads == NULL) {
            CHECK(v == NULL, "shim-env", "(ThreadPool_ForceMaxWorkerThreads: a row stands with no link word)");
            most[0] = 0;
        } else {
            hex_u32(most, (uint32_t)(uintptr_t)__rhu_threadpool_max_threads);
            CHECK(v != NULL && strcmp(v, most) == 0, "shim-env", "(ThreadPool_ForceMaxWorkerThreads: the row is not the link word in hexadecimal)");
        }
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
            if (most[0] == 0) {
                n = put(note, n, "; the pool's most workers: no row, no link word");
            } else {
                n = put(note, n, "; the pool's most workers: the row ");
                n = put(note, n, most);
                n = put(note, n, ", the link word in hexadecimal");
            }
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

/* The mode verb, declared as the caller sees it (the store's per-area
 * header does not carry it). */
int32_t SystemNative_ChMod(const char *path, int32_t mode);
/* The descriptor forms of open, mkdir and unlink and the mode verb by
 * descriptor, declared as the caller sees them. */
intptr_t SystemNative_OpenAt(intptr_t dirfd, const char *path, int32_t flags, int32_t mode);
int32_t SystemNative_MkDirAt(intptr_t dirfd, const char *path, int32_t mode);
int32_t SystemNative_UnlinkAt(intptr_t dirfd, const char *path, int32_t flags);
int32_t SystemNative_FChMod(intptr_t fd, int32_t mode);

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
        for (int i = 0; i < 40; i++)
            SCHECK(SystemNative_Open("/proc/self/stat", RHU_PAL_O_RDONLY, 0) == -1, "shim-fdmap-identity-leak", "(an identity open past the band succeeded)");
        SCHECK(SystemNative_Close(held[n - 1]) == 0, "shim-fdmap-identity-leak", "(close)");
        intptr_t id = SystemNative_Open("/proc/self/stat", RHU_PAL_O_RDONLY, 0);
        SCHECK(id >= 0, "shim-fdmap-identity-leak", "(the identity file did not open once a descriptor was free: a refused open kept the slot it took)");
        SCHECK(SystemNative_Read(id, ib, 11) == 11 && memcmp(ib, "1 (program)", 11) == 0, "shim-fdmap-identity-leak", "(the identity line)");
        SCHECK(SystemNative_Close(id) == 0, "shim-fdmap-identity-leak", "(close)");
        for (int i = 0; i < n - 1; i++)
            SystemNative_Close(held[i]);
        SCHECK(SystemNative_Unlink("/tmp/ident.txt") == 0, "shim-fdmap-identity-leak", "(unlink)");
        pass("shim-fdmap-identity-leak", " (forty refused opens of the identity file past the band, each giving back the slot it took; the file opens and reads once a descriptor is free)");
    }

    /* /etc: a root that exists and holds nothing. A read under it is a
     * missing file; a verb that would create a name there, or change the
     * root itself, is refused with the read-only file system's code; a
     * verb that would change or remove a name that is not there answers
     * that it is missing, as Linux does on a read-only file system; and
     * making the root itself answers that it exists. */
    {
        rhu_pal_filestatus st;
        rhu_pal_directoryentry e;
        rhu_pal_timespec times[2] = {{1000, 5}, {2000, 7}};
        SCHECK(SystemNative_Stat("/etc", &st) == 0 && (st.Mode & RHU_PAL_S_IFMT) == RHU_PAL_S_IFDIR, "shim-fdmap-etc-root", "(/etc is not a directory)");
        SCHECK(SystemNative_Stat("/etc/", &st) == 0 && (st.Mode & RHU_PAL_S_IFMT) == RHU_PAL_S_IFDIR, "shim-fdmap-etc-root", "(/etc with a trailing slash is not the same directory)");
        void *dir = SystemNative_OpenDir("/etc");
        SCHECK(dir != NULL && SystemNative_ReadDir(dir, &e) != 0 && SystemNative_CloseDir(dir) == 0, "shim-fdmap-etc-root", "(the listing of /etc is not empty)");
        SCHECK(SystemNative_Open("/etc/resolv.conf", RHU_PAL_O_RDONLY, 0) == -1 && SystemNative_GetErrNo() == RHU_GERR_ENOENT, "shim-fdmap-etc-root", "(a read under /etc was not answered as a missing file)");
        SCHECK(SystemNative_Stat("/etc/resolv.conf", &st) == -1 && SystemNative_GetErrNo() == RHU_GERR_ENOENT, "shim-fdmap-etc-root", "(the status of a name under /etc was not ENOENT)");
        SCHECK(SystemNative_Open("/etc", RHU_PAL_O_WRONLY, 0) == -1 && SystemNative_GetErrNo() == RHU_GERR_EISDIR, "shim-fdmap-etc-root", "(an open of /etc itself that writes was not EISDIR)");
        SCHECK(SystemNative_Open("/etc/made.txt", RHU_PAL_O_WRONLY | RHU_PAL_O_CREAT, 0x1A4) == -1 && SystemNative_GetErrNo() == RHU_GERR_EROFS, "shim-fdmap-etc-root", "(a file created under /etc was not refused with EROFS)");
        SCHECK(SystemNative_Open("/etc/made.txt", RHU_PAL_O_RDONLY | RHU_PAL_O_CREAT, 0x1A4) == -1 && SystemNative_GetErrNo() == RHU_GERR_EROFS, "shim-fdmap-etc-root", "(a file created under /etc by a read-only open was not refused with EROFS)");
        SCHECK(SystemNative_Open("/etc/resolv.conf", RHU_PAL_O_WRONLY, 0) == -1 && SystemNative_GetErrNo() == RHU_GERR_ENOENT, "shim-fdmap-etc-root", "(a missing file opened for writing under /etc was not answered ENOENT)");
        SCHECK(SystemNative_Open("/etc/resolv.conf", RHU_PAL_O_RDWR, 0) == -1 && SystemNative_GetErrNo() == RHU_GERR_ENOENT, "shim-fdmap-etc-root", "(a missing file opened for reading and writing under /etc was not answered ENOENT)");
        SCHECK(SystemNative_Open("/etc/resolv.conf", RHU_PAL_O_RDONLY | RHU_PAL_O_TRUNC, 0) == -1 && SystemNative_GetErrNo() == RHU_GERR_ENOENT, "shim-fdmap-etc-root", "(a truncating open of a missing file under /etc was not answered ENOENT)");
        SCHECK(SystemNative_MkDir("/etc/made", 0x1ED) == -1 && SystemNative_GetErrNo() == RHU_GERR_EROFS, "shim-fdmap-etc-root", "(a directory made under /etc was not refused with EROFS)");
        SCHECK(SystemNative_MkDir("/etc", 0x1ED) == -1 && SystemNative_GetErrNo() == RHU_GERR_EEXIST, "shim-fdmap-etc-root", "(making /etc itself was not answered EEXIST)");
        intptr_t fd = SystemNative_Open("/tmp/etc-in.txt", RHU_PAL_O_WRONLY | RHU_PAL_O_CREAT, 0x1A4);
        SCHECK(fd >= 0 && SystemNative_Close(fd) == 0, "shim-fdmap-etc-root", "(a file to rename was not made)");
        SCHECK(SystemNative_Rename("/tmp/etc-in.txt", "/etc/in.txt") == -1 && SystemNative_GetErrNo() == RHU_GERR_EROFS && SystemNative_Stat("/tmp/etc-in.txt", &st) == 0, "shim-fdmap-etc-root", "(a rename into /etc was not refused with EROFS, or the file left its place)");
        SCHECK(SystemNative_Rename("/etc/resolv.conf", "/tmp/etc-out.txt") == -1 && SystemNative_GetErrNo() == RHU_GERR_ENOENT && SystemNative_Stat("/tmp/etc-out.txt", &st) == -1, "shim-fdmap-etc-root", "(a rename of a missing name out of /etc was not answered ENOENT, or it made a file)");
        SCHECK(SystemNative_Unlink("/tmp/etc-in.txt") == 0, "shim-fdmap-etc-root", "(unlink)");
        SCHECK(SystemNative_Unlink("/etc/resolv.conf") == -1 && SystemNative_GetErrNo() == RHU_GERR_ENOENT, "shim-fdmap-etc-root", "(a removal of a missing file under /etc was not answered ENOENT)");
        SCHECK(SystemNative_RmDir("/etc/made") == -1 && SystemNative_GetErrNo() == RHU_GERR_ENOENT, "shim-fdmap-etc-root", "(a removal of a missing directory under /etc was not answered ENOENT)");
        SCHECK(SystemNative_RmDir("/etc") == -1 && SystemNative_GetErrNo() == RHU_GERR_EROFS, "shim-fdmap-etc-root", "(a removal of /etc itself was not refused with EROFS)");
        SCHECK(SystemNative_ChMod("/etc", 0x1FF) == -1 && SystemNative_GetErrNo() == RHU_GERR_EROFS, "shim-fdmap-etc-root", "(a change of the mode of /etc was not refused with EROFS)");
        SCHECK(SystemNative_ChMod("/etc/resolv.conf", 0x1A4) == -1 && SystemNative_GetErrNo() == RHU_GERR_ENOENT, "shim-fdmap-etc-root", "(a change of mode of a missing name under /etc was not answered ENOENT)");
        SCHECK(SystemNative_UTimensat("/etc", times) == -1 && SystemNative_GetErrNo() == RHU_GERR_EROFS, "shim-fdmap-etc-root", "(a change of the times of /etc was not refused with EROFS)");
        SCHECK(SystemNative_UTimensat("/etc/resolv.conf", times) == -1 && SystemNative_GetErrNo() == RHU_GERR_ENOENT, "shim-fdmap-etc-root", "(a change of times of a missing name under /etc was not answered ENOENT)");
        dir = SystemNative_OpenDir("/etc");
        SCHECK(dir != NULL && SystemNative_ReadDir(dir, &e) != 0 && SystemNative_CloseDir(dir) == 0 && SystemNative_Stat("/etc/made.txt", &st) == -1, "shim-fdmap-etc-root", "(a refused verb left a name under /etc)");
        fd = SystemNative_Open("/etcetera", RHU_PAL_O_WRONLY | RHU_PAL_O_CREAT, 0x1A4);
        SCHECK(fd >= 0 && SystemNative_Close(fd) == 0 && SystemNative_Unlink("/etcetera") == 0, "shim-fdmap-etc-root", "(a name that only begins with the root's letters was taken for the root)");
        pass("shim-fdmap-etc-root", " (/etc is a directory with an empty listing; a read under it is a missing file, ENOENT; a name made under it, a rename into it and a change or removal of the root are refused with EROFS; a missing name answers ENOENT to a write open without the create flag, a rename, a removal and a change; making the root answers EEXIST; a name that only begins with its letters is not under it)");
    }

    /* a descriptor for a directory: the open's four answers, then what the descriptor serves */
    {
        /* a directory under a missing parent is ENOENT: the code that steers a creation that makes the parents first */
        SCHECK(SystemNative_MkDir("/tmp/dd/in", 0x1C0) == -1 && SystemNative_GetErrNo() == RHU_GERR_ENOENT, "shim-fdmap-dir-open", "(a directory under a missing parent was not ENOENT)");
        SCHECK(SystemNative_MkDir("/tmp/dd", 0x1ED) == 0, "shim-fdmap-dir-open", "(mkdir)");
        intptr_t ff = SystemNative_Open("/tmp/dd/f.txt", RHU_PAL_O_WRONLY | RHU_PAL_O_CREAT, 0x1A4);
        SCHECK(ff >= 0 && SystemNative_Close(ff) == 0, "shim-fdmap-dir-open", "(a file in the directory)");
        /* one: a read-only open of a directory gives a descriptor, with the directory flag and without it */
        intptr_t d1 = SystemNative_Open("/tmp/dd", RHU_PAL_O_RDONLY | RHU_PAL_O_CLOEXEC | RHU_PAL_O_NOFOLLOW | RHU_PAL_O_DIRECTORY, 0);
        SCHECK(d1 >= 0, "shim-fdmap-dir-open", "(a read-only open of a directory was refused)");
        intptr_t d2 = SystemNative_Open("/tmp/dd", RHU_PAL_O_RDONLY, 0);
        SCHECK(d2 >= 0 && d2 != d1, "shim-fdmap-dir-open", "(a read-only open of a directory without the directory flag was refused)");
        /* two: an open of a directory that writes or creates is EISDIR */
        intptr_t x = SystemNative_Open("/tmp/dd", RHU_PAL_O_WRONLY, 0);
        SCHECK(x == -1 && SystemNative_GetErrNo() == RHU_GERR_EISDIR, "shim-fdmap-dir-open", "(a write-only open of a directory was not EISDIR)");
        if (x >= 0)
            SystemNative_Close(x);
        x = SystemNative_Open("/tmp/dd", RHU_PAL_O_RDWR, 0);
        SCHECK(x == -1 && SystemNative_GetErrNo() == RHU_GERR_EISDIR, "shim-fdmap-dir-open", "(a read-write open of a directory was not EISDIR)");
        if (x >= 0)
            SystemNative_Close(x);
        x = SystemNative_Open("/tmp/dd", RHU_PAL_O_RDONLY | RHU_PAL_O_CREAT, 0x1A4);
        SCHECK(x == -1 && SystemNative_GetErrNo() == RHU_GERR_EISDIR, "shim-fdmap-dir-open", "(an open of a directory that creates was not EISDIR)");
        if (x >= 0)
            SystemNative_Close(x);
        /* three: the directory flag on a path that is a file is ENOTDIR */
        x = SystemNative_Open("/tmp/dd/f.txt", RHU_PAL_O_RDONLY | RHU_PAL_O_DIRECTORY, 0);
        SCHECK(x == -1 && SystemNative_GetErrNo() == RHU_GERR_ENOTDIR, "shim-fdmap-dir-open", "(the directory flag on a file was not ENOTDIR)");
        if (x >= 0)
            SystemNative_Close(x);
        /* four: a path that is not there is ENOENT, with the flag and without it */
        x = SystemNative_Open("/tmp/dd/none", RHU_PAL_O_RDONLY | RHU_PAL_O_DIRECTORY, 0);
        SCHECK(x == -1 && SystemNative_GetErrNo() == RHU_GERR_ENOENT, "shim-fdmap-dir-open", "(a missing path with the directory flag was not ENOENT)");
        if (x >= 0)
            SystemNative_Close(x);
        x = SystemNative_Open("/tmp/dd/none", RHU_PAL_O_RDONLY, 0);
        SCHECK(x == -1 && SystemNative_GetErrNo() == RHU_GERR_ENOENT, "shim-fdmap-dir-open", "(a missing path was not ENOENT)");
        if (x >= 0)
            SystemNative_Close(x);
        /* the descriptor: a directory's status with its mode and the one owner; a read is EISDIR; a write is refused */
        SCHECK(SystemNative_FStat(d1, &st) == 0 && is_type(&st, RHU_PAL_S_IFDIR) && (st.Mode & 0x1FF) == 0x1ED && st.Uid == 0, "shim-fdmap-dir-open", "(the descriptor's status is not a directory's with its mode and owner 0)");
        SCHECK(SystemNative_Read(d1, buf, 4) == -1 && SystemNative_GetErrNo() == RHU_GERR_EISDIR, "shim-fdmap-dir-open", "(a read of a directory's descriptor was not EISDIR)");
        SCHECK(SystemNative_Write(d1, "x", 1) == -1 && SystemNative_GetErrNo() == RHU_GERR_EBADF, "shim-fdmap-dir-open", "(a write to a directory's descriptor was not EBADF)");
        /* a root opens as a directory too */
        intptr_t dr = SystemNative_Open("/tmp", RHU_PAL_O_RDONLY | RHU_PAL_O_DIRECTORY, 0);
        SCHECK(dr >= 0 && SystemNative_FStat(dr, &st) == 0 && is_type(&st, RHU_PAL_S_IFDIR) && SystemNative_Close(dr) == 0, "shim-fdmap-dir-open", "(the temporary root does not open as a directory)");
        /* the read-only root too: it holds nothing and takes nothing, and a read-only open is owed its descriptor */
        dr = SystemNative_Open("/etc", RHU_PAL_O_RDONLY | RHU_PAL_O_DIRECTORY, 0);
        SCHECK(dr >= 0 && SystemNative_FStat(dr, &st) == 0 && is_type(&st, RHU_PAL_S_IFDIR) && SystemNative_Close(dr) == 0, "shim-fdmap-dir-open", "(the read-only root does not open as a directory)");
        /* a directory removed while a descriptor holds it: the name goes at once, the descriptor answers until its last close */
        SCHECK(SystemNative_Unlink("/tmp/dd/f.txt") == 0 && SystemNative_RmDir("/tmp/dd") == 0, "shim-fdmap-dir-open", "(the removal of a directory that a descriptor holds was refused)");
        SCHECK(SystemNative_Stat("/tmp/dd", &st) == -1 && SystemNative_GetErrNo() == RHU_GERR_ENOENT, "shim-fdmap-dir-open", "(the removed directory's name still resolves)");
        SCHECK(SystemNative_FStat(d1, &st) == 0 && is_type(&st, RHU_PAL_S_IFDIR), "shim-fdmap-dir-open", "(the removed directory's descriptor no longer answers)");
        SCHECK(SystemNative_MkDir("/tmp/dd", 0x1ED) == 0 && SystemNative_RmDir("/tmp/dd") == 0, "shim-fdmap-dir-open", "(the name of a removed directory that a descriptor holds cannot be made again)");
        SCHECK(SystemNative_Close(d1) == 0 && SystemNative_Close(d2) == 0, "shim-fdmap-dir-open", "(close)");
        SCHECK(SystemNative_FStat(d1, &st) == -1 && SystemNative_GetErrNo() == RHU_GERR_EBADF, "shim-fdmap-dir-open", "(a closed directory descriptor answers)");
        pass("shim-fdmap-dir-open", " (a read-only open of a directory gives a descriptor; one that writes or creates is EISDIR; the directory flag on a file is ENOTDIR; a missing path is ENOENT; the descriptor answers a directory's status, refuses a read with EISDIR and outlives the directory's name)");
    }

    /* the descriptor forms of open, mkdir and unlink: the path forms' own verbs under a directory's descriptor */
    {
        SCHECK(SystemNative_MkDir("/tmp/at", 0x1ED) == 0, "shim-fdmap-at", "(mkdir)");
        intptr_t d = SystemNative_Open("/tmp/at", RHU_PAL_O_RDONLY | RHU_PAL_O_DIRECTORY, 0);
        SCHECK(d >= 0, "shim-fdmap-at", "(the directory's descriptor)");
        SCHECK(SystemNative_MkDirAt(d, "sub", 0x1C0) == 0, "shim-fdmap-at", "(mkdirat refused)");
        SCHECK(SystemNative_Stat("/tmp/at/sub", &st) == 0 && is_type(&st, RHU_PAL_S_IFDIR), "shim-fdmap-at", "(mkdirat did not make the directory under its descriptor's path)");
        SCHECK(SystemNative_MkDirAt(d, "sub", 0x1C0) == -1 && SystemNative_GetErrNo() == RHU_GERR_EEXIST, "shim-fdmap-at", "(a second mkdirat of one name was not EEXIST)");
        intptr_t f = SystemNative_OpenAt(d, "f.bin", RHU_PAL_O_RDWR | RHU_PAL_O_NOFOLLOW | RHU_PAL_O_CLOEXEC, 0);
        SCHECK(f == -1 && SystemNative_GetErrNo() == RHU_GERR_ENOENT, "shim-fdmap-at", "(openat of an absent name without create was not ENOENT)");
        if (f >= 0)
            SystemNative_Close(f);
        f = SystemNative_OpenAt(d, "f.bin", RHU_PAL_O_RDWR | RHU_PAL_O_NOFOLLOW | RHU_PAL_O_CLOEXEC | RHU_PAL_O_CREAT | RHU_PAL_O_EXCL, 0x180);
        SCHECK(f >= 0 && SystemNative_Write(f, "ATAT", 4) == 4, "shim-fdmap-at", "(openat with create and exclusive refused)");
        intptr_t g = SystemNative_OpenAt(d, "f.bin", RHU_PAL_O_RDWR | RHU_PAL_O_NOFOLLOW | RHU_PAL_O_CLOEXEC | RHU_PAL_O_CREAT | RHU_PAL_O_EXCL, 0x180);
        SCHECK(g == -1 && SystemNative_GetErrNo() == RHU_GERR_EEXIST, "shim-fdmap-at", "(openat with exclusive on an existing name was not EEXIST)");
        if (g >= 0)
            SystemNative_Close(g);
        g = SystemNative_OpenAt(d, "f.bin", RHU_PAL_O_RDWR | RHU_PAL_O_NOFOLLOW | RHU_PAL_O_CLOEXEC, 0);
        SCHECK(g >= 0 && g != f && SystemNative_PRead(g, buf, 4, 0) == 4 && memcmp(buf, "ATAT", 4) == 0, "shim-fdmap-at", "(openat of an existing name does not read its bytes)");
        SCHECK(SystemNative_Stat("/tmp/at/f.bin", &st) == 0 && st.Size == 4 && (st.Mode & 0x1FF) == 0x180, "shim-fdmap-at", "(the file is not at its descriptor's path with its size and mode)");
        /* the descriptor of the call: a file's is ENOTDIR, the console's is ENOTDIR, a closed one EBADF; an absolute path ignores it; the empty name is ENOENT */
        SCHECK(SystemNative_OpenAt(f, "x", RHU_PAL_O_RDONLY, 0) == -1 && SystemNative_GetErrNo() == RHU_GERR_ENOTDIR, "shim-fdmap-at", "(openat under a file's descriptor was not ENOTDIR)");
        SCHECK(SystemNative_MkDirAt(f, "x", 0x1C0) == -1 && SystemNative_GetErrNo() == RHU_GERR_ENOTDIR, "shim-fdmap-at", "(mkdirat under a file's descriptor was not ENOTDIR)");
        SCHECK(SystemNative_UnlinkAt(f, "x", 0) == -1 && SystemNative_GetErrNo() == RHU_GERR_ENOTDIR, "shim-fdmap-at", "(unlinkat under a file's descriptor was not ENOTDIR)");
        SCHECK(SystemNative_OpenAt(1, "x", RHU_PAL_O_RDONLY, 0) == -1 && SystemNative_GetErrNo() == RHU_GERR_ENOTDIR, "shim-fdmap-at", "(openat under the console's descriptor was not ENOTDIR)");
        SCHECK(SystemNative_Close(g) == 0, "shim-fdmap-at", "(close)");
        SCHECK(SystemNative_OpenAt(g, "f.bin", RHU_PAL_O_RDONLY, 0) == -1 && SystemNative_GetErrNo() == RHU_GERR_EBADF, "shim-fdmap-at", "(openat under a closed descriptor was not EBADF)");
        SCHECK(SystemNative_MkDirAt(g, "y", 0x1C0) == -1 && SystemNative_GetErrNo() == RHU_GERR_EBADF, "shim-fdmap-at", "(mkdirat under a closed descriptor was not EBADF)");
        SCHECK(SystemNative_UnlinkAt(g, "f.bin", 0) == -1 && SystemNative_GetErrNo() == RHU_GERR_EBADF, "shim-fdmap-at", "(unlinkat under a closed descriptor was not EBADF)");
        intptr_t a = SystemNative_OpenAt(f, "/tmp/at/f.bin", RHU_PAL_O_RDONLY, 0);
        SCHECK(a >= 0 && SystemNative_Close(a) == 0, "shim-fdmap-at", "(an absolute path did not ignore the descriptor)");
        SCHECK(SystemNative_OpenAt(d, "", RHU_PAL_O_RDONLY, 0) == -1 && SystemNative_GetErrNo() == RHU_GERR_ENOENT, "shim-fdmap-at", "(openat of the empty name was not ENOENT)");
        /* unlinkat: a flag out of range; a directory that is not empty; a directory without the directory flag and a
         * file with it, each as the path form answers; an absent name; then the file and the directory */
        SCHECK(SystemNative_UnlinkAt(d, "f.bin", 2) == -1 && SystemNative_GetErrNo() == RHU_GERR_EINVAL, "shim-fdmap-at", "(unlinkat accepted a flag out of range)");
        intptr_t in = SystemNative_Open("/tmp/at/sub/in.txt", RHU_PAL_O_WRONLY | RHU_PAL_O_CREAT, 0x1A4);
        SCHECK(in >= 0 && SystemNative_Close(in) == 0, "shim-fdmap-at", "(a file in the nested directory)");
        SCHECK(SystemNative_UnlinkAt(d, "sub", RHU_PAL_AT_REMOVEDIR) == -1 && SystemNative_GetErrNo() == RHU_GERR_ENOTEMPTY, "shim-fdmap-at", "(unlinkat removed a directory that is not empty)");
        SCHECK(SystemNative_Unlink("/tmp/at/sub/in.txt") == 0, "shim-fdmap-at", "(unlink)");
        int32_t by_path = SystemNative_Unlink("/tmp/at/sub") == -1 ? SystemNative_GetErrNo() : 0;
        SCHECK(by_path != 0 && SystemNative_UnlinkAt(d, "sub", 0) == -1 && SystemNative_GetErrNo() == by_path, "shim-fdmap-at", "(unlinkat of a directory without the directory flag does not answer as unlink does)");
        by_path = SystemNative_RmDir("/tmp/at/f.bin") == -1 ? SystemNative_GetErrNo() : 0;
        SCHECK(by_path != 0 && SystemNative_UnlinkAt(d, "f.bin", RHU_PAL_AT_REMOVEDIR) == -1 && SystemNative_GetErrNo() == by_path, "shim-fdmap-at", "(unlinkat of a file with the directory flag does not answer as rmdir does)");
        SCHECK(SystemNative_UnlinkAt(d, "none", 0) == -1 && SystemNative_GetErrNo() == RHU_GERR_ENOENT, "shim-fdmap-at", "(unlinkat of an absent name was not ENOENT)");
        SCHECK(SystemNative_Close(f) == 0 && SystemNative_UnlinkAt(d, "f.bin", 0) == 0, "shim-fdmap-at", "(unlinkat of a file refused)");
        SCHECK(SystemNative_Stat("/tmp/at/f.bin", &st) == -1 && SystemNative_GetErrNo() == RHU_GERR_ENOENT, "shim-fdmap-at", "(the file unlinkat removed still resolves)");
        SCHECK(SystemNative_UnlinkAt(d, "sub", RHU_PAL_AT_REMOVEDIR) == 0, "shim-fdmap-at", "(unlinkat of a directory refused)");
        SCHECK(SystemNative_Stat("/tmp/at/sub", &st) == -1 && SystemNative_GetErrNo() == RHU_GERR_ENOENT, "shim-fdmap-at", "(the directory unlinkat removed still resolves)");
        /* under a directory removed while its descriptor stood nothing is made */
        SCHECK(SystemNative_RmDir("/tmp/at") == 0, "shim-fdmap-at", "(rmdir)");
        f = SystemNative_OpenAt(d, "late.bin", RHU_PAL_O_RDWR | RHU_PAL_O_CREAT, 0x180);
        SCHECK(f == -1 && SystemNative_GetErrNo() == RHU_GERR_ENOENT, "shim-fdmap-at", "(openat under a removed directory was not ENOENT)");
        if (f >= 0)
            SystemNative_Close(f);
        SCHECK(SystemNative_MkDirAt(d, "late", 0x1C0) == -1 && SystemNative_GetErrNo() == RHU_GERR_ENOENT, "shim-fdmap-at", "(mkdirat under a removed directory was not ENOENT)");
        SCHECK(SystemNative_Close(d) == 0, "shim-fdmap-at", "(close)");
        pass("shim-fdmap-at", " (openat, mkdirat and unlinkat act under their descriptor's directory as the path forms act on the composed path; a file's descriptor is ENOTDIR, a closed one EBADF, an absolute path ignores it, a removed directory holds nothing new)");
    }

    /* a directory keeps its mode: as given at mkdir and mkdirat, changed by chmod and fchmod, reported by both statuses */
    {
        SCHECK(SystemNative_MkDir("/tmp/dm", 0x1C0) == 0 && SystemNative_Stat("/tmp/dm", &st) == 0 && is_type(&st, RHU_PAL_S_IFDIR) && (st.Mode & 0x1FF) == 0x1C0, "shim-fdmap-dir-mode", "(a directory's mode is not the one given at mkdir)");
        SCHECK(SystemNative_ChMod("/tmp/dm", 0x1FF) == 0 && SystemNative_Stat("/tmp/dm", &st) == 0 && (st.Mode & 0x1FF) == 0x1FF, "shim-fdmap-dir-mode", "(chmod of a directory is not kept)");
        intptr_t d = SystemNative_Open("/tmp/dm", RHU_PAL_O_RDONLY | RHU_PAL_O_DIRECTORY, 0);
        SCHECK(d >= 0 && SystemNative_FChMod(d, 0x1C0) == 0 && SystemNative_FStat(d, &st) == 0 && (st.Mode & 0x1FF) == 0x1C0, "shim-fdmap-dir-mode", "(fchmod of a directory's descriptor is not kept)");
        SCHECK(SystemNative_Stat("/tmp/dm", &st) == 0 && (st.Mode & 0x1FF) == 0x1C0, "shim-fdmap-dir-mode", "(the mode set through the descriptor is not the path's)");
        SCHECK(SystemNative_MkDirAt(d, "in", 0x1FF) == 0 && SystemNative_Stat("/tmp/dm/in", &st) == 0 && (st.Mode & 0x1FF) == 0x1FF, "shim-fdmap-dir-mode", "(a directory's mode is not the one given at mkdirat)");
        SCHECK(SystemNative_UnlinkAt(d, "in", RHU_PAL_AT_REMOVEDIR) == 0 && SystemNative_Close(d) == 0 && SystemNative_RmDir("/tmp/dm") == 0, "shim-fdmap-dir-mode", "(unlinkat, close, rmdir)");
        SCHECK(SystemNative_Stat("/tmp", &st) == 0 && (st.Mode & 0x1FF) == 0x1ED, "shim-fdmap-dir-mode", "(the temporary root's mode is not 0755)");
        pass("shim-fdmap-dir-mode", " (a directory's mode is the one given at mkdir or mkdirat until chmod or fchmod changes it, and both statuses report it; a root's is 0755)");
    }

    /* the advisory lock on a directory's descriptor: the record a file has */
    {
        SCHECK(SystemNative_MkDir("/tmp/dl", 0x1ED) == 0, "shim-fdmap-dir-flock", "(mkdir)");
        intptr_t d1 = SystemNative_Open("/tmp/dl", RHU_PAL_O_RDONLY | RHU_PAL_O_DIRECTORY, 0);
        intptr_t d2 = SystemNative_Open("/tmp/dl", RHU_PAL_O_RDONLY | RHU_PAL_O_DIRECTORY, 0);
        SCHECK(d1 >= 0 && d2 >= 0 && d1 != d2, "shim-fdmap-dir-flock", "(two descriptors of one directory)");
        SCHECK(SystemNative_FLock(d1, RHU_PAL_LOCK_EX) == 0, "shim-fdmap-dir-flock", "(an exclusive lock on a free directory refused)");
        SCHECK(SystemNative_FLock(d2, RHU_PAL_LOCK_EX | RHU_PAL_LOCK_NB) == -1 && SystemNative_GetErrNo() == RHU_GERR_EAGAIN, "shim-fdmap-dir-flock", "(a second descriptor took the directory's exclusive lock)");
        SCHECK(SystemNative_FLock(d1, RHU_PAL_LOCK_UN) == 0 && SystemNative_FLock(d2, RHU_PAL_LOCK_EX | RHU_PAL_LOCK_NB) == 0, "shim-fdmap-dir-flock", "(the lock was not given back at the unlock)");
        SCHECK(SystemNative_Close(d2) == 0 && SystemNative_FLock(d1, RHU_PAL_LOCK_EX | RHU_PAL_LOCK_NB) == 0, "shim-fdmap-dir-flock", "(the lock was not given back at the close)");
        SCHECK(SystemNative_FLock(d1, RHU_PAL_LOCK_UN) == 0 && SystemNative_Close(d1) == 0 && SystemNative_RmDir("/tmp/dl") == 0, "shim-fdmap-dir-flock", "(unlock, close, rmdir)");
        pass("shim-fdmap-dir-flock", " (a directory's descriptor carries the advisory lock: exclusive, refused to a second descriptor, given back at the unlock and at the close)");
    }

    /* THE LOCK IN CONFLICT IS REFUSED AT ONCE, BLOCKING OR NOT -- a decision on record, with its error code. Every
     * descriptor of the store belongs to the one process, and the lock is advisory per descriptor: a request that
     * waited for another descriptor's holder would wait on the caller's own process. No timer is read here: the
     * check is that each call returns at all, with its code; a request that waited would never return, and the
     * gate's own bound on the run would report the hang. */
    {
        _Static_assert(RHU_GERR_EAGAIN == 11, "the code a lock in conflict answers is 11");
        intptr_t f1 = SystemNative_Open("/tmp/lock-b.txt", RHU_PAL_O_RDWR | RHU_PAL_O_CREAT, 0x1A4);
        intptr_t f2 = SystemNative_Open("/tmp/lock-b.txt", RHU_PAL_O_RDWR, 0);
        SCHECK(f1 >= 0 && f2 >= 0, "shim-fdmap-flock-blocking", "(two descriptors)");
        SCHECK(SystemNative_FLock(f1, RHU_PAL_LOCK_EX) == 0, "shim-fdmap-flock-blocking", "(a blocking exclusive lock on a free file refused)");
        SCHECK(SystemNative_FLock(f2, RHU_PAL_LOCK_EX) == -1 && SystemNative_GetErrNo() == RHU_GERR_EAGAIN, "shim-fdmap-flock-blocking", "(a blocking exclusive request in conflict was not refused with EWOULDBLOCK, 11)");
        SCHECK(SystemNative_FLock(f2, RHU_PAL_LOCK_SH) == -1 && SystemNative_GetErrNo() == RHU_GERR_EAGAIN, "shim-fdmap-flock-blocking", "(a blocking shared request in conflict was not refused with EWOULDBLOCK, 11)");
        SCHECK(SystemNative_FLock(f1, RHU_PAL_LOCK_UN) == 0 && SystemNative_FLock(f2, RHU_PAL_LOCK_EX) == 0, "shim-fdmap-flock-blocking", "(the blocking exclusive request was not granted after the unlock)");
        SCHECK(SystemNative_FLock(f2, RHU_PAL_LOCK_UN) == 0 && SystemNative_FLock(f1, RHU_PAL_LOCK_SH) == 0 && SystemNative_FLock(f2, RHU_PAL_LOCK_SH) == 0, "shim-fdmap-flock-blocking", "(blocking shared requests do not coexist)");
        SCHECK(SystemNative_FLock(f1, RHU_PAL_LOCK_EX) == -1 && SystemNative_GetErrNo() == RHU_GERR_EAGAIN, "shim-fdmap-flock-blocking", "(a blocking exclusive request over another descriptor's shared lock was not refused with EWOULDBLOCK, 11)");
        SCHECK(SystemNative_Close(f1) == 0 && SystemNative_Close(f2) == 0 && SystemNative_Unlink("/tmp/lock-b.txt") == 0, "shim-fdmap-flock-blocking", "(close, unlink)");
        pass("shim-fdmap-flock-blocking", " (a blocking request in conflict is refused at once with EWOULDBLOCK, 11, exclusive or shared, and is granted after the unlock)");
    }

    /* a shared mapping of a store file is the file's own bytes */
    {
        intptr_t f = SystemNative_Open("/tmp/map.bin", RHU_PAL_O_RDWR | RHU_PAL_O_CREAT | RHU_PAL_O_EXCL, 0x180);
        SCHECK(f >= 0 && SystemNative_FTruncate(f, 4096) == 0, "shim-fdmap-map", "(create and size)");
        uint8_t *m = f >= 0 ? SystemNative_MMap(NULL, 4096, RHU_PAL_PROT_READ | RHU_PAL_PROT_WRITE, RHU_PAL_MAP_SHARED, f, 0) : NULL;
        SCHECK(m != NULL && ((uintptr_t)m & 4095u) == 0, "shim-fdmap-map", "(a shared mapping of a sized file was refused, or is not page-aligned)");
        if (m != NULL) {
            int zero = 1;
            for (uint32_t i = 0; i < 4096u; i++)
                zero &= m[i] == 0;
            SCHECK(zero, "shim-fdmap-map", "(the mapping of a file sized by truncation is not zero)");
            memcpy(m, "MAP!", 4);
            SCHECK(SystemNative_PRead(f, buf, 4, 0) == 4 && memcmp(buf, "MAP!", 4) == 0, "shim-fdmap-map", "(a write through the mapping is not what the file reads)");
            SCHECK(SystemNative_PWrite(f, "FILE", 4, 100) == 4 && memcmp(m + 100, "FILE", 4) == 0, "shim-fdmap-map", "(a write to the file is not what the mapping shows)");
            uint8_t *twin = SystemNative_MMap(NULL, 4096, RHU_PAL_PROT_READ | RHU_PAL_PROT_WRITE, RHU_PAL_MAP_SHARED, f, 0);
            SCHECK(twin != NULL && memcmp(twin, "MAP!", 4) == 0, "shim-fdmap-map", "(a second mapping of the file does not show its bytes)");
            if (twin != NULL) {
                twin[8] = 0x5A;
                SCHECK(m[8] == 0x5A, "shim-fdmap-map", "(two mappings of one file are not the same bytes)");
                SCHECK(SystemNative_MUnmap(twin, 4096) == 0, "shim-fdmap-map", "(munmap of the second mapping)");
            }
            /* what a mapping is not: a private one, a directory's, one that asks more than its descriptor may */
            SCHECK(SystemNative_MMap(NULL, 4096, RHU_PAL_PROT_READ | RHU_PAL_PROT_WRITE, RHU_PAL_MAP_PRIVATE, f, 0) == NULL, "shim-fdmap-map", "(a private mapping of a descriptor was granted)");
            intptr_t dr = SystemNative_Open("/tmp", RHU_PAL_O_RDONLY | RHU_PAL_O_DIRECTORY, 0);
            SCHECK(dr >= 0 && SystemNative_MMap(NULL, 4096, RHU_PAL_PROT_READ, RHU_PAL_MAP_SHARED, dr, 0) == NULL && SystemNative_GetErrNo() == RHU_GERR_ENODEV, "shim-fdmap-map", "(a mapping of a directory's descriptor was not ENODEV)");
            if (dr >= 0)
                SystemNative_Close(dr);
            intptr_t ro = SystemNative_Open("/tmp/map.bin", RHU_PAL_O_RDONLY, 0);
            SCHECK(ro >= 0 && SystemNative_MMap(NULL, 4096, RHU_PAL_PROT_READ | RHU_PAL_PROT_WRITE, RHU_PAL_MAP_SHARED, ro, 0) == NULL && SystemNative_GetErrNo() == RHU_GERR_EACCES, "shim-fdmap-map", "(a writable shared mapping through a read-only descriptor was not EACCES)");
            if (ro >= 0)
                SystemNative_Close(ro);
            SCHECK(SystemNative_MUnmap(m, 4096) == 0, "shim-fdmap-map", "(munmap)");
        }
        if (f >= 0)
            SystemNative_Close(f);
        SCHECK(SystemNative_Unlink("/tmp/map.bin") == 0, "shim-fdmap-map", "(unlink)");
        /* a file of less than a page is mapped too, and shows its bytes */
        f = SystemNative_Open("/tmp/map-s.bin", RHU_PAL_O_RDWR | RHU_PAL_O_CREAT | RHU_PAL_O_EXCL, 0x180);
        SCHECK(f >= 0 && SystemNative_Write(f, "0123456789", 10) == 10, "shim-fdmap-map", "(a small file)");
        m = f >= 0 ? SystemNative_MMap(NULL, 10, RHU_PAL_PROT_READ | RHU_PAL_PROT_WRITE, RHU_PAL_MAP_SHARED, f, 0) : NULL;
        SCHECK(m != NULL && ((uintptr_t)m & 4095u) == 0 && memcmp(m, "0123456789", 10) == 0, "shim-fdmap-map", "(the mapping of a file of less than a page does not show its bytes)");
        if (m != NULL) {
            m[9] = 'X';
            SCHECK(SystemNative_PRead(f, buf, 10, 0) == 10 && memcmp(buf, "012345678X", 10) == 0, "shim-fdmap-map", "(a write through a small file's mapping is not what the file reads)");
            SCHECK(SystemNative_MUnmap(m, 10) == 0, "shim-fdmap-map", "(munmap of the small file's mapping)");
        }
        if (f >= 0)
            SystemNative_Close(f);
        SCHECK(SystemNative_Unlink("/tmp/map-s.bin") == 0, "shim-fdmap-map", "(unlink)");
        pass("shim-fdmap-map", " (a shared mapping of a store file is page-aligned and is the file's own bytes: a write through it is what the file reads, a write to the file is what it shows, two mappings are the same bytes; a private mapping, a directory's and one past its descriptor's access are refused)");
    }

    /* the mapping's lifetime: the block is freed when name, descriptors and mappings are all gone, and is moved only
     * while no mapping stands */
    {
        intptr_t f = SystemNative_Open("/tmp/map-l.bin", RHU_PAL_O_RDWR | RHU_PAL_O_CREAT | RHU_PAL_O_EXCL, 0x180);
        SCHECK(f >= 0 && SystemNative_FTruncate(f, 4096) == 0, "shim-fdmap-map-lifetime", "(create and size)");
        uint8_t *m = f >= 0 ? SystemNative_MMap(NULL, 4096, RHU_PAL_PROT_READ | RHU_PAL_PROT_WRITE, RHU_PAL_MAP_SHARED, f, 0) : NULL;
        SCHECK(m != NULL, "shim-fdmap-map-lifetime", "(a shared mapping of a sized file was refused)");
        if (m != NULL) {
            /* a resize within the block is granted in place, the newly shown bytes zero */
            memcpy(m, "KEEP", 4);
            m[2000] = 0x55;
            SCHECK(SystemNative_FTruncate(f, 1000) == 0 && SystemNative_FStat(f, &st) == 0 && st.Size == 1000, "shim-fdmap-map-lifetime", "(a mapped file did not shrink within its block)");
            SCHECK(SystemNative_FTruncate(f, 4096) == 0 && SystemNative_FStat(f, &st) == 0 && st.Size == 4096, "shim-fdmap-map-lifetime", "(a mapped file did not grow within its block)");
            SCHECK(memcmp(m, "KEEP", 4) == 0 && m[2000] == 0, "shim-fdmap-map-lifetime", "(a resize within the block moved the bytes, or showed old bytes again)");
            /* past the block: EFBIG while a mapping stands, a truncation and a write alike; nothing moves */
            SCHECK(SystemNative_FTruncate(f, 4097) == -1 && SystemNative_GetErrNo() == RHU_GERR_EFBIG, "shim-fdmap-map-lifetime", "(a growth past a mapped file's block was not EFBIG)");
            SCHECK(SystemNative_PWrite(f, "x", 1, 4096) == -1 && SystemNative_GetErrNo() == RHU_GERR_EFBIG, "shim-fdmap-map-lifetime", "(a write past a mapped file's block was not EFBIG)");
            SCHECK(SystemNative_FStat(f, &st) == 0 && st.Size == 4096 && memcmp(m, "KEEP", 4) == 0, "shim-fdmap-map-lifetime", "(a refused growth changed the file)");
            uint8_t *twin = SystemNative_MMap(NULL, 4096, RHU_PAL_PROT_READ | RHU_PAL_PROT_WRITE, RHU_PAL_MAP_SHARED, f, 0);
            SCHECK(twin != NULL && SystemNative_MUnmap(twin, 4096) == 0, "shim-fdmap-map-lifetime", "(a second mapping and its unmap)");
            SCHECK(SystemNative_FTruncate(f, 8192) == -1 && SystemNative_GetErrNo() == RHU_GERR_EFBIG, "shim-fdmap-map-lifetime", "(the unmap of one mapping of two released the block's bound)");
            /* granted once no mapping stands: the bytes go with the file */
            SCHECK(SystemNative_MUnmap(m, 4096) == 0 && SystemNative_FTruncate(f, 8192) == 0, "shim-fdmap-map-lifetime", "(a growth past the block was not granted after the last unmap)");
            SCHECK(SystemNative_PRead(f, buf, 4, 0) == 4 && memcmp(buf, "KEEP", 4) == 0, "shim-fdmap-map-lifetime", "(the grown file lost its bytes)");
            /* an unlink and a close leave the bytes to the mapping that stands */
            m = SystemNative_MMap(NULL, 8192, RHU_PAL_PROT_READ | RHU_PAL_PROT_WRITE, RHU_PAL_MAP_SHARED, f, 0);
            SCHECK(m != NULL, "shim-fdmap-map-lifetime", "(a mapping of the grown file was refused)");
            if (m != NULL) {
                m[5000] = 0x77;
                SCHECK(SystemNative_Unlink("/tmp/map-l.bin") == 0 && SystemNative_Close(f) == 0, "shim-fdmap-map-lifetime", "(unlink and close of a mapped file)");
                f = -1;
                SCHECK(SystemNative_Stat("/tmp/map-l.bin", &st) == -1 && SystemNative_GetErrNo() == RHU_GERR_ENOENT, "shim-fdmap-map-lifetime", "(the unlinked name still resolves)");
                SCHECK(memcmp(m, "KEEP", 4) == 0 && m[5000] == 0x77, "shim-fdmap-map-lifetime", "(the bytes did not stay with the mapping after the unlink and the close)");
                m[1] = 'e';
                SCHECK(m[1] == 'e' && SystemNative_MUnmap(m, 8192) == 0, "shim-fdmap-map-lifetime", "(the last unmap of a file with no name and no descriptor)");
            }
        }
        if (f >= 0) {
            SystemNative_Close(f);
            SystemNative_Unlink("/tmp/map-l.bin");
        }
        /* a file with no name is sized and mapped through its descriptor */
        f = SystemNative_Open("/tmp/map-n.bin", RHU_PAL_O_RDWR | RHU_PAL_O_CREAT | RHU_PAL_O_EXCL, 0x180);
        SCHECK(f >= 0 && SystemNative_Unlink("/tmp/map-n.bin") == 0 && SystemNative_FTruncate(f, 4096) == 0, "shim-fdmap-map-lifetime", "(a file with no name was not sized through its descriptor)");
        m = f >= 0 ? SystemNative_MMap(NULL, 4096, RHU_PAL_PROT_READ | RHU_PAL_PROT_WRITE, RHU_PAL_MAP_SHARED, f, 0) : NULL;
        SCHECK(m != NULL, "shim-fdmap-map-lifetime", "(a file with no name was not mapped through its descriptor)");
        if (m != NULL) {
            m[10] = 0x33;
            SCHECK(SystemNative_PRead(f, buf, 1, 10) == 1 && buf[0] == 0x33, "shim-fdmap-map-lifetime", "(a write through the mapping of a file with no name is not what its descriptor reads)");
            SCHECK(SystemNative_MUnmap(m, 4096) == 0, "shim-fdmap-map-lifetime", "(munmap)");
        }
        if (f >= 0)
            SystemNative_Close(f);
        /* TEN THOUSAND ROUNDS END WITH THE STORE AS IT BEGAN: create, size, map, unmap, close, unlink, as the named
         * mutex's last handle goes; then two thousand in the two other orders, the mapping last and the descriptor
         * last. A round that kept its page would stop within sixteen rounds, since the hosting layer's heap is 64 KiB
         * on this row; a round that kept its row would stop the 33rd. */
        int bad = 0;
        for (uint32_t r = 0; r < 12000u && bad == 0; r++) {
            intptr_t rf = SystemNative_Open("/tmp/map-r.bin", RHU_PAL_O_RDWR | RHU_PAL_O_CREAT | RHU_PAL_O_EXCL, 0x180);
            if (rf < 0 || SystemNative_FTruncate(rf, 4096) != 0) {
                bad = 1;
                if (rf >= 0) {
                    SystemNative_Close(rf);
                    SystemNative_Unlink("/tmp/map-r.bin");
                }
                break;
            }
            uint8_t *rm = SystemNative_MMap(NULL, 4096, RHU_PAL_PROT_READ | RHU_PAL_PROT_WRITE, RHU_PAL_MAP_SHARED, rf, 0);
            if (rm == NULL || rm[0] != 0) {
                bad = 2;
                SystemNative_Close(rf);
                SystemNative_Unlink("/tmp/map-r.bin");
                break;
            }
            rm[0] = 0xA5;
            if (r < 10000u)
                bad = SystemNative_MUnmap(rm, 4096) != 0 || SystemNative_Close(rf) != 0 || SystemNative_Unlink("/tmp/map-r.bin") != 0 ? 3 : 0;
            else if ((r & 1u) == 0)
                bad = SystemNative_Unlink("/tmp/map-r.bin") != 0 || SystemNative_Close(rf) != 0 || rm[0] != 0xA5 || SystemNative_MUnmap(rm, 4096) != 0 ? 4 : 0;
            else
                bad = SystemNative_Unlink("/tmp/map-r.bin") != 0 || SystemNative_MUnmap(rm, 4096) != 0 || SystemNative_Close(rf) != 0 ? 5 : 0;
        }
        SCHECK(bad != 1, "shim-fdmap-map-lifetime", "(a round could not create and size its file: an earlier round kept its row or its page)");
        SCHECK(bad != 2, "shim-fdmap-map-lifetime", "(a round's mapping was refused, or showed an earlier round's byte)");
        SCHECK(bad == 0, "shim-fdmap-map-lifetime", "(a round's unmap, close or unlink was refused)");
        pass("shim-fdmap-map-lifetime", " (a mapped file is resized in place within its block and refused past it with EFBIG until its last unmap; an unlink and a close leave the bytes to the mapping; a file with no name is sized and mapped; twelve thousand rounds end with the store as it began)");
    }

    /* the store's bounds as numbers: 32 files, 24 made directories, 29 descriptors in two ranges, a path under 128
     * characters */
    {
        char p[160];
        int made = 0;
        intptr_t last = -1;
        /* the ladder's own file of the groups above is given back first: the files are counted on an empty store */
        SCHECK(SystemNative_Unlink("/tmp/v.txt") == 0, "shim-fdmap-store-bounds", "(the ladder's own file was not there to give back)");
        for (int i = 0; i < 33; i++) {
            memcpy(p, "/tmp/bf-00", 11);
            p[8] = (char)('0' + i / 10);
            p[9] = (char)('0' + i % 10);
            last = SystemNative_Open(p, RHU_PAL_O_WRONLY | RHU_PAL_O_CREAT | RHU_PAL_O_EXCL, 0x1A4);
            if (last < 0)
                break;
            SystemNative_Close(last);
            made++;
        }
        SCHECK(made == 32 && last == -1 && SystemNative_GetErrNo() == RHU_GERR_ENOSPC, "shim-fdmap-store-bounds", "(the store does not hold 32 files and refuse the 33rd with ENOSPC)");
        for (int i = 0; i < made; i++) {
            memcpy(p, "/tmp/bf-00", 11);
            p[8] = (char)('0' + i / 10);
            p[9] = (char)('0' + i % 10);
            SCHECK(SystemNative_Unlink(p) == 0, "shim-fdmap-store-bounds", "(unlink)");
        }
        made = 0;
        int32_t rc = 0;
        for (int i = 0; i < 25; i++) {
            memcpy(p, "/tmp/bd-00", 11);
            p[8] = (char)('0' + i / 10);
            p[9] = (char)('0' + i % 10);
            rc = SystemNative_MkDir(p, 0x1ED);
            if (rc != 0)
                break;
            made++;
        }
        SCHECK(made == 24 && rc == -1 && SystemNative_GetErrNo() == RHU_GERR_ENOSPC, "shim-fdmap-store-bounds", "(the store does not hold 24 made directories and refuse the 25th with ENOSPC)");
        for (int i = 0; i < made; i++) {
            memcpy(p, "/tmp/bd-00", 11);
            p[8] = (char)('0' + i / 10);
            p[9] = (char)('0' + i % 10);
            SCHECK(SystemNative_RmDir(p) == 0, "shim-fdmap-store-bounds", "(rmdir)");
        }
        /* a path of 127 characters is a path; one of 128 is ENAMETOOLONG */
        memset(p, 'p', sizeof p);
        memcpy(p, "/tmp/", 5);
        p[127] = 0;
        intptr_t lf = SystemNative_Open(p, RHU_PAL_O_RDWR | RHU_PAL_O_CREAT | RHU_PAL_O_EXCL, 0x1A4);
        SCHECK(lf >= 0 && SystemNative_Close(lf) == 0 && SystemNative_Stat(p, &st) == 0 && SystemNative_Unlink(p) == 0, "shim-fdmap-store-bounds", "(a path of 127 characters was refused)");
        p[127] = 'p';
        p[128] = 0;
        lf = SystemNative_Open(p, RHU_PAL_O_RDWR | RHU_PAL_O_CREAT | RHU_PAL_O_EXCL, 0x1A4);
        SCHECK(lf == -1 && SystemNative_GetErrNo() == RHU_GERR_ENAMETOOLONG, "shim-fdmap-store-bounds", "(a path of 128 characters was not ENAMETOOLONG)");
        if (lf >= 0) {
            SystemNative_Close(lf);
            SystemNative_Unlink(p);
        }
        /* what the row's memory gives before the refused opens below: a file of two pages, sized and given back */
        intptr_t big = SystemNative_Open("/tmp/bfd-m.bin", RHU_PAL_O_RDWR | RHU_PAL_O_CREAT | RHU_PAL_O_TRUNC, 0x1A4);
        SCHECK(big >= 0 && SystemNative_FTruncate(big, 8192) == 0 && SystemNative_Close(big) == 0 && SystemNative_Unlink("/tmp/bfd-m.bin") == 0, "shim-fdmap-store-bounds", "(a file of two pages could not be sized and given back)");
        /* the descriptors: 29, each number in 3 to 15 or in 48 to 63, and one more is EMFILE */
        intptr_t held[30];
        int n = 0;
        for (int i = 0; i < 30; i++)
            held[i] = -2;
        int in_band = 1;
        held[0] = SystemNative_Open("/tmp/bfd.txt", RHU_PAL_O_RDWR | RHU_PAL_O_CREAT | RHU_PAL_O_TRUNC, 0x1A4);
        SCHECK(held[0] >= 0, "shim-fdmap-store-bounds", "(the file to hold)");
        for (n = 1; n < 30; n++) {
            held[n] = SystemNative_Open("/tmp/bfd.txt", RHU_PAL_O_RDONLY, 0);
            if (held[n] < 0)
                break;
        }
        for (int i = 0; i < n; i++)
            in_band &= (held[i] >= 3 && held[i] <= 15) || (held[i] >= 48 && held[i] <= 63);
        SCHECK(n == 29 && RHU_SHIM_FD_FILE_COUNT == 29 && held[29] == -1 && SystemNative_GetErrNo() == RHU_GERR_EMFILE, "shim-fdmap-store-bounds", "(the store does not serve 29 descriptors and refuse one more with EMFILE)");
        SCHECK(in_band, "shim-fdmap-store-bounds", "(a descriptor's number is outside 3 to 15 and 48 to 63)");
        /* with no descriptor free, a refused open of the process identity file gives back everything it took: four
         * thousand refused opens, sixteen times what the hosting layer's heap could carry if each kept its record */
        int kept = 0;
        for (uint32_t r = 0; r < 4096u && kept == 0; r++)
            kept = !(SystemNative_Open("/proc/self/stat", RHU_PAL_O_RDONLY, 0) == -1 && SystemNative_GetErrNo() == RHU_GERR_EMFILE);
        SCHECK(kept == 0, "shim-fdmap-store-bounds", "(a refused open of the identity file past the band did not stay EMFILE over four thousand tries)");
        for (int i = 0; i < n && i < 30; i++)
            SystemNative_Close(held[i]);
        big = SystemNative_Open("/tmp/bfd-m.bin", RHU_PAL_O_RDWR | RHU_PAL_O_CREAT | RHU_PAL_O_TRUNC, 0x1A4);
        SCHECK(big >= 0 && SystemNative_FTruncate(big, 8192) == 0, "shim-fdmap-store-bounds", "(the file of two pages cannot be sized again after the refused opens: they kept memory)");
        if (big >= 0)
            SystemNative_Close(big);
        SystemNative_Unlink("/tmp/bfd-m.bin");
        SCHECK(SystemNative_Unlink("/tmp/bfd.txt") == 0, "shim-fdmap-store-bounds", "(unlink)");
        pass("shim-fdmap-store-bounds", " (32 files and the 33rd ENOSPC; 24 made directories and the 25th ENOSPC; a path of 127 characters, and 128 ENAMETOOLONG; 29 descriptors in 3 to 15 and 48 to 63, and one more EMFILE; a refused open keeps nothing)");
    }

    /* THE ROW's POOL IN LIVE PAGES. The store's bytes are the hosting layer's heap, 64 KiB on this row, and a page
     * block is carved by the kernel heap's own aligned entry: the heap's header stands directly before the page, so
     * two pages never touch and a page takes 8 KiB of the heap's span -- seven or eight pages stand at once. A page
     * block padded by a lead of its own alignment takes 12 KiB of span and never gives more than five (six would
     * need 69632 bytes). Seven are asked. One page past the pool is refused by the storage rule, ENOSPC at the
     * truncation with the file's size untouched, and the pool carries the same pages again once all are given back. */
    {
        char p[16];
        int live[2] = {0, 0};
        int clean = 1;
        for (int round = 0; round < 2; round++) {
            int n = 0;
            for (; n < 32; n++) {
                memcpy(p, "/tmp/pg-00", 11);
                p[8] = (char)('0' + n / 10);
                p[9] = (char)('0' + n % 10);
                intptr_t f = SystemNative_Open(p, RHU_PAL_O_RDWR | RHU_PAL_O_CREAT | RHU_PAL_O_EXCL, 0x180);
                if (f < 0)
                    break;
                int32_t rc = SystemNative_FTruncate(f, 4096);
                int32_t e = rc == 0 ? 0 : SystemNative_GetErrNo();
                SystemNative_Close(f);
                if (rc != 0) {
                    clean &= rc == -1 && e == RHU_GERR_ENOSPC && SystemNative_Stat(p, &st) == 0 && st.Size == 0;
                    SystemNative_Unlink(p);
                    break;
                }
            }
            live[round] = n;
            for (int i = 0; i < n; i++) {
                memcpy(p, "/tmp/pg-00", 11);
                p[8] = (char)('0' + i / 10);
                p[9] = (char)('0' + i % 10);
                SystemNative_Unlink(p);
            }
        }
        SCHECK(live[0] >= 7, "shim-fdmap-store-pool", "(the row's pool does not carry seven page files at once: a page takes more of the heap than its bytes and one header)");
        SCHECK(clean, "shim-fdmap-store-pool", "(a page file past the pool was not refused with ENOSPC at the truncation, its size untouched)");
        SCHECK(live[1] == live[0], "shim-fdmap-store-pool", "(the pool does not carry the same pages again after all were given back)");
        pass("shim-fdmap-store-pool", " (seven page files or more stand at once on the row's 64 KiB; one past the pool is refused with ENOSPC at the truncation; the pool gives the same again)");
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
    uint32_t lo = 0;
    uint32_t len = 0;
    sigemptyset(&s);
    sigaddset(&s, g_actsig);
    pthread_sigmask(RHU_GLIBC_SIG_BLOCK, &s, NULL);
    /* The carve itself, from the kernel: the attribute call reports the
     * bounds the runtime's stack-depth helper needs, which begin lower. */
    rhu_thread_stack_bounds((int32_t)pthread_self(), &lo, &len);
    g_w_lo = lo;
    g_w_len = len;
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
    uint32_t lo = 0;
    uint32_t len = 0;
    /* the carve itself, from the kernel (see the spinning worker) */
    rhu_thread_stack_bounds((int32_t)pthread_self(), &lo, &len);
    g_w_lo = lo;
    g_w_len = len;
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
#include "rhu_rt_entry_net11.h"

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

/* The shim's event routine and the word a debugger writes (rhu_host.c). */
extern uint32_t __lamella_debug_events;
void rhu_debug_event_unhandled(void);

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

/* The class library's function for an exception that no handler takes. The
 * shim's event routine reaches it by a branch, with the registers it was
 * entered with; this stand-in hands them on the same way, so the record
 * reads the routine's own argument. */
static uint32_t g_unhandled_calls;
static void *g_unhandled_argument;

__attribute__((used)) static void host_unhandled_record(void *exception)
{
    g_unhandled_calls++;
    g_unhandled_argument = exception;
}

__attribute__((naked)) void OnUnhandledException(void)
{
    __asm__("b host_unhandled_record");
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
    static const uint32_t words[4] = { 0u, (uint32_t)RHU_DEBUG_EVENTS_MAGIC << 16, 1u << RHU_DEBUG_EVENT_UNHANDLED,
                                       ((uint32_t)(RHU_DEBUG_EVENTS_MAGIC + 1) << 16) | (1u << RHU_DEBUG_EVENT_UNHANDLED) };
    void (*routine)(void *) = NULL;
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
    CHECK(g_classlib[RHU_RT_CLASSLIB_SLOT_OnUnhandledException] == (void *)(uintptr_t)&rhu_debug_event_unhandled, "shim-host-classlib", "(slot 7 is not the shim's routine for an unhandled exception)");
    for (i = RHU_RT_CLASSLIB_EXPORTS; i < RHU_RT_CLASSLIB_SLOTS; i++)
        CHECK(g_classlib[i] == NULL, "shim-host-classlib", "(a slot past the exports is not null)");
    pass("shim-host-classlib", " (twelve slots; seven exports by the bootstrapper's names, the shim's routine for an unhandled exception in slot 7, four null)");

    /* The routine in slot 7 while the debug events word does not ask: clear,
     * the magic without the event's bit, the bit without the magic, another
     * magic with the bit. Each time it reaches the class library's function
     * with the argument it was given. */
    routine = (void (*)(void *))(uintptr_t)g_classlib[RHU_RT_CLASSLIB_SLOT_OnUnhandledException];
    CHECK(__lamella_debug_events == 0u, "shim-host-debug-event", "(the debug events word is not zero at the managed entry)");
    for (i = 0; i < 4; i++) {
        __lamella_debug_events = words[i];
        routine((void *)(uintptr_t)(0x00176170u + (uint32_t)i));
        __lamella_debug_events = 0u;
        CHECK(g_unhandled_calls == (uint32_t)i + 1u && g_unhandled_argument == (void *)(uintptr_t)(0x00176170u + (uint32_t)i), "shim-host-debug-event", "(the routine did not reach the class library's function with its argument)");
    }
    pass("shim-host-debug-event", " (with the debug events word clear the routine in slot 7 reaches the class library's function with its argument; so it does while the word holds the magic without the event's bit, the bit without the magic, and another magic with the bit)");
#ifdef RHU_SHIMTEST_HOST_DEBUG_EVENT
    /* The word set as a debugger sets it. The routine's BKPT follows, and with
     * no debugger on the core the kernel reports the fault at that instruction;
     * a routine that comes back executed none. */
    out("shimtest: the debug events word set to the magic and the unhandled event's bit; the routine's BKPT follows");
    __lamella_debug_events = ((uint32_t)RHU_DEBUG_EVENTS_MAGIC << 16) | (1u << RHU_DEBUG_EVENT_UNHANDLED);
    routine((void *)(uintptr_t)0x00176176u);
    return fail("shim-host-debug-event", "(the routine came back with the event asked for: no BKPT was executed)");
#endif

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
     * run (the rate request), under the kernel's contract for the claim
     * (rhu.h, the I2C paragraph): a served rate claims the bus and the same
     * rate again answers 0; another served rate (400 kHz, which the ladder's
     * row makes) is refused EBUSY and the node reads the running rate back; a
     * rate outside the set is refused ENOTSUP before the bus's state. The
     * request is made on a fresh node of the same bus: the claim is the bus's,
     * not the node's, and the transaction then runs through the first node. */
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
        CHECK(ioctl(fr, BD_I2C_RATE, &hz) == -1 && BD_ERRNO_IS(RHU_GERR_EBUSY) && hz == 100000u, "shim-busdev-rate", "(a second served rate was not refused EBUSY with the running rate read back)");
        hz = 250000u;
        CHECK(ioctl(fr, BD_I2C_RATE, &hz) == -1 && BD_ERRNO_IS(RHU_GERR_ENOTSUP) && hz == 250000u, "shim-busdev-rate", "(a rate outside the set was not refused ENOTSUP with the word untouched)");
        CHECK(close(fr) == 0, "shim-busdev-rate", "(the second node's close refused)");
        pass("shim-busdev-rate", " (the one RHU request on the I2C node: a served rate claims the bus for the run and reads back, the same rate again 0, a second served rate EBUSY with the running rate read back, a rate outside the set ENOTSUP before the state with the word untouched, NULL EFAULT, 0 EINVAL, the SPI node ENOTTY)");
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

#ifdef RHU_SHIMTEST_LADDER_PWMROWS

/* --- the PWM device files over a board row's own line table, from a
 * native caller's side. A channel's name is the line's number, and the
 * kernel bounds a line's number by the row's line space alone, so the unit
 * holds no bound of its own on a name. This ladder's unit object is the
 * unit's source compiled with the kernel's eight PWM functions and the
 * shim's diagnostic function renamed on the compile line; the tenant
 * defines the nine and answers as the kernel with the PWM-capable lines of
 * the case's row, in the order of the kernel's facts (PWM_LINES). The unit
 * computes its channel names once, so one image is one row. The case's rows
 * number a line by port and pin, so their tables carry numbers that no
 * small table indexed by the name could hold. The tables below are tied to
 * the kernel's facts by the hosted-runtime gate's row tie. --- */
int32_t rows_pwm_count(void);
int32_t rows_pwm_line(uint32_t index);
int32_t rows_pwm_claim(uint32_t line);
int32_t rows_pwm_set(int32_t h, uint64_t period_ns, uint64_t duty_ns, uint32_t polarity);
int32_t rows_pwm_period(int32_t h, uint64_t *achieved_ns);
int32_t rows_pwm_enable(int32_t h, uint32_t on);
int32_t rows_pwm_release(int32_t h);
int32_t rows_pwm_info(int32_t h, uint32_t *timer, uint32_t *channel, uint32_t *clock_hz);
void rows_shim_log(const char *fmt, ...);

#if defined(RHU_SHIMTEST_PWMROWS_F769)
#define PR_ROW "st-stm32f769i-disco"
static const uint16_t pr_lines[] = { 86, 40, 87, 118, 11, 31 }; /* PWM_LINES of st-stm32f769i-disco */
#elif defined(RHU_SHIMTEST_PWMROWS_U5A5)
#define PR_ROW "st-nucleo-u5a5zj-q"
static const uint16_t pr_lines[] = { 77, 75, 73, 63, 62, 7 }; /* PWM_LINES of st-nucleo-u5a5zj-q */
#else
#error "the PWM rows ladder names its row (RHU_SHIMTEST_PWMROWS_F769 or RHU_SHIMTEST_PWMROWS_U5A5)"
#endif
#define PR_COUNT ((int32_t)(sizeof pr_lines / sizeof pr_lines[0]))
#define PR_HELD_MAX 16
#define PR_CHIP "/sys/class/pwm/pwmchip0"
#define PR_HANDLE 0x100
#define PR_ERRNO_IS(e) (SystemNative_GetErrNo() == (e))
_Static_assert(sizeof pr_lines / sizeof pr_lines[0] <= PR_HELD_MAX, "the row's table is longer than the tenant's record of its claims");

static uint8_t pr_held[PR_HELD_MAX];
static uint64_t pr_period[PR_HELD_MAX];
/* The format of the unit's last diagnostic line. */
static const char *pr_log;

/* The table's index of a line, -1 for a line the row does not list. */
static int32_t pr_at(uint32_t line)
{
    for (int32_t i = 0; i < PR_COUNT; i++) {
        if ((uint32_t)pr_lines[i] == line)
            return i;
    }
    return -1;
}

int32_t rows_pwm_count(void)
{
    return PR_COUNT;
}

int32_t rows_pwm_line(uint32_t index)
{
    return index < (uint32_t)PR_COUNT ? (int32_t)pr_lines[index] : RHU_EINVAL;
}

int32_t rows_pwm_claim(uint32_t line)
{
    int32_t i = pr_at(line);
    if (i < 0)
        return RHU_EINVAL;
    if (pr_held[i])
        return RHU_EBUSY;
    pr_held[i] = 1;
    return PR_HANDLE + i;
}

int32_t rows_pwm_set(int32_t h, uint64_t period_ns, uint64_t duty_ns, uint32_t polarity)
{
    (void)polarity;
    if (h < PR_HANDLE || h >= PR_HANDLE + PR_COUNT || period_ns == 0 || duty_ns > period_ns)
        return RHU_EINVAL;
    pr_period[h - PR_HANDLE] = period_ns;
    return 0;
}

int32_t rows_pwm_period(int32_t h, uint64_t *achieved_ns)
{
    if (h < PR_HANDLE || h >= PR_HANDLE + PR_COUNT)
        return RHU_EINVAL;
    *achieved_ns = pr_period[h - PR_HANDLE];
    return 0;
}

int32_t rows_pwm_enable(int32_t h, uint32_t on)
{
    (void)on;
    return h < PR_HANDLE || h >= PR_HANDLE + PR_COUNT ? RHU_EINVAL : 0;
}

int32_t rows_pwm_release(int32_t h)
{
    if (h < PR_HANDLE || h >= PR_HANDLE + PR_COUNT)
        return RHU_EINVAL;
    pr_held[h - PR_HANDLE] = 0;
    pr_period[h - PR_HANDLE] = 0;
    return 0;
}

int32_t rows_pwm_info(int32_t h, uint32_t *timer, uint32_t *channel, uint32_t *clock_hz)
{
    if (h < PR_HANDLE || h >= PR_HANDLE + PR_COUNT)
        return RHU_EINVAL;
    *timer = (uint32_t)(h - PR_HANDLE);
    *channel = 0;
    *clock_hz = 1000000u;
    return 0;
}

void rows_shim_log(const char *fmt, ...)
{
    pr_log = fmt;
}

/* A number's decimal digits at p; their count. */
static int32_t pr_digits(char *p, uint32_t v)
{
    char t[10];
    int32_t n = 0;
    do {
        t[n++] = (char)('0' + v % 10u);
        v /= 10u;
    } while (v != 0u);
    for (int32_t i = 0; i < n; i++)
        p[i] = t[n - 1 - i];
    return n;
}

/* A channel's path: the chip, the channel's directory by the line's
 * number, and a file's name with its slash ("" for the directory). */
static const char *pr_path(char *path, uint32_t line, const char *file)
{
    size_t n = strlen(PR_CHIP "/pwm");
    memcpy(path, PR_CHIP "/pwm", n);
    n += (size_t)pr_digits(path + n, line);
    memcpy(path + n, file, strlen(file) + 1);
    return path;
}

/* One value written to a node through the store, as the class writes one:
 * the count, or -1 with the write's errno kept past the close. */
static int32_t pr_put(const char *path, const char *text)
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
 * terminated, or -1. */
static int32_t pr_get(const char *path, char *buf, int32_t cap)
{
    intptr_t fd = SystemNative_Open(path, RHU_PAL_O_RDONLY, 0);
    if (fd < 0)
        return -1;
    int32_t n = SystemNative_Read(fd, buf, cap - 1);
    SystemNative_Close(fd);
    if (n >= 0)
        buf[n] = 0;
    return n;
}

/* A listed line through a channel's whole life: 0, or the step that
 * refused -- 1 the export, 2 the period, 3 the duty, 4 the period read
 * back, 5 the unexport, 6 the channel's directory answering after it. */
static int pr_life(uint32_t line)
{
    char path[64], text[12], back[24];
    rhu_pal_filestatus st;
    int32_t n = pr_digits(text, line);
    text[n] = 0;
    if (pr_put(PR_CHIP "/export", text) != n)
        return 1;
    if (pr_put(pr_path(path, line, "/period"), "1000000") != 7)
        return 2;
    if (pr_put(pr_path(path, line, "/duty_cycle"), "250000") != 6)
        return 3;
    if (pr_get(pr_path(path, line, "/period"), back, (int32_t)sizeof back) != 8 || strcmp(back, "1000000\n") != 0)
        return 4;
    if (pr_put(PR_CHIP "/unexport", text) != n)
        return 5;
    if (SystemNative_Stat(pr_path(path, line, ""), &st) != -1)
        return 6;
    return 0;
}

static int pwmrows_ladder(void)
{
    char text[12], back[24], want[12], detail[96];
    out("shimtest-pwmrows: the PWM device files over a board row's own line table (" PR_ROW "), the kernel's PWM functions answered by the tenant");
    CHECK(rhu_shim_init() == 0, "shim-pwmrows-init", "(initialization refused)");
    pass("shim-pwmrows-init", " (the tenant answers as the kernel with the row's PWM-capable lines)");

    /* npwm: the highest listed line plus one. */
    uint32_t top = 0;
    for (int32_t i = 0; i < PR_COUNT; i++) {
        if ((uint32_t)pr_lines[i] > top)
            top = (uint32_t)pr_lines[i];
    }
    int32_t n = pr_digits(want, top + 1u);
    want[n] = '\n';
    want[n + 1] = 0;
    CHECK(pr_get(PR_CHIP "/npwm", back, (int32_t)sizeof back) == n + 1 && strcmp(back, want) == 0, "shim-pwmrows-npwm", "(npwm did not read the highest listed line plus one)");
    pass("shim-pwmrows-npwm", " (the highest listed line plus one)");

    /* Every listed line through a channel's whole life; the lines that a
     * step refused are named in one line, in the table's order. */
    static const char head[] = "(refused at a step of a channel's life, the lines";
    size_t at = sizeof head - 1;
    memcpy(detail, head, at);
    int refused = 0;
    for (int32_t i = 0; i < PR_COUNT; i++) {
        if (pr_life(pr_lines[i]) != 0) {
            detail[at++] = ' ';
            at += (size_t)pr_digits(detail + at, pr_lines[i]);
            refused++;
        }
    }
    detail[at++] = ')';
    detail[at] = 0;
    if (refused != 0)
        return fail("shim-pwmrows-lines", detail);
    for (int32_t i = 0; i < PR_COUNT; i++)
        CHECK(pr_held[i] == 0, "shim-pwmrows-lines", "(a line is still claimed after its unexport)");
    pass("shim-pwmrows-lines", " (every listed line exported under its own number, its period and its duty set, its period read back, unexported and its directory gone; every claim released)");

    /* The names that stand in no table. A hole: the first number under the
     * top that the row does not list. */
    uint32_t hole = top;
    while (hole > 0u && pr_at(hole) >= 0)
        hole--;
    CHECK(pr_at(hole) < 0, "shim-pwmrows-names", "(the row's table leaves no hole under its top)");
    n = pr_digits(text, hole);
    text[n] = 0;
    pr_log = NULL;
    CHECK(pr_put(PR_CHIP "/export", text) == -1 && PR_ERRNO_IS(RHU_GERR_ENODEV), "shim-pwmrows-names", "(a hole under the top was not refused ENODEV)");
    CHECK(pr_log != NULL && strcmp(pr_log, "%s/export: line %u is not a PWM line of this row (ENODEV)") == 0, "shim-pwmrows-names", "(the hole's refusal did not say that the line is no PWM line of the row)");
    n = pr_digits(text, top + 1u);
    text[n] = 0;
    pr_log = NULL;
    CHECK(pr_put(PR_CHIP "/export", text) == -1 && PR_ERRNO_IS(RHU_GERR_ENODEV), "shim-pwmrows-names", "(the name past the top was not refused ENODEV)");
    CHECK(pr_log != NULL && strcmp(pr_log, "%s/export: line %u is past this chip's %u channel names (ENODEV)") == 0, "shim-pwmrows-names", "(the refusal of the name past the top did not say so)");
    CHECK(pr_put(PR_CHIP "/export", "65536") == -1 && PR_ERRNO_IS(RHU_GERR_ENODEV), "shim-pwmrows-names", "(a name wider than a line's number was not refused ENODEV)");
    pass("shim-pwmrows-names", " (a hole under the top ENODEV by the kernel's judgement, under the line that says so; the name past the top ENODEV under its own line; a name wider than a line's number ENODEV)");

    out("SHIMTEST-PWMROWS: ALL GOOD (the PWM device files over a board row's own line table, from a native caller's side)");
    return 0;
}

#endif /* RHU_SHIMTEST_LADDER_PWMROWS */

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
int gpiod_line_is_free(struct gpiod_line *line);
int gpiod_line_direction(struct gpiod_line *line);
int gpiod_line_event_wait(struct gpiod_line *line, const struct gd_timespec *timeout);
int gpiod_line_event_read(struct gpiod_line *line, struct gd_event *event);

#define GD_FLAG_PULL_DOWN (1 << 4)
#define GD_FLAG_PULL_UP (1 << 5)
#define GD_BIAS_AS_IS 1
#define GD_BIAS_PULL_UP 3
#define GD_DIRECTION_INPUT 1
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

/* The reopen case's blocks: every block of the heap that a request can
 * take, held while the case runs. */
#define GD_HOLD_MAX 128u
static void *g_gd_hold[GD_HOLD_MAX];
static uint32_t g_gd_held;

/* Takes every free block that a request can take, largest first, and
 * states the size of the first one taken; 0 when the heap holds more
 * blocks than the case can hold. The largest request stays under the heap
 * of this ladder's row (the board's KHEAP_BYTES), so the kernel refuses
 * none for its size alone. */
static int gd_take_all(size_t *first)
{
    for (size_t size = 32768u; size >= 8u; size >>= 1) {
        for (;;) {
            void *p = malloc(size);
            if (p == NULL)
                break;
            if (g_gd_held == GD_HOLD_MAX) {
                free(p);
                return 0;
            }
            if (first != NULL && g_gd_held == 0u)
                *first = size;
            g_gd_hold[g_gd_held++] = p;
        }
    }
    return 1;
}

/* The reopen case's work: the heap shaped to a room of exactly a table of
 * lines, a smaller room behind it and one crumb of a chip object's size,
 * every other block taken; then an open, its close and the open that
 * follows. The heap is given back before a line goes out, so a refusal
 * never leaves the ladder without memory. NULL when every clause held,
 * the clause that did not otherwise. */
static const char *gd_reopen(void)
{
    /* A table's size: a line object's size, read from two handles of one
     * chip, times the row's lines. */
    struct gpiod_chip *probe = gpiod_chip_open_by_number(0);
    struct gpiod_line *l0 = gpiod_chip_get_line(probe, 0);
    struct gpiod_line *l1 = gpiod_chip_get_line(probe, 1);
    size_t table = l0 != NULL && l1 != NULL ? (size_t)((uintptr_t)l1 - (uintptr_t)l0) * (size_t)rhu_gpio_count() : 0u;
    gpiod_chip_close(probe);
    if (table == 0u)
        return "(the chip did not open before the heap was shaped)";

    const char *why = NULL;
    void *fence1 = NULL;
    void *fence2 = NULL;
    uintptr_t first_at = 0;
    uintptr_t room_at = 0;
    size_t first_size = 0;
    g_gd_held = 0;
    if (!gd_take_all(&first_size) || g_gd_held == 0u) {
        why = "(the heap's free blocks are more than the case holds, or none)";
    } else {
        /* The block taken first, the largest, is given back and cut in
         * address order: a crumb of a chip object's size, a fence, a room
         * of exactly a table, a fence, a room of half a table; what is
         * left of it is taken again. With the crumb and the two rooms
         * given back the heap holds those three blocks and no other that
         * a request can take. */
        void *first = g_gd_hold[0];
        first_at = (uintptr_t)first;
        g_gd_hold[0] = g_gd_hold[--g_gd_held];
        free(first);
        void *crumb = malloc(16u);
        fence1 = malloc(8u);
        void *room = malloc(table);
        fence2 = malloc(8u);
        void *less = malloc(table / 2u);
        room_at = (uintptr_t)room;
        int cut = crumb != NULL && fence1 != NULL && room != NULL && fence2 != NULL && less != NULL;
        cut = cut && (uintptr_t)crumb < (uintptr_t)fence1 && (uintptr_t)fence1 < room_at && room_at < (uintptr_t)fence2 && (uintptr_t)fence2 < (uintptr_t)less;
        if (!cut || !gd_take_all(NULL))
            why = "(the heap's largest block was not cut into the shape)";
        free(crumb);
        free(room);
        free(less);
    }
    if (why == NULL) {
        void *over = malloc(table + 8u);
        void *exact = malloc(table);
        if (over != NULL || (uintptr_t)exact != room_at)
            why = "(the shaped heap does not hold a room of exactly a table)";
        free(over);
        free(exact);
    }
    if (why == NULL) {
        struct gpiod_chip *a = gpiod_chip_open_by_number(0);
        if (a == NULL) {
            why = GD_ERRNO_IS(RHU_GERR_ENOMEM) ? "(an open was refused for memory on a heap that holds a room of exactly a table)" : "(an open on the shaped heap failed, and not for memory)";
        } else {
            struct gpiod_line *la = gpiod_chip_get_line(a, 0);
            int asked = la != NULL && gpiod_line_request_output(la, "first", 0) == 0;
            gpiod_chip_close(a);
            struct gpiod_chip *b = gpiod_chip_open_by_number(0);
            struct gpiod_line *lb = gpiod_chip_get_line(b, 0);
            if (!asked)
                why = "(the line through the first open was refused)";
            else if (b == NULL)
                why = "(the open that follows the last close was refused)";
            else if (lb == NULL || gpiod_line_is_free(lb) != 1 || gpiod_line_direction(lb) != GD_DIRECTION_INPUT || gpiod_line_bias(lb) != GD_BIAS_AS_IS)
                why = "(a reopened chip's line does not read as a fresh table's)";
            gpiod_chip_close(b);
            void *again = malloc(table);
            if (why == NULL && (uintptr_t)again != room_at)
                why = "(the last close did not give the table's room back whole)";
            free(again);
        }
    }
    while (g_gd_held != 0u)
        free(g_gd_hold[--g_gd_held]);
    free(fence1);
    free(fence2);
    /* The heap as it stood: the block taken first is given at its address
     * again. */
    if (first_size != 0u) {
        void *back = malloc(first_size);
        if (why == NULL && (uintptr_t)back != first_at)
            why = "(the heap is not as it stood before the case)";
        free(back);
    }
    return why;
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
        CHECK(a != NULL && b != NULL && a != b && la != NULL && lb != NULL && la != lb, "shim-gpiod-opens", "(two opens share one chip or one line handle)");
        /* One table of lines for the chip, whatever the opens: the two chips' handles of line 0 lie inside one
         * line's object, closer than two lines of one chip do; an open that made a table of its own would put
         * them a table apart. */
        struct gpiod_line *la1 = gpiod_chip_get_line(a, 1);
        uintptr_t stride = (uintptr_t)la1 - (uintptr_t)la;
        uintptr_t apart = (uintptr_t)lb > (uintptr_t)la ? (uintptr_t)lb - (uintptr_t)la : (uintptr_t)la - (uintptr_t)lb;
        CHECK(la1 != NULL && apart < stride, "shim-gpiod-opens", "(a second open took a second table of lines)");
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
        /* An open on a heap that holds a room of exactly a table of lines,
         * its close and the open that follows it: the unit asks for its
         * table before any smaller block of its own, and for the pulls only
         * where none stand, so each open finds the room that a close gave
         * back. The build that asks its small blocks first is refused for
         * memory at the first of the opens. */
        const char *why = gd_reopen();
        CHECK(why == NULL, "shim-gpiod-reopen", why);
        pass("shim-gpiod-reopen", " (on a heap that holds a room of exactly a table of lines, a smaller room and a crumb: an open, its close and the open that follows are given; the reopened chip's line reads as a fresh table's; the heap as it stood)");
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
/* The low-power port's name, by link words. The serial unit names a block
 * /dev/ttyLP<n> where its link word __rhu_uart_lowpower_blocks has the
 * block's bit, and serves the folder /sys/class/tty on such a row. This
 * tenant reads what it expects from a word of its own,
 * __rhu_shimtest_uart_lowpower, the same mask (a weak absolute symbol: no
 * word in this image reads NULL), so the gate links it both ways: with
 * both words, where every check below holds, and with the tenant's word
 * over the unit's word at zero, where the first check refuses. Without the
 * tenant's word the ladder below runs as it stands. The row's blocks are
 * 0, the kernel's console, and 1; the word names block 1. */
extern const char __rhu_shimtest_uart_lowpower[] __attribute__((weak));

static int ttydev_lowpower_names(void)
{
    rhu_pal_filestatus st;
    rhu_pal_directoryentry e;
    out("shimtest-ttydev: the low-power port's name by the link word, on the row's UART blocks");
    CHECK(rhu_shim_init() == 0, "shim-ttydev-init", "(initialization refused)");
    pass("shim-ttydev-init", " (the shim initialized)");
    /* the standard class's first search: the folder, one entry for each port, the one folder under each entry */
    CHECK(SystemNative_Stat("/sys/class/tty", &st) == 0 && (st.Mode & 0xF000) == RHU_PAL_S_IFDIR, "shim-ttydev-lowpower-names", "(/sys/class/tty is not a folder on a row with a low-power block)");
    CHECK((uintptr_t)__rhu_shimtest_uart_lowpower == 2u, "shim-ttydev-lowpower-names", "(the tenant's word does not name block 1 alone)");
    {
        void *dir = SystemNative_OpenDir("/sys/class/tty");
        CHECK(dir != NULL, "shim-ttydev-lowpower-names", "(/sys/class/tty did not open as a directory)");
        int seen = 0, order_ok = 1;
        while (SystemNative_ReadDir(dir, &e) == 0) {
            if (e.InodeType != RHU_PAL_DT_DIR || (seen == 0 && strcmp(e.Name, "ttyS0") != 0) || (seen == 1 && strcmp(e.Name, "ttyLP1") != 0))
                order_ok = 0;
            seen++;
        }
        CHECK(SystemNative_CloseDir(dir) == 0, "shim-ttydev-lowpower-names", "(closedir of /sys/class/tty)");
        CHECK(seen == 2 && order_ok, "shim-ttydev-lowpower-names", "(the listing of /sys/class/tty did not show ttyS0 then ttyLP1 as directories and nothing else)");
    }
    CHECK(SystemNative_Stat("/sys/class/tty/ttyLP1/device/tty", &st) == 0 && (st.Mode & 0xF000) == RHU_PAL_S_IFDIR, "shim-ttydev-lowpower-names", "(ttyLP1's entry has no folder device/tty)");
    CHECK(SystemNative_Stat("/sys/class/tty/ttyS0/device/of_node", &st) == 0 && (st.Mode & 0xF000) == RHU_PAL_S_IFDIR, "shim-ttydev-lowpower-names", "(ttyS0's entry has no folder device/of_node)");
    CHECK(SystemNative_Stat("/sys/class/tty/ttyLP1/device/of_node", &st) == -1 && SystemNative_GetErrNo() == RHU_GERR_ENOENT, "shim-ttydev-lowpower-names", "(ttyLP1's entry has a folder device/of_node)");
    CHECK(SystemNative_Stat("/sys/class/tty/ttyS1", &st) == -1 && SystemNative_GetErrNo() == RHU_GERR_ENOENT, "shim-ttydev-lowpower-names", "(/sys/class/tty has an entry ttyS1)");
    {
        void *dir = SystemNative_OpenDir("/sys/class/tty/ttyLP1/device");
        CHECK(dir != NULL, "shim-ttydev-lowpower-names", "(ttyLP1's device folder did not open as a directory)");
        int seen = 0, order_ok = 1;
        while (SystemNative_ReadDir(dir, &e) == 0) {
            if (e.InodeType != RHU_PAL_DT_DIR || strcmp(e.Name, "tty") != 0)
                order_ok = 0;
            seen++;
        }
        CHECK(SystemNative_CloseDir(dir) == 0, "shim-ttydev-lowpower-names", "(closedir of the device folder)");
        CHECK(seen == 1 && order_ok, "shim-ttydev-lowpower-names", "(ttyLP1's device folder did not list the one folder tty)");
    }
    /* the search of /dev: the same two names, as character devices */
    {
        void *dir = SystemNative_OpenDir("/dev");
        CHECK(dir != NULL, "shim-ttydev-lowpower-names", "(/dev did not open as a directory)");
        int seen = 0, chr = 0, order_ok = 1;
        while (SystemNative_ReadDir(dir, &e) == 0) {
            if (strlen(e.Name) < 4 || memcmp(e.Name, "tty", 3) != 0)
                continue;
            if (e.InodeType == RHU_PAL_DT_CHR)
                chr++;
            if ((seen == 0 && strcmp(e.Name, "ttyS0") != 0) || (seen == 1 && strcmp(e.Name, "ttyLP1") != 0))
                order_ok = 0;
            seen++;
        }
        CHECK(SystemNative_CloseDir(dir) == 0, "shim-ttydev-lowpower-names", "(closedir of /dev)");
        CHECK(seen == 2 && chr == 2 && order_ok, "shim-ttydev-lowpower-names", "(the listing of /dev did not show ttyS0 then ttyLP1 as character devices and no other tty name)");
    }
    CHECK(SystemNative_Stat("/dev/ttyLP1", &st) == 0 && (st.Mode & 0xF000) == RHU_PAL_S_IFCHR, "shim-ttydev-lowpower-names", "(ttyLP1's status is not a character device)");
    CHECK(SystemNative_Access("/dev/ttyLP1", RHU_PAL_R_OK | RHU_PAL_W_OK) == 0, "shim-ttydev-lowpower-names", "(ttyLP1 is not readable and writable)");
    CHECK(SystemNative_Stat("/dev/ttyS1", &st) == -1 && SystemNative_GetErrNo() == RHU_GERR_ENOENT, "shim-ttydev-lowpower-names", "(the name ttyS1 of a low-power block is not absent)");
    CHECK(SystemNative_Access("/dev/ttyS1", RHU_PAL_F_OK) == -1, "shim-ttydev-lowpower-names", "(the name ttyS1 of a low-power block is present to access)");
    CHECK(SystemNative_Stat("/dev/ttyLP0", &st) == -1 && SystemNative_GetErrNo() == RHU_GERR_ENOENT, "shim-ttydev-lowpower-names", "(the name ttyLP0 of an ordinary block is not absent)");
    /* the open: the block's own name opens it and the alias names the same block; the other kind's name answers ENOENT */
    CHECK(open("/dev/ttyS1", TD_FLAGS) == -1 && TD_ERRNO_IS(RHU_GERR_ENOENT), "shim-ttydev-lowpower-names", "(the name ttyS1 of a low-power block was not refused ENOENT)");
    CHECK(open("/dev/ttyLP0", TD_FLAGS) == -1 && TD_ERRNO_IS(RHU_GERR_ENOENT), "shim-ttydev-lowpower-names", "(the name ttyLP0 of an ordinary block was not refused ENOENT)");
    int fd = open("/dev/ttyLP1", TD_FLAGS);
    CHECK(TD_IN_BAND(fd), "shim-ttydev-lowpower-names", "(/dev/ttyLP1 did not open)");
    CHECK(open(TD_ALIAS, TD_FLAGS) == -1 && TD_ERRNO_IS(RHU_GERR_EBUSY), "shim-ttydev-lowpower-names", "(the alias of the held port was not refused EBUSY)");
    CHECK(close(fd) == 0, "shim-ttydev-lowpower-names", "(the close refused)");
    pass("shim-ttydev-lowpower-names", " (/sys/class/tty a folder that lists ttyS0 then ttyLP1, device/of_node under the first and device/tty under the second; /dev lists the same two names as character devices; /dev/ttyLP1 opens and its alias names the same block; ttyS1 and ttyLP0 answer ENOENT)");
    out("SHIMTEST-TTYDEV: ALL GOOD (the low-power port's name and its listing, by the link word)");
    return 0;
}

static int ttydev_ladder(void)
{
    if ((uintptr_t)__rhu_shimtest_uart_lowpower != 0)
        return ttydev_lowpower_names();
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

#ifdef RHU_SHIMTEST_LADDER_SDFS
#include "rhu_pal_surface_net11.h"
int strncmp(const char *a, const char *b, size_t n);
char *strcpy(char *dst, const char *src);
char *strncpy(char *dst, const char *src, size_t n);
char *strchr(const char *s, int c);
int32_t SystemNative_UTimensat(const char *path, rhu_pal_timespec *times);
int32_t SystemNative_FUTimens(intptr_t fd, rhu_pal_timespec *times);
#endif
#ifdef RHU_SHIMTEST_LADDER_SDFS

/* --- the card's files from a native caller's side: the paths under
 * /media/sd0 and the storage band, over a SCRIPTED kernel. The emulator
 * row has no card socket, so this ladder serves the kernel's file family
 * itself through the unit's weak hook: a volume of a few files in memory
 * in the kernel's own shapes (the records, the two handle tables, the
 * codes at Linux's values), a mount the ladder raises and drops, a
 * card-out state that answers EIO on the handles open at the time, a full
 * state that answers ENOSPC on a write. What the ladder reads through the
 * runtime's portability verbs is the unit's own work: the routing of a
 * path, the storage band and its exhaustion, the dup on one handle, the
 * errno map by sign, the status and directory records, the mount-point
 * list and the space and type-name queries, and the store standing beside
 * the card untouched. --- */
#define SK_FILES 12
#define SK_DATA 2048
#define SK_HANDLES 16
#define SK_DIRS 4
#define SK_PATH 64
#define SK_CLUSTER 4096u
#define SK_CLUSTERS 1000u

struct sk_file {
    uint8_t used, dir, ro;
    char key[SK_PATH]; /* the path with the point dropped: "/a.txt", "/d", "/d/b.txt" */
    uint32_t size;
    uint8_t data[SK_DATA];
};
struct sk_handle {
    uint8_t used, gone;
    int32_t file;
    uint32_t pos, flags;
};
struct sk_dir {
    uint8_t used, gone;
    char key[SK_PATH];
    int idx; /* -2 ".", -1 "..", then the table's rows */
};
static struct sk_file g_skf[SK_FILES];
static struct sk_handle g_skh[SK_HANDLES];
static struct sk_dir g_skd[SK_DIRS];
static int g_sk_mounted, g_sk_full;
static uint32_t g_sk_opens, g_sk_closes, g_sk_dopens, g_sk_dcloses, g_sk_syncs;

/* The path's key: the point dropped, one trailing slash dropped, the root "/". */
static int32_t sk_key(const char *path, char *key)
{
    if (path == NULL || path[0] != '/')
        return RHU_EINVAL;
    if (!g_sk_mounted)
        return RHU_ENOENT;
    if (strncmp(path, "/media/sd0", 10) != 0 || (path[10] != 0 && path[10] != '/'))
        return RHU_ENOENT;
    const char *rest = path + 10;
    size_t n = strlen(rest);
    if (n == 0 || (n == 1 && rest[0] == '/')) {
        key[0] = '/';
        key[1] = 0;
        return 0;
    }
    if (n >= SK_PATH)
        return RHU_ENAMETOOLONG;
    memcpy(key, rest, n + 1);
    if (n > 1 && key[n - 1] == '/')
        key[n - 1] = 0;
    return 0;
}

static int sk_find(const char *key)
{
    for (int i = 0; i < SK_FILES; i++) {
        if (g_skf[i].used != 0 && strcmp(g_skf[i].key, key) == 0)
            return i;
    }
    return -1;
}

static int sk_is_dir_key(const char *key)
{
    if (strcmp(key, "/") == 0)
        return 1;
    int i = sk_find(key);
    return i >= 0 && g_skf[i].dir != 0;
}

static int sk_parent_exists(const char *key)
{
    char parent[SK_PATH];
    size_t n = strlen(key);
    while (n > 0 && key[n - 1] != '/')
        n--;
    if (n <= 1)
        return 1;
    memcpy(parent, key, n - 1);
    parent[n - 1] = 0;
    return sk_is_dir_key(parent);
}

static int sk_child_of(const char *key, const char *dir)
{
    size_t dl = strlen(dir);
    if (dl == 1)
        dl = 0;
    if (strncmp(key, dir, dl) != 0 || key[dl] != '/')
        return 0;
    return strchr(key + dl + 1, '/') == NULL;
}

static int sk_has_child(const char *dir)
{
    for (int i = 0; i < SK_FILES; i++) {
        if (g_skf[i].used != 0 && sk_child_of(g_skf[i].key, dir))
            return 1;
    }
    return 0;
}

static int32_t sk_mount_count(void)
{
    return 1;
}

static int32_t sk_mount(uint32_t index, rhu_fs_mount_desc *out)
{
    if (index != 0 || out == NULL)
        return RHU_EINVAL;
    memset(out, 0, sizeof *out);
    memcpy(out->point, "/media/sd0", 11);
    out->state = g_sk_mounted ? RHU_FS_MOUNTED : RHU_FS_NO_CARD;
    if (g_sk_mounted) {
        out->fat_width = 32;
        out->cluster_bytes = SK_CLUSTER;
        out->clusters = SK_CLUSTERS;
        out->free_clusters = g_sk_full ? 0 : SK_CLUSTERS - 100;
        memcpy(out->label, "SCRIPTED", 9);
    }
    return 0;
}

static int32_t sk_open(const char *path, uint32_t flags)
{
    char key[SK_PATH];
    if ((flags & ~63u) != 0 || (flags & (RHU_FS_O_READ | RHU_FS_O_WRITE)) == 0)
        return RHU_EINVAL;
    if ((flags & (RHU_FS_O_TRUNC | RHU_FS_O_APPEND)) != 0 && (flags & RHU_FS_O_WRITE) == 0)
        return RHU_EINVAL;
    int32_t r = sk_key(path, key);
    if (r != 0)
        return r;
    if (sk_is_dir_key(key))
        return RHU_EISDIR;
    int fi = sk_find(key);
    if (fi < 0) {
        if ((flags & RHU_FS_O_CREATE) == 0)
            return RHU_ENOENT;
        if (!sk_parent_exists(key))
            return RHU_ENOENT;
        for (int i = 0; i < SK_FILES && fi < 0; i++) {
            if (g_skf[i].used == 0)
                fi = i;
        }
        if (fi < 0)
            return RHU_ENOSPC;
        memset(&g_skf[fi], 0, sizeof g_skf[fi]);
        g_skf[fi].used = 1;
        strcpy(g_skf[fi].key, key);
    } else if ((flags & RHU_FS_O_EXCL) != 0) {
        return RHU_EEXIST;
    }
    if ((flags & RHU_FS_O_WRITE) != 0 && g_skf[fi].ro != 0)
        return RHU_EROFS;
    int h = -1;
    for (int i = 0; i < SK_HANDLES && h < 0; i++) {
        if (g_skh[i].used == 0)
            h = i;
    }
    if (h < 0)
        return RHU_EMFILE;
    if ((flags & RHU_FS_O_TRUNC) != 0)
        g_skf[fi].size = 0;
    g_skh[h].used = 1;
    g_skh[h].gone = 0;
    g_skh[h].file = fi;
    g_skh[h].pos = 0;
    g_skh[h].flags = flags;
    g_sk_opens++;
    return h;
}

static struct sk_handle *sk_h(int32_t h)
{
    if (h < 0 || h >= SK_HANDLES || g_skh[h].used == 0)
        return NULL;
    return &g_skh[h];
}

static int32_t sk_close(int32_t h)
{
    struct sk_handle *p = sk_h(h);
    if (p == NULL)
        return RHU_EBADF;
    memset(p, 0, sizeof *p);
    g_sk_closes++;
    return 0;
}

static int32_t sk_transfer(int32_t h, void *buf, uint32_t n, int64_t at, int positional, int writing)
{
    struct sk_handle *p = sk_h(h);
    if (p == NULL)
        return RHU_EBADF;
    if (p->gone)
        return RHU_EIO;
    if (buf == NULL && n > 0)
        return RHU_EINVAL;
    if ((p->flags & (writing ? RHU_FS_O_WRITE : RHU_FS_O_READ)) == 0)
        return RHU_EBADF;
    if (positional && (at < 0 || at > 0xFFFFFFFFll))
        return RHU_EINVAL;
    if (positional && writing && (p->flags & RHU_FS_O_APPEND) != 0)
        return RHU_EINVAL;
    struct sk_file *f = &g_skf[p->file];
    uint64_t pos = positional ? (uint64_t)at : (uint64_t)p->pos;
    if (writing) {
        if (g_sk_full)
            return RHU_ENOSPC;
        if ((p->flags & RHU_FS_O_APPEND) != 0)
            pos = f->size;
        if (pos + n > SK_DATA)
            return RHU_ENOSPC;
        if (pos > f->size)
            memset(f->data + f->size, 0, (size_t)(pos - f->size));
        memcpy(f->data + pos, buf, n);
        if (pos + n > f->size)
            f->size = (uint32_t)(pos + n);
    } else {
        if (pos >= f->size)
            n = 0;
        else if (pos + n > f->size)
            n = (uint32_t)(f->size - pos);
        memcpy(buf, f->data + pos, n);
    }
    if (!positional)
        p->pos = (uint32_t)(pos + n);
    return (int32_t)n;
}

static int32_t sk_read(int32_t h, void *buf, uint32_t n)
{
    return sk_transfer(h, buf, n, 0, 0, 0);
}

static int32_t sk_write(int32_t h, const void *buf, uint32_t n)
{
    return sk_transfer(h, (void *)buf, n, 0, 0, 1);
}

static int32_t sk_pread(int32_t h, void *buf, uint32_t n, int64_t offset)
{
    return sk_transfer(h, buf, n, offset, 1, 0);
}

static int32_t sk_pwrite(int32_t h, const void *buf, uint32_t n, int64_t offset)
{
    return sk_transfer(h, (void *)buf, n, offset, 1, 1);
}

static int32_t sk_seek(int32_t h, int64_t offset, uint32_t whence, int64_t *pos)
{
    struct sk_handle *p = sk_h(h);
    if (p == NULL)
        return RHU_EBADF;
    if (p->gone)
        return RHU_EIO;
    int64_t base = whence == RHU_FS_SEEK_SET ? 0 : whence == RHU_FS_SEEK_CUR ? (int64_t)p->pos : whence == RHU_FS_SEEK_END ? (int64_t)g_skf[p->file].size : -1;
    if (base < 0 || pos == NULL || base + offset < 0 || base + offset > 0xFFFFFFFFll)
        return RHU_EINVAL;
    p->pos = (uint32_t)(base + offset);
    *pos = (int64_t)p->pos;
    return 0;
}

static int32_t sk_truncate(int32_t h, int64_t len)
{
    struct sk_handle *p = sk_h(h);
    if (p == NULL)
        return RHU_EBADF;
    if (p->gone)
        return RHU_EIO;
    if (len < 0 || len > 0xFFFFFFFFll)
        return RHU_EINVAL;
    if ((p->flags & RHU_FS_O_WRITE) == 0)
        return RHU_EBADF;
    if (len > SK_DATA)
        return RHU_ENOSPC;
    struct sk_file *f = &g_skf[p->file];
    if ((uint32_t)len > f->size)
        memset(f->data + f->size, 0, (size_t)len - f->size);
    f->size = (uint32_t)len;
    return 0;
}

static int32_t sk_sync(int32_t h)
{
    struct sk_handle *p = sk_h(h);
    if (p == NULL)
        return RHU_EBADF;
    if (p->gone)
        return RHU_EIO;
    g_sk_syncs++;
    return 0;
}

static void sk_fill_stat(const char *key, int fi, rhu_fs_stat_desc *out)
{
    memset(out, 0, sizeof *out);
    if (fi < 0) {
        out->kind = RHU_FS_DIR; /* the root */
        out->first_cluster = 2;
    } else {
        out->kind = g_skf[fi].dir ? RHU_FS_DIR : RHU_FS_FILE;
        out->size = g_skf[fi].dir ? 0 : g_skf[fi].size;
        out->read_only = g_skf[fi].ro;
        out->first_cluster = g_skf[fi].size != 0 || g_skf[fi].dir ? (uint32_t)fi + 3u : 0u;
    }
    (void)key;
    out->mtime = 1700000000ll + (fi < 0 ? 0 : fi);
    out->ctime = out->mtime - 10;
    out->atime = 1699920000ll;
}

static int32_t sk_fstat(int32_t h, rhu_fs_stat_desc *out)
{
    struct sk_handle *p = sk_h(h);
    if (p == NULL)
        return RHU_EBADF;
    if (p->gone)
        return RHU_EIO;
    sk_fill_stat(g_skf[p->file].key, p->file, out);
    return 0;
}

static int32_t sk_stat(const char *path, rhu_fs_stat_desc *out)
{
    char key[SK_PATH];
    int32_t r = sk_key(path, key);
    if (r != 0)
        return r;
    if (strcmp(key, "/") == 0) {
        sk_fill_stat(key, -1, out);
        return 0;
    }
    int fi = sk_find(key);
    if (fi < 0)
        return RHU_ENOENT;
    sk_fill_stat(key, fi, out);
    return 0;
}

static int32_t sk_unlink(const char *path)
{
    char key[SK_PATH];
    int32_t r = sk_key(path, key);
    if (r != 0)
        return r;
    if (strcmp(key, "/") == 0)
        return RHU_EISDIR; /* the root is a directory, as the kernel's FAT layer answers it */
    int fi = sk_find(key);
    if (fi < 0)
        return RHU_ENOENT;
    if (g_skf[fi].dir)
        return RHU_EISDIR;
    if (g_skf[fi].ro)
        return RHU_EROFS;
    g_skf[fi].used = 0;
    return 0;
}

static int32_t sk_mkdir(const char *path)
{
    char key[SK_PATH];
    int32_t r = sk_key(path, key);
    if (r != 0)
        return r;
    if (strcmp(key, "/") == 0 || sk_find(key) >= 0)
        return RHU_EEXIST;
    if (!sk_parent_exists(key))
        return RHU_ENOENT;
    for (int i = 0; i < SK_FILES; i++) {
        if (g_skf[i].used == 0) {
            memset(&g_skf[i], 0, sizeof g_skf[i]);
            g_skf[i].used = 1;
            g_skf[i].dir = 1;
            strcpy(g_skf[i].key, key);
            return 0;
        }
    }
    return RHU_ENOSPC;
}

static int32_t sk_rmdir(const char *path)
{
    char key[SK_PATH];
    int32_t r = sk_key(path, key);
    if (r != 0)
        return r;
    if (strcmp(key, "/") == 0)
        return RHU_EINVAL;
    int fi = sk_find(key);
    if (fi < 0)
        return RHU_ENOENT;
    if (!g_skf[fi].dir)
        return RHU_ENOTDIR;
    if (sk_has_child(key))
        return RHU_ENOTEMPTY;
    g_skf[fi].used = 0;
    return 0;
}

static int32_t sk_rename(const char *old_path, const char *new_path)
{
    char ok[SK_PATH];
    char nk[SK_PATH];
    int32_t r = sk_key(old_path, ok);
    if (r != 0)
        return r;
    r = sk_key(new_path, nk);
    if (r != 0)
        return r;
    int fi = sk_find(ok);
    if (fi < 0)
        return RHU_ENOENT;
    int ni = sk_find(nk);
    if (ni >= 0) {
        if (g_skf[ni].dir)
            return RHU_EEXIST;
        g_skf[ni].used = 0;
    }
    if (!sk_parent_exists(nk))
        return RHU_ENOENT;
    strcpy(g_skf[fi].key, nk);
    return 0;
}

static int32_t sk_opendir(const char *path)
{
    char key[SK_PATH];
    int32_t r = sk_key(path, key);
    if (r != 0)
        return r;
    if (strcmp(key, "/") != 0) {
        int fi = sk_find(key);
        if (fi < 0)
            return RHU_ENOENT;
        if (!g_skf[fi].dir)
            return RHU_ENOTDIR;
    }
    for (int i = 0; i < SK_DIRS; i++) {
        if (g_skd[i].used == 0) {
            g_skd[i].used = 1;
            g_skd[i].gone = 0;
            strcpy(g_skd[i].key, key);
            g_skd[i].idx = -2;
            g_sk_dopens++;
            return i;
        }
    }
    return RHU_EMFILE;
}

static int32_t sk_readdir(int32_t h, rhu_fs_dirent *out)
{
    if (h < 0 || h >= SK_DIRS || g_skd[h].used == 0)
        return RHU_EBADF;
    if (g_skd[h].gone)
        return RHU_EIO;
    if (out == NULL)
        return RHU_EINVAL;
    struct sk_dir *d = &g_skd[h];
    memset(out, 0, sizeof *out);
    if (d->idx == -2 || d->idx == -1) {
        strcpy(out->name, d->idx == -2 ? "." : "..");
        out->kind = RHU_FS_DIR;
        d->idx++;
        return 1;
    }
    while (d->idx < SK_FILES) {
        int i = d->idx++;
        if (g_skf[i].used != 0 && sk_child_of(g_skf[i].key, d->key)) {
            const char *leaf = g_skf[i].key + strlen(d->key) + (strlen(d->key) == 1 ? 0 : 1);
            strcpy(out->name, leaf);
            out->kind = g_skf[i].dir ? RHU_FS_DIR : RHU_FS_FILE;
            out->size = g_skf[i].dir ? 0 : g_skf[i].size;
            return 1;
        }
    }
    return 0;
}

static int32_t sk_closedir(int32_t h)
{
    if (h < 0 || h >= SK_DIRS || g_skd[h].used == 0)
        return RHU_EBADF;
    memset(&g_skd[h], 0, sizeof g_skd[h]);
    g_sk_dcloses++;
    return 0;
}

static int32_t sk_statfs(const char *path, rhu_fs_statfs_desc *out)
{
    char key[SK_PATH];
    int32_t r = sk_key(path, key);
    if (r != 0)
        return r;
    if (strcmp(key, "/") != 0 && sk_find(key) < 0)
        return RHU_ENOENT;
    out->block_size = SK_CLUSTER;
    out->blocks = SK_CLUSTERS;
    out->free = g_sk_full ? 0 : SK_CLUSTERS - 100;
    out->name_max = 255;
    return 0;
}

/* The unit's table shape (rhu_sysnative.c's sdfs_kernel: twenty-one
 * functions in the header's order), served through the weak hook. */
struct sk_table {
    int32_t (*mount_count)(void);
    int32_t (*mount)(uint32_t, rhu_fs_mount_desc *);
    int32_t (*open)(const char *, uint32_t);
    int32_t (*close)(int32_t);
    int32_t (*read)(int32_t, void *, uint32_t);
    int32_t (*write)(int32_t, const void *, uint32_t);
    int32_t (*pread)(int32_t, void *, uint32_t, int64_t);
    int32_t (*pwrite)(int32_t, const void *, uint32_t, int64_t);
    int32_t (*seek)(int32_t, int64_t, uint32_t, int64_t *);
    int32_t (*truncate)(int32_t, int64_t);
    int32_t (*sync)(int32_t);
    int32_t (*fstat)(int32_t, rhu_fs_stat_desc *);
    int32_t (*stat)(const char *, rhu_fs_stat_desc *);
    int32_t (*unlink)(const char *);
    int32_t (*mkdir)(const char *);
    int32_t (*rmdir)(const char *);
    int32_t (*rename)(const char *, const char *);
    int32_t (*opendir)(const char *);
    int32_t (*readdir)(int32_t, rhu_fs_dirent *);
    int32_t (*closedir)(int32_t);
    int32_t (*statfs)(const char *, rhu_fs_statfs_desc *);
};
static const struct sk_table g_sk_table = {
    sk_mount_count, sk_mount, sk_open, sk_close, sk_read, sk_write, sk_pread, sk_pwrite, sk_seek, sk_truncate, sk_sync,
    sk_fstat, sk_stat, sk_unlink, sk_mkdir, sk_rmdir, sk_rename, sk_opendir, sk_readdir, sk_closedir, sk_statfs,
};
const struct sk_table *rhu_sdfs_kernel_hook(void)
{
    return &g_sk_table;
}

/* The card's events the ladder scripts: a card seated (the volume
 * standing, its files kept as a re-seated card keeps them) and a card
 * pulled (the volume gone, every open handle answering EIO). */
static void sk_card_in(void)
{
    g_sk_mounted = 1;
}

static void sk_card_out(void)
{
    g_sk_mounted = 0;
    for (int i = 0; i < SK_HANDLES; i++)
        g_skh[i].gone = g_skh[i].used;
    for (int i = 0; i < SK_DIRS; i++)
        g_skd[i].gone = g_skd[i].used;
}

/* The mount-point callback's collection. */
static char g_mp[4][32];
static int g_mp_n;
static void mp_found(void *ctx, const char *name)
{
    (void)ctx;
    if (g_mp_n < 4) {
        strncpy(g_mp[g_mp_n], name, 31);
        g_mp[g_mp_n][31] = 0;
    }
    g_mp_n++;
}

static int mp_has(const char *name)
{
    for (int i = 0; i < g_mp_n && i < 4; i++) {
        if (strcmp(g_mp[i], name) == 0)
            return 1;
    }
    return 0;
}

#define SD_ERRNO_IS(e) (SystemNative_GetErrNo() == (e))
#define SD_FD_LO 80
#define SD_FD_HI 96

static int sdfs_ladder(void)
{
    rhu_pal_filestatus st;
    rhu_pal_mountpointinformation mpi;
    rhu_pal_directoryentry de;
    char buf[64];
    char fmt[16];
    int64_t ftype = 0;
    intptr_t fds[SK_HANDLES + 2];

    out("shimtest-sdfs: the card's files from a native caller's side over a scripted kernel");
    CHECK(rhu_shim_init() == 0, "shim-sdfs-init", "(initialization refused)");

    /* no card: the point is absent, /media stands, the store's one mount point */
    CHECK(SystemNative_Stat("/media", &st) == 0 && (st.Mode & RHU_PAL_S_IFMT) == RHU_PAL_S_IFDIR, "shim-sdfs-absent", "(/media is not a directory)");
    CHECK(SystemNative_Stat("/media/sd0", &st) == -1 && SD_ERRNO_IS(RHU_GERR_ENOENT), "shim-sdfs-absent", "(the point answered with no card)");
    CHECK(SystemNative_Open("/media/sd0/a.txt", RHU_PAL_O_RDWR | RHU_PAL_O_CREAT, 0644) == -1 && SD_ERRNO_IS(RHU_GERR_ENOENT), "shim-sdfs-absent", "(an open under the absent point was not ENOENT)");
    g_mp_n = 0;
    CHECK(SystemNative_GetAllMountPoints(mp_found, NULL) == 0 && g_mp_n == 1 && mp_has("/"), "shim-sdfs-absent", "(the mount-point list is not the store's root alone)");
    CHECK(SystemNative_GetSpaceInfoForMountPoint("/media/sd0", &mpi) == -1 && SD_ERRNO_IS(RHU_GERR_ENOENT), "shim-sdfs-absent", "(the space query answered with no card)");
    CHECK(g_sk_opens == 0, "shim-sdfs-absent", "(the scripted kernel opened a handle)");
    pass("shim-sdfs-absent", " (no card: the point ENOENT, /media a directory, the store's root the one mount point)");

    /* the card seated: the point a directory, listed under /media, the volume's space and type */
    sk_card_in();
    CHECK(SystemNative_Stat("/media/sd0", &st) == 0 && (st.Mode & RHU_PAL_S_IFMT) == RHU_PAL_S_IFDIR && st.Dev == 2, "shim-sdfs-mounted", "(the point is not a directory on device 2)");
    g_mp_n = 0;
    CHECK(SystemNative_GetAllMountPoints(mp_found, NULL) == 0 && g_mp_n == 2 && mp_has("/") && mp_has("/media/sd0"), "shim-sdfs-mounted", "(the mount-point list lacks the point)");
    CHECK(SystemNative_GetSpaceInfoForMountPoint("/media/sd0", &mpi) == 0 && mpi.TotalSize == 4096000ull && mpi.TotalFreeSpace == 3686400ull && mpi.AvailableFreeSpace == 3686400ull, "shim-sdfs-mounted", "(the space is not the kernel's cluster counts)");
    CHECK(SystemNative_GetFileSystemTypeNameForMountPoint("/media/sd0", fmt, (int32_t)sizeof fmt, &ftype) == 0 && strcmp(fmt, "vfat") == 0 && ftype == -1, "shim-sdfs-mounted", "(the type name is not vfat with the number -1, the number)");
    {
        void *d = SystemNative_OpenDir("/media");
        int seen = 0;
        CHECK(d != NULL, "shim-sdfs-mounted", "(/media did not open as a directory)");
        while (SystemNative_ReadDir(d, &de) == 0) {
            if (strcmp(de.Name, "sd0") == 0 && de.InodeType == RHU_PAL_DT_DIR)
                seen++;
        }
        SystemNative_CloseDir(d);
        CHECK(seen == 1, "shim-sdfs-mounted", "(a listing of /media does not show sd0 once as a directory)");
    }
    pass("shim-sdfs-mounted", " (the point a directory on its own device, listed under /media; the space from the kernel's clusters; the type vfat)");

    /* a file written and read back: the band, the position, the status record */
    {
        intptr_t fd = SystemNative_Open("/media/sd0/a.txt", RHU_PAL_O_WRONLY | RHU_PAL_O_CREAT | RHU_PAL_O_TRUNC, 0644);
        CHECK(fd == SD_FD_LO, "shim-sdfs-roundtrip", "(the first card descriptor is not the storage band's first)");
        CHECK(SystemNative_Write(fd, "0123456789", 10) == 10, "shim-sdfs-roundtrip", "(the write did not move ten bytes)");
        CHECK(SystemNative_Read(fd, buf, 4) == -1 && SD_ERRNO_IS(RHU_GERR_EBADF), "shim-sdfs-roundtrip", "(a write-only descriptor read)");
        CHECK(SystemNative_FSync(fd) == 0 && g_sk_syncs == 1, "shim-sdfs-roundtrip", "(the sync did not reach the kernel)");
        CHECK(SystemNative_Close(fd) == 0 && g_sk_closes == 1, "shim-sdfs-roundtrip", "(the close did not reach the kernel)");
        fd = SystemNative_Open("/media/sd0/a.txt", RHU_PAL_O_RDONLY, 0);
        CHECK(fd == SD_FD_LO, "shim-sdfs-roundtrip", "(the closed descriptor was not reused)");
        CHECK(SystemNative_FStat(fd, &st) == 0 && st.Size == 10 && (st.Mode & RHU_PAL_S_IFMT) == RHU_PAL_S_IFREG && (st.Mode & 0x1FF) == 0x1B6 && st.Dev == 2 && st.Ino == 3 && st.MTime == 1700000000ll && st.CTime == 1700000000ll && st.BirthTime == 1699999990ll && st.ATime == 1699920000ll && (st.Flags & 1) != 0 && st.HardLinkCount == 1, "shim-sdfs-roundtrip", "(the status record is not the kernel's record mapped)");
        memset(buf, 0, sizeof buf);
        CHECK(SystemNative_Read(fd, buf, 4) == 4 && memcmp(buf, "0123", 4) == 0, "shim-sdfs-roundtrip", "(the first read)");
        CHECK(SystemNative_LSeek(fd, 0, RHU_PAL_SEEK_CUR) == 4 && SystemNative_LSeek(fd, 0, RHU_PAL_SEEK_END) == 10 && SystemNative_LSeek(fd, 2, RHU_PAL_SEEK_SET) == 2, "shim-sdfs-roundtrip", "(the seeks)");
        CHECK(SystemNative_PRead(fd, buf, 3, 7) == 3 && memcmp(buf, "789", 3) == 0 && SystemNative_LSeek(fd, 0, RHU_PAL_SEEK_CUR) == 2, "shim-sdfs-roundtrip", "(a positional read moved the position)");
        CHECK(SystemNative_Read(fd, buf, 64) == 8 && memcmp(buf, "23456789", 8) == 0 && SystemNative_Read(fd, buf, 4) == 0, "shim-sdfs-roundtrip", "(the read to the end)");
        CHECK(SystemNative_Write(fd, "x", 1) == -1 && SD_ERRNO_IS(RHU_GERR_EBADF), "shim-sdfs-roundtrip", "(a read-only descriptor wrote)");
        CHECK(SystemNative_Close(fd) == 0, "shim-sdfs-roundtrip", "(the second close)");
        CHECK(SystemNative_Stat("/media/sd0/a.txt", &st) == 0 && st.Size == 10 && SystemNative_Access("/media/sd0/a.txt", RHU_PAL_R_OK | RHU_PAL_W_OK) == 0 && SystemNative_Access("/media/sd0/a.txt", RHU_PAL_X_OK) == -1 && SD_ERRNO_IS(RHU_GERR_EACCES), "shim-sdfs-roundtrip", "(stat and access by path)");
        pass("shim-sdfs-roundtrip", " (the storage band's first descriptor; ten bytes out and back; seeks and a positional read that leaves the position; the status record mapped; access by path)");
    }

    /* a positional write leaves the position; a truncation grows and cuts */
    {
        intptr_t fd = SystemNative_Open("/media/sd0/a.txt", RHU_PAL_O_RDWR, 0);
        CHECK(fd >= SD_FD_LO && fd < SD_FD_HI, "shim-sdfs-pwrite", "(the open)");
        CHECK(SystemNative_LSeek(fd, 3, RHU_PAL_SEEK_SET) == 3 && SystemNative_PWrite(fd, "AB", 2, 8) == 2 && SystemNative_LSeek(fd, 0, RHU_PAL_SEEK_CUR) == 3, "shim-sdfs-pwrite", "(a positional write moved the position)");
        CHECK(SystemNative_PRead(fd, buf, 10, 0) == 10 && memcmp(buf, "01234567AB", 10) == 0, "shim-sdfs-pwrite", "(the bytes at the offset)");
        CHECK(SystemNative_FTruncate(fd, 14) == 0 && SystemNative_FStat(fd, &st) == 0 && st.Size == 14 && SystemNative_PRead(fd, buf, 4, 10) == 4 && buf[0] == 0 && buf[3] == 0, "shim-sdfs-pwrite", "(the growth is not zero-filled)");
        CHECK(SystemNative_FTruncate(fd, 5) == 0 && SystemNative_FStat(fd, &st) == 0 && st.Size == 5, "shim-sdfs-pwrite", "(the cut)");
        CHECK(SystemNative_FAllocate(fd, 2, 10) == 0 && SystemNative_FStat(fd, &st) == 0 && st.Size == 12, "shim-sdfs-pwrite", "(the preallocation did not grow the file)");
        CHECK(SystemNative_Close(fd) == 0, "shim-sdfs-pwrite", "(the close)");
        pass("shim-sdfs-pwrite", " (a positional write leaves the position; truncation grows zero-filled and cuts; preallocation grows)");
    }

    /* a dup: two descriptors on one kernel handle, the handle closed with the last */
    {
        uint32_t closes = g_sk_closes;
        intptr_t fd = SystemNative_Open("/media/sd0/a.txt", RHU_PAL_O_RDONLY, 0);
        intptr_t dup = SystemNative_Dup(fd);
        CHECK(fd >= SD_FD_LO && dup >= SD_FD_LO && dup < SD_FD_HI && dup != fd, "shim-sdfs-dup", "(the dup is not a second descriptor of the band)");
        CHECK(SystemNative_Read(fd, buf, 2) == 2 && SystemNative_Read(dup, buf, 2) == 2 && memcmp(buf, "23", 2) == 0, "shim-sdfs-dup", "(the position is not shared by the two descriptors)");
        CHECK(SystemNative_Close(fd) == 0 && g_sk_closes == closes, "shim-sdfs-dup", "(the first close closed the kernel's handle)");
        CHECK(SystemNative_Read(dup, buf, 2) == 2 && buf[0] == '4' && buf[1] == 0, "shim-sdfs-dup", "(the surviving descriptor does not read the fifth byte and the first of the preallocated zeros)");
        CHECK(SystemNative_Close(dup) == 0 && g_sk_closes == closes + 1, "shim-sdfs-dup", "(the last close did not close the kernel's handle)");
        CHECK(SystemNative_Read(dup, buf, 2) == -1 && SD_ERRNO_IS(RHU_GERR_EBADF), "shim-sdfs-dup", "(a closed descriptor read)");
        pass("shim-sdfs-dup", " (two descriptors on one kernel handle share the position; the handle closes with the last)");
    }

    /* directories: mkdir, a listing with types, rename within the volume and across, rmdir, unlink */
    {
        void *d;
        int files = 0, dirs = 0, dots = 0;
        CHECK(SystemNative_MkDir("/media/sd0/d", 0755) == 0 && SystemNative_MkDir("/media/sd0/d", 0755) == -1 && SD_ERRNO_IS(RHU_GERR_EEXIST), "shim-sdfs-dirs", "(mkdir, and a second mkdir EEXIST)");
        CHECK(SystemNative_MkDir("/media/sd0/none/x", 0755) == -1 && SD_ERRNO_IS(RHU_GERR_ENOENT), "shim-sdfs-dirs", "(a mkdir under a missing parent)");
        {
            intptr_t fd = SystemNative_Open("/media/sd0/d/b.txt", RHU_PAL_O_WRONLY | RHU_PAL_O_CREAT | RHU_PAL_O_EXCL, 0644);
            CHECK(fd >= SD_FD_LO && SystemNative_Write(fd, "bb", 2) == 2 && SystemNative_Close(fd) == 0, "shim-sdfs-dirs", "(a file in the directory)");
            CHECK(SystemNative_Open("/media/sd0/d/b.txt", RHU_PAL_O_WRONLY | RHU_PAL_O_CREAT | RHU_PAL_O_EXCL, 0644) == -1 && SD_ERRNO_IS(RHU_GERR_EEXIST), "shim-sdfs-dirs", "(an exclusive open of an existing file)");
        }
        d = SystemNative_OpenDir("/media/sd0");
        CHECK(d != NULL, "shim-sdfs-dirs", "(the point did not open as a directory)");
        while (SystemNative_ReadDir(d, &de) == 0) {
            if (strcmp(de.Name, ".") == 0 || strcmp(de.Name, "..") == 0)
                dots++;
            else if (de.InodeType == RHU_PAL_DT_DIR)
                dirs++;
            else if (de.InodeType == RHU_PAL_DT_REG)
                files++;
        }
        CHECK(SystemNative_CloseDir(d) == 0 && g_sk_dopens == 1 && g_sk_dcloses == 1, "shim-sdfs-dirs", "(the directory handle did not close in the kernel)");
        CHECK(dots == 2 && dirs == 1 && files == 1, "shim-sdfs-dirs", "(the listing is not the two dots, one directory and one file)");
        CHECK(SystemNative_OpenDir("/media/sd0/a.txt") == NULL && SD_ERRNO_IS(RHU_GERR_ENOTDIR), "shim-sdfs-dirs", "(a file opened as a directory)");
        CHECK(SystemNative_OpenDir("/media/sd0/none") == NULL && SD_ERRNO_IS(RHU_GERR_ENOENT), "shim-sdfs-dirs", "(a missing directory opened)");
        CHECK(SystemNative_RmDir("/media/sd0/d") == -1 && SD_ERRNO_IS(RHU_GERR_ENOTEMPTY), "shim-sdfs-dirs", "(a non-empty directory removed)");
        CHECK(SystemNative_Rename("/media/sd0/d/b.txt", "/media/sd0/c.txt") == 0 && SystemNative_Stat("/media/sd0/c.txt", &st) == 0 && st.Size == 2 && SystemNative_Stat("/media/sd0/d/b.txt", &st) == -1 && SD_ERRNO_IS(RHU_GERR_ENOENT), "shim-sdfs-dirs", "(the rename within the volume)");
        CHECK(SystemNative_Rename("/media/sd0/c.txt", "/tmp/c.txt") == -1 && SD_ERRNO_IS(RHU_GERR_EXDEV), "shim-sdfs-dirs", "(a rename off the volume was not EXDEV)");
        CHECK(SystemNative_Rename("/tmp/none.txt", "/media/sd0/c.txt") == -1 && SD_ERRNO_IS(RHU_GERR_EXDEV), "shim-sdfs-dirs", "(a rename onto the volume was not EXDEV)");
        CHECK(SystemNative_RmDir("/media/sd0/d") == 0 && SystemNative_Stat("/media/sd0/d", &st) == -1 && SD_ERRNO_IS(RHU_GERR_ENOENT), "shim-sdfs-dirs", "(rmdir of the emptied directory)");
        CHECK(SystemNative_Unlink("/media/sd0/c.txt") == 0 && SystemNative_Unlink("/media/sd0/c.txt") == -1 && SD_ERRNO_IS(RHU_GERR_ENOENT), "shim-sdfs-dirs", "(unlink, and a second unlink ENOENT)");
        CHECK(SystemNative_Unlink("/media/sd0") == -1 && SD_ERRNO_IS(RHU_GERR_EISDIR), "shim-sdfs-dirs", "(an unlink of the point was not EISDIR)");
        pass("shim-sdfs-dirs", " (mkdir and its refusals; a listing with the two dots and the types; rename within the volume, EXDEV across; rmdir refuses non-empty; unlink)");
    }

    /* a copy between the bands through the library's copy verb */
    {
        intptr_t s = SystemNative_Open("/tmp/src.txt", RHU_PAL_O_RDWR | RHU_PAL_O_CREAT | RHU_PAL_O_TRUNC, 0644);
        intptr_t t = SystemNative_Open("/media/sd0/copy.txt", RHU_PAL_O_RDWR | RHU_PAL_O_CREAT | RHU_PAL_O_TRUNC, 0644);
        CHECK(s >= 3 && s < 16 && t >= SD_FD_LO, "shim-sdfs-copy", "(the two ends are not in their bands)");
        CHECK(SystemNative_Write(s, "store-to-card", 13) == 13 && SystemNative_CopyFile(s, t, 13) == 0 && SystemNative_FStat(t, &st) == 0 && st.Size == 13 && SystemNative_PRead(t, buf, 13, 0) == 13 && memcmp(buf, "store-to-card", 13) == 0, "shim-sdfs-copy", "(the store's bytes did not land on the card)");
        CHECK(SystemNative_Close(s) == 0 && SystemNative_Close(t) == 0 && SystemNative_Unlink("/tmp/src.txt") == 0, "shim-sdfs-copy", "(the closes)");
        s = SystemNative_Open("/media/sd0/copy.txt", RHU_PAL_O_RDONLY, 0);
        t = SystemNative_Open("/media/sd0/copy2.txt", RHU_PAL_O_WRONLY | RHU_PAL_O_CREAT, 0644);
        CHECK(s >= SD_FD_LO && t >= SD_FD_LO && SystemNative_CopyFile(s, t, 13) == 0 && SystemNative_Close(s) == 0 && SystemNative_Close(t) == 0 && SystemNative_Stat("/media/sd0/copy2.txt", &st) == 0 && st.Size == 13, "shim-sdfs-copy", "(the copy within the card)");
        CHECK(SystemNative_Unlink("/media/sd0/copy.txt") == 0 && SystemNative_Unlink("/media/sd0/copy2.txt") == 0, "shim-sdfs-copy", "(the cleanup)");
        pass("shim-sdfs-copy", " (the library's copy verb moves bytes from the store to the card and within the card)");
    }

    /* the refusals: the errno map by sign, a full volume, a read-only file, a directory opened as a file, the attributes the layer lacks */
    {
        intptr_t fd = SystemNative_Open("/media/sd0/full.txt", RHU_PAL_O_WRONLY | RHU_PAL_O_CREAT, 0644);
        CHECK(fd >= SD_FD_LO, "shim-sdfs-errors", "(the open)");
        g_sk_full = 1;
        CHECK(SystemNative_Write(fd, "x", 1) == -1 && SD_ERRNO_IS(RHU_GERR_ENOSPC), "shim-sdfs-errors", "(a write on the full volume was not ENOSPC: the errno map)");
        CHECK(SystemNative_GetSpaceInfoForMountPoint("/media/sd0/full.txt", &mpi) == 0 && mpi.TotalFreeSpace == 0, "shim-sdfs-errors", "(the space of the full volume)");
        g_sk_full = 0;
        CHECK(SystemNative_Close(fd) == 0, "shim-sdfs-errors", "(the close)");
        g_skf[sk_find("/full.txt")].ro = 1;
        CHECK(SystemNative_Open("/media/sd0/full.txt", RHU_PAL_O_WRONLY, 0) == -1 && SD_ERRNO_IS(RHU_GERR_EROFS), "shim-sdfs-errors", "(a read-only file opened for writing was not EROFS)");
        CHECK(SystemNative_Access("/media/sd0/full.txt", RHU_PAL_W_OK) == -1 && SD_ERRNO_IS(RHU_GERR_EACCES) && SystemNative_Stat("/media/sd0/full.txt", &st) == 0 && (st.Mode & 0x1FF) == 0x124, "shim-sdfs-errors", "(the read-only attribute as the mode and the access refusal)");
        CHECK(SystemNative_Unlink("/media/sd0/full.txt") == -1 && SD_ERRNO_IS(RHU_GERR_EROFS), "shim-sdfs-errors", "(an unlink of the read-only file was not EROFS)");
        g_skf[sk_find("/full.txt")].ro = 0;
        CHECK(SystemNative_Unlink("/media/sd0/full.txt") == 0, "shim-sdfs-errors", "(the cleanup)");
        CHECK(SystemNative_Open("/media/sd0", RHU_PAL_O_RDONLY, 0) == -1 && SD_ERRNO_IS(RHU_GERR_EISDIR), "shim-sdfs-errors", "(the point opened as a file)");
        CHECK(SystemNative_Open("/media/sd0/none.txt", RHU_PAL_O_RDONLY, 0) == -1 && SD_ERRNO_IS(RHU_GERR_ENOENT), "shim-sdfs-errors", "(a missing file opened)");
        CHECK(SystemNative_ChMod("/media/sd0/a.txt", 0600) == -1 && SD_ERRNO_IS(RHU_GERR_ENOTSUP), "shim-sdfs-errors", "(a mode change was not ENOTSUP)");
        CHECK(SystemNative_UTimensat("/media/sd0/a.txt", NULL) == -1 && SD_ERRNO_IS(RHU_GERR_ENOTSUP), "shim-sdfs-errors", "(a set of the times was not ENOTSUP)");
        fd = SystemNative_Open("/media/sd0/a.txt", RHU_PAL_O_RDWR, 0);
        CHECK(fd >= SD_FD_LO && SystemNative_FChMod(fd, 0600) == -1 && SD_ERRNO_IS(RHU_GERR_ENOTSUP) && SystemNative_FUTimens(fd, NULL) == -1 && SD_ERRNO_IS(RHU_GERR_ENOTSUP), "shim-sdfs-errors", "(the descriptor forms of the two attributes)");
        CHECK(SystemNative_FLock(fd, RHU_PAL_LOCK_EX) == 0 && SystemNative_FLock(fd, RHU_PAL_LOCK_UN) == 0 && SystemNative_LockFileRegion(fd, 0, 4, 1) == 0 && SystemNative_FileSystemSupportsLocking(fd, 0, 0) == 1 && SystemNative_PosixFAdvise(fd, 0, 0, 0) == 0, "shim-sdfs-errors", "(the locks and the advice)");
        CHECK(SystemNative_FcntlGetFD(fd) == 0 && SystemNative_FcntlSetFD(fd, 1) == 0 && SystemNative_FcntlGetFD(fd) == 1, "shim-sdfs-errors", "(the close-on-exec bookkeeping)");
        CHECK(SystemNative_Close(fd) == 0, "shim-sdfs-errors", "(the close)");
        pass("shim-sdfs-errors", " (ENOSPC on a full volume by the sign map; EROFS and the mode of a read-only file; EISDIR and ENOENT; the attributes the layer lacks refused ENOTSUP; the locks granted without effect)");
    }

    /* the band's exhaustion: sixteen descriptors, the seventeenth EMFILE from the band while the kernel has a handle free */
    {
        int n = 0;
        for (int i = 0; i < SK_HANDLES; i++) {
            fds[i] = i == 0 ? SystemNative_Open("/media/sd0/a.txt", RHU_PAL_O_RDONLY, 0) : SystemNative_Dup(fds[0]);
            if (fds[i] >= SD_FD_LO && fds[i] < SD_FD_HI)
                n++;
        }
        CHECK(n == SK_HANDLES, "shim-sdfs-band", "(sixteen descriptors did not open)");
        CHECK(SystemNative_Open("/media/sd0/a.txt", RHU_PAL_O_RDONLY, 0) == -1 && SD_ERRNO_IS(RHU_GERR_EMFILE), "shim-sdfs-band", "(the seventeenth open was not EMFILE)");
        CHECK(SystemNative_Dup(fds[0]) == -1 && SD_ERRNO_IS(RHU_GERR_EMFILE), "shim-sdfs-band", "(a dup past the band was not EMFILE)");
        for (int i = 0; i < SK_HANDLES; i++)
            CHECK(SystemNative_Close(fds[i]) == 0, "shim-sdfs-band", "(a close of the sixteen)");
        fds[0] = SystemNative_Open("/media/sd0/a.txt", RHU_PAL_O_RDONLY, 0);
        CHECK(fds[0] == SD_FD_LO && SystemNative_Close(fds[0]) == 0, "shim-sdfs-band", "(the band did not free: the first slot not reused after the closes)");
        pass("shim-sdfs-band", " (sixteen descriptors on the band, the seventeenth EMFILE, every slot free again after the closes)");
    }

    /* the card pulled under an open handle: EIO at every verb but the close; the point gone; the card back */
    {
        intptr_t fd = SystemNative_Open("/media/sd0/a.txt", RHU_PAL_O_RDWR, 0);
        void *d = SystemNative_OpenDir("/media/sd0");
        CHECK(fd >= SD_FD_LO && d != NULL, "shim-sdfs-hotswap", "(the open and the directory)");
        sk_card_out();
        CHECK(SystemNative_Read(fd, buf, 2) == -1 && SD_ERRNO_IS(RHU_GERR_EIO) && SystemNative_Write(fd, "x", 1) == -1 && SD_ERRNO_IS(RHU_GERR_EIO) && SystemNative_FStat(fd, &st) == -1 && SD_ERRNO_IS(RHU_GERR_EIO) && SystemNative_FSync(fd) == -1 && SD_ERRNO_IS(RHU_GERR_EIO), "shim-sdfs-hotswap", "(a handle whose card left did not answer EIO)");
        CHECK(SystemNative_ReadDir(d, &de) == RHU_GERR_EIO, "shim-sdfs-hotswap", "(a directory stream whose card left did not answer EIO)");
        CHECK(SystemNative_Stat("/media/sd0", &st) == -1 && SD_ERRNO_IS(RHU_GERR_ENOENT), "shim-sdfs-hotswap", "(the point still answered)");
        g_mp_n = 0;
        CHECK(SystemNative_GetAllMountPoints(mp_found, NULL) == 0 && g_mp_n == 1, "shim-sdfs-hotswap", "(the mount-point list still names the point)");
        CHECK(SystemNative_Close(fd) == 0 && SystemNative_CloseDir(d) == 0, "shim-sdfs-hotswap", "(the closes after the card left)");
        sk_card_in();
        CHECK(SystemNative_Stat("/media/sd0/a.txt", &st) == 0 && st.Size == 12, "shim-sdfs-hotswap", "(the card back: its file not found)");
        pass("shim-sdfs-hotswap", " (a card pulled under an open handle: EIO at every verb, the close 0, the point ENOENT; the card back with its file)");
    }

    /* the store beside the card: its own paths and bands untouched by the routing */
    {
        intptr_t fd = SystemNative_Open("/tmp/beside.txt", RHU_PAL_O_RDWR | RHU_PAL_O_CREAT | RHU_PAL_O_TRUNC, 0644);
        uint32_t opens = g_sk_opens;
        CHECK(fd >= 3 && fd < 16 && SystemNative_Write(fd, "store", 5) == 5 && SystemNative_Close(fd) == 0 && SystemNative_Unlink("/tmp/beside.txt") == 0, "shim-sdfs-beside", "(the store's file)");
        CHECK(SystemNative_MkDir("/media/x", 0755) == 0 && SystemNative_RmDir("/media/x") == 0, "shim-sdfs-beside", "(a store directory under /media)");
        CHECK(g_sk_opens == opens, "shim-sdfs-beside", "(a store path reached the kernel)");
        CHECK(SystemNative_Stat("/media/sd01", &st) == -1 && SD_ERRNO_IS(RHU_GERR_ENOENT) && g_sk_opens == opens, "shim-sdfs-beside", "(a name that only begins like the point was routed)");
        pass("shim-sdfs-beside", " (the store's paths and band untouched; a name that only begins like the point stays in the store)");
    }

    SystemNative_Sync();
    out("SHIMTEST-SDFS: ALL GOOD (the card's files from a native caller's side over a scripted kernel)");
    return 0;
}
#endif /* RHU_SHIMTEST_LADDER_SDFS */

#if defined(RHU_SHIMTEST_LADDER_SOCKADDR) || defined(RHU_SHIMTEST_LADDER_SOCKERRORS) || defined(RHU_SHIMTEST_LADDER_SOCKREFUSALS)
#include "rhu_pal_networking_net11.h"
#include "rhu_pal_surface_net11.h"
#include "rhu_sock.h"
#endif
#ifdef RHU_SHIMTEST_LADDER_SOCKADDR

/* --- the socket-address helpers from a native caller's side: the record
 * sizes, then every field of an address buffer written and read back
 * through the helpers, with the bytes the buffer then holds read beside
 * them -- the family as Linux numbers it in the record's first two bytes,
 * the port in network byte order, the IPv4 address as its four bytes, the
 * IPv6 address with its scope and the cleared flow word, a Unix-domain
 * record's path left alone; a buffer too short for a field or a record and
 * a record of another family, each answered by its own error; then the two
 * answers about a message's control buffer. The numbers this ladder reads
 * for are written here, not taken from the unit's header: the sizes and the
 * offsets of the Linux records. --- */
static int sockaddr_ladder(void)
{
    uint8_t rec[136];
    out("shimtest-sockaddr: the socket-address helpers from a native caller's side");
    CHECK(rhu_shim_init() == 0, "shim-sock-addr-init", "(initialization refused)");

    /* the sizes */
    {
        int32_t v4 = 0, v6 = 0, uds = 0, max = 0, off = 0, room = 0, size = 0;
        CHECK(SystemNative_GetSocketAddressSizes(&v4, &v6, &uds, &max) == RHU_PAL_SUCCESS && v4 == 16 && v6 == 28 && uds == 110 && max == 128, "shim-sock-addr-sizes", "(the four record sizes)");
        CHECK(SystemNative_GetMaximumAddressSize() == 128, "shim-sock-addr-sizes", "(the largest record's size is not 128)");
        SystemNative_GetDomainSocketSizes(&off, &room, &size);
        CHECK(off == 2 && room == 108 && size == 110, "shim-sock-addr-sizes", "(the Unix-domain record's path offset, path room and size)");
        CHECK(SystemNative_GetSocketAddressSizes(&v4, &v6, &uds, NULL) == RHU_PAL_EFAULT && SystemNative_GetSocketAddressSizes(NULL, &v6, &uds, &max) == RHU_PAL_EFAULT, "shim-sock-addr-sizes", "(a null result pointer was not EFAULT)");
        pass("shim-sock-addr-sizes", " (the four record sizes 16, 28, 110 and 128; the largest record 128; the Unix-domain path at offset 2, 108 bytes; a null result pointer EFAULT)");
    }

    /* the family: the runtime's number in, the platform's number in the buffer, the runtime's number out */
    {
        static const struct {
            int32_t pal;
            uint8_t low;
        } fam[] = {
            { RHU_PAL_AddressFamily_AF_UNSPEC, 0 }, { RHU_PAL_AddressFamily_AF_UNIX, 1 },    { RHU_PAL_AddressFamily_AF_INET, 2 },
            { RHU_PAL_AddressFamily_AF_INET6, 10 }, { RHU_PAL_AddressFamily_AF_PACKET, 17 }, { RHU_PAL_AddressFamily_AF_CAN, 29 },
        };
        int32_t got = 0;
        for (uint32_t i = 0; i < sizeof fam / sizeof fam[0]; i++) {
            memset(rec, 0xEE, sizeof rec);
            CHECK(SystemNative_SetAddressFamily(rec, 128, fam[i].pal) == RHU_PAL_SUCCESS && rec[0] == fam[i].low && rec[1] == 0 && rec[2] == 0xEE, "shim-sock-addr-family", "(a family was not written as the platform's number in the record's first two bytes)");
            got = -2;
            CHECK(SystemNative_GetAddressFamily(rec, 128, &got) == RHU_PAL_SUCCESS && got == fam[i].pal, "shim-sock-addr-family", "(a family written was not read back as the runtime's number)");
        }
        rec[0] = 0xFF;
        rec[1] = 0;
        got = 0;
        CHECK(SystemNative_GetAddressFamily(rec, 128, &got) == RHU_PAL_SUCCESS && got == RHU_PAL_AddressFamily_AF_UNKNOWN, "shim-sock-addr-family", "(a number the runtime has no name for was not read as unknown)");
        rec[0] = 2;
        CHECK(SystemNative_SetAddressFamily(rec, 128, RHU_PAL_AddressFamily_AF_LINK) == RHU_PAL_EAFNOSUPPORT && SystemNative_SetAddressFamily(rec, 128, RHU_PAL_AddressFamily_AF_UNKNOWN) == RHU_PAL_EAFNOSUPPORT && rec[0] == 2 && rec[1] == 0, "shim-sock-addr-family", "(a family the platform has no number for was not refused with the record left alone)");
        CHECK(SystemNative_GetAddressFamily(rec, 1, &got) == RHU_PAL_EFAULT && SystemNative_SetAddressFamily(rec, 1, RHU_PAL_AddressFamily_AF_INET) == RHU_PAL_EFAULT && SystemNative_GetAddressFamily(rec, -1, &got) == RHU_PAL_EFAULT, "shim-sock-addr-family", "(a buffer shorter than the family field was not EFAULT)");
        CHECK(SystemNative_GetAddressFamily(NULL, 128, &got) == RHU_PAL_EFAULT && SystemNative_GetAddressFamily(rec, 128, NULL) == RHU_PAL_EFAULT && SystemNative_SetAddressFamily(NULL, 128, RHU_PAL_AddressFamily_AF_INET) == RHU_PAL_EFAULT, "shim-sock-addr-family", "(a null buffer or a null result was not EFAULT)");
        pass("shim-sock-addr-family", " (the family written as the platform's number and read back as the runtime's; an unknown number read as unknown; a number the platform lacks refused; a buffer shorter than the field EFAULT)");
    }

    /* the port: two bytes, the high one first, in an IPv4 and an IPv6 record */
    {
        uint16_t port = 0;
        memset(rec, 0, sizeof rec);
        CHECK(SystemNative_SetAddressFamily(rec, 16, RHU_PAL_AddressFamily_AF_INET) == RHU_PAL_SUCCESS && SystemNative_SetPort(rec, 16, 0x1234) == RHU_PAL_SUCCESS, "shim-sock-addr-port", "(the port's set on an IPv4 record)");
        CHECK(rec[2] == 0x12 && rec[3] == 0x34, "shim-sock-addr-port", "(the port's bytes at offset 2 are not in network order)");
        CHECK(SystemNative_GetPort(rec, 16, &port) == RHU_PAL_SUCCESS && port == 0x1234, "shim-sock-addr-port", "(the port was not read back from an IPv4 record)");
        CHECK(SystemNative_SetPort(rec, 15, 1) == RHU_PAL_EFAULT && SystemNative_GetPort(rec, 15, &port) == RHU_PAL_EFAULT && rec[2] == 0x12 && rec[3] == 0x34, "shim-sock-addr-port", "(an IPv4 record one byte short was not EFAULT)");
        memset(rec, 0, sizeof rec);
        port = 0;
        CHECK(SystemNative_SetAddressFamily(rec, 28, RHU_PAL_AddressFamily_AF_INET6) == RHU_PAL_SUCCESS && SystemNative_SetPort(rec, 28, 0xABCD) == RHU_PAL_SUCCESS && rec[2] == 0xAB && rec[3] == 0xCD && SystemNative_GetPort(rec, 28, &port) == RHU_PAL_SUCCESS && port == 0xABCD, "shim-sock-addr-port", "(the port of an IPv6 record)");
        CHECK(SystemNative_SetPort(rec, 27, 1) == RHU_PAL_EFAULT && SystemNative_GetPort(rec, 27, &port) == RHU_PAL_EFAULT && SystemNative_GetPort(rec, 28, NULL) == RHU_PAL_EFAULT && SystemNative_GetPort(NULL, 28, &port) == RHU_PAL_EFAULT && SystemNative_SetPort(NULL, 28, 1) == RHU_PAL_EFAULT, "shim-sock-addr-port", "(an IPv6 record one byte short, a null buffer or a null result was not EFAULT)");
        CHECK(SystemNative_SetAddressFamily(rec, 110, RHU_PAL_AddressFamily_AF_UNIX) == RHU_PAL_SUCCESS && SystemNative_SetPort(rec, 110, 1) == RHU_PAL_EAFNOSUPPORT && SystemNative_GetPort(rec, 110, &port) == RHU_PAL_EAFNOSUPPORT, "shim-sock-addr-port", "(a family that carries no port was not EAFNOSUPPORT)");
        pass("shim-sock-addr-port", " (the port in network byte order at offset 2 of an IPv4 and an IPv6 record, read back; another family EAFNOSUPPORT; a short record EFAULT)");
    }

    /* the IPv4 address: the four bytes as the caller's word holds them */
    {
        static const uint8_t want[4] = { 192, 0, 2, 7 };
        uint32_t given = 0, got = 0;
        uint16_t port = 0;
        memcpy(&given, want, sizeof given);
        memset(rec, 0, sizeof rec);
        CHECK(SystemNative_SetAddressFamily(rec, 16, RHU_PAL_AddressFamily_AF_INET) == RHU_PAL_SUCCESS && SystemNative_SetPort(rec, 16, 8080) == RHU_PAL_SUCCESS && SystemNative_SetIPv4Address(rec, 16, given) == RHU_PAL_SUCCESS, "shim-sock-addr-ipv4", "(the address's set)");
        CHECK(memcmp(rec + 4, want, 4) == 0 && rec[0] == 2 && rec[1] == 0, "shim-sock-addr-ipv4", "(the four address bytes are not at offset 4 as given)");
        CHECK(SystemNative_GetIPv4Address(rec, 16, &got) == RHU_PAL_SUCCESS && got == given && SystemNative_GetPort(rec, 16, &port) == RHU_PAL_SUCCESS && port == 8080, "shim-sock-addr-ipv4", "(the address was not read back, or the port beside it moved)");
        CHECK(SystemNative_SetIPv4Address(rec, 15, given) == RHU_PAL_EFAULT && SystemNative_GetIPv4Address(rec, 15, &got) == RHU_PAL_EFAULT && SystemNative_GetIPv4Address(rec, 16, NULL) == RHU_PAL_EFAULT && SystemNative_GetIPv4Address(NULL, 16, &got) == RHU_PAL_EFAULT && SystemNative_SetIPv4Address(NULL, 16, given) == RHU_PAL_EFAULT, "shim-sock-addr-ipv4", "(a short record, a null buffer or a null result was not EFAULT)");
        CHECK(SystemNative_SetAddressFamily(rec, 28, RHU_PAL_AddressFamily_AF_INET6) == RHU_PAL_SUCCESS && SystemNative_SetIPv4Address(rec, 28, given) == RHU_PAL_EINVAL && SystemNative_GetIPv4Address(rec, 28, &got) == RHU_PAL_EINVAL, "shim-sock-addr-ipv4", "(a record of another family was not EINVAL)");
        pass("shim-sock-addr-ipv4", " (the four address bytes as given at offset 4, read back; the port beside them untouched; another family EINVAL; a short record EFAULT)");
    }

    /* the IPv6 address: sixteen bytes, the scope after them, the flow word cleared by a write */
    {
        static const uint8_t want[16] = { 0x20, 0x01, 0x0d, 0xb8, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 7 };
        uint8_t given[16], got[16];
        uint32_t scope = 0;
        uint16_t port = 0;
        memcpy(given, want, sizeof given);
        memset(got, 0, sizeof got);
        memset(rec, 0xEE, sizeof rec);
        CHECK(SystemNative_SetAddressFamily(rec, 28, RHU_PAL_AddressFamily_AF_INET6) == RHU_PAL_SUCCESS && SystemNative_SetPort(rec, 28, 443) == RHU_PAL_SUCCESS && SystemNative_SetIPv6Address(rec, 28, given, 16, 5) == RHU_PAL_SUCCESS, "shim-sock-addr-ipv6", "(the address's set)");
        CHECK(memcmp(rec + 8, want, 16) == 0 && rec[24] == 5 && rec[25] == 0 && rec[26] == 0 && rec[27] == 0 && rec[28] == 0xEE, "shim-sock-addr-ipv6", "(the sixteen address bytes are not at offset 8, or the scope not at offset 24)");
        CHECK(rec[4] == 0 && rec[5] == 0 && rec[6] == 0 && rec[7] == 0, "shim-sock-addr-ipv6", "(the flow word was not cleared by the write)");
        CHECK(SystemNative_GetIPv6Address(rec, 28, got, 16, &scope) == RHU_PAL_SUCCESS && memcmp(got, want, 16) == 0 && scope == 5 && SystemNative_GetPort(rec, 28, &port) == RHU_PAL_SUCCESS && port == 443, "shim-sock-addr-ipv6", "(the address and its scope were not read back, or the port beside them moved)");
        CHECK(SystemNative_SetIPv6Address(rec, 27, given, 16, 5) == RHU_PAL_EFAULT && SystemNative_GetIPv6Address(rec, 27, got, 16, &scope) == RHU_PAL_EFAULT && SystemNative_SetIPv6Address(rec, 28, given, 15, 5) == RHU_PAL_EFAULT && SystemNative_GetIPv6Address(rec, 28, got, 15, &scope) == RHU_PAL_EFAULT, "shim-sock-addr-ipv6", "(a short record or a short address was not EFAULT)");
        CHECK(SystemNative_GetIPv6Address(rec, 28, got, 16, NULL) == RHU_PAL_EFAULT && SystemNative_GetIPv6Address(rec, 28, NULL, 16, &scope) == RHU_PAL_EFAULT && SystemNative_SetIPv6Address(rec, 28, NULL, 16, 5) == RHU_PAL_EFAULT && SystemNative_SetIPv6Address(NULL, 28, given, 16, 5) == RHU_PAL_EFAULT, "shim-sock-addr-ipv6", "(a null pointer was not EFAULT)");
        CHECK(SystemNative_SetAddressFamily(rec, 28, RHU_PAL_AddressFamily_AF_INET) == RHU_PAL_SUCCESS && SystemNative_SetIPv6Address(rec, 28, given, 16, 5) == RHU_PAL_EINVAL && SystemNative_GetIPv6Address(rec, 28, got, 16, &scope) == RHU_PAL_EINVAL, "shim-sock-addr-ipv6", "(a record of another family was not EINVAL)");
        pass("shim-sock-addr-ipv6", " (the sixteen address bytes at offset 8 and the scope at offset 24, read back; the flow word cleared; another family EINVAL; a short record or address EFAULT)");
    }

    /* a Unix-domain record: its family, its path bytes the caller's own, no port */
    {
        static const char path[12] = "/run/a.sock";
        uint16_t port = 0;
        int32_t got = 0;
        memset(rec, 0, sizeof rec);
        memcpy(rec + 2, path, sizeof path);
        CHECK(SystemNative_SetAddressFamily(rec, 110, RHU_PAL_AddressFamily_AF_UNIX) == RHU_PAL_SUCCESS && rec[0] == 1 && rec[1] == 0 && memcmp(rec + 2, path, sizeof path) == 0, "shim-sock-addr-unix", "(a Unix-domain record's family was not written, or its path bytes moved)");
        CHECK(SystemNative_GetAddressFamily(rec, 110, &got) == RHU_PAL_SUCCESS && got == RHU_PAL_AddressFamily_AF_UNIX, "shim-sock-addr-unix", "(a Unix-domain record's family was not read back)");
        CHECK(SystemNative_GetPort(rec, 110, &port) == RHU_PAL_EAFNOSUPPORT, "shim-sock-addr-unix", "(a Unix-domain record answered a port)");
        pass("shim-sock-addr-unix", " (a Unix-domain record takes its family and keeps the path bytes; it has no port)");
    }

    /* a message's control buffer: its size by the families asked for, and no packet information in a message */
    {
        uint8_t info[16];
        memset(info, 0x5A, sizeof info);
        CHECK(SystemNative_GetControlMessageBufferSize(1, 0) == 24 && SystemNative_GetControlMessageBufferSize(0, 1) == 32 && SystemNative_GetControlMessageBufferSize(1, 1) == 56 && SystemNative_GetControlMessageBufferSize(0, 0) == 0, "shim-sock-addr-control", "(the control buffer's size for the families asked for)");
        CHECK(SystemNative_TryGetIPPacketInformation((MessageHeader *)rec, 1, (IPPacketInformation *)info) == 0 && SystemNative_TryGetIPPacketInformation(NULL, 0, NULL) == 0, "shim-sock-addr-control", "(packet information was found in a message)");
        for (uint32_t i = 0; i < sizeof info; i++)
            CHECK(info[i] == 0x5A, "shim-sock-addr-control", "(the caller's packet-information record was written)");
        pass("shim-sock-addr-control", " (the control buffer's size for a packet-information record of each family, 24 and 32; no packet information found in a message)");
    }

    out("SHIMTEST-SOCKADDR: ALL GOOD (the socket-address helpers from a native caller's side)");
    return 0;
}
#endif /* RHU_SHIMTEST_LADDER_SOCKADDR */

#ifdef RHU_SHIMTEST_LADDER_SOCKERRORS

/* --- the kernel's network codes as the runtime's errors: every code of the
 * kernel's set read through the family's table. The rows below are numbers,
 * the kernel's code beside the runtime's error of the same name, so a pair
 * that drifts in either header is read here as a wrong answer; the code of
 * a row with no network medium is read for its one named answer; and every
 * other number from -200 to 200, with the two ends of the range of a
 * 32-bit number, is read for the input-output error and never for
 * success. --- */
static const struct {
    int32_t code;
    int32_t error;
} g_se_named[] = {
    { -11, 0x10006 }, /* EAGAIN */
    { -110, 0x1004D }, /* ETIMEDOUT */
    { -22, 0x1001C }, /* EINVAL */
    { -3, 0x1004A }, /* ESRCH */
    { -4, 0x1001B }, /* EINTR */
    { -5, 0x1001D }, /* EIO */
    { -16, 0x1000A }, /* EBUSY */
    { -13, 0x10002 }, /* EACCES */
    { -95, 0x1003D }, /* ENOTSUP */
    { -6, 0x1003F }, /* ENXIO */
    { -24, 0x10021 }, /* EMFILE */
    { -32, 0x10043 }, /* EPIPE */
    { -90, 0x10023 }, /* EMSGSIZE */
    { -93, 0x10045 }, /* EPROTONOSUPPORT */
    { -97, 0x10005 }, /* EAFNOSUPPORT */
    { -98, 0x10003 }, /* EADDRINUSE */
    { -100, 0x10026 }, /* ENETDOWN */
    { -101, 0x10028 }, /* ENETUNREACH */
    { -103, 0x1000D }, /* ECONNABORTED */
    { -104, 0x1000F }, /* ECONNRESET */
    { -106, 0x1001E }, /* EISCONN */
    { -107, 0x10038 }, /* ENOTCONN */
    { -111, 0x1000E }, /* ECONNREFUSED */
    { -113, 0x10017 }, /* EHOSTUNREACH */
    { -114, 0x10007 }, /* EALREADY */
    { -115, 0x1001A }, /* EINPROGRESS */
    { -30, 0x10048 }, /* EROFS */
    { -2, 0x1002D }, /* ENOENT */
    { -9, 0x10008 }, /* EBADF */
    { -17, 0x10014 }, /* EEXIST */
    { -20, 0x10039 }, /* ENOTDIR */
    { -21, 0x1001F }, /* EISDIR */
    { -28, 0x10034 }, /* ENOSPC */
    { -36, 0x10025 }, /* ENAMETOOLONG */
    { -39, 0x1003A }, /* ENOTEMPTY */
};
#define SE_NAMED (sizeof g_se_named / sizeof g_se_named[0])
#define SE_NODEV (-19)

static int se_in_table(int32_t code)
{
    for (uint32_t i = 0; i < SE_NAMED; i++) {
        if (g_se_named[i].code == code)
            return 1;
    }
    return code == SE_NODEV;
}

static int sockerrors_ladder(void)
{
    out("shimtest-sockerrors: the kernel's network codes as the runtime's errors");
    CHECK(rhu_shim_init() == 0, "shim-sock-errors-init", "(initialization refused)");

    CHECK(SE_NAMED == 35, "shim-sock-errors-named", "(the ladder's own table is not thirty-five rows)");
    for (uint32_t i = 0; i < SE_NAMED; i++)
        CHECK(rhu_sock_error(g_se_named[i].code) == g_se_named[i].error, "shim-sock-errors-named", "(a kernel code was not answered as the runtime's error of the same name)");
    pass("shim-sock-errors-named", " (thirty-five kernel codes each answered as the runtime's error of the same name)");

    CHECK(RHU_ENODEV == SE_NODEV && rhu_sock_error(SE_NODEV) == RHU_PAL_EAFNOSUPPORT, "shim-sock-errors-nodev", "(the code of a row with no medium was not answered as the address family not supported)");
    pass("shim-sock-errors-nodev", " (the kernel's answer of a row with no medium answered as the address family not supported)");

    for (int32_t code = -200; code <= 200; code++)
        CHECK(rhu_sock_error(code) != RHU_PAL_SUCCESS, "shim-sock-errors-unknown", "(a code outside the table was answered as success)");
    for (int32_t code = -200; code <= 200; code++) {
        if (!se_in_table(code))
            CHECK(rhu_sock_error(code) == RHU_PAL_EIO, "shim-sock-errors-unknown", "(a code outside the table, zero or a positive number was not the input-output error)");
    }
    CHECK(rhu_sock_error(INT32_MIN) == RHU_PAL_EIO && rhu_sock_error(INT32_MAX) == RHU_PAL_EIO, "shim-sock-errors-unknown", "(the smallest or the largest number was not the input-output error)");
    pass("shim-sock-errors-unknown", " (a code outside the table, zero and the positive numbers answered as an input-output error; no code from -200 to 200 answered as success)");

    out("SHIMTEST-SOCKERRORS: ALL GOOD (the kernel's network codes as the runtime's errors)");
    return 0;
}
#endif /* RHU_SHIMTEST_LADDER_SOCKERRORS */

#ifdef RHU_SHIMTEST_LADDER_SOCKREFUSALS

/* --- the refused names of the socket surface, each read for the value its
 * managed caller expects: the runtime's own error from an entry point that
 * returns one, -1 with the C library's errno from a reader that follows
 * that convention, zero from a query whose answer is no. One line per
 * name, in seven families. This image links no socket unit, so its last
 * group reads the socket band as the descriptor layer answers it alone:
 * every descriptor of the band a bad descriptor to the seven verbs the
 * layer hands to the unit, and invalid to Poll at once. The test runner's
 * control links the surface built with the wait over lists answering
 * success, which the first group refuses. --- */
#define SR_ERRNO (*__errno_location())
#define SR_MINUS1(call) (SR_ERRNO = 0, (call) == -1 && SR_ERRNO == RHU_GERR_ENOTSUP)
#define SR_BAD(call) (SR_ERRNO = 0, (call) == -1 && SR_ERRNO == RHU_GERR_EBADF)

static int sockrefusals_ladder(void)
{
    static const intptr_t band[3] = { 32, 40, 47 };
    static const int32_t bounds[3] = { 0, 30, -1 };
    rhu_pal_pollevent pe[2];
    rhu_pal_filestatus st;
    uint8_t address[4] = { 192, 0, 2, 1 };
    uint8_t host[16];
    intptr_t file = -1;
    int32_t flag = 0;
    char name[8] = "eth0";
    uint8_t value[8];
    int32_t n = 0;
    int fds[1] = { 0 };
    int triggered = 0;
    int64_t sent = 0;
    intptr_t made = 0;
    void *entry = NULL;
    memset(value, 0, sizeof value);
    out("shimtest-sockrefusals: the refused names of the socket surface, each read for the value its caller expects");
    CHECK(rhu_shim_init() == 0, "shim-sock-refusals-init", "(initialization refused)");

    CHECK(SystemNative_GetAtOutOfBandMark(0, &n) == RHU_PAL_ENOTSUP, "shim-sock-refusals-socket", "(the out-of-band mark was not ENOTSUP)");
    CHECK(SystemNative_Select(fds, 1, NULL, 0, NULL, 0, 0, 0, &triggered) == RHU_PAL_ENOTSUP, "shim-sock-refusals-socket", "(the wait over lists was not ENOTSUP)");
    CHECK(SystemNative_SendFile(1, 0, 0, 1, &sent) == RHU_PAL_ENOTSUP, "shim-sock-refusals-socket", "(the file send was not ENOTSUP)");
    pass("shim-sock-refusals-socket", " (the out-of-band mark, the wait over lists and the file send: ENOTSUP)");

    CHECK(SystemNative_ReceiveSocketError(0, NULL) == RHU_PAL_ENOTSUP, "shim-sock-refusals-data", "(the error queue was not ENOTSUP)");
    CHECK(SystemNative_PlatformSupportsDualModeIPv4PacketInfo() == 0, "shim-sock-refusals-data", "(dual-mode packet information was answered as present)");
    pass("shim-sock-refusals-data", " (the error queue ENOTSUP; no dual-mode packet information)");

    CHECK(SystemNative_GetRawSockOpt(0, 0, 0, value, &n) == RHU_PAL_ENOTSUP, "shim-sock-refusals-option", "(an option read by the platform's numbers was not ENOTSUP)");
    CHECK(SystemNative_SetRawSockOpt(0, 0, 0, value, 4) == RHU_PAL_ENOTSUP, "shim-sock-refusals-option", "(an option written by the platform's numbers was not ENOTSUP)");
    CHECK(SystemNative_GetIPv4MulticastOption(0, 0, NULL) == RHU_PAL_ENOTSUP, "shim-sock-refusals-option", "(the IPv4 multicast option's read was not ENOTSUP)");
    CHECK(SystemNative_SetIPv4MulticastOption(0, 0, NULL) == RHU_PAL_ENOTSUP, "shim-sock-refusals-option", "(the IPv4 multicast option's write was not ENOTSUP)");
    CHECK(SystemNative_GetIPv6MulticastOption(0, 0, NULL) == RHU_PAL_ENOTSUP, "shim-sock-refusals-option", "(the IPv6 multicast option's read was not ENOTSUP)");
    CHECK(SystemNative_SetIPv6MulticastOption(0, 0, NULL) == RHU_PAL_ENOTSUP, "shim-sock-refusals-option", "(the IPv6 multicast option's write was not ENOTSUP)");
    CHECK(SystemNative_InterfaceNameToIndex(name) == 0, "shim-sock-refusals-option", "(an interface index was answered for a name)");
    pass("shim-sock-refusals-option", " (the options by the platform's numbers and the four multicast options: ENOTSUP; no interface index for a name)");

    CHECK(SR_MINUS1(SystemNative_EnumerateGatewayAddressesForInterface(NULL, 1, NULL)), "shim-sock-refusals-netinfo", "(the gateway reader was not -1 with ENOTSUP)");
    CHECK(SystemNative_CreateNetworkChangeListenerSocket(&made) == RHU_PAL_ENOTSUP, "shim-sock-refusals-netinfo", "(the change listener was not ENOTSUP)");
    CHECK(SystemNative_ReadEvents(0, NULL) == RHU_PAL_ENOTSUP, "shim-sock-refusals-netinfo", "(the change listener's reader was not ENOTSUP)");
    pass("shim-sock-refusals-netinfo", " (the gateway reader -1 with ENOTSUP; the change listener and its reader: ENOTSUP)");

    CHECK(SR_MINUS1(SystemNative_GetActiveTcpConnectionInfos(NULL, &n)), "shim-sock-refusals-statistics", "(the connection list was not -1 with ENOTSUP)");
    CHECK(SR_MINUS1(SystemNative_GetActiveUdpListeners(NULL, &n)), "shim-sock-refusals-statistics", "(the datagram listener list was not -1 with ENOTSUP)");
    CHECK(SR_MINUS1(SystemNative_GetEstimatedTcpConnectionCount()), "shim-sock-refusals-statistics", "(the connection count was not -1 with ENOTSUP)");
    CHECK(SR_MINUS1(SystemNative_GetEstimatedUdpListenerCount()), "shim-sock-refusals-statistics", "(the datagram listener count was not -1 with ENOTSUP)");
    CHECK(SR_MINUS1(SystemNative_GetIPv4GlobalStatistics(NULL)), "shim-sock-refusals-statistics", "(the IPv4 statistics were not -1 with ENOTSUP)");
    CHECK(SR_MINUS1(SystemNative_GetIcmpv4GlobalStatistics(NULL)), "shim-sock-refusals-statistics", "(the ICMPv4 statistics were not -1 with ENOTSUP)");
    CHECK(SR_MINUS1(SystemNative_GetIcmpv6GlobalStatistics(NULL)), "shim-sock-refusals-statistics", "(the ICMPv6 statistics were not -1 with ENOTSUP)");
    CHECK(SR_MINUS1(SystemNative_GetNativeIPInterfaceStatistics(name, NULL)), "shim-sock-refusals-statistics", "(the interface statistics were not -1 with ENOTSUP)");
    CHECK(SR_MINUS1(SystemNative_GetNumRoutes()), "shim-sock-refusals-statistics", "(the route count was not -1 with ENOTSUP)");
    CHECK(SR_MINUS1(SystemNative_GetTcpGlobalStatistics(NULL)), "shim-sock-refusals-statistics", "(the TCP statistics were not -1 with ENOTSUP)");
    CHECK(SR_MINUS1(SystemNative_GetUdpGlobalStatistics(NULL)), "shim-sock-refusals-statistics", "(the UDP statistics were not -1 with ENOTSUP)");
    pass("shim-sock-refusals-statistics", " (the eleven statistics readers: -1 with ENOTSUP)");

    CHECK(SystemNative_GetWasiSocketDescriptor(0, &entry, &n) == RHU_PAL_ENOTSUP, "shim-sock-refusals-wasi", "(the WASI descriptor query was not ENOTSUP)");
    CHECK(SystemNative_WasiSubscribeSocketPollable(0, 0) == RHU_PAL_ENOTSUP, "shim-sock-refusals-wasi", "(the WASI subscription was not ENOTSUP)");
    pass("shim-sock-refusals-wasi", " (the two WASI names: ENOTSUP)");

    memset(host, 0, sizeof host);
    CHECK(SystemNative_GetNameInfo(address, 4, 0, host, 16, NULL, 0, 0) == RHU_PAL_GetAddrInfoErrorFlags_EAI_NONAME, "shim-sock-refusals-resolve", "(the reverse lookup of an address was not EAI_NONAME)");
    pass("shim-sock-refusals-resolve", " (the reverse lookup of an address: EAI_NONAME)");

    /* the socket band with no socket unit in the image */
    for (uint32_t i = 0; i < 3; i++) {
        CHECK(SR_BAD(SystemNative_Close(band[i])) && SR_BAD(SystemNative_Read(band[i], value, 4)) && SR_BAD(SystemNative_Write(band[i], value, 4)) && SR_BAD(SystemNative_FStat(band[i], &st)),
              "shim-sock-refusals-no-unit", "(Close, Read, Write or FStat did not answer a descriptor of the band as a bad descriptor)");
        CHECK(SR_BAD(SystemNative_FcntlGetIsNonBlocking(band[i], &flag)) && SR_BAD(SystemNative_FcntlSetIsNonBlocking(band[i], 1)) && SR_BAD(SystemNative_FcntlGetFD(band[i])),
              "shim-sock-refusals-no-unit", "(a descriptor-flag verb did not answer a descriptor of the band as a bad descriptor)");
    }
    file = SystemNative_Open("/tmp/band.txt", RHU_PAL_O_RDWR | RHU_PAL_O_CREAT | RHU_PAL_O_TRUNC, 0x1A4);
    CHECK(file >= 3 && file <= 15, "shim-sock-refusals-no-unit", "(a file of the store was not opened beside the band)");
    for (uint32_t i = 0; i < 3; i += 2) {
        for (uint32_t b = 0; b < 3; b++) {
            uint32_t trig = 99;
            pe[0].FileDescriptor = (int32_t)band[i];
            pe[0].Events = RHU_PAL_POLLIN | RHU_PAL_POLLOUT;
            pe[0].TriggeredEvents = 0;
            CHECK(SystemNative_Poll(pe, 1, bounds[b], &trig) == RHU_PAL_SUCCESS && trig == 1 && pe[0].TriggeredEvents == RHU_PAL_POLLNVAL,
                  "shim-sock-refusals-no-unit", "(a descriptor of the band polled alone was not answered POLLNVAL at once)");
            trig = 99;
            pe[1].FileDescriptor = (int32_t)file;
            pe[1].Events = RHU_PAL_POLLIN;
            pe[1].TriggeredEvents = 0;
            CHECK(SystemNative_Poll(pe, 2, bounds[b], &trig) == RHU_PAL_SUCCESS && trig == 2 && pe[0].TriggeredEvents == RHU_PAL_POLLNVAL && pe[1].TriggeredEvents == RHU_PAL_POLLIN,
                  "shim-sock-refusals-no-unit", "(a descriptor of the band polled beside a file was not answered POLLNVAL at once)");
        }
    }
    CHECK(SystemNative_Close(file) == 0, "shim-sock-refusals-no-unit", "(the file of the store did not close)");
    pass("shim-sock-refusals-no-unit", " (in this image, which links no socket unit, descriptors 32, 40 and 47 are each a bad descriptor to Close, Read, Write, FStat and the three descriptor-flag verbs, and descriptors 32 and 47 polled alone and beside a file of the store answer POLLNVAL at once, with and without a bound)");

    out("SHIMTEST-SOCKREFUSALS: ALL GOOD (the refused names of the socket surface, each read for the value its caller expects)");
    return 0;
}
#endif /* RHU_SHIMTEST_LADDER_SOCKREFUSALS */

#if defined(RHU_SHIMTEST_LADDER_SOCKSTREAM) || defined(RHU_SHIMTEST_LADDER_SOCKLISTEN) || defined(RHU_SHIMTEST_LADDER_SOCKDATAGRAM) || defined(RHU_SHIMTEST_LADDER_SOCKOPTIONS) \
    || defined(RHU_SHIMTEST_LADDER_SOCKPOLL) || defined(RHU_SHIMTEST_LADDER_SOCKPORT) || defined(RHU_SHIMTEST_LADDER_SOCKRESOLVE) || defined(RHU_SHIMTEST_LADDER_SOCKNETINFO)
#include "rhu_pal_networking_net11.h"
#include "rhu_pal_surface_net11.h"
#include "rhu_sock.h"

/* --- the scripted kernel of the socket ladders. The emulator row's ladders
 * need no network: the tenant serves the kernel functions of the unit's
 * table itself through the socket unit's hook, from a few sockets in memory in
 * the kernel's own shapes (the address record, the readiness bits, a word
 * per socket, the codes at Linux's values). A ladder scripts what the
 * network does next -- bytes or a datagram arriving, a handshake ending
 * well or refused, a connection completing behind a listener, the peer
 * closing, transmit room opening -- and reads in the witness what the unit
 * asked of the kernel. An event scripted to arrive at a refused call lands
 * after that call's answer, as an event on the wire lands between a
 * caller's look and its sleep. --- */
#define NK_SOCKS 20
#define NK_BYTES 64
#define NK_QUEUE 4
#define NK_DATAGRAM_BUFFER 96u

struct nk_dgram {
    uint32_t len;
    rhu_net_addr from;
    uint8_t data[NK_BYTES];
};
struct nk_sock {
    uint8_t used, type, bound, listening, connected, shut, peer_closed; /* connected: 0 no, 1 a handshake in flight, 2 yes */
    rhu_net_addr local, peer;
    int32_t pending;
    uint32_t word;
    uint8_t rx[NK_BYTES];
    uint32_t rx_len;
    struct nk_dgram dq[NK_QUEUE];
    uint32_t dq_len;
    int32_t opt[4];
    uint32_t ready; /* a listener's completed connections */
    int32_t seen, risen; /* the level at the last look; the bits that rose since the last take */
};
static struct nk_sock g_nk[NK_SOCKS];
static uint32_t g_nk_events;          /* the one word for all sockets */
static uint32_t g_nk_takes[NK_SOCKS]; /* the takes of the risen bits, by handle */
static int32_t nk_level(const struct nk_sock *s);

/* The script: what the network does next. */
static struct {
    int32_t connect_end; /* a scripted handshake's end: 0 established, or the code left pending */
    uint32_t connect_at; /* the handshake ends at this readiness call; 0: it does not end */
    uint32_t tx_room;    /* a stream's transmit room in bytes */
    uint32_t room_at;    /* with no room: room opens at this refused send */
    uint32_t late_at;    /* with nothing queued: the bytes below arrive at this refused receive */
    uint8_t late[NK_BYTES];
    uint32_t late_len;
    uint32_t accept_at;  /* with none completed: a connection completes at this refused accept */
} g_nks;

/* The witness: what the unit asked. */
static struct {
    uint32_t sockets, closes, sends, recvs, peeks, options, shut[3];
    uint32_t family, type, protocol, backlog, to_null, accepted;
    uint32_t refused_recvs, refused_sends, refused_accepts, readiness_calls;
    int32_t made, closed;
    rhu_net_addr bound, connected, to;
    uint8_t sent[NK_BYTES * 2];
    uint32_t sent_len;
} g_nkw;

static struct nk_sock *nk_of(int32_t h)
{
    return h >= 0 && h < NK_SOCKS && g_nk[h].used ? &g_nk[h] : NULL;
}

/* The bits that rose since the last look join the risen set. A scripted
 * event looks before it changes a socket and bumps after, so a rise is
 * never lost behind an earlier fall. */
static void nk_sync(struct nk_sock *s)
{
    int32_t now = s->used ? nk_level(s) : 0;
    s->risen |= now & ~s->seen;
    s->seen = now;
}

/* A socket's change: its own word moves, and the one word for all. */
static void nk_bump(struct nk_sock *s)
{
    nk_sync(s);
    s->word++;
    rhu_futex_wake(&s->word, 8);
    g_nk_events++;
    rhu_futex_wake(&g_nk_events, 8);
}

static void nk_address(rhu_net_addr *a, uint8_t last, uint32_t port)
{
    memset(a, 0, sizeof *a);
    a->family = RHU_NET_FAMILY_IPV4;
    a->port = port;
    a->addr[0] = 192;
    a->addr[1] = 0;
    a->addr[2] = 2;
    a->addr[3] = last;
}

/* A free slot from the table's top, so a handle is never its descriptor's
 * place in the band. */
static int32_t nk_slot(uint32_t type)
{
    for (int32_t h = NK_SOCKS - 1; h >= 0; h--) {
        if (!g_nk[h].used) {
            memset(&g_nk[h], 0, sizeof g_nk[h]);
            g_nk[h].used = 1;
            g_nk[h].type = (uint8_t)type;
            g_nk[h].local.family = RHU_NET_FAMILY_IPV4;
            g_nkw.made = h;
            return h;
        }
    }
    return RHU_EMFILE;
}

static int32_t nk_socket(uint32_t family, uint32_t type, uint32_t protocol)
{
    g_nkw.sockets++;
    g_nkw.family = family;
    g_nkw.type = type;
    g_nkw.protocol = protocol;
    if (family != RHU_NET_FAMILY_IPV4)
        return RHU_EAFNOSUPPORT;
    if (!((type == RHU_NET_STREAM && (protocol == 0 || protocol == 6)) || (type == RHU_NET_DATAGRAM && (protocol == 0 || protocol == 17))))
        return RHU_EPROTONOSUPPORT;
#ifdef RHU_SHIMTEST_LADDER_SOCKDATAGRAM
    /* The kernel's pool holds two datagram sockets and refuses a third
     * after it has judged the family and the protocol. The datagram
     * ladder's scripted kernel does the same, so that ladder can ask what
     * a caller is answered while the pool is exhausted. (The stream pool
     * is not modelled anywhere: the stream ladder fills the hosting
     * layer's own band of sixteen descriptors on this table.) */
    if (type == RHU_NET_DATAGRAM) {
        int32_t held = 0;
        for (int32_t i = 0; i < NK_SOCKS; i++)
            held += (g_nk[i].used && g_nk[i].type == RHU_NET_DATAGRAM) ? 1 : 0;
        if (held >= 2)
            return RHU_EMFILE;
    }
#endif
    return nk_slot(type);
}

static int32_t nk_bind(int32_t h, const rhu_net_addr *local)
{
    struct nk_sock *s = nk_of(h);
    if (s == NULL)
        return RHU_EBADF;
    if (local == NULL || local->family != RHU_NET_FAMILY_IPV4)
        return RHU_EAFNOSUPPORT;
    g_nkw.bound = *local;
    if (s->bound || s->connected || local->port == 0)
        return RHU_EINVAL;
    for (int32_t i = 0; i < NK_SOCKS; i++) {
        const struct nk_sock *o = &g_nk[i];
        if (o != s && o->used && o->type == s->type && o->bound && o->local.port == local->port && !(o->opt[RHU_NET_OPT_REUSEADDR] && s->opt[RHU_NET_OPT_REUSEADDR]))
            return RHU_EADDRINUSE;
    }
    s->local = *local;
    s->bound = 1;
    return 0;
}

static int32_t nk_listen(int32_t h, uint32_t backlog)
{
    struct nk_sock *s = nk_of(h);
    if (s == NULL)
        return RHU_EBADF;
    g_nkw.backlog = backlog;
    if (s->type != RHU_NET_STREAM || !s->bound || backlog == 0)
        return RHU_EINVAL;
    s->listening = 1;
    return 0;
}

static int32_t nk_accept(int32_t h, rhu_net_addr *peer)
{
    struct nk_sock *s = nk_of(h);
    if (s == NULL)
        return RHU_EBADF;
    if (!s->listening)
        return RHU_EINVAL;
    if (s->ready == 0) {
        g_nkw.refused_accepts++;
        if (g_nks.accept_at != 0 && g_nkw.refused_accepts == g_nks.accept_at) {
            s->ready++;
            nk_bump(s);
        }
        return RHU_EAGAIN;
    }
    int32_t made = nk_slot(RHU_NET_STREAM);
    if (made < 0)
        return made;
    s->ready--;
    g_nkw.accepted++;
    g_nk[made].connected = 2;
    g_nk[made].bound = 1;
    g_nk[made].local = s->local;
    nk_address(&g_nk[made].peer, 20, 40000u + g_nkw.accepted);
    if (peer != NULL)
        *peer = g_nk[made].peer;
    return made;
}

static void nk_bind_auto(struct nk_sock *s)
{
    if (!s->bound) {
        nk_address(&s->local, 10, 49152);
        s->bound = 1;
    }
}

static int32_t nk_connect(int32_t h, const rhu_net_addr *peer)
{
    struct nk_sock *s = nk_of(h);
    if (s == NULL)
        return RHU_EBADF;
    if (peer == NULL || peer->family != RHU_NET_FAMILY_IPV4)
        return RHU_EAFNOSUPPORT;
    g_nkw.connected = *peer;
    if (s->type == RHU_NET_DATAGRAM) {
        s->peer = *peer;
        s->connected = 2;
        nk_bind_auto(s);
        return 0;
    }
    if (s->connected == 1)
        return RHU_EALREADY;
    if (s->connected == 2)
        return RHU_EISCONN;
    s->connected = 1;
    s->peer = *peer;
    nk_bind_auto(s);
    g_nkw.readiness_calls = 0;
    return RHU_EINPROGRESS;
}

/* A handshake's end: established, or failed with the code pending. */
static void nk_end(struct nk_sock *s, int32_t code)
{
    nk_sync(s);
    if (code == 0) {
        s->connected = 2;
    } else {
        s->connected = 0;
        s->pending = code;
        memset(&s->peer, 0, sizeof s->peer);
    }
    nk_bump(s);
}

static int32_t nk_level(const struct nk_sock *s)
{
    int32_t m = 0;
    if (s->type == RHU_NET_DATAGRAM) {
        if (s->dq_len != 0)
            m |= RHU_NET_READABLE;
        if (s->bound)
            m |= RHU_NET_WRITABLE;
        return m;
    }
    if (s->listening)
        return s->ready != 0 ? RHU_NET_READABLE : 0;
    if (s->rx_len != 0 || s->peer_closed)
        m |= RHU_NET_READABLE;
    if (s->peer_closed)
        m |= RHU_NET_HANGUP;
    if (s->connected == 2 && g_nks.tx_room != 0)
        m |= RHU_NET_WRITABLE;
    if (s->pending != 0)
        m |= RHU_NET_WRITABLE | RHU_NET_ERROR | RHU_NET_HANGUP;
    return m;
}

static int32_t nk_readiness(int32_t h, uint32_t take)
{
    struct nk_sock *s = nk_of(h);
    if (s == NULL)
        return RHU_EBADF;
    g_nkw.readiness_calls++;
    nk_sync(s);
    int32_t level = nk_level(s);
    if (take != 0) {
        level = s->risen;
        s->risen = 0;
        g_nk_takes[h]++;
    }
    if (s->connected == 1 && g_nks.connect_at != 0 && g_nkw.readiness_calls == g_nks.connect_at)
        nk_end(s, g_nks.connect_end);
    return level;
}

static int32_t nk_name(int32_t h, rhu_net_addr *local, rhu_net_addr *peer)
{
    struct nk_sock *s = nk_of(h);
    if (s == NULL)
        return RHU_EBADF;
    if (local != NULL)
        *local = s->local;
    if (peer != NULL) {
        if (s->connected == 2)
            *peer = s->peer;
        else
            memset(peer, 0, sizeof *peer);
    }
    return 0;
}

static int32_t nk_shutdown(int32_t h, uint32_t how)
{
    struct nk_sock *s = nk_of(h);
    if (s == NULL)
        return RHU_EBADF;
    if (how > RHU_NET_SHUT_RDWR)
        return RHU_EINVAL;
    g_nkw.shut[how]++;
    s->shut |= (uint8_t)(how == RHU_NET_SHUT_RD ? 1 : how == RHU_NET_SHUT_WR ? 2 : 3);
    return 0;
}

static int32_t nk_close(int32_t h)
{
    struct nk_sock *s = nk_of(h);
    if (s == NULL)
        return RHU_EBADF;
    g_nkw.closes++;
    g_nkw.closed = h;
    s->used = 0;
    nk_bump(s);
    return 0;
}

static int32_t nk_available(int32_t h)
{
    struct nk_sock *s = nk_of(h);
    if (s == NULL)
        return RHU_EBADF;
    if (s->type == RHU_NET_STREAM)
        return (int32_t)s->rx_len;
    return s->dq_len != 0 ? (int32_t)s->dq[0].len : 0;
}

static int32_t nk_error(int32_t h)
{
    struct nk_sock *s = nk_of(h);
    if (s == NULL)
        return RHU_EBADF;
    int32_t code = s->pending;
    s->pending = 0;
    return code;
}

static const uint32_t *nk_event_word(int32_t h)
{
    struct nk_sock *s = nk_of(h);
    return s != NULL ? &s->word : NULL;
}

static void nk_queue_dgram(struct nk_sock *s, const void *bytes, uint32_t n, uint32_t port)
{
    struct nk_dgram *d = &s->dq[s->dq_len++];
    d->len = n;
    memcpy(d->data, bytes, n);
    nk_address(&d->from, 20, port);
}

static int32_t nk_recv(int32_t h, void *buf, uint32_t n, uint32_t flags, rhu_net_addr *from)
{
    struct nk_sock *s = nk_of(h);
    if (s == NULL)
        return RHU_EBADF;
    g_nkw.recvs++;
    if ((flags & ~RHU_NET_PEEK) != 0)
        return RHU_EINVAL;
    int peek = (flags & RHU_NET_PEEK) != 0;
    if (peek)
        g_nkw.peeks++;
    if (s->pending != 0)
        return nk_error(h);
    if (s->type == RHU_NET_STREAM) {
        if (s->connected != 2)
            return s->connected == 1 ? RHU_EAGAIN : RHU_ENOTCONN;
        if (s->rx_len == 0) {
            if (s->peer_closed || (s->shut & 1) != 0)
                return 0;
            g_nkw.refused_recvs++;
            if (g_nks.late_at != 0 && g_nkw.refused_recvs == g_nks.late_at) {
                memcpy(s->rx, g_nks.late, g_nks.late_len);
                s->rx_len = g_nks.late_len;
                nk_bump(s);
            }
            return RHU_EAGAIN;
        }
        uint32_t take = n < s->rx_len ? n : s->rx_len;
        memcpy(buf, s->rx, take);
        if (!peek) {
            for (uint32_t i = take; i < s->rx_len; i++)
                s->rx[i - take] = s->rx[i];
            s->rx_len -= take;
        }
        return (int32_t)take;
    }
    if (s->dq_len == 0) {
        g_nkw.refused_recvs++;
        if (g_nks.late_at != 0 && g_nkw.refused_recvs == g_nks.late_at) {
            nk_queue_dgram(s, g_nks.late, g_nks.late_len, s->peer.port);
            nk_bump(s);
        }
        return RHU_EAGAIN;
    }
    struct nk_dgram first = s->dq[0];
    if (!peek || n < first.len) {
        for (uint32_t i = 1; i < s->dq_len; i++)
            s->dq[i - 1] = s->dq[i];
        s->dq_len--;
    }
    if (n < first.len)
        return RHU_EMSGSIZE;
    memcpy(buf, first.data, first.len);
    if (from != NULL)
        *from = first.from;
    return (int32_t)first.len;
}

static int32_t nk_send(int32_t h, const void *buf, uint32_t n, const rhu_net_addr *to)
{
    struct nk_sock *s = nk_of(h);
    if (s == NULL)
        return RHU_EBADF;
    g_nkw.sends++;
    if (s->pending != 0)
        return nk_error(h);
    if (s->type == RHU_NET_STREAM) {
        if (s->connected != 2)
            return s->connected == 1 ? RHU_EAGAIN : RHU_ENOTCONN;
        if (s->peer_closed || (s->shut & 2) != 0)
            return RHU_EPIPE;
        if (g_nks.tx_room == 0) {
            g_nkw.refused_sends++;
            if (g_nks.room_at != 0 && g_nkw.refused_sends == g_nks.room_at) {
                g_nks.tx_room = 16;
                nk_bump(s);
            }
            return RHU_EAGAIN;
        }
        uint32_t take = n < g_nks.tx_room ? n : g_nks.tx_room;
        memcpy(g_nkw.sent, buf, take);
        g_nkw.sent_len = take;
        return (int32_t)take;
    }
    g_nkw.to_null = to == NULL;
    if (to != NULL)
        g_nkw.to = *to;
    if (to == NULL && s->connected != 2)
        return RHU_ENOTCONN;
    if (n > NK_DATAGRAM_BUFFER)
        return RHU_EMSGSIZE;
    nk_bind_auto(s);
    memcpy(g_nkw.sent, buf, n);
    g_nkw.sent_len = n;
    return (int32_t)n;
}

static int32_t nk_option(int32_t h, uint32_t option, uint32_t set, int32_t value)
{
    struct nk_sock *s = nk_of(h);
    if (s == NULL)
        return RHU_EBADF;
    g_nkw.options++;
    if (option < RHU_NET_OPT_NODELAY || option > RHU_NET_OPT_REUSEADDR)
        return RHU_ENOTSUP;
    if (s->type == RHU_NET_DATAGRAM && option != RHU_NET_OPT_REUSEADDR)
        return RHU_ENOTSUP;
    if (set == 0)
        return s->opt[option];
    s->opt[option] = value;
    return 0;
}

static int32_t nk_limits(rhu_net_limits_desc *out_limits)
{
    if (out_limits == NULL)
        return RHU_EINVAL;
    out_limits->streams = 8;
    out_limits->datagrams = 8;
    out_limits->stream_rx = 2048;
    out_limits->stream_tx = 1024;
    out_limits->datagram_buf = NK_DATAGRAM_BUFFER;
    out_limits->resolve_slots = 2;
    return 0;
}

/* --- the scripted kernel's second part: the one word for all sockets, the
 * media and the lease, the resolver, and a second thread for the calls
 * that sleep. --- */
#define NK_QUERIES 2
struct nk_query {
    uint8_t used, done;
    int32_t code; /* how the query ended: 0 with the two addresses, or the refusal */
    uint32_t word;
};
static struct nk_query g_nkq[NK_QUERIES];

/* The script of the media and the resolver. */
static struct {
    uint32_t media; /* 0 or 1 */
    rhu_net_medium_desc medium;
    rhu_net_lease_desc lease; /* source 0: no lease yet */
    int32_t filing;           /* a filing's refusal; 0: the query is filed */
    uint32_t busy;            /* while not 0 a filing finds no slot free */
    uint32_t free_at;         /* a slot comes free behind this filing: the one word moves */
    int32_t end;              /* how a filed query ends */
    uint32_t end_at_filing;   /* 1: a query has ended when its filing answers */
} g_nks2;

/* The witness of the one word and the resolver. */
static struct {
    uint32_t bumps; /* the unit's own moves of the one word */
    uint32_t filings, filed, final_reads;
    int32_t handle; /* the last query filed */
    char name[64];
    uint32_t name_len;
} g_nkw2;

static void nk_events_move(void)
{
    g_nk_events++;
    rhu_futex_wake(&g_nk_events, 8);
}

static void nk_events_bump(void)
{
    g_nkw2.bumps++;
    nk_events_move();
}

static const uint32_t *nk_events_word(void)
{
    return &g_nk_events;
}

/* A kernel with no medium answers the count 0 and refuses the rest. */
static int32_t nk_count(void)
{
    return (int32_t)g_nks2.media;
}

static int32_t nk_medium(uint32_t medium, rhu_net_medium_desc *out)
{
    if (g_nks2.media == 0)
        return RHU_ENODEV;
    if (medium >= g_nks2.media || out == NULL)
        return RHU_EINVAL;
    *out = g_nks2.medium;
    return 0;
}

static int32_t nk_lease(uint32_t medium, rhu_net_lease_desc *out)
{
    if (g_nks2.media == 0)
        return RHU_ENODEV;
    if (medium >= g_nks2.media || out == NULL)
        return RHU_EINVAL;
    if (g_nks2.lease.source == RHU_NET_LEASE_NONE)
        return RHU_EAGAIN;
    *out = g_nks2.lease;
    return 0;
}

/* A filed query's end: the answer or the refusal lands and the query's
 * own word moves. */
static void nk_query_ends(int32_t h)
{
    g_nkq[h].done = 1;
    g_nkq[h].code = g_nks2.end;
    g_nkq[h].word++;
    rhu_futex_wake(&g_nkq[h].word, 8);
}

static int32_t nk_resolve(const char *name, uint32_t len)
{
    g_nkw2.filings++;
    g_nkw2.name_len = len;
    memset(g_nkw2.name, 0, sizeof g_nkw2.name);
    memcpy(g_nkw2.name, name, len < sizeof g_nkw2.name ? len : sizeof g_nkw2.name - 1u);
    if (g_nks2.busy != 0) {
        if (g_nks2.free_at != 0 && g_nkw2.filings == g_nks2.free_at) {
            g_nks2.busy = 0;
            nk_events_move();
        }
        return RHU_EAGAIN;
    }
    if (g_nks2.filing != 0)
        return g_nks2.filing;
    for (int32_t h = NK_QUERIES - 1; h >= 0; h--) {
        if (!g_nkq[h].used) {
            memset(&g_nkq[h], 0, sizeof g_nkq[h]);
            g_nkq[h].used = 1;
            g_nkw2.filed++;
            g_nkw2.handle = h;
            if (g_nks2.end_at_filing)
                nk_query_ends(h);
            return h;
        }
    }
    return RHU_EAGAIN;
}

/* The answer: two addresses, 198.51.100.7 and 198.51.100.8. Any final
 * answer releases the slot. */
static int32_t nk_resolve_result(int32_t h, rhu_net_addr *out, uint32_t max)
{
    if (h < 0 || h >= NK_QUERIES || !g_nkq[h].used)
        return RHU_EINVAL;
    if (!g_nkq[h].done)
        return RHU_EAGAIN;
    g_nkq[h].used = 0;
    g_nkw2.final_reads++;
    if (g_nkq[h].code != 0)
        return g_nkq[h].code;
    uint32_t n = max < 2u ? max : 2u;
    for (uint32_t i = 0; i < n; i++) {
        memset(&out[i], 0, sizeof out[i]);
        out[i].family = RHU_NET_FAMILY_IPV4;
        out[i].addr[0] = 198;
        out[i].addr[1] = 51;
        out[i].addr[2] = 100;
        out[i].addr[3] = (uint8_t)(7u + i);
    }
    return (int32_t)n;
}

static const uint32_t *nk_resolve_word(int32_t h)
{
    return h >= 0 && h < NK_QUERIES && g_nkq[h].used ? &g_nkq[h].word : NULL;
}

static __attribute__((unused)) uint32_t nk_slots_held(void)
{
    uint32_t n = 0;
    for (int32_t h = 0; h < NK_QUERIES; h++)
        n += g_nkq[h].used;
    return n;
}

/* An address or a mask as the lease carries it: the four octets in the
 * wire's order read as one word, the first octet in the lowest byte. */
static __attribute__((unused)) uint32_t nk_ip4(uint32_t a, uint32_t b, uint32_t c, uint32_t d)
{
    return a | (b << 8) | (c << 16) | (d << 24);
}

/* One wired medium, its link up, with a lease of 192.0.2.10 in a network
 * of 24 bits. */
static __attribute__((unused)) void nk_medium_up(void)
{
    static const uint8_t hw[6] = { 0x02, 0x00, 0x5E, 0x10, 0x20, 0x30 };
    memset(&g_nks2.medium, 0, sizeof g_nks2.medium);
    memset(&g_nks2.lease, 0, sizeof g_nks2.lease);
    g_nks2.media = 1;
    g_nks2.medium.kind = RHU_NET_MEDIUM_WIRED;
    g_nks2.medium.link = 1;
    memcpy(g_nks2.medium.hw, hw, 6);
    g_nks2.lease.ip4 = nk_ip4(192, 0, 2, 10);
    g_nks2.lease.mask = nk_ip4(255, 255, 255, 0);
    g_nks2.lease.source = RHU_NET_LEASE_DHCP;
}

/* The peer closes a stream. */
static __attribute__((unused)) void nk_peer_closes(int32_t h)
{
    nk_sync(&g_nk[h]);
    g_nk[h].peer_closed = 1;
    nk_bump(&g_nk[h]);
}

/* A second thread, one step less urgent than the ladder's: it runs only
 * while the ladder's thread sleeps, and a wake it makes hands the
 * processor back at once. The ladder opens a stage before the call under
 * proof; this thread marks the stage begun, which is the witness that the
 * call slept, and runs the stage's script. */
#define NK_STAGES 4
static void (*g_nk_script[NK_STAGES])(void);
static volatile uint32_t g_nk_open;  /* the stages the ladder has opened */
static volatile uint32_t g_nk_begun; /* the stages this thread has begun */
static __attribute__((unused)) volatile uint32_t g_nk_back; /* the stages whose call has returned to the ladder */

static void nk_below(void *arg)
{
    (void)arg;
    for (uint32_t i = 0; i < NK_STAGES && g_nk_script[i] != NULL; i++) {
        while (g_nk_open <= i)
            rhu_sleep_ns(1000000ull);
        g_nk_begun = i + 1u;
        g_nk_script[i]();
    }
}

static __attribute__((unused)) int nk_below_start(void)
{
    return rhu_thread_spawn(nk_below, NULL, 16384u, (uint32_t)RHU_HOSTED_PRIO_FLAT + 1u, "below") >= 0;
}

/* The unit's table (rhu_sock.h: twenty-four functions in its order),
 * served through the unit's hook. */
static const rhu_sock_kernel g_nk_table = {
    nk_socket, nk_bind, nk_listen, nk_accept, nk_connect, nk_name, nk_shutdown, nk_close,
    nk_available, nk_error, nk_event_word, nk_readiness, nk_recv, nk_send, nk_option, nk_limits,
    nk_events_bump, nk_events_word, nk_count, nk_lease, nk_resolve, nk_resolve_result, nk_resolve_word, nk_medium,
};
const rhu_sock_kernel *rhu_sock_kernel_hook(void)
{
    return &g_nk_table;
}

/* The network's events a ladder scripts. */
static __attribute__((unused)) void nk_bytes_arrive(int32_t h, const char *bytes, uint32_t n)
{
    nk_sync(&g_nk[h]);
    memcpy(g_nk[h].rx + g_nk[h].rx_len, bytes, n);
    g_nk[h].rx_len += n;
    nk_bump(&g_nk[h]);
}

static __attribute__((unused)) void nk_datagram_arrives(int32_t h, const void *bytes, uint32_t n, uint32_t port)
{
    nk_sync(&g_nk[h]);
    nk_queue_dgram(&g_nk[h], bytes, n, port);
    nk_bump(&g_nk[h]);
}

static __attribute__((unused)) void nk_late(const char *bytes, uint32_t n, uint32_t at)
{
    memcpy(g_nks.late, bytes, n);
    g_nks.late_len = n;
    g_nks.late_at = at;
    g_nkw.refused_recvs = 0;
}

/* A caller's Linux IPv4 record, written and read here byte by byte: the
 * family 2 in the first two bytes, the port in network byte order at
 * offset 2, the address at offset 4, eight zero bytes after it. */
static void sin_record(uint8_t *rec, uint8_t last, uint16_t port)
{
    memset(rec, 0, 16);
    rec[0] = 2;
    rec[2] = (uint8_t)(port >> 8);
    rec[3] = (uint8_t)port;
    rec[4] = 192;
    rec[5] = 0;
    rec[6] = 2;
    rec[7] = last;
}

static __attribute__((unused)) int sin_is(const uint8_t *rec, uint8_t last, uint16_t port)
{
    uint8_t want[16];
    sin_record(want, last, port);
    return memcmp(rec, want, 16) == 0;
}

static __attribute__((unused)) int nk_address_is(const rhu_net_addr *a, uint8_t last, uint32_t port)
{
    return a->family == 2 && a->port == port && a->addr[0] == 192 && a->addr[1] == 0 && a->addr[2] == 2 && a->addr[3] == last;
}

/* A connected stream socket of the band: made, connected by the
 * non-blocking path with the handshake ended by the script, then left
 * blocking or not as asked. */
static __attribute__((unused)) int nk_stream(intptr_t *fd, int32_t *h, int32_t nonblocking)
{
    uint8_t rec[16];
    sin_record(rec, 20, 7);
    if (SystemNative_Socket(RHU_PAL_AddressFamily_AF_INET, RHU_PAL_SocketType_SOCK_STREAM, RHU_PAL_ProtocolType_PT_TCP, fd) != RHU_PAL_SUCCESS)
        return 0;
    *h = g_nkw.made;
    if (SystemNative_FcntlSetIsNonBlocking(*fd, 1) != 0 || SystemNative_Connect(*fd, rec, 16) != RHU_PAL_EINPROGRESS)
        return 0;
    nk_end(&g_nk[*h], 0);
    return SystemNative_FcntlSetIsNonBlocking(*fd, nonblocking) == 0;
}
#endif
#ifdef RHU_SHIMTEST_LADDER_SOCKSTREAM

/* --- a stream socket from a native caller's side: a socket made at the
 * band's first descriptor and read back; the descriptor layer's verbs on
 * it; a connect on a non-blocking and on a blocking socket, to a peer that
 * answers and to one that refuses; the two names in Linux records; bytes
 * sent and received through the socket functions and through Read and
 * Write; the peek; the blocking calls, each waiting on the socket's word
 * for an event that lands during its wait; the two timeouts by the kernel's
 * clock; the three shutdown directions; and a handle's close as the managed
 * class performs it, a zero linger first. The numbers this ladder reads
 * for are written here: the band's descriptors, the record's bytes, the
 * kernel's numbers. --- */
static int sockstream_ladder(void)
{
    static const uint8_t twelve[12] = { 'h', 'e', 'l', 'l', 'o', ',', ' ', 'b', 'o', 'a', 'r', 'd' };
    uint8_t rec[32], buf[32];
    int32_t len = 0, n = 0, err = 0, family = 0, type = 0, protocol = 0, listening = 0, flag = 0, h = -1, h2 = -1;
    intptr_t fd = -1, other = -1;
    uint64_t t0 = 0, waited = 0;
    out("shimtest-sockstream: a stream socket over a scripted kernel, from a native caller's side");
    CHECK(rhu_shim_init() == 0, "shim-sock-stream-init", "(initialization refused)");
    g_nks.tx_room = 16;

    /* create */
    {
        CHECK(SystemNative_Socket(RHU_PAL_AddressFamily_AF_INET, RHU_PAL_SocketType_SOCK_STREAM, RHU_PAL_ProtocolType_PT_TCP, &fd) == RHU_PAL_SUCCESS && fd == 32, "shim-sock-stream-create", "(an IPv4 stream socket did not land at descriptor 32)");
        h = g_nkw.made;
        CHECK(g_nkw.sockets == 1 && g_nkw.family == 2 && g_nkw.type == 1 && g_nkw.protocol == 6, "shim-sock-stream-create", "(the kernel was not asked for an IPv4 stream socket)");
        CHECK(SystemNative_GetSocketType(fd, &family, &type, &protocol, &listening) == RHU_PAL_SUCCESS && family == 2 && type == 1 && protocol == 6 && listening == 0, "shim-sock-stream-create", "(the family, type and protocol were not read back)");
        other = 5;
        CHECK(SystemNative_Socket(RHU_PAL_AddressFamily_AF_INET6, RHU_PAL_SocketType_SOCK_STREAM, RHU_PAL_ProtocolType_PT_TCP, &other) == RHU_PAL_EAFNOSUPPORT && other == -1, "shim-sock-stream-create", "(an IPv6 socket was not answered EAFNOSUPPORT)");
        other = 5;
        CHECK(SystemNative_Socket(RHU_PAL_AddressFamily_AF_INET, RHU_PAL_SocketType_SOCK_RAW, RHU_PAL_ProtocolType_PT_UNSPECIFIED, &other) == RHU_PAL_EPROTONOSUPPORT && other == -1, "shim-sock-stream-create", "(a raw socket was not answered EPROTONOSUPPORT)");
        CHECK(SystemNative_Socket(RHU_PAL_AddressFamily_AF_INET, RHU_PAL_SocketType_SOCK_STREAM, RHU_PAL_ProtocolType_PT_TCP, &other) == RHU_PAL_SUCCESS && other == 33 && SystemNative_Close(other) == 0, "shim-sock-stream-create", "(a refused socket took a descriptor)");
        CHECK(SystemNative_Socket(RHU_PAL_AddressFamily_AF_INET, RHU_PAL_SocketType_SOCK_STREAM, RHU_PAL_ProtocolType_PT_TCP, NULL) == RHU_PAL_EFAULT, "shim-sock-stream-create", "(a null result pointer was not answered EFAULT)");
        pass("shim-sock-stream-create", " (an IPv4 stream socket at descriptor 32, the band's first, its family, type and protocol read back; an IPv6 socket EAFNOSUPPORT and a raw socket EPROTONOSUPPORT, no descriptor taken; a null result pointer EFAULT)");
    }

    /* the descriptor layer's verbs */
    {
        rhu_pal_filestatus st;
        CHECK(SystemNative_FStat(fd, &st) == 0 && (st.Mode & 0xF000) == 0xC000, "shim-sock-stream-descriptor", "(the socket's status is not a socket's mode)");
        CHECK(SystemNative_FcntlGetFD(fd) == 1, "shim-sock-stream-descriptor", "(the close-on-exec bit is not set)");
        flag = 7;
        CHECK(SystemNative_FcntlGetIsNonBlocking(fd, &flag) == 0 && flag == 0, "shim-sock-stream-descriptor", "(a new socket is not blocking)");
        CHECK(SystemNative_FcntlSetIsNonBlocking(fd, 1) == 0 && SystemNative_FcntlGetIsNonBlocking(fd, &flag) == 0 && flag == 1, "shim-sock-stream-descriptor", "(the non-blocking flag was not read back)");
        pass("shim-sock-stream-descriptor", " (the socket's status a socket's mode; the close-on-exec bit set; the non-blocking flag written and read back)");
    }

    /* connect */
    {
        sin_record(rec, 20, 7);
        CHECK(SystemNative_Connect(fd, rec, 16) == RHU_PAL_EINPROGRESS, "shim-sock-stream-connect", "(a non-blocking connect did not answer EINPROGRESS)");
        CHECK(nk_address_is(&g_nkw.connected, 20, 7), "shim-sock-stream-connect", "(the kernel was not given the peer's address and the port in the host's order)");
        nk_end(&g_nk[h], 0);
        err = 99;
        CHECK(SystemNative_GetSocketErrorOption(fd, &err) == RHU_PAL_SUCCESS && err == RHU_PAL_SUCCESS, "shim-sock-stream-connect", "(an established connect left a pending error)");
        CHECK(SystemNative_Socket(RHU_PAL_AddressFamily_AF_INET, RHU_PAL_SocketType_SOCK_STREAM, RHU_PAL_ProtocolType_PT_TCP, &other) == RHU_PAL_SUCCESS, "shim-sock-stream-connect", "(socket)");
        g_nks.connect_end = 0;
        g_nks.connect_at = 1;
        CHECK(SystemNative_Connect(other, rec, 16) == RHU_PAL_SUCCESS, "shim-sock-stream-connect", "(a blocking connect did not answer success at the handshake's end)");
        CHECK(g_nkw.readiness_calls >= 2, "shim-sock-stream-connect", "(a blocking connect did not wait for the handshake's end)");
        g_nks.connect_at = 0;
        CHECK(SystemNative_Close(other) == 0, "shim-sock-stream-connect", "(close)");
        pass("shim-sock-stream-connect", " (a non-blocking connect EINPROGRESS and no pending error at the handshake's end; a blocking connect waits for the handshake's end and answers success; the kernel given the peer's address and the port in the host's order)");
    }

    /* the pending error */
    {
        CHECK(SystemNative_Socket(RHU_PAL_AddressFamily_AF_INET, RHU_PAL_SocketType_SOCK_STREAM, RHU_PAL_ProtocolType_PT_TCP, &other) == RHU_PAL_SUCCESS, "shim-sock-stream-pending-error", "(socket)");
        h2 = g_nkw.made;
        CHECK(SystemNative_FcntlSetIsNonBlocking(other, 1) == 0 && SystemNative_Connect(other, rec, 16) == RHU_PAL_EINPROGRESS, "shim-sock-stream-pending-error", "(connect)");
        nk_end(&g_nk[h2], RHU_ECONNREFUSED);
        err = 0;
        CHECK(SystemNative_GetSocketErrorOption(other, &err) == RHU_PAL_SUCCESS && err == RHU_PAL_ECONNREFUSED, "shim-sock-stream-pending-error", "(a refused connect's pending code was not answered)");
        CHECK(SystemNative_GetSocketErrorOption(other, &err) == RHU_PAL_SUCCESS && err == RHU_PAL_SUCCESS, "shim-sock-stream-pending-error", "(the pending code was answered twice)");
        CHECK(SystemNative_Close(other) == 0, "shim-sock-stream-pending-error", "(close)");
        CHECK(SystemNative_Socket(RHU_PAL_AddressFamily_AF_INET, RHU_PAL_SocketType_SOCK_STREAM, RHU_PAL_ProtocolType_PT_TCP, &other) == RHU_PAL_SUCCESS, "shim-sock-stream-pending-error", "(socket)");
        g_nks.connect_end = RHU_ECONNREFUSED;
        g_nks.connect_at = 1;
        CHECK(SystemNative_Connect(other, rec, 16) == RHU_PAL_ECONNREFUSED, "shim-sock-stream-pending-error", "(a blocking connect to a peer that refuses did not answer ECONNREFUSED)");
        g_nks.connect_end = 0;
        g_nks.connect_at = 0;
        CHECK(SystemNative_Close(other) == 0, "shim-sock-stream-pending-error", "(close)");
        pass("shim-sock-stream-pending-error", " (a refused connect's code read once as the runtime's connection-refused error, then none; a blocking connect to a peer that refuses answers that error)");
    }

    /* the names */
    {
        memset(rec, 0xEE, sizeof rec);
        len = 32;
        CHECK(SystemNative_GetSockName(fd, rec, &len) == RHU_PAL_SUCCESS && len == 16 && sin_is(rec, 10, 49152), "shim-sock-stream-names", "(the local name is not the Linux IPv4 record of the socket's own address)");
        memset(rec, 0xEE, sizeof rec);
        len = 32;
        CHECK(SystemNative_GetPeerName(fd, rec, &len) == RHU_PAL_SUCCESS && len == 16 && sin_is(rec, 20, 7), "shim-sock-stream-names", "(the peer name is not the Linux IPv4 record of the peer's address)");
        CHECK(SystemNative_Socket(RHU_PAL_AddressFamily_AF_INET, RHU_PAL_SocketType_SOCK_STREAM, RHU_PAL_ProtocolType_PT_TCP, &other) == RHU_PAL_SUCCESS, "shim-sock-stream-names", "(socket)");
        len = 32;
        CHECK(SystemNative_GetPeerName(other, rec, &len) == RHU_PAL_ENOTCONN, "shim-sock-stream-names", "(an unconnected socket's peer name was not answered ENOTCONN)");
        len = 8;
        CHECK(SystemNative_GetSockName(fd, rec, &len) == RHU_PAL_EFAULT, "shim-sock-stream-names", "(a buffer shorter than the record was not answered EFAULT)");
        CHECK(SystemNative_Close(other) == 0, "shim-sock-stream-names", "(close)");
        pass("shim-sock-stream-names", " (the local and the peer address each in a Linux IPv4 record of 16 bytes, the port in network byte order; an unconnected socket's peer ENOTCONN; a buffer shorter than the record EFAULT)");
    }

    /* the data calls; the socket is non-blocking */
    {
        g_nks.tx_room = 8;
        n = -1;
        CHECK(SystemNative_Send(fd, (void *)twelve, 12, 0, &n) == RHU_PAL_SUCCESS && n == 8 && g_nkw.sent_len == 8 && memcmp(g_nkw.sent, twelve, 8) == 0, "shim-sock-stream-data", "(twelve bytes sent into a room of eight were not reported as eight)");
        g_nks.tx_room = 16;
        nk_bytes_arrive(h, "abcdefghij", 10);
        CHECK(SystemNative_GetBytesAvailable(fd, &n) == RHU_PAL_SUCCESS && n == 10, "shim-sock-stream-data", "(the queued bytes were not counted as available)");
        CHECK(SystemNative_Receive(fd, buf, 6, 0, &n) == RHU_PAL_SUCCESS && n == 6 && memcmp(buf, "abcdef", 6) == 0, "shim-sock-stream-data", "(the first six bytes were not received)");
        CHECK(SystemNative_Receive(fd, buf, 32, 0, &n) == RHU_PAL_SUCCESS && n == 4 && memcmp(buf, "ghij", 4) == 0, "shim-sock-stream-data", "(the rest of the queue was not received)");
        n = 5;
        CHECK(SystemNative_Receive(fd, buf, 32, 0, &n) == RHU_PAL_EAGAIN && n == 0, "shim-sock-stream-data", "(an empty queue on a non-blocking socket was not answered EAGAIN)");
        CHECK(SystemNative_Write(fd, twelve, 5) == 5 && g_nkw.sent_len == 5 && memcmp(g_nkw.sent, twelve, 5) == 0, "shim-sock-stream-data", "(Write on the descriptor did not send its bytes)");
        nk_bytes_arrive(h, "xyz", 3);
        CHECK(SystemNative_Read(fd, buf, 32) == 3 && memcmp(buf, "xyz", 3) == 0, "shim-sock-stream-data", "(Read on the descriptor did not receive the queued bytes)");
        CHECK(SystemNative_Read(fd, buf, 32) == -1 && SystemNative_GetErrNo() == 11, "shim-sock-stream-data", "(Read on an empty queue did not set the platform's EAGAIN)");
        CHECK(nk_stream(&other, &h2, 1), "shim-sock-stream-data", "(a second connected socket)");
        g_nk[h2].peer_closed = 1;
        nk_bump(&g_nk[h2]);
        n = 5;
        CHECK(SystemNative_Receive(other, buf, 32, 0, &n) == RHU_PAL_SUCCESS && n == 0, "shim-sock-stream-data", "(the peer's close was not read as 0 bytes)");
        CHECK(SystemNative_Close(other) == 0, "shim-sock-stream-data", "(close)");
        pass("shim-sock-stream-data", " (twelve bytes sent and the kernel's partial take of eight reported; the bytes queued received byte for byte and counted as available; 0 at the peer's close; an empty queue EAGAIN on a non-blocking socket; Read and Write on the descriptor move the same bytes and set the platform's errno)");
    }

    /* the peek */
    {
        nk_bytes_arrive(h, "peekab", 6);
        CHECK(SystemNative_Receive(fd, buf, 4, RHU_PAL_SocketFlags_MSG_PEEK, &n) == RHU_PAL_SUCCESS && n == 4 && memcmp(buf, "peek", 4) == 0, "shim-sock-stream-peek", "(a peek did not answer the queued bytes)");
        CHECK(SystemNative_Receive(fd, buf, 32, 0, &n) == RHU_PAL_SUCCESS && n == 6 && memcmp(buf, "peekab", 6) == 0, "shim-sock-stream-peek", "(a peek consumed the bytes it read)");
        CHECK(SystemNative_Receive(fd, buf, 4, RHU_PAL_SocketFlags_MSG_OOB, &n) == RHU_PAL_ENOTSUP && SystemNative_Send(fd, (void *)twelve, 4, RHU_PAL_SocketFlags_MSG_DONTROUTE, &n) == RHU_PAL_ENOTSUP, "shim-sock-stream-peek", "(a flag the stack does not serve was not answered ENOTSUP)");
        pass("shim-sock-stream-peek", " (a peek answers the bytes and leaves them queued; a flag the stack does not serve ENOTSUP)");
    }

    /* the blocking calls */
    {
        CHECK(SystemNative_FcntlSetIsNonBlocking(fd, 0) == 0, "shim-sock-stream-blocking", "(the socket was not set blocking)");
        nk_late("late!", 5, 2);
        CHECK(SystemNative_Receive(fd, buf, 32, 0, &n) == RHU_PAL_SUCCESS && n == 5 && memcmp(buf, "late!", 5) == 0, "shim-sock-stream-blocking", "(a blocking receive did not answer the bytes that arrived during its wait)");
        g_nks.late_at = 0;
        g_nks.tx_room = 0;
        g_nkw.refused_sends = 0;
        g_nks.room_at = 2;
        CHECK(SystemNative_Send(fd, (void *)twelve, 12, 0, &n) == RHU_PAL_SUCCESS && n == 12, "shim-sock-stream-blocking", "(a blocking send with no room did not wait for room)");
        g_nks.room_at = 0;
        CHECK(SystemNative_Receive(fd, buf, 32, RHU_PAL_SocketFlags_MSG_DONTWAIT, &n) == RHU_PAL_EAGAIN, "shim-sock-stream-blocking", "(the don't-wait flag did not answer EAGAIN at once on a blocking socket)");
        pass("shim-sock-stream-blocking", " (a blocking receive whose bytes arrive during its wait answers them; a blocking send with no room waits for room; the don't-wait flag answers EAGAIN at once on a blocking socket)");
    }

    /* the timeouts, by the kernel's clock */
    {
        CHECK(SystemNative_SetReceiveTimeout(fd, 30) == RHU_PAL_SUCCESS, "shim-sock-stream-timeout", "(the receive timeout was not taken)");
        t0 = rhu_now_ns();
        err = SystemNative_Receive(fd, buf, 32, 0, &n);
        waited = rhu_now_ns() - t0;
        CHECK(err == RHU_PAL_EAGAIN, "shim-sock-stream-timeout", "(a blocking receive at its receive timeout did not answer EAGAIN)");
        CHECK(waited >= 30000000ull, "shim-sock-stream-timeout", "(a blocking receive answered before its bound)");
        g_nks.tx_room = 0;
        CHECK(SystemNative_SetSendTimeout(fd, 20) == RHU_PAL_SUCCESS, "shim-sock-stream-timeout", "(the send timeout was not taken)");
        t0 = rhu_now_ns();
        err = SystemNative_Send(fd, (void *)twelve, 12, 0, &n);
        waited = rhu_now_ns() - t0;
        CHECK(err == RHU_PAL_EAGAIN && waited >= 20000000ull, "shim-sock-stream-timeout", "(a blocking send with no room did not answer EAGAIN at its send timeout)");
        g_nks.tx_room = 16;
        CHECK(SystemNative_Socket(RHU_PAL_AddressFamily_AF_INET, RHU_PAL_SocketType_SOCK_STREAM, RHU_PAL_ProtocolType_PT_TCP, &other) == RHU_PAL_SUCCESS && SystemNative_SetSendTimeout(other, 20) == RHU_PAL_SUCCESS, "shim-sock-stream-timeout", "(socket)");
        sin_record(rec, 20, 7);
        t0 = rhu_now_ns();
        err = SystemNative_Connect(other, rec, 16);
        waited = rhu_now_ns() - t0;
        CHECK(err == RHU_PAL_EINPROGRESS && waited >= 20000000ull, "shim-sock-stream-timeout", "(a blocking connect with no answer did not answer EINPROGRESS at the send timeout)");
        CHECK(SystemNative_Close(other) == 0, "shim-sock-stream-timeout", "(close)");
        CHECK(SystemNative_SetReceiveTimeout(fd, -1) == RHU_PAL_EINVAL && SystemNative_SetSendTimeout(fd, -1) == RHU_PAL_EINVAL, "shim-sock-stream-timeout", "(a negative timeout was not answered EINVAL)");
        CHECK(SystemNative_SetReceiveTimeout(fd, 0) == RHU_PAL_SUCCESS && SystemNative_SetSendTimeout(fd, 0) == RHU_PAL_SUCCESS, "shim-sock-stream-timeout", "(the bounds were not cleared)");
        pass("shim-sock-stream-timeout", " (a blocking receive answers EAGAIN at its receive timeout and not before; a blocking send at its send timeout; a blocking connect with no answer EINPROGRESS at the send timeout; a negative timeout EINVAL)");
    }

    /* shutdown */
    {
        uint32_t rd = g_nkw.shut[0], wr = g_nkw.shut[1], both = g_nkw.shut[2];
        CHECK(SystemNative_Shutdown(fd, RHU_PAL_SocketShutdown_SHUT_READ) == RHU_PAL_SUCCESS && g_nkw.shut[0] == rd + 1 && g_nkw.shut[1] == wr && g_nkw.shut[2] == both, "shim-sock-stream-shutdown", "(the read direction did not reach the kernel as its own)");
        CHECK(SystemNative_Shutdown(fd, RHU_PAL_SocketShutdown_SHUT_WRITE) == RHU_PAL_SUCCESS && g_nkw.shut[0] == rd + 1 && g_nkw.shut[1] == wr + 1 && g_nkw.shut[2] == both, "shim-sock-stream-shutdown", "(the write direction did not reach the kernel as its own)");
        CHECK(SystemNative_Shutdown(fd, RHU_PAL_SocketShutdown_SHUT_BOTH) == RHU_PAL_SUCCESS && g_nkw.shut[0] == rd + 1 && g_nkw.shut[1] == wr + 1 && g_nkw.shut[2] == both + 1, "shim-sock-stream-shutdown", "(both directions did not reach the kernel as one call)");
        CHECK(SystemNative_Shutdown(fd, 3) == RHU_PAL_EINVAL && SystemNative_Shutdown(fd, -1) == RHU_PAL_EINVAL && g_nkw.shut[2] == both + 1, "shim-sock-stream-shutdown", "(another value was not answered EINVAL)");
        CHECK(SystemNative_Disconnect(fd) == RHU_PAL_SUCCESS && g_nkw.shut[2] == both + 2, "shim-sock-stream-shutdown", "(a disconnect did not shut both directions)");
        pass("shim-sock-stream-shutdown", " (each of the three directions reaches the kernel as its own; another value EINVAL; a disconnect shuts both)");
    }

    /* a handle's close, as the managed class performs it */
    {
        rhu_pal_lingeroption zero_linger = { 1, 0 }, back = { 7, 7 };
        intptr_t all[16];
        uint32_t closes = g_nkw.closes;
        CHECK(SystemNative_SetLingerOption(fd, (LingerOption *)&zero_linger) == RHU_PAL_SUCCESS, "shim-sock-stream-close", "(the zero linger of a handle's close was not answered as success)");
        CHECK(SystemNative_GetLingerOption(fd, (LingerOption *)&back) == RHU_PAL_SUCCESS && back.OnOff == 1 && back.Seconds == 0, "shim-sock-stream-close", "(the zero linger was not read back)");
        CHECK(SystemNative_Close(fd) == 0 && g_nkw.closes == closes + 1 && g_nkw.closed == h, "shim-sock-stream-close", "(the kernel's close was not called once for the socket's handle)");
        CHECK(SystemNative_Close(fd) == -1 && SystemNative_GetErrNo() == 9 && g_nkw.closes == closes + 1, "shim-sock-stream-close", "(a closed descriptor was closed a second time)");
        CHECK(SystemNative_Receive(fd, buf, 4, 0, &n) == RHU_PAL_EBADF, "shim-sock-stream-close", "(a closed descriptor was not answered EBADF)");
        for (int32_t i = 0; i < 16; i++)
            CHECK(SystemNative_Socket(RHU_PAL_AddressFamily_AF_INET, RHU_PAL_SocketType_SOCK_STREAM, RHU_PAL_ProtocolType_PT_TCP, &all[i]) == RHU_PAL_SUCCESS && all[i] == 32 + i, "shim-sock-stream-close", "(the band's descriptors were not handed out from its first, the closed one free again)");
        other = 5;
        CHECK(SystemNative_Socket(RHU_PAL_AddressFamily_AF_INET, RHU_PAL_SocketType_SOCK_STREAM, RHU_PAL_ProtocolType_PT_TCP, &other) == RHU_PAL_EMFILE && other == -1, "shim-sock-stream-close", "(a seventeenth socket was not answered EMFILE)");
        for (int32_t i = 0; i < 16; i++)
            CHECK(SystemNative_Close(all[i]) == 0, "shim-sock-stream-close", "(close)");
        pass("shim-sock-stream-close", " (a zero linger written as a handle's close writes it and read back; the kernel's close called once and the descriptor free for the next socket; a closed descriptor EBADF; the band's sixteen descriptors and the seventeenth socket EMFILE)");
    }

    out("SHIMTEST-SOCKSTREAM: ALL GOOD (a stream socket over a scripted kernel, from a native caller's side)");
    return 0;
}
#endif /* RHU_SHIMTEST_LADDER_SOCKSTREAM */

#ifdef RHU_SHIMTEST_LADDER_SOCKLISTEN

/* --- a listening socket from a native caller's side: a bind that hands the
 * kernel the caller's Linux record, the port in use, the backlog, an accept
 * with no connection completed on a non-blocking and on a blocking
 * listener, an accept that writes the peer's address into the caller's
 * record and makes a blocking stream socket at a new descriptor, and a
 * blocking accept whose connection completes during its wait. --- */
static int socklisten_ladder(void)
{
    uint8_t rec[32], rec2[32];
    int32_t len = 0, err = 0, family = 0, type = 0, protocol = 0, listening = 0, flag = 0, lh = -1;
    intptr_t lfd = -1, fd2 = -1, a1 = -1, a2 = -1;
    uint64_t t0 = 0, waited = 0;
    out("shimtest-socklisten: a listening socket over a scripted kernel, from a native caller's side");
    CHECK(rhu_shim_init() == 0, "shim-sock-listen-init", "(initialization refused)");

    /* bind */
    {
        CHECK(SystemNative_Socket(RHU_PAL_AddressFamily_AF_INET, RHU_PAL_SocketType_SOCK_STREAM, RHU_PAL_ProtocolType_PT_TCP, &lfd) == RHU_PAL_SUCCESS, "shim-sock-listen-bind", "(socket)");
        lh = g_nkw.made;
        sin_record(rec, 10, 8080);
        CHECK(SystemNative_Bind(lfd, RHU_PAL_ProtocolType_PT_TCP, rec, 16) == RHU_PAL_SUCCESS && nk_address_is(&g_nkw.bound, 10, 8080), "shim-sock-listen-bind", "(the kernel was not given the address and the port of the caller's record)");
        memset(rec, 0xEE, sizeof rec);
        len = 32;
        CHECK(SystemNative_GetSockName(lfd, rec, &len) == RHU_PAL_SUCCESS && len == 16 && sin_is(rec, 10, 8080), "shim-sock-listen-bind", "(the bound port was not read back through the local name)");
        CHECK(SystemNative_Socket(RHU_PAL_AddressFamily_AF_INET, RHU_PAL_SocketType_SOCK_STREAM, RHU_PAL_ProtocolType_PT_TCP, &fd2) == RHU_PAL_SUCCESS, "shim-sock-listen-bind", "(socket)");
        memset(rec, 0, sizeof rec);
        rec[0] = 1;
        CHECK(SystemNative_Bind(fd2, RHU_PAL_ProtocolType_PT_TCP, rec, 16) == RHU_PAL_EAFNOSUPPORT, "shim-sock-listen-bind", "(a Unix-domain record was not answered EAFNOSUPPORT)");
        rec[0] = 10;
        CHECK(SystemNative_Bind(fd2, RHU_PAL_ProtocolType_PT_TCP, rec, 28) == RHU_PAL_EAFNOSUPPORT, "shim-sock-listen-bind", "(an IPv6 record was not answered EAFNOSUPPORT)");
        sin_record(rec, 10, 8081);
        CHECK(SystemNative_Bind(fd2, RHU_PAL_ProtocolType_PT_TCP, rec, 8) == RHU_PAL_EFAULT, "shim-sock-listen-bind", "(a short record was not answered EFAULT)");
        pass("shim-sock-listen-bind", " (the kernel given the address and the port of the caller's Linux record, the port in the host's order; the bound port read back through the local name; a record of another family EAFNOSUPPORT; a short record EFAULT)");
    }

    /* the port in use */
    {
        sin_record(rec, 10, 8080);
        CHECK(SystemNative_Bind(fd2, RHU_PAL_ProtocolType_PT_TCP, rec, 16) == RHU_PAL_EADDRINUSE, "shim-sock-listen-in-use", "(a second socket's bind to the port in use was not answered EADDRINUSE)");
        pass("shim-sock-listen-in-use", " (a second socket's bind to the port in use EADDRINUSE)");
    }

    /* listen */
    {
        CHECK(SystemNative_Listen(fd2, 4) == RHU_PAL_EINVAL, "shim-sock-listen-listen", "(an unbound socket was not answered EINVAL)");
        CHECK(SystemNative_Listen(lfd, 4) == RHU_PAL_SUCCESS && g_nkw.backlog == 4, "shim-sock-listen-listen", "(the backlog was not handed to the kernel)");
        CHECK(SystemNative_GetSocketType(lfd, &family, &type, &protocol, &listening) == RHU_PAL_SUCCESS && listening == 1, "shim-sock-listen-listen", "(the socket was not read as listening)");
        CHECK(SystemNative_GetSocketType(fd2, &family, &type, &protocol, &listening) == RHU_PAL_SUCCESS && listening == 0, "shim-sock-listen-listen", "(a socket that does not listen was read as listening)");
        CHECK(SystemNative_Close(fd2) == 0, "shim-sock-listen-listen", "(close)");
        pass("shim-sock-listen-listen", " (the backlog handed to the kernel and the socket then read as listening; an unbound socket EINVAL)");
    }

    /* no connection completed */
    {
        CHECK(SystemNative_FcntlSetIsNonBlocking(lfd, 1) == 0, "shim-sock-listen-none-ready", "(the listener was not set non-blocking)");
        a1 = 5;
        len = 32;
        CHECK(SystemNative_Accept(lfd, rec, &len, &a1) == RHU_PAL_EAGAIN && a1 == -1, "shim-sock-listen-none-ready", "(a non-blocking accept with none completed did not answer EAGAIN and no descriptor)");
        CHECK(SystemNative_FcntlSetIsNonBlocking(lfd, 0) == 0 && SystemNative_SetReceiveTimeout(lfd, 30) == RHU_PAL_SUCCESS, "shim-sock-listen-none-ready", "(the listener was not set blocking with a bound)");
        a1 = 5;
        len = 32;
        t0 = rhu_now_ns();
        err = SystemNative_Accept(lfd, rec, &len, &a1);
        waited = rhu_now_ns() - t0;
        CHECK(err == RHU_PAL_EAGAIN && a1 == -1 && waited >= 30000000ull, "shim-sock-listen-none-ready", "(a blocking accept did not answer EAGAIN at the receive timeout)");
        CHECK(SystemNative_SetReceiveTimeout(lfd, 0) == RHU_PAL_SUCCESS, "shim-sock-listen-none-ready", "(the bound was not cleared)");
        pass("shim-sock-listen-none-ready", " (with no connection completed a non-blocking accept answers EAGAIN and no descriptor; a blocking accept answers EAGAIN at the receive timeout and not before)");
    }

    /* accept */
    {
        g_nk[lh].ready++;
        nk_bump(&g_nk[lh]);
        memset(rec, 0xEE, sizeof rec);
        len = 32;
        CHECK(SystemNative_Accept(lfd, rec, &len, &a1) == RHU_PAL_SUCCESS && a1 >= 32 && a1 <= 47 && a1 != lfd, "shim-sock-listen-accept", "(a completed connection was not accepted at a new descriptor of the band)");
        CHECK(len == 16 && sin_is(rec, 20, 40001), "shim-sock-listen-accept", "(the accepted connection's peer address was not written)");
        flag = 7;
        CHECK(SystemNative_FcntlGetIsNonBlocking(a1, &flag) == 0 && flag == 0 && SystemNative_GetSocketType(a1, &family, &type, &protocol, &listening) == RHU_PAL_SUCCESS && type == 1 && listening == 0, "shim-sock-listen-accept", "(the accepted socket is not a blocking stream)");
        len = 32;
        CHECK(SystemNative_GetPeerName(a1, rec2, &len) == RHU_PAL_SUCCESS && len == 16 && sin_is(rec2, 20, 40001), "shim-sock-listen-accept", "(the accepted socket's peer name is not the address written)");
        g_nkw.refused_accepts = 0;
        g_nks.accept_at = 2;
        memset(rec, 0xEE, sizeof rec);
        len = 32;
        CHECK(SystemNative_Accept(lfd, rec, &len, &a2) == RHU_PAL_SUCCESS && a2 >= 32 && a2 <= 47 && a2 != a1 && a2 != lfd && len == 16 && sin_is(rec, 20, 40002), "shim-sock-listen-accept", "(a blocking accept did not answer the connection that completed during its wait)");
        g_nks.accept_at = 0;
        pass("shim-sock-listen-accept", " (a completed connection accepted at a new descriptor of the band with the peer's address in a Linux IPv4 record of 16 bytes; the new socket a blocking stream whose peer name is that address; a blocking accept answers a connection that completes during its wait)");
    }

    /* close */
    {
        uint32_t closes = g_nkw.closes;
        CHECK(SystemNative_Close(a1) == 0 && SystemNative_Close(a2) == 0 && SystemNative_Close(lfd) == 0 && g_nkw.closes == closes + 3, "shim-sock-listen-close", "(the three sockets were not each closed through the kernel once)");
        pass("shim-sock-listen-close", " (the listener and the two accepted sockets each closed through the kernel once)");
    }

    out("SHIMTEST-SOCKLISTEN: ALL GOOD (a listening socket over a scripted kernel, from a native caller's side)");
    return 0;
}
#endif /* RHU_SHIMTEST_LADDER_SOCKLISTEN */

#ifdef RHU_SHIMTEST_LADDER_SOCKDATAGRAM

/* --- a datagram socket from a native caller's side: a message sent to the
 * destination in the caller's record and one received with its sender's
 * address, the default peer a connect sets, a datagram larger than the
 * socket's buffer and one larger than the caller's, a message of several
 * buffers leaving as one datagram and one datagram received across several
 * buffers, and a blocking receive whose datagram arrives during its wait. --- */
static int sockdatagram_ladder(void)
{
    static uint8_t big[100];
    uint8_t dest[16], from[32], buf[48], control[24], p0[4], p1[4], p2[8];
    rhu_pal_iovector v[3];
    rhu_pal_messageheader m;
    int64_t count = 0;
    int32_t n = 0, family = 0, type = 0, protocol = 0, listening = 0, h = -1;
    intptr_t fd = -1;
    uint32_t sends = 0;
    out("shimtest-sockdatagram: a datagram socket over a scripted kernel, from a native caller's side");
    CHECK(rhu_shim_init() == 0, "shim-sock-datagram-init", "(initialization refused)");

    /* create */
    {
        CHECK(SystemNative_Socket(RHU_PAL_AddressFamily_AF_INET, RHU_PAL_SocketType_SOCK_DGRAM, RHU_PAL_ProtocolType_PT_UDP, &fd) == RHU_PAL_SUCCESS && fd == 32 && g_nkw.type == 2 && g_nkw.protocol == 17, "shim-sock-datagram-create", "(the kernel was not asked for an IPv4 datagram socket)");
        h = g_nkw.made;
        CHECK(SystemNative_GetSocketType(fd, &family, &type, &protocol, &listening) == RHU_PAL_SUCCESS && family == 2 && type == 2 && protocol == 17 && listening == 0, "shim-sock-datagram-create", "(the type and protocol were not read back)");
        pass("shim-sock-datagram-create", " (an IPv4 datagram socket, its type and protocol read back)");
    }

    /* the family's refusal whatever the pool holds: with both datagram
     * sockets of the pool held, the family is still what the kernel
     * judges first, and the pool's own refusal follows for a family it
     * serves; neither refusal keeps a descriptor of the band */
    {
        intptr_t second = -1, other = -1, next = -1;
        CHECK(SystemNative_Socket(RHU_PAL_AddressFamily_AF_INET, RHU_PAL_SocketType_SOCK_DGRAM, RHU_PAL_ProtocolType_PT_UDP, &second) == RHU_PAL_SUCCESS && second == 33, "shim-sock-datagram-family", "(the pool's second datagram socket was not given)");
        uint32_t asked = g_nkw.sockets;
        CHECK(SystemNative_Socket(RHU_PAL_AddressFamily_AF_INET6, RHU_PAL_SocketType_SOCK_DGRAM, RHU_PAL_ProtocolType_PT_UDP, &other) == RHU_PAL_EAFNOSUPPORT && other == -1 && g_nkw.sockets == asked + 1 && g_nkw.family == RHU_NET_FAMILY_IPV6, "shim-sock-datagram-family", "(an IPv6 socket asked with both datagram sockets held was not refused by the kernel with the family's code)");
        CHECK(SystemNative_Socket(RHU_PAL_AddressFamily_AF_INET, RHU_PAL_SocketType_SOCK_DGRAM, RHU_PAL_ProtocolType_PT_UDP, &other) == RHU_PAL_EMFILE && other == -1 && g_nkw.sockets == asked + 2, "shim-sock-datagram-family", "(a third IPv4 datagram socket was not refused by the kernel with the pool's code)");
        CHECK(SystemNative_Socket(RHU_PAL_AddressFamily_AF_INET, RHU_PAL_SocketType_SOCK_STREAM, RHU_PAL_ProtocolType_PT_TCP, &next) == RHU_PAL_SUCCESS && next == 34, "shim-sock-datagram-family", "(a refused socket kept a descriptor: the next one given is not the band's third)");
        CHECK(SystemNative_Close(next) == 0 && SystemNative_Close(second) == 0, "shim-sock-datagram-family", "(close)");
        pass("shim-sock-datagram-family", " (with both datagram sockets of the kernel's pool held, an IPv6 datagram socket is refused with the family's code, EAFNOSUPPORT, and a third IPv4 one with the pool's, EMFILE; the kernel was asked both times and the band kept no descriptor)");
    }

    /* send to an address */
    {
        memset(&m, 0, sizeof m);
        v[0].Base = (uint8_t *)"datagram";
        v[0].Count = 8;
        m.IOVectors = v;
        m.IOVectorCount = 1;
        count = 5;
        CHECK(SystemNative_SendMessage(fd, (MessageHeader *)&m, 0, &count) == RHU_PAL_ENOTCONN && count == 0, "shim-sock-datagram-send-to", "(a send with no destination and no default peer was not answered ENOTCONN)");
        sin_record(dest, 20, 5353);
        m.SocketAddress = dest;
        m.SocketAddressLen = 16;
        sends = g_nkw.sends;
        CHECK(SystemNative_SendMessage(fd, (MessageHeader *)&m, 0, &count) == RHU_PAL_SUCCESS && count == 8 && g_nkw.sends == sends + 1 && g_nkw.sent_len == 8 && memcmp(g_nkw.sent, "datagram", 8) == 0, "shim-sock-datagram-send-to", "(the datagram was not handed to the kernel whole)");
        CHECK(!g_nkw.to_null && nk_address_is(&g_nkw.to, 20, 5353), "shim-sock-datagram-send-to", "(the kernel was not given the destination of the caller's record)");
        m.ControlBuffer = control;
        m.ControlBufferLen = 24;
        CHECK(SystemNative_SendMessage(fd, (MessageHeader *)&m, 0, &count) == RHU_PAL_ENOTSUP && g_nkw.sends == sends + 1, "shim-sock-datagram-send-to", "(a message that carries control data was not answered ENOTSUP)");
        pass("shim-sock-datagram-send-to", " (a datagram handed to the kernel whole with the destination of the caller's record; with no destination and no default peer ENOTCONN; a message that carries control data ENOTSUP)");
    }

    /* receive from an address */
    {
        nk_datagram_arrives(h, "reply-1", 7, 5353);
        CHECK(SystemNative_GetBytesAvailable(fd, &n) == RHU_PAL_SUCCESS && n == 7, "shim-sock-datagram-receive-from", "(the next datagram's length was not counted as available)");
        memset(from, 0xEE, sizeof from);
        v[0].Base = buf;
        v[0].Count = 32;
        m.SocketAddress = from;
        m.SocketAddressLen = 32;
        m.ControlBuffer = control;
        m.ControlBufferLen = 24;
        m.Flags = 0x55;
        CHECK(SystemNative_ReceiveMessage(fd, (MessageHeader *)&m, 0, &count) == RHU_PAL_SUCCESS && count == 7 && memcmp(buf, "reply-1", 7) == 0, "shim-sock-datagram-receive-from", "(the datagram was not received whole)");
        CHECK(m.SocketAddressLen == 16 && sin_is(from, 20, 5353) && m.ControlBufferLen == 0 && m.Flags == 0, "shim-sock-datagram-receive-from", "(the sender's address, the control length or the flags are not as a datagram's)");
        m.SocketAddressLen = 32;
        count = 5;
        CHECK(SystemNative_FcntlSetIsNonBlocking(fd, 1) == 0 && SystemNative_ReceiveMessage(fd, (MessageHeader *)&m, 0, &count) == RHU_PAL_EAGAIN && count == 0, "shim-sock-datagram-receive-from", "(with none queued a non-blocking receive was not answered EAGAIN)");
        pass("shim-sock-datagram-receive-from", " (a datagram received whole with the sender's address in the caller's record, its length 16, no control data and no flag; the next datagram's length counted as available; none queued EAGAIN on a non-blocking socket)");
    }

    /* the default peer */
    {
        sin_record(dest, 20, 9000);
        CHECK(SystemNative_Connect(fd, dest, 16) == RHU_PAL_SUCCESS && nk_address_is(&g_nkw.connected, 20, 9000), "shim-sock-datagram-default-peer", "(a datagram socket's connect did not set the default peer at once)");
        CHECK(SystemNative_Send(fd, "to-peer", 7, 0, &n) == RHU_PAL_SUCCESS && n == 7 && g_nkw.to_null && g_nkw.sent_len == 7 && memcmp(g_nkw.sent, "to-peer", 7) == 0, "shim-sock-datagram-default-peer", "(a send with no destination did not go to the default peer)");
        nk_datagram_arrives(h, "from-peer", 9, 9000);
        CHECK(SystemNative_Receive(fd, buf, 32, 0, &n) == RHU_PAL_SUCCESS && n == 9 && memcmp(buf, "from-peer", 9) == 0, "shim-sock-datagram-default-peer", "(a receive did not read the default peer's datagram)");
        pass("shim-sock-datagram-default-peer", " (a connect sets the default peer and answers at once; a send with no destination goes to it and a receive reads from it)");
    }

    /* a datagram too large */
    {
        n = 5;
        CHECK(SystemNative_Send(fd, big, 100, 0, &n) == RHU_PAL_EMSGSIZE && n == 0, "shim-sock-datagram-oversize", "(a datagram larger than the socket's buffer was not answered EMSGSIZE on a send)");
        nk_datagram_arrives(h, big, 40, 9000);
        n = 5;
        CHECK(SystemNative_Receive(fd, buf, 16, 0, &n) == RHU_PAL_EMSGSIZE && n == 0, "shim-sock-datagram-oversize", "(a datagram larger than the caller's buffer was not answered EMSGSIZE on a receive)");
        pass("shim-sock-datagram-oversize", " (a datagram larger than the socket's buffer EMSGSIZE on a send; a datagram larger than the caller's buffer EMSGSIZE on a receive, nothing counted)");
    }

    /* several buffers */
    {
        memset(&m, 0, sizeof m);
        v[0].Base = (uint8_t *)"one-";
        v[0].Count = 4;
        v[1].Base = (uint8_t *)"two-";
        v[1].Count = 4;
        v[2].Base = (uint8_t *)"three";
        v[2].Count = 5;
        m.IOVectors = v;
        m.IOVectorCount = 3;
        sends = g_nkw.sends;
        CHECK(SystemNative_SendMessage(fd, (MessageHeader *)&m, 0, &count) == RHU_PAL_SUCCESS && count == 13 && g_nkw.sends == sends + 1 && g_nkw.sent_len == 13 && memcmp(g_nkw.sent, "one-two-three", 13) == 0, "shim-sock-datagram-gather", "(a datagram of several buffers did not leave as one)");
        nk_datagram_arrives(h, "abcdefghijkl", 12, 9000);
        v[0].Base = p0;
        v[0].Count = 4;
        v[1].Base = p1;
        v[1].Count = 4;
        v[2].Base = p2;
        v[2].Count = 8;
        CHECK(SystemNative_ReceiveMessage(fd, (MessageHeader *)&m, 0, &count) == RHU_PAL_SUCCESS && count == 12 && memcmp(p0, "abcd", 4) == 0 && memcmp(p1, "efgh", 4) == 0 && memcmp(p2, "ijkl", 4) == 0, "shim-sock-datagram-gather", "(one datagram was not received across three buffers in order)");
        CHECK(SystemNative_FcntlSetIsNonBlocking(fd, 0) == 0, "shim-sock-datagram-gather", "(the socket was not set blocking)");
        nk_late("late-dgram", 10, 2);
        CHECK(SystemNative_Receive(fd, buf, 32, 0, &n) == RHU_PAL_SUCCESS && n == 10 && memcmp(buf, "late-dgram", 10) == 0, "shim-sock-datagram-gather", "(a blocking receive did not answer the datagram that arrived during its wait)");
        g_nks.late_at = 0;
        CHECK(SystemNative_Close(fd) == 0, "shim-sock-datagram-gather", "(close)");
        pass("shim-sock-datagram-gather", " (three buffers leave as one datagram in one send, their bytes in order; one datagram received across three buffers in order; a blocking receive answers a datagram that arrives during its wait)");
    }

    out("SHIMTEST-SOCKDATAGRAM: ALL GOOD (a datagram socket over a scripted kernel, from a native caller's side)");
    return 0;
}
#endif /* RHU_SHIMTEST_LADDER_SOCKDATAGRAM */

#ifdef RHU_SHIMTEST_LADDER_SOCKOPTIONS

/* --- the socket options from a native caller's side: the kernel's three by
 * their numbers, exclusive address use as the inverse of address reuse, the
 * linger record, the two timeouts each bounding its own direction, the type
 * and the listening state, the two buffer sizes of the board's pool with a
 * write to one answered as success, every other option answered
 * ENOPROTOOPT with the kernel not asked, and the pending error. The
 * option numbers the kernel is read for are written here. --- */
static int get_option(intptr_t fd, int32_t level, int32_t name, int32_t *value)
{
    int32_t len = 4;
    int32_t rc = SystemNative_GetSockOpt(fd, level, name, (uint8_t *)value, &len);
    return rc == RHU_PAL_SUCCESS && len != 4 ? -1 : rc;
}

static int set_option(intptr_t fd, int32_t level, int32_t name, int32_t value)
{
    return SystemNative_SetSockOpt(fd, level, name, (uint8_t *)&value, 4);
}

static int sockoptions_ladder(void)
{
    static const uint8_t twelve[12] = { 'h', 'e', 'l', 'l', 'o', ',', ' ', 'b', 'o', 'a', 'r', 'd' };
    uint8_t rec[16], buf[16];
    int32_t v = 0, len = 0, n = 0, err = 0, h = -1;
    intptr_t fd = -1, dfd = -1, lfd = -1;
    uint64_t t0 = 0, waited = 0;
    out("shimtest-sockoptions: the socket options over a scripted kernel, from a native caller's side");
    CHECK(rhu_shim_init() == 0, "shim-sock-options-init", "(initialization refused)");
    g_nks.tx_room = 16;
    CHECK(nk_stream(&fd, &h, 0), "shim-sock-options-init", "(a connected stream socket)");
    CHECK(SystemNative_Socket(RHU_PAL_AddressFamily_AF_INET, RHU_PAL_SocketType_SOCK_DGRAM, RHU_PAL_ProtocolType_PT_UDP, &dfd) == RHU_PAL_SUCCESS, "shim-sock-options-init", "(a datagram socket)");

    /* the kernel's three */
    {
        CHECK(set_option(fd, RHU_PAL_SocketOptionLevel_SOL_TCP, RHU_PAL_SocketOptionName_SO_TCP_NODELAY, 1) == RHU_PAL_SUCCESS && g_nk[h].opt[1] == 1 && get_option(fd, RHU_PAL_SocketOptionLevel_SOL_TCP, RHU_PAL_SocketOptionName_SO_TCP_NODELAY, &v) == RHU_PAL_SUCCESS && v == 1, "shim-sock-options-kernel", "(no-delay was not written to the kernel as its option 1 and read back)");
        CHECK(set_option(fd, RHU_PAL_SocketOptionLevel_SOL_SOCKET, RHU_PAL_SocketOptionName_SO_KEEPALIVE, 1) == RHU_PAL_SUCCESS && g_nk[h].opt[2] == 1 && get_option(fd, RHU_PAL_SocketOptionLevel_SOL_SOCKET, RHU_PAL_SocketOptionName_SO_KEEPALIVE, &v) == RHU_PAL_SUCCESS && v == 1, "shim-sock-options-kernel", "(keep-alive was not written to the kernel as its option 2 and read back)");
        CHECK(set_option(fd, RHU_PAL_SocketOptionLevel_SOL_SOCKET, RHU_PAL_SocketOptionName_SO_REUSEADDR, 1) == RHU_PAL_SUCCESS && g_nk[h].opt[3] == 1 && get_option(fd, RHU_PAL_SocketOptionLevel_SOL_SOCKET, RHU_PAL_SocketOptionName_SO_REUSEADDR, &v) == RHU_PAL_SUCCESS && v == 1, "shim-sock-options-kernel", "(address reuse was not written to the kernel as its option 3 and read back)");
        CHECK(get_option(fd, RHU_PAL_SocketOptionLevel_SOL_SOCKET, RHU_PAL_SocketOptionName_SO_EXCLUSIVEADDRUSE, &v) == RHU_PAL_SUCCESS && v == 0, "shim-sock-options-kernel", "(exclusive address use was not read as the inverse of address reuse)");
        CHECK(set_option(fd, RHU_PAL_SocketOptionLevel_SOL_SOCKET, RHU_PAL_SocketOptionName_SO_EXCLUSIVEADDRUSE, 1) == RHU_PAL_SUCCESS && g_nk[h].opt[3] == 0 && get_option(fd, RHU_PAL_SocketOptionLevel_SOL_SOCKET, RHU_PAL_SocketOptionName_SO_REUSEADDR, &v) == RHU_PAL_SUCCESS && v == 0, "shim-sock-options-kernel", "(exclusive address use was not written as address reuse off)");
        v = 1;
        len = 2;
        CHECK(SystemNative_SetSockOpt(fd, RHU_PAL_SocketOptionLevel_SOL_TCP, RHU_PAL_SocketOptionName_SO_TCP_NODELAY, (uint8_t *)&v, 2) == RHU_PAL_EINVAL && SystemNative_GetSockOpt(fd, RHU_PAL_SocketOptionLevel_SOL_TCP, RHU_PAL_SocketOptionName_SO_TCP_NODELAY, (uint8_t *)&v, &len) == RHU_PAL_EINVAL, "shim-sock-options-kernel", "(a value shorter than four bytes was not answered EINVAL)");
        pass("shim-sock-options-kernel", " (no-delay, keep-alive and address reuse each written to the kernel by its own number and read back from it; exclusive address use the inverse of address reuse; a value shorter than four bytes EINVAL)");
    }

    /* linger */
    {
        rhu_pal_lingeroption linger = { 9, 9 };
        CHECK(SystemNative_GetLingerOption(fd, (LingerOption *)&linger) == RHU_PAL_SUCCESS && linger.OnOff == 0 && linger.Seconds == 0, "shim-sock-options-linger", "(linger is not off at a socket's start)");
        linger.OnOff = 1;
        linger.Seconds = 5;
        CHECK(SystemNative_SetLingerOption(fd, (LingerOption *)&linger) == RHU_PAL_SUCCESS, "shim-sock-options-linger", "(the linger record was not taken)");
        linger.OnOff = 9;
        linger.Seconds = 9;
        CHECK(SystemNative_GetLingerOption(fd, (LingerOption *)&linger) == RHU_PAL_SUCCESS && linger.OnOff == 1 && linger.Seconds == 5, "shim-sock-options-linger", "(the linger record was not read back)");
        linger.Seconds = -1;
        CHECK(SystemNative_SetLingerOption(fd, (LingerOption *)&linger) == RHU_PAL_EINVAL, "shim-sock-options-linger", "(negative seconds were not answered EINVAL)");
        linger.Seconds = 65536;
        CHECK(SystemNative_SetLingerOption(fd, (LingerOption *)&linger) == RHU_PAL_EINVAL, "shim-sock-options-linger", "(seconds past 65535 were not answered EINVAL)");
        linger.Seconds = 65535;
        CHECK(SystemNative_SetLingerOption(fd, (LingerOption *)&linger) == RHU_PAL_SUCCESS, "shim-sock-options-linger", "(65535 seconds were not taken)");
        CHECK(SystemNative_SetLingerOption(fd, NULL) == RHU_PAL_EFAULT && SystemNative_GetLingerOption(fd, NULL) == RHU_PAL_EFAULT, "shim-sock-options-linger", "(a null record was not answered EFAULT)");
        linger.OnOff = 0;
        CHECK(SystemNative_SetLingerOption(fd, (LingerOption *)&linger) == RHU_PAL_SUCCESS && SystemNative_GetLingerOption(fd, (LingerOption *)&linger) == RHU_PAL_SUCCESS && linger.OnOff == 0, "shim-sock-options-linger", "(linger was not set off)");
        pass("shim-sock-options-linger", " (the linger record kept and read back, off at a socket's start; seconds outside 0 to 65535 EINVAL; a null record EFAULT)");
    }

    /* the two timeouts */
    {
        CHECK(SystemNative_SetReceiveTimeout(fd, 20) == RHU_PAL_SUCCESS && SystemNative_SetSendTimeout(fd, 60) == RHU_PAL_SUCCESS, "shim-sock-options-timeouts", "(the timeouts were not taken)");
        t0 = rhu_now_ns();
        err = SystemNative_Receive(fd, buf, 8, 0, &n);
        waited = rhu_now_ns() - t0;
        CHECK(err == RHU_PAL_EAGAIN && waited >= 20000000ull, "shim-sock-options-timeouts", "(a blocking receive did not answer at the receive timeout)");
        g_nks.tx_room = 0;
        t0 = rhu_now_ns();
        err = SystemNative_Send(fd, (void *)twelve, 12, 0, &n);
        waited = rhu_now_ns() - t0;
        CHECK(err == RHU_PAL_EAGAIN && waited >= 60000000ull, "shim-sock-options-timeouts", "(a blocking send answered before the send timeout)");
        g_nks.tx_room = 16;
        CHECK(SystemNative_SetReceiveTimeout(fd, 0) == RHU_PAL_SUCCESS && SystemNative_SetSendTimeout(fd, 0) == RHU_PAL_SUCCESS, "shim-sock-options-timeouts", "(the bounds were not cleared)");
        pass("shim-sock-options-timeouts", " (the two timeouts kept apart: a blocking receive answers at the receive timeout of 20 ms and a blocking send not before the send timeout of 60 ms)");
    }

    /* the type and the listening state */
    {
        CHECK(get_option(fd, RHU_PAL_SocketOptionLevel_SOL_SOCKET, RHU_PAL_SocketOptionName_SO_TYPE, &v) == RHU_PAL_SUCCESS && v == 1, "shim-sock-options-type", "(a stream socket's type was not read as 1)");
        CHECK(get_option(dfd, RHU_PAL_SocketOptionLevel_SOL_SOCKET, RHU_PAL_SocketOptionName_SO_TYPE, &v) == RHU_PAL_SUCCESS && v == 2, "shim-sock-options-type", "(a datagram socket's type was not read as 2)");
        CHECK(SystemNative_Socket(RHU_PAL_AddressFamily_AF_INET, RHU_PAL_SocketType_SOCK_STREAM, RHU_PAL_ProtocolType_PT_TCP, &lfd) == RHU_PAL_SUCCESS, "shim-sock-options-type", "(socket)");
        sin_record(rec, 10, 8090);
        CHECK(SystemNative_Bind(lfd, RHU_PAL_ProtocolType_PT_TCP, rec, 16) == RHU_PAL_SUCCESS && SystemNative_Listen(lfd, 2) == RHU_PAL_SUCCESS, "shim-sock-options-type", "(listen)");
        CHECK(get_option(lfd, RHU_PAL_SocketOptionLevel_SOL_SOCKET, RHU_PAL_SocketOptionName_SO_ACCEPTCONN, &v) == RHU_PAL_SUCCESS && v == 1, "shim-sock-options-type", "(a listener was not read as accepting connections)");
        CHECK(get_option(fd, RHU_PAL_SocketOptionLevel_SOL_SOCKET, RHU_PAL_SocketOptionName_SO_ACCEPTCONN, &v) == RHU_PAL_SUCCESS && v == 0, "shim-sock-options-type", "(a connected socket was read as accepting connections)");
        CHECK(SystemNative_Close(lfd) == 0, "shim-sock-options-type", "(close)");
        pass("shim-sock-options-type", " (the type read as the runtime's number for a stream and for a datagram socket; a listener read as accepting connections)");
    }

    /* the buffer sizes: the scripted pool's 2048, 1024 and 96 */
    {
        CHECK(get_option(fd, RHU_PAL_SocketOptionLevel_SOL_SOCKET, RHU_PAL_SocketOptionName_SO_RCVBUF, &v) == RHU_PAL_SUCCESS && v == 2048, "shim-sock-options-buffers", "(a stream socket's receive buffer size was not read from the pool)");
        CHECK(get_option(fd, RHU_PAL_SocketOptionLevel_SOL_SOCKET, RHU_PAL_SocketOptionName_SO_SNDBUF, &v) == RHU_PAL_SUCCESS && v == 1024, "shim-sock-options-buffers", "(a stream socket's send buffer size was not read from the pool)");
        CHECK(get_option(dfd, RHU_PAL_SocketOptionLevel_SOL_SOCKET, RHU_PAL_SocketOptionName_SO_RCVBUF, &v) == RHU_PAL_SUCCESS && v == 96 && get_option(dfd, RHU_PAL_SocketOptionLevel_SOL_SOCKET, RHU_PAL_SocketOptionName_SO_SNDBUF, &v) == RHU_PAL_SUCCESS && v == 96, "shim-sock-options-buffers", "(a datagram socket's buffer size was not read from the pool)");
        CHECK(set_option(fd, RHU_PAL_SocketOptionLevel_SOL_SOCKET, RHU_PAL_SocketOptionName_SO_RCVBUF, 65536) == RHU_PAL_SUCCESS && get_option(fd, RHU_PAL_SocketOptionLevel_SOL_SOCKET, RHU_PAL_SocketOptionName_SO_RCVBUF, &v) == RHU_PAL_SUCCESS && v == 2048, "shim-sock-options-buffers", "(a write to a stream's buffer size was not answered as success with the size as it was)");
        CHECK(set_option(dfd, RHU_PAL_SocketOptionLevel_SOL_SOCKET, RHU_PAL_SocketOptionName_SO_SNDBUF, 65536) == RHU_PAL_SUCCESS && get_option(dfd, RHU_PAL_SocketOptionLevel_SOL_SOCKET, RHU_PAL_SocketOptionName_SO_SNDBUF, &v) == RHU_PAL_SUCCESS && v == 96, "shim-sock-options-buffers", "(a write to a datagram socket's buffer size was not answered as success with the size as it was)");
        pass("shim-sock-options-buffers", " (a stream socket's two buffer sizes and a datagram socket's read from the row's pool; a write to a buffer size answered as success and the size as it was)");
    }

    /* the options outside the served set */
    {
        uint32_t asked = g_nkw.options;
        CHECK(set_option(fd, RHU_PAL_SocketOptionLevel_SOL_SOCKET, RHU_PAL_SocketOptionName_SO_BROADCAST, 1) == RHU_PAL_ENOPROTOOPT, "shim-sock-options-refusal", "(an option outside the served set was accepted)");
        CHECK(get_option(fd, RHU_PAL_SocketOptionLevel_SOL_SOCKET, RHU_PAL_SocketOptionName_SO_BROADCAST, &v) == RHU_PAL_ENOPROTOOPT, "shim-sock-options-refusal", "(a read of an option outside the served set was not answered ENOPROTOOPT)");
        CHECK(set_option(fd, RHU_PAL_SocketOptionLevel_SOL_IP, RHU_PAL_SocketOptionName_SO_IP_TTL, 64) == RHU_PAL_ENOPROTOOPT && get_option(fd, RHU_PAL_SocketOptionLevel_SOL_IP, RHU_PAL_SocketOptionName_SO_IP_TTL, &v) == RHU_PAL_ENOPROTOOPT, "shim-sock-options-refusal", "(the time-to-live was not answered ENOPROTOOPT)");
        CHECK(get_option(fd, RHU_PAL_SocketOptionLevel_SOL_SOCKET, RHU_PAL_SocketOptionName_SO_LINGER, &v) == RHU_PAL_ENOPROTOOPT && get_option(fd, RHU_PAL_SocketOptionLevel_SOL_SOCKET, RHU_PAL_SocketOptionName_SO_RCVTIMEO, &v) == RHU_PAL_ENOPROTOOPT && get_option(fd, RHU_PAL_SocketOptionLevel_SOL_SOCKET, RHU_PAL_SocketOptionName_SO_SNDTIMEO, &v) == RHU_PAL_ENOPROTOOPT && get_option(fd, RHU_PAL_SocketOptionLevel_SOL_SOCKET, RHU_PAL_SocketOptionName_SO_ERROR, &v) == RHU_PAL_ENOPROTOOPT, "shim-sock-options-refusal", "(linger, a timeout or the pending error asked by name was not answered ENOPROTOOPT)");
        CHECK(set_option(dfd, RHU_PAL_SocketOptionLevel_SOL_UDP, 1, 1) == RHU_PAL_ENOPROTOOPT && set_option(fd, RHU_PAL_SocketOptionLevel_SOL_SOCKET, RHU_PAL_SocketOptionName_SO_TYPE, 2) == RHU_PAL_ENOPROTOOPT, "shim-sock-options-refusal", "(an option of another level, or a write to the type, was not answered ENOPROTOOPT)");
        CHECK(g_nkw.options == asked, "shim-sock-options-refusal", "(the kernel was asked for an option outside the served set)");
        CHECK(set_option(dfd, RHU_PAL_SocketOptionLevel_SOL_SOCKET, RHU_PAL_SocketOptionName_SO_KEEPALIVE, 1) == RHU_PAL_ENOTSUP && get_option(dfd, RHU_PAL_SocketOptionLevel_SOL_SOCKET, RHU_PAL_SocketOptionName_SO_KEEPALIVE, &v) == RHU_PAL_ENOTSUP, "shim-sock-options-refusal", "(a stream option on a datagram socket was not answered as the kernel's ENOTSUP)");
        pass("shim-sock-options-refusal", " (an option outside the served set ENOPROTOOPT on a read and on a write and the kernel not asked; a stream option on a datagram socket the kernel's ENOTSUP)");
    }

    /* the pending error */
    {
        g_nk[h].pending = RHU_ECONNRESET;
        nk_bump(&g_nk[h]);
        err = 0;
        CHECK(SystemNative_GetSocketErrorOption(fd, &err) == RHU_PAL_SUCCESS && err == RHU_PAL_ECONNRESET, "shim-sock-options-error", "(a reset's pending code was not read as ECONNRESET)");
        CHECK(SystemNative_GetSocketErrorOption(fd, &err) == RHU_PAL_SUCCESS && err == RHU_PAL_SUCCESS, "shim-sock-options-error", "(the pending code was answered twice)");
        CHECK(SystemNative_GetSocketErrorOption(fd, NULL) == RHU_PAL_EFAULT, "shim-sock-options-error", "(a null result pointer was not answered EFAULT)");
        CHECK(SystemNative_Close(fd) == 0 && SystemNative_Close(dfd) == 0, "shim-sock-options-error", "(close)");
        pass("shim-sock-options-error", " (a reset's pending code read once as ECONNRESET, then none; a null result pointer EFAULT)");
    }

    out("SHIMTEST-SOCKOPTIONS: ALL GOOD (the socket options over a scripted kernel, from a native caller's side)");
    return 0;
}
#endif /* RHU_SHIMTEST_LADDER_SOCKOPTIONS */

#ifdef RHU_SHIMTEST_LADDER_SOCKPOLL

/* --- Poll over the socket band from a native caller's side: a socket's
 * level as poll events and the level standing over two polls, the events
 * asked and the two that are always answered, a poll with a bound and one
 * with none that sleeps until bytes arrive, one call over a file of the
 * store and a socket, and a descriptor of the band that is not open. --- */
static int32_t g_pl_h;

static void pl_bytes_arrive(void)
{
    nk_bytes_arrive(g_pl_h, "z", 1);
}

static int pl_poll(rhu_pal_pollevent *pe, uint32_t count, int32_t ms, uint32_t want)
{
    uint32_t trig = 99;
    return SystemNative_Poll(pe, count, ms, &trig) == RHU_PAL_SUCCESS && trig == want;
}

static int sockpoll_ladder(void)
{
    rhu_pal_pollevent pe[2];
    uint8_t rec[16], buf[8];
    int32_t h = -1, h2 = -1, h3 = -1, got = 0;
    intptr_t fd = -1, fd2 = -1, fd3 = -1, file = -1;
    uint64_t t0 = 0, waited = 0;
    uint32_t closes = 0;
    int ok = 0;
    out("shimtest-sockpoll: Poll over the socket band over a scripted kernel, from a native caller's side");
    CHECK(rhu_shim_init() == 0, "shim-sock-poll-init", "(initialization refused)");
    g_nks.tx_room = 16;
    closes = g_nkw.closes;

    /* the level */
    {
        CHECK(nk_stream(&fd, &h, 1), "shim-sock-poll-level", "(a connected stream socket was not made)");
        nk_bytes_arrive(h, "abc", 3);
        pe[0].FileDescriptor = (int32_t)fd;
        pe[0].Events = RHU_PAL_POLLIN | RHU_PAL_POLLOUT;
        pe[0].TriggeredEvents = 0x7F;
        CHECK(pl_poll(pe, 1, 0, 1) && pe[0].TriggeredEvents == (RHU_PAL_POLLIN | RHU_PAL_POLLOUT), "shim-sock-poll-level", "(bytes queued and transmit room were not answered as POLLIN and POLLOUT)");
        CHECK(pl_poll(pe, 1, 0, 1) && pe[0].TriggeredEvents == (RHU_PAL_POLLIN | RHU_PAL_POLLOUT), "shim-sock-poll-level", "(a second poll of a standing level answered no event: an edge-triggered poll)");
        CHECK(SystemNative_Receive(fd, buf, 8, 0, &got) == RHU_PAL_SUCCESS && got == 3, "shim-sock-poll-level", "(the queued bytes were not received)");
        pe[0].Events = RHU_PAL_POLLIN;
        CHECK(pl_poll(pe, 1, 0, 0) && pe[0].TriggeredEvents == 0, "shim-sock-poll-level", "(a quiet socket polled for POLLIN answered an event)");
        pass("shim-sock-poll-level", " (a socket's level as poll events: nothing for a quiet socket, POLLIN and POLLOUT with bytes queued and transmit room; a second poll of the standing level answers the same events)");
    }

    /* the events asked, and the two always answered */
    {
        pe[0].Events = RHU_PAL_POLLOUT;
        CHECK(pl_poll(pe, 1, 0, 1) && pe[0].TriggeredEvents == RHU_PAL_POLLOUT, "shim-sock-poll-events", "(a poll for POLLOUT alone did not answer POLLOUT alone)");
        nk_bytes_arrive(h, "d", 1);
        pe[0].Events = RHU_PAL_POLLIN;
        CHECK(pl_poll(pe, 1, 0, 1) && pe[0].TriggeredEvents == RHU_PAL_POLLIN, "shim-sock-poll-events", "(a poll for POLLIN alone answered an event that was not asked)");
        nk_peer_closes(h);
        CHECK(pl_poll(pe, 1, 0, 1) && pe[0].TriggeredEvents == (RHU_PAL_POLLIN | RHU_PAL_POLLHUP), "shim-sock-poll-events", "(the peer's close did not add POLLHUP)");
        pe[0].Events = 0;
        CHECK(pl_poll(pe, 1, 0, 1) && pe[0].TriggeredEvents == RHU_PAL_POLLHUP, "shim-sock-poll-events", "(POLLHUP was not answered where no event was asked)");
        sin_record(rec, 20, 7);
        CHECK(SystemNative_Socket(RHU_PAL_AddressFamily_AF_INET, RHU_PAL_SocketType_SOCK_STREAM, RHU_PAL_ProtocolType_PT_TCP, &fd2) == RHU_PAL_SUCCESS, "shim-sock-poll-events", "(socket)");
        h2 = g_nkw.made;
        CHECK(SystemNative_FcntlSetIsNonBlocking(fd2, 1) == 0 && SystemNative_Connect(fd2, rec, 16) == RHU_PAL_EINPROGRESS, "shim-sock-poll-events", "(a handshake was not started)");
        pe[0].FileDescriptor = (int32_t)fd2;
        pe[0].Events = RHU_PAL_POLLOUT;
        CHECK(pl_poll(pe, 1, 0, 0) && pe[0].TriggeredEvents == 0, "shim-sock-poll-events", "(a handshake in flight answered an event)");
        nk_end(&g_nk[h2], RHU_ECONNREFUSED);
        CHECK(pl_poll(pe, 1, 0, 1) && pe[0].TriggeredEvents == (RHU_PAL_POLLOUT | RHU_PAL_POLLERR | RHU_PAL_POLLHUP), "shim-sock-poll-events", "(a handshake that did not succeed was not answered as POLLOUT, POLLERR and POLLHUP)");
        pass("shim-sock-poll-events", " (only the events asked are answered; the peer's close adds POLLHUP and a handshake that did not succeed POLLERR and POLLHUP, asked or not; a handshake in flight answers no event)");
    }

    /* a bound, and no bound */
    {
        CHECK(nk_stream(&fd3, &h3, 1), "shim-sock-poll-timeout", "(a connected stream socket was not made)");
        pe[0].FileDescriptor = (int32_t)fd3;
        pe[0].Events = RHU_PAL_POLLIN;
        t0 = rhu_now_ns();
        ok = pl_poll(pe, 1, 30, 0);
        waited = rhu_now_ns() - t0;
        CHECK(ok && pe[0].TriggeredEvents == 0 && waited >= 30000000ull, "shim-sock-poll-timeout", "(a quiet socket polled with a bound of 30 ms did not answer none at the bound)");
        g_pl_h = h3;
        g_nk_script[0] = pl_bytes_arrive;
        CHECK(nk_below_start(), "shim-sock-poll-timeout", "(the second thread did not start)");
        g_nk_open = 1;
        ok = pl_poll(pe, 1, -1, 1);
        CHECK(ok && pe[0].TriggeredEvents == RHU_PAL_POLLIN && g_nk_begun == 1u, "shim-sock-poll-timeout", "(a poll with no bound did not sleep until the bytes' arrival)");
        CHECK(SystemNative_Receive(fd3, buf, 8, 0, &got) == RHU_PAL_SUCCESS && got == 1, "shim-sock-poll-timeout", "(the byte was not received)");
        pass("shim-sock-poll-timeout", " (a quiet socket polled with a bound answers none at the bound and not before; a poll with no bound ends at the bytes' arrival, the caller asleep until then)");
    }

    /* a file and a socket in one call */
    {
        file = SystemNative_Open("/tmp/poll.txt", RHU_PAL_O_RDWR | RHU_PAL_O_CREAT | RHU_PAL_O_TRUNC, 0x1A4);
        CHECK(file >= 3 && file <= 15, "shim-sock-poll-file-and-socket", "(a file of the store did not open)");
        pe[0].FileDescriptor = (int32_t)file;
        pe[0].Events = RHU_PAL_POLLIN | RHU_PAL_POLLOUT;
        pe[1].FileDescriptor = (int32_t)fd3;
        pe[1].Events = RHU_PAL_POLLIN;
        CHECK(pl_poll(pe, 2, 0, 1) && pe[0].TriggeredEvents == (RHU_PAL_POLLIN | RHU_PAL_POLLOUT) && pe[1].TriggeredEvents == 0, "shim-sock-poll-file-and-socket", "(a file and a quiet socket were not answered as the file alone)");
        CHECK(pl_poll(pe, 2, -1, 1) && pe[0].TriggeredEvents == (RHU_PAL_POLLIN | RHU_PAL_POLLOUT) && pe[1].TriggeredEvents == 0, "shim-sock-poll-file-and-socket", "(a ready file did not end a poll with no bound at once)");
        nk_bytes_arrive(h3, "y", 1);
        CHECK(pl_poll(pe, 2, 0, 2) && pe[1].TriggeredEvents == RHU_PAL_POLLIN, "shim-sock-poll-file-and-socket", "(the socket's level was not answered beside the file)");
        CHECK(SystemNative_Receive(fd3, buf, 8, 0, &got) == RHU_PAL_SUCCESS && got == 1, "shim-sock-poll-file-and-socket", "(the byte was not received)");
        pe[0].FileDescriptor = 0;
        pe[0].Events = RHU_PAL_POLLIN;
        t0 = rhu_now_ns();
        ok = pl_poll(pe, 2, 30, 0);
        waited = rhu_now_ns() - t0;
        CHECK(ok && waited >= 30000000ull, "shim-sock-poll-file-and-socket", "(the console's input and a quiet socket did not wait together to the bound)");
        CHECK(SystemNative_Close(file) == 0, "shim-sock-poll-file-and-socket", "(the file did not close)");
        pass("shim-sock-poll-file-and-socket", " (one call over a file of the store and a socket: the file answers at once and the socket its level; the console's input and a quiet socket wait together to the bound)");
    }

    /* a descriptor of the band that is not open */
    {
        pe[0].FileDescriptor = 47;
        pe[0].Events = RHU_PAL_POLLIN;
        CHECK(pl_poll(pe, 1, 0, 1) && pe[0].TriggeredEvents == RHU_PAL_POLLNVAL, "shim-sock-poll-invalid", "(a slot of the band that is not open was not answered POLLNVAL)");
        CHECK(pl_poll(pe, 1, -1, 1) && pe[0].TriggeredEvents == RHU_PAL_POLLNVAL, "shim-sock-poll-invalid", "(a descriptor that is not open did not end a poll with no bound)");
        CHECK(SystemNative_Close(fd2) == 0, "shim-sock-poll-invalid", "(close)");
        pe[0].FileDescriptor = (int32_t)fd2;
        CHECK(pl_poll(pe, 1, 0, 1) && pe[0].TriggeredEvents == RHU_PAL_POLLNVAL, "shim-sock-poll-invalid", "(a closed socket's descriptor was not answered POLLNVAL)");
        pass("shim-sock-poll-invalid", " (a descriptor of the band that is not open answers POLLNVAL and counts as triggered, with or without a bound)");
    }

    /* close */
    {
        CHECK(SystemNative_Close(fd) == 0 && SystemNative_Close(fd3) == 0 && g_nkw.closes == closes + 3, "shim-sock-poll-close", "(the three sockets were not each closed through the kernel once)");
        pass("shim-sock-poll-close", " (the sockets closed through the kernel once each)");
    }

    out("SHIMTEST-SOCKPOLL: ALL GOOD (Poll over the socket band over a scripted kernel, from a native caller's side)");
    return 0;
}
#endif /* RHU_SHIMTEST_LADDER_SOCKPOLL */

#ifdef RHU_SHIMTEST_LADDER_SOCKPORT

/* --- the socket event port from a native caller's side: a port and its
 * buffer, the port table at its edge (four ports together, the fifth
 * refused, a closed port's place given again), the refusals of a
 * registration, a socket's level at its first wait
 * and its risen bits after, several sockets in one wait, a buffer with
 * less room than events, and a registration and a close made by a second
 * thread while the ladder's thread sleeps in its wait. Seven sockets: the
 * first four connected streams, the next two with a handshake in flight,
 * the last made in a closed socket's place. --- */
#define PT_RW (RHU_PAL_SocketEvents_SA_READ | RHU_PAL_SocketEvents_SA_WRITE)
#define PT_DATA(i) ((uintptr_t)0xA0000000u + (uintptr_t)(i))
static intptr_t g_pt_port, g_pt_fd[7];
static int32_t g_pt_h[7];
static uint32_t g_pt_slept; /* a wait that this ladder had to wake itself */

static void pt_bytes_on_second(void)
{
    nk_bytes_arrive(g_pt_h[1], "b", 1);
}

/* A registration while the ladder's thread sleeps in its wait. The unit's
 * move of the one word wakes that thread at once, so it is back before
 * this thread's next step; if it is not, it slept through the
 * registration, and this thread wakes it so that the run can say so. */
static void pt_register_third(void)
{
    (void)SystemNative_TryChangeSocketEventRegistration(g_pt_port, g_pt_fd[2], RHU_PAL_SocketEvents_SA_NONE, PT_RW, PT_DATA(2));
    if (g_nk_back < 2u) {
        g_pt_slept = 1;
        nk_events_move();
    }
}

/* A registered socket closed while the ladder's thread sleeps in its wait,
 * a new socket made at its descriptor with bytes queued, then bytes for
 * the first socket. */
static void pt_close_fourth(void)
{
    (void)SystemNative_Close(g_pt_fd[3]);
    if (nk_stream(&g_pt_fd[6], &g_pt_h[6], 1))
        nk_bytes_arrive(g_pt_h[6], "e", 1);
    nk_bytes_arrive(g_pt_h[0], "a", 1);
}

static int pt_is(const rhu_pal_socketevent *ev, int32_t n, uintptr_t data, int32_t events)
{
    for (int32_t i = 0; i < n; i++) {
        if (ev[i].Data == data)
            return ev[i].Events == events;
    }
    return 0;
}

static int pt_took(intptr_t fd, int32_t count)
{
    uint8_t buf[8];
    int32_t got = -1;
    return SystemNative_Receive(fd, buf, 8, 0, &got) == RHU_PAL_SUCCESS && got == count;
}

static int sockport_ladder(void)
{
    rhu_pal_socketevent *ev = NULL;
    SocketEvent *buffer = NULL;
    intptr_t port2 = -1;
    uint8_t rec[16];
    int32_t n = 0, rc = 0;
    uint32_t closes = 0, takes = 0;
    out("shimtest-sockport: the socket event port over a scripted kernel, from a native caller's side");
    CHECK(rhu_shim_init() == 0, "shim-sock-port-init", "(initialization refused)");
    g_nks.tx_room = 16;

    /* a port and its buffer */
    {
        CHECK(SystemNative_CreateSocketEventPort(NULL) == RHU_PAL_EFAULT, "shim-sock-port-create", "(a null pointer was not answered EFAULT)");
        CHECK(SystemNative_CreateSocketEventPort(&g_pt_port) == RHU_PAL_SUCCESS && g_pt_port > 143, "shim-sock-port-create", "(a port's handle is not a value outside every descriptor band)");
        CHECK(SystemNative_Close(g_pt_port) == -1 && *__errno_location() == RHU_GERR_EBADF, "shim-sock-port-create", "(the descriptor layer's close took a port's handle)");
        CHECK(SystemNative_CreateSocketEventPort(&port2) == RHU_PAL_SUCCESS && port2 > 143 && port2 != g_pt_port, "shim-sock-port-create", "(a second port was not made beside the first)");
        CHECK(SystemNative_CloseSocketEventPort(port2) == RHU_PAL_SUCCESS && SystemNative_CloseSocketEventPort(port2) == RHU_PAL_EBADF, "shim-sock-port-create", "(a port did not close once and answer EBADF after)");
        CHECK(SystemNative_CreateSocketEventBuffer(1024, NULL) == RHU_PAL_EFAULT && SystemNative_CreateSocketEventBuffer(-1, &buffer) == RHU_PAL_EFAULT, "shim-sock-port-create", "(a null pointer or a count below 0 was not answered EFAULT)");
        CHECK(SystemNative_CreateSocketEventBuffer(1024, &buffer) == RHU_PAL_SUCCESS && buffer != NULL, "shim-sock-port-create", "(the event buffer was not made)");
        ev = (rhu_pal_socketevent *)buffer;
        pass("shim-sock-port-create", " (a port's handle outside every descriptor band, a bad descriptor to the descriptor layer's close; a second port beside it; the event buffer made; a null pointer EFAULT)");
    }

    /* the port table at its edge: three more ports beside the first, each
     * closed again before the registrations */
    {
        intptr_t more[3] = { -1, -1, -1 }, fifth = 0, again = -1;
        for (int i = 0; i < 3; i++) {
            CHECK(SystemNative_CreateSocketEventPort(&more[i]) == RHU_PAL_SUCCESS && more[i] > 143 && more[i] != g_pt_port, "shim-sock-port-table", "(four ports did not stand together)");
        }
        CHECK(more[0] != more[1] && more[0] != more[2] && more[1] != more[2], "shim-sock-port-table", "(two ports that stand together share a handle)");
        CHECK(SystemNative_CreateSocketEventPort(&fifth) == RHU_PAL_EMFILE && fifth == -1, "shim-sock-port-table", "(a fifth port was not refused with EMFILE and -1 for its handle)");
        CHECK(SystemNative_CloseSocketEventPort(more[1]) == RHU_PAL_SUCCESS && SystemNative_CreateSocketEventPort(&again) == RHU_PAL_SUCCESS && again == more[1], "shim-sock-port-table", "(a closed port's place was not given again)");
        CHECK(SystemNative_CloseSocketEventPort(more[0]) == RHU_PAL_SUCCESS && SystemNative_CloseSocketEventPort(again) == RHU_PAL_SUCCESS && SystemNative_CloseSocketEventPort(more[2]) == RHU_PAL_SUCCESS,
              "shim-sock-port-table", "(a port of the four did not close)");
        pass("shim-sock-port-table", " (four ports stand together; a fifth is refused with EMFILE and -1 for its handle; a closed port's place is given again)");
    }

    /* the refusals of a registration */
    {
        for (int i = 0; i < 4; i++) {
            CHECK(nk_stream(&g_pt_fd[i], &g_pt_h[i], 1), "shim-sock-port-register", "(a connected stream socket was not made)");
        }
        CHECK(SystemNative_TryChangeSocketEventRegistration(g_pt_port, g_pt_fd[0], RHU_PAL_SocketEvents_SA_NONE, 0x40, PT_DATA(0)) == RHU_PAL_EINVAL
              && SystemNative_TryChangeSocketEventRegistration(g_pt_port, g_pt_fd[0], 0x40, PT_RW, PT_DATA(0)) == RHU_PAL_EINVAL, "shim-sock-port-register", "(an event bit outside the five was not answered EINVAL)");
        CHECK(SystemNative_TryChangeSocketEventRegistration(g_pt_port, g_pt_fd[0], PT_RW, PT_RW, PT_DATA(9)) == RHU_PAL_SUCCESS, "shim-sock-port-register", "(the same events twice were not answered as success)");
        CHECK(SystemNative_TryChangeSocketEventRegistration(g_pt_port, 5, RHU_PAL_SocketEvents_SA_NONE, PT_RW, 0) == RHU_PAL_ENOTSOCK, "shim-sock-port-register", "(a descriptor outside the band was not answered ENOTSOCK)");
        CHECK(SystemNative_TryChangeSocketEventRegistration(g_pt_port, 47, RHU_PAL_SocketEvents_SA_NONE, PT_RW, 0) == RHU_PAL_EBADF
              && SystemNative_TryChangeSocketEventRegistration(port2, g_pt_fd[0], RHU_PAL_SocketEvents_SA_NONE, PT_RW, 0) == RHU_PAL_EBADF, "shim-sock-port-register", "(a slot not open or a port not open was not answered EBADF)");
        CHECK(SystemNative_TryChangeSocketEventRegistration(g_pt_port, g_pt_fd[1], PT_RW, RHU_PAL_SocketEvents_SA_NONE, 0) == RHU_PAL_ENOENT, "shim-sock-port-register", "(a removal with no registration was not answered ENOENT)");
        nk_bytes_arrive(g_pt_h[0], "aa", 2);
        CHECK(SystemNative_TryChangeSocketEventRegistration(g_pt_port, g_pt_fd[0], RHU_PAL_SocketEvents_SA_NONE, PT_RW, PT_DATA(0)) == RHU_PAL_SUCCESS, "shim-sock-port-register", "(the same events twice changed a registration, or a socket was not registered)");
        CHECK(SystemNative_TryChangeSocketEventRegistration(g_pt_port, g_pt_fd[0], RHU_PAL_SocketEvents_SA_NONE, PT_RW, PT_DATA(0)) == RHU_PAL_EEXIST, "shim-sock-port-register", "(a socket registered twice was not answered EEXIST)");
        CHECK(SystemNative_TryChangeSocketEventRegistration(g_pt_port, g_pt_fd[1], RHU_PAL_SocketEvents_SA_NONE, PT_RW, PT_DATA(1)) == RHU_PAL_SUCCESS
              && SystemNative_TryChangeSocketEventRegistration(g_pt_port, g_pt_fd[3], RHU_PAL_SocketEvents_SA_NONE, PT_RW, PT_DATA(3)) == RHU_PAL_SUCCESS, "shim-sock-port-register", "(a socket was not registered)");
        pass("shim-sock-port-register", " (an event bit outside the five EINVAL; the same events twice a success that changes nothing; a descriptor outside the band ENOTSOCK, a slot not open and a port not open EBADF; a socket registered twice EEXIST; a removal with no registration ENOENT)");
    }

    /* the level, once */
    {
        n = 1024;
        CHECK(SystemNative_WaitForSocketEvents(g_pt_port, buffer, &n) == RHU_PAL_SUCCESS && n == 3 && pt_is(ev, n, PT_DATA(0), PT_RW)
              && pt_is(ev, n, PT_DATA(1), RHU_PAL_SocketEvents_SA_WRITE) && pt_is(ev, n, PT_DATA(3), RHU_PAL_SocketEvents_SA_WRITE), "shim-sock-port-level-once", "(the first wait did not report each registered socket's level once with its data word)");
        pass("shim-sock-port-level-once", " (a socket readable before its registration is reported at the first wait with its data word: its level, read and write)");
    }

    /* the risen bits, once */
    {
        g_nk_script[0] = pt_bytes_on_second;
        g_nk_script[1] = pt_register_third;
        g_nk_script[2] = pt_close_fourth;
        CHECK(nk_below_start(), "shim-sock-port-risen-once", "(the second thread did not start)");
        g_nk_open = 1;
        n = 1024;
        rc = SystemNative_WaitForSocketEvents(g_pt_port, buffer, &n);
        g_nk_back = 1;
        CHECK(rc == RHU_PAL_SUCCESS && n == 1 && pt_is(ev, n, PT_DATA(1), RHU_PAL_SocketEvents_SA_READ) && g_nk_begun == 1u, "shim-sock-port-risen-once", "(the port reported socket 1 a second time with no new edge (a level-triggered port))");
        CHECK(pt_took(g_pt_fd[1], 1) && pt_took(g_pt_fd[0], 2), "shim-sock-port-risen-once", "(the queued bytes were not received)");
        nk_bytes_arrive(g_pt_h[0], "x", 1);
        n = 1024;
        CHECK(SystemNative_WaitForSocketEvents(g_pt_port, buffer, &n) == RHU_PAL_SUCCESS && n == 1 && pt_is(ev, n, PT_DATA(0), RHU_PAL_SocketEvents_SA_READ), "shim-sock-port-risen-once", "(bytes that arrived after the queue was emptied were not reported once)");
        CHECK(pt_took(g_pt_fd[0], 1), "shim-sock-port-risen-once", "(the byte was not received)");
        pass("shim-sock-port-risen-once", " (with that level standing the second wait sleeps and reports the next socket's arrival alone; bytes that arrive after a receive emptied the queue are reported once)");
    }

    /* several sockets in one wait */
    {
        sin_record(rec, 20, 7);
        for (int i = 4; i < 6; i++) {
            CHECK(SystemNative_Socket(RHU_PAL_AddressFamily_AF_INET, RHU_PAL_SocketType_SOCK_STREAM, RHU_PAL_ProtocolType_PT_TCP, &g_pt_fd[i]) == RHU_PAL_SUCCESS, "shim-sock-port-several", "(socket)");
            g_pt_h[i] = g_nkw.made;
            CHECK(SystemNative_FcntlSetIsNonBlocking(g_pt_fd[i], 1) == 0 && SystemNative_Connect(g_pt_fd[i], rec, 16) == RHU_PAL_EINPROGRESS
                  && SystemNative_TryChangeSocketEventRegistration(g_pt_port, g_pt_fd[i], RHU_PAL_SocketEvents_SA_NONE, PT_RW, PT_DATA(i)) == RHU_PAL_SUCCESS, "shim-sock-port-several", "(a socket with its handshake in flight was not registered)");
        }
        nk_end(&g_nk[g_pt_h[4]], 0);
        nk_end(&g_nk[g_pt_h[5]], RHU_ECONNREFUSED);
        nk_peer_closes(g_pt_h[1]);
        n = 1024;
        CHECK(SystemNative_WaitForSocketEvents(g_pt_port, buffer, &n) == RHU_PAL_SUCCESS && n == 3 && pt_is(ev, n, PT_DATA(4), RHU_PAL_SocketEvents_SA_WRITE) && pt_is(ev, n, PT_DATA(1), PT_RW)
              && pt_is(ev, n, PT_DATA(5), PT_RW | RHU_PAL_SocketEvents_SA_ERROR), "shim-sock-port-several", "(three sockets were not each reported once with their own events)");
        pass("shim-sock-port-several", " (three sockets in one wait, each once with its own data word: a handshake's end as write, the peer's close as read and write, a handshake that did not succeed as read, write and error)");
    }

    /* a buffer with room for fewer */
    {
        nk_bytes_arrive(g_pt_h[0], "1", 1);
        nk_bytes_arrive(g_pt_h[3], "2", 1);
        nk_bytes_arrive(g_pt_h[4], "3", 1);
        memset(&ev[2], 0xEE, sizeof ev[2]);
        n = 2;
        CHECK(SystemNative_WaitForSocketEvents(g_pt_port, buffer, &n) == RHU_PAL_SUCCESS && n == 2 && pt_is(ev, 2, PT_DATA(0), RHU_PAL_SocketEvents_SA_READ)
              && pt_is(ev, 2, PT_DATA(3), RHU_PAL_SocketEvents_SA_READ) && ev[2].Data == (uintptr_t)0xEEEEEEEEu, "shim-sock-port-buffer-bound", "(room for two events was not answered with two and no record past them)");
        n = 1024;
        CHECK(SystemNative_WaitForSocketEvents(g_pt_port, buffer, &n) == RHU_PAL_SUCCESS && n == 1 && pt_is(ev, n, PT_DATA(4), RHU_PAL_SocketEvents_SA_READ), "shim-sock-port-buffer-bound", "(the third socket's bits were not kept for the next wait)");
        n = 0;
        CHECK(SystemNative_WaitForSocketEvents(g_pt_port, buffer, &n) == RHU_PAL_EINVAL, "shim-sock-port-buffer-bound", "(room for no event was not answered EINVAL)");
        n = 4;
        CHECK(SystemNative_WaitForSocketEvents(g_pt_port, NULL, &n) == RHU_PAL_EFAULT && SystemNative_WaitForSocketEvents(g_pt_port, buffer, NULL) == RHU_PAL_EFAULT, "shim-sock-port-buffer-bound", "(a null pointer was not answered EFAULT)");
        CHECK(pt_took(g_pt_fd[0], 1) && pt_took(g_pt_fd[3], 1) && pt_took(g_pt_fd[4], 1), "shim-sock-port-buffer-bound", "(the queued bytes were not received)");
        pass("shim-sock-port-buffer-bound", " (three sockets risen and room for two: two reported, no record written past them, the third at the next wait; room for none EINVAL; a null buffer EFAULT)");
    }

    /* a registration under a waiter */
    {
        nk_bytes_arrive(g_pt_h[2], "c", 1);
        g_nk_open = 2;
        n = 1024;
        rc = SystemNative_WaitForSocketEvents(g_pt_port, buffer, &n);
        g_nk_back = 2;
        CHECK(g_pt_slept == 0, "shim-sock-port-registration-under-waiter", "(a waiter slept through a registration)");
        CHECK(rc == RHU_PAL_SUCCESS && n == 1 && pt_is(ev, n, PT_DATA(2), PT_RW) && g_nk_begun == 2u, "shim-sock-port-registration-under-waiter", "(the wait did not report the socket registered under it)");
        CHECK(pt_took(g_pt_fd[2], 1), "shim-sock-port-registration-under-waiter", "(the byte was not received)");
        pass("shim-sock-port-registration-under-waiter", " (a socket registered while a wait sleeps wakes it: the wait reports that socket's level)");
    }

    /* a close under a waiter */
    {
        closes = g_nkw.closes;
        takes = g_nk_takes[g_pt_h[0]];
        g_nk_open = 3;
        n = 1024;
        rc = SystemNative_WaitForSocketEvents(g_pt_port, buffer, &n);
        g_nk_back = 3;
        CHECK(rc == RHU_PAL_SUCCESS && n == 1 && pt_is(ev, n, PT_DATA(0), RHU_PAL_SocketEvents_SA_READ), "shim-sock-port-close-under-waiter", "(the port reported a closed socket's registration: its data word for a new socket at its descriptor)");
        CHECK(g_nk_begun == 3u && g_nkw.closes == closes + 1 && g_pt_fd[6] == g_pt_fd[3] && g_nk_takes[g_pt_h[0]] >= takes + 3u, "shim-sock-port-close-under-waiter", "(the waiter did not sleep again with no event after the close and the new socket's bytes)");
        pass("shim-sock-port-close-under-waiter", " (a registered socket closed while a wait sleeps ends its registration: a new socket at its descriptor is not reported, the waiter sleeps again with no event and reports the next arrival alone)");
    }

    /* the port closes */
    {
        closes = g_nkw.closes;
        CHECK(SystemNative_CloseSocketEventPort(g_pt_port) == RHU_PAL_SUCCESS, "shim-sock-port-close", "(the port did not close)");
        n = 4;
        CHECK(SystemNative_TryChangeSocketEventRegistration(g_pt_port, g_pt_fd[0], PT_RW, RHU_PAL_SocketEvents_SA_NONE, 0) == RHU_PAL_EBADF
              && SystemNative_WaitForSocketEvents(g_pt_port, buffer, &n) == RHU_PAL_EBADF && n == 0, "shim-sock-port-close", "(a closed port's handle was not answered EBADF)");
        CHECK(SystemNative_FreeSocketEventBuffer(buffer) == RHU_PAL_SUCCESS, "shim-sock-port-close", "(the buffer was not freed)");
        for (int i = 0; i < 7; i++) {
            if (i != 3) {
                CHECK(SystemNative_Close(g_pt_fd[i]) == 0, "shim-sock-port-close", "(close)");
            }
        }
        CHECK(g_nkw.closes == closes + 6, "shim-sock-port-close", "(the sockets were not each closed through the kernel once)");
        pass("shim-sock-port-close", " (the port closed: its handle then EBADF to a registration and to a wait; the buffer freed; the sockets closed through the kernel once each)");
    }

    out("SHIMTEST-SOCKPORT: ALL GOOD (the socket event port over a scripted kernel, from a native caller's side)");
    return 0;
}
#endif /* RHU_SHIMTEST_LADDER_SOCKPORT */

#ifdef RHU_SHIMTEST_LADDER_SOCKRESOLVE

/* --- name resolution from a native caller's side: a name whose answer
 * arrives while the caller sleeps, the board's own name and localhost
 * with no query, each refusal of the kernel's resolver as the runtime's
 * answer, a filing that finds no slot free, and every slot released. --- */
static void rs_answer_arrives(void)
{
    nk_query_ends(g_nkw2.handle);
}

static int32_t rs_lookup(const char *name, int32_t family, rhu_pal_hostentry *e)
{
    memset(e, 0xEE, sizeof *e);
    return SystemNative_GetHostEntryForName((const uint8_t *)name, family, (HostEntry *)e);
}

static int rs_empty(const rhu_pal_hostentry *e)
{
    return e->CanonicalName == NULL && e->Aliases == NULL && e->IPAddressList == NULL && e->IPAddressCount == 0;
}

/* An entry of one IPv4 address under the name asked; the entry is freed. */
static int rs_one(rhu_pal_hostentry *e, const char *name, uint8_t a, uint8_t b, uint8_t c, uint8_t d)
{
    const uint8_t want[4] = { a, b, c, d };
    int ok = e->CanonicalName != NULL && e->IPAddressList != NULL && strcmp((const char *)e->CanonicalName, name) == 0 && e->Aliases == NULL && e->IPAddressCount == 1
             && memcmp(e->IPAddressList[0].Address, want, 4) == 0 && e->IPAddressList[0].IsIPv6 == 0;
    SystemNative_FreeHostEntry((HostEntry *)e);
    return ok;
}

static int sockresolve_ladder(void)
{
    static const uint8_t first[4] = { 198, 51, 100, 7 }, second[4] = { 198, 51, 100, 8 };
    rhu_pal_hostentry e;
    char own[64], upper[64];
    int32_t rc = 0;
    uint32_t filings = 0;
    uint64_t t0 = 0, waited = 0;
    out("shimtest-sockresolve: name resolution over a scripted kernel, from a native caller's side");
    CHECK(rhu_shim_init() == 0, "shim-sock-resolve-init", "(initialization refused)");
    nk_medium_up();

    /* a name the server answers */
    {
        g_nk_script[0] = rs_answer_arrives;
        CHECK(nk_below_start(), "shim-sock-resolve-name", "(the second thread did not start)");
        g_nk_open = 1;
        rc = rs_lookup("device.example", RHU_PAL_AddressFamily_AF_UNSPEC, &e);
        CHECK(nk_slots_held() == 0, "shim-sock-resolve-name", "(a lookup ended with its resolver slot still held)");
        CHECK(rc == RHU_PAL_GetAddrInfoErrorFlags_EAI_SUCCESS && g_nk_begun == 1u, "shim-sock-resolve-name", "(the lookup did not sleep until the answer and then succeed)");
        CHECK(g_nkw2.name_len == 14 && memcmp(g_nkw2.name, "device.example", 14) == 0, "shim-sock-resolve-name", "(the query was not filed with the name's bytes)");
        CHECK(e.CanonicalName != NULL && strcmp((const char *)e.CanonicalName, "device.example") == 0 && e.Aliases == NULL && e.IPAddressCount == 2 && e.IPAddressList != NULL, "shim-sock-resolve-name", "(the entry does not carry the name asked, no aliases and two addresses)");
        CHECK(memcmp(e.IPAddressList[0].Address, first, 4) == 0 && memcmp(e.IPAddressList[1].Address, second, 4) == 0 && e.IPAddressList[0].IsIPv6 == 0 && e.IPAddressList[1].IsIPv6 == 0
              && e.IPAddressList[0].ScopeId == 0, "shim-sock-resolve-name", "(the addresses are not the kernel's two, as IPv4)");
        SystemNative_FreeHostEntry((HostEntry *)&e);
        SystemNative_FreeHostEntry(NULL);
        CHECK(e.CanonicalName == NULL && e.IPAddressList == NULL && e.IPAddressCount == 0, "shim-sock-resolve-name", "(the freed entry still names its allocations)");
        pass("shim-sock-resolve-name", " (a name the server answers: the query filed with the name's bytes, the caller asleep until the answer, then an entry with the name asked as its canonical name, no aliases and the two IPv4 addresses as the kernel wrote them; the entry's two allocations freed)");
    }

    /* the board's own name */
    {
        CHECK(SystemNative_GetHostName((uint8_t *)own, 64) == 0 && own[0] != '\0', "shim-sock-resolve-own-name", "(the host name was not read)");
        for (uint32_t i = 0; i < 64u; i++) {
            upper[i] = own[i] >= 'a' && own[i] <= 'z' ? (char)(own[i] - ('a' - 'A')) : own[i];
            if (own[i] == '\0')
                break;
        }
        filings = g_nkw2.filings;
        rc = rs_lookup(own, RHU_PAL_AddressFamily_AF_INET, &e);
        CHECK(rc == RHU_PAL_GetAddrInfoErrorFlags_EAI_SUCCESS && rs_one(&e, own, 192, 0, 2, 10), "shim-sock-resolve-own-name", "(the board's own name did not answer the address of the medium's lease)");
        rc = rs_lookup(upper, RHU_PAL_AddressFamily_AF_UNSPEC, &e);
        CHECK(rc == RHU_PAL_GetAddrInfoErrorFlags_EAI_SUCCESS && rs_one(&e, upper, 192, 0, 2, 10), "shim-sock-resolve-own-name", "(the board's own name in another letter case did not answer the lease's address)");
        g_nks2.lease.source = RHU_NET_LEASE_NONE;
        rc = rs_lookup(own, RHU_PAL_AddressFamily_AF_UNSPEC, &e);
        CHECK(rc == RHU_PAL_GetAddrInfoErrorFlags_EAI_SUCCESS && rs_one(&e, own, 127, 0, 0, 1), "shim-sock-resolve-own-name", "(with no lease the board's own name did not answer 127.0.0.1)");
        g_nks2.media = 0;
        rc = rs_lookup(own, RHU_PAL_AddressFamily_AF_UNSPEC, &e);
        CHECK(rc == RHU_PAL_GetAddrInfoErrorFlags_EAI_SUCCESS && rs_one(&e, own, 127, 0, 0, 1), "shim-sock-resolve-own-name", "(with no medium the board's own name did not answer 127.0.0.1)");
        CHECK(g_nkw2.filings == filings, "shim-sock-resolve-own-name", "(a query was filed for the board's own name)");
        nk_medium_up();
        pass("shim-sock-resolve-own-name", " (the board's own name in either letter case files no query: the address of the medium's lease, and 127.0.0.1 with no lease and with no medium)");
    }

    /* localhost */
    {
        filings = g_nkw2.filings;
        rc = rs_lookup("localhost", RHU_PAL_AddressFamily_AF_UNSPEC, &e);
        CHECK(rc == RHU_PAL_GetAddrInfoErrorFlags_EAI_SUCCESS && rs_one(&e, "localhost", 127, 0, 0, 1), "shim-sock-resolve-localhost", "(localhost did not answer 127.0.0.1)");
        rc = rs_lookup("LocalHost", RHU_PAL_AddressFamily_AF_INET, &e);
        CHECK(rc == RHU_PAL_GetAddrInfoErrorFlags_EAI_SUCCESS && rs_one(&e, "LocalHost", 127, 0, 0, 1), "shim-sock-resolve-localhost", "(localhost in another letter case did not answer 127.0.0.1)");
        CHECK(g_nkw2.filings == filings, "shim-sock-resolve-localhost", "(a query was filed for localhost)");
        pass("shim-sock-resolve-localhost", " (localhost in either letter case answers 127.0.0.1 and files no query)");
    }

    /* the refusals */
    {
        g_nks2.end_at_filing = 1;
        g_nks2.end = RHU_EHOSTUNREACH;
        rc = rs_lookup("absent.example", RHU_PAL_AddressFamily_AF_UNSPEC, &e);
        CHECK(rc == RHU_PAL_GetAddrInfoErrorFlags_EAI_NONAME && rs_empty(&e), "shim-sock-resolve-failures", "(a name the server does not have was not answered EAI_NONAME with an empty entry)");
        g_nks2.filing = RHU_EINVAL;
        rc = rs_lookup("bad_name", RHU_PAL_AddressFamily_AF_UNSPEC, &e);
        CHECK(rc == RHU_PAL_GetAddrInfoErrorFlags_EAI_NONAME && rs_empty(&e), "shim-sock-resolve-failures", "(a name the resolver does not take was not answered EAI_NONAME with an empty entry)");
        g_nks2.filing = RHU_ENETUNREACH;
        rc = rs_lookup("device.example", RHU_PAL_AddressFamily_AF_UNSPEC, &e);
        CHECK(rc == RHU_PAL_GetAddrInfoErrorFlags_EAI_AGAIN && rs_empty(&e), "shim-sock-resolve-failures", "(no name server was not answered EAI_AGAIN with an empty entry)");
        g_nks2.filing = 0;
        g_nks2.end = RHU_ETIMEDOUT;
        rc = rs_lookup("device.example", RHU_PAL_AddressFamily_AF_UNSPEC, &e);
        CHECK(rc == RHU_PAL_GetAddrInfoErrorFlags_EAI_AGAIN && rs_empty(&e), "shim-sock-resolve-failures", "(the stack's budget passed was not answered EAI_AGAIN with an empty entry)");
        filings = g_nkw2.filings;
        rc = rs_lookup("device.example", RHU_PAL_AddressFamily_AF_INET6, &e);
        CHECK(rc == RHU_PAL_GetAddrInfoErrorFlags_EAI_FAMILY && rs_empty(&e), "shim-sock-resolve-failures", "(the IPv6 family was not answered EAI_FAMILY with an empty entry)");
        rc = rs_lookup("device.example", 99, &e);
        CHECK(rc == RHU_PAL_GetAddrInfoErrorFlags_EAI_FAMILY && rs_empty(&e) && g_nkw2.filings == filings, "shim-sock-resolve-failures", "(a family outside the three was not answered EAI_FAMILY with no query)");
        CHECK(SystemNative_GetHostEntryForName(NULL, 0, (HostEntry *)&e) == RHU_PAL_GetAddrInfoErrorFlags_EAI_BADARG
              && SystemNative_GetHostEntryForName((const uint8_t *)"device.example", 0, NULL) == RHU_PAL_GetAddrInfoErrorFlags_EAI_BADARG, "shim-sock-resolve-failures", "(a null name or entry was not answered EAI_BADARG)");
        pass("shim-sock-resolve-failures", " (a name the server does not have and a name the resolver does not take answer EAI_NONAME; no name server and the stack's budget passed EAI_AGAIN; the IPv6 family and a family outside the three EAI_FAMILY with no query; a null name or entry EAI_BADARG; each leaves the entry empty)");
    }

    /* no slot free */
    {
        g_nks2.end = 0;
        filings = g_nkw2.filings;
        g_nks2.busy = 1;
        g_nks2.free_at = filings + 1u;
        rc = rs_lookup("device.example", RHU_PAL_AddressFamily_AF_UNSPEC, &e);
        CHECK(rc == RHU_PAL_GetAddrInfoErrorFlags_EAI_SUCCESS && e.IPAddressCount == 2 && g_nkw2.filings == filings + 2u, "shim-sock-resolve-no-slot", "(a filing was not tried again when the one word moved)");
        SystemNative_FreeHostEntry((HostEntry *)&e);
        filings = g_nkw2.filings;
        g_nks2.busy = 1;
        g_nks2.free_at = 0;
        t0 = rhu_now_ns();
        rc = rs_lookup("device.example", RHU_PAL_AddressFamily_AF_UNSPEC, &e);
        waited = rhu_now_ns() - t0;
        CHECK(rc == RHU_PAL_GetAddrInfoErrorFlags_EAI_AGAIN && rs_empty(&e) && waited >= 2000000000ull && g_nkw2.filings == filings + 2u, "shim-sock-resolve-no-slot", "(with no slot freed the lookup did not answer EAI_AGAIN at its bound after one more filing)");
        g_nks2.busy = 0;
        pass("shim-sock-resolve-no-slot", " (with no slot free the filing is tried again when the one word moves and then succeeds; with none freed it answers EAI_AGAIN at its bound and not before, after one more filing)");
    }

    /* every slot released */
    {
        CHECK(nk_slots_held() == 0 && g_nkw2.filed == 4u && g_nkw2.final_reads == 4u, "shim-sock-resolve-slots", "(a slot the kernel handed out was not released by one final read)");
        pass("shim-sock-resolve-slots", " (every slot the kernel handed out was released by one final read: after the answer, after each failure, after the wait for a slot)");
    }

    out("SHIMTEST-SOCKRESOLVE: ALL GOOD (name resolution over a scripted kernel, from a native caller's side)");
    return 0;
}
#endif /* RHU_SHIMTEST_LADDER_SOCKRESOLVE */

#ifdef RHU_SHIMTEST_LADDER_SOCKNETINFO

/* --- the network interface records from a native caller's side: the
 * loopback and the medium in one block with their addresses, the prefix
 * length for three masks, the name and the type by the medium's kind, a
 * link that is down, a medium with no lease, a kernel with no medium, and
 * the enumeration through the caller's three functions. --- */
static struct {
    uint32_t v4, v6, ll;
    void *context;
    char name[2][16], link_name[16];
    rhu_pal_ipaddressinfo address[2];
    rhu_pal_linklayeraddressinfo link;
} g_ni;

static void ni_copy(char *dst, const char *src)
{
    uint32_t i = 0;
    for (; i < 15u && src[i] != '\0'; i++)
        dst[i] = src[i];
    dst[i] = '\0';
}

static void ni_v4(void *context, const char *name, IpAddressInfo *info)
{
    g_ni.context = context;
    if (g_ni.v4 < 2u) {
        ni_copy(g_ni.name[g_ni.v4], name);
        memcpy(&g_ni.address[g_ni.v4], info, sizeof g_ni.address[0]);
    }
    g_ni.v4++;
}

static void ni_v6(void *context, const char *name, IpAddressInfo *info, uint32_t *scope)
{
    (void)context;
    (void)name;
    (void)info;
    (void)scope;
    g_ni.v6++;
}

static void ni_ll(void *context, const char *name, LinkLayerAddressInfo *info)
{
    g_ni.context = context;
    ni_copy(g_ni.link_name, name);
    memcpy(&g_ni.link, info, sizeof g_ni.link);
    g_ni.ll++;
}

static int ni_read(int32_t *ni, rhu_pal_networkinterfaceinfo **ifs, int32_t *na, rhu_pal_ipaddressinfo **addrs)
{
    *ni = -1;
    *na = -1;
    *ifs = NULL;
    *addrs = NULL;
    return SystemNative_GetNetworkInterfaces(ni, (NetworkInterfaceInfo **)ifs, na, (IpAddressInfo **)addrs) == 0;
}

static int socknetinfo_ladder(void)
{
    static const uint8_t hw[6] = { 0x02, 0x00, 0x5E, 0x10, 0x20, 0x30 };
    static const uint8_t lo4[4] = { 127, 0, 0, 1 }, ip4[4] = { 192, 0, 2, 10 };
    static const uint32_t masks[3][5] = { { 255, 255, 240, 0, 20 }, { 255, 255, 255, 255, 32 }, { 255, 0, 0, 0, 8 } };
    rhu_pal_networkinterfaceinfo *ifs = NULL;
    rhu_pal_ipaddressinfo *addrs = NULL;
    int32_t ni = 0, na = 0;
    out("shimtest-socknetinfo: the network interface records over a scripted kernel, from a native caller's side");
    CHECK(rhu_shim_init() == 0, "shim-sock-netinfo-init", "(initialization refused)");
    nk_medium_up();

    /* the records */
    {
        CHECK(SystemNative_GetNetworkInterfaces(NULL, (NetworkInterfaceInfo **)&ifs, &na, (IpAddressInfo **)&addrs) == -1 && *__errno_location() == RHU_GERR_EFAULT, "shim-sock-netinfo-interfaces", "(a null pointer was not answered EFAULT)");
        CHECK(ni_read(&ni, &ifs, &na, &addrs) && ni == 2 && na == 2 && ifs != NULL && (void *)addrs == (void *)(ifs + 2), "shim-sock-netinfo-interfaces", "(the loopback and the medium were not answered in one block, the addresses after the interfaces)");
        CHECK(strcmp(ifs[0].Name, "lo") == 0 && ifs[0].InterfaceIndex == 1 && ifs[0].HardwareType == RHU_PAL_NetworkInterfaceType_Loopback && ifs[0].OperationalState == RHU_PAL_OperationalStatus_Up
              && ifs[0].NumAddressBytes == 0, "shim-sock-netinfo-interfaces", "(the loopback's record is not lo at index 1, up, with no hardware address)");
        CHECK(addrs[0].InterfaceIndex == 1 && addrs[0].NumAddressBytes == 4 && memcmp(addrs[0].AddressBytes, lo4, 4) == 0 && addrs[0].PrefixLength == 8, "shim-sock-netinfo-interfaces", "(the loopback's address is not 127.0.0.1 with a prefix of 8)");
        CHECK(strcmp(ifs[1].Name, "eth0") == 0 && ifs[1].InterfaceIndex == 2 && ifs[1].HardwareType == RHU_PAL_NetworkInterfaceType_Ethernet && ifs[1].OperationalState == RHU_PAL_OperationalStatus_Up,
              "shim-sock-netinfo-interfaces", "(the medium's record is not eth0 at index 2, Ethernet, up)");
        CHECK(ifs[1].NumAddressBytes == 6 && memcmp(ifs[1].AddressBytes, hw, 6) == 0 && ifs[1].Speed == -1 && ifs[1].Mtu == -1 && ifs[1].SupportsMulticast == 0, "shim-sock-netinfo-interfaces", "(the medium's record does not carry its six hardware bytes, or its speed and MTU are not -1)");
        CHECK(addrs[1].InterfaceIndex == 2 && addrs[1].NumAddressBytes == 4 && memcmp(addrs[1].AddressBytes, ip4, 4) == 0, "shim-sock-netinfo-interfaces", "(the medium's address is not the lease's)");
        CHECK(addrs[1].PrefixLength == 24, "shim-sock-netinfo-interfaces", "(an address's prefix length is not the count of its mask's bits)");
        free(ifs);
        pass("shim-sock-netinfo-interfaces", " (the loopback and the medium in one block, the address records after the interface records: lo at index 1 with 127.0.0.1 and a prefix of 8; eth0 at index 2, Ethernet, up, its six hardware bytes, speed and MTU not known, with the lease's address and a prefix of 24; a null pointer EFAULT)");
    }

    /* the prefix length */
    {
        for (int i = 0; i < 3; i++) {
            g_nks2.lease.mask = nk_ip4(masks[i][0], masks[i][1], masks[i][2], masks[i][3]);
            CHECK(ni_read(&ni, &ifs, &na, &addrs) && na == 2 && addrs[1].PrefixLength == masks[i][4], "shim-sock-netinfo-prefix", "(a prefix length is not the count of its mask's bits)");
            free(ifs);
        }
        nk_medium_up();
        pass("shim-sock-netinfo-prefix", " (the prefix length is the count of the mask's bits: 20 for 255.255.240.0, 32 for 255.255.255.255, 8 for 255.0.0.0)");
    }

    /* the name and the type by the medium's kind */
    {
        g_nks2.medium.kind = RHU_NET_MEDIUM_WIRELESS;
        CHECK(ni_read(&ni, &ifs, &na, &addrs) && ni == 2 && strcmp(ifs[1].Name, "wlan0") == 0 && ifs[1].HardwareType == RHU_PAL_NetworkInterfaceType_Wireless80211, "shim-sock-netinfo-kinds", "(a wireless medium is not wlan0 of the wireless type)");
        free(ifs);
        g_nks2.medium.kind = RHU_NET_MEDIUM_VIRTUAL;
        CHECK(ni_read(&ni, &ifs, &na, &addrs) && ni == 2 && strcmp(ifs[1].Name, "eth0") == 0 && ifs[1].HardwareType == RHU_PAL_NetworkInterfaceType_Ethernet, "shim-sock-netinfo-kinds", "(the emulator's medium is not eth0 of the Ethernet type)");
        free(ifs);
        nk_medium_up();
        pass("shim-sock-netinfo-kinds", " (a wireless medium is wlan0 of the wireless type; the emulator's medium is eth0 of the Ethernet type)");
    }

    /* a link that is down, a medium with no lease */
    {
        g_nks2.medium.link = 0;
        CHECK(ni_read(&ni, &ifs, &na, &addrs) && ni == 2 && ifs[1].OperationalState == RHU_PAL_OperationalStatus_Down && ifs[0].OperationalState == RHU_PAL_OperationalStatus_Up, "shim-sock-netinfo-link-down", "(a link that is down was not read as down)");
        free(ifs);
        g_nks2.lease.source = RHU_NET_LEASE_NONE;
        CHECK(ni_read(&ni, &ifs, &na, &addrs) && ni == 2 && na == 1 && addrs[0].InterfaceIndex == 1 && strcmp(ifs[1].Name, "eth0") == 0, "shim-sock-netinfo-link-down", "(a medium with no lease did not keep its record with no address record)");
        free(ifs);
        nk_medium_up();
        pass("shim-sock-netinfo-link-down", " (a link that is down reads as down; a medium with no lease has its record and no address record)");
    }

    /* a kernel with no medium */
    {
        g_nks2.media = 0;
        CHECK(ni_read(&ni, &ifs, &na, &addrs) && ni == 0 && na == 0, "shim-sock-netinfo-no-medium", "(a kernel with no medium answered an interface or an address)");
        free(ifs);
        nk_medium_up();
        pass("shim-sock-netinfo-no-medium", " (a kernel with no medium answers no interface and no address)");
    }

    /* the enumeration */
    {
        memset(&g_ni, 0, sizeof g_ni);
        CHECK(SystemNative_EnumerateInterfaceAddresses(&g_ni, ni_v4, ni_v6, ni_ll) == 0 && g_ni.v4 == 2u && g_ni.v6 == 0u && g_ni.ll == 1u && g_ni.context == (void *)&g_ni, "shim-sock-netinfo-enumerate", "(the three functions were not called twice, never and once with the context)");
        CHECK(strcmp(g_ni.name[0], "lo") == 0 && g_ni.address[0].InterfaceIndex == 1 && memcmp(g_ni.address[0].AddressBytes, lo4, 4) == 0 && g_ni.address[0].PrefixLength == 8, "shim-sock-netinfo-enumerate", "(the loopback's address was not handed to the IPv4 function)");
        CHECK(strcmp(g_ni.name[1], "eth0") == 0 && g_ni.address[1].InterfaceIndex == 2 && g_ni.address[1].NumAddressBytes == 4 && memcmp(g_ni.address[1].AddressBytes, ip4, 4) == 0 && g_ni.address[1].PrefixLength == 24,
              "shim-sock-netinfo-enumerate", "(the medium's address and prefix were not handed to the IPv4 function)");
        CHECK(strcmp(g_ni.link_name, "eth0") == 0 && g_ni.link.InterfaceIndex == 2 && g_ni.link.NumAddressBytes == 6 && memcmp(g_ni.link.AddressBytes, hw, 6) == 0
              && g_ni.link.HardwareType == RHU_PAL_NetworkInterfaceType_Ethernet, "shim-sock-netinfo-enumerate", "(the medium's hardware address was not handed to the link-layer function)");
        memset(&g_ni, 0, sizeof g_ni);
        CHECK(SystemNative_EnumerateInterfaceAddresses(&g_ni, NULL, NULL, ni_ll) == 0 && g_ni.v4 == 0u && g_ni.ll == 1u, "shim-sock-netinfo-enumerate", "(a null function was not skipped)");
        pass("shim-sock-netinfo-enumerate", " (the enumeration calls the IPv4 function for the loopback and for the medium's address with its prefix, the link-layer function for the medium's hardware address, the IPv6 function never; the context handed through; a null function skipped)");
    }

    out("SHIMTEST-SOCKNETINFO: ALL GOOD (the network interface records over a scripted kernel, from a native caller's side)");
    return 0;
}
#endif /* RHU_SHIMTEST_LADDER_SOCKNETINFO */

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
#elif defined(RHU_SHIMTEST_LADDER_PWMROWS)
    (void)argc;
    return pwmrows_ladder();
#elif defined(RHU_SHIMTEST_LADDER_IIOFS)
    (void)argc;
    return iiofs_ladder();
#elif defined(RHU_SHIMTEST_LADDER_SDFS)
    (void)argc;
    return sdfs_ladder();
#elif defined(RHU_SHIMTEST_LADDER_TTYDEV)
    (void)argc;
    return ttydev_ladder();
#elif defined(RHU_SHIMTEST_LADDER_FROZEN)
    (void)argc;
    return frozen_ladder();
#elif defined(RHU_SHIMTEST_LADDER_SOCKADDR)
    (void)argc;
    return sockaddr_ladder();
#elif defined(RHU_SHIMTEST_LADDER_SOCKERRORS)
    (void)argc;
    return sockerrors_ladder();
#elif defined(RHU_SHIMTEST_LADDER_SOCKREFUSALS)
    (void)argc;
    return sockrefusals_ladder();
#elif defined(RHU_SHIMTEST_LADDER_SOCKSTREAM)
    (void)argc;
    return sockstream_ladder();
#elif defined(RHU_SHIMTEST_LADDER_SOCKLISTEN)
    (void)argc;
    return socklisten_ladder();
#elif defined(RHU_SHIMTEST_LADDER_SOCKDATAGRAM)
    (void)argc;
    return sockdatagram_ladder();
#elif defined(RHU_SHIMTEST_LADDER_SOCKOPTIONS)
    (void)argc;
    return sockoptions_ladder();
#elif defined(RHU_SHIMTEST_LADDER_SOCKPOLL)
    (void)argc;
    return sockpoll_ladder();
#elif defined(RHU_SHIMTEST_LADDER_SOCKPORT)
    (void)argc;
    return sockport_ladder();
#elif defined(RHU_SHIMTEST_LADDER_SOCKRESOLVE)
    (void)argc;
    return sockresolve_ladder();
#elif defined(RHU_SHIMTEST_LADDER_SOCKNETINFO)
    (void)argc;
    return socknetinfo_ladder();
#else
    return host_checks(argc);
#endif
}
