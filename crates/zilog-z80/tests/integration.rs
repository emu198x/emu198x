use zilog_z80::Z80;
use zilog_z80::registers::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct IoWrite {
    addr: u16,
    data: u8,
}

/// Run the Z80 for a given number of half-cycles with a flat memory bus.
/// Returns the number of half-cycles actually executed.
fn run(z80: &mut Z80, mem: &mut [u8; 65536], max_hc: u32) -> u32 {
    let mut hc = 0u32;
    while hc < max_hc {
        z80.tick();

        // Handle bus transactions
        if z80.mreq && z80.rd {
            z80.data_in = mem[z80.addr as usize];
        } else if z80.mreq && z80.wr {
            mem[z80.addr as usize] = z80.data;
        } else if z80.iorq && z80.rd && !z80.m1 {
            z80.data_in = (z80.addr >> 8) as u8; // FUSE convention
        }

        hc += 1;

        // Stop if HALTed
        if z80.halt {
            break;
        }
    }
    hc
}

/// Run the Z80 and capture any I/O writes observed on the bus.
fn run_with_io_trace(z80: &mut Z80, mem: &mut [u8; 65536], max_hc: u32) -> Vec<IoWrite> {
    let mut writes = Vec::new();
    let mut hc = 0u32;
    let mut io_write_active = false;
    while hc < max_hc {
        z80.tick();

        if z80.mreq && z80.rd {
            z80.data_in = mem[z80.addr as usize];
        } else if z80.mreq && z80.wr {
            mem[z80.addr as usize] = z80.data;
        } else if z80.iorq && z80.rd && !z80.m1 {
            z80.data_in = (z80.addr >> 8) as u8;
        } else if z80.iorq && z80.wr {
            if !io_write_active {
                writes.push(IoWrite {
                    addr: z80.addr,
                    data: z80.data,
                });
                io_write_active = true;
            }
        } else {
            io_write_active = false;
        }

        hc += 1;

        if z80.halt {
            break;
        }
    }

    writes
}

#[test]
fn ld_a_immediate() {
    let mut z80 = Z80::new();
    let mut mem = [0u8; 65536];
    // LD A, 0x42
    mem[0] = 0x3E;
    mem[1] = 0x42;
    // HALT
    mem[2] = 0x76;

    run(&mut z80, &mut mem, 1000);
    assert_eq!(z80.regs.a(), 0x42);
}

#[test]
fn ld_register_to_register() {
    let mut z80 = Z80::new();
    let mut mem = [0u8; 65536];
    // LD A, 0x55
    mem[0] = 0x3E;
    mem[1] = 0x55;
    // LD B, A
    mem[2] = 0x47;
    // HALT
    mem[3] = 0x76;

    run(&mut z80, &mut mem, 1000);
    assert_eq!(z80.regs.a(), 0x55);
    assert_eq!(z80.regs.b(), 0x55);
}

#[test]
fn add_a_register() {
    let mut z80 = Z80::new();
    let mut mem = [0u8; 65536];
    // LD A, 0x10
    mem[0] = 0x3E;
    mem[1] = 0x10;
    // LD B, 0x20
    mem[2] = 0x06;
    mem[3] = 0x20;
    // ADD A, B
    mem[4] = 0x80;
    // HALT
    mem[5] = 0x76;

    run(&mut z80, &mut mem, 1000);
    assert_eq!(z80.regs.a(), 0x30);
}

#[test]
fn ld_rr_immediate() {
    let mut z80 = Z80::new();
    let mut mem = [0u8; 65536];
    // LD BC, 0x1234
    mem[0] = 0x01;
    mem[1] = 0x34; // low
    mem[2] = 0x12; // high
    // LD DE, 0x5678
    mem[3] = 0x11;
    mem[4] = 0x78;
    mem[5] = 0x56;
    // HALT
    mem[6] = 0x76;

    run(&mut z80, &mut mem, 1000);
    assert_eq!(z80.regs.bc, 0x1234);
    assert_eq!(z80.regs.de, 0x5678);
}

#[test]
fn jp_unconditional() {
    let mut z80 = Z80::new();
    let mut mem = [0u8; 65536];
    // JP 0x0010
    mem[0] = 0xC3;
    mem[1] = 0x10; // low
    mem[2] = 0x00; // high
    // HALT at 0x0010
    mem[0x10] = 0x76;

    run(&mut z80, &mut mem, 1000);
    assert!(z80.halt);
    // While halted, PC oscillates between the HALT byte (0x0010) and
    // the one after as the CPU re-fetches phantom NOPs. `run` exits as
    // soon as halt latches — at that moment PC is at the HALT byte
    // because `begin_next_instruction` has decremented it ready for
    // the next phantom M1 fetch.
    assert_eq!(z80.regs.pc, 0x0010);
}

#[test]
fn jp_conditional_taken() {
    let mut z80 = Z80::new();
    let mut mem = [0u8; 65536];

    // LD A, 0x00
    mem[0] = 0x3E;
    mem[1] = 0x00;
    // OR A (sets Z)
    mem[2] = 0xB7;
    // JP Z, 0x0010
    mem[3] = 0xCA;
    mem[4] = 0x10;
    mem[5] = 0x00;
    // LD B, 0xFF (should be skipped)
    mem[6] = 0x06;
    mem[7] = 0xFF;
    // Target: LD B, 0x42; HALT
    mem[0x10] = 0x06;
    mem[0x11] = 0x42;
    mem[0x12] = 0x76;

    run(&mut z80, &mut mem, 2000);
    assert_eq!(z80.regs.b(), 0x42);
    assert!(z80.halt);
}

#[test]
fn jp_conditional_not_taken() {
    let mut z80 = Z80::new();
    let mut mem = [0u8; 65536];

    // LD A, 0x01
    mem[0] = 0x3E;
    mem[1] = 0x01;
    // OR A (clears Z)
    mem[2] = 0xB7;
    // JP Z, 0x0010
    mem[3] = 0xCA;
    mem[4] = 0x10;
    mem[5] = 0x00;
    // LD B, 0x42; HALT
    mem[6] = 0x06;
    mem[7] = 0x42;
    mem[8] = 0x76;
    // Target path should be skipped
    mem[0x10] = 0x06;
    mem[0x11] = 0xFF;
    mem[0x12] = 0x76;

    run(&mut z80, &mut mem, 2000);
    assert_eq!(z80.regs.b(), 0x42);
    assert!(z80.halt);
}

#[test]
fn jp_indirect_hl() {
    let mut z80 = Z80::new();
    let mut mem = [0u8; 65536];

    // LD HL, 0x0010
    mem[0] = 0x21;
    mem[1] = 0x10;
    mem[2] = 0x00;
    // JP (HL)
    mem[3] = 0xE9;
    // HALT at 0x0010
    mem[0x10] = 0x76;

    run(&mut z80, &mut mem, 2000);
    assert!(z80.halt);
    // HALT sits at 0x0010; PC is at the HALT byte when halt latches
    // (see comment in `jp_unconditional`).
    assert_eq!(z80.regs.pc, 0x0010);
}

#[test]
fn push_pop() {
    let mut z80 = Z80::new();
    let mut mem = [0u8; 65536];
    z80.regs.sp = 0xFFFE;

    // LD BC, 0xABCD
    mem[0] = 0x01;
    mem[1] = 0xCD;
    mem[2] = 0xAB;
    // PUSH BC
    mem[3] = 0xC5;
    // LD BC, 0x0000
    mem[4] = 0x01;
    mem[5] = 0x00;
    mem[6] = 0x00;
    // POP DE
    mem[7] = 0xD1;
    // HALT
    mem[8] = 0x76;

    run(&mut z80, &mut mem, 2000);
    assert_eq!(z80.regs.de, 0xABCD);
    assert_eq!(z80.regs.sp, 0xFFFE); // SP restored after push+pop
}

