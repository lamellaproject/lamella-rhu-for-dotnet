# The FAT test images

Copyright (c) 2026 Lamella LLC.
SPDX-License-Identifier: MIT OR Apache-2.0

This folder holds three small FAT disk images used by the file-system tests in
src/kernel-host (the tests named `fat_*` in src/kernel-host/src/tests.rs). The
images are stored in a compact form, the `.rsi` files, which keeps only the
sectors that contain data; src/kernel-host/src/sparse.rs expands them back to
full images in memory. Before the first test runs, it computes the sha256 of
each compact file and of the expanded image and compares both with the table
below. If either differs, the tests stop: these images are fixed test data, and
a changed image is a bug to find, not something to regenerate.

The images were formatted with dosfstools' mkfs.fat and filled with GNU mtools.
The procedure is deterministic: mkfs.fat ran with `--invariant`, every file was
stamped 1 January 2026 00:00:00 UTC, and every file's content follows the rule
given in the last section, so running the same procedure under the same tool
versions produces byte-identical images with the digests below. The full-size
images are not kept in the repository. When a test needs a corrupted image, it
patches its own in-memory copy; there are no separate corrupted images.

## The images

| golden | sectors | clusters | cluster bytes | free clusters | raw image sha256 | sparse file sha256 | sparse bytes |
|---|---|---|---|---|---|---|---|
| fat12-1m | 2048 | 2003 | 512 | 421 | ae243530af2d62417ecb6f1d5f4cbc193198ecb20be5d9f7d47329e8808c08fc | a46e21f2f16f73d30efb25eec4e631e55a461c04849c840234243a72d8533253 | 823560 |
| fat16-32m | 65536 | 32623 | 1024 | 30560 | ffe3f9d940097b2ee785edcf002972d77dc784a866e75b5a500f2485cfb1dacc | 1f49987b9dda98d1629ff0bcf71978f7290aca22ecebd7b04248e4bd006e38bc | 1628005 |
| fat32-64m | 131072 | 129022 | 512 | 125902 | 304343297bf12d739bd2dc91637edd84ae7d6646224b04a5dbd4e5522bab90a8 | c9c3f681fc5c5a752f350889624b352bc7b087123c947e84109a5ce960c2aaf9 | 1663610 |

The columns: `golden` is the file name without `.rsi`; `sectors` is the image
size in 512-byte sectors; `clusters` and `cluster bytes` describe the allocation
unit; `free clusters` is the count after the contents were written; the two
digests are what the tests check; `sparse bytes` is the size of the `.rsi` file.

How each image was formatted:

- fat12-1m: `mkfs.fat -F 12 -s 1` on 1024 KiB. One reserved sector, two FATs
  of 6 sectors each, 512 root directory entries. It is FAT12 because it has
  2003 clusters, below the 4085 limit.
- fat16-32m: `mkfs.fat -F 16 -s 2` on 32768 KiB. Two reserved sectors, two
  FATs of 128 sectors each, 512 root directory entries. It is FAT16 because it
  has 32623 clusters.
- fat32-64m: `mkfs.fat -F 32 -s 1` on 65536 KiB. 32 reserved sectors, two FATs
  of 1009 sectors each, no fixed root directory (the root is a cluster chain
  starting at cluster 2), and the FSInfo sector at sector 1. It is FAT32
  because it has 129022 clusters, above the 65525 limit.

The FAT type follows from the cluster count alone. The volume labels are
RHUFAT12, RHUFAT16 and RHUFAT32, and the volume serial number is 1234-ABCD on
each (`--invariant` fixes it).

## The compact form

A `.rsi` file begins with the line `RHU sparse image 1`, then a line with the
sector count in decimal, then one record for every sector that contains a
non-zero byte, in ascending order: the sector number as four little-endian
bytes, followed by the sector's 512 bytes. A reader starts from an all-zero
image of the given sector count and writes each record into place.

## The build environment and tools

