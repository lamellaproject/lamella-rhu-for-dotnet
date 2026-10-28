/* The C runtime this image carries in place of a C library: the memory
 * allocation family over the kernel heap, the console file descriptors at
 * the C-runtime level, process exit, and the few string functions the
 * compiler's builtins do not provide. Every path out of the process states
 * itself on the console before the machine exits.
 *
 * This file is also the retarget when the toolchain's C library IS linked
 * beside it: the block at the end answers the names that library's own
 * internals reach for, on the two rules that one heap and one console
 * serve the whole image. */
#include <stddef.h>
#include <stdint.h>

#include "rhu.h"
#include "rhu_pal_errno_net11_rc2.h"
#include "rhu_shim.h"

extern int *__errno_location(void);
void *memcpy(void *dst, const void *src, size_t n);
void *memset(void *dst, int c, size_t n);

#define SET_ERRNO(e) (*__errno_location() = (e))

/* --- the allocation family ---
 * Every block carries an 8-byte header ahead of the payload: the requested
 * size, and a tag whose high half is a fixed pattern and whose low half is
 * the distance from the payload back to the kernel block. The size makes
 * realloc possible without a kernel size query; the tag makes a free of a
 * foreign or corrupted pointer die loud before the kernel heap sees it
 * (the kernel's own header check is the second, independent tripwire).
 * A plain payload sits 8 bytes past the kernel block and is 8-aligned; an
 * aligned request above 8 places the payload at the requested alignment
 * inside a larger block and records the distance in the tag. */

typedef struct glue_header {
    uint32_t size;
    uint32_t tag;
} glue_header;

#define GLUE_TAG_PATTERN 0x52480000u
#define GLUE_TAG_MASK 0xFFFF0000u
#define GLUE_LEAD_MASK 0x0000FFFFu
#define GLUE_HEADER_BYTES ((uint32_t)sizeof(glue_header))
#define GLUE_ALIGN_MAX 4096u

static int glue_align_ok(uint32_t align)
{
    return align != 0 && (align & (align - 1u)) == 0 && align <= GLUE_ALIGN_MAX;
}

/* One block: the payload is `lead` bytes past the kernel block, where
 * `lead` is the header size for plain requests and the alignment for
 * aligned ones. */
static void *glue_alloc(uint32_t size, uint32_t align)
{
    if (size == 0)
        size = 1;
    if (align < 8u)
        align = 8u;
    uint32_t lead = align < GLUE_HEADER_BYTES ? GLUE_HEADER_BYTES : align;
    if (size > 0xFFFFFFFFu - lead)
        return NULL;
    uint8_t *base = (uint8_t *)rhu_kheap_alloc(size + lead, align);
    if (base == NULL)
        return NULL;
    uint8_t *payload = base + lead;
    glue_header *h = (glue_header *)(payload - GLUE_HEADER_BYTES);
    h->size = size;
    h->tag = GLUE_TAG_PATTERN | lead;
    return payload;
}

/* The header of a live payload; a tag without the pattern is a foreign or
 * corrupted pointer and dies loud here. */
static glue_header *glue_check(void *p, const char *who)
{
    glue_header *h = (glue_header *)((uint8_t *)p - GLUE_HEADER_BYTES);
    if ((h->tag & GLUE_TAG_MASK) != GLUE_TAG_PATTERN) {
        rhu_shim_log("%s: corrupt block header (canary)", who);
        rhu_abort(134);
    }
    return h;
}

void *malloc(size_t n)
{
    return glue_alloc((uint32_t)n, 8u);
}

void free(void *p)
{
    if (p == NULL)
        return;
    glue_header *h = glue_check(p, "free");
    uint32_t lead = h->tag & GLUE_LEAD_MASK;
    h->tag = 0;
    rhu_kheap_free((uint8_t *)p - lead);
}

void *calloc(size_t n, size_t sz)
{
    size_t total = n * sz;
    if (sz != 0 && total / sz != n)
        return NULL;
    void *p = glue_alloc((uint32_t)total, 8u);
    if (p != NULL)
        memset(p, 0, total);
    return p;
}

