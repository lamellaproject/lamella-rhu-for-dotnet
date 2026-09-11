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
targeting arm-none-eabi for the Cortex-M7 with hard-float FPv5-D16 -- and,
for the mps2-an505 row's five objects, the Cortex-M33 with the
single-precision FPv5 under the same flag set but the core and FPU words
(the class flag set beside the Cortex-M7's in shim.rs) -- the
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
that clause's defect restored; the random entries draw cryptographic bytes
through the kernel's secure door in bounded pieces, all or nothing, with a
refusal honored, and every wall-clock answer (the system time, the C
library's real-time clock and time(), the file store's times) reads the
kernel's calendar, each with a negative control built from the shim with
that answer's defect restored; the host states hosting-API contract
generation 32; the mps2-an505 row's five link objects are the an500 row's
sources and defines compiled for the Cortex-M33 (the row's fields equal
the an500's, so the class flag set is the one variable), each carrying no
double-precision instruction; 34 objects and 5 goldens,
byte-tied by the rederive leg at the freeze.

Records (repo-relative path, sha256; source and defines per object):

- `goldens/shim/shim-core.o`
  `b8926fcc9c7184a511a1e552c2d112f700a3f2df9c2b28114676eb0f26cf3070`
  (src/shim/rhu_shim.c; -DMGC_HOST_ROW_AN500)
- `goldens/shim/shim-core-break-sipid.o`
  `3ab96d903bf2119b497738e7bede50f7a3257c1a256bdb3d86e28e7a4b44434e`
  (src/shim/rhu_shim.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_SIPID)
- `goldens/shim/shim-core-break-ctxpc.o`
  `9544348eae92505f50f7ed117a7207f09958666ea8efdfbae8119b2e35504c42`
  (src/shim/rhu_shim.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_CTXPC)
- `goldens/shim/shim-core-break-tlsoff.o`
  `da4793d036e0eb45e7f62cb00742b68411122c3023dfff1437e2fb18ad415520`
  (src/shim/rhu_shim.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_TLSOFF)
- `goldens/shim/shim-core-break-tlscache.o`
  `15da3c760bef6217d5eb08fdc800b3a1f2484dc1889488666c374556d471c737`
  (src/shim/rhu_shim.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_TLSCACHE)
- `goldens/shim/shim-sysnative.o`
  `86a6e1b803cedb3e8c837db2274beaa8097e770fc48d41d03235bf221476678e`
  (src/shim/rhu_sysnative.c; -DMGC_HOST_ROW_AN500)
- `goldens/shim/shim-sysnative-break-fdbase.o`
  `5af1e28ab092e367adcd59c0ae6339114ce4473a3a2b883d18483317f82fd984`
  (src/shim/rhu_sysnative.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_FD_BASE)
- `goldens/shim/shim-sysnative-break-store-bounds.o`
  `191d44cc3acb17ac4d155b6fd62c053dfda20d8e18d3bd6fb90b13b28d1b3a18`
  (src/shim/rhu_sysnative.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_STORE_BOUNDS)
- `goldens/shim/shim-sysnative-break-store-lifetime.o`
  `e16fc44042d15f6c7ed4b03b0da3e68830419f99df04c4256050cc06fb26b43a`
  (src/shim/rhu_sysnative.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_STORE_LIFETIME)
- `goldens/shim/shim-sysnative-break-store-lock.o`
  `67e27ef65cdc9fffd925369e27f4d9ab36d69f9daf7fb2dee6853c44ad57e4b4`
  (src/shim/rhu_sysnative.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_STORE_LOCK)
- `goldens/shim/shim-sysnative-break-store-open-order.o`
  `5f1ed4499f36066b20d79625f6cb0b68fe434f14f9a554b0609f07250d063728`
  (src/shim/rhu_sysnative.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_STORE_OPEN_ORDER)
- `goldens/shim/shim-sysnative-break-store-access.o`
  `3c3212c64c5ff34187b2c528b637c3cb802807112ad8d3ca9a68e20e2edcdbfa`
  (src/shim/rhu_sysnative.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_STORE_ACCESS)
- `goldens/shim/shim-sysnative-break-store-flock.o`
  `b6e928530f2dc944c9583fd3964a6276186ef72572825cae412cc65b7f3489f1`
  (src/shim/rhu_sysnative.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_STORE_FLOCK)
- `goldens/shim/shim-sysnative-break-store-slash.o`
  `37f24d1cc1833a2ed095dfa40ef1db590cf676045a33e4e3fee61ac2f85943a5`
  (src/shim/rhu_sysnative.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_STORE_SLASH)
- `goldens/shim/shim-sysnative-break-store-times.o`
  `afeff2790f6caf706ac27dcd6f3bbe34d072788ccb48e54c62a769b3d4b76fc3`
  (src/shim/rhu_sysnative.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_STORE_TIMES)
- `goldens/shim/shim-sysnative-break-entropy-refusal.o`
  `db3cc00a9455405145cbd4ceec76e8c81c5dc1abf1bfa077fa7bdbe1d0217874`
  (src/shim/rhu_sysnative.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_ENTROPY_REFUSAL)
- `goldens/shim/shim-sysnative-break-calendar-clock.o`
  `ece6c859b19b3140c7c07ca72160dab2dfca9791d573ade425aa9d56a8e649d8`
  (src/shim/rhu_sysnative.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_CALENDAR_CLOCK)
- `goldens/shim/shim-libc-glue.o`
  `8ffb50ca37ba98ee58a69b71156472609af785bb1dacd5e68bb2103b59ebe267`
  (src/shim/rhu_libc_glue.c; no extra defines)
- `goldens/shim/shim-host.o`
  `573835f6df56af3812736c2c4e686f579941e7a63df0de34a8664a484f6dccd7`
  (src/shim/rhu_host.c; -DMGC_HOST_ROW_AN500)
- `goldens/shim/shim-host-break-dataverify.o`
  `c503fd9d021f22861d50db03514648d802bb10f45c9bdb83b4320cab5dc0d3b3`
  (src/shim/rhu_host.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_DATAVERIFY)
- `goldens/shim/shim-stdio-sym.o`
  `8428ce33a5f364f458b0ccaa949e3398470a9b05500810c6a62109731033a28d`
  (src/shim/rhu_stdio_sym.c; no extra defines)
- `goldens/shim/shimtest-core.o`
  `a2cf4222354aeaf13b42552f705784aae67dca00345782f8881e26545c54403c`
  (src/shim/gate/rhu_shimtest.c; -DRHU_SHIMTEST_LADDER_CORE)
- `goldens/shim/shimtest-core-break-errno.o`
  `e2d742848a43a1a45a5aed1a8af7a40696ac9984de93e19a7179ebb2636f861f`
  (src/shim/gate/rhu_shimtest.c; -DRHU_SHIMTEST_LADDER_CORE -DRHU_SHIMTEST_BREAK_ERRNO)
- `goldens/shim/shimtest-core-break-canary.o`
  `27579c4a76072fe13f2e71cec2b518eb5e24ebacd37f3b2a7a8601a68e2703c5`
  (src/shim/gate/rhu_shimtest.c; -DRHU_SHIMTEST_LADDER_CORE -DRHU_SHIMTEST_BREAK_CANARY)
- `goldens/shim/shimtest-fdmap.o`
  `9cd185e754d7492f508278a809c79b72c8e3bcb9dba420182e3ebd8f8585335f`
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
  `e29367975b5a97b8268eb3c672b684ff83b0fb97d6406bdc62c6d7707524f944`
  (src/shim/rhu_shim.c; -DMGC_HOST_ROW_F769)
- `goldens/shim/shim-core-an505.o`
  `b327eff4fefc537f1a986309fc7b2319a847221e986b3bb77e8821d39ea9f574`
  (src/shim/rhu_shim.c; -DMGC_HOST_ROW_AN505; the Cortex-M33 class flag set)
- `goldens/shim/shim-sysnative-an505.o`
  `b41585fd27a74c8877dc1e5707524d460c8355322c7c0d9651b392d2ffbaa4ef`
  (src/shim/rhu_sysnative.c; -DMGC_HOST_ROW_AN505; the Cortex-M33 class flag set)
- `goldens/shim/shim-libc-glue-an505.o`
  `586fde3e983c35ec2d724ee0b5b692ba61a64552e917a9c6b7d519e69fe609da`
  (src/shim/rhu_libc_glue.c; no extra defines; the Cortex-M33 class flag set)
- `goldens/shim/shim-host-an505.o`
  `e361b86101621e63dc7cdf0a6b58b077cf72457e4937e0211d90075b9a4bb734`
  (src/shim/rhu_host.c; -DMGC_HOST_ROW_AN505; the Cortex-M33 class flag set)
- `goldens/shim/shim-stdio-sym-an505.o`
  `49a42ffaf600fe1315997337de4e9457f6454072c823f8ce3be0d7e4abfb3bb9`
  (src/shim/rhu_stdio_sym.c; no extra defines; the Cortex-M33 class flag set)
- `goldens/shim/shimtest-core.golden.txt`
  `3afa478aaa0da5fdaff61ada500f52eb26c47fbdb2ad8a0e93fb2a48593aa9ac`
  (the payload stream of the shim.core image (class bytes))
- `goldens/shim/shimtest-fdmap.golden.txt`
  `28b974465329510b6aa029d59e62511f9502c7b30d10e4c4e088d967aec15195`
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
