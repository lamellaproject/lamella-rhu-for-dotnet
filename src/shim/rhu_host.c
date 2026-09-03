/* The host: the initialization contract that runs before managed code and
 * the accounting that runs after it. The ORDER is the contract -- each
 * step depends on the one before it, and each prints one marker naming
 * what it did, so a transcript is a proof of sequence and not just of
 * outcome. Nothing here is optional and nothing here is silent: a step
 * that cannot do its work says so and ends the run rather than leaving a
 * later stage to fail somewhere less legible.
 *
 * The steps, in order:
 *
 *   1  the standard error stream is bound to the C library's own object
 *   2  standard output is line buffered, so concurrent writers cannot
 *      fuse two lines into one
 *   3  the initialized data region is verified against the image it was
 *      loaded from -- a dead memory path dies here, not later inside the
 *      collector
 *   4  the static constructors run, and they must run before the runtime
 *      is initialized
 *   5  the runtime is initialized
 *   6  the image registers itself with the runtime, handing over the
 *      class-library function table
 *   7  the module list is initialized -- the one step this file does not
 *      perform, explained below
 *   8  the argument vector is prepared
 *   9  managed entry
 *   10 the arena is audited BEFORE the exit line is printed, so a leak is
 *      reported by a run that ends normally
 *   11 the return code becomes the machine's exit code
 *
 * Step 7 is NOT here. Initializing the module list belongs after the
 * module registration, because the runtime reads what the registration
 * gave it; but on this platform the payload entry initializes the module
 * list itself before it calls into this file, so a second initialization
 * from here would run in the wrong place. The host leaves that step to
 * the payload entry.
 */
#include <stddef.h>
#include <stdint.h>

#include "rhu_shim.h"

#include "rhu.h"
#include "rhu_rt_entry_p7.h"

/* The C-runtime surface the host uses, declared as the caller sees it. */
size_t strlen(const char *s);

/* The stream-shaped surface the C runtime offers. Declared here rather
 * than included: the standard header spells the stream names as macros,
 * and a macro would take the identifier before the linker ever saw it. */
void rhu_stdio_bind_stderr(void);
int rhu_stdio_line_buffer_stdout(void);

/* The runtime entry points this contract calls are declared by the
 * generated header above, as the runtime's own definitions spell them at
 * the pin: the registration takes each range as a start address and a
 * length in bytes. They are the runtime's, never the shim's: in a real
 * image they come from the runtime archives, and under the gate a stand-in
 * exports them so the ORDER can be proven without the runtime present. */

/* The managed program's entry. The kernel's payload flavor calls
 * `__managed__Main` itself, so the host cannot own that name; the
 * program the host hands control to is named here and comes from the
 * image this file is linked into. */
int32_t __rhu_managed_main(int32_t argc, const uint8_t *const *argv);

/* The class-library function table, by the order the runtime indexes it
 * with. The order is READ FROM the runtime's own source at the pin, never
 * typed: a wrong order links cleanly and wires exception handling to the
 * wrong functions.
 * Slot 4 is the dispatch resolver -- a null there also links cleanly and
 * dies at the first interface dispatch -- so the table is filled with the
 * entries this image can supply and the rest are left null. The slot
 * count is the generated header's, read off the runtime's own enum. */
extern void *__rhu_classlib_slot0(void);
extern void *__rhu_classlib_slot4(void);

/* The load and run brackets of the initialized data region. The boot
 * copies [__sdata, __edata) from __sidata; the host's job is to prove it
 * arrived, not to do it again. */
extern char __sdata[];
extern char __edata[];
extern char __sidata[];

/* Static constructors. The linker brackets the array; whether the array
 * lies inside the region the boot copies is checked below, because an
 * array outside it holds whatever the memory held at reset. */
extern void (*__init_array_start[])(void);
extern void (*__init_array_end[])(void);

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

static void host_log(const char *msg)
{
    rhu_console_write("[host] ", 7u);
    rhu_console_write(msg, (uint32_t)strlen(msg));
    rhu_console_write("\r\n", 2u);
}

