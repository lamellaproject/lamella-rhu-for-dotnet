/* Per-board payload-tier GC configuration rows.
 *
 * One row per supported board, selected by exactly one
 * MGC_HOST_ROW_<BOARD> define at compile time. A row is DECLARATION,
 * not machinery: the payload-tier fields (the least managed heap the
 * board guarantees, the card cover, the native heap's carve, the
 * hosted-thread stack default and clamp, the handle slots, the frozen
 * object heap, the system memory left unallocated) plus the collector's
 * internal sizings. Every value is either measured on the gate drives
 * (handle live high-water 8 of 64; window outstanding high-water 2;
 * mark-stack high-water 3 of 8192) or derived from the board's own
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
 * verdict proves the statics window and the whole system-memory window
 * sit inside the cover granule, and the system-memory verdict proves
 * the managed heap took all the rule gives.
 *
 * Invariants a row carries:
 *  - The card cover must span every legal bulk-move destination
 *    (heap ranges + the statics window). An unaffordable cover dies
 *    LOUDLY at boot in the heap's own coverage guard, never a silent
 *    trim. The cover is a power of two (the guard's floor mask) and
 *    the MINIMAL one whose aligned granule holds the board's whole
 *    DATA window -- undersized fails containment, oversized wastes
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
 *    slots and the small rows' sizings follow the drive censuses above.
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
 * guarantees 2M of system memory. The guarantee's arithmetic: native
 * heap 64K + alignment 4K + early mappings 8K + the least managed heap
 * 1536K + its mark bitmap 48K + mark stack 32K + frozen object heap 64K
 * + 64K unallocated = 1820K inside the 2048K floor. Cover 4M holds the
 * whole DATA window in one granule. */
#define MGC_ROW_BOARD             "mps2-an500"
#define MGC_ROW_PROFILE           "suite"
#define MGC_ROW_BASIS             "boot"
#define MGC_ROW_HEAP_MIN_GUARANTEED_BYTES        1572864u
#define MGC_ROW_SYSTEM_MEMORY_UNALLOCATED_BYTES    65536u
#define MGC_ROW_COVER_BYTES       4194304u
#define MGC_ROW_KHEAP_BYTES         65536u
#define MGC_ROW_STACK_DEFAULT       16384u
#define MGC_ROW_STACK_CLAMP         24576u
#define MGC_ROW_HANDLE_SLOTS           64u
#define MGC_ROW_FOH_POOL_BYTES      65536u
#define MGC_ROW_MARKSTACK_ENTRIES    8192u
#define MGC_ROW_FINQ_CAP             2048u
#define MGC_ROW_OOM_RESERVE         65536u
#define MGC_ROW_WINDOW_BYTES         4096u
#define MGC_ROW_WINDOW_FLOOR          256u
#define MGC_ROW_WALL_RETRIES            4u
#define MGC_ROW_FROZEN_SEGS             8u
#elif defined(MGC_HOST_ROW_AN505)
/* QEMU mps2-an505: the an500's ARMv8-M twin by map class. DATA 4M @
 * 0x38000000 (4M-aligned); the same 2M floor and the same row. */
#define MGC_ROW_BOARD             "mps2-an505"
#define MGC_ROW_PROFILE           "suite"
#define MGC_ROW_BASIS             "boot"
#define MGC_ROW_HEAP_MIN_GUARANTEED_BYTES        1572864u
#define MGC_ROW_SYSTEM_MEMORY_UNALLOCATED_BYTES    65536u
#define MGC_ROW_COVER_BYTES       4194304u
#define MGC_ROW_KHEAP_BYTES         65536u
#define MGC_ROW_STACK_DEFAULT       16384u
#define MGC_ROW_STACK_CLAMP         24576u
#define MGC_ROW_HANDLE_SLOTS           64u
#define MGC_ROW_FOH_POOL_BYTES      65536u
#define MGC_ROW_MARKSTACK_ENTRIES    8192u
#define MGC_ROW_FINQ_CAP             2048u
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
#define MGC_ROW_OOM_RESERVE         65536u
#define MGC_ROW_WINDOW_BYTES         4096u
#define MGC_ROW_WINDOW_FLOOR          256u
#define MGC_ROW_WALL_RETRIES            4u
#define MGC_ROW_FROZEN_SEGS             8u
#elif defined(MGC_HOST_ROW_F769)
/* STM32F769I-DISCO: DATA 512K @ 0x20000000 (the DTCM inside DATA,
 * contiguous with SRAM1 and SRAM2), the map's floor of system memory
 * 192K. The small row, and its guarantee's arithmetic exactly at the
 * floor: native heap 16K + alignment 4K + early mappings 8K + the least
 * managed heap 128K + its mark bitmap 4K + mark stack 8K + frozen
 * object heap 16K + 8K unallocated = 192K. The frozen object heap is
 * 16K (about a thousand type objects; the measured use is one). The
 * native-heap carve stays at 16K: the first real hosted program booted
 * to its verdict on carves down to 8K on the QEMU twin. The small
 * sizings scale to the measured high-waters: mark stack 2048 (measured
 * high-water 3), finalization capacity 256 (measured bound 6),
 * out-of-memory reserve 8K (an exception's dispatch allocates small
 * objects; the grace round trip is the protocol either way). Cover 512K
 * is minimal: the granule at 0x20000000 is exactly the DATA window
 * [0x20000000, 0x20080000). */
