//! Shared ColecoVision wiring, statically bound by each concrete board.
//!
//! Fresh-write against the workspace pin-driven bus pattern (RULES.md
//! rule 6: no Bus trait, no callback into the CPU). The donor at
//! `Emu198x-Oldest/crates/machine-coleco-colecovision` used the
//! deprecated `emu_core::Bus` callback and could not port directly; this
//! file uses it as a system spec — memory map, I/O port routing, keypad
//! multiplex, clock ratios — but the wiring is written against
//! [`emu198x_zilog_z80::Z80`]'s public pin fields and `bus_request()` collapse,
//! the same pattern used by `machine-sinclair-zx-spectrum-48k` and
//! `machine-nintendo-nes`.
//!
//! # Hardware
//!
//! - **CPU:** Z80A @ 3.579545 MHz (NTSC)
//! - **VDP:** TMS9918-family register model (16 KB VRAM)
//! - **PSG:** SN76489AN
//! - **RAM:** 1 KB at `$6000-$63FF`, mirrored through `$7FFF`
//! - **BIOS:** 8 KB ROM at `$0000-$1FFF` (required)
//! - **Cartridge:** up to 32 KB at `$8000-$FFFF`
//!
//! # Memory map
//!
//! | Range         | Contents                       |
//! |---------------|--------------------------------|
//! | `$0000-$1FFF` | BIOS ROM                       |
//! | `$2000-$5FFF` | Expansion / unmapped (`0xFF`)  |
//! | `$6000-$7FFF` | 1 KB RAM, mirrored every 1 KB  |
//! | `$8000-$FFFF` | Cartridge ROM (up to 32 KB)    |
//!
//! # I/O map
//!
//! | Port range     | R/W   | Function                                 |
//! |----------------|-------|------------------------------------------|
//! | `$80-$9F`      | write | Select **keypad** controller mode        |
//! | `$A0-$BF` even | r/w   | VDP data port                            |
//! | `$A0-$BF` odd  | r/w   | VDP control / status port                |
//! | `$C0-$DF`      | write | Select **joystick** controller mode      |
//! | `$E0-$FF`      | write | SN76489 PSG                              |
//! | `$E0-$FF`      | read  | Controller (bit 1 selects 1 vs 2)        |
//!
//! # Accuracy
//!
//! Uses the correct **3:2 VDP-dot-to-CPU-T-state phase clock**: the
//! real CV crystal is 10.738635 MHz with the CPU at ÷3 and the VDP dot
//! clock at ÷2, so the VDP advances 3 dots per 2 T-states. This
//! replaced the donor's flat 3:1 tick model (`VDP_DOTS_PER_CPU = 3`),
//! which ran the VDP and wall-clock 1.5× too fast. Same model as the
//! SG-1000 (identical TMS9918A + Z80 at the same clocks).

use emu198x_ti_sn76489::{NoiseLfsr, Sn76489};
pub use emu198x_ti_tms9918::{Tms9918, VdpRegion};
use emu198x_z80_cadence::Z80Machine;
use emu198x_zilog_z80::BusOp;
pub use emu198x_zilog_z80::Z80;
use serde::{Deserialize, Serialize};
use serde_big_array::BigArray;

/// VDP dot ticks per CPU T-state, numerator. The real ColecoVision
/// crystal is 10.738635 MHz with the CPU at ÷3 (3.579545 MHz) and the
/// VDP dot clock at ÷2 (5.369318 MHz), so the VDP runs **3 dots per 2
/// CPU T-states** — the same 3:2 phase as the SG-1000 (identical
/// TMS9918A + Z80 at the same clocks). Replaces the donor's flat 3:1
/// approximation, which ran both the VDP and wall-clock 1.5× fast.
///
/// Accumulated per CPU **half-cycle**, so the denominator is 4 rather than
/// 2: three dots per four half-cycles is the same 3:2 against T-states,
/// interleaved twice as finely.
const VDP_DOT_PHASE_NUMERATOR: u32 = 3;
/// VDP dot ticks per CPU half-cycle, denominator.
const VDP_DOT_PHASE_DENOMINATOR: u32 = 4;

