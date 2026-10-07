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
//!   access stretches to the end of a whole 1 MHz cycle, costing two or
//!   three of them by phase (Advanced User Guide §28.5).
//! - **CRTC:** Motorola 6845
//! - **Video ULA:** Acorn custom (16-entry palette with flashing colours,
//!   pixel rate + fast-clock selection, the cursor's width)
//! - **Teletext:** Mullard SAA5050 character generator for MODE 7
//!   (character rounding, double height, flash, conceal, hold graphics)
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

mod saa5050;

use common_acorn_cassette::{CassetteEvent, CassetteReceiver, TapePulse};
use emu198x_mos_6502::M6502;
use mos_via_6522::Via6522;
use motorola_6845::{Crtc6845, Crtc6845Variant};
use saa5050::Saa5050;
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

/// Framebuffer height: the 256 scan lines MODE 0-2 display, in both fields.
///
/// Blanked for the same reason. R4 = 38 and R9 = 7 give a 312-line field, of
/// which R6 = 32 character rows of 8 lines are displayed. A PAL set shows 288,
/// so the audit reads 89% — again the chip, and again black outside it.
///
/// Every line is held twice, once for each field of the interlaced picture
/// the MOS sets up (R8 bit 0, Advanced User Guide §18.6.1). In MODE 7's
/// interlace sync and video mode the two fields carry different lines of each
/// character, and the SAA5050's character rounding only exists across the
/// pair, so a single field cannot show it. The other modes scan the same lines
/// in both fields and fill both rows. jsbeeb and b-em weave the fields the
/// same way, and the Amiga's 768×576 holds both of its fields too.
pub const FB_HEIGHT: u32 = 512;

/// Framebuffer rows per scan line of one field.
const ROWS_PER_LINE: usize = 2;

/// BBC Micro CPU clock: 2 MHz. Kept as a documented reference even
/// though `CYCLES_PER_FRAME` is the only derived constant the engine
/// reads today.
#[allow(dead_code)]
const CPU_CLOCK_HZ: u32 = 2_000_000;
const CYCLES_PER_FRAME: u64 = 39_936; // 312 lines × 64 µs × 2 MHz
const SCANLINES_PER_FRAME: u16 = 312;
const CYCLES_PER_LINE: u64 = 128;

const SN76489_CLOCK_HZ: u32 = 4_000_000;

