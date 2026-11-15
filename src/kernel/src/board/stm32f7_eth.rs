//! The STM32F7 family's Ethernet controller as a medium of the kernel's
//! network API: the MAC and its DMA (RM0410's Ethernet chapter) over RMII
//! to a LAN8742A transceiver (`phy_lan8742.rs`), behind the medium seam
//! (`net_medium.rs`). The board file states the facts -- the nine lines
//! with their alternate function, the transceiver's management address,
//! the RAM the DMA reaches -- and the arithmetic is `eth_math.rs`'s; the
//! kernel takes no vendor header.
//!
//! THE BRING-UP, at boot before the first thread: the nine lines handed to
//! the controller; the controller held in reset while the RMII is selected
//! and its three clocks are turned on, then released; the DMA's software
//! reset, its wait bounded (the reset needs the transceiver's 50 MHz
//! reference clock, so a wait that runs out names that clock); the
//! statistics counters' interrupts masked; the management clock's divider
//! from the bus clock; the transceiver's identity read before anything is
//! written to it; the transceiver reset only when it has no link; the
//! rings built in their own window of RAM, zeroed here; the DMA set to
//! store and forward both ways with address-aligned beats; the unicast
//! filter at the board's address with broadcast, no multicast. A step that
//! fails is printed with its cause and the medium stays down for the
//! image's life. Nothing here waits on the link.
//!
//! THE LINK, on the network thread: the transceiver is read when the
//! thread passes, 100 ms apart at the closest -- the status register
//! twice, since its link bit keeps a drop until read. A link found up and
//! negotiated is kept as it is; the speed and duplex are read back from
//! the transceiver twice, the controller is set to them, and only then are
//! its transmitter, its receiver and the DMA's two engines started. A link
//! that drops stops them; each engine keeps its place in its ring over the
//! stop, and so does this driver.
//!
//! THE DATA PATH. Four receive and two transmit descriptors, chained, one
//! 1,536-byte buffer each, one frame per buffer. A received frame is handed
//! to the stack in its ring buffer and its descriptor given back to the
//! DMA afterwards; an errored frame or a part of one is dropped and
//! counted. The stack writes a frame straight into the next free transmit
//! buffer; while none is free the stack is told so and tries at its next
//! poll. A data synchronization barrier stands before every write that
//! makes the DMA look at a descriptor.
//!
//! THE INTERRUPT (line 61, the band a kernel critical section masks): the
//! DMA's status read and acknowledged, the wake word bumped, the network
//! thread woken; no frame is touched and nothing is printed there.

use core::mem::MaybeUninit;
use core::ptr::{addr_of_mut, read_volatile, write_volatile};
use core::sync::atomic::{AtomicU32, Ordering};

use crate::board::facts;
use crate::board::net_medium::{Link, Medium, RxSide, TxSide};
use crate::board::phy_lan8742::{self as phy, Decision, Mdio, MdioTimeout, Resolution};
use crate::board::stm32_bus::{pad_take_as, rd, rmw, wr};
use crate::eth_math as m;
use crate::gpio_math as g;
use crate::sched;

// ---------------- the registers ----------------

const ETH: usize = 0x4002_8000;
const MACCR: usize = ETH;
const MACFFR: usize = ETH + 0x04;
const MACMIIAR: usize = ETH + 0x10;
const MACMIIDR: usize = ETH + 0x14;
const MACFCR: usize = ETH + 0x18;
const MACA0HR: usize = ETH + 0x40;
const MACA0LR: usize = ETH + 0x44;
const MMCRIMR: usize = ETH + 0x010C;
const MMCTIMR: usize = ETH + 0x0110;
const DMABMR: usize = ETH + 0x1000;
const DMATPDR: usize = ETH + 0x1004;
const DMARPDR: usize = ETH + 0x1008;
const DMARDLAR: usize = ETH + 0x100C;
const DMATDLAR: usize = ETH + 0x1010;
const DMASR: usize = ETH + 0x1014;
const DMAOMR: usize = ETH + 0x1018;
const DMAIER: usize = ETH + 0x101C;
const DMAMFBOCR: usize = ETH + 0x1020;

