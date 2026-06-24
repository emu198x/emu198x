use crate::alu;
use crate::mcycle;
use crate::registers::*;
use crate::walker::Prefix;
use crate::z80::Z80;

/// Execute the current instruction's operation using staged data.
///
/// Called when the walker reaches an Execute step. The walker's staged
/// data (data_lo, data_hi, addr, etc.) has been populated by previous
/// MSteps (FetchByte, ReadAddr, etc.). This function applies the ALU
/// operation and stores the result.
///
/// For instructions with multiple Execute steps (e.g., IN A,(n) has
/// two: one to stage the port address, one to store the result), the
/// execute_idx tracks which Execute we're processing.
pub(crate) fn execute(z80: &mut Z80) {
    match z80.walker.prefix {
        Prefix::CB => {
            execute_cb(z80);
            return;
        }
        Prefix::ED => {
            execute_ed(z80);
            return;
        }
        Prefix::DD | Prefix::FD => {
            // DD/FD instructions use the same execute as unprefixed
            // but with IX/IY substituted for HL.
            execute_dd_fd(z80);
            return;
        }
        Prefix::DDCB | Prefix::FDCB => {
            execute_ddcb(z80);
            return;
        }
        _ => {}
    }
    // Check if this is an interrupt sequence (opcode = 0, prefix = None, walker has INT/NMI seq)
    if z80.walker.opcode == 0 && z80.walker.prefix == Prefix::None {
        let seq_ptr = z80.walker.sequence.as_ptr();
        let im0_ptr = crate::mcycle::SEQ_INT_IM0.as_ptr();
        let im1_ptr = crate::mcycle::SEQ_INT_IM1.as_ptr();
        let im2_ptr = crate::mcycle::SEQ_INT_IM2.as_ptr();
        let nmi_ptr = crate::mcycle::SEQ_NMI.as_ptr();

        if seq_ptr == im0_ptr {
            execute_int_im0(z80);
            return;
        } else if seq_ptr == im1_ptr {
            execute_int_im1(z80);
            return;
        } else if seq_ptr == im2_ptr {
            execute_int_im2(z80);
            return;
        } else if seq_ptr == nmi_ptr {
            execute_nmi(z80);
            return;
        }
    }

    execute_unprefixed(z80);
}

fn execute_int_im0(z80: &mut Z80) {
    let exec_count = count_executes_before(z80.walker.sequence, z80.walker.step_idx);
    if exec_count == 0 {
        // In IM 0 the interrupting device drives an instruction onto the bus
        // during the ack; `data_in` holds that byte. We model the `RST n` family
        // — the realistic case, and an un-driven bus reads 0xFF = `RST 38h`.
        // `RST n` is encoded `11_ttt_111`, vectoring to `ttt * 8`. Any other
        // opcode (e.g. a multi-byte CALL) is unsupported and falls back to the
        // IM 1 behaviour (RST 38h), which is also what an open bus yields.
        let ack = z80.data_in;
        let target = if ack & 0xC7 == 0xC7 {
            (ack & 0x38) as u16
        } else {
            0x0038
        };
        z80.walker.staged.push_val = z80.regs.pc;
        z80.regs.pc = target;
        z80.regs.wz = target;
    }
}

fn execute_int_im1(z80: &mut Z80) {
    let exec_count = count_executes_before(z80.walker.sequence, z80.walker.step_idx);
    if exec_count == 0 {
        // After IntAck: stage push of current PC, then jump to 0x0038
        z80.walker.staged.push_val = z80.regs.pc;
        z80.regs.pc = 0x0038;
        z80.regs.wz = 0x0038;
    }
}

fn execute_int_im2(z80: &mut Z80) {
    let exec_count = count_executes_before(z80.walker.sequence, z80.walker.step_idx);
    match exec_count {
        0 => {
            // After IntAck: stage push of current PC, compute vector address
            z80.walker.staged.push_val = z80.regs.pc;
            // Vector table address = (I << 8) | data_in (from IntAck)
            let vector_addr = ((z80.regs.i as u16) << 8) | z80.data_in as u16;
            z80.walker.staged.addr = vector_addr;
            z80.regs.wz = vector_addr; // temporary
        }
        1 => {
            // After ReadAddr + ReadAddrHi: data_lo/hi contain the handler address
            let handler =
                u16::from_le_bytes([z80.walker.staged.data_lo, z80.walker.staged.data_hi]);
            z80.regs.pc = handler;
            z80.regs.wz = handler;
        }
        _ => {}
    }
}

fn execute_nmi(z80: &mut Z80) {
    let exec_count = count_executes_before(z80.walker.sequence, z80.walker.step_idx);
    if exec_count == 0 {
        // After Internal(5): stage push of current PC, jump to 0x0066
        z80.walker.staged.push_val = z80.regs.pc;
        z80.regs.pc = 0x0066;
        z80.regs.wz = 0x0066;
    }
}

