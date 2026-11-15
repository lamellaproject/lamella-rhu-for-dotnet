/* The network's names and interfaces for the runtime's portability layer,
 * over the kernel's network functions.
 *
 * A host's addresses by name. The kernel's resolver takes a query and
 * answers a handle at once; the answer arrives later and moves the query's
 * own word. The lookup here is the blocking call the runtime makes on a
 * thread of its pool: it files the query, sleeps on the query's word and
 * reads the address records. The read that ends the wait is the read that
 * releases the kernel's slot, so a lookup holds no slot when it returns.
 * With no slot free the filing is tried again each time the word every
 * socket's change moves is moved -- this unit moves it after a query's
 * last read -- up to a bound, and once more at the bound. Two names are
 * answered without a query: the board's own name (the name the descriptor
 * layer answers as the host name) is the address of every medium that has
 * a lease, or the loopback address with none; "localhost" is the loopback
 * address. The addresses are IPv4, as the kernel resolves them.
 *
 * The interfaces. The runtime builds its interface list from one call that
 * answers two record arrays in one block: the interfaces, then their
 * addresses. A kernel with a medium answers the loopback and the medium:
 * its name by its kind, its hardware address, its link state, and its
 * lease's address with the count of the mask's bits as the prefix length.
 * A kernel with no medium carries no network family, and its list is
 * empty.
 *
 * Every wait goes through the socket unit's one wait (rhu_sock_wait), and
 * the kernel is reached through the family's table (rhu_sock.h).
 *
 * This file is an implementation written from scratch that presents the
 * runtime's own entry points; the signatures are the generated surface
 * header's, read off the runtime's headers. */
#include <stddef.h>
#include <stdint.h>

#include "rhu_shim.h"

#include "rhu.h"
#include "rhu_pal_errno_net11.h"
#include "rhu_pal_networking_net11.h"
#include "rhu_pal_surface_net11.h"
#include "rhu_sock.h"

void *memcpy(void *dst, const void *src, size_t n);
void *memset(void *dst, int c, size_t n);
size_t strlen(const char *s);
void *malloc(size_t n);
void free(void *p);
extern int *__errno_location(void);
#define ERRNO (*__errno_location())

#define RESOLVE_SLOT_WAIT_NS 2000000000ull /* the longest wait for a resolver slot to come free */
#define RESOLVE_RECORDS 8u                  /* the address records one answer is read into */
#define HOST_NAME_ROOM 256
#define NET_IFS 4u /* the loopback and the media */

static const uint8_t g_loopback[4] = { 127, 0, 0, 1 };

/* ------------------------------------------------------------------ */
/* a host's addresses by name                                          */
/* ------------------------------------------------------------------ */

/* Two names compared without the case of their ASCII letters. */
static int name_is(const char *a, const char *b)
{
    for (;; a++, b++) {
        char x = *a, y = *b;
        if (x >= 'A' && x <= 'Z')
            x = (char)(x + ('a' - 'A'));
        if (y >= 'A' && y <= 'Z')
            y = (char)(y + ('a' - 'A'));
        if (x != y)
            return 0;
        if (x == '\0')
            return 1;
    }
}

/* The address of every medium that has a lease, at most max: the count.
 * A lease's address is its four octets in the wire's order. */
static uint32_t lease_addresses(const rhu_sock_kernel *k, uint8_t out[][4], uint32_t max)
{
    uint32_t n = 0;
    int32_t media = k->count();
    for (int32_t m = 0; m < media && n < max; m++) {
        rhu_net_lease_desc lease;
        if (k->lease((uint32_t)m, &lease) == 0 && lease.ip4 != 0) {
            memcpy(out[n], &lease.ip4, 4);
            n++;
        }
    }
    return n;
}

/* An entry for a name and its IPv4 addresses: the name asked as the
 * canonical name, no aliases, the addresses in two allocations that
 * SystemNative_FreeHostEntry frees. */
