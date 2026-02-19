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
pub mod cia;

use crate::agnus::{Agnus, SlotOwner};
use crate::memory::Memory;
use crate::denise::Denise;
use crate::copper::Copper;
use crate::cia::Cia;
use crate::paula::Paula;
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
/// Number of crystal ticks per CIA E-clock
pub const TICKS_PER_ECLOCK: u64 = 40;

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
    pub cia_a: Cia,
    pub cia_b: Cia,
    pub paula: Paula,
}

impl Amiga {
    pub fn new(kickstart: Vec<u8>) -> Self {
        let mut cpu = Cpu68000::new();
        let memory = Memory::new(512 * 1024, kickstart);
        
        // Initial reset vectors come from ROM (overlay is ON at power-on,
        // mapping Kickstart to $000000).
        let ssp = (u32::from(memory.kickstart[0]) << 24) |
                  (u32::from(memory.kickstart[1]) << 16) |
                  (u32::from(memory.kickstart[2]) << 8)  |
                   u32::from(memory.kickstart[3]);
        let pc  = (u32::from(memory.kickstart[4]) << 24) |
                  (u32::from(memory.kickstart[5]) << 16) |
                  (u32::from(memory.kickstart[6]) << 8)  |
                   u32::from(memory.kickstart[7]);

        cpu.reset_to(ssp, pc);

        Self {
            master_clock: 0,
            cpu,
            agnus: Agnus::new(),
            memory,
            denise: Denise::new(),
            copper: Copper::new(),
            cia_a: Cia::new(),
            cia_b: Cia::new(),
            paula: Paula::new(),
        }
    }

    pub fn tick(&mut self) {
        self.master_clock += 1;

        if self.master_clock % TICKS_PER_CCK == 0 {
            let vpos = self.agnus.vpos;
            let hpos = self.agnus.hpos;
            
            // VERTB fires at the start of vblank (beam at line 0, start of frame).
            // The check runs before tick_cck(), so vpos/hpos reflect the current
            // beam position. vpos=0, hpos=0 means the beam just wrapped from the
            // end of the previous frame.
            if vpos == 0 && hpos == 0 {
                self.paula.request_interrupt(5); // bit 5 = VERTB
            }

            let slot = self.agnus.current_slot();
            match slot {
                SlotOwner::Bitplane(plane) => {
                    let idx = plane as usize;
                    let addr = self.agnus.bpl_pt[idx];
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
                        if reg == 0x09C && (val & 0x0010) != 0 { self.paula.request_interrupt(4); }
                        self.write_custom_reg(reg, val);
                    }
                }
                _ => {}
            }

            if let Some((fb_x, fb_y)) = self.beam_to_fb(vpos, hpos) {
                self.denise.output_pixel(fb_x, fb_y);
                self.denise.output_pixel(fb_x + 1, fb_y);
            }

            self.agnus.tick_cck();
        }

        if self.master_clock % TICKS_PER_CPU == 0 {
            let mut bus = AmigaBusWrapper {
                agnus: &mut self.agnus, memory: &mut self.memory, denise: &mut self.denise,
                copper: &mut self.copper, cia_a: &mut self.cia_a, cia_b: &mut self.cia_b, paula: &mut self.paula,
            };
            self.cpu.tick(&mut bus, self.master_clock);
        }

