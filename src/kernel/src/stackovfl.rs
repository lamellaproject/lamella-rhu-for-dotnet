//! The stack-overflow designed death: a thread spawned with a small stack
//! descends past its floor, and the floor must be named -- on the rows
//! that carry the v8-M stack limit registers by the UsageFault the limit
//! raises, on the ARMv7-M rows by the MemManage fault the guard block at
//! the floor raises (the protection unit's 32-byte no-access block
//! the switch places at the running thread's floor). The funnel reports
//! either with its overflow line. A survived descent prints the
//! UNREACHABLE line and exits 3 -- the registered red shape, and what the
//! `stack-guard-off` control image shows on the v7-M rows.

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
    #[cfg(armv8m)]
    kprintln!("[stackovfl] a 4 KB victim parked below a 1 KB eater; the eater descends twelve 256-byte frames past its floor -- expect a UsageFault with the stack-overflow line");
    #[cfg(not(armv8m))]
    kprintln!("[stackovfl] a 4 KB victim parked below a 1 KB eater; the eater descends twelve 256-byte frames past its floor -- expect a MemManage fault with the stack-overflow line (the guard block at the floor)");
    sched::spawn("victim", victim, 0, 2, 4096); // runs at once, parks
    sched::spawn("eater", eater, 0, 2, 1024);
    api::rhu_sleep_ns(20_000_000);
    kprintln!("[stackovfl] UNREACHABLE: the descent survived the floor");
    semihost::exit(3)
}
