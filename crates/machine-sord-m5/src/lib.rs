//! Sord M5 machine wiring.
//!
//! Fresh-write against the workspace pin-driven bus pattern (RULES.md
//! rule 6). The donor at `Emu198x-Oldest/crates/machine-sord-m5` used
//! the deprecated `emu_core::Bus` callback and could not port
//! directly; this file uses it as a system spec — memory map, I/O
//! port routing, keyboard matrix, CTC stub — but the wiring is
//! written against [`zilog_z80::Z80`]'s public pin fields and
//! `bus_request()` collapse.
//!
//! # The Sord M5
//!
//! The M5 (1982) is a Japanese home computer from Sord Computer
//! Corporation, sold in Europe through Takara. Same TMS9918A + SN76489
//! pair as ColecoVision / SG-1000 / MSX, but with a built-in keyboard,
//! an 8 KB monitor / BASIC-I ROM, and a unique cart layout that places
//! the cart at `$2000-$6FFF` rather than the Coleco / SG-1000 ceiling
//! at `$8000-$FFFF` / `$0000-$BFFF`.
//!
//! - **CPU:** Z80A @ 3.579545 MHz
//! - **VDP:** TMS9918A (16 KB VRAM)
//! - **PSG:** SN76489A
//! - **CTC:** Z80 CTC ([`zilog_z80_ctc::Ctc`]) at port `$00-$03`. The
//!   TMS9918A `/INT` line feeds one CTC channel's `CLK/TRG` input; the
//!   channel counts those edges (time constant 1) and the CTC supplies
//!   the matching IM 2 vector. The BIOS programs the channel during early
//!   init; this is what carries the M5 from VDP setup into its VBlank
//!   handler.
//! - **RAM:** 4 KB at `$7000-$7FFF`; optional cart RAM at
//!   `$8000-$BFFF` (up to 16 KB, FALC etc.)
//! - **Monitor ROM:** 8 KB at `$0000-$1FFF`
//! - **Cartridge:** up to 20 KB at `$2000-$6FFF`
//!
//! # Memory map
//!
//! | Range         | Contents                              |
//! |---------------|---------------------------------------|
//! | `$0000-$1FFF` | Monitor / BASIC-I ROM (8 KB)          |
//! | `$2000-$6FFF` | Cartridge ROM (up to 20 KB)           |
//! | `$7000-$7FFF` | 4 KB system RAM                       |
//! | `$8000-$BFFF` | Optional cart RAM (up to 16 KB)       |
//! | `$C000-$FFFF` | Unmapped (returns `$FF`)              |
//!
//! # I/O map
//!
//! Decoded by `port & 0xF8` — each block fills 8 mirrored ports.
//!
//! | Port block | R/W   | Function                                |
//! |------------|-------|-----------------------------------------|
//! | `$00-$03`  | r/w   | Z80 CTC (channel = port & 3)            |
//! | `$10-$11`  | r/w   | TMS9918A ($10 data, $11 control/status) |
//! | `$20-$27`  | write | SN76489 PSG                             |
//! | `$30-$37`  | write | Keyboard row strobe (provisional)       |
//! | `$40-$47`  | read  | Keyboard column read (provisional)      |
//!
//! The CTC / VDP / PSG ports were corrected from a wrong donor map
//! (which had VDP at `$00`, PSG at `$10`, CTC at `$50`) after an I/O
//! trace of the Monitor ROM showed the real assignments above. The
//! keyboard ports are not yet trace-confirmed.
//!
//! # Keyboard
//!
//! 10 rows × 8 columns matrix, active-low. The CPU writes the row
//! index to port `$30-$37` then reads the column data from
//! `$40-$47` (both port assignments provisional — not yet
//! trace-confirmed). Standard M5 layout (function keys + alpha rows +
//! shift/ctrl).
//!
//! # Clock model
//!
//! Adopts SG-1000's correct 3:2 VDP-dot-to-CPU-T-state phase counter
//! (CPU 3.579545 MHz, VDP dot 5.369 MHz). One iteration corresponds
//! to one Z80 T-state; per iteration the phase counter advances by 3
//! and yields one VDP dot whenever it reaches 2.

