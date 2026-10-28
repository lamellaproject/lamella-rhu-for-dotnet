# goldens/gc -- the gc gate's frozen world objects (MANIFEST)

Copyright (c) 2026 Lamella LLC.
SPDX-License-Identifier: MIT OR Apache-2.0

The gc gate's C++ test images link from the objects listed below. Every
object is the bytes of one recorded compile. Before its first case the
gate checks every file here against this manifest, and this manifest
against the digest kept in the harness, and refuses a mismatch: a frozen
file that differs from its record is wrong, and the gate never rebuilds
it. Nothing here is compiled at gate time; the gate needs Rust and QEMU
alone.

The leg `rhu-gate gc rederive` recompiles every object below from src/gc
under the gate's flag set (CXX_FLAGS in src/harness/src/gates/gc.rs, with
the toolchain's C-library headers as the sysroot the resolver appends)
plus the defines its record names, and compares each result with the file
byte for byte; the README's re-derivation section names what the leg
needs.

Compiler: clang version 22.1.8 (the LLVM project's 22.1.8 release, the
version the runtime archives are built with, so one LLVM lineage spells
the 32-bit types and mangles the interface names the same way on both
sides of the seam), targeting arm-none-eabi for the Cortex-M7 with the
hard-float FPv5-D16, and for the mps2-an505 row's three objects, the
st-nucleo-u5a5zj-q row's heap object and the RP2350 rows' heap and collect
objects the Cortex-M33 with the single-precision FPv5 under the same flag
set but the core and FPU words. No C++ standard library header is used:
the collector is freestanding C++ over the toolchain's C headers.

The objects are compiled against the .NET 11 RC2 runtime
(v11.0.0-rc.2.26504.105): the ten runtime-to-GC interface headers vendored
under src/vendor at that tag (the GC interface 5.9, the EE interface's
major version 5, the flat surface the gate ties three ways at 53 names).

The mps2-an505 row's three collector objects are the an500 row's sources
and defines compiled for the Cortex-M33 with the single-precision FPv5
(the class flag set beside the Cortex-M7's in gc.rs; the collector reads
no board row, so the class is the one variable), each carrying no
double-precision instruction. The st-nucleo-u5a5zj-q row's heap object is
that source under the least managed heap the row guarantees, as the row
header declares it; the row shares the mps2-an505 row's skeleton and
collect objects, which no differing field reaches. The four RP2350 rows
(the Raspberry Pi Pico 2 and Pico 2 W, the Pimoroni Pico Plus 2 and Pico
Plus 2 W) declare one mini row each with the same fields but for the
cover and the finalization table, so their collect object is frozen
once under the Pico 2's declaration, named for the chip, and the four
rows name it; the heap object is frozen under the Pico 2's declaration
for the two Raspberry Pi rows (the table 256 entries) and under the
Pimoroni Pico Plus 2's for the pair, whose cover is the PSRAM window's
granule and whose table holds 2048 entries; the skeleton the mps2-an505
row's. The rederive leg ties every board-table row's objects to the row's
own declaration by bytes.

The no-GC region's entry is refused through the pinned status (no budget
and no suppression state exist, so nothing is claimed) and its end answers
that no region is in progress, with a negative control built from the
skeleton with the claim restored and a host drive that reads both
statuses. The collector's mark stack, when full, defers the push and
rescans the marked objects inside the window the deferred objects span,
round by round (the mark-overflow case's collector object carries a
64-entry stack; its control has the recovery compiled out). The windows
skeleton's handle pool grows by segments of 256 slots from the shim's mmap
past the row's 64 and answers no handle when the mmap refuses (the growth
control's skeleton has the growth compiled out; the two drive hosts
exercise both).

The row heap objects size the managed heap at boot: each takes all the
system memory free at the collector's start, less the collector tables for
that size and less what its row keeps for later draws (the frozen object
heap and the unallocated amount), and refuses a boot that would leave less
than the least the row guarantees, so a row object carries what its row
keeps and that least in place of a size (two control objects of the an500
row take one page less and one page more than the rule gives). A request
larger than the managed heap is answered no memory before the fit
arithmetic. The sweep zeroes a dead run at the top of the heap and sets
the bump mark back over it. A finalizable request takes its slot in the
finalization table before its memory, below a bound that holds a few
slots back for an out-of-memory exception's own finalizable allocation
(16 on every row), and a table at its bound means one collection, a
bounded wait for the finalizer thread, then a refusal through the
out-of-memory report, whose grace makes the slots held back usable. A
finalizable or 8-byte-aligned small request that fits the thread's
allocation window is served inside it. The windows skeleton asks, page by
page, how much of a frozen segment's commit is backed and stops the
program when a segment backed for less than its commit has no room for one
more object. Each of these five has a drive host and a control object
built with the behaviour compiled out, the windows drive's tail has a
second control object built with the window never kept, and the
finalization drive a second built with no slot held back.

