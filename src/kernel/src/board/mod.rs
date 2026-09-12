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

/// The GPIO facts per board (the device seam): the emulator rows' virtual
/// facts; the silicon rows' facts read from the vendored board description
/// beside each module where one exists, and stated by the kernel itself,
/// with no named signal, where none does.
#[cfg(any(feature = "board-mps2-an500", feature = "board-mps2-an505"))]
#[path = "facts/mps2.rs"]
pub mod facts;
#[cfg(feature = "board-stm32f769i-disco")]
#[path = "facts/stm32f769i_disco.rs"]
pub mod facts;
#[cfg(feature = "board-nucleo-u5a5zj-q")]
#[path = "facts/nucleo_u5a5zj_q.rs"]
pub mod facts;
#[cfg(feature = "board-pico-2")]
#[path = "facts/pico_2.rs"]
pub mod facts;
#[cfg(feature = "board-pico-plus-2")]
#[path = "facts/pico_plus_2.rs"]
pub mod facts;

/// The GPIO family driver behind the doors (`crate::board::gpio_backend`):
/// the virtual backend on the emulator rows, ST's block with its family's
/// interrupt arm on the ST boards, the RP2350's half on the Picos.
#[cfg(any(feature = "board-mps2-an500", feature = "board-mps2-an505"))]
pub mod virt_gpio;
#[cfg(any(feature = "board-mps2-an500", feature = "board-mps2-an505"))]
pub use virt_gpio as gpio_backend;
#[cfg(any(feature = "board-stm32f769i-disco", feature = "board-nucleo-u5a5zj-q"))]
pub mod stm32_gpio;
#[cfg(any(feature = "board-stm32f769i-disco", feature = "board-nucleo-u5a5zj-q"))]
pub use stm32_gpio as gpio_backend;
#[cfg(any(feature = "board-pico-2", feature = "board-pico-plus-2"))]
pub use rp2350::gpio_backend;

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
/// The family GPIO driver's lines: none on the emulator rows (the virtual
/// backend fires in thread context).
#[cfg(any(feature = "board-mps2-an500", feature = "board-mps2-an505"))]
pub const GPIO_IRQS: &[(usize, unsafe extern "C" fn())] = &[];

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

/// The entropy posture of a row: no hardware noise source (the emulator
/// rows, and a silicon row without a noise driver); a source this
/// clock posture cannot feed; or a source served on this posture. The
/// banner line is the posture's own text, constant per image; the
/// hardware query and the witness carry the runtime verdicts.
#[allow(dead_code)]
#[derive(Clone, Copy)]
pub enum EntropyPosture {
    None,
    AbsentByPosture {
        name: &'static str,
        line: &'static str,
    },
    Source {
        id: u32,
        name: &'static str,
        line: &'static str,
    },
}

impl EntropyPosture {
    /// The source's id in the witness (0 when none serves).
    pub const fn id(self) -> u32 {
        match self {
            EntropyPosture::Source { id, .. } => id,
            _ => 0,
        }
    }

    /// The source's name for the api flavor's line.
    #[allow(dead_code)]
    pub const fn name(self) -> &'static str {
        match self {
            EntropyPosture::Source { name, .. } => name,
            EntropyPosture::AbsentByPosture { name, .. } => name,
            EntropyPosture::None => "none",
        }
    }
}

/// What a hardware draw can refuse: no source serves; the block's own
/// health check failed at this draw; the bytes were not ready within the
/// door's bound.
#[allow(dead_code)]
pub enum EntropyErr {
    NoSource,
    Health,
    Timeout,
}

/// What a driver counted during one draw: health flags seen and cleared
/// (discarded runs), and block resets performed.
pub struct DrawCounters {
    pub errors: u32,
    pub resets: u32,
}

/// What the real-time clock said at boot: no clock on this row; the
/// backup domain cold (the oscillator just started, the clock not yet
/// clocked); running but never set; or set, with the instant read.
#[allow(dead_code)]
pub enum RtcBoot {
    NoRtc,
    Cold,
    Uninitialized,
    Read(u64),
}

/// What an explicit set's write-back did.
#[allow(dead_code)]
pub enum RtcWrite {
    NoRtc,
    Written,
    SkippedLseNotReady,
    SkippedRange,
    /// The clock's source is held by an earlier selection of another source (the
    /// selection is written once per backup-domain reset): the write would be
    /// ignored, so it is reported instead of made.
    SkippedSourceHeld,
}

/// The absent shapes of the two seams: no source, no clock. The emulator
/// rows use them for good; a silicon row without its drivers uses them too.
#[allow(dead_code)]
pub mod absent {
    use super::{DrawCounters, EntropyErr, RtcBoot, RtcWrite};

    pub fn entropy_init() -> bool {
        false
    }

    pub fn entropy_draw(_buf: &mut [u8], _counters: &mut DrawCounters) -> Result<(), EntropyErr> {
        Err(EntropyErr::NoSource)
    }

    pub fn rtc_read_at_boot() -> RtcBoot {
        RtcBoot::NoRtc
    }

    pub fn rtc_write(_ticks: u64) -> RtcWrite {
        RtcWrite::NoRtc
    }
}

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

/// The board's initialization hook for a program built with the
/// `unhosted-main` feature (a test program the kernel hands its main
/// thread to; no hosted runtime in that image): called once on that thread
/// just before the program's entry, with the board's boot-time GPIO claims
/// already in place; returns 0, or the claims table's code when a
/// registration is refused. A board with nothing to register answers 0.
///
/// The Pico family's rows: the wireless module's interrupt line is
/// registered as a tenant of the GPIO bank's handler (the wireless variants
/// carry the module), so the wifi test program can wait on the module's
/// wake word.
#[cfg(all(feature = "unhosted-main", any(feature = "board-pico-2", feature = "board-pico-plus-2")))]
pub fn unhosted_tenant_init() -> i32 {
    rp2350::wireless_tenant::init()
}

/// Every other row: nothing to register.
#[cfg(all(feature = "unhosted-main", not(any(feature = "board-pico-2", feature = "board-pico-plus-2"))))]
pub fn unhosted_tenant_init() -> i32 {
    0
}
