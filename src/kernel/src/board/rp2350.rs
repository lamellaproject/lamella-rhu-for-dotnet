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
#[cfg(not(rp2350_pll150))]
pub const SYSCLK_HZ: u32 = 12_000_000;
/// Clock posture 2 (`<board>-pll150`): the system PLL at 150 MHz off the
/// crystal -- 12 MHz / 1 into the VCO, x125 = 1500 MHz (inside the
/// 400-1600 MHz window the PLL's control register states), / 5 / 2 lands
/// SYSCLK at 150 MHz, the chip's rated system clock.
#[cfg(rp2350_pll150)]
pub const SYSCLK_HZ: u32 = 150_000_000;

/// The peripheral clock: the crystal, in both postures (stated by
/// `clock_init`, which selects and starts it).
const PERI_HZ: u32 = XOSC_HZ;

/// SysTick rate. The kernel clocks SysTick from the processor clock, so
/// the chip's tick-generator block is not needed.
pub const TICK_HZ: u32 = 1_000;

/// Thread stack pool (lives in .kstacks): 96 KiB on these rows' 520 K of
/// SRAM -- the idle thread's 1 KiB, the hosting thread's and the finalizer's
/// 16 KiB each, and three default 16 KiB workers; the link asserts the arena
/// window that remains above the row's floor.
pub const STACK_POOL_BYTES: usize = 96 * 1024;

/// Stack for the kmain thread (the boot thread's continuation).
pub const KMAIN_STACK_BYTES: usize = 16 * 1024;

/// External IRQ lines: 52 (positions 0..=51; the last six are spare lines
/// wired to nothing, and a line that never fires costs eight bytes of
/// flash), so every position the datasheet names has a funnel entry.
pub const NIRQ: usize = 52;

/// The vector line a test image may claim: the last of the spare lines
/// wired to nothing, implemented by the controller (a software pend takes
/// it) and claimed by no kernel handler.
#[allow(dead_code)]
pub const SPARE_IRQ: usize = NIRQ - 1;

/// FPU shape: FPv5 SINGLE precision (the Cortex-M33 FPU) -- kernel f64
/// stays on the soft-float path. Consumed by records and gates, never by
/// context-switch code (lazy stacking is FP-shape-agnostic).
/// The part: the RP2350's Cortex-M33 cores with the standard Arm
/// single-precision FPU (the RP2350 datasheet, the Cortex-M33 section; its
/// double-precision coprocessor is a separate coprocessor reached by
/// coprocessor instructions, not the VFP double forms).
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
#[cfg(rp2350_pll150)]
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

/// The family GPIO driver's handler on the bank's one interrupt line (the
/// device seam: the driver owns the line; a hook is a tenant of it).
pub const GPIO_IRQS: &[(usize, unsafe extern "C" fn())] = &[(gpio_backend::IO_IRQ_BANK0, gpio_backend::__rhu_rp2350_io_bank0_irq)];

#[cfg(rp2350_pll150)]
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
#[cfg(not(rp2350_pll150))]
pub fn clock_init() {
    unsafe {
        clocks_to_xosc();
    }
    clock_init_tail();
}

/// Boring posture: no clock line in the banner (the banked transcript
/// surface stands byte-identical).
#[cfg(not(rp2350_pll150))]
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
#[cfg(rp2350_pll150)]
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
/// chip's flash-speed knob: it has no flash wait states), and `boost` reads
/// "over-drive off" (no voltage lever is pulled for 150 MHz).
#[cfg(rp2350_pll150)]
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
            boost: "over-drive off",
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

/// The entropy posture of this row: the chip's true random number generator
/// (the datasheet's TRNG chapter) served on both clock postures -- its ring
/// oscillator is free-running and sampled every 24 system clocks, its three
/// entropy checks are on and read at every draw. The driver sits after the
/// GPIO backend below.
pub const ENTROPY: super::EntropyPosture = super::EntropyPosture::Source {
    id: 3,
    name: "RP2350 TRNG",
    line: "hardware source RP2350 TRNG enabled on clk_sys; its self-tests gate every draw",
};
/// Whether this row carries a real-time clock the kernel reads at boot and
/// writes at an explicit set.
#[allow(dead_code)]
pub const RTC_PRESENT: bool = false;
/// The calendar banner's text for this row, constant per image.
pub const RTC_LINE: &str = "no calendar RTC on this chip (the always-on timer counts milliseconds); the base is the epoch until the tooling or the application sets it";
pub use super::absent::{rtc_read_at_boot, rtc_write};

