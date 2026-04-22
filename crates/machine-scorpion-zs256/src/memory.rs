use common_sinclair_zx_spectrum::memory::{Bank16K, MemoryBus};
use common_sinclair_zx_spectrum::snapshot::Paged128kMemory;
use std::path::Path;

/// Scorpion ZS-256 memory: 4 × 16K ROM + 16 × 16K RAM banks.
///
/// Paging via two ports:
///
/// Port $7FFD (standard 128K paging):
///   Bits 0-2: RAM bank at $C000 (0-7, from the standard 128K set)
///   Bit 3:    Screen bank (0 = bank 5, 1 = bank 7)
///   Bit 4:    ROM select low bit
///   Bit 5:    Paging lock
///
/// Port $1FFD (Scorpion extension):
///   Bit 0:    RAM bank bit 3 at $C000 (extends to 16 banks)
///   Bit 1:    ROM select high bit
///   Bits 2-4: Reserved
///
/// ROM select = ($1FFD bit 1) << 1 | ($7FFD bit 4). Four ROMs:
///   0 = Service monitor, 1 = TR-DOS, 2 = 128K editor, 3 = 48K BASIC
/// Banks live behind `Vec<Bank16K>` so that `serde`'s deserializer
/// processes one 16 KB chunk at a time into heap memory rather than
/// materialising the whole 320 KB inline-array on the caller's stack.
#[derive(serde::Serialize, serde::Deserialize)]
pub struct MemoryScorpion {
    rom: Vec<Bank16K>,
    ram: Vec<Bank16K>,
    paging_7ffd: u8,
    paging_1ffd: u8,
    locked: bool,
}

impl MemoryScorpion {
    pub fn new() -> Self {
        Self {
            rom: vec![Bank16K::zeroed(); 4],
            ram: vec![Bank16K::zeroed(); 16],
            paging_7ffd: 0,
            paging_1ffd: 0, // TODO: native ProfROM needs Beta disk + Scorpion hardware stubs
            locked: false,
        }
    }

    pub fn load_roms(&mut self, rom0: &[u8], rom1: &[u8], rom2: &[u8], rom3: &[u8]) {
        for (i, data) in [rom0, rom1, rom2, rom3].iter().enumerate() {
            let len = data.len().min(16384);
            self.rom[i][..len].copy_from_slice(&data[..len]);
        }
    }

    pub fn load_rom(&mut self, index: usize, path: &Path) -> std::io::Result<()> {
        let data = std::fs::read(path)?;
        if data.len() != 16384 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("ROM should be 16384 bytes, got {}", data.len()),
            ));
        }
        self.rom[index].copy_from_slice(&data);
        Ok(())
    }

    pub fn write_7ffd(&mut self, val: u8) {
        if self.locked { return; }
        self.paging_7ffd = val;
        if val & 0x20 != 0 {
            self.locked = true;
        }
    }

    /// Read from the TR-DOS ROM (ROM 1) — used when Beta disk is paged in.
    pub fn read_trdos_rom(&self, addr: u16) -> u8 {
        self.rom[1][addr as usize & 0x3FFF]
    }

    pub fn write_1ffd(&mut self, val: u8) {
        if self.locked { return; }
        self.paging_1ffd = val;
    }

    /// RAM bank at $C000: bits 0-2 of $7FFD + bit 0 of $1FFD as bit 3.
    fn current_bank(&self) -> usize {
        let low = (self.paging_7ffd & 0x07) as usize;
        let high = ((self.paging_1ffd & 0x01) as usize) << 3;
        low | high
    }

    /// ROM select: bit 4 of $7FFD (low) + bit 1 of $1FFD (high).
    fn current_rom(&self) -> usize {
        let low = ((self.paging_7ffd >> 4) & 0x01) as usize;
        let high = ((self.paging_1ffd >> 1) & 0x01) as usize;
        (high << 1) | low
    }

    pub fn screen_bank(&self) -> u8 {
        if self.paging_7ffd & 0x08 != 0 { 7 } else { 5 }
    }
}

impl Default for MemoryScorpion {
    fn default() -> Self {
        Self::new()
    }
}

impl Paged128kMemory for MemoryScorpion {
    fn write_7ffd(&mut self, val: u8) {
        MemoryScorpion::write_7ffd(self, val)
    }
}

impl MemoryBus for MemoryScorpion {
    #[inline]
    fn read(&self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x3FFF => self.rom[self.current_rom()][addr as usize],
            0x4000..=0x7FFF => self.ram[5][(addr - 0x4000) as usize],
            0x8000..=0xBFFF => self.ram[2][(addr - 0x8000) as usize],
            0xC000..=0xFFFF => self.ram[self.current_bank()][(addr - 0xC000) as usize],
        }
    }

    #[inline]
    fn write(&mut self, addr: u16, val: u8) {
        match addr {
            0x0000..=0x3FFF => {} // ROM
            0x4000..=0x7FFF => self.ram[5][(addr - 0x4000) as usize] = val,
            0x8000..=0xBFFF => self.ram[2][(addr - 0x8000) as usize] = val,
            0xC000..=0xFFFF => {
                let bank = self.current_bank();
                self.ram[bank][(addr - 0xC000) as usize] = val;
            }
        }
    }

    #[inline]
    fn is_contended(&self, _addr: u16) -> bool {
        false // Scorpion has no contention
    }

    #[inline]
    fn read_screen(&self, addr: u16) -> u8 {
        let bank = self.screen_bank() as usize;
        self.ram[bank][(addr & 0x3FFF) as usize]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bank_16_banks() {
        let mut mem = MemoryScorpion::new();
        // Write to bank 0 and bank 8
        mem.ram[0][0] = 0xAA;
        mem.ram[8][0] = 0xBB;

        // Default: bank 0
        assert_eq!(mem.read(0xC000), 0xAA);

        // Bank 8: $7FFD bits 0-2 = 0, $1FFD bit 0 = 1
        mem.write_7ffd(0x00);
        mem.write_1ffd(0x01);
        assert_eq!(mem.read(0xC000), 0xBB);
    }

    #[test]
    fn four_roms() {
        let mut mem = MemoryScorpion::new();
        mem.rom[0][0] = 0x00;
        mem.rom[1][0] = 0x11;
        mem.rom[2][0] = 0x22;
        mem.rom[3][0] = 0x33;

        assert_eq!(mem.read(0x0000), 0x00); // ROM 0
        mem.write_7ffd(0x10); // ROM 1
        assert_eq!(mem.read(0x0000), 0x11);
        mem.write_7ffd(0x00);
        mem.write_1ffd(0x02); // ROM 2
        assert_eq!(mem.read(0x0000), 0x22);
        mem.write_7ffd(0x10); // ROM 3
        assert_eq!(mem.read(0x0000), 0x33);
    }
}
