//! RP2350 (two Cortex-M33 cores; this kernel runs on core 0 in its Secure
//! state): the chip half shared by the boards built on it. A board module
//! states its own name, console description and kernel-heap carve and
//! re-exports everything else from here. The image executes in place from
//! the QSPI flash window at 0x10000000 through the chip's flash cache; the
//! on-chip boot ROM recognises it by the boot block this module places
//! inside the image's first 4 KB and enters through the vector table at
//! the image base.

use core::ptr::{read_volatile, write_volatile};

use super::ring;

/// The crystal on every board built on this chip.
const XOSC_HZ: u32 = 12_000_000;

/// Clock posture 1 (default): SYSCLK = the crystal, 12 MHz. `clock_init`
/// enforces exactly this posture.
#[cfg(not(any(feature = "pico-2-pll150", feature = "pico-plus-2-pll150")))]
pub const SYSCLK_HZ: u32 = 12_000_000;
/// Clock posture 2 (`<board>-pll150`): the system PLL at 150 MHz off the
/// crystal -- 12 MHz / 1 into the VCO, x125 = 1500 MHz (inside the
/// 400-1600 MHz window the PLL's control register states), / 5 / 2 lands
/// SYSCLK at 150 MHz, the chip's rated system clock.
#[cfg(any(feature = "pico-2-pll150", feature = "pico-plus-2-pll150"))]
pub const SYSCLK_HZ: u32 = 150_000_000;

/// The peripheral clock: the crystal, in both postures (stated by
/// `clock_init`, which selects and starts it).
const PERI_HZ: u32 = XOSC_HZ;

/// SysTick rate. The kernel clocks SysTick from the processor clock, so
/// the chip's tick-generator block is not needed.
pub const TICK_HZ: u32 = 1_000;

/// Thread stack pool (lives in .kstacks) -- the same budgets as the other
/// 512 K-class board.
pub const STACK_POOL_BYTES: usize = 64 * 1024;

/// Stack for the kmain thread (the boot thread's continuation).
pub const KMAIN_STACK_BYTES: usize = 16 * 1024;

/// External IRQ lines: 52 (positions 0..=51; the last six are spare lines
/// wired to nothing, and a line that never fires costs eight bytes of
/// flash), so every position the datasheet names has a funnel entry.
pub const NIRQ: usize = 52;

/// FPU shape: FPv5 SINGLE precision (the Cortex-M33 FPU) -- kernel f64
/// stays on the soft-float path. Consumed by records and gates, never by
/// context-switch code (lazy stacking is FP-shape-agnostic).
#[allow(dead_code)]
pub const FPU_DP: bool = false;
/// The cycle counter the lost-tick witness reads: the core's data
/// watchpoint and trace unit counts processor cycles, enabled and
/// verified to advance at the scheduler's init.
pub const CYCLE_COUNTER: bool = true;

/// RAM windows a hardware-pushed exception frame may plausibly live in:
/// the one contiguous SRAM window this image uses as DATA (the eight
/// striped banks and the two small ones after them,
/// 0x20000000..0x20082000), each end backed off by one 8-word frame.
pub const FRAME_RANGES: [(usize, usize); 1] = [(0x2000_0000, 0x2008_2000 - 32)];

/// The boot block the chip's boot ROM looks for inside the first 4 KB of a
/// flash image (the board maps place it right after the in-image header):
/// the start marker, one item saying "an executable Arm image for this
/// chip, entered in the Secure state" (item type 0x42, one word long:
/// image type 1 = executable, security 2 = Secure, CPU 0 = Arm, chip 1 =
/// RP2350), the last-item word carrying the block's item size, the
/// self-link that makes the block a loop of one, and the end marker.
/// Without it the ROM treats the flash as blank and waits for a USB
/// download instead of entering the vector table.
#[used]
#[unsafe(no_mangle)]
#[unsafe(link_section = ".rp2350_boot_block")]
pub static __rhu_rp2350_boot_block: [u32; 5] =
    [0xffff_ded3, 0x1021_0142, 0x0000_01ff, 0x0000_0000, 0xab12_3579];


