//! Commodore 64 emulator.

use crate::cartridge::Cartridge;
use crate::config::{MachineConfig, MachineVariant, SidRevision, TimingMode};
use crate::disk::Disk;
use crate::input;
use crate::memory::Memory;
use crate::sid::Sid;
use crate::snapshot::Snapshot;
use crate::tap::{Tape, TapeFormat};
use crate::vic::{self, DISPLAY_HEIGHT, DISPLAY_WIDTH, Vic};
use cpu_6502::Mos6502;
use emu_core::{AudioConfig, Cpu, JoystickState, KeyCode, Machine, VideoConfig};

/// Audio sample rate
pub const SAMPLE_RATE: u32 = 44100;

/// Commodore 64 emulator.
pub struct C64 {
    /// Machine configuration (variant, chips, timing)
    config: MachineConfig,
    cpu: Mos6502,
    memory: Memory,
    vic: Vic,
    sid: Sid,
    frame_cycles: u32,
    /// Currently loaded disk image.
    disk: Option<Disk>,
    /// Tape player for TAP files.
    tape: Tape,
}

impl C64 {
    /// Create a new C64 with default configuration (PAL breadbin).
    pub fn new() -> Self {
        Self::with_variant(MachineVariant::default())
    }

    /// Create a C64 with a specific machine variant.
    pub fn with_variant(variant: MachineVariant) -> Self {
        Self::with_config(variant.config())
    }

    /// Create a C64 with a custom configuration.
    pub fn with_config(config: MachineConfig) -> Self {
        let sid = Sid::with_model(match config.sid {
            SidRevision::Mos6581 => crate::sid::SidModel::Mos6581,
            SidRevision::Mos8580 => crate::sid::SidModel::Mos8580,
        });

        let vic = Vic::with_revision(config.vic);

        let mut c64 = Self {
            config,
            cpu: Mos6502::new(),
            memory: Memory::new(),
            vic,
            sid,
            frame_cycles: 0,
            disk: None,
            tape: Tape::new(),
        };
        c64.memory.reset();
        c64
    }

    /// Get the current machine configuration.
    pub fn config(&self) -> &MachineConfig {
        &self.config
    }

    /// Get the timing mode (PAL/NTSC).
    pub fn timing_mode(&self) -> TimingMode {
        self.config.timing_mode()
    }

    /// Get the CPU clock speed in Hz.
    pub fn cpu_clock(&self) -> u32 {
        self.config.cpu_clock()
    }

    /// Get cycles per frame for this configuration.
    pub fn cycles_per_frame(&self) -> u32 {
        self.config.cycles_per_frame()
    }

    /// Get samples per audio frame.
    fn samples_per_frame(&self) -> usize {
        let fps = self.config.timing_mode().fps();
        (SAMPLE_RATE as f32 / fps).ceil() as usize
    }