const MACCR_FES: u32 = 1 << 14;
const MACCR_DM: u32 = 1 << 11;
const MACCR_TE: u32 = 1 << 3;
const MACCR_RE: u32 = 1 << 2;
/// The management register: the transceiver's address in bits 15 to 11, the
/// register in 10 to 6, the clock range in 4 to 2, write in 1, busy in 0.
const MIIAR_PA_SHIFT: u32 = 11;
const MIIAR_MR_SHIFT: u32 = 6;
const MIIAR_CR_SHIFT: u32 = 2;
const MIIAR_MW: u32 = 1 << 1;
const MIIAR_MB: u32 = 1 << 0;
/// The statistics counters' interrupt masks, every one the two registers
/// name: received good unicast, alignment errors and check errors;
/// transmitted good frames, after more than one collision and after one.
const MMCRIMR_ALL: u32 = 1 << 17 | 1 << 6 | 1 << 5;
const MMCTIMR_ALL: u32 = 1 << 21 | 1 << 15 | 1 << 14;
const DMABMR_SR: u32 = 1 << 0;
/// Address-aligned beats, a fixed burst, 32 beats a burst each way with the
/// receive length stated on its own.
const DMABMR_AAB: u32 = 1 << 25;
const DMABMR_USP: u32 = 1 << 23;
const DMABMR_RDP_32: u32 = 32 << 17;
const DMABMR_FB: u32 = 1 << 16;
const DMABMR_PBL_32: u32 = 32 << 8;
const DMAOMR_RSF: u32 = 1 << 25;
const DMAOMR_TSF: u32 = 1 << 21;
const DMAOMR_FTF: u32 = 1 << 20;
const DMAOMR_ST: u32 = 1 << 13;
const DMAOMR_SR: u32 = 1 << 1;
/// The status bits a write of one clears: bits 16 to 13 and 10 to 0.
const DMASR_CLEARABLE: u32 = 0x0001_E7FF;
/// The interrupts enabled: the normal summary with receive and transmit,
/// and the abnormal summary.
const DMAIER_NISE: u32 = 1 << 16;
const DMAIER_AISE: u32 = 1 << 15;
const DMAIER_RIE: u32 = 1 << 6;
const DMAIER_TIE: u32 = 1 << 0;
/// The missed-frame counter's low half: frames missed for want of a
/// receive buffer, cleared by the read.
const MFBOCR_MFC_MASK: u32 = 0xFFFF;

const RCC_AHB1RSTR: usize = 0x4002_3810;
const AHB1RSTR_ETHMACRST: u32 = 1 << 25;
const RCC_AHB1ENR: usize = 0x4002_3830;
const AHB1ENR_ETHMACEN: u32 = 1 << 25;
const AHB1ENR_ETHMACTXEN: u32 = 1 << 26;
const AHB1ENR_ETHMACRXEN: u32 = 1 << 27;
const AHB1ENR_ETH: u32 = AHB1ENR_ETHMACEN | AHB1ENR_ETHMACTXEN | AHB1ENR_ETHMACRXEN;
const SYSCFG_PMC: usize = 0x4001_3804;
const PMC_MII_RMII_SEL: u32 = 1 << 23;
/// The chip's 96-bit unique identifier.
const UNIQUE_ID: usize = 0x1FF0_F420;

/// The controller's interrupt line and its place in the interrupt
/// controller: the second set-enable word, the priority byte.
pub const IRQ: usize = 61;
const NVIC_ISER1: usize = 0xE000_E104;
const NVIC_IPR: usize = 0xE000_E400;
/// The console's band: masked inside a kernel critical section.
const IRQ_PRIO: u8 = 0xE0;

