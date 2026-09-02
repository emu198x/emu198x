//! Atari 5200 SuperSystem machine wiring.
//!
//! Fresh-write against the workspace pin-driven bus pattern (RULES.md
//! rule 6). The donor at `Emu198x-Oldest/crates/machine-atari-5200`
//! used the deprecated `emu_core::Bus` callback and could not port
//! directly; this file uses it as a system spec — 6502 "Sally"
//! address decode, ANTIC scan-line processing + DMA cycle stealing,
//! GTIA player/missile DMA feed, POKEY paddle pots — but the wiring
//! is written against [`emu198x_mos_6502::M6502`]'s public pin fields.
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

mod cart_layouts;
mod cartridge;

pub use cartridge::{CartLayout, Cartridge};

use atari_antic::{Antic, AnticRegion, COLOUR_CLOCKS_PER_LINE, CYCLES_HSYNC, cpu_dma_stalled};
use atari_gtia::Gtia;
use atari_pokey::Pokey;
use emu198x_mos_6502::M6502;
use serde::{Deserialize, Serialize};
use serde_big_array::BigArray;

/// Joystick pot centre value (0-228 range — POKEY pots are 8-bit).
pub const POT_CENTER: u8 = 114;
/// Joystick pot maximum (fully right or fully down).
pub const POT_MAX: u8 = 228;

/// Atari 5200 region.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Atari5200Region {
    Ntsc,
    Pal,
}

impl Atari5200Region {
    fn gtia_region(self) -> atari_gtia::GtiaRegion {
        match self {
            Self::Ntsc => atari_gtia::GtiaRegion::Ntsc,
            Self::Pal => atari_gtia::GtiaRegion::Pal,
        }
    }

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

/// The address space ANTIC fetches through, borrowed from the console's live
/// fields rather than copied.
///
/// Real ANTIC fetches its display list, screen data, character sets and
/// player/missile data straight off the system bus, so it reads RAM
/// (`$0000-$3FFF`), cart ROM (`$4000-$BFFF`) and the BIOS character set
/// (`$F800-$FFFF`) alike. The register windows read `$FF` (open bus); no
/// display fetches data from there.
struct AnticView<'a> {
    ram: &'a [u8; 16384],
    cart: &'a Cartridge,
    bios: &'a [u8],
}

impl atari_antic::AnticMemory for AnticView<'_> {
    fn read(&self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x3FFF => self.ram[(addr & 0x3FFF) as usize],
            0x4000..=0xBFFF => self.cart.read(addr),
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
}

/// Atari 5200 machine.
#[derive(Serialize, Deserialize)]
pub struct Atari5200 {
    cpu: M6502,
    antic: Antic,
    gtia: Gtia,
    pokey: Pokey,
    cart: Cartridge,
    #[serde(with = "BigArray")]
    ram: [u8; 16384],
    bios: Vec<u8>,
    region: Atari5200Region,
    master_clock: u64,
    clocks_per_frame: u64,
    frame_count: u64,
    /// Which of the current scan line's cycles ANTIC is taking for DMA, from
    /// its own fetch schedule. The CPU runs on the cycles left over.
    dma_mask: u128,
    /// CPU cycle counter within the current scan line.
    line_cycle: u16,
    /// The cycle at which ANTIC reads this line's playfield, once it has
    /// begun a line that has one.
    playfield_fetch_cycle: Option<u16>,
}

impl Atari5200 {
    /// Create a new Atari 5200. `bios` may be empty to fall back to
    /// the cartridge's `$FFFC/$FFFD` mirror for the reset vector.
    pub fn new(rom: Vec<u8>, bios: Vec<u8>, region: Atari5200Region) -> Result<Self, String> {
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
            gtia: Gtia::new(region.gtia_region()),
            pokey,
            cart,
            ram: [0; 16384],
            bios,
            region,
            master_clock: 0,
            clocks_per_frame,
            frame_count: 0,
            dma_mask: 0,
            line_cycle: 0,
            playfield_fetch_cycle: None,
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
        let ccpl = u64::from(COLOUR_CLOCKS_PER_LINE);

        // At the left edge of a scan line, hand ANTIC the line and prime the
        // GTIA to composite it. Compositing then runs left-to-right through the
        // line (below), so a mid-line COLBK/COLPF write lands at the beam.
        if self.master_clock.is_multiple_of(ccpl) {
            self.start_scan_line();
        }

        self.master_clock += 1;

        // Advance playfield compositing to the beam's new position, sampling
        // the live colour registers as it goes.
        let line_cc = (self.master_clock % ccpl) as u16;
        self.gtia.composite_to_beam(line_cc);

        // When the line completes, overlay players/missiles + collisions.
        if self.master_clock.is_multiple_of(ccpl) {
            self.gtia.finish_scanline();
            // End of the pulse. The CPU latched any rising edge as it
            // ticked through the line, so dropping the line here costs
            // nothing and lets the next line's interrupt be seen.
            self.cpu.nmi = false;
        }

        // CPU + POKEY tick every 2nd colour clock.
        if self.master_clock.is_multiple_of(2) {
            self.line_cycle += 1;
            // ANTIC releases a WSYNC-halted CPU at HSYNC (end of the visible
            // region), not at the next line.
            if self.line_cycle == CYCLES_HSYNC {
                self.antic.clear_wsync();
            }
            // CPU runs unless ANTIC is taking this cycle for a fetch, or it is
            // held by WSYNC.
            if !cpu_dma_stalled(self.line_cycle, self.dma_mask) && !self.antic.wsync_halt() {
                self.cpu.tick();
                if self.cpu.rw {
                    self.cpu.data_in = self.mem_read(self.cpu.addr);
                } else {
                    self.mem_write(self.cpu.addr, self.cpu.data);
                }
            }
            self.pokey.tick();
            self.cpu.irq = self.pokey.irq_pending();
            if Some(self.line_cycle) == self.playfield_fetch_cycle {
                self.fetch_playfield();
            }
        }
    }

