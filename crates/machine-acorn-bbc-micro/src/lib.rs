//! BBC Micro Model B machine wiring.
//!
//! Fresh-write against the workspace pin-driven bus pattern (RULES.md
//! rule 6). The donor at
//! `Emu198x-Oldest/crates/machine-acorn-bbc-micro` used the
//! deprecated `emu_core::Bus` callback and could not port directly;
//! this file uses it as a system spec — SHEILA I/O page at
//! `$FE00-$FEFF` with 6845 CRTC, Video ULA, ROM bank register,
//! System VIA, User VIA; sideways ROM slot at `$8000-$BFFF`;
//! addressable latch IC32 driven via System VIA port B; SN76489
//! PSG fed via the System VIA + latch — but the wiring is written
//! against `mos-6502`'s public pin fields.
//!
//! # The BBC Micro Model B
//!
//! The BBC Micro (1981) by Acorn Computers is one of the most
//! influential educational computers ever made. Designed in
//! response to the BBC's Computer Literacy Project, it became the
//! UK education-and-home-computing standard for the 1980s.
//!
//! - **CPU:** 6502A @ 2 MHz, dropping to 1 MHz for the 1 MHz-bus
//!   peripherals (FRED `$FC00`, JIM `$FD00`, and the slow SHEILA
//!   devices — CRTC, ACIA, both VIAs, ADC). RAM and ROM stay at 2 MHz,
//!   so unlike the Electron there is no display-fetch contention. The
//!   frame is a fixed 312 × 128 master ticks at 2 MHz; a 1 MHz-bus
//!   access costs two of them. Matches MAME `bbc_state::set_cpu_clock`.
//! - **CRTC:** Motorola 6845
//! - **Video ULA:** Acorn custom (256-colour-pool→16-entry palette,
//!   bpp + fast-clock selection)
//! - **PSG:** SN76489 @ 4 MHz, fed via System VIA + addressable
//!   latch IC32
//! - **VIAs:** Two MOS 6522s — System VIA at `$FE40` (sound,
//!   keyboard, IC32) and User VIA at `$FE60` (Centronics, user port)
//! - **RAM:** 32 KB at `$0000-$7FFF`
//! - **MOS ROM:** 16 KB at `$C000-$FFFF`
//! - **Sideways ROMs:** 16 banks × 16 KB at `$8000-$BFFF`, banked
//!   by `$FE30`
//!
//! # Memory map
//!
//! | Range         | Contents                                       |
//! |---------------|------------------------------------------------|
//! | `$0000-$7FFF` | 32 KB RAM                                      |
//! | `$8000-$BFFF` | Sideways ROM slot (banked via `$FE30`)         |
//! | `$C000-$FBFF` | MOS ROM                                        |
//! | `$FC00-$FCFF` | FRED — 1 MHz expansion                         |
//! | `$FD00-$FDFF` | JIM — 1 MHz expansion                          |
//! | `$FE00-$FEFF` | SHEILA — internal I/O (see below)              |
//! | `$FF00-$FFFF` | MOS ROM (reset / IRQ / NMI vectors)            |
//!
//! ## SHEILA register map
//!
//! | Range         | Device                                           |
//! |---------------|--------------------------------------------------|
//! | `$FE00`/`02`  | 6845 CRTC address register                       |
//! | `$FE01`/`03`  | 6845 CRTC data register                          |
//! | `$FE20`       | Video ULA control                                |
//! | `$FE21`       | Video ULA palette write                          |
//! | `$FE30`       | Sideways ROM bank select                         |
//! | `$FE40-$FE4F` | System VIA                                       |
//! | `$FE60-$FE6F` | User VIA                                         |
//!
//! # PSG path
//!
//! The SN76489 is not directly memory-mapped. The CPU writes the
//! PSG byte into System VIA port A (ORA register `$01`/`$0F`), then
//! flips IC32 latch bit 0 (the SN76489 `/WE`) via a System VIA
//! port B write. When bit 0 of the latch transitions low, the
//! current ORA value is latched into the PSG.

use common_acorn_cassette::{CassetteEvent, CassetteReceiver, TapePulse};
use mos_6502::M6502;
use mos_via_6522::Via6522;
use motorola_6845::Crtc6845;
use serde::{Deserialize, Serialize};
use serde_big_array::BigArray;
use ti_sn76489::{NoiseLfsr, Sn76489};

/// Nanoseconds per 2 MHz master tick — the cassette receiver's time base.
const NS_PER_MASTER_TICK: u64 = 500;

/// Serial ULA (`$FE10`) bit 7: cassette motor relay (1 = motor on).
const MOTOR_BIT: u8 = 0x80;

/// Framebuffer width: the 640 dots MODE 0 displays.
///
/// **Deliberately narrower than a set's window, because the 6845 blanks the
/// rest.** A PAL set shows about 52 µs, which at the BBC's 16 MHz dot clock is
/// 832 dots — but R0 gives a 128-character line and R1 displays 80 of them, so
/// 640 dots carry picture and the other 384 are non-display. The BBC has no
/// border colour register: what a set shows outside the displayed window is
/// black, not a programmable surround, so holding it would be holding black.
///
/// The #1054 audit reads this as 77% of a set's window. That is the hardware,
/// not a crop — the distinction
/// `knowledge/decisions/the-framebuffer-is-the-sets-window.md` exists to make.
/// Register values from `reference/by-system/bbc-micro/bbc-micro-reference.md`
/// §6845.
pub const FB_WIDTH: u32 = 640;

/// Framebuffer height: the 256 scan lines MODE 0-2 display.
///
/// Blanked for the same reason. R4 = 38 and R9 = 7 give a 312-line frame, of
/// which R6 = 32 character rows of 8 lines are displayed. A PAL set shows 288,
/// so the audit reads 89% — again the chip, and again black outside it.
pub const FB_HEIGHT: u32 = 256;

/// BBC Micro CPU clock: 2 MHz. Kept as a documented reference even
/// though `CYCLES_PER_FRAME` is the only derived constant the engine
/// reads today.
#[allow(dead_code)]
const CPU_CLOCK_HZ: u32 = 2_000_000;
const CYCLES_PER_FRAME: u64 = 39_936; // 312 lines × 64 µs × 2 MHz
const SCANLINES_PER_FRAME: u16 = 312;
const CYCLES_PER_LINE: u64 = 128;

const SN76489_CLOCK_HZ: u32 = 4_000_000;

/// Video ULA — palette + control register.
#[derive(Serialize, Deserialize)]
struct VideoUla {
    control: u8,
    palette: [u8; 16],
}

impl VideoUla {
    fn new() -> Self {
        // Default palette: identity with inverted physical bits.
        let mut palette = [0u8; 16];
        for (i, slot) in palette.iter_mut().enumerate() {
            *slot = (i as u8) ^ 0x07;
        }
        Self {
            control: 0,
            palette,
        }
    }

    fn write_control(&mut self, value: u8) {
        self.control = value;
    }

    fn write_palette(&mut self, value: u8) {
        let logical = (value >> 4) as usize;
        let physical = value & 0x0F;
        self.palette[logical] = physical;
    }

    /// Bits 2-3 of the control register.
    ///
    /// The Advanced User Guide (§19.1.3) calls this the number of characters
    /// per line — `11` 80, `10` 40, `01` 20, `00` 10. The ULA uses it as the
    /// divisor on its pixel clock, so it decides how finely each byte is cut
    /// up, not how many bytes a line holds. That count comes from the 6845.
    const fn pixel_rate(&self) -> u8 {
        (self.control >> 2) & 0x03
    }

    /// Pixels the serialiser draws from one byte.
    ///
    /// The ULA shifts a byte out over a character cell. The rate field says
    /// how often it steps, and the slow 6845 clock (modes 4-6) stretches the
    /// cell to twice as many pixel clocks, so the same rate yields twice the
    /// pixels. Every documented mode falls out of this, including the Advanced
    /// User Guide's own `*FX154,224` example (§19.3: slow clock, rate `00`,
    /// "PIXELS PER BYTE-1" = 1, so two):
    ///
    /// | mode | rate | clock | pixels/byte |
    /// |------|------|-------|-------------|
    /// | 0    | 11   | fast  | 8           |
    /// | 1    | 10   | fast  | 4           |
    /// | 2    | 01   | fast  | 2           |
    /// | 4, 6 | 10   | slow  | 8           |
    /// | 5    | 01   | slow  | 4           |
    const fn pixels_per_byte(&self) -> usize {
        (1usize << self.pixel_rate()) * if self.fast_clock() { 1 } else { 2 }
    }

