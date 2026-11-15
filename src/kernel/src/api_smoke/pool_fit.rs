//! The stack pool's flight check, the flight flavor of the api image
//! (`test-pool-fit`): what a program can take of the row's stack pool. The
//! image runs this check alone, from the main thread's first line.
//!
//! (1) THE CENSUS AT THE FIRST LINE, read from the thread table and the
//! pool's report: the pool's size is the row's; no stack is retired; the
//! bytes never carved are the pool less the idle thread's stack and this
//! thread's own. The kernel's other threads alive at that moment -- the
//! network thread on a kernel with a medium, the card watcher on a row with
//! a card socket -- are listed with their stacks and take none of it.
//! (2) SIX STACKS asked through the spawn function and kept alive, as an
//! awaited socket program's runtime asks them: the finalizer's, the socket
//! engine's and the timer's threads at 16384 bytes, the thread pool's gate
//! thread at 24576, two pool workers at 16384 -- all given. (3) THE REFUSAL
//! PAST THE POOL: a third worker answers RHU_EAGAIN with the kernel's own
//! line where the pool has less than its stack left, and the report does
//! not move.
//!
//! The PASS line carries no word about the kernel's own threads: the image
//! with the network and the image without it must print the same line.
//!
//! The control (`test-pool-doctored`): the pool's array carries no bytes
//! for the kernel's own threads, so they take a program's, and the census
//! refuses with the bytes they took.

use core::ffi::c_void;
use core::sync::atomic::AtomicU32;

use crate::{api, board, sched};

/// The word the sequence's threads park on for the image's life.
static PARK: AtomicU32 = AtomicU32::new(0);

extern "C" fn parked(_: *mut c_void) {
    loop {
        let _ = api::rhu_futex_wait(PARK.as_ptr(), 0, sched::DEADLINE_NONE);
    }
}

/// The stacks an awaited socket program's runtime asks, in its order.
const SEQUENCE: [u32; 6] = [16384, 16384, 16384, 24576, 16384, 16384];
/// A pool worker's stack.
const WORKER: u32 = 16384;

fn ask(stack: u32) -> i32 {
    api::rhu_thread_spawn(Some(parked), core::ptr::null_mut(), stack, api::HOSTED_PRIO_FLAT, c"pool".as_ptr())
}

pub fn run() {
    kprintln!("[api] pool-fit: what a program can take of the row's stack pool -- the census at the main thread's first line, six stacks as an awaited socket program's runtime asks them, the refusal past the pool");
    let (pool, free, retired) = sched::pool_remainder();
    let mut rows = [([0u8; sched::NAME_BYTES], 0u32, 0u8); sched::MAX_THREADS];
    let n = sched::probe_stacks(&mut rows);
    let (mut idle, mut own, mut others, mut other_bytes) = (0usize, 0usize, 0u32, 0usize);
    for (name, bytes, who) in rows.iter().take(n) {
        match who {
            1 => idle = *bytes as usize,
            2 => own = *bytes as usize,
            _ => {
                let len = name.iter().position(|b| *b == 0).unwrap_or(name.len());
                kprintln!("[api] pool-fit: the kernel's own thread {} holds a stack of {} bytes", core::str::from_utf8(&name[..len]).unwrap_or("?"), bytes);
                others += 1;
                other_bytes += *bytes as usize;
            }
        }
    }
    assert!(
        pool == board::STACK_POOL_BYTES && retired == 0 && idle != 0 && own != 0 && pool > idle + own,
        "pool-fit: the report reads a pool of {} bytes (the row's is {}) with {} retired stacks; the idle thread's stack {} and this thread's {}",
        pool,
        board::STACK_POOL_BYTES,
        retired,
        idle,
        own
    );
    let want = pool - idle - own;
    assert!(
        free == want,
        "pool-fit: {} bytes never carved at the main thread's first line where the pool of {} less the idle thread's {} and this thread's {} is {}: the kernel's own threads took {} of a program's bytes",
        free,
        pool,
        idle,
        own,
        want,
        want.wrapping_sub(free)
    );
    kprintln!("[api] pool-fit: at the first line {} never carved of {}; the idle thread's stack {} and this thread's {}; {} other threads of the kernel's own with {} bytes of stack, none of it the pool's", free, pool, idle, own, others, other_bytes);
    let mut left = free;
    for (k, stack) in SEQUENCE.iter().enumerate() {
        let tid = ask(*stack);
        assert!(tid > 0, "pool-fit: stack {} of the sequence ({} bytes) answered {} with {} bytes never carved", k + 1, stack, tid, left);
        left -= *stack as usize;
    }
    let (_, after, retired) = sched::pool_remainder();
    assert!(after == left && retired == 0, "pool-fit: after the six stacks the report reads {} never carved and {} retired where {} and 0 were expected", after, retired, left);
    let r = ask(WORKER);
    let (_, last, retired) = sched::pool_remainder();
    if after < WORKER as usize {
        assert!(r == sched::ERR_AGAIN && last == after && retired == 0, "pool-fit: a third worker of {} bytes answered {} and the report reads {} never carved (EAGAIN and {} unmoved were expected)", WORKER, r, last, after);
        kprintln!(
            "[api] PASS pool-fit (the pool of {} bytes: at the main thread's first line {} never carved, the pool less the idle thread's {} and this thread's {}, and no stack retired; six stacks of 16384, 16384, 16384, 24576, 16384 and 16384 bytes all given, {} never carved after them; a third worker of {} refused EAGAIN and the report unmoved)",
            pool, free, idle, own, after, WORKER
        );
    } else {
        assert!(r > 0 && last == after - WORKER as usize && retired == 0, "pool-fit: a third worker of {} bytes answered {} and the report reads {} never carved with {} left before it", WORKER, r, last, after);
        kprintln!(
            "[api] PASS pool-fit (the pool of {} bytes: at the main thread's first line {} never carved, the pool less the idle thread's {} and this thread's {}, and no stack retired; six stacks of 16384, 16384, 16384, 24576, 16384 and 16384 bytes all given, {} never carved after them; a third worker of {} given, the row's pool being the larger)",
            pool, free, idle, own, after, WORKER
        );
    }
}
