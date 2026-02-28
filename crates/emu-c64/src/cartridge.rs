//! CRT cartridge format parser and runtime state.
//!
//! The CRT format wraps C64 cartridge ROM images with a header describing
//! the cartridge type and EXROM/GAME line configuration. ROM data is stored
//! in CHIP packets, each specifying a load address and bank number.
//!
//! Supported types:
//! - Type 0 (Normal): 8K at $8000 or 16K at $8000+$A000. No bankswitching.
//! - Type 1 (Action Replay): 4x8K banks at ROML, mode switching via $DE00.
//! - Type 4 (Simon's BASIC): 2x8K: ROML + ROMH, toggled via $DE00.
//! - Type 5 (Ocean): Up to 64 x 8K banks at $8000, selected via $DE00.
//! - Type 10 (Fun Play / Power Play): 16x8K banks at ROML via $DE00.
//! - Type 19 (Magic Desk): Up to 128 x 8K banks at $8000, selected via $DE00.
//!   Bit 7 of the bank register disables the cartridge (EXROM=1).
//! - Type 32 (EasyFlash): 64x8K dual banks (ROML+ROMH), 256B RAM at $DF00,
//!   control registers at $DE00/$DE02.

#![allow(clippy::cast_possible_truncation)]

/// Cartridge hardware type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CartridgeType {
    /// Type 0: 8K or 16K, no bankswitching.
    Normal,
    /// Type 1: Action Replay — 4x8K banks, mode switching.
    ActionReplay,
    /// Type 4: Simon's BASIC — 2x8K, ROML+ROMH toggled via $DE00.
    SimonsBasic,
    /// Type 5: up to 64 x 8K banks at $8000, selected via $DE00.
    Ocean,
    /// Type 10: Fun Play / Power Play — 16x8K banks at ROML via $DE00.
    FunPlay,
    /// Type 19: up to 128 x 8K banks at $8000, selected via $DE00.
    MagicDesk,
    /// Type 32: EasyFlash — 64x8K dual banks, 256B RAM, control regs.
    EasyFlash,
}

/// A loaded CRT cartridge.
#[derive(Debug, Clone)]
pub struct Cartridge {
    /// Hardware type.
    pub cart_type: CartridgeType,
    /// EXROM line state (active low: false = active/asserted).
    pub exrom: bool,
    /// GAME line state (active low: false = active/asserted).
    pub game: bool,
    /// ROML banks (8K each, mapped at $8000-$9FFF).
    pub roml: Vec<Vec<u8>>,
    /// ROMH banks (8K each, mapped at $A000-$BFFF or $E000-$FFFF).
    pub romh: Vec<Vec<u8>>,
    /// Current bank index (for bankswitched types).
    pub bank: u8,
    /// EasyFlash: 256 bytes of on-cartridge RAM at $DF00-$DFFF.
    pub ef_ram: [u8; 256],
    /// EasyFlash: control register ($DE02) value.
    pub ef_control: u8,
}

/// CRT file signature.
const CRT_SIGNATURE: &[u8; 16] = b"C64 CARTRIDGE   ";

/// CHIP packet signature.
const CHIP_SIGNATURE: &[u8; 4] = b"CHIP";

impl Cartridge {
    /// Read from the current ROML bank at the given offset (0-8191).
    #[must_use]
    pub fn read_roml(&self, offset: u16) -> u8 {
        let bank = self.bank as usize;
        if bank < self.roml.len() {
            let off = offset as usize;
            if off < self.roml[bank].len() {
                return self.roml[bank][off];
            }
        }
        0xFF
    }

    /// Read from the current ROMH bank at the given offset (0-8191).
    #[must_use]
    pub fn read_romh(&self, offset: u16) -> u8 {
        let bank = match self.cart_type {
            CartridgeType::Normal | CartridgeType::SimonsBasic => 0usize,
            CartridgeType::EasyFlash
            | CartridgeType::Ocean
            | CartridgeType::MagicDesk
            | CartridgeType::ActionReplay
            | CartridgeType::FunPlay => self.bank as usize,
        };
        if bank < self.romh.len() {
            let off = offset as usize;
            if off < self.romh[bank].len() {
                return self.romh[bank][off];
            }
        }
        0xFF
    }

