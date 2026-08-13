//! MSX1 home computer machine wiring.
//!
//! Fresh-write against the workspace pin-driven bus pattern (RULES.md
//! rule 6). The donor at `Emu198x-Oldest/crates/machine-msx` used the
//! deprecated `emu_core::Bus` callback and could not port directly;
//! this file uses it as a system spec — slot system, PPI-driven
//! keyboard matrix, I/O port map, MegaROM mapper variants — but the
//! wiring is written against [`zilog_z80::Z80`]'s public pin fields
//! and `bus_request()` collapse.
//!
//! # The MSX standard
//!
//! The MSX (1983) is a standardised home computer designed by ASCII
//! Corporation and Microsoft. Manufactured by Sony, Philips, Yamaha,
//! Toshiba, Sanyo, Matsushita, Spectravideo, and over a dozen others.
//! Strong in Japan, the Netherlands, Spain, Argentina, Brazil, and
//! the Soviet Union; smaller but real Code198x curriculum target.
//!
//! - **CPU:** Z80A @ 3.579545 MHz
//! - **VDP:** TMS9918A (16 KB VRAM)
//! - **PSG:** AY-3-8910 @ 1.789773 MHz (consumed via our
//!   [`gi_ay_3_8912`] crate — the 8912 silicon is an 8910 with port
//!   B pins not bonded out; register 15 still exists in software and
//!   our crate stores/returns it correctly)
//! - **PPI:** Intel 8255 (Mode 0)
//! - **RAM:** 8-64 KB in slot 3 (this port: 64 KB)
//! - **ROM:** 32 KB Main-ROM (BIOS + MSX-BASIC 1.0) in slot 0
//!
//! # Slot system
//!
//! The Z80's 64 KB address space is divided into four 16 KB pages
//! (page 0 = $0000-$3FFF, ... page 3 = $C000-$FFFF). PPI port A
//! selects one of 4 primary slots per page, two bits per page:
//!
//! ```text
//! PPI port A:  [P3 P3 P2 P2 P1 P1 P0 P0]
//!               bit 7              bit 0
//! ```
//!
//! - **Slot 0:** Main-ROM (BIOS) at pages 0-1
//! - **Slot 1:** Cartridge slot 1
//! - **Slot 2:** Cartridge slot 2
//! - **Slot 3:** Main RAM (64 KB)
//!
//! Subslot expansion (writes to $FFFF when slot 3 is in page 3) is
//! recognised but currently disabled — MSX1 doesn't need it; MSX2+
//! adds it.
//!
//! # I/O port map
//!
//! | Port  | R/W   | Function                                        |
//! |-------|-------|-------------------------------------------------|
//! | `$98` | r/w   | VDP data                                        |
//! | `$99` | r/w   | VDP control (write) / status (read)             |
//! | `$A0` | write | PSG register select                             |
//! | `$A1` | write | PSG register data                               |
//! | `$A2` | read  | PSG register read                               |
//! | `$A8` | r/w   | PPI port A — primary slot select                |
//! | `$A9` | read  | PPI port B — keyboard column data               |
//! | `$AA` | r/w   | PPI port C — keyboard row + CAPS LED + cassette |
//! | `$AB` | write | PPI mode register                               |
//!
//! # Keyboard
//!
//! 11 rows × 8 columns matrix, active-low. PPI port C bits 0-3 drive
//! row select; reading PPI port B returns the column data for the
//! selected row. Standard MSX layout (row 0 = digits 0-7, row 1 =
//! 8/9/punctuation/letters, etc.). Host pokes the matrix via
//! [`Msx::press_key`] / [`Msx::release_key`].
//!
//! # Clock model
//!
//! Adopts the SG-1000's correct 3:2 VDP-dot-to-CPU-T-state phase
//! counter (CPU 3.579545 MHz, VDP dot 5.369 MHz). One iteration of
//! [`Msx::run_frame`] corresponds to one Z80 T-state; per iteration
//! the phase counter advances by 3 and yields one VDP dot whenever
//! it reaches 2. PSG ticks every other T-state (clock ÷ 2).

