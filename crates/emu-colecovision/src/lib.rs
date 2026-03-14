//! ColecoVision emulator.
//!
//! The ColecoVision (1982) is a Z80-based console with an 8 KB BIOS ROM,
//! TMS9918A video, and SN76489A audio. Controllers feature a joystick,
//! two fire buttons, and a 12-key numeric keypad — accessed through a
//! multiplexed I/O scheme that alternates between keypad and joystick mode.
//!
//! - **CPU:** Z80A @ 3.579545 MHz
//! - **Video:** TMS9918A (16 KB VRAM)
//! - **Audio:** SN76489AN
//! - **RAM:** 1 KB at $6000-$63FF (mirrored through $7FFF)
//! - **BIOS:** 8 KB ROM at $0000-$1FFF (required)
//! - **Cartridge:** up to 32 KB at $8000-$FFFF

#![allow(clippy::cast_possible_truncation)]

use emu_core::{Bus, Cpu, ReadResult};
use ti_sn76489::Sn76489;
use ti_tms9918::{Tms9918, VdpRegion};
use zilog_z80::Z80;

/// VDP dots per CPU cycle (TMS9918 runs at 3× CPU clock).
const VDP_DOTS_PER_CPU: u64 = 3;
/// Crystal ticks per frame (NTSC: 342 dots × 262 lines × 3 CPU cycles per dot).
const NTSC_TICKS_PER_FRAME: u64 = 342 * 262 * 3;

/// ColecoVision region.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CvRegion {
    Ntsc,
    Pal,
}

/// Keypad key identifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeypadKey {
    K0, K1, K2, K3, K4, K5, K6, K7, K8, K9, Star, Hash,
}

impl KeypadKey {
    /// Active-low encoding for keypad bits 3-0.
    fn encode(self) -> u8 {
        match self {
            Self::K0 => 0x0A,
            Self::K1 => 0x0D,
            Self::K2 => 0x07,
            Self::K3 => 0x0C,
            Self::K4 => 0x02,
            Self::K5 => 0x03,
            Self::K6 => 0x0E,
            Self::K7 => 0x05,
            Self::K8 => 0x01,
            Self::K9 => 0x0B,
            Self::Star => 0x09,
            Self::Hash => 0x06,
        }
    }
}

/// Controller state for one player.
#[derive(Debug, Default, Clone)]
pub struct CvController {
    pub up: bool,
    pub down: bool,
    pub left: bool,
    pub right: bool,
    pub left_button: bool,
    pub right_button: bool,
    pub keypad: Option<KeypadKey>,
}

impl CvController {
    /// Read in joystick mode (active-low).
    fn read_joystick(&self) -> u8 {
        let mut val = 0xFF;
        if self.left { val &= !0x01; }    // bit 0 = left (active-low, directly from port docs: 0=L, 1=D, 2=R, 3=U in ColecoVision convention)
        if self.down { val &= !0x02; }
        if self.right { val &= !0x04; }
        if self.up { val &= !0x08; }
        if self.left_button { val &= !0x40; }
        val
    }

    /// Read in keypad mode (active-low).
    fn read_keypad(&self) -> u8 {
        let mut val: u8 = 0x70; // bits 6-4 high, bit 7 unused
        if let Some(key) = self.keypad {
            val = (val & 0xF0) | key.encode();
        } else {
            val |= 0x0F; // No key pressed
        }
        if self.right_button { val &= !0x40; }
        val
    }
}

/// ColecoVision bus: Z80 memory and I/O routing.
pub struct CvBus {
    /// BIOS ROM (8 KB).
    pub bios: Vec<u8>,
    /// Cartridge ROM (up to 32 KB).
    pub cart_rom: Vec<u8>,
    /// System RAM (1 KB, mirrored through $6000-$7FFF).
    pub ram: [u8; 1024],
    /// TMS9918A VDP.
    pub vdp: Tms9918,
    /// SN76489AN PSG.
    pub psg: Sn76489,
    /// Controller 1.
    pub controller1: CvController,
    /// Controller 2.
    pub controller2: CvController,
    /// True when controllers are in joystick mode, false for keypad mode.
    joystick_mode: bool,
}

impl CvBus {
    /// Create a new ColecoVision bus.
    #[must_use]
    pub fn new(bios: Vec<u8>, cart_rom: Vec<u8>, region: CvRegion) -> Self {
        let vdp_region = match region {
            CvRegion::Ntsc => VdpRegion::Ntsc,
            CvRegion::Pal => VdpRegion::Pal,
        };
        Self {
            bios,
            cart_rom,
            ram: [0; 1024],
            vdp: Tms9918::new(vdp_region),
            psg: Sn76489::new(3_579_545),
            controller1: CvController::default(),
            controller2: CvController::default(),
            joystick_mode: false,
        }
    }
}

