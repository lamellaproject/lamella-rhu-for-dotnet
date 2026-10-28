//! STM32F746G-DISCO (STM32F746NG, Cortex-M7 r0p1): the family's second
//! board. Flash-boot at 0x08000000, USART1 console on the ST-LINK VCP
//! (PA9 transmits, PB7 receives). Everything but the board facts below
//! is the family module's (`stm32f7.rs`): the boot posture, the two clock
//! postures (`st-stm32f746g-disco-pll216` selects PLL216; the part is
//! rated 216 MHz with the same wait states and over-drive rules as the
//! family's first board), the console driver, the random source and the
//! calendar facts. Register addresses per RM0385 and the board's user
//! manual (UM1907). This row hosts the kernel alone: its 320 K of RAM is
//! under the hosted runtime's floor, so the row carries no payload map.

pub use super::stm32f7::*;

pub const NAME: &str = "STM32F746G-DISCO";
/// The chip, as the SDK's facts file names it (the quiet line of the log doors).
pub const CHIP: &str = "STM32F746NG";
pub const CONSOLE: &str = "USART1 (ST-LINK VCP)";

/// FPU shape, per part: the STM32F746NG's FPU is single-precision (DS10916:
/// "its single precision FPU"), so the double-precision forms take the
/// kernel's emulation (`fpemul`); the board facade ties the feature to this
/// fact at compile time.
pub const FPU_DP: bool = false;

/// The console's pins on the ST-LINK bridge (UM1907: PA9 VCP_TX, PB7
/// VCP_RX -- the receive pin on port B), as (port index, pin); the family
/// routes them to USART1.
pub const CONSOLE_PINS: [(usize, u32); 2] = [(0, 9), (1, 7)];

/// The board's high-speed crystal (X1, 25 MHz per UM1907): decode-only,
/// the postures never enable it.
#[allow(dead_code)] // read by the family's clock read-back on the PLL posture alone
pub const HSE_HZ: u32 = 25_000_000;

/// Thread stack pool (lives in .kstacks): 64 KiB on this row's 320 K of
/// contiguous RAM (the DTCM inside DATA) -- the idle thread's 1 KiB, a
/// hosting thread's and a finalizer's 16 KiB each and one default 16 KiB
/// worker, the shape the emulator rows carry; the link asserts the arena
/// window that remains above the row's floor.
pub const STACK_POOL_BYTES: usize = 64 * 1024;

/// The boot kheap carve out of the arena (the GC board table's
/// `kheap_carve` row): the 16K carve of the small rows.
pub const KHEAP_BYTES: usize = 16 * 1024;

/// External IRQ lines on the F74x/F75x (RM0385 vector table, positions
/// 0..=97) -- sizes the vector table.
pub const NIRQ: usize = 98;

/// The vector line a test image may claim: RM0385's position 97 (SPDIFRX),
/// implemented on this part, its peripheral never clocked by the kernel,
/// the line claimed by no kernel handler.
#[allow(dead_code)] // board-facts surface; the api flavor's band-interrupt control is its consumer
pub const SPARE_IRQ: usize = 97;

/// RAM windows a hardware-pushed exception frame may plausibly live in:
/// DTCM+SRAM1+SRAM2, the one contiguous 320 K window this image uses
/// (stacks live here; each end backed off by one 8-word frame).
pub const FRAME_RANGES: [(usize, usize); 1] = [(0x2000_0000, 0x2005_0000 - 32)];
