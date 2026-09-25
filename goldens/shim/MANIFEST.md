# goldens/shim -- the shim gate's frozen world objects and goldens (MANIFEST)

Copyright (c) 2026 Lamella LLC and the Lamella project authors.
SPDX-License-Identifier: MIT OR Apache-2.0

The shim gate's images link from these FROZEN COMMITTED OBJECTS, and its
positive cases compare the tenant's payload stream against these goldens
(the artifact unit is the bytes from one recorded build; the gate verifies
every file against this manifest -- and this manifest against its in-code
pin -- before the first case, and REFUSES a mismatch: a frozen artifact that
moved is WRONG, never "rebuilt"). The consumer chain is Rust + QEMU only;
nothing here is compiled at gate time.

Derivation is the leg `rhu-gate shim rederive` (the compiler of record,
clang 22.1.8, against the Arm GNU Toolchain 12.2.MPACBTI-Rel1 sysroot; the
README's re-derivation section names the prerequisites): it recompiles every
object below from src/shim as C under the gate's flag set of record (CC_FLAGS
in src/harness/src/gates/shim.rs) plus the per-object defines from the in-code
frozen plan, and byte-asserts each output against these files. A re-freeze
(a source, flag, kit or pin change) updates the objects, the goldens, this
manifest, and the in-code manifest pin together in one reviewed commit.
Goldens are captured by the documented run method (the payload stream of
the positive image, wire bytes kept), never edited.

Compiler of record: clang version 22.1.8 (the LLVM project's 22.1.8
release; the version the runtime archives at the pin were built with),
targeting arm-none-eabi for the Cortex-M7 with hard-float FPv5-D16 -- and,
for the mps2-an505 row's objects and the st-nucleo-u5a5zj-q row's core
object, the Cortex-M33 with the
single-precision FPv5 under the same flag set but the core and FPU words
(the class flag set beside the Cortex-M7's in shim.rs) -- the
kit's C-library headers as its sysroot, driving the C front end (-x c
-std=gnu11) with no implicit floating-point use (-mno-implicit-float): the
activation compatibility adapter's object carries no floating-point instruction, and the
gate pins every object's count.

