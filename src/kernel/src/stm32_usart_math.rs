//! The ST serial blocks' arithmetic, facts shape and register sequences,
//! pure: a program's serial port on the STM32F7's USART (RM0410 chapter 34)
//! and on the STM32U5's low-power UART (RM0456 chapter 67), two blocks that
//! share their register layout for everything a port uses and differ in the
//! divisor's law, in the FIFOs (the low-power block has them) and in the
//! prescaler register (the low-power block alone). Written over a two-verb
//! register seam, a read and a write at an address, so that the host tests
//! hold every sequence no emulator row models; a board's backend makes the
//! two verbs volatile accesses and adds the pads, the interrupt line and
//! the record.
//!
//! THE CLOCK of a program's port is the 16 MHz internal oscillator on both
//! chips, chosen by the block's own selector field in the clock controller
//! (RM0410 page 217; RM0456 page 597), so a port's rates do not move with
//! the row's clock posture.
//!
//! THE TWO LAWS. The USART at oversampling by 16 (RM0410 pages 1248 and
//! 1287): USARTDIV = clock / rate to nearest, 16 to 65535, the achieved
//! rate clock / USARTDIV. The low-power block (RM0456 pages 2861, 2887 and
//! 2898): LPUARTDIV = 256 x (clock / prescaler) / rate to nearest, 0x300 to
//! 0xFFFFF, the clock after the prescaler between 3 and 4096 times the
//! rate, the prescaler the smallest of the manual's twelve that fits, the
//! achieved rate 256 x (clock / prescaler) / LPUARTDIV. A rate whose
//! achieved value misses the request by more than 2 percent is refused, as
//! the other serial backends refuse it.
//!
//! THE FRAMES: 7 or 8 data bits, no parity or odd or even, 1 or 2 stop
//! bits. The blocks count the parity bit in the word length, so the word
//! is 7, 8 or 9 bits long (the two M bits of the first control register).
//! No hardware access and no state. Covered by the host tests.

/// The register seam: a 32-bit read and a 32-bit write at an address.
pub trait Regs {
    fn rd(&mut self, addr: usize) -> u32;
    fn wr(&mut self, addr: usize, value: u32);
}

/// The kernel clock of a program's port: the 16 MHz internal oscillator.
pub const CLOCK_HZ: u32 = 16_000_000;

/// The law a block's divisor follows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)] // a row's block table names one of the two
pub enum Law {
    /// The USART at oversampling by 16.
    Usart16,
    /// The low-power UART with its prescaler.
    LowPower,
}

/// A serial block as a board's facts state it for the backend: the block's
/// base and law; its bit in the clock controller's three bus registers
/// (clock enable, reset, clock while the core sleeps); its kernel clock
/// selector (the register, the field's shift and width mask, the code of
/// the 16 MHz internal oscillator); whether it has FIFOs; the alternate
/// function of its two pins; its interrupt line.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Block {
    pub base: usize,
    pub law: Law,
    pub enable_reg: usize,
    pub reset_reg: usize,
    pub sleep_reg: usize,
    pub bus_bit: u32,
    pub select_reg: usize,
    pub select_shift: u32,
    pub select_mask: u32,
    pub select_code: u32,
    pub fifo: bool,
    pub alternate: u8,
    pub irq: usize,
}