fn execute_unprefixed(z80: &mut Z80) {
    let opcode = z80.walker.opcode;
    let exec_count = count_executes_before(z80.walker.sequence, z80.walker.step_idx);

    match opcode {
        // NOP
        0x00 => {}

        // LD r, r' — register to register
        0x40..=0x7F
            if opcode != 0x76 && (opcode & 0x07) != 0x06 && (opcode >> 3) & 0x07 != 0x06 =>
        {
            let src = opcode & 0x07;
            let dst = (opcode >> 3) & 0x07;
            let val = alu::read_r8(&z80.regs, src);
            alu::write_r8(&mut z80.regs, dst, val);
        }

        // LD r, (HL) — two Execute steps
        0x46 | 0x4E | 0x56 | 0x5E | 0x66 | 0x6E | 0x7E => {
            if exec_count == 0 {
                // Stage addr = HL
                z80.walker.staged.addr = z80.regs.hl;
            } else {
                // Store data_lo to destination register
                let dst = (opcode >> 3) & 0x07;
                alu::write_r8(&mut z80.regs, dst, z80.walker.staged.data_lo);
            }
        }

        // LD (HL), r — store to memory. Execute stages the write value.
        0x70..=0x75 | 0x77 => {
            let src = opcode & 0x07;
            z80.walker.staged.addr = z80.regs.hl;
            z80.walker.staged.write_val = alu::read_r8(&z80.regs, src);
        }

        // LD r, n — load immediate byte
        0x06 | 0x0E | 0x16 | 0x1E | 0x26 | 0x2E | 0x3E => {
            let dst = (opcode >> 3) & 0x07;
            alu::write_r8(&mut z80.regs, dst, z80.walker.staged.data_lo);
        }

        // LD (HL), n — store immediate to memory
        0x36 => {
            z80.walker.staged.addr = z80.regs.hl;
            z80.walker.staged.write_val = z80.walker.staged.data_lo;
        }

        // LD A, (BC) — two Execute steps
        0x0A => {
            if exec_count == 0 {
                z80.walker.staged.addr = z80.regs.bc;
            } else {
                z80.regs.set_a(z80.walker.staged.data_lo);
                z80.regs.wz = z80.regs.bc.wrapping_add(1);
            }
        }

        // LD A, (DE) — two Execute steps
        0x1A => {
            if exec_count == 0 {
                z80.walker.staged.addr = z80.regs.de;
            } else {
                z80.regs.set_a(z80.walker.staged.data_lo);
                z80.regs.wz = z80.regs.de.wrapping_add(1);
            }
        }

        // LD (BC), A
        0x02 => {
            z80.walker.staged.addr = z80.regs.bc;
            z80.walker.staged.write_val = z80.regs.a();
            z80.regs.wz = ((z80.regs.a() as u16) << 8) | ((z80.regs.bc.wrapping_add(1)) & 0xFF);
        }

        // LD (DE), A
        0x12 => {
            z80.walker.staged.addr = z80.regs.de;
            z80.walker.staged.write_val = z80.regs.a();
            z80.regs.wz = ((z80.regs.a() as u16) << 8) | ((z80.regs.de.wrapping_add(1)) & 0xFF);
        }

        // LD A, (nn) — two Execute steps
        0x3A => {
            if exec_count == 0 {
                // First Execute: stage addr from fetched immediate bytes
                let addr =
                    u16::from_le_bytes([z80.walker.staged.data_lo, z80.walker.staged.data_hi]);
                z80.walker.staged.addr = addr;
                z80.regs.wz = addr.wrapping_add(1);
            } else {
                // Second Execute: ReadAddr put the byte in data_lo, store to A
                z80.regs.set_a(z80.walker.staged.data_lo);
            }
        }

        // LD (nn), A
        0x32 => {
            let addr = u16::from_le_bytes([z80.walker.staged.data_lo, z80.walker.staged.data_hi]);
            z80.walker.staged.addr = addr;
            z80.walker.staged.write_val = z80.regs.a();
            z80.regs.wz = ((z80.regs.a() as u16) << 8) | ((addr.wrapping_add(1)) & 0xFF);
        }

        // LD rr, nn — 16-bit immediate load
        0x01 | 0x11 | 0x21 | 0x31 => {
            let rr = (opcode >> 4) & 0x03;
            let val = u16::from_le_bytes([z80.walker.staged.data_lo, z80.walker.staged.data_hi]);
            alu::write_rr(&mut z80.regs, rr, val);
        }

        // LD SP, HL
        0xF9 => {
            z80.regs.sp = z80.regs.hl;
        }

        // LD (nn), HL
        0x22 => {
            // Single Execute: stage addr from fetched bytes + write values from HL
            let addr = u16::from_le_bytes([z80.walker.staged.data_lo, z80.walker.staged.data_hi]);
            z80.walker.staged.addr = addr;
            z80.walker.staged.write_val = z80.regs.l();
            z80.walker.staged.write_hi = z80.regs.h();
            z80.regs.wz = addr.wrapping_add(1);
        }

        // LD HL, (nn) — two Execute steps
        0x2A => {
            if exec_count == 0 {
                // First: stage addr from fetched bytes
                let addr =
                    u16::from_le_bytes([z80.walker.staged.data_lo, z80.walker.staged.data_hi]);
                z80.walker.staged.addr = addr;
                z80.regs.wz = addr.wrapping_add(1);
            } else {
                // Second: ReadAddr/ReadAddrHi have populated data_lo/data_hi
                let val =
                    u16::from_le_bytes([z80.walker.staged.data_lo, z80.walker.staged.data_hi]);
                z80.regs.hl = val;
            }
        }

        // PUSH rr
        0xC5 | 0xD5 | 0xE5 | 0xF5 => {
            let rr = (opcode >> 4) & 0x03;
            z80.walker.staged.push_val = alu::read_rr_af(&z80.regs, rr);
        }

        // POP rr
        0xC1 | 0xD1 | 0xE1 | 0xF1 => {
            let rr = (opcode >> 4) & 0x03;
            let val = u16::from_le_bytes([z80.walker.staged.data_lo, z80.walker.staged.data_hi]);
            alu::write_rr_af(&mut z80.regs, rr, val);
        }

        // ALU A, r — 0x80..=0xBF (register operand)
        0x80..=0xBF if (opcode & 0x07) != 0x06 => {
            let src = opcode & 0x07;
            let val = alu::read_r8(&z80.regs, src);
            execute_alu_op(&mut z80.regs, (opcode >> 3) & 0x07, val);
        }

        // ALU A, (HL) — two Execute steps
        0x86 | 0x8E | 0x96 | 0x9E | 0xA6 | 0xAE | 0xB6 | 0xBE => {
            if exec_count == 0 {
                z80.walker.staged.addr = z80.regs.hl;
            } else {
                let val = z80.walker.staged.data_lo;
                execute_alu_op(&mut z80.regs, (opcode >> 3) & 0x07, val);
            }
        }

        // ALU A, n — immediate operand in data_lo from FetchByte
        0xC6 | 0xCE | 0xD6 | 0xDE | 0xE6 | 0xEE | 0xF6 | 0xFE => {
            let val = z80.walker.staged.data_lo;
            execute_alu_op(&mut z80.regs, (opcode >> 3) & 0x07, val);
        }

        // INC r
        0x04 | 0x0C | 0x14 | 0x1C | 0x24 | 0x2C | 0x3C => {
            let r = (opcode >> 3) & 0x07;
            let val = alu::read_r8(&z80.regs, r);
            let result = alu::inc8(&mut z80.regs, val);
            alu::write_r8(&mut z80.regs, r, result);
        }

        // DEC r
        0x05 | 0x0D | 0x15 | 0x1D | 0x25 | 0x2D | 0x3D => {
            let r = (opcode >> 3) & 0x07;
            let val = alu::read_r8(&z80.regs, r);
            let result = alu::dec8(&mut z80.regs, val);
            alu::write_r8(&mut z80.regs, r, result);
        }

        // INC (HL) — two Execute steps
        0x34 => {
            if exec_count == 0 {
                z80.walker.staged.addr = z80.regs.hl;
            } else {
                let result = alu::inc8(&mut z80.regs, z80.walker.staged.data_lo);
                z80.walker.staged.write_val = result;
            }
        }

        // DEC (HL) — two Execute steps
        0x35 => {
            if exec_count == 0 {
                z80.walker.staged.addr = z80.regs.hl;
            } else {
                let result = alu::dec8(&mut z80.regs, z80.walker.staged.data_lo);
                z80.walker.staged.write_val = result;
            }
        }

        // INC rr
        0x03 | 0x13 | 0x23 | 0x33 => {
            let rr = (opcode >> 4) & 0x03;
            let val = alu::read_rr(&z80.regs, rr);
            alu::write_rr(&mut z80.regs, rr, val.wrapping_add(1));
        }

        // DEC rr
        0x0B | 0x1B | 0x2B | 0x3B => {
            let rr = (opcode >> 4) & 0x03;
            let val = alu::read_rr(&z80.regs, rr);
            alu::write_rr(&mut z80.regs, rr, val.wrapping_sub(1));
        }

        // ADD HL, rr
        0x09 | 0x19 | 0x29 | 0x39 => {
            let rr = (opcode >> 4) & 0x03;
            let src = alu::read_rr(&z80.regs, rr);
            let hl = z80.regs.hl;
            z80.regs.hl = alu::add16(&mut z80.regs, hl, src);
            z80.regs.wz = hl.wrapping_add(1);
        }

        // JP nn
        0xC3 => {
            let addr = u16::from_le_bytes([z80.walker.staged.data_lo, z80.walker.staged.data_hi]);
            z80.regs.pc = addr;
            z80.regs.wz = addr;
        }

        // JP cc, nn
        0xC2 | 0xCA | 0xD2 | 0xDA | 0xE2 | 0xEA | 0xF2 | 0xFA => {
            let cc = (opcode >> 3) & 0x07;
            let addr = u16::from_le_bytes([z80.walker.staged.data_lo, z80.walker.staged.data_hi]);
            z80.regs.wz = addr;
            if alu::condition(&z80.regs, cc) {
                z80.regs.pc = addr;
            }
        }

        // JP (HL)
        0xE9 => {
            z80.regs.pc = z80.regs.hl;
        }

        // JR e
        0x18 => {
            let offset = z80.walker.staged.data_lo as i8;
            z80.regs.pc = z80.regs.pc.wrapping_add_signed(offset as i16);
            z80.regs.wz = z80.regs.pc;
        }

        // JR cc, e
        0x20 | 0x28 | 0x30 | 0x38 => {
            if std::ptr::eq(z80.walker.sequence, mcycle::SEQ_JR_CC_TAKEN) {
                let offset = z80.walker.staged.data_lo as i8;
                z80.regs.pc = z80.regs.pc.wrapping_add_signed(offset as i16);
                z80.regs.wz = z80.regs.pc;
            } else {
                z80.regs.pc = z80.regs.pc.wrapping_add(1);
            }
        }

        // DJNZ e
        0x10 => {
            let b = z80.regs.b().wrapping_sub(1);
            z80.regs.set_b(b);
            if std::ptr::eq(z80.walker.sequence, mcycle::SEQ_DJNZ_TAKEN) {
                let offset = z80.walker.staged.data_lo as i8;
                z80.regs.pc = z80.regs.pc.wrapping_add_signed(offset as i16);
                z80.regs.wz = z80.regs.pc;
            } else {
                z80.regs.pc = z80.regs.pc.wrapping_add(1);
            }
        }

        // CALL nn
        0xCD => {
            let addr = u16::from_le_bytes([z80.walker.staged.data_lo, z80.walker.staged.data_hi]);
            z80.walker.staged.push_val = z80.regs.pc;
            z80.regs.pc = addr;
            z80.regs.wz = addr;
        }

        // CALL cc, nn — Execute checks condition, done=true if not taken
        0xC4 | 0xCC | 0xD4 | 0xDC | 0xE4 | 0xEC | 0xF4 | 0xFC => {
            let addr = u16::from_le_bytes([z80.walker.staged.data_lo, z80.walker.staged.data_hi]);
            z80.regs.wz = addr;
            let cc = (opcode >> 3) & 0x07;
            if alu::condition(&z80.regs, cc) {
                // Taken: continue to Internal(1) + PushHi + PushLo
                z80.walker.staged.push_val = z80.regs.pc;
                z80.regs.pc = addr;
            } else {
                // Not taken: skip remaining steps
                z80.walker.done = true;
            }
        }

        // RET
        0xC9 => {
            let addr = u16::from_le_bytes([z80.walker.staged.data_lo, z80.walker.staged.data_hi]);
            z80.regs.pc = addr;
            z80.regs.wz = addr;
        }

        // RET cc — two Execute steps: first checks condition, second sets PC
        0xC0 | 0xC8 | 0xD0 | 0xD8 | 0xE0 | 0xE8 | 0xF0 | 0xF8 => {
            let exec_count = count_executes_before(z80.walker.sequence, z80.walker.step_idx);
            if exec_count == 0 {
                // Check condition
                let cc = (opcode >> 3) & 0x07;
                if !alu::condition(&z80.regs, cc) {
                    z80.walker.done = true; // Skip PopLo/PopHi/Execute
                }
            } else {
                // After pops: set PC from popped address
                let addr =
                    u16::from_le_bytes([z80.walker.staged.data_lo, z80.walker.staged.data_hi]);
                z80.regs.pc = addr;
                z80.regs.wz = addr;
            }
        }

        // RST p
        0xC7 | 0xCF | 0xD7 | 0xDF | 0xE7 | 0xEF | 0xF7 | 0xFF => {
            let addr = (opcode & 0x38) as u16;
            z80.walker.staged.push_val = z80.regs.pc;
            z80.regs.pc = addr;
            z80.regs.wz = addr;
        }

        // Rotates on A
        0x07 => alu::rlca(&mut z80.regs),
        0x0F => alu::rrca(&mut z80.regs),
        0x17 => alu::rla(&mut z80.regs),
        0x1F => alu::rra(&mut z80.regs),

        // DAA, CPL, SCF, CCF
        0x27 => alu::daa(&mut z80.regs),
        0x2F => alu::cpl(&mut z80.regs),
        0x37 => alu::scf(&mut z80.regs),
        0x3F => alu::ccf(&mut z80.regs),

        // EX AF, AF'
        0x08 => z80.regs.ex_af(),
        // EXX
        0xD9 => z80.regs.exx(),
        // EX DE, HL
        0xEB => z80.regs.ex_de_hl(),

        // EX (SP), HL
        0xE3 => {
            // PopLo/PopHi already read old (SP) into data_lo/hi.
            // Execute swaps: HL → push_val, data → HL
            let old_sp_val =
                u16::from_le_bytes([z80.walker.staged.data_lo, z80.walker.staged.data_hi]);
            z80.walker.staged.push_val = z80.regs.hl;
            z80.regs.hl = old_sp_val;
            z80.regs.wz = old_sp_val;
        }

        // DI
        0xF3 => {
            z80.regs.iff1 = false;
            z80.regs.iff2 = false;
        }

        // EI
        0xFB => {
            z80.regs.iff1 = true;
            z80.regs.iff2 = true;
            z80.ei_pending = true;
        }

        // HALT
        0x76 => {
            z80.halt = true;
            // PC stays advanced past HALT — the machine loop handles
            // phantom NOP fetches by not advancing PC during HALT.
        }

        // IN A, (n) — two Execute steps
        0xDB => {
            if exec_count == 0 {
                // First Execute: stage port address = (A << 8) | n
                let port = ((z80.regs.a() as u16) << 8) | z80.walker.staged.data_lo as u16;
                z80.walker.staged.addr = port;
                z80.regs.wz = port.wrapping_add(1);
            } else {
                // Second Execute: store read byte in A
                z80.regs.set_a(z80.walker.staged.data_lo);
            }
        }

        // OUT (n), A
        0xD3 => {
            let port = ((z80.regs.a() as u16) << 8) | z80.walker.staged.data_lo as u16;
            z80.walker.staged.addr = port;
            z80.walker.staged.write_val = z80.regs.a();
            z80.regs.wz =
                ((z80.regs.a() as u16) << 8) | ((z80.walker.staged.data_lo.wrapping_add(1)) as u16);
        }

        // Catch-all for stubs / unknown
        _ => {}
    }
}