    /// Load a D64 disk image.
    pub fn load_disk(&mut self, data: Vec<u8>) -> Result<(), &'static str> {
        let mut disk = Disk::new(data)?;
        // Simulate drive initialization - head knock as it seeks to track 0
        disk.head_knock();
        self.disk = Some(disk);
        Ok(())
    }

    /// Load and run the first PRG from the disk.
    pub fn autorun_disk(&mut self) -> Result<(), String> {
        // Find the first PRG entry
        let entry = {
            let disk = self.disk.as_ref().ok_or("No disk loaded")?;
            disk.read_directory()
                .into_iter()
                .find(|e| e.file_type == crate::disk::FileType::Prg && e.closed)
                .ok_or("No PRG file found on disk")?
        };

        // Load with audio feedback
        let prg_data = {
            let disk = self.disk.as_mut().ok_or("No disk loaded")?;
            disk.load_file_with_audio(&entry)
                .ok_or("Failed to load PRG file")?
        };

        self.load_prg(&prg_data)
    }

    /// Load a specific file from the disk by name.
    pub fn load_from_disk(&mut self, name: &str) -> Result<(), String> {
        // Find the file entry
        let entry = {
            let disk = self.disk.as_ref().ok_or("No disk loaded")?;
            disk.find_file(name)
                .ok_or_else(|| format!("File '{}' not found on disk", name))?
        };

        // Load with audio feedback
        let prg_data = {
            let disk = self.disk.as_mut().ok_or("No disk loaded")?;
            disk.load_file_with_audio(&entry)
                .ok_or("Failed to load file")?
        };

        self.load_prg(&prg_data)
    }

    /// Get directory listing of loaded disk.
    pub fn disk_directory(&self) -> Option<Vec<(String, u16)>> {
        self.disk.as_ref().map(|d| {
            d.read_directory()
                .iter()
                .filter(|e| e.closed)
                .map(|e| (e.name_string(), e.size_sectors))
                .collect()
        })
    }

    /// Load a TAP tape image.
    pub fn load_tape(&mut self, data: Vec<u8>) -> Result<(), &'static str> {
        self.tape.load(data)
    }

    /// Start tape playback.
    pub fn play_tape(&mut self) {
        self.tape.play();
    }

    /// Stop tape playback.
    pub fn stop_tape(&mut self) {
        self.tape.stop();
    }

    /// Rewind tape to beginning.
    pub fn rewind_tape(&mut self) {
        self.tape.rewind();
    }

    /// Check if tape is loaded.
    pub fn is_tape_loaded(&self) -> bool {
        self.tape.is_loaded()
    }

    /// Check if tape is playing.
    pub fn is_tape_playing(&self) -> bool {
        self.tape.is_playing()
    }

    /// Get tape position as percentage.
    pub fn tape_position(&self) -> u8 {
        self.tape.position_percent()
    }

    /// Get tape format.
    pub fn tape_format(&self) -> TapeFormat {
        self.tape.format()
    }

    /// Get T64 tape directory listing.
    pub fn tape_directory(&self) -> Vec<(String, u16, u16)> {
        self.tape
            .directory()
            .iter()
            .map(|e| (e.name_string(), e.load_addr, e.end_addr))
            .collect()
    }

    /// Load the current T64 program directly into memory (instant load).
    /// Returns the load address, or None if not a T64 tape or no more entries.
    pub fn load_t64_program(&mut self) -> Option<u16> {
        let (load_addr, data) = self.tape.get_t64_program()?;

        // Load into RAM
        for (i, &byte) in data.iter().enumerate() {
            let addr = load_addr.wrapping_add(i as u16);
            self.memory.ram[addr as usize] = byte;
        }

        // Update BASIC pointers if loaded at $0801
        if load_addr == 0x0801 {
            let end_addr = load_addr.wrapping_add(data.len() as u16);
            self.memory.ram[0x2D] = (end_addr & 0xFF) as u8;
            self.memory.ram[0x2E] = (end_addr >> 8) as u8;
            self.memory.ram[0x2F] = (end_addr & 0xFF) as u8;
            self.memory.ram[0x30] = (end_addr >> 8) as u8;
            self.memory.ram[0x31] = (end_addr & 0xFF) as u8;
            self.memory.ram[0x32] = (end_addr >> 8) as u8;
        }

        // Advance to next entry
        self.tape.next_t64_entry();

        Some(load_addr)
    }

    /// Seek tape to position (0-100%).
    pub fn seek_tape(&mut self, percent: u8) {
        self.tape.seek(percent);
    }

    /// Enable or disable tape audio (the screech sound).
    pub fn set_tape_audio(&mut self, enabled: bool) {
        self.tape.set_audio_enabled(enabled);
    }

    /// Enable or disable disk audio (head step clicks).
    pub fn set_disk_audio(&mut self, enabled: bool) {
        if let Some(ref mut disk) = self.disk {
            disk.set_audio_enabled(enabled);
        }
    }

    /// Load a cartridge from .crt file data.
    pub fn load_cartridge(&mut self, data: &[u8]) -> Result<(), &'static str> {
        let cartridge = Cartridge::load_crt(data)?;
        self.memory.insert_cartridge(cartridge);
        Ok(())
    }

    /// Load a raw cartridge ROM (8K or 16K).
    pub fn load_cartridge_raw(&mut self, data: &[u8]) -> Result<(), &'static str> {
        let cartridge = Cartridge::load_raw(data)?;
        self.memory.insert_cartridge(cartridge);
        Ok(())
    }

    /// Remove the inserted cartridge.
    pub fn remove_cartridge(&mut self) {
        self.memory.remove_cartridge();
    }

    /// Check if a cartridge is inserted.
    pub fn has_cartridge(&self) -> bool {
        self.memory.has_cartridge()
    }

    /// Get cartridge name (if inserted).
    pub fn cartridge_name(&self) -> Option<&str> {
        if self.memory.has_cartridge() {
            Some(self.memory.cartridge.name())
        } else {
            None
        }
    }

    /// Check if cartridge has autostart signature (CBM80).
    pub fn cartridge_has_autostart(&self) -> bool {
        self.memory.cartridge.has_autostart()
    }

    /// Check if cartridge is a freezer cartridge (Action Replay, etc.).
    pub fn cartridge_is_freezer(&self) -> bool {
        self.memory.cartridge.is_freezer()
    }

    /// Press the freeze button on a freezer cartridge.
    /// Returns true if the button was pressed (cartridge is a freezer and is active).
    pub fn freeze(&mut self) -> bool {
        if self.memory.cartridge.freeze() {
            // Trigger NMI
            self.cpu.nmi(&mut self.memory);
            true
        } else {
            false
        }
    }

    /// Save machine state to a snapshot.
    pub fn save_state(&self) -> Snapshot {
        Snapshot::capture(
            &self.cpu,
            &self.memory,
            &self.vic,
            &self.sid,
            self.frame_cycles,
        )
    }

    /// Save machine state to bytes.
    pub fn save_state_bytes(&self) -> Vec<u8> {
        self.save_state().to_bytes()
    }

    /// Load machine state from a snapshot.
    pub fn load_state(&mut self, snapshot: &Snapshot) {
        // Restore CPU
        self.cpu.set_a(snapshot.cpu.a);
        self.cpu.set_x(snapshot.cpu.x);
        self.cpu.set_y(snapshot.cpu.y);
        self.cpu.set_sp(snapshot.cpu.sp);
        self.cpu.set_pc(snapshot.cpu.pc);
        self.cpu.set_status(snapshot.cpu.status);

        // Restore memory
        self.memory
            .ram
            .copy_from_slice(snapshot.memory.ram.as_ref());
        self.memory.port_ddr = snapshot.memory.port_ddr;
        self.memory.port_data = snapshot.memory.port_data;
        self.memory.vic_registers = snapshot.memory.vic_registers;
        self.memory.sid_registers = snapshot.memory.sid_registers;
        self.memory.color_ram = snapshot.memory.color_ram;
        self.memory.keyboard_matrix = snapshot.memory.keyboard_matrix;
        self.memory.current_raster_line = snapshot.memory.current_raster_line;

        // Restore VIC
        self.vic.raster_line = snapshot.vic.raster_line;
        self.vic.frame_cycle = snapshot.vic.frame_cycle;
        self.vic.ba_low = snapshot.vic.ba_low;
        self.vic.sprite_dma_active = snapshot.vic.sprite_dma_active;
        self.vic.sprite_display_count = snapshot.vic.sprite_display_count;

        // Restore CIAs
        self.restore_cia(&snapshot.cia1, true);
        self.restore_cia(&snapshot.cia2, false);

        // Restore frame cycles
        self.frame_cycles = snapshot.frame_cycles;
    }

    /// Load machine state from bytes.
    pub fn load_state_bytes(&mut self, data: &[u8]) -> Result<(), &'static str> {
        let snapshot = Snapshot::from_bytes(data)?;
        self.load_state(&snapshot);
        Ok(())
    }

    /// Helper to restore CIA state.
    fn restore_cia(&mut self, state: &crate::snapshot::CiaState, is_cia1: bool) {
        let cia = if is_cia1 {
            &mut self.memory.cia1
        } else {
            &mut self.memory.cia2
        };

        cia.pra = state.pra;
        cia.prb = state.prb;
        cia.ddra = state.ddra;
        cia.ddrb = state.ddrb;
        cia.ta_lo = state.ta_lo;
        cia.ta_hi = state.ta_hi;
        cia.ta_latch_lo = state.ta_latch_lo;
        cia.ta_latch_hi = state.ta_latch_hi;
        cia.tb_lo = state.tb_lo;
        cia.tb_hi = state.tb_hi;
        cia.tb_latch_lo = state.tb_latch_lo;
        cia.tb_latch_hi = state.tb_latch_hi;
        cia.cra = state.cra;
        cia.crb = state.crb;
        cia.icr = state.icr;
        cia.icr_mask = state.icr_mask;
        cia.tod_10ths = state.tod_10ths;
        cia.tod_sec = state.tod_sec;
        cia.tod_min = state.tod_min;
        cia.tod_hr = state.tod_hr;
        cia.alarm_10ths = state.alarm_10ths;
        cia.alarm_sec = state.alarm_sec;
        cia.alarm_min = state.alarm_min;
        cia.alarm_hr = state.alarm_hr;
        cia.tod_running = state.tod_running;
        cia.tod_latched = state.tod_latched;
    }

    /// Dump current CPU state for debugging.
    pub fn dump_cpu(&self) -> String {
        self.save_state().dump_cpu()
    }

    /// Dump current VIC state for debugging.
    pub fn dump_vic(&self) -> String {
        self.save_state().dump_vic()
    }

    /// Peek memory at address (for debugging).
    pub fn peek(&self, addr: u16) -> u8 {
        self.memory.ram[addr as usize]
    }

    /// Peek memory range (for debugging).
    pub fn peek_range(&self, start: u16, len: u16) -> &[u8] {
        let start = start as usize;
        let end = (start + len as usize).min(65536);
        &self.memory.ram[start..end]
    }

    /// Load BASIC ROM.
    pub fn load_basic(&mut self, data: &[u8]) {
        self.memory.load_basic(data);
    }

    /// Load KERNAL ROM.
    pub fn load_kernal(&mut self, data: &[u8]) {
        self.memory.load_kernal(data);
    }

    /// Load Character ROM.
    pub fn load_chargen(&mut self, data: &[u8]) {
        self.memory.load_chargen(data);
    }

    /// Load a PRG file.
    pub fn load_prg(&mut self, data: &[u8]) -> Result<(), String> {
        if data.len() < 2 {
            return Err("PRG file too short".to_string());
        }

        // First two bytes are load address (little-endian)
        let load_addr = u16::from_le_bytes([data[0], data[1]]);
        let program = &data[2..];

        // Load into RAM
        for (i, &byte) in program.iter().enumerate() {
            let addr = load_addr.wrapping_add(i as u16);
            self.memory.ram[addr as usize] = byte;
        }

        // Update BASIC pointers if loaded at $0801 (standard BASIC start)
        if load_addr == 0x0801 {
            let end_addr = load_addr.wrapping_add(program.len() as u16);
            // Set end of BASIC (VARTAB)
            self.memory.ram[0x2D] = (end_addr & 0xFF) as u8;
            self.memory.ram[0x2E] = (end_addr >> 8) as u8;
            // Set end of variables (ARYTAB)
            self.memory.ram[0x2F] = (end_addr & 0xFF) as u8;
            self.memory.ram[0x30] = (end_addr >> 8) as u8;
            // Set end of arrays (STREND)
            self.memory.ram[0x31] = (end_addr & 0xFF) as u8;
            self.memory.ram[0x32] = (end_addr >> 8) as u8;
        }

        Ok(())
    }

    /// Run one frame of emulation.
    fn run_frame_internal(&mut self) {
        let cycles_per_frame = self.cycles_per_frame();
        let cycles_per_line = self.config.timing_mode().cycles_per_line();

        self.frame_cycles = 0;
        self.memory.cycles = 0;
        self.vic.reset_frame();

        while self.frame_cycles < cycles_per_frame {
            // Tick VIC first - it may steal cycles via badlines
            let ba_low = self.vic.tick(&self.memory.vic_registers);

            if !ba_low {
                // CPU only runs when bus is available (BA high)
                let prev_cycles = self.memory.cycles;
                self.cpu.step(&mut self.memory);
                let cpu_cycles = self.memory.cycles - prev_cycles;

                // Update tape motor state from processor port bit 5 (active low)
                // (SX-64 and C64 GS have no cassette port, but this is harmless)
                let motor_on = self.memory.port_data & 0x20 == 0;
                self.tape.set_motor(motor_on);

                // Tick tape and update signal in memory
                self.tape.tick(cpu_cycles);
                self.memory.tape_signal = self.tape.signal_level;

                // Process pending SID register writes
                for (reg, value) in self.memory.sid_writes.drain(..) {
                    self.sid.write(reg, value);
                }

                // Tick SID oscillators and envelopes
                self.sid.tick(cpu_cycles);

                // Update readable SID registers
                self.memory.sid_registers[0x1B] = self.sid.read(0x1B);
                self.memory.sid_registers[0x1C] = self.sid.read(0x1C);

                // Tick CIA1 timers and check for IRQ
                if self.memory.tick_cia1(cpu_cycles) {
                    self.cpu.interrupt(&mut self.memory);
                }

                // Tick CIA2 timers and check for NMI
                if self.memory.tick_cia2(cpu_cycles) {
                    self.cpu.nmi(&mut self.memory);
                }

                // Advance frame_cycles by CPU cycles (VIC already ticked once)
                // We need to sync frame_cycles with actual elapsed time
                if cpu_cycles > 1 {
                    // Catch up VIC ticks for multi-cycle instructions
                    for _ in 1..cpu_cycles {
                        self.vic.tick(&self.memory.vic_registers);
                    }
                }
            }

            self.frame_cycles = self.vic.frame_cycle;

            // Sync raster line to memory for accurate reads of $D011/$D012
            self.memory.current_raster_line = self.vic.raster_line;

            // Check for VIC-II raster interrupt at cycle 0 of each line
            if self.vic.frame_cycle % cycles_per_line == 0
                && self.vic.check_raster_irq(&self.memory.vic_registers)
            {
                // Set raster IRQ flag and main IRQ flag in $D019
                self.memory.vic_registers[0x19] |= 0x81;
                self.cpu.interrupt(&mut self.memory);
            }
        }

        // Tick TOD clocks at end of frame (50Hz)
        let (tod_irq, tod_nmi) = self.memory.tick_tod();
        if tod_irq {
            self.cpu.interrupt(&mut self.memory);
        }
        if tod_nmi {
            self.cpu.nmi(&mut self.memory);
        }
    }

    /// Press a key in the keyboard matrix.
    fn press_key(&mut self, col: usize, row: usize) {
        if col < 8 && row < 8 {
            self.memory.keyboard_matrix[col] &= !(1 << row);
        }
    }

    /// Release a key in the keyboard matrix.
    fn release_key(&mut self, col: usize, row: usize) {
        if col < 8 && row < 8 {
            self.memory.keyboard_matrix[col] |= 1 << row;
        }
    }
}