/// The RP2350's GPIO half and THE FAMILY HANDLER that owns the bank's one
/// interrupt line (the device seam). The
/// blocks (the RP2350 datasheet): IO_BANK0 at 0x4002_8000 -- per line the
/// control register at 8n + 4 whose FUNCSEL (bits 4:0) selects the
/// function (5 the SIO for a program's line; 0x1F none); PADS_BANK0 at
/// 0x4003_8000 -- per line at 0x04 + 4n: ISO bit 8 (the isolation latch,
/// set out of reset and cleared by a claim), OD bit 7, IE bit 6 (set by a
/// claim), PUE bit 3, PDE bit 2 (set out of reset; the claim's pull
/// overrides both); the SIO at 0xD000_0000 -- GPIO_IN 0x004, GPIO_OUT_SET
/// 0x018, GPIO_OUT_CLR 0x020, GPIO_OE_SET 0x038, GPIO_OE_CLR 0x040 (the
/// atomic paths), each with its twin at +4 for lines 32 to 47 of the
/// RP2350B; the interrupt arrays in IO_BANK0, eight lines per word and four
/// bits per line (LEVEL_LOW, LEVEL_HIGH, EDGE_LOW, EDGE_HIGH; the edge bits
/// latched and cleared by writing 1 to INTR0..5 at 0x230; PROC0_INTE0..5 at
/// 0x248, PROC0_INTF0..5 at 0x260, PROC0_INTS0..5 at 0x278), on the
/// IO_IRQ_BANK0 vector, position 21. The reset controller's IO_BANK0 and
/// PADS_BANK0 bits are released here as the console's init releases them
/// (idempotent).
///
/// THE OWNERSHIP: this module owns the one handler; a hook is a tenant
/// that claims its enable bit by register and bit and names its wake word
/// in the claims table; the handler reads the status words and dispatches
/// every set bit by ownership -- a tenant's bit to the tenant's word (an
/// edge or a force acknowledged; a LEVEL bit masked before the wake, since
/// a level is not latched and would re-enter this handler until the
/// tenant's thread silenced the source, and re-armed by the tenant after
/// its service pass), a program's armed edge to the line's event word with
/// the edge bit cleared.
pub mod gpio_backend {
    use core::ptr::{addr_of, read_volatile, write_volatile};
    use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};

    use super::{
        IO_BANK0, PADS_BANK0, PAD_IE, PAD_ISO, PAD_OD, PAD_PDE, PAD_PUE, RESETS_RESET, RESETS_RESET_DONE, RESET_IO_BANK0,
        RESET_PADS_BANK0,
    };
    use crate::claims;
    use crate::critical::with_cs;
    use crate::gpio;
    use crate::gpio_math as m;
    use crate::sched;

    pub const HAS_EVENTS: bool = true;

    const FUNCSEL_SIO: u32 = 5;
    const FUNCSEL_NULL: u32 = 0x1F;
    const SIO: usize = 0xD000_0000;
    const GPIO_IN: usize = 0x004;
    const GPIO_OUT_SET: usize = 0x018;
    const GPIO_OUT_CLR: usize = 0x020;
    const GPIO_OE_SET: usize = 0x038;
    const GPIO_OE_CLR: usize = 0x040;
    /// The twin registers for lines 32 to 47 sit four bytes above each.
    const HI: usize = 0x004;
    const INTR0: usize = 0x230;
    const PROC0_INTE0: usize = 0x248;
    const PROC0_INTF0: usize = 0x260;
    const PROC0_INTS0: usize = 0x278;
    const WORDS: usize = 6;
    pub const IO_IRQ_BANK0: usize = 21;
    const NVIC_ISER0: *mut u32 = 0xE000_E100 as *mut u32;
    const NVIC_IPR: *mut u8 = 0xE000_E400 as *mut u8;
    const IRQ_PRIO: u8 = 0xE0;

    fn ctrl(line: u32) -> *mut u32 {
        (IO_BANK0 + 8 * line as usize + 4) as *mut u32
    }

    fn pad(line: u32) -> *mut u32 {
        (PADS_BANK0 + 4 + 4 * line as usize) as *mut u32
    }

    fn sio(off: usize, line: u32) -> (*mut u32, u32) {
        let twin = if line >= 32 { HI } else { 0 };
        ((SIO + off + twin) as *mut u32, 1 << (line % 32))
    }

    fn intr(k: usize) -> *mut u32 {
        (IO_BANK0 + INTR0 + 4 * k) as *mut u32
    }

    pub(super) fn inte(k: usize) -> *mut u32 {
        (IO_BANK0 + PROC0_INTE0 + 4 * k) as *mut u32
    }

    fn intf(k: usize) -> *mut u32 {
        (IO_BANK0 + PROC0_INTF0 + 4 * k) as *mut u32
    }

    fn ints(k: usize) -> *mut u32 {
        (IO_BANK0 + PROC0_INTS0 + 4 * k) as *mut u32
    }

    fn release_banks() {
        unsafe {
            let mask = RESET_IO_BANK0 | RESET_PADS_BANK0;
            write_volatile(RESETS_RESET, read_volatile(RESETS_RESET) & !mask);
            while read_volatile(RESETS_RESET_DONE) & mask != mask {}
        }
    }

    /// The pad's pull and input enable, the function select, then the
    /// output's initial LEVEL (SIO OUT) before its direction (SIO OE), so
    /// the pad never shows the wrong level at open. An open-drain output
    /// at a high initial level is the pad released.
    pub fn configure(line: u32, mode: u32, pull: u32, initial: bool) {
        release_banks();
        with_cs(|| unsafe {
            let p = pad(line);
            let mut v = read_volatile(p) & !(PAD_PUE | PAD_PDE | PAD_OD | PAD_ISO);
            v |= PAD_IE;
            if pull == m::PULL_UP {
                v |= PAD_PUE;
            } else if pull == m::PULL_DOWN {
                v |= PAD_PDE;
            }
            write_volatile(p, v);
            write_volatile(ctrl(line), FUNCSEL_SIO);
            let (oe_set, mask) = sio(GPIO_OE_SET, line);
            let (oe_clr, _) = sio(GPIO_OE_CLR, line);
            let (out_set, _) = sio(GPIO_OUT_SET, line);
            let (out_clr, _) = sio(GPIO_OUT_CLR, line);
            write_volatile(if initial { out_set } else { out_clr }, mask);
            let drive = mode == m::MODE_OUTPUT || (mode == m::MODE_OUTPUT_OPEN_DRAIN && !initial);
            write_volatile(if drive { oe_set } else { oe_clr }, mask);
        });
    }

    /// Back to a released, function-less pad with its input enabled.
    pub fn release(line: u32) {
        with_cs(|| unsafe {
            let (oe_clr, mask) = sio(GPIO_OE_CLR, line);
            write_volatile(oe_clr, mask);
            write_volatile(ctrl(line), FUNCSEL_NULL);
            let p = pad(line);
            write_volatile(p, read_volatile(p) & !(PAD_PUE | PAD_PDE));
        });
    }

    /// The atomic paths. An open-drain output has no native form on this
    /// block: "high" releases the pad (the output disabled), "low" drives
    /// it low (the output enabled).
    pub fn write(line: u32, mode: u32, high: bool) {
        unsafe {
            if mode == m::MODE_OUTPUT_OPEN_DRAIN {
                if high {
                    let (oe_clr, mask) = sio(GPIO_OE_CLR, line);
                    write_volatile(oe_clr, mask);
                } else {
                    let (out_clr, mask) = sio(GPIO_OUT_CLR, line);
                    write_volatile(out_clr, mask);
                    let (oe_set, _) = sio(GPIO_OE_SET, line);
                    write_volatile(oe_set, mask);
                }
            } else {
                let (r, mask) = sio(if high { GPIO_OUT_SET } else { GPIO_OUT_CLR }, line);
                write_volatile(r, mask);
            }
        }
    }

    pub fn read(line: u32) -> bool {
        let (r, mask) = sio(GPIO_IN, line);
        unsafe { read_volatile(r) & mask != 0 }
    }

    /// The bank's vector line enabled at its priority ONCE per boot: the
    /// first arm or tenancy writes, every later call sees the flag and
    /// writes nothing; nothing disables the line afterwards.
    static NVIC_ENABLED: AtomicBool = AtomicBool::new(false);
    pub(super) fn nvic_enable_once() {
        if NVIC_ENABLED.swap(true, Ordering::AcqRel) {
            return;
        }
        unsafe {
            write_volatile(NVIC_IPR.add(IO_IRQ_BANK0), IRQ_PRIO);
            write_volatile(NVIC_ISER0, 1 << IO_IRQ_BANK0);
        }
    }

    pub fn arm(line: u32, rising: bool, falling: bool) -> i32 {
        let (k, s) = m::rp_word(line);
        let bits = ((rising as u32) << (s + m::RP_EDGE_HIGH)) | ((falling as u32) << (s + m::RP_EDGE_LOW));
        with_cs(|| unsafe {
            write_volatile(intr(k), 0b1100 << s);
            write_volatile(inte(k), (read_volatile(inte(k)) & !(0b1111 << s)) | bits);
        });
        nvic_enable_once();
        0
    }

    pub fn disarm(line: u32) {
        let (k, s) = m::rp_word(line);
        with_cs(|| unsafe {
            write_volatile(inte(k), read_volatile(inte(k)) & !(0b1111 << s));
            write_volatile(intr(k), 0b1100 << s);
        });
    }

    /// The software trigger: the force register asserts the line's
    /// rising-edge bit in the status word; the handler clears the force
    /// after the wake, so one force is one event.
    pub fn software_edge(line: u32) -> bool {
        let (k, s) = m::rp_word(line);
        with_cs(|| unsafe {
            write_volatile(intf(k), read_volatile(intf(k)) | (1 << (s + m::RP_EDGE_HIGH)));
        });
        true
    }

    /// The family handler on IO_IRQ_BANK0: bounded work in the board band.
    pub unsafe extern "C" fn __rhu_rp2350_io_bank0_irq() {
        unsafe {
            for k in 0..WORDS {
                let status = read_volatile(ints(k));
                if status == 0 {
                    continue;
                }
                let forced = read_volatile(intf(k));
                let mut p = status;
                while p != 0 {
                    let b = p.trailing_zeros();
                    p &= p - 1;
                    let line = (k as u32) * 8 + b / 4;
                    let kind = b % 4;
                    let acknowledge = |k: usize, b: u32| {
                        if cfg!(feature = "test-gpio-doctored-mask") {
                            return;
                        }
                        if forced & (1 << b) != 0 {
                            write_volatile(intf(k), read_volatile(intf(k)) & !(1 << b));
                        } else if kind >= m::RP_EDGE_LOW {
                            write_volatile(intr(k), 1 << b);
                        }
                    };
                    if let Some(t) = claims::tenant_for(&*addr_of!(gpio::CLAIMS), inte(k) as u32, b) {
                        if forced & (1 << b) == 0 && kind < m::RP_EDGE_LOW {
                            write_volatile(inte(k), read_volatile(inte(k)) & !(1 << b));
                        } else {
                            acknowledge(k, b);
                        }
                        AtomicU32::from_ptr(t.wake as *mut u32).fetch_add(1, Ordering::AcqRel);
                        sched::futex_wake(t.wake as *const u32, u32::MAX);
                        continue;
                    }
                    if forced & (1 << b) == 0 && kind < m::RP_EDGE_LOW {
                        write_volatile(inte(k), read_volatile(inte(k)) & !(1 << b));
                        continue;
                    }
                    acknowledge(k, b);
                    gpio::fire(line, kind == m::RP_EDGE_HIGH || kind == m::RP_LEVEL_HIGH);
                }
            }
        }
    }
}

