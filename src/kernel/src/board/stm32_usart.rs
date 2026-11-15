//! The ST serial blocks behind the serial functions: a program's port on
//! the STM32F7's USART and on the STM32U5's low-power UART, in the shape of
//! the PL011 and CMSDK backends (`uart.rs` owns the rings, the claims, the
//! witness and the functions; this module owns the registers, the pads and
//! the interrupt line). One file serves both chips: the register file is
//! the same for everything a port uses, `crate::stm32_usart_math` states it
//! with the manuals' pages and holds every sequence as a pure function over
//! a two-verb register seam, and a board's facts state each block -- its
//! base, its bits in the clock controller, its kernel clock selector, its
//! rate rule, its interrupt line. Here the two verbs are volatile accesses.
//!
//! THE CLOCK of a port is the 16 MHz internal oscillator, chosen by the
//! block's own selector at the claim, so a port's rates do not move with
//! the row's clock posture. THE PADS: the two lines take the block's
//! alternate function through the family's pad code, the transmit pad
//! push-pull with its pull-up at the port's high speed setting, the
//! receive pad pulled up -- an idle line reads high with nothing attached,
//! while the block is disabled, and while the single-wire mode releases
//! the transmit pad between frames. At the release both pads return to
//! plain inputs without a pull, the GPIO backend's released shape.
//!
//! A HELD BREAK is sent by the pad. These blocks send one break frame at a
//! request and have no held break of their own, so the transmit pad is
//! made a plain output driven low for as long as the break is held, and is
//! handed back to the block after: an input with its pull-up first, so the
//! line rises before the block has it. What a program writes while it holds
//! a break leaves the block and never reaches the line, as on a part whose
//! break is a bit of its own. THE LOOP is the block's single-wire mode,
//! which joins the receiver to the transmit line inside the chip. THE
//! TRANSMIT INTERRUPT is a level on these blocks: the acknowledge turns it
//! off and each byte put turns it on. The interrupt line runs in the lowest
//! band, masked inside a kernel critical section. The record
//! `__rhu_stm32_usart` is read by symbol.

use core::ptr::{addr_of_mut, read_volatile, write_volatile};
use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use crate::board::facts;
use crate::board::gpio_backend as pins;
use crate::board::stm32_bus as pads;
use crate::critical::with_cs;
use crate::gpio_math as g;
use crate::stm32_usart_math as m;
use crate::stm32_usart_math::Regs;

const NP: usize = facts::UART_ROWS.len();
const _: () = assert!(m::blocks_fit(NP, facts::UART_BLOCKS), "the row's serial block table does not fit its port table: not one block a port, a block that is not sound, two rate rules in one table, or two blocks of one base or one interrupt line");
const _: () = assert!(NP == 1, "this backend's one handler serves the port at the table's first position: a second port needs its own handler and its own entry in the board's interrupt list");

/// The ports' clock: the 16 MHz internal oscillator.
pub const CLOCK_HZ: u32 = m::CLOCK_HZ;
/// The blocks' frame set is their own (`uart.rs` asks the pure module's
/// predicate on these rows), with a held break by the pad.
pub const FRAMES_8N1_ONLY: bool = false;
pub const HAS_BREAK: bool = true;
/// The transmitter's share of a pending count.
pub const FIFO_DEPTH: u32 = m::fifo_depth(&facts::UART_BLOCKS[0]);

/// The error word of a received byte, as `rx_get` answers it.
pub const ERR_FRAMING: u32 = m::ERR_FRAMING;
pub const ERR_PARITY: u32 = m::ERR_PARITY;
pub const ERR_BREAK: u32 = m::ERR_BREAK;
/// Never set with a byte on these blocks: an overrun is a flag of the
/// block, not of a byte, and the acknowledge answers it.
pub const ERR_OVERRUN: u32 = 1 << 3;
pub const ACK_OVERRUN: u32 = m::ACK_OVERRUN;

