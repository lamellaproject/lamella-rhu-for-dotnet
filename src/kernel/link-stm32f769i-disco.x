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
 * loads at an LMA inside CODE, and Reset copies it across -- the same
 * copy path the QEMU images use (flash-parity by construction).
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

/* GC-substrate floor: the arena window this board GUARANTEES after
 * statics + .kstacks + the boot-stack reserve (link-enforced by the
 * window asserts at the bottom of this map). */
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
    KEEP(*(.vector_table.interrupts)); /* [16..125] external IRQ lines   */
  } > CODE

  .text : ALIGN(4)
  {
    __stext = .;
    /* The hosting-API surface (rhu_now_ns .. rhu_futex_wake): a
     * consumer links against these AFTER this image is built, so
     * section GC must never fold a fully-inlined one away. The test
     * harness asserts all four in the artifact. */
    KEEP(*(.text.rhu_api));
    *(.text .text.*);
    . = ALIGN(4);
    __etext = .;
  } > CODE

  /* Zero-length module brackets (base family): no unwind tables
   * (panic=abort discards them) and no frozen blobs ever land in a
   * base image, but module 0's self-registration is ONE code path in
   * both families -- these define the same bracket symbols the
   * payload maps bracket for real, as empty ranges in CODE. The two
   * pairs sit at distinct addresses so a cross-wired registration
   * cannot alias into accidental green. */
  __exidx_start = .;
  __exidx_end = .;

  .rodata : ALIGN(4)
  {
    *(.rodata .rodata.*);
    /* Debug-visible layout contract record (__rhu_debug_layout):
     * nothing in the image references it, so only KEEP holds it
     * through section GC. The test harness asserts it in the artifact. */
    KEEP(*(.rhu_debug));
    . = ALIGN(4);
  } > CODE

  /* The frozen pair (empty range; see the bracket note after
   * .text). */
  __rhu_frozen_lo = .;
  __rhu_frozen_hi = .;

  .data : ALIGN(8)
  {
    __sdata = .;
    *(.data .data.*);
    . = ALIGN(8);
  } > DATA AT> CODE
  __sidata = LOADADDR(.data);

  /* The TLS-template pair: module 0's thread-local template section,
   * the payload map's shape (a RAM address -- distinct by region from
   * the frozen and exidx pairs, so a cross-wired registration stays
   * detectable), inside the boot copy window -- [__sdata, __edata)
   * ends here -- at .data's load/run offset (asserted at the bottom
   * of this map). Empty in this map's images: nothing they link
   * carries thread-local input. */
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

  /* panic=abort image: no unwind tables. */
  /DISCARD/ :
  {
    *(.ARM.exidx .ARM.exidx.*);
    *(.ARM.extab .ARM.extab.*);
  }
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
