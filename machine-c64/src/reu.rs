//! REU (Ram Expansion Unit) emulation.
//!
//! The REU is a DMA-based memory expansion for the C64. Models:
//! - 1700: 128K RAM
//! - 1764: 256K RAM
//! - 1750: 512K RAM
//!
//! The REU uses registers at $DF00-$DF0A to control DMA transfers
//! between C64 memory and expansion RAM.

/// REU model variants.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ReuModel {
    /// No REU present
    #[default]
    None,
    /// 1700: 128K RAM (16 banks)
    Reu1700,
    /// 1764: 256K RAM (32 banks)
    Reu1764,
    /// 1750: 512K RAM (64 banks)
    Reu1750,
}

impl ReuModel {
    /// Get the RAM size in bytes for this model.
    pub fn ram_size(&self) -> usize {
        match self {
            ReuModel::None => 0,
            ReuModel::Reu1700 => 128 * 1024,
            ReuModel::Reu1764 => 256 * 1024,
            ReuModel::Reu1750 => 512 * 1024,
        }
    }

    /// Get the number of 8K banks.
    pub fn num_banks(&self) -> u8 {
        match self {
            ReuModel::None => 0,
            ReuModel::Reu1700 => 16,
            ReuModel::Reu1764 => 32,
            ReuModel::Reu1750 => 64,
        }
    }

    /// Get the bank mask for address wrapping.
    pub fn bank_mask(&self) -> u8 {
        match self {
            ReuModel::None => 0,
            ReuModel::Reu1700 => 0x0F,  // 16 banks
            ReuModel::Reu1764 => 0x1F,  // 32 banks
            ReuModel::Reu1750 => 0x3F,  // 64 banks
        }
    }
}

/// Transfer type for REU DMA operations.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransferType {
    /// Stash: C64 RAM -> REU RAM
    Stash = 0,
    /// Fetch: REU RAM -> C64 RAM
    Fetch = 1,
    /// Swap: Exchange C64 RAM <-> REU RAM
    Swap = 2,
    /// Verify: Compare C64 RAM with REU RAM
    Verify = 3,
}

impl From<u8> for TransferType {
    fn from(value: u8) -> Self {
        match value & 0x03 {
            0 => TransferType::Stash,
            1 => TransferType::Fetch,
            2 => TransferType::Swap,
            3 => TransferType::Verify,
            _ => unreachable!(),
        }
    }
}

/// REU (Ram Expansion Unit) state.
#[derive(Clone)]
pub struct Reu {
    /// REU model (determines RAM size)
    model: ReuModel,
    /// Expansion RAM
    ram: Vec<u8>,
    /// Status register ($DF00)
    /// Bit 7: Interrupt pending
    /// Bit 6: End of block
    /// Bit 5: Verify error
    /// Bit 4: Size (0 = 128K, 1 = larger)
    /// Bit 3-0: Version (usually 0)
    status: u8,
    /// Command register ($DF01)
    command: u8,
    /// C64 base address ($DF02-$DF03)
    c64_addr: u16,
    /// REU base address ($DF04-$DF05, low 16 bits)
    reu_addr: u16,
    /// REU bank ($DF06)
    reu_bank: u8,
    /// Transfer length ($DF07-$DF08)
    length: u16,
    /// Interrupt mask ($DF09)
    irq_mask: u8,
    /// Address control ($DF0A)
    /// Bit 7: Fix C64 address
    /// Bit 6: Fix REU address
    addr_control: u8,
    /// Shadow registers for autoload feature
    c64_addr_shadow: u16,
    reu_addr_shadow: u16,
    reu_bank_shadow: u8,
    length_shadow: u16,
    /// Pending DMA operation (set when command is written)
    dma_pending: bool,
}

impl Default for Reu {
    fn default() -> Self {
        Self::new(ReuModel::None)
    }
}

impl Reu {
    /// Create a new REU with the specified model.
    pub fn new(model: ReuModel) -> Self {
        let ram_size = model.ram_size();
        let status = if model != ReuModel::None && model != ReuModel::Reu1700 {
            0x10 // Bit 4 = larger than 128K
        } else {
            0x00
        };

        Self {
            model,
            ram: vec![0xFF; ram_size],
            status,
            command: 0,
            c64_addr: 0,
            reu_addr: 0,
            reu_bank: 0,
            length: 0xFFFF, // Default: 64K (wraps to 0)
            irq_mask: 0,
            addr_control: 0,
            c64_addr_shadow: 0,
            reu_addr_shadow: 0,
            reu_bank_shadow: 0,
            length_shadow: 0xFFFF,
            dma_pending: false,
        }
    }

