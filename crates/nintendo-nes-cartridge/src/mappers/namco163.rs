//! Namco 163 (Mapper 19): PRG and CHR banking, nametables that can come
//! from CHR ROM, a 15-bit IRQ counter, and up to eight wavetable sound
//! channels.
//!
//! # Banking
//!
//! `$8000`–`$BFFF` select the eight 1 KiB CHR windows and `$C000`–`$DFFF`
//! the four nametables. A value of `$E0` or above selects a page of the
//! console's 2 KiB nametable RAM (bit 0) instead of CHR ROM — always for
//! the nametables, and for the pattern windows unless `$E800` bit 6 (low
//! half) or 7 (high half) turns that off. Because a nametable can be CHR
//! ROM, the board answers every nametable access itself and keeps its own
//! copy of the console's nametable RAM.
//!
//! # Audio
//!
//! 128 bytes of internal RAM, reached through `$F800` (address and
//! auto-increment) and `$4800` (data), hold the waveforms as 4-bit samples
//! and, from `$40`, eight bytes of registers per channel: a 24-bit phase,
//! an 18-bit frequency, the waveform length and address, and a 4-bit
//! volume. `$7F` bits 4–6 give the number of enabled channels less one,
//! counted down from channel 7. One channel is updated every 15 CPU
//! cycles, round-robin.
//!
//! The chip outputs one channel at a time, so with many channels enabled
//! the result is a fast multiplex that the console's filtering smooths.
//! This model outputs the average of the enabled channels instead, the
//! approximation Mesen2 also makes; the high-pitched multiplex whine some
//! hardware lets through is not reproduced. The gain into the 2A03 mix is
//! nominal.
//!
//! Facts from the NESdev wiki "Namco 163" and "Namco 163 audio" pages and
//! Mesen2's `Namco163.h` and `Namco163Audio.h` (GPL-3, read for behaviour
//! only). The related Namco 175 and 340 boards (mapper 210) are not here.

use serde::{Deserialize, Serialize};
use serde_big_array::BigArray;

use crate::mapper::{Mapper, Mirroring};
use crate::snapshot::MapperSnapshot;

/// CPU cycles between channel updates.
const UPDATE_CYCLES: u8 = 15;

/// One channel at full volume with a full-scale sample, `7 × 15`.
const CHANNEL_FULL_SCALE: f32 = 105.0;

/// The level one full-scale channel maps to: a 2A03 pulse at full volume.
const FULL_SCALE: f32 = 0.1494;

/// Namco 163, mapper 19.
#[derive(Clone, Serialize, Deserialize)]
pub struct Namco163 {
    prg_rom: Vec<u8>,
    chr: Vec<u8>,
    chr_is_ram: bool,
    prg_ram: Vec<u8>,
    #[serde(with = "BigArray")]
    ciram: [u8; 2048],
    prg_banks: [u8; 3],
    chr_banks: [u8; 8],
    nt_banks: [u8; 4],
    low_chr_rom_only: bool,
    high_chr_rom_only: bool,
    irq_counter: u16,
    irq_pending: bool,
    #[serde(with = "BigArray")]
    sound_ram: [u8; 128],
    ram_address: u8,
    auto_increment: bool,
    sound_disabled: bool,
    update_counter: u8,
    channel: u8,
    channel_output: [i16; 8],
}

