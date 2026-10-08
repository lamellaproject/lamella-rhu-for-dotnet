//! The double-precision emulation's pure halves -- the arithmetic and the
//! decoder -- included from the kernel's own source files, as the crate's
//! root includes the other pure modules. The two declarations live in this
//! file rather than in an inline module block of the root because a path
//! attribute is resolved from the directory of the file that carries it,
//! with an inline block's name appended as a directory: the block's paths
//! would pass through a directory this crate does not have, and a host that
//! resolves every path component in turn refuses to open them. From this
//! file the paths walk the same two levels up as every other include in the
//! crate, through directories that exist on every host.

#[path = "../../kernel/src/fpemul/soft.rs"]
pub mod soft;
#[path = "../../kernel/src/fpemul/decode.rs"]
pub mod decode;
