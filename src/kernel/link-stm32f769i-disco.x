/* Lamella RHU -- STM32F769I-DISCO (STM32F769NI, Cortex-M7) board map.
 *
 * Machine reality (RM0410, F769NI datasheet):
 *   0x08000000  flash      2 MB   single bank; sectors 4x32K + 128K + 7x256K;
 *                                 the part boots this flash through its boot
 *                                 alias with factory option bytes, and the
 *                                 AXIM addresses in the vector table ground
 *                                 execution at 0x08...
 *   0x20000000  DTCM     128 KB   RESERVED-UNUSED in this map revision:
 *                                 adopting it is a future registered step;
 *                                 nothing may land here, code never will
 *   0x20020000  SRAM1    368 KB   contiguous with SRAM2:
 *   0x2007C000  SRAM2     16 KB   384 KB of AXI SRAM = this image's DATA
 *   0x00000000  ITCM RAM  16 KB   unused
 *
 * Region LENGTHs state the physical sizes, so an overflow fails the
 * link loudly instead of producing an image that loads wrapped.
 *
 * The layout is flash-resident for real: .data links at its RAM VMA and
 * loads at an LMA inside CODE, and Reset copies it across -- the exact
 * copy path every QEMU boot has always exercised (flash-parity by
 * construction).
 */

MEMORY
{
  CODE (rx)  : ORIGIN = 0x08000000, LENGTH = 1536K
  /* Reserved for the split-image app slot: nothing from the kernel may
   * land here. 0x08180000 is a 256K-sector boundary, so the slot
   * erases without touching CODE sectors. */
  APP  (rx)  : ORIGIN = 0x08180000, LENGTH = 512K
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
    /* The hosting-API surface (rhu_now_ns .. rhu_futex_wake): a
     * consumer links against these AFTER this image is built, so
     * section GC must never fold a fully-inlined one away. The test
     * harness asserts all four in the artifact. */
    KEEP(*(.text.rhu_api));
    *(.text .text.*);
  } > CODE

  .rodata : ALIGN(4)
  {
    *(.rodata .rodata.*);
    /* Debug-visible layout contract record (__rhu_debug_layout):
     * nothing in the image references it, so only KEEP holds it
     * through section GC. The test harness asserts it in the artifact. */
    KEEP(*(.rhu_debug));
    . = ALIGN(4);
  } > CODE

  .data : ALIGN(4)
  {
    __sdata = .;
    *(.data .data.*);
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

  /* panic=abort image: no unwind tables. */
  /DISCARD/ :
  {
    *(.ARM.exidx .ARM.exidx.*);
    *(.ARM.extab .ARM.extab.*);
  }
}

/* Keep 64K of handler/boot stack headroom above static RAM use. (Top
 * level: the linker rejects a bare ASSERT between output sections and
 * reports the error one command late.) */
ASSERT(__stack_start - __ekstacks >= 64K, "static RAM leaves <64K of boot stack");