const CLOCKS: usize = 0x4001_0000;
const CLK_REF_CTRL: *mut u32 = (CLOCKS + 0x30) as *mut u32;
const CLK_REF_DIV: *mut u32 = (CLOCKS + 0x34) as *mut u32;
const CLK_REF_SELECTED: *mut u32 = (CLOCKS + 0x38) as *mut u32;
const CLK_SYS_CTRL: *mut u32 = (CLOCKS + 0x3C) as *mut u32;
const CLK_SYS_DIV: *mut u32 = (CLOCKS + 0x40) as *mut u32;
const CLK_SYS_SELECTED: *mut u32 = (CLOCKS + 0x44) as *mut u32;
const CLK_PERI_CTRL: *mut u32 = (CLOCKS + 0x48) as *mut u32;

/// CLK_REF_CTRL: SRC [1:0] (00 the ring oscillator, 01 the auxiliary path,
/// 10 the crystal, 11 the low-power oscillator).
const CLK_REF_SRC: u32 = 0b11;
const CLK_REF_SRC_XOSC: u32 = 0b10;
/// CLK_REF_DIV: INT [23:16].
const CLK_REF_DIV_1: u32 = 1 << 16;
/// CLK_SYS_CTRL: SRC bit 0 (0 the reference clock, 1 the auxiliary path
/// the PLL posture takes -- its fields live in that posture's module).
const CLK_SYS_SRC: u32 = 1 << 0;
/// CLK_SYS_DIV: INT [31:16] (0 encodes the maximum + 1), FRAC [15:0].
const CLK_SYS_DIV_1: u32 = 1 << 16;
/// The SELECTED registers are one-hot over their SRC encoding.
const CLK_REF_SELECTED_XOSC: u32 = 1 << 2;
const CLK_SYS_SELECTED_REF: u32 = 1 << 0;
/// CLK_PERI_CTRL: ENABLED (read-only) bit 28, ENABLE bit 11, AUXSRC [7:5]
/// (100 the crystal).
const CLK_PERI_ENABLED: u32 = 1 << 28;
const CLK_PERI_ENABLE: u32 = 1 << 11;
const CLK_PERI_AUXSRC: u32 = 0b111 << 5;
const CLK_PERI_AUXSRC_XOSC: u32 = 0b100 << 5;

const XOSC: usize = 0x4004_8000;
const XOSC_CTRL: *mut u32 = XOSC as *mut u32;
const XOSC_STATUS: *mut u32 = (XOSC + 0x04) as *mut u32;
/// CTRL: ENABLE [23:12] takes the code 0xfab; FREQ_RANGE [11:0] takes
/// 0xaa0 for a 1-15 MHz crystal. STATUS: STABLE bit 31.
const XOSC_CTRL_ENABLE: u32 = 0xfab << 12;
const XOSC_CTRL_FREQ_RANGE_1_15MHZ: u32 = 0xaa0;
const XOSC_STATUS_STABLE: u32 = 1 << 31;

const RESETS: usize = 0x4002_0000;
const RESETS_RESET: *mut u32 = RESETS as *mut u32;
const RESETS_RESET_DONE: *mut u32 = (RESETS + 0x08) as *mut u32;
const RESET_IO_BANK0: u32 = 1 << 6;
const RESET_PADS_BANK0: u32 = 1 << 9;
#[cfg(any(feature = "pico-2-pll150", feature = "pico-plus-2-pll150"))]
const RESET_PLL_SYS: u32 = 1 << 14;
const RESET_SYSINFO: u32 = 1 << 21;
const RESET_UART0: u32 = 1 << 26;

const IO_BANK0: usize = 0x4002_8000;
const IO_BANK0_GPIO0_CTRL: *mut u32 = (IO_BANK0 + 0x04) as *mut u32;
const IO_BANK0_GPIO1_CTRL: *mut u32 = (IO_BANK0 + 0x0C) as *mut u32;
/// GPIOn_CTRL: FUNCSEL [4:0], 2 = UART0 on both pins (TX on GPIO 0, RX on
/// GPIO 1); every override field at its normal value.
const GPIO_CTRL_FUNCSEL_UART0: u32 = 2;