/// Dispatch ALU operation by ALU op code (bits 5:3 of the opcode).
/// 0=ADD, 1=ADC, 2=SUB, 3=SBC, 4=AND, 5=XOR, 6=OR, 7=CP
fn execute_alu_op(regs: &mut Registers, op: u8, val: u8) {
    match op {
        0 => alu::add_a(regs, val),
        1 => alu::adc_a(regs, val),
        2 => alu::sub_a(regs, val, true), // SUB
        3 => alu::sbc_a(regs, val),
        4 => alu::and_a(regs, val),
        5 => alu::xor_a(regs, val),
        6 => alu::or_a(regs, val),
        7 => alu::sub_a(regs, val, false), // CP (no store)
        _ => unreachable!(),
    }
}

/// Count how many Execute steps appear before the given index in a sequence.
fn count_executes_before(sequence: &[crate::mcycle::MStep], idx: usize) -> usize {
    sequence[..idx]
        .iter()
        .filter(|s| matches!(s, crate::mcycle::MStep::Execute))
        .count()
}

// ============================================================================
// CB-prefix execute
// ============================================================================

fn execute_cb(z80: &mut Z80) {
    let opcode = z80.walker.opcode;
    let r = opcode & 0x07;
    let op_type = opcode >> 6; // 0=rot/shift, 1=BIT, 2=RES, 3=SET
    let bit_num = (opcode >> 3) & 0x07;
    let exec_count = count_executes_before(z80.walker.sequence, z80.walker.step_idx);

    if r == 6 {
        // (HL) operand — multi-Execute: first stages addr, second processes result
        if exec_count == 0 {
            z80.walker.staged.addr = z80.regs.hl;
            return;
        }
        let val = z80.walker.staged.data_lo; // byte read from (HL)
        match op_type {
            0 => {
                // Rotate/shift (HL)
                let result = execute_cb_rot(z80, (opcode >> 3) & 0x07, val);
                z80.walker.staged.write_val = result;
            }
            1 => {
                // BIT b, (HL) — read-only
                alu::bit_hl(&mut z80.regs, bit_num, val);
            }
            2 => {
                // RES b, (HL)
                z80.walker.staged.write_val = alu::res(bit_num, val);
            }
            3 => {
                // SET b, (HL)
                z80.walker.staged.write_val = alu::set(bit_num, val);
            }
            _ => unreachable!(),
        }
    } else {
        // Register operand — single Execute
        let val = alu::read_r8(&z80.regs, r);
        let result = match op_type {
            0 => execute_cb_rot(z80, (opcode >> 3) & 0x07, val),
            1 => {
                alu::bit(&mut z80.regs, bit_num, val);
                return;
            }
            2 => alu::res(bit_num, val),
            3 => alu::set(bit_num, val),
            _ => unreachable!(),
        };
        alu::write_r8(&mut z80.regs, r, result);
    }
}

