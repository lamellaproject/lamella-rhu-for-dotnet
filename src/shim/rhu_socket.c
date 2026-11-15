/* The sockets of the runtime's portability layer, over the kernel's network
 * functions.
 *
 * A socket is a descriptor of the socket band. Its record here holds the
 * kernel's handle and what the kernel does not keep: the socket's type,
 * whether it listens, whether the runtime set it non-blocking, the linger
 * record and the two timeouts. The functions below create, bind, listen,
 * accept, connect, name, shut down and close a socket, send and receive
 * through it, and read and write its options.
 *
 * The kernel's functions never wait. A socket is blocking until the runtime
 * sets it non-blocking, and a blocking receive, send, accept or connect that
 * the kernel answers with "not yet" waits here, on the socket's own word,
 * for the kernel's next change, up to the recorded timeout (0 is no bound),
 * and tries again. At the bound a receive, a send and an accept answer
 * EAGAIN and a connect EINPROGRESS, the answers the managed socket class
 * reads as a timeout on a blocking handle. Every wait goes through one
 * function, rhu_sock_wait.
 *
 * Every refusal of a kernel function is answered as the runtime's error
 * through the family's table (rhu_sock_error). A caller's address is the
 * Linux record of its family (rhu_sockaddr.c).
 *
 * The options: no-delay, keep-alive and address reuse are the kernel's;
 * the type, the listening state, linger and the two timeouts are this
 * unit's records; the two buffer sizes are the board's, read from the
 * kernel, and a write to one is answered as success with the size as it
 * was. Any other option is answered ENOPROTOOPT.
 *
 * The event port is the runtime's one way to wait for many sockets. A
 * socket is registered with a port once, with a data word, and a wait
 * answers the sockets that have something new: at a socket's first wait
 * its level, after that the kernel's bits that rose since the last take --
 * readable as read, writable as write, an error as error, a hang-up as
 * read and write together. With nothing to report a wait sleeps on the
 * word every socket's change moves; it answers one event or more, never
 * none. A registration that changes moves that word, and a socket's close
 * ends its registration before the kernel releases the handle.
 *
 * The descriptor layer reaches the band's descriptors through the table
 * rhu_sock_fd_hook answers: close, read, write, status, the descriptor
 * flags, and for Poll a socket's level and the wait.
 *
 * This file is an implementation written from scratch that presents the
 * runtime's own entry points; the signatures are the generated surface
 * header's, read off the runtime's headers. */
#include <stddef.h>
#include <stdint.h>

#include "rhu_shim.h"

#include "rhu.h"
#include "rhu_pal_errno_net11.h"
#include "rhu_pal_io_net11.h"
#include "rhu_pal_networking_net11.h"
#include "rhu_pal_surface_net11.h"
#include "rhu_sock.h"

void *memcpy(void *dst, const void *src, size_t n);
void *memset(void *dst, int c, size_t n);
void *malloc(size_t n);
void free(void *p);
int pthread_mutex_lock(void *mutex);
int pthread_mutex_unlock(void *mutex);
extern int *__errno_location(void);
#define ERRNO (*__errno_location())

#define SOCK_LINGER_MAX 65535 /* the longest linger the runtime's own layer takes, in seconds */

/* The kernel's functions, or a scripted kernel's. */
static const rhu_sock_kernel g_sock_exports = {
    rhu_net_socket, rhu_net_bind, rhu_net_listen, rhu_net_accept, rhu_net_connect, rhu_net_name, rhu_net_shutdown, rhu_net_close,
    rhu_net_available, rhu_net_error, rhu_net_event_word, rhu_net_readiness, rhu_net_recv, rhu_net_send, rhu_net_option, rhu_net_limits,
    rhu_net_events_bump, rhu_net_events_word, rhu_net_count, rhu_net_lease, rhu_net_resolve, rhu_net_resolve_result, rhu_net_resolve_word, rhu_net_medium,
};

const rhu_sock_kernel *rhu_sock_kernel_hook(void) __attribute__((weak));

const rhu_sock_kernel *rhu_sock_kernel_table(void)
{
    return rhu_sock_kernel_hook != NULL ? rhu_sock_kernel_hook() : &g_sock_exports;
}

static const rhu_sock_kernel *sock_k(void)
{
    return rhu_sock_kernel_table();
}

/* A descriptor of the socket band. A slot is free, held while a socket is
 * made or closed behind it, or open. The last four fields are the socket's
 * registration with an event port, written under the port lock: the port
 * (0: none), the events asked for, the data word a wait answers with, and
 * the mark of a registration no wait has yet reported. A registration ends
 * at the socket's close, so a free slot carries none. */
enum { SLOT_FREE = 0, SLOT_HELD = 1, SLOT_OPEN = 2 };

typedef struct sock_fd {
    uint8_t state;
    uint8_t type; /* RHU_NET_STREAM or RHU_NET_DATAGRAM */
    uint8_t nonblocking;
    uint8_t listening;
    int32_t handle; /* the kernel's */
    int32_t linger_on;
    int32_t linger_seconds;
    int32_t recv_timeout_ms; /* 0: no bound */
    int32_t send_timeout_ms;
    uint8_t port; /* 1 .. SOCK_PORTS; 0: not registered */
    uint8_t port_fresh;
    int32_t port_events;
    uintptr_t port_data;
} sock_fd;

static sock_fd g_socks[RHU_SHIM_FD_SOCKET_COUNT];

/* The event ports. A port's handle is its number over a tag, a value
 * outside every band of descriptors, so the descriptor layer's functions
 * answer it as a bad descriptor. */
#define SOCK_PORTS 4
#define SOCK_PORT_TAG ((intptr_t)0x45500000)

static uint8_t g_ports[SOCK_PORTS]; /* 1: open */
static uint32_t g_port_mutex[6];

static void port_lock(void)
{
    pthread_mutex_lock(g_port_mutex);
}

static void port_unlock(void)
{
    pthread_mutex_unlock(g_port_mutex);
}

/* The band's lowest free slot, held for the caller; NULL with the band full. */
static sock_fd *slot_hold(intptr_t *fd)
{
    for (uint32_t i = 0; i < RHU_SHIM_FD_SOCKET_COUNT; i++) {
        uint8_t expect = SLOT_FREE;
        if (__atomic_compare_exchange_n(&g_socks[i].state, &expect, (uint8_t)SLOT_HELD, 0, __ATOMIC_SEQ_CST, __ATOMIC_SEQ_CST)) {
            *fd = RHU_SHIM_FD_SOCKET_LO + (intptr_t)i;
            return &g_socks[i];
        }
    }
    return NULL;
}

static void slot_free(sock_fd *s)
{
    __atomic_store_n(&s->state, (uint8_t)SLOT_FREE, __ATOMIC_SEQ_CST);
}

/* A held slot opened over a kernel handle: a blocking socket with no linger
 * and no timeout, as a new socket is. */
static void slot_open(sock_fd *s, int32_t handle, uint8_t type)
{
    s->type = type;
    s->nonblocking = 0;
    s->listening = 0;
    s->handle = handle;
    s->linger_on = 0;
    s->linger_seconds = 0;
    s->recv_timeout_ms = 0;
    s->send_timeout_ms = 0;
    __atomic_store_n(&s->state, (uint8_t)SLOT_OPEN, __ATOMIC_SEQ_CST);
}

/* The open socket behind a descriptor; NULL with the runtime's error: a
 * descriptor outside the band is not a socket, a slot not open is a bad
 * descriptor. */
