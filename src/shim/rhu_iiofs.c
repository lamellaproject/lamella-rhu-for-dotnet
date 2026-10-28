/* The analog input device files over the kernel's ADC calls: the small
 * tree of text files Linux's industrial I/O class gives a chip's converter,
 * presented here so a program that reads a converter through those files
 * reads it unchanged on this system, and so the board package's analog
 * input class reaches the converter through the stock file classes with no
 * native call of its own. The runtime's portability layer hands a path
 * under /sys/bus to this unit and every descriptor of the analog band to
 * its verbs; the C library's access does the same for a path under
 * /sys/bus.
 *
 * This file is an implementation written from scratch that presents the
 * established device-file surface. The file names and the texts are the
 * Linux industrial I/O class's documented names; the code here maps them
 * onto the kernel's six ADC calls.
 *
 * THE TREE. /sys/bus, /sys/bus/iio and /sys/bus/iio/devices exist on every
 * row. Each converter block the kernel's table states is one directory
 * iio:device<k>, k the block's position in the order the kernel's channel
 * table first names it (a row with one block has iio:device0; a row whose
 * table is empty has the devices directory alone). A device directory
 * carries three kinds of file, each one text with a trailing newline, each
 * read-only:
 *   name              the kernel's own block number, as adc<number>
 *   in_voltage_scale  the millivolts one raw count stands for, the block's
 *                     reference voltage over its full scale, printed with
 *                     nine digits after the point (0.805664062 for 3300 mV
 *                     at twelve bits)
 *   in_voltage<N>_raw one channel: N is the chip's own channel number on
 *                     the block, as the kernel's table states it; an
 *                     internal source (a temperature sensor, a reference)
 *                     is a channel like any other
 *
 * THE VERBS, as Linux has them where it has them. A read of a raw file at
 * offset 0 runs ONE conversion through the kernel's read call and answers
 * the sample as decimal digits and a newline; a read at a later offset
 * answers the rest of the text the last conversion printed, and a read at
 * or past its end answers 0 bytes, so a program reads again by seeking to
 * 0 or by a positional read at 0, as a sysfs attribute is read again. The
 * name and scale files answer their texts the same way and run nothing.
 * Each file stats as a regular file of 4096 bytes (the size Linux reports
 * for an attribute), truncates to any length and locks without effect; the
 * directories stat as directories; every file refuses an open for writing.
 *
 * THE ONE DEPARTURE: on Linux the converter belongs to the kernel and any
 * process reads a raw file at any time; here the open of a raw file CLAIMS
 * the channel through the kernel's claim call for the descriptor's life --
 * the channel's pin is put in analog mode and its digital function and the
 * channel are refused to everyone else meanwhile -- and the close releases
 * it. A program that keeps the file open owns the input; a program that
 * opens, reads and closes claims, converts and releases in one call. The
 * kernel's refusals are answered as the C library's codes: a channel whose
 * pin a program holds as a GPIO line, or a channel this program holds
 * already, EBUSY at the open; a conversion past the kernel's budget
 * ETIMEDOUT at the read, with the block reset by the kernel; a conversion
 * the converter flags in error on every retry EIO at the read, the sample
 * discarded. The name and scale files
 * claim nothing and open any number of times. Every refusal is one line on
 * the console.
 *
 * The kernel's table is read once at the unit's first use through the
 * count and channel calls and kept. The unit's table and its descriptors
 * are kept under one mutex of the shim's family (a zero-filled slot the
 * core adopts at its first use); the kernel's calls run under it, and the
 * read call's wait is bounded by the kernel's own conversion budget, a few
 * milliseconds at most, so no caller of this unit waits longer than that
 * for another's conversion. Nothing here masks an interrupt. A caller's
 * buffer is written once; a NULL buffer is refused EFAULT. */

#include <stddef.h>
#include <stdint.h>

#include "rhu_shim.h"

#include "rhu.h"
#include "rhu_pal_io_net11_rc2.h"
#include "rhu_pal_errno_net11_rc2.h"

