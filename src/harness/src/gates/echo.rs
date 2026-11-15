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
//!
//! A second start and end ask for the echo's gathering mode. The shim
//! gate's serial ladder alone asks for it: its blocking read awaits two
//! echoed bytes against an inter-byte timer, so the pair must leave the
//! peer in one write. Every other caller takes the first pair, and the
//! echo answers it receive by receive as before.

use std::time::Duration;

use crate::tools::{echo_chardev, free_port, spawn_echo, Echo};
use crate::tools::{spawn_gathering_echo, EchoStep, GatheringEcho};

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

/// The gathering echo's start: the chardev argument and the running echo,
/// which writes back in one write what follows a receive within `hold`
/// and keeps a record of its receives and its writes.
pub fn start_gathering_echo(deadline: Duration, hold: Duration) -> Result<(String, GatheringEcho), String> {
    let port = free_port()?;
    Ok((echo_chardev(port), spawn_gathering_echo(port, deadline, hold)))
}

/// The gathering echo's end: the thread joined, its record answered.
pub fn end_gathering_echo(echo: GatheringEcho) -> Result<Vec<EchoStep>, String> {
    echo.finish()
}
