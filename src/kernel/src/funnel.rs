//! The terminal funnel: Rust panics and CPU exceptions both report
//! here, on one console discipline, and leave through one exit path
//! with distinct codes -- boot success exits 0, a kernel panic exits 1,
//! a CPU fault exits 2.

use core::arch::{asm, global_asm};
use core::panic::PanicInfo;
use core::sync::atomic::{AtomicU32, Ordering};

use crate::board::FRAME_RANGES;
use crate::{console, semihost};

/// Hardware-pushed exception frame (basic 8-word layout; an extended FP
/// frame carries the same first 8 words -- EXC_RETURN bit 4 says which).
#[repr(C)]
pub struct ExcFrame {
    pub r0: u32,
    pub r1: u32,
    pub r2: u32,
    pub r3: u32,
    pub r12: u32,
    pub lr: u32,
    pub pc: u32,
    pub xpsr: u32,
}

global_asm!(
    r#"
    .section .text.ExceptionTrampoline, "ax", %progbits
    .global ExceptionTrampoline
    .type ExceptionTrampoline, %function
    .thumb_func
ExceptionTrampoline:
    tst   lr, #4          /* EXC_RETURN bit 2: which stack holds the frame */
    ite   eq
    mrseq r0, msp
    mrsne r0, psp
    mov   r1, lr
    push  {{r4, lr}}      /* the exception return value kept; the call returns it in r0 */
    bl    __rhu_exception
    pop   {{r4, lr}}
    bx    r0
    .size ExceptionTrampoline, . - ExceptionTrampoline
"#
);

const SCB_CFSR: *const u32 = 0xE000_ED28 as *const u32;
const SCB_HFSR: *const u32 = 0xE000_ED2C as *const u32;
const SCB_MMFAR: *const u32 = 0xE000_ED34 as *const u32;
const SCB_BFAR: *const u32 = 0xE000_ED38 as *const u32;

/// Shared terminal-entry guard. Nonzero once a report has started: a
/// second entry (a fault inside a dump, a panic inside a panic print)
/// parks silently instead of recursing into a console that is now
/// suspect.
static TERMINAL_DEPTH: AtomicU32 = AtomicU32::new(0);

fn exception_name(ipsr: u32) -> &'static str {
    match ipsr {
        2 => "NMI",
        3 => "HardFault",
        4 => "MemManage",
        5 => "BusFault",
        6 => "UsageFault",
        11 => "SVCall",
        12 => "DebugMonitor",
        14 => "PendSV",
        15 => "SysTick",
        _ => "external IRQ",
    }
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    if TERMINAL_DEPTH.fetch_add(1, Ordering::SeqCst) > 0 {
        semihost::park();
    }
    console::init();
    kprintln!();
    kprintln!("[panic] *** KERNEL PANIC ***");
    if let Some(loc) = info.location() {
        kprintln!("[panic] at {}:{}:{}", loc.file(), loc.line(), loc.column());
    }
    kprintln!("[panic] {}", info.message());
    kprintln!("[panic] *** halted ***");
    semihost::exit(1)
}

/// The exception entry: the double-precision emulation's gate first (a
/// UsageFault whose status names one of the two classes a core without the
/// double-precision extension raises for such a form -- an undefined
/// instruction on the Cortex-M33 and on a Cortex-M7 with a single-precision
/// FPU, a coprocessor access denied on the Cortex-M4F (the ARMv7-M
/// Architecture Reference Manual, B1.5 and B3.2.15) -- and whose instruction
/// decodes as such a form is computed and the program resumes past it; the
/// value returned is the trampoline's EXC_RETURN), then the terminal report.
#[unsafe(no_mangle)]
extern "C" fn __rhu_exception(frame: *mut ExcFrame, exc_return: u32) -> u32 {
    #[cfg(feature = "fpemul")]
    {
        let raw_ipsr: u32;
        unsafe { asm!("mrs {}, IPSR", out(reg) raw_ipsr, options(nomem, nostack)) };
        let class = unsafe { SCB_CFSR.read_volatile() } & crate::fpemul::CFSR_FORM_CLASSES;
        if raw_ipsr & 0x1FF == 6 && class != 0 {
            match crate::fpemul::trap(frame, exc_return, class) {
                crate::fpemul::Trap::Emulated(out) => return out,
                crate::fpemul::Trap::NotOurs => {}
                crate::fpemul::Trap::Refused(reason) => {
                    console::init();
                    let pc = unsafe { (*frame).pc };
                    kprintln!("[fault] double-precision form {} at {:#010x}: not emulated -- {}", crate::fpemul::name_at(pc), pc, reason);
                }
            }
        }
    }
    terminal(frame, exc_return)
}

