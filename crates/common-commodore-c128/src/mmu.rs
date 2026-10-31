//! The 8722 MMU: which ROM, RAM bank or I/O a C128-mode address reaches.
//!
//! Register by register this follows VICE's `c128mmu.c`, and the address map
//! follows its `c128meminit.c`, `c128mem.c` and `z80mem.c`. C64 mode, where
//! the MMU steps aside for the C64's own PLA, is not modelled yet: the mode
//! bit is held, but the map stays the C128's.
//!
//! The registers are at `$D500-$D50B` while I/O is mapped in, and the first
//! five also at `$FF00-$FF04` in every configuration:
//!
//! | Reg | Name | Meaning |
//! |---|---|---|
//! | 0 | CR | the current configuration (below) |
//! | 1-4 | PCR A-D | preset configurations; a write to `$FF01-$FF04` loads one into CR |
//! | 5 | MCR | bit 0 the CPU (1 = 8502), bit 6 C64 mode, bit 3 fast serial out |
//! | 6 | RCR | bits 6-7 the VIC's 64 KB bank, bits 0-3 common (shared) RAM |
//! | 7-10 | P0L, P0H, P1L, P1H | where pages 0 and 1 really are |
//! | 11 | VR | the MMU's version |
//!
//! A configuration byte (CR or a PCR):
//!
//! | Bits | Area | Meaning |
//! |---|---|---|
//! | 6 | all RAM | the RAM bank (bit 7 selects banks 2-3, which the C128 lacks) |
//! | 4-5 | `$C000-$FFFF` | 00 KERNAL, 01 internal function ROM, 10 external, 11 RAM |
//! | 2-3 | `$8000-$BFFF` | 00 BASIC high, 01 internal, 10 external, 11 RAM |
//! | 1 | `$4000-$7FFF` | 0 BASIC low, 1 RAM |
//! | 0 | `$D000-$DFFF` | 0 I/O, 1 the high area's selection (the character ROM for ROM) |

use serde::{Deserialize, Serialize};

/// What an address reaches.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Target {
    /// RAM, at an address in the 128 KB (bank in bit 16).
    Ram(u32),
    /// BASIC, by offset into the 32 KB of `basiclo` then `basichi`.
    Basic(u16),
    /// The screen editor, `$C000-$CFFF`: the first 4 KB of the KERNAL ROM.
    Editor(u16),
    /// The KERNAL proper, `$E000-$FFFF`: the last 8 KB of the KERNAL ROM.
    Kernal(u16),
    /// The character ROM at `$D000-$DFFF`, by offset into its 4 KB set.
    CharRom(u16),
    /// The Z80 BIOS at `$0000-$0FFF`: the KERNAL ROM's second 4 KB.
    Z80Bios(u16),
    /// A function ROM socket, empty on a stock C128.
    FunctionRom,
    /// The I/O chips at `$D000-$DFFF`.
    Io,
    /// Colour RAM, as the Z80 sees it at `$1000-$13FF`.
    ColourRam(u16),
    /// An MMU register through `$FF00-$FF04`.
    MmuMirror(u8),
}

/// A write to the MMU that the machine has to act on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MmuEvent {
    /// MCR bit 0 changed: the other CPU now has the bus.
    CpuSwitched,
}

/// The 8722 MMU.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct Mmu {
    /// Registers 0-10.
    regs: [u8; 11],
    /// P0H and P1H as written; they take effect when P0L or P1L is.
    p0h_latch: u8,
    p1h_latch: u8,
    /// The 40/80 DISPLAY key is down (latched, it reads in MCR bit 7).
    key_80_columns: bool,
}

impl Mmu {
    /// At reset every register is clear, so the Z80 has the bus, and pages 0
    /// and 1 are where they look.
    pub(crate) fn new() -> Self {
        let mut regs = [0; 11];
        regs[9] = 1;
        Self {
            regs,
            p0h_latch: 0,
            p1h_latch: 0,
            key_80_columns: false,
        }
    }

    /// Whether the 8502 has the bus (MCR bit 0); otherwise the Z80 has.
    pub(crate) fn cpu_8502(&self) -> bool {
        self.regs[5] & 0x01 != 0
    }

