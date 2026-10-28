//! The block-device doors: the device table a
//! program enumerates, the index-th device's record read at the call, the
//! whole-block reads and writes by number, the sync. The one device
//! is the STM32F769 Discovery board's microSD socket (`mmcblk0`, Linux's
//! name for the first MMC/SD block device) on the chip's SDMMC2 controller
//! through the host of board/stm32f7_sdmmc.rs and the card layer of
//! sdcard.rs; every other row serves no device -- the count answers 0 and
//! every other door RHU_ENODEV -- so the doors exist on every row with one
//! shape. The boot banner's sd line is this module's too.

use crate::sched;

/// The most blocks one door call moves (64 KB).
#[cfg_attr(not(rhu_sd_socket), allow(dead_code))]
pub const BLK_MAX: u32 = 128;
#[cfg_attr(not(rhu_sd_socket), allow(dead_code))]
pub const BLOCK_BYTES: u32 = 512;
#[cfg_attr(not(rhu_sd_socket), allow(dead_code))]
pub const FLAG_REMOVABLE: u32 = 1;
#[cfg_attr(not(rhu_sd_socket), allow(dead_code))]
pub const FLAG_PRESENT: u32 = 2;
#[cfg_attr(not(rhu_sd_socket), allow(dead_code))]
pub const FLAG_WRITE_PROTECTED: u32 = 4;
#[cfg_attr(not(rhu_sd_socket), allow(dead_code))]
pub const FLAG_HIGH_CAPACITY: u32 = 8;
pub const ERR_INVAL: i32 = sched::ERR_INVAL;
pub const ERR_NODEV: i32 = sched::ERR_NODEV;
#[cfg_attr(not(rhu_sd_socket), allow(dead_code))]
pub const ERR_IO: i32 = sched::ERR_IO;
#[cfg_attr(not(rhu_sd_socket), allow(dead_code))]
pub const ERR_TIMEDOUT: i32 = sched::ERR_TIMEDOUT;
/// A write on a write-protected card (RHU_EROFS, Linux's value).
#[cfg_attr(not(rhu_sd_socket), allow(dead_code))]
pub const ERR_ROFS: i32 = -30;

/// The device record the header states (`rhu_blk_desc`).
#[repr(C)]
#[derive(Clone, Copy)]
pub struct BlkDesc {
    pub block_bytes: u32,
    pub blocks: u32,
    pub flags: u32,
    pub name: [u8; 16],
}

#[cfg(rhu_sd_socket)]
mod socket {
    use core::ptr::{addr_of_mut, write_volatile};
    use core::sync::atomic::AtomicU32;

    use super::*;
    use crate::board::stm32f7_sdmmc::{self, Sdmmc2Host};
    use crate::sdcard::{self, Card, Step, Stuck, Why};
    use crate::sdhost::{Refusal, SdHost, Width};

    /// The one device's tenancy word.
    static TENANCY: AtomicU32 = AtomicU32::new(0);

    struct Device {
        host: Sdmmc2Host,
        card: Option<Card>,
        /// The identification's refusal, remembered until the card leaves.
        refused: Option<Stuck>,
    }
    static mut DEVICE: Device = Device { host: Sdmmc2Host::new(), card: None, refused: None };

    /// The witness record, read by symbol: the identifications and refusals
    /// with the last refusal's step and reason as the enums' ordinals, the
    /// transfers and the blocks moved each way.
    #[repr(C)]
    #[allow(dead_code)] // written through its pointer, read by symbol
    pub struct SdWitness {
        pub identified: u32,
        pub refusals: u32,
        pub last_step: u32,
        pub last_why: u32,
        pub reads: u32,
        pub writes: u32,
        pub blocks_read: u32,
        pub blocks_written: u32,
        /// The last identification's cycles.
        pub ident_cycles: u32,
        /// The host's status word and its stream's flag word at the last refusal
        /// (stm32f7_sdmmc::last_flags: a fault names its flag here).
        pub last_sta: u32,
        pub last_lisr: u32,
    }
    #[unsafe(no_mangle)]
    pub static mut __rhu_sd_witness: SdWitness = SdWitness { identified: 0, refusals: 0, last_step: 0, last_why: 0, reads: 0, writes: 0, blocks_read: 0, blocks_written: 0, ident_cycles: 0, last_sta: 0, last_lisr: 0 };

