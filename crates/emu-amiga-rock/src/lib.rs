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

        // CIA-A PRA external inputs (active-low accent signals):
        //   Bit 7: /FIR1 = 1 (joystick fire not pressed)
        //   Bit 6: /FIR0 = 1 (joystick fire not pressed)
        //   Bit 5: /DSKRDY = 1 (drive not ready)
        //   Bit 4: /DSKTRACK0 = 0 (at track 0)
        //   Bit 3: /DSKPROT = 1 (not write protected)
        //   Bit 2: /DSKCHANGE = 0 (disk removed / changed)
        //   Bits 1,0: LED/OVL outputs, external pull-up = 1,1
        let mut cia_a = Cia::new();
        cia_a.external_a = 0xEB; // 0b_1110_1011

        Self {
            master_clock: 0,
            cpu,
            agnus: Agnus::new(),
            memory,
            denise: Denise::new(),
            copper: Copper::new(),
            cia_a,
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
        write_custom_register(
            &mut self.agnus, &mut self.denise, &mut self.copper,
            &mut self.paula, &mut self.memory, offset, val,
        );
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
                write_custom_register(
                    self.agnus, self.denise, self.copper,
                    self.paula, self.memory, offset, val,
                );
            } else {
                match offset {
                    // DMACONR: DMA control (active bits) + blitter busy/zero
                    0x002 => {
                        let busy = if self.agnus.blitter_busy { 0x4000 } else { 0 };
                        return BusStatus::Ready(self.agnus.dmacon | busy);
                    }
                    // VPOSR: LOF | Agnus ID | V8
                    // Bit 15: LOF (long frame, always 1 for PAL interlace long field — set to 0 for now)
                    // Bits 14-8: Agnus chip ID (OCS PAL = 0x00)
                    // Bit 0: V8 (bit 8 of vpos)
                    0x004 => return BusStatus::Ready((self.agnus.vpos >> 8) & 1),
                    // VHPOSR: V7-V0 in high byte, H8-H1 in low byte
                    0x006 => return BusStatus::Ready(((self.agnus.vpos & 0xFF) << 8) | (self.agnus.hpos & 0xFF)),
                    // JOY0DAT, JOY1DAT: joystick/mouse — no input
                    0x00A | 0x00C => return BusStatus::Ready(0),
                    // ADKCONR: audio/disk control read
                    0x010 => return BusStatus::Ready(self.paula.adkcon),
                    // POTGOR: active-high button bits, active-low accent. $FF00 = no buttons pressed.
                    0x016 => return BusStatus::Ready(0xFF00),
                    // SERDATR ($DFF018): serial port data and status.
                    // With nothing connected, the RXD pin floats high
                    // (pull-up on the A500). The shift register sees all 1s.
                    // Bit 13: TBE (transmit buffer empty)
                    // Bit 12: TSRE (transmit shift register empty)
                    // Bit 11: RXD (pin state = high/idle)
                    // Bits 8-0: $1FF (all 1s from idle line)
                    0x018 => return BusStatus::Ready(0x39FF),
                    // DSKBYTR: disk byte and status — no disk, nothing ready
                    0x01A => return BusStatus::Ready(0),
                    0x01C => return BusStatus::Ready(self.paula.intena),
                    0x01E => return BusStatus::Ready(self.paula.intreq),
                    // DENISEID: OCS Denise = open bus ($FFFF)
                    0x07C => return BusStatus::Ready(0xFFFF),
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

/// Shared custom register write dispatch used by both CPU and copper paths.
fn write_custom_register(
    agnus: &mut Agnus, denise: &mut Denise, copper: &mut Copper,
    paula: &mut Paula, memory: &mut Memory, offset: u16, val: u16,
) {
    match offset {
        // Blitter registers
        0x040 => agnus.bltcon0 = val,
        0x042 => agnus.bltcon1 = val,
        0x044 => agnus.blt_afwm = val,
        0x046 => agnus.blt_alwm = val,
        0x048 => agnus.blt_cpt = (agnus.blt_cpt & 0x0000FFFF) | (u32::from(val) << 16),
        0x04A => agnus.blt_cpt = (agnus.blt_cpt & 0xFFFF0000) | u32::from(val & 0xFFFE),
        0x04C => agnus.blt_bpt = (agnus.blt_bpt & 0x0000FFFF) | (u32::from(val) << 16),
        0x04E => agnus.blt_bpt = (agnus.blt_bpt & 0xFFFF0000) | u32::from(val & 0xFFFE),
        0x050 => agnus.blt_apt = (agnus.blt_apt & 0x0000FFFF) | (u32::from(val) << 16),
        0x052 => agnus.blt_apt = (agnus.blt_apt & 0xFFFF0000) | u32::from(val & 0xFFFE),
        0x054 => agnus.blt_dpt = (agnus.blt_dpt & 0x0000FFFF) | (u32::from(val) << 16),
        0x056 => agnus.blt_dpt = (agnus.blt_dpt & 0xFFFF0000) | u32::from(val & 0xFFFE),
        0x058 => {
            agnus.bltsize = val;
            agnus.blitter_busy = true;
            execute_blit(agnus, paula, memory);
        }
        0x060 => agnus.blt_cmod = val as i16,
        0x062 => agnus.blt_bmod = val as i16,
        0x064 => agnus.blt_amod = val as i16,
        0x066 => agnus.blt_dmod = val as i16,
        0x070 => agnus.blt_cdat = val,
        0x072 => agnus.blt_bdat = val,
        0x074 => agnus.blt_adat = val,

        // Copper
        0x080 => copper.cop1lc = (copper.cop1lc & 0x0000FFFF) | (u32::from(val) << 16),
        0x082 => copper.cop1lc = (copper.cop1lc & 0xFFFF0000) | u32::from(val & 0xFFFE),
        0x084 => copper.cop2lc = (copper.cop2lc & 0x0000FFFF) | (u32::from(val) << 16),
        0x086 => copper.cop2lc = (copper.cop2lc & 0xFFFF0000) | u32::from(val & 0xFFFE),
        0x088 => copper.restart_cop1(),
        0x08A => copper.restart_cop2(),

        // Display
        0x08E => agnus.diwstrt = val,
        0x090 => agnus.diwstop = val,
        0x092 => agnus.ddfstrt = val,
        0x094 => agnus.ddfstop = val,

        // DMA control
        0x096 => {
            if val & 0x8000 != 0 { agnus.dmacon |= val & 0x7FFF; }
            else { agnus.dmacon &= !(val & 0x7FFF); }
        }

        // Interrupts
        0x09A => paula.write_intena(val),
        0x09C => paula.write_intreq(val),

        // Audio/disk control
        0x09E => paula.write_adkcon(val),

        // Disk
        0x024 => paula.write_dsklen(val),
        0x07E => paula.dsksync = val,

        // Serial (discard)
        0x030 | 0x032 => {}

        // Copper danger
        0x02E => copper.danger = val & 0x02 != 0,

        // Bitplane control
        0x100 => agnus.bplcon0 = val,
        0x102 => denise.bplcon1 = val,
        0x104 => denise.bplcon2 = val,

        // Bitplane modulos
        0x108 => agnus.bpl1mod = val as i16,
        0x10A => agnus.bpl2mod = val as i16,

        // Bitplane pointers ($0E0-$0EE)
        0x0E0..=0x0EE => {
            let idx = ((offset - 0x0E0) / 4) as usize;
            if offset & 2 == 0 { agnus.bpl_pt[idx] = (agnus.bpl_pt[idx] & 0x0000FFFF) | (u32::from(val) << 16); }
            else { agnus.bpl_pt[idx] = (agnus.bpl_pt[idx] & 0xFFFF0000) | u32::from(val & 0xFFFE); }
        }

        // Sprite pointers ($120-$13E)
        0x120..=0x13E => {
            let idx = ((offset - 0x120) / 4) as usize;
            if idx < 8 {
                if offset & 2 == 0 { agnus.spr_pt[idx] = (agnus.spr_pt[idx] & 0x0000FFFF) | (u32::from(val) << 16); }
                else { agnus.spr_pt[idx] = (agnus.spr_pt[idx] & 0xFFFF0000) | u32::from(val & 0xFFFE); }
            }
        }

        // Sprite data ($140-$17E): 8 sprites x 4 regs (POS, CTL, DATA, DATB)
        0x140..=0x17E => {
            let sprite = ((offset - 0x140) / 8) as usize;
            let reg = ((offset - 0x140) % 8) / 2;
            if sprite < 8 {
                match reg {
                    0 => denise.spr_pos[sprite] = val,
                    1 => denise.spr_ctl[sprite] = val,
                    2 => denise.spr_data[sprite] = val,
                    3 => denise.spr_datb[sprite] = val,
                    _ => {}
                }
            }
        }

        // Color palette ($180-$1BE)
        0x180..=0x1BE => {
            let idx = ((offset - 0x180) / 2) as usize;
            denise.set_palette(idx, val);
        }

        // Audio channels ($0A0-$0D4): accept and discard
        0x0A0..=0x0D4 => {}

        _ => {}
    }
}

/// Execute a blitter operation synchronously.
///
/// On real hardware the blitter runs in DMA slots over many CCKs. For boot
/// purposes we run the entire operation instantly when BLTSIZE is written,
/// then clear busy and fire the BLIT interrupt.
fn execute_blit(agnus: &mut Agnus, paula: &mut Paula, memory: &mut Memory) {
    let height = (agnus.bltsize >> 6) & 0x3FF;
    let width_words = agnus.bltsize & 0x3F;
    let height = if height == 0 { 1024 } else { height } as u32;
    let width_words = if width_words == 0 { 64 } else { width_words } as u32;

    let use_a = agnus.bltcon0 & 0x0800 != 0;
    let use_b = agnus.bltcon0 & 0x0400 != 0;
    let use_c = agnus.bltcon0 & 0x0200 != 0;
    let use_d = agnus.bltcon0 & 0x0100 != 0;
    let lf = agnus.bltcon0 as u8; // minterm function (low 8 bits)
    let a_shift = (agnus.bltcon0 >> 12) & 0xF;
    let b_shift = (agnus.bltcon1 >> 12) & 0xF;
    let desc = agnus.bltcon1 & 0x0002 != 0;

    let mut apt = agnus.blt_apt;
    let mut bpt = agnus.blt_bpt;
    let mut cpt = agnus.blt_cpt;
    let mut dpt = agnus.blt_dpt;

    let read_word = |mem: &Memory, addr: u32| -> u16 {
        let hi = mem.read_chip_byte(addr);
        let lo = mem.read_chip_byte(addr.wrapping_add(1));
        (u16::from(hi) << 8) | u16::from(lo)
    };

    let write_word = |mem: &mut Memory, addr: u32, val: u16| {
        mem.write_byte(addr, (val >> 8) as u8);
        mem.write_byte(addr.wrapping_add(1), val as u8);
    };

    let ptr_step: i32 = if desc { -2 } else { 2 };

    for _row in 0..height {
        let mut a_prev: u16 = 0;
        let mut b_prev: u16 = 0;

        for col in 0..width_words {
            // Read source channels
            let a_raw = if use_a { let w = read_word(&*memory, apt); apt = (apt as i32 + ptr_step) as u32; w } else { agnus.blt_adat };
            let b_raw = if use_b { let w = read_word(&*memory, bpt); bpt = (bpt as i32 + ptr_step) as u32; w } else { agnus.blt_bdat };
            let c_val = if use_c { let w = read_word(&*memory, cpt); cpt = (cpt as i32 + ptr_step) as u32; w } else { agnus.blt_cdat };

            // Apply first/last word masks to A channel
            let mut a_masked = a_raw;
            if col == 0 { a_masked &= agnus.blt_afwm; }
            if col == width_words - 1 { a_masked &= agnus.blt_alwm; }

            // Barrel shift A: combine with previous word
            let a_combined = (u32::from(a_prev) << 16) | u32::from(a_masked);
            let a_shifted = if desc {
                // DESC mode: shift left
                (a_combined >> (16 - a_shift)) as u16
            } else {
                (a_combined >> a_shift) as u16
            };

            // Barrel shift B: combine with previous word
            let b_combined = (u32::from(b_prev) << 16) | u32::from(b_raw);
            let b_shifted = if desc {
                (b_combined >> (16 - b_shift)) as u16
            } else {
                (b_combined >> b_shift) as u16
            };

            a_prev = a_masked;
            b_prev = b_raw;

            // Compute minterm for each bit
            let mut result: u16 = 0;
            for bit in 0..16 {
                let a_bit = (a_shifted >> bit) & 1;
                let b_bit = (b_shifted >> bit) & 1;
                let c_bit = (c_val >> bit) & 1;
                let index = (a_bit << 2) | (b_bit << 1) | c_bit;
                if (lf >> index) & 1 != 0 {
                    result |= 1 << bit;
                }
            }

            // Write D channel
            if use_d {
                write_word(memory, dpt, result);
                dpt = (dpt as i32 + ptr_step) as u32;
            }
        }

        // Apply modulos at end of each row
        let mod_dir: i32 = if desc { -1 } else { 1 };
        if use_a { apt = (apt as i32 + i32::from(agnus.blt_amod) * mod_dir) as u32; }
        if use_b { bpt = (bpt as i32 + i32::from(agnus.blt_bmod) * mod_dir) as u32; }
        if use_c { cpt = (cpt as i32 + i32::from(agnus.blt_cmod) * mod_dir) as u32; }
        if use_d { dpt = (dpt as i32 + i32::from(agnus.blt_dmod) * mod_dir) as u32; }
    }

    // Update pointer registers
    agnus.blt_apt = apt;
    agnus.blt_bpt = bpt;
    agnus.blt_cpt = cpt;
    agnus.blt_dpt = dpt;

    agnus.blitter_busy = false;
    paula.request_interrupt(6); // bit 6 = BLIT
}
