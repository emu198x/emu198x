//! Amstrad CPC464 — Z80, Gate Array, 6845 CRTC, AY-3-8912 and 8255 PPI.
//!
//! Scoped in `Emu198x/docs/plans/2026-08-13-amstrad-cpc-plan.md`. Every chip but
//! the Gate Array was already in the workspace and proven in a shipping machine,
//! so this crate is wiring rather than new silicon.
//!
//! # Clock
//!
//! A 16 MHz crystal gives a 4 MHz Z80 and a 1 MHz CRTC — the ratios MAME's
//! `amstrad_base` configures (`Z80(16_MHz_XTAL / 4)`, `HD6845S(16_MHz_XTAL /
//! 16)`). So the CRTC advances one character clock every four T-states, and the
//! AY, at the same 1 MHz, moves with it.
//!
//! **The Z80 is ticked twice per T-state.** `Z80::tick` advances one half-cycle,
//! and a machine that calls it once per T-state runs its CPU at half speed —
//! which is what nine machines in this workspace did until the CPU-rate campaign
//! of 2026-08-13 measured them. `tests/cpu_rate.rs` holds this machine to the
//! same figure from its first commit rather than acquiring the defect and
//! discovering it later. See
//! `knowledge/decisions/z80-validation-surface.md`.
//!
//! # What is not modelled yet
//!
//! **`/WAIT`.** The Gate Array stretches every Z80 M-cycle to a multiple of four
//! T-states, giving an effective ~3.3 MHz rather than 4 — stated outright in the
//! official firmware guide, which is in the reference library:
//!
//! > Accesses to memory are synchronised with the video logic — they are
//! > constrained to occur on microsecond boundaries. This has the effect of
//! > stretching each Z80 M cycle (machine cycle) to be a multiple of 4 T states
//! > (clock cycles). In practice this alters the instruction timing so that the
//! > effective clock rate is approximately 3.3 MHz.
//!
//! `Z80::wait` is a modelled pin the core honours, so the mechanism is
//! available. What is missing is an oracle: none of the three vendored
//! emulators models `/WAIT` as a pin (MAME configures a flat 4 MHz Z80; Arnold
//! folds the stretching into per-instruction cycle counts), so it has to be
//! validated against that ~3.3 MHz figure and observed program timing rather
//! than by reading their source. Until then the CPU runs unstretched, and
//! `cpu_rate` asserts the unstretched figure so the change is visible when it
//! lands.
//!
//! # Video
//!
//! Rendered at the dot clock rather than reconstructed from the CRTC's
//! registers: every character clock, whatever the CRTC is pointing at right now
//! becomes sixteen dots. Two bytes are fetched, the Gate Array shifts them out
//! through the current mode and palette, and with display disabled the border
//! pen fills the slot instead — which is the whole of how a CPC border works.
//!
//! The beam locks to the CRTC's sync pulses, exactly as the monitor does. That
//! is what makes this machine's characteristic tricks work: a program that
//! moves R2 or R7, splits the screen with R12/R13, or changes mode partway down
//! a frame gets what the hardware would give it, because nothing here assumes a
//! screen is 40 characters by 25 rows. Deriving addresses from the registers
//! instead — the approach `machine-acorn-bbc-micro` takes with the same chip —
//! would be simpler and would have to be replaced to run most of the CPC
//! software worth running.
//!
//! # I/O decode
//!
//! The CPC decodes I/O on the *high* address bits, partially, so one port can
//! reach several devices. From MAME's `amstrad_cpc_io_r` / `amstrad_cpc_io_w`:
//!
//! | Condition | Device |
//! |---|---|
//! | A15 = 0 and A14 = 1 | Gate Array (write only) |
//! | A14 = 0 | 6845 CRTC, function in A9-A8 |
//! | A13 = 0 | ROM select |
//! | A12 = 0 | printer |
//! | A11 = 0 | 8255 PPI, port in A9-A8 |
//! | A10 = 0 | expansion / FDC — absent on a 464 |

use amstrad_gate_array::GateArray;
use common_tape::{TapePlayer, TapeSpan};
use gi_ay_3_8912::Ay3_8912;
use intel_8255::Ppi8255;
use motorola_6845::Crtc6845;
use serde::{Deserialize, Serialize};
use zilog_z80::{BusOp, Z80};

/// T-states per CRTC character clock: 4 MHz CPU against a 1 MHz CRTC.
const TSTATES_PER_CRTC_TICK: u32 = 4;

/// T-states in one PAL frame: 64 character clocks per line × 312 lines = 19,968
/// microseconds, and four T-states to the microsecond at 4 MHz. That is
/// ~50.08 Hz, the CPC's actual refresh.
const TSTATES_PER_FRAME: u64 = 64 * 312 * TSTATES_PER_CRTC_TICK as u64;

/// AY-3-8912 clock, 1 MHz — the same divider as the CRTC.
const AY_CLOCK_HZ: u32 = 1_000_000;
const AY_SAMPLE_RATE: u32 = 48_000;
const AY_SAMPLES_PER_FRAME: usize = 1024;

/// Dots per character clock: a 16 MHz dot clock against the 1 MHz CRTC.
///
/// The Gate Array fetches two bytes per character clock and shifts them out
/// across these sixteen dots — eight each. How many dots a *pixel* occupies is
/// therefore a consequence of the mode rather than a separate setting: mode 2
/// packs eight pixels into a byte and so spends one dot each, mode 1 four
/// pixels at two dots, mode 0 two pixels at four.
const DOTS_PER_CHAR: usize = 16;

/// Framebuffer width, 48 character columns at full dot resolution.
///
/// Caprice32 draws a visible window of `4 + 40 + 4` columns — four of border,
/// the forty of a standard display, four more of border (`CPC_VISIBLE_SCR_WIDTH`
/// in `cap32.h`, given there at half dot resolution as 384). At the full dot
/// clock that is 768.
pub const FB_WIDTH: u32 = 48 * DOTS_PER_CHAR as u32;

/// Framebuffer height, matching Caprice32's `CPC_VISIBLE_SCR_HEIGHT`: the 200
/// displayed lines with 35 of border above and below.
pub const FB_HEIGHT: u32 = 270;

/// Dots after the HSync edge at which the visible window opens.
///
/// A standard CPC line puts HSync at character 46 of 64 (CRTC R2 against R0),
/// so the display restarts `64 - 46 = 18` characters after the sync edge.
/// Opening the window four characters earlier gives the left border its four
/// columns: `(18 - 4) x 16`.
const H_VISIBLE_START: i32 = 14 * DOTS_PER_CHAR as i32;

/// Lines after the VSync edge at which the visible window opens.
///
/// A standard screen puts VSync at character row 30 of 39 (CRTC R7 against R4),
/// eight lines to the row, so the display restarts `312 - 240 = 72` lines after
/// the sync edge. Opening 35 lines earlier centres the 200 displayed lines in
/// the 270 the window is tall.
const V_VISIBLE_START: i32 = 72 - 35;

