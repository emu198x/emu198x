//! Sega SG-1000 / SC-3000 emulator.
//!
//! The SG-1000 is Sega's first home console (1983). The SC-3000 is its
//! computer variant with a keyboard. Both share identical hardware:
//!
//! - **CPU:** Z80A @ 3.579545 MHz (NTSC) / 3.546893 MHz (PAL)
//! - **Video:** TMS9918A (16 KB VRAM)
//! - **Audio:** SN76489A
//! - **RAM:** 1 KB (SG-1000) or 2 KB (SC-3000)
//! - **Cartridge:** up to 48 KB at $0000-$BFFF (no BIOS)

#![allow(clippy::cast_possible_truncation)]

use emu_core::{AudioFrame, Bus, Cpu, Machine, ReadResult};
use ti_sn76489::Sn76489;
use ti_tms9918::{Tms9918, VdpRegion};
use zilog_z80::Z80;

/// The TMS9918 runs at 3 dots for every 2 Z80 T-states.
const VDP_DOT_PHASE_NUMERATOR: u8 = 3;
const VDP_DOT_PHASE_DENOMINATOR: u8 = 2;
/// 342 VDP dots correspond to 228 Z80 T-states per scanline.
const CPU_TSTATES_PER_SCANLINE: u64 = 228;
const NTSC_SCANLINES_PER_FRAME: u64 = 262;
const PAL_SCANLINES_PER_FRAME: u64 = 313;

/// Z80 T-states per video frame.
const NTSC_TICKS_PER_FRAME: u64 = CPU_TSTATES_PER_SCANLINE * NTSC_SCANLINES_PER_FRAME;
const PAL_TICKS_PER_FRAME: u64 = CPU_TSTATES_PER_SCANLINE * PAL_SCANLINES_PER_FRAME;
const NTSC_PSG_CLOCK_HZ: u32 = 3_579_545;
const PAL_PSG_CLOCK_HZ: u32 = 3_546_893;

/// SG-1000 system region.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sg1000Region {
    Ntsc,
    Pal,
}

/// Controller button state.
#[derive(Debug, Default, Clone, Copy)]
pub struct ControllerState {
    pub up: bool,
    pub down: bool,
    pub left: bool,
    pub right: bool,
    pub button1: bool,
    pub button2: bool,
}

impl ControllerState {
    /// Read as active-low byte (1 = not pressed).
    fn read_port(&self) -> u8 {
        let mut val = 0xFF;
        if self.up {
            val &= !0x01;
        }
        if self.down {
            val &= !0x02;
        }
        if self.left {
            val &= !0x04;
        }
        if self.right {
            val &= !0x08;
        }
        if self.button1 {
            val &= !0x10;
        }
        if self.button2 {
            val &= !0x20;
        }
        val
    }
}

/// SG-1000 bus: Z80 address and I/O routing.
pub struct Sg1000Bus {
    /// Cartridge ROM.
    pub cart_rom: Vec<u8>,
    /// System RAM (1 KB, mirrored through $C000-$FFFF).
    pub ram: [u8; 1024],
    /// TMS9918A VDP.
    pub vdp: Tms9918,
    /// SN76489A PSG.
    pub psg: Sn76489,
    /// Controller 1.
    pub controller1: ControllerState,
    /// Controller 2.
    pub controller2: ControllerState,
    /// Pause button pressed (triggers NMI).
    pub pause_pressed: bool,
}

impl Sg1000Bus {
    /// Create a new SG-1000 bus with the given cartridge ROM.
    #[must_use]
    pub fn new(cart_rom: Vec<u8>, region: Sg1000Region) -> Self {
        let vdp_region = match region {
            Sg1000Region::Ntsc => VdpRegion::Ntsc,
            Sg1000Region::Pal => VdpRegion::Pal,
        };
        let psg_clock_hz = match region {
            Sg1000Region::Ntsc => NTSC_PSG_CLOCK_HZ,
            Sg1000Region::Pal => PAL_PSG_CLOCK_HZ,
        };
        Self {
            cart_rom,
            ram: [0; 1024],
            vdp: Tms9918::new(vdp_region),
            psg: Sn76489::new(psg_clock_hz),
            controller1: ControllerState::default(),
            controller2: ControllerState::default(),
            pause_pressed: false,
        }
    }
}

