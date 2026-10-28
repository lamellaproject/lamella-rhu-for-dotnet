# The goldens and the release archives

Copyright (c) 2026 Lamella LLC.
SPDX-License-Identifier: MIT OR Apache-2.0

## What the goldens are

The verification chain compares what it builds and runs with frozen expected
outputs, the goldens. They are kept under this directory in five tiers, one
directory per tier. Each tier has its own MANIFEST.md naming every file of
the tier with its SHA256 digest, and the gate that uses the tier checks that
manifest against a digest kept in the harness before its first case.

The garbage-collector tier, the shim tier and the payload tier are in this
directory. The Cortex-M7 hosted-runtime tier and the Cortex-M33
hosted-runtime tier are larger than the source archive allows and are in the
release's archives instead, together with the runtime libraries the
hosted-runtime tiers are rebuilt against.

## What the release archives hold

Each release carries two archives and a SHA256SUMS file beside them.

- `lamella-rhu-1.0.0-alpha.1-goldens.tar` holds the tiers `goldens/rt-m7`
  and `goldens/rt-m33`, a directory per tier. It is unpacked at the
  repository root.
- `lamella-rhu-1.0.0-alpha.1-runtime-archives.tar` holds the runtime library
  sets `net11-rc2-rhu-cortex-m7-sgc-enabled` (Cortex-M7) and
  `net11-rc2-rhu-cortex-m33-sgc-enabled` (Cortex-M33), a directory per set.
  It is unpacked into a directory of your own.

Every archive carries its own MANIFEST.txt at its root: what the archive
holds, every member with its size and SHA256 digest, and the one command
that checks the unpacked files. The sections below repeat each archive's
list, so a checkout verifies a download against the revision it holds.

## How to verify a download

1. Check the downloaded archives: run `sha256sum -c SHA256SUMS` beside them.
   Their digests are in the archive table below.
2. Unpack each archive where its line above says.
3. Check the unpacked files: run `sha256sum -c MANIFEST.txt` where you
   unpacked each archive. Then name each runtime library set's directory
   for the verification chain, as that archive's MANIFEST.txt says.

## The tiers

| Tier | Manifest | Files | Bytes | Where |
|---|---|---|---|---|
| the garbage-collector tier | `goldens/gc/MANIFEST.md` | 63 | 2654281 | in this directory |
| the shim tier | `goldens/shim/MANIFEST.md` | 110 | 4269747 | in this directory |
| the payload tier | `goldens/payload/MANIFEST.md` | 5 | 74718 | in this directory |
| the Cortex-M7 hosted-runtime tier | `goldens/rt-m7/MANIFEST.md` | 17 | 35379849 | in the release archive `lamella-rhu-1.0.0-alpha.1-goldens.tar` |
| the Cortex-M33 hosted-runtime tier | `goldens/rt-m33/MANIFEST.md` | 15 | 22914360 | in the release archive `lamella-rhu-1.0.0-alpha.1-goldens.tar` |

| Manifest | SHA256 |
|---|---|
| `goldens/gc/MANIFEST.md` | `ed993df0212923341938ae884d1a4e4da8eb5b7aa096b2e5d237488944b75ed8` |
| `goldens/shim/MANIFEST.md` | `320e308d956f51d37bbec0e6c4ee3057134772168032d28ad07048c01302fe44` |
| `goldens/payload/MANIFEST.md` | `6c43bc48a345f5e68daa88a23d2f57e6f22821170364a8490407c7d348d08eec` |
| `goldens/rt-m7/MANIFEST.md` | `7c133332cd22c00c4f35a160f64db0a0daebe9141e816b7957e341783a7481cf` |
| `goldens/rt-m33/MANIFEST.md` | `3b9e56a1251d987ff4adea3cd92903d013e9f36dacc7beedfd6b13c68fa3fb94` |

## The release archives

| Archive | Members | Bytes | SHA256 |
|---|---|---|---|
| `lamella-rhu-1.0.0-alpha.1-goldens.tar` | 33 | 58326016 | `2de8d099d14d56e5f6ba9e94feddbe7a2ebeb4aec5eba8dd153827c6e647d1c0` |
| `lamella-rhu-1.0.0-alpha.1-runtime-archives.tar` | 11 | 11142144 | `3b5a2b1c8e6c5e0bced182e25e004bda0330cddc08727fe876c7dc095fbb70f5` |

### lamella-rhu-1.0.0-alpha.1-goldens.tar

33 members, 58326016 bytes. The block lists every member in the archive's
order with its SHA256 digest. Its first line is the archive's own
MANIFEST.txt; the rest is that file's text, as the archive carries it, so a
copy of the block checks the unpacked files the same way.

