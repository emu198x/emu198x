//! Cycle-accurate Atari 800XL emulator.
//!
//! The 800XL shares the same ANTIC/GTIA/POKEY chips as the Atari 5200,
//! with identical timing. The key difference is the memory map: 64KB RAM
//! with ROMs overlaid, controlled by a PIA 6520 at $D300.
//!
//! # Timing
//!
//! The master clock ticks at the colour clock frequency:
//! - NTSC: 3,579,545 Hz
//! - PAL: 3,546,894 Hz
//!
//! The CPU (6502C "SALLY") runs at crystal/2 = 1 CPU cycle per 2
//! colour clocks. ANTIC ticks at the colour clock rate. POKEY ticks
//! once per CPU cycle (every 2nd colour clock).
//!
//! One scan line = 228 colour clocks = 114 CPU cycles.
//! One frame = 262 lines (NTSC) / 312 lines (PAL).

mod bus;
pub mod capture;
mod cartridge;
mod config;
pub mod input_map;

pub use atari_antic as antic;
pub use atari_gtia as gtia;
pub use atari_pokey as pokey;
pub use bus::Atari800xlBus;
pub use config::{Atari800xlConfig, Atari800xlRegion};

use atari_antic::{Antic, AnticRegion, COLOUR_CLOCKS_PER_LINE};
use atari_gtia::Gtia;
use atari_pokey::Pokey;
use emu_core::{Cpu, Observable, Tickable, Value};
use mos_6502::Mos6502;
use mos_pia_6520::Pia6520;

use crate::bus::Atari800xlBusInner;
use crate::cartridge::Cartridge;

/// Atari 800XL system.
pub struct Atari800xl {
    /// 6502C CPU.
    cpu: Mos6502,
    /// Bus (owns RAM, chips, ROMs, cartridge).
    bus: Atari800xlBusInner,
    /// Master clock: counts colour clocks.
    master_clock: u64,
    /// Completed frame counter.
    frame_count: u64,
    /// Video region.
    region: Atari800xlRegion,
    /// Colour clocks per frame.
    clocks_per_frame: u64,
    /// DMA cycles stolen by ANTIC for the current scan line.
    dma_budget: u8,
    /// CPU cycle counter within the current scan line (0-113).
    line_cycle: u16,
}

impl Atari800xl {
    /// Create a new Atari 800XL from the given configuration.
    ///
    /// # Errors
    ///
    /// Returns an error if the cartridge ROM data is an unsupported size.
    pub fn new(config: &Atari800xlConfig) -> Result<Self, String> {
        let cart = if let Some(ref rom_data) = config.rom_data {
            Some(Cartridge::from_rom(rom_data)?)
        } else {
            None
        };

        let antic_region = match config.region {
            Atari800xlRegion::Ntsc => AnticRegion::Ntsc,
            Atari800xlRegion::Pal => AnticRegion::Pal,
        };

        let antic = Antic::new(antic_region);
        let gtia = Gtia::new();
        let pokey = Pokey::new(config.region.cpu_hz());
        let pia = Pia6520::new();

        let mut bus = Atari800xlBusInner {
            ram: [0; 65536],
            antic,
            gtia,
            pokey,
            pia,
            cart,
            os_rom: config.os_rom.clone(),
            basic_rom: config.basic_rom.clone(),
        };

        // Configure PIA PORTB for initial banking state.
        // Set DDR_B = $FF (all output) -- CRB bit 2 starts at 0, so
        // address 2 writes to DDR.
        bus.pia.write(0x02, 0xFF);
        // Set CRB bit 2 = 1 to select data register for future writes.
        bus.pia.write(0x03, 0x04);
        // Set PORTB: OS ROM on (bit 0 = 1), BASIC per config (bit 1 = 0
        // means enabled), self-test off (bit 7 = 1).
        let mut portb: u8 = 0xFF; // All bits high by default
        if config.basic_enabled {
            portb &= !0x02; // Clear bit 1 to enable BASIC
        }
        bus.pia.write(0x02, portb);

        // Without OS ROM, point reset vector at cartridge entry.
        if config.os_rom.is_none()
            && let Some(ref cart) = bus.cart
        {
            let base = cart.base();
            bus.ram[0xFFFC] = (base & 0xFF) as u8;
            bus.ram[0xFFFD] = (base >> 8) as u8;
            // Also set NMI and IRQ vectors to a RTI in RAM.
            // Place RTI ($40) at a known location.
            bus.ram[0x0000] = 0x40; // RTI
            bus.ram[0xFFFA] = 0x00; // NMI low
            bus.ram[0xFFFB] = 0x00; // NMI high
            bus.ram[0xFFFE] = 0x00; // IRQ low
            bus.ram[0xFFFF] = 0x00; // IRQ high
        }

        let mut cpu = Mos6502::new();

        // Read reset vector.
        let reset_lo = emu_core::Bus::read(&mut Atari800xlBus(&mut bus), 0xFFFC).data;
        let reset_hi = emu_core::Bus::read(&mut Atari800xlBus(&mut bus), 0xFFFD).data;
        cpu.regs.pc = u16::from(reset_lo) | (u16::from(reset_hi) << 8);

        let lines = u64::from(config.region.lines_per_frame());
        let clocks_per_frame = lines * u64::from(COLOUR_CLOCKS_PER_LINE);

        Ok(Self {
            cpu,
            bus,
            master_clock: 0,
            frame_count: 0,
            region: config.region,
            clocks_per_frame,
            dma_budget: 0,
            line_cycle: 0,
        })
    }