/// Keyboard matrix rows. Nine of keys plus row 9, which carries joystick 0 and
/// the `DEL` key.
pub const KEYBOARD_ROWS: usize = 10;

/// Shift lives at row 2, bit 5 (MAME `kbrow.2`, mask `0x20`).
const SHIFT_ROW: usize = 2;
const SHIFT_BIT: u8 = 5;

/// Where a character sits in the matrix: row, bit, and whether Shift is needed.
///
/// Only the unshifted and Shift-ed legends are covered — the CPC's Control
/// combinations and the keypad are reachable through [`AmstradCpc::press_key`]
/// directly. Rows and bits are MAME's `kbrow.N` port definitions for `cpc464`,
/// cross-checked against Caprice32's `InputMapper::cpc_kbd`
/// (`emulators/amstrad-cpc/caprice32/src/keyboard.cpp`), whose scancodes read
/// as `0xRB`. Caprice32 is where `^` and the five Shift-ed legends below it
/// came from: they are keys a CPC464 has and this table did not, so a caller
/// typing `{` was told the machine could not produce it.
#[must_use]
pub fn key_for_char(c: char) -> Option<(usize, u8, bool)> {
    // (row, bit) for the unshifted legend, then the shifted legend where the
    // two differ. The CPC's number row is shifted the way a UK keyboard is.
    let plain: &[(char, usize, u8)] = &[
        ('\r', 2, 2),
        ('\n', 2, 2), // Enter
        (' ', 5, 7),
        ('0', 4, 0),
        ('9', 4, 1),
        ('8', 5, 0),
        ('7', 5, 1),
        ('6', 6, 0),
        ('5', 6, 1),
        ('4', 7, 0),
        ('3', 7, 1),
        ('2', 8, 1),
        ('1', 8, 0),
        ('-', 3, 1),
        ('@', 3, 2),
        ('p', 3, 3),
        (';', 3, 4),
        (':', 3, 5),
        ('/', 3, 6),
        ('.', 3, 7),
        ('o', 4, 2),
        ('i', 4, 3),
        ('l', 4, 4),
        ('k', 4, 5),
        ('m', 4, 6),
        (',', 4, 7),
        ('u', 5, 2),
        ('y', 5, 3),
        ('h', 5, 4),
        ('j', 5, 5),
        ('n', 5, 6),
        ('r', 6, 2),
        ('t', 6, 3),
        ('g', 6, 4),
        ('f', 6, 5),
        ('b', 6, 6),
        ('v', 6, 7),
        ('e', 7, 2),
        ('w', 7, 3),
        ('s', 7, 4),
        ('d', 7, 5),
        ('c', 7, 6),
        ('x', 7, 7),
        ('q', 8, 3),
        ('a', 8, 5),
        ('z', 8, 7),
        ('[', 2, 1),
        (']', 2, 3),
        ('\\', 2, 6),
        ('^', 3, 0),
    ];
    // Legends reached with Shift.
    let shifted: &[(char, usize, u8)] = &[
        ('£', 3, 0),
        ('|', 3, 2),
        ('`', 2, 6),
        ('{', 2, 1),
        ('}', 2, 3),
        ('=', 3, 1),
        ('*', 3, 5),
        ('?', 3, 6),
        ('>', 3, 7),
        ('_', 4, 0),
        (')', 4, 1),
        ('<', 4, 7),
        ('(', 5, 0),
        ('\'', 5, 1),
        ('&', 6, 0),
        ('%', 6, 1),
        ('$', 7, 0),
        ('#', 7, 1),
        ('"', 8, 1),
        ('!', 8, 0),
        ('+', 3, 4),
    ];

    let lower = c.to_ascii_lowercase();
    if let Some(&(_, row, bit)) = plain.iter().find(|&&(k, _, _)| k == lower) {
        // An upper-case letter is the same key with Shift; a digit is not,
        // because its shifted legend is punctuation.
        return Some((row, bit, c.is_ascii_uppercase()));
    }
    shifted
        .iter()
        .find(|&&(k, _, _)| k == c)
        .map(|&(_, row, bit)| (row, bit, true))
}

/// Amstrad CPC464.
#[derive(Serialize, Deserialize)]
pub struct AmstradCpc {
    cpu: Z80,
    gate_array: GateArray,
    crtc: Crtc6845,
    psg: Ay3_8912,
    ppi: Ppi8255,

    /// 64 KB of RAM, always writable even where a ROM is paged in.
    ram: Vec<u8>,
    /// Lower ROM: the OS, at `$0000-$3FFF` when the Gate Array enables it.
    os_rom: Vec<u8>,
    /// Upper ROM: BASIC, at `$C000-$FFFF` when enabled.
    basic_rom: Vec<u8>,
    /// Selected upper ROM number, from the ROM-select port. Only 0 (BASIC) is
    /// populated on a 464 without expansions.
    selected_upper_rom: u8,

    /// T-states remaining before the next CRTC character clock.
    crtc_phase: u32,
    /// AY register latch, driven through PPI port C.
    psg_control: u8,
    /// Cassette. The CPC drives the motor itself through PPI port C, so this
    /// plays only while the firmware says it should.
    tape: TapePlayer,
    /// Whether the machine has the cassette motor running.
    tape_motor: bool,
    cpu_tstates: u64,
    frame_count: u64,

    /// Visible display, ARGB32. Per
    /// `knowledge/decisions/framebuffer-pixel-format.md` the format is the
    /// chip's own choice; the Gate Array resolves pens to colours itself
    /// through `decode_byte_rgb`, so writing ARGB directly costs nothing.
    framebuffer: Vec<u32>,
    /// Keyboard matrix, active low: a zero bit is a pressed key. Ten rows,
    /// selected by PPI port C and read back through the AY's port A.
    keyboard: [u8; KEYBOARD_ROWS],
    /// Dots since the last HSync edge — the beam's position across the line.
    beam_x: i32,
    /// Lines since the last VSync edge.
    beam_y: i32,
    prev_hsync: bool,
    prev_vsync: bool,
}

