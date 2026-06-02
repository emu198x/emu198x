//! Atari 2600 (VCS) machine wiring.
//!
//! Fresh-write against the workspace pin-driven bus pattern (RULES.md
//! rule 6). The donor at `Emu198x-Oldest/crates/machine-atari-2600`
//! used the deprecated `emu_core::Bus` callback and could not port
//! directly; this file uses it as a system spec — 6507's 13-bit
//! address decode, master-colour-clock tick model with CPU and RIOT
//! at 1/3 the rate, TIA WSYNC CPU halt, cartridge hotspot bank
//! switching (F8 / F6 / F4) — but the wiring is written against
//! [`mos_6502::M6502`]'s public pin fields.
//!
//! # The Atari 2600 (Video Computer System)
//!
//! The 2600 (Atari, 1977) is the **second-generation cartridge-based
//! home console** that defined the medium. Famously hard to program:
//! the TIA has no framebuffer, so the CPU must "race the beam" and
//! update video registers between scanlines. The chip-level
//! complexity is in `atari-tia`; this crate wires the 6507 to it.
//!
//! - **CPU:** MOS 6507 (a 6502 pin-limited to 13 address lines, so
//!   the effective address space is 8 KB mirrored across the 16-bit
//!   range)
//! - **TIA:** Atari custom video + audio
//! - **RIOT:** MOS 6532 (128 bytes RAM + I/O ports + timer)
//! - **Cart:** 2 KB / 4 KB / 8 KB (F8) / 16 KB (F6) / 32 KB (F4)
//!
//! # Memory decode (post-`addr & 0x1FFF`)
//!
//! - **A12 = 1:** Cartridge ROM at `$1000-$1FFF`
//! - **A12 = 0, A7 = 0:** TIA registers (writes shape the next scanline)
//! - **A12 = 0, A7 = 1, A9 = 0:** RIOT RAM
//! - **A12 = 0, A7 = 1, A9 = 1:** RIOT I/O + timer
//!
//! # Clock model
//!
//! Master clock = TIA colour clock (3.579545 MHz NTSC, 3.546894 MHz
//! PAL). The 6507 + RIOT both tick every 3rd colour clock. TIA's
//! WSYNC line halts the CPU until the next horizontal blank.
//!
//! One scanline = 228 colour clocks = 76 CPU cycles. PAL frames are
//! 312 lines / NTSC 262 lines, but the actual frame is
//! software-controlled — the CPU stops driving VSYNC after as many
//! lines as the game wants.

mod cartridge;

pub use cartridge::{BankingScheme, Cartridge};

use atari_tia::{Tia, TiaRegion, CLOCKS_PER_LINE};
use mos_6502::M6502;
use mos_riot_6532::Riot6532;

/// Atari 2600 region.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Atari2600Region {
    Ntsc,
    Pal,
}

impl Atari2600Region {
    fn tia_region(self) -> TiaRegion {
        match self {
            Self::Ntsc => TiaRegion::Ntsc,
            Self::Pal => TiaRegion::Pal,
        }
    }

    fn lines_per_frame(self) -> u16 {
        match self {
            Self::Ntsc => 262,
            Self::Pal => 312,
        }
    }
}

/// Atari 2600 machine.
pub struct Atari2600 {
    cpu: M6502,
    tia: Tia,
    riot: Riot6532,
    cart: Cartridge,
    /// Master clock = colour clocks since power-on.
    master_clock: u64,
    /// Target colour clocks per frame (lines × 228).
    clocks_per_frame: u64,
    region: Atari2600Region,
    frame_count: u64,
}

impl Atari2600 {
    /// Create a new Atari 2600 with the given cart ROM and region.
    pub fn new(rom: Vec<u8>, region: Atari2600Region) -> Result<Self, String> {
        let cart = Cartridge::from_rom(&rom)?;
        let mut cpu = M6502::new();
        cpu.reset();
        let tia = Tia::new(region.tia_region());
        let riot = Riot6532::new();
        let clocks_per_frame =
            u64::from(region.lines_per_frame()) * u64::from(CLOCKS_PER_LINE);
        Ok(Self {
            cpu,
            tia,
            riot,
            cart,
            master_clock: 0,
            clocks_per_frame,
            region,
            frame_count: 0,
        })
    }

    /// Run one frame and return colour clocks consumed.
    pub fn run_frame(&mut self) -> u64 {
        let start = self.master_clock;
        let target = start + self.clocks_per_frame;
        while self.master_clock < target {
            self.tick_colour_clock();
        }
        self.frame_count += 1;
        self.master_clock - start
    }

    fn tick_colour_clock(&mut self) {
        self.master_clock += 1;
        self.tia.tick();
        // CPU + RIOT tick every 3rd colour clock.
        if self.master_clock.is_multiple_of(3) {
            if !self.tia.wsync_halt {
                self.cpu.tick();
                if self.cpu.rw {
                    self.cpu.data_in = self.mem_read(self.cpu.addr);
                } else {
                    self.mem_write(self.cpu.addr, self.cpu.data);
                }
            }
            self.riot.tick();
        }
        // 6507 has no IRQ/NMI pins exposed externally.
        self.cpu.irq = false;
        self.cpu.nmi = false;
    }

