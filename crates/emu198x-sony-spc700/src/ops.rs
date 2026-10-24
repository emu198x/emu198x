//! Every instruction, cycle by cycle.
//!
//! The bodies and their cycle order follow ares's SPC700 (ISC licence,
//! © ares team, Near et al). Each `?` is a point where the instruction may
//! stop because the pins are waiting on that cycle.

use crate::Halt;
use crate::alu::{self, Binary, Unary, Word};
use crate::exec::{Exec, Step};

/// A register an instruction names as its operand.
#[derive(Clone, Copy)]
enum Reg {
    A,
    X,
    Y,
    Sp,
}

impl Exec<'_> {
    fn get(&self, r: Reg) -> u8 {
        match r {
            Reg::A => self.regs.a,
            Reg::X => self.regs.x,
            Reg::Y => self.regs.y,
            Reg::Sp => self.regs.sp,
        }
    }

    fn set(&mut self, r: Reg, v: u8) {
        match r {
            Reg::A => self.regs.a = v,
            Reg::X => self.regs.x = v,
            Reg::Y => self.regs.y = v,
            Reg::Sp => self.regs.sp = v,
        }
    }

    fn alu2(&mut self, op: Binary, x: u8, y: u8) -> u8 {
        op(&mut self.regs.p, x, y)
    }

    fn alu1(&mut self, op: Unary, x: u8) -> u8 {
        op(&mut self.regs.p, x)
    }
}

/// A flag the set/clear instructions name.
#[derive(Clone, Copy)]
enum FlagId {
    C,
    I,
    P,
}

