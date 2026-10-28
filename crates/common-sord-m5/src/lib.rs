//! Shared Sord M5 wiring, statically bound by each concrete board crate: pin-driven Z80, TMS9918-family video,
//! SN76489 sound and Z80 CTC.
//!
//! # Memory map
//!
//! | Range         | Contents                              |
//! |---------------|---------------------------------------|
//! | `$0000-$1FFF` | Monitor ROM (8 KB; BASIC is a cartridge) |
//! | `$2000-$6FFF` | Cartridge ROM (up to 20 KB)           |
//! | `$7000-$7FFF` | 4 KB system RAM                       |
//! | `$8000-$BFFF` | Optional cart RAM (up to 16 KB)       |
//! | `$C000-$FFFF` | Unmapped (returns `$FF`)              |
//!
//! # I/O map
//!
//! The low address byte selects sixteen-port blocks. The GA015 decoder
//! in Sord's M5C-01 schematic has no A0-A3 inputs. The CTC selects channels
//! with A1/A0; the VDP selects data versus control/status with A0.
//! MAME `src/mame/sord/m5.cpp::m5_io` applies the same aliases.
//!
//! | Port block | R/W   | Function                                |
//! |------------|-------|-----------------------------------------|
//! | `$00-$0F`  | r/w   | Z80 CTC (channel = port & 3)            |
//! | `$10-$1F`  | r/w   | VDP (even data, odd control/status)     |
//! | `$20-$2F`  | write | SN76489 PSG                             |
//! | `$30-$3F`  | read  | Keyboard rows 0-6; joystick at index 7  |
//!
//! Sord BASIC-G Appendix H (page 279) lists the canonical ports. Scan
//! provenance and the schematic interpretation are recorded in the shared
//! primary library, `reference/by-system/sord-m5/basic-g-io-map.md`.
//! The `$30` expansion paging latch, `$40` printer data and `$50` cassette /
//! status / printer control hardware are not implemented here.
//!
//! # Keyboard and interrupts
//!
//! Seven active-high rows are read directly at `$30-$36`, mirrored at
//! `$38-$3E`; there is no row-write strobe. Joystick directions occupy
//! `$37` / `$3F`. The host key mapping is in `runtime-sord-m5::input`.
//! The VDP's active-low interrupt pin feeds CTC channel 3's trigger input.
//! The CTC supplies the Z80 IRQ and IM 2 vector; it encodes the requesting
//! channel in vector bits 2:1. RETI releases its daisy-chain service state.
//!
//! # Clock model
//!
//! The shared Z80 cadence driver advances two CPU half-cycles per T-state.
//! Each T-state advances CTC and PSG once and the VDP by a 3:2 dot ratio.
//! The implemented CPU/PSG rate is 3,579,545 Hz in both regions, with
//! 228 T-states per scanline and 262 / 313 scanlines per frame respectively.
//! These are the implemented rates, not a claim of hardware-measured phase.

use emu198x_ti_sn76489::{NoiseLfsr, Sn76489};
pub use emu198x_ti_tms9918::Tms9918;
pub use emu198x_ti_tms9918::VdpRegion;
use emu198x_z80_cadence::Z80Machine;
use emu198x_zilog_z80::BusOp;
pub use emu198x_zilog_z80::Z80;
pub use emu198x_zilog_z80_ctc::Ctc;
use serde::{Deserialize, Serialize};
use serde_big_array::BigArray;

const VDP_DOT_PHASE_NUMERATOR: u32 = 3;
const VDP_DOT_PHASE_DENOMINATOR: u32 = 2;
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
/// using the channel-3 vector. Handler addresses come from the guest
/// vector table, not from the motherboard wiring.
pub const VDP_INT_CTC_CHANNEL: u8 = 3;

/// Sord M5 region.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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

