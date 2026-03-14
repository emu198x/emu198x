//! Sega Master System / Game Gear emulator.
//!
//! - **CPU:** Z80A @ 3.579545 MHz
//! - **Video:** Sega VDP 315-5124/5246 (16 KB VRAM)
//! - **Audio:** SN76489A
//! - **RAM:** 8 KB at $C000-$DFFF (mirrored to $E000-$FFFF)
//! - **ROM:** No BIOS required (optional); cartridge at $0000-$BFFF
//!
//! The Sega mapper at $FFFC-$FFFF banks 16 KB ROM pages into three slots.
//! Page 0 ($0000-$3FFF) has the first 1 KB always visible for interrupt
//! vectors.

#![allow(clippy::cast_possible_truncation)]

use emu_core::{AudioFrame, Bus, Cpu, Machine, ReadResult};
use sega_vdp::{SegaVdp, VdpRegion, VdpVariant};
use ti_sn76489::Sn76489;
use zilog_z80::Z80;

/// System variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SmsVariant {
    /// Sega Master System (NTSC).
    SmsNtsc,
    /// Sega Master System (PAL).
    SmsPal,
    /// Sega Game Gear.
    GameGear,
}

/// SMS / Game Gear bus.
pub struct SmsBus {
    /// Cartridge ROM.
    pub cart_rom: Vec<u8>,
    /// System RAM (8 KB).
    pub ram: [u8; 8192],
    /// Sega VDP.
    pub vdp: SegaVdp,
    /// SN76489 PSG.
    pub psg: Sn76489,
    /// Sega mapper bank registers.
    mapper_regs: [u8; 4], // $FFFC-$FFFF
    /// Controller port 1 (active-low: bit 0=up, 1=down, 2=left, 3=right, 4=B1, 5=B2).
    pub port_dc: u8,
    /// Controller port 2 / misc.
    pub port_dd: u8,
    /// Game Gear START button (active-low bit 7 of port $00).
    pub gg_start: u8,
    /// Pause button pressed (NMI).
    pub pause_pressed: bool,
    /// Variant.
    variant: SmsVariant,
}

impl SmsBus {
    #[must_use]
    pub fn new(cart_rom: Vec<u8>, variant: SmsVariant) -> Self {
        let (vdp_region, vdp_variant, is_gg) = match variant {
            SmsVariant::SmsNtsc => (VdpRegion::Ntsc, VdpVariant::Sms2, false),
            SmsVariant::SmsPal => (VdpRegion::Pal, VdpVariant::Sms2, false),
            SmsVariant::GameGear => (VdpRegion::Ntsc, VdpVariant::Sms2, true),
        };
        let vdp = if is_gg {
            SegaVdp::new_game_gear()
        } else {
            SegaVdp::new(vdp_region, vdp_variant)
        };
        Self {
            cart_rom,
            ram: [0; 8192],
            vdp,
            psg: Sn76489::new(3_579_545),
            mapper_regs: [0, 0, 1, 2], // Default: banks 0, 1, 2
            port_dc: 0xFF,
            port_dd: 0xFF,
            gg_start: 0xFF,
            pause_pressed: false,
            variant,
        }
    }

    fn read_rom(&self, bank: u8, offset: usize) -> u8 {
        let addr = bank as usize * 16384 + offset;
        self.cart_rom.get(addr).copied().unwrap_or(0xFF)
    }
}