#[test]
fn call_conditional_taken() {
    let mut z80 = Z80::new();
    let mut mem = [0u8; 65536];
    z80.regs.sp = 0xFFFE;

    // LD A, 0x00
    mem[0] = 0x3E;
    mem[1] = 0x00;
    // OR A (sets Z)
    mem[2] = 0xB7;
    // CALL Z, 0x0010
    mem[3] = 0xCC;
    mem[4] = 0x10;
    mem[5] = 0x00;
    // HALT at return address
    mem[6] = 0x76;

    // Subroutine: LD B, 0x42; RET
    mem[0x10] = 0x06;
    mem[0x11] = 0x42;
    mem[0x12] = 0xC9;

    run(&mut z80, &mut mem, 4000);
    assert_eq!(z80.regs.b(), 0x42);
    assert_eq!(z80.regs.sp, 0xFFFE);
    assert!(z80.halt);
}

#[test]
fn call_conditional_not_taken() {
    let mut z80 = Z80::new();
    let mut mem = [0u8; 65536];
    z80.regs.sp = 0xFFFE;

    // LD A, 0x01
    mem[0] = 0x3E;
    mem[1] = 0x01;
    // OR A (clears Z)
    mem[2] = 0xB7;
    // CALL Z, 0x0010
    mem[3] = 0xCC;
    mem[4] = 0x10;
    mem[5] = 0x00;
    // LD B, 0x55; HALT
    mem[6] = 0x06;
    mem[7] = 0x55;
    mem[8] = 0x76;

    // Untaken subroutine path
    mem[0x10] = 0x06;
    mem[0x11] = 0xFF;
    mem[0x12] = 0xC9;

    run(&mut z80, &mut mem, 4000);
    assert_eq!(z80.regs.b(), 0x55);
    assert_eq!(z80.regs.sp, 0xFFFE);
    assert!(z80.halt);
}

#[test]
fn call_ret() {
    let mut z80 = Z80::new();
    let mut mem = [0u8; 65536];
    z80.regs.sp = 0xFFFE;

    // CALL 0x0020
    mem[0] = 0xCD;
    mem[1] = 0x20;
    mem[2] = 0x00;
    // HALT (return address)
    mem[3] = 0x76;

    // Subroutine at 0x0020: LD A, 0x99; RET
    mem[0x20] = 0x3E;
    mem[0x21] = 0x99;
    mem[0x22] = 0xC9; // RET

    run(&mut z80, &mut mem, 2000);
    assert_eq!(z80.regs.a(), 0x99);
    assert!(z80.halt); // Returned to HALT at 0x0003
}

#[test]
fn ret_conditional_taken() {
    let mut z80 = Z80::new();
    let mut mem = [0u8; 65536];
    z80.regs.sp = 0xFFFE;

    // LD A, 0x00
    mem[0] = 0x3E;
    mem[1] = 0x00;
    // OR A (sets Z)
    mem[2] = 0xB7;
    // CALL 0x0010
    mem[3] = 0xCD;
    mem[4] = 0x10;
    mem[5] = 0x00;
    // HALT at return address
    mem[6] = 0x76;

    // RET Z should return immediately
    mem[0x10] = 0xC8;
    // Should be skipped
    mem[0x11] = 0x06;
    mem[0x12] = 0xFF;
    mem[0x13] = 0xC9;

    run(&mut z80, &mut mem, 4000);
    assert_eq!(z80.regs.b(), 0x00);
    assert_eq!(z80.regs.sp, 0xFFFE);
    assert!(z80.halt);
}

#[test]
fn ret_conditional_not_taken() {
    let mut z80 = Z80::new();
    let mut mem = [0u8; 65536];
    z80.regs.sp = 0xFFFE;

    // LD A, 0x01
    mem[0] = 0x3E;
    mem[1] = 0x01;
    // OR A (clears Z)
    mem[2] = 0xB7;
    // CALL 0x0010
    mem[3] = 0xCD;
    mem[4] = 0x10;
    mem[5] = 0x00;
    // HALT at return address
    mem[6] = 0x76;

    // RET Z not taken, then LD B,0x66, RET
    mem[0x10] = 0xC8;
    mem[0x11] = 0x06;
    mem[0x12] = 0x66;
    mem[0x13] = 0xC9;

    run(&mut z80, &mut mem, 4000);
    assert_eq!(z80.regs.b(), 0x66);
    assert_eq!(z80.regs.sp, 0xFFFE);
    assert!(z80.halt);
}

#[test]
fn inc_dec_8bit() {
    let mut z80 = Z80::new();
    let mut mem = [0u8; 65536];

    // LD A, 0xFF
    mem[0] = 0x3E;
    mem[1] = 0xFF;
    // INC A
    mem[2] = 0x3C;
    // HALT
    mem[3] = 0x76;

    run(&mut z80, &mut mem, 1000);
    assert_eq!(z80.regs.a(), 0x00);
    assert!(z80.regs.flag(zilog_z80::registers::FLAG_Z));
    assert!(z80.regs.flag(zilog_z80::registers::FLAG_H));
}

#[test]
fn ld_a_bc_de_and_store_paths_roundtrip() {
    let mut z80 = Z80::new();
    let mut mem = [0u8; 65536];

    // LD BC, 0x8000
    mem[0] = 0x01;
    mem[1] = 0x00;
    mem[2] = 0x80;
    // LD DE, 0x8001
    mem[3] = 0x11;
    mem[4] = 0x01;
    mem[5] = 0x80;
    // LD A, 0x42
    mem[6] = 0x3E;
    mem[7] = 0x42;
    // LD (BC), A
    mem[8] = 0x02;
    // LD A, 0x66
    mem[9] = 0x3E;
    mem[10] = 0x66;
    // LD (DE), A
    mem[11] = 0x12;
    // LD A, 0x00
    mem[12] = 0x3E;
    mem[13] = 0x00;
    // LD A, (BC)
    mem[14] = 0x0A;
    // LD A, (DE)
    mem[15] = 0x1A;
    // HALT
    mem[16] = 0x76;

    run(&mut z80, &mut mem, 4000);
    assert_eq!(mem[0x8000], 0x42);
    assert_eq!(mem[0x8001], 0x66);
    assert_eq!(z80.regs.a(), 0x66);
}

#[test]
fn ld_a_indirect_nn_roundtrip() {
    let mut z80 = Z80::new();
    let mut mem = [0u8; 65536];

    // LD A, 0x5A
    mem[0] = 0x3E;
    mem[1] = 0x5A;
    // LD (0x8123), A
    mem[2] = 0x32;
    mem[3] = 0x23;
    mem[4] = 0x81;
    // LD A, 0x00
    mem[5] = 0x3E;
    mem[6] = 0x00;
    // LD A, (0x8123)
    mem[7] = 0x3A;
    mem[8] = 0x23;
    mem[9] = 0x81;
    // HALT
    mem[10] = 0x76;

    run(&mut z80, &mut mem, 4000);
    assert_eq!(mem[0x8123], 0x5A);
    assert_eq!(z80.regs.a(), 0x5A);
}

#[test]
fn ld_hl_indirect_nn_roundtrip() {
    let mut z80 = Z80::new();
    let mut mem = [0u8; 65536];

    // LD HL, 0xBEEF
    mem[0] = 0x21;
    mem[1] = 0xEF;
    mem[2] = 0xBE;
    // LD (0x9000), HL
    mem[3] = 0x22;
    mem[4] = 0x00;
    mem[5] = 0x90;
    // LD HL, 0x0000
    mem[6] = 0x21;
    mem[7] = 0x00;
    mem[8] = 0x00;
    // LD HL, (0x9000)
    mem[9] = 0x2A;
    mem[10] = 0x00;
    mem[11] = 0x90;
    // HALT
    mem[12] = 0x76;

    run(&mut z80, &mut mem, 5000);
    assert_eq!(mem[0x9000], 0xEF);
    assert_eq!(mem[0x9001], 0xBE);
    assert_eq!(z80.regs.hl, 0xBEEF);
}