/// The management clock's divider for this row's bus clock. A posture whose
/// bus clock the manual names no divider for cannot talk to the
/// transceiver, and its network build is refused here.
const MDC: m::MdcDivider = match m::mdc_divider(crate::board::SYSCLK_HZ) {
    Some(d) => d,
    None => panic!("the net feature on this row needs the PLL clock: the Ethernet management clock has no divider for this bus clock (RM0410 names none under 20 MHz), so the 16 MHz internal oscillator posture cannot reach the transceiver"),
};

// ---------------- the rings ----------------

pub const FRAME_UNIT: usize = 1514;
const BUFFER: usize = 1536;
const RX_COUNT: usize = 4;
const TX_COUNT: usize = 2;

#[repr(C, align(8))]
struct Rings {
    rxd: [[u32; 4]; RX_COUNT],
    txd: [[u32; 4]; TX_COUNT],
    rxb: [[u8; BUFFER]; RX_COUNT],
    txb: [[u8; BUFFER]; TX_COUNT],
}

/// The rings' window: its own section of the link map, not loaded and not
/// zeroed by the boot, placed where the DMA reaches.
#[unsafe(link_section = ".rhu_net_rings")]
static mut RINGS: MaybeUninit<Rings> = MaybeUninit::uninit();

fn rings() -> *mut Rings {
    addr_of_mut!(RINGS) as *mut Rings
}

fn rx_desc(i: usize) -> *mut u32 {
    unsafe { addr_of_mut!((*rings()).rxd[i]) as *mut u32 }
}

fn tx_desc(i: usize) -> *mut u32 {
    unsafe { addr_of_mut!((*rings()).txd[i]) as *mut u32 }
}

fn rx_buf(i: usize) -> *mut [u8; BUFFER] {
    unsafe { addr_of_mut!((*rings()).rxb[i]) }
}

fn tx_buf(i: usize) -> *mut [u8; BUFFER] {
    unsafe { addr_of_mut!((*rings()).txb[i]) }
}

/// The barrier before every write that makes the DMA look at a descriptor:
/// the descriptor's words have left the core's write buffer when the DMA
/// reads them.
fn barrier() {
    unsafe { core::arch::asm!("dsb", options(nostack, preserves_flags)) };
}

// ---------------- the counters ----------------

/// The driver's counters, read by symbol: what the wire and the transceiver
/// did, for a check or a probe to read.
#[repr(C)]
pub struct EthWitness {
    /// 0 not brought up, 1 brought up, 2 and above the step that refused.
    pub bring_up: AtomicU32,
    pub phy_id1: AtomicU32,
    pub phy_id2: AtomicU32,
    pub mdc_divisor: AtomicU32,
    pub link_ups: AtomicU32,
    pub link_downs: AtomicU32,
    /// The last resolution: bit 0 set for 100 Mbit, bit 1 for full duplex.
    pub resolution: AtomicU32,
    pub restarts: AtomicU32,
    pub mdio_timeouts: AtomicU32,
    pub flush_timeouts: AtomicU32,
    pub rx_frames: AtomicU32,
    pub rx_errored: AtomicU32,
    pub rx_fragments: AtomicU32,
    pub rx_missed: AtomicU32,
    pub tx_frames: AtomicU32,
    pub tx_errors: AtomicU32,
    pub irqs: AtomicU32,
    pub last_status: AtomicU32,
    pub last_bmsr: AtomicU32,
    pub last_scsr: AtomicU32,
}

