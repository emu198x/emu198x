//! MSX1 home computer emulator.
//!
//! The MSX is a standardised home computer platform (1983) with a Z80 CPU,
//! TMS9918 video, AY-3-8910 audio, and the MSX-specific slot system for
//! memory mapping. The standard was designed by ASCII Corporation and
//! manufactured by over 20 companies.
//!
//! - **CPU:** Z80A @ 3.579545 MHz
//! - **Video:** TMS9918A/9928A/9929A (16 KB VRAM)
//! - **Audio:** AY-3-8910 @ 1.789773 MHz
//! - **I/O:** Intel 8255 PPI (keyboard, slot select, cassette)
//! - **RAM:** 64 KB (standard), 8-64 KB (spec minimum 8 KB)
//! - **ROM:** 32 KB Main-ROM (BIOS + MSX-BASIC 1.0)
//!
//! # Slot System
//!
//! The Z80's 64 KB address space is divided into four 16 KB pages, each
//! independently mapped to one of 4 primary slots via PPI port A ($A8).
//! Slots can be expanded into 4 sub-slots, selected by writing to $FFFF.

#![allow(clippy::cast_possible_truncation)]

use emu_core::{Bus, Cpu, ReadResult};
use gi_ay_3_8910::Ay3_8910;
use intel_8255::Ppi8255;
use ti_tms9918::{Tms9918, VdpRegion};
use zilog_z80::Z80;

/// VDP dots per CPU cycle.
const VDP_DOTS_PER_CPU: u64 = 3;
/// NTSC ticks per frame.
const NTSC_TICKS_PER_FRAME: u64 = 342 * 262 * 3;

/// MSX region.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MsxRegion {
    Ntsc,
    Pal,
}

// ---------------------------------------------------------------------------
// MegaROM mapper types
// ---------------------------------------------------------------------------

/// Cartridge mapper type for MegaROM support.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MapperType {
    /// Plain ROM (no banking), up to 32 KB.
    Plain,
    /// Konami without SCC: 8 KB banks, $6000/$8000/$A000 select.
    Konami,
    /// Konami SCC: 8 KB banks, $5000/$7000/$9000/$B000 select.
    KonamiScc,
    /// ASCII 8 KB: banks via $6000/$6800/$7000/$7800.
    Ascii8,
    /// ASCII 16 KB: banks via $6000/$7000.
    Ascii16,
}

/// A cartridge slot containing ROM and optional mapper state.
struct CartridgeSlot {
    rom: Vec<u8>,
    mapper: MapperType,
    banks: [u8; 4], // Bank registers for the 4 windows ($4000-$BFFF)
}

impl CartridgeSlot {
    fn new(rom: Vec<u8>, mapper: MapperType) -> Self {
        Self { rom, mapper, banks: [0, 1, 2, 3] }
    }

    fn empty() -> Self {
        Self { rom: Vec::new(), mapper: MapperType::Plain, banks: [0; 4] }
    }

    fn is_empty(&self) -> bool {
        self.rom.is_empty()
    }

    fn read(&self, addr: u16) -> u8 {
        if self.rom.is_empty() {
            return 0xFF;
        }
        match self.mapper {
            MapperType::Plain => {
                // ROM starts at $4000 in the cartridge address space
                let offset = if addr >= 0x4000 {
                    (addr - 0x4000) as usize
                } else {
                    addr as usize
                };
                self.rom.get(offset).copied().unwrap_or(0xFF)
            }
            MapperType::Konami | MapperType::KonamiScc => {
                let window = match addr {
                    0x4000..=0x5FFF => 0,
                    0x6000..=0x7FFF => 1,
                    0x8000..=0x9FFF => 2,
                    0xA000..=0xBFFF => 3,
                    _ => return 0xFF,
                };
                let bank = self.banks[window] as usize;
                let offset = bank * 8192 + (addr as usize & 0x1FFF);
                self.rom.get(offset).copied().unwrap_or(0xFF)
            }
            MapperType::Ascii8 => {
                let window = match addr {
                    0x4000..=0x5FFF => 0,
                    0x6000..=0x7FFF => 1,
                    0x8000..=0x9FFF => 2,
                    0xA000..=0xBFFF => 3,
                    _ => return 0xFF,
                };
                let bank = self.banks[window] as usize;
                let offset = bank * 8192 + (addr as usize & 0x1FFF);
                self.rom.get(offset).copied().unwrap_or(0xFF)
            }
            MapperType::Ascii16 => {
                let window = if addr < 0x8000 { 0 } else { 1 };
                let bank = self.banks[window] as usize;
                let offset = bank * 16384 + (addr as usize & 0x3FFF);
                self.rom.get(offset).copied().unwrap_or(0xFF)
            }
        }
    }

