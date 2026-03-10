//! Commodore Buster — Zorro II bus controller for Amiga systems.
//!
//! Buster manages Zorro II autoconfig at `$E80000` and expansion board
//! I/O dispatch. Each slot holds either a RAM expansion or is empty.
//! The autoconfig protocol presents unconfigured boards one at a time;
//! the OS reads the descriptor, writes a base address, and the next
//! board becomes visible.

// ---------------------------------------------------------------------------
// Autoconfig register offsets (nybble-packed, even byte addresses)
// ---------------------------------------------------------------------------

/// Autoconfig register offsets within the $E80000 block.
/// Registers are nybble-wide at even byte addresses ($00, $02, $04, ...).
#[allow(dead_code)]
mod ac_reg {
    /// Type / size / chained (high nybble).
    pub const TYPE_HI: u32 = 0x00;
    /// Type / flags (low nybble).
    pub const TYPE_LO: u32 = 0x02;
    /// Product number (high nybble).
    pub const PRODUCT_HI: u32 = 0x04;
    /// Product number (low nybble).
    pub const PRODUCT_LO: u32 = 0x06;
    /// Flags (high nybble).
    pub const FLAGS_HI: u32 = 0x08;
    /// Flags (low nybble).
    pub const FLAGS_LO: u32 = 0x0A;
    /// Manufacturer ID bytes.
    pub const MFR_HI_HI: u32 = 0x10;
    pub const MFR_HI_LO: u32 = 0x12;
    pub const MFR_LO_HI: u32 = 0x14;
    pub const MFR_LO_LO: u32 = 0x16;
    /// Serial number (4 bytes, 8 nybbles).
    pub const SERIAL_0: u32 = 0x18;
    pub const SERIAL_1: u32 = 0x1A;
    pub const SERIAL_2: u32 = 0x1C;
    pub const SERIAL_3: u32 = 0x1E;
    pub const SERIAL_4: u32 = 0x20;
    pub const SERIAL_5: u32 = 0x22;
    pub const SERIAL_6: u32 = 0x24;
    pub const SERIAL_7: u32 = 0x26;
    /// Base address write (high nybble).
    pub const BASE_HI: u32 = 0x48;
    /// Base address write (low nybble).
    pub const BASE_LO: u32 = 0x4A;
    /// Shut-up (board responds "not present").
    pub const SHUTUP: u32 = 0x4C;
}

// ---------------------------------------------------------------------------
// Autoconfig descriptor
// ---------------------------------------------------------------------------

/// Size code for Zorro II autoconfig boards.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BoardSize {
    /// 64 KB.
    Size64K,
    /// 128 KB.
    Size128K,
    /// 256 KB.
    Size256K,
    /// 512 KB.
    Size512K,
    /// 1 MB.
    Size1M,
    /// 2 MB.
    Size2M,
    /// 4 MB.
    Size4M,
    /// 8 MB.
    Size8M,
}

impl BoardSize {
    /// Size in bytes.
    #[must_use]
    pub const fn bytes(self) -> u32 {
        match self {
            Self::Size64K => 64 * 1024,
            Self::Size128K => 128 * 1024,
            Self::Size256K => 256 * 1024,
            Self::Size512K => 512 * 1024,
            Self::Size1M => 1024 * 1024,
            Self::Size2M => 2 * 1024 * 1024,
            Self::Size4M => 4 * 1024 * 1024,
            Self::Size8M => 8 * 1024 * 1024,
        }
    }

    /// 3-bit size code for the autoconfig descriptor.
    #[must_use]
    pub const fn code(self) -> u8 {
        match self {
            Self::Size8M => 0b000,
            Self::Size64K => 0b001,
            Self::Size128K => 0b010,
            Self::Size256K => 0b011,
            Self::Size512K => 0b100,
            Self::Size1M => 0b101,
            Self::Size2M => 0b110,
            Self::Size4M => 0b111,
        }
    }

    /// Decode a 3-bit size code.
    #[must_use]
    pub const fn from_code(code: u8) -> Self {
        match code & 0x07 {
            0b000 => Self::Size8M,
            0b001 => Self::Size64K,
            0b010 => Self::Size128K,
            0b011 => Self::Size256K,
            0b100 => Self::Size512K,
            0b101 => Self::Size1M,
            0b110 => Self::Size2M,
            0b111 | _ => Self::Size4M,
        }
    }
}

