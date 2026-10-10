//! Atari 7800 ProSystem machine wiring.
//!
//! Fresh-write against the workspace pin-driven bus pattern (RULES.md
//! rule 6). Donor at `Emu198x-Oldest/crates/machine-atari-7800/src/lib.rs`
//! used the deprecated `emu_core::Bus` callback and could not port
//! directly; the donor is used here as the system spec — 6502C "Sally"
//! address decode, MARIA's zone-based display-list rendering, RIOT for
//! joystick + console switches + timer, TIA-audio register stub — but the
//! wiring is written against [`emu198x_mos_6502::M6502`]'s public pin fields.
//!
//! # The Atari 7800 ProSystem
//!
//! Released in 1986 (designed in 1984 but delayed when Warner sold Atari
//! to Tramiel). Backward-compatible with the 2600 via the same TIA + RIOT
//! pair; native 7800 games drive MARIA instead — a zone-based display
//! processor that DMAs sprite and tile data from RAM each scanline,
//! freeing the 6502C "Sally" CPU from the 2600's race-the-beam model.
//!
//! - **CPU:** MOS 6502C "Sally" — stock 6502 with Atari's HALT pin for
//!   MARIA DMA cycle stealing.
//! - **MARIA:** display processor (zone-based DLL/DL, palette,
//!   320 × 240 framebuffer). See [`atari_maria`].
//! - **RIOT:** I/O and timer (P0 / P1 joystick + console switches).
//! - **TIA:** two-channel audio and controller fire inputs in 7800 mode.
//! - **RAM:** 4 KB main at `$1800-$27FF` (mirrored to `$3FFF`), 192 B
//!   zero-page (`$0040-$00FF`), 192 B stack (`$0140-$01FF`).
//! - **Cart:** up to 128 KB; 16 KB / 32 KB / 48 KB flat or SuperGame
//!   banking. See [`Cartridge`].
//!
//! # Memory map
//!
//! | Range         | Contents                                         |
//! |---------------|--------------------------------------------------|
//! | `$0000-$001F` | TIA (audio only in 7800 mode)                    |
//! | `$0020-$003F` | MARIA registers                                  |
//! | `$0040-$00FF` | Zero-page RAM (192 B)                            |
//! | `$0100-$011F` | TIA mirror                                       |
//! | `$0120-$013F` | MARIA mirror                                     |
//! | `$0140-$01FF` | Stack RAM (192 B)                                |
//! | `$0280-$02FF` | RIOT I/O + timer                                 |
//! | `$1800-$27FF` | Main RAM (4 KB)                                  |
//! | `$2800-$3FFF` | Main RAM mirror                                  |
//! | `$4000-$FFFF` | Cartridge ROM                                    |
//!
//! # Clock model
//!
//! The native oscillator (14.32 MHz NTSC, 14.19 MHz PAL) drives the loop.
//! TIA ticks every fourth oscillator period; CPU + RIOT every eighth.
//! The current line renderer uses 912 oscillator periods (114 CPU cycles)
//! per scanline. MARIA renders one scanline at every
//! boundary and stalls the CPU for the line's DMA budget. WSYNC writes
//! halt the CPU until the next line. DLI fires NMI.

mod cartridge;
mod tia_audio;

pub use cartridge::{Cartridge, PokeyLocation};
pub use tia_audio::TiaAudio;

use atari_maria::{Maria, MariaRegion};
use atari_pokey::Pokey;
use emu198x_mos_6502::M6502;
use mos_riot_6532::Riot6532;
use serde::{Deserialize, Serialize};
use serde_big_array::BigArray;

const MASTER_CLOCKS_PER_LINE: u16 = 912;

/// Atari 7800 region.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Atari7800Region {
    Ntsc,
    Pal,
}

impl Atari7800Region {
    fn maria_region(self) -> MariaRegion {
        match self {
            Self::Ntsc => MariaRegion::Ntsc,
            Self::Pal => MariaRegion::Pal,
        }
    }

    const fn lines_per_frame(self) -> u16 {
        match self {
            Self::Ntsc => 263,
            Self::Pal => 313,
        }
    }

    /// Native oscillator rate in Hz, using the existing regional clock rounding.
    #[must_use]
    pub const fn master_hz(self) -> u64 {
        match self {
            Self::Ntsc => 14_318_180,
            Self::Pal => 14_187_576,
        }
    }

    /// Native oscillator periods consumed by the current raster per frame.
    #[must_use]
    pub const fn frame_ticks(self) -> u64 {
        self.lines_per_frame() as u64 * MASTER_CLOCKS_PER_LINE as u64
    }

    fn cpu_hz(self) -> u32 {
        (self.master_hz() / 8) as u32
    }
}

/// Atari 7800 machine.
#[derive(Serialize, Deserialize)]
pub struct Atari7800 {
    cpu: M6502,
    maria: Maria,
    riot: Riot6532,
    tia_audio: TiaAudio,
    pokey: Option<Pokey>,
    pokey_location: Option<PokeyLocation>,
    cart: Cartridge,
    #[serde(with = "BigArray")]
    ram_zp: [u8; 192],
    #[serde(with = "BigArray")]
    ram_stack: [u8; 192],
    #[serde(with = "BigArray")]
    ram_main: [u8; 4096],
    region: Atari7800Region,
    master_clock: u64,
    clocks_per_frame: u64,
    frame_count: u64,
    dma_budget: u16,
    line_cycle: u16,
    /// The CPU pins hold a transaction awaiting the next phase-2 bus access.
    cpu_bus_active: bool,
}

