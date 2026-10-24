//! Sunsoft FME-7 and 5B (Mapper 69): command-register banking, a 16-bit
//! CPU-cycle IRQ counter, and on the 5B a YM2149-compatible sound chip.
//!
//! The FME-7 takes a command number at `$8000` and its parameter at
//! `$A000`: commands 0–7 bank the eight 1 KiB CHR windows, 8 the `$6000`
//! window (ROM, or RAM when bit 6 is set and enabled when bit 7 is), 9–B
//! the three switchable 8 KiB PRG windows, C the mirroring, D the IRQ
//! control and E/F the counter. The counter decrements every CPU cycle
//! while bit 7 of the control is set and raises the IRQ when it wraps
//! from 0 to `$FFFF`, if bit 0 is set. Writing the control acknowledges.
//!
//! # 5B audio
//!
//! The 5B is a YM2149 behind registers at `$C000` (select) and `$E000`
//! (data), reusing `emu198x-gi-ay-3-8910`. It runs with its clock divider
//! engaged, so it is ticked every other CPU cycle: a tone period of P
//! gives `CPU / (32 × P)`, which is the 5B's documented pitch. The
//! YM2149's finer 32-step volume curve is approximated by the AY's 16
//! steps, and the gain into the 2A03 mix is nominal, set so the three
//! channels at full volume match two 2A03 pulses at full volume.
//!
//! Facts from the NESdev wiki "Sunsoft FME-7" and "Sunsoft 5B audio"
//! pages and Mesen2's `SunsoftFme7.h` and `Sunsoft5bAudio.h` (GPL-3, read
//! for behaviour only).

use emu198x_gi_ay_3_8910::Ay3_8910;
use serde::{Deserialize, Serialize};

use crate::mapper::{Mapper, Mirroring};
use crate::snapshot::MapperSnapshot;

/// The 5B's YM2149 clock as the AY model counts it: half the NTSC CPU
/// clock, so its divide-by-8 prescaler gives the 5B's tone pitch.
const AY_CLOCK_HZ: u32 = 894_886;

/// Two 2A03 pulses at full volume, the level three full 5B channels map to.
const FULL_SCALE: f32 = 0.2584;

/// Sunsoft FME-7 / 5B, mapper 69.
#[derive(Clone, Serialize, Deserialize)]
pub struct Fme7 {
    prg_rom: Vec<u8>,
    chr: Vec<u8>,
    chr_is_ram: bool,
    prg_ram: Vec<u8>,
    command: u8,
    chr_banks: [u8; 8],
    prg_banks: [u8; 3],
    ram_select: u8,
    mirroring: Mirroring,
    irq_enabled: bool,
    counter_enabled: bool,
    counter: u16,
    irq_pending: bool,
    audio: Ay3_8910,
    audio_phase: bool,
}

impl Fme7 {
    /// Construct the board.
    #[must_use]
    pub fn new(prg_rom: Vec<u8>, chr_data: Vec<u8>) -> Self {
        let chr_is_ram = chr_data.is_empty();
        let chr = if chr_is_ram { vec![0; 8192] } else { chr_data };
        Self {
            prg_rom,
            chr,
            chr_is_ram,
            prg_ram: vec![0; 32 * 1024],
            command: 0,
            chr_banks: [0; 8],
            prg_banks: [0; 3],
            ram_select: 0,
            mirroring: Mirroring::Vertical,
            irq_enabled: false,
            counter_enabled: false,
            counter: 0,
            irq_pending: false,
            audio: Ay3_8910::new(AY_CLOCK_HZ, 1, 0),
            audio_phase: false,
        }
    }

    fn prg_8k_count(&self) -> usize {
        (self.prg_rom.len() / 8192).max(1)
    }

    fn read_prg_8k(&self, bank: usize, offset: u16) -> u8 {
        let bank = bank % self.prg_8k_count();
        self.prg_rom[bank * 8192 + usize::from(offset & 0x1FFF)]
    }

    fn ram_index(&self, addr: u16) -> usize {
        let bank = usize::from(self.ram_select & 0x3F) % (self.prg_ram.len() / 8192);
        bank * 8192 + usize::from(addr & 0x1FFF)
    }

    fn chr_index(&self, addr: u16) -> usize {
        let slot = usize::from((addr & 0x1FFF) / 0x0400);
        (usize::from(self.chr_banks[slot]) * 1024 + usize::from(addr & 0x03FF)) % self.chr.len()
    }

    /// The 5B's mixed output, 0.0 to 1.0.
    #[must_use]
    pub fn audio_level(&self) -> f32 {
        self.audio.output()
    }
}

impl Mapper for Fme7 {
    fn cpu_read(&self, addr: u16) -> u8 {
        match addr {
            0x6000..=0x7FFF => {
                if self.ram_select & 0x40 == 0 {
                    self.read_prg_8k(usize::from(self.ram_select & 0x3F), addr)
                } else if self.ram_select & 0x80 != 0 {
                    self.prg_ram[self.ram_index(addr)]
                } else {
                    0
                }
            }
            0x8000..=0xDFFF => {
                let window = usize::from((addr - 0x8000) / 0x2000);
                self.read_prg_8k(usize::from(self.prg_banks[window] & 0x3F), addr)
            }
            0xE000..=0xFFFF => self.read_prg_8k(self.prg_8k_count() - 1, addr),
            _ => 0,
        }
    }

