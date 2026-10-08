# goldens/gc -- the gc gate's frozen world objects (MANIFEST)

Copyright (c) 2026 Lamella LLC.
SPDX-License-Identifier: MIT OR Apache-2.0

The gc gate's C++ test images link from these FROZEN COMMITTED OBJECTS
(the artifact unit is the bytes from one build; the gate verifies every
file against this manifest -- and this manifest against its in-code pin --
before the first case, and REFUSES a mismatch: a frozen artifact that moved
is WRONG, never "rebuilt"). The consumer chain is Rust + QEMU only; nothing
here is compiled at gate time.

Derivation is the leg `rhu-gate gc rederive` (the compiler of record,
clang 22.1.8, against the Arm GNU Toolchain 12.2.MPACBTI-Rel1 sysroot; the
README's re-derivation section names the prerequisites): it recompiles every
object below from src/gc under the gate's flag set of record (CXX_FLAGS in
src/harness/src/gates/gc.rs, the kit's C-library headers as the sysroot the
resolver appends) plus the per-object defines from the in-code frozen plan,
and byte-asserts each output against these files. A re-freeze (a source,
flag, compiler or pin change) updates the objects, this manifest and the
in-code manifest pin together in one reviewed commit.

Compiler of record: clang version 22.1.8 (the LLVM project's 22.1.8
release; the version the runtime archives at the pin were built with, so
one LLVM lineage spells the 32-bit types and mangles the interface names
the same way on both sides of the seam), targeting arm-none-eabi for the
Cortex-M7 with hard-float FPv5-D16 -- and, for the mps2-an505 row's three
objects, the st-nucleo-u5a5zj-q row's heap object and the RP2350 rows'
heap and collect objects, the Cortex-M33 with
the single-precision FPv5 under the same flag set but the core and FPU
words -- no C++ standard library headers (the
collector is freestanding C++ over the kit's C headers).

Frozen at the compile of record for the .NET 11 RC1 pin
(v11.0.0-rc.1.26425.128): the ten runtime-to-GC interface headers vendored
under src/vendor at that tag (the EE interface's major version 5, the flat
surface the gate ties three ways at 53 names); the mps2-an505 row's three
collector objects are the an500 row's sources and defines compiled for the
Cortex-M33 with the single-precision FPv5 (the class flag set beside the
Cortex-M7's in gc.rs; the collector reads no board row, so the class is
the one variable), each carrying no double-precision instruction; the
st-nucleo-u5a5zj-q row's heap object is that source under the least
managed heap the row guarantees, as the row header declares it (the row
shares the
mps2-an505 row's skeleton and collect objects, which no differing field
reaches; the rederive leg ties every board-table row's objects to the
row's own declaration by bytes); the four RP2350 rows (the Raspberry Pi
Pico 2 and Pico 2 W, the Pimoroni Pico Plus 2 and Pico Plus 2 W) declare
one mini row each with the same fields, so their heap and collect objects
are frozen once under the Pico 2's declaration, named for the chip, and
the four rows name them, the skeleton the mps2-an505 row's; the
no-GC region's entry is refused through the pinned status (no budget and
no suppression state exist, so nothing is claimed) and its end answers
that no region is in progress, with a negative control built from the
skeleton with the claim restored and a host drive that reads both
statuses; the collector's mark stack, when full, defers the push and
rescans the marked objects inside the window the deferred objects span,
round by round, in place of the old fail-fast (the mark-overflow case's
collector object carries a 64-entry stack; its control has the recovery
compiled out); the windows skeleton's handle pool grows by segments of
256 slots from the shim's mmap past the row's 64 and answers no handle
when the mmap refuses, in place of the old fail-fast (the growth
control's skeleton has the growth compiled out; the two drive hosts
exercise both); the row heap objects size the managed heap at boot: each
takes all the system memory free at the collector's start, less the
collector tables for that size and less what its row keeps for later
draws (the frozen object heap and the unallocated amount), and refuses a
boot that would leave less than the least the row guarantees, so a row
object carries what its row keeps and that least in place of a size (two
control objects of the an500 row take one page less and one page more
than the rule gives); a request larger than the managed heap is answered
no memory before the fit arithmetic; the sweep zeroes a dead run at the
top of the heap and sets the bump mark back over it; a finalizable
request takes its slot in the finalization table before its memory, and
a full table means one collection, a bounded wait for the finalizer
thread, then a refusal; a finalizable or 8-byte-aligned small request
that fits the thread's allocation window is served inside it; the
windows skeleton asks, page by page, how much of a frozen segment's
commit is backed and stops the program when a segment backed for less
than its commit has no room for one more object (each of these five has
a drive host and a control object built with the behaviour compiled
out, and the windows drive's tail has a second control object built with
the window never kept); 60 objects.

