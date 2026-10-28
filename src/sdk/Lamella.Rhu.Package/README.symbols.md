# Lamella RHU for .NET: a symbols package

This package carries the Lamella RHU kernel archives for the board named by its
package id with their debug information, laid out as the board package lays out
its own--and the part of the kernel shared by the board's processor class the
same way. It is for debugging the kernel: `Lamella.Sdk` does not restore it for a
program build, and a program never references it.

License: your choice of the MIT license or the Apache License 2.0; see `NOTICE.txt`.
