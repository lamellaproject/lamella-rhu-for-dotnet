# VENDORED.md -- third-party components vendored into `src/`

Every vendored component has an entry here: the upstream source and pinned
commit, the per-file sha256 recorded at vendoring, and any modification.
Files under `src/vendor/` are never edited: local needs are met in RHU's own
files instead. Changing a vendored file requires a new entry here with
re-recorded hashes.
`src/vendor/**` is `-text` in `.gitattributes`, so these files stay byte-exact
in the working tree and the hashes below hold on any platform.

## src/vendor/ports/ (13 files) -- vendored 2026-10-16

- **Source:** the public `dotnet/dotnet` repository (the VMR, the source the
  official .NET runtime pack names) at commit
  **`820230c386db5b0106c5430b3e5aee92a7826bf5`**, tag
  **`v11.0.0-rc.2.26504.105`**, its `src/runtime` tree -- the .NET runtime
  version RHU builds against. The `dotnet/runtime` tag of the same name
  (commit `474ed963699fc29809015392738a15231aaae2de`) is the label the VMR's
  source manifest gives for its runtime copy; these thirteen files are
  byte-identical in both.
- **Method:** copied with sha256 verification from a read-only clone at that
  commit: bytes taken from the committed blobs (never from a checkout, whose
  line ends a git checkout setting may convert), the hash computed at copy time
  and re-verified against the working-tree file. Equivalent fetch:
  `https://raw.githubusercontent.com/dotnet/dotnet/820230c386db5b0106c5430b3e5aee92a7826bf5/src/runtime/src/native/<file>`.
- **What it is:** the native half of the standard serial port class
  (`System.IO.Ports.SerialPort`): the termios layer (`pal_termios.c`, the
  signal, speed, reset, discard, drain and break entry points), the device
  open, close, read, write, poll and shutdown entry points (`pal_serial.c`)
  and the custom-rate path over the Linux `termios2` request
  (`pal_termios2.c`), with the shared PAL headers they include: the common
  read, write and poll helpers, the errno conversions, the utilities and
  compiler macros, and the two minipal headers. RHU compiles them unchanged,
  through one translation unit of its own (`src/shim/ports/rhu_ports_native.c`)
  over its own configuration and platform headers (`src/shim/ports/`), into
  the shim tier's `shim-ports-native.o` and its Cortex-M33 twin; the C-library
  calls they make land on the shim's serial device unit.
- **Layout:** the upstream layout under `src/runtime/src/native/` kept
  (`libs/System.IO.Ports.Native/`, `libs/Common/`, `minipal/`); consumers
  compile with the three directories on the include path
  (`-Isrc/vendor/ports/libs/System.IO.Ports.Native -Isrc/vendor/ports/libs/Common -Isrc/vendor/ports`)
  after RHU's own `src/shim/ports/`.
- **Files + sha256 at vendoring** (upstream path prefix `src/runtime/src/native/`):
  - `ports/libs/System.IO.Ports.Native/pal_termios.c`
    `1ee821276a32cfb29e3bbb95e08473349e6d033a62bf32984d99c08e01013bfa`
  - `ports/libs/System.IO.Ports.Native/pal_serial.c`
    `c8774eaf0012707235ccaeed47dfadfd2d4cbb1fe242ddf27dfbad1d5f10db8c`
  - `ports/libs/System.IO.Ports.Native/pal_termios2.c`
    `58bc2f1ec2ef6d9a5e209fdd9dd440f8259592fa8215fd4d95a981a73362a0f2`
  - `ports/libs/System.IO.Ports.Native/pal_termios.h`
    `f8230f594c02f1a51989d42f875a110232c7e8e13fd7ecfc8cf07983ba55de0e`
  - `ports/libs/System.IO.Ports.Native/pal_serial.h`
    `e22bcc31b4266a2e7c6f5b1b7c26c637e85b95a5947d11067ffc9d8f59df2adf`
  - `ports/libs/Common/pal_types.h`
    `e15cf8ec12dbec0b4bbb9893d59c2945f7bee9c9fbdc56c35bfd2335096dcc67`
  - `ports/libs/Common/pal_utilities.h`
    `ff5293ca96c9205c4339039bc176b6acadb7a76bac0f0f617112e7d7f1d61439`
  - `ports/libs/Common/pal_compiler.h`
    `e7082aeb359db214ef865341a766177d329e302a72a7bd2befae4da1d38bc458`
  - `ports/libs/Common/pal_io_common.h`
    `0965f8f2d4dfdb20ef4621991f638a7c4c77beea47bf3d4b706683f2e412a494`
  - `ports/libs/Common/pal_networking_common.h`
    `3bdbd1fff74648207eaa4074a95f71b23ce0bb67630fc5961a2ed00b53b925c1`
  - `ports/libs/Common/pal_error_common.h`
    `be83dd99d38b38977816052518abee6d7de186d4cc22c4590b0f394d63369d94`
  - `ports/minipal/utils.h`
    `be1ea60fdb679da85423fe6a4cb704952ac14c80b5fd0ac08b1c694800f7429e`
  - `ports/minipal/types.h`
    `77af8c49134f82f69138e620fcda4f0d5f384fb9db58d69f7e014248160005ff`
- **License:** MIT (.NET Foundation). Every file carries the .NET Foundation MIT
  header in-file; `src/vendor/gcinterface/NOTICE` carries the upstream copyright
  and SPDX identifier for everything under `src/vendor/`.
- **Modifications: none.** These files are byte-identical to upstream; the
  RHU repository's `shim` quality gate re-verifies the hashes above.

## src/vendor/gcinterface/ (7 files) + src/vendor/gcenv/ (3 files) -- vendored 2026-10-16

- **Source:** the public `dotnet/dotnet` repository (the VMR, the source the
  official .NET runtime pack names) at commit
  **`820230c386db5b0106c5430b3e5aee92a7826bf5`**, tag
  **`v11.0.0-rc.2.26504.105`**, its `src/runtime` tree -- the .NET runtime
  version RHU builds against. The `dotnet/runtime` tag of the same name
  (commit `474ed963699fc29809015392738a15231aaae2de`) is the label the VMR's
  source manifest gives for its runtime copy; these ten files are
  byte-identical in both.
- **Method:** copied with sha256 verification from a read-only clone of that
  tag: bytes taken from the committed blobs, the hash computed at copy time and
  re-verified against the working-tree file. Equivalent fetch:
  `https://raw.githubusercontent.com/dotnet/dotnet/820230c386db5b0106c5430b3e5aee92a7826bf5/src/runtime/src/coreclr/gc/<file>`
  (the gcenv files live under `src/coreclr/gc/env/`).
- **What it is:** the runtime-to-GC interface contract -- `IGCHeap`,
  `IGCHandleManager`, `IGCToCLR`, `GC_VersionInfo`, `GcDacVars`, the flat
  `GCToEEInterface` static-class declaration RHU's collector consumes, the
  GC-visible `ObjHeader`/`MethodTable`/`Object`/`ArrayBase` mirrors, and the
  CGCDesc/CGCDescSeries reference-tracing format. Version constants at this
  commit: `GC_INTERFACE_MAJOR_VERSION 5`, `GC_INTERFACE_MINOR_VERSION 9`,
  `EE_INTERFACE_MAJOR_VERSION 5`.
- **Layout:** two include directories (`gcdesc.h` sits with the gcenv set);
  consumers compile with both on the include path
  (`-Isrc/vendor/gcinterface -Isrc/vendor/gcenv`).
- **Files + sha256 at vendoring** (upstream path prefix `src/runtime/src/coreclr/gc/`):
  - `gcinterface/gcinterface.h`
    `2cd7f7a10016533b6c0e318ad60fd018e9c3c507e82ef3b6a9add2c02c13a42d`
  - `gcinterface/gcinterface.ee.h`
    `1b77b32d0ef3f4088bb26930870ca430459f5f6e7ecd8302b88bea03999ccb38`
  - `gcinterface/gcinterface.dac.h`
    `eb9397631446a083f5d12f66a078ebefbaf73519a82a3e78e9f4780ef32931d5`
  - `gcinterface/handletableconstants.h`
    `89b4dd5a9b987abf13fc9759e0ffb6ce3b4834295692d0e0017e170cb94dec5e`
  - `gcinterface/dac_generation_fields.h`
    `353442990bcdfa4154a71f04b26251ed5f6728a9cc4468b46bddd54d49cb49a1`
  - `gcinterface/dac_gcheap_fields.h`
    `3e6852b0454edc122102d1b240d3a49c071a1e8a549828ab2eebce3c7d7a893c`
  - `gcinterface/gcinterface.dacvars.def`
    `2590f968e5671f36e7e8dfa5d063576e9307db269981bd5facf5884daa6ad99d`
  - `gcenv/gcenv.ee.h` (upstream `env/gcenv.ee.h`)
    `b46f2d5d3888ab328f006ca468251b13c9b1cd4163b171b5266dc996cba350ea`
  - `gcenv/gcenv.object.h` (upstream `env/gcenv.object.h`)
    `507884c48576de19e1af06b263644b2fa72cd00d09c955ef9550cdd58f9ab5d3`
  - `gcenv/gcdesc.h`
    `1a6cac22701c96b110c50e1a3385edf5c3fa0fbc32ea4dff0b4001c8616b71ca`
  - `gcinterface/NOTICE` (RHU-authored, not an upstream file)
    `f583e6710e40fc786cb09c5d31a611f69d68e38027127d62c0a6de8f5a2bb5ae`
    It carries the upstream copyright and SPDX identifier for everything
    under `src/vendor/`.