        if self.master_clock % TICKS_PER_ECLOCK == 0 {
            self.cia_a.tick();
            if self.cia_a.irq_active() { self.paula.request_interrupt(3); }
            self.cia_b.tick();
            if self.cia_b.irq_active() { self.paula.request_interrupt(13); }
        }
    }

    pub fn write_custom_reg(&mut self, offset: u16, val: u16) {
        match offset {
            0x040 => self.agnus.bltcon0 = val,
            0x042 => self.agnus.bltcon1 = val,
            0x058 => self.agnus.start_blitter(val),
            0x080 => self.copper.cop1lc = (self.copper.cop1lc & 0x0000FFFF) | (u32::from(val) << 16),
            0x082 => self.copper.cop1lc = (self.copper.cop1lc & 0xFFFF0000) | u32::from(val & 0xFFFE),
            0x084 => self.copper.cop2lc = (self.copper.cop2lc & 0x0000FFFF) | (u32::from(val) << 16),
            0x086 => self.copper.cop2lc = (self.copper.cop2lc & 0xFFFF0000) | u32::from(val & 0xFFFE),
            0x088 => self.copper.restart_cop1(),
            0x08A => self.copper.restart_cop2(),
            0x092 => self.agnus.ddfstrt = val,
            0x094 => self.agnus.ddfstop = val,
            0x096 => {
                if val & 0x8000 != 0 { self.agnus.dmacon |= val & 0x7FFF; }
                else { self.agnus.dmacon &= !(val & 0x7FFF); }
            }
            0x09A => self.paula.write_intena(val),
            0x09C => self.paula.write_intreq(val),
            0x100 => self.agnus.bplcon0 = val,
            0x0E0..=0x0EE => {
                let idx = ((offset - 0x0E0) / 4) as usize;
                if offset & 2 == 0 { self.agnus.bpl_pt[idx] = (self.agnus.bpl_pt[idx] & 0x0000FFFF) | (u32::from(val) << 16); }
                else { self.agnus.bpl_pt[idx] = (self.agnus.bpl_pt[idx] & 0xFFFF0000) | u32::from(val & 0xFFFE); }
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
        if fb_y >= crate::denise::FB_HEIGHT as u16 { return None; }
        let cck_offset = hpos_cck.wrapping_sub(DISPLAY_HSTART_CCK);
        let fb_x = u32::from(cck_offset) * 2;
        if fb_x + 1 >= crate::denise::FB_WIDTH { return None; }
        Some((fb_x, u32::from(fb_y)))
    }
}

pub struct AmigaBusWrapper<'a> {
    pub agnus: &'a mut Agnus, pub memory: &'a mut Memory, pub denise: &'a mut Denise,
    pub copper: &'a mut Copper, pub cia_a: &'a mut Cia, pub cia_b: &'a mut Cia, pub paula: &'a mut Paula,
}

impl<'a> M68kBus for AmigaBusWrapper<'a> {
    fn poll_ipl(&mut self) -> u8 { self.paula.compute_ipl() }
    fn poll_interrupt_ack(&mut self, level: u8) -> BusStatus { BusStatus::Ready(24 + level as u16) }
    fn reset(&mut self) {
        // RESET instruction asserts the hardware reset line for 124 CPU cycles.
        // This resets all peripherals to their power-on state.
        self.cia_a.reset();
        self.cia_b.reset();
        // After CIA-A reset, DDR-A = 0 (all inputs). On the A500, the /OVL
        // pin has a pull-up resistor, so with CIA-A not driving it, overlay
        // defaults to ON — ROM mapped at $0.
        self.memory.overlay = true;
        // Reset custom chip state
        self.paula.intreq = 0;
        self.paula.intena = 0;
        self.agnus.dmacon = 0;
    }