Records (repo-relative path, sha256; defines documented per record):

- `goldens/gc/skel-bare.o`
  `f80612096df93dd648322816a721ac60c3824db179ad0dd2a42c8e6a9f513b38`
  (src/gc/mgc_skeleton.cpp; no extra defines)
- `goldens/gc/skel-bad.o`
  `46b82f542bcfeecb739e5f8fbfef58311be4cdb9e88f8adc27fc36046e5f88b7`
  (src/gc/mgc_skeleton.cpp; -DMGC_SKEL_BAD_VERSION)
- `goldens/gc/host.o`
  `ddc3a08beb674b869cdf88bc7367d07295309effdb3b173aeb00ba8cb91770b5`
  (src/gc/host/mgc_host.cpp; no extra defines)
- `goldens/gc/probe.o`
  `cc9a74e2da19dca3aaab850361cf52b03c46474c58d654411dda87f85b29a958`
  (src/gc/mgc_ee_probe.cpp; no extra defines)
- `goldens/gc/skel-heap.o`
  `ac1f6e7e2c8368e88d2310edab536ae74f2a13a65043b657f889cb0b0b9690fe`
  (src/gc/mgc_skeleton.cpp; -DMGC_HEAP_LIVE)
- `goldens/gc/heap.o`
  `7605d891c090e3ce6d93a3038d8994f158954caf5947d932984216b241c3239a`
  (src/gc/mgc_heap.cpp; -DMGC_HEAP_LIVE -DMGC_HEAP_BYTES=2097152u)
- `goldens/gc/host-drive.o`
  `a73856b119ef28fbe6b4cc5be56ae10d2c3893d056db419cdd849fcc39cbffda`
  (src/gc/host/mgc_host.cpp; -DMGC_HOST_DRIVE_ALLOC)
- `goldens/gc/skel-collect.o`
  `e3043a37955ee1fec16b8ec9a5c3e12168639573fd041979285d46ea23371d4d`
  (src/gc/mgc_skeleton.cpp; -DMGC_HEAP_LIVE -DMGC_COLLECT_LIVE)
- `goldens/gc/heap-collect.o`
  `714f14afebcafc534425db7752a2a2560a30566fcf49a753cf28446fda45202c`
  (src/gc/mgc_heap.cpp; -DMGC_HEAP_LIVE -DMGC_COLLECT_LIVE -DMGC_HEAP_BYTES=2097152u)
- `goldens/gc/collect.o`
  `0c302969a0aa3a7974882f3625c9fc530af89ad1d305d7995a19b5443e5d90db`
  (src/gc/mgc_collect.cpp; -DMGC_HEAP_LIVE -DMGC_COLLECT_LIVE)
- `goldens/gc/collect-break.o`
  `7b5433c7f0919b373b8cbbddfee3115074e1d74a23be3db4155a77b088750720`
  (src/gc/mgc_collect.cpp; -DMGC_HEAP_LIVE -DMGC_COLLECT_LIVE -DMGC_BREAK_ROOTS)
- `goldens/gc/host-collect.o`
  `538be9f3a4da63bc5e74912b5f5f017ae3ce59dbc1d9f88df59df61e29d00baa`
  (src/gc/host/mgc_host.cpp; -DMGC_HOST_DRIVE_ALLOC -DMGC_HOST_DRIVE_COLLECT)
