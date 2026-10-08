/* The bus device nodes over the kernel's I2C and SPI API: the two device
 * files a program's I2C and SPI drivers open on Linux, presented here so
 * the standard package's classes (System.Device.Gpio at the pin of record,
 * its I2C and SPI Linux drivers) run unchanged on this system. The core's
 * C-library open hands a path that starts with /dev/i2c- or /dev/spidev
 * to this unit, and its ioctl and close hand every descriptor of the
 * device band here; the SDK binds the package's three C-library calls
 * directly, so they land on those functions at link time and the
 * runtime's loader is never asked.
 *
 * This file is an implementation written from scratch that presents the
 * established device-file surface. The request numbers, the flag values
 * and the record layouts are the package's binding, read off its source
 * at the pinned version (its i2c_msg of twelve bytes with the pointer at
 * offset eight, its i2c_rdwr_ioctl_data, its spi_ioc_transfer of
 * thirty-two bytes with the two buffers as 64-bit words, its mode byte);
 * the code here maps them onto the kernel's bus calls.
 *
 * The I2C node /dev/i2c-<bus>: the open checks the board has the bus and
 * hands out a descriptor; the bus is claimed at the first transfer at 100
 * kHz, or earlier at another served rate by this system's one request on
 * the node (0x0740, a word in hertz), which the board class issues for a
 * program that asks a rate -- Linux defines no I2C clock request and the
 * standard binding sends none, so a program on the standard API alone runs
 * the bus at 100 kHz, as a device tree would fix it, and sends nothing new;
 * the functionality request
 * stores the 32-bit ABI's one four-byte word, "plain combined
 * transactions" and nothing else, so the binding always takes its
 * combined-transfer path; the combined-transfer request copies up to
 * eight message records into the kernel's records (the reserved word
 * zero, the binding's padding never read) and runs ONE transaction
 * through rhu_i2c_transfer with a deadline one second past the call --
 * the default of Linux's adapters -- bounding the wait for the shared
 * bus. The SPI node /dev/spidev<bus>.<select>: the open claims the bus
 * and records the select; the mode, word-size and speed requests are the
 * node's own settings (mode 0 to 3 with the no-select bit; eight-bit
 * words; the rate), and EVERY transfer carries them into
 * rhu_spi_transfer, so several devices with their own settings share one
 * bus; the no-select mode bit makes the node transfer with the kernel's
 * no-select value for a program that drives its own chip select through
 * the GPIO class; the select is the kernel's to claim at the first
 * transfer that uses it. The kernel answers the rate it ACHIEVED, and
 * this unit states it on the wire in one line whenever a node's settings
 * first take effect or change, so a program's author reads the true
 * clock.
 *
 * A caller's memory is read ONCE: every record a request names (the
 * transaction's header and its messages, the transfer record, a setting's
 * byte or word) is copied before it is checked, and the request runs and
 * answers from the copy, so a receive buffer that lies over its own
 * record changes nothing and no record need be aligned. A NULL where a
 * request needs memory is refused EFAULT. Any other address is the
 * caller's word: this system has one address space and no copy boundary
 * between a program and the kernel, so an address no memory answers ends
 * the program in the kernel's fault report, as it does at every kernel
 * call.
 *
 * A node's entry belongs to the requests running on it. A request takes
 * its entry under the table's mutex, counts itself in, and runs on ONE
 * copy of the node's settings taken at that moment, so a setting changed
 * meanwhile takes effect at the next request and a mode byte is never
 * seen half changed. A close during a running request answers at once and
 * the descriptor is closed from then on, but the entry is given back only
 * when the last running request on it ends: an open in between receives
 * another number, never the entry a request still runs on. An open
 * becomes visible only when its claim has succeeded (an I2C node's open
 * has none: its bus is claimed at the first transfer, or earlier by the
 * rate request).
 *
 * The limits, plainly: a node's number is the bus's own number on the chip,
 * as the chip's documentation and Linux name it (the block I2C1 is
 * /dev/i2c-1; a program written for another board keeps its numbers where
 * the blocks agree) -- the number passes straight through to the kernel,
 * and a number naming no bus on this board is refused at the open; the
 * select 4294967295 names no node (the number is the kernel's no-select
 * value, which only the no-select mode bit may ask for); an I2C message
 * carries at most 255 bytes and a transaction at most eight messages
 * (refused, never split); seven-bit addresses; eight-bit SPI words with
 * the most significant bit first (the binding reverses bytes itself when
 * this unit refuses its least-significant-bit-first request, as it does
 * on other boards); the select active low; a read shifts out ones (the
 * kernel's answer to a missing transmit buffer); a select the bus does
 * not have is refused at the first transfer, not at the open (no kernel
 * call states a bus's select count); a node's settings are its own
 * descriptor's and start unset (no rate until the program sets one or a
 * transfer carries one); a closed node does NOT release its bus or its
 * select, because this generation of the kernel API has no release call
 * -- the release at the last close is registered as an intent for the
 * kernel's release act. No SMBus emulation, no file-transfer form (read
 * and write on a node, the address-selection requests), no ten-bit
 * addressing, no post-transfer delay, one transfer record per request
 * (an empty message answers 0), no duplicated descriptors, the 32-bit
 * mode and bit-order requests not served: each refused with its errno
 * and, where a program's author needs it, one line naming the request.
 * Nothing here masks anything: every call runs on the caller's thread,
 * a transfer that finds its bus busy sleeps on the kernel's tenancy word,
 * and the table's mutex holds for a few instructions with no transfer
 * and no claim under it. */
