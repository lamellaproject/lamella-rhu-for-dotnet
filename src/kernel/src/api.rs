//! The RHU hosting API -- the C-ABI waistline between the kernel and
//! hosted payloads. One hundred and eleven functions, `rhu_`-prefixed, mirrored
//! exactly by `include/rhu.h` (contract generation `ABI`, at the end
//! of this file); the shim backend is the intended consumer. One rule
//! binds this surface: no second blocking primitive ever joins it, and
//! new needs are shim recipes over the blocking four (join, for one,
//! is a futex recipe on a thread-exit word -- never a verb here).

use core::ffi::{c_char, c_void};

use crate::registry::ModuleDesc;
use crate::{adc, arena, blk, bus, calendar, console, entropy, gpio, kheap, net, pwm, ram, registry, sched, semihost, uart};

#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.rhu_api")]
pub extern "C" fn rhu_now_ns() -> u64 {
    sched::now_ns()
}

#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.rhu_api")]
pub extern "C" fn rhu_sleep_ns(ns: u64) {
    sched::sleep_ns(ns)
}

#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.rhu_api")]
pub extern "C" fn rhu_futex_wait(addr: *const u32, expected: u32, deadline_ns: u64) -> i32 {
    sched::futex_wait(addr, expected, deadline_ns)
}

#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.rhu_api")]
pub extern "C" fn rhu_futex_wake(addr: *const u32, n: u32) -> i32 {
    sched::futex_wake(addr, n)
}

#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.rhu_api")]
pub extern "C" fn rhu_arena_alloc_at(bytes: u32, align: u32, site: u32) -> *mut u8 {
    arena::alloc_at(bytes, align, site)
}

#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.rhu_api")]
pub extern "C" fn rhu_arena_rezero(p: *mut u8, bytes: u32) -> i32 {
    arena::rezero(p, bytes)
}

#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.rhu_api")]
pub extern "C" fn rhu_arena_headroom() -> u32 {
    arena::headroom()
}

#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.rhu_api")]
pub extern "C" fn rhu_arena_report() {
    arena::report()
}

// ---------------- kheap / registry / TLS / threads ----------------

#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.rhu_api")]
pub extern "C" fn rhu_kheap_alloc(bytes: u32, align: u32) -> *mut u8 {
    kheap::alloc(bytes, align)
}

#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.rhu_api")]
pub extern "C" fn rhu_kheap_free(p: *mut u8) {
    kheap::free(p)
}

#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.rhu_api")]
pub extern "C" fn rhu_module_register(d: *const ModuleDesc) -> i32 {
    assert!(!d.is_null(), "module register: null desc (caller bug)");
    registry::register(unsafe { &*d })
}

#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.rhu_api")]
pub extern "C" fn rhu_tls_get(module: i32) -> *mut u8 {
    sched::tls_get(module)
}

#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.rhu_api")]
pub extern "C" fn rhu_thread_atexit(f: Option<extern "C" fn(*mut c_void)>, arg: *mut c_void) -> i32 {
    let f = f.expect("thread atexit: null dtor (caller bug)");
    sched::atexit_push(f, arg)
}

/// The hosted band and its one flat level, spelled once (mirrored by
/// `include/rhu.h`, which carries the consumer text).
pub const HOSTED_PRIO_LO: u32 = 0x10;
pub const HOSTED_PRIO_HI: u32 = 0xE0;
#[allow(dead_code)] pub const HOSTED_PRIO_FLAT: u32 = 0x80; // the hosting seam's level: the payload and api-smoke flavors consume it