/// THE HARDWARE ENTROPY SOURCE: the chip's true random number generator (the
/// datasheet's TRNG chapter; the registers from its list, at 0x400F_0000 +
/// 0x100 on). RNG_IMR masks the four interrupts (all masked: this driver
/// polls). RNG_ISR reports EHR_VALID bit 0 (192 bits collected), AUTOCORR_ERR
/// bit 1 (the autocorrelation test failed four times in a row -- the block
/// ceases functioning until its reset, and the bit clears only by that
/// reset), CRNGT_ERR bit 2 (two consecutive blocks of 16 collected bits
/// equal), VN_ERR bit 3 (32 consecutive collected bits identical). RNG_ICR
/// clears a status bit by a 1 written. TRNG_CONFIG selects the oscillator's
/// inverter-chain length (bits 1:0). The six EHR_DATA words hold the
/// collected bits, the read of the last clearing all six. RND_SOURCE_ENABLE
/// bit 0 starts and stops the source. SAMPLE_CNT1 counts the system clocks
/// between two oscillator samples. TRNG_DEBUG_CONTROL's three bypass bits
/// switch the checks off (written 0 here: the checks are the health flags
/// the policy asks for). TRNG_SW_RESET resets the block by a 1 written.
/// RST_BITS_COUNTER resets the bits counter and the valid registers by any
/// write while the source is disabled.
const TRNG: usize = 0x400F_0000;
const RNG_IMR: *mut u32 = (TRNG + 0x100) as *mut u32;
const RNG_ISR: *mut u32 = (TRNG + 0x104) as *mut u32;
const RNG_ICR: *mut u32 = (TRNG + 0x108) as *mut u32;
const TRNG_CONFIG: *mut u32 = (TRNG + 0x10C) as *mut u32;
const EHR_DATA0: *mut u32 = (TRNG + 0x114) as *mut u32;
const RND_SOURCE_ENABLE: *mut u32 = (TRNG + 0x12C) as *mut u32;
const SAMPLE_CNT1: *mut u32 = (TRNG + 0x130) as *mut u32;
const TRNG_DEBUG_CONTROL: *mut u32 = (TRNG + 0x138) as *mut u32;
const TRNG_SW_RESET: *mut u32 = (TRNG + 0x140) as *mut u32;
const RST_BITS_COUNTER: *mut u32 = (TRNG + 0x1BC) as *mut u32;
const ISR_EHR_VALID: u32 = 1 << 0;
const ISR_AUTOCORR_ERR: u32 = 1 << 1;
const ISR_CRNGT_ERR: u32 = 1 << 2;
const ISR_VN_ERR: u32 = 1 << 3;
const ISR_FAILED: u32 = ISR_AUTOCORR_ERR | ISR_CRNGT_ERR | ISR_VN_ERR;
const ISR_ALL: u32 = ISR_EHR_VALID | ISR_FAILED;
const RND_SRC_EN: u32 = 1 << 0;
/// The configuration: the shortest inverter chain, the three checks on, and
/// two microseconds of the posture's clock between samples (the clock over
/// 500 kHz: 300 at 150 MHz, 24 at 12 MHz). The datasheet's guidance for an
/// average generation time of about two milliseconds, a chain of 0 or 1 and
/// a count of 20 to 25, is given at its example's clock of about 12 MHz; the
/// same count at a faster clock fails the autocorrelation test at once.
const TRNG_CONFIG_VALUE: u32 = 0;
const SAMPLE_CNT1_VALUE: u32 = SYSCLK_HZ / 500_000;
/// The reset controller's TRNG bit (the RESET register's bit 25) and the
/// block's clock gate (CLOCKS.WAKE_EN0 bit 21, set at reset and never
/// written here).
const RESET_TRNG: u32 = 1 << 25;
const CLOCKS_WAKE_EN0: *mut u32 = (CLOCKS + 0xAC) as *mut u32;
const WAKE_EN0_CLK_SYS_TRNG: u32 = 1 << 21;
/// One block = 192 collected bits = six words = 24 bytes.
const EHR_WORDS: usize = 6;
const BLOCK_BYTES: usize = EHR_WORDS * 4;
/// The bound per block, in loop iterations: the cycles of one second over
/// four (the loop body is at least four cycles; with its bus read it is
/// more, so the bound is at least one second at either posture, never less).
/// With the sampling period the same wall time on either posture, the
/// average block is the datasheet's two milliseconds on both, so the bound
/// is about five hundred averages and the datasheet's occasional long
/// generation ("in excess of 100 times the average") completes inside it
/// at either posture. A miss is the door's "try again", counted, never a
/// retirement on its own; three in a row retire the source. The spin runs
/// in the caller's thread outside any critical section: the tick and every
/// interrupt run as always, and only the calling thread waits for the block
/// it asked for.
const BLOCK_BOUND: u32 = SYSCLK_HZ / 4;
/// A short bound for the two reset handshakes (at least a hundred
/// microseconds; each completes within a few clocks).
const SPIN_SHORT: u32 = SYSCLK_HZ / 10_000 / 4;
/// Runs the block discards by its own checks inside ONE block before the
/// source is called faulty: the datasheet documents a discarded run as
/// normal operation ("occasionally"); nine in a row is a fault.
const DISCARDS_PER_BLOCK_MAX: u32 = 8;

