//! Motorola 6845 CRT Controller (CRTC).
//!
//! Generates timing signals and memory addresses for CRT display systems.
//! The 6845 does not generate pixels — it provides addresses (MA0-MA13)
//! and raster line counts (RA0-RA4) for external pixel generation hardware.
//!
//! Used by the BBC Micro, Amstrad CPC, later Commodore PET models,
//! and the Commodore 128's VDC 8563.
//!
//! 18 registers (R0-R17): horizontal/vertical timing, sync positions,
//! display start address, cursor position, and light pen latch.

/// Motorola 6845 CRTC.
pub struct Crtc6845 {
    /// Selected register number (0-17).
    selected: u8,
    /// Registers R0-R17.
    regs: [u8; 18],

    // Counters
    /// Horizontal character counter (0 to R0).
    h_counter: u8,
    /// Raster counter (scanline within character row, 0 to R9).
    ra: u8,
    /// Vertical character row counter (0 to R4).
    v_counter: u8,
    /// Vertical total adjust counter (0 to R5).
    v_adjust: u8,
    /// Whether we're in the vertical adjust period.
    in_v_adjust: bool,

    // Memory address
    /// Memory address counter (14-bit, MA0-MA13).
    ma: u16,
    /// Row start address (latched at the beginning of each character row).
    row_start: u16,

    // Sync and display enable outputs
    /// Horizontal sync active.
    pub hsync: bool,
    /// Vertical sync active.
    pub vsync: bool,
    /// Display enable (active during visible area).
    pub display_enable: bool,

    // Sync counters
    hsync_counter: u8,
    vsync_counter: u8,

    /// Cursor address (R14:R15).
    pub cursor_active: bool,
}

impl Crtc6845 {
    /// Create a new CRTC with default register values.
    #[must_use]
    pub fn new() -> Self {
        Self {
            selected: 0,
            regs: [0; 18],
            h_counter: 0,
            ra: 0,
            v_counter: 0,
            v_adjust: 0,
            in_v_adjust: false,
            ma: 0,
            row_start: 0,
            hsync: false,
            vsync: false,
            display_enable: false,
            hsync_counter: 0,
            vsync_counter: 0,
            cursor_active: false,
        }
    }

    /// Write the address register (selects which register to access).
    pub fn write_address(&mut self, value: u8) {
        self.selected = value & 0x1F;
    }

    /// Write the data register (writes to the currently selected register).
    pub fn write_data(&mut self, value: u8) {
        let reg = self.selected as usize;
        if reg < 18 {
            // R0-R13 are write-only, R14-R15 are R/W
            self.regs[reg] = value;
        }
    }

    /// Read the data register (R14-R17 are readable).
    #[must_use]
    pub fn read_data(&self) -> u8 {
        let reg = self.selected as usize;
        match reg {
            14..=17 => self.regs[reg],
            _ => 0, // Write-only registers return 0
        }
    }

    /// Current memory address output (MA0-MA13, 14-bit).
    #[must_use]
    pub fn memory_address(&self) -> u16 {
        self.ma & 0x3FFF
    }

    /// Current raster address (RA0-RA4, 5-bit).
    #[must_use]
    pub fn raster_address(&self) -> u8 {
        self.ra
    }

    /// Display start address (R12:R13).
    #[must_use]
    pub fn start_address(&self) -> u16 {
        (u16::from(self.regs[12] & 0x3F) << 8) | u16::from(self.regs[13])
    }

    /// Cursor address (R14:R15).
    #[must_use]
    pub fn cursor_address(&self) -> u16 {
        (u16::from(self.regs[14] & 0x3F) << 8) | u16::from(self.regs[15])
    }

    /// Register values (for observation).
    #[must_use]
    pub fn regs(&self) -> &[u8; 18] {
        &self.regs
    }

    /// Maximum scanline per character row (R9).
    #[must_use]
    pub fn max_scanline(&self) -> u8 {
        self.regs[9] & 0x1F
    }

    /// Tick one character clock. Call at the CRTC clock rate (1 or 2 MHz
    /// depending on mode). Returns true at the start of a new frame.
    pub fn tick(&mut self) -> bool {
        let mut new_frame = false;

        // Horizontal counter
        let h_total = self.regs[0];
        let h_displayed = self.regs[1];
        let h_sync_pos = self.regs[2];
        let h_sync_width = self.regs[3] & 0x0F;

        // Display enable: active when both H and V are in displayed area
        let h_visible = self.h_counter < h_displayed;
        let v_visible = self.v_counter < self.regs[6];
        self.display_enable = h_visible && v_visible && !self.in_v_adjust;

        // Update memory address during visible area
        if self.display_enable {
            self.ma = self.ma.wrapping_add(1) & 0x3FFF;
        }

        // Cursor detection
        self.cursor_active = self.display_enable && self.ma == self.cursor_address();

        // HSYNC generation
        if self.h_counter == h_sync_pos {
            self.hsync = true;
            self.hsync_counter = 0;
        }
        if self.hsync {
            self.hsync_counter += 1;
            let width = if h_sync_width == 0 { 16 } else { h_sync_width };
            if self.hsync_counter >= width {
                self.hsync = false;
            }
        }

        // Advance horizontal counter
        self.h_counter += 1;
        if self.h_counter > h_total {
            // End of line
            self.h_counter = 0;
            new_frame = self.advance_vertical();
        }

        new_frame
    }