void *realloc(void *p, size_t n)
{
    if (p == NULL)
        return malloc(n);
    if (n == 0) {
        free(p);
        return NULL;
    }
    glue_header *h = glue_check(p, "realloc");
    if ((uint32_t)n <= h->size) {
        h->size = (uint32_t)n;
        return p;
    }
    void *q = glue_alloc((uint32_t)n, 8u);
    if (q == NULL)
        return NULL;
    memcpy(q, p, h->size);
    free(p);
    return q;
}

size_t malloc_usable_size(void *p)
{
    if (p == NULL)
        return 0;
    return glue_check(p, "malloc_usable_size")->size;
}

void *memalign(size_t align, size_t n)
{
    if (!glue_align_ok((uint32_t)align)) {
        SET_ERRNO(RHU_GERR_EINVAL);
        return NULL;
    }
    return glue_alloc((uint32_t)n, (uint32_t)align);
}

void *aligned_alloc(size_t align, size_t n)
{
    return memalign(align, n);
}

int posix_memalign(void **out, size_t align, size_t n)
{
    if (out == NULL || !glue_align_ok((uint32_t)align) || align < sizeof(void *))
        return RHU_GERR_EINVAL;
    void *p = glue_alloc((uint32_t)n, (uint32_t)align);
    if (p == NULL)
        return RHU_GERR_ENOMEM;
    *out = p;
    return 0;
}

/* --- the console file descriptors --- */

/* The console door takes at most RHU_CONSOLE_MAX_WRITE bytes a call and
 * answers the count: a larger write goes out in bounded pieces and the
 * count written is answered (one call takes at most 0x7FFFFFFF bytes,
 * a count that stays a positive long). A NULL buffer with a count is
 * EFAULT; a refusal after some bytes went answers the count so far,
 * before any EIO. */
static long console_write_pieces(const void *buf, size_t n)
{
    if (n == 0)
        return 0;
    if (buf == NULL) {
        SET_ERRNO(RHU_GERR_EFAULT);
        return -1;
    }
    if (n > 0x7FFFFFFFu)
        n = 0x7FFFFFFFu;
    const uint8_t *p = (const uint8_t *)buf;
    size_t done = 0;
    while (done < n) {
        size_t piece = n - done;
        if (piece > RHU_CONSOLE_MAX_WRITE)
            piece = RHU_CONSOLE_MAX_WRITE;
        int32_t rc = rhu_console_write(p + done, (uint32_t)piece);
        if (rc < 0) {
            if (done > 0)
                return (long)done;
            SET_ERRNO(RHU_GERR_EIO);
            return -1;
        }
        if (rc == 0)
            break;
        done += (size_t)rc;
    }
    return (long)done;
}

long write(int fd, const void *buf, size_t n)
{
    if (fd == 1 || fd == 2)
        return console_write_pieces(buf, n);
    if (fd >= RHU_SHIM_FD_TTY_LO && fd < RHU_SHIM_FD_TTY_LO + RHU_SHIM_FD_TTY_COUNT)
        return rhu_ttydev_write(fd, buf, n);
    SET_ERRNO(RHU_GERR_EBADF);
    return -1;
}

long read(int fd, void *buf, size_t n)
{
    if (fd == 0)
        return rhu_console_read(buf, (uint32_t)n);
    if (fd >= RHU_SHIM_FD_TTY_LO && fd < RHU_SHIM_FD_TTY_LO + RHU_SHIM_FD_TTY_COUNT)
        return rhu_ttydev_read(fd, buf, n);
    SET_ERRNO(RHU_GERR_EBADF);
    return -1;
}

/* The C library's access, for the standard package's own check of a PWM
 * channel's directory and for a program's check of an analog input file: a
 * path under /sys/bus goes to the analog input device files, any other path
 * under /sys to the PWM device files; this library has no other path to
 * answer for. */
int access(const char *path, int mode)
{
    if (rhu_iiofs_owns(path))
        return rhu_iiofs_access(path, mode);
    if (rhu_pwmfs_owns(path))
        return rhu_pwmfs_access(path, mode);
    SET_ERRNO(RHU_GERR_ENOENT);
    return -1;
}

/* --- process exit: every path states itself, then the machine exits --- */

void _exit(int code)
{
    rhu_shim_exit(code);
}

void exit(int code)
{
    rhu_shim_exit(code);
}

void abort(void)
{
    rhu_shim_log("abort() from thread %d", rhu_thread_self());
    rhu_abort(134);
}

/* --- the string functions the compiler builtins do not provide --- */

