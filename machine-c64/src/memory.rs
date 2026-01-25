//! C64 memory model with ROM/RAM banking.
//!
//! The C64's memory map is controlled by the processor port at $00-$01:
//!
//! $01 bits:
//! - Bit 0 (LORAM): BASIC ROM at $A000-$BFFF (1 = ROM, 0 = RAM)
//! - Bit 1 (HIRAM): KERNAL ROM at $E000-$FFFF (1 = ROM, 0 = RAM)
//! - Bit 2 (CHAREN): Character ROM at $D000-$DFFF (0 = Char ROM, 1 = I/O)
//! - Bits 3-5: Cassette/other I/O
//!
//! Default value is $37 (all ROMs visible, I/O enabled).

use emu_core::Bus;

/// C64 memory subsystem.
pub struct Memory {
    /// 64KB RAM
    pub(crate) ram: [u8; 65536],
    /// 8KB BASIC ROM ($A000-$BFFF)
    basic_rom: [u8; 8192],
    /// 8KB KERNAL ROM ($E000-$FFFF)
    kernal_rom: [u8; 8192],
    /// 4KB Character ROM ($D000-$DFFF when CHAREN=0)
    char_rom: [u8; 4096],
    /// Processor port data direction ($00)
    port_ddr: u8,
    /// Processor port data ($01)
    port_data: u8,
    /// VIC-II registers (directly accessed by Memory)
    pub(crate) vic_registers: [u8; 64],
    /// SID registers
    pub(crate) sid_registers: [u8; 32],
    /// CIA1 registers
    pub(crate) cia1: Cia,
    /// CIA2 registers
    pub(crate) cia2: Cia,
    /// Color RAM (1KB, 4-bit values)
    pub(crate) color_ram: [u8; 1024],
    /// Cycle counter for the current frame
    pub(crate) cycles: u32,
    /// Keyboard matrix (directly in Memory for CIA access)
    pub(crate) keyboard_matrix: [u8; 8],
    /// Pending SID writes (register, value)
    pub(crate) sid_writes: Vec<(u8, u8)>,
    /// Current raster line (synced from VIC for accurate $D011/$D012 reads)
    pub current_raster_line: u16,
}

/// CIA (Complex Interface Adapter) chip state.
#[derive(Default)]
pub struct Cia {
    /// Port A data register
    pub pra: u8,
    /// Port B data register
    pub prb: u8,
    /// Port A data direction
    pub ddra: u8,
    /// Port B data direction
    pub ddrb: u8,
    /// Timer A latch (low byte)
    pub ta_latch_lo: u8,
    /// Timer A latch (high byte)
    pub ta_latch_hi: u8,
    /// Timer A counter (low byte)
    pub ta_lo: u8,
    /// Timer A counter (high byte)
    pub ta_hi: u8,
    /// Timer B latch (low byte)
    pub tb_latch_lo: u8,
    /// Timer B latch (high byte)
    pub tb_latch_hi: u8,
    /// Timer B counter (low byte)
    pub tb_lo: u8,
    /// Timer B counter (high byte)
    pub tb_hi: u8,
    /// Control register A
    pub cra: u8,
    /// Control register B
    pub crb: u8,
    /// Interrupt control register
    pub icr: u8,
    /// Interrupt mask
    pub icr_mask: u8,
    /// TOD registers
    pub tod_10ths: u8,
    pub tod_sec: u8,
    pub tod_min: u8,
    pub tod_hr: u8,
}

impl Memory {
    pub fn new() -> Self {
        Self {
            ram: [0; 65536],
            basic_rom: [0; 8192],
            kernal_rom: [0; 8192],
            char_rom: [0; 4096],
            port_ddr: 0x2F,  // Default DDR
            port_data: 0x37, // Default: all ROMs visible, I/O enabled
            vic_registers: [0; 64],
            sid_registers: [0; 32],
            cia1: Cia::default(),
            cia2: Cia::default(),
            color_ram: [0; 1024],
            cycles: 0,
            keyboard_matrix: [0xFF; 8], // All keys released
            sid_writes: Vec::new(),
            current_raster_line: 0,
        }
    }

    /// Load BASIC ROM.
    pub fn load_basic(&mut self, data: &[u8]) {
        let len = data.len().min(8192);
        self.basic_rom[..len].copy_from_slice(&data[..len]);
    }