static sock_fd *sock_of(intptr_t fd, int32_t *error)
{
    if (fd < RHU_SHIM_FD_SOCKET_LO || fd >= RHU_SHIM_FD_SOCKET_LO + RHU_SHIM_FD_SOCKET_COUNT) {
        *error = RHU_PAL_ENOTSOCK;
        return NULL;
    }
    sock_fd *s = &g_socks[fd - RHU_SHIM_FD_SOCKET_LO];
    if (__atomic_load_n(&s->state, __ATOMIC_SEQ_CST) != SLOT_OPEN) {
        *error = RHU_PAL_EBADF;
        return NULL;
    }
    return s;
}

int32_t rhu_sock_wait(const uint32_t *word, rhu_sock_scan scan, void *context, uint64_t deadline_ns)
{
    if (word == NULL)
        return RHU_EBADF;
    for (;;) {
        uint32_t seen = *(const volatile uint32_t *)word;
        int32_t found = scan(context);
        if (found != 0)
            return found;
        int32_t rc = rhu_futex_wait(word, seen, deadline_ns);
        if (rc == RHU_ETIMEDOUT || rc == RHU_EINVAL)
            return rc;
    }
}

/* One kernel call a blocking socket may wait for, and its answer. */
typedef struct sock_op {
    const rhu_sock_kernel *k;
    int32_t handle;
    void *buffer;
    const void *bytes;
    uint32_t count;
    uint32_t flags;
    rhu_net_addr *address;
    const rhu_net_addr *to;
    int32_t answer;
} sock_op;

static int32_t scan_recv(void *context)
{
    sock_op *op = (sock_op *)context;
    op->answer = op->k->recv(op->handle, op->buffer, op->count, op->flags, op->address);
    return op->answer != RHU_EAGAIN;
}

static int32_t scan_send(void *context)
{
    sock_op *op = (sock_op *)context;
    op->answer = op->k->send(op->handle, op->bytes, op->count, op->to);
    return op->answer != RHU_EAGAIN;
}

static int32_t scan_accept(void *context)
{
    sock_op *op = (sock_op *)context;
    op->answer = op->k->accept(op->handle, op->address);
    return op->answer != RHU_EAGAIN;
}

/* A handshake's end: the socket's level shows it writable, failed or hung up. */
static int32_t scan_connected(void *context)
{
    sock_op *op = (sock_op *)context;
    op->answer = op->k->readiness(op->handle, 0);
    return op->answer < 0 || (op->answer & (RHU_NET_WRITABLE | RHU_NET_ERROR | RHU_NET_HANGUP)) != 0;
}

/* The wait of a blocking call on the socket's own word, up to a bound in
 * milliseconds (0: no bound): the scan's answer, or RHU_ETIMEDOUT. */
static int32_t wait_on_socket(sock_op *op, rhu_sock_scan scan, int32_t timeout_ms)
{
    uint64_t deadline = RHU_DEADLINE_NONE;
    if (timeout_ms > 0) {
#ifdef RHU_SHIM_BREAK_SOCK_TIMEOUT
        deadline = rhu_now_ns();
#else
        deadline = rhu_now_ns() + (uint64_t)timeout_ms * 1000000ull;
#endif
    }
    return rhu_sock_wait(op->k->event_word(op->handle), scan, op, deadline);
}

/* One receive: the kernel's answer. On a blocking socket an empty queue is
 * waited for up to the receive timeout, and the bound's answer is RHU_EAGAIN. */
static int32_t recv_once(const sock_fd *s, void *buffer, uint32_t count, uint32_t kflags, int dont_wait, rhu_net_addr *from)
{
    sock_op op = { sock_k(), s->handle, buffer, NULL, count, kflags, from, NULL, 0 };
    op.answer = op.k->recv(op.handle, buffer, count, kflags, from);
    if (op.answer == RHU_EAGAIN && !s->nonblocking && !dont_wait) {
        int32_t w = wait_on_socket(&op, scan_recv, s->recv_timeout_ms);
        if (w < 0)
            return w == RHU_ETIMEDOUT ? RHU_EAGAIN : w;
    }
    return op.answer;
}

/* One send: the count the kernel took. On a blocking socket no room is
 * waited for up to the send timeout, and the bound's answer is RHU_EAGAIN. */
static int32_t send_once(const sock_fd *s, const void *bytes, uint32_t count, int dont_wait, const rhu_net_addr *to)
{
    sock_op op = { sock_k(), s->handle, NULL, bytes, count, 0, NULL, to, 0 };
    op.answer = op.k->send(op.handle, bytes, count, to);
    if (op.answer == RHU_EAGAIN && !s->nonblocking && !dont_wait) {
        int32_t w = wait_on_socket(&op, scan_send, s->send_timeout_ms);
        if (w < 0)
            return w == RHU_ETIMEDOUT ? RHU_EAGAIN : w;
    }
    return op.answer;
}

/* The runtime's flags of a receive or a send as the kernel's. The peek, a
 * receive's alone, and the don't-wait flag are served; with any other flag
 * the answer is 0. */
static int flags_of(int32_t pal, int receive, uint32_t *kflags, int *dont_wait)
{
    int32_t served = RHU_PAL_SocketFlags_MSG_DONTWAIT | (receive ? RHU_PAL_SocketFlags_MSG_PEEK : 0);
    if ((pal & ~served) != 0)
        return 0;
    *dont_wait = (pal & RHU_PAL_SocketFlags_MSG_DONTWAIT) != 0;
    *kflags = 0;
#ifndef RHU_SHIM_BREAK_SOCK_PEEK
    if ((pal & RHU_PAL_SocketFlags_MSG_PEEK) != 0)
        *kflags = RHU_NET_PEEK;
#endif
    return 1;
}

/* The socket's pending code, which the kernel answers once and clears: 0
 * for none. */
static int32_t sock_pending(const sock_fd *s)
{
    int32_t code = sock_k()->error(s->handle);
#ifdef RHU_SHIM_BREAK_SOCK_PENDING_ERROR
    code = 0;
#endif
    return code;
}

/* ------------------------------------------------------------------ */
/* a socket's life: create, bind, listen, accept, connect, shut down   */
/* ------------------------------------------------------------------ */

int32_t SystemNative_Socket(int32_t addressFamily, int32_t socketType, int32_t protocolType, intptr_t* createdSocket)
{
    if (createdSocket == NULL)
        return RHU_PAL_EFAULT;
    *createdSocket = -1;
    uint32_t family, type;
    switch (addressFamily) {
    case RHU_PAL_AddressFamily_AF_INET:
        family = RHU_NET_FAMILY_IPV4;
        break;
    case RHU_PAL_AddressFamily_AF_INET6:
        family = RHU_NET_FAMILY_IPV6;
        break;
    default:
        return RHU_PAL_EAFNOSUPPORT;
    }
    switch (socketType) {
    case RHU_PAL_SocketType_SOCK_STREAM:
        type = RHU_NET_STREAM;
        break;
    case RHU_PAL_SocketType_SOCK_DGRAM:
        type = RHU_NET_DATAGRAM;
        break;
    default:
        return RHU_PAL_EPROTONOSUPPORT;
    }
    if (protocolType != RHU_PAL_ProtocolType_PT_UNSPECIFIED && protocolType != RHU_PAL_ProtocolType_PT_TCP && protocolType != RHU_PAL_ProtocolType_PT_UDP)
        return RHU_PAL_EPROTONOSUPPORT;
    intptr_t fd = -1;
    sock_fd *s = slot_hold(&fd);
    if (s == NULL)
        return RHU_PAL_EMFILE;
    int32_t handle = sock_k()->socket(family, type, (uint32_t)protocolType);
    if (handle < 0) {
        slot_free(s);
        return rhu_sock_error(handle);
    }
    slot_open(s, handle, (uint8_t)type);
    *createdSocket = fd;
    return RHU_PAL_SUCCESS;
}