impl Namco163 {
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
            ciram: [0; 2048],
            prg_banks: [0; 3],
            chr_banks: [0; 8],
            nt_banks: [0xE0, 0xE1, 0xE0, 0xE1],
            low_chr_rom_only: false,
            high_chr_rom_only: false,
            irq_counter: 0,
            irq_pending: false,
            sound_ram: [0; 128],
            ram_address: 0,
            auto_increment: false,
            sound_disabled: false,
            update_counter: 0,
            channel: 7,
            channel_output: [0; 8],
        }
    }

    fn prg_8k_count(&self) -> usize {
        (self.prg_rom.len() / 8192).max(1)
    }

    fn read_prg_8k(&self, bank: usize, offset: u16) -> u8 {
        let bank = bank % self.prg_8k_count();
        self.prg_rom[bank * 8192 + usize::from(offset & 0x1FFF)]
    }

    /// Where a 1 KiB page selection points: nametable RAM, or CHR.
    fn page(&self, value: u8, allow_ciram: bool, offset: u16) -> Page {
        if allow_ciram && value >= 0xE0 {
            Page::Ciram(usize::from(value & 1) * 1024 + usize::from(offset & 0x03FF))
        } else {
            Page::Chr((usize::from(value) * 1024 + usize::from(offset & 0x03FF)) % self.chr.len())
        }
    }

    fn pattern_page(&self, addr: u16) -> Page {
        let slot = usize::from((addr & 0x1FFF) / 0x0400);
        let rom_only = if slot < 4 {
            self.low_chr_rom_only
        } else {
            self.high_chr_rom_only
        };
        self.page(self.chr_banks[slot], !rom_only, addr)
    }

    fn nametable_page(&self, addr: u16) -> Page {
        let slot = usize::from(((addr - 0x2000) & 0x0FFF) / 0x0400);
        self.page(self.nt_banks[slot], true, addr)
    }

    fn read_page(&self, page: Page) -> u8 {
        match page {
            Page::Ciram(i) => self.ciram[i],
            Page::Chr(i) => self.chr[i],
        }
    }

    fn write_page(&mut self, page: Page, value: u8) {
        match page {
            Page::Ciram(i) => self.ciram[i] = value,
            Page::Chr(i) if self.chr_is_ram => self.chr[i] = value,
            Page::Chr(_) => {}
        }
    }

    fn enabled_channels(&self) -> u8 {
        ((self.sound_ram[0x7F] >> 4) & 0x07) + 1
    }

    fn update_channel(&mut self, ch: u8) {
        let base = 0x40 + usize::from(ch) * 8;
        let r = &self.sound_ram;
        let freq = (u32::from(r[base + 4] & 0x03) << 16)
            | (u32::from(r[base + 2]) << 8)
            | u32::from(r[base]);
        let phase =
            (u32::from(r[base + 5]) << 16) | (u32::from(r[base + 3]) << 8) | u32::from(r[base + 1]);
        let length = 256 - u32::from(r[base + 4] & 0xFC);
        let offset = u32::from(r[base + 6]);
        let volume = i16::from(r[base + 7] & 0x0F);
        let phase = (phase + freq) % (length << 16);
        let position = (((phase >> 16) + offset) & 0xFF) as usize;
        let byte = r[position / 2];
        let sample = if position & 1 != 0 {
            byte >> 4
        } else {
            byte & 0x0F
        };
        self.channel_output[usize::from(ch)] = (i16::from(sample) - 8) * volume;
        self.sound_ram[base + 5] = (phase >> 16) as u8;
        self.sound_ram[base + 3] = (phase >> 8) as u8;
        self.sound_ram[base + 1] = phase as u8;
    }

    /// The averaged output of the enabled channels.
    #[must_use]
    pub fn audio_level(&self) -> i16 {
        let count = self.enabled_channels();
        let first = 8 - count;
        let sum: i16 = self.channel_output[usize::from(first)..].iter().sum();
        sum / i16::from(count)
    }
}

/// A resolved 1 KiB page.
#[derive(Clone, Copy)]
enum Page {
    Ciram(usize),
    Chr(usize),
}

impl Mapper for Namco163 {
    fn cpu_read(&self, addr: u16) -> u8 {
        match addr {
            0x5000..=0x57FF => self.irq_counter as u8,
            0x5800..=0x5FFF => (self.irq_counter >> 8) as u8,
            0x6000..=0x7FFF => self.prg_ram[usize::from(addr - 0x6000)],
            0x8000..=0xDFFF => {
                let window = usize::from((addr - 0x8000) / 0x2000);
                self.read_prg_8k(usize::from(self.prg_banks[window] & 0x3F), addr)
            }
            0xE000..=0xFFFF => self.read_prg_8k(self.prg_8k_count() - 1, addr),
            _ => 0,
        }
    }

    fn cpu_read_side_effect(&mut self, addr: u16) -> u8 {
        if (0x4800..=0x4FFF).contains(&addr) {
            let value = self.sound_ram[usize::from(self.ram_address)];
            if self.auto_increment {
                self.ram_address = (self.ram_address + 1) & 0x7F;
            }
            return value;
        }
        self.cpu_read(addr)
    }

    fn cpu_read_expansion(&mut self, addr: u16) -> Option<u8> {
        (0x4800..=0x5FFF)
            .contains(&addr)
            .then(|| self.cpu_read_side_effect(addr))
    }

