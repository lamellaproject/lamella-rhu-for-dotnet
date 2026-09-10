/* The host: the initialization contract that runs before managed code and
 * the accounting that runs after it, in the image's own payload entry.
 * The ORDER is the contract -- each step depends on the one before it,
 * and each prints one marker naming what it did, so a transcript is a
 * proof of sequence and not just of outcome. Nothing here is optional
 * and nothing here is silent: a step that cannot do its work says so
 * and ends the run rather than leaving a later stage to fail somewhere
 * less legible.
 *
 * The steps, in order:
 *
 *   1  the standard error stream is bound to the C library's own object
 *   2  standard output is line buffered, so concurrent writers cannot
 *      fuse two lines into one
 *   3  the initialized data region is verified against the image it was
 *      loaded from -- a dead memory path dies here, not later inside the
 *      collector
 *   4  the static constructors run, the early array and then the main
 *      one, and they must run before the runtime is initialized
 *   5  the runtime is initialized
 *   6  the image registers itself with the runtime: its module handle,
 *      its managed-code and unboxing-stub ranges as the map brackets
 *      them, and the class-library function table
 *   7  the module list is initialized under the same handle -- after the
 *      registration, because the runtime reads what the registration
 *      gave it
 *   8  the argument vector is prepared
 *   9  managed entry
 *   10 the arena is audited BEFORE the exit line is printed, so a leak is
 *      reported by a run that ends normally
 *   11 the return code becomes the machine's exit code
 *
 * The kernel hands its hosting thread to the payload entry once the
 * platform is up. This file defines that entry, so the whole order above
 * is the host's, and it states the hosting-API contract generation it
 * was built for beside it, for the kernel's handshake. The module handle
 * is the image's load address as the in-image header states it: one
 * number the host, the shim's address answer and a debugger all read off
 * the image itself.
 */
#include <stddef.h>
#include <stdint.h>

#include "rhu_shim.h"

#include "rhu.h"
#include "rhu_rt_entry_net11_rc1.h"

/* The stream-shaped surface the C runtime offers. Declared here rather
 * than included: the standard header spells the stream names as macros,
 * and a macro would take the identifier before the linker ever saw it. */
void rhu_stdio_bind_stderr(void);
int rhu_stdio_line_buffer_stdout(void);

/* The runtime entry points this contract calls, the module-list
 * initializer, the managed entry and the class-library exports are
 * declared by the generated header above, as the runtime's own
 * definitions and its bootstrapper spell them at the pin: the
 * registration takes each range as a start address and a length in
 * bytes. They are the runtime's and the program's, never the shim's: in
 * a real image they come from the merged runtime object, and under the
 * gate a stand-in exports them so the ORDER can be proven without the
 * runtime present. */

/* The class-library function table, by the order the runtime indexes it
 * with and by the exported names its bootstrapper installs. The runtime
 * reads the table through its own enum, so the order is the generated
 * header's, read off the runtime's own source at the pin and never
 * typed: a table laid out for another pin's shape links cleanly -- the
 * linker checks names, never positions -- and routes exception handling
 * one slot off, to the wrong entry or to null. The shape this host fills
 * is therefore asserted below against the header, so a pin that inserts
 * or removes a slot is refused at this compile rather than met at the
 * first exception. The exported name behind each filled slot is the
 * header's too, read off the bootstrapper's own table, so the host never
 * spells a name the runtime's build did not. The four slots of a build
 * feature this platform's runtime lacks stay null, as the bootstrapper
 * leaves them. */
