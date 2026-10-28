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
| Pimoroni Pico LiPo 2 (Cortex-M33) | `thumbv8m.main-none-eabihf` | `board-pimoroni-pico-lipo-2` (`pimoroni-pico-lipo-2-xtal12` for the 12 MHz crystal clock) |
| Pimoroni Pico LiPo 2 XL W (Cortex-M33) | `thumbv8m.main-none-eabihf` | `board-pimoroni-pico-lipo-2-xl-w` (`pimoroni-pico-lipo-2-xl-w-xtal12` for the 12 MHz crystal clock) |
| Pimoroni Pico Plus 2 (Cortex-M33) | `thumbv8m.main-none-eabihf` | `board-pimoroni-pico-plus-2` (`pimoroni-pico-plus-2-xtal12` for the 12 MHz crystal clock) |
| Pimoroni Pico Plus 2 W (Cortex-M33) | `thumbv8m.main-none-eabihf` | `board-pimoroni-pico-plus-2-w` (`pimoroni-pico-plus-2-w-xtal12` for the 12 MHz crystal clock) |
| Raspberry Pi Pico 2 (Cortex-M33) | `thumbv8m.main-none-eabihf` | `board-rpi-pico2` (`rpi-pico2-xtal12` for the 12 MHz crystal clock) |
| Raspberry Pi Pico 2 W (Cortex-M33) | `thumbv8m.main-none-eabihf` | `board-rpi-pico2-w` (`rpi-pico2-w-xtal12` for the 12 MHz crystal clock) |
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

    cargo build --release --target thumbv8m.main-none-eabihf --no-default-features --features board-rpi-pico2

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

The standard `System.Device.Gpio` package runs unchanged. Its Linux driver
binds the libgpiod v1 API, which the hosting layer presents over the
kernel's GPIO doors. Two controllers in one program are two owners, as on
Linux: a pin opened through one is refused through the other until it is
closed, and the two may be used from separate threads.

The same package's I2C and SPI classes run unchanged the same way. Its Linux
drivers open `/dev/i2c-<bus>` and `/dev/spidev<bus>.<select>`, which the
hosting layer presents over the kernel's bus doors. A bus is numbered by the
chip's own block number, as Linux names it; an I2C message is at most 255
bytes; a read clocks out ones.

The same package's PWM class runs unchanged too. Its Linux driver reads and
writes `/sys/class/pwm/pwmchip0`, which the hosting layer presents over the
kernel's PWM doors. The channel number is the pin's GPIO number, and the
period file reads back the period the timer runs.

Every hosted thread's stack comes from a pool the kernel carves per board.
A thread that sets no stack size of its own
gets the default; a larger ask is clamped. The SDK property `RhuThreadStack`
(bytes) moves the default for a program. A value that is not a whole number,
is below the kernel's minimum, is above the row's clamp, or leaves fewer
than three such stacks in the pool beside the idle thread's is refused
before anything is compiled.

An idle thread-pool worker gives its stack back to the pool after 100 ms
(the runtime's own default keeps it for 20 s). The SDK property
`RhuThreadPoolThreadTimeoutMs` (milliseconds, or `-1` for never) moves that
time for a program. The runtime reads this setting from the hosting layer's
environment before a program's runtimeconfig switch, so the property is the
way to set it; a value that is not a whole number of milliseconds or `-1`, a
zero, or a value above the 32-bit range is refused before anything is
compiled.

On a board with external RAM (the PSRAM of the Pimoroni Pico Plus 2, Pico
Plus 2 W, Pico LiPo 2 and Pico LiPo 2 XL W and of the Adafruit Feather RP2350 with PSRAM, the
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
hosting layer finds the heap's home by the map.

On the STM32F769I-DISCO, whose 64 MB quad-SPI flash the kernel maps at
boot, the SDK property `RhuProgramMemory` says where a program's managed
code and read-only data live: `internal` (the default: in the chip's 2 MB
flash) or `external` (in the quad-SPI flash, executed in place, the managed
code's unwind tables with them). The build renames those sections of the
compiled object before the merge with the runtime archives, links the image
under the board's external-flash map, and refuses `external` on a board
without such a flash, a value outside the two, and `external` together with `RhuManagedMemory=internal`
(the external-flash map keeps the heap in the SDRAM). The image is then two
halves in one file, the chip's and the window's, written together; the
`lamella` tool of this release writes the chip's flash alone, so such an
image is written with a debugger that programs the board's quad-SPI flash.

The standard analog classes' voltage members (`ReadVoltage`, `MinVoltage`,
`MaxVoltage` and the controller's `VoltageReference`) compute where the
runtime host configuration option `Lamella.Hardware.AnalogVoltage.IsSupported`
is true. The SDK sets it per program: true when the board holds the library
the classes express a voltage in (the two Pimoroni Pico Plus 2 boards and
the Pico LiPo 2 and the Pico LiPo 2 XL W; the
STM32F769I-DISCO under `RhuProgramMemory=external`) and the program's own
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