/// The chip's own record of the block's behaviour, exported by symbol for a
/// debug probe (five words; never a gated line): the blocks generated; the
/// runs discarded by the von Neumann check and by the continuous check --
/// the block's documented normal operation, counted HERE and never as the
/// entropy witness's health flags (which count this source's faults: nine
/// discards in one block, a second autocorrelation failure in one draw);
/// the block resets after an autocorrelation failure (the witness's resets
/// too); the longest wait for one block in loop iterations (the tail's
/// measurement).
#[repr(C)]
pub struct TrngRecord {
    pub blocks: u32,
    pub discards_vn: u32,
    pub discards_crngt: u32,
    pub resets: u32,
    pub longest: u32,
}

#[unsafe(export_name = "__rhu_rp2350_trng")]
pub static mut TRNG_RECORD: TrngRecord = TrngRecord { blocks: 0, discards_vn: 0, discards_crngt: 0, resets: 0, longest: 0 };

unsafe fn trng_bump(p: *mut u32, by: u32) {
    unsafe { write_volatile(p, read_volatile(p).wrapping_add(by)) }
}

/// The source started for one generation.
unsafe fn trng_start() {
    unsafe { write_volatile(RND_SOURCE_ENABLE, RND_SRC_EN) }
}

/// The source stopped, every status bit cleared, the bits counter and the
/// valid registers reset (the reset takes only with the source disabled).
unsafe fn trng_stop() {
    unsafe {
        write_volatile(RND_SOURCE_ENABLE, 0);
        write_volatile(RNG_ICR, ISR_ALL);
        write_volatile(RST_BITS_COUNTER, 1);
    }
}

