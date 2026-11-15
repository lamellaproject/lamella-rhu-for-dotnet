/* The socket-address helpers of the runtime's portability layer, and the
 * table that turns a kernel network code into the runtime's error.
 *
 * The managed socket classes never read a socket address's bytes
 * themselves. They ask this layer for the record sizes once, allocate a
 * buffer of that size, and read and write every field through the functions
 * below: the family, the port, the IPv4 address, the IPv6 address and its
 * scope. The layout those functions keep is therefore this unit's own to
 * state, and it states the Linux one (the generated header's sizes and
 * offsets: a two-byte family at the record's start, the port in network
 * byte order after it, the address after the port), so a buffer a program
 * or a debugger looks at reads as it does on Linux. The family a caller
 * names is the runtime's number and the family in the buffer is the
 * platform's; the two sets differ and the table below pairs them.
 *
 * Every function here answers from its arguments alone: none calls the
 * kernel and none keeps state. An IPv6 or a Unix-domain endpoint serializes
 * through them like an IPv4 one; whether a socket of a family can be made
 * is the socket function's answer, not this unit's.
 *
 * Two answers about a message's control buffer stand beside the helpers
 * because they too need nothing but their arguments: the buffer's size is
 * the room Linux gives one packet-information record of each family asked
 * for, and no packet information is ever found in a message, since none is
 * written into one.
 *
 * This file is an implementation written from scratch that presents the
 * runtime's own entry points; the signatures are the generated surface
 * header's, read off the runtime's headers. */
#include <stddef.h>
#include <stdint.h>

#include "rhu.h"
#include "rhu_glibc_arm32.h"
#include "rhu_pal_errno_net11.h"
#include "rhu_pal_networking_net11.h"
#include "rhu_pal_surface_net11.h"
#include "rhu_sock.h"

void *memcpy(void *dst, const void *src, size_t n);
void *memset(void *dst, int c, size_t n);

_Static_assert(RHU_GLIBC_SA_FAMILY_SIZE == sizeof(uint16_t), "the family field is two bytes");
_Static_assert(RHU_PAL_NUM_BYTES_IN_IPV4_ADDRESS == sizeof(uint32_t), "an IPv4 address is one 32-bit word");

/* The runtime's family numbers beside the platform's. A family in neither
 * column has no record here: written, it is refused; read, it is unknown. */
static const struct {
    int32_t pal;
    uint16_t platform;
} g_families[] = {
    { RHU_PAL_AddressFamily_AF_UNSPEC, RHU_GLIBC_AF_UNSPEC },
    { RHU_PAL_AddressFamily_AF_UNIX, RHU_GLIBC_AF_UNIX },
    { RHU_PAL_AddressFamily_AF_INET, RHU_GLIBC_AF_INET },
    { RHU_PAL_AddressFamily_AF_INET6, RHU_GLIBC_AF_INET6 },
    { RHU_PAL_AddressFamily_AF_PACKET, RHU_GLIBC_AF_PACKET },
    { RHU_PAL_AddressFamily_AF_CAN, RHU_GLIBC_AF_CAN },
};
#define FAMILIES (sizeof g_families / sizeof g_families[0])

/* Whether the caller's buffer holds the family field. */
static int holds_family(const uint8_t *address, int32_t len)
{
    return address != NULL && len >= RHU_GLIBC_SA_FAMILY_OFFSET + RHU_GLIBC_SA_FAMILY_SIZE;
}

static uint16_t family_of(const uint8_t *address)
{
    uint16_t family;
    memcpy(&family, address + RHU_GLIBC_SA_FAMILY_OFFSET, sizeof family);
    return family;
}

/* The port's two bytes at an offset: the high byte first. */
static uint16_t port_at(const uint8_t *address, uint32_t offset)
{
#ifdef RHU_SHIM_BREAK_SOCK_PORT_ORDER
    uint16_t port;
    memcpy(&port, address + offset, sizeof port);
    return port;
#else
    return (uint16_t)((uint16_t)(address[offset] << 8) | address[offset + 1u]);
#endif
}