    /// Load KERNAL ROM.
    pub fn load_kernal(&mut self, data: &[u8]) {
        let len = data.len().min(8192);
        self.kernal_rom[..len].copy_from_slice(&data[..len]);
    }

    /// Load Character ROM.
    pub fn load_chargen(&mut self, data: &[u8]) {
        let len = data.len().min(4096);
        self.char_rom[..len].copy_from_slice(&data[..len]);
    }

    /// Get the effective memory configuration.
    fn config(&self) -> u8 {
        // Combine DDR and data port
        (self.port_data | !self.port_ddr) & 0x07
    }

    /// Check if BASIC ROM is visible.
    fn basic_visible(&self) -> bool {
        self.config() & 0x03 == 0x03 // LORAM and HIRAM both set
    }

    /// Check if KERNAL ROM is visible.
    fn kernal_visible(&self) -> bool {
        self.config() & 0x02 != 0 // HIRAM set
    }

    /// Check if I/O is visible (vs Character ROM).
    fn io_visible(&self) -> bool {
        // I/O visible when CHAREN=1 and (LORAM=1 or HIRAM=1)
        let cfg = self.config();
        (cfg & 0x04 != 0) && (cfg & 0x03 != 0)
    }

    /// Check if Character ROM is visible.
    fn char_visible(&self) -> bool {
        // Char ROM visible when CHAREN=0 and (LORAM=1 or HIRAM=1)
        let cfg = self.config();
        (cfg & 0x04 == 0) && (cfg & 0x03 != 0)
    }

    /// Read from I/O space ($D000-$DFFF).
    fn read_io(&mut self, addr: u16) -> u8 {
        match addr {
            // VIC-II ($D000-$D3FF, mirrors every 64 bytes)
            0xD000..=0xD3FF => {
                let reg = (addr & 0x3F) as usize;
                self.read_vic(reg)
            }
            // SID ($D400-$D7FF, mirrors every 32 bytes)
            // Most SID registers are write-only, return last written value
            0xD400..=0xD7FF => {
                let reg = (addr & 0x1F) as u8;
                // Only $1B (osc3 output) and $1C (env3 output) are readable
                // Return cached value; actual read happens in C64::read_sid
                self.sid_registers[reg as usize]
            }
            // Color RAM ($D800-$DBFF)
            0xD800..=0xDBFF => {
                let offset = (addr - 0xD800) as usize;
                self.color_ram[offset] | 0xF0 // Upper nibble reads as 1s
            }
            // CIA1 ($DC00-$DCFF, mirrors every 16 bytes)
            0xDC00..=0xDCFF => {
                let reg = (addr & 0x0F) as usize;
                self.read_cia1(reg)
            }
            // CIA2 ($DD00-$DDFF, mirrors every 16 bytes)
            0xDD00..=0xDDFF => {
                let reg = (addr & 0x0F) as usize;
                // Note: read_cia2 clears ICR on read, so it needs &mut self
                match reg {
                    0x00 => self.cia2.pra,
                    0x01 => self.cia2.prb,
                    0x02 => self.cia2.ddra,
                    0x03 => self.cia2.ddrb,
                    0x04 => self.cia2.ta_lo,
                    0x05 => self.cia2.ta_hi,
                    0x06 => self.cia2.tb_lo,
                    0x07 => self.cia2.tb_hi,
                    0x08 => self.cia2.tod_10ths,
                    0x09 => self.cia2.tod_sec,
                    0x0A => self.cia2.tod_min,
                    0x0B => self.cia2.tod_hr,
                    0x0D => {
                        // ICR - reading clears it
                        let value = self.cia2.icr;
                        self.cia2.icr = 0;
                        value
                    }
                    0x0E => self.cia2.cra,
                    0x0F => self.cia2.crb,
                    _ => 0xFF,
                }
            }
            // I/O expansion areas
            0xDE00..=0xDFFF => 0xFF, // Open bus
            _ => unreachable!(),
        }
    }

