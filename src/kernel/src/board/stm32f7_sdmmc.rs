//! The STM32F7's SDMMC2 host for the STM32F769 Discovery board's microSD socket:
//! the card
//! layer's `SdHost` (sdhost.rs) over the controller at 0x4001_1C00 with
//! DMA2's stream 0 on channel 11, the socket's six bus pins and its detect
//! line. AUTHORED from RM0410 (chapter 39 the controller: 39.3.1 the
//! clocks, 39.3.2 the DMA recipes, 39.5 the response formats, 39.8 the
//! registers; chapter 8 the DMA with its Table 28; 5.3.26 the kernel clock
//! select), the chip datasheet's Table 13 (the alternate functions) and
//! UM2033 with the board's schematic (the socket's pins, the 47 kohm
//! pull-ups on CMD and D0 to D3, the detect switch to ground with the
//! chip's own pull-up).
//! The kernel takes no vendor header.
//!
//! THE HARDWARE FLOW CONTROL: at the HSI
//! posture the clock register's hardware flow control bit is SET -- 39.7: the card
//! clock stops and the state machines freeze while the FIFO cannot serve, the APB2
//! side stays alive, so a slow refill stalls the wire instead of underrunning it.
//! There the stream's refills from the SDRAM (the FMC at 8 MHz, the bridge at 16 MHz)
//! outpace the card's drain of 4 MB/s by less than twice, and a sustained 64 KB write
//! underruns the transmit FIFO without it (TXUNDERR).
//! At PLL216, the default, with a tenfold margin, the bit stays clear. The
//! controller's and the stream's status words at a refusal are kept for the witness.

use core::ptr::read_volatile;
use core::sync::atomic::{AtomicU32, Ordering};

use crate::blk::BLK_MAX;
use crate::board::facts;
use crate::board::gpio_backend as pins;
use crate::board::stm32_bus::{pad_take_as, rd, rmw, wr};
use crate::gpio_math as g;
use crate::sched;
use crate::sdcard::{BLOCK, CMD12_STOP_TRANSMISSION, CMD18_READ_MULTIPLE_BLOCK, CMD25_WRITE_MULTIPLE_BLOCK};
use crate::sdhost::{Micros, Rate, Refusal, Reply, Response, SdHost, Width};

// ---------------- the clocks ----------------

/// The adapter clock SDMMCCLK: at the PLL216 posture the main PLL's Q
/// output, exactly 48 MHz (PLLQ 9 of the 432 MHz VCO; DCKCFGR2's SDMMC2SEL
/// 0 with CK48MSEL at its reset value, the PLL's Q); at the HSI default no
/// PLL runs and the system clock serves (SDMMC2SEL 1), 16 MHz. Both under
/// the 50 MHz cap of 39.3.1.
#[cfg(stm32f7_pll216)]
const SDMMCCLK_HZ: u32 = 48_000_000;
#[cfg(not(stm32f7_pll216))]
const SDMMCCLK_HZ: u32 = 16_000_000;
#[cfg(stm32f7_pll216)]
const SDMMC2SEL_SYSCLK: bool = false;
#[cfg(not(stm32f7_pll216))]
const SDMMC2SEL_SYSCLK: bool = true;
/// The APB2 clock: the system clock halved at PLL216 (CFGR's PPRE2 in the
/// family module's recipe), the system clock itself at HSI.
#[cfg(stm32f7_pll216)]
const PCLK2_HZ: u32 = crate::board::SYSCLK_HZ / 2;
#[cfg(not(stm32f7_pll216))]
const PCLK2_HZ: u32 = crate::board::SYSCLK_HZ;

/// The two ceilings: the identification clock UNDER 400 kHz (39.8.2), the
/// default-speed transfer clock at or under 25 MHz.
const IDENT_CEILING_HZ: u32 = 400_000;
const TRANSFER_CEILING_HZ: u32 = 25_000_000;

