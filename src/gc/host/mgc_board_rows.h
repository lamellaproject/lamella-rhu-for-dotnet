/* Per-board payload-tier GC configuration rows.
 *
 * One row per supported board, selected by exactly one
 * MGC_HOST_ROW_<BOARD> define at compile time. A row is DECLARATION,
 * not machinery: the payload-tier fields (region, card cover, kheap
 * carve, hosted-thread stack default/clamp, handle slots, FOH pool)
 * plus the collector's internal sizings. Every value is either
 * measured on the gate drives (handle live high-water 8 of 64;
 * window outstanding high-water 2; mark-stack high-water 3 of 8192;
 * the three arena draws) or derived from the board's own link-map
 * geometry (the DATA window, the map's guaranteed arena floor, the
 * board module's kheap carve).
 *
 * The gate parses ALL rows below (as text, unselected blocks
 * included) and checks each against its payload map and board
 * module; the selected row is additionally asserted in the artifact:
 * the row census prints from these constants, the containment
 * verdict proves the statics window and the whole arena window sit
 * inside the cover granule, and the window-fit verdict proves the
 * drawn region + aux + the declared kheap fit the link-time window.
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
 *  - stack_default/stack_clamp and foh_pool are declared here and
 *    consumed at the shim tier. The FOH pool is the row's frozen-
 *    object budget: an arena draw at the mmap bridge's page granule,
 *    reserved for the runtime's frozen-object segment and guarded at
 *    commit (a commit past the pool fails into a managed allocation
 *    failure, never into arena memory the runtime does not own). The
 *    pool is a per-row value inside the floor inequality, never a
 *    uniform field. Handle slots and the mini-tier sizings follow
 *    the drive censuses above.
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
 * guarantees a 2M arena floor and the region IS that floor (basis:
 * link -- the fit is proven in-artifact by the window-fit verdict,
 * the QEMU image's statics being small). Cover 4M holds the whole
 * DATA window in one granule. */
#define MGC_ROW_BOARD             "mps2-an500"
#define MGC_ROW_PROFILE           "suite"
#define MGC_ROW_BASIS             "link"
#define MGC_ROW_REGION_BYTES      2097152u
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
 * 0x38000000 (4M-aligned); region == the 2M floor (basis: link). */
#define MGC_ROW_BOARD             "mps2-an505"
#define MGC_ROW_PROFILE           "suite"
#define MGC_ROW_BASIS             "link"
#define MGC_ROW_REGION_BYTES      2097152u
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
/* NUCLEO-U5A5ZJ-Q: contiguous SRAM 2496K @ 0x20000000, map floor
 * 1536K. Basis: floor -- kheap 64K + region 1024K + bitmap 32K +
 * mark stack 32K + foh 64K = 1216K inside the floor for ANY payload
 * statics size (slack 320K; the roomy row is deliberately
 * conservative). Cover 4M is minimal: a 2M granule at 0x20000000
 * ends below the SRAM end 0x20270000. */
#define MGC_ROW_BOARD             "nucleo-u5a5zj-q"
#define MGC_ROW_PROFILE           "suite"
#define MGC_ROW_BASIS             "floor"
#define MGC_ROW_REGION_BYTES      1048576u
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
 * contiguous with SRAM1 and SRAM2), map floor 192K. The mini row:
 * kheap 16K + region 96K + bitmap 3K + mark stack 8K + foh 64K = 187K
 * inside the floor (slack 5K). The FOH pool is one stock commit unit:
 * the runtime's frozen-object reservation is served from it and its
 * first commit lands whole; the commit guard refuses a second commit
 * into a managed allocation failure. The kernel-heap carve stays at
 * 16K: the first real hosted program booted to its verdict on carves
 * down to 8K on the QEMU twin. Mini sizings scale to the measured
 * high-waters: mark stack 2048 (measured hw 3), finq 256 (measured
 * bound 6), reserve 8K (OOM dispatch allocations are small-object
 * class; the grace round trip is the protocol either way). Cover 512K
 * is minimal: the granule at 0x20000000 is exactly the DATA window
 * [0x20000000, 0x20080000). */
