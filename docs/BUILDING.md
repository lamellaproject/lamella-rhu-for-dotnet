# Building Lamella RHU

The only prerequisite is Rust: the exact toolchain is pinned by
`rust-toolchain.toml` and resolved automatically by rustup, together with
the two Cortex-M targets the kernel is built for, `thumbv7em-none-eabihf`
for the Cortex-M7 boards and `thumbv8m.main-none-eabihf` for the Cortex-M33
boards.

## The kernel

The kernel is the crate under `src/kernel`. Its default build is the QEMU
mps2-an500 configuration, a Cortex-M7 machine, with the emulator's exit
support:

    cd src/kernel
    cargo build --release

Exactly one board feature is selected per build; the crate refuses a build
that selects none or two. The boards and the build each one selects; the
verification chain's payload gate builds the default board with the
`payload` feature described below (and the gate's library build of the
default configuration also with `entropy-accept-software`, the
software-entropy opt-in its checks need), and every other board too when
the two hosted-runtime tiers it links are present:

| Board | Target | Features |
| --- | --- | --- |
| QEMU mps2-an500 (Cortex-M7) | `thumbv7em-none-eabihf` | `qemu-exit,board-mps2-an500` (the default) |
| QEMU mps2-an505 (Cortex-M33) | `thumbv8m.main-none-eabihf` | `qemu-exit,board-mps2-an505` |
| Adafruit Feather RP2350 with HSTX (Cortex-M33) | `thumbv8m.main-none-eabihf` | `board-adafruit-feather-rp2350-hstx` (`adafruit-feather-rp2350-hstx-xtal12` for the 12 MHz crystal clock) |
| Adafruit Feather RP2350 with HSTX and PSRAM (Cortex-M33) | `thumbv8m.main-none-eabihf` | `board-adafruit-feather-rp2350-hstx-psram` (`adafruit-feather-rp2350-hstx-psram-xtal12` for the 12 MHz crystal clock) |
| Adafruit Metro RP2350 (Cortex-M33) | `thumbv8m.main-none-eabihf` | `board-adafruit-metro-rp2350` (`adafruit-metro-rp2350-xtal12` for the 12 MHz crystal clock) |
| Adafruit Metro RP2350 with PSRAM (Cortex-M33) | `thumbv8m.main-none-eabihf` | `board-adafruit-metro-rp2350-psram` (`adafruit-metro-rp2350-psram-xtal12` for the 12 MHz crystal clock) |
| Pimoroni Pico LiPo 2 (Cortex-M33) | `thumbv8m.main-none-eabihf` | `board-pimoroni-pico-lipo-2` (`pimoroni-pico-lipo-2-xtal12` for the 12 MHz crystal clock) |
| Pimoroni Pico LiPo 2 XL W (Cortex-M33) | `thumbv8m.main-none-eabihf` | `board-pimoroni-pico-lipo-2-xl-w` (`pimoroni-pico-lipo-2-xl-w-xtal12` for the 12 MHz crystal clock) |
| Pimoroni Pico Plus 2 (Cortex-M33) | `thumbv8m.main-none-eabihf` | `board-pimoroni-pico-plus-2` (`pimoroni-pico-plus-2-xtal12` for the 12 MHz crystal clock) |
| Pimoroni Pico Plus 2 W (Cortex-M33) | `thumbv8m.main-none-eabihf` | `board-pimoroni-pico-plus-2-w` (`pimoroni-pico-plus-2-w-xtal12` for the 12 MHz crystal clock) |
| Raspberry Pi Pico 2 (Cortex-M33) | `thumbv8m.main-none-eabihf` | `board-raspberrypi-pico-2` (`raspberrypi-pico-2-xtal12` for the 12 MHz crystal clock) |
| Raspberry Pi Pico 2 W (Cortex-M33) | `thumbv8m.main-none-eabihf` | `board-raspberrypi-pico-2-w` (`raspberrypi-pico-2-w-xtal12` for the 12 MHz crystal clock) |
| SparkFun IoT RedBoard RP2350 (Cortex-M33) | `thumbv8m.main-none-eabihf` | `board-sparkfun-iot-redboard-rp2350` (`sparkfun-iot-redboard-rp2350-xtal12` for the 12 MHz crystal clock) |
| ST NUCLEO-U5A5ZJ-Q (Cortex-M33) | `thumbv8m.main-none-eabihf` | `board-st-nucleo-u5a5zj-q` (`st-nucleo-u5a5zj-q-hsi16` for the 16 MHz internal-oscillator clock) |
| ST STM32F769I-DISCO (Cortex-M7) | `thumbv7em-none-eabihf` | `board-st-stm32f769i-disco` (`st-stm32f769i-disco-hsi16` for the 16 MHz internal-oscillator clock) |

