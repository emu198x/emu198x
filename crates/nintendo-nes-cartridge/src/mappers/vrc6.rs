//! Konami VRC6 (Mappers 24 and 26): PRG and CHR banking, the VRC IRQ,
//! and the expansion audio — two pulse channels and a sawtooth.
//!
//! The two mappers are the same chip wired differently: mapper 26
//! (VRC6b, *Madara* and *Esper Dream 2*) swaps the A0 and A1 lines that
//! mapper 24 (VRC6a, *Akumajou Densetsu*) uses as the register select.
//!
//! # Audio
//!
//! Each channel has a 12-bit period divider clocked by the CPU; `$9003`
//! halts all three or shifts every period right by 4 or 8 bits (16× and
//! 256× frequency, a test feature some music drivers use for effects).
//!
//! - **Pulse:** a 16-step sequencer; the channel outputs its 4-bit volume
//!   while the step is at or below the 3-bit duty, or always in digitised
//!   mode (bit 7 of the control register).
//! - **Saw:** a 14-step sequencer that adds the 6-bit rate to an 8-bit
//!   accumulator on every other step and clears it at step 0; the top five
//!   bits are the output.
//!
//! The three outputs sum to 0–61 and enter the 2A03's mix through the
//! cartridge audio pin. The gain here makes one VRC6 pulse at full volume
//! match one 2A03 pulse at full volume, which is how the board's resistor
//! network balances them; it is a nominal figure, not a measurement.
//!
//! Facts from the NESdev wiki "VRC6" and "VRC6 audio" pages and Mesen2's
//! `VRC6.h`, `Vrc6Pulse.h` and `Vrc6Saw.h` (GPL-3, read for behaviour only).
//!
//! # Not modelled
//!
//! The CHR-ROM nametable modes (`$B003` bit 4) and the non-standard
//! nametable selections that use CHR register bits as CIRAM page
//! selectors fall back to vertical mirroring. No licensed game uses them.

use serde::{Deserialize, Serialize};

use super::vrc_irq::VrcIrq;
use crate::mapper::{Mapper, Mirroring};
use crate::snapshot::MapperSnapshot;

/// Linear 2A03 pulse gain for one step of output: `95.88 / (8128 / 15 +
/// 100) / 15`, so 15 on a VRC6 pulse matches 15 on a 2A03 pulse.
const LEVEL_GAIN: f32 = 0.009_96;

/// One VRC6 pulse channel.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Vrc6Pulse {
    volume: u8,
    duty: u8,
    ignore_duty: bool,
    period: u16,
    enabled: bool,
    timer: u16,
    step: u8,
}

impl Vrc6Pulse {
    fn write(&mut self, reg: u16, value: u8) {
        match reg {
            0 => {
                self.volume = value & 0x0F;
                self.duty = (value >> 4) & 0x07;
                self.ignore_duty = value & 0x80 != 0;
            }
            1 => self.period = (self.period & 0x0F00) | u16::from(value),
            _ => {
                self.period = (self.period & 0x00FF) | (u16::from(value & 0x0F) << 8);
                self.enabled = value & 0x80 != 0;
                if !self.enabled {
                    self.step = 0;
                }
            }
        }
    }

    fn clock(&mut self, shift: u8) {
        if !self.enabled {
            return;
        }
        if self.timer == 0 {
            self.step = (self.step + 1) & 0x0F;
            self.timer = self.period >> shift;
        } else {
            self.timer -= 1;
        }
    }

    fn output(&self) -> u8 {
        if !self.enabled {
            0
        } else if self.ignore_duty || self.step <= self.duty {
            self.volume
        } else {
            0
        }
    }
}

/// The VRC6 sawtooth channel.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Vrc6Saw {
    rate: u8,
    accumulator: u8,
    period: u16,
    enabled: bool,
    timer: u16,
    step: u8,
}

impl Vrc6Saw {
    fn write(&mut self, reg: u16, value: u8) {
        match reg {
            0 => self.rate = value & 0x3F,
            1 => self.period = (self.period & 0x0F00) | u16::from(value),
            _ => {
                self.period = (self.period & 0x00FF) | (u16::from(value & 0x0F) << 8);
                self.enabled = value & 0x80 != 0;
                if !self.enabled {
                    // Clearing E forces the accumulator to zero and restarts
                    // the step sequence, but leaves the divider running.
                    self.accumulator = 0;
                    self.step = 0;
                }
            }
        }
    }