    /// Run one complete frame.
    ///
    /// Returns the number of colour clocks executed.
    pub fn run_frame(&mut self) -> u64 {
        self.frame_count += 1;
        let start = self.master_clock;
        let target = start + self.clocks_per_frame;

        while self.master_clock < target {
            self.tick();
        }

        self.master_clock - start
    }

    /// Reference to the framebuffer (ARGB32, 320 x 240).
    #[must_use]
    pub fn framebuffer(&self) -> &[u32] {
        self.bus.gtia.framebuffer()
    }

    /// Framebuffer width.
    #[must_use]
    pub fn framebuffer_width(&self) -> u32 {
        self.bus.gtia.framebuffer_width()
    }

    /// Framebuffer height.
    #[must_use]
    pub fn framebuffer_height(&self) -> u32 {
        self.bus.gtia.framebuffer_height()
    }

    /// Video region.
    #[must_use]
    pub fn region(&self) -> Atari800xlRegion {
        self.region
    }

    /// Reference to the CPU.
    #[must_use]
    pub fn cpu(&self) -> &Mos6502 {
        &self.cpu
    }

    /// Mutable reference to the CPU.
    pub fn cpu_mut(&mut self) -> &mut Mos6502 {
        &mut self.cpu
    }

    /// Master clock tick count (colour clocks).
    #[must_use]
    pub fn master_clock(&self) -> u64 {
        self.master_clock
    }

    /// Completed frame count.
    #[must_use]
    pub fn frame_count(&self) -> u64 {
        self.frame_count
    }

    /// Reference to the ANTIC.
    #[must_use]
    pub fn antic(&self) -> &Antic {
        &self.bus.antic
    }

    /// Reference to the GTIA.
    #[must_use]
    pub fn gtia(&self) -> &Gtia {
        &self.bus.gtia
    }

    /// Reference to the POKEY.
    #[must_use]
    pub fn pokey(&self) -> &Pokey {
        &self.bus.pokey
    }

    /// Reference to the PIA.
    #[must_use]
    pub fn pia(&self) -> &Pia6520 {
        &self.bus.pia
    }

    /// Set joystick direction via PIA PORTA (active-low bits 0-3).
    ///
    /// Each direction clears the corresponding bit when active:
    /// - Bit 0: up
    /// - Bit 1: down
    /// - Bit 2: left
    /// - Bit 3: right
    #[allow(clippy::fn_params_excessive_bools)]
    pub fn set_joystick(&mut self, up: bool, down: bool, left: bool, right: bool) {
        let mut value: u8 = 0xFF;
        if up {
            value &= !0x01;
        }
        if down {
            value &= !0x02;
        }
        if left {
            value &= !0x04;
        }
        if right {
            value &= !0x08;
        }
        self.bus.pia.set_port_a_input(value);
    }

    /// Set fire button state (GTIA TRIG0).
    pub fn set_fire(&mut self, pressed: bool) {
        self.bus.gtia.set_trigger(0, pressed);
    }

    /// Set console key state via GTIA CONSOL register.
    ///
    /// - `start`: START key (bit 0, active low)
    /// - `select`: SELECT key (bit 1, active low)
    /// - `option`: OPTION key (bit 2, active low)
    pub fn set_console_keys(&mut self, start: bool, select: bool, option: bool) {
        let mut consol: u8 = 0x07; // All released (bits 2-0 high)
        if start {
            consol &= !0x01;
        }
        if select {
            consol &= !0x02;
        }
        if option {
            consol &= !0x04;
        }
        self.bus.gtia.write(0x1F, consol);
    }

