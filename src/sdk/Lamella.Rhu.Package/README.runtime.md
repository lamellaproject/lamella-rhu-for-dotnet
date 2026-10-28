# Lamella RHU for .NET: a runtime package

This package carries what every board of one processor class (named by the package
id) links into its image: the .NET runtime's native libraries built for that class,
the C library and the other prebuilt objects shared by the class's boards, and the
part of the Lamella RHU kernel those boards have in common. `Lamella.Sdk` restores
it when a `net11.0` project names a board of the class in `LamellaTargetBoard`,
together with the board package and the tools package for the host; a program never
references it directly.

License: your choice of the MIT license or the Apache License 2.0; see `NOTICE.txt`
for third-party components and their corresponding licenses.
