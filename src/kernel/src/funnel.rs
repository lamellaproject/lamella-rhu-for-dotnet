//! The terminal funnel: Rust panics and CPU exceptions both report
//! here, on one console discipline, and leave through one exit path
//! with distinct codes -- boot success exits 0, a kernel panic exits 1,
//! a CPU fault exits 2.
//!
//! Two faults leave the funnel alive: a double-precision form the
//! emulation computes (the UsageFault gate), and a null
//! data access from a program's thread that a hosting layer has asked for,
//! which the classifier queues for delivery through the activation
//! costume at the thread's next pick (the scheduler's fault seam). Every
//! other MemManage fault is named -- a stack overflow into the guard block
//! on the ARMv7-M rows, a null access that could not be delivered,
//! an instruction fetch below the first page -- and dies through the
//! terminal report, which carries one line on every fault taken from
//! a thread: its stack pointer against its carve.

use core::arch::{asm, global_asm};
use core::panic::PanicInfo;
use core::sync::atomic::{AtomicU32, Ordering};

use crate::board::FRAME_RANGES;
use crate::sched::{self, FaultRefusal};
use crate::{console, mpu, semihost};

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

const SCB_CFSR: *mut u32 = 0xE000_ED28 as *mut u32;
const SCB_HFSR: *const u32 = 0xE000_ED2C as *const u32;
const SCB_MMFAR: *const u32 = 0xE000_ED34 as *const u32;
const SCB_BFAR: *const u32 = 0xE000_ED38 as *const u32;

/// The MemManage status bits (CFSR[7:0]) the classifier reads.
const CFSR_IACCVIOL: u32 = 1 << 0;
const CFSR_DACCVIOL: u32 = 1 << 1;
const CFSR_MSTKERR: u32 = 1 << 4;
const CFSR_MMARVALID: u32 = 1 << 7;
const CFSR_MEMMANAGE_BITS: u32 = 0xFF;
/// HFSR.FORCED: a configurable fault escalated to HardFault.
const HFSR_FORCED: u32 = 1 << 30;

const IPSR_HARDFAULT: u32 = 3;
const IPSR_MEMMANAGE: u32 = 4;

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
        semihost::park_aborted(1);
    }
    console::fatal(); // a panic may predate console bring-up
    kprintln!();
    kprintln!("[panic] *** KERNEL PANIC ***");
    if let Some(loc) = info.location() {
        kprintln!("[panic] at {}:{}:{}", loc.file(), loc.line(), loc.column());
    }
    kprintln!("[panic] {}", info.message());
    kprintln!("[panic] *** halted ***");
    semihost::end(crate::exit_word::STATE_ABORTED, 1)
}

fn read_ipsr() -> u32 {
    let raw: u32;
    unsafe { asm!("mrs {}, IPSR", out(reg) raw, options(nomem, nostack)) };
    raw & 0x1FF
}

/// The exception entry: the double-precision emulation's gate first (a
/// UsageFault whose status names one of the two classes a core without the
/// double-precision extension raises for such a form -- an undefined
/// instruction on the Cortex-M33 and on a Cortex-M7 with a single-precision
/// FPU, a coprocessor access denied on the Cortex-M4F (the ARMv7-M
/// Architecture Reference Manual, B1.5 and B3.2.15) -- and whose instruction
/// decodes as such a form is computed and the program resumes past it; the
/// value returned is the trampoline's EXC_RETURN), then the null-page
/// guard's classifier (a MemManage fault for a data access below the first
/// page, from a program's thread, with the delivery armed: queued for the
/// thread's next pick and the same EXC_RETURN returned -- the pended switch
/// delivers before the thread executes another instruction), then the
/// terminal report.
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
                    console::fatal();
                    let pc = unsafe { (*frame).pc };
                    kprintln!("[fault] double-precision form {} at {:#010x}: not emulated -- {}", crate::fpemul::name_at(pc), pc, reason);
                }
            }
        }
    }
    let refusal = classify_null_access(frame, exc_return);
    match refusal {
        Classified::Queued => exc_return,
        Classified::Refused(r) => terminal(frame, exc_return, Some(r)),
        Classified::NotOurs => terminal(frame, exc_return, None),
    }
}

enum Classified {
    Queued,
    Refused(FaultRefusal),
    NotOurs,
}

