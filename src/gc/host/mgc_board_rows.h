/* Per-board payload-tier GC configuration rows.
 *
 * One row per supported board, selected by exactly one
 * MGC_HOST_ROW_<BOARD> define at compile time. A row is DECLARATION,
 * not machinery: the payload-tier fields (the least managed heap the
 * board guarantees, the card cover, the native heap's carve, the
 * hosted-thread stack default and clamp, the handle slots, the frozen
 * object heap, the system memory left unallocated) plus the collector's
 * internal sizings. Every value is either a measured maximum (handle live
 * high-water 8 of 64; window outstanding high-water 2; mark-stack high-water
 * 3 of 8192) or derived from the board's own
 * link-map geometry (the DATA window, the map's guaranteed floor of
 * system memory, the board module's native-heap carve).
 *
 * THE MANAGED HEAP IS SIZED AT BOOT (basis: boot, on every row). The
 * collector asks at its start how much system memory is still free and
 * takes all of it, less the collector tables for that size and less
 * what the row keeps for the two things drawn later: the frozen object
 * heap and the unallocated amount. A row therefore declares no heap
 * size. It declares two numbers a board author owns:
 *
 *   MGC_ROW_HEAP_MIN_GUARANTEED_BYTES
 *     The smallest managed heap a program is guaranteed on this board:
 *     every image that links gets at least this much, whatever its
 *     static data, and a boot that could not give it stops loudly
 *     instead of running with less.
 *
 *   MGC_ROW_SYSTEM_MEMORY_UNALLOCATED_BYTES
 *     The system memory the managed heap leaves untaken at boot, beside
 *     the frozen object heap: it serves the later draws of the runtime
 *     and the collector (a page of 256 more handles, a small runtime
 *     mapping).
 *
 * The guarantee is the map's floor of system memory less everything
 * else a boot draws: the native heap, one page of alignment, two pages
 * for the runtime's early mappings, the collector tables for the least
 * heap (its mark bitmap, the heap / 32, and the mark stack), the frozen
 * object heap and the unallocated amount. The gate checks that
 * inequality for every row against the board's map.
 *
 * The gate parses ALL rows below (as text, unselected blocks
 * included) and checks each against its payload map and board
 * module; the selected row is additionally asserted in the artifact:
 * the row census prints from these constants, the containment
 * verdict proves the managed heap's window sits inside the cover
 * granule (and, under the internal posture, the statics window with
 * it), and the system-memory verdict proves the managed heap took all
 * the rule gives.
 *
 * Invariants a row carries:
 *  - The card cover must span every legal bulk-move destination
 *    inside the write barrier's bounds: the managed heap's whole
 *    window, and the statics window with it under the internal
 *    posture, where the two share the DATA window (under the external
 *    posture a static destination lies outside the heap bounds and
 *    marks no card). An unaffordable cover dies
 *    LOUDLY at boot in the heap's own coverage guard, never a silent
 *    trim. The cover is a power of two (the guard's floor mask) and
 *    the MINIMAL one whose aligned granule holds the managed heap's
 *    whole window -- the board's DATA window under the internal
 *    posture, the external RAM's window under the external one --
 *    undersized fails containment, oversized wastes
 *    static card bytes; both are refused by the gate's table check.
 *  - Evidence postures (fragmentation census, compaction,
 *    multi-range) carry their own images and goldens. A row image
 *    never carries one; the guard below refuses at compile time.
 *  - stack_default/stack_clamp and the frozen object heap are declared
 *    here and consumed at the shim tier. The frozen object heap's pool
 *    is a draw of whole pages, reserved for the runtime's frozen-object
 *    segment. The runtime commits that segment 64 KB at a time: a pool
 *    of 64 KB is one commit, and a second commit fails into a managed
 *    allocation failure; a smaller pool takes the first commit and the
 *    collector stops the program loudly when the pool has no room for
 *    one more object, before any byte is written outside it. Handle
 *    slots and the small rows' sizings follow the high-waters above.
 */
#ifndef MGC_BOARD_ROWS_H
#define MGC_BOARD_ROWS_H