impl Atari7800 {
    pub fn new(rom: Vec<u8>, region: Atari7800Region) -> Result<Self, String> {
        let cart = Cartridge::from_rom(&rom)?;
        Ok(Self::from_cartridge(cart, region))
    }

    /// Cold-boot with the installed cartridge and a selected region.
    /// ROM/configuration survive; cartridge banks and RAM return to power-on state.
    #[must_use]
    pub fn cold_boot(&self, region: Atari7800Region) -> Self {
        Self::from_cartridge(self.cart.cold_boot(), region)
    }

    fn from_cartridge(cart: Cartridge, region: Atari7800Region) -> Self {
        let pokey_location = cart.pokey_location();
        let pokey = pokey_location.map(|_| Pokey::new(region.cpu_hz()));
        let mut cpu = M6502::new();
        cpu.reset();
        let mut riot = Riot6532::new();
        riot.input_a = 0xFF;
        riot.input_b = 0xFF;
        let clocks_per_frame = region.frame_ticks();
        Self {
            cpu,
            maria: Maria::new(region.maria_region()),
            riot,
            tia_audio: TiaAudio::new(),
            pokey,
            pokey_location,
            cart,
            ram_zp: [0; 192],
            ram_stack: [0; 192],
            ram_main: [0; 4096],
            region,
            master_clock: 0,
            clocks_per_frame,
            frame_count: 0,
            dma_budget: 0,
            line_cycle: 0,
            cpu_bus_active: true,
        }
    }

    /// Run one frame and return elapsed native oscillator periods.
    pub fn run_frame(&mut self) -> u64 {
        let start = self.master_clock;
        let target = start + self.clocks_per_frame;
        while self.master_clock < target {
            self.tick_master_clock();
        }
        self.frame_count += 1;
        self.master_clock - start
    }

    fn tick_master_clock(&mut self) {
        self.master_clock += 1;
        self.maria.address_in = self.cpu.addr;
        self.maria.tick_clock();
        if self.master_clock.is_multiple_of(4) {
            self.tia_audio.tick();
        }

        if self
            .master_clock
            .is_multiple_of(u64::from(MASTER_CLOCKS_PER_LINE))
        {
            self.process_scan_line();
        }

        if self.maria.phi1 {
            self.line_cycle += 1;
            self.cpu_bus_active = self.line_cycle > self.dma_budget;
            if self.cpu_bus_active {
                // RDY holds NMOS reads while leaving writes and the NMI
                // detector clocked. DMA still owns complete legacy slots.
                self.cpu.rdy = !self.maria.wsync_halt();
                self.cpu.tick();
            }
        }
        if self.maria.phi2 {
            self.riot.tick();
            if self.cpu_bus_active {
                if self.cpu.rw {
                    self.cpu.data_in = self.mem_read(self.cpu.addr);
                } else {
                    self.mem_write(self.cpu.addr, self.cpu.data);
                }
            }
        }
        // POKEY still couples chip advancement to fixed-rate host sampling.
        // Variable PCLK wiring requires separating those clocks first; the
        // staged MARIA branch must not ship before that integration gate.
        if self.master_clock.is_multiple_of(8)
            && let Some(pokey) = &mut self.pokey
        {
            pokey.tick();
        }
    }

    fn process_scan_line(&mut self) {
        let cart = &self.cart;
        let ram_zp = &self.ram_zp;
        let ram_stack = &self.ram_stack;
        let ram_main = &self.ram_main;
        let dma_cycles = self.maria.render_line(&mut |addr| match addr {
            0x0040..=0x00FF => ram_zp[(addr - 0x40) as usize],
            0x0140..=0x01FF => ram_stack[(addr - 0x140) as usize],
            0x1800..=0x3FFF => ram_main[((addr - 0x1800) & 0x0FFF) as usize],
            0x4000..=0xFFFF => cart.read(addr),
            _ => 0,
        });
        self.dma_budget = dma_cycles;
        self.line_cycle = 0;
        self.maria.clear_wsync();
        self.cpu.nmi = self.maria.take_dli();
    }