#[unsafe(export_name = "__rhu_eth_witness")]
pub static WITNESS: EthWitness = EthWitness {
    bring_up: AtomicU32::new(0),
    phy_id1: AtomicU32::new(0),
    phy_id2: AtomicU32::new(0),
    mdc_divisor: AtomicU32::new(0),
    link_ups: AtomicU32::new(0),
    link_downs: AtomicU32::new(0),
    resolution: AtomicU32::new(0),
    restarts: AtomicU32::new(0),
    mdio_timeouts: AtomicU32::new(0),
    flush_timeouts: AtomicU32::new(0),
    rx_frames: AtomicU32::new(0),
    rx_errored: AtomicU32::new(0),
    rx_fragments: AtomicU32::new(0),
    rx_missed: AtomicU32::new(0),
    tx_frames: AtomicU32::new(0),
    tx_errors: AtomicU32::new(0),
    irqs: AtomicU32::new(0),
    last_status: AtomicU32::new(0),
    last_bmsr: AtomicU32::new(0),
    last_scsr: AtomicU32::new(0),
};

fn count(c: &AtomicU32) {
    c.fetch_add(1, Ordering::Relaxed);
}

/// The word the network thread sleeps on, bumped by the interrupt.
pub static WAKE: AtomicU32 = AtomicU32::new(0);

// ---------------- the waits ----------------

const DWT_CYCCNT: *const u32 = 0xE000_1004 as *const u32;
const CYCLES_PER_US: u32 = crate::board::SYSCLK_HZ / 1_000_000;
/// A management frame is 64 clock periods, about 30 us at this row's
/// divider; the software reset and the FIFO flush end within a few periods
/// of the clocks they wait on.
const MDIO_BUDGET_US: u32 = 1_000;
const DMA_RESET_BUDGET_US: u32 = 50_000;
const FLUSH_BUDGET_US: u32 = 10_000;
/// The closest two readings of the transceiver stand on the network thread.
const PHY_POLL_MS: u32 = 100;
/// The pads' output speed: very high, for the 50 MHz interface.
const SPEED_VERY_HIGH: u32 = 0b11;

/// A bounded spin on the core's cycle counter: true when `done` answered
/// true inside the budget. It never yields, so it serves the boot too.
fn spin_until(budget_us: u32, mut done: impl FnMut() -> bool) -> bool {
    let t0 = unsafe { read_volatile(DWT_CYCCNT) };
    loop {
        if done() {
            return true;
        }
        if unsafe { read_volatile(DWT_CYCCNT) }.wrapping_sub(t0) >= budget_us * CYCLES_PER_US {
            return done();
        }
    }
}

// ---------------- the management interface ----------------

/// The controller's station management: one clause 22 register read or
/// written, the busy bit waited on inside a bound.
struct Mii;

impl Mii {
    fn run(&mut self, phy: u8, reg: u8, write: bool) -> Result<(), MdioTimeout> {
        let word = ((phy as u32) << MIIAR_PA_SHIFT) | ((reg as u32) << MIIAR_MR_SHIFT) | (MDC.code << MIIAR_CR_SHIFT) | if write { MIIAR_MW } else { 0 } | MIIAR_MB;
        wr(MACMIIAR, word);
        if spin_until(MDIO_BUDGET_US, || rd(MACMIIAR) & MIIAR_MB == 0) {
            Ok(())
        } else {
            count(&WITNESS.mdio_timeouts);
            Err(MdioTimeout)
        }
    }
}

impl Mdio for Mii {
    fn read(&mut self, phy: u8, reg: u8) -> Result<u16, MdioTimeout> {
        self.run(phy, reg, false)?;
        Ok(rd(MACMIIDR) as u16)
    }

    fn write(&mut self, phy: u8, reg: u8, value: u16) -> Result<(), MdioTimeout> {
        wr(MACMIIDR, value as u32);
        self.run(phy, reg, true)
    }
}

// ---------------- the two sides ----------------

pub struct RxRing {
    next: usize,
    running: bool,
}

impl RxRing {
    /// The descriptor at the ring's head given back to the DMA, and the
    /// receive engine asked to look (it parks when it finds no descriptor).
    fn give_back(&mut self) {
        let d = rx_desc(self.next);
        let (status, sizes) = m::rx_arm(BUFFER);
        unsafe {
            write_volatile(d.add(1), sizes);
            write_volatile(d, status);
        }
        barrier();
        wr(DMARPDR, 1);
        self.next = (self.next + 1) % RX_COUNT;
    }
}

