# Lamella RHU for .NET: a symbols package

This package carries the Lamella RHU kernel for a board with its debug
symbols, matching the board support package's layout; portions of the
kernel which are shared by the board's processor class are also included.

It is for debugging the kernel: `Lamella.Sdk` does not restore it for a program
build, and a program never references it.

License: your choice of the MIT license or the Apache License 2.0; see `NOTICE.txt`.