/// CPU T-states per scanline (342 VDP dots × 2 / 3).
#[cfg(test)]
const CPU_TSTATES_PER_SCANLINE: u64 = 228;

/// CPU T-states per NTSC frame (228 × 262 lines).
#[cfg(test)]
const NTSC_CPU_CYCLES_PER_FRAME: u64 = CPU_TSTATES_PER_SCANLINE * 262;

/// CPU T-states per PAL frame (228 × 313 lines).
#[cfg(test)]
const PAL_CPU_CYCLES_PER_FRAME: u64 = CPU_TSTATES_PER_SCANLINE * 313;

/// ColecoVision region.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CvRegion {
    Ntsc,
    Pal,
}

/// Numeric keypad keys (12-key keypad — 0-9, *, #).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum KeypadKey {
    K0,
    K1,
    K2,
    K3,
    K4,
    K5,
    K6,
    K7,
    K8,
    K9,
    Star,
    Hash,
}

impl KeypadKey {
    /// Active-low keypad encoding (bits 3-0 of the keypad read).
    fn encode(self) -> u8 {
        match self {
            Self::K0 => 0x0A,
            Self::K1 => 0x0D,
            Self::K2 => 0x07,
            Self::K3 => 0x0C,
            Self::K4 => 0x02,
            Self::K5 => 0x03,
            Self::K6 => 0x0E,
            Self::K7 => 0x05,
            Self::K8 => 0x01,
            Self::K9 => 0x0B,
            Self::Star => 0x09,
            Self::Hash => 0x06,
        }
    }
}

/// Controller state for one player.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct CvController {
    pub up: bool,
    pub down: bool,
    pub left: bool,
    pub right: bool,
    pub left_button: bool,
    pub right_button: bool,
    pub keypad: Option<KeypadKey>,
}

impl CvController {
    /// Read in joystick mode (active-low: 0 = pressed).
    fn read_joystick(&self) -> u8 {
        let mut val = 0xFF;
        if self.left {
            val &= !0x01;
        }
        if self.down {
            val &= !0x02;
        }
        if self.right {
            val &= !0x04;
        }
        if self.up {
            val &= !0x08;
        }
        if self.left_button {
            val &= !0x40;
        }
        val
    }

    /// Read in keypad mode (active-low: 0 = pressed).
    fn read_keypad(&self) -> u8 {
        let mut val: u8 = 0x70;
        if let Some(key) = self.keypad {
            val = (val & 0xF0) | key.encode();
        } else {
            val |= 0x0F;
        }
        if self.right_button {
            val &= !0x40;
        }
        val
    }
}

/// ColecoVision machine.
///
/// Fully serialisable for save-states: the Z80, the TMS9918 VDP, the SN76489
/// PSG, BIOS, cart ROM, and RAM all carry live state. `io_trace` is a host-side
/// debug buffer, not machine state, so it is skipped and defaults on restore.
#[derive(Serialize, Deserialize)]
pub struct CvState {
    cpu: Z80,
    vdp: Tms9918,
    psg: Sn76489,
    bios: Vec<u8>,
    cart_rom: Vec<u8>,
    #[serde(with = "BigArray")]
    ram: [u8; 1024],
    controller1: CvController,
    controller2: CvController,
    /// True when controllers are in joystick mode, false for keypad mode.
    joystick_mode: bool,
    /// Region.
    region: CvRegion,
    /// CPU T-state counter.
    cpu_cycles: u64,
    /// The cadence driver's half-cycle phase; a restore lands on a T-state
    /// boundary, so it is not part of the snapshot.
    #[serde(skip)]
    cadence_hc: u64,
    /// CPU T-states per frame for the active region.
    cpu_cycles_per_frame: u64,
    /// VDP dot-phase accumulator (numerator units); ticks the VDP 3
    /// dots per 2 T-states.
    vdp_phase: u32,
    /// Frame counter.
    frame_count: u64,
    /// Transient observation for a bounded native-frame request.
    #[serde(skip)]
    native_frame_ready: bool,
    /// When `Some`, every I/O port access is appended here (debug trace).
    #[serde(skip)]
    io_trace: Option<Vec<IoEvent>>,
}

