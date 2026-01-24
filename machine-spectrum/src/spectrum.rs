//! Generic Spectrum emulator parameterized by memory model.

use crate::audio;
use crate::input;
use crate::memory::{MemoryModel, T_STATES_PER_FRAME_48K, Ula};
use crate::tape::Tape;
use crate::video;
use cpu_z80::Z80;
use emu_core::{AudioConfig, Bus, Cpu, IoBus, JoystickState, KeyCode, Machine, VideoConfig};
use std::marker::PhantomData;

/// Memory subsystem combining the 64K address space with ULA.
struct Memory<M: MemoryModel> {
    /// The 64K address space.
    data: [u8; 65536],
    /// ULA state.
    ula: Ula,
    /// Memory model (determines read/write behavior).
    _model: PhantomData<M>,
}

impl<M: MemoryModel> Memory<M> {
    fn new() -> Self {
        Self {
            data: [0; 65536],
            ula: Ula::new(T_STATES_PER_FRAME_48K),
            _model: PhantomData,
        }
    }

    fn reset(&mut self) {
        self.data = [0; 65536];
        self.ula.reset();
    }

    /// Get screen memory slice.
    fn screen(&self) -> &[u8] {
        &self.data[0x4000..0x5B00]
    }
}

impl<M: MemoryModel> Bus for Memory<M> {
    fn read(&mut self, address: u32) -> u8 {
        let addr = (address & 0xFFFF) as u16;
        let model = M::default();

        // Apply contention if in contended memory
        if model.is_contended(addr) {
            let delay = self.ula.contention_delay();
            self.ula.tick(delay);
        }

        // Check for snow effect before ticking (timing matters)
        let snow = self.ula.check_snow(addr);

        self.ula.tick(3); // Memory read takes 3 T-states

        let value = model.read(&self.data, addr, &self.ula);

        if snow {
            // During snow, CPU receives corrupted data
            // Return the floating bus value (what ULA was trying to read)
            self.ula.floating_bus(self.screen())
        } else {
            value
        }
    }

    fn fetch(&mut self, address: u32) -> u8 {
        let addr = (address & 0xFFFF) as u16;
        let model = M::default();

        // Check for snow effect before M1 contention
        let snow = self.ula.check_snow(addr);

        // M1 cycle has different contention pattern: C:1, C:2 vs C:3 for normal read
        self.ula.m1_contention(addr, model.is_contended(addr));

        let value = model.read(&self.data, addr, &self.ula);

        if snow {
            // During snow, CPU receives corrupted data
            self.ula.floating_bus(self.screen())
        } else {
            value
        }
    }

    fn write(&mut self, address: u32, value: u8) {
        let addr = (address & 0xFFFF) as u16;
        let model = M::default();

        // Apply contention if in contended memory
        if model.is_contended(addr) {
            let delay = self.ula.contention_delay();
            self.ula.tick(delay);
        }
        self.ula.tick(3); // Memory write takes 3 T-states

        model.write(&mut self.data, addr, value);
    }

    fn tick(&mut self, cycles: u32) {
        self.ula.tick(cycles);
    }

    fn tick_address(&mut self, address: u32, cycles: u32) {
        let addr = (address & 0xFFFF) as u16;
        let model = M::default();

        // Apply contention for each cycle if address is in contended memory
        // This is used for internal CPU cycles that reference a contended address
        if model.is_contended(addr) {
            for _ in 0..cycles {
                let delay = self.ula.contention_delay();
                self.ula.tick(delay + 1);
            }
        } else {
            self.ula.tick(cycles);
        }
    }

    fn refresh(&mut self, ir: u16) {
        self.ula.refresh_contention(ir);
    }

    fn interrupt_acknowledge(&mut self, ir: u16) {
        self.ula.interrupt_acknowledge(ir);
    }
}

impl<M: MemoryModel> IoBus for Memory<M> {
    fn read_io(&mut self, port: u16) -> u8 {
        // Apply proper I/O contention timing
        self.ula.io_contention(port);

        if port & 0x01 == 0 {
            // ULA port - keyboard
            let high = (port >> 8) as u8;
            self.ula.read_keyboard(high)
        } else if port & 0xFF == 0x1F {
            // Kempston joystick (active high)
            self.ula.kempston
        } else {
            // Unattached port - return floating bus
            let model = M::default();
            model.read(&self.data, (port >> 8) as u16 | 0x4000, &self.ula)
        }
    }

    fn write_io(&mut self, port: u16, value: u8) {
        // Apply proper I/O contention timing
        self.ula.io_contention(port);

        if port & 0x01 == 0 {
            // ULA port
            self.ula.write_port(value);
        }
    }
}

/// Generic ZX Spectrum emulator.
pub struct Spectrum<M: MemoryModel> {
    cpu: Z80,
    memory: Memory<M>,
    tape: Tape,
    frame_count: u32,
    prev_beeper_level: bool,
    /// Tracks the last T-state for tape tick calculations.
    last_tape_t_state: u32,
}