    /// Check if an REU is present.
    pub fn is_present(&self) -> bool {
        self.model != ReuModel::None
    }

    /// Get the REU model.
    pub fn model(&self) -> ReuModel {
        self.model
    }

    /// Reset the REU.
    pub fn reset(&mut self) {
        self.command = 0;
        self.c64_addr = 0;
        self.reu_addr = 0;
        self.reu_bank = 0;
        self.length = 0xFFFF;
        self.irq_mask = 0;
        self.addr_control = 0;
        self.c64_addr_shadow = 0;
        self.reu_addr_shadow = 0;
        self.reu_bank_shadow = 0;
        self.length_shadow = 0xFFFF;
        self.dma_pending = false;

        // Clear status except size bit
        self.status &= 0x10;
    }

    /// Read from REU register space ($DF00-$DF0A).
    pub fn read(&mut self, addr: u16) -> Option<u8> {
        if !self.is_present() {
            return None;
        }

        match addr {
            0xDF00 => {
                // Status register - reading clears interrupt flags
                let value = self.status;
                self.status &= 0x1F; // Clear bits 7-5
                Some(value)
            }
            0xDF01 => Some(self.command & 0x7F), // Command (bit 7 always reads 0)
            0xDF02 => Some(self.c64_addr as u8),
            0xDF03 => Some((self.c64_addr >> 8) as u8),
            0xDF04 => Some(self.reu_addr as u8),
            0xDF05 => Some((self.reu_addr >> 8) as u8),
            0xDF06 => Some(self.reu_bank),
            0xDF07 => Some(self.length as u8),
            0xDF08 => Some((self.length >> 8) as u8),
            0xDF09 => Some(self.irq_mask),
            0xDF0A => Some(self.addr_control),
            _ => None,
        }
    }

    /// Write to REU register space ($DF00-$DF0A).
    /// Returns true if a DMA operation should be triggered.
    pub fn write(&mut self, addr: u16, value: u8) -> bool {
        if !self.is_present() {
            return false;
        }

        match addr {
            0xDF00 => {
                // Status register is read-only
            }
            0xDF01 => {
                // Command register
                self.command = value;

                // Save current addresses to shadow registers
                self.c64_addr_shadow = self.c64_addr;
                self.reu_addr_shadow = self.reu_addr;
                self.reu_bank_shadow = self.reu_bank;
                self.length_shadow = self.length;

                // Check if execute bit is set
                if value & 0x80 != 0 {
                    // If FF00 decode is enabled, wait for write to $FF00
                    if value & 0x10 != 0 {
                        self.dma_pending = true;
                        return false;
                    }
                    // Otherwise execute immediately
                    return true;
                }
            }
            0xDF02 => {
                self.c64_addr = (self.c64_addr & 0xFF00) | value as u16;
            }
            0xDF03 => {
                self.c64_addr = (self.c64_addr & 0x00FF) | ((value as u16) << 8);
            }
            0xDF04 => {
                self.reu_addr = (self.reu_addr & 0xFF00) | value as u16;
            }
            0xDF05 => {
                self.reu_addr = (self.reu_addr & 0x00FF) | ((value as u16) << 8);
            }
            0xDF06 => {
                self.reu_bank = value & self.model.bank_mask();
            }
            0xDF07 => {
                self.length = (self.length & 0xFF00) | value as u16;
            }
            0xDF08 => {
                self.length = (self.length & 0x00FF) | ((value as u16) << 8);
            }
            0xDF09 => {
                self.irq_mask = value & 0xE0;
            }
            0xDF0A => {
                self.addr_control = value & 0xC0;
            }
            _ => {}
        }
        false
    }

    /// Check if FF00 decode mode is active (for triggering DMA on $FF00 write).
    pub fn ff00_decode_active(&self) -> bool {
        self.dma_pending && (self.command & 0x10 != 0)
    }

    /// Trigger DMA from $FF00 write (when FF00 decode mode is active).
    /// Returns true if DMA should execute.
    pub fn trigger_ff00(&mut self) -> bool {
        if self.dma_pending {
            self.dma_pending = false;
            true
        } else {
            false
        }
    }

