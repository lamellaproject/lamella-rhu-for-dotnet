//! The kernel's file API's table: the one mount of a board's card socket at
//! its mount point, the open files and the open directories a program holds
//! through the kernel functions rhu_fs_open and rhu_fs_opendir, the paths
//! routed against the mount point and handed to the FAT layer, every answer
//! a count, a handle or a code of the public header at Linux's values. One
//! pure module over the layer's volume type (no register, no chip, nothing
//! under the critical section): the kernel's fsmount.rs owns a table over
//! the socket's device under the family's tenancy, the host tier compiles
//! this file over its sparse device and pins the tables, the routing and the
//! codes. The records, the constants and the codes at the top of this file
//! compile on every row (api.rs's signatures name them everywhere); the
//! table compiles where a socket is.
#![allow(dead_code)] // the host tier pins every function; the kernel's table calls the subset its rows reach

/// The open-file table's size and the open-directory table's.
pub const FILES: usize = 16;
pub const DIRS: usize = 4;
/// The mount point of a board's first card socket.
pub const POINT: &[u8] = b"/media/sd0";
/// The most bytes of a path read before the name-too-long code.
pub const PATH_MAX: usize = 4096;
/// The open flags (the header's RHU_FS_O_ names; the FAT layer's own values).
pub const O_READ: u32 = 1;
pub const O_WRITE: u32 = 2;
pub const O_CREATE: u32 = 4;
pub const O_EXCL: u32 = 8;
pub const O_TRUNC: u32 = 16;
pub const O_APPEND: u32 = 32;
pub const FLAGS_MASK: u32 = O_READ | O_WRITE | O_CREATE | O_EXCL | O_TRUNC | O_APPEND;
/// The seek words (the header's RHU_FS_SEEK_ names; the layer's own values).
pub const SEEK_SET: u32 = 0;
pub const SEEK_CUR: u32 = 1;
pub const SEEK_END: u32 = 2;
/// The mount's states (the header's RHU_FS_ names).
pub const STATE_MOUNTED: u32 = 1;
pub const STATE_NO_CARD: u32 = 2;
pub const STATE_UNREADABLE: u32 = 3;
/// An entry's kinds (the header's RHU_FS_ names).
pub const KIND_FILE: u32 = 1;
pub const KIND_DIR: u32 = 2;
/// The codes, Linux's values negated (the header's RHU_E names).
pub const ERR_NOENT: i32 = -2;
pub const ERR_IO: i32 = -5;
pub const ERR_BADF: i32 = -9;
pub const ERR_EXIST: i32 = -17;
pub const ERR_NOTDIR: i32 = -20;
pub const ERR_ISDIR: i32 = -21;
pub const ERR_INVAL: i32 = -22;
pub const ERR_MFILE: i32 = -24;
pub const ERR_NOSPC: i32 = -28;
pub const ERR_ROFS: i32 = -30;
pub const ERR_NAMETOOLONG: i32 = -36;
pub const ERR_NOTEMPTY: i32 = -39;
pub const ERR_TIMEDOUT: i32 = -110;

