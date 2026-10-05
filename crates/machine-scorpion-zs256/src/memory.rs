use common_sinclair_zx_spectrum::memory::{Bank16K, MemoryBus};
use common_sinclair_zx_spectrum::snapshot::Paged128kMemory;
use std::path::Path;

/// Scorpion ZS-256 memory: 4 × 16K ROM + 16 × 16K RAM banks.
///
/// Paging via two ports. The bit meanings follow FUSE's
/// `machines/scorpion.c` (`scorpion_memory_map`) and agree with MAME's
/// `sinclair/scorpion.cpp` (`scorpion_update_memory`, whose port comment
/// transcribes the machine's own port description):
///
/// Port $7FFD (standard 128K paging):
///   Bits 0-2: low 3 bits of RAM-bank index at $C000
///   Bit 3:    Screen bank (0 = bank 5, 1 = bank 7)
///   Bit 4:    ROM select between ROM 0 and ROM 1
///   Bit 5:    Paging lock
///
/// Port $1FFD (Scorpion extension):
///   Bit 0:    RAM bank 0 replaces the ROM at $0000-$3FFF (read/write)
///   Bit 1:    Selects ROM 2 (Service monitor) regardless of $7FFD bit 4
///   Bit 4:    high bit (bit 3) of the 16-bank RAM index at $C000
///
/// ROM bank layout (FUSE `rom_scorpion_{0,1,2,3}`, MAME `scorp{0..3}.rom`):
///   0 = 128 Editor (Scorpion-branded, "Scorpion ZS 256 1992-94")
///   1 = 48 BASIC ("© 1982 Sinclair Research Ltd")
///   2 = Service monitor, paged in via $1FFD bit 1
///   3 = TR-DOS: the Beta Disk ROMCS overlay, paged in by the M1 address
///       trap when PC enters $3D00-$3DFF; NOT reachable via $7FFD/$1FFD.
///
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
    /// ROM bank holding TR-DOS, which only the Beta Disk overlay reaches.
    pub const TRDOS_ROM_BANK: u8 = 3;

    pub fn new() -> Self {
        Self {
            rom: vec![Bank16K::zeroed(); 4],
            ram: vec![Bank16K::zeroed(); 16],
            paging_7ffd: 0,
            paging_1ffd: 0,
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
        if self.locked {
            return;
        }
        self.paging_7ffd = val;
        if val & 0x20 != 0 {
            self.locked = true;
        }
    }

    /// Read from the Beta Disk overlay — used while the M1 address trap
    /// has it paged in. FUSE loads `rom_scorpion_3` into the Beta ROMCS
    /// bank rather than into the switchable ROM slots, so the overlay is
    /// ROM 3.
    pub fn read_trdos_rom(&self, addr: u16) -> u8 {
        self.rom[usize::from(Self::TRDOS_ROM_BANK)][addr as usize & 0x3FFF]
    }

    pub fn write_1ffd(&mut self, val: u8) {
        if self.locked {
            return;
        }
        self.paging_1ffd = val;
    }

    /// RAM bank at $C000: FUSE's
    /// `((last_byte2 & 0x10) >> 1) | (last_byte & 0x07)`, the high bit
    /// coming from $1FFD bit 4.
    pub fn current_bank(&self) -> usize {
        let low = (self.paging_7ffd & 0x07) as usize;
        let high = ((self.paging_1ffd >> 4) & 0x01) as usize;
        (high << 3) | low
    }

    /// ROM bank at $0000-$3FFF while RAM is not mapped there: ROM 2 when
    /// $1FFD bit 1 is set, otherwise ROM 0 or 1 by $7FFD bit 4. ROM 3 is
    /// never selected here — it is the Beta Disk overlay.
    pub fn current_rom(&self) -> usize {
        if self.paging_1ffd & 0x02 != 0 {
            2
        } else {
            ((self.paging_7ffd >> 4) & 0x01) as usize
        }
    }

    /// True when $1FFD bit 0 maps RAM bank 0 over the ROM at $0000-$3FFF.
    #[must_use]
    pub fn ram_at_zero(&self) -> bool {
        self.paging_1ffd & 0x01 != 0
    }

    pub fn screen_bank(&self) -> u8 {
        if self.paging_7ffd & 0x08 != 0 { 7 } else { 5 }
    }

    /// Reads one byte from a specific ROM bank, ignoring the current
    /// paging. Used by the runtime's screen-text decoder to reach
    /// the standard glyph table at `$3D00` of ROM 1 (48 BASIC) when
    /// the 128 Editor / Service ROM is currently mapped at
    /// `$0000-$3FFF`. Returns `0` for out-of-range bank indices.
    #[must_use]
    pub fn read_rom_byte(&self, bank: usize, addr: u16) -> u8 {
        self.rom
            .get(bank)
            .and_then(|rom| rom.get(addr as usize))
            .copied()
            .unwrap_or(0)
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
            0x0000..=0x3FFF if self.ram_at_zero() => self.ram[0][addr as usize],
            0x0000..=0x3FFF => self.rom[self.current_rom()][addr as usize],
            0x4000..=0x7FFF => self.ram[5][(addr - 0x4000) as usize],
            0x8000..=0xBFFF => self.ram[2][(addr - 0x8000) as usize],
            0xC000..=0xFFFF => self.ram[self.current_bank()][(addr - 0xC000) as usize],
        }
    }

    #[inline]
    fn write(&mut self, addr: u16, val: u8) {
        match addr {
            0x0000..=0x3FFF if self.ram_at_zero() => self.ram[0][addr as usize] = val,
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
    fn high_bank_bit_is_1ffd_bit_4() {
        let mut mem = MemoryScorpion::new();
        for bank in 0..16 {
            mem.ram[bank][0] = 0xA0 | bank as u8;
        }
        for bank in 0..16u8 {
            mem.write_7ffd(bank & 0x07);
            mem.write_1ffd((bank & 0x08) << 1);
            assert_eq!(mem.current_bank(), usize::from(bank));
            assert_eq!(mem.read(0xC000), 0xA0 | bank);
        }

        // $1FFD bit 0 is not a page bit: with $7FFD = 0 the bank stays 0.
        mem.write_7ffd(0x00);
        mem.write_1ffd(0x01);
        assert_eq!(mem.current_bank(), 0);
    }

    #[test]
    fn rom_select_reaches_roms_0_to_2_only() {
        let mut mem = MemoryScorpion::new();
        for rom in 0..4 {
            mem.rom[rom][0] = 0x11 * rom as u8;
        }

        assert_eq!(mem.read(0x0000), 0x00, "reset state is ROM 0");

        mem.write_7ffd(0x10);
        assert_eq!(mem.read(0x0000), 0x11, "$7FFD bit 4 selects ROM 1");

        mem.write_7ffd(0x00);
        mem.write_1ffd(0x02);
        assert_eq!(mem.read(0x0000), 0x22, "$1FFD bit 1 selects ROM 2");

        // $1FFD bit 1 wins over $7FFD bit 4: no combination reaches ROM 3.
        mem.write_7ffd(0x10);
        assert_eq!(mem.current_rom(), 2);
        assert_eq!(mem.read(0x0000), 0x22);
    }

    #[test]
    fn beta_overlay_reads_rom_3() {
        let mut mem = MemoryScorpion::new();
        for rom in 0..4 {
            mem.rom[rom][0x3D00] = 0x11 * rom as u8;
        }
        assert_eq!(mem.read_trdos_rom(0x3D00), 0x33);
    }

    #[test]
    fn bit_0_of_1ffd_maps_ram_0_over_the_rom() {
        let mut mem = MemoryScorpion::new();
        mem.rom[0][0x0100] = 0x55;
        mem.ram[0][0x0100] = 0xAA;

        mem.write(0x0100, 0x12);
        assert_eq!(mem.read(0x0100), 0x55, "ROM ignores writes");

        mem.write_1ffd(0x01);
        assert_eq!(mem.read(0x0100), 0xAA);
        mem.write(0x0100, 0x34);
        assert_eq!(mem.read(0x0100), 0x34);

        mem.write_1ffd(0x00);
        assert_eq!(mem.read(0x0100), 0x55);
        assert_eq!(mem.ram[0][0x0100], 0x34);
    }
}
