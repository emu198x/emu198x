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

mod rtc;
mod saa5050;

use common_acorn_cassette::{CassetteEvent, CassetteReceiver, TapePulse};
pub use emu198x_mos_6502::M6502;
use emu198x_mos_via_6522::Via6522;
pub use emu198x_motorola_6845::Crtc6845;
use emu198x_motorola_6845::Crtc6845Variant;
use emu198x_ti_sn76489::{NoiseLfsr, Sn76489};
use rtc::Rtc;
use saa5050::Saa5050;
use serde::{Deserialize, Serialize};
use serde_big_array::BigArray;

pub use emu198x_western_digital_wd1770::Disk;
use emu198x_western_digital_wd1770::Wd1770;

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
/// Which BBC Micro this is.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum BbcModel {
    /// The Model B: 32 KB.
    #[default]
    ModelB,
    /// The Model B+ (1985): 64 KB. To the Model B's 32 KB it adds 12 KB of
    /// paged RAM at `&8000-&AFFF` and a 20 KB shadow screen behind
    /// `&3000-&7FFF`.
    ModelBPlus,
    /// The Master 128 (1986): a 65C12, 128 KB and the MOS 3.20 ROM set. To
    /// the 32 KB main RAM it adds ANDY (4 KB at `&8000-&8FFF`), HAZEL (8 KB
    /// at `&C000-&DFFF`) and LYNNE (the 20 KB shadow screen), four banks of
    /// sideways RAM (4-7), a CMOS clock, and a WD1770 at `&FE28`.
    Master128,
}

/// Bytes in one track of a DFS disc: ten 256-byte sectors.
const DFS_TRACK_BYTES: usize = 10 * 256;

#[derive(Serialize, Deserialize)]
pub struct BbcState {
    cpu: M6502,
    crtc: Crtc6845,
    video_ula: VideoUla,
    system_via: Via6522,
    user_via: Via6522,
    psg: Sn76489,
    #[serde(with = "BigArray")]
    ram: [u8; 32768],
    /// Which model this is. Defaults so a Model B snapshot taken before the
    /// B+ existed still reads as one.
    model: BbcModel,
    /// The extra 32 KB of a B+ or Master, empty on a Model B. The 20 KB
    /// shadow screen sits at `[&3000, &8000)`, so a shadow address
    /// `&3000-&7FFF` indexes it directly, and so does a video address when the
    /// display shows the shadow screen, as MAME's `setvideoshadow` offsets its
    /// fetches by `&8000`. Below it, the B+ keeps its 12 KB of paged RAM in
    /// `[0, &3000)`; the Master keeps ANDY in `[0, &1000)` and HAZEL in
    /// `[&1000, &3000)`, b-em's layout.
    extra_ram: Vec<u8>,
    /// ROMSEL bit 7: on a B+, paged RAM at `&8000-&AFFF`; on a Master, ANDY
    /// at `&8000-&8FFF`; either in place of the selected ROM.
    paged_ram: bool,
    /// The display shows the shadow screen: ACCCON bit 7 on a B+, bit 0 (D)
    /// on a Master.
    shadow_screen: bool,
    /// The Master's ACCCON at `&FE34`, all eight bits: IRR, TST, IFJ, ITU, Y
    /// (HAZEL), X (the CPU sees the shadow screen), E (the VDU drivers do)
    /// and D (the display does).
    acccon: u8,
    /// Which sideways banks hold RAM, one bit per bank: 4-7 on a Master.
    sideways_ram: u16,
    /// The Master's CMOS clock on the slow data bus.
    rtc: Rtc,
    /// Whether the instruction now executing reaches the shadow screen at
    /// `&3000-&7FFF`: decided at each opcode fetch, from where the opcode
    /// came from.
    shadow_access: bool,
    /// The B+'s WD1770, at `&FE84-&FE87` and repeating to `&FE9F`. Present on
    /// every model so the shape is fixed; wired only on a B+.
    fdc: Wd1770,
    /// The B+'s drive-control latch at `&FE80`: bits 0 and 1 select drive 0
    /// or 1, bit 2 the side, bit 3 single density, bit 5 releases the
    /// controller's reset (MAME `bbcbp_state::drive_control_w`).
    drive_control: u8,
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
    /// `run_frame`'s fixed tick budget does.
    back_buffer: Vec<u32>,
    /// Scan lines since the 6845's frame began: the beam's line, which
    /// `beam_rows` places in `back_buffer`. The window's origin is the 6845's own — column 0, the
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
    #[serde(skip)]
    native_frame_ready: bool,
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

/// Fixed wiring supplied by a concrete BBC board.
pub trait Board {
    const MODEL: BbcModel;
    const EXTRA_RAM_LEN: usize;
    const SIDEWAYS_RAM: u16;
    fn new_cpu() -> M6502;
}
/// Shared wiring statically bound to one board.
pub struct BbcCore<B: Board> {
    state: BbcState,
    board: std::marker::PhantomData<B>,
}
impl BbcState {
    #[must_use]
    pub fn model(&self) -> BbcModel {
        self.model
    }
}
impl<B: Board> BbcCore<B> {
    /// Recover a live board without resetting CPU, memory or peripherals.
    pub fn from_state(state: BbcState) -> Result<Self, &'static str> {
        if state.model != B::MODEL
            || state.extra_ram.len() != B::EXTRA_RAM_LEN
            || state.sideways_ram != B::SIDEWAYS_RAM
        {
            return Err("snapshot hardware does not match this BBC board");
        }
        let mut machine = Self {
            state,
            board: std::marker::PhantomData,
        };
        machine.finish_restore();
        Ok(machine)
    }
}
impl<B: Board> Serialize for BbcCore<B> {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        self.state.serialize(s)
    }
}
impl<'de, B: Board> Deserialize<'de> for BbcCore<B> {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Self::from_state(BbcState::deserialize(d)?).map_err(serde::de::Error::custom)
    }
}