/// Version-5-compatible live state. Board constructors validate its identity.
#[derive(Serialize, Deserialize)]
pub struct M5State {
    cpu: Z80,
    vdp: Tms9918,
    psg: Sn76489,
    rom: Vec<u8>,
    cart_rom: Vec<u8>,
    cart_ram: Vec<u8>,
    #[serde(with = "BigArray")]
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
    #[serde(skip)]
    io_trace: Option<Vec<IoEvent>>,
    region: M5Region,
    cpu_tstates: u64,
    /// The cadence driver's half-cycle phase; a restore lands on a T-state
    /// boundary, so it is not part of the snapshot.
    #[serde(skip)]
    cadence_hc: u64,
    tstates_per_frame: u64,
    vdp_phase: u32,
    frame_count: u64,
    /// Transient observation for a bounded native-frame request.
    #[serde(skip)]
    native_frame_ready: bool,
    /// Count of IM 2 interrupt acknowledgements taken — a diagnostic for
    /// whether the VDP/CTC frame interrupt is being delivered.
    irq_acks: u64,
    /// Per-CTC-channel acknowledgement counts, decoded from the IM 2 vector.
    /// Diagnostic for *which* channel is driving interrupts — the VDP-fed
    /// channel ([`VDP_INT_CTC_CHANNEL`]) should run at the frame rate.
    irq_acks_by_channel: [u64; 4],
}

/// Fixed wiring supplied by a concrete machine crate; no model switch runs here.
pub trait Board {
    const REGION: M5Region;
    const VDP_REGION: VdpRegion;
    const PSG_CLOCK_HZ: u32;
    const TSTATES_PER_FRAME: u64;
    const FRAMEBUFFER_HEIGHT: u32;
}
/// Shared wiring engine, statically bound to one board.
pub struct M5Core<B: Board> {
    state: M5State,
    board: std::marker::PhantomData<B>,
}
impl M5State {
    #[must_use]
    pub fn region(&self) -> M5Region {
        self.region
    }
}
impl<B: Board> M5Core<B> {
    /// Validate an existing state without reinitialising any chip.
    pub fn from_state(state: M5State) -> Result<Self, &'static str> {
        if state.region != B::REGION
            || state.tstates_per_frame != B::TSTATES_PER_FRAME
            || state.vdp.framebuffer_height() != B::FRAMEBUFFER_HEIGHT
        {
            return Err("snapshot video/region does not match this Sord M5 board");
        }
        Ok(Self {
            state,
            board: std::marker::PhantomData,
        })
    }
    #[must_use]
    pub fn into_state(self) -> M5State {
        self.state
    }
}
impl<B: Board> Serialize for M5Core<B> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.state.serialize(serializer)
    }
}
impl<'de, B: Board> Deserialize<'de> for M5Core<B> {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::from_state(M5State::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}
impl<B: Board> M5Core<B> {
    /// Original cartridge image retained by the live machine.
    #[must_use]
    pub fn cartridge_rom(&self) -> &[u8] {
        &self.state.cart_rom
    }

    /// Original firmware image retained by the live machine.
    #[must_use]
    pub fn firmware_rom(&self) -> &[u8] {
        &self.state.rom
    }

    /// Construct this compile-time board using its fixed wiring constants.
    #[must_use]
    pub fn new(rom: Vec<u8>, cart_rom: Vec<u8>) -> Self {
        let state = M5State {
            cpu: Z80::new(),
            vdp: Tms9918::new(B::VDP_REGION),
            psg: Sn76489::new(B::PSG_CLOCK_HZ, NoiseLfsr::Tms15),
            rom,
            cart_rom,
            cart_ram: Vec::new(),
            ram: [0; 4096],
            key_matrix: [0x00; NUM_KEY_ROWS],
            joystick: 0x00,
            ctc: Ctc::new(),
            prev_opcode_ed: false,
            io_trace: None,
            region: B::REGION,
            cpu_tstates: 0,
            cadence_hc: 0,
            tstates_per_frame: B::TSTATES_PER_FRAME,
            vdp_phase: 0,
            frame_count: 0,
            native_frame_ready: false,
            irq_acks: 0,
            irq_acks_by_channel: [0; 4],
        };
        Self {
            state,
            board: std::marker::PhantomData,
        }
    }

