use crate::registers::*;

/// Precomputed SZ53 flags table: S, Z, bits 5 and 3 from the value itself.
/// Index by the result byte to get S|Z|5|3 flags.
static SZ53: [u8; 256] = {
    let mut table = [0u8; 256];
    let mut i = 0u16;
    while i < 256 {
        let v = i as u8;
        let mut f = v & (FLAG_S | FLAG_5 | FLAG_3);
        if v == 0 {
            f |= FLAG_Z;
        }
        table[i as usize] = f;
        i += 1;
    }
    table
};

/// Precomputed parity table. Even parity = FLAG_PV set.
static PARITY: [u8; 256] = {
    let mut table = [0u8; 256];
    let mut i = 0u16;
    while i < 256 {
        let bits = (i as u8).count_ones();
        if bits.is_multiple_of(2) {
            table[i as usize] = FLAG_PV;
        }
        i += 1;
    }
    table
};

/// SZ53P flags: SZ53 | parity.
pub(crate) static SZ53P: [u8; 256] = {
    let mut table = [0u8; 256];
    let mut i = 0u16;
    while i < 256 {
        table[i as usize] = SZ53[i as usize] | PARITY[i as usize];
        i += 1;
    }
    table
};

// ============================================================================
// 8-bit ALU operations
// ============================================================================

/// ADD A, val
#[inline]
pub fn add_a(regs: &mut Registers, val: u8) {
    let a = regs.a() as u16;
    let v = val as u16;
    let result = a + v;
    let r8 = result as u8;

    let mut f = SZ53[r8 as usize];
    if result > 0xFF {
        f |= FLAG_C;
    }
    if (a ^ v ^ result) & 0x10 != 0 {
        f |= FLAG_H;
    }
    // Overflow: both operands same sign, result different sign
    if (!(a ^ v) & (a ^ result)) & 0x80 != 0 {
        f |= FLAG_PV;
    }

    regs.set_a(r8);
    regs.set_f_q(f);
}

/// ADC A, val
#[inline]
pub fn adc_a(regs: &mut Registers, val: u8) {
    let a = regs.a() as u16;
    let v = val as u16;
    let c = if regs.flag(FLAG_C) { 1u16 } else { 0 };
    let result = a + v + c;
    let r8 = result as u8;

    let mut f = SZ53[r8 as usize];
    if result > 0xFF {
        f |= FLAG_C;
    }
    if (a ^ v ^ result) & 0x10 != 0 {
        f |= FLAG_H;
    }
    if (!(a ^ v) & (a ^ result)) & 0x80 != 0 {
        f |= FLAG_PV;
    }

    regs.set_a(r8);
    regs.set_f_q(f);
}

/// SUB val (also used for CP — CP doesn't store result)
#[inline]
pub fn sub_a(regs: &mut Registers, val: u8, store: bool) {
    let a = regs.a() as u16;
    let v = val as u16;
    let result = a.wrapping_sub(v);
    let r8 = result as u8;

    let mut f = SZ53[r8 as usize] | FLAG_N;
    // For CP, bits 3 and 5 come from the operand, not the result
    if !store {
        f = (f & !(FLAG_3 | FLAG_5)) | (val & (FLAG_3 | FLAG_5));
    }
    if a < v {
        f |= FLAG_C;
    }
    if (a ^ v ^ result) & 0x10 != 0 {
        f |= FLAG_H;
    }
    if ((a ^ v) & (a ^ result)) & 0x80 != 0 {
        f |= FLAG_PV;
    }

    if store {
        regs.set_a(r8);
    }
    regs.set_f_q(f);
}

