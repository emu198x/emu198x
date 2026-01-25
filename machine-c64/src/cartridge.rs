//! C64 cartridge support.
//!
//! Cartridges connect to the expansion port and can provide ROM banks
//! that map into the C64's memory space. The GAME and EXROM control
//! lines determine how memory is banked:
//!
//! | EXROM | GAME | Mode                                              |
//! |-------|------|---------------------------------------------------|
//! |   1   |   1  | Normal (no cartridge effect)                      |
//! |   0   |   1  | 8K cart: ROML at $8000-$9FFF                      |
//! |   0   |   0  | 16K cart: ROML at $8000, ROMH at $A000            |
//! |   1   |   0  | Ultimax: ROML at $8000, ROMH at $E000, no BASIC   |
//!
//! # Supported Formats
//!
//! - `.crt` - Standard cartridge image format with chip headers
//! - Raw ROM files (8K or 16K, detected by size)

/// CRT file signature.
const CRT_SIGNATURE: &[u8; 16] = b"C64 CARTRIDGE   ";

/// Cartridge type IDs from .crt format.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u16)]
pub enum CartridgeType {
    /// Normal cartridge (8K or 16K ROM)
    Normal = 0,
    /// Action Replay
    ActionReplay = 1,
    /// KCS Power Cartridge
    KcsPower = 2,
    /// Final Cartridge III
    FinalCartridgeIii = 3,
    /// Simons' BASIC
    SimonsBasic = 4,
    /// Ocean type 1 (up to 512K)
    Ocean1 = 5,
    /// Expert Cartridge
    Expert = 6,
    /// Fun Play / Power Play
    FunPlay = 7,
    /// Super Games
    SuperGames = 8,
    /// Atomic Power
    AtomicPower = 9,
    /// Epyx Fastload
    EpyxFastload = 10,
    /// Westermann Learning
    Westermann = 11,
    /// Rex Utility
    Rex = 12,
    /// Final Cartridge I
    FinalCartridgeI = 13,
    /// Magic Formel
    MagicFormel = 14,
    /// C64 Game System / System 3
    C64GameSystem = 15,
    /// Warp Speed
    WarpSpeed = 16,
    /// Dinamic
    Dinamic = 17,
    /// Zaxxon / Super Zaxxon
    Zaxxon = 18,
    /// Magic Desk / Domark / HES Australia
    MagicDesk = 19,
    /// Super Snapshot V5
    SuperSnapshot5 = 20,
    /// Comal-80
    Comal80 = 21,
    /// EasyFlash
    EasyFlash = 32,
    /// Unknown type
    Unknown = 0xFFFF,
}

impl From<u16> for CartridgeType {
    fn from(value: u16) -> Self {
        match value {
            0 => CartridgeType::Normal,
            1 => CartridgeType::ActionReplay,
            2 => CartridgeType::KcsPower,
            3 => CartridgeType::FinalCartridgeIii,
            4 => CartridgeType::SimonsBasic,
            5 => CartridgeType::Ocean1,
            6 => CartridgeType::Expert,
            7 => CartridgeType::FunPlay,
            8 => CartridgeType::SuperGames,
            9 => CartridgeType::AtomicPower,
            10 => CartridgeType::EpyxFastload,
            11 => CartridgeType::Westermann,
            12 => CartridgeType::Rex,
            13 => CartridgeType::FinalCartridgeI,
            14 => CartridgeType::MagicFormel,
            15 => CartridgeType::C64GameSystem,
            16 => CartridgeType::WarpSpeed,
            17 => CartridgeType::Dinamic,
            18 => CartridgeType::Zaxxon,
            19 => CartridgeType::MagicDesk,
            20 => CartridgeType::SuperSnapshot5,
            21 => CartridgeType::Comal80,
            32 => CartridgeType::EasyFlash,
            _ => CartridgeType::Unknown,
        }
    }
}

/// A chip (ROM bank) within a cartridge.
#[derive(Clone)]
pub struct CartridgeChip {
    /// Chip type (0 = ROM, 1 = RAM, 2 = Flash)
    pub chip_type: u16,
    /// Bank number
    pub bank: u16,
    /// Load address ($8000 for ROML, $A000 or $E000 for ROMH)
    pub load_address: u16,
    /// ROM data
    pub data: Vec<u8>,
}

/// Loaded cartridge.
#[derive(Clone)]
pub struct Cartridge {
    /// Cartridge type
    pub cart_type: CartridgeType,
    /// EXROM line state (directly from CRT header)
    pub exrom: bool,
    /// GAME line state (directly from CRT header)
    pub game: bool,
    /// Cartridge name (from CRT header)
    pub name: String,
    /// ROM chips
    pub chips: Vec<CartridgeChip>,
    /// Currently selected bank for ROML
    pub current_bank_lo: usize,
    /// Currently selected bank for ROMH
    pub current_bank_hi: usize,
    /// Cartridge RAM (for cartridges that have RAM)
    pub ram: Vec<u8>,
    /// Whether cartridge is active (some can be disabled)
    pub active: bool,
}