    fn mem_read(&mut self, addr: u16) -> u8 {
        if let (Some(pokey), Some(location)) = (&self.pokey, self.pokey_location) {
            let base = location.base();
            if (base..base + 16).contains(&addr) {
                return pokey.read((addr - base) as u8);
            }
        }
        match addr {
            0x0000..=0x001F => self.tia_audio.read(addr as u8),
            0x0020..=0x003F => self.maria.read(addr as u8 - 0x20),
            0x0040..=0x00FF => self.ram_zp[(addr - 0x40) as usize],
            0x0100..=0x011F => self.tia_audio.read((addr & 0x1F) as u8),
            0x0120..=0x013F => self.maria.read((addr & 0x1F) as u8),
            0x0140..=0x01FF => self.ram_stack[(addr - 0x140) as usize],
            0x0200..=0x027F => {
                if addr & 0x20 != 0 {
                    self.maria.read((addr & 0x1F) as u8)
                } else {
                    self.tia_audio.read((addr & 0x1F) as u8)
                }
            }
            0x0280..=0x02FF => self.riot.read(addr),
            0x0300..=0x03FF => {
                if addr & 0x80 != 0 {
                    self.riot.read(addr)
                } else if addr & 0x20 != 0 {
                    self.maria.read((addr & 0x1F) as u8)
                } else {
                    self.tia_audio.read((addr & 0x1F) as u8)
                }
            }
            0x0400..=0x047F => {
                if addr & 0x20 != 0 {
                    self.maria.read((addr & 0x1F) as u8)
                } else {
                    self.tia_audio.read((addr & 0x1F) as u8)
                }
            }
            0x0480..=0x04FF => self.riot.read(addr),
            0x0500..=0x17FF => 0xFF,
            0x1800..=0x3FFF => self.ram_main[((addr - 0x1800) & 0x0FFF) as usize],
            0x4000..=0xFFFF => self.cart.read(addr),
        }
    }

    fn mem_write(&mut self, addr: u16, value: u8) {
        if let (Some(pokey), Some(location)) = (&mut self.pokey, self.pokey_location) {
            let base = location.base();
            if (base..base + 16).contains(&addr) {
                pokey.write((addr - base) as u8, value);
                return;
            }
        }
        match addr {
            0x0000..=0x001F => self.tia_audio.write(addr as u8, value),
            0x0020..=0x003F => self.maria.write(addr as u8 - 0x20, value),
            0x0040..=0x00FF => self.ram_zp[(addr - 0x40) as usize] = value,
            0x0100..=0x011F => self.tia_audio.write((addr & 0x1F) as u8, value),
            0x0120..=0x013F => self.maria.write((addr & 0x1F) as u8, value),
            0x0140..=0x01FF => self.ram_stack[(addr - 0x140) as usize] = value,
            0x0200..=0x027F => {
                if addr & 0x20 != 0 {
                    self.maria.write((addr & 0x1F) as u8, value);
                } else {
                    self.tia_audio.write((addr & 0x1F) as u8, value);
                }
            }
            0x0280..=0x02FF => self.riot.write(addr, value),
            0x0300..=0x03FF => {
                if addr & 0x80 != 0 {
                    self.riot.write(addr, value);
                } else if addr & 0x20 != 0 {
                    self.maria.write((addr & 0x1F) as u8, value);
                } else {
                    self.tia_audio.write((addr & 0x1F) as u8, value);
                }
            }
            0x0400..=0x047F => {
                if addr & 0x20 != 0 {
                    self.maria.write((addr & 0x1F) as u8, value);
                } else {
                    self.tia_audio.write((addr & 0x1F) as u8, value);
                }
            }
            0x0480..=0x04FF => self.riot.write(addr, value),
            0x0500..=0x17FF => {}
            0x1800..=0x3FFF => self.ram_main[((addr - 0x1800) & 0x0FFF) as usize] = value,
            0x4000..=0xFFFF => self.cart.write(addr, value),
        }
    }

    #[must_use]
    pub fn framebuffer(&self) -> &[u32] {
        self.maria.framebuffer()
    }

    #[must_use]
    pub fn framebuffer_width(&self) -> u32 {
        self.maria.framebuffer_width()
    }

    #[must_use]
    pub fn framebuffer_height(&self) -> u32 {
        self.maria.framebuffer_height()
    }

    /// Drain the TIA's mono audio samples produced since the previous call.
    pub fn take_audio_samples(&mut self) -> Vec<f32> {
        self.tia_audio.take_samples()
    }

    /// Drain audio from an optional cartridge POKEY at its native 48 kHz.
    pub fn take_pokey_audio_samples(&mut self) -> Vec<f32> {
        self.pokey
            .as_mut()
            .map_or_else(Vec::new, Pokey::take_buffer)
    }

    /// Native audio rate: two TIA samples per scanline at nominal refresh.
    #[must_use]
    pub fn audio_sample_rate(&self) -> u32 {
        match self.region {
            Atari7800Region::Ntsc => u32::from(self.region.lines_per_frame()) * 2 * 60,
            Atari7800Region::Pal => u32::from(self.region.lines_per_frame()) * 2 * 50,
        }
    }

    /// Set P0 joystick direction. Active-low on RIOT port A bits 4-7.
    #[allow(clippy::fn_params_excessive_bools)]
    pub fn set_joystick(&mut self, up: bool, down: bool, left: bool, right: bool) {
        let mut val = self.riot.input_a | 0xF0;
        if up {
            val &= !0x10;
        }
        if down {
            val &= !0x20;
        }
        if left {
            val &= !0x40;
        }
        if right {
            val &= !0x80;
        }
        self.riot.input_a = val;
    }

    /// Set console switch state (active-low on RIOT port B).
    /// Bit 0 = Reset, bit 1 = Select, bit 3 = Pause.
    pub fn set_console(&mut self, reset: bool, select: bool, pause: bool) {
        let mut val = 0xFFu8;
        if reset {
            val &= !0x01;
        }
        if select {
            val &= !0x02;
        }
        if pause {
            val &= !0x08;
        }
        self.riot.input_b = val;
    }

