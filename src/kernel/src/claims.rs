//! The claims table: every owner of a pin or an interrupt bit registers
//! here, so two owners of one line are refused loudly -- the board's own
//! claims at boot (the console's pins, the debug pins, a wireless hook's
//! lines) and a program's claims through the GPIO doors. One bitmap per
//! bank of thirty-two lines for each side; the interrupt slots a program's
//! armed edge holds where a family multiplexes them (one slot per line
//! number on the ST families, the same number on every port); and the
//! tenancy a board hook takes on an interrupt bit (a register, a bit, a
//! wake word), so the family handler that owns the bank's one interrupt
//! line dispatches that bit to that word. Beside the lines, a bitmap of
//! timer channels (a PWM output serves one line at a time) and a bitmap
//! of converter channels per block. The record `__rhu_claims` is
//! read by symbol (a debugger, the api flavor); nothing in a program
//! reads it.
//!
//! Pure: no board dependency, host-tested. The callers hold the kernel
//! critical section around every call that writes (the doors are called
//! from hosted threads and the handlers run in the board band); this
//! module does no masking of its own.

/// The table's capacity: six banks of thirty-two lines cover the largest
/// family in hand (eleven ports of sixteen on the F7 = 176; forty-eight on
/// the RP2350B). A board states its own count at init.
pub const MAX_LINES: usize = 192;
pub const BANKS: usize = MAX_LINES / 32;
/// Interrupt slots: thirty-two, one bit each of the slot word; the ST
/// families use sixteen of them, their EXTI line numbers.
pub const SLOTS: usize = 32;
pub const NO_OWNER: u16 = u16::MAX;
/// Tenants of a family handler's interrupt line (a hook's register and bit).
pub const MAX_TENANTS: usize = 4;
/// The timer channels: sixteen timers of eight channels, one bit each;
/// the converter channels: four blocks of thirty-two, one bit each.
pub const PWM_TIMERS_MAX: usize = 16;
pub const PWM_CHANNELS_MAX: usize = 8;
pub const PWM_BANKS: usize = PWM_TIMERS_MAX * PWM_CHANNELS_MAX / 32;
pub const ADC_BLOCKS_MAX: usize = 4;
pub const ADC_CHANNELS_MAX: usize = 32;
pub const ADC_BANKS: usize = ADC_BLOCKS_MAX * ADC_CHANNELS_MAX / 32;

/// The refusals, in the hosting API's codes.
pub const REFUSED_BUSY: i32 = -16;
pub const REFUSED_INVAL: i32 = -22;

/// A tenant: the interrupt-enable register and bit a hook arms, and the
/// address of the futex word the family handler wakes when the bit is set.
/// The contract: an edge or a forced bit is acknowledged by the handler
/// before the wake; a LEVEL bit is MASKED by the handler before the wake (a
/// level is not latched, so the handler cannot clear it) and the tenant
/// re-arms it after its service pass has silenced the source.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Tenant {
    pub reg: u32,
    pub bit: u32,
    pub wake: u32,
}

/// The table, as the record exports it.
#[repr(C)]
pub struct Claims {
    pub line_count: u32,
    pub board: [u32; BANKS],
    pub program: [u32; BANKS],
    pub irq_slots: u32,
    pub irq_slot_owner: [u16; SLOTS],
    pub tenants: [Tenant; MAX_TENANTS],
    pub tenant_count: u32,
    pub claims_taken: u32,
    pub releases: u32,
    pub refused_busy: u32,
    pub refused_inval: u32,
    pub last_refused_line: u32,
    pub pwm_channels: [u32; PWM_BANKS],
    pub adc_channels: [u32; ADC_BANKS],
}

