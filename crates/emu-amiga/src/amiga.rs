//! Top-level Amiga 500 PAL system.
//!
//! Crystal: 28,375,160 Hz. Every 8 ticks = 1 colour clock (CCK).
//! Every 4 ticks = 1 CPU clock (gated by DMA).
//! Every 40 ticks = 1 CIA E-clock.
//!
//! One frame = 312 lines × 227 CCKs × 8 = 566,208 crystal ticks.

#![allow(clippy::cast_possible_truncation, clippy::cast_sign_loss, clippy::cast_possible_wrap)]

use emu_68000::M68000;
use emu_core::{Bus, Cpu, Observable, Tickable, Value};

use crate::agnus::{self, SlotOwner};
use crate::bus::AmigaBus;
use crate::config::AmigaConfig;
use crate::denise;
use crate::input::InputQueue;
use std::sync::OnceLock;

/// Crystal frequency: 28,375,160 Hz (PAL).
#[allow(dead_code)]
pub const CRYSTAL_HZ: u64 = 28_375_160;

/// Crystal ticks per CCK.
const CCK_DIVISOR: u64 = 8;

/// Crystal ticks per CPU clock.
const CPU_DIVISOR: u64 = 4;

/// Crystal ticks per CIA E-clock.
const CIA_DIVISOR: u64 = 40;

/// Crystal ticks per frame: 312 lines × 227 CCKs × 8.
pub const TICKS_PER_FRAME: u64 =
    agnus::LINES_PER_FRAME as u64 * agnus::CCKS_PER_LINE as u64 * CCK_DIVISOR;

/// Display window constants for framebuffer coordinate mapping.
/// Standard: DIWSTRT=$2C81 → display starts at VPOS $2C, HPOS $81 (CCK $40).
const DISPLAY_VSTART: u16 = 0x2C;
const DISPLAY_HSTART_CCK: u16 = 0x40;

/// Amiga 500 PAL system.
pub struct Amiga {
    cpu: M68000,
    bus: AmigaBus,
    input: InputQueue,
    /// Master clock: counts crystal ticks.
    master_clock: u64,
    /// Completed frame counter.
    frame_count: u64,
    /// Current CCK slot owner (set at CCK boundary, valid for 8 ticks).
    current_slot: SlotOwner,
    /// VPOS at current CCK (latched at CCK boundary for pixel output).
    cck_vpos: u16,
    /// HPOS at current CCK (latched before beam advance).
    cck_hpos: u16,
}

impl Amiga {
    /// Create a new Amiga 500 PAL from the given configuration.
    ///
    /// # Errors
    ///
    /// Returns an error if the Kickstart ROM is invalid.
    pub fn new(config: &AmigaConfig) -> Result<Self, String> {
        let mut bus = AmigaBus::new(&config.kickstart)?;
        let mut cpu = M68000::new();

        // Read reset vectors from ROM (mapped at $000000 via overlay)
        let ssp_hi = u32::from(bus.read(0x00_0000).data) << 24;
        let ssp_lo3 = u32::from(bus.read(0x00_0001).data) << 16;
        let ssp_lo2 = u32::from(bus.read(0x00_0002).data) << 8;
        let ssp_lo1 = u32::from(bus.read(0x00_0003).data);
        let ssp = ssp_hi | ssp_lo3 | ssp_lo2 | ssp_lo1;

        let pc_hi = u32::from(bus.read(0x00_0004).data) << 24;
        let pc_lo3 = u32::from(bus.read(0x00_0005).data) << 16;
        let pc_lo2 = u32::from(bus.read(0x00_0006).data) << 8;
        let pc_lo1 = u32::from(bus.read(0x00_0007).data);
        let pc = pc_hi | pc_lo3 | pc_lo2 | pc_lo1;

        // Set up the 68000 in supervisor mode
        cpu.regs.ssp = ssp;
        cpu.regs.pc = pc;
        // SR: supervisor mode, all interrupts masked initially
        cpu.regs.sr = 0x2700;

        Ok(Self {
            cpu,
            bus,
            input: InputQueue::new(),
            master_clock: 0,
            frame_count: 0,
            current_slot: SlotOwner::Cpu,
            cck_vpos: 0,
            cck_hpos: 0,
        })
    }

