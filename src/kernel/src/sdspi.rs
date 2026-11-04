//! The SD memory card's SPI mode as a host for the card layer of sdcard.rs.
//! The card layer speaks the native bus's steps -- CMD2, CMD3, CMD7 and
//! ACMD6, the R1 status word, the CID and CSD as 128-bit responses -- and
//! this host answers them from the SPI mode: the one-bit bus of a chip
//! select, a clock and two data lines that a plain SPI block drives, where a
//! command is a six-byte frame, a response begins with the R1 byte, a data
//! block follows a start token and ends in its CRC16, and a write is
//! acknowledged by a data response and a busy signal. Pure: no register, no
//! chip, no clock of its own. Every wait is counted in bytes clocked, the
//! SPI mode's own unit -- the card answers within eight bytes, starts a
//! block within its read time, holds the data line low while it programs --
//! so the host runs before the kernel's tick as well as under the scheduler.
//! The wire (a chip's SPI block with the select, the detect line and the
//! rate ladder) sits behind the `SpiWire` trait: board/rp2350_sdspi.rs
//! implements it over an SPI block of the RP2350 for the Adafruit Metro
//! RP2350 boards' and the SparkFun IoT RedBoard RP2350's microSD sockets,
//! and the host tier's scripted wire
//! implements it from a byte transcript, so the translation is proven on
//! the development machine before a card sees it. The SPI mode's frames are
//! this module's own tables: the six-byte command with its CRC7, the R1, R2,
//! R3 and R7 responses, the start, stop and error tokens, the data response
//! and the busy signal, the 74 clocks with the select high before the first
//! command, CMD58 for the OCR and the stuff byte after CMD12, as the
//! translation below states them; the command numbers are sdcard.rs's.

#![allow(dead_code)] // the host tier exercises the whole surface (every token and status bit); each kernel image uses what its socket needs

use crate::sdcard::{
    self, ACMD41_HCS, BLOCK, CMD0_GO_IDLE_STATE, CMD12_STOP_TRANSMISSION, CMD13_SEND_STATUS, CMD16_SET_BLOCKLEN, CMD17_READ_SINGLE_BLOCK, CMD18_READ_MULTIPLE_BLOCK,
    CMD24_WRITE_BLOCK, CMD25_WRITE_MULTIPLE_BLOCK, CMD2_ALL_SEND_CID, CMD3_SEND_RELATIVE_ADDR, CMD41_SD_SEND_OP_COND, CMD55_APP_CMD, CMD6_SET_BUS_WIDTH,
    CMD7_SELECT_CARD, CMD8_SEND_IF_COND, CMD9_SEND_CSD, STATE_TRAN, STATUS_ADDRESS_ERROR, STATUS_CARD_ECC_FAILED, STATUS_CC_ERROR, STATUS_COM_CRC_ERROR,
    STATUS_ERROR, STATUS_ILLEGAL_COMMAND, STATUS_OUT_OF_RANGE, STATUS_READY_FOR_DATA, STATUS_WP_VIOLATION,
};
use crate::sdhost::{Micros, Rate, Refusal, Reply, Response, SdHost, Width};

/// The SPI mode's own commands: CMD10 reads the CID as a data block, CMD58
/// reads the OCR as R3.
pub const CMD10_SEND_CID: u8 = 10;
pub const CMD58_READ_OCR: u8 = 58;

/// The bytes a card may take before its R1 byte (the command response
/// time): a wait of eight bytes, then the native timeout.
pub const R1_BYTES: u32 = 8;
/// The clocks before CMD0 with the select high: ten bytes, 80 clocks (the
/// card needs 74).
pub const POWER_UP_BYTES: usize = 10;
/// The wait for a block's start token: 100 ms of clocks at the rate in force
/// (the card's read time). The wait for a write's busy signal to end: 250 ms
/// per block (the write time). The wait after CMD12: two seconds (the card
/// programming what a multiple-block write left buffered).
pub const READ_TIMEOUT_MS: u32 = 100;
pub const WRITE_TIMEOUT_MS: u32 = 250;
pub const STOP_BUSY_MS: u32 = 2_000;
/// The polls of a long wait between two yields of the calling thread.
pub const BREATH_BYTES: u32 = 16;