use gi_ay_3_8912::{Ay3_8912, AyWriteRecord, AyWriteWatch};
use intel_8255::Ppi8255;
use serde::{Deserialize, Serialize};
use ti_tms9918::{Tms9918, VdpRegion};
use zilog_z80::{BusOp, Z80};

/// VDP dot ticks per CPU T-state, numerator (TMS9918A on MSX).
///
/// Accumulated per CPU **half-cycle**, so the denominator is 4 rather than
/// 2: three dots per four half-cycles is the same 3:2 against T-states,
/// interleaved twice as finely.
const VDP_DOT_PHASE_NUMERATOR: u32 = 3;
/// VDP dot ticks per CPU half-cycle, denominator.
const VDP_DOT_PHASE_DENOMINATOR: u32 = 4;
/// CPU T-states per scanline (342 VDP dots × 2 / 3).
const CPU_TSTATES_PER_SCANLINE: u64 = 228;
const NTSC_SCANLINES_PER_FRAME: u64 = 262;
const PAL_SCANLINES_PER_FRAME: u64 = 313;
const NTSC_TSTATES_PER_FRAME: u64 = CPU_TSTATES_PER_SCANLINE * NTSC_SCANLINES_PER_FRAME;
const PAL_TSTATES_PER_FRAME: u64 = CPU_TSTATES_PER_SCANLINE * PAL_SCANLINES_PER_FRAME;

const AY_CLOCK_HZ: u32 = 1_789_773;
const AY_SAMPLE_RATE: u32 = 48_000;
/// Pre-allocated PSG buffer for ~60 Hz host audio. At 48 kHz and
/// 60 fps the budget is 800 samples per frame; round up.
const AY_SAMPLES_PER_FRAME: usize = 1024;

/// MSX video region.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MsxRegion {
    Ntsc,
    Pal,
}

/// MegaROM cartridge mapper.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MapperType {
    /// Plain ROM (no banking), up to 32 KB at $4000-$BFFF (or
    /// $0000-$3FFF on smaller carts).
    Plain,
    /// Konami without SCC: 8 KB banks, written at $6000 / $8000 / $A000.
    Konami,
    /// Konami SCC: 8 KB banks at $5000 / $7000 / $9000 / $B000.
    KonamiScc,
    /// ASCII 8 KB: banks at $6000 / $6800 / $7000 / $7800.
    Ascii8,
    /// ASCII 16 KB: banks at $6000 / $7000.
    Ascii16,
}

/// A cartridge slot containing ROM + mapper bank registers.
#[derive(Serialize, Deserialize)]
struct CartridgeSlot {
    rom: Vec<u8>,
    mapper: MapperType,
    banks: [u8; 4],
}

impl CartridgeSlot {
    fn new(rom: Vec<u8>, mapper: MapperType) -> Self {
        Self {
            rom,
            mapper,
            banks: [0, 1, 2, 3],
        }
    }

    fn empty() -> Self {
        Self {
            rom: Vec::new(),
            mapper: MapperType::Plain,
            banks: [0; 4],
        }
    }

    fn read(&self, addr: u16) -> u8 {
        if self.rom.is_empty() {
            return 0xFF;
        }
        match self.mapper {
            MapperType::Plain => {
                let offset = if addr >= 0x4000 {
                    (addr - 0x4000) as usize
                } else {
                    addr as usize
                };
                self.rom.get(offset).copied().unwrap_or(0xFF)
            }
            MapperType::Konami | MapperType::KonamiScc | MapperType::Ascii8 => {
                let window = match addr {
                    0x4000..=0x5FFF => 0,
                    0x6000..=0x7FFF => 1,
                    0x8000..=0x9FFF => 2,
                    0xA000..=0xBFFF => 3,
                    _ => return 0xFF,
                };
                let bank = self.banks[window] as usize;
                let offset = bank * 8192 + (addr as usize & 0x1FFF);
                self.rom.get(offset).copied().unwrap_or(0xFF)
            }
            MapperType::Ascii16 => {
                let window = usize::from(addr >= 0x8000);
                let bank = self.banks[window] as usize;
                let offset = bank * 16384 + (addr as usize & 0x3FFF);
                self.rom.get(offset).copied().unwrap_or(0xFF)
            }
        }
    }