    /// Run one complete frame.
    pub fn run_frame(&mut self) -> u64 {
        self.frame_count += 1;
        let start_clock = self.master_clock;
        let target = start_clock + TICKS_PER_FRAME;
        let frame = self.frame_count;
        let bus = &mut self.bus;
        self.input.process(frame, |code, pressed| {
            bus.queue_keyboard_raw(code, pressed);
        });

        while self.master_clock < target {
            self.tick();
        }

        self.master_clock - start_clock
    }

    /// Reference to the framebuffer (ARGB32).
    #[must_use]
    pub fn framebuffer(&self) -> &[u32] {
        self.bus.denise.framebuffer()
    }

    /// Framebuffer width.
    #[must_use]
    pub fn framebuffer_width(&self) -> u32 {
        denise::FB_WIDTH
    }

    /// Framebuffer height.
    #[must_use]
    pub fn framebuffer_height(&self) -> u32 {
        denise::FB_HEIGHT
    }

    /// Reference to the CPU.
    #[must_use]
    pub fn cpu(&self) -> &M68000 {
        &self.cpu
    }

    /// Mutable reference to the CPU.
    pub fn cpu_mut(&mut self) -> &mut M68000 {
        &mut self.cpu
    }

    /// Reference to the bus.
    #[must_use]
    pub fn bus(&self) -> &AmigaBus {
        &self.bus
    }

    /// Mutable reference to the bus.
    pub fn bus_mut(&mut self) -> &mut AmigaBus {
        &mut self.bus
    }

    /// Reference to the input queue.
    #[must_use]
    pub fn input_queue(&self) -> &InputQueue {
        &self.input
    }

    /// Mutable reference to the input queue.
    pub fn input_queue_mut(&mut self) -> &mut InputQueue {
        &mut self.input
    }

    /// Queue a raw keyboard keycode (press/release).
    pub fn queue_keycode(&mut self, code: u8, pressed: bool) {
        self.bus.queue_keyboard_raw(code, pressed);
    }

    /// Tick only the CPU (for debugging — bypasses DMA/beam/copper).
    pub fn tick_cpu_only(&mut self) {
        self.cpu.tick(&mut self.bus);
    }

    /// Master clock tick count.
    #[must_use]
    pub fn master_clock(&self) -> u64 {
        self.master_clock
    }

    /// Completed frame count.
    #[must_use]
    pub fn frame_count(&self) -> u64 {
        self.frame_count
    }

    /// Perform bitplane DMA for the current CCK slot.
    fn do_bitplane_dma(&mut self) {
        let num_bpl = self.bus.agnus.num_bitplanes() as usize;
        if num_bpl == 0 {
            return;
        }

        // Fetch data for each active bitplane
        for i in 0..num_bpl {
            let addr = self.bus.agnus.bpl_pt[i];
            let data = self.bus.read_chip_word(addr);
            self.bus.denise.load_bitplane(i, data);
            // Advance pointer
            self.bus.agnus.bpl_pt[i] = addr.wrapping_add(2);
        }
    }

    /// Add bitplane modulo at end of display line.
    fn apply_bitplane_modulo(&mut self) {
        let num_bpl = self.bus.agnus.num_bitplanes() as usize;
        #[allow(clippy::cast_possible_wrap)]
        let mod1 = i32::from(self.bus.agnus.bpl1mod as i16);
        #[allow(clippy::cast_possible_wrap)]
        let mod2 = i32::from(self.bus.agnus.bpl2mod as i16);

        for i in 0..num_bpl {
            let modulo = if i % 2 == 0 { mod1 } else { mod2 };
            self.bus.agnus.bpl_pt[i] =
                (self.bus.agnus.bpl_pt[i] as i32).wrapping_add(modulo) as u32;
        }
    }

    /// Map beam position to framebuffer coordinates.
    #[allow(clippy::unused_self)]
    fn beam_to_fb(&self, vpos: u16, hpos_cck: u16) -> Option<(u32, u32)> {
        // Map VPOS to FB Y: display starts at VPOS $2C
        let fb_y = vpos.wrapping_sub(DISPLAY_VSTART);
        if fb_y >= denise::FB_HEIGHT as u16 {
            return None;
        }

        // Map HPOS CCK to FB X: display starts at CCK $40
        // Each CCK = 1 lo-res pixel in our framebuffer
        let fb_x = hpos_cck.wrapping_sub(DISPLAY_HSTART_CCK);
        if fb_x >= denise::FB_WIDTH as u16 {
            return None;
        }

        Some((u32::from(fb_x), u32::from(fb_y)))
    }
}