/// The smallest even CLKDIV whose card clock SDMMCCLK / (CLKDIV + 2) is
/// under the ceiling.
const fn clkdiv_under(adapter_hz: u32, ceiling_hz: u32) -> u32 {
    let mut div = 0;
    while adapter_hz / (div + 2) >= ceiling_hz {
        div += 2;
    }
    assert!(div <= 0xFF, "the divider does not fit CLKDIV");
    div
}
const fn card_hz(div: u32) -> u32 {
    SDMMCCLK_HZ / (div + 2)
}
pub const CLKDIV_IDENT: u32 = clkdiv_under(SDMMCCLK_HZ, IDENT_CEILING_HZ);
pub const CLKDIV_TRANSFER: u32 = clkdiv_under(SDMMCCLK_HZ, TRANSFER_CEILING_HZ + 1);
pub const CARD_HZ_IDENT: u32 = card_hz(CLKDIV_IDENT);
pub const CARD_HZ_TRANSFER: u32 = card_hz(CLKDIV_TRANSFER);
// The numbers pinned: CLKDIV 120 and 0 at PLL216
// (393.4 kHz, 24 MHz), 40 and 0 at HSI (381.0 kHz, 8 MHz).
#[cfg(stm32f7_pll216)]
const _: () = assert!(CLKDIV_IDENT == 120 && CLKDIV_TRANSFER == 0 && CARD_HZ_IDENT == 393_442 && CARD_HZ_TRANSFER == 24_000_000, "the PLL216 dividers are not the pinned ones");
#[cfg(not(stm32f7_pll216))]
const _: () = assert!(CLKDIV_IDENT == 40 && CLKDIV_TRANSFER == 0 && CARD_HZ_IDENT == 380_952 && CARD_HZ_TRANSFER == 8_000_000, "the HSI dividers are not the pinned ones");
// The bus clock's condition: PCLK2 above 3 x 4 / 32 of the card
// clock on four lines; the adapter clock under its cap.
const _: () = assert!(PCLK2_HZ as u64 * 32 > 12 * CARD_HZ_TRANSFER as u64, "PCLK2 under the controller's condition for four lines");
const _: () = assert!(SDMMCCLK_HZ <= 50_000_000, "the adapter clock over 50 MHz");

/// The data timeout in card clocks (39.8.9): 100 ms for a read and 250 ms
/// for a write at the rate in force (at the transfer
/// rate: 2,400,000 and 6,000,000 at 24 MHz, 800,000 and 2,000,000 at 8 MHz).
const fn dtimer(hz: u32, ms: u32) -> u32 {
    (hz / 1000) * ms
}
const READ_TIMEOUT_MS: u32 = 100;
const WRITE_TIMEOUT_MS: u32 = 250;
#[cfg(stm32f7_pll216)]
const _: () = assert!(dtimer(CARD_HZ_TRANSFER, READ_TIMEOUT_MS) == 2_400_000 && dtimer(CARD_HZ_TRANSFER, WRITE_TIMEOUT_MS) == 6_000_000, "the PLL216 data timeouts are not the pinned ones");
#[cfg(not(stm32f7_pll216))]
const _: () = assert!(dtimer(CARD_HZ_TRANSFER, READ_TIMEOUT_MS) == 800_000 && dtimer(CARD_HZ_TRANSFER, WRITE_TIMEOUT_MS) == 2_000_000, "the HSI data timeouts are not the pinned ones");

// ---------------- the controller ----------------

