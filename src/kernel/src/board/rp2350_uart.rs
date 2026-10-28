//! The RP2350's two PL011 UART blocks as the UART doors' backend
//! (`uart.rs` owns the rings, the witness and the doors; this module owns the
//! registers, the pins and the interrupt lines). UART0 at 0x4007_0000 and
//! UART1 at 0x4007_8000 (the reset bits 26 and 27), each on its own
//! interrupt line (33 and 34) at the lowest priority band; the blocks'
//! input clock is the peripheral clock, the 12 MHz crystal at every
//! posture (the PLL posture changes the system clock, not clk_peri), so the rate table
//! does not move with the posture.
//!
//! THE RATE RULE (the datasheet's divisor): BRD = clock / (16 x baud) in
//! 64ths rounded to nearest, the integer part into IBRD (1 to 65,535) and
//! the six fraction bits into FBRD; the achieved rate = clock x 4 / BRD64,
//! and a rate whose achieved value misses the request by more than 2
//! percent, or that needs IBRD 0 or past 65,535, is refused -- at 12 MHz
//! the ceiling is 750,000 Bd (921,600 refused; 460,800 lands 0.16 percent
//! high and is taken), the floor 12 Bd. THE FRAME SET: 5 to 8 data bits
//! (WLEN), none / odd / even / mark / space parity (PEN, EPS, SPS), 1 or 2
//! stop bits (STP2). THE FIFOS on (32 deep each), the receive trigger at
//! 1/8 for latency and the transmit trigger at 1/2 (IFLS).
//!
//! THE PINS: a port's two lines take the UART function (FUNCSEL 2 -- the
//! function table's F2 column; the chip's UART AUX function (select 11) is not
//! served) with the pads configured in the datasheet's order (the pad word
//! with the isolation latch still set, then the latch released): the
//! transmit pin input-enabled without pulls, the receive pin input-enabled
//! and pulled UP before the receiver is enabled (the reset state pulls every
//! pad down, and a pulled-down receive line reads as a permanent break). At
//! the release the pins return to the GPIO door's released state (the
//! function NULL) with the receive pull kept so the line idles high.

use core::ptr::{read_volatile, write_volatile};

use super::rp2350::{gpio_backend, PAD_IE, PAD_ISO, PAD_OD, PAD_PDE, PAD_PUE};

/// The blocks' input clock (clk_peri, the crystal).
pub const CLOCK_HZ: u32 = super::rp2350::PERI_HZ;
/// The part's frame set is the PL011's, with break control and 32-deep FIFOs.
pub const FRAMES_8N1_ONLY: bool = false;
pub const HAS_BREAK: bool = true;
pub const FIFO_DEPTH: u32 = 32;

/// The error flags a received word carries in [11:8] (FE, PE, BE, OE), as
/// `rx_get` answers them.
pub const ERR_FRAMING: u32 = 1 << 0;
pub const ERR_PARITY: u32 = 1 << 1;
pub const ERR_BREAK: u32 = 1 << 2;
pub const ERR_OVERRUN: u32 = 1 << 3;
/// `ack` reports nothing of its own on this part: every error comes with
/// its byte through the data register.
pub const ACK_OVERRUN: u32 = 1 << 0;

const UART_BASES: [usize; 2] = [0x4007_0000, 0x4007_8000];
const RESET_UART0: u32 = 1 << 26;
const RESET_UART1: u32 = 1 << 27;
const UART_RESETS: [u32; 2] = [RESET_UART0, RESET_UART1];
const UART0_IRQ: usize = 33;
const UART1_IRQ: usize = 34;
pub const IRQS: [usize; 2] = [UART0_IRQ, UART1_IRQ];

const DR: usize = 0x00;
const RSR: usize = 0x04;
const FR: usize = 0x18;
const IBRD: usize = 0x24;
const FBRD: usize = 0x28;
const LCR_H: usize = 0x2C;
const CR: usize = 0x30;
const IFLS: usize = 0x34;
const IMSC: usize = 0x38;
const MIS: usize = 0x40;
const ICR: usize = 0x44;