impl Tickable for Amiga {
    fn tick(&mut self) {
        self.master_clock += 1;

        // === CCK boundary (every 8 crystal ticks) ===
        if self.master_clock.is_multiple_of(CCK_DIVISOR) {
            // Latch beam position before advance
            self.cck_vpos = self.bus.agnus.vpos;
            self.cck_hpos = self.bus.agnus.hpos;

            // Check for VBlank at start of frame
            if self.bus.agnus.is_vblank_start() {
                // VERTB interrupt
                self.bus.paula.request_interrupt(5);
                // Restart copper from COP1LC
                self.bus.copper.restart_cop1();
            }

            // Apply bitplane modulo at end of data fetch
            let ddfstop = self.bus.agnus.ddfstop & 0x00FC;
            if self.cck_hpos == ddfstop + 9 && self.bus.agnus.num_bitplanes() > 0 {
                self.apply_bitplane_modulo();
            }

            // Advance beam and get DMA slot allocation
            self.current_slot = self.bus.agnus.tick_cck();

            match self.current_slot {
                SlotOwner::Bitplane => {
                    self.do_bitplane_dma();
                }
                SlotOwner::Copper => {
                    // Copper gets the bus — tick with DMA.
                    // Read chip RAM through memory directly to avoid borrow conflict.
                    let vpos = self.cck_vpos;
                    let hpos = self.cck_hpos;
                    let memory = &self.bus.memory;
                    let result = self.bus.copper.tick_with_bus(
                        |addr| {
                            let a = (addr & crate::memory::CHIP_RAM_WORD_MASK) as usize;
                            let hi = memory.chip_ram[a];
                            let lo = memory.chip_ram[a + 1];
                            u16::from(hi) << 8 | u16::from(lo)
                        },
                        vpos,
                        hpos,
                    );
                    if let Some((reg, value)) = result {
                        self.bus.write_custom_reg(reg, value);
                    }
                }
                _ => {
                    // Copper can still check WAIT without the bus
                    self.bus.copper.tick_no_bus(self.cck_vpos, self.cck_hpos);
                }
            }

            // Denise: output pixel
            if let Some((fb_x, fb_y)) = self.beam_to_fb(self.cck_vpos, self.cck_hpos) {
                // In the active display area with bitplane data
                if self.bus.agnus.num_bitplanes() > 0 {
                    self.bus.denise.output_pixel(fb_x, fb_y);
                } else {
                    self.bus.denise.output_background(fb_x, fb_y);
                }
            }
        }

        // === CPU clock (every 4 crystal ticks) ===
        if self.master_clock.is_multiple_of(CPU_DIVISOR) {
            // CPU only runs when it owns the current CCK slot unless forced.
            let force_cpu = *FORCE_CPU.get_or_init(|| {
                std::env::var("EMU_AMIGA_FORCE_CPU").is_ok()
            });
            let cpu_can_run = force_cpu
                || matches!(
                    self.current_slot,
                    SlotOwner::Cpu | SlotOwner::Copper // CPU can run on copper slots too (odd halves)
                );

            if cpu_can_run {
                // Update IPL from Paula
                let ipl = self.bus.paula.compute_ipl();
                self.cpu.set_ipl(ipl);
                self.cpu.tick(&mut self.bus);
                if std::env::var("EMU_AMIGA_FORCE_WARMSTART").is_ok() {
                    // If Kickstart is about to restart after memory test, set a warm-start flag.
                    if self.cpu.regs.pc == 0x00FC_05B0 {
                        self.bus.memory.write(0x00000000, 0x00);
                        self.bus.memory.write(0x00000001, 0x00);
                        self.bus.memory.write(0x00000002, 0x00);
                        self.bus.memory.write(0x00000003, 0x01);
                    }
                }
            }
        }

        // === CIA E-clock (every 40 crystal ticks) ===
        if self.master_clock.is_multiple_of(CIA_DIVISOR) {
            self.bus.cia_a.tick();
            self.bus.cia_b.tick();
            self.bus.pump_keyboard();

            // CIA-A IRQ → INTREQ bit 3 (PORTS, level 2)
            if self.bus.cia_a.irq_active() {
                self.bus.paula.request_interrupt(3);
            }
            // CIA-B IRQ → INTREQ bit 13 (EXTER, level 6)
            if self.bus.cia_b.irq_active() {
                self.bus.paula.request_interrupt(13);
            }
        }
    }
}

static FORCE_CPU: OnceLock<bool> = OnceLock::new();