/// The data tokens: the start of a single block, of each block a multiple
/// read sends and of CMD24's data; the start of each block of a multiple
/// write; the stop token after a multiple write's last block.
pub const TOKEN_START: u8 = 0xFE;
pub const TOKEN_START_MULTI: u8 = 0xFC;
pub const TOKEN_STOP: u8 = 0xFD;
/// The data response's three forms in its low five bits: accepted, refused
/// for its CRC, refused as a write error.
pub const DATA_ACCEPTED: u8 = 0x05;
pub const DATA_CRC_ERROR: u8 = 0x0B;
pub const DATA_WRITE_ERROR: u8 = 0x0D;
/// The byte read while the card holds the data line low (busy), and the
/// byte read when nothing drives the line.
pub const BUSY: u8 = 0x00;
pub const IDLE_LINE: u8 = 0xFF;

/// R1's bits.
pub const R1_IDLE: u8 = 1 << 0;
pub const R1_ERASE_RESET: u8 = 1 << 1;
pub const R1_ILLEGAL_COMMAND: u8 = 1 << 2;
pub const R1_COM_CRC_ERROR: u8 = 1 << 3;
pub const R1_ERASE_SEQUENCE_ERROR: u8 = 1 << 4;
pub const R1_ADDRESS_ERROR: u8 = 1 << 5;
pub const R1_PARAMETER_ERROR: u8 = 1 << 6;
pub const R1_ERROR_BITS: u8 = R1_ILLEGAL_COMMAND | R1_COM_CRC_ERROR | R1_ERASE_SEQUENCE_ERROR | R1_ADDRESS_ERROR | R1_PARAMETER_ERROR;
/// R2's second byte (CMD13).
pub const R2_CARD_LOCKED: u8 = 1 << 0;
pub const R2_WP_ERASE_SKIP: u8 = 1 << 1;
pub const R2_ERROR: u8 = 1 << 2;
pub const R2_CC_ERROR: u8 = 1 << 3;
pub const R2_CARD_ECC_FAILED: u8 = 1 << 4;
pub const R2_WP_VIOLATION: u8 = 1 << 5;
pub const R2_ERASE_PARAM: u8 = 1 << 6;
pub const R2_OUT_OF_RANGE: u8 = 1 << 7;
/// The data error token (a byte 000xxxxx in a start token's place).
pub const ERROR_TOKEN_ERROR: u8 = 1 << 0;
pub const ERROR_TOKEN_CC_ERROR: u8 = 1 << 1;
pub const ERROR_TOKEN_CARD_ECC_FAILED: u8 = 1 << 2;
pub const ERROR_TOKEN_OUT_OF_RANGE: u8 = 1 << 3;
/// The relative card address this host answers CMD3 with (the SPI mode has
/// none; the card layer carries it in the arguments of commands this host
/// does not send).
pub const RCA: u16 = 1;

/// A chip's SPI block as the card host's wire: the select and the detect
/// line as plain lines, the rate off the chip's own ladder, the bytes, a
/// pause and a yield point, and the witness of a refusal.
pub trait SpiWire {
    /// The wire's lines and block taken (a host attached again takes them
    /// again).
    fn attach(&mut self) -> Result<(), Refusal>;
    /// The detect line's reading: `Some(true)` with a card seated; `None`
    /// where the socket has no switch.
    fn card_present(&mut self) -> Option<bool>;
    /// The clock rate class from now; the rate achieved, in Hz, which the
    /// host counts its byte budgets at.
    fn rate(&mut self, rate: Rate) -> Result<u32, Refusal>;
    /// The chip select: `low` asserts it.
    fn select(&mut self, low: bool);
    /// The bytes sent; what the card returned meanwhile is discarded.
    fn write(&mut self, tx: &[u8]) -> Result<(), Refusal>;
    /// `rx.len()` bytes clocked in with the output line high.
    fn read(&mut self, rx: &mut [u8]) -> Result<(), Refusal>;
    /// A pause of about `micros` with the select high (the ACMD41 polls).
    fn pause(&mut self, micros: Micros);
    /// A point in a long wait where the calling thread may be yielded.
    fn breathe(&mut self);
    /// A refusal's witness: the last R1 byte and the last token the host read.
    fn fault(&mut self, r1: u8, token: u8);
}