/* The kernel is handed the address as it is: address reuse is the caller's
 * to set, and the protocol plays no part. */
int32_t SystemNative_Bind(intptr_t socket, int32_t protocolType, uint8_t* socketAddress, int32_t socketAddressLen)
{
    (void)protocolType;
    if (socketAddress == NULL || socketAddressLen < 0)
        return RHU_PAL_EFAULT;
    int32_t err = RHU_PAL_SUCCESS;
    sock_fd *s = sock_of(socket, &err);
    if (s == NULL)
        return err;
    rhu_net_addr local;
    err = rhu_sock_addr_to_kernel(socketAddress, socketAddressLen, &local);
    if (err != RHU_PAL_SUCCESS)
        return err;
    int32_t rc = sock_k()->bind(s->handle, &local);
    return rc < 0 ? rhu_sock_error(rc) : RHU_PAL_SUCCESS;
}

/* A backlog below one is handed to the kernel as one. */
int32_t SystemNative_Listen(intptr_t socket, int32_t backlog)
{
    int32_t err = RHU_PAL_SUCCESS;
    sock_fd *s = sock_of(socket, &err);
    if (s == NULL)
        return err;
    int32_t rc = sock_k()->listen(s->handle, backlog < 1 ? 1u : (uint32_t)backlog);
    if (rc < 0)
        return rhu_sock_error(rc);
    s->listening = 1;
    return RHU_PAL_SUCCESS;
}

/* The first completed connection behind a listener, as a new blocking
 * socket of the band, with the peer's address in the caller's record. A
 * blocking listener with none completed waits up to its receive timeout. */
int32_t SystemNative_Accept(intptr_t socket, uint8_t* socketAddress, int32_t* socketAddressLen, intptr_t* acceptedSocket)
{
    if (socketAddress == NULL || socketAddressLen == NULL || acceptedSocket == NULL || *socketAddressLen < 0)
        return RHU_PAL_EFAULT;
    *acceptedSocket = -1;
    int32_t err = RHU_PAL_SUCCESS;
    sock_fd *s = sock_of(socket, &err);
    if (s == NULL)
        return err;
    intptr_t fd = -1;
    sock_fd *made = slot_hold(&fd);
    if (made == NULL)
        return RHU_PAL_EMFILE;
    rhu_net_addr peer;
    memset(&peer, 0, sizeof peer);
    sock_op op = { sock_k(), s->handle, NULL, NULL, 0, 0, &peer, NULL, 0 };
    op.answer = op.k->accept(op.handle, &peer);
    if (op.answer == RHU_EAGAIN && !s->nonblocking) {
        int32_t w = wait_on_socket(&op, scan_accept, s->recv_timeout_ms);
        if (w < 0)
            op.answer = w == RHU_ETIMEDOUT ? RHU_EAGAIN : w;
    }
    if (op.answer < 0) {
        slot_free(made);
        return rhu_sock_error(op.answer);
    }
#ifndef RHU_SHIM_BREAK_SOCK_ACCEPT_PEER
    err = rhu_sock_addr_from_kernel(&peer, socketAddress, socketAddressLen);
    if (err != RHU_PAL_SUCCESS) {
        op.k->close(op.answer);
        slot_free(made);
        return err;
    }
#endif
    slot_open(made, op.answer, RHU_NET_STREAM);
    *acceptedSocket = fd;
    return RHU_PAL_SUCCESS;
}

/* A stream's connect starts a handshake. A non-blocking socket answers
 * EINPROGRESS and the caller reads the pending error at the handshake's
 * end; a blocking socket waits for that end up to its send timeout and
 * answers the pending code itself. A datagram socket's connect sets its
 * default peer at once. */
int32_t SystemNative_Connect(intptr_t socket, uint8_t* socketAddress, int32_t socketAddressLen)
{
    if (socketAddress == NULL || socketAddressLen < 0)
        return RHU_PAL_EFAULT;
    int32_t err = RHU_PAL_SUCCESS;
    sock_fd *s = sock_of(socket, &err);
    if (s == NULL)
        return err;
    rhu_net_addr peer;
    err = rhu_sock_addr_to_kernel(socketAddress, socketAddressLen, &peer);
    if (err != RHU_PAL_SUCCESS)
        return err;
    sock_op op = { sock_k(), s->handle, NULL, NULL, 0, 0, NULL, NULL, 0 };
    int32_t rc = op.k->connect(op.handle, &peer);
    if (rc != RHU_EINPROGRESS || s->nonblocking)
        return rc == 0 ? RHU_PAL_SUCCESS : rhu_sock_error(rc);
    int32_t w = wait_on_socket(&op, scan_connected, s->send_timeout_ms);
    if (w == RHU_ETIMEDOUT)
        return RHU_PAL_EINPROGRESS;
    if (w < 0)
        return rhu_sock_error(w);
    if (op.answer < 0)
        return rhu_sock_error(op.answer);
    int32_t pending = sock_pending(s);
    return pending == 0 ? RHU_PAL_SUCCESS : rhu_sock_error(pending);
}

/* A connect that carries no data: the caller sends its bytes itself when
 * none went with the connect. */
int32_t SystemNative_Connectx(intptr_t socket, uint8_t* socketAddress, int32_t socketAddressLen, uint8_t* data, int32_t dataLen, int32_t tfo, int* sent)
{
    (void)data;
    (void)dataLen;
    (void)tfo;
    if (socketAddress == NULL || socketAddressLen < 0 || sent == NULL)
        return RHU_PAL_EFAULT;
    *sent = 0;
    return SystemNative_Connect(socket, socketAddress, socketAddressLen);
}

static int32_t shut(intptr_t socket, uint32_t how)
{
    int32_t err = RHU_PAL_SUCCESS;
    sock_fd *s = sock_of(socket, &err);
    if (s == NULL)
        return err;
    int32_t rc = sock_k()->shutdown(s->handle, how);
    return rc < 0 ? rhu_sock_error(rc) : RHU_PAL_SUCCESS;
}

/* Both directions shut: a stream's close then ends the connection in order. */
int32_t SystemNative_Disconnect(intptr_t socket)
{
    return shut(socket, RHU_NET_SHUT_RDWR);
}

int32_t SystemNative_Shutdown(intptr_t socket, int32_t socketShutdown)
{
    switch (socketShutdown) {
    case RHU_PAL_SocketShutdown_SHUT_READ:
        return shut(socket, RHU_NET_SHUT_RD);
    case RHU_PAL_SocketShutdown_SHUT_WRITE:
        return shut(socket, RHU_NET_SHUT_WR);
    case RHU_PAL_SocketShutdown_SHUT_BOTH:
        return shut(socket, RHU_NET_SHUT_RDWR);
    default:
        return RHU_PAL_EINVAL;
    }
}

