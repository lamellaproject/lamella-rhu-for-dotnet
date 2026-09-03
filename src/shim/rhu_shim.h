/* Declarations shared by the hosting shim's translation units: the
 * diagnostic line writer, the file-descriptor bands, the flat hosted
 * priority, the initialization entry the host calls first, the thread
 * spawn path the PAL routes through, and the loud exit. Nothing here is
 * part of the surface the runtime archives import; those names are
 * defined in the translation units with the signatures the archives
 * expect. */
#ifndef RHU_SHIM_H
#define RHU_SHIM_H

#include <stddef.h>
#include <stdint.h>

/* The board rows header spells its compile-time assertions the C++ way;
 * C11 spells the same assertion _Static_assert. */
#if !defined(__cplusplus) && !defined(static_assert)
#define static_assert _Static_assert
#endif

/* The file-descriptor bands, disjoint by declaration: a descriptor's band
 * names its owner, so no fast path can mistake one kind of descriptor for
 * another. The console band and the file band are served; a descriptor in
 * a reserved band is refused as invalid, never handed to a fallback. */
#define RHU_SHIM_FD_CONSOLE_LO 0
#define RHU_SHIM_FD_CONSOLE_HI 3 /* exclusive: the console is 0, 1, 2 */
#ifndef RHU_SHIM_FD_FILE_LO
#define RHU_SHIM_FD_FILE_LO 3
#endif
#define RHU_SHIM_FD_FILE_COUNT 13 /* 3 .. 15 */
#define RHU_SHIM_FD_SOCKET_LO 32  /* reserved: 32 .. 47 */
#define RHU_SHIM_FD_DEVICE_LO 64  /* reserved: 64 .. 79 */
#define RHU_SHIM_FD_STORAGE_LO 80 /* reserved: 80 .. 95 */

/* Every hosted thread the shim spawns runs at one fixed priority inside
 * the kernel's hosted band: equal-priority round-robin, so no priority
 * inversion can exist among them. */
#define RHU_SHIM_PRIO_FLAT 0x80u

/* Initialization, called once on the main thread before any other shim
 * entry: registers the per-thread block with the kernel's module registry
 * and builds the environment table. Returns 0; a second call is a no-op. */
int rhu_shim_init(void);

/* One diagnostic line on the console, prefixed "[shim] " and terminated,
 * written as one atomic console write. The conversion set is the small
 * one the shim uses: %s %c %d %i %u %x %p %% with an optional 'l' or 'll'
 * length and an optional zero-padded width on the integer conversions;
 * an unsupported conversion is a loud death, never a silent wrong line. */
void rhu_shim_log(const char *fmt, ...);

/* Spawn a hosted thread on the shim's thread path: a stack request of 0
 * means the board row's default; a request above the row's clamp is
 * clamped with a diagnostic line. Returns 0, or the errno value EAGAIN
 * when the kernel refuses the spawn. */
int rhu_shim_create_thread(size_t stack_bytes, void *(*fn)(void *), void *arg);

/* Terminate the machine with a diagnostic line naming the code and the
 * calling thread. Never returns. */
void rhu_shim_exit(int code) __attribute__((noreturn));

#endif /* RHU_SHIM_H */