/// The mount record (the header's rhu_fs_mount_desc, 76 bytes).
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MountDesc {
    pub point: [u8; 16],
    pub device: u32,
    pub state: u32,
    pub fat_width: u32,
    pub cluster_bytes: u32,
    pub clusters: u32,
    pub free_clusters: u32,
    pub label: [u8; 12],
    pub why: [u8; 24],
}
impl MountDesc {
    pub const EMPTY: MountDesc = MountDesc { point: [0; 16], device: 0, state: 0, fat_width: 0, cluster_bytes: 0, clusters: 0, free_clusters: 0, label: [0; 12], why: [0; 24] };
    /// A NUL-terminated field's text.
    pub fn text(field: &[u8]) -> &[u8] {
        let n = field.iter().position(|&b| b == 0).unwrap_or(field.len());
        &field[..n]
    }
}
/// A file's or a directory's status (the header's rhu_fs_stat_desc, 40 bytes).
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StatDesc {
    pub size: u32,
    pub kind: u32,
    pub read_only: u32,
    pub first_cluster: u32,
    pub mtime: i64,
    pub ctime: i64,
    pub atime: i64,
}
impl StatDesc {
    pub const EMPTY: StatDesc = StatDesc { size: 0, kind: 0, read_only: 0, first_cluster: 0, mtime: 0, ctime: 0, atime: 0 };
}
/// A directory listing's entry (the header's rhu_fs_dirent, 264 bytes).
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Dirent {
    pub name: [u8; 256],
    pub kind: u32,
    pub size: u32,
}
impl Dirent {
    pub const EMPTY: Dirent = Dirent { name: [0; 256], kind: 0, size: 0 };
    /// The name without its terminator.
    pub fn name(&self) -> &[u8] {
        MountDesc::text(&self.name)
    }
}
/// A volume's counts (the header's rhu_fs_statfs_desc, 16 bytes).
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StatfsDesc {
    pub block_size: u32,
    pub blocks: u32,
    pub free: u32,
    pub name_max: u32,
}
impl StatfsDesc {
    pub const EMPTY: StatfsDesc = StatfsDesc { block_size: 0, blocks: 0, free: 0, name_max: 0 };
}
const _: () = assert!(core::mem::size_of::<MountDesc>() == 76 && core::mem::size_of::<StatDesc>() == 40 && core::mem::size_of::<Dirent>() == 264 && core::mem::size_of::<StatfsDesc>() == 16, "a record's size is not the header's");

#[cfg(not(rhu_fs_none))]
mod table {
    use super::*;
    use crate::calendar_math::Civil;
    use crate::fat::{self, BlockDevice, Dir, Entry, Error, Handle, Kind, Refused, Stat, Volume};

    const _: () = assert!(O_READ == fat::O_READ && O_WRITE == fat::O_WRITE && O_CREATE == fat::O_CREATE && O_EXCL == fat::O_EXCL && O_TRUNC == fat::O_TRUNC && O_APPEND == fat::O_APPEND && SEEK_SET == fat::SEEK_SET && SEEK_CUR == fat::SEEK_CUR && SEEK_END == fat::SEEK_END, "the flags are not the layer's");

    /// An open file's slot: the layer's handle, the generation it was opened under and its open flags.
    #[derive(Clone, Copy)]
    struct FileSlot {
        open: bool,
        generation: u32,
        flags: u32,
        handle: Option<Handle>,
    }
    impl FileSlot {
        const EMPTY: FileSlot = FileSlot { open: false, generation: 0, flags: 0, handle: None };
    }
    /// An open directory's slot: the layer's walk position and its generation.
    #[derive(Clone, Copy)]
    struct DirSlot {
        open: bool,
        generation: u32,
        dir: Option<Dir>,
    }
    impl DirSlot {
        const EMPTY: DirSlot = DirSlot { open: false, generation: 0, dir: None };
    }

    /// The table: one socket's volume (or none), its state and the words of a
    /// refused mount, the generation, the two handle tables.
    pub struct Table<D: BlockDevice> {
        volume: Option<Volume<D>>,
        generation: u32,
        state: u32,
        why: &'static str,
        files: [FileSlot; FILES],
        dirs: [DirSlot; DIRS],
    }