    /// Per-CTC-channel IM 2 acknowledgement counts (index = channel 0..3).
    /// Diagnostic for which channel drives the Z80 interrupt.
    #[must_use]
    pub fn irq_acks_by_channel(&self) -> [u64; 4] {
        self.state.irq_acks_by_channel
    }

    /// Number of IM 2 interrupt acknowledgements taken since power-on.
    /// Diagnostic for VDP/CTC frame-interrupt delivery.
    #[must_use]
    pub fn irq_acks(&self) -> u64 {
        self.state.irq_acks
    }

    /// Allocate (or resize) cart RAM at `$8000-$BFFF`. Some carts
    /// (FALC and friends) need extra RAM beyond the built-in 4 KB.
    pub fn set_cart_ram_size(&mut self, bytes: usize) {
        let capped = bytes.min(0x4000);
        self.state.cart_ram.resize(capped, 0);
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
    /// interrupt pin fed before each — is `emu198x-z80-cadence`'s; the chips
    /// below tick once per T-state after the CPU, as the hand-rolled loop
    /// always had them.
    fn tick_tstate(&mut self) {
        self.advance_tstates(1);
    }

    fn handle_bus(&mut self) {
        match self.state.cpu.bus_request() {
            Some(BusOp::MemRead) => {
                let byte = self.mem_read(self.state.cpu.addr);
                self.state.cpu.data_in = byte;
                // Watch the opcode stream for RETI (ED 4D) so the CTC
                // daisy chain can release its in-service channel. M1 marks
                // an opcode fetch; the ED prefix and its 4D second byte are
                // consecutive M1 fetches.
                if self.state.cpu.m1 {
                    if self.state.prev_opcode_ed && byte == 0x4D {
                        self.state.ctc.reti();
                    }
                    self.state.prev_opcode_ed = byte == 0xED;
                }
            }
            Some(BusOp::MemWrite) => {
                self.mem_write(self.state.cpu.addr, self.state.cpu.data);
            }
            Some(BusOp::IoRead) => {
                self.state.cpu.data_in = self.io_read(self.state.cpu.addr);
            }
            Some(BusOp::IoWrite) => {
                self.io_write(self.state.cpu.addr, self.state.cpu.data);
            }
            Some(BusOp::IntAck) => {
                // The Monitor ROM runs IM 2 with `I = $70`. The CTC
                // supplies the low vector byte for the requesting channel;
                // the Z80 forms the table address `(I << 8) | vector` and
                // fetches the handler. The CTC inserts the requesting
                // channel into vector bits 2:1 (channel 3 contributes $06).
                let vector = self.state.ctc.acknowledge();
                self.state.cpu.data_in = vector;
                self.state.irq_acks += 1;
                // The CTC encodes the requesting channel in vector bits 2:1.
                let channel = usize::from((vector >> 1) & 0x03);
                self.state.irq_acks_by_channel[channel] += 1;
            }
            None => {}
        }
    }

    fn mem_read(&self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x1FFF => self.state.rom.get(addr as usize).copied().unwrap_or(0xFF),
            0x2000..=0x6FFF => self
                .state
                .cart_rom
                .get((addr - 0x2000) as usize)
                .copied()
                .unwrap_or(0xFF),
            0x7000..=0x7FFF => self.state.ram[(addr & 0x0FFF) as usize],
            0x8000..=0xBFFF => self
                .state
                .cart_ram
                .get((addr - 0x8000) as usize)
                .copied()
                .unwrap_or(0xFF),
            _ => 0xFF,
        }
    }

    fn mem_write(&mut self, addr: u16, value: u8) {
        match addr {
            0x7000..=0x7FFF => self.state.ram[(addr & 0x0FFF) as usize] = value,
            0x8000..=0xBFFF => {
                let idx = (addr - 0x8000) as usize;
                if let Some(slot) = self.state.cart_ram.get_mut(idx) {
                    *slot = value;
                }
            }
            _ => {}
        }
    }