/// FR: BUSY bit 3 (the transmitter busy, the shift register included), RXFE
/// bit 4, TXFF bit 5, TXFE bit 7.
const FR_BUSY: u32 = 1 << 3;
const FR_RXFE: u32 = 1 << 4;
const FR_TXFF: u32 = 1 << 5;
const FR_TXFE: u32 = 1 << 7;
/// A received word: the byte in [7:0], FE PE BE OE in [11:8].
const DR_ERRORS: u32 = 0xF << 8;
/// LCR_H: BRK 0, PEN 1, EPS 2, STP2 3, FEN 4, WLEN [6:5], SPS 7.
const LCR_H_BRK: u32 = 1 << 0;
const LCR_H_PEN: u32 = 1 << 1;
const LCR_H_EPS: u32 = 1 << 2;
const LCR_H_STP2: u32 = 1 << 3;
const LCR_H_FEN: u32 = 1 << 4;
const LCR_H_SPS: u32 = 1 << 7;
/// CR: UARTEN 0, LBE 7 (the loopback bit), TXE 8, RXE 9.
const CR_UARTEN: u32 = 1 << 0;
const CR_LBE: u32 = 1 << 7;
const CR_TXE: u32 = 1 << 8;
const CR_RXE: u32 = 1 << 9;
/// IFLS: the transmit trigger in [2:0] (2 = 1/2), the receive trigger in
/// [5:3] (0 = 1/8).
const IFLS_TX_HALF_RX_EIGHTH: u32 = 0b000_010;
/// The interrupt bits of IMSC, MIS and ICR: RX 4, TX 5, RT 6 (the receive
/// timeout), FE 7, PE 8, BE 9, OE 10.
const INT_RX: u32 = 1 << 4;
const INT_TX: u32 = 1 << 5;
const INT_RT: u32 = 1 << 6;
const INT_FE: u32 = 1 << 7;
const INT_PE: u32 = 1 << 8;
const INT_BE: u32 = 1 << 9;
const INT_OE: u32 = 1 << 10;
const INT_SERVED: u32 = INT_RX | INT_TX | INT_RT | INT_FE | INT_PE | INT_BE | INT_OE;
const ICR_ALL: u32 = 0x7FF;
/// The UART function on a UART pin (the function table's F2 column).
const FUNCSEL_UART: u32 = 2;
const FUNCSEL_NULL: u32 = 0x1F;

const NVIC_ISER1: *mut u32 = 0xE000_E104 as *mut u32;
const NVIC_ICER1: *mut u32 = 0xE000_E184 as *mut u32;
const NVIC_ICPR1: *mut u32 = 0xE000_E284 as *mut u32;
const NVIC_IPR: *mut u8 = 0xE000_E400 as *mut u8;
/// The lowest band: masked inside a kernel critical section (the ceiling
/// 0x40), below the tick (0x80), above the context switch (0xF0).
const IRQ_PRIO: u8 = 0xE0;

fn reg(ix: usize, off: usize) -> *mut u32 {
    (UART_BASES[ix & 1] + off) as *mut u32
}

/// Whether the block is out of reset and enabled (the drain at exit reads
/// a block the console holds).
pub fn is_up(ix: usize) -> bool {
    unsafe { read_volatile(reg(ix, CR)) & CR_UARTEN != 0 }
}

/// The rate rule: the divisor in 64ths and the achieved rate, or None for a
/// rate the part cannot reach within 2 percent.
pub fn divisor(baud: u32) -> Option<(u32, u32)> {
    if baud == 0 {
        return None;
    }
    let brd64 = ((CLOCK_HZ as u64) * 4 + (baud as u64) / 2) / (baud as u64);
    let ibrd = brd64 >> 6;
    if ibrd == 0 || ibrd > 0xFFFF {
        return None;
    }
    let achieved = ((CLOCK_HZ as u64) * 4 / brd64) as u32;
    let miss = achieved.abs_diff(baud) as u64;
    if miss * 50 > baud as u64 {
        return None;
    }
    Some((brd64 as u32, achieved))
}