int strcmp(const char *a, const char *b)
{
    while (*a != 0 && *a == *b) {
        a++;
        b++;
    }
    return (int)(unsigned char)*a - (int)(unsigned char)*b;
}

int strncmp(const char *a, const char *b, size_t n)
{
    while (n != 0) {
        if (*a != *b || *a == 0)
            return (int)(unsigned char)*a - (int)(unsigned char)*b;
        a++;
        b++;
        n--;
    }
    return 0;
}

char *strchr(const char *s, int c)
{
    for (;; s++) {
        if (*s == (char)c)
            return (char *)s;
        if (*s == 0)
            return NULL;
    }
}

char *strrchr(const char *s, int c)
{
    const char *last = NULL;
    for (;; s++) {
        if (*s == (char)c)
            last = s;
        if (*s == 0)
            return (char *)last;
    }
}

char *strcpy(char *dst, const char *src)
{
    char *d = dst;
    while ((*d++ = *src++) != 0) {
    }
    return dst;
}

char *strncpy(char *dst, const char *src, size_t n)
{
    size_t i = 0;
    for (; i < n && src[i] != 0; i++)
        dst[i] = src[i];
    for (; i < n; i++)
        dst[i] = 0;
    return dst;
}

/* --- the C library retarget ---
 *
 * When the toolchain's C library is linked beside this file, its own
 * internals reach for a small set of names a hosted system would provide.
 * Two rules govern what they get.
 *
 * ONE ALLOCATOR. The allocation family above is defined strongly, and the
 * reentrant spellings the library's internals call forward to it, so the
 * library's own allocator is never pulled in and there is exactly one heap
 * in the image. The break request below is the guard on that rule: it
 * always refuses, so the library's allocator can never start.
 *
 * ONE CONSOLE. The descriptor-level calls the library makes route to the
 * same console this image already writes through, so a line printed by the
 * library and a line printed by the shim arrive on the same wire in the
 * order they were written.
 */

struct rhu_libc_stat;

/* The heap break: always refused, so the library's allocator cannot live
 * beside the one above. */
void *_sbrk(int increment)
{
    (void)increment;
    SET_ERRNO(RHU_GERR_ENOMEM);
    return (void *)-1;
}

/* The reentrant allocation spellings the library's internals call. The
 * per-task pointer they pass is not used: this image has one heap. */
void *_malloc_r(void *reent, size_t n)
{
    (void)reent;
    return malloc(n);
}

void _free_r(void *reent, void *p)
{
    (void)reent;
    free(p);
}

void *_realloc_r(void *reent, void *p, size_t n)
{
    (void)reent;
    return realloc(p, n);
}

void *_calloc_r(void *reent, size_t n, size_t sz)
{
    (void)reent;
    return calloc(n, sz);
}

void *_memalign_r(void *reent, size_t align, size_t n)
{
    (void)reent;
    return memalign(align, n);
}

/* The descriptor-level calls, onto the console this image already uses. */
long _write(int fd, const void *buf, size_t n)
{
    return write(fd, buf, n);
}

long _read(int fd, void *buf, size_t n)
{
    return read(fd, buf, n);
}

int _close(int fd)
{
    (void)fd;
    SET_ERRNO(RHU_GERR_EBADF);
    return -1;
}

long _lseek(int fd, long off, int whence)
{
    (void)fd;
    (void)off;
    (void)whence;
    SET_ERRNO(RHU_GERR_ESPIPE);
    return -1;
}

int _fstat(int fd, struct rhu_libc_stat *st)
{
    (void)fd;
    (void)st;
    SET_ERRNO(RHU_GERR_EBADF);
    return -1;
}

int _isatty(int fd)
{
    if (fd >= RHU_SHIM_FD_CONSOLE_LO && fd < RHU_SHIM_FD_CONSOLE_HI)
        return 1;
    SET_ERRNO(RHU_GERR_ENOTTY);
    return 0;
}

int _getpid(void)
{
    return 1;
}

int _kill(int pid, int sig)
{
    (void)pid;
    (void)sig;
    SET_ERRNO(RHU_GERR_EPERM);
    return -1;
}

/* The handle a shared object's destructors are registered under (the C++
 * runtime's exit-time registration takes it as its third argument): this
 * image is the one object there is, so the handle is its own address. */
