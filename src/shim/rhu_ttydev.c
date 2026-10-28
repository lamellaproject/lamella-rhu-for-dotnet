/* The serial device files over the kernel's UART API: the Linux device
 * files a program's serial port class opens (/dev/ttyS<n>, the chip's own
 * block number, and the alias /dev/ttyAMA<n>), presented here so the
 * standard class (System.IO.Ports.SerialPort at the pinned runtime, its
 * native layer compiled unchanged from the vendored sources under
 * src/vendor/ports/) runs unchanged on this system. The core's C-library
 * open hands a path that starts with one of the two prefixes to this unit,
 * and the core's ioctl and close and the glue's read and write hand every
 * descriptor of the tty band here; the C-library names the stock layer
 * reaches for beyond those -- poll, fcntl, shutdown, the termios family,
 * the error string -- are defined in this unit, so every undefined name of
 * the stock layer's object is answered by the shim's own objects and no
 * member of the kit's C library is pulled into a pinned closure.
 *
 * This file is an independent implementation that presents the
 * established device-file surface: the termios shape of a Linux serial
 * port as the stock layer's own source uses it. The request numbers,
 * the flag bits and the record layouts are Linux ARM32's, read from the
 * platform headers under src/shim/ports/ that the stock layer compiles
 * against, so the layer and this unit read one image.
 *
 * ONE TERMIOS IMAGE PER DESCRIPTOR: the stock layer's get-modify-set round
 * trip (tcgetattr, cfmakeraw, the flag edits, cfsetspeed, tcsetattr) reads
 * back exactly what it set. tcsetattr validates the whole struct first and
 * refuses it whole, the image untouched, when it asks for what no port here
 * has (RTS/CTS flow control, software flow control, split speeds, the
 * hang-up speed B0); then the five values the kernel's configure door takes
 * are derived (the rate from the speed constant or, on the termios2 form,
 * from the speed word; the data bits from CSIZE; the parity from PARENB,
 * PARODD and CMSPAR; the stop bits from CSTOPB); the when discipline runs
 * whether or not the values move, as Linux acts on the queues (drain: the
 * pending transmit drained within its bound; flush: drained, then the
 * receive side discarded; now: nothing here, since the configure door
 * itself lets the pending transmit leave at the old rate before the block
 * is reprogrammed); and when any of the five differs from the line's, ONE
 * call of rhu_uart_configure; the door's refusal is the caller's EINVAL
 * with one line naming the reason, and on success the image takes the
 * struct whole and one line states the achieved rate. Every other flag of
 * the struct is accepted and kept without effect: the line is raw (the
 * stock layer sets it so), and nothing of a line discipline runs here.
 *
 * read drains the receive ring; an empty port answers EAGAIN in
 * non-blocking mode (NEVER 0, which the stock layer reads as the end of
 * the stream) and in blocking mode waits on the port's event word under
 * the four Linux VMIN/VTIME cases (0 only at a timer's end, never because
 * another reader or a flush took the bytes the wait saw). write takes what
 * the transmit ring has room for (a partial count is right; the layer
 * slices and continues), EAGAIN when it has none in non-blocking mode,
 * and in blocking mode waits for room until every byte is taken (an error
 * after a take answers the count taken, so no byte is sent twice). poll
 * reports readable when bytes are pending and writable when the ring has
 * room, never an error or a hang-up on a live port, invalid for a closed
 * one; with one tty descriptor the thread sleeps on that port's event word
 * with the deadline (the stock loop's one-millisecond cadence), with
 * several descriptors the sleep is sliced at one millisecond and every
 * descriptor re-tested. The modem lines: no port here has them, so the
 * state is the serial core's for such a port -- clear to send, data set
 * ready and carrier always asserted, DTR and RTS latched in the descriptor
 * and moving nothing on any wire, one line the first time either is
 * changed. tcdrain waits for the transmitter on the event word and,
 * because no interrupt fires when the last character leaves the wire, its
 * last leg is timed: the bytes pending times the frame time at the line's
 * rate, doubled, plus ten milliseconds, re-armed whenever the count moves;
 * past the bound EIO. A break holds the line after a drain for 250 ms, or
 * the asked time (tcsendbreak's milliseconds, rounded up to tenths of a
 * second as the C library forms the request; the TCSBRKP request's tenths
 * as given; no bound on the duration), in slices with the descriptor
 * re-tested at each, so a close under the break ends it (the release
 * drops the line) and the request answers EBADF; a part without break
 * control (the emulator rows) answers 0 with one line, as Linux answers a
 * driver without it, and a descriptor that went away answers EBADF there
 * too (the status door tells the two apart).
 *
 * A descriptor's entry belongs to the requests running on it, in the bus
 * unit's shape: a request takes its entry under the table's mutex, counts
 * itself in, and runs on one copy of the entry. The close is such a
 * request too: it marks the entry closed, drains, releases the block (the
 * release bumps the event word and wakes every waiter, whose next door
 * call the kernel refuses, so the waiter answers EBADF) and counts itself
 * out; the entry is given back when the last request on it ends, and until
 * then a new open of the same block is refused EBUSY, so the block's
 * handle, which the kernel reissues at the next claim, is never driven by
 * a request of the old session. One opener always: the kernel's claim
 * refuses a block already held, so a second open of the same name is
 * EBUSY.
 *
 * The limits, plainly: sixteen descriptors (EMFILE past them, with a
 * line); a port's number is its block's number on the chip, as Linux names
 * it; a block the kernel's console holds, or one whose pins are not named
 * (the Pico rows' second block until a project names a legal pair), is
 * refused EBUSY at the open with the reason; the emulator rows' ports take
 * 8N1 alone and refuse every other frame; the kernel judges the rate (a
 * rate the part misses by more than two percent is refused); no modem
 * line is wired; no line discipline runs; no TIOCMIWAIT; every other
 * request is ENOTTY with its number in a line. Nothing here masks
 * anything: every call runs on the caller's thread, every wait is a sleep
 * on the kernel's event word or a timed sleep, and the table's mutex holds
 * for a few instructions with no door call under it. */
#include <stddef.h>
#include <stdint.h>
#include <fcntl.h>

#include "ports/termios.h"
#include "ports/sys/ioctl.h"
#include "ports/asm/termbits.h"
#include "ports/asm/ioctls.h"
#include "ports/poll.h"
#include "ports/sys/socket.h"
#include "ports/netdb.h"

#include "rhu_shim.h"

#include "rhu.h"
#include "rhu_pal_errno_net11_rc2.h"

extern int *__errno_location(void);
void *memcpy(void *dst, const void *src, size_t n);
void *memset(void *dst, int c, size_t n);
int strncmp(const char *a, const char *b, size_t n);
size_t strlen(const char *s);
int pthread_mutex_lock(void *slot);
int pthread_mutex_unlock(void *slot);

#define ERRNO (*__errno_location())
#define NS_PER_MS 1000000ull
#define NS_PER_S 1000000000ull

/* A port's entry: free; taken by an open whose claim is still running;
 * live; closed while requests still run on it. */
enum { TTY_FREE = 0, TTY_OPENING = 1, TTY_LIVE = 2, TTY_CLOSING = 3 };

/* The modem bits a program may latch. */
#define TTY_LATCH_BITS (TIOCM_DTR | TIOCM_RTS)
/* The lines the serial core asserts for a port without modem lines. */
#define TTY_ALWAYS_BITS (TIOCM_CTS | TIOCM_DSR | TIOCM_CAR)

/* The claim's line state (rhu.h): 115200 8N1. */
#define TTY_CLAIM_RATE 115200u
#define TTY_BREAK_MS 250ull
#define TTY_DRAIN_SLACK_NS (10ull * NS_PER_MS)
#define TTY_POLL_SLICE_NS NS_PER_MS
/* The most one read or write takes: a count that stays a positive long. */
#define TTY_COUNT_MAX 0x7FFFFFFFu