/* The local or the peer half of a socket's name in the caller's record. */
static int32_t name_of(intptr_t socket, uint8_t *address, int32_t *len, int peer_half)
{
    if (address == NULL || len == NULL || *len < 0)
        return RHU_PAL_EFAULT;
    int32_t err = RHU_PAL_SUCCESS;
    sock_fd *s = sock_of(socket, &err);
    if (s == NULL)
        return err;
    rhu_net_addr local, peer;
    memset(&local, 0, sizeof local);
    memset(&peer, 0, sizeof peer);
    int32_t rc = sock_k()->name(s->handle, &local, &peer);
    if (rc < 0)
        return rhu_sock_error(rc);
    if (peer_half && peer.family == 0)
        return RHU_PAL_ENOTCONN;
    return rhu_sock_addr_from_kernel(peer_half ? &peer : &local, address, len);
}

int32_t SystemNative_GetSockName(intptr_t socket, uint8_t* socketAddress, int32_t* socketAddressLen)
{
    return name_of(socket, socketAddress, socketAddressLen, 0);
}

int32_t SystemNative_GetPeerName(intptr_t socket, uint8_t* socketAddress, int32_t* socketAddressLen)
{
    return name_of(socket, socketAddress, socketAddressLen, 1);
}

static int32_t pal_type(const sock_fd *s)
{
    return s->type == RHU_NET_STREAM ? RHU_PAL_SocketType_SOCK_STREAM : RHU_PAL_SocketType_SOCK_DGRAM;
}

int32_t SystemNative_GetSocketType(intptr_t socket, int32_t* addressFamily, int32_t* socketType, int32_t* protocolType, int32_t* isListening)
{
    if (addressFamily == NULL || socketType == NULL || protocolType == NULL || isListening == NULL)
        return RHU_PAL_EFAULT;
    int32_t err = RHU_PAL_SUCCESS;
    sock_fd *s = sock_of(socket, &err);
    if (s == NULL)
        return err;
    *addressFamily = RHU_PAL_AddressFamily_AF_INET;
    *socketType = pal_type(s);
    *protocolType = s->type == RHU_NET_STREAM ? RHU_PAL_ProtocolType_PT_TCP : RHU_PAL_ProtocolType_PT_UDP;
    *isListening = s->listening;
    return RHU_PAL_SUCCESS;
}

/* A stream's queued bytes, or the next datagram's length. */
int32_t SystemNative_GetBytesAvailable(intptr_t socket, int32_t* available)
{
    if (available == NULL)
        return RHU_PAL_EFAULT;
    *available = 0;
    int32_t err = RHU_PAL_SUCCESS;
    sock_fd *s = sock_of(socket, &err);
    if (s == NULL)
        return err;
    int32_t rc = sock_k()->available(s->handle);
    if (rc < 0)
        return rhu_sock_error(rc);
    *available = rc;
    return RHU_PAL_SUCCESS;
}

/* ------------------------------------------------------------------ */
/* the data calls                                                      */
/* ------------------------------------------------------------------ */

int32_t SystemNative_Receive(intptr_t socket, void* buffer, int32_t bufferLen, int32_t flags, int32_t* received)
{
    if (buffer == NULL || bufferLen < 0 || received == NULL)
        return RHU_PAL_EFAULT;
    *received = 0;
    int32_t err = RHU_PAL_SUCCESS;
    sock_fd *s = sock_of(socket, &err);
    if (s == NULL)
        return err;
    uint32_t kflags = 0;
    int dont_wait = 0;
    if (!flags_of(flags, 1, &kflags, &dont_wait))
        return RHU_PAL_ENOTSUP;
    int32_t rc = recv_once(s, buffer, (uint32_t)bufferLen, kflags, dont_wait, NULL);
    if (rc < 0)
        return rhu_sock_error(rc);
    *received = rc;
    return RHU_PAL_SUCCESS;
}

int32_t SystemNative_Send(intptr_t socket, void* buffer, int32_t bufferLen, int32_t flags, int32_t* sent)
{
    if (buffer == NULL || bufferLen < 0 || sent == NULL)
        return RHU_PAL_EFAULT;
    *sent = 0;
    int32_t err = RHU_PAL_SUCCESS;
    sock_fd *s = sock_of(socket, &err);
    if (s == NULL)
        return err;
    uint32_t kflags = 0;
    int dont_wait = 0;
    if (!flags_of(flags, 0, &kflags, &dont_wait))
        return RHU_PAL_ENOTSUP;
    int32_t rc = send_once(s, buffer, (uint32_t)bufferLen, dont_wait, NULL);
    if (rc < 0)
        return rhu_sock_error(rc);
    *sent = rc;
    return RHU_PAL_SUCCESS;
}

/* A message's vectors that hold bytes: how many, the first of them, and
 * their bytes in all. */
static uint32_t vectors_of(const rhu_pal_messageheader *m, const rhu_pal_iovector **first, uint64_t *total)
{
    uint32_t used = 0;
    *first = NULL;
    *total = 0;
    for (int32_t i = 0; i < m->IOVectorCount; i++) {
        const rhu_pal_iovector *v = &m->IOVectors[i];
        if (v->Count == 0)
            continue;
        if (used == 0)
            *first = v;
        used++;
        *total += v->Count;
    }
    return used;
}

/* One datagram from a message's vectors: a datagram leaves whole, so several
 * vectors are gathered into one buffer first. The count sent, or -1 with
 * the runtime's error. */
static int32_t send_datagram(const sock_fd *s, const rhu_pal_messageheader *m, int dont_wait, const rhu_net_addr *to, int32_t *error)
{
    static const uint8_t no_bytes = 0;
    const rhu_pal_iovector *first = NULL;
    uint64_t total = 0;
    uint32_t used = vectors_of(m, &first, &total);
    int32_t rc;
    if (used == 0) {
        rc = send_once(s, &no_bytes, 0, dont_wait, to);
    } else if (used == 1) {
        rc = send_once(s, first->Base, (uint32_t)first->Count, dont_wait, to);
    } else {
#ifdef RHU_SHIM_BREAK_SOCK_GATHER
        rc = send_once(s, first->Base, (uint32_t)first->Count, dont_wait, to);
#else
        rhu_net_limits_desc limits;
        rc = sock_k()->limits(&limits);
        if (rc < 0) {
            *error = rhu_sock_error(rc);
            return -1;
        }
        if (total > limits.datagram_buf) {
            *error = RHU_PAL_EMSGSIZE;
            return -1;
        }
        uint8_t *whole = (uint8_t *)malloc((size_t)total);
        if (whole == NULL) {
            *error = RHU_PAL_ENOMEM;
            return -1;
        }
        uint32_t at = 0;
        for (int32_t i = 0; i < m->IOVectorCount; i++) {
            const rhu_pal_iovector *v = &m->IOVectors[i];
            if (v->Count != 0) {
                memcpy(whole + at, v->Base, v->Count);
                at += (uint32_t)v->Count;
            }
        }
        rc = send_once(s, whole, at, dont_wait, to);
        free(whole);
#endif
    }
    if (rc < 0) {
        *error = rhu_sock_error(rc);
        return -1;
    }
    return rc;
}

/* A message sent. A datagram socket's message leaves as one datagram, to
 * the destination in the message or to the default peer; a stream's
 * vectors are sent in order until the kernel takes less than one holds. A
 * message that carries control data is not served. */
