//! iNES cartridge parser and mapper implementations.
//!
//! Parses the iNES file format (header + PRG ROM + CHR ROM) and provides
//! a `Mapper` trait for address translation. Supports NROM (Mapper 0) and
//! MMC1 (Mapper 1).

#![allow(clippy::cast_possible_truncation)]

/// Nametable mirroring mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mirroring {
    Horizontal,
    Vertical,
    FourScreen,
    SingleScreenLower,
    SingleScreenUpper,
}

/// Parsed iNES file header.
#[derive(Debug)]
#[allow(dead_code)]
pub struct CartridgeHeader {
    pub prg_rom_banks: u8,
    pub chr_rom_banks: u8,
    pub mapper_number: u8,
    pub mirroring: Mirroring,
    pub has_battery: bool,
}

/// Mapper trait: translates CPU and PPU addresses to cartridge ROM/RAM.
pub trait Mapper {
    fn cpu_read(&self, addr: u16) -> u8;
    fn cpu_write(&mut self, addr: u16, value: u8);
    fn chr_read(&self, addr: u16) -> u8;
    fn chr_write(&mut self, addr: u16, value: u8);
    fn mirroring(&self) -> Mirroring;
}

/// NROM (Mapper 0): no bank switching.
///
/// - PRG: 16K mirrored at $8000-$FFFF, or 32K at $8000-$FFFF
/// - CHR: 8K at PPU $0000-$1FFF (ROM or RAM if `chr_rom_banks` == 0)
pub struct Nrom {
    prg_rom: Vec<u8>,
    chr: Vec<u8>,
    chr_is_ram: bool,
    mirroring: Mirroring,
}

impl Nrom {
    #[must_use]
    pub fn new(prg_rom: Vec<u8>, chr_data: Vec<u8>, mirroring: Mirroring) -> Self {
        let chr_is_ram = chr_data.is_empty();
        let chr = if chr_is_ram {
            vec![0u8; 8192] // 8K CHR RAM
        } else {
            chr_data
        };
        Self {
            prg_rom,
            chr,
            chr_is_ram,
            mirroring,
        }
    }
}

impl Mapper for Nrom {
    fn cpu_read(&self, addr: u16) -> u8 {
        match addr {
            0x8000..=0xFFFF => {
                let offset = (addr - 0x8000) as usize;
                if self.prg_rom.len() == 16384 {
                    // 16K: mirror $8000-$BFFF to $C000-$FFFF
                    self.prg_rom[offset % 16384]
                } else {
                    // 32K: direct mapping
                    self.prg_rom[offset % self.prg_rom.len()]
                }
            }
            _ => 0,
        }
    }

    fn cpu_write(&mut self, _addr: u16, _value: u8) {
        // NROM has no writable PRG area
    }

    fn chr_read(&self, addr: u16) -> u8 {
        self.chr[(addr as usize) & 0x1FFF]
    }

    fn chr_write(&mut self, addr: u16, value: u8) {
        if self.chr_is_ram {
            self.chr[(addr as usize) & 0x1FFF] = value;
        }
    }

    fn mirroring(&self) -> Mirroring {
        self.mirroring
    }
}

/// MMC1 (Mapper 1, SxROM): serial shift register bank switching.
///
/// - 5-bit shift register loaded one bit at a time via writes to $8000-$FFFF
/// - After 5 writes, value dispatched to internal register based on address
/// - Writing with bit 7 set resets shift register and sets PRG mode 3
/// - PRG: 16K or 32K banking modes
/// - CHR: 4K or 8K banking modes
/// - PRG RAM: 8K at $6000-$7FFF
struct Mmc1 {
    prg_rom: Vec<u8>,
    chr: Vec<u8>,
    chr_is_ram: bool,
    prg_ram: [u8; 8192],
    shift_register: u8,
    shift_count: u8,
    control: u8,
    chr_bank_0: u8,
    chr_bank_1: u8,
    prg_bank: u8,
}

impl Mmc1 {
    fn new(prg_rom: Vec<u8>, chr_data: Vec<u8>) -> Self {
        let chr_is_ram = chr_data.is_empty();
        let chr = if chr_is_ram {
            vec![0u8; 8192] // 8K CHR RAM
        } else {
            chr_data
        };
        Self {
            prg_rom,
            chr,
            chr_is_ram,
            prg_ram: [0; 8192],
            shift_register: 0,
            shift_count: 0,
            control: 0x0C, // PRG mode 3 (fix last bank) on power-up
            chr_bank_0: 0,
            chr_bank_1: 0,
            prg_bank: 0,
        }
    }