const SDMMC2: usize = 0x4001_1C00;
const POWER: usize = SDMMC2 + 0x00;
const CLKCR: usize = SDMMC2 + 0x04;
const ARG: usize = SDMMC2 + 0x08;
const CMD: usize = SDMMC2 + 0x0C;
const RESP1: usize = SDMMC2 + 0x14;
const DTIMER: usize = SDMMC2 + 0x24;
const DLEN: usize = SDMMC2 + 0x28;
const DCTRL: usize = SDMMC2 + 0x2C;
const STA: usize = SDMMC2 + 0x34;
const ICR: usize = SDMMC2 + 0x38;
const MASK: usize = SDMMC2 + 0x3C;
const FIFO: usize = SDMMC2 + 0x80;
/// PWRCTRL 11: the card clocked.
const POWER_ON: u32 = 0b11;
const CLKCR_CLKEN: u32 = 1 << 8;
/// WIDBUS 01: four data lines (00 one line). PWRSAV, BYPASS and NEGEDGE stay 0;
/// HWFC_EN per posture below.
const CLKCR_WIDBUS_4: u32 = 0b01 << 11;
/// HWFC_EN 14 (RM0410 39.7 and 39.8.2): the hardware flow control -- SET at the HSI
/// posture, CLEAR at PLL216 (the module's note above). With it set the transmit
/// FIFO's empty flag means two words left (RM0410 39.8.11): the clock stops there.
#[cfg(stm32f7_pll216)]
const CLKCR_HWFC: u32 = 0;
#[cfg(not(stm32f7_pll216))]
const CLKCR_HWFC: u32 = 1 << 14;
// The clock words pinned: identification and four-line transfer.
#[cfg(stm32f7_pll216)]
const _: () = assert!(CLKCR_CLKEN | CLKCR_HWFC | CLKDIV_IDENT == 0x0178 && CLKCR_CLKEN | CLKCR_HWFC | CLKCR_WIDBUS_4 | CLKDIV_TRANSFER == 0x0900, "the PLL216 clock words moved");
#[cfg(not(stm32f7_pll216))]
const _: () = assert!(CLKCR_CLKEN | CLKCR_HWFC | CLKDIV_IDENT == 0x4128 && CLKCR_CLKEN | CLKCR_HWFC | CLKCR_WIDBUS_4 | CLKDIV_TRANSFER == 0x4900, "the HSI clock words moved");
const CMD_WAITRESP_SHORT: u32 = 0b01 << 6;
const CMD_WAITRESP_LONG: u32 = 0b11 << 6;
const CMD_CPSMEN: u32 = 1 << 10;
const DCTRL_DTEN: u32 = 1 << 0;
/// DTDIR 1: card to controller.
const DCTRL_DTDIR_READ: u32 = 1 << 1;
const DCTRL_DMAEN: u32 = 1 << 3;
/// DBLOCKSIZE 9: 512-byte blocks.
const DCTRL_DBLOCKSIZE_512: u32 = 9 << 4;
const STA_CCRCFAIL: u32 = 1 << 0;
const STA_DCRCFAIL: u32 = 1 << 1;
const STA_CTIMEOUT: u32 = 1 << 2;
const STA_DTIMEOUT: u32 = 1 << 3;
const STA_TXUNDERR: u32 = 1 << 4;
const STA_RXOVERR: u32 = 1 << 5;
const STA_CMDREND: u32 = 1 << 6;
const STA_CMDSENT: u32 = 1 << 7;
const STA_DATAEND: u32 = 1 << 8;
const STA_CMDACT: u32 = 1 << 11;
const STA_RXDAVL: u32 = 1 << 21;
/// The static flags' clear bits (39.8.12: the eleven, SDIOIT among them).
const ICR_ALL: u32 = 0x0040_05FF;
/// The command path's own flags: CCRCFAIL, CTIMEOUT, CMDREND, CMDSENT.
const ICR_CMD: u32 = STA_CCRCFAIL | STA_CTIMEOUT | STA_CMDREND | STA_CMDSENT;
const STA_DATA_ERRORS: u32 = STA_DCRCFAIL | STA_DTIMEOUT | STA_TXUNDERR | STA_RXOVERR;

// ---------------- the clock enables ----------------

const RCC: usize = 0x4002_3800;
const RCC_AHB1ENR: usize = RCC + 0x30;
const RCC_APB2ENR: usize = RCC + 0x44;
const RCC_DCKCFGR2: usize = RCC + 0x90;
/// The ports B, D, G and I (the six pads and the detect line) and DMA2.
const AHB1ENR_PORTS: u32 = (1 << 1) | (1 << 3) | (1 << 6) | (1 << 8);
const AHB1ENR_DMA2EN: u32 = 1 << 22;
const APB2ENR_SDMMC2EN: u32 = 1 << 7;
/// SDMMC2SEL: 0 the 48 MHz clock, 1 the system clock.
const DCKCFGR2_SDMMC2SEL: u32 = 1 << 29;

// ---------------- the DMA ----------------

