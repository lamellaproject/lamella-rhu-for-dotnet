//! The power-management chip's registers read over the family I2C driver
//! (the power chip's register reading): a gate
//! flavor of the three Arduino Portenta H7 rows, never a shipped image. The
//! row's table carries the chip's I2C1 under this feature alone (PB6 the
//! clock, PB7 the data, alternate function 4; the block's kernel clock the
//! internal oscillator's 64 MHz, the same at both postures), and this flavor
//! claims bus 1 at 100 kHz through the kernel's own door bodies, then reads
//! the chip's identification and regulator registers one at a time -- a
//! one-byte write of the register's address, a repeated start, a one-byte
//! read -- and prints each as it reads it. NOTHING IS WRITTEN TO THE CHIP:
//! every transaction's write phase is a register address alone. The last
//! line hands the machine exit to `rhu_exit(0)` as the api flavor does; a
//! refused claim or a failed read prints its code and exits 2 the same way.

use crate::bus;
use crate::bus_math::{I2cMsg, MSG_READ};
use crate::{api, sched};

/// The chip's bus address (the family's schematics).
const PMIC_ADDR: u16 = 0x08;
/// The bus number: the chip's I2C1, as the row's table names it.
const BUS: u32 = 1;
/// One transaction's deadline: generous at 100 kHz (two bytes take 0.2 ms).
const DEADLINE_NS: u64 = 100_000_000;

/// The registers read, by address and the datasheet's name (PF1550 rev. 7,
/// its register map: the identification three, then every switching and
/// linear regulator's voltage and control registers, the coin cell's, the
/// always-on rail's and the reference's). No interrupt register is read.
const REGS: &[(u8, &str)] = &[
    (0x00, "DEVICE_ID"),
    (0x01, "OTP_FLAVOR"),
    (0x02, "SILICON_REV"),
    (0x30, "COINCELL_CONTROL"),
    (0x32, "SW1_VOLT"),
    (0x33, "SW1_STBY_VOLT"),
    (0x34, "SW1_SLP_VOLT"),
    (0x35, "SW1_CTRL"),
    (0x36, "SW1_CTRL1"),
    (0x38, "SW2_VOLT"),
    (0x39, "SW2_STBY_VOLT"),
    (0x3A, "SW2_SLP_VOLT"),
    (0x3B, "SW2_CTRL"),
    (0x3C, "SW2_CTRL1"),
    (0x3E, "SW3_VOLT"),
    (0x3F, "SW3_STBY_VOLT"),
    (0x40, "SW3_SLP_VOLT"),
    (0x41, "SW3_CTRL"),
    (0x42, "SW3_CTRL1"),
    (0x48, "VSNVS_CTRL"),
    (0x4A, "VREFDDR_CTRL"),
    (0x4C, "LDO1_VOLT"),
    (0x4D, "LDO1_CTRL"),
    (0x4F, "LDO2_VOLT"),
    (0x50, "LDO2_CTRL"),
    (0x52, "LDO3_VOLT"),
    (0x53, "LDO3_CTRL"),
];

/// One register read: the address written, a repeated start, one byte read.
fn read_reg(reg: u8) -> Result<u8, i32> {
    let mut addr = reg;
    let mut val = 0u8;
    let msgs = [
        I2cMsg { addr: PMIC_ADDR, flags: 0, len: 1, reserved: 0, buf: &mut addr },
        I2cMsg { addr: PMIC_ADDR, flags: MSG_READ, len: 1, reserved: 0, buf: &mut val },
    ];
    let r = bus::i2c_transfer(BUS, msgs.as_ptr(), 2, sched::now_ns().wrapping_add(DEADLINE_NS));
    if r < 0 {
        Err(r)
    } else {
        Ok(val)
    }
}

pub fn run() -> ! {
    kprintln!("[pmic] the power-management chip's registers over the chip's I2C1 (bus {} of this image's table, address 0x{:02x}; the kernel clock the internal oscillator's 64 MHz; 100 kHz; read-only: every write phase is a register's address)", BUS, PMIC_ADDR);
    let r = bus::i2c_claim(BUS, 100_000);
    if r < 0 {
        kprintln!("[pmic] CLAIM REFUSED: code {} -- nothing read", r);
        kprintln!("[pmic] exit-door: handing the machine exit to rhu_exit(2)");
        api::rhu_exit(2);
    }
    kprintln!("[pmic] bus {} claimed at 100 kHz", BUS);
    let mut failures = 0u32;
    for &(reg, name) in REGS {
        match read_reg(reg) {
            Ok(v) => kprintln!("[pmic] 0x{:02x} {} = 0x{:02x} (0b{:08b})", reg, name, v, v),
            Err(code) => {
                failures += 1;
                kprintln!("[pmic] 0x{:02x} {} READ FAILED: code {}", reg, name, code);
            }
        }
    }
    // The two voltage registers read a second time, so a value
    // that moved between two reads would show (none should).
    for &(reg, name) in &[(0x38u8, "SW2_VOLT"), (0x3Bu8, "SW2_CTRL")] {
        match read_reg(reg) {
            Ok(v) => kprintln!("[pmic] again 0x{:02x} {} = 0x{:02x}", reg, name, v),
            Err(code) => {
                failures += 1;
                kprintln!("[pmic] again 0x{:02x} {} READ FAILED: code {}", reg, name, code);
            }
        }
    }
    let code = if failures == 0 { 0 } else { 2 };
    kprintln!("[pmic] DONE: {} registers read, {} failed; nothing written to the chip", REGS.len() as u32 + 2 - failures, failures);
    kprintln!("[pmic] exit-door: handing the machine exit to rhu_exit({})", code);
    api::rhu_exit(code)
}
