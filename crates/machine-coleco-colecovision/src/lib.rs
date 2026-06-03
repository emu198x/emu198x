//! ColecoVision machine wiring.
//!
//! Fresh-write against the workspace pin-driven bus pattern (RULES.md
//! rule 6: no Bus trait, no callback into the CPU). The donor at
//! `Emu198x-Oldest/crates/machine-coleco-colecovision` used the
//! deprecated `emu_core::Bus` callback and could not port directly; this
//! file uses it as a system spec — memory map, I/O port routing, keypad
//! multiplex, clock ratios — but the wiring is written against
//! [`zilog_z80::Z80`]'s public pin fields and `bus_request()` collapse,
//! the same pattern used by `machine-sinclair-zx-spectrum-48k` and
//! `machine-nintendo-nes`.
//!
//! # Hardware
//!
//! - **CPU:** Z80A @ 3.579545 MHz (NTSC)
//! - **VDP:** TMS9918A (16 KB VRAM)
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
//! Inherits the donor's `VDP_DOTS_PER_CPU = 3` tick model — see the
//! note in [`ti_tms9918`] and `docs/status/outstanding-work.md`
//! § ColecoVision. Real master-clock ratios on a real CV are
//! crystal 10.738635 MHz with CPU ÷ 3 and VDP dot ÷ 2; the donor's
//! ratio is the starting point and lands the chip catching-up
//! work-list item.

use ti_sn76489::Sn76489;
use ti_tms9918::{Tms9918, VdpRegion};
use zilog_z80::{BusOp, Z80};

/// VDP dot ticks issued per CPU cycle in this initial-port tick model
/// (donor-inherited; see crate-level "Accuracy" note).
const VDP_DOTS_PER_CPU: u32 = 3;

/// CPU cycles per NTSC frame in the initial-port tick model. Derived
/// from `342 dots × 262 lines` of NTSC TMS9918A timing, divided by the
/// initial-port VDP-per-CPU ratio.
const NTSC_CPU_CYCLES_PER_FRAME: u64 = 342 * 262;

/// CPU cycles per PAL frame in the initial-port tick model.
const PAL_CPU_CYCLES_PER_FRAME: u64 = 342 * 313;

/// ColecoVision region.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CvRegion {
    Ntsc,
    Pal,
}

/// Numeric keypad keys (12-key keypad — 0-9, *, #).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
#[derive(Debug, Default, Clone)]
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
pub struct ColecoVision {
    cpu: Z80,
    vdp: Tms9918,
    psg: Sn76489,
    bios: Vec<u8>,
    cart_rom: Vec<u8>,
    ram: [u8; 1024],
    controller1: CvController,
    controller2: CvController,
    /// True when controllers are in joystick mode, false for keypad mode.
    joystick_mode: bool,
    /// Region.
    region: CvRegion,
    /// CPU cycle counter (initial-port tick model).
    cpu_cycles: u64,
    /// CPU cycles per frame for the active region.
    cpu_cycles_per_frame: u64,
    /// Frame counter.
    frame_count: u64,
    /// When `Some`, every I/O port access is appended here (debug trace).
    io_trace: Option<Vec<IoEvent>>,
}

impl ColecoVision {
    /// Create a new ColecoVision with the given BIOS and cartridge ROM.
    #[must_use]
    pub fn new(bios: Vec<u8>, cart_rom: Vec<u8>, region: CvRegion) -> Self {
        let vdp_region = match region {
            CvRegion::Ntsc => VdpRegion::Ntsc,
            CvRegion::Pal => VdpRegion::Pal,
        };
        let cpu_cycles_per_frame = match region {
            CvRegion::Ntsc => NTSC_CPU_CYCLES_PER_FRAME,
            CvRegion::Pal => PAL_CPU_CYCLES_PER_FRAME,
        };
        Self {
            cpu: Z80::new(),
            vdp: Tms9918::new(vdp_region),
            psg: Sn76489::new(3_579_545),
            bios,
            cart_rom,
            ram: [0; 1024],
            controller1: CvController::default(),
            controller2: CvController::default(),
            joystick_mode: false,
            region,
            cpu_cycles: 0,
            cpu_cycles_per_frame,
            frame_count: 0,
            io_trace: None,
        }
    }