impl<B: Board> BbcCore<B> {
    /// Create a new BBC Micro with the 16 KB MOS ROM. Sideways ROMs
    /// start empty; use [`Self::insert_rom`] to install BASIC, DFS,
    /// etc. into specific bank slots.
    #[must_use]
    pub fn new(mos_rom: Vec<u8>) -> Self {
        let mut cpu = B::new_cpu();
        cpu.reset();
        let mut crtc = Crtc6845::new();
        crtc.set_variant(Crtc6845Variant::Hd6845s);
        let mut machine = Self {
            state: BbcState {
                cpu,
                crtc,
                video_ula: VideoUla::new(),
                system_via: Via6522::new(),
                user_via: Via6522::new(),
                psg: Sn76489::new(SN76489_CLOCK_HZ, NoiseLfsr::Tms15),
                ram: [0; 32768],
                model: B::MODEL,
                extra_ram: vec![0; B::EXTRA_RAM_LEN],
                paged_ram: false,
                shadow_screen: false,
                acccon: 0,
                sideways_ram: B::SIDEWAYS_RAM,
                rtc: Rtc::new(),
                shadow_access: false,
                fdc: Wd1770::new(),
                drive_control: 0,
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
                native_frame_ready: false,
                frame_base: 0,
                scanline: 0,
                fire: [false; 2],
                adc: Upd7002::new(),
                acia: Mc6850::new(),
                serial_ula: 0,
                cassette: CassetteReceiver::new(),
                deck_running: true,
            },
            board: std::marker::PhantomData,
        };
        for bank in 0..16 {
            if B::SIDEWAYS_RAM & (1 << bank) != 0 {
                machine.insert_rom(bank, vec![0; 0x4000]);
            }
        }
        machine
    }

    /// Finish a restore from a snapshot: put back what a snapshot does not
    /// carry. The 6502's variant is the part fitted, so it is not serialised
    /// (see `emu198x_mos_6502::M6502::set_variant`); a Master's 65C12 is set
    /// again here.
    pub fn finish_restore(&mut self) {
        let variant = if B::MODEL == BbcModel::Master128 {
            emu198x_mos_6502::Variant::Cmos
        } else {
            emu198x_mos_6502::Variant::Nmos
        };
        self.state.cpu.set_variant(variant);
    }

    /// Which model this is.
    #[must_use]
    pub const fn model(&self) -> BbcModel {
        B::MODEL
    }

    /// Whether this model has the WD1770 disc controller.
    const fn has_wd1770(&self) -> bool {
        matches!(B::MODEL, BbcModel::ModelBPlus | BbcModel::Master128)
    }

    /// Put a disc in drive 0 or 1 of a B+ or Master. Does nothing on a Model
    /// B, whose 8271 controller is not modelled, or for any other drive.
    pub fn insert_disk(&mut self, drive: usize, disk: Disk) {
        if self.has_wd1770() && drive < 2 {
            self.state.fdc.insert_disk(drive, disk);
        }
    }

    /// Take the disc out of drive 0 or 1, returning it with any writes.
    pub fn eject_disk(&mut self, drive: usize) -> Option<Disk> {
        if drive < 2 {
            self.state.fdc.eject_disk(drive)
        } else {
            None
        }
    }

    /// The disc in drive 0 or 1.
    #[must_use]
    pub fn disk(&self, drive: usize) -> Option<&Disk> {
        if drive < 2 {
            self.state.fdc.disk(drive)
        } else {
            None
        }
    }

    /// Build a disc from an Acorn DFS image: `.ssd` with one side, or `.dsd`
    /// with two, interleaved track by track. Each track holds ten 256-byte
    /// sectors numbered 0-9. The track count follows the image's size: 40,
    /// or 80 for a larger image. A short image, which is common because
    /// tools trim the unused tracks, is padded so every sector can be written.
    #[must_use]
    pub fn dfs_disk(image: &[u8], sides: usize, writable: bool) -> Disk {
        dfs_disk(image, sides, writable)
    }

    /// How many sides a DFS image holds, from its bytes alone, since an
    /// image arrives without its `.ssd` or `.dsd` name.
    ///
    /// A `.dsd` interleaves the sides track by track, so its second track is
    /// side 1's track 0, which opens with a catalogue of its own. Its sector
    /// count, in the second catalogue sector, matches side 0's. In a `.ssd`
    /// the same bytes are track 1's file data, which almost never matches.
    /// An image too big for one side of 80 tracks has two, whatever it holds.
    #[must_use]
    pub fn dfs_sides(image: &[u8]) -> usize {
        dfs_sides(image)
    }

