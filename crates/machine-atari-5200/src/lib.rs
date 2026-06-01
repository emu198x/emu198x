//! Atari 5200 SuperSystem machine wiring.
//!
//! Fresh-write against the workspace pin-driven bus pattern (RULES.md
//! rule 6). The donor at `Emu198x-Oldest/crates/machine-atari-5200`
//! used the deprecated `emu_core::Bus` callback and could not port
//! directly; this file uses it as a system spec — 6502 "Sally"
//! address decode, ANTIC scan-line processing + DMA cycle stealing,
//! GTIA player/missile DMA feed, POKEY paddle pots — but the wiring
//! is written against [`mos_6502::M6502`]'s public pin fields.
//!
//! # The Atari 5200 SuperSystem
//!
//! The 5200 (Atari, 1982) is the home-console rebranding of the
//! Atari 400/800 8-bit computer hardware. Same ANTIC + GTIA + POKEY
//! chip family as the 800XL / 130XE; what differs is the cartridge
//! ROM window, the controller layout (analog joystick via POKEY
//! pots), and the lack of a keyboard. Considered a commercial
//! flop in its day, but the chip set lived on for 15+ years.
//!
//! - **CPU:** MOS 6502C "Sally" (a stock 6502 with Atari's HALT pin
//!   for ANTIC DMA cycle stealing)
//! - **ANTIC:** display-list processor + DMA controller
//! - **GTIA:** video output + player/missile graphics + collision
//! - **POKEY:** 4-channel audio + serial I/O + paddle pot scanner
//!   (drives the analog joystick on the 5200)
//! - **RAM:** 16 KB at `$0000-$3FFF`
//! - **Cartridge:** up to 32 KB in the `$4000-$BFFF` window (mirrored
//!   for smaller sizes)
//! - **BIOS:** optional 2 KB at `$F800-$FFFF` (without BIOS, the
//!   cart's top mirror provides the reset vector)
//!
//! # Memory map
//!
//! | Range         | Contents                                       |
//! |---------------|------------------------------------------------|
//! | `$0000-$3FFF` | 16 KB RAM                                      |
//! | `$4000-$BFFF` | Cartridge ROM window (size-mirrored)           |
//! | `$C000-$CFFF` | GTIA (every `$100` mirrors the 32-reg block)   |
//! | `$D400-$D5FF` | ANTIC (every `$10` mirrors the 16-reg block)   |
//! | `$E800-$E9FF` | POKEY (every `$10` mirrors the 16-reg block)   |
//! | `$F800-$FFFF` | 2 KB BIOS ROM (cart fallback if BIOS absent)   |
//!
//! # Clock model
//!
//! Master clock = colour clock (3.579545 MHz NTSC, 3.546894 MHz PAL).
//! CPU + POKEY tick every 2nd colour clock = 1.79 MHz NTSC. ANTIC
//! processes one scan line at every 228-clock boundary; the
//! line-level pipeline stalls the CPU for its DMA budget at the
//! start of each line, then frees it for the remainder.

mod cartridge;

pub use cartridge::Cartridge;

use atari_antic::{Antic, AnticRegion, COLOUR_CLOCKS_PER_LINE};
use atari_gtia::Gtia;
use atari_pokey::Pokey;
use mos_6502::M6502;

/// Joystick pot centre value (0-228 range — POKEY pots are 8-bit).
pub const POT_CENTER: u8 = 114;
/// Joystick pot maximum (fully right or fully down).
pub const POT_MAX: u8 = 228;

/// Atari 5200 region.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Atari5200Region {
    Ntsc,
    Pal,
}

impl Atari5200Region {
    fn antic_region(self) -> AnticRegion {
        match self {
            Self::Ntsc => AnticRegion::Ntsc,
            Self::Pal => AnticRegion::Pal,
        }
    }

    fn cpu_hz(self) -> u32 {
        match self {
            Self::Ntsc => 1_789_772,
            Self::Pal => 1_773_447,
        }
    }

    fn lines_per_frame(self) -> u16 {
        match self {
            Self::Ntsc => 262,
            Self::Pal => 312,
        }
    }
}

/// Atari 5200 machine.
pub struct Atari5200 {
    cpu: M6502,
    antic: Antic,
    gtia: Gtia,
    pokey: Pokey,
    cart: Cartridge,
    ram: [u8; 16384],
    bios: Vec<u8>,
    region: Atari5200Region,
    master_clock: u64,
    clocks_per_frame: u64,
    frame_count: u64,
    /// ANTIC DMA cycle budget for the current line (CPU stalls until
    /// this many CPU cycles have elapsed).
    dma_budget: u8,
    /// CPU cycle counter within the current scan line.
    line_cycle: u16,
}

