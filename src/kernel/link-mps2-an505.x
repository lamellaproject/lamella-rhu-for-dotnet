/* Lamella RHU -- QEMU mps2-an505 (Cortex-M33 FPGA image) board map.
 *
 * Machine reality (AN505 application note; QEMU 11.0.1 probed live --
 * info mtree + QOM init-svtor/EXP_NUMIRQ), all through the SECURE
 * aliases (the M33 boots Secure and this kernel runs secure-flat):
 *   0x10000000  ZBT SSRAM1   4 MB   code; reset vector fetch here
 *                                   (machine init-svtor = 0x10000000)
 *   0x38000000  SSRAM2/3     4 MB   data / bss / boot stack
 *   0x80000000  PSRAM       16 MB   not used by this image
 *
 * Region LENGTHs state the machine's physical sizes, so an overflow
 * fails the link loudly instead of producing an image that loads
 * wrapped.
 *
 * The layout is flash-parity: .data links at its RAM VMA and loads at an
 * LMA inside CODE, and Reset copies it across. The ELF loads at LMA, so
 * every boot on this machine exercises the same copy a flash-resident
 * image needs.
 */

MEMORY
{
  CODE (rx)  : ORIGIN = 0x10000000, LENGTH = 3M
  /* Reserved for the split-image app slot: nothing from the kernel may
   * land here. */
  APP  (rx)  : ORIGIN = 0x10300000, LENGTH = 1M
  DATA (rwx) : ORIGIN = 0x38000000, LENGTH = 4M
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
    KEEP(*(.vector_table.interrupts)); /* [16..139] external IRQ lines   */
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
