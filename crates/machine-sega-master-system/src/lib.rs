//! Sega Master System / Game Gear machine wiring.
//!
//! Fresh-write against the workspace pin-driven bus pattern (RULES.md
//! rule 6). The donor at `Emu198x-Oldest/crates/machine-sega-master-system`
//! used the deprecated `emu_core::Bus` callback and could not port
//! directly; this file uses it as a system spec — Sega mapper at
//! `$FFFC-$FFFF`, I/O port routing, controller layout, Game Gear
//! extensions, clock model — but the wiring is written against
//! [`zilog_z80::Z80`]'s public pin fields and `bus_request()` collapse.
//!
//! # The Sega Master System
//!
//! The Master System (Mark III in Japan, 1985) is Sega's 8-bit console,
//! big in Europe and Brazil. Same Z80A + SN76489 audio + new
//! [Sega VDP](sega_vdp::SegaVdp) (315-5124/5246). **No BIOS required**
//! for most carts — execution starts at `$0000` in the cart. SMS-1
//! BIOSes exist (for the title screen and `Hang On / Safari Hunt`
//! built-ins) but are optional.
//!
//! - **CPU:** Z80A @ 3.579545 MHz
//! - **VDP:** Sega VDP 315-5124 (SMS) / 315-5246 (Game Gear)
//! - **PSG:** SN76489A (mono on SMS, stereo on Game Gear via `$06`)
//! - **RAM:** 8 KB at `$C000-$DFFF`, mirrored at `$E000-$FFFF`
//! - **Cartridge:** up to 4 MB via the Sega mapper
//!
//! # Memory map
//!
//! | Range         | Contents                                       |
//! |---------------|------------------------------------------------|
//! | `$0000-$03FF` | Cart page 0, first 1 KB always visible         |
//! | `$0400-$3FFF` | Cart page 0 (rest), banked via `$FFFD`         |
//! | `$4000-$7FFF` | Cart page 1, banked via `$FFFE`                |
//! | `$8000-$BFFF` | Cart page 2, banked via `$FFFF` (or cart RAM)  |
//! | `$C000-$DFFF` | 8 KB system RAM                                |
//! | `$E000-$FFFF` | RAM mirror; `$FFFC-$FFFF` shadow mapper regs   |
//!
//! # Sega mapper
//!
//! Writes to `$FFFC-$FFFF` in the RAM mirror window also update the
//! mapper registers:
//!
//! - `$FFFC` — control register (bit 3 = cart RAM enable for page 2)
//! - `$FFFD` — bank for slot 0 (`$0000-$3FFF` above the fixed 1 KB)
//! - `$FFFE` — bank for slot 1 (`$4000-$7FFF`)
//! - `$FFFF` — bank for slot 2 (`$8000-$BFFF`)
//!
//! Default banks `[0, 1, 2]` are set at construction so a fresh cart
//! reads sequentially across the three slots before the cart's own
//! init programs the mapper.
//!
//! # I/O map
//!
//! | Port           | R/W   | Function                              |
//! |----------------|-------|---------------------------------------|
//! | `$00`          | read  | Game Gear START button (GG only)      |
//! | `$06`          | write | Game Gear PSG stereo (GG only)        |
//! | `$40-$7F` even | r/w   | V-counter read / PSG write            |
//! | `$41-$7F` odd  | r/w   | H-counter read / PSG write            |
//! | `$80-$BF` even | r/w   | VDP data                              |
//! | `$80-$BF` odd  | r/w   | VDP control / status                  |
//! | `$C0-$FF` even | read  | Controller port 1                     |
//! | `$C0-$FF` odd  | read  | Controller port 2 / misc              |
//!
//! # Pause button → NMI
//!
//! Pressing the SMS front-panel Pause button drives the Z80 NMI line.
//! Exposed via [`Sms::set_pause_pressed`].
//!
//! # Clock model
//!
//! Adopts SG-1000 / MSX 3:2 VDP-dot-per-T-state phase counter. The
//! `sega-vdp` crate exposes only `tick_scanline()` (not per-dot tick)
//! so the machine accumulates 342 dots' worth of phase before issuing
//! one scanline tick. This is even more accuracy-relaxed than
//! `ti-tms9918`'s per-dot tick — refining the VDP to a per-dot model
//! is the obvious next accuracy step, tracked under
//! `docs/status/outstanding-work.md` § Sega Master System.

