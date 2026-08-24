//! The RHU hosting API -- the C-ABI waistline between the kernel and
//! hosted payloads. Four functions, `rhu_`-prefixed, mirrored exactly
//! by `include/rhu.h`; the shim backend is the intended consumer and
//! nothing else links against it yet. One standing rule binds this
//! surface: no second blocking primitive ever joins it, and new needs
//! are shim recipes over these four.

use crate::sched;

#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.rhu_api")]
pub extern "C" fn rhu_now_ns() -> u64 {
    sched::now_ns()
}

#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.rhu_api")]
pub extern "C" fn rhu_sleep_ns(ns: u64) {
    sched::sleep_ns(ns)
}

#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.rhu_api")]
pub extern "C" fn rhu_futex_wait(addr: *const u32, expected: u32, deadline_ns: u64) -> i32 {
    sched::futex_wait(addr, expected, deadline_ns)
}

#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.rhu_api")]
pub extern "C" fn rhu_futex_wake(addr: *const u32, n: u32) -> i32 {
    sched::futex_wake(addr, n)
}