- `goldens/gc/skel-sweep.o`
  `4690e0bd0c590763c93d63388c59c6fc6a677bca99bc9491e164dcea490358f1`
  (src/gc/mgc_skeleton.cpp; -DMGC_HEAP_LIVE -DMGC_COLLECT_LIVE -DMGC_SWEEP_LIVE)
- `goldens/gc/heap-sweep.o`
  `f76e088d03ce86ded7382efc6f8a93c5a8c8a14576709e9a13b490562be86299`
  (src/gc/mgc_heap.cpp; -DMGC_HEAP_LIVE -DMGC_COLLECT_LIVE -DMGC_SWEEP_LIVE -DMGC_HEAP_BYTES=2097152u)
- `goldens/gc/collect-sweep.o`
  `ce4d99935ee67092e3f103a06576a4cd1619ffdab43cc840c6fa7a52c212c609`
  (src/gc/mgc_collect.cpp; -DMGC_HEAP_LIVE -DMGC_COLLECT_LIVE -DMGC_SWEEP_LIVE)
- `goldens/gc/collect-sweep-break.o`
  `be415252f7a0ebec4c54cd02227811e7405cb0eaa4f6d8b547fd0536ce137c58`
  (src/gc/mgc_collect.cpp; -DMGC_HEAP_LIVE -DMGC_COLLECT_LIVE -DMGC_SWEEP_LIVE -DMGC_BREAK_WEAK)
- `goldens/gc/host-sweep.o`
  `e56b0490535bda8f40a8d73fe1d03197fc8b6d5614ce738cc99b6de625ed8699`
  (src/gc/host/mgc_host.cpp; -DMGC_HOST_DRIVE_ALLOC -DMGC_HOST_DRIVE_COLLECT -DMGC_HOST_DRIVE_SWEEP)
- `goldens/gc/skel-tune.o`
  `cd45feca5aaa21ecc0b41473f1597003cd9634b0e6a47c5b829b9c9c2deabcc3`
  (src/gc/mgc_skeleton.cpp; -DMGC_HEAP_LIVE -DMGC_COLLECT_LIVE -DMGC_SWEEP_LIVE -DMGC_TUNE_LIVE -DMGC_HANDLE_SLOTS=64)
- `goldens/gc/heap-tune.o`
  `418636a03f3154a28069129b1ffebbb2afae25eb61d892c06a7c4b89118ddde7`
  (src/gc/mgc_heap.cpp; -DMGC_HEAP_LIVE -DMGC_COLLECT_LIVE -DMGC_SWEEP_LIVE -DMGC_TUNE_LIVE -DMGC_HEAP_BYTES=2097152u)
- `goldens/gc/heap-tune-break.o`
  `9d70a22156b19f1cda59631d8be970c1e8a6a82bad9c70e04ddc3e1332f5cdb9`
  (src/gc/mgc_heap.cpp; -DMGC_HEAP_LIVE -DMGC_COLLECT_LIVE -DMGC_SWEEP_LIVE -DMGC_TUNE_LIVE -DMGC_HEAP_BYTES=2097152u -DMGC_BREAK_WINDOW_FIX)
- `goldens/gc/collect-tune.o`
  `9f2c63c387f6914ae2323e0ab6bc394040b9aad5648252c986203c86d234fd10`
  (src/gc/mgc_collect.cpp; -DMGC_HEAP_LIVE -DMGC_COLLECT_LIVE -DMGC_SWEEP_LIVE -DMGC_TUNE_LIVE)
- `goldens/gc/host-tune.o`
  `ac0ffbe017b67219dcc05206e5ed21b4087b34acd328ceee82a84f4cb00e54bb`
  (src/gc/host/mgc_host.cpp; -DMGC_HOST_DRIVE_ALLOC -DMGC_HOST_DRIVE_COLLECT -DMGC_HOST_DRIVE_SWEEP -DMGC_HOST_DRIVE_TUNE)
