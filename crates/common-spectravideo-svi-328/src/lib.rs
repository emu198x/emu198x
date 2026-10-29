//! Shared SVI-328 wiring, statically bound by each concrete board.
//!
//! Fresh-write against the workspace pin-driven bus pattern (RULES.md
//! rule 6). The donor at
//! `Emu198x-Oldest/crates/machine-spectravideo-svi-328` used the
//! deprecated `emu_core::Bus` callback and could not port directly;
//! this file uses it as a system spec — memory map, PSG-port-B
//! banking, 8255-driven keyboard, I/O port routing — but the wiring
//! is written against [`emu198x_zilog_z80::Z80`]'s public pin fields and
//! `bus_request()` collapse.
//!
//! # The Spectravideo SVI-328
//!
//! The SVI-328 (1983) is the **MSX precursor** — Spectravideo
//! designed it before MSX standardisation, and the MSX standard later
//! crystallised around essentially the same chip stack: Z80 + TMS9918
//! + AY-3-8910 + Intel 8255. The SVI-328 differs from MSX1 mainly in
//!   its simpler **PSG port-B ROM/RAM banking** rather than MSX's
//!   4-slot system, and in its tighter I/O port window (`$80-$97` vs
//!   MSX's spread across `$98`, `$A0-$AB`).
//!
//! - **CPU:** Z80A @ 3.579545 MHz
//! - **VDP:** TMS9918-family register model (16 KB VRAM)
//! - **PSG:** AY-3-8910 @ 1.789773 MHz (CPU ÷ 2) — consumed via our
//!   `emu198x-gi-ay-3-8912` crate (same silicon, port B not bonded out)
//! - **PPI:** Intel 8255 (keyboard row select + column read)
//! - **RAM:** 32 KB at `$8000-$FFFF`, expandable to 64 KB in
//!   ROM-overlay mode
//! - **ROM:** 32 KB BASIC + OS at `$0000-$7FFF`
//!
//! # Memory map
//!
//! | Range         | Default                  | PSG R15 overlay  |
//! |---------------|--------------------------|------------------|
//! | `$0000-$7FFF` | System ROM (BASIC + OS)  | RAM when D1 = 0  |
//! | `$8000-$BFFF` | RAM                      | Cart when D0 = 0 |
//! | `$C000-$FFFF` | RAM                      | (always RAM)     |
//!
//! # I/O map
//!
//! | Port      | R/W   | Function                                      |
//! |-----------|-------|-----------------------------------------------|
//! | `$80`     | write | VDP data                                       |
//! | `$81`     | write | VDP control                                    |
//! | `$84`     | read  | VDP data                                       |
//! | `$85`     | read  | VDP status (vblank ISR acknowledges here)      |
//! | `$88`     | write | PSG register select                            |
//! | `$8C`     | write | PSG register data (R15 bit 1 banks RAM in)     |
//! | `$90`     | read  | PSG register data (R14 = both joysticks)       |
//! | `$94-$97` | write | PPI ports A / B / C / control                  |
//! | `$98`     | read  | PPI port A (paddles, fire buttons, cassette)   |
//! | `$99`     | read  | PPI port B (keyboard column for selected row)  |
//! | `$9A`     | read  | PPI port C                                     |
//!
//! # Keyboard
//!
//! 11 rows × 8 columns matrix, active-low (1 = released). The CPU
//! writes the row index to PPI port C (low nibble) and reads the
//! column data from port B.
//!
//! # Clock model
//!
//! Adopts SG-1000 / MSX 3:2 VDP-dot-per-T-state phase counter. PSG
//! ticks every other T-state for the CPU ÷ 2 = 1.789 MHz AY clock.

mod cassette;

use cassette::Cassette;
pub use cassette::CassetteMedia;
pub use emu198x_gi_ay_3_8912::AyWriteRecord;
use emu198x_gi_ay_3_8912::{Ay3_8912, AyWriteWatch};
use emu198x_intel_8255::Ppi8255;
pub use emu198x_ti_tms9918::{Tms9918, VdpRegion};
use emu198x_z80_cadence::Z80Machine;
use emu198x_zilog_z80::BusOp;
pub use emu198x_zilog_z80::Z80;
use serde::{Deserialize, Serialize};