/// Execute a CB-prefix rotate/shift operation.
fn execute_cb_rot(z80: &mut Z80, op: u8, val: u8) -> u8 {
    match op {
        0 => alu::rlc(&mut z80.regs, val),
        1 => alu::rrc(&mut z80.regs, val),
        2 => alu::rl(&mut z80.regs, val),
        3 => alu::rr(&mut z80.regs, val),
        4 => alu::sla(&mut z80.regs, val),
        5 => alu::sra(&mut z80.regs, val),
        6 => alu::sll(&mut z80.regs, val), // undocumented
        7 => alu::srl(&mut z80.regs, val),
        _ => unreachable!(),
    }
}

// ============================================================================
// ED-prefix execute
// ============================================================================

fn execute_ed(z80: &mut Z80) {
    let opcode = z80.walker.opcode;
    let exec_count = count_executes_before(z80.walker.sequence, z80.walker.step_idx);

    match opcode {
        // LD I, A
        0x47 => {
            z80.regs.i = z80.regs.a();
        }
        // LD R, A
        0x4F => {
            z80.regs.r = z80.regs.a();
        }
        // LD A, I
        0x57 => {
            let val = z80.regs.i;
            z80.regs.set_a(val);
            let mut f = (z80.regs.f() & FLAG_C) | (val & (FLAG_S | FLAG_3 | FLAG_5));
            if val == 0 {
                f |= FLAG_Z;
            }
            if z80.regs.iff2 {
                f |= FLAG_PV;
            }
            z80.regs.set_f_q(f);
        }
        // LD A, R
        0x5F => {
            let val = z80.regs.r;
            z80.regs.set_a(val);
            let mut f = (z80.regs.f() & FLAG_C) | (val & (FLAG_S | FLAG_3 | FLAG_5));
            if val == 0 {
                f |= FLAG_Z;
            }
            if z80.regs.iff2 {
                f |= FLAG_PV;
            }
            z80.regs.set_f_q(f);
        }

        // NEG
        0x44 | 0x4C | 0x54 | 0x5C | 0x64 | 0x6C | 0x74 | 0x7C => {
            let a = z80.regs.a();
            z80.regs.set_a(0);
            alu::sub_a(&mut z80.regs, a, true);
        }

        // RETI / RETN
        0x45 | 0x4D | 0x55 | 0x5D | 0x65 | 0x6D | 0x75 | 0x7D => {
            let addr = u16::from_le_bytes([z80.walker.staged.data_lo, z80.walker.staged.data_hi]);
            z80.regs.pc = addr;
            z80.regs.wz = addr;
            z80.regs.iff1 = z80.regs.iff2; // RETN restores IFF1 from IFF2
        }

        // IM 0 / IM 1 / IM 2
        0x46 | 0x66 => {
            z80.regs.im = 0;
        }
        0x56 | 0x76 => {
            z80.regs.im = 1;
        }
        0x5E | 0x7E => {
            z80.regs.im = 2;
        }
        0x4E | 0x6E => {
            z80.regs.im = 0;
        } // undocumented: same as IM 0

        // IN r, (C)
        0x40 | 0x48 | 0x50 | 0x58 | 0x60 | 0x68 | 0x70 | 0x78 => {
            if exec_count == 0 {
                // Stage port address = BC, set WZ = BC+1 BEFORE any register changes
                z80.walker.staged.addr = z80.regs.bc;
                z80.regs.wz = z80.regs.bc.wrapping_add(1);
            } else {
                let val = z80.walker.staged.data_lo;
                let r = (opcode >> 3) & 0x07;
                // Set flags (SZ53P, H=0, N=0)
                let f = (z80.regs.f() & FLAG_C) | crate::alu::SZ53P[val as usize];
                z80.regs.set_f_q(f);
                if r != 6 {
                    // IN F, (C) = undocumented, flags only, no store
                    alu::write_r8(&mut z80.regs, r, val);
                }
            }
        }

        // OUT (C), r
        0x41 | 0x49 | 0x51 | 0x59 | 0x61 | 0x69 | 0x71 | 0x79 => {
            let r = (opcode >> 3) & 0x07;
            z80.walker.staged.addr = z80.regs.bc;
            z80.walker.staged.write_val = if r == 6 {
                0
            } else {
                alu::read_r8(&z80.regs, r)
            };
            z80.regs.wz = z80.regs.bc.wrapping_add(1);
        }

        // SBC HL, rr
        0x42 | 0x52 | 0x62 | 0x72 => {
            let rr = (opcode >> 4) & 0x03;
            let src = alu::read_rr(&z80.regs, rr);
            let hl = z80.regs.hl;
            let c = if z80.regs.flag(FLAG_C) { 1u32 } else { 0 };
            let result = (hl as u32).wrapping_sub(src as u32).wrapping_sub(c);
            let r16 = result as u16;

            let mut f = FLAG_N;
            f |= (r16 >> 8) as u8 & (FLAG_S | FLAG_3 | FLAG_5);
            if r16 == 0 {
                f |= FLAG_Z;
            }
            if result & 0x10000 != 0 {
                f |= FLAG_C;
            } // borrow
            if (hl ^ src) & (hl ^ r16) & 0x8000 != 0 {
                f |= FLAG_PV;
            }
            if (hl ^ src ^ r16) & 0x1000 != 0 {
                f |= FLAG_H;
            }

            z80.regs.hl = r16;
            z80.regs.set_f_q(f);
            z80.regs.wz = hl.wrapping_add(1);
        }

        // ADC HL, rr
        0x4A | 0x5A | 0x6A | 0x7A => {
            let rr = (opcode >> 4) & 0x03;
            let src = alu::read_rr(&z80.regs, rr);
            let hl = z80.regs.hl;
            let c = if z80.regs.flag(FLAG_C) { 1u32 } else { 0 };
            let result = hl as u32 + src as u32 + c;
            let r16 = result as u16;

            let mut f = 0u8;
            f |= (r16 >> 8) as u8 & (FLAG_S | FLAG_3 | FLAG_5);
            if r16 == 0 {
                f |= FLAG_Z;
            }
            if result > 0xFFFF {
                f |= FLAG_C;
            }
            if !(hl ^ src) & (hl ^ r16) & 0x8000 != 0 {
                f |= FLAG_PV;
            }
            if (hl ^ src ^ r16) & 0x1000 != 0 {
                f |= FLAG_H;
            }

            z80.regs.hl = r16;
            z80.regs.set_f_q(f);
            z80.regs.wz = hl.wrapping_add(1);
        }

        // LD (nn), rr (ED prefix)
        0x43 | 0x53 | 0x63 | 0x73 => {
            let rr = (opcode >> 4) & 0x03;
            let addr = u16::from_le_bytes([z80.walker.staged.data_lo, z80.walker.staged.data_hi]);
            let val = alu::read_rr(&z80.regs, rr);
            z80.walker.staged.addr = addr;
            z80.walker.staged.write_val = val as u8;
            z80.walker.staged.write_hi = (val >> 8) as u8;
            z80.regs.wz = addr.wrapping_add(1);
        }

        // LD rr, (nn) (ED prefix)
        0x4B | 0x5B | 0x6B | 0x7B => {
            if exec_count == 0 {
                let addr =
                    u16::from_le_bytes([z80.walker.staged.data_lo, z80.walker.staged.data_hi]);
                z80.walker.staged.addr = addr;
                z80.regs.wz = addr.wrapping_add(1);
            } else {
                let rr = (opcode >> 4) & 0x03;
                let val =
                    u16::from_le_bytes([z80.walker.staged.data_lo, z80.walker.staged.data_hi]);
                alu::write_rr(&mut z80.regs, rr, val);
            }
        }

        // RLD
        0x6F => {
            if exec_count == 0 {
                z80.walker.staged.addr = z80.regs.hl;
            } else {
                let a = z80.regs.a();
                let val = z80.walker.staged.data_lo;
                // A low nibble ← val high nibble; val = (val << 4) | (A low nibble)
                let new_a = (a & 0xF0) | (val >> 4);
                let new_val = ((val << 4) & 0xF0) | (a & 0x0F);
                z80.regs.set_a(new_a);
                z80.walker.staged.write_val = new_val;
                let f = crate::alu::SZ53P[new_a as usize] | (z80.regs.f() & FLAG_C);
                z80.regs.set_f_q(f);
                z80.regs.wz = z80.regs.hl.wrapping_add(1);
            }
        }

        // RRD
        0x67 => {
            if exec_count == 0 {
                z80.walker.staged.addr = z80.regs.hl;
            } else {
                let a = z80.regs.a();
                let val = z80.walker.staged.data_lo;
                let new_a = (a & 0xF0) | (val & 0x0F);
                let new_val = (a << 4) | (val >> 4);
                z80.regs.set_a(new_a);
                z80.walker.staged.write_val = new_val;
                let f = crate::alu::SZ53P[new_a as usize] | (z80.regs.f() & FLAG_C);
                z80.regs.set_f_q(f);
                z80.regs.wz = z80.regs.hl.wrapping_add(1);
            }
        }

        // LDI
        0xA0 => execute_ldi_ldd(z80, true),
        // LDD
        0xA8 => execute_ldi_ldd(z80, false),
        // LDIR
        0xB0 => {
            let exec_count = count_executes_before(z80.walker.sequence, z80.walker.step_idx);
            execute_ldi_ldd(z80, true);
            if exec_count == 2 {
                if z80.regs.bc != 0 {
                    z80.regs.pc = z80.regs.pc.wrapping_sub(2);
                    z80.regs.wz = z80.regs.pc.wrapping_add(1);
                    // Repeat path: bits 3/5 of F are replaced with bits 3/5
                    // of the HIGH BYTE of the new PC. This is a real hardware
                    // effect discovered via cycle-accurate emulation (SpecIde).
                    let f = z80.regs.f();
                    let pc_hi = (z80.regs.pc >> 8) as u8;
                    z80.regs
                        .set_f_q((f & !(FLAG_3 | FLAG_5)) | (pc_hi & (FLAG_3 | FLAG_5)));
                } else {
                    z80.walker.done = true;
                }
            }
        }
        // LDDR
        0xB8 => {
            let exec_count = count_executes_before(z80.walker.sequence, z80.walker.step_idx);
            execute_ldi_ldd(z80, false);
            if exec_count == 2 {
                if z80.regs.bc != 0 {
                    z80.regs.pc = z80.regs.pc.wrapping_sub(2);
                    z80.regs.wz = z80.regs.pc.wrapping_add(1);
                    let f = z80.regs.f();
                    let pc_hi = (z80.regs.pc >> 8) as u8;
                    z80.regs
                        .set_f_q((f & !(FLAG_3 | FLAG_5)) | (pc_hi & (FLAG_3 | FLAG_5)));
                } else {
                    z80.walker.done = true;
                }
            }
        }

        // CPI
        0xA1 => execute_cpi_cpd(z80, true, false),
        // CPD
        0xA9 => execute_cpi_cpd(z80, false, false),
        // CPIR
        0xB1 => execute_cpi_cpd(z80, true, true),
        // CPDR
        0xB9 => execute_cpi_cpd(z80, false, true),

        // INI
        0xA2 => execute_ini_ind(z80, true, false),
        // IND
        0xAA => execute_ini_ind(z80, false, false),
        // INIR
        0xB2 => execute_ini_ind(z80, true, true),
        // INDR
        0xBA => execute_ini_ind(z80, false, true),

        // OUTI
        0xA3 => execute_outi_outd(z80, true, false),
        // OUTD
        0xAB => execute_outi_outd(z80, false, false),
        // OTIR
        0xB3 => execute_outi_outd(z80, true, true),
        // OTDR
        0xBB => execute_outi_outd(z80, false, true),

        _ => {} // Undocumented ED opcodes = NOP
    }
}