#include <stddef.h>
#include <stdint.h>

#include "rhu_shim.h"

#include "rhu.h"
#include "rhu_pal_errno_net11_rc1.h"

extern int *__errno_location(void);
void *memcpy(void *dst, const void *src, size_t n);
void *memset(void *dst, int c, size_t n);
int strncmp(const char *a, const char *b, size_t n);
int pthread_mutex_lock(void *slot);
int pthread_mutex_unlock(void *slot);

#define ERRNO (*__errno_location())
#define NS_PER_S 1000000000ull

/* The binding's I2C requests, its functionality bit and its read flag. */
#define BUSDEV_I2C_FUNCS 0x0705u
#define BUSDEV_I2C_RDWR 0x0707u
#define BUSDEV_I2C_FUNC_I2C 0x1u
#define BUSDEV_I2C_M_RD 0x0001u
/* The binding's SPI requests: the settings by their exact numbers, the
 * transfer request by its base with the record count in the size field. */
#define BUSDEV_SPI_WR_MODE 0x40016b01u
#define BUSDEV_SPI_RD_MODE 0x80016b01u
#define BUSDEV_SPI_WR_BITS 0x40016b03u
#define BUSDEV_SPI_RD_BITS 0x80016b03u
#define BUSDEV_SPI_WR_SPEED 0x40046b04u
#define BUSDEV_SPI_RD_SPEED 0x80046b04u
#define BUSDEV_SPI_MESSAGE_BASE 0x40006b00u
#define BUSDEV_SPI_SIZE_MASK 0x3fff0000u
#define BUSDEV_SPI_RECORD_BYTES 32u
/* The binding's mode byte. */
#define BUSDEV_SPI_CPHA 0x01u
#define BUSDEV_SPI_CPOL 0x02u
#define BUSDEV_SPI_CS_HIGH 0x04u
#define BUSDEV_SPI_LSB_FIRST 0x08u
#define BUSDEV_SPI_3WIRE 0x10u
#define BUSDEV_SPI_LOOP 0x20u
#define BUSDEV_SPI_NO_CS 0x40u
#define BUSDEV_SPI_READY 0x80u
/* The bits of it this unit keeps: the clock's two and the no-select bit,
 * ONE byte, so a node's mode is never half changed. */
#define BUSDEV_SPI_CLOCK_BITS (BUSDEV_SPI_CPHA | BUSDEV_SPI_CPOL)
#define BUSDEV_SPI_KEPT_BITS (BUSDEV_SPI_CLOCK_BITS | BUSDEV_SPI_NO_CS)

/* The binding's records as this 32-bit target lays them out. The message
 * record's two padding bytes at offset six are never read; the transfer
 * record's two 64-bit buffer words are read as pairs of 32-bit words. */
struct busdev_i2c_msg {
    uint16_t addr;
    uint16_t flags;
    uint16_t len;
    uint8_t *buf;
};
struct busdev_i2c_rdwr {
    struct busdev_i2c_msg *msgs;
    uint32_t nmsgs;
};
struct busdev_spi_transfer {
    uint32_t tx_lo;
    uint32_t tx_hi;
    uint32_t rx_lo;
    uint32_t rx_hi;
    uint32_t len;
    uint32_t speed_hz;
    uint16_t delay_usecs;
    uint8_t bits_per_word;
    uint8_t cs_change;
    uint8_t tx_nbits;
    uint8_t rx_nbits;
    uint16_t pad;
};
static_assert(sizeof(struct busdev_i2c_msg) == 12, "the binding's message record is twelve bytes");
static_assert(sizeof(struct busdev_i2c_rdwr) == 8, "the binding's transaction record is eight bytes");
static_assert(sizeof(struct busdev_spi_transfer) == 32, "the binding's transfer record is thirty-two bytes");
static_assert(sizeof(rhu_i2c_msg) == 12, "the kernel's message record is twelve bytes");
static_assert(sizeof(unsigned long) == 4, "the functionality request's output is one four-byte word on this ABI");

#define BUSDEV_I2C_RATE_HZ 100000u
/* RHU's one request on the I2C node (in the i2c-dev family's range by
 * shape, outside Linux's numbers 0x0701 to 0x0720): one 32-bit word in
 * hertz, in and out -- the bus claimed at that rate for the program's run,
 * the word read back with the rate the bus runs at. The board class issues
 * it; the harness ties the two numbers. */
#define BUSDEV_I2C_RATE 0x0740u
#define BUSDEV_I2C_BUSES 8u
#define BUSDEV_DEADLINE_NS NS_PER_S

