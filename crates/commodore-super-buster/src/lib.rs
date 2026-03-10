//! Commodore Super Buster — Zorro III bus controller for A3000/A4000.
//!
//! Super Buster extends the Zorro II bus protocol to the 32-bit address
//! space. Zorro III boards are configured first, then Zorro II boards.
//! Zorro III boards can be mapped anywhere above `$01000000` in the
//! 68030/040 address space.
//!
//! The autoconfig protocol reuses the same `$E80000` window as Zorro II,
//! with an extended descriptor at `$E80100` for Zorro III boards.

pub use commodore_buster::{BoardSize, ZorroIIRamBoard, ZorroIISlot};

// ---------------------------------------------------------------------------
// Zorro III autoconfig register offsets
// ---------------------------------------------------------------------------

/// Autoconfig register offsets for Zorro III within the $E80000 block.
/// Shares the same layout as Zorro II, but with extended address writes.
#[allow(dead_code)]
mod ac_reg {
    /// Base address write — bits 31:24 of the 32-bit base.
    pub const Z3_BASE_HI: u32 = 0x44;
    /// Base address write — bits 23:16 of the 32-bit base.
    pub const Z3_BASE_LO: u32 = 0x48;
    /// Shut-up (board responds "not present").
    pub const SHUTUP: u32 = 0x4C;
}

// ---------------------------------------------------------------------------
// Zorro III board size
// ---------------------------------------------------------------------------

/// Size code for Zorro III boards (extends Zorro II sizes with larger options).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ZorroIIIBoardSize {
    /// Reuse a Zorro II size (64K–8M).
    ZorroII(BoardSize),
    /// 16 MB.
    Size16M,
    /// 32 MB.
    Size32M,
    /// 64 MB.
    Size64M,
    /// 128 MB.
    Size128M,
    /// 256 MB.
    Size256M,
    /// 512 MB.
    Size512M,
    /// 1 GB.
    Size1G,
}

impl ZorroIIIBoardSize {
    /// Size in bytes.
    #[must_use]
    pub const fn bytes(self) -> u32 {
        match self {
            Self::ZorroII(bs) => bs.bytes(),
            Self::Size16M => 16 * 1024 * 1024,
            Self::Size32M => 32 * 1024 * 1024,
            Self::Size64M => 64 * 1024 * 1024,
            Self::Size128M => 128 * 1024 * 1024,
            Self::Size256M => 256 * 1024 * 1024,
            Self::Size512M => 512 * 1024 * 1024,
            Self::Size1G => 1024 * 1024 * 1024,
        }
    }

    /// 4-bit size code for the autoconfig descriptor (Zorro III extended).
    const fn code(self) -> u8 {
        match self {
            Self::ZorroII(bs) => bs.code(),
            Self::Size16M => 0x00,
            Self::Size32M => 0x01,
            Self::Size64M => 0x02,
            Self::Size128M => 0x03,
            Self::Size256M => 0x04,
            Self::Size512M => 0x05,
            Self::Size1G => 0x06,
        }
    }
}

// ---------------------------------------------------------------------------
// Zorro III autoconfig descriptor
// ---------------------------------------------------------------------------

/// Autoconfig descriptor for a Zorro III board.
#[derive(Debug, Clone)]
pub struct ZorroIIIDescriptor {
    /// Product number (0–255).
    pub product: u8,
    /// Board size.
    pub size: ZorroIIIBoardSize,
    /// Manufacturer ID (16-bit).
    pub manufacturer: u16,
    /// Serial number (32-bit).
    pub serial: u32,
}