/// SBC A, val
#[inline]
pub fn sbc_a(regs: &mut Registers, val: u8) {
    let a = regs.a() as u16;
    let v = val as u16;
    let c = if regs.flag(FLAG_C) { 1u16 } else { 0 };
    let result = a.wrapping_sub(v).wrapping_sub(c);
    let r8 = result as u8;

    let mut f = SZ53[r8 as usize] | FLAG_N;
    if a < v + c {
        f |= FLAG_C;
    }
    if (a ^ v ^ result) & 0x10 != 0 {
        f |= FLAG_H;
    }
    if ((a ^ v) & (a ^ result)) & 0x80 != 0 {
        f |= FLAG_PV;
    }

    regs.set_a(r8);
    regs.set_f_q(f);
}

/// AND val
#[inline]
pub fn and_a(regs: &mut Registers, val: u8) {
    let r = regs.a() & val;
    regs.set_a(r);
    regs.set_f_q(SZ53P[r as usize] | FLAG_H);
}

/// OR val
#[inline]
pub fn or_a(regs: &mut Registers, val: u8) {
    let r = regs.a() | val;
    regs.set_a(r);
    regs.set_f_q(SZ53P[r as usize]);
}

/// XOR val
#[inline]
pub fn xor_a(regs: &mut Registers, val: u8) {
    let r = regs.a() ^ val;
    regs.set_a(r);
    regs.set_f_q(SZ53P[r as usize]);
}

/// INC 8-bit value, returning new value. Sets flags (preserves C).
#[inline]
pub fn inc8(regs: &mut Registers, val: u8) -> u8 {
    let r = val.wrapping_add(1);
    let mut f = SZ53[r as usize] | (regs.f() & FLAG_C);
    if r == 0x80 {
        f |= FLAG_PV;
    } // 0x7F -> 0x80 = overflow
    if (r & 0x0F) == 0 {
        f |= FLAG_H;
    } // half-carry
    regs.set_f_q(f);
    r
}

/// DEC 8-bit value, returning new value. Sets flags (preserves C).
#[inline]
pub fn dec8(regs: &mut Registers, val: u8) -> u8 {
    let r = val.wrapping_sub(1);
    let mut f = SZ53[r as usize] | FLAG_N | (regs.f() & FLAG_C);
    if val == 0x80 {
        f |= FLAG_PV;
    } // 0x80 -> 0x7F = overflow
    if (val & 0x0F) == 0 {
        f |= FLAG_H;
    } // half-borrow
    regs.set_f_q(f);
    r
}

// ============================================================================
// 16-bit arithmetic
// ============================================================================

/// ADD HL, rr (or IX/IY)
#[inline]
pub fn add16(regs: &mut Registers, dest: u16, src: u16) -> u16 {
    let result = dest as u32 + src as u32;
    let r16 = result as u16;

    let mut f = regs.f() & (FLAG_S | FLAG_Z | FLAG_PV); // Preserve S, Z, PV
    f |= (r16 >> 8) as u8 & (FLAG_3 | FLAG_5); // Bits 3,5 from high byte of result
    if result > 0xFFFF {
        f |= FLAG_C;
    }
    if (dest ^ src ^ r16) & 0x1000 != 0 {
        f |= FLAG_H;
    }

    regs.set_f_q(f);
    r16
}

// ============================================================================
// Rotates on A (non-CB prefix)
// ============================================================================

/// RLCA: rotate A left, old bit 7 to carry and bit 0
#[inline]
pub fn rlca(regs: &mut Registers) {
    let a = regs.a();
    let carry = a >> 7;
    let r = (a << 1) | carry;
    regs.set_a(r);
    let f = (regs.f() & (FLAG_S | FLAG_Z | FLAG_PV)) | (r & (FLAG_3 | FLAG_5)) | carry;
    regs.set_f_q(f);
}

/// RRCA: rotate A right, old bit 0 to carry and bit 7
#[inline]
pub fn rrca(regs: &mut Registers) {
    let a = regs.a();
    let carry = a & 1;
    let r = (a >> 1) | (carry << 7);
    regs.set_a(r);
    let f = (regs.f() & (FLAG_S | FLAG_Z | FLAG_PV)) | (r & (FLAG_3 | FLAG_5)) | carry;
    regs.set_f_q(f);
}

