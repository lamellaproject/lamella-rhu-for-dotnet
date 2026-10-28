//! The SD memory card over the host trait: the
//! identification sequence, the capacity from the
//! CSD, the block reads and writes by number, the programming-state wait,
//! every refusal naming its step. Pure: no register, no chip, no
//! allocation; the host tier's scripted host proves it on the development machine
//! (src/kernel-host), a run on the STM32F769 proves it on the socket. AUTHORED
//! from RM0410's chapter 39 (the native bus: the response formats of 39.5,
//! the identification process of 39.4.1, the card status) and ChaN's "How to
//! Use MMC/SDC" (the command table: CMD8's argument, ACMD41's HCS bit).
//! Nothing of the SD Association's specification is read by this project.

#![allow(dead_code)] // the host tier exercises the whole surface (the CRCs the controller computes in hardware, the programming state, the CID words the api flavor alone reads); each kernel image uses what its flavor needs

use crate::sdhost::{Rate, Refusal, Reply, Response, SdHost, Width};

/// The block size every transfer uses (CMD16 sets it; a high-capacity
/// card has no other).
pub const BLOCK: usize = 512;
/// The ACMD41 polls, 1 ms apart: one second's budget for the card's
/// power-up (measured against the cards' real times).
pub const ACMD41_TRIES: u32 = 1000;
/// The programming-state polls, 1 ms apart, per block written (the write
/// timeout of 250 ms per block).
pub const PROGRAM_TRIES_PER_BLOCK: u32 = 250;
/// One retry of a transfer refused with a CRC error or with the host's FIFO
/// losing pace -- after the recovery below.
pub const CRC_RETRIES: u32 = 1;

/// The steps a refusal names.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Step {
    Attach,
    Bus,
    Cmd0,
    Cmd8,
    Acmd41,
    Cmd2,
    Cmd3,
    Cmd9,
    Csd,
    Cmd7,
    Acmd6,
    Cmd16,
    Transfer,
    Cmd13,
}

/// Why a step refused.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Why {
    /// The host refused the command or the transfer with the controller's reason.
    Refused(Refusal),
    /// No card in the socket.
    NoCard,
    /// The card never left busy (ACMD41) or never left the programming
    /// state (CMD13) within the budget.
    Busy,
    /// A response that is not what the step expects: CMD8's echo wrong,
    /// CMD3's address zero three times, a card status with an error bit.
    Echo,
    /// The CSD's geometry impossible: a structure version this layer does
    /// not serve, a block length outside 512 to 2048 bytes, a capacity of
    /// zero or past 32 bits of blocks, or a capacity class that
    /// contradicts the OCR's.
    Geometry,
    /// A card this layer does not serve (no answer to ACMD41: an MMC or a
    /// dead card).
    Unsupported,
    /// A write on a write-protected card.
    ReadOnly,
    /// A caller's argument: a zero count, a short buffer, a range past
    /// the card's end.
    Argument,
}

/// A refusal with its step named (the SDRAM bring-up's `Stuck(step)` shape).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Stuck {
    pub step: Step,
    pub why: Why,
}

/// The identified card.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Card {
    /// The card answered CMD8 (a version 2.00 or later card).
    pub version2: bool,
    /// The OCR's CCS: the data commands take a block number, not a byte address.
    pub high_capacity: bool,
    pub capacity_blocks: u32,
    pub rca: u16,
    pub cid: [u32; 4],
    pub csd: [u32; 4],
    pub write_protected: bool,
    pub width: Width,
}

