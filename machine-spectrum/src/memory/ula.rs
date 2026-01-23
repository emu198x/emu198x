//! ULA (Uncommitted Logic Array) emulation.
//!
//! The ULA handles video generation, keyboard scanning, border color,
//! and beeper output. It also implements contention timing and floating bus.

/// T-states per frame for 48K Spectrum (and 16K).
pub const T_STATES_PER_FRAME_48K: u32 = 69888;

/// T-states per scanline.
const T_STATES_PER_LINE: u32 = 224;

/// First scanline of the display area.
const DISPLAY_START_LINE: u32 = 64;

/// Last scanline of the display area (exclusive).
const DISPLAY_END_LINE: u32 = 256;

/// T-states of active display per line.
const DISPLAY_T_STATES_PER_LINE: u32 = 128;

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
    }

    /// Start a new frame.
    pub fn start_frame(&mut self) {
        self.frame_t_state = 0;
        self.border_transitions.clear();
        self.border_transitions.push((0, self.border)); // Record initial color
        self.beeper_transitions.clear();
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

    /// Read keyboard matrix.
    pub fn read_keyboard(&self, high_byte: u8) -> u8 {
        let mut result = 0x1F; // bits 0-4, active low
        for row in 0..8 {
            if high_byte & (1 << row) == 0 {
                result &= self.keyboard[row];
            }
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
}
