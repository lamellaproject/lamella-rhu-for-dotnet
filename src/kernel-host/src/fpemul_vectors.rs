pub const DECODE_VECTORS: &[(u16, u16, &str, u8, u8, u8, u8, u8)] = &[
    (0xee31, 0x0b02, "vadd.f64", 0, 1, 2, 255, 255), // vadd.f64 d0, d1, d2
    (0xee35, 0x3b09, "vadd.f64", 3, 5, 9, 255, 255), // vadd.f64 d3, d5, d9
    (0xee3e, 0xfb0d, "vadd.f64", 15, 14, 13, 255, 255), // vadd.f64 d15, d14, d13
    (0xee37, 0x7b07, "vadd.f64", 7, 7, 7, 255, 255), // vadd.f64 d7, d7, d7
    (0xee30, 0x8b0f, "vadd.f64", 8, 0, 15, 255, 255), // vadd.f64 d8, d0, d15
    (0xee3c, 0x2b04, "vadd.f64", 2, 12, 4, 255, 255), // vadd.f64 d2, d12, d4
    (0xee31, 0x0b42, "vsub.f64", 0, 1, 2, 255, 255), // vsub.f64 d0, d1, d2
    (0xee35, 0x3b49, "vsub.f64", 3, 5, 9, 255, 255), // vsub.f64 d3, d5, d9
    (0xee3e, 0xfb4d, "vsub.f64", 15, 14, 13, 255, 255), // vsub.f64 d15, d14, d13
    (0xee37, 0x7b47, "vsub.f64", 7, 7, 7, 255, 255), // vsub.f64 d7, d7, d7
    (0xee30, 0x8b4f, "vsub.f64", 8, 0, 15, 255, 255), // vsub.f64 d8, d0, d15
    (0xee3c, 0x2b44, "vsub.f64", 2, 12, 4, 255, 255), // vsub.f64 d2, d12, d4
    (0xee21, 0x0b02, "vmul.f64", 0, 1, 2, 255, 255), // vmul.f64 d0, d1, d2
    (0xee25, 0x3b09, "vmul.f64", 3, 5, 9, 255, 255), // vmul.f64 d3, d5, d9
    (0xee2e, 0xfb0d, "vmul.f64", 15, 14, 13, 255, 255), // vmul.f64 d15, d14, d13
    (0xee27, 0x7b07, "vmul.f64", 7, 7, 7, 255, 255), // vmul.f64 d7, d7, d7
    (0xee20, 0x8b0f, "vmul.f64", 8, 0, 15, 255, 255), // vmul.f64 d8, d0, d15
    (0xee2c, 0x2b04, "vmul.f64", 2, 12, 4, 255, 255), // vmul.f64 d2, d12, d4
    (0xee21, 0x0b42, "vnmul.f64", 0, 1, 2, 255, 255), // vnmul.f64 d0, d1, d2
    (0xee25, 0x3b49, "vnmul.f64", 3, 5, 9, 255, 255), // vnmul.f64 d3, d5, d9
    (0xee2e, 0xfb4d, "vnmul.f64", 15, 14, 13, 255, 255), // vnmul.f64 d15, d14, d13
    (0xee27, 0x7b47, "vnmul.f64", 7, 7, 7, 255, 255), // vnmul.f64 d7, d7, d7
    (0xee20, 0x8b4f, "vnmul.f64", 8, 0, 15, 255, 255), // vnmul.f64 d8, d0, d15
    (0xee2c, 0x2b44, "vnmul.f64", 2, 12, 4, 255, 255), // vnmul.f64 d2, d12, d4
    (0xee81, 0x0b02, "vdiv.f64", 0, 1, 2, 255, 255), // vdiv.f64 d0, d1, d2
    (0xee85, 0x3b09, "vdiv.f64", 3, 5, 9, 255, 255), // vdiv.f64 d3, d5, d9
    (0xee8e, 0xfb0d, "vdiv.f64", 15, 14, 13, 255, 255), // vdiv.f64 d15, d14, d13
    (0xee87, 0x7b07, "vdiv.f64", 7, 7, 7, 255, 255), // vdiv.f64 d7, d7, d7
    (0xee80, 0x8b0f, "vdiv.f64", 8, 0, 15, 255, 255), // vdiv.f64 d8, d0, d15
    (0xee8c, 0x2b04, "vdiv.f64", 2, 12, 4, 255, 255), // vdiv.f64 d2, d12, d4
    (0xee01, 0x0b02, "vmla.f64", 0, 1, 2, 255, 255), // vmla.f64 d0, d1, d2
    (0xee05, 0x3b09, "vmla.f64", 3, 5, 9, 255, 255), // vmla.f64 d3, d5, d9
    (0xee0e, 0xfb0d, "vmla.f64", 15, 14, 13, 255, 255), // vmla.f64 d15, d14, d13
    (0xee07, 0x7b07, "vmla.f64", 7, 7, 7, 255, 255), // vmla.f64 d7, d7, d7
    (0xee00, 0x8b0f, "vmla.f64", 8, 0, 15, 255, 255), // vmla.f64 d8, d0, d15
    (0xee0c, 0x2b04, "vmla.f64", 2, 12, 4, 255, 255), // vmla.f64 d2, d12, d4
    (0xee01, 0x0b42, "vmls.f64", 0, 1, 2, 255, 255), // vmls.f64 d0, d1, d2
    (0xee05, 0x3b49, "vmls.f64", 3, 5, 9, 255, 255), // vmls.f64 d3, d5, d9
    (0xee0e, 0xfb4d, "vmls.f64", 15, 14, 13, 255, 255), // vmls.f64 d15, d14, d13
    (0xee07, 0x7b47, "vmls.f64", 7, 7, 7, 255, 255), // vmls.f64 d7, d7, d7
    (0xee00, 0x8b4f, "vmls.f64", 8, 0, 15, 255, 255), // vmls.f64 d8, d0, d15
    (0xee0c, 0x2b44, "vmls.f64", 2, 12, 4, 255, 255), // vmls.f64 d2, d12, d4
    (0xee11, 0x0b42, "vnmla.f64", 0, 1, 2, 255, 255), // vnmla.f64 d0, d1, d2
    (0xee15, 0x3b49, "vnmla.f64", 3, 5, 9, 255, 255), // vnmla.f64 d3, d5, d9
    (0xee1e, 0xfb4d, "vnmla.f64", 15, 14, 13, 255, 255), // vnmla.f64 d15, d14, d13
    (0xee17, 0x7b47, "vnmla.f64", 7, 7, 7, 255, 255), // vnmla.f64 d7, d7, d7
    (0xee10, 0x8b4f, "vnmla.f64", 8, 0, 15, 255, 255), // vnmla.f64 d8, d0, d15
    (0xee1c, 0x2b44, "vnmla.f64", 2, 12, 4, 255, 255), // vnmla.f64 d2, d12, d4
    (0xee11, 0x0b02, "vnmls.f64", 0, 1, 2, 255, 255), // vnmls.f64 d0, d1, d2
    (0xee15, 0x3b09, "vnmls.f64", 3, 5, 9, 255, 255), // vnmls.f64 d3, d5, d9
    (0xee1e, 0xfb0d, "vnmls.f64", 15, 14, 13, 255, 255), // vnmls.f64 d15, d14, d13
    (0xee17, 0x7b07, "vnmls.f64", 7, 7, 7, 255, 255), // vnmls.f64 d7, d7, d7
    (0xee10, 0x8b0f, "vnmls.f64", 8, 0, 15, 255, 255), // vnmls.f64 d8, d0, d15
    (0xee1c, 0x2b04, "vnmls.f64", 2, 12, 4, 255, 255), // vnmls.f64 d2, d12, d4
    (0xeea1, 0x0b02, "vfma.f64", 0, 1, 2, 255, 255), // vfma.f64 d0, d1, d2
    (0xeea5, 0x3b09, "vfma.f64", 3, 5, 9, 255, 255), // vfma.f64 d3, d5, d9
    (0xeeae, 0xfb0d, "vfma.f64", 15, 14, 13, 255, 255), // vfma.f64 d15, d14, d13
    (0xeea7, 0x7b07, "vfma.f64", 7, 7, 7, 255, 255), // vfma.f64 d7, d7, d7
    (0xeea0, 0x8b0f, "vfma.f64", 8, 0, 15, 255, 255), // vfma.f64 d8, d0, d15
    (0xeeac, 0x2b04, "vfma.f64", 2, 12, 4, 255, 255), // vfma.f64 d2, d12, d4
    (0xeea1, 0x0b42, "vfms.f64", 0, 1, 2, 255, 255), // vfms.f64 d0, d1, d2
    (0xeea5, 0x3b49, "vfms.f64", 3, 5, 9, 255, 255), // vfms.f64 d3, d5, d9
    (0xeeae, 0xfb4d, "vfms.f64", 15, 14, 13, 255, 255), // vfms.f64 d15, d14, d13
    (0xeea7, 0x7b47, "vfms.f64", 7, 7, 7, 255, 255), // vfms.f64 d7, d7, d7
    (0xeea0, 0x8b4f, "vfms.f64", 8, 0, 15, 255, 255), // vfms.f64 d8, d0, d15
    (0xeeac, 0x2b44, "vfms.f64", 2, 12, 4, 255, 255), // vfms.f64 d2, d12, d4
    (0xee91, 0x0b42, "vfnma.f64", 0, 1, 2, 255, 255), // vfnma.f64 d0, d1, d2
    (0xee95, 0x3b49, "vfnma.f64", 3, 5, 9, 255, 255), // vfnma.f64 d3, d5, d9
    (0xee9e, 0xfb4d, "vfnma.f64", 15, 14, 13, 255, 255), // vfnma.f64 d15, d14, d13
    (0xee97, 0x7b47, "vfnma.f64", 7, 7, 7, 255, 255), // vfnma.f64 d7, d7, d7
    (0xee90, 0x8b4f, "vfnma.f64", 8, 0, 15, 255, 255), // vfnma.f64 d8, d0, d15
    (0xee9c, 0x2b44, "vfnma.f64", 2, 12, 4, 255, 255), // vfnma.f64 d2, d12, d4
    (0xee91, 0x0b02, "vfnms.f64", 0, 1, 2, 255, 255), // vfnms.f64 d0, d1, d2
    (0xee95, 0x3b09, "vfnms.f64", 3, 5, 9, 255, 255), // vfnms.f64 d3, d5, d9
    (0xee9e, 0xfb0d, "vfnms.f64", 15, 14, 13, 255, 255), // vfnms.f64 d15, d14, d13
    (0xee97, 0x7b07, "vfnms.f64", 7, 7, 7, 255, 255), // vfnms.f64 d7, d7, d7
    (0xee90, 0x8b0f, "vfnms.f64", 8, 0, 15, 255, 255), // vfnms.f64 d8, d0, d15
    (0xee9c, 0x2b04, "vfnms.f64", 2, 12, 4, 255, 255), // vfnms.f64 d2, d12, d4
    (0xfe81, 0x0b02, "vmaxnm.f64", 0, 1, 2, 255, 255), // vmaxnm.f64 d0, d1, d2
    (0xfe85, 0x3b09, "vmaxnm.f64", 3, 5, 9, 255, 255), // vmaxnm.f64 d3, d5, d9
    (0xfe8e, 0xfb0d, "vmaxnm.f64", 15, 14, 13, 255, 255), // vmaxnm.f64 d15, d14, d13
    (0xfe87, 0x7b07, "vmaxnm.f64", 7, 7, 7, 255, 255), // vmaxnm.f64 d7, d7, d7
    (0xfe80, 0x8b0f, "vmaxnm.f64", 8, 0, 15, 255, 255), // vmaxnm.f64 d8, d0, d15
    (0xfe8c, 0x2b04, "vmaxnm.f64", 2, 12, 4, 255, 255), // vmaxnm.f64 d2, d12, d4
    (0xfe81, 0x0b42, "vminnm.f64", 0, 1, 2, 255, 255), // vminnm.f64 d0, d1, d2
    (0xfe85, 0x3b49, "vminnm.f64", 3, 5, 9, 255, 255), // vminnm.f64 d3, d5, d9
    (0xfe8e, 0xfb4d, "vminnm.f64", 15, 14, 13, 255, 255), // vminnm.f64 d15, d14, d13
    (0xfe87, 0x7b47, "vminnm.f64", 7, 7, 7, 255, 255), // vminnm.f64 d7, d7, d7
    (0xfe80, 0x8b4f, "vminnm.f64", 8, 0, 15, 255, 255), // vminnm.f64 d8, d0, d15
    (0xfe8c, 0x2b44, "vminnm.f64", 2, 12, 4, 255, 255), // vminnm.f64 d2, d12, d4
    (0xfe02, 0x1b03, "vsel.f64", 1, 2, 3, 255, 255), // vseleq.f64 d1, d2, d3
    (0xfe00, 0xfb08, "vsel.f64", 15, 0, 8, 255, 255), // vseleq.f64 d15, d0, d8
    (0xfe0b, 0x6b0c, "vsel.f64", 6, 11, 12, 255, 255), // vseleq.f64 d6, d11, d12
    (0xfe12, 0x1b03, "vsel.f64", 1, 2, 3, 255, 255), // vselvs.f64 d1, d2, d3
    (0xfe10, 0xfb08, "vsel.f64", 15, 0, 8, 255, 255), // vselvs.f64 d15, d0, d8
    (0xfe1b, 0x6b0c, "vsel.f64", 6, 11, 12, 255, 255), // vselvs.f64 d6, d11, d12
    (0xfe22, 0x1b03, "vsel.f64", 1, 2, 3, 255, 255), // vselge.f64 d1, d2, d3
    (0xfe20, 0xfb08, "vsel.f64", 15, 0, 8, 255, 255), // vselge.f64 d15, d0, d8
    (0xfe2b, 0x6b0c, "vsel.f64", 6, 11, 12, 255, 255), // vselge.f64 d6, d11, d12
    (0xfe32, 0x1b03, "vsel.f64", 1, 2, 3, 255, 255), // vselgt.f64 d1, d2, d3
    (0xfe30, 0xfb08, "vsel.f64", 15, 0, 8, 255, 255), // vselgt.f64 d15, d0, d8
    (0xfe3b, 0x6b0c, "vsel.f64", 6, 11, 12, 255, 255), // vselgt.f64 d6, d11, d12
    (0xeeb0, 0x0b41, "vmov.f64", 0, 255, 1, 255, 255), // vmov.f64 d0, d1
    (0xeeb0, 0xfb42, "vmov.f64", 15, 255, 2, 255, 255), // vmov.f64 d15, d2
    (0xeeb0, 0x9b4f, "vmov.f64", 9, 255, 15, 255, 255), // vmov.f64 d9, d15
    (0xeeb0, 0x4b44, "vmov.f64", 4, 255, 4, 255, 255), // vmov.f64 d4, d4
    (0xeeb0, 0xcb47, "vmov.f64", 12, 255, 7, 255, 255), // vmov.f64 d12, d7
    (0xeeb0, 0x0bc1, "vabs.f64", 0, 255, 1, 255, 255), // vabs.f64 d0, d1
    (0xeeb0, 0xfbc2, "vabs.f64", 15, 255, 2, 255, 255), // vabs.f64 d15, d2
    (0xeeb0, 0x9bcf, "vabs.f64", 9, 255, 15, 255, 255), // vabs.f64 d9, d15
    (0xeeb0, 0x4bc4, "vabs.f64", 4, 255, 4, 255, 255), // vabs.f64 d4, d4
    (0xeeb0, 0xcbc7, "vabs.f64", 12, 255, 7, 255, 255), // vabs.f64 d12, d7
    (0xeeb1, 0x0b41, "vneg.f64", 0, 255, 1, 255, 255), // vneg.f64 d0, d1
    (0xeeb1, 0xfb42, "vneg.f64", 15, 255, 2, 255, 255), // vneg.f64 d15, d2
    (0xeeb1, 0x9b4f, "vneg.f64", 9, 255, 15, 255, 255), // vneg.f64 d9, d15
    (0xeeb1, 0x4b44, "vneg.f64", 4, 255, 4, 255, 255), // vneg.f64 d4, d4
    (0xeeb1, 0xcb47, "vneg.f64", 12, 255, 7, 255, 255), // vneg.f64 d12, d7
    (0xeeb1, 0x0bc1, "vsqrt.f64", 0, 255, 1, 255, 255), // vsqrt.f64 d0, d1
    (0xeeb1, 0xfbc2, "vsqrt.f64", 15, 255, 2, 255, 255), // vsqrt.f64 d15, d2
    (0xeeb1, 0x9bcf, "vsqrt.f64", 9, 255, 15, 255, 255), // vsqrt.f64 d9, d15
    (0xeeb1, 0x4bc4, "vsqrt.f64", 4, 255, 4, 255, 255), // vsqrt.f64 d4, d4
    (0xeeb1, 0xcbc7, "vsqrt.f64", 12, 255, 7, 255, 255), // vsqrt.f64 d12, d7
    (0xeeb6, 0x0b41, "vrintr.f64", 0, 255, 1, 255, 255), // vrintr.f64 d0, d1
    (0xeeb6, 0xfb42, "vrintr.f64", 15, 255, 2, 255, 255), // vrintr.f64 d15, d2
    (0xeeb6, 0x9b4f, "vrintr.f64", 9, 255, 15, 255, 255), // vrintr.f64 d9, d15
    (0xeeb6, 0x4b44, "vrintr.f64", 4, 255, 4, 255, 255), // vrintr.f64 d4, d4
    (0xeeb6, 0xcb47, "vrintr.f64", 12, 255, 7, 255, 255), // vrintr.f64 d12, d7
    (0xeeb7, 0x0b41, "vrintx.f64", 0, 255, 1, 255, 255), // vrintx.f64 d0, d1
    (0xeeb7, 0xfb42, "vrintx.f64", 15, 255, 2, 255, 255), // vrintx.f64 d15, d2
    (0xeeb7, 0x9b4f, "vrintx.f64", 9, 255, 15, 255, 255), // vrintx.f64 d9, d15
    (0xeeb7, 0x4b44, "vrintx.f64", 4, 255, 4, 255, 255), // vrintx.f64 d4, d4
    (0xeeb7, 0xcb47, "vrintx.f64", 12, 255, 7, 255, 255), // vrintx.f64 d12, d7
    (0xeeb6, 0x0bc1, "vrintz.f64", 0, 255, 1, 255, 255), // vrintz.f64 d0, d1
    (0xeeb6, 0xfbc2, "vrintz.f64", 15, 255, 2, 255, 255), // vrintz.f64 d15, d2
    (0xeeb6, 0x9bcf, "vrintz.f64", 9, 255, 15, 255, 255), // vrintz.f64 d9, d15
    (0xeeb6, 0x4bc4, "vrintz.f64", 4, 255, 4, 255, 255), // vrintz.f64 d4, d4
    (0xeeb6, 0xcbc7, "vrintz.f64", 12, 255, 7, 255, 255), // vrintz.f64 d12, d7
    (0xfeb8, 0x0b41, "vrinta.f64", 0, 255, 1, 255, 255), // vrinta.f64 d0, d1
    (0xfeb8, 0xfb42, "vrinta.f64", 15, 255, 2, 255, 255), // vrinta.f64 d15, d2
    (0xfeb8, 0x9b4f, "vrinta.f64", 9, 255, 15, 255, 255), // vrinta.f64 d9, d15
    (0xfeb8, 0x4b44, "vrinta.f64", 4, 255, 4, 255, 255), // vrinta.f64 d4, d4
    (0xfeb8, 0xcb47, "vrinta.f64", 12, 255, 7, 255, 255), // vrinta.f64 d12, d7
    (0xfeb9, 0x0b41, "vrintn.f64", 0, 255, 1, 255, 255), // vrintn.f64 d0, d1
    (0xfeb9, 0xfb42, "vrintn.f64", 15, 255, 2, 255, 255), // vrintn.f64 d15, d2
    (0xfeb9, 0x9b4f, "vrintn.f64", 9, 255, 15, 255, 255), // vrintn.f64 d9, d15
    (0xfeb9, 0x4b44, "vrintn.f64", 4, 255, 4, 255, 255), // vrintn.f64 d4, d4
    (0xfeb9, 0xcb47, "vrintn.f64", 12, 255, 7, 255, 255), // vrintn.f64 d12, d7
    (0xfeba, 0x0b41, "vrintp.f64", 0, 255, 1, 255, 255), // vrintp.f64 d0, d1
    (0xfeba, 0xfb42, "vrintp.f64", 15, 255, 2, 255, 255), // vrintp.f64 d15, d2
    (0xfeba, 0x9b4f, "vrintp.f64", 9, 255, 15, 255, 255), // vrintp.f64 d9, d15
    (0xfeba, 0x4b44, "vrintp.f64", 4, 255, 4, 255, 255), // vrintp.f64 d4, d4
    (0xfeba, 0xcb47, "vrintp.f64", 12, 255, 7, 255, 255), // vrintp.f64 d12, d7
    (0xfebb, 0x0b41, "vrintm.f64", 0, 255, 1, 255, 255), // vrintm.f64 d0, d1
    (0xfebb, 0xfb42, "vrintm.f64", 15, 255, 2, 255, 255), // vrintm.f64 d15, d2
    (0xfebb, 0x9b4f, "vrintm.f64", 9, 255, 15, 255, 255), // vrintm.f64 d9, d15
    (0xfebb, 0x4b44, "vrintm.f64", 4, 255, 4, 255, 255), // vrintm.f64 d4, d4
    (0xfebb, 0xcb47, "vrintm.f64", 12, 255, 7, 255, 255), // vrintm.f64 d12, d7
    (0xeeb4, 0x0b41, "vcmp.f64", 0, 255, 1, 255, 255), // vcmp.f64 d0, d1
    (0xeeb4, 0xfb43, "vcmp.f64", 15, 255, 3, 255, 255), // vcmp.f64 d15, d3
    (0xeeb4, 0xab4f, "vcmp.f64", 10, 255, 15, 255, 255), // vcmp.f64 d10, d15
    (0xeeb4, 0x5b45, "vcmp.f64", 5, 255, 5, 255, 255), // vcmp.f64 d5, d5
    (0xeeb5, 0x0b40, "vcmp.f64", 0, 255, 255, 255, 255), // vcmp.f64 d0, #0
    (0xeeb5, 0xfb40, "vcmp.f64", 15, 255, 255, 255, 255), // vcmp.f64 d15, #0
    (0xeeb5, 0x7b40, "vcmp.f64", 7, 255, 255, 255, 255), // vcmp.f64 d7, #0
    (0xeeb4, 0x0bc1, "vcmpe.f64", 0, 255, 1, 255, 255), // vcmpe.f64 d0, d1
    (0xeeb4, 0xfbc3, "vcmpe.f64", 15, 255, 3, 255, 255), // vcmpe.f64 d15, d3
    (0xeeb4, 0xabcf, "vcmpe.f64", 10, 255, 15, 255, 255), // vcmpe.f64 d10, d15
    (0xeeb4, 0x5bc5, "vcmpe.f64", 5, 255, 5, 255, 255), // vcmpe.f64 d5, d5
    (0xeeb5, 0x0bc0, "vcmpe.f64", 0, 255, 255, 255, 255), // vcmpe.f64 d0, #0
    (0xeeb5, 0xfbc0, "vcmpe.f64", 15, 255, 255, 255, 255), // vcmpe.f64 d15, #0
    (0xeeb5, 0x7bc0, "vcmpe.f64", 7, 255, 255, 255, 255), // vcmpe.f64 d7, #0
    (0xeeb7, 0x0bc1, "vcvt.f32.f64", 255, 255, 1, 0, 255), // vcvt.f32.f64 s0, d1
    (0xeef7, 0xfbcf, "vcvt.f32.f64", 255, 255, 15, 31, 255), // vcvt.f32.f64 s31, d15
    (0xeef7, 0x8bc0, "vcvt.f32.f64", 255, 255, 0, 17, 255), // vcvt.f32.f64 s17, d0
    (0xeeb7, 0x2bc9, "vcvt.f32.f64", 255, 255, 9, 4, 255), // vcvt.f32.f64 s4, d9
    (0xeebd, 0x0bc1, "vcvt.s32.f64", 255, 255, 1, 0, 255), // vcvt.s32.f64 s0, d1
    (0xeefd, 0xfbcf, "vcvt.s32.f64", 255, 255, 15, 31, 255), // vcvt.s32.f64 s31, d15
    (0xeefd, 0x8bc0, "vcvt.s32.f64", 255, 255, 0, 17, 255), // vcvt.s32.f64 s17, d0
    (0xeebd, 0x2bc9, "vcvt.s32.f64", 255, 255, 9, 4, 255), // vcvt.s32.f64 s4, d9
    (0xeebc, 0x0bc1, "vcvt.u32.f64", 255, 255, 1, 0, 255), // vcvt.u32.f64 s0, d1
    (0xeefc, 0xfbcf, "vcvt.u32.f64", 255, 255, 15, 31, 255), // vcvt.u32.f64 s31, d15
    (0xeefc, 0x8bc0, "vcvt.u32.f64", 255, 255, 0, 17, 255), // vcvt.u32.f64 s17, d0
    (0xeebc, 0x2bc9, "vcvt.u32.f64", 255, 255, 9, 4, 255), // vcvt.u32.f64 s4, d9
    (0xeebd, 0x0b41, "vcvtr.s32.f64", 255, 255, 1, 0, 255), // vcvtr.s32.f64 s0, d1
    (0xeefd, 0xfb4f, "vcvtr.s32.f64", 255, 255, 15, 31, 255), // vcvtr.s32.f64 s31, d15
    (0xeefd, 0x8b40, "vcvtr.s32.f64", 255, 255, 0, 17, 255), // vcvtr.s32.f64 s17, d0
    (0xeebd, 0x2b49, "vcvtr.s32.f64", 255, 255, 9, 4, 255), // vcvtr.s32.f64 s4, d9
    (0xeebc, 0x0b41, "vcvtr.u32.f64", 255, 255, 1, 0, 255), // vcvtr.u32.f64 s0, d1
    (0xeefc, 0xfb4f, "vcvtr.u32.f64", 255, 255, 15, 31, 255), // vcvtr.u32.f64 s31, d15
    (0xeefc, 0x8b40, "vcvtr.u32.f64", 255, 255, 0, 17, 255), // vcvtr.u32.f64 s17, d0
    (0xeebc, 0x2b49, "vcvtr.u32.f64", 255, 255, 9, 4, 255), // vcvtr.u32.f64 s4, d9
    (0xfebc, 0x0bc1, "vcvt{a,n,p,m}.s32.f64", 255, 255, 1, 0, 255), // vcvta.s32.f64 s0, d1
    (0xfefc, 0xfbcf, "vcvt{a,n,p,m}.s32.f64", 255, 255, 15, 31, 255), // vcvta.s32.f64 s31, d15
    (0xfefc, 0x8bc0, "vcvt{a,n,p,m}.s32.f64", 255, 255, 0, 17, 255), // vcvta.s32.f64 s17, d0
    (0xfebc, 0x2bc9, "vcvt{a,n,p,m}.s32.f64", 255, 255, 9, 4, 255), // vcvta.s32.f64 s4, d9
    (0xfebd, 0x0bc1, "vcvt{a,n,p,m}.s32.f64", 255, 255, 1, 0, 255), // vcvtn.s32.f64 s0, d1
    (0xfefd, 0xfbcf, "vcvt{a,n,p,m}.s32.f64", 255, 255, 15, 31, 255), // vcvtn.s32.f64 s31, d15
    (0xfefd, 0x8bc0, "vcvt{a,n,p,m}.s32.f64", 255, 255, 0, 17, 255), // vcvtn.s32.f64 s17, d0
    (0xfebd, 0x2bc9, "vcvt{a,n,p,m}.s32.f64", 255, 255, 9, 4, 255), // vcvtn.s32.f64 s4, d9
    (0xfebe, 0x0bc1, "vcvt{a,n,p,m}.s32.f64", 255, 255, 1, 0, 255), // vcvtp.s32.f64 s0, d1
    (0xfefe, 0xfbcf, "vcvt{a,n,p,m}.s32.f64", 255, 255, 15, 31, 255), // vcvtp.s32.f64 s31, d15
    (0xfefe, 0x8bc0, "vcvt{a,n,p,m}.s32.f64", 255, 255, 0, 17, 255), // vcvtp.s32.f64 s17, d0
    (0xfebe, 0x2bc9, "vcvt{a,n,p,m}.s32.f64", 255, 255, 9, 4, 255), // vcvtp.s32.f64 s4, d9
    (0xfebf, 0x0bc1, "vcvt{a,n,p,m}.s32.f64", 255, 255, 1, 0, 255), // vcvtm.s32.f64 s0, d1
    (0xfeff, 0xfbcf, "vcvt{a,n,p,m}.s32.f64", 255, 255, 15, 31, 255), // vcvtm.s32.f64 s31, d15
    (0xfeff, 0x8bc0, "vcvt{a,n,p,m}.s32.f64", 255, 255, 0, 17, 255), // vcvtm.s32.f64 s17, d0
    (0xfebf, 0x2bc9, "vcvt{a,n,p,m}.s32.f64", 255, 255, 9, 4, 255), // vcvtm.s32.f64 s4, d9
    (0xfebc, 0x0b41, "vcvt{a,n,p,m}.u32.f64", 255, 255, 1, 0, 255), // vcvta.u32.f64 s0, d1
    (0xfefc, 0xfb4f, "vcvt{a,n,p,m}.u32.f64", 255, 255, 15, 31, 255), // vcvta.u32.f64 s31, d15
    (0xfefc, 0x8b40, "vcvt{a,n,p,m}.u32.f64", 255, 255, 0, 17, 255), // vcvta.u32.f64 s17, d0
    (0xfebc, 0x2b49, "vcvt{a,n,p,m}.u32.f64", 255, 255, 9, 4, 255), // vcvta.u32.f64 s4, d9
    (0xfebf, 0x0b41, "vcvt{a,n,p,m}.u32.f64", 255, 255, 1, 0, 255), // vcvtm.u32.f64 s0, d1
    (0xfeff, 0xfb4f, "vcvt{a,n,p,m}.u32.f64", 255, 255, 15, 31, 255), // vcvtm.u32.f64 s31, d15
    (0xfeff, 0x8b40, "vcvt{a,n,p,m}.u32.f64", 255, 255, 0, 17, 255), // vcvtm.u32.f64 s17, d0
    (0xfebf, 0x2b49, "vcvt{a,n,p,m}.u32.f64", 255, 255, 9, 4, 255), // vcvtm.u32.f64 s4, d9
    (0xeeb3, 0x0b41, "vcvtb.f16.f64", 255, 255, 1, 0, 255), // vcvtb.f16.f64 s0, d1
    (0xeef3, 0xfb4f, "vcvtb.f16.f64", 255, 255, 15, 31, 255), // vcvtb.f16.f64 s31, d15
    (0xeef3, 0x8b40, "vcvtb.f16.f64", 255, 255, 0, 17, 255), // vcvtb.f16.f64 s17, d0
    (0xeeb3, 0x2b49, "vcvtb.f16.f64", 255, 255, 9, 4, 255), // vcvtb.f16.f64 s4, d9
    (0xeeb3, 0x0bc1, "vcvtt.f16.f64", 255, 255, 1, 0, 255), // vcvtt.f16.f64 s0, d1
    (0xeef3, 0xfbcf, "vcvtt.f16.f64", 255, 255, 15, 31, 255), // vcvtt.f16.f64 s31, d15
    (0xeef3, 0x8bc0, "vcvtt.f16.f64", 255, 255, 0, 17, 255), // vcvtt.f16.f64 s17, d0
    (0xeeb3, 0x2bc9, "vcvtt.f16.f64", 255, 255, 9, 4, 255), // vcvtt.f16.f64 s4, d9
    (0xeeb7, 0x0ae0, "vcvt.f64.f32", 0, 255, 255, 255, 1), // vcvt.f64.f32 d0, s1
    (0xeeb7, 0xfaef, "vcvt.f64.f32", 15, 255, 255, 255, 31), // vcvt.f64.f32 d15, s31
    (0xeeb7, 0x8ac8, "vcvt.f64.f32", 8, 255, 255, 255, 16), // vcvt.f64.f32 d8, s16
    (0xeeb7, 0x3acb, "vcvt.f64.f32", 3, 255, 255, 255, 22), // vcvt.f64.f32 d3, s22
    (0xeeb8, 0x0be0, "vcvt.f64.s32", 0, 255, 255, 255, 1), // vcvt.f64.s32 d0, s1
    (0xeeb8, 0xfbef, "vcvt.f64.s32", 15, 255, 255, 255, 31), // vcvt.f64.s32 d15, s31
    (0xeeb8, 0x8bc8, "vcvt.f64.s32", 8, 255, 255, 255, 16), // vcvt.f64.s32 d8, s16
    (0xeeb8, 0x3bcb, "vcvt.f64.s32", 3, 255, 255, 255, 22), // vcvt.f64.s32 d3, s22
    (0xeeb8, 0x0b60, "vcvt.f64.u32", 0, 255, 255, 255, 1), // vcvt.f64.u32 d0, s1
    (0xeeb8, 0xfb6f, "vcvt.f64.u32", 15, 255, 255, 255, 31), // vcvt.f64.u32 d15, s31
    (0xeeb8, 0x8b48, "vcvt.f64.u32", 8, 255, 255, 255, 16), // vcvt.f64.u32 d8, s16
    (0xeeb8, 0x3b4b, "vcvt.f64.u32", 3, 255, 255, 255, 22), // vcvt.f64.u32 d3, s22
    (0xeeb2, 0x0b60, "vcvtb.f64.f16", 0, 255, 255, 255, 1), // vcvtb.f64.f16 d0, s1
    (0xeeb2, 0xfb6f, "vcvtb.f64.f16", 15, 255, 255, 255, 31), // vcvtb.f64.f16 d15, s31
    (0xeeb2, 0x8b48, "vcvtb.f64.f16", 8, 255, 255, 255, 16), // vcvtb.f64.f16 d8, s16
    (0xeeb2, 0x3b4b, "vcvtb.f64.f16", 3, 255, 255, 255, 22), // vcvtb.f64.f16 d3, s22
    (0xeeb2, 0x0be0, "vcvtt.f64.f16", 0, 255, 255, 255, 1), // vcvtt.f64.f16 d0, s1
    (0xeeb2, 0xfbef, "vcvtt.f64.f16", 15, 255, 255, 255, 31), // vcvtt.f64.f16 d15, s31
    (0xeeb2, 0x8bc8, "vcvtt.f64.f16", 8, 255, 255, 255, 16), // vcvtt.f64.f16 d8, s16
    (0xeeb2, 0x3bcb, "vcvtt.f64.f16", 3, 255, 255, 255, 22), // vcvtt.f64.f16 d3, s22
    (0xeebe, 0x0bef, "vcvt.fixed.f64", 0, 255, 255, 255, 255), // vcvt.s32.f64 d0, d0, #1
    (0xeebe, 0xfbc0, "vcvt.fixed.f64", 15, 255, 255, 255, 255), // vcvt.s32.f64 d15, d15, #32
    (0xeebe, 0x6bc8, "vcvt.fixed.f64", 6, 255, 255, 255, 255), // vcvt.s32.f64 d6, d6, #16
    (0xeebe, 0xbbee, "vcvt.fixed.f64", 11, 255, 255, 255, 255), // vcvt.s32.f64 d11, d11, #3
    (0xeebf, 0x0bef, "vcvt.fixed.f64", 0, 255, 255, 255, 255), // vcvt.u32.f64 d0, d0, #1
    (0xeebf, 0xfbc0, "vcvt.fixed.f64", 15, 255, 255, 255, 255), // vcvt.u32.f64 d15, d15, #32
    (0xeebf, 0x6bc8, "vcvt.fixed.f64", 6, 255, 255, 255, 255), // vcvt.u32.f64 d6, d6, #16
    (0xeebf, 0xbbee, "vcvt.fixed.f64", 11, 255, 255, 255, 255), // vcvt.u32.f64 d11, d11, #3
    (0xeebe, 0x0b67, "vcvt.fixed.f64", 0, 255, 255, 255, 255), // vcvt.s16.f64 d0, d0, #1
    (0xeebe, 0xfb40, "vcvt.fixed.f64", 15, 255, 255, 255, 255), // vcvt.s16.f64 d15, d15, #16
    (0xeebe, 0x6b44, "vcvt.fixed.f64", 6, 255, 255, 255, 255), // vcvt.s16.f64 d6, d6, #8
    (0xeebe, 0xbb66, "vcvt.fixed.f64", 11, 255, 255, 255, 255), // vcvt.s16.f64 d11, d11, #3
    (0xeebf, 0x0b67, "vcvt.fixed.f64", 0, 255, 255, 255, 255), // vcvt.u16.f64 d0, d0, #1
    (0xeebf, 0xfb40, "vcvt.fixed.f64", 15, 255, 255, 255, 255), // vcvt.u16.f64 d15, d15, #16
    (0xeebf, 0x6b44, "vcvt.fixed.f64", 6, 255, 255, 255, 255), // vcvt.u16.f64 d6, d6, #8
    (0xeebf, 0xbb66, "vcvt.fixed.f64", 11, 255, 255, 255, 255), // vcvt.u16.f64 d11, d11, #3
    (0xeeba, 0x0bef, "vcvt.f64.fixed", 0, 255, 255, 255, 255), // vcvt.f64.s32 d0, d0, #1
    (0xeeba, 0xfbc0, "vcvt.f64.fixed", 15, 255, 255, 255, 255), // vcvt.f64.s32 d15, d15, #32
    (0xeeba, 0x6bc8, "vcvt.f64.fixed", 6, 255, 255, 255, 255), // vcvt.f64.s32 d6, d6, #16
    (0xeeba, 0xbbee, "vcvt.f64.fixed", 11, 255, 255, 255, 255), // vcvt.f64.s32 d11, d11, #3
    (0xeebb, 0x0bef, "vcvt.f64.fixed", 0, 255, 255, 255, 255), // vcvt.f64.u32 d0, d0, #1
    (0xeebb, 0xfbc0, "vcvt.f64.fixed", 15, 255, 255, 255, 255), // vcvt.f64.u32 d15, d15, #32
    (0xeebb, 0x6bc8, "vcvt.f64.fixed", 6, 255, 255, 255, 255), // vcvt.f64.u32 d6, d6, #16
    (0xeebb, 0xbbee, "vcvt.f64.fixed", 11, 255, 255, 255, 255), // vcvt.f64.u32 d11, d11, #3
    (0xeeba, 0x0b67, "vcvt.f64.fixed", 0, 255, 255, 255, 255), // vcvt.f64.s16 d0, d0, #1
    (0xeeba, 0xfb40, "vcvt.f64.fixed", 15, 255, 255, 255, 255), // vcvt.f64.s16 d15, d15, #16
    (0xeeba, 0x6b44, "vcvt.f64.fixed", 6, 255, 255, 255, 255), // vcvt.f64.s16 d6, d6, #8
    (0xeeba, 0xbb66, "vcvt.f64.fixed", 11, 255, 255, 255, 255), // vcvt.f64.s16 d11, d11, #3
    (0xeebb, 0x0b67, "vcvt.f64.fixed", 0, 255, 255, 255, 255), // vcvt.f64.u16 d0, d0, #1
    (0xeebb, 0xfb40, "vcvt.f64.fixed", 15, 255, 255, 255, 255), // vcvt.f64.u16 d15, d15, #16
    (0xeebb, 0x6b44, "vcvt.f64.fixed", 6, 255, 255, 255, 255), // vcvt.f64.u16 d6, d6, #8
    (0xeebb, 0xbb66, "vcvt.f64.fixed", 11, 255, 255, 255, 255), // vcvt.f64.u16 d11, d11, #3
    (0xeeb7, 0x0b00, "vmov.f64 #imm", 0, 255, 255, 255, 255), // vmov.f64 d0, #1.000000e+00
    (0xeeb7, 0xfb00, "vmov.f64 #imm", 15, 255, 255, 255, 255), // vmov.f64 d15, #1.000000e+00
    (0xeeb7, 0x9b00, "vmov.f64 #imm", 9, 255, 255, 255, 255), // vmov.f64 d9, #1.000000e+00
    (0xeebf, 0x0b00, "vmov.f64 #imm", 0, 255, 255, 255, 255), // vmov.f64 d0, #-1.000000e+00
    (0xeebf, 0xfb00, "vmov.f64 #imm", 15, 255, 255, 255, 255), // vmov.f64 d15, #-1.000000e+00
    (0xeebf, 0x9b00, "vmov.f64 #imm", 9, 255, 255, 255, 255), // vmov.f64 d9, #-1.000000e+00
    (0xeeb0, 0x0b00, "vmov.f64 #imm", 0, 255, 255, 255, 255), // vmov.f64 d0, #2.000000e+00
    (0xeeb0, 0xfb00, "vmov.f64 #imm", 15, 255, 255, 255, 255), // vmov.f64 d15, #2.000000e+00
    (0xeeb0, 0x9b00, "vmov.f64 #imm", 9, 255, 255, 255, 255), // vmov.f64 d9, #2.000000e+00
    (0xeeb7, 0x0b0c, "vmov.f64 #imm", 0, 255, 255, 255, 255), // vmov.f64 d0, #1.750000e+00
    (0xeeb7, 0xfb0c, "vmov.f64 #imm", 15, 255, 255, 255, 255), // vmov.f64 d15, #1.750000e+00
    (0xeeb7, 0x9b0c, "vmov.f64 #imm", 9, 255, 255, 255, 255), // vmov.f64 d9, #1.750000e+00
    (0xeeb3, 0x0b0f, "vmov.f64 #imm", 0, 255, 255, 255, 255), // vmov.f64 d0, #3.100000e+01
    (0xeeb3, 0xfb0f, "vmov.f64 #imm", 15, 255, 255, 255, 255), // vmov.f64 d15, #3.100000e+01
    (0xeeb3, 0x9b0f, "vmov.f64 #imm", 9, 255, 255, 255, 255), // vmov.f64 d9, #3.100000e+01
    (0xeeb4, 0x0b00, "vmov.f64 #imm", 0, 255, 255, 255, 255), // vmov.f64 d0, #1.250000e-01
    (0xeeb4, 0xfb00, "vmov.f64 #imm", 15, 255, 255, 255, 255), // vmov.f64 d15, #1.250000e-01
    (0xeeb4, 0x9b00, "vmov.f64 #imm", 9, 255, 255, 255, 255), // vmov.f64 d9, #1.250000e-01
    (0xeebe, 0x0b00, "vmov.f64 #imm", 0, 255, 255, 255, 255), // vmov.f64 d0, #-5.000000e-01
    (0xeebe, 0xfb00, "vmov.f64 #imm", 15, 255, 255, 255, 255), // vmov.f64 d15, #-5.000000e-01
    (0xeebe, 0x9b00, "vmov.f64 #imm", 9, 255, 255, 255, 255), // vmov.f64 d9, #-5.000000e-01
    (0xeeb2, 0x0b04, "vmov.f64 #imm", 0, 255, 255, 255, 255), // vmov.f64 d0, #1.000000e+01
    (0xeeb2, 0xfb04, "vmov.f64 #imm", 15, 255, 255, 255, 255), // vmov.f64 d15, #1.000000e+01
    (0xeeb2, 0x9b04, "vmov.f64 #imm", 9, 255, 255, 255, 255), // vmov.f64 d9, #1.000000e+01
    (0xeeb0, 0x0b0c, "vmov.f64 #imm", 0, 255, 255, 255, 255), // vmov.f64 d0, #3.500000e+00
    (0xeeb0, 0xfb0c, "vmov.f64 #imm", 15, 255, 255, 255, 255), // vmov.f64 d15, #3.500000e+00
    (0xeeb0, 0x9b0c, "vmov.f64 #imm", 9, 255, 255, 255, 255), // vmov.f64 d9, #3.500000e+00
    (0xeebb, 0x0b0f, "vmov.f64 #imm", 0, 255, 255, 255, 255), // vmov.f64 d0, #-3.100000e+01
    (0xeebb, 0xfb0f, "vmov.f64 #imm", 15, 255, 255, 255, 255), // vmov.f64 d15, #-3.100000e+01
    (0xeebb, 0x9b0f, "vmov.f64 #imm", 9, 255, 255, 255, 255), // vmov.f64 d9, #-3.100000e+01
];
pub const NOT_DECODE_VECTORS: &[(u16, u16)] = &[
    (0xed90, 0x3b02), // vldr d3, [r0, #8]
    (0xed81, 0x7b00), // vstr d7, [r1]
    (0xec43, 0x2b15), // vmov d5, r2, r3
    (0xec55, 0x4b16), // vmov r4, r5, d6
    (0xecb0, 0x0b08), // vldmia r0!, {d0, d1, d2, d3}
    (0xed2d, 0x8b10), // vpush {d8, d9, d10, d11, d12, d13, d14, d15}
    (0xee30, 0x0a81), // vadd.f32 s0, s1, s2
    (0xee31, 0x0b10), // vmov.32 r0, d1[1]
    (0xee01, 0x2a90), // vmov s3, r2
    (0xeef1, 0x0a10), // vmrs r0, fpscr
    (0xeb01, 0x0002), // add.w r0, r1, r2
    (0xfe01, 0x0b42), // the Armv8 group with opc1[3] clear and the second halfword's bit 6 set: unallocated, not VSEL
];