// The command indices (RM0410 gives the native steps).
pub const CMD0_GO_IDLE_STATE: u8 = 0;
pub const CMD2_ALL_SEND_CID: u8 = 2;
pub const CMD3_SEND_RELATIVE_ADDR: u8 = 3;
/// As ACMD6 (after CMD55).
pub const CMD6_SET_BUS_WIDTH: u8 = 6;
pub const CMD7_SELECT_CARD: u8 = 7;
pub const CMD8_SEND_IF_COND: u8 = 8;
pub const CMD9_SEND_CSD: u8 = 9;
pub const CMD12_STOP_TRANSMISSION: u8 = 12;
pub const CMD13_SEND_STATUS: u8 = 13;
pub const CMD16_SET_BLOCKLEN: u8 = 16;
pub const CMD17_READ_SINGLE_BLOCK: u8 = 17;
pub const CMD18_READ_MULTIPLE_BLOCK: u8 = 18;
pub const CMD24_WRITE_BLOCK: u8 = 24;
pub const CMD25_WRITE_MULTIPLE_BLOCK: u8 = 25;
/// As ACMD41 (after CMD55).
pub const CMD41_SD_SEND_OP_COND: u8 = 41;
pub const CMD55_APP_CMD: u8 = 55;

/// CMD8's argument: the supply voltage 2.7 to 3.6 V as [11:8] = 1 and the
/// check pattern 0xAA (ChaN's table, note 3); the R7 echoes both.
pub const CMD8_ARG: u32 = 0x0000_01AA;
/// ACMD41's argument: HCS in bit 30 (ChaN's table, note 2) and the
/// host's voltage window 3.2 to 3.4 V (OCR bits 20 and 21).
pub const ACMD41_VOLTAGE_WINDOW: u32 = 0x0030_0000;
pub const ACMD41_HCS: u32 = 1 << 30;
/// The OCR after ACMD41 (the R3): bit 31 the power-up status, set when
/// the card is ready; bit 30 CCS.
pub const OCR_READY: u32 = 1 << 31;
pub const OCR_CCS: u32 = 1 << 30;

// The card status (R1's 32 bits, RM0410 39.5.1), the bits this layer reads.
pub const STATUS_OUT_OF_RANGE: u32 = 1 << 31;
pub const STATUS_ADDRESS_ERROR: u32 = 1 << 30;
pub const STATUS_BLOCK_LEN_ERROR: u32 = 1 << 29;
pub const STATUS_WP_VIOLATION: u32 = 1 << 26;
pub const STATUS_COM_CRC_ERROR: u32 = 1 << 23;
pub const STATUS_ILLEGAL_COMMAND: u32 = 1 << 22;
pub const STATUS_CARD_ECC_FAILED: u32 = 1 << 21;
pub const STATUS_CC_ERROR: u32 = 1 << 20;
pub const STATUS_ERROR: u32 = 1 << 19;
pub const STATUS_READY_FOR_DATA: u32 = 1 << 8;
pub const STATUS_ERROR_BITS: u32 = STATUS_OUT_OF_RANGE
    | STATUS_ADDRESS_ERROR
    | STATUS_BLOCK_LEN_ERROR
    | STATUS_WP_VIOLATION
    | STATUS_COM_CRC_ERROR
    | STATUS_ILLEGAL_COMMAND
    | STATUS_CARD_ECC_FAILED
    | STATUS_CC_ERROR
    | STATUS_ERROR;
/// CURRENT_STATE, the status's [12:9]: 4 the transfer state, 7 programming.
pub const fn status_state(status: u32) -> u32 {
    (status >> 9) & 0xF
}
pub const STATE_TRAN: u32 = 4;
pub const STATE_PRG: u32 = 7;

/// The CRC7 of a command frame's first five bytes (the start and
/// transmission bits with the index, then the argument), the polynomial
/// x^7 + x^3 + 1 -- the controller computes it on the wire; the host
/// tier's frames and the CSD's and CID's own CRC7 field use it.
pub fn crc7(bytes: &[u8]) -> u8 {
    let mut crc: u8 = 0;
    for &b in bytes {
        let mut d = b;
        for _ in 0..8 {
            let bit = ((crc >> 6) ^ (d >> 7)) & 1;
            crc = (crc << 1) & 0x7F;
            if bit != 0 {
                crc ^= 0x09;
            }
            d <<= 1;
        }
    }
    crc
}