    fn mem_read(&mut self, addr: u16) -> u8 {
        let addr = addr & 0x1FFF;
        if addr & 0x1000 != 0 {
            self.cart.read(addr)
        } else if addr & 0x0080 == 0 {
            self.tia.read(addr as u8)
        } else {
            self.riot.read(addr)
        }
    }

    fn mem_write(&mut self, addr: u16, value: u8) {
        let addr = addr & 0x1FFF;
        if addr & 0x1000 != 0 {
            self.cart.write(addr, value);
        } else if addr & 0x0080 == 0 {
            self.tia.write(addr as u8, value);
        } else {
            self.riot.write(addr, value);
        }
    }

    /// Framebuffer (160 × lines).
    #[must_use]
    pub fn framebuffer(&self) -> &[u32] {
        self.tia.framebuffer()
    }

    /// Framebuffer width (TIA: 160).
    #[must_use]
    pub fn framebuffer_width(&self) -> u32 {
        self.tia.framebuffer_width()
    }

    /// Framebuffer height (depends on TIA region).
    #[must_use]
    pub fn framebuffer_height(&self) -> u32 {
        self.tia.framebuffer_height()
    }

    /// Set RIOT port A input — joystick directions byte.
    pub fn set_joystick_input(&mut self, value: u8) {
        self.riot.set_port_a_input(value);
    }

    /// Set RIOT port B input — console switches byte.
    pub fn set_switch_input(&mut self, value: u8) {
        self.riot.set_port_b_input(value);
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

    /// TIA reference.
    #[must_use]
    pub fn tia(&self) -> &Tia {
        &self.tia
    }

    /// RIOT reference.
    #[must_use]
    pub fn riot(&self) -> &Riot6532 {
        &self.riot
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

    /// Region.
    #[must_use]
    pub fn region(&self) -> Atari2600Region {
        self.region
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn trap_rom() -> Vec<u8> {
        // 4 KB cart. Reset vector at $FFFC-$FFFD (high byte $1F lives
        // in the cart). JMP self at $1000 → 4C 00 F0 → in 4K cart at
        // offset 0 we want $4C $00 $F0. Actually 6502 reset reads
        // from $FFFC = cart offset $0FFC. So put $00 / $10 there so
        // CPU jumps to $1000 (start of cart).
        let mut rom = vec![0xEA_u8; 4096];
        rom[0x0000] = 0x4C;
        rom[0x0001] = 0x00;
        rom[0x0002] = 0x10;
        rom[0x0FFC] = 0x00;
        rom[0x0FFD] = 0x10;
        rom
    }

    #[test]
    fn frame_advances_master_clock_and_count() {
        let mut sys = Atari2600::new(trap_rom(), Atari2600Region::Ntsc).expect("init");
        let clocks = sys.run_frame();
        assert!(clocks > 0);
        assert_eq!(sys.frame_count(), 1);
    }

    #[test]
    fn pal_has_more_clocks_per_frame_than_ntsc() {
        let mut ntsc = Atari2600::new(trap_rom(), Atari2600Region::Ntsc).expect("init");
        let mut pal = Atari2600::new(trap_rom(), Atari2600Region::Pal).expect("init");
        assert!(pal.run_frame() > ntsc.run_frame());
    }

    #[test]
    fn many_frames_complete_without_panic() {
        let mut sys = Atari2600::new(trap_rom(), Atari2600Region::Ntsc).expect("init");
        for _ in 0..30 {
            sys.run_frame();
        }
        assert_eq!(sys.frame_count(), 30);
    }

    #[test]
    fn memory_decode_routes_tia_riot_cart() {
        let mut sys = Atari2600::new(trap_rom(), Atari2600Region::Ntsc).expect("init");
        // Cart space (A12 = 1).
        assert_eq!(sys.mem_read(0x1000), 0x4C);
        // RIOT RAM (A7=1, A9=0) write/read at $80.
        sys.mem_write(0x0080, 0x42);
        assert_eq!(sys.mem_read(0x0080), 0x42);
        // TIA write at $0009 (COLUBK) — doesn't panic.
        sys.mem_write(0x0009, 0x9A);
    }

    #[test]
    fn address_mirroring_13_bit() {
        let mut sys = Atari2600::new(trap_rom(), Atari2600Region::Ntsc).expect("init");
        sys.mem_write(0x0080, 0x77);
        // $2080 & $1FFF = $0080.
        assert_eq!(sys.mem_read(0x2080), 0x77);
    }

    #[test]
    fn rejects_invalid_rom_size() {
        let bad = vec![0u8; 5000];
        assert!(Atari2600::new(bad, Atari2600Region::Ntsc).is_err());
    }
}
