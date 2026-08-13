//! Sega SG-1000 / SC-3000 machine wiring.
//!
//! Fresh-write against the workspace pin-driven bus pattern (RULES.md
//! rule 6). The donor at `Emu198x-Oldest/crates/machine-sega-sg-1000`
//! used the deprecated `emu_core::Bus` callback and could not port
//! directly; this file uses it as a system spec — memory map, I/O
//! port routing, controller layout, **3:2 VDP-phase clock model** —
//! but the wiring is written against [`zilog_z80::Z80`]'s public pin
//! fields and `bus_request()` collapse.
//!
//! # Hardware
//!
//! - **CPU:** Z80A @ 3.579545 MHz (NTSC) / 3.546893 MHz (PAL)
//! - **VDP:** TMS9918A (16 KB VRAM)
//! - **PSG:** SN76489A
//! - **RAM:** 1 KB at `$C000-$C3FF`, mirrored through `$FFFF` (SG-1000
//!   has 1 KB; SC-3000 expanded to 2 KB but the SG-1000 cartridge
//!   target is 1 KB)
//! - **Cartridge:** up to 48 KB at `$0000-$BFFF` — **no BIOS**, the
//!   cart is the reset vector
//!
//! # Memory map
//!
//! | Range         | Contents                              |
//! |---------------|---------------------------------------|
//! | `$0000-$BFFF` | Cartridge ROM (up to 48 KB)           |
//! | `$C000-$FFFF` | 1 KB RAM, mirrored every 1 KB         |
//!
//! # I/O map
//!
//! | Port range    | R/W   | Function                              |
//! |---------------|-------|---------------------------------------|
//! | `$40-$7F`     | write | SN76489 PSG                           |
//! | `$80-$BF` even| r/w   | VDP data port                         |
//! | `$80-$BF` odd | r/w   | VDP control / status port             |
//! | `$C0-$FF` even| read  | Controller 1 (active-low buttons)     |
//! | `$C0-$FF` odd | read  | Controller 2 (active-low buttons)     |
//!
//! # Pause / NMI
//!
//! The SG-1000 console has no front-panel pause button. The SC-3000
//! keyboard variant routes a key press into the Z80's NMI line. This
//! crate exposes [`Sg1000::set_pause_pressed`] for hosts that want to
//! simulate the SC-3000 path; the line is held until released.
//!
//! # Clock model
//!
//! Donor uses 3:2 VDP-dot-to-CPU-T-state phase counter (the correct
//! ratio for the master crystal divided onto a 3.579545 MHz CPU and
//! a 5.369 MHz VDP dot clock). One iteration of [`Sg1000::run_frame`]
//! corresponds to one Z80 T-state; per iteration the phase counter
//! advances by 3 and yields one VDP dot whenever it reaches 2.

use serde::{Deserialize, Serialize};
use serde_big_array::BigArray;
use ti_sn76489::{NoiseLfsr, Sn76489};
use ti_tms9918::{Tms9918, VdpRegion};
use zilog_z80::{BusOp, Z80};

/// VDP dot ticks per CPU T-state, numerator.
const VDP_DOT_PHASE_NUMERATOR: u32 = 3;
/// VDP dot ticks per CPU T-state, denominator.
// Accumulated per CPU **half-cycle**, so the denominator is 4 rather
// than 2: three dots per four half-cycles is the same 3:2 against
// T-states, interleaved twice as finely.
const VDP_DOT_PHASE_DENOMINATOR: u32 = 4;

/// CPU T-states per scanline (342 VDP dots × 2 / 3).
const CPU_TSTATES_PER_SCANLINE: u64 = 228;
const NTSC_SCANLINES_PER_FRAME: u64 = 262;
const PAL_SCANLINES_PER_FRAME: u64 = 313;
const NTSC_TSTATES_PER_FRAME: u64 = CPU_TSTATES_PER_SCANLINE * NTSC_SCANLINES_PER_FRAME;
const PAL_TSTATES_PER_FRAME: u64 = CPU_TSTATES_PER_SCANLINE * PAL_SCANLINES_PER_FRAME;

const NTSC_PSG_CLOCK_HZ: u32 = 3_579_545;
const PAL_PSG_CLOCK_HZ: u32 = 3_546_893;

/// SG-1000 region.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Sg1000Region {
    Ntsc,
    Pal,
}

