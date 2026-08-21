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
mod supercharger;

pub use cartridge::{BankingScheme, Cartridge};
use keypad::Keypad;
use supercharger::ArEffect;

use atari_tia::{ACTIVE_WIDTH, CLOCKS_PER_LINE, HBLANK_CLOCKS, Tia, TiaRegion};
use mos_6502::M6502;
use mos_riot_6532::Riot6532;
use serde::{Deserialize, Serialize};

/// Atari 2600 region.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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

    /// CPU clock (Hz): the TIA colour clock divided by 3. Used to time the DPC
    /// music oscillator.
    fn cpu_clock_hz(self) -> f64 {
        match self {
            Self::Ntsc => 3_579_545.0 / 3.0,
            Self::Pal => 3_546_894.0 / 3.0,
        }
    }
}

/// Atari 2600 machine.
///
/// Fully serialisable for save-states: the 6507, the TIA, the RIOT, the
/// cartridge (bankswitch + on-cart RAM + Supercharger image), and the
/// machine-level bus state all carry live state across a snapshot.
#[derive(Serialize, Deserialize)]
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
    /// The previous bus address, for the distinct-access counter below.
    last_address: u16,
    /// Count of CPU bus accesses whose address differed from the prior one.
    /// Only the Supercharger (AR) scheme consumes it — its control register
    /// times RAM writes by counting *distinct* (address-changed) accesses, so a
    /// run of same-address accesses must not tick it. Matches Stella's
    /// `M6502::distinctAccesses`.
    distinct_accesses: u32,
}

impl Atari2600 {
    /// Create a new Atari 2600 with the given cart ROM and region.
    pub fn new(rom: Vec<u8>, region: Atari2600Region) -> Result<Self, String> {
        let mut cart = Cartridge::from_rom(&rom)?;
        cart.set_dpc_clock_rate(region.cpu_clock_hz());
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
            last_address: 0,
            distinct_accesses: 0,
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
            // Advance the cart's CPU-cycle clock (DPC music oscillator). Time
            // passes during a WSYNC halt, so this is outside the halt guard.
            self.cart.tick();
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
        // The distinct-access counter compares the *full* 16-bit CPU address,
        // before the 13-bit bus mask — matching Stella's `M6502::peek`, which
        // ticks on `address` and only then lets the System fold it to $1FFF.
        self.tick_distinct_access(addr);
        let addr = addr & 0x1FFF;
        // Carts with hotspots outside the $1xxx window (UA) snoop the full bus.
        self.cart.snoop(addr);
        let value = if addr & 0x1000 != 0 {
            if self.cart.scheme() == BankingScheme::Supercharger {
                // AR drives its control register + $1850 fast-load off the
                // distinct-access count and RIOT $80 (the BIOS's load number).
                let ram_80 = self.riot.ram()[0];
                let (byte, effect) = self.cart.ar_read(addr, self.distinct_accesses, ram_80);
                self.apply_ar_effect(effect);
                byte
            } else {
                self.cart.read(addr)
            }
        } else if addr & 0x0080 == 0 {
            // The TIA drives only D6/D7; D0-D5 float and retain the last value
            // on the data bus (merged from `data_bus`, which still holds the
            // pre-read value at this point).
            (self.tia.read(addr as u8) & 0xC0) | (self.data_bus & 0x3F)
        } else {
            self.riot.read(addr)
        };
        self.data_bus = value;
        // FE selects its bank from the value of the access *after* a $01FE
        // touch, so it must see the read result.
        self.cart.snoop_fe(addr, value);
        value
    }

    fn mem_write(&mut self, addr: u16, value: u8) {
        self.tick_distinct_access(addr); // full address, see `mem_read`
        let addr = addr & 0x1FFF;
        self.data_bus = value;
        // Writes can switch banks both by address (UA/0840) and by value (3E).
        self.cart.snoop_write(addr, value);
        self.cart.snoop_fe(addr, value);
        if addr & 0x1000 != 0 {
            if self.cart.scheme() == BankingScheme::Supercharger {
                // AR ignores the data value — the write value is the address.
                let effect = self.cart.ar_write(addr, self.distinct_accesses);
                self.apply_ar_effect(effect);
            } else {
                self.cart.write(addr, value);
            }
        } else if addr & 0x0080 == 0 {
            self.tia.write(addr as u8, value);
        } else {
            self.riot.write(addr, value);
        }
    }