    fn teletext(&self) -> bool {
        self.control & 0x02 != 0
    }

    const fn fast_clock(&self) -> bool {
        self.control & 0x10 != 0
    }

    fn palette_to_argb(&self, index: u8) -> u32 {
        let entry = self.palette[index as usize & 0x0F];
        // Physical colour: bits 0-2 = ~R, ~G, ~B (active-low).
        let r = if entry & 0x01 == 0 { 255 } else { 0 };
        let g = if entry & 0x02 == 0 { 255 } else { 0 };
        let b = if entry & 0x04 == 0 { 255 } else { 0 };
        0xFF00_0000 | (r << 16) | (g << 8) | b
    }
}

/// IC32 addressable latch — System VIA port B writes encode
/// `address = value & 0x07` and `data = (value >> 3) & 1`.
#[derive(Serialize, Deserialize)]
struct AddressableLatch {
    bits: [bool; 8],
}

impl AddressableLatch {
    fn new() -> Self {
        Self { bits: [false; 8] }
    }

    fn write(&mut self, address: u8, data: bool) -> Option<u8> {
        let idx = (address & 0x07) as usize;
        let prev = self.bits[idx];
        self.bits[idx] = data;
        if idx == 0 && prev && !data {
            // Bit 0 falling edge = SN76489 /WE asserted (write PSG).
            Some(0)
        } else {
            None
        }
    }
}

/// 12-bit conversion: 10 ms at the 2 MHz CPU clock.
const ADC_CONVERT_12BIT: u32 = 20_000;
/// 8-bit conversion: 4 ms at the 2 MHz CPU clock.
const ADC_CONVERT_8BIT: u32 = 8_000;

/// μPD7002 4-channel 12-bit ADC — the BBC's analogue port at SHEILA
/// `$FEC0-$FEC3` (mirrored to `$FEDF`). Each channel holds a 12-bit pot value
/// (host-set: ch0/ch1 = joystick 1 X/Y, ch2/ch3 = joystick 2 X/Y). A conversion
/// is a countdown; when it finishes the chip latches the result, asserts
/// end-of-conversion (EOC, wired to System VIA CB1 to raise the analogue
/// interrupt), and holds the "completed" status until the next conversion
/// starts.
///
/// Register model and timing adapted from the `BBCMicro_MiSTer` `upd7002.vhd`
/// reference core: status byte = `completed_n | busy_n | value[11:10] | mode |
/// flag | mux`; result high = `value[11:4]`, result low = `value[3:0] << 4`;
/// conversion takes 10 ms (12-bit) or 4 ms (8-bit).
#[derive(Serialize, Deserialize)]
struct Upd7002 {
    /// 12-bit pot values for the four channels.
    channels: [u16; 4],
    /// Currently selected channel (0-3).
    mux: u8,
    /// Conversion resolution: `false` = 8-bit, `true` = 12-bit.
    mode_12bit: bool,
    /// The spare "flag" bit, latched on write and echoed in the status byte.
    flag: bool,
    /// A conversion is in progress.
    busy: bool,
    /// A conversion has finished and not yet been superseded by a new one.
    completed: bool,
    /// CPU cycles left in the current conversion (decremented at 2 MHz).
    counter: u32,
}

impl Upd7002 {
    fn new() -> Self {
        Self {
            channels: [0x0800; 4], // mid-scale = stick centred
            mux: 0,
            mode_12bit: true,
            flag: false,
            busy: false,
            completed: false,
            counter: 0,
        }
    }

    /// The selected channel's 12-bit value.
    fn value(&self) -> u16 {
        self.channels[(self.mux & 0x03) as usize]
    }

    /// Start a conversion from a write to `$FEC0`: bits 0-1 select the channel,
    /// bit 2 is the spare flag, bit 3 picks 12-bit (`1`) vs 8-bit (`0`).
    fn write_control(&mut self, di: u8) {
        self.mux = di & 0x03;
        self.flag = di & 0x04 != 0;
        self.mode_12bit = di & 0x08 != 0;
        self.busy = true;
        self.completed = false;
        self.counter = if self.mode_12bit {
            ADC_CONVERT_12BIT
        } else {
            ADC_CONVERT_8BIT
        };
    }

    /// Read one of the four ADC registers (`reg` = low 2 bits of the address).
    fn read(&self, reg: u8) -> u8 {
        match reg & 0x03 {
            // Status: completed_n(7) busy_n(6) value[11:10](5:4) mode(3)
            // flag(2) mux(1:0). completed_n / busy_n are active low.
            0x00 => {
                let completed_n = u8::from(!self.completed) << 7;
                let busy_n = u8::from(!self.busy) << 6;
                let top2 = (((self.value() >> 10) & 0x03) as u8) << 4;
                let mode = u8::from(self.mode_12bit) << 3;
                let flag = u8::from(self.flag) << 2;
                completed_n | busy_n | top2 | mode | flag | (self.mux & 0x03)
            }
            0x01 => (self.value() >> 4) as u8, // high 8 bits
            0x02 => ((self.value() & 0x0F) as u8) << 4, // low 4 bits, left-justified
            _ => 0,
        }
    }

    /// Advance the conversion by one CPU cycle. Returns `true` on the cycle
    /// that completes a conversion — the EOC falling edge.
    fn tick(&mut self) -> bool {
        if self.busy && self.counter > 0 {
            self.counter -= 1;
            if self.counter == 0 {
                self.busy = false;
                self.completed = true;
                return true;
            }
        }
        false
    }
}

/// Teletext logical colour (0-7) to ARGB. The three bits are red, green, blue.
fn teletext_colour(c: u8) -> u32 {
    let r = u32::from(c & 0x01 != 0) * 0xFF;
    let g = u32::from(c & 0x02 != 0) * 0xFF;
    let b = u32::from(c & 0x04 != 0) * 0xFF;
    0xFF00_0000 | (r << 16) | (g << 8) | b
}

/// One row of a 2×3 mosaic graphics block as a 12-bit pattern. The block bits
/// in the code are: 0 top-left, 1 top-right, 2 mid-left, 3 mid-right,
/// 4 bottom-left, 6 bottom-right. The cell splits into a left and right half
/// (six pixels each); separated graphics blank the cell's right and bottom
/// edges.
fn mosaic_pattern(code: u8, font_row: usize, separated: bool) -> u16 {
    let (left, right, last) = match font_row {
        0..=2 => (0x01u8, 0x02u8, 2),
        3..=6 => (0x04, 0x08, 6),
        _ => (0x10, 0x40, 9),
    };
    let mut c = 0u16;
    if code & left != 0 {
        c |= 0xFC0;
    }
    if code & right != 0 {
        c |= 0x03F;
    }
    if separated {
        // Blank the right column of each half and the block's bottom row.
        c &= 0x3CF;
        if font_row == last {
            c = 0;
        }
    }
    c
}

/// Motorola 6850 ACIA — the BBC's serial chip at SHEILA `$FE08`/`$FE09`
/// (cassette + RS423). No serial peripheral is wired in this core, so the
/// receiver never fills and the transmitter is always ready; the chip sits idle
/// with TDRE set and asserts an interrupt only if the OS enables the transmit
/// interrupt (it does not at the prompt). Modelled faithfully on b-em's
/// `acia.c`: the status-register interrupt bit (`$80`) is *computed* from the
/// rx/tx interrupt conditions, not stored.
///
/// This exists because the MOS IRQ handler reads `$FE08` to decide whether the
/// ACIA interrupted; the previous `$FF` open-bus read set status bit 7, so the
/// MOS serviced a phantom serial interrupt forever and never cleared the System
/// VIA's 100 Hz timer — an interrupt storm that starved BASIC before it could
/// print its `>` prompt.
#[derive(Serialize, Deserialize)]
struct Mc6850 {
    /// Control register (interrupt enables + word format + clock divide).
    control: u8,
    /// Receive-data-register-full — set when the cassette demodulator delivers a
    /// byte; the read of the data register clears it.
    rx_full: bool,
    /// The last byte the cassette demodulator delivered, returned by a read of
    /// the data register (`$FE09`).
    rx_data: u8,
    /// Latched Data Carrier Detect. The cassette interface raises DCD once the
    /// high-tone carrier has persisted; the BBC MOS uses it to know a block is
    /// coming (the tape filing system will not leave "Searching" without it).
    /// Surfaced as status bit 2, raises the IRQ with RX interrupts enabled, and
    /// is cleared by reading the data register. Faithful to jsbeeb's `acia.js`.
    dcd: bool,
}