    fn bump(field: *mut u32, by: u32) {
        unsafe { write_volatile(field, field.read_volatile().wrapping_add(by)) };
    }

    fn note(s: &Stuck) {
        let w = addr_of_mut!(__rhu_sd_witness);
        unsafe {
            bump(addr_of_mut!((*w).refusals), 1);
            write_volatile(addr_of_mut!((*w).last_step), s.step as u32);
            write_volatile(addr_of_mut!((*w).last_why), why_ordinal(&s.why));
            let (sta, lisr) = stm32f7_sdmmc::last_flags();
            write_volatile(addr_of_mut!((*w).last_sta), sta);
            write_volatile(addr_of_mut!((*w).last_lisr), lisr);
        }
    }

    fn why_ordinal(w: &Why) -> u32 {
        match w {
            Why::Refused(Refusal::Timeout) => 1,
            Why::Refused(Refusal::Crc) => 2,
            Why::Refused(Refusal::Busy) => 3,
            Why::Refused(Refusal::Overrun) => 4,
            Why::Refused(Refusal::NoCard) => 5,
            Why::Refused(Refusal::Argument) => 6,
            Why::NoCard => 7,
            Why::Busy => 8,
            Why::Echo => 9,
            Why::Geometry => 10,
            Why::Unsupported => 11,
            Why::ReadOnly => 12,
            Why::Argument => 13,
        }
    }

