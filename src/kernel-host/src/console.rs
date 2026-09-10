//! The host console: the kernel's line macro over the standard output
//! (the test harness captures it per test).

macro_rules! kprintln {
    () => {{
        println!();
    }};
    ($($arg:tt)*) => {{
        println!($($arg)*);
    }};
}
