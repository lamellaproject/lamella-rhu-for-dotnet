/* The SystemNative_* surface of the .NET runtime's platform abstraction
 * at this hosting: error identities in both directions, the file surface
 * (the console at descriptors 0-2, a RAM-backed file store at 3-15),
 * memory mapping with the frozen-object reservation guard, the malloc
 * family, the threading primitives on the kernel futex, time, random
 * bytes, and what the class library asks about the process it runs in:
 * its identity, its working directory, its environment and its log
 * streams. Managed callers pass and receive the PAL's own constants and
 * structure layouts (the generated headers), never a platform's; errno
 * crosses the boundary through the generated mapping in both directions.
 * Every descriptor belongs to exactly one band, and the console never
 * answers for a file. */
#include <stddef.h>
#include <stdint.h>

#ifdef RHU_SHIM_BREAK_FD_BASE
/* A test knob: places the file band at the console's base to reproduce the
 * failure the bands exist to prevent. Never set in a hosted image. */
#define RHU_SHIM_FD_FILE_LO 0
#endif

#include "rhu_shim.h"

#include "mgc_board_rows.h"
#include "rhu.h"
#include "rhu_glibc_arm32.h"
#include "rhu_pal_console_net11_rc1.h"
#include "rhu_pal_environment_net11_rc1.h"
#include "rhu_pal_errno_net11_rc1.h"
#include "rhu_pal_io_net11_rc1.h"
#include "rhu_pal_log_net11_rc1.h"
#include "rhu_pal_memory_net11_rc1.h"
#include "rhu_pal_process_net11_rc1.h"
#include "rhu_pal_random_net11_rc1.h"
#include "rhu_pal_signal_net11_rc1.h"
#include "rhu_pal_threading_net11_rc1.h"
#include "rhu_pal_time_net11_rc1.h"
#include "rhu_pal_uid_net11_rc1.h"

#ifndef RHU_SHIM_BREAK_FD_BASE
_Static_assert(RHU_SHIM_FD_FILE_LO >= RHU_SHIM_FD_CONSOLE_HI, "the file band starts above the console band");
#endif

extern int *__errno_location(void);
void *malloc(size_t n);
void free(void *p);
void *realloc(void *p, size_t n);
void *memalign(size_t align, size_t n);
size_t malloc_usable_size(void *p);
void *mmap64(void *addr, size_t len, int prot, int flags, int fd, int64_t off);
void *memcpy(void *dst, const void *src, size_t n);
void *memset(void *dst, int c, size_t n);
size_t strlen(const char *s);
int strcmp(const char *a, const char *b);
int strncmp(const char *a, const char *b, size_t n);
char *strchr(const char *s, int c);
char *strrchr(const char *s, int c);
char *strcpy(char *dst, const char *src);
char *getenv(const char *name);
int pthread_mutex_init(void *slot, const void *attr);
int pthread_mutex_lock(void *slot);
int pthread_mutex_unlock(void *slot);

#define ERRNO (*__errno_location())
#define NS_PER_S 1000000000ull
#define PAGE_BYTES 4096u

/* ------------------------------------------------------------------ */
/* errno: both directions from the one generated list                  */
/* ------------------------------------------------------------------ */

int32_t SystemNative_ConvertErrorPlatformToPal(int32_t platformErrno)
{
    switch (platformErrno) {
    case 0:
        return RHU_PAL_SUCCESS;
#define X(pal, plat, name) \
    case (plat):           \
        return (pal);
        RHU_PAL_ERRNO_MAP(X)
#undef X
    default:
        return RHU_PAL_ENONSTANDARD;
    }
}

int32_t SystemNative_ConvertErrorPalToPlatform(int32_t error)
{
    switch (error) {
    case RHU_PAL_SUCCESS:
        return 0;
#define X(pal, plat, name) \
    case (pal):            \
        return (plat);
        RHU_PAL_ERRNO_MAP(X)
#undef X
    default:
        return RHU_GERR_EINVAL;
    }
}

int32_t SystemNative_GetErrNo(void)
{
    return SystemNative_ConvertErrorPlatformToPal(ERRNO);
}

void SystemNative_SetErrNo(int32_t errorCode)
{
    ERRNO = SystemNative_ConvertErrorPalToPlatform(errorCode);
}

static const char *errno_name(int32_t platformErrno)
{
#define X(pal, plat, name) \
    if (platformErrno == (plat))   \
        return name;
    RHU_PAL_ERRNO_MAP(X)
#undef X
    return NULL;
}

/* The message names the errno by its symbol and its number, so a raw
 * platform value stays visible in every managed exception text. */
const char *SystemNative_StrErrorR(int32_t platformErrno, char *buffer, int32_t bufferSize)
{
    const char *name = errno_name(platformErrno);
    char tmp[48];
    uint32_t n = 0;
    const char *head = name != NULL ? name : "errno";
    while (*head != 0 && n < sizeof tmp - 16u)
        tmp[n++] = *head++;
    tmp[n++] = ' ';
    tmp[n++] = '(';
    uint32_t v = (uint32_t)(platformErrno < 0 ? -platformErrno : platformErrno);
    char digits[12];
    uint32_t nd = 0;
    do {
        digits[nd++] = (char)('0' + v % 10u);
        v /= 10u;
    } while (v != 0);
    if (platformErrno < 0)
        tmp[n++] = '-';
    while (nd != 0)
        tmp[n++] = digits[--nd];
    tmp[n++] = ')';
    tmp[n] = 0;
    if (buffer == NULL || bufferSize <= 0)
        return NULL;
    if ((uint32_t)bufferSize <= n) {
        memcpy(buffer, tmp, (size_t)bufferSize - 1u);
        buffer[bufferSize - 1] = 0;
        return NULL;
    }
    memcpy(buffer, tmp, n + 1u);
    return buffer;
}

/* ------------------------------------------------------------------ */
/* the descriptor bands                                                */
/* ------------------------------------------------------------------ */

#define RFS_FILES 12
#define RFS_DIRS 8
#define RFS_PATH 64
#define RFS_MODE_MASK 0x1FFu
/* The store's stated maximum file size. Every offset, count and length
 * is checked against it in 64-bit arithmetic before the store mutates:
 * no position wraps, the growth doubles up to it and no further, and no
 * gap clear runs past a buffer. A write or truncation beyond it is
 * EFBIG; a seek beyond it, or below zero, is EINVAL. */
#define RFS_MAX_FILE 1048576ll
#define RFS_LOCK_NONE 0u
#define RFS_LOCK_SH 1u
#define RFS_LOCK_EX 2u

/* An object exists from its creation to its last close once it has no
 * name: unlink and rename-over take the name away at once and leave the
 * bytes to the descriptors that hold them; a slot is reused only after
 * the last close. */
/* A file's time as the PAL reports it: whole seconds since
 * 1970-01-01T00:00:00Z and the nanoseconds within that second. A time the
 * store sets itself is the kernel's calendar at that moment; a time set
 * through the PAL is kept exactly as given. */
typedef struct rfs_time {
    int64_t sec;
    int32_t nsec;
} rfs_time;

/* The calendar now, as a file time. */
static rfs_time rfs_now(void)
{
    rfs_time t;
    rhu_shim_unix_split(rhu_shim_calendar_unix_ticks(), &t.sec, &t.nsec);
    return t;
}

typedef struct rfs_file {
    uint8_t used;
    uint8_t linked;
    uint8_t opens;
    uint8_t lock_ex;
    uint8_t lock_sh;
    char path[RFS_PATH];
    uint8_t *data;
    uint32_t size;
    uint32_t cap;
    uint32_t mode;
    rfs_time atime;
    rfs_time mtime;
} rfs_file;

typedef struct rfs_dir {
    uint8_t used;
    char path[RFS_PATH];
} rfs_dir;

typedef struct rfs_fd {
    uint8_t used;
    uint8_t file;
    uint8_t lock;
    int32_t access;
    uint32_t pos;
} rfs_fd;

static rfs_file g_files[RFS_FILES];
static rfs_dir g_dirs[RFS_DIRS];
static rfs_fd g_fds[RHU_SHIM_FD_FILE_COUNT];

/* One store, one lock: every verb that reads or writes the file,
 * directory or descriptor tables holds the shim's own mutex, allocation
 * included; the console paths and the waits inside Poll run outside it.
 * An all-zero slot is a valid unlocked mutex of the shim's family. */
static uint8_t g_store_mutex[24];

#ifdef RHU_SHIM_BREAK_STORE_LOCK
static void store_lock(void)
{
    (void)g_store_mutex;
}
static void store_unlock(void)
{
}
#else
static void store_lock(void)
{
    pthread_mutex_lock(g_store_mutex);
}
static void store_unlock(void)
{
    pthread_mutex_unlock(g_store_mutex);
}
#endif

static int fd_is_console(intptr_t fd)
{
    return fd >= RHU_SHIM_FD_CONSOLE_LO && fd < RHU_SHIM_FD_CONSOLE_HI;
}