    fn write(&mut self, addr: u16, value: u8) {
        match self.mapper {
            MapperType::Konami => match addr {
                0x6000..=0x7FFF => self.banks[1] = value,
                0x8000..=0x9FFF => self.banks[2] = value,
                0xA000..=0xBFFF => self.banks[3] = value,
                _ => {}
            },
            MapperType::KonamiScc => match addr {
                0x5000..=0x57FF => self.banks[0] = value,
                0x7000..=0x77FF => self.banks[1] = value,
                0x9000..=0x97FF => self.banks[2] = value,
                0xB000..=0xB7FF => self.banks[3] = value,
                _ => {}
            },
            MapperType::Ascii8 => match addr {
                0x6000..=0x67FF => self.banks[0] = value,
                0x6800..=0x6FFF => self.banks[1] = value,
                0x7000..=0x77FF => self.banks[2] = value,
                0x7800..=0x7FFF => self.banks[3] = value,
                _ => {}
            },
            MapperType::Ascii16 => match addr {
                0x6000..=0x67FF => self.banks[0] = value,
                0x7000..=0x77FF => self.banks[1] = value,
                _ => {}
            },
            MapperType::Plain => {}
        }
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

/// MSX1 machine.
///
/// Fully serialisable for save-states: the Z80, the TMS9918 VDP, the
/// AY-3-8910 PSG, the 8255 PPI, ROM, RAM, slot/bank state, keyboard, and
/// joystick state all carry live state. `io_trace` and `ay_watch` are
/// host-side debug buffers, not machine state, so they are skipped and
/// default on restore.
#[derive(Serialize, Deserialize)]
pub struct Msx {
    cpu: Z80,
    vdp: Tms9918,
    psg: Ay3_8912,
    ppi: Ppi8255,
    bios_rom: Vec<u8>,
    cart1: CartridgeSlot,
    cart2: CartridgeSlot,
    ram: Vec<u8>,
    /// Keyboard matrix: 11 rows × 8 columns, active-low (1 = released).
    keyboard: [u8; 11],
    region: MsxRegion,
    cpu_tstates: u64,
    tstates_per_frame: u64,
    vdp_phase: u32,
    /// Toggles every T-state so the PSG ticks at CPU ÷ 2 (1.789 MHz).
    psg_phase: u8,
    frame_count: u64,
    /// When `Some`, every I/O port access is appended here (debug trace).
    #[serde(skip)]
    io_trace: Option<Vec<IoEvent>>,
    /// When `Some`, every write to the PSG data port ($A1) is captured
    /// for the shared `watch_ay_*` tools. Host-side debug only, not part
    /// of the snapshot.
    #[serde(skip)]
    ay_watch: Option<AyWriteWatch>,
    /// Active-low PSG port-A byte for each of the two joystick ports
    /// (`[0]` = port 1, `[1]` = port 2). The MSX reads the joystick through
    /// the AY-3-891x sound chip: register 14 (port A) presents the directions
    /// and triggers of the port that register 15 bit 6 selects. The standard
    /// per-bit layout is bit 0 up, 1 down, 2 left, 3 right, 4 trigger A,
    /// 5 trigger B, active low (`0` = pressed). Host input only, so it is
    /// not part of the snapshot. Source: MSX PSG joystick standard (no entry
    /// in the in-tree reference library at time of writing).
    joystick: [u8; 2],
}

impl Msx {
    /// Create a new MSX1 with the given BIOS ROM. Both cart slots
    /// start empty; insert with [`Msx::insert_cart1`] /
    /// [`Msx::insert_cart2`].
    #[must_use]
    pub fn new(bios_rom: Vec<u8>, region: MsxRegion) -> Self {
        let vdp_region = match region {
            MsxRegion::Ntsc => VdpRegion::Ntsc,
            MsxRegion::Pal => VdpRegion::Pal,
        };
        let tstates_per_frame = match region {
            MsxRegion::Ntsc => NTSC_TSTATES_PER_FRAME,
            MsxRegion::Pal => PAL_TSTATES_PER_FRAME,
        };
        Self {
            cpu: Z80::new(),
            vdp: Tms9918::new(vdp_region),
            psg: Ay3_8912::new(AY_CLOCK_HZ, AY_SAMPLE_RATE, AY_SAMPLES_PER_FRAME),
            ppi: Ppi8255::new(),
            bios_rom,
            cart1: CartridgeSlot::empty(),
            cart2: CartridgeSlot::empty(),
            ram: vec![0u8; 65536],
            keyboard: [0xFF; 11],
            region,
            cpu_tstates: 0,
            tstates_per_frame,
            vdp_phase: 0,
            psg_phase: 0,
            frame_count: 0,
            io_trace: None,
            ay_watch: None,
            joystick: [0xFF; 2],
        }
    }

    /// Set the digital joystick state for `port` (1 or 2). Composes the
    /// active-low PSG port-A byte the BIOS reads through register 14; the
    /// `fire` button drives trigger A. See the `joystick` field for the
    /// bit layout. Out-of-range ports are clamped to the valid pair.
    #[allow(clippy::fn_params_excessive_bools)]
    pub fn set_joystick(
        &mut self,
        port: u8,
        up: bool,
        down: bool,
        left: bool,
        right: bool,
        fire: bool,
    ) {
        let idx = usize::from(port.clamp(1, 2) - 1);
        let mut byte = 0xFFu8;
        for (pressed, bit) in [
            (up, 0x01),
            (down, 0x02),
            (left, 0x04),
            (right, 0x08),
            (fire, 0x10),
        ] {
            if pressed {
                byte &= !bit;
            }
        }
        self.joystick[idx] = byte;
    }

    /// The active-low PSG port-A byte currently latched for `port` (1 or 2):
    /// the value the BIOS would read through register 14 with that port
    /// selected. Out-of-range ports clamp to the valid pair. For inspection
    /// and host-side input wiring.
    #[must_use]
    pub fn joystick_byte(&self, port: u8) -> u8 {
        self.joystick[usize::from(port.clamp(1, 2) - 1)]
    }

    /// Insert a cartridge into slot 1 with the given mapper.
    pub fn insert_cart1(&mut self, rom: Vec<u8>, mapper: MapperType) {
        self.cart1 = CartridgeSlot::new(rom, mapper);
    }

    /// Insert a cartridge into slot 2.
    pub fn insert_cart2(&mut self, rom: Vec<u8>, mapper: MapperType) {
        self.cart2 = CartridgeSlot::new(rom, mapper);
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

    /// Advance one Z80 T-state.
    fn tick_tstate(&mut self) {
        // Two CPU half-cycles per T-state. `Z80::tick` advances one
        // half-cycle — `T1Rise` then `T1Fall` — so calling it once per
        // T-state ran the CPU at half speed: a `NOP` cost 8 T-states
        // against the Z80's 4, and the machine executed half the work
        // per frame that `tstates_per_frame` budgets for.
        for _ in 0..2 {
            // VDP INT → Z80 IRQ, fed before the tick, not after. The Z80
            // samples `/INT` at an instruction boundary during its own
            // tick, so setting the line afterwards hands it the VDP's
            // state from the previous half-cycle. MSX has no separate NMI
            // source. See
            // `knowledge/decisions/zilog-z80-samples-int-at-the-instruction-boundary.md`.
            self.cpu.irq = self.vdp.interrupt;

            self.cpu.tick();
            self.handle_bus();

            // VDP dots at 3:2 against T-states, accumulated per
            // half-cycle so the frame interrupt lands at the correct
            // scanline relative to CPU execution.
            self.vdp_phase += VDP_DOT_PHASE_NUMERATOR;
            while self.vdp_phase >= VDP_DOT_PHASE_DENOMINATOR {
                self.vdp.tick();
                self.vdp_phase -= VDP_DOT_PHASE_DENOMINATOR;
            }
        }

        // PSG runs at CPU ÷ 2.
        self.psg_phase ^= 1;
        if self.psg_phase == 0 {
            self.psg.tick();
        }

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
                // MSX BIOS sets IM 1 — INT fetches RST 38h via the
                // floating bus.
                self.cpu.data_in = 0xFF;
            }
            None => {}
        }
    }

    /// Resolve which primary slot is selected for the page containing
    /// `addr`. PPI port A holds two bits per page.
    fn resolve_slot(&self, addr: u16) -> u8 {
        let page = (addr >> 14) as usize;
        (self.ppi.port_a >> (page * 2)) & 0x03
    }

    fn mem_read(&self, addr: u16) -> u8 {
        match self.resolve_slot(addr) {
            // BIOS at pages 0-1 ($0000-$7FFF).
            0 if addr < 0x8000 => self.bios_rom.get(addr as usize).copied().unwrap_or(0xFF),
            0 => 0xFF,
            1 => self.cart1.read(addr),
            2 => self.cart2.read(addr),
            3 => self.ram[addr as usize],
            _ => 0xFF,
        }
    }

    fn mem_write(&mut self, addr: u16, value: u8) {
        match self.resolve_slot(addr) {
            0 => {}
            1 => self.cart1.write(addr, value),
            2 => self.cart2.write(addr, value),
            3 => self.ram[addr as usize] = value,
            _ => {}
        }
    }

    fn io_read(&mut self, port: u16) -> u8 {
        let pc = self.cpu.regs.pc;
        let value = match port as u8 {
            0x98 => self.vdp.read_data(),
            0x99 => self.vdp.read_status(),
            0xA2 => {
                // Reading PSG register 14 is the joystick read. Present the
                // selected port's active-low byte through the chip's port-A
                // input mask; register 15 bit 6 selects port 1 (0) or 2 (1).
                if self.psg.selected_register() == 14 {
                    let selected = usize::from((self.psg.registers()[15] >> 6) & 1);
                    self.psg.set_port_a_input_mask(self.joystick[selected]);
                }
                self.psg.read_data()
            }
            0xA8 => self.ppi.read(0),
            0xA9 => {
                // Port B = keyboard column for the row selected by
                // PPI port C bits 0-3.
                let row = self.ppi.keyboard_row() as usize;
                if row < self.keyboard.len() {
                    self.keyboard[row]
                } else {
                    0xFF
                }
            }
            0xAA => self.ppi.read(2),
            0xAB => self.ppi.read(3),
            _ => 0xFF,
        };
        if let Some(trace) = &mut self.io_trace {
            trace.push(IoEvent {
                pc,
                port: port as u8,
                value,
                write: false,
            });
        }
        value
    }

    fn io_write(&mut self, port: u16, value: u8) {
        if let Some(trace) = &mut self.io_trace {
            trace.push(IoEvent {
                pc: self.cpu.regs.pc,
                port: port as u8,
                value,
                write: true,
            });
        }
        match port as u8 {
            0x98 => self.vdp.write_data(value),
            0x99 => self.vdp.write_control(value),
            0xA0 => self.psg.select_register(value),
            0xA1 => {
                if let Some(w) = &mut self.ay_watch {
                    w.record(self.cpu.regs.pc, self.psg.selected_register(), value);
                }
                self.psg.write_data(value);
            }
            0xA8 => self.ppi.write(0, value),
            0xAA => self.ppi.write(2, value),
            0xAB => self.ppi.write(3, value),
            _ => {}
        }
    }

    /// Framebuffer (ARGB32) — active TMS9918 display plus canonical
    /// TV-visible border. Exact dimensions reported by
    /// [`framebuffer_width`](Self::framebuffer_width) and
    /// [`framebuffer_height`](Self::framebuffer_height).
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

    /// Press a key at the given (row, bit) matrix cell.
    pub fn press_key(&mut self, row: usize, bit: u8) {
        if row < self.keyboard.len() && bit < 8 {
            self.keyboard[row] &= !(1 << bit);
        }
    }

    /// Release a key at the given (row, bit) matrix cell.
    pub fn release_key(&mut self, row: usize, bit: u8) {
        if row < self.keyboard.len() && bit < 8 {
            self.keyboard[row] |= 1 << bit;
        }
    }

    /// Mutable keyboard matrix (active-low; 0 = pressed).
    pub fn keyboard_mut(&mut self) -> &mut [u8; 11] {
        &mut self.keyboard
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

    /// PSG reference.
    #[must_use]
    pub fn psg(&self) -> &Ay3_8912 {
        &self.psg
    }

    /// PSG mutable reference (for the host to call `end_frame` on per
    /// audio frame).
    pub fn psg_mut(&mut self) -> &mut Ay3_8912 {
        &mut self.psg
    }

    /// PPI reference.
    #[must_use]
    pub fn ppi(&self) -> &Ppi8255 {
        &self.ppi
    }

    /// Region.
    #[must_use]
    pub fn region(&self) -> MsxRegion {
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

    /// Observe one byte on the Z80 bus without side effects (slot
    /// resolution against current PPI port A, then BIOS / cartridge /
    /// RAM read). Mirrors the private `mem_read`; exposed for host
    /// debugging tools (`memory_read` MCP, watch points, etc.).
    #[must_use]
    pub fn peek(&self, addr: u16) -> u8 {
        self.mem_read(addr)
    }

    /// Write one byte through the slot-resolved bus (RAM accepts it; BIOS /
    /// unmapped slots ignore it). For host debugging (`poke_*` MCP tools).
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

    /// Start (or restart) capturing PSG register writes for `watch_ay_*`.
    /// Returns the log capacity (max records before writes are dropped).
    pub fn start_ay_write_watch(&mut self) -> u32 {
        let watch = AyWriteWatch::new();
        let cap = watch.cap() as u32;
        self.ay_watch = Some(watch);
        cap
    }

    /// Stop capturing PSG writes and drop the log.
    pub fn stop_ay_write_watch(&mut self) {
        self.ay_watch = None;
    }

    /// Captured PSG writes since the last `start_ay_write_watch`, or
    /// `None` when the watch is disarmed.
    #[must_use]
    pub fn ay_write_watch_records(&self) -> Option<&[AyWriteRecord]> {
        self.ay_watch.as_ref().map(AyWriteWatch::records)
    }

    /// Drop captured PSG writes while leaving the watch armed.
    pub fn clear_ay_write_watch_records(&mut self) {
        if let Some(w) = &mut self.ay_watch {
            w.clear();
        }
    }
}

impl zilog_z80::Z80Stepper for Msx {
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

    fn trap_bios() -> Vec<u8> {
        // 32 KB BIOS-shaped ROM full of NOPs with a JR -2 trap at
        // $0008 — gives the Z80 something legal to run for boot-
        // structure tests.
        let mut rom = vec![0u8; 32768];
        rom[0x0008] = 0x18;
        rom[0x0009] = 0xFE;
        rom
    }

    /// Save-state must capture LIVE machine state (Z80 + TMS9918 VDP +
    /// AY-3-8910 PSG + 8255 PPI + RAM + slot/bank state), not cold-boot from
    /// the ROM. Serialise, advance (so the state differs), then deserialise
    /// the first snapshot and confirm re-serialising it is byte-identical —
    /// every stateful field round-trips, including the VDP's 16 KB VRAM.
    #[test]
    fn snapshot_round_trips_live_state() {
        let mut sys = Msx::new(trap_bios(), MsxRegion::Ntsc);
        sys.run_frame();
        // RAM lives in slot 3; at reset PPI port A = 0 selects slot 0 for
        // every page, so route page 3 ($C000-$FFFF) to slot 3 before poking.
        sys.ppi.write(0, 0b1100_0000); // page 3 → slot 3 (RAM)
        sys.poke(0xC100, 0xA5); // a work-RAM byte to carry across the snapshot
        assert_eq!(sys.peek(0xC100), 0xA5, "0xC100 is RAM and accepts the poke");
        sys.run_frame();
        let s1 = postcard::to_allocvec(&sys).expect("encode snapshot");

        sys.run_frame(); // advance past the snapshot point
        let s2 = postcard::to_allocvec(&sys).expect("encode again");
        assert_ne!(s1, s2, "running a frame should change the serialised state");

        let restored: Msx = postcard::from_bytes(&s1).expect("decode snapshot");
        let s3 = postcard::to_allocvec(&restored).expect("re-encode restored");
        assert_eq!(
            s1, s3,
            "restore should reproduce the snapshot state exactly"
        );
    }

    #[test]
    fn ntsc_frame_returns_expected_tstates() {
        let mut sys = Msx::new(trap_bios(), MsxRegion::Ntsc);
        let t = sys.run_frame();
        assert_eq!(t, NTSC_TSTATES_PER_FRAME);
        assert_eq!(sys.frame_count(), 1);
    }

    #[test]
    fn ay_watch_captures_psg_data_writes() {
        let mut sys = Msx::new(trap_bios(), MsxRegion::Ntsc);
        // Disarmed by default: no records.
        assert!(sys.ay_write_watch_records().is_none());

        let cap = sys.start_ay_write_watch();
        assert!(cap > 0, "start reports a non-zero capacity");

        // Select R7 (mixer), write a value; select R8 (channel-A volume),
        // write another. Each data write ($A1) should capture the register
        // selected by the preceding $A0 write.
        sys.io_write(0xA0, 7);
        sys.io_write(0xA1, 0x38);
        sys.io_write(0xA0, 8);
        sys.io_write(0xA1, 0x0F);

        let records = sys.ay_write_watch_records().expect("watch is armed");
        assert_eq!(records.len(), 2, "two data writes captured");
        assert_eq!((records[0].register, records[0].value), (7, 0x38));
        assert_eq!((records[1].register, records[1].value), (8, 0x0F));

        // A bare register-select ($A0) is not a data write — still two.
        sys.io_write(0xA0, 1);
        assert_eq!(sys.ay_write_watch_records().expect("armed").len(), 2);

        sys.clear_ay_write_watch_records();
        assert_eq!(sys.ay_write_watch_records().expect("armed").len(), 0);

        sys.stop_ay_write_watch();
        assert!(sys.ay_write_watch_records().is_none());
    }

    #[test]
    fn pal_frame_returns_expected_tstates() {
        let mut sys = Msx::new(trap_bios(), MsxRegion::Pal);
        let t = sys.run_frame();
        assert_eq!(t, PAL_TSTATES_PER_FRAME);
    }

    #[test]
    fn many_frames_complete_without_panic() {
        let mut sys = Msx::new(trap_bios(), MsxRegion::Ntsc);
        for _ in 0..60 {
            sys.run_frame();
        }
        assert_eq!(sys.frame_count(), 60);
    }

    #[test]
    fn default_slot_layout_reads_bios_then_ram() {
        // PPI port A defaults to 0 → slot 0 selected for all four
        // pages. Page 0 (BIOS NOP region) should read 0.
        let sys = Msx::new(trap_bios(), MsxRegion::Ntsc);
        assert_eq!(sys.mem_read(0x0000), 0);
        // BIOS-trap byte at $0008.
        assert_eq!(sys.mem_read(0x0008), 0x18);
        // Page 2+ reads slot 0 with addr >= 0x8000 — returns 0xFF.
        assert_eq!(sys.mem_read(0xC000), 0xFF);
    }

    #[test]
    fn slot_select_routes_pages() {
        let mut sys = Msx::new(trap_bios(), MsxRegion::Ntsc);
        // Select slot 3 (RAM) for page 3 only.
        sys.ppi.write(0, 0b1100_0000); // page 3 → slot 3
        sys.mem_write(0xC000, 0xAB);
        sys.mem_write(0xC001, 0xCD);
        assert_eq!(sys.mem_read(0xC000), 0xAB);
        assert_eq!(sys.mem_read(0xC001), 0xCD);
        // Page 0 still reads BIOS (slot 0).
        assert_eq!(sys.mem_read(0x0008), 0x18);
    }

    #[test]
    fn joystick_reads_through_psg_port_a() {
        let mut sys = Msx::new(trap_bios(), MsxRegion::Ntsc);

        // Port 1 is selected by default (R15 bit 6 = 0). Press up + fire.
        sys.set_joystick(1, true, false, false, false, true);
        sys.io_write(0xA0, 14); // select PSG register 14 (port A)
        let p1 = sys.io_read(0xA2);
        assert_eq!(p1 & 0x01, 0, "up pressed → bit 0 low");
        assert_eq!(p1 & 0x10, 0, "fire pressed → bit 4 (trigger A) low");
        assert_eq!(p1 & 0x0E, 0x0E, "down/left/right idle high");

        // Release → idle high across the board.
        sys.set_joystick(1, false, false, false, false, false);
        sys.io_write(0xA0, 14);
        assert_eq!(sys.io_read(0xA2), 0xFF, "released joystick reads idle 0xFF");

        // Selecting port 2 (R15 bit 6 = 1) reads the other stick independently.
        sys.set_joystick(2, false, true, false, false, false);
        sys.io_write(0xA0, 15); // select R15 (port B)
        sys.io_write(0xA1, 0x40); // bit 6 = 1 → joystick port 2
        sys.io_write(0xA0, 14);
        let p2 = sys.io_read(0xA2);
        assert_eq!(p2 & 0x02, 0, "port 2 down → bit 1 low");
        assert_eq!(p2 & 0x01, 0x01, "port 2 up idle high");
    }

    #[test]
    fn keyboard_press_and_release_round_trip() {
        let mut sys = Msx::new(trap_bios(), MsxRegion::Ntsc);
        sys.press_key(2, 3);
        assert_eq!(sys.keyboard[2] & 0b0000_1000, 0);
        sys.release_key(2, 3);
        assert_eq!(sys.keyboard[2] & 0b0000_1000, 0b0000_1000);
    }

    #[test]
    fn keyboard_io_returns_selected_row() {
        let mut sys = Msx::new(trap_bios(), MsxRegion::Ntsc);
        sys.keyboard[5] = 0x42;
        // Select row 5 via PPI port C bits 0-3.
        sys.ppi.write(2, 0x05);
        assert_eq!(sys.io_read(0xA9), 0x42);
    }

    #[test]
    fn vdp_dot_ratio_is_three_per_two_tstates() {
        let mut sys = Msx::new(trap_bios(), MsxRegion::Ntsc);
        let start = sys.vdp.scanline();
        for _ in 0..4 {
            sys.tick_tstate();
        }
        assert_eq!(sys.vdp.scanline(), start);
        assert_eq!(sys.vdp_phase, 0);
    }

    #[test]
    fn cart_plain_mapper_reads_at_0x4000() {
        let mut sys = Msx::new(trap_bios(), MsxRegion::Ntsc);
        let mut cart = vec![0u8; 0x4000];
        cart[0] = 0x42;
        cart[1] = 0xAA;
        sys.insert_cart1(cart, MapperType::Plain);
        // Select slot 1 for page 1 ($4000-$7FFF).
        sys.ppi.write(0, 0b0000_0100);
        assert_eq!(sys.mem_read(0x4000), 0x42);
        assert_eq!(sys.mem_read(0x4001), 0xAA);
    }
}