#[test]
fn inc_dec_hl_memory_updates_byte_and_flags() {
    let mut z80 = Z80::new();
    let mut mem = [0u8; 65536];

    // LD HL, 0x8000
    mem[0] = 0x21;
    mem[1] = 0x00;
    mem[2] = 0x80;
    // LD (HL), 0xFF
    mem[3] = 0x36;
    mem[4] = 0xFF;
    // INC (HL)
    mem[5] = 0x34;
    // DEC (HL)
    mem[6] = 0x35;
    // HALT
    mem[7] = 0x76;

    run(&mut z80, &mut mem, 5000);
    assert_eq!(mem[0x8000], 0xFF);
    assert!(z80.regs.flag(FLAG_S));
    assert!(z80.regs.flag(FLAG_H));
    assert!(z80.regs.flag(FLAG_N));
    assert!(!z80.regs.flag(FLAG_Z));
}

#[test]
fn ld_hl_memory() {
    let mut z80 = Z80::new();
    let mut mem = [0u8; 65536];

    // LD HL, 0x8000
    mem[0] = 0x21;
    mem[1] = 0x00;
    mem[2] = 0x80;
    // LD (HL), 0x42
    mem[3] = 0x36;
    mem[4] = 0x42;
    // LD A, (HL)
    mem[5] = 0x7E;
    // HALT
    mem[6] = 0x76;

    run(&mut z80, &mut mem, 2000);
    assert_eq!(mem[0x8000], 0x42);
    assert_eq!(z80.regs.a(), 0x42);
}

#[test]
fn inc_dec_rr_and_add_hl_rr_update_pairs() {
    let mut z80 = Z80::new();
    let mut mem = [0u8; 65536];

    z80.regs.sp = 0x0001;

    // LD BC, 0x1234
    mem[0] = 0x01;
    mem[1] = 0x34;
    mem[2] = 0x12;
    // INC BC
    mem[3] = 0x03;
    // DEC BC
    mem[4] = 0x0B;
    // LD HL, 0xFFFF
    mem[5] = 0x21;
    mem[6] = 0xFF;
    mem[7] = 0xFF;
    // ADD HL, SP
    mem[8] = 0x39;
    // HALT
    mem[9] = 0x76;

    run(&mut z80, &mut mem, 4000);
    assert_eq!(z80.regs.bc, 0x1234);
    assert_eq!(z80.regs.hl, 0x0000);
    assert!(z80.regs.flag(FLAG_C));
    assert!(!z80.regs.flag(FLAG_N));
}

#[test]
fn djnz_taken_and_not_taken_paths() {
    let mut z80 = Z80::new();
    let mut mem = [0u8; 65536];

    // LD B, 0x02
    mem[0] = 0x06;
    mem[1] = 0x02;
    // DJNZ +2 -> jump to HALT at 0x0006
    mem[2] = 0x10;
    mem[3] = 0x02;
    // LD A, 0xFF (should be skipped on the taken branch)
    mem[4] = 0x3E;
    mem[5] = 0xFF;
    // HALT
    mem[6] = 0x76;

    run(&mut z80, &mut mem, 2000);
    assert_eq!(z80.regs.b(), 0x01);
    // HALT sits at 0x0006; PC is at the HALT byte when halt latches.
    assert_eq!(z80.regs.pc, 0x0006);
    assert!(z80.halt);
}

#[test]
fn djnz_not_taken_runs_fallthrough() {
    let mut z80 = Z80::new();
    let mut mem = [0u8; 65536];

    // LD B, 0x01
    mem[0] = 0x06;
    mem[1] = 0x01;
    // DJNZ +2 (not taken after decrement to zero)
    mem[2] = 0x10;
    mem[3] = 0x02;
    // LD A, 0x33; HALT
    mem[4] = 0x3E;
    mem[5] = 0x33;
    mem[6] = 0x76;

    run(&mut z80, &mut mem, 2000);
    assert_eq!(z80.regs.b(), 0x00);
    assert_eq!(z80.regs.a(), 0x33);
    assert!(z80.halt);
}

#[test]
fn jr_conditional() {
    let mut z80 = Z80::new();
    let mut mem = [0u8; 65536];

    // LD A, 0x00
    mem[0] = 0x3E;
    mem[1] = 0x00;
    // OR A (sets Z flag)
    mem[2] = 0xB7;
    // JR Z, +3 (skip to HALT at 0x0008)
    mem[3] = 0x28;
    mem[4] = 0x03; // offset = +3 from PC after fetch (PC=5, 5+3=8)
    // LD A, 0xFF (should be skipped)
    mem[5] = 0x3E;
    mem[6] = 0xFF;
    // HALT (jumped over)
    mem[7] = 0x76;
    // HALT (jump target)
    mem[8] = 0x76;

    run(&mut z80, &mut mem, 2000);
    assert_eq!(z80.regs.a(), 0x00); // The LD A, 0xFF was skipped
    assert!(z80.halt);
}

#[test]
fn rst_38_pushes_return_address() {
    let mut z80 = Z80::new();
    let mut mem = [0u8; 65536];
    z80.regs.sp = 0xFFFE;

    // RST 38h
    mem[0] = 0xFF;
    // HALT at vector
    mem[0x38] = 0x76;

    run(&mut z80, &mut mem, 4000);
    assert_eq!(z80.regs.sp, 0xFFFC);
    assert_eq!(mem[0xFFFC], 0x01);
    assert_eq!(mem[0xFFFD], 0x00);
    // HALT sits at the RST 38 vector (0x0038); PC is at that byte when
    // halt latches.
    assert_eq!(z80.regs.pc, 0x0038);
    assert!(z80.halt);
}

#[test]
fn ex_sp_hl_swaps_register_and_stack_value() {
    let mut z80 = Z80::new();
    let mut mem = [0u8; 65536];
    z80.regs.sp = 0x9000;
    mem[0x9000] = 0x34;
    mem[0x9001] = 0x12;

    // LD HL, 0xABCD
    mem[0] = 0x21;
    mem[1] = 0xCD;
    mem[2] = 0xAB;
    // EX (SP), HL
    mem[3] = 0xE3;
    // HALT
    mem[4] = 0x76;

    run(&mut z80, &mut mem, 5000);
    assert_eq!(z80.regs.hl, 0x1234);
    assert_eq!(mem[0x9000], 0xCD);
    assert_eq!(mem[0x9001], 0xAB);
}

#[test]
fn cb_rlc_register() {
    let mut z80 = Z80::new();
    let mut mem = [0u8; 65536];

    // LD A, 0x85 (10000101)
    mem[0] = 0x3E;
    mem[1] = 0x85;
    // RLC A (CB 07)
    mem[2] = 0xCB;
    mem[3] = 0x07;
    // HALT
    mem[4] = 0x76;

    run(&mut z80, &mut mem, 2000);
    assert_eq!(z80.regs.a(), 0x0B); // 00001011, carry = 1
    assert!(z80.regs.flag(FLAG_C));
}

#[test]
fn rotate_a_instructions_use_bit7_and_carry_paths() {
    let mut z80 = Z80::new();
    let mut mem = [0u8; 65536];

    // LD A, 0x81
    mem[0] = 0x3E;
    mem[1] = 0x81;
    // RLCA
    mem[2] = 0x07;
    // RRCA
    mem[3] = 0x0F;
    // SCF
    mem[4] = 0x37;
    // RLA
    mem[5] = 0x17;
    // RRA
    mem[6] = 0x1F;
    // HALT
    mem[7] = 0x76;

    run(&mut z80, &mut mem, 4000);
    assert_eq!(z80.regs.a(), 0x81);
    assert!(z80.regs.flag(FLAG_C));
}

#[test]
fn daa_adjusts_bcd_sum() {
    let mut z80 = Z80::new();
    let mut mem = [0u8; 65536];

    // LD A, 0x09
    mem[0] = 0x3E;
    mem[1] = 0x09;
    // ADD A, 0x09
    mem[2] = 0xC6;
    mem[3] = 0x09;
    // DAA
    mem[4] = 0x27;
    // HALT
    mem[5] = 0x76;

    run(&mut z80, &mut mem, 3000);
    assert_eq!(z80.regs.a(), 0x18);
    assert!(!z80.regs.flag(FLAG_C));
    assert!(!z80.regs.flag(FLAG_N));
}