enum { KIND_I2C = 1, KIND_SPI = 2 };
/* An entry's life: free; taken by an open whose claim is still running
 * (no request can find it); live; closed while requests still run on it
 * (no request can find it, no open can take it). */
enum { NODE_FREE = 0, NODE_OPENING = 1, NODE_LIVE = 2, NODE_CLOSING = 3 };

/* One node: its state and kind, the requests running on it, its bus, and
 * for an SPI node its select and settings, with the last settings this
 * unit stated on the wire. */
struct busdev_node {
    uint8_t state;
    uint8_t kind;
    uint8_t mode;
    uint8_t stated;
    uint32_t users;
    uint32_t bus;
    uint32_t select;
    uint32_t hz;
    uint32_t stated_mode;
    uint32_t stated_hz;
    uint32_t stated_achieved;
};

static struct busdev_node g_nodes[RHU_SHIM_FD_DEVICE_COUNT];
static uint8_t g_mutex[24];

/* The buses' rates for the program's run: a bus claimed through this unit
 * and the rate it runs at, one entry per bus (no board has more than the
 * table holds). The kernel knows a claimed bus's state but answers one
 * code for "another rate" and for "a pin held as a GPIO line"; the table
 * tells them apart and knows the rate to read back. Filled under the
 * table's mutex WITH the kernel's claim under it, so the two agree at
 * every moment (a transfer in flight holds no lock, so the kernel's wait
 * for the bus cannot deadlock a claim). */
struct busdev_rate {
    uint32_t bus;
    uint32_t hz;
};
static struct busdev_rate g_rates[BUSDEV_I2C_BUSES];
static uint32_t g_rate_count;

static int refuse(int32_t err)
{
    ERRNO = err;
    return -1;
}

/* The table's mutex: a zero-filled slot the core adopts at its first use,
 * two first users at once included. A lock that fails is a fault of the
 * slot, never the caller's: the request answers ENOLCK with one line and
 * touches nothing the lock guards. */
static int table_lock(void)
{
    if (pthread_mutex_lock(g_mutex) == 0)
        return 1;
    rhu_shim_log("bus nodes: the table's lock refused (ENOLCK)");
    ERRNO = RHU_GERR_ENOLCK;
    return 0;
}

static void table_unlock(void)
{
    pthread_mutex_unlock(g_mutex);
}

static struct busdev_node *entry_of(int fd)
{
    if (fd < RHU_SHIM_FD_DEVICE_LO || fd >= RHU_SHIM_FD_DEVICE_LO + RHU_SHIM_FD_DEVICE_COUNT)
        return NULL;
    return &g_nodes[fd - RHU_SHIM_FD_DEVICE_LO];
}

/* A request's entry: the descriptor's live node, counted as in use, with
 * ONE copy of it taken under the lock; NULL with errno set. */
static struct busdev_node *node_enter(int fd, struct busdev_node *copy)
{
    if (!table_lock())
        return NULL;
    struct busdev_node *n = entry_of(fd);
    if (n == NULL || n->state != NODE_LIVE) {
        table_unlock();
        ERRNO = RHU_GERR_EBADF;
        return NULL;
    }
    n->users++;
    *copy = *n;
    table_unlock();
    return n;
}

/* A request's exit: counted out; an entry closed meanwhile is given back
 * when its last request leaves. (A lock refused here would leave the entry
 * counted in for the boot: the same damaged-slot path as at an open's
 * end, stated rather than worked around.) */
static void node_leave(struct busdev_node *n)
{
    if (!table_lock())
        return;
    if (n->users != 0)
        n->users--;
    if (n->state == NODE_CLOSING && n->users == 0)
        memset(n, 0, sizeof *n);
    table_unlock();
}

/* A decimal number of the node's name: digits only, no sign, no leading
 * zero but the number 0 itself, inside 32 bits. */
static int parse_number(const char **p, uint32_t *out)
{
    const char *s = *p;
    uint32_t v = 0;
    int digits = 0;
    while (*s >= '0' && *s <= '9') {
        if (digits == 1 && v == 0)
            return 0;
        if (digits == 10)
            return 0;
        uint32_t next = v * 10u + (uint32_t)(*s - '0');
        if (next / 10u != v)
            return 0;
        v = next;
        digits++;
        s++;
    }
    if (digits == 0)
        return 0;
    *p = s;
    *out = v;
    return 1;
}

/* A kernel call's refusal as the binding sees it: errno set from the code
 * (the codes are the platform's numbers, negated), -1 returned. The line
 * on the wire is the caller's, where one is due. */
static int refuse_code(int32_t rc)
{
    int32_t e;
    switch (rc) {
    case RHU_ENXIO:
        e = RHU_GERR_ENXIO;
        break;
    case RHU_EIO:
        e = RHU_GERR_EIO;
        break;
    case RHU_ETIMEDOUT:
        e = RHU_GERR_ETIMEDOUT;
        break;
    case RHU_EBUSY:
        e = RHU_GERR_EBUSY;
        break;
    case RHU_ENODEV:
        e = RHU_GERR_ENODEV;
        break;
    case RHU_ENOTSUP:
        e = RHU_GERR_ENOTSUP;
        break;
    default:
        e = RHU_GERR_EINVAL;
        break;
    }
    ERRNO = e;
    return -1;
}