// ============================================================================
// DD/FD prefix execute (IX/IY substitution)
// ============================================================================

/// Get the current index register value (IX for DD, IY for FD).
fn index_reg(z80: &Z80) -> u16 {
    match z80.walker.prefix {
        Prefix::DD | Prefix::DDCB => z80.regs.ix,
        Prefix::FD | Prefix::FDCB => z80.regs.iy,
        _ => z80.regs.hl, // fallback
    }
}

/// Set the current index register.
fn set_index_reg(z80: &mut Z80, val: u16) {
    match z80.walker.prefix {
        Prefix::DD | Prefix::DDCB => z80.regs.ix = val,
        Prefix::FD | Prefix::FDCB => z80.regs.iy = val,
        _ => z80.regs.hl = val,
    }
}

/// Compute indexed address: IX/IY + displacement.
fn indexed_addr(z80: &Z80) -> u16 {
    let base = index_reg(z80);
    let disp = z80.walker.staged.disp as i16;
    base.wrapping_add_signed(disp)
}

fn execute_dd_fd(z80: &mut Z80) {
    let opcode = z80.walker.opcode;
    let exec_count = count_executes_before(z80.walker.sequence, z80.walker.step_idx);

    match opcode {
        // Indexed memory operations — (IX+d)/(IY+d)
        // LD r, (IX+d)
        0x46 | 0x4E | 0x56 | 0x5E | 0x66 | 0x6E | 0x7E => {
            if exec_count == 0 {
                let addr = indexed_addr(z80);
                z80.walker.staged.addr = addr;
                z80.regs.wz = addr;
            } else {
                let dst = (opcode >> 3) & 0x07;
                alu::write_r8(&mut z80.regs, dst, z80.walker.staged.data_lo);
            }
        }

        // LD (IX+d), r
        0x70..=0x75 | 0x77 => {
            let src = opcode & 0x07;
            let addr = indexed_addr(z80);
            z80.walker.staged.addr = addr;
            z80.walker.staged.write_val = alu::read_r8(&z80.regs, src);
            z80.regs.wz = addr;
        }

        // LD (IX+d), n
        0x36 => {
            let addr = indexed_addr(z80);
            z80.walker.staged.addr = addr;
            z80.walker.staged.write_val = z80.walker.staged.data_lo;
            z80.regs.wz = addr;
        }

        // ALU A, (IX+d)
        0x86 | 0x8E | 0x96 | 0x9E | 0xA6 | 0xAE | 0xB6 | 0xBE => {
            if exec_count == 0 {
                let addr = indexed_addr(z80);
                z80.walker.staged.addr = addr;
                z80.regs.wz = addr;
            } else {
                execute_alu_op(
                    &mut z80.regs,
                    (opcode >> 3) & 0x07,
                    z80.walker.staged.data_lo,
                );
            }
        }

        // INC (IX+d)
        0x34 => {
            if exec_count == 0 {
                let addr = indexed_addr(z80);
                z80.walker.staged.addr = addr;
                z80.regs.wz = addr;
            } else {
                let result = alu::inc8(&mut z80.regs, z80.walker.staged.data_lo);
                z80.walker.staged.write_val = result;
            }
        }

        // DEC (IX+d)
        0x35 => {
            if exec_count == 0 {
                let addr = indexed_addr(z80);
                z80.walker.staged.addr = addr;
                z80.regs.wz = addr;
            } else {
                let result = alu::dec8(&mut z80.regs, z80.walker.staged.data_lo);
                z80.walker.staged.write_val = result;
            }
        }

        // 16-bit operations on IX/IY

        // LD IX/IY, nn
        0x21 => {
            let val = u16::from_le_bytes([z80.walker.staged.data_lo, z80.walker.staged.data_hi]);
            set_index_reg(z80, val);
        }

        // ADD IX/IY, rr
        0x09 | 0x19 | 0x29 | 0x39 => {
            let rr_idx = (opcode >> 4) & 0x03;
            // rr=2 means IX/IY itself (not HL)
            let src = if rr_idx == 2 {
                index_reg(z80)
            } else {
                alu::read_rr(&z80.regs, rr_idx)
            };
            let idx = index_reg(z80);
            let result = alu::add16(&mut z80.regs, idx, src);
            set_index_reg(z80, result);
            z80.regs.wz = idx.wrapping_add(1);
        }

        // INC IX/IY
        0x23 => {
            let v = index_reg(z80).wrapping_add(1);
            set_index_reg(z80, v);
        }
        // DEC IX/IY
        0x2B => {
            let v = index_reg(z80).wrapping_sub(1);
            set_index_reg(z80, v);
        }

        // LD SP, IX/IY
        0xF9 => {
            z80.regs.sp = index_reg(z80);
        }

        // JP (IX/IY)
        0xE9 => {
            z80.regs.pc = index_reg(z80);
        }

        // PUSH IX/IY
        0xE5 => {
            z80.walker.staged.push_val = index_reg(z80);
        }

        // POP IX/IY
        0xE1 => {
            let val = u16::from_le_bytes([z80.walker.staged.data_lo, z80.walker.staged.data_hi]);
            set_index_reg(z80, val);
        }

        // LD (nn), IX/IY
        0x22 => {
            let addr = u16::from_le_bytes([z80.walker.staged.data_lo, z80.walker.staged.data_hi]);
            let idx = index_reg(z80);
            z80.walker.staged.addr = addr;
            z80.walker.staged.write_val = idx as u8;
            z80.walker.staged.write_hi = (idx >> 8) as u8;
            z80.regs.wz = addr.wrapping_add(1);
        }

        // LD IX/IY, (nn)
        0x2A => {
            if exec_count == 0 {
                let addr =
                    u16::from_le_bytes([z80.walker.staged.data_lo, z80.walker.staged.data_hi]);
                z80.walker.staged.addr = addr;
                z80.regs.wz = addr.wrapping_add(1);
            } else {
                let val =
                    u16::from_le_bytes([z80.walker.staged.data_lo, z80.walker.staged.data_hi]);
                set_index_reg(z80, val);
            }
        }

        // EX (SP), IX/IY
        0xE3 => {
            let old_sp_val =
                u16::from_le_bytes([z80.walker.staged.data_lo, z80.walker.staged.data_hi]);
            z80.walker.staged.push_val = index_reg(z80);
            set_index_reg(z80, old_sp_val);
            z80.regs.wz = old_sp_val;
        }

        // Instructions that use H/L as 8-bit registers → IXH/IXL (undocumented)
        // LD r, r' where src or dst is H (4) or L (5) but NOT (HL) (6)
        0x40..=0x7F
            if opcode != 0x76 && (opcode & 0x07) != 0x06 && ((opcode >> 3) & 0x07) != 0x06 =>
        {
            let src_idx = opcode & 0x07;
            let dst_idx = (opcode >> 3) & 0x07;
            let is_ix = z80.walker.prefix == Prefix::DD;
            let val = alu::read_r8_ix(&z80.regs, src_idx, is_ix);
            alu::write_r8_ix(&mut z80.regs, dst_idx, val, is_ix);
        }

        // LD r, n where r is H (4) or L (5)
        0x26 | 0x2E => {
            let dst = (opcode >> 3) & 0x07;
            let is_ix = z80.walker.prefix == Prefix::DD;
            alu::write_r8_ix(&mut z80.regs, dst, z80.walker.staged.data_lo, is_ix);
        }

        // INC r / DEC r where r is H or L
        0x24 | 0x2C => {
            // INC IXH / INC IXL
            let r = (opcode >> 3) & 0x07;
            let is_ix = z80.walker.prefix == Prefix::DD;
            let val = alu::read_r8_ix(&z80.regs, r, is_ix);
            let result = alu::inc8(&mut z80.regs, val);
            alu::write_r8_ix(&mut z80.regs, r, result, is_ix);
        }
        0x25 | 0x2D => {
            // DEC IXH / DEC IXL
            let r = (opcode >> 3) & 0x07;
            let is_ix = z80.walker.prefix == Prefix::DD;
            let val = alu::read_r8_ix(&z80.regs, r, is_ix);
            let result = alu::dec8(&mut z80.regs, val);
            alu::write_r8_ix(&mut z80.regs, r, result, is_ix);
        }

        // ALU A, r where r is H (4) or L (5)
        0x84 | 0x85 | 0x8C | 0x8D | 0x94 | 0x95 | 0x9C | 0x9D | 0xA4 | 0xA5 | 0xAC | 0xAD
        | 0xB4 | 0xB5 | 0xBC | 0xBD => {
            let src = opcode & 0x07;
            let is_ix = z80.walker.prefix == Prefix::DD;
            let val = alu::read_r8_ix(&z80.regs, src, is_ix);
            execute_alu_op(&mut z80.regs, (opcode >> 3) & 0x07, val);
        }

        // All other opcodes: fall through to unprefixed execute
        _ => {
            execute_unprefixed(z80);
        }
    }
}

