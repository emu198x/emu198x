//! VDC 8563/8568 (Video Display Controller) emulation.
//!
//! The VDC provides 80-column text display for the C128. It has its own
//! dedicated video RAM (16K or 64K) and operates independently of the VIC-II.
//!
//! # Register Access
//!
//! The VDC is accessed through two memory-mapped addresses:
//! - `$D600` (write): Select internal register (0-36)
//! - `$D600` (read): Status register
//!   - Bit 7: Ready (1 = VDC ready for data transfer)
//!   - Bit 6: Light pen strobe
//!   - Bit 5: Vertical blank
//! - `$D601`: Data register - read/write selected register
//!
//! # Video RAM Access
//!
//! Video RAM is accessed through registers 18-19 (address) and 31 (data):
//! 1. Write high byte of address to R18
//! 2. Write low byte of address to R19
//! 3. Read/write data through R31 (auto-increment by R27)
//!
//! # Display Modes
//!
//! - 80x25 text (default)
//! - 80x50 text (interlaced)
//! - 640x200 monochrome graphics
//! - 640x400 interlaced graphics

/// VDC chip variant.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VdcRevision {
    /// Original 8563 (16K VRAM on early C128)
    Vdc8563,
    /// Revised 8568 (64K VRAM on C128DCR)
    Vdc8568,
}

/// VDC 8563/8568 state.
#[derive(Clone)]
pub struct Vdc {
    /// VDC revision (determines RAM size)
    revision: VdcRevision,
    /// Currently selected register (0-36)
    register_select: u8,
    /// Internal registers (37 total)
    registers: [u8; 37],
    /// Video RAM (16K or 64K)
    vram: Vec<u8>,
    /// Ready flag (for status register bit 7)
    ready: bool,
    /// Vertical blank flag
    vblank: bool,
    /// Light pen strobe
    light_pen: bool,
    /// Current scanline
    scanline: u16,
    /// Frame counter
    frame: u32,
    /// Cycles until ready after register write
    ready_countdown: u8,
}

impl Default for Vdc {
    fn default() -> Self {
        Self::new(VdcRevision::Vdc8568)
    }
}

impl Vdc {
    /// Create a new VDC with specified revision.
    pub fn new(revision: VdcRevision) -> Self {
        let vram_size = match revision {
            VdcRevision::Vdc8563 => 16 * 1024,
            VdcRevision::Vdc8568 => 64 * 1024,
        };

        let mut vdc = Self {
            revision,
            register_select: 0,
            registers: [0; 37],
            vram: vec![0; vram_size],
            ready: true,
            vblank: false,
            light_pen: false,
            scanline: 0,
            frame: 0,
            ready_countdown: 0,
        };

        // Initialize registers to power-on defaults
        vdc.reset();
        vdc
    }

    /// Reset VDC to power-on state.
    pub fn reset(&mut self) {
        // Default register values for 80x25 text mode
        self.registers[0] = 0x7E; // Horizontal total (126+1 = 127)
        self.registers[1] = 0x50; // Horizontal displayed (80)
        self.registers[2] = 0x66; // Horizontal sync position
        self.registers[3] = 0x49; // Sync width (horizontal/vertical)
        self.registers[4] = 0x20; // Vertical total (32+1 = 33 character rows)
        self.registers[5] = 0x00; // Vertical adjust
        self.registers[6] = 0x19; // Vertical displayed (25 rows)
        self.registers[7] = 0x1D; // Vertical sync position
        self.registers[8] = 0x00; // Interlace mode (non-interlaced)
        self.registers[9] = 0x07; // Character total vertical (8 scanlines)
        self.registers[10] = 0x20; // Cursor start (enabled, line 0)
        self.registers[11] = 0x07; // Cursor end (line 7)
        self.registers[12] = 0x00; // Display start address high
        self.registers[13] = 0x00; // Display start address low
        self.registers[14] = 0x00; // Cursor position high
        self.registers[15] = 0x00; // Cursor position low
        self.registers[16] = 0x00; // Light pen vertical
        self.registers[17] = 0x00; // Light pen horizontal
        self.registers[18] = 0x00; // Update address high
        self.registers[19] = 0x00; // Update address low
        self.registers[20] = 0x08; // Attribute start high (at $0800)
        self.registers[21] = 0x00; // Attribute start low
        self.registers[22] = 0x78; // Character horizontal size (120 = 8 pixels)
        self.registers[23] = 0x08; // Character vertical size (8 scanlines)
        self.registers[24] = 0x20; // Vertical smooth scroll
        self.registers[25] = 0x47; // Horizontal smooth scroll + display control
        self.registers[26] = 0xF0; // Foreground/background (white on black)
        self.registers[27] = 0x01; // Address increment
        self.registers[28] = 0x10; // Character set address ($1000)
        self.registers[29] = 0x07; // Underline position
        self.registers[30] = 0x00; // Word count
        self.registers[31] = 0x00; // Data register
        self.registers[32] = 0x00; // Block source high
        self.registers[33] = 0x00; // Block source low
        self.registers[34] = 0x7D; // Display enable begin
        self.registers[35] = 0x64; // Display enable end
        self.registers[36] = 0x05; // DRAM refresh rate

        self.register_select = 0;
        self.ready = true;
        self.vblank = false;
        self.scanline = 0;
        self.ready_countdown = 0;
    }