static rfs_fd *fd_file(intptr_t fd)
{
    if (fd < RHU_SHIM_FD_FILE_LO || fd >= RHU_SHIM_FD_FILE_LO + RHU_SHIM_FD_FILE_COUNT)
        return NULL;
    rfs_fd *h = &g_fds[fd - RHU_SHIM_FD_FILE_LO];
    return h->used != 0 ? h : NULL;
}

/* A path as the store keys it: absolute as given, one trailing slash
 * dropped (the root keeps its one character). The copy lives in the
 * caller's buffer; a path too long for the store is left to the length
 * check of the verb that follows. */
static const char *path_norm(const char *path, char *buf)
{
#ifdef RHU_SHIM_BREAK_STORE_SLASH
    (void)buf;
    return path;
#else
    size_t n = strlen(path);
    if (n < 2 || path[n - 1] != '/' || n >= RFS_PATH)
        return path;
    memcpy(buf, path, n - 1);
    buf[n - 1] = 0;
    return buf;
#endif
}

/* The named objects only: an unlinked object has no name to find. */
static int file_find(const char *path)
{
    for (int i = 0; i < RFS_FILES; i++) {
        if (g_files[i].used != 0 && g_files[i].linked != 0 && strcmp(g_files[i].path, path) == 0)
            return i;
    }
    return -1;
}

/* The implicit roots exist without being made. */
static int dir_exists(const char *path)
{
    if (strcmp(path, "/") == 0 || strcmp(path, "/tmp") == 0)
        return 1;
    for (int i = 0; i < RFS_DIRS; i++) {
        if (g_dirs[i].used != 0 && strcmp(g_dirs[i].path, path) == 0)
            return 1;
    }
    return 0;
}

static int dir_find(const char *path)
{
    for (int i = 0; i < RFS_DIRS; i++) {
        if (g_dirs[i].used != 0 && strcmp(g_dirs[i].path, path) == 0)
            return i;
    }
    return -1;
}

static int parent_exists(const char *path)
{
    char parent[RFS_PATH];
    const char *s = strrchr(path, '/');
    if (s == NULL || s == path)
        return 1;
    if ((size_t)(s - path) >= sizeof parent)
        return 0;
    memcpy(parent, path, (size_t)(s - path));
    parent[s - path] = 0;
    return dir_exists(parent);
}

/* Whether `full` is a direct child of `dir`; the leaf name when it is. */
static const char *child_of(const char *full, const char *dir)
{
    size_t dl = strlen(dir);
    if (dl == 1 && dir[0] == '/')
        dl = 0;
    if (strncmp(full, dir, dl) != 0 || full[dl] != '/')
        return NULL;
    const char *leaf = full + dl + 1;
    return strchr(leaf, '/') != NULL ? NULL : leaf;
}

/* `need` is at most the maximum file size (the callers check), so the
 * doubling reaches it and stops. */
static int file_grow(rfs_file *f, uint32_t need)
{
    if (need <= f->cap)
        return 0;
    uint32_t cap = f->cap != 0 ? f->cap : 256u;
    while (cap < need)
        cap = cap < (uint32_t)RFS_MAX_FILE / 2u ? cap * 2u : (uint32_t)RFS_MAX_FILE;
    uint8_t *nb = (uint8_t *)realloc(f->data, cap);
    if (nb == NULL)
        return -1;
    f->data = nb;
    f->cap = cap;
    return 0;
}

static void file_release(rfs_file *f)
{
    free(f->data);
    memset(f, 0, sizeof *f);
}

/* The name goes; the object stays while a descriptor holds it. */
static void file_unlink(int fi)
{
    rfs_file *f = &g_files[fi];
#ifdef RHU_SHIM_BREAK_STORE_LIFETIME
    file_release(f);
#else
    if (f->opens > 0) {
        f->linked = 0;
        f->path[0] = 0;
    } else {
        file_release(f);
    }
#endif
}

/* The advisory lock a descriptor holds, given back. */
static void fd_unlock(rfs_fd *h, rfs_file *f)
{
    if (h->lock == RFS_LOCK_EX)
        f->lock_ex = 0;
    else if (h->lock == RFS_LOCK_SH && f->lock_sh > 0)
        f->lock_sh--;
    h->lock = RFS_LOCK_NONE;
}

/* A descriptor closes: its lock is given back, its object is released
 * at the last close of a nameless object. */
static void fd_close(rfs_fd *h)
{
    rfs_file *f = &g_files[h->file];
    fd_unlock(h, f);
    memset(h, 0, sizeof *h);
    if (f->opens > 0)
        f->opens--;
    if (f->opens == 0 && f->linked == 0)
        file_release(f);
}

static int32_t file_pread(rfs_fd *h, void *buf, int32_t count, int64_t at)
{
    rfs_file *f = &g_files[h->file];
#ifndef RHU_SHIM_BREAK_STORE_ACCESS
    if (h->access == RHU_PAL_O_WRONLY) {
        ERRNO = RHU_GERR_EBADF;
        return -1;
    }
#endif
    if (at < 0 || count < 0) {
        ERRNO = RHU_GERR_EINVAL;
        return -1;
    }
    if (at >= (int64_t)f->size || count == 0)
        return 0;
    uint32_t n = f->size - (uint32_t)at;
    if ((uint32_t)count < n)
        n = (uint32_t)count;
    memcpy(buf, f->data + (uint32_t)at, n);
    f->atime = rfs_now();
    return (int32_t)n;
}

static int32_t file_pwrite(rfs_fd *h, const void *buf, int32_t count, int64_t at)
{
    rfs_file *f = &g_files[h->file];
    if (h->access == RHU_PAL_O_RDONLY) {
        ERRNO = RHU_GERR_EBADF;
        return -1;
    }
    if (at < 0 || count < 0) {
        ERRNO = RHU_GERR_EINVAL;
        return -1;
    }
    if (at + (int64_t)count > RFS_MAX_FILE) {
        ERRNO = RHU_GERR_EFBIG;
        return -1;
    }
    if (count == 0)
        return 0;
    uint32_t end = (uint32_t)(at + (int64_t)count);
    if (file_grow(f, end) != 0) {
        ERRNO = RHU_GERR_ENOSPC;
        return -1;
    }
    if ((uint32_t)at > f->size)
        memset(f->data + f->size, 0, (uint32_t)at - f->size);
    memcpy(f->data + (uint32_t)at, buf, (uint32_t)count);
    if (end > f->size)
        f->size = end;
    f->mtime = rfs_now();
    return count;
}

/* A positioned transfer's offset arrives whole; the store as it stood
 * cut it to 32 bits, and the control restores that cut. */
static int64_t positioned(int64_t fileOffset)
{
#ifdef RHU_SHIM_BREAK_STORE_BOUNDS
    return (int64_t)(uint32_t)fileOffset;
#else
    return fileOffset;
#endif
}

/* ------------------------------------------------------------------ */
/* the file surface                                                     */
/* ------------------------------------------------------------------ */

/* The resources first -- a descriptor, and for a new object a slot --
 * so a refused open leaves the store as it was; the object is created or
 * truncated only once both are in hand. */
static intptr_t open_locked(const char *path, int32_t flags, int32_t mode)
{
    char nb[RFS_PATH];
    int32_t access = flags & RHU_PAL_O_ACCESS_MODE_MASK;
    if (path == NULL) {
        ERRNO = RHU_GERR_EFAULT;
        return -1;
    }
    path = path_norm(path, nb);
    if (dir_exists(path)) {
        ERRNO = RHU_GERR_EISDIR;
        return -1;
    }
    int fi = file_find(path);
    if (fi < 0) {
        if ((flags & RHU_PAL_O_CREAT) == 0) {
            ERRNO = RHU_GERR_ENOENT;
            return -1;
        }
        if (strlen(path) >= RFS_PATH) {
            ERRNO = RHU_GERR_ENAMETOOLONG;
            return -1;
        }
        if (!parent_exists(path)) {
            ERRNO = RHU_GERR_ENOENT;
            return -1;
        }
    } else if ((flags & RHU_PAL_O_EXCL) != 0) {
        ERRNO = RHU_GERR_EEXIST;
        return -1;
    }
    int di = -1;
    for (int i = 0; i < RHU_SHIM_FD_FILE_COUNT && di < 0; i++) {
        if (g_fds[i].used == 0)
            di = i;
    }
#ifndef RHU_SHIM_BREAK_STORE_OPEN_ORDER
    if (di < 0) {
        ERRNO = RHU_GERR_EMFILE;
        return -1;
    }
#endif
    int ni = fi;
    if (ni < 0) {
        for (int i = 0; i < RFS_FILES && ni < 0; i++) {
            if (g_files[i].used == 0)
                ni = i;
        }
        if (ni < 0) {
            ERRNO = RHU_GERR_ENOSPC;
            return -1;
        }
    }
    if (fi < 0) {
        memset(&g_files[ni], 0, sizeof g_files[ni]);
        g_files[ni].used = 1;
        g_files[ni].linked = 1;
        strcpy(g_files[ni].path, path);
        g_files[ni].mode = (uint32_t)mode & RFS_MODE_MASK;
        g_files[ni].atime = rfs_now();
        g_files[ni].mtime = g_files[ni].atime;
    } else if ((flags & RHU_PAL_O_TRUNC) != 0 && access != RHU_PAL_O_RDONLY) {
        g_files[ni].size = 0;
        g_files[ni].mtime = rfs_now();
    }
#ifdef RHU_SHIM_BREAK_STORE_OPEN_ORDER
    if (di < 0) {
        ERRNO = RHU_GERR_EMFILE;
        return -1;
    }
#endif
    g_fds[di].used = 1;
    g_fds[di].file = (uint8_t)ni;
    g_fds[di].lock = RFS_LOCK_NONE;
    g_fds[di].access = access;
    g_fds[di].pos = 0;
    g_files[ni].opens++;
    return RHU_SHIM_FD_FILE_LO + di;
}

