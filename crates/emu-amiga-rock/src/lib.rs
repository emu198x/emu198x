//! The "Rock" - A Cycle-Strict Amiga Emulator.
//!
//! Foundation: Crystal-accuracy.
//! Bus Model: Reactive (Request/Acknowledge), not Predictive.
//! CPU Model: Ticks every 4 crystal cycles, polls bus until DTACK.

pub mod config;
pub mod bus;
pub mod agnus;
pub mod denise;
pub mod paula;
pub mod memory;
pub mod copper;

use crate::agnus::{Agnus, SlotOwner};
use crate::memory::Memory;
use crate::denise::Denise;
use crate::copper::Copper;
use cpu_m68k_rock::cpu::Cpu68000;
use cpu_m68k_rock::bus::{M68kBus, FunctionCode, BusStatus};

/// Standard Amiga PAL Master Crystal Frequency (Hz)
pub const PAL_CRYSTAL_HZ: u64 = 28_375_160;
/// Standard Amiga NTSC Master Crystal Frequency (Hz)
pub const NTSC_CRYSTAL_HZ: u64 = 28_636_360;

/// Number of crystal ticks per Colour Clock (CCK)
pub const TICKS_PER_CCK: u64 = 8;
/// Number of crystal ticks per CPU Cycle
pub const TICKS_PER_CPU: u64 = 4;

/// Display window constants for framebuffer coordinate mapping.
const DISPLAY_VSTART: u16 = 0x2C;
const DISPLAY_HSTART_CCK: u16 = 0x2E;

pub struct Amiga {
    pub master_clock: u64,
    pub cpu: Cpu68000,
    pub agnus: Agnus,
    pub memory: Memory,
    pub denise: Denise,
    pub copper: Copper,
}

impl Amiga {
    pub fn new(kickstart: Vec<u8>) -> Self {
        let mut cpu = Cpu68000::new();
        let memory = Memory::new(512 * 1024, kickstart);
        
        // Initial reset vectors are read from memory via overlay
        let ssp = (u32::from(memory.read_byte(0)) << 24) |
                  (u32::from(memory.read_byte(1)) << 16) |
                  (u32::from(memory.read_byte(2)) << 8)  |
                   u32::from(memory.read_byte(3));
        let pc  = (u32::from(memory.read_byte(4)) << 24) |
                  (u32::from(memory.read_byte(5)) << 16) |
                  (u32::from(memory.read_byte(6)) << 8)  |
                   u32::from(memory.read_byte(7));

        cpu.reset_to(ssp, pc);

        Self {
            master_clock: 0,
            cpu,
            agnus: Agnus::new(),
            memory,
            denise: Denise::new(),
            copper: Copper::new(),
        }
    }

    pub fn tick(&mut self) {
        self.master_clock += 1;

        // 1. Tick Agnus/DMA (Every 8 ticks)
        if self.master_clock % TICKS_PER_CCK == 0 {
            let vpos = self.agnus.vpos;
            let hpos = self.agnus.hpos;
            
            // Determine slot owner BEFORE ticking Agnus
            let slot = self.agnus.current_slot();
            match slot {
                SlotOwner::Bitplane(plane) => {
                    let idx = plane as usize;
                    let addr = self.agnus.bpl_pt[idx];
                    // Cycle-strict read from memory (ignoring overlay for DMA)
                    let hi = self.memory.read_chip_byte(addr);
                    let lo = self.memory.read_chip_byte(addr | 1);
                    let val = (u16::from(hi) << 8) | u16::from(lo);
                    self.denise.load_bitplane(idx, val);
                    self.agnus.bpl_pt[idx] = addr.wrapping_add(2);
                }
                SlotOwner::Copper => {
                    let res = {
                        let memory = &self.memory;
                        self.copper.tick(vpos, hpos, |addr| {
                            let hi = memory.read_chip_byte(addr);
                            let lo = memory.read_chip_byte(addr | 1);
                            (u16::from(hi) << 8) | u16::from(lo)
                        })
                    };
                    if let Some((reg, val)) = res {
                        self.write_custom_reg(reg, val);
                    }
                }
                _ => {}
            }

            // Denise: output 2 lores pixels per CCK
            if let Some((fb_x, fb_y)) = self.beam_to_fb(vpos, hpos) {
                self.denise.output_pixel(fb_x, fb_y);
                self.denise.output_pixel(fb_x + 1, fb_y);
            }

            self.agnus.tick_cck();
        }

        // 2. Tick CPU (Every 4 ticks)
        if self.master_clock % TICKS_PER_CPU == 0 {
            let mut bus = AmigaBusWrapper {
                agnus: &mut self.agnus,
                memory: &mut self.memory,
                denise: &mut self.denise,
                copper: &mut self.copper,
            };
            self.cpu.tick(&mut bus, self.master_clock);
        }
    }