    /// Get VDC revision.
    pub fn revision(&self) -> VdcRevision {
        self.revision
    }

    /// Get VRAM size in bytes.
    pub fn vram_size(&self) -> usize {
        self.vram.len()
    }

    /// Read the status register ($D600).
    pub fn read_status(&self) -> u8 {
        let mut status = 0;
        if self.ready {
            status |= 0x80;
        }
        if self.light_pen {
            status |= 0x40;
        }
        if self.vblank {
            status |= 0x20;
        }
        status
    }

    /// Write the register select ($D600).
    pub fn write_address(&mut self, value: u8) {
        self.register_select = value & 0x3F; // Only 37 registers (0-36)
    }

    /// Read from the data register ($D601).
    pub fn read_data(&mut self) -> u8 {
        let reg = self.register_select as usize;

        match reg {
            // Registers 0-15 are readable
            0..=15 => self.registers[reg],

            // Light pen registers (16-17) are readable
            16 | 17 => self.registers[reg],

            // Update address (18-19) - readable
            18 | 19 => self.registers[reg],

            // Most registers 20-30 are readable
            20..=30 => self.registers[reg],

            // Register 31 reads from VRAM
            31 => {
                let addr = self.update_address();
                let value = if addr < self.vram.len() {
                    self.vram[addr]
                } else {
                    0
                };
                self.increment_update_address();
                // Reading VRAM takes time
                self.ready = false;
                self.ready_countdown = 8;
                value
            }

            // Registers 32-36 are readable
            32..=36 => self.registers[reg],

            _ => 0,
        }
    }

    /// Write to the data register ($D601).
    pub fn write_data(&mut self, value: u8) {
        let reg = self.register_select as usize;
        if reg >= self.registers.len() {
            return;
        }

        // Store the value
        self.registers[reg] = value;

        // Handle special register writes
        match reg {
            // Writing to register 30 triggers block copy/fill
            30 => {
                self.execute_block_operation();
            }

            // Writing to register 31 writes to VRAM
            31 => {
                let addr = self.update_address();
                if addr < self.vram.len() {
                    self.vram[addr] = value;
                }
                self.increment_update_address();
                // Writing VRAM takes time
                self.ready = false;
                self.ready_countdown = 8;
            }

            // Writing update address low (R19) may trigger ready delay
            18 | 19 => {
                self.ready = false;
                self.ready_countdown = 3;
            }

            _ => {}
        }
    }

    /// Get the current update address (R18:R19).
    fn update_address(&self) -> usize {
        let high = self.registers[18] as usize;
        let low = self.registers[19] as usize;
        (high << 8) | low
    }

    /// Increment the update address by the increment value (R27).
    fn increment_update_address(&mut self) {
        let addr = self.update_address();
        let increment = self.registers[27] as usize;
        let new_addr = addr.wrapping_add(increment);
        self.registers[18] = ((new_addr >> 8) & 0xFF) as u8;
        self.registers[19] = (new_addr & 0xFF) as u8;
    }

