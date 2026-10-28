# Lamella RHU for .NET: the build package

Lamella RHU is a unikernel whose one job is hosting the .NET runtime on
microcontrollers. This package carries the MSBuild properties and targets that
build a .NET program into an image for a named board. The program compiles ahead
of time (NativeAOT) for the `linux-arm` runtime identifier, and the LLVM linker
from the tools package for your host links the compiled object with the board's
prebuilt kernel, the runtime's native libraries and the prebuilt objects for the
board's processor class (using the board's linker map).

The build runs on Windows (x64 and Arm64), Linux (x64 and Arm64) and macOS on
Apple silicon.

License: your choice of the MIT license or the Apache License 2.0; see `NOTICE.txt`.