#[test]
fn cpl_scf_and_ccf_update_flags() {
    let mut z80 = Z80::new();
    let mut mem = [0u8; 65536];

    // LD A, 0x55
    mem[0] = 0x3E;
    mem[1] = 0x55;
    // SCF
    mem[2] = 0x37;
    // CCF
    mem[3] = 0x3F;
    // CPL
    mem[4] = 0x2F;
    // HALT
    mem[5] = 0x76;

    run(&mut z80, &mut mem, 3000);
    assert_eq!(z80.regs.a(), 0xAA);
    assert!(!z80.regs.flag(FLAG_C));
    assert!(z80.regs.flag(FLAG_H));
    assert!(z80.regs.flag(FLAG_N));
}

#[test]
fn cb_bit_test() {
    let mut z80 = Z80::new();
    let mut mem = [0u8; 65536];

    // LD B, 0x80
    mem[0] = 0x06;
    mem[1] = 0x80;
    // BIT 7, B (CB 78)
    mem[2] = 0xCB;
    mem[3] = 0x78;
    // HALT
    mem[4] = 0x76;

    run(&mut z80, &mut mem, 2000);
    assert!(!z80.regs.flag(FLAG_Z)); // bit 7 is set
    assert!(z80.regs.flag(FLAG_H)); // H always set for BIT
}

#[test]
fn cb_set_res() {
    let mut z80 = Z80::new();
    let mut mem = [0u8; 65536];

    // LD A, 0x00
    mem[0] = 0x3E;
    mem[1] = 0x00;
    // SET 3, A (CB DF)
    mem[2] = 0xCB;
    mem[3] = 0xDF;
    // HALT
    mem[4] = 0x76;

    run(&mut z80, &mut mem, 2000);
    assert_eq!(z80.regs.a(), 0x08); // bit 3 set
}

#[test]
fn ed_neg() {
    let mut z80 = Z80::new();
    let mut mem = [0u8; 65536];

    // LD A, 0x01
    mem[0] = 0x3E;
    mem[1] = 0x01;
    // NEG (ED 44)
    mem[2] = 0xED;
    mem[3] = 0x44;
    // HALT
    mem[4] = 0x76;

    run(&mut z80, &mut mem, 2000);
    assert_eq!(z80.regs.a(), 0xFF); // 0 - 1 = 0xFF
    assert!(z80.regs.flag(FLAG_S));
    assert!(z80.regs.flag(FLAG_C));
    assert!(z80.regs.flag(FLAG_N));
}

#[test]
fn ed_ld_i_a() {
    let mut z80 = Z80::new();
    let mut mem = [0u8; 65536];

    // LD A, 0x3F
    mem[0] = 0x3E;
    mem[1] = 0x3F;
    // LD I, A (ED 47)
    mem[2] = 0xED;
    mem[3] = 0x47;
    // HALT
    mem[4] = 0x76;

    run(&mut z80, &mut mem, 2000);
    assert_eq!(z80.regs.i, 0x3F);
}

#[test]
fn ed_ld_r_a_then_ld_a_r_roundtrip_matches_refresh_counter_rules() {
    let mut z80 = Z80::new();
    let mut mem = [0u8; 65536];

    z80.regs.set_a(0x55);
    z80.regs.set_f(FLAG_C);
    z80.regs.iff2 = true;

    // LD R, A; LD A, R; HALT
    mem[0] = 0xED;
    mem[1] = 0x4F;
    mem[2] = 0xED;
    mem[3] = 0x5F;
    mem[4] = 0x76;

    run(&mut z80, &mut mem, 4000);
    assert_eq!(z80.regs.a(), 0x57);
    assert!(z80.regs.flag(FLAG_C));
    assert!(z80.regs.flag(FLAG_PV));
    assert!(!z80.regs.flag(FLAG_3));
    assert!(!z80.regs.flag(FLAG_5));
    assert!(!z80.regs.flag(FLAG_S));
    assert!(!z80.regs.flag(FLAG_Z));
    assert!(!z80.regs.flag(FLAG_H));
    assert!(!z80.regs.flag(FLAG_N));
}

#[test]
fn ed_ld_a_i_sets_flags_from_i_and_iff2() {
    let mut z80 = Z80::new();
    let mut mem = [0u8; 65536];

    z80.regs.i = 0xA8;
    z80.regs.af = FLAG_C as u16;
    z80.regs.iff2 = true;

    // LD A, I (ED 57)
    mem[0] = 0xED;
    mem[1] = 0x57;
    // HALT
    mem[2] = 0x76;

    run(&mut z80, &mut mem, 2000);
    assert_eq!(z80.regs.a(), 0xA8);
    assert!(z80.regs.flag(FLAG_S));
    assert!(z80.regs.flag(FLAG_PV));
    assert!(z80.regs.flag(FLAG_C));
    assert!(z80.regs.flag(FLAG_3));
    assert!(z80.regs.flag(FLAG_5));
    assert!(!z80.regs.flag(FLAG_Z));
    assert!(!z80.regs.flag(FLAG_H));
    assert!(!z80.regs.flag(FLAG_N));
}

#[test]
fn ed_retn_restores_pc_sp_and_iff1_from_iff2() {
    let mut z80 = Z80::new();
    let mut mem = [0u8; 65536];

    z80.regs.sp = 0x9000;
    z80.regs.iff1 = false;
    z80.regs.iff2 = true;
    mem[0x9000] = 0x34;
    mem[0x9001] = 0x12;

    // RETN (ED 45)
    mem[0] = 0xED;
    mem[1] = 0x45;
    // HALT at return target
    mem[0x1234] = 0x76;

    run(&mut z80, &mut mem, 4000);
    assert_eq!(z80.regs.sp, 0x9002);
    // HALT sits at the return target (0x1234); PC is at that byte when
    // halt latches.
    assert_eq!(z80.regs.pc, 0x1234);
    assert_eq!(z80.regs.wz, 0x1234);
    assert!(z80.regs.iff1);
    assert!(z80.halt);
}

#[test]
fn ed_im_mode_switches_cover_1_2_and_0() {
    let mut z80 = Z80::new();
    let mut mem = [0u8; 65536];

    // IM 1; IM 2; IM 0
    mem[0] = 0xED;
    mem[1] = 0x56;
    mem[2] = 0xED;
    mem[3] = 0x5E;
    mem[4] = 0xED;
    mem[5] = 0x46;
    // HALT
    mem[6] = 0x76;

    run(&mut z80, &mut mem, 5000);
    assert_eq!(z80.regs.im, 0);
    assert!(z80.halt);
}

#[test]
fn ed_im_undocumented_opcode_still_selects_mode_0() {
    let mut z80 = Z80::new();
    let mut mem = [0u8; 65536];

    z80.regs.im = 2;

    // Undocumented IM 0 opcode (ED 4E)
    mem[0] = 0xED;
    mem[1] = 0x4E;
    mem[2] = 0x76;

    run(&mut z80, &mut mem, 3000);
    assert_eq!(z80.regs.im, 0);
}

#[test]
fn ed_in_r_c_reads_port_data_and_sets_flags() {
    let mut z80 = Z80::new();
    let mut mem = [0u8; 65536];

    z80.regs.bc = 0x81FE;
    z80.regs.af = FLAG_C as u16;

    // IN E, (C) (ED 58)
    mem[0] = 0xED;
    mem[1] = 0x58;
    // HALT
    mem[2] = 0x76;

    run(&mut z80, &mut mem, 3000);
    assert_eq!(z80.regs.e(), 0x81);
    assert_eq!(z80.regs.wz, 0x81FF);
    assert!(z80.regs.flag(FLAG_S));
    assert!(z80.regs.flag(FLAG_PV));
    assert!(z80.regs.flag(FLAG_C));
    assert!(!z80.regs.flag(FLAG_Z));
    assert!(!z80.regs.flag(FLAG_H));
    assert!(!z80.regs.flag(FLAG_N));
}