impl<M: MemoryModel> Spectrum<M> {
    /// Create a new Spectrum with the given memory model.
    pub fn new() -> Self {
        Self {
            cpu: Z80::new(),
            memory: Memory::new(),
            tape: Tape::new(),
            frame_count: 0,
            prev_beeper_level: false,
            last_tape_t_state: 0,
        }
    }

    /// Get the model name.
    pub fn model_name(&self) -> &'static str {
        M::MODEL_NAME
    }

    /// Run one frame of emulation.
    fn run_frame_internal(&mut self) {
        self.memory.ula.start_frame();
        self.last_tape_t_state = 0;

        while !self.memory.ula.frame_complete() {
            // Update tape and EAR level based on elapsed T-states
            self.update_tape();

            // Check for interrupt after each instruction (Z80 samples INT at instruction end)
            // The INT line is active for ~32 T-states at frame start
            if self.memory.ula.int_pending() {
                self.cpu.interrupt(&mut self.memory);
            }

            // Check for tape trap (only when instant loading is enabled)
            if self.tape.instant_load() && self.cpu.pc() == 0x0556 {
                self.handle_tape_load();
            }

            self.cpu.step(&mut self.memory);
        }

        // Final tape update for remaining T-states in frame
        self.update_tape();
    }

    /// Update tape playback and EAR level.
    fn update_tape(&mut self) {
        let current_t_state = self.memory.ula.frame_t_state;
        let elapsed = current_t_state.saturating_sub(self.last_tape_t_state);

        if elapsed > 0 && self.tape.is_playing() && !self.tape.instant_load() {
            self.tape.tick(elapsed);
            self.memory.ula.ear_level = self.tape.ear_level();
        }

        self.last_tape_t_state = current_t_state;
    }

    /// Get screen memory.
    pub fn screen(&self) -> &[u8] {
        self.memory.screen()
    }

    /// Get current border color.
    pub fn border(&self) -> u8 {
        self.memory.ula.border
    }

    /// Get border color transitions for the current frame.
    pub fn border_transitions(&self) -> &[(u32, u8)] {
        &self.memory.ula.border_transitions
    }

    /// Get beeper transitions.
    pub fn beeper_transitions(&self) -> &[(u32, bool)] {
        &self.memory.ula.beeper_transitions
    }

    /// Get snow events for the current frame.
    /// Each event is (display_line, char_column) where the snow occurred.
    pub fn snow_events(&self) -> &[(u32, u32)] {
        &self.memory.ula.snow_events
    }

    /// Load bytes into memory at a given address (for testing).
    pub fn load(&mut self, address: u16, data: &[u8]) {
        for (i, byte) in data.iter().enumerate() {
            self.memory.data[address as usize + i] = *byte;
        }
    }

    /// Load tape data.
    pub fn load_tape(&mut self, data: Vec<u8>) {
        self.tape.load(data);
    }

    /// Enable or disable instant tape loading.
    ///
    /// When enabled (default), the ROM trap at 0x0556 is used for instant loading.
    /// When disabled, accurate pulse generation is used (supports turbo loaders).
    pub fn set_instant_tape_load(&mut self, enabled: bool) {
        self.tape.set_instant_load(enabled);
    }

    /// Start tape playback (for pulse-accurate loading).
    pub fn tape_play(&mut self) {
        self.tape.play();
    }

    /// Stop tape playback.
    pub fn tape_stop(&mut self) {
        self.tape.stop();
    }

    /// Load ROM into memory.
    pub fn load_rom(&mut self, rom: &[u8]) {
        self.memory.data[..rom.len()].copy_from_slice(rom);
    }

    /// Load a .SNA snapshot (48K only).
    pub fn load_sna(&mut self, data: &[u8]) -> Result<(), &'static str> {
        if !M::default().supports_sna() {
            return Err("This model does not support .SNA snapshots");
        }

        if data.len() != 49179 {
            return Err("Invalid .SNA file size (expected 49179 bytes)");
        }

        // Parse 27-byte header
        let i = data[0];
        let hl_shadow = u16::from_le_bytes([data[1], data[2]]);
        let de_shadow = u16::from_le_bytes([data[3], data[4]]);
        let bc_shadow = u16::from_le_bytes([data[5], data[6]]);
        let af_shadow = u16::from_le_bytes([data[7], data[8]]);
        let hl = u16::from_le_bytes([data[9], data[10]]);
        let de = u16::from_le_bytes([data[11], data[12]]);
        let bc = u16::from_le_bytes([data[13], data[14]]);
        let iy = u16::from_le_bytes([data[15], data[16]]);
        let ix = u16::from_le_bytes([data[17], data[18]]);
        let iff2 = data[19] & 0x04 != 0;
        let r = data[20];
        let af = u16::from_le_bytes([data[21], data[22]]);
        let sp = u16::from_le_bytes([data[23], data[24]]);
        let interrupt_mode = data[25];
        let border = data[26] & 0x07;

        // Load 48K RAM (0x4000-0xFFFF)
        self.memory.data[0x4000..].copy_from_slice(&data[27..]);

        // Set border color
        self.memory.ula.border = border;

        // Pop PC from stack
        let pc_low = self.memory.data[sp as usize];
        let pc_high = self.memory.data[sp.wrapping_add(1) as usize];
        let pc = u16::from_le_bytes([pc_low, pc_high]);
        let sp = sp.wrapping_add(2);

        // Restore CPU state
        self.cpu.load_state(
            af,
            bc,
            de,
            hl,
            af_shadow,
            bc_shadow,
            de_shadow,
            hl_shadow,
            ix,
            iy,
            sp,
            pc,
            i,
            r,
            iff2,
            iff2,
            interrupt_mode,
        );

        Ok(())
    }

    /// Press a key by row and bit.
    fn press_key(&mut self, row: usize, bit: u8) {
        self.memory.ula.keyboard[row] &= !(1 << bit);
    }

    /// Release a key by row and bit.
    fn release_key(&mut self, row: usize, bit: u8) {
        self.memory.ula.keyboard[row] |= 1 << bit;
    }

    fn handle_tape_load(&mut self) {
        let Some(block) = self.tape.next_block_for_trap() else {
            self.cpu.set_carry(false);
            self.cpu.force_ret(&mut self.memory);
            return;
        };

        if block.is_empty() {
            self.cpu.set_carry(false);
            self.cpu.force_ret(&mut self.memory);
            return;
        }

        let flag = block[0];
        let expected_flag = self.cpu.a();

        if flag != expected_flag {
            self.cpu.set_carry(false);
            self.cpu.force_ret(&mut self.memory);
            return;
        }

        let ix = self.cpu.ix();
        let de = self.cpu.de();
        let data = &block[1..block.len().saturating_sub(1)];
        let len = (de as usize).min(data.len());

        for i in 0..len {
            let addr = ix.wrapping_add(i as u16) as usize;
            // Only write to RAM, respecting the memory model
            if addr >= 0x4000 {
                self.memory.data[addr] = data[i];
            }
        }

        self.cpu.set_carry(true);
        self.cpu.force_ret(&mut self.memory);
    }
}