The collector's own lines (the skeleton's, the heap's, the probe's and the
collector's) go out through the kernel's log door at the verbose level by
one formatter per file that reads the image's level once and formats
nothing it would drop, and a fail-fast's last words at the silent level;
no C-library stream write is left in the three files. The gate images'
activation host provides vsnprintf beside printf, puts and fflush on its
own enumerated formatter, so no C library enters them. Every heap and
collect object draws the managed heap's region, its mark bitmap and its
mark stack through the shim's collector mapping entry from the window the
managed heap lives in -- the external RAM's window where the image's
rehydrated type data went, the chip's own SRAM otherwise -- and the host
objects provide the entry with the shim's window rule; the three row
covers of the rows with an external window are that window's granule.

62 objects.

Records (repo-relative path, sha256; defines documented per record):

- `goldens/gc/skel-bare.o`
  `0a36f79de87fd9b8e65f67c54212a9f1bb581cc8cd1076e03b1a29411323520b`
  (src/gc/mgc_skeleton.cpp; no extra defines)
- `goldens/gc/skel-bad.o`
  `9c6d437f5f454538958025df028a6d30c8ec79d8ab2a52502ccae5b4e9fba1c4`
  (src/gc/mgc_skeleton.cpp; -DMGC_SKEL_BAD_VERSION)
- `goldens/gc/host.o`
  `9c31929257465cf0355e0a7831b8987e6aa7d0d3c8188440543a142b64c12d2c`
  (src/gc/host/mgc_host.cpp; no extra defines)
- `goldens/gc/probe.o`
  `cc9a74e2da19dca3aaab850361cf52b03c46474c58d654411dda87f85b29a958`
  (src/gc/mgc_ee_probe.cpp; no extra defines)
- `goldens/gc/skel-heap.o`
  `c2d9bb814b966f692f952b480fcd3b14671c29a4dc0893280f75f5bc72aeb3c5`
  (src/gc/mgc_skeleton.cpp; -DMGC_HEAP_LIVE)
- `goldens/gc/heap.o`
  `46009bcd838ab6d0ecf74b94928a2aa4df2b2e51bc3a8eb1c447f046873f06a7`
  (src/gc/mgc_heap.cpp; -DMGC_HEAP_LIVE -DMGC_HEAP_BYTES=2097152u -DMGC_FINQ_RESERVE=16u)
- `goldens/gc/host-drive.o`
  `c2e26dd9c831844032f9e09cc7ebaf77f7fbceb0f9bd4ca9ce59976fc4b4aa15`
  (src/gc/host/mgc_host.cpp; -DMGC_HOST_DRIVE_ALLOC)
- `goldens/gc/skel-collect.o`
  `1921c54ffdfaea6a124b364aa6a1287dd7aee402af73d534d38c17edd5349562`
  (src/gc/mgc_skeleton.cpp; -DMGC_HEAP_LIVE -DMGC_COLLECT_LIVE)
- `goldens/gc/heap-collect.o`
  `e0db499bf1f0fbcd0d22384a782a5d086c9f71d52aba19fb1254fe9ad9fdc62f`
  (src/gc/mgc_heap.cpp; -DMGC_HEAP_LIVE -DMGC_COLLECT_LIVE -DMGC_HEAP_BYTES=2097152u -DMGC_FINQ_RESERVE=16u)
- `goldens/gc/collect.o`
  `6f1ed9c27f631730da88748aeed2dbe5200f3119c5efd0ca094f26d3300b6d4c`
  (src/gc/mgc_collect.cpp; -DMGC_HEAP_LIVE -DMGC_COLLECT_LIVE)
- `goldens/gc/collect-break.o`
  `4fd284bc9fa865e933e93509b86be85895591de6565ab384b2683eca1f7419c2`
  (src/gc/mgc_collect.cpp; -DMGC_HEAP_LIVE -DMGC_COLLECT_LIVE -DMGC_BREAK_ROOTS)
