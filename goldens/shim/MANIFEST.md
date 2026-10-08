# goldens/shim -- the shim gate's frozen world objects and goldens (MANIFEST)

Copyright (c) 2026 Lamella LLC.
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
for the mps2-an505 row's objects and the RP2350 rows' core and
system-call objects, the Cortex-M33 with the
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
kernel transaction with a one-second deadline, the I2C bus claimed at the
first transfer at 100 kHz or earlier at a served rate by this system's one
rate request on the node (0x0740, a word in hertz read back with the rate
the bus runs at; an unserved rate refused ENOTSUP before the bus's state, a
second rate EBUSY with the running rate read back, the one departure from
the Linux surface, which has no clock setting), the SPI mode, word-size
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
per restored defect (the core's one, the bus unit's five, the GPIO
unit's eight), the four whose defect is a race holding the restored
window open, blocked on the kernel's futex, until the second thread's
step is seen (that thread runs one step below the ladder's, so it
cannot step before the window is open), so each is refused at a line
of its own on every run on any host, and every ladder line one console
write, so no verdict line can be torn; the core reads the program's
thread stack size as a weak link-time symbol, the row's default when
absent; the host states hosting-API contract
generation 35, the generation of the kernel's PWM and ADC calls; the mps2-an505 row's five link objects are the an500 row's
sources and defines compiled for the Cortex-M33 (the row's fields equal
the an500's, so the class flag set is the one variable), each carrying no
double-precision instruction; the core writes the runtime's
hard-limit row at initialization from the system memory free then, less
what the board row keeps for later draws (the frozen object heap and the
system memory left unallocated), and the system-call unit serves the
runtime's frozen-object reservation from the row's pool, accepts a
commit that starts inside the pool, refuses one that starts at or past
its end, and answers the C library's question of what memory backs a
mapping with the pool's pages and none of the reservation past them, so
the collector stops a program before a byte is written past a pool
smaller than the runtime's commit; the two objects a differing row
field reaches are frozen per row class -- the st-stm32f769i-disco row's
core and system-call objects for the Cortex-M7, and for the Cortex-M33
one pair for the four RP2350 rows (the Raspberry Pi Pico 2 and Pico 2
W, the Pimoroni Pico Plus 2 and Pico Plus 2 W declare one mini row each
with the same fields), frozen under the Pico 2's selector and named for
the chip -- while the st-nucleo-u5a5zj-q row keeps what the mps2-an505
row keeps and names that row's objects throughout, and the rederive leg
ties every board-table row's objects to the row's own declaration by
bytes; the frozen object heap's ladder runs in this gate on the small
row's objects, with one negative control built from the system-call
unit whose backing answer says every page is memory; the shim answers the pinned runtime pack's
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
tied to the pin and to every family name the objects define; 66
objects, 8 goldens and 1 record, byte-tied by the rederive
leg at the freeze.

Records (repo-relative path, sha256; source and defines per object):

- `goldens/shim/shim-core.o`
  `3c02455e773d89d35455cf1ac36d37972d917c73fadbf5eaeef20c517bf21628`
  (src/shim/rhu_shim.c; -DMGC_HOST_ROW_AN500)
- `goldens/shim/shim-core-break-sipid.o`
  `da3e1baf5c1aa008d4d365443a4d875365964532a64968e0b818be114c7e48bf`
  (src/shim/rhu_shim.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_SIPID)
- `goldens/shim/shim-core-break-ctxpc.o`
  `e93413d1bb4575c7b5306110f80f9c3ed357eef64c7dc381469c8a7dd66f0dd3`
  (src/shim/rhu_shim.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_CTXPC)
- `goldens/shim/shim-core-break-tlsoff.o`
  `dc010f294088cfb7be281b456086d2265761a0ac50bbee067841ee9da7988275`
  (src/shim/rhu_shim.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_TLSOFF)
- `goldens/shim/shim-core-break-tlscache.o`
  `9348006e551a353992063e57003556708d92d1f02913a4d8159064358df7c61d`
  (src/shim/rhu_shim.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_TLSCACHE)
- `goldens/shim/shim-core-break-realtime-wait.o`
  `9366f43d0f3db565e133b04ba2662598ca1650954fe2a4105242a562bab022af`
  (src/shim/rhu_shim.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_REALTIME_WAIT)
- `goldens/shim/shim-sysnative.o`
  `a424a822c6b9acbfbb6d2a2df1b24c706491b9377711cca8511785366cf4a118`
  (src/shim/rhu_sysnative.c; -DMGC_HOST_ROW_AN500)
- `goldens/shim/shim-sysnative-break-fdbase.o`
  `4bc7a2e7762cd408f78338e725a29f6162a226b0b45d158f390f9167c23f08dc`
  (src/shim/rhu_sysnative.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_FD_BASE)
- `goldens/shim/shim-sysnative-break-store-bounds.o`
  `080f8a83dae5dc4b94ccf12c3c36a232d7d302485bf84b4b1c40a5e2af54cb3e`
  (src/shim/rhu_sysnative.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_STORE_BOUNDS)
- `goldens/shim/shim-sysnative-break-store-lifetime.o`
  `a30104ddd49dcc70f589928588a5baabb1570b6c5f41f945ab902c58b9364874`
  (src/shim/rhu_sysnative.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_STORE_LIFETIME)
- `goldens/shim/shim-sysnative-break-store-lock.o`
  `269f6f49b0f154cc9ec3a98bc906d5f1bca53758e3d4b9dbddb835afaab9a636`
  (src/shim/rhu_sysnative.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_STORE_LOCK)
- `goldens/shim/shim-sysnative-break-store-open-order.o`
  `95dd22d5fe1ec8807efd979f26f9752228b27d907ac80a4ba73f8d72c4c53c81`
  (src/shim/rhu_sysnative.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_STORE_OPEN_ORDER)
- `goldens/shim/shim-sysnative-break-store-access.o`
  `d039f9cc8f862af92e2fd02ef1ae0363494015941e95dca22bd918b6f8012144`
  (src/shim/rhu_sysnative.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_STORE_ACCESS)
- `goldens/shim/shim-sysnative-break-store-flock.o`
  `f55148b0bedbf909422cce8f673c4cde27fcfac7a8120c315856b896b2686009`
  (src/shim/rhu_sysnative.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_STORE_FLOCK)
- `goldens/shim/shim-sysnative-break-store-slash.o`
  `0f6ca6ad1e241cf5c6e32fc539eae057e8076aefa35b75a3953d8273321f1746`
  (src/shim/rhu_sysnative.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_STORE_SLASH)
- `goldens/shim/shim-sysnative-break-store-times.o`
  `dc2392365b926670cd66a4573a6ec307eec4875219ebdc068b861e94e464e5ec`
  (src/shim/rhu_sysnative.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_STORE_TIMES)
- `goldens/shim/shim-sysnative-break-entropy-refusal.o`
  `445688618f696bb7e4a57ef44ce5b137c4439462fb05d56ead7417f5139b3ecb`
  (src/shim/rhu_sysnative.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_ENTROPY_REFUSAL)
- `goldens/shim/shim-sysnative-break-calendar-clock.o`
  `ec4d246a4c195b3ca6c9d2947ca9a8f77fd4560d623f471143dab84d260fdf14`
  (src/shim/rhu_sysnative.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_CALENDAR_CLOCK)
- `goldens/shim/shim-libc-glue.o`
  `8ffb50ca37ba98ee58a69b71156472609af785bb1dacd5e68bb2103b59ebe267`
  (src/shim/rhu_libc_glue.c; no extra defines)
- `goldens/shim/shim-host.o`
  `dbf0b8c2ca89abc8156a223cafc588902ff76aebafc8de40528817e29ae48508`
  (src/shim/rhu_host.c; -DMGC_HOST_ROW_AN500)
- `goldens/shim/shim-host-break-dataverify.o`
  `d959c3265c78415734de806e69bb9f0f5679b9ce0212cc17ed378aee2b6d3656`
  (src/shim/rhu_host.c; -DMGC_HOST_ROW_AN500 -DRHU_SHIM_BREAK_DATAVERIFY)
- `goldens/shim/shim-stdio-sym.o`
  `8428ce33a5f364f458b0ccaa949e3398470a9b05500810c6a62109731033a28d`
  (src/shim/rhu_stdio_sym.c; no extra defines)
- `goldens/shim/shimtest-core.o`
  `93aa630e35e2f091e006c2d17d34733ab85c7d28a90625258aa218072518be35`
  (src/shim/gate/rhu_shimtest.c; -DRHU_SHIMTEST_LADDER_CORE)
- `goldens/shim/shimtest-core-break-errno.o`
  `2bcd0f8c71c41a40985dc17aed7a1146dd17ac0f0a430a1a2010882dc81740dc`
  (src/shim/gate/rhu_shimtest.c; -DRHU_SHIMTEST_LADDER_CORE -DRHU_SHIMTEST_BREAK_ERRNO)
- `goldens/shim/shimtest-core-break-canary.o`
  `e32bd3ac87cea42c11d43afc72ff0cb8b44e1f91b52200653372d85b951ee4dd`
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
  `20004cb9cca22b7708fc00a6b1eeda4e02c9f7fcb09e92612200298a3f13f3ac`
  (src/shim/rhu_shim.c; -DMGC_HOST_ROW_F769)
- `goldens/shim/shim-core-an505.o`
  `6b02e2e57cd04350c22d0d3ff009c5664ae8c6fc9816cf410d49513004fbd6b4`
  (src/shim/rhu_shim.c; -DMGC_HOST_ROW_AN505; the Cortex-M33 class flag set)
- `goldens/shim/shim-sysnative-an505.o`
  `aee9475dbfdb3c728ef148d82ff6543f671e8ff21219432a44e7df7f51197f83`
  (src/shim/rhu_sysnative.c; -DMGC_HOST_ROW_AN505; the Cortex-M33 class flag set)
- `goldens/shim/shim-libc-glue-an505.o`
  `586fde3e983c35ec2d724ee0b5b692ba61a64552e917a9c6b7d519e69fe609da`
  (src/shim/rhu_libc_glue.c; no extra defines; the Cortex-M33 class flag set)
- `goldens/shim/shim-host-an505.o`
  `c624c24c79b30375f334965aec4cd7ac01f818f5e3c0c5c555988858f72b9cea`
  (src/shim/rhu_host.c; -DMGC_HOST_ROW_AN505; the Cortex-M33 class flag set)
- `goldens/shim/shim-stdio-sym-an505.o`
  `49a42ffaf600fe1315997337de4e9457f6454072c823f8ce3be0d7e4abfb3bb9`
  (src/shim/rhu_stdio_sym.c; no extra defines; the Cortex-M33 class flag set)
- `goldens/shim/shim-core-rp2350.o`
  `c15a8f346b81df925b72b0bfe959884f334c39aa779f5908fed5f403bd1f4566`
  (src/shim/rhu_shim.c; -DMGC_HOST_ROW_PICO2; the Cortex-M33 class flag set)
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
  `4857f68e1bd503e0a56129d00951a9dd7dc942b80eb4a109b12f1bbab9e5fc1a`
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
  `14cbfbbdcd4940f56c3040f1ed13e65002b349974a49bf4d656bd6dc8c0ad29f`
  (src/shim/rhu_sysnative.c; -DMGC_HOST_ROW_F769)
- `goldens/shim/shim-sysnative-rp2350.o`
  `d46e6754fefdb25261e1c050a89861ce124061c2a7f091da2370ed8a40740163`
  (src/shim/rhu_sysnative.c; -DMGC_HOST_ROW_PICO2; the Cortex-M33 class flag set)
- `goldens/shim/shim-sysnative-f769-break-frozen-backed.o`
  `14a55eaf8222aa2da211916b2e11335465c0d8c5b41b12c012fbcf6ec3f97728`
  (src/shim/rhu_sysnative.c; -DMGC_HOST_ROW_F769 -DRHU_SHIM_BREAK_FROZEN_BACKED)
- `goldens/shim/shimtest-frozen.o`
  `e65c9dc47bc0d2c73995958f22287a0b203a35f2fecd0a0af6bbf72e0f9e0bd1`
  (src/shim/gate/rhu_shimtest.c; -DRHU_SHIMTEST_LADDER_FROZEN)
- `goldens/shim/shimtest-core.golden.txt`
  `a17c22635ea7ce7f65b6ed209ad8c72025af8604597420ac183387ad37dd8ab0`
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
  `30ae1b223fb12627bfa0b7db8775148ace656108829ae21f13da434cf797230f`
  (the payload stream of the shim.busdev image (class bytes))
- `goldens/shim/shimtest-gpiod.golden.txt`
  `b3dc58194fa5e29f608a4520e2dd1cb81af86931f6ac8aeffe6ff6d0c241e70d`
  (the payload stream of the shim.gpiod image (class bytes))
- `goldens/shim/shimtest-frozen.golden.txt`
  `5e0510fc153b7a6a62c57dd88e581a1f9ff89365cbddc213a20995b79893725c`
  (the payload stream of the shim.frozen-pool image (class bytes))
- `goldens/shim/pal-surface.txt`
  `1835f4cb31b605811c3d8e17356510a5e79a200849352ad540ce9fcf14a4020e`
  (the P/Invoke-facing surface expectation the shim.pal-surface case reads: the pin, the pack's five archives with their sha256s, one line per surface name with its family and class; GENERATED from the pack's archives and the runtime's headers at the pin, regenerated at a re-pin, never edited)
