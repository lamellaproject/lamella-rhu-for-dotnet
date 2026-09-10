# goldens/shim -- the shim gate's frozen world objects and goldens (MANIFEST)

Copyright (c) 2026 Lamella LLC and the Lamella project authors.
SPDX-License-Identifier: MIT OR Apache-2.0

The shim gate's images link from these FROZEN COMMITTED OBJECTS, and its
positive cases compare the tenant's payload stream against these goldens
(the artifact unit is the bytes from one recorded build; the gate verifies
every file against this manifest -- and this manifest against its in-code
pin -- before the first case, and REFUSES a mismatch: a frozen artifact that
moved is WRONG, never "rebuilt"). The consumer chain is Rust + QEMU only;
nothing here is compiled at gate time.

Derivation is the leg `rhu-gate shim rederive` (the compiler of record,
clang 22.1.8, against the Arm GNU Toolchain 12.2.MPACBTI-Rel1 sysroot; the
README's re-derivation section names the prerequisites): it recompiles every
object below from src/shim as C under the gate's flag set of record (CC_FLAGS
in src/harness/src/gates/shim.rs) plus the per-object defines from the in-code
frozen plan, and byte-asserts each output against these files. A re-freeze
(a source, flag, kit or pin change) updates the objects, the goldens, this
manifest, and the in-code manifest pin together in one reviewed commit.
Goldens are captured by the documented run method (the payload stream of
the positive image, wire bytes kept), never edited.

Compiler of record: clang version 22.1.8 (the LLVM project's 22.1.8
release; the version the runtime archives at the pin were built with),
targeting arm-none-eabi for the Cortex-M7 with hard-float FPv5-D16, the
kit's C-library headers as its sysroot, driving the C front end (-x c
-std=gnu11) with no implicit floating-point use (-mno-implicit-float): the
activation costume's object carries no floating-point instruction, and the
gate pins every object's count.

Frozen at the compile of record for the .NET 11 RC1 pin
(v11.0.0-rc.1.26425.128): the generated contract headers carry the .NET
major and the pin generation in their names (rhu_pal_<area>_net11_rc1.h,
rhu_rt_entry_net11_rc1.h) and were regenerated from the RC1 source with no
value moved; the shim answers the two names the RC1 runtime archives demand
beyond the previous pin's set -- getauxval (no auxiliary vector: 0 with
ENOENT) and realpath (over the RAM file store) -- and the tenant checks
both; the RAM file store keeps a stated contract (a maximum file size with
every position, count and length checked against it in 64 bits; an
object's name and its open descriptors apart; one store lock; a descriptor
found before anything is created or truncated; the access mode checked at
the read boundary; advisory locks per object; paths with a trailing slash
named the same; file times range-checked), each clause a check in the
tenant's file ladder with a negative control built from the store with
that clause's defect restored; 27 objects and 5 goldens, byte-tied by the
rederive leg at the freeze.

Records (repo-relative path, sha256; source and defines per object):

- `goldens/shim/shim-core.o`
  `4c5ad02aecda6bfd45c85510d77cd4878db56de4f1e4cb0707c42bd71f835f0f`
  (src/shim/rhu_shim.c; -DMGC_HOST_ROW_AN500)
- `goldens/shim/shim-core-break-sipid.o`
  `5ed54c3efc8afc0d526e5a40ce16f3df88b2ef3075d79d6a0b8b4f27679f897d`
  (src/shim/rhu_shim.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_SIPID)
- `goldens/shim/shim-core-break-ctxpc.o`
  `9506b498d036221920e2269f2d8d079e2cf83537c631a6ac5194dceeb90c1cd0`
  (src/shim/rhu_shim.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_CTXPC)
- `goldens/shim/shim-core-break-tlsoff.o`
  `c9ebd1fa4ef1fc90d26ae8895afef0aa9fd1f0f98eda6a2302b16bdee7977080`
  (src/shim/rhu_shim.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_TLSOFF)
- `goldens/shim/shim-core-break-tlscache.o`
  `2737fb0c2964500ec93f1c8e53df5d1347dc7568079f4d16d4a3a935cd26706f`
  (src/shim/rhu_shim.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_TLSCACHE)
- `goldens/shim/shim-sysnative.o`
  `1a46813099524b62acecf2b492c204a5ac0f7e00f6c6ff7e737fd3390cf44ede`
  (src/shim/rhu_sysnative.c; -DMGC_HOST_ROW_AN500)
- `goldens/shim/shim-sysnative-break-fdbase.o`
  `6f7b9b4b80d108ea6865dca4541a6a51a2448e62a0b5e7828a2610fd9ff81e20`
  (src/shim/rhu_sysnative.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_FD_BASE)
- `goldens/shim/shim-sysnative-break-store-bounds.o`
  `d9d76e6e3cd64b1a3dfffb83a00f2f9c171bee61537d75809b175a9b5282c130`
  (src/shim/rhu_sysnative.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_STORE_BOUNDS)
- `goldens/shim/shim-sysnative-break-store-lifetime.o`
  `6325ecd73c2b8f3b6c0ae1cea363851bb89f8748247594db9aa7a074df349c3f`
  (src/shim/rhu_sysnative.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_STORE_LIFETIME)
- `goldens/shim/shim-sysnative-break-store-lock.o`
  `dc1cb1d3d06f204bc157946e72ff8f717a708d39bf482a061697c0acf5771760`
  (src/shim/rhu_sysnative.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_STORE_LOCK)
- `goldens/shim/shim-sysnative-break-store-open-order.o`
  `4780171ef7f03be7fa52d382df6a7ed70c4dc2fd38f86b907d779bab842debc1`
  (src/shim/rhu_sysnative.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_STORE_OPEN_ORDER)