static void set_port_at(uint8_t *address, uint32_t offset, uint16_t port)
{
#ifdef RHU_SHIM_BREAK_SOCK_PORT_ORDER
    memcpy(address + offset, &port, sizeof port);
#else
    address[offset] = (uint8_t)(port >> 8);
    address[offset + 1u] = (uint8_t)port;
#endif
}

int32_t SystemNative_GetSocketAddressSizes(int32_t* ipv4SocketAddressSize, int32_t* ipv6SocketAddressSize, int32_t* udsSocketAddressSize, int32_t* maxSocketAddressSize)
{
    if (ipv4SocketAddressSize == NULL || ipv6SocketAddressSize == NULL || udsSocketAddressSize == NULL || maxSocketAddressSize == NULL)
        return RHU_PAL_EFAULT;
    *ipv4SocketAddressSize = RHU_GLIBC_SOCKADDR_IN_SIZE;
    *ipv6SocketAddressSize = RHU_GLIBC_SOCKADDR_IN6_SIZE;
    *udsSocketAddressSize = RHU_GLIBC_SOCKADDR_UN_SIZE;
    *maxSocketAddressSize = RHU_GLIBC_SOCKADDR_STORAGE_SIZE;
    return RHU_PAL_SUCCESS;
}

/* The largest record: the size the managed side gives a buffer that must
 * hold an address of any family. */
int32_t SystemNative_GetMaximumAddressSize(void)
{
#ifdef RHU_SHIM_BREAK_SOCK_SIZE
    return 0;
#else
    return RHU_GLIBC_SOCKADDR_STORAGE_SIZE;
#endif
}

/* The Unix-domain record: where its path begins, the path's room and the
 * record's size. */
void SystemNative_GetDomainSocketSizes(int32_t* pathOffset, int32_t* pathSize, int32_t* addressSize)
{
    if (pathOffset != NULL)
        *pathOffset = RHU_GLIBC_SUN_PATH_OFFSET;
    if (pathSize != NULL)
        *pathSize = RHU_GLIBC_SUN_PATH_SIZE;
    if (addressSize != NULL)
        *addressSize = RHU_GLIBC_SOCKADDR_UN_SIZE;
}

/* The family of the caller's record as the runtime numbers it; a number the
 * runtime has no name for reads as unknown, which is a success. */
int32_t SystemNative_GetAddressFamily(const uint8_t* socketAddress, int32_t socketAddressLen, int32_t* addressFamily)
{
    if (addressFamily == NULL || !holds_family(socketAddress, socketAddressLen))
        return RHU_PAL_EFAULT;
    uint16_t platform = family_of(socketAddress);
    *addressFamily = RHU_PAL_AddressFamily_AF_UNKNOWN;
    for (uint32_t i = 0; i < FAMILIES; i++) {
        if (g_families[i].platform == platform)
            *addressFamily = g_families[i].pal;
    }
    return RHU_PAL_SUCCESS;
}

/* The family written as the platform numbers it. A family the platform has
 * no number for is refused and the buffer is left as it was. */
int32_t SystemNative_SetAddressFamily(uint8_t* socketAddress, int32_t socketAddressLen, int32_t addressFamily)
{
    if (!holds_family(socketAddress, socketAddressLen))
        return RHU_PAL_EFAULT;
    for (uint32_t i = 0; i < FAMILIES; i++) {
        if (g_families[i].pal == addressFamily) {
            uint16_t platform = g_families[i].platform;
            memcpy(socketAddress + RHU_GLIBC_SA_FAMILY_OFFSET, &platform, sizeof platform);
            return RHU_PAL_SUCCESS;
        }
    }
    return RHU_PAL_EAFNOSUPPORT;
}

/* Where the port lies in the caller's record, by the record's own family:
 * 0 with the offset, or the error -- a buffer that does not hold the family
 * or the whole record, or a family that carries no port. */