    impl<D: BlockDevice> Table<D> {
        pub const fn new() -> Table<D> {
            Table { volume: None, generation: 0, state: STATE_NO_CARD, why: "", files: [FileSlot::EMPTY; FILES], dirs: [DirSlot::EMPTY; DIRS] }
        }
        pub fn state(&self) -> u32 {
            self.state
        }
        pub fn generation(&self) -> u32 {
            self.generation
        }
        pub fn mounted(&self) -> bool {
            self.volume.is_some()
        }
        /// The refused mount's words while the state is unreadable; empty otherwise.
        pub fn why(&self) -> &'static str {
            self.why
        }
        /// The FAT width as the number 12, 16 or 32 while mounted; 0 otherwise.
        pub fn fat_width(&self) -> u32 {
            match self.volume.as_ref().map(|v| v.geometry().kind) {
                Some(Kind::Fat12) => 12,
                Some(Kind::Fat16) => 16,
                Some(Kind::Fat32) => 32,
                None => 0,
            }
        }
        /// The mounted volume for a caller that holds the family's tenancy (the api checks).
        pub fn volume(&mut self) -> Option<&mut Volume<D>> {
            self.volume.as_mut()
        }
        /// The mount: any volume standing is dropped first, the layer's mount runs
        /// into the slot through the caller's scratch, the clock seam is set and
        /// the generation moves; a refusal is remembered with its words as the
        /// unreadable state, and a device that answered no device as the no-card
        /// state (the card left between the detect line's reading and the mount).
        pub fn mount(&mut self, dev: D, scratch: &mut [u8], now: fn() -> Option<Civil>) -> Result<(), Refused> {
            self.drop_volume();
            match Volume::mount_in(&mut self.volume, dev, scratch) {
                Ok(()) => {
                    self.volume.as_mut().unwrap().set_clock(now);
                    self.generation = self.generation.wrapping_add(1);
                    self.state = STATE_MOUNTED;
                    self.why = "";
                    Ok(())
                }
                Err(r) => {
                    if matches!(r, Refused::Device(Error::NoDevice)) {
                        self.state = STATE_NO_CARD;
                        self.why = "";
                    } else {
                        self.state = STATE_UNREADABLE;
                        self.why = r.words();
                    }
                    Err(r)
                }
            }
        }
        /// The unmount: the volume dropped without a flush (its device is gone),
        /// the generation moved so every handle of the mount is dead, the state
        /// no card.
        pub fn unmount(&mut self) {
            self.drop_volume();
            self.state = STATE_NO_CARD;
            self.why = "";
        }
        fn drop_volume(&mut self) {
            if self.volume.is_some() {
                self.volume = None;
                self.generation = self.generation.wrapping_add(1);
            }
        }
        /// The mount table's rows: one, this socket's.
        pub fn mount_count(&self) -> i32 {
            1
        }
        /// The row's record: the point, the state, the volume's figures while mounted, the refusal's words while unreadable.
        pub fn mount_desc(&self, index: u32, out: &mut MountDesc) -> i32 {
            if index != 0 {
                return ERR_INVAL;
            }
            let mut d = MountDesc::EMPTY;
            d.point[..POINT.len()].copy_from_slice(POINT);
            d.state = self.state;
            if let Some(v) = self.volume.as_ref() {
                let g = v.geometry();
                d.fat_width = self.fat_width();
                d.cluster_bytes = g.cluster_bytes;
                d.clusters = g.count_of_clusters;
                d.free_clusters = v.free_clusters();
                let label = v.label();
                let n = label.len().min(d.label.len() - 1);
                d.label[..n].copy_from_slice(&label[..n]);
            }
            let why = self.why.as_bytes();
            let n = why.len().min(d.why.len() - 1);
            d.why[..n].copy_from_slice(&why[..n]);
            *out = d;
            0
        }
        /// A layer error as the family's code: a device that left is the input-output code.
        fn code(e: Error) -> i32 {
            match e {
                Error::NoDevice => ERR_IO,
                other => other.code(),
            }
        }
        /// The path routed: absolute, within the window, the mount point itself
        /// the volume's root, a path under the point with the point dropped; any
        /// other path, and every path while no volume stands, does not exist.
        fn route<'a>(&self, path: &'a [u8]) -> Result<&'a [u8], i32> {
            if path.first() != Some(&b'/') {
                return Err(ERR_INVAL);
            }
            if path.len() > PATH_MAX {
                return Err(ERR_NAMETOOLONG);
            }
            let rest: &[u8] = if path == POINT {
                b"/"
            } else if path.len() > POINT.len() && path.starts_with(POINT) && path[POINT.len()] == b'/' {
                &path[POINT.len()..]
            } else {
                return Err(ERR_NOENT);
            };
            if self.volume.is_none() {
                return Err(ERR_NOENT);
            }
            Ok(rest)
        }
        /// A live file handle: in the table, open, of the standing mount's generation; with its open flags.
        fn file(&mut self, h: i32) -> Result<(&mut Volume<D>, &mut Handle, u32), i32> {
            if h < 0 || h as usize >= FILES {
                return Err(ERR_BADF);
            }
            let Table { volume, generation, files, .. } = self;
            let slot = &mut files[h as usize];
            if !slot.open {
                return Err(ERR_BADF);
            }
            match volume.as_mut() {
                Some(v) if slot.generation == *generation => Ok((v, slot.handle.as_mut().unwrap(), slot.flags)),
                _ => Err(ERR_IO),
            }
        }
        /// A live directory handle, the same way in its own table.
        fn dir(&mut self, h: i32) -> Result<(&mut Volume<D>, &mut Dir), i32> {
            if h < 0 || h as usize >= DIRS {
                return Err(ERR_BADF);
            }
            let Table { volume, generation, dirs, .. } = self;
            let slot = &mut dirs[h as usize];
            if !slot.open {
                return Err(ERR_BADF);
            }
            match volume.as_mut() {
                Some(v) if slot.generation == *generation => Ok((v, slot.dir.as_mut().unwrap())),
                _ => Err(ERR_IO),
            }
        }
        /// The open: the flags' six bits with an access bit (the truncate and
        /// append bits need the write bit), a free slot, the layer's open; the
        /// handle, or the code.
        pub fn open(&mut self, path: &[u8], flags: u32) -> i32 {
            if flags & !FLAGS_MASK != 0 || flags & (O_READ | O_WRITE) == 0 || (flags & (O_TRUNC | O_APPEND) != 0 && flags & O_WRITE == 0) {
                return ERR_INVAL;
            }
            let rest = match self.route(path) {
                Ok(r) => r,
                Err(c) => return c,
            };
            let Some(i) = self.files.iter().position(|s| !s.open) else {
                return ERR_MFILE;
            };
            let g = self.generation;
            match self.volume.as_mut().unwrap().open(rest, flags) {
                Ok(h) => {
                    self.files[i] = FileSlot { open: true, generation: g, flags, handle: Some(h) };
                    i as i32
                }
                Err(e) => Self::code(e),
            }
        }
        /// The close: the layer's close (a sync when owed) for a live handle and
        /// its code; the slot freed either way, a dead handle's close answering 0.
        pub fn close(&mut self, h: i32) -> i32 {
            if h < 0 || h as usize >= FILES {
                return ERR_BADF;
            }
            let i = h as usize;
            if !self.files[i].open {
                return ERR_BADF;
            }
            let live = self.volume.is_some() && self.files[i].generation == self.generation;
            let mut r = 0;
            if live {
                let mut hd = self.files[i].handle.take().unwrap();
                if let Err(e) = self.volume.as_mut().unwrap().close(&mut hd) {
                    r = Self::code(e);
                }
            }
            self.files[i] = FileSlot::EMPTY;
            r
        }
        /// The read: the bytes read, 0 at the end, or the code.
        pub fn read(&mut self, h: i32, buf: &mut [u8]) -> i32 {
            let (v, hd, _) = match self.file(h) {
                Ok(x) => x,
                Err(c) => return c,
            };
            let n = buf.len().min(i32::MAX as usize);
            match v.read(hd, &mut buf[..n]) {
                Ok(n) => n as i32,
                Err(e) => Self::code(e),
            }
        }
        /// The write: the bytes written (the layer's write(2) rule: the bytes
        /// moved before a refusal, the refusal at the next call), or the code.
        pub fn write(&mut self, h: i32, data: &[u8]) -> i32 {
            let (v, hd, _) = match self.file(h) {
                Ok(x) => x,
                Err(c) => return c,
            };
            let n = data.len().min(i32::MAX as usize);
            match v.write(hd, &data[..n]) {
                Ok(n) => n as i32,
                Err(e) => Self::code(e),
            }
        }
        /// The seek: the new position written to pos; a whence outside the three invalid.
        pub fn seek(&mut self, h: i32, offset: i64, whence: u32, pos: &mut i64) -> i32 {
            if whence > SEEK_END {
                return ERR_INVAL;
            }
            let (v, hd, _) = match self.file(h) {
                Ok(x) => x,
                Err(c) => return c,
            };
            match v.seek(hd, offset, whence) {
                Ok(p) => {
                    *pos = p as i64;
                    0
                }
                Err(e) => Self::code(e),
            }
        }
        /// The truncate: a length below 0 or past the layer's 32-bit size invalid.
        pub fn truncate(&mut self, h: i32, len: i64) -> i32 {
            if !(0..=u32::MAX as i64).contains(&len) {
                return ERR_INVAL;
            }
            let (v, hd, _) = match self.file(h) {
                Ok(x) => x,
                Err(c) => return c,
            };
            match v.truncate(hd, len as u32) {
                Ok(()) => 0,
                Err(e) => Self::code(e),
            }
        }
        /// The sync: the file's entry when owed, the FAT copies, the FSInfo, the device.
        pub fn sync(&mut self, h: i32) -> i32 {
            let (v, hd, _) = match self.file(h) {
                Ok(x) => x,
                Err(c) => return c,
            };
            match v.sync_file(hd) {
                Ok(()) => 0,
                Err(e) => Self::code(e),
            }
        }
        /// The positional read: the handle's position saved, the read from the
        /// offset, the position restored even after a refusal (pread(2)'s rule);
        /// an offset below 0 or past the layer's 32-bit size invalid.
        pub fn pread(&mut self, h: i32, buf: &mut [u8], offset: i64) -> i32 {
            if !(0..=u32::MAX as i64).contains(&offset) {
                return ERR_INVAL;
            }
            let (v, hd, _) = match self.file(h) {
                Ok(x) => x,
                Err(c) => return c,
            };
            let saved = hd.position() as i64;
            if let Err(e) = v.seek(hd, offset, SEEK_SET) {
                return Self::code(e);
            }
            let n = buf.len().min(i32::MAX as usize);
            let r = match v.read(hd, &mut buf[..n]) {
                Ok(n) => n as i32,
                Err(e) => Self::code(e),
            };
            let _ = v.seek(hd, saved, SEEK_SET);
            r
        }
        /// The positional write, the same way (pwrite(2)'s rule: the position
        /// unmoved, a gap past the end allocated and zeroed as the write does);
        /// a handle opened for append is refused as invalid, since its every
        /// write goes to the end.
        pub fn pwrite(&mut self, h: i32, data: &[u8], offset: i64) -> i32 {
            if !(0..=u32::MAX as i64).contains(&offset) {
                return ERR_INVAL;
            }
            let (v, hd, flags) = match self.file(h) {
                Ok(x) => x,
                Err(c) => return c,
            };
            if flags & O_APPEND != 0 {
                return ERR_INVAL;
            }
            let saved = hd.position() as i64;
            if let Err(e) = v.seek(hd, offset, SEEK_SET) {
                return Self::code(e);
            }
            let n = data.len().min(i32::MAX as usize);
            let r = match v.write(hd, &data[..n]) {
                Ok(n) => n as i32,
                Err(e) => Self::code(e),
            };
            let _ = v.seek(hd, saved, SEEK_SET);
            r
        }
        fn fill_stat(s: &Stat, out: &mut StatDesc) {
            *out = StatDesc {
                size: if s.is_dir { 0 } else { s.size },
                kind: if s.is_dir { KIND_DIR } else { KIND_FILE },
                read_only: s.read_only as u32,
                first_cluster: s.first,
                mtime: s.mtime,
                ctime: s.ctime,
                atime: s.atime,
            };
        }
        pub fn fstat(&mut self, h: i32, out: &mut StatDesc) -> i32 {
            let (v, hd, _) = match self.file(h) {
                Ok(x) => x,
                Err(c) => return c,
            };
            match v.fstat(hd) {
                Ok(s) => {
                    Self::fill_stat(&s, out);
                    0
                }
                Err(e) => Self::code(e),
            }
        }
        pub fn stat(&mut self, path: &[u8], out: &mut StatDesc) -> i32 {
            let rest = match self.route(path) {
                Ok(r) => r,
                Err(c) => return c,
            };
            match self.volume.as_mut().unwrap().stat(rest) {
                Ok(s) => {
                    Self::fill_stat(&s, out);
                    0
                }
                Err(e) => Self::code(e),
            }
        }
        pub fn unlink(&mut self, path: &[u8]) -> i32 {
            let rest = match self.route(path) {
                Ok(r) => r,
                Err(c) => return c,
            };
            match self.volume.as_mut().unwrap().unlink(rest) {
                Ok(()) => 0,
                Err(e) => Self::code(e),
            }
        }
        pub fn mkdir(&mut self, path: &[u8]) -> i32 {
            let rest = match self.route(path) {
                Ok(r) => r,
                Err(c) => return c,
            };
            match self.volume.as_mut().unwrap().mkdir(rest) {
                Ok(()) => 0,
                Err(e) => Self::code(e),
            }
        }
        pub fn rmdir(&mut self, path: &[u8]) -> i32 {
            let rest = match self.route(path) {
                Ok(r) => r,
                Err(c) => return c,
            };
            match self.volume.as_mut().unwrap().rmdir(rest) {
                Ok(()) => 0,
                Err(e) => Self::code(e),
            }
        }
        /// The rename: both paths under the mount (the layer's rename within the volume).
        pub fn rename(&mut self, old: &[u8], new: &[u8]) -> i32 {
            let (from, to) = match (self.route(old), self.route(new)) {
                (Ok(a), Ok(b)) => (a, b),
                (Err(c), _) | (_, Err(c)) => return c,
            };
            match self.volume.as_mut().unwrap().rename(from, to) {
                Ok(()) => 0,
                Err(e) => Self::code(e),
            }
        }
        /// The directory opened: a free slot of the directory table, the layer's walk; the handle, or the code.
        pub fn opendir(&mut self, path: &[u8]) -> i32 {
            let rest = match self.route(path) {
                Ok(r) => r,
                Err(c) => return c,
            };
            let Some(i) = self.dirs.iter().position(|s| !s.open) else {
                return ERR_MFILE;
            };
            let g = self.generation;
            match self.volume.as_mut().unwrap().opendir(rest) {
                Ok(d) => {
                    self.dirs[i] = DirSlot { open: true, generation: g, dir: Some(d) };
                    i as i32
                }
                Err(e) => Self::code(e),
            }
        }
        /// One entry per call: 1 with the entry written, 0 at the end, or the code.
        pub fn readdir(&mut self, h: i32, out: &mut Dirent) -> i32 {
            let (v, d) = match self.dir(h) {
                Ok(x) => x,
                Err(c) => return c,
            };
            let mut e = Entry::EMPTY;
            match v.readdir(d, &mut e) {
                Ok(true) => {
                    let name = e.name();
                    let n = name.len().min(out.name.len() - 1);
                    out.name = [0; 256];
                    out.name[..n].copy_from_slice(&name[..n]);
                    out.kind = if e.is_dir() { KIND_DIR } else { KIND_FILE };
                    out.size = if e.is_dir() { 0 } else { e.size };
                    1
                }
                Ok(false) => 0,
                Err(e) => Self::code(e),
            }
        }
        /// The directory closed: the slot freed either way, a dead handle's close answering 0.
        pub fn closedir(&mut self, h: i32) -> i32 {
            if h < 0 || h as usize >= DIRS {
                return ERR_BADF;
            }
            let i = h as usize;
            if !self.dirs[i].open {
                return ERR_BADF;
            }
            let live = self.volume.is_some() && self.dirs[i].generation == self.generation;
            let mut r = 0;
            if live {
                let mut d = self.dirs[i].dir.take().unwrap();
                if let Err(e) = self.volume.as_mut().unwrap().closedir(&mut d) {
                    r = Self::code(e);
                }
            }
            self.dirs[i] = DirSlot::EMPTY;
            r
        }
        /// The volume's counts for a path that exists under the mount.
        pub fn statfs(&mut self, path: &[u8], out: &mut StatfsDesc) -> i32 {
            let rest = match self.route(path) {
                Ok(r) => r,
                Err(c) => return c,
            };
            let v = self.volume.as_mut().unwrap();
            if let Err(e) = v.stat(rest) {
                return Self::code(e);
            }
            let s = v.statfs();
            *out = StatfsDesc { block_size: s.block_size, blocks: s.blocks, free: s.free, name_max: s.name_max };
            0
        }
    }
}
#[cfg(not(rhu_fs_none))]
pub use table::Table;