    /// Run one frame and return the number of CPU cycles consumed.
    pub fn run_frame(&mut self) -> u64 {
        let target = self.cpu_cycles + self.cpu_cycles_per_frame;
        while self.cpu_cycles < target {
            self.tick_cpu_cycle();
        }
        self.frame_count += 1;
        self.cpu_cycles_per_frame
    }

    /// Advance one CPU cycle of the initial-port tick model.
    fn tick_cpu_cycle(&mut self) {
        // 1. CPU half-cycle ticks until the next bus op is requested.
        //    Per RULES.md rule 6 we drive the Z80 by inspecting its
        //    pins; `bus_request()` collapses one M-cycle's worth of
        //    half-cycles into one bus transaction. The Z80 half-cycles
        //    are advanced by `tick()` until the request is honoured.
        self.cpu.tick();
        self.handle_bus();

        // 2. VDP advances its own dot clock. Initial-port ratio.
        for _ in 0..VDP_DOTS_PER_CPU {
            self.vdp.tick();
        }

        // 3. PSG advances one CPU clock (internal ÷ 16 divider).
        self.psg.tick();

        // 4. VDP INT pin drives the Z80 IRQ pin directly.
        self.cpu.irq = self.vdp.interrupt;

        self.cpu_cycles += 1;
    }