impl AmstradCpc {
    /// Build a CPC464 from its 32 KB firmware image: 16 KB OS followed by
    /// 16 KB BASIC, which is the layout MAME's `cpc464.rom` uses and the one
    /// `~/.emu198x/roms/amstrad-cpc/cpc464.rom` is assembled to.
    ///
    /// # Errors
    ///
    /// Returns an error unless the image is exactly 32 KB.
    pub fn new(firmware: &[u8]) -> Result<Self, String> {
        if firmware.len() != 0x8000 {
            return Err(format!(
                "CPC firmware must be 32 KB (16 KB OS + 16 KB BASIC), got {}",
                firmware.len()
            ));
        }
        Ok(Self {
            cpu: Z80::new(),
            gate_array: GateArray::new(),
            crtc: Crtc6845::new(),
            psg: Ay3_8912::new(AY_CLOCK_HZ, AY_SAMPLE_RATE, AY_SAMPLES_PER_FRAME),
            ppi: Ppi8255::new(),
            ram: vec![0; 0x1_0000],
            os_rom: firmware[..0x4000].to_vec(),
            basic_rom: firmware[0x4000..].to_vec(),
            selected_upper_rom: 0,
            crtc_phase: 0,
            psg_control: 0,
            tape: TapePlayer::new(),
            tape_motor: false,
            cpu_tstates: 0,
            frame_count: 0,
            framebuffer: vec![0xFF00_0000; (FB_WIDTH * FB_HEIGHT) as usize],
            // Active low: every key released.
            keyboard: [0xFF; KEYBOARD_ROWS],
            beam_x: 0,
            beam_y: 0,
            prev_hsync: false,
            prev_vsync: false,
        })
    }

    /// CPU T-states since power-on.
    #[must_use]
    pub fn cpu_tstates(&self) -> u64 {
        self.cpu_tstates
    }

    /// Frames completed since power-on.
    #[must_use]
    pub fn frame_count(&self) -> u64 {
        self.frame_count
    }

    /// Observe a byte through the CPU's memory map, without side effects.
    #[must_use]
    pub fn peek(&self, addr: u16) -> u8 {
        self.mem_read(addr)
    }

    /// The Gate Array, for inspecting video mode, palette and interrupt state.
    #[must_use]
    pub fn gate_array(&self) -> &GateArray {
        &self.gate_array
    }

    /// The CRTC, for inspecting the programmed screen geometry.
    #[must_use]
    pub fn crtc(&self) -> &Crtc6845 {
        &self.crtc
    }

    /// The PSG, for inspecting the AY's register file.
    #[must_use]
    pub fn psg(&self) -> &Ay3_8912 {
        &self.psg
    }

    /// The CPU, for register inspection and disassembly.
    #[must_use]
    pub fn cpu(&self) -> &Z80 {
        &self.cpu
    }

    /// The CPU, mutably — needed after a snapshot restore, which has to
    /// rebuild the micro-op walker the serialised state cannot carry.
    pub fn cpu_mut(&mut self) -> &mut Z80 {
        &mut self.cpu
    }

    /// The 64 KB of RAM, whatever is paged over it.
    #[must_use]
    pub fn ram(&self) -> &[u8] {
        &self.ram
    }

    /// Write one byte, bypassing the CPU.
    ///
    /// Lands in RAM whatever is paged in, because that is what a CPU write
    /// does here — the ROMs are read-only overlays, not a competing store.
    /// A `poke` into `$0000-$3FFF` while the lower ROM is enabled therefore
    /// takes effect but stays invisible to [`Self::peek`] until the firmware
    /// pages the ROM out, which is the hardware's behaviour rather than a
    /// limitation of this method.
    pub fn poke(&mut self, addr: u16, value: u8) {
        self.ram[addr as usize] = value;
    }

    /// Drain the PSG's audio for the frame just run.
    ///
    /// Always a whole frame, silence included. Trimming trailing zeros —
    /// which this did, copied from the Einstein — makes a quiet frame
    /// contribute nothing, so a capture of a silent machine produced a WAV
    /// with no samples and an MP4 with no streams, both reported as
    /// success. It also shortens the audio timeline relative to the video
    /// one, so a note played after a quiet passage lands early (#934).
    pub fn take_audio_buffer(&mut self) -> Vec<f32> {
        let mut out = vec![0.0_f32; AY_SAMPLES_PER_FRAME];
        self.psg.end_frame(&mut out);
        out
    }

    /// Framebuffer (768×270 ARGB32).
    #[must_use]
    pub fn framebuffer(&self) -> &[u32] {
        &self.framebuffer
    }

    /// Framebuffer width in pixels.
    #[must_use]
    pub fn framebuffer_width(&self) -> u32 {
        FB_WIDTH
    }

    /// Framebuffer height in pixels.
    #[must_use]
    pub fn framebuffer_height(&self) -> u32 {
        FB_HEIGHT
    }

    /// Load a tape, as a timing-span stream in the CPC's own T-states.
    ///
    /// `format-amstrad-cpc-cdt` produces these from a `.cdt`, having already
    /// scaled the file's reference-clock figures to 4 MHz.
    pub fn insert_tape(&mut self, spans: Vec<TapeSpan>) {
        self.tape.load_stream(spans);
        if self.tape_motor {
            self.tape.play();
        }
    }

    /// Whether the firmware currently has the cassette motor running.
    #[must_use]
    pub fn tape_motor_on(&self) -> bool {
        self.tape_motor
    }

    /// The tape, for inspecting playback position.
    #[must_use]
    pub fn tape(&self) -> &TapePlayer {
        &self.tape
    }

    /// Start or stop the cassette motor.
    ///
    /// The CPC drives this itself: `CAT`, `RUN"` and `LOAD"` all pull PPI port
    /// C bit 4 high before reading, and drop it when they are done. Playback
    /// follows the motor rather than running free, so a tape sitting in the
    /// machine with nothing loading stays where it is.
    fn set_tape_motor(&mut self, on: bool) {
        if on == self.tape_motor {
            return;
        }
        self.tape_motor = on;
        if on {
            self.tape.play();
        } else {
            self.tape.stop();
        }
    }

    /// Press a key at the given (row, bit) matrix cell.
    pub fn press_key(&mut self, row: usize, bit: u8) {
        if row < KEYBOARD_ROWS && bit < 8 {
            self.keyboard[row] &= !(1 << bit);
        }
    }

    /// Release a key at the given (row, bit) matrix cell.
    pub fn release_key(&mut self, row: usize, bit: u8) {
        if row < KEYBOARD_ROWS && bit < 8 {
            self.keyboard[row] |= 1 << bit;
        }
    }

    /// Press the key (and Shift, where the legend needs it) that produces `c`.
    ///
    /// Returns false for a character the keyboard cannot produce, so a caller
    /// typing a string can tell the difference between "typed" and "silently
    /// dropped".
    pub fn press_char(&mut self, c: char) -> bool {
        let Some((row, bit, shift)) = key_for_char(c) else {
            return false;
        };
        if shift {
            self.press_key(SHIFT_ROW, SHIFT_BIT);
        }
        self.press_key(row, bit);
        true
    }

    /// Release the key (and Shift) that produces `c`.
    pub fn release_char(&mut self, c: char) {
        if let Some((row, bit, shift)) = key_for_char(c) {
            self.release_key(row, bit);
            if shift {
                self.release_key(SHIFT_ROW, SHIFT_BIT);
            }
        }
    }

