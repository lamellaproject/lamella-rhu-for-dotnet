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

- `lamella-rhu-1.0.0-alpha.2-goldens.tar` holds the tiers `goldens/rt-m7`
  and `goldens/rt-m33`, a directory per tier. It is unpacked at the
  repository root.
- `lamella-rhu-1.0.0-alpha.2-runtime-archives.tar` holds the runtime library
  sets `net11-rc3-rhu-cortex-m7-sgc-enabled` (Cortex-M7) and
  `net11-rc3-rhu-cortex-m33-sgc-enabled` (Cortex-M33), a directory per set.
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
| the shim tier | `goldens/shim/MANIFEST.md` | 115 | 4913504 | in this directory |
| the payload tier | `goldens/payload/MANIFEST.md` | 5 | 74718 | in this directory |
| the Cortex-M7 hosted-runtime tier | `goldens/rt-m7/MANIFEST.md` | 17 | 35490367 | in the release archive `lamella-rhu-1.0.0-alpha.2-goldens.tar` |
| the Cortex-M33 hosted-runtime tier | `goldens/rt-m33/MANIFEST.md` | 15 | 22972017 | in the release archive `lamella-rhu-1.0.0-alpha.2-goldens.tar` |

| Manifest | SHA256 |
|---|---|
| `goldens/gc/MANIFEST.md` | `e10ccc9b0822a4c40a8d7d00093dd49e95310dcd5776783bda7bf6df1f70db21` |
| `goldens/shim/MANIFEST.md` | `fb44374bb9565f1e396eb73b12a042bed253183d521cabc63966ce1c08ba41d9` |
| `goldens/payload/MANIFEST.md` | `c6cae88b10d135af4f05baa52b44c4f297f051d00fb3761d001ece7acf6e8bde` |
| `goldens/rt-m7/MANIFEST.md` | `26af45129561e224edae764e9c974a02b930ed5c36791c714232d3f7b8c92f1a` |
| `goldens/rt-m33/MANIFEST.md` | `bc558252e4d6d3b9a2331ad70716ca1f8b09e2cfb227fdcf3ed979d10f88f40d` |

## The release archives

| Archive | Members | Bytes | SHA256 |
|---|---|---|---|
| `lamella-rhu-1.0.0-alpha.2-goldens.tar` | 33 | 58493952 | `f490aed5faf246cb25746e300baeeba0b3f8b51c6d6c3e09391cd6fd5da84421` |
| `lamella-rhu-1.0.0-alpha.2-runtime-archives.tar` | 11 | 11142144 | `312818350ef277e267d0cf8f5d1d2a46f5f566b5a4cabcc6f4798ac52270a173` |

### lamella-rhu-1.0.0-alpha.2-goldens.tar

33 members, 58493952 bytes. The block lists every member in the archive's
order with its SHA256 digest. Its first line is the archive's own
MANIFEST.txt; the rest is that file's text, as the archive carries it, so a
copy of the block checks the unpacked files the same way.