intptr_t SystemNative_Open(const char *path, int32_t flags, int32_t mode)
{
    store_lock();
    intptr_t r = open_locked(path, flags, mode);
    store_unlock();
    return r;
}

int32_t SystemNative_Close(intptr_t fd)
{
    if (fd_is_console(fd))
        return 0;
    store_lock();
    rfs_fd *h = fd_file(fd);
    if (h == NULL) {
        store_unlock();
        ERRNO = RHU_GERR_EBADF;
        return -1;
    }
    fd_close(h);
    store_unlock();
    return 0;
}

/* A console descriptor has itself as its only name; a file descriptor has
 * no second name at this hosting. */
intptr_t SystemNative_Dup(intptr_t oldfd)
{
    if (fd_is_console(oldfd))
        return oldfd;
    store_lock();
    int is_file = fd_file(oldfd) != NULL;
    store_unlock();
    if (is_file) {
        rhu_shim_log("dup: a file descriptor has no second name at this hosting (ENOTSUP)");
        ERRNO = RHU_GERR_ENOTSUP;
        return -1;
    }
    ERRNO = RHU_GERR_EBADF;
    return -1;
}

int32_t SystemNative_Read(intptr_t fd, void *buffer, int32_t bufferSize)
{
    if (fd == 0)
        return rhu_console_read(buffer, (uint32_t)bufferSize);
    store_lock();
    rfs_fd *h = fd_file(fd);
    if (h == NULL) {
        store_unlock();
        ERRNO = RHU_GERR_EBADF;
        return -1;
    }
    int32_t n = file_pread(h, buffer, bufferSize, (int64_t)h->pos);
    if (n > 0)
        h->pos += (uint32_t)n;
    store_unlock();
    return n;
}

int32_t SystemNative_Write(intptr_t fd, const void *buffer, int32_t bufferSize)
{
    if (fd == 1 || fd == 2)
        return rhu_console_write(buffer, (uint32_t)bufferSize);
    store_lock();
    rfs_fd *h = fd_file(fd);
    if (h == NULL) {
        store_unlock();
        ERRNO = RHU_GERR_EBADF;
        return -1;
    }
    int32_t n = file_pwrite(h, buffer, bufferSize, (int64_t)h->pos);
    if (n > 0)
        h->pos += (uint32_t)n;
    store_unlock();
    return n;
}

int32_t SystemNative_PRead(intptr_t fd, void *buffer, int32_t bufferSize, int64_t fileOffset)
{
    store_lock();
    rfs_fd *h = fd_file(fd);
    if (h == NULL) {
        store_unlock();
        ERRNO = fd_is_console(fd) ? RHU_GERR_ESPIPE : RHU_GERR_EBADF;
        return -1;
    }
    int32_t n = file_pread(h, buffer, bufferSize, positioned(fileOffset));
    store_unlock();
    return n;
}

int32_t SystemNative_PWrite(intptr_t fd, void *buffer, int32_t bufferSize, int64_t fileOffset)
{
    store_lock();
    rfs_fd *h = fd_file(fd);
    if (h == NULL) {
        store_unlock();
        ERRNO = fd_is_console(fd) ? RHU_GERR_ESPIPE : RHU_GERR_EBADF;
        return -1;
    }
    int32_t n = file_pwrite(h, buffer, bufferSize, positioned(fileOffset));
    store_unlock();
    return n;
}

/* Vectored transfers run over the scalar paths: a short transfer ends the
 * gather, and a failure after some bytes moved reports the bytes moved. */
static int64_t vectored(intptr_t fd, rhu_pal_iovector *v, int32_t n, int64_t at, int positional, int writing)
{
    if (v == NULL || n < 0) {
        ERRNO = RHU_GERR_EFAULT;
        return -1;
    }
    int64_t total = 0;
    for (int32_t i = 0; i < n; i++) {
        if (v[i].Count == 0)
            continue;
        int32_t chunk = v[i].Count > 0x7FFFFFFFu ? 0x7FFFFFFF : (int32_t)v[i].Count;
        int32_t r;
        if (positional)
            r = writing ? SystemNative_PWrite(fd, v[i].Base, chunk, at + total)
                        : SystemNative_PRead(fd, v[i].Base, chunk, at + total);
        else
            r = writing ? SystemNative_Write(fd, v[i].Base, chunk) : SystemNative_Read(fd, v[i].Base, chunk);
        if (r < 0)
            return total > 0 ? total : -1;
        total += r;
        if (r < chunk)
            break;
    }
    return total;
}

int64_t SystemNative_ReadV(intptr_t fd, rhu_pal_iovector *vectors, int32_t vectorCount)
{
    return vectored(fd, vectors, vectorCount, 0, 0, 0);
}

int64_t SystemNative_WriteV(intptr_t fd, rhu_pal_iovector *vectors, int32_t vectorCount)
{
    return vectored(fd, vectors, vectorCount, 0, 0, 1);
}

int64_t SystemNative_PReadV(intptr_t fd, rhu_pal_iovector *vectors, int32_t vectorCount, int64_t fileOffset)
{
    return vectored(fd, vectors, vectorCount, fileOffset, 1, 0);
}

int64_t SystemNative_PWriteV(intptr_t fd, rhu_pal_iovector *vectors, int32_t vectorCount, int64_t fileOffset)
{
    return vectored(fd, vectors, vectorCount, fileOffset, 1, 1);
}

/* A position may sit past the end of the object (a later write zero-fills
 * the gap) but never past the maximum file size, and never below zero. */
int64_t SystemNative_LSeek(intptr_t fd, int64_t offset, int32_t whence)
{
    store_lock();
    rfs_fd *h = fd_file(fd);
    if (h == NULL) {
        store_unlock();
        ERRNO = fd_is_console(fd) ? RHU_GERR_ESPIPE : RHU_GERR_EBADF;
        return -1;
    }
    rfs_file *f = &g_files[h->file];
    int64_t base = whence == RHU_PAL_SEEK_SET ? 0
                 : whence == RHU_PAL_SEEK_CUR ? (int64_t)h->pos
                 : whence == RHU_PAL_SEEK_END ? (int64_t)f->size
                                              : -1;
    if (base < 0 || offset < -RFS_MAX_FILE || offset > RFS_MAX_FILE || base + offset < 0 || base + offset > RFS_MAX_FILE) {
        store_unlock();
        ERRNO = RHU_GERR_EINVAL;
        return -1;
    }
    h->pos = (uint32_t)(base + offset);
    int64_t r = (int64_t)h->pos;
    store_unlock();
    return r;
}

static void status_times(rhu_pal_filestatus *st, const rfs_time *atime, const rfs_time *mtime)
{
    st->ATime = atime->sec;
    st->ATimeNsec = atime->nsec;
    st->MTime = mtime->sec;
    st->MTimeNsec = mtime->nsec;
    st->CTime = st->MTime;
    st->CTimeNsec = st->MTimeNsec;
}

static void status_of_file(rhu_pal_filestatus *st, int index)
{
    const rfs_file *f = &g_files[index];
    memset(st, 0, sizeof *st);
    st->Mode = (int32_t)(RHU_PAL_S_IFREG | f->mode);
    st->Size = (int64_t)f->size;
    st->Ino = (int64_t)index + 1;
    st->Dev = 1;
    st->HardLinkCount = f->linked != 0 ? 1 : 0;
    status_times(st, &f->atime, &f->mtime);
}

static void status_of_dir(rhu_pal_filestatus *st)
{
    memset(st, 0, sizeof *st);
    st->Mode = (int32_t)(RHU_PAL_S_IFDIR | 0x1EDu);
    st->Dev = 1;
    st->HardLinkCount = 1;
}

/* The console answers only for the console band; a descriptor from any
 * other band is asked of its own owner, and a closed one is invalid. */
int32_t SystemNative_FStat(intptr_t fd, rhu_pal_filestatus *output)
{
    if (fd_is_console(fd)) {
        memset(output, 0, sizeof *output);
        output->Mode = (int32_t)(RHU_PAL_S_IFCHR | 0x1B6u);
        output->HardLinkCount = 1;
        return 0;
    }
    store_lock();
    rfs_fd *h = fd_file(fd);
    if (h == NULL) {
        store_unlock();
        memset(output, 0, sizeof *output);
        ERRNO = RHU_GERR_EBADF;
        return -1;
    }
    status_of_file(output, h->file);
    store_unlock();
    return 0;
}