const PADS_BANK0: usize = 0x4003_8000;
const PADS_BANK0_GPIO0: *mut u32 = (PADS_BANK0 + 0x04) as *mut u32;
const PADS_BANK0_GPIO1: *mut u32 = (PADS_BANK0 + 0x08) as *mut u32;
/// Pad control: ISO bit 8 (the isolation latch, set at reset and released
/// once the pad is configured), OD bit 7, IE bit 6, PUE bit 3, PDE bit 2
/// (set at reset: every pad pulls down until told otherwise).
const PAD_ISO: u32 = 1 << 8;
const PAD_OD: u32 = 1 << 7;
const PAD_IE: u32 = 1 << 6;
const PAD_PUE: u32 = 1 << 3;
const PAD_PDE: u32 = 1 << 2;

const UART0: usize = 0x4007_0000;
const UART0_DR: *mut u32 = UART0 as *mut u32;
const UART0_RSR: *mut u32 = (UART0 + 0x04) as *mut u32;
const UART0_FR: *mut u32 = (UART0 + 0x18) as *mut u32;
const UART0_IBRD: *mut u32 = (UART0 + 0x24) as *mut u32;
const UART0_FBRD: *mut u32 = (UART0 + 0x28) as *mut u32;
const UART0_LCR_H: *mut u32 = (UART0 + 0x2C) as *mut u32;
const UART0_CR: *mut u32 = (UART0 + 0x30) as *mut u32;
const UART0_IMSC: *mut u32 = (UART0 + 0x38) as *mut u32;
const UART0_ICR: *mut u32 = (UART0 + 0x44) as *mut u32;
/// FR: TXFF bit 5 (the transmit FIFO full), RXFE bit 4 (the receive FIFO
/// empty) -- the FIFOs being on.
const FR_TXFF: u32 = 1 << 5;
const FR_RXFE: u32 = 1 << 4;
/// A received word carries the byte in [7:0] and the framing, parity,
/// break and overrun flags in [11:8].
const DR_ERRORS: u32 = 0xF << 8;
/// LCR_H: WLEN [6:5] = 11 for 8 data bits; FEN bit 4 turns the two
/// 32-entry FIFOs on (the flag register then reads their state).
const LCR_H_WLEN_8: u32 = 0b11 << 5;
const LCR_H_FEN: u32 = 1 << 4;
/// CR: UARTEN bit 0, TXE bit 8, RXE bit 9.
const CR_UARTEN: u32 = 1 << 0;
const CR_TXE: u32 = 1 << 8;
const CR_RXE: u32 = 1 << 9;
/// The transmit interrupt: bit 5 of the mask register (IMSC) and of the
/// clear register (ICR). It is raised when the transmit FIFO falls
/// through its trigger level (half full, the reset setting kept), never
/// by an empty FIFO the interrupt was enabled over -- so the writer
/// primes the FIFO itself and the interrupt only ever drains a backlog.
const UART_TX_INT: u32 = 1 << 5;
/// ICR's eleven clear bits (10:0): every interrupt cleared at init.
const ICR_ALL: u32 = 0x7FF;
const BAUD: u32 = 115_200;
/// The baud divisor in 64ths, rounded to nearest: the integer part into
/// IBRD, the six fraction bits into FBRD (12 MHz gives 6 + 33/64, which is
/// 115,108 baud, 0.08 percent low).
const BAUD_DIV_64: u32 = (PERI_HZ * 4 + BAUD / 2) / BAUD;
const IBRD: u32 = BAUD_DIV_64 >> 6;
const FBRD: u32 = BAUD_DIV_64 & 0x3F;
const _: () = assert!(IBRD >= 1 && IBRD <= 0xFFFF, "baud divisor outside the integer register");