/// A command frame's last byte: the CRC7 shifted left with the end bit
/// (0x95 for CMD0, 0x87 for CMD8 with 0x1AA).
pub fn frame_crc_byte(index: u8, arg: u32) -> u8 {
    let f = [0x40 | (index & 0x3F), (arg >> 24) as u8, (arg >> 16) as u8, (arg >> 8) as u8, arg as u8];
    (crc7(&f) << 1) | 1
}

/// The CRC16 of a data line's bytes (the CCITT polynomial
/// x^16 + x^12 + x^5 + 1, no reflection, the initial value 0), as the
/// controller checks it per data line.
pub fn crc16(bytes: &[u8]) -> u16 {
    let mut crc: u16 = 0;
    for &b in bytes {
        crc ^= (b as u16) << 8;
        for _ in 0..8 {
            crc = if crc & 0x8000 != 0 { (crc << 1) ^ 0x1021 } else { crc << 1 };
        }
    }
    crc
}

/// A field of a 128-bit register held most significant word first (the
/// long response: bit 127 is word 0's bit 31), bits `hi` down to `lo`
/// inclusive, at most 32 wide.
pub const fn field(r: &[u32; 4], hi: u32, lo: u32) -> u32 {
    let mut v: u32 = 0;
    let mut p = hi + 1;
    while p > lo {
        p -= 1;
        v = (v << 1) | ((r[((127 - p) / 32) as usize] >> (p % 32)) & 1);
    }
    v
}

/// What the CSD says of the card.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Geometry {
    pub capacity_blocks: u32,
    pub write_protected: bool,
    /// CSD_STRUCTURE 1 (the version 2.0 CSD of a high-capacity card).
    pub version2_csd: bool,
}

/// The CSD decoded (CSD_STRUCTURE at [127:126]; version
/// 1.0's READ_BL_LEN [83:80], C_SIZE [73:62] and C_SIZE_MULT [49:47] with
/// the capacity (C_SIZE + 1) x 2^(C_SIZE_MULT + 2) x 2^READ_BL_LEN bytes;
/// version 2.0's C_SIZE [69:48] with the capacity (C_SIZE + 1) x 512 KB;
/// the write-protect bits PERM [13] and TMP [12]).
pub fn decode_csd(csd: &[u32; 4]) -> Result<Geometry, Why> {
    let write_protected = field(csd, 13, 12) != 0;
    match field(csd, 127, 126) {
        0 => {
            let read_bl_len = field(csd, 83, 80);
            if !(9..=11).contains(&read_bl_len) {
                return Err(Why::Geometry);
            }
            let c_size = field(csd, 73, 62) as u64;
            let c_size_mult = field(csd, 49, 47);
            let blocks = ((c_size + 1) << (c_size_mult + 2 + read_bl_len)) / BLOCK as u64;
            if blocks == 0 || blocks > u32::MAX as u64 {
                return Err(Why::Geometry);
            }
            Ok(Geometry { capacity_blocks: blocks as u32, write_protected, version2_csd: false })
        }
        1 => {
            let blocks = (field(csd, 69, 48) as u64 + 1) * 1024;
            if blocks > u32::MAX as u64 {
                return Err(Why::Geometry);
            }
            Ok(Geometry { capacity_blocks: blocks as u32, write_protected, version2_csd: true })
        }
        _ => Err(Why::Geometry),
    }
}

/// The CID's maker id [127:120], product name [103:64] (five ASCII bytes)
/// and serial number [55:24], for the banner and the record.
pub fn cid_manufacturer(cid: &[u32; 4]) -> u8 {
    field(cid, 127, 120) as u8
}
pub fn cid_product(cid: &[u32; 4]) -> [u8; 5] {
    let mut p = [0u8; 5];
    let mut i = 0;
    while i < 5 {
        let hi = 103 - 8 * i as u32;
        p[i] = field(cid, hi, hi - 7) as u8;
        i += 1;
    }
    p
}
pub fn cid_serial(cid: &[u32; 4]) -> u32 {
    field(cid, 55, 24)
}