/// Run the instruction at PC from its opcode fetch.
pub(crate) fn run(e: &mut Exec) -> Step {
    use Reg::{A, Sp, X, Y};
    let opcode = e.fetch()?;
    match opcode {
        0x00 => e.read_pc(),
        0xEF => halt(e, Halt::Sleep),
        0xFF => halt(e, Halt::Stop),

        // CALL table entries $x1.
        n if n & 0x0F == 0x01 => call_table(e, n >> 4),
        // SET1 / CLR1 direct bit: $x2.
        n if n & 0x0F == 0x02 => bit_set(e, n >> 5, n & 0x10 == 0),
        // BBS / BBC: $x3.
        n if n & 0x0F == 0x03 => branch_bit(e, n >> 5, n & 0x10 == 0),

        0x04 => direct_read(e, alu::or, A),
        0x05 => absolute_read(e, alu::or, A),
        0x06 => indirect_x_read(e, alu::or),
        0x07 => indexed_indirect_read(e, alu::or, X),
        0x08 => immediate_read(e, alu::or, A),
        0x09 => direct_direct_modify(e, alu::or),
        0x0A => absolute_bit_modify(e, 0),
        0x0B => direct_modify(e, alu::asl),
        0x0C => absolute_modify(e, alu::asl),
        0x0D => {
            let p = e.regs.p.bits();
            push(e, p)
        }
        0x0E => test_set_bits(e, true),
        0x0F => brk(e),
        0x10 => {
            let take = !e.regs.p.n;
            branch(e, take)
        }
        0x14 => direct_indexed_read(e, alu::or, A, X),
        0x15 => absolute_indexed_read(e, alu::or, X),
        0x16 => absolute_indexed_read(e, alu::or, Y),
        0x17 => indirect_indexed_read(e, alu::or, Y),
        0x18 => direct_immediate_modify(e, alu::or),
        0x19 => indirect_x_write_indirect_y(e, alu::or),
        0x1A => direct_modify_word(e, -1),
        0x1B => direct_indexed_modify(e, alu::asl, X),
        0x1C => implied_modify(e, alu::asl, A),
        0x1D => implied_modify(e, alu::dec, X),
        0x1E => absolute_read(e, alu::cmp, X),
        0x1F => jump_indirect_x(e),
        0x20 => flag_set(e, FlagId::P, false),
        0x24 => direct_read(e, alu::and, A),
        0x25 => absolute_read(e, alu::and, A),
        0x26 => indirect_x_read(e, alu::and),
        0x27 => indexed_indirect_read(e, alu::and, X),
        0x28 => immediate_read(e, alu::and, A),
        0x29 => direct_direct_modify(e, alu::and),
        0x2A => absolute_bit_modify(e, 1),
        0x2B => direct_modify(e, alu::rol),
        0x2C => absolute_modify(e, alu::rol),
        0x2D => {
            let a = e.regs.a;
            push(e, a)
        }
        0x2E => branch_not_direct(e),
        0x2F => branch(e, true),
        0x30 => {
            let take = e.regs.p.n;
            branch(e, take)
        }
        0x34 => direct_indexed_read(e, alu::and, A, X),
        0x35 => absolute_indexed_read(e, alu::and, X),
        0x36 => absolute_indexed_read(e, alu::and, Y),
        0x37 => indirect_indexed_read(e, alu::and, Y),
        0x38 => direct_immediate_modify(e, alu::and),
        0x39 => indirect_x_write_indirect_y(e, alu::and),
        0x3A => direct_modify_word(e, 1),
        0x3B => direct_indexed_modify(e, alu::rol, X),
        0x3C => implied_modify(e, alu::rol, A),
        0x3D => implied_modify(e, alu::inc, X),
        0x3E => direct_read(e, alu::cmp, X),
        0x3F => call_absolute(e),
        0x40 => flag_set(e, FlagId::P, true),
        0x44 => direct_read(e, alu::eor, A),
        0x45 => absolute_read(e, alu::eor, A),
        0x46 => indirect_x_read(e, alu::eor),
        0x47 => indexed_indirect_read(e, alu::eor, X),
        0x48 => immediate_read(e, alu::eor, A),
        0x49 => direct_direct_modify(e, alu::eor),
        0x4A => absolute_bit_modify(e, 2),
        0x4B => direct_modify(e, alu::lsr),
        0x4C => absolute_modify(e, alu::lsr),
        0x4D => {
            let x = e.regs.x;
            push(e, x)
        }
        0x4E => test_set_bits(e, false),
        0x4F => call_page(e),
        0x50 => {
            let take = !e.regs.p.v;
            branch(e, take)
        }
        0x54 => direct_indexed_read(e, alu::eor, A, X),
        0x55 => absolute_indexed_read(e, alu::eor, X),
        0x56 => absolute_indexed_read(e, alu::eor, Y),
        0x57 => indirect_indexed_read(e, alu::eor, Y),
        0x58 => direct_immediate_modify(e, alu::eor),
        0x59 => indirect_x_write_indirect_y(e, alu::eor),
        0x5A => direct_compare_word(e, alu::cpw),
        0x5B => direct_indexed_modify(e, alu::lsr, X),
        0x5C => implied_modify(e, alu::lsr, A),
        0x5D => transfer(e, A, X),
        0x5E => absolute_read(e, alu::cmp, Y),
        0x5F => {
            let target = e.fetch_word()?;
            e.regs.pc = target;
            Ok(())
        }
        0x60 => flag_set(e, FlagId::C, false),
        0x64 => direct_read(e, alu::cmp, A),
        0x65 => absolute_read(e, alu::cmp, A),
        0x66 => indirect_x_read(e, alu::cmp),
        0x67 => indexed_indirect_read(e, alu::cmp, X),
        0x68 => immediate_read(e, alu::cmp, A),
        0x69 => direct_direct_compare(e, alu::cmp),
        0x6A => absolute_bit_modify(e, 3),
        0x6B => direct_modify(e, alu::ror),
        0x6C => absolute_modify(e, alu::ror),
        0x6D => {
            let y = e.regs.y;
            push(e, y)
        }
        0x6E => branch_not_direct_decrement(e),
        0x6F => return_subroutine(e),
        0x70 => {
            let take = e.regs.p.v;
            branch(e, take)
        }
        0x74 => direct_indexed_read(e, alu::cmp, A, X),
        0x75 => absolute_indexed_read(e, alu::cmp, X),
        0x76 => absolute_indexed_read(e, alu::cmp, Y),
        0x77 => indirect_indexed_read(e, alu::cmp, Y),
        0x78 => direct_immediate_compare(e, alu::cmp),
        0x79 => indirect_x_compare_indirect_y(e, alu::cmp),
        0x7A => direct_read_word(e, alu::adw),
        0x7B => direct_indexed_modify(e, alu::ror, X),
        0x7C => implied_modify(e, alu::ror, A),
        0x7D => transfer(e, X, A),
        0x7E => direct_read(e, alu::cmp, Y),
        0x7F => return_interrupt(e),
        0x80 => flag_set(e, FlagId::C, true),
        0x84 => direct_read(e, alu::adc, A),
        0x85 => absolute_read(e, alu::adc, A),
        0x86 => indirect_x_read(e, alu::adc),
        0x87 => indexed_indirect_read(e, alu::adc, X),
        0x88 => immediate_read(e, alu::adc, A),
        0x89 => direct_direct_modify(e, alu::adc),
        0x8A => absolute_bit_modify(e, 4),
        0x8B => direct_modify(e, alu::dec),
        0x8C => absolute_modify(e, alu::dec),
        0x8D => immediate_read(e, alu::ld, Y),
        0x8E => {
            e.read_pc()?;
            e.idle()?;
            let p = e.pull()?;
            e.regs.p = crate::Flags::from_bits(p);
            Ok(())
        }
        0x8F => direct_immediate_write(e),
        0x90 => {
            let take = !e.regs.p.c;
            branch(e, take)
        }
        0x94 => direct_indexed_read(e, alu::adc, A, X),
        0x95 => absolute_indexed_read(e, alu::adc, X),
        0x96 => absolute_indexed_read(e, alu::adc, Y),
        0x97 => indirect_indexed_read(e, alu::adc, Y),
        0x98 => direct_immediate_modify(e, alu::adc),
        0x99 => indirect_x_write_indirect_y(e, alu::adc),
        0x9A => direct_read_word(e, alu::sbw),
        0x9B => direct_indexed_modify(e, alu::dec, X),
        0x9C => implied_modify(e, alu::dec, A),
        0x9D => transfer(e, Sp, X),
        0x9E => divide(e),
        0x9F => {
            e.read_pc()?;
            e.idle()?;
            e.idle()?;
            e.idle()?;
            let a = e.regs.a.rotate_left(4);
            e.regs.a = a;
            e.regs.p.z = a == 0;
            e.regs.p.n = a & 0x80 != 0;
            Ok(())
        }
        0xA0 => flag_set(e, FlagId::I, true),
        0xA4 => direct_read(e, alu::sbc, A),
        0xA5 => absolute_read(e, alu::sbc, A),
        0xA6 => indirect_x_read(e, alu::sbc),
        0xA7 => indexed_indirect_read(e, alu::sbc, X),
        0xA8 => immediate_read(e, alu::sbc, A),
        0xA9 => direct_direct_modify(e, alu::sbc),
        0xAA => absolute_bit_modify(e, 5),
        0xAB => direct_modify(e, alu::inc),
        0xAC => absolute_modify(e, alu::inc),
        0xAD => immediate_read(e, alu::cmp, Y),
        0xAE => pull(e, A),
        0xAF => {
            // MOV (X)+,A: an idle where most writes have a dummy read.
            e.read_pc()?;
            e.idle()?;
            let x = e.regs.x;
            e.regs.x = x.wrapping_add(1);
            let a = e.regs.a;
            e.store(x, a)
        }
        0xB0 => {
            let take = e.regs.p.c;
            branch(e, take)
        }
        0xB4 => direct_indexed_read(e, alu::sbc, A, X),
        0xB5 => absolute_indexed_read(e, alu::sbc, X),
        0xB6 => absolute_indexed_read(e, alu::sbc, Y),
        0xB7 => indirect_indexed_read(e, alu::sbc, Y),
        0xB8 => direct_immediate_modify(e, alu::sbc),
        0xB9 => indirect_x_write_indirect_y(e, alu::sbc),
        0xBA => direct_read_word(e, alu::ldw),
        0xBB => direct_indexed_modify(e, alu::inc, X),
        0xBC => implied_modify(e, alu::inc, A),
        0xBD => transfer(e, X, Sp),
        0xBE => decimal_adjust(e, false),
        0xBF => {
            // MOV A,(X)+: one more idle than most reads.
            e.read_pc()?;
            let x = e.regs.x;
            e.regs.x = x.wrapping_add(1);
            let data = e.load(x)?;
            e.regs.a = data;
            e.idle()?;
            e.regs.p.z = data == 0;
            e.regs.p.n = data & 0x80 != 0;
            Ok(())
        }
        0xC0 => flag_set(e, FlagId::I, false),
        0xC4 => direct_write(e, A),
        0xC5 => absolute_write(e, A),
        0xC6 => {
            e.read_pc()?;
            let x = e.regs.x;
            e.load(x)?;
            let a = e.regs.a;
            e.store(x, a)
        }
        0xC7 => indexed_indirect_write(e, A, X),
        0xC8 => immediate_read(e, alu::cmp, X),
        0xC9 => absolute_write(e, X),
        0xCA => absolute_bit_modify(e, 6),
        0xCB => direct_write(e, Y),
        0xCC => absolute_write(e, Y),
        0xCD => immediate_read(e, alu::ld, X),
        0xCE => pull(e, X),
        0xCF => multiply(e),
        0xD0 => {
            let take = !e.regs.p.z;
            branch(e, take)
        }
        0xD4 => direct_indexed_write(e, A, X),
        0xD5 => absolute_indexed_write(e, X),
        0xD6 => absolute_indexed_write(e, Y),
        0xD7 => indirect_indexed_write(e, A, Y),
        0xD8 => direct_write(e, X),
        0xD9 => direct_indexed_write(e, X, Y),
        0xDA => {
            let addr = e.fetch()?;
            e.load(addr)?;
            let a = e.regs.a;
            e.store(addr, a)?;
            let y = e.regs.y;
            e.store(addr.wrapping_add(1), y)
        }
        0xDB => direct_indexed_write(e, Y, X),
        0xDC => implied_modify(e, alu::dec, Y),
        0xDD => transfer(e, Y, A),
        0xDE => branch_not_direct_indexed(e),
        0xDF => decimal_adjust(e, true),
        0xE0 => {
            e.read_pc()?;
            e.regs.p.h = false;
            e.regs.p.v = false;
            Ok(())
        }
        0xE4 => direct_read(e, alu::ld, A),
        0xE5 => absolute_read(e, alu::ld, A),
        0xE6 => indirect_x_read(e, alu::ld),
        0xE7 => indexed_indirect_read(e, alu::ld, X),
        0xE8 => immediate_read(e, alu::ld, A),
        0xE9 => absolute_read(e, alu::ld, X),
        0xEA => absolute_bit_modify(e, 7),
        0xEB => direct_read(e, alu::ld, Y),
        0xEC => absolute_read(e, alu::ld, Y),
        0xED => {
            e.read_pc()?;
            e.idle()?;
            e.regs.p.c = !e.regs.p.c;
            Ok(())
        }
        0xEE => pull(e, Y),
        0xF0 => {
            let take = e.regs.p.z;
            branch(e, take)
        }
        0xF4 => direct_indexed_read(e, alu::ld, A, X),
        0xF5 => absolute_indexed_read(e, alu::ld, X),
        0xF6 => absolute_indexed_read(e, alu::ld, Y),
        0xF7 => indirect_indexed_read(e, alu::ld, Y),
        0xF8 => direct_read(e, alu::ld, X),
        0xF9 => direct_indexed_read(e, alu::ld, X, Y),
        0xFA => {
            let source = e.fetch()?;
            let data = e.load(source)?;
            let target = e.fetch()?;
            e.store(target, data)
        }
        0xFB => direct_indexed_read(e, alu::ld, Y, X),
        0xFC => implied_modify(e, alu::inc, Y),
        0xFD => transfer(e, A, Y),
        0xFE => branch_not_y_decrement(e),
        // Every other opcode is matched above; the low-nibble arms cover the
        // rest.
        _ => Ok(()),
    }
}

