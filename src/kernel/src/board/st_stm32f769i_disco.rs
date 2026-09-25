//! STM32F769I-DISCO (STM32F769NI, Cortex-M7 r1p0): the first silicon
//! board. Flash-boot at 0x08000000, USART1 console on the ST-LINK VCP
//! (PA9 transmits, PA10 receives). Everything but the board facts below
//! is the family module's (`stm32f7.rs`): the boot posture, the two clock
//! postures (`st-stm32f769i-disco-pll216` selects PLL216), the console
//! driver, the random source and the calendar facts. Register addresses
//! per RM0410 and the board's user manual (UM2033).

pub use super::stm32f7::*;

pub const NAME: &str = "STM32F769I-DISCO";
pub const CONSOLE: &str = "USART1 (ST-LINK VCP)";

/// FPU shape, per part: the STM32F769NI's FPU computes double precision
/// (DS11532: "double-precision and single-precision data-processing
/// instructions"), so this row runs no emulation; the board facade ties the
/// feature to this fact at compile time.
pub const FPU_DP: bool = true;

/// The console's pins on the ST-LINK bridge (UM2033: PA9 VCP_TX, PA10
/// VCP_RX), as (port index, pin); the family routes them to USART1.
pub const CONSOLE_PINS: [(usize, u32); 2] = [(0, 9), (0, 10)];

/// The board's high-speed crystal (X2, 25 MHz per UM2033): decode-only,
/// the postures never enable it.
#[allow(dead_code)]
pub const HSE_HZ: u32 = 25_000_000;

/// Thread stack pool (lives in .kstacks): 96 KiB on this row's 512 K of
/// contiguous RAM (the DTCM inside DATA) -- the idle thread's 1 KiB, the
/// hosting thread's and the finalizer's 16 KiB each, and three default
/// 16 KiB workers; the link asserts the arena window that remains above
/// the row's floor.
pub const STACK_POOL_BYTES: usize = 96 * 1024;

/// The boot kheap carve out of the arena (the GC board table's
/// `kheap_carve` row): in a hosted image the C library's whole heap.
/// 16K is the mini row's carve, funded inside the payload map's 192K
/// floor beside the 96K region and a frozen-object pool of one stock
/// commit; the first real hosted program booted to its verdict on
/// carves down to 8K (measured on the QEMU twin at this revision).
pub const KHEAP_BYTES: usize = 16 * 1024;

/// External IRQ lines on the F76x/F77x (RM0410 vector table, positions
/// 0..=109) -- sizes the vector table.
pub const NIRQ: usize = 110;

/// The vector line a test image may claim: RM0410's position 109 (MDIOS),
/// implemented on this part, its peripheral never clocked by the kernel,
/// the line claimed by no kernel handler.
#[allow(dead_code)]
pub const SPARE_IRQ: usize = 109;

/// RAM windows a hardware-pushed exception frame may plausibly live in:
/// DTCM+SRAM1+SRAM2, the one contiguous 512 K window this image uses
/// (stacks live here; each end backed off by one 8-word frame).
pub const FRAME_RANGES: [(usize, usize); 1] = [(0x2000_0000, 0x2008_0000 - 32)];
