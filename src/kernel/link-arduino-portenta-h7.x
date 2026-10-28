/* Lamella RHU -- Arduino Portenta H7 family (STM32H747XI, the Cortex-M7 core) board map:
 * one map for the Portenta H7, the Portenta H7 Lite and the Portenta H7
 * Lite Connected (one chip, one memory layout; the three files are
 * byte-identical).
 *
 * Machine reality (RM0399, the STM32H747xI datasheet):
 *   0x08000000  flash      2 MB   two banks of 1 MB, eight 128K sectors each;
 *                                 the part boots this flash at the address
 *                                 its boot option bytes name (the factory
 *                                 0x08000000); no alias of it at address 0,
 *                                 so the vector table's address is stated to
 *                                 the core at reset
 *   0x24000000  AXI SRAM 512 KB   the D1 domain's RAM on the bus matrix,
 *                                 reachable by every DMA master: this image's
 *                                 DATA, the one contiguous window every
 *                                 kernel static, the console mirror, the
 *                                 stacks and the arena live in
 *   0x20000000  DTCM     128 KB   the core's zero-wait-state data memory on
 *                                 its own port (the DMA masters do not reach
 *                                 it); enabled, no section placed here
 *   0x30000000  SRAM1..3 288 KB   the D2 domain's three blocks, contiguous;
 *                                 allocated to this core at boot, no section
 *   0x38000000  SRAM4     64 KB   the D3 domain's block; no section
 *   0x00000000  ITCM      64 KB   the instruction tightly coupled memory;
 *                                 unused (the first page denied by the
 *                                 protection unit)
 *
 * Region LENGTHs state the physical sizes, so an overflow fails the
 * link loudly instead of producing an image that loads wrapped.
 *
 * The layout is flash-resident for real: .data links at its RAM VMA and
 * loads at an LMA inside CODE, and Reset copies it across -- the same
 * copy path the QEMU images use (flash-parity by construction).
 *
 * The entry is the family's register-only supply entry: the chip's supply
 * configuration is written there before any RAM is touched, then Reset.
 */

MEMORY
{
  CODE (rx)  : ORIGIN = 0x08000000, LENGTH = 1536K
  /* Reserved for the split-image app slot: nothing from the kernel may
   * land here. 0x08180000 is a 128K-sector boundary (the twelfth sector
   * of bank 2 begins there), so the slot erases without touching CODE
   * sectors. */
  APP  (rx)  : ORIGIN = 0x08180000, LENGTH = 512K
  DATA (rwx) : ORIGIN = 0x24000000, LENGTH = 512K
}

/* GC-substrate floor: the arena window this board GUARANTEES after
 * statics + .kstacks + the console mirror + the boot-stack reserve
 * (link-enforced by the window asserts at the bottom of this map). */
RHU_ARENA_MIN = 192K;

ENTRY(__rhu_stm32h7_supply_reset);

/* Boot stack: top of DATA, full-descending. */
__stack_start = ORIGIN(DATA) + LENGTH(DATA);

SECTIONS
{
  .vector_table ORIGIN(CODE) :
  {
    __vector_table = .;
    LONG(__stack_start);               /* [0]       initial MSP          */
    KEEP(*(.vector_table.reset));      /* [1]       the supply entry     */
    KEEP(*(.vector_table.exceptions)); /* [2..15]   system exceptions    */
    KEEP(*(.vector_table.interrupts)); /* [16..165] external IRQ lines   */
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

  /* The console mirror (__rhu_console_log): the transcript a debug
   * probe reads out of RAM, and the transmit ring's storage. NOLOAD,
   * after the thread stacks and outside the statics window like them;
   * the board module writes its header at boot. KEEP: the probe reads
   * it by symbol, nothing in the image references it by name. */
  .rhu_console_log (NOLOAD) : ALIGN(8)
  {
    KEEP(*(.rhu_console_log));
    . = ALIGN(8);
    __rhu_console_log_end = .;
  } > DATA

  /* panic=abort image: no unwind tables. */
  /DISCARD/ :
  {
    *(.ARM.exidx .ARM.exidx.*);
    *(.ARM.extab .ARM.extab.*);
  }
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

/* The in-image header sits at the fixed offset every reader uses, the
 * vector table's padded slot ends exactly there, and the record is
 * layout version 1's sixteen words -- on every board. */
ASSERT(ADDR(.rhu_header) == ORIGIN(CODE) + 0x400,
  "image header not at the fixed offset 0x400 from the image base");
ASSERT(SIZEOF(.vector_table) == 0x400,
  "vector table slot is not the 0x400 bytes before the image header");
ASSERT(SIZEOF(.rhu_header) == 64,
  "image header is not the 64 bytes of layout version 1");

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