/// The control register 1 at 0x00 (RM0410 page 1276; RM0456 page 2875): UE
/// bit 0 (the block's enable), RE bit 2 and TE bit 3 (the receiver and the
/// transmitter), RXNEIE bit 5 and TXEIE bit 7 (the receive and transmit
/// interrupts; with the FIFOs on they mean "receive FIFO not empty" and
/// "transmit FIFO not full"), PS bit 9 (odd parity), PCE bit 10 (parity
/// on), the word length's low bit at bit 12 and its high bit at bit 28
/// (00 eight bits, 01 nine, 10 seven), FIFOEN bit 29 (the low-power
/// block's FIFOs).
pub const CR1: usize = 0x00;
pub const CR1_UE: u32 = 1 << 0;
pub const CR1_RE: u32 = 1 << 2;
pub const CR1_TE: u32 = 1 << 3;
pub const CR1_RXNEIE: u32 = 1 << 5;
pub const CR1_TXEIE: u32 = 1 << 7;
pub const CR1_PS: u32 = 1 << 9;
pub const CR1_PCE: u32 = 1 << 10;
pub const CR1_M0: u32 = 1 << 12;
pub const CR1_M1: u32 = 1 << 28;
pub const CR1_FIFOEN: u32 = 1 << 29;
/// The control register 2 at 0x04: the stop bits at bits 13:12, 00 one
/// stop bit and 10 two (RM0410 page 1281; RM0456 page 2883).
pub const CR2: usize = 0x04;
pub const CR2_STOP_2: u32 = 0b10 << 12;
/// The control register 3 at 0x08: HDSEL bit 3, the single-wire mode, which
/// joins the receiver to the transmit line inside the block (RM0410 page
/// 1286; RM0456 page 2885).
pub const CR3: usize = 0x08;
pub const CR3_HDSEL: u32 = 1 << 3;
/// The divisor register at 0x0C, written only while the block is disabled.
pub const BRR: usize = 0x0C;
/// The request register at 0x18: RXFRQ bit 3 discards what was received
/// and not read (RM0410 page 1290; RM0456 page 2888).
pub const RQR: usize = 0x18;
pub const RQR_RXFRQ: u32 = 1 << 3;
/// The status register at 0x1C (RM0410 page 1291; RM0456 page 2888): PE
/// bit 0, FE bit 1, NF bit 2 and ORE bit 3 (parity, framing, noise,
/// overrun), RXNE bit 5 (a byte to read), TC bit 6 (the transmitter idle),
/// TXE bit 7 (room for a byte), TEACK bit 21 and REACK bit 22 (the
/// transmitter's and the receiver's acknowledge after the enable).
pub const ISR: usize = 0x1C;
pub const ISR_PE: u32 = 1 << 0;
pub const ISR_FE: u32 = 1 << 1;
pub const ISR_NF: u32 = 1 << 2;
pub const ISR_ORE: u32 = 1 << 3;
pub const ISR_RXNE: u32 = 1 << 5;
pub const ISR_TC: u32 = 1 << 6;
pub const ISR_TXE: u32 = 1 << 7;
pub const ISR_TEACK: u32 = 1 << 21;
pub const ISR_REACK: u32 = 1 << 22;
/// The flag clear register at 0x20: a 1 clears the flag at the same bit of
/// the status register -- parity, framing, noise and overrun at bits 0 to
/// 3 (RM0410 page 1296; RM0456 page 2896).
pub const ICR: usize = 0x20;
/// The receive and transmit data registers at 0x24 and 0x28.
pub const RDR: usize = 0x24;
pub const TDR: usize = 0x28;
/// The low-power block's prescaler register at 0x2C, its code at bits 3:0
/// (RM0456 page 2898), written only while the block is disabled.
pub const PRESC: usize = 0x2C;

/// The low-power block's twelve prescalers, by code (RM0456 page 2898).
pub const PRESCALERS: [u32; 12] = [1, 2, 4, 6, 8, 10, 12, 16, 32, 64, 128, 256];

/// The error word of a received byte, as the serial functions' module
/// counts it, and the acknowledge's overrun word.
pub const ERR_FRAMING: u32 = 1 << 0;
pub const ERR_PARITY: u32 = 1 << 1;
pub const ERR_BREAK: u32 = 1 << 2;
pub const ACK_OVERRUN: u32 = 1 << 0;

/// The serial functions' parity codes this frame set takes.
pub const PARITY_NONE: u32 = 0;
pub const PARITY_ODD: u32 = 1;
pub const PARITY_EVEN: u32 = 2;

/// Whether an achieved rate is within 2 percent of the rate asked.
pub const fn within_2_percent(asked: u32, got: u32) -> bool {
    let d = if got > asked { got - asked } else { asked - got };
    d as u64 * 50 <= asked as u64
}

/// The USART's divisor at oversampling by 16: the register word (USARTDIV
/// itself) and the achieved rate, or none for a rate outside the block's
/// reach.
pub const fn divisor_usart16(clock: u32, baud: u32) -> Option<(u32, u32)> {
    if baud == 0 {
        return None;
    }
    let div = (clock as u64 + baud as u64 / 2) / baud as u64;
    if div < 16 || div > 65_535 {
        return None;
    }
    let got = (clock as u64 / div) as u32;
    if within_2_percent(baud, got) { Some((div as u32, got)) } else { None }
}