    /// The VIC's 64 KB bank (RCR bit 6).
    pub(crate) fn vic_bank(&self) -> u32 {
        u32::from((self.regs[6] >> 6) & 0x01)
    }

    /// Hold the 40/80 DISPLAY key down or let it up.
    pub(crate) fn set_80_column_key(&mut self, down: bool) {
        self.key_80_columns = down;
    }

    /// Read a register, as at `$D500 + reg`.
    pub(crate) fn read(&self, reg: u8) -> u8 {
        match reg {
            // MCR: bit 7 is the 40/80 key (1 = up), bits 4-5 the cartridge's
            // GAME and EXROM lines, high with no cartridge.
            5 => (self.regs[5] & 0x0F) | 0x30 | if self.key_80_columns { 0 } else { 0x80 },
            // The high halves of P0H and P1H are not implemented.
            8 | 10 => self.regs[usize::from(reg)] | 0xF0,
            // The version: an 8722 in a 128 KB machine.
            11 => 0x20,
            0..=10 => self.regs[usize::from(reg)],
            _ => 0xFF,
        }
    }

    /// Write a register, as at `$D500 + reg`.
    pub(crate) fn write(&mut self, reg: u8, value: u8) -> Option<MmuEvent> {
        let index = usize::from(reg);
        match reg {
            8 => self.p0h_latch = value,
            10 => self.p1h_latch = value,
            7 => {
                self.regs[7] = value;
                self.regs[8] = self.p0h_latch;
            }
            9 => {
                self.regs[9] = value;
                self.regs[10] = self.p1h_latch;
            }
            5 => {
                let switched = (self.regs[5] ^ value) & 0x01 != 0;
                self.regs[5] = value;
                return switched.then_some(MmuEvent::CpuSwitched);
            }
            0..=6 => self.regs[index] = value,
            _ => {}
        }
        None
    }

    /// Read `$FF00-$FF04`: CR, then the four PCRs.
    pub(crate) fn read_mirror(&self, index: u8) -> u8 {
        self.regs[usize::from(index)]
    }

    /// Write `$FF00-$FF04`. `$FF00` is CR; a write of anything to `$FF01-$FF04`
    /// loads that PCR into CR (the load configuration registers).
    pub(crate) fn write_mirror(&mut self, index: u8, value: u8) {
        self.regs[0] = if index == 0 {
            value
        } else {
            self.regs[usize::from(index)]
        };
    }

    /// The configuration register.
    #[cfg(test)]
    pub(crate) fn cr(&self) -> u8 {
        self.regs[0]
    }

    /// Whether I/O is mapped at `$D000-$DFFF` (CR bit 0 clear).
    pub(crate) fn io_mapped(&self) -> bool {
        self.regs[0] & 0x01 == 0
    }

    /// Where an 8502 access lands. Addresses 0 and 1, the 8502's own port,
    /// are the machine's to handle before asking.
    pub(crate) fn target_8502(&self, addr: u16) -> Target {
        let cr = self.regs[0];
        let high = (cr >> 4) & 0x03;
        match addr {
            0xFF00..=0xFF04 => Target::MmuMirror((addr - 0xFF00) as u8),
            0x4000..=0x7FFF if cr & 0x02 == 0 => Target::Basic(addr - 0x4000),
            0x8000..=0xBFFF => match (cr >> 2) & 0x03 {
                0 => Target::Basic(addr - 0x4000),
                1 | 2 => Target::FunctionRom,
                _ => Target::Ram(self.ram_address(addr)),
            },
            0xD000..=0xDFFF if cr & 0x01 == 0 => Target::Io,
            0xC000..=0xFFFF => match high {
                0 => match addr {
                    0xC000..=0xCFFF => Target::Editor(addr - 0xC000),
                    0xD000..=0xDFFF => Target::CharRom(addr - 0xD000),
                    _ => Target::Kernal(addr - 0xE000),
                },
                1 | 2 => Target::FunctionRom,
                _ => Target::Ram(self.ram_address(addr)),
            },
            _ => Target::Ram(self.ram_address(addr)),
        }
    }