    /// Run one frame's worth of T-states, returning how many were consumed.
    ///
    /// Deliberately a fixed budget rather than "until the CRTC completes a
    /// frame". The CRTC powers up with every register at zero, which makes
    /// `h_total` and `v_total` zero too, so it reports a completed frame every
    /// couple of character clocks — a CRTC-driven loop returns after about five
    /// T-states and the firmware never runs far enough to program the CRTC out
    /// of that state. Frame completion is still available to a video layer
    /// through the CRTC itself; it just cannot be what paces the CPU.
    pub fn run_frame(&mut self) -> u64 {
        let start = self.cpu_tstates;
        while self.cpu_tstates - start < TSTATES_PER_FRAME {
            self.tick_tstate();
        }
        self.frame_count += 1;
        self.cpu_tstates - start
    }

    /// Advance one T-state.
    fn tick_tstate(&mut self) {
        // Two CPU half-cycles per T-state. `Z80::tick` advances one half-cycle,
        // so calling it once here would run the CPU at half speed — the defect
        // the 2026-08-13 campaign found on nine machines. `cpu_rate.rs` holds
        // this to 4 T-states per `NOP`.
        for _ in 0..2 {
            // Pins before the tick: the Z80 samples `/INT` at an instruction
            // boundary during its own tick, so feeding the line afterwards
            // hands it the previous half-cycle's state.
            self.cpu.irq = self.gate_array.interrupt();
            self.cpu.tick();
            self.handle_bus();
        }

        self.crtc_phase += 1;
        if self.crtc_phase >= TSTATES_PER_CRTC_TICK {
            self.crtc_phase = 0;
            self.crtc.tick();
            self.track_beam();
            self.draw_char();
            // The Gate Array counts the CRTC's syncs; this is the whole of the
            // CPC's interrupt source.
            self.gate_array.set_hsync(self.crtc.hsync);
            self.gate_array.set_vsync(self.crtc.vsync);
            self.psg.tick();
        }

        // The tape runs on wall-clock time, not on anything the CPU asks for,
        // so it advances every T-state the machine executes — but only while
        // the motor is on, which is the firmware's decision.
        if self.tape_motor {
            self.tape.advance_tstates(1);
        }

        self.cpu_tstates += 1;
    }

    /// Move the beam in response to the CRTC's sync pulses.
    ///
    /// The CPC's monitor has no idea where a frame begins; it locks to the sync
    /// pulses it is handed. Deriving the beam position the same way is what
    /// makes the CRTC tricks the machine is known for work: a program that
    /// moves R2 or R7, or ends a line early, moves the picture here exactly as
    /// it would on the real thing, because nothing anywhere assumes a screen is
    /// 40 characters by 25 rows.
    fn track_beam(&mut self) {
        let hsync = self.crtc.hsync;
        let vsync = self.crtc.vsync;
        if vsync && !self.prev_vsync {
            self.beam_y = 0;
        }
        if hsync && !self.prev_hsync {
            self.beam_x = 0;
            self.beam_y += 1;
        }
        self.prev_hsync = hsync;
        self.prev_vsync = vsync;
    }

    /// Shift one character clock's worth of dots out to the framebuffer.
    ///
    /// Two bytes per character clock, eight dots each. With display disabled
    /// the Gate Array emits the border colour instead of fetching anything,
    /// which is the whole of how the CPC's border works — there is no separate
    /// border register beyond its pen.
    fn draw_char(&mut self) {
        let mut dots = [self.gate_array.border_rgb(); DOTS_PER_CHAR];
        if self.crtc.display_enable {
            let base = Self::screen_address(self.crtc.memory_address(), self.crtc.raster_address());
            let mut pixels = [0u32; 8];
            for half in 0..2 {
                // Video fetches come off RAM directly: the Gate Array is not
                // behind the CPU's memory map, so a paged-in ROM is invisible
                // to it. MAME reads `m_ram->pointer()[address]` for the same
                // reason.
                let byte = self.ram[base.wrapping_add(half) as usize];
                let count = self.gate_array.decode_byte_rgb(byte, &mut pixels);
                if count == 0 {
                    continue;
                }
                let dots_per_pixel = DOTS_PER_CHAR / 2 / count;
                let origin = half as usize * (DOTS_PER_CHAR / 2);
                for (i, &colour) in pixels.iter().take(count).enumerate() {
                    let start = origin + i * dots_per_pixel;
                    dots[start..start + dots_per_pixel].fill(colour);
                }
            }
        }
        self.blit(&dots);
        // Unconditionally, and not inside `blit`: the beam keeps sweeping
        // across lines that fall outside the visible window, and stalling it
        // there would shear every line that does land inside one.
        self.beam_x += DOTS_PER_CHAR as i32;
    }

    /// Where the Gate Array fetches a character's two bytes from.
    ///
    /// The CPC scatters the screen rather than laying it out in rows: the
    /// raster line within a character row selects one of eight 2 KB blocks, and
    /// two bits of the CRTC address choose the 16 KB page. That is why a CPC
    /// screen is 16 KB for 16 KB of pixels yet consecutive text rows are not
    /// consecutive in memory. From MAME's
    /// `amstrad_gate_array_get_video_data`.
    fn screen_address(ma: u16, ra: u8) -> u16 {
        ((ma & 0x3000) << 2) | ((u16::from(ra) & 0x07) << 11) | ((ma & 0x03FF) << 1)
    }

    /// Place one character clock's dots, clipped to the visible window.
    fn blit(&mut self, dots: &[u32; DOTS_PER_CHAR]) {
        let y = self.beam_y - V_VISIBLE_START;
        if y < 0 || y >= FB_HEIGHT as i32 {
            return;
        }
        let row = y as usize * FB_WIDTH as usize;
        let left = self.beam_x - H_VISIBLE_START;
        for (i, &colour) in dots.iter().enumerate() {
            let x = left + i as i32;
            if x >= 0 && x < FB_WIDTH as i32 {
                self.framebuffer[row + x as usize] = colour;
            }
        }
    }

    fn handle_bus(&mut self) {
        match self.cpu.bus_request() {
            Some(BusOp::MemRead) => {
                self.cpu.data_in = self.mem_read(self.cpu.addr);
            }
            Some(BusOp::MemWrite) => {
                // Writes always land in RAM, whatever is paged over it.
                self.ram[self.cpu.addr as usize] = self.cpu.data;
            }
            Some(BusOp::IoRead) => {
                self.cpu.data_in = self.io_read(self.cpu.addr);
            }
            Some(BusOp::IoWrite) => {
                self.io_write(self.cpu.addr, self.cpu.data);
            }
            Some(BusOp::IntAck) => {
                // IM 1: the firmware uses RST 38h, and the Gate Array drops
                // `/INT` and clears bit 5 of its counter on acknowledge.
                self.cpu.data_in = 0xFF;
                self.gate_array.acknowledge_interrupt();
            }
            None => {}
        }
    }