/// Autoconfig descriptor for a Zorro II board.
#[derive(Debug, Clone)]
pub struct AutoconfigDescriptor {
    /// Board type: $C0 = Zorro II memory, $C1 = Zorro II I/O.
    pub board_type: u8,
    /// Product number (0–255).
    pub product: u8,
    /// Board size.
    pub size: BoardSize,
    /// Manufacturer ID (16-bit IANA-like).
    pub manufacturer: u16,
    /// Serial number (32-bit).
    pub serial: u32,
    /// "Can't shut up" flag — board must be configured.
    pub cant_shutup: bool,
}

impl AutoconfigDescriptor {
    /// Build the nybble-packed autoconfig ROM image (64 bytes).
    ///
    /// Even byte addresses $00–$42 contain the descriptor nybbles.
    /// Values are inverted (complemented) as per the Zorro II spec.
    fn build_rom(&self) -> [u8; 64] {
        let mut rom = [0xFFu8; 64];

        // $00: type byte high nybble — ERT_TYPEMASK | ERT_CHAINEDCONFIG | size
        let type_hi = 0xC0 | (self.size.code() << 0);
        // $02: type byte low nybble
        let type_lo = if self.board_type & 0x01 != 0 { 0x01 } else { 0x00 };

        // Nybbles are inverted and placed in bits 7:4 of the byte.
        rom[0x00] = !type_hi;
        rom[0x02] = !type_lo;
        rom[0x04] = !(self.product >> 4);
        rom[0x06] = !(self.product & 0x0F);

        // Flags at $08/$0A — keep simple (no flags set).
        let flags_hi = if self.cant_shutup { 0x01 } else { 0x00 };
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
// Zorro II board types
// ---------------------------------------------------------------------------

/// A configured Zorro II RAM expansion board.
#[derive(Debug, Clone)]
pub struct ZorroIIRamBoard {
    /// Descriptor for autoconfig.
    pub descriptor: AutoconfigDescriptor,
    /// RAM data.
    pub ram: Vec<u8>,
    /// Configured base address (set by the OS during autoconfig).
    /// `None` until configured.
    pub base_addr: Option<u32>,
}

impl ZorroIIRamBoard {
    /// Create a new RAM board with the given size.
    #[must_use]
    pub fn new(size: BoardSize) -> Self {
        Self {
            descriptor: AutoconfigDescriptor {
                board_type: 0xC0, // Zorro II memory
                product: 1,
                size,
                manufacturer: 0x0198, // EMU198X
                serial: 0,
                cant_shutup: false,
            },
            ram: vec![0; size.bytes() as usize],
            base_addr: None,
        }
    }
}

/// Contents of a Zorro II slot.
#[derive(Debug, Clone)]
pub enum ZorroIISlot {
    /// RAM expansion board.
    Ram(ZorroIIRamBoard),
}

impl ZorroIISlot {
    /// Autoconfig descriptor for this slot.
    fn descriptor(&self) -> &AutoconfigDescriptor {
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

    /// Read a byte relative to the board's base address.
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

    /// Write a byte relative to the board's base address.
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
// Buster
// ---------------------------------------------------------------------------

/// Buster — Zorro II bus controller.
///
/// Manages autoconfig at `$E80000` and dispatches I/O to configured
/// expansion boards.
#[derive(Debug, Clone)]
pub struct Buster {
    /// Zorro II slots (unconfigured boards first, then configured).
    slots: Vec<ZorroIISlot>,
    /// Index of the next unconfigured board (for autoconfig).
    /// When `>= slots.len()`, autoconfig is complete.
    current_autoconfig: usize,
}

impl Buster {
    /// Create a new Buster with no expansion boards.
    #[must_use]
    pub fn new() -> Self {
        Self {
            slots: Vec::new(),
            current_autoconfig: 0,
        }
    }

    /// Add a Zorro II expansion board.
    pub fn add_slot(&mut self, slot: ZorroIISlot) {
        self.slots.push(slot);
    }

    /// Add a Zorro II RAM expansion board of the given size.
    pub fn add_ram(&mut self, size: BoardSize) {
        self.slots.push(ZorroIISlot::Ram(ZorroIIRamBoard::new(size)));
    }

    /// Number of Zorro II slots.
    #[must_use]
    pub fn slot_count(&self) -> usize {
        self.slots.len()
    }

    /// True when autoconfig is complete (all boards configured or shut up).
    #[must_use]
    pub fn autoconfig_complete(&self) -> bool {
        self.current_autoconfig >= self.slots.len()
    }

    /// Reset autoconfig state (all boards become unconfigured).
    pub fn reset(&mut self) {
        self.current_autoconfig = 0;
        for slot in &mut self.slots {
            match slot {
                ZorroIISlot::Ram(board) => board.base_addr = None,
            }
        }
    }

    /// Read a byte from the autoconfig space ($E80000–$E8007F).
    ///
    /// Returns the nybble-packed descriptor for the current unconfigured
    /// board, or $FF if autoconfig is complete.
    #[must_use]
    pub fn autoconfig_read(&self, addr: u32) -> u8 {
        if self.current_autoconfig >= self.slots.len() {
            return 0xFF;
        }
        let slot = &self.slots[self.current_autoconfig];
        let rom = slot.descriptor().build_rom();
        let offset = (addr & 0x7F) as usize;
        if offset < rom.len() {
            rom[offset]
        } else {
            0xFF
        }
    }

    /// Write a byte to the autoconfig space.
    ///
    /// Handles base-address assignment ($48/$4A) and shut-up ($4C).
    pub fn autoconfig_write(&mut self, addr: u32, val: u8) {
        if self.current_autoconfig >= self.slots.len() {
            return;
        }

        let offset = addr & 0xFF;
        match offset as u32 {
            ac_reg::BASE_HI => {
                // Base address high byte: val maps to A23:A16.
                // For Zorro II, boards live in $200000–$9FFFFF.
                let base = u32::from(val) << 16;
                self.slots[self.current_autoconfig].set_base_addr(base);
                self.current_autoconfig += 1;
            }
            ac_reg::BASE_LO => {
                // Refine address with A15:A8 (for boards ≤ 64K).
                // Updates the most recently configured board.
                if let Some(prev) = self.current_autoconfig.checked_sub(1) {
                    if let Some(base) = self.slots.get(prev).and_then(|s| s.base_addr()) {
                        let refined = base | (u32::from(val) << 8);
                        self.slots[prev].set_base_addr(refined);
                    }
                }
            }
            ac_reg::SHUTUP => {
                // Board shut up — skip to next.
                self.current_autoconfig += 1;
            }
            _ => {} // Other writes ignored during autoconfig.
        }
    }

    /// Check if a 24-bit address falls within any configured board's range.
    /// Returns `Some(byte)` for reads, or `true` via `board_write` for writes.
    #[must_use]
    pub fn board_read(&self, addr: u32) -> Option<u8> {
        let addr24 = addr & 0xFF_FFFF;
        for slot in &self.slots {
            if let Some(base) = slot.base_addr() {
                let end = base + slot.size();
                if addr24 >= base && addr24 < end {
                    return Some(slot.read_byte(addr24 - base));
                }
            }
        }
        None
    }

    /// Write a byte to a configured board. Returns `true` if a board
    /// claimed the address.
    pub fn board_write(&mut self, addr: u32, val: u8) -> bool {
        let addr24 = addr & 0xFF_FFFF;
        for slot in &mut self.slots {
            if let Some(base) = slot.base_addr() {
                let end = base + slot.size();
                if addr24 >= base && addr24 < end {
                    slot.write_byte(addr24 - base, val);
                    return true;
                }
            }
        }
        false
    }
}

impl Default for Buster {
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
    fn empty_buster_autoconfig_returns_ff() {
        let b = Buster::new();
        assert_eq!(b.autoconfig_read(0xE8_0000), 0xFF);
        assert!(b.autoconfig_complete());
    }

    #[test]
    fn single_ram_board_visible_at_autoconfig() {
        let mut b = Buster::new();
        b.add_ram(BoardSize::Size512K);

        assert!(!b.autoconfig_complete());

        // Read type byte high nybble (offset $00).
        let type_hi = b.autoconfig_read(0xE8_0000);
        // Should be inverted $C4 (Zorro II memory, 512K size code = 4).
        assert_eq!(type_hi, !0xC4u8);
    }

    #[test]
    fn configure_board_at_base_address() {
        let mut b = Buster::new();
        b.add_ram(BoardSize::Size512K);

        // Write base address $200000 (high nybble = $20 → val bits 7:4 = 2).
        b.autoconfig_write(0xE8_0000 | ac_reg::BASE_HI, 0x20);

        // Board should now be configured.
        assert!(b.autoconfig_complete());

        // Write data and read it back.
        assert!(b.board_write(0x20_0000, 0x42));
        assert_eq!(b.board_read(0x20_0000), Some(0x42));
    }

    #[test]
    fn shutup_skips_board() {
        let mut b = Buster::new();
        b.add_ram(BoardSize::Size512K);

        b.autoconfig_write(0xE8_0000 | ac_reg::SHUTUP, 0);

        assert!(b.autoconfig_complete());
        // Board was shut up — no base address, board_read returns None.
        assert_eq!(b.board_read(0x20_0000), None);
    }

    #[test]
    fn two_boards_configure_sequentially() {
        let mut b = Buster::new();
        b.add_ram(BoardSize::Size512K);
        b.add_ram(BoardSize::Size512K);

        assert!(!b.autoconfig_complete());

        // Configure first board at $200000.
        b.autoconfig_write(0xE8_0000 | ac_reg::BASE_HI, 0x20);
        assert!(!b.autoconfig_complete());

        // Second board should now be visible at autoconfig.
        let type_hi = b.autoconfig_read(0xE8_0000);
        assert_ne!(type_hi, 0xFF);

        // Configure second board at $280000.
        b.autoconfig_write(0xE8_0000 | ac_reg::BASE_HI, 0x28);
        assert!(b.autoconfig_complete());

        // Both boards should be accessible.
        b.board_write(0x20_0000, 0xAA);
        b.board_write(0x28_0000, 0xBB);
        assert_eq!(b.board_read(0x20_0000), Some(0xAA));
        assert_eq!(b.board_read(0x28_0000), Some(0xBB));
    }

    #[test]
    fn board_read_outside_range_returns_none() {
        let mut b = Buster::new();
        b.add_ram(BoardSize::Size64K);
        b.autoconfig_write(0xE8_0000 | ac_reg::BASE_HI, 0x20);

        // $200000 + 64K = $210000. Reading $210000 should miss.
        assert_eq!(b.board_read(0x21_0000), None);
    }

    #[test]
    fn reset_unconfigures_all_boards() {
        let mut b = Buster::new();
        b.add_ram(BoardSize::Size512K);
        b.autoconfig_write(0xE8_0000 | ac_reg::BASE_HI, 0x20);
        assert!(b.autoconfig_complete());

        b.reset();

        assert!(!b.autoconfig_complete());
        assert_eq!(b.board_read(0x20_0000), None);
    }

    #[test]
    fn descriptor_product_and_manufacturer_readable() {
        let mut b = Buster::new();
        let mut board = ZorroIIRamBoard::new(BoardSize::Size1M);
        board.descriptor.product = 0xAB;
        board.descriptor.manufacturer = 0x1234;
        b.add_slot(ZorroIISlot::Ram(board));

        // Product high nybble at $04, low at $06 (inverted).
        let prod_hi = b.autoconfig_read(0xE8_0004);
        let prod_lo = b.autoconfig_read(0xE8_0006);
        assert_eq!(!prod_hi & 0x0F, 0x0A); // $AB >> 4 = $A
        assert_eq!(!prod_lo & 0x0F, 0x0B); // $AB & $F = $B

        // Manufacturer at $10-$16.
        let mfr_0 = !b.autoconfig_read(0xE8_0010) & 0x0F;
        let mfr_1 = !b.autoconfig_read(0xE8_0012) & 0x0F;
        let mfr_2 = !b.autoconfig_read(0xE8_0014) & 0x0F;
        let mfr_3 = !b.autoconfig_read(0xE8_0016) & 0x0F;
        let mfr = (u16::from(mfr_0) << 12) | (u16::from(mfr_1) << 8)
            | (u16::from(mfr_2) << 4) | u16::from(mfr_3);
        assert_eq!(mfr, 0x1234);
    }

    #[test]
    fn board_size_roundtrip() {
        let sizes = [
            BoardSize::Size64K, BoardSize::Size128K, BoardSize::Size256K,
            BoardSize::Size512K, BoardSize::Size1M, BoardSize::Size2M,
            BoardSize::Size4M, BoardSize::Size8M,
        ];
        for size in sizes {
            assert_eq!(BoardSize::from_code(size.code()), size);
        }
    }

    #[test]
    fn board_write_returns_false_for_unclaimed_address() {
        let mut b = Buster::new();
        b.add_ram(BoardSize::Size512K);
        b.autoconfig_write(0xE8_0000 | ac_reg::BASE_HI, 0x20);

        assert!(!b.board_write(0x50_0000, 0x42));
    }

    #[test]
    fn slot_count_reflects_added_boards() {
        let mut b = Buster::new();
        assert_eq!(b.slot_count(), 0);
        b.add_ram(BoardSize::Size512K);
        assert_eq!(b.slot_count(), 1);
        b.add_ram(BoardSize::Size1M);
        assert_eq!(b.slot_count(), 2);
    }
}