_Static_assert(RHU_RT_CLASSLIB_SLOT_GetRuntimeException == 0, "the runtime-exception slot ties the pin");
_Static_assert(RHU_RT_CLASSLIB_SLOT_FailFast == 1, "the fail-fast slot ties the pin");
_Static_assert(RHU_RT_CLASSLIB_SLOT_ThreadEntryPoint == 2, "the thread-entry slot ties the pin");
_Static_assert(RHU_RT_CLASSLIB_SLOT_AppendExceptionStackFrame == 3, "the stack-frame slot ties the pin");
_Static_assert(RHU_RT_CLASSLIB_SLOT_ResolveDispatch == 4, "the dispatch slot ties the pin");
_Static_assert(RHU_RT_CLASSLIB_SLOT_GetSystemArrayEEType == 5, "the array-type slot ties the pin");
_Static_assert(RHU_RT_CLASSLIB_SLOT_OnFirstChanceException == 6, "the first-chance slot ties the pin");
_Static_assert(RHU_RT_CLASSLIB_SLOT_OnUnhandledException == 7, "the unhandled-exception slot ties the pin");
_Static_assert(RHU_RT_CLASSLIB_SLOTS == 12, "the slot count ties the pin");
_Static_assert(RHU_RT_CLASSLIB_SLOTS > RHU_RT_CLASSLIB_SLOT_ResolveDispatch, "the table holds the dispatch slot");
_Static_assert(RHU_RT_CLASSLIB_EXPORTS == 8, "eight slots carry an export at this pin");

/* The in-image header the kernel states at the image base: sixteen
 * words -- the magic, the layout version, the header's size, the load
 * address, and the rest. The host reads the load address after checking
 * the first two, and passes it to the runtime as the module handle. */
extern const uint32_t __rhu_image_header[16];
#define RHU_HOST_IMAGE_MAGIC 0x49554852u
#define RHU_HOST_IMAGE_LAYOUT 1u

/* The load and run brackets of the initialized data region. The boot
 * copies [__sdata, __edata) from __sidata; the host's job is to prove it
 * arrived, not to do it again. */
extern char __sdata[];
extern char __edata[];
extern char __sidata[];

/* The constructor and destructor arrays. The linker brackets each;
 * whether an array lies inside the region the boot copies is checked
 * below, because an array outside it holds whatever the memory held at
 * reset. */
extern void (*__preinit_array_start[])(void);
extern void (*__preinit_array_end[])(void);
extern void (*__init_array_start[])(void);
extern void (*__init_array_end[])(void);
extern void (*__fini_array_start[])(void);
extern void (*__fini_array_end[])(void);

/* The managed-code and unboxing-stub ranges as the map brackets them
 * (empty pairs in an image that carries neither), and the module table
 * the linker brackets. */
extern char __rhu_managedcode_lo[];
extern char __rhu_managedcode_hi[];
extern char __rhu_unbox_lo[];
extern char __rhu_unbox_hi[];
extern void *__start___modules[];
extern void *__stop___modules[];

/* The argument vector. There is no command line on this platform: the
 * program sees its own name and nothing after it (the managed startup
 * keeps the first entry as the program's name and hands the rest to the
 * program, so a vector of one is a program with no arguments). */
static char g_arg0[] = "rhu";
static char *g_argv[] = { g_arg0, NULL };

/* A datum with a known initial value, in the initialized data region,
 * written by nothing. Step 3 compares it against the image it was loaded
 * from: equal means the region arrived; unequal means the memory path
 * between the load image and this address does not work, which is worth
 * knowing here rather than three stages later. */
static volatile uint32_t g_data_canary = 0x5AA55AA5u;

/* An address the compiler may not fold. Two distinct extern objects are
 * exactly the shape whose difference the distinct-objects rule allows it
 * to collapse. */
static uintptr_t opaque_addr(const void *p)
{
    uintptr_t v = (uintptr_t)p;
    __asm__("" : "+r"(v));
    return v;
}

/* One line, one console write: the host shares the console with hosted
 * threads that print whole lines, and a line written in pieces can carry
 * another thread's line inside it. Every host line is built here and
 * written once; a line longer than the buffer is cut, never split. */
#define HOST_LINE_BYTES 200u

static uint32_t line_put_str(char *buf, uint32_t n, const char *s)
{
    while (*s != '\0' && n < HOST_LINE_BYTES - 2u)
        buf[n++] = *s++;
    return n;
}