    pub fn write_custom_reg(&mut self, offset: u16, val: u16) {
        match offset {
            0x080 => self.copper.cop1lc = (self.copper.cop1lc & 0x0000FFFF) | (u32::from(val) << 16),
            0x082 => self.copper.cop1lc = (self.copper.cop1lc & 0xFFFF0000) | u32::from(val & 0xFFFE),
            0x084 => self.copper.cop2lc = (self.copper.cop2lc & 0x0000FFFF) | (u32::from(val) << 16),
            0x086 => self.copper.cop2lc = (self.copper.cop2lc & 0xFFFF0000) | u32::from(val & 0xFFFE),
            0x088 => self.copper.restart_cop1(),
            0x08A => self.copper.restart_cop2(),
            0x092 => self.agnus.ddfstrt = val,
            0x094 => self.agnus.ddfstop = val,
            0x096 => { // DMACON
                if val & 0x8000 != 0 {
                    self.agnus.dmacon |= val & 0x7FFF;
                } else {
                    self.agnus.dmacon &= !(val & 0x7FFF);
                }
            }
            0x100 => {
                self.agnus.bplcon0 = val;
            }
            0x0E0..=0x0EE => { // BPLxPT
                let idx = ((offset - 0x0E0) / 4) as usize;
                if offset & 2 == 0 { // High word
                    self.agnus.bpl_pt[idx] = (self.agnus.bpl_pt[idx] & 0x0000FFFF) | (u32::from(val) << 16);
                } else { // Low word
                    self.agnus.bpl_pt[idx] = (self.agnus.bpl_pt[idx] & 0xFFFF0000) | u32::from(val & 0xFFFE);
                }
            }
            0x180..=0x1BE => {
                let idx = ((offset - 0x180) / 2) as usize;
                self.denise.set_palette(idx, val);
            }
            _ => {}
        }
    }

    fn beam_to_fb(&self, vpos: u16, hpos_cck: u16) -> Option<(u32, u32)> {
        let fb_y = vpos.wrapping_sub(DISPLAY_VSTART);
        if fb_y >= crate::denise::FB_HEIGHT as u16 {
            return None;
        }

        let cck_offset = hpos_cck.wrapping_sub(DISPLAY_HSTART_CCK);
        let fb_x = u32::from(cck_offset) * 2;
        if fb_x + 1 >= crate::denise::FB_WIDTH {
            return None;
        }

        Some((fb_x, u32::from(fb_y)))
    }
}

pub struct AmigaBusWrapper<'a> {
    pub agnus: &'a mut Agnus,
    pub memory: &'a mut Memory,
    pub denise: &'a mut Denise,
    pub copper: &'a mut Copper,
}