impl Bus for Sg1000Bus {
    fn read(&mut self, addr: u32) -> ReadResult {
        let addr = addr as u16;
        let data = match addr {
            // Cartridge ROM: $0000-$BFFF
            0x0000..=0xBFFF => {
                let idx = addr as usize;
                if idx < self.cart_rom.len() {
                    self.cart_rom[idx]
                } else {
                    0xFF
                }
            }
            // RAM: $C000-$FFFF (1 KB mirrored)
            0xC000..=0xFFFF => self.ram[(addr & 0x03FF) as usize],
        };
        ReadResult { data, wait: 0 }
    }

    fn write(&mut self, addr: u32, data: u8) -> u8 {
        let addr = addr as u16;
        match addr {
            0xC000..=0xFFFF => self.ram[(addr & 0x03FF) as usize] = data,
            _ => {} // ROM writes ignored
        }
        0
    }

    fn io_read(&mut self, port: u32) -> ReadResult {
        let data = match port as u8 {
            // VDP data read (even ports $80-$BF)
            p @ 0x80..=0xBF if p & 1 == 0 => self.vdp.read_data(),
            // VDP status read (odd ports $80-$BF)
            p @ 0x80..=0xBF if p & 1 == 1 => self.vdp.read_status(),
            // Controller 1 (even ports $C0-$FE)
            p @ 0xC0..=0xFF if p & 1 == 0 => self.controller1.read_port(),
            // Controller 2 (odd ports $C1-$FF)
            p @ 0xC0..=0xFF if p & 1 == 1 => self.controller2.read_port(),
            _ => 0xFF,
        };
        ReadResult { data, wait: 0 }
    }

    fn io_write(&mut self, port: u32, data: u8) -> u8 {
        match port as u8 {
            // SN76489 PSG (write-only, $40-$7F)
            0x40..=0x7F => self.psg.write(data),
            // VDP data write (even ports $80-$BF)
            p @ 0x80..=0xBF if p & 1 == 0 => self.vdp.write_data(data),
            // VDP control write (odd ports $80-$BF)
            p @ 0x80..=0xBF if p & 1 == 1 => self.vdp.write_control(data),
            _ => {}
        }
        0
    }
}

/// SG-1000 system.
pub struct Sg1000 {
    cpu: Z80,
    bus: Sg1000Bus,
    master_clock: u64,
    ticks_per_frame: u64,
    vdp_phase: u8,
    frame_count: u64,
}

impl Sg1000 {
    /// Create a new SG-1000 with the given cartridge ROM.
    #[must_use]
    pub fn new(cart_rom: Vec<u8>, region: Sg1000Region) -> Self {
        let ticks_per_frame = match region {
            Sg1000Region::Ntsc => NTSC_TICKS_PER_FRAME,
            Sg1000Region::Pal => PAL_TICKS_PER_FRAME,
        };
        let bus = Sg1000Bus::new(cart_rom, region);
        let cpu = Z80::new();

        // Z80 starts at $0000 after reset (default)

        Self {
            cpu,
            bus,
            master_clock: 0,
            ticks_per_frame,
            vdp_phase: 0,
            frame_count: 0,
        }
    }