    /// Tick the distinct-access counter when the bus address changes (the AR
    /// scheme's write timing depends on it; a no-op for every other cart).
    fn tick_distinct_access(&mut self, addr: u16) {
        if addr != self.last_address {
            self.distinct_accesses = self.distinct_accesses.wrapping_add(1);
            self.last_address = addr;
        }
    }

    /// Apply an effect the AR cart returned: stage its load parameters into the
    /// RIOT RAM the dummy BIOS reads (`$fe`/`$ff`/`$80`). Other effects are
    /// informational (the 2600 has no dirty-page tracking).
    fn apply_ar_effect(&mut self, effect: ArEffect) {
        if let ArEffect::RamPokes(pokes) = effect {
            let ram = self.riot.ram_mut();
            for (a, v) in pokes {
                ram[(a & 0x7F) as usize] = v;
            }
        }
    }

    /// Full TIA raster (228 × lines): the visible 160-pixel picture preceded
    /// by the 68-clock horizontal-blank margin. Use [`Self::visible_framebuffer_width`]
    /// + [`Self::hblank_clocks`] to crop to the displayable region.
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

    /// Full framebuffer width (228 = 68 HBLANK + 160 visible).
    #[must_use]
    pub fn framebuffer_width(&self) -> u32 {
        self.tia.framebuffer_width()
    }

    /// Width of the displayable picture (160), excluding the HBLANK margin.
    #[must_use]
    pub fn visible_framebuffer_width(&self) -> u32 {
        ACTIVE_WIDTH
    }

    /// Leading horizontal-blank columns in each [`Self::framebuffer`] row (68)
    /// — the always-black left margin to skip when displaying.
    #[must_use]
    pub fn hblank_clocks(&self) -> u32 {
        u32::from(HBLANK_CLOCKS)
    }

    /// Framebuffer height (depends on TIA region).
    #[must_use]
    pub fn framebuffer_height(&self) -> u32 {
        self.tia.framebuffer_height()
    }

    /// Height of the visible window: the scan lines a set displays.
    ///
    /// 240 on NTSC and 288 on PAL, per
    /// `knowledge/decisions/the-framebuffer-is-the-sets-window.md`. These used
    /// to be Stella's TIA base heights, 228 and 274 — that emulator's display
    /// convention rather than a field, and 228 of 240 is the 95% the #1054
    /// audit read.
    #[must_use]
    pub fn visible_framebuffer_height(&self) -> u32 {
        match self.region {
            Atari2600Region::Ntsc => 240,
            Atari2600Region::Pal => 288,
        }
    }

