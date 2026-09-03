//! The module registry: the table
//! behind the exidx/frozen/statics walks, and the anchor of N-module
//! TLS. Born deliberately small: 4 slots, descs copied at
//! registration, ranges immutable afterward, no unregister ever
//! (modules never unload; a dynamic app-slot story would carry its
//! own note when it arrives).

use core::ptr::{self, addr_of};

use crate::critical::with_cs;
use crate::sched;

/// Registry capacity. The Tcb's per-module TLS pointer array is
/// sized by this too.
pub const MAX_MODULES: usize = 4;

/// Mirror of `rhu_module_desc` (include/rhu.h) -- the two files change
/// together or not at all. The STRUCT is copied at registration; the
/// two pointers are kept as pointers and their referents must outlive
/// the module.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct ModuleDesc {
    pub text_lo: u32,
    pub text_hi: u32,
    pub statics_lo: [u32; 4],
    pub statics_hi: [u32; 4],
    pub frozen_lo: u32,
    pub frozen_hi: u32,
    pub exidx_lo: u32,
    pub exidx_hi: u32,
    pub tls_bytes: u32,
    pub tls_init: *const u8,
    pub name: *const u8,
}

const DESC_INIT: ModuleDesc = ModuleDesc {
    text_lo: 0,
    text_hi: 0,
    statics_lo: [0; 4],
    statics_hi: [0; 4],
    frozen_lo: 0,
    frozen_hi: 0,
    exidx_lo: 0,
    exidx_hi: 0,
    tls_bytes: 0,
    tls_init: ptr::null(),
    name: ptr::null(),
};

struct Registry {
    modules: [ModuleDesc; MAX_MODULES],
    count: usize,
}

static mut REGISTRY: Registry = Registry { modules: [DESC_INIT; MAX_MODULES], count: 0 };

#[allow(clippy::mut_from_ref)]
unsafe fn registry() -> &'static mut Registry { unsafe {
    &mut *ptr::addr_of_mut!(REGISTRY)
}}

/// Register a module: copy the desc, then eagerly give every live
/// thread its TLS block. Handle on success; ERR_AGAIN when the table
/// is full or a TLS block was refused (the walk unwinds wholesale and
/// nothing is committed).
pub fn register(d: &ModuleDesc) -> i32 {
    with_cs(|| unsafe {
        let r = registry();
        if r.count == MAX_MODULES {
            return sched::ERR_AGAIN;
        }
        let handle = r.count;
        if !sched::tls_on_register(handle, d.tls_bytes, d.tls_init) {
            return sched::ERR_AGAIN;
        }
        r.modules[handle] = *d;
        r.count = handle + 1;
        handle as i32
    })
}

/// (tls_bytes, tls_init) per registered module, for the spawn-side
/// eager walk. Runs under the caller's CS.
pub fn tls_demands() -> ([(u32, *const u8); MAX_MODULES], usize) {
    with_cs(|| unsafe {
        let r = registry();
        let mut out = [(0u32, ptr::null::<u8>()); MAX_MODULES];
        for i in 0..r.count {
            out[i] = (r.modules[i].tls_bytes, r.modules[i].tls_init);
        }
        (out, r.count)
    })
}

/// Registered-module count (for the tls_get handle validation).
pub fn count() -> usize {
    with_cs(|| unsafe { registry().count })
}

/// Containing-module lookup by text range: the FIRST
/// registered module whose half-open [text_lo, text_hi) contains
/// addr wins (registration order is the tiebreak; module 0 first).
/// Returns the handle with a copy of the stored desc, or None on a
/// miss. CS-bounded copy; never blocks.
pub fn find(addr: u32) -> Option<(i32, ModuleDesc)> {
    with_cs(|| unsafe {
        let r = registry();
        for i in 0..r.count {
            let m = &r.modules[i];
            if addr >= m.text_lo && addr < m.text_hi {
                return Some((i as i32, *m));
            }
        }
        None
    })
}

/// The GC-facing enumeration: a copy of the stored desc for
/// a handle, or None for any handle outside [0, count) -- the walk's
/// terminator. CS-bounded copy; never blocks.
pub fn roots(handle: i32) -> Option<ModuleDesc> {
    with_cs(|| unsafe {
        let r = registry();
        if handle < 0 || handle as usize >= r.count {
            return None;
        }
        Some(r.modules[handle as usize])
    })
}

#[cfg(feature = "payload")]
core::arch::global_asm!(
    r#"
    .section .tdata.rhu_anchor, "awT", %progbits
    .p2align 0
    .text
"#
);

/// Self-register the merged image as module 0 from the link symbols
/// the maps export. Called from `kmain` after `kheap::init()` and
/// before `sched::init()`: zero threads live, so the eager TLS walk is
/// empty by construction.
pub fn init_module0() {
    unsafe extern "C" {
        static __stext: u32;
        static __etext: u32;
        static __sdata: u32;
        static __edata: u32;
        static __sbss: u32;
        static __ebss: u32;
        static __rhu_frozen_lo: u32;
        static __rhu_frozen_hi: u32;
        static __exidx_start: u32;
        static __exidx_end: u32;
        static __rhu_tdata_lo: u32;
        static __rhu_tdata_hi: u32;
    }
    let tlo = core::hint::black_box(addr_of!(__rhu_tdata_lo) as u32);
    let thi = core::hint::black_box(addr_of!(__rhu_tdata_hi) as u32);
    let tls_bytes = thi - tlo;
    let d = ModuleDesc {
        text_lo: addr_of!(__stext) as u32,
        text_hi: addr_of!(__etext) as u32,
        statics_lo: [addr_of!(__sdata) as u32, addr_of!(__sbss) as u32, 0, 0],
        statics_hi: [addr_of!(__edata) as u32, addr_of!(__ebss) as u32, 0, 0],
        frozen_lo: addr_of!(__rhu_frozen_lo) as u32,
        frozen_hi: addr_of!(__rhu_frozen_hi) as u32,
        exidx_lo: addr_of!(__exidx_start) as u32,
        exidx_hi: addr_of!(__exidx_end) as u32,
        tls_bytes,
        tls_init: if tls_bytes == 0 { ptr::null() } else { tlo as *const u8 },
        name: c"kernel".as_ptr() as *const u8,
    };
    let h = register(&d);
    assert!(h == 0, "module 0 self-registration returned {}", h);
}

/// Stored-desc observability for the api smoke ONLY (the copy proof
/// and the module-0 self-registration check read this; never shipped
/// in the default build). Invalid handles die loud -- probe misuse is
/// a harness bug.
#[cfg(feature = "api-smoke")]
pub fn probe_module(handle: i32) -> ModuleDesc {
    with_cs(|| unsafe {
        let r = registry();
        assert!(
            handle >= 0 && (handle as usize) < r.count,
            "probe_module: invalid handle (harness bug)"
        );
        r.modules[handle as usize]
    })
}