/// `SLEEP` and `STOP`: the CPU parks itself. The host sees a read of PC and
/// an idle cycle, alternating, until reset.
fn halt(e: &mut Exec, kind: Halt) -> Step {
    e.regs.halt = kind;
    Ok(())
}

fn bit(data: u8, n: u8) -> bool {
    data >> n & 1 != 0
}

fn absolute_bit_modify(e: &mut Exec, mode: u8) -> Step {
    let operand = e.fetch_word()?;
    let n = (operand >> 13) as u8;
    let addr = operand & 0x1FFF;
    let mut data = e.read(addr)?;
    let c = e.regs.p.c;
    match mode {
        0 => {
            e.idle()?;
            e.regs.p.c = c | bit(data, n);
        }
        1 => {
            e.idle()?;
            e.regs.p.c = c | !bit(data, n);
        }
        2 => e.regs.p.c = c & bit(data, n),
        3 => e.regs.p.c = c & !bit(data, n),
        4 => {
            e.idle()?;
            e.regs.p.c = c ^ bit(data, n);
        }
        5 => e.regs.p.c = bit(data, n),
        6 => {
            e.idle()?;
            data = data & !(1 << n) | u8::from(c) << n;
            e.write(addr, data)?;
        }
        _ => {
            data ^= 1 << n;
            e.write(addr, data)?;
        }
    }
    Ok(())
}