static int32_t port_offset(const uint8_t *address, int32_t len, uint32_t *offset)
{
    if (!holds_family(address, len))
        return RHU_PAL_EFAULT;
    switch (family_of(address)) {
    case RHU_GLIBC_AF_INET:
        if (len < RHU_GLIBC_SOCKADDR_IN_SIZE)
            return RHU_PAL_EFAULT;
        *offset = RHU_GLIBC_SIN_PORT_OFFSET;
        return RHU_PAL_SUCCESS;
    case RHU_GLIBC_AF_INET6:
        if (len < RHU_GLIBC_SOCKADDR_IN6_SIZE)
            return RHU_PAL_EFAULT;
        *offset = RHU_GLIBC_SIN6_PORT_OFFSET;
        return RHU_PAL_SUCCESS;
    default:
        return RHU_PAL_EAFNOSUPPORT;
    }
}

int32_t SystemNative_GetPort(const uint8_t* socketAddress, int32_t socketAddressLen, uint16_t* port)
{
    uint32_t offset = 0;
    if (port == NULL)
        return RHU_PAL_EFAULT;
    int32_t err = port_offset(socketAddress, socketAddressLen, &offset);
    if (err != RHU_PAL_SUCCESS)
        return err;
    *port = port_at(socketAddress, offset);
    return RHU_PAL_SUCCESS;
}

int32_t SystemNative_SetPort(uint8_t* socketAddress, int32_t socketAddressLen, uint16_t port)
{
    uint32_t offset = 0;
    int32_t err = port_offset(socketAddress, socketAddressLen, &offset);
    if (err != RHU_PAL_SUCCESS)
        return err;
    set_port_at(socketAddress, offset, port);
    return RHU_PAL_SUCCESS;
}

/* Whether the caller's buffer holds a whole record of one family: 0, the
 * fault for a buffer too short for it, or the invalid argument for a record
 * of another family. */
static int32_t record_of(const uint8_t *address, int32_t len, uint16_t family, int32_t size)
{
    if (address == NULL || len < size)
        return RHU_PAL_EFAULT;
    return family_of(address) == family ? RHU_PAL_SUCCESS : RHU_PAL_EINVAL;
}

/* The IPv4 address is the record's four bytes as they lie: the managed side
 * passes and takes them in one word in the same order. */
int32_t SystemNative_GetIPv4Address(const uint8_t* socketAddress, int32_t socketAddressLen, uint32_t* address)
{
    if (address == NULL)
        return RHU_PAL_EFAULT;
    int32_t err = record_of(socketAddress, socketAddressLen, RHU_GLIBC_AF_INET, RHU_GLIBC_SOCKADDR_IN_SIZE);
    if (err != RHU_PAL_SUCCESS)
        return err;
    memcpy(address, socketAddress + RHU_GLIBC_SIN_ADDR_OFFSET, sizeof *address);
    return RHU_PAL_SUCCESS;
}

int32_t SystemNative_SetIPv4Address(uint8_t* socketAddress, int32_t socketAddressLen, uint32_t address)
{
    int32_t err = record_of(socketAddress, socketAddressLen, RHU_GLIBC_AF_INET, RHU_GLIBC_SOCKADDR_IN_SIZE);
    if (err != RHU_PAL_SUCCESS)
        return err;
    memcpy(socketAddress + RHU_GLIBC_SIN_ADDR_OFFSET, &address, sizeof address);
    return RHU_PAL_SUCCESS;
}

/* The IPv6 address: sixteen bytes and the scope. A write clears the flow
 * word, which the managed side does not carry. */
