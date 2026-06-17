//! Atari 2600 (VCS) machine wiring.
//!
//! Fresh-write against the workspace pin-driven bus pattern (RULES.md
//! rule 6). The donor at `Emu198x-Oldest/crates/machine-atari-2600`
//! used the deprecated `emu_core::Bus` callback and could not port
//! directly; this file uses it as a system spec — 6507's 13-bit
//! address decode, master-colour-clock tick model with CPU and RIOT
//! at 1/3 the rate, TIA WSYNC CPU halt, cartridge hotspot bank
//! switching (F8 / F6 / F4) — but the wiring is written against
//! [`mos_6502::M6502`]'s public pin fields.
//!
//! # The Atari 2600 (Video Computer System)
//!
//! The 2600 (Atari, 1977) is the **second-generation cartridge-based
//! home console** that defined the medium. Famously hard to program:
//! the TIA has no framebuffer, so the CPU must "race the beam" and
//! update video registers between scanlines. The chip-level
//! complexity is in `atari-tia`; this crate wires the 6507 to it.
//!
//! - **CPU:** MOS 6507 (a 6502 pin-limited to 13 address lines, so
//!   the effective address space is 8 KB mirrored across the 16-bit
//!   range)
//! - **TIA:** Atari custom video + audio
//! - **RIOT:** MOS 6532 (128 bytes RAM + I/O ports + timer)
//! - **Cart:** 2 KB / 4 KB / 8 KB (F8) / 16 KB (F6) / 32 KB (F4)
//!
//! # Memory decode (post-`addr & 0x1FFF`)
//!
//! - **A12 = 1:** Cartridge ROM at `$1000-$1FFF`
//! - **A12 = 0, A7 = 0:** TIA registers (writes shape the next scanline)
//! - **A12 = 0, A7 = 1, A9 = 0:** RIOT RAM
//! - **A12 = 0, A7 = 1, A9 = 1:** RIOT I/O + timer
//!
//! # Clock model
//!
//! Master clock = TIA colour clock (3.579545 MHz NTSC, 3.546894 MHz
//! PAL). The 6507 + RIOT both tick every 3rd colour clock. TIA's
//! WSYNC line halts the CPU until the next horizontal blank.
//!
//! One scanline = 228 colour clocks = 76 CPU cycles. PAL frames are
//! 312 lines / NTSC 262 lines, but the actual frame is
//! software-controlled — the CPU stops driving VSYNC after as many
//! lines as the game wants.

mod cartridge;
mod keypad;

pub use cartridge::{BankingScheme, Cartridge};
use keypad::Keypad;

use atari_tia::{CLOCKS_PER_LINE, Tia, TiaRegion};
use mos_6502::M6502;
use mos_riot_6532::Riot6532;

/// Atari 2600 region.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Atari2600Region {
    Ntsc,
    Pal,
}

impl Atari2600Region {
    fn tia_region(self) -> TiaRegion {
        match self {
            Self::Ntsc => TiaRegion::Ntsc,
            Self::Pal => TiaRegion::Pal,
        }
    }

    fn lines_per_frame(self) -> u16 {
        match self {
            Self::Ntsc => 262,
            Self::Pal => 312,
        }
    }
}

/// Atari 2600 machine.
pub struct Atari2600 {
    cpu: M6502,
    tia: Tia,
    riot: Riot6532,
    cart: Cartridge,
    /// Master clock = colour clocks since power-on.
    master_clock: u64,
    /// Target colour clocks per frame (lines × 228).
    clocks_per_frame: u64,
    region: Atari2600Region,
    frame_count: u64,
    /// Last value driven on the data bus. The TIA drives only D6/D7 on a read;
    /// the lower bits float and retain whatever was last on the bus, so reads
    /// of TIA registers merge these retained bits into D0-D5.
    data_bus: u8,
    /// Optional keypad controller per jack (`[port 1, port 2]`). When present,
    /// it drives that jack's INPT lines from its scanned matrix each cycle.
    keypad: [Option<Keypad>; 2],
}