/// The host: the card layer's `SdHost` over a wire.
pub struct SpiHost<W: SpiWire> {
    pub wire: W,
    /// The rate in force, in Hz (the byte budgets are counted at it).
    hz: u32,
    /// The last R1 byte and the last token read, the witness at a refusal.
    r1: u8,
    token: u8,
}

/// The six-byte command frame: the start and transmission bits with the
/// index, the argument, the CRC7 with the end bit.
pub fn frame(index: u8, arg: u32) -> [u8; 6] {
    [0x40 | (index & 0x3F), (arg >> 24) as u8, (arg >> 16) as u8, (arg >> 8) as u8, arg as u8, sdcard::frame_crc_byte(index, arg)]
}

/// An R1 byte as the native status word the card layer reads.
pub const fn status_of_r1(r1: u8) -> u32 {
    let mut s = 0;
    if r1 & R1_ILLEGAL_COMMAND != 0 {
        s |= STATUS_ILLEGAL_COMMAND;
    }
    if r1 & R1_COM_CRC_ERROR != 0 {
        s |= STATUS_COM_CRC_ERROR;
    }
    if r1 & R1_ADDRESS_ERROR != 0 {
        s |= STATUS_ADDRESS_ERROR;
    }
    if r1 & R1_PARAMETER_ERROR != 0 {
        s |= STATUS_OUT_OF_RANGE;
    }
    if r1 & R1_ERASE_SEQUENCE_ERROR != 0 {
        s |= STATUS_ERROR;
    }
    s
}

/// CMD13's R2 (the R1 byte and the second byte) as the native status word:
/// the error bits mapped, the transfer state with ready-for-data set.
pub const fn status_of_r2(r1: u8, r2: u8) -> u32 {
    let mut s = status_of_r1(r1) | (STATE_TRAN << 9) | STATUS_READY_FOR_DATA;
    if r2 & R2_ERROR != 0 {
        s |= STATUS_ERROR;
    }
    if r2 & R2_CC_ERROR != 0 {
        s |= STATUS_CC_ERROR;
    }
    if r2 & R2_CARD_ECC_FAILED != 0 {
        s |= STATUS_CARD_ECC_FAILED;
    }
    if r2 & R2_WP_VIOLATION != 0 {
        s |= STATUS_WP_VIOLATION;
    }
    if r2 & R2_OUT_OF_RANGE != 0 {
        s |= STATUS_OUT_OF_RANGE;
    }
    s
}

/// Four bytes most significant first as a word.
const fn word(b: &[u8]) -> u32 {
    ((b[0] as u32) << 24) | ((b[1] as u32) << 16) | ((b[2] as u32) << 8) | b[3] as u32
}

impl<W: SpiWire> SpiHost<W> {
    pub const fn new(wire: W) -> Self {
        SpiHost { wire, hz: 0, r1: IDLE_LINE, token: IDLE_LINE }
    }

    /// A refusal, with the witness words handed to the wire first.
    fn fail(&mut self, r: Refusal) -> Refusal {
        self.wire.fault(self.r1, self.token);
        r
    }

    /// `ms` of clocks at the rate in force, in bytes (never under the R1 wait).
    fn budget(&self, ms: u32) -> u32 {
        let bytes = self.hz as u64 * ms as u64 / 8_000;
        if bytes < R1_BYTES as u64 { R1_BYTES } else { bytes as u32 }
    }

    fn byte(&mut self) -> Result<u8, Refusal> {
        let mut b = [IDLE_LINE];
        self.wire.read(&mut b).map_err(|r| self.fail(r))?;
        Ok(b[0])
    }

    /// The select asserted, the frame sent (CMD12's stuff byte skipped), the
    /// R1 byte awaited within its eight bytes. The select stays asserted for
    /// the caller's response or data.
    fn send(&mut self, index: u8, arg: u32) -> Result<u8, Refusal> {
        self.wire.select(true);
        let f = frame(index, arg);
        self.wire.write(&f).map_err(|r| self.fail(r))?;
        if index == CMD12_STOP_TRANSMISSION {
            self.byte()?;
        }
        for _ in 0..R1_BYTES {
            let b = self.byte()?;
            if b & 0x80 == 0 {
                self.r1 = b;
                return Ok(b);
            }
        }
        self.r1 = IDLE_LINE;
        Err(self.fail(Refusal::Timeout))
    }

