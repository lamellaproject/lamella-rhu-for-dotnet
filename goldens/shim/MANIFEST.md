# goldens/shim -- the shim gate's frozen world objects and goldens (MANIFEST)

Copyright (c) 2026 Lamella LLC.
SPDX-License-Identifier: MIT OR Apache-2.0

The shim gate's images link from the objects listed below, and its positive
cases compare the test program's payload stream (its output with the
kernel's own lines removed) against the goldens listed below. Every object
is the bytes of one recorded compile; every golden is the payload stream of
one recorded run, with its wire bytes kept. Before its first case the gate
checks every file here against this manifest, and this manifest against the
digest kept in the harness, and refuses a mismatch: a frozen file that
differs from its record is wrong, and the gate never rebuilds it. Nothing
here is compiled at gate time; the gate needs Rust and QEMU alone.

The leg `rhu-gate shim rederive` recompiles every object below from src/shim
as C under the gate's flag set (CC_FLAGS in src/harness/src/gates/shim.rs)
plus the defines its record names, with the compiler named below against
the Arm GNU Toolchain 12.2.MPACBTI-Rel1 sysroot, and compares each result
with the file byte for byte; the README's re-derivation section names what
the leg needs.

Compiler: clang version 22.1.8 (the LLVM project's 22.1.8 release, the
version the runtime archives are built with), targeting arm-none-eabi for
the Cortex-M7 with the hard-float FPv5-D16, and for the mps2-an505 row's
objects and the RP2350 rows' core and system-call objects the Cortex-M33
with the single-precision FPv5 under the same flag set but the core and FPU
words (the class flag set beside the Cortex-M7's in shim.rs). The
toolchain's C-library headers are the sysroot. The objects are compiled as
C (-x c -std=gnu11) with no implicit floating-point use
(-mno-implicit-float): the activation compatibility adapter's object
carries no floating-point instruction, and the gate pins every object's
count.

The objects are compiled against the .NET 11 RC2 runtime pack
(v11.0.0-rc.2.26504.105): the contract headers carry the .NET major and
the pin generation in their names (rhu_pal_<area>_net11_rc2.h,
rhu_rt_entry_net11_rc2.h) and are generated from that pack's source. The
shim answers two names the runtime archives demand of the C library,
getauxval (no auxiliary vector: 0 with ENOENT) and realpath (over the RAM
file store), and the test program checks both.

The RAM file store keeps a stated contract: a maximum file size with every
position, count and length checked against it in 64 bits; an object's name
and its open descriptors kept apart; one store lock; a descriptor found
before anything is created or truncated; the access mode checked at the
read boundary; advisory locks per object; paths with a trailing slash
named the same; file times range-checked; a refused open of the process
identity line giving back the object it materialized. Each clause is a
check in the test program's file ladder, with a negative control built
from the store with that clause's defect restored.

The random entries draw cryptographic bytes through the kernel's secure
entropy call in bounded pieces, all or nothing, with a refusal honored.
Every wall-clock answer (the system time, the C library's real-time clock
and time(), the file store's times) reads the kernel's calendar. Each of
these has a negative control built from the shim with that answer's defect
restored.

The timed waits honor the clock a timestamp names: a boot-clock timestamp
is the kernel deadline itself; a real-time (calendar) timestamp is
translated into the boot-clock domain with every step checked, and re-read
against the calendar every slice while the wait is pending, so a calendar
set moves the wait. A negative control is built from the shim with the
clock discarded.

The 64-bit atomic operations the runtime's compiler names as library calls
(the runtime archives are built for the core, which has no 64-bit
exclusive-access instruction) are answered under one lock of the futex
family with a full barrier on each side, so every such operation is atomic
against every other and no interrupt is masked.

The libgpiod v1 C API the .NET GPIO library's binding names (twenty-two
entry points, with the binding's own signatures) is presented over the
kernel's GPIO calls in a unit of its own, bound at link time by the SDK's
direct P/Invoke of the library's name, so the library's stock driver runs
unchanged. The process identity line that driver reads is answered by the
store as a synthesized unlinked object released at its last close. One
negative control is built from the unit, with the API generation stated
below the bias flags; the hosted-runtime gates' suite refuses it in every
world, and the control's Cortex-M33 twin stands beside it. The unit's
event wait takes the line's word once and waits again when the kernel's
futex wait reports an activation, as the kernel call's contract says. The
errno calls carry the raw platform errno the PAL's contract names, and the
managed side converts it.

The two device-node families the .NET I2C and SPI drivers open,
/dev/i2c-<bus> and /dev/spidev<bus>.<select> (the node's number is the
chip's block number as Linux names it), are presented over the kernel's
bus calls in a unit of its own; the core's open, ioctl and close dispatch
to it, bound at link time by the SDK's direct P/Invoke of the three
C-library entry points, so the drivers run unchanged. The functionality
request is answered as plain combined transactions; the combined-transfer
request is carried into one kernel transaction with a one-second deadline;
the I2C bus is claimed at the first transfer at 100 kHz, or earlier at a
served rate by this system's one rate request on the node (0x0740, a word
in hertz read back with the rate the bus runs at; an unserved rate is
refused ENOTSUP before the bus's state, a second rate EBUSY with the
running rate read back -- the one departure from the Linux surface, which
has no clock setting); the SPI mode, word-size and rate requests are kept
per node and carried into every transfer, with the achieved rate stated
once per change; every other path and request is refused with its errno
and one line. One negative control is built from the unit, whose
functionality answer names no combined transfer; the hosted-runtime
gates' suite refuses it in every world, and the control's Cortex-M33 twin
stands beside it.

Both units keep a stated contract under concurrent callers. The bus unit
reads a caller's memory once (a receive buffer lying over its own record
changes nothing), stores the functionality word's four bytes and not one
more, gives a node's entry to the requests running on it (a close under a
running request answers at once, and the entry goes to no one until that
request ends), refuses the select 4294967295 as a node's name (the number
is the kernel's no-select value) and honors a lock that fails. The GPIO
unit makes each open its own chip object with one holder per line across
them, keeps its chips, lines and iterators under one lock with every
handle checked by address before it is read, refuses two bias flags
together, reads the living request's bias and leaves a pad's pull as it
is under a flagless request, refuses a timespec field outside its range,
reads the mark before the arm so every edge counted from the arm on is
delivered, reads the count and the pad's level together so an event's kind
is the counted edge's, and its event read waits for an edge as the
library's does.