```
0835150192ee81188e13241567b0c021aae90cfdb07e7993b332c56472924619  MANIFEST.txt
# lamella-rhu-1.0.0-alpha.2-goldens.tar
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
#      14963  goldens/rt-m33/MANIFEST.md
#       4486  goldens/rt-m33/archives-net11-rc3-rhu-cortex-m33-sgc-enabled.txt
#         76  goldens/rt-m33/attr-donor-v8m.bin
#         60  goldens/rt-m33/control-attr-a-profile.bin
#        928  goldens/rt-m33/control-d32-site.o
#        924  goldens/rt-m33/control-ldrexd-site.o
#       5789  goldens/rt-m33/rt-libc.members.txt
#     141376  goldens/rt-m33/rt-libc.o
#    7530932  goldens/rt-m33/rthello-merged.o
#       3414  goldens/rt-m33/rthello-merged.undef.txt
#        167  goldens/rt-m33/rthello.golden.txt
#   15224704  goldens/rt-m33/rtsuite-merged.o
#       6027  goldens/rt-m33/rtsuite-merged.undef.txt
#       2779  goldens/rt-m33/rtsuite.golden.txt
#      35392  goldens/rt-m33/startup-cookie-copied.o
#      19444  goldens/rt-m7/MANIFEST.md
#       4475  goldens/rt-m7/archives-net11-rc3-rhu-cortex-m7-sgc-enabled.txt
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
#   15237390  goldens/rt-m7/rtsuite-merged.o
#       5809  goldens/rt-m7/rtsuite-merged.undef.txt
#       2779  goldens/rt-m7/rtsuite.golden.txt
#    9887688  goldens/rt-m7/rtsuite.o
#      35404  goldens/rt-m7/startup-cookie-copied.o
#
# Each line below is a member's SHA256 digest, two spaces and its path.
# The lines above, which start with a number sign, are comments that
# sha256sum skips.
#
bc558252e4d6d3b9a2331ad70716ca1f8b09e2cfb227fdcf3ed979d10f88f40d  goldens/rt-m33/MANIFEST.md
b2bf20c072493861ec2890ed79acfacaaac35432511c521e9e8804cd21f2d64f  goldens/rt-m33/archives-net11-rc3-rhu-cortex-m33-sgc-enabled.txt
e8ebecc9e0784f841ca9f09a6bfa415ad9d04431976db2d0fad100a74e81c3b1  goldens/rt-m33/attr-donor-v8m.bin
5c1121c6df6659326c8e22a43d54080ea3dc2f495078f57e375625d93349e6ca  goldens/rt-m33/control-attr-a-profile.bin
762ff2df1f0504c23347bdb47435b6ed48c3932c678a455aada60c994a441f14  goldens/rt-m33/control-d32-site.o
e29d4d1c92e619fb9e2b53e50a52ec1b643c5902a353b5b9108beaf54b0cf533  goldens/rt-m33/control-ldrexd-site.o
d07a3ab13c7de6e761a8a880b4c606e1b35f493ffc19ca51c238607422ed6347  goldens/rt-m33/rt-libc.members.txt
787fa5a3f597591438811736480ac183d88305654a26fd5648ebf160224795fe  goldens/rt-m33/rt-libc.o
926cd9c3e3ef14cf142ed8aded012b1de6b7c894ec4867060b2f7091c07e24f6  goldens/rt-m33/rthello-merged.o
e4e032fb62f715d7173e73a81088f3581aa85c973c1a39422f3156c11a74b8de  goldens/rt-m33/rthello-merged.undef.txt
66c7f87ca29b27cf4b8ed05c8fb2d0eb30d16b48375325ec65c726e835a5fa7b  goldens/rt-m33/rthello.golden.txt
31b5ffc8687fdeb357e86467db7acff74323f35f0bdb1e512c4349e6611b42d6  goldens/rt-m33/rtsuite-merged.o
f616d9623b94cb5382574b28118e312e516787ff07f7202058f3375720e5e0e6  goldens/rt-m33/rtsuite-merged.undef.txt
8a3f0dcdccf41c2c0b2c0b4ba08fed8a44269c96b6a905cf5afb3b10e33ca325  goldens/rt-m33/rtsuite.golden.txt
51530c50da7f4bff7d37eabfcdf3ce9b2488dc5f1480f301bfe0dbfa280c6805  goldens/rt-m33/startup-cookie-copied.o
26af45129561e224edae764e9c974a02b930ed5c36791c714232d3f7b8c92f1a  goldens/rt-m7/MANIFEST.md
3bc101c75e3c3e06f0f8d3f60873e3b9d06251bc636eab75135e42bb3a5a7b46  goldens/rt-m7/archives-net11-rc3-rhu-cortex-m7-sgc-enabled.txt
f0806a5d3672b24489ff1c896f7e3b74e3c3d1fe030bcacd9f010ee3cdf5216d  goldens/rt-m7/attr-donor-v7em.bin
5c1121c6df6659326c8e22a43d54080ea3dc2f495078f57e375625d93349e6ca  goldens/rt-m7/control-attr-a-profile.bin
762ff2df1f0504c23347bdb47435b6ed48c3932c678a455aada60c994a441f14  goldens/rt-m7/control-d32-site.o
e29d4d1c92e619fb9e2b53e50a52ec1b643c5902a353b5b9108beaf54b0cf533  goldens/rt-m7/control-ldrexd-site.o
12f814c7d5d6d310ba9643d1f2fbdbf82dac2a9491ff4cd06e89f37f1eeacb5f  goldens/rt-m7/rt-libc.members.txt
536d99f34384bde7003e0b9c3dd52df97739977254e1eae4c1d12f5e623fcacf  goldens/rt-m7/rt-libc.o
15bfb99277a36ff8f8d6d958f28f29b5474426bef33d24ad74a9c0a95e8ab2f3  goldens/rt-m7/rthello-merged.o
6e6696c5a4606ba86ee8e70b7e29a763797669ec0dfaa19e4364fd75a94eaaee  goldens/rt-m7/rthello-merged.undef.txt
66c7f87ca29b27cf4b8ed05c8fb2d0eb30d16b48375325ec65c726e835a5fa7b  goldens/rt-m7/rthello.golden.txt
0906b4d3aeac24dd451cbab3f899e86c9f3bcac6df62ce534006edd771dc3a9c  goldens/rt-m7/rthello.o
40e40d3dd280148b95e0a5152f74971bbbdbff5da15da2977ddffff8461c77a1  goldens/rt-m7/rtsuite-merged.o
0b5b3ed43b2b1732bdb5b1120f28431dbac03a916673e75ca81264427bc9ac98  goldens/rt-m7/rtsuite-merged.undef.txt
8a3f0dcdccf41c2c0b2c0b4ba08fed8a44269c96b6a905cf5afb3b10e33ca325  goldens/rt-m7/rtsuite.golden.txt
33d47d9a4949171617332d4c1fc903e9f7b08918a6d64cf33ba8d1823ddb2a1e  goldens/rt-m7/rtsuite.o
b815a377c4fccad85f421bae165fbebdea08c8be66abb870e2777845ff28d351  goldens/rt-m7/startup-cookie-copied.o
```

