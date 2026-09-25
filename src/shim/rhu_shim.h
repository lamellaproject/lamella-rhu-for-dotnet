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
#define RHU_SHIM_FD_SOCKET_LO 32  /* reserved: 32 .. 47 */
#define RHU_SHIM_FD_DEVICE_LO 64  /* the bus device nodes: 64 .. 79 */
#define RHU_SHIM_FD_DEVICE_COUNT 16
#define RHU_SHIM_FD_STORAGE_LO 80 /* reserved: 80 .. 95 */

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
