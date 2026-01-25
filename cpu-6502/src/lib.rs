//! MOS 6502/6510/8502 CPU emulator.
//!
//! This implements the NMOS 6502 instruction set including commonly-used
//! undocumented ("illegal") opcodes. Compatible CPU variants:
//!
//! - **6502** - Original NMOS CPU
//! - **6510** - C64 variant with I/O port at $00-$01
//! - **8502** - C128 variant, identical to 6510 but runs at 2 MHz
//!
//! The I/O port at addresses $00-$01 (used by 6510/8502 for memory banking)
//! is handled by the memory subsystem, not this CPU implementation.
//!
//! # Illegal Opcodes
//!
//! Implemented undocumented opcodes commonly used by C64/C128 software:
//! - LAX, SAX, DCP, ISC (Tier 1 - essential)
//! - SLO, SRE, RLA, RRA (Tier 2 - important)
//! - ANC, ALR, ARR, SBX (Tier 3 - immediate-only)

use emu_core::{Bus, Cpu};

mod addressing;
mod flags;

use flags::*;

/// The MOS 6502/6510/8502 CPU state.
///
/// This struct represents the CPU registers and internal state. It can be used
/// to emulate any of the compatible variants (6502, 6510, 8502) - the only
/// differences between these CPUs are in their I/O ports and clock speeds,
/// which are handled externally by the machine emulation.
pub struct Mos6502 {
    /// Accumulator
    pub(crate) a: u8,
    /// X index register
    pub(crate) x: u8,
    /// Y index register
    pub(crate) y: u8,
    /// Stack pointer (points to $0100-$01FF)
    pub(crate) sp: u8,
    /// Program counter
    pub(crate) pc: u16,
    /// Status register (NV-BDIZC)
    pub(crate) p: u8,

    /// NMI pending flag
    nmi_pending: bool,
    /// IRQ pending flag
    irq_pending: bool,
}

impl Mos6502 {
    pub fn new() -> Self {
        Self {
            a: 0,
            x: 0,
            y: 0,
            sp: 0xFD, // After reset, SP is $FD
            pc: 0,
            p: 0x24, // I flag set, bit 5 always 1
            nmi_pending: false,
            irq_pending: false,
        }
    }

    // =========================================================================
    // Public register accessors
    // =========================================================================

    pub fn pc(&self) -> u16 {
        self.pc
    }

    pub fn a(&self) -> u8 {
        self.a
    }

    pub fn x(&self) -> u8 {
        self.x
    }

    pub fn y(&self) -> u8 {
        self.y
    }

    pub fn sp(&self) -> u8 {
        self.sp
    }

    pub fn status(&self) -> u8 {
        self.p
    }

    pub fn set_a(&mut self, value: u8) {
        self.a = value;
    }

    pub fn set_x(&mut self, value: u8) {
        self.x = value;
    }

    pub fn set_y(&mut self, value: u8) {
        self.y = value;
    }

    pub fn set_sp(&mut self, value: u8) {
        self.sp = value;
    }

    pub fn set_pc(&mut self, value: u16) {
        self.pc = value;
    }

    pub fn set_status(&mut self, value: u8) {
        self.p = value | (1 << FLAG_U); // Bit 5 always 1
    }

    // =========================================================================
    // ALU operations
    // =========================================================================

    /// ADC - Add with Carry
    fn adc(&mut self, value: u8) {
        if self.decimal() {
            self.adc_decimal(value);
        } else {
            self.adc_binary(value);
        }
    }

    fn adc_binary(&mut self, value: u8) {
        let a = self.a as u16;
        let v = value as u16;
        let c = if self.carry() { 1 } else { 0 };

        let result = a + v + c;
        let result8 = result as u8;

        self.set_flag(FLAG_C, result > 0xFF);
        self.set_flag(FLAG_V, (self.a ^ result8) & (value ^ result8) & 0x80 != 0);
        self.set_zn(result8);
        self.a = result8;
    }

    fn adc_decimal(&mut self, value: u8) {
        let a = self.a as u16;
        let v = value as u16;
        let c = if self.carry() { 1 } else { 0 };

        // Low nibble
        let mut low = (a & 0x0F) + (v & 0x0F) + c;
        if low > 9 {
            low += 6;
        }

        // High nibble
        let mut high = (a >> 4) + (v >> 4) + if low > 0x0F { 1 } else { 0 };

        // Set flags based on intermediate binary result for Z, N, V
        // (This matches NMOS 6502 behavior)
        let binary_result = (a + v + c) as u8;
        let br16 = binary_result as u16;
        self.set_flag(FLAG_Z, binary_result == 0);
        self.set_flag(FLAG_N, high & 0x08 != 0);
        self.set_flag(FLAG_V, ((a ^ br16) & (v ^ br16) & 0x80) != 0);

        if high > 9 {
            high += 6;
        }

        self.set_flag(FLAG_C, high > 0x0F);
        self.a = ((high << 4) | (low & 0x0F)) as u8;
    }

    /// SBC - Subtract with Carry (borrow)
    fn sbc(&mut self, value: u8) {
        if self.decimal() {
            self.sbc_decimal(value);
        } else {
            self.sbc_binary(value);
        }
    }

    fn sbc_binary(&mut self, value: u8) {
        let a = self.a as u16;
        let v = value as u16;
        let c = if self.carry() { 0 } else { 1 };

        let result = a.wrapping_sub(v).wrapping_sub(c);
        let result8 = result as u8;

        self.set_flag(FLAG_C, result < 0x100);
        self.set_flag(FLAG_V, (self.a ^ value) & (self.a ^ result8) & 0x80 != 0);
        self.set_zn(result8);
        self.a = result8;
    }

    fn sbc_decimal(&mut self, value: u8) {
        let a = self.a as i16;
        let v = value as i16;
        let c = if self.carry() { 0 } else { 1 };

        // Low nibble
        let mut low = (a & 0x0F) - (v & 0x0F) - c;
        if low < 0 {
            low = ((low - 6) & 0x0F) - 0x10;
        }

        // High nibble
        let mut high = (a >> 4) - (v >> 4) + if low < 0 { -1 } else { 0 };
        if high < 0 {
            high = (high - 6) & 0x0F;
        }

        // Binary result for flags (NMOS behavior)
        let binary_result = a.wrapping_sub(v).wrapping_sub(c);
        let nv = !v; // complement of v
        self.set_flag(FLAG_C, binary_result >= 0);
        self.set_flag(FLAG_Z, (binary_result as u8) == 0);
        self.set_flag(FLAG_N, binary_result & 0x80 != 0);
        self.set_flag(
            FLAG_V,
            ((a ^ binary_result) & (nv ^ binary_result) & 0x80) != 0,
        );

        self.a = ((high << 4) | (low & 0x0F)) as u8;
    }

    /// CMP - Compare accumulator
    fn cmp(&mut self, a: u8, value: u8) {
        let result = a.wrapping_sub(value);
        self.set_flag(FLAG_C, a >= value);
        self.set_zn(result);
    }

    /// ASL - Arithmetic Shift Left
    fn asl(&mut self, value: u8) -> u8 {
        self.set_flag(FLAG_C, value & 0x80 != 0);
        let result = value << 1;
        self.set_zn(result);
        result
    }

    /// LSR - Logical Shift Right
    fn lsr(&mut self, value: u8) -> u8 {
        self.set_flag(FLAG_C, value & 0x01 != 0);
        let result = value >> 1;
        self.set_zn(result);
        result
    }

    /// ROL - Rotate Left
    fn rol(&mut self, value: u8) -> u8 {
        let carry_in = if self.carry() { 1 } else { 0 };
        self.set_flag(FLAG_C, value & 0x80 != 0);
        let result = (value << 1) | carry_in;
        self.set_zn(result);
        result
    }

    /// ROR - Rotate Right
    fn ror(&mut self, value: u8) -> u8 {
        let carry_in = if self.carry() { 0x80 } else { 0 };
        self.set_flag(FLAG_C, value & 0x01 != 0);
        let result = (value >> 1) | carry_in;
        self.set_zn(result);
        result
    }

    /// BIT - Bit Test
    fn bit(&mut self, value: u8) {
        self.set_flag(FLAG_Z, self.a & value == 0);
        self.set_flag(FLAG_N, value & 0x80 != 0);
        self.set_flag(FLAG_V, value & 0x40 != 0);
    }
}

impl Default for Mos6502 {
    fn default() -> Self {
        Self::new()
    }
}