impl Atari2600 {
    /// Create a new Atari 2600 with the given cart ROM and region.
    pub fn new(rom: Vec<u8>, region: Atari2600Region) -> Result<Self, String> {
        let cart = Cartridge::from_rom(&rom)?;
        let mut cpu = M6502::new();
        cpu.reset();
        let tia = Tia::new(region.tia_region());
        let riot = Riot6532::new();
        let clocks_per_frame = u64::from(region.lines_per_frame()) * u64::from(CLOCKS_PER_LINE);
        Ok(Self {
            cpu,
            tia,
            riot,
            cart,
            master_clock: 0,
            clocks_per_frame,
            region,
            frame_count: 0,
            data_bus: 0,
            keypad: [None, None],
        })
    }

    /// Run one frame and return colour clocks consumed.
    pub fn run_frame(&mut self) -> u64 {
        let start = self.master_clock;
        let target = start + self.clocks_per_frame;
        while self.master_clock < target {
            self.tick_colour_clock();
        }
        self.frame_count += 1;
        self.master_clock - start
    }

    fn tick_colour_clock(&mut self) {
        self.master_clock += 1;
        self.tia.tick();
        // CPU + RIOT tick every 3rd colour clock.
        if self.master_clock.is_multiple_of(3) {
            // Refresh any keypad's INPT drive from the current SWCHA row scan
            // before the CPU can read it back.
            self.update_keypads();
            if !self.tia.wsync_halt {
                self.cpu.tick();
                if self.cpu.rw {
                    self.cpu.data_in = self.mem_read(self.cpu.addr);
                } else {
                    self.mem_write(self.cpu.addr, self.cpu.data);
                }
            }
            self.riot.tick();
        }
        // 6507 has no IRQ/NMI pins exposed externally.
        self.cpu.irq = false;
        self.cpu.nmi = false;
    }

    fn mem_read(&mut self, addr: u16) -> u8 {
        let addr = addr & 0x1FFF;
        let value = if addr & 0x1000 != 0 {
            self.cart.read(addr)
        } else if addr & 0x0080 == 0 {
            // The TIA drives only D6/D7; D0-D5 float and retain the last value
            // on the data bus (merged from `data_bus`, which still holds the
            // pre-read value at this point).
            (self.tia.read(addr as u8) & 0xC0) | (self.data_bus & 0x3F)
        } else {
            self.riot.read(addr)
        };
        self.data_bus = value;
        value
    }

    fn mem_write(&mut self, addr: u16, value: u8) {
        let addr = addr & 0x1FFF;
        self.data_bus = value;
        if addr & 0x1000 != 0 {
            self.cart.write(addr, value);
        } else if addr & 0x0080 == 0 {
            self.tia.write(addr as u8, value);
        } else {
            self.riot.write(addr, value);
        }
    }

    /// Framebuffer (160 × lines).
    #[must_use]
    pub fn framebuffer(&self) -> &[u32] {
        self.tia.framebuffer()
    }

    /// Drain the TIA's mono audio samples produced since the last call.
    pub fn take_audio_samples(&mut self) -> Vec<f32> {
        self.tia.take_audio_samples()
    }

    /// Native audio sample rate: two TIA samples per scanline at the region's
    /// nominal refresh (NTSC 262×2×60, PAL 312×2×50). The host resamples.
    #[must_use]
    pub fn audio_sample_rate(&self) -> u32 {
        match self.region {
            Atari2600Region::Ntsc => 31_440,
            Atari2600Region::Pal => 31_200,
        }
    }

    /// Framebuffer width (TIA: 160).
    #[must_use]
    pub fn framebuffer_width(&self) -> u32 {
        self.tia.framebuffer_width()
    }

    /// Framebuffer height (depends on TIA region).
    #[must_use]
    pub fn framebuffer_height(&self) -> u32 {
        self.tia.framebuffer_height()
    }

    /// Set RIOT port A input — joystick directions byte.
    pub fn set_joystick_input(&mut self, value: u8) {
        self.riot.set_port_a_input(value);
    }

