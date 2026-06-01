use common_sinclair_zx_spectrum::memory::{Bank16K, MemoryBus};
use common_sinclair_zx_spectrum::snapshot::Paged128kMemory;
use std::path::Path;

/// Scorpion ZS-256 memory: 4 × 16K ROM + 16 × 16K RAM banks.
///
/// Paging via two ports (cross-referenced against FUSE's
/// `machines/scorpion.c`):
///
/// Port $7FFD (standard 128K paging):
///   Bits 0-2: low 3 bits of RAM-bank index at $C000
///   Bit 3:    Screen bank (0 = bank 5, 1 = bank 7)
///   Bit 4:    ROM select between ROM 0 and ROM 1
///   Bit 5:    Paging lock
///
/// Port $1FFD (Scorpion extension):
///   Bit 0:    "all RAM at $0000-$3FFF" mode (+3-style, unused at boot)
///   Bit 1:    When set, forces ROM 2 (TR-DOS / Service swap)
///             regardless of $7FFD bit 4
///   Bit 4:    high bit (bit 3) of the 16-bank RAM index
///
/// ROM bank layout (matching FUSE's `rom_scorpion_{0,1,2,3}`):
///   0 = 128 Editor (Scorpion-branded, "Scorpion ZS 256 1992-94")
///   1 = 48 BASIC ("© 1982 Sinclair Research Ltd")
///   2 = TR-DOS / Service swap, paged in via $1FFD bit 1
///   3 = Beta Disk ROMCS overlay, paged in by the M1 address trap
///       when PC enters $3D00-$3DFF; NOT reachable via $7FFD/$1FFD.
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
        if self.locked {
            return;
        }
        self.paging_7ffd = val;
        if val & 0x20 != 0 {
            self.locked = true;
        }
    }

    /// Read from the Beta Disk overlay — used when the M1 address
    /// trap pages it in. Per FUSE's `machines/scorpion.c` the Beta
    /// overlay is intended to live in a separate ROMCS bank (loaded
    /// from `rom_scorpion_3`). The ROM distribution we currently
    /// ship surfaces TR-DOS in ROM bank 1 instead — switching this
    /// to read from `rom[3]` regressed the boot, so we keep the
    /// existing index until the ROM layout is verified.
    pub fn read_trdos_rom(&self, addr: u16) -> u8 {
        self.rom[1][addr as usize & 0x3FFF]
    }

    pub fn write_1ffd(&mut self, val: u8) {
        if self.locked {
            return;
        }
        self.paging_1ffd = val;
    }

    /// RAM bank at $C000. FUSE's `machines/scorpion.c` formula is
    /// `((last_byte2 & 0x10) >> 1) | (last_byte & 0x07)` (high bit
    /// from $1FFD bit 4). The Scorpion ROM distribution we currently
    /// ship targets the alternate convention where $1FFD bit 0
    /// carries the high page bit. Switching to FUSE's formula
    /// regresses the boot — the Editor's banks land in the wrong
    /// slots and the CPU never reaches `EI`. Tracked as a separate
    /// open question pending evidence on which Scorpion ROM
    /// distribution our files match.
    fn current_bank(&self) -> usize {
        let low = (self.paging_7ffd & 0x07) as usize;
        let high = ((self.paging_1ffd & 0x01) as usize) << 3;
        low | high
    }

    /// ROM bank at $0000-$3FFF. FUSE's logic is "if $1FFD bit 1 then
    /// ROM 2, else $7FFD bit 4 → ROM 0/1" — but the Scorpion ROM
    /// distribution we ship boots correctly only with the 2-bit
    /// composite `($1FFD bit 1) << 1 | ($7FFD bit 4)` index that
    /// reaches all 4 ROM slots. Tracked as the same open question
    /// as `current_bank()` above.
    fn current_rom(&self) -> usize {
        let low = ((self.paging_7ffd >> 4) & 0x01) as usize;
        let high = ((self.paging_1ffd >> 1) & 0x01) as usize;
        (high << 1) | low
    }

    pub fn screen_bank(&self) -> u8 {
        if self.paging_7ffd & 0x08 != 0 { 7 } else { 5 }
    }

    /// Reads one byte from a specific ROM bank, ignoring the current
    /// paging. Used by the runtime's screen-text decoder to reach
    /// the standard glyph table at `$3D00` of ROM 1 (48 BASIC) when
    /// the menu / TR-DOS / Service ROM is currently mapped at
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

        // Bank 8: $7FFD bits 0-2 = 0, $1FFD bit 0 = 1.
        // Our current Scorpion ROM distribution targets this
        // convention — see the note on `current_bank()` for the
        // FUSE-vs-distribution open question.
        mem.write_7ffd(0x00);
        mem.write_1ffd(0x01);
        assert_eq!(mem.read(0xC000), 0xBB);
    }

    #[test]
    fn three_main_roms_plus_beta_overlay() {
        // Per FUSE machines/scorpion.c: $7FFD bit 4 selects between
        // ROM 0 (Scorpion-branded 128 Editor) and ROM 1 (48 BASIC);
        // $1FFD bit 1 forces ROM 2 (TR-DOS / Service swap) regardless
        // of $7FFD bit 4. ROM 3 is the Beta Disk overlay and is not
        // reachable through this bank-select path — it's paged in by
        // the M1 trap mechanism (read_trdos_rom).
        let mut mem = MemoryScorpion::new();
        mem.rom[0][0] = 0x00;
        mem.rom[1][0] = 0x11;
        mem.rom[2][0] = 0x22;
        mem.rom[3][0] = 0x33;

        // Default state — ROM 0.
        assert_eq!(mem.read(0x0000), 0x00);

        // $7FFD bit 4 → ROM 1.
        mem.write_7ffd(0x10);
        assert_eq!(mem.read(0x0000), 0x11);

        // $1FFD bit 1 + $7FFD bit 4 clear → ROM 2.
        mem.write_7ffd(0x00);
        mem.write_1ffd(0x02);
        assert_eq!(mem.read(0x0000), 0x22);

        // $1FFD bit 1 + $7FFD bit 4 both set → ROM 3 (with the
        // composite 2-bit index our current Scorpion ROM expects).
        mem.write_7ffd(0x10);
        assert_eq!(mem.read(0x0000), 0x33);

        // M1-trap read currently sources from ROM 1 (see comment on
        // read_trdos_rom for the FUSE-vs-our-ROM-layout question).
        assert_eq!(mem.read_trdos_rom(0x0000), 0x11);
    }
}