/// VDP dot ticks per CPU half-cycle. Three dots per four half-cycles is the
/// TMS9918A's 3:2 against T-states, interleaved twice as finely.
const VDP_DOT_PHASE_NUMERATOR: u32 = 3;
const VDP_DOT_PHASE_DENOMINATOR: u32 = 4;
#[cfg(test)]
const CPU_TSTATES_PER_SCANLINE: u64 = 228;
#[cfg(test)]
const NTSC_SCANLINES_PER_FRAME: u64 = 262;
#[cfg(test)]
const PAL_SCANLINES_PER_FRAME: u64 = 313;
#[cfg(test)]
const NTSC_TSTATES_PER_FRAME: u64 = CPU_TSTATES_PER_SCANLINE * NTSC_SCANLINES_PER_FRAME;
#[cfg(test)]
const PAL_TSTATES_PER_FRAME: u64 = CPU_TSTATES_PER_SCANLINE * PAL_SCANLINES_PER_FRAME;

const AY_CLOCK_HZ: u32 = 1_789_773;
const AY_SAMPLE_RATE: u32 = 48_000;
const AY_SAMPLES_PER_FRAME: usize = 1024;

/// Number of keyboard matrix rows.
pub const NUM_KEY_ROWS: usize = 11;

/// SVI-328 region.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SviRegion {
    Ntsc,
    Pal,
}

/// Spectravideo SVI-328 machine.
///
/// Fully serialisable for save-states: the Z80, the TMS9918 VDP, the
/// AY-3-8910 PSG, the 8255 PPI, ROM, RAM, banking, keyboard, and joystick
/// state all carry live state. `io_trace` and `ay_watch` are host-side debug
/// buffers, not machine state, so they are skipped and default on restore.
#[derive(Serialize, Deserialize)]
pub struct SviState {
    cpu: Z80,
    vdp: Tms9918,
    psg: Ay3_8912,
    ppi: Ppi8255,
    rom: Vec<u8>,
    /// 64 KB RAM; the lower 32 KB is exposed in place of the BASIC ROM when
    /// the PSG port B bank register selects it.
    ram: Vec<u8>,
    cart_rom: Vec<u8>,
    /// `true` → RAM replaces ROM at `$0000-$7FFF`. Driven by AY-3-8910 port B
    /// (R15) bit 1 (`bk21`): low banks RAM in. ROM is visible at reset.
    bank_ram_low: bool,
    /// `true` → cart ROM replaces RAM at `$8000-$BFFF`. Driven by AY-3-8910
    /// port B (R15) bit 0 (`CART`): low enables the cartridge bank.
    bank_cart: bool,
    /// The PSG register last selected via the address port ($88); used to spot
    /// writes to R15, the memory-bank register.
    psg_reg_select: u8,
    /// 11×8 keyboard matrix, active-low.
    keyboard: [u8; NUM_KEY_ROWS],
    /// Joystick direction lines, presented on PSG port A (register 14):
    /// player 1 in the low nibble, player 2 in the high nibble, active low
    /// (bit 0/4 up, 1/5 down, 2/6 left, 3/7 right). Idle is `0xFF`.
    joy_dirs: u8,
    /// Joystick fire buttons `[player1, player2]`, read on PPI port A bits
    /// 4-5 (active low). See [`SviCore::io_read`] `$98`.
    joy_fire: [bool; 2],
    cassette: Cassette,
    region: SviRegion,
    cpu_tstates: u64,
    /// The cadence driver's half-cycle phase; a restore lands on a T-state
    /// boundary, so it is not part of the snapshot.
    #[serde(skip)]
    cadence_hc: u64,
    tstates_per_frame: u64,
    vdp_phase: u32,
    psg_phase: u8,
    frame_count: u64,
    /// Transient observation for a bounded native-frame request.
    #[serde(skip)]
    native_frame_ready: bool,
    /// When `Some`, every I/O port access is appended here (debug trace).
    #[serde(skip)]
    io_trace: Option<Vec<IoEvent>>,
    /// When `Some`, every write to the PSG data port ($8C) is captured
    /// for the shared `watch_ay_*` tools. Host-side debug only, not
    /// part of the snapshot.
    #[serde(skip)]
    ay_watch: Option<AyWriteWatch>,
}