/// The null-page guard's classifier: a MemManage fault whose status says a
/// DATA access violation with the faulting address valid and below the
/// first page is a null access. The scheduler's seam decides whether it is
/// deliverable; when it is, the fault's status bits are cleared and the
/// thread resumes through the pended switch into the costume. An
/// instruction fetch below the page (IACCVIOL) is never delivered: its
/// faulting address is the null address itself, outside every managed
/// frame, so the runtime could only decline it. A forced HardFault (the
/// fault raised under the kernel's mask or inside a handler of equal or
/// higher urgency) is always terminal: the report names it.
fn classify_null_access(frame: *mut ExcFrame, exc_return: u32) -> Classified {
    if read_ipsr() != IPSR_MEMMANAGE {
        return Classified::NotOurs;
    }
    let cfsr = unsafe { SCB_CFSR.read_volatile() };
    if cfsr & CFSR_DACCVIOL == 0 || cfsr & CFSR_MMARVALID == 0 {
        return Classified::NotOurs;
    }
    let addr = unsafe { SCB_MMFAR.read_volatile() };
    if addr >= mpu::NULL_PAGE_BYTES {
        return Classified::NotOurs;
    }
    let access = if frame_plausible(frame) { access_class(unsafe { (*frame).pc }) } else { sched::ACCESS_UNKNOWN };
    match sched::fault_queue(sched::FAULT_NULL, access, addr, cfsr & CFSR_MEMMANAGE_BITS, exc_return, frame as u32) {
        Ok(()) => {
            // The status bits are write-one-to-clear: the next fault
            // starts from a clean word.
            unsafe { SCB_CFSR.write_volatile(cfsr & CFSR_MEMMANAGE_BITS) };
            Classified::Queued
        }
        Err(r) => Classified::Refused(r),
    }
}

fn frame_plausible(frame: *const ExcFrame) -> bool {
    let f = frame as usize;
    FRAME_RANGES.iter().any(|&(lo, hi)| f >= lo && f < hi)
}

/// The access class of the instruction at `pc` (the faulting data access:
/// the stacked pc of a precise data fault is the instruction itself):
/// a load, a store, or unknown. The 16-bit load/store forms by their
/// opcode fields (the ARMv7-M ARM, A5.2.4 and A5.2.5); the 32-bit
/// load/store single, multiple, dual, exclusive and floating-point
/// transfer families all carry the load bit at hw1[4]. The pc lies in
/// code (the fetch succeeded), so the read is safe.
fn access_class(pc: u32) -> u8 {
    let hw1 = unsafe { ((pc & !1) as *const u16).read_volatile() } as u32;
    match hw1 >> 11 {
        0b01001 => sched::ACCESS_READ, // LDR (literal)
        0b01010 | 0b01011 => {
            // The register forms: STR, STRH, STRB (op 0..2), then LDRSB,
            // LDR, LDRH, LDRB, LDRSH.
            if (hw1 >> 9) & 7 < 3 { sched::ACCESS_WRITE } else { sched::ACCESS_READ }
        }
        0b01100 | 0b01110 | 0b10000 | 0b10010 => sched::ACCESS_WRITE, // STR/STRB/STRH imm, STR sp-relative
        0b01101 | 0b01111 | 0b10001 | 0b10011 => sched::ACCESS_READ, // LDR/LDRB/LDRH imm, LDR sp-relative
        0b11000 => sched::ACCESS_WRITE, // STM
        0b11001 => sched::ACCESS_READ,  // LDM
        0b10110 | 0b10111 => {
            if hw1 & 0xFE00 == 0xB400 {
                sched::ACCESS_WRITE // PUSH
            } else if hw1 & 0xFE00 == 0xBC00 {
                sched::ACCESS_READ // POP
            } else {
                sched::ACCESS_UNKNOWN
            }
        }
        0b11101 | 0b11110 | 0b11111 => {
            let family = hw1 & 0xFE00;
            // Load/store single (1111 100x), multiple/dual/exclusive
            // (1110 100x), floating-point and coprocessor transfers
            // (1110 110x): the load bit at hw1[4].
            if family == 0xF800 || family == 0xE800 || family == 0xEC00 {
                if hw1 & (1 << 4) != 0 { sched::ACCESS_READ } else { sched::ACCESS_WRITE }
            } else {
                sched::ACCESS_UNKNOWN
            }
        }
        _ => sched::ACCESS_UNKNOWN,
    }
}

fn access_word(class: u8) -> &'static str {
    match class {
        sched::ACCESS_READ => "read",
        sched::ACCESS_WRITE => "write",
        _ => "access",
    }
}

