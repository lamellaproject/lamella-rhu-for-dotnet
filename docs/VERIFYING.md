# Verifying Lamella RHU

This document says what Lamella RHU's quality gate chain needs, how to run
it, which gates a checkout runs, how the frozen objects the gates pin are
re-derived from source, and where the hosted .NET runtime comes from.

## Prerequisites

To run the gate chain you need three things:

- Rust, installed through rustup. The exact toolchain, its cross-compilation
  targets and its `llvm-tools` component are pinned by `rust-toolchain.toml`
  and installed by rustup on the first build.
- QEMU, for the gates that run the kernel: the `qemu-system-arm` executable
  of the release the gates are pinned to (the QEMU 11.0 series). The
  chain finds it through the `QEMU` environment variable (the path of the
  executable), or else `qemu-system-arm` on your PATH, or else the first line
  of the file `tools/toolchains/qemu.local`; if none resolves, it refuses.
- clang 22.1.8, the LLVM release from llvm.org: the api gate's first case
  compiles the hosting header `src/kernel/include/rhu.h` with it, and the
  chain fails without it; a clang reporting another version is refused.

Re-deriving the frozen objects from source (optional; see below) needs, in
addition:

- The Arm GNU Toolchain 12.2.MPACBTI-Rel1 (arm-none-eabi), used only as the
  sysroot: the C library and headers the garbage-collector and shim legs
  compile against. The shim leg refuses another release of it.
- For the two hosted-runtime legs, the .NET 11 SDK at the pin carried by the
  projects under `src/rt/gate`, and the runtime archives built for each
  processor class as described under "Where the hosted runtime comes from".
- For the payload leg, the same .NET 11 SDK at the pin carried by
  `src/payload/global.json`, run in a Linux environment where that SDK is
  installed (see "The payload leg" below).

Each of these is named through an environment variable or an untracked
pointer file under `tools/toolchains/`, listed under "Re-deriving the frozen
objects". On Windows, the Arm GNU Toolchain unpacks to paths longer than the
default 260-character limit: enable long paths, or unpack it under WSL.

## Running the gate chain

The unikernel's quality gate chain builds each kernel configuration, runs it
on QEMU, and checks its behavior. To run it via Rust's cargo:

    cd src/harness
    cargo run --release -- chain

Please note that gate results are pinned against a specific QEMU release
(the QEMU 11.0 series; each gate's result line reports the exact
QEMU and Rust versions it ran under). If your QEMU version is different, the
chain may report correspondingly-different results.

The few gate runs that count the guest's instructions are each a fixed
amount of emulator work, so the harness judges them by the processor time
your machine gave the emulator -- read from the operating system through
`GetProcessTimes` on Windows and through the process's `stat` file under
`/proc` with `sysconf` on Linux, while on any other host the wall clock
judges them -- and a busy machine slows the chain down without failing it,
while a run that works past its budget, or does no work for a whole
deadline, is stopped and fails its gate.

The kernel each gate image links is built with its compilation directory
remapped to a neutral prefix, so a gate-built image's identity does not
depend on where the checkout sits. The identity of record is still per
host, and not by that prefix's path separators: cargo hashes the host's
compiler identity into the crate's metadata, so another host's build of
the same tree carries the same functions at the same sizes in another
order, with other symbol hashes (the remapped prefix and the host's
separators sit in the debug strings alone, outside the loadable sections).

## Which gates a checkout runs

The chain has nine gates, published in this repository and driven through
Rust (cargo). Three of them -- the base, scheduler and hosting-api gates --
run the kernel under QEMU. The host gate runs the kernel's arithmetic
modules (the heap's sizing, the wake deadlines, the arena's bounds) through
Rust's own test runner on your machine; they are hardware-independent, so
it needs no emulator. The remaining five -- the payload, garbage-collector
and shim gates, and the two hosted-runtime gates (distinct for each
processor class) -- run under QEMU and also need the directory of frozen
inputs published under `goldens/` (compiled objects and golden transcripts
whose bytes the gate pins). The chain runs every gate whose
inputs are present and names each tier it could not run on its verdict line:
a passing chain never overstates what it checked.