/* Evidence postures never ride a row image (census/compact/
 * multirange carry their own images and goldens). */
#if defined(MGC_FRAG_CENSUS) || defined(MGC_COMPACT) || defined(MGC_MULTIRANGE)
#error "evidence postures never ride a row image (census/compact/multirange carry their own images and goldens)"
#endif

#if defined(MGC_HOST_ROW_AN500)
/* QEMU mps2-an500: the gate world. DATA 4M @ 0x20000000; the map
 * guarantees 2M of system memory there, and the machine's 16 MB PSRAM
 * bank at 0x60000000 is the arena's second window after the
 * rehydrated-data region, where the managed heap and its collector
 * tables live (the external posture). The guarantee's arithmetic, in two
 * halves: the SRAM's native heap 64K + alignment 4K + early mappings 8K
 * + frozen object heap 64K + 64K unallocated = 204K inside the 2048K
 * floor; the bank's least managed heap 1536K + its mark bitmap 48K +
 * mark stack 32K = 1616K inside the 16M window. Cover 16M holds the
 * whole bank in one granule. */
#define MGC_ROW_BOARD             "mps2-an500"
#define MGC_ROW_PROFILE           "suite"
#define MGC_ROW_BASIS             "boot"
#define MGC_ROW_HEAP_MIN_GUARANTEED_BYTES        1572864u
#define MGC_ROW_SYSTEM_MEMORY_UNALLOCATED_BYTES    65536u
#define MGC_ROW_COVER_BYTES      16777216u
#define MGC_ROW_KHEAP_BYTES         65536u
#define MGC_ROW_STACK_DEFAULT       16384u
#define MGC_ROW_STACK_CLAMP         24576u
#define MGC_ROW_HANDLE_SLOTS           64u
#define MGC_ROW_FOH_POOL_BYTES      65536u
#define MGC_ROW_MARKSTACK_ENTRIES    8192u
#define MGC_ROW_FINQ_CAP             2048u
#define MGC_ROW_FINQ_RESERVE           16u
#define MGC_ROW_OOM_RESERVE         65536u
#define MGC_ROW_WINDOW_BYTES         4096u
#define MGC_ROW_WINDOW_FLOOR          256u
#define MGC_ROW_WALL_RETRIES            4u
#define MGC_ROW_FROZEN_SEGS             8u
#elif defined(MGC_HOST_ROW_AN505)
/* QEMU mps2-an505: the an500's ARMv8-M twin by map class. DATA 4M @
 * 0x38000000 (4M-aligned); the same 2M floor, the machine's 16 MB PSRAM
 * bank at 0x80000000 as the same second window, and the same row. */
#define MGC_ROW_BOARD             "mps2-an505"
#define MGC_ROW_PROFILE           "suite"
#define MGC_ROW_BASIS             "boot"
#define MGC_ROW_HEAP_MIN_GUARANTEED_BYTES        1572864u
#define MGC_ROW_SYSTEM_MEMORY_UNALLOCATED_BYTES    65536u
#define MGC_ROW_COVER_BYTES      16777216u
#define MGC_ROW_KHEAP_BYTES         65536u
#define MGC_ROW_STACK_DEFAULT       16384u
#define MGC_ROW_STACK_CLAMP         24576u
#define MGC_ROW_HANDLE_SLOTS           64u
#define MGC_ROW_FOH_POOL_BYTES      65536u
#define MGC_ROW_MARKSTACK_ENTRIES    8192u
#define MGC_ROW_FINQ_CAP             2048u
#define MGC_ROW_FINQ_RESERVE           16u
#define MGC_ROW_OOM_RESERVE         65536u
#define MGC_ROW_WINDOW_BYTES         4096u
#define MGC_ROW_WINDOW_FLOOR          256u
#define MGC_ROW_WALL_RETRIES            4u
#define MGC_ROW_FROZEN_SEGS             8u
#elif defined(MGC_HOST_ROW_U5A5)
/* NUCLEO-U5A5ZJ-Q: contiguous SRAM 2496K @ 0x20000000, the map's floor
 * of system memory 1536K. The guarantee's arithmetic: native heap 64K
 * + alignment 4K + early mappings 8K + the least managed heap 1024K +
 * its mark bitmap 32K + mark stack 32K + frozen object heap 64K + 64K
 * unallocated = 1292K inside the floor (the roomy row guarantees a
 * round megabyte). Cover 4M is minimal: a 2M granule at 0x20000000
 * ends below the SRAM end 0x20270000. */