use ti_sn76489::Sn76489;
use ti_tms9918::{Tms9918, VdpRegion};
use zilog_z80::{BusOp, Z80};
use zilog_z80_ctc::Ctc;

const VDP_DOT_PHASE_NUMERATOR: u32 = 3;
const VDP_DOT_PHASE_DENOMINATOR: u32 = 2;
const CPU_TSTATES_PER_SCANLINE: u64 = 228;
const NTSC_SCANLINES_PER_FRAME: u64 = 262;
const PAL_SCANLINES_PER_FRAME: u64 = 313;
const NTSC_TSTATES_PER_FRAME: u64 = CPU_TSTATES_PER_SCANLINE * NTSC_SCANLINES_PER_FRAME;
const PAL_TSTATES_PER_FRAME: u64 = CPU_TSTATES_PER_SCANLINE * PAL_SCANLINES_PER_FRAME;

const NTSC_PSG_CLOCK_HZ: u32 = 3_579_545;
const PAL_PSG_CLOCK_HZ: u32 = 3_546_893;

/// Number of keyboard matrix rows on the M5. The keyboard is seven rows
/// (Y0-Y6) read directly at I/O ports `$30`-`$36`, active-high — MAME
/// `sord/m5.cpp` `portr("Y0")`..`"Y6"`.
pub const NUM_KEY_ROWS: usize = 7;

/// CTC channel whose `CLK/TRG` input is wired to the TMS9918A `/INT`
/// line on the Sord M5. The BIOS arms this channel in counter mode with
/// time constant 1, so every VDP frame interrupt produces one vectored
/// Z80 interrupt.
///
/// Confirmed by I/O trace of the Monitor ROM: it programs CTC channel 3
/// as `counter, int-enable, TC=1` (control `$C7`, TC `$01` on port `$03`),
/// vectoring to `$7006 -> $01DF` — the per-frame VDP service routine.
/// Channels 1/2 are `÷256` timers (system jiffy); channel 0 is a spare
/// counter pointing at the `$186C` no-op handler.
pub const VDP_INT_CTC_CHANNEL: u8 = 3;

/// Sord M5 region.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum M5Region {
    Ntsc,
    Pal,
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

/// Sord M5 machine.
pub struct SordM5 {
    cpu: Z80,
    vdp: Tms9918,
    psg: Sn76489,
    rom: Vec<u8>,
    cart_rom: Vec<u8>,
    cart_ram: Vec<u8>,
    ram: [u8; 4096],
    /// 7×8 keyboard matrix, active-high (1 = pressed). Each row Y0-Y6 is read
    /// directly at `$30`-`$36`; there is no row strobe (MAME `sord/m5.cpp`).
    key_matrix: [u8; NUM_KEY_ROWS],
    /// Joystick directions read at `$37`. Both sticks pack into one byte,
    /// **active high** (pressed = 1): player 1 = bit 0 right, 1 up, 2 left,
    /// 3 down; player 2 = bits 4-7 in the same order. Idle is `0x00`.
    joystick: u8,
    /// Z80 CTC at port `$00-$03`. The VDP `/INT` line drives one
    /// channel's `CLK/TRG`; see [`VDP_INT_CTC_CHANNEL`].
    ctc: Ctc,
    /// RETI (`ED 4D`) detector: set when the previous M1 opcode fetch was
    /// the `ED` prefix, so a following `4D` fetch releases the CTC daisy
    /// chain. The Z80 exports no RETI signal, so the machine watches the
    /// opcode stream.
    prev_opcode_ed: bool,
    /// When `Some`, every I/O port access is appended here (debug trace).
    io_trace: Option<Vec<IoEvent>>,
    region: M5Region,
    cpu_tstates: u64,
    tstates_per_frame: u64,
    vdp_phase: u32,
    frame_count: u64,
}

