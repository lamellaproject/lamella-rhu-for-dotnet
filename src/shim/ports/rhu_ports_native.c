/* The stock .NET serial layer, compiled unchanged: the
 * three source files of the runtime's System.IO.Ports.Native library,
 * copied byte for byte under src/vendor/ports/ and listed with their
 * digests in src/VENDORED.md, brought into one translation unit here so
 * one object per processor class carries the layer. Nothing of the
 * layer is rewritten: the nineteen SystemIoPortsNative_* entry points the
 * standard serial port class binds at link time are the upstream bodies,
 * and every call they make -- open, close, read, write, poll, ioctl,
 * shutdown, the termios family, the error string -- lands on the shim's
 * serial device unit (rhu_ttydev.c) through the C library's own names.
 *
 * The configuration they compile under is the shim's: pal_config.h beside
 * this file answers the upstream build's platform probes, and the platform
 * headers beside it (termios.h, sys/ioctl.h, asm/termbits.h, asm/ioctls.h,
 * poll.h, sys/socket.h, netdb.h, errno.h) carry the Linux ARM32 layouts and
 * numbers the layer expects, which the kit's C library lacks or holds with
 * other values. The include path puts this directory first, then the three
 * vendored directories, then the kit's; NDEBUG is defined as upstream's
 * release build defines it. The three files' file-scope names do not
 * collide, and each of the two headers without an include guard is
 * included once. */
#include "pal_termios.c"
#include "pal_serial.c"
#include "pal_termios2.c"