#define MGC_ROW_BOARD             "st-nucleo-u5a5zj-q"
#define MGC_ROW_PROFILE           "suite"
#define MGC_ROW_BASIS             "boot"
#define MGC_ROW_HEAP_MIN_GUARANTEED_BYTES        1048576u
#define MGC_ROW_SYSTEM_MEMORY_UNALLOCATED_BYTES    65536u
#define MGC_ROW_COVER_BYTES       4194304u
#define MGC_ROW_KHEAP_BYTES         65536u
#define MGC_ROW_STACK_DEFAULT       16384u
#define MGC_ROW_STACK_CLAMP         24576u
#define MGC_ROW_HANDLE_SLOTS           64u
#define MGC_ROW_FOH_POOL_BYTES      65536u
#define MGC_ROW_MARKSTACK_ENTRIES    8192u
#define MGC_ROW_FINQ_CAP             2048u
#define MGC_ROW_FINQ_RESERVE           16u
#define MGC_ROW_OOM_RESERVE         65536u
#define MGC_ROW_WINDOW_BYTES         4096u
#define MGC_ROW_WINDOW_FLOOR          256u
#define MGC_ROW_WALL_RETRIES            4u
#define MGC_ROW_FROZEN_SEGS             8u
#elif defined(MGC_HOST_ROW_F769)
/* STM32F769I-DISCO: DATA 512K @ 0x20000000 (the DTCM inside DATA,
 * contiguous with SRAM1 and SRAM2), the map's floor of system memory
 * 160K; the board's 16 MB SDRAM at 0xC0000000 is the arena's second
 * window after the rehydrated-data region, where the managed heap and
 * its collector tables live (the external posture). The small row, and
 * its guarantee's arithmetic in two halves: the SRAM's native heap 16K
 * + alignment 4K + early mappings 8K + frozen object heap 16K + 8K
 * unallocated = 52K inside the 160K floor; the SDRAM's least managed
 * heap 96K + its mark bitmap 4K (3K, in whole pages) + mark stack 8K =
 * 108K inside the 16M window. The
 * frozen object heap is 16K (room for about a thousand type objects).
 * The native-heap carve is 16K, twice the smallest carve a hosted program
 * boots and runs to its end on under the emulator (8K). The small
 * sizings scale to the observed high-waters: mark stack 2048 (observed
 * high-water 3); the finalization table 2048 entries, as the suite rows
 * carry, with 16 of them held back for an out-of-memory exception's own
 * finalizable allocation -- the larger table covers a typical dotnet/iot
 * application; the table is 10K
 * of the collector's statics (8K of slots, 2K of states) in the image's
 * .bss, placed by the map before the floor is measured, so no term of
 * the floor carries it; the out-of-memory reserve 8K (an exception's
 * dispatch allocates small objects; the grace round trip is the
 * protocol either way, a refused finalizer registration included: it is
 * answered through the same report, the exception's own finalizable
 * allocation served from the slots held back). Cover 16M
 * is minimal: the granule at 0xC0000000 is exactly the SDRAM
 * [0xC0000000, 0xC1000000). */