#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.rhu_api")]
pub extern "C" fn rhu_thread_spawn(
    entry: Option<extern "C" fn(*mut c_void)>,
    arg: *mut c_void,
    stack_bytes: u32,
    prio: u32,
    name: *const c_char,
) -> i32 {
    let entry = entry.expect("thread spawn: null entry (caller bug)");
    // Same ABI shape (one pointer-sized argument, C calling
    // convention), so the fn-pointer cast is sound; the kernel-side
    // signature keeps the scheduler's existing type.
    let entry: extern "C" fn(usize) = unsafe { core::mem::transmute(entry) };
    // The stack floor: the frames the birth writes and an activation
    // delivery need RHU_THREAD_STACK_MIN bytes, and a size whose 8-byte
    // round-up would wrap is no size at all -- both refused before
    // anything moves (RHU_EAGAIN stays the honest exhaustion).
    if (stack_bytes as usize) < sched::THREAD_STACK_MIN || stack_bytes > u32::MAX - 7 {
        return sched::ERR_INVAL;
    }
    let prio = prio.clamp(HOSTED_PRIO_LO, HOSTED_PRIO_HI) as u8;
    // The name is COPIED into the control block at birth (at most
    // NAME_BYTES - 1 kept, cut at a character boundary), so the caller's
    // buffer may go away once this door returns; only the bytes the copy
    // can keep are read (a wild pointer faults loud within them). NULL
    // reads as "hosted"; a byte sequence that is not UTF-8 falls back to
    // it rather than lying in the debug contract; a character the limit
    // cuts is dropped whole.
    let mut name_bytes = [0u8; sched::NAME_BYTES];
    let name: &str = if name.is_null() {
        "hosted"
    } else {
        let mut n = 0usize;
        unsafe {
            while n < sched::NAME_BYTES - 1 && *name.add(n) != 0 {
                name_bytes[n] = *name.add(n) as u8;
                n += 1;
            }
        }
        match core::str::from_utf8(&name_bytes[..n]) {
            Ok(s) => s,
            Err(e) if e.error_len().is_none() => {
                core::str::from_utf8(&name_bytes[..e.valid_up_to()]).unwrap_or("hosted")
            }
            Err(_) => "hosted",
        }
    };
    match sched::try_spawn(name, entry, arg as usize, prio, stack_bytes as usize) {
        Ok((_, tid)) => tid,
        Err(why) => {
            // The honest refusal, every reason RHU_EAGAIN, and one loud line
            // naming the reason: a program that sees the code sees why, and
            // a stack request sees the pool it was refused from.
            match why {
                sched::SpawnErr::PoolExhausted => {
                    let (pool, free, retired) = sched::pool_remainder();
                    kprintln!(
                        "[sched] spawn refused: a stack of {} bytes asked; the pool of {} bytes has {} never carved and {} retired stacks",
                        stack_bytes, pool, free, retired
                    );
                }
                sched::SpawnErr::NoSlot => kprintln!("[sched] spawn refused: every thread slot taken ({} slots)", sched::MAX_THREADS),
                sched::SpawnErr::TlsExhausted => kprintln!("[sched] spawn refused: a module's thread-local block for the new thread was refused"),
                sched::SpawnErr::TidExhausted => kprintln!("[sched] spawn refused: the thread-id space is spent"),
            }
            sched::ERR_AGAIN
        }
    }
}

#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.rhu_api")]
pub extern "C" fn rhu_thread_exit() -> ! {
    sched::exit_current()
}

#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.rhu_api")]
pub extern "C" fn rhu_thread_self() -> i32 {
    sched::self_tid()
}

#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.rhu_api")]
pub extern "C" fn rhu_thread_yield() {
    sched::yield_now()
}

#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.rhu_api")]
pub extern "C" fn rhu_thread_stack_bounds(tid: i32, lo: *mut u32, bytes: *mut u32) -> i32 {
    if lo.is_null() || bytes.is_null() {
        return sched::ERR_INVAL; // the futex null convention
    }
    match sched::stack_bounds_by_tid(tid) {
        Some((base, len)) => {
            unsafe {
                lo.write(base);
                bytes.write(len);
            }
            0
        }
        None => sched::ERR_SRCH,
    }
}

// ---------------- suspension + activation ----------------

#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.rhu_api")]
pub extern "C" fn rhu_thread_suspend(tid: i32, out_ctx: *mut sched::RhuCtx) -> i32 {
    // The futex conventions: null refused, and a context is a word
    // array, so a pointer that is not 4-aligned is refused before any
    // write through it.
    if out_ctx.is_null() || (out_ctx as usize) & 3 != 0 {
        return sched::ERR_INVAL;
    }
    sched::suspend_by_tid(tid, out_ctx)
}

