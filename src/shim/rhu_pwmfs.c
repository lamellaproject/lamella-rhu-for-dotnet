/* The PWM device files over the kernel's PWM API: the small tree of text
 * files a program's PWM driver reads and writes on Linux, presented here so
 * the standard package's PWM class (System.Device.Pwm at the pin of record)
 * runs unchanged on this system. The runtime's portability layer hands a
 * path under /sys to this unit and every descriptor of the sysfs band to
 * its verbs; the C library's access does the same for the package's own
 * check of the channel directory.
 *
 * This file is an independent implementation that presents the
 * established device-file surface. The file names, the texts and the two
 * polarity words are the Linux PWM class's as the package's source reads
 * them at the pinned version; the code here maps them onto the kernel's
 * eight PWM calls.
 *
 * THE TREE. /sys, /sys/class and /sys/class/pwm exist on every row. The one
 * chip of a row, /sys/class/pwm/pwmchip0, is the chip's PWM block and exists
 * when the kernel states at least one PWM-capable line; its npwm file reads
 * the number of channel names, its export and unexport files take a channel
 * number. THE CHANNEL NUMBER IS THE LINE NUMBER: PwmChannel.Create(0, 15, ...)
 * drives GPIO 15, and npwm is the highest PWM-capable line plus one, so a
 * line no timer drives is a hole whose export is refused. A Linux controller
 * numbers its own outputs instead and binds each to a pin by configuration
 * outside the program, which this system has no place for; here the pin a
 * program names is the pin it gets, and the kernel's claims keep a timer
 * channel exclusive (a line that reaches the same timer output as a held
 * line is refused). An exported channel's directory pwm<N> carries period
 * and duty_cycle in nanoseconds, polarity ("normal" or "inversed") and
 * enable ("0" or "1"), each one text with a trailing newline, as Linux
 * prints them.
 *
 * THE ONE DEPARTURE IN A READ: the period file reads back the period the
 * timer RUNS, from the kernel's own reading of its clock and prescaler, not
 * the value asked; Linux reads back the value asked. The standard class never
 * reads it; a program that does learns the quantized truth. The duty_cycle
 * file reads the value asked, as Linux does (the kernel states no duty
 * readback), and a polarity change while the output runs is applied live
 * (some Linux drivers refuse it).
 *
 * THE WRITES, as Linux takes them: each write call is one value (a decimal
 * number, or one of the two polarity words, a trailing newline or spaces
 * tolerated; anything else EINVAL). export: the kernel's claim of the line;
 * a line past npwm or one no timer drives ENODEV; a held line, or a channel
 * already exported, EBUSY. unexport: the kernel's release; a channel not
 * exported ENODEV. period: 0 EINVAL; otherwise the kernel's set with the
 * channel's duty and polarity, its refusals passed on (EBUSY when another
 * held channel of the same timer runs another period; EINVAL for a period
 * the timer cannot reach or below the duty already set). duty_cycle before
 * any period EINVAL (as the Linux core refuses a state with no period);
 * otherwise the set, a duty past the period EINVAL. enable before any
 * period EINVAL; otherwise the output driven or parked. Every refusal is one
 * line on the console; a successful export is one line naming the timer,
 * the channel and the clock, so a program's author reads what the pin
 * reached. Each file stats as a regular file of 4096 bytes (the size Linux
 * reports for an attribute), truncates to any length and locks without
 * effect; the directories stat as directories.
 *
 * A caller's memory is read ONCE: a write's bytes are copied before they
 * are parsed. A NULL buffer is refused EFAULT. The unit's table and its
 * descriptors are kept under one mutex of the shim's family (a zero-filled
 * slot the core adopts at its first use); the kernel's calls run under it,
 * none of them waits, and nothing here masks an interrupt. */

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
#define PWMFS_SYS "/sys"
#define PWMFS_CLASS "/sys/class"
#define PWMFS_ROOT "/sys/class/pwm"
#define PWMFS_CHIP "/sys/class/pwm/pwmchip0"
#define PWMFS_PATH_MAX 64
/* The table's reach over lines: the largest package in hand has 48. */
#define PWMFS_LINES_MAX 64
/* The size Linux reports for a sysfs attribute file. */
#define PWMFS_ATTR_SIZE 4096
/* A text of a node, with its newline and terminator. */
#define PWMFS_TEXT_MAX 32