fn short(r: Reply) -> u32 {
    match r {
        Reply::Short(v) => v,
        _ => 0,
    }
}
fn long(r: Reply) -> [u32; 4] {
    match r {
        Reply::Long(v) => v,
        _ => [0; 4],
    }
}
fn cmd<H: SdHost>(h: &mut H, step: Step, index: u8, arg: u32, response: Response) -> Result<Reply, Stuck> {
    h.command(index, arg, response).map_err(|r| Stuck { step, why: Why::Refused(r) })
}
fn r1_ok(step: Step, status: u32) -> Result<u32, Stuck> {
    if status & STATUS_ERROR_BITS != 0 { Err(Stuck { step, why: Why::Echo }) } else { Ok(status) }
}
fn rca_arg(rca: u16) -> u32 {
    (rca as u32) << 16
}

/// The identification (the native steps over
/// the native bus): the host attached at the
/// identification rate on one line; CMD0; CMD8 (a timeout marks a version
/// 1 card); ACMD41 with HCS on a version 2 card, polled until the OCR's
/// power-up bit, one second's budget; the OCR's CCS; CMD2; CMD3 (the RCA,
/// re-asked while zero); CMD9 and the CSD's decode; CMD7; ACMD6 to four
/// lines (a card that refuses stays at one); CMD16 with 512 (sent
/// to every card); the transfer rate at the width reached. The SCR is not
/// read (ACMD51 moves eight bytes on the data line, a transfer this
/// trait's 512-byte blocks do not carry; the width is proven by ACMD6's
/// status and the first transfer's CRCs).
pub fn identify<H: SdHost>(h: &mut H) -> Result<Card, Stuck> {
    h.attach().map_err(|r| Stuck { step: Step::Attach, why: Why::Refused(r) })?;
    if h.card_present() == Some(false) {
        return Err(Stuck { step: Step::Attach, why: Why::NoCard });
    }
    h.set_bus(Rate::Identification, Width::One).map_err(|r| Stuck { step: Step::Bus, why: Why::Refused(r) })?;
    cmd(h, Step::Cmd0, CMD0_GO_IDLE_STATE, 0, Response::None)?;
    let version2 = match h.command(CMD8_SEND_IF_COND, CMD8_ARG, Response::Short) {
        Ok(r) => {
            if short(r) & 0xFFF != CMD8_ARG {
                return Err(Stuck { step: Step::Cmd8, why: Why::Echo });
            }
            true
        }
        Err(Refusal::Timeout) => false,
        Err(r) => return Err(Stuck { step: Step::Cmd8, why: Why::Refused(r) }),
    };
    let arg41 = ACMD41_VOLTAGE_WINDOW | if version2 { ACMD41_HCS } else { 0 };
    let mut ocr;
    let mut tries = 0;
    loop {
        match h.command(CMD55_APP_CMD, 0, Response::Short) {
            Ok(r) => {
                r1_ok(Step::Acmd41, short(r))?;
            }
            Err(Refusal::Timeout) => return Err(Stuck { step: Step::Acmd41, why: Why::Unsupported }),
            Err(r) => return Err(Stuck { step: Step::Acmd41, why: Why::Refused(r) }),
        }
        match h.command(CMD41_SD_SEND_OP_COND, arg41, Response::ShortNoCrc) {
            Ok(r) => ocr = short(r),
            Err(Refusal::Timeout) => return Err(Stuck { step: Step::Acmd41, why: Why::Unsupported }),
            Err(r) => return Err(Stuck { step: Step::Acmd41, why: Why::Refused(r) }),
        }
        if ocr & OCR_READY != 0 {
            break;
        }
        tries += 1;
        if tries >= ACMD41_TRIES {
            return Err(Stuck { step: Step::Acmd41, why: Why::Busy });
        }
        h.pause(1000);
    }
    let high_capacity = version2 && ocr & OCR_CCS != 0;
    let cid = long(cmd(h, Step::Cmd2, CMD2_ALL_SEND_CID, 0, Response::Long)?);
    let mut rca: u16 = 0;
    for _ in 0..3 {
        rca = (short(cmd(h, Step::Cmd3, CMD3_SEND_RELATIVE_ADDR, 0, Response::Short)?) >> 16) as u16;
        if rca != 0 {
            break;
        }
    }
    if rca == 0 {
        return Err(Stuck { step: Step::Cmd3, why: Why::Echo });
    }
    let csd = long(cmd(h, Step::Cmd9, CMD9_SEND_CSD, rca_arg(rca), Response::Long)?);
    let g = decode_csd(&csd).map_err(|why| Stuck { step: Step::Csd, why })?;
    if g.version2_csd != high_capacity {
        return Err(Stuck { step: Step::Csd, why: Why::Geometry });
    }
    r1_ok(Step::Cmd7, short(cmd(h, Step::Cmd7, CMD7_SELECT_CARD, rca_arg(rca), Response::ShortBusy)?))?;
    r1_ok(Step::Acmd6, short(cmd(h, Step::Acmd6, CMD55_APP_CMD, rca_arg(rca), Response::Short)?))?;
    let width = match h.command(CMD6_SET_BUS_WIDTH, 2, Response::Short) {
        Ok(r) if short(r) & STATUS_ERROR_BITS == 0 => Width::Four,
        Ok(_) | Err(Refusal::Timeout) => Width::One,
        Err(r) => return Err(Stuck { step: Step::Acmd6, why: Why::Refused(r) }),
    };
    r1_ok(Step::Cmd16, short(cmd(h, Step::Cmd16, CMD16_SET_BLOCKLEN, BLOCK as u32, Response::Short)?))?;
    h.set_bus(Rate::Default, width).map_err(|r| Stuck { step: Step::Bus, why: Why::Refused(r) })?;
    Ok(Card { version2, high_capacity, capacity_blocks: g.capacity_blocks, rca, cid, csd, write_protected: g.write_protected, width })
}