- `goldens/gc/skel-row.o`
  `cd45feca5aaa21ecc0b41473f1597003cd9634b0e6a47c5b829b9c9c2deabcc3`
  (src/gc/mgc_skeleton.cpp; -DMGC_HEAP_LIVE -DMGC_COLLECT_LIVE -DMGC_SWEEP_LIVE -DMGC_TUNE_LIVE -DMGC_HANDLE_SLOTS=64)
- `goldens/gc/heap-row.o`
  `ad0800fcabcdf2eb1abd5cfbf9d610fa408a5dbd4859a06541d6ccc6291d2752`
  (src/gc/mgc_heap.cpp; -DMGC_HEAP_LIVE -DMGC_COLLECT_LIVE -DMGC_SWEEP_LIVE -DMGC_TUNE_LIVE -DMGC_HEAP_AT_BOOT -DMGC_HEAP_KEEP_BYTES=131072u -DMGC_HEAP_MIN_GUARANTEED_BYTES=1572864u -DMGC_ARENA_COVER_BYTES=4194304u)
- `goldens/gc/heap-row-break-cover.o`
  `ceae9223acef45063e4904c48b2c8e03bba643d413ad1d1a948aaf0b89e9b71a`
  (src/gc/mgc_heap.cpp; -DMGC_HEAP_LIVE -DMGC_COLLECT_LIVE -DMGC_SWEEP_LIVE -DMGC_TUNE_LIVE -DMGC_HEAP_AT_BOOT -DMGC_HEAP_KEEP_BYTES=131072u -DMGC_HEAP_MIN_GUARANTEED_BYTES=1572864u -DMGC_ARENA_COVER_BYTES=2097152u)
- `goldens/gc/collect-row.o`
  `9f2c63c387f6914ae2323e0ab6bc394040b9aad5648252c986203c86d234fd10`
  (src/gc/mgc_collect.cpp; -DMGC_HEAP_LIVE -DMGC_COLLECT_LIVE -DMGC_SWEEP_LIVE -DMGC_TUNE_LIVE)
- `goldens/gc/host-row.o`
  `fa4f9ad700792caf5e547036027a4900f0aefb612ab98490eab62eab554e5633`
  (src/gc/host/mgc_host.cpp; -DMGC_HOST_DRIVE_ALLOC -DMGC_HOST_DRIVE_COLLECT -DMGC_HOST_DRIVE_SWEEP -DMGC_HOST_DRIVE_TUNE -DMGC_HOST_ROW_AN500)
- `goldens/gc/heap-row-f769.o`
  `e0d338046fad819c44c1f1cf94b6eee949ba29ea146854a08f477876c2866784`
  (src/gc/mgc_heap.cpp; -DMGC_HEAP_LIVE -DMGC_COLLECT_LIVE -DMGC_SWEEP_LIVE -DMGC_TUNE_LIVE -DMGC_HEAP_AT_BOOT -DMGC_HEAP_KEEP_BYTES=24576u -DMGC_HEAP_MIN_GUARANTEED_BYTES=131072u -DMGC_ARENA_COVER_BYTES=524288u -DMGC_FINQ_CAP=256u -DMGC_OOM_RESERVE=8192u -DMGC_WINDOW_BYTES=4096u -DMGC_WINDOW_FLOOR=256u -DMGC_WALL_RETRIES=4u)
- `goldens/gc/collect-row-f769.o`
  `a4578b7b5b0c99e1a72a9602f9bc2331b736c4b928cc1cd4140b5540d20ddfad`
  (src/gc/mgc_collect.cpp; -DMGC_HEAP_LIVE -DMGC_COLLECT_LIVE -DMGC_SWEEP_LIVE -DMGC_TUNE_LIVE -DMGC_MARK_STACK_ENTRIES=2048u -DMGC_OOM_RESERVE=8192u)
