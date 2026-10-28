/* Declarations shared by the hosting shim's translation units: the
 * diagnostic line writer, the file-descriptor bands, the flat hosted
 * priority, the initialization entry the host calls first, the thread
 * spawn path the PAL routes through, and the loud exit. Nothing here is
 * part of the surface the runtime archives import; those names are
 * defined in the translation units with the signatures the archives
 * expect. */
#ifndef RHU_SHIM_H
#define RHU_SHIM_H

#include <stdarg.h>
#include <stddef.h>
#include <stdint.h>

/* The board rows header spells its compile-time assertions the C++ way;
 * C11 spells the same assertion _Static_assert. */
#if !defined(__cplusplus) && !defined(static_assert)
#define static_assert _Static_assert
#endif

/* The file-descriptor bands, disjoint by declaration: a descriptor's band
 * names its owner, so no fast path can mistake one kind of descriptor for
 * another. The console band, the file band and the device band are
 * served; a descriptor in a reserved band is refused as invalid, never
 * handed to a fallback. */
#define RHU_SHIM_FD_CONSOLE_LO 0
#define RHU_SHIM_FD_CONSOLE_HI 3 /* exclusive: the console is 0, 1, 2 */
#ifndef RHU_SHIM_FD_FILE_LO
#define RHU_SHIM_FD_FILE_LO 3
#endif
#define RHU_SHIM_FD_FILE_COUNT 13 /* 3 .. 15 */
#define RHU_SHIM_FD_TTY_LO 16     /* the serial device files: 16 .. 31 */
#define RHU_SHIM_FD_TTY_COUNT 16
#define RHU_SHIM_FD_SOCKET_LO 32  /* reserved: 32 .. 47 */
#define RHU_SHIM_FD_DEVICE_LO 64  /* the bus device nodes: 64 .. 79 */
#define RHU_SHIM_FD_DEVICE_COUNT 16
#define RHU_SHIM_FD_STORAGE_LO 80 /* reserved: 80 .. 95 */
#define RHU_SHIM_FD_SYSFS_LO 96   /* the PWM device files under /sys: 96 .. 127 */
#define RHU_SHIM_FD_SYSFS_COUNT 32
#define RHU_SHIM_FD_IIO_LO 128    /* the analog input device files under /sys/bus/iio: 128 .. 143 */
#define RHU_SHIM_FD_IIO_COUNT 16

/* Every hosted thread the shim spawns runs at the one flat priority the
 * kernel names for the hosted band (rhu.h): the level the payload host
 * thread itself holds while it runs the entry, so a started thread and
 * its starter share one level in round-robin and no priority inversion
 * can exist among them. The shim adopts the kernel's name, never a
 * literal of its own. */
#define RHU_SHIM_PRIO_FLAT ((uint32_t)RHU_HOSTED_PRIO_FLAT)

/* The bus device unit (rhu_busdev.c): the two device-node families a
 * program's I2C and SPI drivers open, presented over the kernel's bus
 * doors. The core's open hands it a path that starts with /dev/i2c- or
 * /dev/spidev; the core's ioctl and close hand it every descriptor of the
 * device band. Each answers as the C library's call would: the result, or
 * -1 with errno set. */
int rhu_busdev_open(const char *path, int flags);
int rhu_busdev_ioctl(int fd, unsigned long request, void *arg);
int rhu_busdev_close(int fd);

/* The PWM device files (rhu_pwmfs.c): the tree under /sys the standard
 * package's PWM class reads and writes, over the kernel's PWM API. The
 * runtime's portability layer hands a /sys path and every descriptor of the
 * sysfs band here; the C library's access does the same for a /sys path. */
