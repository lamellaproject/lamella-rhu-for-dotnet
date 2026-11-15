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
/// The kernel's GPIO API's pure halves: the claims table and the functions' arithmetic.
#[path = "../../kernel/src/claims.rs"]
mod claims;
#[path = "../../kernel/src/gpio_math.rs"]
mod gpio_math;
/// The kernel's bus API's pure half: the descriptors, the message partition, the timing word, the divisor ladder and the budget.
#[path = "../../kernel/src/bus_math.rs"]
mod bus_math;
#[path = "../../kernel/src/rp2350_bus_math.rs"]
mod rp2350_bus_math;
/// The PWM and ADC functions' pure halves: the quantization and the conversion budget.
#[path = "../../kernel/src/pwm_math.rs"]
mod pwm_math;
#[path = "../../kernel/src/adc_math.rs"]
mod adc_math;
/// The ST timers' arithmetic, pure: the timer block's facts, the timers' clock rule, the channel fields and the tables' soundness.
#[path = "../../kernel/src/stm32_tim_math.rs"]
mod stm32_tim_math;
/// The ST converters' arithmetic and register sequences, pure: both chips' converter blocks over a scripted register file, one conversion a block at a time.
#[path = "../../kernel/src/stm32_adc_math.rs"]
mod stm32_adc_math;
/// The ST serial blocks' arithmetic and register sequences, pure: the two divisor laws at the internal oscillator, the frame set, both chips' blocks over a scripted register file.
#[path = "../../kernel/src/stm32_usart_math.rs"]
mod stm32_usart_math;
/// The RP2350's slice mapping, pure: the line to slice-and-output rule and the shared-output twin.
#[path = "../../kernel/src/board/rp2350_map.rs"]
mod rp2350_map;
/// The SD card's pure halves: the host trait and the card layer over it, proven over a scripted host.
#[path = "../../kernel/src/sdhost.rs"]
mod sdhost;
#[path = "../../kernel/src/sdcard.rs"]
mod sdcard;
/// The SPI-mode host for the card layer, pure over its wire trait, proven over a scripted wire.
#[path = "../../kernel/src/sdspi.rs"]
mod sdspi;
/// The FAT filesystem layer, compiled from the kernel's own file over the host tier's sparse device;
/// the manifest check takes the harness's SHA-256 by path (dependency-free).
#[path = "../../kernel/src/fat.rs"]
mod fat;
/// The kernel's file API's table over the FAT layer, pure: the mount's record and states, the two
/// handle tables with their generation, the paths' routing and the codes, pinned here over the sparse device.
#[path = "../../kernel/src/fs.rs"]
mod fs;
#[path = "../../harness/src/sha256.rs"]
mod sha256;
/// The sparse in-memory block device the FAT cases run over (host-only).
mod sparse;
/// The second reader (host-only): a read-only FAT walker written from the specification separately
/// from fat.rs, the independent check of the write side's cases; it shares no helper, constant or type with fat.rs.
mod fatread;
/// The network checksum arithmetic, pure: the matcher the host cases pin against the vendored stack's own parser.
#[path = "../../kernel/src/net_math.rs"]
mod net_math;
/// The kernel's network API's scripted peer, pure: the virtual medium's other end, every reply it builds parsed here by the stack's own wire types.
#[path = "../../kernel/src/net_peer.rs"]
mod net_peer;
/// The wired medium's arithmetic, pure: the hardware address from a chip's unique identifier, the management clock's divider, the controller's descriptor words and the judgement of a received one.
#[path = "../../kernel/src/eth_math.rs"]
mod eth_math;
/// The LAN8742A transceiver's registers and decisions, pure: the identity, the four states found at boot, the resolution read back twice.
#[path = "../../kernel/src/board/phy_lan8742.rs"]
mod phy_lan8742;
/// The UART pin words' arithmetic, pure: the effective pair of a UART block from the row's pair, the chip's legal sets and the project's words.
#[path = "../../kernel/src/uart_math.rs"]
mod uart_math;
/// The STM32U5's second I/O supply, pure: the facts' shape and the reference manual's sequence, proven over a scripted register file.
#[path = "../../kernel/src/stm32u5_supply_math.rs"]
mod stm32u5_supply_math;
/// The exit word's pure half: the header word's encoding and decoding, the states' names.
#[path = "../../kernel/src/exit_word.rs"]
mod exit_word;
/// The double-precision emulation's pure halves: the arithmetic and the decoder.
mod fpemul;
/// The decoder's vectors: the assembler's encodings of every form (a generated table).
mod fpemul_vectors;
/// SHA-256 as a stream: the digest the kernel checks the radio's images against their pins with.
#[path = "../../kernel/src/digest.rs"]
mod digest;
/// The radio's three images: their pins, the directory of the flash corner they travel in, and the check of their content.
#[path = "../../kernel/src/wifi_images.rs"]
mod wifi_images;
/// The stored network's reader: the two slots, a slot's validity, the current record.
#[path = "../../kernel/src/wifi_stored.rs"]
mod wifi_stored;
/// The wireless medium's pure half: the wifi driver behind the medium seam, proven here over the driver's scripted transport.
#[path = "../../kernel/src/wifi_medium.rs"]
mod wifi_medium;
/// The wireless medium's recorded exchanges (a generated table, host-only).
#[cfg(test)]
mod wifi_exchange;

#[cfg(test)]
mod tests;
