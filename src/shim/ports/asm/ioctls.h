/* The Linux termios2 requests the stock .NET serial layer's custom-rate
 * path uses: the generic numbering (asm-generic
 * ioctls.h), which the ARM port takes -- the direction, size, type and
 * number words of the kernel's request encoding over the 44-byte termios2
 * record. The other terminal requests are the C library's, taken from the
 * ioctl header beside this one. */
#ifndef RHU_PORTS_ASM_IOCTLS_H
#define RHU_PORTS_ASM_IOCTLS_H

#include "../sys/ioctl.h"

#define TCGETS2 0x802C542A
#define TCSETS2 0x402C542B
#define TCSETSW2 0x402C542C
#define TCSETSF2 0x402C542D

#endif /* RHU_PORTS_ASM_IOCTLS_H */
