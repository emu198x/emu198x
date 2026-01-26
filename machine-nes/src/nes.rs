//! NES machine emulation.

use crate::apu::Apu;
use crate::controller::buttons;
use crate::memory::NesMemory;
use crate::ppu::Ppu;
use crate::Cartridge;
use cpu_6502::Mos6502;
use emu_core::{AudioConfig, Bus, Cpu, JoystickState, KeyCode, Machine, VideoConfig};

/// NTSC timing constants.
pub mod ntsc {
    /// CPU cycles per scanline.
    pub const CYCLES_PER_LINE: u32 = 114; // ~113.67, rounded
    /// Visible scanlines.
    pub const VISIBLE_LINES: u32 = 240;
    /// Total scanlines including vblank.
    pub const LINES_PER_FRAME: u32 = 262;
    /// CPU cycles per frame.
    pub const CYCLES_PER_FRAME: u32 = 29781; // 262 * 341 / 3
    /// CPU clock frequency.
    pub const CPU_CLOCK: u32 = 1789773;
    /// Frames per second.
    pub const FPS: f32 = 60.0988;
}

/// PAL timing constants.
pub mod pal {
    /// CPU cycles per scanline.
    pub const CYCLES_PER_LINE: u32 = 107;
    /// Visible scanlines.
    pub const VISIBLE_LINES: u32 = 240;
    /// Total scanlines including vblank.
    pub const LINES_PER_FRAME: u32 = 312;
    /// CPU cycles per frame.
    pub const CYCLES_PER_FRAME: u32 = 33248;
    /// CPU clock frequency.
    pub const CPU_CLOCK: u32 = 1662607;
    /// Frames per second.
    pub const FPS: f32 = 50.0070;
}

/// Sample rate for audio output.
const SAMPLE_RATE: u32 = 44100;

/// NES palette (2C02, sourced from nesdev wiki).
const NES_PALETTE: [(u8, u8, u8); 64] = [
    (84, 84, 84),
    (0, 30, 116),
    (8, 16, 144),
    (48, 0, 136),
    (68, 0, 100),
    (92, 0, 48),
    (84, 4, 0),
    (60, 24, 0),
    (32, 42, 0),
    (8, 58, 0),
    (0, 64, 0),
    (0, 60, 0),
    (0, 50, 60),
    (0, 0, 0),
    (0, 0, 0),
    (0, 0, 0),
    (152, 150, 152),
    (8, 76, 196),
    (48, 50, 236),
    (92, 30, 228),
    (136, 20, 176),
    (160, 20, 100),
    (152, 34, 32),
    (120, 60, 0),
    (84, 90, 0),
    (40, 114, 0),
    (8, 124, 0),
    (0, 118, 40),
    (0, 102, 120),
    (0, 0, 0),
    (0, 0, 0),
    (0, 0, 0),
    (236, 238, 236),
    (76, 154, 236),
    (120, 124, 236),
    (176, 98, 236),
    (228, 84, 236),
    (236, 88, 180),
    (236, 106, 100),
    (212, 136, 32),
    (160, 170, 0),
    (116, 196, 0),
    (76, 208, 32),
    (56, 204, 108),
    (56, 180, 204),
    (60, 60, 60),
    (0, 0, 0),
    (0, 0, 0),
    (236, 238, 236),
    (168, 204, 236),
    (188, 188, 236),
    (212, 178, 236),
    (236, 174, 236),
    (236, 174, 212),
    (236, 180, 176),
    (228, 196, 144),
    (204, 210, 120),
    (180, 222, 120),
    (168, 226, 144),
    (152, 226, 180),
    (160, 214, 228),
    (160, 162, 160),
    (0, 0, 0),
    (0, 0, 0),
];

/// NES timing mode.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TimingMode {
    /// NTSC (60 Hz, 262 scanlines).
    Ntsc,
    /// PAL (50 Hz, 312 scanlines).
    Pal,
}

/// The Nintendo Entertainment System.
pub struct Nes {
    /// 2A03 CPU (6502 without BCD).
    cpu: Mos6502,
    /// PPU (picture processing unit).
    pub ppu: Ppu,
    /// APU (audio processing unit).
    apu: Apu,
    /// Memory/bus subsystem.
    pub memory: NesMemory,
    /// Timing mode.
    timing: TimingMode,
    /// Frame cycle counter.
    frame_cycles: u32,
    /// Total cycles executed.
    total_cycles: u64,
    /// Audio sample accumulator.
    audio_accum: f32,
    /// Audio samples per CPU cycle.
    samples_per_cycle: f32,
    /// Audio buffer for frame.
    audio_buffer: Vec<f32>,
    /// Pending PPU cycles from previous CPU step.
    pending_ppu_cycles: u32,
    /// Delayed NMI (triggers after next instruction).
    delayed_nmi: bool,
}

impl Nes {
    /// Create a new NES with NTSC timing.
    pub fn new() -> Self {
        Self::with_timing(TimingMode::Ntsc)
    }