// ============================================================================
// DDCB/FDCB prefix execute (indexed bit operations)
// ============================================================================

fn execute_ddcb(z80: &mut Z80) {
    let sub_opcode = z80.walker.opcode;
    let r = sub_opcode & 0x07;
    let op_type = sub_opcode >> 6;
    let bit_num = (sub_opcode >> 3) & 0x07;
    let exec_count = count_executes_before(z80.walker.sequence, z80.walker.step_idx);

    if exec_count == 0 {
        // Stage addr = IX/IY + d, set WZ
        let addr = indexed_addr(z80);
        z80.walker.staged.addr = addr;
        z80.regs.wz = addr;
        return;
    }

    let val = z80.walker.staged.data_lo;

    let result = match op_type {
        0 => {
            // Rotate/shift
            execute_cb_rot(z80, (sub_opcode >> 3) & 0x07, val)
        }
        1 => {
            // BIT — uses WZ high byte for bits 3/5
            z80.regs.wz = z80.walker.staged.addr;
            alu::bit_hl(&mut z80.regs, bit_num, val);
            return; // No write-back for BIT
        }
        2 => alu::res(bit_num, val),
        3 => alu::set(bit_num, val),
        _ => unreachable!(),
    };

    z80.walker.staged.write_val = result;
    // Undocumented: DDCB SET/RES/rotate also stores result in register r (if r != 6)
    if r != 6 {
        alu::write_r8(&mut z80.regs, r, result);
    }
}

