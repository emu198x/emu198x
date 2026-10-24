//! Arithmetic and logic, each setting the flags it touches.

use crate::Flags;

/// A two-operand byte operation: `op(flags, lhs, rhs)`.
pub(crate) type Binary = fn(&mut Flags, u8, u8) -> u8;
/// A one-operand byte operation.
pub(crate) type Unary = fn(&mut Flags, u8) -> u8;
/// A word operation on `YA` and a 16-bit operand.
pub(crate) type Word = fn(&mut Flags, u16, u16) -> u16;

fn nz(p: &mut Flags, x: u8) {
    p.z = x == 0;
    p.n = x & 0x80 != 0;
}

pub(crate) fn adc(p: &mut Flags, x: u8, y: u8) -> u8 {
    let z = i32::from(x) + i32::from(y) + i32::from(p.c);
    p.c = z > 0xFF;
    p.z = z as u8 == 0;
    p.h = (i32::from(x) ^ i32::from(y) ^ z) & 0x10 != 0;
    p.v = !(i32::from(x) ^ i32::from(y)) & (i32::from(x) ^ z) & 0x80 != 0;
    p.n = z & 0x80 != 0;
    z as u8
}

pub(crate) fn sbc(p: &mut Flags, x: u8, y: u8) -> u8 {
    adc(p, x, !y)
}

pub(crate) fn and(p: &mut Flags, x: u8, y: u8) -> u8 {
    let r = x & y;
    nz(p, r);
    r
}

pub(crate) fn or(p: &mut Flags, x: u8, y: u8) -> u8 {
    let r = x | y;
    nz(p, r);
    r
}

pub(crate) fn eor(p: &mut Flags, x: u8, y: u8) -> u8 {
    let r = x ^ y;
    nz(p, r);
    r
}

pub(crate) fn cmp(p: &mut Flags, x: u8, y: u8) -> u8 {
    let z = i32::from(x) - i32::from(y);
    p.c = z >= 0;
    p.z = z as u8 == 0;
    p.n = z & 0x80 != 0;
    x
}

/// `MOV`: the result is the operand, flags from it.
pub(crate) fn ld(p: &mut Flags, _x: u8, y: u8) -> u8 {
    nz(p, y);
    y
}

pub(crate) fn asl(p: &mut Flags, x: u8) -> u8 {
    p.c = x & 0x80 != 0;
    let r = x << 1;
    nz(p, r);
    r
}

pub(crate) fn lsr(p: &mut Flags, x: u8) -> u8 {
    p.c = x & 0x01 != 0;
    let r = x >> 1;
    nz(p, r);
    r
}

pub(crate) fn rol(p: &mut Flags, x: u8) -> u8 {
    let carry = u8::from(p.c);
    p.c = x & 0x80 != 0;
    let r = x << 1 | carry;
    nz(p, r);
    r
}

pub(crate) fn ror(p: &mut Flags, x: u8) -> u8 {
    let carry = u8::from(p.c);
    p.c = x & 0x01 != 0;
    let r = carry << 7 | x >> 1;
    nz(p, r);
    r
}

pub(crate) fn inc(p: &mut Flags, x: u8) -> u8 {
    let r = x.wrapping_add(1);
    nz(p, r);
    r
}

pub(crate) fn dec(p: &mut Flags, x: u8) -> u8 {
    let r = x.wrapping_sub(1);
    nz(p, r);
    r
}

pub(crate) fn adw(p: &mut Flags, x: u16, y: u16) -> u16 {
    p.c = false;
    let lo = adc(p, x as u8, y as u8);
    let hi = adc(p, (x >> 8) as u8, (y >> 8) as u8);
    let z = u16::from(hi) << 8 | u16::from(lo);
    p.z = z == 0;
    z
}

pub(crate) fn sbw(p: &mut Flags, x: u16, y: u16) -> u16 {
    p.c = true;
    let lo = sbc(p, x as u8, y as u8);
    let hi = sbc(p, (x >> 8) as u8, (y >> 8) as u8);
    let z = u16::from(hi) << 8 | u16::from(lo);
    p.z = z == 0;
    z
}

pub(crate) fn cpw(p: &mut Flags, x: u16, y: u16) -> u16 {
    let z = i32::from(x) - i32::from(y);
    p.c = z >= 0;
    p.z = z as u16 == 0;
    p.n = z & 0x8000 != 0;
    x
}

pub(crate) fn ldw(p: &mut Flags, _x: u16, y: u16) -> u16 {
    p.z = y == 0;
    p.n = y & 0x8000 != 0;
    y
}
