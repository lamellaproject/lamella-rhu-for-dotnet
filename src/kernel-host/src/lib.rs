//! The host tier: the kernel's arithmetic modules (the arena, the heap, the
//! wake clock, the calendar, the floating-point emulation) and the handshake's
//! judge (the contract window), compiled FROM THEIR OWN
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
#[path = "../../kernel/src/calendar_math.rs"]
mod calendar_math;
#[path = "../../kernel/src/abi_window.rs"]
mod abi_window;
/// The GPIO doors' pure halves: the claims table and the doors' arithmetic.
#[path = "../../kernel/src/claims.rs"]
mod claims;
#[path = "../../kernel/src/gpio_math.rs"]
mod gpio_math;
/// The bus doors' pure half: the descriptors, the message partition, the timing word, the divisor ladder and the budget.
#[path = "../../kernel/src/bus_math.rs"]
mod bus_math;
#[path = "../../kernel/src/rp2350_bus_math.rs"]
mod rp2350_bus_math;
/// The PWM and ADC doors' pure halves: the quantization and the conversion budget.
#[path = "../../kernel/src/pwm_math.rs"]
mod pwm_math;
#[path = "../../kernel/src/adc_math.rs"]
mod adc_math;
/// The RP2350's slice mapping, pure: the line to slice-and-output rule and the shared-output twin.
#[path = "../../kernel/src/board/rp2350_map.rs"]
mod rp2350_map;
/// The double-precision emulation's pure halves: the arithmetic and the decoder.
mod fpemul;
/// The decoder's vectors: the assembler's encodings of every form (a generated table).
mod fpemul_vectors;

#[cfg(test)]
mod tests;
