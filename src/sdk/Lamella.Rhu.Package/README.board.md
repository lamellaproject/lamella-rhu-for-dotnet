# Lamella RHU for .NET: a board package

This package carries the board support package for the board: the board's
prebuilt kernel in one or more configurations, its linker maps, its facts
file and its prebuilt objects.

`Lamella.Sdk` restores it when a `net11.0` project names the board id in
`LamellaTargetBoard`, together with the runtime package for the board's processor
class and the tools package for the host; a program never references it directly.

The kernel archives carry no debug information. The symbols package of the
same board carries the same archives with their debug information, for a
debugger of the kernel.

License: your choice of the MIT license or the Apache License 2.0; see `NOTICE.txt`.