extern int *__errno_location(void);
void *memcpy(void *dst, const void *src, size_t n);
void *memset(void *dst, int c, size_t n);
int strcmp(const char *a, const char *b);
int strncmp(const char *a, const char *b, size_t n);
size_t strlen(const char *s);
int pthread_mutex_lock(void *slot);
int pthread_mutex_unlock(void *slot);

#define ERRNO (*__errno_location())

/* The paths. */
#define IIOFS_BUS "/sys/bus"
#define IIOFS_IIO "/sys/bus/iio"
#define IIOFS_ROOT "/sys/bus/iio/devices"
#define IIOFS_DEVICE_PREFIX "iio:device"
#define IIOFS_PATH_MAX 80
/* The table's reach: the kernel states at most four converter blocks; the
 * largest row in hand offers seven channels across its blocks. */
#define IIOFS_BLOCKS_MAX 4
#define IIOFS_CHANNELS_MAX 64
/* A channel number's reach on a block (the kernel's claims table). */
#define IIOFS_CHANNEL_NUMBER_MAX 32
/* The size Linux reports for a sysfs attribute file. */
#define IIOFS_ATTR_SIZE 4096
/* A text of a file, with its newline and terminator: a sixteen-bit
 * sample's five digits, or a scale's one digit, point and nine digits. */
#define IIOFS_TEXT_MAX 16
/* The kernel's no-line value for an internal source, kept in sixteen
 * bits here. */
#define IIOFS_NO_LINE 0xFFFFu

/* The nodes a path can name. */
enum {
    NODE_NONE = 0,
    NODE_DIR_BUS,
    NODE_DIR_IIO,
    NODE_DIR_ROOT,
    NODE_DIR_DEVICE,
    NODE_NAME,
    NODE_SCALE,
    NODE_RAW
};

typedef struct iiofs_node {
    uint8_t kind;
    uint8_t device;  /* the block's position in the devices directory */
    uint8_t channel; /* the chip's channel number (NODE_RAW) */
} iiofs_node;

/* A converter block as the kernel's table states it. */
typedef struct iiofs_block {
    uint32_t number;
    uint32_t bits;
    uint32_t reference_mv;
} iiofs_block;

/* A channel: its block's position, the chip's channel number on it, its
 * kernel index (the enumeration position, which the claim call takes) and
 * the GPIO line it reads (IIOFS_NO_LINE for an internal source). */
typedef struct iiofs_channel {
    uint8_t device;
    uint8_t channel;
    uint8_t index;
    uint8_t pad;
    uint16_t line;
    uint16_t pad2;
} iiofs_channel;

/* A descriptor of the analog band: the node, the access mode, the
 * position, the kernel's handle for a raw file (-1 otherwise) and the text
 * the last read at offset 0 printed. */
typedef struct iiofs_fd {
    uint8_t used;
    uint8_t kind;
    uint8_t device;
    uint8_t channel;
    uint8_t access;
    uint8_t len;
    uint16_t pad;
    int32_t handle;
    uint32_t pos;
    char text[IIOFS_TEXT_MAX];
} iiofs_fd;

static iiofs_block g_blocks[IIOFS_BLOCKS_MAX];
static iiofs_channel g_channels[IIOFS_CHANNELS_MAX];
static iiofs_fd g_fd[RHU_SHIM_FD_IIO_COUNT];
static uint8_t g_mutex[24];
/* The counts, read once from the kernel's table (-1 before the first
 * use). */
static int32_t g_nblocks = -1;
static int32_t g_nchannels;

static int refuse(int32_t err)
{
    ERRNO = err;
    return -1;
}

static int table_lock(void)
{
    if (pthread_mutex_lock(g_mutex) == 0)
        return 0;
    rhu_shim_log("analog input files: the table's lock refused (ENOLCK)");
    ERRNO = RHU_GERR_ENOLCK;
    return -1;
}

static void table_unlock(void)
{
    pthread_mutex_unlock(g_mutex);
}

/* The kernel's table, read once through the count and channel calls (under
 * the lock): every block in the order the channel table first names it,
 * with its reference voltage; every channel with its block's position. */