/// Fixed wiring supplied by a concrete machine crate; no model switch runs here.
pub trait Board {
    const REGION: CvRegion;
    const VDP_REGION: VdpRegion;
    const PSG_CLOCK_HZ: u32;
    const TSTATES_PER_FRAME: u64;
    const FRAMEBUFFER_HEIGHT: u32;
}
/// Shared wiring engine, statically bound to one board.
pub struct CvCore<B: Board> {
    state: CvState,
    board: std::marker::PhantomData<B>,
}
impl CvState {
    #[must_use]
    pub fn region(&self) -> CvRegion {
        self.region
    }
}
impl<B: Board> CvCore<B> {
    /// Validate an existing state without reinitialising any chip.
    pub fn from_state(state: CvState) -> Result<Self, &'static str> {
        if state.region != B::REGION
            || state.cpu_cycles_per_frame != B::TSTATES_PER_FRAME
            || state.vdp.framebuffer_height() != B::FRAMEBUFFER_HEIGHT
        {
            return Err("snapshot video/region does not match this ColecoVision board");
        }
        Ok(Self {
            state,
            board: std::marker::PhantomData,
        })
    }
}
impl<B: Board> Serialize for CvCore<B> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.state.serialize(serializer)
    }
}
impl<'de, B: Board> Deserialize<'de> for CvCore<B> {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::from_state(CvState::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}

impl<B: Board> CvCore<B> {
    /// Original cartridge image retained by the live machine.
    #[must_use]
    pub fn cartridge_rom(&self) -> &[u8] {
        &self.state.cart_rom
    }

    /// Original firmware image retained by the live machine.
    #[must_use]
    pub fn firmware_rom(&self) -> &[u8] {
        &self.state.bios
    }

    /// Create a new ColecoVision with the given BIOS and cartridge ROM.
    #[must_use]
    pub fn new(bios: Vec<u8>, cart_rom: Vec<u8>) -> Self {
        let state = CvState {
            cpu: Z80::new(),
            vdp: Tms9918::new(B::VDP_REGION),
            psg: Sn76489::new(B::PSG_CLOCK_HZ, NoiseLfsr::Tms15),
            bios,
            cart_rom,
            ram: [0; 1024],
            controller1: CvController::default(),
            controller2: CvController::default(),
            joystick_mode: false,
            region: B::REGION,
            cpu_cycles: 0,
            cadence_hc: 0,
            cpu_cycles_per_frame: B::TSTATES_PER_FRAME,
            vdp_phase: 0,
            frame_count: 0,
            native_frame_ready: false,
            io_trace: None,
        };
        Self {
            state,
            board: std::marker::PhantomData,
        }
    }

    /// Run one frame and return the number of CPU cycles consumed.
    /// Stop on the video chip's next raster wrap, not a rounded CPU budget.
    /// The two-field safety budget reports false rather than inventing a frame.
    pub fn run_native_frame(&mut self) -> (u64, bool) {
        let start = self.state.cpu_cycles;
        self.state.native_frame_ready = false;
        while !self.state.native_frame_ready
            && self.state.cpu_cycles - start < 2 * self.state.cpu_cycles_per_frame
        {
            self.tick_cpu_cycle();
        }
        if self.state.native_frame_ready {
            self.state.frame_count += 1;
        }
        (self.state.cpu_cycles - start, self.state.native_frame_ready)
    }

    pub fn run_frame(&mut self) -> u64 {
        let target = self.state.cpu_cycles + self.state.cpu_cycles_per_frame;
        while self.state.cpu_cycles < target {
            self.tick_cpu_cycle();
        }
        self.state.frame_count += 1;
        self.state.cpu_cycles_per_frame
    }

    /// Advance one Z80 T-state and the chips it drives. The cadence — two
    /// CPU half-cycles, the interrupt pin fed before each — is
    /// `emu198x-z80-cadence`'s; this machine only orders its own chips
    /// around it (the VDP after each CPU edge, the PSG and T-state counter
    /// once per T-state, as the hand-rolled loop always had them).
    fn tick_cpu_cycle(&mut self) {
        self.advance_tstates(1);
    }

