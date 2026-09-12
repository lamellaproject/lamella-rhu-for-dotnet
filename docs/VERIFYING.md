# Verifying Lamella RHU

This document says what Lamella RHU's quality gate chain needs, how to run
it, which gates a checkout runs, how the frozen objects the gates pin are
re-derived from source, and where the hosted .NET runtime comes from.

## Prerequisites

To run the gate chain you need two things:

- Rust, installed through rustup. The exact toolchain, its cross-compilation
  targets and its `llvm-tools` component are pinned by `rust-toolchain.toml`
  and installed by rustup on the first build.
- QEMU, for the gates that run the kernel: the `qemu-system-arm` executable
  of the release the gates are pinned to (the QEMU 11.0 series today). The
  chain finds it through the `QEMU` environment variable (the path of the
  executable), or else `qemu-system-arm` on your PATH, or else the first line
  of the file `tools/toolchains/qemu.local`; if none resolves, it refuses.

Re-deriving the frozen objects from source (optional; see below) needs, in
addition:

- clang 22.1.8, the LLVM release from llvm.org; a clang reporting another
  version is refused.
- The Arm GNU Toolchain 12.2.MPACBTI-Rel1 (arm-none-eabi), used only as the
  sysroot: the C library and headers the garbage-collector and shim legs
  compile against. The shim leg refuses another release of it.
- For the two hosted-runtime legs, the .NET 11 SDK at the pin carried by the
  projects under `src/rt/gate`, and the runtime archives built for each
  processor class as described under "Where the hosted runtime comes from".

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
(currently the QEMU 11.0 series; each gate's result line reports the exact
QEMU and Rust versions it ran under). If your QEMU version is different, the
chain may report correspondingly-different results.

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

This repository publishes the garbage-collector and shim tiers' inputs
(`goldens/gc`, `goldens/shim`; both carry objects for every processor class);
their re-derivation legs run with the prerequisites above.

## Re-deriving the frozen objects

Each frozen tier has a re-derivation leg that rebuilds its objects from
source and checks them byte for byte against the tier.

From `src/harness`:

    cargo run --release -- gc rederive
    cargo run --release -- shim rederive
    cargo run --release -- rt-m7 rederive
    cargo run --release -- rt-m33 rederive

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

or through untracked pointer files under `tools/toolchains/`, one value per
line: `clang.local` (two lines: the command, then the sysroot),
`newlib-m7.local`, `rtarchives-m7.local`, `newlib-m33.local` and
`rtarchives-m33.local`. The shim leg's C-library case needs `RHU_NEWLIB_M7`;
each hosted-runtime leg needs its class's archive set and C-library
directory. The kit's directory layout differs by host: the Linux kit keeps
the Cortex-M7 library under `arm-none-eabi/lib/thumb/v7e-m+dp/hard` and
the Cortex-M33 one under `arm-none-eabi/lib/thumb/v8-m.main+fp/hard`,
whereas the Windows kit keeps them under
`lib/gcc/arm-none-eabi/12.2.1/thumb/v7e-m+dp/hard` and
`lib/gcc/arm-none-eabi/12.2.1/thumb/v8-m.main+fp/hard`.

The frozen objects were produced by the LLVM 22.1.8 release from llvm.org,
run on Linux. Another build of clang 22.1.8 carries another producer stamp,
which the object comparisons set aside: a difference in that section alone
is not a difference in the code, and every comparison of the shim leg,
its internal controls included, is made that way.

Where the toolchain's object copier can remove an object's producer stamp (the
section naming the compiler build that produced it), the comparison is made
without that section and the two stamps are printed for the record; where it
cannot (the merged runtime objects), the object is compared whole -- each leg
says which.

## Where the hosted runtime comes from

Each hosted-runtime gate links a .NET 11 program, compiled ahead of time by
the .NET SDK, against the .NET runtime's native archives built for its
processor class. The two programs under `src/rt/gate` carry the project
properties, the SDK pin and the single package source; they are published with

    dotnet publish -c Release -r linux-arm

on a project with `PublishAot` set, and the gate links the object the SDK
writes under `obj/Release/net11.0/linux-arm/native/`.

Lamella RHU builds the runtime archives itself from the dotnet/runtime
source at its pinned tag (.NET 11 RC1 today), with clang 22.1.8,
through the runtime repository's own build entry in a cross environment
whose target sysroot is Ubuntu 22.04's armhf packages:

    ./build.sh clr.nativeaotruntime -c Release -arch arm -cross

with one build flag moved from the stock build:

    CLR_ARM_FPU_TYPE=fpv5-d16

The stock pack is built for application-profile cores, whose FPU has
thirty-two double-precision registers; the Cortex-M7's FPU has sixteen,
so a stock archive contains instructions the Cortex-M7 cannot execute.
The flag keeps every compiled function inside the sixteen. The Cortex-M33
class's archives are built by the same recipe with `fpv5-sp-d16`: that
core's FPU is single-precision, so the compiler emits software
double-precision arithmetic, and the hosted-runtime gate for that class
consumes those archives. The stock runtime pack for the same version,
`Microsoft.NETCore.App.Runtime.NativeAOT.linux-arm`, is the parity control:
it and the RHU build have the same archive members with the same defined
and undefined names, and differ only in that flag's effect.

