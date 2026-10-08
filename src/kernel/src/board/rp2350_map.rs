//! The RP2350's slice mapping, arithmetic alone with no hardware access: which PWM slice and output a GPIO line
//! reaches -- the RP2350 datasheet, section 12.5.2, Table 1130 (the mapping
//! of PWM channels to GPIO pins): lines 0 to 15 carry slice 0's A output,
//! slice 0's B, slice 1's A and so on to slice 7's B; lines 16 to 31 repeat
//! that order; lines 32 to 39 carry slices 8 to 11 and lines 40 to 47 repeat
//! them; an even line is the A output, an odd line the B output. Two lines
//! therefore share each slice output -- sixteen apart below line 32, eight
//! apart above -- and carry the same signal when both select it. Pure,
//! covered by the host tests; the rows' facts tables are generated from it.
#![allow(dead_code)]

/// The slice a line reaches.
pub const fn slice_of(line: usize) -> u8 {
    if line < 32 {
        ((line / 2) % 8) as u8
    } else {
        (8 + ((line - 32) / 2) % 4) as u8
    }
}

/// The output the line reaches on that slice: 0 for A, 1 for B.
pub const fn output_of(line: usize) -> u8 {
    (line % 2) as u8
}

/// The other line of a package of `lines` lines that reaches the same slice
/// output, when the package has one.
pub const fn twin_of(line: usize, lines: usize) -> Option<usize> {
    let twin = if line < 16 {
        line + 16
    } else if line < 32 {
        line - 16
    } else if line < 40 {
        line + 8
    } else {
        line - 8
    };
    if twin < lines {
        Some(twin)
    } else {
        None
    }
}