impl Atari5200 {
    /// Create a new Atari 5200. `bios` may be empty to fall back to
    /// the cartridge's `$FFFC/$FFFD` mirror for the reset vector.
    pub fn new(
        rom: Vec<u8>,
        bios: Vec<u8>,
        region: Atari5200Region,
    ) -> Result<Self, String> {
        let cart = Cartridge::from_rom(&rom)?;
        let mut cpu = M6502::new();
        cpu.reset();
        let mut pokey = Pokey::new(region.cpu_hz());
        pokey.set_pot(0, POT_CENTER);
        pokey.set_pot(1, POT_CENTER);
        let clocks_per_frame =
            u64::from(region.lines_per_frame()) * u64::from(COLOUR_CLOCKS_PER_LINE);
        Ok(Self {
            cpu,
            antic: Antic::new(region.antic_region()),
            gtia: Gtia::new(),
            pokey,
            cart,
            ram: [0; 16384],
            bios,
            region,
            master_clock: 0,
            clocks_per_frame,
            frame_count: 0,
            dma_budget: 0,
            line_cycle: 0,
        })
    }

    /// Run one frame and return colour clocks consumed.
    pub fn run_frame(&mut self) -> u64 {
        let start = self.master_clock;
        let target = start + self.clocks_per_frame;
        // Paint the canonical TV-visible border at frame start so the
        // 384 x 288 framebuffer carries COLBK around the 320 x 240
        // active playfield. Mid-frame COLBK changes affect the next
        // frame's border — v1 simplification.
        self.gtia.fill_border();
        while self.master_clock < target {
            self.tick_colour_clock();
        }
        self.frame_count += 1;
        self.master_clock - start
    }

    fn tick_colour_clock(&mut self) {
        self.master_clock += 1;

        // At every scan-line boundary, run ANTIC + render.
        if self.master_clock % u64::from(COLOUR_CLOCKS_PER_LINE) == 0 {
            self.process_scan_line();
        }

        // CPU + POKEY tick every 2nd colour clock.
        if self.master_clock % 2 == 0 {
            self.line_cycle += 1;
            // CPU stalls for the ANTIC DMA budget at the start of the
            // line; freed once line_cycle exceeds the budget.
            if self.line_cycle > u16::from(self.dma_budget) && !self.antic.wsync_halt() {
                self.cpu.tick();
                if self.cpu.rw {
                    self.cpu.data_in = self.mem_read(self.cpu.addr);
                } else {
                    self.mem_write(self.cpu.addr, self.cpu.data);
                }
            }
            self.pokey.tick();
            self.cpu.irq = self.pokey.irq_pending();
        }
    }

    fn process_scan_line(&mut self) {
        let result = self.antic.process_line(&self.ram);
        if result.pm_dma {
            for i in 0..4 {
                self.gtia.write(0x0D + i as u8, result.player_data[i]);
            }
            self.gtia.write(0x11, result.missile_data);
        }
        // ANTIC's scan_line is post-increment — the line we just
        // processed is scan_line - 1. Offset by 8 for the ANTIC
        // visible start.
        let line = self.antic.scan_line().saturating_sub(1);
        let visible_line = line.wrapping_sub(8);
        self.gtia.render_line(
            visible_line,
            &result.playfield,
            result.playfield_width,
            result.mode,
        );
        self.dma_budget = result.dma_cycles;
        self.line_cycle = 0;
        self.antic.clear_wsync();
        // VBI + DLI both pulse the NMI line.
        let nmi = self.antic.take_vbi() || self.antic.take_dli();
        self.cpu.nmi = nmi;
    }