static void enumerate_locked(void)
{
    if (g_nblocks >= 0)
        return;
    int32_t count = rhu_adc_count();
    int32_t nb = 0, nc = 0;
    for (int32_t i = 0; i < count; i++) {
        uint32_t block = 0, channel = 0, line = 0, bits = 0;
        if (rhu_adc_channel((uint32_t)i, &block, &channel, &line, &bits) != 0)
            continue;
        int32_t d = -1;
        for (int32_t k = 0; k < nb; k++) {
            if (g_blocks[k].number == block)
                d = k;
        }
        if (d < 0) {
            if (nb >= IIOFS_BLOCKS_MAX) {
                rhu_shim_log("%s: the kernel states more than %u converter blocks; block %u is not presented", IIOFS_ROOT, (uint32_t)IIOFS_BLOCKS_MAX, block);
                continue;
            }
            uint32_t mv = 0;
            if (rhu_adc_reference(block, &mv) != 0)
                mv = 0;
            d = nb++;
            g_blocks[d].number = block;
            g_blocks[d].bits = bits;
            g_blocks[d].reference_mv = mv;
        }
        if (nc >= IIOFS_CHANNELS_MAX || i > 255 || channel >= IIOFS_CHANNEL_NUMBER_MAX) {
            rhu_shim_log("%s: the kernel's channel %d (block %u, channel %u) is past this unit's table and is not presented", IIOFS_ROOT, i, block, channel);
            continue;
        }
        g_channels[nc].device = (uint8_t)d;
        g_channels[nc].channel = (uint8_t)channel;
        g_channels[nc].index = (uint8_t)i;
        g_channels[nc].line = line == RHU_ADC_NO_LINE || line > 0xFFFEu ? IIOFS_NO_LINE : (uint16_t)line;
        nc++;
    }
    g_nblocks = nb;
    g_nchannels = nc;
}

/* The table entry of a device's channel number, or NULL. */
static const iiofs_channel *channel_locked(uint8_t device, uint8_t channel)
{
    for (int32_t i = 0; i < g_nchannels; i++) {
        if (g_channels[i].device == device && g_channels[i].channel == channel)
            return &g_channels[i];
    }
    return NULL;
}

/* A decimal number: digits alone, no sign, no leading zero but "0" itself. */
static int parse_number(const char **p, uint32_t *out)
{
    const char *s = *p;
    uint32_t v = 0;
    if (*s < '0' || *s > '9')
        return 0;
    if (*s == '0' && s[1] >= '0' && s[1] <= '9')
        return 0;
    while (*s >= '0' && *s <= '9') {
        uint32_t d = (uint32_t)(*s - '0');
        if (v > (0xFFFFFFFFu - d) / 10u)
            return 0;
        v = v * 10u + d;
        s++;
    }
    *p = s;
    *out = v;
    return 1;
}

/* A path to its node, with one trailing slash dropped; a device directory
 * and its files exist only as the table says, which the caller judges. */