    /// Where a Z80 memory access lands. The Z80 never sees BASIC or the
    /// KERNAL: in bank 0 its BIOS answers reads at `$0000-$0FFF` (writes go
    /// to the RAM under it), colour RAM sits at `$1000-$13FF` while I/O is
    /// mapped, and the rest is RAM (VICE `z80mem.c`, configurations 0-1).
    pub(crate) fn target_z80(&self, addr: u16, write: bool) -> Target {
        let bank_0 = self.regs[0] & 0x40 == 0;
        match addr {
            0xFF00..=0xFF04 => Target::MmuMirror((addr - 0xFF00) as u8),
            0x0000..=0x0FFF if bank_0 && !write => Target::Z80Bios(addr),
            0x0000..=0x0FFF if bank_0 => Target::Ram(u32::from(addr)),
            0x1000..=0x13FF if bank_0 && self.io_mapped() => Target::ColourRam(addr - 0x1000),
            _ => Target::Ram(self.ram_address(addr)),
        }
    }

    /// Common RAM: RCR bits 0-1 give its size (1, 4, 8 or 16 KB), bit 2 puts
    /// it at the bottom of memory and bit 3 at the top. It is always bank 0.
    fn shared(&self, addr: u16) -> bool {
        let rcr = self.regs[6];
        let size: u32 = match rcr & 0x03 {
            0 => 0x0400,
            n => 0x1000 << (n - 1),
        };
        let addr = u32::from(addr);
        (rcr & 0x04 != 0 && addr < size) || (rcr & 0x08 != 0 && addr >= 0x1_0000 - size)
    }

    /// The RAM bank in CR: 0 or 1 on a 128 KB machine.
    fn cr_bank(&self) -> u32 {
        u32::from((self.regs[0] >> 6) & 0x01)
    }

    /// Whether `page` in `bank` is RAM under the current configuration, as
    /// VICE's `mmu_is_valid_ram` decides it.
    fn page_is_current_ram(&self, page: u8, bank: u32) -> bool {
        if bank != self.cr_bank() {
            return false;
        }
        let cr = self.regs[0];
        let high_ram = (cr >> 4) & 0x03 == 3;
        match page {
            0x00..=0x3F => true,
            0x40..=0x7F => cr & 0x02 != 0,
            0x80..=0xBF => (cr >> 2) & 0x03 == 3,
            0xD0..=0xDF => cr & 0x01 != 0 && high_ram,
            0xFF => false,
            _ => high_ram,
        }
    }

    /// The RAM address an access to `addr` reaches: the CR bank, bank 0 in
    /// common RAM, and pages 0 and 1 relocated by P0 and P1. The relocation
    /// is VICE's `c128_mem_mmu_wrap_read` for C128 mode, case for case: page
    /// 0 or 1 goes to its pointer, and the page a pointer names comes back
    /// to page 0 or 1 when that page is RAM in the current bank.
    pub(crate) fn ram_address(&self, addr: u16) -> u32 {
        if let Some(relocated) = self.relocate(addr) {
            return relocated;
        }
        let bank = if self.shared(addr) { 0 } else { self.cr_bank() };
        (bank << 16) | u32::from(addr)
    }