    /// Create a new NES with specified timing mode.
    pub fn with_timing(timing: TimingMode) -> Self {
        let cpu_clock = match timing {
            TimingMode::Ntsc => ntsc::CPU_CLOCK,
            TimingMode::Pal => pal::CPU_CLOCK,
        };
        Self {
            cpu: Mos6502::new_2a03(), // 2A03: 6502 without decimal mode
            ppu: Ppu::new(),
            apu: Apu::new(),
            memory: NesMemory::new(),
            timing,
            frame_cycles: 0,
            total_cycles: 0,
            audio_accum: 0.0,
            samples_per_cycle: SAMPLE_RATE as f32 / cpu_clock as f32,
            audio_buffer: Vec::with_capacity(1024),
            pending_ppu_cycles: 0,
            delayed_nmi: false,
        }
    }

    /// Get timing mode.
    pub fn timing(&self) -> TimingMode {
        self.timing
    }

    /// Reset the NES.
    fn reset_internal(&mut self) {
        self.cpu.reset(&mut self.memory);
        self.ppu.reset();
        self.apu.reset();
        self.frame_cycles = 0;
        self.audio_buffer.clear();
        self.audio_accum = 0.0;
        self.pending_ppu_cycles = 0;
    }

    /// Run for one frame.
    fn run_frame_internal(&mut self) {
        let cycles_per_frame = match self.timing {
            TimingMode::Ntsc => ntsc::CYCLES_PER_FRAME,
            TimingMode::Pal => pal::CYCLES_PER_FRAME,
        };

        self.frame_cycles = 0;
        self.audio_buffer.clear();

        while self.frame_cycles < cycles_per_frame {
            self.step();
        }
    }

    /// Run a single CPU step.
    pub fn step(&mut self) -> u32 {
        // Run pending PPU cycles from previous step to improve synchronization
        // This helps timing-sensitive tests see PPU state changes sooner
        if self.pending_ppu_cycles > 0 {
            for _ in 0..self.pending_ppu_cycles {
                let (nmi, _pixel) = self.ppu.tick(&mut self.memory);
                if nmi {
                    self.cpu.nmi(&mut self.memory);
                }
                // Keep status synced so CPU sees changes immediately
                self.memory.ppu_status = self.ppu.status;
            }
            self.pending_ppu_cycles = 0;
        }

        // Sync PPU state to memory before CPU step (for reads)
        self.memory.ppu_status = self.ppu.status;
        self.memory.ppu_oam_data = self.ppu.oam[self.ppu.oam_addr as usize];
        self.memory.ppu_data_buffer = self.ppu.data_buffer;
        self.memory.ppu_vram_addr = self.ppu.vram_addr;

        let cycles = self.cpu.step(&mut self.memory);

        // Trigger delayed NMI (from enabling NMI while VBL set)
        // This happens after the current instruction completes
        if self.delayed_nmi {
            self.delayed_nmi = false;
            self.cpu.nmi(&mut self.memory);
        }

        // Process any pending PPU register writes
        for (reg, value) in self.memory.take_ppu_reg_writes() {
            if reg & 0x80 != 0 {
                // Special: register read event
                match reg & 0x7F {
                    2 => {
                        // Status was read - clear vblank and write toggle
                        self.ppu.status &= 0x7F;
                        self.ppu.write_toggle = false;
                        self.ppu.nmi_occurred = false;
                    }
                    7 => {
                        // VRAM data was read - update buffer and increment address
                        let addr = self.ppu.vram_addr & 0x3FFF;
                        if addr >= 0x3F00 {
                            // Palette reads update buffer with underlying nametable data
                            // The palette value was already returned immediately in memory.rs
                            self.ppu.data_buffer = self.memory.ppu_read(addr & 0x2FFF);
                        } else {
                            // Buffer the current location for next read
                            self.ppu.data_buffer = self.memory.ppu_read(addr);
                        }
                        self.ppu.increment_vram_addr();
                        self.memory.ppu_data_buffer = self.ppu.data_buffer;
                    }
                    _ => {}
                }
            } else {
                // Normal register write
                let nmi = self.ppu.write_register(reg as u16, value, &mut self.memory);
                if nmi {
                    // Delay NMI until after the next instruction
                    self.delayed_nmi = true;
                }
            }
        }

        // Process any pending APU writes
        for (addr, value) in self.memory.take_apu_writes() {
            self.apu.write(addr, value);
        }
        // Sync APU status to memory for reads
        self.memory.apu_status = self.apu.read_status();

        // Handle OAM DMA (takes 513/514 cycles, simplified here)
        // DMA starts at current OAM address and wraps around
        if let Some(page) = self.memory.take_oam_dma() {
            let base = (page as u16) << 8;
            let oam_start = self.ppu.oam_addr;
            for i in 0..256u16 {
                let value = self.memory.read((base + i) as u32);
                let oam_index = oam_start.wrapping_add(i as u8);
                self.ppu.oam[oam_index as usize] = value;
            }
            // OAM address is unchanged after DMA
        }

        // Store PPU cycles to run at start of next step
        // This improves timing accuracy for tests that poll PPU status
        self.pending_ppu_cycles += cycles * 3;

        // APU runs at CPU speed
        for _ in 0..cycles {
            let irq = self.apu.tick();
            if irq {
                self.cpu.interrupt(&mut self.memory);
            }

            // Generate audio sample
            self.audio_accum += self.samples_per_cycle;
            if self.audio_accum >= 1.0 {
                self.audio_accum -= 1.0;
                self.audio_buffer.push(self.apu.output());
            }
        }

        self.frame_cycles += cycles;
        self.total_cycles += cycles as u64;

        cycles
    }