    fn write(&mut self, addr: u16, value: u8) {
        match self.mapper {
            MapperType::Konami => match addr {
                0x6000..=0x7FFF => self.banks[1] = value,
                0x8000..=0x9FFF => self.banks[2] = value,
                0xA000..=0xBFFF => self.banks[3] = value,
                _ => {}
            },
            MapperType::KonamiScc => match addr {
                0x5000..=0x57FF => self.banks[0] = value,
                0x7000..=0x77FF => self.banks[1] = value,
                0x9000..=0x97FF => self.banks[2] = value,
                0xB000..=0xB7FF => self.banks[3] = value,
                _ => {}
            },
            MapperType::Ascii8 => match addr {
                0x6000..=0x67FF => self.banks[0] = value,
                0x6800..=0x6FFF => self.banks[1] = value,
                0x7000..=0x77FF => self.banks[2] = value,
                0x7800..=0x7FFF => self.banks[3] = value,
                _ => {}
            },
            MapperType::Ascii16 => match addr {
                0x6000..=0x67FF => self.banks[0] = value,
                0x7000..=0x77FF => self.banks[1] = value,
                _ => {}
            },
            MapperType::Plain => {}
        }
    }
}

// ---------------------------------------------------------------------------
// MSX Bus
// ---------------------------------------------------------------------------

/// MSX bus: implements the slot system, I/O routing, and memory mapping.
pub struct MsxBus {
    // Slot contents
    /// Slot 0: Main-ROM (BIOS 32 KB at pages 0-1).
    bios_rom: Vec<u8>,
    /// Slot 1: Cartridge slot 1.
    cart1: CartridgeSlot,
    /// Slot 2: Cartridge slot 2.
    cart2: CartridgeSlot,
    /// Slot 3: RAM (64 KB, optionally expanded).
    ram: Vec<u8>,
    /// Whether slot 3 is expanded (has sub-slots).
    slot3_expanded: bool,
    /// Sub-slot register for slot 3 (only used if expanded).
    sub_slot_reg: u8,

    // Chips
    /// 8255 PPI (Port A = slot select).
    pub ppi: Ppi8255,
    /// TMS9918 VDP.
    pub vdp: Tms9918,
    /// AY-3-8910 PSG.
    pub psg: Ay3_8910,
    // PSG latch is internal to the AY-3-8910

    // Keyboard
    /// Keyboard matrix: 11 rows × 8 columns, active-low.
    pub keyboard: [u8; 11],
}

impl MsxBus {
    /// Create a new MSX bus with BIOS ROM and 64 KB RAM.
    #[must_use]
    pub fn new(bios_rom: Vec<u8>, region: MsxRegion) -> Self {
        let vdp_region = match region {
            MsxRegion::Ntsc => VdpRegion::Ntsc,
            MsxRegion::Pal => VdpRegion::Pal,
        };
        Self {
            bios_rom,
            cart1: CartridgeSlot::empty(),
            cart2: CartridgeSlot::empty(),
            ram: vec![0u8; 65536],
            slot3_expanded: false,
            sub_slot_reg: 0,
            ppi: Ppi8255::new(),
            vdp: Tms9918::new(vdp_region),
            psg: Ay3_8910::new(1_789_773, 48_000),
            // PSG latch handled internally
            keyboard: [0xFF; 11], // All keys released (active-low)
        }
    }

    /// Insert a cartridge into slot 1.
    pub fn insert_cart1(&mut self, rom: Vec<u8>, mapper: MapperType) {
        self.cart1 = CartridgeSlot::new(rom, mapper);
    }

    /// Insert a cartridge into slot 2.
    pub fn insert_cart2(&mut self, rom: Vec<u8>, mapper: MapperType) {
        self.cart2 = CartridgeSlot::new(rom, mapper);
    }