#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.rhu_api")]
pub extern "C" fn rhu_thread_set_context(tid: i32, ctx: *const sched::RhuCtx) -> i32 {
    if ctx.is_null() || (ctx as usize) & 3 != 0 {
        return sched::ERR_INVAL; // null or misaligned: the futex conventions
    }
    sched::set_context_by_tid(tid, ctx)
}

#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.rhu_api")]
pub extern "C" fn rhu_thread_resume(tid: i32) -> i32 {
    sched::resume_by_tid(tid)
}

#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.rhu_api")]
pub extern "C" fn rhu_activation_register(
    handler: Option<extern "C" fn(*mut sched::RhuCtx)>,
) -> i32 {
    let Some(h) = handler else {
        return sched::ERR_INVAL;
    };
    sched::act_register(h as usize)
}

#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.rhu_api")]
pub extern "C" fn rhu_activation_send(tid: i32) -> i32 {
    sched::act_send(tid)
}

// ---------------- module queries ----------------

#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.rhu_api")]
pub extern "C" fn rhu_module_find(addr: u32, out: *mut ModuleDesc) -> i32 {
    if out.is_null() {
        return sched::ERR_INVAL; // the futex null convention
    }
    match registry::find(addr) {
        Some((h, d)) => {
            unsafe { out.write(d) };
            h
        }
        None => sched::ERR_SRCH, // miss writes nothing
    }
}

#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.rhu_api")]
pub extern "C" fn rhu_module_roots(handle: i32, out: *mut ModuleDesc) -> i32 {
    if out.is_null() {
        return sched::ERR_INVAL;
    }
    match registry::roots(handle) {
        Some(d) => {
            unsafe { out.write(d) };
            0
        }
        None => sched::ERR_SRCH, // the enumeration walk's terminator
    }
}

// ---------------- the hosting doors: console / exit / entropy ----------------

/// The console write door's bound: one write is at most this many bytes
/// (`RHU_CONSOLE_MAX_WRITE` in the header states it to the consumer); a
/// larger request is refused, and a larger buffer is written in bounded
/// pieces by the caller. Half the console ring, so a bounded write always
/// fits once the wire has drained.
pub const CONSOLE_MAX_WRITE: u32 = 4096;

#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.rhu_api")]
pub extern "C" fn rhu_console_write(buf: *const u8, n: u32) -> i32 {
    if (buf.is_null() && n > 0) || n > CONSOLE_MAX_WRITE {
        return sched::ERR_INVAL; // the futex null convention; the bound
    }
    if n == 0 {
        return 0;
    }
    console::door_write(console::Stream::Console, unsafe { core::slice::from_raw_parts(buf, n as usize) });
    n as i32
}

#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.rhu_api")]
pub extern "C" fn rhu_console_read(buf: *mut u8, n: u32) -> i32 {
    if (buf.is_null() && n > 0) || n > i32::MAX as u32 {
        return sched::ERR_INVAL;
    }
    if n == 0 {
        return 0;
    }
    console::door_read(unsafe { core::slice::from_raw_parts_mut(buf, n as usize) }) as i32
}

/// The log doors (the console's two streams; include/rhu.h states the
/// contract): the image's output level, 1 silent | 2 quiet | 3 verbose, so
/// a hosting layer skips formatting a line it would drop; and a line of
/// the kernel's diagnostic stream written at a level -- printed when the
/// level is at or below the image's, dropped otherwise, the count answered
/// either way (the bound, the whole-buffer lock and the codes of
/// `rhu_console_write`; RHU_EINVAL for a level outside 1 to 3).
#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.rhu_api")]
pub extern "C" fn rhu_log_level() -> i32 {
    console::level() as i32
}

