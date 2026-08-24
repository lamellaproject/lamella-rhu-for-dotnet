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
    b     __rhu_exception
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

#[unsafe(no_mangle)]
extern "C" fn __rhu_exception(frame: *const ExcFrame, exc_return: u32) -> ! {
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
    const BITS: [(u32, &str); 19] = [
        (16, "UNDEFINSTR undefined instruction"),
        (17, "INVSTATE invalid EPSR state"),
        (18, "INVPC invalid EXC_RETURN"),
        (19, "NOCP coprocessor access denied"),
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