/// Fixed wiring supplied by a concrete machine crate; no model switch runs here.
pub trait Board {
    const REGION: SviRegion;
    const VDP_REGION: VdpRegion;
    const TSTATES_PER_FRAME: u64;
    const FRAMEBUFFER_HEIGHT: u32;
}
/// Shared wiring engine, statically bound to one board.
pub struct SviCore<B: Board> {
    state: SviState,
    board: std::marker::PhantomData<B>,
}
impl SviState {
    #[must_use]
    pub fn region(&self) -> SviRegion {
        self.region
    }
}
impl<B: Board> SviCore<B> {
    /// Validate an existing state without reinitialising any chip.
    pub fn from_state(state: SviState) -> Result<Self, &'static str> {
        if state.region != B::REGION
            || state.tstates_per_frame != B::TSTATES_PER_FRAME
            || state.vdp.framebuffer_height() != B::FRAMEBUFFER_HEIGHT
        {
            return Err("snapshot video/region does not match this SVI-328 board");
        }
        Ok(Self {
            state,
            board: std::marker::PhantomData,
        })
    }
}
impl<B: Board> Serialize for SviCore<B> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.state.serialize(serializer)
    }
}
impl<'de, B: Board> Deserialize<'de> for SviCore<B> {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::from_state(SviState::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}

impl<B: Board> SviCore<B> {
    /// Create a new SVI-328 with the given system ROM (32 KB).
    #[must_use]
    pub fn new(rom: Vec<u8>) -> Self {
        let state = SviState {
            cpu: Z80::new(),
            vdp: Tms9918::new(B::VDP_REGION),
            psg: Ay3_8912::new(AY_CLOCK_HZ, AY_SAMPLE_RATE, AY_SAMPLES_PER_FRAME),
            ppi: Ppi8255::new(),
            rom,
            ram: vec![0u8; 65536],
            cart_rom: Vec::new(),
            bank_ram_low: false,
            bank_cart: false,
            psg_reg_select: 0,
            keyboard: [0xFF; NUM_KEY_ROWS],
            joy_dirs: 0xFF,
            joy_fire: [false; 2],
            cassette: Cassette::default(),
            region: B::REGION,
            cpu_tstates: 0,
            cadence_hc: 0,
            tstates_per_frame: B::TSTATES_PER_FRAME,
            vdp_phase: 0,
            psg_phase: 0,
            frame_count: 0,
            native_frame_ready: false,
            io_trace: None,
            ay_watch: None,
        };
        Self {
            state,
            board: std::marker::PhantomData,
        }
    }

    /// Original system ROM retained by the live machine.
    #[must_use]
    pub fn firmware_rom(&self) -> &[u8] {
        &self.state.rom
    }

    /// Original cartridge ROM retained by the live machine.
    #[must_use]
    pub fn cartridge_rom(&self) -> &[u8] {
        &self.state.cart_rom
    }

    /// Capture immutable tape media without changing the live transport.
    #[must_use]
    pub fn cassette_media(&self) -> Option<CassetteMedia> {
        self.state.cassette.media()
    }

    /// Put recovered tape media in a rewound, stopped deck.
    pub fn insert_cassette_media(&mut self, media: &CassetteMedia) {
        self.state.cassette.insert_media(media);
    }

    /// Insert a cartridge ROM. PSG R15 controls whether it is visible;
    /// insertion alone does not change a hardware bank latch.
    pub fn insert_cart(&mut self, rom: Vec<u8>) {
        self.state.cart_rom = rom;
    }

    /// Insert decoded CAS payload blocks and leave the transport stopped.
    pub fn insert_cassette(&mut self, blocks: &[Vec<u8>]) {
        self.state.cassette.insert(blocks);
    }

