//! 6502 CPU implementation.
//!
//! Cycle-accurate emulation where each `tick()` performs exactly one
//! bus access. Instructions are broken down into their component cycles.

use emu_core::{Bus, Cpu, Observable, Value};

use crate::flags::{C, D, I, N, V, Z};
use crate::{Registers, Status};

/// Internal state tracking instruction execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    /// Fetching opcode byte.
    FetchOpcode,
    /// Executing instruction cycles.
    Execute,
    /// CPU is stopped (JAM/KIL instruction).
    Stopped,
}

/// The MOS 6502 CPU.
///
/// Implements cycle-accurate execution where each `tick()` advances
/// exactly one CPU cycle. The 6502 performs one bus access per cycle.
#[derive(Debug)]
pub struct Mos6502 {
    /// CPU registers.
    pub regs: Registers,

    /// Current execution state.
    state: State,

    /// Current opcode being executed.
    opcode: u8,

    /// Current cycle within the instruction (0 = opcode fetch).
    cycle: u8,

    /// Temporary address register for addressing modes.
    addr: u16,

    /// Temporary data register.
    data: u8,

    /// Pointer for indirect addressing.
    pointer: u8,

    /// NMI edge detector - true when NMI line went low.
    nmi_pending: bool,

    /// IRQ level - true when IRQ line is low.
    irq_pending: bool,

    /// Total cycles executed (for debugging).
    total_cycles: u64,
}

impl Default for Mos6502 {
    fn default() -> Self {
        Self::new()
    }
}

impl Mos6502 {
    /// Create a new 6502 in reset state.
    #[must_use]
    pub fn new() -> Self {
        Self {
            regs: Registers::new(),
            state: State::FetchOpcode,
            opcode: 0,
            cycle: 0,
            addr: 0,
            data: 0,
            pointer: 0,
            nmi_pending: false,
            irq_pending: false,
            total_cycles: 0,
        }
    }

    /// Execute one CPU cycle.
    fn execute_cycle<B: Bus>(&mut self, bus: &mut B) {
        self.total_cycles += 1;

        match self.state {
            State::FetchOpcode => {
                // Check for interrupts before fetching next opcode
                if self.nmi_pending {
                    self.nmi_pending = false;
                    self.begin_nmi(bus);
                    return;
                }
                if self.irq_pending && !self.regs.p.is_set(I) {
                    self.begin_irq(bus);
                    return;
                }

                // Fetch opcode
                self.opcode = bus.read(self.regs.pc).data;
                self.regs.pc = self.regs.pc.wrapping_add(1);
                self.cycle = 1;
                self.state = State::Execute;
            }
            State::Execute => {
                self.execute_instruction(bus);
            }
            State::Stopped => {
                // JAM/KIL - CPU is locked up, just read current PC
                let _ = bus.read(self.regs.pc);
            }
        }
    }

    /// Begin NMI sequence.
    fn begin_nmi<B: Bus>(&mut self, bus: &mut B) {
        // NMI uses the same sequence as BRK but reads from $FFFA
        // Cycle 1: read next instruction byte (discarded)
        let _ = bus.read(self.regs.pc);
        self.opcode = 0x00; // Treat as BRK for cycle counting
        self.cycle = 2;
        self.addr = 0xFFFA; // NMI vector
        self.state = State::Execute;
    }

    /// Begin IRQ sequence.
    fn begin_irq<B: Bus>(&mut self, bus: &mut B) {
        // IRQ uses the same sequence as BRK but reads from $FFFE
        // Cycle 1: read next instruction byte (discarded)
        let _ = bus.read(self.regs.pc);
        self.opcode = 0x00; // Treat as BRK for cycle counting
        self.cycle = 2;
        self.addr = 0xFFFE; // IRQ vector (same as BRK)
        self.state = State::Execute;
    }