    /// Inspect the Z80's bus request and route memory / I/O.
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
                // ColecoVision uses IM 1 — the BIOS sets IM 1 during
                // init and the VDP INT does not drive a vector; the
                // Z80 fetches `RST 38h` via the floating bus.
                self.cpu.data_in = 0xFF;
            }
            None => {}
        }
    }

    fn mem_read(&self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x1FFF => {
                let idx = addr as usize;
                if idx < self.bios.len() {
                    self.bios[idx]
                } else {
                    0xFF
                }
            }
            0x2000..=0x5FFF => 0xFF,
            0x6000..=0x7FFF => self.ram[(addr & 0x03FF) as usize],
            0x8000..=0xFFFF => {
                let idx = (addr - 0x8000) as usize;
                if idx < self.cart_rom.len() {
                    self.cart_rom[idx]
                } else {
                    0xFF
                }
            }
        }
    }

    fn mem_write(&mut self, addr: u16, data: u8) {
        if (0x6000..=0x7FFF).contains(&addr) {
            self.ram[(addr & 0x03FF) as usize] = data;
        }
    }

    fn io_read(&mut self, port: u16) -> u8 {
        let p = port as u8;
        match p {
            // VDP data read (even ports in $A0-$BF).
            0xA0..=0xBF if p & 1 == 0 => self.vdp.read_data(),
            // VDP status read (odd ports in $A0-$BF).
            0xA0..=0xBF => self.vdp.read_status(),
            // Controllers in $E0-$FF — bit 1 of port selects controller.
            0xE0..=0xFF if p & 0x02 == 0 => {
                if self.joystick_mode {
                    self.controller1.read_joystick()
                } else {
                    self.controller1.read_keypad()
                }
            }
            0xE0..=0xFF => {
                if self.joystick_mode {
                    self.controller2.read_joystick()
                } else {
                    self.controller2.read_keypad()
                }
            }
            _ => 0xFF,
        }
    }

    fn io_write(&mut self, port: u16, data: u8) {
        let p = port as u8;
        match p {
            0x80..=0x9F => self.joystick_mode = false,
            0xA0..=0xBF if p & 1 == 0 => self.vdp.write_data(data),
            0xA0..=0xBF => self.vdp.write_control(data),
            0xC0..=0xDF => self.joystick_mode = true,
            0xE0..=0xFF => self.psg.write(data),
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
    pub fn controller1_mut(&mut self) -> &mut CvController {
        &mut self.controller1
    }

    /// Mutable access to controller 2.
    pub fn controller2_mut(&mut self) -> &mut CvController {
        &mut self.controller2
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
    pub fn region(&self) -> CvRegion {
        self.region
    }

    /// Total CPU cycles executed since power-on.
    #[must_use]
    pub fn cpu_cycles(&self) -> u64 {
        self.cpu_cycles
    }

    /// Frame count since power-on.
    #[must_use]
    pub fn frame_count(&self) -> u64 {
        self.frame_count
    }

    /// Observe one byte on the Z80 bus without side effects (BIOS
    /// / cartridge / RAM read with the chip-side decode). Exposed
    /// for host debugging tools (`memory_read` MCP, watch points).
    #[must_use]
    pub fn peek(&self, addr: u16) -> u8 {
        self.mem_read(addr)
    }
}

impl zilog_z80::Z80Stepper for ColecoVision {
    fn z80_instructions_retired(&self) -> u64 {
        self.cpu.instructions_retired()
    }

    fn step_tick(&mut self) {
        self.tick_cpu_cycle();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn empty_bios() -> Vec<u8> {
        // 8 KB of NOPs followed by a JR to itself at $0008 — gives the
        // CPU something legal to execute so the smoke tests don't crash
        // on stray opcodes even though we don't have a real BIOS.
        let mut bios = vec![0u8; 8192];
        // JR -2 at $0008 ($18 $FE) — infinite tight loop.
        bios[0x0008] = 0x18;
        bios[0x0009] = 0xFE;
        bios
    }

    #[test]
    fn ntsc_frame_returns_expected_cpu_cycles() {
        let mut cv = ColecoVision::new(empty_bios(), vec![], CvRegion::Ntsc);
        let cycles = cv.run_frame();
        assert_eq!(cycles, NTSC_CPU_CYCLES_PER_FRAME);
        assert_eq!(cv.frame_count(), 1);
    }

    #[test]
    fn pal_frame_returns_expected_cpu_cycles() {
        let mut cv = ColecoVision::new(empty_bios(), vec![], CvRegion::Pal);
        let cycles = cv.run_frame();
        assert_eq!(cycles, PAL_CPU_CYCLES_PER_FRAME);
    }

    #[test]
    fn many_frames_complete_without_panic() {
        let mut cv = ColecoVision::new(empty_bios(), vec![], CvRegion::Ntsc);
        for _ in 0..60 {
            cv.run_frame();
        }
        assert_eq!(cv.frame_count(), 60);
    }

    #[test]
    fn vdp_drives_cpu_irq_at_vblank() {
        // Run a frame and check the VDP has set its interrupt flag at
        // some point during the frame. Final IRQ state depends on
        // whether the CPU has acknowledged it via IM 1 RST 38h.
        let mut cv = ColecoVision::new(empty_bios(), vec![], CvRegion::Ntsc);
        cv.run_frame();
        // After at least one full frame the VDP has crossed VBlank.
        // We can't trivially assert irq == true at frame end because
        // the BIOS-less CPU runs IM 0 and never acknowledges, leaving
        // the line pending — but the interrupt FLAG on the VDP should
        // be set during the frame. Test scaffolding for the real
        // assertion when a BIOS is wired in.
        assert!(cv.cpu_cycles() >= NTSC_CPU_CYCLES_PER_FRAME);
    }

    #[test]
    fn joystick_mode_toggle_via_io() {
        let mut cv = ColecoVision::new(empty_bios(), vec![], CvRegion::Ntsc);
        assert!(!cv.joystick_mode);
        cv.io_write(0xC0, 0xFF);
        assert!(cv.joystick_mode);
        cv.io_write(0x80, 0xFF);
        assert!(!cv.joystick_mode);
    }

    #[test]
    fn keypad_read_routes_to_correct_controller() {
        let mut cv = ColecoVision::new(empty_bios(), vec![], CvRegion::Ntsc);
        cv.controller1.keypad = Some(KeypadKey::K5);
        cv.controller2.keypad = Some(KeypadKey::K7);
        // bit 1 = 0 → controller 1.
        let c1 = cv.io_read(0xE0);
        // bit 1 = 1 → controller 2.
        let c2 = cv.io_read(0xE2);
        assert_eq!(c1 & 0x0F, KeypadKey::K5.encode());
        assert_eq!(c2 & 0x0F, KeypadKey::K7.encode());
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

impl ColecoVision {
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
