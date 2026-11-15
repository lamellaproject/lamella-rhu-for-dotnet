//! The board's half of the wireless medium: the wifi driver's bus engine on
//! the RP2350 and the kernel's own clock bound to the medium behind the seam
//! (`crate::wifi_medium`), the network stored on the board read where the
//! lamella tool wrote it, the radio's corner of the flash handed to the
//! medium as it lies there, and the console's lines. A child of the network
//! module: it writes that module's witness and uses its wake word.
//!
//! WHERE THINGS LIE. The last 262144 B of the board's flash device are the
//! radio's corner, and the row's facts state its place: a room for the
//! radio's firmware behind a directory, then two erase units in which the
//! lamella tool stores the developer's network. The kernel reads both in
//! place through the chip's flash window and never writes or erases either.
//! The stored network's name and secret stay where they lie: the medium
//! borrows them from the flash, no line of the console carries them, and
//! the witness below holds counters and the protocol's status codes alone.
//!
//! WHAT THE RADIO TAKES OF THE CHIP. Its four lines (the row's facts; the
//! board claims them on this kernel, so no program can) and PIO block 0,
//! which nothing else in the kernel uses. The driver's engine brings both
//! up at its first use, and the medium asks nothing of the engine before it
//! has found a stored network and checked the firmware in the corner: on a
//! board with no stored network the radio's power line is never raised.
//!
//! THE NETWORK THREAD. The medium is serviced by the thread's passes, at the
//! driver's own cadence, and a frame that arrives between two passes wakes
//! the thread at once. The chip's data line is also its interrupt line, and
//! the line's level-high interrupt at the IO bank is this module's tenancy
//! on the bank's handler, registered with the thread's wake word when the
//! medium is made. The interrupt is armed across the thread's wait and at
//! no other time: the end of a pass arms it where the driver waits on the
//! line, and the next service call begins by disarming it and reading the
//! line's level. While the radio is off the line is never armed and never
//! read. At the end of a pass this module also prints what the medium has
//! to say and shapes the pass's delay: the driver's own wait bounds it from
//! above, and every pass is followed by sleep unless something wakes the
//! thread, so a program keeps its share of the processor while the radio is
//! brought up. At the service call in which the radio starts, this module
//! hands the medium the seed of the spread on the driver's rests behind
//! refused secrets: four bytes of the kernel's software stream, which
//! seeds itself from the chip's hardware source at its first draw.
//!
//! THE PROGRAM'S END. The network is the program's while it runs. When the
//! program ends -- its entry returns, or it calls an exit function -- and
//! when a kernel image without a program ends, the machine's end calls
//! this module before it prints its last line: under the network's
//! tenancy the medium tells the network that it leaves and the radio's
//! power line is lowered, with one line of the console. Inside a terminal
//! report, where the scheduler is not relied on, the power line is lowered
//! by two register writes and nothing else is done; the medium learns it
//! at its next service call, should the network thread run again.

use core::mem::MaybeUninit;
use core::ptr::{addr_of_mut, read_volatile, write_volatile};
use core::sync::atomic::{AtomicU32, Ordering};

use lamella_wifi_cyw4343x::gspi::{Gspi, GspiWire};
use lamella_wifi_cyw4343x::rp2350::{PioWire, ATOMIC_CLR, GPIO24_LEVEL_HIGH, IO_BANK0, PIN_CLOCK, PIN_DATA, PIN_POWER, PIN_SELECT, PROC0_INTE3, SIO, SIO_GPIO_OUT_CLR};
use lamella_wifi_cyw4343x::{Clock, Micros};
use smoltcp::iface::Interface;
use smoltcp::wire::{EthernetAddress, HardwareAddress};

use super::served::with_medium;
use super::{witness, WAKE};
use crate::board::facts;
use crate::board::net_medium::{Link, Medium, RxSide, TxSide};
use crate::critical::with_cs;
use crate::gpio_math::{rp_word, RP_LEVEL_HIGH};
use crate::sched;
use crate::wifi_images::{self, PINS};
use crate::wifi_medium::{End, Stage, Wifi};
use crate::wifi_stored;