    fn poll_cycle(&mut self, addr: u32, _fc: FunctionCode, is_read: bool, is_word: bool, data: Option<u16>) -> BusStatus {
        let addr = addr & 0xFFFFFF;

        // CIA-A ($BFE001, odd bytes)
        if (addr & 0xFFF000) == 0xBFE000 {
            let reg = ((addr >> 8) & 0x0F) as u8;
            if is_read {
                if addr & 1 != 0 { return BusStatus::Ready(u16::from(self.cia_a.read(reg))); }
                return BusStatus::Ready(0xFF00);
            } else {
                if addr & 1 != 0 {
                    let val = data.unwrap_or(0) as u8;
                    self.cia_a.write(reg, val);
                    if reg == 0 {
                        let out = self.cia_a.port_a_output();
                        self.memory.overlay = out & 0x01 != 0;
                    }
                }
                return BusStatus::Ready(0);
            }
        }

        // CIA-B ($BFD000, even bytes)
        if (addr & 0xFFF000) == 0xBFD000 {
            let reg = ((addr >> 8) & 0x0F) as u8;
            if is_read {
                if addr & 1 == 0 { return BusStatus::Ready(u16::from(self.cia_b.read(reg)) << 8 | 0x00FF); }
                return BusStatus::Ready(0x00FF);
            } else {
                if addr & 1 == 0 { self.cia_b.write(reg, (data.unwrap_or(0) >> 8) as u8); }
                return BusStatus::Ready(0);
            }
        }

        // Custom Registers ($DFF000)
        if (addr & 0xFFF000) == 0xDFF000 {
            let offset = (addr & 0x1FE) as u16;
            if !is_read {
                let val = data.unwrap_or(0);
                match offset {
                    0x040 => self.agnus.bltcon0 = val,
                    0x042 => self.agnus.bltcon1 = val,
                    0x058 => self.agnus.start_blitter(val),
                    0x080 => self.copper.cop1lc = (self.copper.cop1lc & 0x0000FFFF) | (u32::from(val) << 16),
                    0x082 => self.copper.cop1lc = (self.copper.cop1lc & 0xFFFF0000) | u32::from(val & 0xFFFE),
                    0x084 => self.copper.cop2lc = (self.copper.cop2lc & 0x0000FFFF) | (u32::from(val) << 16),
                    0x086 => self.copper.cop2lc = (self.copper.cop2lc & 0xFFFF0000) | u32::from(val & 0xFFFE),
                    0x088 => self.copper.restart_cop1(),
                    0x08A => self.copper.restart_cop2(),
                    0x092 => self.agnus.ddfstrt = val,
                    0x094 => self.agnus.ddfstop = val,
                    0x096 => {
                        if val & 0x8000 != 0 { self.agnus.dmacon |= val & 0x7FFF; }
                        else { self.agnus.dmacon &= !(val & 0x7FFF); }
                    }
                    0x09A => self.paula.write_intena(val),
                    0x09C => self.paula.write_intreq(val),
                    0x100 => self.agnus.bplcon0 = val,
                    0x0E0..=0x0EE => {
                        let idx = ((offset - 0x0E0) / 4) as usize;
                        if offset & 2 == 0 { self.agnus.bpl_pt[idx] = (self.agnus.bpl_pt[idx] & 0x0000FFFF) | (u32::from(val) << 16); }
                        else { self.agnus.bpl_pt[idx] = (self.agnus.bpl_pt[idx] & 0xFFFF0000) | u32::from(val & 0xFFFE); }
                    }
                    0x180..=0x1BE => {
                        let idx = ((offset - 0x180) / 2) as usize;
                        self.denise.set_palette(idx, val);
                    }
                    _ => {}
                }
            } else {
                match offset {
                    0x002 => {
                        let busy = if self.agnus.blitter_busy { 0x4000 } else { 0 };
                        return BusStatus::Ready(busy | (self.agnus.vpos & 0xFF));
                    }
                    0x004 => return BusStatus::Ready((self.agnus.vpos >> 8) << 15 | self.agnus.hpos),
                    0x01A => return BusStatus::Ready(self.agnus.dmacon),
                    0x01C => return BusStatus::Ready(self.paula.intena),
                    0x01E => return BusStatus::Ready(self.paula.intreq),
                    // SERDATR ($DFF018): serial port data and status.
                    // With nothing connected, the RXD pin floats high
                    // (pull-up on the A500). The shift register sees all 1s.
                    // Bit 13: TBE (transmit buffer empty)
                    // Bit 12: TSRE (transmit shift register empty)
                    // Bit 11: RXD (pin state = high/idle)
                    // Bits 8-0: $1FF (all 1s from idle line)
                    0x018 => return BusStatus::Ready(0x39FF),
                    _ => {}
                }
            }
            return BusStatus::Ready(0);
        }

        if addr < 0x200000 {
            match self.agnus.current_slot() {
                SlotOwner::Cpu => {
                    if is_read {
                        let val = if is_word {
                            let hi = self.memory.read_byte(addr);
                            let lo = self.memory.read_byte(addr | 1);
                            (u16::from(hi) << 8) | u16::from(lo)
                        } else { u16::from(self.memory.read_byte(addr)) };
                        BusStatus::Ready(val)
                    } else {
                        let val = data.unwrap_or(0);
                        if is_word { self.memory.write_byte(addr, (val >> 8) as u8); self.memory.write_byte(addr | 1, val as u8); }
                        else { self.memory.write_byte(addr, val as u8); }
                        BusStatus::Ready(0)
                    }
                }
                _ => BusStatus::Wait,
            }
        } else {
            if is_read {
                let val = if is_word {
                    let hi = self.memory.read_byte(addr);
                    let lo = self.memory.read_byte(addr | 1);
                    (u16::from(hi) << 8) | u16::from(lo)
                } else { u16::from(self.memory.read_byte(addr)) };
                BusStatus::Ready(val)
            } else { BusStatus::Ready(0) }
        }
    }
}
