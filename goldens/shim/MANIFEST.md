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

The objects are compiled against the .NET 11 runtime pack (v11.0.0): the
contract headers carry the .NET major in their names
(rhu_pal_<area>_net11.h, rhu_rt_entry_net11.h) and are generated from that
pack's source, and each file's identity lines name the pack it is
generated from. The shim answers two names the runtime archives demand of
the C library, getauxval (no auxiliary vector: 0 with ENOENT) and
realpath (over the RAM file store), and the test program checks both.

The class library's cross-process mutex family, nine entry points, is
answered on a single-program device by a futex word with the owner and
abandoned words the class library keeps through the accessors, in the
block the family sizes at 24 bytes. The owner's thread id is 64 bits
wide in that block and in both accessors, as the pack's header declares
them; the core ladder calls all nine against the generated surface
header's declarations, and a negative control keeps the id in 32 bits.

The RAM file store keeps a stated contract: a maximum file size with every
position, count and length checked against it in 64 bits; an object's name
and its open descriptors kept apart; one store lock; a descriptor found
before anything is created or truncated; the access mode checked at the
read boundary; advisory locks per object; paths with a trailing slash
named the same; file times range-checked; a refused open of the process
identity line giving back the object it materialized; /etc present as a
root that holds nothing, a read under it a missing file and every
creation or change at it or under it refused with the read-only file
system's code (making the root itself answers that it exists). Each
clause is a check in the test program's file ladder, with a negative
control built from the store with that clause's defect restored.

The store also serves what the class library's named wait handles and
memory-mapped files ask of a temporary folder. A read-only open of a
directory gives a descriptor, and an open that writes one is refused as
a directory; openat, mkdirat and unlinkat act under their descriptor's
directory as the path forms act on the composed path; a directory keeps
the mode it was made with until chmod or fchmod changes it; a
directory's descriptor carries the advisory lock; a lock request in
conflict is refused at once, whether it asks to wait or not. A shared
mapping of a file is the file's own bytes at a page-aligned address:
the file is resized in place within its block while a mapping stands,
and the block is freed when no name, no descriptor and no mapping is
left. A block of a page or more is whole pages, carved by the kernel
heap's aligned entry. The store's bounds are stated and read by count:
32 files, 24 made directories, a path of 127 characters and 29
descriptors in two ranges of numbers. Each clause is a group of the file
ladder; eight more negative controls restore one defect each, and the
refusal of a waiting lock request is read with no control of its own.

The card's files: a path under /media/sd0 reaches the kernel's file
functions over the FAT volume the kernel mounts while a card is seated,
through the storage band of sixteen descriptors (a dup a second
descriptor on one kernel handle, closed with the last), a directory
stream over a kernel directory handle, the kernel's negative code as
errno, the status record from the kernel's, and the mount-point list
naming the point while the volume stands with the space from the
kernel's cluster counts; /media is a directory of the store that lists
sd0 while the volume stands. The test program's card ladder serves the
kernel's family itself through the unit's hook (the emulator row has no
socket) and reads the routing, the band, the records and the refusals,
with three negative controls built from the unit with a defect restored
(the code taken as errno without its sign, a close keeping its slot, a
dup's first close closing the kernel's handle).

