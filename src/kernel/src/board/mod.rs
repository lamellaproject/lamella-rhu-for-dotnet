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
    pub boost: &'static str,
}

/// The console ring shared by the boards driven the ring way (the two
/// emulator rows keep their polled console and answer unbounded room).
#[cfg(any(
    feature = "board-pimoroni-pico-plus-2",
    feature = "board-pimoroni-pico-plus-2-w",
    feature = "board-rpi-pico2",
    feature = "board-rpi-pico2-w",
    feature = "board-st-nucleo-l4r5zi",
    feature = "board-st-nucleo-u5a5zj-q",
    feature = "board-st-stm32f746g-disco",
    feature = "board-st-stm32f769i-disco"
))]
pub mod ring;

/// The GPIO facts per board (the device seam): the emulator rows' virtual
/// facts; the silicon rows' facts read from the vendored board description
/// beside each module where one exists, and stated by the kernel itself,
/// with no named signal, where none does.
#[cfg(any(feature = "board-mps2-an500", feature = "board-mps2-an505"))]
#[path = "facts/mps2.rs"]
pub mod facts;
#[cfg(feature = "board-pimoroni-pico-plus-2")]
#[path = "facts/pimoroni_pico_plus_2.rs"]
pub mod facts;
#[cfg(feature = "board-pimoroni-pico-plus-2-w")]
#[path = "facts/pimoroni_pico_plus_2_w.rs"]
pub mod facts;
#[cfg(feature = "board-rpi-pico2")]
#[path = "facts/rpi_pico2.rs"]
pub mod facts;
#[cfg(feature = "board-rpi-pico2-w")]
#[path = "facts/rpi_pico2_w.rs"]
pub mod facts;
#[cfg(feature = "board-st-nucleo-l4r5zi")]
#[path = "facts/st_nucleo_l4r5zi.rs"]
pub mod facts;
#[cfg(feature = "board-st-nucleo-u5a5zj-q")]
#[path = "facts/st_nucleo_u5a5zj_q.rs"]
pub mod facts;
#[cfg(feature = "board-st-stm32f746g-disco")]
#[path = "facts/st_stm32f746g_disco.rs"]
pub mod facts;
#[cfg(feature = "board-st-stm32f769i-disco")]
#[path = "facts/st_stm32f769i_disco.rs"]
pub mod facts;

/// The GPIO family driver behind the doors (`crate::board::gpio_backend`):
/// the virtual backend on the emulator rows, ST's block with its family's
/// interrupt arm on the ST boards, the RP2350's half on the Picos.
#[cfg(any(feature = "board-mps2-an500", feature = "board-mps2-an505"))]
pub mod virt_gpio;
#[cfg(any(feature = "board-mps2-an500", feature = "board-mps2-an505"))]
pub use virt_gpio as gpio_backend;
#[cfg(any(feature = "board-pimoroni-pico-plus-2", feature = "board-pimoroni-pico-plus-2-w", feature = "board-rpi-pico2", feature = "board-rpi-pico2-w"))]
pub use rp2350::gpio_backend;
#[cfg(any(feature = "board-st-nucleo-l4r5zi", feature = "board-st-nucleo-u5a5zj-q", feature = "board-st-stm32f746g-disco", feature = "board-st-stm32f769i-disco"))]
pub mod stm32_gpio;
#[cfg(any(feature = "board-st-nucleo-l4r5zi", feature = "board-st-nucleo-u5a5zj-q", feature = "board-st-stm32f746g-disco", feature = "board-st-stm32f769i-disco"))]
pub use stm32_gpio as gpio_backend;