impl Observable for Amiga {
    fn query(&self, path: &str) -> Option<Value> {
        if let Some(rest) = path.strip_prefix("cpu.") {
            self.cpu.query(rest)
        } else if let Some(rest) = path.strip_prefix("agnus.") {
            match rest {
                "vpos" => Some(self.bus.agnus.vpos.into()),
                "hpos" => Some(self.bus.agnus.hpos.into()),
                "dmacon" => Some(self.bus.agnus.dmacon.into()),
                _ => None,
            }
        } else if let Some(rest) = path.strip_prefix("paula.") {
            match rest {
                "intena" => Some(self.bus.paula.intena.into()),
                "intreq" => Some(self.bus.paula.intreq.into()),
                "ipl" => Some(Value::U8(self.bus.paula.compute_ipl())),
                _ => None,
            }
        } else if let Some(rest) = path.strip_prefix("copper.") {
            match rest {
                "pc" => Some(self.bus.copper.pc().into()),
                _ => None,
            }
        } else if let Some(rest) = path.strip_prefix("memory.") {
            let addr = parse_hex_or_dec(rest)?;
            Some(Value::U8(self.bus.peek_chip_ram(addr as u32)))
        } else {
            match path {
                "master_clock" => Some(self.master_clock.into()),
                "frame_count" => Some(self.frame_count.into()),
                _ => self.cpu.query(path),
            }
        }
    }

    fn query_paths(&self) -> &'static [&'static str] {
        &[
            "cpu.<68000_paths>",
            "agnus.vpos",
            "agnus.hpos",
            "agnus.dmacon",
            "paula.intena",
            "paula.intreq",
            "paula.ipl",
            "copper.pc",
            "memory.<address>",
            "master_clock",
            "frame_count",
        ]
    }
}

fn parse_hex_or_dec(s: &str) -> Option<u64> {
    if let Some(hex) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        u64::from_str_radix(hex, 16).ok()
    } else if let Some(hex) = s.strip_prefix('$') {
        u64::from_str_radix(hex, 16).ok()
    } else {
        s.parse().ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_amiga() -> Amiga {
        let mut rom = vec![0u8; 256 * 1024];
        // SSP at $000000 = $00080000
        rom[0] = 0x00;
        rom[1] = 0x08;
        rom[2] = 0x00;
        rom[3] = 0x00;
        // PC at $000004 = $F80008 (just past the vectors in ROM)
        rom[4] = 0x00;
        rom[5] = 0xF8;
        rom[6] = 0x00;
        rom[7] = 0x08;
        // Put a NOP ($4E71) at $F80008 (offset 8)
        rom[8] = 0x4E;
        rom[9] = 0x71;
        // Another NOP
        rom[10] = 0x4E;
        rom[11] = 0x71;
        // Fill rest with STOP #$2700 ($4E72 $2700) to halt cleanly
        let stop_opcode = [0x4E, 0x72, 0x27, 0x00];
        for offset in (12..rom.len()).step_by(4) {
            let remaining = rom.len() - offset;
            let copy_len = remaining.min(4);
            rom[offset..offset + copy_len].copy_from_slice(&stop_opcode[..copy_len]);
        }

        let config = AmigaConfig { kickstart: rom };
        Amiga::new(&config).expect("valid amiga")
    }

    #[test]
    fn master_clock_advances() {
        let mut amiga = make_amiga();
        assert_eq!(amiga.master_clock(), 0);
        amiga.tick();
        assert_eq!(amiga.master_clock(), 1);
    }

    #[test]
    fn run_frame_returns_tick_count() {
        let mut amiga = make_amiga();
        let ticks = amiga.run_frame();
        assert_eq!(ticks, TICKS_PER_FRAME);
    }

    #[test]
    fn framebuffer_correct_size() {
        let amiga = make_amiga();
        assert_eq!(amiga.framebuffer_width(), denise::FB_WIDTH);
        assert_eq!(amiga.framebuffer_height(), denise::FB_HEIGHT);
        assert_eq!(
            amiga.framebuffer().len(),
            (denise::FB_WIDTH * denise::FB_HEIGHT) as usize
        );
    }

    #[test]
    fn observable_master_clock() {
        let amiga = make_amiga();
        assert_eq!(amiga.query("master_clock"), Some(Value::U64(0)));
    }

    #[test]
    fn observable_agnus_vpos() {
        let amiga = make_amiga();
        assert_eq!(amiga.query("agnus.vpos"), Some(Value::U16(0)));
    }
}