static int parse_path(const char *path, iiofs_node *n)
{
    char buf[IIOFS_PATH_MAX];
    n->kind = NODE_NONE;
    n->device = 0;
    n->channel = 0;
    if (path == NULL)
        return 0;
    size_t len = strlen(path);
    if (len == 0 || len >= IIOFS_PATH_MAX)
        return 0;
    memcpy(buf, path, len + 1);
    if (len > 1 && buf[len - 1] == '/')
        buf[--len] = 0;
    if (strcmp(buf, IIOFS_BUS) == 0) {
        n->kind = NODE_DIR_BUS;
        return 1;
    }
    if (strcmp(buf, IIOFS_IIO) == 0) {
        n->kind = NODE_DIR_IIO;
        return 1;
    }
    if (strcmp(buf, IIOFS_ROOT) == 0) {
        n->kind = NODE_DIR_ROOT;
        return 1;
    }
    size_t rl = strlen(IIOFS_ROOT);
    if (strncmp(buf, IIOFS_ROOT, rl) != 0 || buf[rl] != '/')
        return 0;
    const char *p = buf + rl + 1;
    size_t dl = strlen(IIOFS_DEVICE_PREFIX);
    if (strncmp(p, IIOFS_DEVICE_PREFIX, dl) != 0)
        return 0;
    p += dl;
    uint32_t device;
    if (!parse_number(&p, &device) || device >= IIOFS_BLOCKS_MAX)
        return 0;
    n->device = (uint8_t)device;
    if (*p == 0) {
        n->kind = NODE_DIR_DEVICE;
        return 1;
    }
    if (*p != '/')
        return 0;
    p++;
    if (strcmp(p, "name") == 0) {
        n->kind = NODE_NAME;
        return 1;
    }
    if (strcmp(p, "in_voltage_scale") == 0) {
        n->kind = NODE_SCALE;
        return 1;
    }
    if (strncmp(p, "in_voltage", 10) != 0)
        return 0;
    p += 10;
    uint32_t channel;
    if (!parse_number(&p, &channel) || channel >= IIOFS_CHANNEL_NUMBER_MAX)
        return 0;
    if (strcmp(p, "_raw") != 0)
        return 0;
    n->channel = (uint8_t)channel;
    n->kind = NODE_RAW;
    return 1;
}

static int node_is_dir(uint8_t kind)
{
    return kind == NODE_DIR_BUS || kind == NODE_DIR_IIO || kind == NODE_DIR_ROOT || kind == NODE_DIR_DEVICE;
}

/* Whether the node exists on this row (under the lock). */
static int node_exists_locked(const iiofs_node *n)
{
    enumerate_locked();
    switch (n->kind) {
    case NODE_DIR_BUS:
    case NODE_DIR_IIO:
    case NODE_DIR_ROOT:
        return 1;
    case NODE_DIR_DEVICE:
    case NODE_NAME:
    case NODE_SCALE:
        return n->device < g_nblocks;
    case NODE_RAW:
        return n->device < g_nblocks && channel_locked(n->device, n->channel) != NULL;
    default:
        return 0;
    }
}

/* The permission bits of a node: 0444 for every file, 0755 for a
 * directory. */
static uint32_t node_mode(uint8_t kind)
{
    return node_is_dir(kind) ? 0x1EDu : 0x124u;
}

static void status_of(rhu_pal_filestatus *st, const iiofs_node *n)
{
    memset(st, 0, sizeof *st);
    if (node_is_dir(n->kind)) {
        st->Mode = (int32_t)(RHU_PAL_S_IFDIR | node_mode(n->kind));
    } else {
        st->Mode = (int32_t)(RHU_PAL_S_IFREG | node_mode(n->kind));
        st->Size = IIOFS_ATTR_SIZE;
    }
    st->Ino = 0x60000 + (int64_t)n->kind * (IIOFS_BLOCKS_MAX * IIOFS_CHANNEL_NUMBER_MAX) + (int64_t)n->device * IIOFS_CHANNEL_NUMBER_MAX + n->channel;
    st->Dev = 2;
    st->HardLinkCount = 1;
}

/* A number to decimal text with a newline; the length. */
static uint32_t put_number(char *buf, uint64_t v)
{
    char tmp[24];
    uint32_t k = 0;
    do {
        tmp[k++] = (char)('0' + (v % 10u));
        v /= 10u;
    } while (v != 0);
    uint32_t len = 0;
    while (k > 0)
        buf[len++] = tmp[--k];
    buf[len++] = '\n';
    buf[len] = 0;
    return len;
}

/* The scale's text: the reference in millivolts over the full scale, as
 * the integer part, a point and nine digits (the fraction truncated), with
 * a newline; the length. */