const DMA2: usize = 0x4002_6400;
const DMA_LISR: usize = DMA2 + 0x00;
const DMA_LIFCR: usize = DMA2 + 0x08;
const S0CR: usize = DMA2 + 0x10;
const S0NDTR: usize = DMA2 + 0x14;
const S0PAR: usize = DMA2 + 0x18;
const S0M0AR: usize = DMA2 + 0x1C;
const S0FCR: usize = DMA2 + 0x24;
/// Stream 0's five flags in LISR and LIFCR: FEIF0 0, DMEIF0 2, TEIF0 3,
/// HTIF0 4, TCIF0 5.
const S0_FLAGS: u32 = 0b11_1101;
const S0_TCIF: u32 = 1 << 5;
const S0_TEIF: u32 = 1 << 3;
/// CHSEL 11 (Table 28: SDMMC2 on DMA2 stream 0).
const CR_CHSEL_11: u32 = 11 << 25;
const CR_MBURST_INCR4: u32 = 0b01 << 23;
const CR_PBURST_INCR4: u32 = 0b01 << 21;
const CR_PL_HIGH: u32 = 0b10 << 16;
const CR_MSIZE_WORD: u32 = 0b10 << 13;
const CR_PSIZE_WORD: u32 = 0b10 << 11;
const CR_MINC: u32 = 1 << 10;
/// DIR 01: memory to peripheral (00 peripheral to memory).
const CR_DIR_M2P: u32 = 0b01 << 6;
/// The peripheral as the flow controller: the controller ends the
/// transfer, the stream's count is not the measure (8.3.16).
const CR_PFCTRL: u32 = 1 << 5;
const CR_EN: u32 = 1 << 0;
/// The FIFO mode (direct mode disabled) with the threshold full: four
/// words a burst, which 8.3.14's table allows at single beats or INCR4 on
/// the memory side for the word size.
const FCR_DMDIS_FULL: u32 = (1 << 2) | 0b11;

// ---------------- the cycle counter ----------------

const DWT_CYCCNT: *const u32 = 0xE000_1004 as *const u32;
const CYCLES_PER_US: u32 = crate::board::SYSCLK_HZ / 1_000_000;
/// The spin before the first yield: about twenty microseconds (a short
/// response at the identification rate takes about 120 us on the wire; a
/// yield costs a context switch).
const SPIN_US: u32 = 20;
/// The command poll's budget: ten times the controller's own 64-clock
/// timeout at the identification rate is about 1.6 ms; 4 ms gives the
/// margin.
const COMMAND_BUDGET_US: u32 = 4_000;
/// The busy wait after a short-busy response (CMD7, CMD12): the card's
/// programming of what a multiple-block write left buffered.
const BUSY_BUDGET_US: u32 = 2_000_000;
/// Every budget stays under what the counter's wrap allows (19.8 s at 216 MHz).
const BUDGET_CAP_US: u32 = 10_000_000;
/// The pads' output speed: very high (OSPEEDR 11), the setting the
/// datasheet characterizes the interface at.
const SPEED_VERY_HIGH: u32 = 0b11;

/// The yields this module's waits took (the cost a
/// transfer's poll puts on the calling thread).
pub static YIELDS: AtomicU32 = AtomicU32::new(0);
/// The controller's status word and the stream's flag word read at the last refusal
/// (the witness's two raw fields: a fault names its flag without a diagnostic
/// copy -- TXUNDERR bit 4, RXOVERR 5, DTIMEOUT 3, DCRCFAIL 1, CTIMEOUT 2; TEIF0 bit 3
/// of LISR).
pub static LAST_STA: AtomicU32 = AtomicU32::new(0);
pub static LAST_LISR: AtomicU32 = AtomicU32::new(0);

fn fault(sta: u32, lisr: u32) {
    LAST_STA.store(sta, Ordering::Relaxed);
    LAST_LISR.store(lisr, Ordering::Relaxed);
}

/// The two words of the last refusal.
pub fn last_flags() -> (u32, u32) {
    (LAST_STA.load(Ordering::Relaxed), LAST_LISR.load(Ordering::Relaxed))
}

/// The core's cycle counter, read.
pub fn cycles() -> u32 {
    unsafe { read_volatile(DWT_CYCCNT) }
}

/// The chip module arms the counter (the unit's software lock opened, the
/// counter proved to advance).
fn arm_cycle_counter() {
    super::stm32f7::arm_cycle_counter();
}

/// The write-spacing rule: after a write to CLKCR, CMD, DCTRL or
/// POWER the register may not be written again for three adapter clocks
/// plus two bus clocks -- the register read back, then 32 core clocks
/// (seven adapter periods at 216 MHz, thirty-two at 16 MHz).
fn settle(reg: usize) {
    let _ = rd(reg);
    let t0 = cycles();
    while cycles().wrapping_sub(t0) < 32 {}
}

/// The STM32F769 socket's host: the controller's state the card layer drives.
pub struct Sdmmc2Host {
    /// Whether the scheduler runs: the waits yield between polls when it
    /// does; the boot banner's identification runs before it and spins.
    pub yielding: bool,
    rate: Rate,
}

impl Sdmmc2Host {
    pub const fn new() -> Self {
        Sdmmc2Host { yielding: false, rate: Rate::Identification }
    }

