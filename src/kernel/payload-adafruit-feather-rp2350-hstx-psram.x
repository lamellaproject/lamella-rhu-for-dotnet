/* Lamella RHU -- Adafruit Feather RP2350 with HSTX and PSRAM PAYLOAD link map: the kernel
 * staticlib (librhu_kernel.a, feature `payload`) merged with one frozen
 * ILC payload object under an external `ld -T`. Same machine map as
 * link-adafruit-feather-rp2350-hstx-psram.x -- keep the two in sync -- plus the managed
 * sections an ILC object carries: managed code and unboxing stubs
 * bracketed inside text, the __modules table bracketed for the
 * module-initializer walk, frozen/metadata blobs into rodata, the
 * rehydrated-data region zero-filled and bracketed at the head of the
 * external RAM region, the GOT
 * inside the boot copy window, and the ARM unwind tables KEPT (managed
 * code emits them; the base script's panic=abort discard does not apply
 * here).
 *
 * Geometry discipline: every output section lands inside a declared
 * MEMORY region, so overflow fails the LINK loudly; the payload gate
 * additionally asserts the contract symbols and the managed entry in
 * the ARTIFACT by nm (a section the linker cannot police would need
 * exactly that assert, so the gate carries it).
 */

MEMORY
{
  /* The QSPI flash through the chip's cached flash window (the RP2350
   * datasheet's address map; the board's published facts for the 8 MB
   * part). */
  CODE (rx)  : ORIGIN = 0x10000000, LENGTH = 6M
  /* Reserved for the split-image app slot: nothing from this image may
   * land here. 0x10600000 is a 4 KB-sector boundary. */
  APP  (rx)  : ORIGIN = 0x10600000, LENGTH = 2M
  /* SRAM0-7 (striped, 512 KB) + SRAM8 + SRAM9 (4 KB each): one
   * contiguous 520 KB window ending at 0x20082000, the whole of it this
   * image's DATA (data and stacks only; no code is ever placed in RAM). */
  DATA (rwx) : ORIGIN = 0x20000000, LENGTH = 520K
  /* The board's 8 MB PSRAM part behind the flash interface's second chip
   * select (GPIO 8), shown through the chip's cached window for that
   * select (the RP2350 datasheet's address map: the lower half of the
   * XIP space holds two 16 MB windows, one per chip select). The PSRAM
   * crate brings the part up and proves it at boot; the kernel then
   * offers the region past the rehydrated-data region to the hosted
   * runtime as its second RAM range (the arena's window 1, bracketed
   * below). The rehydrated-data region alone is placed here (the
   * section below). */
  PSRAM (rwx) : ORIGIN = 0x11000000, LENGTH = 8M
}

/* GC-substrate floor: the arena window this board GUARANTEES after
 * statics + .kstacks + the console mirror + the boot-stack reserve
 * (link-enforced by the window asserts at the bottom of this map).
 * Under the external posture (the managed heap, the collector's mark
 * bitmap and its mark stack in the PSRAM's window 1) this window serves
 * the hot small mappings alone -- the 16K kernel-heap carve, the 4K
 * handle page and the 8K early mappings, the 16K frozen-object pool and
 * the row's 8K unallocated: 52K -- so the floor is 64K, and the SRAM
 * beside it holds a hosted program's statics. */
RHU_ARENA_MIN = 64K;

ENTRY(Reset);

/* Boot stack: top of DATA, full-descending. */
__stack_start = ORIGIN(DATA) + LENGTH(DATA);