    /// The select released and eight clocks sent, which complete the command
    /// on the card's side.
    fn end(&mut self) {
        self.wire.select(false);
        let _ = self.wire.write(&[IDLE_LINE]);
    }

    /// The command and its R1 byte, ended.
    fn simple(&mut self, index: u8, arg: u32) -> Result<u8, Refusal> {
        let r = self.send(index, arg);
        self.end();
        r
    }

    /// A byte that is not the idle line awaited within `budget` bytes.
    fn await_token(&mut self, budget: u32) -> Result<u8, Refusal> {
        for n in 0..budget {
            let b = self.byte()?;
            if b != IDLE_LINE {
                self.token = b;
                return Ok(b);
            }
            if n % BREATH_BYTES == BREATH_BYTES - 1 {
                self.wire.breathe();
            }
        }
        Err(self.fail(Refusal::Timeout))
    }

    /// The busy signal's end (the idle line read) awaited within `budget`
    /// bytes.
    fn await_idle(&mut self, budget: u32) -> Result<(), Refusal> {
        for n in 0..budget {
            if self.byte()? == IDLE_LINE {
                return Ok(());
            }
            if n % BREATH_BYTES == BREATH_BYTES - 1 {
                self.wire.breathe();
            }
        }
        Err(self.fail(Refusal::Busy))
    }

    /// A data block into `buf`: the start token within the read time (an
    /// error token refuses: the card's ECC failure as a CRC refusal, which the
    /// card layer retries once; the rest as the timeout), the bytes, the CRC16
    /// checked.
    fn block_in(&mut self, buf: &mut [u8]) -> Result<(), Refusal> {
        let budget = self.budget(READ_TIMEOUT_MS);
        let token = self.await_token(budget)?;
        if token != TOKEN_START {
            let r = if token & 0xE0 == 0 && token & ERROR_TOKEN_CARD_ECC_FAILED != 0 { Refusal::Crc } else { Refusal::Timeout };
            return Err(self.fail(r));
        }
        self.wire.read(buf).map_err(|r| self.fail(r))?;
        let mut crc = [0u8; 2];
        self.wire.read(&mut crc).map_err(|r| self.fail(r))?;
        if ((crc[0] as u16) << 8 | crc[1] as u16) != sdcard::crc16(buf) {
            return Err(self.fail(Refusal::Crc));
        }
        Ok(())
    }

    /// A data block out: the token, the bytes, their CRC16, then the data
    /// response within eight bytes (accepted; refused for its CRC, which the
    /// card layer retries once; a write error as the timeout, the card's
    /// reason read by CMD13 after) and the busy signal's end within the write
    /// time.
    fn block_out(&mut self, token: u8, data: &[u8]) -> Result<(), Refusal> {
        self.wire.write(&[token]).map_err(|r| self.fail(r))?;
        self.wire.write(data).map_err(|r| self.fail(r))?;
        let crc = sdcard::crc16(data);
        self.wire.write(&[(crc >> 8) as u8, crc as u8]).map_err(|r| self.fail(r))?;
        let mut response = IDLE_LINE;
        for _ in 0..R1_BYTES {
            let b = self.byte()?;
            if b & 0x11 == 0x01 {
                response = b;
                break;
            }
        }
        self.token = response;
        match response & 0x1F {
            DATA_ACCEPTED => {}
            DATA_CRC_ERROR => return Err(self.fail(Refusal::Crc)),
            _ => return Err(self.fail(Refusal::Timeout)),
        }
        let budget = self.budget(WRITE_TIMEOUT_MS);
        self.await_idle(budget)
    }

    /// A 16-byte register (the CID by CMD10, the CSD by CMD9) as the long
    /// reply, most significant word first.
    fn register(&mut self, index: u8) -> Result<Reply, Refusal> {
        let r = self.send(index, 0).and_then(|r1| {
            if r1 & R1_ERROR_BITS != 0 {
                return Err(self.fail(Refusal::Timeout));
            }
            let mut reg = [0u8; 16];
            self.block_in(&mut reg)?;
            Ok(Reply::Long([word(&reg[0..4]), word(&reg[4..8]), word(&reg[8..12]), word(&reg[12..16])]))
        });
        self.end();
        r
    }