/* The nodes a path can name. */
enum {
    NODE_NONE = 0,
    NODE_DIR_SYS,
    NODE_DIR_CLASS,
    NODE_DIR_ROOT,
    NODE_DIR_CHIP,
    NODE_NPWM,
    NODE_EXPORT,
    NODE_UNEXPORT,
    NODE_DIR_CHANNEL,
    NODE_PERIOD,
    NODE_DUTY,
    NODE_POLARITY,
    NODE_ENABLE
};

typedef struct pwmfs_node {
    uint8_t kind;
    uint8_t line;
} pwmfs_node;

/* An exported channel: the kernel's handle, the period and duty asked, the
 * polarity and the enable as last written. */
typedef struct pwmfs_channel {
    uint8_t exported;
    uint8_t period_set;
    uint8_t polarity;
    uint8_t enabled;
    int32_t handle;
    uint64_t period_ns;
    uint64_t duty_ns;
} pwmfs_channel;

/* A descriptor of the sysfs band. */
typedef struct pwmfs_fd {
    uint8_t used;
    uint8_t kind;
    uint8_t line;
    uint8_t access;
    uint32_t pos;
} pwmfs_fd;

static pwmfs_channel g_ch[PWMFS_LINES_MAX];
static pwmfs_fd g_fd[RHU_SHIM_FD_SYSFS_COUNT];
static uint8_t g_mutex[24];
/* The number of channel names: computed once from the kernel's enumeration
 * (-1 before the first use). */
static int32_t g_npwm = -1;

static int refuse(int32_t err)
{
    ERRNO = err;
    return -1;
}

static int table_lock(void)
{
    if (pthread_mutex_lock(g_mutex) == 0)
        return 0;
    rhu_shim_log("pwm files: the table's lock refused (ENOLCK)");
    ERRNO = RHU_GERR_ENOLCK;
    return -1;
}

static void table_unlock(void)
{
    pthread_mutex_unlock(g_mutex);
}

/* The kernel's refusal codes as the C library states them. */
static int32_t kerr(int32_t rc)
{
    switch (rc) {
    case RHU_EBUSY:
        return RHU_GERR_EBUSY;
    case RHU_EINVAL:
        return RHU_GERR_EINVAL;
    case RHU_ENODEV:
        return RHU_GERR_ENODEV;
    default:
        return RHU_GERR_EIO;
    }
}

/* The highest PWM-capable line plus one, from the kernel's enumeration; 0 on
 * a row with no such line. */
