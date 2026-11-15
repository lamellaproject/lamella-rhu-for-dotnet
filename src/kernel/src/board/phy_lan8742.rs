//! The LAN8742A Ethernet transceiver (Microchip; the LAN8742A and the
//! LAN8742Ai speak the same registers): the registers a medium's driver
//! reads and writes over the management interface, and the decisions made
//! from them, pure. The registers are IEEE 802.3 clause 22's -- the basic
//! control register 0, the basic status register 1, the identifiers 2 and
//! 3 -- and the part's special control/status register 31, as the part's
//! datasheet (DS00001989A, section 4.2) states them. A controller's driver
//! implements `Mdio` over its own management registers and calls the rest.
//!
//! THE IDENTITY FIRST. The first management act reads registers 2 and 3
//! and compares them with the datasheet's identifier (0007h, C13xh: the
//! last four bits are the silicon's revision). A read that does not match
//! means the management wires, the management clock or the address strap
//! are not what the board's facts say, and nothing is written to the part.
//!
//! THE DECISION. A transceiver found at boot is in one of four states, read
//! from the basic control register, the basic status register read twice
//! (its link bit keeps a drop until it is read), and register 31:
//! `Keep` -- the link is up and its negotiation is done: it is left as it
//! is, and the part is never reset under a link that is up (a switch
//! punishes a link that bounces by holding the port's traffic);
//! `Wait` -- the link is up and a negotiation the part has enabled is
//! still running: the caller waits a bounded time;
//! `Restart` -- the link is up with the negotiation disabled or the wait
//! spent: the negotiation is enabled and restarted at once, by a write to
//! the control register that is not a reset;
//! `Down` -- no link: the caller may reset the part.
//!
//! THE RESOLUTION. The speed and the duplex a negotiation settled on are
//! read back from register 31 twice; two equal readings stand, two that
//! differ are refused (a negotiation is still moving), and a code the
//! datasheet does not list is taken as the slowest, 10 Mbit half duplex.

/// The registers, by clause 22's numbers.
pub const REG_BMCR: u8 = 0;
pub const REG_BMSR: u8 = 1;
pub const REG_ID1: u8 = 2;
pub const REG_ID2: u8 = 3;
pub const REG_SCSR: u8 = 31;

/// The basic control register: the soft reset (the part clears it when the
/// reset is done, within half a second), the negotiation's enable and its
/// restart (the part clears it).
pub const BMCR_RESET: u16 = 1 << 15;
pub const BMCR_ANEG_ENABLE: u16 = 1 << 12;
pub const BMCR_ANEG_RESTART: u16 = 1 << 9;
/// The basic status register: the negotiation done, and the link (low
/// until read once the link dropped).
pub const BMSR_ANEG_DONE: u16 = 1 << 5;
pub const BMSR_LINK: u16 = 1 << 2;
/// Register 31: the negotiation done, and the three bits of the speed
/// indication.
pub const SCSR_AUTODONE: u16 = 1 << 12;
pub const SCSR_SPEED_SHIFT: u16 = 2;
pub const SCSR_SPEED_MASK: u16 = 0b111;
/// The identifier: register 2 whole, and register 3 without its revision.
pub const ID1: u16 = 0x0007;
pub const ID2: u16 = 0xC130;
pub const ID2_REVISION_MASK: u16 = 0x000F;
/// The soft reset's bound, from the datasheet's sentence on clause 22.
pub const RESET_BOUND_MS: u32 = 500;
/// The bound on a negotiation found running at boot before it is restarted.
pub const WAIT_BOUND_MS: u32 = 5_000;

/// A management transaction that did not end inside its bound.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct MdioTimeout;

/// The management interface a controller offers: a clause 22 register of
/// the transceiver at `phy` read or written, each a bounded wait.
pub trait Mdio {
    fn read(&mut self, phy: u8, reg: u8) -> Result<u16, MdioTimeout>;
    fn write(&mut self, phy: u8, reg: u8, value: u16) -> Result<(), MdioTimeout>;
}

/// Whether registers 2 and 3 are the part's.
pub const fn identity(id1: u16, id2: u16) -> bool {
    id1 == ID1 && id2 & !ID2_REVISION_MASK == ID2
}