    /// Execute a block copy or fill operation.
    fn execute_block_operation(&mut self) {
        let word_count = self.registers[30] as usize;
        if word_count == 0 {
            return;
        }

        let dest = self.update_address();
        let source = ((self.registers[32] as usize) << 8) | (self.registers[33] as usize);

        // Bit 7 of R24 determines copy (0) vs fill (1)
        let is_fill = self.registers[24] & 0x80 != 0;

        if is_fill {
            // Fill: repeat R31 value
            let fill_value = self.registers[31];
            for i in 0..word_count {
                let addr = dest.wrapping_add(i) % self.vram.len();
                self.vram[addr] = fill_value;
            }
        } else {
            // Copy: copy from source to dest
            for i in 0..word_count {
                let src_addr = source.wrapping_add(i) % self.vram.len();
                let dst_addr = dest.wrapping_add(i) % self.vram.len();
                self.vram[dst_addr] = self.vram[src_addr];
            }
        }

        // Update the update address
        let new_addr = dest.wrapping_add(word_count);
        self.registers[18] = ((new_addr >> 8) & 0xFF) as u8;
        self.registers[19] = (new_addr & 0xFF) as u8;

        // Block operations take time
        self.ready = false;
        self.ready_countdown = (word_count as u8).max(20);
    }

    /// Tick the VDC for the given number of cycles.
    pub fn tick(&mut self, cycles: u32) {
        // Handle ready countdown
        if self.ready_countdown > 0 {
            self.ready_countdown = self.ready_countdown.saturating_sub(cycles as u8);
            if self.ready_countdown == 0 {
                self.ready = true;
            }
        }
    }

    /// Advance to the next scanline.
    pub fn next_scanline(&mut self) {
        self.scanline += 1;
        let total_lines = self.total_scanlines();

        if self.scanline >= total_lines {
            self.scanline = 0;
            self.frame += 1;
        }

        // Calculate vblank based on vertical displayed
        let char_height = (self.registers[9] & 0x1F) as u16 + 1;
        let displayed_rows = self.registers[6] as u16;
        let visible_lines = displayed_rows * char_height;

        self.vblank = self.scanline >= visible_lines;
    }

    /// Get total scanlines per frame.
    fn total_scanlines(&self) -> u16 {
        let char_height = (self.registers[9] & 0x1F) as u16 + 1;
        let total_rows = (self.registers[4] & 0x7F) as u16 + 1;
        let adjust = self.registers[5] as u16;
        total_rows * char_height + adjust
    }

    /// Get display start address.
    pub fn display_address(&self) -> u16 {
        ((self.registers[12] as u16) << 8) | (self.registers[13] as u16)
    }

    /// Get attribute start address.
    pub fn attribute_address(&self) -> u16 {
        ((self.registers[20] as u16) << 8) | (self.registers[21] as u16)
    }

    /// Get character set address.
    pub fn charset_address(&self) -> u16 {
        (self.registers[28] as u16) << 8
    }

    /// Get cursor position.
    pub fn cursor_position(&self) -> u16 {
        ((self.registers[14] as u16) << 8) | (self.registers[15] as u16)
    }

    /// Check if cursor is enabled.
    pub fn cursor_enabled(&self) -> bool {
        self.registers[10] & 0x60 != 0x20
    }

    /// Get cursor start and end scanlines.
    pub fn cursor_shape(&self) -> (u8, u8) {
        let start = self.registers[10] & 0x1F;
        let end = self.registers[11] & 0x1F;
        (start, end)
    }

    /// Get foreground color (0-15).
    pub fn foreground_color(&self) -> u8 {
        self.registers[26] >> 4
    }

    /// Get background color (0-15).
    pub fn background_color(&self) -> u8 {
        self.registers[26] & 0x0F
    }

    /// Get number of displayed columns (typically 80).
    pub fn columns(&self) -> u8 {
        self.registers[1]
    }

    /// Get number of displayed rows (typically 25).
    pub fn rows(&self) -> u8 {
        self.registers[6]
    }

    /// Get character height in scanlines.
    pub fn char_height(&self) -> u8 {
        (self.registers[9] & 0x1F) + 1
    }

    /// Check if graphics mode is enabled.
    pub fn graphics_mode(&self) -> bool {
        self.registers[25] & 0x80 != 0
    }

    /// Check if attribute mode is enabled.
    pub fn attribute_mode(&self) -> bool {
        self.registers[25] & 0x40 != 0
    }

    /// Check if semi-graphics mode is enabled.
    pub fn semi_graphics(&self) -> bool {
        self.registers[25] & 0x20 != 0
    }

    /// Read a byte directly from VRAM (for rendering).
    pub fn vram_read(&self, addr: u16) -> u8 {
        let addr = addr as usize;
        if addr < self.vram.len() {
            self.vram[addr]
        } else {
            0
        }
    }

    /// Write a byte directly to VRAM (for testing/init).
    pub fn vram_write(&mut self, addr: u16, value: u8) {
        let addr = addr as usize;
        if addr < self.vram.len() {
            self.vram[addr] = value;
        }
    }

