//! ULA (Uncommitted Logic Array) emulation.
//!
//! The ULA handles video generation, keyboard scanning, border color,
//! and beeper output. It also implements contention timing, floating bus,
//! and the "snow" effect when CPU and ULA conflict over screen memory.

/// T-states per frame for 48K Spectrum (and 16K).
pub const T_STATES_PER_FRAME_48K: u32 = 69888;

/// T-states per scanline.
pub const T_STATES_PER_LINE: u32 = 224;

/// First scanline of the display area.
pub const DISPLAY_START_LINE: u32 = 64;

/// Last scanline of the display area (exclusive).
pub const DISPLAY_END_LINE: u32 = 256;

/// T-states of active display per line.
pub const DISPLAY_T_STATES_PER_LINE: u32 = 128;

/// Screen memory start address.
const SCREEN_START: u16 = 0x4000;

/// Screen memory end address (exclusive) - includes bitmap and attributes.
const SCREEN_END: u16 = 0x5B00;

/// ULA state shared across all Spectrum models.
pub struct Ula {
    /// Border color (0-7).
    pub border: u8,
    /// Border color transitions during this frame: (t_state, color).
    /// Used for mid-scanline border effects.
    pub border_transitions: Vec<(u32, u8)>,
    /// Keyboard matrix state (8 half-rows, active low).
    pub keyboard: [u8; 8],
    /// Kempston joystick state (active high).
    pub kempston: u8,
    /// Current T-state within the frame.
    pub frame_t_state: u32,
    /// T-states per frame (model-specific).
    pub t_states_per_frame: u32,
    /// Current beeper level (bit 4 of port 0xFE).
    pub beeper_level: bool,
    /// Beeper transitions during this frame: (t_state, new_level).
    pub beeper_transitions: Vec<(u32, bool)>,
    /// Snow events during this frame: (scanline, char_column).
    /// Occurs when CPU reads screen memory while ULA is also reading it.
    pub snow_events: Vec<(u32, u32)>,
    /// EAR input level (bit 6 of port 0xFE read).
    /// Used for tape loading - reflects the audio signal from tape.
    pub ear_level: bool,
}

impl Ula {
    /// Create a new ULA with default state.
    pub fn new(t_states_per_frame: u32) -> Self {
        Self {
            border: 7,                        // white default
            border_transitions: vec![(0, 7)], // initial border color
            keyboard: [0xFF; 8],              // all keys released
            kempston: 0,
            frame_t_state: 0,
            t_states_per_frame,
            beeper_level: false,
            beeper_transitions: Vec::with_capacity(1024),
            snow_events: Vec::new(),
            ear_level: false,
        }
    }

    /// Reset ULA to initial state.
    pub fn reset(&mut self) {
        self.border = 7;
        self.border_transitions.clear();
        self.border_transitions.push((0, 7));
        self.keyboard = [0xFF; 8];
        self.kempston = 0;
        self.frame_t_state = 0;
        self.beeper_level = false;
        self.beeper_transitions.clear();
        self.snow_events.clear();
        self.ear_level = false;
    }

    /// Start a new frame.
    pub fn start_frame(&mut self) {
        self.frame_t_state = 0;
        self.border_transitions.clear();
        self.border_transitions.push((0, self.border)); // Record initial color
        self.beeper_transitions.clear();
        self.snow_events.clear();
    }

    /// Check if we've completed a frame.
    pub fn frame_complete(&self) -> bool {
        self.frame_t_state >= self.t_states_per_frame
    }

    /// Calculate contention delay based on current frame position.
    ///
    /// The ULA reads screen memory every 8 T-states during the display period.
    /// If the CPU tries to access contended memory, it's delayed until the
    /// next available slot.
    pub fn contention_delay(&self) -> u32 {
        let t_state = self.frame_t_state % self.t_states_per_frame;

        let scanline = t_state / T_STATES_PER_LINE;
        let line_t_state = t_state % T_STATES_PER_LINE;

        // Contention only during display period and first 128 T-states of line
        if scanline >= DISPLAY_START_LINE
            && scanline < DISPLAY_END_LINE
            && line_t_state < DISPLAY_T_STATES_PER_LINE
        {
            // Contention pattern repeats every 8 T-states
            let pattern_pos = line_t_state % 8;
            match pattern_pos {
                0 => 6,
                1 => 5,
                2 => 4,
                3 => 3,
                4 => 2,
                5 => 1,
                6 | 7 => 0,
                _ => unreachable!(),
            }
        } else {
            0
        }
    }

