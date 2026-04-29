//! NROM (Mapper 0): no bank switching.

use serde::{Deserialize, Serialize};
use serde_big_array::BigArray;

use crate::mapper::{Mapper, Mirroring};
use crate::snapshot::MapperSnapshot;

/// NROM (Mapper 0): no bank switching.
///
/// The simplest cartridge: 16 KiB or 32 KiB of PRG ROM wired
/// directly to `$8000-$FFFF`, and 8 KiB of CHR (ROM or RAM) wired
/// directly to `$0000-$1FFF` on the PPU bus. Used by *Super Mario
/// Bros.*, *Donkey Kong*, *Ice Climber*, *Excitebike*, *Balloon
/// Fight*, and most of Nintendo's first-party launch titles.
///
/// ## Memory map
///
/// - `$6000-$7FFF` — 8 KiB work RAM. Many test ROMs (blargg's) use
///   NROM and write their results to this region, so the port
///   carries the RAM even though *Super Mario Bros.* doesn't touch
///   it.
/// - `$8000-$BFFF` — first 16 KiB of PRG ROM.
/// - `$C000-$FFFF` — second 16 KiB of PRG ROM for 32 KiB carts, or
///   a mirror of `$8000-$BFFF` for 16 KiB carts.
/// - PPU `$0000-$1FFF` — 8 KiB CHR ROM, or 8 KiB CHR RAM if the
///   iNES header reports zero CHR banks.
#[derive(Clone, Serialize, Deserialize)]
pub struct Nrom {
    prg_rom: Vec<u8>,
    chr: Vec<u8>,
    chr_is_ram: bool,
    mirroring: Mirroring,
    #[serde(with = "BigArray")]
    prg_ram: [u8; 8192],
}

impl Nrom {
    /// Construct an NROM from the parsed iNES payload.
    ///
    /// `chr_data` is the raw CHR ROM bytes from the iNES file; pass
    /// an empty `Vec` for a CHR-RAM cartridge (8 KiB of writable
    /// RAM will be allocated).
    #[must_use]
    pub fn new(prg_rom: Vec<u8>, chr_data: Vec<u8>, mirroring: Mirroring) -> Self {
        let chr_is_ram = chr_data.is_empty();
        let chr = if chr_is_ram {
            vec![0u8; 8192]
        } else {
            chr_data
        };
        Self {
            prg_rom,
            chr,
            chr_is_ram,
            mirroring,
            prg_ram: [0; 8192],
        }
    }
}

impl Mapper for Nrom {
    fn cpu_read(&self, addr: u16) -> u8 {
        match addr {
            0x6000..=0x7FFF => self.prg_ram[(addr - 0x6000) as usize],
            0x8000..=0xFFFF => {
                let offset = (addr - 0x8000) as usize;
                if self.prg_rom.len() == 16384 {
                    // 16 KiB cart — mirror $8000-$BFFF to
                    // $C000-$FFFF.
                    self.prg_rom[offset % 16384]
                } else {
                    // 32 KiB cart — direct mapping (modulo for
                    // safety against malformed headers).
                    self.prg_rom[offset % self.prg_rom.len()]
                }
            }
            _ => 0,
        }
    }

    fn cpu_write(&mut self, addr: u16, value: u8) {
        if (0x6000..=0x7FFF).contains(&addr) {
            self.prg_ram[(addr - 0x6000) as usize] = value;
        }
        // Writes to $8000-$FFFF are ignored on NROM — there is no
        // bank-switching register to latch into.
    }

    fn chr_read(&mut self, addr: u16) -> u8 {
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

    fn snapshot(&self) -> MapperSnapshot {
        MapperSnapshot::Nrom(self.clone())
    }
}