impl<M: MemoryModel> Default for Spectrum<M> {
    fn default() -> Self {
        Self::new()
    }
}

impl<M: MemoryModel + 'static> Machine for Spectrum<M> {
    fn video_config(&self) -> VideoConfig {
        VideoConfig {
            width: video::NATIVE_WIDTH,
            height: video::NATIVE_HEIGHT,
            fps: 50.0,
        }
    }

    fn audio_config(&self) -> AudioConfig {
        AudioConfig {
            sample_rate: audio::SAMPLE_RATE,
            samples_per_frame: audio::SAMPLES_PER_FRAME,
        }
    }

    fn run_frame(&mut self) {
        self.run_frame_internal();
        self.frame_count = self.frame_count.wrapping_add(1);
    }

    fn render(&self, buffer: &mut [u8]) {
        let flash_swap = (self.frame_count / 16) % 2 == 1;
        video::render_screen(
            self.screen(),
            self.border_transitions(),
            self.snow_events(),
            flash_swap,
            buffer,
        );
    }

    fn generate_audio(&mut self, buffer: &mut [f32]) {
        audio::generate_frame_samples(self.beeper_transitions(), self.prev_beeper_level, buffer);
        self.prev_beeper_level = self.memory.ula.beeper_level;
    }

    fn key_down(&mut self, key: KeyCode) {
        for &(row, bit) in input::map_key(key) {
            self.press_key(row, bit);
        }
    }

    fn key_up(&mut self, key: KeyCode) {
        for &(row, bit) in input::map_key(key) {
            self.release_key(row, bit);
        }
    }

    fn set_joystick(&mut self, _port: u8, state: JoystickState) {
        self.memory.ula.kempston = input::joystick_to_kempston(state);
    }

    fn reset(&mut self) {
        self.cpu = Z80::new();
        self.memory.reset();
        self.tape.clear();
        self.frame_count = 0;
        self.prev_beeper_level = false;
        self.last_tape_t_state = 0;
    }

    fn load_file(&mut self, path: &str, data: &[u8]) -> Result<(), String> {
        let lower = path.to_lowercase();

        if lower.ends_with(".rom") || lower == "48.rom" || lower == "16.rom" {
            self.load_rom(data);
            Ok(())
        } else if lower.ends_with(".sna") {
            self.load_sna(data).map_err(|e| e.to_string())
        } else if lower.ends_with(".tap") {
            self.load_tape(data.to_vec());
            Ok(())
        } else {
            Err(format!("Unknown file type: {}", path))
        }
    }
}
