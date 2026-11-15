/* The libgpiod v1 C API over the kernel's GPIO API: the twenty-two
 * entry points the .NET GPIO library's v1 binding names (System.Device.Gpio
 * at the pin of record, its Interop.LibgpiodV1 class), with the binding's
 * own signatures, so a program's `new GpioController()` from the standard
 * package runs unchanged on this system. The SDK names the library as a
 * direct P/Invoke, the compiler binds these symbols at link time, and the
 * runtime's loader is never asked; the package's stock v1 driver then runs
 * as it does on Linux, and its v2 and sysfs probes are never reached.
 *
 * This file is an independent implementation that presents the
 * established libgpiod v1 API. The signatures, the flag values and the
 * enumerations are the binding's (its request flags, its direction and
 * bias readings, its event kinds, its timespec of two 32-bit words and
 * its 12-byte event record). The version string reads 1.6.0, an API
 * version with the bias flags (the driver requires 1.5 or above); it is
 * not the version of any libgpiod build.
 *
 * The mapping from the kernel's seven GPIO calls: a chip open takes the
 * line space (rhu_gpio_count) and allocates ITS OWN chip object, a few
 * words -- two opens are two objects, each the owner of the lines
 * requested through it, and a close gives back that chip's lines and no
 * other's. The chip's lines live in ONE table of one line object per
 * line: the first open makes it, every later open shares it, and the close
 * of the last chip gives it back, so a second open costs its chip object
 * and no second table. A line handle names its chip and its line (the
 * address of the line's object plus the chip's number), so two opens
 * answer two handles for one line, and the line's object names the chip
 * that holds it. A request is the CLAIM (rhu_gpio_claim: an input with its
 * pull, an output at its initial level), refused EBUSY while any chip
 * object of this program holds the line; a release is the RELEASE
 * (rhu_gpio_release); the value calls are the READ and the WRITE
 * (rhu_gpio_read, rhu_gpio_write) on a line the handle's chip holds; a
 * both-edges request is the claim as an input, then THE EVENT WORD
 * (rhu_gpio_event_word) read as the consumed mark BEFORE the EDGE ARM
 * (rhu_gpio_edge, both edges), so an edge the kernel counts from the arm's
 * first instant is an event this unit delivers; the event wait is the
 * kernel's futex wait on that word bounded by the binding's timespec (no
 * timespec: no bound); the event read waits for a counted edge and
 * consumes one, its kind from the pad's level read TOGETHER with the count
 * (the count, the level, the count again, repeated while the count moves),
 * a coalesced burst alternating backwards from the last.
 *
 * The bias, as Linux gives it: a request's flag (disabled, pull-up,
 * pull-down; two of them together refused EINVAL) is applied and is the
 * line's reading while that request lives; a request with no bias flag
 * leaves the pad's pull AS IT IS -- the kernel's claim always programs a
 * pull, so this unit remembers the pull it last applied to each line and
 * applies that again (none, for an output) -- and the reading is "as is".
 *
 * The limits, plainly: one owner per line (the kernel refuses a second
 * claimant EBUSY; this unit refuses a second request on a line it already
 * holds EBUSY until the release); as many chips open at once as a line
 * object has handle steps, eight on this target (one more open is refused
 * EMFILE with one line); a line's direction and bias readings are the
 * line's, through whichever chip's handle they are read; a refused kernel
 * call is named on the
 * wire in one line; both edges only (the level-triggered and wakeup kinds
 * have no entry point in this binding); the open-drain output the kernel
 * serves has no entry point either; the active-low and open-source flags
 * are refused EINVAL; the kernel API counts edges and
 * keeps no record of each one's kind and time (a count and nothing
 * more), so an event's kind is read off the pad when the
 * event is READ and its time is the time of that read; and a pulse shorter
 * than the interrupt handler's latency counts ONCE on a family whose
 * hardware latches one pending bit per line, and is then one event of the
 * level the pad shows afterwards. One thread at a
 * time uses a line handle, as the library asks of its callers; separate
 * handles are safe from separate threads: the chips, their lines'
 * ownership and the iterators change under one mutex of this unit, held
 * for a few instructions and never across a wait. Nothing here masks
 * anything: the kernel's GPIO calls mask nothing, every call runs on the
 * caller's thread, and the wait is the kernel's futex wait. */
#include <stddef.h>
#include <stdint.h>

#include "rhu_shim.h"

#include "rhu.h"
#include "rhu_pal_errno_net11.h"

extern int *__errno_location(void);
void *malloc(size_t n);
void free(void *p);
void *memcpy(void *dst, const void *src, size_t n);
void *memset(void *dst, int c, size_t n);
size_t strlen(const char *s);
int pthread_mutex_lock(void *slot);
int pthread_mutex_unlock(void *slot);
#if defined(RHU_SHIM_BREAK_GPIOD_ARM_ORDER) || defined(RHU_SHIM_BREAK_GPIOD_KIND_ORDER) || defined(RHU_SHIM_BREAK_GPIOD_OPEN_UNLOCKED)
/* The builds under these defines hold their race window OPEN, BLOCKED on the
 * kernel's futex, until the other thread's step is seen -- never for a
 * counted number of turns given away: a turn is a chance the host's timing
 * can spend, and a window that can miss proves nothing. The test program
 * gate/rhu_shimtest.c runs its second thread one step below this thread's
 * level, so that thread runs only while this one is blocked here and can
 * never take its step before the window is open. A window no second thread
 * will ever step into (the test program's thread alone in a request, a read
 * or an open) gives up at this deadline of the kernel's clock, the only way
 * the deadline is reached. */