static uint32_t line_put_u(char *buf, uint32_t n, uint32_t v)
{
    char digits[10];
    int k = 0;

    if (v == 0u) {
        digits[k++] = '0';
    } else {
        while (v != 0u && k < (int)sizeof digits) {
            digits[k++] = (char)('0' + (v % 10u));
            v /= 10u;
        }
    }
    while (k-- > 0 && n < HOST_LINE_BYTES - 2u)
        buf[n++] = digits[k];
    return n;
}

static void line_end(char *buf, uint32_t n)
{
    buf[n++] = '\r';
    buf[n++] = '\n';
    rhu_console_write(buf, n);
}

static void host_log(const char *msg)
{
    char buf[HOST_LINE_BYTES];
    uint32_t n = line_put_str(buf, 0u, "[host] ");
    n = line_put_str(buf, n, msg);
    line_end(buf, n);
}

/* One marker carrying one number. The host prints its own lines rather
 * than borrowing another surface's prefix: a transcript that mixes two
 * voices is harder to read as a sequence, and the sequence is the point. */
static void host_logu(const char *msg, uint32_t v)
{
    char buf[HOST_LINE_BYTES];
    uint32_t n = line_put_str(buf, 0u, "[host] ");
    n = line_put_str(buf, n, msg);
    n = line_put_u(buf, n, v);
    line_end(buf, n);
}

/* One marker carrying two numbers. */
static void host_logu2(const char *a, uint32_t x, const char *b, uint32_t y, const char *c)
{
    char buf[HOST_LINE_BYTES];
    uint32_t n = line_put_str(buf, 0u, "[host] ");
    n = line_put_str(buf, n, a);
    n = line_put_u(buf, n, x);
    n = line_put_str(buf, n, b);
    n = line_put_u(buf, n, y);
    n = line_put_str(buf, n, c);
    line_end(buf, n);
}

static void host_die(const char *msg)
{
    char buf[HOST_LINE_BYTES];
    uint32_t n = line_put_str(buf, 0u, "[host] FAIL ");
    n = line_put_str(buf, n, msg);
    line_end(buf, n);
    rhu_exit(70);
}

/* Step 3: the initialized data region against the image it came from.
 * Only the canary is compared, not the whole region: everything else in
 * it has been running code's to write since the boot copy, and a
 * whole-region compare would report those writes as damage. */
static void verify_loaded_data(void)
{
    uintptr_t run = opaque_addr(__sdata);
    uintptr_t load = opaque_addr(__sidata);
    uintptr_t at = opaque_addr((const void *)&g_data_canary);
    const uint32_t *loaded;
    uint32_t seen;

    if (at < run || at >= opaque_addr(__edata))
        host_die("the data check reads outside the initialized data region");
    loaded = (const uint32_t *)(load + (at - run));
#ifdef RHU_SHIM_BREAK_DATAVERIFY
    /* Gate control: one byte of the run image damaged, exactly as a dead
     * memory path would leave it. */
    g_data_canary ^= 0x00000100u;
#endif
    seen = g_data_canary;
    if (seen != *loaded)
        host_die("initialized data does not match the image it was loaded from");
    host_log("data: initialized region verified against its load image");
}

/* One constructor or destructor array as the linker brackets it: its
 * two ends and its entry count, checked before anything is walked. */
typedef struct ctor_array {
    void (**lo)(void);
    uint32_t n;
} ctor_array;

/* The array's brackets must be ordered and the array must lie inside the
 * region the boot copies, or its entries are whatever the memory held at
 * reset rather than the addresses the image was built with. On a board
 * with the load image in flash that is the difference between running
 * the constructors and jumping into noise, and an emulator that writes
 * the array straight into memory would hide it -- so the placement is
 * checked, not assumed. An empty array is placed nowhere and passes. */
