//! Konami VRC7 (Mapper 85): PRG and CHR banking, the VRC IRQ, and a
//! six-channel FM synthesiser — the YM2413-derived DS1001, modelled by
//! `emu198x-yamaha-ym2413`'s VRC7 variant.
//!
//! Two board wirings exist: VRC7a (*Lagrange Point*) selects registers
//! with A4 and VRC7b (*Tiny Toon Adventures 2*) with A3. Without a NES 2.0
//! submapper the two are told apart the way Mesen2 does: any write with A4
//! set, other than the audio ports at `$9010` and `$9030`, is treated as
//! the A3 register.
//!
//! The synthesiser runs from the board's 3.58 MHz crystal, twice the NTSC
//! CPU clock, so it takes two input clocks per CPU cycle. Each completed
//! sample (one per 72 input clocks, about 49.7 kHz) sets the level fed to
//! the cartridge audio pin until the next. `$E000` bit 6 silences the
//! audio and makes the chip ignore writes. The gain into the 2A03 mix is
//! nominal: one channel at full volume peaks near one 2A03 pulse.
//!
//! Facts from the NESdev wiki "VRC7" and "VRC7 audio" pages and Mesen2's
//! `VRC7.h` and `Vrc7Audio.h` (GPL-3, read for behaviour only).

use emu198x_yamaha_ym2413::{Variant, Ym2413};
use serde::{Deserialize, Serialize};

use super::vrc_irq::VrcIrq;
use crate::mapper::{Mapper, Mirroring};
use crate::snapshot::MapperSnapshot;

/// The melody-pin sample one channel at full volume peaks at, measured
/// from this model (a keyed instrument-1 note at volume 0 peaks at 261).
/// The DS1001 puts its six channels on one pin, so each 18-slot sample is
/// the sum of every channel's output.
const FULL_SCALE_SUM: f32 = 256.0;

/// One 2A03 pulse at full volume.
const PULSE_LEVEL: f32 = 0.1494;

/// Konami VRC7, mapper 85.
#[derive(Clone, Serialize, Deserialize)]
pub struct Vrc7 {
    prg_rom: Vec<u8>,
    chr: Vec<u8>,
    chr_is_ram: bool,
    prg_ram: Vec<u8>,
    prg_banks: [u8; 3],
    chr_banks: [u8; 8],
    control: u8,
    irq: VrcIrq,
    fm: Ym2413,
    level: i32,
}

impl Vrc7 {
    /// Construct the board.
    #[must_use]
    pub fn new(prg_rom: Vec<u8>, chr_data: Vec<u8>) -> Self {
        let chr_is_ram = chr_data.is_empty();
        let chr = if chr_is_ram { vec![0; 8192] } else { chr_data };
        Self {
            prg_rom,
            chr,
            chr_is_ram,
            prg_ram: vec![0; 8192],
            prg_banks: [0; 3],
            chr_banks: [0; 8],
            control: 0,
            irq: VrcIrq::default(),
            fm: Ym2413::new(Variant::Vrc7),
            level: 0,
        }
    }

    /// Fold the A3 and A4 wirings onto one register map.
    fn register(addr: u16) -> u16 {
        let mut addr = addr;
        if addr & 0x10 != 0 && addr & 0xF010 != 0x9010 {
            addr = (addr | 0x08) & !0x10;
        }
        addr & 0xF038
    }

    fn muted(&self) -> bool {
        self.control & 0x40 != 0
    }

    fn prg_8k_count(&self) -> usize {
        (self.prg_rom.len() / 8192).max(1)
    }

    fn read_prg_8k(&self, bank: usize, offset: u16) -> u8 {
        let bank = bank % self.prg_8k_count();
        self.prg_rom[bank * 8192 + usize::from(offset & 0x1FFF)]
    }

    fn chr_index(&self, addr: u16) -> usize {
        let slot = usize::from((addr & 0x1FFF) / 0x0400);
        (usize::from(self.chr_banks[slot]) * 1024 + usize::from(addr & 0x03FF)) % self.chr.len()
    }

    /// The latest synthesiser sample: the melody pin summed over 18 slots.
    #[must_use]
    pub fn audio_level(&self) -> i32 {
        if self.muted() { 0 } else { self.level }
    }
}

impl Mapper for Vrc7 {
    fn cpu_read(&self, addr: u16) -> u8 {
        match addr {
            0x6000..=0x7FFF if self.control & 0x80 != 0 => self.prg_ram[usize::from(addr - 0x6000)],
            0x8000..=0xDFFF => {
                let window = usize::from((addr - 0x8000) / 0x2000);
                self.read_prg_8k(usize::from(self.prg_banks[window] & 0x3F), addr)
            }
            0xE000..=0xFFFF => self.read_prg_8k(self.prg_8k_count() - 1, addr),
            _ => 0,
        }
    }