/// The block's configuration written whole (after the reset at the init and
/// after a block reset): the source off, the checks on, the chain and the
/// sample count, the interrupts masked, every status bit cleared, the
/// counter reset.
unsafe fn trng_configure() {
    unsafe {
        write_volatile(RND_SOURCE_ENABLE, 0);
        write_volatile(TRNG_DEBUG_CONTROL, 0);
        write_volatile(TRNG_CONFIG, TRNG_CONFIG_VALUE);
        write_volatile(SAMPLE_CNT1, SAMPLE_CNT1_VALUE);
        write_volatile(RNG_IMR, ISR_ALL);
        write_volatile(RNG_ICR, ISR_ALL);
        write_volatile(RST_BITS_COUNTER, 1);
    }
}

/// The configuration read back: the three words as written and no status
/// bit set (the reachability proof; no noise is involved).
unsafe fn trng_configured() -> bool {
    unsafe {
        read_volatile(TRNG_CONFIG) == TRNG_CONFIG_VALUE
            && read_volatile(SAMPLE_CNT1) == SAMPLE_CNT1_VALUE
            && read_volatile(TRNG_DEBUG_CONTROL) == 0
            && read_volatile(RNG_ISR) & ISR_ALL == 0
    }
}

/// The block's own reset after an autocorrelation failure (the one status
/// bit software cannot clear): the reset written, the bit waited for to
/// clear under the short bound, the configuration rewritten and read back.
unsafe fn trng_reset_block() -> bool {
    unsafe {
        write_volatile(TRNG_SW_RESET, 1);
        let mut n = 0u32;
        while read_volatile(RNG_ISR) & ISR_AUTOCORR_ERR != 0 {
            n += 1;
            if n > SPIN_SHORT {
                return false;
            }
        }
        trng_configure();
        trng_configured()
    }
}

