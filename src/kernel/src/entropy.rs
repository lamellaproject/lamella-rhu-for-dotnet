//! The entropy door's source: a seeded software PRNG, with the
//! quality DECLARED rather than implied -- `is_hardware` answers 0 on
//! every current board, and a board that grows a real hardware TRNG
//! serves it here and flips the answer to 1. Callers that need
//! cryptographic strength must check the query; this module never
//! pretends.

use crate::critical::with_cs;
use crate::sched;

static mut STATE: u64 = 0;

/// One SplitMix64 step under the CS: seed if virgin, advance, mix.
fn next_u64() -> u64 {
    with_cs(|| unsafe {
        let s = &mut *core::ptr::addr_of_mut!(STATE);
        if *s == 0 {
            *s = sched::now_ns() | 1;
        }
        *s = s.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = *s;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    })
}

/// Fill the whole buffer -- all-or-nothing by construction (a
/// software stream has no short-draw failure mode; the argument
/// refusals live at the API door).
pub fn fill(buf: &mut [u8]) {
    for chunk in buf.chunks_mut(8) {
        let v = next_u64().to_le_bytes();
        chunk.copy_from_slice(&v[..chunk.len()]);
    }
}

/// The declared-quality query: 0 = software PRNG (every current
/// board), 1 = a real hardware TRNG serves the fill.
pub fn is_hardware() -> i32 {
    0
}