/// The bus doors' backends (`crate::board::bus_backend`: the register seam
/// and the pad seam; `i2c_driver` and `spi_driver`: the family drivers over
/// it): on the two emulator rows the register-level models of the ST blocks
/// with the drivers of the blocks each row models -- the STM32F7's I2C and
/// SPI files on the Cortex-M7 twin, the STM32U5's on the Cortex-M33 twin.
/// The ST boards whose facts carry a bus run the same drivers over the
/// silicon backend (`stm32_bus`: volatile register accesses, the pads
/// through the family GPIO driver). A row whose facts carry no bus compiles
/// none of them; its bus calls answer the count 0 and refuse the rest.
#[cfg(all(any(feature = "board-mps2-an500", feature = "board-mps2-an505"), not(rhu_bus_rp2350)))]
pub mod virt_bus;
#[cfg(all(any(feature = "board-mps2-an500", feature = "board-mps2-an505"), not(rhu_bus_rp2350)))]
pub use virt_bus as bus_backend;

/// The PWM and ADC backends (`crate::board::pwm_backend` and
/// `crate::board::adc_backend`): the emulator rows' virtual models in
/// kernel memory; the RP2350's slices and converter on the Pico rows. A row
/// whose facts carry no table compiles none.
#[cfg(any(feature = "board-mps2-an500", feature = "board-mps2-an505"))]
pub mod virt_pwm;
#[cfg(any(feature = "board-mps2-an500", feature = "board-mps2-an505"))]
pub use virt_pwm as pwm_backend;
#[cfg(any(feature = "board-mps2-an500", feature = "board-mps2-an505"))]
pub mod virt_adc;
#[cfg(any(feature = "board-mps2-an500", feature = "board-mps2-an505"))]
pub use virt_adc as adc_backend;
#[cfg(any(feature = "board-pimoroni-pico-plus-2", feature = "board-pimoroni-pico-plus-2-w", feature = "board-rpi-pico2", feature = "board-rpi-pico2-w"))]
pub mod rp2350_map;
#[cfg(any(feature = "board-pimoroni-pico-plus-2", feature = "board-pimoroni-pico-plus-2-w", feature = "board-rpi-pico2", feature = "board-rpi-pico2-w"))]
pub mod rp2350_pwm;
#[cfg(any(feature = "board-pimoroni-pico-plus-2", feature = "board-pimoroni-pico-plus-2-w", feature = "board-rpi-pico2", feature = "board-rpi-pico2-w"))]
pub use rp2350_pwm as pwm_backend;
#[cfg(any(feature = "board-pimoroni-pico-plus-2", feature = "board-pimoroni-pico-plus-2-w", feature = "board-rpi-pico2", feature = "board-rpi-pico2-w"))]
pub mod rp2350_adc;
#[cfg(any(feature = "board-pimoroni-pico-plus-2", feature = "board-pimoroni-pico-plus-2-w", feature = "board-rpi-pico2", feature = "board-rpi-pico2-w"))]
pub use rp2350_adc as adc_backend;
#[cfg(any(feature = "board-st-nucleo-u5a5zj-q", feature = "board-st-stm32f769i-disco"))]
pub mod stm32_bus;
#[cfg(any(feature = "board-st-nucleo-u5a5zj-q", feature = "board-st-stm32f769i-disco"))]
pub use stm32_bus as bus_backend;
#[cfg(all(rhu_bus_rows, not(rhu_bus_rp2350)))]
pub mod stm32_i2c;
#[cfg(all(rhu_bus_rows, not(rhu_bus_rp2350)))]
pub use stm32_i2c as i2c_driver;
#[cfg(any(feature = "board-mps2-an500", feature = "board-st-stm32f769i-disco"))]
pub mod stm32f7_spi;
#[cfg(any(feature = "board-mps2-an500", feature = "board-st-stm32f769i-disco"))]
pub use stm32f7_spi as spi_driver;
#[cfg(all(any(feature = "board-mps2-an505", feature = "board-st-nucleo-u5a5zj-q"), not(rhu_bus_rp2350)))]
pub mod stm32u5_spi;
#[cfg(all(any(feature = "board-mps2-an505", feature = "board-st-nucleo-u5a5zj-q"), not(rhu_bus_rp2350)))]
pub use stm32u5_spi as spi_driver;

#[cfg(feature = "board-mps2-an500")]
mod mps2_an500;
#[cfg(feature = "board-mps2-an500")]
pub use mps2_an500::*;