int32_t SystemNative_SendMessage(intptr_t socket, MessageHeader* messageHeader, int32_t flags, int64_t* sent)
{
    const rhu_pal_messageheader *m = (const rhu_pal_messageheader *)messageHeader;
    if (m == NULL || sent == NULL || m->SocketAddressLen < 0 || m->ControlBufferLen < 0 || m->IOVectorCount < 0 || (m->IOVectorCount > 0 && m->IOVectors == NULL))
        return RHU_PAL_EFAULT;
    *sent = 0;
    int32_t err = RHU_PAL_SUCCESS;
    sock_fd *s = sock_of(socket, &err);
    if (s == NULL)
        return err;
    uint32_t kflags = 0;
    int dont_wait = 0;
    if (!flags_of(flags, 0, &kflags, &dont_wait) || m->ControlBufferLen > 0)
        return RHU_PAL_ENOTSUP;
    if (s->type == RHU_NET_DATAGRAM) {
        rhu_net_addr to;
        const rhu_net_addr *destination = NULL;
        if (m->SocketAddress != NULL && m->SocketAddressLen > 0) {
            err = rhu_sock_addr_to_kernel(m->SocketAddress, m->SocketAddressLen, &to);
            if (err != RHU_PAL_SUCCESS)
                return err;
            destination = &to;
        }
        int32_t rc = send_datagram(s, m, dont_wait, destination, &err);
        if (rc < 0)
            return err;
        *sent = rc;
        return RHU_PAL_SUCCESS;
    }
    int64_t done = 0;
    for (int32_t i = 0; i < m->IOVectorCount; i++) {
        const rhu_pal_iovector *v = &m->IOVectors[i];
        if (v->Count == 0)
            continue;
        int32_t rc = send_once(s, v->Base, (uint32_t)v->Count, dont_wait || done > 0, NULL);
        if (rc < 0) {
            if (done > 0)
                break;
            return rhu_sock_error(rc);
        }
        done += rc;
        if ((uint32_t)rc < v->Count)
            break;
    }
    *sent = done;
    return RHU_PAL_SUCCESS;
}

/* One datagram into a message's vectors: straight into the one vector that
 * has room, or through a buffer of the vectors' room in all and then across
 * them in order. The datagram's length, or -1 with the runtime's error. */
static int32_t recv_datagram(const sock_fd *s, const rhu_pal_messageheader *m, uint32_t kflags, int dont_wait, rhu_net_addr *from, int32_t *error)
{
    uint8_t no_room = 0;
    const rhu_pal_iovector *first = NULL;
    uint64_t total = 0;
    uint32_t used = vectors_of(m, &first, &total);
    int32_t rc;
    if (used <= 1) {
        rc = recv_once(s, used == 1 ? first->Base : &no_room, used == 1 ? (uint32_t)first->Count : 0u, kflags, dont_wait, from);
    } else {
        rhu_net_limits_desc limits;
        rc = sock_k()->limits(&limits);
        if (rc < 0) {
            *error = rhu_sock_error(rc);
            return -1;
        }
        if (total > limits.datagram_buf)
            total = limits.datagram_buf;
        uint8_t *whole = (uint8_t *)malloc((size_t)total);
        if (whole == NULL) {
            *error = RHU_PAL_ENOMEM;
            return -1;
        }
        rc = recv_once(s, whole, (uint32_t)total, kflags, dont_wait, from);
        uint32_t at = 0;
        for (int32_t i = 0; rc > 0 && i < m->IOVectorCount && at < (uint32_t)rc; i++) {
            const rhu_pal_iovector *v = &m->IOVectors[i];
            uint32_t piece = (uint32_t)rc - at;
            if (piece > v->Count)
                piece = (uint32_t)v->Count;
            if (piece != 0)
                memcpy(v->Base, whole + at, piece);
            at += piece;
        }
        free(whole);
    }
    if (rc < 0) {
        *error = rhu_sock_error(rc);
        return -1;
    }
    return rc;
}

/* A message received. A datagram socket's message is one datagram, with
 * its sender's address in the message's record; a stream's vectors are
 * filled in order while the queue holds bytes. No control data is written:
 * the message's control length is answered 0 and its flags 0. */
int32_t SystemNative_ReceiveMessage(intptr_t socket, MessageHeader* messageHeader, int32_t flags, int64_t* received)
{
    rhu_pal_messageheader *m = (rhu_pal_messageheader *)messageHeader;
    if (m == NULL || received == NULL || m->SocketAddressLen < 0 || m->ControlBufferLen < 0 || m->IOVectorCount < 0 || (m->IOVectorCount > 0 && m->IOVectors == NULL))
        return RHU_PAL_EFAULT;
    *received = 0;
    int32_t err = RHU_PAL_SUCCESS;
    sock_fd *s = sock_of(socket, &err);
    if (s == NULL)
        return err;
    uint32_t kflags = 0;
    int dont_wait = 0;
    if (!flags_of(flags, 1, &kflags, &dont_wait))
        return RHU_PAL_ENOTSUP;
    rhu_net_addr from;
    memset(&from, 0, sizeof from);
    int64_t done = 0;
    if (s->type == RHU_NET_DATAGRAM) {
        int32_t rc = recv_datagram(s, m, kflags, dont_wait, &from, &err);
        if (rc < 0)
            return err;
        done = rc;
    } else {
        for (int32_t i = 0; i < m->IOVectorCount; i++) {
            const rhu_pal_iovector *v = &m->IOVectors[i];
            if (v->Count == 0)
                continue;
            int32_t rc = recv_once(s, v->Base, (uint32_t)v->Count, kflags, dont_wait || done > 0, NULL);
            if (rc < 0) {
                if (done > 0)
                    break;
                return rhu_sock_error(rc);
            }
            done += rc;
            if ((uint32_t)rc < v->Count || (kflags & RHU_NET_PEEK) != 0)
                break;
        }
    }
    if (m->SocketAddress != NULL && m->SocketAddressLen > 0 && from.family != 0) {
        int32_t len = m->SocketAddressLen;
        err = rhu_sock_addr_from_kernel(&from, m->SocketAddress, &len);
        if (err != RHU_PAL_SUCCESS)
            return err;
        m->SocketAddressLen = len;
    } else {
        m->SocketAddressLen = 0;
    }
    m->ControlBufferLen = 0;
    m->Flags = 0;
    *received = done;
    return RHU_PAL_SUCCESS;
}

/* ------------------------------------------------------------------ */
/* the options                                                         */
/* ------------------------------------------------------------------ */

enum { OPT_NONE = 0, OPT_KERNEL, OPT_KERNEL_INVERSE, OPT_TYPE, OPT_LISTENING, OPT_SEND_BUFFER, OPT_RECEIVE_BUFFER };

/* What a named option is here, and for the kernel's three its number. */
static int option_of(int32_t level, int32_t name, uint32_t *kernel_option)
{
    *kernel_option = 0;
    if (level == RHU_PAL_SocketOptionLevel_SOL_SOCKET) {
        switch (name) {
        case RHU_PAL_SocketOptionName_SO_REUSEADDR:
            *kernel_option = RHU_NET_OPT_REUSEADDR;
            return OPT_KERNEL;
        case RHU_PAL_SocketOptionName_SO_EXCLUSIVEADDRUSE:
            *kernel_option = RHU_NET_OPT_REUSEADDR;
            return OPT_KERNEL_INVERSE;
        case RHU_PAL_SocketOptionName_SO_KEEPALIVE:
            *kernel_option = RHU_NET_OPT_KEEPALIVE;
            return OPT_KERNEL;
        case RHU_PAL_SocketOptionName_SO_TYPE:
            return OPT_TYPE;
        case RHU_PAL_SocketOptionName_SO_ACCEPTCONN:
            return OPT_LISTENING;
        case RHU_PAL_SocketOptionName_SO_SNDBUF:
            return OPT_SEND_BUFFER;
        case RHU_PAL_SocketOptionName_SO_RCVBUF:
            return OPT_RECEIVE_BUFFER;
        default:
            return OPT_NONE;
        }
    }
    if (level == RHU_PAL_SocketOptionLevel_SOL_TCP && name == RHU_PAL_SocketOptionName_SO_TCP_NODELAY) {
        *kernel_option = RHU_NET_OPT_NODELAY;
        return OPT_KERNEL;
    }
    return OPT_NONE;
}