    /// Execute one cycle of the current instruction.
    fn execute_instruction<B: Bus>(&mut self, bus: &mut B) {
        match self.opcode {
            // BRK - 7 cycles
            0x00 => self.op_brk(bus),

            // ORA (zp,X) - 6 cycles
            0x01 => self.addr_izx(bus, Self::do_ora),

            // ORA zp - 3 cycles
            0x05 => self.addr_zp(bus, Self::do_ora),

            // ASL zp - 5 cycles
            0x06 => self.addr_zp_rmw(bus, Self::do_asl),

            // PHP - 3 cycles
            0x08 => self.op_php(bus),

            // ORA imm - 2 cycles
            0x09 => self.addr_imm(bus, Self::do_ora),

            // ASL A - 2 cycles
            0x0A => self.op_asl_a(bus),

            // ORA abs - 4 cycles
            0x0D => self.addr_abs(bus, Self::do_ora),

            // ASL abs - 6 cycles
            0x0E => self.addr_abs_rmw(bus, Self::do_asl),

            // BPL rel - 2/3/4 cycles
            0x10 => self.op_branch(bus, !self.regs.p.is_set(N)),

            // ORA (zp),Y - 5/6 cycles
            0x11 => self.addr_izy(bus, Self::do_ora),

            // ORA zp,X - 4 cycles
            0x15 => self.addr_zpx(bus, Self::do_ora),

            // ASL zp,X - 6 cycles
            0x16 => self.addr_zpx_rmw(bus, Self::do_asl),

            // CLC - 2 cycles
            0x18 => self.op_flag(bus, C, false),

            // ORA abs,Y - 4/5 cycles
            0x19 => self.addr_aby(bus, Self::do_ora),

            // ORA abs,X - 4/5 cycles
            0x1D => self.addr_abx(bus, Self::do_ora),

            // ASL abs,X - 7 cycles
            0x1E => self.addr_abx_rmw(bus, Self::do_asl),

            // JSR abs - 6 cycles
            0x20 => self.op_jsr(bus),

            // AND (zp,X) - 6 cycles
            0x21 => self.addr_izx(bus, Self::do_and),

            // BIT zp - 3 cycles
            0x24 => self.addr_zp(bus, Self::do_bit),

            // AND zp - 3 cycles
            0x25 => self.addr_zp(bus, Self::do_and),

            // ROL zp - 5 cycles
            0x26 => self.addr_zp_rmw(bus, Self::do_rol),

            // PLP - 4 cycles
            0x28 => self.op_plp(bus),

            // AND imm - 2 cycles
            0x29 => self.addr_imm(bus, Self::do_and),

            // ROL A - 2 cycles
            0x2A => self.op_rol_a(bus),

            // BIT abs - 4 cycles
            0x2C => self.addr_abs(bus, Self::do_bit),

            // AND abs - 4 cycles
            0x2D => self.addr_abs(bus, Self::do_and),

            // ROL abs - 6 cycles
            0x2E => self.addr_abs_rmw(bus, Self::do_rol),

            // BMI rel - 2/3/4 cycles
            0x30 => self.op_branch(bus, self.regs.p.is_set(N)),

            // AND (zp),Y - 5/6 cycles
            0x31 => self.addr_izy(bus, Self::do_and),

            // AND zp,X - 4 cycles
            0x35 => self.addr_zpx(bus, Self::do_and),

            // ROL zp,X - 6 cycles
            0x36 => self.addr_zpx_rmw(bus, Self::do_rol),

            // SEC - 2 cycles
            0x38 => self.op_flag(bus, C, true),

            // AND abs,Y - 4/5 cycles
            0x39 => self.addr_aby(bus, Self::do_and),

            // AND abs,X - 4/5 cycles
            0x3D => self.addr_abx(bus, Self::do_and),

            // ROL abs,X - 7 cycles
            0x3E => self.addr_abx_rmw(bus, Self::do_rol),

            // RTI - 6 cycles
            0x40 => self.op_rti(bus),

            // EOR (zp,X) - 6 cycles
            0x41 => self.addr_izx(bus, Self::do_eor),

            // EOR zp - 3 cycles
            0x45 => self.addr_zp(bus, Self::do_eor),

            // LSR zp - 5 cycles
            0x46 => self.addr_zp_rmw(bus, Self::do_lsr),

            // PHA - 3 cycles
            0x48 => self.op_pha(bus),

            // EOR imm - 2 cycles
            0x49 => self.addr_imm(bus, Self::do_eor),

            // LSR A - 2 cycles
            0x4A => self.op_lsr_a(bus),

            // JMP abs - 3 cycles
            0x4C => self.op_jmp_abs(bus),

            // EOR abs - 4 cycles
            0x4D => self.addr_abs(bus, Self::do_eor),

            // LSR abs - 6 cycles
            0x4E => self.addr_abs_rmw(bus, Self::do_lsr),

            // BVC rel - 2/3/4 cycles
            0x50 => self.op_branch(bus, !self.regs.p.is_set(V)),

            // EOR (zp),Y - 5/6 cycles
            0x51 => self.addr_izy(bus, Self::do_eor),

            // EOR zp,X - 4 cycles
            0x55 => self.addr_zpx(bus, Self::do_eor),

            // LSR zp,X - 6 cycles
            0x56 => self.addr_zpx_rmw(bus, Self::do_lsr),

            // CLI - 2 cycles
            0x58 => self.op_flag(bus, I, false),

            // EOR abs,Y - 4/5 cycles
            0x59 => self.addr_aby(bus, Self::do_eor),

            // EOR abs,X - 4/5 cycles
            0x5D => self.addr_abx(bus, Self::do_eor),

            // LSR abs,X - 7 cycles
            0x5E => self.addr_abx_rmw(bus, Self::do_lsr),

            // RTS - 6 cycles
            0x60 => self.op_rts(bus),

            // ADC (zp,X) - 6 cycles
            0x61 => self.addr_izx(bus, Self::do_adc),

            // ADC zp - 3 cycles
            0x65 => self.addr_zp(bus, Self::do_adc),

            // ROR zp - 5 cycles
            0x66 => self.addr_zp_rmw(bus, Self::do_ror),

            // PLA - 4 cycles
            0x68 => self.op_pla(bus),

            // ADC imm - 2 cycles
            0x69 => self.addr_imm(bus, Self::do_adc),

            // ROR A - 2 cycles
            0x6A => self.op_ror_a(bus),

            // JMP (ind) - 5 cycles
            0x6C => self.op_jmp_ind(bus),

            // ADC abs - 4 cycles
            0x6D => self.addr_abs(bus, Self::do_adc),

            // ROR abs - 6 cycles
            0x6E => self.addr_abs_rmw(bus, Self::do_ror),

            // BVS rel - 2/3/4 cycles
            0x70 => self.op_branch(bus, self.regs.p.is_set(V)),

            // ADC (zp),Y - 5/6 cycles
            0x71 => self.addr_izy(bus, Self::do_adc),

            // ADC zp,X - 4 cycles
            0x75 => self.addr_zpx(bus, Self::do_adc),

            // ROR zp,X - 6 cycles
            0x76 => self.addr_zpx_rmw(bus, Self::do_ror),

            // SEI - 2 cycles
            0x78 => self.op_flag(bus, I, true),

            // ADC abs,Y - 4/5 cycles
            0x79 => self.addr_aby(bus, Self::do_adc),

            // ADC abs,X - 4/5 cycles
            0x7D => self.addr_abx(bus, Self::do_adc),

            // ROR abs,X - 7 cycles
            0x7E => self.addr_abx_rmw(bus, Self::do_ror),

            // STA (zp,X) - 6 cycles
            0x81 => self.addr_izx_w(bus, |cpu| cpu.regs.a),

            // STY zp - 3 cycles
            0x84 => self.addr_zp_w(bus, |cpu| cpu.regs.y),

            // STA zp - 3 cycles
            0x85 => self.addr_zp_w(bus, |cpu| cpu.regs.a),

            // STX zp - 3 cycles
            0x86 => self.addr_zp_w(bus, |cpu| cpu.regs.x),

            // DEY - 2 cycles
            0x88 => self.op_dey(bus),

            // TXA - 2 cycles
            0x8A => self.op_txa(bus),

            // STY abs - 4 cycles
            0x8C => self.addr_abs_w(bus, |cpu| cpu.regs.y),

            // STA abs - 4 cycles
            0x8D => self.addr_abs_w(bus, |cpu| cpu.regs.a),

            // STX abs - 4 cycles
            0x8E => self.addr_abs_w(bus, |cpu| cpu.regs.x),

            // BCC rel - 2/3/4 cycles
            0x90 => self.op_branch(bus, !self.regs.p.is_set(C)),

            // STA (zp),Y - 6 cycles
            0x91 => self.addr_izy_w(bus, |cpu| cpu.regs.a),

            // STY zp,X - 4 cycles
            0x94 => self.addr_zpx_w(bus, |cpu| cpu.regs.y),

            // STA zp,X - 4 cycles
            0x95 => self.addr_zpx_w(bus, |cpu| cpu.regs.a),

            // STX zp,Y - 4 cycles
            0x96 => self.addr_zpy_w(bus, |cpu| cpu.regs.x),

            // TYA - 2 cycles
            0x98 => self.op_tya(bus),

            // STA abs,Y - 5 cycles
            0x99 => self.addr_aby_w(bus, |cpu| cpu.regs.a),

            // TXS - 2 cycles
            0x9A => self.op_txs(bus),

            // STA abs,X - 5 cycles
            0x9D => self.addr_abx_w(bus, |cpu| cpu.regs.a),

            // LDY imm - 2 cycles
            0xA0 => self.addr_imm(bus, Self::do_ldy),

            // LDA (zp,X) - 6 cycles
            0xA1 => self.addr_izx(bus, Self::do_lda),

            // LDX imm - 2 cycles
            0xA2 => self.addr_imm(bus, Self::do_ldx),

            // LDY zp - 3 cycles
            0xA4 => self.addr_zp(bus, Self::do_ldy),

            // LDA zp - 3 cycles
            0xA5 => self.addr_zp(bus, Self::do_lda),

            // LDX zp - 3 cycles
            0xA6 => self.addr_zp(bus, Self::do_ldx),

            // TAY - 2 cycles
            0xA8 => self.op_tay(bus),

            // LDA imm - 2 cycles
            0xA9 => self.addr_imm(bus, Self::do_lda),

            // TAX - 2 cycles
            0xAA => self.op_tax(bus),

            // LDY abs - 4 cycles
            0xAC => self.addr_abs(bus, Self::do_ldy),

            // LDA abs - 4 cycles
            0xAD => self.addr_abs(bus, Self::do_lda),

            // LDX abs - 4 cycles
            0xAE => self.addr_abs(bus, Self::do_ldx),

            // BCS rel - 2/3/4 cycles
            0xB0 => self.op_branch(bus, self.regs.p.is_set(C)),

            // LDA (zp),Y - 5/6 cycles
            0xB1 => self.addr_izy(bus, Self::do_lda),

            // LDY zp,X - 4 cycles
            0xB4 => self.addr_zpx(bus, Self::do_ldy),

            // LDA zp,X - 4 cycles
            0xB5 => self.addr_zpx(bus, Self::do_lda),

            // LDX zp,Y - 4 cycles
            0xB6 => self.addr_zpy(bus, Self::do_ldx),

            // CLV - 2 cycles
            0xB8 => self.op_flag(bus, V, false),

            // LDA abs,Y - 4/5 cycles
            0xB9 => self.addr_aby(bus, Self::do_lda),

            // TSX - 2 cycles
            0xBA => self.op_tsx(bus),

            // LDY abs,X - 4/5 cycles
            0xBC => self.addr_abx(bus, Self::do_ldy),

            // LDA abs,X - 4/5 cycles
            0xBD => self.addr_abx(bus, Self::do_lda),

            // LDX abs,Y - 4/5 cycles
            0xBE => self.addr_aby(bus, Self::do_ldx),

            // CPY imm - 2 cycles
            0xC0 => self.addr_imm(bus, Self::do_cpy),

            // CMP (zp,X) - 6 cycles
            0xC1 => self.addr_izx(bus, Self::do_cmp),

            // CPY zp - 3 cycles
            0xC4 => self.addr_zp(bus, Self::do_cpy),

            // CMP zp - 3 cycles
            0xC5 => self.addr_zp(bus, Self::do_cmp),

            // DEC zp - 5 cycles
            0xC6 => self.addr_zp_rmw(bus, Self::do_dec),

            // INY - 2 cycles
            0xC8 => self.op_iny(bus),

            // CMP imm - 2 cycles
            0xC9 => self.addr_imm(bus, Self::do_cmp),

            // DEX - 2 cycles
            0xCA => self.op_dex(bus),

            // CPY abs - 4 cycles
            0xCC => self.addr_abs(bus, Self::do_cpy),

            // CMP abs - 4 cycles
            0xCD => self.addr_abs(bus, Self::do_cmp),

            // DEC abs - 6 cycles
            0xCE => self.addr_abs_rmw(bus, Self::do_dec),

            // BNE rel - 2/3/4 cycles
            0xD0 => self.op_branch(bus, !self.regs.p.is_set(Z)),

            // CMP (zp),Y - 5/6 cycles
            0xD1 => self.addr_izy(bus, Self::do_cmp),

            // CMP zp,X - 4 cycles
            0xD5 => self.addr_zpx(bus, Self::do_cmp),

            // DEC zp,X - 6 cycles
            0xD6 => self.addr_zpx_rmw(bus, Self::do_dec),

            // CLD - 2 cycles
            0xD8 => self.op_flag(bus, D, false),

            // CMP abs,Y - 4/5 cycles
            0xD9 => self.addr_aby(bus, Self::do_cmp),

            // CMP abs,X - 4/5 cycles
            0xDD => self.addr_abx(bus, Self::do_cmp),

            // DEC abs,X - 7 cycles
            0xDE => self.addr_abx_rmw(bus, Self::do_dec),

            // CPX imm - 2 cycles
            0xE0 => self.addr_imm(bus, Self::do_cpx),

            // SBC (zp,X) - 6 cycles
            0xE1 => self.addr_izx(bus, Self::do_sbc),

            // CPX zp - 3 cycles
            0xE4 => self.addr_zp(bus, Self::do_cpx),

            // SBC zp - 3 cycles
            0xE5 => self.addr_zp(bus, Self::do_sbc),

            // INC zp - 5 cycles
            0xE6 => self.addr_zp_rmw(bus, Self::do_inc),

            // INX - 2 cycles
            0xE8 => self.op_inx(bus),

            // SBC imm - 2 cycles
            0xE9 => self.addr_imm(bus, Self::do_sbc),

            // NOP - 2 cycles
            0xEA => self.op_nop(bus),

            // CPX abs - 4 cycles
            0xEC => self.addr_abs(bus, Self::do_cpx),

            // SBC abs - 4 cycles
            0xED => self.addr_abs(bus, Self::do_sbc),

            // INC abs - 6 cycles
            0xEE => self.addr_abs_rmw(bus, Self::do_inc),

            // BEQ rel - 2/3/4 cycles
            0xF0 => self.op_branch(bus, self.regs.p.is_set(Z)),

            // SBC (zp),Y - 5/6 cycles
            0xF1 => self.addr_izy(bus, Self::do_sbc),

            // SBC zp,X - 4 cycles
            0xF5 => self.addr_zpx(bus, Self::do_sbc),

            // INC zp,X - 6 cycles
            0xF6 => self.addr_zpx_rmw(bus, Self::do_inc),

            // SED - 2 cycles
            0xF8 => self.op_flag(bus, D, true),

            // SBC abs,Y - 4/5 cycles
            0xF9 => self.addr_aby(bus, Self::do_sbc),

            // SBC abs,X - 4/5 cycles
            0xFD => self.addr_abx(bus, Self::do_sbc),

            // INC abs,X - 7 cycles
            0xFE => self.addr_abx_rmw(bus, Self::do_inc),

            // Unimplemented - treat as NOP for now (will be illegal opcodes)
            _ => {
                // Single-byte NOP for unimplemented opcodes
                if self.cycle == 1 {
                    let _ = bus.read(self.regs.pc); // Dummy read
                    self.finish();
                }
            }
        }
    }