    fn io_read(&mut self, port: u16) -> u8 {
        let pc = self.state.cpu.regs.pc;
        let p = port as u8;
        let value = match p & 0xF0 {
            // CTC at $00-$03 (channel = CS1,CS0 = A1,A0); read = live counter.
            0x00 => self.state.ctc.read(p & 0x03),
            // TMS9918A at $10 (data) / $11 (status).
            0x10 => {
                if p & 1 == 0 {
                    self.state.vdp.read_data()
                } else {
                    self.state.vdp.read_status()
                }
            }
            // Keyboard rows Y0-Y6 at $30-$36 and the joystick at $37, each read
            // directly (active-high), with the A3 mirror folding $38-$3F onto
            // $30-$37. MAME `sord/m5.cpp`: `portr("Y0")`..`"Y6"` + `portr("JOY")`.
            // There is no row strobe — the donor's $30-write / $40-read scheme
            // was fiction ($40 is the Centronics data latch, write-only).
            0x30 => {
                let sel = (p & 0x07) as usize;
                if sel < NUM_KEY_ROWS {
                    self.state.key_matrix[sel]
                } else {
                    self.state.joystick
                }
            }
            _ => 0xFF,
        };
        if let Some(trace) = &mut self.state.io_trace {
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
        let pc = self.state.cpu.regs.pc;
        let p = port as u8;
        if let Some(trace) = &mut self.state.io_trace {
            trace.push(IoEvent {
                pc,
                port: p,
                value,
                write: true,
            });
        }
        match p & 0xF0 {
            // CTC at $00-$03 (channel = CS1,CS0 = A1,A0).
            0x00 => self.state.ctc.write(p & 0x03, value),
            // TMS9918A at $10 (data) / $11 (control).
            0x10 => {
                if p & 1 == 0 {
                    self.state.vdp.write_data(value);
                } else {
                    self.state.vdp.write_control(value);
                }
            }
            // SN76489A PSG at $20.
            0x20 => self.state.psg.write(value),
            // $30 write is the 64KBF memory-paging latch (expansion RAM, not
            // modelled); $40 is the Centronics data latch. Neither touches the
            // keyboard, which is read-only at $30-$36.
            _ => {}
        }
    }

    /// Framebuffer (ARGB32).
    #[must_use]
    pub fn framebuffer(&self) -> &[u32] {
        self.state.vdp.framebuffer()
    }

    /// Framebuffer width.
    #[must_use]
    pub fn framebuffer_width(&self) -> u32 {
        self.state.vdp.framebuffer_width()
    }

    /// Framebuffer height.
    #[must_use]
    pub fn framebuffer_height(&self) -> u32 {
        self.state.vdp.framebuffer_height()
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
        self.state.io_trace = Some(Vec::new());
    }

    /// Stop tracing and return the captured I/O events.
    pub fn take_io_trace(&mut self) -> Vec<IoEvent> {
        self.state.io_trace.take().unwrap_or_default()
    }

    /// Run whole instructions until the CPU reaches `target_pc` or
    /// `max_tstates` elapse. Returns `(tstates_consumed, reached)`.
    pub fn run_until_pc(&mut self, target_pc: u16, max_tstates: u64) -> (u64, bool) {
        let start = self.state.cpu_tstates;
        while self.state.cpu_tstates - start < max_tstates {
            self.tick_tstate();
            if self.state.cpu.instruction_complete() && self.state.cpu.regs.pc == target_pc {
                return (self.state.cpu_tstates - start, true);
            }
        }
        (self.state.cpu_tstates - start, false)
    }

    /// Take the accumulated PSG audio buffer.
    pub fn take_audio_buffer(&mut self) -> Vec<f32> {
        self.state.psg.take_buffer()
    }

    /// Press a key at the given matrix (row, bit) cell. Active-high: a pressed
    /// key sets its bit (the BIOS reads `$30`-`$36` and sees the 1).
    pub fn press_key(&mut self, row: usize, bit: u8) {
        if row < self.state.key_matrix.len() && bit < 8 {
            self.state.key_matrix[row] |= 1 << bit;
        }
    }

    /// Release a key at the given matrix (row, bit) cell. Active-high: clearing
    /// the bit returns the cell to its idle (released) state.
    pub fn release_key(&mut self, row: usize, bit: u8) {
        if row < self.state.key_matrix.len() && bit < 8 {
            self.state.key_matrix[row] &= !(1 << bit);
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
        self.state.joystick = (self.state.joystick & !(0x0F << shift)) | (nibble << shift);
    }

    /// The joystick directions byte read at `$37`. For inspection and
    /// host-side input wiring.
    #[must_use]
    pub fn joystick_byte(&self) -> u8 {
        self.state.joystick
    }

    /// Mutable keyboard matrix (active-high; 1 = pressed).
    pub fn key_matrix_mut(&mut self) -> &mut [u8; NUM_KEY_ROWS] {
        &mut self.state.key_matrix
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

    /// CTC reference (for observation / debug).
    #[must_use]
    pub fn ctc(&self) -> &Ctc {
        &self.state.ctc
    }

    /// Region.
    #[must_use]
    pub fn region(&self) -> M5Region {
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

    /// CPU T-states in one frame for the current region.
    #[must_use]
    pub fn tstates_per_frame(&self) -> u64 {
        self.state.tstates_per_frame
    }
}

impl<B: Board> Z80Machine for M5Core<B> {
    fn hc(&self) -> u64 {
        self.state.cadence_hc
    }

    fn hc_mut(&mut self) -> &mut u64 {
        &mut self.state.cadence_hc
    }

    // The VDP is *not* re-denominated to half-cycles, because it does not
    // reach the CPU directly — the CTC stands between them and ticks once
    // per T-state, so a finer VDP interleave could not change when the
    // interrupt arrives.
    fn feed_interrupt_pins(&mut self) {
        self.state.cpu.irq = self.state.ctc.interrupt();
    }

    fn tick_cpu_and_bus(&mut self) {
        self.state.cpu.tick();
        self.handle_bus();
    }

    fn tick_chips(&mut self) {
        self.state.vdp_phase += VDP_DOT_PHASE_NUMERATOR;
        while self.state.vdp_phase >= VDP_DOT_PHASE_DENOMINATOR {
            self.state.native_frame_ready |= self.state.vdp.tick();
            self.state.vdp_phase -= VDP_DOT_PHASE_DENOMINATOR;
        }

        self.state.psg.tick();

        // VDP /INT feeds the CTC channel's CLK/TRG input; the CTC ticks
        // on the CPU clock and edge-counts those frame interrupts. The
        // CTC's INT output (not the raw VDP line) drives the Z80 IRQ pin,
        // so the interrupt is vectored through IM 2.
        //
        // The line is *inverted* into the trigger, matching MAME
        // (`vdp.int_callback().set(m_ctc, trg3).invert()`). The BIOS arms this
        // channel for the falling edge; with inversion that falling edge lands
        // at VBlank (when the active-low /INT asserts), which is the frame sync
        // the game waits on. Feeding the raw line instead put the falling edge
        // at status-read time — inside the very handler the interrupt is meant
        // to trigger — so the channel deadlocked and round logic never advanced.
        self.state
            .ctc
            .set_trg(VDP_INT_CTC_CHANNEL, !self.state.vdp.interrupt);
        self.state.ctc.tick();

        self.state.cpu_tstates += 1;
    }
}

impl<B: Board> emu198x_zilog_z80::Z80Stepper for M5Core<B> {
    fn z80_instructions_retired(&self) -> u64 {
        self.state.cpu.instructions_retired()
    }

    fn step_tick(&mut self) {
        self.tick_tstate();
    }
}

#[cfg(test)]
mod tests;