/// RLA: rotate A left through carry
#[inline]
pub fn rla(regs: &mut Registers) {
    let a = regs.a();
    let old_carry = if regs.flag(FLAG_C) { 1u8 } else { 0 };
    let new_carry = a >> 7;
    let r = (a << 1) | old_carry;
    regs.set_a(r);
    let f = (regs.f() & (FLAG_S | FLAG_Z | FLAG_PV)) | (r & (FLAG_3 | FLAG_5)) | new_carry;
    regs.set_f_q(f);
}

/// RRA: rotate A right through carry
#[inline]
pub fn rra(regs: &mut Registers) {
    let a = regs.a();
    let old_carry = if regs.flag(FLAG_C) { 1u8 } else { 0 };
    let new_carry = a & 1;
    let r = (a >> 1) | (old_carry << 7);
    regs.set_a(r);
    let f = (regs.f() & (FLAG_S | FLAG_Z | FLAG_PV)) | (r & (FLAG_3 | FLAG_5)) | new_carry;
    regs.set_f_q(f);
}

// ============================================================================
// Misc
// ============================================================================

/// DAA — decimal adjust A after BCD addition/subtraction
#[inline]
pub fn daa(regs: &mut Registers) {
    let a = regs.a();
    let mut correction = 0u8;
    let mut carry = false;

    if regs.flag(FLAG_H) || (a & 0x0F) > 9 {
        correction |= 0x06;
    }
    if regs.flag(FLAG_C) || a > 0x99 {
        correction |= 0x60;
        carry = true;
    }

    let r = if regs.flag(FLAG_N) {
        a.wrapping_sub(correction)
    } else {
        a.wrapping_add(correction)
    };

    let mut f = SZ53P[r as usize];
    if carry {
        f |= FLAG_C;
    }
    if regs.flag(FLAG_N) {
        f |= FLAG_N;
    }
    // H: for add, H if (a ^ r) & 0x10; for sub, H if old_H and (a ^ r) & 0x10
    if regs.flag(FLAG_N) {
        if regs.flag(FLAG_H) && (a & 0x0F) < 6 {
            f |= FLAG_H;
        }
    } else if (a & 0x0F) > 9 {
        f |= FLAG_H;
    }

    regs.set_a(r);
    regs.set_f_q(f);
}

/// CPL — complement A (flip all bits)
#[inline]
pub fn cpl(regs: &mut Registers) {
    let r = !regs.a();
    regs.set_a(r);
    let f = (regs.f() & (FLAG_S | FLAG_Z | FLAG_PV | FLAG_C))
        | (r & (FLAG_3 | FLAG_5))
        | FLAG_H
        | FLAG_N;
    regs.set_f_q(f);
}

/// SCF — set carry flag.
/// Bits 3/5 depend on Q register:
/// - If Q == F (previous instruction set flags): bits 3/5 from A only
/// - If Q != F (previous instruction didn't set flags): bits 3/5 from A | old_F
#[inline]
pub fn scf(regs: &mut Registers) {
    let a = regs.a();
    let old_f = regs.f();
    // Q register determines bits 3/5 source
    let bits35_source = if regs.prev_q == old_f {
        a // previous instruction set flags → A only
    } else {
        a | old_f // previous instruction didn't set flags → A | F
    };
    let f = (old_f & (FLAG_S | FLAG_Z | FLAG_PV)) | (bits35_source & (FLAG_3 | FLAG_5)) | FLAG_C;
    regs.set_f(f);
    regs.q = f;
}

/// CCF — complement carry flag.
/// Same Q register behaviour as SCF for bits 3/5.
#[inline]
pub fn ccf(regs: &mut Registers) {
    let a = regs.a();
    let old_f = regs.f();
    let old_c = old_f & FLAG_C;
    let bits35_source = if regs.prev_q == old_f { a } else { a | old_f };
    let mut f = (old_f & (FLAG_S | FLAG_Z | FLAG_PV)) | (bits35_source & (FLAG_3 | FLAG_5));
    if old_c != 0 {
        f |= FLAG_H;
    }
    if old_c == 0 {
        f |= FLAG_C;
    }
    regs.set_f(f);
    regs.q = f;
}

