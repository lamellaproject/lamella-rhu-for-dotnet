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
| Pimoroni Pico Plus 2 (Cortex-M33) | `thumbv8m.main-none-eabihf` | `board-pimoroni-pico-plus-2` (`pimoroni-pico-plus-2-pll150` for the 150 MHz clock) |
| Pimoroni Pico Plus 2 W (Cortex-M33) | `thumbv8m.main-none-eabihf` | `board-pimoroni-pico-plus-2-w` (`pimoroni-pico-plus-2-w-pll150` for the 150 MHz clock) |
| Raspberry Pi Pico 2 (Cortex-M33) | `thumbv8m.main-none-eabihf` | `board-rpi-pico2` (`rpi-pico2-pll150` for the 150 MHz clock) |
| Raspberry Pi Pico 2 W (Cortex-M33) | `thumbv8m.main-none-eabihf` | `board-rpi-pico2-w` (`rpi-pico2-w-pll150` for the 150 MHz clock) |
| ST NUCLEO-L4R5ZI (Cortex-M4) | `thumbv7em-none-eabihf` | `board-st-nucleo-l4r5zi` (`st-nucleo-l4r5zi-pll120` for the 120 MHz clock) |
| ST NUCLEO-U5A5ZJ-Q (Cortex-M33) | `thumbv8m.main-none-eabihf` | `board-st-nucleo-u5a5zj-q` (`st-nucleo-u5a5zj-q-pll160` for the 160 MHz clock) |
| ST STM32F746G-DISCO (Cortex-M7) | `thumbv7em-none-eabihf` | `board-st-stm32f746g-disco` (`st-stm32f746g-disco-pll216` for the 216 MHz clock) |
| ST STM32F769I-DISCO (Cortex-M7) | `thumbv7em-none-eabihf` | `board-st-stm32f769i-disco` (`st-stm32f769i-disco-pll216` for the 216 MHz clock) |

The STM32F746G-DISCO row hosts the kernel alone: its 320 KB of RAM is under
the hosted runtime's floor, so it has no payload map and no place in the
hosted-runtime link family. The NUCLEO-L4R5ZI row hosts the kernel alone:
no hosted-runtime tier exists for its Cortex-M4 core, so it has no payload
map.

A board's default clock is the part's reset clock (16 MHz on the ST boards,
the 12 MHz crystal on the RP2350 boards); the clock feature beside its board
feature selects the production clock, which the boot banner reads back. The
STM32F769I-DISCO's hardware random source needs the 48 MHz PLL output, so it
serves under `st-stm32f769i-disco-pll216` only: on the default clock the boot
banner says the source is absent, cryptographic draws are refused and the
software stream serves the rest.

A board build names its target and turns the default features off, for
example:

    cargo build --release --target thumbv8m.main-none-eabihf --no-default-features --features board-rpi-pico2

The `qemu-exit` feature belongs to the emulator boards alone: it ends a run
through the emulator's exit call, which silicon does not have.

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

A program of your own takes the hosted runtime through the
`Lamella.Rhu.Sdk` package, whose runtime archives carry every runtime
helper; that path opens at the launch, when the SDK and tools packages
publish, and the SDK's board table carries the two emulator rows and the
first two physical boards' rows today; the other boards' rows follow. A
program's `new GpioController()` from the standard
`System.Device.Gpio` package runs unchanged: the package's Linux driver
binds the libgpiod v1 API, which the hosting layer presents over the
kernel's GPIO doors. Two controllers in one program are two owners, as on
Linux: a pin opened through one is refused through the other until it is
closed, and the two may be used from separate threads. The same package's
I2C and SPI classes run unchanged
the same way: its Linux drivers open `/dev/i2c-<bus>` and
`/dev/spidev<bus>.<select>`, which the hosting layer presents over the
kernel's bus doors -- a bus numbered by the chip's own block number as
Linux names it, an I2C message of at most 255 bytes, and a read clocking
out ones. Every hosted thread's stack comes from a pool the kernel
carves per board, a fact the build prints beside the board line with the
row's default managed stack and its clamp: a thread that sets no stack
size of its own gets the default, a larger ask is clamped, and the SDK
property `RhuThreadStack` (bytes) moves the default for a program -- a
value that is not a whole number, is below the kernel's minimum, is above
the row's clamp, or leaves fewer than three such stacks in the pool beside
the idle thread's is refused before anything is compiled. The minimal
runtime under `src/payload` is the payload gate's fixture, sized to its
two test programs, not a target for a program of your own. A program's
published object is the same at any checkout and any commit: the SDK
leaves the checkout's revision and every source-control fact out of the
build, so a build's identity depends on the program, the package and the
tools alone.