/// The chip's flash window: the flash device is mapped from here (the
/// RP2350 datasheet's address map, the execute-in-place region).
const FLASH: usize = 0x1000_0000;
/// The room for the radio's firmware and the erase unit, the row's facts.
const ROOM: usize = facts::RADIO_CORNER_BYTES;
const UNIT: usize = facts::RADIO_CORNER_UNIT;
const _: () = assert!(ROOM == wifi_images::ROOM && UNIT == wifi_images::UNIT && UNIT == wifi_stored::UNIT, "the row's facts of the radio's corner are not the reader's");
const _: () = assert!(facts::RADIO_CORNER_OFFSET % UNIT == 0 && FLASH + facts::RADIO_CORNER_OFFSET + wifi_images::CORNER <= 0x1100_0000, "the radio's corner does not lie inside the first chip select's flash window");
const _: () = assert!(
    PIN_POWER == facts::WIRELESS_POWER_LINE && PIN_DATA == facts::WIRELESS_DATA_LINE && PIN_SELECT == facts::WIRELESS_SELECT_LINE && PIN_CLOCK == facts::WIRELESS_CLOCK_LINE,
    "the radio's bus engine is built for other lines than the row's facts state"
);
const _: () = assert!(
    board_holds(facts::WIRELESS_POWER_LINE) && board_holds(facts::WIRELESS_DATA_LINE) && board_holds(facts::WIRELESS_SELECT_LINE) && board_holds(facts::WIRELESS_CLOCK_LINE),
    "a line of the wireless module is not a board claim on a kernel built with the radio"
);

const fn board_holds(line: u32) -> bool {
    facts::BOARD_CLAIM_BITS[(line / 32) as usize] & (1 << (line % 32)) != 0
}

// The cut writes the power line's bit in the SIO's first output word and
// the data line's interrupt bit in the IO bank's fourth enable register.
const _: () = assert!(PIN_POWER < 32 && PIN_DATA == 24, "the cut's two register writes are written for other lines");

// The bit the board registers for this module at the bank's handler -- the
// data line's level-high bit, by the bank's own arithmetic -- is the one
// the engine arms: the second bit of the fourth enable word.
const _: () = assert!(rp_word(PIN_DATA).0 == 3 && rp_word(PIN_DATA).1 + RP_LEVEL_HIGH == GPIO24_LEVEL_HIGH.trailing_zeros(), "the radio's tenancy registers another bit than the engine arms");

/// Where this module stands for the machine's end: no medium yet (an end
/// before the network's init has nothing to switch off), the medium made,
/// or the power cut inside a terminal report.
static STANDS: AtomicU32 = AtomicU32::new(NO_MEDIUM);
const NO_MEDIUM: u32 = 0;
const MADE: u32 = 1;
const CUT: u32 = 2;

/// While the link is down the stack's own wish to be polled counts for at
/// least this long. A stack that has something to send and no link to send
/// it on asks to be polled at once, for ever; a kernel function that
/// queues work still wakes the thread at once.
const DOWN_MS: u32 = 50;

/// The driver's clock on the kernel's own: the scheduler's nanoseconds in
/// microseconds, and a bounded spin for the settles the driver asks for
/// (never more than 100 microseconds in one call).
pub struct KernelClock;

impl Clock for KernelClock {
    fn now_us(&mut self) -> Micros {
        sched::now_ns() / 1000
    }

    fn delay_us(&mut self, us: u32) {
        let until = sched::now_ns() + us as u64 * 1000;
        while sched::now_ns() < until {
            core::hint::spin_loop();
        }
    }
}

type Radio = Wifi<'static, Gspi<PioWire>, KernelClock>;