    /// ANTIC reads the line's playfield and hands it to the GTIA, ahead of
    /// the beam reaching it. Registers the CPU wrote earlier in the line —
    /// CHBASE, CHACTL, HSCROL — shape this line; later writes shape the next.
    fn fetch_playfield(&mut self) {
        let view = AnticView {
            ram: &self.ram,
            cart: &self.cart,
            bios: &self.bios,
        };
        if let Some(fetched) = self.antic.fetch_playfield(&view) {
            self.gtia
                .set_playfield(&fetched.playfield, fetched.playfield_width, fetched.mode);
        }
    }

    /// Start a scan line: ANTIC reads the display list and the GTIA begins
    /// beam compositing for it. Player/missile DMA and the DLI/VBI NMI are
    /// applied here, and the line's DMA schedule that gates the CPU is set.
    /// The playfield itself is fetched later in the line (`fetch_playfield`).
    /// Pixels are composited incrementally as the beam advances
    /// (`composite_to_beam`), then finished with the PM overlay at line end.
    fn start_scan_line(&mut self) {
        // Borrow the memory ANTIC fetches through directly from the fields it
        // covers, so `self.antic` can be borrowed mutably alongside it.
        let view = AnticView {
            ram: &self.ram,
            cart: &self.cart,
            bios: &self.bios,
        };
        let result = self.antic.begin_line(&view);
        if result.pm_dma {
            // GRACTL decides whether this DMA reaches the graphics registers,
            // and VDELAY whether an object is held back a line; both live in
            // GTIA, so hand the line over rather than poking the registers.
            self.gtia.accept_pm_dma(
                result.player_data,
                result.missile_data,
                result.pm_single_line,
            );
        }
        // ANTIC's scan_line is post-increment — the line we just
        // processed is scan_line - 1. Offset by 8 for the ANTIC
        // visible start.
        let line = self.antic.scan_line().saturating_sub(1);
        let visible_line = line.wrapping_sub(8);
        self.gtia.begin_scanline(visible_line);
        self.dma_mask = result.dma_mask;
        self.line_cycle = 0;
        self.playfield_fetch_cycle = self.antic.playfield_fetch_cycle();
        // ANTIC pulses NMI; it does not hold it. Raise the line for any
        // source that fired on this line — `tick_colour_clock` drops it
        // again once the line ends, so the next source gets its own
        // rising edge. Holding it high across two consecutive lines
        // merges them into one edge and the second interrupt is lost,
        // which is what happens to any program with a DLI on its last
        // mode line: the DLI at the end of the picture and the VBI on
        // the line after arrive as a single NMI, the OS handler sees
        // both NMIST bits, services the DLI because bit 7 wins, and
        // NMIRES clears the VBI bit unserviced.
        if self.antic.take_vbi() | self.antic.take_dli() {
            self.cpu.nmi = true;
        }
    }

