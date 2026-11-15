/* The socket family's own declarations, shared by the family's units, the
 * descriptor layer and the gate's ladders. */
#ifndef RHU_SOCK_H
#define RHU_SOCK_H

#include <stdint.h>

#include "rhu.h"

/* The runtime's error for a kernel network function's refusal. A kernel code
 * is answered as the runtime's error of the same name, with one named case:
 * RHU_ENODEV, the kernel's answer on a row with no network medium, is the
 * address family not supported, the error a socket's constructor reports on
 * a system without the family. Every other value -- a code outside the
 * kernel's set, zero, a positive number -- is the input-output error: the
 * answer is never success. */
int32_t rhu_sock_error(int32_t kernel_code);

/* A caller's socket-address record and the kernel's address record. The
 * caller's buffer holds the Linux record of its family, the port in network
 * byte order; the kernel's record holds the family as Linux numbers it, the
 * port as a number and the address bytes in the wire's order. Each answers
 * the runtime's success or its error: the fault for a buffer too short for
 * its record, the address family not supported for a family with no kernel
 * record. The second takes the buffer's room in *len and leaves the
 * record's size there. */
int32_t rhu_sock_addr_to_kernel(const uint8_t *address, int32_t len, rhu_net_addr *out);
int32_t rhu_sock_addr_from_kernel(const rhu_net_addr *in, uint8_t *address, int32_t *len);

/* The kernel's socket functions as the family's units call them: one table,
 * the kernel's own functions its default. An image that defines
 * rhu_sock_kernel_hook serves its own table in this order (the gate's
 * ladders script a kernel through it); the socket unit names the hook
 * weakly, so every other image keeps the kernel's functions. */
typedef struct rhu_sock_kernel {
    int32_t (*socket)(uint32_t family, uint32_t type, uint32_t protocol);
    int32_t (*bind)(int32_t h, const rhu_net_addr *local);
    int32_t (*listen)(int32_t h, uint32_t backlog);
    int32_t (*accept)(int32_t h, rhu_net_addr *peer);
    int32_t (*connect)(int32_t h, const rhu_net_addr *peer);
    int32_t (*name)(int32_t h, rhu_net_addr *local, rhu_net_addr *peer);
    int32_t (*shutdown)(int32_t h, uint32_t how);
    int32_t (*close)(int32_t h);
    int32_t (*available)(int32_t h);
    int32_t (*error)(int32_t h);
    const uint32_t *(*event_word)(int32_t h);
    int32_t (*readiness)(int32_t h, uint32_t take);
    int32_t (*recv)(int32_t h, void *buf, uint32_t n, uint32_t flags, rhu_net_addr *from);
    int32_t (*send)(int32_t h, const void *buf, uint32_t n, const rhu_net_addr *to);
    int32_t (*option)(int32_t h, uint32_t option, uint32_t set, int32_t value);
    int32_t (*limits)(rhu_net_limits_desc *out);
    void (*events_bump)(void);
    const uint32_t *(*events_word)(void);
    int32_t (*count)(void);
    int32_t (*lease)(uint32_t medium, rhu_net_lease_desc *out);
    int32_t (*resolve)(const char *name, uint32_t len);
    int32_t (*resolve_result)(int32_t handle, rhu_net_addr *out, uint32_t max);
    const uint32_t *(*resolve_word)(int32_t handle);
    int32_t (*medium)(uint32_t medium, rhu_net_medium_desc *out);
} rhu_sock_kernel;

const rhu_sock_kernel *rhu_sock_kernel_hook(void);

/* The table as the family's units call it: the hook's when the image
 * defines one, the kernel's own functions otherwise. */
const rhu_sock_kernel *rhu_sock_kernel_table(void);

/* The socket band's descriptors as the descriptor layer hands them on: the
 * verbs a socket's handle reaches outside the socket functions. Each
 * answers as the layer's own verb does: the result, or -1 with the
 * platform's errno set. The socket unit defines rhu_sock_fd_hook; the
 * descriptor layer names it weakly, so an image that carries no socket unit
 * links without it and its socket band has no owner.
 *
 * The last two are Poll's. poll_events answers an open socket's level as
 * the runtime's poll events: readable and writable where the caller asked
 * for them, an error and a hang-up always; the invalid event for a
 * descriptor that is not open. poll_wait is the one wait (rhu_sock_wait)
 * on the word every socket's change moves, with the caller's scan: the
 * scan's first answer that is not 0, or 0 when the deadline passes first. */
struct rhu_pal_filestatus;
typedef int32_t (*rhu_sock_scan)(void *context);
typedef struct rhu_sock_fd_verbs {
    int32_t (*close)(intptr_t fd);
    int32_t (*read)(intptr_t fd, void *buffer, int32_t count);
    int32_t (*write)(intptr_t fd, const void *buffer, int32_t count);
    int32_t (*fstat)(intptr_t fd, struct rhu_pal_filestatus *output);
    int32_t (*get_nonblocking)(intptr_t fd, int32_t *is_nonblocking);
    int32_t (*set_nonblocking)(intptr_t fd, int32_t is_nonblocking);
    int32_t (*get_fd_flags)(intptr_t fd);
    int32_t (*poll_events)(intptr_t fd, int32_t wanted);
    int32_t (*poll_wait)(rhu_sock_scan scan, void *context, uint64_t deadline_ns);
} rhu_sock_fd_verbs;

const rhu_sock_fd_verbs *rhu_sock_fd_hook(void);

/* The one wait over a kernel readiness word. The word is read, then the
 * caller's scan runs; while the scan answers 0 the thread sleeps until the
 * word moves, and the scan runs again. The answer is the scan's first
 * answer that is not 0; RHU_ETIMEDOUT when the deadline passes first
 * (RHU_DEADLINE_NONE: no bound); RHU_EBADF for no word. A scan may run any
 * number of times and keeps what it found in its context. */
int32_t rhu_sock_wait(const uint32_t *word, rhu_sock_scan scan, void *context, uint64_t deadline_ns);

#endif /* RHU_SOCK_H */