fn bit_set(e: &mut Exec, n: u8, value: bool) -> Step {
    let addr = e.fetch()?;
    let data = e.load(addr)?;
    let data = if value {
        data | 1 << n
    } else {
        data & !(1 << n)
    };
    e.store(addr, data)
}

fn absolute_read(e: &mut Exec, op: Binary, target: Reg) -> Step {
    let addr = e.fetch_word()?;
    let data = e.read(addr)?;
    let r = e.alu2(op, e.get(target), data);
    e.set(target, r);
    Ok(())
}

fn absolute_modify(e: &mut Exec, op: Unary) -> Step {
    let addr = e.fetch_word()?;
    let data = e.read(addr)?;
    let r = e.alu1(op, data);
    e.write(addr, r)
}

fn absolute_write(e: &mut Exec, source: Reg) -> Step {
    let addr = e.fetch_word()?;
    e.read(addr)?;
    let data = e.get(source);
    e.write(addr, data)
}

fn absolute_indexed_read(e: &mut Exec, op: Binary, index: Reg) -> Step {
    let addr = e.fetch_word()?;
    e.idle()?;
    let data = e.read(addr.wrapping_add(u16::from(e.get(index))))?;
    let r = e.alu2(op, e.regs.a, data);
    e.regs.a = r;
    Ok(())
}

