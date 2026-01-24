//! 6502 addressing modes.
//!
//! The 6502 has 13 addressing modes:
//! - Implied: No operand (e.g., CLC, RTS)
//! - Accumulator: Operates on A register (e.g., ASL A)
//! - Immediate: #$nn (literal value)
//! - Zero Page: $nn (8-bit address in page zero)
//! - Zero Page,X: $nn,X (8-bit address + X, wraps in page zero)
//! - Zero Page,Y: $nn,Y (8-bit address + Y, wraps in page zero)
//! - Absolute: $nnnn (16-bit address)
//! - Absolute,X: $nnnn,X (16-bit address + X, may cross page)
//! - Absolute,Y: $nnnn,Y (16-bit address + Y, may cross page)
//! - Indirect: ($nnnn) (JMP only, buggy page boundary behavior)
//! - Indexed Indirect: ($nn,X) (pointer in zero page indexed by X)
//! - Indirect Indexed: ($nn),Y (zero page pointer + Y)
//! - Relative: Branch offset (-128 to +127)

use crate::Mos6502;
use emu_core::Bus;

impl Mos6502 {
    /// Fetch the next byte at PC and increment PC.
    pub(crate) fn fetch(&mut self, bus: &mut impl Bus) -> u8 {
        let value = bus.read(self.pc as u32);
        self.pc = self.pc.wrapping_add(1);
        value
    }

    /// Fetch a 16-bit word (little-endian) at PC.
    pub(crate) fn fetch_word(&mut self, bus: &mut impl Bus) -> u16 {
        let low = self.fetch(bus);
        let high = self.fetch(bus);
        u16::from_le_bytes([low, high])
    }

    /// Read a 16-bit word from memory (little-endian).
    pub(crate) fn read_word(&self, bus: &mut impl Bus, addr: u16) -> u16 {
        let low = bus.read(addr as u32);
        let high = bus.read(addr.wrapping_add(1) as u32);
        u16::from_le_bytes([low, high])
    }

    /// Read a 16-bit word with 6502 page boundary bug (for indirect JMP).
    /// If addr is $xxFF, high byte comes from $xx00 instead of $xx00+$100.
    pub(crate) fn read_word_page_bug(&self, bus: &mut impl Bus, addr: u16) -> u16 {
        let low = bus.read(addr as u32);
        // High byte address wraps within the same page
        let high_addr = (addr & 0xFF00) | ((addr.wrapping_add(1)) & 0x00FF);
        let high = bus.read(high_addr as u32);
        u16::from_le_bytes([low, high])
    }

    /// Push a byte onto the stack.
    pub(crate) fn push(&mut self, bus: &mut impl Bus, value: u8) {
        bus.write(0x0100 | self.sp as u32, value);
        self.sp = self.sp.wrapping_sub(1);
    }

    /// Pull a byte from the stack.
    pub(crate) fn pull(&mut self, bus: &mut impl Bus) -> u8 {
        self.sp = self.sp.wrapping_add(1);
        bus.read(0x0100 | self.sp as u32)
    }

    /// Push a 16-bit word onto the stack (high byte first).
    pub(crate) fn push_word(&mut self, bus: &mut impl Bus, value: u16) {
        self.push(bus, (value >> 8) as u8);
        self.push(bus, value as u8);
    }

    /// Pull a 16-bit word from the stack (low byte first).
    pub(crate) fn pull_word(&mut self, bus: &mut impl Bus) -> u16 {
        let low = self.pull(bus);
        let high = self.pull(bus);
        u16::from_le_bytes([low, high])
    }

    // =========================================================================
    // Addressing mode helpers
    // =========================================================================

    /// Zero Page: $nn
    pub(crate) fn addr_zero_page(&mut self, bus: &mut impl Bus) -> u16 {
        self.fetch(bus) as u16
    }

    /// Zero Page,X: $nn,X (wraps within zero page)
    pub(crate) fn addr_zero_page_x(&mut self, bus: &mut impl Bus) -> u16 {
        let base = self.fetch(bus);
        // Dummy read for the add cycle
        bus.read(base as u32);
        base.wrapping_add(self.x) as u16
    }

    /// Zero Page,Y: $nn,Y (wraps within zero page)
    pub(crate) fn addr_zero_page_y(&mut self, bus: &mut impl Bus) -> u16 {
        let base = self.fetch(bus);
        // Dummy read for the add cycle
        bus.read(base as u32);
        base.wrapping_add(self.y) as u16
    }

    /// Absolute: $nnnn
    pub(crate) fn addr_absolute(&mut self, bus: &mut impl Bus) -> u16 {
        self.fetch_word(bus)
    }