SECTIONS
{
  .vector_table ORIGIN(CODE) :
  {
    __vector_table = .;
    LONG(__stack_start);               /* [0]       initial MSP          */
    KEEP(*(.vector_table.reset));      /* [1]       Reset                */
    KEEP(*(.vector_table.exceptions)); /* [2..15]   system exceptions    */
    KEEP(*(.vector_table.interrupts)); /* [16..67]  external IRQ lines   */
    /* The table's slot is 0x400 bytes on every board: padded to the
     * in-image header's fixed offset (zero fill), so the header follows
     * inside the same load segment and the flash image stays one
     * contiguous run. A table that outgrows the slot moves the header
     * and refuses the link at the assert below. */
    . = ALIGN(0x400);
  } > CODE

  /* The in-image header (__rhu_image_header): the image's own statement
   * of its identity and load address, at a fixed offset from the image
   * base on every board so a reader holding the flash bytes alone finds
   * it (header.rs states the layout). KEEP: nothing in the image
   * references the record. The test harness reads it off the artifact. */
  .rhu_header ORIGIN(CODE) + 0x400 :
  {
    KEEP(*(.rhu_header));
  } > CODE

  /* The boot block the chip's boot ROM scans the first 4 KB of a flash
   * image for: five words right after the header, inside the same load
   * segment (the chip module states the words). KEEP: nothing in the
   * image references it. Asserted at its offset and size below. */
  .rp2350_boot_block ORIGIN(CODE) + 0x440 :
  {
    KEEP(*(.rp2350_boot_block));
  } > CODE

  .text : ALIGN(4)
  {
    __stext = .;
    /* The hosting-API surface (rhu_now_ns .. rhu_futex_wake): the shim
     * backend links against these, so section GC must never fold a
     * fully-inlined one away. The test harness asserts all four. */
    KEEP(*(.text.rhu_api));
    *(.text .text.*);
    /* The hosted program's managed code and its unboxing stubs, each
     * bracketed: the runtime's module registration takes both ranges by
     * address (the host reads the brackets), so the two are KEPT and the
     * pairs exact; both stay inside [__stext, __etext), module 0's code
     * bracket. Empty pairs until a hosted object carries the sections. */
    . = ALIGN(4);
    __rhu_managedcode_lo = .;
    KEEP(*(__managedcode*));
    __rhu_managedcode_hi = .;
    . = ALIGN(4);
    __rhu_unbox_lo = .;
    KEEP(*(__unbox*));
    __rhu_unbox_hi = .;
    . = ALIGN(4);
    __etext = .;
  } > CODE

  /* Module table: the managed entry sequence walks
   * [__start___modules, __stop___modules) to initialize each linked
   * managed module. KEEP -- nothing references the table by section. */
  __modules : ALIGN(4)
  {
    __start___modules = .;
    KEEP(*(__modules*));
    __stop___modules = .;
  } > CODE

  .rodata : ALIGN(4)
  {
    *(.rodata .rodata.*);
    *(.dotnet*);
    /* Debug-visible layout contract record (__rhu_debug_layout):
     * nothing in the image references it, so only KEEP holds it
     * through section GC. The test harness asserts it in the artifact. */
    KEEP(*(.rhu_debug));
    . = ALIGN(4);
  } > CODE

  /* Frozen segments as first-class link facts: the GC's
   * root-source range -- rhu_module_roots serves it from these
   * brackets. Empty until a payload carries frozen blobs; the
   * brackets are exact either way. */
  .frozen : ALIGN(4)
  {
    __rhu_frozen_lo = .;
    *(.frozen*);
    __rhu_frozen_hi = .;
  } > CODE

  /* ARM unwind tables, kept: managed code emits an exidx/extab pair
   * per code section. PREL31 entries reach +/-1 GB, so they stay in
   * CODE beside the text they describe. */
  .ARM.extab : { *(.ARM.extab*) } > CODE
  .ARM.exidx : ALIGN(4)
  {
    __exidx_start = .;
    *(.ARM.exidx*);
    __exidx_end = .;
  } > CODE

  /* The arena placement: the payload's GC arena and
   * its start bits (payload.rs: ARENA, STARTBITS) at the HEAD of DATA in
   * their own NOLOAD section, before .data -- no static the kernel adds
   * and no .data growth moves the arena, so the collector's count is a
   * layout figure of an address fixed at ORIGIN(DATA). Outside
   * [__sdata, __ebss): the collector carves its own heap out of the
   * statics window it scans, so no root changes; boot.rs zeroes this
   * bracket beside .bss under the payload feature. The arena's input
   * first, then the bits (two named inputs fix the order). No KEEP: an
   * image whose runtime never asks for the kernel arena (the frozen
   * runtime carves its own heap) carries the section EMPTY, section GC
   * dropping the statics, its window unmoved. */
  .rhu_payload_arena ORIGIN(DATA) (NOLOAD) : ALIGN(8)
  {
    __rhu_payload_arena_lo = .;
    *(.bss.rhu_payload_arena);
    *(.bss.rhu_payload_startbits);
    . = ALIGN(8);
    __rhu_payload_arena_hi = .;
  } > DATA

  .data : ALIGN(8)
  {
    __sdata = .;
    *(.data .data.*);
    . = ALIGN(8);
  } > DATA AT> CODE
  __sidata = LOADADDR(.data);

  /* The static-link GOT (GOT-relative references a hosted object
   * carries resolve to link-time constants): inside the boot copy window
   * at .data's load/run offset (asserted at the bottom of this map), so
   * the linker's read-only-after-relocation run -- the GOT, the three
   * arrays and the TLS template -- is one run of neighboring sections,
   * the shape it requires. Empty until a hosted object carries such
   * references; the 8-aligned tail keeps the arrays' offset. */
  .got : ALIGN(4)
  {
    *(.got) *(.got.plt);
    . = ALIGN(8);
  } > DATA AT> CODE

  /* The constructor and destructor arrays, inside the boot copy window:
   * a hosted image's static constructors register here (.preinit_array,
   * then .init_array in priority order; .fini_array holds the
   * destructors), and the payload entry walks the brackets before the
   * module list. Each array loads from CODE at .data's load/run offset
   * (asserted at the bottom of this map) so the boot's single copy lands
   * it: an array left outside the window holds whatever the RAM held at
   * reset -- a defect an emulator that writes sections straight into
   * memory hides and flash-resident silicon exposes. KEEP: the arrays
   * are consumed by address through the brackets, never by symbol
   * reference. Each tail aligns to 8 so the next section keeps the
   * offset. The brackets are exact: an image with no constructors reads
   * three empty pairs. Nothing in the kernel runs the destructor array
   * (the machine exit runs no destructors); a host that owns the entry
   * owns that walk. */
  .preinit_array : ALIGN(4)
  {
    __preinit_array_start = .;
    KEEP(*(.preinit_array .preinit_array.*));
    __preinit_array_end = .;
    . = ALIGN(8);
  } > DATA AT> CODE

  .init_array : ALIGN(4)
  {
    __init_array_start = .;
    KEEP(*(SORT_BY_INIT_PRIORITY(.init_array.*) .init_array));
    __init_array_end = .;
    . = ALIGN(8);
  } > DATA AT> CODE

  .fini_array : ALIGN(4)
  {
    __fini_array_start = .;
    KEEP(*(SORT_BY_INIT_PRIORITY(.fini_array.*) .fini_array));
    __fini_array_end = .;
    . = ALIGN(8);
  } > DATA AT> CODE

  /* Module 0's TLS template, in its own output section: the assembler
   * marks .tdata/.tbss input thread-local and the linker refuses that
   * mark inside a plain data section, so the template cannot share
   * .data. The section is PROGBITS by construction -- the kernel
   * contributes a byte-less thread-local PROGBITS input, so zero-fill
   * input emits as file zeros and the whole span is one initialized
   * master copy -- and sits inside the boot copy window ([__sdata,
   * __edata) ends here) at the same load/run offset as .data
   * (asserted at the bottom of this map), so the existing copy lands
   * it. KEEP: the template is consumed by address through the
   * brackets, never by symbol reference, so section GC would
   * otherwise drop it. The 8-align matches the kernel's per-thread
   * block alignment and is the segment alignment thread-local offsets
   * are computed against. Empty until a hosted payload carries
   * thread-local state -- an empty pair is an honest value. */
  .tdata : ALIGN(8)
  {
    __rhu_tdata_lo = .;
    KEEP(*(.tdata .tdata.*));
    KEEP(*(.tbss .tbss.*));
    __rhu_tdata_hi = .;
    __edata = .;
  } > DATA AT> CODE

  /* The hosted runtime's rehydrated-data region: zero-filled RAM the
   * runtime fills at startup from its compact form, bracketed and
   * 8-aligned as the runtime lays it out. Its own NOLOAD section at the
   * head of the external RAM region, ahead of .bss in this script so
   * the inputs land here and nowhere else. Outside the zero-fill
   * window: the kernel zeroes the bracket at boot, after the external
   * RAM is brought up and proved (boot.rs). The arena's second window
   * starts at the section's end (bracketed below). Reference-retained
   * (the compact form points into it); an empty pair at the region's
   * origin until a hosted object carries the region. */
  .rhu_hydrated ORIGIN(PSRAM) (NOLOAD) : ALIGN(8)
  {
    __rhu_hydrated_lo = .;
    *(.hydrated*);
    . = ALIGN(8);
    __rhu_hydrated_hi = .;
  } > PSRAM

  .bss (NOLOAD) : ALIGN(4)
  {
    __sbss = .;
    *(.bss .bss.* COMMON);
    . = ALIGN(4);
    __ebss = .;
  } > DATA

  /* Thread stacks: OUTSIDE the __sdata..__ebss window, so a
   * conservative statics scan never walks stale thread-stack contents.
   * NOLOAD; the scheduler zeroes each carved stack at spawn. */
  .kstacks (NOLOAD) : ALIGN(8)
  {
    *(.kstacks);
    . = ALIGN(8);
    __ekstacks = .;
  } > DATA

  /* The console mirror (__rhu_console_log): the transcript a debug
   * probe reads out of RAM. NOLOAD, after the thread stacks and outside
   * the statics window like them; the chip module writes its header at
   * boot. KEEP: the probe reads it by symbol, nothing in the image
   * references it by name. */
  .rhu_console_log (NOLOAD) : ALIGN(8)
  {
    KEEP(*(.rhu_console_log));
    . = ALIGN(8);
    __rhu_console_log_end = .;
  } > DATA

  /DISCARD/ : { *(.comment) *(.note*) }
}