int32_t SystemNative_Stat(const char *path, rhu_pal_filestatus *output)
{
    char nb[RFS_PATH];
    path = path_norm(path, nb);
    store_lock();
    int fi = file_find(path);
    if (fi >= 0) {
        status_of_file(output, fi);
        store_unlock();
        return 0;
    }
    int is_dir = dir_exists(path);
    store_unlock();
    if (is_dir) {
        status_of_dir(output);
        return 0;
    }
    memset(output, 0, sizeof *output);
    ERRNO = RHU_GERR_ENOENT;
    return -1;
}

int32_t SystemNative_LStat(const char *path, rhu_pal_filestatus *output)
{
    return SystemNative_Stat(path, output);
}

int32_t SystemNative_Unlink(const char *path)
{
    char nb[RFS_PATH];
    path = path_norm(path, nb);
    store_lock();
    int fi = file_find(path);
    if (fi < 0) {
        int is_dir = dir_exists(path);
        store_unlock();
        ERRNO = is_dir ? RHU_GERR_EISDIR : RHU_GERR_ENOENT;
        return -1;
    }
    file_unlink(fi);
    store_unlock();
    return 0;
}

static int32_t mkdir_locked(const char *path)
{
    if (dir_exists(path) || file_find(path) >= 0) {
        ERRNO = RHU_GERR_EEXIST;
        return -1;
    }
    if (strlen(path) >= RFS_PATH) {
        ERRNO = RHU_GERR_ENAMETOOLONG;
        return -1;
    }
    if (!parent_exists(path)) {
        ERRNO = RHU_GERR_ENOENT;
        return -1;
    }
    for (int i = 0; i < RFS_DIRS; i++) {
        if (g_dirs[i].used == 0) {
            g_dirs[i].used = 1;
            strcpy(g_dirs[i].path, path);
            return 0;
        }
    }
    ERRNO = RHU_GERR_ENOSPC;
    return -1;
}

int32_t SystemNative_MkDir(const char *path, int32_t mode)
{
    char nb[RFS_PATH];
    (void)mode;
    path = path_norm(path, nb);
    store_lock();
    int32_t r = mkdir_locked(path);
    store_unlock();
    return r;
}

static int dir_has_children(const char *path)
{
    for (int i = 0; i < RFS_FILES; i++) {
        if (g_files[i].used != 0 && g_files[i].linked != 0 && child_of(g_files[i].path, path) != NULL)
            return 1;
    }
    for (int i = 0; i < RFS_DIRS; i++) {
        if (g_dirs[i].used != 0 && child_of(g_dirs[i].path, path) != NULL)
            return 1;
    }
    return 0;
}

static int32_t rmdir_locked(const char *path)
{
    int di = dir_find(path);
    if (di < 0) {
        ERRNO = dir_exists(path) ? RHU_GERR_EBUSY : (file_find(path) >= 0 ? RHU_GERR_ENOTDIR : RHU_GERR_ENOENT);
        return -1;
    }
    if (dir_has_children(path)) {
        ERRNO = RHU_GERR_ENOTEMPTY;
        return -1;
    }
    g_dirs[di].used = 0;
    return 0;
}

int32_t SystemNative_RmDir(const char *path)
{
    char nb[RFS_PATH];
    path = path_norm(path, nb);
    store_lock();
    int32_t r = rmdir_locked(path);
    store_unlock();
    return r;
}

static int32_t rename_locked(const char *oldPath, const char *newPath)
{
    int fi = file_find(oldPath);
    if (fi < 0) {
        if (dir_exists(oldPath)) {
            rhu_shim_log("rename: directories keep their names at this hosting (ENOTSUP)");
            ERRNO = RHU_GERR_ENOTSUP;
        } else {
            ERRNO = RHU_GERR_ENOENT;
        }
        return -1;
    }
    if (strlen(newPath) >= RFS_PATH) {
        ERRNO = RHU_GERR_ENAMETOOLONG;
        return -1;
    }
    if (dir_exists(newPath)) {
        ERRNO = RHU_GERR_EISDIR;
        return -1;
    }
    if (!parent_exists(newPath)) {
        ERRNO = RHU_GERR_ENOENT;
        return -1;
    }
    int over = file_find(newPath);
    if (over >= 0 && over != fi)
        file_unlink(over);
    strcpy(g_files[fi].path, newPath);
    return 0;
}

int32_t SystemNative_Rename(const char *oldPath, const char *newPath)
{
    char ob[RFS_PATH];
    char nb[RFS_PATH];
    oldPath = path_norm(oldPath, ob);
    newPath = path_norm(newPath, nb);
    store_lock();
    int32_t r = rename_locked(oldPath, newPath);
    store_unlock();
    return r;
}

int32_t SystemNative_FSync(intptr_t fd)
{
    if (fd_is_console(fd))
        return 0;
    store_lock();
    int ok = fd_file(fd) != NULL;
    store_unlock();
    if (ok)
        return 0;
    ERRNO = RHU_GERR_EBADF;
    return -1;
}

/* Advisory locks per object, held per descriptor: exclusive excludes
 * every other holder, shared holders coexist. The store has no waiters,
 * so a conflicting request is refused with EWOULDBLOCK whether or not the
 * caller asked not to block -- an honest refusal in place of a wait that
 * nothing would end. A descriptor's lock goes back at unlock and at
 * close. */
static int32_t flock_locked(intptr_t fd, int32_t operation)
{
    if (fd_is_console(fd))
        return 0;
    rfs_fd *h = fd_file(fd);
    if (h == NULL) {
        ERRNO = RHU_GERR_EBADF;
        return -1;
    }
#ifdef RHU_SHIM_BREAK_STORE_FLOCK
    (void)operation;
    return 0;
#else
    rfs_file *f = &g_files[h->file];
    int32_t kind = operation & ~RHU_PAL_LOCK_NB;
    if (kind == RHU_PAL_LOCK_UN) {
        fd_unlock(h, f);
        return 0;
    }
    if (kind != RHU_PAL_LOCK_SH && kind != RHU_PAL_LOCK_EX) {
        ERRNO = RHU_GERR_EINVAL;
        return -1;
    }
    int other_ex = f->lock_ex != 0 && h->lock != RFS_LOCK_EX;
    int other_sh = (int)f->lock_sh - (h->lock == RFS_LOCK_SH ? 1 : 0);
    if (kind == RHU_PAL_LOCK_EX ? (other_ex || other_sh > 0) : other_ex) {
        ERRNO = RHU_GERR_EAGAIN;
        return -1;
    }
    fd_unlock(h, f);
    if (kind == RHU_PAL_LOCK_EX) {
        f->lock_ex = 1;
        h->lock = RFS_LOCK_EX;
    } else {
        f->lock_sh++;
        h->lock = RFS_LOCK_SH;
    }
    return 0;
#endif
}

int32_t SystemNative_FLock(intptr_t fd, int32_t operation)
{
    store_lock();
    int32_t r = flock_locked(fd, operation);
    store_unlock();
    return r;
}

/* Path canonicalization over the store, for the runtime's portability
 * layer (it resolves the executable's path through it). The store keys
 * every file and directory by its absolute path as given and holds no
 * symbolic links and no relative segments, so a path that exists is
 * already canonical, and a path that does not is ENOENT. A null buffer
 * asks for a heap copy the caller frees, as the C library's contract
 * says; a caller's buffer is taken to hold the path (the store's paths
 * are short, and the contract's minimum buffer is far longer). */
char *realpath(const char *path, char *resolved)
{
    char nb[RFS_PATH];
    if (path == NULL) {
        ERRNO = RHU_GERR_EINVAL;
        return NULL;
    }
    path = path_norm(path, nb);
    store_lock();
    int present = file_find(path) >= 0 || dir_exists(path);
    store_unlock();
    if (!present) {
        ERRNO = RHU_GERR_ENOENT;
        return NULL;
    }
    size_t n = strlen(path) + 1;
    if (resolved == NULL) {
        resolved = (char *)malloc(n);
        if (resolved == NULL) {
            ERRNO = RHU_GERR_ENOMEM;
            return NULL;
        }
    }
    memcpy(resolved, path, n);
    return resolved;
}

int32_t SystemNative_Access(const char *path, int32_t mode)
{
    char nb[RFS_PATH];
    (void)mode;
    path = path_norm(path, nb);
    store_lock();
    int present = file_find(path) >= 0 || dir_exists(path);
    store_unlock();
    if (present)
        return 0;
    ERRNO = RHU_GERR_ENOENT;
    return -1;
}