struct rhu_pal_filestatus;
int rhu_pwmfs_owns(const char *path);
intptr_t rhu_pwmfs_open(const char *path, int32_t flags);
int32_t rhu_pwmfs_stat(const char *path, struct rhu_pal_filestatus *output);
int32_t rhu_pwmfs_access(const char *path, int32_t mode);
int rhu_pwmfs_fd(intptr_t fd);
int32_t rhu_pwmfs_close(intptr_t fd);
int32_t rhu_pwmfs_pread(intptr_t fd, void *buffer, int32_t bufferSize, int64_t offset, int use_pos);
int32_t rhu_pwmfs_pwrite(intptr_t fd, const void *buffer, int32_t bufferSize, int64_t offset, int use_pos);
int64_t rhu_pwmfs_lseek(intptr_t fd, int64_t offset, int32_t whence);
int32_t rhu_pwmfs_fstat(intptr_t fd, struct rhu_pal_filestatus *output);
int32_t rhu_pwmfs_ftruncate(intptr_t fd, int64_t length);
int32_t rhu_pwmfs_flock(intptr_t fd, int32_t operation);
int32_t rhu_pwmfs_fsync(intptr_t fd);
int32_t rhu_pwmfs_fcntl_setfd(intptr_t fd, int32_t flags);
intptr_t rhu_pwmfs_dup(intptr_t fd);

/* The analog input device files (rhu_iiofs.c): the tree under /sys/bus/iio
 * the board package's analog input class reads, over the kernel's ADC calls.
 * The runtime's portability layer hands a /sys/bus path and every descriptor
 * of the analog band here; the C library's access does the same for a
 * /sys/bus path. */
int rhu_iiofs_owns(const char *path);
intptr_t rhu_iiofs_open(const char *path, int32_t flags);
int32_t rhu_iiofs_stat(const char *path, struct rhu_pal_filestatus *output);
int32_t rhu_iiofs_access(const char *path, int32_t mode);
int rhu_iiofs_fd(intptr_t fd);
int32_t rhu_iiofs_close(intptr_t fd);
int32_t rhu_iiofs_pread(intptr_t fd, void *buffer, int32_t bufferSize, int64_t offset, int use_pos);
int32_t rhu_iiofs_pwrite(intptr_t fd, const void *buffer, int32_t bufferSize, int64_t offset, int use_pos);
int64_t rhu_iiofs_lseek(intptr_t fd, int64_t offset, int32_t whence);
int32_t rhu_iiofs_fstat(intptr_t fd, struct rhu_pal_filestatus *output);
int32_t rhu_iiofs_ftruncate(intptr_t fd, int64_t length);
int32_t rhu_iiofs_flock(intptr_t fd, int32_t operation);
int32_t rhu_iiofs_fsync(intptr_t fd);
int32_t rhu_iiofs_fcntl_setfd(intptr_t fd, int32_t flags);
intptr_t rhu_iiofs_dup(intptr_t fd);

/* The serial device files (rhu_ttydev.c): /dev/ttyS<n> and the alias
 * /dev/ttyAMA<n> over the kernel's UART doors, the Linux termios shape the
 * standard serial port class's native layer (compiled unchanged from the
 * vendored sources) drives through the C library. The core's open hands it
 * a path it owns; the core's ioctl and close and the glue's read and write
 * hand it every descriptor of the tty band. Each answers as the C library's
 * call would: the result, or -1 with errno set. The layer's other C-library
 * names (poll, fcntl, shutdown, the termios family, the error string) are
 * defined in the unit itself. The file system's two doors: whether a name
 * the unit owns is a port of this board, and the index-th row's leaf name
 * for a listing of /dev (the alias is never listed). */
int rhu_ttydev_owns_path(const char *path);
int rhu_ttydev_exists(const char *path);
int rhu_ttydev_name(uint32_t index, char *buf, size_t cap);
int rhu_ttydev_open(const char *path, int flags);
int rhu_ttydev_close(int fd);
int rhu_ttydev_ioctl(int fd, unsigned long request, void *arg);
long rhu_ttydev_read(int fd, void *buf, size_t n);
long rhu_ttydev_write(int fd, const void *buf, size_t n);

/* Initialization, called once on the main thread before any other shim
 * entry: registers the per-thread block with the kernel's module registry
 * and builds the environment table. Returns 0; a second call is a no-op. */
int rhu_shim_init(void);

/* The number of thread activations the kernel has delivered to this
 * image's handler so far (every delivery counts, whichever handler was
 * recorded); the host prints it at exit. */