/// The low-power block's divisor: the register word (LPUARTDIV in bits
/// 19:0 and the prescaler's code in bits 23:20) and the achieved rate, or
/// none for a rate outside the block's reach. The smallest prescaler that fits
/// is taken, so the divisor is as long as the register allows.
pub const fn divisor_low_power(clock: u32, baud: u32) -> Option<(u32, u32)> {
    if baud == 0 {
        return None;
    }
    let mut code = 0;
    while code < PRESCALERS.len() {
        let ck = (clock / PRESCALERS[code]) as u64;
        if ck >= 3 * baud as u64 && ck <= 4096 * baud as u64 {
            let div = (256 * ck + baud as u64 / 2) / baud as u64;
            if div >= 0x300 && div <= 0xF_FFFF {
                let got = (256 * ck / div) as u32;
                return if within_2_percent(baud, got) { Some(((code as u32) << 20 | div as u32, got)) } else { None };
            }
        }
        code += 1;
    }
    None
}

/// A block's divisor by its law at the port's clock.
pub const fn divisor(law: Law, baud: u32) -> Option<(u32, u32)> {
    match law {
        Law::Usart16 => divisor_usart16(CLOCK_HZ, baud),
        Law::LowPower => divisor_low_power(CLOCK_HZ, baud),
    }
}

/// The frame set of the two blocks inside the serial functions' vocabulary.
pub const fn frame_ok(data_bits: u32, parity: u32, stop_bits: u32) -> bool {
    (data_bits == 7 || data_bits == 8) && parity <= PARITY_EVEN && (stop_bits == 1 || stop_bits == 2)
}

/// The first control register's frame fields: the word length counts the
/// parity bit, so 7 data bits alone are a 7-bit word (the high bit), 8 with
/// parity a 9-bit word (the low bit), and 7 with parity or 8 alone the
/// 8-bit word (neither bit).
pub const fn cr1_frame(data_bits: u32, parity: u32) -> u32 {
    let word = data_bits + if parity != PARITY_NONE { 1 } else { 0 };
    let m = if word == 7 {
        CR1_M1
    } else if word == 9 {
        CR1_M0
    } else {
        0
    };
    let p = if parity == PARITY_ODD {
        CR1_PCE | CR1_PS
    } else if parity == PARITY_EVEN {
        CR1_PCE
    } else {
        0
    };
    m | p
}

/// The second control register's stop field.
pub const fn cr2_stop(stop_bits: u32) -> u32 {
    if stop_bits == 2 { CR2_STOP_2 } else { 0 }
}

/// The mask of a frame's data bits in the receive data register (the
/// register carries the parity bit above them).
pub const fn rx_mask(data_bits: u32) -> u32 {
    (1 << data_bits) - 1
}

/// THE BRING-UP of a block at a claim: its bus clock enabled and read back;
/// the block reset through its reset bit; its kernel clock selector written
/// to the internal oscillator with every other field of that register
/// kept; its clock while the core sleeps. The block is left disabled.
pub fn bring_up<R: Regs>(r: &mut R, b: &Block) {
    let bit = 1 << b.bus_bit;
    let v = r.rd(b.enable_reg);
    r.wr(b.enable_reg, v | bit);
    let _ = r.rd(b.enable_reg);
    let v = r.rd(b.reset_reg);
    r.wr(b.reset_reg, v | bit);
    r.wr(b.reset_reg, v & !bit);
    let v = r.rd(b.select_reg);
    r.wr(b.select_reg, (v & !(b.select_mask << b.select_shift)) | (b.select_code << b.select_shift));
    let v = r.rd(b.sleep_reg);
    r.wr(b.sleep_reg, v | bit);
}

/// Whether a block's selector field reads the internal oscillator's code.
pub fn selected<R: Regs>(r: &mut R, b: &Block) -> bool {
    (r.rd(b.select_reg) >> b.select_shift) & b.select_mask == b.select_code
}