/// The terminal report: never returns (the emulator rows exit, silicon parks).
fn terminal(frame: *const ExcFrame, exc_return: u32) -> ! {
    if TERMINAL_DEPTH.fetch_add(1, Ordering::SeqCst) > 0 {
        semihost::park();
    }
    console::init();

    let raw_ipsr: u32;
    unsafe { asm!("mrs {}, IPSR", out(reg) raw_ipsr, options(nomem, nostack)) };
    let ipsr = raw_ipsr & 0x1FF;

    kprintln!();
    kprintln!("[fault] *** EXCEPTION: {} (IPSR={}) ***", exception_name(ipsr), ipsr);

    let (cfsr, hfsr, mmfar, bfar) = unsafe {
        (
            SCB_CFSR.read_volatile(),
            SCB_HFSR.read_volatile(),
            SCB_MMFAR.read_volatile(),
            SCB_BFAR.read_volatile(),
        )
    };
    kprintln!(
        "[fault] CFSR={:#010x} HFSR={:#010x} MMFAR={:#010x} BFAR={:#010x}",
        cfsr,
        hfsr,
        mmfar,
        bfar
    );
    decode_cfsr(cfsr);

    #[cfg(armv8m)]
    if cfsr & (1 << 20) != 0 {
        let floor: u32;
        let which = if exc_return & 0x4 != 0 {
            unsafe { asm!("mrs {}, PSPLIM", out(reg) floor, options(nomem, nostack)) };
            "thread"
        } else {
            unsafe { asm!("mrs {}, MSPLIM", out(reg) floor, options(nomem, nostack)) };
            "handler"
        };
        kprintln!(
            "[fault] stack overflow: the {} stack's floor {:#010x} (the limit register) was crossed -- the frame dump below may be unstacked",
            which,
            floor
        );
    }

    let f = frame as usize;
    if FRAME_RANGES.iter().any(|&(lo, hi)| f >= lo && f < hi) {
        let fr = unsafe { &*frame };
        kprintln!(
            "[fault] r0={:#010x} r1={:#010x} r2={:#010x} r3={:#010x}",
            fr.r0,
            fr.r1,
            fr.r2,
            fr.r3
        );
        kprintln!(
            "[fault] r12={:#010x} lr={:#010x} pc={:#010x} xpsr={:#010x}",
            fr.r12,
            fr.lr,
            fr.pc,
            fr.xpsr
        );
    } else {
        kprintln!("[fault] frame pointer {:#010x} implausible -- register dump skipped", f);
    }
    kprintln!(
        "[fault] EXC_RETURN={:#010x} ({} frame on {})",
        exc_return,
        if exc_return & 0x10 == 0 { "extended FP" } else { "basic" },
        if exc_return & 0x4 == 0 { "MSP" } else { "PSP" }
    );

    kprintln!("[fault] *** halted ***");
    semihost::exit(2)
}

/// Print one line per set CFSR status bit, by its architectural name
/// (ARMv7-M Configurable Fault Status Register).
fn decode_cfsr(cfsr: u32) {
    if cfsr == 0 {
        return;
    }
    const BITS: [(u32, &str); 20] = [
        (16, "UNDEFINSTR undefined instruction"),
        (17, "INVSTATE invalid EPSR state"),
        (18, "INVPC invalid EXC_RETURN"),
        (19, "NOCP coprocessor access denied"),
        (20, "STKOF stack limit violation (the v8-M limit registers)"),
        (24, "UNALIGNED unaligned access trap"),
        (25, "DIVBYZERO divide by zero"),
        (8, "IBUSERR instruction-fetch bus error"),
        (9, "PRECISERR precise data bus error (address in BFAR)"),
        (10, "IMPRECISERR imprecise data bus error"),
        (11, "UNSTKERR exception-return unstacking error"),
        (12, "STKERR exception-entry stacking error"),
        (13, "LSPERR lazy FP stacking error"),
        (15, "BFARVALID"),
        (0, "IACCVIOL instruction access violation"),
        (1, "DACCVIOL data access violation (address in MMFAR)"),
        (3, "MUNSTKERR exception-return unstacking violation"),
        (4, "MSTKERR exception-entry stacking violation"),
        (5, "MLSPERR lazy FP stacking violation"),
        (7, "MMARVALID"),
    ];
    for (bit, name) in BITS {
        if cfsr & (1 << bit) != 0 {
            kprintln!("[fault] CFSR[{}] {}", bit, name);
        }
    }
}
