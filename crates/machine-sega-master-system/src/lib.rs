//! Sega Master System / Game Gear machine wiring.
//!
//! Fresh-write against the workspace pin-driven bus pattern (RULES.md
//! rule 6). The donor at `Emu198x-Oldest/crates/machine-sega-master-system`
//! used the deprecated `emu_core::Bus` callback and could not port
//! directly; this file uses it as a system spec — Sega mapper at
//! `$FFFC-$FFFF`, I/O port routing, controller layout, Game Gear
//! extensions, clock model — but the wiring is written against
//! [`emu198x_zilog_z80::Z80`]'s public pin fields and `bus_request()` collapse.
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
//! - `$FFFC` — control register (bit 3 = cart RAM enable for page 2;
//!   bit 2 selects either 16 KB RAM bank)
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

mod cartridge;

pub use cartridge::{CartridgeHeader, CartridgeTerritory, normalize_cartridge};

use emu198x_zilog_z80::{BusOp, Z80};
use sega_vdp::{SegaVdp, VdpRegion, VdpVariant};
use serde::{Deserialize, Serialize};
use serde_big_array::BigArray;
use ti_sn76489::{NoiseLfsr, Sn76489};

const CPU_TSTATES_PER_SCANLINE: u64 = 228;
// VDP dot clock vs CPU clock: master 10.738635 MHz, CPU ÷3, VDP dot ÷2, so
// 3 VDP dots advance per 2 CPU T-states (342 dots / 228 T-states per line).
//
// Accumulated per CPU **half-cycle**, so the denominator is 4 rather than
// 2: three dots per four half-cycles is the same 3:2 against T-states,
// interleaved twice as finely.
const VDP_DOT_PHASE_NUMERATOR: u32 = 3;
const VDP_DOT_PHASE_DENOMINATOR: u32 = 4;
const NTSC_SCANLINES_PER_FRAME: u64 = 262;
const PAL_SCANLINES_PER_FRAME: u64 = 313;
const NTSC_TSTATES_PER_FRAME: u64 = CPU_TSTATES_PER_SCANLINE * NTSC_SCANLINES_PER_FRAME;
const PAL_TSTATES_PER_FRAME: u64 = CPU_TSTATES_PER_SCANLINE * PAL_SCANLINES_PER_FRAME;

const NTSC_PSG_CLOCK_HZ: u32 = 3_579_545;
const PAL_PSG_CLOCK_HZ: u32 = 3_546_893;

// Port $3E is active low. A no-BIOS machine starts with cartridge, work RAM,
// and controllers enabled; unused external/card slots and the absent BIOS are
// disabled. A BIOS machine starts in Sega's documented $E0 power-on map.
const MEMORY_CONTROL_NO_BIOS: u8 = 0xA8;
const MEMORY_CONTROL_WITH_BIOS: u8 = 0xE0;
const MEMORY_DISABLE_IO: u8 = 0x04;
const MEMORY_DISABLE_BIOS: u8 = 0x08;
const MEMORY_DISABLE_WORK_RAM: u8 = 0x10;
const MEMORY_DISABLE_CARTRIDGE: u8 = 0x40;

/// SMS / Game Gear system variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SmsVariant {
    /// Export Sega Master System (NTSC — US / Brazil), 315-5246 VDP.
    SmsNtsc,
    /// Japanese Sega Master System (NTSC), whose I/O chip reports
    /// output-configured TH pins differently from export machines.
    SmsJapanNtsc,
    /// Sega Master System (PAL — Europe), 315-5246 VDP.
    SmsPal,
    /// Early Sega Master System (NTSC) with the 315-5124 VDP.
    Sms1Ntsc,
    /// Early Sega Master System (PAL) with the 315-5124 VDP.
    Sms1Pal,
    /// Sega Game Gear (NTSC).
    GameGear,
}

impl SmsVariant {
    fn psg_clock_hz(self) -> u32 {
        match self {
            Self::SmsPal | Self::Sms1Pal => PAL_PSG_CLOCK_HZ,
            _ => NTSC_PSG_CLOCK_HZ,
        }
    }

    fn tstates_per_frame(self) -> u64 {
        match self {
            Self::SmsPal | Self::Sms1Pal => PAL_TSTATES_PER_FRAME,
            _ => NTSC_TSTATES_PER_FRAME,
        }
    }

    fn is_game_gear(self) -> bool {
        matches!(self, Self::GameGear)
    }

    fn is_japan(self) -> bool {
        matches!(self, Self::SmsJapanNtsc)
    }

    /// Which revision of the VDP this machine carries.
    ///
    /// The 315-5124 shipped in the early Master System and the 315-5246 in
    /// the Master System II and the Game Gear. The difference is a handful of
    /// register bits the earlier chip ANDs with the VRAM address bus, plus a
    /// sprite-magnification quirk — see `sega-vdp`.
    fn vdp_variant(self) -> VdpVariant {
        match self {
            Self::Sms1Ntsc | Self::Sms1Pal => VdpVariant::Sms1,
            _ => VdpVariant::Sms2,
        }
    }

    /// The television standard this machine's VDP scans.
    fn region(self) -> VdpRegion {
        match self {
            Self::SmsPal | Self::Sms1Pal => VdpRegion::Pal,
            _ => VdpRegion::Ntsc,
        }
    }
}

/// A Sega Light Phaser plugged into a controller port.
///
/// The gun is a photodiode and a trigger, and it reports position by timing
/// rather than by sending one: it pulls the port's TH pin low while the beam
/// is lighting the spot it is aimed at, and the VDP latches its H counter when
/// the pin comes back up. The game reads $7F and $7E afterwards and gets the
/// raster position of the moment the light stopped.
///
/// This means the picture is part of the input path. A game draws a bright
/// reticle where it thinks the target is; the gun only answers if the beam
/// crosses something bright inside its field of view. Aiming at a dark part of
/// the screen produces no reading at all, which is how the hardware tells a
/// hit from a miss.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
struct LightPhaser {
    /// Where the gun is pointed, in active-display pixels. `None` when no gun
    /// is plugged into this port.
    aim: Option<(u16, u16)>,
    /// Trigger held.
    trigger: bool,
    /// Whether the photodiode is seeing light, which is TH low.
    sensing: bool,
    /// Dots left before the delayed H-counter latch fires, if one is due.
    latch_in: Option<u8>,
}

/// Half-width of the gun's field of view at each vertical distance from its
/// centre, in screen pixels.
///
/// MAME models the sensor as a circle of radius 6 and takes
/// `ceil(sqrt(r^2 - dy^2))` for the half-width, with a shortcut that leaves it
/// at zero on the last row. Six values and a zero, so a table rather than a
/// square root — and the table is exact where a float would invite a rounding
/// question at the edges.
const AIM_HALF_WIDTH: [i32; 7] = [6, 6, 6, 6, 5, 4, 0];

/// Screen pixels between the photodiode releasing TH and the VDP latching.
///
/// MAME schedules the latch this far past the beam position rather than at it,
/// noting that "a delay seems to occur when the Light Phaser latches the VDP
/// hcount". It carries a per-cartridge override for the games needing another
/// figure and falls back to 19; with no cartridge database here, 19 is what
/// every game gets.
const PHASER_LATCH_DELAY: u8 = 19;

/// The luma at or above which the photodiode sees light.
///
/// MAME's comment calls it "brightness of the lightgray color in the frame
/// drawn by Light Phaser games" — the threshold is set by what those games
/// draw, not by a property of the sensor.
const PHASER_MIN_BRIGHTNESS: u32 = 0x7F;

/// Luma of a rendered pixel, by the W3C AERT coefficients MAME uses here.
fn luma(argb: u32) -> u32 {
    let r = (argb >> 16) & 0xFF;
    let g = (argb >> 8) & 0xFF;
    let b = argb & 0xFF;
    (r * 77 + g * 150 + b * 29) >> 8
}