    /// Run one complete frame.
    pub fn run_frame(&mut self) {
        let target = self.master_clock + self.ticks_per_frame;

        while self.master_clock < target {
            // One Z80 T-state.
            self.cpu.tick(&mut self.bus);

            // The TMS9918 dot clock runs at 3/2 the Z80 T-state rate.
            self.vdp_phase = self
                .vdp_phase
                .saturating_add(VDP_DOT_PHASE_NUMERATOR);
            while self.vdp_phase >= VDP_DOT_PHASE_DENOMINATOR {
                self.bus.vdp.tick();
                self.vdp_phase -= VDP_DOT_PHASE_DENOMINATOR;
            }

            // PSG input clock matches the Z80 clock on SG-1000 hardware.
            self.bus.psg.tick();

            // VDP interrupt → Z80 INT
            if self.bus.vdp.interrupt {
                self.cpu.interrupt();
            }

            // Pause button → NMI
            if self.bus.pause_pressed {
                self.cpu.nmi();
                self.bus.pause_pressed = false;
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

    /// Controller 1 state.
    pub fn controller1_mut(&mut self) -> &mut ControllerState {
        &mut self.bus.controller1
    }

    /// Controller 2 state.
    pub fn controller2_mut(&mut self) -> &mut ControllerState {
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

impl Machine for Sg1000 {
    fn run_frame(&mut self) {
        self.run_frame();
    }

    fn framebuffer(&self) -> &[u32] {
        self.framebuffer()
    }

    fn framebuffer_width(&self) -> u32 {
        ti_tms9918::FB_WIDTH
    }

    fn framebuffer_height(&self) -> u32 {
        ti_tms9918::FB_HEIGHT
    }

    fn take_audio_buffer(&mut self) -> Vec<AudioFrame> {
        self.take_audio_buffer()
            .into_iter()
            .map(|s| [s, s])
            .collect()
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
    use std::ops::RangeInclusive;

    const NTSC_AUDIO_SAMPLES_PER_FRAME: RangeInclusive<usize> = 799..=801;

    fn minimal_rom() -> Vec<u8> {
        // Minimal ROM: DI; JR -2 (infinite loop at $0000)
        let mut rom = vec![0u8; 8192];
        rom[0] = 0xF3; // DI
        rom[1] = 0x18; // JR
        rom[2] = 0xFE; // -2 (jump to self)
        rom
    }

    #[test]
    fn boots_and_runs_frame() {
        let mut sg = Sg1000::new(minimal_rom(), Sg1000Region::Ntsc);
        sg.run_frame();
        assert_eq!(sg.frame_count(), 1);
        assert_eq!(sg.framebuffer().len(), 256 * 192);
    }

    #[test]
    fn frame_tick_budget_matches_video_timing() {
        let mut ntsc = Sg1000::new(minimal_rom(), Sg1000Region::Ntsc);
        ntsc.run_frame();
        assert_eq!(ntsc.master_clock, NTSC_TICKS_PER_FRAME);

        let mut pal = Sg1000::new(minimal_rom(), Sg1000Region::Pal);
        pal.run_frame();
        assert_eq!(pal.master_clock, PAL_TICKS_PER_FRAME);
    }

    #[test]
    fn frame_produces_expected_audio_sample_count() {
        let mut ntsc = Sg1000::new(minimal_rom(), Sg1000Region::Ntsc);
        ntsc.run_frame();

        let samples = ntsc.take_audio_buffer();
        assert!(
            NTSC_AUDIO_SAMPLES_PER_FRAME.contains(&samples.len()),
            "expected about 800 samples per NTSC frame, got {}",
            samples.len()
        );
    }

    #[test]
    fn ram_read_write() {
        let mut bus = Sg1000Bus::new(minimal_rom(), Sg1000Region::Ntsc);
        bus.write(0xC000, 0xAB);
        assert_eq!(bus.read(0xC000).data, 0xAB);
        // Mirror
        assert_eq!(bus.read(0xC400).data, 0xAB);
    }

    #[test]
    fn controller_default_unpressed() {
        let state = ControllerState::default();
        assert_eq!(state.read_port(), 0xFF);
    }

    #[test]
    fn controller_button_active_low() {
        let mut state = ControllerState::default();
        state.up = true;
        state.button1 = true;
        assert_eq!(state.read_port(), 0xFF & !0x01 & !0x10);
    }

    #[test]
    fn vdp_accessible_via_io() {
        let mut bus = Sg1000Bus::new(minimal_rom(), Sg1000Region::Ntsc);
        // Write VDP register 1 = $40 (enable display)
        bus.io_write(0xBF, 0x40); // value
        bus.io_write(0xBF, 0x81); // register 1
        // Read status should work
        let _status = bus.io_read(0xBF).data;
    }

    #[test]
    fn psg_accessible_via_io() {
        let mut bus = Sg1000Bus::new(minimal_rom(), Sg1000Region::Ntsc);
        // Write to PSG — should not panic
        bus.io_write(0x7F, 0x9F); // Mute channel 0
    }
}