impl SordM5 {
    /// Create a new Sord M5 with the given Monitor / BASIC-I ROM and
    /// optional cart ROM. Cart RAM starts empty; call
    /// [`SordM5::set_cart_ram_size`] to allocate it.
    #[must_use]
    pub fn new(rom: Vec<u8>, cart_rom: Vec<u8>, region: M5Region) -> Self {
        let vdp_region = match region {
            M5Region::Ntsc => VdpRegion::Ntsc,
            M5Region::Pal => VdpRegion::Pal,
        };
        let psg_clock_hz = match region {
            M5Region::Ntsc => NTSC_PSG_CLOCK_HZ,
            M5Region::Pal => PAL_PSG_CLOCK_HZ,
        };
        let tstates_per_frame = match region {
            M5Region::Ntsc => NTSC_TSTATES_PER_FRAME,
            M5Region::Pal => PAL_TSTATES_PER_FRAME,
        };
        Self {
            cpu: Z80::new(),
            vdp: Tms9918::new(vdp_region),
            psg: Sn76489::new(psg_clock_hz),
            rom,
            cart_rom,
            cart_ram: Vec::new(),
            ram: [0; 4096],
            key_matrix: [0x00; NUM_KEY_ROWS],
            joystick: 0x00,
            ctc: Ctc::new(),
            prev_opcode_ed: false,
            io_trace: None,
            region,
            cpu_tstates: 0,
            tstates_per_frame,
            vdp_phase: 0,
            frame_count: 0,
        }
    }

    /// Allocate (or resize) cart RAM at `$8000-$BFFF`. Some carts
    /// (FALC and friends) need extra RAM beyond the built-in 4 KB.
    pub fn set_cart_ram_size(&mut self, bytes: usize) {
        let capped = bytes.min(0x4000);
        self.cart_ram.resize(capped, 0);
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

        self.psg.tick();

        // VDP /INT feeds the CTC channel's CLK/TRG input; the CTC ticks
        // on the CPU clock and edge-counts those frame interrupts. The
        // CTC's INT output (not the raw VDP line) drives the Z80 IRQ pin,
        // so the interrupt is vectored through IM 2.
        self.ctc.set_trg(VDP_INT_CTC_CHANNEL, self.vdp.interrupt);
        self.ctc.tick();
        self.cpu.irq = self.ctc.interrupt();

        self.cpu_tstates += 1;
    }

    fn handle_bus(&mut self) {
        match self.cpu.bus_request() {
            Some(BusOp::MemRead) => {
                let byte = self.mem_read(self.cpu.addr);
                self.cpu.data_in = byte;
                // Watch the opcode stream for RETI (ED 4D) so the CTC
                // daisy chain can release its in-service channel. M1 marks
                // an opcode fetch; the ED prefix and its 4D second byte are
                // consecutive M1 fetches.
                if self.cpu.m1 {
                    if self.prev_opcode_ed && byte == 0x4D {
                        self.ctc.reti();
                    }
                    self.prev_opcode_ed = byte == 0xED;
                }
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
                // The Monitor ROM runs IM 2 with `I = $70`. The CTC
                // supplies the low vector byte for the requesting channel;
                // the Z80 forms the table address `(I << 8) | vector` and
                // fetches the handler. With the VDP-driven channel this
                // lands on `$7002 -> $1861` (the VBlank jiffy handler),
                // carrying the BIOS past VDP init.
                self.cpu.data_in = self.ctc.acknowledge();
            }
            None => {}
        }
    }