impl ZorroIIIDescriptor {
    /// Build the nybble-packed autoconfig ROM image (64 bytes).
    /// Uses the same inverted-nybble format as Zorro II, but with the
    /// Zorro III type identifier (bit 5 set in type byte).
    fn build_rom(&self) -> [u8; 64] {
        let mut rom = [0xFFu8; 64];

        // For Zorro III, we use the lower 3 bits of the Z2 size code field,
        // plus the extension flag. The type byte encodes:
        //   bits 7:6 = 11 (autoconfig board present + chained)
        //   bit  5   = 1  (Zorro III)
        //   bits 4:0 = size code (3 bits used, top 2 for Z3 extension)
        let z2_size_code = match self.size {
            ZorroIIIBoardSize::ZorroII(bs) => bs.code(),
            _ => 0b000, // Z3-only sizes use the extended size field
        };
        let type_hi = 0xE0 | z2_size_code; // $E0 = Z3 memory, chained
        let type_lo = 0x00;

        rom[0x00] = !type_hi;
        rom[0x02] = !type_lo;
        rom[0x04] = !(self.product >> 4);
        rom[0x06] = !(self.product & 0x0F);

        // Flags at $08/$0A — include Z3 extended size code.
        let flags_hi = self.size.code() & 0x0F;
        rom[0x08] = !flags_hi;
        rom[0x0A] = !0u8;

        // Manufacturer at $10–$16.
        rom[0x10] = !(self.manufacturer >> 12) as u8;
        rom[0x12] = !((self.manufacturer >> 8) & 0x0F) as u8;
        rom[0x14] = !((self.manufacturer >> 4) & 0x0F) as u8;
        rom[0x16] = !(self.manufacturer & 0x0F) as u8;

        // Serial at $18–$26.
        rom[0x18] = !(self.serial >> 28) as u8;
        rom[0x1A] = !((self.serial >> 24) & 0x0F) as u8;
        rom[0x1C] = !((self.serial >> 20) & 0x0F) as u8;
        rom[0x1E] = !((self.serial >> 16) & 0x0F) as u8;
        rom[0x20] = !((self.serial >> 12) & 0x0F) as u8;
        rom[0x22] = !((self.serial >> 8) & 0x0F) as u8;
        rom[0x24] = !((self.serial >> 4) & 0x0F) as u8;
        rom[0x26] = !(self.serial & 0x0F) as u8;

        rom
    }
}

// ---------------------------------------------------------------------------
// Zorro III board types
// ---------------------------------------------------------------------------

/// A configured Zorro III RAM expansion board.
#[derive(Debug, Clone)]
pub struct ZorroIIIRamBoard {
    /// Descriptor for autoconfig.
    pub descriptor: ZorroIIIDescriptor,
    /// RAM data.
    pub ram: Vec<u8>,
    /// Configured base address (32-bit, set by OS during autoconfig).
    pub base_addr: Option<u32>,
}

impl ZorroIIIRamBoard {
    /// Create a new Zorro III RAM board.
    #[must_use]
    pub fn new(size: ZorroIIIBoardSize) -> Self {
        Self {
            descriptor: ZorroIIIDescriptor {
                product: 1,
                size,
                manufacturer: 0x0198, // EMU198X
                serial: 0,
            },
            ram: vec![0; size.bytes() as usize],
            base_addr: None,
        }
    }
}

/// Contents of a Zorro III slot.
#[derive(Debug, Clone)]
pub enum ZorroIIISlot {
    /// RAM expansion board.
    Ram(ZorroIIIRamBoard),
}

impl ZorroIIISlot {
    /// Autoconfig descriptor for this slot.
    fn descriptor(&self) -> &ZorroIIIDescriptor {
        match self {
            Self::Ram(board) => &board.descriptor,
        }
    }

    /// Configured base address, if any.
    fn base_addr(&self) -> Option<u32> {
        match self {
            Self::Ram(board) => board.base_addr,
        }
    }

    /// Set the base address (during autoconfig).
    fn set_base_addr(&mut self, addr: u32) {
        match self {
            Self::Ram(board) => board.base_addr = Some(addr),
        }
    }

    /// Size in bytes.
    fn size(&self) -> u32 {
        match self {
            Self::Ram(board) => board.descriptor.size.bytes(),
        }
    }

    /// Read a byte at an offset from the board's base address.
    fn read_byte(&self, offset: u32) -> u8 {
        match self {
            Self::Ram(board) => {
                let off = offset as usize;
                if off < board.ram.len() {
                    board.ram[off]
                } else {
                    0
                }
            }
        }
    }