impl Bus for CvBus {
    fn read(&mut self, addr: u32) -> ReadResult {
        let addr = addr as u16;
        let data = match addr {
            // BIOS ROM: $0000-$1FFF
            0x0000..=0x1FFF => {
                let idx = addr as usize;
                if idx < self.bios.len() { self.bios[idx] } else { 0xFF }
            }
            // Expansion: $2000-$5FFF (unmapped)
            0x2000..=0x5FFF => 0xFF,
            // RAM: $6000-$7FFF (1 KB mirrored)
            0x6000..=0x7FFF => self.ram[(addr & 0x03FF) as usize],
            // Cartridge ROM: $8000-$FFFF
            0x8000..=0xFFFF => {
                let idx = (addr - 0x8000) as usize;
                if idx < self.cart_rom.len() { self.cart_rom[idx] } else { 0xFF }
            }
        };
        ReadResult { data, wait: 0 }
    }

    fn write(&mut self, addr: u32, data: u8) -> u8 {
        let addr = addr as u16;
        if (0x6000..=0x7FFF).contains(&addr) {
            self.ram[(addr & 0x03FF) as usize] = data;
        }
        0
    }

    fn io_read(&mut self, port: u32) -> ReadResult {
        let data = match port as u8 {
            // VDP data read (even ports $A0-$BF)
            p @ 0xA0..=0xBF if p & 1 == 0 => self.vdp.read_data(),
            // VDP status read (odd ports $A0-$BF)
            p @ 0xA0..=0xBF if p & 1 == 1 => self.vdp.read_status(),
            // Controller 1 (A1=0 ports in $E0-$FF)
            p @ 0xE0..=0xFF if p & 0x02 == 0 => {
                if self.joystick_mode {
                    self.controller1.read_joystick()
                } else {
                    self.controller1.read_keypad()
                }
            }
            // Controller 2 (A1=1 ports in $E0-$FF)
            p @ 0xE0..=0xFF if p & 0x02 != 0 => {
                if self.joystick_mode {
                    self.controller2.read_joystick()
                } else {
                    self.controller2.read_keypad()
                }
            }
            _ => 0xFF,
        };
        ReadResult { data, wait: 0 }
    }

    fn io_write(&mut self, port: u32, data: u8) -> u8 {
        match port as u8 {
            // Select keypad mode ($80-$9F)
            0x80..=0x9F => self.joystick_mode = false,
            // VDP data write (even ports $A0-$BF)
            p @ 0xA0..=0xBF if p & 1 == 0 => self.vdp.write_data(data),
            // VDP control write (odd ports $A0-$BF)
            p @ 0xA0..=0xBF if p & 1 == 1 => self.vdp.write_control(data),
            // Select joystick mode ($C0-$DF)
            0xC0..=0xDF => self.joystick_mode = true,
            // SN76489 PSG ($E0-$FF)
            0xE0..=0xFF => self.psg.write(data),
            _ => {}
        }
        0
    }
}

/// ColecoVision system.
pub struct ColecoVision {
    cpu: Z80,
    bus: CvBus,
    master_clock: u64,
    ticks_per_frame: u64,
    frame_count: u64,
}

impl ColecoVision {
    /// Create a new ColecoVision with BIOS and cartridge ROM.
    #[must_use]
    pub fn new(bios: Vec<u8>, cart_rom: Vec<u8>, region: CvRegion) -> Self {
        let ticks_per_frame = match region {
            CvRegion::Ntsc => NTSC_TICKS_PER_FRAME,
            CvRegion::Pal => 342 * 313 * 3,
        };
        let bus = CvBus::new(bios, cart_rom, region);
        let mut cpu = Z80::new();
        // Z80 starts at $0000 (BIOS entry point, default)

        Self {
            cpu,
            bus,
            master_clock: 0,
            ticks_per_frame,
            frame_count: 0,
        }
    }

    /// Run one complete frame.
    pub fn run_frame(&mut self) {
        let target = self.master_clock + self.ticks_per_frame;

        while self.master_clock < target {
            self.cpu.tick(&mut self.bus);

            for _ in 0..VDP_DOTS_PER_CPU {
                self.bus.vdp.tick();
            }

            self.bus.psg.tick();

            // VDP interrupt → Z80 INT (directly from TMS9918A INT pin)
            if self.bus.vdp.interrupt {
                self.cpu.interrupt();
            }

            self.master_clock += 1;
        }

        self.frame_count += 1;
    }

