# Lamella RHU for .NET: the tools package

This package carries the two LLVM programs the Lamella RHU build runs on this
host, the linker `ld.lld` and `llvm-objcopy`, taken unchanged from the LLVM
release named in `TOOLS.txt`; the build checks both against that record before
every link. `Lamella.Sdk` restores the package for the host the build runs on,
by its runtime identifier (`win-x64`, `win-arm64`, `linux-x64`, `linux-arm64` or
`osx-arm64`); a program never references it directly. There is no package for
macOS on Intel processors.

License: your choice of the MIT license or the Apache License 2.0; see `NOTICE.txt`
for third-party components and their corresponding licenses.