    /// Write the drive-control latch: select the drive and side, set the
    /// density, and pulse the controller's reset when the reset line is
    /// released. The bits differ between the two interfaces (b-em
    /// `wd1770_wctl_acorn` and `wd1770_wctl_master`):
    ///
    /// | | B+ (`&FE80`) | Master (`&FE24`) |
    /// |---|---|---|
    /// | drive 0 / 1 | bits 0 / 1 | bits 0 / 1 |
    /// | side | bit 2 | bit 4 |
    /// | single density | bit 3 | bit 5 |
    /// | reset released | bit 5 | bit 2 |
    fn write_drive_control(&mut self, value: u8) {
        let (side, single, reset) = if B::MODEL == BbcModel::Master128 {
            (0x10, 0x20, 0x04)
        } else {
            (0x04, 0x08, 0x20)
        };
        if value & 0x01 != 0 {
            self.state.fdc.set_drive(0);
        } else if value & 0x02 != 0 {
            self.state.fdc.set_drive(1);
        }
        self.state.fdc.set_side(u8::from(value & side != 0));
        // Single density moves a byte every 64 µs: 128 ticks at the 2 MHz
        // the chip is ticked at here. Double density is twice as fast. The
        // DFS takes each byte on an NMI, which is edge-triggered, so DRQ has
        // to fall between bytes.
        self.state
            .fdc
            .set_byte_cycles(if value & single != 0 { 128 } else { 64 });
        let releasing_reset = self.state.drive_control & reset == 0 && value & reset != 0;
        self.state.drive_control = value;
        if releasing_reset {
            self.state.fdc.reset();
        }
    }

    /// Whether `addr` reaches the B+'s paged RAM, or the Master's ANDY,
    /// rather than a sideways bank.
    fn in_paged_ram(&self, addr: u16) -> bool {
        let end = if B::MODEL == BbcModel::Master128 {
            0x8FFF
        } else {
            0xAFFF
        };
        self.state.paged_ram && (0x8000..=end).contains(&addr)
    }

    /// Whether `addr` reaches the Master's HAZEL rather than the MOS: ACCCON
    /// bit 3 (Y) pages it in at `&C000-&DFFF`.
    fn in_hazel(&self, addr: u16) -> bool {
        B::MODEL == BbcModel::Master128
            && self.state.acccon & 0x08 != 0
            && (0xC000..=0xDFFF).contains(&addr)
    }

    /// The Master's ACCCON test bit (TST): reads of `&FC00-&FEFF` come from
    /// the MOS ROM instead of the I/O.
    fn io_reads_rom(&self, addr: u16) -> bool {
        B::MODEL == BbcModel::Master128 && self.state.acccon & 0x40 != 0 && addr >= 0xFC00
    }

    /// Whether `addr` reaches the B+'s shadow screen rather than main RAM.
    fn in_shadow_screen(&self, addr: u16) -> bool {
        self.state.shadow_access && (0x3000..=0x7FFF).contains(&addr)
    }

    /// At an opcode fetch, decide whether the instruction reaches the shadow
    /// screen. On a B+ with ACCCON bit 7 set, code running in the MOS's VDU
    /// drivers (`&C000-&DFFF`) does, and so does code in paged RAM between
    /// `&A000` and `&AFFF`; everything else sees main RAM. MAME
    /// `bbcbp_state::fetch_r`.
    ///
    /// On a Master, ACCCON's X bit (2) gives every instruction the shadow
    /// screen, except that code in the VDU drivers at `&C000-&DFFF` follows
    /// the E bit (1) instead, unless HAZEL (Y, bit 3) is paged in there. That
    /// is b-em's `write_acccon_master`, whose page tables come to the same.
    fn latch_shadow_access(&mut self, opcode_addr: u16) {
        self.state.shadow_access = match B::MODEL {
            BbcModel::ModelB => false,
            BbcModel::ModelBPlus => {
                self.state.shadow_screen
                    && match opcode_addr {
                        0xC000..=0xDFFF => true,
                        0xA000..=0xAFFF => self.state.paged_ram,
                        _ => false,
                    }
            }
            BbcModel::Master128 => {
                let vdu = (0xC000..=0xDFFF).contains(&opcode_addr) && self.state.acccon & 0x08 == 0;
                if vdu {
                    self.state.acccon & 0x02 != 0
                } else {
                    self.state.acccon & 0x04 != 0
                }
            }
        };
    }

    /// Loads a cassette tape from a decoded UEF pulse stream, rewound to the
    /// start. The tape only advances while the motor relay is energised.
    pub fn insert_tape(&mut self, pulses: Vec<TapePulse>) {
        self.state.cassette.load(pulses);
    }

    /// Ejects the cassette tape.
    pub fn eject_tape(&mut self) {
        self.state.cassette.eject();
    }

    /// Returns `true` when a cassette tape is loaded.
    #[must_use]
    pub fn tape_loaded(&self) -> bool {
        self.state.cassette.is_loaded()
    }

    /// Whether the host has the deck running. See [`Self::set_deck_running`].
    #[must_use]
    pub const fn deck_running(&self) -> bool {
        self.state.deck_running
    }

    /// Starts or stops the deck, independently of the guest's motor relay.
    pub const fn set_deck_running(&mut self, running: bool) {
        self.state.deck_running = running;
    }

    /// Returns `true` when the cassette motor relay (`$FE10` bit 7) is on.
    #[must_use]
    pub fn cassette_motor_on(&self) -> bool {
        self.state.serial_ula & MOTOR_BIT != 0
    }

    /// Set a joystick fire button (`port` 1 or 2, `true` = pressed). The switch
    /// is read on System VIA PB4 (joy 1) / PB5 (joy 2), active low; the value is
    /// merged into the VIA input latch on the next tick. Out-of-range ports
    /// clamp to the valid pair.
    pub fn set_fire_button(&mut self, port: u8, pressed: bool) {
        self.state.fire[usize::from(port.clamp(1, 2) - 1)] = pressed;
    }