A board's default clock is the 160 MHz PLL on the ST NUCLEO board, the
216 MHz PLL on the STM32F769I-DISCO and the chip's rated 150 MHz system
PLL on the RP2350 boards, which the boot banner reads back; the clock
feature beside its board feature selects the other posture: the 16 MHz
internal oscillator on the two ST boards and the 12 MHz crystal on the
RP2350 boards (no clock line either way). The STM32F769I-DISCO's hardware random
source needs the 48 MHz PLL output, so it serves at the 216 MHz clock only:
under `st-stm32f769i-disco-hsi16` the boot banner says the source is
absent, cryptographic draws are refused and the software stream serves the
rest.

A board build names its target and turns the default features off, for
example:

    cargo build --release --target thumbv8m.main-none-eabihf --no-default-features --features board-raspberrypi-pico-2

The `qemu-exit` feature belongs to the emulator boards alone: it ends a run
through the emulator's exit call, which silicon does not have. The SDK
package's kernels for the two emulator rows are built with
`entropy-accept-software` beside the table's features, because the emulator
has no hardware noise source and every gate image on those rows is built
so; no silicon row's kernel names it.

## Hosting a program

With the `payload` feature added to a board's features, the build produces
the kernel as a library, `librhu_kernel.a` under `target/<target>/release/`,
that a hosted program is linked against under the board's linker map,
`src/kernel/payload-<board>.x`. The map places the program's sections, the
kernel's, and the memory arena the hosted runtime allocates from. The
verification chain's payload gate performs these six builds and links the
hosted runtime's frozen merged object under every map, checking the
placement, the code region's free space and the arena window off each
linked image, when the two hosted-runtime tiers are present; a checkout
without them runs the gate's other cases and names these NOT RUN on its
verdict line; see [docs/VERIFYING.md](./VERIFYING.md).

A program of your own takes the hosted runtime through the Lamella SDK
(`Lamella.Sdk`, whose `net11.0` branch is RHU's): the SDK's shell restores
RHU's build package (the targets), the board's package (its kernels, maps and
facts), the processor class's runtime package (the runtime archives, which
carry every runtime helper) and this host's tools package (the linker), each
by exact version. This checkout builds the kernel; the gates build the hosted
programs. RHU's board table has a row for each emulator machine and each
supported board.

The Debug configuration, which a plain `dotnet run` builds, leaves the
program's own code unoptimized and compiles everything else with the code
optimization on, on every board. The program's own code is the project's
assembly and the class libraries it references by project, directly or
through another project; the framework, every package and an assembly
referenced as a compiled file are compiled optimized. In the image's debug
information every statement of the program's own code that does something
has a row of its own, every local has one location for the whole of its
method, and every method is a function of its own, never inlined: two methods
of one body are two functions, and only instantiations of one generic method
may share one. The image's record says how it was compiled, in two keys: a
Debug image has `program_codegen` `unoptimized` and `codegen_profile`
`optimized`.

Four things differ from an image compiled without the optimization as a
whole. A line that holds only the brace of a block inside a method carries
no code, so a breakpoint set on it has no place of its own; a method's
closing brace keeps its code. A static field's initializer the compiler evaluates at build time
cannot be stepped through: that code ran when the image was built. Stepping
into a package's or the framework's code is not served. And a method the
program itself marks `AggressiveOptimization` stays optimized.

`Optimize` speaks for the program's own code alone, in any configuration:
`-p:Optimize=true` compiles the program's own code optimized in a Debug build
too (both keys `optimized`, as a Release image has them), and
`-p:Optimize=false` leaves it unoptimized in a Release build
(`program_codegen` `unoptimized`, `codegen_profile` `optimized`). A build with
nothing changed does not run the native compiler again.

A class library referenced by project is compiled under the SDK's path map,
as the program is: the image names its sources under `/rhu/src/` and carries
no path of the build machine. This has two costs: a referenced project's own
`PathMap` is replaced under an RHU program's build, and no byte-for-byte tie
across hosts is claimed for a program with a class library.