This repository publishes the payload, garbage-collector and shim tiers'
inputs (`goldens/payload`; `goldens/gc` and `goldens/shim`, which carry
objects for every processor class); their re-derivation legs run with the
prerequisites above. The garbage-collector gate's cases include the
collector's two never-abort behaviours: a mark stack that fills defers the
push and rescans the marked objects inside the window the deferred objects
span, and a handle pool that spends the row's slots grows by segments from
the shim's mmap and answers no handle only when the mmap refuses. A program
may hold as many finalizable objects, live or awaiting finalization, as the
board's finalization table has entries; the next request for one is
answered out of memory after one collection, and none is ever handed out
unregistered (the gate's `gc.finalize-full` case). The
payload gate's two programs, a boot smoke test and
a collector test compiled against a minimal runtime, are the payload tier's
frozen objects, built from the sources under `src/payload/`; the payload leg
rebuilds them with the pinned .NET SDK and ties them to the tier byte for
byte. The payload gate's rt-link cases link the hosted runtime's frozen
merged object under every board's payload map and need the two
hosted-runtime tiers; without them the gate runs its other cases and names
those cases NOT RUN, with the absent tier, on its verdict line.

The two hosted-runtime tiers and the two runtime archive sets are too large
for the source archive: they are published as two archives of the release
that carries this copy, named with the project and the version --
`lamella-rhu-<version>-goldens.tar` (the tiers, a directory per tier
inside, `goldens/rt-m7` and `goldens/rt-m33`) and
`lamella-rhu-<version>-runtime-archives.tar` (the runtime archive sets, a
directory per set inside) -- with `SHA256SUMS` beside them. Each archive
carries `MANIFEST.txt` at its root: what it holds, every member with its
size and sha256, and the one command that checks the unpacked files. The
record `goldens/MANIFEST.md` in this tree names every tier and both
archives with their sha256s and byte counts and repeats each archive's
list, so the bytes you fetch are checked against the revision you hold
before any gate reads them. Fetch the three files from the release with
the GitHub command-line tool (or each one by name with `curl -L` from the
release's page), check them, unpack the goldens archive at the repository
root and the runtime-archives archive into a directory of your own, check
the unpacked files, and name each set's own directory by `RHU_RT_ARCHIVES_M7`
and `RHU_RT_ARCHIVES_M33` or by the pointer files `rtarchives-m7.local` and
`rtarchives-m33.local` described below:

    gh release download <tag> --repo lamellaproject/lamella-rhu-for-dotnet \
        --pattern 'lamella-rhu-*.tar' --pattern SHA256SUMS
    sha256sum -c SHA256SUMS
    tar -xf lamella-rhu-<version>-goldens.tar
    sha256sum -c MANIFEST.txt
    mkdir -p /path/to/archives
    tar -xf lamella-rhu-<version>-runtime-archives.tar -C /path/to/archives
    (cd /path/to/archives && sha256sum -c MANIFEST.txt)
    export RHU_RT_ARCHIVES_M7=/path/to/archives/net11-rc2-rhu-cortex-m7-sgc-enabled
    export RHU_RT_ARCHIVES_M33=/path/to/archives/net11-rc2-rhu-cortex-m33-sgc-enabled

The goldens archive leaves its `MANIFEST.txt` at the repository root, where
git ignores it. With the tiers in place the chain runs every gate, and the
harness's selfcheck ties `goldens/MANIFEST.md` to the tree it runs in,
refusing a record that does not match the files it holds.

## Re-deriving the frozen objects

Each frozen tier has a re-derivation leg that rebuilds its objects from
source and checks them byte for byte against the tier.

From `src/harness`:

    cargo run --release -- gc rederive
    cargo run --release -- shim rederive
    cargo run --release -- rt-m7 rederive
    cargo run --release -- rt-m33 rederive
    cargo run --release -- payload rederive

The garbage-collector and shim legs compile with clang 22.1.8; the C library
and headers they compile against are the Arm GNU Toolchain 12.2.MPACBTI-Rel1's
(arm-none-eabi). That kit is used only as the sysroot, never as a compiler,
and the shim leg refuses another release of it. A clang reporting another
version is refused.

Choose the compiler and the sysroot through environment variables:

    RHU_CLANG=<the clang 22.1.8 command>
    RHU_CLANG_SYSROOT=<the toolchain's arm-none-eabi directory>
    RHU_NEWLIB_M7=<the kit's directory holding libc_nano.a for v7e-m+dp/hard>
    RHU_RT_ARCHIVES_M7=<the directory of the Cortex-M7 runtime archives described below>
    RHU_NEWLIB_M33=<the kit's directory holding libc_nano.a for v8-m.main+fp/hard>
    RHU_RT_ARCHIVES_M33=<the directory of the Cortex-M33 runtime archives>
    RHU_NET_PACK=<the pinned .NET runtime pack's native directory, runtimes/linux-arm/native>

or through untracked pointer files under `tools/toolchains/`, one value per
line: `clang.local` (two lines: the command, then the sysroot),
`newlib-m7.local`, `rtarchives-m7.local`, `newlib-m33.local`,
`rtarchives-m33.local` and `netpack.local`. The shim leg's C-library case
needs `RHU_NEWLIB_M7`; its survey control, which re-reads the pinned runtime
pack's five class-library archives against the frozen surface expectation
(`goldens/shim/pal-surface.txt`), needs `RHU_NET_PACK` and is declared, not
run, without it; each hosted-runtime leg needs its class's archive set and
C-library directory. The kit's directory layout differs by host: the Linux kit keeps
the Cortex-M7 library under `arm-none-eabi/lib/thumb/v7e-m+dp/hard` and
the Cortex-M33 one under `arm-none-eabi/lib/thumb/v8-m.main+fp/hard`,
whereas the Windows kit keeps them under
`lib/gcc/arm-none-eabi/12.2.1/thumb/v7e-m+dp/hard` and
`lib/gcc/arm-none-eabi/12.2.1/thumb/v8-m.main+fp/hard`.

### The payload leg

The payload leg rebuilds the payload tier's two programs from `src/payload`
through the pinned .NET SDK. The two projects publish without debug
information, so each object depends on the code alone: the same bytes from
any directory and whatever the sources' comment lines. Each rebuilt object
is compared whole against the frozen one, and the rebuilt objects are linked
and booted by the payload gate's own method against the goldens; an object
published with another debug type is refused, never compared. The leg reads
each rebuilt object where the publish wrote it, or from a directory named by
`RHU_PAYLOAD_OBJECTS`; without either, and without the publish environment of
the second form below, the leg is declared not run.

Publish the two objects from this checkout's `src/payload` in a Linux
environment where the SDK named by `src/payload/global.json` is installed
(on a Windows host, a WSL distribution: from a Windows host the SDK refuses
a cross-OS native publish). Each command runs in its program's directory,
where `dotnet` reads that pin:

    (cd src/payload/rtsmoke && dotnet publish rtsmoke.csproj -c Release -r linux-arm)
    (cd src/payload/gctest && dotnet publish gctest.csproj -c Release -r linux-arm)

Each publish writes its object at
`src/payload/<program>/obj/Release/net11.0/linux-arm/native/<program>.o` and
then fails at the host's native link step, by design (the host cannot link
an ARM image); then run the leg, which reads each object where the publish
wrote it (to read them from another directory, name it by
`RHU_PAYLOAD_OBJECTS`). A compiler that refuses the programs' system module at the pin
fails the publish with the compiler's lines; a build whose bytes differ
from the tier's reads MOVED; the leg also boots the rebuilt objects
against the goldens and compares their output.

The leg's second form drives the publish itself, through a script the
harness runs in that Linux environment, when `RHU_PUBLISH` (the command
prefix that runs a script there) and `RHU_PUBLISH_ROOT` (this checkout's
root as that environment sees it) are set, or the two lines of the
untracked pointer file `tools/toolchains/publish.local` are present; on a
Windows host use the pointer file, since a POSIX-emulating shell converts a
path-shaped value typed into a variable before the harness reads it.
That form adds four controls through the real tool: a source with one code
literal changed, a source with one comment line added (its object must be
the same bytes), a build with the debug type overridden (its object is
refused), and a project with its system-module property renamed; from an
objects directory the four are named NOT RUN on the verdict line.

The frozen objects were produced by the LLVM 22.1.8 release from llvm.org,
run on Linux. Another build of clang 22.1.8 carries another producer stamp,
which the object comparisons set aside: a difference in that section alone
is not a difference in the code, and every comparison of the shim and
garbage-collector legs, their internal controls included, is made that
way.

Where the toolchain's object copier can remove an object's producer stamp (the
section naming the compiler build that produced it), the comparison is made
without that section and both stamps are printed; where it
cannot (the merged runtime objects), the object is compared whole -- each leg
says which.

## Where the hosted runtime comes from

Each hosted-runtime gate links a .NET 11 program, compiled ahead of time by
the .NET SDK, against the .NET runtime's native archives built for its
processor class. The two programs under `src/rt/gate` carry the project
properties, the SDK pin and the package sources; each is published in its
own directory with the board named, since the SDK refuses a build that
names no board:

    cd src/rt/gate/rthello
    dotnet publish -c Release -r linux-arm -p:LamellaTargetBoard=arm-mps2-an500

(the Cortex-M33 gate publishes the same programs for `arm-mps2-an505`),
and the gate links the object the SDK writes under
`obj/Release/net11.0/linux-arm/native/`. The projects name the Lamella SDK
(`Sdk="Lamella.Sdk"`, whose shell restores RHU's build, board, runtime and
tools packages): the gate
consumes the tier's frozen objects, and the SDK parity leg declares itself
not run without the Linux environment for the publish. With it, the leg ties
the object, the surgery object, the merged object and the image byte for
byte, ties the board table's stack facts to the tree, boots the image a
program's stack knob produces, and exercises the SDK's refusals through
the real tool, among them a publish from a git checkout at another path,
which must tie the frozen object.
The SDK's link runs the tools package for the host the .NET SDK reports;
the packages are packed for `win-x64`, `win-arm64`, `osx-arm64`, `linux-x64`
and `linux-arm64` from the LLVM release's own binaries and the Lamella
project's `lamella` tool built for each host, and a macOS host publishes
natively. The compile reads its reference assemblies from the
package `Microsoft.NETCore.App.Ref`, never from the copy a .NET SDK
bundles, and the SDK names that package's version itself, whatever .NET
SDK is installed; so the object of record is one object on every host of
the pinned .NET SDK.
Another .NET SDK version compiles the same code under another
identity: its object differs in the sixteen bytes of the assembly's module
version id alone.

Lamella RHU builds the runtime archives itself from the dotnet/runtime
source at its pinned tag (.NET 11 RC2), with clang 22.1.8,
through the runtime repository's own build entry in a cross environment
whose target sysroot is Ubuntu 22.04's armhf packages:

    ./build.sh clr.nativeaotruntime -c Release -arch arm -cross

with two build flags that differ from the stock build:

    CLR_ARM_FPU_TYPE=fpv5-d16
    -mcpu=cortex-m7        (the C and C++ compiles)

The stock pack is built for application-profile cores, whose FPU has
thirty-two double-precision registers and whose instruction set has
64-bit exclusive-access instructions; the Cortex-M7's FPU has sixteen,
and no Cortex-M has those instructions, so a stock archive contains
instructions the Cortex-M7 cannot execute. The first flag keeps every
compiled function inside the sixteen. The second names the core, so the
compiler emits no 64-bit exclusive-access instruction and calls a library
function for each 64-bit atomic operation instead, which the hosting shim
answers under one lock. The Cortex-M33 class's archives are built by the
same recipe with `fpv5-sp-d16` and `-mcpu=cortex-m33`: that core's FPU is
single-precision, so the compiler emits software double-precision
arithmetic, and the hosted-runtime gate for that class consumes those
archives. The stock runtime pack for the same version,
`Microsoft.NETCore.App.Runtime.NativeAOT.linux-arm`, is the parity control:
it and the RHU build have the same archive members with the same defined
names; the RHU build's undefined names add the 64-bit atomic library
calls and omit the integer division helpers the core has in hardware, and
the two differ otherwise only in the flags' effect. Because the emulator
executes both kinds of instruction, each hosted-runtime gate censuses its
frozen objects and its linked images for them at instruction boundaries,
with a frozen control object of each kind that the census must refuse.

