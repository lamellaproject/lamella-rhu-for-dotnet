# Lamella RHU for .NET

![Status](https://img.shields.io/badge/status-in_development-orange)

A runtime-hosting unikernel for .NET: runs full .NET 11 (NativeAOT) on
microcontrollers. Pronounced "roo."

Lamella RHU (the Lamella Runtime-Hosting Unikernel) is a from-scratch OS kernel
built and optimized purely for hosting the full .NET runtime -- with dotnet/iot
support -- on Cortex-M class hardware. The whole OS is one purpose-built
kernel: its API surface is exactly what the hosted runtime consumes, nothing
else.

Lamella RHU is part of the [Lamella project](https://github.com/lamellaproject/lamella):
Lamella's toolchain brings C# to even the smallest parts; Lamella RHU runs the
full .NET runtime wherever there is headroom for it. Lamella RHU is free and
open source under the [licenses below](#license).

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
- A cumulative gate chain with negative controls and machine-readable results

Hardware board support and the full runtime bring-up land incrementally.

To be notified when releases are available, click **Watch** at the top of this
repo, select **Custom**, and check **Releases**.

## Building

The only prerequisite is Rust: the exact toolchain is pinned by
`rust-toolchain.toml` and resolved automatically by rustup (target
`thumbv7em-none-eabihf`).

    cd src/kernel
    cargo build --release

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