/* An option read: one 32-bit value. Exclusive address use is the inverse of
 * address reuse, as the runtime pairs the two. */
int32_t SystemNative_GetSockOpt(intptr_t socket, int32_t socketOptionLevel, int32_t socketOptionName, uint8_t* optionValue, int32_t* optionLen)
{
    if (optionLen == NULL || *optionLen < 0)
        return RHU_PAL_EFAULT;
    int32_t err = RHU_PAL_SUCCESS;
    sock_fd *s = sock_of(socket, &err);
    if (s == NULL)
        return err;
    uint32_t kernel_option = 0;
    int what = option_of(socketOptionLevel, socketOptionName, &kernel_option);
#ifndef RHU_SHIM_BREAK_SOCK_OPTION_REFUSAL
    if (what == OPT_NONE)
        return RHU_PAL_ENOPROTOOPT;
#endif
    int32_t value = 0;
    if (optionValue == NULL || *optionLen < (int32_t)sizeof value)
        return RHU_PAL_EINVAL;
    if (what == OPT_KERNEL || what == OPT_KERNEL_INVERSE) {
        int32_t rc = sock_k()->option(s->handle, kernel_option, 0, 0);
        if (rc < 0)
            return rhu_sock_error(rc);
        value = rc != 0 ? 1 : 0;
        if (what == OPT_KERNEL_INVERSE)
            value = !value;
    } else if (what == OPT_TYPE) {
        value = pal_type(s);
    } else if (what == OPT_LISTENING) {
        value = s->listening;
    } else if (what == OPT_SEND_BUFFER || what == OPT_RECEIVE_BUFFER) {
        rhu_net_limits_desc limits;
        int32_t rc = sock_k()->limits(&limits);
        if (rc < 0)
            return rhu_sock_error(rc);
        if (s->type == RHU_NET_STREAM)
            value = (int32_t)(what == OPT_SEND_BUFFER ? limits.stream_tx : limits.stream_rx);
        else
            value = (int32_t)limits.datagram_buf;
    }
    memcpy(optionValue, &value, sizeof value);
    *optionLen = (int32_t)sizeof value;
    return RHU_PAL_SUCCESS;
}

/* An option written. The kernel's three go to the kernel; a buffer size is
 * the board's, so a write to one is answered as success and changes
 * nothing; the type and the listening state are read-only facts. */
int32_t SystemNative_SetSockOpt(intptr_t socket, int32_t socketOptionLevel, int32_t socketOptionName, uint8_t* optionValue, int32_t optionLen)
{
    int32_t err = RHU_PAL_SUCCESS;
    sock_fd *s = sock_of(socket, &err);
    if (s == NULL)
        return err;
    uint32_t kernel_option = 0;
    int what = option_of(socketOptionLevel, socketOptionName, &kernel_option);
    if (what == OPT_NONE || what == OPT_TYPE || what == OPT_LISTENING) {
#ifdef RHU_SHIM_BREAK_SOCK_OPTION_REFUSAL
        return RHU_PAL_SUCCESS;
#else
        return RHU_PAL_ENOPROTOOPT;
#endif
    }
    int32_t value = 0;
    if (optionValue == NULL || optionLen < (int32_t)sizeof value)
        return RHU_PAL_EINVAL;
    memcpy(&value, optionValue, sizeof value);
    if (what == OPT_SEND_BUFFER || what == OPT_RECEIVE_BUFFER)
        return RHU_PAL_SUCCESS;
    if (what == OPT_KERNEL_INVERSE) {
        if (value != 0 && value != 1)
            return RHU_PAL_EINVAL;
        value = !value;
    }
    int32_t rc = sock_k()->option(s->handle, kernel_option, 1, value != 0 ? 1 : 0);
    return rc < 0 ? rhu_sock_error(rc) : RHU_PAL_SUCCESS;
}

int32_t SystemNative_GetLingerOption(intptr_t socket, LingerOption* option)
{
    rhu_pal_lingeroption *o = (rhu_pal_lingeroption *)option;
    if (o == NULL)
        return RHU_PAL_EFAULT;
    int32_t err = RHU_PAL_SUCCESS;
    sock_fd *s = sock_of(socket, &err);
    if (s == NULL)
        return err;
    o->OnOff = s->linger_on;
    o->Seconds = s->linger_seconds;
    return RHU_PAL_SUCCESS;
}

/* The linger record is kept and read back. The kernel's close never waits,
 * so the record changes no close. */
int32_t SystemNative_SetLingerOption(intptr_t socket, LingerOption* option)
{
    const rhu_pal_lingeroption *o = (const rhu_pal_lingeroption *)option;
    if (o == NULL)
        return RHU_PAL_EFAULT;
    if (o->OnOff != 0 && (o->Seconds < 0 || o->Seconds > SOCK_LINGER_MAX))
        return RHU_PAL_EINVAL;
    int32_t err = RHU_PAL_SUCCESS;
    sock_fd *s = sock_of(socket, &err);
    if (s == NULL)
        return err;
#ifdef RHU_SHIM_BREAK_SOCK_CLOSE_LINGER
    return RHU_PAL_ENOTSUP;
#else
    s->linger_on = o->OnOff != 0 ? 1 : 0;
    if (o->OnOff != 0)
        s->linger_seconds = o->Seconds;
    return RHU_PAL_SUCCESS;
#endif
}

/* The bound of a blocking receive and accept, in milliseconds; 0 is no bound. */
int32_t SystemNative_SetReceiveTimeout(intptr_t socket, int32_t millisecondsTimeout)
{
    if (millisecondsTimeout < 0)
        return RHU_PAL_EINVAL;
    int32_t err = RHU_PAL_SUCCESS;
    sock_fd *s = sock_of(socket, &err);
    if (s == NULL)
        return err;
    s->recv_timeout_ms = millisecondsTimeout;
    return RHU_PAL_SUCCESS;
}

/* The bound of a blocking send and connect, in milliseconds; 0 is no bound. */
int32_t SystemNative_SetSendTimeout(intptr_t socket, int32_t millisecondsTimeout)
{
    if (millisecondsTimeout < 0)
        return RHU_PAL_EINVAL;
    int32_t err = RHU_PAL_SUCCESS;
    sock_fd *s = sock_of(socket, &err);
    if (s == NULL)
        return err;
    s->send_timeout_ms = millisecondsTimeout;
    return RHU_PAL_SUCCESS;
}

/* The socket's pending error as the runtime's, read once and cleared; the
 * runtime's success for none. */
int32_t SystemNative_GetSocketErrorOption(intptr_t socket, int32_t* error)
{
    if (error == NULL)
        return RHU_PAL_EFAULT;
    int32_t err = RHU_PAL_SUCCESS;
    sock_fd *s = sock_of(socket, &err);
    if (s == NULL)
        return err;
    int32_t pending = sock_pending(s);
    *error = pending == 0 ? RHU_PAL_SUCCESS : rhu_sock_error(pending);
    return RHU_PAL_SUCCESS;
}

/* ------------------------------------------------------------------ */
/* the event port                                                      */
/* ------------------------------------------------------------------ */

/* The open port behind a handle: its number, 1 .. SOCK_PORTS; 0 for a
 * value that is no open port's handle. */
