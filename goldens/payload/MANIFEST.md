# goldens/payload -- the payload gate's frozen program objects and goldens (MANIFEST)

Copyright (c) 2026 Lamella LLC and the Lamella project authors.
SPDX-License-Identifier: MIT OR Apache-2.0

The payload gate links these FROZEN COMMITTED OBJECTS -- two programs, a
boot smoke test and a collector test, compiled ahead of time by the .NET
NativeAOT compiler against the minimal runtime under src/payload/minirt,
each program carrying that runtime as its own system module -- under every
board's payload map, boots them on the emulator's Cortex-M7 row, and
compares each program's payload stream against these goldens (the artifact
unit is the bytes from one recorded build; the gate checks every file
against this manifest, and the objects and the goldens against their
in-code pins, before the first case, and REFUSES a mismatch: a frozen
artifact that moved is WRONG, never "rebuilt"). The consumer chain is
Rust + QEMU only; nothing here is compiled at gate time.

Derivation is the leg `rhu-gate payload rederive` (the verifying document's
re-derivation section names the prerequisites): each program is published
from src/payload through the pinned .NET SDK, in a Linux environment where
that SDK is installed, with its project as committed -- the debug type
none, so the object carries no debug information and no path and is the
same bytes from any directory and whatever the sources' comment lines; the
leg refuses an object carrying debug information, compares each object
whole against the file below, diagnoses a difference by the loadable
identity (the debug sections and the comment removed), links and boots the
rebuilt objects by the gate's own method against the goldens, and runs four
controls through the real tool (a source with one code literal changed, a
source with one comment line added, which must rebuild the same bytes, a
build with the debug type overridden, a project with its system-module
property renamed). A compiler that refuses the programs'
system module at the pin is the leg's loud FAIL; a passing build whose
bytes differ reads MOVED until the tier is re-frozen under the leg's boot
read -- the objects replaced by the leg's own rebuild, this manifest's rows
and the gate's in-code pins re-stamped, the goldens unchanged, one reviewed
commit. The goldens are captured on the wire, never edited, and never move
at a re-pin.

Compiler of record: the .NET SDK 11.0.100-rc.1.26425.128 (the pin in
src/payload/global.json) with its ahead-of-time compiler
Microsoft.DotNet.ILCompiler 11.0.0-rc.1.26425.128, the SDK's implicit
reference; each object's producer string names that compiler build. Each
program is published for linux-arm with PublishAot, NoStdLib,
IlcSystemModule naming the program itself (the minimal runtime compiles
INTO each program), OptimizationPreference Size, IlcDehydrate false,
DebugType none and the SDK's default deterministic option; the compiler's
response files are not part of the tier (they name the publish
environment's package cache and toolchain directories). The payload gate links each object by
the runner's linker of record (rust-lld from the pinned toolchain's
sysroot, `-flavor gnu`) under the board's payload map, and reads the
collector test's final line for the collection count and the free byte
count its in-code pins state, re-derived on every image the object links
into.

The objects are compiled forms of the two programs and of the minimal
runtime, whose parts dictated by the .NET NativeAOT compiler come from the
dotnet/runtime repository (Copyright (c) .NET Foundation and Contributors,
MIT license); the NOTICE file's entry for src/payload/minirt/minirt.cs
covers them.

## Files

| file | sha256 | bytes | class |
|---|---|---|---|
| `rtsmoke.o` | `e72d824a26cc516190aee0db37b39e7520aa0c4c353fd8ee3837e788b0370a81` | 35,360 | frozen ILC object, RHU-built from `src/payload/rtsmoke` at the pin |
| `gctest.o` | `c0a91cd86216595f1a52a0d6088bd26ea1bac656ae33cd639660cdcb3fcb05e4` | 33,781 | frozen ILC object, RHU-built from `src/payload/gctest` at the pin |
| `rtsmoke-mps2.golden.txt` | `230e32ad303a37564fdc2093fde03730592113aa3718f533e70adfe547c90eb5` | 306 | golden, DETERMINISTIC (byte class): payload stream byte-identical after LF normalization both sides |
| `gctest-mps2.golden.txt` | `0eb3d1ba8a9f0fc4be43ee4fe5d626626147d98a9a5605da6d1c8ae2887d8b2c` | 621 | golden, STATISTICAL (pins class): pin the five `PASS <name>` markers (`list-survives`, `boxes-survive`, `explicit-collect`, `post-churn-alloc`, `did-collect`) + `ALL GOOD` + final `collections=N free=M` with **M exact = 89016** and **N >= 10**; mid-run `free=` riders are conservative-retention layout noise, REPORT-ONLY |

Both `.golden.txt` files are AUTHENTIC WIRE CAPTURES with CRLF line endings.
They are stored `-text` (.gitattributes) so git never normalizes them; a
golden that churns under checkout is not a golden.