    /// Finish current instruction and return to opcode fetch.
    fn finish(&mut self) {
        self.state = State::FetchOpcode;
        self.cycle = 0;
    }

    // ========================================================================
    // Addressing mode helpers - read operations
    // ========================================================================

    /// Immediate addressing: operand is next byte.
    fn addr_imm<B: Bus>(&mut self, bus: &mut B, op: fn(&mut Self, u8)) {
        // Cycle 1: read operand
        if self.cycle == 1 {
            self.data = bus.read(self.regs.pc).data;
            self.regs.pc = self.regs.pc.wrapping_add(1);
            op(self, self.data);
            self.finish();
        }
    }

    /// Zero page addressing: operand is at zero page address.
    fn addr_zp<B: Bus>(&mut self, bus: &mut B, op: fn(&mut Self, u8)) {
        match self.cycle {
            1 => {
                // Read zero page address
                self.addr = u16::from(bus.read(self.regs.pc).data);
                self.regs.pc = self.regs.pc.wrapping_add(1);
                self.cycle = 2;
            }
            2 => {
                // Read from zero page
                self.data = bus.read(self.addr).data;
                op(self, self.data);
                self.finish();
            }
            _ => unreachable!(),
        }
    }

    /// Zero page,X addressing.
    fn addr_zpx<B: Bus>(&mut self, bus: &mut B, op: fn(&mut Self, u8)) {
        match self.cycle {
            1 => {
                // Read zero page address
                self.pointer = bus.read(self.regs.pc).data;
                self.regs.pc = self.regs.pc.wrapping_add(1);
                self.cycle = 2;
            }
            2 => {
                // Dummy read while adding X (wraps in zero page)
                let _ = bus.read(u16::from(self.pointer));
                self.addr = u16::from(self.pointer.wrapping_add(self.regs.x));
                self.cycle = 3;
            }
            3 => {
                // Read from indexed address
                self.data = bus.read(self.addr).data;
                op(self, self.data);
                self.finish();
            }
            _ => unreachable!(),
        }
    }

    /// Zero page,Y addressing.
    fn addr_zpy<B: Bus>(&mut self, bus: &mut B, op: fn(&mut Self, u8)) {
        match self.cycle {
            1 => {
                self.pointer = bus.read(self.regs.pc).data;
                self.regs.pc = self.regs.pc.wrapping_add(1);
                self.cycle = 2;
            }
            2 => {
                let _ = bus.read(u16::from(self.pointer));
                self.addr = u16::from(self.pointer.wrapping_add(self.regs.y));
                self.cycle = 3;
            }
            3 => {
                self.data = bus.read(self.addr).data;
                op(self, self.data);
                self.finish();
            }
            _ => unreachable!(),
        }
    }

    /// Absolute addressing: operand is at 16-bit address.
    fn addr_abs<B: Bus>(&mut self, bus: &mut B, op: fn(&mut Self, u8)) {
        match self.cycle {
            1 => {
                // Read low byte of address
                self.addr = u16::from(bus.read(self.regs.pc).data);
                self.regs.pc = self.regs.pc.wrapping_add(1);
                self.cycle = 2;
            }
            2 => {
                // Read high byte of address
                self.addr |= u16::from(bus.read(self.regs.pc).data) << 8;
                self.regs.pc = self.regs.pc.wrapping_add(1);
                self.cycle = 3;
            }
            3 => {
                // Read from address
                self.data = bus.read(self.addr).data;
                op(self, self.data);
                self.finish();
            }
            _ => unreachable!(),
        }
    }