- `goldens/gc/host-collect.o`
  `72f5f43adda5cfe0cf58f124065656442c5087306e588312a91c1d385b9b9a61`
  (src/gc/host/mgc_host.cpp; -DMGC_HOST_DRIVE_ALLOC -DMGC_HOST_DRIVE_COLLECT)
- `goldens/gc/skel-sweep.o`
  `85eb33f0a60a4b63ca36b514cd90b6817e4085a9cfbe9cbf3b9eecbb8d4973af`
  (src/gc/mgc_skeleton.cpp; -DMGC_HEAP_LIVE -DMGC_COLLECT_LIVE -DMGC_SWEEP_LIVE)
- `goldens/gc/heap-sweep.o`
  `fd08d7304cadb445c3123c6c573b71515dae1bf779d426b534792116d37fc053`
  (src/gc/mgc_heap.cpp; -DMGC_HEAP_LIVE -DMGC_COLLECT_LIVE -DMGC_SWEEP_LIVE -DMGC_HEAP_BYTES=2097152u -DMGC_FINQ_RESERVE=16u)
- `goldens/gc/collect-sweep.o`
  `a4796cb676e7ec351490f29a0e8dd349dfcf390818868f66494dc2d5cf48f33d`
  (src/gc/mgc_collect.cpp; -DMGC_HEAP_LIVE -DMGC_COLLECT_LIVE -DMGC_SWEEP_LIVE)
- `goldens/gc/collect-sweep-break.o`
  `6895aba57b169eed5aef8a0991957016b16cc8cf27c2b3f52730eae5e6e0cbff`
  (src/gc/mgc_collect.cpp; -DMGC_HEAP_LIVE -DMGC_COLLECT_LIVE -DMGC_SWEEP_LIVE -DMGC_BREAK_WEAK)
- `goldens/gc/host-sweep.o`
  `a3131a6b5399dc006d327a6f26e77f07b4e57b830e50692adf19e8bc55c3f0f9`
  (src/gc/host/mgc_host.cpp; -DMGC_HOST_DRIVE_ALLOC -DMGC_HOST_DRIVE_COLLECT -DMGC_HOST_DRIVE_SWEEP)
- `goldens/gc/skel-tune.o`
  `8ad642990063d4d2d02538cc87fa43860dbedc2c0f8a5e77bcd12ca8161905fd`
  (src/gc/mgc_skeleton.cpp; -DMGC_HEAP_LIVE -DMGC_COLLECT_LIVE -DMGC_SWEEP_LIVE -DMGC_TUNE_LIVE -DMGC_HANDLE_SLOTS=64)
- `goldens/gc/heap-tune.o`
  `aa959ff1b88dac679f09c36e90a0e47348e411201de97dea0918814804918cef`
  (src/gc/mgc_heap.cpp; -DMGC_HEAP_LIVE -DMGC_COLLECT_LIVE -DMGC_SWEEP_LIVE -DMGC_TUNE_LIVE -DMGC_HEAP_BYTES=2097152u -DMGC_FINQ_RESERVE=16u)
- `goldens/gc/heap-tune-break.o`
  `27c3ba93811733af1c403f636a75c5f4df47b258965332f42720f9ee8741e502`
  (src/gc/mgc_heap.cpp; -DMGC_HEAP_LIVE -DMGC_COLLECT_LIVE -DMGC_SWEEP_LIVE -DMGC_TUNE_LIVE -DMGC_HEAP_BYTES=2097152u -DMGC_FINQ_RESERVE=16u -DMGC_BREAK_WINDOW_FIX)
- `goldens/gc/collect-tune.o`
  `65d68d5a7c83dd7bfe3f514dc461e14c7f8c30f06fbfc80a00a17be73ac3fec6`
  (src/gc/mgc_collect.cpp; -DMGC_HEAP_LIVE -DMGC_COLLECT_LIVE -DMGC_SWEEP_LIVE -DMGC_TUNE_LIVE)
- `goldens/gc/host-tune.o`
  `5a2924350d3b4a9cef60b04e0565f01ef78817ecf96f5f6a74093460a6d3f846`
  (src/gc/host/mgc_host.cpp; -DMGC_HOST_DRIVE_ALLOC -DMGC_HOST_DRIVE_COLLECT -DMGC_HOST_DRIVE_SWEEP -DMGC_HOST_DRIVE_TUNE)