    /// Handle a write to the I/O expansion area ($DE00-$DFFF).
    pub fn write_io(&mut self, addr: u16, value: u8) {
        match self.cart_type {
            CartridgeType::Normal => {} // No bankswitching
            CartridgeType::ActionReplay => {
                if addr == 0xDE00 {
                    // Bits 0-1: bank select. Bits 2-3: EXROM/GAME mode.
                    self.bank = value & 0x03;
                    self.exrom = value & 0x04 != 0;
                    self.game = value & 0x08 != 0;
                }
            }
            CartridgeType::SimonsBasic => {
                if addr == 0xDE00 {
                    // Toggle between ROML ($8000) and ROMH ($A000)
                    self.game = !self.game;
                }
            }
            CartridgeType::Ocean => {
                if addr == 0xDE00 {
                    self.bank = value & 0x3F;
                }
            }
            CartridgeType::FunPlay => {
                if addr == 0xDE00 {
                    self.bank = value & 0x0F;
                }
            }
            CartridgeType::MagicDesk => {
                if addr == 0xDE00 {
                    self.bank = value & 0x7F;
                    // Bit 7: 1 = disable cartridge (EXROM=1)
                    self.exrom = value & 0x80 != 0;
                }
            }
            CartridgeType::EasyFlash => {
                match addr {
                    0xDE00 => {
                        // Bits 0-5: bank select, bit 7: LED
                        self.bank = value & 0x3F;
                    }
                    0xDE02 => {
                        // Bit 0: GAME, bit 1: EXROM, bit 2: cartridge off
                        self.ef_control = value;
                        if value & 0x04 != 0 {
                            // Cartridge off: release both lines
                            self.exrom = true;
                            self.game = true;
                        } else {
                            self.game = value & 0x01 == 0;   // Active low
                            self.exrom = value & 0x02 == 0;  // Active low
                        }
                    }
                    0xDF00..=0xDFFF => {
                        self.ef_ram[(addr - 0xDF00) as usize] = value;
                    }
                    _ => {}
                }
            }
        }
    }

    /// Read from I/O expansion area ($DE00-$DFFF).
    #[must_use]
    pub fn read_io(&self, addr: u16) -> u8 {
        match self.cart_type {
            CartridgeType::EasyFlash => {
                if (0xDF00..=0xDFFF).contains(&addr) {
                    return self.ef_ram[(addr - 0xDF00) as usize];
                }
                0xFF
            }
            _ => 0xFF,
        }
    }
}

/// Read a big-endian u16 from a byte slice.
fn read_be_u16(data: &[u8], offset: usize) -> u16 {
    u16::from(data[offset]) << 8 | u16::from(data[offset + 1])
}

/// Read a big-endian u32 from a byte slice.
fn read_be_u32(data: &[u8], offset: usize) -> u32 {
    u32::from(data[offset]) << 24
        | u32::from(data[offset + 1]) << 16
        | u32::from(data[offset + 2]) << 8
        | u32::from(data[offset + 3])
}

/// Parse a CRT file into a `Cartridge`.
///
/// # Errors
///
/// Returns an error for invalid signatures, unsupported cartridge types,
/// or malformed CHIP packets.
pub fn parse_crt(data: &[u8]) -> Result<Cartridge, String> {
    if data.len() < 64 {
        return Err("CRT file too short for header".to_string());
    }

    // Validate signature
    if &data[0..16] != CRT_SIGNATURE {
        return Err("Invalid CRT signature".to_string());
    }

    // Header length (offset 0x10, big-endian u32)
    let header_len = read_be_u32(data, 0x10) as usize;
    if header_len < 32 || header_len > data.len() {
        return Err(format!("Invalid CRT header length: {header_len}"));
    }

    // Cartridge type (offset 0x16, big-endian u16)
    let type_id = read_be_u16(data, 0x16);
    let cart_type = match type_id {
        0 => CartridgeType::Normal,
        1 => CartridgeType::ActionReplay,
        4 => CartridgeType::SimonsBasic,
        5 => CartridgeType::Ocean,
        10 => CartridgeType::FunPlay,
        19 => CartridgeType::MagicDesk,
        32 => CartridgeType::EasyFlash,
        _ => return Err(format!("Unsupported CRT type: {type_id}")),
    };

    // EXROM line (offset 0x18)
    let exrom = data[0x18] != 0;
    // GAME line (offset 0x19)
    let game = data[0x19] != 0;

    // Parse CHIP packets
    let mut roml: Vec<Vec<u8>> = Vec::new();
    let mut romh: Vec<Vec<u8>> = Vec::new();
    let mut offset = header_len;

    while offset + 16 <= data.len() {
        // Validate CHIP signature
        if &data[offset..offset + 4] != CHIP_SIGNATURE {
            return Err(format!(
                "Expected CHIP signature at offset {offset}, got {:?}",
                &data[offset..offset + 4]
            ));
        }

        // CHIP packet total length (offset+4, big-endian u32)
        let chip_len = read_be_u32(data, offset + 4) as usize;
        if chip_len < 16 || offset + chip_len > data.len() {
            return Err(format!(
                "Invalid CHIP packet length {chip_len} at offset {offset}"
            ));
        }

        // Load address (offset+8, big-endian u16) — but CRT spec says offset+0x0C
        // Actually: CHIP header is:
        //   +0: "CHIP" (4 bytes)
        //   +4: total packet length (4 bytes, BE)
        //   +8: chip type (2 bytes, BE) — 0=ROM, 1=RAM, 2=Flash
        //   +A: bank number (2 bytes, BE)
        //   +C: load address (2 bytes, BE)
        //   +E: ROM size (2 bytes, BE)
        //   +10: ROM data...
        let bank = read_be_u16(data, offset + 0x0A) as usize;
        let load_addr = read_be_u16(data, offset + 0x0C);
        let rom_size = read_be_u16(data, offset + 0x0E) as usize;

        let rom_start = offset + 0x10;
        let rom_end = rom_start + rom_size;
        if rom_end > data.len() {
            return Err(format!(
                "CHIP ROM data extends past end of file at offset {offset}"
            ));
        }

        let rom_data = data[rom_start..rom_end].to_vec();

        match load_addr {
            0x8000 => {
                // ROML: ensure Vec is large enough
                while roml.len() <= bank {
                    roml.push(Vec::new());
                }
                roml[bank] = rom_data;
            }
            0xA000 | 0xE000 => {
                // ROMH
                while romh.len() <= bank {
                    romh.push(Vec::new());
                }
                romh[bank] = rom_data;
            }
            _ => {
                return Err(format!(
                    "Unexpected CHIP load address ${load_addr:04X} at offset {offset}"
                ));
            }
        }

        offset += chip_len;
    }

    if roml.is_empty() && romh.is_empty() {
        return Err("CRT file contains no CHIP packets".to_string());
    }

    Ok(Cartridge {
        cart_type,
        exrom,
        game,
        roml,
        romh,
        bank: 0,
        ef_ram: [0; 256],
        ef_control: 0,
    })
}