#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.rhu_api")]
pub extern "C" fn rhu_log_write(level: u32, buf: *const u8, n: u32) -> i32 {
    if !(console::LEVEL_SILENT..=console::LEVEL_VERBOSE).contains(&level) {
        return sched::ERR_INVAL;
    }
    if (buf.is_null() && n > 0) || n > CONSOLE_MAX_WRITE {
        return sched::ERR_INVAL;
    }
    if n == 0 {
        return 0;
    }
    if console::prints(level) {
        console::door_write(console::Stream::Log, unsafe { core::slice::from_raw_parts(buf, n as usize) });
    }
    n as i32
}

#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.rhu_api")]
pub extern "C" fn rhu_exit(code: i32) -> ! {
    semihost::end(crate::exit_word::STATE_RETURNED, code)
}

#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.rhu_api")]
pub extern "C" fn rhu_abort(code: i32) -> ! {
    semihost::end(crate::exit_word::STATE_ABORTED, code)
}

#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.rhu_api")]
pub extern "C" fn rhu_entropy(buf: *mut u8, n: u32) -> i32 {
    if buf.is_null() && n > 0 {
        return sched::ERR_INVAL;
    }
    if n == 0 {
        return 0;
    }
    entropy::fill(unsafe { core::slice::from_raw_parts_mut(buf, n as usize) });
    0
}

#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.rhu_api")]
pub extern "C" fn rhu_entropy_is_hardware() -> i32 {
    entropy::is_hardware()
}

/// The cryptographic door: bytes from a hardware noise source whose
/// health flags read clean at this draw, or an honest refusal (the
/// header states the codes); the build opt-in serves the software
/// stream here on the emulator rows' gate images. One call is bounded
/// at `SECURE_MAX_DRAW` bytes and writes nothing on a refusal.
#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.rhu_api")]
pub extern "C" fn rhu_entropy_secure(buf: *mut u8, n: u32) -> i32 {
    if (buf.is_null() && n > 0) || n > entropy::SECURE_MAX_DRAW {
        return sched::ERR_INVAL;
    }
    if n == 0 {
        return 0;
    }
    entropy::fill_secure(unsafe { core::slice::from_raw_parts_mut(buf, n as usize) })
}

// ---------------- the calendar doors ----------------

/// The calendar now -- the base plus the boot clock's elapsed, in
/// 100-nanosecond ticks since 0001-01-01T00:00:00Z -- and the source
/// that set the base (0 unset, the epoch). Both pointers 4-aligned and
/// non-null, or RHU_EINVAL writing nothing.
#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.rhu_api")]
pub extern "C" fn rhu_calendar_get(ticks: *mut u64, source: *mut i32) -> i32 {
    if ticks.is_null() || source.is_null() || (ticks as usize) & 3 != 0 || (source as usize) & 3 != 0 {
        return sched::ERR_INVAL;
    }
    let (t, s) = calendar::get();
    unsafe {
        ticks.write_unaligned(t);
        source.write(s);
    }
    0
}

/// The explicit set: the calendar reads the given instant from now on
/// (the elapsed clock never moves), the source recorded (network time
/// or the application; the kernel records its own two sources itself),
/// the real-time clock written back where the part has one. Refuses a
/// source the hosted side may not claim and an instant past the maximum.
#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.rhu_api")]
pub extern "C" fn rhu_calendar_set(ticks: u64, source: i32) -> i32 {
    calendar::set(ticks, source)
}

/// The GPIO doors (the seven verbs the
/// header states): the line space, a claim (an input with its pull, or an
/// output at its initial level; a claim on a line this program already
/// holds re-configures it), its release, a write on a claimed output, a
/// read of the pad on any claimed line, an armed event kind, and the
/// line's event word -- the futex word a waiting thread sleeps on through
/// rhu_futex_wait, incremented once per detected edge. The codes are the
/// header's; a refusal programs nothing.
#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.rhu_api")]
pub extern "C" fn rhu_gpio_count() -> i32 {
    gpio::count()
}

#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.rhu_api")]
pub extern "C" fn rhu_gpio_claim(line: u32, mode: u32, pull: u32, initial: u32) -> i32 {
    gpio::claim(line, mode, pull, initial)
}

#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.rhu_api")]
pub extern "C" fn rhu_gpio_release(line: u32) -> i32 {
    gpio::release(line)
}