impl RxSide for RxRing {
    fn peek(&mut self) -> Option<&mut [u8]> {
        if !self.running {
            return None;
        }
        for _ in 0..RX_COUNT {
            let status = unsafe { read_volatile(rx_desc(self.next)) };
            if status & m::RDES0_OWN != 0 {
                return None;
            }
            match m::rx_judge(status, BUFFER) {
                m::RxVerdict::Frame(len) => return Some(unsafe { &mut (&mut *rx_buf(self.next))[..len] }),
                m::RxVerdict::Errored => count(&WITNESS.rx_errored),
                m::RxVerdict::Fragment => count(&WITNESS.rx_fragments),
            }
            self.give_back();
        }
        None
    }

    fn consume(&mut self) {
        count(&WITNESS.rx_frames);
        self.give_back();
    }
}

pub struct TxRing {
    next: usize,
    running: bool,
    /// The descriptors handed to the DMA whose completion was not yet read.
    sent: [bool; TX_COUNT],
}

impl TxRing {
    /// The descriptors the DMA finished: a transmit error counted.
    fn reclaim(&mut self) {
        for i in 0..TX_COUNT {
            if self.sent[i] {
                let status = unsafe { read_volatile(tx_desc(i)) };
                if status & m::TDES0_OWN == 0 {
                    self.sent[i] = false;
                    if status & m::TDES0_ES != 0 {
                        count(&WITNESS.tx_errors);
                    }
                }
            }
        }
    }
}

impl TxSide for TxRing {
    fn room(&self) -> bool {
        self.running && unsafe { read_volatile(tx_desc(self.next)) } & m::TDES0_OWN == 0
    }

    fn slot(&mut self, len: usize) -> Option<&mut [u8]> {
        if len > BUFFER || !self.room() {
            return None;
        }
        Some(unsafe { &mut (&mut *tx_buf(self.next))[..] })
    }

    fn commit(&mut self, len: usize) {
        let d = tx_desc(self.next);
        let (control, sizes) = m::tx_commit(len);
        unsafe {
            write_volatile(d.add(1), sizes);
            write_volatile(d, control);
        }
        barrier();
        wr(DMATPDR, 1);
        self.sent[self.next] = true;
        self.next = (self.next + 1) % TX_COUNT;
        count(&WITNESS.tx_frames);
    }
}

// ---------------- the medium ----------------

#[derive(Clone, Copy, PartialEq, Eq)]
enum State {
    /// The bring-up refused: the medium stays down.
    Dead,
    /// The transceiver's soft reset written at this time, not yet read back.
    Resetting(Option<u32>),
    /// No link, or a negotiation running since this time.
    Down(Option<u32>),
    Up,
}

pub struct Eth {
    rx: RxRing,
    tx: TxRing,
    address: [u8; 6],
    state: State,
    polled_ms: Option<u32>,
    armed: bool,
}

/// The steps a bring-up can refuse at, as the witness's `bring_up` names them.
const REFUSED_WINDOW: u32 = 2;
const REFUSED_RESET: u32 = 3;
const REFUSED_MANAGEMENT: u32 = 4;
const REFUSED_IDENTITY: u32 = 5;

impl Eth {
    /// The medium brought up as the module's head states. The hardware
    /// address stands whether or not the bring-up succeeds.
    pub fn bring_up() -> Self {
        let mut id = [0u8; 12];
        for (i, chunk) in id.chunks_exact_mut(4).enumerate() {
            chunk.copy_from_slice(&rd(UNIQUE_ID + 4 * i).to_le_bytes());
        }
        let mut eth = Eth { rx: RxRing { next: 0, running: false }, tx: TxRing { next: 0, running: false, sent: [false; TX_COUNT] }, address: m::address_from_unique_id(&id), state: State::Dead, polled_ms: None, armed: false };
        match eth.start() {
            Ok(state) => {
                eth.state = state;
                WITNESS.bring_up.store(1, Ordering::Relaxed);
            }
            Err(step) => WITNESS.bring_up.store(step, Ordering::Relaxed),
        }
        eth
    }