/// The four states the module's head describes.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Decision {
    Keep,
    Wait,
    Restart,
    Down,
}

/// The four words a decision is made from, as read in this order: the
/// control register, the status register twice, register 31. The status
/// register's second reading is the link now; its first is the link since
/// the reading before it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Readings {
    pub bmcr: u16,
    pub bmsr_first: u16,
    pub bmsr_second: u16,
    pub scsr: u16,
}

impl Readings {
    /// The link now: the status register's second reading.
    pub const fn link(&self) -> bool {
        self.bmsr_second & BMSR_LINK != 0
    }

    /// Whether the link went away and came back since the status register
    /// was read before these readings: the first reading low, the second high.
    pub const fn dropped_since(&self) -> bool {
        self.bmsr_first & BMSR_LINK == 0 && self.link()
    }

    /// The decision. The link is judged by the second status reading; a
    /// negotiation is done when both the status register and register 31
    /// say so.
    pub const fn decide(&self) -> Decision {
        if !self.link() {
            return Decision::Down;
        }
        if self.bmsr_second & BMSR_ANEG_DONE != 0 && self.scsr & SCSR_AUTODONE != 0 {
            return Decision::Keep;
        }
        if self.bmcr & BMCR_ANEG_ENABLE != 0 { Decision::Wait } else { Decision::Restart }
    }
}

/// A resolved link: 100 Mbit (else 10) and full duplex (else half).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Resolution {
    pub hundred: bool,
    pub full: bool,
}

/// The slowest link, taken for a speed indication the datasheet does not list.
pub const FLOOR: Resolution = Resolution { hundred: false, full: false };

/// One reading of register 31 as a resolution.
pub const fn resolution(scsr: u16) -> Resolution {
    match (scsr >> SCSR_SPEED_SHIFT) & SCSR_SPEED_MASK {
        0b001 => Resolution { hundred: false, full: false },
        0b101 => Resolution { hundred: false, full: true },
        0b010 => Resolution { hundred: true, full: false },
        0b110 => Resolution { hundred: true, full: true },
        _ => FLOOR,
    }
}

/// The resolution from two readings of register 31: `None` when their
/// speed indications differ.
pub const fn resolve(first: u16, second: u16) -> Option<Resolution> {
    let mask = SCSR_SPEED_MASK << SCSR_SPEED_SHIFT;
    if first & mask != second & mask {
        return None;
    }
    Some(resolution(second))
}

/// The identity read: registers 2 and 3 of the transceiver at `phy`, the
/// pair answered whether or not it is the part's.
pub fn read_identity<M: Mdio>(m: &mut M, phy: u8) -> Result<(u16, u16), MdioTimeout> {
    let id1 = m.read(phy, REG_ID1)?;
    let id2 = m.read(phy, REG_ID2)?;
    Ok((id1, id2))
}

/// The readings a decision is made from, read in their order.
pub fn read_state<M: Mdio>(m: &mut M, phy: u8) -> Result<Readings, MdioTimeout> {
    let bmcr = m.read(phy, REG_BMCR)?;
    let bmsr_first = m.read(phy, REG_BMSR)?;
    let bmsr_second = m.read(phy, REG_BMSR)?;
    let scsr = m.read(phy, REG_SCSR)?;
    Ok(Readings { bmcr, bmsr_first, bmsr_second, scsr })
}

/// The negotiation enabled and restarted: the one write `Restart` asks for.
pub fn restart<M: Mdio>(m: &mut M, phy: u8) -> Result<(), MdioTimeout> {
    m.write(phy, REG_BMCR, BMCR_ANEG_ENABLE | BMCR_ANEG_RESTART)
}

/// The soft reset written (the part clears the bit when it is done).
pub fn reset<M: Mdio>(m: &mut M, phy: u8) -> Result<(), MdioTimeout> {
    m.write(phy, REG_BMCR, BMCR_RESET)
}

/// The resolution read back: register 31 twice.
pub fn read_resolution<M: Mdio>(m: &mut M, phy: u8) -> Result<Option<Resolution>, MdioTimeout> {
    let first = m.read(phy, REG_SCSR)?;
    let second = m.read(phy, REG_SCSR)?;
    Ok(resolve(first, second))
}
