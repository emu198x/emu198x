//! TAP tape image support for the Commodore 64.
//!
//! TAP files contain pulse timing data that represents the actual
//! tape signal. This module provides:
//! - TAP file parsing (v0 and v1 formats)
//! - T64 tape archive support (instant loading)
//! - Pulse playback synchronized to CPU cycles
//! - Motor control simulation
//! - Play button simulation

/// TAP file signature.
const TAP_SIGNATURE: &[u8; 12] = b"C64-TAPE-RAW";

/// T64 file signature.
const T64_SIGNATURE: &[u8; 3] = b"C64";

/// Tape format type.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TapeFormat {
    /// No tape loaded.
    None,
    /// TAP file with pulse data.
    Tap,
    /// T64 tape archive with raw program data.
    T64,
}

/// Entry in a T64 tape archive.
#[derive(Clone, Debug)]
pub struct T64Entry {
    /// Filename (up to 16 characters, padded with spaces).
    pub name: [u8; 16],
    /// Load address.
    pub load_addr: u16,
    /// End address.
    pub end_addr: u16,
    /// Offset in T64 data.
    offset: u32,
    /// Size in bytes.
    size: u32,
}

impl T64Entry {
    /// Get the filename as a string.
    pub fn name_string(&self) -> String {
        self.name
            .iter()
            .take_while(|&&c| c != 0xA0 && c != 0)
            .map(|&c| {
                if c >= 0x20 && c < 0x80 {
                    c as char
                } else {
                    '?'
                }
            })
            .collect()
    }
}

/// Tape audio volume (0.0 to 1.0).
const TAPE_AUDIO_VOLUME: f32 = 0.15;

/// Tape image loaded from a TAP or T64 file.
pub struct Tape {
    /// Raw file data.
    data: Vec<u8>,
    /// Tape format.
    format: TapeFormat,
    /// TAP version (0 or 1).
    version: u8,
    /// Current position in pulse data (TAP mode).
    position: usize,
    /// Cycles remaining until next pulse edge.
    cycles_remaining: u32,
    /// Current signal level (directly affects $01 bit 4).
    pub signal_level: bool,
    /// Whether tape motor is running (from $01 bit 5).
    motor_on: bool,
    /// Whether play button is pressed.
    play_pressed: bool,
    /// T64 directory entries.
    t64_entries: Vec<T64Entry>,
    /// Current T64 entry index being loaded.
    t64_current: usize,
    /// Audio enabled flag.
    audio_enabled: bool,
}

impl Tape {
    /// Create an empty tape (no tape loaded).
    pub fn new() -> Self {
        Self {
            data: Vec::new(),
            format: TapeFormat::None,
            version: 0,
            position: 0,
            cycles_remaining: 0,
            signal_level: false,
            motor_on: false,
            play_pressed: false,
            t64_entries: Vec::new(),
            t64_current: 0,
            audio_enabled: true,
        }
    }

    /// Load a TAP or T64 file (auto-detects format).
    pub fn load(&mut self, data: Vec<u8>) -> Result<(), &'static str> {
        // Try T64 first (shorter signature)
        if data.len() >= 64 && &data[0..3] == T64_SIGNATURE {
            return self.load_t64(data);
        }

        // Try TAP
        if data.len() >= 20 && &data[0..12] == TAP_SIGNATURE {
            return self.load_tap(data);
        }