/// The runtime declined a delivered fault (its handler handed the context
/// back with the pc unchanged): the belt under hosting's death rule, run
/// from the resume door. One line names the access, then the program ends
/// through the terminal exit -- never a second delivery of the same fault.
pub fn fault_declined(tid: i32, name: &str, access: u8, addr: u32, pc: u32) -> ! {
    if TERMINAL_DEPTH.fetch_add(1, Ordering::SeqCst) > 0 {
        semihost::park_aborted(2);
    }
    mpu::off();
    console::fatal();
    kprintln!();
    kprintln!(
        "[fault] null access: {} at {:#010x} from thread {} {} (the first page is no-access; declined by the runtime's handler: the fault context came back with its pc {:#010x} unchanged, so a resume would only fault again)",
        access_word(access),
        addr,
        tid,
        name,
        pc
    );
    kprintln!("[fault] *** halted ***");
    semihost::end(crate::exit_word::STATE_ABORTED, 2)
}

/// The words for a refusal, in the null line.
fn refusal_words(r: &FaultRefusal) -> &'static str {
    match r {
        FaultRefusal::NotStarted => "not delivered: raised by the kernel's boot thread",
        FaultRefusal::HandlerMode => "not delivered: raised in handler mode, the kernel's own code",
        FaultRefusal::Idle => "not delivered: raised by the idle thread",
        FaultRefusal::InActHandler => "not delivered: raised inside the activation handler",
        FaultRefusal::Masked => "not delivered: raised with the kernel's interrupt mask set",
        FaultRefusal::NotArmed => "not delivered: no hosting layer has armed the delivery",
        FaultRefusal::NoHandler => "not delivered: the kind is armed but no activation handler is registered",
        FaultRefusal::StackRoom(_) => "not delivered: too little stack above the thread's floor for the delivery",
    }
}

/// Who raised the fault, from EXC_RETURN and the scheduler's state: a
/// thread by tid and name (thread mode on the process stack with the
/// scheduler started), handler mode, or the kernel's boot thread.
fn print_null_line(kind: &str, addr: u32, exc_return: u32, thread: &Option<sched::FaultThread>, reason: &str) {
    if exc_return & 0x8 == 0 {
        kprintln!("[fault] null access: {} at {:#010x} from handler mode (the first page is no-access; {})", kind, addr, reason);
    } else if let (true, Some(t)) = (exc_return & 0x4 != 0, thread) {
        let name = core::str::from_utf8(&t.name[..t.name_len]).unwrap_or("?");
        kprintln!("[fault] null access: {} at {:#010x} from thread {} {} (the first page is no-access; {})", kind, addr, t.tid, name, reason);
    } else {
        kprintln!("[fault] null access: {} at {:#010x} from the kernel's boot thread (the first page is no-access; {})", kind, addr, reason);
    }
}