/// Sega Master System / Game Gear machine.
///
/// Fully serialisable for save-states: the Z80, the Sega VDP, the SN76489 PSG,
/// cart ROM, RAM, mapper registers, and controller/pause lines all carry live
/// state. `io_trace` is a host-side debug buffer, not machine state, so it is
/// skipped and defaults on restore.
#[derive(Serialize, Deserialize)]
pub struct Sms {
    cpu: Z80,
    vdp: SegaVdp,
    psg: Sn76489,
    cart_rom: Vec<u8>,
    cart_header: Option<CartridgeHeader>,
    /// Optional base-unit boot ROM, selected through memory-control port $3E.
    bios_rom: Vec<u8>,
    #[serde(with = "BigArray")]
    ram: [u8; 8192],
    /// Battery-backed cartridge SRAM. The Sega mapper exposes either 16 KB
    /// half at `$8000-$BFFF` when `$FFFC` bit 3 is set.
    cartridge_ram: Vec<u8>,
    /// Host-visible writeback signal. Set only when software changes SRAM.
    cartridge_ram_dirty: bool,
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
    /// Port $3F, the I/O control register. Bits 1 and 3 set the controller
    /// ports' TH pins to input; bits 5 and 7 are their output levels.
    io_control: u8,
    /// Port $3E active-low memory/device enables.
    memory_control: u8,
    /// A Light Phaser in each controller port, if one is plugged in.
    phasers: [LightPhaser; 2],
    variant: SmsVariant,
    cpu_tstates: u64,
    tstates_per_frame: u64,
    /// VDP dot-clock phase accumulator (3 dots per 2 CPU T-states).
    vdp_phase: u32,
    frame_count: u64,
    /// When `Some`, every I/O port access is appended here (debug trace).
    #[serde(skip)]
    io_trace: Option<Vec<IoEvent>>,
}

impl Sms {
    /// Create a new SMS / Game Gear with the given cart ROM.
    #[must_use]
    pub fn new(cart_rom: Vec<u8>, variant: SmsVariant) -> Self {
        Self::new_with_bios(cart_rom, Vec::new(), variant)
    }

    /// Create an SMS / Game Gear with an optional base-unit BIOS ROM.
    #[must_use]
    pub fn new_with_bios(cart_rom: Vec<u8>, bios_rom: Vec<u8>, variant: SmsVariant) -> Self {
        let (cart_rom, cart_header) = normalize_cartridge(cart_rom);
        let vdp = if variant.is_game_gear() {
            SegaVdp::new_game_gear()
        } else {
            SegaVdp::new(variant.region(), variant.vdp_variant())
        };
        let memory_control = if bios_rom.is_empty() {
            MEMORY_CONTROL_NO_BIOS
        } else {
            MEMORY_CONTROL_WITH_BIOS
        };
        Self {
            cpu: Z80::new(),
            vdp,
            psg: Sn76489::new(variant.psg_clock_hz(), NoiseLfsr::Sega16),
            cart_rom,
            cart_header,
            bios_rom,
            ram: [0; 8192],
            cartridge_ram: vec![0xFF; 32768],
            cartridge_ram_dirty: false,
            mapper_regs: [0x00, 0x00, 0x01, 0x02],
            port_dc: 0xFF,
            port_dd: 0xFF,
            gg_start: 0xFF,
            pause_pressed: false,
            io_control: 0,
            memory_control,
            phasers: [LightPhaser::default(); 2],
            variant,
            cpu_tstates: 0,
            tstates_per_frame: variant.tstates_per_frame(),
            vdp_phase: 0,
            frame_count: 0,
            io_trace: None,
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
        // Two CPU half-cycles per T-state. `Z80::tick` advances one
        // half-cycle — `T1Rise` then `T1Fall` — so calling it once per
        // T-state ran the CPU at half speed: a `NOP` cost 8 T-states
        // against the Z80's 4, and the machine executed half the work
        // per frame that 228 T-states per scanline budgets for.
        for _ in 0..2 {
            // Pins before the tick, not after. The Z80 samples `/INT` at
            // an instruction boundary during its own tick, so feeding the
            // line afterwards hands it the VDP's state from the previous
            // half-cycle. Same contract as the Spectrum driver; see
            // `knowledge/decisions/zilog-z80-samples-int-at-the-instruction-boundary.md`.
            self.cpu.irq = self.vdp.interrupt;
            // Pause → NMI (level-driven; the host releases).
            self.cpu.nmi = self.pause_pressed;

            self.cpu.tick();
            self.handle_bus();

            // Interleave the VDP per dot, so the line and frame
            // interrupts land at the correct scanline relative to CPU
            // execution — Mode-4 raster splits depend on this.
            self.vdp_phase += VDP_DOT_PHASE_NUMERATOR;
            while self.vdp_phase >= VDP_DOT_PHASE_DENOMINATOR {
                // Where the beam is *about* to draw, taken before the dot is
                // drawn so the pixel read afterwards is the one it just lit.
                let beam = self.vdp.beam_framebuffer_position();
                self.vdp.tick();
                self.tick_light_phasers(beam);
                self.vdp_phase -= VDP_DOT_PHASE_DENOMINATOR;
            }
        }

        // PSG ticks at the Z80 clock on SMS.
        self.psg.tick();

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
                let io_port = (self.cpu.addr & 0xFF) as u8;
                let io_pc = self.cpu.regs.pc;
                let io_val = self.io_read(self.cpu.addr);
                self.cpu.data_in = io_val;
                if let Some(trace) = &mut self.io_trace {
                    trace.push(IoEvent {
                        pc: io_pc,
                        port: io_port,
                        value: io_val,
                        write: false,
                    });
                }
            }
            Some(BusOp::IoWrite) => {
                if let Some(trace) = &mut self.io_trace {
                    trace.push(IoEvent {
                        pc: self.cpu.regs.pc,
                        port: (self.cpu.addr & 0xFF) as u8,
                        value: self.cpu.data,
                        write: true,
                    });
                }
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
            0x0000..=0xBFFF => self.read_selected_rom(addr),
            // 8 KB RAM mirrored across $C000-$FFFF.
            0xC000..=0xFFFF if self.memory_control & MEMORY_DISABLE_WORK_RAM == 0 => {
                self.ram[(addr & 0x1FFF) as usize]
            }
            0xC000..=0xFFFF => 0xFF,
        }
    }

    fn read_selected_rom(&self, addr: u16) -> u8 {
        let mut data = 0xFF;
        if self.memory_control & MEMORY_DISABLE_CARTRIDGE == 0 {
            data &= match addr {
                // The cartridge keeps its first 1 KB fixed for vectors.
                0x0000..=0x03FF => self.cart_rom.get(addr as usize).copied().unwrap_or(0xFF),
                0x0400..=0x3FFF => self.read_rom(self.mapper_regs[1], (addr & 0x3FFF) as usize),
                0x4000..=0x7FFF => self.read_rom(self.mapper_regs[2], (addr & 0x3FFF) as usize),
                0x8000..=0xBFFF if self.mapper_regs[0] & 0x08 == 0 => {
                    self.read_rom(self.mapper_regs[3], (addr & 0x3FFF) as usize)
                }
                0x8000..=0xBFFF => self.cartridge_ram[self.cartridge_ram_addr(addr)],
                _ => unreachable!(),
            };
        }
        if self.memory_control & MEMORY_DISABLE_BIOS == 0 {
            data &= self.bios_rom.get(addr as usize).copied().unwrap_or(0xFF);
        }
        data
    }