    fn cpu_write(&mut self, addr: u16, value: u8) {
        match addr & 0xF800 {
            0x4800 => {
                self.sound_ram[usize::from(self.ram_address)] = value;
                if self.auto_increment {
                    self.ram_address = (self.ram_address + 1) & 0x7F;
                }
            }
            0x5000 => {
                self.irq_counter = (self.irq_counter & 0xFF00) | u16::from(value);
                self.irq_pending = false;
            }
            0x5800 => {
                self.irq_counter = (self.irq_counter & 0x00FF) | (u16::from(value) << 8);
                self.irq_pending = false;
            }
            0x6000 | 0x6800 | 0x7000 | 0x7800 => {
                self.prg_ram[usize::from(addr - 0x6000)] = value;
            }
            0x8000..=0xB800 => self.chr_banks[usize::from((addr - 0x8000) >> 11)] = value,
            0xC000..=0xD800 => self.nt_banks[usize::from((addr - 0xC000) >> 11)] = value,
            0xE000 => {
                self.prg_banks[0] = value & 0x3F;
                self.sound_disabled = value & 0x40 != 0;
            }
            0xE800 => {
                self.prg_banks[1] = value & 0x3F;
                self.low_chr_rom_only = value & 0x40 != 0;
                self.high_chr_rom_only = value & 0x80 != 0;
            }
            0xF000 => self.prg_banks[2] = value & 0x3F,
            0xF800 => {
                self.ram_address = value & 0x7F;
                self.auto_increment = value & 0x80 != 0;
            }
            _ => {}
        }
    }

    fn chr_read(&mut self, addr: u16) -> u8 {
        let page = self.pattern_page(addr);
        self.read_page(page)
    }

    fn chr_write(&mut self, addr: u16, value: u8) {
        let page = self.pattern_page(addr);
        self.write_page(page, value);
    }

    fn mirroring(&self) -> Mirroring {
        // Every nametable access is answered by `nametable_read`; this is
        // only what a debugger shows.
        Mirroring::Vertical
    }

    fn irq_pending(&self) -> bool {
        self.irq_pending
    }

    fn cpu_tick(&mut self) {
        if self.irq_counter & 0x8000 != 0 && self.irq_counter & 0x7FFF != 0x7FFF {
            self.irq_counter += 1;
            if self.irq_counter & 0x7FFF == 0x7FFF {
                self.irq_pending = true;
            }
        }
        if self.sound_disabled {
            return;
        }
        self.update_counter += 1;
        if self.update_counter == UPDATE_CYCLES {
            self.update_counter = 0;
            self.update_channel(self.channel);
            let first = 8 - self.enabled_channels();
            self.channel = if self.channel <= first {
                7
            } else {
                self.channel - 1
            };
        }
    }

    fn expansion_audio_sample(&self) -> f32 {
        f32::from(self.audio_level()) / CHANNEL_FULL_SCALE * FULL_SCALE
    }

    fn nametable_read(&mut self, addr: u16) -> Option<u8> {
        let page = self.nametable_page(addr);
        Some(self.read_page(page))
    }

    fn nametable_peek(&self, addr: u16) -> Option<u8> {
        Some(self.read_page(self.nametable_page(addr)))
    }

    fn nametable_write(&mut self, addr: u16, value: u8) -> bool {
        let page = self.nametable_page(addr);
        self.write_page(page, value);
        true
    }