#define GPIOD_CONTROL_WINDOW_NS 500000000ull
#endif
#ifdef RHU_SHIM_BREAK_GPIOD_OPEN_UNLOCKED
/* The open's window blocks on a word nobody wakes (the head's move is a
 * store the kernel does not announce) and re-reads the head at each wake;
 * the first opener inside holds the window, a second one, arriving while
 * it waits, stores at once. */
static uint32_t g_control_word;
static uint32_t g_control_inside;
#endif

#define ERRNO (*__errno_location())
#define NS_PER_S 1000000000ull

/* The binding's request flags (its RequestFlag enumeration). */
#define GPIOD_REQ_OPEN_DRAIN (1u << 0)
#define GPIOD_REQ_OPEN_SOURCE (1u << 1)
#define GPIOD_REQ_ACTIVE_LOW (1u << 2)
#define GPIOD_REQ_BIAS_DISABLE (1u << 3)
#define GPIOD_REQ_BIAS_PULL_DOWN (1u << 4)
#define GPIOD_REQ_BIAS_PULL_UP (1u << 5)
#define GPIOD_REQ_BIAS_ALL (GPIOD_REQ_BIAS_DISABLE | GPIOD_REQ_BIAS_PULL_DOWN | GPIOD_REQ_BIAS_PULL_UP)
/* The binding's readings of a line's direction and bias. */
#define GPIOD_DIRECTION_INPUT 1
#define GPIOD_DIRECTION_OUTPUT 2
#define GPIOD_BIAS_AS_IS 1
#define GPIOD_BIAS_DISABLE 2
#define GPIOD_BIAS_PULL_UP 3
#define GPIOD_BIAS_PULL_DOWN 4
/* The binding's event kinds, and its wait results. */
#define GPIOD_EVENT_RISING 1
#define GPIOD_EVENT_FALLING 2
#define GPIOD_WAIT_ERROR (-1)
#define GPIOD_WAIT_TIMEOUT 0
#define GPIOD_WAIT_EVENT 1

/* The binding's timespec (two native words on this 32-bit target) and its
 * event record (the timespec, then the kind). */
struct gpiod_timespec {
    int32_t sec;
    int32_t nsec;
};
struct gpiod_line_event {
    struct gpiod_timespec ts;
    int32_t event_type;
};

struct gpiod_line {
    uint32_t offset;
    uint8_t holder;    /* the number of the chip that holds the line, plus one; 0 while none does */
    uint8_t direction; /* the last request's; input when never requested */
    uint8_t bias;      /* the living request's flag; "as is" otherwise */
    uint8_t events;    /* 1 while both-edge events are armed */
    const uint32_t *word; /* the kernel's event word while armed */
    uint32_t mark;        /* the events consumed so far, as the word's value */
    char consumer[16];
};

/* A line handle is the address of the line's object in the table plus the
 * chip's number in steps of the object's alignment: every handle is an
 * address inside its own line's object, and no two chips' handles of one
 * line are equal. The object's size in steps bounds the chips open at
 * once: thirty-two bytes at an alignment of four on this target, so
 * eight. */
#define GPIOD_HANDLE_STEP ((uintptr_t)_Alignof(struct gpiod_line))
#define GPIOD_CHIPS_MAX ((uint32_t)(sizeof(struct gpiod_line) / _Alignof(struct gpiod_line)))
_Static_assert(sizeof(struct gpiod_line) / _Alignof(struct gpiod_line) <= 32u, "the numbers of the open chips fit one word of bits");

/* One open: a few words, linked into the unit's list of open chips. Its
 * number is the lowest that no other open chip has; the table is the one
 * table of the chip's lines, which every open shares. */
struct gpiod_chip {
    struct gpiod_chip *next;
    uint32_t lines;
    uint32_t number;
    struct gpiod_line *table;
};

/* One iterator: its own chip, opened when the iterator is made, and
 * whether it was yielded (a yielded chip is the caller's to close). */
struct gpiod_chip_iter {
    struct gpiod_chip_iter *next;
    struct gpiod_chip *chip;
    int yielded;
};

static struct gpiod_chip *g_chips;
#ifndef RHU_SHIM_BREAK_GPIOD_TABLE_PER_OPEN
/* The one table of the chip's lines while any chip is open. */
static struct gpiod_line *g_table;
#endif
#ifndef RHU_SHIM_BREAK_GPIOD_ITER_SHARED
static struct gpiod_chip_iter *g_iters;
#endif
/* The pull this unit last applied to each line, kept for the boot: the
 * pad's state a flagless request leaves as it is. */
static uint8_t *g_pulls;
static uint32_t g_pull_lines;
static uint8_t g_mutex[24];

#ifdef RHU_SHIM_BREAK_GPIOD_VERSION
/* Built with this define, the version string names an API version without
 * the bias flags, which the driver reads as "no pull modes"; the GPIO
 * section of the managed test program under src/rt/gate fails. */
static const char g_version[] = "1.4.0";
#else
static const char g_version[] = "1.6.0";
#endif

/* The unit's mutex: a zero-filled slot the core adopts at its first use.
 * A lock that fails is a fault of the slot: the call answers ENOLCK (or
 * does nothing, where it answers nothing) and touches no guarded state. */
static int unit_lock(void)
{
    if (pthread_mutex_lock(g_mutex) == 0)
        return 1;
    rhu_shim_log("gpiod: the unit's lock refused (ENOLCK)");
    ERRNO = RHU_GERR_ENOLCK;
    return 0;
}

static void unit_unlock(void)
{
    pthread_mutex_unlock(g_mutex);
}

