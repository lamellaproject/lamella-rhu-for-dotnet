# Lamella RHU&trade; for .NET

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
  funnel that decodes and reports CFSR/HFSR state (the base gate)
- A preemptive scheduler: BASEPRI-based critical sections, a reserved
  zero-latency interrupt band that is never masked, lazy-FP-aware context
  switching (the scheduler gate)
- Payload hosting: NativeAOT-compiled test payloads linked into the kernel
  image and verified against golden transcripts (the payload gate)
- The hosting API a runtime consumes: futexes and nanosecond time, a
  zero-initialized memory arena and a heap, threads with per-module TLS, a
  module registry, and the suspend/resume and activation primitives a garbage
  collector needs (the hosting-api gate on the emulator; the host gate for
  the arithmetic modules)
- A garbage collector running against the .NET runtime's GC interface:
  allocation, stop-the-world mark and sweep, finalization, and per-board memory
  configurations (the garbage-collector gate, on frozen objects its
  re-derivation leg rebuilds from `src/gc`)
- Hosted-runtime bring-up: a real .NET program (NativeAOT) is linked with the
  runtime, the shim and the collector into one image and booted on the
  emulator by the hosted-runtime gate; two programs ship as its fixtures, a
  hello program and a ten-section runtime suite (smoke, collections, text,
  exceptions, GC, time, dispatch, threads contending on a lock, thread
  suspension for collection, and a worker parked on a monitor across
  collections) -- the hosted-runtime gates, one per processor class
- The application path -- a program of your own, built with standard .NET
  tooling against the `Lamella.Rhu.Sdk` package and deployed to a board --
  opens at the launch, when the SDK and tools packages publish; today a
  checkout builds the kernel and runs the verification chain, and the
  SDK's board table carries a row for each emulator machine and each
  supported board
- The standard GPIO package: a program's `new GpioController()` from
  `System.Device.Gpio` runs unchanged -- the package's own Linux driver
  binds the libgpiod v1 API, which the hosting layer presents over the
  kernel's GPIO doors (the claim with its pull, the release, the read, the
  write, both-edge events to a waiting thread), proven by the hosted-runtime
  suite's GPIO section on the emulator rows' virtual lines
- The same package's I2C and SPI classes: a program's `I2cDevice.Create`
  and `SpiDevice.Create` run unchanged -- the package's own Linux drivers
  open the device files `/dev/i2c-<bus>` and `/dev/spidev<bus>.<select>`,
  which the hosting layer presents over the kernel's bus doors (a bus
  claimed for the program's run and shared by its devices, each SPI device
  with its own mode and clock under its own chip select), proven by the
  hosted-runtime suite's I2C and SPI sections on the emulator rows' block
  models; a bus is numbered by the chip's own block number, as Linux names
  it, and two things differ from Linux and are stated here once: an I2C
  message carries at most 255 bytes, and while a program only reads from an
  SPI device the bytes clocked out are ones
- An in-image header at a fixed offset stating each image's load address, ABI
  generation and debug-layout version, for flash tooling and debuggers (read
  back from every linked image by the payload and hosted-runtime gates)
- A calendar and a secure random source: `DateTime.UtcNow` and file times
  count from the .NET epoch until a board clock, the debug channel or the
  application sets the base, and a native timed wait against the real-time
  clock follows the calendar, a later set included; random bytes come from a
  hardware source or the call is refused, and the emulator images take a
  software stream by a build option their boot banner names (the hosting-api
  gate's entropy cases; the shim gate's calendar, timed-wait and secure-door
  controls)
- Double precision on the Cortex-M33 boards, whose FPU is single-precision:
  the kernel traps and emulates the double-precision instructions, so a
  program's `double` arithmetic gives the same results as on the Cortex-M7
  at the cost of one fault per emulated instruction (the hosting-api gate
  computes one vector set on the Cortex-M7 row's FPU and on the Cortex-M33
  row through the emulation and compares the results)
- GPIO for a program's `System.Device.Gpio` driver: a pin is claimed by one
  owner at a time, read as an input or driven as an output, with edge events
  delivered to a waiting thread, on the emulator and on every supported board
  (the hosting-api gate's three GPIO cases and a doctored control)
- I2C and SPI for a program's `System.Device.I2c` and `System.Device.Spi`
  drivers: a bus is claimed for the program's run and shared by its devices
  under one tenancy, an I2C transaction of up to eight messages with
  repeated starts between them, an SPI exchange with each device's own mode
  and clock under its own chip select, every wait on the wire a bounded
  poll and nothing under the kernel's critical section, on the emulator
  rows' block models (the hosting-api gate's three bus cases and a doctored
  control) and on the silicon of the ST and RP2350 rows whose facts carry a bus; a row
  whose facts carry no bus answers a bus count of zero
- Supported boards: Pimoroni Pico Plus 2, Pimoroni Pico Plus 2 W, Raspberry Pi
  Pico 2, Raspberry Pi Pico 2 W, ST NUCLEO-U5A5ZJ-Q, ST STM32F769I-DISCO

To be notified when releases are available, click **Watch** at the top of this
repo, select **Custom**, and check **Releases**.

## Building

The only prerequisite is Rust: the exact toolchain and its
cross-compilation targets are pinned by `rust-toolchain.toml` and resolved
automatically by rustup.

    cd src/kernel
    cargo build --release

Each supported board's build, what it produces and how a program is hosted
are documented in [docs/BUILDING.md](docs/BUILDING.md).

A program may ask which board it was built for: `new Lamella.Hardware.TargetBoard()`
answers the board's name, chip and clock, its pins by the labels printed on the
board, and its buses with the rates the image serves; the package `Lamella.Hardware`
will come from the same feed as the SDK.

## Verifying (optional)

The unikernel's quality gate chain builds each kernel configuration, runs it
on QEMU, and checks its behavior. To run it via Rust's cargo:

    cd src/harness
    cargo run --release -- chain

The prerequisites, which gates a checkout runs and how the chain finds the
emulator and the .NET runtime pack are documented in
[docs/VERIFYING.md](docs/VERIFYING.md).

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

<sub>Lamella and RHU are trademarks of Lamella LLC.</sub>