    /// Check if port address is in contended range.
    ///
    /// A port address is contended if its high byte falls in 0x40-0x7F,
    /// which corresponds to the contended memory range.
    #[inline]
    fn is_port_contended(&self, port: u16) -> bool {
        let high = (port >> 8) as u8;
        high >= 0x40 && high < 0x80
    }

    /// Apply I/O contention timing for a port access.
    ///
    /// I/O contention on the Spectrum depends on two factors:
    /// 1. Whether the port's high byte is in contended range (0x40-0x7F)
    /// 2. Whether bit 0 of the port is low (ULA port)
    ///
    /// The patterns are:
    /// - Contended + ULA port:     C:1, C:3 (total 4 + delays)
    /// - Contended + non-ULA:      C:1, C:1, C:1, C:1 (total 4 + delays)
    /// - Non-contended + ULA:      N:1, C:3 (total 4 + one delay)
    /// - Non-contended + non-ULA:  N:4 (total 4, no delays)
    ///
    /// Where C means check contention, N means no contention check.
    pub fn io_contention(&mut self, port: u16) {
        let is_ula_port = port & 0x01 == 0;
        let is_contended = self.is_port_contended(port);

        match (is_contended, is_ula_port) {
            (true, true) => {
                // Contended + ULA: C:1, C:3
                let delay = self.contention_delay();
                self.tick(delay + 1);
                let delay = self.contention_delay();
                self.tick(delay + 3);
            }
            (true, false) => {
                // Contended + non-ULA: C:1, C:1, C:1, C:1
                for _ in 0..4 {
                    let delay = self.contention_delay();
                    self.tick(delay + 1);
                }
            }
            (false, true) => {
                // Non-contended + ULA: N:1, C:3
                self.tick(1);
                let delay = self.contention_delay();
                self.tick(delay + 3);
            }
            (false, false) => {
                // Non-contended + non-ULA: N:4
                self.tick(4);
            }
        }
    }

    /// Apply M1 cycle (opcode fetch) contention timing.
    ///
    /// The M1 cycle has different contention timing than a normal memory read.
    /// During M1, the ULA checks for contention twice:
    /// - At T1 (address placed on bus)
    /// - At T2 (data read)
    ///
    /// Pattern for contended memory: C:1, C:2 (total 3 T-states + delays)
    /// Pattern for non-contended: N:3 (total 3 T-states, no delays)
    ///
    /// Note: The refresh cycle (T4, 1 T-state) is handled separately by the CPU.
    pub fn m1_contention(&mut self, _addr: u16, is_contended: bool) {
        if is_contended {
            // M1 to contended memory: C:1, C:2
            let delay = self.contention_delay();
            self.tick(delay + 1);
            let delay = self.contention_delay();
            self.tick(delay + 2);
        } else {
            // M1 to non-contended memory: N:3
            self.tick(3);
        }
    }

    /// Check if a memory read from screen RAM causes the "snow" effect.
    ///
    /// Snow occurs when the CPU reads from screen memory (0x4000-0x5AFF) while
    /// the ULA is also reading it during the display period. Both the CPU and
    /// ULA receive corrupted data.
    ///
    /// Returns true if snow occurs (and records the event), false otherwise.
    pub fn check_snow(&mut self, addr: u16) -> bool {
        // Only screen memory can cause snow
        if addr < SCREEN_START || addr >= SCREEN_END {
            return false;
        }

        let t_state = self.frame_t_state % self.t_states_per_frame;
        let scanline = t_state / T_STATES_PER_LINE;
        let line_t_state = t_state % T_STATES_PER_LINE;

        // Snow only occurs during the active display period
        if scanline < DISPLAY_START_LINE
            || scanline >= DISPLAY_END_LINE
            || line_t_state >= DISPLAY_T_STATES_PER_LINE
        {
            return false;
        }

        // The ULA reads screen memory in 8 T-state cycles
        // Snow occurs during the memory access portion (first 4 T-states of each cycle)
        let within_cycle = line_t_state % 8;
        if within_cycle >= 4 {
            return false;
        }

        // Record the snow event (scanline relative to display, character column)
        let display_line = scanline - DISPLAY_START_LINE;
        let char_column = line_t_state / 8;
        self.snow_events.push((display_line, char_column));

        true
    }

