# VENDORED.md -- third-party components vendored into `src/`

Every vendored component has an entry here: the upstream source and pinned
commit, the per-file sha256 recorded at vendoring, and any modification.
Files under `src/vendor/` are never edited: local needs are met in RHU's own
files instead. Changing a vendored file requires a new entry here with
re-recorded hashes.
`src/vendor/**` is `-text` in `.gitattributes`, so these files stay byte-exact
in the working tree and the hashes below hold on any platform.

## src/vendor/gcinterface/ (7 files) + src/vendor/gcenv/ (3 files) -- vendored 2026-09-09

- **Source:** the public `dotnet/runtime` repository at commit
  **`ab19415702aa8139d5369e47c73edb47343c34ad`**, tag
  **`v11.0.0-rc.1.26425.128`** -- the .NET runtime version RHU builds
  against.
- **Method:** copied with sha256 verification from a read-only clone of that
  tag: bytes taken from the committed blobs, the hash computed at copy time and
  re-verified against the working-tree file. Equivalent fetch:
  `https://raw.githubusercontent.com/dotnet/runtime/ab19415702aa8139d5369e47c73edb47343c34ad/src/coreclr/gc/<file>`
  (the gcenv files live under `src/coreclr/gc/env/`).
- **What it is:** the runtime-to-GC interface contract -- `IGCHeap`,
  `IGCHandleManager`, `IGCToCLR`, `GC_VersionInfo`, `GcDacVars`, the flat
  `GCToEEInterface` static-class declaration RHU's collector consumes, the
  GC-visible `ObjHeader`/`MethodTable`/`Object`/`ArrayBase` mirrors, and the
  CGCDesc/CGCDescSeries reference-tracing format. Version constants at this
  commit: `GC_INTERFACE_MAJOR_VERSION 5`, `GC_INTERFACE_MINOR_VERSION 8`,
  `EE_INTERFACE_MAJOR_VERSION 5`.
- **Layout:** two include directories (`gcdesc.h` sits with the gcenv set);
  consumers compile with both on the include path
  (`-Isrc/vendor/gcinterface -Isrc/vendor/gcenv`).
- **Files + sha256 at vendoring** (upstream path prefix `src/coreclr/gc/`):
  - `gcinterface/gcinterface.h`
    `a6888ea2bc376d9d0a90911476dcbb9b32be932dd556229e95379bbb466aa9d8`
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
    `31c3574a8e163f56aaab9f4d7aac2b7450e32c99d18320d370c4b98a9b3de022`
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
- **Modifications: NONE.** These files are byte-identical to upstream; the
  RHU repository's `gc` quality gate re-verifies the hashes above.