/// Execute LDI or LDD: block transfer single step.
/// Called from 3 Execute steps:
///   0: stage addr = HL (for ReadAddr)
///   1: stage addr = DE, write_val = data_lo (for WriteAddr)
///   2: update HL, DE, BC, flags
fn execute_ldi_ldd(z80: &mut Z80, increment: bool) {
    let exec_count = count_executes_before(z80.walker.sequence, z80.walker.step_idx);

    match exec_count {
        0 => {
            // Stage read address = HL
            z80.walker.staged.addr = z80.regs.hl;
        }
        1 => {
            // Stage write address = DE, value = byte read from (HL)
            z80.walker.staged.addr = z80.regs.de;
            z80.walker.staged.write_val = z80.walker.staged.data_lo;
        }
        _ => {
            // Update registers and flags
            let byte = z80.walker.staged.data_lo;
            if increment {
                z80.regs.hl = z80.regs.hl.wrapping_add(1);
                z80.regs.de = z80.regs.de.wrapping_add(1);
            } else {
                z80.regs.hl = z80.regs.hl.wrapping_sub(1);
                z80.regs.de = z80.regs.de.wrapping_sub(1);
            }
            z80.regs.bc = z80.regs.bc.wrapping_sub(1);

            // Flags: H=0, N=0, PV=(BC!=0), bits 3/5 from A+byte
            let n = z80.regs.a().wrapping_add(byte);
            let mut f = z80.regs.f() & (FLAG_S | FLAG_Z | FLAG_C);
            if z80.regs.bc != 0 {
                f |= FLAG_PV;
            }
            if n & 0x02 != 0 {
                f |= FLAG_5;
            } // bit 1 of (A+byte) → flag 5
            f |= n & FLAG_3; // bit 3 of (A+byte) → flag 3
            z80.regs.set_f_q(f);
        }
    }
}

/// Execute CPI or CPD (and CPIR/CPDR).
/// Two Execute steps:
///   0: stage addr = HL
///   1: compare A with data_lo, update HL, BC, flags
fn execute_cpi_cpd(z80: &mut Z80, increment: bool, repeat: bool) {
    let exec_count = count_executes_before(z80.walker.sequence, z80.walker.step_idx);

    match exec_count {
        0 => {
            z80.walker.staged.addr = z80.regs.hl;
        }
        _ => {
            let a = z80.regs.a();
            let val = z80.walker.staged.data_lo;

            // Compare: A - val (don't store result)
            let result = a.wrapping_sub(val);

            // Update HL and BC
            if increment {
                z80.regs.hl = z80.regs.hl.wrapping_add(1);
            } else {
                z80.regs.hl = z80.regs.hl.wrapping_sub(1);
            }
            z80.regs.bc = z80.regs.bc.wrapping_sub(1);

            // Flags
            let mut f = (z80.regs.f() & FLAG_C) | FLAG_N; // Preserve C, set N

            // S, Z from the subtraction result
            f |= result & FLAG_S;
            if result == 0 {
                f |= FLAG_Z;
            }

            // H from the subtraction
            if (a ^ val ^ result) & 0x10 != 0 {
                f |= FLAG_H;
            }

            // PV = BC != 0
            if z80.regs.bc != 0 {
                f |= FLAG_PV;
            }

            // Bits 3 and 5: from (A - val - H), not from the result directly
            let n = result.wrapping_sub(if f & FLAG_H != 0 { 1 } else { 0 });
            if n & 0x02 != 0 {
                f |= FLAG_5;
            } // bit 1 of n → flag 5
            f |= n & FLAG_3; // bit 3 of n → flag 3

            z80.regs.set_f_q(f);

            // MEMPTR: CPI/CPIR increments, CPD/CPDR decrements
            if increment {
                z80.regs.wz = z80.regs.wz.wrapping_add(1);
            } else {
                z80.regs.wz = z80.regs.wz.wrapping_sub(1);
            }

            // Repeat logic for CPIR/CPDR
            if repeat {
                if z80.regs.bc != 0 && result != 0 {
                    z80.regs.pc = z80.regs.pc.wrapping_sub(2);
                    z80.regs.wz = z80.regs.pc.wrapping_add(1);
                    // Repeat: bits 3/5 of F replaced with PC high byte
                    let f = z80.regs.f();
                    let pc_hi = (z80.regs.pc >> 8) as u8;
                    z80.regs
                        .set_f_q((f & !(FLAG_3 | FLAG_5)) | (pc_hi & (FLAG_3 | FLAG_5)));
                } else {
                    z80.walker.done = true;
                }
            }
        }
    }
}