int32_t SystemNative_GetIPv6Address(const uint8_t* socketAddress, int32_t socketAddressLen, uint8_t* address, int32_t addressLen, uint32_t* scopeId)
{
    if (address == NULL || scopeId == NULL || addressLen < RHU_PAL_NUM_BYTES_IN_IPV6_ADDRESS)
        return RHU_PAL_EFAULT;
    int32_t err = record_of(socketAddress, socketAddressLen, RHU_GLIBC_AF_INET6, RHU_GLIBC_SOCKADDR_IN6_SIZE);
    if (err != RHU_PAL_SUCCESS)
        return err;
    memcpy(address, socketAddress + RHU_GLIBC_SIN6_ADDR_OFFSET, RHU_PAL_NUM_BYTES_IN_IPV6_ADDRESS);
    memcpy(scopeId, socketAddress + RHU_GLIBC_SIN6_SCOPE_ID_OFFSET, sizeof *scopeId);
    return RHU_PAL_SUCCESS;
}

int32_t SystemNative_SetIPv6Address(uint8_t* socketAddress, int32_t socketAddressLen, uint8_t* address, int32_t addressLen, uint32_t scopeId)
{
    if (address == NULL || addressLen < RHU_PAL_NUM_BYTES_IN_IPV6_ADDRESS)
        return RHU_PAL_EFAULT;
    int32_t err = record_of(socketAddress, socketAddressLen, RHU_GLIBC_AF_INET6, RHU_GLIBC_SOCKADDR_IN6_SIZE);
    if (err != RHU_PAL_SUCCESS)
        return err;
    memset(socketAddress + RHU_GLIBC_SIN6_FLOWINFO_OFFSET, 0, sizeof(uint32_t));
    memcpy(socketAddress + RHU_GLIBC_SIN6_ADDR_OFFSET, address, RHU_PAL_NUM_BYTES_IN_IPV6_ADDRESS);
    memcpy(socketAddress + RHU_GLIBC_SIN6_SCOPE_ID_OFFSET, &scopeId, sizeof scopeId);
    return RHU_PAL_SUCCESS;
}

/* A caller's record as the kernel's address: the family as the record
 * numbers it, the port as a number, the address bytes as they lie. */
int32_t rhu_sock_addr_to_kernel(const uint8_t *address, int32_t len, rhu_net_addr *out)
{
    if (out == NULL || !holds_family(address, len))
        return RHU_PAL_EFAULT;
    memset(out, 0, sizeof *out);
    switch (family_of(address)) {
    case RHU_GLIBC_AF_INET:
        if (len < RHU_GLIBC_SOCKADDR_IN_SIZE)
            return RHU_PAL_EFAULT;
        out->family = RHU_GLIBC_AF_INET;
        out->port = port_at(address, RHU_GLIBC_SIN_PORT_OFFSET);
        memcpy(out->addr, address + RHU_GLIBC_SIN_ADDR_OFFSET, RHU_PAL_NUM_BYTES_IN_IPV4_ADDRESS);
        return RHU_PAL_SUCCESS;
    case RHU_GLIBC_AF_INET6:
        if (len < RHU_GLIBC_SOCKADDR_IN6_SIZE)
            return RHU_PAL_EFAULT;
        out->family = RHU_GLIBC_AF_INET6;
        out->port = port_at(address, RHU_GLIBC_SIN6_PORT_OFFSET);
        memcpy(out->addr, address + RHU_GLIBC_SIN6_ADDR_OFFSET, RHU_PAL_NUM_BYTES_IN_IPV6_ADDRESS);
        memcpy(&out->scope, address + RHU_GLIBC_SIN6_SCOPE_ID_OFFSET, sizeof out->scope);
        return RHU_PAL_SUCCESS;
    default:
        return RHU_PAL_EAFNOSUPPORT;
    }
}

/* The kernel's address as the Linux record of its family in the caller's
 * buffer, every byte of the record written. */
