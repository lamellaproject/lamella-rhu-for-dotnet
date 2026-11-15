//! The wired medium's arithmetic, pure: the hardware address derived from a
//! chip's unique identifier, the management clock's divider from the bus
//! clock, and the words of the STM32F7 Ethernet controller's descriptors
//! with the judgement of a received one. Const functions on `core` alone,
//! no allocation, no register, no memory of their own; the controller's
//! driver (`board/stm32f7_eth.rs`) types none of these numbers itself.
//!
//! THE HARDWARE ADDRESS. A board without an address memory takes its
//! address from the chip's 96-bit unique identifier: the twelve bytes, in
//! the order they stand in memory, are folded by the 64-bit FNV-1a
//! function (offset basis 0xcbf29ce484222325, prime 0x100000001b3), and
//! the six address bytes are the fold's upper 48 bits, most significant
//! first, with the first byte's two low bits forced to 10 -- a locally
//! administered unicast address. The same twelve bytes give the same six
//! on every boot. It is a digest for telling boards apart on one network,
//! not a registered address and not a secret.
//!
//! THE MANAGEMENT CLOCK. The controller derives the PHY's management clock
//! from the bus clock by one of five dividers, each named for a range of
//! the bus clock (RM0410, the MII address register's CR field); no divider
//! is named under 20 MHz or over 216 MHz, and `mdc_divider` answers `None`
//! there.
//!
//! THE DESCRIPTORS. The controller's normal descriptors are four words:
//! the status and control word, the sizes word, the buffer's address, and
//! in the chained form the next descriptor's address (RM0410, the DMA
//! descriptors). A received frame's length field counts the frame's four
//! check-sequence bytes, which `rx_judge` takes off.

/// The hardware address from the chip's unique identifier (the twelve
/// bytes in memory order): the digest the module's head states.
pub const fn address_from_unique_id(id: &[u8; 12]) -> [u8; 6] {
    const BASIS: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;
    let mut h = BASIS;
    let mut i = 0;
    while i < id.len() {
        h = (h ^ id[i] as u64).wrapping_mul(PRIME);
        i += 1;
    }
    let b = h.to_be_bytes();
    [(b[0] & 0xFC) | 0x02, b[1], b[2], b[3], b[4], b[5]]
}

/// The management clock's divider for a bus clock: the CR field's code and
/// the divisor it selects.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct MdcDivider {
    pub code: u32,
    pub divisor: u32,
}

/// The divider RM0410 names for the bus clock, `None` where it names none
/// (under 20 MHz, over 216 MHz). A clock on a boundary between two ranges
/// takes the upper range's divider, the slower management clock of the two.
pub const fn mdc_divider(hclk_hz: u32) -> Option<MdcDivider> {
    match hclk_hz {
        20_000_000..=34_999_999 => Some(MdcDivider { code: 0b010, divisor: 16 }),
        35_000_000..=59_999_999 => Some(MdcDivider { code: 0b011, divisor: 26 }),
        60_000_000..=99_999_999 => Some(MdcDivider { code: 0b000, divisor: 42 }),
        100_000_000..=149_999_999 => Some(MdcDivider { code: 0b001, divisor: 62 }),
        150_000_000..=216_000_000 => Some(MdcDivider { code: 0b100, divisor: 102 }),
        _ => None,
    }
}