    /// A bounded poll: the condition read until it holds, spinning for the
    /// first `SPIN_US` and then yielding between reads where the scheduler
    /// runs; false when the budget passes first.
    fn wait(&self, budget_us: u32, mut cond: impl FnMut() -> bool) -> bool {
        let budget = budget_us.min(BUDGET_CAP_US) as u64 * CYCLES_PER_US as u64;
        let spin = SPIN_US * CYCLES_PER_US;
        let t0 = cycles();
        loop {
            if cond() {
                return true;
            }
            let dt = cycles().wrapping_sub(t0);
            if dt as u64 >= budget {
                return false;
            }
            if self.yielding && dt >= spin {
                YIELDS.fetch_add(1, Ordering::Relaxed);
                sched::yield_now();
            }
        }
    }

    fn spin_us(&self, us: u32) {
        let n = us.min(BUDGET_CAP_US).saturating_mul(CYCLES_PER_US);
        let t0 = cycles();
        while cycles().wrapping_sub(t0) < n {}
    }

    fn card_hz(&self) -> u32 {
        match self.rate {
            Rate::Identification => CARD_HZ_IDENT,
            Rate::Default => CARD_HZ_TRANSFER,
        }
    }

    /// The command path: ARG, then CMD with the response kind and the
    /// state machine enabled; the status polled for the end flags under the
    /// budget; CMDREND (CMDSENT for no response) the success, CTIMEOUT and
    /// CCRCFAIL the refusals -- except that R3's CRC field is all ones, so
    /// CCRCFAIL is the expected end of ACMD41 and ignored for it alone
    /// the long response's four words most significant first; a
    /// short-busy response waits for the data line to leave busy (D0 read
    /// through its port's input register) under its own budget.
    fn command_raw(&self, index: u8, arg: u32, response: Response) -> Result<Reply, Refusal> {
        if !self.wait(COMMAND_BUDGET_US, || rd(STA) & STA_CMDACT == 0) {
            fault(rd(STA), 0);
            return Err(Refusal::Timeout);
        }
        wr(ICR, ICR_CMD);
        wr(ARG, arg);
        let waitresp = match response {
            Response::None => 0,
            Response::Long => CMD_WAITRESP_LONG,
            _ => CMD_WAITRESP_SHORT,
        };
        wr(CMD, (index as u32 & 0x3F) | waitresp | CMD_CPSMEN);
        settle(CMD);
        let done = match response {
            Response::None => STA_CMDSENT,
            _ => STA_CMDREND,
        };
        let ends = done | STA_CCRCFAIL | STA_CTIMEOUT;
        let seen = self.wait(COMMAND_BUDGET_US, || rd(STA) & ends != 0);
        let sta = rd(STA);
        wr(ICR, ICR_CMD);
        if !seen || sta & STA_CTIMEOUT != 0 {
            fault(sta, 0);
            return Err(Refusal::Timeout);
        }
        if sta & STA_CCRCFAIL != 0 && response != Response::ShortNoCrc {
            fault(sta, 0);
            return Err(Refusal::Crc);
        }
        let reply = match response {
            Response::None => Reply::None,
            Response::Long => Reply::Long([rd(RESP1), rd(RESP1 + 4), rd(RESP1 + 8), rd(RESP1 + 12)]),
            _ => Reply::Short(rd(RESP1)),
        };
        if response == Response::ShortBusy && !self.wait(BUSY_BUDGET_US, || pins::read(facts::SD_D0_LINE)) {
            fault(rd(STA), 0);
            return Err(Refusal::Busy);
        }
        Ok(reply)
    }

    /// The stream programmed for one transfer: disabled and
    /// its flags cleared first; the FIFO's address, the buffer, the word
    /// count (the peripheral is the flow controller: informational); the FIFO
    /// mode at the full threshold; channel 11, word sizes, memory increment,
    /// the peripheral's four-beat bursts, the memory side's four-beat bursts
    /// where the buffer is 16-byte aligned (a burst never crosses a 1 KB
    /// boundary then) and single beats otherwise; then enabled.
    fn dma_arm(&self, buf: usize, len: usize, to_card: bool) -> Result<(), Refusal> {
        wr(S0CR, rd(S0CR) & !CR_EN);
        if !self.wait(1_000, || rd(S0CR) & CR_EN == 0) {
            fault(rd(STA), rd(DMA_LISR));
            return Err(Refusal::Overrun);
        }
        wr(DMA_LIFCR, S0_FLAGS);
        wr(S0PAR, FIFO as u32);
        wr(S0M0AR, buf as u32);
        wr(S0NDTR, (len / 4) as u32);
        wr(S0FCR, FCR_DMDIS_FULL);
        let mburst = if buf % 16 == 0 { CR_MBURST_INCR4 } else { 0 };
        let dir = if to_card { CR_DIR_M2P } else { 0 };
        wr(S0CR, CR_CHSEL_11 | mburst | CR_PBURST_INCR4 | CR_PL_HIGH | CR_MSIZE_WORD | CR_PSIZE_WORD | CR_MINC | dir | CR_PFCTRL);
        wr(S0CR, rd(S0CR) | CR_EN);
        Ok(())
    }