- `goldens/gc/skel-row.o`
  `8ad642990063d4d2d02538cc87fa43860dbedc2c0f8a5e77bcd12ca8161905fd`
  (src/gc/mgc_skeleton.cpp; -DMGC_HEAP_LIVE -DMGC_COLLECT_LIVE -DMGC_SWEEP_LIVE -DMGC_TUNE_LIVE -DMGC_HANDLE_SLOTS=64)
- `goldens/gc/heap-row.o`
  `e3f970ef4e9379a075414a5b51f9f9e4a610651e14cdc787aaa28b6c80dfde5e`
  (src/gc/mgc_heap.cpp; -DMGC_HEAP_LIVE -DMGC_COLLECT_LIVE -DMGC_SWEEP_LIVE -DMGC_TUNE_LIVE -DMGC_HEAP_AT_BOOT -DMGC_HEAP_KEEP_BYTES=131072u -DMGC_HEAP_MIN_GUARANTEED_BYTES=1572864u -DMGC_ARENA_COVER_BYTES=16777216u -DMGC_FINQ_RESERVE=16u)
- `goldens/gc/heap-row-break-cover.o`
  `f7e1338001542f7bbb6d9b5988f4127d93ccf866b18f6c9521b5355925b42b7b`
  (src/gc/mgc_heap.cpp; -DMGC_HEAP_LIVE -DMGC_COLLECT_LIVE -DMGC_SWEEP_LIVE -DMGC_TUNE_LIVE -DMGC_HEAP_AT_BOOT -DMGC_HEAP_KEEP_BYTES=131072u -DMGC_HEAP_MIN_GUARANTEED_BYTES=1572864u -DMGC_ARENA_COVER_BYTES=2097152u -DMGC_FINQ_RESERVE=16u)
- `goldens/gc/collect-row.o`
  `65d68d5a7c83dd7bfe3f514dc461e14c7f8c30f06fbfc80a00a17be73ac3fec6`
  (src/gc/mgc_collect.cpp; -DMGC_HEAP_LIVE -DMGC_COLLECT_LIVE -DMGC_SWEEP_LIVE -DMGC_TUNE_LIVE)
- `goldens/gc/host-row.o`
  `ad9847ad1d669b8eefd57c7f6f3d2fa31a978cc5a9ad21f84de929bdbfdfed08`
  (src/gc/host/mgc_host.cpp; -DMGC_HOST_DRIVE_ALLOC -DMGC_HOST_DRIVE_COLLECT -DMGC_HOST_DRIVE_SWEEP -DMGC_HOST_DRIVE_TUNE -DMGC_HOST_ROW_AN500)
- `goldens/gc/heap-row-f769.o`
  `77c7904ca5235901ae4282a6e5a1500b79b781037b174831c26538c018a0698c`
  (src/gc/mgc_heap.cpp; -DMGC_HEAP_LIVE -DMGC_COLLECT_LIVE -DMGC_SWEEP_LIVE -DMGC_TUNE_LIVE -DMGC_HEAP_AT_BOOT -DMGC_HEAP_KEEP_BYTES=24576u -DMGC_HEAP_MIN_GUARANTEED_BYTES=98304u -DMGC_ARENA_COVER_BYTES=16777216u -DMGC_FINQ_CAP=2048u -DMGC_FINQ_RESERVE=16u -DMGC_OOM_RESERVE=8192u -DMGC_WINDOW_BYTES=4096u -DMGC_WINDOW_FLOOR=256u -DMGC_WALL_RETRIES=4u)
- `goldens/gc/collect-row-f769.o`
  `9ac09e90bee9a7023c4263e7686615ac67203711838e828f041b5bc99deb1f88`
  (src/gc/mgc_collect.cpp; -DMGC_HEAP_LIVE -DMGC_COLLECT_LIVE -DMGC_SWEEP_LIVE -DMGC_TUNE_LIVE -DMGC_MARK_STACK_ENTRIES=2048u -DMGC_OOM_RESERVE=8192u)
- `goldens/gc/host-freepad.o`
  `27a7b34fa55f502caeddd0ade94e3e7d1bf236d602698cb2f764d754f1b78251`
  (src/gc/host/mgc_host.cpp; -DMGC_HOST_DRIVE_ALLOC -DMGC_HOST_DRIVE_COLLECT -DMGC_HOST_DRIVE_SWEEP -DMGC_HOST_DRIVE_FREEPAD)