    /// Execute a DMA transfer.
    /// Takes a closure that can read/write C64 memory.
    /// Returns true if an IRQ should be triggered.
    pub fn execute_dma<F>(&mut self, mut access_c64: F) -> bool
    where
        F: FnMut(u16, Option<u8>) -> u8,
    {
        if !self.is_present() {
            return false;
        }

        let transfer_type = TransferType::from(self.command);
        let fix_c64 = self.addr_control & 0x80 != 0;
        let fix_reu = self.addr_control & 0x40 != 0;

        // Transfer length: 0 means 64K (0x10000)
        let length = if self.length == 0 {
            0x10000usize
        } else {
            self.length as usize
        };

        let mut c64_addr = self.c64_addr;
        let mut reu_addr = self.reu_addr as u32 | ((self.reu_bank as u32) << 16);
        let bank_mask = self.model.bank_mask() as u32;
        let ram_mask = (self.model.ram_size() - 1) as u32;

        let mut verify_error = false;

        for _ in 0..length {
            // Calculate REU RAM offset with bank wrapping
            let reu_offset = (reu_addr & ram_mask) as usize;

            match transfer_type {
                TransferType::Stash => {
                    // C64 -> REU
                    let byte = access_c64(c64_addr, None);
                    if reu_offset < self.ram.len() {
                        self.ram[reu_offset] = byte;
                    }
                }
                TransferType::Fetch => {
                    // REU -> C64
                    let byte = if reu_offset < self.ram.len() {
                        self.ram[reu_offset]
                    } else {
                        0xFF
                    };
                    access_c64(c64_addr, Some(byte));
                }
                TransferType::Swap => {
                    // Exchange
                    let c64_byte = access_c64(c64_addr, None);
                    let reu_byte = if reu_offset < self.ram.len() {
                        self.ram[reu_offset]
                    } else {
                        0xFF
                    };
                    access_c64(c64_addr, Some(reu_byte));
                    if reu_offset < self.ram.len() {
                        self.ram[reu_offset] = c64_byte;
                    }
                }
                TransferType::Verify => {
                    // Compare
                    let c64_byte = access_c64(c64_addr, None);
                    let reu_byte = if reu_offset < self.ram.len() {
                        self.ram[reu_offset]
                    } else {
                        0xFF
                    };
                    if c64_byte != reu_byte {
                        verify_error = true;
                    }
                }
            }

            // Update addresses unless fixed
            if !fix_c64 {
                c64_addr = c64_addr.wrapping_add(1);
            }
            if !fix_reu {
                reu_addr = (reu_addr + 1) & ((bank_mask << 16) | 0xFFFF);
            }
        }

        // Update registers with final addresses (unless autoload)
        if self.command & 0x20 == 0 {
            // No autoload - update registers
            self.c64_addr = c64_addr;
            self.reu_addr = reu_addr as u16;
            self.reu_bank = ((reu_addr >> 16) & bank_mask) as u8;
            self.length = 1; // Transfer complete
        } else {
            // Autoload - restore from shadow
            self.c64_addr = self.c64_addr_shadow;
            self.reu_addr = self.reu_addr_shadow;
            self.reu_bank = self.reu_bank_shadow;
            self.length = self.length_shadow;
        }

        // Set status bits
        self.status |= 0x40; // End of block

        if verify_error {
            self.status |= 0x20; // Verify error
        }

        // Check for IRQ
        if self.irq_mask & 0x80 != 0 {
            // End of block IRQ enabled
            self.status |= 0x80;
            return true;
        }
        if verify_error && (self.irq_mask & 0x20 != 0) {
            // Verify error IRQ enabled
            self.status |= 0x80;
            return true;
        }

        false
    }

    /// Get the total RAM size.
    pub fn ram_size(&self) -> usize {
        self.ram.len()
    }

    /// Direct read from REU RAM (for debugging/save states).
    pub fn read_ram(&self, addr: u32) -> u8 {
        self.ram.get(addr as usize).copied().unwrap_or(0xFF)
    }

    /// Direct write to REU RAM (for debugging/save states).
    pub fn write_ram(&mut self, addr: u32, value: u8) {
        if let Some(byte) = self.ram.get_mut(addr as usize) {
            *byte = value;
        }
    }
}