    fn clock(&mut self, shift: u8) {
        if !self.enabled {
            return;
        }
        if self.timer == 0 {
            self.timer = self.period >> shift;
            self.step = (self.step + 1) % 14;
            if self.step == 0 {
                self.accumulator = 0;
            } else if self.step & 1 == 0 {
                self.accumulator = self.accumulator.wrapping_add(self.rate);
            }
        } else {
            self.timer -= 1;
        }
    }

    fn output(&self) -> u8 {
        if self.enabled {
            self.accumulator >> 3
        } else {
            0
        }
    }
}

/// Konami VRC6, mappers 24 and 26.
#[derive(Clone, Serialize, Deserialize)]
pub struct Vrc6 {
    prg_rom: Vec<u8>,
    chr: Vec<u8>,
    chr_is_ram: bool,
    prg_ram: Vec<u8>,
    /// Mapper 26 swaps A0 and A1.
    swap_a0_a1: bool,
    prg_16k: u8,
    prg_8k: u8,
    chr_regs: [u8; 8],
    banking: u8,
    irq: VrcIrq,
    pulses: [Vrc6Pulse; 2],
    saw: Vrc6Saw,
    halt: bool,
    shift: u8,
}

impl Vrc6 {
    /// Construct the board. `mapper_26` selects the VRC6b wiring.
    #[must_use]
    pub fn new(prg_rom: Vec<u8>, chr_data: Vec<u8>, mapper_26: bool) -> Self {
        let chr_is_ram = chr_data.is_empty();
        let chr = if chr_is_ram { vec![0; 8192] } else { chr_data };
        Self {
            prg_rom,
            chr,
            chr_is_ram,
            prg_ram: vec![0; 8192],
            swap_a0_a1: mapper_26,
            prg_16k: 0,
            prg_8k: 0,
            chr_regs: [0; 8],
            banking: 0x20,
            irq: VrcIrq::default(),
            pulses: [Vrc6Pulse::default(), Vrc6Pulse::default()],
            saw: Vrc6Saw::default(),
            halt: false,
            shift: 0,
        }
    }

    /// The register address with mapper 26's A0/A1 swap undone.
    fn register(&self, addr: u16) -> u16 {
        if self.swap_a0_a1 {
            (addr & 0xF000) | ((addr & 1) << 1) | ((addr >> 1) & 1)
        } else {
            addr & 0xF003
        }
    }

    fn prg_8k_count(&self) -> usize {
        (self.prg_rom.len() / 8192).max(1)
    }

    fn read_prg_8k(&self, bank: usize, offset: u16) -> u8 {
        let bank = bank % self.prg_8k_count();
        self.prg_rom[bank * 8192 + usize::from(offset)]
    }

    /// The 1 KiB CHR bank for one slot, in the banking mode `$B003`
    /// selects.
    fn chr_bank(&self, slot: usize) -> usize {
        let regs = &self.chr_regs;
        let (mask, or) = if self.banking & 0x20 != 0 {
            (0xFE, 1)
        } else {
            (0xFF, 0)
        };
        let bank = match self.banking & 0x03 {
            0 => regs[slot],
            1 => (regs[slot / 2] & mask) | if slot % 2 == 1 { or } else { 0 },
            _ if slot < 4 => regs[slot],
            _ => (regs[4 + (slot - 4) / 2] & mask) | if slot % 2 == 1 { or } else { 0 },
        };
        usize::from(bank)
    }

    fn chr_index(&self, addr: u16) -> usize {
        let slot = usize::from((addr & 0x1FFF) / 0x0400);
        (self.chr_bank(slot) * 1024 + usize::from(addr & 0x03FF)) % self.chr.len()
    }

    /// The summed audio level, 0-61.
    #[must_use]
    pub fn audio_level(&self) -> u8 {
        self.pulses[0].output() + self.pulses[1].output() + self.saw.output()
    }
}