    /// Absolute,X addressing with page crossing check.
    fn addr_abx<B: Bus>(&mut self, bus: &mut B, op: fn(&mut Self, u8)) {
        match self.cycle {
            1 => {
                self.addr = u16::from(bus.read(self.regs.pc).data);
                self.regs.pc = self.regs.pc.wrapping_add(1);
                self.cycle = 2;
            }
            2 => {
                let hi = bus.read(self.regs.pc).data;
                self.regs.pc = self.regs.pc.wrapping_add(1);
                let lo = (self.addr as u8).wrapping_add(self.regs.x);
                self.addr = u16::from(lo) | (u16::from(hi) << 8);
                // Check if page crossed
                self.data = if lo < self.regs.x { 1 } else { 0 };
                self.cycle = 3;
            }
            3 => {
                if self.data != 0 {
                    // Page crossed - dummy read from wrong address, then fix
                    let _ = bus.read(self.addr);
                    self.addr = self.addr.wrapping_add(0x100);
                    self.cycle = 4;
                } else {
                    // No page cross - read data
                    self.data = bus.read(self.addr).data;
                    op(self, self.data);
                    self.finish();
                }
            }
            4 => {
                // Read from correct address after page fix
                self.data = bus.read(self.addr).data;
                op(self, self.data);
                self.finish();
            }
            _ => unreachable!(),
        }
    }

    /// Absolute,Y addressing with page crossing check.
    fn addr_aby<B: Bus>(&mut self, bus: &mut B, op: fn(&mut Self, u8)) {
        match self.cycle {
            1 => {
                self.addr = u16::from(bus.read(self.regs.pc).data);
                self.regs.pc = self.regs.pc.wrapping_add(1);
                self.cycle = 2;
            }
            2 => {
                let hi = bus.read(self.regs.pc).data;
                self.regs.pc = self.regs.pc.wrapping_add(1);
                let lo = (self.addr as u8).wrapping_add(self.regs.y);
                self.addr = u16::from(lo) | (u16::from(hi) << 8);
                self.data = if lo < self.regs.y { 1 } else { 0 };
                self.cycle = 3;
            }
            3 => {
                if self.data != 0 {
                    let _ = bus.read(self.addr);
                    self.addr = self.addr.wrapping_add(0x100);
                    self.cycle = 4;
                } else {
                    self.data = bus.read(self.addr).data;
                    op(self, self.data);
                    self.finish();
                }
            }
            4 => {
                self.data = bus.read(self.addr).data;
                op(self, self.data);
                self.finish();
            }
            _ => unreachable!(),
        }
    }

    /// Indexed indirect (zp,X) addressing.
    fn addr_izx<B: Bus>(&mut self, bus: &mut B, op: fn(&mut Self, u8)) {
        match self.cycle {
            1 => {
                self.pointer = bus.read(self.regs.pc).data;
                self.regs.pc = self.regs.pc.wrapping_add(1);
                self.cycle = 2;
            }
            2 => {
                // Dummy read while adding X
                let _ = bus.read(u16::from(self.pointer));
                self.pointer = self.pointer.wrapping_add(self.regs.x);
                self.cycle = 3;
            }
            3 => {
                // Read low byte of address
                self.addr = u16::from(bus.read(u16::from(self.pointer)).data);
                self.cycle = 4;
            }
            4 => {
                // Read high byte of address (wraps in zero page)
                self.addr |=
                    u16::from(bus.read(u16::from(self.pointer.wrapping_add(1))).data) << 8;
                self.cycle = 5;
            }
            5 => {
                // Read from final address
                self.data = bus.read(self.addr).data;
                op(self, self.data);
                self.finish();
            }
            _ => unreachable!(),
        }
    }

    /// Indirect indexed (zp),Y addressing.
    fn addr_izy<B: Bus>(&mut self, bus: &mut B, op: fn(&mut Self, u8)) {
        match self.cycle {
            1 => {
                self.pointer = bus.read(self.regs.pc).data;
                self.regs.pc = self.regs.pc.wrapping_add(1);
                self.cycle = 2;
            }
            2 => {
                // Read low byte of base address
                self.addr = u16::from(bus.read(u16::from(self.pointer)).data);
                self.cycle = 3;
            }
            3 => {
                // Read high byte of base address
                let hi = bus.read(u16::from(self.pointer.wrapping_add(1))).data;
                let lo = (self.addr as u8).wrapping_add(self.regs.y);
                self.addr = u16::from(lo) | (u16::from(hi) << 8);
                self.data = if lo < self.regs.y { 1 } else { 0 };
                self.cycle = 4;
            }
            4 => {
                if self.data != 0 {
                    // Page crossed
                    let _ = bus.read(self.addr);
                    self.addr = self.addr.wrapping_add(0x100);
                    self.cycle = 5;
                } else {
                    self.data = bus.read(self.addr).data;
                    op(self, self.data);
                    self.finish();
                }
            }
            5 => {
                self.data = bus.read(self.addr).data;
                op(self, self.data);
                self.finish();
            }
            _ => unreachable!(),
        }
    }

    // ========================================================================
    // Addressing mode helpers - write operations
    // ========================================================================

    /// Zero page write.
    fn addr_zp_w<B: Bus>(&mut self, bus: &mut B, val: fn(&Self) -> u8) {
        match self.cycle {
            1 => {
                self.addr = u16::from(bus.read(self.regs.pc).data);
                self.regs.pc = self.regs.pc.wrapping_add(1);
                self.cycle = 2;
            }
            2 => {
                bus.write(self.addr, val(self));
                self.finish();
            }
            _ => unreachable!(),
        }
    }

    /// Zero page,X write.
    fn addr_zpx_w<B: Bus>(&mut self, bus: &mut B, val: fn(&Self) -> u8) {
        match self.cycle {
            1 => {
                self.pointer = bus.read(self.regs.pc).data;
                self.regs.pc = self.regs.pc.wrapping_add(1);
                self.cycle = 2;
            }
            2 => {
                let _ = bus.read(u16::from(self.pointer));
                self.addr = u16::from(self.pointer.wrapping_add(self.regs.x));
                self.cycle = 3;
            }
            3 => {
                bus.write(self.addr, val(self));
                self.finish();
            }
            _ => unreachable!(),
        }
    }

    /// Zero page,Y write.
    fn addr_zpy_w<B: Bus>(&mut self, bus: &mut B, val: fn(&Self) -> u8) {
        match self.cycle {
            1 => {
                self.pointer = bus.read(self.regs.pc).data;
                self.regs.pc = self.regs.pc.wrapping_add(1);
                self.cycle = 2;
            }
            2 => {
                let _ = bus.read(u16::from(self.pointer));
                self.addr = u16::from(self.pointer.wrapping_add(self.regs.y));
                self.cycle = 3;
            }
            3 => {
                bus.write(self.addr, val(self));
                self.finish();
            }
            _ => unreachable!(),
        }
    }

    /// Absolute write.
    fn addr_abs_w<B: Bus>(&mut self, bus: &mut B, val: fn(&Self) -> u8) {
        match self.cycle {
            1 => {
                self.addr = u16::from(bus.read(self.regs.pc).data);
                self.regs.pc = self.regs.pc.wrapping_add(1);
                self.cycle = 2;
            }
            2 => {
                self.addr |= u16::from(bus.read(self.regs.pc).data) << 8;
                self.regs.pc = self.regs.pc.wrapping_add(1);
                self.cycle = 3;
            }
            3 => {
                bus.write(self.addr, val(self));
                self.finish();
            }
            _ => unreachable!(),
        }
    }