    /// Inspect the Z80's bus request and route memory / I/O.
    fn handle_bus(&mut self) {
        match self.state.cpu.bus_request() {
            Some(BusOp::MemRead) => {
                self.state.cpu.data_in = self.mem_read(self.state.cpu.addr);
            }
            Some(BusOp::MemWrite) => {
                self.mem_write(self.state.cpu.addr, self.state.cpu.data);
            }
            Some(BusOp::IoRead) => {
                let io_port = (self.state.cpu.addr & 0xFF) as u8;
                let io_pc = self.state.cpu.regs.pc;
                let io_val = self.io_read(self.state.cpu.addr);
                self.state.cpu.data_in = io_val;
                if let Some(trace) = &mut self.state.io_trace {
                    trace.push(IoEvent {
                        pc: io_pc,
                        port: io_port,
                        value: io_val,
                        write: false,
                    });
                }
            }
            Some(BusOp::IoWrite) => {
                if let Some(trace) = &mut self.state.io_trace {
                    trace.push(IoEvent {
                        pc: self.state.cpu.regs.pc,
                        port: (self.state.cpu.addr & 0xFF) as u8,
                        value: self.state.cpu.data,
                        write: true,
                    });
                }
                self.io_write(self.state.cpu.addr, self.state.cpu.data);
            }
            Some(BusOp::IntAck) => {
                // ColecoVision uses IM 1 — the BIOS sets IM 1 during
                // init and the VDP INT does not drive a vector; the
                // Z80 fetches `RST 38h` via the floating bus.
                self.state.cpu.data_in = 0xFF;
            }
            None => {}
        }
    }

    fn mem_read(&self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x1FFF => {
                let idx = addr as usize;
                if idx < self.state.bios.len() {
                    self.state.bios[idx]
                } else {
                    0xFF
                }
            }
            0x2000..=0x5FFF => 0xFF,
            0x6000..=0x7FFF => self.state.ram[(addr & 0x03FF) as usize],
            0x8000..=0xFFFF => {
                let idx = (addr - 0x8000) as usize;
                if idx < self.state.cart_rom.len() {
                    self.state.cart_rom[idx]
                } else {
                    0xFF
                }
            }
        }
    }

    fn mem_write(&mut self, addr: u16, data: u8) {
        if (0x6000..=0x7FFF).contains(&addr) {
            self.state.ram[(addr & 0x03FF) as usize] = data;
        }
    }

    fn io_read(&mut self, port: u16) -> u8 {
        let p = port as u8;
        match p {
            // VDP data read (even ports in $A0-$BF).
            0xA0..=0xBF if p & 1 == 0 => self.state.vdp.read_data(),
            // VDP status read (odd ports in $A0-$BF).
            0xA0..=0xBF => self.state.vdp.read_status(),
            // Controllers in $E0-$FF — bit 1 of port selects controller.
            0xE0..=0xFF if p & 0x02 == 0 => {
                if self.state.joystick_mode {
                    self.state.controller1.read_joystick()
                } else {
                    self.state.controller1.read_keypad()
                }
            }
            0xE0..=0xFF => {
                if self.state.joystick_mode {
                    self.state.controller2.read_joystick()
                } else {
                    self.state.controller2.read_keypad()
                }
            }
            _ => 0xFF,
        }
    }

    fn io_write(&mut self, port: u16, data: u8) {
        let p = port as u8;
        match p {
            0x80..=0x9F => self.state.joystick_mode = false,
            0xA0..=0xBF if p & 1 == 0 => self.state.vdp.write_data(data),
            0xA0..=0xBF => self.state.vdp.write_control(data),
            0xC0..=0xDF => self.state.joystick_mode = true,
            0xE0..=0xFF => self.state.psg.write(data),
            _ => {}
        }
    }

    /// Framebuffer (ARGB32) — TMS9918A active display plus canonical
    /// TV-visible border.
    #[must_use]
    pub fn framebuffer(&self) -> &[u32] {
        self.state.vdp.framebuffer()
    }

    #[must_use]
    pub fn framebuffer_width(&self) -> u32 {
        self.state.vdp.framebuffer_width()
    }