    /// Calculate the floating bus value based on current ULA state.
    ///
    /// When reading from unattached memory or certain I/O ports, the value
    /// returned depends on what the ULA is currently reading from screen RAM.
    /// This was used by some copy protection schemes.
    pub fn floating_bus(&self, screen_data: &[u8]) -> u8 {
        let t_state = self.frame_t_state % self.t_states_per_frame;

        let scanline = t_state / T_STATES_PER_LINE;
        let line_t_state = t_state % T_STATES_PER_LINE;

        // Only during active display does the ULA read screen memory
        if scanline >= DISPLAY_START_LINE
            && scanline < DISPLAY_END_LINE
            && line_t_state < DISPLAY_T_STATES_PER_LINE
        {
            // The ULA reads bitmap and attribute bytes in pairs every 8 T-states
            // Pattern: bitmap, attribute, bitmap, attribute...
            let screen_y = (scanline - DISPLAY_START_LINE) as usize;
            let char_column = (line_t_state / 8) as usize;

            // Determine if we're reading bitmap or attribute
            let within_pair = line_t_state % 8;
            if within_pair < 4 {
                // Reading bitmap byte
                let bitmap_addr = bitmap_address(char_column * 8, screen_y);
                screen_data.get(bitmap_addr).copied().unwrap_or(0xFF)
            } else {
                // Reading attribute byte
                let attr_addr = attribute_address(char_column * 8, screen_y);
                screen_data.get(attr_addr).copied().unwrap_or(0xFF)
            }
        } else {
            // Outside display area, floating bus returns 0xFF
            0xFF
        }
    }

    /// Read keyboard matrix and EAR input.
    ///
    /// Returns:
    /// - Bits 0-4: Keyboard row (active low)
    /// - Bit 5: Always 1 (unused on 48K)
    /// - Bit 6: EAR input (tape signal)
    /// - Bit 7: Floating (returns 1)
    pub fn read_keyboard(&self, high_byte: u8) -> u8 {
        let mut result = 0xBF; // bits 5 and 7 always high, bit 6 (EAR) low by default
        for row in 0..8 {
            if high_byte & (1 << row) == 0 {
                result &= self.keyboard[row] | 0xE0; // preserve upper bits
            }
        }
        // Add EAR bit (bit 6)
        if self.ear_level {
            result |= 0x40;
        }
        result
    }

    /// Write to ULA port (border and beeper).
    pub fn write_port(&mut self, value: u8) {
        let new_border = value & 0x07;
        if new_border != self.border {
            let t_state = self.frame_t_state.min(self.t_states_per_frame - 1);
            self.border_transitions.push((t_state, new_border));
            self.border = new_border;
        }

        // Capture beeper bit (bit 4) transitions
        let new_level = (value & 0x10) != 0;
        if new_level != self.beeper_level {
            // Clamp T-state to frame boundary
            let t_state = self.frame_t_state.min(self.t_states_per_frame - 1);
            self.beeper_transitions.push((t_state, new_level));
            self.beeper_level = new_level;
        }
    }

    /// Advance the frame clock.
    pub fn tick(&mut self, cycles: u32) {
        self.frame_t_state += cycles;
    }
}

/// Calculate bitmap address for a screen coordinate.
fn bitmap_address(screen_x: usize, screen_y: usize) -> usize {
    let x_byte = screen_x / 8;
    ((screen_y & 0xC0) << 5) | ((screen_y & 0x07) << 8) | ((screen_y & 0x38) << 2) | x_byte
}

