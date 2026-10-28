//! The UART pin words' arithmetic (the pin override's kernel half):
//! the effective pins of a UART block from the row's pre-defined pair,
//! the chip's legal pin sets and the project's two link-time words, as a
//! pure function the host tier exercises with the RP2350 rows' tables. The
//! doors' module (`uart.rs`) calls it at every claim and for the
//! enumeration, and the console's wire resolution reaches it through the
//! same call, so what the host gate proves is what every image runs.
//!
//! THE WORDS. A project names a block's pins as two absolute link-time
//! symbols, `__rhu_uart<n>_tx` and `__rhu_uart<n>_rx` (the header's comment
//! block): 0 = not named (the script's `PROVIDE(... = 0)` default; the row's
//! pre-defined pair stands), `0x100 | gpio` = named (bit 8 the valid flag,
//! bits 0 to 7 the GPIO number). Any other value is malformed and refused
//! like an illegal pair: the SDK never writes one, a hand-built image could.
//!
//! THE LAW. A named pair is legal when the TX pin is in the block's
//! TX-capable set, the RX pin is in its RX-capable set and the two differ.
//! The sets are the chip's function table (the facts' `uartPairs`, whose two
//! arrays are SETS: the RP2350's multiplexer is per pin, so any TX-capable
//! pin with any RX-capable pin of the same block is a legal pair). A block
//! with no pre-defined pair (the Pico rows' UART1) is legal only through a
//! named legal pair; a row whose UARTs have no pins on a chip without a
//! sets table (the emulator rows' CMSDK UARTs) is legal as it stands and the
//! words are ignored there. On `core` alone: no allocation, no memory.

use core::fmt;

/// The pin answer of a block without pins (`RHU_UART_NO_PIN`).
pub const NO_PIN: u32 = 0xFFFF_FFFF;
/// The valid flag of a named pin word.
pub const NAMED: u32 = 0x100;

/// The refusal's words at the claim, the block number and the legal pins
/// around them (`[uart] UART1 refused: <words> (the legal pins: ...)`).
pub const REFUSED_NO_DEFAULT: &str = "no default pins on this row; the project's pin words must name a legal pair of this chip";
pub const REFUSED_NOT_LEGAL: &str = "the project's pin words are not a legal pair of this chip";

/// A project's pin word as the kernel reads it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Word {
    Unset,
    Named(u8),
    Malformed,
}

/// The read of one word: 0 unset; `0x100 | gpio` named; anything else
/// malformed (a bit above 8, or bits below 8 without the valid flag).
pub const fn read_word(w: u32) -> Word {
    if w == 0 {
        Word::Unset
    } else if w & !0x1FF == 0 && w & NAMED != 0 {
        Word::Named((w & 0xFF) as u8)
    } else {
        Word::Malformed
    }
}

/// The legal pins of a block: the TX-capable and the RX-capable GPIO sets
/// (the facts' `uartPairs` row of the block).
#[derive(Clone, Copy)]
pub struct PinSets<'a> {
    pub tx: &'a [u8],
    pub rx: &'a [u8],
}

/// The effective pins of a block and whether they are legal: `(tx, rx,
/// legal)`. `facts` is the row's pre-defined pair (`NO_PIN` twice for a
/// block without one), `sets` the chip's legal sets for the block (none
/// on a chip without a table), the two words the project's. A legal named
/// pair answers itself; everything else answers the facts' pair, legal
/// only when the words are unset and the facts' pair has pins.
pub fn effective_pair(facts: (u32, u32), sets: Option<&PinSets<'_>>, tx_word: u32, rx_word: u32) -> (u32, u32, bool) {
    let (ftx, frx) = facts;
    if ftx == NO_PIN && sets.is_none() {
        return (ftx, frx, true);
    }
    let (wt, wr) = (read_word(tx_word), read_word(rx_word));
    if wt == Word::Unset && wr == Word::Unset {
        return (ftx, frx, ftx != NO_PIN);
    }
    if wt == Word::Malformed || wr == Word::Malformed {
        return (ftx, frx, false);
    }
    let tx = match wt {
        Word::Named(g) => g as u32,
        _ => ftx,
    };
    let rx = match wr {
        Word::Named(g) => g as u32,
        _ => frx,
    };
    let legal = tx != NO_PIN
        && rx != NO_PIN
        && tx != rx
        && match sets {
            Some(s) => s.tx.contains(&(tx as u8)) && s.rx.contains(&(rx as u8)),
            None => false,
        };
    if legal { (tx, rx, true) } else { (ftx, frx, false) }
}

/// The legal pins of a block as the refusal line prints them:
/// `TX 4, 8, 20, 24, 36 or 40; RX 5, 9, 21, 25, 37 or 41`, or `none listed
/// for this chip` on a chip without a table.
pub struct LegalPins<'a>(pub Option<&'a PinSets<'a>>);

fn write_set(f: &mut fmt::Formatter<'_>, name: &str, set: &[u8]) -> fmt::Result {
    f.write_str(name)?;
    for (i, p) in set.iter().enumerate() {
        let sep = if i == 0 {
            " "
        } else if i + 1 == set.len() {
            " or "
        } else {
            ", "
        };
        write!(f, "{sep}{p}")?;
    }
    Ok(())
}

impl fmt::Display for LegalPins<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            Some(s) => {
                write_set(f, "TX", s.tx)?;
                f.write_str("; ")?;
                write_set(f, "RX", s.rx)
            }
            None => f.write_str("none listed for this chip"),
        }
    }
}