impl Bus for SmsBus {
    fn read(&mut self, addr: u32) -> ReadResult {
        let addr = addr as u16;
        let data = match addr {
            // Slot 0: first 1KB always visible, rest banked
            0x0000..=0x03FF => {
                self.cart_rom.get(addr as usize).copied().unwrap_or(0xFF)
            }
            0x0400..=0x3FFF => {
                self.read_rom(self.mapper_regs[1], (addr & 0x3FFF) as usize)
            }
            // Slot 1: banked
            0x4000..=0x7FFF => {
                self.read_rom(self.mapper_regs[2], (addr & 0x3FFF) as usize)
            }
            // Slot 2: banked (or cart RAM)
            0x8000..=0xBFFF => {
                if self.mapper_regs[0] & 0x08 != 0 {
                    // Cart RAM enabled — not implemented, return RAM
                    0xFF
                } else {
                    self.read_rom(self.mapper_regs[3], (addr & 0x3FFF) as usize)
                }
            }
            // System RAM: 8KB mirrored
            0xC000..=0xFFFF => {
                self.ram[(addr & 0x1FFF) as usize]
            }
        };
        ReadResult::new(data)
    }

    fn write(&mut self, addr: u32, value: u8) -> u8 {
        let addr = addr as u16;
        match addr {
            0xC000..=0xFFFF => {
                // Write to RAM
                self.ram[(addr & 0x1FFF) as usize] = value;
                // Mapper registers overlay $FFFC-$FFFF
                match addr {
                    0xFFFC => self.mapper_regs[0] = value,
                    0xFFFD => self.mapper_regs[1] = value,
                    0xFFFE => self.mapper_regs[2] = value,
                    0xFFFF => self.mapper_regs[3] = value,
                    _ => {}
                }
            }
            _ => {} // ROM space — cart mappers could intercept here
        }
        0
    }

    fn io_read(&mut self, port: u32) -> ReadResult {
        let data = match port as u8 {
            // Game Gear ports
            0x00 if self.variant == SmsVariant::GameGear => self.gg_start,
            // V counter
            0x40..=0x7F if port as u8 & 1 == 0 => self.vdp.read_v_counter(),
            // H counter
            0x41..=0x7F if port as u8 & 1 == 1 => self.vdp.read_h_counter(),
            // VDP data
            0x80..=0xBF if port as u8 & 1 == 0 => self.vdp.read_data(),
            // VDP status
            0x81..=0xBF if port as u8 & 1 == 1 => self.vdp.read_status(),
            // Controller port 1
            0xC0..=0xFF if port as u8 & 1 == 0 => self.port_dc,
            // Controller port 2 / misc
            0xC1..=0xFF if port as u8 & 1 == 1 => self.port_dd,
            _ => 0xFF,
        };
        ReadResult::new(data)
    }

    fn io_write(&mut self, port: u32, value: u8) -> u8 {
        match port as u8 {
            // GG stereo
            0x06 if self.variant == SmsVariant::GameGear => {
                self.psg.write_stereo(value);
            }
            // PSG
            0x40..=0x7F => self.psg.write(value),
            // VDP data
            p @ 0x80..=0xBF if p & 1 == 0 => self.vdp.write_data(value),
            // VDP control
            p @ 0x80..=0xBF if p & 1 == 1 => self.vdp.write_control(value),
            _ => {}
        }
        0
    }
}

/// SMS / Game Gear system.
pub struct Sms {
    cpu: Z80,
    bus: SmsBus,
    master_clock: u64,
    ticks_per_frame: u64,
    frame_count: u64,
    variant: SmsVariant,
}

impl Sms {
    #[must_use]
    pub fn new(cart_rom: Vec<u8>, variant: SmsVariant) -> Self {
        let ticks_per_frame = match variant {
            SmsVariant::SmsNtsc | SmsVariant::GameGear => 342 * 262 * 3,
            SmsVariant::SmsPal => 342 * 313 * 3,
        };
        let bus = SmsBus::new(cart_rom, variant);
        let cpu = Z80::new();

        Self { cpu, bus, master_clock: 0, ticks_per_frame, frame_count: 0, variant }
    }

    pub fn run_frame(&mut self) {
        let target = self.master_clock + self.ticks_per_frame;
        while self.master_clock < target {
            self.cpu.tick(&mut self.bus);

            for _ in 0..3 {
                self.bus.vdp.tick_scanline(); // Should be tick() for dot accuracy
            }
            self.bus.psg.tick();

            if self.bus.vdp.interrupt {
                self.cpu.interrupt();
            }
            if self.bus.pause_pressed {
                self.cpu.nmi();
                self.bus.pause_pressed = false;
            }
            self.master_clock += 1;
        }
        self.frame_count += 1;
    }

