//! Arduino Portenta H7 (STM32H747XI, the Cortex-M7 core; on the Portenta
//! Mid Carrier): flash-boot at 0x08000000 after the board is
//! erased, the memory console by default and USART1 on the maker's console
//! pins (PA9 transmits, PA10 receives) as the probe channel. The board
//! facts are the Portenta family's (`portenta_h7_common.rs`); everything
//! else is the chip family module's (`stm32h7.rs`): the supply entry, the
//! two clock postures (`arduino-portenta-h7-hsi64` selects the internal
//! oscillator posture), the console driver, the random source and the
//! calendar seam. This variant carries the radio module, the secure element
//! and the video bridge (its facts module states the population).

pub use super::stm32h7::*;
pub use super::portenta_h7_common::*;

pub const NAME: &str = "Arduino Portenta H7";