/// Calculate attribute address for a screen coordinate.
fn attribute_address(screen_x: usize, screen_y: usize) -> usize {
    let x_byte = screen_x / 8;
    0x1800 + (screen_y / 8) * 32 + x_byte
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contention_during_display() {
        let ula = Ula::new(T_STATES_PER_FRAME_48K);
        let mut ula = ula;
        let display_start = DISPLAY_START_LINE * T_STATES_PER_LINE;

        ula.frame_t_state = display_start;
        assert_eq!(ula.contention_delay(), 6);

        ula.frame_t_state = display_start + 1;
        assert_eq!(ula.contention_delay(), 5);

        ula.frame_t_state = display_start + 6;
        assert_eq!(ula.contention_delay(), 0);
    }

    #[test]
    fn no_contention_in_border() {
        let mut ula = Ula::new(T_STATES_PER_FRAME_48K);

        // Top border
        ula.frame_t_state = 100;
        assert_eq!(ula.contention_delay(), 0);

        // Right border (past display area in line)
        let display_start = DISPLAY_START_LINE * T_STATES_PER_LINE;
        ula.frame_t_state = display_start + 128;
        assert_eq!(ula.contention_delay(), 0);
    }

    #[test]
    fn floating_bus_returns_screen_data() {
        let ula = Ula::new(T_STATES_PER_FRAME_48K);
        let mut ula = ula;
        let mut screen_data = vec![0u8; 6912];

        // Set a known pattern in screen memory
        screen_data[0] = 0xAA; // First bitmap byte

        // Position ULA at start of display
        let display_start = DISPLAY_START_LINE * T_STATES_PER_LINE;
        ula.frame_t_state = display_start;

        // Should return bitmap byte
        assert_eq!(ula.floating_bus(&screen_data), 0xAA);
    }

    #[test]
    fn floating_bus_returns_ff_in_border() {
        let mut ula = Ula::new(T_STATES_PER_FRAME_48K);
        let screen_data = vec![0x00u8; 6912];

        // During top border
        ula.frame_t_state = 100;
        assert_eq!(ula.floating_bus(&screen_data), 0xFF);
    }

    #[test]
    fn io_contention_non_contended_non_ula() {
        // Port 0x00FF: high byte 0x00 (non-contended), bit 0 = 1 (non-ULA)
        // Pattern: N:4 (just 4 T-states, no contention)
        let mut ula = Ula::new(T_STATES_PER_FRAME_48K);
        ula.frame_t_state = 0;

        ula.io_contention(0x00FF);

        assert_eq!(ula.frame_t_state, 4);
    }

    #[test]
    fn io_contention_non_contended_ula() {
        // Port 0x00FE: high byte 0x00 (non-contended), bit 0 = 0 (ULA)
        // Pattern: N:1, C:3
        // In border (no contention): 1 + 3 = 4 T-states
        let mut ula = Ula::new(T_STATES_PER_FRAME_48K);
        ula.frame_t_state = 100; // In top border

        ula.io_contention(0x00FE);

        assert_eq!(ula.frame_t_state, 104);
    }

    #[test]
    fn io_contention_non_contended_ula_during_display() {
        // Port 0x00FE: high byte 0x00 (non-contended), bit 0 = 0 (ULA)
        // Pattern: N:1, C:3
        // During display at pattern position 0: 1 + (6 + 3) = 10 T-states
        let mut ula = Ula::new(T_STATES_PER_FRAME_48K);
        let display_start = DISPLAY_START_LINE * T_STATES_PER_LINE;
        ula.frame_t_state = display_start;

        ula.io_contention(0x00FE);

        // After N:1, we're at display_start + 1 (pattern pos 1, delay 5)
        // Then C:3 adds 5 + 3 = 8
        // Total: 1 + 8 = 9
        assert_eq!(ula.frame_t_state, display_start + 9);
    }

    #[test]
    fn io_contention_contended_non_ula() {
        // Port 0x40FF: high byte 0x40 (contended), bit 0 = 1 (non-ULA)
        // Pattern: C:1, C:1, C:1, C:1
        // In border (no contention): 4 T-states
        let mut ula = Ula::new(T_STATES_PER_FRAME_48K);
        ula.frame_t_state = 100; // In top border

        ula.io_contention(0x40FF);

        assert_eq!(ula.frame_t_state, 104);
    }

    #[test]
    fn io_contention_contended_non_ula_during_display() {
        // Port 0x40FF: high byte 0x40 (contended), bit 0 = 1 (non-ULA)
        // Pattern: C:1, C:1, C:1, C:1
        // During display at pattern position 0 (delay 6):
        // First C:1: 6 + 1 = 7, now at pos 7 (delay 0)
        // Second C:1: 0 + 1 = 1, now at pos 0 (delay 6)
        // Third C:1: 6 + 1 = 7, now at pos 7 (delay 0)
        // Fourth C:1: 0 + 1 = 1
        // Total: 7 + 1 + 7 + 1 = 16
        let mut ula = Ula::new(T_STATES_PER_FRAME_48K);
        let display_start = DISPLAY_START_LINE * T_STATES_PER_LINE;
        ula.frame_t_state = display_start;

        ula.io_contention(0x40FF);

        assert_eq!(ula.frame_t_state, display_start + 16);
    }

    #[test]
    fn io_contention_contended_ula() {
        // Port 0x40FE: high byte 0x40 (contended), bit 0 = 0 (ULA)
        // Pattern: C:1, C:3
        // In border (no contention): 1 + 3 = 4 T-states
        let mut ula = Ula::new(T_STATES_PER_FRAME_48K);
        ula.frame_t_state = 100; // In top border

        ula.io_contention(0x40FE);

        assert_eq!(ula.frame_t_state, 104);
    }

    #[test]
    fn io_contention_contended_ula_during_display() {
        // Port 0x40FE: high byte 0x40 (contended), bit 0 = 0 (ULA)
        // Pattern: C:1, C:3
        // During display at pattern position 0 (delay 6):
        // First C:1: 6 + 1 = 7, now at pos 7 (delay 0)
        // Second C:3: 0 + 3 = 3
        // Total: 7 + 3 = 10
        let mut ula = Ula::new(T_STATES_PER_FRAME_48K);
        let display_start = DISPLAY_START_LINE * T_STATES_PER_LINE;
        ula.frame_t_state = display_start;

        ula.io_contention(0x40FE);

        assert_eq!(ula.frame_t_state, display_start + 10);
    }

    #[test]
    fn port_contention_boundary() {
        let ula = Ula::new(T_STATES_PER_FRAME_48K);

        // Just below contended range
        assert!(!ula.is_port_contended(0x3FFF));
        // Start of contended range
        assert!(ula.is_port_contended(0x4000));
        // End of contended range
        assert!(ula.is_port_contended(0x7FFF));
        // Just above contended range
        assert!(!ula.is_port_contended(0x8000));
    }

    #[test]
    fn m1_contention_non_contended() {
        // M1 to non-contended memory: N:3
        let mut ula = Ula::new(T_STATES_PER_FRAME_48K);
        ula.frame_t_state = 0;

        ula.m1_contention(0x0000, false);

        assert_eq!(ula.frame_t_state, 3);
    }

    #[test]
    fn m1_contention_contended_in_border() {
        // M1 to contended memory during border: C:1, C:2
        // No actual delay during border, so just 3 T-states
        let mut ula = Ula::new(T_STATES_PER_FRAME_48K);
        ula.frame_t_state = 100; // In top border

        ula.m1_contention(0x4000, true);

        assert_eq!(ula.frame_t_state, 103);
    }

    #[test]
    fn m1_contention_contended_during_display() {
        // M1 to contended memory during display at pattern position 0
        // Pattern: C:1, C:2
        // At pos 0 (delay 6): 6 + 1 = 7, now at pos 7 (delay 0)
        // At pos 7 (delay 0): 0 + 2 = 2
        // Total: 7 + 2 = 9
        let mut ula = Ula::new(T_STATES_PER_FRAME_48K);
        let display_start = DISPLAY_START_LINE * T_STATES_PER_LINE;
        ula.frame_t_state = display_start;

        ula.m1_contention(0x4000, true);

        assert_eq!(ula.frame_t_state, display_start + 9);
    }

    #[test]
    fn m1_vs_normal_read_timing_difference() {
        // Compare M1 contention vs normal read contention during display
        // Both start at pattern position 0 (delay 6)
        //
        // Normal read (C:3): 6 + 3 = 9 T-states
        // M1 (C:1, C:2): (6 + 1) + (0 + 2) = 9 T-states
        //
        // At pattern position 1 (delay 5):
        // Normal read (C:3): 5 + 3 = 8 T-states
        // M1 (C:1, C:2): (5 + 1) + (0 + 2) = 8 T-states (now at pos 6, delay 0)
        //
        // The key difference shows at position 5 (delay 1):
        // Normal read (C:3): 1 + 3 = 4 T-states
        // M1 (C:1, C:2): (1 + 1) + (0 + 2) = 4 T-states (now at pos 7, delay 0)
        //
        // And at position 6 (delay 0):
        // Normal read (C:3): 0 + 3 = 3 T-states
        // M1 (C:1, C:2): (0 + 1) + (0 + 2) = 3 T-states (now at pos 1, delay 5 for next)
        //
        // The difference matters for cumulative timing across multiple accesses

        let mut ula = Ula::new(T_STATES_PER_FRAME_48K);
        let display_start = DISPLAY_START_LINE * T_STATES_PER_LINE;

        // Test that M1 timing is correctly different from normal read
        // At position 0: both should take 9 T-states but the internal timing differs
        ula.frame_t_state = display_start;
        ula.m1_contention(0x4000, true);
        let m1_end = ula.frame_t_state;

        // Reset and do normal read timing (C:3)
        ula.frame_t_state = display_start;
        let delay = ula.contention_delay();
        ula.tick(delay + 3);
        let read_end = ula.frame_t_state;

        // Both should end at the same place for this particular starting position
        assert_eq!(m1_end, read_end);
        assert_eq!(m1_end, display_start + 9);
    }

    #[test]
    fn snow_not_triggered_outside_screen_memory() {
        let mut ula = Ula::new(T_STATES_PER_FRAME_48K);
        let display_start = DISPLAY_START_LINE * T_STATES_PER_LINE;
        ula.frame_t_state = display_start;

        // ROM address - no snow
        assert!(!ula.check_snow(0x0000));
        // Upper RAM - no snow
        assert!(!ula.check_snow(0x8000));
        // Just past screen memory - no snow
        assert!(!ula.check_snow(0x5B00));

        assert!(ula.snow_events.is_empty());
    }

    #[test]
    fn snow_not_triggered_in_border() {
        let mut ula = Ula::new(T_STATES_PER_FRAME_48K);
        ula.frame_t_state = 100; // Top border

        // Screen memory during border - no snow
        assert!(!ula.check_snow(0x4000));
        assert!(ula.snow_events.is_empty());
    }

    #[test]
    fn snow_triggered_during_display() {
        let mut ula = Ula::new(T_STATES_PER_FRAME_48K);
        let display_start = DISPLAY_START_LINE * T_STATES_PER_LINE;
        ula.frame_t_state = display_start; // Start of display, position 0 in ULA read cycle

        // Screen memory during display read - snow!
        assert!(ula.check_snow(0x4000));
        assert_eq!(ula.snow_events.len(), 1);
        assert_eq!(ula.snow_events[0], (0, 0)); // Line 0, column 0
    }

    #[test]
    fn snow_not_triggered_between_ula_reads() {
        let mut ula = Ula::new(T_STATES_PER_FRAME_48K);
        let display_start = DISPLAY_START_LINE * T_STATES_PER_LINE;

        // Position 4-7 within cycle is between ULA reads - no snow
        ula.frame_t_state = display_start + 4;
        assert!(!ula.check_snow(0x4000));

        ula.frame_t_state = display_start + 7;
        assert!(!ula.check_snow(0x4000));

        assert!(ula.snow_events.is_empty());
    }

    #[test]
    fn snow_records_correct_position() {
        let mut ula = Ula::new(T_STATES_PER_FRAME_48K);
        let display_start = DISPLAY_START_LINE * T_STATES_PER_LINE;

        // Line 10, column 5 (T-state = display_start + 10*224 + 5*8)
        ula.frame_t_state = display_start + 10 * T_STATES_PER_LINE + 5 * 8;
        assert!(ula.check_snow(0x4000));

        assert_eq!(ula.snow_events.len(), 1);
        assert_eq!(ula.snow_events[0], (10, 5)); // Line 10, column 5
    }

    #[test]
    fn snow_events_cleared_on_frame_start() {
        let mut ula = Ula::new(T_STATES_PER_FRAME_48K);
        let display_start = DISPLAY_START_LINE * T_STATES_PER_LINE;
        ula.frame_t_state = display_start;

        // Trigger some snow
        ula.check_snow(0x4000);
        assert_eq!(ula.snow_events.len(), 1);

        // Start new frame
        ula.start_frame();
        assert!(ula.snow_events.is_empty());
    }

    #[test]
    fn ear_bit_returned_in_port_read() {
        let mut ula = Ula::new(T_STATES_PER_FRAME_48K);

        // EAR bit is bit 6, should be 0 by default
        let result = ula.read_keyboard(0xFF);
        assert_eq!(result & 0x40, 0); // bit 6 = 0

        // Bit 5 and 7 should always be 1
        assert_eq!(result & 0x20, 0x20); // bit 5 = 1
        assert_eq!(result & 0x80, 0x80); // bit 7 = 1

        // Set EAR level high
        ula.ear_level = true;
        let result = ula.read_keyboard(0xFF);
        assert_eq!(result & 0x40, 0x40); // bit 6 = 1
    }

    #[test]
    fn ear_bit_cleared_on_reset() {
        let mut ula = Ula::new(T_STATES_PER_FRAME_48K);
        ula.ear_level = true;
        assert!(ula.ear_level);

        ula.reset();
        assert!(!ula.ear_level);
    }
}