- `goldens/gc/host-freepad.o`
  `d3d5bb14d5ef15162183e315c67c51857a0f47b1611030113a842a53d932388f`
  (src/gc/host/mgc_host.cpp; -DMGC_HOST_DRIVE_ALLOC -DMGC_HOST_DRIVE_COLLECT -DMGC_HOST_DRIVE_SWEEP -DMGC_HOST_DRIVE_FREEPAD)
- `goldens/gc/heap-sweep-break-freepad.o`
  `fcdd6b069d05e6b259cfe93bd959f7d7db652fed2bdfa3afcb162b4bb7bbb550`
  (src/gc/mgc_heap.cpp; -DMGC_HEAP_LIVE -DMGC_COLLECT_LIVE -DMGC_SWEEP_LIVE -DMGC_HEAP_BYTES=2097152u -DMGC_BREAK_FREE_PAD)
- `goldens/gc/host-handles.o`
  `e5e0adcb22a27ceaf07edeb4bb276a1a877e88426178c3f3f4881fbaadfc3fd4`
  (src/gc/host/mgc_host.cpp; -DMGC_HOST_DRIVE_ALLOC -DMGC_HOST_DRIVE_COLLECT -DMGC_HOST_DRIVE_SWEEP -DMGC_HOST_DRIVE_TUNE -DMGC_HOST_DRIVE_HANDLES -DMGC_HANDLES_ITER=100000u)
- `goldens/gc/skel-tune-break-lock.o`
  `fdf272bccb9ade7778489e7e400c706dfa1b1ec4f841d1d3b6640614a1413497`
  (src/gc/mgc_skeleton.cpp; -DMGC_HEAP_LIVE -DMGC_COLLECT_LIVE -DMGC_SWEEP_LIVE -DMGC_TUNE_LIVE -DMGC_HANDLE_SLOTS=64 -DMGC_BREAK_HANDLE_LOCK)
- `goldens/gc/host-nogc.o`
  `1f1d89fb247ad44ecde341fd3403effaa9f727394b80858a5ee8c3d3f9e4083f`
  (src/gc/host/mgc_host.cpp; -DMGC_HOST_DRIVE_ALLOC -DMGC_HOST_DRIVE_COLLECT -DMGC_HOST_DRIVE_SWEEP -DMGC_HOST_DRIVE_TUNE -DMGC_HOST_DRIVE_NOGC)
- `goldens/gc/skel-tune-break-nogc.o`
  `15697d0b1c841ec845d8ba32f60e25549cc02d230aa281c5df6a94a1cbe11804`
  (src/gc/mgc_skeleton.cpp; -DMGC_HEAP_LIVE -DMGC_COLLECT_LIVE -DMGC_SWEEP_LIVE -DMGC_TUNE_LIVE -DMGC_HANDLE_SLOTS=64 -DMGC_BREAK_NOGC_CLAIM)
- `goldens/gc/skel-row-an505.o`
  `450168efd20d52d14eadf77d6b11460720b09b2945b7a4f0a3828139def0af73`
  (src/gc/mgc_skeleton.cpp; -DMGC_HEAP_LIVE -DMGC_COLLECT_LIVE -DMGC_SWEEP_LIVE -DMGC_TUNE_LIVE -DMGC_HANDLE_SLOTS=64; the Cortex-M33 class flag set)
- `goldens/gc/heap-row-an505.o`
  `ebfc5bab52746b5a503456d76d6f6ed21ea59d1e9be3f46feade9e4ec8e291b9`
  (src/gc/mgc_heap.cpp; -DMGC_HEAP_LIVE -DMGC_COLLECT_LIVE -DMGC_SWEEP_LIVE -DMGC_TUNE_LIVE -DMGC_HEAP_AT_BOOT -DMGC_HEAP_KEEP_BYTES=131072u -DMGC_HEAP_MIN_GUARANTEED_BYTES=1572864u -DMGC_ARENA_COVER_BYTES=4194304u; the Cortex-M33 class flag set)