- **License:** MIT (.NET Foundation). Eight of the ten upstream files carry
  the .NET Foundation MIT header in-file; the two `dac_*_fields.h` include
  fragments carry no header upstream, and the repository-level MIT license
  governs them.
- **Modifications: none.** These files are byte-identical to upstream; the
  RHU repository's `gc` quality gate re-verifies the hashes above.

## src/vendor/crates/ (8 crates, 289 files) -- vendored 2026-10-15 (the network doors' stack and its dependencies)

- **What it is:** the network stack smoltcp 0.14.0 the kernel's network doors
  ride on (behind the kernel crate's `net` feature, off by default on
  every row) and its seven dependencies, as crates.io released them:
  `smoltcp-0.14.0`, `bitflags-1.3.2`, `byteorder-1.5.0`, `cfg-if-1.0.5`,
  `hash32-0.3.1`, `heapless-0.9.3`, `managed-0.8.0`, `stable_deref_trait-1.2.1`.
  The host tier (`src/kernel-host`) takes smoltcp alone, the parser its
  `net_math` cases pin the kernel's checksum arithmetic against.
- **Source:** the crates.io release archives, vendored 2026-10-15
  (never a copy from another tree). Each archive's sha256, equal to the
  registry's checksum for the release and to the `package` field of the
  crate's `.cargo-checksum.json`:
  - `smoltcp-0.14.0` (released 2026-08-17, vendored whole) `b6f8b28ad56c6e35524a37dd492af5d1a47e31e1a4d175cd12f89c075f01980f`
  - `bitflags-1.3.2` (released 2021-08-16, vendored whole) `bef38d45163c2f1dde094a7dfd33ccf595c92905c8f8f4fdc18d06fb1037718a`
  - `byteorder-1.5.0` (released 2023-10-06, vendored whole) `1fd0f2584146f6f2ef48085050886acf353beff7305ebd1ae69500e27c67f64b`
  - `cfg-if-1.0.5` (released 2026-09-16, vendored whole) `4e7648175b45a9a48536d676f68d918270699102aa8dab5496df06904c914600`
  - `hash32-0.3.1` (released 2022-08-09, vendored whole) `47d60b12902ba28e2730cd37e95b8c9223af2808df9e902d4df49588d1470606`
  - `heapless-0.9.3` (released 2026-04-30, vendored whole) `25ba4bd83f9415b58b4ed8dc5714c76e626a105be4646c02630ad730ad3b5aa4`
  - `managed-0.8.0` (released 2020-07-04, vendored whole) `0ca88d725a0a943b096803bd34e73a4437208b6077654cc4ecb2947a5f91618d`
  - `stable_deref_trait-1.2.1` (released 2025-10-09, vendored whole) `6ce2be8dc25455e1f91df71bfa12ad37d7af1092ae736f3a6cd0e37bc7810596`
- **Method:** `cargo vendor --versioned-dirs` over the two manifests that take
  the crates (`src/kernel/Cargo.toml`, `src/kernel-host/Cargo.toml`), which
  downloads each release archive, checks it against the registry's checksum
  and unpacks it whole; beside it, every archive fetched a second time by
  hand from the release's address and its sha256 compared with the
  registry's and with the vendored copy's `package` field (all eight equal).
  The per-file sha256 of
  every vendored file is in the crate's `.cargo-checksum.json`, which cargo
  verifies at every build.
  The two crates read the registry through this directory
  and never consult it (`[source.crates-io] replace-with` in each crate's
  `.cargo/config.toml`), so a build is the tree's bytes alone.