impl Mc6850 {
    const RDRF: u8 = 0x01; // receive data register full
    const TDRE: u8 = 0x02; // transmit data register empty
    const IRQ: u8 = 0x80; // interrupt request

    const DCD: u8 = 0x04; // data carrier detect

    fn new() -> Self {
        Self {
            control: 0,
            rx_full: false,
            rx_data: 0,
            dcd: false,
        }
    }

    /// Receive interrupt: RDRF set and RX interrupt enabled (control bit 7).
    fn rx_int(&self) -> bool {
        self.rx_full && (self.control & 0x80 != 0)
    }

    /// Carrier-detect interrupt: DCD latched, gated by the RX interrupt enable.
    fn dcd_int(&self) -> bool {
        self.dcd && (self.control & 0x80 != 0)
    }

    /// Raise Data Carrier Detect — the cassette demodulator saw sustained
    /// carrier tone.
    fn set_carrier_detect(&mut self) {
        self.dcd = true;
    }

    /// Transmit interrupt: TDRE set (always, here) and TX-interrupt mode
    /// selected (control bits 6-5 == 01).
    fn tx_int(&self) -> bool {
        (self.control & 0x60) == 0x20
    }

    fn irq(&self) -> bool {
        self.rx_int() || self.dcd_int() || self.tx_int()
    }

    /// Status register: TDRE always set (transmitter idle/ready), RDRF if a byte
    /// is waiting, DCD if carrier was detected, IRQ computed from the conditions.
    fn status(&self) -> u8 {
        let mut s = Self::TDRE;
        if self.rx_full {
            s |= Self::RDRF;
        }
        if self.dcd {
            s |= Self::DCD;
        }
        if self.irq() {
            s |= Self::IRQ;
        }
        s
    }

    fn read(&mut self, addr: u16) -> u8 {
        if addr & 1 == 0 {
            self.status()
        } else {
            // Read receive data — clears RDRF and the latched DCD (and the
            // interrupts they caused).
            self.rx_full = false;
            self.dcd = false;
            self.rx_data
        }
    }

    fn write(&mut self, addr: u16, value: u8) {
        if addr & 1 == 0 {
            // Control register. Master reset (bits 0-1 = 11) just re-idles the
            // chip; with no serial line there is nothing else to reset.
            self.control = value;
        }
        // Transmit-data write (odd) completes instantly with nothing connected,
        // so TDRE stays set — nothing to model.
    }
}

/// BBC Micro Model B machine.
#[derive(Serialize, Deserialize)]
pub struct BbcMicro {
    cpu: M6502,
    crtc: Crtc6845,
    video_ula: VideoUla,
    system_via: Via6522,
    user_via: Via6522,
    psg: Sn76489,
    #[serde(with = "BigArray")]
    ram: [u8; 32768],
    mos_rom: Vec<u8>,
    sideways_roms: Vec<Vec<u8>>,
    rom_bank: u8,
    latch: AddressableLatch,
    /// Keyboard matrix (10 columns × 8 rows), active-high.
    keyboard: [[bool; 8]; 10],
    /// SAA5050 teletext character ROM (96 glyphs × 10 rows). Empty until a
    /// font is supplied; MODE 7 then renders blank.
    teletext_font: Vec<u8>,
    framebuffer: Vec<u32>,
    cpu_cycles: u64,
    /// 2 MHz master-clock ticks since construction. The CPU runs at
    /// 2 MHz (one tick per cycle) for RAM, ROM and fast I/O, but slows
    /// to 1 MHz (two ticks) for the 1 MHz peripherals — the BBC bus
    /// contention. The frame is a fixed 312 × 128 master ticks; the CPU
    /// fits a variable number of cycles into it.
    master_ticks: u64,
    frame_count: u64,
    /// Tick at which the current frame started, and the line within it.
    ///
    /// Line boundaries used to be local to `run_frame`, so the work done at
    /// each one never happened when the debugger stepped instructions (#1202).
    frame_base: u64,
    scanline: u16,
    /// Joystick fire buttons, `[joy1, joy2]`. The two analogue joysticks each
    /// have a switch wired to System VIA port B: PB4 (joy 1) and PB5 (joy 2),
    /// both active low. Merged into the VIA input latch each tick. (The X/Y
    /// axes are read through the μPD7002 ADC — a separate path.)
    fire: [bool; 2],
    /// μPD7002 ADC — the analogue joystick X/Y axes (`$FEC0-$FEDF`).
    adc: Upd7002,
    /// 6850 ACIA — cassette / RS423 serial at `$FE08`/`$FE09`.
    acia: Mc6850,
    /// Serial ULA register (`$FE10`): RX/TX baud select, RS423/cassette select,
    /// and bit 7 the cassette motor relay. Write-only on hardware; we keep the
    /// last value to gate the cassette on the motor bit.
    serial_ula: u8,
    /// Cassette demodulator. Advances at the 2 MHz master clock while the motor
    /// relay (`$FE10` bit 7) is energised, delivering recovered bytes to the
    /// ACIA's receive register and raising its RX interrupt.
    cassette: CassetteReceiver,
    /// Whether the host has the deck running.
    ///
    /// The guest's motor relay says whether the *machine* wants the tape to
    /// move; this says whether the deck is running at all, which on real
    /// hardware is the play button. Both must be true for the tape to
    /// advance. It defaults to `true`, so a machine behaves as it always did
    /// until something drives it, and `media_transport` is what drives it --
    /// so a script can stop a tape mid-load and look at what happened
    /// (#1198).
    deck_running: bool,
}

impl BbcMicro {
    /// Create a new BBC Micro with the 16 KB MOS ROM. Sideways ROMs
    /// start empty; use [`Self::insert_rom`] to install BASIC, DFS,
    /// etc. into specific bank slots.
    #[must_use]
    pub fn new(mos_rom: Vec<u8>) -> Self {
        let mut cpu = M6502::new();
        cpu.reset();
        Self {
            cpu,
            crtc: Crtc6845::new(),
            video_ula: VideoUla::new(),
            system_via: Via6522::new(),
            user_via: Via6522::new(),
            psg: Sn76489::new(SN76489_CLOCK_HZ, NoiseLfsr::Tms15),
            ram: [0; 32768],
            mos_rom,
            sideways_roms: Vec::new(),
            rom_bank: 0,
            latch: AddressableLatch::new(),
            keyboard: [[false; 8]; 10],
            teletext_font: Vec::new(),
            framebuffer: vec![0xFF00_0000; (FB_WIDTH * FB_HEIGHT) as usize],
            cpu_cycles: 0,
            master_ticks: 0,
            frame_count: 0,
            frame_base: 0,
            scanline: 0,
            fire: [false; 2],
            adc: Upd7002::new(),
            acia: Mc6850::new(),
            serial_ula: 0,
            cassette: CassetteReceiver::new(),
            deck_running: true,
        }
    }

    /// Loads a cassette tape from a decoded UEF pulse stream, rewound to the
    /// start. The tape only advances while the motor relay is energised.
    pub fn insert_tape(&mut self, pulses: Vec<TapePulse>) {
        self.cassette.load(pulses);
    }

    /// Ejects the cassette tape.
    pub fn eject_tape(&mut self) {
        self.cassette.eject();
    }

    /// Returns `true` when a cassette tape is loaded.
    #[must_use]
    pub fn tape_loaded(&self) -> bool {
        self.cassette.is_loaded()
    }

    /// Whether the host has the deck running. See [`Self::set_deck_running`].
    #[must_use]
    pub const fn deck_running(&self) -> bool {
        self.deck_running
    }

    /// Starts or stops the deck, independently of the guest's motor relay.
    pub const fn set_deck_running(&mut self, running: bool) {
        self.deck_running = running;
    }

    /// Returns `true` when the cassette motor relay (`$FE10` bit 7) is on.
    #[must_use]
    pub fn cassette_motor_on(&self) -> bool {
        self.serial_ula & MOTOR_BIT != 0
    }

    /// Set a joystick fire button (`port` 1 or 2, `true` = pressed). The switch
    /// is read on System VIA PB4 (joy 1) / PB5 (joy 2), active low; the value is
    /// merged into the VIA input latch on the next tick. Out-of-range ports
    /// clamp to the valid pair.
    pub fn set_fire_button(&mut self, port: u8, pressed: bool) {
        self.fire[usize::from(port.clamp(1, 2) - 1)] = pressed;
    }