- `goldens/gc/heap-sweep-break-freepad.o`
  `04dee764ce6c2049772fa63d49ea29ae6d438a239ae438ee741a6ec479f14098`
  (src/gc/mgc_heap.cpp; -DMGC_HEAP_LIVE -DMGC_COLLECT_LIVE -DMGC_SWEEP_LIVE -DMGC_HEAP_BYTES=2097152u -DMGC_FINQ_RESERVE=16u -DMGC_BREAK_FREE_PAD)
- `goldens/gc/host-handles.o`
  `88dea22433a7e784f80b5061a9866692d8be0a817c9b9116a866a7a2f0c0d0b5`
  (src/gc/host/mgc_host.cpp; -DMGC_HOST_DRIVE_ALLOC -DMGC_HOST_DRIVE_COLLECT -DMGC_HOST_DRIVE_SWEEP -DMGC_HOST_DRIVE_TUNE -DMGC_HOST_DRIVE_HANDLES -DMGC_HANDLES_ITER=100000u)
- `goldens/gc/skel-tune-break-lock.o`
  `5170031d2b9669d1db0081ddf836b6bccdd0725b3fa5698c3cf962b7b1d0cb16`
  (src/gc/mgc_skeleton.cpp; -DMGC_HEAP_LIVE -DMGC_COLLECT_LIVE -DMGC_SWEEP_LIVE -DMGC_TUNE_LIVE -DMGC_HANDLE_SLOTS=64 -DMGC_BREAK_HANDLE_LOCK)
- `goldens/gc/host-nogc.o`
  `db9cfa17ec2db845eb54ee171a0e7bd6c03ab8b95ffa728cf539b53385123484`
  (src/gc/host/mgc_host.cpp; -DMGC_HOST_DRIVE_ALLOC -DMGC_HOST_DRIVE_COLLECT -DMGC_HOST_DRIVE_SWEEP -DMGC_HOST_DRIVE_TUNE -DMGC_HOST_DRIVE_NOGC)
- `goldens/gc/skel-tune-break-nogc.o`
  `857938195acc295f9e9640c20dc9d4f0b285f0b2c196b638456ba7771fae3e82`
  (src/gc/mgc_skeleton.cpp; -DMGC_HEAP_LIVE -DMGC_COLLECT_LIVE -DMGC_SWEEP_LIVE -DMGC_TUNE_LIVE -DMGC_HANDLE_SLOTS=64 -DMGC_BREAK_NOGC_CLAIM)
- `goldens/gc/skel-row-an505.o`
  `9f852dd291fef6f4b3425ecb243700b73278a8384b5cf1b856802ec005c7911c`
  (src/gc/mgc_skeleton.cpp; -DMGC_HEAP_LIVE -DMGC_COLLECT_LIVE -DMGC_SWEEP_LIVE -DMGC_TUNE_LIVE -DMGC_HANDLE_SLOTS=64; the Cortex-M33 class flag set)
- `goldens/gc/heap-row-an505.o`
  `10bdd4add4c1882540bb32cb8f9fad29ee66f5a2ac4e5f95d760184e30e1e99f`
  (src/gc/mgc_heap.cpp; -DMGC_HEAP_LIVE -DMGC_COLLECT_LIVE -DMGC_SWEEP_LIVE -DMGC_TUNE_LIVE -DMGC_HEAP_AT_BOOT -DMGC_HEAP_KEEP_BYTES=131072u -DMGC_HEAP_MIN_GUARANTEED_BYTES=1572864u -DMGC_ARENA_COVER_BYTES=16777216u -DMGC_FINQ_RESERVE=16u; the Cortex-M33 class flag set)
- `goldens/gc/collect-row-an505.o`
  `c3399d3288b241bfc5d703477b2afd691587b01460101e0a8de6187ed61c77bf`
  (src/gc/mgc_collect.cpp; -DMGC_HEAP_LIVE -DMGC_COLLECT_LIVE -DMGC_SWEEP_LIVE -DMGC_TUNE_LIVE; the Cortex-M33 class flag set)