- `goldens/gc/collect-row-an505.o`
  `0b9ca975bbcb5a862782bba35f58907be61ff84ebcca8781e41a9865b5be5405`
  (src/gc/mgc_collect.cpp; -DMGC_HEAP_LIVE -DMGC_COLLECT_LIVE -DMGC_SWEEP_LIVE -DMGC_TUNE_LIVE; the Cortex-M33 class flag set)
- `goldens/gc/heap-row-u5a5.o`
  `bb42f2802f3922be0be6a3553254c8244b7e1d0a7520d8106e0b12d34bdd4a5e`
  (src/gc/mgc_heap.cpp; -DMGC_HEAP_LIVE -DMGC_COLLECT_LIVE -DMGC_SWEEP_LIVE -DMGC_TUNE_LIVE -DMGC_HEAP_AT_BOOT -DMGC_HEAP_KEEP_BYTES=131072u -DMGC_HEAP_MIN_GUARANTEED_BYTES=1048576u -DMGC_ARENA_COVER_BYTES=4194304u; the Cortex-M33 class flag set)
- `goldens/gc/heap-row-rp2350.o`
  `af7b7e322c2a5b3837d0459df7e40f72f1fd7bc84edffeb24606be291466fbff`
  (src/gc/mgc_heap.cpp; -DMGC_HEAP_LIVE -DMGC_COLLECT_LIVE -DMGC_SWEEP_LIVE -DMGC_TUNE_LIVE -DMGC_HEAP_AT_BOOT -DMGC_HEAP_KEEP_BYTES=24576u -DMGC_HEAP_MIN_GUARANTEED_BYTES=131072u -DMGC_ARENA_COVER_BYTES=1048576u -DMGC_FINQ_CAP=256u -DMGC_OOM_RESERVE=8192u -DMGC_WINDOW_BYTES=4096u -DMGC_WINDOW_FLOOR=256u -DMGC_WALL_RETRIES=4u; the Cortex-M33 class flag set)
- `goldens/gc/collect-row-rp2350.o`
  `405d4c58c4df27e31d99877eeee1de50eb6a40756cfe9774e172eddc8b214ad7`
  (src/gc/mgc_collect.cpp; -DMGC_HEAP_LIVE -DMGC_COLLECT_LIVE -DMGC_SWEEP_LIVE -DMGC_TUNE_LIVE -DMGC_MARK_STACK_ENTRIES=2048u -DMGC_OOM_RESERVE=8192u; the Cortex-M33 class flag set)
- `goldens/gc/collect-tune-stk64.o`
  `69260267f6ccfa3603b7f511cdca523f21d53be481ba09edc7257d8e1be92080`
  (src/gc/mgc_collect.cpp; -DMGC_HEAP_LIVE -DMGC_COLLECT_LIVE -DMGC_SWEEP_LIVE -DMGC_TUNE_LIVE -DMGC_MARK_STACK_ENTRIES=64u)
- `goldens/gc/collect-tune-stk64-break.o`
  `0cd85ca137cbb3eba6f5b836ba0769b57a783c76f0e06f04c4288de10d12ddf3`
  (src/gc/mgc_collect.cpp; -DMGC_HEAP_LIVE -DMGC_COLLECT_LIVE -DMGC_SWEEP_LIVE -DMGC_TUNE_LIVE -DMGC_MARK_STACK_ENTRIES=64u -DMGC_BREAK_MARK_OVERFLOW)
- `goldens/gc/host-markovf.o`
  `5fe6629beb34ff785eef03aa1efd3f3519dbe2a796891445760330c8071a188a`
  (src/gc/host/mgc_host.cpp; -DMGC_HOST_DRIVE_ALLOC -DMGC_HOST_DRIVE_COLLECT -DMGC_HOST_DRIVE_SWEEP -DMGC_HOST_DRIVE_TUNE -DMGC_HOST_DRIVE_MARKOVF)