#define MGC_ROW_BOARD             "st-stm32f769i-disco"
#define MGC_ROW_PROFILE           "mini"
#define MGC_ROW_BASIS             "boot"
#define MGC_ROW_HEAP_MIN_GUARANTEED_BYTES          98304u
#define MGC_ROW_SYSTEM_MEMORY_UNALLOCATED_BYTES     8192u
#define MGC_ROW_COVER_BYTES      16777216u
#define MGC_ROW_KHEAP_BYTES         16384u
#define MGC_ROW_STACK_DEFAULT       16384u
#define MGC_ROW_STACK_CLAMP         24576u
#define MGC_ROW_HANDLE_SLOTS           64u
#define MGC_ROW_FOH_POOL_BYTES      16384u
#define MGC_ROW_MARKSTACK_ENTRIES    2048u
#define MGC_ROW_FINQ_CAP             2048u
#define MGC_ROW_FINQ_RESERVE           16u
#define MGC_ROW_OOM_RESERVE          8192u
#define MGC_ROW_WINDOW_BYTES         4096u
#define MGC_ROW_WINDOW_FLOOR          256u
#define MGC_ROW_WALL_RETRIES            4u
#define MGC_ROW_FROZEN_SEGS             8u
#elif defined(MGC_HOST_ROW_PICO2)
/* Raspberry Pi Pico 2: DATA 520K @ 0x20000000 (the
 * eight striped SRAM banks and the two small ones after them, one
 * contiguous window), the map's floor of system memory 160K. The small
 * row, its guarantee's arithmetic the STM32F769I-DISCO's: 16K + 4K +
 * 8K + 96K + 4K + 8K + 16K + 8K = 160K, exactly the floor. The
 * finalization table stays 256 entries, 240 of them usable (16 held
 * back for an out-of-memory exception's own finalizable allocation):
 * the board has no room for a larger table's SRAM. Cover 1M
 * is minimal: a 512K granule at 0x20000000 ends at 0x20080000, 8K
 * short of the window's end 0x20082000; the 1M granule holds it. */
#define MGC_ROW_BOARD             "rpi-pico2"
#define MGC_ROW_PROFILE           "mini"
#define MGC_ROW_BASIS             "boot"
#define MGC_ROW_HEAP_MIN_GUARANTEED_BYTES          98304u
#define MGC_ROW_SYSTEM_MEMORY_UNALLOCATED_BYTES     8192u
#define MGC_ROW_COVER_BYTES       1048576u
#define MGC_ROW_KHEAP_BYTES         16384u
#define MGC_ROW_STACK_DEFAULT       16384u
#define MGC_ROW_STACK_CLAMP         24576u
#define MGC_ROW_HANDLE_SLOTS           64u
#define MGC_ROW_FOH_POOL_BYTES      16384u
#define MGC_ROW_MARKSTACK_ENTRIES    2048u
#define MGC_ROW_FINQ_CAP              256u
#define MGC_ROW_FINQ_RESERVE           16u
#define MGC_ROW_OOM_RESERVE          8192u
#define MGC_ROW_WINDOW_BYTES         4096u
#define MGC_ROW_WINDOW_FLOOR          256u
#define MGC_ROW_WALL_RETRIES            4u
#define MGC_ROW_FROZEN_SEGS             8u
#elif defined(MGC_HOST_ROW_PICO2W)
/* Raspberry Pi Pico 2 W: the Pico 2 with a wireless module fitted, whose
 * lines the kernel never touches -- the same chip, the same 520K window
 * and map floor, so the same small row (the finalization table 256, 240
 * usable) and the same 1M cover. */