    /// Run one frame and return T-states consumed.
    /// Stop on the video chip's next raster wrap, not a rounded CPU budget.
    /// The two-field safety budget reports false rather than inventing a frame.
    pub fn run_native_frame(&mut self) -> (u64, bool) {
        let start = self.state.cpu_tstates;
        self.state.native_frame_ready = false;
        while !self.state.native_frame_ready
            && self.state.cpu_tstates - start < 2 * self.state.tstates_per_frame
        {
            self.tick_tstate();
        }
        if self.state.native_frame_ready {
            self.state.frame_count += 1;
        }
        (
            self.state.cpu_tstates - start,
            self.state.native_frame_ready,
        )
    }

    pub fn run_frame(&mut self) -> u64 {
        let target = self.state.cpu_tstates + self.state.tstates_per_frame;
        while self.state.cpu_tstates < target {
            self.tick_tstate();
        }
        self.state.frame_count += 1;
        self.state.tstates_per_frame
    }

    /// Advance one Z80 T-state. The cadence — two CPU half-cycles, the
    /// interrupt pin fed before each — is `emu198x-z80-cadence`'s; the VDP
    /// ticks on every half-cycle after the CPU, and the PSG, cassette and
    /// T-state counter once per T-state, as the hand-rolled loop always
    /// had them.
    fn tick_tstate(&mut self) {
        self.advance_tstates(1);
    }

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
                // SVI-328 system ROM sets IM 1 — INT fetches RST 38h
                // via the floating bus.
                self.state.cpu.data_in = 0xFF;
            }
            None => {}
        }
    }

    fn mem_read(&self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x7FFF => {
                if self.state.bank_ram_low {
                    self.state.ram[addr as usize]
                } else {
                    self.state.rom.get(addr as usize).copied().unwrap_or(0xFF)
                }
            }
            0x8000..=0xBFFF => {
                if self.state.bank_cart && !self.state.cart_rom.is_empty() {
                    self.state
                        .cart_rom
                        .get((addr - 0x8000) as usize)
                        .copied()
                        .unwrap_or(0xFF)
                } else {
                    self.state.ram[addr as usize]
                }
            }
            0xC000..=0xFFFF => self.state.ram[addr as usize],
        }
    }

    fn mem_write(&mut self, addr: u16, value: u8) {
        match addr {
            0x0000..=0x7FFF => {
                if self.state.bank_ram_low {
                    self.state.ram[addr as usize] = value;
                }
            }
            0x8000..=0xBFFF => {
                if !self.state.bank_cart {
                    self.state.ram[addr as usize] = value;
                }
            }
            0xC000..=0xFFFF => self.state.ram[addr as usize] = value,
        }
    }

    fn io_read(&mut self, port: u16) -> u8 {
        match port as u8 {
            // The VDP is written at $80/$81 but *read* at $84/$85. This split
            // matters: the vblank ISR reads the status at $85 to acknowledge
            // and clear the interrupt. (MAME `svi318` io map.)
            0x84 => self.state.vdp.read_data(),
            0x85 => self.state.vdp.read_status(),
            // PSG data read at $90. Register 14 (port A) carries the two
            // joysticks' directions, so present them before the read: P1 in
            // the low nibble, P2 in the high nibble (active low). Unlike the
            // MSX, the SVI reads both sticks at once — no port select.
            0x90 => {
                if self.state.psg.selected_register() == 14 {
                    self.state.psg.set_port_a_input_mask(self.state.joy_dirs);
                }
                self.state.psg.read_data()
            }
            // PPI port A read ($98): paddle (0-3), joystick fire buttons (4-5,
            // active low), cassette (6-7). With no cassette attached the
            // no-cassette bit reads high and the cassette input low, so the
            // idle byte is 0x7F; a held fire button pulls its bit (PB1 = bit 4,
            // PB2 = bit 5) low. (MAME `svi318` `ppi_port_a_r`.)
            0x98 => {
                let mut data = if self.state.cassette.is_present() {
                    0x3F
                } else {
                    0x7F
                };
                if self.state.cassette.input_high() {
                    data |= 0x80;
                }
                if self.state.joy_fire[0] {
                    data &= !0x10;
                }
                if self.state.joy_fire[1] {
                    data &= !0x20;
                }
                data
            }
            // PPI port B read ($99): keyboard column data for the row selected
            // via port C.
            0x99 => {
                let row = usize::from(self.state.ppi.port_c() & 0x0F);
                self.state.keyboard.get(row).copied().unwrap_or(0xFF)
            }
            // PPI port C read ($9A).
            0x9A => self.state.ppi.read(2),
            _ => 0xFF,
        }
    }

    fn io_write(&mut self, port: u16, value: u8) {
        match port as u8 {
            0x80 => self.state.vdp.write_data(value),
            0x81 => self.state.vdp.write_control(value),
            // PSG: address latch at $88, data at $8C.
            0x88 => {
                self.state.psg_reg_select = value & 0x0F;
                self.state.psg.select_register(value);
            }
            0x8C => {
                if let Some(w) = &mut self.state.ay_watch {
                    w.record(
                        self.state.cpu.regs.pc,
                        self.state.psg.selected_register(),
                        value,
                    );
                }
                self.state.psg.write_data(value);
                // The AY-3-8910's port B (R15) is the memory-bank register.
                // D0 (`CART`) low enables the cartridge bank; D1 (`BK21`) low
                // banks the lower 32 KB of RAM in over BASIC ROM. D2-D4 and
                // D6-D7 address expansion hardware that the base machine does
                // not contain. Service manual PSG Port B table; MAME
                // `svi318.cpp` `bank_w`.
                if self.state.psg_reg_select == 0x0F {
                    self.state.bank_cart = value & 0x01 == 0;
                    self.state.bank_ram_low = value & 0x02 == 0;
                }
            }
            // PPI ports: A $94, B $95, C $96, control $97. Port C's low nibble
            // selects the keyboard row.
            0x94 => self.state.ppi.write(0, value),
            0x95 => self.state.ppi.write(1, value),
            0x96 | 0x97 => {
                self.state.ppi.write(port as u8 - 0x94, value);
                // PC4 is active-low motor control; PC5 is cassette output and
                // PC6 speaker enable, both intentionally read-only for now.
                // Read the line, not the written byte: a mode word or a
                // bit set/reset on $97 moves it too.
                self.state
                    .cassette
                    .set_motor(self.state.ppi.port_c() & 0x10 == 0);
            }
            _ => {}
        }
    }

    /// Framebuffer (256×192 ARGB32).
    #[must_use]
    pub fn framebuffer(&self) -> &[u32] {
        self.state.vdp.framebuffer()
    }

    /// Finish the current PSG frame and return its mono host samples.
    pub fn take_audio_buffer(&mut self) -> Vec<f32> {
        let mut out = vec![0.0; AY_SAMPLES_PER_FRAME];
        self.state.psg.end_frame(&mut out);
        out
    }

    /// Press a key at the given (row, bit).
    pub fn press_key(&mut self, row: usize, bit: u8) {
        if row < self.state.keyboard.len() && bit < 8 {
            self.state.keyboard[row] &= !(1 << bit);
        }
    }

    /// Release a key at the given (row, bit).
    pub fn release_key(&mut self, row: usize, bit: u8) {
        if row < self.state.keyboard.len() && bit < 8 {
            self.state.keyboard[row] |= 1 << bit;
        }
    }

    /// Set the digital joystick state for `port` (1 or 2). The directions are
    /// read on PSG port A — player 1 in the low nibble, player 2 in the high
    /// nibble (active low: bit 0/4 up, 1/5 down, 2/6 left, 3/7 right) — and the
    /// fire button on PPI port A bit 4 (player 1) / bit 5 (player 2). Both
    /// joysticks are read independently. Out-of-range ports clamp to the pair.
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
        let mut nibble = 0x0Fu8;
        for (pressed, bit) in [(up, 0x01), (down, 0x02), (left, 0x04), (right, 0x08)] {
            if pressed {
                nibble &= !bit;
            }
        }
        let shift = idx * 4;
        self.state.joy_dirs = (self.state.joy_dirs & !(0x0F << shift)) | (nibble << shift);
        self.state.joy_fire[idx] = fire;
    }

    /// The active-low PSG port-A byte the joysticks present (both players'
    /// directions). For inspection and host-side input wiring.
    #[must_use]
    pub fn joystick_dirs(&self) -> u8 {
        self.state.joy_dirs
    }

    /// Observe the column bits for a keyboard matrix row (active-low:
    /// a `0` bit is a pressed key). Returns `0xFF` for an out-of-range
    /// row.
    #[must_use]
    pub fn key_row(&self, row: usize) -> u8 {
        self.state.keyboard.get(row).copied().unwrap_or(0xFF)
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
    pub fn region(&self) -> SviRegion {
        self.state.region
    }

    /// CPU T-states executed since power-on.
    #[must_use]
    pub fn cpu_tstates(&self) -> u64 {
        self.state.cpu_tstates
    }

    /// Frame count since power-on.
    #[must_use]
    pub fn frame_count(&self) -> u64 {
        self.state.frame_count
    }

    /// Memory-control register state — `(ram_low, cart)`.
    #[must_use]
    pub fn memory_control(&self) -> (bool, bool) {
        (self.state.bank_ram_low, self.state.bank_cart)
    }
}

