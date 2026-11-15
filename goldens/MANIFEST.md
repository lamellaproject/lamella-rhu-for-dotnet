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

- `lamella-rhu-1.0.0-alpha.3-goldens.tar` holds the tiers `goldens/rt-m7`
  and `goldens/rt-m33`, a directory per tier. It is unpacked at the
  repository root.
- `lamella-rhu-1.0.0-alpha.3-runtime-archives.tar` holds the runtime library
  sets `net11.0.0-rhu-cortex-m7-sgc-enabled` (Cortex-M7) and
  `net11.0.0-rhu-cortex-m33-sgc-enabled` (Cortex-M33), a directory per set.
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
| the garbage-collector tier | `goldens/gc/MANIFEST.md` | 63 | 2654414 | in this directory |
| the shim tier | `goldens/shim/MANIFEST.md` | 183 | 7722904 | in this directory |
| the payload tier | `goldens/payload/MANIFEST.md` | 5 | 74686 | in this directory |
| the Cortex-M7 hosted-runtime tier | `goldens/rt-m7/MANIFEST.md` | 17 | 36049447 | in the release archive `lamella-rhu-1.0.0-alpha.3-goldens.tar` |
| the Cortex-M33 hosted-runtime tier | `goldens/rt-m33/MANIFEST.md` | 15 | 23256672 | in the release archive `lamella-rhu-1.0.0-alpha.3-goldens.tar` |

| Manifest | SHA256 |
|---|---|
| `goldens/gc/MANIFEST.md` | `bfcf10f6710308d456bd9e1c93c9c78122222c789fabc412bc65350db8ab5911` |
| `goldens/shim/MANIFEST.md` | `0f8850d9b1473c1abf7d42ec38ed5ac8567e5d9971963fbf1b1b8fb92228549f` |
| `goldens/payload/MANIFEST.md` | `ed83f77f8cb06ec56f84634889b86940b1a0b705c107f47dd2e36bd9b473979b` |
| `goldens/rt-m7/MANIFEST.md` | `bc6b7469f40310affb5fe9874df34daaebfc4f2ab07c9030f73e210e4c51aebc` |
| `goldens/rt-m33/MANIFEST.md` | `a53a5905404ec007e9ec5ae66f91ceaa2891c96fceba0ceabd6cdd4f240e0127` |

## The release archives

| Archive | Members | Bytes | SHA256 |
|---|---|---|---|
| `lamella-rhu-1.0.0-alpha.3-goldens.tar` | 33 | 59339264 | `5f08e43f8369f6c1243c50097bb3722599b48fd39b67255fb0f885ac50c233fe` |
| `lamella-rhu-1.0.0-alpha.3-runtime-archives.tar` | 11 | 11064320 | `ccb0a31d3b80c13c52dbc599fb0204b7a472a359216c9176b57ed99c65b5380f` |

### lamella-rhu-1.0.0-alpha.3-goldens.tar

33 members, 59339264 bytes. The block lists every member in the archive's
order with its SHA256 digest. Its first line is the archive's own
MANIFEST.txt; the rest is that file's text, as the archive carries it, so a
copy of the block checks the unpacked files the same way.