/// Execute INI or IND (and INIR/INDR).
/// Three Execute steps:
///   0: decrement B, stage port addr = BC (B already decremented)
///   1: stage write addr = HL, write_val = data_lo (byte from port)
///   2: update HL, set flags
fn execute_ini_ind(z80: &mut Z80, increment: bool, repeat: bool) {
    let exec_count = count_executes_before(z80.walker.sequence, z80.walker.step_idx);

    match exec_count {
        0 => {
            // Stage port address = ORIGINAL BC (before B decrement).
            // The Z80 puts the original B on the address bus, then decrements B internally.
            z80.walker.staged.addr = z80.regs.bc;
            // WZ = original BC ± 1
            if increment {
                z80.regs.wz = z80.regs.bc.wrapping_add(1);
            } else {
                z80.regs.wz = z80.regs.bc.wrapping_sub(1);
            }
            // Now decrement B
            let b = z80.regs.b().wrapping_sub(1);
            z80.regs.set_b(b);
        }
        1 => {
            // Stage write address = HL, value = byte read from port
            z80.walker.staged.addr = z80.regs.hl;
            z80.walker.staged.write_val = z80.walker.staged.data_lo;
        }
        _ => {
            // Update HL
            if increment {
                z80.regs.hl = z80.regs.hl.wrapping_add(1);
            } else {
                z80.regs.hl = z80.regs.hl.wrapping_sub(1);
            }

            // Flags — verified against FUSE, z80cpp, SpecIde
            let data = z80.walker.staged.data_lo;
            let b_after = z80.regs.b(); // B already decremented in exec_count 0
            let c = z80.regs.c();

            // k = data + ((C ± 1) & 0xFF)
            let port_adj: u8 = if increment {
                c.wrapping_add(1)
            } else {
                c.wrapping_sub(1)
            };
            let k: u16 = data as u16 + port_adj as u16;

            let mut f = 0u8;
            f |= b_after & (FLAG_S | FLAG_5 | FLAG_3);
            if b_after == 0 {
                f |= FLAG_Z;
            }
            if data & 0x80 != 0 {
                f |= FLAG_N;
            }
            if k > 0xFF {
                f |= FLAG_H | FLAG_C;
            }
            let p_input = (k as u8 & 0x07) ^ b_after;
            if p_input.count_ones().is_multiple_of(2) {
                f |= FLAG_PV;
            }

            z80.regs.set_f_q(f);

            // Repeat logic.
            //
            // WZ was set to `BC + 1` (or `BC - 1` for IND) in exec_count
            // 0 and must NOT be overwritten when the repeat path kicks
            // in — FUSE's `edb2_1` / `edba_1` and Patrik Rak's
            // `z80memptr` 102 / 103 both observe state mid-repeat (B
            // already decremented, but before the next M1 fetch) and
            // assert WZ == BC_initial ± 1. Per
            // `decisions/spectrum-test-oracle-priority.md`, FUSE +
            // Patrik Rak's consensus wins over Tom Harte for Spectrum.
            if repeat {
                if b_after != 0 {
                    z80.regs.pc = z80.regs.pc.wrapping_sub(2);
                    repeat_block_io_flags(z80, b_after);
                } else {
                    z80.walker.done = true;
                }
            }
        }
    }
}

/// Execute OUTI or OUTD (and OTIR/OTDR).
/// Three Execute steps:
///   0: stage read addr = HL
///   1: decrement B, stage port addr = BC (B decremented), write_val = data_lo
///   2: update HL, set flags
fn execute_outi_outd(z80: &mut Z80, increment: bool, repeat: bool) {
    let exec_count = count_executes_before(z80.walker.sequence, z80.walker.step_idx);

    match exec_count {
        0 => {
            // Stage read address = HL (for ReadAddr)
            z80.walker.staged.addr = z80.regs.hl;
        }
        1 => {
            // Decrement B
            let b = z80.regs.b().wrapping_sub(1);
            z80.regs.set_b(b);
            // Stage port address = BC (with B decremented), data = byte from (HL)
            z80.walker.staged.addr = z80.regs.bc;
            z80.walker.staged.write_val = z80.walker.staged.data_lo;
        }
        _ => {
            // Update HL
            if increment {
                z80.regs.hl = z80.regs.hl.wrapping_add(1);
            } else {
                z80.regs.hl = z80.regs.hl.wrapping_sub(1);
            }

            // Flags — verified against FUSE, z80cpp, SpecIde
            let data = z80.walker.staged.data_lo;
            let b_after = z80.regs.b();
            let l_after = z80.regs.l(); // L AFTER HL adjustment

            // k = data + L (L after HL increment/decrement)
            let k: u16 = data as u16 + l_after as u16;

            let mut f = 0u8;
            // S, Z, bits 5, 3 from B (after decrement)
            f |= b_after & (FLAG_S | FLAG_5 | FLAG_3);
            if b_after == 0 {
                f |= FLAG_Z;
            }
            // N = bit 7 of data byte
            if data & 0x80 != 0 {
                f |= FLAG_N;
            }
            // H and C both set if k > 0xFF
            if k > 0xFF {
                f |= FLAG_H | FLAG_C;
            }
            // P = parity of ((k & 7) ^ B)
            let p_input = (k as u8 & 0x07) ^ b_after;
            if p_input.count_ones().is_multiple_of(2) {
                f |= FLAG_PV;
            }

            z80.regs.set_f_q(f);

            // MEMPTR
            if increment {
                z80.regs.wz = z80.regs.bc.wrapping_add(1);
            } else {
                z80.regs.wz = z80.regs.bc.wrapping_sub(1);
            }

            // Repeat logic. As with INI/IND, WZ was already set to
            // `BC ± 1` just above and the repeat path must not stomp
            // it — same FUSE / Patrik Rak observation point.
            if repeat {
                if b_after != 0 {
                    z80.regs.pc = z80.regs.pc.wrapping_sub(2);
                    repeat_block_io_flags(z80, b_after);
                } else {
                    z80.walker.done = true;
                }
            }
        }
    }
}

/// Recalculate flags for INIR/INDR/OTIR/OTDR repeat path.
/// Based on SpecIde's cycle-accurate implementation.
fn repeat_block_io_flags(z80: &mut Z80, b_after: u8) {
    let mut f = z80.regs.f();
    let pc_hi = (z80.regs.pc >> 8) as u8;

    // Replace bits 3/5 with PC high byte
    f = (f & !(FLAG_3 | FLAG_5)) | (pc_hi & (FLAG_3 | FLAG_5));

    // Carry adjustment: if C was set, adjust H and recalculate PV
    let mut acc = b_after;
    if f & FLAG_C != 0 {
        acc = if f & FLAG_N != 0 {
            b_after.wrapping_sub(1)
        } else {
            b_after.wrapping_add(1)
        };
        // H flag recalculated from B ^ adjusted
        f = (f & !FLAG_H) | ((b_after ^ acc) & FLAG_H);
    }

    // PV: parity of (acc & 7), XORed with current PV
    // SpecIde: acc &= 7; parity of acc; flg ^= parity ? 0 : FLAG_PV
    let p3 = acc & 0x07;
    let parity_odd = !p3.count_ones().is_multiple_of(2);
    // Toggle PV based on parity of the low 3 bits
    if parity_odd {
        f ^= FLAG_PV;
    }

    z80.regs.set_f_q(f);
}
