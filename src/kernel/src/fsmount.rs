//! The kernel's file API's socket half: the file family's table of fs.rs
//! over the board's card socket (blk.rs's device under the FAT layer), the
//! family's tenancy, the mount thread that follows the socket's detect
//! line, the mount at boot with the banner's clause, and the function
//! bodies api.rs serves to a hosted program. On a row with a socket the
//! table stands in a static of its own (the volume, the two handle tables,
//! a 4 KB scratch for the mount's pass over the FAT); on every other row,
//! and on the STM32F769 row's doctored control image, the functions exist
//! with one shape: no mount, no handle, every path absent.

#[cfg(all(rhu_sd_socket, not(feature = "test-sd-doctored")))]
mod socket {
    use core::ptr::{addr_of_mut, write_volatile};
    use core::sync::atomic::{AtomicU32, Ordering};
    use crate::board::sd;
    use crate::fs::{self, Dirent, MountDesc, StatDesc, StatfsDesc, Table};
    use crate::{blk, calendar, calendar_math, sched};

    /// The family's tenancy word.
    static TENANCY: AtomicU32 = AtomicU32::new(0);
    static mut TABLE: Table<blk::SocketDevice> = Table::new();
    /// The mount's scratch: eight sectors per device call of the free pass, aligned for the hosts' bursts.
    #[repr(C, align(16))]
    struct Scratch([u8; 4096]);
    static mut SCRATCH: Scratch = Scratch([0; 4096]);
    /// The last mount's time in the host's ticks (the core's cycles on the STM32F769 row; 0 where the host counts nothing at boot).
    static MOUNT_TICKS: AtomicU32 = AtomicU32::new(0);
    /// The mount thread: one notch more urgent than the hosted flat level, its stack.
    pub const THREAD_PRIO: u8 = 0x7F;
    pub const THREAD_STACK: usize = 6 * 1024;
    /// The detect line's poll and its debounce: a level read three times in a row, 20 ms apart, is the stable level.
    pub const POLL_MS: u32 = 20;
    pub const DEBOUNCE: u32 = 3;
    /// A card's arrival and the mount: the first attempt waits this many polls
    /// after the switch reads a card (the card settles and powers up after the
    /// switch closes), a refused attempt is tried again after this many polls,
    /// and the attempts since the card arrived are bounded; after the last the
    /// record stays unreadable until the card leaves.
    pub const SETTLE_POLLS: u32 = 12;
    pub const RETRY_POLLS: u32 = 25;
    pub const MOUNT_TRIES: u32 = 8;