impl<B: Bus> Cpu<B> for Mos6502 {
    fn step(&mut self, bus: &mut B) -> u32 {
        // Handle pending interrupts
        if self.nmi_pending {
            self.nmi_pending = false;
            return self.handle_nmi(bus);
        }

        if self.irq_pending && !self.interrupt_disable() {
            self.irq_pending = false;
            return self.handle_irq(bus);
        }

        let opcode = self.fetch(bus);
        self.execute(bus, opcode)
    }

    fn reset(&mut self, bus: &mut B) {
        // Reset takes 7 cycles
        bus.tick(7);

        // Read reset vector at $FFFC-$FFFD
        self.pc = self.read_word(bus, 0xFFFC);

        // Initial state after reset
        self.sp = 0xFD; // SP decremented by 3 during reset (but no actual writes)
        self.p = 0x24; // I flag set, bit 5 always 1
        self.a = 0;
        self.x = 0;
        self.y = 0;
        self.nmi_pending = false;
        self.irq_pending = false;
    }

    fn interrupt(&mut self, _bus: &mut B) {
        self.irq_pending = true;
    }

    fn nmi(&mut self, _bus: &mut B) {
        self.nmi_pending = true;
    }

    fn pc(&self) -> u16 {
        self.pc
    }
}

impl Mos6502 {
    fn handle_irq(&mut self, bus: &mut impl Bus) -> u32 {
        // IRQ/BRK takes 7 cycles
        bus.tick(2); // Internal operations

        self.push_word(bus, self.pc);
        self.push(bus, self.status_for_push(false)); // B flag clear for IRQ

        self.set_flag(FLAG_I, true);
        self.pc = self.read_word(bus, 0xFFFE);

        7
    }

    fn handle_nmi(&mut self, bus: &mut impl Bus) -> u32 {
        // NMI takes 7 cycles
        bus.tick(2); // Internal operations

        self.push_word(bus, self.pc);
        self.push(bus, self.status_for_push(false)); // B flag clear for NMI

        self.set_flag(FLAG_I, true);
        self.pc = self.read_word(bus, 0xFFFA);

        7
    }