/// UART0's interrupt line (the chip's interrupt table), its enable bit in
/// the interrupt controller's second set-enable and clear-pending
/// registers, and its priority byte in the priority array.
pub const UART0_IRQ: usize = 33;
const NVIC_ISER1: *mut u32 = 0xE000_E104 as *mut u32;
const NVIC_ICPR1: *mut u32 = 0xE000_E284 as *mut u32;
const NVIC_IPR: *mut u8 = 0xE000_E400 as *mut u8;
/// The lowest band: masked inside a kernel critical section (the ceiling
/// 0x40), below the tick (0x80), above the context switch (0xF0) -- the
/// drain preempts thread code and nothing else, and the writer's section
/// is its lock.
const UART0_IRQ_PRIO: u8 = 0xE0;

/// The interrupt lines this chip module claims with its own handlers,
/// folded into the vector table by the boot module: UART0's transmit
/// interrupt.
pub const BOARD_IRQS: &[(usize, unsafe extern "C" fn())] = &[(UART0_IRQ, __rhu_rp2350_uart0_irq)];

#[cfg(any(feature = "pico-2-pll150", feature = "pico-plus-2-pll150"))]
mod pll150 {
    /// CLK_SYS_CTRL's auxiliary path: SRC bit 0 = 1 selects it; AUXSRC
    /// [7:5] names its source (000 the system PLL, 011 the crystal); the
    /// SELECTED register reads 1 << 1 once the glitchless mux sits on it.
    pub const CLK_SYS_SRC_AUX: u32 = 1 << 0;
    pub const CLK_SYS_AUXSRC: u32 = 0b111 << 5;
    pub const CLK_SYS_AUXSRC_PLL_SYS: u32 = 0b000 << 5;
    pub const CLK_SYS_AUXSRC_XOSC: u32 = 0b011 << 5;
    pub const CLK_SYS_SELECTED_AUX: u32 = 1 << 1;

    const PLL_SYS: usize = 0x4005_0000;
    pub const PLL_SYS_CS: *mut u32 = PLL_SYS as *mut u32;
    pub const PLL_SYS_PWR: *mut u32 = (PLL_SYS + 0x04) as *mut u32;
    pub const PLL_SYS_FBDIV_INT: *mut u32 = (PLL_SYS + 0x08) as *mut u32;
    pub const PLL_SYS_PRIM: *mut u32 = (PLL_SYS + 0x0C) as *mut u32;
    /// CS: LOCK bit 31, BYPASS bit 8 (kept clear), REFDIV [5:0].
    pub const CS_LOCK: u32 = 1 << 31;
    pub const CS_REFDIV: u32 = 0x3F;
    /// PWR: PD bit 0, DSMPD bit 2 (the fractional path, unused: left
    /// powered down), POSTDIVPD bit 3, VCOPD bit 5 -- all set at reset.
    pub const PWR_PD: u32 = 1 << 0;
    pub const PWR_DSMPD: u32 = 1 << 2;
    pub const PWR_POSTDIVPD: u32 = 1 << 3;
    pub const PWR_VCOPD: u32 = 1 << 5;
    /// FBDIV_INT [11:0], 16..=320.
    pub const FBDIV_INT: u32 = 0xFFF;
    /// PRIM: POSTDIV1 [18:16], POSTDIV2 [14:12], each 1..=7.
    pub const PRIM_POSTDIV1_SHIFT: u32 = 16;
    pub const PRIM_POSTDIV2_SHIFT: u32 = 12;
    pub const PRIM_POSTDIV: u32 = 0b111;
    /// The recipe: 12 MHz / 1 into the VCO, x125 = 1500 MHz, / 5 / 2 = 150.
    pub const REFDIV: u32 = 1;
    pub const FBDIV: u32 = 125;
    pub const POSTDIV1: u32 = 5;
    pub const POSTDIV2: u32 = 2;
    /// The flash interface's window-0 timing register: CLKDIV [7:0] (0
    /// encodes 256) divides the system clock into the flash clock.
    pub const QMI_M0_TIMING: *mut u32 = 0x400d_000c as *mut u32;
    pub const QMI_TIMING_CLKDIV: u32 = 0xFF;
    /// 150 MHz / 4 = 37.5 MHz: under the 50 MHz the plain serial read
    /// command is rated for on these boards' flash parts, whichever read
    /// command the boot ROM settled on, and between the ROM's own two
    /// settings on these parts (the quad read at divisor 3, the plain read
    /// at divisor 6), with the ROM's receive delay kept. The ROM's choice
    /// follows the flash part's state at its scan and differs between a
    /// power-up and a warm reset, so the posture lands this one value
    /// rather than bounding whatever it finds: the clock line reads the
    /// same number on every boot.
    pub const QMI_CLKDIV_PLL150: u32 = 4;
    /// The cached flash window (the dummy read after the divisor write).
    pub const XIP_BASE: *const u32 = 0x1000_0000 as *const u32;
}