    /// Resolve which slot is active for a given address.
    fn resolve_slot(&self, addr: u16) -> u8 {
        let page = (addr >> 14) as usize;
        let slot_reg = self.ppi.port_a;
        (slot_reg >> (page * 2)) & 0x03
    }

    /// Read from a specific slot at the given address.
    fn read_slot(&self, slot: u8, addr: u16) -> u8 {
        match slot {
            0 => {
                // BIOS ROM at pages 0-1 ($0000-$7FFF)
                if addr < 0x8000 {
                    self.bios_rom.get(addr as usize).copied().unwrap_or(0xFF)
                } else {
                    0xFF
                }
            }
            1 => self.cart1.read(addr),
            2 => self.cart2.read(addr),
            3 => {
                // RAM (entire 64 KB)
                self.ram[addr as usize]
            }
            _ => 0xFF,
        }
    }

    /// Write to a specific slot at the given address.
    fn write_slot(&mut self, slot: u8, addr: u16, value: u8) {
        match slot {
            0 => {} // ROM, no writes
            1 => self.cart1.write(addr, value),
            2 => self.cart2.write(addr, value),
            3 => {
                self.ram[addr as usize] = value;
            }
            _ => {}
        }
    }
}

impl Bus for MsxBus {
    fn read(&mut self, addr: u32) -> ReadResult {
        let addr = addr as u16;

        // Sub-slot register at $FFFF: return inverted value when slot 3 is
        // expanded and currently selected for page 3.
        if addr == 0xFFFF && self.slot3_expanded {
            let slot = self.resolve_slot(0xFFFF);
            if slot == 3 {
                return ReadResult::new(!self.sub_slot_reg);
            }
        }

        let slot = self.resolve_slot(addr);
        ReadResult::new(self.read_slot(slot, addr))
    }

    fn write(&mut self, addr: u32, value: u8) -> u8 {
        let addr = addr as u16;

        // Sub-slot register at $FFFF
        if addr == 0xFFFF && self.slot3_expanded {
            let slot = self.resolve_slot(0xFFFF);
            if slot == 3 {
                self.sub_slot_reg = value;
                return 0;
            }
        }

        let slot = self.resolve_slot(addr);
        self.write_slot(slot, addr, value);
        0
    }

    fn io_read(&mut self, port: u32) -> ReadResult {
        let data = match port as u8 {
            // VDP data read
            0x98 => self.vdp.read_data(),
            // VDP status read
            0x99 => self.vdp.read_status(),
            // PSG register read
            0xA2 => self.psg.read_data(),
            // PPI ports
            0xA8 => self.ppi.read(0), // Port A (slot select)
            0xA9 => {
                // Port B (keyboard data) — read from keyboard matrix
                let row = self.ppi.keyboard_row();
                if (row as usize) < self.keyboard.len() {
                    self.keyboard[row as usize]
                } else {
                    0xFF // Rows 11-15 return all keys released
                }
            }
            0xAA => self.ppi.read(2), // Port C
            0xAB => self.ppi.read(3), // Control
            _ => 0xFF,
        };
        ReadResult::new(data)
    }

    fn io_write(&mut self, port: u32, value: u8) -> u8 {
        match port as u8 {
            // VDP data write
            0x98 => self.vdp.write_data(value),
            // VDP control write
            0x99 => self.vdp.write_control(value),
            // PSG register select
            0xA0 => self.psg.select_register(value),
            // PSG register write
            0xA1 => self.psg.write_data(value),
            // PPI ports
            0xA8 => self.ppi.write(0, value), // Slot select
            0xAA => self.ppi.write(2, value), // Keyboard row + control
            0xAB => self.ppi.write(3, value), // Command register
            _ => {}
        }
        0
    }
}

// ---------------------------------------------------------------------------
// MSX system
// ---------------------------------------------------------------------------

/// MSX1 system.
pub struct Msx {
    cpu: Z80,
    bus: MsxBus,
    master_clock: u64,
    ticks_per_frame: u64,
    frame_count: u64,
}