    /// Four response bytes after R1 (R3's OCR, R7's echo) as the short reply.
    fn four(&mut self) -> Result<Reply, Refusal> {
        let mut b = [0u8; 4];
        self.wire.read(&mut b).map_err(|r| self.fail(r))?;
        Ok(Reply::Short(word(&b)))
    }

    /// An R1 byte with an error bit on a data command: the address and
    /// parameter errors as the host's own argument refusal (the card layer
    /// bounds every address, so neither is expected), the rest as the timeout.
    fn data_r1(&mut self, r1: u8) -> Result<(), Refusal> {
        if r1 & R1_ERROR_BITS == 0 {
            Ok(())
        } else if r1 & (R1_ADDRESS_ERROR | R1_PARAMETER_ERROR) != 0 {
            Err(self.fail(Refusal::Argument))
        } else {
            Err(self.fail(Refusal::Timeout))
        }
    }

    /// A command sent for its R1 byte alone (CMD16 and any other the card
    /// layer may send), answered as the native status word.
    fn plain(&mut self, index: u8, arg: u32) -> Result<Reply, Refusal> {
        let r1 = self.simple(index, arg)?;
        Ok(Reply::Short(status_of_r1(r1)))
    }

    fn blocks_in(&mut self, index: u8, arg: u32, blocks: u32, buf: &mut [u8]) -> Result<(), Refusal> {
        let r1 = self.send(index, arg)?;
        self.data_r1(r1)?;
        for k in 0..blocks as usize {
            self.block_in(&mut buf[k * BLOCK..(k + 1) * BLOCK])?;
        }
        if index == CMD18_READ_MULTIPLE_BLOCK {
            self.end();
            let r1 = self.send(CMD12_STOP_TRANSMISSION, 0)?;
            self.data_r1(r1)?;
            let budget = self.budget(STOP_BUSY_MS);
            self.await_idle(budget)?;
        }
        Ok(())
    }

    fn blocks_out(&mut self, index: u8, arg: u32, blocks: u32, data: &[u8]) -> Result<(), Refusal> {
        let r1 = self.send(index, arg)?;
        self.data_r1(r1)?;
        if index == CMD24_WRITE_BLOCK {
            return self.block_out(TOKEN_START, &data[..BLOCK]);
        }
        for k in 0..blocks as usize {
            self.block_out(TOKEN_START_MULTI, &data[k * BLOCK..(k + 1) * BLOCK])?;
        }
        self.wire.write(&[TOKEN_STOP]).map_err(|r| self.fail(r))?;
        self.byte()?;
        let budget = self.budget(WRITE_TIMEOUT_MS);
        self.await_idle(budget)
    }
}

impl<W: SpiWire> SdHost for SpiHost<W> {
    /// The wire taken, the identification rate, the select high, the
    /// power-up clocks.
    fn attach(&mut self) -> Result<(), Refusal> {
        self.wire.attach()?;
        self.hz = self.wire.rate(Rate::Identification)?;
        self.wire.select(false);
        self.wire.write(&[IDLE_LINE; POWER_UP_BYTES]).map_err(|r| self.fail(r))
    }

    fn card_present(&mut self) -> Option<bool> {
        self.wire.card_present()
    }

    /// The rate from the wire's ladder; the SPI mode has one data line, so a
    /// width of four is refused as an argument (the card layer never asks it:
    /// ACMD6 is answered illegal).
    fn set_bus(&mut self, rate: Rate, width: Width) -> Result<(), Refusal> {
        if width == Width::Four {
            return Err(Refusal::Argument);
        }
        self.hz = self.wire.rate(rate)?;
        Ok(())
    }

