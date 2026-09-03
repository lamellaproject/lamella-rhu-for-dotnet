/* The stream bridge, deliberately without the standard stream header.
 *
 * Two C libraries meet in this image. The runtime's platform layer was
 * compiled against a library that spells the standard error stream as a
 * DATA SYMBOL, and it links against that symbol by name. The C library
 * this image actually links spells the same stream as a MACRO over an
 * indirection through its own per-task record -- so the name the runtime
 * wants does not exist as a symbol, and including the stream header here
 * would turn every mention of it into that macro before the linker ever
 * saw an identifier to define.
 *
 * So this file defines the missing data symbol and points it at the real
 * stream object, and states the shape of that object itself rather than
 * including a header that would rewrite the names. Every layout fact and
 * flag value it relies on comes from the generated header, which is read
 * off the C library's own headers by a compile-only probe under the pinned
 * kit; the mirror records below are asserted against those values at
 * compile time, so a library whose layout moved refuses to build this file
 * instead of writing the wrong word.
 */
#include <stddef.h>
#include <stdint.h>

#include "rhu_newlib_arm32.h"

/* The C library's per-task record, from which the standard streams hang.
 * Only the leading fields are named: errno, then the three standard
 * streams. The record is reached through the library's own accessor so
 * this file never has to know where the task pointer lives. */
struct rhu_libc_task;
extern struct rhu_libc_task *__getreent(void) __attribute__((weak));
extern struct rhu_libc_task *_impure_ptr __attribute__((weak));

/* The stream object's first fields, in order, as the C library lays them
 * out: the read pointer, the read count, the write count, the flag word
 * and the descriptor. Only the flag word is read or written. */
struct rhu_libc_stream {
    unsigned char *rp;
    int rcount;
    int wcount;
    short flags;
    short file;
};
_Static_assert(offsetof(struct rhu_libc_stream, flags) == RHU_NEWLIB_STREAM_FLAGS_OFFSET, "the stream flag word's offset");
_Static_assert(sizeof(((struct rhu_libc_stream *)0)->flags) == RHU_NEWLIB_STREAM_FLAGS_SIZE, "the stream flag word's width");
_Static_assert(offsetof(struct rhu_libc_stream, file) == RHU_NEWLIB_STREAM_FILE_OFFSET, "the stream descriptor's offset");

/* The buffering bits of that flag word, as the library defines them. Line
 * buffering is what a gate diffing transcripts needs: without it two
 * threads writing at once fuse their output mid-line and the diff reports
 * damage that is really just interleaving. */
#define RHU_LIBC_STREAM_LINEBUF RHU_NEWLIB_STREAM_LINEBUF
#define RHU_LIBC_STREAM_NOBUF RHU_NEWLIB_STREAM_NOBUF

/* The task record's leading fields: errno, then in, out, err. */
struct rhu_libc_task_head {
    int errno_slot;
    struct rhu_libc_stream *sin;
    struct rhu_libc_stream *sout;
    struct rhu_libc_stream *serr;
};
_Static_assert(offsetof(struct rhu_libc_task_head, errno_slot) == RHU_NEWLIB_TASK_ERRNO_OFFSET, "the task record's errno offset");
_Static_assert(offsetof(struct rhu_libc_task_head, sin) == RHU_NEWLIB_TASK_STDIN_OFFSET, "the task record's standard input offset");
_Static_assert(offsetof(struct rhu_libc_task_head, sout) == RHU_NEWLIB_TASK_STDOUT_OFFSET, "the task record's standard output offset");
_Static_assert(offsetof(struct rhu_libc_task_head, serr) == RHU_NEWLIB_TASK_STDERR_OFFSET, "the task record's standard error offset");

/* The data symbol the runtime's platform layer links against. Defined
 * here, filled in at bind time. */
void *stderr;

static struct rhu_libc_task_head *task_head(void)
{
    struct rhu_libc_task *t = NULL;

    if (__getreent != NULL)
        t = __getreent();
    else if (&_impure_ptr != NULL)
        t = _impure_ptr;
    return (struct rhu_libc_task_head *)t;
}

/* Bind the data symbol to the library's own error stream. With no C
 * library linked, there is no stream object and the symbol stays null --
 * which is the truth, and better than a pointer to something invented. */
void rhu_stdio_bind_stderr(void)
{
    struct rhu_libc_task_head *h = task_head();

    stderr = (h != NULL) ? (void *)h->serr : NULL;
}

/* Ask the library's standard output for line buffering. Returns 0 when
 * the stream now line-buffers -- including the case where there is no
 * library stream at all, because this image's own console writer emits
 * one whole line per call and cannot interleave. */
int rhu_stdio_line_buffer_stdout(void)
{
    struct rhu_libc_task_head *h = task_head();

    if (h == NULL || h->sout == NULL)
        return 0;
    h->sout->flags = (short)((h->sout->flags & ~RHU_LIBC_STREAM_NOBUF) | RHU_LIBC_STREAM_LINEBUF);
    return (h->sout->flags & RHU_LIBC_STREAM_LINEBUF) != 0 ? 0 : -1;
}