/// LCR_H for a frame (the FIFOs on; the break bit as held).
fn lcr_h(data_bits: u32, parity: u32, stop_bits: u32, brk: bool) -> u32 {
    let mut v = LCR_H_FEN | ((data_bits - 5) & 3) << 5;
    if stop_bits == 2 {
        v |= LCR_H_STP2;
    }
    match parity {
        crate::uart::PARITY_ODD => v |= LCR_H_PEN,
        crate::uart::PARITY_EVEN => v |= LCR_H_PEN | LCR_H_EPS,
        crate::uart::PARITY_MARK => v |= LCR_H_PEN | LCR_H_SPS,
        crate::uart::PARITY_SPACE => v |= LCR_H_PEN | LCR_H_SPS | LCR_H_EPS,
        _ => {}
    }
    if brk {
        v |= LCR_H_BRK;
    }
    v
}

/// The divisors and the line format written with the block disabled (the
/// line-control write latches the divisors), then the block enabled both
/// ways.
unsafe fn program(ix: usize, brd64: u32, lcr: u32) {
    unsafe {
        write_volatile(reg(ix, CR), 0);
        write_volatile(reg(ix, IBRD), brd64 >> 6);
        write_volatile(reg(ix, FBRD), brd64 & 0x3F);
        write_volatile(reg(ix, LCR_H), lcr);
        write_volatile(reg(ix, CR), CR_UARTEN | CR_TXE | CR_RXE);
    }
}

/// The block brought up at the claim's state: out of reset, the two pins
/// on the UART function with their pads, 115200 8N1 with the FIFOs on and
/// the triggers set, every interrupt cleared, the served interrupts armed
/// at the band's lowest priority, the block enabled both ways.
pub fn claim(ix: usize, tx: u32, rx: u32) {
    unsafe {
        super::rp2350::release_reset(super::rp2350::RESET_IO_BANK0 | super::rp2350::RESET_PADS_BANK0 | UART_RESETS[ix & 1]);
        if tx != crate::uart::NO_PIN {
            write_volatile(gpio_backend::ctrl(tx), FUNCSEL_UART);
            write_volatile(gpio_backend::ctrl(rx), FUNCSEL_UART);
            let pad_tx = (read_volatile(gpio_backend::pad(tx)) & !(PAD_OD | PAD_PUE | PAD_PDE)) | PAD_IE;
            write_volatile(gpio_backend::pad(tx), pad_tx | PAD_ISO);
            write_volatile(gpio_backend::pad(tx), pad_tx & !PAD_ISO);
            let pad_rx = (read_volatile(gpio_backend::pad(rx)) & !(PAD_OD | PAD_PDE)) | PAD_IE | PAD_PUE;
            write_volatile(gpio_backend::pad(rx), pad_rx | PAD_ISO);
            write_volatile(gpio_backend::pad(rx), pad_rx & !PAD_ISO);
        }
        let (brd64, _) = divisor(crate::uart::DEFAULT_BAUD).unwrap_or((417, 0));
        write_volatile(reg(ix, CR), 0);
        write_volatile(reg(ix, IMSC), 0);
        write_volatile(reg(ix, ICR), ICR_ALL);
        write_volatile(reg(ix, RSR), 0);
        write_volatile(reg(ix, IFLS), IFLS_TX_HALF_RX_EIGHTH);
        program(ix, brd64, lcr_h(8, crate::uart::PARITY_NONE, 1, false));
        let irq = IRQS[ix & 1];
        write_volatile(NVIC_IPR.add(irq), IRQ_PRIO);
        write_volatile(NVIC_ICPR1, 1 << (irq - 32));
        write_volatile(NVIC_ISER1, 1 << (irq - 32));
        write_volatile(reg(ix, IMSC), INT_SERVED);
    }
}

/// The line discipline reprogrammed (the caller let the pending transmit
/// leave and holds the section).
pub fn configure(ix: usize, brd64: u32, data_bits: u32, parity: u32, stop_bits: u32, brk: bool) {
    unsafe {
        while read_volatile(reg(ix, FR)) & FR_BUSY != 0 {}
        program(ix, brd64, lcr_h(data_bits, parity, stop_bits, brk));
    }
}