    /// First displayable scanline — the top of the visible window.
    ///
    /// Whatever the frame has above the field, which is the vertical interval
    /// a set blanks: 262 lines less 240 on NTSC, 312 less 288 on PAL. Derived
    /// rather than chosen, the same way the ZX80 anchors its window, and so it
    /// cannot drift away from the height above.
    ///
    /// It used to be Stella's `ystart` — 23 and 32. The NTSC figure was within
    /// a line of this; the PAL one was eight out, and taking 288 lines from
    /// line 32 would run off the end of a 312-line frame.
    #[must_use]
    pub fn visible_first_line(&self) -> u32 {
        u32::from(self.region.lines_per_frame()) - self.visible_framebuffer_height()
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

    /// Save-state must capture LIVE machine state (6507 + TIA + RIOT + cart),
    /// not cold-boot from the ROM. Serialise, advance (so the state differs),
    /// then deserialise the first snapshot and confirm re-serialising it is
    /// byte-identical — every stateful field across the CPU, TIA, RIOT, and
    /// cartridge round-trips, and a poked RIOT-RAM byte survives.
    #[test]
    fn snapshot_round_trips_live_state() {
        let mut sys = Atari2600::new(trap_rom(), Atari2600Region::Ntsc).expect("init");
        sys.run_frame();
        sys.poke(0x0080, 0xA5); // a RIOT work-RAM byte to carry across the snapshot
        sys.run_frame();
        let s1 = postcard::to_allocvec(&sys).expect("encode snapshot");

        sys.run_frame(); // advance past the snapshot point
        let s2 = postcard::to_allocvec(&sys).expect("encode again");
        assert_ne!(s1, s2, "running a frame should change the serialised state");

        let restored: Atari2600 = postcard::from_bytes(&s1).expect("decode snapshot");
        assert_eq!(
            restored.riot().ram()[0],
            0xA5,
            "poked RIOT RAM byte survives the round-trip"
        );
        let s3 = postcard::to_allocvec(&restored).expect("re-encode restored");
        assert_eq!(
            s1, s3,
            "restore should reproduce the snapshot state exactly"
        );
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
    fn fe_cart_switches_banks_from_the_stack_snoop() {
        // 8K FE image: bank 0 = 0xE0, bank 1 = 0xE1, FE signature + reset vector.
        let mut rom = vec![0u8; 8192];
        rom[0..4096].fill(0xE0);
        rom[4096..8192].fill(0xE1);
        rom[0x40..0x45].copy_from_slice(&[0x20, 0xC3, 0xF8, 0xA5, 0x82]); // FE sig
        rom[0x0FFC] = 0x00;
        rom[0x0FFD] = 0x10;
        let mut sys = Atari2600::new(rom, Atari2600Region::Ntsc).expect("FE");

        // Arm with a bus access to $01FE, then a $D0-valued write → bank 1.
        sys.mem_read(0x01FE);
        sys.mem_write(0x0080, 0xD0);
        assert_eq!(sys.mem_read(0x1000), 0xE1, "$01FE then value $D0 → bank 1");

        // Arm again, a $F0-valued write → bank 0.
        sys.mem_read(0x01FE);
        sys.mem_write(0x0080, 0xF0);
        assert_eq!(sys.mem_read(0x1000), 0xE0, "$01FE then value $F0 → bank 0");
    }

    #[test]
    fn ua_cart_switches_banks_via_bus_snoop() {
        // 8K UA image: bank 0 = 0xA0, bank 1 = 0xA1, with a UA signature and a
        // reset vector pointing into the cart.
        let mut rom = vec![0u8; 8192];
        rom[0..4096].fill(0xA0);
        rom[4096..8192].fill(0xA1);
        rom[0x20..0x23].copy_from_slice(&[0x8D, 0x40, 0x02]); // STA $240 → UA sig
        rom[0x0FFC] = 0x00;
        rom[0x0FFD] = 0x10;
        let mut sys = Atari2600::new(rom, Atari2600Region::Ntsc).expect("UA");

        // A bus read of $0240 (outside the cart window) is snooped → bank 1.
        sys.mem_read(0x0240);
        assert_eq!(sys.mem_read(0x1F00), 0xA1, "$0240 read snoop → bank 1");
        sys.mem_read(0x0220);
        assert_eq!(sys.mem_read(0x1F00), 0xA0, "$0220 read snoop → bank 0");

        // The real Funky Fish access is a write (STA $240) — also snooped.
        sys.mem_write(0x0240, 0x00);
        assert_eq!(sys.mem_read(0x1F00), 0xA1, "STA $240 write snoop → bank 1");
    }

    #[test]
    fn jam_opcode_halts_the_cpu() {
        // A 4K cart whose reset vector points at a JAM ($02) stop-code — the
        // shape of a corrupted ROM dump. The CPU must halt (real silicon
        // behaviour), which the UI/query layer surfaces instead of a silent
        // grey screen.
        let mut rom = vec![0u8; 4096];
        rom[0x000] = 0x02; // JAM at $1000
        rom[0x0FFC] = 0x00;
        rom[0x0FFD] = 0x10; // reset vector → $1000
        let mut sys = Atari2600::new(rom, Atari2600Region::Ntsc).expect("init");

        assert!(!sys.cpu().halted, "a fresh CPU is running");
        sys.run_frame();
        assert!(sys.cpu().halted, "executing a JAM opcode halts the CPU");
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