- `goldens/shim/shim-sysnative-break-store-access.o`
  `f14073ea3f064c637c8b42c2c5b8c8b2f01af6526cd8a9fb29724de384017c5a`
  (src/shim/rhu_sysnative.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_STORE_ACCESS)
- `goldens/shim/shim-sysnative-break-store-flock.o`
  `f7eafff86676b16fe645cb63e94696ae6b3bac7b2098cf8fa25fee7f9c20b219`
  (src/shim/rhu_sysnative.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_STORE_FLOCK)
- `goldens/shim/shim-sysnative-break-store-slash.o`
  `0cf8ad0d4be481ebae782a850201c7c235a5123fc8a4efa36db321b121e29640`
  (src/shim/rhu_sysnative.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_STORE_SLASH)
- `goldens/shim/shim-sysnative-break-store-times.o`
  `e4800b3d261449d20c2f2095fa8109fade59c9a8b0b4af6df470e22b1a45971a`
  (src/shim/rhu_sysnative.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_STORE_TIMES)
- `goldens/shim/shim-libc-glue.o`
  `8ffb50ca37ba98ee58a69b71156472609af785bb1dacd5e68bb2103b59ebe267`
  (src/shim/rhu_libc_glue.c; no extra defines)
- `goldens/shim/shim-host.o`
  `d8f025d3800b7c74fbf7a6e7b860d2fbc38bbae052c5df66c9a1f36fc85fc706`
  (src/shim/rhu_host.c; -DMGC_HOST_ROW_AN500)
- `goldens/shim/shim-host-break-dataverify.o`
  `b697b151be9bf27d8b4aef21c0674b707c4e86b1c894cde031f46f5bd6304f85`
  (src/shim/rhu_host.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_DATAVERIFY)
- `goldens/shim/shim-stdio-sym.o`
  `8428ce33a5f364f458b0ccaa949e3398470a9b05500810c6a62109731033a28d`
  (src/shim/rhu_stdio_sym.c; no extra defines)
- `goldens/shim/shimtest-core.o`
  `1b54f7f9694a187943400e322d772178654cbb8b577decc9366834d3530acea5`
  (src/shim/gate/rhu_shimtest.c; -DRHU_SHIMTEST_LADDER_CORE)
- `goldens/shim/shimtest-core-break-errno.o`
  `26ac2e321f74486b92330d482065af0cb5225c727d16b933586f9f1ef94eaa56`
  (src/shim/gate/rhu_shimtest.c; -DRHU_SHIMTEST_LADDER_CORE -DRHU_SHIMTEST_BREAK_ERRNO)
- `goldens/shim/shimtest-core-break-canary.o`
  `779fb15a79a7a4b62260dde0035b01fe37c65a40dbe43b80cb5ca660c1bbc67a`
  (src/shim/gate/rhu_shimtest.c; -DRHU_SHIMTEST_LADDER_CORE -DRHU_SHIMTEST_BREAK_CANARY)
- `goldens/shim/shimtest-fdmap.o`
  `0e1471fec3b759ab1162de0e0d99d9567fbee122ca661a036aa3da07c8847abe`
  (src/shim/gate/rhu_shimtest.c; -DRHU_SHIMTEST_LADDER_FDMAP)
- `goldens/shim/shimtest-activation.o`
  `6c97b3c291d79eddb1158ac9a08999de5b6f91936ceeed63b2d66e0cd7891273`
  (src/shim/gate/rhu_shimtest.c; -DRHU_SHIMTEST_LADDER_ACTIVATION)
- `goldens/shim/shimtest-tls.o`
  `cdc7b9c7955598ec1e6631fb2133c77c36f0cd079a8b63bfee0c26a0e964e700`
  (src/shim/gate/rhu_shimtest.c; -DRHU_SHIMTEST_LADDER_TLS)
- `goldens/shim/shimtest-host.o`
  `e247afc48310917018461b543e0c3a60dd9031cdf53321c10ddf83c8c4694371`
  (src/shim/gate/rhu_shimtest.c; -DRHU_SHIMTEST_LADDER_HOST)
- `goldens/shim/shim-core-f769.o`
  `339e99f8e8318a429248cd72cf3a3e8cef49af6088a84a2d60c8d9b3a859496f`
  (src/shim/rhu_shim.c; -DMGC_HOST_ROW_F769)
- `goldens/shim/shimtest-core.golden.txt`
  `3a11b9596ff3d05b9749b3690ad150cd356532d02813f544e288b1bb7e830fe0`
  (the payload stream of the shim.core image (class bytes))
- `goldens/shim/shimtest-fdmap.golden.txt`
  `6effa0fc4bc3b4b823a98de0906c43c3620c27246ad1b5e38bf1c08dfd1df0aa`
  (the payload stream of the shim.fdmap image (class bytes))
- `goldens/shim/shimtest-activation.golden.txt`
  `6a871eb23fbb9fb66f672da2624bb2caaae1cc41c7c73aec77ebc0838d250ae3`
  (the payload stream of the shim.activation image (class bytes))
- `goldens/shim/shimtest-tls.golden.txt`
  `1684cbccbea60fc52822d3389034ef541b72f63d6b33235420953694826e562b`
  (the payload stream of the shim.tls image (class bytes))
- `goldens/shim/shimtest-host.golden.txt`
  `aeb710f3cdc677e9c5ba29a1c4a432fe2389fda884b05a54dfcd2d910191cfb5`
  (the payload stream of the shim.host image (class bytes))