#[cfg(feature = "board-mps2-an505")]
mod mps2_an505;
#[cfg(feature = "board-mps2-an505")]
pub use mps2_an505::*;

#[cfg(any(feature = "board-pimoroni-pico-plus-2", feature = "board-pimoroni-pico-plus-2-w", feature = "board-rpi-pico2", feature = "board-rpi-pico2-w"))]
mod rp2350;

#[cfg(feature = "board-pimoroni-pico-plus-2")]
mod pimoroni_pico_plus_2;
#[cfg(feature = "board-pimoroni-pico-plus-2")]
pub use pimoroni_pico_plus_2::*;

#[cfg(feature = "board-pimoroni-pico-plus-2-w")]
mod pimoroni_pico_plus_2_w;
#[cfg(feature = "board-pimoroni-pico-plus-2-w")]
pub use pimoroni_pico_plus_2_w::*;

#[cfg(feature = "board-rpi-pico2")]
mod rpi_pico2;
#[cfg(feature = "board-rpi-pico2")]
pub use rpi_pico2::*;

#[cfg(feature = "board-rpi-pico2-w")]
mod rpi_pico2_w;
#[cfg(feature = "board-rpi-pico2-w")]
pub use rpi_pico2_w::*;

#[cfg(feature = "board-st-nucleo-l4r5zi")]
mod stm32l4plus;

#[cfg(feature = "board-st-nucleo-l4r5zi")]
mod st_nucleo_l4r5zi;
#[cfg(feature = "board-st-nucleo-l4r5zi")]
pub use st_nucleo_l4r5zi::*;

#[cfg(feature = "board-st-nucleo-u5a5zj-q")]
mod st_nucleo_u5a5zj_q;
#[cfg(feature = "board-st-nucleo-u5a5zj-q")]
pub use st_nucleo_u5a5zj_q::*;

#[cfg(any(feature = "board-st-stm32f746g-disco", feature = "board-st-stm32f769i-disco"))]
mod stm32f7;

#[cfg(feature = "board-st-stm32f746g-disco")]
mod st_stm32f746g_disco;
#[cfg(feature = "board-st-stm32f746g-disco")]
pub use st_stm32f746g_disco::*;

#[cfg(feature = "board-st-stm32f769i-disco")]
mod st_stm32f769i_disco;
#[cfg(feature = "board-st-stm32f769i-disco")]
pub use st_stm32f769i_disco::*;

const _: () = assert!(FPU_DP != cfg!(feature = "fpemul"), "the board's FPU_DP and the fpemul feature disagree");

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
    + cfg!(feature = "board-mps2-an505") as usize
    + cfg!(feature = "board-pimoroni-pico-plus-2") as usize
    + cfg!(feature = "board-pimoroni-pico-plus-2-w") as usize
    + cfg!(feature = "board-rpi-pico2") as usize
    + cfg!(feature = "board-rpi-pico2-w") as usize
    + cfg!(feature = "board-st-nucleo-l4r5zi") as usize
    + cfg!(feature = "board-st-nucleo-u5a5zj-q") as usize
    + cfg!(feature = "board-st-stm32f746g-disco") as usize
    + cfg!(feature = "board-st-stm32f769i-disco") as usize;
const _: () = assert!(SELECTED_BOARDS == 1, "select exactly one board-* feature");

#[cfg(all(feature = "pimoroni-pico-plus-2-xtal12", not(feature = "board-pimoroni-pico-plus-2")))]
compile_error!("pimoroni-pico-plus-2-xtal12 is a posture of board-pimoroni-pico-plus-2 -- select that board");

#[cfg(all(feature = "pimoroni-pico-plus-2-w-xtal12", not(feature = "board-pimoroni-pico-plus-2-w")))]
compile_error!("pimoroni-pico-plus-2-w-xtal12 is a posture of board-pimoroni-pico-plus-2-w -- select that board");

#[cfg(all(feature = "rpi-pico2-xtal12", not(feature = "board-rpi-pico2")))]
compile_error!("rpi-pico2-xtal12 is a posture of board-rpi-pico2 -- select that board");