    /// Process the start of a new scan line: run ANTIC, render via GTIA,
    /// handle NMI, and set up DMA budget for the line.
    fn process_scan_line(&mut self) {
        // ANTIC reads from RAM directly (sees RAM underneath ROMs).
        let result = self.bus.antic.process_line(&self.bus.ram);

        // Feed player/missile data to GTIA if PM DMA occurred.
        if result.pm_dma {
            for i in 0..4 {
                self.bus.gtia.write(0x0D + i as u8, result.player_data[i]);
            }
            self.bus.gtia.write(0x11, result.missile_data);
        }

        // Render the line via GTIA. ANTIC scan_line is post-increment,
        // so the line we just processed is scan_line - 1.
        let line = self.bus.antic.scan_line().saturating_sub(1);
        let visible_line = line.wrapping_sub(8);
        self.bus.gtia.render_line(
            visible_line,
            &result.playfield,
            result.playfield_width,
            result.mode,
        );

        // Set DMA budget for this line.
        self.dma_budget = result.dma_cycles;
        self.line_cycle = 0;

        // Clear WSYNC at line boundary.
        self.bus.antic.clear_wsync();

        // Handle NMIs from ANTIC.
        if self.bus.antic.take_vbi() {
            self.cpu.nmi();
        }
        if self.bus.antic.take_dli() {
            self.cpu.nmi();
        }
    }
}

impl Tickable for Atari800xl {
    fn tick(&mut self) {
        self.master_clock += 1;

        // At the start of each scan line (every 228 colour clocks).
        if self.master_clock.is_multiple_of(u64::from(COLOUR_CLOCKS_PER_LINE)) {
            self.process_scan_line();
        }

        // CPU + POKEY tick every 2nd colour clock.
        if self.master_clock.is_multiple_of(2) {
            self.line_cycle += 1;

            // CPU: skip if ANTIC is stealing cycles (DMA) or WSYNC is active.
            if self.line_cycle > u16::from(self.dma_budget)
                && !self.bus.antic.wsync_halt()
            {
                self.cpu.tick(&mut Atari800xlBus(&mut self.bus));
            }

            // POKEY always ticks.
            self.bus.pokey.tick();

            // POKEY IRQ or PIA IRQ -> CPU IRQ.
            if self.bus.pokey.irq_pending() || self.bus.pia.irq_pending() {
                self.cpu.interrupt();
            }
        }
    }
}

