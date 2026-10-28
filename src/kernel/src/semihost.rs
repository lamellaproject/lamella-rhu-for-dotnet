//! The machine's end: the exit word stored, the held wires drained, then
//! the ARM semihosting exit (feature `qemu-exit`) or the park. Without a
//! debugger attached, BKPT #0xAB escalates to HardFault on real silicon,
//! so non-QEMU builds turn every exit into a quiet park.

use core::arch::asm;

use crate::board::ring;
use crate::exit_word;

#[cfg(feature = "qemu-exit")]
const SYS_EXIT: u32 = 0x18;
#[cfg(feature = "qemu-exit")]
const SYS_EXIT_EXTENDED: u32 = 0x20;
#[cfg(feature = "qemu-exit")]
const ADP_STOPPED_APPLICATION_EXIT: u32 = 0x20026;

#[cfg(feature = "qemu-exit")]
unsafe fn call(op: u32, param: u32) -> u32 { unsafe {
    let ret;
    asm!("bkpt #0xab", inlateout("r0") op => ret, in("r1") param, options(nostack));
    ret
}}

/// The program's end (the two exit doors and the fault funnel): the
/// census line, the handler stack's watermark line where the image measures
/// it, the exit code and the end magic stored into the mirror's
/// header, the held wires drained, then the semihosting exit
/// (the emulator rows) or the park (silicon).
pub fn end(state: u32, code: i32) -> ! {
    // The census line every gated transcript ends with: the lost-tick
    // witness's counts (or why there are none), on every exit path.
    crate::sched::tick_witness_print();
    crate::stack_watermark::report();
    ring::end_mark(state, code);
    // A UART wire the console holds through the UART doors' module drains
    // through its interrupt; the exit waits for its ring and transmitter so
    // the transcript is on the wire whole before the machine stops.
    crate::uart::drain_held();
    leave(code as u32)
}

/// The kernel's own images' exit (a boot flavor's tail, a verification
/// image's verdict): the census line, the watermark line where the image
/// measures it, the end magic as returned with the
/// code whole, the held wires drained, then the exit or the park. No path
/// prints a line for the end, so every recorded transcript ends with the
/// census line.
pub fn exit(code: u32) -> ! {
    crate::sched::tick_witness_print();
    crate::stack_watermark::report();
    ring::end_mark(exit_word::STATE_RETURNED, code as i32);
    crate::uart::drain_held();
    leave(code)
}

/// A second terminal event inside the terminal report: the end magic as
/// aborted with the code, then the park; nothing more is printed.
pub fn park_aborted(code: i32) -> ! {
    ring::end_mark(exit_word::STATE_ABORTED, code);
    park()
}

/// Terminate the machine with an exit code (QEMU) or park (silicon).
/// SYS_EXIT_EXTENDED carries the code into the host's exit status;
/// plain SYS_EXIT follows for semihosting hosts without it (the code is
/// lost there).
fn leave(code: u32) -> ! {
    #[cfg(feature = "qemu-exit")]
    unsafe {
        let block = [ADP_STOPPED_APPLICATION_EXIT, code];
        call(SYS_EXIT_EXTENDED, block.as_ptr() as u32);
        call(SYS_EXIT, ADP_STOPPED_APPLICATION_EXIT);
    }
    #[cfg(not(feature = "qemu-exit"))]
    let _ = code;
    park()
}

/// Low-power idle, forever.
pub fn park() -> ! {
    loop {
        unsafe { asm!("wfi", options(nomem, nostack)) };
    }
}