static int32_t ftruncate_locked(intptr_t fd, int64_t length)
{
    rfs_fd *h = fd_file(fd);
    if (h == NULL) {
        ERRNO = fd_is_console(fd) ? RHU_GERR_EINVAL : RHU_GERR_EBADF;
        return -1;
    }
    if (h->access == RHU_PAL_O_RDONLY) {
        ERRNO = RHU_GERR_EBADF;
        return -1;
    }
    if (length < 0) {
        ERRNO = RHU_GERR_EINVAL;
        return -1;
    }
    if (length > RFS_MAX_FILE) {
        ERRNO = RHU_GERR_EFBIG;
        return -1;
    }
    rfs_file *f = &g_files[h->file];
    uint32_t len = (uint32_t)length;
    if (len > f->size) {
        if (file_grow(f, len) != 0) {
            ERRNO = RHU_GERR_ENOSPC;
            return -1;
        }
        memset(f->data + f->size, 0, len - f->size);
    }
    f->size = len;
    f->mtime = rfs_now();
    return 0;
}

int32_t SystemNative_FTruncate(intptr_t fd, int64_t length)
{
    store_lock();
    int32_t r = ftruncate_locked(fd, length);
    store_unlock();
    return r;
}

typedef struct rfs_iter {
    char dir[RFS_PATH];
    int idx;
    char name[RFS_PATH];
} rfs_iter;

void *SystemNative_OpenDir(const char *path)
{
    char nb[RFS_PATH];
    path = path_norm(path, nb);
    store_lock();
    if (!dir_exists(path)) {
        int is_file = file_find(path) >= 0;
        store_unlock();
        ERRNO = is_file ? RHU_GERR_ENOTDIR : RHU_GERR_ENOENT;
        return NULL;
    }
    store_unlock();
    if (strlen(path) >= RFS_PATH) {
        ERRNO = RHU_GERR_ENAMETOOLONG;
        return NULL;
    }
    rfs_iter *it = (rfs_iter *)malloc(sizeof *it);
    if (it == NULL) {
        ERRNO = RHU_GERR_ENOMEM;
        return NULL;
    }
    memset(it, 0, sizeof *it);
    strcpy(it->dir, path);
    return it;
}

int32_t SystemNative_CloseDir(void *dir)
{
    free(dir);
    return 0;
}

/* The entry's name lives in the iterator until the next call; the index
 * walks the file table (the named objects), then the directory table.
 * Returns 0 with an entry, -1 at the end of the stream, and an errno
 * value on a broken iterator. */
int32_t SystemNative_ReadDir(void *dir, rhu_pal_directoryentry *outputEntry)
{
    rfs_iter *it = (rfs_iter *)dir;
    if (it == NULL) {
        memset(outputEntry, 0, sizeof *outputEntry);
        return RHU_GERR_EBADF;
    }
    store_lock();
    while (it->idx < RFS_FILES + RFS_DIRS) {
        int i = it->idx++;
        const char *full = NULL;
        int32_t type = RHU_PAL_DT_REG;
        if (i < RFS_FILES) {
            if (g_files[i].used != 0 && g_files[i].linked != 0)
                full = g_files[i].path;
        } else if (g_dirs[i - RFS_FILES].used != 0) {
            full = g_dirs[i - RFS_FILES].path;
            type = RHU_PAL_DT_DIR;
        }
        if (full == NULL)
            continue;
        const char *leaf = child_of(full, it->dir);
        if (leaf == NULL)
            continue;
        strcpy(it->name, leaf);
        store_unlock();
        outputEntry->Name = it->name;
        outputEntry->NameLength = -1;
        outputEntry->InodeType = type;
        return 0;
    }
    store_unlock();
    memset(outputEntry, 0, sizeof *outputEntry);
    return -1;
}

int32_t SystemNative_Pipe(int32_t pipefd[2], int32_t flags)
{
    (void)pipefd;
    (void)flags;
    rhu_shim_log("pipe: not available (ENOSYS)");
    ERRNO = RHU_GERR_ENOSYS;
    return -1;
}

/* The console output descriptors and every file are always ready; the
 * console input is never reported ready (nothing waits on it); a
 * descriptor from no band is invalid. Waiting is done in short sleeps up
 * to the deadline, outside the store lock. */
int32_t SystemNative_Poll(rhu_pal_pollevent *pollEvents, uint32_t eventCount, int32_t milliseconds, uint32_t *triggered)
{
    if (pollEvents == NULL || triggered == NULL)
        return RHU_PAL_EFAULT;
    uint64_t deadline = milliseconds < 0 ? 0u : rhu_now_ns() + (uint64_t)milliseconds * 1000000ull;
    for (;;) {
        uint32_t n = 0;
        store_lock();
        for (uint32_t i = 0; i < eventCount; i++) {
            rhu_pal_pollevent *e = &pollEvents[i];
            int32_t fd = e->FileDescriptor;
            int16_t want = e->Events;
            e->TriggeredEvents = 0;
            if (fd == 1 || fd == 2) {
                if ((want & RHU_PAL_POLLOUT) != 0)
                    e->TriggeredEvents = RHU_PAL_POLLOUT;
            } else if (fd == 0) {
                e->TriggeredEvents = 0;
            } else if (fd_file(fd) != NULL) {
                e->TriggeredEvents = (int16_t)(want & (RHU_PAL_POLLIN | RHU_PAL_POLLOUT));
            } else {
                e->TriggeredEvents = RHU_PAL_POLLNVAL;
            }
            if (e->TriggeredEvents != 0)
                n++;
        }
        store_unlock();
        uint64_t now = rhu_now_ns();
        if (n != 0 || milliseconds == 0 || (deadline != 0 && now >= deadline)) {
            *triggered = n;
            return RHU_PAL_SUCCESS;
        }
        uint64_t left = deadline != 0 ? deadline - now : 10000000ull;
        rhu_sleep_ns(left > 10000000ull ? 10000000ull : left);
    }
}

int32_t SystemNative_ChMod(const char *path, int32_t mode)
{
    char nb[RFS_PATH];
    path = path_norm(path, nb);
    store_lock();
    int fi = file_find(path);
    if (fi >= 0) {
        g_files[fi].mode = (uint32_t)mode & RFS_MODE_MASK;
        store_unlock();
        return 0;
    }
    int is_dir = dir_exists(path);
    store_unlock();
    if (is_dir)
        return 0;
    ERRNO = RHU_GERR_ENOENT;
    return -1;
}

int32_t SystemNative_FChMod(intptr_t fd, int32_t mode)
{
    if (fd_is_console(fd))
        return 0;
    store_lock();
    rfs_fd *h = fd_file(fd);
    if (h == NULL) {
        store_unlock();
        ERRNO = RHU_GERR_EBADF;
        return -1;
    }
    g_files[h->file].mode = (uint32_t)mode & RFS_MODE_MASK;
    store_unlock();
    return 0;
}

/* The times a file carries: the pair is access then modification; a null
 * pair means now; a nanosecond field outside its range is refused before
 * either time moves. Directories carry no times. */
static int32_t set_times(int fi, const rhu_pal_timespec *times)
{
    if (times == NULL) {
        g_files[fi].atime = rfs_now();
        g_files[fi].mtime = g_files[fi].atime;
        return 0;
    }
#ifndef RHU_SHIM_BREAK_STORE_TIMES
    for (int i = 0; i < 2; i++) {
        if (times[i].tv_nsec < 0 || times[i].tv_nsec >= (int64_t)NS_PER_S) {
            ERRNO = RHU_GERR_EINVAL;
            return -1;
        }
    }
#endif
    g_files[fi].atime.sec = times[0].tv_sec;
    g_files[fi].atime.nsec = (int32_t)times[0].tv_nsec;
    g_files[fi].mtime.sec = times[1].tv_sec;
    g_files[fi].mtime.nsec = (int32_t)times[1].tv_nsec;
    return 0;
}

int32_t SystemNative_UTimensat(const char *path, rhu_pal_timespec *times)
{
    char nb[RFS_PATH];
    path = path_norm(path, nb);
    store_lock();
    int fi = file_find(path);
    if (fi >= 0) {
        int32_t r = set_times(fi, times);
        store_unlock();
        return r;
    }
    int is_dir = dir_exists(path);
    store_unlock();
    if (is_dir)
        return 0;
    ERRNO = RHU_GERR_ENOENT;
    return -1;
}

int32_t SystemNative_FUTimens(intptr_t fd, rhu_pal_timespec *times)
{
    store_lock();
    rfs_fd *h = fd_file(fd);
    if (h == NULL) {
        store_unlock();
        ERRNO = RHU_GERR_EBADF;
        return -1;
    }
    int32_t r = set_times(h->file, times);
    store_unlock();
    return r;
}

int32_t SystemNative_IsMemfdSupported(void)
{
    return 0;
}

intptr_t SystemNative_MemfdCreate(const char *name, int32_t isReadonly)
{
    (void)name;
    (void)isReadonly;
    ERRNO = RHU_GERR_ENOTSUP;
    return -1;
}

intptr_t SystemNative_ShmOpen(const char *name, int32_t flags, int32_t mode)
{
    (void)name;
    (void)flags;
    (void)mode;
    ERRNO = RHU_GERR_ENOTSUP;
    return -1;
}