uint32_t rhu_shim_activation_count(void);

/* One diagnostic line on the console, prefixed "[shim] " and terminated,
 * written as one atomic console write. The conversion set is the small
 * one the shim uses: %s %c %d %i %u %x %p %% with an optional 'l' or 'll'
 * length and an optional zero-padded width on the integer conversions;
 * an unsupported conversion is a loud death, never a silent wrong line. */
void rhu_shim_log(const char *fmt, ...);

/* The same line at RHU_LOG_SILENT: the last words of a path that ends the
 * program (a refusal at init, a fault the layer cannot deliver, a draw of
 * entropy refused), printed whatever the image's output level. */
void rhu_shim_last_words(const char *fmt, ...);

/* The diagnostic line's conversion core over a caller's buffer: the same
 * conversion set as rhu_shim_log, written into buf up to cap - 1 bytes and
 * terminated when cap is not 0 (cap 0 measures without writing), the full
 * length of the formatted text returned -- the C library's vsnprintf
 * contract, which the PAL's snprintf entry points state; an unsupported
 * conversion is a loud death here too, never a silent wrong string. */
int32_t rhu_shim_vformat(char *buf, uint32_t cap, const char *fmt, va_list ap);

/* The environment table the shim built at initialization, as the C
 * library's environ: the rows in the order stated, then a null. */
char **rhu_shim_environ(void);

/* The collector's own mapping entry: the managed heap's one region, its
 * mark bitmap and its mark stack, served from the window the managed
 * heap lives in -- window 1, the external RAM, when the link map placed
 * the runtime's rehydrated type data inside it; window 0 otherwise, as
 * mmap64 serves every other mapping. Page-rounded, page-aligned,
 * zero-born; (void *)-1 with ENOMEM when that window is exhausted, with
 * EINVAL for a zero length. */
void *rhu_shim_collector_mmap(size_t len);

/* Spawn a hosted thread on the shim's thread path: a stack request of 0
 * means the board row's default; a request above the row's clamp is
 * clamped with a diagnostic line. Returns 0, or the errno value EAGAIN
 * when the kernel refuses the spawn. */
int rhu_shim_create_thread(size_t stack_bytes, void *(*fn)(void *), void *arg);

/* Terminate the machine with a diagnostic line naming the code and the
 * calling thread. Never returns. */
void rhu_shim_exit(int code) __attribute__((noreturn));

/* The wall clock is the kernel's calendar. RHU_SHIM_UNIX_EPOCH_TICKS is the
 * calendar tick of 1970-01-01T00:00:00Z: 719,162 days of 864,000,000,000
 * ticks of 100 ns each since the epoch of 0001-01-01T00:00:00Z, the
 * hosted runtime's own epoch offset. rhu_shim_calendar_unix_ticks reads the
 * kernel's calendar for every wall-clock answer the shim gives (the PAL's
 * system time, the C library's real-time clock and time(), the file
 * store's times) as ticks since 1970-01-01T00:00:00Z, signed: negative
 * before that instant, which is how an unset calendar reads (the epoch
 * plus the elapsed clock). The first read states the calendar's source on
 * the console. */
#define RHU_SHIM_UNIX_EPOCH_TICKS 621355968000000000ll
static_assert(RHU_SHIM_UNIX_EPOCH_TICKS == 719162ll * 864000000000ll, "the epoch offset is the day count times the ticks per day");
int64_t rhu_shim_calendar_unix_ticks(void);

/* A signed tick count since 1970-01-01T00:00:00Z split into whole seconds
 * and the nanoseconds within that second, 0 to 999,999,999: the floor is
 * taken, so a negative count never yields a negative nanosecond field. */
static inline void rhu_shim_unix_split(int64_t ticks, int64_t *sec, int32_t *nsec)
{
    int64_t s = ticks / 10000000ll;
    int64_t rem = ticks - s * 10000000ll;
    if (rem < 0) {
        s -= 1;
        rem += 10000000ll;
    }
    *sec = s;
    *nsec = (int32_t)(rem * 100);
}

#endif /* RHU_SHIM_H */