static ctor_array ctor_checked(void (**lo)(void), void (**hi)(void), const char *inverted, const char *outside)
{
    ctor_array a;
    uintptr_t b = opaque_addr((const void *)lo);
    uintptr_t e = opaque_addr((const void *)hi);
    uintptr_t sdata = opaque_addr(__sdata);
    uintptr_t edata = opaque_addr(__edata);

    if (e < b)
        host_die(inverted);
    a.lo = lo;
    a.n = (uint32_t)((e - b) / sizeof(void (*)(void)));
    if (a.n != 0u && (b < sdata || e > edata))
        host_die(outside);
    return a;
}

static void ctor_walk(const ctor_array *a)
{
    uint32_t i;

    for (i = 0; i < a->n; i++)
        a->lo[i]();
}

/* Step 4: the static constructors. All three arrays are checked first,
 * then the early array runs, then the main one; the destructor array is
 * placed and checked like the other two and never walked, because the
 * machine exit runs no destructors. */
static void run_static_ctors(void)
{
    ctor_array pre = ctor_checked(__preinit_array_start, __preinit_array_end,
                                  "the early constructor array's brackets are inverted",
                                  "the early constructor array lies outside the region the boot copies");
    ctor_array init = ctor_checked(__init_array_start, __init_array_end,
                                   "the constructor array's brackets are inverted",
                                   "the constructor array lies outside the region the boot copies");
    ctor_array fini = ctor_checked(__fini_array_start, __fini_array_end,
                                   "the destructor array's brackets are inverted",
                                   "the destructor array lies outside the region the boot copies");

    (void)fini;
    if (pre.n == 0u && init.n == 0u) {
        host_log("ctors: none (this image carries no static constructor)");
        return;
    }
    ctor_walk(&pre);
    ctor_walk(&init);
    host_logu2("ctors run: ", pre.n, " early, ", init.n, " main");
}

/* Step 6's module handle: the image's load address as its own header
 * states it, once the header reads as the layout this host knows. */
static void *image_base(void)
{
    if (__rhu_image_header[0] != RHU_HOST_IMAGE_MAGIC || __rhu_image_header[1] != RHU_HOST_IMAGE_LAYOUT)
        host_die("the in-image header does not read as layout version 1");
    return (void *)(uintptr_t)__rhu_image_header[3];
}

/* A bracketed range's length in bytes, its ends checked for order. */
static uint32_t range_bytes(const char *lo, const char *hi, const char *inverted)
{
    uintptr_t a = opaque_addr(lo);
    uintptr_t b = opaque_addr(hi);

    if (b < a)
        host_die(inverted);
    return (uint32_t)(b - a);
}

int32_t rhu_host_main(int argc, char **argv);

