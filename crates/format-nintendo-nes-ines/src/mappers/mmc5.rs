//! MMC5 / ExROM (Mapper 5): PRG/CHR banking, extended RAM, MMC5
//! nametable mapping, fill mode, multiplier registers, plus pulse
//! and PCM expansion audio.

use serde::{Deserialize, Serialize};

use crate::mapper::{Mapper, Mirroring};
use crate::snapshot::MapperSnapshot;

const MMC5_LENGTH_TABLE: [u8; 32] = [
    10, 254, 20, 2, 40, 4, 80, 6, 160, 8, 60, 10, 14, 12, 26, 14, 12, 16, 24, 18, 48, 20, 96, 22,
    192, 24, 72, 26, 16, 28, 32, 30,
];
const MMC5_PULSE_DUTY: [[bool; 8]; 4] = [
    [false, true, false, false, false, false, false, false],
    [false, true, true, false, false, false, false, false],
    [false, true, true, true, true, false, false, false],
    [true, false, false, true, true, true, true, true],
];

#[derive(Clone, Serialize, Deserialize)]
struct Mmc5Envelope {
    start_flag: bool,
    divider: u8,
    decay_level: u8,
    volume: u8,
    constant_volume: bool,
    loop_flag: bool,
}

impl Mmc5Envelope {
    fn new() -> Self {
        Self {
            start_flag: false,
            divider: 0,
            decay_level: 0,
            volume: 0,
            constant_volume: false,
            loop_flag: false,
        }
    }

    fn clock(&mut self) {
        if self.start_flag {
            self.start_flag = false;
            self.decay_level = 15;
            self.divider = self.volume;
        } else if self.divider == 0 {
            self.divider = self.volume;
            if self.decay_level > 0 {
                self.decay_level -= 1;
            } else if self.loop_flag {
                self.decay_level = 15;
            }
        } else {
            self.divider -= 1;
        }
    }