    /// Set RIOT port B input — console switches byte.
    pub fn set_switch_input(&mut self, value: u8) {
        self.riot.set_port_b_input(value);
    }

    /// Set a paddle position on INPT line `index` (0-3): INPT0/1 are the two
    /// paddles on the left jack, INPT2/3 the right. `value` is the 8-bit pot
    /// position (0 charges fastest, 255 slowest). The TIA reads it as the
    /// capacitor-charge timing on INPT0-3.
    pub fn set_paddle(&mut self, index: u8, value: u8) {
        self.tia.set_paddle(index, value);
    }

    /// Set a joystick fire button (`pressed` = button down). Port 1 (the left
    /// jack) drives INPT4, port 2 (the right jack) INPT5; out-of-range ports
    /// clamp to the valid pair. The TIA applies its latch mode (VBLANK bit 6)
    /// on top of this pin level.
    pub fn set_fire(&mut self, port: u8, pressed: bool) {
        if port == 2 {
            self.tia.set_inpt5(pressed);
        } else {
            self.tia.set_inpt4(pressed);
        }
    }

    /// Set a CBS Booster-Grip extra button. The booster grip is a joystick
    /// with two added buttons wired to the jack's paddle INPT lines: the
    /// *booster* on INPT1 (left jack) / INPT3 (right), the *trigger* on INPT0 /
    /// INPT2. Each connects its line to Vcc when pressed (reads high) and floats
    /// it low when released (per Stella's `Booster.cxx`). `booster` selects the
    /// booster button (`true`) or the trigger (`false`); the stick directions
    /// and main fire button use the ordinary joystick paths.
    pub fn set_booster_button(&mut self, port: u8, booster: bool, pressed: bool) {
        let base = if port == 2 { 2 } else { 0 };
        let line = base + u8::from(booster);
        self.tia.set_inpt_digital(line, Some(pressed));
    }

    /// Set the Sega Genesis / Mega Drive pad's extra **C** button. The
    /// three-button pad reads as an ordinary joystick (directions on SWCHA,
    /// button B on INPT4/INPT5) plus a C button on the jack's INPT1 (left) /
    /// INPT3 (right) line — wired *inverted* relative to the Booster-Grip:
    /// pressed pulls the line to ground (reads low), released ties it to Vcc
    /// (reads high), per Stella's `Genesis.cxx`. (Button A isn't readable on a
    /// stock 2600.)
    pub fn set_genesis_button_c(&mut self, port: u8, pressed: bool) {
        let line = if port == 2 { 3 } else { 1 };
        self.tia.set_inpt_digital(line, Some(!pressed));
    }

    /// Press or release a key on the keypad controller attached to `port`
    /// (1 or 2). Attaching is implicit: the first event on a port installs a
    /// keypad there, which then drives that jack's INPT lines until
    /// [`Self::detach_keypad`]. `row` is 0-3 (top→bottom), `col` 0-2
    /// (left→right); see [`keypad`] for the matrix layout.
    pub fn set_keypad_key(&mut self, port: u8, row: u8, col: u8, pressed: bool) {
        let idx = usize::from(port == 2);
        self.keypad[idx]
            .get_or_insert_with(Keypad::default)
            .set_key(row, col, pressed);
    }

    /// Remove the keypad from `port` (1 or 2), releasing its column lines back
    /// to the paddle pot path.
    pub fn detach_keypad(&mut self, port: u8) {
        let idx = usize::from(port == 2);
        if self.keypad[idx].take().is_some() {
            let base = if idx == 1 { 2 } else { 0 };
            self.tia.set_inpt_digital(base, None);
            self.tia.set_inpt_digital(base + 1, None);
        }
    }

