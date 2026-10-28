//! The emulator rows' CMSDK APB UARTs as the UART doors' backend (`uart.rs`
//! owns the rings, the witness and the doors). Both
//! rows carry the block twice: UART0 (the consoles' default wire, driven
//! polled by the board module -- this backend never touches a block the
//! console holds that way) and UART1, the program's port by default, on
//! the bases and interrupt lines the board module states (`UART_BASES`,
//! `UART_IRQS`: the AN505's UART1 at 0x5020_1000 through the secure alias
//! with the receive line 34 and the transmit line 35; the AN500's at
//! 0x4000_5000 with the lines 2 and 3 -- QEMU's mps2-tz.c and mps2.c, the
//! copies of record).
//!
//! THE RATE RULE: BAUDDIV = PCLK / baud rounded to nearest, 16 to 2^20 - 1;
//! the achieved rate PCLK / BAUDDIV; a rate the divisor misses by more
//! than 2 percent, or that needs a divisor below 16, is refused.

use core::ptr::{read_volatile, write_volatile};

/// The blocks' input clock: the FPGA image's one clock.
pub const CLOCK_HZ: u32 = super::SYSCLK_HZ;
/// 8N1 alone, no break, one character each way.
pub const FRAMES_8N1_ONLY: bool = true;
pub const HAS_BREAK: bool = false;
pub const FIFO_DEPTH: u32 = 1;

/// The error flags `rx_get` could answer (the part reports none with a
/// byte; an overrun comes through `ack`).
pub const ERR_FRAMING: u32 = 1 << 0;
pub const ERR_PARITY: u32 = 1 << 1;
pub const ERR_BREAK: u32 = 1 << 2;
pub const ERR_OVERRUN: u32 = 1 << 3;
/// `ack` reports a receive overrun (a character arrived while one was
/// pending; the newest is lost).
pub const ACK_OVERRUN: u32 = 1 << 0;

const DATA: usize = 0x0;
const STATE: usize = 0x4;
const CTRL: usize = 0x8;
const INTSTATUS: usize = 0xC;
const BAUDDIV: usize = 0x10;

const STATE_TXFULL: u32 = 1 << 0;
const STATE_RXFULL: u32 = 1 << 1;
const CTRL_TX_EN: u32 = 1 << 0;
const CTRL_RX_EN: u32 = 1 << 1;
const CTRL_TX_INTEN: u32 = 1 << 2;
const CTRL_RX_INTEN: u32 = 1 << 3;
const CTRL_RXO_INTEN: u32 = 1 << 5;
const INT_RXO: u32 = 1 << 3;
const INT_ALL: u32 = 0xF;
const CTRL_UP: u32 = CTRL_TX_EN | CTRL_RX_EN | CTRL_TX_INTEN | CTRL_RX_INTEN | CTRL_RXO_INTEN;
const BAUDDIV_MIN: u32 = 16;
const BAUDDIV_MAX: u32 = (1 << 20) - 1;

const NVIC_ISER: *mut u32 = 0xE000_E100 as *mut u32;
const NVIC_ICER: *mut u32 = 0xE000_E180 as *mut u32;
const NVIC_ICPR: *mut u32 = 0xE000_E280 as *mut u32;
const NVIC_IPR: *mut u8 = 0xE000_E400 as *mut u8;
/// The lowest band: masked inside a kernel critical section (the ceiling
/// 0x40), below the tick (0x80), above the context switch (0xF0).
const IRQ_PRIO: u8 = 0xE0;

fn reg(ix: usize, off: usize) -> *mut u32 {
    (super::UART_BASES[ix & 1] + off) as *mut u32
}

unsafe fn nvic(reg_base: *mut u32, line: usize) {
    unsafe { write_volatile(reg_base.add(line / 32), 1 << (line % 32)) }
}

/// Whether the block is enabled by this backend.
pub fn is_up(ix: usize) -> bool {
    unsafe { read_volatile(reg(ix, CTRL)) & CTRL_TX_EN != 0 }
}

/// The rate rule: the divider and the achieved rate, or None.
pub fn divisor(baud: u32) -> Option<(u32, u32)> {
    if baud == 0 {
        return None;
    }
    let div = (CLOCK_HZ as u64 + (baud as u64) / 2) / (baud as u64);
    if div < BAUDDIV_MIN as u64 || div > BAUDDIV_MAX as u64 {
        return None;
    }
    let achieved = (CLOCK_HZ as u64 / div) as u32;
    if achieved.abs_diff(baud) as u64 * 50 > baud as u64 {
        return None;
    }
    Some((div as u32, achieved))
}