- `goldens/gc/host-handle-growth.o`
  `fed8c4a20a7184940939a1bbf8c410f3f7d0168134d7bd1abd592951e5d6f767`
  (src/gc/host/mgc_host.cpp; -DMGC_HOST_DRIVE_ALLOC -DMGC_HOST_DRIVE_COLLECT -DMGC_HOST_DRIVE_SWEEP -DMGC_HOST_DRIVE_TUNE -DMGC_HOST_DRIVE_HANDLE_GROWTH)
- `goldens/gc/skel-tune-break-growth.o`
  `9b93d3e227fb3c143ca66f5a79179298fb4471cfb95ef4bee615301ffc8a396d`
  (src/gc/mgc_skeleton.cpp; -DMGC_HEAP_LIVE -DMGC_COLLECT_LIVE -DMGC_SWEEP_LIVE -DMGC_TUNE_LIVE -DMGC_HANDLE_SLOTS=64 -DMGC_BREAK_HANDLE_GROWTH)
- `goldens/gc/host-alloc-bound.o`
  `95029d64a57112b89dd338c776541c30aea3b8af6d87326ed73b664ea3de8284`
  (src/gc/host/mgc_host.cpp; -DMGC_HOST_DRIVE_ALLOC -DMGC_HOST_DRIVE_COLLECT -DMGC_HOST_DRIVE_SWEEP -DMGC_HOST_DRIVE_TUNE -DMGC_HOST_DRIVE_ALLOC_BOUND)
- `goldens/gc/heap-tune-break-bound.o`
  `2e762ec8f161f2f5a47d31762c78753929a86117d39cd8fc768709b76120886d`
  (src/gc/mgc_heap.cpp; -DMGC_HEAP_LIVE -DMGC_COLLECT_LIVE -DMGC_SWEEP_LIVE -DMGC_TUNE_LIVE -DMGC_HEAP_BYTES=2097152u -DMGC_BREAK_ALLOC_BOUND)
- `goldens/gc/host-tail-rejoin.o`
  `99c088219652bbe8ca469ea227e0982c9d2320dd075b3989ec54a9d7b071f955`
  (src/gc/host/mgc_host.cpp; -DMGC_HOST_DRIVE_ALLOC -DMGC_HOST_DRIVE_COLLECT -DMGC_HOST_DRIVE_SWEEP -DMGC_HOST_DRIVE_TUNE -DMGC_HOST_DRIVE_TAIL_REJOIN)
- `goldens/gc/collect-tune-break-rejoin.o`
  `2ce5872700a522f2de07fab811fb483997313f818f1a357bb5aa69a6d7331295`
  (src/gc/mgc_collect.cpp; -DMGC_HEAP_LIVE -DMGC_COLLECT_LIVE -DMGC_SWEEP_LIVE -DMGC_TUNE_LIVE -DMGC_BREAK_TAIL_REJOIN)
- `goldens/gc/host-finalize-full.o`
  `bef75b879b1ba263e16f87bda30006154e7f7c481d252841bec160faa93560c6`
  (src/gc/host/mgc_host.cpp; -DMGC_HOST_DRIVE_ALLOC -DMGC_HOST_DRIVE_COLLECT -DMGC_HOST_DRIVE_SWEEP -DMGC_HOST_DRIVE_TUNE -DMGC_HOST_DRIVE_FINALIZE_FULL)
- `goldens/gc/heap-tune-break-finalize.o`
  `4634c9344992080fd87ee6a23b153388132b4eab1f53eecd718919cd8fcd97a0`
  (src/gc/mgc_heap.cpp; -DMGC_HEAP_LIVE -DMGC_COLLECT_LIVE -DMGC_SWEEP_LIVE -DMGC_TUNE_LIVE -DMGC_HEAP_BYTES=2097152u -DMGC_BREAK_FINALIZE_FULL)
- `goldens/gc/host-window-keep.o`
  `fce8655701b0bc71632ae33b17d7134589c2fbe164f9e5e6930562e192c133b1`
  (src/gc/host/mgc_host.cpp; -DMGC_HOST_DRIVE_ALLOC -DMGC_HOST_DRIVE_COLLECT -DMGC_HOST_DRIVE_SWEEP -DMGC_HOST_DRIVE_TUNE -DMGC_HOST_DRIVE_WINDOW_KEEP)