#[test]
fn ed_in_f_c_updates_flags_without_overwriting_registers() {
    let mut z80 = Z80::new();
    let mut mem = [0u8; 65536];

    z80.regs.af = 0xAA01;
    z80.regs.bc = 0x00FE;
    z80.regs.hl = 0x1234;

    // Undocumented IN F, (C) form (ED 70)
    mem[0] = 0xED;
    mem[1] = 0x70;
    mem[2] = 0x76;

    run(&mut z80, &mut mem, 3000);
    assert_eq!(z80.regs.a(), 0xAA);
    assert_eq!(z80.regs.bc, 0x00FE);
    assert_eq!(z80.regs.hl, 0x1234);
    assert_eq!(z80.regs.f(), FLAG_Z | FLAG_PV | FLAG_C);
    assert_eq!(z80.regs.wz, 0x00FF);
}

#[test]
fn ed_out_c_r_drives_io_bus() {
    let mut z80 = Z80::new();
    let mut mem = [0u8; 65536];

    z80.regs.bc = 0x12FE;
    z80.regs.de = 0xA500;

    // OUT (C), D (ED 51)
    mem[0] = 0xED;
    mem[1] = 0x51;
    // HALT
    mem[2] = 0x76;

    let writes = run_with_io_trace(&mut z80, &mut mem, 3000);
    assert_eq!(
        writes,
        vec![IoWrite {
            addr: 0x12FE,
            data: 0xA5,
        }]
    );
    assert_eq!(z80.regs.wz, 0x12FF);
}

#[test]
fn ed_out_c_zero_uses_zero_for_undocumented_r6_form() {
    let mut z80 = Z80::new();
    let mut mem = [0u8; 65536];

    z80.regs.bc = 0x34FE;
    z80.regs.hl = 0xBEEF;

    // Undocumented OUT (C), 0 form (ED 71)
    mem[0] = 0xED;
    mem[1] = 0x71;
    mem[2] = 0x76;

    let writes = run_with_io_trace(&mut z80, &mut mem, 3000);
    assert_eq!(
        writes,
        vec![IoWrite {
            addr: 0x34FE,
            data: 0x00,
        }]
    );
    assert_eq!(z80.regs.hl, 0xBEEF);
    assert_eq!(z80.regs.wz, 0x34FF);
}

#[test]
fn ed_adc_hl_bc_updates_result_and_flags() {
    let mut z80 = Z80::new();
    let mut mem = [0u8; 65536];

    z80.regs.af = 0x0000;
    z80.regs.hl = 0x7FFF;
    z80.regs.bc = 0x0001;

    // ADC HL, BC (ED 4A)
    mem[0] = 0xED;
    mem[1] = 0x4A;
    // HALT
    mem[2] = 0x76;

    run(&mut z80, &mut mem, 3000);
    assert_eq!(z80.regs.hl, 0x8000);
    assert_eq!(z80.regs.wz, 0x8000);
    assert!(z80.regs.flag(FLAG_S));
    assert!(z80.regs.flag(FLAG_H));
    assert!(z80.regs.flag(FLAG_PV));
    assert!(!z80.regs.flag(FLAG_C));
    assert!(!z80.regs.flag(FLAG_N));
}

#[test]
fn ed_sbc_hl_de_with_carry_sets_borrow_flags() {
    let mut z80 = Z80::new();
    let mut mem = [0u8; 65536];

    z80.regs.af = FLAG_C as u16;
    z80.regs.hl = 0x0000;
    z80.regs.de = 0x0000;

    // SBC HL, DE (ED 52)
    mem[0] = 0xED;
    mem[1] = 0x52;
    // HALT
    mem[2] = 0x76;

    run(&mut z80, &mut mem, 3000);
    assert_eq!(z80.regs.hl, 0xFFFF);
    assert_eq!(z80.regs.wz, 0x0001);
    assert!(z80.regs.flag(FLAG_S));
    assert!(z80.regs.flag(FLAG_H));
    assert!(z80.regs.flag(FLAG_C));
    assert!(z80.regs.flag(FLAG_N));
    assert!(!z80.regs.flag(FLAG_Z));
}

#[test]
fn ed_ld_rr_indirect_roundtrip() {
    let mut z80 = Z80::new();
    let mut mem = [0u8; 65536];

    z80.regs.bc = 0x1234;

    // LD (0x9000), BC
    mem[0] = 0xED;
    mem[1] = 0x43;
    mem[2] = 0x00;
    mem[3] = 0x90;
    // LD DE, (0x9000)
    mem[4] = 0xED;
    mem[5] = 0x5B;
    mem[6] = 0x00;
    mem[7] = 0x90;
    // HALT
    mem[8] = 0x76;

    run(&mut z80, &mut mem, 6000);
    assert_eq!(mem[0x9000], 0x34);
    assert_eq!(mem[0x9001], 0x12);
    assert_eq!(z80.regs.de, 0x1234);
    assert_eq!(z80.regs.wz, 0x9001);
}

#[test]
fn ed_rld_updates_a_and_memory_nibbles() {
    let mut z80 = Z80::new();
    let mut mem = [0u8; 65536];

    z80.regs.hl = 0x9000;
    z80.regs.set_a(0x3C);
    mem[0x9000] = 0xA5;

    // RLD (ED 6F)
    mem[0] = 0xED;
    mem[1] = 0x6F;
    // HALT
    mem[2] = 0x76;

    run(&mut z80, &mut mem, 4000);
    assert_eq!(z80.regs.a(), 0x3A);
    assert_eq!(mem[0x9000], 0x5C);
    assert_eq!(z80.regs.wz, 0x9001);
}

#[test]
fn ed_rrd_updates_a_and_memory_nibbles() {
    let mut z80 = Z80::new();
    let mut mem = [0u8; 65536];

    z80.regs.hl = 0x9000;
    z80.regs.set_a(0x3C);
    mem[0x9000] = 0xA5;

    // RRD (ED 67)
    mem[0] = 0xED;
    mem[1] = 0x67;
    // HALT
    mem[2] = 0x76;

    run(&mut z80, &mut mem, 4000);
    assert_eq!(z80.regs.a(), 0x35);
    assert_eq!(mem[0x9000], 0xCA);
    assert_eq!(z80.regs.wz, 0x9001);
}

#[test]
fn ed_ldi_block_transfer() {
    let mut z80 = Z80::new();
    let mut mem = [0u8; 65536];

    // Set up: copy byte from 0x8000 to 0x9000
    // LD HL, 0x8000
    mem[0] = 0x21;
    mem[1] = 0x00;
    mem[2] = 0x80;
    // LD DE, 0x9000
    mem[3] = 0x11;
    mem[4] = 0x00;
    mem[5] = 0x90;
    // LD BC, 0x0001
    mem[6] = 0x01;
    mem[7] = 0x01;
    mem[8] = 0x00;
    // Source byte
    mem[0x8000] = 0xAA;
    // LDI (ED A0)
    mem[9] = 0xED;
    mem[10] = 0xA0;
    // HALT
    mem[11] = 0x76;

    run(&mut z80, &mut mem, 5000);
    assert_eq!(mem[0x9000], 0xAA); // Byte copied
    assert_eq!(z80.regs.hl, 0x8001); // HL incremented
    assert_eq!(z80.regs.de, 0x9001); // DE incremented
    assert_eq!(z80.regs.bc, 0x0000); // BC decremented
    assert!(!z80.regs.flag(FLAG_PV)); // BC = 0, PV clear
}

#[test]
fn ed_ldd_block_transfer_moves_backward() {
    let mut z80 = Z80::new();
    let mut mem = [0u8; 65536];

    z80.regs.hl = 0x8001;
    z80.regs.de = 0x9001;
    z80.regs.bc = 0x0001;
    mem[0x8001] = 0xAA;

    // LDD (ED A8)
    mem[0] = 0xED;
    mem[1] = 0xA8;
    // HALT
    mem[2] = 0x76;

    run(&mut z80, &mut mem, 4000);
    assert_eq!(mem[0x9001], 0xAA);
    assert_eq!(z80.regs.hl, 0x8000);
    assert_eq!(z80.regs.de, 0x9000);
    assert_eq!(z80.regs.bc, 0x0000);
    assert!(!z80.regs.flag(FLAG_PV));
}