    #[must_use] pub fn framebuffer(&self) -> &[u32] { self.bus.vdp.framebuffer() }
    pub fn take_audio_buffer(&mut self) -> Vec<f32> { self.bus.psg.take_buffer() }
    #[must_use] pub fn frame_count(&self) -> u64 { self.frame_count }
    #[must_use] pub fn cpu(&self) -> &Z80 { &self.cpu }
    pub fn cpu_mut(&mut self) -> &mut Z80 { &mut self.cpu }
    #[must_use] pub fn variant(&self) -> SmsVariant { self.variant }

    /// Set controller port $DC value (active-low).
    pub fn set_port_dc(&mut self, value: u8) { self.bus.port_dc = value; }

    /// Set controller port $DD value (active-low).
    pub fn set_port_dd(&mut self, value: u8) { self.bus.port_dd = value; }

    /// Trigger a pause button press (NMI).
    pub fn press_pause(&mut self) { self.bus.pause_pressed = true; }
}

impl Machine for Sms {
    fn run_frame(&mut self) {
        self.run_frame();
    }

    fn framebuffer(&self) -> &[u32] {
        self.framebuffer()
    }

    fn framebuffer_width(&self) -> u32 {
        sega_vdp::FB_WIDTH
    }

    fn framebuffer_height(&self) -> u32 {
        sega_vdp::FB_HEIGHT
    }

    fn take_audio_buffer(&mut self) -> Vec<AudioFrame> {
        self.take_audio_buffer().into_iter().map(|s| [s, s]).collect()
    }

    fn frame_count(&self) -> u64 {
        self.frame_count()
    }

    fn reset(&mut self) {
        self.cpu_mut().reset();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn minimal_rom() -> Vec<u8> {
        let mut rom = vec![0u8; 32768];
        rom[0] = 0xF3; // DI
        rom[1] = 0x18; // JR
        rom[2] = 0xFE; // -2
        rom
    }

    #[test]
    fn boots_and_runs_frame() {
        let mut sms = Sms::new(minimal_rom(), SmsVariant::SmsNtsc);
        sms.run_frame();
        assert_eq!(sms.frame_count(), 1);
    }

    #[test]
    fn ram_read_write() {
        let mut bus = SmsBus::new(minimal_rom(), SmsVariant::SmsNtsc);
        bus.write(0xC000, 0xAB);
        assert_eq!(bus.read(0xC000).data, 0xAB);
        assert_eq!(bus.read(0xE000).data, 0xAB); // Mirror
    }

    #[test]
    fn mapper_bank_switching() {
        let mut rom = vec![0u8; 64 * 1024];
        rom[0] = 0xF3; // DI at $0000
        rom[16384] = 0xAA; // Start of bank 1
        rom[32768] = 0xBB; // Start of bank 2
        rom[49152] = 0xCC; // Start of bank 3

        let mut bus = SmsBus::new(rom, SmsVariant::SmsNtsc);
        // Default: slot 1 = bank 1
        assert_eq!(bus.read(0x4000).data, 0xAA);

        // Switch slot 1 to bank 3
        bus.write(0xFFFE, 3);
        assert_eq!(bus.read(0x4000).data, 0xCC);
    }

    #[test]
    fn game_gear_variant() {
        let sms = Sms::new(minimal_rom(), SmsVariant::GameGear);
        assert_eq!(sms.variant(), SmsVariant::GameGear);
    }

    #[test]
    fn vdp_accessible() {
        let mut bus = SmsBus::new(minimal_rom(), SmsVariant::SmsNtsc);
        bus.io_write(0xBF, 0x40);
        bus.io_write(0xBF, 0x81);
        let _status = bus.io_read(0xBF);
    }
}