Frozen at the compile of record for the .NET 11 RC1 pin
(v11.0.0-rc.1.26425.128): the generated contract headers carry the .NET
major and the pin generation in their names (rhu_pal_<area>_net11_rc1.h,
rhu_rt_entry_net11_rc1.h) and were regenerated from the RC1 source with no
value moved; the shim answers the two names the RC1 runtime archives demand
beyond the previous pin's set -- getauxval (no auxiliary vector: 0 with
ENOENT) and realpath (over the RAM file store) -- and the tenant checks
both; the RAM file store keeps a stated contract (a maximum file size with
every position, count and length checked against it in 64 bits; an
object's name and its open descriptors apart; one store lock; a descriptor
found before anything is created or truncated; the access mode checked at
the read boundary; advisory locks per object; paths with a trailing slash
named the same; file times range-checked), each clause a check in the
tenant's file ladder with a negative control built from the store with
that clause's defect restored; the random entries draw cryptographic bytes
through the kernel's secure entropy call in bounded pieces, all or nothing, with a
refusal honored, and every wall-clock answer (the system time, the C
library's real-time clock and time(), the file store's times) reads the
kernel's calendar, each with a negative control built from the shim with
that answer's defect restored; the timed waits honor the clock a
timestamp names -- a boot-clock timestamp is the kernel deadline itself,
a real-time (calendar) timestamp is translated into the boot-clock
domain with every step checked and re-read against the calendar every
slice while the wait is pending, so a calendar set moves the wait -- with
a negative control built from the shim with the clock discarded; the
64-bit atomic operations the runtime's compiler names as library calls
(the runtime archives are built for the core, which has no 64-bit
exclusive-access instruction) are answered under one lock of the futex
family with a full barrier on each side, so every such operation is
atomic against every other and no interrupt is masked; the libgpiod v1 C
API the .NET GPIO library's binding names (twenty-two entry points, with
the binding's own signatures) is presented over the kernel's GPIO calls in
a unit of its own, bound at link time by the SDK's direct P/Invoke of the
library's name, so the library's stock driver runs unchanged, with the
process identity line that driver reads answered by the store as a
synthesized unlinked object released at its last close, and one negative
control built from the unit -- the API generation stated below the bias
flags, which the hosted-runtime gates' suite refuses in every world (the
control's Cortex-M33 twin beside it; the variant whose event wait went
dead after an activation left the plan, inert on the wire: the runtime
suspends a thread blocked outside managed code lazily, at its return,
and no activation reaches the wait during a collection); the unit's
event wait takes the line's word once and re-waits when the kernel's
futex wait reports an activation, as the kernel call's contract says, and the
errno calls carry the raw platform errno the PAL's contract names, the
managed side converting (the errno contract, proven on the wire); the two device-node families the
.NET I2C and SPI drivers open, /dev/i2c-<bus> and /dev/spidev<bus>.<select>
(the node's number the chip's block number as Linux names it), are
presented over the kernel's bus calls in a unit of its own, the core's
open, ioctl and close dispatching to it and bound at link time by the
SDK's direct P/Invoke of the three C-library entry points, so the
drivers run unchanged: the functionality request answered as plain
combined transactions, the combined-transfer request carried into one
kernel transaction with a one-second deadline, the SPI mode, word-size
and rate requests kept per node and carried into every transfer with
the achieved rate stated once per change, every other path and request
refused with its errno and one line, and one negative control built
from the unit whose functionality answer names no combined transfer
(the hosted-runtime gates' suite refuses it in every world, the
control's Cortex-M33 twin beside it); both units keep a stated contract
under concurrent callers: the bus unit reads a caller's memory once (a
receive buffer lying over its own record changes nothing), stores the
functionality word's four bytes and not one more, gives a node's entry
to the requests running on it (a close under a running request answers
at once and the entry goes to no one until that request ends), refuses
the select 4294967295 as a node's name (the number is the kernel's
no-select value) and honors a lock that fails; the GPIO unit makes each
open its own chip object with one holder per line across them, keeps
its chips, lines and iterators under one lock with every handle checked
by address before it is read, refuses two bias flags together, reads
the living request's bias and leaves a pad's pull as it is under a
flagless request, refuses a timespec field outside its range, reads the
mark before the arm so every edge counted from the arm on is delivered,
reads the count and the pad's level together so an event's kind is the
counted edge's, and its event read waits for an edge as the library's
does; the core adopts a zero-filled mutex or condition slot by one
compare-and-swap, so two first users at once are both served; each
unit's ladder runs in this gate from a native caller's side, on the
emulator row's bus models and virtual lines, with one negative control
per restored defect (the core's one, the bus unit's four, the GPIO
unit's eight), the four whose defect is a race holding the restored
window open, blocked on the kernel's futex, until the second thread's
step is seen (that thread runs one step below the ladder's, so it
cannot step before the window is open), so each is refused at a line
of its own on every run on any host, and every ladder line one console
write, so no verdict line can be torn; the core reads the program's
thread stack size as a weak link-time symbol, the row's default when
absent; the host states hosting-API contract
generation 34, the generation of the kernel's bus calls; the mps2-an505 row's five link objects are the an500 row's
sources and defines compiled for the Cortex-M33 (the row's fields equal
the an500's, so the class flag set is the one variable), each carrying no
double-precision instruction; the st-nucleo-u5a5zj-q row's core object is
the core's source under that row's own selector (the core writes the
runtime's hard-limit row from the row's managed region, which differs from
the mps2-an505 row's; the row shares that row's other objects, which no
differing field reaches, and the rederive leg ties every board-table row's
objects to the row's own declaration by bytes); the shim answers the pinned runtime pack's
whole P/Invoke-facing native surface (the five class-library families'
export tables, 757 names at this pin): the names the hand-written units
serve over the RAM file store, the kernel's calendar and its secure entropy
call,
and for every name of an area this system does not serve a generated
refusal (nothing printed, nothing trapped; the errno of record where the
convention is errno-style) in the surface object beside the five link
objects, with its Cortex-M33 twin for the an505 row; the expectation the
gate's surface case reads -- generated from the pack's archives and the
runtime's headers at the pin, never typed -- is the tier's text record,
tied to the pin and to every family name the objects define; 61
objects, 7 goldens and 1 record, byte-tied by the rederive
leg at the freeze.

Records (repo-relative path, sha256; source and defines per object):

- `goldens/shim/shim-core.o`
  `5ba30b8bd8b9d112011adf4fe1917af331147b0290d2ec5a4950e991ab6b273d`
  (src/shim/rhu_shim.c; -DMGC_HOST_ROW_AN500)
- `goldens/shim/shim-core-break-sipid.o`
  `1b2c09f9319e0f62123152516d81bc6ef6f161fe993b7c82d8eb6c87e098186a`
  (src/shim/rhu_shim.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_SIPID)
- `goldens/shim/shim-core-break-ctxpc.o`
  `a2b0bfcdb02786ae63aa05f08599a0a0c67e270f99a304bbbbbba41952368107`
  (src/shim/rhu_shim.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_CTXPC)
- `goldens/shim/shim-core-break-tlsoff.o`
  `27e2db56511990dca507b9ae0e13a6074af8c32feb83df43706371e60451493d`
  (src/shim/rhu_shim.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_TLSOFF)
- `goldens/shim/shim-core-break-tlscache.o`
  `efd4c764c9872c8d6b1caf43a9ce163dad1def27cd8c175a75aa757c5e0145df`
  (src/shim/rhu_shim.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_TLSCACHE)
- `goldens/shim/shim-core-break-realtime-wait.o`
  `61e6410de90c25b5133e1a3188070b47d5fb56412f94f001a6e74fe5281e4f5d`
  (src/shim/rhu_shim.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_REALTIME_WAIT)
- `goldens/shim/shim-sysnative.o`
  `ed2ec3ec11d820b6ae8b6b2b39afce46c41d021f1990374700dbe65667be3d52`
  (src/shim/rhu_sysnative.c; -DMGC_HOST_ROW_AN500)
- `goldens/shim/shim-sysnative-break-fdbase.o`
  `7a0e17ff8cac4d7b01a769e5d757446facf118e3e8f4fd6bf9da74d37e19ec54`
  (src/shim/rhu_sysnative.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_FD_BASE)
- `goldens/shim/shim-sysnative-break-store-bounds.o`
  `815282f3c2223ce81f77ed60165bce2677a9b55014531ca20967f49526477edf`
  (src/shim/rhu_sysnative.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_STORE_BOUNDS)
- `goldens/shim/shim-sysnative-break-store-lifetime.o`
  `f8719b6a75b16847561b1d27ac662810931efdcb57fa1def853b4d33280031d8`
  (src/shim/rhu_sysnative.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_STORE_LIFETIME)
- `goldens/shim/shim-sysnative-break-store-lock.o`
  `62e6466abb6e63cdfde6a174ac6badfcafdcad0fa8ffbe83991b3fe95baf165a`
  (src/shim/rhu_sysnative.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_STORE_LOCK)
- `goldens/shim/shim-sysnative-break-store-open-order.o`
  `22561f4e2302e0da231e94c94960d57e986428c9c599326c607203199fc70d9b`
  (src/shim/rhu_sysnative.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_STORE_OPEN_ORDER)
- `goldens/shim/shim-sysnative-break-store-access.o`
  `05103017674da49cfb436500f8b478c6d0a6789737085077a596b559f47c25d7`
  (src/shim/rhu_sysnative.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_STORE_ACCESS)
- `goldens/shim/shim-sysnative-break-store-flock.o`
  `e06a3bb83eca308521ebeba42d918ccb6c772dc99ddfd50c3c55709294d3d57f`
  (src/shim/rhu_sysnative.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_STORE_FLOCK)
- `goldens/shim/shim-sysnative-break-store-slash.o`
  `bdb593cdf9fd60cd5fd8a6f77827065eadd83e285efddf0827686bb41efe801d`
  (src/shim/rhu_sysnative.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_STORE_SLASH)
- `goldens/shim/shim-sysnative-break-store-times.o`
  `ff758422aed213ed39dde7d0f5abeaa7bdf366d6434360e463ba1e53b23ab77b`
  (src/shim/rhu_sysnative.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_STORE_TIMES)
- `goldens/shim/shim-sysnative-break-entropy-refusal.o`
  `309b028e69e49355ca41107ef534b8f6ad97177df6aad84c63494a8a4c1534d9`
  (src/shim/rhu_sysnative.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_ENTROPY_REFUSAL)
- `goldens/shim/shim-sysnative-break-calendar-clock.o`
  `11ee203db125ddb115ddd75715c330d37bf6ae127b2671ff83c5cf74424c439c`
  (src/shim/rhu_sysnative.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_CALENDAR_CLOCK)
- `goldens/shim/shim-libc-glue.o`
  `8ffb50ca37ba98ee58a69b71156472609af785bb1dacd5e68bb2103b59ebe267`
  (src/shim/rhu_libc_glue.c; no extra defines)
- `goldens/shim/shim-host.o`
  `9ced37a77463646fed7e5f49205f969017e322394a2aa1bab6bbd075fbca55d8`
  (src/shim/rhu_host.c; -DMGC_HOST_ROW_AN500)
- `goldens/shim/shim-host-break-dataverify.o`
  `ae2935849b2e8c5ca26629b6784637052e28fb29f12c6e2065511d0152e72a6a`
  (src/shim/rhu_host.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_DATAVERIFY)
- `goldens/shim/shim-stdio-sym.o`
  `8428ce33a5f364f458b0ccaa949e3398470a9b05500810c6a62109731033a28d`
  (src/shim/rhu_stdio_sym.c; no extra defines)
- `goldens/shim/shimtest-core.o`
  `2a556e250c5a7ff87aa8ce17e2840ba649fad0df30502c70f64e0f812bcf0336`
  (src/shim/gate/rhu_shimtest.c; -DRHU_SHIMTEST_LADDER_CORE)
- `goldens/shim/shimtest-core-break-errno.o`
  `0d66220fe58d41762bd5826dd55203a80e0d7bc4b56193633f6791e59181b9e1`
  (src/shim/gate/rhu_shimtest.c; -DRHU_SHIMTEST_LADDER_CORE -DRHU_SHIMTEST_BREAK_ERRNO)
- `goldens/shim/shimtest-core-break-canary.o`
  `418c6023278b2621bf23438a51da67f5eb45d4590b8195d357d150e6b280b6cd`
  (src/shim/gate/rhu_shimtest.c; -DRHU_SHIMTEST_LADDER_CORE -DRHU_SHIMTEST_BREAK_CANARY)
- `goldens/shim/shimtest-fdmap.o`
  `0153ae74383bbde1ff5f504191459053f8d38e9c246306847f3e27d20f3e7ebd`
  (src/shim/gate/rhu_shimtest.c; -DRHU_SHIMTEST_LADDER_FDMAP)
- `goldens/shim/shimtest-activation.o`
  `92d5baff843aaee77ba7430ffe1afdede520f684421e668819d458a70ac464e7`
  (src/shim/gate/rhu_shimtest.c; -DRHU_SHIMTEST_LADDER_ACTIVATION)
- `goldens/shim/shimtest-tls.o`
  `10a30bc83fc94b99ae70fda477719731fc61abc6dec2fb665e5b1f5133bc1786`
  (src/shim/gate/rhu_shimtest.c; -DRHU_SHIMTEST_LADDER_TLS)
- `goldens/shim/shimtest-host.o`
  `e5cd2e077173cfb26f7751b465e2660595162a03b1971de06ca13879411a8152`
  (src/shim/gate/rhu_shimtest.c; -DRHU_SHIMTEST_LADDER_HOST)
- `goldens/shim/shim-core-f769.o`
  `f371b02b04e39d0ef6096405a6a497cfaf57f8dbf7f7b87b303fd222f3bbb9c2`
  (src/shim/rhu_shim.c; -DMGC_HOST_ROW_F769)
- `goldens/shim/shim-core-an505.o`
  `a69a0c11cf45340049942bd9caac2816707313a8e000f8648a7665f4b2ab7804`
  (src/shim/rhu_shim.c; -DMGC_HOST_ROW_AN505; the Cortex-M33 class flag set)
- `goldens/shim/shim-sysnative-an505.o`
  `c15da32c842616ae2db9910c2a1723ed28f9dc0e9d498fe9b8a7637c36a3ade1`
  (src/shim/rhu_sysnative.c; -DMGC_HOST_ROW_AN505; the Cortex-M33 class flag set)
- `goldens/shim/shim-libc-glue-an505.o`
  `586fde3e983c35ec2d724ee0b5b692ba61a64552e917a9c6b7d519e69fe609da`
  (src/shim/rhu_libc_glue.c; no extra defines; the Cortex-M33 class flag set)
- `goldens/shim/shim-host-an505.o`
  `593fa6340dda50858309b081d2ca4c24b426ad6cc290f3994dd39ff2c7aa5b5c`
  (src/shim/rhu_host.c; -DMGC_HOST_ROW_AN505; the Cortex-M33 class flag set)
- `goldens/shim/shim-stdio-sym-an505.o`
  `49a42ffaf600fe1315997337de4e9457f6454072c823f8ce3be0d7e4abfb3bb9`
  (src/shim/rhu_stdio_sym.c; no extra defines; the Cortex-M33 class flag set)
- `goldens/shim/shim-core-u5a5.o`
  `e213aec445b89ef41901403f17ee4935d56c7c67362f3344915b6d770c8d8268`
  (src/shim/rhu_shim.c; -DMGC_HOST_ROW_U5A5; the Cortex-M33 class flag set)
- `goldens/shim/shim-pal-surface.o`
  `ba588dc52d48b2df933c27836f398783917a371933d62be18c10927f3842750c`
  (src/shim/rhu_pal_surface_net11_rc1.c; -DMGC_HOST_ROW_AN500)
- `goldens/shim/shim-pal-surface-an505.o`
  `3e49d989d3b8626b510b1b529fe043a26b9195203feb6b6e7045cb48efe73499`
  (src/shim/rhu_pal_surface_net11_rc1.c; -DMGC_HOST_ROW_AN505; the Cortex-M33 class flag set)
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
  `6fe2cd3d9757cb2c579a36cb491f555552cf6ad7aa9b3dcc120beec10de979af`
  (src/shim/rhu_busdev.c; -DMGC_HOST_ROW_AN500)
- `goldens/shim/shim-busdev-an505.o`
  `54e28680cc19f8ead34e059a9077ef7cce3b607a96019fb6a69b6204afc78332`
  (src/shim/rhu_busdev.c; -DMGC_HOST_ROW_AN505; the Cortex-M33 class flag set)
- `goldens/shim/shim-busdev-break-funcs.o`
  `78dce953acac45c12555e2ec1c5d6ea5b23d0857b07f93b9f5dfa056095e82e7`
  (src/shim/rhu_busdev.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_BUSDEV_FUNCS)
- `goldens/shim/shim-busdev-break-funcs-an505.o`
  `99995d427f4d35d677e01788f2a3bd3dc497710ecd569d1c5ff2e9e2bff984af`
  (src/shim/rhu_busdev.c; -DMGC_HOST_ROW_AN505 -DRHU_SHIM_BREAK_BUSDEV_FUNCS; the Cortex-M33 class flag set)
- `goldens/shim/shimtest-busdev.o`
  `0bb0427ce76989edeb53a6fdbc719c985188bfebdd6dc88db08b98e609c5be45`
  (src/shim/gate/rhu_shimtest.c; -DRHU_SHIMTEST_LADDER_BUSDEV)
- `goldens/shim/shimtest-gpiod.o`
  `f53f2e0a740b4d56e1d1066c581005d32195d28297093a6a9ea77aed053fbbd0`
  (src/shim/gate/rhu_shimtest.c; -DRHU_SHIMTEST_LADDER_GPIOD)
- `goldens/shim/shim-core-break-mutex-adopt.o`
  `5078ed46c9b9c56538fa96e668af4b2267471473bbe03dc208e4bdb41c490f4a`
  (src/shim/rhu_shim.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_MUTEX_ADOPT)
- `goldens/shim/shim-busdev-break-select-name.o`
  `b785c54c7a385ba21d14ce6486bc983fcb818f37b89821853acb72cfdb12412a`
  (src/shim/rhu_busdev.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_BUSDEV_SELECT_NAME)
- `goldens/shim/shim-busdev-break-funcs-wide.o`
  `bd5e6a059e5cdb1b788cd874347340edbede2ad481efa6592cb5f2f0d5ad8359`
  (src/shim/rhu_busdev.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_BUSDEV_FUNCS_WIDE)
- `goldens/shim/shim-busdev-break-reread.o`
  `982cdf9d4c63be44bc8945eaa7d4a287a997374d47335d87d47496c5fabe54d3`
  (src/shim/rhu_busdev.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_BUSDEV_REREAD)
- `goldens/shim/shim-busdev-break-unowned.o`
  `86acc978090802ada911d8a06a3d4c0d64362bab3812c3899c6f95dc88de57f8`
  (src/shim/rhu_busdev.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_BUSDEV_UNOWNED)
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
- `goldens/shim/shimtest-core.golden.txt`
  `c0f7f0970ef2739421472e21986887c87214476037adfe7082c63dbc21ac07d8`
  (the payload stream of the shim.core image (class bytes))
- `goldens/shim/shimtest-fdmap.golden.txt`
  `28b974465329510b6aa029d59e62511f9502c7b30d10e4c4e088d967aec15195`
  (the payload stream of the shim.fdmap image (class bytes))
- `goldens/shim/shimtest-activation.golden.txt`
  `33b8276c29670c9e1472b03ce863f5b5e1d1215afb384131a2db9039971455cc`
  (the payload stream of the shim.activation image (class bytes))
- `goldens/shim/shimtest-tls.golden.txt`
  `1684cbccbea60fc52822d3389034ef541b72f63d6b33235420953694826e562b`
  (the payload stream of the shim.tls image (class bytes))
- `goldens/shim/shimtest-host.golden.txt`
  `aeb710f3cdc677e9c5ba29a1c4a432fe2389fda884b05a54dfcd2d910191cfb5`
  (the payload stream of the shim.host image (class bytes))
- `goldens/shim/shimtest-busdev.golden.txt`
  `299f9040e8ce2767f981a0dae2dc74021e2e86cbf7bbc6b503888aec4d48cd35`
  (the payload stream of the shim.busdev image (class bytes))
- `goldens/shim/shimtest-gpiod.golden.txt`
  `b3dc58194fa5e29f608a4520e2dd1cb81af86931f6ac8aeffe6ff6d0c241e70d`
  (the payload stream of the shim.gpiod image (class bytes))
- `goldens/shim/pal-surface.txt`
  `1835f4cb31b605811c3d8e17356510a5e79a200849352ad540ce9fcf14a4020e`
  (the P/Invoke-facing surface expectation the shim.pal-surface case reads: the pin, the pack's five archives with their sha256s, one line per surface name with its family and class; GENERATED from the pack's archives and the runtime's headers at the pin, regenerated at a re-pin, never edited)