    /// The current framebuffer (256×192 ARGB32).
    #[must_use]
    pub fn framebuffer(&self) -> &[u32] {
        self.bus.vdp.framebuffer()
    }

    /// Take audio samples (mono f32, 48 kHz).
    pub fn take_audio_buffer(&mut self) -> Vec<f32> {
        self.bus.psg.take_buffer()
    }

    /// Controller 1.
    pub fn controller1_mut(&mut self) -> &mut CvController {
        &mut self.bus.controller1
    }

    /// Controller 2.
    pub fn controller2_mut(&mut self) -> &mut CvController {
        &mut self.bus.controller2
    }

    /// Frame count.
    #[must_use]
    pub fn frame_count(&self) -> u64 {
        self.frame_count
    }

    /// Access the CPU.
    #[must_use]
    pub fn cpu(&self) -> &Z80 {
        &self.cpu
    }

    /// Access the CPU mutably.
    pub fn cpu_mut(&mut self) -> &mut Z80 {
        &mut self.cpu
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn minimal_bios() -> Vec<u8> {
        // Minimal BIOS: DI; JP $8000 (jump to cartridge)
        let mut rom = vec![0u8; 8192];
        rom[0] = 0xF3; // DI
        rom[1] = 0xC3; // JP
        rom[2] = 0x00; // $8000 low
        rom[3] = 0x80; // $8000 high
        rom
    }

    fn minimal_cart() -> Vec<u8> {
        // Cart header + infinite loop
        let mut rom = vec![0u8; 8192];
        rom[0] = 0x55; // Skip title screen
        rom[1] = 0xAA;
        // Start vector at $800A
        rom[0x0A] = 0x10; // $8010 low
        rom[0x0B] = 0x80; // $8010 high
        // Code at $8010: DI; JR -2
        rom[0x10] = 0xF3; // DI
        rom[0x11] = 0x18; // JR
        rom[0x12] = 0xFE; // -2
        rom
    }

    #[test]
    fn boots_and_runs_frame() {
        let mut cv = ColecoVision::new(minimal_bios(), minimal_cart(), CvRegion::Ntsc);
        cv.run_frame();
        assert_eq!(cv.frame_count(), 1);
    }

    #[test]
    fn ram_read_write() {
        let mut bus = CvBus::new(minimal_bios(), minimal_cart(), CvRegion::Ntsc);
        bus.write(0x6000, 0xCD);
        assert_eq!(bus.read(0x6000).data, 0xCD);
        // Mirror
        assert_eq!(bus.read(0x6400).data, 0xCD);
    }

    #[test]
    fn bios_accessible() {
        let bios = minimal_bios();
        let bus = CvBus::new(bios.clone(), minimal_cart(), CvRegion::Ntsc);
        assert_eq!(bus.bios[0], 0xF3);
    }

    #[test]
    fn cart_accessible() {
        let mut bus = CvBus::new(minimal_bios(), minimal_cart(), CvRegion::Ntsc);
        assert_eq!(bus.read(0x8000).data, 0x55);
        assert_eq!(bus.read(0x8001).data, 0xAA);
    }

    #[test]
    fn controller_joystick_mode() {
        let mut bus = CvBus::new(minimal_bios(), minimal_cart(), CvRegion::Ntsc);
        // Select joystick mode
        bus.io_write(0xC0, 0x00);
        bus.controller1.up = true;
        let val = bus.io_read(0xE0).data;
        assert_eq!(val & 0x08, 0); // Up is active-low bit 3
    }

    #[test]
    fn controller_keypad_mode() {
        let mut bus = CvBus::new(minimal_bios(), minimal_cart(), CvRegion::Ntsc);
        // Select keypad mode
        bus.io_write(0x80, 0x00);
        bus.controller1.keypad = Some(KeypadKey::K5);
        let val = bus.io_read(0xE0).data;
        assert_eq!(val & 0x0F, 0x03); // Key 5 = $03
    }

    #[test]
    fn controller_no_key_pressed() {
        let mut bus = CvBus::new(minimal_bios(), minimal_cart(), CvRegion::Ntsc);
        bus.io_write(0x80, 0x00); // Keypad mode
        let val = bus.io_read(0xE0).data;
        assert_eq!(val & 0x0F, 0x0F); // No key = $0F
    }

    #[test]
    fn vdp_accessible_via_io() {
        let mut bus = CvBus::new(minimal_bios(), minimal_cart(), CvRegion::Ntsc);
        bus.io_write(0xA1, 0x40); // VDP reg value
        bus.io_write(0xA1, 0x81); // Register 1
        let _status = bus.io_read(0xA1).data;
    }
}