int32_t SystemNative_ShmUnlink(const char *name)
{
    (void)name;
    ERRNO = RHU_GERR_ENOENT;
    return -1;
}

/* ------------------------------------------------------------------ */
/* memory mapping: the arena, and the frozen-object reservation guard   */
/* ------------------------------------------------------------------ */

/* The runtime reserves a large frozen-object segment with no access and
 * then commits it in fixed chunks; the constants are baked into the
 * runtime and a failed commit surfaces as a managed allocation failure.
 * The reservation is served from the board row's pool: at the asked
 * size when the ask fits the pool, otherwise the ask is recorded over the
 * pool and every commit beyond the pool is refused -- the guard is what
 * keeps the runtime out of arena memory it never received. The pool is
 * drawn once and never returned. */
#define FOH_SITE 0x464F4821u
#define GLIBC_MAP_SHARED 0x01
#define GLIBC_MAP_PRIVATE 0x02
#define GLIBC_MAP_ANONYMOUS 0x20

static uint8_t *g_foh_base;
static uint32_t g_foh_claimed;
static uint32_t g_foh_pool;
static uint32_t g_foh_committed;
static uint32_t g_foh_release_notes;

void *SystemNative_MMap(void *address, uint64_t length, int32_t protection, int32_t flags, intptr_t fd, int64_t offset)
{
    if (address == NULL && protection == RHU_PAL_PROT_NONE && fd == -1 && (flags & RHU_PAL_MAP_ANONYMOUS) != 0) {
        if (g_foh_base != NULL) {
            rhu_shim_log("frozen-object reserve %u KB: the pool is already claimed -> NULL (managed allocation failure)",
                         (unsigned)(length / 1024u));
            return NULL;
        }
        uint32_t pool = length <= (uint64_t)MGC_ROW_FOH_POOL_BYTES ? (uint32_t)length : (uint32_t)MGC_ROW_FOH_POOL_BYTES;
        pool = (pool + PAGE_BYTES - 1u) & ~(PAGE_BYTES - 1u);
        uint8_t *base = (uint8_t *)rhu_arena_alloc_at(pool, PAGE_BYTES, FOH_SITE);
        if (base == NULL)
            return NULL;
        g_foh_base = base;
        g_foh_pool = pool;
        g_foh_claimed = (uint32_t)length;
        g_foh_committed = 0;
        rhu_shim_log("frozen-object reserve %u KB -> pool %u KB (commit guard armed%s)",
                     (unsigned)(length / 1024u), (unsigned)(pool / 1024u),
                     pool == (uint32_t)length ? ", served at size" : "");
        return base;
    }
    int native_flags = 0;
    if ((flags & RHU_PAL_MAP_SHARED) != 0)
        native_flags |= GLIBC_MAP_SHARED;
    if ((flags & RHU_PAL_MAP_PRIVATE) != 0)
        native_flags |= GLIBC_MAP_PRIVATE;
    if ((flags & RHU_PAL_MAP_ANONYMOUS) != 0)
        native_flags |= GLIBC_MAP_ANONYMOUS;
    void *p = mmap64(address, (size_t)length, protection, native_flags, (int)fd, offset);
    return p == (void *)-1 ? NULL : p;
}

int32_t SystemNative_MUnmap(void *address, uint64_t length)
{
    (void)length;
    if (g_foh_base != NULL && address == g_foh_base && g_foh_release_notes++ == 0)
        rhu_shim_log("frozen-object pool release ignored (the pool is permanent)");
    return 0;
}

int32_t SystemNative_MProtect(void *address, uint64_t length, int32_t protection)
{
    (void)protection;
    uint8_t *a = (uint8_t *)address;
    if (g_foh_base != NULL && a >= g_foh_base && a < g_foh_base + g_foh_claimed) {
        if ((uint64_t)(uintptr_t)(a - g_foh_base) + length <= (uint64_t)g_foh_pool) {
            uint32_t end = (uint32_t)((a - g_foh_base) + (uint32_t)length);
            if (end > g_foh_committed)
                g_foh_committed = end;
            return 0;
        }
        rhu_shim_log("frozen-object commit refused beyond the pool (managed allocation failure, never a wild write)");
        ERRNO = RHU_GERR_ENOMEM;
        return -1;
    }
    return 0;
}

int32_t SystemNative_MAdvise(void *address, uint64_t length, int32_t advice)
{
    (void)address;
    (void)length;
    (void)advice;
    return 0;
}

int32_t SystemNative_MSync(void *address, uint64_t length, int32_t flags)
{
    (void)address;
    (void)length;
    if ((flags & (RHU_PAL_MS_ASYNC | RHU_PAL_MS_SYNC)) == (RHU_PAL_MS_ASYNC | RHU_PAL_MS_SYNC)) {
        ERRNO = RHU_GERR_EINVAL;
        return -1;
    }
    return 0;
}

int64_t SystemNative_SysConf(int32_t name)
{
    switch (name) {
    case RHU_PAL_SC_CLK_TCK:
        return 100;
    case RHU_PAL_SC_PAGESIZE:
        return (int64_t)PAGE_BYTES;
    default:
        ERRNO = RHU_GERR_EINVAL;
        return -1;
    }
}

/* ------------------------------------------------------------------ */
/* the malloc family                                                    */
/* ------------------------------------------------------------------ */

void *SystemNative_Malloc(uintptr_t size)
{
    return malloc(size);
}

void SystemNative_Free(void *ptr)
{
    free(ptr);
}

void *SystemNative_Calloc(uintptr_t num, uintptr_t size)
{
    uintptr_t total = num * size;
    if (size != 0 && total / size != num)
        return NULL;
    void *p = malloc(total);
    if (p != NULL)
        memset(p, 0, total);
    return p;
}

void *SystemNative_Realloc(void *ptr, uintptr_t new_size)
{
    return realloc(ptr, new_size);
}

void *SystemNative_AlignedAlloc(uintptr_t alignment, uintptr_t size)
{
    return memalign(alignment, size);
}

void SystemNative_AlignedFree(void *ptr)
{
    free(ptr);
}

void *SystemNative_AlignedRealloc(void *ptr, uintptr_t alignment, uintptr_t size)
{
    void *q = memalign(alignment, size);
    if (q == NULL)
        return NULL;
    if (ptr != NULL) {
        size_t old = malloc_usable_size(ptr);
        memcpy(q, ptr, old < size ? old : size);
        free(ptr);
    }
    return q;
}

/* ------------------------------------------------------------------ */
/* threading: monitors and futexes on the kernel's one blocking verb    */
/* ------------------------------------------------------------------ */

/* A monitor is a mutex and a sequence word: a waiter captures the word
 * under the mutex, releases the mutex, and waits for the word to move;
 * a signal moves the word and wakes one waiter. A timed wait answers
 * false only on a real timeout. */
typedef struct monitor {
    uint8_t mutex[24];
    volatile uint32_t seq;
} monitor;

void *SystemNative_LowLevelMonitor_Create(void)
{
    monitor *m = (monitor *)malloc(sizeof *m);
    if (m == NULL)
        return NULL;
    memset(m, 0, sizeof *m);
    pthread_mutex_init(m->mutex, NULL);
    return m;
}

void SystemNative_LowLevelMonitor_Destroy(void *monitor_)
{
    free(monitor_);
}

void SystemNative_LowLevelMonitor_Acquire(void *monitor_)
{
    pthread_mutex_lock(((monitor *)monitor_)->mutex);
}

void SystemNative_LowLevelMonitor_Release(void *monitor_)
{
    pthread_mutex_unlock(((monitor *)monitor_)->mutex);
}

void SystemNative_LowLevelMonitor_Wait(void *monitor_)
{
    monitor *m = (monitor *)monitor_;
    uint32_t s = __atomic_load_n(&m->seq, __ATOMIC_ACQUIRE);
    pthread_mutex_unlock(m->mutex);
    rhu_futex_wait((const uint32_t *)&m->seq, s, RHU_DEADLINE_NONE);
    pthread_mutex_lock(m->mutex);
}

int32_t SystemNative_LowLevelMonitor_TimedWait(void *monitor_, int32_t timeoutMilliseconds)
{
    monitor *m = (monitor *)monitor_;
    uint32_t s = __atomic_load_n(&m->seq, __ATOMIC_ACQUIRE);
    uint64_t deadline = timeoutMilliseconds < 0 ? RHU_DEADLINE_NONE
                                                : rhu_now_ns() + (uint64_t)timeoutMilliseconds * 1000000ull;
    pthread_mutex_unlock(m->mutex);
    int32_t rc = rhu_futex_wait((const uint32_t *)&m->seq, s, deadline);
    pthread_mutex_lock(m->mutex);
    return rc != RHU_ETIMEDOUT;
}

void SystemNative_LowLevelMonitor_Signal_Release(void *monitor_)
{
    monitor *m = (monitor *)monitor_;
    __atomic_fetch_add(&m->seq, 1u, __ATOMIC_RELEASE);
    rhu_futex_wake((const uint32_t *)&m->seq, 1u);
    pthread_mutex_unlock(m->mutex);
}