/* ------------------------------------------------------------------ */
/* open and close                                                      */
/* ------------------------------------------------------------------ */

int rhu_busdev_open(const char *path, int flags)
{
    (void)flags;
    uint8_t kind;
    uint32_t bus = 0;
    uint32_t select = 0;
    const char *p;
    if (strncmp(path, "/dev/i2c-", 9) == 0) {
        p = path + 9;
        if (!parse_number(&p, &bus) || *p != 0) {
            rhu_shim_log("%s: not a bus node name (ENOENT)", path);
            return refuse(RHU_GERR_ENOENT);
        }
        kind = KIND_I2C;
    } else if (strncmp(path, "/dev/spidev", 11) == 0) {
        p = path + 11;
        if (!parse_number(&p, &bus) || *p != '.' || (p++, !parse_number(&p, &select)) || *p != 0) {
            rhu_shim_log("%s: not a bus node name (ENOENT)", path);
            return refuse(RHU_GERR_ENOENT);
        }
#ifndef RHU_SHIM_BREAK_BUSDEV_SELECT_NAME
        /* The kernel's no-select value is no select's name: only the mode
         * bit asks for a transfer without a select. */
        if (select == RHU_SPI_SELECT_NONE) {
            rhu_shim_log("%s: not a bus node name (ENOENT)", path);
            return refuse(RHU_GERR_ENOENT);
        }
#endif
        kind = KIND_SPI;
    } else {
        rhu_shim_log("%s: not a bus node name (ENOENT)", path);
        return refuse(RHU_GERR_ENOENT);
    }
    if (kind == KIND_I2C) {
        /* The board has the bus, by the enumeration doors (no claim here:
         * the bus is claimed at the first transfer, or earlier by the rate
         * request); a number none of them answers is a missing node. */
        int32_t count = rhu_i2c_count();
        int found = 0;
        for (int32_t i = 0; i < count && !found; i++) {
            int32_t number = rhu_i2c_bus((uint32_t)i);
            found = number >= 0 && (uint32_t)number == bus;
        }
        if (!found) {
            rhu_shim_log("%s: no such bus on this board (ENOENT)", path);
            return refuse(RHU_GERR_ENOENT);
        }
    }
    if (!table_lock())
        return -1;
    int slot = -1;
    for (int i = 0; i < RHU_SHIM_FD_DEVICE_COUNT && slot < 0; i++) {
        if (g_nodes[i].state == NODE_FREE)
            slot = i;
    }
    if (slot < 0) {
        table_unlock();
        rhu_shim_log("%s: no descriptor free (EMFILE: %d device descriptors are open)", path, RHU_SHIM_FD_DEVICE_COUNT);
        return refuse(RHU_GERR_EMFILE);
    }
    struct busdev_node *n = &g_nodes[slot];
    memset(n, 0, sizeof *n);
    n->state = NODE_OPENING;
    n->kind = kind;
    n->bus = bus;
    n->select = select;
    table_unlock();
    /* The SPI node's claim, with the node's number as the bus number,
     * outside the lock (it waits for a transfer running on the bus):
     * idempotent by the kernel's contract (a second claim of a claimed bus
     * answers 0 and changes nothing), so every SPI open claims; a number
     * naming no bus on this board is the kernel's ENODEV, which is a
     * missing node here. The I2C node claims nothing at its open: its bus
     * is claimed at the first transfer, or earlier by the rate request. */
    int32_t rc = kind == KIND_I2C ? 0 : rhu_spi_claim(bus);
    /* A lock refused HERE would leave this one entry in its opening state
     * for the boot (no request finds it, no open takes it): the lock was
     * taken a moment ago, so only a damaged slot gets here. */
    if (!table_lock())
        return -1;
    if (rc == 0)
        n->state = NODE_LIVE;
    else
        memset(n, 0, sizeof *n);
    table_unlock();
    if (rc != 0) {
        if (rc == RHU_EBUSY)
            rhu_shim_log("%s: the bus refused (EBUSY: a program holds one of its pins as a GPIO line)", path);
        else if (rc == RHU_ENODEV) {
            rhu_shim_log("%s: no such bus on this board (ENOENT)", path);
            return refuse(RHU_GERR_ENOENT);
        } else {
            rhu_shim_log("%s: the bus refused (EIO: the claim answered %d)", path, rc);
            return refuse(RHU_GERR_EIO);
        }
        return refuse_code(rc);
    }
    return RHU_SHIM_FD_DEVICE_LO + slot;
}

int rhu_busdev_close(int fd)
{
    if (!table_lock())
        return -1;
    struct busdev_node *n = entry_of(fd);
    if (n == NULL || n->state != NODE_LIVE) {
        table_unlock();
        return refuse(RHU_GERR_EBADF);
    }
    /* The bus and its selects stay the board's: no release call in this
     * generation of the kernel API (the release at the last close an
     * intent). The entry goes back now, or when the last request running
     * on it ends. */
#ifdef RHU_SHIM_BREAK_BUSDEV_UNOWNED
    memset(n, 0, sizeof *n);
#else
    if (n->users == 0)
        memset(n, 0, sizeof *n);
    else
        n->state = NODE_CLOSING;
#endif
    table_unlock();
    return 0;
}

