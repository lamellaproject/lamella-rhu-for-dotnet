/* The Lamella RHU hosting API -- the C ABI for hosted payloads.
 *
 * Mirrors src/api.rs exactly; the two files change together or not at
 * all. Minimal on purpose: a futex pair plus a nanosecond clock and
 * sleep is the entire blocking surface of the kernel, and every richer
 * primitive (mutex, event, semaphore, condition variable) is a recipe
 * over these four in the layer above. No signals exist or ever will.
 */
#ifndef RHU_H
#define RHU_H

#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/* Deadline value meaning "wait forever". Deadlines are ABSOLUTE
 * nanoseconds on the rhu_now_ns clock, never relative. */
#define RHU_DEADLINE_NONE UINT64_MAX

/* Error surface (RHU-owned values, errno-mirroring by convention). */
#define RHU_EAGAIN (-11)    /* *addr != expected at entry; no sleep    */
#define RHU_ETIMEDOUT (-110) /* the deadline passed                    */
#define RHU_EINVAL (-22)    /* address null or not 4-aligned           */

/* Monotonic nanoseconds since scheduler init, 64-bit. Sub-millisecond
 * resolution by hardware-counter interpolation; never steps backward,
 * including across the tick-wrap window. */
uint64_t rhu_now_ns(void);

/* Sleep at least ns nanoseconds (relative request). NEVER returns
 * early; may return up to ~2 ms late at the 1 kHz tick. */
void rhu_sleep_ns(uint64_t ns);

/* Atomically (with respect to rhu_futex_wake) compare *addr against
 * expected and block while they match. Returns 0 when woken by
 * rhu_futex_wake, RHU_EAGAIN when *addr != expected at entry (no sleep
 * happened -- the caller re-reads its state and decides), RHU_ETIMEDOUT
 * when deadline_ns passed (never early), RHU_EINVAL on a bad address.
 * addr must be 4-aligned, non-null, and mapped for the life of the
 * wait. */
int32_t rhu_futex_wait(const uint32_t *addr, uint32_t expected,
                       uint64_t deadline_ns);

/* Ready up to n threads blocked on addr; returns the count readied
 * (0 when no thread waits there). Selection is rotation-fair in
 * scheduler slot order, never FIFO. A readied thread that is equal or
 * more urgent than the caller preempts immediately. */
int32_t rhu_futex_wake(const uint32_t *addr, uint32_t n);

#ifdef __cplusplus
}
#endif

#endif /* RHU_H */
