# goldens/gc -- the gc gate's frozen world objects (MANIFEST)

Copyright (c) 2026 Lamella LLC and the Lamella project authors.
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
objects, the Cortex-M33 with the single-precision FPv5 under the same flag
set but the core and FPU words -- no C++ standard library headers (the
collector is freestanding C++ over the kit's C headers).

Frozen at the compile of record for the .NET 11 RC1 pin
(v11.0.0-rc.1.26425.128): the ten runtime-to-GC interface headers vendored
under src/vendor at that tag (the EE interface's major version 5, the flat
surface the gate ties three ways at 53 names); the mps2-an505 row's three
collector objects are the an500 row's sources and defines compiled for the
Cortex-M33 with the single-precision FPv5 (the class flag set beside the
Cortex-M7's in gc.rs; the collector reads no board row, so the class is
the one variable), each carrying no double-precision instruction;
36 objects.

Records (repo-relative path, sha256; defines documented per record):

- `goldens/gc/skel-bare.o`
  `cb5a40f2bbaac4da6f46581670c57355a06c8c28118cb9991c12a68f5a064465`
  (src/gc/mgc_skeleton.cpp; no extra defines)
- `goldens/gc/skel-bad.o`
  `4aa666c476882560938b047ccabc26b7a34bb4b2ed617947aee4a552f59b638b`
  (src/gc/mgc_skeleton.cpp; -DMGC_SKEL_BAD_VERSION)
- `goldens/gc/host.o`
  `ddc3a08beb674b869cdf88bc7367d07295309effdb3b173aeb00ba8cb91770b5`
  (src/gc/host/mgc_host.cpp; no extra defines)
- `goldens/gc/probe.o`
  `cc9a74e2da19dca3aaab850361cf52b03c46474c58d654411dda87f85b29a958`
  (src/gc/mgc_ee_probe.cpp; no extra defines)
- `goldens/gc/skel-heap.o`
  `3189bbd6e86a43a40747d071a1b38099611a04d65b676cf68f37982628328bed`
  (src/gc/mgc_skeleton.cpp; -DMGC_HEAP_LIVE)
- `goldens/gc/heap.o`
  `7605d891c090e3ce6d93a3038d8994f158954caf5947d932984216b241c3239a`
  (src/gc/mgc_heap.cpp; -DMGC_HEAP_LIVE -DMGC_HEAP_BYTES=2097152u)
- `goldens/gc/host-drive.o`
  `a73856b119ef28fbe6b4cc5be56ae10d2c3893d056db419cdd849fcc39cbffda`
  (src/gc/host/mgc_host.cpp; -DMGC_HOST_DRIVE_ALLOC)
- `goldens/gc/skel-collect.o`
  `1b47e043a831d656331757842f9c9677db2ff691997600a452c66bdb8b8eec99`
  (src/gc/mgc_skeleton.cpp; -DMGC_HEAP_LIVE -DMGC_COLLECT_LIVE)
- `goldens/gc/heap-collect.o`
  `714f14afebcafc534425db7752a2a2560a30566fcf49a753cf28446fda45202c`
  (src/gc/mgc_heap.cpp; -DMGC_HEAP_LIVE -DMGC_COLLECT_LIVE -DMGC_HEAP_BYTES=2097152u)
- `goldens/gc/collect.o`
  `40a50d56be86453fb5925eeabec677787743806a133875d1416f631d727fe48e`
  (src/gc/mgc_collect.cpp; -DMGC_HEAP_LIVE -DMGC_COLLECT_LIVE)
- `goldens/gc/collect-break.o`
  `3530cc6fcfc982a35f8de378db77a0efc6a0c4e4a50eb4b66f086ee29a722340`
  (src/gc/mgc_collect.cpp; -DMGC_HEAP_LIVE -DMGC_COLLECT_LIVE -DMGC_BREAK_ROOTS)
- `goldens/gc/host-collect.o`
  `538be9f3a4da63bc5e74912b5f5f017ae3ce59dbc1d9f88df59df61e29d00baa`
  (src/gc/host/mgc_host.cpp; -DMGC_HOST_DRIVE_ALLOC -DMGC_HOST_DRIVE_COLLECT)
- `goldens/gc/skel-sweep.o`
  `7efed8dffa1097d85d083bea6ad8b59e56fc37d528bbe782308d31016733e36c`
  (src/gc/mgc_skeleton.cpp; -DMGC_HEAP_LIVE -DMGC_COLLECT_LIVE -DMGC_SWEEP_LIVE)
- `goldens/gc/heap-sweep.o`
  `e6a5519992157d57f33e04382c23f3406376a225f275bb7eb5622ec3b8326c96`
  (src/gc/mgc_heap.cpp; -DMGC_HEAP_LIVE -DMGC_COLLECT_LIVE -DMGC_SWEEP_LIVE -DMGC_HEAP_BYTES=2097152u)
- `goldens/gc/collect-sweep.o`
  `8f8d58c9a3deebe10f949b8c590112284b612b2a20546225d5e3899d1ce472c1`
  (src/gc/mgc_collect.cpp; -DMGC_HEAP_LIVE -DMGC_COLLECT_LIVE -DMGC_SWEEP_LIVE)
- `goldens/gc/collect-sweep-break.o`
  `77b42b1fc77dcf3c3db86017c4583af70915748e3cffc9ddd4c50ca2c8299c5f`
  (src/gc/mgc_collect.cpp; -DMGC_HEAP_LIVE -DMGC_COLLECT_LIVE -DMGC_SWEEP_LIVE -DMGC_BREAK_WEAK)
- `goldens/gc/host-sweep.o`
  `2f6fedf73a45690b6e56a407f611db07a343888b791eb4c2c27140bec5df0f83`
  (src/gc/host/mgc_host.cpp; -DMGC_HOST_DRIVE_ALLOC -DMGC_HOST_DRIVE_COLLECT -DMGC_HOST_DRIVE_SWEEP)
- `goldens/gc/skel-tune.o`
  `14b23de1645945ac4ebd0fcdf8a3ebfbfcba7a174ba3cdfea2aca1e13ce5010f`
  (src/gc/mgc_skeleton.cpp; -DMGC_HEAP_LIVE -DMGC_COLLECT_LIVE -DMGC_SWEEP_LIVE -DMGC_TUNE_LIVE -DMGC_HANDLE_SLOTS=64)
- `goldens/gc/heap-tune.o`
  `f562dd57438fa9515a7779235daa6642b75ab801f37cde82c9d1c31b898d039c`
  (src/gc/mgc_heap.cpp; -DMGC_HEAP_LIVE -DMGC_COLLECT_LIVE -DMGC_SWEEP_LIVE -DMGC_TUNE_LIVE -DMGC_HEAP_BYTES=2097152u)
- `goldens/gc/heap-tune-break.o`
  `487025b53777d13488162dc9155d72aeb56c057da13943f8529e6ede4b497ccf`
  (src/gc/mgc_heap.cpp; -DMGC_HEAP_LIVE -DMGC_COLLECT_LIVE -DMGC_SWEEP_LIVE -DMGC_TUNE_LIVE -DMGC_HEAP_BYTES=2097152u -DMGC_BREAK_WINDOW_FIX)
- `goldens/gc/collect-tune.o`
  `3472cc6f9431472456eede5f594c1b42c284767964bba7a96d28f7dfcb16e198`
  (src/gc/mgc_collect.cpp; -DMGC_HEAP_LIVE -DMGC_COLLECT_LIVE -DMGC_SWEEP_LIVE -DMGC_TUNE_LIVE)
- `goldens/gc/host-tune.o`
  `8b1b19c5776d01de3bd0b9296b64eae57fabd6022823dd6e9443ab0dd36890cd`
  (src/gc/host/mgc_host.cpp; -DMGC_HOST_DRIVE_ALLOC -DMGC_HOST_DRIVE_COLLECT -DMGC_HOST_DRIVE_SWEEP -DMGC_HOST_DRIVE_TUNE)
- `goldens/gc/skel-row.o`
  `14b23de1645945ac4ebd0fcdf8a3ebfbfcba7a174ba3cdfea2aca1e13ce5010f`
  (src/gc/mgc_skeleton.cpp; -DMGC_HEAP_LIVE -DMGC_COLLECT_LIVE -DMGC_SWEEP_LIVE -DMGC_TUNE_LIVE -DMGC_HANDLE_SLOTS=64)
- `goldens/gc/heap-row.o`
  `4f34d74e25a9b2319012b29fe7347cec5a68b5fa9872f1b69510a1b099b56be7`
  (src/gc/mgc_heap.cpp; -DMGC_HEAP_LIVE -DMGC_COLLECT_LIVE -DMGC_SWEEP_LIVE -DMGC_TUNE_LIVE -DMGC_HEAP_BYTES=2097152u -DMGC_ARENA_COVER_BYTES=4194304u)
- `goldens/gc/heap-row-break-cover.o`
  `9ebceaea1b1e8f6c4f6af3d92c31003ba337237736dc7ed026e1ad7bf1f204dd`
  (src/gc/mgc_heap.cpp; -DMGC_HEAP_LIVE -DMGC_COLLECT_LIVE -DMGC_SWEEP_LIVE -DMGC_TUNE_LIVE -DMGC_HEAP_BYTES=2097152u -DMGC_ARENA_COVER_BYTES=2097152u)
- `goldens/gc/collect-row.o`
  `3472cc6f9431472456eede5f594c1b42c284767964bba7a96d28f7dfcb16e198`
  (src/gc/mgc_collect.cpp; -DMGC_HEAP_LIVE -DMGC_COLLECT_LIVE -DMGC_SWEEP_LIVE -DMGC_TUNE_LIVE)
- `goldens/gc/host-row.o`
  `d2ed8c002d3945f8289ceab3a63d9fd1656fb5a2b73d7bd8bec5f615b36a2450`
  (src/gc/host/mgc_host.cpp; -DMGC_HOST_DRIVE_ALLOC -DMGC_HOST_DRIVE_COLLECT -DMGC_HOST_DRIVE_SWEEP -DMGC_HOST_DRIVE_TUNE -DMGC_HOST_ROW_AN500)
- `goldens/gc/heap-row-f769.o`
  `50796571d45d481e585eaee82bdbb3cc35ffb00534c2a3855cb481c91b7f10c6`
  (src/gc/mgc_heap.cpp; -DMGC_HEAP_LIVE -DMGC_COLLECT_LIVE -DMGC_SWEEP_LIVE -DMGC_TUNE_LIVE -DMGC_HEAP_BYTES=98304u -DMGC_ARENA_COVER_BYTES=524288u -DMGC_FINQ_CAP=256u -DMGC_OOM_RESERVE=8192u -DMGC_WINDOW_BYTES=4096u -DMGC_WINDOW_FLOOR=256u -DMGC_WALL_RETRIES=4u)
- `goldens/gc/collect-row-f769.o`
  `ed01a9a29df1ca8c8f404951bc00f336c26596bd4b04434c09342c61df82d736`
  (src/gc/mgc_collect.cpp; -DMGC_HEAP_LIVE -DMGC_COLLECT_LIVE -DMGC_SWEEP_LIVE -DMGC_TUNE_LIVE -DMGC_MARK_STACK_ENTRIES=2048u -DMGC_OOM_RESERVE=8192u)
- `goldens/gc/host-freepad.o`
  `d3d5bb14d5ef15162183e315c67c51857a0f47b1611030113a842a53d932388f`
  (src/gc/host/mgc_host.cpp; -DMGC_HOST_DRIVE_ALLOC -DMGC_HOST_DRIVE_COLLECT -DMGC_HOST_DRIVE_SWEEP -DMGC_HOST_DRIVE_FREEPAD)
- `goldens/gc/heap-sweep-break-freepad.o`
  `88d7ac14e36aa8090c28eb892377e5e1469cced0b44df91ca35dff0ac6868612`
  (src/gc/mgc_heap.cpp; -DMGC_HEAP_LIVE -DMGC_COLLECT_LIVE -DMGC_SWEEP_LIVE -DMGC_HEAP_BYTES=2097152u -DMGC_BREAK_FREE_PAD)
- `goldens/gc/host-handles.o`
  `abe273d6ee3eb8b21446ae133f9946e4e26d380d9a1e65afa8fc77e68d87976e`
  (src/gc/host/mgc_host.cpp; -DMGC_HOST_DRIVE_ALLOC -DMGC_HOST_DRIVE_COLLECT -DMGC_HOST_DRIVE_SWEEP -DMGC_HOST_DRIVE_TUNE -DMGC_HOST_DRIVE_HANDLES -DMGC_HANDLES_ITER=100000u)
- `goldens/gc/skel-tune-break-lock.o`
  `f860fc70362a890a5e013077e99393407a1f6958b5d1ad91861b4804da47e67e`
  (src/gc/mgc_skeleton.cpp; -DMGC_HEAP_LIVE -DMGC_COLLECT_LIVE -DMGC_SWEEP_LIVE -DMGC_TUNE_LIVE -DMGC_HANDLE_SLOTS=64 -DMGC_BREAK_HANDLE_LOCK)
- `goldens/gc/skel-row-an505.o`
  `fb174cf14361e2ef5f8eb8914b654c9ef4d150ad3e9fa806d3d94c1c1bdf3aa2`
  (src/gc/mgc_skeleton.cpp; -DMGC_HEAP_LIVE -DMGC_COLLECT_LIVE -DMGC_SWEEP_LIVE -DMGC_TUNE_LIVE -DMGC_HANDLE_SLOTS=64; the Cortex-M33 class flag set)
- `goldens/gc/heap-row-an505.o`
  `d25728e835907f4ad8eaeaaded81f847b51f1531f9b96829583dbf61e7ee445c`
  (src/gc/mgc_heap.cpp; -DMGC_HEAP_LIVE -DMGC_COLLECT_LIVE -DMGC_SWEEP_LIVE -DMGC_TUNE_LIVE -DMGC_HEAP_BYTES=2097152u -DMGC_ARENA_COVER_BYTES=4194304u; the Cortex-M33 class flag set)
- `goldens/gc/collect-row-an505.o`
  `05e6fbf7f2a2e91b0233bbcc9572334a8fe695e894e4db6495a185d3b4dd1101`
  (src/gc/mgc_collect.cpp; -DMGC_HEAP_LIVE -DMGC_COLLECT_LIVE -DMGC_SWEEP_LIVE -DMGC_TUNE_LIVE; the Cortex-M33 class flag set)
