//! The null-page guard's designed death and its deliveries. A thread
//! named reader reads a word through address 4 -- an offset the compiler
//! leaves unchecked on a null reference -- laundered so no fold or check
//! appears.

use core::hint::black_box;

use crate::{api, sched, semihost};

/// The address the reader touches: 4, a field at offset 4 on a null
/// reference, laundered through black_box so no fold or check appears.
#[cfg_attr(any(feature = "test-nullpage-fetch", feature = "test-nullpage-header"), allow(dead_code))]
fn null4() -> *const u32 {
    black_box(4usize) as *const u32
}

extern "C" fn reader(_: usize) {
    #[cfg(feature = "test-nullpage-cs")]
    {
        let v = crate::critical::with_cs(|| unsafe { core::ptr::read_volatile(null4()) });
        kprintln!("[nullpage] UNREACHABLE: the read through null inside the critical section returned {:#010x}", v);
        semihost::exit(3)
    }
    #[cfg(feature = "test-nullpage-fetch")]
    {
        // A jump to address 0 in Thumb state (bit 0 set): the fetch at 0
        // is the fault, an instruction access violation with the stacked
        // pc at 0.
        unsafe { core::arch::asm!("mov r0, #1", "bx r0", options(noreturn)) }
    }
    #[cfg(feature = "test-nullpage-deliver")]
    deliver::read_site();
    #[cfg(feature = "test-nullpage-header")]
    {
        // The header's first word through the linker symbol, laundered so
        // no fold or check appears: inside the page on this row, the read
        // is the designed death.
        let p = black_box(core::ptr::addr_of!(crate::header::__rhu_image_header) as *const u32);
        let v = unsafe { core::ptr::read_volatile(p) };
        kprintln!("[nullpage] UNREACHABLE: the read of the image header's first word at {:#010x} returned {:#010x}", p as usize, v);
        semihost::exit(3)
    }
    #[cfg(not(any(feature = "test-nullpage-cs", feature = "test-nullpage-fetch", feature = "test-nullpage-deliver", feature = "test-nullpage-header")))]
    {
        let v = unsafe { core::ptr::read_volatile(null4()) };
        kprintln!("[nullpage] UNREACHABLE: the read through null returned {:#010x}", v);
        semihost::exit(3)
    }
}

#[cfg(feature = "test-nullpage-fetch")]
extern "C" fn fetch_handler(_: *mut sched::RhuCtx) {
    panic!("nullpage: an instruction fetch below the page was delivered");
}

#[cfg(feature = "test-nullpage-deliver")]
mod deliver {
    use core::arch::asm;
    use core::sync::atomic::{AtomicU32, Ordering};

    use crate::{sched, semihost};

    pub const MARK_READ: u32 = 0xDE11_0001;
    pub const MARK_WRITE: u32 = 0xDE11_0002;
    /// The callee-saved words the thread holds at each site, read back
    /// from the context by the handler.
    const R4: u32 = 0x4444_1111;
    const R5: u32 = 0x5555_2222;
    const R8: u32 = 0x8888_4444;
    const R10: u32 = 0xAAAA_5555;

    static DELIVERED: AtomicU32 = AtomicU32::new(0);

    unsafe extern "C" {
        static rhu_nullpage_site_read: u8;
        static rhu_nullpage_site_write: u8;
    }

    /// The read through null with the callee-saved registers set to known
    /// words; the site is a global label, so the handler ties the
    /// context's pc to it (a named label is allowed here: the function is
    /// never inlined, so the label is defined once). Never returns: the
    /// handler redirects the thread to the landing (a read that returned
    /// would be the undefined instruction after it, a UsageFault death).
    #[inline(never)]
    #[allow(named_asm_labels)]
    pub fn read_site() -> ! {
        unsafe {
            asm!(
                ".global rhu_nullpage_site_read",
                "rhu_nullpage_site_read:",
                "ldr r0, [{p}]",
                "udf #0",
                p = in(reg) super::null4(),
                in("r4") R4,
                in("r5") R5,
                in("r8") R8,
                in("r10") R10,
                options(noreturn)
            )
        }
    }

    /// The write through null at offset 0x10, the same way.
    #[inline(never)]
    #[allow(named_asm_labels)]
    pub fn write_site() -> ! {
        let p = core::hint::black_box(0x10usize) as *mut u32;
        unsafe {
            asm!(
                ".global rhu_nullpage_site_write",
                "rhu_nullpage_site_write:",
                "str r1, [{p}]",
                "udf #0",
                p = in(reg) p,
                in("r1") MARK_WRITE,
                in("r4") R4,
                in("r5") R5,
                in("r8") R8,
                in("r10") R10,
                options(noreturn)
            )
        }
    }