    /// Write to I/O space ($D000-$DFFF).
    fn write_io(&mut self, addr: u16, value: u8) {
        match addr {
            // VIC-II ($D000-$D3FF)
            0xD000..=0xD3FF => {
                let reg = (addr & 0x3F) as usize;
                self.write_vic(reg, value);
            }
            // SID ($D400-$D7FF)
            0xD400..=0xD7FF => {
                let reg = (addr & 0x1F) as u8;
                self.sid_registers[reg as usize] = value;
                // Queue for the actual SID chip
                self.sid_writes.push((reg, value));
            }
            // Color RAM ($D800-$DBFF)
            0xD800..=0xDBFF => {
                let offset = (addr - 0xD800) as usize;
                self.color_ram[offset] = value & 0x0F; // Only lower nibble
            }
            // CIA1 ($DC00-$DCFF)
            0xDC00..=0xDCFF => {
                let reg = (addr & 0x0F) as usize;
                self.write_cia1(reg, value);
            }
            // CIA2 ($DD00-$DDFF)
            0xDD00..=0xDDFF => {
                let reg = (addr & 0x0F) as usize;
                self.write_cia2(reg, value);
            }
            // I/O expansion areas - ignored
            0xDE00..=0xDFFF => {}
            _ => unreachable!(),
        }
    }

    fn read_vic(&mut self, reg: usize) -> u8 {
        match reg {
            0x11 => {
                // Control register 1 with current raster bit 8
                let raster_bit8 = if self.current_raster_line > 255 {
                    0x80
                } else {
                    0
                };
                (self.vic_registers[0x11] & 0x7F) | raster_bit8
            }
            0x12 => {
                // Raster counter (low 8 bits)
                (self.current_raster_line & 0xFF) as u8
            }
            0x19 => {
                // Interrupt register - always return with bit 7 set if any IRQ
                let irq_status = self.vic_registers[0x19];
                if irq_status & 0x0F != 0 {
                    irq_status | 0x80
                } else {
                    irq_status
                }
            }
            0x1E => {
                // Sprite-sprite collision register - cleared on read
                let value = self.vic_registers[0x1E];
                self.vic_registers[0x1E] = 0;
                value
            }
            0x1F => {
                // Sprite-background collision register - cleared on read
                let value = self.vic_registers[0x1F];
                self.vic_registers[0x1F] = 0;
                value
            }
            _ => self.vic_registers[reg],
        }
    }

    fn write_vic(&mut self, reg: usize, value: u8) {
        match reg {
            0x19 => {
                // Acknowledge interrupts (write 1 to clear)
                self.vic_registers[0x19] &= !value;
            }
            _ => {
                self.vic_registers[reg] = value;
            }
        }
    }

    fn read_cia1(&mut self, reg: usize) -> u8 {
        match reg {
            0x00 => {
                // Port A - keyboard column select output
                self.cia1.pra
            }
            0x01 => {
                // Port B - keyboard row input
                // Compute dynamically based on port A column select
                let column_select = self.cia1.pra | !self.cia1.ddra;
                let mut result = 0xFF;
                for col in 0..8 {
                    if column_select & (1 << col) == 0 {
                        result &= self.keyboard_matrix[col];
                    }
                }
                result
            }
            0x02 => self.cia1.ddra,
            0x03 => self.cia1.ddrb,
            0x04 => self.cia1.ta_lo,
            0x05 => self.cia1.ta_hi,
            0x06 => self.cia1.tb_lo,
            0x07 => self.cia1.tb_hi,
            0x08 => self.cia1.tod_10ths,
            0x09 => self.cia1.tod_sec,
            0x0A => self.cia1.tod_min,
            0x0B => self.cia1.tod_hr,
            0x0D => {
                // ICR - reading clears it
                let value = self.cia1.icr;
                self.cia1.icr = 0;
                value
            }
            0x0E => self.cia1.cra,
            0x0F => self.cia1.crb,
            _ => 0xFF,
        }
    }

