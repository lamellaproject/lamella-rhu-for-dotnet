//! The host critical section: a plain call. The tier is single-threaded
//! and nothing masks; the included modules' atomicity claims are the
//! firmware's, proven on the emulator rows and on silicon.

pub fn with_cs<R>(f: impl FnOnce() -> R) -> R {
    f()
}
