# Lamella RHU&trade; for .NET

![Status](https://img.shields.io/badge/status-in_development-orange)

A runtime-hosting unikernel for .NET: runs full .NET (NativeAOT) on
microcontrollers. Pronounced "roo."

Lamella RHU (the Lamella Runtime-Hosting Unikernel) is a from-scratch OS kernel
built and optimized purely for hosting the full .NET runtime -- with dotnet/iot
support -- on Cortex-M class hardware. The whole OS is one purpose-built
kernel: its API surface is exactly what the hosted runtime consumes, nothing
else.

Lamella RHU is part of the [Lamella project](https://github.com/lamellaproject/lamella).
Lamella's toolchain brings C# to even the smallest parts; Lamella RHU runs the
full .NET runtime wherever the hardware has enough capacity.

Lamella RHU is free and open source under the [licenses below](#license).

## Development status

Lamella RHU is in development and is **not yet ready for use**.

What works today:

- The verification chain runs a hosted .NET program on the emulator.
- The supported boards run the test suite's peripheral sections on their silicon: GPIO on every board, I2C and SPI where the board has a bus, PWM on the RP2350 boards.
- GPIO, I2C, SPI, PWM and the serial port use the standard System.Device and System.IO.Ports packages.
- Analog inputs are read through the standard `System.Device.Analog` classes.
- Supported boards:
  Adafruit Feather RP2350 with HSTX;
  Adafruit Feather RP2350 with HSTX and PSRAM;
  Pimoroni Pico LiPo 2; Pimoroni Pico LiPo 2 XL W;
  Pimoroni Pico Plus 2; Pimoroni Pico Plus 2 W;
  Raspberry Pi Pico 2; Raspberry Pi Pico 2 W;
  ST NUCLEO-U5A5ZJ-Q; ST STM32F769I-DISCO.

To be notified when releases are available, click **Watch** at the top of this
repo, select **Custom**, and check **Releases**.

Start with [GETTING-STARTED](docs/GETTING-STARTED.md).
See [PERIPHERALS](docs/PERIPHERALS.md) for hardware APIs and runtime behavior.

## Build the kernel

From the checkout root, using Rust's cargo:

```
cd src/kernel
cargo build --release
```

Read [docs/BUILDING.md](docs/BUILDING.md) for details.

## Run the verification chain (optional)

From the checkout root, run the chain through Rust's cargo:

```
cd src/harness
cargo run --release -- chain
```

Read [docs/VERIFYING.md](docs/VERIFYING.md) for details.

## Contributing

The Lamella Project encourages community collaboration.  Many Lamella repositories
gladly accept pull requests under a contributor agreement (to ensure that all code
remains free to use, dual-licensed as Apache-2.0 / MIT).

Lamella RHU is purpose-built to host the .NET runtime.
Its design depends on staying as small as that job requires. To preserve that design,
this repository accepts issues only, not pull requests.
If you find a bug, please [open an issue](https://github.com/lamellaproject/lamella-rhu-for-dotnet/issues).

## How this repo is managed

Lamella RHU is built by humans using traditional software engineering practices, in
collaboration with frontier AI models. @lamella-mel, AI co-author for this project,
will often interact with community members to help support and accelerate
their requests.

## About the name

**RHU** stands for **Runtime-Hosting Unikernel**, pronounced "roo."
A unikernel is an operating system built as a single thin layer under a single application.
RHU takes that shape to its logical end for .NET: the whole OS is one
purpose-built kernel whose only job is hosting the runtime, so nothing sits
between your program and the hardware that does not serve it.

"For .NET" describes what it runs.
Lamella is an independent project and is not affiliated with or endorsed by Microsoft.

## License

Dual licensed under either of

- MIT license ([LICENSE-MIT](LICENSE-MIT))
- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))

at your option.

<sub>Lamella and RHU are trademarks of Lamella LLC.</sub>