/// The block started: its clock gate read (set at reset; a resident image
/// that gated it leaves the source absent), the block put INTO reset and
/// released (the boot ROM uses the block for its per-boot random with the
/// checks bypassed and leaves it as it was, so the reset is the known state;
/// the module's own release polls the done bit), configured whole and read
/// back. Nothing is drawn at the boot: the block has no startup self-test
/// the datasheet names -- its three checks run on every generation, so the
/// first draw is the first test, and a boot-time generation would wait
/// milliseconds with a documented tail for no verdict the draw does not
/// give. Nothing here masks an interrupt: the boot path runs before the
/// scheduler.
pub fn entropy_init() -> bool {
    unsafe {
        if read_volatile(CLOCKS_WAKE_EN0) & WAKE_EN0_CLK_SYS_TRNG == 0 {
            return false;
        }
        write_volatile(RESETS_RESET, read_volatile(RESETS_RESET) | RESET_TRNG);
        let mut n = 0u32;
        while read_volatile(RESETS_RESET_DONE) & RESET_TRNG != 0 {
            n += 1;
            if n > SPIN_SHORT {
                return false;
            }
        }
        release_reset(RESET_TRNG);
        trng_configure();
        trng_configured()
    }
}

/// One draw, per block of 24 bytes: the source enabled; THE WAIT OUTSIDE ANY
/// SECTION -- the status polled until the block reports a complete number,
/// a failed check or the bound; then the outcome:
///   * a complete number: under the kernel critical section the status is
///     read again (a check that failed meanwhile is handled as below) and
///     the six words are read, the last read clearing the result registers;
///     outside it the source is stopped and the status cleared;
///   * a run the von Neumann or the continuous check discarded: the block's
///     documented normal operation -- counted in the chip's record, the run
///     restarted within the same bound; past eight discards in one block
///     the source is faulty (the health flag: counted, the policy retires);
///   * the autocorrelation test failed four times in a row: the block has
///     ceased functioning and only its reset revives it -- the block reset,
///     the configuration rewritten, a block reset counted (the witness's
///     resets), the run restarted once; a second in one draw is the health
///     flag, and so is a reset that does not take;
///   * the bound met: the draw refuses "try again" (the witness's timeouts;
///     three in a row retire).
/// The source is stopped on every exit ("when not in use"). The section
/// covers seven register reads and never a wait: an interrupt at or above
/// the ceiling is delayed by those reads alone, the zero-latency band below
/// it never. The bytes land in the caller's buffer word by word; the policy
/// layer copies out only when the whole draw succeeded.
pub fn entropy_draw(buf: &mut [u8], counters: &mut super::DrawCounters) -> Result<(), super::EntropyErr> {
    for chunk in buf.chunks_mut(BLOCK_BYTES) {
        let mut words = [0u32; EHR_WORDS];
        let mut n = 0u32;
        let mut vn = 0u32;
        let mut crngt = 0u32;
        let mut resets = 0u32;
        unsafe { trng_start() };
        let outcome = loop {
            let isr = unsafe { read_volatile(RNG_ISR) } & ISR_ALL;
            if isr != 0 {
                let seen = crate::critical::with_cs(|| unsafe {
                    let isr = read_volatile(RNG_ISR) & ISR_ALL;
                    if isr & ISR_FAILED != 0 {
                        return isr;
                    }
                    for (i, w) in words.iter_mut().enumerate() {
                        *w = read_volatile(EHR_DATA0.add(i));
                    }
                    ISR_EHR_VALID
                });
                if seen & ISR_AUTOCORR_ERR != 0 {
                    resets += 1;
                    if resets > 1 || !unsafe { trng_reset_block() } {
                        break Err(super::EntropyErr::Health);
                    }
                    unsafe { trng_start() };
                } else if seen & ISR_FAILED != 0 {
                    if seen & ISR_VN_ERR != 0 {
                        vn += 1;
                    }
                    if seen & ISR_CRNGT_ERR != 0 {
                        crngt += 1;
                    }
                    if vn + crngt > DISCARDS_PER_BLOCK_MAX {
                        break Err(super::EntropyErr::Health);
                    }
                    unsafe {
                        trng_stop();
                        trng_start();
                    }
                } else {
                    break Ok(());
                }
            }
            n += 1;
            if n > BLOCK_BOUND {
                break Err(super::EntropyErr::Timeout);
            }
        };
        unsafe { trng_stop() };
        crate::critical::with_cs(|| unsafe {
            let r = core::ptr::addr_of_mut!(TRNG_RECORD);
            if outcome.is_ok() {
                trng_bump(core::ptr::addr_of_mut!((*r).blocks), 1);
            }
            trng_bump(core::ptr::addr_of_mut!((*r).discards_vn), vn);
            trng_bump(core::ptr::addr_of_mut!((*r).discards_crngt), crngt);
            trng_bump(core::ptr::addr_of_mut!((*r).resets), resets);
            if n > read_volatile(core::ptr::addr_of!((*r).longest)) {
                write_volatile(core::ptr::addr_of_mut!((*r).longest), n);
            }
        });
        counters.resets += resets;
        match outcome {
            Ok(()) => {
                let mut bytes = [0u8; BLOCK_BYTES];
                for (i, w) in words.iter().enumerate() {
                    bytes[i * 4..i * 4 + 4].copy_from_slice(&w.to_le_bytes());
                }
                chunk.copy_from_slice(&bytes[..chunk.len()]);
            }
            Err(e) => {
                if matches!(&e, super::EntropyErr::Health) {
                    counters.errors += 1;
                }
                return Err(e);
            }
        }
    }
    Ok(())
}