    /// Render a scanline to the output buffer.
    ///
    /// Returns the number of pixels written (typically 640 for 80-column mode).
    /// Each pixel is a 4-bit color index (0-15).
    pub fn render_scanline(&self, line: u16, buffer: &mut [u8]) -> usize {
        let columns = self.columns() as usize;
        let char_height = self.char_height() as u16;

        // Calculate which character row this scanline belongs to
        let char_row = line / char_height;
        let char_line = (line % char_height) as u8;

        if char_row >= self.rows() as u16 {
            // Below visible area - fill with background
            let bg = self.background_color();
            for i in 0..(columns * 8) {
                if i < buffer.len() {
                    buffer[i] = bg;
                }
            }
            return columns * 8;
        }

        let display_addr = self.display_address() as usize;
        let attr_addr = self.attribute_address() as usize;
        let charset_addr = self.charset_address() as usize;
        let use_attrs = self.attribute_mode();

        let row_offset = char_row as usize * columns;

        for col in 0..columns {
            let char_addr = display_addr.wrapping_add(row_offset + col) % self.vram.len();
            let char_code = self.vram[char_addr] as usize;

            // Get character bitmap from character set
            let bitmap_addr = charset_addr
                .wrapping_add(char_code * 16)
                .wrapping_add(char_line as usize)
                % self.vram.len();
            let bitmap = self.vram[bitmap_addr];

            // Get colors
            let (fg, bg) = if use_attrs {
                let attr_a = attr_addr.wrapping_add(row_offset + col) % self.vram.len();
                let attr = self.vram[attr_a];
                (attr >> 4, attr & 0x0F)
            } else {
                (self.foreground_color(), self.background_color())
            };

            // Render 8 pixels
            for bit in 0..8 {
                let pixel_on = bitmap & (0x80 >> bit) != 0;
                let color = if pixel_on { fg } else { bg };
                let pixel_idx = col * 8 + bit;
                if pixel_idx < buffer.len() {
                    buffer[pixel_idx] = color;
                }
            }
        }

        // Handle cursor
        if self.cursor_enabled() {
            let cursor_pos = self.cursor_position() as usize;
            let cursor_offset = cursor_pos.saturating_sub(display_addr);
            let cursor_row = cursor_offset / columns;
            let cursor_col = cursor_offset % columns;

            if cursor_row as u16 == char_row {
                let (start, end) = self.cursor_shape();
                if char_line >= start && char_line <= end {
                    // Cursor visible on this line - invert the character
                    let fg = self.foreground_color();
                    for bit in 0..8 {
                        let pixel_idx = cursor_col * 8 + bit;
                        if pixel_idx < buffer.len() {
                            buffer[pixel_idx] = fg;
                        }
                    }
                }
            }
        }

        columns * 8
    }

    /// Get current scanline.
    pub fn scanline(&self) -> u16 {
        self.scanline
    }

    /// Check if in vertical blank.
    pub fn in_vblank(&self) -> bool {
        self.vblank
    }

    /// Get current frame number.
    pub fn frame(&self) -> u32 {
        self.frame
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_vdc() {
        let vdc = Vdc::new(VdcRevision::Vdc8568);
        assert_eq!(vdc.vram_size(), 64 * 1024);
        assert!(vdc.ready);
    }

    #[test]
    fn test_status_register() {
        let vdc = Vdc::new(VdcRevision::Vdc8568);
        let status = vdc.read_status();
        assert!(status & 0x80 != 0); // Ready bit
    }

    #[test]
    fn test_vram_access() {
        let mut vdc = Vdc::new(VdcRevision::Vdc8568);

        // Set update address to $1000
        vdc.write_address(18);
        vdc.write_data(0x10);
        vdc.write_address(19);
        vdc.write_data(0x00);

        // Write a value
        vdc.write_address(31);
        vdc.write_data(0x42);

        // Read it back
        vdc.write_address(18);
        vdc.write_data(0x10);
        vdc.write_address(19);
        vdc.write_data(0x00);
        vdc.write_address(31);
        let value = vdc.read_data();

        assert_eq!(value, 0x42);
    }

    #[test]
    fn test_default_text_mode() {
        let vdc = Vdc::new(VdcRevision::Vdc8568);
        assert_eq!(vdc.columns(), 80);
        assert_eq!(vdc.rows(), 25);
        assert_eq!(vdc.char_height(), 8);
    }
}
