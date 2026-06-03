//! Tatung Einstein TC-01 machine wiring.
//!
//! Fresh-write against the workspace pin-driven bus pattern (RULES.md
//! rule 6). The donor at `Emu198x-Oldest/crates/machine-tatung-einstein`
//! used the deprecated `emu_core::Bus` callback and could not port
//! directly; this file uses it as a system spec — memory map, port-`$21`
//! ROM page-out, AY-driven keyboard, I/O port routing — but the
//! wiring is written against [`zilog_z80::Z80`]'s public pin fields
//! and `bus_request()` collapse.
//!
//! # The Tatung Einstein TC-01
//!
//! The Einstein (1984) is a UK-designed Z80-based home computer with
//! built-in floppy drive and CP/M as the primary OS — sold mainly into
//! the UK and German education / small-business markets. Same chip
//! stack as MSX (Z80 + TMS9918 + AY-3-8910) but no PPI: the keyboard
//! row select goes through AY port A instead.
//!
//! - **CPU:** Z80A @ 4 MHz (faster than the 3.58 MHz TMS9918-family
//!   standard)
//! - **VDP:** TMS9918A (16 KB VRAM)
//! - **PSG:** AY-3-8910 @ 2 MHz (CPU ÷ 2) — consumed via our
//!   `gi-ay-3-8912` crate (same silicon)
//! - **RAM:** 64 KB
//! - **ROM:** 8 KB X-TAL MOS at `$0000-$1FFF` (pageable)
//! - **CTC:** Z80 CTC (channel 0 stubbed at port `$28`)
//! - **Floppy:** WD1770 (not wired in this initial port)
//!
//! # Memory map
//!
//! Page 0 (`$0000-$1FFF`) returns ROM at reset; **any write to port
//! `$21`** pages the ROM out, leaving the 64 KB RAM visible across the
//! whole address space. Writes always land in RAM regardless of the
//! ROM-page state.
//!
//! # I/O map
//!
//! | Port  | R/W   | Function                                       |
//! |-------|-------|------------------------------------------------|
//! | `$00` | write | AY register select                             |
//! | `$01` | write | AY data write                                  |
//! | `$02` | read  | AY data read                                   |
//! | `$08` | r/w   | VDP data                                       |
//! | `$09` | r/w   | VDP control / status                           |
//! | `$20` | read  | Keyboard column for the row in AY port A (R14) |
//! | `$21` | write | ROM page-out (any value)                       |
//! | `$23` | r/w   | 8-bit ADC (stub — reads `$00`)                 |
//! | `$28` | r/w   | Z80 CTC channel 0 (stub)                       |
//!
//! # Keyboard
//!
//! 8 rows × 8 columns matrix, active-low. The CPU writes the row
//! index (bits 0-2) to AY-3-8910 register 14 (port A output mode);
//! port `$20` reads the column data for the selected row from the
//! keyboard matrix.
//!
//! # Clock model
//!
//! Adopts the 3:2 VDP-dot-per-T-state phase counter pattern from
//! SG-1000 / MSX. Einstein's CPU runs at 4 MHz (vs the 3.58 MHz
//! TMS9918-family standard); the absolute clock rates differ but the
//! relative phase counter holds because both chips run on their own
//! crystals and we approximate using the ratio. PSG ticks every other
//! T-state for the CPU ÷ 2 = 2 MHz AY clock.

use gi_ay_3_8912::Ay3_8912;
use ti_tms9918::{Tms9918, VdpRegion};
use zilog_z80::{BusOp, Z80};

const VDP_DOT_PHASE_NUMERATOR: u32 = 3;
const VDP_DOT_PHASE_DENOMINATOR: u32 = 2;
const CPU_TSTATES_PER_SCANLINE: u64 = 228;
const NTSC_SCANLINES_PER_FRAME: u64 = 262;
const PAL_SCANLINES_PER_FRAME: u64 = 313;
const NTSC_TSTATES_PER_FRAME: u64 = CPU_TSTATES_PER_SCANLINE * NTSC_SCANLINES_PER_FRAME;
const PAL_TSTATES_PER_FRAME: u64 = CPU_TSTATES_PER_SCANLINE * PAL_SCANLINES_PER_FRAME;

const AY_CLOCK_HZ: u32 = 2_000_000;
const AY_SAMPLE_RATE: u32 = 48_000;
const AY_SAMPLES_PER_FRAME: usize = 1024;

/// Number of keyboard matrix rows.
pub const NUM_KEY_ROWS: usize = 8;

/// Einstein region.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EinsteinRegion {
    Ntsc,
    Pal,
}