| tool | version |
|---|---|
| dosfstools (mkfs.fat) | 4.2-1.1build1 |
| mtools (mcopy, mmd, mdel, mrd, mdir) | 4.0.43-1build1 |
| python3 (the content generator) | 3.12.3 |

The images were built on Ubuntu 24.04 with the tool versions above. Other
versions of these tools may lay a volume out differently, so the digests above
describe these three images, not every image the same procedure could produce.

One quirk worth knowing: mtools 4.0.43 stores a long-name character above
U+FFFF as its low sixteen bits. One file was named with the character U+1D11E
(a musical symbol), and the volume holds U+D11E (a Hangul syllable) in its
place. The name on the volume is therefore four Latin-1 letters and one Hangul
syllable, while the file's content was generated from the name as originally
written, with U+1D11E.

## The file table

| volume path | bytes | sha256 |
|---|---|---|
| hello.txt | 58 | 24c6d98a9d95481073a2ba142a19514ca718bf1de9c6011bf114084556df3051 |
| deep.txt, three directories down (a, then b, then c) | 100 | d8112cdd7815132be89b09db9b42f5568325759e7f544146259d47f5c15c9bb9 |
| the-two-hundred-character-name-the-two-hundred-character-name-the-two-hundred-character-name-the-two-hundred-character-name-the-two-hundred-character-name-the-two-hundred-character-name-the-two-hu.txt | 200 | ad41368b9618a5b1579ec8c0d48adc455f06d5c2c9bd113cb11f9943051056ac |
| U+00E4 U+00F6 U+00FC U+00DF U+D11E .txt | 40 | 6fc98e6301ed868ab44b09fbee88ac935cb9d07e9a42a9d3f36886a3fb9655fd |
| empty.bin | 0 | e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855 |
| big.bin (fat16-32m and fat32-64m) | 1048576 | 5acea60b74e6302b52bdc1c554bb4363684f192b7dd193e700a20b1114aac667 |
| big.bin (fat12-1m) | 262144 | 07c7901ada8d41aef3f99f4cdd109a83063614aebc6cc4c140ee40102b8f6810 |
| many/f0001.txt | 16 | c09d25c9d5b0f6846eb8c90a1a594cf1a36d46e7b99a5f2002496f3776e33248 |
| many/f0500.txt | 16 | ed52e194854a9e757b4663f06b5cd231f3ad3def1746a0eddbcc838bd4f68fb2 |
| many/f1000.txt | 16 | 34b86ff1ddde41a73bea80ab281784bbb654641d13f350533901d790c54ef907 |

The content rule: a file's bytes are little-endian 32-bit integers counting up
from a seed. The seed is the first four bytes, read little-endian, of the sha256
of the file's name in UTF-8. The sequence is cut to the file's length. Every
digest above follows from this rule, and a reader that assembles big.bin's
pieces in the wrong order cannot match it.

What is on each volume:

- At the root: hello.txt (58 bytes), empty.bin (0 bytes), the file with the
  200-character name and the file with the non-ASCII name.
- a/b/c/deep.txt, three directories down.
- big.bin: 1 MiB on fat16-32m and fat32-64m, 256 KiB on fat12-1m, deliberately
  stored in eight separate runs of clusters (512 clusters on fat12-1m, 1024 on
  fat16-32m, 2048 on fat32-64m). This was done by filling a directory named
  frag with alternating zero-filled hole and filler files (32 KiB each on
  fat12-1m, 128 KiB on the others), deleting the holes, copying big.bin into
  the gaps, then deleting the fillers and the directory itself. As on any FAT
  volume, the freed directory clusters still contain the deleted entries.
- many/f0001.txt to many/f1000.txt: 1,000 files of 16 bytes each. Their
  lower-case names are stored as short entries with the NT case bits, which is
  how mtools stores a lower-case name that fits the 8.3 form. The three listed
  in the table stand for the whole set; every one follows the content rule.

There are no hidden entries. The volume label is the first entry in each root
directory.