pub const EMPTY: Claims = Claims {
    line_count: 0,
    board: [0; BANKS],
    program: [0; BANKS],
    irq_slots: 0,
    irq_slot_owner: [NO_OWNER; SLOTS],
    tenants: [Tenant { reg: 0, bit: 0, wake: 0 }; MAX_TENANTS],
    tenant_count: 0,
    claims_taken: 0,
    releases: 0,
    refused_busy: 0,
    refused_inval: 0,
    last_refused_line: 0,
    pwm_channels: [0; PWM_BANKS],
    adc_channels: [0; ADC_BANKS],
};

/// The board's claim list folded into bitmaps AT COMPILE TIME: a line
/// outside the table or two board claims on one line refuse to compile
/// (the same discipline the vector-table fold applies to `BOARD_IRQS`).
pub const fn fold_board(lines: &[u16]) -> [u32; BANKS] {
    let mut bits = [0u32; BANKS];
    let mut i = 0;
    while i < lines.len() {
        let l = lines[i] as usize;
        assert!(l < MAX_LINES, "a board claim names a line outside the claims table");
        let (bank, mask) = (l / 32, 1u32 << (l % 32));
        assert!(bits[bank] & mask == 0, "two board claims on one line");
        bits[bank] |= mask;
        i += 1;
    }
    bits
}

const fn split(line: u32) -> (usize, u32) {
    ((line / 32) as usize, 1u32 << (line % 32))
}

/// Reset the table to a board's line count and its folded claims.
pub fn init(c: &mut Claims, line_count: u32, board: [u32; BANKS]) {
    *c = EMPTY;
    c.line_count = line_count;
    c.board = board;
}

fn refuse(c: &mut Claims, line: u32, code: i32) -> i32 {
    if code == REFUSED_BUSY {
        c.refused_busy += 1;
    } else {
        c.refused_inval += 1;
    }
    c.last_refused_line = line;
    code
}

/// A program's claim of a line: refused `REFUSED_INVAL` past the count,
/// `REFUSED_BUSY` when the board or another claim holds it.
pub fn claim(c: &mut Claims, line: u32) -> i32 {
    if line >= c.line_count {
        return refuse(c, line, REFUSED_INVAL);
    }
    let (bank, mask) = split(line);
    if c.board[bank] & mask != 0 || c.program[bank] & mask != 0 {
        return refuse(c, line, REFUSED_BUSY);
    }
    c.program[bank] |= mask;
    c.claims_taken += 1;
    0
}

/// A board claim taken at runtime (a bus's pads at its claim, a chip select
/// at its first use): refused `REFUSED_INVAL` past the count, `REFUSED_BUSY`
/// when a program holds the line; 0 when the board takes it now or already
/// holds it (idempotent), a taken line counted.
pub fn claim_board(c: &mut Claims, line: u32) -> i32 {
    if line >= c.line_count {
        return refuse(c, line, REFUSED_INVAL);
    }
    let (bank, mask) = split(line);
    if c.program[bank] & mask != 0 {
        return refuse(c, line, REFUSED_BUSY);
    }
    if c.board[bank] & mask == 0 {
        c.board[bank] |= mask;
        c.claims_taken += 1;
    }
    0
}

/// A board claim given back at runtime (a UART port's two lines at its
/// release): the bit cleared when the board holds the line, a release
/// counted; nothing when it does not (idempotent, never a refusal).
#[cfg_attr(not(rhu_uart_rows), allow(dead_code))] // the UART doors' release on the rows with a UART table
pub fn release_board(c: &mut Claims, line: u32) {
    if line >= c.line_count {
        return;
    }
    let (bank, mask) = split(line);
    if c.board[bank] & mask != 0 {
        c.board[bank] &= !mask;
        c.releases += 1;
    }
}

/// A program's release: refused `REFUSED_INVAL` when the program does not
/// hold the line (a board claim is never released through a door).
pub fn release(c: &mut Claims, line: u32) -> i32 {
    if line >= c.line_count {
        return refuse(c, line, REFUSED_INVAL);
    }
    let (bank, mask) = split(line);
    if c.program[bank] & mask == 0 {
        return refuse(c, line, REFUSED_INVAL);
    }
    c.program[bank] &= !mask;
    c.releases += 1;
    0
}