impl Mapper for Vrc6 {
    fn cpu_read(&self, addr: u16) -> u8 {
        match addr {
            0x6000..=0x7FFF if self.banking & 0x80 != 0 => self.prg_ram[usize::from(addr - 0x6000)],
            0x8000..=0xBFFF => {
                let bank =
                    usize::from(self.prg_16k & 0x0F) * 2 + usize::from((addr - 0x8000) / 0x2000);
                self.read_prg_8k(bank, addr & 0x1FFF)
            }
            0xC000..=0xDFFF => self.read_prg_8k(usize::from(self.prg_8k & 0x1F), addr - 0xC000),
            0xE000..=0xFFFF => self.read_prg_8k(self.prg_8k_count() - 1, addr - 0xE000),
            _ => 0,
        }
    }

    fn cpu_write(&mut self, addr: u16, value: u8) {
        if (0x6000..=0x7FFF).contains(&addr) {
            if self.banking & 0x80 != 0 {
                self.prg_ram[usize::from(addr - 0x6000)] = value;
            }
            return;
        }
        if addr < 0x8000 {
            return;
        }
        let reg = self.register(addr);
        match reg {
            0x8000..=0x8003 => self.prg_16k = value,
            0x9000..=0x9002 => self.pulses[0].write(reg & 3, value),
            0x9003 => {
                self.halt = value & 0x01 != 0;
                self.shift = if value & 0x04 != 0 {
                    8
                } else if value & 0x02 != 0 {
                    4
                } else {
                    0
                };
            }
            0xA000..=0xA002 => self.pulses[1].write(reg & 3, value),
            0xB000..=0xB002 => self.saw.write(reg & 3, value),
            0xB003 => self.banking = value,
            0xC000..=0xC003 => self.prg_8k = value,
            0xD000..=0xD003 => self.chr_regs[usize::from(reg & 3)] = value,
            0xE000..=0xE003 => self.chr_regs[4 + usize::from(reg & 3)] = value,
            0xF000 => self.irq.set_reload(value),
            0xF001 => self.irq.set_control(value),
            0xF002 => self.irq.acknowledge(),
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
        match self.banking & 0x2F {
            0x23 | 0x24 => Mirroring::Horizontal,
            0x28 | 0x2F => Mirroring::SingleScreenLower,
            0x2B | 0x2C => Mirroring::SingleScreenUpper,
            _ => Mirroring::Vertical,
        }
    }

    fn irq_pending(&self) -> bool {
        self.irq.pending()
    }

    fn cpu_tick(&mut self) {
        self.irq.cpu_tick();
        if !self.halt {
            let shift = self.shift;
            self.pulses[0].clock(shift);
            self.pulses[1].clock(shift);
            self.saw.clock(shift);
        }
    }

    fn expansion_audio_sample(&self) -> f32 {
        f32::from(self.audio_level()) * LEVEL_GAIN
    }

    fn snapshot(&self) -> MapperSnapshot {
        MapperSnapshot::Vrc6(self.clone())
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

    fn board(mapper_26: bool) -> Vrc6 {
        let prg: Vec<u8> = (0..32 * 8192).map(|i| (i / 8192) as u8).collect();
        Vrc6::new(prg, vec![0; 8192], mapper_26)
    }

    /// Count how often the summed level changes over `cycles` CPU cycles.
    fn edges(vrc: &mut Vrc6, cycles: usize) -> usize {
        let mut last = vrc.audio_level();
        let mut edges = 0;
        for _ in 0..cycles {
            vrc.cpu_tick();
            let now = vrc.audio_level();
            if now != last {
                edges += 1;
                last = now;
            }
        }
        edges
    }

    #[test]
    fn prg_banks_map_16k_8k_and_the_fixed_last_bank() {
        let mut vrc = board(false);
        vrc.cpu_write(0x8000, 3);
        vrc.cpu_write(0xC000, 9);
        assert_eq!(vrc.cpu_read(0x8000), 6);
        assert_eq!(vrc.cpu_read(0xA000), 7);
        assert_eq!(vrc.cpu_read(0xC000), 9);
        assert_eq!(vrc.cpu_read(0xE000), 31);
    }

    #[test]
    fn a_pulse_at_half_duty_toggles_at_its_period() {
        let mut vrc = board(false);
        vrc.cpu_write(0x9000, 0x7F); // duty 7 of 16, volume 15
        vrc.cpu_write(0x9001, 99); // period 99: 100 cycles a step
        vrc.cpu_write(0x9002, 0x80);
        // A 16-step cycle is 1600 cycles with two level changes.
        assert_eq!(edges(&mut vrc, 16_000), 20);
        assert!(vrc.expansion_audio_sample() >= 0.0);
    }

    #[test]
    fn the_saw_ramps_and_resets_every_14_steps() {
        let mut vrc = board(false);
        vrc.cpu_write(0xB000, 42);
        vrc.cpu_write(0xB001, 0);
        vrc.cpu_write(0xB002, 0x80);
        let mut peak = 0;
        let mut seen_zero_after_peak = false;
        for _ in 0..14 {
            vrc.cpu_tick();
            peak = peak.max(vrc.audio_level());
            if peak > 0 && vrc.audio_level() == 0 {
                seen_zero_after_peak = true;
            }
        }
        assert_eq!(
            peak,
            ((6 * 42u16) >> 3) as u8,
            "six adds of 42 before the reset"
        );
        assert!(seen_zero_after_peak);
    }

    #[test]
    fn the_frequency_shift_and_halt_bits_apply_to_every_channel() {
        let mut vrc = board(false);
        vrc.cpu_write(0x9000, 0x7F);
        vrc.cpu_write(0x9001, 0xFF);
        vrc.cpu_write(0x9002, 0x8F); // period $FFF
        vrc.cpu_write(0x9003, 0x04); // >> 8: period 15
        assert_eq!(edges(&mut vrc, 16 * 16), 2);
        vrc.cpu_write(0x9003, 0x01);
        assert_eq!(edges(&mut vrc, 10_000), 0, "halted");
    }

    #[test]
    fn mapper_26_swaps_the_register_lines() {
        let mut vrc = board(true);
        vrc.cpu_write(0x9002, 0x7F); // A1 high: register 1 on mapper 26
        vrc.cpu_write(0x9001, 0x80); // register 2: enable
        vrc.cpu_write(0x9000, 0x8F); // register 0: digitised, volume 15
        vrc.cpu_tick();
        assert_eq!(vrc.audio_level(), 15);
    }

    #[test]
    fn the_irq_raises_and_mirroring_follows_b003() {
        let mut vrc = board(false);
        vrc.cpu_write(0xF000, 0xFE);
        vrc.cpu_write(0xF001, 0x06);
        vrc.cpu_tick();
        vrc.cpu_tick();
        assert!(vrc.irq_pending());
        vrc.cpu_write(0xF002, 0);
        assert!(!vrc.irq_pending());
        vrc.cpu_write(0xB003, 0x24);
        assert_eq!(vrc.mirroring(), Mirroring::Horizontal);
        vrc.cpu_write(0xB003, 0xAC);
        assert_eq!(vrc.mirroring(), Mirroring::SingleScreenUpper);
        vrc.cpu_write(0x6000, 0x5A);
        assert_eq!(vrc.cpu_read(0x6000), 0x5A, "bit 7 enables PRG RAM");
    }

    #[test]
    fn chr_banking_modes_pair_registers() {
        let prg = vec![0; 4 * 8192];
        let chr: Vec<u8> = (0..64 * 1024).map(|i| (i / 1024) as u8).collect();
        let mut vrc = Vrc6::new(prg, chr, false);
        for (i, bank) in [10u8, 11, 12, 13, 14, 15, 16, 17].iter().enumerate() {
            vrc.cpu_write(0xD000 + (i as u16 & 3) + 0x1000 * (i as u16 / 4), *bank);
        }
        assert_eq!(vrc.chr_read(0x0400), 11, "mode 0: 1 KiB each");
        vrc.cpu_write(0xB003, 0x21);
        assert_eq!(vrc.chr_read(0x0400), 11, "mode 1 pairs R0: 10 & $FE | 1");
        assert_eq!(vrc.chr_read(0x0800), 10, "R1 & $FE");
        vrc.cpu_write(0xB003, 0x22);
        assert_eq!(vrc.chr_read(0x1400), 15, "mode 2 pairs R4 above $1000");
    }
}