/// Tatung Einstein TC-01 machine.
pub struct Einstein {
    cpu: Z80,
    vdp: Tms9918,
    psg: Ay3_8912,
    rom: Vec<u8>,
    ram: [u8; 65536],
    /// `$0000-$1FFF` returns ROM at reset; any write to `$21` flips
    /// this `false` and exposes the 64 KB RAM across the full space.
    rom_paged_in: bool,
    /// 8×8 keyboard matrix, active-low.
    keyboard: [u8; NUM_KEY_ROWS],
    /// CTC channel 0 stub.
    ctc_reg: u8,
    region: EinsteinRegion,
    cpu_tstates: u64,
    tstates_per_frame: u64,
    vdp_phase: u32,
    psg_phase: u8,
    frame_count: u64,
    /// When `Some`, every I/O port access is appended here (debug trace).
    io_trace: Option<Vec<IoEvent>>,
}

impl Einstein {
    /// Create a new Einstein with the given 8 KB X-TAL MOS ROM.
    #[must_use]
    pub fn new(rom: Vec<u8>, region: EinsteinRegion) -> Self {
        let vdp_region = match region {
            EinsteinRegion::Ntsc => VdpRegion::Ntsc,
            EinsteinRegion::Pal => VdpRegion::Pal,
        };
        let tstates_per_frame = match region {
            EinsteinRegion::Ntsc => NTSC_TSTATES_PER_FRAME,
            EinsteinRegion::Pal => PAL_TSTATES_PER_FRAME,
        };
        Self {
            cpu: Z80::new(),
            vdp: Tms9918::new(vdp_region),
            psg: Ay3_8912::new(AY_CLOCK_HZ, AY_SAMPLE_RATE, AY_SAMPLES_PER_FRAME),
            rom,
            ram: [0; 65536],
            rom_paged_in: true,
            keyboard: [0xFF; NUM_KEY_ROWS],
            ctc_reg: 0,
            region,
            cpu_tstates: 0,
            tstates_per_frame,
            vdp_phase: 0,
            psg_phase: 0,
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
        self.cpu.tick();
        self.handle_bus();

        self.vdp_phase += VDP_DOT_PHASE_NUMERATOR;
        while self.vdp_phase >= VDP_DOT_PHASE_DENOMINATOR {
            self.vdp.tick();
            self.vdp_phase -= VDP_DOT_PHASE_DENOMINATOR;
        }

        self.psg_phase ^= 1;
        if self.psg_phase == 0 {
            self.psg.tick();
        }

        // VDP /INT → Z80 /IRQ; CTC stub doesn't generate interrupts.
        self.cpu.irq = self.vdp.interrupt;

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
                // X-TAL MOS sets IM 1 — INT fetches RST 38h via the
                // floating bus.
                self.cpu.data_in = 0xFF;
            }
            None => {}
        }
    }

    fn mem_read(&self, addr: u16) -> u8 {
        if self.rom_paged_in && addr < 0x2000 {
            self.rom.get(addr as usize).copied().unwrap_or(0xFF)
        } else {
            self.ram[addr as usize]
        }
    }

    fn mem_write(&mut self, addr: u16, value: u8) {
        // Writes always go to RAM, even with ROM paged in (so the
        // initial RAM clear can populate $0000-$1FFF before page-out).
        self.ram[addr as usize] = value;
    }

    fn io_read(&mut self, port: u16) -> u8 {
        match port as u8 {
            0x02 => self.psg.read_data(),
            0x08 => self.vdp.read_data(),
            0x09 => self.vdp.read_status(),
            0x20 => {
                // AY port A (register 14) low 3 bits select the
                // keyboard row.
                let row = (self.psg.registers()[14] & 0x07) as usize;
                self.keyboard[row]
            }
            0x23 => 0x00,
            0x28 => self.ctc_reg,
            _ => 0xFF,
        }
    }

    fn io_write(&mut self, port: u16, value: u8) {
        match port as u8 {
            0x00 => self.psg.select_register(value),
            0x01 => self.psg.write_data(value),
            0x08 => self.vdp.write_data(value),
            0x09 => self.vdp.write_control(value),
            0x21 => self.rom_paged_in = false,
            0x23 => {}
            0x28 => self.ctc_reg = value,
            _ => {}
        }
    }

    /// Framebuffer (ARGB32).
    #[must_use]
    pub fn framebuffer(&self) -> &[u32] {
        self.vdp.framebuffer()
    }

    /// Framebuffer width.
    #[must_use]
    pub fn framebuffer_width(&self) -> u32 {
        self.vdp.framebuffer_width()
    }

    /// Framebuffer height.
    #[must_use]
    pub fn framebuffer_height(&self) -> u32 {
        self.vdp.framebuffer_height()
    }

    /// Observe one byte on the Z80 bus without side effects.
    #[must_use]
    pub fn peek(&self, addr: u16) -> u8 {
        self.mem_read(addr)
    }

    /// Press a key at the given (row, column).
    pub fn press_key(&mut self, row: usize, col: u8) {
        if row < self.keyboard.len() && col < 8 {
            self.keyboard[row] &= !(1 << col);
        }
    }

    /// Release a key at the given (row, column).
    pub fn release_key(&mut self, row: usize, col: u8) {
        if row < self.keyboard.len() && col < 8 {
            self.keyboard[row] |= 1 << col;
        }
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
    pub fn region(&self) -> EinsteinRegion {
        self.region
    }

    /// Drain accumulated PSG audio samples for the most recent frame.
    pub fn take_audio_buffer(&mut self) -> Vec<f32> {
        let mut out = vec![0.0_f32; AY_SAMPLES_PER_FRAME];
        self.psg.end_frame(&mut out);
        if let Some(last) = out.iter().rposition(|s| *s != 0.0) {
            out.truncate(last + 1);
        } else {
            out.clear();
        }
        out
    }

    /// `true` if the X-TAL MOS ROM is currently visible at
    /// `$0000-$1FFF`.
    #[must_use]
    pub fn rom_paged_in(&self) -> bool {
        self.rom_paged_in
    }

    /// CPU T-states executed since power-on.
    #[must_use]
    pub fn cpu_tstates(&self) -> u64 {
        self.cpu_tstates
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

    fn trap_rom() -> Vec<u8> {
        let mut rom = vec![0u8; 0x2000];
        rom[0x0008] = 0x18;
        rom[0x0009] = 0xFE;
        rom
    }

    #[test]
    fn ntsc_frame_returns_expected_tstates() {
        let mut sys = Einstein::new(trap_rom(), EinsteinRegion::Ntsc);
        let t = sys.run_frame();
        assert_eq!(t, NTSC_TSTATES_PER_FRAME);
        assert_eq!(sys.frame_count(), 1);
    }

    #[test]
    fn pal_frame_returns_expected_tstates() {
        let mut sys = Einstein::new(trap_rom(), EinsteinRegion::Pal);
        let t = sys.run_frame();
        assert_eq!(t, PAL_TSTATES_PER_FRAME);
    }

    #[test]
    fn many_frames_complete_without_panic() {
        let mut sys = Einstein::new(trap_rom(), EinsteinRegion::Ntsc);
        for _ in 0..60 {
            sys.run_frame();
        }
        assert_eq!(sys.frame_count(), 60);
    }

    #[test]
    fn rom_visible_at_reset_pages_out_on_write_21() {
        let mut sys = Einstein::new(trap_rom(), EinsteinRegion::Ntsc);
        assert!(sys.rom_paged_in());
        assert_eq!(sys.mem_read(0x0008), 0x18);
        sys.io_write(0x21, 0x00);
        assert!(!sys.rom_paged_in());
        // After page-out, $0008 reads from RAM (default 0).
        assert_eq!(sys.mem_read(0x0008), 0x00);
    }

    #[test]
    fn writes_always_land_in_ram_even_with_rom_paged_in() {
        let mut sys = Einstein::new(trap_rom(), EinsteinRegion::Ntsc);
        // ROM still in view but write to RAM underneath.
        sys.mem_write(0x0100, 0x42);
        // Page ROM out and re-read.
        sys.io_write(0x21, 0x00);
        assert_eq!(sys.mem_read(0x0100), 0x42);
    }

    #[test]
    fn keyboard_row_selected_via_ay_port_a() {
        let mut sys = Einstein::new(trap_rom(), EinsteinRegion::Ntsc);
        sys.keyboard[5] = 0xAB;
        // Select AY R14 then write row index 5.
        sys.io_write(0x00, 14);
        sys.io_write(0x01, 5);
        assert_eq!(sys.io_read(0x20), 0xAB);
    }

    #[test]
    fn vdp_dot_ratio_is_three_per_two_tstates() {
        let mut sys = Einstein::new(trap_rom(), EinsteinRegion::Ntsc);
        let start = sys.vdp.scanline();
        for _ in 0..4 {
            sys.tick_tstate();
        }
        assert_eq!(sys.vdp.scanline(), start);
        assert_eq!(sys.vdp_phase, 0);
    }

    #[test]
    fn key_press_and_release() {
        let mut sys = Einstein::new(trap_rom(), EinsteinRegion::Ntsc);
        sys.press_key(2, 5);
        assert_eq!(sys.keyboard[2] & (1 << 5), 0);
        sys.release_key(2, 5);
        assert_eq!(sys.keyboard[2] & (1 << 5), 1 << 5);
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

impl Einstein {
    /// Write one byte through the bus (RAM accepts it; ROM ignores it).
    pub fn poke(&mut self, addr: u16, value: u8) {
        self.mem_write(addr, value);
    }

    /// Run exactly one whole Z80 instruction, returning the clocks it
    /// consumed. A safety cap prevents an unbounded spin.
    pub fn step_instruction(&mut self) -> u64 {
        let start = self.cpu_tstates;
        let cap = start + 1024;
        // Tick until exactly one instruction retires. The monotonic
        // retirement counter is the only reliable boundary signal —
        // `instruction_complete` flips false→true within a single tick
        // for one-M-cycle ops, so a between-tick level check over-runs.
        let target = self.cpu.instructions_retired() + 1;
        while self.cpu.instructions_retired() < target && self.cpu_tstates < cap {
            self.tick_tstate();
        }
        self.cpu_tstates - start
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