static const char *code_name(int32_t rc)
{
    switch (rc) {
    case RHU_EBUSY:
        return "EBUSY: the board or another claim holds it";
    case RHU_EACCES:
        return "EACCES: an output on an input-only line, or a write on an input";
    case RHU_ENOTSUP:
        return "ENOTSUP: an event kind the line cannot serve";
    case RHU_ENODEV:
        return "ENODEV: no lines on this row";
    default:
        return "EINVAL: past the space, unpopulated, unclaimed or a bad selector";
    }
}

/* A kernel call's refusal as the binding sees it: errno set from the code
 * (the codes are the platform's numbers, negated), one line on the wire
 * naming the line and the reason, and -1 returned. Never under the lock. */
static int refuse(uint32_t line, const char *what, int32_t rc)
{
    int32_t e;
    switch (rc) {
    case RHU_EBUSY:
        e = RHU_GERR_EBUSY;
        break;
    case RHU_EACCES:
        e = RHU_GERR_EACCES;
        break;
    case RHU_ENOTSUP:
        e = RHU_GERR_ENOTSUP;
        break;
    case RHU_ENODEV:
        e = RHU_GERR_ENODEV;
        break;
    default:
        e = RHU_GERR_EINVAL;
        break;
    }
    rhu_shim_log("gpiod: line %u %s refused (%s)", line, what, code_name(rc));
    ERRNO = e;
    return -1;
}

static int fail(int32_t err)
{
    ERRNO = err;
    return -1;
}

/* A handle is checked by ADDRESS against the objects this unit made, and
 * only then read: a stale or foreign pointer is refused, never followed.
 * Both run under the lock. */
static int chip_live(const struct gpiod_chip *chip)
{
    for (const struct gpiod_chip *c = g_chips; c != NULL; c = c->next) {
        if (c == chip)
            return 1;
    }
    return 0;
}

/* A line handle's line object and its chip: the open chip whose table the
 * handle lies in and whose number the handle carries; NULL for a handle of
 * no open chip. */
static struct gpiod_line *line_of(const struct gpiod_line *handle, struct gpiod_chip **chip)
{
    uintptr_t h = (uintptr_t)handle;
    for (struct gpiod_chip *c = g_chips; c != NULL; c = c->next) {
        uintptr_t lo = (uintptr_t)c->table;
        if (h < lo || h - lo >= (uintptr_t)c->lines * sizeof(struct gpiod_line))
            continue;
        if ((h - lo) % sizeof(struct gpiod_line) != (uintptr_t)c->number * GPIOD_HANDLE_STEP)
            continue;
        *chip = c;
        return &c->table[(h - lo) / sizeof(struct gpiod_line)];
    }
    return NULL;
}

/* Whether this chip is the line's holder. */
static int holds(const struct gpiod_chip *chip, const struct gpiod_line *l)
{
    return l->holder == (uint8_t)(chip->number + 1u);
}

/* Whether any open chip holds this line. */
static int line_held(uint32_t offset)
{
    for (const struct gpiod_chip *c = g_chips; c != NULL; c = c->next) {
        if (offset < c->lines && c->table[offset].holder != 0)
            return 1;
    }
    return 0;
}

/* A table of the chip's lines, every line free: an input, its bias as it
 * is. NULL without memory for it. */
static struct gpiod_line *table_new(uint32_t n)
{
    struct gpiod_line *t = (struct gpiod_line *)malloc((size_t)n * sizeof *t);
    if (t == NULL)
        return NULL;
    memset(t, 0, (size_t)n * sizeof *t);
    for (uint32_t i = 0; i < n; i++) {
        t[i].offset = i;
        t[i].direction = GPIOD_DIRECTION_INPUT;
        t[i].bias = GPIOD_BIAS_AS_IS;
    }
    return t;
}

/* ------------------------------------------------------------------ */
/* the chip                                                            */
/* ------------------------------------------------------------------ */

const char *gpiod_version_string(void)
{
    return g_version;
}