#define MGC_ROW_BOARD             "rpi-pico2-w"
#define MGC_ROW_PROFILE           "mini"
#define MGC_ROW_BASIS             "boot"
#define MGC_ROW_HEAP_MIN_GUARANTEED_BYTES          98304u
#define MGC_ROW_SYSTEM_MEMORY_UNALLOCATED_BYTES     8192u
#define MGC_ROW_COVER_BYTES       1048576u
#define MGC_ROW_KHEAP_BYTES         16384u
#define MGC_ROW_STACK_DEFAULT       16384u
#define MGC_ROW_STACK_CLAMP         24576u
#define MGC_ROW_HANDLE_SLOTS           64u
#define MGC_ROW_FOH_POOL_BYTES      16384u
#define MGC_ROW_MARKSTACK_ENTRIES    2048u
#define MGC_ROW_FINQ_CAP              256u
#define MGC_ROW_FINQ_RESERVE           16u
#define MGC_ROW_OOM_RESERVE          8192u
#define MGC_ROW_WINDOW_BYTES         4096u
#define MGC_ROW_WINDOW_FLOOR          256u
#define MGC_ROW_WALL_RETRIES            4u
#define MGC_ROW_FROZEN_SEGS             8u
#elif defined(MGC_HOST_ROW_PICOPLUS2)
/* Pimoroni Pico Plus 2: the same chip, the same 520K SRAM
 * window and map floor as the Pico 2, so the same small row but for the
 * cover: the board's 8 MB PSRAM at 0x11000000 is the arena's second
 * window after the rehydrated-data region, where the managed heap and
 * its collector tables live (the external posture), so the cover is the
 * PSRAM's granule 8M; the SRAM keeps the native heap, the frozen object
 * heap and the unallocated amount (16K + 4K + 8K + 16K + 8K = 52K inside
 * the floor). The finalization table is 2048 entries (2032 usable, 16
 * held back for an out-of-memory exception's own finalizable
 * allocation): the larger table covers a typical dotnet/iot application;
 * the table's 10K (8K of slots, 2K of states)
 * is the collector's statics in the image's .bss, placed by the map
 * before the floor is measured, so no term of the floor carries it. */
#define MGC_ROW_BOARD             "pimoroni-pico-plus-2"
#define MGC_ROW_PROFILE           "mini"
#define MGC_ROW_BASIS             "boot"
#define MGC_ROW_HEAP_MIN_GUARANTEED_BYTES          98304u
#define MGC_ROW_SYSTEM_MEMORY_UNALLOCATED_BYTES     8192u
#define MGC_ROW_COVER_BYTES       8388608u
#define MGC_ROW_KHEAP_BYTES         16384u
#define MGC_ROW_STACK_DEFAULT       16384u
#define MGC_ROW_STACK_CLAMP         24576u
#define MGC_ROW_HANDLE_SLOTS           64u
#define MGC_ROW_FOH_POOL_BYTES      16384u
#define MGC_ROW_MARKSTACK_ENTRIES    2048u
#define MGC_ROW_FINQ_CAP             2048u
#define MGC_ROW_FINQ_RESERVE           16u
#define MGC_ROW_OOM_RESERVE          8192u
#define MGC_ROW_WINDOW_BYTES         4096u
#define MGC_ROW_WINDOW_FLOOR          256u
#define MGC_ROW_WALL_RETRIES            4u
#define MGC_ROW_FROZEN_SEGS             8u
#elif defined(MGC_HOST_ROW_PICOPLUS2W)
/* Pimoroni Pico Plus 2 W: the Pico Plus 2 with a wireless module fitted,
 * whose lines are the wifi driver's -- the same chip, the same 520K
 * window and map floor, the same 8 MB PSRAM as the second window, so the
 * same small row (the finalization table 2048, 2032 usable) and the same
 * 8M cover. */
#define MGC_ROW_BOARD             "pimoroni-pico-plus-2-w"
#define MGC_ROW_PROFILE           "mini"
#define MGC_ROW_BASIS             "boot"
#define MGC_ROW_HEAP_MIN_GUARANTEED_BYTES          98304u
#define MGC_ROW_SYSTEM_MEMORY_UNALLOCATED_BYTES     8192u
#define MGC_ROW_COVER_BYTES       8388608u
#define MGC_ROW_KHEAP_BYTES         16384u
#define MGC_ROW_STACK_DEFAULT       16384u
#define MGC_ROW_STACK_CLAMP         24576u
#define MGC_ROW_HANDLE_SLOTS           64u
#define MGC_ROW_FOH_POOL_BYTES      16384u
#define MGC_ROW_MARKSTACK_ENTRIES    2048u
#define MGC_ROW_FINQ_CAP             2048u
#define MGC_ROW_FINQ_RESERVE           16u
#define MGC_ROW_OOM_RESERVE          8192u
#define MGC_ROW_WINDOW_BYTES         4096u
#define MGC_ROW_WINDOW_FLOOR          256u
#define MGC_ROW_WALL_RETRIES            4u
#define MGC_ROW_FROZEN_SEGS             8u
#elif defined(MGC_HOST_ROW_FEATHER2350)
/* Adafruit Feather RP2350 with HSTX: the Pico * 2's chip (the RP2350A),
 * the same 520K SRAM window and map floor, no external RAM, so the Pico
 * 2's small row whole: the finalization table 256 (240 usable) and the
 * 1M cover. The flash is 8 MB; nothing of this row reads it. */