The PWM device files the .NET PWM class reads and writes --
/sys/class/pwm/pwmchip0 with its npwm, export and unexport, and each
exported channel's period, duty_cycle, polarity and enable -- are
presented over the kernel's eight PWM calls in a unit of its own; the
store's path and descriptor verbs and the C library's access dispatch to
it (a path under /sys, a descriptor of the sysfs band), so the class's
stock driver runs unchanged. The channel number is the line number (every
pin a program names is reachable, and the kernel's claims keep a timer
channel exclusive), and the period file reads the period the timer runs,
the one departure from Linux a program can see. Four negative controls are
built from the unit: the period file answering the value written (which
the hosted-runtime gates' suite refuses in every world, with the control's
Cortex-M33 twin beside it), export answering 0 for a hole, unexport
releasing nothing, and npwm as the door's count.

The analog input device files the board package's analog input class
reads -- /sys/bus/iio/devices with one directory iio:device<k> per
converter block the kernel's table states, each with its name, its
in_voltage_scale and one in_voltage<N>_raw file per channel -- are
presented over the kernel's six ADC calls in a unit of its own; the
store's path and descriptor verbs and the C library's access dispatch to
it (a path under /sys/bus, a descriptor of the analog band), so a program
that reads a converter through those files reads it unchanged. The open
of a channel's raw file claims the channel for the descriptor's life and
the close releases it, the one departure from Linux a program can see; a
read at offset 0 runs one conversion and answers the sample as text, and
the scale file answers the millivolts one count stands for. Four negative
controls are built from the unit: a read answering the last conversion's
text without a new conversion (which the hosted-runtime gates' suite
refuses in every world, with the control's Cortex-M33 twin beside it),
the close releasing nothing, the scale computed for one bit less than the
width, and the open of a channel the kernel refused as held.

The serial device files /dev/ttyS<n> (and the alias /dev/ttyAMA<n>) the
serial port class's native layer opens are presented over the kernel's
twelve UART calls in a unit of its own; the core's open, ioctl and close
and the glue's read and write dispatch to it. The layer's other C-library
names (poll, fcntl, shutdown, the termios family, the error string) are
defined there, so no member of the toolchain's C library is pulled. The
layer itself is compiled unchanged from the vendored sources recorded in
src/VENDORED.md into an object of its own per class and bound at link time
by the SDK's direct P/Invoke of the library's name. Four negative controls
are built from the unit: an empty non-blocking read answering 0, poll
reporting an error on a live port, the attributes read answering the
claim's state and not the image set, and close keeping the port.

The kernel's null-page fault reaches the runtime's SIGSEGV handler as the
Linux record (SEGV_MAPERR, the faulting address, the registers) once the
runtime has recorded that handler; the core arms the kernel's delivery at
the recording. A declined fault or an unrecorded disposition ends the
process at 139, and an upward stack-pointer edit is honoured through the
core's trampoline. Two negative controls are built from the core (the
arming never done; the address reported one past the guarded page); the
hosted-runtime gates' suite refuses them in every world, with the
controls' Cortex-M33 twins beside them. The core adopts a zero-filled
mutex or condition slot by one compare-and-swap, so two first users at
once are both served.