#define MGC_ROW_BOARD             "st-stm32f769i-disco"
#define MGC_ROW_PROFILE           "mini"
#define MGC_ROW_BASIS             "boot"
#define MGC_ROW_HEAP_MIN_GUARANTEED_BYTES         131072u
#define MGC_ROW_SYSTEM_MEMORY_UNALLOCATED_BYTES     8192u
#define MGC_ROW_COVER_BYTES        524288u
#define MGC_ROW_KHEAP_BYTES         16384u
#define MGC_ROW_STACK_DEFAULT       16384u
#define MGC_ROW_STACK_CLAMP         24576u
#define MGC_ROW_HANDLE_SLOTS           64u
#define MGC_ROW_FOH_POOL_BYTES      16384u
#define MGC_ROW_MARKSTACK_ENTRIES    2048u
#define MGC_ROW_FINQ_CAP              256u
#define MGC_ROW_OOM_RESERVE          8192u
#define MGC_ROW_WINDOW_BYTES         4096u
#define MGC_ROW_WINDOW_FLOOR          256u
#define MGC_ROW_WALL_RETRIES            4u
#define MGC_ROW_FROZEN_SEGS             8u
#elif defined(MGC_HOST_ROW_PICO2)
/* Raspberry Pi Pico 2: DATA 520K @ 0x20000000 (the
 * eight striped SRAM banks and the two small ones after them, one
 * contiguous window), the map's floor of system memory 192K. The small
 * row, its guarantee's arithmetic the STM32F769I-DISCO's: 16K + 4K +
 * 8K + 128K + 4K + 8K + 16K + 8K = 192K, exactly the floor. Cover 1M
 * is minimal: a 512K granule at 0x20000000 ends at 0x20080000, 8K
 * short of the window's end 0x20082000; the 1M granule holds it. */
#define MGC_ROW_BOARD             "rpi-pico2"
#define MGC_ROW_PROFILE           "mini"
#define MGC_ROW_BASIS             "boot"
#define MGC_ROW_HEAP_MIN_GUARANTEED_BYTES         131072u
#define MGC_ROW_SYSTEM_MEMORY_UNALLOCATED_BYTES     8192u
#define MGC_ROW_COVER_BYTES       1048576u
#define MGC_ROW_KHEAP_BYTES         16384u
#define MGC_ROW_STACK_DEFAULT       16384u
#define MGC_ROW_STACK_CLAMP         24576u
#define MGC_ROW_HANDLE_SLOTS           64u
#define MGC_ROW_FOH_POOL_BYTES      16384u
#define MGC_ROW_MARKSTACK_ENTRIES    2048u
#define MGC_ROW_FINQ_CAP              256u
#define MGC_ROW_OOM_RESERVE          8192u
#define MGC_ROW_WINDOW_BYTES         4096u
#define MGC_ROW_WINDOW_FLOOR          256u
#define MGC_ROW_WALL_RETRIES            4u
#define MGC_ROW_FROZEN_SEGS             8u
#elif defined(MGC_HOST_ROW_PICO2W)
/* Raspberry Pi Pico 2 W: the Pico 2 with a wireless module fitted, whose
 * lines the kernel never touches -- the same chip, the same 520K window
 * and map floor, so the same small row and the same 1M cover. */
