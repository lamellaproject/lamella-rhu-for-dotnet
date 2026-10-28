//! The SD host trait: the
//! seam between a chip's SD/MMC controller and the card layer of
//! sdcard.rs. The trait names no register and no chip: the STM32F7's
//! SDMMC2 with its DMA stream (the STM32F769 Discovery board's socket) implements
//! it in board/stm32f7_sdmmc.rs, the STM32H7 boards' SDMMC with its internal DMA
//! fits under it unchanged, and the host tier's scripted host implements it
//! from a transcript so the card layer is tested without a card.

/// Microseconds, the unit of a pause the card layer asks of the host.
pub type Micros = u32;

/// The card clock's class: under 400 kHz while a card is identified, the
/// default-speed transfer clock (at or under 25 MHz) after it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Rate {
    Identification,
    Default,
}

/// The data bus width the host drives.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Width {
    One,
    Four,
}

/// The response a command expects, as the controller must be told it:
/// none (CMD0); a 48-bit short response with its CRC checked (R1, R6, R7);
/// a short response whose CRC field is not checked (R3, the OCR after
/// ACMD41 -- RM0410's 39.5: the one response type without a CRC); a short
/// response followed by the data line's busy signal (R1b: CMD7, CMD12); a
/// 136-bit long response (R2: the CID after CMD2, the CSD after CMD9, its
/// CRC inside the 127 bits and checked).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Response {
    None,
    Short,
    ShortNoCrc,
    ShortBusy,
    Long,
}

/// What a command answered: nothing; the short response's 32 bits (the
/// response's [39:8] -- the card status for R1); the long response's 128
/// bits most significant word first ([127:96] in word 0; the register's
/// CRC7 and the end bit in the low byte of word 3).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Reply {
    None,
    Short(u32),
    Long([u32; 4]),
}

/// Why the host refused, from the controller's flags: the command or data
/// timeout; a CRC failure on the command line or a data line; the data
/// line busy past its timeout; the FIFO overrun or underrun (the data path
/// lost pace); the socket empty; a caller's argument the host cannot serve
/// (a misaligned or short buffer, a count of zero).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Refusal {
    Timeout,
    Crc,
    Busy,
    Overrun,
    NoCard,
    Argument,
}

/// The seven operations of a host. The data operations move 512-byte
/// blocks through the host's own engine (the F7's DMA stream, the H747's
/// internal DMA); the buffer is the caller's, word-aligned, in memory the
/// engine reaches -- a buffer the host cannot serve is refused `Argument`.
pub trait SdHost {
    /// The controller's bring-up from reset to "powered, at the
    /// identification rate, one data line, at least 74 clocks sent with the
    /// command line high" (the power-up requirement). A host attached again
    /// starts over.
    fn attach(&mut self) -> Result<(), Refusal>;
    /// The detect line's reading: `Some(true)` with a card seated; `None`
    /// where the socket has no switch.
    fn card_present(&mut self) -> Option<bool>;
    /// The card clock's class and the bus width the host drives from now.
    fn set_bus(&mut self, rate: Rate, width: Width) -> Result<(), Refusal>;
    /// One command and its response, no data.
    fn command(&mut self, index: u8, arg: u32, response: Response) -> Result<Reply, Refusal>;
    /// A data command (17 or 18) with its transfer of `blocks` blocks into
    /// `buf`; the host returns when the controller reports the data end
    /// with the FIFO empty and every CRC checked, having sent CMD12 itself
    /// after a multiple-block command.
    fn read_blocks(&mut self, index: u8, arg: u32, blocks: u32, buf: &mut [u8]) -> Result<(), Refusal>;
    /// A data command (24 or 25) with its transfer of `blocks` blocks from
    /// `data`; the host sends CMD12 itself after a multiple-block command
    /// and returns at the data end (the card's programming state is the
    /// card layer's to wait out by CMD13).
    fn write_blocks(&mut self, index: u8, arg: u32, blocks: u32, data: &[u8]) -> Result<(), Refusal>;
    /// A pause of about `micros` with the calling thread yielded (the
    /// ACMD41 and programming-state polls sleep through it).
    fn pause(&mut self, micros: Micros);
}