    /// The translation (the module's note); the data commands belong to the
    /// data path and are refused here as an argument.
    fn command(&mut self, index: u8, arg: u32, _response: Response) -> Result<Reply, Refusal> {
        match index {
            CMD0_GO_IDLE_STATE => {
                self.simple(index, arg)?;
                Ok(Reply::None)
            }
            CMD8_SEND_IF_COND => {
                let r = self.send(index, arg).and_then(|r1| {
                    if r1 & R1_ILLEGAL_COMMAND != 0 {
                        return Err(self.fail(Refusal::Timeout));
                    }
                    if r1 & R1_ERROR_BITS != 0 {
                        return Err(self.fail(Refusal::Crc));
                    }
                    self.four()
                });
                self.end();
                r
            }
            CMD55_APP_CMD => {
                let r1 = self.simple(index, arg)?;
                if r1 & R1_ILLEGAL_COMMAND != 0 {
                    return Err(self.fail(Refusal::Timeout));
                }
                Ok(Reply::Short(status_of_r1(r1)))
            }
            CMD41_SD_SEND_OP_COND => {
                let r1 = self.simple(index, arg & ACMD41_HCS)?;
                if r1 & R1_ILLEGAL_COMMAND != 0 {
                    return Err(self.fail(Refusal::Timeout));
                }
                if r1 & (R1_IDLE | R1_ERROR_BITS) != 0 {
                    return Ok(Reply::Short(0));
                }
                let r = self.send(CMD58_READ_OCR, 0).and_then(|r1| {
                    if r1 & R1_ERROR_BITS != 0 {
                        return Err(self.fail(Refusal::Timeout));
                    }
                    self.four()
                });
                self.end();
                r
            }
            CMD2_ALL_SEND_CID => self.register(CMD10_SEND_CID),
            CMD9_SEND_CSD => self.register(CMD9_SEND_CSD),
            CMD3_SEND_RELATIVE_ADDR => Ok(Reply::Short((RCA as u32) << 16)),
            CMD7_SELECT_CARD => Ok(Reply::Short(0)),
            CMD6_SET_BUS_WIDTH => Ok(Reply::Short(STATUS_ILLEGAL_COMMAND)),
            CMD13_SEND_STATUS => {
                let r = self.send(index, 0).and_then(|r1| {
                    let r2 = self.byte()?;
                    Ok(Reply::Short(status_of_r2(r1, r2)))
                });
                self.end();
                r
            }
            CMD12_STOP_TRANSMISSION => {
                let r = self.send(index, 0).and_then(|r1| {
                    let budget = self.budget(STOP_BUSY_MS);
                    self.await_idle(budget)?;
                    Ok(Reply::Short(status_of_r1(r1)))
                });
                self.end();
                r
            }
            CMD17_READ_SINGLE_BLOCK | CMD18_READ_MULTIPLE_BLOCK | CMD24_WRITE_BLOCK | CMD25_WRITE_MULTIPLE_BLOCK => Err(Refusal::Argument),
            CMD16_SET_BLOCKLEN => self.plain(index, arg),
            _ => self.plain(index, arg),
        }
    }

    /// CMD17 or CMD18: the blocks read with their tokens and CRCs, CMD12 sent
    /// after a multiple read with its busy signal waited out; the select
    /// released on every path.
    fn read_blocks(&mut self, index: u8, arg: u32, blocks: u32, buf: &mut [u8]) -> Result<(), Refusal> {
        let len = blocks as usize * BLOCK;
        if blocks == 0 || buf.len() < len || (index != CMD17_READ_SINGLE_BLOCK && index != CMD18_READ_MULTIPLE_BLOCK) {
            return Err(Refusal::Argument);
        }
        if self.card_present() == Some(false) {
            return Err(Refusal::NoCard);
        }
        let r = self.blocks_in(index, arg, blocks, &mut buf[..len]);
        self.end();
        r
    }

    /// CMD24 or CMD25: the blocks written with their tokens, CRCs, data
    /// responses and busy signals, the stop token after a multiple write; the
    /// select released on every path.
    fn write_blocks(&mut self, index: u8, arg: u32, blocks: u32, data: &[u8]) -> Result<(), Refusal> {
        let len = blocks as usize * BLOCK;
        if blocks == 0 || data.len() < len || (index != CMD24_WRITE_BLOCK && index != CMD25_WRITE_MULTIPLE_BLOCK) {
            return Err(Refusal::Argument);
        }
        if self.card_present() == Some(false) {
            return Err(Refusal::NoCard);
        }
        let r = self.blocks_out(index, arg, blocks, &data[..len]);
        self.end();
        r
    }

    fn pause(&mut self, micros: Micros) {
        self.wire.pause(micros);
    }
}