int32_t rhu_sock_addr_from_kernel(const rhu_net_addr *in, uint8_t *address, int32_t *len)
{
    if (in == NULL || address == NULL || len == NULL)
        return RHU_PAL_EFAULT;
    uint16_t family = (uint16_t)in->family;
    switch (in->family) {
    case RHU_GLIBC_AF_INET:
        if (*len < RHU_GLIBC_SOCKADDR_IN_SIZE)
            return RHU_PAL_EFAULT;
        memset(address, 0, RHU_GLIBC_SOCKADDR_IN_SIZE);
        memcpy(address + RHU_GLIBC_SA_FAMILY_OFFSET, &family, sizeof family);
        set_port_at(address, RHU_GLIBC_SIN_PORT_OFFSET, (uint16_t)in->port);
        memcpy(address + RHU_GLIBC_SIN_ADDR_OFFSET, in->addr, RHU_PAL_NUM_BYTES_IN_IPV4_ADDRESS);
        *len = RHU_GLIBC_SOCKADDR_IN_SIZE;
        return RHU_PAL_SUCCESS;
    case RHU_GLIBC_AF_INET6:
        if (*len < RHU_GLIBC_SOCKADDR_IN6_SIZE)
            return RHU_PAL_EFAULT;
        memset(address, 0, RHU_GLIBC_SOCKADDR_IN6_SIZE);
        memcpy(address + RHU_GLIBC_SA_FAMILY_OFFSET, &family, sizeof family);
        set_port_at(address, RHU_GLIBC_SIN6_PORT_OFFSET, (uint16_t)in->port);
        memcpy(address + RHU_GLIBC_SIN6_ADDR_OFFSET, in->addr, RHU_PAL_NUM_BYTES_IN_IPV6_ADDRESS);
        memcpy(address + RHU_GLIBC_SIN6_SCOPE_ID_OFFSET, &in->scope, sizeof in->scope);
        *len = RHU_GLIBC_SOCKADDR_IN6_SIZE;
        return RHU_PAL_SUCCESS;
    default:
        return RHU_PAL_EAFNOSUPPORT;
    }
}

/* The room a control buffer needs for one packet-information record of each
 * family the caller asks for. */
int32_t SystemNative_GetControlMessageBufferSize(int32_t isIPv4, int32_t isIPv6)
{
    return (isIPv4 != 0 ? RHU_GLIBC_CMSG_SPACE_IN_PKTINFO : 0) + (isIPv6 != 0 ? RHU_GLIBC_CMSG_SPACE_IN6_PKTINFO : 0);
}

/* No control data is written into a message, so no packet information is
 * found in one: the answer is 0, not found, and the caller's record is left
 * as it was. */
int32_t SystemNative_TryGetIPPacketInformation(MessageHeader* messageHeader, int32_t isIPv4, IPPacketInformation* packetInfo)
{
    (void)messageHeader;
    (void)isIPv4;
    (void)packetInfo;
    return 0;
}

/* The kernel's codes that are the runtime's error of the same name. */
#define SOCK_SAME_NAME(X) \
    X(EAGAIN) X(ETIMEDOUT) X(EINVAL) X(ESRCH) X(EINTR) X(EIO) X(EBUSY) X(EACCES) X(ENOTSUP) \
    X(ENXIO) X(EMFILE) X(EPIPE) X(EMSGSIZE) X(EPROTONOSUPPORT) X(EAFNOSUPPORT) X(EADDRINUSE) \
    X(ENETDOWN) X(ENETUNREACH) X(ECONNABORTED) X(ECONNRESET) X(EISCONN) X(ENOTCONN) \
    X(ECONNREFUSED) X(EHOSTUNREACH) X(EALREADY) X(EINPROGRESS) X(EROFS) X(ENOENT) X(EBADF) \
    X(EEXIST) X(ENOTDIR) X(EISDIR) X(ENOSPC) X(ENAMETOOLONG) X(ENOTEMPTY)

int32_t rhu_sock_error(int32_t kernel_code)
{
    switch (kernel_code) {
#define X(name)      \
    case RHU_##name: \
        return RHU_PAL_##name;
        SOCK_SAME_NAME(X)
#undef X
    case RHU_ENODEV:
        return RHU_PAL_EAFNOSUPPORT;
    default:
#ifdef RHU_SHIM_BREAK_SOCK_ERRMAP
        return RHU_PAL_SUCCESS;
#else
        return RHU_PAL_EIO;
#endif
    }
}