/// CONFIGURE: the block disabled with its interrupts off; the divisor (and
/// the low-power block's prescaler); the stop field; the single-wire mode
/// for the loop; the frame fields with the transmitter, the receiver, the
/// receive interrupt and the FIFOs written before the enable (the FIFO bit
/// is writable only while the block is disabled); the block enabled; both
/// acknowledge flags awaited for `polls` reads of the status register. A
/// block that never acknowledges is disabled again and answers false.
pub fn configure<R: Regs>(r: &mut R, b: &Block, word: u32, data_bits: u32, parity: u32, stop_bits: u32, loop_on: bool, polls: u32) -> bool {
    r.wr(b.base + CR1, 0);
    r.wr(b.base + BRR, word & 0xF_FFFF);
    if b.law == Law::LowPower {
        r.wr(b.base + PRESC, (word >> 20) & 0xF);
    }
    r.wr(b.base + CR2, cr2_stop(stop_bits));
    r.wr(b.base + CR3, if loop_on { CR3_HDSEL } else { 0 });
    let cr1 = cr1_frame(data_bits, parity) | CR1_TE | CR1_RE | CR1_RXNEIE | if b.fifo { CR1_FIFOEN } else { 0 };
    r.wr(b.base + CR1, cr1);
    r.wr(b.base + CR1, cr1 | CR1_UE);
    let both = ISR_TEACK | ISR_REACK;
    let mut n = 0;
    while n < polls {
        if r.rd(b.base + ISR) & both == both {
            return true;
        }
        n += 1;
    }
    r.wr(b.base + CR1, 0);
    false
}

/// The single-wire mode turned on or off on a configured block: the block
/// disabled for the write (the bit is writable only then) and put back as
/// it was.
pub fn set_loop<R: Regs>(r: &mut R, b: &Block, on: bool) {
    let cr1 = r.rd(b.base + CR1);
    r.wr(b.base + CR1, cr1 & !CR1_UE);
    let cr3 = r.rd(b.base + CR3);
    r.wr(b.base + CR3, if on { cr3 | CR3_HDSEL } else { cr3 & !CR3_HDSEL });
    r.wr(b.base + CR1, cr1);
}

/// The acknowledge of an interrupt pass. The transmit interrupt is a level
/// on these blocks (it stands for as long as there is room for a byte), so
/// it is turned off here and turned on again by each byte put, and by a
/// byte that finds no room (`tx_room`): a pass with nothing to send leaves
/// it off. An overrun is cleared and answered.
pub fn ack<R: Regs>(r: &mut R, b: &Block) -> u32 {
    let cr1 = r.rd(b.base + CR1);
    r.wr(b.base + CR1, cr1 & !CR1_TXEIE);
    if r.rd(b.base + ISR) & ISR_ORE != 0 {
        r.wr(b.base + ICR, ISR_ORE);
        ACK_OVERRUN
    } else {
        0
    }
}

/// Whether the transmitter has room for a byte, asked when a byte waits.
/// Without room the transmit interrupt is turned on, so that the pass
/// which could not send is followed by one when room is made: the
/// acknowledge turns that interrupt off, and a pass raised by a received
/// byte while the data register was full would otherwise leave the waiting
/// bytes with no interrupt to send them.
pub fn tx_room<R: Regs>(r: &mut R, b: &Block) -> bool {
    if r.rd(b.base + ISR) & ISR_TXE != 0 {
        return true;
    }
    let cr1 = r.rd(b.base + CR1);
    r.wr(b.base + CR1, cr1 | CR1_TXEIE);
    false
}

/// A byte put: the data register written and the transmit interrupt on.
pub fn tx_put<R: Regs>(r: &mut R, b: &Block, byte: u8) {
    r.wr(b.base + TDR, byte as u32);
    let cr1 = r.rd(b.base + CR1);
    r.wr(b.base + CR1, cr1 | CR1_TXEIE);
}

/// Whether the last byte has left the wire.
pub fn tx_idle<R: Regs>(r: &mut R, b: &Block) -> bool {
    r.rd(b.base + ISR) & ISR_TC != 0
}

