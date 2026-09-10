//! The wake-target arithmetic: pure functions over tick counts, shared by
//! the scheduler's sleep and timed-wait paths and compiled unchanged by
//! the host tier from this file. The timer walk compares 32-bit ticks in
//! a window of 2^31 - 1 ticks ahead; a target beyond the window is armed
//! at the window's far edge and re-armed there at expiry until the
//! target itself lies inside -- a deadline is never early, however far
//! out.

/// The window a 32-bit armed tick can express ahead of now: the walk's
/// expiry test is a signed 32-bit difference.
pub const WINDOW: u64 = (1 << 31) - 1;

/// The 64-bit tick at which `now_ns() >= deadline_ns` is guaranteed: ceil
/// to the next tick boundary, then one more for the arm-race margin (a
/// tick firing between the deadline computation and the arm). A deadline
/// at the top of the range saturates instead of wrapping.
pub fn tick_for_deadline(deadline_ns: u64, ns_per_tick: u64) -> u64 {
    deadline_ns.div_ceil(ns_per_tick).saturating_add(1)
}

/// The 32-bit tick to arm for a 64-bit target from the 64-bit tick now:
/// the target itself when it lies inside the window, else the window's
/// far edge (the caller keeps the target and re-arms at expiry). The
/// second word says which.
pub fn arm(now: u64, target: u64) -> (u32, bool) {
    let ahead = target.saturating_sub(now);
    if ahead > WINDOW {
        (now.wrapping_add(WINDOW) as u32, true)
    } else {
        (target as u32, false)
    }
}

/// The timer walk's expiry test: the armed tick is due once now has
/// reached it, in the signed 32-bit window.
pub fn expired(now: u32, armed: u32) -> bool {
    (now.wrapping_sub(armed) as i32) >= 0
}