struct gpiod_chip *gpiod_chip_open_by_number(int number)
{
    if (number != 0) {
        ERRNO = RHU_GERR_ENOENT;
        return NULL;
    }
    int32_t n = rhu_gpio_count();
    if (n <= 0) {
        rhu_shim_log("gpiod: chip 0 has no lines on this row (ENODEV)");
        ERRNO = RHU_GERR_ENODEV;
        return NULL;
    }
    if ((size_t)n > SIZE_MAX / sizeof(struct gpiod_line)) {
        ERRNO = RHU_GERR_ENOMEM;
        return NULL;
    }
    /* What an open asks of the heap, largest first: the one table of the
     * chip's lines, then its chip object, then the unit's pulls -- the
     * table and the pulls only where a look under the lock found none
     * standing. A close gives back the table's room and the chip object's
     * and no other, so the open that follows the last close asks for those
     * two rooms again, the table before any smaller block of its own:
     * where the first open was given, it is given. Each block is made
     * outside the lock; the loop's last pass holds the lock with all of
     * them in hand. An open that finds the table takes the room of its
     * chip object and no more. */
    struct gpiod_chip *c = NULL;
    struct gpiod_line *t = NULL;
    uint8_t *pulls = NULL;
#ifdef RHU_SHIM_BREAK_GPIOD_PULLS_FIRST
    /* Built with this define, every open asks its chip object and a block
     * of pulls before it looks for the table, and the reopen check in
     * gate/rhu_shimtest.c fails. */
    c = (struct gpiod_chip *)malloc(sizeof *c);
    pulls = (uint8_t *)malloc((size_t)n);
    if (c == NULL || pulls == NULL) {
        free(c);
        free(pulls);
        ERRNO = RHU_GERR_ENOMEM;
        return NULL;
    }
    memset(pulls, RHU_GPIO_PULL_NONE, (size_t)n);
#endif
    for (;;) {
        if (!unit_lock()) {
            free(c);
            free(t);
            free(pulls);
            return NULL;
        }
#ifdef RHU_SHIM_BREAK_GPIOD_TABLE_PER_OPEN
        /* Built with this define, every open makes a table of its own, and
         * the two-open check in gate/rhu_shimtest.c fails. */
        int table_due = t == NULL;
#else
        int table_due = t == NULL && g_table == NULL;
#endif
        int pulls_due = pulls == NULL && g_pulls == NULL;
        if (c != NULL && !table_due && !pulls_due)
            break;
        unit_unlock();
        int given = 1;
        if (table_due)
            given = (t = table_new((uint32_t)n)) != NULL;
        if (given && c == NULL)
            given = (c = (struct gpiod_chip *)malloc(sizeof *c)) != NULL;
        if (given && pulls_due) {
            given = (pulls = (uint8_t *)malloc((size_t)n)) != NULL;
            if (given)
                memset(pulls, RHU_GPIO_PULL_NONE, (size_t)n);
        }
        if (!given) {
            free(c);
            free(t);
            free(pulls);
            ERRNO = RHU_GERR_ENOMEM;
            return NULL;
        }
    }
    c->lines = (uint32_t)n;
    /* The chip's number: the lowest that no open chip has. */
    uint32_t taken = 0;
    for (const struct gpiod_chip *o = g_chips; o != NULL; o = o->next)
        taken |= 1u << o->number;
    uint32_t mine = 0;
    while (mine < GPIOD_CHIPS_MAX && (taken & (1u << mine)) != 0)
        mine++;
    if (mine >= GPIOD_CHIPS_MAX) {
        unit_unlock();
        free(c);
        free(t);
        free(pulls);
        rhu_shim_log("gpiod: chip 0 is open %u times, the most at once (EMFILE)", (unsigned)GPIOD_CHIPS_MAX);
        ERRNO = RHU_GERR_EMFILE;
        return NULL;
    }
    c->number = mine;
#ifdef RHU_SHIM_BREAK_GPIOD_TABLE_PER_OPEN
    c->table = t;
    t = NULL;
#else
    if (g_table == NULL) {
        g_table = t;
        t = NULL;
    }
    c->table = g_table;
#endif
    if (g_pulls == NULL) {
        g_pulls = pulls;
        g_pull_lines = (uint32_t)n;
        pulls = NULL;
    }
#ifdef RHU_SHIM_BREAK_GPIOD_OPEN_UNLOCKED
    /* Built with this define, the list is joined with no lock, the window
     * between the read of its head and the store held open, blocked,
     * until another opener has moved the head. With two threads inside
     * the open together both read one head, the second to arrive stores
     * its chip at once, and the first then stores the stale head over it:
     * one chip is lost from the list every time, and the two-thread open
     * check in gate/rhu_shimtest.c fails. */
    unit_unlock();
    struct gpiod_chip *head = g_chips;
    if (__atomic_fetch_add(&g_control_inside, 1u, __ATOMIC_ACQ_REL) == 0u) {
        uint64_t deadline = rhu_now_ns() + GPIOD_CONTROL_WINDOW_NS;
        while (*(struct gpiod_chip *volatile *)&g_chips == head && rhu_now_ns() < deadline)
            rhu_futex_wait(&g_control_word, 0u, deadline);
    }
    c->next = head;
    g_chips = c;
    __atomic_fetch_sub(&g_control_inside, 1u, __ATOMIC_ACQ_REL);
#else
    c->next = g_chips;
    g_chips = c;
    unit_unlock();
#endif
    /* A table another opener put in place between this open's two looks
     * makes the one in hand spare. */
    free(t);
    free(pulls);
    return c;
}

/* A held line given back, under the lock: the edge disarmed, the kernel's
 * release, the object's request state cleared. The kernel's refusal, if
 * any, is returned for the caller's line outside the lock. */
static int32_t release_locked(struct gpiod_line *l)
{
    if (l->events)
        rhu_gpio_edge(l->offset, RHU_GPIO_EDGE_NONE);
    int32_t rc = rhu_gpio_release(l->offset);
    l->holder = 0;
    l->events = 0;
    l->word = NULL;
    l->mark = 0;
    l->bias = GPIOD_BIAS_AS_IS;
    return rc;
}

void gpiod_chip_close(struct gpiod_chip *chip)
{
    if (!unit_lock())
        return;
    if (!chip_live(chip)) {
        unit_unlock();
        return;
    }
#ifdef RHU_SHIM_BREAK_GPIOD_CLOSE_KEEPS
    /* Built with this define, a closed chip keeps its lines, and the
     * two-open check in gate/rhu_shimtest.c fails. */
    unit_unlock();
#else
    for (struct gpiod_chip **p = &g_chips; *p != NULL; p = &(*p)->next) {
        if (*p == chip) {
            *p = chip->next;
            break;
        }
    }
    /* The close gives back every line this chip handed out and still
     * holds -- the library's own rule for a chip close -- and no other
     * chip's: a line's object names its holder. */
    for (uint32_t i = 0; i < chip->lines; i++) {
        if (holds(chip, &chip->table[i]))
            (void)release_locked(&chip->table[i]);
    }
    /* The one table goes back with the last open chip. */
    struct gpiod_line *gone = chip->table;
#ifndef RHU_SHIM_BREAK_GPIOD_TABLE_PER_OPEN
    if (g_chips != NULL)
        gone = NULL;
    else
        g_table = NULL;
#endif
    unit_unlock();
    free(gone);
    free(chip);
#endif
}