/// The terminal report: never returns (the emulator rows exit, silicon parks).
fn terminal(frame: *const ExcFrame, exc_return: u32, refusal: Option<FaultRefusal>) -> ! {
    if TERMINAL_DEPTH.fetch_add(1, Ordering::SeqCst) > 0 {
        semihost::park_aborted(2);
    }
    // The protection unit off before anything is read: a dump that reads
    // a frame at a floor, inside a guard block, must not fault a second
    // time inside the report. The program is ending.
    mpu::off();
    console::fatal(); // the exception may predate console bring-up

    let ipsr = read_ipsr();

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

    // The v8-M stack limit: when the floor was crossed, name it and say
    // what the dump below is worth -- the architecture writes no frame
    // when the entry's own stacking would cross the limit and leaves the
    // stack pointer at the floor, so the words there may be the floor's
    // neighbourhood rather than a frame. The limit of the faulting
    // context: the thread's when the exception return says thread mode
    // on the process stack, the handler's otherwise.
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

    let plausible = frame_plausible(frame);
    let thread_mode_psp = exc_return & 0x8 != 0 && exc_return & 0x4 != 0;
    let thread = if thread_mode_psp { sched::fault_thread() } else { None };
    let memmanage = ipsr == IPSR_MEMMANAGE || (ipsr == IPSR_HARDFAULT && hfsr & HFSR_FORCED != 0);

    // The v7-M stack floor: a MemManage into the running thread's
    // guard block -- by the exception entry's own stacking, or by the
    // instruction the frame names -- is a stack overflow, named like the
    // v8-M limit's; a descent that jumped the block is named by the
    // bounds line below at the fault it causes.
    if mpu::GUARD_ON && memmanage {
        if let Some(t) = &thread {
            let (glo, ghi) = mpu::guard_block(t.base);
            let name = core::str::from_utf8(&t.name[..t.name_len]).unwrap_or("?");
            if cfsr & CFSR_MSTKERR != 0 {
                kprintln!(
                    "[fault] stack overflow: thread {} {}'s stack [{:#010x}, {:#010x}) -- the guard block [{:#010x}, {:#010x}) at its floor was crossed by the exception entry's stacking; the frame dump below may be unstacked",
                    t.tid, name, t.base, t.top, glo, ghi
                );
            } else if cfsr & CFSR_DACCVIOL != 0 && cfsr & CFSR_MMARVALID != 0 && mmfar >= glo && mmfar < ghi {
                let pc = if plausible { unsafe { (*frame).pc } } else { 0 };
                kprintln!(
                    "[fault] stack overflow: thread {} {}'s stack [{:#010x}, {:#010x}) -- the guard block [{:#010x}, {:#010x}) at its floor was crossed by the instruction at {:#010x} (a {} into the block); the frame dump below may be unstacked",
                    t.tid, name, t.base, t.top, glo, ghi, pc, access_word(access_class(pc))
                );
            }
        }
    }

    // The null-page guard's naming: a data access below the first
    // page that was not delivered, with the reason; an instruction fetch
    // below it, never delivered.
    if memmanage && cfsr & CFSR_DACCVIOL != 0 && cfsr & CFSR_MMARVALID != 0 && mmfar < mpu::NULL_PAGE_BYTES {
        let access = if plausible { access_class(unsafe { (*frame).pc }) } else { sched::ACCESS_UNKNOWN };
        let reason = match &refusal {
            Some(r) => refusal_words(r),
            None if ipsr == IPSR_HARDFAULT => "not delivered: raised with the kernel's interrupt mask set or inside a handler of equal or higher urgency, escalated to HardFault",
            None => "not delivered",
        };
        print_null_line(access_word(access), mmfar, exc_return, &thread, reason);
        if let Some(FaultRefusal::StackRoom(room)) = &refusal {
            kprintln!("[fault] the thread's room above its floor at the fault: {} bytes; the delivery needs the switch's save, 148 bytes and the guard's slack", room);
        }
    } else if memmanage && cfsr & CFSR_IACCVIOL != 0 && plausible {
        let pc = unsafe { (*frame).pc };
        if pc < mpu::NULL_PAGE_BYTES {
            print_null_line("fetch", pc, exc_return, &thread, "an instruction fetch below the page is never delivered: no managed frame to throw from");
        }
    }

    // One line on every fault taken from a thread: its stack pointer at
    // the entry against its carve, so a descent that jumped a guard is
    // named at the fault it causes.
    if let Some(t) = &thread {
        let name = core::str::from_utf8(&t.name[..t.name_len]).unwrap_or("?");
        let sp = frame as u32;
        if sp >= t.base && sp < t.top {
            kprintln!("[fault] thread {} {}: sp {:#010x} against its carve [{:#010x}, {:#010x}): inside", t.tid, name, sp, t.base, t.top);
        } else if sp < t.base {
            kprintln!(
                "[fault] thread {} {}: sp {:#010x} against its carve [{:#010x}, {:#010x}): {} bytes below the floor",
                t.tid, name, sp, t.base, t.top, t.base - sp
            );
        } else {
            kprintln!(
                "[fault] thread {} {}: sp {:#010x} against its carve [{:#010x}, {:#010x}): {} bytes above the top",
                t.tid, name, sp, t.base, t.top, sp - t.top
            );
        }
    }

    // Dereference the frame only if it points into plausible RAM: a wild
    // SP must not fault the dump itself.
    let f = frame as usize;
    if plausible {
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
    semihost::end(crate::exit_word::STATE_ABORTED, 2)
}

/// Print one line per set CFSR status bit, by its architectural name
/// (ARMv7-M Configurable Fault Status Register).
fn decode_cfsr(cfsr: u32) {
    if cfsr == 0 {
        return;
    }
    const BITS: [(u32, &str); 20] = [
        // UsageFault status, CFSR[31:16]
        (16, "UNDEFINSTR undefined instruction"),
        (17, "INVSTATE invalid EPSR state"),
        (18, "INVPC invalid EXC_RETURN"),
        (19, "NOCP coprocessor access denied"),
        (20, "STKOF stack limit violation (the v8-M limit registers)"),
        (24, "UNALIGNED unaligned access trap"),
        (25, "DIVBYZERO divide by zero"),
        // BusFault status, CFSR[15:8]
        (8, "IBUSERR instruction-fetch bus error"),
        (9, "PRECISERR precise data bus error (address in BFAR)"),
        (10, "IMPRECISERR imprecise data bus error"),
        (11, "UNSTKERR exception-return unstacking error"),
        (12, "STKERR exception-entry stacking error"),
        (13, "LSPERR lazy FP stacking error"),
        (15, "BFARVALID"),
        // MemManage status, CFSR[7:0]
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