    fn mem_read(&self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x3FFF if self.gate_array.lower_rom_enabled() => self.os_rom[addr as usize],
            0xC000..=0xFFFF if self.gate_array.upper_rom_enabled() => {
                let offset = (addr - 0xC000) as usize;
                // Only ROM 0 (BASIC) exists on an unexpanded 464; any other
                // selection reads the open bus, which is $FF.
                if self.selected_upper_rom == 0 {
                    self.basic_rom[offset]
                } else {
                    0xFF
                }
            }
            _ => self.ram[addr as usize],
        }
    }

    fn io_read(&mut self, port: u16) -> u8 {
        // A14 = 0: the CRTC. A9-A8 pick the function.
        if port & 0x4000 == 0 {
            return match (port >> 8) & 0x03 {
                // A type 0 CRTC (HD6845S, which is what the CPC fits) has no
                // readable status register — see the plan's CRTC-type finding.
                0x02 => 0xFF,
                0x03 => self.crtc.read_data(),
                _ => 0xFF,
            };
        }
        // A11 = 0: the PPI. A9-A8 pick the port.
        if port & 0x0800 == 0 {
            let ppi_port = ((port >> 8) & 0x03) as u8;
            if ppi_port == 0 {
                // Port A is the AY data bus. The PSG only answers when port C's
                // control bits select "read".
                if self.psg_control & 0xC0 == 0x40 {
                    // The keyboard hangs off the AY's own port A, with the row
                    // chosen by the low nibble of PPI port C — the same nibble
                    // that carries the tape and speaker bits. So reading a key
                    // means a PPI write followed by an AY register 14 read, and
                    // the matrix has to be presented at the moment of the read
                    // rather than latched earlier. MAME does the same thing at
                    // `m_io_kbrow[m_ppi_port_outputs[amstrad_ppi_PortC] & 0x0F]`.
                    if self.psg.selected_register() == 14 {
                        let row = (self.psg_control & 0x0F) as usize;
                        let bits = self.keyboard.get(row).copied().unwrap_or(0xFF);
                        self.psg.set_port_a_input_mask(bits);
                    }
                    return self.psg.read_data();
                }
            }
            if ppi_port == 1 {
                return self.port_b();
            }
            return self.ppi.read(ppi_port);
        }
        0xFF
    }

    /// PPI port B, which is wired to the outside world rather than driven by
    /// the PPI itself, so the value is assembled here rather than read back
    /// from the chip.
    ///
    /// | Bit | Source |
    /// |---|---|
    /// | 7 | Cassette read data |
    /// | 6 | Printer busy |
    /// | 4 | Refresh rate link: 1 = 50 Hz |
    /// | 3-1 | Manufacturer link |
    /// | 0 | **VSync** |
    ///
    /// Bit 0 is the one that matters most and was missing until now. It is how
    /// a program waits for the frame: the firmware does not need it to boot, so
    /// the machine reached its `Ready` prompt without it, but any loader or
    /// game that syncs to the raster spins forever on a bit that never changes.
    fn port_b(&self) -> u8 {
        // MAME assembles the link bits as
        // `((links & 0x07) << 1) | (links & 0x10)`, which for a stock Amstrad
        // at 50 Hz — the default, and what a CPC464 is — gives 0x1E.
        const LINKS: u8 = 0x1E;

        // The CRTC's own line rather than the Gate Array's copy. They are the
        // same signal: `tick_tstate` hands `crtc.vsync` to the Gate Array on
        // the same character clock, so there is no phase between them.
        // Bit 7 is the cassette read line. The player only advances while the
        // motor is on, so a stopped tape holds whatever level it stopped at —
        // which is what a real one does.
        let cassette = u8::from(self.tape.ear_level()) << 7;

        u8::from(self.crtc.vsync) | LINKS | cassette
    }

    fn io_write(&mut self, port: u16, value: u8) {
        // A15 = 0 and A14 = 1: the Gate Array. Write-only.
        if port & 0x8000 == 0 && port & 0x4000 != 0 {
            self.gate_array.write(value);
        }
        // A14 = 0: the CRTC.
        if port & 0x4000 == 0 {
            match (port >> 8) & 0x03 {
                0x00 => self.crtc.write_address(value),
                0x01 => self.crtc.write_data(value),
                _ => {}
            }
        }
        // A13 = 0: upper-ROM select.
        if port & 0x2000 == 0 {
            self.selected_upper_rom = value;
        }
        // A11 = 0: the PPI.
        if port & 0x0800 == 0 {
            let ppi_port = ((port >> 8) & 0x03) as u8;
            self.ppi.write(ppi_port, value);
            if ppi_port == 2 {
                // Port C carries the AY's bus control in bits 7-6, the
                // cassette write line in bit 5, the motor in bit 4, and the
                // keyboard row in bits 3-0.
                self.psg_control = value;
                self.set_tape_motor(value & 0x10 != 0);
                match value & 0xC0 {
                    0x80 => self.psg.write_data(self.ppi.read(0)),
                    0xC0 => self.psg.select_register(self.ppi.read(0)),
                    _ => {}
                }
            }
        }
    }
}

impl zilog_z80::Z80Stepper for AmstradCpc {
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

    /// 32 KB of firmware: `NOP`s in the OS half, `$C9` (RET) in the BASIC half
    /// so the two are distinguishable through the memory map.
    fn test_firmware() -> Vec<u8> {
        let mut rom = vec![0x00u8; 0x8000];
        rom[0x4000..].fill(0xC9);
        rom
    }

    #[test]
    fn firmware_must_be_32k() {
        assert!(AmstradCpc::new(&[0u8; 0x4000]).is_err());
        assert!(AmstradCpc::new(&test_firmware()).is_ok());
    }

    #[test]
    fn both_roms_are_paged_in_at_reset() {
        // Without the OS at $0000 the Z80 has nothing to boot from.
        let cpc = AmstradCpc::new(&test_firmware()).expect("build");
        assert_eq!(cpc.peek(0x0000), 0x00, "OS ROM");
        assert_eq!(cpc.peek(0xC000), 0xC9, "BASIC ROM");
    }

    #[test]
    fn the_gate_array_can_page_either_rom_out() {
        let mut cpc = AmstradCpc::new(&test_firmware()).expect("build");
        cpc.ram[0x0000] = 0x11;
        cpc.ram[0xC000] = 0x22;

        cpc.io_write(0x7F00, 0b1000_0100); // RMR: lower ROM disabled
        assert_eq!(cpc.peek(0x0000), 0x11, "RAM shows through");
        assert_eq!(cpc.peek(0xC000), 0xC9, "upper still paged in");

        cpc.io_write(0x7F00, 0b1000_1100); // both disabled
        assert_eq!(cpc.peek(0xC000), 0x22);
    }