// ============================================================================
// CB-prefix rotates and shifts
// ============================================================================

/// RLC val — rotate left, old bit 7 to carry and bit 0
#[inline]
pub fn rlc(regs: &mut Registers, val: u8) -> u8 {
    let carry = val >> 7;
    let r = (val << 1) | carry;
    regs.set_f_q(SZ53P[r as usize] | carry);
    r
}

/// RRC val
#[inline]
pub fn rrc(regs: &mut Registers, val: u8) -> u8 {
    let carry = val & 1;
    let r = (val >> 1) | (carry << 7);
    regs.set_f_q(SZ53P[r as usize] | carry);
    r
}

/// RL val — rotate left through carry
#[inline]
pub fn rl(regs: &mut Registers, val: u8) -> u8 {
    let old_carry = if regs.flag(FLAG_C) { 1u8 } else { 0 };
    let new_carry = val >> 7;
    let r = (val << 1) | old_carry;
    regs.set_f_q(SZ53P[r as usize] | new_carry);
    r
}

/// RR val — rotate right through carry
#[inline]
pub fn rr(regs: &mut Registers, val: u8) -> u8 {
    let old_carry = if regs.flag(FLAG_C) { 1u8 } else { 0 };
    let new_carry = val & 1;
    let r = (val >> 1) | (old_carry << 7);
    regs.set_f_q(SZ53P[r as usize] | new_carry);
    r
}

/// SLA val — shift left arithmetic (bit 0 = 0)
#[inline]
pub fn sla(regs: &mut Registers, val: u8) -> u8 {
    let carry = val >> 7;
    let r = val << 1;
    regs.set_f_q(SZ53P[r as usize] | carry);
    r
}

/// SRA val — shift right arithmetic (bit 7 preserved)
#[inline]
pub fn sra(regs: &mut Registers, val: u8) -> u8 {
    let carry = val & 1;
    let r = (val >> 1) | (val & 0x80);
    regs.set_f_q(SZ53P[r as usize] | carry);
    r
}

/// SRL val — shift right logical (bit 7 = 0)
#[inline]
pub fn srl(regs: &mut Registers, val: u8) -> u8 {
    let carry = val & 1;
    let r = val >> 1;
    regs.set_f_q(SZ53P[r as usize] | carry);
    r
}

/// SLL val — undocumented shift left (bit 0 = 1)
#[inline]
pub fn sll(regs: &mut Registers, val: u8) -> u8 {
    let carry = val >> 7;
    let r = (val << 1) | 1;
    regs.set_f_q(SZ53P[r as usize] | carry);
    r
}

/// BIT b, val — test bit b of val
#[inline]
pub fn bit(regs: &mut Registers, b: u8, val: u8) {
    let r = val & (1 << b);
    // Z and PV are the same for BIT (set if bit is 0)
    let mut f = (regs.f() & FLAG_C) | FLAG_H;
    if r == 0 {
        f |= FLAG_Z | FLAG_PV;
    }
    // S is set if bit 7 is tested and set
    if b == 7 && r != 0 {
        f |= FLAG_S;
    }
    // Bits 3 and 5 come from val for BIT b, r; from high byte of addr for BIT b, (HL)
    f |= val & (FLAG_3 | FLAG_5);
    regs.set_f_q(f);
}

/// BIT b, (HL) — bits 3 and 5 come from high byte of address (WZ)
#[inline]
pub fn bit_hl(regs: &mut Registers, b: u8, val: u8) {
    let r = val & (1 << b);
    let mut f = (regs.f() & FLAG_C) | FLAG_H;
    if r == 0 {
        f |= FLAG_Z | FLAG_PV;
    }
    if b == 7 && r != 0 {
        f |= FLAG_S;
    }
    // Bits 3/5 from WZ high byte (MEMPTR)
    f |= regs.w() & (FLAG_3 | FLAG_5);
    regs.set_f_q(f);
}