/* ------------------------------------------------------------------ */
/* the I2C node's requests                                             */
/* ------------------------------------------------------------------ */

static int i2c_functionality(void *arg)
{
#ifdef RHU_SHIM_BREAK_BUSDEV_FUNCS
    uint32_t word = 0;
#else
    uint32_t word = BUSDEV_I2C_FUNC_I2C;
#endif
#ifdef RHU_SHIM_BREAK_BUSDEV_FUNCS_WIDE
    uint32_t wide[2] = { word, 0 };
    memcpy(arg, wide, sizeof wide);
#else
    /* The ABI's unsigned long: four bytes, and not one more. */
    memcpy(arg, &word, sizeof word);
#endif
    return 0;
}

/* A bus claimed at a rate for the program's run, through the table and the
 * kernel under the lock. 0 with *running the rate the bus runs at (the
 * asked one, now or already; for the first transfer's default claim, the
 * rate an earlier request set); -1 with errno otherwise -- EBUSY with
 * *running the table's rate when the bus runs at another rate for this
 * run, EBUSY with *running 0 when a program holds one of the bus's pins
 * as a GPIO line (the table has no entry: the kernel's one code told
 * apart by the table), ENOTSUP for a rate not served at this clock (the
 * kernel judges it before the bus's state, so on a claimed bus too),
 * EMFILE for a bus beyond the table, the kernel's other codes as the
 * transaction maps them. One line on the wire when a claim takes effect,
 * one for a refusal. */
static int i2c_rate_claim(uint32_t bus, uint32_t hz, uint32_t *running, int asked)
{
    *running = 0;
    if (!table_lock())
        return -1;
    struct busdev_rate *e = NULL;
    for (uint32_t i = 0; i < g_rate_count && e == NULL; i++) {
        if (g_rates[i].bus == bus)
            e = &g_rates[i];
    }
    if (e != NULL && (!asked || e->hz == hz)) {
        /* Claimed earlier for this run: the default meets it as it runs; the
         * same rate again is the kernel's idempotence, answered here. */
        *running = e->hz;
        table_unlock();
        return 0;
    }
    if (e == NULL && g_rate_count == BUSDEV_I2C_BUSES) {
        table_unlock();
        rhu_shim_log("/dev/i2c-%u: a bus beyond the rate table's %u refused (EMFILE)", bus, BUSDEV_I2C_BUSES);
        return refuse(RHU_GERR_EMFILE);
    }
    int32_t rc = rhu_i2c_claim(bus, hz);
    if (rc == 0) {
        if (e == NULL) {
            g_rates[g_rate_count].bus = bus;
            g_rates[g_rate_count].hz = hz;
            g_rate_count++;
        } else {
            e->hz = hz;
        }
        table_unlock();
        *running = hz;
        if (asked)
            rhu_shim_log("/dev/i2c-%u: the bus claimed at %u Hz (asked)", bus, hz);
        else
            rhu_shim_log("/dev/i2c-%u: the bus claimed at %u Hz (the default, at the first transfer)", bus, hz);
        return 0;
    }
    uint32_t have = e != NULL ? e->hz : 0;
    table_unlock();
    switch (rc) {
    case RHU_EBUSY:
        if (have != 0) {
            *running = have;
            rhu_shim_log("/dev/i2c-%u: a rate of %u Hz refused (EBUSY: the bus runs at %u Hz for this program's run)", bus, hz, have);
        } else {
            rhu_shim_log("/dev/i2c-%u: a rate of %u Hz refused (EBUSY: a program holds one of its pins as a GPIO line)", bus, hz);
        }
        return refuse(RHU_GERR_EBUSY);
    case RHU_ENOTSUP:
        rhu_shim_log("/dev/i2c-%u: a rate of %u Hz refused (ENOTSUP: not served at this image's clock)", bus, hz);
        return refuse(RHU_GERR_ENOTSUP);
    case RHU_ENODEV:
        rhu_shim_log("/dev/i2c-%u: no such bus on this board (ENODEV)", bus);
        return refuse(RHU_GERR_ENODEV);
    default:
        rhu_shim_log("/dev/i2c-%u: the claim answered %d (EIO)", bus, rc);
        return refuse(RHU_GERR_EIO);
    }
}

/* The rate request: the word read once; 0 refused EINVAL; the bus claimed
 * at the rate for the run and the word written back with the rate the bus
 * runs at (the asked one on 0; the running one on EBUSY for another rate;
 * untouched on every other refusal). */