struct tty_node {
    uint8_t state;
    uint8_t nonblock;
    uint8_t accmode; /* O_RDONLY, O_WRONLY or O_RDWR */
    uint8_t exclusive;
    uint8_t modem;      /* the latched DTR and RTS bits */
    uint8_t modem_said; /* the one line on the first change */
    uint8_t break_said; /* the one line on a part without break control */
    uint8_t pad;
    uint32_t users;
    int32_t h;
    uint32_t block;
    uint32_t rate;
    uint32_t bits;
    uint32_t parity;
    uint32_t stops;
    uint32_t achieved;
    struct termios image;
};

static struct tty_node g_tty[RHU_SHIM_FD_TTY_COUNT];
static uint8_t g_mutex[24];

static int refuse(int32_t err)
{
    ERRNO = err;
    return -1;
}

/* The port's name for a line: /dev/ttyS<block>. */
static const char *port_name(uint32_t block, char *buf)
{
    static const char prefix[] = "/dev/ttyS";
    size_t i = 0;
    while (prefix[i] != 0) {
        buf[i] = prefix[i];
        i++;
    }
    char digits[11];
    size_t d = 0;
    do {
        digits[d++] = (char)('0' + block % 10u);
        block /= 10u;
    } while (block != 0);
    while (d > 0)
        buf[i++] = digits[--d];
    buf[i] = 0;
    return buf;
}

/* The table's mutex: a zero-filled slot the core adopts at its first use.
 * A lock that fails is a fault of the slot, never the caller's: the request
 * answers ENOLCK with one line and touches nothing the lock guards. */
static int table_lock(void)
{
    if (pthread_mutex_lock(g_mutex) == 0)
        return 1;
    rhu_shim_log("serial ports: the table's lock refused (ENOLCK)");
    ERRNO = RHU_GERR_ENOLCK;
    return 0;
}

static void table_unlock(void)
{
    pthread_mutex_unlock(g_mutex);
}

static int fd_in_band(int fd)
{
    return fd >= RHU_SHIM_FD_TTY_LO && fd < RHU_SHIM_FD_TTY_LO + RHU_SHIM_FD_TTY_COUNT;
}

static int fd_is_console(int fd)
{
    return fd >= RHU_SHIM_FD_CONSOLE_LO && fd < RHU_SHIM_FD_CONSOLE_HI;
}

/* A descriptor of another band the shim serves: the files, the bus device
 * nodes, the PWM device files. What such a descriptor is open to is its
 * own unit's to say; this unit answers for it only what the C library's
 * form of the call allows without looking (ENOTTY for a termios verb; the
 * minimal fcntl answers). */
static int fd_of_other_band(int fd)
{
    return (fd >= RHU_SHIM_FD_FILE_LO && fd < RHU_SHIM_FD_FILE_LO + RHU_SHIM_FD_FILE_COUNT)
        || (fd >= RHU_SHIM_FD_DEVICE_LO && fd < RHU_SHIM_FD_DEVICE_LO + RHU_SHIM_FD_DEVICE_COUNT)
        || (fd >= RHU_SHIM_FD_SYSFS_LO && fd < RHU_SHIM_FD_SYSFS_LO + RHU_SHIM_FD_SYSFS_COUNT);
}

static struct tty_node *entry_of(int fd)
{
    if (!fd_in_band(fd))
        return NULL;
    return &g_tty[fd - RHU_SHIM_FD_TTY_LO];
}

/* A request's entry: the descriptor's live port, counted as in use, with
 * one copy of it taken under the lock; NULL with errno set. */