    /// Write a byte at an offset from the board's base address.
    fn write_byte(&mut self, offset: u32, val: u8) {
        match self {
            Self::Ram(board) => {
                let off = offset as usize;
                if off < board.ram.len() {
                    board.ram[off] = val;
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Autoconfig phase
// ---------------------------------------------------------------------------

/// Which phase of autoconfig the Super Buster is in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AutoconfigPhase {
    /// Configuring Zorro III boards.
    ZorroIII,
    /// Configuring Zorro II boards.
    ZorroII,
    /// All boards configured.
    Complete,
}

// ---------------------------------------------------------------------------
// Super Buster
// ---------------------------------------------------------------------------

/// Super Buster — Zorro III bus controller.
///
/// Manages both Zorro III (32-bit) and Zorro II (24-bit) expansion slots.
/// Zorro III boards are configured first through the autoconfig protocol,
/// then Zorro II boards.
#[derive(Debug, Clone)]
pub struct SuperBuster {
    /// Zorro III slots.
    z3_slots: Vec<ZorroIIISlot>,
    /// Zorro II slots (delegated to inner Buster).
    z2: commodore_buster::Buster,
    /// Index of the next unconfigured Z3 board.
    z3_current: usize,
    /// Current autoconfig phase.
    phase: AutoconfigPhase,
}

impl SuperBuster {
    /// Create a new Super Buster with no expansion boards.
    #[must_use]
    pub fn new() -> Self {
        Self {
            z3_slots: Vec::new(),
            z2: commodore_buster::Buster::new(),
            z3_current: 0,
            phase: AutoconfigPhase::ZorroIII,
        }
    }

    /// Add a Zorro III expansion board.
    pub fn add_z3_slot(&mut self, slot: ZorroIIISlot) {
        self.z3_slots.push(slot);
    }

    /// Add a Zorro III RAM expansion board.
    pub fn add_z3_ram(&mut self, size: ZorroIIIBoardSize) {
        self.z3_slots
            .push(ZorroIIISlot::Ram(ZorroIIIRamBoard::new(size)));
    }

    /// Add a Zorro II expansion board.
    pub fn add_z2_slot(&mut self, slot: ZorroIISlot) {
        self.z2.add_slot(slot);
    }

    /// Add a Zorro II RAM expansion board.
    pub fn add_z2_ram(&mut self, size: BoardSize) {
        self.z2.add_ram(size);
    }

    /// Current autoconfig phase (computed — auto-advances past exhausted Z3).
    #[must_use]
    pub fn phase(&self) -> AutoconfigPhase {
        self.effective_phase()
    }

    /// True when all boards (Z3 and Z2) are configured.
    #[must_use]
    pub fn autoconfig_complete(&self) -> bool {
        self.effective_phase() == AutoconfigPhase::Complete
    }

    /// Compute the effective phase, skipping past Z3 when exhausted.
    fn effective_phase(&self) -> AutoconfigPhase {
        match self.phase {
            AutoconfigPhase::ZorroIII if self.z3_current >= self.z3_slots.len() => {
                if self.z2.slot_count() == 0 || self.z2.autoconfig_complete() {
                    AutoconfigPhase::Complete
                } else {
                    AutoconfigPhase::ZorroII
                }
            }
            other => other,
        }
    }

    /// Reset autoconfig state (all boards become unconfigured).
    pub fn reset(&mut self) {
        self.z3_current = 0;
        self.phase = if self.z3_slots.is_empty() {
            if self.z2.slot_count() == 0 {
                AutoconfigPhase::Complete
            } else {
                AutoconfigPhase::ZorroII
            }
        } else {
            AutoconfigPhase::ZorroIII
        };
        for slot in &mut self.z3_slots {
            match slot {
                ZorroIIISlot::Ram(board) => board.base_addr = None,
            }
        }
        self.z2.reset();
    }

    /// Advance from Z3 phase to Z2 phase (or complete).
    fn advance_from_z3(&mut self) {
        if self.z2.slot_count() == 0 || self.z2.autoconfig_complete() {
            self.phase = AutoconfigPhase::Complete;
        } else {
            self.phase = AutoconfigPhase::ZorroII;
        }
    }

    /// Read a byte from the autoconfig space ($E80000–$E8007F).
    #[must_use]
    pub fn autoconfig_read(&self, addr: u32) -> u8 {
        match self.effective_phase() {
            AutoconfigPhase::ZorroIII => {
                if self.z3_current < self.z3_slots.len() {
                    let slot = &self.z3_slots[self.z3_current];
                    let rom = slot.descriptor().build_rom();
                    let offset = (addr & 0x7F) as usize;
                    if offset < rom.len() {
                        rom[offset]
                    } else {
                        0xFF
                    }
                } else {
                    0xFF
                }
            }
            AutoconfigPhase::ZorroII => self.z2.autoconfig_read(addr),
            AutoconfigPhase::Complete => 0xFF,
        }
    }

    /// Write a byte to the autoconfig space.
    pub fn autoconfig_write(&mut self, addr: u32, val: u8) {
        match self.effective_phase() {
            AutoconfigPhase::ZorroIII => {
                if self.z3_current >= self.z3_slots.len() {
                    return;
                }
                let offset = addr & 0xFF;
                match offset {
                    ac_reg::Z3_BASE_HI => {
                        // Z3 base address: val provides A31:A24.
                        // The full 32-bit address is refined by Z3_BASE_LO.
                        let base = u32::from(val) << 24;
                        self.z3_slots[self.z3_current].set_base_addr(base);
                    }
                    ac_reg::Z3_BASE_LO => {
                        // Z3 base address refinement: val provides A23:A16.
                        // This also completes the configuration (advances to
                        // next board).
                        if let Some(base) = self.z3_slots[self.z3_current].base_addr() {
                            let refined = base | (u32::from(val) << 16);
                            self.z3_slots[self.z3_current].set_base_addr(refined);
                        }
                        self.z3_current += 1;
                        if self.z3_current >= self.z3_slots.len() {
                            self.advance_from_z3();
                        }
                    }
                    ac_reg::SHUTUP => {
                        self.z3_current += 1;
                        if self.z3_current >= self.z3_slots.len() {
                            self.advance_from_z3();
                        }
                    }
                    _ => {}
                }
            }
            AutoconfigPhase::ZorroII => {
                self.z2.autoconfig_write(addr, val);
                if self.z2.autoconfig_complete() {
                    self.phase = AutoconfigPhase::Complete;
                }
            }
            AutoconfigPhase::Complete => {}
        }
    }

    /// Read a byte from a configured Zorro III board (32-bit address space).
    #[must_use]
    pub fn z3_board_read(&self, addr: u32) -> Option<u8> {
        for slot in &self.z3_slots {
            if let Some(base) = slot.base_addr() {
                let end = base.wrapping_add(slot.size());
                if addr >= base && addr < end {
                    return Some(slot.read_byte(addr - base));
                }
            }
        }
        None
    }

    /// Write a byte to a configured Zorro III board. Returns `true` if claimed.
    pub fn z3_board_write(&mut self, addr: u32, val: u8) -> bool {
        for slot in &mut self.z3_slots {
            if let Some(base) = slot.base_addr() {
                let end = base.wrapping_add(slot.size());
                if addr >= base && addr < end {
                    slot.write_byte(addr - base, val);
                    return true;
                }
            }
        }
        false
    }

    /// Read a byte from a configured Zorro II board (24-bit address space).
    #[must_use]
    pub fn z2_board_read(&self, addr: u32) -> Option<u8> {
        self.z2.board_read(addr)
    }

    /// Write a byte to a configured Zorro II board. Returns `true` if claimed.
    pub fn z2_board_write(&mut self, addr: u32, val: u8) -> bool {
        self.z2.board_write(addr, val)
    }
}

impl Default for SuperBuster {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_super_buster_is_complete() {
        let sb = SuperBuster::new();
        // No Z3 slots means we skip straight past Z3 phase at first read.
        // But the phase starts as ZorroIII — it advances when we discover
        // there are no Z3 boards during autoconfig_read.
        assert_eq!(sb.autoconfig_read(0xE8_0000), 0xFF);
    }

    #[test]
    fn z3_boards_configure_before_z2() {
        let mut sb = SuperBuster::new();
        sb.add_z3_ram(ZorroIIIBoardSize::ZorroII(BoardSize::Size1M));
        sb.add_z2_ram(BoardSize::Size512K);

        // Should be in Z3 phase.
        assert_eq!(sb.phase(), AutoconfigPhase::ZorroIII);
        assert!(!sb.autoconfig_complete());

        // Read Z3 descriptor — type byte should have bit 5 set (Z3 flag).
        let type_hi = sb.autoconfig_read(0xE8_0000);
        let decoded = !type_hi;
        assert!(decoded & 0x20 != 0, "Z3 flag (bit 5) should be set");

        // Configure Z3 board at $40000000.
        sb.autoconfig_write(0xE8_0000 | ac_reg::Z3_BASE_HI, 0x40);
        sb.autoconfig_write(0xE8_0000 | ac_reg::Z3_BASE_LO, 0x00);

        // Should now be in Z2 phase.
        assert_eq!(sb.phase(), AutoconfigPhase::ZorroII);

        // Configure Z2 board at $200000.
        sb.autoconfig_write(0xE8_0000 | 0x48, 0x20);

        // Should be complete.
        assert!(sb.autoconfig_complete());

        // Verify Z3 board is accessible.
        assert!(sb.z3_board_write(0x4000_0000, 0xAA));
        assert_eq!(sb.z3_board_read(0x4000_0000), Some(0xAA));

        // Verify Z2 board is accessible.
        assert!(sb.z2_board_write(0x20_0000, 0xBB));
        assert_eq!(sb.z2_board_read(0x20_0000), Some(0xBB));
    }

    #[test]
    fn z3_only_completes_after_all_z3_configured() {
        let mut sb = SuperBuster::new();
        sb.add_z3_ram(ZorroIIIBoardSize::Size16M);

        assert_eq!(sb.phase(), AutoconfigPhase::ZorroIII);

        // Configure at $10000000.
        sb.autoconfig_write(0xE8_0000 | ac_reg::Z3_BASE_HI, 0x10);
        sb.autoconfig_write(0xE8_0000 | ac_reg::Z3_BASE_LO, 0x00);

        // No Z2 boards, so should be complete.
        assert!(sb.autoconfig_complete());

        // Write and read 16 MB board.
        assert!(sb.z3_board_write(0x1000_0000, 0xCC));
        assert_eq!(sb.z3_board_read(0x1000_0000), Some(0xCC));

        // Edge: just before end of board.
        let last = 0x1000_0000 + 16 * 1024 * 1024 - 1;
        assert!(sb.z3_board_write(last, 0xDD));
        assert_eq!(sb.z3_board_read(last), Some(0xDD));

        // Past end — should miss.
        assert_eq!(sb.z3_board_read(0x1000_0000 + 16 * 1024 * 1024), None);
    }

    #[test]
    fn z3_shutup_skips_board() {
        let mut sb = SuperBuster::new();
        sb.add_z3_ram(ZorroIIIBoardSize::ZorroII(BoardSize::Size2M));

        sb.autoconfig_write(0xE8_0000 | ac_reg::SHUTUP, 0);

        // Board was shut up — should complete (no Z2 boards).
        assert!(sb.autoconfig_complete());
        assert_eq!(sb.z3_board_read(0x4000_0000), None);
    }

    #[test]
    fn reset_unconfigures_all() {
        let mut sb = SuperBuster::new();
        sb.add_z3_ram(ZorroIIIBoardSize::ZorroII(BoardSize::Size1M));
        sb.add_z2_ram(BoardSize::Size512K);

        // Configure everything.
        sb.autoconfig_write(0xE8_0000 | ac_reg::Z3_BASE_HI, 0x40);
        sb.autoconfig_write(0xE8_0000 | ac_reg::Z3_BASE_LO, 0x00);
        sb.autoconfig_write(0xE8_0000 | 0x48, 0x20);
        assert!(sb.autoconfig_complete());

        sb.reset();

        assert_eq!(sb.phase(), AutoconfigPhase::ZorroIII);
        assert!(!sb.autoconfig_complete());
        assert_eq!(sb.z3_board_read(0x4000_0000), None);
        assert_eq!(sb.z2_board_read(0x20_0000), None);
    }

    #[test]
    fn z2_only_skips_z3_phase() {
        let mut sb = SuperBuster::new();
        sb.add_z2_ram(BoardSize::Size512K);

        // No Z3 boards — effective phase auto-advances past empty Z3.
        assert_eq!(sb.phase(), AutoconfigPhase::ZorroII);

        // Z2 descriptor should be visible at autoconfig.
        assert_ne!(sb.autoconfig_read(0xE8_0000), 0xFF);

        // Configure Z2 board at $200000.
        sb.autoconfig_write(0xE8_0000 | 0x48, 0x20);
        assert!(sb.autoconfig_complete());
    }

    #[test]
    fn multiple_z3_boards() {
        let mut sb = SuperBuster::new();
        sb.add_z3_ram(ZorroIIIBoardSize::ZorroII(BoardSize::Size2M));
        sb.add_z3_ram(ZorroIIIBoardSize::ZorroII(BoardSize::Size2M));

        // Configure first Z3 at $40000000.
        sb.autoconfig_write(0xE8_0000 | ac_reg::Z3_BASE_HI, 0x40);
        sb.autoconfig_write(0xE8_0000 | ac_reg::Z3_BASE_LO, 0x00);

        assert_eq!(sb.phase(), AutoconfigPhase::ZorroIII);

        // Configure second Z3 at $40200000.
        sb.autoconfig_write(0xE8_0000 | ac_reg::Z3_BASE_HI, 0x40);
        sb.autoconfig_write(0xE8_0000 | ac_reg::Z3_BASE_LO, 0x20);

        assert!(sb.autoconfig_complete());

        // Verify both boards.
        sb.z3_board_write(0x4000_0000, 0x11);
        sb.z3_board_write(0x4020_0000, 0x22);
        assert_eq!(sb.z3_board_read(0x4000_0000), Some(0x11));
        assert_eq!(sb.z3_board_read(0x4020_0000), Some(0x22));
    }

    #[test]
    fn z3_board_size_bytes() {
        assert_eq!(ZorroIIIBoardSize::Size16M.bytes(), 16 * 1024 * 1024);
        assert_eq!(ZorroIIIBoardSize::Size32M.bytes(), 32 * 1024 * 1024);
        assert_eq!(ZorroIIIBoardSize::Size64M.bytes(), 64 * 1024 * 1024);
        assert_eq!(ZorroIIIBoardSize::Size128M.bytes(), 128 * 1024 * 1024);
        assert_eq!(ZorroIIIBoardSize::Size256M.bytes(), 256 * 1024 * 1024);
        assert_eq!(ZorroIIIBoardSize::Size512M.bytes(), 512 * 1024 * 1024);
        assert_eq!(ZorroIIIBoardSize::Size1G.bytes(), 1024 * 1024 * 1024);
        assert_eq!(
            ZorroIIIBoardSize::ZorroII(BoardSize::Size2M).bytes(),
            2 * 1024 * 1024
        );
    }

    #[test]
    fn z3_board_read_outside_range_returns_none() {
        let mut sb = SuperBuster::new();
        sb.add_z3_ram(ZorroIIIBoardSize::ZorroII(BoardSize::Size1M));

        sb.autoconfig_write(0xE8_0000 | ac_reg::Z3_BASE_HI, 0x40);
        sb.autoconfig_write(0xE8_0000 | ac_reg::Z3_BASE_LO, 0x00);

        // 1 MB at $40000000 ends at $40100000.
        assert_eq!(sb.z3_board_read(0x4010_0000), None);
        assert_eq!(sb.z3_board_read(0x3FFF_FFFF), None);
    }
}