#[cfg(all(feature = "rpi-pico2-w-xtal12", not(feature = "board-rpi-pico2-w")))]
compile_error!("rpi-pico2-w-xtal12 is a posture of board-rpi-pico2-w -- select that board");

#[cfg(all(feature = "st-nucleo-l4r5zi-pll120", not(feature = "board-st-nucleo-l4r5zi")))]
compile_error!("st-nucleo-l4r5zi-pll120 is a posture of board-st-nucleo-l4r5zi -- select that board");

#[cfg(all(feature = "st-nucleo-u5a5zj-q-pll160", not(feature = "board-st-nucleo-u5a5zj-q")))]
compile_error!("st-nucleo-u5a5zj-q-pll160 is a posture of board-st-nucleo-u5a5zj-q -- select that board");

#[cfg(all(feature = "st-stm32f746g-disco-pll216", not(feature = "board-st-stm32f746g-disco")))]
compile_error!("st-stm32f746g-disco-pll216 is a posture of board-st-stm32f746g-disco -- select that board");

#[cfg(all(feature = "st-stm32f769i-disco-pll216", not(feature = "board-st-stm32f769i-disco")))]
compile_error!("st-stm32f769i-disco-pll216 is a posture of board-st-stm32f769i-disco -- select that board");

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
/// The rows that carry a wireless module: its interrupt line is registered
/// as a tenant of the GPIO bank's handler, so the wifi test program can
/// wait on the module's wake word.
#[cfg(all(feature = "unhosted-main", any(feature = "board-pimoroni-pico-plus-2-w", feature = "board-rpi-pico2-w")))]
pub fn unhosted_tenant_init() -> i32 {
    rp2350::wireless_tenant::init()
}

/// Every other row: nothing to register.
#[cfg(all(feature = "unhosted-main", not(any(feature = "board-pimoroni-pico-plus-2-w", feature = "board-rpi-pico2-w"))))]
pub fn unhosted_tenant_init() -> i32 {
    0
}

/// The RP2350's bus backend and drivers: the chip's Synopsys I2C block and
/// Arm PL022 SPI block, on the four boards of the chip over the silicon
/// backend (`rp2350_bus`: volatile register accesses, a block taken by
/// cycling it through the reset controller, the pads through the chip's
/// GPIO backend). Under the test feature `test-bus-rp2350` the Cortex-M33
/// emulator row carries register-level models of the two blocks
/// (`virt_bus_rp2350`) in place of the ST blocks' models, so the same
/// drivers are gated on QEMU; the feature names that row alone.
#[cfg(any(feature = "board-pimoroni-pico-plus-2", feature = "board-pimoroni-pico-plus-2-w", feature = "board-rpi-pico2", feature = "board-rpi-pico2-w"))]
pub mod rp2350_bus;
#[cfg(any(feature = "board-pimoroni-pico-plus-2", feature = "board-pimoroni-pico-plus-2-w", feature = "board-rpi-pico2", feature = "board-rpi-pico2-w"))]
pub use rp2350_bus as bus_backend;
#[cfg(all(feature = "board-mps2-an505", feature = "test-bus-rp2350"))]
pub mod virt_bus_rp2350;
#[cfg(all(feature = "board-mps2-an505", feature = "test-bus-rp2350"))]
pub use virt_bus_rp2350 as bus_backend;
#[cfg(rhu_bus_rp2350)]
pub mod rp2350_i2c;
#[cfg(rhu_bus_rp2350)]
pub use rp2350_i2c as i2c_driver;
#[cfg(rhu_bus_rp2350)]
pub mod rp2350_spi;
#[cfg(rhu_bus_rp2350)]
pub use rp2350_spi as spi_driver;
#[cfg(all(feature = "test-bus-rp2350", not(feature = "board-mps2-an505")))]
compile_error!("feature `test-bus-rp2350` requires `board-mps2-an505`: the RP2350's bus models stand in on the Cortex-M33 emulator row alone");
