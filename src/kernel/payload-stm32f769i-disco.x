/* Lamella RHU -- STM32F769I-DISCO PAYLOAD link map: the kernel staticlib
 * (librhu_kernel.a, feature `payload`) merged with one frozen ILC
 * payload object under an external `ld -T`. Same machine map as
 * link-stm32f769i-disco.x -- keep the two in sync BY HAND -- plus the
 * managed sections an ILC object carries: __managedcode into text, the
 * __modules table bracketed for the module-initializer walk, frozen/
 * metadata blobs into rodata, hydrated data into .data, and the ARM
 * unwind tables KEPT (managed code emits them; the base script's
 * panic=abort discard does not apply here).
 *
 * Geometry discipline: every output section lands inside a declared
 * MEMORY region, so overflow fails the LINK loudly; the payload gate
 * additionally asserts the contract symbols and the managed entry in
 * the ARTIFACT by nm (a section the linker cannot police would need
 * exactly that assert, so the gate carries it from day one).
 */

MEMORY
{
  CODE (rx)  : ORIGIN = 0x08000000, LENGTH = 1536K
  /* Reserved for the split-image app slot: nothing from this image may
   * land here. 0x08180000 is a 256K-sector boundary. */
  APP  (rx)  : ORIGIN = 0x08180000, LENGTH = 512K
  /* 0x20000000 DTCM 128K: RESERVED-UNUSED in this map revision. */
  DATA (rwx) : ORIGIN = 0x20020000, LENGTH = 384K
}

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
    KEEP(*(.vector_table.interrupts)); /* [16..125] external IRQ lines   */
  } > CODE

  .text : ALIGN(4)
  {
    /* The hosting-API surface (rhu_now_ns .. rhu_futex_wake): the shim
     * backend links against these, so section GC must never fold a
     * fully-inlined one away. The test harness asserts all four. */
    KEEP(*(.text.rhu_api));
    *(.text .text.*);
    *(__managedcode*);
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
    *(.frozen*);
    *(.dotnet*);
    /* Debug-visible layout contract record (__rhu_debug_layout):
     * nothing in the image references it, so only KEEP holds it
     * through section GC. The test harness asserts it in the artifact. */
    KEEP(*(.rhu_debug));
    . = ALIGN(4);
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

  .data : ALIGN(4)
  {
    __sdata = .;
    *(.data .data.*);
    *(.hydrated*);
    . = ALIGN(4);
    __edata = .;
  } > DATA AT> CODE
  __sidata = LOADADDR(.data);

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

/* Keep 64K of handler/boot stack headroom above static RAM use. (Top
 * level: the linker rejects a bare ASSERT between output sections and
 * reports the error one command late.) */
ASSERT(__stack_start - __ekstacks >= 64K, "static RAM leaves <64K of boot stack");