    /// Drive each attached keypad's INPT lines from the live SWCHA row scan.
    fn update_keypads(&mut self) {
        if self.keypad[0].is_none() && self.keypad[1].is_none() {
            return;
        }
        let drive = self.riot.port_a_drive();
        for idx in 0..2 {
            let Some(keypad) = self.keypad[idx] else {
                continue;
            };
            // Port 1's rows ride SWCHA bits 4-7, port 2's bits 0-3.
            let rows = if idx == 1 {
                drive & 0x0F
            } else {
                (drive >> 4) & 0x0F
            };
            let gnd = keypad.columns_grounded(rows);
            // Columns 0/1 → analog INPT lines (0/1 left jack, 2/3 right): a
            // grounded column reads low (bit 7 = 0), otherwise high.
            let base = if idx == 1 { 2 } else { 0 };
            self.tia.set_inpt_digital(base, Some(!gnd[0]));
            self.tia.set_inpt_digital(base + 1, Some(!gnd[1]));
            // Column 2 → the digital fire line (INPT4 left / INPT5 right): a
            // grounded column pulls it low, exactly like a fire press.
            if idx == 1 {
                self.tia.set_inpt5(gnd[2]);
            } else {
                self.tia.set_inpt4(gnd[2]);
            }
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

    /// TIA reference.
    #[must_use]
    pub fn tia(&self) -> &Tia {
        &self.tia
    }

    /// RIOT reference.
    #[must_use]
    pub fn riot(&self) -> &Riot6532 {
        &self.riot
    }

    /// Master clock (colour clocks since power-on).
    #[must_use]
    pub fn master_clock(&self) -> u64 {
        self.master_clock
    }

    /// Frame count.
    #[must_use]
    pub fn frame_count(&self) -> u64 {
        self.frame_count
    }

    /// Region.
    #[must_use]
    pub fn region(&self) -> Atari2600Region {
        self.region
    }
}

impl Atari2600 {
    /// Read one byte with no side effects: cartridge ROM and the 128 bytes
    /// of RIOT RAM; `$FF` for TIA / RIOT-I/O (read side effects).
    #[must_use]
    pub fn peek(&self, addr: u16) -> u8 {
        let a = addr & 0x1FFF;
        if a & 0x1000 != 0 {
            self.cart.peek(a)
        } else if a & 0x0200 == 0 && a & 0x0080 != 0 {
            self.riot.ram()[(a & 0x7F) as usize]
        } else {
            0xFF
        }
    }

    /// Write one byte through the bus (RAM accepts it; ROM ignores it).
    pub fn poke(&mut self, addr: u16, value: u8) {
        self.mem_write(addr, value);
    }

    /// Run exactly one whole 6507 instruction, returning the colour clocks
    /// it consumed. A safety cap prevents an unbounded spin.
    pub fn step_instruction(&mut self) -> u64 {
        let mut ticks = 0u64;
        while self.cpu.instruction_complete() && ticks < 4096 {
            self.tick_colour_clock();
            ticks += 1;
        }
        while !self.cpu.instruction_complete() && ticks < 4096 {
            self.tick_colour_clock();
            ticks += 1;
        }
        ticks
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn trap_rom() -> Vec<u8> {
        // 4 KB cart. Reset vector at $FFFC-$FFFD (high byte $1F lives
        // in the cart). JMP self at $1000 → 4C 00 F0 → in 4K cart at
        // offset 0 we want $4C $00 $F0. Actually 6502 reset reads
        // from $FFFC = cart offset $0FFC. So put $00 / $10 there so
        // CPU jumps to $1000 (start of cart).
        let mut rom = vec![0xEA_u8; 4096];
        rom[0x0000] = 0x4C;
        rom[0x0001] = 0x00;
        rom[0x0002] = 0x10;
        rom[0x0FFC] = 0x00;
        rom[0x0FFD] = 0x10;
        rom
    }

    #[test]
    fn tia_reads_float_their_low_bits_to_the_data_bus() {
        let mut sys = Atari2600::new(trap_rom(), Atari2600Region::Ntsc).expect("init");

        // A write leaves its value on the data bus. INPT4 (default released)
        // drives only D7; D0-D5 read back the retained bus bits.
        sys.mem_write(0x06, 0xFF); // COLUP0 write → bus = 0xFF
        assert_eq!(
            sys.mem_read(0x0C),
            0xBF,
            "INPT4: D7 driven high, D0-D5 float to bus 0x3F"
        );

        // Drive the bus low and the floating bits follow.
        sys.mem_write(0x06, 0x00); // bus = 0x00
        assert_eq!(
            sys.mem_read(0x0C),
            0x80,
            "floating bits now low, D7 still driven"
        );
    }

    #[test]
    fn set_fire_drives_the_right_inpt_line() {
        let mut sys = Atari2600::new(trap_rom(), Atari2600Region::Ntsc).expect("init");
        // Default released: INPT4/INPT5 bit 7 high.
        assert_eq!(sys.tia().read(0x0C) & 0x80, 0x80);
        assert_eq!(sys.tia().read(0x0D) & 0x80, 0x80);

        // Port 1 → INPT4 ($0C), port 2 → INPT5 ($0D); pressed pulls bit 7 low.
        sys.set_fire(1, true);
        assert_eq!(sys.tia().read(0x0C) & 0x80, 0, "p1 fire → INPT4 low");
        assert_eq!(sys.tia().read(0x0D) & 0x80, 0x80, "p2 untouched");
        sys.set_fire(2, true);
        assert_eq!(sys.tia().read(0x0D) & 0x80, 0, "p2 fire → INPT5 low");

        sys.set_fire(1, false);
        assert_eq!(sys.tia().read(0x0C) & 0x80, 0x80, "p1 release → INPT4 high");
    }

    #[test]
    fn booster_grip_buttons_drive_the_paddle_inpt_lines() {
        let mut sys = Atari2600::new(trap_rom(), Atari2600Region::Ntsc).expect("init");

        // Port 1: booster → INPT1 ($09), trigger → INPT0 ($08). Pressed = Vcc
        // = bit 7 high; released floats low.
        sys.set_booster_button(1, true, true);
        assert_eq!(sys.tia().read(0x09) & 0x80, 0x80, "booster → INPT1 high");
        sys.set_booster_button(1, false, true);
        assert_eq!(sys.tia().read(0x08) & 0x80, 0x80, "trigger → INPT0 high");

        // Port 2 maps to INPT3 ($0B) / INPT2 ($0A).
        sys.set_booster_button(2, true, true);
        assert_eq!(sys.tia().read(0x0B) & 0x80, 0x80, "p2 booster → INPT3 high");

        sys.set_booster_button(1, true, false);
        assert_eq!(
            sys.tia().read(0x09) & 0x80,
            0,
            "booster release → INPT1 low"
        );
    }

    #[test]
    fn genesis_button_c_is_active_low_on_the_inpt_line() {
        let mut sys = Atari2600::new(trap_rom(), Atari2600Region::Ntsc).expect("init");

        // Inverted vs the booster: released = Vcc = high, pressed = ground = low.
        sys.set_genesis_button_c(1, false);
        assert_eq!(sys.tia().read(0x09) & 0x80, 0x80, "C released → INPT1 high");
        sys.set_genesis_button_c(1, true);
        assert_eq!(sys.tia().read(0x09) & 0x80, 0, "C pressed → INPT1 low");

        // Port 2 lands on INPT3 ($0B).
        sys.set_genesis_button_c(2, true);
        assert_eq!(sys.tia().read(0x0B) & 0x80, 0, "p2 C pressed → INPT3 low");
    }

    #[test]
    fn keypad_grounds_its_column_when_its_row_is_scanned() {
        let mut sys = Atari2600::new(trap_rom(), Atari2600Region::Ntsc).expect("init");

        // Keypad on port 1; press "6" (row 1, col 2 → reads on INPT4).
        sys.set_keypad_key(1, 1, 2, true);

        // Make SWCHA's high nibble outputs (SWACNT $281) and scan row 1 by
        // driving its pin (bit 5) low, the other rows high.
        sys.poke(0x281, 0xF0); // port 1 nibble = outputs
        sys.poke(0x280, 0xD0); // bits 4,6,7 high, bit 5 (row 1) low
        sys.step_instruction(); // a tick refreshes the keypad INPT drive

        // Column 2 grounds → INPT4 reads low; columns 0/1 stay high.
        assert_eq!(
            sys.tia().read(0x0C) & 0x80,
            0,
            "row 1 scanned → '6' grounds INPT4"
        );
        assert_eq!(sys.tia().read(0x08) & 0x80, 0x80, "col 0 high");
        assert_eq!(sys.tia().read(0x09) & 0x80, 0x80, "col 1 high");

        // Scan a different row (row 0 low instead): "6" is not on it, so its
        // column no longer grounds.
        sys.poke(0x280, 0xE0); // bit 4 (row 0) low, bit 5 high
        sys.step_instruction();
        assert_eq!(
            sys.tia().read(0x0C) & 0x80,
            0x80,
            "row 0 scanned → INPT4 released"
        );

        // Detaching hands INPT0/1 back to the pot path (and clears the override).
        sys.detach_keypad(1);
        sys.step_instruction();
        assert_eq!(
            sys.tia().read(0x08) & 0x80,
            0,
            "released line falls to the cold pot"
        );
    }

    #[test]
    fn audio_registers_drive_non_silent_output_end_to_end() {
        let mut sys = Atari2600::new(trap_rom(), Atari2600Region::Ntsc).expect("init");

        // Silent by default: a frame of the JMP-self kernel emits samples, all
        // zero (volume 0).
        sys.run_frame();
        let silent = sys.take_audio_samples();
        assert!(!silent.is_empty(), "a frame produces audio samples");
        assert!(silent.iter().all(|&s| s == 0.0), "no volume → silence");

        // Program a pure tone through the bus (AUDC0/AUDF0/AUDV0), run a frame,
        // and confirm the channel now sounds.
        sys.mem_write(0x15, 0x04); // AUDC0 = pure tone
        sys.mem_write(0x17, 0x03); // AUDF0
        sys.mem_write(0x19, 0x0F); // AUDV0 = full
        sys.run_frame();
        let tone = sys.take_audio_samples();
        let max = tone.iter().cloned().fold(0.0_f32, f32::max);
        assert!(max > 0.0, "a programmed tone produces audible output");
    }

    #[test]
    fn frame_advances_master_clock_and_count() {
        let mut sys = Atari2600::new(trap_rom(), Atari2600Region::Ntsc).expect("init");
        let clocks = sys.run_frame();
        assert!(clocks > 0);
        assert_eq!(sys.frame_count(), 1);
    }

    #[test]
    fn pal_has_more_clocks_per_frame_than_ntsc() {
        let mut ntsc = Atari2600::new(trap_rom(), Atari2600Region::Ntsc).expect("init");
        let mut pal = Atari2600::new(trap_rom(), Atari2600Region::Pal).expect("init");
        assert!(pal.run_frame() > ntsc.run_frame());
    }

    #[test]
    fn many_frames_complete_without_panic() {
        let mut sys = Atari2600::new(trap_rom(), Atari2600Region::Ntsc).expect("init");
        for _ in 0..30 {
            sys.run_frame();
        }
        assert_eq!(sys.frame_count(), 30);
    }

    #[test]
    fn memory_decode_routes_tia_riot_cart() {
        let mut sys = Atari2600::new(trap_rom(), Atari2600Region::Ntsc).expect("init");
        // Cart space (A12 = 1).
        assert_eq!(sys.mem_read(0x1000), 0x4C);
        // RIOT RAM (A7=1, A9=0) write/read at $80.
        sys.mem_write(0x0080, 0x42);
        assert_eq!(sys.mem_read(0x0080), 0x42);
        // TIA write at $0009 (COLUBK) — doesn't panic.
        sys.mem_write(0x0009, 0x9A);
    }

    #[test]
    fn address_mirroring_13_bit() {
        let mut sys = Atari2600::new(trap_rom(), Atari2600Region::Ntsc).expect("init");
        sys.mem_write(0x0080, 0x77);
        // $2080 & $1FFF = $0080.
        assert_eq!(sys.mem_read(0x2080), 0x77);
    }

    #[test]
    fn rejects_invalid_rom_size() {
        let bad = vec![0u8; 5000];
        assert!(Atari2600::new(bad, Atari2600Region::Ntsc).is_err());
    }
}