    fn start(&mut self) -> Result<State, u32> {
        let (lo, hi) = (rings() as usize, rings() as usize + core::mem::size_of::<Rings>());
        if lo < facts::ETH_DMA_WINDOW.0 || hi > facts::ETH_DMA_WINDOW.1 {
            kprintln!("[boot] eth: REFUSED -- the rings at {:#010x} to {:#010x} are outside the RAM the controller's DMA reaches ({:#010x} to {:#010x})", lo, hi, facts::ETH_DMA_WINDOW.0, facts::ETH_DMA_WINDOW.1);
            return Err(REFUSED_WINDOW);
        }
        super::stm32f7::arm_cycle_counter();
        for &(line, alternate) in facts::ETH_PADS.iter() {
            pad_take_as(line, alternate, false, g::PULL_NONE, SPEED_VERY_HIGH);
        }
        // The controller held in reset with its clocks off while the RMII is selected; the clocks, then the release.
        rmw(facts::RCC_SYSCFG_EN as usize, 0, facts::RCC_SYSCFG_EN_BIT);
        rmw(RCC_AHB1RSTR, 0, AHB1RSTR_ETHMACRST);
        rmw(RCC_AHB1ENR, AHB1ENR_ETH, 0);
        rmw(SYSCFG_PMC, 0, PMC_MII_RMII_SEL);
        rmw(RCC_AHB1ENR, 0, AHB1ENR_ETH);
        rmw(RCC_AHB1RSTR, AHB1RSTR_ETHMACRST, 0);
        wr(DMABMR, DMABMR_SR);
        if !spin_until(DMA_RESET_BUDGET_US, || rd(DMABMR) & DMABMR_SR == 0) {
            kprintln!("[boot] eth: REFUSED -- the controller's software reset did not end in {} ms: no 50 MHz reference clock from the transceiver on PA1", DMA_RESET_BUDGET_US / 1000);
            return Err(REFUSED_RESET);
        }
        // The statistics counters raise the controller's interrupt at half their range and nothing reads them here.
        wr(MMCRIMR, MMCRIMR_ALL);
        wr(MMCTIMR, MMCTIMR_ALL);
        WITNESS.mdc_divisor.store(MDC.divisor, Ordering::Relaxed);
        let phy_at = facts::ETH_PHY_ADDRESS;
        let Ok((id1, id2)) = phy::read_identity(&mut Mii, phy_at) else {
            kprintln!("[boot] eth: REFUSED -- the management interface did not answer inside {} us (the management clock is the bus clock over {})", MDIO_BUDGET_US, MDC.divisor);
            return Err(REFUSED_MANAGEMENT);
        };
        WITNESS.phy_id1.store(id1 as u32, Ordering::Relaxed);
        WITNESS.phy_id2.store(id2 as u32, Ordering::Relaxed);
        if !phy::identity(id1, id2) {
            kprintln!("[boot] eth: REFUSED -- the transceiver at management address {} reads {:04x} {:04x} where the LAN8742A reads 0007 c13x: the address strap, the management wires or the management clock (the bus clock over {})", phy_at, id1, id2, MDC.divisor);
            return Err(REFUSED_IDENTITY);
        }
        kprintln!("[boot] eth: the transceiver at management address {} reads {:04x} {:04x}; the management clock is the bus clock over {}", phy_at, id1, id2, MDC.divisor);
        // The transceiver reset only without a link; a link that is up is left as found.
        let Ok(found) = phy::read_state(&mut Mii, phy_at) else { return Err(REFUSED_MANAGEMENT) };
        let state = if found.decide() == Decision::Down {
            if phy::reset(&mut Mii, phy_at).is_err() {
                return Err(REFUSED_MANAGEMENT);
            }
            State::Resetting(None)
        } else {
            State::Down(None)
        };
        self.build_rings();
        wr(DMABMR, DMABMR_AAB | DMABMR_USP | DMABMR_RDP_32 | DMABMR_FB | DMABMR_PBL_32);
        wr(DMAOMR, DMAOMR_RSF | DMAOMR_TSF);
        // The unicast filter at the board's address (the two words in the one order `address_words` holds),
        // broadcast passed, no multicast; no flow control.
        let (high, low) = m::address_words(&self.address);
        wr(MACA0HR, high);
        wr(MACA0LR, low);
        wr(MACFFR, 0);
        wr(MACFCR, 0);
        Ok(state)
    }