    fn output(&self) -> u8 {
        if self.constant_volume {
            self.volume
        } else {
            self.decay_level
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
struct Mmc5Pulse {
    timer_period: u16,
    timer: u16,
    duty_pos: u8,
    duty: u8,
    envelope: Mmc5Envelope,
    length_counter: u8,
    length_halt: bool,
    enabled: bool,
}

impl Mmc5Pulse {
    fn new() -> Self {
        Self {
            timer_period: 0,
            timer: 0,
            duty_pos: 0,
            duty: 0,
            envelope: Mmc5Envelope::new(),
            length_counter: 0,
            length_halt: false,
            enabled: false,
        }
    }

    fn write_control(&mut self, value: u8) {
        self.duty = (value >> 6) & 0x03;
        self.length_halt = value & 0x20 != 0;
        self.envelope.loop_flag = self.length_halt;
        self.envelope.constant_volume = value & 0x10 != 0;
        self.envelope.volume = value & 0x0F;
    }

    fn write_timer_low(&mut self, value: u8) {
        self.timer_period = (self.timer_period & 0x0700) | u16::from(value);
    }

    fn write_timer_high(&mut self, value: u8) {
        self.timer_period = (self.timer_period & 0x00FF) | (u16::from(value & 0x07) << 8);
        self.duty_pos = 0;
        self.envelope.start_flag = true;
        if self.enabled {
            self.length_counter = MMC5_LENGTH_TABLE[usize::from(value >> 3)];
        }
    }

    fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        if !enabled {
            self.length_counter = 0;
        }
    }

    fn clock_timer(&mut self) {
        if self.timer == 0 {
            self.timer = self.timer_period;
            self.duty_pos = (self.duty_pos + 1) & 0x07;
        } else {
            self.timer -= 1;
        }
    }

    fn clock_envelope_and_length(&mut self) {
        self.envelope.clock();
        if !self.length_halt && self.length_counter > 0 {
            self.length_counter -= 1;
        }
    }

    fn output(&self) -> u8 {
        if self.length_counter == 0 || self.timer_period < 8 {
            return 0;
        }
        if !MMC5_PULSE_DUTY[self.duty as usize][self.duty_pos as usize] {
            return 0;
        }
        self.envelope.output()
    }
}

/// MMC5 / ExROM (Mapper 5): PRG/CHR banking, extended RAM, MMC5
/// nametable mapping, fill mode, and multiplier registers.
///
/// This implementation covers the memory-management behaviours needed by
/// ordinary cartridge execution, plus MMC5 pulse/PCM expansion audio and
/// scanline IRQ detection from the PPU nametable-read pattern.
#[derive(Clone, Serialize, Deserialize)]
pub struct Mmc5 {
    prg_rom: Vec<u8>,
    chr: Vec<u8>,
    chr_is_ram: bool,
    prg_ram: Vec<u8>,
    exram: Vec<u8>,
    nt_ram: Vec<u8>,
    prg_mode: u8,
    chr_mode: u8,
    prg_ram_protect_1: u8,
    prg_ram_protect_2: u8,
    exram_mode: u8,
    nametable_mapping: u8,
    fill_tile: u8,
    fill_attr: u8,
    prg_ram_bank: u8,
    prg_banks: [u8; 4],
    chr_banks: [u16; 12],
    chr_bank_high: u8,
    use_background_chr_regs: bool,
    irq_scanline: u8,
    irq_enabled: bool,
    irq_pending: bool,
    in_frame: bool,
    scanline_counter: u8,
    no_ppu_read_cpu_cycles: u8,
    last_nt_read: u16,
    same_nt_read_count: u8,
    pending_scanline_detect: bool,
    multiplicand: u8,
    multiplier: u8,
    audio_pulses: [Mmc5Pulse; 2],
    audio_odd_cycle: bool,
    audio_frame_divider: u16,
    pcm_mode_read: bool,
    pcm_irq_enabled: bool,
    pcm_irq_pending: bool,
    pcm_output: u8,
}

impl Mmc5 {
    /// Construct MMC5 from parsed iNES payloads.
    #[must_use]
    pub fn new(prg_rom: Vec<u8>, chr_data: Vec<u8>) -> Self {
        let chr_is_ram = chr_data.is_empty();
        let chr = if chr_is_ram {
            vec![0; 1024 * 1024]
        } else {
            chr_data
        };
        Self {
            prg_rom,
            chr,
            chr_is_ram,
            prg_ram: vec![0; 128 * 1024],
            exram: vec![0; 1024],
            nt_ram: vec![0; 2048],
            prg_mode: 3,
            chr_mode: 3,
            prg_ram_protect_1: 0,
            prg_ram_protect_2: 0,
            exram_mode: 0,
            nametable_mapping: 0,
            fill_tile: 0,
            fill_attr: 0,
            prg_ram_bank: 0,
            prg_banks: [0, 1, 2, 0xFF],
            chr_banks: [0; 12],
            chr_bank_high: 0,
            use_background_chr_regs: false,
            irq_scanline: 0,
            irq_enabled: false,
            irq_pending: false,
            in_frame: false,
            scanline_counter: 0,
            no_ppu_read_cpu_cycles: 0,
            last_nt_read: 0xFFFF,
            same_nt_read_count: 0,
            pending_scanline_detect: false,
            multiplicand: 0xFF,
            multiplier: 0xFF,
            audio_pulses: [Mmc5Pulse::new(), Mmc5Pulse::new()],
            audio_odd_cycle: false,
            audio_frame_divider: 0,
            pcm_mode_read: true,
            pcm_irq_enabled: false,
            pcm_irq_pending: false,
            pcm_output: 0xFF,
        }
    }

    fn prg_8k_count(&self) -> usize {
        (self.prg_rom.len() / 8192).max(1)
    }

    fn prg_ram_8k_count(&self) -> usize {
        (self.prg_ram.len() / 8192).max(1)
    }

    fn prg_ram_writable(&self) -> bool {
        self.prg_ram_protect_1 == 0x02 && self.prg_ram_protect_2 == 0x01
    }

    fn read_prg_rom_8k(&self, bank: usize, offset: usize) -> u8 {
        let bank = bank % self.prg_8k_count();
        self.prg_rom[bank * 8192 + offset]
    }

    fn read_prg_ram_8k(&self, bank: usize, offset: usize) -> u8 {
        let bank = bank % self.prg_ram_8k_count();
        self.prg_ram[bank * 8192 + offset]
    }

    fn write_prg_ram_8k(&mut self, bank: usize, offset: usize, value: u8) {
        if self.prg_ram_writable() {
            let bank = bank % self.prg_ram_8k_count();
            self.prg_ram[bank * 8192 + offset] = value;
        }
    }

    fn read_prg_reg_8k(&self, reg: u8, offset: usize, force_rom: bool) -> u8 {
        if force_rom || reg & 0x80 != 0 {
            self.read_prg_rom_8k(usize::from(reg & 0x7F), offset)
        } else {
            self.read_prg_ram_8k(usize::from(reg & 0x7F), offset)
        }
    }

    fn write_prg_reg_8k(&mut self, reg: u8, offset: usize, value: u8) {
        if reg & 0x80 == 0 {
            self.write_prg_ram_8k(usize::from(reg & 0x7F), offset, value);
        }
    }

    fn prg_reg_for_addr(&self, addr: u16) -> (u8, usize, bool) {
        let offset = usize::from(addr & 0x1FFF);
        match self.prg_mode & 0x03 {
            0 => (
                (self.prg_banks[3] & 0xFC).wrapping_add(((addr - 0x8000) / 0x2000) as u8),
                offset,
                true,
            ),
            1 => {
                if addr < 0xC000 {
                    let bank = (self.prg_banks[1] & 0xFE).wrapping_add(((addr >> 13) & 1) as u8);
                    (bank, offset, false)
                } else {
                    let bank = (self.prg_banks[3] & 0xFE).wrapping_add(((addr >> 13) & 1) as u8);
                    (bank, offset, true)
                }
            }
            2 => {
                if addr < 0xC000 {
                    let bank = (self.prg_banks[1] & 0xFE).wrapping_add(((addr >> 13) & 1) as u8);
                    (bank, offset, false)
                } else if addr < 0xE000 {
                    (self.prg_banks[2], offset, false)
                } else {
                    (self.prg_banks[3], offset, true)
                }
            }
            3 => match addr {
                0x8000..=0x9FFF => (self.prg_banks[0], offset, false),
                0xA000..=0xBFFF => (self.prg_banks[1], offset, false),
                0xC000..=0xDFFF => (self.prg_banks[2], offset, false),
                _ => (self.prg_banks[3], offset, true),
            },
            _ => unreachable!(),
        }
    }

    fn chr_reg_for_addr(&self, addr: u16) -> u16 {
        let addr = addr & 0x1FFF;
        let use_bg = self.use_background_chr_regs;
        match self.chr_mode & 0x03 {
            0 => self.chr_banks[if use_bg { 11 } else { 7 }] & !0x07,
            1 => {
                if addr < 0x1000 {
                    self.chr_banks[if use_bg { 11 } else { 3 }] & !0x03
                } else {
                    self.chr_banks[if use_bg { 11 } else { 7 }] & !0x03
                }
            }
            2 => {
                let slot = usize::from(addr / 0x0800);
                if use_bg {
                    self.chr_banks[9 + (slot & 1) * 2] & !0x01
                } else {
                    self.chr_banks[[1, 3, 5, 7][slot]] & !0x01
                }
            }
            3 if use_bg => self.chr_banks[8 + usize::from((addr & 0x0FFF) / 0x0400)],
            3 => self.chr_banks[usize::from(addr / 0x0400)],
            _ => unreachable!(),
        }
    }

    fn chr_index(&self, addr: u16) -> usize {
        let unit_size = match self.chr_mode & 0x03 {
            0 => 8192,
            1 => 4096,
            2 => 2048,
            3 => 1024,
            _ => unreachable!(),
        };
        let offset = usize::from(addr) & (unit_size - 1);
        let bank_count = (self.chr.len() / unit_size).max(1);
        let bank = usize::from(self.chr_reg_for_addr(addr)) % bank_count;
        bank * unit_size + offset
    }

    fn nt_page_kind(&self, addr: u16) -> u8 {
        let page = ((addr - 0x2000) & 0x0FFF) / 0x0400;
        (self.nametable_mapping >> (page * 2)) & 0x03
    }

    fn multiplier_product(&self) -> u16 {
        u16::from(self.multiplicand) * u16::from(self.multiplier)
    }

    fn write_pcm(&mut self, value: u8) {
        if value == 0 {
            self.pcm_irq_pending = true;
        } else {
            self.pcm_irq_pending = false;
            self.pcm_output = value;
        }
    }

    fn read_with_side_effect(&mut self, addr: u16) -> u8 {
        let value = self.cpu_read(addr);
        match addr {
            0x5010 => {
                self.pcm_irq_pending = false;
            }
            0x5204 => {
                self.irq_pending = false;
            }
            0x8000..=0xBFFF if self.pcm_mode_read => self.write_pcm(value),
            _ => {}
        }
        value
    }

    fn detect_scanline(&mut self) {
        self.no_ppu_read_cpu_cycles = 0;
        if !self.in_frame {
            self.in_frame = true;
            self.scanline_counter = 0;
            self.irq_pending = false;
            return;
        }

        self.scanline_counter = self.scanline_counter.wrapping_add(1);
        if self.irq_scanline != 0 && self.scanline_counter == self.irq_scanline {
            self.irq_pending = true;
        }
    }

    fn clock_audio(&mut self) {
        if self.audio_odd_cycle {
            self.audio_pulses[0].clock_timer();
            self.audio_pulses[1].clock_timer();
        }
        self.audio_odd_cycle = !self.audio_odd_cycle;

        self.audio_frame_divider += 1;
        if self.audio_frame_divider >= 7457 {
            self.audio_frame_divider = 0;
            self.audio_pulses[0].clock_envelope_and_length();
            self.audio_pulses[1].clock_envelope_and_length();
        }
    }

    fn expansion_audio_level(&self) -> f32 {
        let pulse_sum =
            f32::from(self.audio_pulses[0].output()) + f32::from(self.audio_pulses[1].output());
        let pulse = if pulse_sum > 0.0 {
            95.88 / (8128.0 / pulse_sum + 100.0)
        } else {
            0.0
        };
        let pcm = (f32::from(self.pcm_output) / 255.0) * 0.08;
        pulse + pcm
    }
}

impl Mapper for Mmc5 {
    fn cpu_read(&self, addr: u16) -> u8 {
        match addr {
            0x5010 => {
                (u8::from(self.pcm_irq_pending && self.pcm_irq_enabled) << 7)
                    | u8::from(self.pcm_mode_read)
            }
            0x5015 => {
                u8::from(self.audio_pulses[0].length_counter > 0)
                    | (u8::from(self.audio_pulses[1].length_counter > 0) << 1)
            }
            0x5204 => (u8::from(self.irq_pending) << 7) | (u8::from(self.in_frame) << 6),
            0x5205 => self.multiplier_product() as u8,
            0x5206 => (self.multiplier_product() >> 8) as u8,
            0x5C00..=0x5FFF => self.exram[usize::from(addr - 0x5C00)],
            0x6000..=0x7FFF => {
                let bank = usize::from(self.prg_ram_bank & 0x7F);
                self.read_prg_ram_8k(bank, usize::from(addr - 0x6000))
            }
            0x8000..=0xFFFF => {
                let (reg, offset, force_rom) = self.prg_reg_for_addr(addr);
                self.read_prg_reg_8k(reg, offset, force_rom)
            }
            _ => 0,
        }
    }

    fn cpu_read_side_effect(&mut self, addr: u16) -> u8 {
        self.read_with_side_effect(addr)
    }

    fn cpu_write(&mut self, addr: u16, value: u8) {
        match addr {
            0x5000 => self.audio_pulses[0].write_control(value),
            0x5002 => self.audio_pulses[0].write_timer_low(value),
            0x5003 => self.audio_pulses[0].write_timer_high(value),
            0x5004 => self.audio_pulses[1].write_control(value),
            0x5006 => self.audio_pulses[1].write_timer_low(value),
            0x5007 => self.audio_pulses[1].write_timer_high(value),
            0x5010 => {
                self.pcm_irq_enabled = value & 0x80 != 0;
                self.pcm_mode_read = value & 1 != 0;
            }
            0x5011 if !self.pcm_mode_read => {
                self.write_pcm(value);
            }
            0x5015 => {
                self.audio_pulses[0].set_enabled(value & 0x01 != 0);
                self.audio_pulses[1].set_enabled(value & 0x02 != 0);
            }
            0x5100 => self.prg_mode = value & 0x03,
            0x5101 => self.chr_mode = value & 0x03,
            0x5102 => self.prg_ram_protect_1 = value & 0x03,
            0x5103 => self.prg_ram_protect_2 = value & 0x03,
            0x5104 => self.exram_mode = value & 0x03,
            0x5105 => self.nametable_mapping = value,
            0x5106 => self.fill_tile = value,
            0x5107 => self.fill_attr = value & 0x03,
            0x5113 => self.prg_ram_bank = value & 0x7F,
            0x5114..=0x5117 => self.prg_banks[usize::from(addr - 0x5114)] = value,
            0x5120..=0x5127 => {
                self.use_background_chr_regs = false;
                self.chr_banks[usize::from(addr - 0x5120)] =
                    (u16::from(self.chr_bank_high) << 8) | u16::from(value);
            }
            0x5128..=0x512B => {
                self.use_background_chr_regs = true;
                self.chr_banks[usize::from(addr - 0x5120)] =
                    (u16::from(self.chr_bank_high) << 8) | u16::from(value);
            }
            0x5130 => self.chr_bank_high = value & 0x03,
            0x5203 => self.irq_scanline = value,
            0x5204 => self.irq_enabled = value & 0x80 != 0,
            0x5205 => self.multiplicand = value,
            0x5206 => self.multiplier = value,
            0x5C00..=0x5FFF => self.exram[usize::from(addr - 0x5C00)] = value,
            0x6000..=0x7FFF => {
                let bank = usize::from(self.prg_ram_bank & 0x7F);
                self.write_prg_ram_8k(bank, usize::from(addr - 0x6000), value);
            }
            0x8000..=0xFFFF => {
                let (reg, offset, force_rom) = self.prg_reg_for_addr(addr);
                if !force_rom {
                    self.write_prg_reg_8k(reg, offset, value);
                }
            }
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
        Mirroring::Vertical
    }

    fn irq_pending(&self) -> bool {
        (self.irq_enabled && self.irq_pending) || (self.pcm_irq_enabled && self.pcm_irq_pending)
    }

    fn notify_ppu_read(&mut self, addr: u16, rendering: bool) {
        self.no_ppu_read_cpu_cycles = 0;
        if !rendering {
            return;
        }

        if self.pending_scanline_detect {
            self.pending_scanline_detect = false;
            self.detect_scanline();
        }

        if (0x2000..=0x2FFF).contains(&addr) {
            if addr == self.last_nt_read {
                self.same_nt_read_count = self.same_nt_read_count.saturating_add(1);
            } else {
                self.last_nt_read = addr;
                self.same_nt_read_count = 1;
            }

            if self.same_nt_read_count >= 3 {
                self.pending_scanline_detect = true;
                self.same_nt_read_count = 0;
            }
        } else {
            self.same_nt_read_count = 0;
            self.last_nt_read = 0xFFFF;
        }
    }

    fn cpu_tick(&mut self) {
        self.clock_audio();
        self.no_ppu_read_cpu_cycles = self.no_ppu_read_cpu_cycles.saturating_add(1);
        if self.no_ppu_read_cpu_cycles >= 3 {
            self.in_frame = false;
            self.scanline_counter = 0;
            self.irq_pending = false;
            self.pending_scanline_detect = false;
            self.same_nt_read_count = 0;
            self.last_nt_read = 0xFFFF;
        }
    }

    fn expansion_audio_sample(&self) -> f32 {
        self.expansion_audio_level()
    }

    fn nametable_read(&mut self, addr: u16) -> Option<u8> {
        let offset = usize::from((addr - 0x2000) & 0x03FF);
        let value = match self.nt_page_kind(addr) {
            0 => self.nt_ram[offset],
            1 => self.nt_ram[1024 + offset],
            2 => self.exram[offset],
            3 if offset < 0x03C0 => self.fill_tile,
            3 => self.fill_attr * 0x55,
            _ => unreachable!(),
        };
        Some(value)
    }

    fn nametable_write(&mut self, addr: u16, value: u8) -> bool {
        let offset = usize::from((addr - 0x2000) & 0x03FF);
        match self.nt_page_kind(addr) {
            0 => self.nt_ram[offset] = value,
            1 => self.nt_ram[1024 + offset] = value,
            2 => self.exram[offset] = value,
            3 => {}
            _ => unreachable!(),
        }
        true
    }

    fn save_ram(&self) -> &[u8] {
        &self.prg_ram
    }

    fn restore_save_ram(&mut self, bytes: &[u8]) {
        let n = bytes.len().min(self.prg_ram.len());
        self.prg_ram[..n].copy_from_slice(&bytes[..n]);
    }

    fn snapshot(&self) -> MapperSnapshot {
        MapperSnapshot::Mmc5(self.clone())
    }
}