/// Release peripherals from the reset controller and wait for each.
unsafe fn release_reset(mask: u32) {
    unsafe {
        write_volatile(RESETS_RESET, read_volatile(RESETS_RESET) & !mask);
        while read_volatile(RESETS_RESET_DONE) & mask != mask {}
    }
}

/// The crystal on and stable, the reference clock on it at divider 1, the
/// system clock on the reference clock at divider 1 (each glitchless
/// switch polled on its SELECTED register), the peripheral clock sourced
/// from the crystal and started: the posture-1 shape every posture passes
/// through. Entry assumes nothing -- a resident image's PLL posture is
/// left behind here, before any PLL register is touched.
unsafe fn clocks_to_xosc() {
    unsafe {
        write_volatile(XOSC_CTRL, XOSC_CTRL_ENABLE | XOSC_CTRL_FREQ_RANGE_1_15MHZ);
        while read_volatile(XOSC_STATUS) & XOSC_STATUS_STABLE == 0 {}

        write_volatile(CLK_REF_DIV, CLK_REF_DIV_1);
        let r = read_volatile(CLK_REF_CTRL) & !CLK_REF_SRC;
        write_volatile(CLK_REF_CTRL, r | CLK_REF_SRC_XOSC);
        while read_volatile(CLK_REF_SELECTED) != CLK_REF_SELECTED_XOSC {}

        let s = read_volatile(CLK_SYS_CTRL) & !CLK_SYS_SRC;
        write_volatile(CLK_SYS_CTRL, s);
        while read_volatile(CLK_SYS_SELECTED) != CLK_SYS_SELECTED_REF {}
        write_volatile(CLK_SYS_DIV, CLK_SYS_DIV_1);

        let p = read_volatile(CLK_PERI_CTRL) & !(CLK_PERI_ENABLE | CLK_PERI_AUXSRC);
        write_volatile(CLK_PERI_CTRL, p);
        write_volatile(CLK_PERI_CTRL, p | CLK_PERI_AUXSRC_XOSC);
        write_volatile(CLK_PERI_CTRL, p | CLK_PERI_AUXSRC_XOSC | CLK_PERI_ENABLE);
        while read_volatile(CLK_PERI_CTRL) & CLK_PERI_ENABLED == 0 {}
    }
}

/// The boot path's tail shared by both postures: the identity block
/// released from reset (so a debug probe reads the chip's identity
/// registers while the image runs), the console mirror armed, and the
/// console init that follows marked as the boot's.
fn clock_init_tail() {
    unsafe {
        release_reset(RESET_SYSINFO);
    }
    ring::boot_mark();
}

/// Enforce the crystal posture, then the shared tail.
#[cfg(not(any(feature = "pico-2-pll150", feature = "pico-plus-2-pll150")))]
pub fn clock_init() {
    unsafe {
        clocks_to_xosc();
    }
    clock_init_tail();
}

/// Boring posture: no clock line in the banner (the banked transcript
/// surface stands byte-identical).
#[cfg(not(any(feature = "pico-2-pll150", feature = "pico-plus-2-pll150")))]
pub fn clock_readback() -> Option<crate::board::ClockReadback> {
    None
}

