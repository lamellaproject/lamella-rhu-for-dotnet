# Getting started

[Project overview](../README.md) | [Peripherals and runtime behavior](PERIPHERALS.md)

## Prepare to build the kernel

Rust is the only prerequisite.
`rust-toolchain.toml` pins the exact toolchain and cross-compilation targets.
Rustup resolves them automatically.

## Build the kernel

Use the [kernel build commands](../README.md#build-the-kernel), starting from the checkout root.
Read [building details](BUILDING.md) for each board's build, its output, and how programs are hosted.

## Run the verification chain (optional)

The verification chain builds each kernel configuration, runs it on QEMU, and checks its behavior.
Read [verification details](VERIFYING.md) for prerequisites, available checks, and how the chain finds the emulator and .NET runtime pack.
Then use the [verification commands](../README.md#run-the-verification-chain-optional).

## Build a .NET program

A program takes the hosted runtime through the Lamella SDK (`Lamella.Sdk`): a project names it in its `Sdk` attribute with `net11.0` as its target framework, and a single file names it with `#:sdk Lamella.Sdk@<version>` and `#:property TargetFramework=net11.0`; the SDK's shell restores RHU's build, board, runtime and tools packages by exact version.
The board class `Lamella.Hardware.TargetBoard` comes in the package `Lamella.Hardware`; the publish compiles the board's facts into a small assembly beside your program, so `new TargetBoard()` answers for the board you named. RHU's package sources are under `src/sdk`.
This checkout builds the kernel, and the verification chain's gates build the hosted programs.

A publish names its board by the property `LamellaTargetBoard`; a publish that names no board is refused. A build without a board produces the program's assembly; the image comes from a publish or a run that names a board.
An application names `<OutputType>Exe</OutputType>`, as .NET's console template writes it; a class library keeps .NET's default, builds and packs through the SDK, and is refused at publish (an image is an application's).
No C compiler is needed on any host: the SDK's own linker links the image.
The SDK's board table has a row for each emulator machine and each supported board.
See [how a program is hosted](BUILDING.md) and [where the hosted runtime comes from](VERIFYING.md).

## Run a .NET program

`dotnet run --list-devices` lists the devices a program can run on: each attached debug probe (`probe:<serial>`) and, for an emulator row, the emulator machine (`emulator:<machine>`) where `qemu-system-arm` is installed (the SDK never installs it); a project that names no board lists each probe with each board it reaches, and the chosen device names the board.
`dotnet run --device <id>` publishes the image, writes it to the board over that probe (or starts the emulator), prints the run's execution model, shows the program's output on your terminal and returns the program's exit code unchanged.
The tool that writes the image and reads the program's output is the Lamella SDK's own, restored with it; nothing else is installed.
A plain `dotnet run` builds the Debug configuration, as everywhere in .NET; `dotnet run -c Release --device <id>` writes an optimized image.
`dotnet run --no-build --device <id>` runs the image of the last publish as it stands and publishes nothing, whatever changed since (a source, a knob); with no image published yet it is refused naming the image. A run without the option builds, publishes and runs.
A program on a board takes no arguments yet: arguments after `--` are refused with one sentence.
A single-file program builds and publishes an image; `dotnet run` reaches a device from a project.

## Understand what the checks cover

A gate is a verification check; several gates run together as the verification chain.
Golden transcripts record the expected output of test runs.
A deliberately modified test control is one build of a test with one thing broken on purpose; it must fail at the check it exercises.

| Component | Implemented behavior | Verification |
| --- | --- | --- |
| Kernel startup | Boot and console on QEMU mps2-an500, Cortex-M7. A common fault handler decodes and reports CFSR/HFSR state. | base gate |
| Preemptive scheduler | BASEPRI-based critical sections and a reserved, unmasked zero-latency interrupt band. Context switching handles lazy floating-point state. | scheduler gate |
| Payload hosting | NativeAOT test payloads link into the kernel image. Their output is compared with golden transcripts. | payload gate |
| Hosting API | Futexes, nanosecond time, a zero-initialized memory arena, and a heap. Threads have per-module TLS. A module registry and suspend/resume and activation primitives support garbage collection. | hosting-api gate on the emulator; host gate for the arithmetic modules |
| Garbage collector | Uses the .NET runtime's GC interface. Supports allocation, stop-the-world mark and sweep, finalization, and per-board memory configurations. | garbage-collector gate; its re-derivation leg rebuilds the frozen objects from `src/gc` |

The host gate runs the kernel's arithmetic modules through Rust's own test runner on your machine, compiled from their own source files, with no emulator.
Those modules are the arena, the heap, the wake clock, the calendar, the double-precision emulation, the pure halves of the GPIO, bus, PWM and ADC doors, the SD card layer over a scripted host, the network checksum matcher, the network doors' scripted peer, and the UART pin words.

The hosted-runtime gates link a real NativeAOT .NET program, the runtime, the compatibility layer, and the collector into one image.
They boot that image on the emulator, with one gate per processor class.
Their two test programs are a hello program and a seventeen-section runtime suite.

| Suite area | Covered behavior |
| --- | --- |
| General runtime | smoke, collections, text, exceptions, GC, time, dispatch |
| Thread synchronization | Threads contend on a lock. A 64-bit counter is shared with a worker through the interlocked family. |
| Collection and threads | Threads suspend for collection. A worker remains parked on a monitor across collections. |
| Math | The C library's math functions are reached through the runtime's imports. |
| Peripherals | The standard GPIO, I2C, SPI and PWM packages run over the emulator's virtual lines, bus models and virtual timers; see [PERIPHERALS](PERIPHERALS.md). |
| Managed faults | A null reference is caught as the managed exception. |

## Inspect image metadata

Each image has a header at a fixed offset for flash tools and debuggers.
It records the load address, ABI generation, and debug-layout version.
The payload and hosted-runtime gates read the header back from every linked image.