impl Observable for Atari800xl {
    fn query(&self, path: &str) -> Option<Value> {
        if let Some(rest) = path.strip_prefix("cpu.") {
            self.cpu.query(rest)
        } else if let Some(rest) = path.strip_prefix("antic.") {
            match rest {
                "scan_line" => Some(self.bus.antic.scan_line().into()),
                "vcount" => Some(self.bus.antic.vcount().into()),
                _ => None,
            }
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
            "cpu.<6502_paths>",
            "antic.scan_line",
            "antic.vcount",
            "master_clock",
            "frame_count",
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a minimal 8KB cartridge that sets COLBK to blue and loops.
    fn minimal_cart() -> Vec<u8> {
        let mut rom = vec![0; 8192]; // 8KB at $A000-$BFFF

        // Code at $A000 (offset 0)
        let code: &[u8] = &[
            // Copy display list to RAM at $0600
            0xA2, 0x00,       // LDX #0
            0xBD, 0x40, 0xA0, // LDA $A040,X
            0x9D, 0x00, 0x06, // STA $0600,X
            0xE8,             // INX
            0xE0, 0x0C,       // CPX #12
            0xD0, 0xF5,       // BNE loop ($A002)
            // Set COLBK = $94 (blue)
            0xA9, 0x94,       // LDA #$94
            0x8D, 0x1A, 0xD0, // STA $D01A (GTIA COLBK)
            // Set COLPF0 = white
            0xA9, 0x0E,       // LDA #$0E
            0x8D, 0x16, 0xD0, // STA $D016
            // Set DMACTL = $22 (normal PF, DL DMA on)
            0xA9, 0x22,       // LDA #$22
            0x8D, 0x00, 0xD4, // STA $D400
            // Set DL pointer = $0600
            0xA9, 0x00,       // LDA #$00
            0x8D, 0x02, 0xD4, // STA $D402 (DLISTL)
            0xA9, 0x06,       // LDA #$06
            0x8D, 0x03, 0xD4, // STA $D403 (DLISTH)
            // Set NMIEN = $40 (enable VBI)
            0xA9, 0x40,       // LDA #$40
            0x8D, 0x0E, 0xD4, // STA $D40E
            // Infinite loop (JMP to self, patched below)
            0x4C, 0x00, 0x00,
        ];
        rom[..code.len()].copy_from_slice(code);

        // Patch JMP target to point at itself.
        let loop_addr = 0xA000u16 + code.len() as u16 - 3;
        rom[code.len() - 2] = (loop_addr & 0xFF) as u8;
        rom[code.len() - 1] = (loop_addr >> 8) as u8;

        // Display list at offset $40 ($A040), copied to $0600
        let dl: &[u8] = &[
            0x70, 0x70, 0x70, // 3x 8 blank lines
            0x4D,             // Mode D + LMS
            0x00, 0x40,       // Screen at $4000 (RAM, zeros = COLBK)
            0x0D, 0x0D, 0x0D, // 3 more mode D lines
            0x41,             // JVB
            0x00, 0x06,       // Back to $0600
        ];
        rom[0x40..0x40 + dl.len()].copy_from_slice(dl);

        // Reset vector at $BFFC-$BFFD (offset $1FFC)
        rom[0x1FFC] = 0x00;
        rom[0x1FFD] = 0xA0;

        // NMI at $BFFA -> RTI at $A100
        rom[0x0100] = 0x40; // RTI
        rom[0x1FFA] = 0x00;
        rom[0x1FFB] = 0xA1;

        // IRQ at $BFFE -> RTI
        rom[0x1FFE] = 0x00;
        rom[0x1FFF] = 0xA1;

        rom
    }

    fn make_system_ntsc(cart: Vec<u8>) -> Atari800xl {
        let config = Atari800xlConfig {
            rom_data: Some(cart),
            os_rom: None,
            basic_rom: None,
            region: Atari800xlRegion::Ntsc,
            basic_enabled: false,
        };
        Atari800xl::new(&config).expect("system creation should succeed")
    }

    fn make_system_pal(cart: Vec<u8>) -> Atari800xl {
        let config = Atari800xlConfig {
            rom_data: Some(cart),
            os_rom: None,
            basic_rom: None,
            region: Atari800xlRegion::Pal,
            basic_enabled: false,
        };
        Atari800xl::new(&config).expect("system creation should succeed")
    }

    #[test]
    fn cpu_starts_at_cart_entry() {
        let system = make_system_ntsc(minimal_cart());
        assert_eq!(system.cpu().regs.pc, 0xA000, "CPU should start at $A000");
    }

    #[test]
    fn blue_background_after_frames() {
        let mut system = make_system_ntsc(minimal_cart());

        // Run several frames so the cart code has time to execute, set up
        // the display list, and ANTIC renders at least one full frame.
        for _ in 0..5 {
            system.run_frame();
        }

        // The cart writes $94 to COLBK; the framebuffer should have
        // non-black pixels.
        let fb = system.framebuffer();
        let non_black = fb.iter().any(|&p| p != 0 && p != 0xFF00_0000);
        assert!(non_black, "framebuffer should have non-black pixels after cart sets COLBK");
    }

    #[test]
    fn ntsc_clock_count() {
        let mut system = make_system_ntsc(minimal_cart());
        let clocks = system.run_frame();
        // NTSC: 228 colour clocks x 262 lines = 59,736
        assert_eq!(clocks, 228 * 262);
    }

    #[test]
    fn pal_clock_count() {
        let mut system = make_system_pal(minimal_cart());
        let clocks = system.run_frame();
        // PAL: 228 colour clocks x 312 lines = 71,136
        assert_eq!(clocks, 228 * 312);
    }

    #[test]
    fn master_clock_advances() {
        let mut system = make_system_ntsc(minimal_cart());
        assert_eq!(system.master_clock(), 0);
        system.run_frame();
        assert_eq!(system.master_clock(), 228 * 262);
        system.run_frame();
        assert_eq!(system.master_clock(), 228 * 262 * 2);
    }

    #[test]
    fn pia_portb_banking() {
        let config = Atari800xlConfig {
            rom_data: None,
            os_rom: None,
            basic_rom: Some(vec![0xBB; 8192]),
            region: Atari800xlRegion::Ntsc,
            basic_enabled: true,
        };
        let mut system = Atari800xl::new(&config).expect("system creation");

        // With BASIC enabled, $A000 should read BASIC ROM
        let val = emu_core::Bus::read(&mut Atari800xlBus(&mut system.bus), 0xA000).data;
        assert_eq!(val, 0xBB, "BASIC ROM should be visible at $A000");

        // Disable BASIC: set PORTB bit 1 = 1.
        // PIA already has DDR_B=$FF, CRB bit 2=1 from init.
        system.bus.pia.write(0x02, 0xFF); // PORTB = $FF (bit 1 = 1)
        let val = emu_core::Bus::read(&mut Atari800xlBus(&mut system.bus), 0xA000).data;
        assert_eq!(val, 0x00, "RAM (zeros) should be visible at $A000 with BASIC disabled");
    }

    #[test]
    fn frame_count_increments() {
        let mut system = make_system_ntsc(minimal_cart());
        assert_eq!(system.frame_count(), 0);
        system.run_frame();
        assert_eq!(system.frame_count(), 1);
        system.run_frame();
        assert_eq!(system.frame_count(), 2);
    }

    #[test]
    fn region_accessor() {
        let system = make_system_ntsc(minimal_cart());
        assert_eq!(system.region(), Atari800xlRegion::Ntsc);

        let system = make_system_pal(minimal_cart());
        assert_eq!(system.region(), Atari800xlRegion::Pal);
    }
}