static int i2c_rate(const struct busdev_node *c, void *arg)
{
    uint32_t hz;
    memcpy(&hz, arg, sizeof hz);
    if (hz == 0) {
        rhu_shim_log("/dev/i2c-%u: a rate of 0 Hz refused (EINVAL)", c->bus);
        return refuse(RHU_GERR_EINVAL);
    }
#ifdef RHU_SHIM_BREAK_BUSDEV_RATE_UNJUDGED
    /* The careless shape: the request answers 0 and claims nothing. */
    return 0;
#else
    uint32_t running;
    int rc = i2c_rate_claim(c->bus, hz, &running, 1);
    if (rc == 0 || running != 0)
        memcpy(arg, &running, sizeof running);
    return rc;
#endif
}

static int i2c_transaction(const struct busdev_node *c, void *arg)
{
    struct busdev_i2c_rdwr set;
    memcpy(&set, arg, sizeof set);
    uint32_t count = set.nmsgs;
    if (count == 0 || count > RHU_I2C_MSG_MAX) {
        rhu_shim_log("/dev/i2c-%u: %u messages in one transaction refused (EINVAL: the limit is %u)", c->bus, count, (uint32_t)RHU_I2C_MSG_MAX);
        return refuse(RHU_GERR_EINVAL);
    }
    if (set.msgs == NULL)
        return refuse(RHU_GERR_EINVAL);
    rhu_i2c_msg msgs[RHU_I2C_MSG_MAX];
    for (uint32_t i = 0; i < count; i++) {
        struct busdev_i2c_msg m;
        memcpy(&m, (const uint8_t *)set.msgs + i * sizeof m, sizeof m);
        if ((m.flags & ~BUSDEV_I2C_M_RD) != 0) {
            rhu_shim_log("/dev/i2c-%u: a message flag 0x%x refused (EINVAL: only the read flag is served)", c->bus, (uint32_t)m.flags);
            return refuse(RHU_GERR_EINVAL);
        }
        if (m.len > RHU_I2C_LEN_MAX) {
            rhu_shim_log("/dev/i2c-%u: a message of %u bytes refused (EINVAL: the limit is %u)", c->bus, (uint32_t)m.len, (uint32_t)RHU_I2C_LEN_MAX);
            return refuse(RHU_GERR_EINVAL);
        }
        if (m.buf == NULL && m.len != 0) {
            rhu_shim_log("/dev/i2c-%u: a message of %u bytes with no buffer refused (EFAULT)", c->bus, (uint32_t)m.len);
            return refuse(RHU_GERR_EFAULT);
        }
        msgs[i].addr = m.addr;
        msgs[i].flags = (m.flags & BUSDEV_I2C_M_RD) != 0 ? RHU_I2C_MSG_READ : 0;
        msgs[i].len = m.len;
        msgs[i].reserved = 0;
        msgs[i].buf = m.buf;
    }
    /* The bus claimed at the default rate at the first transfer, unless the
     * rate request claimed it earlier: a program that names no rate meets
     * the bus at 100 kHz, as the open gave it before (its refusal for a pin
     * a program holds as a GPIO line lands here, where the binding's lazy
     * open already put it from the program's side). */
    uint32_t running;
    if (i2c_rate_claim(c->bus, BUSDEV_I2C_RATE_HZ, &running, 0) != 0)
        return -1;
    int32_t rc = rhu_i2c_transfer(c->bus, msgs, count, rhu_now_ns() + BUSDEV_DEADLINE_NS);
    if (rc == 0)
        return (int)count;
    switch (rc) {
    case RHU_ENXIO:
    case RHU_EIO:
    case RHU_ETIMEDOUT:
        /* An address not acknowledged is a scan's normal answer; the other
         * two the kernel names in its own line. */
        break;
    case RHU_EINVAL:
        rhu_shim_log("/dev/i2c-%u: the transaction refused by the kernel (EINVAL: an address over 0x7F, or the bus not claimed)", c->bus);
        break;
    case RHU_ENODEV:
        rhu_shim_log("/dev/i2c-%u: the transaction refused (ENODEV: no such bus on this row)", c->bus);
        break;
    default:
        rhu_shim_log("/dev/i2c-%u: the transaction answered %d (EIO)", c->bus, rc);
        return refuse(RHU_GERR_EIO);
    }
    return refuse_code(rc);
}

static int i2c_request(const struct busdev_node *c, unsigned long request, void *arg)
{
    switch (request) {
    case BUSDEV_I2C_FUNCS:
        return arg == NULL ? refuse(RHU_GERR_EFAULT) : i2c_functionality(arg);
    case BUSDEV_I2C_RDWR:
        return arg == NULL ? refuse(RHU_GERR_EFAULT) : i2c_transaction(c, arg);
    case BUSDEV_I2C_RATE:
        return arg == NULL ? refuse(RHU_GERR_EFAULT) : i2c_rate(c, arg);
    default:
        break;
    }
    rhu_shim_log("/dev/i2c-%u: request 0x%x not served (ENOTTY: the combined-transfer form is the one presented)", c->bus, (uint32_t)request);
    return refuse(RHU_GERR_ENOTTY);
}

/* ------------------------------------------------------------------ */
/* the SPI node's requests                                             */
/* ------------------------------------------------------------------ */