#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.rhu_api")]
pub extern "C" fn rhu_gpio_write(line: u32, value: u32) -> i32 {
    gpio::write(line, value)
}

#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.rhu_api")]
pub extern "C" fn rhu_gpio_read(line: u32) -> i32 {
    gpio::read(line)
}

#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.rhu_api")]
pub extern "C" fn rhu_gpio_edge(line: u32, kind: u32) -> i32 {
    gpio::edge(line, kind)
}

#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.rhu_api")]
pub extern "C" fn rhu_gpio_event_word(line: u32) -> *const u32 {
    gpio::event_word(line)
}

/// The bus doors (the eight verbs the
/// header states): for each family the bus count, the enumeration (the
/// number of the index-th bus: the chip's own block number, the name every
/// other door takes), a claim for the program's whole run, and a transfer
/// -- one combined I2C transaction over message records, one full-duplex
/// SPI exchange under the device's own select, mode and rate with the
/// achieved rate answered. The codes are the header's; a refusal moves no
/// byte.
#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.rhu_api")]
pub extern "C" fn rhu_i2c_count() -> i32 {
    bus::i2c_count()
}

#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.rhu_api")]
pub extern "C" fn rhu_i2c_bus(index: u32) -> i32 {
    bus::i2c_bus(index)
}

#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.rhu_api")]
pub extern "C" fn rhu_i2c_claim(bus_index: u32, hz: u32) -> i32 {
    bus::i2c_claim(bus_index, hz)
}

#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.rhu_api")]
pub extern "C" fn rhu_i2c_transfer(bus_index: u32, msgs: *const crate::bus_math::I2cMsg, count: u32, deadline_ns: u64) -> i32 {
    bus::i2c_transfer(bus_index, msgs, count, deadline_ns)
}

#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.rhu_api")]
pub extern "C" fn rhu_spi_count() -> i32 {
    bus::spi_count()
}

#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.rhu_api")]
pub extern "C" fn rhu_spi_bus(index: u32) -> i32 {
    bus::spi_bus(index)
}

#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.rhu_api")]
pub extern "C" fn rhu_spi_claim(bus_index: u32) -> i32 {
    bus::spi_claim(bus_index)
}

#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.rhu_api")]
pub extern "C" fn rhu_spi_transfer(bus_index: u32, select: u32, mode: u32, hz: u32, tx: *const u8, rx: *mut u8, len: u32) -> i32 {
    bus::spi_transfer(bus_index, select, mode, hz, tx, rx, len)
}

/// The PWM doors (the eight verbs the
/// header states): the count of lines a timer can drive, the enumeration
/// (the index-th line's number), a claim of the line and its timer channel
/// for the program's run answering a handle, the period and the duty set
/// in nanoseconds with the polarity, the period the timer runs read back,
/// the output driven or parked, the release, and the timer-and-channel
/// numbering with the timer's clock. The codes are the header's; a
/// refusal programs nothing.
#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.rhu_api")]
pub extern "C" fn rhu_pwm_count() -> i32 {
    pwm::count()
}

#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.rhu_api")]
pub extern "C" fn rhu_pwm_line(index: u32) -> i32 {
    pwm::line(index)
}

#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.rhu_api")]
pub extern "C" fn rhu_pwm_claim(line: u32) -> i32 {
    pwm::claim(line)
}

#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.rhu_api")]
pub extern "C" fn rhu_pwm_set(h: i32, period_ns: u64, duty_ns: u64, polarity: u32) -> i32 {
    pwm::set(h, period_ns, duty_ns, polarity)
}

#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.rhu_api")]
pub extern "C" fn rhu_pwm_period(h: i32, achieved_ns: *mut u64) -> i32 {
    pwm::period(h, achieved_ns)
}

#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.rhu_api")]
pub extern "C" fn rhu_pwm_enable(h: i32, on: u32) -> i32 {
    pwm::enable(h, on)
}

#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.rhu_api")]
pub extern "C" fn rhu_pwm_release(h: i32) -> i32 {
    pwm::release(h)
}