/// THE WIRELESS MODULE'S TENANCY on the bank's one interrupt line, for the
/// unhosted program (the claims table's tenant interface). The module's data
/// line is GPIO 24 as the board wires it, and its level-high enable is bit 1
/// of PROC0_INTE3, the word that covers lines 24 to 31 at four bits per
/// line. The program arms that bit at the IO bank across each wait for a
/// frame and disarms it after; the family handler masks the bit before it
/// wakes the word (a level is not latched); the vector line is enabled here
/// once, at the registration, and nothing fires until the program arms the
/// bit, so an image that never arms it carries the enabled line harmlessly.
/// The wake word is the kernel's, so its address is mapped and aligned for
/// the life of every wait; the program reads it by symbol, as the kernel
/// reaches the program's entry. What is masked: one enable bit of one line
/// at the IO bank, by the handler and by the program; nothing at the
/// controller's global level, nothing in the zero-latency band, and the
/// handler itself runs in the lowest kernel band.
#[cfg(feature = "unhosted-main")]
pub mod wireless_tenant {
    use core::ptr::addr_of_mut;
    use core::sync::atomic::AtomicU32;

    use super::gpio_backend::{inte, nvic_enable_once};
    use crate::critical::with_cs;
    use crate::gpio_math as m;
    use crate::{claims, gpio};

    /// The module's data line.
    const DATA_LINE: u32 = 24;

    /// The wake word the family handler wakes for the data line's bit.
    #[unsafe(export_name = "__rhu_wireless_wake")]
    pub static WIRELESS_WAKE: AtomicU32 = AtomicU32::new(0);

    /// The tenancy registered and the vector line enabled; the table's code
    /// (0 taken; the busy code when the bit is held or the table is full).
    pub fn init() -> i32 {
        let (k, s) = m::rp_word(DATA_LINE);
        let wake = WIRELESS_WAKE.as_ptr() as u32;
        let r = with_cs(|| unsafe {
            claims::tenant_add(&mut *addr_of_mut!(gpio::CLAIMS), inte(k) as u32, s + m::RP_LEVEL_HIGH, wake)
        });
        if r == 0 {
            nvic_enable_once();
        }
        r
    }
}