    /// Number of 16K PRG banks.
    fn prg_bank_count(&self) -> usize {
        self.prg_rom.len() / 16384
    }

    /// Read a byte from PRG ROM at a given 16K bank + offset.
    fn read_prg(&self, bank: usize, offset: usize) -> u8 {
        let bank = bank % self.prg_bank_count();
        self.prg_rom[bank * 16384 + offset]
    }

    /// Write to the shift register or reset it.
    fn write_register(&mut self, addr: u16, value: u8) {
        if value & 0x80 != 0 {
            // Reset: clear shift register, set PRG mode 3
            self.shift_register = 0;
            self.shift_count = 0;
            self.control |= 0x0C;
            return;
        }

        // Shift bit 0 of value into the register (LSB first)
        self.shift_register |= (value & 1) << self.shift_count;
        self.shift_count += 1;

        if self.shift_count == 5 {
            let data = self.shift_register;
            // Dispatch based on address bits 14:13
            match (addr >> 13) & 0x03 {
                0 => self.control = data,    // $8000-$9FFF
                1 => self.chr_bank_0 = data, // $A000-$BFFF
                2 => self.chr_bank_1 = data, // $C000-$DFFF
                3 => self.prg_bank = data,   // $E000-$FFFF
                _ => unreachable!(),
            }
            self.shift_register = 0;
            self.shift_count = 0;
        }
    }
}

impl Mapper for Mmc1 {
    fn cpu_read(&self, addr: u16) -> u8 {
        match addr {
            0x6000..=0x7FFF => {
                // PRG RAM (active-low enable in prg_bank bit 4, but many
                // games assume always enabled — default to enabled)
                self.prg_ram[(addr - 0x6000) as usize]
            }
            0x8000..=0xBFFF => {
                let offset = (addr - 0x8000) as usize;
                let prg_mode = (self.control >> 2) & 0x03;
                match prg_mode {
                    0 | 1 => {
                        // 32K mode: bit 0 of bank number ignored
                        let bank = self.prg_bank as usize & 0x0E;
                        self.read_prg(bank, offset)
                    }
                    2 => {
                        // Fix first: $8000 = bank 0
                        self.read_prg(0, offset)
                    }
                    3 => {
                        // Switch: $8000 = selected bank
                        self.read_prg(self.prg_bank as usize & 0x0F, offset)
                    }
                    _ => unreachable!(),
                }
            }
            0xC000..=0xFFFF => {
                let offset = (addr - 0xC000) as usize;
                let prg_mode = (self.control >> 2) & 0x03;
                match prg_mode {
                    0 | 1 => {
                        // 32K mode: second 16K of the 32K block
                        let bank = self.prg_bank as usize & 0x0E;
                        self.read_prg(bank + 1, offset)
                    }
                    2 => {
                        // Fix first: $C000 = selected bank
                        self.read_prg(self.prg_bank as usize & 0x0F, offset)
                    }
                    3 => {
                        // Fix last: $C000 = last bank
                        self.read_prg(self.prg_bank_count() - 1, offset)
                    }
                    _ => unreachable!(),
                }
            }
            _ => 0,
        }
    }

    fn cpu_write(&mut self, addr: u16, value: u8) {
        match addr {
            0x6000..=0x7FFF => {
                self.prg_ram[(addr - 0x6000) as usize] = value;
            }
            0x8000..=0xFFFF => {
                self.write_register(addr, value);
            }
            _ => {}
        }
    }

    fn chr_read(&self, addr: u16) -> u8 {
        let addr = addr as usize & 0x1FFF;
        let chr_mode = (self.control >> 4) & 1;

        if chr_mode == 0 {
            // 8K mode: bit 0 of chr_bank_0 ignored
            let bank = (self.chr_bank_0 as usize & 0x1E) * 4096;
            let index = (bank + addr) % self.chr.len();
            self.chr[index]
        } else {
            // 4K mode
            let bank = if addr < 0x1000 {
                self.chr_bank_0 as usize
            } else {
                self.chr_bank_1 as usize
            };
            let offset = addr & 0x0FFF;
            let index = (bank * 4096 + offset) % self.chr.len();
            self.chr[index]
        }
    }

    fn chr_write(&mut self, addr: u16, value: u8) {
        if !self.chr_is_ram {
            return;
        }
        let addr = addr as usize & 0x1FFF;
        let chr_mode = (self.control >> 4) & 1;

        if chr_mode == 0 {
            let bank = (self.chr_bank_0 as usize & 0x1E) * 4096;
            let index = (bank + addr) % self.chr.len();
            self.chr[index] = value;
        } else {
            let bank = if addr < 0x1000 {
                self.chr_bank_0 as usize
            } else {
                self.chr_bank_1 as usize
            };
            let offset = addr & 0x0FFF;
            let index = (bank * 4096 + offset) % self.chr.len();
            self.chr[index] = value;
        }
    }