#[test]
fn lddr_block_transfer_multiple_moves_backward_until_bc_zero() {
    let mut z80 = Z80::new();
    let mut mem = [0u8; 65536];

    z80.regs.hl = 0x8001;
    z80.regs.de = 0x9001;
    z80.regs.bc = 0x0002;
    mem[0x8000] = 0xDE;
    mem[0x8001] = 0xAD;

    // LDDR (ED B8)
    mem[0] = 0xED;
    mem[1] = 0xB8;
    mem[2] = 0x76;

    run(&mut z80, &mut mem, 12000);
    assert_eq!(&mem[0x9000..0x9002], &[0xDE, 0xAD]);
    assert_eq!(z80.regs.bc, 0x0000);
    assert_eq!(z80.regs.hl, 0x7FFF);
    assert_eq!(z80.regs.de, 0x8FFF);
    assert!(!z80.regs.flag(FLAG_PV));
}

#[test]
fn di_ei() {
    let mut z80 = Z80::new();
    let mut mem = [0u8; 65536];

    // EI
    mem[0] = 0xFB;
    // DI
    mem[1] = 0xF3;
    // HALT
    mem[2] = 0x76;

    run(&mut z80, &mut mem, 2000);
    assert!(!z80.regs.iff1);
    assert!(!z80.regs.iff2);
}

#[test]
fn ex_af_and_exx_swap_primary_and_alternate_registers() {
    let mut z80 = Z80::new();
    let mut mem = [0u8; 65536];

    z80.regs.af = 0x1234;
    z80.regs.af_alt = 0xABCD;
    z80.regs.bc = 0x1111;
    z80.regs.de = 0x2222;
    z80.regs.hl = 0x3333;
    z80.regs.bc_alt = 0xAAAA;
    z80.regs.de_alt = 0xBBBB;
    z80.regs.hl_alt = 0xCCCC;

    // EX AF, AF'
    mem[0] = 0x08;
    // EXX
    mem[1] = 0xD9;
    // HALT
    mem[2] = 0x76;

    run(&mut z80, &mut mem, 2000);
    assert_eq!(z80.regs.af, 0xABCD);
    assert_eq!(z80.regs.af_alt, 0x1234);
    assert_eq!(z80.regs.bc, 0xAAAA);
    assert_eq!(z80.regs.de, 0xBBBB);
    assert_eq!(z80.regs.hl, 0xCCCC);
    assert_eq!(z80.regs.bc_alt, 0x1111);
    assert_eq!(z80.regs.de_alt, 0x2222);
    assert_eq!(z80.regs.hl_alt, 0x3333);
}

#[test]
fn ex_de_hl() {
    let mut z80 = Z80::new();
    let mut mem = [0u8; 65536];

    // LD DE, 0x1234
    mem[0] = 0x11;
    mem[1] = 0x34;
    mem[2] = 0x12;
    // LD HL, 0x5678
    mem[3] = 0x21;
    mem[4] = 0x78;
    mem[5] = 0x56;
    // EX DE, HL
    mem[6] = 0xEB;
    // HALT
    mem[7] = 0x76;

    run(&mut z80, &mut mem, 2000);
    assert_eq!(z80.regs.de, 0x5678);
    assert_eq!(z80.regs.hl, 0x1234);
}

#[test]
fn in_a_n_reads_port_and_updates_wz() {
    let mut z80 = Z80::new();
    let mut mem = [0u8; 65536];

    // LD A, 0x12
    mem[0] = 0x3E;
    mem[1] = 0x12;
    // IN A, (0x34)
    mem[2] = 0xDB;
    mem[3] = 0x34;
    // HALT
    mem[4] = 0x76;

    run(&mut z80, &mut mem, 3000);
    assert_eq!(z80.regs.a(), 0x12);
    assert_eq!(z80.regs.wz, 0x1235);
}

#[test]
fn out_n_a_drives_io_bus_and_updates_wz() {
    let mut z80 = Z80::new();
    let mut mem = [0u8; 65536];

    // LD A, 0x56
    mem[0] = 0x3E;
    mem[1] = 0x56;
    // OUT (0x34), A
    mem[2] = 0xD3;
    mem[3] = 0x34;
    // HALT
    mem[4] = 0x76;

    let writes = run_with_io_trace(&mut z80, &mut mem, 3000);
    assert_eq!(
        writes,
        vec![IoWrite {
            addr: 0x5634,
            data: 0x56,
        }]
    );
    assert_eq!(z80.regs.wz, 0x5635);
}

#[test]
fn dd_ld_ix_nn() {
    let mut z80 = Z80::new();
    let mut mem = [0u8; 65536];

    // LD IX, 0xBEEF (DD 21 EF BE)
    mem[0] = 0xDD;
    mem[1] = 0x21;
    mem[2] = 0xEF;
    mem[3] = 0xBE;
    // HALT
    mem[4] = 0x76;

    run(&mut z80, &mut mem, 2000);
    assert_eq!(z80.regs.ix, 0xBEEF);
}

#[test]
fn dd_ld_r_ixd() {
    let mut z80 = Z80::new();
    let mut mem = [0u8; 65536];

    // LD IX, 0x8000
    mem[0] = 0xDD;
    mem[1] = 0x21;
    mem[2] = 0x00;
    mem[3] = 0x80;
    // LD A, (IX+5) (DD 7E 05)
    mem[4] = 0xDD;
    mem[5] = 0x7E;
    mem[6] = 0x05;
    // HALT
    mem[7] = 0x76;
    // Data at 0x8005
    mem[0x8005] = 0x42;

    run(&mut z80, &mut mem, 5000);
    assert_eq!(z80.regs.a(), 0x42);
}

#[test]
fn dd_ld_ixd_r() {
    let mut z80 = Z80::new();
    let mut mem = [0u8; 65536];

    // LD IX, 0x9000
    mem[0] = 0xDD;
    mem[1] = 0x21;
    mem[2] = 0x00;
    mem[3] = 0x90;
    // LD A, 0x77
    mem[4] = 0x3E;
    mem[5] = 0x77;
    // LD (IX+3), A (DD 77 03)
    mem[6] = 0xDD;
    mem[7] = 0x77;
    mem[8] = 0x03;
    // HALT
    mem[9] = 0x76;

    run(&mut z80, &mut mem, 5000);
    assert_eq!(mem[0x9003], 0x77);
}

#[test]
fn fd_iy_operations() {
    let mut z80 = Z80::new();
    let mut mem = [0u8; 65536];

    // LD IY, 0xA000 (FD 21 00 A0)
    mem[0] = 0xFD;
    mem[1] = 0x21;
    mem[2] = 0x00;
    mem[3] = 0xA0;
    // LD (IY+2), 0x55 (FD 36 02 55)
    mem[4] = 0xFD;
    mem[5] = 0x36;
    mem[6] = 0x02;
    mem[7] = 0x55;
    // LD A, (IY+2) (FD 7E 02)
    mem[8] = 0xFD;
    mem[9] = 0x7E;
    mem[10] = 0x02;
    // HALT
    mem[11] = 0x76;

    run(&mut z80, &mut mem, 8000);
    assert_eq!(mem[0xA002], 0x55);
    assert_eq!(z80.regs.a(), 0x55);
}

#[test]
fn ddcb_set_bit() {
    let mut z80 = Z80::new();
    let mut mem = [0u8; 65536];

    // LD IX, 0x8000
    mem[0] = 0xDD;
    mem[1] = 0x21;
    mem[2] = 0x00;
    mem[3] = 0x80;
    // Data at 0x8003
    mem[0x8003] = 0x00;
    // SET 5, (IX+3) (DD CB 03 EE)
    mem[4] = 0xDD;
    mem[5] = 0xCB;
    mem[6] = 0x03; // displacement
    mem[7] = 0xEE; // SET 5, (IX+d)  — 11 101 110
    // HALT
    mem[8] = 0x76;

    run(&mut z80, &mut mem, 8000);
    assert_eq!(mem[0x8003], 0x20); // bit 5 set
}