```
3526a96480db428e7a6e90a511c76e75917d9f17bae2d434f347b7f420dc43a0  MANIFEST.txt
# lamella-rhu-1.0.0-alpha.1-goldens.tar
#
# This archive holds two of the five frozen tiers of expected outputs
# that the verification chain of Lamella RHU compares its results with,
# each in its own directory: goldens/rt-m7 (the Cortex-M7 hosted-runtime
# tier) and goldens/rt-m33 (the Cortex-M33 hosted-runtime tier). The
# other tiers are in the repository under goldens/.
#
# Unpack this archive at the root of a checkout of the repository. Then
# check every unpacked file against this list, from that root:
#
#     sha256sum -c MANIFEST.txt
#
# The members and their sizes in bytes:
#
#      14742  goldens/rt-m33/MANIFEST.md
#       4490  goldens/rt-m33/archives-net11-rc2-rhu-cortex-m33-sgc-enabled.txt
#         76  goldens/rt-m33/attr-donor-v8m.bin
#         60  goldens/rt-m33/control-attr-a-profile.bin
#        928  goldens/rt-m33/control-d32-site.o
#        924  goldens/rt-m33/control-ldrexd-site.o
#       5789  goldens/rt-m33/rt-libc.members.txt
#     141376  goldens/rt-m33/rt-libc.o
#    7530932  goldens/rt-m33/rthello-merged.o
#       3414  goldens/rt-m33/rthello-merged.undef.txt
#        167  goldens/rt-m33/rthello.golden.txt
#   15167852  goldens/rt-m33/rtsuite-merged.o
#       5584  goldens/rt-m33/rtsuite-merged.undef.txt
#       2634  goldens/rt-m33/rtsuite.golden.txt
#      35392  goldens/rt-m33/startup-cookie-copied.o
#      19166  goldens/rt-m7/MANIFEST.md
#       4480  goldens/rt-m7/archives-net11-rc2-rhu-cortex-m7-sgc-enabled.txt
#         50  goldens/rt-m7/attr-donor-v7em.bin
#         60  goldens/rt-m7/control-attr-a-profile.bin
#        928  goldens/rt-m7/control-d32-site.o
#        924  goldens/rt-m7/control-ldrexd-site.o
#       5822  goldens/rt-m7/rt-libc.members.txt
#     134562  goldens/rt-m7/rt-libc.o
#    7543618  goldens/rt-m7/rthello-merged.o
#       3196  goldens/rt-m7/rthello-merged.undef.txt
#        167  goldens/rt-m7/rthello.golden.txt
#    2608051  goldens/rt-m7/rthello.o
#   15180546  goldens/rt-m7/rtsuite-merged.o
#       5366  goldens/rt-m7/rtsuite-merged.undef.txt
#       2634  goldens/rt-m7/rtsuite.golden.txt
#    9834875  goldens/rt-m7/rtsuite.o
#      35404  goldens/rt-m7/startup-cookie-copied.o
#
# Each line below is a member's SHA256 digest, two spaces and its path.
# The lines above, which start with a number sign, are comments that
# sha256sum skips.
#
3b9e56a1251d987ff4adea3cd92903d013e9f36dacc7beedfd6b13c68fa3fb94  goldens/rt-m33/MANIFEST.md
11d4223702deedd89f84676e49160a53947bb22c0a3168ee8802da5a92d5f221  goldens/rt-m33/archives-net11-rc2-rhu-cortex-m33-sgc-enabled.txt
e8ebecc9e0784f841ca9f09a6bfa415ad9d04431976db2d0fad100a74e81c3b1  goldens/rt-m33/attr-donor-v8m.bin
5c1121c6df6659326c8e22a43d54080ea3dc2f495078f57e375625d93349e6ca  goldens/rt-m33/control-attr-a-profile.bin
762ff2df1f0504c23347bdb47435b6ed48c3932c678a455aada60c994a441f14  goldens/rt-m33/control-d32-site.o
e29d4d1c92e619fb9e2b53e50a52ec1b643c5902a353b5b9108beaf54b0cf533  goldens/rt-m33/control-ldrexd-site.o
d07a3ab13c7de6e761a8a880b4c606e1b35f493ffc19ca51c238607422ed6347  goldens/rt-m33/rt-libc.members.txt
787fa5a3f597591438811736480ac183d88305654a26fd5648ebf160224795fe  goldens/rt-m33/rt-libc.o
d8b44ebadfdee553bbe51cf45eb4b350d31f2e13c5c9cb563e700f8f7d6f96f4  goldens/rt-m33/rthello-merged.o
e4e032fb62f715d7173e73a81088f3581aa85c973c1a39422f3156c11a74b8de  goldens/rt-m33/rthello-merged.undef.txt
66c7f87ca29b27cf4b8ed05c8fb2d0eb30d16b48375325ec65c726e835a5fa7b  goldens/rt-m33/rthello.golden.txt
bbc89f0d73c01659439bbb2ec46cb526296e885134895e678534fc537ef26c66  goldens/rt-m33/rtsuite-merged.o
481be2b4fb378b52d104ef32ab5eea9416cb47be99c3dbb838de3df424951cf1  goldens/rt-m33/rtsuite-merged.undef.txt
c61409238d54da2bb6d38de937a48a72a5077c80adabda215dade67f4bc54632  goldens/rt-m33/rtsuite.golden.txt
f1cbb245803a82240435a283b197f671b1550651f5a70b964817802f6a37e662  goldens/rt-m33/startup-cookie-copied.o
7c133332cd22c00c4f35a160f64db0a0daebe9141e816b7957e341783a7481cf  goldens/rt-m7/MANIFEST.md
b1e82b9e578f83b99788185d85476469319dd4982771dd4d212ba82eec6c66d7  goldens/rt-m7/archives-net11-rc2-rhu-cortex-m7-sgc-enabled.txt
f0806a5d3672b24489ff1c896f7e3b74e3c3d1fe030bcacd9f010ee3cdf5216d  goldens/rt-m7/attr-donor-v7em.bin
5c1121c6df6659326c8e22a43d54080ea3dc2f495078f57e375625d93349e6ca  goldens/rt-m7/control-attr-a-profile.bin
762ff2df1f0504c23347bdb47435b6ed48c3932c678a455aada60c994a441f14  goldens/rt-m7/control-d32-site.o
e29d4d1c92e619fb9e2b53e50a52ec1b643c5902a353b5b9108beaf54b0cf533  goldens/rt-m7/control-ldrexd-site.o
12f814c7d5d6d310ba9643d1f2fbdbf82dac2a9491ff4cd06e89f37f1eeacb5f  goldens/rt-m7/rt-libc.members.txt
536d99f34384bde7003e0b9c3dd52df97739977254e1eae4c1d12f5e623fcacf  goldens/rt-m7/rt-libc.o
d189d3d6279bec42b2dbec337e907daa700330c1189ba25d8c936e2698485b41  goldens/rt-m7/rthello-merged.o
6e6696c5a4606ba86ee8e70b7e29a763797669ec0dfaa19e4364fd75a94eaaee  goldens/rt-m7/rthello-merged.undef.txt
66c7f87ca29b27cf4b8ed05c8fb2d0eb30d16b48375325ec65c726e835a5fa7b  goldens/rt-m7/rthello.golden.txt
c5e8727a9e5ef1215f14aaca0d247a015cf1e8fbd21a06c294cdede8bf612008  goldens/rt-m7/rthello.o
88196e54593d27084810c70c4775d65c662275a76e11830078a0abf1d6d9838a  goldens/rt-m7/rtsuite-merged.o
349a11704291142a9de061df1e6ffc292ce20560ccb8ce3e62c030ddc709b856  goldens/rt-m7/rtsuite-merged.undef.txt
c61409238d54da2bb6d38de937a48a72a5077c80adabda215dade67f4bc54632  goldens/rt-m7/rtsuite.golden.txt
de71fbc159273766b8896084debf9fff0c5d6038c7fda2399be36a6aef3ee5ab  goldens/rt-m7/rtsuite.o
e3c15b19cd5798e0c34cf3524497e8f0cd52e57559715622f7421a4f93a8d53c  goldens/rt-m7/startup-cookie-copied.o
```