    fn write_cia1(&mut self, reg: usize, value: u8) {
        match reg {
            0x00 => self.cia1.pra = value,
            0x01 => self.cia1.prb = value,
            0x02 => self.cia1.ddra = value,
            0x03 => self.cia1.ddrb = value,
            0x04 => self.cia1.ta_latch_lo = value,
            0x05 => {
                self.cia1.ta_latch_hi = value;
                // If timer not running, load latch into counter
                if self.cia1.cra & 0x01 == 0 {
                    self.cia1.ta_lo = self.cia1.ta_latch_lo;
                    self.cia1.ta_hi = self.cia1.ta_latch_hi;
                }
            }
            0x06 => self.cia1.tb_latch_lo = value,
            0x07 => {
                self.cia1.tb_latch_hi = value;
                if self.cia1.crb & 0x01 == 0 {
                    self.cia1.tb_lo = self.cia1.tb_latch_lo;
                    self.cia1.tb_hi = self.cia1.tb_latch_hi;
                }
            }
            0x0D => {
                // ICR mask register
                if value & 0x80 != 0 {
                    // Set bits
                    self.cia1.icr_mask |= value & 0x1F;
                } else {
                    // Clear bits
                    self.cia1.icr_mask &= !(value & 0x1F);
                }
            }
            0x0E => {
                self.cia1.cra = value;
                // Force load if bit 4 set
                if value & 0x10 != 0 {
                    self.cia1.ta_lo = self.cia1.ta_latch_lo;
                    self.cia1.ta_hi = self.cia1.ta_latch_hi;
                }
            }
            0x0F => {
                self.cia1.crb = value;
                if value & 0x10 != 0 {
                    self.cia1.tb_lo = self.cia1.tb_latch_lo;
                    self.cia1.tb_hi = self.cia1.tb_latch_hi;
                }
            }
            _ => {}
        }
    }

    fn write_cia2(&mut self, reg: usize, value: u8) {
        match reg {
            0x00 => self.cia2.pra = value,
            0x01 => self.cia2.prb = value,
            0x02 => self.cia2.ddra = value,
            0x03 => self.cia2.ddrb = value,
            0x04 => self.cia2.ta_latch_lo = value,
            0x05 => {
                self.cia2.ta_latch_hi = value;
                // If timer not running, load latch into counter
                if self.cia2.cra & 0x01 == 0 {
                    self.cia2.ta_lo = self.cia2.ta_latch_lo;
                    self.cia2.ta_hi = self.cia2.ta_latch_hi;
                }
            }
            0x06 => self.cia2.tb_latch_lo = value,
            0x07 => {
                self.cia2.tb_latch_hi = value;
                if self.cia2.crb & 0x01 == 0 {
                    self.cia2.tb_lo = self.cia2.tb_latch_lo;
                    self.cia2.tb_hi = self.cia2.tb_latch_hi;
                }
            }
            0x0D => {
                // ICR mask register
                if value & 0x80 != 0 {
                    self.cia2.icr_mask |= value & 0x1F;
                } else {
                    self.cia2.icr_mask &= !(value & 0x1F);
                }
            }
            0x0E => {
                self.cia2.cra = value;
                // Force load if bit 4 set
                if value & 0x10 != 0 {
                    self.cia2.ta_lo = self.cia2.ta_latch_lo;
                    self.cia2.ta_hi = self.cia2.ta_latch_hi;
                }
            }
            0x0F => {
                self.cia2.crb = value;
                if value & 0x10 != 0 {
                    self.cia2.tb_lo = self.cia2.tb_latch_lo;
                    self.cia2.tb_hi = self.cia2.tb_latch_hi;
                }
            }
            _ => {}
        }
    }

    /// Get VIC bank base address (controlled by CIA2 port A).
    pub fn vic_bank(&self) -> u16 {
        // Bits 0-1 of CIA2 port A select VIC bank (active low)
        let bank = (!self.cia2.pra) & 0x03;
        (bank as u16) * 0x4000
    }