The standard `System.Device.Gpio` package runs unchanged. Its Linux driver
binds the libgpiod v1 API, which the hosting layer presents over the
kernel's GPIO API. Two controllers in one program are two owners, as on
Linux: a pin opened through one is refused through the other until it is
closed, and the two may be used from separate threads.

The same package's I2C and SPI classes run unchanged the same way. Its Linux
drivers open `/dev/i2c-<bus>` and `/dev/spidev<bus>.<select>`, which the
hosting layer presents over the kernel's bus API. A bus is numbered by the
chip's own block number, as Linux names it; an I2C message is at most 255
bytes; a read clocks out ones.

The same package's PWM class runs unchanged too. Its Linux driver reads and
writes `/sys/class/pwm/pwmchip0`, which the hosting layer presents over the
kernel's PWM API. The channel number is the pin's GPIO number, and the
period file reads back the period the timer runs.

Every hosted thread's stack comes from a pool the kernel carves per board.
A thread that sets no stack size of its own
gets the default; a larger ask is clamped. The SDK property `RhuThreadStack`
(bytes) moves the default for a program. A value that is not a whole number,
is below the kernel's minimum, is above the row's clamp, or leaves fewer
than three such stacks in the pool beside the idle thread's is refused
before anything is compiled.

The thread pool is bounded by the same pool. The build states the most
workers the board holds beside the kernel's own threads and the runtime's
service threads, in a line of its own with the arithmetic, and the runtime
is told that number. A program's `ThreadPool.SetMaxThreads` with another
number returns false; work items beyond the bound wait in the queue and all
run. .NET's own project property `ThreadPoolMaxThreads` above the board's
most, and `ThreadPoolMinThreads` above the bound, are refused before
anything is compiled.

The class library guards work that may nest deeply with the runtime's stack
helper: a record's `ToString()`, compiled expression trees, the
dependency-injection container, the non-backtracking regular expressions,
a task's continuation asked to run synchronously and a wait for a task on a
pool thread all ask it first. The helper says no while 6144 bytes of the
thread's stack are left,
so guarded code may use 10240 bytes of a 16384-byte stack and 2048 bytes of
a thread started at 8192, and what it throws then is
`InsufficientExecutionStackException`, which a program can catch. Code that
asks no helper finds the stack's end as a stack overflow, which ends the
image: formatting a double takes about 7680 bytes, and JSON nested 64 deep
needs a thread started with a larger stack (`new Thread(body, 24576)`).