static uint32_t put_scale(char *buf, const iiofs_block *b)
{
    uint32_t bits = b->bits;
#ifdef RHU_SHIM_BREAK_IIOFS_SCALE_BITS
    /* A test knob: the scale computed for one bit less than the width.
     * Never set in a hosted image. */
    bits = bits > 1 ? bits - 1 : bits;
#endif
    uint64_t e9 = ((uint64_t)b->reference_mv * 1000000000ull) >> (bits > 63 ? 63 : bits);
    uint64_t whole = e9 / 1000000000ull;
    uint64_t frac = e9 % 1000000000ull;
    uint32_t len = put_number(buf, whole);
    buf[len - 1] = '.';
    for (int i = 8; i >= 0; i--) {
        buf[len + (uint32_t)i] = (char)('0' + (frac % 10u));
        frac /= 10u;
    }
    len += 9;
    buf[len++] = '\n';
    buf[len] = 0;
    return len;
}

/* The text of the name file: adc<number> with a newline; the length. */
static uint32_t put_name(char *buf, const iiofs_block *b)
{
    memcpy(buf, "adc", 3);
    return 3 + put_number(buf + 3, b->number);
}

/* The kernel's refusal of a read as the C library's code, with its line:
 * the budget passed, ETIMEDOUT (the block reset by the kernel); the
 * converter flagging the conversion in error on every retry, EIO (the
 * sample discarded); a descriptor the kernel does not hold, EBADF. */
static int32_t read_refused(const iiofs_fd *h, int32_t rc)
{
    if (rc == RHU_ETIMEDOUT) {
        rhu_shim_log("%s/iio:device%u/in_voltage%u_raw: the conversion did not complete within its budget; the converter was reset (ETIMEDOUT)", IIOFS_ROOT, h->device, h->channel);
        return RHU_GERR_ETIMEDOUT;
    }
    if (rc == RHU_EIO) {
        rhu_shim_log("%s/iio:device%u/in_voltage%u_raw: the converter flagged the conversion in error on every retry; the sample is discarded (EIO)", IIOFS_ROOT, h->device, h->channel);
        return RHU_GERR_EIO;
    }
    if (rc == RHU_EINVAL) {
        rhu_shim_log("%s/iio:device%u/in_voltage%u_raw: the kernel does not hold this descriptor's channel (EBADF)", IIOFS_ROOT, h->device, h->channel);
        return RHU_GERR_EBADF;
    }
    rhu_shim_log("%s/iio:device%u/in_voltage%u_raw: the read answered %d (EIO)", IIOFS_ROOT, h->device, h->channel, rc);
    return RHU_GERR_EIO;
}

/* The text a read at offset 0 prints into the descriptor (under the
 * lock): one conversion for a raw file, the constant texts otherwise; 0,
 * or the C library's code. */
static int32_t print_locked(iiofs_fd *h)
{
    if (h->kind == NODE_RAW) {
#ifdef RHU_SHIM_BREAK_IIOFS_READ_STALE
        /* A test knob: a read after the first answers the text of the last
         * conversion without running a new one. Never set in a hosted
         * image. */
        if (h->len != 0)
            return 0;
#endif
        int32_t s = rhu_adc_read(h->handle);
        if (s < 0)
            return read_refused(h, s);
        h->len = (uint8_t)put_number(h->text, (uint64_t)s);
        return 0;
    }
    if (h->kind == NODE_SCALE)
        h->len = (uint8_t)put_scale(h->text, &g_blocks[h->device]);
    else
        h->len = (uint8_t)put_name(h->text, &g_blocks[h->device]);
    return 0;
}

/* ------------------------------------------------------------------ */
/* the path verbs                                                      */
/* ------------------------------------------------------------------ */

int rhu_iiofs_owns(const char *path)
{
    return path != NULL && strncmp(path, IIOFS_BUS, 8) == 0 && (path[8] == 0 || path[8] == '/');
}

int32_t rhu_iiofs_stat(const char *path, rhu_pal_filestatus *output)
{
    iiofs_node n;
    if (output == NULL)
        return refuse(RHU_GERR_EFAULT);
    if (!parse_path(path, &n)) {
        memset(output, 0, sizeof *output);
        return refuse(RHU_GERR_ENOENT);
    }
    if (table_lock() != 0)
        return -1;
    int exists = node_exists_locked(&n);
    table_unlock();
    if (!exists) {
        memset(output, 0, sizeof *output);
        return refuse(RHU_GERR_ENOENT);
    }
    status_of(output, &n);
    return 0;
}

