//! Flag bit positions in the F register.
//!
//! The low nibble of F is hardwired to zero on real hardware — any
//! write to F (e.g. `POP AF`) masks the low four bits off.

pub const FLAG_Z: u8 = 0x80;
pub const FLAG_N: u8 = 0x40;
pub const FLAG_H: u8 = 0x20;
pub const FLAG_C: u8 = 0x10;