/// The interrupt controller's set-enable, clear-enable and clear-pending
/// registers (one bit a line, write-one) and its priority bytes.
const NVIC_ISER: usize = 0xE000_E100;
const NVIC_ICER: usize = 0xE000_E180;
const NVIC_ICPR: usize = 0xE000_E280;
const NVIC_IPR: usize = 0xE000_E400;
/// The lowest band: masked inside a kernel critical section (the ceiling
/// 0x40), below the tick (0x80), above the context switch (0xF0).
const IRQ_PRIO: u8 = 0xE0;

/// The pads' output speed (the port's OSPEEDR field): the high setting for
/// the driven pad, the low one for the pad that only listens.
const SPEED_HIGH: u32 = 0b10;
const SPEED_LOW: u32 = 0b00;

/// The bound on the two acknowledge flags after an enable, in reads of the
/// status register. The low-power block's acknowledges arrive through its
/// prescaled clock, 62.5 kHz at the slowest rates it serves: some 48
/// microseconds, which this count exceeds more than ten times at the
/// fastest core clock of the rows.
const ACK_POLLS: u32 = 20_000;

/// Per port: the loop as asked (kept across a configure) and the mask of
/// the frame's data bits in a received word.
static LOOP: [AtomicBool; NP] = [const { AtomicBool::new(false) }; NP];
static MASK: [AtomicU32; NP] = [const { AtomicU32::new(0xFF) }; NP];

/// The backend's record, by symbol: the divisor register, the prescaler
/// register (0 on a block without one) and the three control registers as
/// they read back after the last configure; the selector's field as it
/// read back at the last claim; the configures whose two acknowledge flags
/// did not arrive inside the bound; and the second I/O supply's valid bit
/// as it read at the last claim (2 on a row that states no such supply).
#[repr(C)]
pub struct UsartRecord {
    pub divisor: u32,
    pub prescaler: u32,
    pub cr1: u32,
    pub cr2: u32,
    pub cr3: u32,
    pub selector: u32,
    pub acks_missed: u32,
    pub supply: u32,
}

#[unsafe(export_name = "__rhu_stm32_usart")]
pub static mut RECORD: UsartRecord = UsartRecord { divisor: 0, prescaler: 0, cr1: 0, cr2: 0, cr3: 0, selector: 0, acks_missed: 0, supply: 2 };

/// The register seam on the chip: a volatile read and a volatile write.
struct Chip;

impl Regs for Chip {
    fn rd(&mut self, addr: usize) -> u32 {
        unsafe { read_volatile(addr as *const u32) }
    }
    fn wr(&mut self, addr: usize, value: u32) {
        unsafe { write_volatile(addr as *mut u32, value) }
    }
}

/// The block of a table position.
fn block(ix: usize) -> &'static m::Block {
    &facts::UART_BLOCKS[ix % NP]
}

/// The interrupt lines this backend claims with its handler, for the
/// board's list: the block's line of each table position.
pub const IRQS: [(usize, unsafe extern "C" fn()); NP] = [(facts::UART_BLOCKS[0].irq, __rhu_stm32_usart0_irq)];

/// Whether the block is enabled.
pub fn is_up(ix: usize) -> bool {
    m::is_up(&mut Chip, block(ix))
}

/// The rate rule of the row's blocks at the port's clock: the register
/// word and the achieved rate, or none for a rate outside the block's reach
/// or more than 2 percent off.
pub fn divisor(baud: u32) -> Option<(u32, u32)> {
    m::divisor(facts::UART_BLOCKS[0].law, baud)
}

/// The word the block is given. The doctored control writes the divisor
/// twice as long as the rule's, the prescaler's code kept: the wire then
/// runs at half the rate the function answered.
fn written(word: u32) -> u32 {
    if cfg!(feature = "test-uart-doctored") {
        let div = (word & 0xF_FFFF) * 2;
        (word & !0xF_FFFF) | if div > 0xF_FFFF { 0xF_FFFF } else { div }
    } else {
        word
    }
}