```
2f44d98fc3fe80f349695ddc8cfe4fafdc4b58c47eae1c96351664d19ef1619f  MANIFEST.txt
# lamella-rhu-1.0.0-alpha.3-goldens.tar
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
#      15299  goldens/rt-m33/MANIFEST.md
#       4960  goldens/rt-m33/archives-net11.0.0-rhu-cortex-m33-sgc-enabled.txt
#         76  goldens/rt-m33/attr-donor-v8m.bin
#         60  goldens/rt-m33/control-attr-a-profile.bin
#        928  goldens/rt-m33/control-d32-site.o
#        924  goldens/rt-m33/control-ldrexd-site.o
#       5789  goldens/rt-m33/rt-libc.members.txt
#     141376  goldens/rt-m33/rt-libc.o
#    7532184  goldens/rt-m33/rthello-merged.o
#       3421  goldens/rt-m33/rthello-merged.undef.txt
#        167  goldens/rt-m33/rthello.golden.txt
#   15507232  goldens/rt-m33/rtsuite-merged.o
#       6156  goldens/rt-m33/rtsuite-merged.undef.txt
#       3264  goldens/rt-m33/rtsuite.golden.txt
#      34836  goldens/rt-m33/startup-cookie-copied.o
#      20383  goldens/rt-m7/MANIFEST.md
#       4949  goldens/rt-m7/archives-net11.0.0-rhu-cortex-m7-sgc-enabled.txt
#         50  goldens/rt-m7/attr-donor-v7em.bin
#         60  goldens/rt-m7/control-attr-a-profile.bin
#        928  goldens/rt-m7/control-d32-site.o
#        924  goldens/rt-m7/control-ldrexd-site.o
#       5822  goldens/rt-m7/rt-libc.members.txt
#     134562  goldens/rt-m7/rt-libc.o
#    7544906  goldens/rt-m7/rthello-merged.o
#       3203  goldens/rt-m7/rthello-merged.undef.txt
#        167  goldens/rt-m7/rthello.golden.txt
#    2610763  goldens/rt-m7/rthello.o
#   15519954  goldens/rt-m7/rtsuite-merged.o
#       5938  goldens/rt-m7/rtsuite-merged.undef.txt
#       3264  goldens/rt-m7/rtsuite.golden.txt
#   10158730  goldens/rt-m7/rtsuite.o
#      34844  goldens/rt-m7/startup-cookie-copied.o
#
# Each line below is a member's SHA256 digest, two spaces and its path.
# The lines above, which start with a number sign, are comments that
# sha256sum skips.
#
a53a5905404ec007e9ec5ae66f91ceaa2891c96fceba0ceabd6cdd4f240e0127  goldens/rt-m33/MANIFEST.md
cebafadd28c638de7dda4b543adef14e24ab4539e115c9ed1fba35fd57f1aa1a  goldens/rt-m33/archives-net11.0.0-rhu-cortex-m33-sgc-enabled.txt
e8ebecc9e0784f841ca9f09a6bfa415ad9d04431976db2d0fad100a74e81c3b1  goldens/rt-m33/attr-donor-v8m.bin
5c1121c6df6659326c8e22a43d54080ea3dc2f495078f57e375625d93349e6ca  goldens/rt-m33/control-attr-a-profile.bin
762ff2df1f0504c23347bdb47435b6ed48c3932c678a455aada60c994a441f14  goldens/rt-m33/control-d32-site.o
e29d4d1c92e619fb9e2b53e50a52ec1b643c5902a353b5b9108beaf54b0cf533  goldens/rt-m33/control-ldrexd-site.o
d07a3ab13c7de6e761a8a880b4c606e1b35f493ffc19ca51c238607422ed6347  goldens/rt-m33/rt-libc.members.txt
787fa5a3f597591438811736480ac183d88305654a26fd5648ebf160224795fe  goldens/rt-m33/rt-libc.o
d94c8baead88cfe2237455a67f81b55f25086f14487a2731c5f44fd98effff4f  goldens/rt-m33/rthello-merged.o
ee10d3c0a328caba9f19280f5cb235912e6336d3a9314fb2500b76da064740cc  goldens/rt-m33/rthello-merged.undef.txt
66c7f87ca29b27cf4b8ed05c8fb2d0eb30d16b48375325ec65c726e835a5fa7b  goldens/rt-m33/rthello.golden.txt
28a08fd321337a92521d429fe9ced7300fb60579c99cb653c7a684ea258b7b3b  goldens/rt-m33/rtsuite-merged.o
67b6e30c12ba40a98d3346200dc9bcf696e574dcc96a2eb0deade1ecd18b5d9b  goldens/rt-m33/rtsuite-merged.undef.txt
7aef39fac68b29684be5aa3f4d167e78f95a2a28208ea3629227b548034db9c6  goldens/rt-m33/rtsuite.golden.txt
cd05adba49165b84a9af8471d96ef52cae60d253b7d8f3bd46b23acdb9d97bc6  goldens/rt-m33/startup-cookie-copied.o
bc6b7469f40310affb5fe9874df34daaebfc4f2ab07c9030f73e210e4c51aebc  goldens/rt-m7/MANIFEST.md
bfbd3c3f327d9285d7c77de31ce761ac124cc7642ac5c426286a4c48048bbecd  goldens/rt-m7/archives-net11.0.0-rhu-cortex-m7-sgc-enabled.txt
f0806a5d3672b24489ff1c896f7e3b74e3c3d1fe030bcacd9f010ee3cdf5216d  goldens/rt-m7/attr-donor-v7em.bin
5c1121c6df6659326c8e22a43d54080ea3dc2f495078f57e375625d93349e6ca  goldens/rt-m7/control-attr-a-profile.bin
762ff2df1f0504c23347bdb47435b6ed48c3932c678a455aada60c994a441f14  goldens/rt-m7/control-d32-site.o
e29d4d1c92e619fb9e2b53e50a52ec1b643c5902a353b5b9108beaf54b0cf533  goldens/rt-m7/control-ldrexd-site.o
12f814c7d5d6d310ba9643d1f2fbdbf82dac2a9491ff4cd06e89f37f1eeacb5f  goldens/rt-m7/rt-libc.members.txt
536d99f34384bde7003e0b9c3dd52df97739977254e1eae4c1d12f5e623fcacf  goldens/rt-m7/rt-libc.o
979672c9aef8e905bb241e9399b2dce9d2922f5e2b2495ca90106b90ac7dc70d  goldens/rt-m7/rthello-merged.o
d00c7cfd635402220b0ca6b4820aa38ca9f41c89a002470d31bbda7920abca4e  goldens/rt-m7/rthello-merged.undef.txt
66c7f87ca29b27cf4b8ed05c8fb2d0eb30d16b48375325ec65c726e835a5fa7b  goldens/rt-m7/rthello.golden.txt
8f58f41d21e1170c70858bdf203adb5f0f8b939d6068be97df8600a5a0a29d50  goldens/rt-m7/rthello.o
faf7e094a3c6d64f9e8340c82503c0482de5bc2c46feb971210ae807501b4180  goldens/rt-m7/rtsuite-merged.o
0878f319e9f25bf2bb832f10d1ba820b5e81f5419c4283a787f8cf9d17e2c3d1  goldens/rt-m7/rtsuite-merged.undef.txt
7aef39fac68b29684be5aa3f4d167e78f95a2a28208ea3629227b548034db9c6  goldens/rt-m7/rtsuite.golden.txt
2e41e9f23e774e1b2532c87539a597079e6b4f103f04f3ea200d8b0b6af4f400  goldens/rt-m7/rtsuite.o
2616895fdb63257589aa05d6bb0d5ccaf27f50e922b2d1db753a562c30aa9c86  goldens/rt-m7/startup-cookie-copied.o
```