- `goldens/gc/heap-row-u5a5.o`
  `f3cab6e5ed14a5476203e9c52cee3eeebc440b554b722ba419816bc593e3ab69`
  (src/gc/mgc_heap.cpp; -DMGC_HEAP_LIVE -DMGC_COLLECT_LIVE -DMGC_SWEEP_LIVE -DMGC_TUNE_LIVE -DMGC_HEAP_AT_BOOT -DMGC_HEAP_KEEP_BYTES=131072u -DMGC_HEAP_MIN_GUARANTEED_BYTES=1048576u -DMGC_ARENA_COVER_BYTES=4194304u -DMGC_FINQ_RESERVE=16u; the Cortex-M33 class flag set)
- `goldens/gc/heap-row-rp2350.o`
  `a17837562d409c785285dd2f01951e8919b9ec04f89aafb30a8309c2a94fa632`
  (src/gc/mgc_heap.cpp; -DMGC_HEAP_LIVE -DMGC_COLLECT_LIVE -DMGC_SWEEP_LIVE -DMGC_TUNE_LIVE -DMGC_HEAP_AT_BOOT -DMGC_HEAP_KEEP_BYTES=24576u -DMGC_HEAP_MIN_GUARANTEED_BYTES=98304u -DMGC_ARENA_COVER_BYTES=1048576u -DMGC_FINQ_CAP=256u -DMGC_FINQ_RESERVE=16u -DMGC_OOM_RESERVE=8192u -DMGC_WINDOW_BYTES=4096u -DMGC_WINDOW_FLOOR=256u -DMGC_WALL_RETRIES=4u; the Cortex-M33 class flag set)
- `goldens/gc/collect-row-rp2350.o`
  `12d7a3655e1d2df6ecbcd462851cd92341a76c0780d75803c82a3fda7854e81c`
  (src/gc/mgc_collect.cpp; -DMGC_HEAP_LIVE -DMGC_COLLECT_LIVE -DMGC_SWEEP_LIVE -DMGC_TUNE_LIVE -DMGC_MARK_STACK_ENTRIES=2048u -DMGC_OOM_RESERVE=8192u; the Cortex-M33 class flag set)
- `goldens/gc/heap-row-picoplus2.o`
  `c4d39d88ef023ef22dd82dc186bf9c37f657e67eab0d385920fad1e32a29333e`
  (src/gc/mgc_heap.cpp; -DMGC_HEAP_LIVE -DMGC_COLLECT_LIVE -DMGC_SWEEP_LIVE -DMGC_TUNE_LIVE -DMGC_HEAP_AT_BOOT -DMGC_HEAP_KEEP_BYTES=24576u -DMGC_HEAP_MIN_GUARANTEED_BYTES=98304u -DMGC_ARENA_COVER_BYTES=8388608u -DMGC_FINQ_CAP=2048u -DMGC_FINQ_RESERVE=16u -DMGC_OOM_RESERVE=8192u -DMGC_WINDOW_BYTES=4096u -DMGC_WINDOW_FLOOR=256u -DMGC_WALL_RETRIES=4u; the Cortex-M33 class flag set)
- `goldens/gc/collect-tune-stk64.o`
  `d36875b8e4ed9476e9a4f65003d146aadc340d0b14433bbd39291e8e27d9e9b5`
  (src/gc/mgc_collect.cpp; -DMGC_HEAP_LIVE -DMGC_COLLECT_LIVE -DMGC_SWEEP_LIVE -DMGC_TUNE_LIVE -DMGC_MARK_STACK_ENTRIES=64u)
- `goldens/gc/collect-tune-stk64-break.o`
  `0f8095cc717ed56fa24405d6a7d6879aa98f324874d9e5e19ab3cac4f9e91890`
  (src/gc/mgc_collect.cpp; -DMGC_HEAP_LIVE -DMGC_COLLECT_LIVE -DMGC_SWEEP_LIVE -DMGC_TUNE_LIVE -DMGC_MARK_STACK_ENTRIES=64u -DMGC_BREAK_MARK_OVERFLOW)
- `goldens/gc/host-markovf.o`
  `80f4b715ae72161963b2cf11362a078511d971e2bae92c3f618dc9bcf465bb0b`
  (src/gc/host/mgc_host.cpp; -DMGC_HOST_DRIVE_ALLOC -DMGC_HOST_DRIVE_COLLECT -DMGC_HOST_DRIVE_SWEEP -DMGC_HOST_DRIVE_TUNE -DMGC_HOST_DRIVE_MARKOVF)