- **Features taken (smoltcp):** `medium-ethernet`, `proto-ipv4`, `socket-tcp`,
  `socket-udp`, `socket-dhcpv4`, `socket-dns`; `default-features = false`
  (no `std`, no `alloc`, no logging). The optional dependencies those
  features leave off (`defmt`, `libc`, `log`; heapless's optional set) are
  not vendored.
- **Layout:** `src/vendor/crates/<name>-<version>/`, each crate as released
  (its own license files, README, tests and examples included; nothing
  pruned, so the hashes hold).
- **Licenses, read from each copy:** smoltcp 0BSD (`LICENSE-0BSD.txt`);
  managed 0BSD (`LICENSE-0BSD.txt`); bitflags, cfg-if, heapless, hash32 and
  stable_deref_trait MIT or Apache-2.0 (`LICENSE-MIT`, `LICENSE-APACHE`);
  byteorder Unlicense or MIT (`UNLICENSE`, `LICENSE-MIT`, `COPYING`). RHU
  takes the MIT option wherever a choice is offered. NOTICE carries them.
- **Modifications: none.** Every file is byte-identical to its release
  archive (the `-text` attribute on `src/vendor/**` keeps it so on every
  platform); the port needed no edit. A future edit takes a new entry here
  with re-recorded hashes, as the rule above says.
- **Files + sha256 at vendoring** (289 files: every file under src/vendor/crates as
  cargo vendor wrote it, each crate's `.cargo-checksum.json` included, in the record
  form the gc gate parses and re-verifies at every run; the hashes of the files
  the checksum maps list are the maps' own):
  - `crates/bitflags-1.3.2/.cargo-checksum.json`
    `f55e8259de325556eb0d05cbadc29969e3e7fd70a5562a71137443f3d8924782`
  - `crates/bitflags-1.3.2/.cargo_vcs_info.json`
    `5386b6ede1417ea7b388580bcc0f40cd12a88b54c9f5fa324aba53d5b2177a53`
  - `crates/bitflags-1.3.2/CHANGELOG.md`
    `d362fc1fccaaf4d421bcf0fe8b80ddb4f625dade0c1ee52d08bd0b95509a49d1`
  - `crates/bitflags-1.3.2/CODE_OF_CONDUCT.md`
    `42634d0f6d922f49857175af991802822f7f920487aefa2ee250a50d12251a66`
  - `crates/bitflags-1.3.2/Cargo.toml`
    `87aced7532a7974eb37ab5fe6037f0abafc36d6b2d74891ecd2bf2f14f50d11e`
  - `crates/bitflags-1.3.2/Cargo.toml.orig`
    `b4f5eefea5453292bbe9a91f618666eb346fac484f940bbc53fe927b8c2e0071`
  - `crates/bitflags-1.3.2/LICENSE-APACHE`
    `a60eea817514531668d7e00765731449fe14d059d3249e0bc93b36de45f759f2`
  - `crates/bitflags-1.3.2/LICENSE-MIT`
    `6485b8ed310d3f0340bf1ad1f47645069ce4069dcc6bb46c7d5c6faf41de1fdb`
  - `crates/bitflags-1.3.2/README.md`
    `baa8604f8afb34fd93b9c79729daafb884dedcaf34023e4af8ad037d916061fd`
  - `crates/bitflags-1.3.2/.github/workflows/rust.yml`
    `01a64da62e40322f547eca8b38e92a4b0cd47bbbdc580d806cfbfc42cc85d07d`
  - `crates/bitflags-1.3.2/src/example_generated.rs`
    `e43eb59e90f317f38d436670a6067d2fd9eb35fb319fe716184e4a04e24ed1b2`
  - `crates/bitflags-1.3.2/src/lib.rs`
    `e6477688535ee326d27238aeedc9cb4320ac35b9d17a4deda09e0587b0ccdbd4`
  - `crates/bitflags-1.3.2/tests/basic.rs`
    `146f1cbf6279bc609242cd3349f29cb21b41294f5e4921875f5ec95bd83529a2`
  - `crates/bitflags-1.3.2/tests/compile.rs`
    `f27c67a7dd183ca30efea1b6e0880e3469a6dd63b92b1fd711c082df182c9eec`
  - `crates/bitflags-1.3.2/tests/compile-fail/impls/copy.rs`
    `b791371237ddc75a7c04d2130e03b462c9c00a80dca08bd45aa97433d9c0d13a`
  - `crates/bitflags-1.3.2/tests/compile-fail/impls/copy.stderr.beta`
    `77d83484ce221d4b6ff2f7de843929a452d779fcfff428122710dd8218c298e3`
  - `crates/bitflags-1.3.2/tests/compile-fail/impls/eq.rs`
    `0cee8b9e07d537890e0189710293b53972d0fab63c09366f33c391065afafa99`
  - `crates/bitflags-1.3.2/tests/compile-fail/impls/eq.stderr.beta`
    `381fc6143d45ce76d7cecc47aa59cb69fe5e79c0b60a4a85d5c6163b400b3cc7`
  - `crates/bitflags-1.3.2/tests/compile-fail/non_integer_base/all_defined.rs`
    `95e14cad9e94560262f2862c3c01865ac30369b69da1001b0e7285cb55e6cb75`
  - `crates/bitflags-1.3.2/tests/compile-fail/non_integer_base/all_defined.stderr.beta`
    `1760739a276690903bb03844025587d37939f5dfcbfab309db3c86f32bdbf748`
  - `crates/bitflags-1.3.2/tests/compile-fail/non_integer_base/all_missing.rs`
    `b3d9da619d23213731ba2581aa7999c796c3c79aaf4f0ee6b11ceec08a11537f`
  - `crates/bitflags-1.3.2/tests/compile-fail/non_integer_base/all_missing.stderr.beta`
    `37e102290d3867e175b21976be798939f294efb17580d5b51e7b17b590d55132`
  - `crates/bitflags-1.3.2/tests/compile-fail/visibility/private_field.rs`
    `38e4d3fe6471829360d12c8d09b097f6a21aa93fb51eac3b215d96bdae23316b`
  - `crates/bitflags-1.3.2/tests/compile-fail/visibility/private_field.stderr.beta`
    `5aa24a3ebb39326f31927721c5017b8beb66c3e501fb865a3fa814c9763bfa0f`
  - `crates/bitflags-1.3.2/tests/compile-fail/visibility/private_flags.rs`
    `2ce4235802aa4e9c96c4e77d9e31d8401ef58dcda4741325184f0764ab1fe393`
  - `crates/bitflags-1.3.2/tests/compile-fail/visibility/private_flags.stderr.beta`
    `f3eb9f7baf2689258f3519ff7ee5c6ec3c237264ebcfe63f40c40f2023e5022f`
  - `crates/bitflags-1.3.2/tests/compile-fail/visibility/pub_const.rs`
    `8f813a97ac518c5ea8ac65b184101912452384afaf7b8d6c5e62f8370eca3c0a`
  - `crates/bitflags-1.3.2/tests/compile-fail/visibility/pub_const.stderr.beta`
    `823976ae1794d7f5372e2ec9aabba497e7bb88004722904c38da342ed98e8962`
  - `crates/bitflags-1.3.2/tests/compile-pass/impls/convert.rs`
    `88fe80bfb9cd5779f0e1d92c9ec02a8b6bb67e334c07f2309e9c0ba5ef776eb0`
  - `crates/bitflags-1.3.2/tests/compile-pass/impls/default.rs`
    `c508f9a461691f44b45142fa5ad599f02326e1de4c0cbca6c0593f4652eba109`
  - `crates/bitflags-1.3.2/tests/compile-pass/impls/inherent_methods.rs`
    `ecc26388e9a394bfa7a5bb69a5d621ab3d4d1e53f28f657bb8e78fe79f437913`
  - `crates/bitflags-1.3.2/tests/compile-pass/redefinition/core.rs`
    `ff5b6e72f87acc6ebb12405d3c0f6e3fa62e669933656a454bb63b30ea44179c`
  - `crates/bitflags-1.3.2/tests/compile-pass/redefinition/stringify.rs`
    `1edbce42b900c14425d7ffa14e83e165ebe452d7dccd8c0a8a821bdec64f5c93`
  - `crates/bitflags-1.3.2/tests/compile-pass/repr/c.rs`
    `6fda17f7c2edfcd155314579e83d0fc8a16209e400f1f9a5ca77bd9a799041f2`
  - `crates/bitflags-1.3.2/tests/compile-pass/repr/transparent.rs`
    `6cdc87a2137d8a4e0c8ce9b6cba83c82255f8ea125951bf614418685600489ce`
  - `crates/bitflags-1.3.2/tests/compile-pass/visibility/bits_field.rs`
    `1f3e5ba5a047440066a9f6bf7b7af33f5b06f6b1da3dd9af6886168199a7ea0a`
  - `crates/bitflags-1.3.2/tests/compile-pass/visibility/pub_in.rs`
    `e95312ff60966d42ec4bc00225507895a9b8ec24056ce6a9edd9145be35d730f`
  - `crates/byteorder-1.5.0/.cargo-checksum.json`
    `c98421717e17c9814bc877c20a9a8f12310b2e0aafac53ba7071db29014e1d31`
  - `crates/byteorder-1.5.0/.cargo_vcs_info.json`
    `22f6dd88c6eafeaed5cf136324470cac49d811de2b72b731170f3624f31e39bf`
  - `crates/byteorder-1.5.0/CHANGELOG.md`
    `c1cb69be6db5933c4bb4ebb6591e0fe3e7b97d491face3abcf947383c218bb31`
  - `crates/byteorder-1.5.0/COPYING`
    `01c266bced4a434da0051174d6bee16a4c82cf634e2679b6155d40d75012390f`
  - `crates/byteorder-1.5.0/Cargo.toml`
    `94ba374cb26f3c68fb83da2e5e7dce85920fc4fb827620b06b39d71a9d0e1e18`
  - `crates/byteorder-1.5.0/Cargo.toml.orig`
    `9d03fe1393b5f6308f0368c7a37efb8516b64d4521569be5c8a0432cbc8f36a2`
  - `crates/byteorder-1.5.0/LICENSE-MIT`
    `0f96a83840e146e43c0ec96a22ec1f392e0680e6c1226e6f3ba87e0740af850f`
  - `crates/byteorder-1.5.0/README.md`
    `2f2d64924c35b7203e3e3f3d136fcb714281762d145ca3513246da5547b1d014`
  - `crates/byteorder-1.5.0/UNLICENSE`
    `7e12e5df4bae12cb21581ba157ced20e1986a0508dd10d0e8a4ab9a4cf94e85c`
  - `crates/byteorder-1.5.0/rustfmt.toml`
    `1ca600239a27401c4a43f363cf3f38183a212affc1f31bff3ae93234bbaec228`
  - `crates/byteorder-1.5.0/.github/workflows/ci.yml`
    `a9ff175a9ff8e3d874351f433a4d73f59cb67601bec47c97c5fa587dc5edf60b`
  - `crates/byteorder-1.5.0/benches/bench.rs`
    `8b114080042d3292ec8de425904e4114b7f532fe3add0d807521e6cc166a17ea`
  - `crates/byteorder-1.5.0/src/io.rs`
    `9612530634d0e7ce9887a23836b58c0d972c1f45b05d9ada8355961567075627`
  - `crates/byteorder-1.5.0/src/lib.rs`
    `ab3394c385b32457795931440cfb8dbca70ba5d9e1a428fcf651f7ccb2d6c34f`
  - `crates/cfg-if-1.0.5/.cargo-checksum.json`
    `cf5a27f0edd863a614631ca118670ee7caed2e350f15cfb9561efbedcc8b9027`
  - `crates/cfg-if-1.0.5/.cargo_vcs_info.json`
    `ad1a083c000b6022a05855a9f2a99993a0415ea20ef074ebff48da8deed5283c`
  - `crates/cfg-if-1.0.5/CHANGELOG.md`
    `6b441001a1f4c3c1bf54a1a52a37bfecfd235dfd79605c32d83ef99da2c5af7b`
  - `crates/cfg-if-1.0.5/Cargo.lock`
    `0075c497b36065d8af4834ab452d792931cfdf729edf1cacc61958807a7463c4`
  - `crates/cfg-if-1.0.5/Cargo.toml`
    `d646663ec1bd4e8e795bd6a386cff9c374880d6237bbfa36bbac08b7f26ce65c`
  - `crates/cfg-if-1.0.5/Cargo.toml.orig`
    `41073cdac43584a3d0c2589ae5ad315281497d6b9f17288208f69e04c5630408`
  - `crates/cfg-if-1.0.5/LICENSE-APACHE`
    `a60eea817514531668d7e00765731449fe14d059d3249e0bc93b36de45f759f2`
  - `crates/cfg-if-1.0.5/LICENSE-MIT`
    `378f5840b258e2779c39418f3f2d7b2ba96f1c7917dd6be0713f88305dbda397`
  - `crates/cfg-if-1.0.5/README.md`
    `d16553c2dc0028d196c2ceb82e1685932061daacd3e0240339d9e393695c1e71`
  - `crates/cfg-if-1.0.5/.github/dependabot.yml`
    `828e3ecefdc555a5210a5bdffd5621ef3625ceb35c7fc91a0b4faef6f9921b75`
  - `crates/cfg-if-1.0.5/.github/workflows/main.yaml`
    `1db40c2a6c2addb69e8ee48f4e8edc5faaa205145945d061b1c30917d14037dc`
  - `crates/cfg-if-1.0.5/.github/workflows/publish.yaml`
    `7e460ab355a07549ac284f9062b83db5f077ad62fbf49835b4a3770b8dfdb5ac`
  - `crates/cfg-if-1.0.5/src/lib.rs`
    `2fe40504f7a897ac6b742a85bdf8c4b5b2c6de87cdd72d0cf49fa50585ac9955`
  - `crates/cfg-if-1.0.5/tests/xcrate.rs`
    `bcec148e69db81b1a618bdd6f96a25d9a0442e6ecc692fe28f1206d9bffc006a`
  - `crates/hash32-0.3.1/.cargo-checksum.json`
    `a584ed4e0ba5ade91d0303455393a0c2c9487062d787315db4c83ba577f8c308`
  - `crates/hash32-0.3.1/.cargo_vcs_info.json`
    `f886cb4552ea1974de3bb178c61a0a9ba3f4af9f7826182fd96574aa5893709d`
  - `crates/hash32-0.3.1/.travis.yml`
    `c31401333c62eaffbffb813638e5dc57164f8ba5cff3e8fae2ef45d1b5e76df4`
  - `crates/hash32-0.3.1/CHANGELOG.md`
    `9d1499ff3a31f6677fe7d1b85c475a143fbe84b6702f6dc7f57c78bdf34dc89a`
  - `crates/hash32-0.3.1/Cargo.lock`
    `a794e199c1cdf5d2b335200cd937c02e3209cfedab1207798e733b7928c49777`
  - `crates/hash32-0.3.1/Cargo.toml`
    `167a05eea579dd60a0489ef2ecfe5dcc1353eca2189d7c10fb010fa28a5f7974`
  - `crates/hash32-0.3.1/Cargo.toml.orig`
    `d31cd36d73f02708a83742aeb7012631c3878fa35eae831a9ea47b6190427914`
  - `crates/hash32-0.3.1/LICENSE-APACHE`
    `a60eea817514531668d7e00765731449fe14d059d3249e0bc93b36de45f759f2`
  - `crates/hash32-0.3.1/LICENSE-MIT`
    `c986bcdb83103d4ddf58aeb7b5302359782e54dc4eea14a0cc27c62dcbd50729`
  - `crates/hash32-0.3.1/README.md`
    `4af21f69cb38d4651ca73b4c5d2191362d90f9804424ec79106f415c0928ae31`
  - `crates/hash32-0.3.1/.github/bors.toml`
    `1eefee7786efb5237fab3e74eb8704a917e8c1f235b967919cea14d4693a2b17`
  - `crates/hash32-0.3.1/.github/workflows/ci.yml`
    `7acd475af1ccb8c462d9a8d99ceef08c93d2b87d63aa325eb78496bddc01e0b9`
  - `crates/hash32-0.3.1/examples/derive.rs`
    `e64bcc2de0a285da4519f18928ba741987374ea9f0bb78848287ee8c754ef44c`
  - `crates/hash32-0.3.1/src/fnv.rs`
    `db654d88ad27c6acd361a65a96140bbf0ad67505fd45692a3f36af284d7cd81b`
  - `crates/hash32-0.3.1/src/lib.rs`
    `1619dfa22a88463e9ff4d65abdd6668f04655d7a1dc5caf121e4869af0871415`
  - `crates/hash32-0.3.1/src/murmur3.rs`
    `49bd9828c48607a060cb4f4263de6d3954a2a2b41a054cb083560da4fac632eb`
  - `crates/heapless-0.9.3/.cargo-checksum.json`
    `850c641be8f371158bf2914dc7968e42777e42e70cfd0d683dad0b65e8cddfd8`
  - `crates/heapless-0.9.3/.cargo_vcs_info.json`
    `532d3c3334f7e86fbb292bbc73b2b803ad7d90cf4889f5291eb193946e688557`
  - `crates/heapless-0.9.3/.rustfmt.toml`
    `2550dec5c78b0e01b7ee0abd685fbcc3daf63b1900ee246b96ad06da13117ac6`
  - `crates/heapless-0.9.3/CHANGELOG.md`
    `4d7d09246a5163d3859a930900edde1daab76a6f3de632b096eb663b531951dd`
  - `crates/heapless-0.9.3/CODE_OF_CONDUCT.md`
    `fba7846e321b6ac7f74932cbf5831d89a7116c71763f1b263ba1c49ac7c2f382`
  - `crates/heapless-0.9.3/Cargo.lock`
    `a6c87390cc2a3554b6bd66b90b7ec77f71d4e989d86177a961fb61f876d7acd8`
  - `crates/heapless-0.9.3/Cargo.toml`
    `21463bf53d8b8505eb7a48a11c92b7bb04771f4cf4ab71b9f79ca85d0f447197`
  - `crates/heapless-0.9.3/Cargo.toml.orig`
    `11e1331091e0396678535570d68728fe71da9f176b7f8fb4eb63d5b7f405d420`
  - `crates/heapless-0.9.3/LICENSE-APACHE`
    `a60eea817514531668d7e00765731449fe14d059d3249e0bc93b36de45f759f2`
  - `crates/heapless-0.9.3/LICENSE-MIT`
    `035e70219855119df4273b3c5b97543ae82e0dd60c520416e759107c602f651b`
  - `crates/heapless-0.9.3/README.md`
    `1ce816ccc884c93f0e797eadfc10c378b0183fc9c87197745c9cb720adc4c9d2`
  - `crates/heapless-0.9.3/build.rs`
    `8bb1b56df2c231f086314c0e61fa995594b221a4c8b67c1bab2d743bc8e2957c`
  - `crates/heapless-0.9.3/rustfmt.toml`
    `4953048a8d16fba8736c5cdb65ce418e462317a79793f16f79023f571f3fd5ce`
  - `crates/heapless-0.9.3/suppressions.txt`
    `531ca2293c65b2ced87e2ffff06abce49c60cceeb1a0c1fc4c551e816528a34a`
  - `crates/heapless-0.9.3/.github/bors.toml`
    `1018c52657cc4064f7cb92137dee5f7ccb44ad273e83884e475767295733049f`
  - `crates/heapless-0.9.3/.github/workflows/build.yml`
    `668fa6dcf77f1151ccc08676953b41e586ddb165d185114b86ccb7a12ec6a9d3`
  - `crates/heapless-0.9.3/.github/workflows/changelog.yml`
    `d7ae186823f61513ff8094d08dd83e6f4fb1ef0721b51e7e6190393838aa4503`
  - `crates/heapless-0.9.3/.github/workflows/properties/build.properties.json`
    `b6d2f2f3bb768f084ebeaf372b26edba37c809ea987a8e0fb7be87e98d1610cc`
  - `crates/heapless-0.9.3/src/binary_heap.rs`
    `5afedfef64f33e21254a548d7e1873ae118f9931c04724390c64edccbd2597f2`
  - `crates/heapless-0.9.3/src/bytes.rs`
    `0940a37f7c6f0c7fabda76370dbfe0a5f9c3731c96243947e61f90553156c795`
  - `crates/heapless-0.9.3/src/c_string.rs`
    `375763204ae6d9a3e999c6be06333e71eb98b00a7b38534641b5b8314af14cf4`
  - `crates/heapless-0.9.3/src/de.rs`
    `9db6dc4032b0c1996131e965d48bce275a099f03bd949d9fc4375b1996f03004`
  - `crates/heapless-0.9.3/src/defmt.rs`
    `bd94982d1b8457d1309b4ec01deaa7750556620a168852a023b7a0db3f4d9be3`
  - `crates/heapless-0.9.3/src/deque.rs`
    `e4e43d559580112d8b896a89c65f95b2c8a5f99d0fbcfbf8605ed37807774889`
  - `crates/heapless-0.9.3/src/embedded_io.rs`
    `3dbf95b2244b9e1bcb53d92f82cf77090902910607f75cf41a9d83bbb55d9649`
  - `crates/heapless-0.9.3/src/history_buf.rs`
    `9b335b39b0533b3fc10ee373f68f87af0dc4cd603f7d4b140543dfaddfb02734`
  - `crates/heapless-0.9.3/src/index_map.rs`
    `8381a2bf30be108e7b898466938e2617d0b607f9b43e595ea722ad5e4d662ce2`
  - `crates/heapless-0.9.3/src/index_set.rs`
    `ca11d79fdf4ee5bc3682bf703c0538a707e831462d1146c50248fd81369a922f`
  - `crates/heapless-0.9.3/src/len_type.rs`
    `7795ca946c56a44afc8fe9abc6229907afb90b6e48e19ddfd1857bb74c71076f`
  - `crates/heapless-0.9.3/src/lib.rs`
    `866774518e3ff45d1daee16daf510a1fc1ffc48a6439c29732d69f9879771990`
  - `crates/heapless-0.9.3/src/linear_map.rs`
    `3d7e09431fb3ec76056d679961219a1483e0284af39cbdd69a37172107ddd1db`
  - `crates/heapless-0.9.3/src/mpmc.rs`
    `8034ea2116da748f8f5836fd62363a1c89ff7abc0fbd9a3ecd8890729f5ae398`
  - `crates/heapless-0.9.3/src/pool.rs`
    `6e8350d1745eccf0b6195ed6d0d6b16c84e049a0578a88ed2da9505562512f49`
  - `crates/heapless-0.9.3/src/ser.rs`
    `c0fc644dfba84a00c334e26c1d85e1c32519b9cdb39efef6923bf3c7230b63a4`
  - `crates/heapless-0.9.3/src/slice.rs`
    `fefddc0b36efeee374d8fd79e9786dca89c5054c77fbb260da7c953ace9ba372`
  - `crates/heapless-0.9.3/src/sorted_linked_list.rs`
    `fe7226da2c7cd69d797e3befca7790f24d90b0a5fd4ce4facb321cee151753e1`
  - `crates/heapless-0.9.3/src/spsc.rs`
    `29efe90bfc1d9964718d5303141b1a7672bac5aa17bb35a312e528c50ff5ca96`
  - `crates/heapless-0.9.3/src/storage.rs`
    `03cc6d9a443250dfd499512a34e7c4f03a0b24f4e6dcc20d8991b992394340c9`
  - `crates/heapless-0.9.3/src/test_helpers.rs`
    `dbd0bc47d7f56a20f878a1416ae4028f810a1ebbab8156a64b165481eb483320`
  - `crates/heapless-0.9.3/src/ufmt.rs`
    `820731b0b8483e3b7b96891b9a4c57f738361523b5c181ba9b94056f639d81f7`
  - `crates/heapless-0.9.3/src/pool/arc.rs`
    `7eaf0a082231c944d99f68d970b528c2cbf67204975bbab6d74b87ceab4bed6e`
  - `crates/heapless-0.9.3/src/pool/boxed.rs`
    `8ffd16b8e19d9430b0d3090f7f7b3d4df7c6846c5a760424c14fb9ad3c58e8d5`
  - `crates/heapless-0.9.3/src/pool/object.rs`
    `b8b9f1e2a9ed86e5f6c6508e8cb469da44f34280db611285240e6cb7d98c123a`
  - `crates/heapless-0.9.3/src/pool/treiber.rs`
    `f9dac61f0e579b777099c4d01dadfb5bdfe71fd06758aefe73b53ae76d1630ae`
  - `crates/heapless-0.9.3/src/pool/treiber/cas.rs`
    `dad864ef1faacfbbd7a97c7a5eced0db23ca3aea2b090b4b7e0865f776135d04`
  - `crates/heapless-0.9.3/src/pool/treiber/llsc.rs`
    `dd1d3ee966c09cbde432c335bb5cf35856e4f67c357bf708199e5d221afab111`
  - `crates/heapless-0.9.3/src/string/drain.rs`
    `2793ff8b438b37ceb2fdf341c3ef9d09a3ea099dbc471599c08c2c142331081b`
  - `crates/heapless-0.9.3/src/string/mod.rs`
    `85424df39b9334468a220b35c1447cabfb2c61a85456608cc2f474328e06401c`
  - `crates/heapless-0.9.3/src/vec/drain.rs`
    `e8f71fef94f6b6f1fb636db45efa610054fbf2177b727b97b3ee023b9f4d7300`
  - `crates/heapless-0.9.3/src/vec/mod.rs`
    `31ca793f0f60d81cce99302e4d305dfc41abea4a945fdb408717e7966e0281c4`
  - `crates/heapless-0.9.3/tests/cpass.rs`
    `8e07641526ffc9d72a8f88c9ca40a9af30b44ed9a3e46c466e8732dfc3044768`
  - `crates/heapless-0.9.3/tests/tsan.rs`
    `e4574a9cdb640836563409d2a0fee89d52a760173e2c776306e0dd2c14a19216`
  - `crates/managed-0.8.0/.cargo-checksum.json`
    `1e474a9a8b1d817936e3b351c30a7f93df0b0da1cfb078a2c5ce28961c24f5f2`
  - `crates/managed-0.8.0/.cargo_vcs_info.json`
    `05f3e88e8d143717e3dd9345e656d99260bea370ccadc0e7a34cd40615dfe9f0`
  - `crates/managed-0.8.0/.travis.yml`
    `2ecb6a365be4c111ffd257856f42aad30446c7a27054c5f09832590558ad3d9b`
  - `crates/managed-0.8.0/Cargo.toml`
    `0d65ebdf9d9976b76bfc6dfc0c72549472749d286805f3bf7582c9b48c6d2a81`
  - `crates/managed-0.8.0/Cargo.toml.orig`
    `e5fabf34a24e5d7c933a0bcef5828dc8f515f960ef098024e0aec8e5ad8534e4`
  - `crates/managed-0.8.0/LICENSE-0BSD.txt`
    `cd29cb316eb7a949599133073c5f5fc8d75d19ed21bdae8b0d4d1c32c5ccf117`
  - `crates/managed-0.8.0/README.md`
    `5c3291440a94a59106b0b5a3553ce0e01a6f998b0b5bf178c2cace4361b8e4ab`
  - `crates/managed-0.8.0/coverage.sh`
    `57bd8ef3165e7f017da7addcd74b5be8e97042b0d1f5d67070adec814e8304e9`
  - `crates/managed-0.8.0/src/lib.rs`
    `503fccb6af7c95d4561188bde5dc073323f7e82abda8640735f5235b31cb0c93`
  - `crates/managed-0.8.0/src/map.rs`
    `ace5fdc144052faefa44c7be86c00811e21461b889f65090849381c280b04c09`
  - `crates/managed-0.8.0/src/object.rs`
    `78c98ca572b9d1cf34222a25249433176f1e321a47cd2e7602a56a45c38bcdb5`
  - `crates/managed-0.8.0/src/slice.rs`
    `e4d23f465b9dcd8c9c20fa83303e76910108db708d7009ec980a2132ca452199`
  - `crates/managed-0.8.0/src/slotmap.rs`
    `080bc281163adf548ddefc945b29aae3f00f0a71a8e5f1dd9a7cc84c7519e043`
  - `crates/smoltcp-0.14.0/.cargo-checksum.json`
    `8383633953dc7e3c978f2f95cd5535f86d9a7f28f42c09e4722391d96eb0bc64`
  - `crates/smoltcp-0.14.0/.cargo_vcs_info.json`
    `c93171d3804cbb3405be96001a2e67275aa2b157f5ba7f820102d1bc125aa61e`
  - `crates/smoltcp-0.14.0/CHANGELOG.md`
    `abaf3014135acc77cfcd1e8869039014d7861dc2a47a393d44b9ea69dea4ee25`
  - `crates/smoltcp-0.14.0/CONTRIBUTING.txt`
    `15f3866e557f38a5bd4afe930d1e1716679eac5b03d2fac662228b7d5bb63cff`
  - `crates/smoltcp-0.14.0/Cargo.lock`
    `f6d8c6ae37a259dac0166b92d703b5b005b9c46c6e32d5ab333113f686b2332e`
  - `crates/smoltcp-0.14.0/Cargo.toml`
    `c9b9c53136ef6c6c94c292587c6a629712e1a3167e25df9033edf7f0e3b0c0fe`
  - `crates/smoltcp-0.14.0/Cargo.toml.orig`
    `4f3fc24aa49fb309278e9a749f7e690c4227479c325b5a9b69342dadc067d149`
  - `crates/smoltcp-0.14.0/LICENSE-0BSD.txt`
    `beb2cad88fab8447f7975564e21f9506e733e14c344836f146fa02e811216694`
  - `crates/smoltcp-0.14.0/README.md`
    `d75bdf1a3e4cfb0dedb1a1d32b73940e5cd0a258ad8008511b5b6f3e813c5407`
  - `crates/smoltcp-0.14.0/build.rs`
    `77331ea0f4f21b60da452aec44028978deedb06a3577cf2aaa95c837886c9ac0`
  - `crates/smoltcp-0.14.0/ci.sh`
    `bbbf1593513df0bed298dbeda2764f6096b7c1c7cf4ee5307ee48543600c560a`
  - `crates/smoltcp-0.14.0/gen_config.py`
    `4865d681d5d29cc25fc3f703107b3e9b0bd949dcc439d205daa3efd7fcd826f1`
  - `crates/smoltcp-0.14.0/.github/codecov.yml`
    `a3f9dbd81eea794acde6bd9a2d684f56add1f2f69ddfc906362667935482706e`
  - `crates/smoltcp-0.14.0/.github/workflows/coverage.yml`
    `92f368658ec6729e928f01f774eeba89543e1e399ec804feb287c7f88535cb56`
  - `crates/smoltcp-0.14.0/.github/workflows/fuzz.yml`
    `3f3e4dc343f611858da19741e90861db0782bb907f95a53212336ef2d74f0172`
  - `crates/smoltcp-0.14.0/.github/workflows/matrix-bot.yml`
    `492d9f4e23ffb645c4a7a8c0ff51f458b49e66c09ee8eb6d491bea6a4b3aa930`
  - `crates/smoltcp-0.14.0/.github/workflows/rustfmt.yaml`
    `618f58a0efeb91bfed5854c368cbc2097e0bfdf7721c619d436d68a21fc4d496`
  - `crates/smoltcp-0.14.0/.github/workflows/test.yml`
    `2965a697f72f1310b091ce794fc3a7e5f4278633af2a59c16119ae5797756e04`
  - `crates/smoltcp-0.14.0/benches/bench.rs`
    `17c94bb960e26a3173e78659ebe5154d4c1241a666d787cae9740590d18cd670`
  - `crates/smoltcp-0.14.0/examples/benchmark.rs`
    `234a001a44028b41a8318e82f7f5b9c77de74e24f40fa1c79eb894856b7d7d99`
  - `crates/smoltcp-0.14.0/examples/client.rs`
    `ef028135033ad5f31588e9122e8d96711f228ab5297242d28978d4fa650e858f`
  - `crates/smoltcp-0.14.0/examples/dhcp_client.rs`
    `2f95eda55d144b2cd1476c2393d4c123d3dddd695651bee03fc1bd5ca96bc7b8`
  - `crates/smoltcp-0.14.0/examples/dns.rs`
    `619a0d7f2737a8bd72c3541c63ae704e85186f5e89f8cffb99cca117ceda54cc`
  - `crates/smoltcp-0.14.0/examples/httpclient.rs`
    `9864d4c0c72660917a439fae202bfff00bd60debb74c96850e7e610be81fe117`
  - `crates/smoltcp-0.14.0/examples/loopback.rs`
    `fe9870794281be7f94fa20122f55f641f6fac6530646c095e941fa27eab485b3`
  - `crates/smoltcp-0.14.0/examples/loopback_benchmark.rs`
    `33e7f22e0f8a81269e93eaa9169b24f58189d88ff4aa12c2f39a3aba1e460e49`
  - `crates/smoltcp-0.14.0/examples/multicast.rs`
    `f19f2b5cfc9470525b1868899be5b3f0ad4e20502fd3390ab509d53092966c20`
  - `crates/smoltcp-0.14.0/examples/multicast6.rs`
    `0eccad9be12fece3e2e919e185a9fca025beeca39af259c4a1218a6ccd49cd0a`
  - `crates/smoltcp-0.14.0/examples/ping.rs`
    `a4393bc2a9a05683ef10100504636ac941b1a384f5e2b4f1d8528f55aeda56df`
  - `crates/smoltcp-0.14.0/examples/server.rs`
    `84b7b0ba03e4e2d10206927f8e1d9500eba0f9998fc8a070a0dbefc826082bd5`
  - `crates/smoltcp-0.14.0/examples/sixlowpan.rs`
    `303d5be135dafe9b1d4a1b2ed791d0ed6119ffb2534555202a2f154909a6a442`
  - `crates/smoltcp-0.14.0/examples/sixlowpan_benchmark.rs`
    `10b5aef6813d5bebf01ad9241c77f88db1a23ec59d713375bee171ac9c1afd77`
  - `crates/smoltcp-0.14.0/examples/slaac.rs`
    `94c008e9581239573129bac9ea9ed244dcf742e3b5aad4886821b5a20a26014a`
  - `crates/smoltcp-0.14.0/examples/tcpdump.rs`
    `a6cf35290a796865ec1793b9b4e020300dfdcbee127f03c49aec43e6183dcc3d`
  - `crates/smoltcp-0.14.0/examples/utils.rs`
    `2ccee54aa98683d95600d7111f06de07fe2a05d436bfc85a0c33db3993cd0c84`
  - `crates/smoltcp-0.14.0/src/lib.rs`
    `ee6d4a6f0db53cab60a0f628aac08b598abbd6d9268399936c0d51f74d80c4c2`
  - `crates/smoltcp-0.14.0/src/macros.rs`
    `a742cfce854e6707e470ff86bc042265a51aecf0fe586bb3f8ef7c49f076ad81`
  - `crates/smoltcp-0.14.0/src/parsers.rs`
    `6447cb240c630580beb5937334a2430850c80b26046d19041dfecaf50629e026`
  - `crates/smoltcp-0.14.0/src/rand.rs`
    `0d9e7dd2f2077730096a35f94c6feda9c1eb33188e1f5e75b2d3b7ecc48860f9`
  - `crates/smoltcp-0.14.0/src/tests.rs`
    `07373ce1625a244dedd34db2f6bcaee7859c425fba16ebed25b0c354c2aa7978`
  - `crates/smoltcp-0.14.0/src/time.rs`
    `3c9958ce416164336d1d1ca460fc6bbea30d89a6067009c12294e19ee4c54e47`
  - `crates/smoltcp-0.14.0/src/iface/fragmentation.rs`
    `d6642025b57e93bce4b93d781c5a7385634b976fdde65eb22de4a1b11de23f37`
  - `crates/smoltcp-0.14.0/src/iface/mod.rs`
    `fbd728615ef7ad171c67db96bd6e9456d2194e8e72d4ea51ecfba8ff40367e07`
  - `crates/smoltcp-0.14.0/src/iface/neighbor.rs`
    `6c72cd684d6db7e3710927dad20d40b2ea56f64778fe7990ba014c0f53e24dd4`
  - `crates/smoltcp-0.14.0/src/iface/packet.rs`
    `5544f27d25b246976cf00ac83ed5a71b08798c53b80356ffc729af6d69f8921d`
  - `crates/smoltcp-0.14.0/src/iface/route.rs`
    `04e49caa64717a2516ff6de428a17057b76420b4a4b626aac862ff5503cade38`
  - `crates/smoltcp-0.14.0/src/iface/slaac.rs`
    `a7ee905b4e70b58ced607db206d972d877ec482cb770a42540e29fb4637dc081`
  - `crates/smoltcp-0.14.0/src/iface/socket_meta.rs`
    `4719a9b1af655673264d517c0bc54654559523bc42f7529e63eca0500e8a5109`
  - `crates/smoltcp-0.14.0/src/iface/socket_set.rs`
    `9b9bfbf8a003d23e7c892eb0af8680200171c84d312a216e1368f211b34cb8a7`
  - `crates/smoltcp-0.14.0/src/iface/interface/ethernet.rs`
    `a8f0c7626caf4cc4c0df75befbee7dff279250b90f004ae71e57200a7fa60365`
  - `crates/smoltcp-0.14.0/src/iface/interface/ieee802154.rs`
    `1dbe7640c56fccd52b4cac1938e6f43c65850058dee259120b030afd4b0bff4d`
  - `crates/smoltcp-0.14.0/src/iface/interface/ipv4.rs`
    `574d8a1e421023ca23caf3adbba9a4d829bad1125c622588b0528392e510d2d5`
  - `crates/smoltcp-0.14.0/src/iface/interface/ipv6.rs`
    `5d8137aba994f1759a80328ec972895ba65ea1e417a1cbe7faa6421ce08c713e`
  - `crates/smoltcp-0.14.0/src/iface/interface/mod.rs`
    `b490b161e1ddc3fb44e2a9c8d765d3599f48c234560b1210d7a29bc93abc666e`
  - `crates/smoltcp-0.14.0/src/iface/interface/multicast.rs`
    `0a899a258e54314bd677be82d3e1086ef3e67ef74d69fd41f681f470f2a40de6`
  - `crates/smoltcp-0.14.0/src/iface/interface/sixlowpan.rs`
    `3a7d173a9d37939024e466937c6af86bd863a5d756be18ddd36be945a7179221`
  - `crates/smoltcp-0.14.0/src/iface/interface/tcp.rs`
    `0954554ddc2863e810c2349ab1bd95ae760bfcf92b8845925a32439f20051e86`
  - `crates/smoltcp-0.14.0/src/iface/interface/udp.rs`
    `32553d52d249c451be0b06af0bac32f883ae194da17f208a7235e54f8528fc0c`
  - `crates/smoltcp-0.14.0/src/iface/interface/tests/ipv4.rs`
    `43ed8bc001de66d671cf7eebd1747a17d10e82f4bdd3020f27ceae1b7ef505fc`
  - `crates/smoltcp-0.14.0/src/iface/interface/tests/ipv6.rs`
    `5f3dac3f0afbcde57f8005b34fdcb98f865f145b607235858b7901ae9c8c9fe2`
  - `crates/smoltcp-0.14.0/src/iface/interface/tests/mod.rs`
    `ae0cc0124ccdf650e2140d4bd9727d10864e002b2f76491669d22ee126d23ede`
  - `crates/smoltcp-0.14.0/src/iface/interface/tests/sixlowpan.rs`
    `321be10f399a0bf48c75bbde95f8bb42be3c7c0b46fa60eff9fa7f27be385a4d`
  - `crates/smoltcp-0.14.0/src/iface/rpl/consts.rs`
    `2ea0bbfac3585383b5f94f72e67f348a622a0183bb6083fec8b5a9ea258754e7`
  - `crates/smoltcp-0.14.0/src/iface/rpl/lollipop.rs`
    `5f4541a9dbfcfdccb4c69d4e19565700673798936336067fe1b57837cf2c50ad`
  - `crates/smoltcp-0.14.0/src/iface/rpl/mod.rs`
    `fa8ecf836d9b044b2c84ea94a8a4073f1ade52dcd198991c9a7a3786bb8369de`
  - `crates/smoltcp-0.14.0/src/iface/rpl/of0.rs`
    `28e27d3975e14c8efa553fe0be2d530e0b4d0d483d2a6221884bb9dc9030f353`
  - `crates/smoltcp-0.14.0/src/iface/rpl/parents.rs`
    `6dc449086d498590913dd30bdef6311913a7188e37cecf309036efd9f1e639b4`
  - `crates/smoltcp-0.14.0/src/iface/rpl/rank.rs`
    `93ab780238dc1cc198d0507befd4a285ff553ab1e5a7d1a31f019bd4cd4e6214`
  - `crates/smoltcp-0.14.0/src/iface/rpl/relations.rs`
    `2670e36791e00d3b069362adb68df91cd6cddac956093c60271493c6cb486fe1`
  - `crates/smoltcp-0.14.0/src/iface/rpl/trickle.rs`
    `c5821901f598a5e66e82dc234018fe4dbd64a0b08cffe4e433ecba780fc7f390`
  - `crates/smoltcp-0.14.0/src/phy/fault_injector.rs`
    `543ff634fee8862ad91575075a67cc219759c32a4109190cf0010e7e21f595f9`
  - `crates/smoltcp-0.14.0/src/phy/fuzz_injector.rs`
    `a2c96e237293091690784fc0d4884ecee6e9067f54f907cc0611bfef96b9b714`
  - `crates/smoltcp-0.14.0/src/phy/loopback.rs`
    `09be4e8fcd1d373c39437e68b71ac842c60dc270add7d5bec02e6b58584522e1`
  - `crates/smoltcp-0.14.0/src/phy/mod.rs`
    `4a113561d8de1d415cb38fa8f9a9286e878832b11e8de795c46fd509aba7d0f0`
  - `crates/smoltcp-0.14.0/src/phy/pcap_writer.rs`
    `b2017a7c6c28b224e1fd9d772ab481e9588e4fdbd46024a24d53af9dbcf5d3c3`
  - `crates/smoltcp-0.14.0/src/phy/raw_socket.rs`
    `0bdfd8ac15d9653d73d0295d39adec7260fb997e4c8b6db3e97ec348f6e7e8f4`
  - `crates/smoltcp-0.14.0/src/phy/tracer.rs`
    `9920546c8624a48d178b84fbdfed37d00ae981531ce5a24ff41a1a62828ebf39`
  - `crates/smoltcp-0.14.0/src/phy/tuntap_interface.rs`
    `09d83c561007d0dbf1d2c0ecd1dcbd95f94d66e2c6db366c3961350b6532a40d`
  - `crates/smoltcp-0.14.0/src/phy/sys/bpf.rs`
    `5718df2c72c75f05e15cf7c62a351a559c7ddc820518cf0f01869a2192ae1492`
  - `crates/smoltcp-0.14.0/src/phy/sys/linux.rs`
    `95c2ce63d4e64232c4484e034b098d491da94617d36219f7f3f812c26562d12f`
  - `crates/smoltcp-0.14.0/src/phy/sys/mod.rs`
    `8d4ac3b5596c90bff232540f1a4ab02b750ce521955ca8431a577f699f214fa1`
  - `crates/smoltcp-0.14.0/src/phy/sys/raw_socket.rs`
    `136c668b7a07b8a668655a7d7a7b6e4c6bdf70522f6c55fc47cd30d92cf3d8fa`
  - `crates/smoltcp-0.14.0/src/phy/sys/tuntap_interface.rs`
    `9cb5e8d8e96f0a82f56a6ae0d7a5bb7bc0ac734d72e496429da0ce1b08250318`
  - `crates/smoltcp-0.14.0/src/socket/dhcpv4.rs`
    `cd5ebe4f0937f77b962704232b34dee27abad7591713bd0dd46f28d8c68edf05`
  - `crates/smoltcp-0.14.0/src/socket/dns.rs`
    `7560edc6039396eaddd109918efef2d657f2d7f1e1a20b4344d5069dd846e168`
  - `crates/smoltcp-0.14.0/src/socket/icmp.rs`
    `a35e8dd3820efef7152db253e96c5aa94940d1871101405d06de6dd9e01f594e`
  - `crates/smoltcp-0.14.0/src/socket/mod.rs`
    `4771c83980cba607da119e78fe272d3544300311052f1f5a6b583a8fcd8cce18`
  - `crates/smoltcp-0.14.0/src/socket/raw.rs`
    `bf3c701d282bfde868cb4b5666281cd2478ab240c642c3cb315aa9b47b878d4c`
  - `crates/smoltcp-0.14.0/src/socket/tcp.rs`
    `da2f1bec16505310b7f9a3ae83a4e804d98897665006f7d7c76da4109e96748d`
  - `crates/smoltcp-0.14.0/src/socket/udp.rs`
    `fcf963f0d859710f28283dfcdf01861976bc487f89250937e13e334eabf5743a`
  - `crates/smoltcp-0.14.0/src/socket/waker.rs`
    `dc0492d9f9429f0b02355bdd3f1746e506f8dacdfa845bfdabdd9bc76a7c6bf5`
  - `crates/smoltcp-0.14.0/src/socket/tcp/congestion.rs`
    `e16afe4561f0ecc733ad7a461dc8fd60dc48c813ba603c15a84dbcbf6ea9024b`
  - `crates/smoltcp-0.14.0/src/socket/tcp/congestion/cubic.rs`
    `048778a2b6196d3d2ba43cf07fce2ae58cdd438d9cb4577ff07b306170f9bfb3`
  - `crates/smoltcp-0.14.0/src/socket/tcp/congestion/no_control.rs`
    `4644be1fdbfd2ea0367c75b51bf37ef162a806b35ca45f45c9758275ac31803c`
  - `crates/smoltcp-0.14.0/src/socket/tcp/congestion/reno.rs`
    `0df79a7ed5635fe3e6d284e1547d2762b040d6ba940637aaec493b2c40e8d2a7`
  - `crates/smoltcp-0.14.0/src/storage/assembler.rs`
    `783d0f6dbbae08f738cbbca7363f31a563ff9dad3d5f2612286ddb6a2631e97f`
  - `crates/smoltcp-0.14.0/src/storage/mod.rs`
    `d443e70c0fd4ac7ff33fa4632dc26e4eee8dd790e30477eaa8fe15aa6bf13e7e`
  - `crates/smoltcp-0.14.0/src/storage/packet_buffer.rs`
    `771a2da8131c8ddc39e1c5d36600c34431134aed4c95cc584bb4f6fa31edbb5b`
  - `crates/smoltcp-0.14.0/src/storage/ring_buffer.rs`
    `c1995593174301b1f84e467215befc57c41660e22ead3d968bafcb1c44be8259`
  - `crates/smoltcp-0.14.0/src/wire/arp.rs`
    `0cac2e26414a1e3f69006a0d41a497b0aa625e1b4a1552f0d0ca3313753f84a4`
  - `crates/smoltcp-0.14.0/src/wire/dhcpv4.rs`
    `748fbcd4554076108dff05712a4d23c4702f2a73573b0926ae26de8ed4990eab`
  - `crates/smoltcp-0.14.0/src/wire/dns.rs`
    `56ff26e814c7228636fdde517a146cdbc5b1e675daa7bc3d06805e51fa044c58`
  - `crates/smoltcp-0.14.0/src/wire/ethernet.rs`
    `3d5bb3709c03d72198b60de0014c7efdaf9a60cdd2336bf98caa69b432955926`
  - `crates/smoltcp-0.14.0/src/wire/icmp.rs`
    `788bf58fd931614b4294f67d1926702fb02870c77e2899290c6d9c0539f388dd`
  - `crates/smoltcp-0.14.0/src/wire/icmpv4.rs`
    `c406e14531dab5f81ca612fb6b62ebe64807677e5541fdc5ce09e840e4af20c2`
  - `crates/smoltcp-0.14.0/src/wire/icmpv6.rs`
    `3a38a3facfbebe7a85cb4fd0ab48e7c47d230e8445baba3c54ebb23fea3c6d42`
  - `crates/smoltcp-0.14.0/src/wire/ieee802154.rs`
    `caf3bac4727cdfc8b563de89774d8e3b9446f3a91d840d37914f0f00e3b96ab0`
  - `crates/smoltcp-0.14.0/src/wire/igmp.rs`
    `8433b96d35008453a03c279b6c2ee756bdfc0e3ef59f7b1a0909dac5a1731faf`
  - `crates/smoltcp-0.14.0/src/wire/ip.rs`
    `547c7999ec4e9098aff5ceeb41bec847da829adbec9e87f261215d3f0eeea41f`
  - `crates/smoltcp-0.14.0/src/wire/ipsec_ah.rs`
    `40ce6bb807b3c93655f5dcb038860bb497ed5cc897e4f27a1de51297951ced70`
  - `crates/smoltcp-0.14.0/src/wire/ipsec_esp.rs`
    `78b9be7fb0a13adec7e03c7ef816c51f86059727249f8e4f77de1123c0effef9`
  - `crates/smoltcp-0.14.0/src/wire/ipv4.rs`
    `e1fd898e2ca3465715d9b795bd0485006c8d738692fb6e774da109dde4f9d8e8`
  - `crates/smoltcp-0.14.0/src/wire/ipv6.rs`
    `4df16854e843a8e3bc814e372c509edf8eb247c4c2cb6012255482c646bf3d13`
  - `crates/smoltcp-0.14.0/src/wire/ipv6ext_header.rs`
    `52d9c296e93c629f3920faddab4f2025f0aa7f404b8c0a34c6efd122e963aaec`
  - `crates/smoltcp-0.14.0/src/wire/ipv6fragment.rs`
    `11dcf71f60da5254ae0d73a4a52d111f1006bcab62adf7077f877689f771d48a`
  - `crates/smoltcp-0.14.0/src/wire/ipv6hbh.rs`
    `c3b6b01540c555fcb5123f6b306f1e84f9f57260e3c273c378cad4bb46e4c6b0`
  - `crates/smoltcp-0.14.0/src/wire/ipv6option.rs`
    `df0da7929d3372be9e163b3fccaaf15df9c3db0a01615b341274ed9e42e4d202`
  - `crates/smoltcp-0.14.0/src/wire/ipv6routing.rs`
    `236c6f27f93445729b6a9f17529d1fbe6475a72b965c89c118a0e1293a594187`
  - `crates/smoltcp-0.14.0/src/wire/mld.rs`
    `574567dbde7600607bb374e1b8b3a5130f2ed58fcf12b8eaec84f58833d99a5b`
  - `crates/smoltcp-0.14.0/src/wire/mod.rs`
    `907c4ae45915e24173aec6ebb8f024714ca34643a64f78672c1757a6d856a563`
  - `crates/smoltcp-0.14.0/src/wire/ndisc.rs`
    `2f35fd528ad4d9c29485b6080471484cd9e468c1b63d20e9a7cd8dfcaf6a1263`
  - `crates/smoltcp-0.14.0/src/wire/ndiscoption.rs`
    `a521e2a431905510f6c25bf873f1e0152e0a25a20e50a4196787a5fc07b2482d`
  - `crates/smoltcp-0.14.0/src/wire/pretty_print.rs`
    `3bc2299749a29c8218351251267c80cdc15cfb953171f80c41d57097489226d3`
  - `crates/smoltcp-0.14.0/src/wire/rpl.rs`
    `4094d902a8e260847e948d546dd8561beb7aa565e1f3f430cc9160f7c5033613`
  - `crates/smoltcp-0.14.0/src/wire/tcp.rs`
    `7e2e1d7c7ce6263d6a09441338dc5ea515cc20a6367fd64a96f83b9b7438f9e3`
  - `crates/smoltcp-0.14.0/src/wire/udp.rs`
    `9a90af993ef1e7d2b9e39018449e34d1337694cb12e61ff3db6db707d578fe3a`
  - `crates/smoltcp-0.14.0/src/wire/sixlowpan/frag.rs`
    `7ccf658bb67bf010b3d1ba679e85219bd7687f4c7e3629bf7db51e6d544ca838`
  - `crates/smoltcp-0.14.0/src/wire/sixlowpan/iphc.rs`
    `98dc1a92870fab2ab4446cfc0fc505a8cd5031f6d5e0e8b76c7256273957d6ec`
  - `crates/smoltcp-0.14.0/src/wire/sixlowpan/mod.rs`
    `e863f6a25d26ca022d91c89afa20bc4d048401e7b9de9e8f6c661361f78a61c1`
  - `crates/smoltcp-0.14.0/src/wire/sixlowpan/nhc.rs`
    `178b470aa71b5cc3c852db2f7b3ce1c81a86ad585dac4749f4a5c2040465ada7`
  - `crates/smoltcp-0.14.0/tests/netsim.rs`
    `751324adc291f59214d4328f026178a5e762e5e2e18c2be28cdfaddf2a600fac`
  - `crates/smoltcp-0.14.0/tests/snapshots/netsim__netsim.snap`
    `c595fec9718a107eb39ecc743186b31f2c32963321228e607cda5d7972b4513c`
  - `crates/smoltcp-0.14.0/tests/snapshots/netsim__netsim_multiflow.snap`
    `ca7fd2e9ff33c0c6bc01ab0331406d8d448bd1e72216c2db95d4a78b66454dca`
  - `crates/smoltcp-0.14.0/utils/packet2pcap.rs`
    `dd64131dbd16785f81883a6893a6aea9358ddde0711aeaa61c22b57c1de2da42`
  - `crates/stable_deref_trait-1.2.1/.cargo-checksum.json`
    `850b3589fbb468679bb7a3b2e88806eb6a15b5426aae9343994a3807b0b4169b`
  - `crates/stable_deref_trait-1.2.1/.cargo_vcs_info.json`
    `e58d6ca3bc691221bb37ea20808b6168e7120494124e9b0aa7080acff3236c57`
  - `crates/stable_deref_trait-1.2.1/Cargo.lock`
    `81f573ca2ff7a97a07356e6676fc8ec0cd5ad645d79453329147899c92b82e03`
  - `crates/stable_deref_trait-1.2.1/Cargo.toml`
    `5cddd84a243940e60e772ecba2f05b28b00cbd825d341d9e0b0881dcae7847dc`
  - `crates/stable_deref_trait-1.2.1/Cargo.toml.orig`
    `e0d471ba7acef517f26f4c34c4017d78436999bcfdbd9f11338df3fce732cc7f`
  - `crates/stable_deref_trait-1.2.1/LICENSE-APACHE`
    `a60eea817514531668d7e00765731449fe14d059d3249e0bc93b36de45f759f2`
  - `crates/stable_deref_trait-1.2.1/LICENSE-MIT`
    `5e05b024f653a5ce199e77cbbbd42fb5553562ec714b819421ed0c3e552a75d7`
  - `crates/stable_deref_trait-1.2.1/README.md`
    `750fc2d57412de57566289e567b5674a666cc7a8a054f8c7129b8553f9003de7`
  - `crates/stable_deref_trait-1.2.1/src/lib.rs`
    `4736941fa6c790367158082cb55e5fc89aba7c7f0a85405aada8271fbb5c87ad`