void *__dso_handle = &__dso_handle;

/* The library's exit walks its own finalizer array through these; this
 * image has nothing to finalize and says so by having nothing to do. */
void _init(void)
{
}

void _fini(void)
{
}

/* --- the C library's lock family ------------------------------------ */

/* The library's own lock member carries empty locks, a single-threaded
 * library's defaults. Hosted threads share one standard output through
 * the library's line buffer, and under empty locks two threads' lines
 * interleave inside it. This image answers the whole family here, so the
 * library's member is never pulled: one lock word per lock over the
 * kernel futex (free, held, held with a waiter possible), recursion by
 * the holder's thread id; the named locks the library's internals reach
 * for are defined below by their library names. A lock the library
 * never initialized (a null handle) is left as the library left it. */
struct __lock {
    uint32_t word;
    uint32_t owner;
    uint32_t depth;
};
struct __lock __lock___sfp_recursive_mutex;
struct __lock __lock___atexit_recursive_mutex;
struct __lock __lock___at_quick_exit_mutex;
struct __lock __lock___malloc_recursive_mutex;
struct __lock __lock___env_recursive_mutex;
struct __lock __lock___tz_mutex;
struct __lock __lock___dd_hash_mutex;
struct __lock __lock___arc4random_mutex;

static void lock_take(struct __lock *l)
{
    uint32_t c = 0u;
    if (__atomic_compare_exchange_n(&l->word, &c, 1u, 0, __ATOMIC_ACQUIRE, __ATOMIC_RELAXED))
        return;
    if (c != 2u)
        c = __atomic_exchange_n(&l->word, 2u, __ATOMIC_ACQUIRE);
    while (c != 0u) {
        (void)rhu_futex_wait(&l->word, 2u, RHU_DEADLINE_NONE);
        c = __atomic_exchange_n(&l->word, 2u, __ATOMIC_ACQUIRE);
    }
}

static void lock_drop(struct __lock *l)
{
    if (__atomic_fetch_sub(&l->word, 1u, __ATOMIC_RELEASE) != 1u) {
        __atomic_store_n(&l->word, 0u, __ATOMIC_RELEASE);
        (void)rhu_futex_wake(&l->word, 1);
    }
}

static int lock_try(struct __lock *l)
{
    uint32_t c = 0u;
    return __atomic_compare_exchange_n(&l->word, &c, 1u, 0, __ATOMIC_ACQUIRE, __ATOMIC_RELAXED) ? 1 : 0;
}

void __retarget_lock_init(struct __lock **lock)
{
    *lock = (struct __lock *)calloc(1u, sizeof(struct __lock));
}

void __retarget_lock_init_recursive(struct __lock **lock)
{
    *lock = (struct __lock *)calloc(1u, sizeof(struct __lock));
}

void __retarget_lock_close(struct __lock *lock)
{
    free(lock);
}

void __retarget_lock_close_recursive(struct __lock *lock)
{
    free(lock);
}

void __retarget_lock_acquire(struct __lock *lock)
{
    if (lock != NULL)
        lock_take(lock);
}

void __retarget_lock_acquire_recursive(struct __lock *lock)
{
    if (lock == NULL)
        return;
    uint32_t me = (uint32_t)rhu_thread_self();
    if (lock->depth != 0u && lock->owner == me) {
        lock->depth++;
        return;
    }
    lock_take(lock);
    lock->owner = me;
    lock->depth = 1u;
}

int __retarget_lock_try_acquire(struct __lock *lock)
{
    return lock == NULL ? 1 : lock_try(lock);
}

int __retarget_lock_try_acquire_recursive(struct __lock *lock)
{
    if (lock == NULL)
        return 1;
    uint32_t me = (uint32_t)rhu_thread_self();
    if (lock->depth != 0u && lock->owner == me) {
        lock->depth++;
        return 1;
    }
    if (!lock_try(lock))
        return 0;
    lock->owner = me;
    lock->depth = 1u;
    return 1;
}

void __retarget_lock_release(struct __lock *lock)
{
    if (lock != NULL)
        lock_drop(lock);
}

void __retarget_lock_release_recursive(struct __lock *lock)
{
    if (lock == NULL || lock->depth == 0u)
        return;
    if (--lock->depth == 0u) {
        lock->owner = 0u;
        lock_drop(lock);
    }
}