/* The arena window: link-defined slack between the end of the console
 * mirror and the boot/handler-stack reserve at the top of DATA.
 * Kernel-owned NOLOAD space outside every section; arena::init()
 * establishes the zero-birth invariant over it at boot. (Top level: the
 * linker rejects a bare ASSERT between output sections and reports the
 * error one command late.) */
RHU_BOOT_STACK_RESERVE = 16K;
__rhu_arena_lo = ALIGN(__rhu_console_log_end, 8);
__rhu_arena_hi = __stack_start - RHU_BOOT_STACK_RESERVE;
ASSERT(__rhu_arena_hi >= __rhu_arena_lo,
  "static RAM + .kstacks + the console mirror overflow into the boot-stack reserve (arena window negative)");
ASSERT(__rhu_arena_hi - __rhu_arena_lo >= RHU_ARENA_MIN,
  "arena window under this board's GC-substrate floor (RHU_ARENA_MIN)");

/* The arena's second window: the external RAM region from the end of
 * the rehydrated-data region (8-aligned) to the region's end.
 * Kernel-owned space outside every section, like the first window; the
 * kernel takes it only when the external RAM has been brought up and
 * proved at boot, and arena::init() establishes the zero-birth
 * invariant over it then. A rehydrated-data region that fills the
 * whole external region refuses the link. */