static uint8_t port_of(intptr_t port)
{
    intptr_t n = port - SOCK_PORT_TAG;
    if (n < 1 || n > SOCK_PORTS || __atomic_load_n(&g_ports[n - 1], __ATOMIC_SEQ_CST) == 0)
        return 0;
    return (uint8_t)n;
}

int32_t SystemNative_CreateSocketEventPort(intptr_t* port)
{
    if (port == NULL)
        return RHU_PAL_EFAULT;
    for (intptr_t n = 1; n <= SOCK_PORTS; n++) {
        uint8_t expect = 0;
        if (__atomic_compare_exchange_n(&g_ports[n - 1], &expect, (uint8_t)1, 0, __ATOMIC_SEQ_CST, __ATOMIC_SEQ_CST)) {
            *port = SOCK_PORT_TAG + n;
            return RHU_PAL_SUCCESS;
        }
    }
    *port = -1;
    return RHU_PAL_EMFILE;
}

/* The port closes: its registrations end, and a wait asleep on it, woken
 * by the word's move, finds the port gone. */
int32_t SystemNative_CloseSocketEventPort(intptr_t port)
{
    port_lock();
    uint8_t n = port_of(port);
    if (n != 0) {
        for (uint32_t i = 0; i < RHU_SHIM_FD_SOCKET_COUNT; i++) {
            if (g_socks[i].port == n)
                g_socks[i].port = 0;
        }
        __atomic_store_n(&g_ports[n - 1], (uint8_t)0, __ATOMIC_SEQ_CST);
    }
    port_unlock();
    if (n == 0)
        return RHU_PAL_EBADF;
    sock_k()->events_bump();
    return RHU_PAL_SUCCESS;
}

/* Room for the events of one wait. A wait answers at most one event for
 * each descriptor of the band, so the room is the caller's count or the
 * band's, whichever is smaller. */
int32_t SystemNative_CreateSocketEventBuffer(int32_t count, SocketEvent** buffer)
{
    if (buffer == NULL || count < 0)
        return RHU_PAL_EFAULT;
    uint32_t room = count < RHU_SHIM_FD_SOCKET_COUNT ? (uint32_t)count : (uint32_t)RHU_SHIM_FD_SOCKET_COUNT;
    void *p = malloc((room != 0 ? room : 1u) * sizeof(rhu_pal_socketevent));
    if (p == NULL)
        return RHU_PAL_ENOMEM;
    *buffer = (SocketEvent *)p;
    return RHU_PAL_SUCCESS;
}

int32_t SystemNative_FreeSocketEventBuffer(SocketEvent* buffer)
{
    free(buffer);
    return RHU_PAL_SUCCESS;
}

/* A socket's registration with a port: made, changed or removed. A new or
 * changed registration is marked fresh, so the next wait reports the
 * socket's level once, as an edge-triggered port reports a state that
 * stood before the registration. The word every socket's change moves is
 * moved last, so a wait asleep over the old registrations scans the new. */
int32_t SystemNative_TryChangeSocketEventRegistration(intptr_t port, intptr_t socket, int32_t currentEvents, int32_t newEvents, uintptr_t data)
{
    const int32_t served = RHU_PAL_SocketEvents_SA_READ | RHU_PAL_SocketEvents_SA_WRITE | RHU_PAL_SocketEvents_SA_READCLOSE
                           | RHU_PAL_SocketEvents_SA_CLOSE | RHU_PAL_SocketEvents_SA_ERROR;
    if ((currentEvents & ~served) != 0 || (newEvents & ~served) != 0)
        return RHU_PAL_EINVAL;
    if (currentEvents == newEvents)
        return RHU_PAL_SUCCESS;
    int32_t err = RHU_PAL_SUCCESS;
    port_lock();
    uint8_t n = port_of(port);
    sock_fd *s = n != 0 ? sock_of(socket, &err) : NULL;
    if (n == 0)
        err = RHU_PAL_EBADF;
    else if (s != NULL && currentEvents == RHU_PAL_SocketEvents_SA_NONE && s->port != 0)
        err = RHU_PAL_EEXIST;
    else if (s != NULL && currentEvents != RHU_PAL_SocketEvents_SA_NONE && s->port != n)
        err = RHU_PAL_ENOENT;
    if (err == RHU_PAL_SUCCESS) {
        if (newEvents == RHU_PAL_SocketEvents_SA_NONE) {
            s->port = 0;
        } else {
            s->port_events = newEvents;
            s->port_data = data;
            s->port_fresh = 1;
            s->port = n;
        }
    }
    port_unlock();
    if (err != RHU_PAL_SUCCESS)
        return err;
#ifndef RHU_SHIM_BREAK_SOCK_PORT_LOST_REGISTRATION
    sock_k()->events_bump();
#endif
    return RHU_PAL_SUCCESS;
}

typedef struct port_scan {
    const rhu_sock_kernel *k;
    uint8_t port;
    rhu_pal_socketevent *events;
    int32_t room;
    int32_t count;
} port_scan;

/* One pass over the sockets registered with the port: each one's new bits
 * as an event, up to the room. A socket's first pass after its
 * registration drops the bits taken and reports its level. A full buffer
 * ends the pass before the next socket's bits are taken, so they stand for
 * the next wait. The answer is the count of events, or the kernel's
 * bad-descriptor code for a port that closed. */
static int32_t scan_port(void *context)
{
    port_scan *p = (port_scan *)context;
    p->count = 0;
    port_lock();
    if (__atomic_load_n(&g_ports[p->port - 1], __ATOMIC_SEQ_CST) == 0) {
        port_unlock();
        return RHU_EBADF;
    }
    for (uint32_t i = 0; i < RHU_SHIM_FD_SOCKET_COUNT && p->count < p->room; i++) {
        sock_fd *s = &g_socks[i];
        if (s->port != p->port || __atomic_load_n(&s->state, __ATOMIC_SEQ_CST) != SLOT_OPEN)
            continue;
#ifdef RHU_SHIM_BREAK_SOCK_PORT_LEVEL
        int32_t bits = p->k->readiness(s->handle, 0);
        s->port_fresh = 0;
#else
        int32_t bits = p->k->readiness(s->handle, 1);
        if (s->port_fresh) {
            s->port_fresh = 0;
            bits = p->k->readiness(s->handle, 0);
        }
#endif
        if (bits <= 0)
            continue;
        int32_t events = 0;
        if ((bits & RHU_NET_READABLE) != 0 && (s->port_events & RHU_PAL_SocketEvents_SA_READ) != 0)
            events |= RHU_PAL_SocketEvents_SA_READ;
        if ((bits & RHU_NET_WRITABLE) != 0 && (s->port_events & RHU_PAL_SocketEvents_SA_WRITE) != 0)
            events |= RHU_PAL_SocketEvents_SA_WRITE;
        if ((bits & RHU_NET_HANGUP) != 0)
            events |= RHU_PAL_SocketEvents_SA_READ | RHU_PAL_SocketEvents_SA_WRITE;
        if ((bits & RHU_NET_ERROR) != 0)
            events |= RHU_PAL_SocketEvents_SA_ERROR;
        if (events == 0)
            continue;
        rhu_pal_socketevent *e = &p->events[p->count++];
        e->Data = s->port_data;
        e->Events = events;
        e->Padding = 0;
    }
    port_unlock();
    return p->count;
}

/* The wait: the port's sockets scanned, and with nothing to report a sleep
 * on the word every socket's change moves, then the scan again. The count
 * answered is one or more. */