impl Default for C64 {
    fn default() -> Self {
        Self::new()
    }
}

impl Machine for C64 {
    fn video_config(&self) -> VideoConfig {
        VideoConfig {
            width: DISPLAY_WIDTH,
            height: DISPLAY_HEIGHT,
            fps: self.config.timing_mode().fps(),
        }
    }

    fn audio_config(&self) -> AudioConfig {
        AudioConfig {
            sample_rate: SAMPLE_RATE,
            samples_per_frame: self.samples_per_frame(),
        }
    }

    fn run_frame(&mut self) {
        self.run_frame_internal();
    }

    fn render(&mut self, buffer: &mut [u8]) {
        vic::render(&self.vic, &mut self.memory, buffer);
    }

    fn generate_audio(&mut self, buffer: &mut [f32]) {
        // Generate SID audio (clock speed varies by PAL/NTSC)
        self.sid
            .generate_samples(buffer, self.cpu_clock(), SAMPLE_RATE);

        // Mix in tape audio (the characteristic screech)
        if self.tape.is_playing() && self.tape.is_audio_enabled() {
            let tape_sample = self.tape.audio_sample();
            for sample in buffer.iter_mut() {
                *sample = (*sample + tape_sample).clamp(-1.0, 1.0);
            }
        }

        // Mix in disk audio (head steps, knocks)
        if let Some(ref mut disk) = self.disk {
            if disk.is_audio_enabled() && disk.has_audio_pending() {
                for sample in buffer.iter_mut() {
                    let disk_sample = disk.audio_sample();
                    *sample = (*sample + disk_sample).clamp(-1.0, 1.0);
                }
            }
        }
    }