    fn mem_write(&mut self, addr: u16, value: u8) {
        if (0x8000..=0xBFFF).contains(&addr)
            && self.memory_control & MEMORY_DISABLE_CARTRIDGE == 0
            && self.mapper_regs[0] & 0x08 != 0
        {
            let ram_addr = self.cartridge_ram_addr(addr);
            if self.cartridge_ram[ram_addr] != value {
                self.cartridge_ram[ram_addr] = value;
                self.cartridge_ram_dirty = true;
            }
            return;
        }
        if (0xC000..=0xFFFF).contains(&addr) {
            if self.memory_control & MEMORY_DISABLE_WORK_RAM == 0 {
                self.ram[(addr & 0x1FFF) as usize] = value;
            }
            if self.memory_control & MEMORY_DISABLE_CARTRIDGE == 0 {
                match addr {
                    0xFFFC => self.mapper_regs[0] = value,
                    0xFFFD => self.mapper_regs[1] = value,
                    0xFFFE => self.mapper_regs[2] = value,
                    0xFFFF => self.mapper_regs[3] = value,
                    _ => {}
                }
            }
        }
    }

    fn cartridge_ram_addr(&self, addr: u16) -> usize {
        let bank = usize::from((self.mapper_regs[0] & 0x04) != 0);
        bank * 0x4000 + usize::from(addr & 0x3FFF)
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
            0xC0..=0xFF if p & 1 == 0 => self.read_controller_port(1),
            0xC0..=0xFF => self.read_controller_port(2),
            _ => 0xFF,
        }
    }

    fn io_write(&mut self, port: u16, value: u8) {
        let p = port as u8;
        if self.variant.is_game_gear() && p == 0x06 {
            self.psg.write_stereo(value);
            return;
        }
        if p == 0x3E {
            self.memory_control = value;
            return;
        }
        match p {
            0x00..=0x3F if p & 1 == 1 => self.write_io_control(value),
            0x40..=0x7F => self.psg.write(value),
            0x80..=0xBF if p & 1 == 0 => self.vdp.write_data(value),
            0x80..=0xBF => self.vdp.write_control(value),
            _ => {}
        }
    }

    /// Advance both Light Phasers by one dot.
    ///
    /// Costs nothing with no gun plugged in, which is the ordinary case: every
    /// aim is `None` and this returns immediately.
    fn tick_light_phasers(&mut self, beam: Option<(u32, u32)>) {
        if self.phasers.iter().all(|phaser| phaser.aim.is_none()) {
            return;
        }
        // Off the displayed screen — sync, blanking, the parts of the border a
        // set crops — there is nothing for a diode to see, but a latch already
        // scheduled still has to run down.
        let lit = beam
            .and_then(|(x, y)| self.vdp.framebuffer_pixel(x, y))
            .is_some_and(|argb| luma(argb) >= PHASER_MIN_BRIGHTNESS);

        for index in 0..self.phasers.len() {
            self.tick_phaser(index, beam, lit);
        }
    }

    /// One gun, one dot.
    fn tick_phaser(&mut self, index: usize, beam: Option<(u32, u32)>, lit: bool) {
        // A latch already scheduled runs down whatever the sensor does next.
        // The countdown is set on the dot TH rose and stepped on each dot
        // after it, so firing when the *decrement* reaches zero puts the latch
        // exactly `PHASER_LATCH_DELAY` dots later — firing when the stored
        // value reads zero would put it one dot further still.
        if let Some(remaining) = self.phasers[index].latch_in {
            let remaining = remaining - 1;
            self.phasers[index].latch_in = if remaining == 0 {
                self.vdp.latch_h_counter();
                None
            } else {
                Some(remaining)
            };
        }

        let Some((aim_x, aim_y)) = self.phasers[index].aim else {
            return;
        };
        let in_view = beam.is_some_and(|(x, y)| {
            let dy = (y as i32 - i32::from(aim_y)).abs();
            dy < AIM_HALF_WIDTH.len() as i32
                && (x as i32 - i32::from(aim_x)).abs() <= AIM_HALF_WIDTH[dy as usize]
        });

        // Once the diode is conducting it stays on until the beam leaves its
        // field of view, whatever the picture does in between — MAME keeps
        // "sensor on until out of the aim area" for the same reason. Without
        // it a reticle with a dark pixel in it would chatter the pin.
        let sensing = if self.phasers[index].sensing {
            in_view
        } else {
            in_view && lit
        };

        if sensing != self.phasers[index].sensing {
            self.phasers[index].sensing = sensing;
            if !sensing {
                // TH has come back up, which is the edge the VDP latches on.
                self.phasers[index].latch_in = Some(PHASER_LATCH_DELAY);
            }
        }
    }

    /// Point a Light Phaser at a spot on the framebuffer, or unplug it with
    /// `None`.
    ///
    /// Framebuffer rather than picture coordinates because the sensor reads
    /// the *screen*, border included: a gun aimed near the edge of the picture
    /// has part of its field of view in the border, and one aimed squarely at
    /// a bright border can be tripped by it.
    pub fn set_light_phaser_aim(&mut self, port: u8, aim: Option<(u16, u16)>) {
        if let Some(phaser) = self.phaser_mut(port) {
            if aim.is_none() {
                *phaser = LightPhaser::default();
            } else {
                phaser.aim = aim;
            }
        }
    }

    /// Where a Light Phaser is pointed, in active-display pixels, or `None`
    /// if the port is empty or the gun is aimed off the picture.
    #[must_use]
    pub fn light_phaser_aim(&self, port: u8) -> Option<(u16, u16)> {
        match port {
            1 => self.phasers[0].aim,
            2 => self.phasers[1].aim,
            _ => None,
        }
    }

    /// Hold or release a Light Phaser's trigger, which the game reads on the
    /// port's TL bit.
    pub fn set_light_phaser_trigger(&mut self, port: u8, pressed: bool) {
        if let Some(phaser) = self.phaser_mut(port) {
            phaser.trigger = pressed;
        }
    }

    fn phaser_mut(&mut self, port: u8) -> Option<&mut LightPhaser> {
        match port {
            1 => Some(&mut self.phasers[0]),
            2 => Some(&mut self.phasers[1]),
            _ => None,
        }
    }

    /// The byte the CPU reads for a controller port, active low.
    ///
    /// This is what the game sees, so it carries a Light Phaser's trigger as
    /// well as whatever the host set for a pad: the trigger is TL, the same
    /// pin a pad uses for button 1.
    #[must_use]
    pub fn read_controller_port(&self, port: u8) -> u8 {
        if self.memory_control & MEMORY_DISABLE_IO != 0 {
            return 0xFF;
        }
        match port {
            1 => self.with_trigger(self.port_dc, 0, 4),
            2 => self.read_controller_port_dd(),
            _ => 0xFF,
        }
    }

    /// Controller/misc port `$DD`, including the TH pins software uses to
    /// distinguish Japanese and export I/O hardware.
    fn read_controller_port_dd(&self) -> u8 {
        let mut value = self.with_trigger(self.port_dd, 1, 2);

        // When TH is an input, the external pin remains visible. When it is an
        // output, export hardware reflects the programmed level while Japanese
        // hardware reads zero. There is no PAL/NTSC flag at $00 or $DD.
        for (direction, output, input) in [(0x02, 0x20, 0x40), (0x08, 0x80, 0x80)] {
            if self.io_control & direction == 0 {
                value &= !input;
                if !self.variant.is_japan() && self.io_control & output != 0 {
                    value |= input;
                }
            }
        }
        value
    }

    /// A controller byte with any Light Phaser trigger folded in.
    ///
    /// The trigger is TL — bit 4 of $DC for port 1, bit 2 of $DD for port 2 —
    /// and it is active low.
    fn with_trigger(&self, value: u8, index: usize, bit: u8) -> u8 {
        if self.phasers[index].aim.is_some() && self.phasers[index].trigger {
            value & !(1 << bit)
        } else {
            value
        }
    }

    /// Port $3F, the I/O control register.
    ///
    /// Bits 1 and 3 set the TH pins of controller ports A and B to input;
    /// bits 5 and 7 are their levels when driven as outputs. A pin switched
    /// to input floats high, so taking TH from output-low to input is a
    /// low-to-high transition on the pin — and that is what latches the VDP's
    /// H counter.
    ///
    /// Writing $3F twice, once with TH low and once with it released, is
    /// therefore how a game reads a horizontal raster position without a
    /// light gun: it is the only path to the counter, since the CPU cannot
    /// see it free running.
    ///
    /// Driving TH high as an *output* does not latch. The sense path is
    /// connected to the pin only while TH is configured as an input, so an
    /// output level is not an edge the counter can see — and a pin that was
    /// already high as an output has no edge to give when it is released
    /// either, which is what the second half of each mask below tests for.
    ///
    /// MAME gates the same transition on the pin's *external* level as well,
    /// because a peripheral can hold TH down. Nothing here drives TH, and an
    /// unconnected or ordinary pad leaves it high, so that term is constant
    /// true until the Light Phaser lands (#205).
    fn write_io_control(&mut self, value: u8) {
        const PORT_A: (u8, u8) = (0x02, 0x22); // TH input bit, input-or-high mask
        const PORT_B: (u8, u8) = (0x08, 0x88);
        let released = |(input, held): (u8, u8)| value & input != 0 && self.io_control & held == 0;
        if released(PORT_A) || released(PORT_B) {
            self.vdp.latch_h_counter();
        }
        self.io_control = value;
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

    /// The complete 32 KB battery-backed SRAM image.
    #[must_use]
    pub fn cartridge_ram(&self) -> &[u8] {
        &self.cartridge_ram
    }

    /// Whether cartridge software has changed SRAM since construction/load.
    #[must_use]
    pub const fn cartridge_ram_dirty(&self) -> bool {
        self.cartridge_ram_dirty
    }

    /// Replace SRAM from host state and choose whether it still needs writing.
    pub fn restore_cartridge_ram(&mut self, bytes: &[u8], dirty: bool) -> bool {
        if bytes.len() != self.cartridge_ram.len() {
            return false;
        }
        self.cartridge_ram.copy_from_slice(bytes);
        self.cartridge_ram_dirty = dirty;
        true
    }

    /// Standard Sega cartridge header metadata, when the signature is present.
    #[must_use]
    pub fn cartridge_header(&self) -> Option<&CartridgeHeader> {
        self.cart_header.as_ref()
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

impl emu198x_zilog_z80::Z80Stepper for Sms {
    fn z80_instructions_retired(&self) -> u64 {
        self.cpu.instructions_retired()
    }

    fn step_tick(&mut self) {
        self.tick_tstate();
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

    /// #140: the H counter was declared, serialised and read, but never
    /// written, so port $7F always returned 0. It is latched by a low-to-high
    /// transition on a controller port's TH pin, and port $3F is what moves
    /// those pins — so with $3F ignored there was no path to the counter at
    /// all.
    ///
    /// The idiom is two writes: drive TH low, then release it to input.
    #[test]
    fn releasing_th_latches_the_h_counter() {
        let mut sys = Sms::new(trap_cart_64k(), SmsVariant::SmsNtsc);
        assert_eq!(sys.io_read(0x7F), 0, "nothing latched from cold");

        // Let the beam get somewhere non-zero, then drive both TH pins low as
        // outputs and release port A's to input.
        for _ in 0..200 {
            sys.tick_tstate();
        }
        sys.io_write(0x3F, 0x00);
        assert_eq!(sys.io_read(0x7F), 0, "driving TH low must not latch");
        sys.io_write(0x3F, 0x02);
        let first = sys.io_read(0x7F);
        assert_ne!(first, 0, "releasing TH to input should latch");

        // Repeating the same value is not a transition.
        for _ in 0..200 {
            sys.tick_tstate();
        }
        sys.io_write(0x3F, 0x02);
        assert_eq!(
            sys.io_read(0x7F),
            first,
            "TH already high is not a low-to-high transition"
        );

        // Driving it low again and releasing gives a new position.
        sys.io_write(0x3F, 0x00);
        sys.io_write(0x3F, 0x02);
        assert_ne!(
            sys.io_read(0x7F),
            first,
            "the beam moved, so the second latch should differ"
        );
    }

    /// Either port's TH latches, but only by becoming an *input*. While TH is
    /// driven as an output the VDP's sense path is disconnected from it, so
    /// raising an output high is not something the counter can see.
    #[test]
    fn only_switching_th_to_input_latches() {
        for (write, latches) in [
            (0x02u8, true), // port A TH to input
            (0x08, true),   // port B TH to input
            (0x20, false),  // port A TH driven high as an output
            (0x80, false),  // port B TH driven high as an output
            (0x00, false),  // still driven low
        ] {
            let mut sys = Sms::new(trap_cart_64k(), SmsVariant::SmsNtsc);
            for _ in 0..300 {
                sys.tick_tstate();
            }
            sys.io_write(0x3F, 0x00); // both TH pins output, low
            sys.io_write(0x3F, write);
            assert_eq!(
                sys.io_read(0x7F) != 0,
                latches,
                "$3F = {write:#04X} should {} latch",
                if latches { "" } else { "not" }
            );
        }
    }

    /// The pin has to have been *low* for releasing it to be a transition. TH
    /// driven high as an output is already high, so switching it to input
    /// changes nothing and latches nothing — which is what the "was neither
    /// input nor output-high" half of the condition is for.
    #[test]
    fn releasing_a_pin_that_was_already_high_does_not_latch() {
        let mut sys = Sms::new(trap_cart_64k(), SmsVariant::SmsNtsc);
        for _ in 0..300 {
            sys.tick_tstate();
        }
        sys.io_write(0x3F, 0x20); // port A TH: output, driven high
        sys.io_write(0x3F, 0x02); // now an input — but the pin never moved
        assert_eq!(sys.io_read(0x7F), 0, "no edge, so nothing to latch");

        // Whereas from low it does.
        sys.io_write(0x3F, 0x00);
        sys.io_write(0x3F, 0x02);
        assert_ne!(sys.io_read(0x7F), 0);
    }

    /// Port $3F is odd-addressed in the $00-$3F range; the even ports are the
    /// memory-control register and must not latch anything.
    #[test]
    fn the_memory_control_port_does_not_latch() {
        let mut sys = Sms::new(trap_cart_64k(), SmsVariant::SmsNtsc);
        for _ in 0..300 {
            sys.tick_tstate();
        }
        sys.io_write(0x3E, 0x02);
        assert_eq!(sys.io_read(0x7F), 0, "$3E is memory control, not I/O");
    }

    #[test]
    fn memory_control_switches_between_bios_cartridge_and_open_bus() {
        let cart = vec![0xC3; 0xC000];
        let bios = vec![0x5A; 0x2000];
        let mut sys = Sms::new_with_bios(cart, bios, SmsVariant::SmsNtsc);

        assert_eq!(
            sys.peek(0),
            0x5A,
            "the documented power-on map selects BIOS"
        );
        sys.io_write(0x3E, MEMORY_CONTROL_NO_BIOS);
        assert_eq!(
            sys.peek(0),
            0xC3,
            "disabling BIOS and enabling cart selects cart"
        );
        sys.io_write(0x3E, MEMORY_CONTROL_NO_BIOS | MEMORY_DISABLE_CARTRIDGE);
        assert_eq!(sys.peek(0), 0xFF, "no selected ROM leaves the bus open");
    }

    #[test]
    fn memory_control_cart_disable_disconnects_every_cartridge_window() {
        let mut cart = vec![0; 0xC000];
        cart[0] = 0x11;
        cart[0x4000] = 0x22;
        cart[0x8000] = 0x33;
        let mut sys = Sms::new(cart, SmsVariant::SmsNtsc);
        assert_eq!(
            [sys.peek(0), sys.peek(0x4000), sys.peek(0x8000)],
            [0x11, 0x22, 0x33]
        );

        sys.io_write(0x3E, MEMORY_CONTROL_NO_BIOS | MEMORY_DISABLE_CARTRIDGE);
        assert_eq!([sys.peek(0), sys.peek(0x4000), sys.peek(0x8000)], [0xFF; 3]);
    }

    #[test]
    fn memory_control_gates_controllers_and_work_ram() {
        let mut sys = Sms::new(trap_cart_64k(), SmsVariant::SmsNtsc);
        sys.set_port_dc(0x00);
        sys.poke(0xC123, 0xA5);
        assert_eq!(sys.io_read(0xDC), 0x00);
        assert_eq!(sys.peek(0xC123), 0xA5);

        sys.io_write(
            0x3E,
            MEMORY_CONTROL_NO_BIOS | MEMORY_DISABLE_IO | MEMORY_DISABLE_WORK_RAM,
        );
        assert_eq!(sys.io_read(0xDC), 0xFF);
        assert_eq!(sys.peek(0xC123), 0xFF);
        sys.poke(0xC123, 0x3C);

        sys.io_write(0x3E, MEMORY_CONTROL_NO_BIOS);
        assert_eq!(
            sys.peek(0xC123),
            0xA5,
            "disabled work RAM must ignore writes"
        );
    }

    #[test]
    fn th_output_readback_distinguishes_export_and_japanese_io_chips() {
        for (variant, expected) in [
            (SmsVariant::SmsNtsc, 0xC0),
            (SmsVariant::SmsPal, 0xC0),
            (SmsVariant::SmsJapanNtsc, 0x00),
        ] {
            let mut sys = Sms::new(trap_cart_64k(), variant);
            sys.set_port_dd(0xFF);
            // Both TH pins are outputs (direction bits 1/3 clear) driven high
            // (level bits 5/7 set).
            sys.io_write(0x3F, 0xA0);
            assert_eq!(
                sys.io_read(0xDD) & 0xC0,
                expected,
                "{variant:?} TH output readback"
            );
        }
    }

    #[test]
    fn input_configured_th_pins_report_the_external_level_in_every_region() {
        for variant in [SmsVariant::SmsNtsc, SmsVariant::SmsJapanNtsc] {
            let mut sys = Sms::new(trap_cart_64k(), variant);
            sys.set_port_dd(0xFF);
            sys.io_write(0x3F, 0x0A); // both TH pins inputs
            assert_eq!(sys.io_read(0xDD) & 0xC0, 0xC0, "{variant:?}");
        }
    }

    #[test]
    fn pal_and_ntsc_export_profiles_have_the_same_region_readback() {
        let mut ntsc = Sms::new(trap_cart_64k(), SmsVariant::SmsNtsc);
        let mut pal = Sms::new(trap_cart_64k(), SmsVariant::SmsPal);
        ntsc.io_write(0x3F, 0xA0);
        pal.io_write(0x3F, 0xA0);
        assert_eq!(ntsc.io_read(0xDD), pal.io_read(0xDD));
    }

    /// Two latches a known distance apart differ by half that in counts,
    /// because the counter is a beam position rather than a clock.
    #[test]
    fn the_latched_value_tracks_where_the_beam_is() {
        let mut sys = Sms::new(trap_cart_64k(), SmsVariant::SmsNtsc);
        let mut samples = Vec::new();
        for _ in 0..4 {
            for _ in 0..40 {
                sys.tick_tstate();
            }
            sys.io_write(0x3F, 0x00);
            sys.io_write(0x3F, 0x02);
            samples.push(sys.io_read(0x7F));
        }
        // Equal T-state gaps should give equal counter gaps, whatever the
        // exact dots-per-T-state ratio works out to.
        let steps: Vec<i32> = samples
            .windows(2)
            .map(|w| i32::from(w[1]) - i32::from(w[0]))
            .collect();
        assert!(
            steps.iter().all(|&s| s > 0),
            "the beam only moves forward across a line: {samples:?}"
        );
        assert!(
            steps.windows(2).all(|w| (w[0] - w[1]).abs() <= 1),
            "equal waits should advance the counter equally: {steps:?}"
        );
    }

    /// Save-state must capture LIVE machine state (Z80 + Sega VDP + SN76489
    /// PSG + work/cart RAM + mapper), not cold-boot from the cart. Serialise, advance (so
    /// the state differs), then deserialise the first snapshot and confirm
    /// re-serialising it is byte-identical — every stateful field across all
    /// three chips round-trips, including the VDP's 16 KB VRAM.
    #[test]
    fn snapshot_round_trips_live_state() {
        let mut sys = Sms::new(trap_cart_64k(), SmsVariant::SmsNtsc);
        sys.run_frame();
        sys.poke(0xC100, 0xA5); // a work-RAM byte to carry across the snapshot
        sys.poke(0xFFFC, 0x08);
        sys.poke(0x8123, 0x5A); // a cartridge-SRAM byte to carry across it too
        sys.run_frame();
        let s1 = postcard::to_allocvec(&sys).expect("encode snapshot");

        sys.run_frame(); // advance past the snapshot point
        let s2 = postcard::to_allocvec(&sys).expect("encode again");
        assert_ne!(s1, s2, "running a frame should change the serialised state");

        let restored: Sms = postcard::from_bytes(&s1).expect("decode snapshot");
        assert_eq!(restored.peek(0x8123), 0x5A);
        let s3 = postcard::to_allocvec(&restored).expect("re-encode restored");
        assert_eq!(
            s1, s3,
            "restore should reproduce the snapshot state exactly"
        );
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
    fn sega_mapper_cartridge_ram_round_trips_and_restores_rom() {
        let mut cart = vec![0u8; 0x10000];
        cart[0x8000] = 0x42;
        let mut sys = Sms::new(cart, SmsVariant::SmsNtsc);

        assert_eq!(sys.mem_read(0x8000), 0x42);
        sys.mem_write(0xFFFC, 0x08);
        assert_eq!(sys.mem_read(0x8000), 0xFF);
        sys.mem_write(0x8000, 0xA5);
        assert_eq!(sys.mem_read(0x8000), 0xA5);

        sys.mem_write(0xFFFC, 0x00);
        assert_eq!(sys.mem_read(0x8000), 0x42);
    }

    #[test]
    fn sega_mapper_selects_both_cartridge_ram_banks() {
        let mut sys = Sms::new(trap_cart_64k(), SmsVariant::SmsNtsc);

        sys.mem_write(0xFFFC, 0x08);
        sys.mem_write(0xBFFF, 0x11);
        sys.mem_write(0xFFFC, 0x0C);
        assert_eq!(sys.mem_read(0xBFFF), 0xFF);
        sys.mem_write(0xBFFF, 0x22);

        sys.mem_write(0xFFFC, 0x08);
        assert_eq!(sys.mem_read(0xBFFF), 0x11);
        sys.mem_write(0xFFFC, 0x0C);
        assert_eq!(sys.mem_read(0xBFFF), 0x22);
    }

    #[test]
    fn cartridge_ram_is_dirty_only_after_a_changed_sram_write() {
        let mut sys = Sms::new(trap_cart_64k(), SmsVariant::SmsNtsc);
        assert!(!sys.cartridge_ram_dirty());

        sys.mem_write(0x8000, 0x42);
        assert!(!sys.cartridge_ram_dirty(), "ROM writes do not dirty SRAM");
        sys.mem_write(0xFFFC, 0x08);
        sys.mem_write(0x8000, 0xFF);
        assert!(
            !sys.cartridge_ram_dirty(),
            "unchanged SRAM does not need a save"
        );
        sys.mem_write(0x8000, 0x42);
        assert!(sys.cartridge_ram_dirty());

        let image = sys.cartridge_ram().to_vec();
        assert!(sys.restore_cartridge_ram(&image, false));
        assert!(!sys.cartridge_ram_dirty());
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
        sys.io_write(0x3F, 0x0A); // both TH pins inputs, exposing external levels
        assert_eq!(sys.io_read(0xDC), 0xAA);
        assert_eq!(sys.io_read(0xDD), 0x55);
    }

    #[test]
    fn game_gear_start_button_at_port_00() {
        let mut sys = Sms::new(trap_cart_64k(), SmsVariant::GameGear);
        sys.set_gg_start(0x7F); // START pressed (bit 7 low).
        assert_eq!(sys.io_read(0x00), 0x7F);
    }

    // -----------------------------------------------------------------------
    // The Sega Light Phaser
    //
    // The gun reports position by timing rather than by sending one, so the
    // picture is part of the input path and these tests have to draw one. A
    // game puts a bright reticle where it thinks the target is; the gun only
    // answers if the beam crosses something bright inside its field of view.
    // -----------------------------------------------------------------------

    fn phaser_write_register(sys: &mut Sms, reg: u8, value: u8) {
        sys.io_write(0xBF, value);
        sys.io_write(0xBF, 0x80 | (reg & 0x0F));
    }

    fn phaser_poke_vram(sys: &mut Sms, addr: u16, bytes: &[u8]) {
        sys.io_write(0xBF, addr as u8);
        sys.io_write(0xBF, ((addr >> 8) as u8 & 0x3F) | 0x40);
        for &b in bytes {
            sys.io_write(0xBE, b);
        }
    }

    fn phaser_poke_cram(sys: &mut Sms, index: u8, value: u8) {
        sys.io_write(0xBF, index);
        sys.io_write(0xBF, 0xC0);
        sys.io_write(0xBE, value);
    }

    /// A screen filled with one colour. `0x3F` is white and reads as bright;
    /// `0x00` is black and does not.
    fn phaser_screen_of(colour: u8) -> Sms {
        let mut sys = Sms::new(trap_cart_64k(), SmsVariant::SmsNtsc);
        phaser_write_register(&mut sys, 0, 0x04); // Mode 4
        phaser_write_register(&mut sys, 1, 0x40); // display on
        phaser_write_register(&mut sys, 2, 0x0E); // name table $3800
        phaser_write_register(&mut sys, 3, 0xFF);
        phaser_write_register(&mut sys, 4, 0x07);
        phaser_poke_cram(&mut sys, 1, colour);
        let mut tile = [0u8; 32];
        for row in 0..8 {
            tile[row * 4] = 0xFF; // every pixel colour index 1
        }
        phaser_poke_vram(&mut sys, 0x0020, &tile);
        for row in 0..28u16 {
            for col in 0..32u16 {
                phaser_poke_vram(&mut sys, 0x3800 + row * 64 + col * 2, &[0x01, 0x00]);
            }
        }
        sys
    }

    /// Aim at a spot on the *picture*, which is where these tests draw.
    ///
    /// The machine takes framebuffer coordinates, so the picture sits an
    /// NTSC border in from the top-left: 12 pixels across, 25 down.
    fn phaser_aim(sys: &mut Sms, port: u8, x: u16, y: u16) {
        sys.set_light_phaser_aim(port, Some((x + 12, y + 25)));
    }

    /// Read the latched horizontal position the way a game does.
    ///
    /// Only H is latched. The V counter is live — MAME computes it from the
    /// beam on every read — so a game takes it immediately, while the beam is
    /// still near the line the light was on. Nothing here runs Z80 code, so
    /// there is no "immediately" to read it in, and these tests pin vertical
    /// placement through what the gun does rather than through $7E.
    fn phaser_latched(sys: &mut Sms) -> u8 {
        sys.io_read(0x7F)
    }

    fn phaser_run_frames(sys: &mut Sms, count: usize) {
        for _ in 0..count {
            sys.run_frame();
        }
    }

    /// Aimed at a lit screen the gun latches a position; aimed at a dark one
    /// it latches nothing and $7F keeps its reset value. That difference is
    /// the whole mechanism — it is how the hardware tells a hit from a miss.
    #[test]
    fn the_gun_answers_a_lit_screen_and_ignores_a_dark_one() {
        let mut lit = phaser_screen_of(0x3F);
        phaser_aim(&mut lit, 1, 128, 96);
        phaser_run_frames(&mut lit, 2);
        assert_ne!(
            phaser_latched(&mut lit),
            0,
            "a bright screen should have latched something"
        );

        let mut dark = phaser_screen_of(0x00);
        phaser_aim(&mut dark, 1, 128, 96);
        phaser_run_frames(&mut dark, 2);
        assert_eq!(
            phaser_latched(&mut dark),
            0,
            "a dark screen gives the diode nothing to see"
        );
    }

    /// A screen with a bright band across rows 4 and 5 — lines 32 to 47 — and
    /// everything else dark, which is roughly the shape of the reticle a
    /// light-gun game draws.
    fn phaser_banded_screen() -> Sms {
        let mut sys = Sms::new(trap_cart_64k(), SmsVariant::SmsNtsc);
        phaser_write_register(&mut sys, 0, 0x04);
        phaser_write_register(&mut sys, 1, 0x40);
        phaser_write_register(&mut sys, 2, 0x0E);
        phaser_write_register(&mut sys, 3, 0xFF);
        phaser_write_register(&mut sys, 4, 0x07);
        phaser_poke_cram(&mut sys, 1, 0x3F); // white
        phaser_poke_cram(&mut sys, 2, 0x00); // black
        for (index, colour) in [(1u16, 0u8), (2, 1)] {
            let mut tile = [0u8; 32];
            for row in 0..8 {
                tile[row * 4 + colour as usize] = 0xFF;
            }
            phaser_poke_vram(&mut sys, index * 32, &tile);
        }
        for row in 0..28u16 {
            let tile = if (4..=5).contains(&row) { 0x01 } else { 0x02 };
            for col in 0..32u16 {
                phaser_poke_vram(&mut sys, 0x3800 + row * 64 + col * 2, &[tile, 0x00]);
            }
        }
        sys
    }

    /// The gun sees the picture, not the aim point. Pointed into the bright
    /// band it answers; pointed at the dark screen a few rows away it does
    /// not, though nothing about the gun has changed.
    ///
    /// This is the vertical half of the field of view and the brightness gate
    /// at once, and it is what makes a light gun a light gun: the console
    /// never learns where the barrel is pointing, only when the beam lit
    /// something the diode could see.
    #[test]
    fn the_gun_answers_only_where_the_picture_is_lit() {
        for (aim_y, answers) in [(20u16, false), (40, true), (80, false)] {
            let mut sys = phaser_banded_screen();
            phaser_aim(&mut sys, 1, 128, aim_y);
            phaser_run_frames(&mut sys, 2);
            assert_eq!(
                phaser_latched(&mut sys) != 0,
                answers,
                "aimed at line {aim_y}, where the band covers 32 to 47"
            );
        }
    }

    /// The field of view has a size: aiming just outside the band still
    /// catches it, because the diode sees a circle six pixels across and not
    /// a point.
    #[test]
    fn the_field_of_view_reaches_a_few_lines_past_the_aim() {
        for (aim_y, answers) in [(27u16, true), (24, false)] {
            let mut sys = phaser_banded_screen();
            phaser_aim(&mut sys, 1, 128, aim_y);
            phaser_run_frames(&mut sys, 2);
            assert_eq!(
                phaser_latched(&mut sys) != 0,
                answers,
                "aimed at line {aim_y}, five lines above a band starting at 32"
            );
        }
    }

    /// The latched H counter tracks the column aimed at. The counter steps
    /// once per two pixels, so moving the aim across the screen moves the
    /// reading by half as much — and it is that proportion, not the absolute
    /// value, that a game calibrates against.
    #[test]
    fn the_latched_column_tracks_the_column_aimed_at() {
        let read = |aim_x: u16| {
            let mut sys = phaser_screen_of(0x3F);
            phaser_aim(&mut sys, 1, aim_x, 96);
            phaser_run_frames(&mut sys, 2);
            u32::from(phaser_latched(&mut sys))
        };

        let near = read(64);
        let far = read(192);
        assert!(far > near, "aiming right should latch a higher count");
        let step = far - near;
        assert!(
            (60..=70).contains(&step),
            "128 pixels of aim should move the counter about 64 steps, saw {step}"
        );
    }

    /// The H counter as the VDP derives it, so a test can say where the beam
    /// was without asking the chip.
    fn phaser_hcount_at(dot: i32) -> u8 {
        ((((dot + 62).rem_euclid(342)) - 46) >> 1) as u8
    }

    /// The latch happens when the diode *stops* seeing light, not when it
    /// starts, and `PHASER_LATCH_DELAY` dots after that.
    ///
    /// With the band ending at line 47 and the gun aimed at line 44, the last
    /// line the diode sees is three above centre, where its field of view is
    /// six pixels wide. So the light stops at `aim + 7` and the counter is
    /// taken 19 dots later — an exact position, not a range.
    ///
    /// This is the difference our own reference had backwards until yesterday:
    /// the falling edge sets the TH bit a program polls, and the rising edge
    /// is what reaches the counter. Latching on the wrong one puts every shot
    /// about thirteen pixels left of where it was aimed.
    #[test]
    fn the_latch_takes_the_trailing_edge_plus_the_delay() {
        for aim_x in [64u16, 128, 192] {
            let mut sys = phaser_banded_screen();
            phaser_aim(&mut sys, 1, aim_x, 44);
            phaser_run_frames(&mut sys, 2);

            let aim = i32::from(aim_x);
            let delay = i32::from(PHASER_LATCH_DELAY);
            assert_eq!(
                phaser_latched(&mut sys),
                phaser_hcount_at(aim + 6 + 1 + delay),
                "aim {aim_x}: the trailing edge of a six-pixel view, plus {delay}"
            );
            assert_ne!(
                phaser_latched(&mut sys),
                phaser_hcount_at(aim - 6 + delay),
                "aim {aim_x}: not the leading edge"
            );
        }
    }

    /// A screen that is dark but for one lit column of tiles, so the gun's
    /// horizontal reach is the only thing that decides whether it answers.
    fn phaser_striped_screen(lit_column: u16) -> Sms {
        let mut sys = Sms::new(trap_cart_64k(), SmsVariant::SmsNtsc);
        phaser_write_register(&mut sys, 0, 0x04);
        phaser_write_register(&mut sys, 1, 0x40);
        phaser_write_register(&mut sys, 2, 0x0E);
        phaser_write_register(&mut sys, 3, 0xFF);
        phaser_write_register(&mut sys, 4, 0x07);
        phaser_poke_cram(&mut sys, 1, 0x3F); // white
        phaser_poke_cram(&mut sys, 2, 0x00); // black
        for (index, plane) in [(1u16, 0usize), (2, 1)] {
            let mut tile = [0u8; 32];
            for row in 0..8 {
                tile[row * 4 + plane] = 0xFF;
            }
            phaser_poke_vram(&mut sys, index * 32, &tile);
        }
        for row in 0..28u16 {
            for col in 0..32u16 {
                let tile = if col == lit_column { 0x01 } else { 0x02 };
                phaser_poke_vram(&mut sys, 0x3800 + row * 64 + col * 2, &[tile, 0x00]);
            }
        }
        sys
    }

    /// The diode sees a circle six pixels across, not a point, so it catches
    /// light a little to either side of where the gun is pointed. Aim five
    /// pixels off the lit column and it still answers; aim ten and it does
    /// not.
    #[test]
    fn the_field_of_view_reaches_a_few_pixels_to_each_side() {
        // Tile column 16 lights pixels 128 through 135.
        for (aim_x, answers) in [(140u16, true), (145, false), (123, true), (118, false)] {
            let mut sys = phaser_striped_screen(16);
            phaser_aim(&mut sys, 1, aim_x, 96);
            phaser_run_frames(&mut sys, 2);
            assert_eq!(
                phaser_latched(&mut sys) != 0,
                answers,
                "aimed at {aim_x}, with light from 128 to 135"
            );
        }
    }

    /// A screen dark but for one lit tile — pixels 128 to 135 across, lines
    /// 32 to 39 down. Small enough that the gun's field of view is larger
    /// than the light, which is what makes the shape of that view visible.
    fn phaser_block_screen() -> Sms {
        let mut sys = Sms::new(trap_cart_64k(), SmsVariant::SmsNtsc);
        phaser_write_register(&mut sys, 0, 0x04);
        phaser_write_register(&mut sys, 1, 0x40);
        phaser_write_register(&mut sys, 2, 0x0E);
        phaser_write_register(&mut sys, 3, 0xFF);
        phaser_write_register(&mut sys, 4, 0x07);
        phaser_poke_cram(&mut sys, 1, 0x3F); // white
        phaser_poke_cram(&mut sys, 2, 0x00); // black
        for (index, plane) in [(1u16, 0usize), (2, 1)] {
            let mut tile = [0u8; 32];
            for row in 0..8 {
                tile[row * 4 + plane] = 0xFF;
            }
            phaser_poke_vram(&mut sys, index * 32, &tile);
        }
        for row in 0..28u16 {
            for col in 0..32u16 {
                let tile = if row == 4 && col == 16 { 0x01 } else { 0x02 };
                phaser_poke_vram(&mut sys, 0x3800 + row * 64 + col * 2, &[tile, 0x00]);
            }
        }
        sys
    }

    /// The field of view is round. Its half-width shrinks as the beam moves
    /// away from the centre line — 6 pixels for the first four rows, then 5,
    /// then 4, then nothing — so a gun aimed five pixels to the side of some
    /// light picks it up while the light is nearly level and loses it as the
    /// vertical distance grows.
    ///
    /// A square field of view of the same reach would keep answering, which
    /// is the difference this pins.
    #[test]
    fn the_field_of_view_is_round_and_narrows_off_centre() {
        // Light runs 128..135 across and 32..39 down; every aim below sits
        // five pixels right of its right edge, and differs only in height.
        for (aim_y, answers) in [(36u16, true), (28, true), (27, false)] {
            let mut sys = phaser_block_screen();
            phaser_aim(&mut sys, 1, 140, aim_y);
            phaser_run_frames(&mut sys, 2);
            assert_eq!(
                phaser_latched(&mut sys) != 0,
                answers,
                "aimed at line {aim_y}, five pixels beside light on lines 32 to 39"
            );
        }
    }

    /// Once the diode is conducting it stays on until the beam leaves its
    /// field of view, whatever the picture does in between. So the latch lands
    /// where the *view* ends, not where the light does — and a reticle with a
    /// dark pixel in it cannot chatter the pin.
    #[test]
    fn the_diode_stays_on_until_the_beam_leaves_its_view() {
        let mut sys = phaser_block_screen();
        phaser_aim(&mut sys, 1, 132, 36);
        phaser_run_frames(&mut sys, 2);

        let delay = i32::from(PHASER_LATCH_DELAY);
        // Line 39 is the last with any light in view; there the view runs to
        // 138 while the light stops at 135.
        assert_eq!(
            phaser_latched(&mut sys),
            phaser_hcount_at(132 + 6 + 1 + delay),
            "the latch should follow the edge of the view"
        );
        assert_ne!(
            phaser_latched(&mut sys),
            phaser_hcount_at(135 + 1 + delay),
            "and not the edge of the light"
        );
    }

    /// The diode answers to luminance, not to how much colour is on screen.
    /// A full-intensity green triggers it and a full-intensity red or blue
    /// does not, though all three drive one channel as hard as the chip can.
    ///
    /// That is the W3C AERT weighting MAME uses here — green carries about
    /// three fifths of perceived brightness and blue about a ninth — and it
    /// is why light-gun games draw white or light-grey reticles rather than
    /// coloured ones. A plain average of the channels would reject all three.
    #[test]
    fn the_diode_weights_the_channels_by_luminance() {
        for (colour, name, answers) in [
            (0x3Fu8, "white", true),
            (0x0C, "green", true),
            (0x03, "red", false),
            (0x30, "blue", false),
            (0x00, "black", false),
        ] {
            let mut sys = phaser_screen_of(colour);
            phaser_aim(&mut sys, 1, 128, 96);
            phaser_run_frames(&mut sys, 2);
            assert_eq!(
                phaser_latched(&mut sys) != 0,
                answers,
                "a screen of {name} should {} the diode",
                if answers { "trip" } else { "not trip" }
            );
        }
    }

    /// The sensor reads the screen, and the screen includes the border. A gun
    /// aimed squarely at a bright backdrop is tripped by it, on a screen whose
    /// picture is entirely dark.
    ///
    /// This is what a light gun is looking at, rather than what the game
    /// thinks it drew, and it is why the aim is in framebuffer coordinates.
    #[test]
    fn a_bright_border_trips_the_gun() {
        let mut sys = phaser_screen_of(0x00); // black picture
        phaser_write_register(&mut sys, 7, 0x00);
        phaser_poke_cram(&mut sys, 16, 0x3F); // white backdrop
        // Framebuffer (4, 120): four pixels in from the left edge, so well
        // inside the twelve-pixel border.
        sys.set_light_phaser_aim(1, Some((4, 120)));
        phaser_run_frames(&mut sys, 2);
        assert_ne!(
            phaser_latched(&mut sys),
            0,
            "a white border is light the diode can see"
        );

        let mut dark = phaser_screen_of(0x00);
        phaser_write_register(&mut dark, 7, 0x00);
        phaser_poke_cram(&mut dark, 16, 0x00); // black backdrop
        dark.set_light_phaser_aim(1, Some((4, 120)));
        phaser_run_frames(&mut dark, 2);
        assert_eq!(phaser_latched(&mut dark), 0, "a dark one is not");
    }

    /// A gun aimed at the edge of the picture has part of its field of view in
    /// the border, and sees what is there. With a dark picture and a bright
    /// backdrop it is the border alone that answers.
    #[test]
    fn a_gun_at_the_edge_of_the_picture_sees_into_the_border() {
        let mut sys = phaser_screen_of(0x00);
        phaser_write_register(&mut sys, 7, 0x00);
        phaser_poke_cram(&mut sys, 16, 0x3F); // white backdrop
        // Picture column 2 — three pixels of the six-wide view fall in the
        // border to its left.
        phaser_aim(&mut sys, 1, 2, 100);
        phaser_run_frames(&mut sys, 2);
        assert_ne!(
            phaser_latched(&mut sys),
            0,
            "the part of the view over the border is still part of it"
        );

        // Well inside the picture, the same dark screen gives nothing.
        let mut inside = phaser_screen_of(0x00);
        phaser_write_register(&mut inside, 7, 0x00);
        phaser_poke_cram(&mut inside, 16, 0x3F);
        phaser_aim(&mut inside, 1, 128, 100);
        phaser_run_frames(&mut inside, 2);
        assert_eq!(phaser_latched(&mut inside), 0);
    }

    /// Sync and blanking are not screen. The beam passes through them on every
    /// line, and a gun cannot be tripped there however bright the picture.
    #[test]
    fn the_gun_sees_nothing_during_blanking() {
        let mut sys = phaser_screen_of(0x3F);
        // Framebuffer positions stop at 280 across on NTSC; anything past that
        // is a place the beam goes but the screen does not show.
        sys.set_light_phaser_aim(1, Some((300, 120)));
        phaser_run_frames(&mut sys, 2);
        assert_eq!(phaser_latched(&mut sys), 0);
    }

    /// Nothing is latched with no gun plugged in, however bright the screen.
    #[test]
    fn an_empty_port_latches_nothing() {
        let mut sys = phaser_screen_of(0x3F);
        phaser_run_frames(&mut sys, 2);
        assert_eq!(phaser_latched(&mut sys), 0);
    }

    /// Unplugging the gun takes its reading away with it, rather than leaving
    /// the sensor half on.
    #[test]
    fn unplugging_the_gun_stops_it_answering() {
        let mut sys = phaser_screen_of(0x3F);
        phaser_aim(&mut sys, 1, 128, 96);
        phaser_run_frames(&mut sys, 2);
        assert_ne!(phaser_latched(&mut sys), 0);

        sys.set_light_phaser_aim(1, None);
        let before = phaser_latched(&mut sys);
        phaser_run_frames(&mut sys, 2);
        assert_eq!(
            phaser_latched(&mut sys),
            before,
            "with the gun out, nothing should move the counter"
        );
    }

    /// The trigger is TL, the same pin a pad uses for button 1 — bit 4 of $DC
    /// on port 1, bit 2 of $DD on port 2 — and it is active low.
    #[test]
    fn the_trigger_reads_back_on_the_ports_tl_bit() {
        for (port, io_port, bit) in [(1u8, 0xDCu16, 4u8), (2, 0xDD, 2)] {
            let mut sys = phaser_screen_of(0x3F);
            phaser_aim(&mut sys, port, 128, 96);
            assert_eq!(
                sys.io_read(io_port) & (1 << bit),
                1 << bit,
                "port {port}: an unheld trigger reads high"
            );

            sys.set_light_phaser_trigger(port, true);
            assert_eq!(
                sys.io_read(io_port) & (1 << bit),
                0,
                "port {port}: a held trigger pulls TL low"
            );

            sys.set_light_phaser_trigger(port, false);
            assert_eq!(sys.io_read(io_port) & (1 << bit), 1 << bit);
        }
    }

    /// A trigger with no gun behind it does nothing: the bit belongs to
    /// whatever is plugged into the port, and with nothing there it stays high.
    #[test]
    fn a_trigger_without_a_gun_does_not_pull_the_pin() {
        let mut sys = phaser_screen_of(0x3F);
        sys.set_light_phaser_trigger(1, true);
        assert_eq!(sys.io_read(0xDC) & 0x10, 0x10);
    }

    /// Both ports work, and independently.
    #[test]
    fn each_port_has_its_own_gun() {
        let mut sys = phaser_screen_of(0x3F);
        phaser_aim(&mut sys, 2, 128, 96);
        sys.set_light_phaser_trigger(2, true);
        phaser_run_frames(&mut sys, 2);

        assert_ne!(phaser_latched(&mut sys), 0, "the gun in port 2 answers");
        assert_eq!(sys.io_read(0xDD) & 0x04, 0, "and its trigger is port 2's");
        assert_eq!(
            sys.io_read(0xDC) & 0x10,
            0x10,
            "port 1 is empty and unaffected"
        );
    }
}

/// One captured I/O port access, for the debug trace.
#[derive(Debug, Clone, Copy)]
pub struct IoEvent {
    /// CPU program counter at the time of the access.
    pub pc: u16,
    /// I/O port (low 8 bits of the address bus).
    pub port: u8,
    /// Byte written, or byte returned on a read.
    pub value: u8,
    /// `true` for `OUT`, `false` for `IN`.
    pub write: bool,
}

impl Sms {
    /// Write one byte through the bus (RAM accepts it; ROM ignores it).
    pub fn poke(&mut self, addr: u16, value: u8) {
        self.mem_write(addr, value);
    }

    /// Start (or restart) the I/O port-access trace.
    pub fn start_io_trace(&mut self) {
        self.io_trace = Some(Vec::new());
    }

    /// Stop tracing and return the captured I/O events.
    pub fn take_io_trace(&mut self) -> Vec<IoEvent> {
        self.io_trace.take().unwrap_or_default()
    }
}