    fn execute(&mut self, bus: &mut impl Bus, opcode: u8) -> u32 {
        match opcode {
            // =====================================================================
            // Load/Store Operations
            // =====================================================================

            // LDA - Load Accumulator
            0xA9 => {
                // LDA #nn (Immediate)
                self.a = self.fetch(bus);
                self.set_zn(self.a);
                2
            }
            0xA5 => {
                // LDA $nn (Zero Page)
                let addr = self.addr_zero_page(bus);
                self.a = bus.read(addr as u32);
                self.set_zn(self.a);
                3
            }
            0xB5 => {
                // LDA $nn,X (Zero Page,X)
                let addr = self.addr_zero_page_x(bus);
                self.a = bus.read(addr as u32);
                self.set_zn(self.a);
                4
            }
            0xAD => {
                // LDA $nnnn (Absolute)
                let addr = self.addr_absolute(bus);
                self.a = bus.read(addr as u32);
                self.set_zn(self.a);
                4
            }
            0xBD => {
                // LDA $nnnn,X (Absolute,X)
                let (addr, page_crossed) = self.addr_absolute_x(bus);
                if page_crossed {
                    bus.tick(1);
                }
                self.a = bus.read(addr as u32);
                self.set_zn(self.a);
                4 + if page_crossed { 1 } else { 0 }
            }
            0xB9 => {
                // LDA $nnnn,Y (Absolute,Y)
                let (addr, page_crossed) = self.addr_absolute_y(bus);
                if page_crossed {
                    bus.tick(1);
                }
                self.a = bus.read(addr as u32);
                self.set_zn(self.a);
                4 + if page_crossed { 1 } else { 0 }
            }
            0xA1 => {
                // LDA ($nn,X) (Indexed Indirect)
                let addr = self.addr_indexed_indirect(bus);
                self.a = bus.read(addr as u32);
                self.set_zn(self.a);
                6
            }
            0xB1 => {
                // LDA ($nn),Y (Indirect Indexed)
                let (addr, page_crossed) = self.addr_indirect_indexed(bus);
                if page_crossed {
                    bus.tick(1);
                }
                self.a = bus.read(addr as u32);
                self.set_zn(self.a);
                5 + if page_crossed { 1 } else { 0 }
            }

            // LDX - Load X Register
            0xA2 => {
                // LDX #nn (Immediate)
                self.x = self.fetch(bus);
                self.set_zn(self.x);
                2
            }
            0xA6 => {
                // LDX $nn (Zero Page)
                let addr = self.addr_zero_page(bus);
                self.x = bus.read(addr as u32);
                self.set_zn(self.x);
                3
            }
            0xB6 => {
                // LDX $nn,Y (Zero Page,Y)
                let addr = self.addr_zero_page_y(bus);
                self.x = bus.read(addr as u32);
                self.set_zn(self.x);
                4
            }
            0xAE => {
                // LDX $nnnn (Absolute)
                let addr = self.addr_absolute(bus);
                self.x = bus.read(addr as u32);
                self.set_zn(self.x);
                4
            }
            0xBE => {
                // LDX $nnnn,Y (Absolute,Y)
                let (addr, page_crossed) = self.addr_absolute_y(bus);
                if page_crossed {
                    bus.tick(1);
                }
                self.x = bus.read(addr as u32);
                self.set_zn(self.x);
                4 + if page_crossed { 1 } else { 0 }
            }

            // LDY - Load Y Register
            0xA0 => {
                // LDY #nn (Immediate)
                self.y = self.fetch(bus);
                self.set_zn(self.y);
                2
            }
            0xA4 => {
                // LDY $nn (Zero Page)
                let addr = self.addr_zero_page(bus);
                self.y = bus.read(addr as u32);
                self.set_zn(self.y);
                3
            }
            0xB4 => {
                // LDY $nn,X (Zero Page,X)
                let addr = self.addr_zero_page_x(bus);
                self.y = bus.read(addr as u32);
                self.set_zn(self.y);
                4
            }
            0xAC => {
                // LDY $nnnn (Absolute)
                let addr = self.addr_absolute(bus);
                self.y = bus.read(addr as u32);
                self.set_zn(self.y);
                4
            }
            0xBC => {
                // LDY $nnnn,X (Absolute,X)
                let (addr, page_crossed) = self.addr_absolute_x(bus);
                if page_crossed {
                    bus.tick(1);
                }
                self.y = bus.read(addr as u32);
                self.set_zn(self.y);
                4 + if page_crossed { 1 } else { 0 }
            }

            // STA - Store Accumulator
            0x85 => {
                // STA $nn (Zero Page)
                let addr = self.addr_zero_page(bus);
                bus.write(addr as u32, self.a);
                3
            }
            0x95 => {
                // STA $nn,X (Zero Page,X)
                let addr = self.addr_zero_page_x(bus);
                bus.write(addr as u32, self.a);
                4
            }
            0x8D => {
                // STA $nnnn (Absolute)
                let addr = self.addr_absolute(bus);
                bus.write(addr as u32, self.a);
                4
            }
            0x9D => {
                // STA $nnnn,X (Absolute,X)
                let addr = self.addr_absolute_x_rmw(bus);
                bus.write(addr as u32, self.a);
                5
            }
            0x99 => {
                // STA $nnnn,Y (Absolute,Y)
                let addr = self.addr_absolute_y_rmw(bus);
                bus.write(addr as u32, self.a);
                5
            }
            0x81 => {
                // STA ($nn,X) (Indexed Indirect)
                let addr = self.addr_indexed_indirect(bus);
                bus.write(addr as u32, self.a);
                6
            }
            0x91 => {
                // STA ($nn),Y (Indirect Indexed)
                let addr = self.addr_indirect_indexed_rmw(bus);
                bus.write(addr as u32, self.a);
                6
            }

            // STX - Store X Register
            0x86 => {
                // STX $nn (Zero Page)
                let addr = self.addr_zero_page(bus);
                bus.write(addr as u32, self.x);
                3
            }
            0x96 => {
                // STX $nn,Y (Zero Page,Y)
                let addr = self.addr_zero_page_y(bus);
                bus.write(addr as u32, self.x);
                4
            }
            0x8E => {
                // STX $nnnn (Absolute)
                let addr = self.addr_absolute(bus);
                bus.write(addr as u32, self.x);
                4
            }

            // STY - Store Y Register
            0x84 => {
                // STY $nn (Zero Page)
                let addr = self.addr_zero_page(bus);
                bus.write(addr as u32, self.y);
                3
            }
            0x94 => {
                // STY $nn,X (Zero Page,X)
                let addr = self.addr_zero_page_x(bus);
                bus.write(addr as u32, self.y);
                4
            }
            0x8C => {
                // STY $nnnn (Absolute)
                let addr = self.addr_absolute(bus);
                bus.write(addr as u32, self.y);
                4
            }

            // =====================================================================
            // Register Transfers
            // =====================================================================
            0xAA => {
                // TAX (Transfer A to X)
                bus.tick(1);
                self.x = self.a;
                self.set_zn(self.x);
                2
            }
            0xA8 => {
                // TAY (Transfer A to Y)
                bus.tick(1);
                self.y = self.a;
                self.set_zn(self.y);
                2
            }
            0x8A => {
                // TXA (Transfer X to A)
                bus.tick(1);
                self.a = self.x;
                self.set_zn(self.a);
                2
            }
            0x98 => {
                // TYA (Transfer Y to A)
                bus.tick(1);
                self.a = self.y;
                self.set_zn(self.a);
                2
            }
            0xBA => {
                // TSX (Transfer SP to X)
                bus.tick(1);
                self.x = self.sp;
                self.set_zn(self.x);
                2
            }
            0x9A => {
                // TXS (Transfer X to SP)
                bus.tick(1);
                self.sp = self.x;
                2
            }

            // =====================================================================
            // Stack Operations
            // =====================================================================
            0x48 => {
                // PHA (Push A)
                bus.tick(1);
                self.push(bus, self.a);
                3
            }
            0x08 => {
                // PHP (Push Processor Status)
                bus.tick(1);
                self.push(bus, self.status_for_push(true)); // B flag set for PHP
                3
            }
            0x68 => {
                // PLA (Pull A)
                bus.tick(2);
                self.a = self.pull(bus);
                self.set_zn(self.a);
                4
            }
            0x28 => {
                // PLP (Pull Processor Status)
                bus.tick(2);
                let status = self.pull(bus);
                self.set_status_from_stack(status);
                4
            }

            // =====================================================================
            // Arithmetic Operations
            // =====================================================================

            // ADC - Add with Carry
            0x69 => {
                // ADC #nn (Immediate)
                let value = self.fetch(bus);
                self.adc(value);
                2
            }
            0x65 => {
                // ADC $nn (Zero Page)
                let addr = self.addr_zero_page(bus);
                let value = bus.read(addr as u32);
                self.adc(value);
                3
            }
            0x75 => {
                // ADC $nn,X (Zero Page,X)
                let addr = self.addr_zero_page_x(bus);
                let value = bus.read(addr as u32);
                self.adc(value);
                4
            }
            0x6D => {
                // ADC $nnnn (Absolute)
                let addr = self.addr_absolute(bus);
                let value = bus.read(addr as u32);
                self.adc(value);
                4
            }
            0x7D => {
                // ADC $nnnn,X (Absolute,X)
                let (addr, page_crossed) = self.addr_absolute_x(bus);
                if page_crossed {
                    bus.tick(1);
                }
                let value = bus.read(addr as u32);
                self.adc(value);
                4 + if page_crossed { 1 } else { 0 }
            }
            0x79 => {
                // ADC $nnnn,Y (Absolute,Y)
                let (addr, page_crossed) = self.addr_absolute_y(bus);
                if page_crossed {
                    bus.tick(1);
                }
                let value = bus.read(addr as u32);
                self.adc(value);
                4 + if page_crossed { 1 } else { 0 }
            }
            0x61 => {
                // ADC ($nn,X) (Indexed Indirect)
                let addr = self.addr_indexed_indirect(bus);
                let value = bus.read(addr as u32);
                self.adc(value);
                6
            }
            0x71 => {
                // ADC ($nn),Y (Indirect Indexed)
                let (addr, page_crossed) = self.addr_indirect_indexed(bus);
                if page_crossed {
                    bus.tick(1);
                }
                let value = bus.read(addr as u32);
                self.adc(value);
                5 + if page_crossed { 1 } else { 0 }
            }

            // SBC - Subtract with Carry
            0xE9 => {
                // SBC #nn (Immediate)
                let value = self.fetch(bus);
                self.sbc(value);
                2
            }
            0xE5 => {
                // SBC $nn (Zero Page)
                let addr = self.addr_zero_page(bus);
                let value = bus.read(addr as u32);
                self.sbc(value);
                3
            }
            0xF5 => {
                // SBC $nn,X (Zero Page,X)
                let addr = self.addr_zero_page_x(bus);
                let value = bus.read(addr as u32);
                self.sbc(value);
                4
            }
            0xED => {
                // SBC $nnnn (Absolute)
                let addr = self.addr_absolute(bus);
                let value = bus.read(addr as u32);
                self.sbc(value);
                4
            }
            0xFD => {
                // SBC $nnnn,X (Absolute,X)
                let (addr, page_crossed) = self.addr_absolute_x(bus);
                if page_crossed {
                    bus.tick(1);
                }
                let value = bus.read(addr as u32);
                self.sbc(value);
                4 + if page_crossed { 1 } else { 0 }
            }
            0xF9 => {
                // SBC $nnnn,Y (Absolute,Y)
                let (addr, page_crossed) = self.addr_absolute_y(bus);
                if page_crossed {
                    bus.tick(1);
                }
                let value = bus.read(addr as u32);
                self.sbc(value);
                4 + if page_crossed { 1 } else { 0 }
            }
            0xE1 => {
                // SBC ($nn,X) (Indexed Indirect)
                let addr = self.addr_indexed_indirect(bus);
                let value = bus.read(addr as u32);
                self.sbc(value);
                6
            }
            0xF1 => {
                // SBC ($nn),Y (Indirect Indexed)
                let (addr, page_crossed) = self.addr_indirect_indexed(bus);
                if page_crossed {
                    bus.tick(1);
                }
                let value = bus.read(addr as u32);
                self.sbc(value);
                5 + if page_crossed { 1 } else { 0 }
            }

            // =====================================================================
            // Compare Operations
            // =====================================================================

            // CMP - Compare Accumulator
            0xC9 => {
                // CMP #nn (Immediate)
                let value = self.fetch(bus);
                self.cmp(self.a, value);
                2
            }
            0xC5 => {
                // CMP $nn (Zero Page)
                let addr = self.addr_zero_page(bus);
                let value = bus.read(addr as u32);
                self.cmp(self.a, value);
                3
            }
            0xD5 => {
                // CMP $nn,X (Zero Page,X)
                let addr = self.addr_zero_page_x(bus);
                let value = bus.read(addr as u32);
                self.cmp(self.a, value);
                4
            }
            0xCD => {
                // CMP $nnnn (Absolute)
                let addr = self.addr_absolute(bus);
                let value = bus.read(addr as u32);
                self.cmp(self.a, value);
                4
            }
            0xDD => {
                // CMP $nnnn,X (Absolute,X)
                let (addr, page_crossed) = self.addr_absolute_x(bus);
                if page_crossed {
                    bus.tick(1);
                }
                let value = bus.read(addr as u32);
                self.cmp(self.a, value);
                4 + if page_crossed { 1 } else { 0 }
            }
            0xD9 => {
                // CMP $nnnn,Y (Absolute,Y)
                let (addr, page_crossed) = self.addr_absolute_y(bus);
                if page_crossed {
                    bus.tick(1);
                }
                let value = bus.read(addr as u32);
                self.cmp(self.a, value);
                4 + if page_crossed { 1 } else { 0 }
            }
            0xC1 => {
                // CMP ($nn,X) (Indexed Indirect)
                let addr = self.addr_indexed_indirect(bus);
                let value = bus.read(addr as u32);
                self.cmp(self.a, value);
                6
            }
            0xD1 => {
                // CMP ($nn),Y (Indirect Indexed)
                let (addr, page_crossed) = self.addr_indirect_indexed(bus);
                if page_crossed {
                    bus.tick(1);
                }
                let value = bus.read(addr as u32);
                self.cmp(self.a, value);
                5 + if page_crossed { 1 } else { 0 }
            }

            // CPX - Compare X Register
            0xE0 => {
                // CPX #nn (Immediate)
                let value = self.fetch(bus);
                self.cmp(self.x, value);
                2
            }
            0xE4 => {
                // CPX $nn (Zero Page)
                let addr = self.addr_zero_page(bus);
                let value = bus.read(addr as u32);
                self.cmp(self.x, value);
                3
            }
            0xEC => {
                // CPX $nnnn (Absolute)
                let addr = self.addr_absolute(bus);
                let value = bus.read(addr as u32);
                self.cmp(self.x, value);
                4
            }

            // CPY - Compare Y Register
            0xC0 => {
                // CPY #nn (Immediate)
                let value = self.fetch(bus);
                self.cmp(self.y, value);
                2
            }
            0xC4 => {
                // CPY $nn (Zero Page)
                let addr = self.addr_zero_page(bus);
                let value = bus.read(addr as u32);
                self.cmp(self.y, value);
                3
            }
            0xCC => {
                // CPY $nnnn (Absolute)
                let addr = self.addr_absolute(bus);
                let value = bus.read(addr as u32);
                self.cmp(self.y, value);
                4
            }

            // =====================================================================
            // Increment/Decrement Operations
            // =====================================================================

            // INC - Increment Memory
            0xE6 => {
                // INC $nn (Zero Page)
                let addr = self.addr_zero_page(bus);
                let value = bus.read(addr as u32);
                bus.tick(1);
                let result = value.wrapping_add(1);
                self.set_zn(result);
                bus.write(addr as u32, result);
                5
            }
            0xF6 => {
                // INC $nn,X (Zero Page,X)
                let addr = self.addr_zero_page_x(bus);
                let value = bus.read(addr as u32);
                bus.tick(1);
                let result = value.wrapping_add(1);
                self.set_zn(result);
                bus.write(addr as u32, result);
                6
            }
            0xEE => {
                // INC $nnnn (Absolute)
                let addr = self.addr_absolute(bus);
                let value = bus.read(addr as u32);
                bus.tick(1);
                let result = value.wrapping_add(1);
                self.set_zn(result);
                bus.write(addr as u32, result);
                6
            }
            0xFE => {
                // INC $nnnn,X (Absolute,X)
                let addr = self.addr_absolute_x_rmw(bus);
                let value = bus.read(addr as u32);
                bus.tick(1);
                let result = value.wrapping_add(1);
                self.set_zn(result);
                bus.write(addr as u32, result);
                7
            }

            // INX - Increment X
            0xE8 => {
                bus.tick(1);
                self.x = self.x.wrapping_add(1);
                self.set_zn(self.x);
                2
            }

            // INY - Increment Y
            0xC8 => {
                bus.tick(1);
                self.y = self.y.wrapping_add(1);
                self.set_zn(self.y);
                2
            }

            // DEC - Decrement Memory
            0xC6 => {
                // DEC $nn (Zero Page)
                let addr = self.addr_zero_page(bus);
                let value = bus.read(addr as u32);
                bus.tick(1);
                let result = value.wrapping_sub(1);
                self.set_zn(result);
                bus.write(addr as u32, result);
                5
            }
            0xD6 => {
                // DEC $nn,X (Zero Page,X)
                let addr = self.addr_zero_page_x(bus);
                let value = bus.read(addr as u32);
                bus.tick(1);
                let result = value.wrapping_sub(1);
                self.set_zn(result);
                bus.write(addr as u32, result);
                6
            }
            0xCE => {
                // DEC $nnnn (Absolute)
                let addr = self.addr_absolute(bus);
                let value = bus.read(addr as u32);
                bus.tick(1);
                let result = value.wrapping_sub(1);
                self.set_zn(result);
                bus.write(addr as u32, result);
                6
            }
            0xDE => {
                // DEC $nnnn,X (Absolute,X)
                let addr = self.addr_absolute_x_rmw(bus);
                let value = bus.read(addr as u32);
                bus.tick(1);
                let result = value.wrapping_sub(1);
                self.set_zn(result);
                bus.write(addr as u32, result);
                7
            }

            // DEX - Decrement X
            0xCA => {
                bus.tick(1);
                self.x = self.x.wrapping_sub(1);
                self.set_zn(self.x);
                2
            }

            // DEY - Decrement Y
            0x88 => {
                bus.tick(1);
                self.y = self.y.wrapping_sub(1);
                self.set_zn(self.y);
                2
            }

            // =====================================================================
            // Logical Operations
            // =====================================================================

            // AND - Logical AND
            0x29 => {
                // AND #nn (Immediate)
                let value = self.fetch(bus);
                self.a &= value;
                self.set_zn(self.a);
                2
            }
            0x25 => {
                // AND $nn (Zero Page)
                let addr = self.addr_zero_page(bus);
                let value = bus.read(addr as u32);
                self.a &= value;
                self.set_zn(self.a);
                3
            }
            0x35 => {
                // AND $nn,X (Zero Page,X)
                let addr = self.addr_zero_page_x(bus);
                let value = bus.read(addr as u32);
                self.a &= value;
                self.set_zn(self.a);
                4
            }
            0x2D => {
                // AND $nnnn (Absolute)
                let addr = self.addr_absolute(bus);
                let value = bus.read(addr as u32);
                self.a &= value;
                self.set_zn(self.a);
                4
            }
            0x3D => {
                // AND $nnnn,X (Absolute,X)
                let (addr, page_crossed) = self.addr_absolute_x(bus);
                if page_crossed {
                    bus.tick(1);
                }
                let value = bus.read(addr as u32);
                self.a &= value;
                self.set_zn(self.a);
                4 + if page_crossed { 1 } else { 0 }
            }
            0x39 => {
                // AND $nnnn,Y (Absolute,Y)
                let (addr, page_crossed) = self.addr_absolute_y(bus);
                if page_crossed {
                    bus.tick(1);
                }
                let value = bus.read(addr as u32);
                self.a &= value;
                self.set_zn(self.a);
                4 + if page_crossed { 1 } else { 0 }
            }
            0x21 => {
                // AND ($nn,X) (Indexed Indirect)
                let addr = self.addr_indexed_indirect(bus);
                let value = bus.read(addr as u32);
                self.a &= value;
                self.set_zn(self.a);
                6
            }
            0x31 => {
                // AND ($nn),Y (Indirect Indexed)
                let (addr, page_crossed) = self.addr_indirect_indexed(bus);
                if page_crossed {
                    bus.tick(1);
                }
                let value = bus.read(addr as u32);
                self.a &= value;
                self.set_zn(self.a);
                5 + if page_crossed { 1 } else { 0 }
            }

            // EOR - Exclusive OR
            0x49 => {
                // EOR #nn (Immediate)
                let value = self.fetch(bus);
                self.a ^= value;
                self.set_zn(self.a);
                2
            }
            0x45 => {
                // EOR $nn (Zero Page)
                let addr = self.addr_zero_page(bus);
                let value = bus.read(addr as u32);
                self.a ^= value;
                self.set_zn(self.a);
                3
            }
            0x55 => {
                // EOR $nn,X (Zero Page,X)
                let addr = self.addr_zero_page_x(bus);
                let value = bus.read(addr as u32);
                self.a ^= value;
                self.set_zn(self.a);
                4
            }
            0x4D => {
                // EOR $nnnn (Absolute)
                let addr = self.addr_absolute(bus);
                let value = bus.read(addr as u32);
                self.a ^= value;
                self.set_zn(self.a);
                4
            }
            0x5D => {
                // EOR $nnnn,X (Absolute,X)
                let (addr, page_crossed) = self.addr_absolute_x(bus);
                if page_crossed {
                    bus.tick(1);
                }
                let value = bus.read(addr as u32);
                self.a ^= value;
                self.set_zn(self.a);
                4 + if page_crossed { 1 } else { 0 }
            }
            0x59 => {
                // EOR $nnnn,Y (Absolute,Y)
                let (addr, page_crossed) = self.addr_absolute_y(bus);
                if page_crossed {
                    bus.tick(1);
                }
                let value = bus.read(addr as u32);
                self.a ^= value;
                self.set_zn(self.a);
                4 + if page_crossed { 1 } else { 0 }
            }
            0x41 => {
                // EOR ($nn,X) (Indexed Indirect)
                let addr = self.addr_indexed_indirect(bus);
                let value = bus.read(addr as u32);
                self.a ^= value;
                self.set_zn(self.a);
                6
            }
            0x51 => {
                // EOR ($nn),Y (Indirect Indexed)
                let (addr, page_crossed) = self.addr_indirect_indexed(bus);
                if page_crossed {
                    bus.tick(1);
                }
                let value = bus.read(addr as u32);
                self.a ^= value;
                self.set_zn(self.a);
                5 + if page_crossed { 1 } else { 0 }
            }

            // ORA - Logical OR
            0x09 => {
                // ORA #nn (Immediate)
                let value = self.fetch(bus);
                self.a |= value;
                self.set_zn(self.a);
                2
            }
            0x05 => {
                // ORA $nn (Zero Page)
                let addr = self.addr_zero_page(bus);
                let value = bus.read(addr as u32);
                self.a |= value;
                self.set_zn(self.a);
                3
            }
            0x15 => {
                // ORA $nn,X (Zero Page,X)
                let addr = self.addr_zero_page_x(bus);
                let value = bus.read(addr as u32);
                self.a |= value;
                self.set_zn(self.a);
                4
            }
            0x0D => {
                // ORA $nnnn (Absolute)
                let addr = self.addr_absolute(bus);
                let value = bus.read(addr as u32);
                self.a |= value;
                self.set_zn(self.a);
                4
            }
            0x1D => {
                // ORA $nnnn,X (Absolute,X)
                let (addr, page_crossed) = self.addr_absolute_x(bus);
                if page_crossed {
                    bus.tick(1);
                }
                let value = bus.read(addr as u32);
                self.a |= value;
                self.set_zn(self.a);
                4 + if page_crossed { 1 } else { 0 }
            }
            0x19 => {
                // ORA $nnnn,Y (Absolute,Y)
                let (addr, page_crossed) = self.addr_absolute_y(bus);
                if page_crossed {
                    bus.tick(1);
                }
                let value = bus.read(addr as u32);
                self.a |= value;
                self.set_zn(self.a);
                4 + if page_crossed { 1 } else { 0 }
            }
            0x01 => {
                // ORA ($nn,X) (Indexed Indirect)
                let addr = self.addr_indexed_indirect(bus);
                let value = bus.read(addr as u32);
                self.a |= value;
                self.set_zn(self.a);
                6
            }
            0x11 => {
                // ORA ($nn),Y (Indirect Indexed)
                let (addr, page_crossed) = self.addr_indirect_indexed(bus);
                if page_crossed {
                    bus.tick(1);
                }
                let value = bus.read(addr as u32);
                self.a |= value;
                self.set_zn(self.a);
                5 + if page_crossed { 1 } else { 0 }
            }

            // BIT - Bit Test
            0x24 => {
                // BIT $nn (Zero Page)
                let addr = self.addr_zero_page(bus);
                let value = bus.read(addr as u32);
                self.bit(value);
                3
            }
            0x2C => {
                // BIT $nnnn (Absolute)
                let addr = self.addr_absolute(bus);
                let value = bus.read(addr as u32);
                self.bit(value);
                4
            }

            // =====================================================================
            // Shift/Rotate Operations
            // =====================================================================

            // ASL - Arithmetic Shift Left
            0x0A => {
                // ASL A (Accumulator)
                bus.tick(1);
                self.a = self.asl(self.a);
                2
            }
            0x06 => {
                // ASL $nn (Zero Page)
                let addr = self.addr_zero_page(bus);
                let value = bus.read(addr as u32);
                bus.tick(1);
                let result = self.asl(value);
                bus.write(addr as u32, result);
                5
            }
            0x16 => {
                // ASL $nn,X (Zero Page,X)
                let addr = self.addr_zero_page_x(bus);
                let value = bus.read(addr as u32);
                bus.tick(1);
                let result = self.asl(value);
                bus.write(addr as u32, result);
                6
            }
            0x0E => {
                // ASL $nnnn (Absolute)
                let addr = self.addr_absolute(bus);
                let value = bus.read(addr as u32);
                bus.tick(1);
                let result = self.asl(value);
                bus.write(addr as u32, result);
                6
            }
            0x1E => {
                // ASL $nnnn,X (Absolute,X)
                let addr = self.addr_absolute_x_rmw(bus);
                let value = bus.read(addr as u32);
                bus.tick(1);
                let result = self.asl(value);
                bus.write(addr as u32, result);
                7
            }

            // LSR - Logical Shift Right
            0x4A => {
                // LSR A (Accumulator)
                bus.tick(1);
                self.a = self.lsr(self.a);
                2
            }
            0x46 => {
                // LSR $nn (Zero Page)
                let addr = self.addr_zero_page(bus);
                let value = bus.read(addr as u32);
                bus.tick(1);
                let result = self.lsr(value);
                bus.write(addr as u32, result);
                5
            }
            0x56 => {
                // LSR $nn,X (Zero Page,X)
                let addr = self.addr_zero_page_x(bus);
                let value = bus.read(addr as u32);
                bus.tick(1);
                let result = self.lsr(value);
                bus.write(addr as u32, result);
                6
            }
            0x4E => {
                // LSR $nnnn (Absolute)
                let addr = self.addr_absolute(bus);
                let value = bus.read(addr as u32);
                bus.tick(1);
                let result = self.lsr(value);
                bus.write(addr as u32, result);
                6
            }
            0x5E => {
                // LSR $nnnn,X (Absolute,X)
                let addr = self.addr_absolute_x_rmw(bus);
                let value = bus.read(addr as u32);
                bus.tick(1);
                let result = self.lsr(value);
                bus.write(addr as u32, result);
                7
            }

            // ROL - Rotate Left
            0x2A => {
                // ROL A (Accumulator)
                bus.tick(1);
                self.a = self.rol(self.a);
                2
            }
            0x26 => {
                // ROL $nn (Zero Page)
                let addr = self.addr_zero_page(bus);
                let value = bus.read(addr as u32);
                bus.tick(1);
                let result = self.rol(value);
                bus.write(addr as u32, result);
                5
            }
            0x36 => {
                // ROL $nn,X (Zero Page,X)
                let addr = self.addr_zero_page_x(bus);
                let value = bus.read(addr as u32);
                bus.tick(1);
                let result = self.rol(value);
                bus.write(addr as u32, result);
                6
            }
            0x2E => {
                // ROL $nnnn (Absolute)
                let addr = self.addr_absolute(bus);
                let value = bus.read(addr as u32);
                bus.tick(1);
                let result = self.rol(value);
                bus.write(addr as u32, result);
                6
            }
            0x3E => {
                // ROL $nnnn,X (Absolute,X)
                let addr = self.addr_absolute_x_rmw(bus);
                let value = bus.read(addr as u32);
                bus.tick(1);
                let result = self.rol(value);
                bus.write(addr as u32, result);
                7
            }

            // ROR - Rotate Right
            0x6A => {
                // ROR A (Accumulator)
                bus.tick(1);
                self.a = self.ror(self.a);
                2
            }
            0x66 => {
                // ROR $nn (Zero Page)
                let addr = self.addr_zero_page(bus);
                let value = bus.read(addr as u32);
                bus.tick(1);
                let result = self.ror(value);
                bus.write(addr as u32, result);
                5
            }
            0x76 => {
                // ROR $nn,X (Zero Page,X)
                let addr = self.addr_zero_page_x(bus);
                let value = bus.read(addr as u32);
                bus.tick(1);
                let result = self.ror(value);
                bus.write(addr as u32, result);
                6
            }
            0x6E => {
                // ROR $nnnn (Absolute)
                let addr = self.addr_absolute(bus);
                let value = bus.read(addr as u32);
                bus.tick(1);
                let result = self.ror(value);
                bus.write(addr as u32, result);
                6
            }
            0x7E => {
                // ROR $nnnn,X (Absolute,X)
                let addr = self.addr_absolute_x_rmw(bus);
                let value = bus.read(addr as u32);
                bus.tick(1);
                let result = self.ror(value);
                bus.write(addr as u32, result);
                7
            }

            // =====================================================================
            // Jump/Call Operations
            // =====================================================================

            // JMP - Jump
            0x4C => {
                // JMP $nnnn (Absolute)
                self.pc = self.fetch_word(bus);
                3
            }
            0x6C => {
                // JMP ($nnnn) (Indirect)
                let addr = self.fetch_word(bus);
                self.pc = self.read_word_page_bug(bus, addr);
                5
            }

            // JSR - Jump to Subroutine
            0x20 => {
                // JSR $nnnn
                let low = self.fetch(bus);
                bus.tick(1); // Internal operation
                // Push PC-1 (address of last byte of JSR)
                self.push_word(bus, self.pc);
                let high = self.fetch(bus);
                self.pc = u16::from_le_bytes([low, high]);
                6
            }

            // RTS - Return from Subroutine
            0x60 => {
                bus.tick(2); // Internal operations
                self.pc = self.pull_word(bus);
                self.pc = self.pc.wrapping_add(1);
                bus.tick(1);
                6
            }

            // RTI - Return from Interrupt
            0x40 => {
                bus.tick(2); // Internal operations
                let status = self.pull(bus);
                self.set_status_from_stack(status);
                self.pc = self.pull_word(bus);
                6
            }

            // =====================================================================
            // Branch Operations
            // =====================================================================
            0x10 => {
                // BPL - Branch if Plus (N = 0)
                2 + self.branch_if(bus, !self.negative())
            }
            0x30 => {
                // BMI - Branch if Minus (N = 1)
                2 + self.branch_if(bus, self.negative())
            }
            0x50 => {
                // BVC - Branch if Overflow Clear (V = 0)
                2 + self.branch_if(bus, !self.overflow())
            }
            0x70 => {
                // BVS - Branch if Overflow Set (V = 1)
                2 + self.branch_if(bus, self.overflow())
            }
            0x90 => {
                // BCC - Branch if Carry Clear (C = 0)
                2 + self.branch_if(bus, !self.carry())
            }
            0xB0 => {
                // BCS - Branch if Carry Set (C = 1)
                2 + self.branch_if(bus, self.carry())
            }
            0xD0 => {
                // BNE - Branch if Not Equal (Z = 0)
                2 + self.branch_if(bus, !self.zero())
            }
            0xF0 => {
                // BEQ - Branch if Equal (Z = 1)
                2 + self.branch_if(bus, self.zero())
            }

            // =====================================================================
            // Status Flag Operations
            // =====================================================================
            0x18 => {
                // CLC - Clear Carry
                bus.tick(1);
                self.set_flag(FLAG_C, false);
                2
            }
            0x38 => {
                // SEC - Set Carry
                bus.tick(1);
                self.set_flag(FLAG_C, true);
                2
            }
            0x58 => {
                // CLI - Clear Interrupt Disable
                bus.tick(1);
                self.set_flag(FLAG_I, false);
                2
            }
            0x78 => {
                // SEI - Set Interrupt Disable
                bus.tick(1);
                self.set_flag(FLAG_I, true);
                2
            }
            0xD8 => {
                // CLD - Clear Decimal Mode
                bus.tick(1);
                self.set_flag(FLAG_D, false);
                2
            }
            0xF8 => {
                // SED - Set Decimal Mode
                bus.tick(1);
                self.set_flag(FLAG_D, true);
                2
            }
            0xB8 => {
                // CLV - Clear Overflow
                bus.tick(1);
                self.set_flag(FLAG_V, false);
                2
            }

            // =====================================================================
            // System Operations
            // =====================================================================
            0x00 => {
                // BRK - Software Interrupt
                self.fetch(bus); // Padding byte (ignored but fetched)
                self.push_word(bus, self.pc);
                self.push(bus, self.status_for_push(true)); // B flag set for BRK
                self.set_flag(FLAG_I, true);
                self.pc = self.read_word(bus, 0xFFFE);
                7
            }

            0xEA => {
                // NOP - No Operation
                bus.tick(1);
                2
            }

            // =====================================================================
            // Undocumented NOPs (for compatibility)
            // =====================================================================

            // 1-byte NOPs
            0x1A | 0x3A | 0x5A | 0x7A | 0xDA | 0xFA => {
                bus.tick(1);
                2
            }

            // 2-byte NOPs (skip one byte)
            0x80 | 0x82 | 0x89 | 0xC2 | 0xE2 => {
                self.fetch(bus);
                2
            }

            // DOP (Double NOP) - Zero Page
            0x04 | 0x44 | 0x64 => {
                self.fetch(bus);
                bus.tick(1);
                3
            }

            // DOP - Zero Page,X
            0x14 | 0x34 | 0x54 | 0x74 | 0xD4 | 0xF4 => {
                let base = self.fetch(bus);
                bus.read(base as u32);
                bus.tick(1);
                4
            }

            // TOP (Triple NOP) - Absolute
            0x0C => {
                self.fetch_word(bus);
                4
            }

            // TOP - Absolute,X
            0x1C | 0x3C | 0x5C | 0x7C | 0xDC | 0xFC => {
                let (_, page_crossed) = self.addr_absolute_x(bus);
                if page_crossed {
                    bus.tick(1);
                }
                4 + if page_crossed { 1 } else { 0 }
            }

            // =====================================================================
            // Illegal Opcodes (Undocumented but commonly used)
            // =====================================================================

            // LAX - Load A and X (LDA + LDX combined)
            0xA7 => {
                // LAX $nn (Zero Page)
                let addr = self.addr_zero_page(bus);
                let value = bus.read(addr as u32);
                self.a = value;
                self.x = value;
                self.set_zn(value);
                3
            }
            0xB7 => {
                // LAX $nn,Y (Zero Page,Y)
                let addr = self.addr_zero_page_y(bus);
                let value = bus.read(addr as u32);
                self.a = value;
                self.x = value;
                self.set_zn(value);
                4
            }
            0xAF => {
                // LAX $nnnn (Absolute)
                let addr = self.addr_absolute(bus);
                let value = bus.read(addr as u32);
                self.a = value;
                self.x = value;
                self.set_zn(value);
                4
            }
            0xBF => {
                // LAX $nnnn,Y (Absolute,Y)
                let (addr, page_crossed) = self.addr_absolute_y(bus);
                if page_crossed {
                    bus.tick(1);
                }
                let value = bus.read(addr as u32);
                self.a = value;
                self.x = value;
                self.set_zn(value);
                4 + if page_crossed { 1 } else { 0 }
            }
            0xA3 => {
                // LAX ($nn,X) (Indexed Indirect)
                let addr = self.addr_indexed_indirect(bus);
                let value = bus.read(addr as u32);
                self.a = value;
                self.x = value;
                self.set_zn(value);
                6
            }
            0xB3 => {
                // LAX ($nn),Y (Indirect Indexed)
                let (addr, page_crossed) = self.addr_indirect_indexed(bus);
                if page_crossed {
                    bus.tick(1);
                }
                let value = bus.read(addr as u32);
                self.a = value;
                self.x = value;
                self.set_zn(value);
                5 + if page_crossed { 1 } else { 0 }
            }

            // SAX - Store A AND X
            0x87 => {
                // SAX $nn (Zero Page)
                let addr = self.addr_zero_page(bus);
                bus.write(addr as u32, self.a & self.x);
                3
            }
            0x97 => {
                // SAX $nn,Y (Zero Page,Y)
                let addr = self.addr_zero_page_y(bus);
                bus.write(addr as u32, self.a & self.x);
                4
            }
            0x8F => {
                // SAX $nnnn (Absolute)
                let addr = self.addr_absolute(bus);
                bus.write(addr as u32, self.a & self.x);
                4
            }
            0x83 => {
                // SAX ($nn,X) (Indexed Indirect)
                let addr = self.addr_indexed_indirect(bus);
                bus.write(addr as u32, self.a & self.x);
                6
            }

            // DCP - Decrement memory then Compare (DEC + CMP)
            0xC7 => {
                // DCP $nn (Zero Page)
                let addr = self.addr_zero_page(bus);
                let value = bus.read(addr as u32);
                bus.tick(1);
                let result = value.wrapping_sub(1);
                bus.write(addr as u32, result);
                self.cmp(self.a, result);
                5
            }
            0xD7 => {
                // DCP $nn,X (Zero Page,X)
                let addr = self.addr_zero_page_x(bus);
                let value = bus.read(addr as u32);
                bus.tick(1);
                let result = value.wrapping_sub(1);
                bus.write(addr as u32, result);
                self.cmp(self.a, result);
                6
            }
            0xCF => {
                // DCP $nnnn (Absolute)
                let addr = self.addr_absolute(bus);
                let value = bus.read(addr as u32);
                bus.tick(1);
                let result = value.wrapping_sub(1);
                bus.write(addr as u32, result);
                self.cmp(self.a, result);
                6
            }
            0xDF => {
                // DCP $nnnn,X (Absolute,X)
                let addr = self.addr_absolute_x_rmw(bus);
                let value = bus.read(addr as u32);
                bus.tick(1);
                let result = value.wrapping_sub(1);
                bus.write(addr as u32, result);
                self.cmp(self.a, result);
                7
            }
            0xDB => {
                // DCP $nnnn,Y (Absolute,Y)
                let addr = self.addr_absolute_y_rmw(bus);
                let value = bus.read(addr as u32);
                bus.tick(1);
                let result = value.wrapping_sub(1);
                bus.write(addr as u32, result);
                self.cmp(self.a, result);
                7
            }
            0xC3 => {
                // DCP ($nn,X) (Indexed Indirect)
                let addr = self.addr_indexed_indirect(bus);
                let value = bus.read(addr as u32);
                bus.tick(1);
                let result = value.wrapping_sub(1);
                bus.write(addr as u32, result);
                self.cmp(self.a, result);
                8
            }
            0xD3 => {
                // DCP ($nn),Y (Indirect Indexed)
                let addr = self.addr_indirect_indexed_rmw(bus);
                let value = bus.read(addr as u32);
                bus.tick(1);
                let result = value.wrapping_sub(1);
                bus.write(addr as u32, result);
                self.cmp(self.a, result);
                8
            }

            // ISC/ISB - Increment memory then Subtract (INC + SBC)
            0xE7 => {
                // ISC $nn (Zero Page)
                let addr = self.addr_zero_page(bus);
                let value = bus.read(addr as u32);
                bus.tick(1);
                let result = value.wrapping_add(1);
                bus.write(addr as u32, result);
                self.sbc(result);
                5
            }
            0xF7 => {
                // ISC $nn,X (Zero Page,X)
                let addr = self.addr_zero_page_x(bus);
                let value = bus.read(addr as u32);
                bus.tick(1);
                let result = value.wrapping_add(1);
                bus.write(addr as u32, result);
                self.sbc(result);
                6
            }
            0xEF => {
                // ISC $nnnn (Absolute)
                let addr = self.addr_absolute(bus);
                let value = bus.read(addr as u32);
                bus.tick(1);
                let result = value.wrapping_add(1);
                bus.write(addr as u32, result);
                self.sbc(result);
                6
            }
            0xFF => {
                // ISC $nnnn,X (Absolute,X)
                let addr = self.addr_absolute_x_rmw(bus);
                let value = bus.read(addr as u32);
                bus.tick(1);
                let result = value.wrapping_add(1);
                bus.write(addr as u32, result);
                self.sbc(result);
                7
            }
            0xFB => {
                // ISC $nnnn,Y (Absolute,Y)
                let addr = self.addr_absolute_y_rmw(bus);
                let value = bus.read(addr as u32);
                bus.tick(1);
                let result = value.wrapping_add(1);
                bus.write(addr as u32, result);
                self.sbc(result);
                7
            }
            0xE3 => {
                // ISC ($nn,X) (Indexed Indirect)
                let addr = self.addr_indexed_indirect(bus);
                let value = bus.read(addr as u32);
                bus.tick(1);
                let result = value.wrapping_add(1);
                bus.write(addr as u32, result);
                self.sbc(result);
                8
            }
            0xF3 => {
                // ISC ($nn),Y (Indirect Indexed)
                let addr = self.addr_indirect_indexed_rmw(bus);
                let value = bus.read(addr as u32);
                bus.tick(1);
                let result = value.wrapping_add(1);
                bus.write(addr as u32, result);
                self.sbc(result);
                8
            }

            // SLO - Shift Left then OR (ASL + ORA)
            0x07 => {
                // SLO $nn (Zero Page)
                let addr = self.addr_zero_page(bus);
                let value = bus.read(addr as u32);
                bus.tick(1);
                let result = self.asl(value);
                bus.write(addr as u32, result);
                self.a |= result;
                self.set_zn(self.a);
                5
            }
            0x17 => {
                // SLO $nn,X (Zero Page,X)
                let addr = self.addr_zero_page_x(bus);
                let value = bus.read(addr as u32);
                bus.tick(1);
                let result = self.asl(value);
                bus.write(addr as u32, result);
                self.a |= result;
                self.set_zn(self.a);
                6
            }
            0x0F => {
                // SLO $nnnn (Absolute)
                let addr = self.addr_absolute(bus);
                let value = bus.read(addr as u32);
                bus.tick(1);
                let result = self.asl(value);
                bus.write(addr as u32, result);
                self.a |= result;
                self.set_zn(self.a);
                6
            }
            0x1F => {
                // SLO $nnnn,X (Absolute,X)
                let addr = self.addr_absolute_x_rmw(bus);
                let value = bus.read(addr as u32);
                bus.tick(1);
                let result = self.asl(value);
                bus.write(addr as u32, result);
                self.a |= result;
                self.set_zn(self.a);
                7
            }
            0x1B => {
                // SLO $nnnn,Y (Absolute,Y)
                let addr = self.addr_absolute_y_rmw(bus);
                let value = bus.read(addr as u32);
                bus.tick(1);
                let result = self.asl(value);
                bus.write(addr as u32, result);
                self.a |= result;
                self.set_zn(self.a);
                7
            }
            0x03 => {
                // SLO ($nn,X) (Indexed Indirect)
                let addr = self.addr_indexed_indirect(bus);
                let value = bus.read(addr as u32);
                bus.tick(1);
                let result = self.asl(value);
                bus.write(addr as u32, result);
                self.a |= result;
                self.set_zn(self.a);
                8
            }
            0x13 => {
                // SLO ($nn),Y (Indirect Indexed)
                let addr = self.addr_indirect_indexed_rmw(bus);
                let value = bus.read(addr as u32);
                bus.tick(1);
                let result = self.asl(value);
                bus.write(addr as u32, result);
                self.a |= result;
                self.set_zn(self.a);
                8
            }

            // SRE - Shift Right then Exclusive OR (LSR + EOR)
            0x47 => {
                // SRE $nn (Zero Page)
                let addr = self.addr_zero_page(bus);
                let value = bus.read(addr as u32);
                bus.tick(1);
                let result = self.lsr(value);
                bus.write(addr as u32, result);
                self.a ^= result;
                self.set_zn(self.a);
                5
            }
            0x57 => {
                // SRE $nn,X (Zero Page,X)
                let addr = self.addr_zero_page_x(bus);
                let value = bus.read(addr as u32);
                bus.tick(1);
                let result = self.lsr(value);
                bus.write(addr as u32, result);
                self.a ^= result;
                self.set_zn(self.a);
                6
            }
            0x4F => {
                // SRE $nnnn (Absolute)
                let addr = self.addr_absolute(bus);
                let value = bus.read(addr as u32);
                bus.tick(1);
                let result = self.lsr(value);
                bus.write(addr as u32, result);
                self.a ^= result;
                self.set_zn(self.a);
                6
            }
            0x5F => {
                // SRE $nnnn,X (Absolute,X)
                let addr = self.addr_absolute_x_rmw(bus);
                let value = bus.read(addr as u32);
                bus.tick(1);
                let result = self.lsr(value);
                bus.write(addr as u32, result);
                self.a ^= result;
                self.set_zn(self.a);
                7
            }
            0x5B => {
                // SRE $nnnn,Y (Absolute,Y)
                let addr = self.addr_absolute_y_rmw(bus);
                let value = bus.read(addr as u32);
                bus.tick(1);
                let result = self.lsr(value);
                bus.write(addr as u32, result);
                self.a ^= result;
                self.set_zn(self.a);
                7
            }
            0x43 => {
                // SRE ($nn,X) (Indexed Indirect)
                let addr = self.addr_indexed_indirect(bus);
                let value = bus.read(addr as u32);
                bus.tick(1);
                let result = self.lsr(value);
                bus.write(addr as u32, result);
                self.a ^= result;
                self.set_zn(self.a);
                8
            }
            0x53 => {
                // SRE ($nn),Y (Indirect Indexed)
                let addr = self.addr_indirect_indexed_rmw(bus);
                let value = bus.read(addr as u32);
                bus.tick(1);
                let result = self.lsr(value);
                bus.write(addr as u32, result);
                self.a ^= result;
                self.set_zn(self.a);
                8
            }

            // RLA - Rotate Left then AND (ROL + AND)
            0x27 => {
                // RLA $nn (Zero Page)
                let addr = self.addr_zero_page(bus);
                let value = bus.read(addr as u32);
                bus.tick(1);
                let result = self.rol(value);
                bus.write(addr as u32, result);
                self.a &= result;
                self.set_zn(self.a);
                5
            }
            0x37 => {
                // RLA $nn,X (Zero Page,X)
                let addr = self.addr_zero_page_x(bus);
                let value = bus.read(addr as u32);
                bus.tick(1);
                let result = self.rol(value);
                bus.write(addr as u32, result);
                self.a &= result;
                self.set_zn(self.a);
                6
            }
            0x2F => {
                // RLA $nnnn (Absolute)
                let addr = self.addr_absolute(bus);
                let value = bus.read(addr as u32);
                bus.tick(1);
                let result = self.rol(value);
                bus.write(addr as u32, result);
                self.a &= result;
                self.set_zn(self.a);
                6
            }
            0x3F => {
                // RLA $nnnn,X (Absolute,X)
                let addr = self.addr_absolute_x_rmw(bus);
                let value = bus.read(addr as u32);
                bus.tick(1);
                let result = self.rol(value);
                bus.write(addr as u32, result);
                self.a &= result;
                self.set_zn(self.a);
                7
            }
            0x3B => {
                // RLA $nnnn,Y (Absolute,Y)
                let addr = self.addr_absolute_y_rmw(bus);
                let value = bus.read(addr as u32);
                bus.tick(1);
                let result = self.rol(value);
                bus.write(addr as u32, result);
                self.a &= result;
                self.set_zn(self.a);
                7
            }
            0x23 => {
                // RLA ($nn,X) (Indexed Indirect)
                let addr = self.addr_indexed_indirect(bus);
                let value = bus.read(addr as u32);
                bus.tick(1);
                let result = self.rol(value);
                bus.write(addr as u32, result);
                self.a &= result;
                self.set_zn(self.a);
                8
            }
            0x33 => {
                // RLA ($nn),Y (Indirect Indexed)
                let addr = self.addr_indirect_indexed_rmw(bus);
                let value = bus.read(addr as u32);
                bus.tick(1);
                let result = self.rol(value);
                bus.write(addr as u32, result);
                self.a &= result;
                self.set_zn(self.a);
                8
            }

            // RRA - Rotate Right then Add (ROR + ADC)
            0x67 => {
                // RRA $nn (Zero Page)
                let addr = self.addr_zero_page(bus);
                let value = bus.read(addr as u32);
                bus.tick(1);
                let result = self.ror(value);
                bus.write(addr as u32, result);
                self.adc(result);
                5
            }
            0x77 => {
                // RRA $nn,X (Zero Page,X)
                let addr = self.addr_zero_page_x(bus);
                let value = bus.read(addr as u32);
                bus.tick(1);
                let result = self.ror(value);
                bus.write(addr as u32, result);
                self.adc(result);
                6
            }
            0x6F => {
                // RRA $nnnn (Absolute)
                let addr = self.addr_absolute(bus);
                let value = bus.read(addr as u32);
                bus.tick(1);
                let result = self.ror(value);
                bus.write(addr as u32, result);
                self.adc(result);
                6
            }
            0x7F => {
                // RRA $nnnn,X (Absolute,X)
                let addr = self.addr_absolute_x_rmw(bus);
                let value = bus.read(addr as u32);
                bus.tick(1);
                let result = self.ror(value);
                bus.write(addr as u32, result);
                self.adc(result);
                7
            }
            0x7B => {
                // RRA $nnnn,Y (Absolute,Y)
                let addr = self.addr_absolute_y_rmw(bus);
                let value = bus.read(addr as u32);
                bus.tick(1);
                let result = self.ror(value);
                bus.write(addr as u32, result);
                self.adc(result);
                7
            }
            0x63 => {
                // RRA ($nn,X) (Indexed Indirect)
                let addr = self.addr_indexed_indirect(bus);
                let value = bus.read(addr as u32);
                bus.tick(1);
                let result = self.ror(value);
                bus.write(addr as u32, result);
                self.adc(result);
                8
            }
            0x73 => {
                // RRA ($nn),Y (Indirect Indexed)
                let addr = self.addr_indirect_indexed_rmw(bus);
                let value = bus.read(addr as u32);
                bus.tick(1);
                let result = self.ror(value);
                bus.write(addr as u32, result);
                self.adc(result);
                8
            }

            // ANC - AND immediate then copy N to C
            0x0B | 0x2B => {
                let value = self.fetch(bus);
                self.a &= value;
                self.set_zn(self.a);
                self.set_flag(FLAG_C, self.a & 0x80 != 0);
                2
            }

            // ALR/ASR - AND immediate then LSR A
            0x4B => {
                let value = self.fetch(bus);
                self.a &= value;
                self.a = self.lsr(self.a);
                2
            }

            // ARR - AND immediate then ROR A (with special flag behavior)
            0x6B => {
                let value = self.fetch(bus);
                self.a &= value;
                self.a = self.ror(self.a);
                // Special flag behavior for ARR
                self.set_flag(FLAG_C, self.a & 0x40 != 0);
                self.set_flag(FLAG_V, ((self.a & 0x40) ^ ((self.a & 0x20) << 1)) != 0);
                2
            }

            // SBX/AXS - (A AND X) - immediate -> X (no borrow)
            0xCB => {
                let value = self.fetch(bus);
                let temp = (self.a & self.x) as u16;
                let result = temp.wrapping_sub(value as u16);
                self.x = result as u8;
                self.set_flag(FLAG_C, result < 0x100);
                self.set_zn(self.x);
                2
            }

            // Unknown/illegal opcode
            _ => {
                panic!(
                    "Unimplemented opcode: ${:02X} at PC=${:04X}",
                    opcode,
                    self.pc.wrapping_sub(1)
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestBus {
        memory: [u8; 65536],
    }

    impl TestBus {
        fn new() -> Self {
            Self { memory: [0; 65536] }
        }
    }

    impl Bus for TestBus {
        fn read(&mut self, address: u32) -> u8 {
            self.memory[(address & 0xFFFF) as usize]
        }

        fn write(&mut self, address: u32, value: u8) {
            self.memory[(address & 0xFFFF) as usize] = value;
        }

        fn tick(&mut self, _cycles: u32) {}
    }

    #[test]
    fn test_lda_immediate() {
        let mut cpu = Mos6502::new();
        let mut bus = TestBus::new();

        bus.memory[0] = 0xA9; // LDA #$42
        bus.memory[1] = 0x42;

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 2);
        assert_eq!(cpu.a, 0x42);
        assert!(!cpu.zero());
        assert!(!cpu.negative());
    }

    #[test]
    fn test_lda_zero_flag() {
        let mut cpu = Mos6502::new();
        let mut bus = TestBus::new();

        bus.memory[0] = 0xA9; // LDA #$00
        bus.memory[1] = 0x00;

        cpu.step(&mut bus);

        assert_eq!(cpu.a, 0x00);
        assert!(cpu.zero());
        assert!(!cpu.negative());
    }

    #[test]
    fn test_lda_negative_flag() {
        let mut cpu = Mos6502::new();
        let mut bus = TestBus::new();

        bus.memory[0] = 0xA9; // LDA #$80
        bus.memory[1] = 0x80;

        cpu.step(&mut bus);

        assert_eq!(cpu.a, 0x80);
        assert!(!cpu.zero());
        assert!(cpu.negative());
    }

    #[test]
    fn test_adc_simple() {
        let mut cpu = Mos6502::new();
        let mut bus = TestBus::new();

        cpu.a = 0x10;
        bus.memory[0] = 0x69; // ADC #$20
        bus.memory[1] = 0x20;

        cpu.step(&mut bus);

        assert_eq!(cpu.a, 0x30);
        assert!(!cpu.carry());
        assert!(!cpu.overflow());
    }

    #[test]
    fn test_adc_carry() {
        let mut cpu = Mos6502::new();
        let mut bus = TestBus::new();

        cpu.a = 0xFF;
        bus.memory[0] = 0x69; // ADC #$01
        bus.memory[1] = 0x01;

        cpu.step(&mut bus);

        assert_eq!(cpu.a, 0x00);
        assert!(cpu.carry());
        assert!(cpu.zero());
    }

    #[test]
    fn test_branch_taken() {
        let mut cpu = Mos6502::new();
        let mut bus = TestBus::new();

        cpu.set_flag(FLAG_Z, true);
        bus.memory[0] = 0xF0; // BEQ $05
        bus.memory[1] = 0x05;

        let cycles = cpu.step(&mut bus);

        assert_eq!(cpu.pc, 0x07); // 0x02 + 0x05
        assert_eq!(cycles, 3); // 2 base + 1 for taken
    }

    #[test]
    fn test_branch_not_taken() {
        let mut cpu = Mos6502::new();
        let mut bus = TestBus::new();

        cpu.set_flag(FLAG_Z, false);
        bus.memory[0] = 0xF0; // BEQ $05
        bus.memory[1] = 0x05;

        let cycles = cpu.step(&mut bus);

        assert_eq!(cpu.pc, 0x02); // No branch
        assert_eq!(cycles, 2);
    }

    #[test]
    fn test_jsr_rts() {
        let mut cpu = Mos6502::new();
        let mut bus = TestBus::new();

        // JSR $1000
        bus.memory[0x0000] = 0x20;
        bus.memory[0x0001] = 0x00;
        bus.memory[0x0002] = 0x10;

        // At $1000: LDA #$42, RTS
        bus.memory[0x1000] = 0xA9;
        bus.memory[0x1001] = 0x42;
        bus.memory[0x1002] = 0x60;

        cpu.step(&mut bus); // JSR
        assert_eq!(cpu.pc, 0x1000);

        cpu.step(&mut bus); // LDA #$42
        assert_eq!(cpu.a, 0x42);

        cpu.step(&mut bus); // RTS
        assert_eq!(cpu.pc, 0x0003);
    }
}