- `goldens/gc/heap-tune-128k.o`
  `9053cd116d43cb8d2d08932581094ee2738045b7afc8d2dea2801cfa67e9d98f`
  (src/gc/mgc_heap.cpp; -DMGC_HEAP_LIVE -DMGC_COLLECT_LIVE -DMGC_SWEEP_LIVE -DMGC_TUNE_LIVE -DMGC_HEAP_BYTES=131072u -DMGC_OOM_RESERVE=8192u)
- `goldens/gc/heap-tune-128k-break.o`
  `2f64c4530415af193ad42bea728b4f0b16e422a39bb0922609d658d664b81b08`
  (src/gc/mgc_heap.cpp; -DMGC_HEAP_LIVE -DMGC_COLLECT_LIVE -DMGC_SWEEP_LIVE -DMGC_TUNE_LIVE -DMGC_HEAP_BYTES=131072u -DMGC_OOM_RESERVE=8192u -DMGC_BREAK_WINDOW_KEEP)
- `goldens/gc/host-frozen-bound.o`
  `9bca851c790bfc1e2b8d7826a6a26de5027543da5ac24a5cc94a8a5c4151aa1a`
  (src/gc/host/mgc_host.cpp; -DMGC_HOST_DRIVE_ALLOC -DMGC_HOST_DRIVE_COLLECT -DMGC_HOST_DRIVE_SWEEP -DMGC_HOST_DRIVE_TUNE -DMGC_HOST_DRIVE_FROZEN_BOUND)
- `goldens/gc/skel-tune-break-frozen.o`
  `e008f8e1e094d704c5e71d2ab4a5c33fd879ed0e6722e54870a20456bad37f8c`
  (src/gc/mgc_skeleton.cpp; -DMGC_HEAP_LIVE -DMGC_COLLECT_LIVE -DMGC_SWEEP_LIVE -DMGC_TUNE_LIVE -DMGC_HANDLE_SLOTS=64 -DMGC_BREAK_FROZEN_BOUND)
- `goldens/gc/heap-tune-break-keep.o`
  `81c919ac0a17e1eaad6afd2c0595de48fe3db44a7f0a95de927770a77b232df8`
  (src/gc/mgc_heap.cpp; -DMGC_HEAP_LIVE -DMGC_COLLECT_LIVE -DMGC_SWEEP_LIVE -DMGC_TUNE_LIVE -DMGC_HEAP_BYTES=2097152u -DMGC_BREAK_WINDOW_KEEP)
- `goldens/gc/heap-row-break-less.o`
  `0a5c12eeb4b0134c489ff3befc12391a369fdc25fe3f07d625cb15f5a3d1336f`
  (src/gc/mgc_heap.cpp; -DMGC_HEAP_LIVE -DMGC_COLLECT_LIVE -DMGC_SWEEP_LIVE -DMGC_TUNE_LIVE -DMGC_HEAP_AT_BOOT -DMGC_HEAP_KEEP_BYTES=131072u -DMGC_HEAP_MIN_GUARANTEED_BYTES=1572864u -DMGC_ARENA_COVER_BYTES=4194304u -DMGC_BREAK_BOOT_LESS)
- `goldens/gc/heap-row-break-more.o`
  `db15549c30a873056653b47105574842a56d27760c4ca1bee11f25e42ed42095`
  (src/gc/mgc_heap.cpp; -DMGC_HEAP_LIVE -DMGC_COLLECT_LIVE -DMGC_SWEEP_LIVE -DMGC_TUNE_LIVE -DMGC_HEAP_AT_BOOT -DMGC_HEAP_KEEP_BYTES=131072u -DMGC_HEAP_MIN_GUARANTEED_BYTES=1572864u -DMGC_ARENA_COVER_BYTES=4194304u -DMGC_BREAK_BOOT_MORE)