impl<'a> M68kBus for AmigaBusWrapper<'a> {
    fn poll_cycle(
        &mut self,
        addr: u32,
        _fc: FunctionCode,
        is_read: bool,
        is_word: bool,
        data: Option<u16>,
    ) -> BusStatus {
        let addr = addr & 0xFFFFFF;

        // Custom Registers ($DFF000)
        if (addr & 0xFFF000) == 0xDFF000 {
            let offset = (addr & 0x1FE) as u16;
            if !is_read {
                let val = data.unwrap_or(0);
                // We need a way to call write_custom_reg from here.
                // Since we don't have access to Amiga, we duplicate the match or move it to a helper.
                // For now, let's duplicate the logic or use a shared helper.
                match offset {
                    0x080 => self.copper.cop1lc = (self.copper.cop1lc & 0x0000FFFF) | (u32::from(val) << 16),
                    0x082 => self.copper.cop1lc = (self.copper.cop1lc & 0xFFFF0000) | u32::from(val & 0xFFFE),
                    0x084 => self.copper.cop2lc = (self.copper.cop2lc & 0x0000FFFF) | (u32::from(val) << 16),
                    0x086 => self.copper.cop2lc = (self.copper.cop2lc & 0xFFFF0000) | u32::from(val & 0xFFFE),
                    0x088 => self.copper.restart_cop1(),
                    0x08A => self.copper.restart_cop2(),
                    0x092 => self.agnus.ddfstrt = val,
                    0x094 => self.agnus.ddfstop = val,
                    0x096 => { // DMACON
                        if val & 0x8000 != 0 {
                            self.agnus.dmacon |= val & 0x7FFF;
                        } else {
                            self.agnus.dmacon &= !(val & 0x7FFF);
                        }
                    }
                    0x100 => {
                        self.agnus.bplcon0 = val;
                    }
                    0x0E0..=0x0EE => { // BPLxPT
                        let idx = ((offset - 0x0E0) / 4) as usize;
                        if offset & 2 == 0 { // High word
                            self.agnus.bpl_pt[idx] = (self.agnus.bpl_pt[idx] & 0x0000FFFF) | (u32::from(val) << 16);
                        } else { // Low word
                            self.agnus.bpl_pt[idx] = (self.agnus.bpl_pt[idx] & 0xFFFF0000) | u32::from(val & 0xFFFE);
                        }
                    }
                    0x180..=0x1BE => {
                        let idx = ((offset - 0x180) / 2) as usize;
                        self.denise.set_palette(idx, val);
                    }
                    _ => {}
                }
            }
            return BusStatus::Ready(0);
        }

        // Chip RAM contention check
        if addr < 0x200000 {
            match self.agnus.current_slot() {
                SlotOwner::Cpu => {
                    // CPU has the bus!
                    if is_read {
                        let val = if is_word {
                            let hi = self.memory.read_byte(addr);
                            let lo = self.memory.read_byte(addr | 1);
                            (u16::from(hi) << 8) | u16::from(lo)
                        } else {
                            u16::from(self.memory.read_byte(addr))
                        };
                        BusStatus::Ready(val)
                    } else {
                        let val = data.unwrap_or(0);
                        if is_word {
                            self.memory.write_byte(addr, (val >> 8) as u8);
                            self.memory.write_byte(addr | 1, val as u8);
                        } else {
                            self.memory.write_byte(addr, val as u8);
                        }
                        BusStatus::Ready(0)
                    }
                }
                _ => BusStatus::Wait,
            }
        } else {
            // ROM or other: no contention
            if is_read {
                let val = if is_word {
                    let hi = self.memory.read_byte(addr);
                    let lo = self.memory.read_byte(addr | 1);
                    (u16::from(hi) << 8) | u16::from(lo)
                } else {
                    u16::from(self.memory.read_byte(addr))
                };
                BusStatus::Ready(val)
            } else {
                // Ignore writes to ROM
                BusStatus::Ready(0)
            }
        }
    }

    fn poll_interrupt_ack(&mut self, _level: u8) -> BusStatus {
        BusStatus::Ready(24 + _level as u16) // Autovector stub
    }

    fn reset(&mut self) {
        // Handle system reset
    }
}