    /// The layer's clock seam: the calendar's reading when a source set it, else None (the FAT floor).
    fn now_civil() -> Option<calendar_math::Civil> {
        let (ticks, source) = calendar::get();
        if source == calendar::SOURCE_UNSET {
            None
        } else {
            Some(calendar_math::ticks_to_civil(ticks))
        }
    }
    /// The table under the family's tenancy for one call; the tenancy's own code when the wait fails (it does not, at no deadline).
    fn with_table<R>(f: impl FnOnce(&mut Table<blk::SocketDevice>) -> R) -> Result<R, i32> {
        crate::bus::tenancy_take(&TENANCY, sched::DEADLINE_NONE)?;
        let r = f(unsafe { &mut *addr_of_mut!(TABLE) });
        crate::bus::tenancy_give(&TENANCY);
        Ok(r)
    }
    /// The table's mount through the static scratch, timed in the host's ticks.
    fn try_mount(t: &mut Table<blk::SocketDevice>) -> Result<(), crate::fat::Refused> {
        let t0 = sd::ticks();
        let r = t.mount(blk::SocketDevice, unsafe { &mut (*addr_of_mut!(SCRATCH)).0 }, now_civil);
        MOUNT_TICKS.store(sd::ticks().wrapping_sub(t0), Ordering::Relaxed);
        r
    }
    /// The mount at boot, called by the block layer's boot banner after the
    /// card's identification, in its spin posture with nothing else running
    /// (no tenancy: the table is this call's alone): the FAT width's digits
    /// for the banner's clause, or the refusal's words.
    pub fn boot_mount() -> Result<&'static str, &'static str> {
        let t = unsafe { &mut *addr_of_mut!(TABLE) };
        match try_mount(t) {
            Ok(()) => Ok(width_words(t.fat_width())),
            Err(r) => Err(r.words()),
        }
    }
    fn width_words(w: u32) -> &'static str {
        match w {
            12 => "12",
            16 => "16",
            _ => "32",
        }
    }
    /// The last mount's ticks (the api checks' line).
    #[cfg(feature = "api-smoke")]
    pub fn mount_ticks() -> u32 {
        MOUNT_TICKS.load(Ordering::Relaxed)
    }
    /// The mount thread spawned (after the scheduler's init).
    pub fn start() {
        sched::spawn("sdmount", watch, 0, THREAD_PRIO, THREAD_STACK);
    }
    /// The thread: the detect line read every 20 ms, a level stable after
    /// three agreeing reads; a stable card over no volume mounts -- the first
    /// attempt after the settle, a refused attempt tried again after the
    /// retry pause with the block layer's remembered refusal forgotten and the
    /// card identified anew, the attempts bounded -- and a stable absence over
    /// a volume (or a remembered refusal) unmounts, each under the family's
    /// tenancy; one line per mount, per refusal and per removal.
    extern "C" fn watch(_: usize) {
        let mut last = blk::card_seated();
        let mut agree = 1u32;
        let mut stable = last;
        let mut since = 0u32;
        let mut tries = 0u32;
        loop {
            sched::sleep_ms(POLL_MS);
            let s = blk::card_seated();
            if s == last {
                agree = agree.saturating_add(1);
            } else {
                last = s;
                agree = 1;
            }
            if agree >= DEBOUNCE && stable != s {
                stable = s;
                since = 0;
                tries = 0;
            }
            since = since.saturating_add(1);
            let _ = with_table(|t| {
                if stable && t.state() != fs::STATE_MOUNTED && tries < MOUNT_TRIES && since >= if tries == 0 { SETTLE_POLLS } else { RETRY_POLLS } {
                    since = 0;
                    tries += 1;
                    if t.state() == fs::STATE_UNREADABLE {
                        t.unmount();
                        blk::forget_refusal();
                    }
                    match try_mount(t) {
                        Ok(()) => kprintln!("[sd] FAT{} mounted at /media/sd0", width_words(t.fat_width())),
                        Err(r) => {
                            if t.state() == fs::STATE_UNREADABLE {
                                kprintln!("[sd] no volume mounted at /media/sd0 ({}; attempt {} of {})", r.words(), tries, MOUNT_TRIES);
                            }
                        }
                    }
                } else if !stable && t.state() != fs::STATE_NO_CARD {
                    t.unmount();
                    kprintln!("[sd] /media/sd0 unmounted: the card left");
                }
            });
        }
    }
    /// The family's tenancy held and released by a caller that reads the
    /// volume itself across several calls (the api checks of the layer).
    #[cfg(feature = "api-smoke")]
    pub fn hold() {
        let _ = crate::bus::tenancy_take(&TENANCY, sched::DEADLINE_NONE);
    }
    #[cfg(feature = "api-smoke")]
    pub fn release() {
        crate::bus::tenancy_give(&TENANCY);
    }
    /// The mounted volume for a holder of the tenancy, or None with no volume.
    #[cfg(feature = "api-smoke")]
    pub fn volume() -> Option<&'static mut crate::fat::Volume<blk::SocketDevice>> {
        unsafe { (*addr_of_mut!(TABLE)).volume() }
    }
    /// The table's state and the refused mount's words, read without the tenancy (one word each).
    #[cfg(feature = "api-smoke")]
    pub fn state() -> u32 {
        unsafe { (*addr_of_mut!(TABLE)).state() }
    }
    #[cfg(feature = "api-smoke")]
    pub fn why() -> &'static str {
        unsafe { (*addr_of_mut!(TABLE)).why() }
    }

    /// A C path as the family reads it: NUL-terminated within the window; a NULL pointer invalid, no terminator too long.
    unsafe fn path_slice<'a>(p: *const u8) -> Result<&'a [u8], i32> {
        if p.is_null() {
            return Err(fs::ERR_INVAL);
        }
        for i in 0..=fs::PATH_MAX {
            if unsafe { *p.add(i) } == 0 {
                return Ok(unsafe { core::slice::from_raw_parts(p, i) });
            }
        }
        Err(fs::ERR_NAMETOOLONG)
    }
    /// A record pointer as the family writes it: NULL or misaligned invalid.
    fn record_ok<T>(out: *mut T) -> bool {
        !out.is_null() && out as usize % 4 == 0
    }
    fn answer(r: Result<i32, i32>) -> i32 {
        r.unwrap_or_else(|c| c)
    }
    pub fn mount_count() -> i32 {
        1
    }
    pub fn mount_desc(index: u32, out: *mut MountDesc) -> i32 {
        if !record_ok(out) {
            return fs::ERR_INVAL;
        }
        let mut d = MountDesc::EMPTY;
        let r = answer(with_table(|t| t.mount_desc(index, &mut d)));
        if r == 0 {
            unsafe { write_volatile(out, d) };
        }
        r
    }
    pub fn open(path: *const u8, flags: u32) -> i32 {
        let p = match unsafe { path_slice(path) } {
            Ok(p) => p,
            Err(c) => return c,
        };
        answer(with_table(|t| t.open(p, flags)))
    }
    pub fn close(h: i32) -> i32 {
        answer(with_table(|t| t.close(h)))
    }
    pub fn read(h: i32, buf: *mut u8, n: u32) -> i32 {
        if n != 0 && buf.is_null() {
            return fs::ERR_INVAL;
        }
        let b = if n == 0 { &mut [][..] } else { unsafe { core::slice::from_raw_parts_mut(buf, n.min(i32::MAX as u32) as usize) } };
        answer(with_table(|t| t.read(h, b)))
    }
    pub fn write(h: i32, buf: *const u8, n: u32) -> i32 {
        if n != 0 && buf.is_null() {
            return fs::ERR_INVAL;
        }
        let b = if n == 0 { &[][..] } else { unsafe { core::slice::from_raw_parts(buf, n.min(i32::MAX as u32) as usize) } };
        answer(with_table(|t| t.write(h, b)))
    }
    pub fn pread(h: i32, buf: *mut u8, n: u32, offset: i64) -> i32 {
        if n != 0 && buf.is_null() {
            return fs::ERR_INVAL;
        }
        let b = if n == 0 { &mut [][..] } else { unsafe { core::slice::from_raw_parts_mut(buf, n.min(i32::MAX as u32) as usize) } };
        answer(with_table(|t| t.pread(h, b, offset)))
    }
    pub fn pwrite(h: i32, buf: *const u8, n: u32, offset: i64) -> i32 {
        if n != 0 && buf.is_null() {
            return fs::ERR_INVAL;
        }
        let b = if n == 0 { &[][..] } else { unsafe { core::slice::from_raw_parts(buf, n.min(i32::MAX as u32) as usize) } };
        answer(with_table(|t| t.pwrite(h, b, offset)))
    }
    pub fn seek(h: i32, offset: i64, whence: u32, pos: *mut i64) -> i32 {
        if !record_ok(pos) {
            return fs::ERR_INVAL;
        }
        let mut p = 0i64;
        let r = answer(with_table(|t| t.seek(h, offset, whence, &mut p)));
        if r == 0 {
            unsafe { write_volatile(pos, p) };
        }
        r
    }
    pub fn truncate(h: i32, len: i64) -> i32 {
        answer(with_table(|t| t.truncate(h, len)))
    }
    pub fn sync(h: i32) -> i32 {
        answer(with_table(|t| t.sync(h)))
    }
    pub fn fstat(h: i32, out: *mut StatDesc) -> i32 {
        if !record_ok(out) {
            return fs::ERR_INVAL;
        }
        let mut d = StatDesc::EMPTY;
        let r = answer(with_table(|t| t.fstat(h, &mut d)));
        if r == 0 {
            unsafe { write_volatile(out, d) };
        }
        r
    }
    pub fn stat(path: *const u8, out: *mut StatDesc) -> i32 {
        if !record_ok(out) {
            return fs::ERR_INVAL;
        }
        let p = match unsafe { path_slice(path) } {
            Ok(p) => p,
            Err(c) => return c,
        };
        let mut d = StatDesc::EMPTY;
        let r = answer(with_table(|t| t.stat(p, &mut d)));
        if r == 0 {
            unsafe { write_volatile(out, d) };
        }
        r
    }
    pub fn unlink(path: *const u8) -> i32 {
        let p = match unsafe { path_slice(path) } {
            Ok(p) => p,
            Err(c) => return c,
        };
        answer(with_table(|t| t.unlink(p)))
    }
    pub fn mkdir(path: *const u8) -> i32 {
        let p = match unsafe { path_slice(path) } {
            Ok(p) => p,
            Err(c) => return c,
        };
        answer(with_table(|t| t.mkdir(p)))
    }
    pub fn rmdir(path: *const u8) -> i32 {
        let p = match unsafe { path_slice(path) } {
            Ok(p) => p,
            Err(c) => return c,
        };
        answer(with_table(|t| t.rmdir(p)))
    }
    pub fn rename(old: *const u8, new: *const u8) -> i32 {
        let (a, b) = match (unsafe { path_slice(old) }, unsafe { path_slice(new) }) {
            (Ok(a), Ok(b)) => (a, b),
            (Err(c), _) | (_, Err(c)) => return c,
        };
        answer(with_table(|t| t.rename(a, b)))
    }
    pub fn opendir(path: *const u8) -> i32 {
        let p = match unsafe { path_slice(path) } {
            Ok(p) => p,
            Err(c) => return c,
        };
        answer(with_table(|t| t.opendir(p)))
    }
    pub fn readdir(h: i32, out: *mut Dirent) -> i32 {
        if !record_ok(out) {
            return fs::ERR_INVAL;
        }
        let mut d = Dirent::EMPTY;
        let r = answer(with_table(|t| t.readdir(h, &mut d)));
        if r == 1 {
            unsafe { write_volatile(out, d) };
        }
        r
    }
    pub fn closedir(h: i32) -> i32 {
        answer(with_table(|t| t.closedir(h)))
    }
    pub fn statfs(path: *const u8, out: *mut StatfsDesc) -> i32 {
        if !record_ok(out) {
            return fs::ERR_INVAL;
        }
        let p = match unsafe { path_slice(path) } {
            Ok(p) => p,
            Err(c) => return c,
        };
        let mut d = StatfsDesc::EMPTY;
        let r = answer(with_table(|t| t.statfs(p, &mut d)));
        if r == 0 {
            unsafe { write_volatile(out, d) };
        }
        r
    }
}
#[cfg(all(rhu_sd_socket, not(feature = "test-sd-doctored")))]
pub use socket::*;