    /// Absolute,X write (always 5 cycles, no page crossing optimization).
    fn addr_abx_w<B: Bus>(&mut self, bus: &mut B, val: fn(&Self) -> u8) {
        match self.cycle {
            1 => {
                self.addr = u16::from(bus.read(self.regs.pc).data);
                self.regs.pc = self.regs.pc.wrapping_add(1);
                self.cycle = 2;
            }
            2 => {
                let hi = bus.read(self.regs.pc).data;
                self.regs.pc = self.regs.pc.wrapping_add(1);
                let lo = (self.addr as u8).wrapping_add(self.regs.x);
                self.addr = u16::from(lo) | (u16::from(hi) << 8);
                self.data = if lo < self.regs.x { 1 } else { 0 };
                self.cycle = 3;
            }
            3 => {
                // Always dummy read for write operations
                let _ = bus.read(self.addr);
                if self.data != 0 {
                    self.addr = self.addr.wrapping_add(0x100);
                }
                self.cycle = 4;
            }
            4 => {
                bus.write(self.addr, val(self));
                self.finish();
            }
            _ => unreachable!(),
        }
    }

    /// Absolute,Y write (always 5 cycles).
    fn addr_aby_w<B: Bus>(&mut self, bus: &mut B, val: fn(&Self) -> u8) {
        match self.cycle {
            1 => {
                self.addr = u16::from(bus.read(self.regs.pc).data);
                self.regs.pc = self.regs.pc.wrapping_add(1);
                self.cycle = 2;
            }
            2 => {
                let hi = bus.read(self.regs.pc).data;
                self.regs.pc = self.regs.pc.wrapping_add(1);
                let lo = (self.addr as u8).wrapping_add(self.regs.y);
                self.addr = u16::from(lo) | (u16::from(hi) << 8);
                self.data = if lo < self.regs.y { 1 } else { 0 };
                self.cycle = 3;
            }
            3 => {
                let _ = bus.read(self.addr);
                if self.data != 0 {
                    self.addr = self.addr.wrapping_add(0x100);
                }
                self.cycle = 4;
            }
            4 => {
                bus.write(self.addr, val(self));
                self.finish();
            }
            _ => unreachable!(),
        }
    }

    /// Indexed indirect (zp,X) write.
    fn addr_izx_w<B: Bus>(&mut self, bus: &mut B, val: fn(&Self) -> u8) {
        match self.cycle {
            1 => {
                self.pointer = bus.read(self.regs.pc).data;
                self.regs.pc = self.regs.pc.wrapping_add(1);
                self.cycle = 2;
            }
            2 => {
                let _ = bus.read(u16::from(self.pointer));
                self.pointer = self.pointer.wrapping_add(self.regs.x);
                self.cycle = 3;
            }
            3 => {
                self.addr = u16::from(bus.read(u16::from(self.pointer)).data);
                self.cycle = 4;
            }
            4 => {
                self.addr |=
                    u16::from(bus.read(u16::from(self.pointer.wrapping_add(1))).data) << 8;
                self.cycle = 5;
            }
            5 => {
                bus.write(self.addr, val(self));
                self.finish();
            }
            _ => unreachable!(),
        }
    }

    /// Indirect indexed (zp),Y write (always 6 cycles).
    fn addr_izy_w<B: Bus>(&mut self, bus: &mut B, val: fn(&Self) -> u8) {
        match self.cycle {
            1 => {
                self.pointer = bus.read(self.regs.pc).data;
                self.regs.pc = self.regs.pc.wrapping_add(1);
                self.cycle = 2;
            }
            2 => {
                self.addr = u16::from(bus.read(u16::from(self.pointer)).data);
                self.cycle = 3;
            }
            3 => {
                let hi = bus.read(u16::from(self.pointer.wrapping_add(1))).data;
                let lo = (self.addr as u8).wrapping_add(self.regs.y);
                self.addr = u16::from(lo) | (u16::from(hi) << 8);
                self.data = if lo < self.regs.y { 1 } else { 0 };
                self.cycle = 4;
            }
            4 => {
                // Always dummy read for writes
                let _ = bus.read(self.addr);
                if self.data != 0 {
                    self.addr = self.addr.wrapping_add(0x100);
                }
                self.cycle = 5;
            }
            5 => {
                bus.write(self.addr, val(self));
                self.finish();
            }
            _ => unreachable!(),
        }
    }

    // ========================================================================
    // Addressing mode helpers - read-modify-write operations
    // ========================================================================

    /// Zero page read-modify-write.
    fn addr_zp_rmw<B: Bus>(&mut self, bus: &mut B, op: fn(&mut Self, u8) -> u8) {
        match self.cycle {
            1 => {
                self.addr = u16::from(bus.read(self.regs.pc).data);
                self.regs.pc = self.regs.pc.wrapping_add(1);
                self.cycle = 2;
            }
            2 => {
                self.data = bus.read(self.addr).data;
                self.cycle = 3;
            }
            3 => {
                // Write original value back (dummy write)
                bus.write(self.addr, self.data);
                self.data = op(self, self.data);
                self.cycle = 4;
            }
            4 => {
                bus.write(self.addr, self.data);
                self.finish();
            }
            _ => unreachable!(),
        }
    }

    /// Zero page,X read-modify-write.
    fn addr_zpx_rmw<B: Bus>(&mut self, bus: &mut B, op: fn(&mut Self, u8) -> u8) {
        match self.cycle {
            1 => {
                self.pointer = bus.read(self.regs.pc).data;
                self.regs.pc = self.regs.pc.wrapping_add(1);
                self.cycle = 2;
            }
            2 => {
                let _ = bus.read(u16::from(self.pointer));
                self.addr = u16::from(self.pointer.wrapping_add(self.regs.x));
                self.cycle = 3;
            }
            3 => {
                self.data = bus.read(self.addr).data;
                self.cycle = 4;
            }
            4 => {
                bus.write(self.addr, self.data);
                self.data = op(self, self.data);
                self.cycle = 5;
            }
            5 => {
                bus.write(self.addr, self.data);
                self.finish();
            }
            _ => unreachable!(),
        }
    }

    /// Absolute read-modify-write.
    fn addr_abs_rmw<B: Bus>(&mut self, bus: &mut B, op: fn(&mut Self, u8) -> u8) {
        match self.cycle {
            1 => {
                self.addr = u16::from(bus.read(self.regs.pc).data);
                self.regs.pc = self.regs.pc.wrapping_add(1);
                self.cycle = 2;
            }
            2 => {
                self.addr |= u16::from(bus.read(self.regs.pc).data) << 8;
                self.regs.pc = self.regs.pc.wrapping_add(1);
                self.cycle = 3;
            }
            3 => {
                self.data = bus.read(self.addr).data;
                self.cycle = 4;
            }
            4 => {
                bus.write(self.addr, self.data);
                self.data = op(self, self.data);
                self.cycle = 5;
            }
            5 => {
                bus.write(self.addr, self.data);
                self.finish();
            }
            _ => unreachable!(),
        }
    }

    /// Absolute,X read-modify-write (always 7 cycles).
    fn addr_abx_rmw<B: Bus>(&mut self, bus: &mut B, op: fn(&mut Self, u8) -> u8) {
        match self.cycle {
            1 => {
                self.addr = u16::from(bus.read(self.regs.pc).data);
                self.regs.pc = self.regs.pc.wrapping_add(1);
                self.cycle = 2;
            }
            2 => {
                let hi = bus.read(self.regs.pc).data;
                self.regs.pc = self.regs.pc.wrapping_add(1);
                let lo = (self.addr as u8).wrapping_add(self.regs.x);
                self.addr = u16::from(lo) | (u16::from(hi) << 8);
                self.data = if lo < self.regs.x { 1 } else { 0 };
                self.cycle = 3;
            }
            3 => {
                let _ = bus.read(self.addr);
                if self.data != 0 {
                    self.addr = self.addr.wrapping_add(0x100);
                }
                self.cycle = 4;
            }
            4 => {
                self.data = bus.read(self.addr).data;
                self.cycle = 5;
            }
            5 => {
                bus.write(self.addr, self.data);
                self.data = op(self, self.data);
                self.cycle = 6;
            }
            6 => {
                bus.write(self.addr, self.data);
                self.finish();
            }
            _ => unreachable!(),
        }
    }