static int spi_set_mode(struct busdev_node *n, const struct busdev_node *c, void *arg)
{
    uint8_t byte;
    memcpy(&byte, arg, sizeof byte);
    uint32_t m = byte;
    if ((m & BUSDEV_SPI_CS_HIGH) != 0) {
        rhu_shim_log("/dev/spidev%u.%u: an active-high select refused (EINVAL: the kernel drives the select low)", c->bus, c->select);
        return refuse(RHU_GERR_EINVAL);
    }
    if ((m & BUSDEV_SPI_LSB_FIRST) != 0) {
        rhu_shim_log("/dev/spidev%u.%u: least-significant-bit-first refused (EINVAL: the block sends the most significant bit first)", c->bus, c->select);
        return refuse(RHU_GERR_EINVAL);
    }
    if ((m & (BUSDEV_SPI_3WIRE | BUSDEV_SPI_LOOP | BUSDEV_SPI_READY)) != 0) {
        rhu_shim_log("/dev/spidev%u.%u: the mode bits 0x%x refused (EINVAL: three-wire, loopback and ready signalling are not served)", c->bus, c->select, m);
        return refuse(RHU_GERR_EINVAL);
    }
    if (!table_lock())
        return -1;
    n->mode = (uint8_t)(m & BUSDEV_SPI_KEPT_BITS);
    table_unlock();
    return 0;
}

static int spi_set_bits(const struct busdev_node *c, void *arg)
{
    uint8_t byte;
    memcpy(&byte, arg, sizeof byte);
    uint32_t bits = byte;
    if (bits != 0 && bits != 8) {
        rhu_shim_log("/dev/spidev%u.%u: a word of %u bits refused (EINVAL: the kernel clocks eight-bit frames)", c->bus, c->select, bits);
        return refuse(RHU_GERR_EINVAL);
    }
    return 0;
}

static int spi_set_speed(struct busdev_node *n, const struct busdev_node *c, void *arg)
{
    uint32_t hz;
    memcpy(&hz, arg, sizeof hz);
    if (hz == 0) {
        rhu_shim_log("/dev/spidev%u.%u: a rate of 0 Hz refused (EINVAL)", c->bus, c->select);
        return refuse(RHU_GERR_EINVAL);
    }
    if (!table_lock())
        return -1;
    n->hz = hz;
    table_unlock();
    return 0;
}

/* The achieved rate, stated once per change of a node's settings: the
 * node's memory of its last line under the lock, the line itself outside
 * it, named from the request's own copy. */
static void state_rate(struct busdev_node *n, const struct busdev_node *c, uint32_t hz, uint32_t achieved)
{
    uint32_t mode = c->mode & BUSDEV_SPI_CLOCK_BITS;
    int say = 0;
    if (table_lock()) {
        if (n->stated == 0 || n->stated_mode != mode || n->stated_hz != hz || n->stated_achieved != achieved) {
            n->stated = 1;
            n->stated_mode = mode;
            n->stated_hz = hz;
            n->stated_achieved = achieved;
            say = 1;
        }
        table_unlock();
    }
    if (say)
        rhu_shim_log("/dev/spidev%u.%u: mode %u, 8 bits, %u Hz asked, %u Hz achieved", c->bus, c->select, mode, hz, achieved);
}