/// The data command's argument: the block number on a high-capacity
/// card, the byte address on a standard-capacity one (the two
/// address rules; under 2 GB the byte address fits 32 bits).
pub fn address(card: &Card, lba: u32, blocks: u32) -> Result<u32, Why> {
    if blocks == 0 || lba.checked_add(blocks).is_none_or(|end| end > card.capacity_blocks) {
        return Err(Why::Argument);
    }
    if card.high_capacity { Ok(lba) } else { lba.checked_mul(BLOCK as u32).ok_or(Why::Argument) }
}

/// After a transfer the host refused on its data path (the FIFO lost pace, a
/// CRC failed, the data timeout, the data line stuck busy) the card may still
/// be in its data state -- receiving the rest of a block that never comes, or
/// sending -- and ignores a new data command: the recovery
/// is CMD12 to end the transfer on the card's side (an R1b whose busy covers the
/// programming of what the card received; its status is not read -- a card
/// already in the transfer state answers it ILLEGAL_COMMAND, cleared by that
/// very response), then the programming state waited out by CMD13 as after any
/// write. A CMD12 the card does not answer returns the transfer's own refusal.
fn recover<H: SdHost>(h: &mut H, card: &Card, blocks: u32, refusal: Refusal) -> Result<(), Stuck> {
    if h.command(CMD12_STOP_TRANSMISSION, 0, Response::ShortBusy).is_err() {
        return Err(Stuck { step: Step::Transfer, why: Why::Refused(refusal) });
    }
    wait_programmed(h, card, blocks)
}

/// A transfer's refusal, answered: no card and an argument need no recovery and
/// end the call; every data-path refusal is recovered from first; a CRC failure
/// or the FIFO losing pace is then retried once (`attempt` counts), a timeout or
/// a busy line is final.
fn refused<H: SdHost>(h: &mut H, card: &Card, blocks: u32, r: Refusal, attempt: &mut u32) -> Result<(), Stuck> {
    let stuck = Stuck { step: Step::Transfer, why: Why::Refused(r) };
    if matches!(r, Refusal::NoCard | Refusal::Argument) {
        return Err(stuck);
    }
    recover(h, card, blocks, r)?;
    if matches!(r, Refusal::Crc | Refusal::Overrun) && *attempt < CRC_RETRIES {
        *attempt += 1;
        Ok(())
    } else {
        Err(stuck)
    }
}

