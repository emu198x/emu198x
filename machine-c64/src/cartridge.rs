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
    /// Initial EXROM state (for reset)
    initial_exrom: bool,
    /// Initial GAME state (for reset)
    initial_game: bool,
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
    /// Epyx Fastload capacitor charge (counts down, disables at 0)
    epyx_capacitor: u32,
    /// Whether freeze button was pressed (triggers NMI)
    freeze_pending: bool,
    /// Whether cartridge ROM is enabled (for freezer cartridges)
    rom_enabled: bool,
    /// Whether cartridge RAM is enabled at $DF00-$DFFF
    ram_enabled: bool,
    /// Action Replay control register shadow
    ar_control: u8,
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
            initial_exrom: true,
            initial_game: true,
            name: String::new(),
            chips: Vec::new(),
            current_bank_lo: 0,
            current_bank_hi: 0,
            ram: Vec::new(),
            active: false,
            epyx_capacitor: 0,
            freeze_pending: false,
            rom_enabled: true,
            ram_enabled: false,
            ar_control: 0,
        }
    }

    /// Reset cartridge to initial state (called on C64 reset).
    pub fn reset(&mut self) {
        self.exrom = self.initial_exrom;
        self.game = self.initial_game;
        self.current_bank_lo = 0;
        self.current_bank_hi = 0;
        self.freeze_pending = false;
        self.rom_enabled = true;
        self.ram_enabled = false;
        self.ar_control = 0;

        if self.cart_type == CartridgeType::EpyxFastload {
            self.epyx_capacitor = 512; // ~512 cycles before timeout
        }

        // Re-enable if it was disabled by software
        if !self.chips.is_empty() {
            self.active = true;
        }
    }

    /// Press the freeze button (for freezer cartridges).
    /// Returns true if an NMI should be triggered.
    pub fn freeze(&mut self) -> bool {
        if !self.active {
            return false;
        }

        match self.cart_type {
            CartridgeType::ActionReplay | CartridgeType::AtomicPower => {
                // Action Replay: pressing freeze triggers NMI and enters freeze mode
                self.freeze_pending = true;
                // Set to Ultimax mode during freeze
                self.exrom = true;
                self.game = false;
                self.rom_enabled = true;
                true // Trigger NMI
            }
            CartridgeType::FinalCartridgeIii => {
                // Final Cartridge III: freeze triggers NMI
                self.freeze_pending = true;
                self.exrom = true;
                self.game = false;
                true
            }
            CartridgeType::SuperSnapshot5 => {
                // Super Snapshot V5: freeze triggers NMI
                self.freeze_pending = true;
                self.exrom = false;
                self.game = false;
                true
            }
            _ => false,
        }
    }

    /// Check if freeze is pending and clear the flag.
    pub fn take_freeze_pending(&mut self) -> bool {
        let pending = self.freeze_pending;
        self.freeze_pending = false;
        pending
    }

    /// Check if this is a freezer cartridge.
    pub fn is_freezer(&self) -> bool {
        matches!(
            self.cart_type,
            CartridgeType::ActionReplay
                | CartridgeType::AtomicPower
                | CartridgeType::FinalCartridgeIii
                | CartridgeType::FinalCartridgeI
                | CartridgeType::SuperSnapshot5
                | CartridgeType::Expert
        )
    }

    /// Check if cartridge RAM is enabled at $DF00 (for REU priority).
    pub fn ram_enabled_at_df00(&self) -> bool {
        self.active && self.ram_enabled
    }

    /// Tick the cartridge (for time-based cartridge behavior).
    /// Called once per CPU cycle.
    pub fn tick(&mut self) {
        match self.cart_type {
            CartridgeType::EpyxFastload => {
                // Epyx Fastload uses a capacitor that discharges over time
                if self.epyx_capacitor > 0 {
                    self.epyx_capacitor -= 1;
                    if self.epyx_capacitor == 0 {
                        // Capacitor discharged - cartridge becomes invisible
                        self.exrom = true;
                        self.game = true;
                    }
                }
            }
            _ => {}
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
    pub fn read_roml(&mut self, addr: u16) -> Option<u8> {
        if !self.active {
            return None;
        }

        match self.cart_type {
            CartridgeType::Zaxxon => {
                // Zaxxon has special mirrored ROM behavior:
                // - $8000-$8FFF: First 4K, also selects bank based on A12
                // - $9000-$9FFF: Second 4K from selected bank
                let offset = (addr & 0x0FFF) as usize;
                if addr < 0x9000 {
                    // Reading $8000-$8FFF selects bank 0 and returns first chip
                    self.current_bank_lo = 0;
                    self.find_chip(0x8000, 0)
                        .and_then(|chip| chip.data.get(offset).copied())
                } else {
                    // Reading $9000-$9FFF selects bank 1 and returns from offset+4K
                    self.current_bank_lo = 1;
                    self.find_chip(0x8000, 0)
                        .and_then(|chip| chip.data.get(offset + 0x1000).copied())
                }
            }
            _ => {
                let offset = (addr - 0x8000) as usize;
                self.find_chip(0x8000, self.current_bank_lo)
                    .and_then(|chip| chip.data.get(offset).copied())
            }
        }
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
            CartridgeType::Normal | CartridgeType::SimonsBasic => {
                // Normal carts don't handle I/O writes
                // Simons' BASIC uses standard 16K mode
                false
            }

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

            CartridgeType::EasyFlash => {
                // EasyFlash: $DE00 = bank, $DE02 = control
                match addr {
                    0xDE00 => {
                        // Bank register (6 bits = 64 banks)
                        self.current_bank_lo = (value & 0x3F) as usize;
                        self.current_bank_hi = self.current_bank_lo;
                        true
                    }
                    0xDE02 => {
                        // Control register:
                        // Bit 0: GAME (directly controls line)
                        // Bit 1: EXROM (directly controls line)
                        // Bit 2: LED (ignored in emulation)
                        // Bit 7: NMI (active low, ignored for now)
                        self.game = value & 0x01 == 0;
                        self.exrom = value & 0x02 == 0;
                        true
                    }
                    _ => false,
                }
            }

            CartridgeType::SuperGames | CartridgeType::FunPlay => {
                // Super Games / Fun Play: $DF00 = bank and mode control
                if addr == 0xDF00 {
                    let bank = (value & 0x03) as usize;
                    self.current_bank_lo = bank;
                    self.current_bank_hi = bank;

                    if value & 0x04 != 0 {
                        // Bit 2: disable cartridge
                        self.active = false;
                    } else if value & 0x08 != 0 {
                        // Bit 3: 16K mode (GAME=0, EXROM=0)
                        self.game = false;
                        self.exrom = false;
                    } else {
                        // 8K mode (GAME=1, EXROM=0)
                        self.game = true;
                        self.exrom = false;
                    }
                    true
                } else {
                    false
                }
            }

            CartridgeType::Comal80 => {
                // Comal-80: $DE00 = bank select
                if addr == 0xDE00 {
                    self.current_bank_lo = (value & 0x03) as usize;
                    self.current_bank_hi = self.current_bank_lo;
                    // Bit 7 controls EXROM (active low)
                    self.exrom = value & 0x80 == 0;
                    true
                } else {
                    false
                }
            }

            CartridgeType::Zaxxon => {
                // Zaxxon doesn't use I/O writes - bank select is via ROM reads
                false
            }

            CartridgeType::EpyxFastload => {
                // Epyx Fastload doesn't use I/O writes
                false
            }

            CartridgeType::ActionReplay | CartridgeType::AtomicPower => {
                // Action Replay / Atomic Power: $DE00 = control register
                if addr == 0xDE00 {
                    self.ar_control = value;

                    // Bit 0: Game line (directly controls)
                    // Bit 1: EXROM line (directly controls)
                    // Bit 2: Enable/disable cartridge ROM (active low in some versions)
                    // Bit 3: Enable RAM at $DF00-$DFFF
                    // Bit 4-5: Bank select
                    self.game = value & 0x01 == 0;
                    self.exrom = value & 0x02 == 0;
                    self.rom_enabled = value & 0x04 == 0;
                    self.ram_enabled = value & 0x08 != 0;

                    let bank = ((value >> 4) & 0x03) as usize;
                    self.current_bank_lo = bank;
                    self.current_bank_hi = bank;

                    // If bit 2 is set, disable the cartridge completely
                    if value & 0x04 != 0 && !self.freeze_pending {
                        self.active = false;
                    }
                    true
                } else if (0xDF00..=0xDFFF).contains(&addr) && self.ram_enabled {
                    // Write to cartridge RAM
                    let offset = (addr - 0xDF00) as usize;
                    if offset < self.ram.len() {
                        self.ram[offset] = value;
                    }
                    true
                } else {
                    false
                }
            }

            CartridgeType::FinalCartridgeIii => {
                // Final Cartridge III: $DFFF = control register
                if addr == 0xDFFF {
                    // Bits 0-1: Bank select (4 banks)
                    // Bit 4: NMI disable
                    // Bit 5: Freeze mode
                    // Bit 6: GAME line (active low)
                    // Bit 7: Hide cartridge (disable)
                    let bank = (value & 0x03) as usize;
                    self.current_bank_lo = bank;
                    self.current_bank_hi = bank;

                    self.game = value & 0x40 == 0;
                    self.exrom = false; // EXROM is always low on FC3

                    if value & 0x80 != 0 {
                        // Hide cartridge
                        self.exrom = true;
                        self.game = true;
                        if !self.freeze_pending {
                            self.active = false;
                        }
                    }
                    true
                } else if (0xDF00..=0xDFFE).contains(&addr) {
                    // FC3 RAM at $DF00-$DFFE
                    let offset = (addr - 0xDF00) as usize;
                    if offset < self.ram.len() {
                        self.ram[offset] = value;
                    }
                    true
                } else {
                    false
                }
            }

            CartridgeType::SuperSnapshot5 => {
                // Super Snapshot V5: $DE00 = control register
                if addr == 0xDE00 {
                    // Bit 0: GAME line
                    // Bit 1: EXROM line
                    // Bit 2: RAM at $8000
                    // Bit 3: RAM enable
                    // Bit 4-5: ROM bank
                    self.game = value & 0x01 != 0;
                    self.exrom = value & 0x02 != 0;
                    self.ram_enabled = value & 0x08 != 0;

                    let bank = ((value >> 4) & 0x03) as usize;
                    self.current_bank_lo = bank;
                    self.current_bank_hi = bank;
                    true
                } else if (0xDF00..=0xDFFF).contains(&addr) && self.ram_enabled {
                    // Write to cartridge RAM
                    let offset = (addr - 0xDF00) as usize;
                    if offset < self.ram.len() {
                        self.ram[offset] = value;
                    }
                    true
                } else {
                    false
                }
            }

            _ => false,
        }
    }

    /// Read from cartridge I/O area ($DE00-$DFFF).
    pub fn read_io(&mut self, addr: u16) -> Option<u8> {
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

            CartridgeType::EpyxFastload => {
                // Reading $DF00-$DFFF resets the capacitor and re-enables the cartridge
                if (0xDF00..=0xDFFF).contains(&addr) {
                    self.epyx_capacitor = 512;
                    self.exrom = false;
                    self.game = true;
                    Some(0xFF) // Open bus
                } else {
                    None
                }
            }

            CartridgeType::EasyFlash => {
                // EasyFlash: reads from $DE00 area return open bus
                if (0xDE00..=0xDEFF).contains(&addr) {
                    Some(0xFF)
                } else {
                    None
                }
            }

            CartridgeType::ActionReplay | CartridgeType::AtomicPower => {
                if (0xDF00..=0xDFFF).contains(&addr) && self.ram_enabled {
                    // Read from cartridge RAM
                    let offset = (addr - 0xDF00) as usize;
                    Some(self.ram.get(offset).copied().unwrap_or(0xFF))
                } else {
                    Some(0xFF) // Open bus
                }
            }

            CartridgeType::FinalCartridgeIii => {
                if (0xDF00..=0xDFFE).contains(&addr) {
                    // Read from FC3 RAM
                    let offset = (addr - 0xDF00) as usize;
                    Some(self.ram.get(offset).copied().unwrap_or(0xFF))
                } else if addr == 0xDFFF {
                    // Reading $DFFF returns open bus
                    Some(0xFF)
                } else {
                    None
                }
            }

            CartridgeType::SuperSnapshot5 => {
                if (0xDF00..=0xDFFF).contains(&addr) && self.ram_enabled {
                    // Read from SS5 RAM
                    let offset = (addr - 0xDF00) as usize;
                    Some(self.ram.get(offset).copied().unwrap_or(0xFF))
                } else {
                    Some(0xFF)
                }
            }

            _ => None,
        }
    }

    /// Read from $9E00-$9EFF area (special handling for Epyx Fastload).
    /// Call this from the memory read handler.
    pub fn read_roml_9exx(&mut self, addr: u16) -> Option<u8> {
        if !self.active {
            return None;
        }

        if self.cart_type == CartridgeType::EpyxFastload && (0x9E00..=0x9EFF).contains(&addr) {
            // Reading $9E00-$9EFF resets the Epyx capacitor
            self.epyx_capacitor = 512;
            self.exrom = false;
            self.game = true;
        }

        // Return the actual ROM data
        let offset = (addr - 0x8000) as usize;
        self.find_chip(0x8000, self.current_bank_lo)
            .and_then(|chip| chip.data.get(offset).copied())
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
            | CartridgeType::SimonsBasic
            | CartridgeType::EasyFlash
            | CartridgeType::Zaxxon
            | CartridgeType::EpyxFastload
            | CartridgeType::SuperGames
            | CartridgeType::FunPlay
            | CartridgeType::Comal80
            // Freezer cartridges
            | CartridgeType::ActionReplay
            | CartridgeType::AtomicPower
            | CartridgeType::FinalCartridgeIii
            | CartridgeType::SuperSnapshot5 => {}
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

        // Allocate RAM for cartridges that need it
        let ram = match cart_type {
            CartridgeType::EasyFlash => vec![0xFF; 256], // 256 bytes at $DF00
            CartridgeType::ActionReplay | CartridgeType::AtomicPower => {
                vec![0xFF; 8192] // 8K RAM
            }
            CartridgeType::FinalCartridgeIii => vec![0xFF; 256], // 256 bytes at $DF00-$DFFE
            CartridgeType::SuperSnapshot5 => vec![0xFF; 32768],  // 32K RAM
            _ => Vec::new(),
        };

        // Epyx Fastload starts with capacitor charged
        let epyx_capacitor = if cart_type == CartridgeType::EpyxFastload {
            512
        } else {
            0
        };

        Ok(Self {
            cart_type,
            exrom,
            game,
            initial_exrom: exrom,
            initial_game: game,
            name,
            chips,
            current_bank_lo: 0,
            current_bank_hi: 0,
            ram,
            active: true,
            epyx_capacitor,
            freeze_pending: false,
            rom_enabled: true,
            ram_enabled: false,
            ar_control: 0,
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
                    initial_exrom: false,
                    initial_game: true,
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
                    epyx_capacitor: 0,
                    freeze_pending: false,
                    rom_enabled: true,
                    ram_enabled: false,
                    ar_control: 0,
                })
            }
            16384 => {
                // 16K cartridge - ROML + ROMH
                Ok(Self {
                    cart_type: CartridgeType::Normal,
                    exrom: false, // Active
                    game: false,  // 16K mode
                    initial_exrom: false,
                    initial_game: false,
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
                    epyx_capacitor: 0,
                    freeze_pending: false,
                    rom_enabled: true,
                    ram_enabled: false,
                    ar_control: 0,
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
