//! NUCLEO-L4R5ZI (STM32L4R5ZI, Cortex-M4 with a single-precision FPU):
//! an STM32L4+ board. Flash-boot at 0x08000000, LPUART1
//! console on the ST-LINK VCP (PG7 transmits, PG8 receives). Everything
//! but the board facts below is the family module's (`stm32l4plus.rs`):
//! the boot posture, the two clock postures (`st-nucleo-l4r5zi-pll120`
//! selects PLL120, the part's ceiling under the regulator's boost mode),
//! the console driver, the random source and the calendar facts. Register
//! addresses per RM0432 and the board's user manual (UM2179). This row
//! hosts the kernel alone: the hosted runtime is not built for its core
//! class, so the row carries no payload map.

pub use super::stm32l4plus::*;

pub const NAME: &str = "NUCLEO-L4R5ZI";
/// The chip, as the SDK's facts file names it (the quiet line of the log doors).
pub const CHIP: &str = "STM32L4R5ZI";
pub const CONSOLE: &str = "LPUART1 (ST-LINK VCP)";

/// The console's pins on the ST-LINK bridge (UM2179: PG7 LPUART1 TX, PG8
/// LPUART1 RX, both on port G by the board's default solder bridges), as
/// (port index, pin); the family routes them to LPUART1.
pub const CONSOLE_PINS: [(usize, u32); 2] = [(6, 7), (6, 8)];

/// The board's high-speed clock input (UM2179: HSE unused by default, no
/// crystal fitted; the ST-LINK's 8 MHz clock output is the fitted option):
/// decode-only, the postures never enable it.
#[allow(dead_code)] // read by the family's clock read-back on the PLL posture alone
pub const HSE_HZ: u32 = 8_000_000;

/// Thread stack pool (lives in .kstacks): 96 KiB on this row's 640 K of
/// contiguous RAM -- the idle thread's 1 KiB, a hosting thread's and a
/// finalizer's 16 KiB each, and three default 16 KiB workers, the shape
/// the family's larger rows carry; the link asserts the arena window that
/// remains above the row's floor.
pub const STACK_POOL_BYTES: usize = 96 * 1024;

/// The boot kheap carve out of the arena (the GC board table's
/// `kheap_carve` row): the 16K carve of the small rows.
pub const KHEAP_BYTES: usize = 16 * 1024;

/// External IRQ lines on the STM32L4+ (RM0432 vector table, positions
/// 0..=94) -- sizes the vector table.
pub const NIRQ: usize = 95;

/// The vector line a test image may claim: RM0432's position 94 (the DMA
/// request multiplexer's overrun line), implemented on this part, its
/// peripheral never clocked by the kernel, the line claimed by no kernel
/// handler.
#[allow(dead_code)] // board-facts surface; the api flavor's band-interrupt control is its consumer
pub const SPARE_IRQ: usize = 94;

/// RAM windows a hardware-pushed exception frame may plausibly live in:
/// SRAM1+SRAM2+SRAM3, the one contiguous 640 K window this image uses
/// (stacks live here; each end backed off by one 8-word frame).
pub const FRAME_RANGES: [(usize, usize); 1] = [(0x2000_0000, 0x200A_0000 - 32)];