    /// Advance vertical counters at end of each horizontal line.
    /// Returns true at frame start.
    fn advance_vertical(&mut self) -> bool {
        let max_scan = self.regs[9] & 0x1F;
        let v_total = self.regs[4] & 0x7F;
        let v_adjust = self.regs[5] & 0x1F;
        let v_sync_pos = self.regs[7] & 0x7F;
        let v_sync_width = (self.regs[3] >> 4) & 0x0F;

        if self.in_v_adjust {
            self.v_adjust += 1;
            if self.v_adjust >= v_adjust {
                // Frame complete — restart
                self.in_v_adjust = false;
                self.v_counter = 0;
                self.ra = 0;
                self.ma = self.start_address();
                self.row_start = self.ma;
                return true;
            }
            return false;
        }

        // Advance raster counter
        if self.ra >= max_scan {
            // End of character row
            self.ra = 0;

            // VSYNC generation
            if self.v_counter == v_sync_pos {
                self.vsync = true;
                self.vsync_counter = 0;
            }

            self.v_counter += 1;

            if self.v_counter > v_total {
                // Start vertical adjust period
                if v_adjust > 0 {
                    self.in_v_adjust = true;
                    self.v_adjust = 0;
                } else {
                    // No adjust — restart immediately
                    self.v_counter = 0;
                    self.ra = 0;
                    self.ma = self.start_address();
                    self.row_start = self.ma;
                    return true;
                }
            }

            // Latch row start address for next row
            self.row_start = self.ma;
        } else {
            self.ra += 1;
            // Restart MA from the beginning of this character row
            self.ma = self.row_start;
        }

        // VSYNC width
        if self.vsync {
            self.vsync_counter += 1;
            let width = if v_sync_width == 0 { 16 } else { v_sync_width };
            if self.vsync_counter >= width {
                self.vsync = false;
            }
        }

        false
    }
}

impl Default for Crtc6845 {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_mode0(crtc: &mut Crtc6845) {
        // BBC Micro MODE 0: 80-column, 2 MHz CRTC clock
        let vals = [
            127, 80, 98, 0x28, 38, 0, 32, 34,
            0, 7, 0, 0, 0x0C, 0x00, 0, 0, 0, 0,
        ];
        for (i, &v) in vals.iter().enumerate() {
            crtc.write_address(i as u8);
            crtc.write_data(v);
        }
    }

    #[test]
    fn register_write_and_read() {
        let mut crtc = Crtc6845::new();
        // R14 (cursor high) is read/write
        crtc.write_address(14);
        crtc.write_data(0x12);
        assert_eq!(crtc.read_data(), 0x12);
    }

    #[test]
    fn write_only_registers_return_zero() {
        let mut crtc = Crtc6845::new();
        crtc.write_address(0);
        crtc.write_data(127);
        assert_eq!(crtc.read_data(), 0); // R0 is write-only
    }

    #[test]
    fn start_address_from_r12_r13() {
        let mut crtc = Crtc6845::new();
        crtc.write_address(12);
        crtc.write_data(0x0C);
        crtc.write_address(13);
        crtc.write_data(0x00);
        assert_eq!(crtc.start_address(), 0x0C00);
    }

    #[test]
    fn horizontal_counter_wraps_at_r0() {
        let mut crtc = Crtc6845::new();
        setup_mode0(&mut crtc);
        // Tick 128 times (R0 = 127, wraps at 128)
        for _ in 0..128 {
            crtc.tick();
        }
        assert_eq!(crtc.h_counter, 0);
    }

    #[test]
    fn display_enable_during_visible() {
        let mut crtc = Crtc6845::new();
        setup_mode0(&mut crtc);
        // First tick should be in visible area (h=0, v=0)
        crtc.tick();
        assert!(crtc.display_enable);
    }

    #[test]
    fn display_enable_off_during_hblank() {
        let mut crtc = Crtc6845::new();
        setup_mode0(&mut crtc);
        // Tick past R1 (80 displayed chars)
        for _ in 0..81 {
            crtc.tick();
        }
        assert!(!crtc.display_enable);
    }

    #[test]
    fn frame_completes() {
        let mut crtc = Crtc6845::new();
        setup_mode0(&mut crtc);
        // MODE 0: 128 chars/line × 39 rows × 8 scanlines = 39936 ticks
        let mut frame_done = false;
        for _ in 0..40000 {
            if crtc.tick() {
                frame_done = true;
                break;
            }
        }
        assert!(frame_done, "frame should complete within 40000 ticks");
    }

    #[test]
    fn memory_address_increments_during_display() {
        let mut crtc = Crtc6845::new();
        setup_mode0(&mut crtc);
        // Run one full frame to load start address into MA
        loop {
            if crtc.tick() { break; }
        }
        let ma_start = crtc.memory_address();
        // Tick a few visible characters
        for _ in 0..10 {
            crtc.tick();
        }
        // MA should have advanced from the start
        assert!(crtc.memory_address() > ma_start);
    }
}