pub fn is_program(c: &Claims, line: u32) -> bool {
    if line >= c.line_count {
        return false;
    }
    let (bank, mask) = split(line);
    c.program[bank] & mask != 0
}

#[allow(dead_code)] // read by the host tier's tests
pub fn is_board(c: &Claims, line: u32) -> bool {
    if line >= c.line_count {
        return false;
    }
    let (bank, mask) = split(line);
    c.board[bank] & mask != 0
}

/// An interrupt slot claimed for a line's armed edge: the same line may
/// re-arm its own slot; another line on the slot is refused `REFUSED_BUSY`.
#[allow(dead_code)] // the ST arms' interrupt slots
pub fn irq_slot_claim(c: &mut Claims, slot: u32, line: u32) -> i32 {
    if slot as usize >= SLOTS {
        return refuse(c, line, REFUSED_INVAL);
    }
    let mask = 1u32 << slot;
    if c.irq_slots & mask != 0 {
        if c.irq_slot_owner[slot as usize] as u32 == line {
            return 0;
        }
        return refuse(c, line, REFUSED_BUSY);
    }
    c.irq_slots |= mask;
    c.irq_slot_owner[slot as usize] = line as u16;
    0
}

/// The slot released by its owner (a release by any other line is ignored).
#[allow(dead_code)] // the ST arms' interrupt slots
pub fn irq_slot_release(c: &mut Claims, slot: u32, line: u32) {
    if slot as usize >= SLOTS {
        return;
    }
    if c.irq_slot_owner[slot as usize] as u32 == line {
        c.irq_slots &= !(1u32 << slot);
        c.irq_slot_owner[slot as usize] = NO_OWNER;
    }
}

#[allow(dead_code)] // the ST arms' interrupt slots
pub fn irq_slot_owner(c: &Claims, slot: u32) -> Option<u32> {
    if slot as usize >= SLOTS || c.irq_slots & (1u32 << slot) == 0 {
        return None;
    }
    Some(c.irq_slot_owner[slot as usize] as u32)
}

/// A tenant's claim of an interrupt bit: refused `REFUSED_BUSY` when the
/// bit is held or the table is full.
#[allow(dead_code)] // the tenant interface: the RP2350 handler reads it; no board registers a tenant
pub fn tenant_add(c: &mut Claims, reg: u32, bit: u32, wake: u32) -> i32 {
    let n = c.tenant_count as usize;
    let mut i = 0;
    while i < n {
        if c.tenants[i].reg == reg && c.tenants[i].bit == bit {
            return refuse(c, bit, REFUSED_BUSY);
        }
        i += 1;
    }
    if n >= MAX_TENANTS {
        return refuse(c, bit, REFUSED_BUSY);
    }
    c.tenants[n] = Tenant { reg, bit, wake };
    c.tenant_count += 1;
    0
}

#[allow(dead_code)] // the tenant interface: the RP2350 handler reads it; no board registers a tenant
pub fn tenant_for(c: &Claims, reg: u32, bit: u32) -> Option<Tenant> {
    let n = c.tenant_count as usize;
    let mut i = 0;
    while i < n {
        if c.tenants[i].reg == reg && c.tenants[i].bit == bit {
            return Some(c.tenants[i]);
        }
        i += 1;
    }
    None
}

/// A refusal on a timer channel or a converter channel: counted, the line
/// record untouched (a channel is not a line).
fn refuse_channel(c: &mut Claims, code: i32) -> i32 {
    if code == REFUSED_BUSY {
        c.refused_busy += 1;
    } else {
        c.refused_inval += 1;
    }
    code
}

const fn channel_split(bit: usize) -> (usize, u32) {
    (bit / 32, 1u32 << (bit % 32))
}

