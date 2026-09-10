//! The stack-overflow designed death, on the rows that carry the v8-M
//! stack limit registers: a thread spawned with a small stack descends
//! past its floor, and the limit register must raise the UsageFault the
//! funnel reports with the overflow line. A survived descent prints the
//! UNREACHABLE line and exits 3 -- the registered red shape.

#[cfg(not(armv8m))]
compile_error!("the stack-overflow flavor needs the v8-M stack limit registers: build it for a thumbv8m target");

use core::hint::black_box;
use core::sync::atomic::AtomicU32;

use crate::{api, sched, semihost};

static PARK: AtomicU32 = AtomicU32::new(0);

extern "C" fn victim(_: usize) {
    loop {
        let _ = api::rhu_futex_wait(PARK.as_ptr(), 0, sched::DEADLINE_NONE);
    }
}

/// One 256-byte frame per level, kept live past the compiler's reach.
#[inline(never)]
fn descend(n: u32) -> u32 {
    let mut frame = [0u32; 64];
    frame[(n & 63) as usize] = n;
    black_box(&mut frame);
    if n == 0 {
        frame[0]
    } else {
        descend(n - 1).wrapping_add(black_box(frame[(n & 63) as usize]))
    }
}

extern "C" fn eater(_: usize) {
    let r = descend(black_box(12));
    black_box(r);
}

pub fn run() -> ! {
    kprintln!("[stkof] a 4 KB victim parked below a 1 KB eater; the eater descends twelve 256-byte frames past its floor -- expect a UsageFault with the stack-overflow line");
    sched::spawn("victim", victim, 0, 2, 4096);
    sched::spawn("eater", eater, 0, 2, 1024);
    api::rhu_sleep_ns(20_000_000);
    kprintln!("[stkof] UNREACHABLE: the descent survived the floor");
    semihost::exit(3)
}
