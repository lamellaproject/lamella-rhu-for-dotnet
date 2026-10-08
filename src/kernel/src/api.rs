//! The RHU hosting API -- the C-ABI waistline between the kernel and
//! hosted payloads. Sixty-three functions, `rhu_`-prefixed, mirrored
//! exactly by `include/rhu.h` (contract generation `ABI`, at the end
//! of this file); the shim backend is the intended consumer. One rule
//! binds this surface: no second blocking primitive ever joins it, and
//! new needs are shim recipes over the blocking four (join, for one,
//! is a futex recipe on a thread-exit word -- never a verb here).

use core::ffi::{c_char, c_void};

use crate::registry::ModuleDesc;
use crate::{adc, arena, bus, calendar, console, entropy, gpio, kheap, pwm, registry, sched, semihost};

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
#[allow(dead_code)] pub const HOSTED_PRIO_FLAT: u32 = 0x80;

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
    let entry: extern "C" fn(usize) = unsafe { core::mem::transmute(entry) };
    if (stack_bytes as usize) < sched::THREAD_STACK_MIN || stack_bytes > u32::MAX - 7 {
        return sched::ERR_INVAL;
    }
    let prio = prio.clamp(HOSTED_PRIO_LO, HOSTED_PRIO_HI) as u8;
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
        return sched::ERR_INVAL;
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


#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.rhu_api")]
pub extern "C" fn rhu_thread_suspend(tid: i32, out_ctx: *mut sched::RhuCtx) -> i32 {
    if out_ctx.is_null() || (out_ctx as usize) & 3 != 0 {
        return sched::ERR_INVAL;
    }
    sched::suspend_by_tid(tid, out_ctx)
}

#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.rhu_api")]
pub extern "C" fn rhu_thread_set_context(tid: i32, ctx: *const sched::RhuCtx) -> i32 {
    if ctx.is_null() || (ctx as usize) & 3 != 0 {
        return sched::ERR_INVAL;
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


#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.rhu_api")]
pub extern "C" fn rhu_module_find(addr: u32, out: *mut ModuleDesc) -> i32 {
    if out.is_null() {
        return sched::ERR_INVAL;
    }
    match registry::find(addr) {
        Some((h, d)) => {
            unsafe { out.write(d) };
            h
        }
        None => sched::ERR_SRCH,
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
        None => sched::ERR_SRCH,
    }
}


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
        return sched::ERR_INVAL;
    }
    if n == 0 {
        return 0;
    }
    console::door_write(unsafe { core::slice::from_raw_parts(buf, n as usize) });
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

#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.rhu_api")]
pub extern "C" fn rhu_exit(code: i32) -> ! {
    semihost::exit(code as u32)
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

/// The GPIO doors (the device seam's first peripheral; the seven verbs the
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

/// The bus doors (the device seam's second peripheral; the eight verbs the
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

/// The PWM doors (the device seam's third peripheral; the eight verbs the
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

/// The ADC doors (the device seam's fourth peripheral; the six verbs the
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

/// The hosting-API contract generation this kernel serves: `RHU_ABI`
/// in `include/rhu.h`, the two moving together or not at all. A
/// payload that provides its own `rhu_payload_entry` states the
/// generation it was built against in `rhu_payload_abi`, and the
/// payload flavor refuses a mismatch before that entry runs
/// (payload.rs).
pub const ABI: u32 = 35;
/// The window's floor: `RHU_ABI_COMPAT` in `include/rhu.h`, the two
/// moving together or not at all. A payload built against any
/// generation from this one through `ABI` runs; only a breaking change
/// raises it (to the new `ABI`). The judge is `abi_window`.
pub const ABI_COMPAT: u32 = 31;
const _: () = assert!(ABI_COMPAT <= ABI);