/// The radio's witness, read by symbol at a flight: where the medium
/// stands, what it counted, and the last failed join attempt as its class
/// with the protocol's status codes. Counters and codes alone -- nothing of
/// the stored network, nothing the radio heard.
#[repr(C)]
pub struct RadioWitness {
    /// The medium's stage, in the order the medium declares them (0 off).
    pub stage: u32,
    /// The time inside the corner's read and check, summed over its
    /// service calls, in microseconds, and the calls.
    pub check_us: u32,
    pub check_calls: u32,
    /// The medium's own counters.
    pub polls: u32,
    pub joined: u32,
    pub lost: u32,
    pub retried: u32,
    pub dropped: u32,
    pub notes_lost: u32,
    pub service_max_us: u32,
    /// How the radio was switched off at the machine's end: 0 not yet, 1
    /// the network answered the disconnect, 2 it did not within the
    /// bound, 3 no disconnect was asked, 4 the cut of a terminal report.
    pub ended: u32,
    /// The time inside the medium's service calls behind the check,
    /// summed, in microseconds; it wraps.
    pub service_us: u32,
    /// The times the data line's interrupt was armed at the end of a pass,
    /// and the service calls that read the line high where the driver
    /// waited on it.
    pub armed: u32,
    pub line_high: u32,
    /// The last failed join attempt, as the medium's four words: its class
    /// (1 an association failure, 2 the supplicant, 3 a loss during the
    /// attempt, 4 no completing event, 5 the network not seen, 6 a
    /// mismatch) and the status words of the driver's report for it.
    pub failed: [u32; 4],
}

#[unsafe(export_name = "__rhu_wifi_witness")]
pub static mut WITNESS: RadioWitness = RadioWitness { stage: 0, check_us: 0, check_calls: 0, polls: 0, joined: 0, lost: 0, retried: 0, dropped: 0, notes_lost: 0, service_max_us: 0, ended: 0, service_us: 0, armed: 0, line_high: 0, failed: [0; 4] };

/// The row's medium: a handle to the one medium, which lives in a static
/// of its own, so that the boot stack never carries the medium's
/// kilobytes on the way into the stack's record.
pub struct RowMedium {
    wifi: &'static mut Radio,
    /// The data line's interrupt bit is this module's at the bank's
    /// handler. Without the tenancy the bit is never armed, and the medium
    /// is serviced at the driver's cadence alone.
    tenant: bool,
}

impl Medium for RowMedium {
    fn hardware_address(&self) -> Option<[u8; 6]> {
        self.wifi.hardware_address()
    }

    fn link(&self) -> Link {
        self.wifi.link()
    }

    fn wake_word(&self) -> &AtomicU32 {
        self.wifi.wake_word()
    }

    fn service(&mut self, now_ms: u32) {
        // A terminal report cut the radio's power: the medium is down
        // before anything else is done.
        if STANDS.load(Ordering::Acquire) == CUT {
            return self.wifi.cut();
        }
        // Where the driver waits on the data line, the line's interrupt is
        // disarmed for the length of the pass and the line's level is
        // read: a high line polls the driver at this call.
        let line = self.wifi.sample(|bus| {
            let wire = bus.wire_mut();
            wire.arm_irq(false);
            wire.irq_asserted()
        });
        if line == Some(true) {
            unsafe {
                let w = addr_of_mut!(WITNESS);
                write_volatile(addr_of_mut!((*w).line_high), read_volatile(addr_of_mut!((*w).line_high)).wrapping_add(1));
            }
        }
        // The corner's read and check are timed on the kernel's clock: the
        // check's cost at a start is a figure a flight reads.
        if matches!(self.wifi.stage(), Stage::Corner | Stage::Checking) {
            let began = sched::now_ns();
            self.wifi.service(now_ms);
            let spent = ((sched::now_ns() - began) / 1000).min(u32::MAX as u64) as u32;
            unsafe {
                let w = addr_of_mut!(WITNESS);
                write_volatile(addr_of_mut!((*w).check_us), read_volatile(addr_of_mut!((*w).check_us)).saturating_add(spent));
                write_volatile(addr_of_mut!((*w).check_calls), read_volatile(addr_of_mut!((*w).check_calls)).wrapping_add(1));
            }
            // The radio starts: the firmware's check passed and the attach
            // is asked. The spread on the driver's rests behind refused
            // secrets takes its seed here, once, outside the check's time.
            if self.wifi.stage() == Stage::Attaching {
                let mut word = [0u8; 4];
                crate::entropy::fill(&mut word);
                self.wifi.seed(u32::from_le_bytes(word));
            }
        } else {
            self.wifi.service(now_ms);
        }
    }