    fn mirroring(&self) -> Mirroring {
        match self.control & 0x03 {
            0 => Mirroring::SingleScreenLower,
            1 => Mirroring::SingleScreenUpper,
            2 => Mirroring::Vertical,
            3 => Mirroring::Horizontal,
            _ => unreachable!(),
        }
    }
}

/// Parse an iNES file and return a boxed mapper.
///
/// # Errors
///
/// Returns an error string if the header is invalid or the mapper is unsupported.
pub fn parse_ines(data: &[u8]) -> Result<Box<dyn Mapper>, String> {
    if data.len() < 16 {
        return Err("iNES file too short (< 16 bytes)".to_string());
    }

    // Check magic: "NES\x1A"
    if &data[0..4] != b"NES\x1a" {
        return Err("Invalid iNES magic (expected NES\\x1A)".to_string());
    }

    let prg_banks = data[4];
    let chr_banks = data[5];
    let flags6 = data[6];
    let flags7 = data[7];

    let mapper_lo = (flags6 >> 4) & 0x0F;
    let mapper_hi = flags7 & 0xF0;
    let mapper_number = mapper_hi | mapper_lo;

    let mirroring = if flags6 & 0x08 != 0 {
        Mirroring::FourScreen
    } else if flags6 & 0x01 != 0 {
        Mirroring::Vertical
    } else {
        Mirroring::Horizontal
    };

    let has_battery = flags6 & 0x02 != 0;
    let has_trainer = flags6 & 0x04 != 0;

    let header = CartridgeHeader {
        prg_rom_banks: prg_banks,
        chr_rom_banks: chr_banks,
        mapper_number,
        mirroring,
        has_battery,
    };

    let prg_size = usize::from(prg_banks) * 16384;
    let chr_size = usize::from(chr_banks) * 8192;

    let prg_start = if has_trainer { 16 + 512 } else { 16 };
    let chr_start = prg_start + prg_size;

    if data.len() < chr_start + chr_size {
        return Err(format!(
            "iNES file too short: expected {} bytes, got {}",
            chr_start + chr_size,
            data.len()
        ));
    }

    let prg_rom = data[prg_start..prg_start + prg_size].to_vec();
    let chr_data = if chr_size > 0 {
        data[chr_start..chr_start + chr_size].to_vec()
    } else {
        Vec::new() // CHR RAM
    };

    match header.mapper_number {
        0 => Ok(Box::new(Nrom::new(prg_rom, chr_data, mirroring))),
        1 => Ok(Box::new(Mmc1::new(prg_rom, chr_data))),
        n => Err(format!("Unsupported mapper: {n}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_ines(prg_banks: u8, chr_banks: u8, flags6: u8) -> Vec<u8> {
        let prg_size = usize::from(prg_banks) * 16384;
        let chr_size = usize::from(chr_banks) * 8192;
        let mut data = vec![0u8; 16 + prg_size + chr_size];
        data[0..4].copy_from_slice(b"NES\x1a");
        data[4] = prg_banks;
        data[5] = chr_banks;
        data[6] = flags6;
        // Fill PRG with a recognizable pattern
        for i in 0..prg_size {
            data[16 + i] = (i & 0xFF) as u8;
        }
        // Fill CHR with a different pattern
        for i in 0..chr_size {
            data[16 + prg_size + i] = ((i + 0x80) & 0xFF) as u8;
        }
        data
    }

    #[test]
    fn parse_valid_nrom_16k() {
        let data = make_ines(1, 1, 0x00); // Horizontal mirroring
        let mapper = parse_ines(&data).expect("parse failed");
        assert_eq!(mapper.mirroring(), Mirroring::Horizontal);
        // PRG at $8000 should be byte 0 of PRG ROM
        assert_eq!(mapper.cpu_read(0x8000), 0x00);
        // 16K mirrored: $C000 should mirror $8000
        assert_eq!(mapper.cpu_read(0xC000), 0x00);
    }

    #[test]
    fn parse_valid_nrom_32k() {
        let data = make_ines(2, 1, 0x01); // Vertical mirroring
        let mapper = parse_ines(&data).expect("parse failed");
        assert_eq!(mapper.mirroring(), Mirroring::Vertical);
        assert_eq!(mapper.cpu_read(0x8000), 0x00);
        // $C000 maps to bank 1 start
        assert_eq!(mapper.cpu_read(0xC000), 0x00); // offset 0x4000 & 0xFF = 0
    }

    #[test]
    fn chr_read_write_ram() {
        let data = make_ines(1, 0, 0x00); // CHR RAM (0 banks)
        let mut mapper = parse_ines(&data).expect("parse failed");
        assert_eq!(mapper.chr_read(0x0000), 0);
        mapper.chr_write(0x0000, 0xAB);
        assert_eq!(mapper.chr_read(0x0000), 0xAB);
    }

    #[test]
    fn chr_rom_not_writable() {
        let data = make_ines(1, 1, 0x00); // CHR ROM (1 bank)
        let mut mapper = parse_ines(&data).expect("parse failed");
        let original = mapper.chr_read(0x0000);
        mapper.chr_write(0x0000, 0xFF);
        assert_eq!(mapper.chr_read(0x0000), original); // Unchanged
    }

    #[test]
    fn invalid_magic() {
        let data = vec![0u8; 32];
        assert!(parse_ines(&data).is_err());
    }

    #[test]
    fn unsupported_mapper() {
        let mut data = make_ines(1, 1, 0x00);
        data[6] = 0x20; // Mapper 2 (low nibble)
        assert!(parse_ines(&data).is_err());
    }

    // --- MMC1 tests ---

    /// Build an MMC1 with `prg_banks` x 16K PRG and `chr_banks` x 8K CHR.
    /// PRG banks are filled with their bank index (0,1,2…) in every byte.
    /// CHR 4K pages are filled with their page index.
    fn make_mmc1(prg_banks: u8, chr_banks: u8) -> Mmc1 {
        let prg_size = prg_banks as usize * 16384;
        let chr_size = chr_banks as usize * 8192;
        let mut prg_rom = vec![0u8; prg_size];
        for bank in 0..prg_banks as usize {
            for i in 0..16384 {
                prg_rom[bank * 16384 + i] = bank as u8;
            }
        }
        let chr_data = if chr_size > 0 {
            let mut chr = vec![0u8; chr_size];
            let pages = chr_size / 4096;
            for page in 0..pages {
                for i in 0..4096 {
                    chr[page * 4096 + i] = page as u8;
                }
            }
            chr
        } else {
            Vec::new()
        };
        Mmc1::new(prg_rom, chr_data)
    }

    /// Write a 5-bit value to an MMC1 register at `addr`.
    fn mmc1_write_5(mapper: &mut Mmc1, addr: u16, value: u8) {
        for bit in 0..5 {
            mapper.cpu_write(addr, (value >> bit) & 1);
        }
    }

    #[test]
    fn mmc1_parse_ines() {
        // flags6 low nibble = 1 (mapper 1)
        let data = make_ines(8, 2, 0x10);
        let mapper = parse_ines(&data).expect("parse failed");
        // Should be MMC1 — default mirroring from control=0x0C → bits 1:0 = 0
        // = SingleScreenLower
        assert_eq!(mapper.mirroring(), Mirroring::SingleScreenLower);
    }

    #[test]
    fn mmc1_reset_on_bit7() {
        let mut m = make_mmc1(8, 2);
        // Partially fill shift register
        m.cpu_write(0x8000, 1);
        m.cpu_write(0x8000, 0);
        assert_eq!(m.shift_count, 2);
        // Write with bit 7 → reset
        m.cpu_write(0x8000, 0x80);
        assert_eq!(m.shift_count, 0);
        assert_eq!(m.shift_register, 0);
        // PRG mode should be 3 (bits 3:2 of control set)
        assert_eq!((m.control >> 2) & 0x03, 3);
    }

    #[test]
    fn mmc1_shift_register_5_writes() {
        let mut m = make_mmc1(8, 2);
        // Write 0b10101 = 21 to control register ($8000-$9FFF)
        mmc1_write_5(&mut m, 0x8000, 0b10101);
        assert_eq!(m.control, 0b10101);

        // Write 0b00011 = 3 to CHR bank 0 ($A000-$BFFF)
        mmc1_write_5(&mut m, 0xA000, 3);
        assert_eq!(m.chr_bank_0, 3);

        // Write 0b00101 = 5 to CHR bank 1 ($C000-$DFFF)
        mmc1_write_5(&mut m, 0xC000, 5);
        assert_eq!(m.chr_bank_1, 5);

        // Write 0b00010 = 2 to PRG bank ($E000-$FFFF)
        mmc1_write_5(&mut m, 0xE000, 2);
        assert_eq!(m.prg_bank, 2);
    }

    #[test]
    fn mmc1_prg_mode_3_fix_last() {
        // Default mode 3: $8000 = selected bank, $C000 = last bank
        let mut m = make_mmc1(8, 0); // 8 x 16K banks (0-7)
        // control defaults to 0x0C = mode 3
        assert_eq!((m.control >> 2) & 0x03, 3);

        // Select bank 2 for $8000
        mmc1_write_5(&mut m, 0xE000, 2);
        assert_eq!(m.cpu_read(0x8000), 2); // bank 2
        assert_eq!(m.cpu_read(0xC000), 7); // last bank
    }

    #[test]
    fn mmc1_prg_mode_2_fix_first() {
        let mut m = make_mmc1(8, 0);
        // Set mode 2: control bits 3:2 = 10
        mmc1_write_5(&mut m, 0x8000, 0b01000); // control = 8

        // Select bank 5 for $C000
        mmc1_write_5(&mut m, 0xE000, 5);
        assert_eq!(m.cpu_read(0x8000), 0); // bank 0 fixed
        assert_eq!(m.cpu_read(0xC000), 5); // selected bank
    }

    #[test]
    fn mmc1_prg_mode_0_32k() {
        let mut m = make_mmc1(8, 0); // 8 x 16K = 4 x 32K
        // Set mode 0: control bits 3:2 = 00
        mmc1_write_5(&mut m, 0x8000, 0b00000);

        // Select bank 3 — bit 0 ignored in 32K mode, so effective = bank 2
        mmc1_write_5(&mut m, 0xE000, 3);
        // 32K block 1: banks 2,3
        assert_eq!(m.cpu_read(0x8000), 2); // first 16K of 32K block
        assert_eq!(m.cpu_read(0xC000), 3); // second 16K of 32K block
    }

    #[test]
    fn mmc1_chr_4k_mode() {
        let mut m = make_mmc1(2, 2); // 2 x 8K CHR = 4 x 4K pages (0-3)
        // Set CHR 4K mode: control bit 4 = 1
        mmc1_write_5(&mut m, 0x8000, 0b11100); // control = 0x1C

        // Select page 1 for $0000-$0FFF
        mmc1_write_5(&mut m, 0xA000, 1);
        // Select page 3 for $1000-$1FFF
        mmc1_write_5(&mut m, 0xC000, 3);

        assert_eq!(m.chr_read(0x0000), 1); // page 1
        assert_eq!(m.chr_read(0x1000), 3); // page 3
    }

    #[test]
    fn mmc1_chr_8k_mode() {
        let mut m = make_mmc1(2, 2); // 4 x 4K CHR pages
        // Set CHR 8K mode: control bit 4 = 0
        mmc1_write_5(&mut m, 0x8000, 0b01100); // control = 0x0C, CHR 8K

        // Select bank 3 — bit 0 ignored → effective bank 2 (pages 2,3)
        mmc1_write_5(&mut m, 0xA000, 3);
        assert_eq!(m.chr_read(0x0000), 2); // page 2
        assert_eq!(m.chr_read(0x1000), 3); // page 3
    }

    #[test]
    fn mmc1_prg_ram() {
        let mut m = make_mmc1(2, 0);
        // Write and read PRG RAM at $6000-$7FFF
        assert_eq!(m.cpu_read(0x6000), 0);
        m.cpu_write(0x6000, 0x42);
        assert_eq!(m.cpu_read(0x6000), 0x42);
        m.cpu_write(0x7FFF, 0xAB);
        assert_eq!(m.cpu_read(0x7FFF), 0xAB);
    }

    #[test]
    fn mmc1_mirroring_dynamic() {
        let mut m = make_mmc1(2, 0);
        // Default control = 0x0C → bits 1:0 = 0 → SingleScreenLower
        assert_eq!(m.mirroring(), Mirroring::SingleScreenLower);

        // Set mirroring to vertical (bits 1:0 = 2)
        mmc1_write_5(&mut m, 0x8000, 0b01110); // control = 0x0E
        assert_eq!(m.mirroring(), Mirroring::Vertical);

        // Set to horizontal (bits 1:0 = 3)
        mmc1_write_5(&mut m, 0x8000, 0b01111); // control = 0x0F
        assert_eq!(m.mirroring(), Mirroring::Horizontal);

        // Set to single-screen upper (bits 1:0 = 1)
        mmc1_write_5(&mut m, 0x8000, 0b01101); // control = 0x0D
        assert_eq!(m.mirroring(), Mirroring::SingleScreenUpper);
    }
}