    fn mem_read(&self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x1FFF => self.rom.get(addr as usize).copied().unwrap_or(0xFF),
            0x2000..=0x6FFF => self
                .cart_rom
                .get((addr - 0x2000) as usize)
                .copied()
                .unwrap_or(0xFF),
            0x7000..=0x7FFF => self.ram[(addr & 0x0FFF) as usize],
            0x8000..=0xBFFF => self
                .cart_ram
                .get((addr - 0x8000) as usize)
                .copied()
                .unwrap_or(0xFF),
            _ => 0xFF,
        }
    }

    fn mem_write(&mut self, addr: u16, value: u8) {
        match addr {
            0x7000..=0x7FFF => self.ram[(addr & 0x0FFF) as usize] = value,
            0x8000..=0xBFFF => {
                let idx = (addr - 0x8000) as usize;
                if let Some(slot) = self.cart_ram.get_mut(idx) {
                    *slot = value;
                }
            }
            _ => {}
        }
    }

    fn io_read(&mut self, port: u16) -> u8 {
        let pc = self.cpu.regs.pc;
        let p = port as u8;
        let value = match p & 0xF8 {
            // CTC at $00-$03 (channel = CS1,CS0 = A1,A0); read = live counter.
            0x00 => self.ctc.read(p & 0x03),
            // TMS9918A at $10 (data) / $11 (status).
            0x10 => {
                if p & 1 == 0 {
                    self.vdp.read_data()
                } else {
                    self.vdp.read_status()
                }
            }
            // Keyboard rows Y0-Y6 at $30-$36 and the joystick at $37, each read
            // directly (active-high), with the A3 mirror folding $38-$3F onto
            // $30-$37. MAME `sord/m5.cpp`: `portr("Y0")`..`"Y6"` + `portr("JOY")`.
            // There is no row strobe — the donor's $30-write / $40-read scheme
            // was fiction ($40 is the Centronics data latch, write-only).
            0x30 | 0x38 => {
                let sel = (p & 0x07) as usize;
                if sel < NUM_KEY_ROWS {
                    self.key_matrix[sel]
                } else {
                    self.joystick
                }
            }
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

    fn io_write(&mut self, port: u16, value: u8) {
        let pc = self.cpu.regs.pc;
        let p = port as u8;
        if let Some(trace) = &mut self.io_trace {
            trace.push(IoEvent {
                pc,
                port: p,
                value,
                write: true,
            });
        }
        match p & 0xF8 {
            // CTC at $00-$03 (channel = CS1,CS0 = A1,A0).
            0x00 => self.ctc.write(p & 0x03, value),
            // TMS9918A at $10 (data) / $11 (control).
            0x10 => {
                if p & 1 == 0 {
                    self.vdp.write_data(value);
                } else {
                    self.vdp.write_control(value);
                }
            }
            // SN76489A PSG at $20.
            0x20 => self.psg.write(value),
            // $30 write is the 64KBF memory-paging latch (expansion RAM, not
            // modelled); $40 is the Centronics data latch. Neither touches the
            // keyboard, which is read-only at $30-$36.
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

    /// Write one byte through the bus (RAM accepts it; ROM ignores it).
    pub fn poke(&mut self, addr: u16, value: u8) {
        self.mem_write(addr, value);
    }

    /// Start (or restart) the I/O port-access trace. Every subsequent
    /// `IN`/`OUT` is recorded until [`SordM5::take_io_trace`].
    pub fn start_io_trace(&mut self) {
        self.io_trace = Some(Vec::new());
    }

    /// Stop tracing and return the captured I/O events.
    pub fn take_io_trace(&mut self) -> Vec<IoEvent> {
        self.io_trace.take().unwrap_or_default()
    }

    /// Run whole instructions until the CPU reaches `target_pc` or
    /// `max_tstates` elapse. Returns `(tstates_consumed, reached)`.
    pub fn run_until_pc(&mut self, target_pc: u16, max_tstates: u64) -> (u64, bool) {
        let start = self.cpu_tstates;
        while self.cpu_tstates - start < max_tstates {
            self.tick_tstate();
            if self.cpu.instruction_complete() && self.cpu.regs.pc == target_pc {
                return (self.cpu_tstates - start, true);
            }
        }
        (self.cpu_tstates - start, false)
    }

    /// Take the accumulated PSG audio buffer.
    pub fn take_audio_buffer(&mut self) -> Vec<f32> {
        self.psg.take_buffer()
    }

    /// Press a key at the given matrix (row, bit) cell. Active-high: a pressed
    /// key sets its bit (the BIOS reads `$30`-`$36` and sees the 1).
    pub fn press_key(&mut self, row: usize, bit: u8) {
        if row < self.key_matrix.len() && bit < 8 {
            self.key_matrix[row] |= 1 << bit;
        }
    }

    /// Release a key at the given matrix (row, bit) cell. Active-high: clearing
    /// the bit returns the cell to its idle (released) state.
    pub fn release_key(&mut self, row: usize, bit: u8) {
        if row < self.key_matrix.len() && bit < 8 {
            self.key_matrix[row] &= !(1 << bit);
        }
    }

    /// Set the digital joystick directions for `port` (1 or 2). Read at `$37`,
    /// active high (pressed = 1), bit order right/up/left/down per player
    /// (player 1 in bits 0-3, player 2 in bits 4-7). The M5 control port has no
    /// separate fire line — action buttons are on the keyboard. Out-of-range
    /// ports clamp to the valid pair.
    pub fn set_joystick(&mut self, port: u8, up: bool, down: bool, left: bool, right: bool) {
        let mut nibble = 0u8;
        for (pressed, bit) in [(right, 0x01), (up, 0x02), (left, 0x04), (down, 0x08)] {
            if pressed {
                nibble |= bit;
            }
        }
        let shift = (port.clamp(1, 2) - 1) * 4;
        self.joystick = (self.joystick & !(0x0F << shift)) | (nibble << shift);
    }

    /// The joystick directions byte read at `$37`. For inspection and
    /// host-side input wiring.
    #[must_use]
    pub fn joystick_byte(&self) -> u8 {
        self.joystick
    }

    /// Mutable keyboard matrix (active-high; 1 = pressed).
    pub fn key_matrix_mut(&mut self) -> &mut [u8; NUM_KEY_ROWS] {
        &mut self.key_matrix
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

    /// CTC reference (for observation / debug).
    #[must_use]
    pub fn ctc(&self) -> &Ctc {
        &self.ctc
    }

    /// Region.
    #[must_use]
    pub fn region(&self) -> M5Region {
        self.region
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

    /// CPU T-states in one frame for the current region.
    #[must_use]
    pub fn tstates_per_frame(&self) -> u64 {
        self.tstates_per_frame
    }
}

impl zilog_z80::Z80Stepper for SordM5 {
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

    fn trap_rom() -> Vec<u8> {
        // 8 KB Monitor-ROM stub: NOPs with a JR -2 trap at $0008.
        let mut rom = vec![0u8; 8192];
        rom[0x0008] = 0x18;
        rom[0x0009] = 0xFE;
        rom
    }

    #[test]
    fn ntsc_frame_returns_expected_tstates() {
        let mut sys = SordM5::new(trap_rom(), vec![], M5Region::Ntsc);
        let t = sys.run_frame();
        assert_eq!(t, NTSC_TSTATES_PER_FRAME);
        assert_eq!(sys.frame_count(), 1);
    }

    #[test]
    fn pal_frame_returns_expected_tstates() {
        let mut sys = SordM5::new(trap_rom(), vec![], M5Region::Pal);
        let t = sys.run_frame();
        assert_eq!(t, PAL_TSTATES_PER_FRAME);
    }

    #[test]
    fn many_frames_complete_without_panic() {
        let mut sys = SordM5::new(trap_rom(), vec![], M5Region::Ntsc);
        for _ in 0..60 {
            sys.run_frame();
        }
        assert_eq!(sys.frame_count(), 60);
    }

    #[test]
    fn memory_map_routes_pages() {
        let mut sys = SordM5::new(trap_rom(), vec![0xCAu8; 0x4000], M5Region::Ntsc);
        // BIOS trap byte.
        assert_eq!(sys.mem_read(0x0008), 0x18);
        // Cart ROM mirror.
        assert_eq!(sys.mem_read(0x2000), 0xCA);
        // RAM round-trip.
        sys.mem_write(0x7100, 0x42);
        assert_eq!(sys.mem_read(0x7100), 0x42);
        // Cart RAM round-trip after allocating.
        sys.set_cart_ram_size(0x4000);
        sys.mem_write(0x8000, 0x77);
        assert_eq!(sys.mem_read(0x8000), 0x77);
        // Unmapped region.
        assert_eq!(sys.mem_read(0xC000), 0xFF);
    }

    #[test]
    fn keyboard_rows_read_directly_at_0x30_block() {
        let mut sys = SordM5::new(trap_rom(), vec![], M5Region::Ntsc);
        // Each row Y0-Y6 is read directly at its own port $30+row — no strobe.
        sys.key_matrix[3] = 0x77;
        assert_eq!(sys.io_read(0x33), 0x77, "row 3 reads at $33");
        sys.key_matrix[6] = 0xAA;
        assert_eq!(sys.io_read(0x36), 0xAA, "row 6 reads at $36");
        // A3 mirror: $38-$3E fold onto $30-$36.
        assert_eq!(sys.io_read(0x3B), 0x77, "$3B mirrors $33");
        // $37 in the same block is the joystick, not a keyboard row.
        sys.set_joystick(1, true, false, false, false);
        assert_eq!(sys.io_read(0x37) & 0x02, 0x02, "$37 is JOY (P1 up)");
        // $40 read is unmapped open bus (Centronics is write-only there).
        assert_eq!(sys.io_read(0x40), 0xFF);
    }

    #[test]
    fn joystick_reads_active_high_at_0x37() {
        let mut sys = SordM5::new(trap_rom(), vec![], M5Region::Ntsc);
        // Idle: nothing pressed → all lines low (active high).
        assert_eq!(sys.io_read(0x37), 0x00);

        // Player 1 right + up → bits 0 and 1 high. (up, down, left, right)
        sys.set_joystick(1, true, false, false, true);
        let v = sys.io_read(0x37);
        assert_eq!(v & 0x01, 0x01, "P1 right → bit 0 high");
        assert_eq!(v & 0x02, 0x02, "P1 up → bit 1 high");
        assert_eq!(v & 0xF0, 0x00, "P2 nibble idle low");

        // Player 2 down → bit 7; independent of P1.
        sys.set_joystick(2, false, true, false, false);
        let v = sys.io_read(0x37);
        assert_eq!(v & 0x80, 0x80, "P2 down → bit 7 high");
        assert_eq!(v & 0x03, 0x03, "P1 right+up still held");

        // Mirror: $3F (A3 set) reads the same byte.
        assert_eq!(sys.io_read(0x3F), sys.io_read(0x37));
    }

    #[test]
    fn vdp_io_routes_at_0x10_block() {
        let mut sys = SordM5::new(trap_rom(), vec![], M5Region::Ntsc);
        sys.io_write(0x11, 0x00); // VDP control low byte
        sys.io_write(0x11, 0x40); // VDP control high (set VRAM write addr $0000)
        sys.io_write(0x10, 0x42); // VDP data write
        // Reading status ($11) doesn't panic.
        let _ = sys.io_read(0x11);
    }

    #[test]
    fn ctc_routes_at_0x00_block() {
        let mut sys = SordM5::new(trap_rom(), vec![], M5Region::Ntsc);
        // Channel 0: vector base, then a ÷16 timer with time constant 5.
        sys.io_write(0x00, 0x40); // vector base $40 (D0=0 → vector)
        sys.io_write(0x00, 0x05); // control: TC follows + control word
        sys.io_write(0x00, 5); // time constant
        assert!(sys.ctc().running(0));
        assert_eq!(sys.io_read(0x00), 5, "read returns the live counter");
        assert_eq!(sys.ctc().vector_base(), 0x40);
    }

    #[test]
    fn vdp_dot_ratio_is_three_per_two_tstates() {
        let mut sys = SordM5::new(trap_rom(), vec![], M5Region::Ntsc);
        let start = sys.vdp.scanline();
        for _ in 0..4 {
            sys.tick_tstate();
        }
        assert_eq!(sys.vdp.scanline(), start);
        assert_eq!(sys.vdp_phase, 0);
    }

    #[test]
    fn key_press_and_release_round_trip() {
        let mut sys = SordM5::new(trap_rom(), vec![], M5Region::Ntsc);
        // Active-high: pressing sets the cell, releasing clears it. Reading the
        // row's port shows the BIOS-visible bit.
        sys.press_key(2, 5);
        assert_eq!(sys.key_matrix[2] & 0b0010_0000, 0b0010_0000);
        assert_eq!(sys.io_read(0x32) & 0b0010_0000, 0b0010_0000);
        sys.release_key(2, 5);
        assert_eq!(sys.key_matrix[2] & 0b0010_0000, 0);
    }
}