    /// Load a cartridge.
    pub fn load_cartridge(&mut self, cartridge: Cartridge) {
        self.memory.load_cartridge(cartridge);
        self.reset_internal();
    }

    /// Set controller state.
    pub fn set_controller(&mut self, controller: u8) {
        self.memory.controller1_state = controller;
    }

    /// Get total cycles executed.
    pub fn total_cycles(&self) -> u64 {
        self.total_cycles
    }
}

impl Default for Nes {
    fn default() -> Self {
        Self::new()
    }
}

impl Machine for Nes {
    fn video_config(&self) -> VideoConfig {
        let fps = match self.timing {
            TimingMode::Ntsc => ntsc::FPS,
            TimingMode::Pal => pal::FPS,
        };
        VideoConfig {
            width: 256,
            height: 240,
            fps,
        }
    }

    fn audio_config(&self) -> AudioConfig {
        let samples_per_frame = match self.timing {
            TimingMode::Ntsc => (SAMPLE_RATE as f32 / ntsc::FPS) as usize,
            TimingMode::Pal => (SAMPLE_RATE as f32 / pal::FPS) as usize,
        };
        AudioConfig {
            sample_rate: SAMPLE_RATE,
            samples_per_frame,
        }
    }

    fn run_frame(&mut self) {
        self.run_frame_internal();
    }

    fn render(&mut self, buffer: &mut [u8]) {
        for (i, &color_idx) in self.ppu.framebuffer.iter().enumerate() {
            let (r, g, b) = NES_PALETTE[(color_idx & 0x3F) as usize];
            let offset = i * 4;
            buffer[offset] = r;
            buffer[offset + 1] = g;
            buffer[offset + 2] = b;
            buffer[offset + 3] = 255;
        }
    }

    fn generate_audio(&mut self, buffer: &mut [f32]) {
        let len = buffer.len().min(self.audio_buffer.len());
        buffer[..len].copy_from_slice(&self.audio_buffer[..len]);
        // Fill remainder with silence if needed
        for sample in buffer[len..].iter_mut() {
            *sample = 0.0;
        }
    }

    fn key_down(&mut self, key: KeyCode) {
        let button = match key {
            KeyCode::KeyZ => buttons::A,
            KeyCode::KeyX => buttons::B,
            KeyCode::Space => buttons::SELECT,
            KeyCode::Enter => buttons::START,
            KeyCode::ArrowUp => buttons::UP,
            KeyCode::ArrowDown => buttons::DOWN,
            KeyCode::ArrowLeft => buttons::LEFT,
            KeyCode::ArrowRight => buttons::RIGHT,
            _ => return,
        };
        self.memory.controller1_state |= button;
    }

    fn key_up(&mut self, key: KeyCode) {
        let button = match key {
            KeyCode::KeyZ => buttons::A,
            KeyCode::KeyX => buttons::B,
            KeyCode::Space => buttons::SELECT,
            KeyCode::Enter => buttons::START,
            KeyCode::ArrowUp => buttons::UP,
            KeyCode::ArrowDown => buttons::DOWN,
            KeyCode::ArrowLeft => buttons::LEFT,
            KeyCode::ArrowRight => buttons::RIGHT,
            _ => return,
        };
        self.memory.controller1_state &= !button;
    }

    fn set_joystick(&mut self, _port: u8, state: JoystickState) {
        let mut buttons = 0u8;
        if state.fire {
            buttons |= buttons::A;
        }
        if state.fire2 {
            buttons |= buttons::B;
        }
        if state.up {
            buttons |= buttons::UP;
        }
        if state.down {
            buttons |= buttons::DOWN;
        }
        if state.left {
            buttons |= buttons::LEFT;
        }
        if state.right {
            buttons |= buttons::RIGHT;
        }
        self.memory.controller1_state = buttons;
    }

    fn reset(&mut self) {
        self.reset_internal();
    }

    fn load_file(&mut self, path: &str, data: &[u8]) -> Result<(), String> {
        let lower = path.to_lowercase();
        if lower.ends_with(".nes") {
            let cartridge = Cartridge::from_ines(data)?;
            self.load_cartridge(cartridge);
            Ok(())
        } else {
            Err(format!("Unknown file format: {}", path))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_nes() {
        let nes = Nes::new();
        assert_eq!(nes.timing(), TimingMode::Ntsc);
    }
}