    /// Set an ADC channel's 12-bit pot value (`0..=0x0FFF`, clamped). The four
    /// channels are the analogue joystick axes: channel 0/1 = joystick 1 X/Y,
    /// channel 2/3 = joystick 2 X/Y. `0x0800` is centre. Out-of-range channels
    /// are ignored. The OS reads these through the μPD7002 at `$FEC0-$FEC3`.
    pub fn set_adc_channel(&mut self, channel: u8, value: u16) {
        if let Some(slot) = self.adc.channels.get_mut(channel as usize) {
            *slot = value.min(0x0FFF);
        }
    }

    /// The 12-bit pot value currently latched on an ADC channel (0-3), or 0 for
    /// an out-of-range channel. For inspection and host-side input wiring.
    #[must_use]
    pub fn adc_channel(&self, channel: u8) -> u16 {
        self.adc
            .channels
            .get(channel as usize)
            .copied()
            .unwrap_or(0)
    }

    /// Supply the SAA5050 teletext character ROM (960 bytes: 96 glyphs of
    /// 10 rows). Required for MODE 7 to render anything but a blank screen.
    pub fn set_teletext_font(&mut self, font: Vec<u8>) {
        self.teletext_font = font;
    }

    /// Install a sideways ROM into the given bank slot (0-15).
    pub fn insert_rom(&mut self, bank: usize, rom: Vec<u8>) {
        while self.sideways_roms.len() <= bank {
            self.sideways_roms.push(Vec::new());
        }
        self.sideways_roms[bank] = rom;
    }

    /// Run one PAL frame.
    pub fn run_frame(&mut self) -> u64 {
        let start = self.master_ticks;
        while self.master_ticks - start < CYCLES_PER_FRAME {
            self.tick_cpu_cycle();
        }
        CYCLES_PER_FRAME
    }

    /// Close off the scanline the machine has just finished: paint it, drive
    /// the CRTC's VSYNC into the System VIA, and step to the next.
    ///
    /// This used to live in `run_frame`'s loop, so none of it happened when
    /// the debugger stepped instructions. The VIAs and the CRTC tick per
    /// cycle, so timer interrupts were fine, but the VSYNC line into CA1 was
    /// never driven and nothing was painted -- a stepped machine ran without
    /// its 50 Hz interrupt and handed back a stale framebuffer (#1202).
    ///
    /// The frame is a fixed 312 x 128 = 39,936 master ticks at 2 MHz. Each
    /// line is 128 ticks; the CPU fits a variable number of 6502 cycles into
    /// one because accesses to the 1 MHz peripherals cost two ticks. Anchor
    /// the boundaries to a frame base so a cycle that overruns one carries
    /// its extra tick into the next line rather than stretching the frame.
    fn finish_scanline(&mut self) {
        if self.scanline < FB_HEIGHT as u16 {
            self.render_scanline(self.scanline as usize);
        }
        // System VIA CA1 is wired to the CRTC's VSYNC. Drive the level so the
        // VIA's edge detector latches the interrupt.
        self.system_via.set_ca1_level(!self.crtc.vsync);
        self.scanline += 1;
        if self.scanline >= SCANLINES_PER_FRAME {
            self.scanline = 0;
            self.frame_base += CYCLES_PER_FRAME;
            self.frame_count += 1;
        }
    }

    fn tick_cpu_cycle(&mut self) {
        // The keyboard hangs off the System VIA's port A: the CPU drives a key
        // code onto PA0-6 (PA0-3 column, PA4-6 row) and reads PA7, which is
        // high when that key is down. Without this the MOS reads PA7 as a stuck
        // "key held" during its power-on scan and never reaches the CLI that
        // enables interrupts and prints the banner.
        self.update_keyboard_pa7();
        self.update_keyboard_ca2();
        self.update_joystick_fire();
        self.cpu.tick();
        let cost = Self::access_master_ticks(self.cpu.addr);
        if self.cpu.rw {
            self.cpu.data_in = self.mem_read(self.cpu.addr);
        } else {
            self.mem_write(self.cpu.addr, self.cpu.data);
        }
        // The chips run off the constant 2 MHz clock, so they advance one
        // tick per master tick — one or two per 6502 cycle depending on
        // whether this access hit a 1 MHz peripheral.
        for _ in 0..cost {
            self.crtc.tick();
            self.system_via.tick();
            self.user_via.tick();
            self.psg.tick();
            // μPD7002 end-of-conversion is wired to System VIA CB1. Drive
            // the line low on the completion edge (the OS's CB1 is set for
            // a negative edge), latching the analogue interrupt.
            if self.adc.tick() {
                self.system_via.set_cb1_level(false);
            }
        }
        self.tick_cassette(cost);
        self.cpu.irq = self.system_via.irq || self.user_via.irq || self.acia.irq();
        self.master_ticks += cost;
        self.cpu_cycles += 1;
        // A cycle can cost more than one tick, so it may carry the machine
        // over more than one line boundary.
        while self.master_ticks >= self.frame_base + u64::from(self.scanline + 1) * CYCLES_PER_LINE
        {
            self.finish_scanline();
        }
    }

    /// Advances the cassette demodulator by `cost` master ticks while the motor
    /// relay is energised, delivering each recovered byte to the ACIA's receive
    /// register and raising its RX-full flag (which the per-cycle IRQ fold then
    /// turns into a CPU interrupt if the OS has enabled it). The 6850 has no
    /// high-tone line — that is the serial ULA's job — so carrier edges are not
    /// surfaced here.
    fn tick_cassette(&mut self, cost: u64) {
        if self.serial_ula & MOTOR_BIT == 0 || !self.deck_running {
            return;
        }
        let ns = cost * NS_PER_MASTER_TICK;
        // Disjoint borrows: the receiver drives the ACIA register it feeds.
        let BbcMicro { cassette, acia, .. } = self;
        cassette.advance(ns, &mut |event| match event {
            CassetteEvent::ByteReady(byte) => {
                acia.rx_data = byte;
                acia.rx_full = true;
            }
            // Sustained carrier tone raises the ACIA's Data Carrier Detect, the
            // signal the MOS tape filing system waits on before reading a block.
            CassetteEvent::HighTone => acia.set_carrier_detect(),
        });
    }

    /// Master ticks (2 MHz) a 6502 cycle accessing `addr` consumes — the
    /// BBC's 1 MHz-bus contention. The CPU runs at 2 MHz for RAM, ROM and
    /// the fast SHEILA devices (one tick), but slows to 1 MHz (two ticks)
    /// for the 1 MHz peripherals: FRED (`$FC00`), JIM (`$FD00`), and the
    /// SHEILA slow devices — 6845 CRTC / ACIA / serial (`$FE00-$FE1F`),
    /// System VIA (`$FE40-$FE5F`), User VIA (`$FE60-$FE7F`) and the ADC
    /// (`$FEC0-$FEDF`). Mirrors MAME `bbc_state::set_cpu_clock`. The
    /// half-cycle 2→1 MHz clock-resync penalty is not yet modelled.
    fn access_master_ticks(addr: u16) -> u64 {
        match addr & 0xFF00 {
            0xFC00 | 0xFD00 => 2,
            0xFE00 => match addr & 0x00E0 {
                0x00 | 0x40 | 0x60 | 0xC0 => 2,
                _ => 1,
            },
            _ => 1,
        }
    }

    /// Drive System VIA PA7 from the key selected by the code on PA0-6.
    fn update_keyboard_pa7(&mut self) {
        let code = self.system_via.ora();
        let col = (code & 0x0F) as usize;
        let row = ((code >> 4) & 0x07) as usize;
        let pressed = self
            .keyboard
            .get(col)
            .and_then(|c| c.get(row))
            .copied()
            .unwrap_or(false);
        let bit = if pressed { 0x80 } else { 0x00 };
        self.system_via.pa_in = (self.system_via.pa_in & 0x7F) | bit;
    }

    /// Drive the System VIA CA2 "key pressed" interrupt line that the MOS uses
    /// to detect keystrokes. Faithful to jsbeeb's `SysVia.updateKeys`: when the
    /// keyboard is auto-scanning (IC32 addressable-latch bit 3 set) CA2 goes
    /// high if any key in rows 1-7 of any column is down; otherwise it reflects
    /// the column the CPU is currently driving on PA0-3. Row 0 (SHIFT / CTRL)
    /// never raises the interrupt, exactly as the hardware's keyboard scanner.
    fn update_keyboard_ca2(&mut self) {
        let pressed_in_column = |col: &[bool; 8]| col[1..8].iter().any(|&down| down);
        let any_key = if self.latch.bits[3] {
            self.keyboard.iter().any(pressed_in_column)
        } else {
            let col = (self.system_via.ora() & 0x0F) as usize;
            self.keyboard.get(col).is_some_and(pressed_in_column)
        };
        self.system_via.set_ca2_level(any_key);
    }