/* The futex verbs map one to one onto the kernel's: a wake, a spurious
 * wake or an activation returns to a caller that rechecks its own
 * predicate; the timed form answers false only on a real timeout. */
void SystemNative_LowLevelFutex_WaitOnAddress(int32_t *address, int32_t comparand)
{
    rhu_futex_wait((const uint32_t *)address, (uint32_t)comparand, RHU_DEADLINE_NONE);
}

int32_t SystemNative_LowLevelFutex_WaitOnAddressTimeout(int32_t *address, int32_t comparand, int32_t timeoutMilliseconds)
{
    uint64_t deadline = timeoutMilliseconds < 0 ? RHU_DEADLINE_NONE
                                                : rhu_now_ns() + (uint64_t)timeoutMilliseconds * 1000000ull;
    int32_t rc = rhu_futex_wait((const uint32_t *)address, (uint32_t)comparand, deadline);
    return rc != RHU_ETIMEDOUT;
}

void SystemNative_LowLevelFutex_WakeByAddressSingle(int32_t *address)
{
    rhu_futex_wake((const uint32_t *)address, 1u);
}

int32_t SystemNative_CreateThread(uintptr_t stackSize, void *(*startAddress)(void *), void *parameter)
{
    return rhu_shim_create_thread((size_t)stackSize, startAddress, parameter) == 0;
}

int32_t SystemNative_SchedGetCpu(void)
{
    return 0;
}

void SystemNative_Exit(int32_t exitCode)
{
    rhu_shim_exit(exitCode);
}

/* A failure that ends the process lands on the thread that raised it. */
void SystemNative_Abort(void)
{
    rhu_shim_log("Abort() on thread %d", rhu_thread_self());
    rhu_exit(134);
    __builtin_unreachable();
}

uint64_t SystemNative_GetUInt64OSThreadId(void)
{
    return (uint64_t)(uint32_t)rhu_thread_self();
}

uint32_t SystemNative_TryGetUInt32OSThreadId(void)
{
    return (uint32_t)rhu_thread_self();
}

/* ------------------------------------------------------------------ */
/* time                                                                 */
/* ------------------------------------------------------------------ */

int64_t SystemNative_GetTimestamp(void)
{
    return (int64_t)rhu_now_ns();
}

int64_t SystemNative_GetLowResolutionTimestamp(void)
{
    return (int64_t)(rhu_now_ns() / 1000000ull);
}

/* The system time in 100 ns ticks since 1970-01-01T00:00:00Z, read from
 * the kernel's calendar (the runtime adds its own epoch offset to make a
 * calendar value): negative while the calendar is unset, which the
 * runtime reads as the epoch plus the elapsed clock. */
int64_t SystemNative_GetSystemTimeAsTicks(void)
{
#ifdef RHU_SHIM_BREAK_CALENDAR_CLOCK
    /* A test knob: the boot clock in place of the calendar, the answer of a
     * shim that has no calendar. Never set in a hosted image. */
    return (int64_t)(rhu_now_ns() / 100ull);
#else
    return rhu_shim_calendar_unix_ticks();
#endif
}

double SystemNative_GetCpuUtilization(rhu_pal_processcpuinformation *previousCpuInfo)
{
    previousCpuInfo->lastRecordedCurrentTime = rhu_now_ns();
    previousCpuInfo->lastRecordedKernelTime = 0;
    previousCpuInfo->lastRecordedUserTime = 0;
    return 0.0;
}

/* ------------------------------------------------------------------ */
/* random bytes: the kernel's two doors; the door's answer reported     */
/* ------------------------------------------------------------------ */

/* The non-cryptographic draw: the kernel's software stream. */
void SystemNative_GetNonCryptographicallySecureRandomBytes(uint8_t *buffer, int32_t bufferLength)
{
    rhu_entropy(buffer, (uint32_t)bufferLength);
}

/* The secure door's refusal, in words, for the line the first refusal
 * prints. */
static const char *entropy_refusal_text(int32_t code)
{
    switch (code) {
    case RHU_EINVAL:
        return "an invalid argument";
    case RHU_ENODEV:
        return "no hardware source serves";
    case RHU_EIO:
        return "the source failed its health check and is retired";
    case RHU_EAGAIN:
        return "the bytes were not ready in time";
    default:
        return "a code the door does not name";
    }
}

/* The cryptographic draw: the kernel's secure door, asked in pieces of at
 * most its bound, all or nothing. A refusal by the door fails the whole
 * draw -- the bytes already served are cleared, so the buffer never
 * carries a partial draw -- and the runtime surfaces the failure to its
 * caller; the first refusal is announced with the door's reason. A
 * negative length is refused before the door is asked; a zero length is
 * served trivially. */
int32_t SystemNative_GetCryptographicallySecureRandomBytes(uint8_t *buffer, int32_t bufferLength)
{
    static uint32_t served_announced;
    static uint32_t refusal_announced;
    if (bufferLength < 0)
        return -1;
    uint32_t n = (uint32_t)bufferLength;
    uint32_t off = 0;
    while (off < n) {
        uint32_t piece = n - off;
        if (piece > RHU_ENTROPY_SECURE_MAX)
            piece = RHU_ENTROPY_SECURE_MAX;
        int32_t r = rhu_entropy_secure(buffer == NULL ? NULL : buffer + off, piece);
#ifdef RHU_SHIM_BREAK_ENTROPY_REFUSAL
        /* A test knob: the door's answer ignored, every draw reported as
         * served. Never set in a hosted image. */
        r = 0;
#endif
        if (r != 0) {
            if (buffer != NULL && off > 0)
                memset(buffer, 0, off);
            if (refusal_announced++ == 0)
                rhu_shim_log("entropy: the secure door refused a cryptographic draw (%s); the draw fails and nothing is served (further refusals counted silently)", entropy_refusal_text(r));
            return -1;
        }
        off += piece;
    }
    if (n != 0 && served_announced++ == 0)
        rhu_shim_log("entropy: cryptographic draws through the kernel's secure door; hardware source: %s", rhu_entropy_is_hardware() != 0 ? "yes" : "no");
    return 0;
}

/* ------------------------------------------------------------------ */
/* signals: registrations recorded, none deliverable                   */
/* ------------------------------------------------------------------ */

/* No signal source exists on this platform: no terminal, no child
 * processes, no other process. Initialization therefore has nothing to
 * install and reports success once with the fact stated; the callbacks and
 * the per-signal enablement are recorded, and nothing consults them
 * because no signal arrives; the posix-signal identities map onto the
 * platform's numbers; and the default action for a terminating signal the
 * runtime asks to carry out is carried out. */
static rhu_pal_posixsignal_handler g_posix_handler;
static rhu_pal_sigchld_callback g_sigchld_callback;
static rhu_pal_terminal_invalidation_callback g_terminal_callback;
static void (*g_sigchld_console_callback)(void);
static uint8_t g_posix_enabled[RHU_GLIBC_NSIG];
static uint32_t g_signals_announced;

int32_t SystemNative_InitializeTerminalAndSignalHandling(void)
{
    if (g_signals_announced++ == 0)
        rhu_shim_log("signals: no signal source on this platform; registrations are recorded, none can arrive");
    return 1;
}

void SystemNative_SetTerminalInvalidationHandler(rhu_pal_terminal_invalidation_callback callback)
{
    g_terminal_callback = callback;
}

void SystemNative_RegisterForSigChld(rhu_pal_sigchld_callback callback)
{
    g_sigchld_callback = callback;
}

void SystemNative_SetDelayedSigChildConsoleConfigurationHandler(void (*callback)(void))
{
    g_sigchld_console_callback = callback;
}

void SystemNative_SetPosixSignalHandler(rhu_pal_posixsignal_handler signalHandler)
{
    g_posix_handler = signalHandler;
}

int32_t SystemNative_GetPlatformSignalNumber(rhu_pal_posixsignal signal)
{
    switch (signal) {
    case RHU_PosixSignalSIGHUP:
        return RHU_GSIG_SIGHUP;
    case RHU_PosixSignalSIGINT:
        return RHU_GSIG_SIGINT;
    case RHU_PosixSignalSIGQUIT:
        return RHU_GSIG_SIGQUIT;
    case RHU_PosixSignalSIGTERM:
        return RHU_GSIG_SIGTERM;
    case RHU_PosixSignalSIGCHLD:
        return RHU_GSIG_SIGCHLD;
    case RHU_PosixSignalSIGCONT:
        return RHU_GSIG_SIGCONT;
    case RHU_PosixSignalSIGWINCH:
        return RHU_GSIG_SIGWINCH;
    case RHU_PosixSignalSIGTTIN:
        return RHU_GSIG_SIGTTIN;
    case RHU_PosixSignalSIGTTOU:
        return RHU_GSIG_SIGTTOU;
    case RHU_PosixSignalSIGTSTP:
        return RHU_GSIG_SIGTSTP;
    case RHU_PosixSignalSIGKILL:
        return RHU_GSIG_SIGKILL;
    default:
        break;
    }
    return (signal > 0 && signal < RHU_GLIBC_NSIG) ? signal : 0;
}