int32_t SystemNative_WaitForSocketEvents(intptr_t port, SocketEvent* buffer, int32_t* count)
{
    if (buffer == NULL || count == NULL || *count < 0)
        return RHU_PAL_EFAULT;
    port_scan p = { sock_k(), port_of(port), (rhu_pal_socketevent *)buffer, *count < RHU_SHIM_FD_SOCKET_COUNT ? *count : RHU_SHIM_FD_SOCKET_COUNT, 0 };
    *count = 0;
    if (p.port == 0)
        return RHU_PAL_EBADF;
    if (p.room == 0)
        return RHU_PAL_EINVAL;
    int32_t w = rhu_sock_wait(p.k->events_word(), scan_port, &p, RHU_DEADLINE_NONE);
    if (w < 0)
        return rhu_sock_error(w);
    *count = w;
    return RHU_PAL_SUCCESS;
}

/* ------------------------------------------------------------------ */
/* the band's descriptors as the descriptor layer hands them on        */
/* ------------------------------------------------------------------ */

/* -1 with the platform's errno for one of the runtime's errors. */
static int32_t fd_refuse(int32_t pal_error)
{
    ERRNO = SystemNative_ConvertErrorPalToPlatform(pal_error);
    return -1;
}

/* The socket leaves the band and the kernel releases its handle. Its
 * registration with an event port ends first, under the port lock, so no
 * wait takes the bits of a handle the kernel has released; the word's move
 * then lets a wait asleep on the port scan without it. */
static int32_t sock_fd_close(intptr_t fd)
{
    int32_t err = RHU_PAL_SUCCESS;
    sock_fd *s = sock_of(fd, &err);
    uint8_t expect = SLOT_OPEN;
    if (s == NULL || !__atomic_compare_exchange_n(&s->state, &expect, (uint8_t)SLOT_HELD, 0, __ATOMIC_SEQ_CST, __ATOMIC_SEQ_CST))
        return fd_refuse(RHU_PAL_EBADF);
    uint8_t registered = 0;
    port_lock();
#ifndef RHU_SHIM_BREAK_SOCK_PORT_CLOSE_REGISTRATION
    registered = s->port;
    s->port = 0;
#endif
    int32_t rc = sock_k()->close(s->handle);
    port_unlock();
    slot_free(s);
    if (registered != 0)
        sock_k()->events_bump();
    return rc < 0 ? fd_refuse(rhu_sock_error(rc)) : 0;
}

static int32_t sock_fd_read(intptr_t fd, void *buffer, int32_t count)
{
    int32_t err = RHU_PAL_SUCCESS;
    sock_fd *s = sock_of(fd, &err);
    if (s == NULL)
        return fd_refuse(RHU_PAL_EBADF);
    if (count < 0)
        return fd_refuse(RHU_PAL_EINVAL);
    if (buffer == NULL)
        return fd_refuse(RHU_PAL_EFAULT);
    int32_t rc = recv_once(s, buffer, (uint32_t)count, 0, 0, NULL);
    return rc < 0 ? fd_refuse(rhu_sock_error(rc)) : rc;
}

static int32_t sock_fd_write(intptr_t fd, const void *buffer, int32_t count)
{
    int32_t err = RHU_PAL_SUCCESS;
    sock_fd *s = sock_of(fd, &err);
    if (s == NULL)
        return fd_refuse(RHU_PAL_EBADF);
    if (count < 0)
        return fd_refuse(RHU_PAL_EINVAL);
    if (buffer == NULL)
        return fd_refuse(RHU_PAL_EFAULT);
    int32_t rc = send_once(s, buffer, (uint32_t)count, 0, NULL);
    return rc < 0 ? fd_refuse(rhu_sock_error(rc)) : rc;
}

/* A socket's status: its mode says socket, and nothing else is told. */
static int32_t sock_fd_fstat(intptr_t fd, struct rhu_pal_filestatus *output)
{
    int32_t err = RHU_PAL_SUCCESS;
    memset(output, 0, sizeof *output);
    if (sock_of(fd, &err) == NULL)
        return fd_refuse(RHU_PAL_EBADF);
    output->Mode = (int32_t)(RHU_PAL_S_IFSOCK | 0x1FFu);
    output->HardLinkCount = 1;
    return 0;
}

static int32_t sock_fd_get_nonblocking(intptr_t fd, int32_t *is_nonblocking)
{
    int32_t err = RHU_PAL_SUCCESS;
    sock_fd *s = sock_of(fd, &err);
    if (s == NULL)
        return fd_refuse(RHU_PAL_EBADF);
    *is_nonblocking = s->nonblocking;
    return 0;
}

static int32_t sock_fd_set_nonblocking(intptr_t fd, int32_t is_nonblocking)
{
    int32_t err = RHU_PAL_SUCCESS;
    sock_fd *s = sock_of(fd, &err);
    if (s == NULL)
        return fd_refuse(RHU_PAL_EBADF);
    s->nonblocking = is_nonblocking != 0 ? 1u : 0u;
    return 0;
}

/* A socket carries the close-on-exec bit from its start (the value 1 is the
 * glibc personality's FD_CLOEXEC). */
static int32_t sock_fd_get_fd_flags(intptr_t fd)
{
    int32_t err = RHU_PAL_SUCCESS;
    if (sock_of(fd, &err) == NULL)
        return fd_refuse(RHU_PAL_EBADF);
    return 1;
}

/* An open socket's level as the runtime's poll events: readable and
 * writable where the caller asked for them, an error and a hang-up always;
 * the invalid event for a descriptor that is not open. */
static int32_t sock_fd_poll_events(intptr_t fd, int32_t wanted)
{
    int32_t err = RHU_PAL_SUCCESS;
    sock_fd *s = sock_of(fd, &err);
    if (s == NULL)
        return RHU_PAL_POLLNVAL;
#ifdef RHU_SHIM_BREAK_SOCK_POLL_LEVEL
    int32_t level = sock_k()->readiness(s->handle, 1);
#else
    int32_t level = sock_k()->readiness(s->handle, 0);
#endif
    if (level < 0)
        return RHU_PAL_POLLNVAL;
    int32_t events = 0;
    if ((level & RHU_NET_READABLE) != 0)
        events |= RHU_PAL_POLLIN;
    if ((level & RHU_NET_WRITABLE) != 0)
        events |= RHU_PAL_POLLOUT;
    events &= wanted;
    if ((level & RHU_NET_ERROR) != 0)
        events |= RHU_PAL_POLLERR;
    if ((level & RHU_NET_HANGUP) != 0)
        events |= RHU_PAL_POLLHUP;
    return events;
}

/* Poll's wait: the one wait, on the word every socket's change moves. The
 * scan's first answer that is not 0, or 0 when the deadline passes first. */
static int32_t sock_fd_poll_wait(rhu_sock_scan scan, void *context, uint64_t deadline_ns)
{
    int32_t w = rhu_sock_wait(sock_k()->events_word(), scan, context, deadline_ns);
    if (w == RHU_ETIMEDOUT)
        return 0;
    return w < 0 ? fd_refuse(rhu_sock_error(w)) : w;
}

static const rhu_sock_fd_verbs g_sock_fd_verbs = {
    sock_fd_close, sock_fd_read, sock_fd_write, sock_fd_fstat, sock_fd_get_nonblocking, sock_fd_set_nonblocking, sock_fd_get_fd_flags,
    sock_fd_poll_events, sock_fd_poll_wait,
};

const rhu_sock_fd_verbs *rhu_sock_fd_hook(void)
{
    return &g_sock_fd_verbs;
}