__rhu_arena1_lo = ALIGN(__rhu_hydrated_hi, 8);
__rhu_arena1_hi = ORIGIN(PSRAM) + LENGTH(PSRAM);
ASSERT(__rhu_arena1_hi > __rhu_arena1_lo,
  "rehydrated-data region fills the external region (the arena's second window empty)");

/* The rehydrated-data region lies inside the external RAM region. */
ASSERT(__rhu_hydrated_lo >= ORIGIN(PSRAM) && __rhu_hydrated_hi <= ORIGIN(PSRAM) + LENGTH(PSRAM),
  "rehydrated-data region outside the external region");

/* The module-0 TLS template rides the boot's single .data copy: its
 * load/run offset must equal .data's, on every board. */
ASSERT(LOADADDR(.tdata) - ADDR(.tdata) == LOADADDR(.data) - ADDR(.data),
  "module-0 TLS template outside the boot copy's single load/run offset");

/* The constructor and destructor arrays ride the same copy: each at
 * .data's load/run offset, on every board. */
ASSERT(LOADADDR(.preinit_array) - ADDR(.preinit_array) == LOADADDR(.data) - ADDR(.data),
  ".preinit_array outside the boot copy's single load/run offset");
ASSERT(LOADADDR(.init_array) - ADDR(.init_array) == LOADADDR(.data) - ADDR(.data),
  ".init_array outside the boot copy's single load/run offset");
ASSERT(LOADADDR(.fini_array) - ADDR(.fini_array) == LOADADDR(.data) - ADDR(.data),
  ".fini_array outside the boot copy's single load/run offset");

/* The in-image header sits at the fixed offset every reader uses, the
 * vector table's padded slot ends exactly there, and the record is
 * layout version 1's sixteen words -- on every board. */
ASSERT(ADDR(.rhu_header) == ORIGIN(CODE) + 0x400,
  "image header not at the fixed offset 0x400 from the image base");