    /// Absolute,X: $nnnn,X
    /// Returns (address, page_crossed) - page crossing affects cycle count for reads.
    pub(crate) fn addr_absolute_x(&mut self, bus: &mut impl Bus) -> (u16, bool) {
        let base = self.fetch_word(bus);
        let addr = base.wrapping_add(self.x as u16);
        let page_crossed = (base & 0xFF00) != (addr & 0xFF00);
        (addr, page_crossed)
    }

    /// Absolute,X: $nnnn,X (always takes penalty cycle for RMW)
    pub(crate) fn addr_absolute_x_rmw(&mut self, bus: &mut impl Bus) -> u16 {
        let base = self.fetch_word(bus);
        let addr = base.wrapping_add(self.x as u16);
        // RMW always does a dummy read, regardless of page crossing
        let partial = (base & 0xFF00) | (addr & 0x00FF);
        bus.read(partial as u32);
        addr
    }

    /// Absolute,Y: $nnnn,Y
    /// Returns (address, page_crossed) - page crossing affects cycle count for reads.
    pub(crate) fn addr_absolute_y(&mut self, bus: &mut impl Bus) -> (u16, bool) {
        let base = self.fetch_word(bus);
        let addr = base.wrapping_add(self.y as u16);
        let page_crossed = (base & 0xFF00) != (addr & 0xFF00);
        (addr, page_crossed)
    }

    /// Absolute,Y (always takes penalty cycle for writes/RMW)
    pub(crate) fn addr_absolute_y_rmw(&mut self, bus: &mut impl Bus) -> u16 {
        let base = self.fetch_word(bus);
        let addr = base.wrapping_add(self.y as u16);
        // Always do a dummy read
        let partial = (base & 0xFF00) | (addr & 0x00FF);
        bus.read(partial as u32);
        addr
    }

    /// Indexed Indirect: ($nn,X)
    /// The pointer is at zero page address (operand + X), wrapping within ZP.
    pub(crate) fn addr_indexed_indirect(&mut self, bus: &mut impl Bus) -> u16 {
        let base = self.fetch(bus);
        // Dummy read at base address
        bus.read(base as u32);
        let ptr = base.wrapping_add(self.x);
        // Read 16-bit address from zero page (wraps within ZP)
        let low = bus.read(ptr as u32);
        let high = bus.read(ptr.wrapping_add(1) as u32);
        u16::from_le_bytes([low, high])
    }

    /// Indirect Indexed: ($nn),Y
    /// Returns (address, page_crossed).
    pub(crate) fn addr_indirect_indexed(&mut self, bus: &mut impl Bus) -> (u16, bool) {
        let ptr = self.fetch(bus);
        // Read 16-bit address from zero page (wraps within ZP)
        let low = bus.read(ptr as u32);
        let high = bus.read(ptr.wrapping_add(1) as u32);
        let base = u16::from_le_bytes([low, high]);
        let addr = base.wrapping_add(self.y as u16);
        let page_crossed = (base & 0xFF00) != (addr & 0xFF00);
        (addr, page_crossed)
    }

    /// Indirect Indexed: ($nn),Y (always takes penalty for writes/RMW)
    pub(crate) fn addr_indirect_indexed_rmw(&mut self, bus: &mut impl Bus) -> u16 {
        let ptr = self.fetch(bus);
        let low = bus.read(ptr as u32);
        let high = bus.read(ptr.wrapping_add(1) as u32);
        let base = u16::from_le_bytes([low, high]);
        let addr = base.wrapping_add(self.y as u16);
        // Dummy read at partial address
        let partial = (base & 0xFF00) | (addr & 0x00FF);
        bus.read(partial as u32);
        addr
    }

    /// Relative: Branch offset
    /// Returns the target address after applying the signed offset.
    pub(crate) fn branch_offset(&mut self, bus: &mut impl Bus) -> u16 {
        let offset = self.fetch(bus) as i8;
        self.pc.wrapping_add(offset as u16)
    }

    /// Execute a branch if condition is true.
    /// Returns extra cycles (1 if branch taken, +1 more if page crossed).
    pub(crate) fn branch_if(&mut self, bus: &mut impl Bus, condition: bool) -> u32 {
        let target = self.branch_offset(bus);
        if condition {
            // Branch taken - 1 extra cycle
            bus.tick(1);
            let page_crossed = (self.pc & 0xFF00) != (target & 0xFF00);
            self.pc = target;
            if page_crossed {
                // Page crossing - 1 more cycle
                bus.tick(1);
                2
            } else {
                1
            }
        } else {
            0
        }
    }
}