An idle thread-pool worker gives its stack back to the pool after 100 ms
(the runtime's own default keeps it for 20 s). The SDK property
`RhuThreadPoolThreadTimeoutMs` (milliseconds, or `-1` for never) moves that
time for a program. The runtime reads this setting from the hosting layer's
environment before a program's runtimeconfig switch, so the property is the
way to set it; a value that is not a whole number of milliseconds or `-1`, a
zero, or a value above the 32-bit range is refused before anything is
compiled.

Named mutexes are supported, and how many a program can hold open at once
varies by board, according to the board's memory configuration.

On a board with external RAM (the PSRAM of the Pimoroni Pico Plus 2, Pico
Plus 2 W, Pico LiPo 2 and Pico LiPo 2 XL W, of the Adafruit Feather RP2350 with PSRAM, of the Adafruit Metro RP2350 with PSRAM and of the SparkFun IoT RedBoard RP2350, the
SDRAM of the STM32F769I-DISCO) the SDK property
`RhuExternalRamInit` chooses when that memory is zeroed: `boot` (the
default: all of it before the program starts), `first-use` (each piece at
the moment the runtime first asks for it) or `background` (ahead of use
whenever the processor is idle, the rest on demand). The build links the
kernel built for that mode and refuses the value on a board without
external RAM. The memory a program
receives reads zero in every mode.

On the same boards, and on the two emulator machines, whose 16 MB bank is
their external RAM, the SDK property `RhuManagedMemory` says where a
program's managed heap and the runtime's type data live: `external` (the
default: both in the external RAM, the chip's own RAM keeping the kernel,
the thread stacks and the runtime's own small allocations) or `internal`
(both in the chip's own RAM, as on a board without external RAM). The
build links the image under the board's payload map for `external` and
under the board's second map for `internal`, and refuses `internal` on a
board without external RAM and a value outside the two. The kernel is the same under either map: the
hosting layer finds the heap's home by the map. The one exception is a
board whose packages carry two kernels (`RhuNetwork` below): there
`internal` also takes the kernel without the network.

On the STM32F769I-DISCO, whose 64 MB quad-SPI flash the kernel maps at
boot, the SDK property `RhuProgramMemory` says where a program's managed
code and read-only data live: `external` (in the quad-SPI flash, executed
in place, the managed code's unwind tables with them) or `internal` (in the
chip's 2 MB flash). A project that names neither has the board's default
home, which is `external` on this board, and `internal` wherever
`RhuManagedMemory` is `internal`. For `external` the build renames those
sections of the compiled object before the merge with the runtime archives
and links the image under the board's external-flash map; it refuses
`external` on a board without such a flash, a value outside the two, and
`external` together with `RhuManagedMemory=internal` (the external-flash
map keeps the heap in the SDRAM). The image is then two halves in one file,
the chip's and the window's, written together: the `lamella` tool, from its
version 0.4.0, writes both as a matched set through the board's own ST-LINK
and reads both back.

On the same board the SDK property `RhuNetwork` says whether the image
links the kernel with the board's network: `on` or `off`. The board's
packages carry two sets of kernels, one built with the network and one
without it. A project that names nothing gets the kernel with the network,
and the kernel without it where `RhuManagedMemory` is `internal`, because
the network's buffers and the managed heap do not both fit the chip's own
RAM; the build says in a line of its own when a program is built without
networking. `off` takes the kernel without the network under every map and
gives the program the memory the network holds. The build refuses a value
outside the two, `on` together with `RhuManagedMemory=internal`, and either
value on a board with one kernel, before anything is compiled; an image
whose data leaves the chip's own RAM too little beside the network is
refused at the link, in words that name the property.

The Pimoroni Pico Plus 2 W and the Pico LiPo 2 XL W carry two sets of
kernels in the same way, and `RhuNetwork` rules there as above; their
network is the board's wifi radio. The radio runs firmware
of its own, in a place of its own in the board's flash. The board's package
carries it as one file with its record and its license, and the build names
that file in a line of its own (`the board's flash region for a deploy`). A
deploy writes it: `lamella flash <image> --region <the file>` writes the
image and, where the board's flash does not hold the firmware yet, the
firmware; a later deploy finds it in place and writes none of it. A program
built with the default log level prints nothing about the radio on its
console: the tool's lines say what the deploy did with the firmware, and
the kernel's wifi lines appear when the project sets `RhuLogLevel` to
`verbose` (below).

The standard analog classes' voltage members (`ReadVoltage`, `MinVoltage`,
`MaxVoltage` and the controller's `VoltageReference`) compute where the
runtime host configuration option `Lamella.Hardware.AnalogVoltage.IsSupported`
is true. The SDK sets it per program: true when the board holds the library
the classes express a voltage in (the two Pimoroni Pico Plus 2 boards and
the Pico LiPo 2, the Pico LiPo 2 XL W, the two Adafruit Metro RP2350 boards and the SparkFun IoT RedBoard RP2350; the
STM32F769I-DISCO while the program lies in its external flash, the default
there) and the program's own
code, or a project it references, names one of the four members. Everywhere
else the SDK sets the option false: the members throw `NotSupportedException` and the
image carries no code of the library. A project may set the option itself;
`true` on a board that does not hold the library is refused before anything
is compiled.

Three more properties say how loud the kernel is and where its output goes.
`RhuLogLevel` (`silent`, `quiet` or `verbose`; `quiet` unless the project
says otherwise) governs the kernel's own diagnostic lines: silent prints
nothing but a fatal error's last words, quiet prints one line naming the
version, the board and its clock, and verbose prints the whole boot report.
The program's own output is never filtered. `RhuLogChannel` and
`RhuConsoleChannel` (each `memory`, `probe`, `uart0`, `uart1` or `off`;
empty means the board's own default) route the kernel's lines and the
program's output to a sink: the 16 KB memory console read over the debug
port, the ST-Link boards' virtual COM port, a UART of the board's table held
by the kernel from boot, or nowhere. A value the board cannot serve is
refused before anything is compiled.

The minimal runtime under `src/payload` is the payload gate's fixture, sized
to its two test programs, not a target for a program of your own.

A program's published object is the same at any checkout and any commit: the
SDK leaves the checkout's revision and every source-control fact out of the
build, so a build's identity depends on the program, the package and the
tools alone.