    #[must_use]
    pub fn framebuffer_height(&self) -> u32 {
        self.state.vdp.framebuffer_height()
    }

    /// Take the accumulated mono PSG audio buffer (f32, 48 kHz).
    pub fn take_audio_buffer(&mut self) -> Vec<f32> {
        self.state.psg.take_buffer()
    }

    /// Mutable access to controller 1.
    pub fn controller1_mut(&mut self) -> &mut CvController {
        &mut self.state.controller1
    }

    /// Mutable access to controller 2.
    pub fn controller2_mut(&mut self) -> &mut CvController {
        &mut self.state.controller2
    }

    /// CPU reference.
    #[must_use]
    pub fn cpu(&self) -> &Z80 {
        &self.state.cpu
    }

    /// CPU mutable reference.
    pub fn cpu_mut(&mut self) -> &mut Z80 {
        &mut self.state.cpu
    }

    /// VDP reference.
    #[must_use]
    pub fn vdp(&self) -> &Tms9918 {
        &self.state.vdp
    }

    /// Region.
    #[must_use]
    pub fn region(&self) -> CvRegion {
        self.state.region
    }

    /// Total CPU cycles executed since power-on.
    #[must_use]
    pub fn cpu_cycles(&self) -> u64 {
        self.state.cpu_cycles
    }

    /// Frame count since power-on.
    #[must_use]
    pub fn frame_count(&self) -> u64 {
        self.state.frame_count
    }

    /// Observe one byte on the Z80 bus without side effects (BIOS
    /// / cartridge / RAM read with the chip-side decode). Exposed
    /// for host debugging tools (`memory_read` MCP, watch points).
    #[must_use]
    pub fn peek(&self, addr: u16) -> u8 {
        self.mem_read(addr)
    }
}

impl<B: Board> Z80Machine for CvCore<B> {
    fn hc(&self) -> u64 {
        self.state.cadence_hc
    }

    fn hc_mut(&mut self) -> &mut u64 {
        &mut self.state.cadence_hc
    }

    // Per RULES.md rule 6 we drive the Z80 by inspecting its pins.
    fn feed_interrupt_pins(&mut self) {
        // VDP INT pin drives the Z80 IRQ pin directly, fed before the tick
        // rather than after. The Z80 samples `/INT` at an instruction
        // boundary during its own tick, so setting the line afterwards
        // hands it the VDP's state from the previous half-cycle.
        self.state.cpu.irq = self.state.vdp.interrupt;
    }

    fn tick_cpu_and_bus(&mut self) {
        self.state.cpu.tick();
        self.handle_bus();
    }

    fn tick_chips_halfcycle(&mut self) {
        // VDP advances by 3/2 dots per T-state — phase counter, accumulated
        // per half-cycle.
        self.state.vdp_phase += VDP_DOT_PHASE_NUMERATOR;
        while self.state.vdp_phase >= VDP_DOT_PHASE_DENOMINATOR {
            self.state.native_frame_ready |= self.state.vdp.tick();
            self.state.vdp_phase -= VDP_DOT_PHASE_DENOMINATOR;
        }
    }

    fn tick_chips(&mut self) {
        // PSG advances one CPU clock (internal ÷ 16 divider).
        self.state.psg.tick();

        self.state.cpu_cycles += 1;
    }
}

impl<B: Board> emu198x_zilog_z80::Z80Stepper for CvCore<B> {
    fn z80_instructions_retired(&self) -> u64 {
        self.state.cpu.instructions_retired()
    }

    fn step_tick(&mut self) {
        self.tick_cpu_cycle();
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

impl<B: Board> CvCore<B> {
    /// Write one byte through the bus (RAM accepts it; ROM ignores it).
    pub fn poke(&mut self, addr: u16, value: u8) {
        self.mem_write(addr, value);
    }

    /// Start (or restart) the I/O port-access trace.
    pub fn start_io_trace(&mut self) {
        self.state.io_trace = Some(Vec::new());
    }

    /// Stop tracing and return the captured I/O events.
    pub fn take_io_trace(&mut self) -> Vec<IoEvent> {
        self.state.io_trace.take().unwrap_or_default()
    }
}

#[cfg(test)]
mod tests;