/* One marker carrying one number. The host prints its own lines rather
 * than borrowing another surface's prefix: a transcript that mixes two
 * voices is harder to read as a sequence, and the sequence is the point. */
static void host_logu(const char *msg, uint32_t v)
{
    char digits[10];
    int n = 0;

    rhu_console_write("[host] ", 7u);
    rhu_console_write(msg, (uint32_t)strlen(msg));
    if (v == 0u) {
        digits[n++] = '0';
    } else {
        while (v != 0u && n < (int)sizeof digits) {
            digits[n++] = (char)('0' + (v % 10u));
            v /= 10u;
        }
    }
    while (n-- > 0)
        rhu_console_write(&digits[n], 1u);
    rhu_console_write("\r\n", 2u);
}

static void host_die(const char *msg)
{
    rhu_console_write("[host] FAIL ", 12u);
    rhu_console_write(msg, (uint32_t)strlen(msg));
    rhu_console_write("\r\n", 2u);
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

/* Step 4: the static constructors.
 *
 * The array the linker brackets must lie inside the region the boot
 * copies, or its entries are whatever the memory held at reset rather
 * than the addresses the image was built with. On a board with the load
 * image in flash that is the difference between running the constructors
 * and jumping into noise, and an emulator that writes the array straight
 * into memory would hide it -- so the placement is checked, not assumed. */
static void run_static_ctors(void)
{
    uintptr_t lo = opaque_addr((const void *)__init_array_start);
    uintptr_t hi = opaque_addr((const void *)__init_array_end);
    uintptr_t sdata = opaque_addr(__sdata);
    uintptr_t edata = opaque_addr(__edata);
    uint32_t n;

    if (hi < lo)
        host_die("the constructor array's brackets are inverted");
    n = (uint32_t)((hi - lo) / sizeof(void (*)(void)));
    if (n == 0u) {
        host_log("ctors: none (this image carries no static constructor)");
        return;
    }
    if (lo < sdata || hi > edata)
        host_die("the constructor array lies outside the region the boot copies");
    {
        uint32_t i;
        for (i = 0; i < n; i++)
            __init_array_start[i]();
    }
    host_logu("ctors run: ", n);
}

int32_t rhu_host_main(int32_t argc, const uint8_t *const *argv)
{
    void *classlib[RHU_RT_CLASSLIB_SLOTS];
    int32_t i;
    int32_t rc;

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

    /* 5, 6, 7: the runtime. The constructors precede it: the archives
     * build their own state in constructors that the runtime then reads. */
    if (!RhInitialize(false))
        host_die("the runtime refused to initialize");
    host_log("runtime: initialized");

    for (i = 0; i < RHU_RT_CLASSLIB_SLOTS; i++)
        classlib[i] = NULL;
    classlib[0] = (void *)(uintptr_t)&__rhu_classlib_slot0;
    classlib[4] = (void *)(uintptr_t)&__rhu_classlib_slot4;
    if (!RhRegisterOSModule((void *)opaque_addr(__sdata), NULL, 0u, NULL, 0u,
                            classlib, RHU_RT_CLASSLIB_SLOTS))
        host_die("the runtime refused this image's module registration");
    host_logu("runtime: module registered, class-library slots: ", (uint32_t)RHU_RT_CLASSLIB_SLOTS);

    /* 8: the arguments. There is no command line on this platform; the
     * vector the program sees is the one it was built with. */
    host_logu("argv: ", (uint32_t)(argc < 0 ? 0 : argc));

    /* 9: managed entry. */
    host_log("entering managed entry");
    rc = __rhu_managed_main(argc, argv);

    /* 10: the audit BEFORE the exit line, so a run that ends normally
     * still reports what it left behind. */
    rhu_arena_report();
    host_logu("arena headroom at exit: ", rhu_arena_headroom());

    /* 11. */
    host_logu("exit: ", (uint32_t)(rc < 0 ? 0 : rc));
    return rc;
}