    fn split(&mut self) -> (&mut dyn RxSide, &mut dyn TxSide) {
        self.wifi.split()
    }

    fn frame_unit(&self) -> usize {
        self.wifi.frame_unit()
    }
}

/// The medium built over the board, once, at the network's init and before
/// any thread runs: the stored network read where the lamella tool wrote
/// it, the corner's room as it lies in flash. Nothing of the radio is
/// touched here; the medium's first service call begins its work.
pub fn row_medium() -> RowMedium {
    static mut RADIO: MaybeUninit<Radio> = MaybeUninit::uninit();
    // The corner's room and the two units behind it, through the flash
    // window: the chip's own read-only mapping of the flash device, which
    // this kernel never writes, alive for as long as the image runs.
    let corner = (FLASH + facts::RADIO_CORNER_OFFSET) as *const u8;
    let (room, first, second) = unsafe { (core::slice::from_raw_parts(corner, ROOM), core::slice::from_raw_parts(corner.add(ROOM), UNIT), core::slice::from_raw_parts(corner.add(ROOM + UNIT), UNIT)) };
    let stored = wifi_stored::current(first, second);
    let bus = Gspi::new(PioWire::new(crate::board::SYSCLK_HZ));
    let wifi = unsafe { (*addr_of_mut!(RADIO)).write(Wifi::new(bus, KernelClock, &WAKE, stored, room, &PINS)) };
    STANDS.store(MADE, Ordering::Release);
    // The data line's interrupt bit registered at the bank's handler with
    // the thread's wake word. Nothing fires until a pass arms the bit.
    let tenant = crate::board::radio_tenant::init(&WAKE) == 0;
    RowMedium { wifi, tenant }
}

/// The radio at the machine's end, called first by both of its functions:
/// in a thread with the scheduler running, or inside a terminal report.
/// Nothing is done before the medium is made. Inside a terminal report the
/// power is cut: the data line's interrupt bit cleared at the IO bank and
/// the power line's bit cleared at the SIO, two writes that are safe in any
/// context, and a word for the medium's next service call. Everywhere else
/// the medium's own end runs under the network's tenancy, so the network
/// thread is outside its pass: the network is told where one is held, the
/// data line's interrupt is disarmed and the power line lowered through the
/// bus's wire, and what the medium has to say is printed. The witness says
/// which end it was, and the first one stands.
pub fn end() {
    if STANDS.load(Ordering::Acquire) == NO_MEDIUM {
        return;
    }
    let (ended, stage) = if crate::funnel::terminal_report() {
        STANDS.store(CUT, Ordering::Release);
        unsafe {
            write_volatile((IO_BANK0 + PROC0_INTE3 + ATOMIC_CLR) as *mut u32, GPIO24_LEVEL_HIGH);
            write_volatile((SIO + SIO_GPIO_OUT_CLR) as *mut u32, 1 << PIN_POWER);
        }
        (Some(End::Cut), None)
    } else {
        with_medium(|medium| {
            let wifi = &mut *medium.wifi;
            let ended = wifi.end(|bus| {
                let wire = bus.wire_mut();
                wire.arm_irq(false);
                wire.set_power(false);
            });
            while let Some(note) = wifi.take_note() {
                kprintln!("{}", note);
            }
            (ended, Some(wifi.stage()))
        })
    };
    unsafe {
        let w = addr_of_mut!(WITNESS);
        if let Some(stage) = stage {
            write_volatile(addr_of_mut!((*w).stage), stage as u32);
        }
        if let (Some(ended), 0) = (ended, read_volatile(addr_of_mut!((*w).ended))) {
            write_volatile(addr_of_mut!((*w).ended), ended as u32);
        }
    }
}