static int32_t npwm_locked(void)
{
    if (g_npwm >= 0)
        return g_npwm;
    int32_t count = rhu_pwm_count();
    int32_t top = -1;
    for (int32_t i = 0; i < count; i++) {
        int32_t line = rhu_pwm_line((uint32_t)i);
        if (line > top)
            top = line;
    }
#ifdef RHU_SHIM_BREAK_PWMFS_NPWM_COUNT
    /* Built with this define, npwm reads the count of lines in place of the
     * highest line plus one, so a channel name past the count is refused by
     * the class before the kernel is asked. Never set in a hosted image. */
    g_npwm = count > 0 ? count : 0;
#else
    g_npwm = count > 0 ? top + 1 : 0;
#endif
    return g_npwm;
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

/* A path to its node, with one trailing slash dropped; the chip and a
 * channel directory exist only as the table says, which the caller judges. */
static int parse_path(const char *path, pwmfs_node *n)
{
    char buf[PWMFS_PATH_MAX];
    n->kind = NODE_NONE;
    n->line = 0;
    if (path == NULL)
        return 0;
    size_t len = strlen(path);
    if (len == 0 || len >= PWMFS_PATH_MAX)
        return 0;
    memcpy(buf, path, len + 1);
    if (len > 1 && buf[len - 1] == '/')
        buf[--len] = 0;
    if (strcmp(buf, PWMFS_SYS) == 0) {
        n->kind = NODE_DIR_SYS;
        return 1;
    }
    if (strcmp(buf, PWMFS_CLASS) == 0) {
        n->kind = NODE_DIR_CLASS;
        return 1;
    }
    if (strcmp(buf, PWMFS_ROOT) == 0) {
        n->kind = NODE_DIR_ROOT;
        return 1;
    }
    size_t cl = strlen(PWMFS_CHIP);
    if (strncmp(buf, PWMFS_CHIP, cl) != 0)
        return 0;
    const char *p = buf + cl;
    if (*p == 0) {
        n->kind = NODE_DIR_CHIP;
        return 1;
    }
    if (*p != '/')
        return 0;
    p++;
    if (strcmp(p, "npwm") == 0) {
        n->kind = NODE_NPWM;
        return 1;
    }
    if (strcmp(p, "export") == 0) {
        n->kind = NODE_EXPORT;
        return 1;
    }
    if (strcmp(p, "unexport") == 0) {
        n->kind = NODE_UNEXPORT;
        return 1;
    }
    if (strncmp(p, "pwm", 3) != 0)
        return 0;
    p += 3;
    uint32_t line;
    if (!parse_number(&p, &line) || line >= PWMFS_LINES_MAX)
        return 0;
    n->line = (uint8_t)line;
    if (*p == 0) {
        n->kind = NODE_DIR_CHANNEL;
        return 1;
    }
    if (*p != '/')
        return 0;
    p++;
    if (strcmp(p, "period") == 0)
        n->kind = NODE_PERIOD;
    else if (strcmp(p, "duty_cycle") == 0)
        n->kind = NODE_DUTY;
    else if (strcmp(p, "polarity") == 0)
        n->kind = NODE_POLARITY;
    else if (strcmp(p, "enable") == 0)
        n->kind = NODE_ENABLE;
    else
        return 0;
    return 1;
}

static int node_is_dir(uint8_t kind)
{
    return kind == NODE_DIR_SYS || kind == NODE_DIR_CLASS || kind == NODE_DIR_ROOT || kind == NODE_DIR_CHIP || kind == NODE_DIR_CHANNEL;
}

/* Whether the node exists on this row now (under the lock). */
static int node_exists_locked(const pwmfs_node *n)
{
    switch (n->kind) {
    case NODE_DIR_SYS:
    case NODE_DIR_CLASS:
    case NODE_DIR_ROOT:
        return 1;
    case NODE_DIR_CHIP:
    case NODE_NPWM:
    case NODE_EXPORT:
    case NODE_UNEXPORT:
        return npwm_locked() > 0;
    case NODE_DIR_CHANNEL:
    case NODE_PERIOD:
    case NODE_DUTY:
    case NODE_POLARITY:
    case NODE_ENABLE:
        return npwm_locked() > 0 && n->line < npwm_locked() && g_ch[n->line].exported != 0;
    default:
        return 0;
    }
}

/* The permission bits of a node: 0644 for the attributes, 0444 for npwm,
 * 0200 for export and unexport, 0755 for a directory. */
static uint32_t node_mode(uint8_t kind)
{
    if (node_is_dir(kind))
        return 0x1EDu;
    if (kind == NODE_NPWM)
        return 0x124u;
    if (kind == NODE_EXPORT || kind == NODE_UNEXPORT)
        return 0x080u;
    return 0x1A4u;
}

static void status_of(rhu_pal_filestatus *st, const pwmfs_node *n)
{
    memset(st, 0, sizeof *st);
    if (node_is_dir(n->kind)) {
        st->Mode = (int32_t)(RHU_PAL_S_IFDIR | node_mode(n->kind));
    } else {
        st->Mode = (int32_t)(RHU_PAL_S_IFREG | node_mode(n->kind));
        st->Size = PWMFS_ATTR_SIZE;
    }
    st->Ino = 0x50000 + (int64_t)n->kind * PWMFS_LINES_MAX + n->line;
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

static uint32_t put_word(char *buf, const char *w)
{
    uint32_t len = (uint32_t)strlen(w);
    memcpy(buf, w, len);
    buf[len++] = '\n';
    buf[len] = 0;
    return len;
}

/* The text a readable node answers (under the lock). */
static uint32_t text_of_locked(const pwmfs_fd *h, char *buf)
{
    pwmfs_channel *c = &g_ch[h->line];
    switch (h->kind) {
    case NODE_NPWM:
        return put_number(buf, (uint64_t)npwm_locked());
    case NODE_PERIOD: {
        uint64_t ns = 0;
#ifdef RHU_SHIM_BREAK_PWMFS_PERIOD_ASKED
        /* Built with this define, the period asked is read back in place of
         * the period the timer runs. Never set in a hosted image. */
        ns = c->period_set ? c->period_ns : 0;
#else
        if (c->exported && rhu_pwm_period(c->handle, &ns) != 0)
            ns = 0;
#endif
        return put_number(buf, ns);
    }
    case NODE_DUTY:
        return put_number(buf, c->duty_ns);
    case NODE_POLARITY:
        return put_word(buf, c->polarity ? "inversed" : "normal");
    case NODE_ENABLE:
        return put_number(buf, c->enabled);
    default:
        buf[0] = 0;
        return 0;
    }
}

/* A write's bytes as one value: the decimal number, or the word; a trailing
 * newline or spaces dropped. */
static int take_text(const void *src, int32_t n, char *out, uint32_t cap)
{
    if (src == NULL)
        return 0;
    if (n < 0 || (uint32_t)n >= cap)
        return 0;
    memcpy(out, src, (size_t)n);
    out[n] = 0;
    while (n > 0 && (out[n - 1] == '\n' || out[n - 1] == ' ' || out[n - 1] == '\t' || out[n - 1] == '\r'))
        out[--n] = 0;
    return n > 0;
}

static int take_u64(const char *text, uint64_t *out)
{
    const char *s = text;
    uint64_t v = 0;
    if (*s < '0' || *s > '9')
        return 0;
    while (*s >= '0' && *s <= '9') {
        uint64_t d = (uint64_t)(*s - '0');
        if (v > (0xFFFFFFFFFFFFFFFFull - d) / 10ull)
            return 0;
        v = v * 10ull + d;
        s++;
    }
    if (*s != 0)
        return 0;
    *out = v;
    return 1;
}

/* ------------------------------------------------------------------ */
/* the export and unexport writes (under the lock)                     */
/* ------------------------------------------------------------------ */

static int32_t export_locked(uint32_t line)
{
    int32_t npwm = npwm_locked();
    if (line >= (uint32_t)npwm || line >= PWMFS_LINES_MAX) {
        rhu_shim_log("%s/export: line %u is past this chip's %u channel names (ENODEV)", PWMFS_CHIP, line, (uint32_t)npwm);
        return RHU_GERR_ENODEV;
    }
    pwmfs_channel *c = &g_ch[line];
    if (c->exported) {
        rhu_shim_log("%s/export: line %u is exported already (EBUSY)", PWMFS_CHIP, line);
        return RHU_GERR_EBUSY;
    }
    int32_t h = rhu_pwm_claim(line);
#ifdef RHU_SHIM_BREAK_PWMFS_EXPORT_UNJUDGED
    /* Built with this define, a line no timer drives is exported as if the
     * claim had answered a handle. Never set in a hosted image. */
    if (h == RHU_EINVAL)
        h = 0x7FFF;
#endif
    if (h == RHU_EINVAL) {
        rhu_shim_log("%s/export: line %u is not a PWM line of this row (ENODEV)", PWMFS_CHIP, line);
        return RHU_GERR_ENODEV;
    }
    if (h == RHU_EBUSY) {
        rhu_shim_log("%s/export: line %u is held (EBUSY: as a GPIO line, by the board, or its timer output through another line)", PWMFS_CHIP, line);
        return RHU_GERR_EBUSY;
    }
    if (h < 0) {
        rhu_shim_log("%s/export: the claim of line %u answered %d (EIO)", PWMFS_CHIP, line, h);
        return RHU_GERR_EIO;
    }
    memset(c, 0, sizeof *c);
    c->exported = 1;
    c->handle = h;
    uint32_t timer = 0, channel = 0, clock_hz = 0;
    if (rhu_pwm_info(h, &timer, &channel, &clock_hz) == 0)
        rhu_shim_log("%s/export: line %u exported (timer %u channel %u, the timer's clock %u Hz)", PWMFS_CHIP, line, timer, channel, clock_hz);
    else
        rhu_shim_log("%s/export: line %u exported", PWMFS_CHIP, line);
    return 0;
}

static int32_t unexport_locked(uint32_t line)
{
    if (line >= (uint32_t)npwm_locked() || line >= PWMFS_LINES_MAX || !g_ch[line].exported) {
        rhu_shim_log("%s/unexport: line %u is not exported (ENODEV)", PWMFS_CHIP, line);
        return RHU_GERR_ENODEV;
    }
#ifdef RHU_SHIM_BREAK_PWMFS_UNEXPORT_KEEPS
    /* Built with this define, the unexport answers 0 and releases nothing.
     * Never set in a hosted image. */
    return 0;
#else
    pwmfs_channel *c = &g_ch[line];
    int32_t rc = rhu_pwm_release(c->handle);
    if (rc != 0)
        rhu_shim_log("%s/unexport: the release of line %u answered %d; the channel is dropped", PWMFS_CHIP, line, rc);
    memset(c, 0, sizeof *c);
    return 0;
#endif
}

/* The attribute writes of an exported channel (under the lock); 0 or the
 * C library's code. */
static int32_t attribute_write_locked(const pwmfs_fd *h, const char *text)
{
    pwmfs_channel *c = &g_ch[h->line];
    if (!c->exported) {
        rhu_shim_log("%s/pwm%u: written after its unexport (ENODEV)", PWMFS_CHIP, h->line);
        return RHU_GERR_ENODEV;
    }
    uint64_t v = 0;
    int32_t rc;
    switch (h->kind) {
    case NODE_PERIOD:
        if (!take_u64(text, &v) || v == 0) {
            rhu_shim_log("%s/pwm%u/period: not a period in nanoseconds (EINVAL)", PWMFS_CHIP, h->line);
            return RHU_GERR_EINVAL;
        }
        rc = rhu_pwm_set(c->handle, v, c->duty_ns, c->polarity);
        if (rc == RHU_EBUSY) {
            rhu_shim_log("%s/pwm%u/period: %u ns refused (EBUSY: another held channel of the same timer runs another period)", PWMFS_CHIP, h->line, (uint32_t)v);
            return RHU_GERR_EBUSY;
        }
        if (rc == RHU_EINVAL) {
            rhu_shim_log("%s/pwm%u/period: %u ns refused (EINVAL: past the timer's reach, or below the duty already set)", PWMFS_CHIP, h->line, (uint32_t)v);
            return RHU_GERR_EINVAL;
        }
        if (rc != 0) {
            rhu_shim_log("%s/pwm%u/period: the set answered %d (EIO)", PWMFS_CHIP, h->line, rc);
            return RHU_GERR_EIO;
        }
        c->period_ns = v;
        c->period_set = 1;
        return 0;
    case NODE_DUTY:
        if (!take_u64(text, &v)) {
            rhu_shim_log("%s/pwm%u/duty_cycle: not a duty in nanoseconds (EINVAL)", PWMFS_CHIP, h->line);
            return RHU_GERR_EINVAL;
        }
        if (!c->period_set) {
            rhu_shim_log("%s/pwm%u/duty_cycle: written before any period (EINVAL)", PWMFS_CHIP, h->line);
            return RHU_GERR_EINVAL;
        }
        rc = rhu_pwm_set(c->handle, c->period_ns, v, c->polarity);
        if (rc == RHU_EINVAL) {
            rhu_shim_log("%s/pwm%u/duty_cycle: %u ns refused (EINVAL: past the period)", PWMFS_CHIP, h->line, (uint32_t)v);
            return RHU_GERR_EINVAL;
        }
        if (rc != 0) {
            rhu_shim_log("%s/pwm%u/duty_cycle: the set answered %d (%s)", PWMFS_CHIP, h->line, rc, rc == RHU_EBUSY ? "EBUSY" : "EIO");
            return kerr(rc);
        }
        c->duty_ns = v;
        return 0;
    case NODE_POLARITY: {
        uint32_t pol;
        if (strcmp(text, "normal") == 0)
            pol = RHU_PWM_POLARITY_NORMAL;
        else if (strcmp(text, "inversed") == 0)
            pol = RHU_PWM_POLARITY_INVERSED;
        else {
            rhu_shim_log("%s/pwm%u/polarity: neither normal nor inversed (EINVAL)", PWMFS_CHIP, h->line);
            return RHU_GERR_EINVAL;
        }
        if (c->period_set) {
            rc = rhu_pwm_set(c->handle, c->period_ns, c->duty_ns, pol);
            if (rc != 0) {
                rhu_shim_log("%s/pwm%u/polarity: the set answered %d", PWMFS_CHIP, h->line, rc);
                return kerr(rc);
            }
        }
        c->polarity = (uint8_t)pol;
        return 0;
    }
    case NODE_ENABLE:
        if (!take_u64(text, &v) || v > 1) {
            rhu_shim_log("%s/pwm%u/enable: neither 0 nor 1 (EINVAL)", PWMFS_CHIP, h->line);
            return RHU_GERR_EINVAL;
        }
        rc = rhu_pwm_enable(c->handle, (uint32_t)v);
        if (rc == RHU_EINVAL) {
            rhu_shim_log("%s/pwm%u/enable: written before any period (EINVAL)", PWMFS_CHIP, h->line);
            return RHU_GERR_EINVAL;
        }
        if (rc != 0) {
            rhu_shim_log("%s/pwm%u/enable: the kernel answered %d", PWMFS_CHIP, h->line, rc);
            return kerr(rc);
        }
        c->enabled = (uint8_t)v;
        return 0;
    default:
        return RHU_GERR_EBADF;
    }
}

/* ------------------------------------------------------------------ */
/* the path verbs                                                      */
/* ------------------------------------------------------------------ */

int rhu_pwmfs_owns(const char *path)
{
    return path != NULL && strncmp(path, PWMFS_SYS, 4) == 0 && (path[4] == 0 || path[4] == '/');
}

int32_t rhu_pwmfs_stat(const char *path, rhu_pal_filestatus *output)
{
    pwmfs_node n;
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

int32_t rhu_pwmfs_access(const char *path, int32_t mode)
{
    pwmfs_node n;
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

intptr_t rhu_pwmfs_open(const char *path, int32_t flags)
{
    pwmfs_node n;
    int32_t access = flags & RHU_PAL_O_ACCESS_MODE_MASK;
    if (!parse_path(path, &n)) {
        rhu_shim_log("open(%s): no such file under /sys (%s)", path != NULL ? path : "(null)", (flags & RHU_PAL_O_CREAT) != 0 ? "EACCES: nothing is created there" : "ENOENT");
        return refuse((flags & RHU_PAL_O_CREAT) != 0 ? RHU_GERR_EACCES : RHU_GERR_ENOENT);
    }
    if (table_lock() != 0)
        return -1;
    if (!node_exists_locked(&n)) {
        table_unlock();
        rhu_shim_log("open(%s): no such file under /sys (%s)", path, (flags & RHU_PAL_O_CREAT) != 0 ? "EACCES: nothing is created there" : "ENOENT");
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
    uint32_t bits = node_mode(n.kind);
    int wants_read = access == RHU_PAL_O_RDONLY || access == RHU_PAL_O_RDWR;
    int wants_write = access == RHU_PAL_O_WRONLY || access == RHU_PAL_O_RDWR;
    if ((wants_read && (bits & 0x100u) == 0) || (wants_write && (bits & 0x080u) == 0)) {
        table_unlock();
        rhu_shim_log("open(%s): the file is %s (EACCES)", path, (bits & 0x100u) == 0 ? "write-only" : "read-only");
        return refuse(RHU_GERR_EACCES);
    }
    int di = -1;
    for (int i = 0; i < RHU_SHIM_FD_SYSFS_COUNT && di < 0; i++) {
        if (g_fd[i].used == 0)
            di = i;
    }
    if (di < 0) {
        table_unlock();
        rhu_shim_log("open(%s): the sysfs band's %u descriptors are all open (EMFILE)", path, (uint32_t)RHU_SHIM_FD_SYSFS_COUNT);
        return refuse(RHU_GERR_EMFILE);
    }
    g_fd[di].used = 1;
    g_fd[di].kind = n.kind;
    g_fd[di].line = n.line;
    g_fd[di].access = (uint8_t)access;
    g_fd[di].pos = 0;
    table_unlock();
    return RHU_SHIM_FD_SYSFS_LO + di;
}

/* ------------------------------------------------------------------ */
/* the descriptor verbs                                                */
/* ------------------------------------------------------------------ */

int rhu_pwmfs_fd(intptr_t fd)
{
    return fd >= RHU_SHIM_FD_SYSFS_LO && fd < RHU_SHIM_FD_SYSFS_LO + RHU_SHIM_FD_SYSFS_COUNT;
}

/* The descriptor's entry under the lock, or NULL with EBADF set. */
static pwmfs_fd *fd_entry_locked(intptr_t fd)
{
    if (!rhu_pwmfs_fd(fd) || g_fd[fd - RHU_SHIM_FD_SYSFS_LO].used == 0) {
        ERRNO = RHU_GERR_EBADF;
        return NULL;
    }
    return &g_fd[fd - RHU_SHIM_FD_SYSFS_LO];
}

int32_t rhu_pwmfs_close(intptr_t fd)
{
    if (table_lock() != 0)
        return -1;
    pwmfs_fd *h = fd_entry_locked(fd);
    if (h == NULL) {
        table_unlock();
        return -1;
    }
    memset(h, 0, sizeof *h);
    table_unlock();
    return 0;
}

/* A read of the node's text from an offset: the descriptor's position when
 * use_pos is set (and advanced), the offset given otherwise. */
int32_t rhu_pwmfs_pread(intptr_t fd, void *buffer, int32_t bufferSize, int64_t offset, int use_pos)
{
    char text[PWMFS_TEXT_MAX];
    if (buffer == NULL && bufferSize != 0)
        return refuse(RHU_GERR_EFAULT);
    if (bufferSize < 0)
        return refuse(RHU_GERR_EINVAL);
    if (table_lock() != 0)
        return -1;
    pwmfs_fd *h = fd_entry_locked(fd);
    if (h == NULL) {
        table_unlock();
        return -1;
    }
    if (h->access == RHU_PAL_O_WRONLY) {
        table_unlock();
        return refuse(RHU_GERR_EBADF);
    }
    int64_t at = use_pos ? (int64_t)h->pos : offset;
    if (at < 0) {
        table_unlock();
        return refuse(RHU_GERR_EINVAL);
    }
    uint32_t len = text_of_locked(h, text);
    int32_t n = 0;
    if (at < (int64_t)len) {
        n = (int32_t)(len - (uint32_t)at);
        if (n > bufferSize)
            n = bufferSize;
        memcpy(buffer, text + at, (size_t)n);
    }
    if (use_pos)
        h->pos += (uint32_t)n;
    table_unlock();
    return n;
}

/* A write: the bytes as one value, whatever the offset (as Linux parses each
 * write call alone); the position advanced by the count when use_pos is set. */
int32_t rhu_pwmfs_pwrite(intptr_t fd, const void *buffer, int32_t bufferSize, int64_t offset, int use_pos)
{
    char text[PWMFS_TEXT_MAX];
    (void)offset;
    if (buffer == NULL && bufferSize != 0)
        return refuse(RHU_GERR_EFAULT);
    if (bufferSize < 0)
        return refuse(RHU_GERR_EINVAL);
    if (table_lock() != 0)
        return -1;
    pwmfs_fd *h = fd_entry_locked(fd);
    if (h == NULL) {
        table_unlock();
        return -1;
    }
    if (h->access == RHU_PAL_O_RDONLY || h->kind == NODE_NPWM) {
        table_unlock();
        return refuse(RHU_GERR_EBADF);
    }
    if (bufferSize == 0) {
        table_unlock();
        return 0;
    }
    if (!take_text(buffer, bufferSize, text, sizeof text)) {
        table_unlock();
        rhu_shim_log("%s: a write of %d bytes is not one value (EINVAL)", PWMFS_CHIP, bufferSize);
        return refuse(RHU_GERR_EINVAL);
    }
    int32_t err;
    if (h->kind == NODE_EXPORT || h->kind == NODE_UNEXPORT) {
        const char *p = text;
        uint32_t line;
        if (!parse_number(&p, &line) || *p != 0) {
            table_unlock();
            rhu_shim_log("%s/%s: not a channel number (EINVAL)", PWMFS_CHIP, h->kind == NODE_EXPORT ? "export" : "unexport");
            return refuse(RHU_GERR_EINVAL);
        }
        err = h->kind == NODE_EXPORT ? export_locked(line) : unexport_locked(line);
    } else {
        err = attribute_write_locked(h, text);
    }
    if (err == 0 && use_pos)
        h->pos += (uint32_t)bufferSize;
    table_unlock();
    if (err != 0)
        return refuse(err);
    return bufferSize;
}

int64_t rhu_pwmfs_lseek(intptr_t fd, int64_t offset, int32_t whence)
{
    if (table_lock() != 0)
        return -1;
    pwmfs_fd *h = fd_entry_locked(fd);
    if (h == NULL) {
        table_unlock();
        return -1;
    }
    int64_t base = whence == RHU_PAL_SEEK_SET ? 0
                 : whence == RHU_PAL_SEEK_CUR ? (int64_t)h->pos
                 : whence == RHU_PAL_SEEK_END ? (int64_t)PWMFS_ATTR_SIZE
                                              : -1;
    if (base < 0 || base + offset < 0 || base + offset > PWMFS_ATTR_SIZE) {
        table_unlock();
        return refuse(RHU_GERR_EINVAL);
    }
    h->pos = (uint32_t)(base + offset);
    int64_t r = (int64_t)h->pos;
    table_unlock();
    return r;
}

int32_t rhu_pwmfs_fstat(intptr_t fd, rhu_pal_filestatus *output)
{
    if (output == NULL)
        return refuse(RHU_GERR_EFAULT);
    if (table_lock() != 0)
        return -1;
    pwmfs_fd *h = fd_entry_locked(fd);
    if (h == NULL) {
        table_unlock();
        memset(output, 0, sizeof *output);
        return -1;
    }
    pwmfs_node n;
    n.kind = h->kind;
    n.line = h->line;
    table_unlock();
    status_of(output, &n);
    return 0;
}

/* A truncate, a lock, a sync and a descriptor flag change have no effect on
 * an attribute file and answer 0, as Linux answers them. */
int32_t rhu_pwmfs_ftruncate(intptr_t fd, int64_t length)
{
    if (length < 0)
        return refuse(RHU_GERR_EINVAL);
    if (table_lock() != 0)
        return -1;
    pwmfs_fd *h = fd_entry_locked(fd);
    table_unlock();
    return h != NULL ? 0 : -1;
}

int32_t rhu_pwmfs_flock(intptr_t fd, int32_t operation)
{
    (void)operation;
    if (table_lock() != 0)
        return -1;
    pwmfs_fd *h = fd_entry_locked(fd);
    table_unlock();
    return h != NULL ? 0 : -1;
}

int32_t rhu_pwmfs_fsync(intptr_t fd)
{
    if (table_lock() != 0)
        return -1;
    pwmfs_fd *h = fd_entry_locked(fd);
    table_unlock();
    return h != NULL ? 0 : -1;
}

int32_t rhu_pwmfs_fcntl_setfd(intptr_t fd, int32_t flags)
{
    (void)flags;
    if (table_lock() != 0)
        return -1;
    pwmfs_fd *h = fd_entry_locked(fd);
    table_unlock();
    return h != NULL ? 0 : -1;
}

/* A descriptor of this band has no second name at this hosting, as the
 * store's descriptors have none. */
intptr_t rhu_pwmfs_dup(intptr_t fd)
{
    if (table_lock() != 0)
        return -1;
    pwmfs_fd *h = fd_entry_locked(fd);
    table_unlock();
    if (h == NULL)
        return -1;
    rhu_shim_log("dup: a PWM file descriptor has no second name at this hosting (ENOTSUP)");
    return refuse(RHU_GERR_ENOTSUP);
}