static struct tty_node *node_enter(int fd, struct tty_node *copy)
{
    if (!table_lock())
        return NULL;
    struct tty_node *n = entry_of(fd);
    if (n == NULL || n->state != TTY_LIVE) {
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
 * when its last request leaves (the close counts as one). */
static void node_leave(struct tty_node *n)
{
    if (!table_lock())
        return;
    if (n->users != 0)
        n->users--;
    if (n->state == TTY_CLOSING && n->users == 0)
        memset(n, 0, sizeof *n);
    table_unlock();
}

/* A termios verb's entry: node_enter, with the C library's errno for a
 * number outside the tty band -- ENOTTY for a descriptor of another band
 * the shim serves (the console, the files, the device nodes), EBADF for
 * any other number and for a closed one of this band. */
static struct tty_node *verb_enter(int fd, struct tty_node *copy)
{
    struct tty_node *n = node_enter(fd, copy);
    if (n == NULL && ERRNO == RHU_GERR_EBADF && (fd_is_console(fd) || fd_of_other_band(fd)))
        ERRNO = RHU_GERR_ENOTTY;
    return n;
}

/* A decimal number of the port's name: digits only, no sign, no leading
 * zero but the number 0 itself, inside 32 bits (the bus unit's parser). */
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

/* The two prefixes this unit owns. A path that starts with either is this
 * unit's whatever follows (a malformed rest is ENOENT here, as it would
 * be on Linux). */
int rhu_ttydev_owns_path(const char *path)
{
    return path != NULL && (strncmp(path, "/dev/ttyS", 9) == 0 || strncmp(path, "/dev/ttyAMA", 11) == 0);
}

/* The name's block number: 1 with the number when the path is /dev/ttyS<n>
 * or /dev/ttyAMA<n> with a well-formed <n> and nothing after it; 0 for any
 * other name. */
static int parse_port_name(const char *path, uint32_t *number)
{
    const char *p;
    if (path == NULL)
        return 0;
    if (strncmp(path, "/dev/ttyS", 9) == 0)
        p = path + 9;
    else if (strncmp(path, "/dev/ttyAMA", 11) == 0)
        p = path + 11;
    else
        return 0;
    return parse_number(&p, number) && *p == 0;
}

/* Whether the board has the block, by the enumeration doors: the rows
 * answer their block numbers; a number none answers is a missing device
 * (the ST rows answer no row). */
static int board_has_block(uint32_t number)
{
    int32_t count = rhu_uart_count();
    for (int32_t i = 0; i < count; i++) {
        uint32_t block, tx, rx, clk;
        if (rhu_uart_port((uint32_t)i, &block, &tx, &rx, &clk) == 0 && block == number)
            return 1;
    }
    return 0;
}

/* The file system's view of a name this unit owns: 1 when the name is a
 * serial port of this board (under either prefix), 0 for a malformed
 * rest or a block no row answers. */
int rhu_ttydev_exists(const char *path)
{
    uint32_t number = 0;
    return parse_port_name(path, &number) && board_has_block(number);
}

/* The index-th row's device file as a listing of /dev shows it: the leaf
 * name "ttyS<block>" written to buf, 1; 0 past the last row (the alias is
 * never listed) or when buf cannot hold the name. */
int rhu_ttydev_name(uint32_t index, char *buf, size_t cap)
{
    uint32_t block, tx, rx, clk;
    int32_t count = rhu_uart_count();
    if (buf == NULL || count <= 0 || index >= (uint32_t)count)
        return 0;
    if (rhu_uart_port(index, &block, &tx, &rx, &clk) != 0)
        return 0;
    char digits[11];
    size_t d = 0;
    do {
        digits[d++] = (char)('0' + block % 10u);
        block /= 10u;
    } while (block != 0);
    if (cap < 4 + d + 1)
        return 0;
    memcpy(buf, "ttyS", 4);
    for (size_t i = 0; i < d; i++)
        buf[4 + i] = digits[d - 1 - i];
    buf[4 + d] = 0;
    return 1;
}

/* ------------------------------------------------------------------ */
/* the speed constants and the five line values                        */
/* ------------------------------------------------------------------ */

static const struct {
    speed_t code;
    uint32_t rate;
} g_speeds[] = {
    {B50, 50},         {B75, 75},           {B110, 110},         {B134, 134},         {B150, 150},
    {B200, 200},       {B300, 300},         {B600, 600},         {B1200, 1200},       {B1800, 1800},
    {B2400, 2400},     {B4800, 4800},       {B9600, 9600},       {B19200, 19200},     {B38400, 38400},
    {B57600, 57600},   {B115200, 115200},   {B230400, 230400},   {B460800, 460800},   {B500000, 500000},
    {B576000, 576000}, {B921600, 921600},   {B1000000, 1000000}, {B1152000, 1152000}, {B1500000, 1500000},
    {B2000000, 2000000}, {B2500000, 2500000}, {B3000000, 3000000}, {B3500000, 3500000}, {B4000000, 4000000},
};

/* A speed constant's rate; 0 for B0 and for a code outside the table. */
static uint32_t rate_of_code(speed_t code)
{
    for (size_t i = 0; i < sizeof g_speeds / sizeof g_speeds[0]; i++) {
        if (g_speeds[i].code == code)
            return g_speeds[i].rate;
    }
    return 0;
}

/* The image of the claim's state: 115200 8N1, raw, the receiver on, the
 * line local, one byte to satisfy a read, no timer. */
static void claim_image(struct termios *t)
{
    memset(t, 0, sizeof *t);
    t->c_cflag = B115200 | CS8 | CREAD | CLOCAL;
    t->c_cc[VMIN] = 1;
    t->c_cc[VTIME] = 0;
    t->c_ispeed = B115200;
    t->c_ospeed = B115200;
}

/* The five values a struct asks for, validated whole: 0 with errno and the line printed on a refusal. The speed
 * words are the C library's form (the constants), or on the termios2 form
 * the rates themselves: BOTHER in the CBAUD bits of the control flags
 * marks the output word as a number, BOTHER in the input bits above
 * IBSHIFT the input word (the ioctl path presents both forms here in the
 * same words). An input word of zero follows the output speed, the C
 * library's convention. */
static int derive_line(const struct termios *t, const char *name, uint32_t *rate, uint32_t *bits, uint32_t *parity, uint32_t *stops)
{
    if ((t->c_cflag & CRTSCTS) != 0) {
        rhu_shim_log("%s: RTS/CTS flow control is not available on this port (EINVAL)", name);
        return 0;
    }
    if ((t->c_iflag & (IXON | IXOFF)) != 0) {
        rhu_shim_log("%s: software flow control is not served on this port (EINVAL)", name);
        return 0;
    }
    speed_t out = t->c_ospeed;
    speed_t in = t->c_ispeed;
    tcflag_t in_bits = (t->c_cflag >> IBSHIFT) & CBAUD;
    int out_number = out == BOTHER || (t->c_cflag & CBAUD) == BOTHER;
    int in_number = out_number || in_bits == BOTHER;
    uint32_t r = out_number ? ((uint32_t)out == BOTHER ? 0u : (uint32_t)out) : rate_of_code(out);
    uint32_t ri = in == 0 ? r : (in_number ? ((uint32_t)in == BOTHER ? 0u : (uint32_t)in) : rate_of_code(in));
    if (ri != r) {
        rhu_shim_log("%s: the input and output speeds differ; one rate per port (EINVAL)", name);
        return 0;
    }
    if (r == 0) {
        rhu_shim_log("%s: the speed B0 (hang up) or an unknown speed constant is not served (EINVAL)", name);
        return 0;
    }
    uint32_t b;
    switch (t->c_cflag & CSIZE) {
    case CS5:
        b = 5;
        break;
    case CS6:
        b = 6;
        break;
    case CS7:
        b = 7;
        break;
    default:
        b = 8;
        break;
    }
    uint32_t p = RHU_UART_PARITY_NONE;
    if ((t->c_cflag & PARENB) != 0) {
        if ((t->c_cflag & CMSPAR) != 0)
            p = (t->c_cflag & PARODD) != 0 ? RHU_UART_PARITY_MARK : RHU_UART_PARITY_SPACE;
        else
            p = (t->c_cflag & PARODD) != 0 ? RHU_UART_PARITY_ODD : RHU_UART_PARITY_EVEN;
    }
    uint32_t s = (t->c_cflag & CSTOPB) != 0 ? 2u : 1u;
    *rate = r;
    *bits = b;
    *parity = p;
    *stops = s;
    return 1;
}

static char parity_letter(uint32_t parity)
{
    switch (parity) {
    case RHU_UART_PARITY_ODD:
        return 'O';
    case RHU_UART_PARITY_EVEN:
        return 'E';
    case RHU_UART_PARITY_MARK:
        return 'M';
    case RHU_UART_PARITY_SPACE:
        return 'S';
    default:
        return 'N';
    }
}

/* ------------------------------------------------------------------ */
/* the waits: the event word, the status door, the deadline            */
/* ------------------------------------------------------------------ */

/* The frame's time on the wire at the line's rate, in nanoseconds: the
 * start bit, the data bits, the parity bit if any, the stop bits. */
static uint64_t frame_ns(const struct tty_node *c)
{
    uint64_t bits = 1u + c->bits + (c->parity != RHU_UART_PARITY_NONE ? 1u : 0u) + c->stops;
    uint64_t rate = c->rate != 0 ? c->rate : TTY_CLAIM_RATE;
    return (bits * NS_PER_S + rate - 1) / rate;
}

/* The transmitter drained: 0 when nothing is pending; otherwise the wait
 * on the event word, the deadline the pending count's frame time doubled
 * plus ten milliseconds, re-armed whenever the count moves (bytes left,
 * or a writer on another thread added); past the bound with the count
 * unmoved, -1 with EIO and one line. A descriptor closed meanwhile is
 * EBADF: the kernel refuses the released handle at the status door. */
static int drain(const struct tty_node *c, const char *name)
{
    const uint32_t *word = rhu_uart_event_word(c->h);
    uint32_t rx, pending, room;
    if (rhu_uart_status(c->h, &rx, &pending, &room) != 0)
        return refuse(RHU_GERR_EBADF);
    if (pending == 0)
        return 0;
    uint32_t last = pending;
    uint64_t bound = rhu_now_ns() + (uint64_t)pending * frame_ns(c) * 2u + TTY_DRAIN_SLACK_NS;
    for (;;) {
        uint32_t v = word != NULL ? *word : 0u;
        if (rhu_uart_status(c->h, &rx, &pending, &room) != 0)
            return refuse(RHU_GERR_EBADF);
        if (pending == 0)
            return 0;
        uint64_t now = rhu_now_ns();
        if (pending != last) {
            last = pending;
            bound = now + (uint64_t)pending * frame_ns(c) * 2u + TTY_DRAIN_SLACK_NS;
        } else if (now >= bound) {
            rhu_shim_log("%s: the transmitter did not drain within its bound (EIO: %u bytes still pending)", name, pending);
            return refuse(RHU_GERR_EIO);
        }
        if (word != NULL) {
            rhu_futex_wait(word, v, bound);
        } else {
            uint64_t left = bound - now;
            rhu_sleep_ns(left > TTY_POLL_SLICE_NS ? TTY_POLL_SLICE_NS : left);
        }
    }
}

/* A wait for the receive side on the port's event word until the test
 * passes or the deadline comes (RHU_DEADLINE_NONE for none): 1 when the
 * test passed, 0 at the deadline, -1 with EBADF when the descriptor went
 * away (the kernel refuses the released handle at the status door). The
 * test: at least `want` bytes pending. */
static int wait_rx(const struct tty_node *c, uint32_t want, uint64_t deadline)
{
    const uint32_t *word = rhu_uart_event_word(c->h);
    for (;;) {
        uint32_t v = word != NULL ? *word : 0u;
        uint32_t rx, pending, room;
        if (rhu_uart_status(c->h, &rx, &pending, &room) != 0) {
            ERRNO = RHU_GERR_EBADF;
            return -1;
        }
        if (rx >= want)
            return 1;
        if (deadline != RHU_DEADLINE_NONE && rhu_now_ns() >= deadline)
            return 0;
        if (word != NULL) {
            int32_t rc = rhu_futex_wait(word, v, deadline);
            if (rc == RHU_ETIMEDOUT)
                return 0;
        } else {
            rhu_sleep_ns(TTY_POLL_SLICE_NS);
        }
    }
}

/* ------------------------------------------------------------------ */
/* open and close                                                      */
/* ------------------------------------------------------------------ */

int rhu_ttydev_open(const char *path, int flags)
{
    uint32_t number = 0;
    if (!parse_port_name(path, &number)) {
        rhu_shim_log("%s: not a serial port name (ENOENT)", path);
        return refuse(RHU_GERR_ENOENT);
    }
    /* The board has the block, by the enumeration doors: a number none of
     * the rows answers is a missing device (the ST rows answer no row). */
    if (!board_has_block(number)) {
        rhu_shim_log("%s: no such serial port on this board (ENOENT)", path);
        return refuse(RHU_GERR_ENOENT);
    }
    int accmode = flags & O_ACCMODE;
    if (accmode != O_RDONLY && accmode != O_WRONLY && accmode != O_RDWR) {
        rhu_shim_log("%s: the access mode %d is not one of the three (EINVAL)", path, accmode);
        return refuse(RHU_GERR_EINVAL);
    }
    if (!table_lock())
        return -1;
    /* The block's own entry first: one that is closed while a request
     * still runs on it is held until that request leaves (the kernel
     * reissues the block's handle at the next claim, and a request of the
     * old session must never drive it); a live or opening one is another
     * opener's. */
    int closing = 0, held = 0, slot = -1;
    for (int i = 0; i < RHU_SHIM_FD_TTY_COUNT; i++) {
        if (g_tty[i].state != TTY_FREE && g_tty[i].block == number) {
            if (g_tty[i].state == TTY_CLOSING)
                closing = 1;
            else
                held = 1;
        } else if (g_tty[i].state == TTY_FREE && slot < 0) {
            slot = i;
        }
    }
    if (closing || held) {
        table_unlock();
        if (closing)
            rhu_shim_log("%s: the port is closing; a request still runs on its closed descriptor (EBUSY)", path);
        else
            rhu_shim_log("%s: the port is held (EBUSY: the kernel's console or another opener)", path);
        return refuse(RHU_GERR_EBUSY);
    }
    if (slot < 0) {
        table_unlock();
        rhu_shim_log("%s: no descriptor free (EMFILE: %d serial descriptors are open)", path, RHU_SHIM_FD_TTY_COUNT);
        return refuse(RHU_GERR_EMFILE);
    }
    struct tty_node *n = &g_tty[slot];
    memset(n, 0, sizeof *n);
    n->state = TTY_OPENING;
    n->block = number;
    table_unlock();
    /* The claim, outside the lock: the block for the program's run, its
     * state 115200 8N1 with the receiver on. */
    int32_t h = rhu_uart_claim(number);
    if (!table_lock()) {
        /* The lock's fault, not the caller's: the claim is given back and
         * the slot reset directly, so nothing leaks past the refusal. */
        if (h >= 0)
            rhu_uart_release(h);
        memset(n, 0, sizeof *n);
        return -1;
    }
    if (h >= 0) {
        n->h = h;
        n->state = TTY_LIVE;
        n->nonblock = (flags & O_NONBLOCK) != 0 ? 1u : 0u;
        n->accmode = (uint8_t)accmode;
        n->modem = TTY_LATCH_BITS;
        n->rate = TTY_CLAIM_RATE;
        n->bits = 8;
        n->parity = RHU_UART_PARITY_NONE;
        n->stops = 1;
        n->achieved = TTY_CLAIM_RATE;
        claim_image(&n->image);
    } else {
        memset(n, 0, sizeof *n);
    }
    table_unlock();
    if (h < 0) {
        if (h == RHU_EBUSY) {
            rhu_shim_log("%s: the port is held (EBUSY: the kernel's console or another opener)", path);
            return refuse(RHU_GERR_EBUSY);
        }
        if (h == RHU_EINVAL) {
            rhu_shim_log("%s: no pins are named for this port (EBUSY: name its pins before opening it)", path);
            return refuse(RHU_GERR_EBUSY);
        }
        if (h == RHU_ENODEV) {
            rhu_shim_log("%s: no such serial port on this board (ENOENT)", path);
            return refuse(RHU_GERR_ENOENT);
        }
        rhu_shim_log("%s: the port refused (EIO: the claim answered %d)", path, h);
        return refuse(RHU_GERR_EIO);
    }
    return RHU_SHIM_FD_TTY_LO + slot;
}

int rhu_ttydev_close(int fd)
{
    char name[24];
    if (!table_lock())
        return -1;
    struct tty_node *n = entry_of(fd);
    if (n == NULL || n->state != TTY_LIVE) {
        table_unlock();
        return refuse(RHU_GERR_EBADF);
    }
    /* The close is a request on the entry like any other: counted in, so
     * the entry is held through the drain and the release and goes to no
     * new opener until the last request on it, this one or a later leaver,
     * has left. */
    n->state = TTY_CLOSING;
    n->users++;
    struct tty_node copy = *n;
    table_unlock();
    /* The pending transmit leaves first (Linux's close waits for the
     * transmitter within its bound); a stalled line is not the close's
     * failure. Then the block is released: disabled and in reset, the rings
     * dropped, the pads and lines returned, the event word bumped so a
     * waiter wakes and finds the descriptor gone. */
    port_name(copy.block, name);
    (void)drain(&copy, name);
#ifndef RHU_SHIM_BREAK_TTY_CLOSE_KEEPS
    rhu_uart_release(copy.h);
#endif
    node_leave(n);
    return 0;
}

/* ------------------------------------------------------------------ */
/* read and write                                                      */
/* ------------------------------------------------------------------ */

/* A blocking read's four Linux cases on the port's event word, entered
 * with the ring found empty: the count read; 0 at a timer's end on a
 * silent line; -1 with errno when the descriptor went away. A wait that
 * passed and a read that then finds the ring empty (another reader or a
 * flush took the bytes) waits again: 0 is a timer's answer alone, never
 * an end of stream. */
static long read_blocking(const struct tty_node *c, void *buf, uint32_t want)
{
    uint32_t vmin = c->image.c_cc[VMIN];
    uint32_t vtime = c->image.c_cc[VTIME];
    uint64_t tenths = (uint64_t)vtime * 100ull * NS_PER_MS;
    uint32_t need = vmin < want ? vmin : want;
    if (vmin == 0 && vtime == 0)
        return 0;
    /* With no minimum, one timer bounds the whole read. */
    uint64_t timer = vmin == 0 ? rhu_now_ns() + tenths : RHU_DEADLINE_NONE;
    for (;;) {
        int w;
        if (vmin == 0) {
            w = wait_rx(c, 1, timer);
        } else if (vtime == 0) {
            w = wait_rx(c, need, RHU_DEADLINE_NONE);
        } else {
            /* The first byte without a bound, then until `need` are
             * pending or the inter-byte time passes with no new byte. */
            w = wait_rx(c, 1, RHU_DEADLINE_NONE);
            while (w > 0) {
                uint32_t rx, pending, room;
                if (rhu_uart_status(c->h, &rx, &pending, &room) != 0) {
                    ERRNO = RHU_GERR_EBADF;
                    w = -1;
                    break;
                }
                if (rx >= need)
                    break;
                w = wait_rx(c, rx + 1, rhu_now_ns() + tenths);
                if (w == 0) {
                    w = 1;
                    break;
                }
            }
        }
        if (w < 0)
            return -1;
        if (w == 0)
            return 0;
        int32_t got = rhu_uart_read(c->h, buf, want);
        if (got > 0)
            return got;
        if (got < 0) {
            ERRNO = RHU_GERR_EBADF;
            return -1;
        }
    }
}

long rhu_ttydev_read(int fd, void *buf, size_t n)
{
    struct tty_node c;
    struct tty_node *node = node_enter(fd, &c);
    if (node == NULL)
        return -1;
    long result;
    if (c.accmode == O_WRONLY) {
        result = refuse(RHU_GERR_EBADF);
    } else if (n == 0) {
        result = 0;
    } else if (buf == NULL) {
        result = refuse(RHU_GERR_EFAULT);
    } else {
        uint32_t want = n > TTY_COUNT_MAX ? TTY_COUNT_MAX : (uint32_t)n;
        int32_t got = rhu_uart_read(c.h, buf, want);
        if (got > 0) {
            result = got;
        } else if (got < 0) {
            result = refuse(RHU_GERR_EBADF);
        } else if (c.nonblock) {
#ifdef RHU_SHIM_BREAK_TTY_EOF
            result = 0;
#else
            result = refuse(RHU_GERR_EAGAIN);
#endif
        } else {
            result = read_blocking(&c, buf, want);
        }
    }
    node_leave(node);
    return result;
}

long rhu_ttydev_write(int fd, const void *buf, size_t n)
{
    struct tty_node c;
    struct tty_node *node = node_enter(fd, &c);
    if (node == NULL)
        return -1;
    long result;
    if (c.accmode == O_RDONLY) {
        result = refuse(RHU_GERR_EBADF);
    } else if (n == 0) {
        result = 0;
    } else if (buf == NULL) {
        result = refuse(RHU_GERR_EFAULT);
    } else {
        uint32_t want = n > TTY_COUNT_MAX ? TTY_COUNT_MAX : (uint32_t)n;
        int32_t taken = rhu_uart_write(c.h, buf, want);
        if (taken < 0) {
            result = refuse(RHU_GERR_EBADF);
        } else if (c.nonblock) {
            result = taken > 0 ? taken : refuse(RHU_GERR_EAGAIN);
        } else {
            /* Blocking: wait for room on the event word and loop until
             * every byte is taken; an error after a take answers the count
             * taken, so a caller continuing from the count sends no byte
             * twice. */
            const uint32_t *word = rhu_uart_event_word(c.h);
            const uint8_t *bytes = (const uint8_t *)buf;
            uint32_t done = (uint32_t)taken;
            while (done < want) {
                uint32_t v = word != NULL ? *word : 0u;
                int32_t t = rhu_uart_write(c.h, bytes + done, want - done);
                if (t < 0) {
                    ERRNO = RHU_GERR_EBADF;
                    break;
                }
                if (t > 0) {
                    done += (uint32_t)t;
                    continue;
                }
                if (word != NULL)
                    rhu_futex_wait(word, v, RHU_DEADLINE_NONE);
                else
                    rhu_sleep_ns(TTY_POLL_SLICE_NS);
            }
            result = done > 0 ? (long)done : -1;
        }
    }
    node_leave(node);
    return result;
}

/* ------------------------------------------------------------------ */
/* the termios verbs: the image, the validation, the one configure door */
/* ------------------------------------------------------------------ */

/* The struct applied to the line (tcsetattr and the TCSETS* requests) on
 * a request's entered entry: the whole struct validated, the five values
 * derived, the when discipline, one configure call when any value
 * differs, the image taking the struct whole. 0 on success, -1 with
 * errno. */
static int apply_image_on(struct tty_node *node, const struct tty_node *c, int when, const struct termios *t)
{
    char name[24];
    port_name(c->block, name);
    uint32_t rate, bits, parity, stops;
    if (when != TCSANOW && when != TCSADRAIN && when != TCSAFLUSH)
        return refuse(RHU_GERR_EINVAL);
    if (!derive_line(t, name, &rate, &bits, &parity, &stops))
        return refuse(RHU_GERR_EINVAL);
    /* The when discipline, whether or not the line values move: the drain
     * form waits for the pending transmit within its bound; the flush form
     * then discards the receive side. The now form applies at once -- the
     * configure door itself lets the pending transmit leave at the old
     * rate before the block is reprogrammed, so nothing is garbled and a
     * stalled line fails nothing here. */
    if (when != TCSANOW) {
        if (drain(c, name) != 0)
            return -1;
        if (when == TCSAFLUSH && rhu_uart_flush(c->h, RHU_UART_FLUSH_RX) != 0)
            return refuse(RHU_GERR_EBADF);
    }
    uint32_t achieved = c->achieved;
    if (rate != c->rate || bits != c->bits || parity != c->parity || stops != c->stops) {
        int32_t rc = rhu_uart_configure(c->h, rate, bits, parity, stops, &achieved);
        if (rc != 0) {
            rhu_shim_log("%s: %u %u%c%u refused by the port (EINVAL: a rate the part misses or a frame it lacks; the door answered %d)", name, rate, bits, parity_letter(parity), stops, rc);
            return refuse(RHU_GERR_EINVAL);
        }
        rhu_shim_log("%s: %u %u%c%u (%u Bd achieved)", name, rate, bits, parity_letter(parity), stops, achieved);
    }
    if (!table_lock())
        return -1;
    if (node->state == TTY_LIVE || node->state == TTY_CLOSING) {
        node->image = *t;
        node->rate = rate;
        node->bits = bits;
        node->parity = parity;
        node->stops = stops;
        node->achieved = achieved;
    }
    table_unlock();
    return 0;
}

static int apply_image(int fd, int when, const struct termios *t)
{
    struct tty_node c;
    struct tty_node *node = verb_enter(fd, &c);
    if (node == NULL)
        return -1;
    int result = apply_image_on(node, &c, when, t);
    node_leave(node);
    return result;
}

int tcgetattr(int fd, struct termios *t)
{
    struct tty_node c;
    struct tty_node *node = verb_enter(fd, &c);
    if (node == NULL)
        return -1;
    if (t == NULL) {
        node_leave(node);
        return refuse(RHU_GERR_EFAULT);
    }
#ifdef RHU_SHIM_BREAK_TTY_IMAGE
    claim_image(t);
#else
    *t = c.image;
#endif
    node_leave(node);
    return 0;
}

int tcsetattr(int fd, int when, const struct termios *t)
{
    if (t == NULL)
        return refuse(RHU_GERR_EFAULT);
    return apply_image(fd, when, t);
}

void cfmakeraw(struct termios *t)
{
    t->c_iflag &= ~(tcflag_t)(IGNBRK | BRKINT | PARMRK | ISTRIP | INLCR | IGNCR | ICRNL | IXON);
    t->c_oflag &= ~(tcflag_t)OPOST;
    t->c_lflag &= ~(tcflag_t)(ECHO | ECHONL | ICANON | ISIG | IEXTEN);
    t->c_cflag &= ~(tcflag_t)(CSIZE | PARENB);
    t->c_cflag |= CS8;
    t->c_cc[VMIN] = 1;
    t->c_cc[VTIME] = 0;
}

speed_t cfgetispeed(const struct termios *t)
{
    return t->c_ispeed;
}

speed_t cfgetospeed(const struct termios *t)
{
    return t->c_ospeed;
}

/* The speed set the C library's way: the constant in the speed word and in
 * the CBAUD bits of the control flags (glibc keeps both), BOTHER kept as
 * the termios2 form's marker. */
static int set_speed(struct termios *t, speed_t speed, int input, int output)
{
    if (speed != BOTHER && speed != B0 && rate_of_code(speed) == 0)
        return refuse(RHU_GERR_EINVAL);
    if (output) {
        t->c_ospeed = speed;
        t->c_cflag = (t->c_cflag & ~(tcflag_t)CBAUD) | (speed & CBAUD);
    }
    if (input)
        t->c_ispeed = speed;
    return 0;
}

int cfsetispeed(struct termios *t, speed_t speed)
{
    return set_speed(t, speed, 1, 0);
}

int cfsetospeed(struct termios *t, speed_t speed)
{
    return set_speed(t, speed, 0, 1);
}

int cfsetspeed(struct termios *t, speed_t speed)
{
    return set_speed(t, speed, 1, 1);
}

static int flush_queue(const struct tty_node *c, int queue)
{
    switch (queue) {
    case TCIFLUSH:
        return rhu_uart_flush(c->h, RHU_UART_FLUSH_RX) == 0 ? 0 : refuse(RHU_GERR_EBADF);
    case TCOFLUSH:
        return rhu_uart_flush(c->h, RHU_UART_FLUSH_TX) == 0 ? 0 : refuse(RHU_GERR_EBADF);
    case TCIOFLUSH:
        return rhu_uart_flush(c->h, RHU_UART_FLUSH_BOTH) == 0 ? 0 : refuse(RHU_GERR_EBADF);
    default:
        return refuse(RHU_GERR_EINVAL);
    }
}

int tcflush(int fd, int queue)
{
    struct tty_node c;
    struct tty_node *node = verb_enter(fd, &c);
    if (node == NULL)
        return -1;
    int r = flush_queue(&c, queue);
    node_leave(node);
    return r;
}

int tcdrain(int fd)
{
    char name[24];
    struct tty_node c;
    struct tty_node *node = verb_enter(fd, &c);
    if (node == NULL)
        return -1;
    int r = drain(&c, port_name(c.block, name));
    node_leave(node);
    return r;
}

/* Whether the descriptor's handle is still held, by the status door,
 * which refuses the released handle alone: 0 live, -1 with EBADF when
 * the port went away under the request. */
static int still_held(const struct tty_node *c)
{
    uint32_t rx, pending, room;
    return rhu_uart_status(c->h, &rx, &pending, &room) != 0 ? refuse(RHU_GERR_EBADF) : 0;
}

/* The one line on a part without break control. */
static void say_no_break(struct tty_node *node, const struct tty_node *c, const char *name)
{
    if (c->break_said)
        return;
    rhu_shim_log("%s: this port has no break control; the break is a no-op", name);
    if (table_lock()) {
        node->break_said = 1;
        table_unlock();
    }
}

/* A break after the drain: held for the asked tenths of a second, 250 ms
 * for 0, in slices with the descriptor re-tested at each (a close under
 * the break ends it: the release drops the line, and the request answers
 * EBADF); a part without break control answers 0 with one line, as Linux
 * answers a driver without it. The break door answers the one code for
 * a part without break control and for a handle no longer held, so that
 * answer is re-tested at the status door before the no-op mapping. */
static int send_break(struct tty_node *node, const struct tty_node *c, const char *name, int tenths)
{
    if (drain(c, name) != 0)
        return -1;
    int32_t rc = rhu_uart_break(c->h, 1);
    if (rc == RHU_EINVAL) {
        if (still_held(c) != 0)
            return -1;
        say_no_break(node, c, name);
        return 0;
    }
    if (rc != 0)
        return refuse(RHU_GERR_EBADF);
    uint64_t hold = tenths > 0 ? (uint64_t)tenths * 100ull * NS_PER_MS : TTY_BREAK_MS * NS_PER_MS;
    uint64_t end = rhu_now_ns() + hold;
    for (;;) {
        uint64_t now = rhu_now_ns();
        if (now >= end)
            break;
        uint64_t left = end - now;
        rhu_sleep_ns(left > TTY_POLL_SLICE_NS ? TTY_POLL_SLICE_NS : left);
        if (still_held(c) != 0)
            return -1;
    }
    rhu_uart_break(c->h, 0);
    return 0;
}

/* tcsendbreak: the C library's duration is in milliseconds -- zero or less
 * the default break, else the tenths of a second rounded up, as the C
 * library forms the TCSBRKP request from it; no bound on the duration (a
 * close under the break ends it). */
int tcsendbreak(int fd, int duration)
{
    char name[24];
    struct tty_node c;
    struct tty_node *node = verb_enter(fd, &c);
    if (node == NULL)
        return -1;
    int tenths = duration <= 0 ? 0 : duration / 100 + (duration % 100 != 0);
    int r = send_break(node, &c, port_name(c.block, name), tenths);
    node_leave(node);
    return r;
}

/* tcflow: no port here suspends its transmitter or receiver on request,
 * so a live descriptor of the band answers ENOTSUP; any other number as
 * the family's other verbs answer it. */
int tcflow(int fd, int action)
{
    (void)action;
    struct tty_node c;
    struct tty_node *node = verb_enter(fd, &c);
    if (node == NULL)
        return -1;
    node_leave(node);
    return refuse(RHU_GERR_ENOTSUP);
}

/* ------------------------------------------------------------------ */
/* the requests                                                        */
/* ------------------------------------------------------------------ */

/* The kernel's 36-byte termios (the TCGETS family's record): the glibc
 * image without its speed words, the speed in the CBAUD bits. */
struct tty_kernel_termios {
    tcflag_t c_iflag;
    tcflag_t c_oflag;
    tcflag_t c_cflag;
    tcflag_t c_lflag;
    cc_t c_line;
    cc_t c_cc[RHU_KERNEL_NCCS];
};

static void image_to_kernel(const struct termios *img, struct tty_kernel_termios *k)
{
    memset(k, 0, sizeof *k);
    k->c_iflag = img->c_iflag;
    k->c_oflag = img->c_oflag;
    k->c_cflag = img->c_cflag;
    k->c_lflag = img->c_lflag;
    k->c_line = img->c_line;
    memcpy(k->c_cc, img->c_cc, RHU_KERNEL_NCCS);
}

static void image_to_termios2(const struct termios *img, uint32_t rate, struct termios2 *k)
{
    memset(k, 0, sizeof *k);
    k->c_iflag = img->c_iflag;
    k->c_oflag = img->c_oflag;
    k->c_cflag = img->c_cflag;
    k->c_lflag = img->c_lflag;
    k->c_line = img->c_line;
    memcpy(k->c_cc, img->c_cc, RHU_KERNEL_NCCS);
    /* The termios2 form carries the numbers: Linux answers the rate in
     * both speed words whatever the CBAUD bits say. */
    k->c_ispeed = rate;
    k->c_ospeed = rate;
}

/* A kernel-form record into the C library's form over the current image
 * (the control characters past the kernel's nineteen kept). The output
 * speed is the CBAUD code; the input speed the code in the input bits
 * above IBSHIFT, or the output's when they are clear -- a code of its own
 * is a split the validation refuses. */
static void kernel_to_image(const struct tty_kernel_termios *k, const struct termios *cur, struct termios *out)
{
    *out = *cur;
    out->c_iflag = k->c_iflag;
    out->c_oflag = k->c_oflag;
    out->c_cflag = k->c_cflag;
    out->c_lflag = k->c_lflag;
    out->c_line = k->c_line;
    memcpy(out->c_cc, k->c_cc, RHU_KERNEL_NCCS);
    speed_t code = k->c_cflag & CBAUD;
    speed_t in_code = (k->c_cflag >> IBSHIFT) & CBAUD;
    out->c_ospeed = code;
    out->c_ispeed = in_code == 0 ? code : in_code;
}

static void termios2_to_image(const struct termios2 *k, const struct termios *cur, struct termios *out)
{
    *out = *cur;
    out->c_iflag = k->c_iflag;
    out->c_oflag = k->c_oflag;
    out->c_cflag = k->c_cflag;
    out->c_lflag = k->c_lflag;
    out->c_line = k->c_line;
    memcpy(out->c_cc, k->c_cc, RHU_KERNEL_NCCS);
    tcflag_t in_bits = (k->c_cflag >> IBSHIFT) & CBAUD;
    if ((k->c_cflag & CBAUD) == BOTHER) {
        /* The number travels in the speed words; the input word's own
         * CBAUD bits (shifted by IBSHIFT) are the output's unless set. */
        out->c_ospeed = k->c_ospeed;
        out->c_ispeed = (in_bits == 0 || in_bits == BOTHER) ? k->c_ispeed : rate_of_code(in_bits);
        if (in_bits == 0)
            out->c_ispeed = k->c_ospeed;
        /* The validation reads BOTHER in the flags and the rate in the
         * output word; the input word must carry the same number. */
        out->c_cflag &= ~(tcflag_t)(CBAUD << IBSHIFT);
    } else {
        /* The output speed by its code; the input speed the output's when
         * its bits are clear, the number in the input word when they say
         * BOTHER, else the code of its own (a split the validation
         * refuses). */
        speed_t code = k->c_cflag & CBAUD;
        out->c_ospeed = code;
        out->c_ispeed = in_bits == 0 ? code : (in_bits == BOTHER ? k->c_ispeed : in_bits);
    }
}

/* The latch moved by TIOCMSET (how 0), TIOCMBIS (1) or TIOCMBIC (2) on
 * the entry's live value under the lock, so two threads' updates compose;
 * the one line on the first change is written after the lock is given
 * back. */
static int modem_set(struct tty_node *node, const char *name, int bits, int how)
{
    uint8_t asked = (uint8_t)(bits & TTY_LATCH_BITS);
    int say = 0;
    if (!table_lock())
        return -1;
    uint8_t latch = node->modem;
    switch (how) {
    case 0:
        latch = asked;
        break;
    case 1:
        latch |= asked;
        break;
    default:
        latch &= (uint8_t)~asked;
        break;
    }
    if (latch != node->modem && !node->modem_said) {
        node->modem_said = 1;
        say = 1;
    }
    node->modem = latch;
    table_unlock();
    if (say)
        rhu_shim_log("%s: DTR and RTS are not wired on this port; the state is kept", name);
    return 0;
}

int rhu_ttydev_ioctl(int fd, unsigned long request, void *arg)
{
    char name[24];
    struct tty_node c;
    struct tty_node *node = node_enter(fd, &c);
    if (node == NULL)
        return -1;
    port_name(c.block, name);
    int result = 0;
    uint32_t rx, pending, room;
    switch (request) {
    case FIONREAD:
        if (arg == NULL)
            result = refuse(RHU_GERR_EFAULT);
        else if (rhu_uart_status(c.h, &rx, &pending, &room) != 0)
            result = refuse(RHU_GERR_EBADF);
        else
            *(int *)arg = (int)rx;
        break;
    case TIOCOUTQ:
        if (arg == NULL)
            result = refuse(RHU_GERR_EFAULT);
        else if (rhu_uart_status(c.h, &rx, &pending, &room) != 0)
            result = refuse(RHU_GERR_EBADF);
        else
            *(int *)arg = (int)pending;
        break;
    case TIOCMGET:
        if (arg == NULL)
            result = refuse(RHU_GERR_EFAULT);
        else
            *(int *)arg = (int)(TTY_ALWAYS_BITS | c.modem);
        break;
    case TIOCMSET:
    case TIOCMBIS:
    case TIOCMBIC:
        if (arg == NULL)
            result = refuse(RHU_GERR_EFAULT);
        else
            result = modem_set(node, name, *(int *)arg, request == TIOCMSET ? 0 : (request == TIOCMBIS ? 1 : 2));
        break;
    case TIOCEXCL:
    case TIOCNXCL:
        if (table_lock()) {
            node->exclusive = request == TIOCEXCL ? 1u : 0u;
            table_unlock();
        } else {
            result = -1;
        }
        break;
    case TCGETS:
        if (arg == NULL)
            result = refuse(RHU_GERR_EFAULT);
        else
            image_to_kernel(&c.image, (struct tty_kernel_termios *)arg);
        break;
    case TCSETS:
    case TCSETSW:
    case TCSETSF: {
        if (arg == NULL) {
            result = refuse(RHU_GERR_EFAULT);
            break;
        }
        struct tty_kernel_termios k;
        struct termios want;
        memcpy(&k, arg, sizeof k);
        kernel_to_image(&k, &c.image, &want);
        int when = request == TCSETS ? TCSANOW : (request == TCSETSW ? TCSADRAIN : TCSAFLUSH);
        result = apply_image_on(node, &c, when, &want);
        break;
    }
    case TCGETS2:
        if (arg == NULL)
            result = refuse(RHU_GERR_EFAULT);
        else
            image_to_termios2(&c.image, c.rate, (struct termios2 *)arg);
        break;
    case TCSETS2:
    case TCSETSW2:
    case TCSETSF2: {
        if (arg == NULL) {
            result = refuse(RHU_GERR_EFAULT);
            break;
        }
        struct termios2 k;
        struct termios want;
        memcpy(&k, arg, sizeof k);
        termios2_to_image(&k, &c.image, &want);
        int when = request == TCSETS2 ? TCSANOW : (request == TCSETSW2 ? TCSADRAIN : TCSAFLUSH);
        result = apply_image_on(node, &c, when, &want);
        break;
    }
    case TCFLSH:
        /* The queue travels by value in the argument word. */
        result = flush_queue(&c, (int)(intptr_t)arg);
        break;
    case TCSBRK:
        /* Linux: a zero argument sends a break of the default duration
         * after the drain; a nonzero one drains alone. */
        result = (intptr_t)arg == 0 ? send_break(node, &c, name, 0) : drain(&c, name);
        break;
    case TCSBRKP:
        /* The argument in tenths of a second, as Linux reads it. */
        result = send_break(node, &c, name, (int)(intptr_t)arg);
        break;
    case TIOCSBRK:
    case TIOCCBRK: {
        int32_t rc = rhu_uart_break(c.h, request == TIOCSBRK ? 1u : 0u);
        if (rc == RHU_EINVAL) {
            /* The door's one code for a part without break control and
             * for a handle no longer held: the status door tells them
             * apart. */
            if (still_held(&c) != 0)
                result = -1;
            else
                say_no_break(node, &c, name);
        } else if (rc != 0) {
            result = refuse(RHU_GERR_EBADF);
        }
        break;
    }
    case TIOCMIWAIT:
        rhu_shim_log("%s: no modem line can change on this port (ENOTTY: request 0x%x)", name, (unsigned)request);
        result = refuse(RHU_GERR_ENOTTY);
        break;
    default:
        rhu_shim_log("%s: the request 0x%x is not served on a serial port (ENOTTY)", name, (unsigned)request);
        result = refuse(RHU_GERR_ENOTTY);
        break;
    }
    node_leave(node);
    return result;
}

/* ------------------------------------------------------------------ */
/* the C-library names the stock layer reaches for beyond the verbs    */
/* ------------------------------------------------------------------ */

/* poll: the console output descriptors always writable, the console input
 * never ready, a file descriptor readable and writable, a tty descriptor
 * by its rings (the readable request POLLIN or POLLRDNORM, the writable
 * POLLOUT or POLLWRNORM, the bits asked echoed), every other band
 * invalid. With one tty descriptor the thread sleeps on that port's event
 * word with the deadline; otherwise the sleep is sliced at one millisecond
 * with every descriptor re-tested. A live port never reports an error or
 * a hang-up. */
int poll(struct pollfd *fds, nfds_t nfds, int timeout)
{
    if (fds == NULL && nfds != 0)
        return refuse(RHU_GERR_EFAULT);
    uint64_t deadline = timeout < 0 ? RHU_DEADLINE_NONE : rhu_now_ns() + (uint64_t)timeout * NS_PER_MS;
    for (;;) {
        int ready = 0;
        const uint32_t *word = NULL;
        uint32_t word_value = 0;
        int tty_count = 0;
        for (nfds_t i = 0; i < nfds; i++) {
            struct pollfd *p = &fds[i];
            short want = p->events;
            short got = 0;
            int fd = p->fd;
            if (fd < 0) {
                got = 0;
            } else if (fd_is_console(fd)) {
                if (fd != 0 && (want & (POLLOUT | POLLWRNORM)) != 0)
                    got = (short)(want & (POLLOUT | POLLWRNORM));
            } else if (fd >= RHU_SHIM_FD_FILE_LO && fd < RHU_SHIM_FD_FILE_LO + RHU_SHIM_FD_FILE_COUNT) {
                got = (short)(want & (POLLIN | POLLRDNORM | POLLOUT | POLLWRNORM));
            } else if (fd_in_band(fd)) {
                struct tty_node c;
                struct tty_node *node = node_enter(fd, &c);
                if (node == NULL) {
                    got = POLLNVAL;
                } else {
                    const uint32_t *w = rhu_uart_event_word(c.h);
                    uint32_t v = w != NULL ? *w : 0u;
                    uint32_t rx, pending, room;
                    if (rhu_uart_status(c.h, &rx, &pending, &room) != 0) {
                        got = POLLNVAL;
                    } else {
                        if ((want & (POLLIN | POLLRDNORM)) != 0 && rx > 0)
                            got |= (short)(want & (POLLIN | POLLRDNORM));
                        if ((want & (POLLOUT | POLLWRNORM)) != 0 && room > 0)
                            got |= (short)(want & (POLLOUT | POLLWRNORM));
#ifdef RHU_SHIM_BREAK_TTY_POLLERR
                        got |= POLLERR;
#endif
                    }
                    if (tty_count == 0) {
                        word = w;
                        word_value = v;
                    }
                    tty_count++;
                    node_leave(node);
                }
            } else {
                got = POLLNVAL;
            }
            p->revents = got;
            if (got != 0)
                ready++;
        }
        uint64_t now = rhu_now_ns();
        if (ready != 0 || timeout == 0 || (deadline != RHU_DEADLINE_NONE && now >= deadline))
            return ready;
        if (tty_count == 1 && nfds == 1 && word != NULL) {
            rhu_futex_wait(word, word_value, deadline);
        } else {
            uint64_t left = deadline == RHU_DEADLINE_NONE ? TTY_POLL_SLICE_NS : deadline - now;
            rhu_sleep_ns(left > TTY_POLL_SLICE_NS ? TTY_POLL_SLICE_NS : left);
        }
    }
}

/* fcntl: the descriptor flags (nothing to keep: 0) and the status flags
 * (the access mode with the non-blocking bit) on the tty band; the console
 * descriptors answer their status as the C library's fcntl would; a
 * descriptor of another band the shim serves answers the minimum (no
 * descriptor flags, the status read-write, a set accepted), since nothing
 * of its own is kept here; every other command EINVAL, every number
 * outside the bands EBADF. */
int fcntl(int fd, int cmd, ...)
{
    __builtin_va_list ap;
    __builtin_va_start(ap, cmd);
    int argument = __builtin_va_arg(ap, int);
    __builtin_va_end(ap);
    if (fd_is_console(fd)) {
        switch (cmd) {
        case F_GETFD:
        case F_SETFD:
            return 0;
        case F_GETFL:
            return fd == 0 ? O_RDONLY : O_WRONLY;
        case F_SETFL:
            return 0;
        default:
            return refuse(RHU_GERR_EINVAL);
        }
    }
    if (!fd_in_band(fd)) {
        if (!fd_of_other_band(fd))
            return refuse(RHU_GERR_EBADF);
        switch (cmd) {
        case F_GETFD:
        case F_SETFD:
        case F_SETFL:
            return 0;
        case F_GETFL:
            return O_RDWR;
        default:
            return refuse(RHU_GERR_EINVAL);
        }
    }
    struct tty_node c;
    struct tty_node *node = node_enter(fd, &c);
    if (node == NULL)
        return -1;
    int result = 0;
    switch (cmd) {
    case F_GETFD:
    case F_SETFD:
        break;
    case F_GETFL:
        result = (int)c.accmode | (c.nonblock ? O_NONBLOCK : 0);
        break;
    case F_SETFL:
        if (table_lock()) {
            node->nonblock = (argument & O_NONBLOCK) != 0 ? 1u : 0u;
            table_unlock();
        } else {
            result = -1;
        }
        break;
    default:
        result = refuse(RHU_GERR_EINVAL);
        break;
    }
    node_leave(node);
    return result;
}

/* shutdown: a serial port is not a socket (the stock layer's close path
 * calls it and ignores the answer); a closed number of the band is
 * EBADF, as any number the shim does not serve. */
int shutdown(int fd, int how)
{
    (void)how;
    if (fd_in_band(fd)) {
        struct tty_node c;
        struct tty_node *node = node_enter(fd, &c);
        if (node == NULL)
            return -1;
        node_leave(node);
        return refuse(RHU_GERR_ENOTSOCK);
    }
    if (fd_is_console(fd))
        return refuse(RHU_GERR_ENOTSOCK);
    return refuse(RHU_GERR_EBADF);
}

/* The error string's POSIX form (the kit's C library declares it under
 * the name below; the plain name is kept for a caller that declares its
 * own): the number spelled into the caller's buffer, since this image
 * carries no message table. ERANGE when the buffer is too small, with as
 * much of the message as fits, terminated. */
static int spell_error(int err, char *buf, size_t n)
{
    static const char prefix[] = "error ";
    char text[24];
    char digits[12];
    size_t d = 0;
    unsigned v = err < 0 ? 0u - (unsigned)err : (unsigned)err;
    do {
        digits[d++] = (char)('0' + v % 10u);
        v /= 10u;
    } while (v != 0);
    if (buf == NULL || n == 0)
        return RHU_GERR_EINVAL;
    size_t i = 0;
    while (prefix[i] != 0) {
        text[i] = prefix[i];
        i++;
    }
    if (err < 0)
        text[i++] = '-';
    while (d > 0)
        text[i++] = digits[--d];
    text[i] = 0;
    size_t len = i;
    size_t room = n - 1 < len ? n - 1 : len;
    memcpy(buf, text, room);
    buf[room] = 0;
    return room < len ? RHU_GERR_ERANGE : 0;
}

int __xpg_strerror_r(int err, char *buf, size_t n)
{
    return spell_error(err, buf, n);
}

int strerror_r(int err, char *buf, size_t n)
{
    return spell_error(err, buf, n);
}

/* The resolver's error string: no resolver runs in this image. */
const char *gai_strerror(int code)
{
    (void)code;
    return "no resolver in this image";
}

/* strlcpy: the copy the stock layer's string helper uses on this system,
 * the whole source's length answered, the destination always terminated
 * when it has room for the terminator. */
size_t strlcpy(char *dst, const char *src, size_t size)
{
    size_t len = strlen(src);
    if (size != 0) {
        size_t n = len < size - 1 ? len : size - 1;
        memcpy(dst, src, n);
        dst[n] = 0;
    }
    return len;
}