#define MGC_ROW_BOARD             "rpi-pico2-w"
#define MGC_ROW_PROFILE           "mini"
#define MGC_ROW_BASIS             "boot"
#define MGC_ROW_HEAP_MIN_GUARANTEED_BYTES         131072u
#define MGC_ROW_SYSTEM_MEMORY_UNALLOCATED_BYTES     8192u
#define MGC_ROW_COVER_BYTES       1048576u
#define MGC_ROW_KHEAP_BYTES         16384u
#define MGC_ROW_STACK_DEFAULT       16384u
#define MGC_ROW_STACK_CLAMP         24576u
#define MGC_ROW_HANDLE_SLOTS           64u
#define MGC_ROW_FOH_POOL_BYTES      16384u
#define MGC_ROW_MARKSTACK_ENTRIES    2048u
#define MGC_ROW_FINQ_CAP              256u
#define MGC_ROW_OOM_RESERVE          8192u
#define MGC_ROW_WINDOW_BYTES         4096u
#define MGC_ROW_WINDOW_FLOOR          256u
#define MGC_ROW_WALL_RETRIES            4u
#define MGC_ROW_FROZEN_SEGS             8u
#elif defined(MGC_HOST_ROW_PICOPLUS2)
/* Pimoroni Pico Plus 2: the same chip, the same 520K SRAM
 * window and map floor as the Pico 2, so the same small row and the same
 * 1M cover; the board's PSRAM is not part of the map. */
#define MGC_ROW_BOARD             "pimoroni-pico-plus-2"
#define MGC_ROW_PROFILE           "mini"
#define MGC_ROW_BASIS             "boot"
#define MGC_ROW_HEAP_MIN_GUARANTEED_BYTES         131072u
#define MGC_ROW_SYSTEM_MEMORY_UNALLOCATED_BYTES     8192u
#define MGC_ROW_COVER_BYTES       1048576u
#define MGC_ROW_KHEAP_BYTES         16384u
#define MGC_ROW_STACK_DEFAULT       16384u
#define MGC_ROW_STACK_CLAMP         24576u
#define MGC_ROW_HANDLE_SLOTS           64u
#define MGC_ROW_FOH_POOL_BYTES      16384u
#define MGC_ROW_MARKSTACK_ENTRIES    2048u
#define MGC_ROW_FINQ_CAP              256u
#define MGC_ROW_OOM_RESERVE          8192u
#define MGC_ROW_WINDOW_BYTES         4096u
#define MGC_ROW_WINDOW_FLOOR          256u
#define MGC_ROW_WALL_RETRIES            4u
#define MGC_ROW_FROZEN_SEGS             8u
#elif defined(MGC_HOST_ROW_PICOPLUS2W)
/* Pimoroni Pico Plus 2 W: the Pico Plus 2 with a wireless module fitted,
 * whose lines are the wifi driver's -- the same chip, the same 520K
 * window and map floor, so the same small row and the same 1M cover; the
 * board's PSRAM is not part of the map. */
#define MGC_ROW_BOARD             "pimoroni-pico-plus-2-w"
#define MGC_ROW_PROFILE           "mini"
#define MGC_ROW_BASIS             "boot"
#define MGC_ROW_HEAP_MIN_GUARANTEED_BYTES         131072u
#define MGC_ROW_SYSTEM_MEMORY_UNALLOCATED_BYTES     8192u
#define MGC_ROW_COVER_BYTES       1048576u
#define MGC_ROW_KHEAP_BYTES         16384u
#define MGC_ROW_STACK_DEFAULT       16384u
#define MGC_ROW_STACK_CLAMP         24576u
#define MGC_ROW_HANDLE_SLOTS           64u
#define MGC_ROW_FOH_POOL_BYTES      16384u
#define MGC_ROW_MARKSTACK_ENTRIES    2048u
#define MGC_ROW_FINQ_CAP              256u
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
static_assert(MGC_ROW_WINDOW_FLOOR < MGC_ROW_WINDOW_BYTES,
              "the window floor is a fraction of the window");
static_assert(MGC_ROW_STACK_DEFAULT <= MGC_ROW_STACK_CLAMP,
              "the default hosted stack respects the clamp");
static_assert((MGC_ROW_FOH_POOL_BYTES & 4095u) == 0u,
              "the frozen object heap is whole pages (the mmap bridge's draw granule)");
static_assert((MGC_ROW_SYSTEM_MEMORY_UNALLOCATED_BYTES & 4095u) == 0u,
              "the unallocated system memory is whole pages");

#endif /* MGC_BOARD_ROWS_H */
