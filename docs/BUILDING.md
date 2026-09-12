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
verification chain builds every one of them with the `payload` feature
described below, and the gate's library build of the default configuration
also with `entropy-accept-software`, the software-entropy opt-in its checks
need:

| Board | Target | Features |
| --- | --- | --- |
| QEMU mps2-an500 (Cortex-M7) | `thumbv7em-none-eabihf` | `qemu-exit,board-mps2-an500` (the default) |
| QEMU mps2-an505 (Cortex-M33) | `thumbv8m.main-none-eabihf` | `qemu-exit,board-mps2-an505` |
| NUCLEO-U5A5ZJ-Q (Cortex-M33) | `thumbv8m.main-none-eabihf` | `board-nucleo-u5a5zj-q` |
| STM32F769I-DISCO (Cortex-M7) | `thumbv7em-none-eabihf` | `board-stm32f769i-disco` |
| Raspberry Pi Pico 2 (Cortex-M33) | `thumbv8m.main-none-eabihf` | `board-pico-2` |
| Pimoroni Pico Plus 2 W (Cortex-M33) | `thumbv8m.main-none-eabihf` | `board-pico-plus-2` |

A board build names its target and turns the default features off, for
example:

    cargo build --release --target thumbv8m.main-none-eabihf --no-default-features --features board-pico-2

The `qemu-exit` feature belongs to the emulator boards alone: it ends a run
through the emulator's exit call, which silicon does not have.

## Hosting a program

With the `payload` feature added to a board's features, the build produces
the kernel as a library, `librhu_kernel.a` under `target/<target>/release/`,
that a hosted program is linked against under the board's linker map,
`src/kernel/payload-<board>.x`. The map places the program's sections, the
kernel's, and the memory arena the hosted runtime allocates from. The
verification chain's payload gate performs these six builds and links a
frozen program object under every map, and checks the placement, the code
region's free space and the arena window off each linked image; see
[docs/VERIFYING.md](./VERIFYING.md).