use sega_vdp::{SegaVdp, VdpRegion, VdpVariant};
use ti_sn76489::Sn76489;
use zilog_z80::{BusOp, Z80};

const CPU_TSTATES_PER_SCANLINE: u64 = 228;
const NTSC_SCANLINES_PER_FRAME: u64 = 262;
const PAL_SCANLINES_PER_FRAME: u64 = 313;
const NTSC_TSTATES_PER_FRAME: u64 = CPU_TSTATES_PER_SCANLINE * NTSC_SCANLINES_PER_FRAME;
const PAL_TSTATES_PER_FRAME: u64 = CPU_TSTATES_PER_SCANLINE * PAL_SCANLINES_PER_FRAME;

const NTSC_PSG_CLOCK_HZ: u32 = 3_579_545;
const PAL_PSG_CLOCK_HZ: u32 = 3_546_893;

/// SMS / Game Gear system variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SmsVariant {
    /// Sega Master System (NTSC — Japan / US / Brazil).
    SmsNtsc,
    /// Sega Master System (PAL — Europe).
    SmsPal,
    /// Sega Game Gear (NTSC).
    GameGear,
}

impl SmsVariant {
    fn psg_clock_hz(self) -> u32 {
        match self {
            Self::SmsPal => PAL_PSG_CLOCK_HZ,
            _ => NTSC_PSG_CLOCK_HZ,
        }
    }

    fn tstates_per_frame(self) -> u64 {
        match self {
            Self::SmsPal => PAL_TSTATES_PER_FRAME,
            _ => NTSC_TSTATES_PER_FRAME,
        }
    }

    fn is_game_gear(self) -> bool {
        matches!(self, Self::GameGear)
    }
}

/// Sega Master System / Game Gear machine.
pub struct Sms {
    cpu: Z80,
    vdp: SegaVdp,
    psg: Sn76489,
    cart_rom: Vec<u8>,
    ram: [u8; 8192],
    /// Sega mapper bank registers, shadowed from RAM writes at
    /// `$FFFC-$FFFF`.
    mapper_regs: [u8; 4],
    /// Controller port 1 active-low byte. Standard layout:
    /// bit 0 = up, 1 = down, 2 = left, 3 = right,
    /// bit 4 = button 1, 5 = button 2.
    port_dc: u8,
    /// Controller port 2 / misc port active-low byte.
    port_dd: u8,
    /// Game Gear START button (active-low bit 7 of port `$00`).
    gg_start: u8,
    /// Pause line; held by the host until released.
    pause_pressed: bool,
    variant: SmsVariant,
    cpu_tstates: u64,
    tstates_per_frame: u64,
    /// T-states accumulated within the current scanline.
    scanline_tstates: u64,
    frame_count: u64,
}

impl Sms {
    /// Create a new SMS / Game Gear with the given cart ROM.
    #[must_use]
    pub fn new(cart_rom: Vec<u8>, variant: SmsVariant) -> Self {
        let vdp = if variant.is_game_gear() {
            SegaVdp::new_game_gear()
        } else {
            let region = match variant {
                SmsVariant::SmsPal => VdpRegion::Pal,
                _ => VdpRegion::Ntsc,
            };
            SegaVdp::new(region, VdpVariant::Sms2)
        };
        Self {
            cpu: Z80::new(),
            vdp,
            psg: Sn76489::new(variant.psg_clock_hz()),
            cart_rom,
            ram: [0; 8192],
            mapper_regs: [0x00, 0x00, 0x01, 0x02],
            port_dc: 0xFF,
            port_dd: 0xFF,
            gg_start: 0xFF,
            pause_pressed: false,
            variant,
            cpu_tstates: 0,
            tstates_per_frame: variant.tstates_per_frame(),
            scanline_tstates: 0,
            frame_count: 0,
        }
    }