    /// Tick CIA1 timers. Returns true if IRQ should fire.
    pub fn tick_cia1(&mut self, cycles: u32) -> bool {
        let mut irq = false;
        let mut ta_underflows = 0u32;

        // Timer A - counts down when started (CRA bit 0)
        if self.cia1.cra & 0x01 != 0 {
            let timer = u16::from_le_bytes([self.cia1.ta_lo, self.cia1.ta_hi]);
            if let Some(new_timer) = timer.checked_sub(cycles as u16) {
                self.cia1.ta_lo = (new_timer & 0xFF) as u8;
                self.cia1.ta_hi = (new_timer >> 8) as u8;
            } else {
                // Timer underflow
                ta_underflows = 1;
                self.cia1.icr |= 0x01; // Set Timer A interrupt flag

                // Check if Timer A interrupt is enabled
                if self.cia1.icr_mask & 0x01 != 0 {
                    self.cia1.icr |= 0x80; // Set IRQ flag
                    irq = true;
                }

                // Reload from latch (or stop if one-shot mode)
                if self.cia1.cra & 0x08 != 0 {
                    // One-shot mode - stop timer
                    self.cia1.cra &= !0x01;
                }
                // Reload latch
                self.cia1.ta_lo = self.cia1.ta_latch_lo;
                self.cia1.ta_hi = self.cia1.ta_latch_hi;
            }
        }

        // Timer B - counts down when started (CRB bit 0)
        // CRB bits 5-6 select input source:
        //   00 = count system clock
        //   01 = count Timer A underflows
        //   10 = count CNT pin (not implemented)
        //   11 = count CNT pin while CNT=1 (not implemented)
        if self.cia1.crb & 0x01 != 0 {
            let tb_mode = (self.cia1.crb >> 5) & 0x03;

            let ticks = match tb_mode {
                0b00 => cycles,        // Count system clock
                0b01 => ta_underflows, // Count Timer A underflows
                _ => 0,                // CNT modes not implemented
            };

            if ticks > 0 {
                let timer = u16::from_le_bytes([self.cia1.tb_lo, self.cia1.tb_hi]);
                if let Some(new_timer) = timer.checked_sub(ticks as u16) {
                    self.cia1.tb_lo = (new_timer & 0xFF) as u8;
                    self.cia1.tb_hi = (new_timer >> 8) as u8;
                } else {
                    // Timer underflow
                    self.cia1.icr |= 0x02; // Set Timer B interrupt flag

                    if self.cia1.icr_mask & 0x02 != 0 {
                        self.cia1.icr |= 0x80;
                        irq = true;
                    }

                    if self.cia1.crb & 0x08 != 0 {
                        self.cia1.crb &= !0x01;
                    }
                    self.cia1.tb_lo = self.cia1.tb_latch_lo;
                    self.cia1.tb_hi = self.cia1.tb_latch_hi;
                }
            }
        }

        irq
    }

    /// Tick CIA2 timers. Returns true if NMI should fire.
    pub fn tick_cia2(&mut self, cycles: u32) -> bool {
        let mut nmi = false;
        let mut ta_underflows = 0u32;

        // Timer A - counts down when started (CRA bit 0)
        if self.cia2.cra & 0x01 != 0 {
            let timer = u16::from_le_bytes([self.cia2.ta_lo, self.cia2.ta_hi]);
            if let Some(new_timer) = timer.checked_sub(cycles as u16) {
                self.cia2.ta_lo = (new_timer & 0xFF) as u8;
                self.cia2.ta_hi = (new_timer >> 8) as u8;
            } else {
                // Timer underflow
                ta_underflows = 1;
                self.cia2.icr |= 0x01; // Set Timer A interrupt flag

                // Check if Timer A interrupt is enabled
                if self.cia2.icr_mask & 0x01 != 0 {
                    self.cia2.icr |= 0x80; // Set NMI flag
                    nmi = true;
                }

                // Reload from latch (or stop if one-shot mode)
                if self.cia2.cra & 0x08 != 0 {
                    self.cia2.cra &= !0x01;
                }
                self.cia2.ta_lo = self.cia2.ta_latch_lo;
                self.cia2.ta_hi = self.cia2.ta_latch_hi;
            }
        }

        // Timer B with cascade mode support
        if self.cia2.crb & 0x01 != 0 {
            let tb_mode = (self.cia2.crb >> 5) & 0x03;

            let ticks = match tb_mode {
                0b00 => cycles,
                0b01 => ta_underflows,
                _ => 0,
            };

            if ticks > 0 {
                let timer = u16::from_le_bytes([self.cia2.tb_lo, self.cia2.tb_hi]);
                if let Some(new_timer) = timer.checked_sub(ticks as u16) {
                    self.cia2.tb_lo = (new_timer & 0xFF) as u8;
                    self.cia2.tb_hi = (new_timer >> 8) as u8;
                } else {
                    self.cia2.icr |= 0x02;

                    if self.cia2.icr_mask & 0x02 != 0 {
                        self.cia2.icr |= 0x80;
                        nmi = true;
                    }

                    if self.cia2.crb & 0x08 != 0 {
                        self.cia2.crb &= !0x01;
                    }
                    self.cia2.tb_lo = self.cia2.tb_latch_lo;
                    self.cia2.tb_hi = self.cia2.tb_latch_hi;
                }
            }
        }

        nmi
    }