/// The block at a claim: its clock, its reset and its selector by the pure
/// module's sequence, under the section (the clock controller's registers
/// are shared between blocks); the selector's field and the second I/O
/// supply's valid bit read into the record; the two pads handed to the
/// block (the family's GPIO driver enables each port's clock first); the
/// claim's state, 115200 Bd with 8 data bits, no parity and 1 stop bit, the
/// loop off; then the interrupt line armed at the lowest band.
pub fn claim(ix: usize, tx: u32, rx: u32) {
    let b = block(ix);
    with_cs(|| {
        m::bring_up(&mut Chip, b);
        unsafe {
            let r = addr_of_mut!(RECORD);
            write_volatile(addr_of_mut!((*r).selector), (Chip.rd(b.select_reg) >> b.select_shift) & b.select_mask);
            #[cfg(rhu_io2_supply)]
            write_volatile(addr_of_mut!((*r).supply), (Chip.rd(facts::IO2_SUPPLY.control_reg) >> facts::IO2_SUPPLY.valid_bit) & 1);
        }
    });
    if tx != crate::uart::NO_PIN {
        pads::pad_take_as(tx, b.alternate, false, g::PULL_UP, SPEED_HIGH);
        pads::pad_take_as(rx, b.alternate, false, g::PULL_UP, SPEED_LOW);
    }
    LOOP[ix % NP].store(false, Ordering::Release);
    let word = match divisor(crate::uart::DEFAULT_BAUD) {
        Some((word, _)) => word,
        None => 0,
    };
    with_cs(|| configure(ix, word, 8, m::PARITY_NONE, 1, false));
    unsafe {
        write_volatile((NVIC_IPR + b.irq) as *mut u8, IRQ_PRIO);
        write_volatile((NVIC_ICPR + 4 * (b.irq / 32)) as *mut u32, 1 << (b.irq % 32));
        write_volatile((NVIC_ISER + 4 * (b.irq / 32)) as *mut u32, 1 << (b.irq % 32));
    }
}

/// The line discipline reprogrammed by the pure module's sequence (the
/// caller let the pending transmit leave and holds the section): the block
/// disabled, the divisor, the frame, the loop as asked, the block enabled
/// and its two acknowledge flags awaited inside the bound. The registers
/// are read back into the record; a block that did not acknowledge is left
/// disabled and counted. A held break is the pad's and stands across this.
pub fn configure(ix: usize, word: u32, data_bits: u32, parity: u32, stop_bits: u32, _brk: bool) {
    let b = block(ix);
    let acked = m::configure(&mut Chip, b, written(word), data_bits, parity, stop_bits, LOOP[ix % NP].load(Ordering::Acquire), ACK_POLLS);
    MASK[ix % NP].store(m::rx_mask(data_bits), Ordering::Release);
    unsafe {
        let r = addr_of_mut!(RECORD);
        write_volatile(addr_of_mut!((*r).divisor), Chip.rd(b.base + m::BRR));
        write_volatile(addr_of_mut!((*r).prescaler), if matches!(b.law, m::Law::LowPower) { Chip.rd(b.base + m::PRESC) } else { 0 });
        write_volatile(addr_of_mut!((*r).cr1), Chip.rd(b.base + m::CR1));
        write_volatile(addr_of_mut!((*r).cr2), Chip.rd(b.base + m::CR2));
        write_volatile(addr_of_mut!((*r).cr3), Chip.rd(b.base + m::CR3));
        if !acked {
            write_volatile(addr_of_mut!((*r).acks_missed), read_volatile(addr_of_mut!((*r).acks_missed)).wrapping_add(1));
        }
    }
}