#define MGC_ROW_BOARD             "adafruit-feather-rp2350-hstx"
#define MGC_ROW_PROFILE           "mini"
#define MGC_ROW_BASIS             "boot"
#define MGC_ROW_HEAP_MIN_GUARANTEED_BYTES          98304u
#define MGC_ROW_SYSTEM_MEMORY_UNALLOCATED_BYTES     8192u
#define MGC_ROW_COVER_BYTES       1048576u
#define MGC_ROW_KHEAP_BYTES         16384u
#define MGC_ROW_STACK_DEFAULT       16384u
#define MGC_ROW_STACK_CLAMP         24576u
#define MGC_ROW_HANDLE_SLOTS           64u
#define MGC_ROW_FOH_POOL_BYTES      16384u
#define MGC_ROW_MARKSTACK_ENTRIES    2048u
#define MGC_ROW_FINQ_CAP              256u
#define MGC_ROW_FINQ_RESERVE           16u
#define MGC_ROW_OOM_RESERVE          8192u
#define MGC_ROW_WINDOW_BYTES         4096u
#define MGC_ROW_WINDOW_FLOOR          256u
#define MGC_ROW_WALL_RETRIES            4u
#define MGC_ROW_FROZEN_SEGS             8u
#elif defined(MGC_HOST_ROW_FEATHER2350PSRAM)
/* Adafruit Feather RP2350 with HSTX and PSRAM: the Pico 2's chip with the
 * same 8 MB PSRAM part the Pimoroni Pico Plus 2 carries, behind the flash
 * interface's second chip select (GPIO 8) at the same window, so the Pico
 * Plus 2's small row whole: the cover the PSRAM's granule 8M, the
 * finalization table 2048 (2032 usable). */
#define MGC_ROW_BOARD             "adafruit-feather-rp2350-hstx-psram"
#define MGC_ROW_PROFILE           "mini"
#define MGC_ROW_BASIS             "boot"
#define MGC_ROW_HEAP_MIN_GUARANTEED_BYTES          98304u
#define MGC_ROW_SYSTEM_MEMORY_UNALLOCATED_BYTES     8192u
#define MGC_ROW_COVER_BYTES       8388608u
#define MGC_ROW_KHEAP_BYTES         16384u
#define MGC_ROW_STACK_DEFAULT       16384u
#define MGC_ROW_STACK_CLAMP         24576u
#define MGC_ROW_HANDLE_SLOTS           64u
#define MGC_ROW_FOH_POOL_BYTES      16384u
#define MGC_ROW_MARKSTACK_ENTRIES    2048u
#define MGC_ROW_FINQ_CAP             2048u
#define MGC_ROW_FINQ_RESERVE           16u
#define MGC_ROW_OOM_RESERVE          8192u
#define MGC_ROW_WINDOW_BYTES         4096u
#define MGC_ROW_WINDOW_FLOOR          256u
#define MGC_ROW_WALL_RETRIES            4u
#define MGC_ROW_FROZEN_SEGS             8u
#elif defined(MGC_HOST_ROW_PICOLIPO2)
/* Pimoroni Pico LiPo 2: the Pico Plus 2's chip (the RP2350B), SRAM
 * window, map floor and 8 MB PSRAM at the same window behind the same
 * chip select (GPIO 47), so the Pico Plus 2's small row whole. */