    /// The stream disabled (EN read back 0) and its flags cleared.
    fn dma_disarm(&self) {
        wr(S0CR, rd(S0CR) & !CR_EN);
        let _ = self.wait(1_000, || rd(S0CR) & CR_EN == 0);
        wr(DMA_LIFCR, S0_FLAGS);
    }

    /// A transfer abandoned: the stream first, the data path's DMAEN after
    /// (the manual's two notes on an underrun or overrun), the flags cleared.
    fn cancel(&self) {
        self.dma_disarm();
        wr(DCTRL, 0);
        settle(DCTRL);
        wr(ICR, ICR_ALL);
    }

    /// The data path: the arguments checked, the detect
    /// line read, the stream armed, DTIMER and DLEN; for a read DCTRL before
    /// the command (DTEN, DTDIR, DMAEN, 512-byte blocks), for a write after
    /// its response; the status polled for DATAEND or an error flag under
    /// the budget (DTIMER's time per block plus the wire time, capped); then
    /// the stream's transfer-complete flag, RXOVERR read once more (the
    /// manual's note) and the receive FIFO empty; the stream disarmed, DCTRL
    /// cleared; CMD12 after a multiple-block command.
    fn transfer(&mut self, index: u8, arg: u32, blocks: u32, buf: usize, len: usize, to_card: bool) -> Result<(), Refusal> {
        if blocks == 0 || blocks > BLK_MAX || len != blocks as usize * BLOCK || buf == 0 || buf % 4 != 0 {
            return Err(Refusal::Argument);
        }
        if self.card_present() == Some(false) {
            return Err(Refusal::NoCard);
        }
        self.dma_arm(buf, len, to_card)?;
        wr(ICR, ICR_ALL);
        let ms = if to_card { WRITE_TIMEOUT_MS } else { READ_TIMEOUT_MS };
        wr(DTIMER, dtimer(self.card_hz(), ms));
        wr(DLEN, len as u32);
        let dctrl = DCTRL_DTEN | DCTRL_DMAEN | DCTRL_DBLOCKSIZE_512 | if to_card { 0 } else { DCTRL_DTDIR_READ };
        if !to_card {
            wr(DCTRL, dctrl);
            settle(DCTRL);
        }
        if let Err(r) = self.command_raw(index, arg, Response::Short) {
            self.cancel();
            return Err(r);
        }
        if to_card {
            wr(DCTRL, dctrl);
            settle(DCTRL);
        }
        let budget_us = blocks.saturating_mul(ms * 1000 + 1_000).saturating_add(20_000);
        let seen = self.wait(budget_us, || rd(STA) & (STA_DATAEND | STA_DATA_ERRORS) != 0);
        let sta = rd(STA);
        if !seen || sta & STA_DATA_ERRORS != 0 {
            fault(sta, rd(DMA_LISR));
            self.cancel();
            return Err(if !seen || sta & STA_DTIMEOUT != 0 {
                Refusal::Timeout
            } else if sta & STA_DCRCFAIL != 0 {
                Refusal::Crc
            } else {
                Refusal::Overrun
            });
        }
        let tc = self.wait(20_000, || rd(DMA_LISR) & (S0_TCIF | S0_TEIF) != 0);
        let lisr = rd(DMA_LISR);
        let sta = rd(STA);
        if !tc || lisr & S0_TEIF != 0 || sta & STA_RXOVERR != 0 || (!to_card && sta & STA_RXDAVL != 0) {
            fault(sta, lisr);
            self.cancel();
            return Err(Refusal::Overrun);
        }
        self.dma_disarm();
        wr(DCTRL, 0);
        settle(DCTRL);
        wr(ICR, ICR_ALL);
        if index == CMD18_READ_MULTIPLE_BLOCK || index == CMD25_WRITE_MULTIPLE_BLOCK {
            self.command_raw(CMD12_STOP_TRANSMISSION, 0, Response::ShortBusy)?;
        }
        Ok(())
    }
}