ASSERT(SIZEOF(.vector_table) == 0x400,
  "vector table slot is not the 0x400 bytes before the image header");
ASSERT(SIZEOF(.rhu_header) == 64,
  "image header is not the 64 bytes of layout version 1");

/* The boot block sits right after the header, five words, inside the
 * 4 KB the boot ROM scans. */
ASSERT(ADDR(.rp2350_boot_block) == ORIGIN(CODE) + 0x440,
  "boot block not at the fixed offset 0x440 from the image base");
ASSERT(SIZEOF(.rp2350_boot_block) == 20,
  "boot block is not the five words the boot ROM recognises");

/* The hosted runtime's demand as the frozen tier states it (goldens/rt-m7:
 * the merged program object's section classes by llvm-size -- the code
 * class, the copied data, the zero-filled data). The regions must hold
 * that demand beside the kernel's own reserves on every board: a region
 * shrunk under it refuses the link whatever the image at hand carries.
 * The kernel reserves are declared allowances, not measurements. The
 * zero-filled class counts the rehydrated-data region, which this map
 * places in the external RAM region: the DATA assert keeps counting
 * it, a floor at least as strict as the SRAM's own demand. */
RHU_RT_CODE_BYTES = 933510;
RHU_RT_COPY_BYTES = 6365;
RHU_RT_ZERO_BYTES = 105936;
RHU_KERNEL_CODE_RESERVE = 128K;
RHU_KERNEL_RAM_RESERVE = 16K;
ASSERT(LENGTH(CODE) >= RHU_RT_CODE_BYTES + RHU_RT_COPY_BYTES + RHU_KERNEL_CODE_RESERVE,
  "CODE region under the hosted runtime's code class and copied data beside the kernel's code reserve");
ASSERT(LENGTH(DATA) >= RHU_RT_COPY_BYTES + RHU_RT_ZERO_BYTES + SIZEOF(.kstacks) + SIZEOF(.rhu_console_log) + RHU_BOOT_STACK_RESERVE + RHU_ARENA_MIN + RHU_KERNEL_RAM_RESERVE,
  "DATA region under the hosted runtime's data beside the thread stacks, the console mirror, the boot-stack reserve, the arena floor and the kernel's RAM reserve");

/* The GOT rides the boot's single copy: at .data's load/run offset. */
ASSERT(LOADADDR(.got) - ADDR(.got) == LOADADDR(.data) - ADDR(.data),
  ".got outside the boot copy's single load/run offset");

/* The arena placement: the payload's GC arena and its start bits at the head
 * of DATA, the section's size the golden contract's 96 KB plus the bits. */
ASSERT(ADDR(.rhu_payload_arena) == ORIGIN(DATA),
  "the payload arena is not at the head of DATA (the collector's count follows its address)");
ASSERT(SIZEOF(.rhu_payload_arena) == 0 || SIZEOF(.rhu_payload_arena) == 99840,
  "the payload arena section is neither empty (a runtime that never asks for the kernel arena) nor the 96 KB arena plus its 1,536 bytes of start bits");

/* The null-page guard: the protection unit denies the first 4 KB of
 * the address space on every row, so no code may be placed below it; the
 * vector table alone may (exception vector reads bypass the unit). */
ASSERT(__stext >= 0x1000,
  "code placed inside the first page, which the protection unit denies (the null-page guard)");

/* The console's link-time words. A project's knobs reach this
 * prebuilt kernel as ABSOLUTE SYMBOLS the kernel reads by address at boot;
 * an image linked without a knob gets 0 = the row's default (the facts'
 * channels row; the level's 0 = verbose). The SDK passes --defsym from the
 * project's properties; build.rs passes the environment's for the kernel's
 * own images. The pin words are the serial act's (0 = the facts' pair). */
PROVIDE(__rhu_log_level = 0);
PROVIDE(__rhu_log_channel = 0);
PROVIDE(__rhu_console_channel = 0);
PROVIDE(__rhu_uart0_tx = 0);
PROVIDE(__rhu_uart0_rx = 0);
PROVIDE(__rhu_uart1_tx = 0);
PROVIDE(__rhu_uart1_rx = 0);
/* The handler stack's watermark word: 0 = off; 1 = the boot stack reserve is
 * filled with a pattern at reset and its deepest use printed at the end. */
PROVIDE(__rhu_stack_watermark = 0);