impl Default for Cartridge {
    fn default() -> Self {
        Self::none()
    }
}

impl Cartridge {
    /// Create an empty (no cartridge) state.
    pub fn none() -> Self {
        Self {
            cart_type: CartridgeType::Normal,
            exrom: true, // High = no effect
            game: true,  // High = no effect
            name: String::new(),
            chips: Vec::new(),
            current_bank_lo: 0,
            current_bank_hi: 0,
            ram: Vec::new(),
            active: false,
        }
    }

    /// Check if a cartridge is inserted.
    pub fn is_inserted(&self) -> bool {
        self.active && !self.chips.is_empty()
    }

    /// Get the EXROM line state (active low in hardware, but we use true=asserted).
    pub fn exrom_active(&self) -> bool {
        self.active && !self.exrom
    }

    /// Get the GAME line state (active low in hardware, but we use true=asserted).
    pub fn game_active(&self) -> bool {
        self.active && !self.game
    }

    /// Read from ROML area ($8000-$9FFF).
    pub fn read_roml(&self, addr: u16) -> Option<u8> {
        if !self.active {
            return None;
        }

        let offset = (addr - 0x8000) as usize;
        self.find_chip(0x8000, self.current_bank_lo)
            .and_then(|chip| chip.data.get(offset).copied())
    }

    /// Read from ROMH area ($A000-$BFFF or $E000-$FFFF in Ultimax mode).
    pub fn read_romh(&self, addr: u16) -> Option<u8> {
        if !self.active {
            return None;
        }

        // ROMH can be at $A000 (16K mode) or $E000 (Ultimax)
        let (base, search_addr) = if addr >= 0xE000 {
            (0xE000u16, 0xE000u16)
        } else {
            (0xA000u16, 0xA000u16)
        };

        let offset = (addr - base) as usize;
        self.find_chip(search_addr, self.current_bank_hi)
            .and_then(|chip| chip.data.get(offset).copied())
    }

    /// Find a chip by load address and bank.
    fn find_chip(&self, load_addr: u16, bank: usize) -> Option<&CartridgeChip> {
        self.chips
            .iter()
            .find(|c| c.load_address == load_addr && c.bank as usize == bank)
    }

    /// Write to cartridge I/O area ($DE00-$DFFF).
    /// Returns true if the write was handled.
    pub fn write_io(&mut self, addr: u16, value: u8) -> bool {
        if !self.active {
            return false;
        }

        match self.cart_type {
            CartridgeType::Normal => false, // Normal carts don't handle I/O writes

            CartridgeType::Ocean1 => {
                // Ocean type 1: bank select via $DE00
                if addr == 0xDE00 {
                    self.current_bank_lo = (value & 0x3F) as usize;
                    self.current_bank_hi = self.current_bank_lo;
                    true
                } else {
                    false
                }
            }

            CartridgeType::MagicDesk | CartridgeType::Dinamic => {
                // Magic Desk / Domark / HES: bank select via $DE00
                if addr == 0xDE00 {
                    if value & 0x80 != 0 {
                        // Bit 7 set = disable cartridge
                        self.active = false;
                    } else {
                        self.current_bank_lo = (value & 0x3F) as usize;
                    }
                    true
                } else {
                    false
                }
            }

            CartridgeType::C64GameSystem => {
                // C64 Game System / System 3: bank in low bits of address
                if (0xDE00..=0xDEFF).contains(&addr) {
                    self.current_bank_lo = (addr & 0x3F) as usize;
                    true
                } else {
                    false
                }
            }

            _ => false,
        }
    }