### lamella-rhu-1.0.0-alpha.1-runtime-archives.tar

11 members, 11142144 bytes. The block lists every member in the archive's
order with its SHA256 digest. Its first line is the archive's own
MANIFEST.txt; the rest is that file's text, as the archive carries it, so a
copy of the block checks the unpacked files the same way.

```
c93da396f175b3c8e02bca03aba818717f786acd443e048d55fcb43d7c3aca48  MANIFEST.txt
# lamella-rhu-1.0.0-alpha.1-runtime-archives.tar
#
# This archive holds two sets of .NET runtime libraries that Lamella RHU
# built for its processor classes, each in its own directory:
# net11-rc2-rhu-cortex-m7-sgc-enabled (Cortex-M7) and
# net11-rc2-rhu-cortex-m33-sgc-enabled (Cortex-M33). The hosted-runtime
# tiers are rebuilt against these libraries. How each set was built is
# recorded beside the tier that uses it, in
# goldens/rt-m7/archives-net11-rc2-rhu-cortex-m7-sgc-enabled.txt and
# goldens/rt-m33/archives-net11-rc2-rhu-cortex-m33-sgc-enabled.txt.
#
# Unpack this archive into a directory of your own. Then check every
# unpacked file against this list, from that directory:
#
#     sha256sum -c MANIFEST.txt
#
# Name the Cortex-M7 set's directory for the verification chain: put its
# path (the directory you unpacked into, then
# net11-rc2-rhu-cortex-m7-sgc-enabled) into
# tools/toolchains/rtarchives-m7.local, or set RHU_RT_ARCHIVES_M7 to it.
#
# Name the Cortex-M33 set's directory for the verification chain: put
# its path (the directory you unpacked into, then
# net11-rc2-rhu-cortex-m33-sgc-enabled) into
# tools/toolchains/rtarchives-m33.local, or set RHU_RT_ARCHIVES_M33 to
# it.
#
# The members and their sizes in bytes:
#
#    5055834  net11-rc2-rhu-cortex-m33-sgc-enabled/libRuntime.WorkstationGC.a
#     169324  net11-rc2-rhu-cortex-m33-sgc-enabled/libaotminipal.a
#     222138  net11-rc2-rhu-cortex-m33-sgc-enabled/libeventpipe-disabled.a
#     101254  net11-rc2-rhu-cortex-m33-sgc-enabled/libstandalonegc-enabled.a
#       9734  net11-rc2-rhu-cortex-m33-sgc-enabled/libstdc++compat.a
#    5067648  net11-rc2-rhu-cortex-m7-sgc-enabled/libRuntime.WorkstationGC.a
#     170128  net11-rc2-rhu-cortex-m7-sgc-enabled/libaotminipal.a
#     222126  net11-rc2-rhu-cortex-m7-sgc-enabled/libeventpipe-disabled.a
#     101454  net11-rc2-rhu-cortex-m7-sgc-enabled/libstandalonegc-enabled.a
#       9754  net11-rc2-rhu-cortex-m7-sgc-enabled/libstdc++compat.a
#
# Each line below is a member's SHA256 digest, two spaces and its path.
# The lines above, which start with a number sign, are comments that
# sha256sum skips.
#
1d177b20eaf1f976666c20950292f5629d46e9aa0e34953f4ef4a43b00fd39a1  net11-rc2-rhu-cortex-m33-sgc-enabled/libRuntime.WorkstationGC.a
1b2234511db7e42d7f9becd97b7969d57e3a441da490d7d5e9778e0b9a2f1ac2  net11-rc2-rhu-cortex-m33-sgc-enabled/libaotminipal.a
c46b9cc02766a3b85ecee29e61dff98b3b2e210f37e681ddfd164bf0a1f200f2  net11-rc2-rhu-cortex-m33-sgc-enabled/libeventpipe-disabled.a
0f1fe21a7eed484596eb5dd7aa01b1971283e351ed28f04b58f12a111b964d22  net11-rc2-rhu-cortex-m33-sgc-enabled/libstandalonegc-enabled.a
5c8af92e1445285951d30b53df5be5a93a650cc0661b80ed6f7bf89f37688113  net11-rc2-rhu-cortex-m33-sgc-enabled/libstdc++compat.a
7822045ee5ac157e1f1f6ecc57f757c89ed65d8eb3b3e79e514138a6f3bca40b  net11-rc2-rhu-cortex-m7-sgc-enabled/libRuntime.WorkstationGC.a
679954796d867ff5aec06fd8651ac0974d1fe7fdaf721f2dd86285464a1826c6  net11-rc2-rhu-cortex-m7-sgc-enabled/libaotminipal.a
552eb2ecbc2b1d87e906db814cba1dfa3553391727c25e7c4b906c53076ee1bd  net11-rc2-rhu-cortex-m7-sgc-enabled/libeventpipe-disabled.a
4e4b8b2cf6034756fbbdf3240fa0208356a0b808785c2d5d51dd83a540f7df2b  net11-rc2-rhu-cortex-m7-sgc-enabled/libstandalonegc-enabled.a
362e7a5e41290dcbef5db1905a6528b5cbe8302d04696097e48776b31d56f999  net11-rc2-rhu-cortex-m7-sgc-enabled/libstdc++compat.a
```

## How the archives are made

Each archive is a POSIX ustar archive written by the harness (`rhu-gate
release pack`): the members sorted bytewise by path, mode 0644, owner and
group 0, modification time 0, no owner names, no directory entries. GNU tar
writes the same bytes from the unpacked members:

    tar --format=ustar -b 1 --owner=0 --group=0 --numeric-owner --mtime=@0 --mode=u=rw,go=r -cf <archive> <the members in the order above>

The same tree writes the same bytes on every host. The harness's selfcheck
rebuilds the archives from the tree and refuses a record that does not
match them. This record is written by `rhu-gate release manifest` and is
regenerated in the same commit as any change of a tier.