    // ========================================================================
    // ALU operations
    // ========================================================================

    fn do_lda(&mut self, val: u8) {
        self.regs.a = val;
        self.regs.p.update_nz(val);
    }

    fn do_ldx(&mut self, val: u8) {
        self.regs.x = val;
        self.regs.p.update_nz(val);
    }

    fn do_ldy(&mut self, val: u8) {
        self.regs.y = val;
        self.regs.p.update_nz(val);
    }

    fn do_ora(&mut self, val: u8) {
        self.regs.a |= val;
        self.regs.p.update_nz(self.regs.a);
    }

    fn do_and(&mut self, val: u8) {
        self.regs.a &= val;
        self.regs.p.update_nz(self.regs.a);
    }

    fn do_eor(&mut self, val: u8) {
        self.regs.a ^= val;
        self.regs.p.update_nz(self.regs.a);
    }

    fn do_adc(&mut self, val: u8) {
        if self.regs.p.is_set(D) {
            self.do_adc_decimal(val);
        } else {
            self.do_adc_binary(val);
        }
    }

    fn do_adc_binary(&mut self, val: u8) {
        let a = self.regs.a;
        let carry = if self.regs.p.is_set(C) { 1u16 } else { 0 };
        let sum = u16::from(a) + u16::from(val) + carry;
        let result = sum as u8;

        self.regs.p.set_if(C, sum > 0xFF);
        self.regs.p
            .set_if(V, (a ^ result) & (val ^ result) & 0x80 != 0);
        self.regs.a = result;
        self.regs.p.update_nz(result);
    }

    fn do_adc_decimal(&mut self, val: u8) {
        let a = self.regs.a;
        let carry = if self.regs.p.is_set(C) { 1 } else { 0 };

        // Low nibble
        let mut lo = (a & 0x0F) + (val & 0x0F) + carry;
        if lo > 9 {
            lo += 6;
        }

        // High nibble
        let mut hi = (a >> 4) + (val >> 4) + if lo > 0x0F { 1 } else { 0 };

        // N and V flags are set based on binary result on NMOS 6502
        let bin_sum = u16::from(a) + u16::from(val) + u16::from(carry);
        let bin_result = bin_sum as u8;
        self.regs.p.set_if(Z, bin_result == 0);
        self.regs.p.set_if(N, hi & 0x08 != 0);
        self.regs
            .p
            .set_if(V, (a ^ bin_result) & (val ^ bin_result) & 0x80 != 0);

        if hi > 9 {
            hi += 6;
        }

        self.regs.p.set_if(C, hi > 0x0F);
        self.regs.a = ((hi << 4) | (lo & 0x0F)) as u8;
    }

    fn do_sbc(&mut self, val: u8) {
        if self.regs.p.is_set(D) {
            self.do_sbc_decimal(val);
        } else {
            // SBC is ADC with inverted operand
            self.do_adc_binary(!val);
        }
    }

    fn do_sbc_decimal(&mut self, val: u8) {
        let a = self.regs.a;
        let borrow = if self.regs.p.is_set(C) { 0i16 } else { 1 };

        // Binary result for flags (NMOS behavior)
        let bin_result = i16::from(a) - i16::from(val) - borrow;
        self.regs.p.set_if(C, bin_result >= 0);
        self.regs.p.set_if(Z, (bin_result as u8) == 0);
        self.regs.p.set_if(N, bin_result & 0x80 != 0);
        self.regs.p.set_if(
            V,
            (i16::from(a) ^ bin_result) & (i16::from(a) ^ i16::from(val)) & 0x80 != 0,
        );

        // Decimal calculation
        let mut lo = i16::from(a & 0x0F) - i16::from(val & 0x0F) - borrow;
        let mut hi = i16::from(a >> 4) - i16::from(val >> 4);

        if lo < 0 {
            lo -= 6;
            hi -= 1;
        }
        if hi < 0 {
            hi -= 6;
        }

        self.regs.a = ((hi << 4) as u8) | ((lo & 0x0F) as u8);
    }

    fn do_cmp(&mut self, val: u8) {
        let result = self.regs.a.wrapping_sub(val);
        self.regs.p.set_if(C, self.regs.a >= val);
        self.regs.p.update_nz(result);
    }

    fn do_cpx(&mut self, val: u8) {
        let result = self.regs.x.wrapping_sub(val);
        self.regs.p.set_if(C, self.regs.x >= val);
        self.regs.p.update_nz(result);
    }

    fn do_cpy(&mut self, val: u8) {
        let result = self.regs.y.wrapping_sub(val);
        self.regs.p.set_if(C, self.regs.y >= val);
        self.regs.p.update_nz(result);
    }

    fn do_bit(&mut self, val: u8) {
        self.regs.p.set_if(Z, self.regs.a & val == 0);
        self.regs.p.set_if(N, val & 0x80 != 0);
        self.regs.p.set_if(V, val & 0x40 != 0);
    }

    fn do_asl(&mut self, val: u8) -> u8 {
        self.regs.p.set_if(C, val & 0x80 != 0);
        let result = val << 1;
        self.regs.p.update_nz(result);
        result
    }

    fn do_lsr(&mut self, val: u8) -> u8 {
        self.regs.p.set_if(C, val & 0x01 != 0);
        let result = val >> 1;
        self.regs.p.update_nz(result);
        result
    }

    fn do_rol(&mut self, val: u8) -> u8 {
        let carry = if self.regs.p.is_set(C) { 1 } else { 0 };
        self.regs.p.set_if(C, val & 0x80 != 0);
        let result = (val << 1) | carry;
        self.regs.p.update_nz(result);
        result
    }

    fn do_ror(&mut self, val: u8) -> u8 {
        let carry = if self.regs.p.is_set(C) { 0x80 } else { 0 };
        self.regs.p.set_if(C, val & 0x01 != 0);
        let result = (val >> 1) | carry;
        self.regs.p.update_nz(result);
        result
    }

    fn do_inc(&mut self, val: u8) -> u8 {
        let result = val.wrapping_add(1);
        self.regs.p.update_nz(result);
        result
    }

    fn do_dec(&mut self, val: u8) -> u8 {
        let result = val.wrapping_sub(1);
        self.regs.p.update_nz(result);
        result
    }

    // ========================================================================
    // Individual instruction implementations
    // ========================================================================

    fn op_brk<B: Bus>(&mut self, bus: &mut B) {
        match self.cycle {
            1 => {
                // Padding byte (ignored but PC incremented)
                let _ = bus.read(self.regs.pc);
                self.regs.pc = self.regs.pc.wrapping_add(1);
                self.cycle = 2;
            }
            2 => {
                // Push PCH
                let addr = self.regs.push();
                bus.write(addr, (self.regs.pc >> 8) as u8);
                self.cycle = 3;
            }
            3 => {
                // Push PCL
                let addr = self.regs.push();
                bus.write(addr, self.regs.pc as u8);
                self.cycle = 4;
            }
            4 => {
                // Push status with B flag set
                let addr = self.regs.push();
                bus.write(addr, self.regs.p.to_byte_brk());
                self.cycle = 5;
            }
            5 => {
                // Read vector low byte
                // Use self.addr if set (for NMI/IRQ), otherwise use BRK vector
                let vector = if self.addr != 0 { self.addr } else { 0xFFFE };
                self.addr = u16::from(bus.read(vector).data);
                self.cycle = 6;
            }
            6 => {
                // Read vector high byte
                let vector = if self.addr < 0x100 {
                    // IRQ/NMI vector
                    self.addr | 0xFF00
                } else {
                    0xFFFF
                };
                let vector_addr = if self.addr >= 0xFFFA {
                    self.addr + 1
                } else {
                    vector
                };
                self.addr =
                    (self.addr & 0xFF) | (u16::from(bus.read(vector_addr).data) << 8);
                self.regs.pc = self.addr;
                self.regs.p.set(I);
                self.addr = 0;
                self.finish();
            }
            _ => unreachable!(),
        }
    }