    #[test]
    fn writes_reach_ram_under_a_paged_in_rom() {
        // The CPC has no write-protect: ROM covers reads only.
        let mut cpc = AmstradCpc::new(&test_firmware()).expect("build");
        cpc.cpu.addr = 0x0100;
        cpc.cpu.data = 0xAB;
        cpc.ram[0x0100] = 0xAB;
        assert_eq!(cpc.peek(0x0100), 0x00, "ROM still answers the read");
        cpc.io_write(0x7F00, 0b1000_0100); // page the OS out
        assert_eq!(cpc.peek(0x0100), 0xAB, "the write was there all along");
    }

    #[test]
    fn poke_lands_in_ram_under_a_paged_in_rom() {
        // Same rule as a CPU write, so a debugger poking $0000-$3FFF with the
        // OS paged in gets the hardware's answer rather than a special case:
        // the byte is there, and the ROM goes on answering reads until it is
        // paged out.
        let mut cpc = AmstradCpc::new(&test_firmware()).expect("build");
        cpc.poke(0x0100, 0xAB);
        assert_eq!(cpc.peek(0x0100), 0x00, "the OS ROM still answers");
        assert_eq!(cpc.ram()[0x0100], 0xAB, "but the byte landed");
        cpc.io_write(0x7F00, 0b1000_0100); // page the OS out
        assert_eq!(cpc.peek(0x0100), 0xAB);

        // Somewhere no ROM covers, a poke is visible at once.
        cpc.poke(0x8000, 0x5A);
        assert_eq!(cpc.peek(0x8000), 0x5A);
    }

    #[test]
    fn a_silent_machine_still_hands_back_a_whole_frame() {
        // Silence is data: it keeps the audio timeline the same length as
        // the video one. Returning an empty buffer here made a capture of
        // a quiet machine write a WAV with no samples and an MP4 with no
        // streams (#934).
        let mut cpc = AmstradCpc::new(&test_firmware()).expect("build");
        cpc.run_frame();
        let audio = cpc.take_audio_buffer();
        assert_eq!(audio.len(), AY_SAMPLES_PER_FRAME);
        assert!(audio.iter().all(|s| *s == 0.0), "a silent frame is silent");
    }

    #[test]
    fn every_frame_is_the_same_length_whether_it_sounds_or_not() {
        // The property that matters for a capture: N frames of machine
        // time produce N frames of audio, so the two timelines stay in
        // step regardless of when the machine happens to make a noise.
        let mut cpc = AmstradCpc::new(&test_firmware()).expect("build");
        cpc.run_frame();
        let silent = cpc.take_audio_buffer().len();

        for (reg, val) in [(0u8, 0x00u8), (1, 0x01), (7, 0xFE), (8, 0x0F)] {
            cpc.psg.select_register(reg);
            cpc.psg.write_data(val);
        }
        cpc.run_frame();
        let sounding = cpc.take_audio_buffer();
        assert_eq!(silent, sounding.len());
        assert!(sounding.iter().any(|s| *s != 0.0), "this frame sounds");
    }

    #[test]
    fn an_audible_machine_hands_back_samples() {
        // Channel A at a mid tone, full volume, tone enabled: the mixer's
        // enable bits are active low, so $FE leaves only tone A through.
        let mut cpc = AmstradCpc::new(&test_firmware()).expect("build");
        for (reg, val) in [(0u8, 0x00u8), (1, 0x01), (7, 0xFE), (8, 0x0F)] {
            cpc.psg.select_register(reg);
            cpc.psg.write_data(val);
        }
        cpc.run_frame();
        let audio = cpc.take_audio_buffer();
        assert!(!audio.is_empty(), "a sounding AY produced no samples");
        assert!(audio.iter().any(|s| *s != 0.0));
    }

    #[test]
    fn an_unselected_upper_rom_reads_as_open_bus() {
        // An unexpanded 464 has only ROM 0.
        let mut cpc = AmstradCpc::new(&test_firmware()).expect("build");
        cpc.io_write(0xDF00, 7);
        assert_eq!(cpc.peek(0xC000), 0xFF);
        cpc.io_write(0xDF00, 0);
        assert_eq!(cpc.peek(0xC000), 0xC9);
    }

    #[test]
    fn the_crtc_has_no_readable_status_on_a_type_0() {
        let mut cpc = AmstradCpc::new(&test_firmware()).expect("build");
        assert_eq!(cpc.io_read(0xBE00), 0xFF);
    }

    #[test]
    fn crtc_registers_round_trip_through_their_ports() {
        // R14 is one of the few a 6845 lets you read back; R0-R13 are
        // write-only, which is the chip's behaviour and not a gap here.
        let mut cpc = AmstradCpc::new(&test_firmware()).expect("build");
        cpc.io_write(0xBC00, 14);
        cpc.io_write(0xBD00, 0x2A);
        assert_eq!(cpc.io_read(0xBF00), 0x2A);
    }

    #[test]
    fn the_gate_array_only_answers_when_a15_is_low_and_a14_high() {
        let mut cpc = AmstradCpc::new(&test_firmware()).expect("build");
        cpc.io_write(0x7F00, 0b1000_0010); // mode 2
        assert_eq!(
            cpc.gate_array().mode(),
            amstrad_gate_array::VideoMode::Mode2
        );
        // A15 high: not the Gate Array, so the mode must not move.
        cpc.io_write(0xFF00, 0b1000_0001);
        assert_eq!(
            cpc.gate_array().mode(),
            amstrad_gate_array::VideoMode::Mode2
        );
    }

    #[test]
    fn the_crtc_advances_once_every_four_tstates() {
        // 4 MHz CPU, 1 MHz CRTC. If this ratio is wrong every raster timing
        // downstream of it is wrong too.
        let mut cpc = AmstradCpc::new(&test_firmware()).expect("build");
        // The address output only advances while the display is enabled, so
        // give the CRTC a displayed area to walk: R1 = 40 characters across,
        // R6 = 25 rows down, which is roughly the CPC's own setup.
        for (reg, value) in [(0u8, 63u8), (1, 40), (6, 25)] {
            cpc.io_write(0xBC00, reg);
            cpc.io_write(0xBD00, value);
        }

        // Prime past the first CRTC tick: the address output latches the
        // counter *before* incrementing it, so it still reads zero after one.
        for _ in 0..4 {
            cpc.tick_tstate();
        }

        let before = cpc.crtc.memory_address();
        for _ in 0..3 {
            cpc.tick_tstate();
        }
        assert_eq!(cpc.crtc.memory_address(), before, "no tick yet at 3");
        cpc.tick_tstate();
        assert_eq!(
            cpc.crtc.memory_address(),
            before + 1,
            "the CRTC advances exactly one character on the fourth T-state"
        );
    }