    /// Run one frame and return T-states consumed.
    pub fn run_frame(&mut self) -> u64 {
        let target = self.cpu_tstates + self.tstates_per_frame;
        while self.cpu_tstates < target {
            self.tick_tstate();
        }
        self.frame_count += 1;
        self.tstates_per_frame
    }

    fn tick_tstate(&mut self) {
        self.cpu.tick();
        self.handle_bus();

        // sega-vdp only exposes tick_scanline (per-scanline render),
        // so accumulate T-states and trigger one scanline at the
        // 228-T-state boundary.
        self.scanline_tstates += 1;
        if self.scanline_tstates >= CPU_TSTATES_PER_SCANLINE {
            self.scanline_tstates = 0;
            self.vdp.tick_scanline();
        }

        // PSG ticks at the Z80 clock on SMS.
        self.psg.tick();

        // VDP INT (VBlank + line interrupt) → Z80 IRQ.
        self.cpu.irq = self.vdp.interrupt;

        // Pause → NMI (level-driven; the host releases).
        self.cpu.nmi = self.pause_pressed;

        self.cpu_tstates += 1;
    }

    fn handle_bus(&mut self) {
        match self.cpu.bus_request() {
            Some(BusOp::MemRead) => {
                self.cpu.data_in = self.mem_read(self.cpu.addr);
            }
            Some(BusOp::MemWrite) => {
                self.mem_write(self.cpu.addr, self.cpu.data);
            }
            Some(BusOp::IoRead) => {
                self.cpu.data_in = self.io_read(self.cpu.addr);
            }
            Some(BusOp::IoWrite) => {
                self.io_write(self.cpu.addr, self.cpu.data);
            }
            Some(BusOp::IntAck) => {
                // SMS carts set IM 1 — INT fetches RST 38h via the
                // floating bus.
                self.cpu.data_in = 0xFF;
            }
            None => {}
        }
    }

    fn read_rom(&self, bank: u8, offset: usize) -> u8 {
        // Sega mapper masks the bank register against the number of
        // 16 KB banks in the cart (assumed power-of-two). For a 128 KB
        // cart (8 banks), bank $82 reads as bank 2.
        let cart_banks = self.cart_rom.len() / 0x4000;
        if cart_banks == 0 {
            return 0xFF;
        }
        let mask = cart_banks.next_power_of_two().saturating_sub(1);
        let bank = (bank as usize) & mask;
        let addr = bank * 0x4000 + offset;
        self.cart_rom.get(addr).copied().unwrap_or(0xFF)
    }

    fn mem_read(&self, addr: u16) -> u8 {
        match addr {
            // First 1 KB is always cart page 0 — interrupt vectors.
            0x0000..=0x03FF => self.cart_rom.get(addr as usize).copied().unwrap_or(0xFF),
            // Rest of slot 0.
            0x0400..=0x3FFF => self.read_rom(self.mapper_regs[1], (addr & 0x3FFF) as usize),
            // Slot 1.
            0x4000..=0x7FFF => self.read_rom(self.mapper_regs[2], (addr & 0x3FFF) as usize),
            // Slot 2 (or cart RAM when control bit 3 is set; cart RAM
            // not yet modelled — returns $FF).
            0x8000..=0xBFFF => {
                if self.mapper_regs[0] & 0x08 != 0 {
                    0xFF
                } else {
                    self.read_rom(self.mapper_regs[3], (addr & 0x3FFF) as usize)
                }
            }
            // 8 KB RAM mirrored across $C000-$FFFF.
            0xC000..=0xFFFF => self.ram[(addr & 0x1FFF) as usize],
        }
    }

    fn mem_write(&mut self, addr: u16, value: u8) {
        if (0xC000..=0xFFFF).contains(&addr) {
            self.ram[(addr & 0x1FFF) as usize] = value;
            match addr {
                0xFFFC => self.mapper_regs[0] = value,
                0xFFFD => self.mapper_regs[1] = value,
                0xFFFE => self.mapper_regs[2] = value,
                0xFFFF => self.mapper_regs[3] = value,
                _ => {}
            }
        }
    }