    /// Set an ADC channel's 12-bit pot value (`0..=0x0FFF`, clamped). The four
    /// channels are the analogue joystick axes: channel 0/1 = joystick 1 X/Y,
    /// channel 2/3 = joystick 2 X/Y. `0x0800` is centre. Out-of-range channels
    /// are ignored. The OS reads these through the μPD7002 at `$FEC0-$FEC3`.
    pub fn set_adc_channel(&mut self, channel: u8, value: u16) {
        if let Some(slot) = self.state.adc.channels.get_mut(channel as usize) {
            *slot = value.min(0x0FFF);
        }
    }

    /// The 12-bit pot value currently latched on an ADC channel (0-3), or 0 for
    /// an out-of-range channel. For inspection and host-side input wiring.
    #[must_use]
    pub fn adc_channel(&self, channel: u8) -> u16 {
        self.state
            .adc
            .channels
            .get(channel as usize)
            .copied()
            .unwrap_or(0)
    }

    /// Supply the SAA5050 teletext character ROM (960 bytes: 96 glyphs of
    /// 10 rows). Required for MODE 7 to render anything but a blank screen.
    pub fn set_teletext_font(&mut self, font: Vec<u8>) {
        self.state.teletext_font = font;
    }

    /// Whether the selected teletext output lacks a complete character ROM.
    ///
    /// Bitmap modes do not use this ROM. Derive this from live machine state
    /// so a restored snapshot reports its own video mode and font.
    #[must_use]
    pub fn teletext_font_missing(&self) -> bool {
        self.state.video_ula.teletext() && self.state.teletext_font.len() < 96 * 10
    }

    /// Install a sideways ROM into the given bank slot (0-15).
    pub fn insert_rom(&mut self, bank: usize, rom: Vec<u8>) {
        while self.state.sideways_roms.len() <= bank {
            self.state.sideways_roms.push(Vec::new());
        }
        self.state.sideways_roms[bank] = rom;
    }

    /// Number of sideways banks holding a nonempty ROM image.
    #[must_use]
    pub fn sideways_rom_count(&self) -> usize {
        self.state
            .sideways_roms
            .iter()
            .filter(|rom| !rom.is_empty())
            .count()
    }

    /// Run one PAL frame.
    /// Advance to the CRTC field event; the safety budget is not a frame.
    pub fn run_native_frame(&mut self) -> (u64, bool) {
        let start = self.state.master_ticks;
        self.state.native_frame_ready = false;
        while (!self.state.native_frame_ready || self.state.video_frame_ended)
            && self.state.master_ticks - start < 2 * CYCLES_PER_FRAME
        {
            self.tick_cpu_cycle();
        }
        (
            self.state.master_ticks - start,
            self.state.native_frame_ready && !self.state.video_frame_ended,
        )
    }