#[test]
fn push_pop_ix() {
    let mut z80 = Z80::new();
    let mut mem = [0u8; 65536];
    z80.regs.sp = 0xFFFE;

    // LD IX, 0x1234 (DD 21 34 12)
    mem[0] = 0xDD;
    mem[1] = 0x21;
    mem[2] = 0x34;
    mem[3] = 0x12;
    // PUSH IX (DD E5)
    mem[4] = 0xDD;
    mem[5] = 0xE5;
    // LD IX, 0x0000
    mem[6] = 0xDD;
    mem[7] = 0x21;
    mem[8] = 0x00;
    mem[9] = 0x00;
    // POP IY (FD E1)
    mem[10] = 0xFD;
    mem[11] = 0xE1;
    // HALT
    mem[12] = 0x76;

    run(&mut z80, &mut mem, 8000);
    assert_eq!(z80.regs.iy, 0x1234); // IX value pushed, popped into IY
}

#[test]
fn cpi_block_compare() {
    let mut z80 = Z80::new();
    let mut mem = [0u8; 65536];

    // Search for 0x42 in memory at 0x8000
    // LD HL, 0x8000
    mem[0] = 0x21;
    mem[1] = 0x00;
    mem[2] = 0x80;
    // LD BC, 0x0003
    mem[3] = 0x01;
    mem[4] = 0x03;
    mem[5] = 0x00;
    // LD A, 0x42
    mem[6] = 0x3E;
    mem[7] = 0x42;
    // Data
    mem[0x8000] = 0x11;
    mem[0x8001] = 0x42; // match
    mem[0x8002] = 0x33;
    // CPIR (ED B1)
    mem[8] = 0xED;
    mem[9] = 0xB1;
    // HALT
    mem[10] = 0x76;

    run(&mut z80, &mut mem, 10000);
    assert_eq!(z80.regs.hl, 0x8002); // HL points past the match
    assert_eq!(z80.regs.bc, 0x0001); // BC decremented twice (checked 2 bytes)
    assert!(z80.regs.flag(FLAG_Z)); // Match found (A == data)
}

#[test]
fn cpd_block_compare_moves_backward_once() {
    let mut z80 = Z80::new();
    let mut mem = [0u8; 65536];

    z80.regs.hl = 0x8001;
    z80.regs.bc = 0x0001;
    z80.regs.set_a(0x42);
    mem[0x8001] = 0x33;

    // CPD (ED A9)
    mem[0] = 0xED;
    mem[1] = 0xA9;
    mem[2] = 0x76;

    run(&mut z80, &mut mem, 5000);
    assert_eq!(z80.regs.hl, 0x8000);
    assert_eq!(z80.regs.bc, 0x0000);
    assert!(z80.regs.flag(FLAG_N));
    assert!(!z80.regs.flag(FLAG_Z));
    assert!(!z80.regs.flag(FLAG_PV));
}

#[test]
fn cpdr_block_compare_repeats_backward_until_match() {
    let mut z80 = Z80::new();
    let mut mem = [0u8; 65536];

    z80.regs.hl = 0x8002;
    z80.regs.bc = 0x0003;
    z80.regs.set_a(0x42);
    mem[0x8000] = 0x11;
    mem[0x8001] = 0x42;
    mem[0x8002] = 0x33;

    // CPDR (ED B9)
    mem[0] = 0xED;
    mem[1] = 0xB9;
    // HALT
    mem[2] = 0x76;

    run(&mut z80, &mut mem, 10000);
    assert_eq!(z80.regs.hl, 0x8000);
    assert_eq!(z80.regs.bc, 0x0001);
    assert!(z80.regs.flag(FLAG_Z));
}

#[test]
fn outi_block_output() {
    let mut z80 = Z80::new();
    let mut mem = [0u8; 65536];

    // Output one byte from (HL) to port (C)
    // LD HL, 0x8000
    mem[0] = 0x21;
    mem[1] = 0x00;
    mem[2] = 0x80;
    // LD BC, 0x01FE (B=1 count, C=0xFE port)
    mem[3] = 0x01;
    mem[4] = 0xFE;
    mem[5] = 0x01;
    // Data at 0x8000
    mem[0x8000] = 0xAA;
    // OUTI (ED A3)
    mem[6] = 0xED;
    mem[7] = 0xA3;
    // HALT
    mem[8] = 0x76;

    run(&mut z80, &mut mem, 5000);
    assert_eq!(z80.regs.b(), 0x00); // B decremented from 1 to 0
    assert_eq!(z80.regs.hl, 0x8001); // HL incremented
    assert!(z80.regs.flag(FLAG_Z)); // B == 0 → Z set
}

#[test]
fn otir_block_output_repeats_forward_until_b_zero() {
    let mut z80 = Z80::new();
    let mut mem = [0u8; 65536];

    z80.regs.hl = 0x8000;
    z80.regs.bc = 0x02FE;
    mem[0x8000] = 0xAA;
    mem[0x8001] = 0x55;

    // OTIR (ED B3)
    mem[0] = 0xED;
    mem[1] = 0xB3;
    mem[2] = 0x76;

    let writes = run_with_io_trace(&mut z80, &mut mem, 12000);
    assert_eq!(
        writes,
        vec![
            IoWrite {
                addr: 0x01FE,
                data: 0xAA,
            },
            IoWrite {
                addr: 0x00FE,
                data: 0x55,
            },
        ]
    );
    assert_eq!(z80.regs.b(), 0x00);
    assert_eq!(z80.regs.hl, 0x8002);
    assert!(z80.regs.flag(FLAG_Z));
}

#[test]
fn otdr_block_output_repeats_backward_until_b_zero() {
    let mut z80 = Z80::new();
    let mut mem = [0u8; 65536];

    z80.regs.hl = 0x8001;
    z80.regs.bc = 0x02FE;
    mem[0x8000] = 0x55;
    mem[0x8001] = 0xAA;

    // OTDR (ED BB)
    mem[0] = 0xED;
    mem[1] = 0xBB;
    // HALT
    mem[2] = 0x76;

    let writes = run_with_io_trace(&mut z80, &mut mem, 12000);
    assert_eq!(
        writes,
        vec![
            IoWrite {
                addr: 0x01FE,
                data: 0xAA,
            },
            IoWrite {
                addr: 0x00FE,
                data: 0x55,
            },
        ]
    );
    assert_eq!(z80.regs.b(), 0x00);
    assert_eq!(z80.regs.hl, 0x7FFF);
    assert!(z80.regs.flag(FLAG_Z));
}

#[test]
fn ini_block_input() {
    let mut z80 = Z80::new();
    let mut mem = [0u8; 65536];

    // Input one byte from port to (HL)
    // LD HL, 0x9000
    mem[0] = 0x21;
    mem[1] = 0x00;
    mem[2] = 0x90;
    // LD BC, 0x01AB (B=1 count, C=0xAB port)
    mem[3] = 0x01;
    mem[4] = 0xAB;
    mem[5] = 0x01;
    // INI (ED A2) — reads from port 0x00AB (B decremented to 0 first, so port = 0x00AB)
    mem[6] = 0xED;
    mem[7] = 0xA2;
    // HALT
    mem[8] = 0x76;

    run(&mut z80, &mut mem, 5000);
    assert_eq!(z80.regs.b(), 0x00); // B decremented
    assert_eq!(z80.regs.hl, 0x9001); // HL incremented
    // INI reads from ORIGINAL BC (before B decrement) = 0x01AB
    // Test harness returns port>>8 = 0x01
    assert_eq!(mem[0x9000], 0x01); // Byte written from port read
    assert!(z80.regs.flag(FLAG_Z)); // B == 0 → Z set
}