Each unit's ladder runs in this gate from a native caller's side, on the
emulator row's bus models and virtual lines, with one negative control per
restored defect (the core's one, the bus unit's five, the GPIO
unit's eight, the PWM unit's four, the analog input unit's four, the serial
unit's four). The four
controls whose defect is a race hold the restored window open, blocked on
the kernel's futex, until the second thread's step is seen (that thread
runs one step below the ladder's, so it cannot step before the window is
open), so each is refused at a line of its own on every run on any host.
Every ladder line is one console write, so no verdict line can be torn.
The core reads the program's thread stack size as a weak link-time symbol,
the row's default when absent.

The core's own lines and the host's go out through the kernel's
log door at the verbose level, and the last word that ends the program
at the silent level, so a quiet or silent image prints none of them. The
host object states hosting-API
generation 41, the number the kernel's header states for its hosting
API, which a hosted image must match.

The mps2-an505 row's link objects are the an500 row's sources and
defines compiled for the Cortex-M33 (the row's fields equal the an500's,
so the class flag set is the one variable), each carrying no
double-precision instruction.

The core reads the managed heap's window once at initialization -- the
external RAM's window when the link map placed the runtime's rehydrated
type data inside it, the internal SRAM's otherwise -- and writes the
runtime's hard-limit row from that window's free memory then, less what
the board row keeps for later draws (the frozen object heap and the
system memory left unallocated) when the window is the internal one;
the collector's region, mark bitmap and mark stack are served from the
same window through the core's own entry, every other mapping from the
internal SRAM, and the C library's page counts are that window's. The
core's environment table also carries the thread pool's idle-worker
timeout row, 100 ms by default and the image's link word when the
project's property defined one. The
system-call unit serves the runtime's frozen-object reservation from the
row's pool, accepts a commit that starts inside the pool, refuses one that
starts at or past its end, and answers the C library's question of what
memory backs a mapping with the pool's pages and none of the reservation
past them, so the collector stops a program before a byte is written past
a pool smaller than the runtime's commit. The frozen object heap's ladder
runs in this gate on the small row's objects, with one negative control
built from the system-call unit whose backing answer says every page is
memory.

The two objects a differing row field reaches are frozen per row class:
the st-stm32f769i-disco row's core and system-call objects for the
Cortex-M7, and for the Cortex-M33 one pair for the four RP2350 rows (the
Raspberry Pi Pico 2 and Pico 2 W, the Pimoroni Pico Plus 2 and Pico Plus
2 W declare one mini row each with the same fields), frozen under the
Pico 2's selector and named for the chip. The st-nucleo-u5a5zj-q row keeps
what the mps2-an505 row keeps and names that row's objects throughout. The
rederive leg ties every board-table row's objects to the row's own
declaration by bytes.

The shim answers the runtime pack's whole P/Invoke-facing native surface
(the five class-library families' export tables, 757 names): the names the
hand-written units serve over the RAM file store, the kernel's calendar
and its secure entropy call, and for every name of an area this system
does not serve a generated refusal (nothing printed, nothing trapped; the
errno of record where the convention is errno-style) in the surface object
beside the row's link objects, with its Cortex-M33 twin for the an505 row.
The expectation the gate's surface case reads, generated from the pack's
archives and the runtime's headers, is the tier's text record, tied to the
pack and to every family name the objects define.

97 objects, 11 goldens and 1 record.

Records (repo-relative path, sha256; source and defines per object):

- `goldens/shim/shim-core.o`
  `867e5498062c7cf1d150d59f0ed96b5d3f779cadfcfded175f280f20ecf86259`
  (src/shim/rhu_shim.c; -DMGC_HOST_ROW_AN500)
- `goldens/shim/shim-core-break-sipid.o`
  `c7cd36733f38ed9fb102b22fd6127ffd8f07715f443bb9c1dcf7ea42d79e53fc`
  (src/shim/rhu_shim.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_SIPID)
- `goldens/shim/shim-core-break-ctxpc.o`
  `cdb4a2aac534c96353189982eac099f623a645890fef10fc17bb93dcb96bbc2c`
  (src/shim/rhu_shim.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_CTXPC)
- `goldens/shim/shim-core-break-tlsoff.o`
  `1c6242ceb7bbc9d638347ff35be794c4d13ee6c6133f5813ff95f40c8f22663a`
  (src/shim/rhu_shim.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_TLSOFF)
- `goldens/shim/shim-core-break-tlscache.o`
  `3b700b7de2e2df157a09aa091669d63ac64e1c0fb157d715ba7a574bc6540355`
  (src/shim/rhu_shim.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_TLSCACHE)
- `goldens/shim/shim-core-break-realtime-wait.o`
  `be6a816dc730b97b0defb108f03bc1df616841948bb618e2b4485b495a2e9aba`
  (src/shim/rhu_shim.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_REALTIME_WAIT)
- `goldens/shim/shim-sysnative.o`
  `6a6791fa7a80ef0c0721de3b93037800e79db02b4b8fe6642ddeebc3d27ebf05`
  (src/shim/rhu_sysnative.c; -DMGC_HOST_ROW_AN500)
- `goldens/shim/shim-sysnative-break-fdbase.o`
  `77c6e0955edf9ab4e6d1c3184e919b1b5a5aa74cee873a86d831fcadb8725f8e`
  (src/shim/rhu_sysnative.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_FD_BASE)
- `goldens/shim/shim-sysnative-break-store-bounds.o`
  `da0faa7d319b10bfc017bf6b3347c2dc5aa86e28f31bf0915db80b2be1145d5f`
  (src/shim/rhu_sysnative.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_STORE_BOUNDS)
- `goldens/shim/shim-sysnative-break-store-lifetime.o`
  `f6768a0f9089721869a7d9ffca9c83e2a3d301e125f796a50f858464ba38ff92`
  (src/shim/rhu_sysnative.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_STORE_LIFETIME)
- `goldens/shim/shim-sysnative-break-store-lock.o`
  `588ba9c66967d71d5f7a96d43860be5747005c171b0a0a143bdb331d8628c43f`
  (src/shim/rhu_sysnative.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_STORE_LOCK)
- `goldens/shim/shim-sysnative-break-store-open-order.o`
  `78fd85b6887b21520bda9faaeb6e3379c891a57c90429f1b293ee892ac3e3ab1`
  (src/shim/rhu_sysnative.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_STORE_OPEN_ORDER)
- `goldens/shim/shim-sysnative-break-store-access.o`
  `558da126c6ddbe0e8c325589a342b916a21d8ade8aaf011004608e2e3b6ddf0e`
  (src/shim/rhu_sysnative.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_STORE_ACCESS)
- `goldens/shim/shim-sysnative-break-store-flock.o`
  `bd9f3e7d355ae99eb71f29046a9874361d0a14d75be3379c5168f246c78ac978`
  (src/shim/rhu_sysnative.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_STORE_FLOCK)
- `goldens/shim/shim-sysnative-break-store-slash.o`
  `5c6ce75f66010fba8c7f20c1b6b492c14e845252a7cf80e674ad6c967de8a0e9`
  (src/shim/rhu_sysnative.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_STORE_SLASH)
- `goldens/shim/shim-sysnative-break-store-times.o`
  `0e23aa5f34c46e4496aecf5048909485dbd077e584f0ffb506ee652e71784b27`
  (src/shim/rhu_sysnative.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_STORE_TIMES)
- `goldens/shim/shim-sysnative-break-entropy-refusal.o`
  `2273449053479fd5bfe42c4a5f47f4282a11ca0152b7d13aa35ef3239cd10026`
  (src/shim/rhu_sysnative.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_ENTROPY_REFUSAL)
- `goldens/shim/shim-sysnative-break-calendar-clock.o`
  `2e17b56ba314111b620042a6e2f6b58c75f4017087d9308708302c4d8e836386`
  (src/shim/rhu_sysnative.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_CALENDAR_CLOCK)
- `goldens/shim/shim-libc-glue.o`
  `feffa0977cc6ac71e97c7b203f4c6c7640ef951ebc688c7fcefb77bbdf076f49`
  (src/shim/rhu_libc_glue.c; no extra defines)
- `goldens/shim/shim-host.o`
  `93dc8184a1c1c248c1c1a16106b28634f7f5b8d14fbdf2f4b07b91df77bdf2fd`
  (src/shim/rhu_host.c; -DMGC_HOST_ROW_AN500)
- `goldens/shim/shim-host-break-dataverify.o`
  `00635534e9a84c805374744d1d9b7db95b623b9a7c11d26403d1daa99c46e4b1`
  (src/shim/rhu_host.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_DATAVERIFY)
- `goldens/shim/shim-stdio-sym.o`
  `8428ce33a5f364f458b0ccaa949e3398470a9b05500810c6a62109731033a28d`
  (src/shim/rhu_stdio_sym.c; no extra defines)
- `goldens/shim/shimtest-core.o`
  `7cf0aee3567d68b22199b3b767c1f24446e5329146325790024edffba26c9f90`
  (src/shim/gate/rhu_shimtest.c; -DRHU_SHIMTEST_LADDER_CORE)
- `goldens/shim/shimtest-core-break-errno.o`
  `ce270f81fee39969728c7dcdd83bf1c8df90ebf200202820b5b1e2b7000a3f0e`
  (src/shim/gate/rhu_shimtest.c; -DRHU_SHIMTEST_LADDER_CORE -DRHU_SHIMTEST_BREAK_ERRNO)
- `goldens/shim/shimtest-core-break-canary.o`
  `27a6f2624e73acfbf8e10689eb9befd992ce9fb5f143a639258f3549c52b70e9`
  (src/shim/gate/rhu_shimtest.c; -DRHU_SHIMTEST_LADDER_CORE -DRHU_SHIMTEST_BREAK_CANARY)
- `goldens/shim/shimtest-fdmap.o`
  `2c19a1aeb8116aec48b9831ec9dd8b72294f5b2d74bffa185910d08ed1dd26ef`
  (src/shim/gate/rhu_shimtest.c; -DRHU_SHIMTEST_LADDER_FDMAP)
- `goldens/shim/shimtest-activation.o`
  `68406c4fff9eb9b22a0768c756e0e074b62a0819162ed909584a8e4f73716b9c`
  (src/shim/gate/rhu_shimtest.c; -DRHU_SHIMTEST_LADDER_ACTIVATION)
- `goldens/shim/shimtest-tls.o`
  `10a30bc83fc94b99ae70fda477719731fc61abc6dec2fb665e5b1f5133bc1786`
  (src/shim/gate/rhu_shimtest.c; -DRHU_SHIMTEST_LADDER_TLS)
- `goldens/shim/shimtest-host.o`
  `54ac07f888cf89cf8d086fe16e9d7a8482d3aa68c5b090aa2732f75e0c6d2722`
  (src/shim/gate/rhu_shimtest.c; -DRHU_SHIMTEST_LADDER_HOST)
- `goldens/shim/shim-core-f769.o`
  `7ebe98062ccb5a1659a4e2fba0b7040ef11e2278e1124694fede2dede7776571`
  (src/shim/rhu_shim.c; -DMGC_HOST_ROW_F769)
- `goldens/shim/shim-core-an505.o`
  `0c4d1d32c9e3dd98667d8f1ce458d2f7a563c1c8247daa461c22bf6a4a15cc04`
  (src/shim/rhu_shim.c; -DMGC_HOST_ROW_AN505; the Cortex-M33 class flag set)
- `goldens/shim/shim-sysnative-an505.o`
  `be6472965f85a7bb04e0d165807c3e1da82865a1e3ae596000c8261e0cc3db04`
  (src/shim/rhu_sysnative.c; -DMGC_HOST_ROW_AN505; the Cortex-M33 class flag set)
- `goldens/shim/shim-libc-glue-an505.o`
  `45d2179c03034d89fcf2e1633e48bf21a56a5255561697ed1a1021f2e27a5618`
  (src/shim/rhu_libc_glue.c; no extra defines; the Cortex-M33 class flag set)
- `goldens/shim/shim-host-an505.o`
  `3935bc26e74c54a9cfb64c4070115d5b59a7c2131e51385e01bf87c9abcb68b0`
  (src/shim/rhu_host.c; -DMGC_HOST_ROW_AN505; the Cortex-M33 class flag set)
- `goldens/shim/shim-stdio-sym-an505.o`
  `49a42ffaf600fe1315997337de4e9457f6454072c823f8ce3be0d7e4abfb3bb9`
  (src/shim/rhu_stdio_sym.c; no extra defines; the Cortex-M33 class flag set)
- `goldens/shim/shim-core-rp2350.o`
  `afea5c2333a16c4fee200c74e492942951fba3b1f16e683056ad24e0e278815c`
  (src/shim/rhu_shim.c; -DMGC_HOST_ROW_PICO2; the Cortex-M33 class flag set)
- `goldens/shim/shim-pal-surface.o`
  `4ffcfe7b757028e7c13c6755cd169e34e13e65c0b9c1ca9c3707e3392f223316`
  (src/shim/rhu_pal_surface_net11_rc2.c; -DMGC_HOST_ROW_AN500)
- `goldens/shim/shim-pal-surface-an505.o`
  `8c4b2905bdb6bfd663337c380065b565d398738dc037afafaad9f16b2e51bc61`
  (src/shim/rhu_pal_surface_net11_rc2.c; -DMGC_HOST_ROW_AN505; the Cortex-M33 class flag set)
- `goldens/shim/shim-gpiod.o`
  `c89651bbba2231d39154f975cc984f14f3dab928ab57c8b9c72d45e69ef3470c`
  (src/shim/rhu_gpiod.c; -DMGC_HOST_ROW_AN500)
- `goldens/shim/shim-gpiod-an505.o`
  `eb81ef4493cdce03e65c0e6205b5e9830a898a092bb9811bac8ac7136a006125`
  (src/shim/rhu_gpiod.c; -DMGC_HOST_ROW_AN505; the Cortex-M33 class flag set)
- `goldens/shim/shim-gpiod-break-version.o`
  `af44e4166f8be8766d88e0f0a4836103b2d9b1043619a38b489c261d54eec467`
  (src/shim/rhu_gpiod.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_GPIOD_VERSION)
- `goldens/shim/shim-gpiod-break-version-an505.o`
  `3306e3a6edc944168b99322d99e7378cd830ebef1757e1e5020a905e00f5cf75`
  (src/shim/rhu_gpiod.c; -DMGC_HOST_ROW_AN505 -DRHU_SHIM_BREAK_GPIOD_VERSION; the Cortex-M33 class flag set)
- `goldens/shim/shim-busdev.o`
  `57f0a9ab681955b88b66721a26da8fde0394fedd8fb1b0f579566046b84797b7`
  (src/shim/rhu_busdev.c; -DMGC_HOST_ROW_AN500)
- `goldens/shim/shim-busdev-an505.o`
  `424f64d67d4a862977432bb9266ff9de1bc942721422c17ab18620f6f3d8dea3`
  (src/shim/rhu_busdev.c; -DMGC_HOST_ROW_AN505; the Cortex-M33 class flag set)
- `goldens/shim/shim-busdev-break-funcs.o`
  `fd7852276728892d43057b6a59a614c9f633b0bb598697befa484e7ff5c6769a`
  (src/shim/rhu_busdev.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_BUSDEV_FUNCS)
- `goldens/shim/shim-busdev-break-funcs-an505.o`
  `da0ac99ef13dccb7ff7f23a976310275aaedd7ab439bf0913f585a82413de8f6`
  (src/shim/rhu_busdev.c; -DMGC_HOST_ROW_AN505 -DRHU_SHIM_BREAK_BUSDEV_FUNCS; the Cortex-M33 class flag set)
- `goldens/shim/shimtest-busdev.o`
  `fef5775565d37675a20d4dd8852978cac9598997bec2b86f9b84f8e29d5ce051`
  (src/shim/gate/rhu_shimtest.c; -DRHU_SHIMTEST_LADDER_BUSDEV)
- `goldens/shim/shimtest-gpiod.o`
  `f53f2e0a740b4d56e1d1066c581005d32195d28297093a6a9ea77aed053fbbd0`
  (src/shim/gate/rhu_shimtest.c; -DRHU_SHIMTEST_LADDER_GPIOD)
- `goldens/shim/shim-core-break-mutex-adopt.o`
  `8cce6414d3bbb23e37e16c78c1417178a676e8791547c904d5d41d10ed11e939`
  (src/shim/rhu_shim.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_MUTEX_ADOPT)
- `goldens/shim/shim-busdev-break-select-name.o`
  `767c9eca1e954d9242a8853e1a882fecfc576186854e11fabfe54449568f7b06`
  (src/shim/rhu_busdev.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_BUSDEV_SELECT_NAME)
- `goldens/shim/shim-busdev-break-funcs-wide.o`
  `d6c7577701bd6931d2f8126243f72e4bf195bec4636c0c25d93b6e2eb787e47f`
  (src/shim/rhu_busdev.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_BUSDEV_FUNCS_WIDE)
- `goldens/shim/shim-busdev-break-reread.o`
  `0c98600db58aa28353837f11af37170f4997c3ba5b91e3944eb7ea994ba71b8c`
  (src/shim/rhu_busdev.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_BUSDEV_REREAD)
- `goldens/shim/shim-busdev-break-unowned.o`
  `85ecb82122f1f93eb75cf329cae5554ef9c2709f7551e01690ef9bb561487965`
  (src/shim/rhu_busdev.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_BUSDEV_UNOWNED)
- `goldens/shim/shim-busdev-break-rate-unjudged.o`
  `597ef260b522287d29f4641669cce15377b218ed0118642e165f27922f1362f7`
  (src/shim/rhu_busdev.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_BUSDEV_RATE_UNJUDGED)
- `goldens/shim/shim-gpiod-break-open-unlocked.o`
  `4a1266d9e5800025bd1f24a9841f4fc7c365a84bc87d29b0d92f35ef45158fcc`
  (src/shim/rhu_gpiod.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_GPIOD_OPEN_UNLOCKED)
- `goldens/shim/shim-gpiod-break-close-keeps.o`
  `98c1fabe3ec1b8b8e515f1958166c60374438385669c8782d1f0747128d289e1`
  (src/shim/rhu_gpiod.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_GPIOD_CLOSE_KEEPS)
- `goldens/shim/shim-gpiod-break-iter-shared.o`
  `90e205c07560c55dcb8523f5958d60135e11302fa5c58578ee0a92a74cab4187`
  (src/shim/rhu_gpiod.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_GPIOD_ITER_SHARED)
- `goldens/shim/shim-gpiod-break-both-pulls.o`
  `50c291c974f5772cb8f07d917f8477ce11f277a273e4aa31032941d021a6ced4`
  (src/shim/rhu_gpiod.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_GPIOD_BOTH_PULLS)
- `goldens/shim/shim-gpiod-break-bias-stale.o`
  `9c23119341eceaf8c03e1510035dc8505d778dccab662bf56378009139f1093c`
  (src/shim/rhu_gpiod.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_GPIOD_BIAS_STALE)
- `goldens/shim/shim-gpiod-break-timeout-fields.o`
  `0a9f0aa8a52acf7805bfa1ce0e4857b0d59843f11e1a0b572961aff9cff7d0dc`
  (src/shim/rhu_gpiod.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_GPIOD_TIMEOUT_FIELDS)
- `goldens/shim/shim-gpiod-break-arm-order.o`
  `208ce5779b895bd7b7ae22b835cbcf14e29319e1f68b97817d00b51e121ef785`
  (src/shim/rhu_gpiod.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_GPIOD_ARM_ORDER)
- `goldens/shim/shim-gpiod-break-kind-order.o`
  `623cd413e365415086a549d7405201512f1ae6b0719b1fc94efbf00d13cb9d9e`
  (src/shim/rhu_gpiod.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_GPIOD_KIND_ORDER)
- `goldens/shim/shim-sysnative-f769.o`
  `2e63f64f74a63b48afd4045cfa6315b665b808d455b7a9b5ffebae3ab2b1dcd1`
  (src/shim/rhu_sysnative.c; -DMGC_HOST_ROW_F769)
- `goldens/shim/shim-sysnative-rp2350.o`
  `f7be95beb6c89a7a27a6ee4c578081b9206f4e9c4d11755270192aceec52a8a4`
  (src/shim/rhu_sysnative.c; -DMGC_HOST_ROW_PICO2; the Cortex-M33 class flag set)
- `goldens/shim/shim-sysnative-f769-break-frozen-backed.o`
  `8f8b5c3d7b01cbde95ed1eb6959348d9dad547098ed81a5703f3462fcd4d0082`
  (src/shim/rhu_sysnative.c; -DMGC_HOST_ROW_F769 -DRHU_SHIM_BREAK_FROZEN_BACKED)
- `goldens/shim/shimtest-frozen.o`
  `e9ad9aa79c44fd6628edb11090d22e58c56064fc92ce1d24c7b5dc431beffba7`
  (src/shim/gate/rhu_shimtest.c; -DRHU_SHIMTEST_LADDER_FROZEN)
- `goldens/shim/shim-pwmfs.o`
  `fe6918b0cd2e6938cb321eb9fc0a839f8fdd6f8545728462e6e3445660aeef6c`
  (src/shim/rhu_pwmfs.c; -DMGC_HOST_ROW_AN500)
- `goldens/shim/shim-pwmfs-an505.o`
  `3bef04650abc22f8a091edc5d18d6600a4a9e6ddb6f9cdaae05e88ebf6469811`
  (src/shim/rhu_pwmfs.c; -DMGC_HOST_ROW_AN505; the Cortex-M33 class flag set)
- `goldens/shim/shim-pwmfs-break-period-asked.o`
  `6376ecce44b079338b141a39502836e137c064c76ba6938f8afff555fdd93f5e`
  (src/shim/rhu_pwmfs.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_PWMFS_PERIOD_ASKED)
- `goldens/shim/shim-pwmfs-break-period-asked-an505.o`
  `578474f61ab54c900447e0583b01b0d0e2b7040425ba655c724f812c11c64df3`
  (src/shim/rhu_pwmfs.c; -DMGC_HOST_ROW_AN505 -DRHU_SHIM_BREAK_PWMFS_PERIOD_ASKED; the Cortex-M33 class flag set)
- `goldens/shim/shim-pwmfs-break-export-unjudged.o`
  `6f5de6a8af5fa6b86f47436ef74ff1c40c34de4a10d1320dbe5d37c6efac85ec`
  (src/shim/rhu_pwmfs.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_PWMFS_EXPORT_UNJUDGED)
- `goldens/shim/shim-pwmfs-break-unexport-keeps.o`
  `bd9b4111adc886e52cf8c211336123ea0f0b829044d5389d60f8f3d0b6e60794`
  (src/shim/rhu_pwmfs.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_PWMFS_UNEXPORT_KEEPS)
- `goldens/shim/shim-pwmfs-break-npwm-count.o`
  `4d53afd9f7839ac6d3745a7e5d474c9bc9bfd993eae7ee91d5584c3c1af83339`
  (src/shim/rhu_pwmfs.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_PWMFS_NPWM_COUNT)
- `goldens/shim/shimtest-pwmfs.o`
  `0205fe79b7b03b9b5b1c65409d44d3e64c75ed9a057609798bf07847ed55ad6d`
  (src/shim/gate/rhu_shimtest.c; -DRHU_SHIMTEST_LADDER_PWMFS)
- `goldens/shim/shim-sysnative-break-identity-leak.o`
  `e3325956d818bcc1342ebd48e53bac1e953cedb273b07fcbb0cf169b7b137c72`
  (src/shim/rhu_sysnative.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_IDENTITY_LEAK)
- `goldens/shim/shim-core-break-fault-unarmed.o`
  `6c5b21fac276fea93bb947a74ae95e5e4d35821584c511f0bf85df888271c71f`
  (src/shim/rhu_shim.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_FAULT_UNARMED)
- `goldens/shim/shim-core-break-fault-addr.o`
  `1512c0a091b57d982a645084c514f6c4c9bfbbf9b5d98b6994459ac6fde10877`
  (src/shim/rhu_shim.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_FAULT_ADDR)
- `goldens/shim/shim-core-break-fault-unarmed-an505.o`
  `26fa7689f304b2ac216f75b0b451221b8350617645b07376b37757b23d7b5fa3`
  (src/shim/rhu_shim.c; -DMGC_HOST_ROW_AN505 -DRHU_SHIM_BREAK_FAULT_UNARMED; the Cortex-M33 class flag set)
- `goldens/shim/shim-core-break-fault-addr-an505.o`
  `81a75a1bf34a09b7e3531888dac68a997d126148674b07ce452491d3d2b0a64d`
  (src/shim/rhu_shim.c; -DMGC_HOST_ROW_AN505 -DRHU_SHIM_BREAK_FAULT_ADDR; the Cortex-M33 class flag set)
- `goldens/shim/shim-ttydev.o`
  `54c9b749fab2cbc2ffd0e35f5e5e844162dcbdca1a830ea4f6d15192df7e1ecb`
  (src/shim/rhu_ttydev.c; -DMGC_HOST_ROW_AN500)
- `goldens/shim/shim-ttydev-an505.o`
  `9cb95c14543cce5c4dec58269876f6a7b1b69753ab649dda9b6049b5e2a8d688`
  (src/shim/rhu_ttydev.c; -DMGC_HOST_ROW_AN505; the Cortex-M33 class flag set)
- `goldens/shim/shim-ttydev-break-eof.o`
  `14f0f8ff2a425a51837166f67d4fe65727dad0669f0f86e10e7e091b34fee5c8`
  (src/shim/rhu_ttydev.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_TTY_EOF)
- `goldens/shim/shim-ttydev-break-pollerr.o`
  `4c07f74cac810e9647653fd1eaf16b76e3ea413e6845c858aaeddd4784c1657f`
  (src/shim/rhu_ttydev.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_TTY_POLLERR)
- `goldens/shim/shim-ttydev-break-image.o`
  `88f16a35f0d242986e61c1b09b3db655b35ff58275f491dea4bee1b8ac7ac775`
  (src/shim/rhu_ttydev.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_TTY_IMAGE)
- `goldens/shim/shim-ttydev-break-close-keeps.o`
  `18bdbc8b79c36f1835f52fde30cf698a5dac15b096a4a6ee744246ff11d0ce56`
  (src/shim/rhu_ttydev.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_TTY_CLOSE_KEEPS)
- `goldens/shim/shim-ttydev-break-close-keeps-an505.o`
  `7e0f152043a5ad67d3562c9d76f229e869edd96eb1bf19b318054212ba6dc0fd`
  (src/shim/rhu_ttydev.c; -DMGC_HOST_ROW_AN505 -DRHU_SHIM_BREAK_TTY_CLOSE_KEEPS; the Cortex-M33 class flag set)
- `goldens/shim/shim-ports-native.o`
  `0b91acf32247d09d3e605d48a213ae7fea09ba28a838721eee0ef503804a033b`
  (src/shim/ports/rhu_ports_native.c; -DMGC_HOST_ROW_AN500 -Isrc/shim/ports -Isrc/vendor/ports/libs/System.IO.Ports.Native -Isrc/vendor/ports/libs/Common -Isrc/vendor/ports -DNDEBUG)
- `goldens/shim/shim-ports-native-an505.o`
  `fdaf6b1c2d41714a0ca7a9f53d586c33edc01a695d41451ab465466b2e6530a3`
  (src/shim/ports/rhu_ports_native.c; -DMGC_HOST_ROW_AN505 -Isrc/shim/ports -Isrc/vendor/ports/libs/System.IO.Ports.Native -Isrc/vendor/ports/libs/Common -Isrc/vendor/ports -DNDEBUG; the Cortex-M33 class flag set)
- `goldens/shim/shimtest-ttydev.o`
  `879afdf2b4a332aea2796eb922dd44a138361262d2a747c1b053a94d287aaded`
  (src/shim/gate/rhu_shimtest.c; -DRHU_SHIMTEST_LADDER_TTYDEV)
- `goldens/shim/shim-iiofs.o`
  `e93a0c7bc63af0cb589b0a7544cfc8f041412a502b2677704f71f54cbc1e2140`
  (src/shim/rhu_iiofs.c; -DMGC_HOST_ROW_AN500)
- `goldens/shim/shim-iiofs-an505.o`
  `f3d9bf622eed5abdf54bce4e5c3d218a18da85aebad57c6b3d4ce2ff61596995`
  (src/shim/rhu_iiofs.c; -DMGC_HOST_ROW_AN505; the Cortex-M33 class flag set)
- `goldens/shim/shim-iiofs-break-read-stale.o`
  `8e6303776e78fe7ef82243d26694435de1261a6c28cbe5d943e1e62c988a7e06`
  (src/shim/rhu_iiofs.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_IIOFS_READ_STALE)
- `goldens/shim/shim-iiofs-break-read-stale-an505.o`
  `85a73afbe6904226619221510c79b83577fb9dc0ad7521dca4421adecba93a7e`
  (src/shim/rhu_iiofs.c; -DMGC_HOST_ROW_AN505 -DRHU_SHIM_BREAK_IIOFS_READ_STALE; the Cortex-M33 class flag set)
- `goldens/shim/shim-iiofs-break-close-keeps.o`
  `03dd7fdf602f6ee1ecfcf2fbc6d649b2695be02ae0faef191da4d31952a47b31`
  (src/shim/rhu_iiofs.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_IIOFS_CLOSE_KEEPS)
- `goldens/shim/shim-iiofs-break-scale-bits.o`
  `6d3856084391404fff7f148f26df30a869c35a2df782d85c0eb233f5001eecab`
  (src/shim/rhu_iiofs.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_IIOFS_SCALE_BITS)
- `goldens/shim/shim-iiofs-break-open-unjudged.o`
  `0ff4a159466e79166736b5903b4aa8712e7895fc5c4b985bf59d405dd9703afc`
  (src/shim/rhu_iiofs.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_IIOFS_OPEN_UNJUDGED)
- `goldens/shim/shimtest-iiofs.o`
  `2bc9d0852be5106e73f6951dca3377fcccd3264bd62f1340f6e38a84453a7f9f`
  (src/shim/gate/rhu_shimtest.c; -DRHU_SHIMTEST_LADDER_IIOFS)
- `goldens/shim/shimtest-core.golden.txt`
  `b715243edeccbae9551e0d50e71c5bd2250e9e5730721be3fea7d6e078a8dbbb`
  (the payload stream of the shim.core image (class bytes))
- `goldens/shim/shimtest-fdmap.golden.txt`
  `1549d611cc7f29e4398fa6f46fb289ff4b07efc1be6fbf77a6b4aacf33c4df7a`
  (the payload stream of the shim.fdmap image (class bytes))
- `goldens/shim/shimtest-activation.golden.txt`
  `4988c244a7e5d35269a432e99103e9d97fe3b016d5e6b111c878785a1ad8b461`
  (the payload stream of the shim.activation image (class bytes))
- `goldens/shim/shimtest-tls.golden.txt`
  `1684cbccbea60fc52822d3389034ef541b72f63d6b33235420953694826e562b`
  (the payload stream of the shim.tls image (class bytes))
- `goldens/shim/shimtest-host.golden.txt`
  `aeb710f3cdc677e9c5ba29a1c4a432fe2389fda884b05a54dfcd2d910191cfb5`
  (the payload stream of the shim.host image (class bytes))
- `goldens/shim/shimtest-busdev.golden.txt`
  `30ae1b223fb12627bfa0b7db8775148ace656108829ae21f13da434cf797230f`
  (the payload stream of the shim.busdev image (class bytes))
- `goldens/shim/shimtest-gpiod.golden.txt`
  `b3dc58194fa5e29f608a4520e2dd1cb81af86931f6ac8aeffe6ff6d0c241e70d`
  (the payload stream of the shim.gpiod image (class bytes))
- `goldens/shim/shimtest-frozen.golden.txt`
  `09abf34b8fd1f3ad39e883eba5ccebe765be260179555b7a24fde18af9640070`
  (the payload stream of the shim.frozen-pool image (class bytes))
- `goldens/shim/shimtest-pwmfs.golden.txt`
  `a37c3ebb2dafe04e8d6b9e6521180ac3a02fe9684d157ff1f952bc4a413526ca`
  (the payload stream of the shim.pwmfs image (class bytes))
- `goldens/shim/shimtest-ttydev.golden.txt`
  `f8c5ecf48f9356c88e0102c039ea42ada4cf16e213afd5d5d2de0a110dea5841`
  (the payload stream of the shim.ttydev image (class bytes))
- `goldens/shim/shimtest-iiofs.golden.txt`
  `179324c333ddfc7ffedb8de24b7cba04fc74aee11f3190ea985419a90beadcbf`
  (the payload stream of the shim.iiofs image (class bytes))
- `goldens/shim/pal-surface.txt`
  `7597505afa88492e01701f086e25e3583460b1ee880017397bad7e2f025321c7`
  (the P/Invoke-facing surface expectation the shim.pal-surface case reads: the pin, the pack's five archives with their sha256s, one line per surface name with its family and class; GENERATED from the pack's archives and the runtime's headers at the pin, regenerated at a re-pin, never edited)