    pub fn run_frame(&mut self) -> u64 {
        let start = self.state.master_ticks;
        while self.state.master_ticks - start < CYCLES_PER_FRAME {
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
        self.state.system_via.set_ca1_level(!self.state.crtc.vsync);
        self.state.scanline += 1;
        if self.state.scanline >= SCANLINES_PER_FRAME {
            self.state.scanline = 0;
            self.state.frame_base += CYCLES_PER_FRAME;
            self.state.frame_count += 1;
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
        self.state.cpu.tick();
        let cost = Self::access_master_ticks(self.state.cpu.addr, self.state.master_ticks);
        // The chips run off the constant 2 MHz clock, so they advance one
        // tick per master tick — one, two or three per 6502 cycle depending
        // on whether this access hit a 1 MHz peripheral, and where in the
        // 1 MHz cycle it started.
        for tick in 0..cost {
            // A stretched cycle reaches its device in the 1 MHz cycle it
            // ends with, so the access lands in the final tick, ahead of the
            // edge that closes it. A 2 MHz cycle has only the one tick.
            if tick + 1 == cost {
                if self.state.cpu.rw {
                    if self.state.cpu.sync {
                        self.latch_shadow_access(self.state.cpu.addr);
                    }
                    self.state.cpu.data_in = self.mem_read(self.state.cpu.addr);
                } else {
                    self.mem_write(self.state.cpu.addr, self.state.cpu.data);
                }
            }
            let one_mhz_edge = Self::ends_one_mhz_cycle(self.state.master_ticks + tick);
            // The 6845 runs at 2 MHz in MODE 0-3 and 1 MHz in MODE 4-7:
            // Video ULA control bit 4 picks which (Advanced User Guide
            // §19.1.4). Both give a 64 µs line and a 50 Hz frame.
            if self.state.video_ula.fast_clock() || one_mhz_edge {
                self.clock_video();
            }
            // Both VIAs sit on the 1 MHz clock (Advanced User Guide §28.5),
            // so their timers count microseconds: one VIA cycle per two
            // master ticks.
            if one_mhz_edge {
                self.state.system_via.tick();
                self.state.user_via.tick();
            }
            // The SN76489 runs off a 4 MHz clock (Advanced User Guide
            // §23.3.1): two of its cycles per 2 MHz master tick.
            self.state.psg.tick();
            self.state.psg.tick();
            // μPD7002 end-of-conversion is wired to System VIA CB1. Drive
            // the line low on the completion edge (the OS's CB1 is set for
            // a negative edge), latching the analogue interrupt.
            if self.state.adc.tick() {
                self.state.system_via.set_cb1_level(false);
            }
        }
        self.tick_cassette(cost);
        self.state.cpu.irq =
            self.state.system_via.irq || self.state.user_via.irq || self.state.acia.irq();
        if B::MODEL == BbcModel::Master128 {
            self.state.rtc.tick(cost);
        }
        if self.has_wd1770() {
            for _ in 0..cost {
                self.state.fdc.tick();
            }
            // The 1770's INTRQ and DRQ both drive NMI: the DFS moves every
            // byte in its NMI handler (MAME `bbc_state::update_nmi`).
            self.state.cpu.nmi = self.state.fdc.intrq() || self.state.fdc.drq();
        }
        self.state.master_ticks += cost;
        self.state.cpu_cycles += 1;
        // A cycle can cost more than one tick, so it may carry the machine
        // over more than one line boundary.
        while self.state.master_ticks
            >= self.state.frame_base + u64::from(self.state.scanline + 1) * CYCLES_PER_LINE
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
        if self.state.serial_ula & MOTOR_BIT == 0 || !self.state.deck_running {
            return;
        }
        let ns = cost * NS_PER_MASTER_TICK;
        // Disjoint borrows: the receiver drives the ACIA register it feeds.
        let BbcState { cassette, acia, .. } = &mut self.state;
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
        let code = self.state.system_via.ora();
        let col = (code & 0x0F) as usize;
        let row = ((code >> 4) & 0x07) as usize;
        let pressed = self
            .state
            .keyboard
            .get(col)
            .and_then(|c| c.get(row))
            .copied()
            .unwrap_or(false);
        let bit = if pressed { 0x80 } else { 0x00 };
        if B::MODEL == BbcModel::Master128 {
            // The CMOS clock shares the slow data bus, driving it during a
            // read and otherwise letting it float high. The keyboard pulls
            // PA7 low only while it is enabled (IC32 bit 3 low) and the
            // selected key is up, so with the keyboard disabled bit 7 is the
            // clock's (b-em `sysvia_update_sdb`).
            let bus = self.state.rtc.bus(self.state.latch.bits[1]);
            let keyboard_enabled = !self.state.latch.bits[3];
            self.state.system_via.pa_in = if keyboard_enabled && !pressed {
                bus & 0x7F
            } else {
                bus
            };
            return;
        }
        self.state.system_via.pa_in = (self.state.system_via.pa_in & 0x7F) | bit;
    }

    /// Drive the System VIA CA2 "key pressed" interrupt line that the MOS uses
    /// to detect keystrokes. Faithful to jsbeeb's `SysVia.updateKeys`: when the
    /// keyboard is auto-scanning (IC32 addressable-latch bit 3 set) CA2 goes
    /// high if any key in rows 1-7 of any column is down; otherwise it reflects
    /// the column the CPU is currently driving on PA0-3. Row 0 (SHIFT / CTRL)
    /// never raises the interrupt, exactly as the hardware's keyboard scanner.
    fn update_keyboard_ca2(&mut self) {
        let pressed_in_column = |col: &[bool; 8]| col[1..8].iter().any(|&down| down);
        let any_key = if self.state.latch.bits[3] {
            self.state.keyboard.iter().any(pressed_in_column)
        } else {
            let col = (self.state.system_via.ora() & 0x0F) as usize;
            self.state.keyboard.get(col).is_some_and(pressed_in_column)
        };
        self.state.system_via.set_ca2_level(any_key);
    }

    /// Merge the joystick fire buttons into System VIA port B: PB4 (joy 1) and
    /// PB5 (joy 2), active low (pressed pulls the line low). Read-modify-write
    /// leaves the addressable-latch outputs (PB0-3) and the speech lines
    /// (PB6-7) untouched.
    fn update_joystick_fire(&mut self) {
        let mut bits = 0x30u8; // both fire lines idle high
        if self.state.fire[0] {
            bits &= !0x10;
        }
        if self.state.fire[1] {
            bits &= !0x20;
        }
        self.state.system_via.pb_in = (self.state.system_via.pb_in & !0x30) | bits;
    }

    /// After a System VIA write, let the Master's clock see the slow data bus
    /// and its control lines. A port B write also drives PB6 and PB7.
    fn update_rtc(&mut self, port_b_written: bool) {
        let bus = self.state.system_via.port_a_drive_state();
        if port_b_written {
            self.state
                .rtc
                .write_port_b(self.state.system_via.port_b_drive_state(), bus);
        }
        self.state
            .rtc
            .update(self.state.latch.bits[1], self.state.latch.bits[2], bus);
    }

    /// The Master's own I/O map, where it differs from the Model B's: the
    /// serial ULA at `&FE10` and the ADC at `&FE18`, the Video ULA in
    /// `&FE20-&FE23` alone, the drive latch at `&FE24` and the 1770 at
    /// `&FE28`, and ROMSEL and ACCCON readable (b-em `do_readmem`).
    fn master_io_read(&mut self, addr: u16) -> Option<u8> {
        Some(match addr {
            0xFE18..=0xFE1F => self.state.adc.read((addr & 0x03) as u8),
            0xFE24..=0xFE27 => 0xFF,
            0xFE28..=0xFE2F => self.state.fdc.read((addr & 0x03) as u8),
            0xFE30..=0xFE33 => self.state.rom_bank | if self.state.paged_ram { 0x80 } else { 0 },
            0xFE34..=0xFE37 => self.state.acccon,
            0xFEC0..=0xFEDF => 0xFF,
            _ => return None,
        })
    }

    /// Write the Master's own I/O, returning whether `addr` was one of them.
    fn master_io_write(&mut self, addr: u16, value: u8) -> bool {
        match addr {
            0xFE10..=0xFE17 => self.state.serial_ula = value,
            0xFE18..=0xFE1F => {
                if addr & 0x03 == 0 {
                    self.state.adc.write_control(value);
                    self.state.system_via.set_cb1_level(true);
                }
            }
            0xFE20..=0xFE23 if addr & 1 == 0 => self.state.video_ula.write_control(value),
            0xFE20..=0xFE23 => self.state.video_ula.write_palette(value),
            0xFE24..=0xFE27 => self.write_drive_control(value),
            0xFE28..=0xFE2F => self.state.fdc.write((addr & 0x03) as u8, value),
            0xFE30..=0xFE33 => {
                self.state.rom_bank = value & 0x0F;
                self.state.paged_ram = value & 0x80 != 0;
            }
            0xFE34..=0xFE37 => {
                self.state.acccon = value;
                self.state.shadow_screen = value & 0x01 != 0;
            }
            0xFE38..=0xFE3F | 0xFEC0..=0xFEDF => {}
            _ => return false,
        }
        true
    }

    fn mem_read(&mut self, addr: u16) -> u8 {
        if self.in_shadow_screen(addr) || self.in_paged_ram(addr) {
            return self.state.extra_ram[usize::from(addr & 0x7FFF)];
        }
        if self.in_hazel(addr) {
            return self.state.extra_ram[usize::from(addr - 0xB000)];
        }
        if self.io_reads_rom(addr) {
            return self
                .state
                .mos_rom
                .get(usize::from(addr - 0xC000))
                .copied()
                .unwrap_or(0xFF);
        }
        if B::MODEL == BbcModel::Master128
            && let Some(value) = self.master_io_read(addr)
        {
            return value;
        }
        match addr {
            0x0000..=0x7FFF => self.state.ram[addr as usize],
            0x8000..=0xBFFF => self
                .state
                .sideways_roms
                .get(self.state.rom_bank as usize)
                .and_then(|rom| rom.get((addr - 0x8000) as usize).copied())
                .unwrap_or(0xFF),
            0xFE00..=0xFE07 if addr & 1 == 1 => self.state.crtc.read_data(),
            // Each SHEILA device answers across its whole block, repeating
            // its registers (the Advanced User Guide's SHEILA map, which
            // opens its hardware section: "the same devices appear at
            // several different Sheila addresses").
            0xFE84..=0xFE9F if B::MODEL == BbcModel::ModelBPlus => {
                self.state.fdc.read((addr & 0x03) as u8)
            }
            0xFE40..=0xFE5F => self.state.system_via.read((addr & 0x0F) as u8),
            0xFE60..=0xFE7F => self.state.user_via.read((addr & 0x0F) as u8),
            0xFEC0..=0xFEDF => self.state.adc.read((addr & 0x03) as u8),
            0xFE08..=0xFE0F => self.state.acia.read(addr),
            0xFC00..=0xFEFF => 0xFF,
            0xC000..=0xFFFF => self
                .state
                .mos_rom
                .get((addr - 0xC000) as usize)
                .copied()
                .unwrap_or(0xFF),
        }
    }

    fn mem_write(&mut self, addr: u16, value: u8) {
        if self.in_shadow_screen(addr) || self.in_paged_ram(addr) {
            self.state.extra_ram[usize::from(addr & 0x7FFF)] = value;
            return;
        }
        if self.in_hazel(addr) {
            self.state.extra_ram[usize::from(addr - 0xB000)] = value;
            return;
        }
        if (0x8000..=0xBFFF).contains(&addr)
            && self.state.sideways_ram & (1 << self.state.rom_bank) != 0
        {
            if let Some(byte) = self
                .state
                .sideways_roms
                .get_mut(usize::from(self.state.rom_bank))
                .and_then(|bank| bank.get_mut(usize::from(addr - 0x8000)))
            {
                *byte = value;
            }
            return;
        }
        if B::MODEL == BbcModel::Master128 && self.master_io_write(addr, value) {
            return;
        }
        match addr {
            0x0000..=0x7FFF => self.state.ram[addr as usize] = value,
            0xFE00..=0xFE07 if addr & 1 == 0 => self.state.crtc.write_address(value),
            0xFE00..=0xFE07 if addr & 1 == 1 => self.state.crtc.write_data(value),
            // SHEILA devices repeat across their blocks: the Video ULA's two
            // registers through `&FE2F`, the ROM select through `&FE3F`, each
            // VIA's sixteen through 32 bytes.
            0xFE20..=0xFE2F if addr & 1 == 0 => self.state.video_ula.write_control(value),
            0xFE20..=0xFE2F => self.state.video_ula.write_palette(value),
            0xFE08..=0xFE0F => self.state.acia.write(addr, value),
            // Serial ULA: RX/TX baud, RS423/cassette select, bit 7 motor relay.
            0xFE10..=0xFE1F => self.state.serial_ula = value,
            // ROMSEL at `&FE30`; on a B+, bit 7 pages RAM in at `&8000-&AFFF`,
            // and ACCCON at `&FE34` selects the shadow screen with bit 7.
            // Each repeats at +8 (MAME `bbcbp_state::romsel_w`).
            0xFE30..=0xFE3F if B::MODEL == BbcModel::ModelBPlus => match addr & 0x07 {
                0x00 => {
                    self.state.rom_bank = value & 0x0F;
                    self.state.paged_ram = value & 0x80 != 0;
                }
                0x04 => self.state.shadow_screen = value & 0x80 != 0,
                _ => {}
            },
            0xFE30..=0xFE3F => self.state.rom_bank = value & 0x0F,
            0xFE40..=0xFE5F => {
                let reg = (addr & 0x0F) as u8;
                self.state.system_via.write(reg, value);
                // System VIA port B carries the IC32 addressable
                // latch encoding: low 3 bits = address, bit 3 = data.
                if reg == 0x00 {
                    let latch_addr = value & 0x07;
                    let latch_data = value & 0x08 != 0;
                    if let Some(()) = self.state.latch.write(latch_addr, latch_data).map(|_| ()) {
                        // SN76489 /WE asserted — latch the byte on
                        // ORA into the PSG.
                        self.state.psg.write(self.state.system_via.ora());
                    }
                }
                if B::MODEL == BbcModel::Master128 {
                    self.update_rtc(reg == 0x00);
                }
            }
            0xFE60..=0xFE7F => self.state.user_via.write((addr & 0x0F) as u8, value),
            0xFE80..=0xFE83 if B::MODEL == BbcModel::ModelBPlus => {
                self.write_drive_control(value);
            }
            0xFE84..=0xFE9F if B::MODEL == BbcModel::ModelBPlus => {
                self.state.fdc.write((addr & 0x03) as u8, value);
            }
            // Only a write to the control register ($FEC0, reg 0) starts a
            // conversion. Beginning one releases EOC (CB1 high) until the
            // countdown completes and pulls it low again. Writes to the result
            // registers are no-ops (they fall through to the catch-all).
            0xFEC0..=0xFEDF if addr & 0x03 == 0 => {
                self.state.adc.write_control(value);
                self.state.system_via.set_cb1_level(true);
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
        let column = self.state.crtc.horizontal_counter();
        let frame_ended = self.state.crtc.tick();
        // The SAA5050 counts lines on DISPTMG and fields on VSYNC whatever
        // mode the Video ULA is in. R8 bits 4-5 = `11` hold DISPTMG off.
        let disptmg = self.state.crtc.display_enable && self.state.crtc.regs()[8] & 0x30 != 0x30;
        self.state
            .saa5050
            .clock_timing(disptmg, self.state.crtc.vsync);
        if column == 0 {
            self.start_video_line();
        }
        self.draw_character(column);
        self.draw_cursor(column);
        if frame_ended {
            self.state.video_frame_ended = true;
            self.state.native_frame_ready = true;
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
        if self.state.video_frame_ended {
            self.state.video_frame_ended = false;
            self.state.beam_line = 0;
            core::mem::swap(&mut self.state.framebuffer, &mut self.state.back_buffer);
            self.state
                .back_buffer
                .copy_from_slice(&self.state.framebuffer);
        } else {
            self.state.beam_line = self.state.beam_line.saturating_add(1);
        }
        if let Some(rows) = self.beam_rows() {
            let width = FB_WIDTH as usize;
            self.state.back_buffer[rows.start * width..rows.end * width].fill(BLANK);
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
        let first = usize::from(self.state.beam_line) * ROWS_PER_LINE;
        if first >= FB_HEIGHT as usize {
            return None;
        }
        Some(if self.state.crtc.interlace_sync_and_video() {
            let row = first + usize::from(self.state.crtc.odd_field());
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
            self.state
                .back_buffer
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
                address.wrapping_sub(self.state.latch.screen_wrap_size()) & 0x7FFF
            }
        }
    }

    /// The byte the video circuitry fetches at `address`: from the B+'s
    /// shadow screen while ACCCON selects it, otherwise from main RAM.
    fn video_byte(&self, address: u16) -> u8 {
        if self.state.shadow_screen {
            self.state.extra_ram[usize::from(address & 0x7FFF)]
        } else {
            self.state.ram[usize::from(address & 0x7FFF)]
        }
    }

    /// Draw the character the 6845 is addressing at `column` of the current
    /// line, if it is displaying one.
    fn draw_character(&mut self, column: u8) {
        let Some(rows) = self.beam_rows() else {
            return;
        };
        let ma = self.state.crtc.memory_address();
        let ra = self.state.crtc.raster_address();
        let byte = self.video_byte(self.video_address(ma, ra));
        if self.state.video_ula.teletext() {
            self.draw_teletext_character(rows, usize::from(column), byte, ra);
            return;
        }
        // Display enable is masked by RA3, so a cell taller than eight lines
        // blanks the rest: the gaps between rows in the gapped text modes 3
        // and 6. R8 bits 4-5 = `11` turns the display off outright (§18.6.2).
        if !self.state.crtc.display_enable
            || ra & 0x08 != 0
            || self.state.crtc.regs()[8] & 0x30 == 0x30
        {
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
        let char_pixels = if self.state.video_ula.fast_clock() {
            FAST_CHAR_PIXELS
        } else {
            FAST_CHAR_PIXELS * 2
        };
        let pixels_per_byte = self.state.video_ula.pixels_per_byte();
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
            let argb = self.state.video_ula.palette_to_argb(colour_idx);
            let x = x0 + px * pixel_width;
            for fb_x in x..(x + pixel_width).min(FB_WIDTH as usize) {
                self.state.back_buffer[offset + fb_x] = argb;
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
        let skew = self.state.crtc.regs()[8] >> 6;
        if self.state.crtc.cursor_active && skew != 3 {
            self.state.cursor_stage = 3 - skew;
        }
        if self.state.cursor_stage == 0 {
            return;
        }
        let stage = self.state.cursor_stage;
        self.state.cursor_stage = (stage + 1) % CURSOR_STAGES;
        if !self.state.video_ula.cursor_segment(stage) {
            return;
        }
        let Some(rows) = self.beam_rows() else {
            return;
        };
        let (cell, width) = if self.state.video_ula.teletext() {
            let Some(cell) = column.checked_sub(TELETEXT_PICTURE_DELAY) else {
                return;
            };
            (cell, TELETEXT_CELL_WIDTH)
        } else if self.state.video_ula.fast_clock() {
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
            for argb in &mut self.state.back_buffer[offset + x0..offset + x1] {
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
        if !self.state.crtc.display_enable || self.state.crtc.regs()[8] & 0x30 == 0x30 {
            return;
        }
        // Only D0-D6 reach the SAA5050, so `$81` is the control code `$01`.
        // That is how the MOS's coloured text works; b-em masks the same way.
        let cell =
            self.state
                .saa5050
                .character(byte & 0x7F, ra & 0x01 != 0, &self.state.teletext_font);
        let x0 = column * TELETEXT_CELL_WIDTH;
        let offset = rows.start * FB_WIDTH as usize;
        for (px, coverage) in teletext_coverage(cell.pattern).into_iter().enumerate() {
            let fb_x = x0 + px;
            if fb_x >= FB_WIDTH as usize {
                break;
            }
            self.state.back_buffer[offset + fb_x] = teletext_pixel(cell.fg, cell.bg, coverage);
        }
        let x1 = (x0 + TELETEXT_CELL_WIDTH).min(FB_WIDTH as usize);
        if x0 < x1 {
            self.repeat_span(rows, x0, x1);
        }
    }

    /// Framebuffer (640×512 ARGB32): both fields, woven (see [`FB_HEIGHT`]).
    #[must_use]
    pub fn framebuffer(&self) -> &[u32] {
        &self.state.framebuffer
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
        self.state.psg.take_buffer()
    }

    /// Press a key at the given (column, row).
    pub fn press_key(&mut self, col: usize, row: usize) {
        if col < 10 && row < 8 {
            self.state.keyboard[col][row] = true;
        }
    }

    /// Release a key at the given (column, row).
    pub fn release_key(&mut self, col: usize, row: usize) {
        if col < 10 && row < 8 {
            self.state.keyboard[col][row] = false;
        }
    }

    /// CPU reference.
    #[must_use]
    pub fn cpu(&self) -> &M6502 {
        &self.state.cpu
    }

    /// CPU mutable reference.
    pub fn cpu_mut(&mut self) -> &mut M6502 {
        &mut self.state.cpu
    }

    /// CRTC reference.
    #[must_use]
    pub fn crtc(&self) -> &Crtc6845 {
        &self.state.crtc
    }

    /// Current ROM bank (0-15).
    #[must_use]
    pub fn rom_bank(&self) -> u8 {
        self.state.rom_bank
    }

    /// Frame count since power-on.
    #[must_use]
    pub fn frame_count(&self) -> u64 {
        self.state.frame_count
    }

    /// CPU cycles since power-on.
    #[must_use]
    pub fn cpu_cycles(&self) -> u64 {
        self.state.cpu_cycles
    }
}

impl<B: Board> BbcCore<B> {
    /// Read one byte with no side effects (RAM / sideways ROM / MOS;
    /// `$FF` for the SHEILA I/O page).
    #[must_use]
    pub fn peek(&self, addr: u16) -> u8 {
        if self.in_shadow_screen(addr) || self.in_paged_ram(addr) {
            return self.state.extra_ram[usize::from(addr & 0x7FFF)];
        }
        match addr {
            0x0000..=0x7FFF => self.state.ram[addr as usize],
            0x8000..=0xBFFF => self
                .state
                .sideways_roms
                .get(self.state.rom_bank as usize)
                .and_then(|rom| rom.get((addr - 0x8000) as usize).copied())
                .unwrap_or(0xFF),
            0xFC00..=0xFEFF => 0xFF,
            0xC000..=0xFFFF => self
                .state
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
        while self.state.cpu.instruction_complete() && ticks < 4096 {
            self.tick_cpu_cycle();
            ticks += 1;
        }
        while !self.state.cpu.instruction_complete() && ticks < 4096 {
            self.tick_cpu_cycle();
            ticks += 1;
        }
        ticks
    }
}

#[cfg(test)]
mod tests;

/// Shared DFS image operation, independent of board wiring.
pub fn dfs_disk(image: &[u8], sides: usize, writable: bool) -> Disk {
    let sides = sides.clamp(1, 2);
    let tracks_in_image = image.len().div_ceil(DFS_TRACK_BYTES * sides);
    let tracks = if tracks_in_image > 40 { 80 } else { 40 };
    let mut data = image.to_vec();
    data.resize(data.len().max(tracks * sides * DFS_TRACK_BYTES), 0);
    Disk::new(data, tracks, sides, 10, 256)
        .with_first_sector_id(0)
        .write_protected(!writable)
}
/// Shared DFS image operation, independent of board wiring.
pub fn dfs_sides(image: &[u8]) -> usize {
    if image.len() > 80 * DFS_TRACK_BYTES {
        return 2;
    }
    let sector_count = |catalogue: usize| -> Option<u16> {
        let bytes = image.get(catalogue + 256 + 5..catalogue + 256 + 8)?;
        // Byte 5 is eight times the file count, at most 31 files.
        if bytes[0] % 8 != 0 || bytes[0] > 31 * 8 {
            return None;
        }
        Some(u16::from(bytes[1] & 0x03) << 8 | u16::from(bytes[2]))
    };
    match (sector_count(0), sector_count(DFS_TRACK_BYTES)) {
        (Some(side0), Some(side1)) if side0 == side1 && matches!(side0, 400 | 800) => 2,
        _ => 1,
    }
}