/// The block brought up at the claim's state: the divider for 115200, the
/// status bits cleared, both interrupt lines armed at the band's lowest
/// priority, the block enabled both ways with its interrupts on.
pub fn claim(ix: usize, _tx: u32, _rx: u32) {
    unsafe {
        write_volatile(reg(ix, CTRL), 0);
        let (div, _) = divisor(crate::uart::DEFAULT_BAUD).unwrap_or((CLOCK_HZ / crate::uart::DEFAULT_BAUD, 0));
        write_volatile(reg(ix, BAUDDIV), div);
        write_volatile(reg(ix, INTSTATUS), INT_ALL);
        let (rx_line, tx_line) = super::UART_IRQS[ix & 1];
        for line in [rx_line, tx_line] {
            write_volatile(NVIC_IPR.add(line), IRQ_PRIO);
            nvic(NVIC_ICPR, line);
            nvic(NVIC_ISER, line);
        }
        write_volatile(reg(ix, CTRL), CTRL_UP);
    }
}

/// The divider reprogrammed (8N1 is the part's only frame; the caller let
/// the pending transmit leave and holds the section).
pub fn configure(ix: usize, div: u32, _data_bits: u32, _parity: u32, _stop_bits: u32, _brk: bool) {
    unsafe {
        write_volatile(reg(ix, CTRL), 0);
        write_volatile(reg(ix, BAUDDIV), div);
        write_volatile(reg(ix, CTRL), CTRL_UP);
    }
}

/// The block disabled, its status cleared, both lines disarmed.
pub fn release(ix: usize, _tx: u32, _rx: u32) {
    unsafe {
        write_volatile(reg(ix, CTRL), 0);
        write_volatile(reg(ix, INTSTATUS), INT_ALL);
        let (rx_line, tx_line) = super::UART_IRQS[ix & 1];
        for line in [rx_line, tx_line] {
            nvic(NVIC_ICER, line);
            nvic(NVIC_ICPR, line);
        }
    }
}

pub fn tx_room(ix: usize) -> bool {
    unsafe { read_volatile(reg(ix, STATE)) & STATE_TXFULL == 0 }
}

pub fn tx_put(ix: usize, b: u8) {
    unsafe { write_volatile(reg(ix, DATA), b as u32) }
}

/// The transmitter idle: the one character slot empty (the model sends a
/// written character before it clears the flag).
pub fn tx_idle(ix: usize) -> bool {
    tx_room(ix)
}

/// The pending character, or None (the part reports no error with a byte).
pub fn rx_get(ix: usize) -> Option<(u8, u32)> {
    unsafe {
        if read_volatile(reg(ix, STATE)) & STATE_RXFULL == 0 {
            return None;
        }
        Some((read_volatile(reg(ix, DATA)) as u8, 0))
    }
}

/// The pending character discarded.
pub fn rx_flush(ix: usize) {
    unsafe {
        if read_volatile(reg(ix, STATE)) & STATE_RXFULL != 0 {
            let _ = read_volatile(reg(ix, DATA));
        }
    }
}

/// No break control on this part (the door refuses before it reaches here).
pub fn set_break(_ix: usize, _on: bool) {}

/// The status bits read and written back to clear; a receive overrun
/// reported.
pub fn ack(ix: usize) -> u32 {
    unsafe {
        let s = read_volatile(reg(ix, INTSTATUS));
        write_volatile(reg(ix, INTSTATUS), s);
        if s & INT_RXO != 0 { ACK_OVERRUN } else { 0 }
    }
}

/// No loopback bit on this part.
#[allow(dead_code)]
pub fn loopback(_ix: usize, _on: bool) -> bool {
    false
}

/// The four handlers (a line per direction per block), each the doors'
/// module's handler for its block.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __rhu_cmsdk_uart0_rx_irq() {
    crate::uart::irq(0)
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __rhu_cmsdk_uart0_tx_irq() {
    crate::uart::irq(0)
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __rhu_cmsdk_uart1_rx_irq() {
    crate::uart::irq(1)
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __rhu_cmsdk_uart1_tx_irq() {
    crate::uart::irq(1)
}

/// The interrupt lines this backend claims in the vector table (the boot
/// module folds them): a handler is inert until a port arms its lines.
pub const BOARD_IRQS: &[(usize, unsafe extern "C" fn())] = &[
    (super::UART_IRQS[0].0, __rhu_cmsdk_uart0_rx_irq),
    (super::UART_IRQS[0].1, __rhu_cmsdk_uart0_tx_irq),
    (super::UART_IRQS[1].0, __rhu_cmsdk_uart1_rx_irq),
    (super::UART_IRQS[1].1, __rhu_cmsdk_uart1_tx_irq),
];
