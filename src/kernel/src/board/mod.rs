//! Board facade: exactly one board module compiles in, selected by a
//! `board-*` feature, and its surface is re-exported as `crate::board`.

/// Clock-posture read-back for the boot banner. Boards on their boring
/// default posture return `None` (their banked transcript surface never
/// moves); a gated posture returns the live facts with every numeric
/// field DECODED from clock-tree registers, never restated from consts
/// -- the banner line is a query, and the kmain assert against
/// `SYSCLK_HZ` turns a silently-failed posture into a loud panic.
pub struct ClockReadback {
    pub posture: &'static str,
    pub sysclk_hz: u32,
    pub sws: &'static str,
    pub src: &'static str,
    pub pllm: u32,
    pub plln: u32,
    pub pllp: u32,
    pub flash_ws: u32,
    pub overdrive: bool,
}

/// The console ring shared by the boards driven the ring way (the two
/// emulator rows keep their polled console and answer unbounded room).
#[cfg(any(
    feature = "board-stm32f769i-disco",
    feature = "board-nucleo-u5a5zj-q",
    feature = "board-pico-2",
    feature = "board-pico-plus-2"
))]
pub mod ring;

#[cfg(feature = "board-mps2-an500")]
mod mps2_an500;
#[cfg(feature = "board-mps2-an500")]
pub use mps2_an500::*;

#[cfg(feature = "board-stm32f769i-disco")]
mod stm32f769i_disco;
#[cfg(feature = "board-stm32f769i-disco")]
pub use stm32f769i_disco::*;

#[cfg(feature = "board-mps2-an505")]
mod mps2_an505;
#[cfg(feature = "board-mps2-an505")]
pub use mps2_an505::*;

#[cfg(feature = "board-nucleo-u5a5zj-q")]
mod nucleo_u5a5zj_q;
#[cfg(feature = "board-nucleo-u5a5zj-q")]
pub use nucleo_u5a5zj_q::*;

#[cfg(any(feature = "board-pico-2", feature = "board-pico-plus-2"))]
mod rp2350;

#[cfg(feature = "board-pico-2")]
mod pico_2;
#[cfg(feature = "board-pico-2")]
pub use pico_2::*;

#[cfg(feature = "board-pico-plus-2")]
mod pico_plus_2;
#[cfg(feature = "board-pico-plus-2")]
pub use pico_plus_2::*;

/// The interrupt lines a board claims with its own handlers (the line and
/// the handler), folded into the vector table by the boot module. The two
/// emulator rows claim none and inherit this empty list; every silicon
/// board states its own list (its console ring's transmit drain).
#[cfg(any(feature = "board-mps2-an500", feature = "board-mps2-an505"))]
pub const BOARD_IRQS: &[(usize, unsafe extern "C" fn())] = &[];

/// Exactly one board feature: the count of selected boards, asserted at
/// compile time (a pairwise refusal would need one arm per pair).
const SELECTED_BOARDS: usize = cfg!(feature = "board-mps2-an500") as usize
    + cfg!(feature = "board-stm32f769i-disco") as usize
    + cfg!(feature = "board-mps2-an505") as usize
    + cfg!(feature = "board-nucleo-u5a5zj-q") as usize
    + cfg!(feature = "board-pico-2") as usize
    + cfg!(feature = "board-pico-plus-2") as usize;
const _: () = assert!(SELECTED_BOARDS == 1, "select exactly one board-* feature");

#[cfg(all(feature = "stm32f769i-disco-pll216", not(feature = "board-stm32f769i-disco")))]
compile_error!("stm32f769i-disco-pll216 is a posture of board-stm32f769i-disco -- select that board");

#[cfg(all(feature = "nucleo-u5a5zj-q-pll160", not(feature = "board-nucleo-u5a5zj-q")))]
compile_error!("nucleo-u5a5zj-q-pll160 is a posture of board-nucleo-u5a5zj-q -- select that board");

#[cfg(all(feature = "pico-2-pll150", not(feature = "board-pico-2")))]
compile_error!("pico-2-pll150 is a posture of board-pico-2 -- select that board");

#[cfg(all(feature = "pico-plus-2-pll150", not(feature = "board-pico-plus-2")))]
compile_error!("pico-plus-2-pll150 is a posture of board-pico-plus-2 -- select that board");

/// The arena's window: the link-defined NOLOAD slack between the last
/// kernel section and the boot-stack reserve, bracketed by the two map
/// symbols on every board. Behind the facade so a host build of the pure
/// modules supplies a window of its own.
pub fn arena_window() -> (usize, usize) {
    unsafe extern "C" {
        static __rhu_arena_lo: u32;
        static __rhu_arena_hi: u32;
    }
    (
        core::ptr::addr_of!(__rhu_arena_lo) as usize,
        core::ptr::addr_of!(__rhu_arena_hi) as usize,
    )
}