/// Extract the cartridge name from a CRT header (up to 32 bytes at offset 0x20).
#[must_use]
pub fn crt_name(data: &[u8]) -> String {
    if data.len() < 0x40 {
        return String::new();
    }
    let name_bytes = &data[0x20..0x40];
    let end = name_bytes.iter().position(|&b| b == 0).unwrap_or(name_bytes.len());
    String::from_utf8_lossy(&name_bytes[..end]).trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a minimal CRT file header.
    fn make_crt_header(cart_type: u16, exrom: u8, game: u8) -> Vec<u8> {
        let mut header = Vec::new();
        // Signature (16 bytes)
        header.extend_from_slice(CRT_SIGNATURE);
        // Header length (4 bytes, BE) = 64
        header.extend_from_slice(&[0x00, 0x00, 0x00, 0x40]);
        // Version (2 bytes, BE) = 1.0
        header.extend_from_slice(&[0x01, 0x00]);
        // Cartridge type (2 bytes, BE)
        header.push((cart_type >> 8) as u8);
        header.push(cart_type as u8);
        // EXROM line
        header.push(exrom);
        // GAME line
        header.push(game);
        // Reserved (6 bytes)
        header.extend_from_slice(&[0; 6]);
        // Name (32 bytes, padded with zeros)
        let name = b"Test Cart";
        header.extend_from_slice(name);
        header.extend_from_slice(&vec![0u8; 32 - name.len()]);
        assert_eq!(header.len(), 64);
        header
    }

    /// Build a CHIP packet.
    fn make_chip(bank: u16, load_addr: u16, rom_data: &[u8]) -> Vec<u8> {
        let rom_size = rom_data.len() as u16;
        let total_len: u32 = 0x10 + rom_data.len() as u32;
        let mut chip = Vec::new();
        // "CHIP" signature
        chip.extend_from_slice(CHIP_SIGNATURE);
        // Total packet length (4 bytes, BE)
        chip.extend_from_slice(&total_len.to_be_bytes());
        // Chip type (2 bytes, BE) = 0 (ROM)
        chip.extend_from_slice(&[0x00, 0x00]);
        // Bank number (2 bytes, BE)
        chip.push((bank >> 8) as u8);
        chip.push(bank as u8);
        // Load address (2 bytes, BE)
        chip.push((load_addr >> 8) as u8);
        chip.push(load_addr as u8);
        // ROM size (2 bytes, BE)
        chip.push((rom_size >> 8) as u8);
        chip.push(rom_size as u8);
        // ROM data
        chip.extend_from_slice(rom_data);
        chip
    }

    #[test]
    fn parse_crt_normal_8k() {
        let mut crt = make_crt_header(0, 0, 1); // EXROM=0, GAME=1 → 8K mode
        let rom = vec![0xAA; 8192];
        crt.extend(make_chip(0, 0x8000, &rom));

        let cart = parse_crt(&crt).expect("should parse");
        assert_eq!(cart.cart_type, CartridgeType::Normal);
        assert!(!cart.exrom); // EXROM=0 (active)
        assert!(cart.game); // GAME=1 (inactive)
        assert_eq!(cart.roml.len(), 1);
        assert_eq!(cart.roml[0].len(), 8192);
        assert_eq!(cart.roml[0][0], 0xAA);
        assert!(cart.romh.is_empty());
    }

    #[test]
    fn parse_crt_normal_16k() {
        let mut crt = make_crt_header(0, 0, 0); // EXROM=0, GAME=0 → 16K mode
        let roml = vec![0xBB; 8192];
        let romh = vec![0xCC; 8192];
        crt.extend(make_chip(0, 0x8000, &roml));
        crt.extend(make_chip(0, 0xA000, &romh));

        let cart = parse_crt(&crt).expect("should parse");
        assert_eq!(cart.cart_type, CartridgeType::Normal);
        assert!(!cart.exrom);
        assert!(!cart.game);
        assert_eq!(cart.roml.len(), 1);
        assert_eq!(cart.romh.len(), 1);
        assert_eq!(cart.roml[0][0], 0xBB);
        assert_eq!(cart.romh[0][0], 0xCC);
    }

    #[test]
    fn parse_crt_ocean() {
        let mut crt = make_crt_header(5, 0, 1); // Ocean: EXROM=0, GAME=1
        // Add 4 banks
        for bank in 0..4u16 {
            let rom = vec![bank as u8; 8192];
            crt.extend(make_chip(bank, 0x8000, &rom));
        }

        let cart = parse_crt(&crt).expect("should parse");
        assert_eq!(cart.cart_type, CartridgeType::Ocean);
        assert_eq!(cart.roml.len(), 4);
        assert_eq!(cart.roml[0][0], 0);
        assert_eq!(cart.roml[3][0], 3);
    }

    #[test]
    fn parse_crt_ocean_bank_switch() {
        let mut crt = make_crt_header(5, 0, 1);
        for bank in 0..4u16 {
            let rom = vec![(bank + 0x10) as u8; 8192];
            crt.extend(make_chip(bank, 0x8000, &rom));
        }

        let mut cart = parse_crt(&crt).expect("should parse");
        assert_eq!(cart.bank, 0);
        assert_eq!(cart.read_roml(0), 0x10);

        cart.write_io(0xDE00, 2);
        assert_eq!(cart.bank, 2);
        assert_eq!(cart.read_roml(0), 0x12);

        // Mask to 6 bits
        cart.write_io(0xDE00, 0xFF);
        assert_eq!(cart.bank, 0x3F);
    }

    #[test]
    fn parse_crt_magic_desk() {
        let mut crt = make_crt_header(19, 0, 1); // MagicDesk: EXROM=0, GAME=1
        for bank in 0..3u16 {
            let rom = vec![(bank + 0x20) as u8; 8192];
            crt.extend(make_chip(bank, 0x8000, &rom));
        }

        let mut cart = parse_crt(&crt).expect("should parse");
        assert_eq!(cart.cart_type, CartridgeType::MagicDesk);

        // Bank switch
        cart.write_io(0xDE00, 1);
        assert_eq!(cart.bank, 1);
        assert_eq!(cart.read_roml(0), 0x21);

        // Bit 7 disables cartridge
        assert!(!cart.exrom);
        cart.write_io(0xDE00, 0x80);
        assert!(cart.exrom); // Now disabled
        assert_eq!(cart.bank, 0); // Bank = 0x80 & 0x7F = 0

        // Re-enable
        cart.write_io(0xDE00, 0x02);
        assert!(!cart.exrom);
        assert_eq!(cart.bank, 2);
    }

    #[test]
    fn parse_crt_bad_signature() {
        let mut data = vec![0u8; 64];
        data[0..16].copy_from_slice(b"NOT A CARTRIDGE!");
        assert!(parse_crt(&data).is_err());
    }

    #[test]
    fn parse_crt_unsupported_type() {
        let crt = make_crt_header(99, 0, 0);
        let err = parse_crt(&crt).unwrap_err();
        assert!(err.contains("Unsupported CRT type: 99"), "got: {err}");
    }

    #[test]
    fn parse_crt_too_short() {
        assert!(parse_crt(&[0; 10]).is_err());
    }

    #[test]
    fn crt_name_extraction() {
        let crt = make_crt_header(0, 0, 1);
        assert_eq!(crt_name(&crt), "Test Cart");
    }
}