/// SET b, val
#[inline]
pub fn set(b: u8, val: u8) -> u8 {
    val | (1 << b)
}

/// RES b, val
#[inline]
pub fn res(b: u8, val: u8) -> u8 {
    val & !(1 << b)
}

// ============================================================================
// Condition code evaluation
// ============================================================================

/// Evaluate a condition code (bits 4:3 of opcode).
/// 0=NZ, 1=Z, 2=NC, 3=C, 4=PO, 5=PE, 6=P, 7=M
#[inline]
pub fn condition(regs: &Registers, cc: u8) -> bool {
    let f = regs.f();
    match cc {
        0 => f & FLAG_Z == 0,  // NZ
        1 => f & FLAG_Z != 0,  // Z
        2 => f & FLAG_C == 0,  // NC
        3 => f & FLAG_C != 0,  // C
        4 => f & FLAG_PV == 0, // PO
        5 => f & FLAG_PV != 0, // PE
        6 => f & FLAG_S == 0,  // P
        7 => f & FLAG_S != 0,  // M
        _ => unreachable!(),
    }
}

// ============================================================================
// Register selection helpers
// ============================================================================

/// Read an 8-bit register by index (bits 2:0 or 5:3 of opcode).
/// 0=B, 1=C, 2=D, 3=E, 4=H, 5=L, 6=(HL) [not handled here], 7=A
#[inline]
pub fn read_r8(regs: &Registers, idx: u8) -> u8 {
    match idx {
        0 => regs.b(),
        1 => regs.c(),
        2 => regs.d(),
        3 => regs.e(),
        4 => regs.h(),
        5 => regs.l(),
        // 6 = (HL), handled by caller with memory read
        7 => regs.a(),
        _ => unreachable!(),
    }
}

/// Write an 8-bit register by index.
#[inline]
pub fn write_r8(regs: &mut Registers, idx: u8, val: u8) {
    match idx {
        0 => regs.set_b(val),
        1 => regs.set_c(val),
        2 => regs.set_d(val),
        3 => regs.set_e(val),
        4 => regs.set_h(val),
        5 => regs.set_l(val),
        // 6 = (HL), handled by caller with memory write
        7 => regs.set_a(val),
        _ => unreachable!(),
    }
}

/// Read an 8-bit register with DD/FD prefix — H→IXH, L→IXL (undocumented).
/// `is_ix` true for DD prefix, false for FD (IY).
#[inline]
pub fn read_r8_ix(regs: &Registers, idx: u8, is_ix: bool) -> u8 {
    match idx {
        4 => {
            if is_ix {
                regs.ixh()
            } else {
                regs.iyh()
            }
        }
        5 => {
            if is_ix {
                regs.ixl()
            } else {
                regs.iyl()
            }
        }
        _ => read_r8(regs, idx), // B, C, D, E, A unchanged
    }
}

/// Write an 8-bit register with DD/FD prefix — H→IXH, L→IXL.
#[inline]
pub fn write_r8_ix(regs: &mut Registers, idx: u8, val: u8, is_ix: bool) {
    match idx {
        4 => {
            if is_ix {
                regs.set_ixh(val)
            } else {
                regs.set_iyh(val)
            }
        }
        5 => {
            if is_ix {
                regs.set_ixl(val)
            } else {
                regs.set_iyl(val)
            }
        }
        _ => write_r8(regs, idx, val),
    }
}

/// Read a 16-bit register pair by index (bits 5:4 of opcode).
/// 0=BC, 1=DE, 2=HL, 3=SP
#[inline]
pub fn read_rr(regs: &Registers, idx: u8) -> u16 {
    match idx {
        0 => regs.bc,
        1 => regs.de,
        2 => regs.hl,
        3 => regs.sp,
        _ => unreachable!(),
    }
}