    fn key_down(&mut self, key: KeyCode) {
        // Handle keys that need shift on C64
        if input::needs_shift(key) {
            self.press_key(1, 7); // Left shift
        }

        if let Some((col, row)) = input::map_key(key) {
            self.press_key(col, row);
        }
    }

    fn key_up(&mut self, key: KeyCode) {
        // Release shift for keys that needed it
        if input::needs_shift(key) {
            self.release_key(1, 7);
        }

        if let Some((col, row)) = input::map_key(key) {
            self.release_key(col, row);
        }
    }

    fn set_joystick(&mut self, port: u8, state: JoystickState) {
        // Joystick 1 is on CIA1 port A, Joystick 2 on CIA1 port B
        // Active low: 0 = pressed
        let mut value = 0xFF;

        if state.up {
            value &= !0x01;
        }
        if state.down {
            value &= !0x02;
        }
        if state.left {
            value &= !0x04;
        }
        if state.right {
            value &= !0x08;
        }
        if state.fire {
            value &= !0x10;
        }

        match port {
            0 => {
                // Port 2 (primary) uses CIA1 port A
                self.memory.cia1.pra = (self.memory.cia1.pra & 0xE0) | (value & 0x1F);
            }
            1 => {
                // Port 1 uses CIA1 port B
                self.memory.cia1.prb = (self.memory.cia1.prb & 0xE0) | (value & 0x1F);
            }
            _ => {}
        }
    }

