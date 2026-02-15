//! Top-level Amiga system.
//!
//! Crystal ticks at 28,375,160 Hz (PAL) / 28,636,360 Hz (NTSC).
//! Every 8 ticks = 1 colour clock (CCK).
//! Every 4 ticks = 1 CPU clock.
//! Every 40 ticks = 1 CIA E-clock.
//!
//! The CPU **always ticks** — it is never gated. Chip RAM contention is
//! returned as wait_cycles via BusResult, consumed by the CPU as idle ticks.

#![allow(clippy::cast_possible_truncation, clippy::cast_sign_loss, clippy::cast_possible_wrap)]

use cpu_m68k::{Cpu68000, FunctionCode, M68kBus};

use crate::agnus::SlotOwner;
use crate::agnus::beam;
use crate::bus::AmigaBus;
use crate::config::{AmigaConfig, AmigaModel, Region};
use crate::custom_regs;
use crate::denise;
use crate::input::InputQueue;
use crate::memory::Memory;

/// Crystal ticks per CCK.
const CCK_DIVISOR: u64 = 8;

/// Crystal ticks per CPU clock.
const CPU_DIVISOR: u64 = 4;

/// Crystal ticks per CIA E-clock.
const CIA_DIVISOR: u64 = 40;

/// Display window constants for framebuffer coordinate mapping.
const DISPLAY_VSTART: u16 = 0x2C;
/// Horizontal start in CCKs. Each CCK = 2 lores pixels.
const DISPLAY_HSTART_CCK: u16 = 0x2E;

/// Amiga system.
pub struct Amiga {
    cpu: Cpu68000,
    bus: AmigaBus,
    input: InputQueue,
    /// Master clock: counts crystal ticks.
    master_clock: u64,
    /// Completed frame counter.
    frame_count: u64,
    /// Crystal ticks per frame.
    ticks_per_frame: u64,
    /// Current CCK slot owner (set at CCK boundary, valid for 8 ticks).
    current_slot: SlotOwner,
    /// VPOS at current CCK (latched at CCK boundary for pixel output).
    cck_vpos: u16,
    /// HPOS at current CCK (latched before beam advance).
    cck_hpos: u16,
    /// Model name for display.
    model: AmigaModel,
    /// Region for timing.
    region: Region,
}

impl Amiga {
    /// Create a new Amiga from the given configuration.
    ///
    /// # Errors
    ///
    /// Returns an error if the configuration is invalid.
    pub fn new(config: &AmigaConfig) -> Result<Self, String> {
        let memory = Memory::new(config)?;
        let agnus = crate::agnus::Agnus::new(config.agnus, config.region);
        let denise = crate::denise::Denise::new(config.denise);
        let mut bus = AmigaBus::new(memory, agnus, denise);

        // Read reset vectors from ROM/WCS (mapped at $000000 via overlay).
        //
        // A500+ ROMs have standard 68K reset vectors at offset 0 (SSP, PC).
        // A1000 WCS images start with $1111 magic + JMP <entry>, so the first
        // 8 bytes aren't valid vectors. Detect this and handle it.
        let fc = FunctionCode::SupervisorData;
        let magic = bus.read_word(0x000000, fc).data;

        let (ssp, pc) = if magic == 0x1111 {
            // A1000 WCS image: $1111 magic at offset 0, JMP abs.L at offset 2.
            // Parse the JMP target as the entry PC. SSP = top of chip RAM.
            let opcode = bus.read_word(0x000002, fc).data;
            if opcode != 0x4EF9 {
                return Err(format!(
                    "KS WCS image has $1111 magic but no JMP at offset 2 (got ${opcode:04X})"
                ));
            }
            let pc_hi = bus.read_word(0x000004, fc).data;
            let pc_lo = bus.read_word(0x000006, fc).data;
            let entry_pc = u32::from(pc_hi) << 16 | u32::from(pc_lo);
            let ssp = config.chip_ram_size as u32;
            (ssp, entry_pc)
        } else {
            // Standard ROM: 68K reset vectors at offset 0.
            let ssp_lo = bus.read_word(0x000002, fc).data;
            let ssp = u32::from(magic) << 16 | u32::from(ssp_lo);

            let pc_hi = bus.read_word(0x000004, fc).data;
            let pc_lo = bus.read_word(0x000006, fc).data;
            let pc = u32::from(pc_hi) << 16 | u32::from(pc_lo);
            (ssp, pc)
        };

        let mut cpu = Cpu68000::new();
        let regs = cpu.registers_mut();
        regs.ssp = ssp;
        regs.pc = pc;
        regs.sr = 0x2700; // Supervisor mode, all interrupts masked

        let ticks_per_frame = beam::ticks_per_frame(config.region);

        Ok(Self {
            cpu,
            bus,
            input: InputQueue::new(),
            master_clock: 0,
            frame_count: 0,
            ticks_per_frame,
            current_slot: SlotOwner::Cpu,
            cck_vpos: 0,
            cck_hpos: 0,
            model: config.model,
            region: config.region,
        })
    }