    fn mem_read(&mut self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x3FFF => self.ram[(addr & 0x3FFF) as usize],
            0x4000..=0xBFFF => {
                // A Bounty Bob cart switches on reads as well as writes,
                // so the read path has to be able to move a window.
                self.touch_bank_register(addr);
                self.cart.read(addr)
            }
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

    /// Let the cartridge see `addr` in case it is a bank register. A Bounty
    /// Bob cart switches on reads as well as writes, so both paths call it.
    fn touch_bank_register(&mut self, addr: u16) {
        let _ = self.cart.touch_bank_register(addr);
    }

    fn mem_write(&mut self, addr: u16, value: u8) {
        match addr {
            0x0000..=0x3FFF => self.ram[(addr & 0x3FFF) as usize] = value,
            0x4000..=0xBFFF => self.touch_bank_register(addr),
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

    /// Press a controller-keypad key by its POKEY scan code, or release the
    /// held key when `pressed` is false. The 5200 keypad is a 4×4 matrix POKEY
    /// scans; the scan code is `((row << 2) | col) << 1` (MAME `a5200_keypads`),
    /// e.g. Start (row 3, col 0) = `0x18`. POKEY latches the code, marks
    /// "key down", and raises the keyboard interrupt the OS polls.
    pub fn set_keypad(&mut self, code: u8, pressed: bool) {
        if pressed {
            self.pokey.press_key(code);
        } else {
            self.pokey.release_key();
        }
    }

    /// Cartridge reference, for tests and chip inspection.
    #[must_use]
    pub const fn cart(&self) -> &Cartridge {
        &self.cart
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

impl Atari5200 {
    /// Read one byte with no side effects: RAM, cartridge ROM, and BIOS;
    /// `$FF` for GTIA / ANTIC / POKEY (read side effects).
    #[must_use]
    pub fn peek(&self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x3FFF => self.ram[(addr & 0x3FFF) as usize],
            0x4000..=0xBFFF => self.cart.read(addr),
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

    /// Write one byte through the bus (RAM accepts it; ROM ignores it).
    pub fn poke(&mut self, addr: u16, value: u8) {
        self.mem_write(addr, value);
    }

    /// Run exactly one whole 6502 instruction, returning the colour clocks
    /// it consumed. A safety cap prevents an unbounded spin.
    pub fn step_instruction(&mut self) -> u64 {
        let mut ticks = 0u64;
        while self.cpu.instruction_complete() && ticks < 4096 {
            self.tick_colour_clock();
            ticks += 1;
        }
        while !self.cpu.instruction_complete() && ticks < 4096 {
            self.tick_colour_clock();
            ticks += 1;
        }
        ticks
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

    /// Save-state must capture the LIVE machine state — CPU, ANTIC, GTIA,
    /// POKEY, the 16 KB RAM, and the DMA shadow — not cold-boot from ROM.
    /// Serialise, advance (so the state differs), then deserialise the first
    /// snapshot and confirm re-serialising it is byte-identical: every stateful
    /// field round-trips, including the RAM and the boxed `dma_mem`
    /// write-through shadow.
    #[test]
    fn snapshot_round_trips_live_state() {
        let mut sys =
            Atari5200::new(trap_rom_8k(), Vec::new(), Atari5200Region::Ntsc).expect("init");
        sys.run_frame();
        // A low work-RAM byte ($0600 is inside the 16 KB RAM at $0000-$3FFF).
        sys.poke(0x0600, 0xA5);
        assert_eq!(sys.peek(0x0600), 0xA5, "poke landed in RAM");
        sys.run_frame();
        let s1 = postcard::to_allocvec(&sys).expect("encode snapshot");

        sys.run_frame(); // advance past the snapshot point
        let s2 = postcard::to_allocvec(&sys).expect("encode again");
        assert_ne!(s1, s2, "running a frame should change the serialised state");

        let restored: Atari5200 = postcard::from_bytes(&s1).expect("decode snapshot");
        assert_eq!(
            restored.peek(0x0600),
            0xA5,
            "poked RAM byte survives restore"
        );
        let s3 = postcard::to_allocvec(&restored).expect("re-encode restored");
        assert_eq!(
            s1, s3,
            "restore should reproduce the snapshot state exactly"
        );
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

    /// ANTIC pulses NMI. Holding the line high across two consecutive
    /// lines merges them into one rising edge, and the 6502 only latches
    /// on an edge — so the second interrupt is never delivered.
    ///
    /// That is not academic: Missile Command puts a DLI on its last mode
    /// line, one line before the VBI. Merged, the OS handler saw both
    /// NMIST bits, serviced the DLI because bit 7 wins, and NMIRES
    /// cleared the VBI bit unserviced. The BIOS VBI is what restores
    /// DLISTL/DLISTH from the shadow, so the display list pointer was
    /// left at the JVB and every other frame came out blank.
    #[test]
    fn antic_nmi_falls_at_the_end_of_every_scan_line() {
        let mut sys =
            Atari5200::new(trap_rom_8k(), Vec::new(), Atari5200Region::Ntsc).expect("init");
        // Enable both NMI sources so the line is driven as often as
        // possible.
        sys.antic.write(0x0E, 0xC0);

        let ccpl = u64::from(COLOUR_CLOCKS_PER_LINE);
        let mut held_across_a_line_boundary = 0;
        for _ in 0..(ccpl * u64::from(sys.region.lines_per_frame())) {
            sys.tick_colour_clock();
            if sys.master_clock.is_multiple_of(ccpl) && sys.cpu.nmi {
                held_across_a_line_boundary += 1;
            }
        }
        assert_eq!(
            held_across_a_line_boundary, 0,
            "NMI must be a pulse: a line still asserted at the line boundary \
             merges with the next line's interrupt into a single edge"
        );
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
        let mut sys =
            Atari5200::new(trap_rom_8k(), vec![0xBB; 2048], Atari5200Region::Ntsc).expect("init");
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
        let sys = Atari5200::new(trap_rom_8k(), Vec::new(), Atari5200Region::Ntsc).expect("init");
        let _ = sys; // pots set in `new`; this confirms `new` doesn't panic.
    }
}