/// Every other row, and the STM32F769 row's doctored control image: no
/// mount, no handle, every path absent -- the functions with one shape.
#[cfg(not(all(rhu_sd_socket, not(feature = "test-sd-doctored"))))]
mod none {
    use crate::fs::{self, Dirent, MountDesc, StatDesc, StatfsDesc};
    /// A NULL path or record is invalid before anything else; every other
    /// path is absent and every handle outside the (empty) tables.
    fn path_code(path: *const u8) -> i32 {
        if path.is_null() { fs::ERR_INVAL } else { fs::ERR_NOENT }
    }
    fn record_code<T>(out: *mut T, code: i32) -> i32 {
        if out.is_null() { fs::ERR_INVAL } else { code }
    }
    pub fn mount_count() -> i32 {
        0
    }
    pub fn mount_desc(_index: u32, _out: *mut MountDesc) -> i32 {
        fs::ERR_INVAL
    }
    pub fn open(path: *const u8, _flags: u32) -> i32 {
        path_code(path)
    }
    pub fn close(_h: i32) -> i32 {
        fs::ERR_BADF
    }
    pub fn read(_h: i32, _buf: *mut u8, _n: u32) -> i32 {
        fs::ERR_BADF
    }
    pub fn write(_h: i32, _buf: *const u8, _n: u32) -> i32 {
        fs::ERR_BADF
    }
    pub fn pread(_h: i32, _buf: *mut u8, _n: u32, _offset: i64) -> i32 {
        fs::ERR_BADF
    }
    pub fn pwrite(_h: i32, _buf: *const u8, _n: u32, _offset: i64) -> i32 {
        fs::ERR_BADF
    }
    pub fn seek(_h: i32, _offset: i64, _whence: u32, pos: *mut i64) -> i32 {
        record_code(pos, fs::ERR_BADF)
    }
    pub fn truncate(_h: i32, _len: i64) -> i32 {
        fs::ERR_BADF
    }
    pub fn sync(_h: i32) -> i32 {
        fs::ERR_BADF
    }
    pub fn fstat(_h: i32, out: *mut StatDesc) -> i32 {
        record_code(out, fs::ERR_BADF)
    }
    pub fn stat(path: *const u8, out: *mut StatDesc) -> i32 {
        record_code(out, path_code(path))
    }
    pub fn unlink(path: *const u8) -> i32 {
        path_code(path)
    }
    pub fn mkdir(path: *const u8) -> i32 {
        path_code(path)
    }
    pub fn rmdir(path: *const u8) -> i32 {
        path_code(path)
    }
    pub fn rename(old: *const u8, new: *const u8) -> i32 {
        if old.is_null() || new.is_null() { fs::ERR_INVAL } else { fs::ERR_NOENT }
    }
    pub fn opendir(path: *const u8) -> i32 {
        path_code(path)
    }
    pub fn readdir(_h: i32, out: *mut Dirent) -> i32 {
        record_code(out, fs::ERR_BADF)
    }
    pub fn closedir(_h: i32) -> i32 {
        fs::ERR_BADF
    }
    pub fn statfs(path: *const u8, out: *mut StatfsDesc) -> i32 {
        record_code(out, path_code(path))
    }
}
#[cfg(not(all(rhu_sd_socket, not(feature = "test-sd-doctored"))))]
pub use none::*;