#define MGC_ROW_BOARD             "pimoroni-pico-lipo-2"
#define MGC_ROW_PROFILE           "mini"
#define MGC_ROW_BASIS             "boot"
#define MGC_ROW_HEAP_MIN_GUARANTEED_BYTES          98304u
#define MGC_ROW_SYSTEM_MEMORY_UNALLOCATED_BYTES     8192u
#define MGC_ROW_COVER_BYTES       8388608u
#define MGC_ROW_KHEAP_BYTES         16384u
#define MGC_ROW_STACK_DEFAULT       16384u
#define MGC_ROW_STACK_CLAMP         24576u
#define MGC_ROW_HANDLE_SLOTS           64u
#define MGC_ROW_FOH_POOL_BYTES      16384u
#define MGC_ROW_MARKSTACK_ENTRIES    2048u
#define MGC_ROW_FINQ_CAP             2048u
#define MGC_ROW_FINQ_RESERVE           16u
#define MGC_ROW_OOM_RESERVE          8192u
#define MGC_ROW_WINDOW_BYTES         4096u
#define MGC_ROW_WINDOW_FLOOR          256u
#define MGC_ROW_WALL_RETRIES            4u
#define MGC_ROW_FROZEN_SEGS             8u
#elif defined(MGC_HOST_ROW_PICOLIPO2XLW)
/* Pimoroni Pico LiPo 2 XL W: the Pico LiPo 2's board with a wireless module the kernel does not drive -- the Pico Plus 2's chip (the RP2350B), SRAM
 * window, map floor and 8 MB PSRAM at the same window behind the same
 * chip select (GPIO 47), so the Pico Plus 2's small row whole. */
#define MGC_ROW_BOARD             "pimoroni-pico-lipo-2-xl-w"
#define MGC_ROW_PROFILE           "mini"
#define MGC_ROW_BASIS             "boot"
#define MGC_ROW_HEAP_MIN_GUARANTEED_BYTES          98304u
#define MGC_ROW_SYSTEM_MEMORY_UNALLOCATED_BYTES     8192u
#define MGC_ROW_COVER_BYTES       8388608u
#define MGC_ROW_KHEAP_BYTES         16384u
#define MGC_ROW_STACK_DEFAULT       16384u
#define MGC_ROW_STACK_CLAMP         24576u
#define MGC_ROW_HANDLE_SLOTS           64u
#define MGC_ROW_FOH_POOL_BYTES      16384u
#define MGC_ROW_MARKSTACK_ENTRIES    2048u
#define MGC_ROW_FINQ_CAP             2048u
#define MGC_ROW_FINQ_RESERVE           16u
#define MGC_ROW_OOM_RESERVE          8192u
#define MGC_ROW_WINDOW_BYTES         4096u
#define MGC_ROW_WINDOW_FLOOR          256u
#define MGC_ROW_WALL_RETRIES            4u
#define MGC_ROW_FROZEN_SEGS             8u
#else
#error "a row world names its board (define exactly one MGC_HOST_ROW_<BOARD>)"
#endif

/* Row self-consistency (external knob ties are proven in the transcript:
 * the cover line's buffer total, the boot line's four numbers, the
 * handle census's slot count). */
static_assert((MGC_ROW_COVER_BYTES & (MGC_ROW_COVER_BYTES - 1u)) == 0u,
              "the cover is a power of two (the coverage guard's floor mask)");
static_assert(MGC_ROW_OOM_RESERVE < MGC_ROW_HEAP_MIN_GUARANTEED_BYTES,
              "the out-of-memory reserve lives inside the least managed heap the board guarantees");
static_assert(MGC_ROW_FINQ_RESERVE < MGC_ROW_FINQ_CAP,
              "the finalization reserve is a part of the table (the slots held back for an exception's own finalizable allocation), not the table");
static_assert(MGC_ROW_WINDOW_FLOOR < MGC_ROW_WINDOW_BYTES,
              "the window floor is a fraction of the window");
static_assert(MGC_ROW_STACK_DEFAULT <= MGC_ROW_STACK_CLAMP,
              "the default hosted stack respects the clamp");
static_assert((MGC_ROW_FOH_POOL_BYTES & 4095u) == 0u,
              "the frozen object heap is whole pages (the mmap bridge's draw granule)");
static_assert((MGC_ROW_SYSTEM_MEMORY_UNALLOCATED_BYTES & 4095u) == 0u,
              "the unallocated system memory is whole pages");

#endif /* MGC_BOARD_ROWS_H */