int32_t rhu_iiofs_access(const char *path, int32_t mode)
{
    iiofs_node n;
    if (!parse_path(path, &n))
        return refuse(RHU_GERR_ENOENT);
    if (table_lock() != 0)
        return -1;
    int exists = node_exists_locked(&n);
    table_unlock();
    if (!exists)
        return refuse(RHU_GERR_ENOENT);
    uint32_t bits = node_mode(n.kind);
    if ((mode & RHU_PAL_R_OK) != 0 && (bits & 0x100u) == 0)
        return refuse(RHU_GERR_EACCES);
    if ((mode & RHU_PAL_W_OK) != 0 && (bits & 0x080u) == 0)
        return refuse(RHU_GERR_EACCES);
    if ((mode & RHU_PAL_X_OK) != 0 && (bits & 0x040u) == 0)
        return refuse(RHU_GERR_EACCES);
    return 0;
}

intptr_t rhu_iiofs_open(const char *path, int32_t flags)
{
    iiofs_node n;
    int32_t access = flags & RHU_PAL_O_ACCESS_MODE_MASK;
    if (!parse_path(path, &n)) {
        rhu_shim_log("open(%s): no such file under %s (%s)", path != NULL ? path : "(null)", IIOFS_BUS, (flags & RHU_PAL_O_CREAT) != 0 ? "EACCES: nothing is created there" : "ENOENT");
        return refuse((flags & RHU_PAL_O_CREAT) != 0 ? RHU_GERR_EACCES : RHU_GERR_ENOENT);
    }
    if (table_lock() != 0)
        return -1;
    if (!node_exists_locked(&n)) {
        table_unlock();
        rhu_shim_log("open(%s): no such file under %s (%s)", path, IIOFS_BUS, (flags & RHU_PAL_O_CREAT) != 0 ? "EACCES: nothing is created there" : "ENOENT");
        return refuse((flags & RHU_PAL_O_CREAT) != 0 ? RHU_GERR_EACCES : RHU_GERR_ENOENT);
    }
    if (node_is_dir(n.kind)) {
        table_unlock();
        return refuse(RHU_GERR_EISDIR);
    }
    if ((flags & RHU_PAL_O_CREAT) != 0 && (flags & RHU_PAL_O_EXCL) != 0) {
        table_unlock();
        return refuse(RHU_GERR_EEXIST);
    }
    if (access != RHU_PAL_O_RDONLY) {
        table_unlock();
        rhu_shim_log("open(%s): the file is read-only (EACCES)", path);
        return refuse(RHU_GERR_EACCES);
    }
    int di = -1;
    for (int i = 0; i < RHU_SHIM_FD_IIO_COUNT && di < 0; i++) {
        if (g_fd[i].used == 0)
            di = i;
    }
    if (di < 0) {
        table_unlock();
        rhu_shim_log("open(%s): the analog band's %u descriptors are all open (EMFILE)", path, (uint32_t)RHU_SHIM_FD_IIO_COUNT);
        return refuse(RHU_GERR_EMFILE);
    }
    int32_t handle = -1;
    if (n.kind == NODE_RAW) {
        const iiofs_channel *c = channel_locked(n.device, n.channel);
        handle = rhu_adc_claim(c->index);
#ifdef RHU_SHIM_BREAK_IIOFS_OPEN_UNJUDGED
        /* A test knob: a channel the kernel refused as held is opened as if
         * the claim had answered its handle. Never set in a hosted image. */
        if (handle == RHU_EBUSY)
            handle = (int32_t)c->index;
#endif
        if (handle == RHU_EBUSY) {
            table_unlock();
            if (c->line != IIOFS_NO_LINE)
                rhu_shim_log("open(%s): the analog input is held (EBUSY: a program holds GPIO line %u as a GPIO line, or this program already holds the channel)", path, (uint32_t)c->line);
            else
                rhu_shim_log("open(%s): the analog input is held (EBUSY: this program already holds the channel)", path);
            return refuse(RHU_GERR_EBUSY);
        }
        if (handle == RHU_EINVAL) {
            table_unlock();
            rhu_shim_log("open(%s): the kernel states no such channel (ENOENT)", path);
            return refuse(RHU_GERR_ENOENT);
        }
        if (handle < 0) {
            table_unlock();
            rhu_shim_log("open(%s): the claim answered %d (EIO)", path, handle);
            return refuse(RHU_GERR_EIO);
        }
    }
    memset(&g_fd[di], 0, sizeof g_fd[di]);
    g_fd[di].used = 1;
    g_fd[di].kind = n.kind;
    g_fd[di].device = n.device;
    g_fd[di].channel = n.channel;
    g_fd[di].access = (uint8_t)access;
    g_fd[di].handle = handle;
    table_unlock();
    return RHU_SHIM_FD_IIO_LO + di;
}