static int32_t entry_fill(rhu_pal_hostentry *entry, const char *name, uint8_t addresses[][4], uint32_t count)
{
    size_t len = strlen(name) + 1u;
    size_t room = (count != 0 ? count : 1u) * sizeof(rhu_pal_ipaddress);
    uint8_t *canonical = (uint8_t *)malloc(len);
    rhu_pal_ipaddress *list = (rhu_pal_ipaddress *)malloc(room);
    if (canonical == NULL || list == NULL) {
        free(canonical);
        free(list);
        return RHU_PAL_GetAddrInfoErrorFlags_EAI_MEMORY;
    }
    memcpy(canonical, name, len);
    memset(list, 0, room);
    for (uint32_t i = 0; i < count; i++)
        memcpy(list[i].Address, addresses[i], 4);
    entry->CanonicalName = canonical;
    entry->Aliases = NULL;
    entry->IPAddressList = list;
    entry->IPAddressCount = (int32_t)count;
    return RHU_PAL_GetAddrInfoErrorFlags_EAI_SUCCESS;
}

/* One query: the name, the handle its filing answered, and the result. */
typedef struct resolve_op {
    const rhu_sock_kernel *k;
    const char *name;
    uint32_t len;
    int32_t handle;
    int32_t answer;
    rhu_net_addr records[RESOLVE_RECORDS];
} resolve_op;

/* The filing: done at any answer but "no slot free". */
static int32_t scan_file(void *context)
{
    resolve_op *q = (resolve_op *)context;
    q->handle = q->k->resolve(q->name, q->len);
    return q->handle != RHU_EAGAIN;
}

/* The result's read: done at any answer but "pending". The read that is
 * done releases the kernel's slot. */
static int32_t scan_result(void *context)
{
    resolve_op *q = (resolve_op *)context;
    q->answer = q->k->resolve_result(q->handle, q->records, RESOLVE_RECORDS);
    return q->answer != RHU_EAGAIN;
}

/* The runtime's answer for a refusal of the kernel's resolver. */
static int32_t resolve_error(int32_t code)
{
    switch (code) {
    case RHU_EHOSTUNREACH: /* the server has no such name */
    case RHU_EINVAL:       /* a name the resolver does not take */
        return RHU_PAL_GetAddrInfoErrorFlags_EAI_NONAME;
    case RHU_ENETUNREACH: /* no name server: no lease yet, or a static record that names none */
    case RHU_ETIMEDOUT:   /* the stack's retry budget passed with no answer */
    case RHU_EAGAIN:      /* no slot came free within the bound */
        return RHU_PAL_GetAddrInfoErrorFlags_EAI_AGAIN;
    default:
        return RHU_PAL_GetAddrInfoErrorFlags_EAI_FAIL;
    }
}

int32_t SystemNative_GetHostEntryForName(const uint8_t* address, int32_t addressFamily, HostEntry* entry)
{
    rhu_pal_hostentry *e = (rhu_pal_hostentry *)entry;
    const char *name = (const char *)address;
    uint8_t found[RESOLVE_RECORDS][4];
    char own[HOST_NAME_ROOM];
    if (name == NULL || e == NULL)
        return RHU_PAL_GetAddrInfoErrorFlags_EAI_BADARG;
    e->CanonicalName = NULL;
    e->Aliases = NULL;
    e->IPAddressList = NULL;
    e->IPAddressCount = 0;
    if (addressFamily != RHU_PAL_AddressFamily_AF_UNSPEC && addressFamily != RHU_PAL_AddressFamily_AF_INET)
        return RHU_PAL_GetAddrInfoErrorFlags_EAI_FAMILY;
    const rhu_sock_kernel *k = rhu_sock_kernel_table();
    if (name_is(name, "localhost")) {
        memcpy(found[0], g_loopback, 4);
        return entry_fill(e, name, found, 1);
    }
    if (SystemNative_GetHostName((uint8_t *)own, (int32_t)sizeof own) == 0 && name_is(name, own)) {
        uint32_t n = lease_addresses(k, found, RESOLVE_RECORDS);
        if (n == 0) {
            memcpy(found[0], g_loopback, 4);
            n = 1;
        }
        return entry_fill(e, name, found, n);
    }

    resolve_op q;
    q.k = k;
    q.name = name;
    q.len = (uint32_t)strlen(name);
    q.handle = RHU_EAGAIN;
    q.answer = RHU_EAGAIN;
    int32_t w = rhu_sock_wait(k->events_word(), scan_file, &q, rhu_now_ns() + RESOLVE_SLOT_WAIT_NS);
    if (w == RHU_ETIMEDOUT)
        q.handle = k->resolve(q.name, q.len);
    else if (w < 0)
        q.handle = w;
    if (q.handle < 0)
        return resolve_error(q.handle);

    /* The answer's wait has no bound of its own: the stack ends every
     * query, with an answer or at its retry budget, and the result is
     * read then. */
#ifdef RHU_SHIM_BREAK_SOCK_RESOLVE_SLOT_LEAK
    w = rhu_sock_wait(k->resolve_word(q.handle), scan_result, &q, rhu_now_ns());
    if (w == RHU_ETIMEDOUT)
        return RHU_PAL_GetAddrInfoErrorFlags_EAI_AGAIN;
#else
    w = rhu_sock_wait(k->resolve_word(q.handle), scan_result, &q, RHU_DEADLINE_NONE);
#endif
    if (w < 0) {
        /* The kernel did not take the wait. The result is read once more,
         * so a query that has ended holds no slot. */
        (void)k->resolve_result(q.handle, q.records, RESOLVE_RECORDS);
        k->events_bump();
        return RHU_PAL_GetAddrInfoErrorFlags_EAI_FAIL;
    }
    k->events_bump();
    if (q.answer <= 0)
        return q.answer == 0 ? RHU_PAL_GetAddrInfoErrorFlags_EAI_NONAME : resolve_error(q.answer);
    uint32_t n = (uint32_t)q.answer < RESOLVE_RECORDS ? (uint32_t)q.answer : RESOLVE_RECORDS;
    for (uint32_t i = 0; i < n; i++)
        memcpy(found[i], q.records[i].addr, 4);
    return entry_fill(e, name, found, n);
}