    /// Set player 1's primary fire button (proline button 1), read through the
    /// TIA's `INPT1`/`INPT4` registers.
    pub fn set_fire(&mut self, pressed: bool) {
        self.tia_audio.set_button(1, 1, pressed);
    }

    /// Set player 1's second fire button (proline button 2), read through the
    /// TIA's `INPT0`/`INPT4` registers.
    pub fn set_fire2(&mut self, pressed: bool) {
        self.tia_audio.set_button(1, 2, pressed);
    }

    #[must_use]
    pub fn cpu(&self) -> &M6502 {
        &self.cpu
    }
    pub fn cpu_mut(&mut self) -> &mut M6502 {
        &mut self.cpu
    }
    #[must_use]
    pub fn maria(&self) -> &Maria {
        &self.maria
    }
    #[must_use]
    pub fn region(&self) -> Atari7800Region {
        self.region
    }
    /// Elapsed native oscillator periods since cold boot.
    #[must_use]
    pub fn master_clock(&self) -> u64 {
        self.master_clock
    }
    #[must_use]
    pub fn frame_count(&self) -> u64 {
        self.frame_count
    }
}

impl Atari7800 {
    /// Read one byte with no side effects: zero-page / stack / main RAM
    /// and cartridge ROM; `$FF` for TIA / MARIA / RIOT (read side effects).
    #[must_use]
    pub fn peek(&self, addr: u16) -> u8 {
        match addr {
            0x0040..=0x00FF => self.ram_zp[(addr - 0x40) as usize],
            0x0140..=0x01FF => self.ram_stack[(addr - 0x140) as usize],
            0x1800..=0x3FFF => self.ram_main[((addr - 0x1800) & 0x0FFF) as usize],
            0x4000..=0xFFFF => self.cart.read(addr),
            _ => 0xFF,
        }
    }

    /// Write one byte through the bus (RAM accepts it; ROM ignores it).
    pub fn poke(&mut self, addr: u16, value: u8) {
        self.mem_write(addr, value);
    }