/// Whether the last service call handed the stack a frame, so that another
/// call right after the stack's poll may find the next one.
pub fn staged(medium: &RowMedium) -> bool {
    medium.wifi.more()
}

/// The end of a pass of the network thread, which began at `began_ms`:
/// what the medium has to say is printed, the witness is brought up to
/// date, and the pass's delay is shaped -- while the link is down the
/// stack's own wish counts for at least `DOWN_MS`; the delay is at most the
/// driver's own wait, rounded up to a millisecond; and it is at least the
/// pass's own length and a millisecond more, since the thread takes the
/// pass's length off the delay and would otherwise not sleep at all behind
/// a pass that worked for longer than its delay.
pub fn pass_end(medium: &mut RowMedium, began_ms: u32, delay: &mut u32) {
    while let Some(note) = medium.wifi.take_note() {
        kprintln!("{}", note);
    }
    let counts = medium.wifi.counts();
    unsafe {
        let w = addr_of_mut!(WITNESS);
        write_volatile(addr_of_mut!((*w).stage), medium.wifi.stage() as u32);
        write_volatile(addr_of_mut!((*w).polls), counts.polls);
        write_volatile(addr_of_mut!((*w).joined), counts.joined);
        write_volatile(addr_of_mut!((*w).lost), counts.lost);
        write_volatile(addr_of_mut!((*w).retried), counts.retried);
        write_volatile(addr_of_mut!((*w).dropped), counts.dropped);
        write_volatile(addr_of_mut!((*w).notes_lost), counts.notes_lost);
        write_volatile(addr_of_mut!((*w).service_max_us), counts.service_max_us);
        write_volatile(addr_of_mut!((*w).service_us), counts.service_us);
        write_volatile(addr_of_mut!((*w).failed), counts.failed);
    }
    if medium.wifi.link() == Link::Down {
        *delay = (*delay).max(DOWN_MS);
    }
    if let Some(us) = medium.wifi.wait_us() {
        *delay = (*delay).min(us.div_ceil(1000).min(u32::MAX as Micros) as u32);
    }
    *delay = (*delay).max(sched::now_ms().wrapping_sub(began_ms).saturating_add(1));
    // The data line's interrupt is armed across the thread's wait where
    // the medium says so, as the pass's last act; a cut of the radio's
    // power stands, whenever it came.
    if medium.tenant {
        with_cs(|| {
            if STANDS.load(Ordering::Acquire) == MADE && medium.wifi.arm(|bus| bus.wire_mut().arm_irq(true)) {
                unsafe {
                    let w = addr_of_mut!(WITNESS);
                    write_volatile(addr_of_mut!((*w).armed), read_volatile(addr_of_mut!((*w).armed)).wrapping_add(1));
                }
            }
        });
    }
}

/// The link's rise: the chip's own hardware address adopted on the stack's
/// interface and written to the network's witness, before the address
/// client is started over. An address that is not a station's own is not
/// adopted.
pub fn rise(iface: &mut Interface, medium: &RowMedium) {
    let Some(hw) = medium.hardware_address() else {
        return;
    };
    if !EthernetAddress(hw).is_unicast() {
        return;
    }
    iface.set_hardware_addr(HardwareAddress::Ethernet(EthernetAddress(hw)));
    let mut hw8 = [0u8; 8];
    hw8[..6].copy_from_slice(&hw);
    with_cs(|| unsafe { write_volatile(addr_of_mut!((*witness()).hw), hw8) });
}