void SystemNative_FreeHostEntry(HostEntry* entry)
{
    rhu_pal_hostentry *e = (rhu_pal_hostentry *)entry;
    if (e == NULL)
        return;
    free(e->CanonicalName);
    free(e->IPAddressList);
    e->CanonicalName = NULL;
    e->IPAddressList = NULL;
    e->IPAddressCount = 0;
}

/* ------------------------------------------------------------------ */
/* the interfaces                                                      */
/* ------------------------------------------------------------------ */

/* One interface as both readers answer it. */
typedef struct net_if {
    char name[16];
    uint32_t index;
    uint16_t type; /* the runtime's interface type */
    uint8_t up;
    uint8_t hw_len;
    uint8_t hw[6];
    uint8_t has_address;
    uint8_t prefix;
    uint8_t address[4];
} net_if;

/* The count of a mask's bits: the prefix length of its address. */
static uint8_t prefix_of(uint32_t mask)
{
    uint8_t n = 0;
    for (uint32_t bit = 0; bit < 32u; bit++) {
#ifdef RHU_SHIM_BREAK_SOCK_NETINFO_PREFIX
        n = (uint8_t)(n + (((mask >> bit) & 1u) == 0u));
#else
        n = (uint8_t)(n + ((mask >> bit) & 1u));
#endif
    }
    return n;
}

/* An interface's name: its kind's stem and its number among that kind. */
static void if_name(char *out, const char *stem, uint32_t number)
{
    size_t len = strlen(stem);
    memset(out, 0, 16);
    memcpy(out, stem, len);
    out[len] = (char)('0' + number % 10u);
}

/* The interfaces of this kernel, at most NET_IFS: the loopback at index 1,
 * then each medium. A kernel with no medium has none. */
static uint32_t interfaces_of(net_if *out)
{
    const rhu_sock_kernel *k = rhu_sock_kernel_table();
    int32_t media = k->count();
    uint32_t n = 0, wired = 0, wireless = 0;
    if (media <= 0)
        return 0;
    memset(&out[0], 0, sizeof out[0]);
    memcpy(out[0].name, "lo", 3);
    out[0].index = 1;
    out[0].type = RHU_PAL_NetworkInterfaceType_Loopback;
    out[0].up = 1;
    out[0].has_address = 1;
    out[0].prefix = 8;
    memcpy(out[0].address, g_loopback, 4);
    n = 1;
    for (int32_t m = 0; m < media && n < NET_IFS; m++) {
        rhu_net_medium_desc desc;
        rhu_net_lease_desc lease;
        if (k->medium((uint32_t)m, &desc) != 0)
            continue;
        net_if *f = &out[n];
        memset(f, 0, sizeof *f);
        if (desc.kind == RHU_NET_MEDIUM_WIRELESS) {
            if_name(f->name, "wlan", wireless++);
            f->type = RHU_PAL_NetworkInterfaceType_Wireless80211;
        } else {
            if_name(f->name, "eth", wired++);
            f->type = RHU_PAL_NetworkInterfaceType_Ethernet;
        }
        f->index = n + 1u;
        f->up = desc.link != 0;
        f->hw_len = 6;
        memcpy(f->hw, desc.hw, 6);
        if (k->lease((uint32_t)m, &lease) == 0 && lease.ip4 != 0) {
            f->has_address = 1;
            f->prefix = prefix_of(lease.mask);
            memcpy(f->address, &lease.ip4, 4);
        }
        n++;
    }
    return n;
}