/// Write a 16-bit register pair by index.
#[inline]
pub fn write_rr(regs: &mut Registers, idx: u8, val: u16) {
    match idx {
        0 => regs.bc = val,
        1 => regs.de = val,
        2 => regs.hl = val,
        3 => regs.sp = val,
        _ => unreachable!(),
    }
}

/// Read a 16-bit register pair for PUSH/POP (AF instead of SP).
/// 0=BC, 1=DE, 2=HL, 3=AF
#[inline]
pub fn read_rr_af(regs: &Registers, idx: u8) -> u16 {
    match idx {
        0 => regs.bc,
        1 => regs.de,
        2 => regs.hl,
        3 => regs.af,
        _ => unreachable!(),
    }
}

/// Write a 16-bit register pair for PUSH/POP (AF instead of SP).
#[inline]
pub fn write_rr_af(regs: &mut Registers, idx: u8, val: u16) {
    match idx {
        0 => regs.bc = val,
        1 => regs.de = val,
        2 => regs.hl = val,
        3 => regs.af = val,
        _ => unreachable!(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_a_basic() {
        let mut r = Registers::default();
        r.set_a(0x10);
        add_a(&mut r, 0x20);
        assert_eq!(r.a(), 0x30);
        assert!(!r.flag(FLAG_C));
        assert!(!r.flag(FLAG_Z));
        assert!(!r.flag(FLAG_N));
    }

    #[test]
    fn add_a_carry() {
        let mut r = Registers::default();
        r.set_a(0xFF);
        add_a(&mut r, 0x01);
        assert_eq!(r.a(), 0x00);
        assert!(r.flag(FLAG_C));
        assert!(r.flag(FLAG_Z));
        assert!(r.flag(FLAG_H));
    }

    #[test]
    fn add_a_overflow() {
        let mut r = Registers::default();
        r.set_a(0x7F); // +127
        add_a(&mut r, 0x01); // +1
        assert_eq!(r.a(), 0x80); // -128 (overflow)
        assert!(r.flag(FLAG_PV));
        assert!(r.flag(FLAG_S));
    }

    #[test]
    fn sub_a_basic() {
        let mut r = Registers::default();
        r.set_a(0x30);
        sub_a(&mut r, 0x10, true);
        assert_eq!(r.a(), 0x20);
        assert!(r.flag(FLAG_N));
        assert!(!r.flag(FLAG_C));
    }

    #[test]
    fn cp_does_not_store() {
        let mut r = Registers::default();
        r.set_a(0x30);
        sub_a(&mut r, 0x10, false); // CP
        assert_eq!(r.a(), 0x30); // A unchanged
        assert!(r.flag(FLAG_N));
    }

    #[test]
    fn and_sets_h() {
        let mut r = Registers::default();
        r.set_a(0xFF);
        and_a(&mut r, 0x0F);
        assert_eq!(r.a(), 0x0F);
        assert!(r.flag(FLAG_H));
        assert!(!r.flag(FLAG_C));
    }

    #[test]
    fn inc_overflow() {
        let mut r = Registers::default();
        let v = inc8(&mut r, 0x7F);
        assert_eq!(v, 0x80);
        assert!(r.flag(FLAG_PV));
        assert!(r.flag(FLAG_S));
        assert!(r.flag(FLAG_H));
    }

    #[test]
    fn dec_underflow() {
        let mut r = Registers::default();
        let v = dec8(&mut r, 0x80);
        assert_eq!(v, 0x7F);
        assert!(r.flag(FLAG_PV));
        assert!(!r.flag(FLAG_S));
    }

    #[test]
    fn rlca_basic() {
        let mut r = Registers::default();
        r.set_a(0x85); // 10000101
        rlca(&mut r);
        assert_eq!(r.a(), 0x0B); // 00001011
        assert!(r.flag(FLAG_C));
    }

    #[test]
    fn condition_codes() {
        let mut r = Registers::default();
        r.set_f(FLAG_Z | FLAG_C);
        assert!(!condition(&r, 0)); // NZ = false (Z is set)
        assert!(condition(&r, 1)); // Z = true
        assert!(!condition(&r, 2)); // NC = false (C is set)
        assert!(condition(&r, 3)); // C = true
    }

    #[test]
    fn parity_table() {
        assert_eq!(PARITY[0], FLAG_PV); // 0 bits set = even parity
        assert_eq!(PARITY[1], 0); // 1 bit set = odd parity
        assert_eq!(PARITY[3], FLAG_PV); // 2 bits set (0b11) = even parity
        assert_eq!(PARITY[7], 0); // 3 bits set (0b111) = odd parity
        assert_eq!(PARITY[0xFF], FLAG_PV); // 8 bits set = even parity
    }

    #[test]
    fn bit_test() {
        let mut r = Registers::default();
        r.set_f(FLAG_C); // preserve carry
        bit(&mut r, 0, 0x01);
        assert!(!r.flag(FLAG_Z)); // bit 0 is set
        assert!(r.flag(FLAG_C)); // carry preserved
        assert!(r.flag(FLAG_H)); // H always set

        bit(&mut r, 7, 0x00);
        assert!(r.flag(FLAG_Z)); // bit 7 is clear
    }

    #[test]
    fn register_selection() {
        let r = Registers {
            bc: 0x1234,
            de: 0x5678,
            hl: 0x9ABC,
            af: 0xDE00,
            ..Registers::default()
        };

        assert_eq!(read_r8(&r, 0), 0x12); // B
        assert_eq!(read_r8(&r, 1), 0x34); // C
        assert_eq!(read_r8(&r, 2), 0x56); // D
        assert_eq!(read_r8(&r, 3), 0x78); // E
        assert_eq!(read_r8(&r, 4), 0x9A); // H
        assert_eq!(read_r8(&r, 5), 0xBC); // L
        assert_eq!(read_r8(&r, 7), 0xDE); // A
    }

    #[test]
    fn adc_a_includes_carry_in() {
        let mut r = Registers::default();
        r.set_a(0x10);
        r.set_flag(FLAG_C, true);
        adc_a(&mut r, 0x20);
        assert_eq!(r.a(), 0x31); // 0x10 + 0x20 + carry-in
        assert!(!r.flag(FLAG_C));
        assert!(!r.flag(FLAG_Z));

        // Carry-in tipping the result over 0xFF.
        let mut r = Registers::default();
        r.set_a(0xFE);
        r.set_flag(FLAG_C, true);
        adc_a(&mut r, 0x01);
        assert_eq!(r.a(), 0x00);
        assert!(r.flag(FLAG_C));
        assert!(r.flag(FLAG_Z));
    }

    #[test]
    fn sbc_a_includes_carry_in() {
        let mut r = Registers::default();
        r.set_a(0x10);
        r.set_flag(FLAG_C, true);
        sbc_a(&mut r, 0x01);
        assert_eq!(r.a(), 0x0E); // 0x10 - 0x01 - 1
        assert!(r.flag(FLAG_N));
        assert!(!r.flag(FLAG_C));

        // Borrow propagates from the carry-in alone.
        let mut r = Registers::default();
        r.set_a(0x00);
        r.set_flag(FLAG_C, true);
        sbc_a(&mut r, 0x00);
        assert_eq!(r.a(), 0xFF);
        assert!(r.flag(FLAG_C));
        assert!(r.flag(FLAG_S));
    }

    #[test]
    fn or_xor_clear_carry_and_half_carry() {
        let mut r = Registers::default();
        r.set_a(0xF0);
        r.set_flag(FLAG_C, true);
        r.set_flag(FLAG_H, true);
        or_a(&mut r, 0x0F);
        assert_eq!(r.a(), 0xFF);
        assert!(!r.flag(FLAG_C));
        assert!(!r.flag(FLAG_H));
        assert!(r.flag(FLAG_S));

        let mut r = Registers::default();
        r.set_a(0xFF);
        r.set_flag(FLAG_C, true);
        r.set_flag(FLAG_H, true);
        xor_a(&mut r, 0xFF);
        assert_eq!(r.a(), 0x00);
        assert!(!r.flag(FLAG_C));
        assert!(!r.flag(FLAG_H));
        assert!(r.flag(FLAG_Z));
        assert!(r.flag(FLAG_PV)); // 0 has even parity
    }

    #[test]
    fn write_r8_round_trips_every_index() {
        // Indices 0..=5 plus 7 form a complete 8-bit register selector;
        // index 6 is `(HL)` and is handled by the caller.
        let mut r = Registers::default();
        for idx in [0u8, 1, 2, 3, 4, 5, 7] {
            write_r8(&mut r, idx, 0xA0 | idx);
            assert_eq!(read_r8(&r, idx), 0xA0 | idx, "index {idx}");
        }
    }

    #[test]
    fn r8_ix_prefix_swaps_h_l_for_ixh_ixl() {
        let mut r = Registers {
            ix: 0x1122,
            iy: 0x3344,
            hl: 0x5566,
            ..Registers::default()
        };

        // is_ix = true selects IXH/IXL for indices 4/5; others fall through.
        assert_eq!(read_r8_ix(&r, 4, true), 0x11);
        assert_eq!(read_r8_ix(&r, 5, true), 0x22);
        assert_eq!(read_r8_ix(&r, 4, false), 0x33);
        assert_eq!(read_r8_ix(&r, 5, false), 0x44);
        // Non-H/L indices ignore the prefix.
        r.bc = 0xAB00;
        assert_eq!(read_r8_ix(&r, 0, true), 0xAB);

        write_r8_ix(&mut r, 4, 0xEE, true);
        write_r8_ix(&mut r, 5, 0xFF, true);
        assert_eq!(r.ix, 0xEEFF);
        write_r8_ix(&mut r, 4, 0x99, false);
        write_r8_ix(&mut r, 5, 0x88, false);
        assert_eq!(r.iy, 0x9988);
        // HL should be untouched by either prefix path.
        assert_eq!(r.hl, 0x5566);
    }

    #[test]
    fn rr_index_helpers_distinguish_sp_from_af() {
        let mut r = Registers {
            bc: 0x1111,
            de: 0x2222,
            hl: 0x3333,
            sp: 0x4444,
            af: 0x5555,
            ..Registers::default()
        };

        // Standard table (idx 3 = SP).
        assert_eq!(read_rr(&r, 0), 0x1111);
        assert_eq!(read_rr(&r, 1), 0x2222);
        assert_eq!(read_rr(&r, 2), 0x3333);
        assert_eq!(read_rr(&r, 3), 0x4444);
        // PUSH/POP table (idx 3 = AF).
        assert_eq!(read_rr_af(&r, 3), 0x5555);

        write_rr(&mut r, 3, 0xC0DE);
        assert_eq!(r.sp, 0xC0DE);
        write_rr_af(&mut r, 3, 0xBABE);
        assert_eq!(r.af, 0xBABE);
        // Confirm AF write didn't bleed into SP and vice versa.
        assert_eq!(r.sp, 0xC0DE);
    }

    #[test]
    fn bit_set_res_round_trip() {
        // SET writes the bit, RES clears it, leaving every other bit alone.
        let v = 0b0000_0000;
        let with_bit3 = set(3, v);
        assert_eq!(with_bit3, 0b0000_1000);
        assert_eq!(res(3, with_bit3), 0b0000_0000);

        let cleared = res(7, 0xFF);
        assert_eq!(cleared, 0x7F);
    }
}