int gpiod_chip_num_lines(struct gpiod_chip *chip)
{
    int n = 0;
    if (unit_lock()) {
        if (chip_live(chip))
            n = (int)chip->lines;
        unit_unlock();
    }
    return n;
}

static int chip_is_live(struct gpiod_chip *chip)
{
    int live = 0;
    if (unit_lock()) {
        live = chip_live(chip);
        unit_unlock();
    }
    return live;
}

const char *gpiod_chip_name(struct gpiod_chip *chip)
{
    return chip_is_live(chip) ? "gpiochip0" : "";
}

const char *gpiod_chip_label(struct gpiod_chip *chip)
{
    return chip_is_live(chip) ? "rhu" : "";
}

#ifdef RHU_SHIM_BREAK_GPIOD_ITER_SHARED
/* Built with this define, every iterator is one object whose progress a new
 * iterator resets, and the two-iterator check in gate/rhu_shimtest.c fails. */
static struct gpiod_chip_iter g_shared_iter;
#endif

struct gpiod_chip_iter *gpiod_chip_iter_new(void)
{
#ifdef RHU_SHIM_BREAK_GPIOD_ITER_SHARED
    g_shared_iter.chip = NULL;
    g_shared_iter.yielded = 0;
    return &g_shared_iter;
#else
    /* The iterator's chip is opened HERE, so a failure is the making's
     * (NULL with the open's errno), never a silent early end. */
    struct gpiod_chip_iter *it = (struct gpiod_chip_iter *)malloc(sizeof *it);
    if (it == NULL) {
        ERRNO = RHU_GERR_ENOMEM;
        return NULL;
    }
    it->chip = gpiod_chip_open_by_number(0);
    if (it->chip == NULL) {
        free(it);
        return NULL;
    }
    it->yielded = 0;
    if (!unit_lock()) {
        gpiod_chip_close(it->chip);
        free(it);
        return NULL;
    }
    it->next = g_iters;
    g_iters = it;
    unit_unlock();
    return it;
#endif
}

struct gpiod_chip *gpiod_chip_iter_next_noclose(struct gpiod_chip_iter *iter)
{
#ifdef RHU_SHIM_BREAK_GPIOD_ITER_SHARED
    if (iter != &g_shared_iter || iter->yielded)
        return NULL;
    iter->yielded = 1;
    return gpiod_chip_open_by_number(0);
#else
    struct gpiod_chip *c = NULL;
    if (!unit_lock())
        return NULL;
    for (struct gpiod_chip_iter *it = g_iters; it != NULL; it = it->next) {
        if (it == iter && !it->yielded) {
            it->yielded = 1;
            c = it->chip;
        }
    }
    unit_unlock();
    return c;
#endif
}

void gpiod_chip_iter_free_noclose(struct gpiod_chip_iter *iter)
{
#ifdef RHU_SHIM_BREAK_GPIOD_ITER_SHARED
    (void)iter;
#else
    struct gpiod_chip *unyielded = NULL;
    int found = 0;
    if (!unit_lock())
        return;
    for (struct gpiod_chip_iter **p = &g_iters; *p != NULL; p = &(*p)->next) {
        if (*p == iter) {
            *p = iter->next;
            found = 1;
            break;
        }
    }
    if (found && !iter->yielded)
        unyielded = iter->chip;
    unit_unlock();
    if (!found)
        return;
    /* A chip the caller never received is the iterator's to close. */
    if (unyielded != NULL)
        gpiod_chip_close(unyielded);
    free(iter);
#endif
}

struct gpiod_line *gpiod_chip_get_line(struct gpiod_chip *chip, int offset)
{
    struct gpiod_line *l = NULL;
    if (!unit_lock())
        return NULL;
    if (chip_live(chip) && offset >= 0 && (uint32_t)offset < chip->lines)
        l = (struct gpiod_line *)((uintptr_t)&chip->table[offset] + (uintptr_t)chip->number * GPIOD_HANDLE_STEP);
    unit_unlock();
    if (l == NULL)
        ERRNO = RHU_GERR_EINVAL;
    return l;
}

/* ------------------------------------------------------------------ */
/* the requests: the claim, in its shapes                              */
/* ------------------------------------------------------------------ */

static void remember_consumer(struct gpiod_line *l, const char *consumer)
{
    size_t n = consumer != NULL ? strlen(consumer) : 0;
    if (n >= sizeof l->consumer)
        n = sizeof l->consumer - 1;
    if (n != 0)
        memcpy(l->consumer, consumer, n);
    l->consumer[n] = 0;
}

/* One request, whole or refused. The order of its checks is the
 * library's: the object, then whether the line is held (EBUSY), then the
 * flags (EINVAL). */