    fn cpu_write(&mut self, addr: u16, value: u8) {
        if (0x6000..=0x7FFF).contains(&addr) {
            if self.control & 0x80 != 0 {
                self.prg_ram[usize::from(addr - 0x6000)] = value;
            }
            return;
        }
        if addr < 0x8000 {
            return;
        }
        match Self::register(addr) {
            0x8000 => self.prg_banks[0] = value,
            0x8008 => self.prg_banks[1] = value,
            0x9000 => self.prg_banks[2] = value,
            0x9010 if !self.muted() => self.fm.write(0, value),
            0x9030 if !self.muted() => self.fm.write(1, value),
            reg @ (0xA000 | 0xA008 | 0xB000 | 0xB008 | 0xC000 | 0xC008 | 0xD000 | 0xD008) => {
                let slot = usize::from((reg - 0xA000) >> 12) * 2 + usize::from(reg & 0x08 != 0);
                self.chr_banks[slot] = value;
            }
            0xE000 => self.control = value,
            0xE008 => self.irq.set_reload(value),
            0xF000 => self.irq.set_control(value),
            0xF008 => self.irq.acknowledge(),
            _ => {}
        }
    }

    fn chr_read(&mut self, addr: u16) -> u8 {
        self.chr[self.chr_index(addr)]
    }

    fn chr_write(&mut self, addr: u16, value: u8) {
        if self.chr_is_ram {
            let index = self.chr_index(addr);
            self.chr[index] = value;
        }
    }

    fn mirroring(&self) -> Mirroring {
        match self.control & 0x03 {
            0 => Mirroring::Vertical,
            1 => Mirroring::Horizontal,
            2 => Mirroring::SingleScreenLower,
            _ => Mirroring::SingleScreenUpper,
        }
    }

    fn irq_pending(&self) -> bool {
        self.irq.pending()
    }

    fn cpu_tick(&mut self) {
        self.irq.cpu_tick();
        self.fm.tick();
        self.fm.tick();
        if let Some(sample) = self.fm.take_samples().last() {
            self.level = sample[0];
        }
    }

    fn expansion_audio_sample(&self) -> f32 {
        self.audio_level() as f32 / FULL_SCALE_SUM * PULSE_LEVEL
    }

    fn snapshot(&self) -> MapperSnapshot {
        MapperSnapshot::Vrc7(Box::new(self.clone()))
    }

    fn save_ram(&self) -> &[u8] {
        &self.prg_ram
    }

    fn restore_save_ram(&mut self, bytes: &[u8]) {
        let n = bytes.len().min(self.prg_ram.len());
        self.prg_ram[..n].copy_from_slice(&bytes[..n]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn board() -> Vrc7 {
        let prg: Vec<u8> = (0..16 * 8192).map(|i| (i / 8192) as u8).collect();
        let chr: Vec<u8> = (0..32 * 1024).map(|i| (i / 1024) as u8).collect();
        Vrc7::new(prg, chr)
    }

    fn fm(v: &mut Vrc7, reg: u8, value: u8) {
        v.cpu_write(0x9010, reg);
        for _ in 0..8 {
            v.cpu_tick();
        }
        v.cpu_write(0x9030, value);
        for _ in 0..48 {
            v.cpu_tick();
        }
    }

    #[test]
    fn both_wirings_bank_prg_and_chr() {
        let mut v = board();
        v.cpu_write(0x8000, 3);
        v.cpu_write(0x8010, 4); // VRC7a: A4
        v.cpu_write(0x9000, 5);
        assert_eq!(v.cpu_read(0x8000), 3);
        assert_eq!(v.cpu_read(0xA000), 4);
        assert_eq!(v.cpu_read(0xC000), 5);
        assert_eq!(v.cpu_read(0xE000), 15);
        v.cpu_write(0x8008, 6); // VRC7b: A3
        assert_eq!(v.cpu_read(0xA000), 6);
        v.cpu_write(0xB008, 21);
        v.cpu_write(0xD010, 30);
        assert_eq!(v.chr_read(0x0C00), 21);
        assert_eq!(v.chr_read(0x1C00), 30);
        v.cpu_write(0xE000, 0x83);
        assert_eq!(v.mirroring(), Mirroring::SingleScreenUpper);
        v.cpu_write(0x6000, 0x5A);
        assert_eq!(v.cpu_read(0x6000), 0x5A);
    }

    #[test]
    fn the_irq_runs_in_cycle_mode() {
        let mut v = board();
        v.cpu_write(0xE010, 0xFE);
        v.cpu_write(0xF000, 0x06);
        v.cpu_tick();
        v.cpu_tick();
        assert!(v.irq_pending());
        v.cpu_write(0xF010, 0);
        assert!(!v.irq_pending());
    }

    #[test]
    fn a_keyed_note_sounds_and_the_mute_bit_silences_it() {
        let mut v = board();
        fm(&mut v, 0x30, 0x10); // channel 0: instrument 1, full volume
        fm(&mut v, 0x10, 0xAC); // F-number low
        fm(&mut v, 0x20, 0x18); // key on, block 4
        let mut peak = 0;
        for _ in 0..20_000 {
            v.cpu_tick();
            peak = peak.max(v.audio_level().abs());
        }
        assert!(peak > 200, "the note sounds near full scale: {peak}");
        let peak_level = peak as f32 / FULL_SCALE_SUM * PULSE_LEVEL;
        assert!(
            (PULSE_LEVEL * 0.8..PULSE_LEVEL * 1.2).contains(&peak_level),
            "one full channel peaks near a 2A03 pulse: {peak_level}"
        );
        v.cpu_write(0xE000, 0x40);
        assert_eq!(v.audio_level(), 0);
        assert_eq!(v.expansion_audio_sample(), 0.0);
    }
}