    fn io_read(&mut self, port: u16) -> u8 {
        let p = port as u8;
        // Game Gear-specific I/O at the low end.
        if self.variant.is_game_gear() && p == 0x00 {
            return self.gg_start;
        }
        match p {
            0x40..=0x7F if p & 1 == 0 => self.vdp.read_v_counter(),
            0x40..=0x7F => self.vdp.read_h_counter(),
            0x80..=0xBF if p & 1 == 0 => self.vdp.read_data(),
            0x80..=0xBF => self.vdp.read_status(),
            0xC0..=0xFF if p & 1 == 0 => self.port_dc,
            0xC0..=0xFF => self.port_dd,
            _ => 0xFF,
        }
    }

    fn io_write(&mut self, port: u16, value: u8) {
        let p = port as u8;
        if self.variant.is_game_gear() && p == 0x06 {
            self.psg.write_stereo(value);
            return;
        }
        match p {
            0x40..=0x7F => self.psg.write(value),
            0x80..=0xBF if p & 1 == 0 => self.vdp.write_data(value),
            0x80..=0xBF => self.vdp.write_control(value),
            _ => {}
        }
    }

    /// Framebuffer (ARGB32) — VDP active display plus canonical
    /// TV-visible border.
    #[must_use]
    pub fn framebuffer(&self) -> &[u32] {
        self.vdp.framebuffer()
    }

    #[must_use]
    pub fn framebuffer_width(&self) -> u32 {
        self.vdp.framebuffer_width()
    }

    #[must_use]
    pub fn framebuffer_height(&self) -> u32 {
        self.vdp.framebuffer_height()
    }

    /// Take the accumulated mono PSG audio buffer.
    pub fn take_audio_buffer(&mut self) -> Vec<f32> {
        self.psg.take_buffer()
    }

    /// Set controller port 1 (active-low byte).
    pub fn set_port_dc(&mut self, value: u8) {
        self.port_dc = value;
    }

    /// Set controller port 2 (active-low byte).
    pub fn set_port_dd(&mut self, value: u8) {
        self.port_dd = value;
    }

    /// Set the Game Gear START button (active-low bit 7 of `$00`).
    pub fn set_gg_start(&mut self, value: u8) {
        self.gg_start = value;
    }

    /// Pause / NMI line (level-driven; host-controlled).
    pub fn set_pause_pressed(&mut self, pressed: bool) {
        self.pause_pressed = pressed;
    }

    /// CPU reference.
    #[must_use]
    pub fn cpu(&self) -> &Z80 {
        &self.cpu
    }

    /// CPU mutable reference.
    pub fn cpu_mut(&mut self) -> &mut Z80 {
        &mut self.cpu
    }

    /// VDP reference.
    #[must_use]
    pub fn vdp(&self) -> &SegaVdp {
        &self.vdp
    }

    /// Variant.
    #[must_use]
    pub fn variant(&self) -> SmsVariant {
        self.variant
    }

    /// Sega mapper register values `[$FFFC, $FFFD, $FFFE, $FFFF]`.
    #[must_use]
    pub fn mapper_regs(&self) -> &[u8; 4] {
        &self.mapper_regs
    }

    /// CPU T-states executed since power-on.
    #[must_use]
    pub fn cpu_tstates(&self) -> u64 {
        self.cpu_tstates
    }

    /// Observe one byte on the Z80 bus without side effects.
    /// Resolves Sega mapper banks / RAM via the standard memory map.
    #[must_use]
    pub fn peek(&self, addr: u16) -> u8 {
        self.mem_read(addr)
    }