    /// The window zeroed, every descriptor chained to the next and the last
    /// to the first, the receive descriptors handed to the DMA.
    fn build_rings(&mut self) {
        unsafe { core::ptr::write_bytes(rings() as *mut u8, 0, core::mem::size_of::<Rings>()) };
        for i in 0..RX_COUNT {
            let d = rx_desc(i);
            let (status, sizes) = m::rx_arm(BUFFER);
            unsafe {
                write_volatile(d.add(1), sizes);
                write_volatile(d.add(2), rx_buf(i) as u32);
                write_volatile(d.add(3), rx_desc((i + 1) % RX_COUNT) as u32);
                write_volatile(d, status);
            }
        }
        for i in 0..TX_COUNT {
            let d = tx_desc(i);
            unsafe {
                write_volatile(d.add(1), 0);
                write_volatile(d.add(2), tx_buf(i) as u32);
                write_volatile(d.add(3), tx_desc((i + 1) % TX_COUNT) as u32);
                write_volatile(d, m::TDES0_IDLE);
            }
        }
        wr(DMARDLAR, rx_desc(0) as u32);
        wr(DMATDLAR, tx_desc(0) as u32);
    }

    /// The link up: the transmit FIFO flushed (a flush that does not end
    /// leaves the link down for the next reading to try again), the
    /// controller set to the resolution read back with its transmitter and
    /// receiver enabled in the same write, the DMA's two engines started;
    /// the interrupt enabled at the first one.
    fn go_up(&mut self, r: Resolution) {
        wr(DMAOMR, DMAOMR_RSF | DMAOMR_TSF | DMAOMR_FTF);
        if !spin_until(FLUSH_BUDGET_US, || rd(DMAOMR) & DMAOMR_FTF == 0) {
            count(&WITNESS.flush_timeouts);
            return;
        }
        wr(MACCR, if r.hundred { MACCR_FES } else { 0 } | if r.full { MACCR_DM } else { 0 } | MACCR_TE | MACCR_RE);
        if !self.armed {
            self.armed = true;
            wr(DMAIER, DMAIER_NISE | DMAIER_AISE | DMAIER_RIE | DMAIER_TIE);
            unsafe {
                write_volatile((NVIC_IPR + IRQ) as *mut u8, IRQ_PRIO);
                write_volatile(NVIC_ISER1 as *mut u32, 1 << (IRQ - 32));
            }
        }
        barrier();
        wr(DMAOMR, DMAOMR_RSF | DMAOMR_TSF | DMAOMR_ST | DMAOMR_SR);
        self.rx.running = true;
        self.tx.running = true;
        self.state = State::Up;
        WITNESS.resolution.store(r.hundred as u32 | (r.full as u32) << 1, Ordering::Relaxed);
        count(&WITNESS.link_ups);
        kprintln!("[net] eth: link up, {} Mbit {} duplex", if r.hundred { 100 } else { 10 }, if r.full { "full" } else { "half" });
    }