static int request(struct gpiod_line *line, const char *consumer, uint32_t mode, uint32_t flags, uint32_t initial, int events)
{
    if (!unit_lock())
        return -1;
    struct gpiod_chip *chip = NULL;
    struct gpiod_line *l = line_of(line, &chip);
    if (l == NULL) {
        unit_unlock();
        return fail(RHU_GERR_EINVAL);
    }
    uint32_t offset = l->offset;
    /* The holder a granted request writes into the line's object: the
     * handle's chip. */
    uint8_t holder = (uint8_t)(chip->number + 1u);
    if (line_held(offset)) {
        unit_unlock();
        return fail(RHU_GERR_EBUSY);
    }
    if ((flags & (GPIOD_REQ_OPEN_DRAIN | GPIOD_REQ_OPEN_SOURCE | GPIOD_REQ_ACTIVE_LOW)) != 0) {
        unit_unlock();
        rhu_shim_log("gpiod: line %u claim refused (EINVAL: the open-drain, open-source and active-low flags are not served on an input)", offset);
        return fail(RHU_GERR_EINVAL);
    }
    uint32_t bias_flags = flags & GPIOD_REQ_BIAS_ALL;
#ifndef RHU_SHIM_BREAK_GPIOD_BOTH_PULLS
    if ((bias_flags & (bias_flags - 1u)) != 0) {
        unit_unlock();
        rhu_shim_log("gpiod: line %u claim refused (EINVAL: more than one bias flag)", offset);
        return fail(RHU_GERR_EINVAL);
    }
#endif
    /* The pull: the flag's; with no flag the one last applied to the pad
     * (as it is); none for an output. */
    uint8_t bias = GPIOD_BIAS_AS_IS;
    uint32_t pull = offset < g_pull_lines ? g_pulls[offset] : RHU_GPIO_PULL_NONE;
    if (mode != RHU_GPIO_INPUT) {
        pull = RHU_GPIO_PULL_NONE;
    } else if ((bias_flags & GPIOD_REQ_BIAS_PULL_UP) != 0) {
        bias = GPIOD_BIAS_PULL_UP;
        pull = RHU_GPIO_PULL_UP;
    } else if ((bias_flags & GPIOD_REQ_BIAS_PULL_DOWN) != 0) {
        bias = GPIOD_BIAS_PULL_DOWN;
        pull = RHU_GPIO_PULL_DOWN;
    } else if ((bias_flags & GPIOD_REQ_BIAS_DISABLE) != 0) {
        bias = GPIOD_BIAS_DISABLE;
        pull = RHU_GPIO_PULL_NONE;
    }
    const char *what = "claim";
    int32_t rc = rhu_gpio_claim(offset, mode, pull, initial);
    if (rc >= 0 && events) {
#ifdef RHU_SHIM_BREAK_GPIOD_ARM_ORDER
        /* Built with this define, the arm comes first and the mark is read
         * after, the window between them held open, blocked on the line's
         * word, until the kernel has counted an edge; that edge is swallowed
         * by the mark, and the arm check in gate/rhu_shimtest.c fails. */
        what = "edge arm";
        rc = rhu_gpio_edge(offset, RHU_GPIO_EDGE_BOTH);
        const uint32_t *w = rc < 0 ? NULL : rhu_gpio_event_word(offset);
        if (rc >= 0 && w != NULL) {
            uint32_t armed_at = *(const volatile uint32_t *)w;
            unit_unlock();
            uint64_t deadline = rhu_now_ns() + GPIOD_CONTROL_WINDOW_NS;
            while (*(const volatile uint32_t *)w == armed_at && rhu_now_ns() < deadline)
                rhu_futex_wait(w, armed_at, deadline);
            if (!unit_lock())
                return -1;
        }
        if (rc >= 0 && w == NULL) {
            what = "event word";
            rc = RHU_EINVAL;
        }
        if (rc >= 0) {
            l->word = w;
            l->mark = *(const volatile uint32_t *)w;
        }
#else
        /* The word is any claimed line's, and the kernel counts an edge
         * only while one is armed: the mark read BEFORE the arm is the
         * count at the arm's first instant, exactly. */
        const uint32_t *w = rhu_gpio_event_word(offset);
        if (w == NULL) {
            what = "event word";
            rc = RHU_EINVAL;
        } else {
            l->word = w;
            l->mark = *(const volatile uint32_t *)w;
            what = "edge arm";
            rc = rhu_gpio_edge(offset, RHU_GPIO_EDGE_BOTH);
        }
#endif
        if (rc < 0) {
            /* The request is whole or refused: the claim goes back. */
            rhu_gpio_edge(offset, RHU_GPIO_EDGE_NONE);
            rhu_gpio_release(offset);
            l->word = NULL;
            l->mark = 0;
        }
    }
    if (rc < 0) {
        unit_unlock();
        return refuse(offset, what, rc);
    }
    if (offset < g_pull_lines)
        g_pulls[offset] = (uint8_t)pull;
    l->holder = holder;
    l->events = events ? 1 : 0;
    l->direction = mode == RHU_GPIO_INPUT ? GPIOD_DIRECTION_INPUT : GPIOD_DIRECTION_OUTPUT;
#ifdef RHU_SHIM_BREAK_GPIOD_BIAS_STALE
    /* Built with this define, the reading keeps the last flagged request's
     * bias through flagless requests and outputs, and the bias check in
     * gate/rhu_shimtest.c fails. */
    if (bias != GPIOD_BIAS_AS_IS)
        l->bias = bias;
#else
    l->bias = bias;
#endif
    remember_consumer(l, consumer);
    unit_unlock();
    return 0;
}

int gpiod_line_request_input(struct gpiod_line *line, const char *consumer)
{
    return request(line, consumer, RHU_GPIO_INPUT, 0, 0, 0);
}

int gpiod_line_request_input_flags(struct gpiod_line *line, const char *consumer, int flags)
{
    return request(line, consumer, RHU_GPIO_INPUT, (uint32_t)flags, 0, 0);
}

int gpiod_line_request_output(struct gpiod_line *line, const char *consumer, int default_val)
{
    return request(line, consumer, RHU_GPIO_OUTPUT, 0, default_val != 0 ? 1u : 0u, 0);
}

int gpiod_line_request_both_edges_events(struct gpiod_line *line, const char *consumer)
{
    return request(line, consumer, RHU_GPIO_INPUT, 0, 0, 1);
}