### lamella-rhu-1.0.0-alpha.3-runtime-archives.tar

11 members, 11064320 bytes. The block lists every member in the archive's
order with its SHA256 digest. Its first line is the archive's own
MANIFEST.txt; the rest is that file's text, as the archive carries it, so a
copy of the block checks the unpacked files the same way.

```
9db69916714e3b0a000e3d853c1aa8e89aa74eaaf33dd1b80138a594e78b3163  MANIFEST.txt
# lamella-rhu-1.0.0-alpha.3-runtime-archives.tar
#
# This archive holds two sets of .NET runtime libraries that Lamella RHU
# built for its processor classes, each in its own directory:
# net11.0.0-rhu-cortex-m7-sgc-enabled (Cortex-M7) and
# net11.0.0-rhu-cortex-m33-sgc-enabled (Cortex-M33). The hosted-runtime
# tiers are rebuilt against these libraries. How each set was built is
# recorded beside the tier that uses it, in
# goldens/rt-m7/archives-net11.0.0-rhu-cortex-m7-sgc-enabled.txt and
# goldens/rt-m33/archives-net11.0.0-rhu-cortex-m33-sgc-enabled.txt.
#
# Unpack this archive into a directory of your own. Then check every
# unpacked file against this list, from that directory:
#
#     sha256sum -c MANIFEST.txt
#
# Name the Cortex-M7 set's directory for the verification chain: put its
# path (the directory you unpacked into, then
# net11.0.0-rhu-cortex-m7-sgc-enabled) into
# tools/toolchains/rtarchives-m7.local, or set RHU_RT_ARCHIVES_M7 to it.
#
# Name the Cortex-M33 set's directory for the verification chain: put
# its path (the directory you unpacked into, then
# net11.0.0-rhu-cortex-m33-sgc-enabled) into
# tools/toolchains/rtarchives-m33.local, or set RHU_RT_ARCHIVES_M33 to
# it.
#
# The members and their sizes in bytes:
#
#    5020186  net11.0.0-rhu-cortex-m33-sgc-enabled/libRuntime.WorkstationGC.a
#     169032  net11.0.0-rhu-cortex-m33-sgc-enabled/libaotminipal.a
#     220014  net11.0.0-rhu-cortex-m33-sgc-enabled/libeventpipe-disabled.a
#     100646  net11.0.0-rhu-cortex-m33-sgc-enabled/libstandalonegc-enabled.a
#       9498  net11.0.0-rhu-cortex-m33-sgc-enabled/libstdc++compat.a
#    5032004  net11.0.0-rhu-cortex-m7-sgc-enabled/libRuntime.WorkstationGC.a
#     169896  net11.0.0-rhu-cortex-m7-sgc-enabled/libaotminipal.a
#     220018  net11.0.0-rhu-cortex-m7-sgc-enabled/libeventpipe-disabled.a
#     100850  net11.0.0-rhu-cortex-m7-sgc-enabled/libstandalonegc-enabled.a
#       9514  net11.0.0-rhu-cortex-m7-sgc-enabled/libstdc++compat.a
#
# Each line below is a member's SHA256 digest, two spaces and its path.
# The lines above, which start with a number sign, are comments that
# sha256sum skips.
#
2146930909eaed6a8cb77aed9f7222d9af6ecfc7678d256ea4584aacf937498e  net11.0.0-rhu-cortex-m33-sgc-enabled/libRuntime.WorkstationGC.a
31fdbffe9859037b65ac5bdd2fe2c19c48196858c95781a3dcf52007cd7bdc5a  net11.0.0-rhu-cortex-m33-sgc-enabled/libaotminipal.a
c515fce9a36566c6a22a7e624ad963a09deebf980c72e50944afb44cbef65179  net11.0.0-rhu-cortex-m33-sgc-enabled/libeventpipe-disabled.a
e12f0703e5da6f35c02489248ec77ef569c2552b99363ae5cda5658a146945d5  net11.0.0-rhu-cortex-m33-sgc-enabled/libstandalonegc-enabled.a
4781147bf4211037589682398b0b03a9e3336ff53498939501a03cdbebbe9f3e  net11.0.0-rhu-cortex-m33-sgc-enabled/libstdc++compat.a
c22c3ae3321eccf4b3cf6dc65c31d3764ec63d32d89bf6e283029cb55e7f3963  net11.0.0-rhu-cortex-m7-sgc-enabled/libRuntime.WorkstationGC.a
c02aea4393f45ead55cbd51bd061a5c46cad3e1b42d95cc68fb230523644f353  net11.0.0-rhu-cortex-m7-sgc-enabled/libaotminipal.a
4b07e0a383bfa95e7a4171fe718205d7dc76b4e0c928a77a8fe2d18551cd9f94  net11.0.0-rhu-cortex-m7-sgc-enabled/libeventpipe-disabled.a
b07ba64fdd7665d4505494ef1c67d6bf7f058664b0ddc0bed7ba4a2cfc5c02f0  net11.0.0-rhu-cortex-m7-sgc-enabled/libstandalonegc-enabled.a
6a178e9fe946e8408a661aecf3e08f669c8ab9b326914e57a83dcc564e2762c0  net11.0.0-rhu-cortex-m7-sgc-enabled/libstdc++compat.a
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