/// The receive descriptor's status word (RDES0): the owner bit (the DMA's
/// while set), the frame length field, the error summary, and the first
/// and last descriptor marks of a frame.
pub const RDES0_OWN: u32 = 1 << 31;
pub const RDES0_FL_SHIFT: u32 = 16;
pub const RDES0_FL_MASK: u32 = 0x3FFF;
pub const RDES0_ES: u32 = 1 << 15;
pub const RDES0_FS: u32 = 1 << 9;
pub const RDES0_LS: u32 = 1 << 8;
/// The receive descriptor's sizes word (RDES1): the chained mark (the
/// fourth word is the next descriptor's address) and the buffer's size.
pub const RDES1_RCH: u32 = 1 << 14;
pub const RDES1_RBS1_MASK: u32 = 0x1FFF;
/// The transmit descriptor's control and status word (TDES0): the owner
/// bit, the interrupt on completion, the last and first segment marks, the
/// chained mark, and the error summary the DMA writes back.
pub const TDES0_OWN: u32 = 1 << 31;
pub const TDES0_IC: u32 = 1 << 30;
pub const TDES0_LS: u32 = 1 << 29;
pub const TDES0_FS: u32 = 1 << 28;
pub const TDES0_TCH: u32 = 1 << 20;
pub const TDES0_ES: u32 = 1 << 15;
/// The transmit descriptor's sizes word (TDES1): the buffer's byte count.
pub const TDES1_TBS1_MASK: u32 = 0x1FFF;
/// The frame check sequence the controller leaves at a received frame's end.
pub const FCS_BYTES: usize = 4;
/// The shortest frame the medium hands up: the Ethernet header.
pub const HEADER_BYTES: usize = 14;

/// What a received descriptor the DMA gave back holds.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RxVerdict {
    /// A whole frame of this many bytes, its check sequence taken off.
    Frame(usize),
    /// The controller's error summary is set, or the length is one no
    /// frame in the buffer can have.
    Errored,
    /// A frame's first or last part alone: the frame did not fit one buffer.
    Fragment,
}

/// The judgement of a received descriptor's status word, for a buffer of
/// `buffer` bytes. The caller has read the owner bit clear.
pub const fn rx_judge(rdes0: u32, buffer: usize) -> RxVerdict {
    if rdes0 & (RDES0_FS | RDES0_LS) != (RDES0_FS | RDES0_LS) {
        return RxVerdict::Fragment;
    }
    if rdes0 & RDES0_ES != 0 {
        return RxVerdict::Errored;
    }
    let length = ((rdes0 >> RDES0_FL_SHIFT) & RDES0_FL_MASK) as usize;
    if length < HEADER_BYTES + FCS_BYTES || length > buffer {
        return RxVerdict::Errored;
    }
    RxVerdict::Frame(length - FCS_BYTES)
}

/// A receive descriptor armed for the DMA: the status word and the sizes
/// word for a buffer of `buffer` bytes in a chained ring.
pub const fn rx_arm(buffer: usize) -> (u32, u32) {
    (RDES0_OWN, RDES1_RCH | (buffer as u32 & RDES1_RBS1_MASK))
}

/// A transmit descriptor handed to the DMA with one whole frame of `len`
/// bytes in its buffer: the control word (the owner bit, the interrupt on
/// completion, the first and the last segment in one, chained) and the
/// sizes word.
pub const fn tx_commit(len: usize) -> (u32, u32) {
    (TDES0_OWN | TDES0_IC | TDES0_LS | TDES0_FS | TDES0_TCH, len as u32 & TDES1_TBS1_MASK)
}

/// A transmit descriptor at rest in a chained ring: the host's, the
/// chained mark alone.
pub const TDES0_IDLE: u32 = TDES0_TCH;

/// The hardware address as the controller's two address registers take it:
/// the high register's low sixteen bits, then the low register's word.
/// THE BYTE ORDER LIVES HERE ALONE. RM0410 names the fields by the
/// address's bit numbers (the high register holds bits 47:32, the low
/// register bits 31:0) and names no byte; this function reads those numbers
/// in the order of transmission, in which IEEE 802.3 sends the first
/// byte's least significant bit first: the first byte on the wire is the
/// low register's bits 7:0 and the sixth is the high register's bits 15:8.
pub const fn address_words(a: &[u8; 6]) -> (u32, u32) {
    ((a[5] as u32) << 8 | a[4] as u32, (a[3] as u32) << 24 | (a[2] as u32) << 16 | (a[1] as u32) << 8 | a[0] as u32)
}