impl<B: Board> Z80Machine for SviCore<B> {
    fn hc(&self) -> u64 {
        self.state.cadence_hc
    }

    fn hc_mut(&mut self) -> &mut u64 {
        &mut self.state.cadence_hc
    }

    fn feed_interrupt_pins(&mut self) {
        // VDP INT → Z80 IRQ, fed before the tick. The Z80 samples
        // `/INT` at an instruction boundary during its own tick, so
        // setting the line afterwards hands it the previous
        // half-cycle's state.
        self.state.cpu.irq = self.state.vdp.interrupt;
    }

    fn tick_cpu_and_bus(&mut self) {
        self.state.cpu.tick();
        self.handle_bus();
    }

    fn tick_chips_halfcycle(&mut self) {
        self.state.vdp_phase += VDP_DOT_PHASE_NUMERATOR;
        while self.state.vdp_phase >= VDP_DOT_PHASE_DENOMINATOR {
            self.state.native_frame_ready |= self.state.vdp.tick();
            self.state.vdp_phase -= VDP_DOT_PHASE_DENOMINATOR;
        }
    }

    fn tick_chips(&mut self) {
        self.state.psg_phase ^= 1;
        if self.state.psg_phase == 0 {
            self.state.psg.tick();
        }

        self.state.cassette.tick_tstate();

        self.state.cpu_tstates += 1;
    }
}