#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.rhu_api")]
pub extern "C" fn rhu_pwm_info(h: i32, timer: *mut u32, channel: *mut u32, clock_hz: *mut u32) -> i32 {
    pwm::info(h, timer, channel, clock_hz)
}

/// The ADC doors (the six verbs the
/// header states): the count of channels across the converter blocks, the
/// enumeration (the index-th channel's block, number, line and width), a
/// claim of the channel and its line answering a handle, one conversion
/// answering the raw sample, a block's reference voltage, and the release.
/// The codes are the header's; a refusal converts nothing.
#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.rhu_api")]
pub extern "C" fn rhu_adc_count() -> i32 {
    adc::count()
}

#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.rhu_api")]
pub extern "C" fn rhu_adc_channel(index: u32, block: *mut u32, channel: *mut u32, line: *mut u32, bits: *mut u32) -> i32 {
    adc::channel(index, block, channel, line, bits)
}

#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.rhu_api")]
pub extern "C" fn rhu_adc_claim(index: u32) -> i32 {
    adc::claim(index)
}

#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.rhu_api")]
pub extern "C" fn rhu_adc_read(h: i32) -> i32 {
    adc::read(h)
}

#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.rhu_api")]
pub extern "C" fn rhu_adc_reference(block: u32, millivolts: *mut u32) -> i32 {
    adc::reference(block, millivolts)
}

#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.rhu_api")]
pub extern "C" fn rhu_adc_release(h: i32) -> i32 {
    adc::release(h)
}

/// The UART doors (the twelve verbs the header states): the count of
/// UART rows, the enumeration (the index-th row's block, lines and clock), a
/// claim of a block for the program's run answering a handle (the console's
/// block and a held block refused), the line discipline live with the
/// achieved rate answered, a never-blocking write and read over the port's
/// rings, the three counts, the event word, the flush, the break, the
/// release and the held port's facts. The codes are the header's; a refusal
/// moves no byte.
#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.rhu_api")]
pub extern "C" fn rhu_uart_count() -> i32 {
    uart::count()
}

#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.rhu_api")]
pub extern "C" fn rhu_uart_port(index: u32, block: *mut u32, tx_line: *mut u32, rx_line: *mut u32, clock_hz: *mut u32) -> i32 {
    uart::port(index, block, tx_line, rx_line, clock_hz)
}

#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.rhu_api")]
pub extern "C" fn rhu_uart_claim(block: u32) -> i32 {
    uart::claim(block)
}

#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.rhu_api")]
pub extern "C" fn rhu_uart_configure(h: i32, baud: u32, data_bits: u32, parity: u32, stop_bits: u32, achieved_baud: *mut u32) -> i32 {
    uart::configure(h, baud, data_bits, parity, stop_bits, achieved_baud)
}

#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.rhu_api")]
pub extern "C" fn rhu_uart_write(h: i32, buf: *const u8, n: u32) -> i32 {
    uart::write(h, buf, n)
}

#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.rhu_api")]
pub extern "C" fn rhu_uart_read(h: i32, buf: *mut u8, n: u32) -> i32 {
    uart::read(h, buf, n)
}

#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.rhu_api")]
pub extern "C" fn rhu_uart_status(h: i32, rx_avail: *mut u32, tx_pending: *mut u32, tx_room: *mut u32) -> i32 {
    uart::status(h, rx_avail, tx_pending, tx_room)
}

#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.rhu_api")]
pub extern "C" fn rhu_uart_event_word(h: i32) -> *const u32 {
    uart::event_word(h)
}

#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.rhu_api")]
pub extern "C" fn rhu_uart_flush(h: i32, which: u32) -> i32 {
    uart::flush(h, which)
}

#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.rhu_api")]
pub extern "C" fn rhu_uart_break(h: i32, on: u32) -> i32 {
    uart::brk(h, on)
}

#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.rhu_api")]
pub extern "C" fn rhu_uart_release(h: i32) -> i32 {
    uart::release(h)
}

