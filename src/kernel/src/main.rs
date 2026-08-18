//! The gate binary shell: the entire kernel -- vector table, Reset,
//! scheduler, funnel, selftests -- lives in the `rhu_kernel` lib so the
//! same tree also builds the payload-hosting staticlib. Linking the lib
//! is this file's whole job; `ENTRY(Reset)` in the board script roots
//! the image.

#![no_std]
#![no_main]

extern crate rhu_kernel;