/// `blocks` blocks from `lba` into `buf` (CMD17 for one, CMD18 with the
/// host's CMD12 for more); a refusal recovered from and, for a CRC failure or
/// the FIFO losing pace, retried once.
pub fn read<H: SdHost>(h: &mut H, card: &Card, lba: u32, blocks: u32, buf: &mut [u8]) -> Result<(), Stuck> {
    let arg = address(card, lba, blocks).map_err(|why| Stuck { step: Step::Transfer, why })?;
    let len = blocks as usize * BLOCK;
    if buf.len() < len {
        return Err(Stuck { step: Step::Transfer, why: Why::Argument });
    }
    let index = if blocks == 1 { CMD17_READ_SINGLE_BLOCK } else { CMD18_READ_MULTIPLE_BLOCK };
    let mut attempt = 0;
    loop {
        match h.read_blocks(index, arg, blocks, &mut buf[..len]) {
            Ok(()) => return Ok(()),
            Err(r) => refused(h, card, blocks, r, &mut attempt)?,
        }
    }
}

/// `blocks` blocks from `data` to `lba` (CMD24 for one, CMD25 with the
/// host's CMD12 for more), then the card's programming state waited out
/// by CMD13; a refusal recovered from (CMD12, the programming wait) and, for
/// a CRC failure or the FIFO losing pace, retried once.
pub fn write<H: SdHost>(h: &mut H, card: &Card, lba: u32, blocks: u32, data: &[u8]) -> Result<(), Stuck> {
    if card.write_protected {
        return Err(Stuck { step: Step::Transfer, why: Why::ReadOnly });
    }
    let arg = address(card, lba, blocks).map_err(|why| Stuck { step: Step::Transfer, why })?;
    let len = blocks as usize * BLOCK;
    if data.len() < len {
        return Err(Stuck { step: Step::Transfer, why: Why::Argument });
    }
    let index = if blocks == 1 { CMD24_WRITE_BLOCK } else { CMD25_WRITE_MULTIPLE_BLOCK };
    let mut attempt = 0;
    loop {
        match h.write_blocks(index, arg, blocks, &data[..len]) {
            Ok(()) => break,
            Err(r) => refused(h, card, blocks, r, &mut attempt)?,
        }
    }
    wait_programmed(h, card, blocks)
}

/// The card status by CMD13.
pub fn status<H: SdHost>(h: &mut H, card: &Card) -> Result<u32, Stuck> {
    Ok(short(cmd(h, Step::Cmd13, CMD13_SEND_STATUS, rca_arg(card.rca), Response::Short)?))
}

/// The programming-state wait after a write: CMD13 until the card reads
/// the transfer state with READY_FOR_DATA, 1 ms between polls, 250 ms per
/// block written; an error bit in the status refuses (a write-protect
/// violation as `ReadOnly`).
pub fn wait_programmed<H: SdHost>(h: &mut H, card: &Card, blocks: u32) -> Result<(), Stuck> {
    let budget = PROGRAM_TRIES_PER_BLOCK.saturating_mul(blocks.max(1));
    let mut tries = 0;
    loop {
        let s = status(h, card)?;
        if s & STATUS_ERROR_BITS != 0 {
            let why = if s & STATUS_WP_VIOLATION != 0 { Why::ReadOnly } else { Why::Echo };
            return Err(Stuck { step: Step::Cmd13, why });
        }
        if status_state(s) == STATE_TRAN && s & STATUS_READY_FOR_DATA != 0 {
            return Ok(());
        }
        tries += 1;
        if tries >= budget {
            return Err(Stuck { step: Step::Cmd13, why: Why::Busy });
        }
        h.pause(1000);
    }
}