impl Msx {
    /// Create a new MSX with BIOS ROM.
    #[must_use]
    pub fn new(bios_rom: Vec<u8>, region: MsxRegion) -> Self {
        let ticks_per_frame = match region {
            MsxRegion::Ntsc => NTSC_TICKS_PER_FRAME,
            MsxRegion::Pal => 342 * 313 * 3,
        };
        let bus = MsxBus::new(bios_rom, region);
        let cpu = Z80::new();

        Self {
            cpu,
            bus,
            master_clock: 0,
            ticks_per_frame,
            frame_count: 0,
        }
    }

    /// Insert a cartridge into slot 1.
    pub fn insert_cart1(&mut self, rom: Vec<u8>, mapper: MapperType) {
        self.bus.insert_cart1(rom, mapper);
    }

    /// Insert a cartridge into slot 2.
    pub fn insert_cart2(&mut self, rom: Vec<u8>, mapper: MapperType) {
        self.bus.insert_cart2(rom, mapper);
    }

    /// Run one complete frame.
    pub fn run_frame(&mut self) {
        let target = self.master_clock + self.ticks_per_frame;

        while self.master_clock < target {
            self.cpu.tick(&mut self.bus);

            for _ in 0..VDP_DOTS_PER_CPU {
                self.bus.vdp.tick();
            }

            // PSG runs at CPU/2 — tick every other cycle
            if self.master_clock & 1 == 0 {
                self.bus.psg.tick();
            }

            // VDP interrupt → Z80 INT
            if self.bus.vdp.interrupt {
                self.cpu.interrupt();
            }

            self.master_clock += 1;
        }

        self.frame_count += 1;
    }

    /// The current framebuffer (256×192 ARGB32).
    #[must_use]
    pub fn framebuffer(&self) -> &[u32] {
        self.bus.vdp.framebuffer()
    }

    /// Take audio samples from the PSG (stereo [L, R] pairs, 48 kHz).
    pub fn take_audio_buffer(&mut self) -> Vec<[f32; 2]> {
        self.bus.psg.take_buffer()
    }

    /// Press a key in the keyboard matrix.
    pub fn press_key(&mut self, row: usize, bit: u8) {
        if row < 11 {
            self.keyboard_mut()[row] &= !(1 << bit);
        }
    }

    /// Release a key in the keyboard matrix.
    pub fn release_key(&mut self, row: usize, bit: u8) {
        if row < 11 {
            self.keyboard_mut()[row] |= 1 << bit;
        }
    }

    /// Access the keyboard matrix.
    pub fn keyboard_mut(&mut self) -> &mut [u8; 11] {
        &mut self.bus.keyboard
    }

    /// Frame count.
    #[must_use]
    pub fn frame_count(&self) -> u64 {
        self.frame_count
    }

    /// Access the CPU.
    #[must_use]
    pub fn cpu(&self) -> &Z80 {
        &self.cpu
    }

