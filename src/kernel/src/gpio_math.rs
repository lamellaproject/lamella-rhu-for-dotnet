//! The GPIO doors' pure arithmetic: the selectors the header states and
//! their partition (served, refused as unsupported, refused as invalid),
//! the event counter's modular difference, and the families' line
//! arithmetic (the ST port and bit of a line, the F7's grouped and the U5's
//! per-line interrupt positions, the RP2350's interrupt word and shift).
//! Pure, host-tested; the drivers cite the manuals for the numbers here.

/// The modes and pulls of `rhu_gpio_claim`, as the header numbers them.
pub const MODE_INPUT: u32 = 0;
#[allow(dead_code)]
pub const MODE_OUTPUT: u32 = 1;
pub const MODE_OUTPUT_OPEN_DRAIN: u32 = 2;
#[allow(dead_code)]
pub const PULL_NONE: u32 = 0;
#[allow(dead_code)]
pub const PULL_UP: u32 = 1;
pub const PULL_DOWN: u32 = 2;

/// The event kinds of `rhu_gpio_edge`: three served, the level and wakeup
/// kinds refused as unsupported (an edge block cannot serve a level
/// contract honestly), anything else invalid.
pub const KIND_NONE: u32 = 0;
pub const KIND_RISING: u32 = 1;
pub const KIND_FALLING: u32 = 2;
pub const KIND_BOTH: u32 = 3;
pub const KIND_LEVEL_LOW: u32 = 4;
pub const KIND_LEVEL_HIGH: u32 = 5;
pub const KIND_WAKE_RISING: u32 = 6;
pub const KIND_WAKE_FALLING: u32 = 7;
pub const KIND_WAKE_BOTH: u32 = 8;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum KindClass {
    None,
    Edge { rising: bool, falling: bool },
    Unsupported,
    Invalid,
}

pub const fn kind_class(kind: u32) -> KindClass {
    match kind {
        KIND_NONE => KindClass::None,
        KIND_RISING => KindClass::Edge { rising: true, falling: false },
        KIND_FALLING => KindClass::Edge { rising: false, falling: true },
        KIND_BOTH => KindClass::Edge { rising: true, falling: true },
        KIND_LEVEL_LOW | KIND_LEVEL_HIGH | KIND_WAKE_RISING | KIND_WAKE_FALLING | KIND_WAKE_BOTH => KindClass::Unsupported,
        _ => KindClass::Invalid,
    }
}

pub const fn mode_valid(mode: u32) -> bool {
    mode <= MODE_OUTPUT_OPEN_DRAIN
}

pub const fn pull_valid(pull: u32) -> bool {
    pull <= PULL_DOWN
}

/// Edges since a snapshot of the counter: modular, so the counter may
/// wrap; a wait misreads only if exactly 2^32 edges land between the
/// snapshot and the wait.
#[allow(dead_code)]
pub const fn edges_since(now: u32, snapshot: u32) -> u32 {
    now.wrapping_sub(snapshot)
}

/// An ST line number is the port letter's index times sixteen plus the
/// bit (PA0 = 0, PB0 = 16, PG14 = 110).
#[allow(dead_code)]
pub const fn st_split(line: u32) -> (u32, u32) {
    (line / 16, line % 16)
}

/// The classic EXTI positions in the vector table (the F7 and the L4+):
/// lines 0 to 4 at 6 to 10, lines 5 to 9 together at 23, lines 10 to 15
/// together at 40 (RM0410's and RM0432's vector tables).
#[allow(dead_code)]
pub const fn st_f7_nvic(bit: u32) -> usize {
    match bit {
        0..=4 => 6 + bit as usize,
        5..=9 => 23,
        _ => 40,
    }
}

/// The U5's EXTI positions: one per line, EXTI0 to EXTI15 at 11 to 26
/// (RM0456's vector table).
#[allow(dead_code)]
pub const fn st_u5_nvic(bit: u32) -> usize {
    11 + bit as usize
}

/// The RP2350's interrupt arrays: eight lines per 32-bit word, four bits
/// per line -- the word index and the line's shift within it.
#[allow(dead_code)]
pub const fn rp_word(line: u32) -> (usize, u32) {
    ((line / 8) as usize, (line % 8) * 4)
}

/// The four bits of a line in the RP2350's interrupt words.
#[allow(dead_code)]
pub const RP_LEVEL_LOW: u32 = 0;
#[allow(dead_code)]
pub const RP_LEVEL_HIGH: u32 = 1;
#[allow(dead_code)]
pub const RP_EDGE_LOW: u32 = 2;
#[allow(dead_code)]
pub const RP_EDGE_HIGH: u32 = 3;
