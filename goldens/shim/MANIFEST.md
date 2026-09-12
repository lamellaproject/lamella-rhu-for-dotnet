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
generation 33; the mps2-an505 row's five link objects are the an500 row's
sources and defines compiled for the Cortex-M33 (the row's fields equal
the an500's, so the class flag set is the one variable), each carrying no
double-precision instruction; the shim answers the pinned runtime pack's
whole P/Invoke-facing native surface (the five class-library families'
export tables, 757 names at this pin): the names the hand-written units
serve over the RAM file store, the kernel's calendar and its secure door,
and for every name of an area this system does not serve a generated
refusal (nothing printed, nothing trapped; the errno of record where the
convention is errno-style) in the surface object beside the five link
objects, with its Cortex-M33 twin for the an505 row; the expectation the
gate's surface case reads -- generated from the pack's archives and the
runtime's headers at the pin, never typed -- is the tier's text record,
tied to the pin and to every family name the objects define; 36
objects, 5 goldens and 1 record, byte-tied by the rederive
leg at the freeze.

Records (repo-relative path, sha256; source and defines per object):

- `goldens/shim/shim-core.o`
  `496ab1bc2f7534c36007e8c0010095fe13641a1ad3c2b32a98c8f6cfbf882c34`
  (src/shim/rhu_shim.c; -DMGC_HOST_ROW_AN500)
- `goldens/shim/shim-core-break-sipid.o`
  `e132e31f424b3cb683da0221d4ac664ae5a6b6c36f48ca891fb64d6bcb125b0c`
  (src/shim/rhu_shim.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_SIPID)
- `goldens/shim/shim-core-break-ctxpc.o`
  `f70ffd9d5c65617ece2df08c419ada7b2e22253cc6e286b518ce11d5def6c69a`
  (src/shim/rhu_shim.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_CTXPC)
- `goldens/shim/shim-core-break-tlsoff.o`
  `1d5cd933a58c8085df8cf0bbee1cbd383276a1fd384ad653c729b87eeb86b1b4`
  (src/shim/rhu_shim.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_TLSOFF)
- `goldens/shim/shim-core-break-tlscache.o`
  `4d0b93ea4e7bf0d35467ad621da0847bccb5be9960c05fb76d080dab775cc0c0`
  (src/shim/rhu_shim.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_TLSCACHE)
- `goldens/shim/shim-sysnative.o`
  `cac8eaad8c5d77b9221383dad35b96d22be34a36213cc1be133e42ff9167cc35`
  (src/shim/rhu_sysnative.c; -DMGC_HOST_ROW_AN500)
- `goldens/shim/shim-sysnative-break-fdbase.o`
  `c5e8760710c36f4a4a69490caf13f83d66cd67e6fff1a40a87fb3a4994a6d70d`
  (src/shim/rhu_sysnative.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_FD_BASE)
- `goldens/shim/shim-sysnative-break-store-bounds.o`
  `42b685b27763276ac250cd91a8fcbbf0b6ffed44afa8ced1f159415bfa6dd9f0`
  (src/shim/rhu_sysnative.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_STORE_BOUNDS)
- `goldens/shim/shim-sysnative-break-store-lifetime.o`
  `785d333735bb5dbd3359b550a8782c6dd47fd7461f398e4c05f7b2d46e3f5fd9`
  (src/shim/rhu_sysnative.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_STORE_LIFETIME)
- `goldens/shim/shim-sysnative-break-store-lock.o`
  `d56ca99cf630ec6ae02c9b28ca56097386ab48f8652abee88e6018926d566c09`
  (src/shim/rhu_sysnative.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_STORE_LOCK)
- `goldens/shim/shim-sysnative-break-store-open-order.o`
  `0e53ed5e72648f4abebe4c9c4240318bfa1f1883d3ffb5a26e697cf103baf502`
  (src/shim/rhu_sysnative.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_STORE_OPEN_ORDER)
- `goldens/shim/shim-sysnative-break-store-access.o`
  `521fde3667486ce5cb82d047214ca596338da4d46d31149807b1f41affecfdba`
  (src/shim/rhu_sysnative.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_STORE_ACCESS)
- `goldens/shim/shim-sysnative-break-store-flock.o`
  `1d965c1d18ba904c077535ef431080f77d10aa9727ed1cc6d5284c76dfee27dd`
  (src/shim/rhu_sysnative.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_STORE_FLOCK)
- `goldens/shim/shim-sysnative-break-store-slash.o`
  `3ba27a45d222ae1060a05f25f111b452d8bd41f86f9df3efb155004ce15c67c5`
  (src/shim/rhu_sysnative.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_STORE_SLASH)
- `goldens/shim/shim-sysnative-break-store-times.o`
  `d1aff9dfe19ac6ff292ae9d67347deed1a45e0ab2214adce3e3ad93491d32c80`
  (src/shim/rhu_sysnative.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_STORE_TIMES)
- `goldens/shim/shim-sysnative-break-entropy-refusal.o`
  `a1d17969916384ba438fb27a556dd02beed6ea336960091926e1bd81047045fc`
  (src/shim/rhu_sysnative.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_ENTROPY_REFUSAL)
- `goldens/shim/shim-sysnative-break-calendar-clock.o`
  `c695e5de2f2c5ad0148b656971f81a7d09b40d99f7843071e73df79c14edb8ee`
  (src/shim/rhu_sysnative.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_CALENDAR_CLOCK)
- `goldens/shim/shim-libc-glue.o`
  `8ffb50ca37ba98ee58a69b71156472609af785bb1dacd5e68bb2103b59ebe267`
  (src/shim/rhu_libc_glue.c; no extra defines)
- `goldens/shim/shim-host.o`
  `696b047d428b67ba1503de6a0b67a005f105fc139ea11bc5cfcde18bc6bb0033`
  (src/shim/rhu_host.c; -DMGC_HOST_ROW_AN500)
- `goldens/shim/shim-host-break-dataverify.o`
  `fae0129a212520221e35d60e0ae736a89621b593c28dd7510226390b70db19a0`
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
  `00239bffb34b4e16db54f6bc13ce3f4ffafd163b50c18ccfc399bbfda01f13a9`
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
  `65f7da6ad2923c1f851021793310bfaabf574f0372cc1c9f65f324c1c390ce7d`
  (src/shim/rhu_shim.c; -DMGC_HOST_ROW_F769)
- `goldens/shim/shim-core-an505.o`
  `ddf55edd85bdb6bdba480a381b8f39294fa410bd1b00a201cc3ed49789c4dd33`
  (src/shim/rhu_shim.c; -DMGC_HOST_ROW_AN505; the Cortex-M33 class flag set)
- `goldens/shim/shim-sysnative-an505.o`
  `f797c034b791e8966487e1247da0094268d8bb16cc137a5cf374c01ddee735df`
  (src/shim/rhu_sysnative.c; -DMGC_HOST_ROW_AN505; the Cortex-M33 class flag set)
- `goldens/shim/shim-libc-glue-an505.o`
  `586fde3e983c35ec2d724ee0b5b692ba61a64552e917a9c6b7d519e69fe609da`
  (src/shim/rhu_libc_glue.c; no extra defines; the Cortex-M33 class flag set)
- `goldens/shim/shim-host-an505.o`
  `91bef3e9759203cc03af25af9c8a8cb0850bf0eb9a69cf8f091ea930adf1a8d2`
  (src/shim/rhu_host.c; -DMGC_HOST_ROW_AN505; the Cortex-M33 class flag set)
- `goldens/shim/shim-stdio-sym-an505.o`
  `49a42ffaf600fe1315997337de4e9457f6454072c823f8ce3be0d7e4abfb3bb9`
  (src/shim/rhu_stdio_sym.c; no extra defines; the Cortex-M33 class flag set)
- `goldens/shim/shim-pal-surface.o`
  `ba588dc52d48b2df933c27836f398783917a371933d62be18c10927f3842750c`
  (src/shim/rhu_pal_surface_net11_rc1.c; -DMGC_HOST_ROW_AN500)
- `goldens/shim/shim-pal-surface-an505.o`
  `3e49d989d3b8626b510b1b529fe043a26b9195203feb6b6e7045cb48efe73499`
  (src/shim/rhu_pal_surface_net11_rc1.c; -DMGC_HOST_ROW_AN505; the Cortex-M33 class flag set)
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
- `goldens/shim/pal-surface.txt`
  `1835f4cb31b605811c3d8e17356510a5e79a200849352ad540ce9fcf14a4020e`
  (the P/Invoke-facing surface expectation the shim.pal-surface case reads: the pin, the pack's five archives with their sha256s, one line per surface name with its family and class; GENERATED from the pinned pack's archives and the runtime's headers, regenerated at a re-pin, never edited)