    /// Run one complete frame.
    pub fn run_frame(&mut self) -> u64 {
        self.frame_count += 1;
        let start_clock = self.master_clock;
        let target = start_clock + self.ticks_per_frame;
        let frame = self.frame_count;

        // Process input events for this frame
        let bus = &mut self.bus;
        self.input.process(frame, |code, pressed| {
            bus.queue_keyboard_raw(code, pressed);
        });

        while self.master_clock < target {
            self.tick();
        }

        self.master_clock - start_clock
    }

    /// One crystal tick.
    pub fn tick(&mut self) {
        self.master_clock += 1;

        // === CCK boundary (every 8 crystal ticks) ===
        if self.master_clock % CCK_DIVISOR == 0 {
            // Latch beam position before advance
            self.cck_vpos = self.bus.agnus.vpos;
            self.cck_hpos = self.bus.agnus.hpos;

            // VBlank at start of frame
            if self.bus.agnus.is_vblank_start() {
                self.bus.paula.request_interrupt(5); // VERTB
                // Copper only restarts when COPEN DMA is active.
                if self.bus.agnus.channel_enabled(custom_regs::DMAF_COPEN) {
                    self.bus.copper.restart_cop1();
                }
            }

            // Apply bitplane modulo at end of data fetch
            let ddfstop = self.bus.agnus.ddfstop & 0x00FC;
            if self.cck_hpos == ddfstop + 9 && self.bus.agnus.num_bitplanes() > 0 {
                self.apply_bitplane_modulo();
            }

            // Advance beam and get DMA slot allocation
            self.current_slot = self.bus.agnus.tick_cck();

            // DMA at system level (avoids borrow checker conflicts)
            match self.current_slot {
                SlotOwner::Bitplane(plane) => {
                    self.do_bitplane_dma(plane);
                }
                SlotOwner::Copper => {
                    let vpos = self.cck_vpos;
                    let hpos = self.cck_hpos;
                    self.bus.diag_copper_pc = self.bus.copper.pc();
                    let memory = &self.bus.memory;
                    let result = self.bus.copper.tick_with_bus(
                        |addr| memory.read_chip_word(addr),
                        vpos,
                        hpos,
                    );
                    if let Some((reg, value)) = result {
                        self.bus.write_custom_reg_from(reg, value, "cop");
                    }
                }
                _ => {
                    self.bus
                        .copper
                        .tick_no_bus(self.cck_vpos, self.cck_hpos);
                }
            }

            // Denise: output 2 lores pixels per CCK
            if let Some((fb_x, fb_y)) = self.beam_to_fb(self.cck_vpos, self.cck_hpos) {
                if self.bus.agnus.num_bitplanes() > 0 {
                    self.bus.denise.output_pixel(fb_x, fb_y);
                    self.bus.denise.output_pixel(fb_x + 1, fb_y);
                } else {
                    self.bus.denise.output_background(fb_x, fb_y);
                    self.bus.denise.output_background(fb_x + 1, fb_y);
                }
            }
        }

        // === CPU clock (every 4 crystal ticks) ===
        // The CPU ALWAYS ticks. Contention is handled via wait_cycles.
        if self.master_clock % CPU_DIVISOR == 0 {
            self.bus.diag_cpu_pc = self.cpu.registers().pc;
            let ipl = self.bus.paula.compute_ipl();
            self.cpu.set_ipl(ipl);
            self.cpu.tick(&mut self.bus);
        }

        // === CIA E-clock (every 40 crystal ticks) ===
        if self.master_clock % CIA_DIVISOR == 0 {
            self.bus.cia_a.tick();
            self.bus.cia_b.tick();
            self.bus.pump_keyboard();

            // CIA-A IRQ -> INTREQ bit 3 (PORTS, level 2)
            if self.bus.cia_a.irq_active() {
                self.bus.paula.request_interrupt(3);
            }
            // CIA-B IRQ -> INTREQ bit 13 (EXTER, level 6)
            if self.bus.cia_b.irq_active() {
                self.bus.paula.request_interrupt(13);
            }
        }
    }