/// Enforce the PLL150 posture, every step on its hardware ready flag.
/// Entry assumes nothing: first the crystal posture (the safe downshift --
/// the system clock on the crystal before the PLL is touched), then the
/// PLL released from reset, configured while powered down (its reset
/// state), powered up and locked, its post-dividers set and powered; the
/// flash clock divisor LANDED at its one value BEFORE the system clock
/// rises, with a read from the flash window and barriers after the write
/// so the new divisor is in effect first (the datasheet's own sequence;
/// the ROM's read command and receive delay are kept); then the system
/// clock's auxiliary source set to the
/// PLL while the glitchless mux still sits on the reference clock (the
/// auxiliary mux glitches, the glitchless one does not), and the mux
/// switched and polled. The peripheral clock stays on the crystal.
#[cfg(any(feature = "pico-2-pll150", feature = "pico-plus-2-pll150"))]
pub fn clock_init() {
    use pll150::*;
    unsafe {
        clocks_to_xosc();
        release_reset(RESET_PLL_SYS);

        write_volatile(PLL_SYS_PWR, PWR_PD | PWR_DSMPD | PWR_POSTDIVPD | PWR_VCOPD);
        write_volatile(PLL_SYS_CS, REFDIV);
        write_volatile(PLL_SYS_FBDIV_INT, FBDIV);
        write_volatile(PLL_SYS_PWR, PWR_DSMPD | PWR_POSTDIVPD);
        while read_volatile(PLL_SYS_CS) & CS_LOCK == 0 {}
        write_volatile(
            PLL_SYS_PRIM,
            (POSTDIV1 << PRIM_POSTDIV1_SHIFT) | (POSTDIV2 << PRIM_POSTDIV2_SHIFT),
        );
        write_volatile(PLL_SYS_PWR, PWR_DSMPD);

        let timing = read_volatile(QMI_M0_TIMING);
        if timing & QMI_TIMING_CLKDIV != QMI_CLKDIV_PLL150 {
            write_volatile(QMI_M0_TIMING, (timing & !QMI_TIMING_CLKDIV) | QMI_CLKDIV_PLL150);
            let _ = read_volatile(XIP_BASE);
            core::arch::asm!("dsb sy", "isb sy", options(nostack));
        }

        let s = read_volatile(CLK_SYS_CTRL) & !CLK_SYS_AUXSRC;
        write_volatile(CLK_SYS_CTRL, s | CLK_SYS_AUXSRC_PLL_SYS);
        write_volatile(CLK_SYS_CTRL, read_volatile(CLK_SYS_CTRL) | CLK_SYS_SRC_AUX);
        while read_volatile(CLK_SYS_SELECTED) != CLK_SYS_SELECTED_AUX {}
    }
    clock_init_tail();
}

/// Gated posture: report the live clock tree for the banner line, every
/// numeric field decoded from registers (a silently-failed switch reads
/// the reference clock's 12 MHz here, and the kmain assert dies loudly).
/// Field mapping under the shared banner wording: `pllm` carries REFDIV,
/// `plln` FBDIV, `pllp` the product of the two post-dividers (the divider
/// that lands SYSCLK on this chip), `src` the crystal (the PLL's reference
/// by construction), `flash_ws` the flash interface's clock divisor (this
/// chip's flash-speed knob: it has no flash wait states), and `overdrive`
/// reads off (no voltage lever is pulled for 150 MHz).
#[cfg(any(feature = "pico-2-pll150", feature = "pico-plus-2-pll150"))]
pub fn clock_readback() -> Option<crate::board::ClockReadback> {
    use pll150::*;
    unsafe {
        let selected = read_volatile(CLK_SYS_SELECTED);
        let ctrl = read_volatile(CLK_SYS_CTRL);
        let cs = read_volatile(PLL_SYS_CS);
        let fbdiv = read_volatile(PLL_SYS_FBDIV_INT) & FBDIV_INT;
        let prim = read_volatile(PLL_SYS_PRIM);
        let refdiv = cs & CS_REFDIV;
        let postdiv = ((prim >> PRIM_POSTDIV1_SHIFT) & PRIM_POSTDIV)
            * ((prim >> PRIM_POSTDIV2_SHIFT) & PRIM_POSTDIV);
        let pll_hz = if refdiv == 0 || postdiv == 0 {
            0
        } else {
            XOSC_HZ / refdiv * fbdiv / postdiv
        };
        let ref_hz = if read_volatile(CLK_REF_SELECTED) == CLK_REF_SELECTED_XOSC {
            XOSC_HZ
        } else {
            0
        };
        let (sws, sysclk_hz) = if selected == CLK_SYS_SELECTED_AUX {
            match ctrl & CLK_SYS_AUXSRC {
                CLK_SYS_AUXSRC_PLL_SYS => ("PLL_SYS", pll_hz),
                CLK_SYS_AUXSRC_XOSC => ("XOSC", XOSC_HZ),
                _ => ("aux", 0),
            }
        } else {
            ("clk_ref", ref_hz)
        };
        let clkdiv = read_volatile(QMI_M0_TIMING) & QMI_TIMING_CLKDIV;
        Some(crate::board::ClockReadback {
            posture: "PLL150",
            sysclk_hz,
            sws,
            src: "XOSC",
            pllm: refdiv,
            plln: fbdiv,
            pllp: postdiv,
            flash_ws: if clkdiv == 0 { 256 } else { clkdiv },
            overdrive: false,
        })
    }
}