    /// Merge the joystick fire buttons into System VIA port B: PB4 (joy 1) and
    /// PB5 (joy 2), active low (pressed pulls the line low). Read-modify-write
    /// leaves the addressable-latch outputs (PB0-3) and the speech lines
    /// (PB6-7) untouched.
    fn update_joystick_fire(&mut self) {
        let mut bits = 0x30u8; // both fire lines idle high
        if self.fire[0] {
            bits &= !0x10;
        }
        if self.fire[1] {
            bits &= !0x20;
        }
        self.system_via.pb_in = (self.system_via.pb_in & !0x30) | bits;
    }

    fn mem_read(&mut self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x7FFF => self.ram[addr as usize],
            0x8000..=0xBFFF => self
                .sideways_roms
                .get(self.rom_bank as usize)
                .and_then(|rom| rom.get((addr - 0x8000) as usize).copied())
                .unwrap_or(0xFF),
            0xFE00..=0xFE07 if addr & 1 == 1 => self.crtc.read_data(),
            0xFE40..=0xFE4F => self.system_via.read((addr & 0x0F) as u8),
            0xFE60..=0xFE6F => self.user_via.read((addr & 0x0F) as u8),
            0xFEC0..=0xFEDF => self.adc.read((addr & 0x03) as u8),
            0xFE08..=0xFE0F => self.acia.read(addr),
            0xFC00..=0xFEFF => 0xFF,
            0xC000..=0xFFFF => self
                .mos_rom
                .get((addr - 0xC000) as usize)
                .copied()
                .unwrap_or(0xFF),
        }
    }

    fn mem_write(&mut self, addr: u16, value: u8) {
        match addr {
            0x0000..=0x7FFF => self.ram[addr as usize] = value,
            0xFE00..=0xFE07 if addr & 1 == 0 => self.crtc.write_address(value),
            0xFE00..=0xFE07 if addr & 1 == 1 => self.crtc.write_data(value),
            0xFE20 => self.video_ula.write_control(value),
            0xFE21 => self.video_ula.write_palette(value),
            0xFE08..=0xFE0F => self.acia.write(addr, value),
            // Serial ULA: RX/TX baud, RS423/cassette select, bit 7 motor relay.
            0xFE10..=0xFE1F => self.serial_ula = value,
            0xFE30 => self.rom_bank = value & 0x0F,
            0xFE40..=0xFE4F => {
                let reg = (addr & 0x0F) as u8;
                self.system_via.write(reg, value);
                // System VIA port B carries the IC32 addressable
                // latch encoding: low 3 bits = address, bit 3 = data.
                if reg == 0x00 {
                    let latch_addr = value & 0x07;
                    let latch_data = value & 0x08 != 0;
                    if let Some(()) = self.latch.write(latch_addr, latch_data).map(|_| ()) {
                        // SN76489 /WE asserted — latch the byte on
                        // ORA into the PSG.
                        self.psg.write(self.system_via.ora());
                    }
                }
            }
            0xFE60..=0xFE6F => self.user_via.write((addr & 0x0F) as u8, value),
            // Only a write to the control register ($FEC0, reg 0) starts a
            // conversion. Beginning one releases EOC (CB1 high) until the
            // countdown completes and pulls it low again. Writes to the result
            // registers are no-ops (they fall through to the catch-all).
            0xFEC0..=0xFEDF if addr & 0x03 == 0 => {
                self.adc.write_control(value);
                self.system_via.set_cb1_level(true);
            }
            _ => {}
        }
    }

    fn render_scanline(&mut self, line: usize) {
        let offset = line * FB_WIDTH as usize;
        if self.video_ula.teletext() {
            self.render_teletext_scanline(line, offset);
            return;
        }
        // The ULA has no bit-depth setting and no per-mode decode: it loads a
        // byte into an 8-bit shift register, and every pixel takes its
        // four-bit palette index from bits 7, 5, 3 and 1 of whatever is in
        // there. Between pixels the register shifts left and a `1` comes in
        // at the bottom. Two-colour modes work because the MOS programs the
        // palette so the entries a shifted-in `1` can reach all hold the same
        // colour — not because the ULA narrows the index.
        //
        // Decoding per depth instead, with a bespoke bit layout for each, is
        // what made MODE 0 and MODE 3 come out black: every pixel resolved to
        // logical colour 0, which those modes leave as the background (#1195).
        let pixels_per_byte = self.video_ula.pixels_per_byte();
        // Bytes per line is the 6845's horizontal-displayed (R1), not
        // something the ULA knows. Guessing it from the ULA clock bit gave
        // MODE 2 and MODE 5 the wrong width.
        let chars_per_line = usize::from(self.crtc.regs()[1]).max(1);
        let pixel_width = (FB_WIDTH as usize / (chars_per_line * pixels_per_byte)).max(1);
        let crtc_start = self.crtc.start_address() as usize;
        // Character cell height is the 6845's R9 (max scanline address), not
        // a fixed eight: the gapped text modes 3 and 6 use ten-line cells, and
        // counting rows in eights walked off the end of their screen memory.
        let cell_height = usize::from(self.crtc.regs()[9]) + 1;
        let ra = line % cell_height;
        let char_row = line / cell_height;
        // Display enable is masked by RA3, so a cell taller than eight lines
        // blanks the rest instead of showing anything. That is where the gap
        // between rows in the gapped text modes 3 and 6 comes from.
        if ra & 0x08 != 0 {
            let blank = self.video_ula.palette_to_argb(0);
            self.framebuffer[offset..offset + FB_WIDTH as usize].fill(blank);
            return;
        }
        for col in 0..chars_per_line {
            let ma = crtc_start + char_row * chars_per_line + col;
            // Only RA0-RA2 reach the address bus, so a cell taller than eight
            // lines repeats its first rows rather than reading past itself.
            let ram_addr = ((ma & 0x3FFF) << 3) | (ra & 0x07);
            let byte = if ram_addr < 0x8000 {
                self.ram[ram_addr]
            } else {
                0
            };
            let mut shiftreg = byte;
            for px in 0..pixels_per_byte {
                let colour_idx = ((shiftreg >> 4) & 0x08)
                    | ((shiftreg >> 3) & 0x04)
                    | ((shiftreg >> 2) & 0x02)
                    | ((shiftreg >> 1) & 0x01);
                shiftreg = (shiftreg << 1) | 1;
                let argb = self.video_ula.palette_to_argb(colour_idx);
                let fb_x = (col * pixels_per_byte + px) * pixel_width;
                for w in 0..pixel_width {
                    if fb_x + w < FB_WIDTH as usize {
                        self.framebuffer[offset + fb_x + w] = argb;
                    }
                }
            }
        }
    }

    /// Render one MODE 7 (teletext) scanline through a model of the SAA5050.
    ///
    /// Each of the 40 columns is a 12×10 cell. Control codes (`$00-$1F`) act
    /// "set-after" — they show as a space (or the held mosaic) and change the
    /// state used by the *following* cells. Displayable codes are either
    /// alphanumeric glyphs from the character ROM or 2×3 mosaic blocks while in
    /// graphics mode. Colours are the fixed 3-bit teletext set, not the Video
    /// ULA palette.
    fn render_teletext_scanline(&mut self, line: usize, offset: usize) {
        const COLS: usize = 40;
        const CELL_W: usize = 12;
        const CELL_H: usize = 10;
        const X_BASE: usize = (FB_WIDTH as usize - COLS * CELL_W) / 2;

        self.framebuffer[offset..offset + FB_WIDTH as usize].fill(teletext_colour(0));

        let char_row = line / CELL_H;
        let font_row = line % CELL_H;
        if char_row >= 25 {
            return;
        }
        let row_base = 0x7C00usize + char_row * COLS;

        // State resets at the start of each character row.
        let mut fg: u8 = 7;
        let mut bg: u8 = 0;
        let mut graphics = false;
        let mut separated = false;
        let mut hold = false;
        let mut held_pattern: u16 = 0;

        for col in 0..COLS {
            let code = self.peek((row_base + col) as u16);
            let mut pattern: u16 = 0;

            if code < 0x20 {
                if hold && graphics {
                    pattern = held_pattern;
                }
                match code {
                    0x01..=0x07 => {
                        graphics = false;
                        fg = code;
                    }
                    0x11..=0x17 => {
                        graphics = true;
                        fg = code & 0x07;
                    }
                    0x19 => separated = false,
                    0x1A => separated = true,
                    0x1C => bg = 0,
                    0x1D => bg = fg,
                    0x1E => hold = true,
                    0x1F => hold = false,
                    _ => {}
                }
            } else if graphics && (code & 0x20) == 0 {
                // $40-$5F stay alphanumeric even in graphics mode.
                pattern = self.teletext_alpha(code, font_row);
            } else if graphics {
                pattern = mosaic_pattern(code, font_row, separated);
                held_pattern = pattern;
            } else {
                pattern = self.teletext_alpha(code, font_row);
            }

            let fg_argb = teletext_colour(fg);
            let bg_argb = teletext_colour(bg);
            let x0 = X_BASE + col * CELL_W;
            for px in 0..CELL_W {
                let on = (pattern >> (CELL_W - 1 - px)) & 1 != 0;
                let fb_x = x0 + px;
                if fb_x < FB_WIDTH as usize {
                    self.framebuffer[offset + fb_x] = if on { fg_argb } else { bg_argb };
                }
            }
        }
    }

    /// One row of an alphanumeric glyph as a 12-bit pattern (the six source
    /// columns each doubled). Font bit 0 is the rightmost pixel.
    fn teletext_alpha(&self, code: u8, font_row: usize) -> u16 {
        if !(0x20..0x80).contains(&code) {
            return 0;
        }
        let idx = (code as usize - 0x20) * 10 + font_row;
        let byte = self.teletext_font.get(idx).copied().unwrap_or(0);
        let mut pattern = 0u16;
        for c in 0..6u16 {
            if byte & (1 << c) != 0 {
                pattern |= 0b11 << (c * 2);
            }
        }
        pattern
    }

    /// Framebuffer (640×256 ARGB32).
    #[must_use]
    pub fn framebuffer(&self) -> &[u32] {
        &self.framebuffer
    }

    /// Framebuffer width.
    #[must_use]
    pub fn framebuffer_width(&self) -> u32 {
        FB_WIDTH
    }

    /// Framebuffer height.
    #[must_use]
    pub fn framebuffer_height(&self) -> u32 {
        FB_HEIGHT
    }

    /// Take the PSG audio buffer.
    pub fn take_audio_buffer(&mut self) -> Vec<f32> {
        self.psg.take_buffer()
    }

    /// Press a key at the given (column, row).
    pub fn press_key(&mut self, col: usize, row: usize) {
        if col < 10 && row < 8 {
            self.keyboard[col][row] = true;
        }
    }

    /// Release a key at the given (column, row).
    pub fn release_key(&mut self, col: usize, row: usize) {
        if col < 10 && row < 8 {
            self.keyboard[col][row] = false;
        }
    }

    /// CPU reference.
    #[must_use]
    pub fn cpu(&self) -> &M6502 {
        &self.cpu
    }

    /// CPU mutable reference.
    pub fn cpu_mut(&mut self) -> &mut M6502 {
        &mut self.cpu
    }

    /// CRTC reference.
    #[must_use]
    pub fn crtc(&self) -> &Crtc6845 {
        &self.crtc
    }

    /// Current ROM bank (0-15).
    #[must_use]
    pub fn rom_bank(&self) -> u8 {
        self.rom_bank
    }

    /// Frame count since power-on.
    #[must_use]
    pub fn frame_count(&self) -> u64 {
        self.frame_count
    }

    /// CPU cycles since power-on.
    #[must_use]
    pub fn cpu_cycles(&self) -> u64 {
        self.cpu_cycles
    }
}