        Err("Unknown tape format")
    }

    /// Load a TAP file.
    fn load_tap(&mut self, data: Vec<u8>) -> Result<(), &'static str> {
        let version = data[12];
        if version > 1 {
            return Err("Unsupported TAP version");
        }

        // Data length is stored in bytes 16-19 (little-endian)
        let data_len = u32::from_le_bytes([data[16], data[17], data[18], data[19]]) as usize;

        if data.len() < 20 + data_len {
            return Err("TAP file truncated");
        }

        self.format = TapeFormat::Tap;
        self.version = version;
        self.data = data[20..20 + data_len].to_vec();
        self.position = 0;
        self.cycles_remaining = 0;
        self.signal_level = false;
        self.play_pressed = false;
        self.t64_entries.clear();

        Ok(())
    }

    /// Load a T64 tape archive.
    fn load_t64(&mut self, data: Vec<u8>) -> Result<(), &'static str> {
        if data.len() < 64 {
            return Err("T64 file too small");
        }

        // Parse header
        let max_entries = u16::from_le_bytes([data[34], data[35]]) as usize;
        let used_entries = u16::from_le_bytes([data[36], data[37]]) as usize;

        if used_entries == 0 {
            return Err("T64 file has no entries");
        }

        // Parse directory entries (32 bytes each, starting at offset 64)
        let mut entries = Vec::new();
        for i in 0..max_entries.min(used_entries) {
            let entry_offset = 64 + i * 32;
            if entry_offset + 32 > data.len() {
                break;
            }

            let entry_type = data[entry_offset];
            if entry_type == 0 {
                continue; // Empty entry
            }

            let load_addr = u16::from_le_bytes([data[entry_offset + 2], data[entry_offset + 3]]);
            let end_addr = u16::from_le_bytes([data[entry_offset + 4], data[entry_offset + 5]]);
            let offset = u32::from_le_bytes([
                data[entry_offset + 8],
                data[entry_offset + 9],
                data[entry_offset + 10],
                data[entry_offset + 11],
            ]);

            let mut name = [0u8; 16];
            name.copy_from_slice(&data[entry_offset + 16..entry_offset + 32]);

            let size = (end_addr.saturating_sub(load_addr)) as u32;

            entries.push(T64Entry {
                name,
                load_addr,
                end_addr,
                offset,
                size,
            });
        }

        if entries.is_empty() {
            return Err("T64 file has no valid entries");
        }

        self.format = TapeFormat::T64;
        self.version = 0;
        self.data = data;
        self.position = 0;
        self.cycles_remaining = 0;
        self.signal_level = false;
        self.play_pressed = false;
        self.t64_entries = entries;
        self.t64_current = 0;

        Ok(())
    }

    /// Get the tape format.
    pub fn format(&self) -> TapeFormat {
        self.format
    }

    /// Check if a tape is loaded.
    pub fn is_loaded(&self) -> bool {
        self.format != TapeFormat::None
    }

    /// Check if tape is currently playing (motor on + play pressed).
    pub fn is_playing(&self) -> bool {
        self.play_pressed && self.motor_on
    }

    /// Check if tape has reached the end.
    pub fn is_at_end(&self) -> bool {
        match self.format {
            TapeFormat::None => true,
            TapeFormat::Tap => self.position >= self.data.len(),
            TapeFormat::T64 => self.t64_current >= self.t64_entries.len(),
        }
    }

    /// Press play button (start tape playback).
    pub fn play(&mut self) {
        if self.is_loaded() {
            self.play_pressed = true;
            if self.format == TapeFormat::Tap && self.position == 0 {
                self.load_next_pulse();
            }
        }
    }

    /// Release play button (stop tape playback).
    pub fn stop(&mut self) {
        self.play_pressed = false;
    }

    /// Check if play button is pressed.
    pub fn is_play_pressed(&self) -> bool {
        self.play_pressed
    }

    /// Rewind tape to beginning.
    pub fn rewind(&mut self) {
        self.position = 0;
        self.cycles_remaining = 0;
        self.signal_level = false;
        self.t64_current = 0;
    }

    /// Seek to a position (0-100%).
    pub fn seek(&mut self, percent: u8) {
        let percent = percent.min(100) as usize;
        match self.format {
            TapeFormat::None => {}
            TapeFormat::Tap => {
                self.position = (self.data.len() * percent) / 100;
                self.cycles_remaining = 0;
                if self.is_playing() {
                    self.load_next_pulse();
                }
            }
            TapeFormat::T64 => {
                self.t64_current = (self.t64_entries.len() * percent) / 100;
            }
        }
    }

    /// Set motor state (controlled by $01 bit 5).
    pub fn set_motor(&mut self, on: bool) {
        self.motor_on = on;
    }

    /// Get motor state.
    pub fn motor_on(&self) -> bool {
        self.motor_on
    }

    /// Get current tape position as percentage (0-100).
    pub fn position_percent(&self) -> u8 {
        match self.format {
            TapeFormat::None => 0,
            TapeFormat::Tap => {
                if self.data.is_empty() {
                    0
                } else {
                    ((self.position as u64 * 100) / self.data.len() as u64) as u8
                }
            }
            TapeFormat::T64 => {
                if self.t64_entries.is_empty() {
                    0
                } else {
                    ((self.t64_current as u64 * 100) / self.t64_entries.len() as u64) as u8
                }
            }
        }
    }

    /// Get T64 directory entries (empty for TAP files).
    pub fn directory(&self) -> &[T64Entry] {
        &self.t64_entries
    }

    /// Get current T64 entry for instant loading.
    /// Returns (load_address, program_data) if available.
    pub fn get_t64_program(&self) -> Option<(u16, Vec<u8>)> {
        if self.format != TapeFormat::T64 {
            return None;
        }

        let entry = self.t64_entries.get(self.t64_current)?;
        let offset = entry.offset as usize;
        let size = entry.size as usize;

        if offset + size > self.data.len() {
            return None;
        }

        Some((entry.load_addr, self.data[offset..offset + size].to_vec()))
    }

    /// Advance to next T64 entry.
    pub fn next_t64_entry(&mut self) {
        if self.format == TapeFormat::T64 && self.t64_current < self.t64_entries.len() {
            self.t64_current += 1;
        }
    }

    /// Tick the tape for the given number of CPU cycles.
    /// Updates signal_level when pulses occur (TAP mode only).
    pub fn tick(&mut self, cycles: u32) {
        // T64 doesn't need ticking - it uses instant loading
        if self.format != TapeFormat::Tap || !self.is_playing() || self.is_at_end() {
            return;
        }

        let mut remaining = cycles;

        while remaining > 0 && !self.is_at_end() {
            if remaining >= self.cycles_remaining {
                remaining -= self.cycles_remaining;
                self.cycles_remaining = 0;

                // Pulse edge - toggle signal
                self.signal_level = !self.signal_level;

                // Load next pulse duration
                self.load_next_pulse();
            } else {
                self.cycles_remaining -= remaining;
                remaining = 0;
            }
        }
    }

    /// Load the next pulse duration from TAP data.
    fn load_next_pulse(&mut self) {
        if self.position >= self.data.len() {
            return;
        }

        let byte = self.data[self.position];
        self.position += 1;

        if byte == 0 && self.version == 1 {
            // Version 1: 0 byte followed by 3-byte duration (little-endian)
            if self.position + 3 <= self.data.len() {
                let lo = self.data[self.position] as u32;
                let mid = self.data[self.position + 1] as u32;
                let hi = self.data[self.position + 2] as u32;
                self.cycles_remaining = lo | (mid << 8) | (hi << 16);
                self.position += 3;
            }
        } else if byte == 0 && self.version == 0 {
            // Version 0: 0 byte means 256 * 8 cycles
            self.cycles_remaining = 256 * 8;
        } else {
            // Normal pulse: byte * 8 cycles
            self.cycles_remaining = byte as u32 * 8;
        }
    }

    /// Clear the loaded tape.
    pub fn clear(&mut self) {
        self.data.clear();
        self.format = TapeFormat::None;
        self.position = 0;
        self.cycles_remaining = 0;
        self.signal_level = false;
        self.play_pressed = false;
        self.t64_entries.clear();
        self.t64_current = 0;
    }

    /// Enable or disable tape audio.
    pub fn set_audio_enabled(&mut self, enabled: bool) {
        self.audio_enabled = enabled;
    }

    /// Check if tape audio is enabled.
    pub fn is_audio_enabled(&self) -> bool {
        self.audio_enabled
    }

    /// Get audio sample for the current tape state.
    /// Returns a value in the range -1.0 to 1.0.
    pub fn audio_sample(&self) -> f32 {
        if !self.audio_enabled || !self.is_playing() || self.format != TapeFormat::Tap {
            return 0.0;
        }

        // Convert signal level to audio: high = positive, low = negative
        // This creates the characteristic screech sound
        let sample = if self.signal_level { 1.0 } else { -1.0 };
        sample * TAPE_AUDIO_VOLUME
    }
}

impl Default for Tape {
    fn default() -> Self {
        Self::new()
    }
}