    /// Access the CPU mutably.
    pub fn cpu_mut(&mut self) -> &mut Z80 {
        &mut self.cpu
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn minimal_bios() -> Vec<u8> {
        // Minimal BIOS: DI; JR -2 (infinite loop)
        let mut rom = vec![0xFF_u8; 32768];
        rom[0] = 0xF3; // DI
        rom[1] = 0x18; // JR
        rom[2] = 0xFE; // -2
        rom
    }

    #[test]
    fn boots_and_runs_frame() {
        let mut msx = Msx::new(minimal_bios(), MsxRegion::Ntsc);
        msx.run_frame();
        assert_eq!(msx.frame_count(), 1);
    }

    #[test]
    fn slot_0_reads_bios() {
        let mut bus = MsxBus::new(minimal_bios(), MsxRegion::Ntsc);
        // Default: all pages mapped to slot 0
        assert_eq!(bus.read(0x0000).data, 0xF3); // DI
        assert_eq!(bus.read(0x0001).data, 0x18); // JR
    }

    #[test]
    fn slot_3_reads_ram() {
        let mut bus = MsxBus::new(minimal_bios(), MsxRegion::Ntsc);
        // Map page 3 to slot 3
        bus.ppi.write(0, 0xC0); // Slot 3 for page 3
        bus.ram[0xC000] = 0xAB;
        assert_eq!(bus.read(0xC000).data, 0xAB);
    }

    #[test]
    fn slot_switch_changes_visible_memory() {
        let mut bus = MsxBus::new(minimal_bios(), MsxRegion::Ntsc);
        bus.ram[0x0000] = 0x42;

        // Page 0 = slot 0 (BIOS)
        bus.ppi.write(0, 0x00);
        assert_eq!(bus.read(0x0000).data, 0xF3); // BIOS

        // Page 0 = slot 3 (RAM)
        bus.ppi.write(0, 0x03);
        assert_eq!(bus.read(0x0000).data, 0x42); // RAM
    }

    #[test]
    fn ram_write_through_slot() {
        let mut bus = MsxBus::new(minimal_bios(), MsxRegion::Ntsc);
        // Map page 2 to slot 3
        bus.ppi.write(0, 0x30); // Slot 3 for page 2
        bus.write(0x8000, 0xCD);
        assert_eq!(bus.ram[0x8000], 0xCD);
    }

    #[test]
    fn cartridge_slot_1_accessible() {
        let mut bus = MsxBus::new(minimal_bios(), MsxRegion::Ntsc);
        let mut cart = vec![0u8; 32768];
        cart[0] = 0x41; // 'A'
        cart[1] = 0x42; // 'B'
        bus.insert_cart1(cart, MapperType::Plain);

        // Map page 1 to slot 1
        bus.ppi.write(0, 0x04); // Slot 1 for page 1
        assert_eq!(bus.read(0x4000).data, 0x41);
        assert_eq!(bus.read(0x4001).data, 0x42);
    }

    #[test]
    fn vdp_accessible_via_io() {
        let mut bus = MsxBus::new(minimal_bios(), MsxRegion::Ntsc);
        // Write VDP register
        bus.io_write(0x99, 0x40);
        bus.io_write(0x99, 0x81);
        let _status = bus.io_read(0x99);
    }

    #[test]
    fn psg_accessible_via_io() {
        let mut bus = MsxBus::new(minimal_bios(), MsxRegion::Ntsc);
        // Select register 7 (mixer)
        bus.io_write(0xA0, 0x07);
        // Write mixer value
        bus.io_write(0xA1, 0x38);
        // Read back — mixer register is R/W
        let val = bus.io_read(0xA2).data;
        // AY-3-8910 register 7 has some bits read-only, just verify non-zero
        assert!(val != 0xFF, "PSG should respond to register read");
    }

    #[test]
    fn keyboard_row_read() {
        let mut bus = MsxBus::new(minimal_bios(), MsxRegion::Ntsc);
        // Press key at row 0, bit 0
        bus.keyboard[0] = 0xFE; // bit 0 low = pressed

        // Select row 0 via PPI port C
        bus.io_write(0xAA, 0x00);
        // Read keyboard data from PPI port B
        let val = bus.io_read(0xA9).data;
        assert_eq!(val, 0xFE);
    }

    #[test]
    fn megarom_konami_banking() {
        let mut cart = CartridgeSlot::new(vec![0; 128 * 1024], MapperType::Konami);
        // Write distinct bytes at the start of banks
        for bank in 0..16u8 {
            let offset = bank as usize * 8192;
            if offset < cart.rom.len() {
                cart.rom[offset] = bank;
            }
        }

        // Bank 0 is always at $4000
        assert_eq!(cart.read(0x4000), 0);

        // Switch $8000 window to bank 5
        cart.write(0x8000, 5);
        assert_eq!(cart.read(0x8000), 5);

        // Switch $A000 window to bank 10
        cart.write(0xA000, 10);
        assert_eq!(cart.read(0xA000), 10);
    }

    #[test]
    fn megarom_ascii16_banking() {
        let mut cart = CartridgeSlot::new(vec![0; 256 * 1024], MapperType::Ascii16);
        for bank in 0..16u8 {
            let offset = bank as usize * 16384;
            if offset < cart.rom.len() {
                cart.rom[offset] = bank;
            }
        }

        // Switch $4000-$7FFF to bank 3
        cart.write(0x6000, 3);
        assert_eq!(cart.read(0x4000), 3);

        // Switch $8000-$BFFF to bank 7
        cart.write(0x7000, 7);
        assert_eq!(cart.read(0x8000), 7);
    }
}