/// The interrupt line disarmed, the block disabled with its interrupts
/// off, the line's pending bit cleared, and the two pads back to plain
/// inputs without a pull (a pad held low for a break among them).
pub fn release(ix: usize, tx: u32, rx: u32) {
    let b = block(ix);
    unsafe { write_volatile((NVIC_ICER + 4 * (b.irq / 32)) as *mut u32, 1 << (b.irq % 32)) };
    with_cs(|| m::release(&mut Chip, b));
    unsafe { write_volatile((NVIC_ICPR + 4 * (b.irq / 32)) as *mut u32, 1 << (b.irq % 32)) };
    if tx != crate::uart::NO_PIN {
        pins::release(tx);
        pins::release(rx);
    }
}

/// Whether the transmitter takes a byte now; the functions ask when a byte
/// waits, and without room the transmit interrupt is turned on.
pub fn tx_room(ix: usize) -> bool {
    m::tx_room(&mut Chip, block(ix))
}

pub fn tx_put(ix: usize, b: u8) {
    m::tx_put(&mut Chip, block(ix), b)
}

/// The transmitter idle: the last byte has left the wire.
pub fn tx_idle(ix: usize) -> bool {
    m::tx_idle(&mut Chip, block(ix))
}

/// One received byte with its error word, or none.
pub fn rx_get(ix: usize) -> Option<(u8, u32)> {
    m::rx_get(&mut Chip, block(ix), MASK[ix % NP].load(Ordering::Acquire))
}

/// What was received and not read is discarded, with every error flag.
pub fn rx_flush(ix: usize) {
    m::rx_flush(&mut Chip, block(ix))
}

/// The break held or let go, by the transmit pad (the caller holds the
/// section): a plain output driven low, its data register written before
/// its mode; or the pad handed back to the block, an input with its
/// pull-up first.
pub fn set_break(ix: usize, on: bool) {
    let tx = facts::UART_ROWS[ix % NP].tx as u32;
    if on {
        pins::configure(tx, g::MODE_OUTPUT, g::PULL_UP, false);
    } else {
        pads::pad_take_as(tx, block(ix).alternate, false, g::PULL_UP, SPEED_HIGH);
    }
}

/// The acknowledge of an interrupt pass: the transmit interrupt off, a
/// standing overrun cleared and answered.
pub fn ack(ix: usize) -> u32 {
    m::ack(&mut Chip, block(ix))
}

/// The loop on or off: the block's single-wire mode, kept across the
/// configures that follow. Under the section: the block is disabled for
/// the write, and the handler writes the same control register.
pub fn loopback(ix: usize, on: bool) -> bool {
    LOOP[ix % NP].store(on, Ordering::Release);
    with_cs(|| m::set_loop(&mut Chip, block(ix), on));
    true
}

/// What the flight check reads of a port's block, from the chip as it
/// stands: whether its bus clock is enabled, whether its selector reads the
/// internal oscillator, the status register, and the divisor register, the
/// prescaler register (0 on a block without one) and the three control
/// registers (all 0 while the bus clock is off); with the record's count of
/// acknowledges missed and its supply word.
#[allow(dead_code)]
pub fn probe(ix: usize) -> (bool, bool, u32, [u32; 5], u32, u32) {
    let b = block(ix);
    let clocked = Chip.rd(b.enable_reg) & (1 << b.bus_bit) != 0;
    let selected = m::selected(&mut Chip, b);
    let (status, regs) = if clocked {
        let prescaler = if matches!(b.law, m::Law::LowPower) { Chip.rd(b.base + m::PRESC) } else { 0 };
        (Chip.rd(b.base + m::ISR), [Chip.rd(b.base + m::BRR), prescaler, Chip.rd(b.base + m::CR1), Chip.rd(b.base + m::CR2), Chip.rd(b.base + m::CR3)])
    } else {
        (0, [0; 5])
    };
    unsafe {
        let r = addr_of_mut!(RECORD);
        (clocked, selected, status, regs, read_volatile(addr_of_mut!((*r).acks_missed)), read_volatile(addr_of_mut!((*r).supply)))
    }
}

/// The block's interrupt handler: the functions' module's handler for the
/// table's first position (the line is armed only while the port is held).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __rhu_stm32_usart0_irq() {
    crate::uart::irq(0)
}