#define MGC_ROW_BOARD             "stm32f769i-disco"
#define MGC_ROW_PROFILE           "mini"
#define MGC_ROW_BASIS             "floor"
#define MGC_ROW_REGION_BYTES        98304u
#define MGC_ROW_COVER_BYTES        524288u
#define MGC_ROW_KHEAP_BYTES         16384u
#define MGC_ROW_STACK_DEFAULT       16384u
#define MGC_ROW_STACK_CLAMP         24576u
#define MGC_ROW_HANDLE_SLOTS           64u
#define MGC_ROW_FOH_POOL_BYTES      65536u
#define MGC_ROW_MARKSTACK_ENTRIES    2048u
#define MGC_ROW_FINQ_CAP              256u
#define MGC_ROW_OOM_RESERVE          8192u
#define MGC_ROW_WINDOW_BYTES         4096u
#define MGC_ROW_WINDOW_FLOOR          256u
#define MGC_ROW_WALL_RETRIES            4u
#define MGC_ROW_FROZEN_SEGS             8u
#elif defined(MGC_HOST_ROW_PICO2)
/* Raspberry Pi Pico 2 (and Pico 2 W): DATA 520K @ 0x20000000 (the
 * eight striped SRAM banks and the two small ones after them, one
 * contiguous window), map floor 192K. The mini row: kheap 16K +
 * region 96K + bitmap 3K + mark stack 8K + foh 64K = 187K inside the
 * floor (slack 5K). Cover 1M is minimal: a 512K granule at 0x20000000
 * ends at 0x20080000, 8K short of the window's end 0x20082000; the 1M
 * granule holds it. */
#define MGC_ROW_BOARD             "pico-2"
#define MGC_ROW_PROFILE           "mini"
#define MGC_ROW_BASIS             "floor"
#define MGC_ROW_REGION_BYTES        98304u
#define MGC_ROW_COVER_BYTES       1048576u
#define MGC_ROW_KHEAP_BYTES         16384u
#define MGC_ROW_STACK_DEFAULT       16384u
#define MGC_ROW_STACK_CLAMP         24576u
#define MGC_ROW_HANDLE_SLOTS           64u
#define MGC_ROW_FOH_POOL_BYTES      65536u
#define MGC_ROW_MARKSTACK_ENTRIES    2048u
#define MGC_ROW_FINQ_CAP              256u
#define MGC_ROW_OOM_RESERVE          8192u
#define MGC_ROW_WINDOW_BYTES         4096u
#define MGC_ROW_WINDOW_FLOOR          256u
#define MGC_ROW_WALL_RETRIES            4u
#define MGC_ROW_FROZEN_SEGS             8u
#elif defined(MGC_HOST_ROW_PICOPLUS2)
/* Pimoroni Pico Plus 2 (and Plus 2 W): the same chip, the same 520K SRAM
 * window and map floor as the Pico 2, so the same mini row and the same
 * 1M cover; the board's PSRAM is not part of the map. */
#define MGC_ROW_BOARD             "pico-plus-2"
#define MGC_ROW_PROFILE           "mini"
#define MGC_ROW_BASIS             "floor"
#define MGC_ROW_REGION_BYTES        98304u
#define MGC_ROW_COVER_BYTES       1048576u
#define MGC_ROW_KHEAP_BYTES         16384u
#define MGC_ROW_STACK_DEFAULT       16384u
#define MGC_ROW_STACK_CLAMP         24576u
#define MGC_ROW_HANDLE_SLOTS           64u
#define MGC_ROW_FOH_POOL_BYTES      65536u
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

/* Row self-consistency (external knob ties are proven on the wire:
 * the cover line's buffer total, the census region size, the handle
 * census's slot count). */
static_assert((MGC_ROW_COVER_BYTES & (MGC_ROW_COVER_BYTES - 1u)) == 0u,
              "the cover is a power of two (the coverage guard's floor mask)");
static_assert(MGC_ROW_OOM_RESERVE < MGC_ROW_REGION_BYTES,
              "the OOM reserve lives inside the region");
static_assert(MGC_ROW_WINDOW_FLOOR < MGC_ROW_WINDOW_BYTES,
              "the window floor is a fraction of the window");
static_assert(MGC_ROW_STACK_DEFAULT <= MGC_ROW_STACK_CLAMP,
              "the default hosted stack respects the clamp");
static_assert((MGC_ROW_FOH_POOL_BYTES & 4095u) == 0u,
              "the FOH pool is whole pages (the mmap bridge's draw granule)");

#endif /* MGC_BOARD_ROWS_H */