/* ------------------------------------------------------------------ */
/* the descriptor verbs                                                */
/* ------------------------------------------------------------------ */

int rhu_iiofs_fd(intptr_t fd)
{
    return fd >= RHU_SHIM_FD_IIO_LO && fd < RHU_SHIM_FD_IIO_LO + RHU_SHIM_FD_IIO_COUNT;
}

/* The descriptor's entry under the lock, or NULL with EBADF set. */
static iiofs_fd *fd_entry_locked(intptr_t fd)
{
    if (!rhu_iiofs_fd(fd) || g_fd[fd - RHU_SHIM_FD_IIO_LO].used == 0) {
        ERRNO = RHU_GERR_EBADF;
        return NULL;
    }
    return &g_fd[fd - RHU_SHIM_FD_IIO_LO];
}

int32_t rhu_iiofs_close(intptr_t fd)
{
    if (table_lock() != 0)
        return -1;
    iiofs_fd *h = fd_entry_locked(fd);
    if (h == NULL) {
        table_unlock();
        return -1;
    }
#ifndef RHU_SHIM_BREAK_IIOFS_CLOSE_KEEPS
    if (h->kind == NODE_RAW) {
        int32_t rc = rhu_adc_release(h->handle);
        if (rc != 0)
            rhu_shim_log("%s/iio:device%u/in_voltage%u_raw: the release answered %d at the close; the descriptor is dropped", IIOFS_ROOT, h->device, h->channel, rc);
    }
#else
    /* A test knob: the close drops the descriptor and releases nothing.
     * Never set in a hosted image. */
#endif
    memset(h, 0, sizeof *h);
    table_unlock();
    return 0;
}

/* A read of the file's text from an offset: the descriptor's position when
 * use_pos is set (and advanced), the offset given otherwise. A read at
 * offset 0 prints the text afresh (one conversion for a raw file); a read
 * at a later offset answers the rest of the text last printed. */
int32_t rhu_iiofs_pread(intptr_t fd, void *buffer, int32_t bufferSize, int64_t offset, int use_pos)
{
    if (buffer == NULL && bufferSize != 0)
        return refuse(RHU_GERR_EFAULT);
    if (bufferSize < 0)
        return refuse(RHU_GERR_EINVAL);
    if (table_lock() != 0)
        return -1;
    iiofs_fd *h = fd_entry_locked(fd);
    if (h == NULL) {
        table_unlock();
        return -1;
    }
    int64_t at = use_pos ? (int64_t)h->pos : offset;
    if (at < 0) {
        table_unlock();
        return refuse(RHU_GERR_EINVAL);
    }
    if (at == 0) {
        int32_t err = print_locked(h);
        if (err != 0) {
            table_unlock();
            return refuse(err);
        }
    }
    int32_t n = 0;
    if (at < (int64_t)h->len) {
        n = (int32_t)(h->len - (uint32_t)at);
        if (n > bufferSize)
            n = bufferSize;
        memcpy(buffer, h->text + at, (size_t)n);
    }
    if (use_pos)
        h->pos += (uint32_t)n;
    table_unlock();
    return n;
}