    /// The test activation handler: entered on the reader's own stack
    /// with the fault context; asserts every field, then redirects.
    pub extern "C" fn handler(ctx: *mut sched::RhuCtx) {
        let c = unsafe { &mut *ctx };
        assert!(c.flags & sched::CTX_FLAG_FAULT != 0, "nullpage: the handler entered without the fault flag (flags {:#010x})", c.flags);
        let kind = (c.flags & sched::CTX_KIND_MASK) >> sched::CTX_KIND_SHIFT;
        let words = ctx as *const u32;
        let fault_addr = unsafe { words.add(18).read() };
        let fault_status = unsafe { words.add(19).read() };
        let n = DELIVERED.fetch_add(1, Ordering::SeqCst);
        let (site, want_addr, which, mark) = if n == 0 {
            (core::ptr::addr_of!(rhu_nullpage_site_read) as usize as u32, 4u32, "read", MARK_READ)
        } else {
            (core::ptr::addr_of!(rhu_nullpage_site_write) as usize as u32, 0x10u32, "write", MARK_WRITE)
        };
        assert!(kind == sched::FAULT_NULL, "nullpage: kind {} delivered, wanted {}", kind, sched::FAULT_NULL);
        assert!(fault_addr == want_addr, "nullpage: fault address {:#010x}, wanted {:#010x}", fault_addr, want_addr);
        assert!(c.pc & !1 == site & !1, "nullpage: the context's pc {:#010x} is not the {} site {:#010x}", c.pc, which, site);
        assert!(fault_status & 0x82 == 0x82, "nullpage: fault status {:#04x} without DACCVIOL and MMARVALID", fault_status);
        assert!(
            c.r[4] == R4 && c.r[5] == R5 && c.r[8] == R8 && c.r[10] == R10,
            "nullpage: the registers are not the thread's (r4 {:#010x} r5 {:#010x} r8 {:#010x} r10 {:#010x})",
            c.r[4], c.r[5], c.r[8], c.r[10]
        );
        kprintln!(
            "[nullpage] delivered: kind {} {} at {:#010x} from the {} site, the registers as the thread held them; pc redirected to the landing",
            kind, which, fault_addr, which
        );
        c.r[0] = mark;
        c.r[1] = fault_addr;
        c.pc = landing as *const () as usize as u32;
    }

    /// Where the redirected thread lands: r0 the marker, r1 the address
    /// the handler saw; the fault site's own stack pointer.
    pub extern "C" fn landing(mark: u32, addr: u32) -> ! {
        if mark == MARK_READ {
            kprintln!("[nullpage] landed from the read with marker {:#010x} (address {:#010x}); the write follows", mark, addr);
            write_site()
        }
        assert!(mark == MARK_WRITE, "nullpage: landed with an unknown marker {:#010x}", mark);
        kprintln!("[nullpage] landed from the write with marker {:#010x} (address {:#010x})", mark, addr);
        kprintln!("[nullpage] PASS (two null accesses delivered through the activation costume and landed: the read and the write)");
        semihost::exit(0)
    }
}

pub fn run() -> ! {
    #[cfg(feature = "test-nullpage-deliver")]
    {
        kprintln!("[nullpage] deliver: the test handler registered and the null kind armed through the door; the reader reads [4] then writes [0x10] -- expect two deliveries through the activation costume and two landings");
        assert!(api::rhu_activation_register(Some(deliver::handler)) == 0, "nullpage: the handler was refused");
        assert!(api::rhu_fault_register(2) == sched::ERR_INVAL, "nullpage: an unknown kind was accepted by the door");
        assert!(api::rhu_fault_register(sched::FAULT_NULL) == 0, "nullpage: the null kind was refused by the door");
        assert!(api::rhu_fault_register(sched::FAULT_NULL) == 0, "nullpage: arming twice was refused");
    }
    #[cfg(feature = "test-nullpage-fetch")]
    {
        kprintln!("[nullpage] fetch: the test handler registered and the null kind armed; the reader jumps to address 0 -- expect the terminal fetch line (an instruction fetch is never delivered)");
        assert!(api::rhu_activation_register(Some(fetch_handler)) == 0, "nullpage: the handler was refused");
        assert!(api::rhu_fault_register(sched::FAULT_NULL) == 0, "nullpage: the null kind was refused by the door");
    }
    #[cfg(feature = "test-nullpage-cs")]
    kprintln!("[nullpage] cs: the reader reads a word through address 4 inside the kernel's critical section -- expect the forced HardFault's naming");
    #[cfg(feature = "test-nullpage-header")]
    kprintln!("[nullpage] header: the reader reads the image header's first word through the linker symbol, at 0x400 on this row and inside the page, with nothing armed -- expect the null-access death");
    #[cfg(not(any(feature = "test-nullpage-cs", feature = "test-nullpage-fetch", feature = "test-nullpage-deliver", feature = "test-nullpage-header")))]
    kprintln!("[nullpage] a thread named reader reads a word through address 4 with nothing armed -- expect the null-access death");
    sched::spawn("reader", reader, 0, 2, 4096); // more urgent than this thread: runs at once
    api::rhu_sleep_ns(100_000_000);
    kprintln!("[nullpage] UNREACHABLE: the reader neither died nor landed within 100 ms");
    semihost::exit(3)
}