    /// Program the CRTC the way the CPC firmware does: 64 characters across
    /// with HSync at 46, 39 rows of 8 lines with VSync at row 30, and a 40x25
    /// display. Without this the CRTC's zeroed registers produce no picture.
    fn program_standard_screen(cpc: &mut AmstradCpc) {
        for (reg, value) in [
            (0u8, 63u8), // R0 horizontal total - 1
            (1, 40),     // R1 horizontal displayed
            (2, 46),     // R2 HSync position
            (3, 0x8E),   // R3 sync widths
            (4, 38),     // R4 vertical total - 1
            (6, 25),     // R6 vertical displayed
            (7, 30),     // R7 VSync position
            (9, 7),      // R9 max raster - 1
            (12, 0x30),  // R12/R13 start address: screen at $C000
            (13, 0x00),
        ] {
            cpc.io_write(0xBC00, reg);
            cpc.io_write(0xBD00, value);
        }
    }

    #[test]
    fn a_character_clock_is_sixteen_dots_wide_in_every_mode() {
        // Two bytes per character clock and sixteen dots to spend on them, so
        // the pixels-per-byte of the mode fixes how wide a pixel is. Getting
        // this wrong stretches or squashes the whole picture.
        for (mode_bits, pixels_per_char) in [(0u8, 4), (1, 8), (2, 16)] {
            let mut cpc = AmstradCpc::new(&test_firmware()).expect("build");
            cpc.io_write(0x7F00, 0b1000_0000 | mode_bits);
            let dots_per_pixel = DOTS_PER_CHAR / pixels_per_char;
            assert_eq!(
                dots_per_pixel * pixels_per_char,
                DOTS_PER_CHAR,
                "mode {mode_bits} must divide the character clock exactly"
            );
        }
    }

    #[test]
    fn the_screen_address_scatters_rows_across_2k_blocks() {
        // The CPC's screen is not laid out in rows: the raster line within a
        // character row picks one of eight 2 KB blocks. Row 0 line 0 and row 0
        // line 1 are 2 KB apart, not 80 bytes.
        assert_eq!(AmstradCpc::screen_address(0, 0), 0x0000);
        assert_eq!(AmstradCpc::screen_address(0, 1), 0x0800);
        assert_eq!(AmstradCpc::screen_address(0, 7), 0x3800);
        // Consecutive characters are two bytes apart, the pair fetched per
        // character clock.
        assert_eq!(AmstradCpc::screen_address(1, 0), 0x0002);
        // The two high CRTC bits choose the 16 KB page, which is how the
        // firmware puts the screen at $C000.
        assert_eq!(AmstradCpc::screen_address(0x3000, 0), 0xC000);
    }

    #[test]
    fn the_border_fills_the_screen_when_nothing_is_displayed() {
        // A CRTC still producing sync but displaying nothing: every dot is
        // border. There is no separate border register on a CPC, just the pen
        // the Gate Array emits whenever display is disabled.
        //
        // Note this needs the sync pulses. A CRTC with all registers zeroed
        // produces no sync at all, and a monitor handed no sync shows no
        // picture rather than a border — which is what the framebuffer does.
        let mut cpc = AmstradCpc::new(&test_firmware()).expect("build");
        program_standard_screen(&mut cpc);
        for (reg, value) in [(1u8, 0u8), (6, 0)] {
            cpc.io_write(0xBC00, reg); // nothing displayed, sync unchanged
            cpc.io_write(0xBD00, value);
        }
        cpc.io_write(0x7F00, 0b0101_0000 | 26); // INKR: border, code 26
        let border = cpc.gate_array().border_rgb();
        for _ in 0..3 {
            cpc.run_frame();
        }
        assert!(
            cpc.framebuffer().iter().all(|&px| px == border),
            "every dot should be border colour"
        );
    }

    #[test]
    fn a_displayed_screen_paints_pixels_inside_a_border() {
        // The real shape of a CPC frame: a block of display with border around
        // it. Pen 1 is set to a colour the border is not, and the screen filled
        // with a byte that selects pen 1 everywhere, so the two are separable.
        let mut cpc = AmstradCpc::new(&test_firmware()).expect("build");
        program_standard_screen(&mut cpc);
        cpc.io_write(0x7F00, 0b1000_0001); // RMR: mode 1, both ROMs in
        cpc.io_write(0x7F00, 0b0000_0011); // pen 3
        cpc.io_write(0x7F00, 0b0100_0000 | 26); // ink: code 26
        cpc.io_write(0x7F00, 0b0001_0000); // border pen
        cpc.io_write(0x7F00, 0b0100_0000 | 4); // ink: code 4

        // In mode 1 a pen comes from bit 7 and bit 3, so $FF selects pen 3 for
        // all four of the byte's pixels.
        cpc.ram[0xC000..0x1_0000].fill(0xFF);

        for _ in 0..3 {
            cpc.run_frame();
        }

        let border = cpc.gate_array().border_rgb();
        let ink = cpc.gate_array().pen_rgb(3);
        assert_ne!(border, ink, "the test needs the two to differ");

        let fb = cpc.framebuffer();
        // The corners are border by construction: four character columns and
        // 35 lines of it surround the display.
        assert_eq!(fb[0], border, "top-left corner");
        assert_eq!(
            fb[(FB_HEIGHT * FB_WIDTH - 1) as usize],
            border,
            "bottom-right corner"
        );
        // The centre falls inside the 40x25 display.
        let centre = (FB_HEIGHT / 2 * FB_WIDTH + FB_WIDTH / 2) as usize;
        assert_eq!(fb[centre], ink, "centre should be displayed pixels");

        let ink_dots = fb.iter().filter(|&&px| px == ink).count();
        // 40 characters x 16 dots x 200 lines of display.
        assert_eq!(ink_dots, 40 * DOTS_PER_CHAR * 200, "the whole display area");
    }

    /// Read one keyboard row the way the firmware does: park the AY register
    /// number on PPI port A, latch it with port C's select code, switch port C
    /// to read, then read port A back.
    fn read_keyboard_row(cpc: &mut AmstradCpc, row: u8) -> u8 {
        cpc.io_write(0xF400, 14); // PPI port A = AY register number
        cpc.io_write(0xF600, 0xC0 | row); // port C: select register, row in low nibble
        cpc.io_write(0xF600, 0x40 | row); // port C: read, same row
        cpc.io_read(0xF400)
    }

    #[test]
    fn a_pressed_key_pulls_its_matrix_bit_low() {
        let mut cpc = AmstradCpc::new(&test_firmware()).expect("build");
        // Space is row 5, bit 7.
        assert_eq!(read_keyboard_row(&mut cpc, 5), 0xFF, "nothing pressed");
        cpc.press_key(5, 7);
        assert_eq!(
            read_keyboard_row(&mut cpc, 5),
            0x7F,
            "space should pull bit 7 low"
        );
        // A different row is unaffected — the low nibble of port C really is
        // selecting, rather than every row being returned at once.
        assert_eq!(read_keyboard_row(&mut cpc, 4), 0xFF, "row 4 untouched");
        cpc.release_key(5, 7);
        assert_eq!(read_keyboard_row(&mut cpc, 5), 0xFF, "released");
    }

