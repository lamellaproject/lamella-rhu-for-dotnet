//! Board facade: exactly one board module compiles in, selected by a
//! `board-*` feature, and its surface is re-exported as `crate::board`.

#[cfg(feature = "board-mps2-an500")]
mod mps2_an500;
#[cfg(feature = "board-mps2-an500")]
pub use mps2_an500::*;

#[cfg(not(any(feature = "board-mps2-an500")))]
compile_error!("select exactly one board-* feature");