fn absolute_indexed_write(e: &mut Exec, index: Reg) -> Step {
    let addr = e.fetch_word()?;
    e.idle()?;
    let target = addr.wrapping_add(u16::from(e.get(index)));
    e.read(target)?;
    let a = e.regs.a;
    e.write(target, a)
}

fn relative(e: &mut Exec, displacement: u8) {
    e.regs.pc = e.regs.pc.wrapping_add(displacement as i8 as u16);
}

fn branch(e: &mut Exec, take: bool) -> Step {
    let displacement = e.fetch()?;
    if !take {
        return Ok(());
    }
    e.idle()?;
    e.idle()?;
    relative(e, displacement);
    Ok(())
}

fn branch_bit(e: &mut Exec, n: u8, set: bool) -> Step {
    let addr = e.fetch()?;
    let data = e.load(addr)?;
    e.idle()?;
    let displacement = e.fetch()?;
    if bit(data, n) != set {
        return Ok(());
    }
    e.idle()?;
    e.idle()?;
    relative(e, displacement);
    Ok(())
}

fn branch_not_direct(e: &mut Exec) -> Step {
    let addr = e.fetch()?;
    let data = e.load(addr)?;
    e.idle()?;
    let displacement = e.fetch()?;
    if e.regs.a == data {
        return Ok(());
    }
    e.idle()?;
    e.idle()?;
    relative(e, displacement);
    Ok(())
}

fn branch_not_direct_decrement(e: &mut Exec) -> Step {
    let addr = e.fetch()?;
    let data = e.load(addr)?.wrapping_sub(1);
    e.store(addr, data)?;
    let displacement = e.fetch()?;
    if data == 0 {
        return Ok(());
    }
    e.idle()?;
    e.idle()?;
    relative(e, displacement);
    Ok(())
}

fn branch_not_direct_indexed(e: &mut Exec) -> Step {
    let addr = e.fetch()?;
    e.idle()?;
    let data = e.load(addr.wrapping_add(e.regs.x))?;
    e.idle()?;
    let displacement = e.fetch()?;
    if e.regs.a == data {
        return Ok(());
    }
    e.idle()?;
    e.idle()?;
    relative(e, displacement);
    Ok(())
}

fn branch_not_y_decrement(e: &mut Exec) -> Step {
    e.read_pc()?;
    e.idle()?;
    let displacement = e.fetch()?;
    e.regs.y = e.regs.y.wrapping_sub(1);
    if e.regs.y == 0 {
        return Ok(());
    }
    e.idle()?;
    e.idle()?;
    relative(e, displacement);
    Ok(())
}

fn push_pc(e: &mut Exec) -> Step {
    let pc = e.regs.pc;
    e.push((pc >> 8) as u8)?;
    e.push(pc as u8)
}

