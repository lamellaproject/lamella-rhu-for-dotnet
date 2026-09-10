//! The host tier: the kernel's pure modules compiled FROM THEIR OWN
//! SOURCE FILES (path includes, unchanged bytes) against a host critical
//! section (a plain call), a host board (the row facts and an arena
//! window over a host buffer) and a host console (the standard output),
//! then exercised by `cargo test` on the host toolchain -- the boundary
//! cases a bounded emulator run cannot reach: a request that wraps the
//! heap's arithmetic, a wake target past the 32-bit tick window, the
//! arena's own overflow refusals. The test harness runs this tier as the
//! chain's host leg and pins its case count.
#![allow(dead_code)]

#[macro_use]
mod console;
mod board;
mod critical;
mod sched;

#[path = "../../kernel/src/arena.rs"]
mod arena;
#[path = "../../kernel/src/kheap.rs"]
mod kheap;
#[path = "../../kernel/src/wake.rs"]
mod wake;

#[cfg(test)]
mod tests;