/// Route GPIO 0 and 1 to UART0 (function 2 on both), configure the two
/// pads and release their isolation latches, and program 115200 8N1 from
/// the crystal-sourced peripheral clock with the FIFOs on. The pads:
/// drive and slew as reset (4 mA, slow), input enabled on both (the
/// receiver needs it, the transmitter ignores it), output enabled, the
/// transmit pin without pulls, and the receive pin pulled UP -- the reset
/// state pulls every pad down, and a pulled-down receive line reads as a
/// permanent break, a floating one as noise; pulled up, an unwired line
/// idles high. Runs once, at the first console init (a panic before the
/// boot's own init reaches it through the same path).
unsafe fn console_hw_init() {
    unsafe {
        release_reset(RESET_IO_BANK0 | RESET_PADS_BANK0 | RESET_UART0);

        write_volatile(IO_BANK0_GPIO0_CTRL, GPIO_CTRL_FUNCSEL_UART0);
        write_volatile(IO_BANK0_GPIO1_CTRL, GPIO_CTRL_FUNCSEL_UART0);

        let pad0 = (read_volatile(PADS_BANK0_GPIO0) & !(PAD_OD | PAD_PUE | PAD_PDE)) | PAD_IE;
        write_volatile(PADS_BANK0_GPIO0, pad0 | PAD_ISO);
        write_volatile(PADS_BANK0_GPIO0, pad0 & !PAD_ISO);
        let pad1 = (read_volatile(PADS_BANK0_GPIO1) & !(PAD_OD | PAD_PDE)) | PAD_IE | PAD_PUE;
        write_volatile(PADS_BANK0_GPIO1, pad1 | PAD_ISO);
        write_volatile(PADS_BANK0_GPIO1, pad1 & !PAD_ISO);

        write_volatile(UART0_CR, 0);
        write_volatile(UART0_IBRD, IBRD);
        write_volatile(UART0_FBRD, FBRD);
        write_volatile(UART0_LCR_H, LCR_H_WLEN_8 | LCR_H_FEN);
        write_volatile(UART0_IMSC, 0);
        write_volatile(UART0_ICR, ICR_ALL);
        write_volatile(UART0_RSR, 0);
        write_volatile(UART0_CR, CR_UARTEN | CR_TXE | CR_RXE);
    }
}