    /// Reset the memory subsystem.
    pub fn reset(&mut self) {
        self.ram = [0; 65536];
        self.port_ddr = 0x2F;
        self.port_data = 0x37;
        self.vic_registers = [0; 64];
        self.sid_registers = [0; 32];
        self.cia1 = Cia::default();
        self.cia2 = Cia::default();
        self.color_ram = [0; 1024];
        self.cycles = 0;
        self.keyboard_matrix = [0xFF; 8];
        self.sid_writes.clear();
        self.current_raster_line = 0;

        // Initialize VIC-II to sensible defaults
        self.vic_registers[0x11] = 0x1B; // Screen on, 25 rows
        self.vic_registers[0x16] = 0xC8; // 40 columns
        self.vic_registers[0x18] = 0x15; // Screen at $0400, chars at $1000
        self.vic_registers[0x20] = 0x0E; // Border: light blue
        self.vic_registers[0x21] = 0x06; // Background: blue
    }

    /// Get screen memory pointer for VIC-II.
    pub fn screen_ptr(&self) -> u16 {
        let vm = ((self.vic_registers[0x18] >> 4) & 0x0F) as u16;
        self.vic_bank() + vm * 0x400
    }

    /// Get character memory pointer for VIC-II.
    pub fn char_ptr(&self) -> u16 {
        let cb = ((self.vic_registers[0x18] >> 1) & 0x07) as u16;
        self.vic_bank() + cb * 0x800
    }

    /// Read a byte for VIC-II rendering (sees RAM or Char ROM).
    pub fn vic_read(&self, addr: u16) -> u8 {
        let bank = self.vic_bank();
        let physical = bank.wrapping_add(addr);

        // Check for Character ROM at $1000-$1FFF or $9000-$9FFF in bank 0 or 2
        if (bank == 0x0000 || bank == 0x8000) && (addr & 0x3000 == 0x1000) {
            return self.char_rom[(addr & 0x0FFF) as usize];
        }

        self.ram[physical as usize]
    }
}

impl Default for Memory {
    fn default() -> Self {
        Self::new()
    }
}

impl Bus for Memory {
    fn read(&mut self, address: u32) -> u8 {
        let addr = (address & 0xFFFF) as u16;
        self.cycles += 1;

        match addr {
            // Processor port
            0x0000 => self.port_ddr,
            0x0001 => {
                // Port data (bits 6-7 read from datasette, always high here)
                (self.port_data & self.port_ddr) | (!self.port_ddr & 0xC0) | 0x10
            }
            // Zero page and stack (always RAM)
            0x0002..=0x01FF => self.ram[addr as usize],
            // RAM
            0x0200..=0x9FFF => self.ram[addr as usize],
            // BASIC ROM or RAM ($A000-$BFFF)
            0xA000..=0xBFFF => {
                if self.basic_visible() {
                    self.basic_rom[(addr - 0xA000) as usize]
                } else {
                    self.ram[addr as usize]
                }
            }
            // RAM ($C000-$CFFF)
            0xC000..=0xCFFF => self.ram[addr as usize],
            // I/O, Char ROM, or RAM ($D000-$DFFF)
            0xD000..=0xDFFF => {
                if self.io_visible() {
                    self.read_io(addr)
                } else if self.char_visible() {
                    self.char_rom[(addr - 0xD000) as usize]
                } else {
                    self.ram[addr as usize]
                }
            }
            // KERNAL ROM or RAM ($E000-$FFFF)
            0xE000..=0xFFFF => {
                if self.kernal_visible() {
                    self.kernal_rom[(addr - 0xE000) as usize]
                } else {
                    self.ram[addr as usize]
                }
            }
        }
    }

    fn write(&mut self, address: u32, value: u8) {
        let addr = (address & 0xFFFF) as u16;
        self.cycles += 1;

        match addr {
            // Processor port
            0x0000 => self.port_ddr = value,
            0x0001 => self.port_data = value,
            // I/O space (writes always go here when visible)
            0xD000..=0xDFFF if self.io_visible() => {
                self.write_io(addr, value);
            }
            // All writes go to RAM (including ROM areas)
            _ => {
                self.ram[addr as usize] = value;
            }
        }
    }

    fn tick(&mut self, cycles: u32) {
        self.cycles += cycles;
    }
}