#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.rhu_api")]
pub extern "C" fn rhu_uart_info(h: i32, block: *mut u32, tx_line: *mut u32, rx_line: *mut u32, clock_hz: *mut u32) -> i32 {
    uart::info(h, block, tx_line, rx_line, clock_hz)
}

// ---------------- the fault funnel's arming door ----------------

/// Arm the delivery of the fault kinds named by bits (bit 0 = a null data
/// access): sticky for the process's life; 0, or RHU_EINVAL for an unknown
/// bit. Until a hosting layer calls it, a null access is the funnel's
/// designed death (include/rhu.h states the contract).
#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.rhu_api")]
pub extern "C" fn rhu_fault_register(kinds: u32) -> i32 {
    sched::fault_register(kinds)
}

/// The RAM-range doors (the three verbs the header
/// states): the count of ranges this image serves, the index-th range's
/// record (the physical range as the board's facts state it, the arena
/// window inside it the runtime may be handed, that window's headroom, the
/// flags), and an allocation drawn from a named range -- range 0 exactly
/// `rhu_arena_alloc_at`, which allocates from the internal
/// SRAM. The codes are the header's; a refusal writes nothing.
#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.rhu_api")]
pub extern "C" fn rhu_ram_count() -> i32 {
    ram::count()
}

#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.rhu_api")]
pub extern "C" fn rhu_ram_range(index: u32, out: *mut ram::RamRange) -> i32 {
    ram::range(index, out)
}

#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.rhu_api")]
pub extern "C" fn rhu_arena_alloc_from(range: u32, bytes: u32, align: u32, site: u32) -> *mut u8 {
    arena::alloc_from(range, bytes, align, site)
}

/// The block-device doors (the five verbs the header
/// states): the count of block devices this image
/// serves, the index-th device's record read at the call, whole-block reads
/// and writes by block number, the sync. The codes are the header's; a
/// refusal moves no byte.
#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.rhu_api")]
pub extern "C" fn rhu_blk_count() -> i32 {
    blk::count()
}

#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.rhu_api")]
pub extern "C" fn rhu_blk_device(index: u32, out: *mut blk::BlkDesc) -> i32 {
    blk::device(index, out)
}

#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.rhu_api")]
pub extern "C" fn rhu_blk_read(dev: u32, lba: u32, blocks: u32, buf: *mut c_void) -> i32 {
    blk::read(dev, lba, blocks, buf as *mut u8)
}

#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.rhu_api")]
pub extern "C" fn rhu_blk_write(dev: u32, lba: u32, blocks: u32, buf: *const c_void) -> i32 {
    blk::write(dev, lba, blocks, buf as *const u8)
}

#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.rhu_api")]
pub extern "C" fn rhu_blk_sync(dev: u32) -> i32 {
    blk::sync(dev)
}

/// The network doors (the twenty-six verbs the header states, the
/// `rhu_net_` family with the Linux socket verbs): the media and the address
/// (the count, a medium's facts and word, the configuration, the lease, the
/// row's limits), name resolution (a request with a handle, never a wait),
/// and the sockets (open, bind, listen, accept, connect, send, receive, the
/// queued count, shutdown, close, the readiness mask with its risen bits, the
/// pending error, the event words, the self-wake, the names, the options).
/// No door blocks; the codes are the header's; a refusal moves no byte.
#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.rhu_api")]
pub extern "C" fn rhu_net_count() -> i32 {
    net::count()
}

#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.rhu_api")]
pub extern "C" fn rhu_net_medium(medium: u32, out: *mut net::MediumInfo) -> i32 {
    net::medium_info(medium, out)
}

#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.rhu_api")]
pub extern "C" fn rhu_net_medium_word(medium: u32) -> *const u32 {
    net::medium_word(medium)
}

#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.rhu_api")]
pub extern "C" fn rhu_net_configure(medium: u32, fixed: *const net::Lease) -> i32 {
    net::configure(medium, fixed)
}

#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.rhu_api")]
pub extern "C" fn rhu_net_lease(medium: u32, out: *mut net::Lease) -> i32 {
    net::lease(medium, out)
}

