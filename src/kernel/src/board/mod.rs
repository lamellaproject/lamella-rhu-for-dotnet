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

#[cfg(any(
    all(
        feature = "board-mps2-an500",
        any(
            feature = "board-stm32f769i-disco",
            feature = "board-mps2-an505",
            feature = "board-nucleo-u5a5zj-q",
        )
    ),
    all(
        feature = "board-stm32f769i-disco",
        any(feature = "board-mps2-an505", feature = "board-nucleo-u5a5zj-q")
    ),
    all(feature = "board-mps2-an505", feature = "board-nucleo-u5a5zj-q"),
))]
compile_error!("select exactly one board-* feature");

#[cfg(not(any(
    feature = "board-mps2-an500",
    feature = "board-stm32f769i-disco",
    feature = "board-mps2-an505",
    feature = "board-nucleo-u5a5zj-q",
)))]
compile_error!("select exactly one board-* feature");

#[cfg(all(feature = "stm32f769i-disco-pll216", not(feature = "board-stm32f769i-disco")))]
compile_error!("stm32f769i-disco-pll216 is a posture of board-stm32f769i-disco -- select that board");

#[cfg(all(feature = "nucleo-u5a5zj-q-pll160", not(feature = "board-nucleo-u5a5zj-q")))]
compile_error!("nucleo-u5a5zj-q-pll160 is a posture of board-nucleo-u5a5zj-q -- select that board");