    fn relocate(&self, addr: u16) -> Option<u32> {
        let (p0, p0_bank) = (self.regs[7], u32::from(self.regs[8] & 0x01));
        let (p1, p1_bank) = (self.regs[9], u32::from(self.regs[10] & 0x01));
        if p0 == 0 && p1 == 1 && p0_bank == 0 && p1_bank == 0 {
            return None;
        }
        let page = (addr >> 8) as u8;
        let zero_page_shared = self.shared(0);
        let (page, bank) = if zero_page_shared && page == 1 {
            (p1, 0)
        } else if zero_page_shared && page == 0 {
            (p0, 0)
        } else if page == 1 {
            (p1, p1_bank)
        } else if page == p1 && self.page_is_current_ram(p1, p1_bank) {
            (1, p1_bank)
        } else if page == 0 {
            (p0, p0_bank)
        } else if page == p0 && self.page_is_current_ram(p0, p0_bank) {
            (0, p0_bank)
        } else {
            return None;
        };
        Some((bank << 16) | (u32::from(page) << 8) | u32::from(addr & 0xFF))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reset_gives_the_z80_the_bus_and_the_roms_and_io() {
        let mmu = Mmu::new();
        assert!(!mmu.cpu_8502());
        assert_eq!(mmu.target_8502(0x4000), Target::Basic(0));
        assert_eq!(mmu.target_8502(0x8000), Target::Basic(0x4000));
        assert_eq!(mmu.target_8502(0xC123), Target::Editor(0x123));
        assert_eq!(mmu.target_8502(0xD020), Target::Io);
        assert_eq!(mmu.target_8502(0xE000), Target::Kernal(0));
        assert_eq!(mmu.target_8502(0xFF00), Target::MmuMirror(0));
        assert_eq!(mmu.target_8502(0x1234), Target::Ram(0x1234));
        assert_eq!(mmu.target_z80(0x0000, false), Target::Z80Bios(0));
        assert_eq!(mmu.target_z80(0x0000, true), Target::Ram(0));
        assert_eq!(mmu.target_z80(0x1000, false), Target::ColourRam(0));
    }

    #[test]
    fn cr_selects_ram_the_character_rom_and_bank_1() {
        let mut mmu = Mmu::new();
        mmu.write(0, 0x3F); // all RAM, bank 0
        assert_eq!(mmu.target_8502(0xD000), Target::Ram(0xD000));
        mmu.write(0, 0x01); // ROMs with I/O off: the character ROM
        assert_eq!(mmu.target_8502(0xD800), Target::CharRom(0x800));
        mmu.write(0, 0x7F); // all RAM, bank 1
        assert_eq!(mmu.target_8502(0x2000), Target::Ram(0x1_2000));
        mmu.write(0, 0x04);
        assert_eq!(mmu.target_8502(0x8000), Target::FunctionRom);
    }

    #[test]
    fn common_ram_is_bank_0_whatever_cr_says() {
        let mut mmu = Mmu::new();
        mmu.write(0, 0x7F);
        mmu.write(6, 0x04); // 1 KB at the bottom
        assert_eq!(mmu.ram_address(0x03FF), 0x03FF);
        assert_eq!(mmu.ram_address(0x0400), 0x1_0400);
        mmu.write(6, 0x0B); // 16 KB at the top
        assert_eq!(mmu.ram_address(0xC000), 0xC000);
        assert_eq!(mmu.ram_address(0xBFFF), 0x1_BFFF);
    }

    #[test]
    fn the_load_configuration_registers_copy_a_preset_into_cr() {
        let mut mmu = Mmu::new();
        mmu.write(1, 0x3F);
        mmu.write_mirror(1, 0x00); // the value written does not matter
        assert_eq!(mmu.cr(), 0x3F);
        mmu.write_mirror(0, 0x0E);
        assert_eq!(mmu.read_mirror(0), 0x0E);
    }

    #[test]
    fn page_pointers_take_effect_on_the_low_byte_and_swap_the_pages() {
        let mut mmu = Mmu::new();
        mmu.write(0, 0x3F);
        mmu.write(8, 0x01); // P0H latched...
        assert_eq!(mmu.read(8), 0xF0, "not yet");
        mmu.write(7, 0x20); // ...and committed with P0L: page 0 is $1:2000
        assert_eq!(mmu.read(8), 0xF1);
        assert_eq!(mmu.ram_address(0x0012), 0x1_2012);
        // Page $20 in bank 1 is not current RAM (CR is bank 0), so it stays.
        assert_eq!(mmu.ram_address(0x2012), 0x2012);
        mmu.write(8, 0x00);
        mmu.write(7, 0x20); // page 0 is $0:2000, which now swaps with page 0
        assert_eq!(mmu.ram_address(0x2012), 0x0012);
    }

    #[test]
    fn mcr_switches_cpu_and_reads_the_40_80_key() {
        let mut mmu = Mmu::new();
        assert_eq!(mmu.read(5), 0xB0, "Z80, key up, no cartridge");
        assert_eq!(mmu.write(5, 0x01), Some(MmuEvent::CpuSwitched));
        assert!(mmu.cpu_8502());
        assert_eq!(mmu.write(5, 0x01), None);
        mmu.set_80_column_key(true);
        assert_eq!(mmu.read(5), 0x31);
        assert_eq!(mmu.read(11), 0x20);
        assert_eq!(mmu.read(12), 0xFF);
    }
}