#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.rhu_api")]
pub extern "C" fn rhu_net_limits(out: *mut net::Limits) -> i32 {
    net::limits(out)
}

#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.rhu_api")]
pub extern "C" fn rhu_net_resolve(name: *const u8, len: u32) -> i32 {
    net::resolve(name, len)
}

#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.rhu_api")]
pub extern "C" fn rhu_net_resolve_result(h: i32, out: *mut net::Addr, max: u32) -> i32 {
    net::resolve_result(h, out, max)
}

#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.rhu_api")]
pub extern "C" fn rhu_net_resolve_word(h: i32) -> *const u32 {
    net::resolve_word(h)
}

#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.rhu_api")]
pub extern "C" fn rhu_net_socket(family: u32, kind: u32, protocol: u32) -> i32 {
    net::socket(family, kind, protocol)
}

#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.rhu_api")]
pub extern "C" fn rhu_net_bind(h: i32, local: *const net::Addr) -> i32 {
    net::bind(h, local)
}

#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.rhu_api")]
pub extern "C" fn rhu_net_listen(h: i32, backlog: u32) -> i32 {
    net::listen(h, backlog)
}

#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.rhu_api")]
pub extern "C" fn rhu_net_accept(h: i32, peer: *mut net::Addr) -> i32 {
    net::accept(h, peer)
}

#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.rhu_api")]
pub extern "C" fn rhu_net_connect(h: i32, peer: *const net::Addr) -> i32 {
    net::connect(h, peer)
}

#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.rhu_api")]
pub extern "C" fn rhu_net_send(h: i32, buf: *const u8, n: u32, to: *const net::Addr) -> i32 {
    net::send(h, buf, n, to)
}

#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.rhu_api")]
pub extern "C" fn rhu_net_recv(h: i32, buf: *mut u8, n: u32, flags: u32, from: *mut net::Addr) -> i32 {
    net::recv(h, buf, n, flags, from)
}

#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.rhu_api")]
pub extern "C" fn rhu_net_available(h: i32) -> i32 {
    net::available(h)
}

#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.rhu_api")]
pub extern "C" fn rhu_net_shutdown(h: i32, how: u32) -> i32 {
    net::shutdown(h, how)
}

#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.rhu_api")]
pub extern "C" fn rhu_net_close(h: i32) -> i32 {
    net::close(h)
}

#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.rhu_api")]
pub extern "C" fn rhu_net_readiness(h: i32, take: u32) -> i32 {
    net::readiness(h, take)
}

#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.rhu_api")]
pub extern "C" fn rhu_net_error(h: i32) -> i32 {
    net::error(h)
}

#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.rhu_api")]
pub extern "C" fn rhu_net_event_word(h: i32) -> *const u32 {
    net::event_word(h)
}

#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.rhu_api")]
pub extern "C" fn rhu_net_events_word() -> *const u32 {
    net::events_word()
}

#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.rhu_api")]
pub extern "C" fn rhu_net_events_bump() {
    net::events_bump()
}

#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.rhu_api")]
pub extern "C" fn rhu_net_name(h: i32, local: *mut net::Addr, peer: *mut net::Addr) -> i32 {
    net::name(h, local, peer)
}

#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.rhu_api")]
pub extern "C" fn rhu_net_option(h: i32, option: u32, set: u32, value: i32) -> i32 {
    net::option(h, option, set, value)
}

/// The hosting-API contract generation this kernel serves: `RHU_ABI`
/// in `include/rhu.h`, the two moving together or not at all. A
/// payload that provides its own `rhu_payload_entry` states the
/// generation it was built against in `rhu_payload_abi`, and the
/// payload flavor refuses a mismatch before that entry runs
/// (payload.rs).
pub const ABI: u32 = 41;
/// The window's floor: `RHU_ABI_COMPAT` in `include/rhu.h`, the two
/// moving together or not at all. A payload built against any
/// generation from this one through `ABI` runs; only a breaking change
/// raises it (to the new `ABI`). The judge is `abi_window`.
pub const ABI_COMPAT: u32 = 31;
const _: () = assert!(ABI_COMPAT <= ABI);