static void address_record(rhu_pal_ipaddressinfo *out, const net_if *f)
{
    memset(out, 0, sizeof *out);
    out->InterfaceIndex = f->index;
    memcpy(out->AddressBytes, f->address, 4);
    out->NumAddressBytes = 4;
    out->PrefixLength = f->prefix;
}

/* The interface records and, in the same block after them, the address
 * records; the caller frees the block by its first pointer. The link speed
 * and the MTU are answered as -1, the runtime's value for a fact the
 * system does not tell. */
int32_t SystemNative_GetNetworkInterfaces(int32_t* interfaceCount, NetworkInterfaceInfo** interfaces, int32_t* addressCount, IpAddressInfo** addressList)
{
    net_if ifs[NET_IFS];
    if (interfaceCount == NULL || interfaces == NULL || addressCount == NULL || addressList == NULL) {
        ERRNO = RHU_GERR_EFAULT;
        return -1;
    }
    uint32_t n = interfaces_of(ifs), addresses = 0;
    for (uint32_t i = 0; i < n; i++)
        addresses += ifs[i].has_address;
    size_t bytes = n * sizeof(rhu_pal_networkinterfaceinfo) + addresses * sizeof(rhu_pal_ipaddressinfo);
    rhu_pal_networkinterfaceinfo *block = (rhu_pal_networkinterfaceinfo *)malloc(bytes != 0 ? bytes : 1u);
    if (block == NULL) {
        ERRNO = RHU_GERR_ENOMEM;
        return -1;
    }
    memset(block, 0, bytes != 0 ? bytes : 1u);
    rhu_pal_ipaddressinfo *list = (rhu_pal_ipaddressinfo *)(void *)(block + n);
    uint32_t a = 0;
    for (uint32_t i = 0; i < n; i++) {
        const net_if *f = &ifs[i];
        rhu_pal_networkinterfaceinfo *o = &block[i];
        memcpy(o->Name, f->name, sizeof o->Name);
        o->Speed = -1;
        o->InterfaceIndex = f->index;
        o->Mtu = -1;
        o->HardwareType = f->type;
        o->OperationalState = f->up ? RHU_PAL_OperationalStatus_Up : RHU_PAL_OperationalStatus_Down;
        o->NumAddressBytes = f->hw_len;
        memcpy(o->AddressBytes, f->hw, f->hw_len);
        if (f->has_address)
            address_record(&list[a++], f);
    }
    *interfaceCount = (int32_t)n;
    *interfaces = (NetworkInterfaceInfo *)block;
    *addressCount = (int32_t)a;
    *addressList = (IpAddressInfo *)list;
    return 0;
}

/* Each interface's IPv4 address to the first function and its hardware
 * address to the third. */
int32_t SystemNative_EnumerateInterfaceAddresses(void* context, IPv4AddressFound onIpv4Found, IPv6AddressFound onIpv6Found, LinkLayerAddressFound onLinkLayerFound)
{
    net_if ifs[NET_IFS];
    uint32_t n = interfaces_of(ifs);
    (void)onIpv6Found;
    for (uint32_t i = 0; i < n; i++) {
        const net_if *f = &ifs[i];
        if (f->has_address && onIpv4Found != NULL) {
            rhu_pal_ipaddressinfo info;
            address_record(&info, f);
            onIpv4Found(context, f->name, (IpAddressInfo *)&info);
        }
        if (f->hw_len != 0 && onLinkLayerFound != NULL) {
            rhu_pal_linklayeraddressinfo link;
            memset(&link, 0, sizeof link);
            link.InterfaceIndex = f->index;
            memcpy(link.AddressBytes, f->hw, f->hw_len);
            link.NumAddressBytes = f->hw_len;
            link.HardwareType = f->type;
            onLinkLayerFound(context, f->name, (LinkLayerAddressInfo *)&link);
        }
    }
    return 0;
}
