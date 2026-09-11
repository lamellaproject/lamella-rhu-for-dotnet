//! The scheduler's one surface the arithmetic modules touch: the refusal code
//! the arena returns for a bad range. The scheduler itself is not
//! host-compiled.

pub const ERR_INVAL: i32 = -22;