void gpiod_line_release(struct gpiod_line *line)
{
    if (!unit_lock())
        return;
    struct gpiod_chip *chip = NULL;
    struct gpiod_line *l = line_of(line, &chip);
    if (l == NULL || !holds(chip, l)) {
        unit_unlock();
        return;
    }
    uint32_t offset = l->offset;
#ifdef RHU_SHIM_BREAK_GPIOD_BIAS_STALE
    uint8_t keep = l->bias;
#endif
    int32_t rc = release_locked(l);
#ifdef RHU_SHIM_BREAK_GPIOD_BIAS_STALE
    l->bias = keep;
#endif
    unit_unlock();
    if (rc < 0)
        (void)refuse(offset, "release", rc);
}

/* ------------------------------------------------------------------ */
/* the values and the readings                                         */
/* ------------------------------------------------------------------ */

/* A value call's line: live, and held through this handle's chip (EPERM
 * when it is not, the library's answer); its offset and direction out. */
static int held_line(struct gpiod_line *line, uint32_t *offset, uint8_t *direction)
{
    if (!unit_lock())
        return -1;
    struct gpiod_chip *chip = NULL;
    struct gpiod_line *l = line_of(line, &chip);
    if (l == NULL) {
        unit_unlock();
        return fail(RHU_GERR_EINVAL);
    }
    if (!holds(chip, l)) {
        unit_unlock();
        return fail(RHU_GERR_EPERM);
    }
    *offset = l->offset;
    *direction = l->direction;
    unit_unlock();
    return 0;
}

int gpiod_line_set_value(struct gpiod_line *line, int value)
{
    uint32_t offset;
    uint8_t direction;
    if (held_line(line, &offset, &direction) != 0)
        return -1;
    if (direction != GPIOD_DIRECTION_OUTPUT)
        return fail(RHU_GERR_EPERM);
    int32_t rc = rhu_gpio_write(offset, value != 0 ? 1u : 0u);
    return rc < 0 ? refuse(offset, "write", rc) : 0;
}

int gpiod_line_get_value(struct gpiod_line *line)
{
    uint32_t offset;
    uint8_t direction;
    if (held_line(line, &offset, &direction) != 0)
        return -1;
    int32_t rc = rhu_gpio_read(offset);
    return rc < 0 ? refuse(offset, "read", rc) : (rc != 0 ? 1 : 0);
}

int gpiod_line_is_free(struct gpiod_line *line)
{
    int is_free = 0;
    if (unit_lock()) {
        struct gpiod_chip *chip = NULL;
        const struct gpiod_line *l = line_of(line, &chip);
        is_free = l != NULL && !holds(chip, l);
        unit_unlock();
    }
    return is_free;
}

int gpiod_line_direction(struct gpiod_line *line)
{
    int d = GPIOD_DIRECTION_INPUT;
    if (unit_lock()) {
        struct gpiod_chip *chip = NULL;
        const struct gpiod_line *l = line_of(line, &chip);
        if (l != NULL)
            d = l->direction;
        unit_unlock();
    }
    return d;
}

int gpiod_line_bias(struct gpiod_line *line)
{
    int b = GPIOD_BIAS_AS_IS;
    if (unit_lock()) {
        struct gpiod_chip *chip = NULL;
        const struct gpiod_line *l = line_of(line, &chip);
        if (l != NULL)
            b = l->bias;
        unit_unlock();
    }
    return b;
}

/* ------------------------------------------------------------------ */
/* the events: the wait on the word, the read of one counted edge      */
/* ------------------------------------------------------------------ */

/* The binding's relative timeout as a kernel deadline on the boot clock:
 * no timespec is no bound; a field outside its range is refused (0 out);
 * the sum saturated below the no-deadline word. */
static int deadline_of(const struct gpiod_timespec *t, uint64_t *deadline)
{
    if (t == NULL) {
        *deadline = RHU_DEADLINE_NONE;
        return 1;
    }
    int32_t sec = t->sec;
    int32_t nsec = t->nsec;
    uint64_t now = rhu_now_ns();
#ifdef RHU_SHIM_BREAK_GPIOD_TIMEOUT_FIELDS
    /* Built with this define, a field outside its range becomes an ordinary
     * deadline, and the timeout check in gate/rhu_shimtest.c fails. */
    if (sec < 0 || nsec < 0) {
        *deadline = now;
        return 1;
    }
#else
    if (sec < 0 || nsec < 0 || nsec >= (int32_t)NS_PER_S)
        return 0;
#endif
    uint64_t rel = (uint64_t)sec * NS_PER_S + (uint64_t)nsec;
    *deadline = rel > RHU_DEADLINE_NONE - 1u - now ? RHU_DEADLINE_NONE - 1u : now + rel;
    return 1;
}

/* An event call's line: live and holding an event request (EPERM when it
 * does not, the library's answer); its word, mark and offset out. */
static int event_line(struct gpiod_line *line, const uint32_t **word, uint32_t *mark, uint32_t *offset)
{
    if (!unit_lock())
        return -1;
    struct gpiod_chip *chip = NULL;
    const struct gpiod_line *l = line_of(line, &chip);
    if (l == NULL) {
        unit_unlock();
        return fail(RHU_GERR_EINVAL);
    }
    if (!holds(chip, l) || !l->events || l->word == NULL) {
        unit_unlock();
        return fail(RHU_GERR_EPERM);
    }
    *word = l->word;
    *mark = l->mark;
    *offset = l->offset;
    unit_unlock();
    return 0;
}

#ifdef RHU_SHIM_BREAK_GPIOD_ACTIVATION
/* Built with this define, after the first activation delivered during a
 * wait every later wait answers a timeout, so no later edge reaches the
 * program; the GPIO section of the managed test program under src/rt/gate
 * fails at its collection-during-wait case. */