- `goldens/gc/host-handle-growth.o`
  `b51c002715cba5303cd77ea49b8d0cf1fd6e3cdd97fa29297bb2c25563535508`
  (src/gc/host/mgc_host.cpp; -DMGC_HOST_DRIVE_ALLOC -DMGC_HOST_DRIVE_COLLECT -DMGC_HOST_DRIVE_SWEEP -DMGC_HOST_DRIVE_TUNE -DMGC_HOST_DRIVE_HANDLE_GROWTH)
- `goldens/gc/skel-tune-break-growth.o`
  `178181b94b887cefb9d3a0e190f9acc972a6fba50748cba128839a17fa442d45`
  (src/gc/mgc_skeleton.cpp; -DMGC_HEAP_LIVE -DMGC_COLLECT_LIVE -DMGC_SWEEP_LIVE -DMGC_TUNE_LIVE -DMGC_HANDLE_SLOTS=64 -DMGC_BREAK_HANDLE_GROWTH)
- `goldens/gc/host-alloc-bound.o`
  `7a81eac18de997180068cbf0121054f3e67f885d27a166b044ad463b19b46bba`
  (src/gc/host/mgc_host.cpp; -DMGC_HOST_DRIVE_ALLOC -DMGC_HOST_DRIVE_COLLECT -DMGC_HOST_DRIVE_SWEEP -DMGC_HOST_DRIVE_TUNE -DMGC_HOST_DRIVE_ALLOC_BOUND)
- `goldens/gc/heap-tune-break-bound.o`
  `0ef45091f519d07968c6e94845bdf3e079c187f8b30dd89a55909f7deee823f8`
  (src/gc/mgc_heap.cpp; -DMGC_HEAP_LIVE -DMGC_COLLECT_LIVE -DMGC_SWEEP_LIVE -DMGC_TUNE_LIVE -DMGC_HEAP_BYTES=2097152u -DMGC_FINQ_RESERVE=16u -DMGC_BREAK_ALLOC_BOUND)
- `goldens/gc/host-tail-rejoin.o`
  `f2fc8fe897cef5c8599a34109255f1281aed6f9d776c07522659904149d552a7`
  (src/gc/host/mgc_host.cpp; -DMGC_HOST_DRIVE_ALLOC -DMGC_HOST_DRIVE_COLLECT -DMGC_HOST_DRIVE_SWEEP -DMGC_HOST_DRIVE_TUNE -DMGC_HOST_DRIVE_TAIL_REJOIN)
- `goldens/gc/collect-tune-break-rejoin.o`
  `9748bea5898d956348ed02942b61825f9ea1af4b79a40cf22e7962ece0e52ff6`
  (src/gc/mgc_collect.cpp; -DMGC_HEAP_LIVE -DMGC_COLLECT_LIVE -DMGC_SWEEP_LIVE -DMGC_TUNE_LIVE -DMGC_BREAK_TAIL_REJOIN)
- `goldens/gc/host-finalize-full.o`
  `23fd035f63c5dc9b53a8203e74dd90b2a9ecffd2e4dc36aec365ff2e553840c9`
  (src/gc/host/mgc_host.cpp; -DMGC_HOST_DRIVE_ALLOC -DMGC_HOST_DRIVE_COLLECT -DMGC_HOST_DRIVE_SWEEP -DMGC_HOST_DRIVE_TUNE -DMGC_HOST_DRIVE_FINALIZE_FULL)
- `goldens/gc/heap-tune-break-finalize.o`
  `40d7426a8c6570136d91b8bb2e952c673cd50da32cd47658ebfa5b8426ed59cd`
  (src/gc/mgc_heap.cpp; -DMGC_HEAP_LIVE -DMGC_COLLECT_LIVE -DMGC_SWEEP_LIVE -DMGC_TUNE_LIVE -DMGC_HEAP_BYTES=2097152u -DMGC_FINQ_RESERVE=16u -DMGC_BREAK_FINALIZE_FULL)
- `goldens/gc/heap-tune-break-finq-reserve.o`
  `29cb51be6ef5244e6186d458f6679da0b09c1abcdd94000201a28881ef34af2c`
  (src/gc/mgc_heap.cpp; -DMGC_HEAP_LIVE -DMGC_COLLECT_LIVE -DMGC_SWEEP_LIVE -DMGC_TUNE_LIVE -DMGC_HEAP_BYTES=2097152u -DMGC_FINQ_RESERVE=0u)