impl<B: Board> emu198x_zilog_z80::Z80Stepper for SviCore<B> {
    fn z80_instructions_retired(&self) -> u64 {
        self.state.cpu.instructions_retired()
    }

    fn step_tick(&mut self) {
        self.tick_tstate();
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

impl<B: Board> SviCore<B> {
    /// Observe one byte on the bus without side effects.
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
        self.state.io_trace = Some(Vec::new());
    }

    /// Stop tracing and return the captured I/O events.
    pub fn take_io_trace(&mut self) -> Vec<IoEvent> {
        self.state.io_trace.take().unwrap_or_default()
    }

    /// Start (or restart) capturing PSG register writes for `watch_ay_*`.
    /// Returns the log capacity (max records before writes are dropped).
    pub fn start_ay_write_watch(&mut self) -> u32 {
        let watch = AyWriteWatch::new();
        let cap = watch.cap() as u32;
        self.state.ay_watch = Some(watch);
        cap
    }

    /// Stop capturing PSG writes and drop the log.
    pub fn stop_ay_write_watch(&mut self) {
        self.state.ay_watch = None;
    }

    /// Captured PSG writes since the last `start_ay_write_watch`, or
    /// `None` when the watch is disarmed.
    #[must_use]
    pub fn ay_write_watch_records(&self) -> Option<&[AyWriteRecord]> {
        self.state.ay_watch.as_ref().map(AyWriteWatch::records)
    }

    /// Drop captured PSG writes while leaving the watch armed.
    pub fn clear_ay_write_watch_records(&mut self) {
        if let Some(w) = &mut self.state.ay_watch {
            w.clear();
        }
    }
}

#[cfg(test)]
mod tests;
