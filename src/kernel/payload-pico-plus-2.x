/* Lamella RHU -- Pimoroni Pico Plus 2 PAYLOAD link map: the kernel
 * staticlib (librhu_kernel.a, feature `payload`) merged with one frozen
 * ILC payload object under an external `ld -T`. Same machine map as
 * link-pico-plus-2.x -- keep the two in sync BY HAND -- plus the managed
 * sections an ILC object carries: managed code and unboxing stubs
 * bracketed inside text, the __modules table bracketed for the
 * module-initializer walk, frozen/metadata blobs into rodata, the
 * rehydrated-data region zero-filled and bracketed inside .bss, the GOT
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
   * datasheet's address map; the board's published facts for the 16 MB
   * part). */
  CODE (rx)  : ORIGIN = 0x10000000, LENGTH = 12M
  /* Reserved for the split-image app slot: nothing from this image may
   * land here. 0x10c00000 is a 4 KB-sector boundary. */
  APP  (rx)  : ORIGIN = 0x10c00000, LENGTH = 4M
  /* SRAM0-7 (striped, 512 KB) + SRAM8 + SRAM9 (4 KB each): one
   * contiguous 520 KB window ending at 0x20082000, the whole of it this
   * image's DATA (data and stacks only; no code is ever placed in RAM). */
  DATA (rwx) : ORIGIN = 0x20000000, LENGTH = 520K
}

/* GC-substrate floor: the arena window this board GUARANTEES after
 * statics + .kstacks + the console mirror + the boot-stack reserve
 * (link-enforced by the window asserts at the bottom of this map). The
 * 512 K-class floor: the mini GC row fits it beside the hosted runtime's
 * declared demand (the asserts at the bottom of this map). */
RHU_ARENA_MIN = 192K;

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

  .bss (NOLOAD) : ALIGN(4)
  {
    __sbss = .;
    *(.bss .bss.* COMMON);
    /* The hosted runtime's rehydrated-data region: zero-filled RAM the
     * runtime fills at startup from its compact form, bracketed and
     * 8-aligned as the runtime lays it out, inside the zero-fill and
     * statics windows. Reference-retained (the compact form points into
     * it); an empty pair until a hosted object carries the region. */
    . = ALIGN(8);
    __rhu_hydrated_lo = .;
    *(.hydrated*);
    __rhu_hydrated_hi = .;
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
RHU_BOOT_STACK_RESERVE = 64K;
__rhu_arena_lo = ALIGN(__rhu_console_log_end, 8);
__rhu_arena_hi = __stack_start - RHU_BOOT_STACK_RESERVE;
ASSERT(__rhu_arena_hi >= __rhu_arena_lo,
  "static RAM + .kstacks + the console mirror overflow into the boot-stack reserve (arena window negative)");
ASSERT(__rhu_arena_hi - __rhu_arena_lo >= RHU_ARENA_MIN,
  "arena window under this board's GC-substrate floor (RHU_ARENA_MIN)");

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

/* The hosted runtime's demand as the frozen tier states it (goldens/rt:
 * the merged program object's section classes by llvm-size -- the code
 * class, the copied data, the zero-filled data). The regions must hold
 * that demand beside the kernel's own reserves on every board: a region
 * shrunk under it refuses the link whatever the image at hand carries.
 * The kernel reserves are declared allowances, not measurements. */
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

/* The rehydrated-data region lies inside the zero-fill window. */
ASSERT(__rhu_hydrated_lo >= __sbss && __rhu_hydrated_hi <= __ebss,
  "rehydrated-data region outside the zero-fill window");