/* Every file of the tree is read-only, and every descriptor of the band
 * was opened for reading: a write answers EBADF, as a write on a
 * read-only descriptor does. */
int32_t rhu_iiofs_pwrite(intptr_t fd, const void *buffer, int32_t bufferSize, int64_t offset, int use_pos)
{
    (void)offset;
    (void)use_pos;
    if (buffer == NULL && bufferSize != 0)
        return refuse(RHU_GERR_EFAULT);
    if (bufferSize < 0)
        return refuse(RHU_GERR_EINVAL);
    if (table_lock() != 0)
        return -1;
    iiofs_fd *h = fd_entry_locked(fd);
    table_unlock();
    if (h == NULL)
        return -1;
    return refuse(RHU_GERR_EBADF);
}

int64_t rhu_iiofs_lseek(intptr_t fd, int64_t offset, int32_t whence)
{
    if (table_lock() != 0)
        return -1;
    iiofs_fd *h = fd_entry_locked(fd);
    if (h == NULL) {
        table_unlock();
        return -1;
    }
    int64_t base = whence == RHU_PAL_SEEK_SET ? 0
                 : whence == RHU_PAL_SEEK_CUR ? (int64_t)h->pos
                 : whence == RHU_PAL_SEEK_END ? (int64_t)IIOFS_ATTR_SIZE
                                              : -1;
    if (base < 0 || base + offset < 0 || base + offset > IIOFS_ATTR_SIZE) {
        table_unlock();
        return refuse(RHU_GERR_EINVAL);
    }
    h->pos = (uint32_t)(base + offset);
    int64_t r = (int64_t)h->pos;
    table_unlock();
    return r;
}

int32_t rhu_iiofs_fstat(intptr_t fd, rhu_pal_filestatus *output)
{
    if (output == NULL)
        return refuse(RHU_GERR_EFAULT);
    if (table_lock() != 0)
        return -1;
    iiofs_fd *h = fd_entry_locked(fd);
    if (h == NULL) {
        table_unlock();
        memset(output, 0, sizeof *output);
        return -1;
    }
    iiofs_node n;
    n.kind = h->kind;
    n.device = h->device;
    n.channel = h->channel;
    table_unlock();
    status_of(output, &n);
    return 0;
}

/* A truncate, a lock, a sync and a descriptor flag change have no effect on
 * an attribute file and answer 0, as Linux answers them. */
int32_t rhu_iiofs_ftruncate(intptr_t fd, int64_t length)
{
    if (length < 0)
        return refuse(RHU_GERR_EINVAL);
    if (table_lock() != 0)
        return -1;
    iiofs_fd *h = fd_entry_locked(fd);
    table_unlock();
    return h != NULL ? 0 : -1;
}

int32_t rhu_iiofs_flock(intptr_t fd, int32_t operation)
{
    (void)operation;
    if (table_lock() != 0)
        return -1;
    iiofs_fd *h = fd_entry_locked(fd);
    table_unlock();
    return h != NULL ? 0 : -1;
}

int32_t rhu_iiofs_fsync(intptr_t fd)
{
    if (table_lock() != 0)
        return -1;
    iiofs_fd *h = fd_entry_locked(fd);
    table_unlock();
    return h != NULL ? 0 : -1;
}

int32_t rhu_iiofs_fcntl_setfd(intptr_t fd, int32_t flags)
{
    (void)flags;
    if (table_lock() != 0)
        return -1;
    iiofs_fd *h = fd_entry_locked(fd);
    table_unlock();
    return h != NULL ? 0 : -1;
}

/* A descriptor of this band has no second name at this hosting, as the
 * store's descriptors have none. */
intptr_t rhu_iiofs_dup(intptr_t fd)
{
    if (table_lock() != 0)
        return -1;
    iiofs_fd *h = fd_entry_locked(fd);
    table_unlock();
    if (h == NULL)
        return -1;
    rhu_shim_log("dup: an analog input file descriptor has no second name at this hosting (ENOTSUP)");
    return refuse(RHU_GERR_ENOTSUP);
}