    fn op_rti<B: Bus>(&mut self, bus: &mut B) {
        match self.cycle {
            1 => {
                // Dummy read
                let _ = bus.read(self.regs.pc);
                self.cycle = 2;
            }
            2 => {
                // Dummy stack read
                let _ = bus.read(self.regs.stack_addr());
                self.cycle = 3;
            }
            3 => {
                // Pull status
                let addr = self.regs.pop();
                self.regs.p = Status::from_byte(bus.read(addr).data);
                self.cycle = 4;
            }
            4 => {
                // Pull PCL
                let addr = self.regs.pop();
                self.addr = u16::from(bus.read(addr).data);
                self.cycle = 5;
            }
            5 => {
                // Pull PCH
                let addr = self.regs.pop();
                self.addr |= u16::from(bus.read(addr).data) << 8;
                self.regs.pc = self.addr;
                self.finish();
            }
            _ => unreachable!(),
        }
    }

    fn op_rts<B: Bus>(&mut self, bus: &mut B) {
        match self.cycle {
            1 => {
                let _ = bus.read(self.regs.pc);
                self.cycle = 2;
            }
            2 => {
                let _ = bus.read(self.regs.stack_addr());
                self.cycle = 3;
            }
            3 => {
                let addr = self.regs.pop();
                self.addr = u16::from(bus.read(addr).data);
                self.cycle = 4;
            }
            4 => {
                let addr = self.regs.pop();
                self.addr |= u16::from(bus.read(addr).data) << 8;
                self.cycle = 5;
            }
            5 => {
                // Increment PC (RTS returns to address + 1)
                let _ = bus.read(self.addr);
                self.regs.pc = self.addr.wrapping_add(1);
                self.finish();
            }
            _ => unreachable!(),
        }
    }

    fn op_jsr<B: Bus>(&mut self, bus: &mut B) {
        match self.cycle {
            1 => {
                // Read low byte of target
                self.addr = u16::from(bus.read(self.regs.pc).data);
                self.regs.pc = self.regs.pc.wrapping_add(1);
                self.cycle = 2;
            }
            2 => {
                // Internal operation (stack read)
                let _ = bus.read(self.regs.stack_addr());
                self.cycle = 3;
            }
            3 => {
                // Push PCH
                let addr = self.regs.push();
                bus.write(addr, (self.regs.pc >> 8) as u8);
                self.cycle = 4;
            }
            4 => {
                // Push PCL
                let addr = self.regs.push();
                bus.write(addr, self.regs.pc as u8);
                self.cycle = 5;
            }
            5 => {
                // Read high byte of target
                self.addr |= u16::from(bus.read(self.regs.pc).data) << 8;
                self.regs.pc = self.addr;
                self.finish();
            }
            _ => unreachable!(),
        }
    }

    fn op_jmp_abs<B: Bus>(&mut self, bus: &mut B) {
        match self.cycle {
            1 => {
                self.addr = u16::from(bus.read(self.regs.pc).data);
                self.regs.pc = self.regs.pc.wrapping_add(1);
                self.cycle = 2;
            }
            2 => {
                self.addr |= u16::from(bus.read(self.regs.pc).data) << 8;
                self.regs.pc = self.addr;
                self.finish();
            }
            _ => unreachable!(),
        }
    }

    fn op_jmp_ind<B: Bus>(&mut self, bus: &mut B) {
        match self.cycle {
            1 => {
                self.addr = u16::from(bus.read(self.regs.pc).data);
                self.regs.pc = self.regs.pc.wrapping_add(1);
                self.cycle = 2;
            }
            2 => {
                self.addr |= u16::from(bus.read(self.regs.pc).data) << 8;
                self.cycle = 3;
            }
            3 => {
                self.data = bus.read(self.addr).data;
                self.cycle = 4;
            }
            4 => {
                // 6502 bug: wraps within page for high byte
                let hi_addr = (self.addr & 0xFF00) | ((self.addr.wrapping_add(1)) & 0x00FF);
                let hi = bus.read(hi_addr).data;
                self.regs.pc = u16::from(self.data) | (u16::from(hi) << 8);
                self.finish();
            }
            _ => unreachable!(),
        }
    }

    fn op_branch<B: Bus>(&mut self, bus: &mut B, taken: bool) {
        match self.cycle {
            1 => {
                // Read offset
                self.data = bus.read(self.regs.pc).data;
                self.regs.pc = self.regs.pc.wrapping_add(1);
                if !taken {
                    self.finish();
                } else {
                    self.cycle = 2;
                }
            }
            2 => {
                // Branch taken - calculate target
                let _ = bus.read(self.regs.pc); // Dummy read
                let offset = self.data as i8 as i16;
                let new_pc = (self.regs.pc as i16).wrapping_add(offset) as u16;
                // Check for page crossing
                if (new_pc ^ self.regs.pc) & 0xFF00 != 0 {
                    // Page crossed - need extra cycle
                    self.addr = new_pc;
                    self.cycle = 3;
                } else {
                    self.regs.pc = new_pc;
                    self.finish();
                }
            }
            3 => {
                // Page boundary crossed
                let _ = bus.read((self.regs.pc & 0xFF00) | (self.addr & 0x00FF));
                self.regs.pc = self.addr;
                self.finish();
            }
            _ => unreachable!(),
        }
    }

    fn op_php<B: Bus>(&mut self, bus: &mut B) {
        match self.cycle {
            1 => {
                let _ = bus.read(self.regs.pc);
                self.cycle = 2;
            }
            2 => {
                let addr = self.regs.push();
                bus.write(addr, self.regs.p.to_byte_brk());
                self.finish();
            }
            _ => unreachable!(),
        }
    }

    fn op_plp<B: Bus>(&mut self, bus: &mut B) {
        match self.cycle {
            1 => {
                let _ = bus.read(self.regs.pc);
                self.cycle = 2;
            }
            2 => {
                let _ = bus.read(self.regs.stack_addr());
                self.cycle = 3;
            }
            3 => {
                let addr = self.regs.pop();
                self.regs.p = Status::from_byte(bus.read(addr).data);
                self.finish();
            }
            _ => unreachable!(),
        }
    }

    fn op_pha<B: Bus>(&mut self, bus: &mut B) {
        match self.cycle {
            1 => {
                let _ = bus.read(self.regs.pc);
                self.cycle = 2;
            }
            2 => {
                let addr = self.regs.push();
                bus.write(addr, self.regs.a);
                self.finish();
            }
            _ => unreachable!(),
        }
    }

    fn op_pla<B: Bus>(&mut self, bus: &mut B) {
        match self.cycle {
            1 => {
                let _ = bus.read(self.regs.pc);
                self.cycle = 2;
            }
            2 => {
                let _ = bus.read(self.regs.stack_addr());
                self.cycle = 3;
            }
            3 => {
                let addr = self.regs.pop();
                self.regs.a = bus.read(addr).data;
                self.regs.p.update_nz(self.regs.a);
                self.finish();
            }
            _ => unreachable!(),
        }
    }

    fn op_flag<B: Bus>(&mut self, bus: &mut B, flag: u8, set: bool) {
        if self.cycle == 1 {
            let _ = bus.read(self.regs.pc);
            self.regs.p.set_if(flag, set);
            self.finish();
        }
    }

    fn op_nop<B: Bus>(&mut self, bus: &mut B) {
        if self.cycle == 1 {
            let _ = bus.read(self.regs.pc);
            self.finish();
        }
    }

