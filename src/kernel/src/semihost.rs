//! ARM semihosting exit paths (feature `qemu-exit`). Without a debugger
//! attached, BKPT #0xAB escalates to HardFault on real silicon, so
//! non-QEMU builds turn every exit into a quiet park.

use core::arch::asm;

#[cfg(feature = "qemu-exit")]
const SYS_EXIT: u32 = 0x18;
#[cfg(feature = "qemu-exit")]
const SYS_EXIT_EXTENDED: u32 = 0x20;
#[cfg(feature = "qemu-exit")]
const ADP_STOPPED_APPLICATION_EXIT: u32 = 0x20026;

#[cfg(feature = "qemu-exit")]
unsafe fn call(op: u32, param: u32) -> u32 {
    let ret;
    asm!("bkpt #0xab", inlateout("r0") op => ret, in("r1") param, options(nostack));
    ret
}

/// Terminate the machine with an exit code (QEMU) or park (silicon).
/// SYS_EXIT_EXTENDED carries the code into the host's exit status;
/// plain SYS_EXIT follows for semihosting hosts without it (the code is
/// lost there).
pub fn exit(code: u32) -> ! {
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