    /// Frame count since power-on.
    #[must_use]
    pub fn frame_count(&self) -> u64 {
        self.frame_count
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn trap_cart_64k() -> Vec<u8> {
        // 64 KB cart: NOPs + JR -2 trap at $0008. Enough to fill all
        // three mapper slots (16 KB each).
        let mut cart = vec![0u8; 0x10000];
        cart[0x0008] = 0x18;
        cart[0x0009] = 0xFE;
        cart
    }

    #[test]
    fn ntsc_frame_returns_expected_tstates() {
        let mut sys = Sms::new(trap_cart_64k(), SmsVariant::SmsNtsc);
        let t = sys.run_frame();
        assert_eq!(t, NTSC_TSTATES_PER_FRAME);
        assert_eq!(sys.frame_count(), 1);
    }

    #[test]
    fn pal_frame_returns_expected_tstates() {
        let mut sys = Sms::new(trap_cart_64k(), SmsVariant::SmsPal);
        let t = sys.run_frame();
        assert_eq!(t, PAL_TSTATES_PER_FRAME);
    }

    #[test]
    fn many_frames_complete_without_panic() {
        let mut sys = Sms::new(trap_cart_64k(), SmsVariant::SmsNtsc);
        for _ in 0..60 {
            sys.run_frame();
        }
        assert_eq!(sys.frame_count(), 60);
    }

    #[test]
    fn first_1k_always_reads_cart_bank_0() {
        let mut cart = trap_cart_64k();
        cart[0x0100] = 0x42;
        let sys = Sms::new(cart, SmsVariant::SmsNtsc);
        // $0100 is in the always-visible first 1 KB.
        assert_eq!(sys.mem_read(0x0100), 0x42);
    }

    #[test]
    fn mapper_fffd_swaps_slot_0_high() {
        let mut cart = vec![0u8; 0x10000];
        // Bank 0 at $1000 = $11; Bank 2 at $1000 = $22.
        cart[0x1000] = 0x11;
        cart[0x4000 + 0x1000] = 0xAA; // bank 1
        cart[0x8000 + 0x1000] = 0x22; // bank 2
        let mut sys = Sms::new(cart, SmsVariant::SmsNtsc);
        // Default mapper: bank 0 in slot 0 high.
        assert_eq!(sys.mem_read(0x1000), 0x11);
        // Write to $FFFD to swap bank 2 in.
        sys.mem_write(0xFFFD, 2);
        assert_eq!(sys.mapper_regs()[1], 2);
        assert_eq!(sys.mem_read(0x1000), 0x22);
    }

    #[test]
    fn mapper_fffe_swaps_slot_1() {
        let mut cart = vec![0u8; 0x10000];
        cart[0x4000] = 0xAA; // bank 1
        cart[0x8000] = 0xBB; // bank 2
        let mut sys = Sms::new(cart, SmsVariant::SmsNtsc);
        // Default: slot 1 = bank 1.
        assert_eq!(sys.mem_read(0x4000), 0xAA);
        sys.mem_write(0xFFFE, 2);
        assert_eq!(sys.mem_read(0x4000), 0xBB);
    }

    #[test]
    fn ram_round_trip_through_mirror() {
        let mut sys = Sms::new(trap_cart_64k(), SmsVariant::SmsNtsc);
        // Avoid $FFFC-$FFFF mapper window; pick somewhere safe.
        sys.mem_write(0xC100, 0x42);
        assert_eq!(sys.mem_read(0xC100), 0x42);
        // 8 KB mirror — same byte appears at $E100.
        assert_eq!(sys.mem_read(0xE100), 0x42);
    }

    #[test]
    fn pause_line_drives_z80_nmi() {
        let mut sys = Sms::new(trap_cart_64k(), SmsVariant::SmsNtsc);
        sys.set_pause_pressed(true);
        sys.tick_tstate();
        assert!(sys.cpu.nmi);
        sys.set_pause_pressed(false);
        sys.tick_tstate();
        assert!(!sys.cpu.nmi);
    }

    #[test]
    fn controller_routing_by_port_parity() {
        let mut sys = Sms::new(trap_cart_64k(), SmsVariant::SmsNtsc);
        sys.set_port_dc(0xAA);
        sys.set_port_dd(0x55);
        assert_eq!(sys.io_read(0xDC), 0xAA);
        assert_eq!(sys.io_read(0xDD), 0x55);
    }

    #[test]
    fn game_gear_start_button_at_port_00() {
        let mut sys = Sms::new(trap_cart_64k(), SmsVariant::GameGear);
        sys.set_gg_start(0x7F); // START pressed (bit 7 low).
        assert_eq!(sys.io_read(0x00), 0x7F);
    }
}