- `goldens/gc/host-window-keep.o`
  `eae1e62cc23aabb54932bc9d0c0995ecb6803ea5011f644f00902f046d9bd57d`
  (src/gc/host/mgc_host.cpp; -DMGC_HOST_DRIVE_ALLOC -DMGC_HOST_DRIVE_COLLECT -DMGC_HOST_DRIVE_SWEEP -DMGC_HOST_DRIVE_TUNE -DMGC_HOST_DRIVE_WINDOW_KEEP)
- `goldens/gc/heap-tune-128k.o`
  `c0ec140fb871b7cae67ae5f52c546c809168854d9f43f5ebcd4e8dea383501cb`
  (src/gc/mgc_heap.cpp; -DMGC_HEAP_LIVE -DMGC_COLLECT_LIVE -DMGC_SWEEP_LIVE -DMGC_TUNE_LIVE -DMGC_HEAP_BYTES=131072u -DMGC_FINQ_RESERVE=16u -DMGC_OOM_RESERVE=8192u)
- `goldens/gc/heap-tune-128k-break.o`
  `baed4e93ec76ed56dc2f9819d13ca23742489c82fefacfc7a364f28982e05129`
  (src/gc/mgc_heap.cpp; -DMGC_HEAP_LIVE -DMGC_COLLECT_LIVE -DMGC_SWEEP_LIVE -DMGC_TUNE_LIVE -DMGC_HEAP_BYTES=131072u -DMGC_FINQ_RESERVE=16u -DMGC_OOM_RESERVE=8192u -DMGC_BREAK_WINDOW_KEEP)
- `goldens/gc/host-frozen-bound.o`
  `1cd68550ae60a38ee7dcf290e8a988944309c5024f6ccda01c04e3b00cf864d3`
  (src/gc/host/mgc_host.cpp; -DMGC_HOST_DRIVE_ALLOC -DMGC_HOST_DRIVE_COLLECT -DMGC_HOST_DRIVE_SWEEP -DMGC_HOST_DRIVE_TUNE -DMGC_HOST_DRIVE_FROZEN_BOUND)
- `goldens/gc/skel-tune-break-frozen.o`
  `de8d8f3c607e0873bc66d80f9af9e09217144ee7a564e7b28eaaf167575294f1`
  (src/gc/mgc_skeleton.cpp; -DMGC_HEAP_LIVE -DMGC_COLLECT_LIVE -DMGC_SWEEP_LIVE -DMGC_TUNE_LIVE -DMGC_HANDLE_SLOTS=64 -DMGC_BREAK_FROZEN_BOUND)
- `goldens/gc/heap-tune-break-keep.o`
  `9eccd2b84dd9bd23aabf0cdbfd9d59447a0c2abc73c9209d55b5f0ed33b63274`
  (src/gc/mgc_heap.cpp; -DMGC_HEAP_LIVE -DMGC_COLLECT_LIVE -DMGC_SWEEP_LIVE -DMGC_TUNE_LIVE -DMGC_HEAP_BYTES=2097152u -DMGC_FINQ_RESERVE=16u -DMGC_BREAK_WINDOW_KEEP)
- `goldens/gc/heap-row-break-less.o`
  `d5ca7671389fe512f6cb601033dd018c39a2fb9e7dbe52854df914370c77371c`
  (src/gc/mgc_heap.cpp; -DMGC_HEAP_LIVE -DMGC_COLLECT_LIVE -DMGC_SWEEP_LIVE -DMGC_TUNE_LIVE -DMGC_HEAP_AT_BOOT -DMGC_HEAP_KEEP_BYTES=131072u -DMGC_HEAP_MIN_GUARANTEED_BYTES=1572864u -DMGC_ARENA_COVER_BYTES=16777216u -DMGC_FINQ_RESERVE=16u -DMGC_BREAK_BOOT_LESS)
- `goldens/gc/heap-row-break-more.o`
  `94c1ce2da72b9bbdd1f81481e1c21d343bdc7575ac6c59f79ebbdf1650280564`
  (src/gc/mgc_heap.cpp; -DMGC_HEAP_LIVE -DMGC_COLLECT_LIVE -DMGC_SWEEP_LIVE -DMGC_TUNE_LIVE -DMGC_HEAP_AT_BOOT -DMGC_HEAP_KEEP_BYTES=131072u -DMGC_HEAP_MIN_GUARANTEED_BYTES=1572864u -DMGC_ARENA_COVER_BYTES=16777216u -DMGC_FINQ_RESERVE=16u -DMGC_BREAK_BOOT_MORE)