    fn snapshot(&self) -> MapperSnapshot {
        MapperSnapshot::Namco163(Box::new(self.clone()))
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

    fn board() -> Namco163 {
        let prg: Vec<u8> = (0..16 * 8192).map(|i| (i / 8192) as u8).collect();
        let chr: Vec<u8> = (0..64 * 1024).map(|i| (i / 1024) as u8).collect();
        Namco163::new(prg, chr)
    }

    fn poke_sound(n: &mut Namco163, addr: u8, bytes: &[u8]) {
        n.cpu_write(0xF800, 0x80 | addr);
        for &b in bytes {
            n.cpu_write(0x4800, b);
        }
    }

    #[test]
    fn prg_chr_and_nametable_banks() {
        let mut n = board();
        n.cpu_write(0xE000, 3);
        n.cpu_write(0xF000, 9);
        assert_eq!(n.cpu_read(0x8000), 3);
        assert_eq!(n.cpu_read(0xC000), 9);
        assert_eq!(n.cpu_read(0xE000), 15);
        n.cpu_write(0x8800, 21);
        assert_eq!(n.chr_read(0x0400), 21);
        // A nametable from CHR ROM page 5, and one from nametable RAM.
        n.cpu_write(0xC000, 5);
        n.cpu_write(0xC800, 0xE1);
        assert_eq!(n.nametable_read(0x2000), Some(5));
        assert!(n.nametable_write(0x2400, 0x77));
        assert_eq!(n.nametable_read(0x2400), Some(0x77));
        assert!(
            n.nametable_write(0x2000, 0x11),
            "ROM nametables swallow writes"
        );
        assert_eq!(n.nametable_read(0x2000), Some(5));
        // A pattern window on nametable RAM, unless $E800 forbids it.
        n.cpu_write(0x8000, 0xE1);
        assert_eq!(n.chr_read(0x0000), 0x77);
        n.cpu_write(0xE800, 0x40);
        assert_eq!(
            n.chr_read(0x0000),
            0xE1 % 64,
            "CHR ROM page $E1, wrapped to 64 KiB"
        );
    }

    #[test]
    fn the_irq_counts_up_to_7fff() {
        let mut n = board();
        n.cpu_write(0x5000, 0xFD);
        n.cpu_write(0x5800, 0xFF);
        n.cpu_tick();
        assert!(!n.irq_pending());
        n.cpu_tick();
        assert!(n.irq_pending());
        assert_eq!(n.cpu_read(0x5000), 0xFF);
        n.cpu_tick();
        assert_eq!(n.cpu_read(0x5000), 0xFF, "it stops at $7FFF");
        n.cpu_write(0x5000, 0);
        assert!(!n.irq_pending());
    }

    #[test]
    fn sound_ram_auto_increments_both_ways() {
        let mut n = board();
        poke_sound(&mut n, 0x10, &[1, 2, 3]);
        n.cpu_write(0xF800, 0x90);
        assert_eq!(n.cpu_read_expansion(0x4800), Some(1));
        assert_eq!(n.cpu_read_expansion(0x4800), Some(2));
        assert_eq!(n.cpu_read_expansion(0x4800), Some(3));
        assert_eq!(n.cpu_read_expansion(0x6000), None);
    }

    #[test]
    fn one_channel_plays_its_waveform_at_its_frequency() {
        let mut n = board();
        // A 16-sample square: eight samples of 15, eight of 0.
        poke_sound(&mut n, 0x00, &[0xFF, 0xFF, 0xFF, 0xFF, 0, 0, 0, 0]);
        // Channel 7 at $78: frequency $10000 steps one sample per update;
        // length 256 - $F0 = 16 samples; wave at 0; volume 15; one channel.
        poke_sound(
            &mut n,
            0x78,
            &[0x00, 0x00, 0x00, 0x00, 0xF1, 0x00, 0x00, 0x0F],
        );
        let mut levels = Vec::new();
        for _ in 0..16 {
            for _ in 0..UPDATE_CYCLES {
                n.cpu_tick();
            }
            levels.push(n.audio_level());
        }
        assert_eq!(
            levels[..7],
            [105; 7],
            "first eight samples high: (15 - 8) x 15"
        );
        assert_eq!(levels[7..15], [-120; 8], "then (0 - 8) x 15");
        assert!(n.expansion_audio_sample() != 0.0);
        n.cpu_write(0xE000, 0x40);
        let held = n.audio_level();
        for _ in 0..1000 {
            n.cpu_tick();
        }
        assert_eq!(n.audio_level(), held, "$E000 bit 6 stops the sound");
    }

    #[test]
    fn many_channels_average() {
        let mut n = board();
        poke_sound(&mut n, 0x00, &[0xFF]);
        // Two channels (7 and 6), both on a constant 15 sample, volume 15
        // and 0; $7F also holds channel 7's volume, so set the count there.
        poke_sound(&mut n, 0x70, &[0, 0, 0, 0, 0xFC, 0, 0, 0x00]);
        poke_sound(&mut n, 0x78, &[0, 0, 0, 0, 0xFC, 0, 0, 0x1F]);
        for _ in 0..UPDATE_CYCLES * 4 {
            n.cpu_tick();
        }
        assert_eq!(n.audio_level(), 105 / 2);
    }
}
