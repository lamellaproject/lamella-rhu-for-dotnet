/* Lamella RHU -- QEMU mps2-an505 PAYLOAD link map: the kernel staticlib
 * (librhu_kernel.a, feature `payload`) merged with one frozen ILC
 * payload object under an external `ld -T`. Same machine map as
 * link-mps2-an505.x -- keep the two in sync BY HAND -- plus the managed
 * sections an ILC object carries: __managedcode into text, the
 * __modules table bracketed for the module-initializer walk, frozen/
 * metadata blobs into rodata, hydrated data into .data, and the ARM
 * unwind tables KEPT (managed code emits them; the base script's
 * panic=abort discard does not apply here).
 *
 * Geometry discipline: every output section lands inside a declared
 * MEMORY region, so overflow fails the LINK loudly; the payload gate
 * additionally asserts the contract symbols and the managed entry in
 * the ARTIFACT by nm (a section the linker cannot police would need
 * exactly that assert, so the gate carries it).
 */

MEMORY
{
  CODE (rx)  : ORIGIN = 0x10000000, LENGTH = 3M
  /* Reserved for the split-image app slot: nothing from this image may
   * land here. */
  APP  (rx)  : ORIGIN = 0x10300000, LENGTH = 1M
  DATA (rwx) : ORIGIN = 0x38000000, LENGTH = 4M
}

/* GC-substrate floor: the arena window this board GUARANTEES after
 * statics + .kstacks + the boot-stack reserve (link-enforced by the
 * window asserts at the bottom of this map). */
RHU_ARENA_MIN = 2M;

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
    KEEP(*(.vector_table.interrupts)); /* [16..139] external IRQ lines   */
  } > CODE

  .text : ALIGN(4)
  {
    __stext = .;
    /* The hosting-API surface (rhu_now_ns .. rhu_futex_wake): the shim
     * backend links against these, so section GC must never fold a
     * fully-inlined one away. The test harness asserts all four. */
    KEEP(*(.text.rhu_api));
    *(.text .text.*);
    *(__managedcode*);
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

  /* Static-link GOT: any GOT-relative references in the payload object
   * resolve to link-time constants parked here, read-only. */
  .got : ALIGN(4) { *(.got) *(.got.plt) } > CODE

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
    *(.hydrated*);
    . = ALIGN(8);
  } > DATA AT> CODE
  __sidata = LOADADDR(.data);

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

  /DISCARD/ : { *(.comment) *(.note*) }
}

/* The arena window: link-defined slack between the end of
 * .kstacks and the boot/handler-stack reserve at the top of DATA.
 * Kernel-owned NOLOAD space outside every section; arena::init()
 * establishes the zero-birth invariant over it at boot. (Top level: the
 * linker rejects a bare ASSERT between output sections and reports the
 * error one command late.) */
RHU_BOOT_STACK_RESERVE = 64K;
__rhu_arena_lo = ALIGN(__ekstacks, 8);
__rhu_arena_hi = __stack_start - RHU_BOOT_STACK_RESERVE;
ASSERT(__rhu_arena_hi >= __rhu_arena_lo,
  "static RAM + .kstacks overflow into the boot-stack reserve (arena window negative)");
ASSERT(__rhu_arena_hi - __rhu_arena_lo >= RHU_ARENA_MIN,
  "arena window under this board's GC-substrate floor (RHU_ARENA_MIN)");

/* The module-0 TLS template rides the boot's single .data copy: its
 * load/run offset must equal .data's, on every board. */
ASSERT(LOADADDR(.tdata) - ADDR(.tdata) == LOADADDR(.data) - ADDR(.data),
  "module-0 TLS template outside the boot copy's single load/run offset");