    // Transfer instructions
    fn op_tax<B: Bus>(&mut self, bus: &mut B) {
        if self.cycle == 1 {
            let _ = bus.read(self.regs.pc);
            self.regs.x = self.regs.a;
            self.regs.p.update_nz(self.regs.x);
            self.finish();
        }
    }

    fn op_tay<B: Bus>(&mut self, bus: &mut B) {
        if self.cycle == 1 {
            let _ = bus.read(self.regs.pc);
            self.regs.y = self.regs.a;
            self.regs.p.update_nz(self.regs.y);
            self.finish();
        }
    }

    fn op_txa<B: Bus>(&mut self, bus: &mut B) {
        if self.cycle == 1 {
            let _ = bus.read(self.regs.pc);
            self.regs.a = self.regs.x;
            self.regs.p.update_nz(self.regs.a);
            self.finish();
        }
    }

    fn op_tya<B: Bus>(&mut self, bus: &mut B) {
        if self.cycle == 1 {
            let _ = bus.read(self.regs.pc);
            self.regs.a = self.regs.y;
            self.regs.p.update_nz(self.regs.a);
            self.finish();
        }
    }

    fn op_tsx<B: Bus>(&mut self, bus: &mut B) {
        if self.cycle == 1 {
            let _ = bus.read(self.regs.pc);
            self.regs.x = self.regs.s;
            self.regs.p.update_nz(self.regs.x);
            self.finish();
        }
    }

    fn op_txs<B: Bus>(&mut self, bus: &mut B) {
        if self.cycle == 1 {
            let _ = bus.read(self.regs.pc);
            self.regs.s = self.regs.x;
            // TXS does not affect flags
            self.finish();
        }
    }

    // Increment/decrement registers
    fn op_inx<B: Bus>(&mut self, bus: &mut B) {
        if self.cycle == 1 {
            let _ = bus.read(self.regs.pc);
            self.regs.x = self.regs.x.wrapping_add(1);
            self.regs.p.update_nz(self.regs.x);
            self.finish();
        }
    }

    fn op_iny<B: Bus>(&mut self, bus: &mut B) {
        if self.cycle == 1 {
            let _ = bus.read(self.regs.pc);
            self.regs.y = self.regs.y.wrapping_add(1);
            self.regs.p.update_nz(self.regs.y);
            self.finish();
        }
    }

    fn op_dex<B: Bus>(&mut self, bus: &mut B) {
        if self.cycle == 1 {
            let _ = bus.read(self.regs.pc);
            self.regs.x = self.regs.x.wrapping_sub(1);
            self.regs.p.update_nz(self.regs.x);
            self.finish();
        }
    }

    fn op_dey<B: Bus>(&mut self, bus: &mut B) {
        if self.cycle == 1 {
            let _ = bus.read(self.regs.pc);
            self.regs.y = self.regs.y.wrapping_sub(1);
            self.regs.p.update_nz(self.regs.y);
            self.finish();
        }
    }

    // Accumulator shift/rotate
    fn op_asl_a<B: Bus>(&mut self, bus: &mut B) {
        if self.cycle == 1 {
            let _ = bus.read(self.regs.pc);
            self.regs.a = self.do_asl(self.regs.a);
            self.finish();
        }
    }

    fn op_lsr_a<B: Bus>(&mut self, bus: &mut B) {
        if self.cycle == 1 {
            let _ = bus.read(self.regs.pc);
            self.regs.a = self.do_lsr(self.regs.a);
            self.finish();
        }
    }

    fn op_rol_a<B: Bus>(&mut self, bus: &mut B) {
        if self.cycle == 1 {
            let _ = bus.read(self.regs.pc);
            self.regs.a = self.do_rol(self.regs.a);
            self.finish();
        }
    }

    fn op_ror_a<B: Bus>(&mut self, bus: &mut B) {
        if self.cycle == 1 {
            let _ = bus.read(self.regs.pc);
            self.regs.a = self.do_ror(self.regs.a);
            self.finish();
        }
    }
}

// ============================================================================
// Trait implementations
// ============================================================================

impl Cpu for Mos6502 {
    type Registers = Registers;

    fn tick<B: Bus>(&mut self, bus: &mut B) {
        self.execute_cycle(bus);
    }

    fn pc(&self) -> u16 {
        self.regs.pc
    }

    fn registers(&self) -> Self::Registers {
        self.regs
    }

    fn is_halted(&self) -> bool {
        self.state == State::Stopped
    }

    fn interrupt(&mut self) -> bool {
        if !self.regs.p.is_set(I) {
            self.irq_pending = true;
            true
        } else {
            false
        }
    }

    fn nmi(&mut self) {
        self.nmi_pending = true;
    }

    fn reset(&mut self) {
        self.regs = Registers::new();
        self.state = State::FetchOpcode;
        self.opcode = 0;
        self.cycle = 0;
        self.addr = 0;
        self.data = 0;
        self.pointer = 0;
        self.nmi_pending = false;
        self.irq_pending = false;
        // Note: reset sequence should read from $FFFC/$FFFD
        // For now, caller must set PC after reset
    }
}

impl Observable for Mos6502 {
    fn query(&self, path: &str) -> Option<Value> {
        match path {
            "pc" => Some(self.regs.pc.into()),
            "a" => Some(self.regs.a.into()),
            "x" => Some(self.regs.x.into()),
            "y" => Some(self.regs.y.into()),
            "s" | "sp" => Some(self.regs.s.into()),
            "p" | "status" => Some(self.regs.p.0.into()),
            "flags.c" | "c" => Some(self.regs.p.is_set(C).into()),
            "flags.z" | "z" => Some(self.regs.p.is_set(Z).into()),
            "flags.i" | "i" => Some(self.regs.p.is_set(I).into()),
            "flags.d" | "d" => Some(self.regs.p.is_set(D).into()),
            "flags.v" | "v" => Some(self.regs.p.is_set(V).into()),
            "flags.n" | "n" => Some(self.regs.p.is_set(N).into()),
            "cycle" => Some(Value::U64(self.total_cycles)),
            "halted" => Some(self.is_halted().into()),
            _ => None,
        }
    }

    fn query_paths(&self) -> &'static [&'static str] {
        &[
            "pc", "a", "x", "y", "s", "p", "flags.c", "flags.z", "flags.i", "flags.d", "flags.v",
            "flags.n", "cycle", "halted",
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use emu_core::SimpleBus;

    #[test]
    fn test_lda_immediate() {
        let mut cpu = Mos6502::new();
        let mut bus = SimpleBus::new();

        // LDA #$42
        bus.load(0x0000, &[0xA9, 0x42]);
        cpu.regs.pc = 0x0000;

        // Cycle 1: fetch opcode
        cpu.tick(&mut bus);
        // Cycle 2: fetch operand, execute
        cpu.tick(&mut bus);

        assert_eq!(cpu.regs.a, 0x42);
        assert_eq!(cpu.regs.pc, 0x0002);
    }

    #[test]
    fn test_sta_zeropage() {
        let mut cpu = Mos6502::new();
        let mut bus = SimpleBus::new();

        cpu.regs.a = 0x55;
        // STA $10
        bus.load(0x0000, &[0x85, 0x10]);
        cpu.regs.pc = 0x0000;

        // 3 cycles for STA zp
        for _ in 0..3 {
            cpu.tick(&mut bus);
        }

        assert_eq!(bus.peek(0x0010), 0x55);
    }

    #[test]
    fn test_jmp_absolute() {
        let mut cpu = Mos6502::new();
        let mut bus = SimpleBus::new();

        // JMP $1234
        bus.load(0x0000, &[0x4C, 0x34, 0x12]);
        cpu.regs.pc = 0x0000;

        // 3 cycles for JMP abs
        for _ in 0..3 {
            cpu.tick(&mut bus);
        }

        assert_eq!(cpu.regs.pc, 0x1234);
    }
}
