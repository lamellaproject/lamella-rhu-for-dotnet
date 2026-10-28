//! The wire's echo on an image's second UART, shared by every gate whose
//! image talks on it: the api gate's UART cases, the shim gate's serial
//! device-file ladder and the hosted-runtime suite's serial section. One
//! copy of the start and the end; the thread, the port and the chardev
//! argument live in the tools module.
//!
//! The start takes a free TCP port on the loopback, spells the QEMU chardev
//! argument that listens on it (the image's second `-serial`), and spawns
//! the thread that connects with retries and writes back every byte it
//! reads until the peer closes or the deadline passes. The end raises the
//! stop flag, joins the thread and answers the bytes echoed; a complaint
//! is the caller's failure to report in its own words.

use std::time::Duration;

use crate::tools::{echo_chardev, free_port, spawn_echo, Echo};

/// The echo's start: the chardev argument and the running echo. The
/// deadline bounds the run alone, so it is spawned after the case's build.
pub fn start_echo(deadline: Duration) -> Result<(String, Echo), String> {
    let port = free_port()?;
    Ok((echo_chardev(port), spawn_echo(port, deadline)))
}

/// The echo's end: the thread joined, the count of bytes echoed answered.
pub fn end_echo(echo: Echo) -> Result<u64, String> {
    echo.finish()
}