    fn cpu_write(&mut self, addr: u16, value: u8) {
        match addr & 0xE000 {
            0x6000 => {
                if self.ram_select & 0xC0 == 0xC0 {
                    let index = self.ram_index(addr);
                    self.prg_ram[index] = value;
                }
            }
            0x8000 => self.command = value & 0x0F,
            0xA000 => match self.command {
                0..=7 => self.chr_banks[usize::from(self.command)] = value,
                8 => self.ram_select = value,
                9..=0x0B => self.prg_banks[usize::from(self.command - 9)] = value,
                0x0C => {
                    self.mirroring = match value & 0x03 {
                        0 => Mirroring::Vertical,
                        1 => Mirroring::Horizontal,
                        2 => Mirroring::SingleScreenLower,
                        _ => Mirroring::SingleScreenUpper,
                    };
                }
                0x0D => {
                    self.irq_enabled = value & 0x01 != 0;
                    self.counter_enabled = value & 0x80 != 0;
                    self.irq_pending = false;
                }
                0x0E => self.counter = (self.counter & 0xFF00) | u16::from(value),
                _ => self.counter = (self.counter & 0x00FF) | (u16::from(value) << 8),
            },
            0xC000 => self.audio.select_register(value & 0x0F),
            0xE000 => self.audio.write_data(value),
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
        self.mirroring
    }

    fn irq_pending(&self) -> bool {
        self.irq_pending
    }

    fn cpu_tick(&mut self) {
        if self.counter_enabled {
            self.counter = self.counter.wrapping_sub(1);
            if self.counter == 0xFFFF && self.irq_enabled {
                self.irq_pending = true;
            }
        }
        self.audio_phase = !self.audio_phase;
        if self.audio_phase {
            self.audio.tick();
        }
    }

    fn expansion_audio_sample(&self) -> f32 {
        self.audio.output() * FULL_SCALE
    }

    fn snapshot(&self) -> MapperSnapshot {
        MapperSnapshot::Fme7(Box::new(self.clone()))
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

    fn board() -> Fme7 {
        let prg: Vec<u8> = (0..32 * 8192).map(|i| (i / 8192) as u8).collect();
        Fme7::new(prg, vec![0; 8192])
    }

    fn command(fme: &mut Fme7, cmd: u8, value: u8) {
        fme.cpu_write(0x8000, cmd);
        fme.cpu_write(0xA000, value);
    }

    #[test]
    fn commands_bank_prg_and_select_ram_at_6000() {
        let mut fme = board();
        command(&mut fme, 9, 4);
        command(&mut fme, 0x0B, 7);
        assert_eq!(fme.cpu_read(0x8000), 4);
        assert_eq!(fme.cpu_read(0xC000), 7);
        assert_eq!(fme.cpu_read(0xE000), 31);
        command(&mut fme, 8, 5);
        assert_eq!(fme.cpu_read(0x6000), 5, "ROM at $6000");
        command(&mut fme, 8, 0xC1);
        fme.cpu_write(0x6000, 0x42);
        assert_eq!(fme.cpu_read(0x6000), 0x42, "enabled RAM bank 1");
        command(&mut fme, 8, 0x41);
        assert_eq!(fme.cpu_read(0x6000), 0, "RAM selected but disabled");
        command(&mut fme, 0x0C, 3);
        assert_eq!(fme.mirroring(), Mirroring::SingleScreenUpper);
    }

    #[test]
    fn the_counter_raises_the_irq_when_it_wraps() {
        let mut fme = board();
        command(&mut fme, 0x0E, 2);
        command(&mut fme, 0x0F, 0);
        command(&mut fme, 0x0D, 0x81);
        fme.cpu_tick();
        fme.cpu_tick();
        assert!(!fme.irq_pending(), "2 -> 1 -> 0");
        fme.cpu_tick();
        assert!(fme.irq_pending(), "0 -> $FFFF");
        command(&mut fme, 0x0D, 0x80);
        assert!(!fme.irq_pending(), "writing the control acknowledges");
    }

    #[test]
    fn a_5b_tone_has_the_documented_pitch() {
        let mut fme = board();
        let reg = |fme: &mut Fme7, r: u8, v: u8| {
            fme.cpu_write(0xC000, r);
            fme.cpu_write(0xE000, v);
        };
        reg(&mut fme, 0, 100); // tone A period 100
        reg(&mut fme, 1, 0);
        reg(&mut fme, 7, 0b0011_1110); // tone A only
        reg(&mut fme, 8, 15);
        let mut last = fme.audio_level();
        let mut edges = 0;
        let cycles = 32 * 100 * 10;
        for _ in 0..cycles {
            fme.cpu_tick();
            let now = fme.audio_level();
            if (now - last).abs() > f32::EPSILON {
                edges += 1;
                last = now;
            }
        }
        // CPU / (32 x 100): ten periods, two edges each.
        assert!((19..=21).contains(&edges), "{edges}");
        assert!(fme.expansion_audio_sample() <= FULL_SCALE);
    }
}