    /// The step's word for the banner ("card refused at <step>").
    pub fn step_name(step: Step) -> &'static str {
        match step {
            Step::Attach => "attach",
            Step::Bus => "bus",
            Step::Cmd0 => "cmd0",
            Step::Cmd8 => "cmd8",
            Step::Acmd41 => "acmd41",
            Step::Cmd2 => "cmd2",
            Step::Cmd3 => "cmd3",
            Step::Cmd9 => "cmd9",
            Step::Csd => "csd",
            Step::Cmd7 => "cmd7",
            Step::Acmd6 => "acmd6",
            Step::Cmd16 => "cmd16",
            Step::Transfer => "transfer",
            Step::Cmd13 => "cmd13",
        }
    }

    /// The reason's word (the api flavor's lines).
    #[cfg(feature = "api-smoke")]
    pub fn why_name(w: &Why) -> &'static str {
        match w {
            Why::Refused(Refusal::Timeout) => "the controller timed out",
            Why::Refused(Refusal::Crc) => "a CRC failed",
            Why::Refused(Refusal::Busy) => "the data line stayed busy",
            Why::Refused(Refusal::Overrun) => "the FIFO lost pace",
            Why::Refused(Refusal::NoCard) | Why::NoCard => "no card",
            Why::Refused(Refusal::Argument) | Why::Argument => "an argument",
            Why::Busy => "the card never left busy",
            Why::Echo => "an unexpected response",
            Why::Geometry => "an impossible geometry or capacity class",
            Why::Unsupported => "a card this kernel does not serve",
            Why::ReadOnly => "write-protected",
        }
    }

    /// The header's code for a refusal: a transfer past its budget (the
    /// controller's timeout at the transfer or at its status poll, the
    /// programming wait's budget) is the timeout code; a refused
    /// identification is an I/O error whatever its reason.
    fn code_of(s: &Stuck) -> i32 {
        let transfer = matches!(s.step, Step::Transfer | Step::Cmd13);
        match s.why {
            Why::Refused(Refusal::Timeout) if transfer => ERR_TIMEDOUT,
            Why::Busy if s.step == Step::Cmd13 => ERR_TIMEDOUT,
            Why::Refused(Refusal::NoCard) | Why::NoCard => ERR_NODEV,
            Why::ReadOnly => ERR_ROFS,
            Why::Refused(Refusal::Argument) | Why::Argument => ERR_INVAL,
            _ => ERR_IO,
        }
    }

    /// The card as identified, or the reason it is not: the detect line
    /// read now; a card gone drops the identification and forgets a
    /// refusal; a card present and unidentified is identified here (the
    /// host attached anew), a refusal remembered until the card leaves.
    fn ensure<'a>(host: &mut Sdmmc2Host, card: &'a mut Option<Card>, refused: &mut Option<Stuck>) -> Result<&'a Card, i32> {
        if host.card_present() == Some(false) {
            *card = None;
            *refused = None;
            return Err(ERR_NODEV);
        }
        if card.is_none() {
            if let Some(s) = refused {
                return Err(code_of(s));
            }
            let t0 = stm32f7_sdmmc::cycles();
            let r = sdcard::identify(host);
            unsafe { write_volatile(addr_of_mut!((*addr_of_mut!(__rhu_sd_witness)).ident_cycles), stm32f7_sdmmc::cycles().wrapping_sub(t0)) };
            match r {
                Ok(c) => {
                    *card = Some(c);
                    bump(unsafe { addr_of_mut!((*addr_of_mut!(__rhu_sd_witness)).identified) }, 1);
                }
                Err(s) => {
                    note(&s);
                    *refused = Some(s);
                    return Err(code_of(&s));
                }
            }
        }
        Ok(card.as_ref().unwrap())
    }

    /// The device under its tenancy for one door call; the tenancy's own
    /// code when the wait fails (it does not, at no deadline).
    fn with_device<R>(f: impl FnOnce(&mut Device) -> R) -> Result<R, i32> {
        crate::bus::tenancy_take(&TENANCY, sched::DEADLINE_NONE)?;
        let r = f(unsafe { &mut *addr_of_mut!(DEVICE) });
        crate::bus::tenancy_give(&TENANCY);
        Ok(r)
    }

    fn describe(present: bool, card: Option<&Card>) -> BlkDesc {
        let mut flags = FLAG_REMOVABLE | if present { FLAG_PRESENT } else { 0 };
        let mut blocks = 0;
        if let Some(c) = card {
            blocks = c.capacity_blocks;
            if c.write_protected {
                flags |= FLAG_WRITE_PROTECTED;
            }
            if c.high_capacity {
                flags |= FLAG_HIGH_CAPACITY;
            }
        }
        let mut name = [0u8; 16];
        name[..7].copy_from_slice(b"mmcblk0");
        BlkDesc { block_bytes: BLOCK_BYTES, blocks, flags, name }
    }

    /// The boot banner: the card
    /// identified before the scheduler and the tick (the
    /// host spinning, no tenancy taken: nothing else runs), the line one of
    /// three -- the card with its class, size, lines and clock; no card; the
    /// refusal's step. A refused card is not a boot death. The host yields
    /// from here on.
    pub fn boot() {
        let Device { host, card, refused } = unsafe { &mut *addr_of_mut!(DEVICE) };
        host.yielding = false;
        match ensure(host, card, refused) {
            Ok(c) => {
                // Tenths of a gibibyte: blocks x 512 x 10 / 2^30 = blocks x 5 / 2^20.
                let tenths = (c.capacity_blocks as u64 * 5) >> 20;
                kprintln!(
                    "[boot] sd: {} {}.{} GB on SDMMC2 ({} at {} MHz)",
                    if c.high_capacity { "SDHC" } else { "SDSC" },
                    tenths / 10,
                    tenths % 10,
                    if c.width == Width::Four { "4 lines" } else { "1 line" },
                    stm32f7_sdmmc::CARD_HZ_TRANSFER / 1_000_000
                );
            }
            Err(_) => match refused {
                Some(s) => kprintln!("[boot] sd: card refused at {}", step_name(s.step)),
                None => kprintln!("[boot] sd: no card in the socket"),
            },
        }
        host.yielding = true;
    }

    pub fn count() -> i32 {
        1
    }

    pub fn device(index: u32, out: *mut BlkDesc) -> i32 {
        if index != 0 || out.is_null() || out as usize % 4 != 0 {
            return ERR_INVAL;
        }
        let rec = with_device(|d| {
            let Device { host, card, refused } = d;
            let present = host.card_present() != Some(false);
            let _ = ensure(host, card, refused);
            describe(present, card.as_ref())
        });
        match rec {
            Ok(r) => {
                unsafe { write_volatile(out, r) };
                0
            }
            Err(code) => code,
        }
    }

    /// A transfer's outcome into the door's answer: the blocks moved, or
    /// the code; a refusal for no card drops the identification.
    fn finish(r: Result<(), Stuck>, card: &mut Option<Card>, blocks: u32, moved: *mut u32, calls: *mut u32) -> i32 {
        match r {
            Ok(()) => {
                bump(calls, 1);
                bump(moved, blocks);
                blocks as i32
            }
            Err(s) => {
                note(&s);
                if matches!(s.why, Why::Refused(Refusal::NoCard) | Why::NoCard) {
                    *card = None;
                }
                code_of(&s)
            }
        }
    }

    fn range_ok(c: &Card, lba: u32, blocks: u32) -> bool {
        lba.checked_add(blocks).is_some_and(|end| end <= c.capacity_blocks)
    }

    pub fn read(dev: u32, lba: u32, blocks: u32, buf: *mut u8) -> i32 {
        if dev != 0 {
            return ERR_NODEV;
        }
        if buf.is_null() || buf as usize % 4 != 0 || blocks == 0 || blocks > BLK_MAX {
            return ERR_INVAL;
        }
        let len = blocks as usize * BLOCK_BYTES as usize;
        let r = with_device(|d| {
            let Device { host, card, refused } = d;
            let c = *ensure(host, card, refused)?;
            if !range_ok(&c, lba, blocks) {
                return Err(ERR_INVAL);
            }
            let slice = unsafe { core::slice::from_raw_parts_mut(buf, len) };
            let w = addr_of_mut!(__rhu_sd_witness);
            Ok(finish(sdcard::read(host, &c, lba, blocks, slice), card, blocks, unsafe { addr_of_mut!((*w).blocks_read) }, unsafe { addr_of_mut!((*w).reads) }))
        });
        match r {
            Ok(Ok(n)) => n,
            Ok(Err(code)) | Err(code) => code,
        }
    }

    pub fn write(dev: u32, lba: u32, blocks: u32, buf: *const u8) -> i32 {
        if dev != 0 {
            return ERR_NODEV;
        }
        if buf.is_null() || buf as usize % 4 != 0 || blocks == 0 || blocks > BLK_MAX {
            return ERR_INVAL;
        }
        let len = blocks as usize * BLOCK_BYTES as usize;
        let r = with_device(|d| {
            let Device { host, card, refused } = d;
            let c = *ensure(host, card, refused)?;
            if !range_ok(&c, lba, blocks) {
                return Err(ERR_INVAL);
            }
            if c.write_protected {
                return Err(ERR_ROFS);
            }
            let slice = unsafe { core::slice::from_raw_parts(buf, len) };
            let w = addr_of_mut!(__rhu_sd_witness);
            Ok(finish(sdcard::write(host, &c, lba, blocks, slice), card, blocks, unsafe { addr_of_mut!((*w).blocks_written) }, unsafe { addr_of_mut!((*w).writes) }))
        });
        match r {
            Ok(Ok(n)) => n,
            Ok(Err(code)) | Err(code) => code,
        }
    }

    /// The card's programming state waited out (a no-op after the
    /// synchronous writes; the seam for an engine that queues).
    pub fn sync(dev: u32) -> i32 {
        if dev != 0 {
            return ERR_NODEV;
        }
        let r = with_device(|d| {
            let Device { host, card, refused } = d;
            let c = *ensure(host, card, refused)?;
            sdcard::wait_programmed(host, &c, 1).map_err(|s| {
                note(&s);
                code_of(&s)
            })
        });
        match r {
            Ok(Ok(())) => 0,
            Ok(Err(code)) | Err(code) => code,
        }
    }

    /// The identified card's facts for the kernel's own checks (the api
    /// flavor): None with no card or a refused one.
    #[cfg(all(feature = "api-smoke", not(feature = "test-sd-doctored")))]
    pub fn with_card<R>(f: impl FnOnce(&Card) -> R) -> Option<R> {
        with_device(|d| {
            let Device { host, card, refused } = d;
            ensure(host, card, refused).ok().map(f)
        })
        .ok()
        .flatten()
    }

    /// The last identification's cycles and the host's yield count (the api
    /// flavor's measurement lines).
    #[cfg(all(feature = "api-smoke", not(feature = "test-sd-doctored")))]
    pub fn ident_cycles() -> u32 {
        unsafe { addr_of_mut!((*addr_of_mut!(__rhu_sd_witness)).ident_cycles).read_volatile() }
    }
    #[cfg(all(feature = "api-smoke", not(feature = "test-sd-doctored")))]
    pub fn yields() -> u32 {
        stm32f7_sdmmc::YIELDS.load(core::sync::atomic::Ordering::Relaxed)
    }

    /// The filesystem layer's seam over the identified card: a unit struct over
    /// the doors' own paths under the device's tenancy, the header's codes mapped
    /// to the layer's errors; the api checks are its caller.
    #[cfg(all(feature = "api-smoke", not(feature = "test-sd-doctored")))]
    pub struct SocketDevice;

    #[cfg(all(feature = "api-smoke", not(feature = "test-sd-doctored")))]
    impl crate::fat::BlockDevice for SocketDevice {
        fn blocks(&mut self) -> u32 {
            with_card(|c| c.capacity_blocks).unwrap_or(0)
        }

        fn read(&mut self, lba: u32, buf: &mut [u8]) -> Result<(), crate::fat::Error> {
            fat_code(read(0, lba, (buf.len() / BLOCK_BYTES as usize) as u32, buf.as_mut_ptr()))
        }

        fn write(&mut self, lba: u32, data: &[u8]) -> Result<(), crate::fat::Error> {
            fat_code(write(0, lba, (data.len() / BLOCK_BYTES as usize) as u32, data.as_ptr()))
        }

        fn sync(&mut self) -> Result<(), crate::fat::Error> {
            fat_code(sync(0))
        }
    }

    /// A door's answer as the layer's result: the blocks moved (or 0) Ok, a code its error.
    #[cfg(all(feature = "api-smoke", not(feature = "test-sd-doctored")))]
    fn fat_code(r: i32) -> Result<(), crate::fat::Error> {
        use crate::fat::Error;
        if r >= 0 {
            Ok(())
        } else {
            Err(match r {
                ERR_TIMEDOUT => Error::Timeout,
                ERR_NODEV => Error::NoDevice,
                ERR_ROFS => Error::ReadOnly,
                _ => Error::Io,
            })
        }
    }

    /// The witness's counters for the api flavor's lines (refusals, the last step
    /// and reason as ordinals, writes, blocks written, the host's status word and
    /// the stream's flag word at the last refusal).
    #[cfg(all(feature = "api-smoke", not(feature = "test-sd-doctored")))]
    pub fn witness_snapshot() -> (u32, u32, u32, u32, u32, u32, u32) {
        let w = addr_of_mut!(__rhu_sd_witness);
        unsafe {
            let f = |p: *const u32| p.read_volatile();
            (f(core::ptr::addr_of!((*w).refusals)), f(core::ptr::addr_of!((*w).last_step)), f(core::ptr::addr_of!((*w).last_why)), f(core::ptr::addr_of!((*w).writes)), f(core::ptr::addr_of!((*w).blocks_written)), f(core::ptr::addr_of!((*w).last_sta)), f(core::ptr::addr_of!((*w).last_lisr)))
        }
    }

    /// The remembered refusal's words (the api flavor's control).
    #[cfg(feature = "api-smoke")]
    pub fn refusal_words() -> (&'static str, &'static str) {
        with_device(|d| match d.refused {
            Some(s) => (step_name(s.step), why_name(&s.why)),
            None => ("none", "none"),
        })
        .unwrap_or(("none", "none"))
    }
}

#[cfg(rhu_sd_socket)]
pub use socket::*;

/// Every other row: no device, the doors with one shape.
#[cfg(not(rhu_sd_socket))]
mod none {
    use super::*;

    pub fn count() -> i32 {
        0
    }

    pub fn device(_index: u32, _out: *mut BlkDesc) -> i32 {
        ERR_INVAL
    }

    pub fn read(_dev: u32, _lba: u32, _blocks: u32, _buf: *mut u8) -> i32 {
        ERR_NODEV
    }

    pub fn write(_dev: u32, _lba: u32, _blocks: u32, _buf: *const u8) -> i32 {
        ERR_NODEV
    }

    pub fn sync(_dev: u32) -> i32 {
        ERR_NODEV
    }
}

#[cfg(not(rhu_sd_socket))]
pub use none::*;