int32_t rhu_host_main(int argc, char **argv)
{
    void *classlib[RHU_RT_CLASSLIB_SLOTS];
    void *module;
    uint32_t managed_bytes;
    uint32_t unbox_bytes;
    uint32_t modules;
    int i;
    int rc;

    /* 1, 2: the streams, before anything that might want to report. */
    rhu_stdio_bind_stderr();
    host_log("stdio: standard error bound to the C library's stream");
    if (rhu_stdio_line_buffer_stdout() != 0)
        host_die("standard output could not be line buffered");
    host_log("stdio: standard output line buffered");

    /* 3, 4: the image, then the constructors -- and in that order,
     * because a constructor running on data that did not arrive would
     * fail somewhere with no bearing on the cause. */
    verify_loaded_data();
    run_static_ctors();

    /* 5: the runtime. The constructors precede it: the archives build
     * their own state in constructors that the runtime then reads. */
    if (!RhInitialize(false))
        host_die("the runtime refused to initialize");
    host_log("runtime: initialized");

    /* 6: the registration -- the handle, the two ranges, the table. */
    module = image_base();
    for (i = 0; i < RHU_RT_CLASSLIB_SLOTS; i++)
        classlib[i] = NULL;
    classlib[RHU_RT_CLASSLIB_SLOT_GetRuntimeException] = (void *)(uintptr_t)&RHU_RT_CLASSLIB_EXPORT_GetRuntimeException;
    classlib[RHU_RT_CLASSLIB_SLOT_FailFast] = (void *)(uintptr_t)&RHU_RT_CLASSLIB_EXPORT_FailFast;
    classlib[RHU_RT_CLASSLIB_SLOT_ThreadEntryPoint] = (void *)(uintptr_t)&RHU_RT_CLASSLIB_EXPORT_ThreadEntryPoint;
    classlib[RHU_RT_CLASSLIB_SLOT_AppendExceptionStackFrame] = (void *)(uintptr_t)&RHU_RT_CLASSLIB_EXPORT_AppendExceptionStackFrame;
    classlib[RHU_RT_CLASSLIB_SLOT_ResolveDispatch] = (void *)(uintptr_t)&RHU_RT_CLASSLIB_EXPORT_ResolveDispatch;
    classlib[RHU_RT_CLASSLIB_SLOT_GetSystemArrayEEType] = (void *)(uintptr_t)&RHU_RT_CLASSLIB_EXPORT_GetSystemArrayEEType;
    classlib[RHU_RT_CLASSLIB_SLOT_OnFirstChanceException] = (void *)(uintptr_t)&RHU_RT_CLASSLIB_EXPORT_OnFirstChanceException;
    classlib[RHU_RT_CLASSLIB_SLOT_OnUnhandledException] = (void *)(uintptr_t)&RHU_RT_CLASSLIB_EXPORT_OnUnhandledException;
    managed_bytes = range_bytes(__rhu_managedcode_lo, __rhu_managedcode_hi, "the managed-code bracket pair is inverted");
    unbox_bytes = range_bytes(__rhu_unbox_lo, __rhu_unbox_hi, "the unboxing-stub bracket pair is inverted");
    host_logu2("runtime: managed code ", managed_bytes, " bytes, unboxing stubs ", unbox_bytes, " bytes");
    if (!RhRegisterOSModule(module, __rhu_managedcode_lo, managed_bytes, __rhu_unbox_lo, unbox_bytes,
                            classlib, RHU_RT_CLASSLIB_SLOTS))
        host_die("the runtime refused this image's module registration");
    host_logu("runtime: module registered, class-library slots: ", (uint32_t)RHU_RT_CLASSLIB_SLOTS);

    /* 7: the module list, under the same handle, after the registration. */
    modules = range_bytes((const char *)__start___modules, (const char *)__stop___modules,
                          "the module table's bracket pair is inverted") / (uint32_t)sizeof(void *);
    InitializeModules(module, __start___modules, (int)modules, classlib, RHU_RT_CLASSLIB_SLOTS);
    host_logu("runtime: modules initialized: ", modules);

    /* 8: the arguments. */
    host_logu("argv: ", (uint32_t)(argc < 0 ? 0 : argc));

    /* 9: managed entry, by the name the runtime's bootstrapper declares. */
    host_log("entering managed entry");
    rc = __managed__Main(argc, argv);

    /* 10: the audit BEFORE the exit line, so a run that ends normally
     * still reports what it left behind. */
    rhu_arena_report();
    host_logu("arena headroom at exit: ", rhu_arena_headroom());
    host_logu("activations delivered: ", rhu_shim_activation_count());

    /* 11. */
    host_logu("exit: ", (uint32_t)(rc < 0 ? 0 : rc));
    return rc;
}

/* The hosting-API contract generation this host was built for, and the
 * entry the kernel hands its hosting thread to. Both are strong
 * definitions: they displace the kernel's weak defaults, and the kernel's
 * handshake refuses a generation that is not its own before the entry
 * runs. The shim is initialized first, before any constructor or runtime
 * code can need its per-thread block or its guard word. */
const uint32_t rhu_payload_abi = RHU_ABI;

int32_t rhu_payload_entry(void)
{
    rhu_shim_init();
    return rhu_host_main(1, g_argv);
}
