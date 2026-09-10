# Lamella&trade; RHU for .NET

![Status](https://img.shields.io/badge/status-in_development-orange)

A runtime-hosting unikernel for .NET: runs full .NET (NativeAOT) on
microcontrollers. Pronounced "roo."

Lamella RHU (the Lamella Runtime-Hosting Unikernel) is a from-scratch OS kernel
built and optimized purely for hosting the full .NET runtime -- with dotnet/iot
support -- on Cortex-M class hardware. The whole OS is one purpose-built
kernel: its API surface is exactly what the hosted runtime consumes, nothing
else.

Lamella RHU is part of the [Lamella project](https://github.com/lamellaproject/lamella):
Lamella's toolchain brings C# to even the smallest parts; Lamella RHU runs the
full .NET runtime wherever there is headroom for it.

Lamella RHU is free and open source under the [licenses below](#license).

## Status

Lamella RHU is in development and is **not yet ready for use**.

In the tree today:

- Kernel bring-up on QEMU (mps2-an500, Cortex-M7): boot, console, and a fault
  funnel that decodes and reports CFSR/HFSR state
- A preemptive scheduler: BASEPRI-based critical sections, a reserved
  zero-latency interrupt band that is never masked, lazy-FP-aware context
  switching
- Payload hosting: NativeAOT-compiled test payloads linked into the kernel
  image and verified against golden transcripts
- The hosting API a runtime consumes: futexes and nanosecond time, a
  zero-initialized memory arena and a heap, threads with per-module TLS, a
  module registry, and the suspend/resume and activation primitives a garbage
  collector needs
- A garbage collector running against the .NET runtime's GC interface:
  allocation, stop-the-world mark and sweep, finalization, and per-board memory
  configurations
- Hosted-runtime bring-up: a real .NET program (NativeAOT) is linked with the
  runtime, the shim and the collector into one image and booted on the
  emulator by the hosted-runtime gate; two programs ship as its fixtures, a
  hello program and a ten-section runtime suite (smoke, collections, text,
  exceptions, GC, time, dispatch, threads contending on a lock, thread
  suspension for collection, and a worker parked on a monitor across
  collections)
- An in-image header at a fixed offset stating each image's load address, ABI
  generation and debug-layout version, for flash tooling and debuggers
- Calendar time reads as uptime: the hosted runtime's clock starts at the
  Unix epoch at boot and advances with the elapsed clock, so `DateTime.UtcNow`
  and file times are boot-relative. The calendar policy shared with the
  Lamella runtime is the destination: an unset calendar reads the .NET epoch
  plus uptime, and the base is established by a board clock read at boot, an
  explicit network synchronization or the debug channel, never an automatic
  network sync
- Supported boards: NUCLEO-U5A5ZJ-Q, Pimoroni Pico Plus 2 W,
  Raspberry Pi Pico 2, STM32F769I-DISCO

Further board support and the remaining runtime surface land incrementally.

To be notified when releases are available, click **Watch** at the top of this
repo, select **Custom**, and check **Releases**.

## Building

The only prerequisite is Rust: the exact toolchain is pinned by
`rust-toolchain.toml` and resolved automatically by rustup (target
`thumbv7em-none-eabihf`).

    cd src/kernel
    cargo build --release

## Verifying (optional)

The unikernel's quality gate chain builds each kernel configuration, runs it
on QEMU, and checks its behavior. To run it via Rust's cargo:

    cd src/harness
    cargo run --release -- chain

The chain finds the emulator through the `QEMU` environment variable (the
path of the executable), or else `qemu-system-arm` on your PATH, or else the
first line of the file `tools/toolchains/qemu.local`; if none resolves, it
refuses.

Please note that gate results are pinned against a specific QEMU release
(currently the QEMU 11.0 series; each gate's result line reports the exact
QEMU and Rust versions it ran under). If your QEMU version is different, the
chain may report correspondingly-different results.

### Which gates a checkout runs

The chain has eight gates, published in this repository and driven through
Rust (cargo). Three of them -- the base, scheduler and hosting-api gates --
run the kernel under QEMU. The host gate runs the kernel's arithmetic
modules (the heap's sizing, the wake deadlines, the arena's bounds) through
Rust's own test runner on your machine; they are hardware-independent, so
it needs no emulator. The remaining four -- the payload, garbage-collector,
shim and hosted-runtime gates -- run under QEMU and also need the directory of
frozen inputs published under `goldens/` (compiled objects and golden
transcripts whose bytes the gate pins). The chain runs every gate whose
inputs are present and names each tier it could not run on its verdict line:
a passing chain never overstates what it checked.

### Re-deriving the frozen objects

Each frozen tier has a re-derivation leg that rebuilds its objects from
source and checks them byte for byte against the tier.

From `src/harness`:

    cargo run --release -- gc rederive
    cargo run --release -- shim rederive
    cargo run --release -- rt rederive

The garbage-collector and shim legs compile with clang 22.1.8; the C library
and headers they compile against are the Arm GNU Toolchain 12.2.MPACBTI-Rel1's
(arm-none-eabi). That kit is used only as the sysroot, never as a compiler,
and the shim leg refuses another release of it. A clang reporting another
version is refused.

Choose the compiler and the sysroot through environment variables:

    RHU_CLANG=<the clang 22.1.8 command>
    RHU_CLANG_SYSROOT=<the toolchain's arm-none-eabi directory>
    RHU_NEWLIB=<the kit's directory holding libc_nano.a for v7e-m+dp/hard>
    RHU_RT_ARCHIVES=<the directory of runtime archives described below>

or through untracked pointer files under `tools/toolchains/`, one value per
line: `clang.local` (two lines: the command, then the sysroot),
`newlib.local` and `rtarchives.local`. The shim leg's C-library case needs
`RHU_NEWLIB`; the hosted-runtime leg needs `RHU_RT_ARCHIVES`. The kit's
directory layout differs by host: the Linux kit keeps that library under
`arm-none-eabi/lib/thumb/v7e-m+dp/hard`, whereas the Windows kit keeps it
under `lib/gcc/arm-none-eabi/12.2.1/thumb/v7e-m+dp/hard`.

The legs check byte identity against the objects the compiler BUILD of
record produced (the LLVM 22.1.8 release from llvm.org, run on Linux).
Another build of clang 22.1.8 will differ at least in the producer string the
objects carry, so its rebuild reads as a difference on that leg even when
the code it generates is the same.

On Windows, the Arm GNU Toolchain unpacks to paths longer than the default
260-character limit: enable long paths, or unpack it under WSL.

### Where the hosted runtime comes from

The hosted-runtime gate links a .NET 11 program, compiled ahead of time by
the .NET SDK, against the .NET runtime's native archives. The two programs
under `src/rt/gate` carry the project properties, the SDK pin and the single
package source; they are published with

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
The flag keeps every compiled function inside the sixteen. The stock
runtime pack for the same
version, `Microsoft.NETCore.App.Runtime.NativeAOT.linux-arm`, is the parity
control: it and the RHU build have the same archive members with the same
defined and undefined names, and differ only in that flag's effect.

## Contributing

The Lamella Project encourages community collaboration.  Many Lamella repositories
gladly accept pull requests under a contributor agreement (to ensure that all code
remains free to use, dual-licensed as Apache-2.0 / MIT).

Lamella RHU is purpose-built to host the .NET runtime, and its value depends on
staying exactly as small as that one job requires. To protect that design, this
repository accepts issues only, not pull requests. If you find a bug, please
feel welcome to
[open an issue](https://github.com/lamellaproject/lamella-rhu-for-dotnet/issues).

## How this repo is managed

Lamella RHU is built by humans using traditional software engineering practices, in
collaboration with frontier AI models. @lamella-mel, AI co-author for this project,
will often interact with community members to help support and accelerate
their requests.

## About the name

**RHU** stands for **Runtime-Hosting Unikernel**, pronounced "roo." A unikernel
is an operating system built as a single thin layer under a single application.
RHU takes that shape to its logical end for .NET: the whole OS is one
purpose-built kernel whose only job is hosting the runtime, so nothing sits
between your program and the hardware that does not serve it.

"For .NET" describes what it runs: Lamella RHU is an independent project and is
not affiliated with or endorsed by Microsoft.

## License

Dual licensed under either of

- MIT license ([LICENSE-MIT](LICENSE-MIT))
- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))

at your option.

<sub>Lamella is a trademark of Lamella LLC.</sub>