/// SG-1000 controller — direction pad + two buttons.
#[derive(Debug, Default, Clone, Copy, Serialize, Deserialize)]
pub struct ControllerState {
    pub up: bool,
    pub down: bool,
    pub left: bool,
    pub right: bool,
    pub button1: bool,
    pub button2: bool,
}

impl ControllerState {
    /// Active-low byte (1 = not pressed).
    fn read_port(&self) -> u8 {
        let mut val = 0xFF;
        if self.up {
            val &= !0x01;
        }
        if self.down {
            val &= !0x02;
        }
        if self.left {
            val &= !0x04;
        }
        if self.right {
            val &= !0x08;
        }
        if self.button1 {
            val &= !0x10;
        }
        if self.button2 {
            val &= !0x20;
        }
        val
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

/// SG-1000 / SC-3000 machine.
///
/// Fully serialisable for save-states: the Z80, the TMS9918 VDP, the SN76489
/// PSG, cart ROM, and RAM all carry live state. `io_trace` is a host-side debug
/// buffer, not machine state, so it is skipped and defaults on restore.
#[derive(Serialize, Deserialize)]
pub struct Sg1000 {
    cpu: Z80,
    vdp: Tms9918,
    psg: Sn76489,
    cart_rom: Vec<u8>,
    #[serde(with = "BigArray")]
    ram: [u8; 1024],
    controller1: ControllerState,
    controller2: ControllerState,
    pause_pressed: bool,
    region: Sg1000Region,
    /// CPU T-state counter.
    cpu_tstates: u64,
    /// T-states per frame for the active region.
    tstates_per_frame: u64,
    /// VDP dot phase accumulator (numerator units).
    vdp_phase: u32,
    /// Frame counter.
    frame_count: u64,
    /// When `Some`, every I/O port access is appended here (debug trace).
    #[serde(skip)]
    io_trace: Option<Vec<IoEvent>>,
}

impl Sg1000 {
    /// Create a new SG-1000 with the given cartridge ROM.
    #[must_use]
    pub fn new(cart_rom: Vec<u8>, region: Sg1000Region) -> Self {
        let vdp_region = match region {
            Sg1000Region::Ntsc => VdpRegion::Ntsc,
            Sg1000Region::Pal => VdpRegion::Pal,
        };
        let psg_clock_hz = match region {
            Sg1000Region::Ntsc => NTSC_PSG_CLOCK_HZ,
            Sg1000Region::Pal => PAL_PSG_CLOCK_HZ,
        };
        let tstates_per_frame = match region {
            Sg1000Region::Ntsc => NTSC_TSTATES_PER_FRAME,
            Sg1000Region::Pal => PAL_TSTATES_PER_FRAME,
        };
        Self {
            cpu: Z80::new(),
            vdp: Tms9918::new(vdp_region),
            psg: Sn76489::new(psg_clock_hz, NoiseLfsr::Tms15),
            cart_rom,
            ram: [0; 1024],
            controller1: ControllerState::default(),
            controller2: ControllerState::default(),
            pause_pressed: false,
            region,
            cpu_tstates: 0,
            tstates_per_frame,
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

    /// Advance one Z80 T-state and the chips it drives.
    fn tick_tstate(&mut self) {
        // Two CPU half-cycles per T-state. `Z80::tick` advances one
        // half-cycle, and this loop used to call it once while counting a
        // whole T-state — the comment even said "CPU half-cycle tick"
        // next to `cpu_tstates += 1`. A `NOP` cost 8 T-states against the
        // Z80's 4, so the machine executed half the work per frame that
        // 228 T-states per scanline budgets for.
        for _ in 0..2 {
            // 1. Pins before the tick, not after. The Z80 samples `/INT`
            //    at an instruction boundary during its own tick, so
            //    feeding the line afterwards hands it the VDP's state
            //    from the previous half-cycle. See
            //    `knowledge/decisions/zilog-z80-samples-int-at-the-instruction-boundary.md`.
            self.cpu.irq = self.vdp.interrupt;
            // 2. Pause line → Z80 NMI (level-driven; the host releases).
            self.cpu.nmi = self.pause_pressed;

            // 3. CPU half-cycle tick + pin-driven bus inspection.
            self.cpu.tick();
            self.handle_bus();

            // 4. VDP advances by 3/4 dots per half-cycle — phase counter.
            self.vdp_phase += VDP_DOT_PHASE_NUMERATOR;
            while self.vdp_phase >= VDP_DOT_PHASE_DENOMINATOR {
                self.vdp.tick();
                self.vdp_phase -= VDP_DOT_PHASE_DENOMINATOR;
            }
        }

        // 5. PSG runs at the Z80 clock on SG-1000.
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
                self.cpu.data_in = self.io_read(self.cpu.addr);
            }
            Some(BusOp::IoWrite) => {
                self.io_write(self.cpu.addr, self.cpu.data);
            }
            Some(BusOp::IntAck) => {
                // SG-1000 cartridges set IM 1 — INT fetches RST 38h via
                // floating bus. No external IM 2 vector hardware.
                self.cpu.data_in = 0xFF;
            }
            None => {}
        }
    }

    fn mem_read(&self, addr: u16) -> u8 {
        match addr {
            0x0000..=0xBFFF => {
                let idx = addr as usize;
                if idx < self.cart_rom.len() {
                    self.cart_rom[idx]
                } else {
                    0xFF
                }
            }
            0xC000..=0xFFFF => self.ram[(addr & 0x03FF) as usize],
        }
    }

    fn mem_write(&mut self, addr: u16, data: u8) {
        if (0xC000..=0xFFFF).contains(&addr) {
            self.ram[(addr & 0x03FF) as usize] = data;
        }
    }

    fn io_read(&mut self, port: u16) -> u8 {
        let pc = self.cpu.regs.pc;
        let p = port as u8;
        let value = match p {
            0x80..=0xBF if p & 1 == 0 => self.vdp.read_data(),
            0x80..=0xBF => self.vdp.read_status(),
            0xC0..=0xFF if p & 1 == 0 => self.controller1.read_port(),
            0xC0..=0xFF => self.controller2.read_port(),
            _ => 0xFF,
        };
        if let Some(trace) = &mut self.io_trace {
            trace.push(IoEvent {
                pc,
                port: p,
                value,
                write: false,
            });
        }
        value
    }

    fn io_write(&mut self, port: u16, data: u8) {
        let p = port as u8;
        if let Some(trace) = &mut self.io_trace {
            trace.push(IoEvent {
                pc: self.cpu.regs.pc,
                port: p,
                value: data,
                write: true,
            });
        }
        match p {
            0x40..=0x7F => self.psg.write(data),
            0x80..=0xBF if p & 1 == 0 => self.vdp.write_data(data),
            0x80..=0xBF => self.vdp.write_control(data),
            _ => {}
        }
    }

    /// Framebuffer (ARGB32) — TMS9918A active display plus canonical
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

    /// Take the accumulated mono PSG audio buffer (f32, 48 kHz).
    pub fn take_audio_buffer(&mut self) -> Vec<f32> {
        self.psg.take_buffer()
    }

    /// Mutable access to controller 1.
    pub fn controller1_mut(&mut self) -> &mut ControllerState {
        &mut self.controller1
    }

    /// Mutable access to controller 2.
    pub fn controller2_mut(&mut self) -> &mut ControllerState {
        &mut self.controller2
    }

    /// Pause / SC-3000 NMI line (level-driven, host-controlled).
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
    pub fn vdp(&self) -> &Tms9918 {
        &self.vdp
    }

    /// Region.
    #[must_use]
    pub fn region(&self) -> Sg1000Region {
        self.region
    }

    /// Total T-states executed since power-on.
    #[must_use]
    pub fn cpu_tstates(&self) -> u64 {
        self.cpu_tstates
    }

    /// Observe one byte on the Z80 bus without side effects.
    /// Resolves cartridge / RAM via the standard SG-1000 memory map.
    #[must_use]
    pub fn peek(&self, addr: u16) -> u8 {
        self.mem_read(addr)
    }

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

    /// Frame count since power-on.
    #[must_use]
    pub fn frame_count(&self) -> u64 {
        self.frame_count
    }
}

impl zilog_z80::Z80Stepper for Sg1000 {
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

    /// Save-state must capture LIVE machine state (Z80 + TMS9918 VDP + SN76489
    /// PSG + RAM), not cold-boot from the cart. Serialise, advance (so the state
    /// differs), then deserialise the first snapshot and confirm re-serialising
    /// it is byte-identical — every stateful field across all three chips
    /// round-trips, including the VDP's 16 KB VRAM.
    #[test]
    fn snapshot_round_trips_live_state() {
        let mut sys = Sg1000::new(trap_cart(), Sg1000Region::Ntsc);
        sys.run_frame();
        sys.poke(0xC100, 0xA5); // a work-RAM byte to carry across the snapshot
        sys.run_frame();
        let s1 = postcard::to_allocvec(&sys).expect("encode snapshot");

        sys.run_frame(); // advance past the snapshot point
        let s2 = postcard::to_allocvec(&sys).expect("encode again");
        assert_ne!(s1, s2, "running a frame should change the serialised state");

        let restored: Sg1000 = postcard::from_bytes(&s1).expect("decode snapshot");
        let s3 = postcard::to_allocvec(&restored).expect("re-encode restored");
        assert_eq!(
            s1, s3,
            "restore should reproduce the snapshot state exactly"
        );
    }

    fn trap_cart() -> Vec<u8> {
        // 48 KB cart full of NOPs with a JR -2 trap at $0008 — gives
        // the Z80 something legal to run for boot-smoke tests that
        // don't load real software.
        let mut cart = vec![0u8; 0xC000];
        cart[0x0008] = 0x18;
        cart[0x0009] = 0xFE;
        cart
    }

    #[test]
    fn ntsc_frame_returns_expected_tstates() {
        let mut sys = Sg1000::new(trap_cart(), Sg1000Region::Ntsc);
        let t = sys.run_frame();
        assert_eq!(t, NTSC_TSTATES_PER_FRAME);
        assert_eq!(sys.frame_count(), 1);
    }

    #[test]
    fn pal_frame_returns_expected_tstates() {
        let mut sys = Sg1000::new(trap_cart(), Sg1000Region::Pal);
        let t = sys.run_frame();
        assert_eq!(t, PAL_TSTATES_PER_FRAME);
    }

    #[test]
    fn vdp_dot_ratio_is_three_per_two_tstates() {
        // After exactly 2 T-states we expect 3 VDP dots ticked.
        // Use the phase counter directly: each T-state adds 3 to phase,
        // each dot consumes 2. Run 2 T-states → 6 added, 3 dots taken.
        let mut sys = Sg1000::new(trap_cart(), Sg1000Region::Ntsc);
        let start = sys.vdp.scanline();
        // Manually tick 4 T-states; expect 6 VDP dots which is well
        // within the same scanline at start (342 dots per line). The
        // VDP scanline shouldn't change yet, but the phase counter
        // should hold the running fractional remainder.
        for _ in 0..4 {
            sys.tick_tstate();
        }
        assert_eq!(sys.vdp.scanline(), start);
        // 4 × 3 = 12 phase units, 12 / 2 = 6 dots, 12 % 2 = 0 leftover.
        assert_eq!(sys.vdp_phase, 0);
    }

    #[test]
    fn pause_line_drives_z80_nmi() {
        let mut sys = Sg1000::new(trap_cart(), Sg1000Region::Ntsc);
        sys.set_pause_pressed(true);
        sys.tick_tstate();
        assert!(sys.cpu.nmi);
        sys.set_pause_pressed(false);
        sys.tick_tstate();
        assert!(!sys.cpu.nmi);
    }

    #[test]
    fn controller_routing_by_port_parity() {
        let mut sys = Sg1000::new(trap_cart(), Sg1000Region::Ntsc);
        sys.controller1.button1 = true;
        sys.controller2.button2 = true;
        // Even port → controller 1.
        let c1 = sys.io_read(0xC0);
        // Odd port → controller 2.
        let c2 = sys.io_read(0xC1);
        // Button 1 = bit 4, active-low.
        assert_eq!(c1 & 0x10, 0);
        // Button 2 = bit 5, active-low.
        assert_eq!(c2 & 0x20, 0);
        // Cross-check: c1's button2 bit (controller 1) is unset (1).
        assert_eq!(c1 & 0x20, 0x20);
    }

    #[test]
    fn psg_write_port_range() {
        let mut sys = Sg1000::new(trap_cart(), Sg1000Region::Ntsc);
        // Write to $40 — should land on the PSG. Cheapest test: it
        // doesn't panic. PSG state inspection is the PSG crate's job.
        sys.io_write(0x40, 0x80);
        sys.io_write(0x7F, 0xFF);
    }

    #[test]
    fn many_frames_complete_without_panic() {
        let mut sys = Sg1000::new(trap_cart(), Sg1000Region::Ntsc);
        for _ in 0..60 {
            sys.run_frame();
        }
        assert_eq!(sys.frame_count(), 60);
    }
}