int32_t SystemNative_GetPlatformSIGSTOP(void)
{
    return RHU_GSIG_SIGSTOP;
}

int32_t SystemNative_EnablePosixSignalHandling(int signalCode)
{
    if (signalCode < 1 || signalCode >= RHU_GLIBC_NSIG)
        return 0;
    g_posix_enabled[signalCode] = 1;
    return 1;
}

void SystemNative_DisablePosixSignalHandling(int signalCode)
{
    if (signalCode >= 1 && signalCode < RHU_GLIBC_NSIG)
        g_posix_enabled[signalCode] = 0;
}

/* The default action of a signal the managed handlers did not cancel:
 * continue, stop and ignore have nothing to do here; everything else
 * terminates, with the conventional code. */
void SystemNative_HandleNonCanceledPosixSignal(int32_t signalCode)
{
    switch (signalCode) {
    case RHU_GSIG_SIGCONT:
    case RHU_GSIG_SIGTSTP:
    case RHU_GSIG_SIGTTIN:
    case RHU_GSIG_SIGTTOU:
    case RHU_GSIG_SIGCHLD:
    case RHU_GSIG_SIGURG:
    case RHU_GSIG_SIGWINCH:
        return;
    default:
        rhu_shim_log("signal %d: default action is termination; exiting %d", signalCode, 128 + signalCode);
        rhu_shim_exit(128 + signalCode);
    }
}


/* ------------------------------------------------------------------ */
/* the console PAL: this platform's console is the kernel's byte pair  */
/* ------------------------------------------------------------------ */

/* There is no terminal here: one console, always attached, no geometry,
 * no line discipline, no modes to save or restore. Every row that would
 * configure a terminal succeeds by having nothing to do, and every row
 * that would report terminal state answers with the truth about this
 * platform rather than a plausible-looking number. The break mode is
 * recorded because the caller reads back what it set. */
static int32_t g_signal_for_break = 1;

int32_t SystemNative_IsATty(intptr_t fd)
{
    if (fd >= RHU_SHIM_FD_CONSOLE_LO && fd < RHU_SHIM_FD_CONSOLE_HI)
        return 1;
    ERRNO = RHU_GERR_ENOTTY;
    return 0;
}

/* No window: the caller must not read a size it could act on. */
int32_t SystemNative_GetWindowSize(intptr_t fd, rhu_pal_winsize *windowSize)
{
    (void)fd;
    if (windowSize != NULL) {
        windowSize->Row = 0;
        windowSize->Col = 0;
        windowSize->XPixel = 0;
        windowSize->YPixel = 0;
    }
    ERRNO = RHU_GERR_ENOTSUP;
    return -1;
}

int32_t SystemNative_ReadStdin(void *buffer, int32_t bufferSize)
{
    int32_t got;

    if (bufferSize < 0 || (buffer == NULL && bufferSize > 0)) {
        ERRNO = RHU_GERR_EINVAL;
        return -1;
    }
    if (bufferSize == 0)
        return 0;
    got = rhu_console_read(buffer, (uint32_t)bufferSize);
    if (got < 0) {
        ERRNO = RHU_GERR_EINVAL;
        return -1;
    }
    return got;
}

/* The kernel's read never waits, so readiness is what a read would have
 * returned -- and this platform has no way to look without taking. A
 * caller polling for input is told that none is waiting rather than
 * given a promise the next read cannot keep. */
int32_t SystemNative_StdinReady(void)
{
    return 0;
}

/* The control characters a terminal would disable: every one of them is
 * unavailable here, so each answers with the caller's own disable value. */
void SystemNative_GetControlCharacters(int32_t *controlCharacterNames, uint8_t *controlCharacterValues,
                                       int32_t controlCharacterLength, uint8_t *posixDisableValue)
{
    int32_t i;
    uint8_t disable = 0;

    if (posixDisableValue != NULL) {
        *posixDisableValue = 0;
        disable = 0;
    }
    if (controlCharacterValues == NULL || controlCharacterLength <= 0)
        return;
    (void)controlCharacterNames;
    for (i = 0; i < controlCharacterLength; i++)
        controlCharacterValues[i] = disable;
}

/* Nothing to transmit, nothing to configure, nothing to restore. */
void SystemNative_SetKeypadXmit(intptr_t fd, const char *terminfoString)
{
    (void)fd;
    (void)terminfoString;
}

void SystemNative_InitializeConsoleBeforeRead(uint8_t minChars, uint8_t decisecondsTimeout)
{
    (void)minChars;
    (void)decisecondsTimeout;
}

void SystemNative_UninitializeConsoleAfterRead(void)
{
}

void SystemNative_ConfigureTerminalForChildProcess(int32_t enable)
{
    (void)enable;
}

void SystemNative_UninitializeTerminal(void)
{
}

int32_t SystemNative_GetSignalForBreak(void)
{
    return g_signal_for_break;
}

int32_t SystemNative_SetSignalForBreak(int32_t signalForBreak)
{
    g_signal_for_break = signalForBreak != 0 ? 1 : 0;
    return 1;
}

/* ------------------------------------------------------------------ */
/* the process as the class library sees it: descriptor mode, the      */
/* working directory, identity, the environment, the log streams       */
/* ------------------------------------------------------------------ */

/* No served descriptor is ever non-blocking: the console answers what it
 * has and a file in the store is always ready. The query reports that,
 * and the non-blocking read is the plain read. */
int32_t SystemNative_FcntlGetIsNonBlocking(intptr_t fd, int32_t *isNonBlocking)
{
    if (!(fd_is_console(fd) || fd_file(fd) != NULL)) {
        ERRNO = RHU_GERR_EBADF;
        return -1;
    }
    *isNonBlocking = 0;
    return 0;
}

int32_t SystemNative_ReadFromNonblocking(intptr_t fd, void *buffer, int32_t bufferSize)
{
    return SystemNative_Read(fd, buffer, bufferSize);
}

/* The working directory is the root of the file store: the one directory
 * this hosting has, so a relative path resolves under it. */
char *SystemNative_GetCwd(char *buffer, int32_t bufferSize)
{
    if (buffer == NULL || bufferSize < 2) {
        ERRNO = RHU_GERR_ERANGE;
        return NULL;
    }
    buffer[0] = '/';
    buffer[1] = 0;
    return buffer;
}

/* The one principal of a single-program device. Its identity is 0, and
 * its password entry names it and its home directory (the directory the
 * environment table names) and nothing else; any other identity has no
 * entry. */
#define RHU_SHIM_UID 0u
static const char g_principal_name[] = "rhu";

uint32_t SystemNative_GetEUid(void)
{
    return RHU_SHIM_UID;
}

/* One string of the record copied into the caller's buffer, advancing
 * the cursor; null when the buffer cannot hold it. */
static char *pw_put(char **at, const char *end, const char *s)
{
    size_t n = strlen(s) + 1u;
    char *p = *at;
    if ((size_t)(end - p) < n)
        return NULL;
    memcpy(p, s, n);
    *at = p + n;
    return p;
}

/* The record's string fields are copied into the caller's buffer, as the
 * library function this row stands for does; a buffer too small answers
 * the range error as a positive number, the row's own contract, and an
 * identity without an entry answers -1. */
int32_t SystemNative_GetPwUidR(uint32_t uid, rhu_pal_passwd *pwd, char *buf, int32_t buflen)
{
    char *at = buf;
    const char *end;
    const char *home;
    if (pwd == NULL || buf == NULL || buflen < 0)
        return RHU_GERR_EINVAL;
    if (uid != RHU_SHIM_UID)
        return -1;
    end = buf + buflen;
    home = getenv("HOME");
    memset(pwd, 0, sizeof *pwd);
    pwd->Name = pw_put(&at, end, g_principal_name);
    pwd->Password = pw_put(&at, end, "");
    pwd->UserId = uid;
    pwd->GroupId = uid;
    pwd->UserInfo = pw_put(&at, end, "");
    pwd->HomeDirectory = pw_put(&at, end, home != NULL ? home : "/");
    pwd->Shell = pw_put(&at, end, "");
    if (pwd->Name == NULL || pwd->Password == NULL || pwd->UserInfo == NULL
        || pwd->HomeDirectory == NULL || pwd->Shell == NULL)
        return RHU_GERR_ERANGE;
    return 0;
}

/* The environment is the shim's own table. */
char *SystemNative_GetEnv(const char *variable)
{
    return getenv(variable);
}

/* The two log streams are the console: the bytes go out as given, no
 * line ending added, exactly as the library function this row stands
 * for writes them. */
void SystemNative_Log(uint8_t *buffer, int32_t length)
{
    if (length > 0)
        rhu_console_write(buffer, (uint32_t)length);
}

void SystemNative_LogError(uint8_t *buffer, int32_t length)
{
    if (length > 0)
        rhu_console_write(buffer, (uint32_t)length);
}