    fn reset(&mut self) {
        self.memory.reset();
        self.vic.reset();
        self.sid.reset();
        self.tape.stop();
        self.tape.rewind();
        self.cpu.reset(&mut self.memory);
        self.frame_cycles = 0;
    }

    fn load_file(&mut self, path: &str, data: &[u8]) -> Result<(), String> {
        let lower = path.to_lowercase();

        if lower.ends_with(".prg") {
            self.load_prg(data)
        } else if lower.ends_with(".d64") {
            // Load disk and auto-run first PRG
            self.load_disk(data.to_vec()).map_err(|e| e.to_string())?;
            self.autorun_disk()
        } else if lower.ends_with(".tap") {
            // Load tape and start playback
            self.load_tape(data.to_vec()).map_err(|e| e.to_string())?;
            self.play_tape();
            Ok(())
        } else if lower.ends_with(".t64") {
            // Load T64 tape archive and instantly load first program
            self.load_tape(data.to_vec()).map_err(|e| e.to_string())?;
            self.load_t64_program()
                .map(|_| ())
                .ok_or_else(|| "No programs in T64 file".to_string())
        } else if lower.ends_with(".crt") {
            // Load cartridge and reset to start it
            self.load_cartridge(data).map_err(|e| e.to_string())?;
            self.reset();
            Ok(())
        } else if lower.ends_with("basic.bin") || lower.ends_with("basic.rom") {
            self.load_basic(data);
            Ok(())
        } else if lower.ends_with("kernal.bin") || lower.ends_with("kernal.rom") {
            self.load_kernal(data);
            Ok(())
        } else if lower.ends_with("chargen.bin") || lower.ends_with("chargen.rom") {
            self.load_chargen(data);
            Ok(())
        } else {
            Err(format!("Unknown file type: {}", path))
        }
    }
}