static int g_break_dead;
#endif

/* The wait on the word from a mark, to a deadline: 1 an edge past the
 * mark, 0 the deadline, -1 a refusal. The kernel's event word stays
 * mapped for the boot, so a release from another thread during the wait
 * makes the count read stale, never a fault. */
static int wait_past(const uint32_t *word, uint32_t mark, uint64_t deadline)
{
    for (;;) {
        uint32_t cur = *(const volatile uint32_t *)word;
        if (cur != mark)
            return GPIOD_WAIT_EVENT;
        int32_t rc = rhu_futex_wait(word, mark, deadline);
        if (rc == RHU_EAGAIN)
            return GPIOD_WAIT_EVENT;
        if (rc == RHU_ETIMEDOUT)
            return GPIOD_WAIT_TIMEOUT;
        if (rc == RHU_EINTR) {
            /* An activation was delivered during the wait: the kernel's
             * contract is to re-evaluate the word and re-wait, exactly as
             * for a spurious wake, with the deadline kept. (In practice
             * the runtime suspends a thread blocked outside
             * managed code lazily, at its return, and sends it no
             * activation, so none reaches this wait during a collection;
             * the re-wait is the contract's path, not one a collection
             * takes.) The driver's own EINTR path is never reached, and
             * nothing is asked of it. */
#ifdef RHU_SHIM_BREAK_GPIOD_ACTIVATION
            g_break_dead = 1;
            return GPIOD_WAIT_TIMEOUT;
#else
            continue;
#endif
        }
        if (rc < 0) {
            ERRNO = RHU_GERR_EINVAL;
            return GPIOD_WAIT_ERROR;
        }
        /* Woken: the word is re-read at the top; a wake without a count
         * (a spurious one) waits again until the deadline. */
    }
}

int gpiod_line_event_wait(struct gpiod_line *line, const struct gpiod_timespec *timeout)
{
    const uint32_t *word;
    uint32_t mark, offset;
    if (event_line(line, &word, &mark, &offset) != 0)
        return GPIOD_WAIT_ERROR;
    uint64_t deadline;
    if (!deadline_of(timeout, &deadline)) {
        ERRNO = RHU_GERR_EINVAL;
        return GPIOD_WAIT_ERROR;
    }
#ifdef RHU_SHIM_BREAK_GPIOD_ACTIVATION
    if (g_break_dead)
        return GPIOD_WAIT_TIMEOUT;
#endif
    return wait_past(word, mark, deadline);
}

/* How many times the count-and-level read repeats while edges keep
 * arriving, before the last pair read is taken as it is. */
#define GPIOD_COHERENT_TRIES 8

int gpiod_line_event_read(struct gpiod_line *line, struct gpiod_line_event *event)
{
    if (event == NULL)
        return fail(RHU_GERR_EINVAL);
    for (;;) {
        const uint32_t *word;
        uint32_t mark, offset;
        if (event_line(line, &word, &mark, &offset) != 0)
            return -1;
        /* Nothing counted yet: the read waits, as the library's does,
         * outside the lock. */
        int w = wait_past(word, mark, RHU_DEADLINE_NONE);
        if (w != GPIOD_WAIT_EVENT)
            return -1;
        /* The count and the pad's level TOGETHER: the level names the
         * LAST counted edge's kind only if no edge arrived between the
         * two reads, so the count is read again after the level and the
         * pair retaken while it moves. */
        uint32_t cur = 0;
        int32_t level = 0;
        for (int tries = 0; tries < GPIOD_COHERENT_TRIES; tries++) {
            cur = *(const volatile uint32_t *)word;
#ifdef RHU_SHIM_BREAK_GPIOD_KIND_ORDER
            /* Built with this define, the count is read, then the level, never
             * checked against the count again, the window between them held
             * open, blocked on the word, until another edge has been counted;
             * the level is then a later edge's, and the kind check in
             * gate/rhu_shimtest.c fails. */
            uint64_t deadline = rhu_now_ns() + GPIOD_CONTROL_WINDOW_NS;
            while (*(const volatile uint32_t *)word == cur && rhu_now_ns() < deadline)
                rhu_futex_wait(word, cur, deadline);
            level = rhu_gpio_read(offset);
            break;
#else
            level = rhu_gpio_read(offset);
            if (level < 0 || *(const volatile uint32_t *)word == cur)
                break;
#endif
        }
        if (level < 0)
            return refuse(offset, "read", level);
        /* Consumed under the lock, from the mark this read started at: a
         * second reader of the same object that got there first sends
         * this one around again. */
        if (!unit_lock())
            return -1;
        struct gpiod_chip *chip = NULL;
        struct gpiod_line *l = line_of(line, &chip);
        if (l == NULL || !holds(chip, l) || !l->events || l->word != word) {
            unit_unlock();
            return fail(RHU_GERR_EPERM);
        }
        if (l->mark != mark) {
            unit_unlock();
            continue;
        }
        uint32_t pending = cur - mark;
        l->mark = mark + 1u;
        unit_unlock();
        /* An earlier edge of a coalesced burst is the opposite of the one
         * after it. */
        int last = level != 0 ? GPIOD_EVENT_RISING : GPIOD_EVENT_FALLING;
        int kind = (pending & 1u) != 0 ? last : (last == GPIOD_EVENT_RISING ? GPIOD_EVENT_FALLING : GPIOD_EVENT_RISING);
        uint64_t now = rhu_now_ns();
        event->ts.sec = (int32_t)(now / NS_PER_S);
        event->ts.nsec = (int32_t)(now % NS_PER_S);
        event->event_type = kind;
        return 0;
    }
}