fn brk(e: &mut Exec) -> Step {
    e.read_pc()?;
    push_pc(e)?;
    let p = e.regs.p.bits();
    e.push(p)?;
    e.idle()?;
    let lo = e.read(0xFFDE)?;
    let hi = e.read(0xFFDF)?;
    e.regs.pc = u16::from(hi) << 8 | u16::from(lo);
    e.regs.p.i = false;
    e.regs.p.b = true;
    Ok(())
}

fn call_absolute(e: &mut Exec) -> Step {
    let addr = e.fetch_word()?;
    e.idle()?;
    push_pc(e)?;
    e.idle()?;
    e.idle()?;
    e.regs.pc = addr;
    Ok(())
}

fn call_page(e: &mut Exec) -> Step {
    let addr = e.fetch()?;
    e.idle()?;
    push_pc(e)?;
    e.idle()?;
    e.regs.pc = 0xFF00 | u16::from(addr);
    Ok(())
}

fn call_table(e: &mut Exec, vector: u8) -> Step {
    e.read_pc()?;
    e.idle()?;
    push_pc(e)?;
    e.idle()?;
    let addr = 0xFFDE - (u16::from(vector) << 1);
    let lo = e.read(addr)?;
    let hi = e.read(addr + 1)?;
    e.regs.pc = u16::from(hi) << 8 | u16::from(lo);
    Ok(())
}

fn decimal_adjust(e: &mut Exec, add: bool) -> Step {
    e.read_pc()?;
    e.idle()?;
    let p = &mut e.regs.p;
    let mut a = e.regs.a;
    if add {
        if p.c || a > 0x99 {
            a = a.wrapping_add(0x60);
            p.c = true;
        }
        if p.h || a & 15 > 9 {
            a = a.wrapping_add(0x06);
        }
    } else {
        if !p.c || a > 0x99 {
            a = a.wrapping_sub(0x60);
            p.c = false;
        }
        if !p.h || a & 15 > 9 {
            a = a.wrapping_sub(0x06);
        }
    }
    p.z = a == 0;
    p.n = a & 0x80 != 0;
    e.regs.a = a;
    Ok(())
}

fn direct_read(e: &mut Exec, op: Binary, target: Reg) -> Step {
    let addr = e.fetch()?;
    let data = e.load(addr)?;
    let r = e.alu2(op, e.get(target), data);
    e.set(target, r);
    Ok(())
}

fn direct_modify(e: &mut Exec, op: Unary) -> Step {
    let addr = e.fetch()?;
    let data = e.load(addr)?;
    let r = e.alu1(op, data);
    e.store(addr, r)
}

fn direct_write(e: &mut Exec, source: Reg) -> Step {
    let addr = e.fetch()?;
    e.load(addr)?;
    let data = e.get(source);
    e.store(addr, data)
}

fn direct_direct_compare(e: &mut Exec, op: Binary) -> Step {
    let source = e.fetch()?;
    let rhs = e.load(source)?;
    let target = e.fetch()?;
    let lhs = e.load(target)?;
    e.alu2(op, lhs, rhs);
    e.idle()
}

fn direct_direct_modify(e: &mut Exec, op: Binary) -> Step {
    let source = e.fetch()?;
    let rhs = e.load(source)?;
    let target = e.fetch()?;
    let lhs = e.load(target)?;
    let r = e.alu2(op, lhs, rhs);
    e.store(target, r)
}

fn direct_immediate_compare(e: &mut Exec, op: Binary) -> Step {
    let immediate = e.fetch()?;
    let addr = e.fetch()?;
    let data = e.load(addr)?;
    e.alu2(op, data, immediate);
    e.idle()
}

fn direct_immediate_modify(e: &mut Exec, op: Binary) -> Step {
    let immediate = e.fetch()?;
    let addr = e.fetch()?;
    let data = e.load(addr)?;
    let r = e.alu2(op, data, immediate);
    e.store(addr, r)
}

fn direct_immediate_write(e: &mut Exec) -> Step {
    let immediate = e.fetch()?;
    let addr = e.fetch()?;
    e.load(addr)?;
    e.store(addr, immediate)
}

fn direct_compare_word(e: &mut Exec, op: Word) -> Step {
    let addr = e.fetch()?;
    let lo = e.load(addr)?;
    let hi = e.load(addr.wrapping_add(1))?;
    let data = u16::from(hi) << 8 | u16::from(lo);
    let ya = e.regs.ya();
    let r = op(&mut e.regs.p, ya, data);
    e.regs.set_ya(r);
    Ok(())
}