impl BbcMicro {
    /// Read one byte with no side effects (RAM / sideways ROM / MOS;
    /// `$FF` for the SHEILA I/O page).
    #[must_use]
    pub fn peek(&self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x7FFF => self.ram[addr as usize],
            0x8000..=0xBFFF => self
                .sideways_roms
                .get(self.rom_bank as usize)
                .and_then(|rom| rom.get((addr - 0x8000) as usize).copied())
                .unwrap_or(0xFF),
            0xFC00..=0xFEFF => 0xFF,
            0xC000..=0xFFFF => self
                .mos_rom
                .get((addr - 0xC000) as usize)
                .copied()
                .unwrap_or(0xFF),
        }
    }

    /// Write one byte through the bus (RAM accepts it; ROM ignores it).
    pub fn poke(&mut self, addr: u16, value: u8) {
        self.mem_write(addr, value);
    }

    /// Run exactly one whole 6502 instruction, returning the clocks it
    /// consumed. A safety cap prevents an unbounded spin.
    pub fn step_instruction(&mut self) -> u64 {
        let mut ticks = 0u64;
        while self.cpu.instruction_complete() && ticks < 4096 {
            self.tick_cpu_cycle();
            ticks += 1;
        }
        while !self.cpu.instruction_complete() && ticks < 4096 {
            self.tick_cpu_cycle();
            ticks += 1;
        }
        ticks
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn trap_rom() -> Vec<u8> {
        // 16 KB MOS ROM with JMP self at $C000 + reset / IRQ / NMI
        // vectors pointing there.
        let mut rom = vec![0xEA_u8; 0x4000];
        rom[0x0000] = 0x4C;
        rom[0x0001] = 0x00;
        rom[0x0002] = 0xC0;
        rom[0x3FFA] = 0x00;
        rom[0x3FFB] = 0xC0;
        rom[0x3FFC] = 0x00;
        rom[0x3FFD] = 0xC0;
        rom[0x3FFE] = 0x00;
        rom[0x3FFF] = 0xC0;
        rom
    }

    /// Save-state must capture LIVE machine state (6502, 6845 CRTC, Video ULA,
    /// both 6522 VIAs, SN76489 PSG, 32 KB RAM, ADC, ACIA, latch), not cold-boot
    /// from the MOS ROM. Serialise, advance (so the state differs), then
    /// deserialise the first snapshot and confirm re-serialising it is
    /// byte-identical: every stateful field across all chips round-trips,
    /// including the 32 KB RAM via BigArray.
    #[test]
    fn snapshot_round_trips_live_state() {
        let mut sys = BbcMicro::new(trap_rom());
        sys.run_frame();
        sys.poke(0x0100, 0xA5); // a low-RAM byte to carry across the snapshot
        assert_eq!(sys.peek(0x0100), 0xA5, "poke lands in BBC main RAM");
        sys.run_frame();
        let s1 = postcard::to_allocvec(&sys).expect("encode snapshot");

        sys.run_frame(); // advance past the snapshot point
        let s2 = postcard::to_allocvec(&sys).expect("encode again");
        assert_ne!(s1, s2, "running a frame should change the serialised state");

        let restored: BbcMicro = postcard::from_bytes(&s1).expect("decode snapshot");
        let s3 = postcard::to_allocvec(&restored).expect("re-encode restored");
        assert_eq!(
            s1, s3,
            "restore should reproduce the snapshot state exactly"
        );
    }

    #[test]
    fn frame_runs_expected_cycles() {
        let mut sys = BbcMicro::new(trap_rom());
        let t = sys.run_frame();
        assert_eq!(t, CYCLES_PER_FRAME);
        assert_eq!(sys.frame_count(), 1);
    }

    #[test]
    fn many_frames_complete_without_panic() {
        let mut sys = BbcMicro::new(trap_rom());
        for _ in 0..10 {
            sys.run_frame();
        }
        assert_eq!(sys.frame_count(), 10);
    }

    #[test]
    fn one_mhz_bus_accesses_cost_two_ticks_rest_one() {
        // RAM, ROM and the fast SHEILA devices run at 2 MHz (one tick);
        // FRED/JIM and the slow SHEILA devices at 1 MHz (two ticks).
        // Fast:
        assert_eq!(BbcMicro::access_master_ticks(0x0000), 1); // RAM
        assert_eq!(BbcMicro::access_master_ticks(0xC000), 1); // MOS ROM
        assert_eq!(BbcMicro::access_master_ticks(0x8000), 1); // sideways ROM
        assert_eq!(BbcMicro::access_master_ticks(0xFE20), 1); // video ULA
        assert_eq!(BbcMicro::access_master_ticks(0xFE30), 1); // ROM-page latch
        assert_eq!(BbcMicro::access_master_ticks(0xFE80), 1); // FDC (fast)
        // Slow (1 MHz bus):
        assert_eq!(BbcMicro::access_master_ticks(0xFC00), 2); // FRED
        assert_eq!(BbcMicro::access_master_ticks(0xFD00), 2); // JIM
        assert_eq!(BbcMicro::access_master_ticks(0xFE00), 2); // CRTC
        assert_eq!(BbcMicro::access_master_ticks(0xFE40), 2); // System VIA
        assert_eq!(BbcMicro::access_master_ticks(0xFE5F), 2); // System VIA top
        assert_eq!(BbcMicro::access_master_ticks(0xFE60), 2); // User VIA
        assert_eq!(BbcMicro::access_master_ticks(0xFEC0), 2); // ADC
    }

    #[test]
    fn frame_is_a_fixed_master_tick_budget() {
        // However the CPU's access mix falls out, the frame is exactly
        // 312 × 128 master ticks; the CPU just fits fewer cycles in when
        // it hits the 1 MHz bus.
        let mut sys = BbcMicro::new(trap_rom());
        sys.run_frame();
        assert_eq!(sys.master_ticks, CYCLES_PER_FRAME);
        // The trap loop runs entirely in ROM (2 MHz), so it fits one CPU
        // cycle per master tick — the maximum.
        assert_eq!(sys.cpu_cycles(), CYCLES_PER_FRAME);
    }

    #[test]
    fn memory_map_routes_pages() {
        let mut rom = trap_rom();
        rom[0x0100] = 0x99;
        let mut sys = BbcMicro::new(rom);
        sys.insert_rom(0, vec![0x77; 0x4000]);
        // MOS at $C000.
        assert_eq!(sys.mem_read(0xC100), 0x99);
        // Sideways ROM at $8000.
        assert_eq!(sys.mem_read(0x8000), 0x77);
        // RAM round-trip.
        sys.mem_write(0x4000, 0x42);
        assert_eq!(sys.mem_read(0x4000), 0x42);
        // ROM writes ignored.
        sys.mem_write(0xC100, 0x00);
        assert_eq!(sys.mem_read(0xC100), 0x99);
    }

    #[test]
    fn rom_bank_register_at_fe30() {
        let mut sys = BbcMicro::new(trap_rom());
        sys.insert_rom(0, vec![0xAA; 0x4000]);
        sys.insert_rom(7, vec![0xBB; 0x4000]);
        sys.mem_write(0xFE30, 7);
        assert_eq!(sys.rom_bank(), 7);
        assert_eq!(sys.mem_read(0x8000), 0xBB);
        sys.mem_write(0xFE30, 0);
        assert_eq!(sys.mem_read(0x8000), 0xAA);
    }

    #[test]
    fn video_ula_palette_write_decodes_logical_and_physical() {
        let mut sys = BbcMicro::new(trap_rom());
        // Set logical entry 5 to physical 3 ($53 → logical=5, phys=3).
        sys.mem_write(0xFE21, 0x53);
        assert_eq!(sys.video_ula.palette[5], 3);
    }

    /// The control values the MOS writes for each mode, from the Advanced
    /// User Guide's own table (§19.1.7), against the pixels each byte has to
    /// produce for the mode to come out the documented width.
    ///
    /// The old decode read bits 3-2 as a bit-depth field, which the register
    /// does not have — they set the pixel rate. It got MODE 1 and MODE 2
    /// backwards and doubled MODE 4 and MODE 6, and the test that covered it
    /// asserted the same wrong answer (#1195).
    #[test]
    fn video_ula_pixels_per_byte_matches_the_documented_modes() {
        let mut sys = BbcMicro::new(trap_rom());
        for (mode, control, expected, width) in [
            (0u8, 0x9Cu8, 8usize, 640usize),
            (1, 0xD8, 4, 320),
            (2, 0xF4, 2, 160),
            (3, 0x9C, 8, 640),
            (4, 0x88, 8, 320),
            (5, 0xC4, 4, 160),
            (6, 0x88, 8, 320),
        ] {
            sys.mem_write(0xFE20, control);
            assert_eq!(
                sys.video_ula.pixels_per_byte(),
                expected,
                "MODE {mode} (control ${control:02X})"
            );
            // Bytes per line comes from the 6845, so pair each mode with its
            // own to confirm the geometry lands on the documented width.
            let bytes_per_line = if mode <= 3 { 80 } else { 40 };
            assert_eq!(bytes_per_line * expected, width, "MODE {mode} pixel width");
        }
    }

    /// The guide's `*FX154,224` worked example (§19.3): a 16-colour mode with
    /// ten characters per line, which its own listing documents as two pixels
    /// per byte.
    #[test]
    fn video_ula_matches_the_guides_mode_8_example() {
        let mut sys = BbcMicro::new(trap_rom());
        sys.mem_write(0xFE20, 0xE0);
        assert!(!sys.video_ula.fast_clock());
        assert_eq!(sys.video_ula.pixels_per_byte(), 2);
    }

    #[test]
    fn system_via_writes_round_trip() {
        let mut sys = BbcMicro::new(trap_rom());
        sys.mem_write(0xFE43, 0xFF); // DDRA
        sys.mem_write(0xFE41, 0x77); // ORA
        assert_eq!(sys.system_via.ora(), 0x77);
    }

    #[test]
    fn joystick_fire_buttons_pull_system_via_pb4_pb5_low() {
        let mut sys = BbcMicro::new(trap_rom());
        // Idle: both fire lines read high.
        sys.update_joystick_fire();
        assert_eq!(sys.system_via.pb_in & 0x30, 0x30);

        // Joy 1 fire → PB4 low, PB5 still high.
        sys.set_fire_button(1, true);
        sys.update_joystick_fire();
        assert_eq!(sys.system_via.pb_in & 0x10, 0, "joy1 fire → PB4 low");
        assert_eq!(sys.system_via.pb_in & 0x20, 0x20, "joy2 idle → PB5 high");

        // Joy 2 fire as well → both low.
        sys.set_fire_button(2, true);
        sys.update_joystick_fire();
        assert_eq!(sys.system_via.pb_in & 0x30, 0, "both fire → PB4+PB5 low");

        // Release joy 1 → PB4 high again, PB5 held low.
        sys.set_fire_button(1, false);
        sys.update_joystick_fire();
        assert_eq!(
            sys.system_via.pb_in & 0x10,
            0x10,
            "joy1 released → PB4 high"
        );
        assert_eq!(sys.system_via.pb_in & 0x20, 0, "joy2 still held → PB5 low");

        // It must reach the CPU through the IRB read with port B as input.
        let pb = sys.mem_read(0xFE40);
        assert_eq!(pb & 0x20, 0, "PB5 low visible at $FE40");
    }

    #[test]
    fn adc_converts_a_channel_and_reports_completion() {
        let mut sys = BbcMicro::new(trap_rom());
        // Park a known value on channel 1 (joystick 1 Y): 0x0ABC.
        sys.set_adc_channel(1, 0x0ABC);

        // Start a 12-bit conversion on channel 1 (bit 3 = 12-bit, mux = 01).
        sys.mem_write(0xFEC0, 0b0000_1001);
        // Immediately busy: status bit 6 (busy_n) low, bit 7 (completed_n) high.
        let status = sys.mem_read(0xFEC0);
        assert_eq!(status & 0x40, 0, "busy_n low while converting");
        assert_eq!(status & 0x80, 0x80, "completed_n high while converting");

        // Run the conversion to completion (12-bit = 20000 cycles).
        for _ in 0..ADC_CONVERT_12BIT {
            sys.adc.tick();
        }
        let status = sys.mem_read(0xFEC0);
        assert_eq!(status & 0x80, 0, "completed_n low once finished");
        assert_eq!(status & 0x40, 0x40, "busy_n high once finished");
        assert_eq!(status & 0x03, 0x01, "mux echoes channel 1");
        // Top two value bits (0x0ABC >> 10 = 0b10) appear in status bits 5-4.
        assert_eq!((status >> 4) & 0x03, 0b10, "value[11:10] in status");

        // Result registers: high = value[11:4], low = value[3:0] << 4.
        assert_eq!(sys.mem_read(0xFEC1), 0xAB, "high byte = value[11:4]");
        assert_eq!(sys.mem_read(0xFEC2), 0xC0, "low byte = value[3:0] << 4");
    }

    #[test]
    fn adc_completion_raises_the_system_via_cb1_interrupt() {
        let mut sys = BbcMicro::new(trap_rom());
        // Configure System VIA CB1 for a negative-edge interrupt and enable it:
        // PCR bit 4 = 0 (CB1 negative edge); IER bit 4 + bit 7 (set-enable).
        sys.mem_write(0xFE4C, 0x00); // PCR: CB1 negative edge
        sys.mem_write(0xFE4E, 0x90); // IER: enable CB1 (bit 4) with set bit (7)

        // A conversion in flight presents CB1 high (no edge yet).
        sys.mem_write(0xFEC0, 0b0000_1000); // 12-bit, channel 0
        assert_eq!(sys.mem_read(0xFE4D) & 0x10, 0, "no CB1 flag mid-conversion");

        // Drive it to completion through the real per-cycle tick so the
        // EOC→CB1 falling edge is delivered the same way the engine does it.
        for _ in 0..ADC_CONVERT_12BIT {
            sys.tick_cpu_cycle();
        }
        assert_ne!(
            sys.mem_read(0xFE4D) & 0x10,
            0,
            "CB1 (ADC end-of-conversion) interrupt flag set"
        );
    }

    #[test]
    fn ic32_falling_edge_on_bit_0_writes_psg() {
        let mut sys = BbcMicro::new(trap_rom());
        // Set ORA = $80 (PSG tone latch byte for ch0).
        sys.mem_write(0xFE43, 0xFF);
        sys.mem_write(0xFE41, 0x80);
        // Raise latch bit 0 (write port B with addr=0, data=1).
        sys.mem_write(0xFE40, 0b0000_1000);
        // Drop latch bit 0 — should latch ORA into PSG.
        sys.mem_write(0xFE40, 0b0000_0000);
        // PSG sweep / mute behaviour is verified inside ti-sn76489;
        // here we just confirm the write path didn't panic.
    }

    #[test]
    fn acia_idle_does_not_signal_an_interrupt() {
        // The MOS IRQ handler reads $FE08 to decide whether the 6850 ACIA
        // interrupted. An idle ACIA must report TDRE set and the interrupt bit
        // ($80) CLEAR — the old open-bus $FF read set bit 7 and the MOS serviced
        // a phantom serial interrupt forever, never clearing the System VIA
        // timer (the storm that kept BASIC from printing `>`).
        let mut sys = BbcMicro::new(trap_rom());
        let status = sys.mem_read(0xFE08);
        assert_eq!(status & 0x80, 0, "idle ACIA must not assert IRQ (bit 7)");
        assert_eq!(
            status & 0x02,
            0x02,
            "idle ACIA reports TDRE (ready to send)"
        );
        assert!(!sys.acia.irq(), "idle ACIA drives no CPU interrupt");

        // Faithful detail: enabling the transmit interrupt (control bits 6-5 =
        // 01) does make TDRE assert the interrupt, matching b-em.
        sys.mem_write(0xFE08, 0x20);
        assert!(sys.acia.irq(), "TX-interrupt mode + TDRE asserts IRQ");
        assert_eq!(sys.mem_read(0xFE08) & 0x80, 0x80, "and shows in the status");
    }

    // Kansas-City encoding for the cassette wiring tests.
    const T_ZERO_HALF: u32 = 416_667;
    const T_ONE_HALF: u32 = 208_333;

    fn push_tape_byte(pulses: &mut Vec<TapePulse>, byte: u8) {
        let mut push_bit = |set: bool| {
            pulses.push(if set {
                TapePulse::Cycles {
                    half_period_ns: T_ONE_HALF,
                    count: 2,
                }
            } else {
                TapePulse::Cycles {
                    half_period_ns: T_ZERO_HALF,
                    count: 1,
                }
            });
        };
        push_bit(false); // start
        for i in 0..8 {
            push_bit((byte >> i) & 1 == 1);
        }
        push_bit(true); // stop
    }

    /// A long carrier leader then one framed byte.
    fn carrier_then_byte(byte: u8) -> Vec<TapePulse> {
        let mut pulses = vec![TapePulse::Cycles {
            half_period_ns: T_ONE_HALF,
            count: 256,
        }];
        push_tape_byte(&mut pulses, byte);
        pulses
    }

    #[test]
    fn cassette_does_not_play_while_the_motor_is_off() {
        let mut sys = BbcMicro::new(trap_rom());
        sys.insert_tape(carrier_then_byte(0xA5));
        // The serial ULA motor bit ($FE10 bit 7) defaults off.
        assert!(!sys.cassette_motor_on());
        for _ in 0..10 {
            sys.run_frame();
        }
        assert!(
            !sys.acia.rx_full,
            "no byte should arrive with the motor off"
        );
        assert!(sys.tape_loaded());
    }

    /// The motor line says whether the machine wants the tape to move; the
    /// deck gate says whether it is running at all. A script could not stop
    /// the tape at all before, which made a stalled load hard to inspect
    /// (#1198).
    /// Stepping instructions and running frames have to drive the same
    /// hardware. The scanline loop lived in `run_frame`, so a stepped BBC
    /// never had VSYNC driven into the System VIA and painted nothing --
    /// no 50 Hz interrupt, and a stale framebuffer (#1202).
    #[test]
    fn stepping_a_frames_worth_of_instructions_is_a_frame() {
        let mut stepped = BbcMicro::new(trap_rom());
        let mut run = BbcMicro::new(trap_rom());

        while stepped.master_ticks < CYCLES_PER_FRAME {
            stepped.step_instruction();
        }
        run.run_frame();

        assert_eq!(stepped.frame_count(), run.frame_count());
        assert_eq!(
            stepped.framebuffer(),
            run.framebuffer(),
            "a stepped frame has to paint what a run frame paints"
        );
    }

    #[test]
    fn a_stopped_deck_does_not_advance_even_with_the_motor_on() {
        let mut sys = BbcMicro::new(trap_rom());
        sys.insert_tape(carrier_then_byte(0xA5));
        sys.mem_write(0xFE08, 0x80); // ACIA: enable RX interrupt
        sys.mem_write(0xFE10, 0x80); // serial ULA: motor on
        assert!(sys.cassette_motor_on());

        sys.set_deck_running(false);
        for _ in 0..12 {
            sys.run_frame();
        }
        assert!(
            !sys.acia.rx_full,
            "the motor is on, but the deck is stopped, so no byte can arrive"
        );

        sys.set_deck_running(true);
        let mut arrived = false;
        for _ in 0..12 {
            sys.run_frame();
            if sys.acia.rx_full {
                arrived = true;
                break;
            }
        }
        assert!(arrived, "starting the deck lets the same tape play");
    }

    #[test]
    fn cassette_byte_fills_the_acia_and_clears_on_read() {
        let mut sys = BbcMicro::new(trap_rom());
        sys.insert_tape(carrier_then_byte(0xA5));
        sys.mem_write(0xFE08, 0x80); // ACIA control: enable RX interrupt (CR7)
        sys.mem_write(0xFE10, 0x80); // serial ULA: motor on
        assert!(sys.cassette_motor_on());

        let mut arrived = false;
        for _ in 0..12 {
            sys.run_frame();
            if sys.acia.rx_full {
                arrived = true;
                break;
            }
        }
        assert!(arrived, "the ACIA never received a byte");
        // RDRF + RX-int enable raises the ACIA interrupt and the status bit.
        assert!(sys.acia.irq(), "received byte must assert the ACIA IRQ");
        assert_eq!(sys.mem_read(0xFE08) & 0x01, 0x01, "status shows RDRF");
        // Reading the data register ($FE09) returns the byte and clears RDRF.
        assert_eq!(sys.mem_read(0xFE09), 0xA5);
        assert!(!sys.acia.rx_full, "reading $FE09 clears RDRF");
        assert!(!sys.acia.irq(), "and drops the interrupt");
    }

    #[test]
    fn cassette_carrier_raises_dcd_and_clears_on_data_read() {
        let mut sys = BbcMicro::new(trap_rom());
        // A sustained carrier tone with no data.
        sys.insert_tape(vec![TapePulse::Cycles {
            half_period_ns: T_ONE_HALF,
            count: 256,
        }]);
        sys.mem_write(0xFE08, 0x80); // ACIA control: enable RX interrupt
        sys.mem_write(0xFE10, 0x80); // serial ULA: motor on

        let mut dcd = false;
        for _ in 0..6 {
            sys.run_frame();
            if sys.mem_read(0xFE08) & 0x04 != 0 {
                dcd = true;
                break;
            }
        }
        // The MOS waits on Data Carrier Detect (status bit 2) before reading a
        // tape block; sustained carrier must raise it and the interrupt.
        assert!(dcd, "sustained carrier must raise DCD");
        assert!(sys.acia.irq(), "DCD raises the ACIA interrupt");
        // Reading the data register clears the latched DCD.
        let _ = sys.mem_read(0xFE09);
        assert_eq!(sys.mem_read(0xFE08) & 0x04, 0, "reading data clears DCD");
    }
}