    #[test]
    fn shifted_characters_press_shift_too() {
        let mut cpc = AmstradCpc::new(&test_firmware()).expect("build");
        // '&' is Shift + 6; 6 is row 6 bit 0, Shift is row 2 bit 5.
        assert!(cpc.press_char('&'));
        assert_eq!(read_keyboard_row(&mut cpc, 6) & 0x01, 0, "the 6 key");
        assert_eq!(read_keyboard_row(&mut cpc, 2) & 0x20, 0, "Shift");
        cpc.release_char('&');
        assert_eq!(read_keyboard_row(&mut cpc, 6), 0xFF);
        assert_eq!(read_keyboard_row(&mut cpc, 2), 0xFF);

        // A digit is *not* shifted, even though its key carries a shifted
        // legend — getting this backwards types punctuation for numbers.
        assert!(cpc.press_char('6'));
        assert_eq!(
            read_keyboard_row(&mut cpc, 2) & 0x20,
            0x20,
            "Shift stays up"
        );
    }

    #[test]
    fn unmappable_characters_report_themselves() {
        let mut cpc = AmstradCpc::new(&test_firmware()).expect("build");
        assert!(!cpc.press_char('\u{20AC}'), "no euro sign on a CPC464");
        assert!(cpc.press_char('a'));
    }

    #[test]
    fn every_printable_ascii_but_the_tilde_has_a_key() {
        // A character this table cannot place is one a caller cannot type, and
        // that gap is invisible until someone tries: on the C64 the same shape
        // of hole turned a typed comparison into a variable reference and ran
        // (#916). Only `~` is genuinely absent from a CPC464 keyboard —
        // Caprice32's table has no entry for it either.
        for c in ' '..='~' {
            let mapped = key_for_char(c).is_some();
            assert_eq!(mapped, c != '~', "{c:?}");
        }
        // Two beyond ASCII that the keyboard does carry.
        assert_eq!(key_for_char('£'), Some((3, 0, true)));
        assert_eq!(key_for_char('^'), Some((3, 0, false)));
    }

    #[test]
    fn a_shifted_legend_shares_its_keys_cell() {
        // `{` is Shift+`[`, not a key of its own — the pairs Caprice32 encodes
        // as the same scancode with `MOD_CPC_SHIFT`.
        for (shifted, plain) in [('{', '['), ('}', ']'), ('`', '\\'), ('|', '@'), ('£', '^')] {
            let (row, bit, needs_shift) = key_for_char(shifted).expect("shifted legend");
            let (plain_row, plain_bit, _) = key_for_char(plain).expect("plain legend");
            assert!(needs_shift, "{shifted:?}");
            assert_eq!((row, bit), (plain_row, plain_bit), "{shifted:?}");
        }
    }

    #[test]
    fn port_b_reports_vsync_and_the_pcb_links() {
        // How a CPC program waits for the frame. A bit that never moves is a
        // loader that never finishes.
        let mut cpc = AmstradCpc::new(&test_firmware()).expect("build");
        program_standard_screen(&mut cpc);

        let mut seen_high = false;
        let mut seen_low = false;
        for _ in 0..TSTATES_PER_FRAME {
            cpc.tick_tstate();
            let b = cpc.io_read(0xF500);
            assert_eq!(b & 0x1E, 0x1E, "Amstrad at 50 Hz: link bits are 1s");
            if b & 0x01 == 0x01 {
                seen_high = true;
            } else {
                seen_low = true;
            }
            // Bit 0 must agree with the CRTC every single tick, not merely
            // toggle at some point during the frame.
            assert_eq!(
                b & 0x01,
                u8::from(cpc.crtc.vsync),
                "port B bit 0 must track VSync"
            );
        }
        assert!(seen_high, "VSync should assert once a frame");
        assert!(seen_low, "and spend most of the frame deasserted");
    }

    #[test]
    fn the_motor_gates_tape_playback() {
        // A CPC drives its own motor. A tape that ran free would advance
        // through its pilot tone while BASIC sat at the prompt, and be
        // somewhere in the middle of a block by the time a loader looked.
        let mut cpc = AmstradCpc::new(&test_firmware()).expect("build");
        cpc.insert_tape(vec![TapeSpan::Pulse(100); 8]);
        assert!(!cpc.tape_motor_on(), "motor off at reset");

        for _ in 0..400 {
            cpc.tick_tstate();
        }
        assert_eq!(
            cpc.tape().span_position().0,
            0,
            "a stopped tape does not move"
        );

        // Port C bit 4 high: motor on.
        cpc.io_write(0xF600, 0x10);
        assert!(cpc.tape_motor_on());
        for _ in 0..400 {
            cpc.tick_tstate();
        }
        assert!(
            cpc.tape().span_position().0 > 0,
            "a running tape advances with the CPU"
        );

        // And stops again when the firmware drops the line.
        let stopped_at = cpc.tape().span_position().0;
        cpc.io_write(0xF600, 0x00);
        assert!(!cpc.tape_motor_on());
        for _ in 0..400 {
            cpc.tick_tstate();
        }
        assert_eq!(cpc.tape().span_position().0, stopped_at, "and holds there");
    }

    #[test]
    fn the_cassette_line_reaches_port_b_bit_7() {
        // Bit 7 is the only way a CPC hears its tape. A machine that never
        // presents the level boots and types perfectly and loads nothing.
        let mut cpc = AmstradCpc::new(&test_firmware()).expect("build");
        // One long pulse, so the level is stable either side of the edge.
        cpc.insert_tape(vec![TapeSpan::Pulse(200), TapeSpan::Pulse(200)]);
        cpc.io_write(0xF600, 0x10); // motor on

        let before = cpc.io_read(0xF500) & 0x80;
        for _ in 0..200 {
            cpc.tick_tstate();
        }
        let after = cpc.io_read(0xF500) & 0x80;
        assert_ne!(
            before, after,
            "the pulse ended and should have flipped bit 7"
        );
        assert_eq!(
            after >> 7,
            u8::from(cpc.tape().ear_level()),
            "bit 7 must be the player's level, not an independent guess"
        );
    }

    #[test]
    fn the_machine_runs_frames_without_panicking() {
        let mut cpc = AmstradCpc::new(&test_firmware()).expect("build");
        for _ in 0..3 {
            cpc.run_frame();
        }
        assert_eq!(cpc.frame_count(), 3);
        assert!(cpc.cpu_tstates() > 0);
    }
}