impl SdHost for Sdmmc2Host {
    /// The bring-up: the cycle counter; the clocks -- the
    /// ports' and DMA2's AHB1 bits, SDMMC2's APB2 bit, the kernel clock
    /// select per posture; the six pads at their alternate functions,
    /// push-pull with NO internal pull (the board's 47 kohm pull-ups serve
    /// CMD and D0 to D3; the clock needs none) at very high speed; the detect
    /// line an input with the chip's pull-up; the controller with no
    /// interrupt and the data path idle, the clock at the identification
    /// rate on one line, then the power on; a millisecond with the clock
    /// running and the command line high (about 400 clocks at this rate:
    /// the 74 the card needs and more).
    fn attach(&mut self) -> Result<(), Refusal> {
        arm_cycle_counter();
        rmw(RCC_AHB1ENR, 0, AHB1ENR_PORTS | AHB1ENR_DMA2EN);
        rmw(RCC_APB2ENR, 0, APB2ENR_SDMMC2EN);
        rmw(RCC_DCKCFGR2, DCKCFGR2_SDMMC2SEL, if SDMMC2SEL_SYSCLK { DCKCFGR2_SDMMC2SEL } else { 0 });
        for &(line, alternate) in facts::SD_PADS.iter() {
            pad_take_as(line, alternate, false, g::PULL_NONE, SPEED_VERY_HIGH);
        }
        pins::configure(facts::SD_DETECT_LINE, g::MODE_INPUT, g::PULL_UP, false);
        wr(MASK, 0);
        self.dma_disarm();
        wr(DCTRL, 0);
        settle(DCTRL);
        wr(POWER, 0);
        settle(POWER);
        wr(CLKCR, CLKCR_CLKEN | CLKCR_HWFC | CLKDIV_IDENT);
        settle(CLKCR);
        self.rate = Rate::Identification;
        wr(POWER, POWER_ON);
        settle(POWER);
        wr(ICR, ICR_ALL);
        self.pause(1_000);
        Ok(())
    }

    /// The detect line: the socket's switch pulls it low with a card seated.
    fn card_present(&mut self) -> Option<bool> {
        Some(!pins::read(facts::SD_DETECT_LINE))
    }

    fn set_bus(&mut self, rate: Rate, width: Width) -> Result<(), Refusal> {
        let div = match rate {
            Rate::Identification => CLKDIV_IDENT,
            Rate::Default => CLKDIV_TRANSFER,
        };
        let wid = match width {
            Width::One => 0,
            Width::Four => CLKCR_WIDBUS_4,
        };
        wr(CLKCR, CLKCR_CLKEN | CLKCR_HWFC | wid | div);
        settle(CLKCR);
        self.rate = rate;
        Ok(())
    }

    fn command(&mut self, index: u8, arg: u32, response: Response) -> Result<Reply, Refusal> {
        #[cfg(feature = "test-sd-doctored")]
        let arg = if index == crate::sdcard::CMD41_SD_SEND_OP_COND { arg & !crate::sdcard::ACMD41_HCS } else { arg };
        self.command_raw(index, arg, response)
    }

    fn read_blocks(&mut self, index: u8, arg: u32, blocks: u32, buf: &mut [u8]) -> Result<(), Refusal> {
        let len = blocks as usize * BLOCK;
        if buf.len() < len {
            return Err(Refusal::Argument);
        }
        self.transfer(index, arg, blocks, buf.as_mut_ptr() as usize, len, false)
    }

    fn write_blocks(&mut self, index: u8, arg: u32, blocks: u32, data: &[u8]) -> Result<(), Refusal> {
        let len = blocks as usize * BLOCK;
        if data.len() < len {
            return Err(Refusal::Argument);
        }
        self.transfer(index, arg, blocks, data.as_ptr() as usize, len, true)
    }

    /// A pause: the thread slept by the tick where the scheduler runs (the
    /// ACMD41 and programming-state polls), a spin by the cycle counter
    /// before it.
    fn pause(&mut self, micros: Micros) {
        if self.yielding {
            sched::sleep_ms(micros.div_ceil(1000).max(1));
        } else {
            self.spin_us(micros);
        }
    }
}