    /// Run exactly one whole 6502C instruction, returning the native oscillator periods
    /// it consumed. A safety cap prevents an unbounded spin.
    pub fn step_instruction(&mut self) -> u64 {
        let mut ticks = 0u64;
        while self.cpu.instruction_complete() && ticks < 16_384 {
            self.tick_master_clock();
            ticks += 1;
        }
        while !self.cpu.instruction_complete() && ticks < 16_384 {
            self.tick_master_clock();
            ticks += 1;
        }
        ticks
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn trap_rom_32k() -> Vec<u8> {
        let mut rom = vec![0xEAu8; 32768];
        rom[0x0000] = 0x4C;
        rom[0x0001] = 0x00;
        rom[0x0002] = 0x80;
        rom[0x7FFA] = 0x00;
        rom[0x7FFB] = 0x80;
        rom[0x7FFC] = 0x00;
        rom[0x7FFD] = 0x80;
        rom[0x7FFE] = 0x00;
        rom[0x7FFF] = 0x80;
        rom
    }

    fn a78_with_pokey_0440() -> Vec<u8> {
        let rom = trap_rom_32k();
        let mut image = vec![0; 128];
        image[0] = 4;
        image[1..10].copy_from_slice(b"ATARI7800");
        image[49..53].copy_from_slice(&(rom.len() as u32).to_be_bytes());
        image[67] = 1;
        image.extend_from_slice(&rom);
        image
    }

    #[test]
    fn cpu_wsync_colour_bars_extend_into_live_maria_borders() {
        let mut rom = trap_rom_32k();
        // BC on, DMA off. Alternate BACKGRND once per WSYNC from real CPU writes.
        rom[..16].copy_from_slice(&[
            0x78, 0xa9, 0x08, 0x85, 0x3c, 0xa9, 0x4e, 0x85, 0x20, 0x85, 0x24, 0x49, 0xc0, 0x4c,
            0x07, 0x80,
        ]);
        for region in [Atari7800Region::Ntsc, Atari7800Region::Pal] {
            let mut sys = Atari7800::new(rom.clone(), region).expect("colour-bar cartridge");
            sys.run_frame();
            let maria_region = region.maria_region();
            let width = maria_region.framebuffer_width() as usize;
            let top = maria_region.border_top() as usize;
            let palette = match region {
                Atari7800Region::Ntsc => &atari_maria::NTSC_PALETTE,
                Atari7800Region::Pal => &atari_maria::PAL_PALETTE,
            };
            let mut previous = None;
            for y in top..top + atari_maria::ACTIVE_HEIGHT as usize {
                let row = &sys.maria.framebuffer()[y * width..(y + 1) * width];
                let active = row[width / 2];
                assert!([palette[0x4e >> 1], palette[0x8e >> 1]].contains(&active));
                assert_ne!(
                    previous,
                    Some(active),
                    "WSYNC must produce alternating lines"
                );
                assert!(
                    row.iter().all(|&pixel| pixel == active),
                    "border differs from background on row {y}"
                );
                previous = Some(active);
            }
        }
    }

    #[test]
    fn cpu_bus_writes_wait_for_phase_two() {
        let mut rom = trap_rom_32k();
        rom[..7].copy_from_slice(&[0xa9, 0x5a, 0x85, 0x40, 0x4c, 0x04, 0x80]);
        let mut sys = Atari7800::new(rom, Atari7800Region::Ntsc).expect("write guest");
        let mut saw_address = false;
        let mut saw_write = false;
        for _ in 0..256 {
            sys.tick_master_clock();
            if sys.cpu.addr == 0x0040 && !sys.cpu.rw {
                if sys.maria.phi1 {
                    saw_address = true;
                    assert_eq!(sys.peek(0x40), 0, "write happened in address phase");
                }
                if sys.maria.phi2 {
                    saw_write = true;
                    assert_eq!(sys.peek(0x40), 0x5a, "phase 2 commits the write");
                    break;
                }
            }
        }
        assert!(saw_address && saw_write, "both bus phases must execute");
    }

    #[test]
    fn cpu_bus_reads_sample_memory_in_phase_two() {
        let mut rom = trap_rom_32k();
        rom[..7].copy_from_slice(&[0xa5, 0x40, 0x85, 0x41, 0x4c, 0x04, 0x80]);
        let mut sys = Atari7800::new(rom, Atari7800Region::Ntsc).expect("read guest");
        sys.poke(0x40, 0x11);
        let mut changed = false;
        let mut stored = false;
        for _ in 0..256 {
            sys.tick_master_clock();
            if sys.maria.phi1 && sys.cpu.rw && sys.cpu.addr == 0x0040 {
                // An external change after address presentation must reach the
                // later data sample. Sampling immediately at phi1 loses it.
                sys.poke(0x40, 0x77);
                changed = true;
            }
            if sys.peek(0x41) != 0 {
                stored = true;
                assert_eq!(sys.peek(0x41), 0x77);
                break;
            }
        }
        assert!(
            changed && stored,
            "the complete read/store path must execute"
        );
    }

    #[test]
    fn cpu_bus_wsync_keeps_nmi_edge_detection_running() {
        let mut sys = Atari7800::new(trap_rom_32k(), Atari7800Region::Ntsc).expect("WSYNC fixture");
        sys.maria.write(0x04, 0);
        sys.cpu.nmi = true;
        let cycles = sys.cpu.total_cycles;
        for _ in 0..24 {
            sys.tick_master_clock();
        }
        assert_eq!(sys.cpu.total_cycles, cycles, "WSYNC holds execution");
        assert!(
            sys.cpu.nmi_prev(),
            "NMI edge must still reach the CPU detector"
        );
        sys.cpu.nmi = false;
        for _ in 0..24 {
            sys.tick_master_clock();
        }
        assert!(!sys.cpu.nmi_prev(), "detector also observes pulse release");
    }

    #[test]
    fn cpu_bus_wsync_allows_both_read_modify_write_cycles() {
        let mut rom = trap_rom_32k();
        rom[..5].copy_from_slice(&[0xe6, 0x24, 0x4c, 0x02, 0x80]); // INC WSYNC
        let mut sys = Atari7800::new(rom, Atari7800Region::Ntsc).expect("RMW guest");
        let mut writes = Vec::new();
        for tick in 1..=2048 {
            sys.tick_master_clock();
            if sys.maria.phi2
                && sys.cpu.addr == 0x0024
                && !sys.cpu.rw
                && writes.last().is_none_or(|&(_, data)| data != sys.cpu.data)
            {
                writes.push((tick, sys.cpu.data));
                if writes.len() == 2 {
                    break;
                }
            }
        }
        assert_eq!(writes.len(), 2, "dummy and final writes must both execute");
        assert_eq!((writes[0].1, writes[1].1), (0, 1));
        assert_eq!(
            writes[1].0 - writes[0].0,
            8,
            "RDY cannot stall NMOS write cycles"
        );
    }

    #[test]
    fn cpu_bus_pending_writes_resume_at_the_same_memory_phase() {
        for region in [Atari7800Region::Ntsc, Atari7800Region::Pal] {
            for address in [0x0040u16, 0x0280] {
                for offset in 0..4 {
                    let mut rom = trap_rom_32k();
                    let [low, high] = address.to_le_bytes();
                    rom[..8].copy_from_slice(&[0xa9, 0x5a, 0x8d, low, high, 0x4c, 0x05, 0x80]);
                    let mut sys = Atari7800::new(rom, region).expect("write snapshot guest");
                    sys.poke(0x0281, 0xff);
                    sys.poke(0x0280, 0);
                    let mut presented = false;
                    for _ in 0..256 {
                        sys.tick_master_clock();
                        if sys.maria.phi1 && sys.cpu.addr == address && !sys.cpu.rw {
                            presented = true;
                            break;
                        }
                    }
                    assert!(presented);
                    for _ in 0..offset {
                        sys.tick_master_clock();
                    }
                    let saved = postcard::to_allocvec(&sys).expect("save pending write");
                    let mut restored: Atari7800 =
                        postcard::from_bytes(&saved).expect("restore pending write");
                    for tick in 1..=12 {
                        sys.tick_master_clock();
                        restored.tick_master_clock();
                        let observed = if address == 0x0040 {
                            sys.peek(address)
                        } else {
                            sys.riot.port_a_drive()
                        };
                        let resumed = if address == 0x0040 {
                            restored.peek(address)
                        } else {
                            restored.riot.port_a_drive()
                        };
                        assert_eq!(
                            observed,
                            if offset + tick < 4 { 0 } else { 0x5a },
                            "{region:?}, address {address:04x}, offset {offset}, tick {tick}"
                        );
                        assert_eq!(resumed, observed);
                    }
                    assert_eq!(
                        postcard::to_allocvec(&restored).expect("resumed state"),
                        postcard::to_allocvec(&sys).expect("original state")
                    );
                }
            }
        }
    }

    #[test]
    fn cpu_bus_pending_reads_resume_with_live_memory_and_peripheral_inputs() {
        for region in [Atari7800Region::Ntsc, Atari7800Region::Pal] {
            for address in [0x0040u16, 0x0280] {
                for offset in 0..4 {
                    let mut rom = trap_rom_32k();
                    let [low, high] = address.to_le_bytes();
                    rom[..8].copy_from_slice(&[0xad, low, high, 0x85, 0x41, 0x4c, 0x05, 0x80]);
                    let mut sys = Atari7800::new(rom, region).expect("read snapshot guest");
                    sys.poke(0x40, 0x11);
                    sys.riot.input_a = 0x11;
                    let mut presented = false;
                    for _ in 0..256 {
                        sys.tick_master_clock();
                        if sys.maria.phi1 && sys.cpu.addr == address && sys.cpu.rw {
                            presented = true;
                            break;
                        }
                    }
                    assert!(presented);
                    for _ in 0..offset {
                        sys.tick_master_clock();
                    }
                    let saved = postcard::to_allocvec(&sys).expect("save pending read");
                    let mut restored: Atari7800 =
                        postcard::from_bytes(&saved).expect("restore pending read");
                    for machine in [&mut sys, &mut restored] {
                        machine.poke(0x40, 0x77);
                        machine.riot.input_a = 0x77;
                    }
                    let mut stored = false;
                    for _ in 0..128 {
                        sys.tick_master_clock();
                        restored.tick_master_clock();
                        assert_eq!(restored.cpu.data_in, sys.cpu.data_in);
                        assert_eq!(restored.peek(0x41), sys.peek(0x41));
                        if sys.peek(0x41) != 0 {
                            stored = true;
                            assert_eq!(sys.peek(0x41), 0x77);
                            break;
                        }
                    }
                    assert!(stored, "the restored read must reach its store");
                }
            }
        }
    }

    #[test]
    fn slow_peripheral_reads_extend_the_cpu_clock_and_following_phase() {
        for region in [Atari7800Region::Ntsc, Atari7800Region::Pal] {
            for (address, expected) in [
                (0x0000u16, 60),
                (0x0280, 60),
                (0x0020, 56),
                (0x0040, 56),
                (0x9000, 56),
            ] {
                let mut rom = trap_rom_32k();
                let [low, high] = address.to_le_bytes();
                // LDA absolute (4 cycles), JMP $8000 (3). One slow access
                // extends phase 2 and the following phase 1 by two ticks each.
                rom[..6].copy_from_slice(&[0xad, low, high, 0x4c, 0x00, 0x80]);
                let mut sys = Atari7800::new(rom, region).expect("clock guest");
                for _ in 0..8 {
                    sys.step_instruction();
                }
                for _ in 0..16 {
                    let before = sys.cpu.total_cycles;
                    let ticks = sys.step_instruction() + sys.step_instruction();
                    assert_eq!(sys.cpu.total_cycles - before, 7);
                    assert_eq!(ticks, expected, "{region:?} address {address:04x}");
                }
            }
        }
    }

    #[test]
    fn riot_follows_phase_two_while_cpu_execution_is_held() {
        for region in [Atari7800Region::Ntsc, Atari7800Region::Pal] {
            for (address, period) in [(0x8000, 8u16), (0x0280, 12), (0x0000, 12)] {
                let mut sys = Atari7800::new(trap_rom_32k(), region).expect("timer fixture");
                sys.cpu.addr = address;
                sys.maria.write(0x04, 0); // WSYNC drives the machine's RDY input.
                sys.riot.write(0x0294, 200);
                for tick in 1..=120 {
                    sys.tick_master_clock();
                    // First phase 2 is at native tick 4; subsequent periods
                    // follow the independently measured fast/slow clock.
                    let elapsed = if tick < 4 { 0 } else { 1 + (tick - 4) / period };
                    assert_eq!(
                        sys.riot.timer_value(),
                        200 - elapsed as u8,
                        "{region:?}, address {address:04x}, tick {tick}"
                    );
                }
                assert_eq!(sys.cpu.total_cycles, 0, "RDY holds execution, not RIOT");
            }
        }
    }

    #[test]
    fn restored_slow_accesses_preserve_clock_bus_timer_and_audio() {
        let mut rom = trap_rom_32k();
        rom[..6].copy_from_slice(&[0xad, 0x80, 0x02, 0x4c, 0x00, 0x80]);
        for region in [Atari7800Region::Ntsc, Atari7800Region::Pal] {
            // The measured loop is 60 native ticks. Visit every point in it,
            // including the divider-selection and held-access boundaries.
            for phase in 0..60 {
                let mut sys = Atari7800::new(rom.clone(), region).expect("slow guest");
                sys.poke(0x15, 0x04);
                sys.poke(0x19, 0x0f);
                sys.poke(0x0294, 0xc8);
                for _ in 0..1024 + phase {
                    sys.tick_master_clock();
                }
                sys.take_audio_samples();
                let saved = postcard::to_allocvec(&sys).expect("save slow phase");
                let mut restored: Atari7800 =
                    postcard::from_bytes(&saved).expect("restore slow phase");
                let before = sys.cpu.total_cycles;
                for _ in 0..600 {
                    sys.tick_master_clock();
                    restored.tick_master_clock();
                    assert_eq!(
                        (
                            restored.cpu.addr,
                            restored.cpu.data_in,
                            restored.cpu.rw,
                            restored.cpu.total_cycles
                        ),
                        (
                            sys.cpu.addr,
                            sys.cpu.data_in,
                            sys.cpu.rw,
                            sys.cpu.total_cycles
                        )
                    );
                    assert_eq!(
                        (restored.maria.phi1, restored.maria.phi2),
                        (sys.maria.phi1, sys.maria.phi2)
                    );
                    assert_eq!(restored.riot.timer_value(), sys.riot.timer_value());
                    assert_eq!(restored.take_audio_samples(), sys.take_audio_samples());
                }
                assert_eq!(sys.cpu.total_cycles - before, 70, "ten complete loops");
                assert_eq!(
                    postcard::to_allocvec(&restored).expect("restored continuation"),
                    postcard::to_allocvec(&sys).expect("original continuation")
                );
            }
        }
    }

    #[test]
    fn instruction_ticks_are_native_oscillator_periods() {
        for region in [Atari7800Region::Ntsc, Atari7800Region::Pal] {
            let mut sys = Atari7800::new(trap_rom_32k(), region).expect("init");
            sys.step_instruction(); // Finish reset before measuring a complete instruction.
            for _ in 0..32 {
                let before = sys.cpu.total_cycles;
                let ticks = sys.step_instruction();
                let cycles = sys.cpu.total_cycles - before;
                assert!(cycles > 0);
                assert_eq!(ticks, cycles * 8, "{region:?}");
            }
        }
    }

    #[test]
    fn native_clock_preserves_tia_sample_stream() {
        for region in [Atari7800Region::Ntsc, Atari7800Region::Pal] {
            let mut sys = Atari7800::new(trap_rom_32k(), region).expect("init");
            let mut sound = TiaAudio::new();
            for (register, value) in [(0x15, 0x04), (0x17, 0x03), (0x19, 0x0f)] {
                sys.poke(u16::from(register), value);
                sound.write(register, value);
            }
            for colour_clock in 0..1024 {
                // Writes on non-aligned native phases must reach the next sound tick.
                for phase in 0..4 {
                    if phase == 1 && colour_clock % 113 == 0 {
                        let frequency = ((colour_clock / 113) & 0x1f) as u8;
                        sys.poke(0x17, frequency);
                        sound.write(0x17, frequency);
                    }
                    sys.tick_master_clock();
                    if phase == 3 {
                        sound.tick();
                    }
                    assert_eq!(sys.take_audio_samples(), sound.take_samples());
                }
            }
        }
    }

    #[test]
    fn restored_native_phases_preserve_bus_timer_and_audio() {
        for region in [Atari7800Region::Ntsc, Atari7800Region::Pal] {
            for phase in 0..8 {
                let mut sys = Atari7800::new(trap_rom_32k(), region).expect("init");
                sys.poke(0x15, 0x04);
                sys.poke(0x19, 0x0f);
                sys.poke(0x0294, 0x71); // RIOT divide-by-one timer.
                for _ in 0..(1024 + phase) {
                    sys.tick_master_clock();
                }
                // Output already delivered to the host is deliberately not saved.
                sys.take_audio_samples();
                let saved = postcard::to_allocvec(&sys).expect("save phase");
                let mut restored: Atari7800 = postcard::from_bytes(&saved).expect("restore phase");
                let cycles = sys.cpu.total_cycles;
                for _ in 0..2048 {
                    sys.tick_master_clock();
                    restored.tick_master_clock();
                    assert_eq!(
                        (
                            sys.cpu.addr,
                            sys.cpu.data,
                            sys.cpu.rw,
                            sys.cpu.nmi,
                            sys.cpu.total_cycles
                        ),
                        (
                            restored.cpu.addr,
                            restored.cpu.data,
                            restored.cpu.rw,
                            restored.cpu.nmi,
                            restored.cpu.total_cycles
                        ),
                        "{region:?} phase {phase}"
                    );
                    assert_eq!(sys.riot.timer_value(), restored.riot.timer_value());
                    assert_eq!(sys.take_audio_samples(), restored.take_audio_samples());
                }
                assert!(sys.cpu.total_cycles > cycles);
                assert_eq!(
                    postcard::to_allocvec(&sys).expect("continued state"),
                    postcard::to_allocvec(&restored).expect("restored state")
                );
            }
        }
    }

    #[test]
    fn frame_advances_master_clock_and_count() {
        let mut sys = Atari7800::new(trap_rom_32k(), Atari7800Region::Ntsc).expect("init");
        let clocks = sys.run_frame();
        assert!(clocks > 0);
        assert_eq!(sys.frame_count(), 1);
    }

    #[test]
    fn pal_runs_more_clocks_than_ntsc() {
        let mut ntsc = Atari7800::new(trap_rom_32k(), Atari7800Region::Ntsc).expect("init");
        let mut pal = Atari7800::new(trap_rom_32k(), Atari7800Region::Pal).expect("init");
        assert!(pal.run_frame() > ntsc.run_frame());
    }

    #[test]
    fn frame_produces_tia_audio_at_native_rate() {
        let mut sys = Atari7800::new(trap_rom_32k(), Atari7800Region::Ntsc).expect("init");
        sys.poke(0x0015, 0x04);
        sys.poke(0x0017, 0x00);
        sys.poke(0x0019, 0x0F);
        sys.run_frame();
        let samples = sys.take_audio_samples();
        assert_eq!(samples.len(), 263 * 2);
        assert!(samples.iter().any(|sample| *sample > 0.0));
        assert_eq!(sys.audio_sample_rate(), 31_560);
    }

    #[test]
    fn a78_pokey_feature_installs_and_routes_the_chip() {
        let mut sys =
            Atari7800::new(a78_with_pokey_0440(), Atari7800Region::Ntsc).expect("POKEY cartridge");
        assert!(sys.pokey.is_some());
        assert_eq!(sys.pokey_location, Some(PokeyLocation::Addr0440));
        sys.mem_write(0x0441, 0x1F);
        sys.run_frame();
        assert!(
            sys.take_pokey_audio_samples()
                .iter()
                .any(|sample| *sample != 0.0)
        );
    }

    #[test]
    fn memory_map_routes_ram_and_cart() {
        let mut sys = Atari7800::new(trap_rom_32k(), Atari7800Region::Ntsc).expect("init");
        sys.mem_write(0x0040, 0x55);
        assert_eq!(sys.mem_read(0x0040), 0x55);
        sys.mem_write(0x1800, 0x66);
        assert_eq!(sys.mem_read(0x1800), 0x66);
        assert_eq!(sys.mem_read(0x2800), 0x66);
        assert_eq!(sys.mem_read(0x8000), 0x4C);
    }

    #[test]
    fn maria_register_route() {
        let mut sys = Atari7800::new(trap_rom_32k(), Atari7800Region::Ntsc).expect("init");
        sys.mem_write(0x0020, 0x94);
        // BACKGRND is write-only — read returns 0 from MSTAT mirror at this addr.
        // Just verify no panic.
        let _ = sys.mem_read(0x0020);
    }

    #[test]
    fn joystick_drives_riot_port_a() {
        let mut sys = Atari7800::new(trap_rom_32k(), Atari7800Region::Ntsc).expect("init");
        sys.set_joystick(true, false, false, false);
        assert_eq!(sys.riot.input_a & 0x10, 0);
        sys.set_joystick(false, false, false, false);
        assert_eq!(sys.riot.input_a & 0xF0, 0xF0);
    }

    #[test]
    fn console_switches_drive_riot_port_b() {
        let mut sys = Atari7800::new(trap_rom_32k(), Atari7800Region::Ntsc).expect("init");
        sys.set_console(true, false, false);
        assert_eq!(sys.riot.input_b & 0x01, 0);
        sys.set_console(false, true, false);
        assert_eq!(sys.riot.input_b & 0x02, 0);
        sys.set_console(false, false, true);
        assert_eq!(sys.riot.input_b & 0x08, 0);
    }

    #[test]
    fn rejects_oversized_rom() {
        assert!(Atari7800::new(vec![0u8; 256_000], Atari7800Region::Ntsc).is_err());
    }

    /// Save-state must capture LIVE machine state (6502C + MARIA + RIOT + TIA
    /// audio + cart), not cold-boot from the ROM. Serialise, advance (so the
    /// state differs), then deserialise the first snapshot and confirm
    /// re-serialising it is byte-identical — every stateful field across the
    /// CPU, MARIA, RIOT, TIA, and cartridge round-trips, and a poked RAM byte
    /// survives.
    #[test]
    fn snapshot_round_trips_live_state() {
        let mut sys = Atari7800::new(trap_rom_32k(), Atari7800Region::Ntsc).expect("init");
        sys.run_frame();
        sys.poke(0x0040, 0xA5); // a zero-page RAM byte to carry across the snapshot
        sys.run_frame();
        let s1 = postcard::to_allocvec(&sys).expect("encode snapshot");

        sys.run_frame(); // advance past the snapshot point
        let s2 = postcard::to_allocvec(&sys).expect("encode again");
        assert_ne!(s1, s2, "running a frame should change the serialised state");

        let restored: Atari7800 = postcard::from_bytes(&s1).expect("decode snapshot");
        assert_eq!(
            restored.peek(0x0040),
            0xA5,
            "poked RAM byte survives the round-trip"
        );
        let s3 = postcard::to_allocvec(&restored).expect("re-encode restored");
        assert_eq!(
            s1, s3,
            "restore should reproduce the snapshot state exactly"
        );
    }
}