/// Video ULA — palette + control register (Advanced User Guide §19).
///
/// It has no scroll register of any kind. Scrolling on the BBC is the 6845's
/// start address (R12/R13) with the addressable latch's wrap (§18.10), a
/// character at a time. Pixel-level horizontal offset is a feature of the
/// later VideoNuLA replacement board, which b-em emulates at `&FE22`; the
/// Model B's own ULA decodes only `&FE20` and `&FE21`.
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

    /// Control bit 0: which colour of each flashing pair is showing.
    ///
    /// The Advanced User Guide (§19.1.1): `0` the first colour, `1` the
    /// second. The ULA does not time the flash; the MOS toggles this bit from
    /// its vertical-sync interrupt, at the mark and space durations `*FX9` and
    /// `*FX10` set.
    const fn second_flash_colour(&self) -> bool {
        self.control & 0x01 != 0
    }

    /// Whether the cursor segment `stage` of the ULA's cursor sequence is
    /// enabled by control bits 5-7.
    ///
    /// The ULA widens the 6845's one-character CURSOR pulse into up to four
    /// character clocks, each gated by a control bit: bit 7 the first, bit 6
    /// the second, bit 5 the third and fourth. That is the Advanced User
    /// Guide's "master cursor size" and "width of cursor in bytes" (§19.1.5,
    /// §19.1.6): the MOS's values give one byte in MODE 0, 3, 4 and 6 (bit 7),
    /// two in MODE 1 and 5 (bits 7 and 6), four in MODE 2 (bits 7, 6, 5), and
    /// all three clear hide the cursor "under ALL conditions". Stages 0-2 come
    /// before the first segment and are where the 6845's cursor skew
    /// (R8 bits 6-7) starts the sequence; b-em's `cursorlook` and jsbeeb's
    /// `cursorTable` are the same table.
    const fn cursor_segment(&self, stage: u8) -> bool {
        let mask = match stage {
            3 => 0x80,
            4 => 0x40,
            5 | 6 => 0x20,
            _ => 0,
        };
        self.control & mask != 0
    }

    fn palette_to_argb(&self, index: u8) -> u32 {
        let mut entry = self.palette[index as usize & 0x0F];
        // Bit 3 marks a flashing colour. While control bit 0 selects the
        // second colour of the pair, the ULA inverts the three colour bits:
        // physical `&08` is black then white, `&09` red then cyan (Advanced
        // User Guide §19.2.3). b-em and jsbeeb apply the same inversion.
        if entry & 0x08 != 0 && self.second_flash_colour() {
            entry ^= 0x07;
        }
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

    /// Bytes the hardware-scroll wrap takes off a screen address that has run
    /// past `$7FFF`, chosen by latch outputs B4 and B5.
    ///
    /// The Advanced User Guide (§18.10, §23.2) describes the circuit: when the
    /// 6845 asks for an address above `$7FFF` it adds a constant, which in a
    /// 15-bit RAM address is the same as subtracting the screen's size. The
    /// guide's own §23.2 table pairs the bit patterns with the wrong sizes for
    /// the 20K and 10K modes. MOS 1.20 writes B5 = 1, B4 = 0 for MODE 0-2 and
    /// sets both for MODE 4-5, so those are the patterns that must wrap by 20K
    /// and 10K for the MOS's own scrolled screens to come back to their start.
    /// b-em (`sysvia.c` `scrsize`, `video.c` `screenlen`) and jsbeeb
    /// (`video.js` `screenAddrSubtract`) decode the bits the same way.
    ///
    /// | B5 | B4 | size | modes |
    /// |----|----|------|-------|
    /// | 0  | 0  | 16K  | 3     |
    /// | 0  | 1  | 8K   | 6     |
    /// | 1  | 0  | 20K  | 0-2   |
    /// | 1  | 1  | 10K  | 4-5   |
    const fn screen_wrap_size(&self) -> u16 {
        match (self.bits[5], self.bits[4]) {
            (false, false) => 0x4000,
            (false, true) => 0x2000,
            (true, false) => 0x5000,
            (true, true) => 0x2800,
        }
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

/// Framebuffer pixels one 6845 character occupies at the 2 MHz (fast) clock:
/// half a microsecond of the 16 MHz dot clock. The 1 MHz clock doubles it.
const FAST_CHAR_PIXELS: usize = 8;

/// MODE 7's character cell: one microsecond of the 16 MHz framebuffer, the
/// same sixteen pixels as a 1 MHz-clock character in MODE 4-6.
///
/// The SAA5050 takes one character per 6845 clock, and MODE 7 runs the 6845
/// at 1 MHz (Video ULA control `&4B`, bit 4 clear; R0 = 63 for a 64 µs line;
/// Advanced User Guide §18.3.1, §19.1.4). Its twelve half-dots (see
/// [`saa5050`]) therefore span sixteen pixels, and the forty columns the
/// whole 640, as MODE 0-6 do. They were twelve pixels, the forty columns
/// centred in 480 of the 640, which drew MODE 7 three-quarters as wide as the
/// hardware (#1623).
///
/// The cells start where the 6845's display does, as every mode's do: the
/// core places the picture by the 6845's character count, not by its position
/// against horizontal sync, and does not model R2. Against sync, the MOS's
/// MODE 7 sits a microsecond right of MODE 0-6: R2 = 51 starts the 6845's
/// display two characters earlier than MODE 4-6's 49 (§18.3.3), and the
/// SAA5050 puts each character out [`TELETEXT_PICTURE_DELAY`] clocks after
/// the 6845 addresses it. jsbeeb and b-em draw it there.
const TELETEXT_CELL_WIDTH: usize = FAST_CHAR_PIXELS * 2;

/// Each channel of a MODE 7 pixel that the foreground lights none, a third,
/// two thirds or all of, the rest being background, as an sRGB value.
///
/// Twelve half-dots across sixteen pixels make each half-dot four thirds of
/// a pixel, so a pixel holds one half-dot, or a third of one and two thirds
/// of the next. Each pixel is drawn as the light the SAA5050's output puts
/// into it — the coverage mixed in linear light, then sRGB-encoded — so a
/// half-dot's edge between pixels keeps its position rather than snapping to
/// one of them, and a dot is the same width wherever it falls. jsbeeb draws
/// the same thirds (`makeHiResGlyphs`) and mixes them through a 2.2 gamma;
/// b-em mixes sixteen levels straight in sRGB. Snapping each half-dot to the
/// nearest pixels instead would draw dots two and three pixels wide in turn,
/// and move the half-dot steps character rounding draws.
const TELETEXT_COVERAGE_LEVELS: [u8; 4] = [0, 156, 213, 255];

/// What the Video ULA and SAA5050 put out while the 6845 is not displaying.
const BLANK: u32 = 0xFF00_0000;

/// Character clocks between the 6845 addressing a MODE 7 character and the
/// SAA5050 putting it on screen.
///
/// The cell is drawn here at the column it was fetched in, but on the
/// hardware it reaches the ULA three clocks later, which is why the MOS sets a
/// two-character cursor skew in MODE 7 (Advanced User Guide §18.6.3) and
/// enables only the second cursor segment (§19.1.7, `&4B`): skew 2 plus three
/// segment stages lands the cursor three clocks after the match, on the
/// character that matched. jsbeeb models the delay as the SAA5050's
/// four-entry input queue.
const TELETEXT_PICTURE_DELAY: u8 = 3;

/// The ULA's cursor sequence is seven stages long; stage 0 is idle.
const CURSOR_STAGES: u8 = 7;

/// How much of each of a MODE 7 cell's sixteen pixels the foreground covers,
/// in thirds (0-3), from the SAA5050's twelve half-dots, leftmost in bit 11.
///
/// Every three half-dots fill four pixels: the first pixel is the first
/// half-dot, the second a third of it and two thirds of the next, the third
/// two thirds of that and a third of the last, and the fourth the last.
fn teletext_coverage(pattern: u16) -> [u8; TELETEXT_CELL_WIDTH] {
    const _: () = assert!(saa5050::CELL_WIDTH * 4 == TELETEXT_CELL_WIDTH * 3);
    let lit = |half_dot: usize| u8::from(pattern & (0x800 >> half_dot) != 0);
    let mut coverage = [0; TELETEXT_CELL_WIDTH];
    for (group, pixels) in coverage.as_chunks_mut::<4>().0.iter_mut().enumerate() {
        let (a, b, c) = (lit(group * 3), lit(group * 3 + 1), lit(group * 3 + 2));
        *pixels = [3 * a, a + 2 * b, 2 * b + c, 3 * c];
    }
    coverage
}

/// A MODE 7 pixel `coverage` thirds foreground and the rest background, as
/// ARGB. The colours are the SAA5050's three bits, red, green and blue from
/// bit 0. See [`TELETEXT_COVERAGE_LEVELS`].
fn teletext_pixel(fg: u8, bg: u8, coverage: u8) -> u32 {
    let level = |bit: u8| -> u32 {
        let thirds = match (fg & bit != 0, bg & bit != 0) {
            (true, true) => 3,
            (false, false) => 0,
            (true, false) => coverage,
            (false, true) => 3 - coverage,
        };
        u32::from(TELETEXT_COVERAGE_LEVELS[usize::from(thirds)])
    };
    0xFF00_0000 | (level(0x01) << 16) | (level(0x02) << 8) | level(0x04)
}

/// The ULA's cursor inverts the red, green and blue it passes, before they
/// leave the machine. A MODE 7 pixel mixed from foreground and background
/// inverts to the same mix of the inverted colours, so a channel a third lit
/// becomes two thirds lit; the full and empty channels of every other pixel
/// just swap.
fn invert_rgb(argb: u32) -> u32 {
    let [low, high] = [TELETEXT_COVERAGE_LEVELS[1], TELETEXT_COVERAGE_LEVELS[2]];
    let invert = |shift: u32| -> u32 {
        let channel = (argb >> shift) as u8;
        let inverted = if channel == low {
            high
        } else if channel == high {
            low
        } else {
            !channel
        };
        u32::from(inverted) << shift
    };
    (argb & 0xFF00_0000) | invert(16) | invert(8) | invert(0)
}

fn blank_frame() -> Vec<u32> {
    vec![BLANK; (FB_WIDTH * FB_HEIGHT) as usize]
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
    /// The last complete frame the 6845 scanned out.
    framebuffer: Vec<u32>,
    /// The frame being scanned out now. Swapped with `framebuffer` when the
    /// 6845 starts a new frame, so a reader never sees half of one picture
    /// over half of another — the 6845's frame need not begin where
    /// [`Self::run_frame`]'s fixed tick budget does.
    back_buffer: Vec<u32>,
    /// Scan lines since the 6845's frame began: the beam's line, which
    /// [`Self::beam_rows`] places in `back_buffer`. The window's origin is the 6845's own — column 0, the
    /// first line of the frame — so it is where R12/R13's first character
    /// lands.
    beam_line: u16,
    /// The 6845 finished a frame; the next line it starts is line 0 of the
    /// next.
    video_frame_ended: bool,
    /// The SAA5050 teletext character generator's state.
    saa5050: Saa5050,
    /// Where the Video ULA is in its cursor sequence: 0 idle, otherwise the
    /// stage this character clock is at (see [`VideoUla::cursor_segment`]).
    cursor_stage: u8,
    cpu_cycles: u64,
    /// 2 MHz master-clock ticks since construction. The CPU runs at
    /// 2 MHz (one tick per cycle) for RAM, ROM and fast I/O, but stretches
    /// to the end of a whole 1 MHz cycle (two or three ticks) for the 1 MHz
    /// peripherals — the BBC bus contention. The frame is a fixed 312 × 128 master ticks; the CPU
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
        let mut crtc = Crtc6845::new();
        crtc.set_variant(Crtc6845Variant::Hd6845s);
        Self {
            cpu,
            crtc,
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
            framebuffer: blank_frame(),
            back_buffer: blank_frame(),
            beam_line: 0,
            video_frame_ended: true,
            saa5050: Saa5050::new(),
            cursor_stage: 0,
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

    /// Number of sideways banks holding a nonempty ROM image.
    #[must_use]
    pub fn sideways_rom_count(&self) -> usize {
        self.sideways_roms
            .iter()
            .filter(|rom| !rom.is_empty())
            .count()
    }

    /// Run one PAL frame.
    pub fn run_frame(&mut self) -> u64 {
        let start = self.master_ticks;
        while self.master_ticks - start < CYCLES_PER_FRAME {
            self.tick_cpu_cycle();
        }
        CYCLES_PER_FRAME
    }

    /// Close off the scanline the machine has just finished: drive the CRTC's
    /// VSYNC into the System VIA and step to the next.
    ///
    /// This used to live in `run_frame`'s loop, so none of it happened when
    /// the debugger stepped instructions. The VIAs and the CRTC tick per
    /// cycle, so timer interrupts were fine, but the VSYNC line into CA1 was
    /// never driven -- a stepped machine ran without its 50 Hz interrupt
    /// (#1202). Painting is no longer done here at all: the display is drawn
    /// a character at a time as the 6845 produces it (#163).
    ///
    /// The frame is a fixed 312 x 128 = 39,936 master ticks at 2 MHz. Each
    /// line is 128 ticks; the CPU fits a variable number of 6502 cycles into
    /// one because accesses to the 1 MHz peripherals cost two or three. Anchor
    /// the boundaries to a frame base so a cycle that overruns one carries
    /// its extra tick into the next line rather than stretching the frame.
    fn finish_scanline(&mut self) {
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
        let cost = Self::access_master_ticks(self.cpu.addr, self.master_ticks);
        // The chips run off the constant 2 MHz clock, so they advance one
        // tick per master tick — one, two or three per 6502 cycle depending
        // on whether this access hit a 1 MHz peripheral, and where in the
        // 1 MHz cycle it started.
        for tick in 0..cost {
            // A stretched cycle reaches its device in the 1 MHz cycle it
            // ends with, so the access lands in the final tick, ahead of the
            // edge that closes it. A 2 MHz cycle has only the one tick.
            if tick + 1 == cost {
                if self.cpu.rw {
                    self.cpu.data_in = self.mem_read(self.cpu.addr);
                } else {
                    self.mem_write(self.cpu.addr, self.cpu.data);
                }
            }
            let one_mhz_edge = Self::ends_one_mhz_cycle(self.master_ticks + tick);
            // The 6845 runs at 2 MHz in MODE 0-3 and 1 MHz in MODE 4-7:
            // Video ULA control bit 4 picks which (Advanced User Guide
            // §19.1.4). Both give a 64 µs line and a 50 Hz frame.
            if self.video_ula.fast_clock() || one_mhz_edge {
                self.clock_video();
            }
            // Both VIAs sit on the 1 MHz clock (Advanced User Guide §28.5),
            // so their timers count microseconds: one VIA cycle per two
            // master ticks.
            if one_mhz_edge {
                self.system_via.tick();
                self.user_via.tick();
            }
            // The SN76489 runs off a 4 MHz clock (Advanced User Guide
            // §23.3.1): two of its cycles per 2 MHz master tick.
            self.psg.tick();
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

    /// Whether master tick `tick` is the second half of a 1 MHz cycle, at the
    /// end of which the 1 MHz clock (1MHzE) falls. The 1 MHz peripherals —
    /// both VIAs, and the 6845 in MODE 4-7 — are clocked on that edge.
    const fn ends_one_mhz_cycle(tick: u64) -> bool {
        tick & 1 == 0
    }

    /// Whether `addr` is on the 1 MHz bus: FRED (`$FC00`), JIM (`$FD00`),
    /// and the slow SHEILA devices — 6845 CRTC / ACIA / serial ULA
    /// (`$FE00-$FE1F`), System VIA (`$FE40-$FE5F`), User VIA
    /// (`$FE60-$FE7F`) and the ADC (`$FEC0-$FEDF`). RAM, ROM and the rest of
    /// SHEILA (Video ULA, ROM latch, FDC, Econet, Tube) run at 2 MHz. Matches
    /// MAME `bbc_state::set_cpu_clock`, b-em and jsbeeb's `FEslowdown` table,
    /// and the MiSTer core's `mhz1_enable`.
    const fn is_one_mhz(addr: u16) -> bool {
        match addr & 0xFF00 {
            0xFC00 | 0xFD00 => true,
            0xFE00 => matches!(addr & 0x00E0, 0x00 | 0x40 | 0x60 | 0xC0),
            _ => false,
        }
    }

    /// Master ticks (2 MHz) a 6502 cycle accessing `addr`, starting at master
    /// tick `start`, consumes — the BBC's 1 MHz-bus clock stretching.
    ///
    /// A 2 MHz access takes one tick. For a 1 MHz one the slow-down circuit
    /// holds the CPU clock high "until the next coincident falling edge of
    /// the 2MHz and 1MHz clocks", and the device needs a whole 1MHzE high
    /// phase after the address is out (Advanced User Guide §28.4 pin 4,
    /// §28.5.2 and figure 28.2). A cycle that starts as a 1 MHz cycle begins
    /// runs to that cycle's end: two ticks. One that starts half-way through
    /// has to let the rest of that cycle go and take the whole of the next:
    /// three ticks.
    ///
    /// b-em (`do_readmem`: `polltime(2)` or `polltime(1)` on cycle parity)
    /// and jsbeeb (`polltimeAddr`: `1 + ((cycles ^ currentCycles) & 1)`)
    /// charge the same one or two extra ticks; the MiSTer core's
    /// `cycle_stretch` masks one or two CPU slots on the 1 MHz enable.
    /// MAME halves the clock instead, always two ticks, with no phase.
    const fn access_master_ticks(addr: u16, start: u64) -> u64 {
        if !Self::is_one_mhz(addr) {
            1
        } else if Self::ends_one_mhz_cycle(start) {
            3
        } else {
            2
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
            // Each SHEILA device answers across its whole block, repeating
            // its registers (the Advanced User Guide's SHEILA map, which
            // opens its hardware section: "the same devices appear at
            // several different Sheila addresses").
            0xFE40..=0xFE5F => self.system_via.read((addr & 0x0F) as u8),
            0xFE60..=0xFE7F => self.user_via.read((addr & 0x0F) as u8),
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
            // SHEILA devices repeat across their blocks: the Video ULA's two
            // registers through `&FE2F`, the ROM select through `&FE3F`, each
            // VIA's sixteen through 32 bytes.
            0xFE20..=0xFE2F if addr & 1 == 0 => self.video_ula.write_control(value),
            0xFE20..=0xFE2F => self.video_ula.write_palette(value),
            0xFE08..=0xFE0F => self.acia.write(addr, value),
            // Serial ULA: RX/TX baud, RS423/cassette select, bit 7 motor relay.
            0xFE10..=0xFE1F => self.serial_ula = value,
            0xFE30..=0xFE3F => self.rom_bank = value & 0x0F,
            0xFE40..=0xFE5F => {
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
            0xFE60..=0xFE7F => self.user_via.write((addr & 0x0F) as u8, value),
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

    /// One 6845 character clock: tick the CRTC and draw what it addressed.
    ///
    /// Everything about the picture comes from the chip's own outputs — the
    /// memory address (MA), raster address (RA) and display enable — the same
    /// signals the BBC's video circuitry is wired to. Nothing is re-derived
    /// from the line number or from the mode, so R1, R6, R9, R12/R13 and the
    /// rest take effect exactly when the 6845 acts on them (#163).
    fn clock_video(&mut self) {
        // Read the column first: it names the character this tick puts out.
        let column = self.crtc.horizontal_counter();
        let frame_ended = self.crtc.tick();
        // The SAA5050 counts lines on DISPTMG and fields on VSYNC whatever
        // mode the Video ULA is in. R8 bits 4-5 = `11` hold DISPTMG off.
        let disptmg = self.crtc.display_enable && self.crtc.regs()[8] & 0x30 != 0x30;
        self.saa5050.clock_timing(disptmg, self.crtc.vsync);
        if column == 0 {
            self.start_video_line();
        }
        self.draw_character(column);
        self.draw_cursor(column);
        if frame_ended {
            self.video_frame_ended = true;
        }
    }

    /// The 6845 has begun a scan line. Advance the beam, present the field
    /// just finished if this is the first line of the next, and clear the
    /// line: anything the 6845 does not display is black, because the BBC has
    /// no border colour.
    ///
    /// The new field is drawn over the picture just presented, so the rows it
    /// does not scan keep the other field's lines: that is the woven picture
    /// an interlaced set shows (see [`FB_HEIGHT`]).
    fn start_video_line(&mut self) {
        if self.video_frame_ended {
            self.video_frame_ended = false;
            self.beam_line = 0;
            core::mem::swap(&mut self.framebuffer, &mut self.back_buffer);
            self.back_buffer.copy_from_slice(&self.framebuffer);
        } else {
            self.beam_line = self.beam_line.saturating_add(1);
        }
        if let Some(rows) = self.beam_rows() {
            let width = FB_WIDTH as usize;
            self.back_buffer[rows.start * width..rows.end * width].fill(BLANK);
        }
    }

    /// The framebuffer rows the beam's current line lands on, or `None` below
    /// the bottom of the window.
    ///
    /// In interlace sync and video mode (MODE 7) each field scans alternate
    /// lines of every character row, the even field the even raster addresses
    /// and the odd field the odd ones (Advanced User Guide figure 18.2c), so a
    /// line fills only its own field's row. Otherwise both fields scan the same
    /// lines and each fills both rows.
    fn beam_rows(&self) -> Option<core::ops::Range<usize>> {
        let first = usize::from(self.beam_line) * ROWS_PER_LINE;
        if first >= FB_HEIGHT as usize {
            return None;
        }
        Some(if self.crtc.interlace_sync_and_video() {
            let row = first + usize::from(self.crtc.odd_field());
            row..row + 1
        } else {
            first..first + ROWS_PER_LINE
        })
    }

    /// Copy columns `x0..x1` of the first row in `rows` to the others.
    fn repeat_span(&mut self, rows: core::ops::Range<usize>, x0: usize, x1: usize) {
        let width = FB_WIDTH as usize;
        let source = rows.start * width;
        for row in rows.skip(1) {
            self.back_buffer
                .copy_within(source + x0..source + x1, row * width + x0);
        }
    }

    /// Where the video circuitry fetches the byte for a 6845 address.
    ///
    /// - **MA13 set: teletext.** The SAA5050 reads one byte per character
    ///   from a 1K window; MA0-9 address it and MA11 picks `$7C00` or, on the
    ///   Model B only, `$3C00`. That is why the Advanced User Guide's MODE 7
    ///   start address is the RAM address "minus &74, EOR &20" (§18.11.3): it
    ///   sets MA13 and MA11. jsbeeb's `readVideoMem` decodes the same bits.
    /// - **Otherwise: bitmap.** Each character is eight consecutive bytes, so
    ///   the address is MA × 8 plus RA0-2. Past `$7FFF` the hardware-scroll
    ///   wrap brings it back into the screen (§18.10; see
    ///   [`AddressableLatch::screen_wrap_size`]) (#164).
    fn video_address(&self, ma: u16, ra: u8) -> u16 {
        if ma & 0x2000 != 0 {
            let bank = if ma & 0x0800 != 0 { 0x7C00 } else { 0x3C00 };
            bank | (ma & 0x03FF)
        } else {
            let address = ((ma & 0x1FFF) << 3) | u16::from(ra & 0x07);
            if address & 0x8000 == 0 {
                address
            } else {
                address.wrapping_sub(self.latch.screen_wrap_size()) & 0x7FFF
            }
        }
    }

    /// Draw the character the 6845 is addressing at `column` of the current
    /// line, if it is displaying one.
    fn draw_character(&mut self, column: u8) {
        let Some(rows) = self.beam_rows() else {
            return;
        };
        let ma = self.crtc.memory_address();
        let ra = self.crtc.raster_address();
        let byte = self.ram[usize::from(self.video_address(ma, ra))];
        if self.video_ula.teletext() {
            self.draw_teletext_character(rows, usize::from(column), byte, ra);
            return;
        }
        // Display enable is masked by RA3, so a cell taller than eight lines
        // blanks the rest: the gaps between rows in the gapped text modes 3
        // and 6. R8 bits 4-5 = `11` turns the display off outright (§18.6.2).
        if !self.crtc.display_enable || ra & 0x08 != 0 || self.crtc.regs()[8] & 0x30 == 0x30 {
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
        let char_pixels = if self.video_ula.fast_clock() {
            FAST_CHAR_PIXELS
        } else {
            FAST_CHAR_PIXELS * 2
        };
        let pixels_per_byte = self.video_ula.pixels_per_byte();
        let pixel_width = char_pixels / pixels_per_byte;
        let x0 = usize::from(column) * char_pixels;
        let offset = rows.start * FB_WIDTH as usize;
        let mut shiftreg = byte;
        for px in 0..pixels_per_byte {
            let colour_idx = ((shiftreg >> 4) & 0x08)
                | ((shiftreg >> 3) & 0x04)
                | ((shiftreg >> 2) & 0x02)
                | ((shiftreg >> 1) & 0x01);
            shiftreg = (shiftreg << 1) | 1;
            let argb = self.video_ula.palette_to_argb(colour_idx);
            let x = x0 + px * pixel_width;
            for fb_x in x..(x + pixel_width).min(FB_WIDTH as usize) {
                self.back_buffer[offset + fb_x] = argb;
            }
        }
        let x1 = (x0 + char_pixels).min(FB_WIDTH as usize);
        if x0 < x1 {
            self.repeat_span(rows, x0, x1);
        }
    }

    /// Run the Video ULA's cursor sequence for the character clock at
    /// `column`, inverting the picture where it is enabled.
    ///
    /// The 6845 raises CURSOR for the character at R14/R15 on the raster
    /// lines R10-R11 pick, when R10's blink lets it (Advanced User Guide §18.8;
    /// the chip model does this). R8 bits 6-7 skew it by none, one or two
    /// characters, or switch it off (§18.6.3). The ULA then runs its own
    /// sequence of character clocks from that pulse, each gated by a control
    /// bit, and inverts the RGB output over the enabled ones. It works in
    /// teletext too, since the SAA5050's output goes through the ULA; there
    /// the picture runs [`TELETEXT_PICTURE_DELAY`] clocks behind the fetch.
    fn draw_cursor(&mut self, column: u8) {
        let skew = self.crtc.regs()[8] >> 6;
        if self.crtc.cursor_active && skew != 3 {
            self.cursor_stage = 3 - skew;
        }
        if self.cursor_stage == 0 {
            return;
        }
        let stage = self.cursor_stage;
        self.cursor_stage = (stage + 1) % CURSOR_STAGES;
        if !self.video_ula.cursor_segment(stage) {
            return;
        }
        let Some(rows) = self.beam_rows() else {
            return;
        };
        let (cell, width) = if self.video_ula.teletext() {
            let Some(cell) = column.checked_sub(TELETEXT_PICTURE_DELAY) else {
                return;
            };
            (cell, TELETEXT_CELL_WIDTH)
        } else if self.video_ula.fast_clock() {
            (column, FAST_CHAR_PIXELS)
        } else {
            (column, FAST_CHAR_PIXELS * 2)
        };
        let x0 = usize::from(cell) * width;
        if x0 >= FB_WIDTH as usize {
            return;
        }
        let x1 = (x0 + width).min(FB_WIDTH as usize);
        for row in rows {
            let offset = row * FB_WIDTH as usize;
            for argb in &mut self.back_buffer[offset + x0..offset + x1] {
                *argb = invert_rgb(*argb);
            }
        }
    }

    /// Feed one character to the SAA5050 (MODE 7) and draw the cell it puts
    /// out: twelve half-dots of foreground and background in the fixed 3-bit
    /// teletext colours, not the Video ULA palette, across the sixteen pixels
    /// of [`TELETEXT_CELL_WIDTH`]. See [`saa5050`] for how the chip decides
    /// them.
    ///
    /// The character rounding select input is RA0, so in MODE 7's interlace
    /// sync and video mode the even field draws the upper line of each pair
    /// of glyph lines and the odd field the lower.
    fn draw_teletext_character(
        &mut self,
        rows: core::ops::Range<usize>,
        column: usize,
        byte: u8,
        ra: u8,
    ) {
        if !self.crtc.display_enable || self.crtc.regs()[8] & 0x30 == 0x30 {
            return;
        }
        // Only D0-D6 reach the SAA5050, so `$81` is the control code `$01`.
        // That is how the MOS's coloured text works; b-em masks the same way.
        let cell = self
            .saa5050
            .character(byte & 0x7F, ra & 0x01 != 0, &self.teletext_font);
        let x0 = column * TELETEXT_CELL_WIDTH;
        let offset = rows.start * FB_WIDTH as usize;
        for (px, coverage) in teletext_coverage(cell.pattern).into_iter().enumerate() {
            let fb_x = x0 + px;
            if fb_x >= FB_WIDTH as usize {
                break;
            }
            self.back_buffer[offset + fb_x] = teletext_pixel(cell.fg, cell.bg, coverage);
        }
        let x1 = (x0 + TELETEXT_CELL_WIDTH).min(FB_WIDTH as usize);
        if x0 < x1 {
            self.repeat_span(rows, x0, x1);
        }
    }

    /// Framebuffer (640×512 ARGB32): both fields, woven (see [`FB_HEIGHT`]).
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
    fn one_mhz_bus_accesses_cost_two_or_three_ticks_rest_one() {
        // Even and odd starts: a 1 MHz access costs two ticks when it starts
        // as a 1 MHz cycle begins (odd here) and three half-way through one.
        for start in [10u64, 11] {
            let slow = if BbcMicro::ends_one_mhz_cycle(start) {
                3
            } else {
                2
            };
            // Fast (2 MHz):
            for addr in [
                0x0000, 0xC000, 0x8000, 0xFE20, 0xFE30, 0xFE80, 0xFEA0, 0xFEE0,
            ] {
                assert_eq!(BbcMicro::access_master_ticks(addr, start), 1, "{addr:04X}");
            }
            // Slow (1 MHz bus): FRED, JIM, CRTC, ACIA, serial ULA, both
            // VIAs, ADC.
            for addr in [
                0xFC00, 0xFD00, 0xFE00, 0xFE08, 0xFE10, 0xFE40, 0xFE5F, 0xFE60, 0xFEC0,
            ] {
                assert_eq!(
                    BbcMicro::access_master_ticks(addr, start),
                    slow,
                    "{addr:04X}"
                );
            }
        }
        assert_eq!(BbcMicro::access_master_ticks(0xFE40, 11), 2);
        assert_eq!(BbcMicro::access_master_ticks(0xFE40, 10), 3);
    }

    /// A 16 KB MOS ROM whose reset vector runs `program` at `$C000`.
    fn rom_running(program: &[u8]) -> Vec<u8> {
        let mut rom = trap_rom();
        rom[..program.len()].copy_from_slice(program);
        rom
    }

    /// Master ticks per pass of a loop of `instructions` instructions, once
    /// the loop has settled onto its steady phase against the 1 MHz clock.
    fn ticks_per_pass(program: &[u8], instructions: usize) -> u64 {
        const PASSES: u64 = 64;
        let mut sys = BbcMicro::new(rom_running(program));
        // Reset, then a few passes to settle the phase.
        for _ in 0..8 * instructions {
            sys.step_instruction();
        }
        let start = sys.master_ticks;
        for _ in 0..PASSES as usize * instructions {
            sys.step_instruction();
        }
        let total = sys.master_ticks - start;
        assert_eq!(
            total % PASSES,
            0,
            "the loop should settle to a fixed period"
        );
        total / PASSES
    }

    /// The stretched cycle ends on a falling edge of the 1 MHz clock, and
    /// a cycle that starts while that clock is high must wait out the low
    /// half and a whole further high half before it can (Advanced User Guide
    /// §28.4 pin 4 and §28.5.2). So a 1 MHz access costs two master ticks
    /// when it starts as the 1 MHz cycle begins, and three when it starts
    /// half-way through one.
    ///
    /// `LDA &FE4F : JMP loop` has six 2 MHz cycles between VIA reads, an even
    /// number, so every read starts in phase: 6 + 2 = 8 ticks a pass. Add a
    /// three-cycle `BIT &00` and the gap is odd, so every read starts half a
    /// microsecond out and costs three: 9 + 3 = 12, not 11.
    #[test]
    fn one_mhz_access_waits_for_the_next_whole_one_mhz_cycle() {
        // LDA &FE4F : JMP &C000
        let in_phase = [0xAD, 0x4F, 0xFE, 0x4C, 0x00, 0xC0];
        assert_eq!(ticks_per_pass(&in_phase, 2), 8);
        // LDA &FE4F : BIT &00 : JMP &C000
        let out_of_phase = [0xAD, 0x4F, 0xFE, 0x24, 0x00, 0x4C, 0x00, 0xC0];
        assert_eq!(ticks_per_pass(&out_of_phase, 3), 12);
        // The User VIA, the CRTC and FRED stretch the same way.
        for page_and_offset in [[0x6F, 0xFE], [0x01, 0xFE], [0x00, 0xFC]] {
            let program = [
                0xAD,
                page_and_offset[0],
                page_and_offset[1],
                0x24,
                0x00,
                0x4C,
                0x00,
                0xC0,
            ];
            assert_eq!(ticks_per_pass(&program, 3), 12, "{page_and_offset:02X?}");
        }
        // RAM, ROM and the fast SHEILA devices never stretch: LDA &FE30
        // (the ROM latch) : BIT &00 : JMP is 4 + 3 + 3 = 10.
        let fast = [0xAD, 0x30, 0xFE, 0x24, 0x00, 0x4C, 0x00, 0xC0];
        assert_eq!(ticks_per_pass(&fast, 3), 10);
    }

    /// Whatever the phase it starts in, a cycle that touches the 1 MHz bus
    /// ends as a 1 MHz cycle ends (Advanced User Guide §28.4: "The trailing
    /// edges of the 1MHzE and 2MHz processor clock are then coincidental").
    /// Exercise every phase with a read-modify-write, whose three VIA
    /// accesses follow back to back, and an odd-length gap between passes.
    #[test]
    fn every_one_mhz_cycle_ends_on_the_one_mhz_clock() {
        // INC &FE4F : BIT &00 : JMP &C000
        let program = [0xEE, 0x4F, 0xFE, 0x24, 0x00, 0x4C, 0x00, 0xC0];
        let mut sys = BbcMicro::new(rom_running(&program));
        let mut slow_cycles = 0;
        for _ in 0..2_000 {
            let start = sys.master_ticks;
            sys.tick_cpu_cycle();
            let addr = sys.cpu.addr;
            if BbcMicro::is_one_mhz(addr) {
                slow_cycles += 1;
                assert_eq!(
                    sys.master_ticks % 2,
                    1,
                    "a 1 MHz access at {addr:04X} from tick {start} ended mid-way \
                     through a 1 MHz cycle"
                );
            }
        }
        assert!(slow_cycles > 100, "the loop should hit the VIA");
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

    /// SHEILA devices repeat across their blocks (the Advanced User Guide's
    /// SHEILA map): the Video ULA at `&20-&2F`, the ROM select at
    /// `&30-&3F`, the VIAs at `&40-&5F` and `&60-&7F`. Only the first
    /// address of each used to answer.
    #[test]
    fn sheila_devices_answer_across_their_whole_blocks() {
        let mut sys = BbcMicro::new(trap_rom());
        sys.mem_write(0xFE2E, 0x9D);
        assert_eq!(sys.video_ula.control, 0x9D, "&FE2E is the control register");
        sys.mem_write(0xFE2F, 0x53);
        assert_eq!(sys.video_ula.palette[5], 3, "&FE2F is the palette");
        sys.mem_write(0xFE3F, 0x07);
        assert_eq!(sys.rom_bank(), 7, "&FE3F selects a ROM");
        sys.mem_write(0xFE52, 0xFF); // system VIA DDRB, at its mirror
        assert_eq!(sys.mem_read(0xFE42), 0xFF);
        sys.mem_write(0xFE73, 0xA5); // user VIA DDRA, at its mirror
        assert_eq!(sys.mem_read(0xFE63), 0xA5);
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

    /// Both VIAs are clocked by the 1 MHz clock, not the 2 MHz CPU clock
    /// (Advanced User Guide §28.5: "All 1MHz peripherals are clocked by a
    /// 1MHz 50% duty cycle square wave … to allow chips such as 6522 VIAs to
    /// use their internal timing elements correctly"). A one-shot T1 loaded
    /// with N times out N + 1.5 µs later — about 2N + 3 master ticks. Clocked
    /// at 2 MHz it fired in half that, and the MOS's 100 Hz tick ran at 200.
    #[test]
    fn via_timers_count_at_one_megahertz() {
        const LATCH: u64 = 100;
        let mut sys = BbcMicro::new(trap_rom());
        for via_base in [0xFE40u16, 0xFE60] {
            sys.mem_write(via_base + 0x04, LATCH as u8); // T1 latch low
            sys.mem_write(via_base + 0x05, 0); // T1 counter high: start
        }
        let start = sys.master_ticks;
        let mut fired = [None, None];
        while fired.contains(&None) && sys.master_ticks - start < 1_000 {
            sys.tick_cpu_cycle();
            for (slot, via_base) in fired.iter_mut().zip([0xFE40u16, 0xFE60]) {
                if slot.is_none() && sys.mem_read(via_base + 0x0D) & 0x40 != 0 {
                    *slot = Some(sys.master_ticks - start);
                }
            }
        }
        for (name, ticks) in ["System VIA", "User VIA"].iter().zip(fired) {
            let ticks = ticks.unwrap_or(u64::MAX);
            assert!(
                (2 * LATCH..=2 * LATCH + 6).contains(&ticks),
                "{name} T1 = {LATCH} should time out after ~{} master ticks \
                 (1 MHz), took {ticks}",
                2 * LATCH + 3
            );
        }
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

    /// The BBC's SN76489 runs off a 4 MHz clock: "frequency = 4 000 000/32 x
    /// 10 bit binary number" (Advanced User Guide §23.3.1). A tone of N = 125
    /// is 1 kHz, so one emulated second holds 48,000 samples crossing zero
    /// 2,000 times. Ticked once per 2 MHz master tick it produced half the
    /// samples, each pitched an octave low.
    #[test]
    fn psg_runs_off_a_four_megahertz_clock() {
        const N: u16 = 125;
        let mut sys = BbcMicro::new(trap_rom());
        sys.psg.write(0x80 | (N & 0x0F) as u8); // tone 0, low four bits
        sys.psg.write((N >> 4) as u8); // tone 0, high six bits
        sys.psg.write(0x90); // tone 0 at full volume
        for silence in [0xBF, 0xDF, 0xFF] {
            sys.psg.write(silence);
        }
        // Let the DC blocker settle, then take one second.
        for _ in 0..10 {
            sys.run_frame();
        }
        sys.take_audio_buffer();
        for _ in 0..50 {
            sys.run_frame();
        }
        let samples = sys.take_audio_buffer();
        assert!(
            (47_900..=48_100).contains(&samples.len()),
            "one second should hold 48,000 samples; got {}",
            samples.len()
        );
        let crossings = samples
            .windows(2)
            .filter(|pair| (pair[0] < 0.0) != (pair[1] < 0.0))
            .count();
        assert!(
            (1_980..=2_020).contains(&crossings),
            "a 1 kHz tone crosses zero 2,000 times a second; got {crossings}"
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

    /// Program the 6845 the way the MOS does for a mode (Advanced User Guide
    /// §18 register tables) and the Video ULA's control register.
    fn program_mode(sys: &mut BbcMicro, crtc: [u8; 14], ula_control: u8) {
        for (reg, value) in crtc.into_iter().enumerate() {
            sys.mem_write(0xFE00, reg as u8);
            sys.mem_write(0xFE01, value);
        }
        sys.mem_write(0xFE20, ula_control);
    }

    const MODE0_CRTC: [u8; 14] = [
        127, 80, 98, 0x28, 38, 0, 32, 34, 0x01, 7, 0x67, 8, 0x06, 0x00,
    ];
    const MODE4_CRTC: [u8; 14] = [
        63, 40, 49, 0x24, 38, 0, 32, 34, 0x01, 7, 0x67, 8, 0x0B, 0x00,
    ];
    const MODE7_CRTC: [u8; 14] = [
        63, 40, 51, 0x24, 30, 2, 25, 27, 0x93, 18, 0x72, 19, 0x28, 0x00,
    ];

    /// MODE 0's two-colour palette: logical 0-7 black, 8-15 white, the way
    /// the MOS sets it so a shifted-in `1` cannot change the colour.
    fn mode0_palette(sys: &mut BbcMicro) {
        for logical in 0..16u8 {
            let physical = if logical >= 8 { 7 } else { 0 };
            sys.mem_write(0xFE21, (logical << 4) | (physical ^ 7));
        }
    }

    /// Set addressable-latch output `bit` to `on` through System VIA port B.
    fn set_latch(sys: &mut BbcMicro, bit: u8, on: bool) {
        sys.mem_write(0xFE40, bit | if on { 0x08 } else { 0 });
    }

    const WHITE: u32 = 0xFFFF_FFFF;

    fn pixel(sys: &BbcMicro, x: usize, y: usize) -> u32 {
        sys.framebuffer()[y * FB_WIDTH as usize + x]
    }

    /// A screen scrolled so its first row sits at the top of RAM carries on
    /// from `$3000`, not from ROM: the wrap circuit takes MODE 0's 20K off
    /// any address past `$7FFF` (Advanced User Guide §18.10) (#164). Before,
    /// the second row read past RAM and came out black.
    #[test]
    fn a_scrolled_screen_wraps_past_the_top_of_ram() {
        let mut sys = BbcMicro::new(trap_rom());
        let mut crtc = MODE0_CRTC;
        // Start one row (640 bytes) below the top of RAM.
        let start = (0x8000u16 - 640) / 8;
        crtc[12] = (start >> 8) as u8;
        crtc[13] = start as u8;
        program_mode(&mut sys, crtc, 0x9C);
        mode0_palette(&mut sys);
        // MODE 0's wrap: B5 = 1, B4 = 0, as the MOS sets it.
        set_latch(&mut sys, 5, true);
        set_latch(&mut sys, 4, false);
        for offset in 0..8 {
            sys.ram[0x7D80 + offset] = 0xFF; // row 0, first character
            sys.ram[0x3000 + offset] = 0xFF; // row 1 after the wrap
        }
        for _ in 0..3 {
            sys.run_frame();
        }
        // Two character rows of eight lines, each line on two rows.
        for y in 0..32 {
            for x in 0..8 {
                assert_eq!(pixel(&sys, x, y), WHITE, "lit cell at ({x}, {y})");
            }
            assert_eq!(pixel(&sys, 8, y), BLANK, "unlit neighbour at (8, {y})");
        }
    }

    /// Each latch setting wraps by its mode's screen size. Decoded from the
    /// bits the MOS writes; see `screen_wrap_size`.
    #[test]
    fn the_latch_selects_each_modes_screen_size() {
        let mut sys = BbcMicro::new(trap_rom());
        for (b5, b4, size, mode) in [
            (true, false, 0x5000u16, "0-2"),
            (false, false, 0x4000, "3"),
            (true, true, 0x2800, "4-5"),
            (false, true, 0x2000, "6"),
        ] {
            set_latch(&mut sys, 5, b5);
            set_latch(&mut sys, 4, b4);
            // MA $1000 is RAM address $8000: the first byte past the top.
            assert_eq!(
                sys.video_address(0x1000, 0),
                0x8000 - size,
                "MODE {mode} wraps to its own start"
            );
        }
    }

    /// The displayed rows come from the 6845's R6, not from the framebuffer's
    /// height: a 16-row display blanks everything below it (#163).
    #[test]
    fn rows_past_r6_are_not_displayed() {
        let mut sys = BbcMicro::new(trap_rom());
        let mut crtc = MODE0_CRTC;
        crtc[6] = 16;
        program_mode(&mut sys, crtc, 0x9C);
        mode0_palette(&mut sys);
        sys.ram[0x3000..0x8000].fill(0xFF);
        for _ in 0..3 {
            sys.run_frame();
        }
        assert_eq!(pixel(&sys, 0, 255), WHITE, "row 15 is displayed");
        assert_eq!(pixel(&sys, 0, 256), BLANK, "row 16 is past R6");
        assert_eq!(pixel(&sys, 320, 511), BLANK);
    }

    /// The slow-clock modes run the 6845 at 1 MHz, so their 64-character
    /// lines take the same 64 µs as MODE 0's 128 and the frame is still
    /// 50 Hz. Clocked at 2 MHz they produced VSYNC at 100 Hz (#163).
    #[test]
    fn slow_clock_modes_still_sync_at_fifty_hertz() {
        for (crtc, control, name) in [(MODE0_CRTC, 0x9C, "MODE 0"), (MODE4_CRTC, 0x88, "MODE 4")] {
            let mut sys = BbcMicro::new(trap_rom());
            program_mode(&mut sys, crtc, control);
            sys.run_frame();
            let mut edges = 0;
            let mut was = sys.crtc().vsync;
            let start = sys.master_ticks;
            while sys.master_ticks - start < CYCLES_PER_FRAME * 10 {
                sys.tick_cpu_cycle();
                let now = sys.crtc().vsync;
                edges += u32::from(now && !was);
                was = now;
            }
            assert_eq!(edges, 10, "{name}: one VSYNC per 20 ms frame");
        }
    }

    /// Run until the picture holds a whole MODE 7 frame: both of its fields,
    /// each started after the 6845 was programmed.
    fn run_both_fields(sys: &mut BbcMicro) {
        for _ in 0..4 {
            sys.run_frame();
        }
    }

    /// A glyph ROM whose `A` lights every pixel of every row and whose other
    /// glyphs are empty.
    fn font_with_a_lit() -> Vec<u8> {
        let mut font = vec![0u8; 96 * 10];
        let a = usize::from(b'A' - 0x20) * 10;
        font[a..a + 10].fill(0x3F);
        font
    }

    /// MODE 7 reads the characters the 6845 addresses. Its hardware scroll
    /// moves the start address, so the first character on screen is wherever
    /// R12/R13 point; it used to be read from `$7C00` regardless (#163).
    #[test]
    fn mode7_follows_the_start_address() {
        let mut sys = BbcMicro::new(trap_rom());
        sys.set_teletext_font(font_with_a_lit());
        let mut crtc = MODE7_CRTC;
        crtc[13] = 40; // scrolled up one row: the screen starts at $7C28
        program_mode(&mut sys, crtc, 0x4B);
        sys.ram[0x7C00..0x8000].fill(b' ');
        sys.ram[0x7C28] = b'A';
        run_both_fields(&mut sys);
        let x = 0;
        assert_eq!(pixel(&sys, x, 0), WHITE, "$7C28 is the top-left cell");
        assert_eq!(pixel(&sys, x, 20), BLANK, "and not the second row's");
    }

    /// MODE 7's 1K screen wraps on its own: MA0-9 address it, so the row
    /// after `$7FFF` is `$7C00` (Advanced User Guide §18.11.3).
    #[test]
    fn mode7_wraps_within_its_kilobyte() {
        let mut sys = BbcMicro::new(trap_rom());
        sys.set_teletext_font(font_with_a_lit());
        let mut crtc = MODE7_CRTC;
        // Start at $7FD8: forty bytes short of the top of RAM, one row.
        crtc[12] = 0x2B;
        crtc[13] = 0xD8;
        program_mode(&mut sys, crtc, 0x4B);
        sys.ram[0x7C00..0x8000].fill(b' ');
        sys.ram[0x7C00] = b'A';
        run_both_fields(&mut sys);
        assert_eq!(pixel(&sys, 0, 20), WHITE, "row 1 is $7C00");
    }

    /// MODE 7's interlace sync and video mode scans the even raster addresses
    /// in one field and the odd ones in the next (Advanced User Guide figure
    /// 18.2c), and the picture holds both: framebuffer row 0 is the even
    /// field's first line and row 1 the odd field's. Before, one 256-line
    /// field overwrote the other, so a glyph's first line filled one row.
    #[test]
    fn mode7_weaves_its_two_fields() {
        let mut sys = BbcMicro::new(trap_rom());
        let mut font = vec![0u8; 96 * 10];
        font[usize::from(b'A' - 0x20) * 10] = 0x3F; // only the glyph's top line
        sys.set_teletext_font(font);
        program_mode(&mut sys, MODE7_CRTC, 0x4B);
        sys.ram[0x7C00..0x8000].fill(b' ');
        sys.ram[0x7C00] = b'A';
        run_both_fields(&mut sys);
        let x = 0;
        assert_eq!(pixel(&sys, x, 0), WHITE, "the even field's line");
        assert_eq!(pixel(&sys, x, 1), WHITE, "the odd field's line");
        assert_eq!(pixel(&sys, x, 2), BLANK, "the glyph's second line");
    }

    /// Only D0-D6 reach the SAA5050, so the MOS's `CHR$129` is the
    /// red-alphanumerics control code and the text after it is red.
    #[test]
    fn teletext_control_codes_ignore_bit_7() {
        let mut sys = BbcMicro::new(trap_rom());
        sys.set_teletext_font(font_with_a_lit());
        program_mode(&mut sys, MODE7_CRTC, 0x4B);
        sys.ram[0x7C00..0x8000].fill(b' ');
        sys.ram[0x7C00] = 0x81;
        sys.ram[0x7C01] = b'A';
        run_both_fields(&mut sys);
        assert_eq!(
            pixel(&sys, TELETEXT_CELL_WIDTH, 0),
            0xFFFF_0000,
            "the A after CHR$129 is red"
        );
    }

    /// MODE 7 with `rows` of teletext, one string of codes per screen row,
    /// and a font whose `A` has the dot rows `a`. Both fields are drawn.
    fn teletext_screen(a: [u8; 10], rows: &[&[u8]]) -> BbcMicro {
        let mut sys = BbcMicro::new(trap_rom());
        let mut font = vec![0u8; 96 * 10];
        let glyph = usize::from(b'A' - 0x20) * 10;
        font[glyph..glyph + 10].copy_from_slice(&a);
        sys.set_teletext_font(font);
        program_mode(&mut sys, MODE7_CRTC, 0x4B);
        sys.ram[0x7C00..0x8000].fill(b' ');
        for (row, codes) in rows.iter().enumerate() {
            let start = 0x7C00 + row * 40;
            sys.ram[start..start + codes.len()].copy_from_slice(codes);
        }
        run_both_fields(&mut sys);
        sys
    }

    /// Framebuffer rows of MODE 7 cell (`column`, `row`) that have any
    /// pixel in `colour`, as offsets 0-19 from the top of the cell.
    fn cell_rows_in(sys: &BbcMicro, column: usize, row: usize, colour: u32) -> Vec<usize> {
        let x0 = column * TELETEXT_CELL_WIDTH;
        (0..20)
            .filter(|&y| {
                (x0..x0 + TELETEXT_CELL_WIDTH).any(|x| pixel(sys, x, row * 20 + y) == colour)
            })
            .collect()
    }

    /// How much of each pixel of one framebuffer row of a MODE 7 cell white
    /// text covers on black, in thirds.
    fn cell_line(sys: &BbcMicro, column: usize, y: usize) -> Vec<u8> {
        let x0 = column * TELETEXT_CELL_WIDTH;
        (0..TELETEXT_CELL_WIDTH)
            .map(|x| {
                let green = (pixel(sys, x0 + x, y) >> 8) as u8;
                let thirds = TELETEXT_COVERAGE_LEVELS.iter().position(|&l| l == green);
                thirds.expect("a white-on-black level") as u8
            })
            .collect()
    }

    /// Character rounding: a diagonal step from dot 2 on glyph row 1 to dot 3
    /// on row 2 is smoothed by half a dot, the odd field's line of row 1
    /// reaching towards row 2 and the even field's line of row 2 reaching
    /// back. Before, each dot row was drawn square on whichever field came
    /// last.
    ///
    /// The half-dots are 5-7 and 4-6 of twelve, four thirds of a pixel each,
    /// so the steps fall a third and two thirds of the way into pixels.
    #[test]
    fn mode7_rounds_diagonals_across_the_two_fields() {
        let sys = teletext_screen([0, 0x04, 0x08, 0, 0, 0, 0, 0, 0, 0], &[b"A"]);
        #[rustfmt::skip]
        let expected: [(usize, [u8; 16], &str); 4] = [
            (2, [0, 0, 0, 0, 0, 0, 0, 0, 3, 3, 2, 0, 0, 0, 0, 0], "row 1, even field"),
            (3, [0, 0, 0, 0, 0, 0, 1, 3, 3, 3, 2, 0, 0, 0, 0, 0], "row 1, odd field"),
            (4, [0, 0, 0, 0, 0, 2, 3, 3, 3, 1, 0, 0, 0, 0, 0, 0], "row 2, even field"),
            (5, [0, 0, 0, 0, 0, 2, 3, 3, 0, 0, 0, 0, 0, 0, 0, 0], "row 2, odd field"),
        ];
        for (y, thirds, line) in expected {
            assert_eq!(cell_line(&sys, 0, y), thirds, "{line}");
        }
    }

    /// The SAA5050 puts out a character a microsecond, the 6845's clock in
    /// MODE 7, so a cell is the sixteen pixels a microsecond fills on the
    /// 16 MHz framebuffer, and MODE 7's forty columns fill the 640 as MODE
    /// 0-6 do. They were twelve pixels each, centred in 480 (#1623).
    #[test]
    fn mode7_cells_are_a_microsecond_wide_and_fill_the_window() {
        assert_eq!(TELETEXT_CELL_WIDTH, 16);
        let line = |columns: &[usize]| {
            let mut row = [b' '; 40];
            for &column in columns {
                row[column] = b'A';
            }
            white_run(&teletext_screen([0x3F; 10], &[&row]), 0)
        };
        assert_eq!(line(&[0]), Some((0, 16)), "the first cell");
        assert_eq!(line(&[39]), Some((624, 640)), "the fortieth cell");
        let all: Vec<usize> = (0..40).collect();
        assert_eq!(line(&all), Some((0, FB_WIDTH as usize)), "forty cells");
    }

    /// Each half-dot covers four thirds of a pixel, from `4h/3` to
    /// `4(h + 1)/3`, so the thirds of each pixel it covers are the overlap of
    /// that span with the pixel's. The resampler must give every pattern the
    /// sum of its half-dots' overlaps: no light lost or moved.
    #[test]
    fn mode7_resampling_keeps_each_half_dot_where_it_falls() {
        let overlap = |half_dot: usize, pixel: usize| -> u8 {
            let start = (4 * half_dot).max(3 * pixel);
            let end = (4 * half_dot + 4).min(3 * pixel + 3);
            end.saturating_sub(start) as u8
        };
        for pattern in 0..0x1000u16 {
            let expected: Vec<u8> = (0..16)
                .map(|pixel| {
                    (0..12)
                        .filter(|&h| pattern & (0x800 >> h) != 0)
                        .map(|h| overlap(h, pixel))
                        .sum()
                })
                .collect();
            assert_eq!(
                teletext_coverage(pattern).to_vec(),
                expected,
                "{pattern:012b}"
            );
        }
    }

    /// The levels are coverage mixed in linear light and sRGB-encoded.
    #[test]
    fn mode7_coverage_levels_are_linear_light_in_srgb() {
        for (thirds, &level) in TELETEXT_COVERAGE_LEVELS.iter().enumerate() {
            let linear = thirds as f64 / 3.0;
            let encoded = if linear <= 0.003_130_8 {
                12.92 * linear
            } else {
                1.055 * linear.powf(1.0 / 2.4) - 0.055
            };
            assert_eq!(level, (encoded * 255.0).round() as u8, "{thirds} thirds");
        }
    }

    /// The cursor inverts the SAA5050's colours before they are mixed, so an
    /// inverted pixel is the same mix of the inverted foreground and
    /// background.
    #[test]
    fn the_cursor_inverts_a_mixed_mode7_pixel_to_the_inverted_mix() {
        for fg in 0..8 {
            for bg in 0..8 {
                for coverage in 0..=3 {
                    assert_eq!(
                        invert_rgb(teletext_pixel(fg, bg, coverage)),
                        teletext_pixel(fg ^ 7, bg ^ 7, coverage),
                        "fg {fg}, bg {bg}, {coverage} thirds"
                    );
                }
            }
        }
    }

    /// Double height (code 141, `$8D`) is set-after: the code's own cell is a
    /// space, the characters after it draw their top half across the row,
    /// each dot row four lines tall, and the row below draws the bottom
    /// halves of its own double-height characters. The MOS repeats the line
    /// to get that, as the Advanced Teletext System User Guide describes.
    #[test]
    fn double_height_draws_top_and_bottom_halves_on_two_rows() {
        // Dot rows 1 and 6 lit: the top half shows row 1, the bottom row 6.
        let a = [0, 0x3F, 0, 0, 0, 0, 0x3F, 0, 0, 0];
        let sys = teletext_screen(a, &[b"\x8DA", b"\x8DA", b"A"]);
        assert_eq!(
            cell_rows_in(&sys, 0, 0, WHITE),
            Vec::<usize>::new(),
            "the code's cell"
        );
        assert_eq!(
            cell_rows_in(&sys, 1, 0, WHITE),
            [4, 5, 6, 7],
            "row 1, doubled"
        );
        assert_eq!(
            cell_rows_in(&sys, 1, 1, WHITE),
            [4, 5, 6, 7],
            "row 6, doubled"
        );
        assert_eq!(
            cell_rows_in(&sys, 0, 2, WHITE),
            [2, 3, 12, 13],
            "the next row is normal"
        );
    }

    /// The row after a double-height row shows only double-height characters.
    /// If it has no double-height code at all, it is blank, whatever it
    /// holds; and normal-height characters on it are blank too.
    #[test]
    fn the_row_under_double_height_shows_only_double_height_characters() {
        let a = [0x3F; 10];
        let sys = teletext_screen(a, &[b"\x8DA", b"A A\x8DA"]);
        assert_eq!(
            cell_rows_in(&sys, 0, 1, WHITE),
            Vec::<usize>::new(),
            "normal height"
        );
        assert_eq!(
            cell_rows_in(&sys, 2, 1, WHITE),
            Vec::<usize>::new(),
            "normal height"
        );
        assert_eq!(cell_rows_in(&sys, 4, 1, WHITE).len(), 20, "double height");

        let sys = teletext_screen(a, &[b"\x8DA", b"A"]);
        assert_eq!(
            cell_rows_in(&sys, 0, 1, WHITE),
            Vec::<usize>::new(),
            "no code: blank"
        );
    }

    /// Flash (136, `$88`) hides the text after it for 16 fields in every 64;
    /// steady (137, `$89`) stops it at its own cell. Watched on the even
    /// field's rows, which change once a frame pair.
    #[test]
    fn flashing_teletext_hides_for_16_fields_in_64() {
        let mut sys = teletext_screen([0x3F; 10], &[b"\x88A\x89A"]);
        let x = TELETEXT_CELL_WIDTH;
        let steady = 3 * TELETEXT_CELL_WIDTH;
        let mut shown = Vec::new();
        for _ in 0..150 {
            sys.run_frame();
            shown.push(pixel(&sys, x, 0) == WHITE);
            assert_eq!(pixel(&sys, steady, 0), WHITE, "the steady A never flashes");
        }
        // Runs of frames shown or hidden, without the partial first and last.
        let mut runs = Vec::new();
        let mut run = 1;
        for pair in shown.windows(2) {
            if pair[0] == pair[1] {
                run += 1;
            } else {
                runs.push((pair[0], run));
                run = 1;
            }
        }
        let whole = runs.get(1..).unwrap_or_default();
        assert!(whole.len() >= 2, "it flashes: {shown:?}");
        for &(lit, frames) in whole {
            assert_eq!(frames, if lit { 48 } else { 16 }, "{runs:?}");
        }
    }

    /// Conceal (152, `$98`) hides what follows until a colour code, which
    /// reveals the text after it.
    #[test]
    fn conceal_hides_text_until_a_colour_code() {
        let sys = teletext_screen([0x3F; 10], &[b"\x98A\x82A"]);
        assert_eq!(
            cell_rows_in(&sys, 1, 0, WHITE),
            Vec::<usize>::new(),
            "concealed"
        );
        assert_eq!(
            cell_rows_in(&sys, 3, 0, 0xFF00_FF00).len(),
            20,
            "green, shown"
        );
    }

    /// Hold graphics (158, `$9E`) is set-at, so its own cell repeats the last
    /// mosaic; and a colour change under hold is set-after, so its cell
    /// repeats the mosaic in the old colour. Before, the hold cell was a
    /// space and the colour cell took the new colour.
    #[test]
    fn hold_graphics_repeats_the_mosaic_in_its_own_colour() {
        let sys = teletext_screen([0; 10], &[b"\x97\x7F\x9E\x91\x7F"]);
        assert_eq!(cell_rows_in(&sys, 2, 0, WHITE).len(), 20, "the hold cell");
        assert_eq!(
            cell_rows_in(&sys, 3, 0, WHITE).len(),
            20,
            "the colour cell, still white"
        );
        assert_eq!(cell_rows_in(&sys, 4, 0, 0xFFFF_0000).len(), 20, "then red");
    }

    /// Physical colour `&08` flashes black and white: the palette stores it
    /// EOR 7 as `&0F`, and Video ULA control bit 0 picks which of the pair is
    /// showing (Advanced User Guide §19.1.1, §19.2.3). Before, the flash bit
    /// was ignored and the colour was black whatever bit 0 said (#384).
    #[test]
    fn control_bit_0_selects_which_flashing_colour_shows() {
        for (control, expected, phase) in [(0x9C, BLANK, "first"), (0x9D, WHITE, "second")] {
            let mut sys = BbcMicro::new(trap_rom());
            program_mode(&mut sys, MODE0_CRTC, control);
            for logical in 0..16u8 {
                let physical = if logical >= 8 { 0x08 } else { 0 };
                sys.mem_write(0xFE21, (logical << 4) | (physical ^ 7));
            }
            sys.ram[0x3000..0x3008].fill(0xFF);
            for _ in 0..3 {
                sys.run_frame();
            }
            assert_eq!(
                pixel(&sys, 0, 0),
                expected,
                "flashing black-white shows its {phase} colour"
            );
        }
    }

    /// A steady cursor on raster 7 of the character at `column` of row 0.
    fn mode0_with_cursor(r8: u8, control: u8, column: u16) -> BbcMicro {
        let mut sys = BbcMicro::new(trap_rom());
        let mut crtc = MODE0_CRTC;
        crtc[8] = r8;
        crtc[10] = 0x07; // steady, starting on raster 7
        crtc[11] = 7;
        program_mode(&mut sys, crtc, control);
        mode0_palette(&mut sys);
        let cursor = 0x0600 + column;
        sys.mem_write(0xFE00, 14);
        sys.mem_write(0xFE01, (cursor >> 8) as u8);
        sys.mem_write(0xFE00, 15);
        sys.mem_write(0xFE01, cursor as u8);
        for _ in 0..3 {
            sys.run_frame();
        }
        sys
    }

    /// The pixels of framebuffer row `y` that are white, as a run `start..end`.
    fn white_run(sys: &BbcMicro, y: usize) -> Option<(usize, usize)> {
        let lit: Vec<usize> = (0..FB_WIDTH as usize)
            .filter(|&x| pixel(sys, x, y) == WHITE)
            .collect();
        let (&first, &last) = (lit.first()?, lit.last()?);
        assert_eq!(last + 1 - first, lit.len(), "the lit pixels are one run");
        Some((first, last + 1))
    }

    /// The 6845's CURSOR is one character wide; control bits 7, 6 and 5
    /// widen it to the one, two or four bytes a mode's character takes
    /// (Advanced User Guide §19.1.5-§19.1.7). It inverts the picture, so over
    /// black it is white, and only on the rasters R10-R11 select. Before,
    /// the BBC drew no cursor at all (#384).
    #[test]
    fn the_cursor_is_as_wide_as_control_bits_5_to_7_make_it() {
        for (control, width, mode) in [
            (0x9C, 8, "bit 7 alone: MODE 0's one byte"),
            (0xDC, 16, "bits 7 and 6: MODE 1's two bytes"),
            (0xFC, 32, "bits 7, 6 and 5: MODE 2's four bytes"),
        ] {
            let sys = mode0_with_cursor(0x00, control, 2);
            // Raster 7 of the first row is framebuffer rows 14 and 15.
            assert_eq!(white_run(&sys, 14), Some((16, 16 + width)), "{mode}");
            assert_eq!(white_run(&sys, 15), Some((16, 16 + width)), "{mode}");
            assert_eq!(white_run(&sys, 13), None, "{mode}: raster 6 has none");
        }
        let sys = mode0_with_cursor(0x00, 0x1C, 2);
        assert_eq!(white_run(&sys, 14), None, "bits 5-7 clear hide it");
    }

    /// R8 bits 6-7 delay the cursor by none, one or two characters, or turn
    /// it off (Advanced User Guide §18.6.3).
    #[test]
    fn r8_skews_the_cursor_or_turns_it_off() {
        for (skew, start) in [(0u8, 16usize), (1, 24), (2, 32)] {
            let sys = mode0_with_cursor(skew << 6, 0x9C, 2);
            assert_eq!(white_run(&sys, 14), Some((start, start + 8)), "skew {skew}");
        }
        let sys = mode0_with_cursor(0xC0, 0x9C, 2);
        assert_eq!(white_run(&sys, 14), None, "skew 3 disables the cursor");
    }

    /// R10 = `&67`, the MOS's bitmap-mode cursor, blinks with a 32-field
    /// period: sixteen frames on, sixteen off (Advanced User Guide §18.8).
    #[test]
    fn the_mos_cursor_blinks_every_sixteen_fields() {
        let mut sys = mode0_with_cursor(0x00, 0x9C, 2);
        sys.mem_write(0xFE00, 10);
        sys.mem_write(0xFE01, 0x67);
        let shown: Vec<bool> = (0..96)
            .map(|_| {
                sys.run_frame();
                white_run(&sys, 14).is_some()
            })
            .collect();
        let changes: Vec<usize> = (1..shown.len())
            .filter(|&i| shown[i] != shown[i - 1])
            .collect();
        assert!(changes.len() >= 5, "blinks: {shown:?}");
        // The first change can come early: the phase was running while the
        // cursor was steady.
        for pair in changes[1..].windows(2) {
            assert_eq!(pair[1] - pair[0], 16, "a 16-field half-period: {shown:?}");
        }
    }

    /// MODE 7's cursor: the MOS sets a two-character skew and enables only
    /// the second segment (`&4B`), which together land it on the character
    /// the 6845 matched, three clocks on, once the SAA5050 has drawn it. With
    /// R10/R11 = 18-19 it underlines the cell's tenth line: raster 18 in the
    /// even field and 19 in the odd, framebuffer rows 18 and 19.
    #[test]
    fn the_mode7_cursor_underlines_the_matched_cell() {
        let mut sys = BbcMicro::new(trap_rom());
        sys.set_teletext_font(font_with_a_lit());
        let mut crtc = MODE7_CRTC;
        crtc[10] = 0x12; // steady, from raster 18
        program_mode(&mut sys, crtc, 0x4B);
        sys.ram[0x7C00..0x8000].fill(b' ');
        sys.mem_write(0xFE00, 14);
        sys.mem_write(0xFE01, 0x28);
        sys.mem_write(0xFE00, 15);
        sys.mem_write(0xFE01, 0x05); // the sixth cell of row 0
        run_both_fields(&mut sys);
        let x = 5 * TELETEXT_CELL_WIDTH;
        assert_eq!(white_run(&sys, 18), Some((x, x + TELETEXT_CELL_WIDTH)));
        assert_eq!(white_run(&sys, 19), Some((x, x + TELETEXT_CELL_WIDTH)));
        assert_eq!(white_run(&sys, 17), None, "only the bottom line");
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