    /// Perform bitplane DMA for the current CCK slot.
    fn do_bitplane_dma(&mut self, plane: u8) {
        let num_bpl = self.bus.agnus.num_bitplanes();
        if num_bpl == 0 || plane >= num_bpl {
            return;
        }

        let idx = plane as usize;
        let addr = self.bus.agnus.bpl_pt[idx];
        let data = self.bus.memory.read_chip_word(addr);
        self.bus.denise.load_bitplane(idx, data);
        self.bus.agnus.bpl_pt[idx] = addr.wrapping_add(2);
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

    /// Map beam position to framebuffer coordinates (lores pixels).
    /// Each CCK = 2 lores pixels, so returns the left pixel of the pair.
    fn beam_to_fb(&self, vpos: u16, hpos_cck: u16) -> Option<(u32, u32)> {
        let fb_y = vpos.wrapping_sub(DISPLAY_VSTART);
        if fb_y >= denise::FB_HEIGHT as u16 {
            return None;
        }

        let cck_offset = hpos_cck.wrapping_sub(DISPLAY_HSTART_CCK);
        let fb_x = u32::from(cck_offset) * 2;
        if fb_x + 1 >= denise::FB_WIDTH {
            return None;
        }

        Some((fb_x, u32::from(fb_y)))
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
    pub fn cpu(&self) -> &Cpu68000 {
        &self.cpu
    }

    /// Mutable reference to the CPU.
    pub fn cpu_mut(&mut self) -> &mut Cpu68000 {
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

    /// Queue a raw keyboard keycode.
    pub fn queue_keycode(&mut self, code: u8, pressed: bool) {
        self.bus.queue_keyboard_raw(code, pressed);
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

    /// Crystal ticks per frame.
    #[must_use]
    pub fn ticks_per_frame(&self) -> u64 {
        self.ticks_per_frame
    }

    /// Model name.
    #[must_use]
    pub fn model(&self) -> AmigaModel {
        self.model
    }

    /// Region.
    #[must_use]
    pub fn region(&self) -> Region {
        self.region
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{AmigaConfig, AmigaModel};
    use crate::memory::KICKSTART_SIZE;

    fn make_amiga() -> Amiga {
        let mut rom = vec![0u8; KICKSTART_SIZE];
        // SSP = $00080000
        rom[0] = 0x00; rom[1] = 0x08; rom[2] = 0x00; rom[3] = 0x00;
        // PC = $F80008
        rom[4] = 0x00; rom[5] = 0xF8; rom[6] = 0x00; rom[7] = 0x08;
        // NOP at $F80008
        rom[8] = 0x4E; rom[9] = 0x71;
        rom[10] = 0x4E; rom[11] = 0x71;
        // Fill with STOP #$2700
        let stop_opcode = [0x4E, 0x72, 0x27, 0x00];
        for offset in (12..rom.len()).step_by(4) {
            let remaining = rom.len() - offset;
            let copy_len = remaining.min(4);
            rom[offset..offset + copy_len].copy_from_slice(&stop_opcode[..copy_len]);
        }

        let config = AmigaConfig::preset(AmigaModel::A1000, rom);
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
        assert_eq!(ticks, beam::ticks_per_frame(Region::Pal));
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
}