/// The console driver's init. The first call configures the hardware.
/// The call the boot path makes (marked by `clock_init` through the ring
/// module) arms the transmit interrupt -- its priority byte, its enable
/// bit, the UART's mask -- and takes the interrupt mode; any later call
/// is the panic or fault path's re-init and switches the driver to the
/// polled mode with the interrupt masked, so the report that follows
/// goes through the wire synchronously. The console mirror is not
/// touched here (see `clock_init`), so a panic transcript appends to the
/// boot's.
pub fn console_init() {
    unsafe {
        if ring::mode() == 0 {
            console_hw_init();
        }
        if ring::take_boot_mark() {
            write_volatile(NVIC_IPR.add(UART0_IRQ), UART0_IRQ_PRIO);
            write_volatile(NVIC_ICPR1, 1 << (UART0_IRQ - 32));
            write_volatile(NVIC_ISER1, 1 << (UART0_IRQ - 32));
            write_volatile(UART0_IMSC, UART_TX_INT);
            ring::set_mode(ring::MODE_IRQ);
        } else {
            write_volatile(UART0_IMSC, 0);
            ring::set_mode(ring::MODE_POLLED);
        }
    }
}

/// The ring's four hooks on this UART: the transmit FIFO takes a byte
/// while its full flag is clear; a byte goes into the data register; the
/// transmit interrupt is latched and cleared through the interrupt-clear
/// register; and the interrupt is transition-shaped (it fires when the
/// FIFO falls through its trigger level), so its mask stays set for the
/// driver's life and the level-shaped enable is a no-op here.
pub unsafe fn ring_tx_room() -> bool {
    unsafe { read_volatile(UART0_FR) & FR_TXFF == 0 }
}
pub unsafe fn ring_tx_put(b: u8) {
    unsafe { write_volatile(UART0_DR, b as u32) }
}
pub unsafe fn ring_tx_ack() {
    unsafe { write_volatile(UART0_ICR, UART_TX_INT) }
}
pub unsafe fn ring_tx_irq(_enabled: bool) {}

/// UART0's transmit interrupt: the ring's drain in the interrupt mode.
/// Raised when the transmit FIFO falls through its trigger level; the
/// interrupt is cleared first, then the FIFO refilled from the ring, so
/// a later fall through the level raises it again while a backlog
/// remains. Runs at the lowest priority band: a kernel critical section
/// (the writer's lock) holds it off, and it holds off only the context
/// switch; the FIFO's remaining half keeps the wire busy meanwhile.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __rhu_rp2350_uart0_irq() {
    unsafe { ring::irq() }
}

/// Byte transmit through the shared ring (the mirror, the primed FIFO,
/// the interrupt drain; the polled drain on the panic and fault paths).
/// The caller holds the kernel critical section (the console's line
/// lock), which masks the interrupt.
pub fn console_putb(b: u8) {
    ring::putb(b)
}

/// The room the ring has for a writer, in bytes (the door's back-pressure
/// reads it outside the critical section).
pub fn console_room() -> usize {
    ring::room()
}

/// Non-blocking byte receive: one pending byte, or None immediately. A
/// byte received with an error flag (framing, parity, break, overrun) is
/// dropped and the receive status cleared, so the receiver never wedges;
/// the lost byte is not recovered.
pub fn console_getb() -> Option<u8> {
    unsafe {
        if read_volatile(UART0_FR) & FR_RXFE != 0 {
            return None;
        }
        let word = read_volatile(UART0_DR);
        if word & DR_ERRORS != 0 {
            write_volatile(UART0_RSR, 0);
            return None;
        }
        Some(word as u8)
    }
}

/// Whether address 0 is readable memory on this row -- the low-address
/// question's per-row record, MEASURED on the part (a read that returns
/// a word, or a bus fault): the api flavor's observation reads it before
/// it touches the address.
/// This row: the boot ROM at 0.
#[allow(dead_code)]
pub const ADDRESS_ZERO_READABLE: bool = true;

/// The entropy posture of this row: the chip carries a TRNG block that no driver
/// serves on this row, so no source serves.
pub const ENTROPY: super::EntropyPosture = super::EntropyPosture::None;
/// Whether this row carries a real-time clock the kernel reads at boot and
/// writes at an explicit set.
#[allow(dead_code)]
pub const RTC_PRESENT: bool = false;
/// The calendar banner's text for this row, constant per image.
pub const RTC_LINE: &str = "no calendar RTC on this chip (the always-on timer counts milliseconds); the base is the epoch until the tooling or the application sets it";
pub use super::absent::{entropy_draw, entropy_init, rtc_read_at_boot, rtc_write};