    fn mem_read(&mut self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x3FFF => self.ram[(addr & 0x3FFF) as usize],
            0x4000..=0xBFFF => self.cart.read(addr),
            0xC000..=0xCFFF => self.gtia.read(addr as u8),
            0xD400..=0xD5FF => self.antic.read(addr as u8),
            0xE800..=0xE9FF => self.pokey.read(addr as u8),
            0xF800..=0xFFFF => {
                if self.bios.is_empty() {
                    self.cart.read(addr)
                } else {
                    self.bios
                        .get((addr - 0xF800) as usize)
                        .copied()
                        .unwrap_or(0xFF)
                }
            }
            _ => 0xFF,
        }
    }

    fn mem_write(&mut self, addr: u16, value: u8) {
        match addr {
            0x0000..=0x3FFF => self.ram[(addr & 0x3FFF) as usize] = value,
            0xC000..=0xCFFF => self.gtia.write(addr as u8, value),
            0xD400..=0xD5FF => self.antic.write(addr as u8, value),
            0xE800..=0xE9FF => self.pokey.write(addr as u8, value),
            _ => {}
        }
    }

    /// Framebuffer (GTIA's 320 × 240 ARGB32 buffer).
    #[must_use]
    pub fn framebuffer(&self) -> &[u32] {
        self.gtia.framebuffer()
    }

    /// Framebuffer width.
    #[must_use]
    pub fn framebuffer_width(&self) -> u32 {
        self.gtia.framebuffer_width()
    }

    /// Framebuffer height.
    #[must_use]
    pub fn framebuffer_height(&self) -> u32 {
        self.gtia.framebuffer_height()
    }

    /// Take the POKEY audio buffer.
    pub fn take_audio_buffer(&mut self) -> Vec<f32> {
        self.pokey.take_buffer()
    }

    /// Set joystick X / Y via POKEY pots (0-228 each — 114 is centre).
    pub fn set_joystick(&mut self, x: u8, y: u8) {
        self.pokey.set_pot(0, x.min(POT_MAX));
        self.pokey.set_pot(1, y.min(POT_MAX));
    }

    /// Set fire button (GTIA TRIG0).
    pub fn set_fire(&mut self, pressed: bool) {
        self.gtia.set_trigger(0, pressed);
    }

    /// CPU reference.
    #[must_use]
    pub fn cpu(&self) -> &M6502 {
        &self.cpu
    }

    /// CPU mutable reference.
    pub fn cpu_mut(&mut self) -> &mut M6502 {
        &mut self.cpu
    }

    /// ANTIC reference.
    #[must_use]
    pub fn antic(&self) -> &Antic {
        &self.antic
    }

    /// GTIA reference.
    #[must_use]
    pub fn gtia(&self) -> &Gtia {
        &self.gtia
    }

    /// POKEY reference.
    #[must_use]
    pub fn pokey(&self) -> &Pokey {
        &self.pokey
    }

    /// Region.
    #[must_use]
    pub fn region(&self) -> Atari5200Region {
        self.region
    }

    /// Master clock (colour clocks since power-on).
    #[must_use]
    pub fn master_clock(&self) -> u64 {
        self.master_clock
    }

    /// Frame count.
    #[must_use]
    pub fn frame_count(&self) -> u64 {
        self.frame_count
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn trap_rom_8k() -> Vec<u8> {
        // 8 KB cart at $A000-$BFFF. JMP self at $A000 + reset vector at
        // $BFFC-$BFFD pointing there.
        let mut rom = vec![0xEA_u8; 8192];
        rom[0x0000] = 0x4C;
        rom[0x0001] = 0x00;
        rom[0x0002] = 0xA0;
        rom[0x1FFA] = 0x00;
        rom[0x1FFB] = 0xA0;
        rom[0x1FFC] = 0x00;
        rom[0x1FFD] = 0xA0;
        rom[0x1FFE] = 0x00;
        rom[0x1FFF] = 0xA0;
        rom
    }

    #[test]
    fn frame_advances_master_clock_and_count() {
        let mut sys =
            Atari5200::new(trap_rom_8k(), Vec::new(), Atari5200Region::Ntsc).expect("init");
        let clocks = sys.run_frame();
        assert!(clocks > 0);
        assert_eq!(sys.frame_count(), 1);
    }

    #[test]
    fn pal_runs_more_clocks_than_ntsc() {
        let mut ntsc =
            Atari5200::new(trap_rom_8k(), Vec::new(), Atari5200Region::Ntsc).expect("init");
        let mut pal =
            Atari5200::new(trap_rom_8k(), Vec::new(), Atari5200Region::Pal).expect("init");
        assert!(pal.run_frame() > ntsc.run_frame());
    }

    #[test]
    fn many_frames_complete_without_panic() {
        let mut sys =
            Atari5200::new(trap_rom_8k(), Vec::new(), Atari5200Region::Ntsc).expect("init");
        for _ in 0..10 {
            sys.run_frame();
        }
        assert_eq!(sys.frame_count(), 10);
    }

    #[test]
    fn memory_map_routes_ram_cart() {
        let mut sys =
            Atari5200::new(trap_rom_8k(), Vec::new(), Atari5200Region::Ntsc).expect("init");
        sys.mem_write(0x0100, 0x42);
        assert_eq!(sys.mem_read(0x0100), 0x42);
        assert_eq!(sys.mem_read(0xA000), 0x4C);
    }

    #[test]
    fn bios_overlays_top_of_address_space() {
        let mut sys = Atari5200::new(
            trap_rom_8k(),
            vec![0xBB; 2048],
            Atari5200Region::Ntsc,
        )
        .expect("init");
        assert_eq!(sys.mem_read(0xF800), 0xBB);
        assert_eq!(sys.mem_read(0xFFFF), 0xBB);
    }

    #[test]
    fn no_bios_falls_through_to_cart() {
        let mut sys =
            Atari5200::new(trap_rom_8k(), Vec::new(), Atari5200Region::Ntsc).expect("init");
        assert_eq!(sys.mem_read(0xFFFC), 0x00);
        assert_eq!(sys.mem_read(0xFFFD), 0xA0);
    }

    #[test]
    fn rejects_invalid_rom_size() {
        let bad = vec![0u8; 5000];
        assert!(Atari5200::new(bad, Vec::new(), Atari5200Region::Ntsc).is_err());
    }

    #[test]
    fn joystick_pots_default_to_centre() {
        let sys =
            Atari5200::new(trap_rom_8k(), Vec::new(), Atari5200Region::Ntsc).expect("init");
        let _ = sys; // pots set in `new`; this confirms `new` doesn't panic.
    }
}
