//! The in-image header: the image's own statement of what it is and
//! where it loads, at a fixed offset from the image base on every board
//! (image base + 0x400 -- the board maps place the section there and
//! assert the offset), so a reader holding the flash bytes alone -- a
//! flasher, a manifest generator, a debug adapter -- finds it without
//! the ELF. A consumer reads a fact the image states; nothing types it.

use core::ptr::addr_of;

use crate::{api, sched};

/// The record, `repr(C)`: the word layout above is the struct layout.
#[repr(C)]
pub struct ImageHeader {
    pub magic: u32,
    pub layout_version: u32,
    pub header_bytes: u32,
    pub load_addr: *const u32,
    pub abi: u32,
    pub debug_layout_version: u32,
    pub reserved: [u32; 10],
}

unsafe impl Sync for ImageHeader {}

pub const IMAGE_HEADER_MAGIC: u32 = 0x4955_4852;
pub const IMAGE_HEADER_LAYOUT: u32 = 1;

const _: () = assert!(
    core::mem::size_of::<ImageHeader>() == 64,
    "image header layout version 1 is sixteen words"
);

unsafe extern "C" {
    static __vector_table: u32;
}

#[used]
#[unsafe(no_mangle)]
#[unsafe(link_section = ".rhu_header")]
pub static __rhu_image_header: ImageHeader = ImageHeader {
    magic: IMAGE_HEADER_MAGIC,
    layout_version: IMAGE_HEADER_LAYOUT,
    header_bytes: core::mem::size_of::<ImageHeader>() as u32,
    load_addr: addr_of!(__vector_table),
    abi: api::ABI,
    debug_layout_version: sched::DEBUG_LAYOUT_VERSION,
    reserved: [0; 10],
};