### lamella-rhu-1.0.0-alpha.2-runtime-archives.tar

11 members, 11142144 bytes. The block lists every member in the archive's
order with its SHA256 digest. Its first line is the archive's own
MANIFEST.txt; the rest is that file's text, as the archive carries it, so a
copy of the block checks the unpacked files the same way.

```
b66cceb76c395db15ffcf873a1d02c9b86e45998ea306dbf8c6fefd45226664d  MANIFEST.txt
# lamella-rhu-1.0.0-alpha.2-runtime-archives.tar
#
# This archive holds two sets of .NET runtime libraries that Lamella RHU
# built for its processor classes, each in its own directory:
# net11-rc3-rhu-cortex-m7-sgc-enabled (Cortex-M7) and
# net11-rc3-rhu-cortex-m33-sgc-enabled (Cortex-M33). The hosted-runtime
# tiers are rebuilt against these libraries. How each set was built is
# recorded beside the tier that uses it, in
# goldens/rt-m7/archives-net11-rc3-rhu-cortex-m7-sgc-enabled.txt and
# goldens/rt-m33/archives-net11-rc3-rhu-cortex-m33-sgc-enabled.txt.
#
# Unpack this archive into a directory of your own. Then check every
# unpacked file against this list, from that directory:
#
#     sha256sum -c MANIFEST.txt
#
# Name the Cortex-M7 set's directory for the verification chain: put its
# path (the directory you unpacked into, then
# net11-rc3-rhu-cortex-m7-sgc-enabled) into
# tools/toolchains/rtarchives-m7.local, or set RHU_RT_ARCHIVES_M7 to it.
#
# Name the Cortex-M33 set's directory for the verification chain: put
# its path (the directory you unpacked into, then
# net11-rc3-rhu-cortex-m33-sgc-enabled) into
# tools/toolchains/rtarchives-m33.local, or set RHU_RT_ARCHIVES_M33 to
# it.
#
# The members and their sizes in bytes:
#
#    5055834  net11-rc3-rhu-cortex-m33-sgc-enabled/libRuntime.WorkstationGC.a
#     169324  net11-rc3-rhu-cortex-m33-sgc-enabled/libaotminipal.a
#     222138  net11-rc3-rhu-cortex-m33-sgc-enabled/libeventpipe-disabled.a
#     101254  net11-rc3-rhu-cortex-m33-sgc-enabled/libstandalonegc-enabled.a
#       9734  net11-rc3-rhu-cortex-m33-sgc-enabled/libstdc++compat.a
#    5067648  net11-rc3-rhu-cortex-m7-sgc-enabled/libRuntime.WorkstationGC.a
#     170128  net11-rc3-rhu-cortex-m7-sgc-enabled/libaotminipal.a
#     222126  net11-rc3-rhu-cortex-m7-sgc-enabled/libeventpipe-disabled.a
#     101454  net11-rc3-rhu-cortex-m7-sgc-enabled/libstandalonegc-enabled.a
#       9754  net11-rc3-rhu-cortex-m7-sgc-enabled/libstdc++compat.a
#
# Each line below is a member's SHA256 digest, two spaces and its path.
# The lines above, which start with a number sign, are comments that
# sha256sum skips.
#
c2f2c2fa6f4f252468c801892d7f86da0c349d00e1e6c4ffccb9166c8fdc3d40  net11-rc3-rhu-cortex-m33-sgc-enabled/libRuntime.WorkstationGC.a
299bfc5af734c2f16612520656d198de2d154e2169bd8695d5db7fd3ac4da67c  net11-rc3-rhu-cortex-m33-sgc-enabled/libaotminipal.a
c6397dc58ef42dbac80d9b58fc3bef448a2f601a7ae72f477f38ed0c059d562a  net11-rc3-rhu-cortex-m33-sgc-enabled/libeventpipe-disabled.a
f42dd3711cd71a09a7aea336ee0a0388f0f978e31d3f98c07dabc65ed3c1668d  net11-rc3-rhu-cortex-m33-sgc-enabled/libstandalonegc-enabled.a
a5f54fa2bc70b124eccce7a45dfadfd67be2c1c817031f6f77a449d84371a641  net11-rc3-rhu-cortex-m33-sgc-enabled/libstdc++compat.a
0eec549a9595ba0de120ec00fb0343bfa7f0faca070ef7b68014c0366c1c3f69  net11-rc3-rhu-cortex-m7-sgc-enabled/libRuntime.WorkstationGC.a
14f323ae1948568a933ce9dabb320ee82669203096c2ddfe7eb2905f2906cbab  net11-rc3-rhu-cortex-m7-sgc-enabled/libaotminipal.a
1620d51a23c2bc356880ea8256f5280dea58826c9dd358fe6ce3252e1740e7ff  net11-rc3-rhu-cortex-m7-sgc-enabled/libeventpipe-disabled.a
03cb710b0fc73ae9d9634cca163c2902d18898da762d981685946c4e04322bf2  net11-rc3-rhu-cortex-m7-sgc-enabled/libstandalonegc-enabled.a
fc71fcd4daa1c19eeb09ae0fa021fbf64d7a12b514d275397bcb03e148e103b0  net11-rc3-rhu-cortex-m7-sgc-enabled/libstdc++compat.a
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