The socket functions the .NET socket classes call are presented over the
kernel's network functions in three units of their own: the
socket-address helpers with the map from the kernel's network codes to
the runtime's errors; the sockets, a band of sixteen descriptors beside
the file bands, with the blocking calls and their timeouts, Poll over
files and sockets in one call and the event port the runtime's socket
engine waits on, one wait over the kernel's readiness words serving them
all; and name resolution with the network interface records. The
descriptor layer reaches the socket unit through weak references, and an
image that links no socket unit answers the band's descriptors as bad
descriptors. Each unit's ladder runs in this gate from a native caller's
side over a scripted kernel (the test program serves the unit's table of
kernel functions through the unit's hook), with sixteen negative controls
built from the units with one defect restored each; the names the surface
refuses are read for the value each caller expects, with one negative
control built from the surface with one refusal answering success. The
datagram ladder's scripted kernel holds two datagram sockets, as the
kernel's pool does, and the ladder reads that an IPv6 socket asked while
both are held is refused with the family's code, a third IPv4 one with
the pool's.

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
releasing nothing, and npwm as the kernel function's count. The unit
holds no bound of its own on a channel number: its table has one entry a
PWM-capable line, at the kernel's index of the line, so the line's number
is a name alone. Two further cases drive the tree over a board row's own
line table: their unit object is the unit's source compiled to call the
test program's functions in place of the kernel's eight PWM calls and the
shim's diagnostic function (the names renamed on the compile line), and
the test program answers as the kernel with the row's PWM-capable lines,
one image a row. A fifth control, built from that object, refuses a
channel number of 64 or more before the kernel is asked, and each of the
two images refuses it naming the row's lines of that size.

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
the row's default when absent. The bounds the core answers for a
thread's stack are the ones the runtime's stack-depth helper needs, not
the carve: the upper bound is the carve's own, and the lower bound
stands 65536 bytes less a reserve of 6144 under the carve's base, so the
helper, which adds 65536 to the lower bound it is told, stops guarded
code 6144 bytes above the true base on a carve of any size larger than
the reserve; a carve not larger than the reserve is answered as it is.
A thread's true bounds are the kernel's (rhu_thread_stack_bounds). The
core ladder reads all of it against the kernel's answer, and a control
object of the core built to answer the carve as it is, kept for both
processor classes, is refused at that check.

The core's own lines and the host's go out through the kernel's
log function at the verbose level, and the last word that ends the program
at the silent level, so a quiet or silent image prints none of them. The
host object states hosting-API
generation 42, the number the kernel's header states for its hosting
API, which a hosted image must match. The host object also defines the
debug events word, 32 bits of zero-filled memory that a debugger writes,
and the routine the class-library table carries for an unhandled
exception: with the word's marker and the event's bit set the routine
executes a breakpoint instruction and then calls the class library's
function, and with the word clear it calls that function alone. The
host ladder runs the routine both ways; the run with the word set ends
at the kernel's fault report at that instruction's address.

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
project's property defined one, and the thread pool's most-workers row
when the image's link line defined its word: the number in hexadecimal,
the form the pool reads, and no row without the word. The core ladder
is linked both ways, and a control object of the core that writes the
number in decimal is refused. The
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
(the five class-library families' export tables, 760 names): the names the
hand-written units serve over the RAM file store, the kernel's calendar,
its secure entropy call and its network functions, and for every name of
an area this system does not serve a generated refusal (nothing printed, nothing trapped; the
errno of record where the convention is errno-style) in the surface object
beside the row's link objects, with its Cortex-M33 twin for the an505 row.
The expectation the gate's surface case reads, generated from the pack's
archives and the runtime's headers, is the tier's text record, tied to the
pack and to every family name the objects define.

156 objects, 25 goldens and 1 record.

Records (repo-relative path, sha256; source and defines per object):

- `goldens/shim/shim-core.o`
  `d94cd7db1f0831cfa8fad043dfae6afcff0ec4c8885b940d27b16a4bfb8a805a`
  (src/shim/rhu_shim.c; -DMGC_HOST_ROW_AN500)
- `goldens/shim/shim-core-break-sipid.o`
  `22bf080afcc69c17f3817ab5e9dd246a0764294be8042022ad469cd8cbf085a8`
  (src/shim/rhu_shim.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_SIPID)
- `goldens/shim/shim-core-break-ctxpc.o`
  `931c55b2285d4c0608643f0546ad324d8261914a2fd9ebef458538d5076ce87b`
  (src/shim/rhu_shim.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_CTXPC)
- `goldens/shim/shim-core-break-tlsoff.o`
  `5ddd594121b723dfa041969a3c8d3124d135a6c3c8fb1f5511de8463a98d5a74`
  (src/shim/rhu_shim.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_TLSOFF)
- `goldens/shim/shim-core-break-tlscache.o`
  `d5e9b036da805cb33bfa576b688b831a4dc04057657d5c530cc34854334c147b`
  (src/shim/rhu_shim.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_TLSCACHE)
- `goldens/shim/shim-core-break-realtime-wait.o`
  `5a123a0e8ea079ccd21a7e705711792c17c9ccee41316cc8eae003ce6f7c9129`
  (src/shim/rhu_shim.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_REALTIME_WAIT)
- `goldens/shim/shim-sysnative.o`
  `e8c81d9109976bae1317604625c21d2069dcb5536f6992e951da2b265f757004`
  (src/shim/rhu_sysnative.c; -DMGC_HOST_ROW_AN500)
- `goldens/shim/shim-sysnative-break-fdbase.o`
  `d3ada4dd9564be6cfab3962ddecad0a6589aa101eb0b2ad96623028c2a66176b`
  (src/shim/rhu_sysnative.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_FD_BASE)
- `goldens/shim/shim-sysnative-break-store-bounds.o`
  `348620b5b6f8ff77fb11b185c0034f9d6a3cea928c2f24188854ef384a3ed64a`
  (src/shim/rhu_sysnative.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_STORE_BOUNDS)
- `goldens/shim/shim-sysnative-break-store-lifetime.o`
  `c10fe351a26674f76a2c9b5f1e62c30b1effcfd0c1a092b67fa833e7e1ca11d8`
  (src/shim/rhu_sysnative.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_STORE_LIFETIME)
- `goldens/shim/shim-sysnative-break-store-lock.o`
  `d6efdd86ce4a7d7e9d04b0893f4804444a1263488c5e9ffa3331b0942ed0663e`
  (src/shim/rhu_sysnative.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_STORE_LOCK)
- `goldens/shim/shim-sysnative-break-store-open-order.o`
  `bbedf695a6ec7756be2d9a319d6abb163cf0c98e1c8faac4055594938ed22922`
  (src/shim/rhu_sysnative.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_STORE_OPEN_ORDER)
- `goldens/shim/shim-sysnative-break-store-access.o`
  `d65d188f2b72a9c6193f2f77ae5c1eac7749d576d467d820ebe58362e838c9a3`
  (src/shim/rhu_sysnative.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_STORE_ACCESS)
- `goldens/shim/shim-sysnative-break-store-flock.o`
  `5d8d0a2c9274521aed309fca7f7f00c1c28be9c29441beda4bf5d14fac758c74`
  (src/shim/rhu_sysnative.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_STORE_FLOCK)
- `goldens/shim/shim-sysnative-break-store-slash.o`
  `bef7f4bfd6687e1d1bcb7189f228f713be916d8674c3a92168ef62e60643edb0`
  (src/shim/rhu_sysnative.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_STORE_SLASH)
- `goldens/shim/shim-sysnative-break-store-times.o`
  `3bfc4787412ff373cc302699a806a54f24be717c27bc927d72a4f3146cbb2128`
  (src/shim/rhu_sysnative.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_STORE_TIMES)
- `goldens/shim/shim-sysnative-break-entropy-refusal.o`
  `b6ec2403fe3d22807280c980c456002ade034e7338c40f5fa63704c0164a18f4`
  (src/shim/rhu_sysnative.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_ENTROPY_REFUSAL)
- `goldens/shim/shim-sysnative-break-calendar-clock.o`
  `d12d548c6dae65493370bc03018ac4c4eed4eb5c97c5c8fa2687762dfde9094d`
  (src/shim/rhu_sysnative.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_CALENDAR_CLOCK)
- `goldens/shim/shim-libc-glue.o`
  `feffa0977cc6ac71e97c7b203f4c6c7640ef951ebc688c7fcefb77bbdf076f49`
  (src/shim/rhu_libc_glue.c; no extra defines)
- `goldens/shim/shim-host.o`
  `afb2a236e569221ff9545e4fd11564feddd6ee8f0fb35078298207c548eabb1f`
  (src/shim/rhu_host.c; -DMGC_HOST_ROW_AN500)
- `goldens/shim/shim-host-break-dataverify.o`
  `dc04a53b90dcfb5f13a293d89b776636ecc6fbeccf05977f70c3fd56373c7a78`
  (src/shim/rhu_host.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_DATAVERIFY)
- `goldens/shim/shim-stdio-sym.o`
  `8428ce33a5f364f458b0ccaa949e3398470a9b05500810c6a62109731033a28d`
  (src/shim/rhu_stdio_sym.c; no extra defines)
- `goldens/shim/shimtest-core.o`
  `7297ae15684e46db9dd7382520f69e884ec79b395b05b96ccdf632176c3228a2`
  (src/shim/gate/rhu_shimtest.c; -DRHU_SHIMTEST_LADDER_CORE)
- `goldens/shim/shimtest-core-break-errno.o`
  `4f9d4a7a919c9b42a29da50ebd609e85d5a45d3a7921c99c6b4b35dddcda6a59`
  (src/shim/gate/rhu_shimtest.c; -DRHU_SHIMTEST_LADDER_CORE -DRHU_SHIMTEST_BREAK_ERRNO)
- `goldens/shim/shimtest-core-break-canary.o`
  `d6d2badc93fd5fe67ec14f645ed5d165543c670180863bcdb2a79ba81a2901ae`
  (src/shim/gate/rhu_shimtest.c; -DRHU_SHIMTEST_LADDER_CORE -DRHU_SHIMTEST_BREAK_CANARY)
- `goldens/shim/shimtest-fdmap.o`
  `4a9e3c96d3aaa2b92277bc950482933519154c3db2ae3102a5796b2d2a4862db`
  (src/shim/gate/rhu_shimtest.c; -DRHU_SHIMTEST_LADDER_FDMAP)
- `goldens/shim/shimtest-activation.o`
  `d21b119e133c961a37c2d1d54643ab1f907697634b9fe8f99fbac54635dc925d`
  (src/shim/gate/rhu_shimtest.c; -DRHU_SHIMTEST_LADDER_ACTIVATION)
- `goldens/shim/shimtest-tls.o`
  `10a30bc83fc94b99ae70fda477719731fc61abc6dec2fb665e5b1f5133bc1786`
  (src/shim/gate/rhu_shimtest.c; -DRHU_SHIMTEST_LADDER_TLS)
- `goldens/shim/shimtest-host.o`
  `1bde522fc6d06c7e3751640fd6b02b40f4ec262173d75bcd1e5e3b3c703d73ed`
  (src/shim/gate/rhu_shimtest.c; -DRHU_SHIMTEST_LADDER_HOST)
- `goldens/shim/shim-core-f769.o`
  `743c96ecd4c376e13d79df1657d6f03658cef34ffb6ced1d78a89b47d15c79a4`
  (src/shim/rhu_shim.c; -DMGC_HOST_ROW_F769)
- `goldens/shim/shim-core-an505.o`
  `167b7190a95ddc333f73effa6863d5291c958f5f714758e9358be66259941b8c`
  (src/shim/rhu_shim.c; -DMGC_HOST_ROW_AN505; the Cortex-M33 class flag set)
- `goldens/shim/shim-sysnative-an505.o`
  `0354dc4f20a99980f9ae38aef3ffec2640907dac3c67cd148f2822251bc82a54`
  (src/shim/rhu_sysnative.c; -DMGC_HOST_ROW_AN505; the Cortex-M33 class flag set)
- `goldens/shim/shim-libc-glue-an505.o`
  `45d2179c03034d89fcf2e1633e48bf21a56a5255561697ed1a1021f2e27a5618`
  (src/shim/rhu_libc_glue.c; no extra defines; the Cortex-M33 class flag set)
- `goldens/shim/shim-host-an505.o`
  `861ffb02573c0068ea9ecbe1202682072400ca37c4f206deb0b9b3aaa6bafcd6`
  (src/shim/rhu_host.c; -DMGC_HOST_ROW_AN505; the Cortex-M33 class flag set)
- `goldens/shim/shim-stdio-sym-an505.o`
  `49a42ffaf600fe1315997337de4e9457f6454072c823f8ce3be0d7e4abfb3bb9`
  (src/shim/rhu_stdio_sym.c; no extra defines; the Cortex-M33 class flag set)
- `goldens/shim/shim-core-rp2350.o`
  `3c28b72756227e5586fb6640ed31d20c7e4e4fea61321097aca338352c08e78f`
  (src/shim/rhu_shim.c; -DMGC_HOST_ROW_PICO2; the Cortex-M33 class flag set)
- `goldens/shim/shim-pal-surface.o`
  `da0216b82376775d0bd77f43e99bbb32a408c1aa5b3a8ecf19a90c1b45df4809`
  (src/shim/rhu_pal_surface_net11.c; -DMGC_HOST_ROW_AN500)
- `goldens/shim/shim-pal-surface-an505.o`
  `27865496c6151c5c7706d1a886675c91ba8c92719d60ef0a64c8499867431c92`
  (src/shim/rhu_pal_surface_net11.c; -DMGC_HOST_ROW_AN505; the Cortex-M33 class flag set)
- `goldens/shim/shim-gpiod.o`
  `e1cfc8db1562021e5b4d32f9ab00e0b31d1da32241dab729e3f71eb6fc931778`
  (src/shim/rhu_gpiod.c; -DMGC_HOST_ROW_AN500)
- `goldens/shim/shim-gpiod-an505.o`
  `9db6e0885d945c4669606c85e5af627009a79209607dc7345cdb836ab01ccf18`
  (src/shim/rhu_gpiod.c; -DMGC_HOST_ROW_AN505; the Cortex-M33 class flag set)
- `goldens/shim/shim-gpiod-break-version.o`
  `fa599a2f23569a04d0439006b63ce2047cbb83ffc4ca5c2083d3c440f938b392`
  (src/shim/rhu_gpiod.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_GPIOD_VERSION)
- `goldens/shim/shim-gpiod-break-version-an505.o`
  `25c6cd9e20406ee15fd7bd99f6859649bc39857aec75f8a2feb53e028f94c3b3`
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
  `9775f47555d65d104048c8445747c6c0e17681ac5283241d5b5a8c8fc8a26479`
  (src/shim/gate/rhu_shimtest.c; -DRHU_SHIMTEST_LADDER_BUSDEV)
- `goldens/shim/shimtest-gpiod.o`
  `6b696706a880f02e1d9526796c674df1287b96ddc355e3ba23f7be5b58c7d16f`
  (src/shim/gate/rhu_shimtest.c; -DRHU_SHIMTEST_LADDER_GPIOD)
- `goldens/shim/shim-core-break-mutex-adopt.o`
  `41aa280eca2b5f31f0adf083b01908f58e764e5b7519cb2b62397bf5b11ea2ca`
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
  `5bb40209ba0668aef5d4abb7dafcdd3ab52dd20c53babb9cdece0ef462d07db4`
  (src/shim/rhu_gpiod.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_GPIOD_OPEN_UNLOCKED)
- `goldens/shim/shim-gpiod-break-close-keeps.o`
  `3d78fffb3edd0e90d7261b53ef2ff6c8d2e76940106ad12b7e65c18c3fcb1bf7`
  (src/shim/rhu_gpiod.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_GPIOD_CLOSE_KEEPS)
- `goldens/shim/shim-gpiod-break-iter-shared.o`
  `133276e99fabeb65e0107eab99070f3723d0c063fbe0538bd5b9d4a45647949c`
  (src/shim/rhu_gpiod.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_GPIOD_ITER_SHARED)
- `goldens/shim/shim-gpiod-break-both-pulls.o`
  `ac0b0cefa5848b5b8081d895ef0292b744a93c1e9deddd29437d0be93ff8c4ec`
  (src/shim/rhu_gpiod.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_GPIOD_BOTH_PULLS)
- `goldens/shim/shim-gpiod-break-bias-stale.o`
  `9ac56e796332a81fe493c4b47ba42de297d2d8806454fe69b49e36debca230d8`
  (src/shim/rhu_gpiod.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_GPIOD_BIAS_STALE)
- `goldens/shim/shim-gpiod-break-timeout-fields.o`
  `fd281b5277f7c385af35fc8ec8a60fb1be341c7702cdeba2e224988e3fdf5101`
  (src/shim/rhu_gpiod.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_GPIOD_TIMEOUT_FIELDS)
- `goldens/shim/shim-gpiod-break-arm-order.o`
  `5dcfaaea221763c5643f513a111d7d0f352200729a54feb34971eb65ff692187`
  (src/shim/rhu_gpiod.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_GPIOD_ARM_ORDER)
- `goldens/shim/shim-gpiod-break-kind-order.o`
  `a53a5efab285904bb1eae15f4816510a6947d28214c23e86d95d7533c3ab3c21`
  (src/shim/rhu_gpiod.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_GPIOD_KIND_ORDER)
- `goldens/shim/shim-sysnative-f769.o`
  `7b89ad74375fe6ecd893cf53760ee2cc9106af898af6a96899598d524054b203`
  (src/shim/rhu_sysnative.c; -DMGC_HOST_ROW_F769)
- `goldens/shim/shim-sysnative-rp2350.o`
  `8429f7164578558e31266e7257e21f686ea295356fb060d25b4121b44f422c68`
  (src/shim/rhu_sysnative.c; -DMGC_HOST_ROW_PICO2; the Cortex-M33 class flag set)
- `goldens/shim/shim-sysnative-f769-break-frozen-backed.o`
  `fff1916576798874b48bd809d46cb59e4ea49c01eafc46afe5c9ad1fb32b2d96`
  (src/shim/rhu_sysnative.c; -DMGC_HOST_ROW_F769 -DRHU_SHIM_BREAK_FROZEN_BACKED)
- `goldens/shim/shimtest-frozen.o`
  `e9ad9aa79c44fd6628edb11090d22e58c56064fc92ce1d24c7b5dc431beffba7`
  (src/shim/gate/rhu_shimtest.c; -DRHU_SHIMTEST_LADDER_FROZEN)
- `goldens/shim/shim-pwmfs.o`
  `24294d98e27ca2087749e2cc866c87bd95416d1dc021678727ad415ee3df1038`
  (src/shim/rhu_pwmfs.c; -DMGC_HOST_ROW_AN500)
- `goldens/shim/shim-pwmfs-an505.o`
  `75663f175c50df8b4774591f28373437232be176450882eac433421e781b01e2`
  (src/shim/rhu_pwmfs.c; -DMGC_HOST_ROW_AN505; the Cortex-M33 class flag set)
- `goldens/shim/shim-pwmfs-break-period-asked.o`
  `07b73ae190a2ccf7652f87d8c77210f9dc3972fca2d35c99c39942d274d90d36`
  (src/shim/rhu_pwmfs.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_PWMFS_PERIOD_ASKED)
- `goldens/shim/shim-pwmfs-break-period-asked-an505.o`
  `dc0efd80a44886a189bfbf1689668ca1573167e6f6334907f5776938f83fd050`
  (src/shim/rhu_pwmfs.c; -DMGC_HOST_ROW_AN505 -DRHU_SHIM_BREAK_PWMFS_PERIOD_ASKED; the Cortex-M33 class flag set)
- `goldens/shim/shim-pwmfs-break-export-unjudged.o`
  `d0dae1f6fa67a738e4522becec7ff16c3ca4cfeef3a4dbb774c698b710aefd80`
  (src/shim/rhu_pwmfs.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_PWMFS_EXPORT_UNJUDGED)
- `goldens/shim/shim-pwmfs-break-unexport-keeps.o`
  `1547fe89ee99ea4748fd813db981b789fd011f0f08f0136f1df6ee992cce76f6`
  (src/shim/rhu_pwmfs.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_PWMFS_UNEXPORT_KEEPS)
- `goldens/shim/shim-pwmfs-break-npwm-count.o`
  `81b3487c5d0aeee23d75f03e43986734c4c5e52e35e57ab4ab57c0ca323451ba`
  (src/shim/rhu_pwmfs.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_PWMFS_NPWM_COUNT)
- `goldens/shim/shimtest-pwmfs.o`
  `0205fe79b7b03b9b5b1c65409d44d3e64c75ed9a057609798bf07847ed55ad6d`
  (src/shim/gate/rhu_shimtest.c; -DRHU_SHIMTEST_LADDER_PWMFS)
- `goldens/shim/shim-sysnative-break-identity-leak.o`
  `916b3dabc72b4c36a24e4a205b0e83e6b4fc5f7c13009867a876f29bf6c213f6`
  (src/shim/rhu_sysnative.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_IDENTITY_LEAK)
- `goldens/shim/shim-core-break-fault-unarmed.o`
  `0a221118730baf3004d8bcd4cf3720dbdbdfa04b835c7f745772c0a0720a088c`
  (src/shim/rhu_shim.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_FAULT_UNARMED)
- `goldens/shim/shim-core-break-fault-addr.o`
  `705aebafcaeb26dc36c9433c3f7c8f1cfec1584fecff5815105a0b64a744b14f`
  (src/shim/rhu_shim.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_FAULT_ADDR)
- `goldens/shim/shim-core-break-fault-unarmed-an505.o`
  `5375b3060e4a062346b0a65dc6b48f04244bf307b8fd5886304562527fc3d6c5`
  (src/shim/rhu_shim.c; -DMGC_HOST_ROW_AN505 -DRHU_SHIM_BREAK_FAULT_UNARMED; the Cortex-M33 class flag set)
- `goldens/shim/shim-core-break-fault-addr-an505.o`
  `5e8ff68b9cc1d2c2370389c157960076405bbb921f7529146d952c393bda0eef`
  (src/shim/rhu_shim.c; -DMGC_HOST_ROW_AN505 -DRHU_SHIM_BREAK_FAULT_ADDR; the Cortex-M33 class flag set)
- `goldens/shim/shim-ttydev.o`
  `e08f5055e0eacfb68c7e84c2159817c08a45240fe070b36cb9dfabbb650517b6`
  (src/shim/rhu_ttydev.c; -DMGC_HOST_ROW_AN500)
- `goldens/shim/shim-ttydev-an505.o`
  `162795f3590178366e5662cf8eeddfb775e5fd68949000ed18399406122224c2`
  (src/shim/rhu_ttydev.c; -DMGC_HOST_ROW_AN505; the Cortex-M33 class flag set)
- `goldens/shim/shim-ttydev-break-eof.o`
  `32f620c7fbd2656a424b24361b06b5c6dc041821ce0b30b251faeddad408d6d9`
  (src/shim/rhu_ttydev.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_TTY_EOF)
- `goldens/shim/shim-ttydev-break-pollerr.o`
  `2b60c1433ff96608c3e9e0a9485ff1080aa4fae61f8cbceb8e573d1a9a0402aa`
  (src/shim/rhu_ttydev.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_TTY_POLLERR)
- `goldens/shim/shim-ttydev-break-image.o`
  `ee02138b116ad7c37dacbbd7f05f25574528ef426d54c218e035cc9f3e8c7842`
  (src/shim/rhu_ttydev.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_TTY_IMAGE)
- `goldens/shim/shim-ttydev-break-close-keeps.o`
  `0a1d02799a50b4a67a754ec87341ef7f8c201e008128809476a242d632215a96`
  (src/shim/rhu_ttydev.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_TTY_CLOSE_KEEPS)
- `goldens/shim/shim-ttydev-break-close-keeps-an505.o`
  `4aaf4e131ea07e971cb17fbf66913c26589ec8db9d46625f03e38b119fb0251c`
  (src/shim/rhu_ttydev.c; -DMGC_HOST_ROW_AN505 -DRHU_SHIM_BREAK_TTY_CLOSE_KEEPS; the Cortex-M33 class flag set)
- `goldens/shim/shim-ports-native.o`
  `0b91acf32247d09d3e605d48a213ae7fea09ba28a838721eee0ef503804a033b`
  (src/shim/ports/rhu_ports_native.c; -DMGC_HOST_ROW_AN500 -Isrc/shim/ports -Isrc/vendor/ports/libs/System.IO.Ports.Native -Isrc/vendor/ports/libs/Common -Isrc/vendor/ports -DNDEBUG)
- `goldens/shim/shim-ports-native-an505.o`
  `fdaf6b1c2d41714a0ca7a9f53d586c33edc01a695d41451ab465466b2e6530a3`
  (src/shim/ports/rhu_ports_native.c; -DMGC_HOST_ROW_AN505 -Isrc/shim/ports -Isrc/vendor/ports/libs/System.IO.Ports.Native -Isrc/vendor/ports/libs/Common -Isrc/vendor/ports -DNDEBUG; the Cortex-M33 class flag set)
- `goldens/shim/shimtest-ttydev.o`
  `62a3a467cafcdf601876631d3c428f5ee6645e48da79f3af68570d460ec76ab3`
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
- `goldens/shim/shim-sysnative-break-sdfs-sign.o`
  `a702ae5d9904f49a798775b38dbcd08f94d85e865a75499bf3d149c76c284209`
  (src/shim/rhu_sysnative.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_SDFS_SIGN)
- `goldens/shim/shim-sysnative-break-sdfs-close-keeps.o`
  `8ddefadaa0128d1005592e22af3e67377dc84f1856a33530b29edc422bbef76b`
  (src/shim/rhu_sysnative.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_SDFS_CLOSE_KEEPS)
- `goldens/shim/shim-sysnative-break-sdfs-dup-closes.o`
  `dfcf82776625d6b6d2d09b1388d042b7b5696fc62249b33893ad491d99f5ea13`
  (src/shim/rhu_sysnative.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_SDFS_DUP_CLOSES)
- `goldens/shim/shimtest-sdfs.o`
  `f0a9ad05357b6226d7f605a983a055148a37daa0334f0f08645751343e40d726`
  (src/shim/gate/rhu_shimtest.c; -DRHU_SHIMTEST_LADDER_SDFS)
- `goldens/shim/shim-sockaddr.o`
  `451c5ad1ce52da61e32e7f64998650f2d2387461641cc4c33831ccc37f2fb411`
  (src/shim/rhu_sockaddr.c; no extra defines)
- `goldens/shim/shim-sockaddr-an505.o`
  `050e8b316f9fdef6a220e1f1dc872d0bb04b794dedeb5f184a388507f0b963e2`
  (src/shim/rhu_sockaddr.c; no extra defines; the Cortex-M33 class flag set)
- `goldens/shim/shim-sockaddr-break-port-order.o`
  `6c603ca9cefa0215c51bef1b9819e666e38b4a5e0ed320221af1f5e648efc9bc`
  (src/shim/rhu_sockaddr.c; -DRHU_SHIM_BREAK_SOCK_PORT_ORDER)
- `goldens/shim/shim-sockaddr-break-size.o`
  `4233bfee55a9b09e88567f8b471b886436f32307a32eed817d158de36cb2cd84`
  (src/shim/rhu_sockaddr.c; -DRHU_SHIM_BREAK_SOCK_SIZE)
- `goldens/shim/shim-sockaddr-break-errmap.o`
  `08fdf349ca75b7d6e1c9dbd79d35ff37dec77676d4f9d75285d86cba5dbb458f`
  (src/shim/rhu_sockaddr.c; -DRHU_SHIM_BREAK_SOCK_ERRMAP)
- `goldens/shim/shim-socket.o`
  `484a6038b7b94873d2a1584dcf6620189b3a92a9dd7e8a134663777f068767fe`
  (src/shim/rhu_socket.c; no extra defines)
- `goldens/shim/shim-socket-an505.o`
  `16ef7110dd523e7df1a65557bc5a8640fd7b76d2d566d90c66103f95f0476873`
  (src/shim/rhu_socket.c; no extra defines; the Cortex-M33 class flag set)
- `goldens/shim/shim-socket-break-peek.o`
  `31d8825d87d1b2c4a25e2ac66d37fdb374e68d5b9ea946772a67df20b231d96d`
  (src/shim/rhu_socket.c; -DRHU_SHIM_BREAK_SOCK_PEEK)
- `goldens/shim/shim-socket-break-pending-error.o`
  `2dd8b1185401cd6706d5429e54035bf9b6fe66091f271229776c4450868394c2`
  (src/shim/rhu_socket.c; -DRHU_SHIM_BREAK_SOCK_PENDING_ERROR)
- `goldens/shim/shim-socket-break-timeout.o`
  `59319b22cbd28b5af3437c91087f048eaf239e7dedf0a393e1a78cc96270420e`
  (src/shim/rhu_socket.c; -DRHU_SHIM_BREAK_SOCK_TIMEOUT)
- `goldens/shim/shim-socket-break-close-linger.o`
  `25d787719a81c1c444608606b584d3ab26f9e839c19c5a1e2b8ae7b816914f9c`
  (src/shim/rhu_socket.c; -DRHU_SHIM_BREAK_SOCK_CLOSE_LINGER)
- `goldens/shim/shim-socket-break-accept-peer.o`
  `3313a384fa879f5bd3d49381c6ae2dc4cb5d6c9a70e52924e5e11ae2a44af126`
  (src/shim/rhu_socket.c; -DRHU_SHIM_BREAK_SOCK_ACCEPT_PEER)
- `goldens/shim/shim-socket-break-gather.o`
  `75bd985c693c708a9405e5db4a2936e46e7a31e01b098fe8e1942d085d4ae23e`
  (src/shim/rhu_socket.c; -DRHU_SHIM_BREAK_SOCK_GATHER)
- `goldens/shim/shim-socket-break-option-refusal.o`
  `9239b412df31dd50c35b39a789f738cdc3d2fd0559362d86b3d5e28272720bca`
  (src/shim/rhu_socket.c; -DRHU_SHIM_BREAK_SOCK_OPTION_REFUSAL)
- `goldens/shim/shim-socket-break-poll-level.o`
  `fb2a75b77f63e5bc6f144b9318a29d3f5242bf129bd8934b4a1d3ac25ff7aa43`
  (src/shim/rhu_socket.c; -DRHU_SHIM_BREAK_SOCK_POLL_LEVEL)
- `goldens/shim/shim-socket-break-port-level.o`
  `a3dc2806a0536c6b5269ff7ddfd7ced7595799438e5818758c9b0f1c447de209`
  (src/shim/rhu_socket.c; -DRHU_SHIM_BREAK_SOCK_PORT_LEVEL)
- `goldens/shim/shim-socket-break-port-lost-registration.o`
  `986a2b004ba937a20aeb677db719ad7e1c7867395ddca93a679891ff65db499e`
  (src/shim/rhu_socket.c; -DRHU_SHIM_BREAK_SOCK_PORT_LOST_REGISTRATION)
- `goldens/shim/shim-socket-break-port-close-registration.o`
  `18f472154ac8263e7382222c8bddc6982754e82e579112b6d0525072164556c0`
  (src/shim/rhu_socket.c; -DRHU_SHIM_BREAK_SOCK_PORT_CLOSE_REGISTRATION)
- `goldens/shim/shim-netinfo.o`
  `acfe39d32c391f8df8538921327b6d72271baf9dea6bc5013b09531cfda22ffa`
  (src/shim/rhu_netinfo.c; no extra defines)
- `goldens/shim/shim-netinfo-an505.o`
  `686785a16525a99a4366622fff98421a0a6c9b3c4d96e8acd7203f3e0cfaab45`
  (src/shim/rhu_netinfo.c; no extra defines; the Cortex-M33 class flag set)
- `goldens/shim/shim-netinfo-break-slot-leak.o`
  `d0239f4017949519ad7558952c1da79993bfdd57dbd0007ca85f6749c2ada467`
  (src/shim/rhu_netinfo.c; -DRHU_SHIM_BREAK_SOCK_RESOLVE_SLOT_LEAK)
- `goldens/shim/shim-netinfo-break-prefix.o`
  `f8d6ba38af1e98bcb77bcf951a5d1a9e1aebc7faafb1f23718152f28448a1da8`
  (src/shim/rhu_netinfo.c; -DRHU_SHIM_BREAK_SOCK_NETINFO_PREFIX)
- `goldens/shim/shim-pal-surface-break-refusal-value.o`
  `24335a2bf471a5f41181d4be73a51188c273b45d6614acc162a2bc9f721f507f`
  (src/shim/rhu_pal_surface_net11.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_SURFACE_REFUSAL_VALUE)
- `goldens/shim/shimtest-sockaddr.o`
  `e6cec84644e17b3afce3ebd862c3633f7f1f4aeb30f57518ec588bac9eda73ae`
  (src/shim/gate/rhu_shimtest.c; -DRHU_SHIMTEST_LADDER_SOCKADDR)
- `goldens/shim/shimtest-sockerrors.o`
  `98a241864b5b4fa6b31538ea45c29239600289a92056238bb9c77c3564cfd0bf`
  (src/shim/gate/rhu_shimtest.c; -DRHU_SHIMTEST_LADDER_SOCKERRORS)
- `goldens/shim/shimtest-sockrefusals.o`
  `5a069fbe01b287935c97ee1e58bde4f74c23fffb15a61eaf8695f043c5828076`
  (src/shim/gate/rhu_shimtest.c; -DRHU_SHIMTEST_LADDER_SOCKREFUSALS)
- `goldens/shim/shimtest-sockstream.o`
  `499267c938bbf5d41598d453841b0c9dc9262be4f7560dea6dba8f755a084935`
  (src/shim/gate/rhu_shimtest.c; -DRHU_SHIMTEST_LADDER_SOCKSTREAM)
- `goldens/shim/shimtest-socklisten.o`
  `f198aa8c77ee7cbdee45d0cbfa691e8a81802c01fd0a4477d3bedec18a53ff01`
  (src/shim/gate/rhu_shimtest.c; -DRHU_SHIMTEST_LADDER_SOCKLISTEN)
- `goldens/shim/shimtest-sockdatagram.o`
  `467f466ad796199aa9b90d7dec57ffa46d4cb33148e00593a66a54f149b82b0c`
  (src/shim/gate/rhu_shimtest.c; -DRHU_SHIMTEST_LADDER_SOCKDATAGRAM)
- `goldens/shim/shimtest-sockoptions.o`
  `fbc8deba81906497d3ee6d145e687f1e3b7d88ea297606c2c2b075c1018df41a`
  (src/shim/gate/rhu_shimtest.c; -DRHU_SHIMTEST_LADDER_SOCKOPTIONS)
- `goldens/shim/shimtest-sockpoll.o`
  `03c95d5147cbc399cce2e652a16323805a70f85c91fb042bd95af44e38141be5`
  (src/shim/gate/rhu_shimtest.c; -DRHU_SHIMTEST_LADDER_SOCKPOLL)
- `goldens/shim/shimtest-sockport.o`
  `14fd58ce0240c389ff4b9f66865aed745af658f60a7bbad4da239d92b315489c`
  (src/shim/gate/rhu_shimtest.c; -DRHU_SHIMTEST_LADDER_SOCKPORT)
- `goldens/shim/shimtest-sockresolve.o`
  `b8a7af33d360524325ae41062ba03f8515431b5cb45d454ea0c3a68a48cb508d`
  (src/shim/gate/rhu_shimtest.c; -DRHU_SHIMTEST_LADDER_SOCKRESOLVE)
- `goldens/shim/shimtest-socknetinfo.o`
  `2c62299e7584687844d418e9b2c865c35d6407dc53ff745d2639672a97f59e2b`
  (src/shim/gate/rhu_shimtest.c; -DRHU_SHIMTEST_LADDER_SOCKNETINFO)
- `goldens/shim/shimtest-host-debug-event.o`
  `7d60ff2bb52f119a911c2bc6c44f507f1c6f9c07d94ce2bea061e30a03879a9c`
  (src/shim/gate/rhu_shimtest.c; -DRHU_SHIMTEST_LADDER_HOST -DRHU_SHIMTEST_HOST_DEBUG_EVENT)
- `goldens/shim/shim-core-break-stack-true-bounds.o`
  `067bf5b40c0f751c0044f3f8998240b3f5883ea3e2fc7197b9a8a2b195c9191c`
  (src/shim/rhu_shim.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_STACK_TRUE_BOUNDS)
- `goldens/shim/shim-core-break-stack-true-bounds-an505.o`
  `8ee03d62a44983b7079ad95d449c974c8ada7f47299847a66b778f66b01dc120`
  (src/shim/rhu_shim.c; -DMGC_HOST_ROW_AN505 -DRHU_SHIM_BREAK_STACK_TRUE_BOUNDS; the Cortex-M33 class flag set)
- `goldens/shim/shim-core-break-pool-max-decimal.o`
  `74757725677c4edac61fa3f6cc999873ca3e9b6f9b00c1b859a9e388df8d5cff`
  (src/shim/rhu_shim.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_POOL_MAX_DECIMAL)
- `goldens/shim/shim-sysnative-break-etc-root-absent.o`
  `1db176187134ab4095ff8ff7530fe2f232d137c952769e0fd6dcbe73db6c2f80`
  (src/shim/rhu_sysnative.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_ETC_ROOT_ABSENT)
- `goldens/shim/shim-sysnative-break-xpm-owner-width.o`
  `946b25a3c3eeb06f2b4c80b5f59794ddc0cf560efd5cd7eac7407c9d1c3adedc`
  (src/shim/rhu_sysnative.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_XPM_OWNER_WIDTH)
- `goldens/shim/shim-sysnative-break-store-identity-block.o`
  `9f5250592db20d90630b9e9887f83ffff920186c66f5e6c885dc52aa517aba02`
  (src/shim/rhu_sysnative.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_STORE_IDENTITY_BLOCK)
- `goldens/shim/shim-sysnative-break-store-dir-open.o`
  `95905aa44616d0746c5c27fa5721e9cebf428f056e678735c29832e32f50e3cd`
  (src/shim/rhu_sysnative.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_STORE_DIR_OPEN)
- `goldens/shim/shim-sysnative-break-store-at-root.o`
  `878d33ce54a4a5bd74b2ca9858570f397d61211d75d5b410f7178cbd0b1a88c1`
  (src/shim/rhu_sysnative.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_STORE_AT_ROOT)
- `goldens/shim/shim-sysnative-break-store-dir-mode.o`
  `39ce0efe96262bb8fb9bb12dc864465990057708421a7973011d1193fc37eb08`
  (src/shim/rhu_sysnative.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_STORE_DIR_MODE)
- `goldens/shim/shim-sysnative-break-store-dir-flock.o`
  `e5ddebb8d5667d8fbea3496c41326b2d3b310d7c79a819337f3440a6352436a9`
  (src/shim/rhu_sysnative.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_STORE_DIR_FLOCK)
- `goldens/shim/shim-sysnative-break-store-map-copy.o`
  `3f241c6ad3b1429a513e6af48b2a7d6d8da58e8586113b34f25999fabbb468b8`
  (src/shim/rhu_sysnative.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_STORE_MAP_COPY)
- `goldens/shim/shim-sysnative-break-store-unmap.o`
  `2216455f3fd9c227097d718d5bf16cb93504b008d14486933919b5d0ec421c7e`
  (src/shim/rhu_sysnative.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_STORE_UNMAP)
- `goldens/shim/shim-sysnative-break-store-page-pad.o`
  `76f001f0823154ab4ef01b0f841f5c7c7625e5cbacf9568995f5e496594892a8`
  (src/shim/rhu_sysnative.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_STORE_PAGE_PAD)
- `goldens/shim/shim-pwmfs-rows.o`
  `4ff552d575a3759f6ff00c7f00ee7bd10270978733a7a6e32fe000eb17f1640b`
  (src/shim/rhu_pwmfs.c; -DMGC_HOST_ROW_AN500 -Drhu_pwm_count=rows_pwm_count -Drhu_pwm_line=rows_pwm_line -Drhu_pwm_claim=rows_pwm_claim -Drhu_pwm_set=rows_pwm_set -Drhu_pwm_period=rows_pwm_period -Drhu_pwm_enable=rows_pwm_enable -Drhu_pwm_release=rows_pwm_release -Drhu_pwm_info=rows_pwm_info -Drhu_shim_log=rows_shim_log)
- `goldens/shim/shim-pwmfs-rows-break-names-at-64.o`
  `852f43eb91ced86a6868ccdeb4cc3cc7f6fb5a25bf3d4f26548195ee955e1900`
  (src/shim/rhu_pwmfs.c; -DMGC_HOST_ROW_AN500 -Drhu_pwm_count=rows_pwm_count -Drhu_pwm_line=rows_pwm_line -Drhu_pwm_claim=rows_pwm_claim -Drhu_pwm_set=rows_pwm_set -Drhu_pwm_period=rows_pwm_period -Drhu_pwm_enable=rows_pwm_enable -Drhu_pwm_release=rows_pwm_release -Drhu_pwm_info=rows_pwm_info -Drhu_shim_log=rows_shim_log -DRHU_SHIM_BREAK_PWMFS_NAMES_AT_64)
- `goldens/shim/shimtest-pwmrows-f769.o`
  `ef68639c12ad06312bc387ecb6bd45529b6bee536df62d884eb552e0d60f865f`
  (src/shim/gate/rhu_shimtest.c; -DRHU_SHIMTEST_LADDER_PWMROWS -DRHU_SHIMTEST_PWMROWS_F769)
- `goldens/shim/shimtest-pwmrows-u5a5.o`
  `0d5296fbbc0c57e179703d962cadb90b50fc31b6c2044c3695e1573688484951`
  (src/shim/gate/rhu_shimtest.c; -DRHU_SHIMTEST_LADDER_PWMROWS -DRHU_SHIMTEST_PWMROWS_U5A5)
- `goldens/shim/shim-gpiod-break-table-per-open.o`
  `f630ea02f854256f64c0ab9af81a31c3f1b969dd1c464aecc64a561c24eb9ba0`
  (src/shim/rhu_gpiod.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_GPIOD_TABLE_PER_OPEN)
- `goldens/shim/shim-sysnative-break-etc-missing-erofs.o`
  `cf4ec991c2daea4e9362a599198eb4a9b2021c35f4b503c9beb5a6e46d903e5a`
  (src/shim/rhu_sysnative.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_ETC_MISSING_EROFS)
- `goldens/shim/shim-gpiod-break-pulls-first.o`
  `30b6bf5493092bb80139940085350f6879ae1e1747e8efd35c26f0ff73b6065e`
  (src/shim/rhu_gpiod.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_GPIOD_PULLS_FIRST)
- `goldens/shim/shimtest-core.golden.txt`
  `43566eb524ed5bb9de88315034abb39f2e44391ddf926b8c9b708c601b69a104`
  (the payload stream of the shim.core image (class bytes))
- `goldens/shim/shimtest-fdmap.golden.txt`
  `4001ee504183876a4bfefad2c68f414ab77f528955d5605071c5184bc8cd70e1`
  (the payload stream of the shim.fdmap image (class bytes))
- `goldens/shim/shimtest-activation.golden.txt`
  `4988c244a7e5d35269a432e99103e9d97fe3b016d5e6b111c878785a1ad8b461`
  (the payload stream of the shim.activation image (class bytes))
- `goldens/shim/shimtest-tls.golden.txt`
  `1684cbccbea60fc52822d3389034ef541b72f63d6b33235420953694826e562b`
  (the payload stream of the shim.tls image (class bytes))
- `goldens/shim/shimtest-host.golden.txt`
  `270a8a37e95cd68c8b94bb99133bc90a79be8db68e4f96d8adab3947022eb06d`
  (the payload stream of the shim.host image (class bytes))
- `goldens/shim/shimtest-busdev.golden.txt`
  `86958b62198f75fcaf04c05dafc2d7bbc32d3e9cd467e645056e43e507300759`
  (the payload stream of the shim.busdev image (class bytes))
- `goldens/shim/shimtest-gpiod.golden.txt`
  `8e24545a919112773a51c42878ff9414e50be293bd3aa22b5c53ec2fdb6d078b`
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
- `goldens/shim/shimtest-sdfs.golden.txt`
  `228bc4287d480bdb710f81874eeccf9a3932b118af5da1a5debb59bd69cbd9a8`
  (the payload stream of the shim.sdfs image (class bytes))
- `goldens/shim/shimtest-sockaddr.golden.txt`
  `4b785b15f337bff59eed93f28eeb57dfcffc30188bf60f1a0f97a226ead859a0`
  (the payload stream of the shim.sock-addr image (class bytes))
- `goldens/shim/shimtest-sockerrors.golden.txt`
  `3969fdfcebd507490917ebad097f6fd34afe5dc6b0bd104db286c641e77f9952`
  (the payload stream of the shim.sock-errors image (class bytes))
- `goldens/shim/shimtest-sockrefusals.golden.txt`
  `c01aaf952a089c634e26e3585dcd8564524dc1c3c687357fd5973ecc51033fee`
  (the payload stream of the shim.sock-refusals image (class bytes))
- `goldens/shim/shimtest-sockstream.golden.txt`
  `11c4ba2dbccc97d7a236b7afb6f97259cc53d34c0e73c8f4673d7713a1309cb1`
  (the payload stream of the shim.sock-stream image (class bytes))
- `goldens/shim/shimtest-socklisten.golden.txt`
  `11a4f00890752b72bb73bcdf5eb5b9368ffdb2f847f2bd82b4326415e3f57302`
  (the payload stream of the shim.sock-listen image (class bytes))
- `goldens/shim/shimtest-sockdatagram.golden.txt`
  `77e53072b2e7ae2227848dee28178469049055a689fcb7ff8353f974627e509e`
  (the payload stream of the shim.sock-datagram image (class bytes))
- `goldens/shim/shimtest-sockoptions.golden.txt`
  `64391ae5166b65e80c594f74e39b585db6b4dea12586cbadfcedfe7ed9ac921a`
  (the payload stream of the shim.sock-options image (class bytes))
- `goldens/shim/shimtest-sockpoll.golden.txt`
  `ec5c23dceade8496d183f4cfedc9ae0840f90e869cf9c9e49e0a76dbeb8c4537`
  (the payload stream of the shim.sock-poll image (class bytes))
- `goldens/shim/shimtest-sockport.golden.txt`
  `ef3936185e1a08f8789931ffd413f142915cd7aa546cebcb51415100ea7ef627`
  (the payload stream of the shim.sock-port image (class bytes))
- `goldens/shim/shimtest-sockresolve.golden.txt`
  `1af6b9ebeb5832ac76a0a5cbb5d7b3d8dee22753d918e13ec4222215dc3e0e25`
  (the payload stream of the shim.sock-resolve image (class bytes))
- `goldens/shim/shimtest-socknetinfo.golden.txt`
  `6d89377cc37eab64e2f4c1701d3a0baf2f40c9cbbce5d5263d7762415aaad64a`
  (the payload stream of the shim.sock-netinfo image (class bytes))
- `goldens/shim/shimtest-pwmrows-f769.golden.txt`
  `21d3bed8ed48e917f0efdd31f4bd3caa9f6cf1bceef0590bd55b773fd885ff2f`
  (the payload stream of the shim.pwm-rows-f769 image (class bytes))
- `goldens/shim/shimtest-pwmrows-u5a5.golden.txt`
  `e138ed44ba59fc47d1257f5b99c29c44b8aa976ed20277026595bb4b1896f938`
  (the payload stream of the shim.pwm-rows-u5a5 image (class bytes))
- `goldens/shim/pal-surface.txt`
  `0dafceea130de8424523c82da48a8e0193f27af742242271c856e8db55cbf71d`
  (the P/Invoke-facing surface expectation the shim.pal-surface case reads: the pin, the pack's five archives with their sha256s, one line per surface name with its family and class; GENERATED from the pack's archives and the runtime's headers at the pin, regenerated at a re-pin, never edited)