static int spi_transfer(struct busdev_node *n, const struct busdev_node *c, unsigned long request, void *arg)
{
    uint32_t size = (uint32_t)(request & BUSDEV_SPI_SIZE_MASK) >> 16;
    if (size == 0)
        return 0;
    if (size != BUSDEV_SPI_RECORD_BYTES) {
        rhu_shim_log("/dev/spidev%u.%u: %u transfer records in one request refused (EINVAL: one is served)", c->bus, c->select, size / BUSDEV_SPI_RECORD_BYTES);
        return refuse(RHU_GERR_EINVAL);
    }
    if (arg == NULL)
        return refuse(RHU_GERR_EFAULT);
    /* The record is the caller's memory and the receive buffer may lie
     * over it: copied once, and the answer is the copy's length. */
    struct busdev_spi_transfer t;
    memcpy(&t, arg, sizeof t);
    if (t.tx_hi != 0 || t.rx_hi != 0) {
        rhu_shim_log("/dev/spidev%u.%u: a buffer address above 32 bits refused (EINVAL)", c->bus, c->select);
        return refuse(RHU_GERR_EINVAL);
    }
    const void *tx = (const void *)t.tx_lo;
    void *rx = (void *)t.rx_lo;
    if (tx == NULL && rx == NULL) {
        rhu_shim_log("/dev/spidev%u.%u: a transfer with neither buffer refused (EINVAL)", c->bus, c->select);
        return refuse(RHU_GERR_EINVAL);
    }
    if (t.len == 0 || t.len > RHU_SPI_LEN_MAX) {
        rhu_shim_log("/dev/spidev%u.%u: a transfer of %u bytes refused (EINVAL: the limit is %u)", c->bus, c->select, t.len, (uint32_t)RHU_SPI_LEN_MAX);
        return refuse(RHU_GERR_EINVAL);
    }
    uint32_t hz = t.speed_hz != 0 ? t.speed_hz : c->hz;
    if (hz == 0) {
        rhu_shim_log("/dev/spidev%u.%u: no clock rate set (EINVAL)", c->bus, c->select);
        return refuse(RHU_GERR_EINVAL);
    }
    if (t.bits_per_word != 0 && t.bits_per_word != 8) {
        rhu_shim_log("/dev/spidev%u.%u: a word of %u bits refused (EINVAL: the kernel clocks eight-bit frames)", c->bus, c->select, (uint32_t)t.bits_per_word);
        return refuse(RHU_GERR_EINVAL);
    }
    if (t.delay_usecs != 0 || t.cs_change != 0) {
        rhu_shim_log("/dev/spidev%u.%u: a post-transfer delay or a held select refused (EINVAL: the kernel releases the select after every exchange)", c->bus, c->select);
        return refuse(RHU_GERR_EINVAL);
    }
    if (t.tx_nbits > 1 || t.rx_nbits > 1) {
        rhu_shim_log("/dev/spidev%u.%u: a dual or quad transfer refused (EINVAL: one data line each way)", c->bus, c->select);
        return refuse(RHU_GERR_EINVAL);
    }
    uint32_t select = (c->mode & BUSDEV_SPI_NO_CS) != 0 ? RHU_SPI_SELECT_NONE : c->select;
    int32_t rc = rhu_spi_transfer(c->bus, select, c->mode & BUSDEV_SPI_CLOCK_BITS, hz, tx, rx, t.len);
    if (rc > 0) {
        state_rate(n, c, hz, (uint32_t)rc);
#ifdef RHU_SHIM_BREAK_BUSDEV_REREAD
        return (int)((const struct busdev_spi_transfer *)arg)->len;
#else
        return (int)t.len;
#endif
    }
    switch (rc) {
    case RHU_ETIMEDOUT:
        /* The kernel's own line names the byte and the reset. */
        break;
    case RHU_EINVAL:
        rhu_shim_log("/dev/spidev%u.%u: the transfer refused by the kernel (EINVAL: a select the bus does not have, or a rate below the block's slowest)", c->bus, c->select);
        break;
    case RHU_EBUSY:
        rhu_shim_log("/dev/spidev%u.%u: select %u refused (EBUSY: the program holds it as a GPIO line)", c->bus, c->select, c->select);
        break;
    case RHU_ENODEV:
        rhu_shim_log("/dev/spidev%u.%u: the transfer refused (ENODEV: no such bus on this row)", c->bus, c->select);
        break;
    default:
        rhu_shim_log("/dev/spidev%u.%u: the transfer answered %d (EIO)", c->bus, c->select, rc);
        return refuse(RHU_GERR_EIO);
    }
    return refuse_code(rc);
}

static int spi_request(struct busdev_node *n, const struct busdev_node *c, unsigned long request, void *arg)
{
    uint8_t byte;
    switch (request) {
    case BUSDEV_SPI_WR_MODE:
        return arg == NULL ? refuse(RHU_GERR_EFAULT) : spi_set_mode(n, c, arg);
    case BUSDEV_SPI_RD_MODE:
        if (arg == NULL)
            return refuse(RHU_GERR_EFAULT);
        byte = c->mode;
        memcpy(arg, &byte, sizeof byte);
        return 0;
    case BUSDEV_SPI_WR_BITS:
        return arg == NULL ? refuse(RHU_GERR_EFAULT) : spi_set_bits(c, arg);
    case BUSDEV_SPI_RD_BITS:
        if (arg == NULL)
            return refuse(RHU_GERR_EFAULT);
        byte = 8;
        memcpy(arg, &byte, sizeof byte);
        return 0;
    case BUSDEV_SPI_WR_SPEED:
        return arg == NULL ? refuse(RHU_GERR_EFAULT) : spi_set_speed(n, c, arg);
    case BUSDEV_SPI_RD_SPEED:
        if (arg == NULL)
            return refuse(RHU_GERR_EFAULT);
        memcpy(arg, &c->hz, sizeof c->hz);
        return 0;
    default:
        break;
    }
    if ((request & ~(unsigned long)BUSDEV_SPI_SIZE_MASK) == BUSDEV_SPI_MESSAGE_BASE)
        return spi_transfer(n, c, request, arg);
    rhu_shim_log("/dev/spidev%u.%u: request 0x%x not served (ENOTTY)", c->bus, c->select, (uint32_t)request);
    return refuse(RHU_GERR_ENOTTY);
}

/* ------------------------------------------------------------------ */
/* the request dispatch                                                */
/* ------------------------------------------------------------------ */

/* The order of a request's checks: the descriptor (EBADF), then the
 * request's number (ENOTTY for one not served, whatever its argument),
 * then the memory that request needs (EFAULT for a NULL). */
int rhu_busdev_ioctl(int fd, unsigned long request, void *arg)
{
    struct busdev_node c;
    struct busdev_node *n = node_enter(fd, &c);
    if (n == NULL)
        return -1;
    int rc = c.kind == KIND_I2C ? i2c_request(&c, request, arg) : spi_request(n, &c, request, arg);
    node_leave(n);
    return rc;
}