    /// The link down: the two sides closed to the stack, the engines, the
    /// receiver and the transmitter stopped.
    fn go_down(&mut self, now_ms: u32) {
        self.rx.running = false;
        self.tx.running = false;
        wr(DMAOMR, DMAOMR_RSF | DMAOMR_TSF);
        wr(MACCR, rd(MACCR) & !(MACCR_TE | MACCR_RE));
        self.state = State::Down(Some(now_ms));
        count(&WITNESS.link_downs);
        kprintln!("[net] eth: link down");
    }

    /// One reading of the transceiver and what follows from it.
    fn poll_phy(&mut self, now_ms: u32) {
        let phy_at = facts::ETH_PHY_ADDRESS;
        if let State::Resetting(since) = self.state {
            let since = since.unwrap_or(now_ms);
            let done = matches!(Mii.read(phy_at, phy::REG_BMCR), Ok(v) if v & phy::BMCR_RESET == 0);
            self.state = if done || now_ms.wrapping_sub(since) >= phy::RESET_BOUND_MS { State::Down(None) } else { State::Resetting(Some(since)) };
            return;
        }
        let Ok(r) = phy::read_state(&mut Mii, phy_at) else { return };
        WITNESS.last_bmsr.store(r.bmsr_second as u32, Ordering::Relaxed);
        WITNESS.last_scsr.store(r.scsr as u32, Ordering::Relaxed);
        if self.state == State::Up {
            // A link that is gone, or that went and came back since the last reading, is a link lost.
            if !r.link() || r.dropped_since() {
                self.go_down(now_ms);
            }
            return;
        }
        let State::Down(since) = self.state else { return };
        match r.decide() {
            Decision::Keep => {
                if let Ok(Some(resolution)) = phy::read_resolution(&mut Mii, phy_at) {
                    self.go_up(resolution);
                }
            }
            Decision::Wait => {
                let since = since.unwrap_or(now_ms);
                if now_ms.wrapping_sub(since) >= phy::WAIT_BOUND_MS {
                    self.restart(now_ms);
                } else {
                    self.state = State::Down(Some(since));
                }
            }
            Decision::Restart => self.restart(now_ms),
            Decision::Down => self.state = State::Down(None),
        }
    }

    fn restart(&mut self, now_ms: u32) {
        if phy::restart(&mut Mii, facts::ETH_PHY_ADDRESS).is_ok() {
            count(&WITNESS.restarts);
        }
        self.state = State::Down(Some(now_ms));
    }
}

impl Medium for Eth {
    fn hardware_address(&self) -> Option<[u8; 6]> {
        Some(self.address)
    }

    fn link(&self) -> Link {
        if self.state == State::Up { Link::Up } else { Link::Down }
    }

    fn wake_word(&self) -> &AtomicU32 {
        &WAKE
    }

    /// The transmit completions read, the missed-frame counter added to the
    /// witness, and the transceiver read when its last reading is 100 ms old.
    fn service(&mut self, now_ms: u32) {
        if self.state == State::Dead {
            return;
        }
        self.tx.reclaim();
        WITNESS.rx_missed.fetch_add(rd(DMAMFBOCR) & MFBOCR_MFC_MASK, Ordering::Relaxed);
        if self.polled_ms.is_none_or(|t| now_ms.wrapping_sub(t) >= PHY_POLL_MS) {
            self.polled_ms = Some(now_ms);
            self.poll_phy(now_ms);
        }
    }

    fn split(&mut self) -> (&mut dyn RxSide, &mut dyn TxSide) {
        (&mut self.rx, &mut self.tx)
    }

    fn frame_unit(&self) -> usize {
        FRAME_UNIT
    }
}

/// The controller's interrupt: the DMA's status read and acknowledged, the
/// wake word bumped, the network thread woken.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __rhu_stm32f7_eth_irq() {
    let status = rd(DMASR);
    wr(DMASR, status & DMASR_CLEARABLE);
    WITNESS.last_status.store(status, Ordering::Relaxed);
    count(&WITNESS.irqs);
    WAKE.fetch_add(1, Ordering::AcqRel);
    sched::futex_wake(WAKE.as_ptr(), 1);
}