/// One received byte with its error word, or none: the status read with
/// the byte, the byte masked to the frame's data bits, the parity, framing
/// and noise flags cleared. The three flags are those of the byte waiting
/// in the data register (RM0456 page 2892: with the FIFOs on, each error
/// "is associated with the character" in that register), so they are
/// cleared BEFORE the byte is taken: cleared after it, the write would
/// clear the flags of the byte behind it. A framing error with a zero byte
/// is a break. An overrun is not this byte's fault (the byte read is
/// sound; a later one was lost) and is left to the acknowledge.
pub fn rx_get<R: Regs>(r: &mut R, b: &Block, mask: u32) -> Option<(u8, u32)> {
    let isr = r.rd(b.base + ISR);
    if isr & ISR_RXNE == 0 {
        return None;
    }
    let flags = isr & (ISR_PE | ISR_FE | ISR_NF);
    if flags != 0 {
        r.wr(b.base + ICR, flags);
    }
    let data = r.rd(b.base + RDR) & mask;
    let mut err = 0;
    if isr & ISR_FE != 0 {
        err |= if data == 0 { ERR_BREAK } else { ERR_FRAMING };
    }
    if isr & ISR_PE != 0 {
        err |= ERR_PARITY;
    }
    Some((data as u8, err))
}

/// What was received and not read is discarded, with every error flag.
pub fn rx_flush<R: Regs>(r: &mut R, b: &Block) {
    r.wr(b.base + RQR, RQR_RXFRQ);
    r.wr(b.base + ICR, ISR_PE | ISR_FE | ISR_NF | ISR_ORE);
}

/// THE RELEASE: the block disabled with its interrupts off. Its bus clock
/// stays on, so its registers stay readable.
pub fn release<R: Regs>(r: &mut R, b: &Block) {
    r.wr(b.base + CR1, 0);
}

/// Whether a block is enabled.
pub fn is_up<R: Regs>(r: &mut R, b: &Block) -> bool {
    r.rd(b.base + CR1) & CR1_UE != 0
}

/// The low-power block's FIFOs, each 8 deep (RM0456 Table 686 on page
/// 2847).
pub const LOW_POWER_FIFO: u32 = 8;

/// The transmitter's share of a port's pending count: the FIFO's depth, or
/// the one data register of a block without FIFOs.
pub const fn fifo_depth(b: &Block) -> u32 {
    if b.fifo { LOW_POWER_FIFO } else { 1 }
}

/// Whether a block's record is sound: every address a word's, the bus bit
/// inside its register, the selector's field inside its register and the
/// oscillator's code inside the field, FIFOs on the low-power block alone,
/// the alternate function a pad's four bits.
pub const fn block_sound(b: &Block) -> bool {
    let words = b.base != 0 && b.base % 4 == 0 && b.enable_reg % 4 == 0 && b.reset_reg % 4 == 0 && b.sleep_reg % 4 == 0 && b.select_reg % 4 == 0;
    let field = b.bus_bit < 32 && b.select_mask != 0 && b.select_shift < 32 && (b.select_mask << b.select_shift) >> b.select_shift == b.select_mask && b.select_code <= b.select_mask;
    let fifo = !b.fifo || matches!(b.law, Law::LowPower);
    words && field && fifo && b.alternate < 16
}

/// Whether a row's block table fits its port table: one block a port, each
/// sound, one law for the table (a row's rate rule is one), no two blocks
/// of one base or one interrupt line.
pub const fn blocks_fit(ports: usize, blocks: &[Block]) -> bool {
    if blocks.len() != ports {
        return false;
    }
    let mut i = 0;
    while i < blocks.len() {
        if !block_sound(&blocks[i]) || !matches!((blocks[i].law, blocks[0].law), (Law::Usart16, Law::Usart16) | (Law::LowPower, Law::LowPower)) {
            return false;
        }
        let mut j = 0;
        while j < i {
            if blocks[j].base == blocks[i].base || blocks[j].irq == blocks[i].irq {
                return false;
            }
            j += 1;
        }
        i += 1;
    }
    true
}

/// Whether a port's two lines are sound: two different lines.
pub const fn pair_sound(tx: u32, rx: u32) -> bool {
    tx != rx
}

/// Whether neither of a port's two lines is among the board's own claims,
/// by the claims' bitmap (a bit a line, 32 lines a word).
pub const fn lines_free(board: &[u32], tx: u32, rx: u32) -> bool {
    (board[(tx / 32) as usize] >> (tx % 32)) & 1 == 0 && (board[(rx / 32) as usize] >> (rx % 32)) & 1 == 0
}
