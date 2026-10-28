/* The Linux kernel's termios2 record and the custom-rate bits the stock
 * .NET serial layer's custom-rate path uses: the
 * generic layout (asm-generic termbits.h), which the ARM port takes --
 * four flag words, the line discipline byte, nineteen control characters
 * and the two speed words, 44 bytes. The flag bits and speed constants are
 * the C library's, taken from the termios header beside this one. */
#ifndef RHU_PORTS_ASM_TERMBITS_H
#define RHU_PORTS_ASM_TERMBITS_H

#include "../termios.h"

#define RHU_KERNEL_NCCS 19
struct termios2 {
    tcflag_t c_iflag;
    tcflag_t c_oflag;
    tcflag_t c_cflag;
    tcflag_t c_lflag;
    cc_t c_line;
    cc_t c_cc[RHU_KERNEL_NCCS];
    speed_t c_ispeed;
    speed_t c_ospeed;
};

#define BOTHER 0010000
#define IBSHIFT 16

#endif /* RHU_PORTS_ASM_TERMBITS_H */