    /// Read from cartridge I/O area ($DE00-$DFFF).
    pub fn read_io(&self, addr: u16) -> Option<u8> {
        if !self.active {
            return None;
        }

        match self.cart_type {
            CartridgeType::C64GameSystem => {
                // C64 Game System: reading from $DE00-$DEFF selects bank
                if (0xDE00..=0xDEFF).contains(&addr) {
                    // Bank select happens on read too, but we return open bus
                    Some(0xFF)
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    /// Load a .crt cartridge image.
    pub fn load_crt(data: &[u8]) -> Result<Self, &'static str> {
        if data.len() < 64 {
            return Err("CRT file too small");
        }

        // Check signature
        if &data[0..16] != CRT_SIGNATURE {
            return Err("Invalid CRT signature");
        }

        // Parse header
        let header_len = u32::from_be_bytes([data[16], data[17], data[18], data[19]]) as usize;
        let _version_hi = data[20];
        let _version_lo = data[21];
        let cart_type = u16::from_be_bytes([data[22], data[23]]);
        let exrom = data[24] == 0; // 0 = active (directly maps to line state)
        let game = data[25] == 0; // 0 = active

        // Get cartridge name (up to 32 bytes, null-terminated)
        let name_bytes = &data[32..64.min(header_len)];
        let name = name_bytes
            .iter()
            .take_while(|&&b| b != 0)
            .map(|&b| b as char)
            .collect::<String>();

        let cart_type = CartridgeType::from(cart_type);

        // Validate we support this type
        match cart_type {
            CartridgeType::Normal
            | CartridgeType::Ocean1
            | CartridgeType::MagicDesk
            | CartridgeType::Dinamic
            | CartridgeType::C64GameSystem
            | CartridgeType::SimonsBasic => {}
            CartridgeType::Unknown => return Err("Unknown cartridge type"),
            _ => return Err("Unsupported cartridge type (bank switching not implemented)"),
        }

        // Parse CHIP packets
        let mut chips = Vec::new();
        let mut offset = header_len;

        while offset + 16 <= data.len() {
            // Check for CHIP signature
            if &data[offset..offset + 4] != b"CHIP" {
                break;
            }

            let packet_len =
                u32::from_be_bytes([data[offset + 4], data[offset + 5], data[offset + 6], data[offset + 7]])
                    as usize;

            let chip_type = u16::from_be_bytes([data[offset + 8], data[offset + 9]]);
            let bank = u16::from_be_bytes([data[offset + 10], data[offset + 11]]);
            let load_address = u16::from_be_bytes([data[offset + 12], data[offset + 13]]);
            let rom_size = u16::from_be_bytes([data[offset + 14], data[offset + 15]]) as usize;

            // Validate load address
            if load_address != 0x8000 && load_address != 0xA000 && load_address != 0xE000 {
                return Err("Invalid chip load address");
            }

            let rom_start = offset + 16;
            let rom_end = rom_start + rom_size;

            if rom_end > data.len() {
                return Err("CRT file truncated");
            }

            chips.push(CartridgeChip {
                chip_type,
                bank,
                load_address,
                data: data[rom_start..rom_end].to_vec(),
            });

            offset += packet_len;
        }

        if chips.is_empty() {
            return Err("No ROM chips found in CRT");
        }

        Ok(Self {
            cart_type,
            exrom,
            game,
            name,
            chips,
            current_bank_lo: 0,
            current_bank_hi: 0,
            ram: Vec::new(),
            active: true,
        })
    }

    /// Load a raw ROM file (auto-detect 8K or 16K).
    pub fn load_raw(data: &[u8]) -> Result<Self, &'static str> {
        match data.len() {
            8192 => {
                // 8K cartridge - ROML only
                Ok(Self {
                    cart_type: CartridgeType::Normal,
                    exrom: false, // Active
                    game: true,   // 8K mode
                    name: String::from("8K Cartridge"),
                    chips: vec![CartridgeChip {
                        chip_type: 0,
                        bank: 0,
                        load_address: 0x8000,
                        data: data.to_vec(),
                    }],
                    current_bank_lo: 0,
                    current_bank_hi: 0,
                    ram: Vec::new(),
                    active: true,
                })
            }
            16384 => {
                // 16K cartridge - ROML + ROMH
                Ok(Self {
                    cart_type: CartridgeType::Normal,
                    exrom: false, // Active
                    game: false,  // 16K mode
                    name: String::from("16K Cartridge"),
                    chips: vec![
                        CartridgeChip {
                            chip_type: 0,
                            bank: 0,
                            load_address: 0x8000,
                            data: data[..8192].to_vec(),
                        },
                        CartridgeChip {
                            chip_type: 0,
                            bank: 0,
                            load_address: 0xA000,
                            data: data[8192..].to_vec(),
                        },
                    ],
                    current_bank_lo: 0,
                    current_bank_hi: 0,
                    ram: Vec::new(),
                    active: true,
                })
            }
            _ => Err("Invalid ROM size (expected 8K or 16K)"),
        }
    }

    /// Check for CBM80 auto-start signature at $8004.
    pub fn has_autostart(&self) -> bool {
        if let Some(chip) = self.find_chip(0x8000, 0) {
            // CBM80 signature at offset 4 (address $8004)
            if chip.data.len() >= 9 {
                return &chip.data[4..9] == b"CBM80";
            }
        }
        false
    }

    /// Get the cold start vector (address $8000-$8001).
    pub fn cold_start_vector(&self) -> Option<u16> {
        if let Some(chip) = self.find_chip(0x8000, 0) {
            if chip.data.len() >= 2 {
                return Some(u16::from_le_bytes([chip.data[0], chip.data[1]]));
            }
        }
        None
    }

    /// Get the warm start vector (address $8002-$8003).
    pub fn warm_start_vector(&self) -> Option<u16> {
        if let Some(chip) = self.find_chip(0x8000, 0) {
            if chip.data.len() >= 4 {
                return Some(u16::from_le_bytes([chip.data[2], chip.data[3]]));
            }
        }
        None
    }

    /// Get the cartridge name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Get the cartridge type.
    pub fn cart_type(&self) -> CartridgeType {
        self.cart_type
    }

    /// Get total ROM size in bytes.
    pub fn rom_size(&self) -> usize {
        self.chips.iter().map(|c| c.data.len()).sum()
    }

    /// Get number of banks.
    pub fn num_banks(&self) -> usize {
        self.chips
            .iter()
            .map(|c| c.bank as usize + 1)
            .max()
            .unwrap_or(0)
    }
}