#[test]
fn inir_block_input_repeats_forward_until_b_zero() {
    let mut z80 = Z80::new();
    let mut mem = [0u8; 65536];

    z80.regs.hl = 0x9000;
    z80.regs.bc = 0x02AB;

    // INIR (ED B2)
    mem[0] = 0xED;
    mem[1] = 0xB2;
    mem[2] = 0x76;

    run(&mut z80, &mut mem, 12000);
    assert_eq!(mem[0x9000], 0x02);
    assert_eq!(mem[0x9001], 0x01);
    assert_eq!(z80.regs.b(), 0x00);
    assert_eq!(z80.regs.hl, 0x9002);
    assert!(z80.regs.flag(FLAG_Z));
}

#[test]
fn ind_block_input_moves_backward() {
    let mut z80 = Z80::new();
    let mut mem = [0u8; 65536];

    z80.regs.hl = 0x9001;
    z80.regs.bc = 0x01AB;

    // IND (ED AA)
    mem[0] = 0xED;
    mem[1] = 0xAA;
    // HALT
    mem[2] = 0x76;

    run(&mut z80, &mut mem, 5000);
    assert_eq!(z80.regs.b(), 0x00);
    assert_eq!(z80.regs.hl, 0x9000);
    assert_eq!(mem[0x9001], 0x01);
    assert!(z80.regs.flag(FLAG_Z));
}

#[test]
fn ldir_block_transfer_multiple() {
    let mut z80 = Z80::new();
    let mut mem = [0u8; 65536];

    // Copy 4 bytes from 0x8000 to 0x9000
    mem[0] = 0x21;
    mem[1] = 0x00;
    mem[2] = 0x80; // LD HL, 0x8000
    mem[3] = 0x11;
    mem[4] = 0x00;
    mem[5] = 0x90; // LD DE, 0x9000
    mem[6] = 0x01;
    mem[7] = 0x04;
    mem[8] = 0x00; // LD BC, 4
    mem[0x8000] = 0xDE;
    mem[0x8001] = 0xAD;
    mem[0x8002] = 0xBE;
    mem[0x8003] = 0xEF;
    // LDIR (ED B0)
    mem[9] = 0xED;
    mem[10] = 0xB0;
    // HALT
    mem[11] = 0x76;

    run(&mut z80, &mut mem, 20000);
    assert_eq!(&mem[0x9000..0x9004], &[0xDE, 0xAD, 0xBE, 0xEF]);
    assert_eq!(z80.regs.bc, 0x0000);
    assert_eq!(z80.regs.hl, 0x8004);
    assert_eq!(z80.regs.de, 0x9004);
    assert!(!z80.regs.flag(FLAG_PV)); // BC=0 → PV clear
}

#[test]
fn ldir_flags_bits35() {
    // Reproduce Tom Harte ED B0 test 1 failure
    let mut z80 = Z80::new();
    let mut mem = [0u8; 65536];

    z80.regs.af = 0xDE_DC; // A=0xDE, F=0xDC
    z80.regs.hl = 0x5D41;
    z80.regs.de = 0x5780;
    z80.regs.bc = 0xD0E4; // >1 so LDIR repeats
    z80.regs.sp = 0xFFFF;
    z80.regs.pc = 0x0000;

    mem[0] = 0xED;
    mem[1] = 0xB0; // LDIR
    mem[0x5D41] = 0x2E; // Source byte

    // LDIR = 21T = 42 HC per repeat iteration
    // Instrument: track what address the Z80 reads from during the LDI
    let mut reads = Vec::new();
    let mut hc = 0u32;
    while hc < 42 {
        z80.tick();
        if z80.mreq && z80.rd {
            z80.data_in = mem[z80.addr as usize];
            reads.push((hc, z80.addr, z80.data_in));
        } else if z80.mreq && z80.wr {
            mem[z80.addr as usize] = z80.data;
        }
        hc += 1;
        if z80.halt {
            break;
        }
    }
    eprintln!("Memory reads:");
    for (h, addr, data) in &reads {
        eprintln!("  hc={}: [{:#06X}] = {:#04X}", h, addr, data);
    }

    // After one iteration:
    assert_eq!(mem[0x5780], 0x2E, "byte should be copied to DE");
    assert_eq!(z80.regs.hl, 0x5D42, "HL incremented once");
    assert_eq!(z80.regs.de, 0x5781, "DE incremented once");
    assert_eq!(z80.regs.bc, 0xD0E3, "BC decremented once");

    // Check flags: n = A + byte = 0xDE + 0x2E = 0x0C
    // Flag 5 = bit 1 of 0x0C = 0
    // Flag 3 = bit 3 of 0x0C = 1
    let f = z80.regs.f();
    eprintln!("F = {:#04X}", f);
    eprintln!("Expected from Tom Harte: 0xC4");
    // Tom Harte expects 0xC4 = S=1, Z=1, 5=0, H=0, 3=0, PV=1, N=0, C=0
    // Our formula gives 0xCC = S=1, Z=1, 5=0, H=0, 3=1, PV=1, N=0, C=0
}

#[test]
fn interrupt_im1() {
    let mut z80 = Z80::new();
    let mut mem = [0u8; 65536];
    z80.regs.sp = 0xFFFE;

    // EI
    mem[0] = 0xFB;
    // NOP (EI defers interrupt by one instruction)
    mem[1] = 0x00;
    // HALT (will be interrupted)
    mem[2] = 0x76;

    // ISR at 0x0038 (IM 1 vector): LD A, 0xAA; HALT
    mem[0x38] = 0x3E;
    mem[0x39] = 0xAA;
    mem[0x3A] = 0x76;

    // Run a few instructions to get past EI + NOP
    // EI = 8 HC, NOP = 8 HC, HALT starts at HC 16
    let mut hc = 0u32;
    while hc < 40 {
        z80.tick();
        if z80.mreq && z80.rd {
            z80.data_in = mem[z80.addr as usize];
        }
        if z80.mreq && z80.wr {
            mem[z80.addr as usize] = z80.data;
        }
        hc += 1;
    }
    // CPU should be halted now
    assert!(z80.halt);

    // Assert IRQ
    z80.irq = true;

    // Run more to let the interrupt fire
    let mut hc2 = 0u32;
    while hc2 < 200 {
        z80.tick();
        if z80.mreq && z80.rd {
            z80.data_in = mem[z80.addr as usize];
        }
        if z80.mreq && z80.wr {
            mem[z80.addr as usize] = z80.data;
        }
        if z80.iorq && z80.m1 {
            z80.data_in = 0xFF;
        } // IntAck data
        hc2 += 1;
        if z80.halt && z80.regs.pc == 0x3A {
            break;
        } // Halted in ISR
    }

    assert_eq!(z80.regs.a(), 0xAA); // ISR executed
    assert!(!z80.regs.iff1); // Interrupts disabled by INT
}

#[test]
fn nmi_jumps_to_0066() {
    let mut z80 = Z80::new();
    let mut mem = [0u8; 65536];
    z80.regs.sp = 0xFFFE;

    // NOP; NOP; HALT
    mem[0] = 0x00;
    mem[1] = 0x00;
    mem[2] = 0x76;

    // NMI handler at 0x0066: LD A, 0x66; HALT
    mem[0x66] = 0x3E;
    mem[0x67] = 0x66;
    mem[0x68] = 0x76;

    // Run the first NOP
    let mut hc = 0u32;
    while hc < 10 {
        z80.tick();
        if z80.mreq && z80.rd {
            z80.data_in = mem[z80.addr as usize];
        }
        if z80.mreq && z80.wr {
            mem[z80.addr as usize] = z80.data;
        }
        hc += 1;
    }

    // Trigger NMI (edge-triggered)
    z80.nmi = true;

    // Run until halted in NMI handler
    let mut hc2 = 0u32;
    while hc2 < 300 {
        z80.tick();
        if z80.mreq && z80.rd {
            z80.data_in = mem[z80.addr as usize];
        }
        if z80.mreq && z80.wr {
            mem[z80.addr as usize] = z80.data;
        }
        hc2 += 1;
        if z80.halt {
            break;
        }
    }

    assert_eq!(z80.regs.a(), 0x66); // NMI handler executed
}