fn direct_read_word(e: &mut Exec, op: Word) -> Step {
    let addr = e.fetch()?;
    let lo = e.load(addr)?;
    e.idle()?;
    let hi = e.load(addr.wrapping_add(1))?;
    let data = u16::from(hi) << 8 | u16::from(lo);
    let ya = e.regs.ya();
    let r = op(&mut e.regs.p, ya, data);
    e.regs.set_ya(r);
    Ok(())
}

fn direct_modify_word(e: &mut Exec, adjust: i32) -> Step {
    let addr = e.fetch()?;
    let mut data = (i32::from(e.load(addr)?) + adjust) as u16;
    e.store(addr, data as u8)?;
    let hi = e.load(addr.wrapping_add(1))?;
    data = data.wrapping_add(u16::from(hi) << 8);
    e.store(addr.wrapping_add(1), (data >> 8) as u8)?;
    e.regs.p.z = data == 0;
    e.regs.p.n = data & 0x8000 != 0;
    Ok(())
}

fn direct_indexed_read(e: &mut Exec, op: Binary, target: Reg, index: Reg) -> Step {
    let addr = e.fetch()?;
    e.idle()?;
    let data = e.load(addr.wrapping_add(e.get(index)))?;
    let r = e.alu2(op, e.get(target), data);
    e.set(target, r);
    Ok(())
}

fn direct_indexed_modify(e: &mut Exec, op: Unary, index: Reg) -> Step {
    let addr = e.fetch()?;
    e.idle()?;
    let target = addr.wrapping_add(e.get(index));
    let data = e.load(target)?;
    let r = e.alu1(op, data);
    e.store(target, r)
}

fn direct_indexed_write(e: &mut Exec, source: Reg, index: Reg) -> Step {
    let addr = e.fetch()?;
    e.idle()?;
    let target = addr.wrapping_add(e.get(index));
    e.load(target)?;
    let data = e.get(source);
    e.store(target, data)
}

fn divide(e: &mut Exec) -> Step {
    e.read_pc()?;
    for _ in 0..10 {
        e.idle()?;
    }
    let ya = u32::from(e.regs.ya());
    let x = u32::from(e.regs.x);
    let y = u32::from(e.regs.y);
    let p = &mut e.regs.p;
    p.h = (y & 15) >= (x & 15);
    p.v = y >= x;
    let (a, y) = if y < x << 1 {
        (ya / x, ya % x)
    } else {
        // The quotient will not fit in nine bits; this is what the S-SMP's
        // divider produces then.
        (
            255 - (ya - (x << 9)) / (256 - x),
            x + (ya - (x << 9)) % (256 - x),
        )
    };
    let a = a as u8;
    e.regs.a = a;
    e.regs.y = y as u8;
    e.regs.p.z = a == 0;
    e.regs.p.n = a & 0x80 != 0;
    Ok(())
}

fn flag_set(e: &mut Exec, flag: FlagId, value: bool) -> Step {
    e.read_pc()?;
    match flag {
        FlagId::C => e.regs.p.c = value,
        FlagId::I => {
            e.idle()?;
            e.regs.p.i = value;
        }
        FlagId::P => e.regs.p.p = value,
    }
    Ok(())
}

fn immediate_read(e: &mut Exec, op: Binary, target: Reg) -> Step {
    let data = e.fetch()?;
    let r = e.alu2(op, e.get(target), data);
    e.set(target, r);
    Ok(())
}

fn implied_modify(e: &mut Exec, op: Unary, target: Reg) -> Step {
    e.read_pc()?;
    let r = e.alu1(op, e.get(target));
    e.set(target, r);
    Ok(())
}

fn direct_pointer(e: &mut Exec, at: u8) -> Step<u16> {
    let lo = e.load(at)?;
    let hi = e.load(at.wrapping_add(1))?;
    Ok(u16::from(hi) << 8 | u16::from(lo))
}

fn indexed_indirect_read(e: &mut Exec, op: Binary, index: Reg) -> Step {
    let indirect = e.fetch()?;
    e.idle()?;
    let addr = direct_pointer(e, indirect.wrapping_add(e.get(index)))?;
    let data = e.read(addr)?;
    let r = e.alu2(op, e.regs.a, data);
    e.regs.a = r;
    Ok(())
}

fn indexed_indirect_write(e: &mut Exec, source: Reg, index: Reg) -> Step {
    let indirect = e.fetch()?;
    e.idle()?;
    let addr = direct_pointer(e, indirect.wrapping_add(e.get(index)))?;
    e.read(addr)?;
    let data = e.get(source);
    e.write(addr, data)
}