/// A program's claim of a timer channel (a PWM output): refused
/// `REFUSED_INVAL` past the bitmap's reach, `REFUSED_BUSY` when held.
#[allow(dead_code)] // a row whose facts carry no PWM line or ADC channel claims no channel
pub fn pwm_channel_claim(c: &mut Claims, timer: u32, channel: u32) -> i32 {
    if timer as usize >= PWM_TIMERS_MAX || channel as usize >= PWM_CHANNELS_MAX {
        return refuse_channel(c, REFUSED_INVAL);
    }
    let (bank, mask) = channel_split(timer as usize * PWM_CHANNELS_MAX + channel as usize);
    if c.pwm_channels[bank] & mask != 0 {
        return refuse_channel(c, REFUSED_BUSY);
    }
    c.pwm_channels[bank] |= mask;
    0
}

/// The release of a timer channel: refused `REFUSED_INVAL` when it is not held.
#[allow(dead_code)] // a row whose facts carry no PWM line or ADC channel claims no channel
pub fn pwm_channel_release(c: &mut Claims, timer: u32, channel: u32) -> i32 {
    if timer as usize >= PWM_TIMERS_MAX || channel as usize >= PWM_CHANNELS_MAX {
        return refuse_channel(c, REFUSED_INVAL);
    }
    let (bank, mask) = channel_split(timer as usize * PWM_CHANNELS_MAX + channel as usize);
    if c.pwm_channels[bank] & mask == 0 {
        return refuse_channel(c, REFUSED_INVAL);
    }
    c.pwm_channels[bank] &= !mask;
    0
}

#[allow(dead_code)] // read by the host tier's tests
pub fn pwm_channel_held(c: &Claims, timer: u32, channel: u32) -> bool {
    if timer as usize >= PWM_TIMERS_MAX || channel as usize >= PWM_CHANNELS_MAX {
        return false;
    }
    let (bank, mask) = channel_split(timer as usize * PWM_CHANNELS_MAX + channel as usize);
    c.pwm_channels[bank] & mask != 0
}

/// A program's claim of a converter channel: refused `REFUSED_INVAL` past
/// the bitmap's reach, `REFUSED_BUSY` when held.
#[allow(dead_code)] // a row whose facts carry no PWM line or ADC channel claims no channel
pub fn adc_channel_claim(c: &mut Claims, block: u32, channel: u32) -> i32 {
    if block as usize >= ADC_BLOCKS_MAX || channel as usize >= ADC_CHANNELS_MAX {
        return refuse_channel(c, REFUSED_INVAL);
    }
    let (bank, mask) = channel_split(block as usize * ADC_CHANNELS_MAX + channel as usize);
    if c.adc_channels[bank] & mask != 0 {
        return refuse_channel(c, REFUSED_BUSY);
    }
    c.adc_channels[bank] |= mask;
    0
}

/// The release of a converter channel: refused `REFUSED_INVAL` when it is not held.
#[allow(dead_code)] // a row whose facts carry no PWM line or ADC channel claims no channel
pub fn adc_channel_release(c: &mut Claims, block: u32, channel: u32) -> i32 {
    if block as usize >= ADC_BLOCKS_MAX || channel as usize >= ADC_CHANNELS_MAX {
        return refuse_channel(c, REFUSED_INVAL);
    }
    let (bank, mask) = channel_split(block as usize * ADC_CHANNELS_MAX + channel as usize);
    if c.adc_channels[bank] & mask == 0 {
        return refuse_channel(c, REFUSED_INVAL);
    }
    c.adc_channels[bank] &= !mask;
    0
}

#[allow(dead_code)] // read by the host tier's tests
pub fn adc_channel_held(c: &Claims, block: u32, channel: u32) -> bool {
    if block as usize >= ADC_BLOCKS_MAX || channel as usize >= ADC_CHANNELS_MAX {
        return false;
    }
    let (bank, mask) = channel_split(block as usize * ADC_CHANNELS_MAX + channel as usize);
    c.adc_channels[bank] & mask != 0
}