/// The block disabled and held in reset, its interrupts masked and its
/// line disarmed, the two pads returned to the released state.
pub fn release(ix: usize, tx: u32, rx: u32) {
    unsafe {
        write_volatile(reg(ix, IMSC), 0);
        let irq = IRQS[ix & 1];
        write_volatile(NVIC_ICER1, 1 << (irq - 32));
        write_volatile(reg(ix, CR), 0);
        write_volatile(reg(ix, ICR), ICR_ALL);
        write_volatile(NVIC_ICPR1, 1 << (irq - 32));
        super::rp2350::assert_reset(UART_RESETS[ix & 1]);
        if tx != crate::uart::NO_PIN {
            write_volatile(gpio_backend::ctrl(tx), FUNCSEL_NULL);
            write_volatile(gpio_backend::ctrl(rx), FUNCSEL_NULL);
            let p = gpio_backend::pad(tx);
            write_volatile(p, read_volatile(p) & !(PAD_PUE | PAD_PDE));
            let p = gpio_backend::pad(rx);
            write_volatile(p, read_volatile(p) & !(PAD_PUE | PAD_PDE));
        }
    }
}

pub fn tx_room(ix: usize) -> bool {
    unsafe { read_volatile(reg(ix, FR)) & FR_TXFF == 0 }
}

pub fn tx_put(ix: usize, b: u8) {
    unsafe { write_volatile(reg(ix, DR), b as u32) }
}

/// The transmitter idle: the FIFO empty and the shift register done.
pub fn tx_idle(ix: usize) -> bool {
    let fr = unsafe { read_volatile(reg(ix, FR)) };
    fr & FR_TXFE != 0 && fr & FR_BUSY == 0
}

/// One received word, or None when the FIFO is empty: the byte and its
/// error flags (the receive status cleared when a flag is set).
pub fn rx_get(ix: usize) -> Option<(u8, u32)> {
    unsafe {
        if read_volatile(reg(ix, FR)) & FR_RXFE != 0 {
            return None;
        }
        let w = read_volatile(reg(ix, DR));
        let mut err = 0;
        if w & DR_ERRORS != 0 {
            write_volatile(reg(ix, RSR), 0);
            if w & (1 << 8) != 0 {
                err |= ERR_FRAMING;
            }
            if w & (1 << 9) != 0 {
                err |= ERR_PARITY;
            }
            if w & (1 << 10) != 0 {
                err |= ERR_BREAK;
            }
            if w & (1 << 11) != 0 {
                err |= ERR_OVERRUN;
            }
        }
        Some((w as u8, err))
    }
}

/// The receive FIFO drained and discarded, the receive status cleared.
pub fn rx_flush(ix: usize) {
    unsafe {
        while read_volatile(reg(ix, FR)) & FR_RXFE == 0 {
            let _ = read_volatile(reg(ix, DR));
        }
        write_volatile(reg(ix, RSR), 0);
    }
}

/// The break condition on the line (LCR_H's BRK), the rest of the register
/// kept.
pub fn set_break(ix: usize, on: bool) {
    unsafe {
        let v = read_volatile(reg(ix, LCR_H));
        write_volatile(reg(ix, LCR_H), if on { v | LCR_H_BRK } else { v & !LCR_H_BRK });
    }
}

/// The pending interrupt sources read and cleared (the served bits of
/// ICR); the errors come with their bytes, so nothing is reported here.
pub fn ack(ix: usize) -> u32 {
    unsafe {
        let mis = read_volatile(reg(ix, MIS));
        write_volatile(reg(ix, ICR), mis & INT_SERVED);
    }
    0
}

/// The loopback bit (CR.LBE): the transmitter fed back to the receiver
/// inside the block (the flight's first image).
pub fn loopback(ix: usize, on: bool) -> bool {
    unsafe {
        let v = read_volatile(reg(ix, CR));
        write_volatile(reg(ix, CR), if on { v | CR_LBE } else { v & !CR_LBE });
    }
    true
}

/// The two blocks' interrupt handlers: the doors' module's handler for the
/// block (the lines are armed only while a port is held).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __rhu_rp2350_uart0_irq() {
    crate::uart::irq(0)
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __rhu_rp2350_uart1_irq() {
    crate::uart::irq(1)
}