fn indirect_indexed_read(e: &mut Exec, op: Binary, index: Reg) -> Step {
    let indirect = e.fetch()?;
    e.idle()?;
    let addr = direct_pointer(e, indirect)?;
    let data = e.read(addr.wrapping_add(u16::from(e.get(index))))?;
    let r = e.alu2(op, e.regs.a, data);
    e.regs.a = r;
    Ok(())
}

fn indirect_indexed_write(e: &mut Exec, source: Reg, index: Reg) -> Step {
    let indirect = e.fetch()?;
    let addr = direct_pointer(e, indirect)?;
    e.idle()?;
    let target = addr.wrapping_add(u16::from(e.get(index)));
    e.read(target)?;
    let data = e.get(source);
    e.write(target, data)
}

fn indirect_x_read(e: &mut Exec, op: Binary) -> Step {
    e.read_pc()?;
    let x = e.regs.x;
    let data = e.load(x)?;
    let r = e.alu2(op, e.regs.a, data);
    e.regs.a = r;
    Ok(())
}

fn indirect_x_compare_indirect_y(e: &mut Exec, op: Binary) -> Step {
    e.read_pc()?;
    let y = e.regs.y;
    let rhs = e.load(y)?;
    let x = e.regs.x;
    let lhs = e.load(x)?;
    e.alu2(op, lhs, rhs);
    e.idle()
}

fn indirect_x_write_indirect_y(e: &mut Exec, op: Binary) -> Step {
    e.read_pc()?;
    let y = e.regs.y;
    let rhs = e.load(y)?;
    let x = e.regs.x;
    let lhs = e.load(x)?;
    let r = e.alu2(op, lhs, rhs);
    e.store(x, r)
}

fn jump_indirect_x(e: &mut Exec) -> Step {
    let addr = e.fetch_word()?;
    e.idle()?;
    let at = addr.wrapping_add(u16::from(e.regs.x));
    let lo = e.read(at)?;
    let hi = e.read(at.wrapping_add(1))?;
    e.regs.pc = u16::from(hi) << 8 | u16::from(lo);
    Ok(())
}

fn multiply(e: &mut Exec) -> Step {
    e.read_pc()?;
    for _ in 0..7 {
        e.idle()?;
    }
    let ya = u16::from(e.regs.y) * u16::from(e.regs.a);
    e.regs.set_ya(ya);
    let y = e.regs.y;
    e.regs.p.z = y == 0;
    e.regs.p.n = y & 0x80 != 0;
    Ok(())
}

fn pull(e: &mut Exec, target: Reg) -> Step {
    e.read_pc()?;
    e.idle()?;
    let data = e.pull()?;
    e.set(target, data);
    Ok(())
}

fn push(e: &mut Exec, data: u8) -> Step {
    e.read_pc()?;
    e.push(data)?;
    e.idle()
}

fn return_interrupt(e: &mut Exec) -> Step {
    e.read_pc()?;
    e.idle()?;
    let p = e.pull()?;
    e.regs.p = crate::Flags::from_bits(p);
    let lo = e.pull()?;
    let hi = e.pull()?;
    e.regs.pc = u16::from(hi) << 8 | u16::from(lo);
    Ok(())
}

fn return_subroutine(e: &mut Exec) -> Step {
    e.read_pc()?;
    e.idle()?;
    let lo = e.pull()?;
    let hi = e.pull()?;
    e.regs.pc = u16::from(hi) << 8 | u16::from(lo);
    Ok(())
}

fn test_set_bits(e: &mut Exec, set: bool) -> Step {
    let addr = e.fetch_word()?;
    let data = e.read(addr)?;
    let diff = e.regs.a.wrapping_sub(data);
    e.regs.p.z = diff == 0;
    e.regs.p.n = diff & 0x80 != 0;
    e.read(addr)?;
    let a = e.regs.a;
    e.write(addr, if set { data | a } else { data & !a })
}

fn transfer(e: &mut Exec, from: Reg, to: Reg) -> Step {
    e.read_pc()?;
    let v = e.get(from);
    e.set(to, v);
    if !matches!(to, Reg::Sp) {
        e.regs.p.z = v == 0;
        e.regs.p.n = v & 0x80 != 0;
    }
    Ok(())
}
