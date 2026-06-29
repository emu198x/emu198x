//! Acorn Atom (1980) — 6502 + MC6847 text-mode VDG + 6520 PIA.
//!
//! Fresh-write against the workspace pin-driven bus pattern (RULES.md
//! rule 6). The donor at `Emu198x-Oldest/crates/machine-acorn-atom/`
//! used the deprecated `emu_core::Bus` callback; the wiring here goes
//! through [`mos_6502::M6502`]'s public pin fields.
//!
//! # The Acorn Atom
//!
//! Acorn's £120 self-build (1980) — designed by Sophie Wilson and
//! Steve Furber, the team that would design the BBC Micro the
//! following year. Used as the platform for several Acornsoft
//! titles and the first commercial release of Elite (an
//! Atom-targeted demo).
//!
//! - **CPU:** MOS 6502 at 1 MHz
//! - **VDG:** Motorola MC6847 text mode (32 × 16 chars, 8 × 12 cell)
//!   — Atom-specific variant with an embedded 64-glyph character
//!   ROM (see [`vdg`])
//! - **PPI:** Intel INS8255 at `$B000-$B003` (keyboard + cassette)
//! - **RAM:** 2.5 KB base, expandable to 12 KB
//! - **Video RAM:** 1 KB at `$8000-$83FF` (mirrored to `$9FFF`)
//! - **ROM:** 24 KB combined — BASIC (split `$A000` + `$C000`),
//!   FP at `$B004-$BFFF`, OS at `$D000-$FFFF`
//!
//! # I/O — the INS8255 PPI at `$B000-$B003`
//!
//! Per the Atom Technical Manual (Issue 2), the 8255 drives the keyboard
//! through a 4-to-10 line decoder and reads the columns back:
//!
//! - **Port A** (`$B000`): low nibble = the binary keyboard row index
//!   (0-9) into the decoder; high nibble = the MC6847 mode bits.
//! - **Port B** (`$B001`): the six keyboard column lines, active low.
//! - **Port C** (`$B002`): bits 0-3 output (cassette out / speaker / colour
//!   set CSS); bits 4-7 input — PC4 = the 2.4 kHz reference tone, PC5 =
//!   cassette DATA in, PC7 = the VDG vertical-blank (field-sync) the MOS
//!   times its keyboard scan off (Atom Technical Manual §25.5).
//!
//! Clock model: one master tick = one 6502 cycle (1 MHz). VDG ticks
//! at the same rate. One PAL frame ≈ 71,136 ticks (228 × 312).
//!
//! Scope of this port: text mode only. Graphics modes 1-4 (semi-
//! graphics) and mode 5 (256 × 192 dot graphics) are stubbed in the
//! VDG and tracked as follow-ups in `docs/status/outstanding-work.md`.

pub mod input;
mod keyboard;
pub mod vdg;

pub use input::AtomKey;
pub use keyboard::KeyboardState;
pub use vdg::{FB_HEIGHT, FB_WIDTH, Mc6847};

use common_acorn_cassette::{CassetteReceiver, TapePulse};
use intel_8255::Ppi8255;
use mos_6502::M6502;
use serde::{Deserialize, Serialize};
use serde_big_array::BigArray;

/// Nanoseconds per master tick — the Atom runs at 1 MHz, so one 6502 cycle.
const NS_PER_TICK: u64 = 1000;

/// One PAL field in master ticks. The UK Atom's MC6847 runs PAL (312-line, see
/// [`vdg`]), so the field rate is 50 Hz — one field every 20,000 ticks at 1 MHz.
/// (The Technical Manual quotes the 6847's nominal 60 Hz figure; the UK machine
/// is PAL. Tying this to the VDG's own frame counter is deferred to the
/// graphics-mode timing work, #367 — the VDG line clock is not yet 1:1 here.)
const FIELD_TICKS: u64 = 20_000;
/// 8255 PC7 carries the 6847 field-sync (FS̄): HIGH through the 192 active
/// display lines, LOW during vertical blanking/flyback (Atom Technical Manual /
/// *Atomic Theory and Practice* §25.5 — "60 Hz sync signal, low during
/// flyback"). 192 of the 312 PAL lines are active.
const FIELD_ACTIVE_TICKS: u64 = FIELD_TICKS * 192 / 312;
/// PC4 reference tone: 2.4 kHz divided from the 4 MHz crystal (÷1667). At a
/// 1 MHz master clock one full cycle is ~416 ticks; the divider output is a 50%
/// square wave (Technical Manual §62; *Atomic Theory and Practice* §19, §25.5).
const TONE_PERIOD_TICKS: u64 = 1_000_000 / 2_400;

/// Acorn Atom machine.
#[derive(Serialize, Deserialize)]
pub struct AcornAtom {
    cpu: M6502,
    ram: Vec<u8>,
    ram_size: usize,
    #[serde(with = "BigArray")]
    video_ram: [u8; 1024],
    rom: Vec<u8>,
    /// Intel 8255 PPI: port A drives the keyboard column (PA0-3) and the
    /// MC6847 mode bits (PA4-7); port B reads the six keyboard row lines;
    /// port C handles cassette / speaker / 2.4 kHz.
    ppi: Ppi8255,
    vdg: Mc6847,
    keyboard: KeyboardState,
    master_clock: u64,
    frame_count: u64,
    /// Cassette waveform. The Atom has no serial receiver and no motor relay —
    /// the COS bit-bangs the raw line level on PPI PC5 in software — so the tape
    /// runs whenever one is loaded and the machine samples [`CassetteReceiver::level`].
    cassette: CassetteReceiver,
}

impl AcornAtom {
    /// Create a new Atom. `rom` is the combined 24 KB BASIC + FP + OS
    /// blob (BASIC1 at offset 0, FP at $1000, BASIC2 at $2000, OS at
    /// $3000). `ram_size` is 2560-12288 bytes.
    pub fn new(rom: Vec<u8>, ram_size: usize) -> Self {
        // Run the 6502 reset sequence so the first fetch comes from the MOS
        // reset vector ($FFFC); without it the CPU powers on at PC=$0000 and
        // never cold-starts, leaving the uninitialised character grid on screen.
        let mut cpu = M6502::new();
        cpu.reset();
        Self {
            cpu,
            ram: vec![0; ram_size],
            ram_size,
            video_ram: [0; 1024],
            rom,
            ppi: Ppi8255::new(),
            vdg: Mc6847::new(),
            keyboard: KeyboardState::new(),
            master_clock: 0,
            frame_count: 0,
            cassette: CassetteReceiver::new(),
        }
    }

    /// Loads a cassette tape from a decoded UEF pulse stream, rewound to the
    /// start. The Atom has no motor relay, so the tape plays whenever loaded.
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

    pub fn run_frame(&mut self) -> u64 {
        let start = self.master_clock;
        for _ in 0..200_000 {
            self.tick();
            if self.vdg.take_frame_complete() {
                break;
            }
        }
        self.frame_count += 1;
        self.master_clock - start
    }

    /// Present the keyboard rows for the column the MOS has driven on 8255
    /// port A (PA0-3, a binary 0-9 column index) on port B, active low.
    fn update_keyboard(&mut self) {
        let column = (self.ppi.port_a & 0x0F) as usize;
        self.ppi.port_b = !self.keyboard.read_row(column);
    }

    fn tick(&mut self) {
        self.master_clock += 1;
        // Advance the tape so PC5 reflects the current line level when the COS
        // samples it this tick. No motor relay — it runs while loaded.
        self.cassette.advance(NS_PER_TICK, &mut |_| {});
        let video_ram = &self.video_ram;
        self.vdg.tick(|addr| video_ram[(addr & 0x03FF) as usize]);
        // The Atom keyboard is polled, not interrupt-driven; the 8255 has no
        // interrupt line in Mode 0 and the donor models no other IRQ source.
        self.cpu.irq = false;
        self.cpu.tick();
        if self.cpu.rw {
            self.cpu.data_in = self.mem_read(self.cpu.addr);
        } else {
            self.mem_write(self.cpu.addr, self.cpu.data);
        }
    }

    fn mem_read(&mut self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x7FFF => {
                if (addr as usize) < self.ram_size {
                    self.ram[addr as usize]
                } else {
                    0xFF
                }
            }
            0x8000..=0x9FFF => self.video_ram[(addr & 0x03FF) as usize],
            0xA000..=0xAFFF => {
                let offset = (addr - 0xA000) as usize;
                self.rom.get(offset).copied().unwrap_or(0xFF)
            }
            0xB000..=0xB003 => {
                self.update_keyboard();
                // Port C inputs (Atom Technical Manual §25.5 / Atomulator
                // `8255.c`): PC7 = the field-sync FS̄ (high during active video,
                // low during flyback — the MOS times its keyboard scan off it),
                // PC5 = cassette DATA input, PC4 = the 2.4 kHz reference tone.
                let field_active = (self.master_clock % FIELD_TICKS) < FIELD_ACTIVE_TICKS;
                let tone_2400 = (self.master_clock % TONE_PERIOD_TICKS) < TONE_PERIOD_TICKS / 2;
                let cassette = self.cassette.is_loaded() && self.cassette.level();
                let inputs = (u8::from(field_active) << 7)
                    | (u8::from(cassette) << 5)
                    | (u8::from(tone_2400) << 4);
                self.ppi.port_c = (self.ppi.port_c & 0x0F) | inputs;
                self.ppi.read((addr - 0xB000) as u8)
            }
            0xB004..=0xBFFF => {
                let offset = 0x1000 + (addr - 0xB000) as usize;
                self.rom.get(offset).copied().unwrap_or(0xFF)
            }
            0xC000..=0xCFFF => {
                let offset = 0x2000 + (addr - 0xC000) as usize;
                self.rom.get(offset).copied().unwrap_or(0xFF)
            }
            0xD000..=0xFFFF => {
                let offset = 0x3000 + (addr - 0xD000) as usize;
                self.rom.get(offset).copied().unwrap_or(0xFF)
            }
        }
    }

    fn mem_write(&mut self, addr: u16, value: u8) {
        match addr {
            0x0000..=0x7FFF if (addr as usize) < self.ram_size => {
                self.ram[addr as usize] = value;
            }
            0x8000..=0x9FFF => {
                self.video_ram[(addr & 0x03FF) as usize] = value;
            }
            0xB000 => {
                // Port A: low nibble selects the keyboard column, high nibble
                // carries the MC6847 mode bits — latch both.
                self.ppi.write(0, value);
                self.vdg.control = value;
            }
            0xB001..=0xB003 => {
                self.ppi.write((addr - 0xB000) as u8, value);
                // CSS (MC6847 colour-set select) is on 8255 PC3, set by a port-C
                // write or a BSR control-word write ($B003) — refresh the VDG
                // from port C after either path (#369).
                self.vdg.css = self.ppi.port_c & 0x08 != 0;
            }
            _ => {}
        }
    }

    #[must_use]
    pub fn framebuffer(&self) -> &[u32] {
        self.vdg.framebuffer()
    }

    #[must_use]
    pub fn framebuffer_width(&self) -> u32 {
        self.vdg.framebuffer_width()
    }

    #[must_use]
    pub fn framebuffer_height(&self) -> u32 {
        self.vdg.framebuffer_height()
    }

    pub fn press_key(&mut self, key: AtomKey) {
        self.set_key(key, true);
    }

    pub fn release_key(&mut self, key: AtomKey) {
        self.set_key(key, false);
    }

    fn set_key(&mut self, key: AtomKey, pressed: bool) {
        match key {
            AtomKey::Shift => self.keyboard.set_shift(pressed),
            AtomKey::Ctrl => self.keyboard.set_ctrl(pressed),
            other => {
                if let Some((row, col)) = other.matrix() {
                    self.keyboard.set_key(row, col, pressed);
                }
            }
        }
    }

    pub fn release_all_keys(&mut self) {
        self.keyboard.release_all();
    }

    #[must_use]
    pub fn peek_memory(&self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x7FFF if (addr as usize) < self.ram_size => self.ram[addr as usize],
            0x8000..=0x9FFF => self.video_ram[(addr & 0x03FF) as usize],
            0xB000 => self.vdg.control,
            0xA000..=0xAFFF => self
                .rom
                .get((addr - 0xA000) as usize)
                .copied()
                .unwrap_or(0xFF),
            0xB004..=0xBFFF => self
                .rom
                .get(0x1000 + (addr - 0xB000) as usize)
                .copied()
                .unwrap_or(0xFF),
            0xC000..=0xCFFF => self
                .rom
                .get(0x2000 + (addr - 0xC000) as usize)
                .copied()
                .unwrap_or(0xFF),
            0xD000..=0xFFFF => self
                .rom
                .get(0x3000 + (addr - 0xD000) as usize)
                .copied()
                .unwrap_or(0xFF),
            _ => 0xFF,
        }
    }

    #[must_use]
    pub fn cpu(&self) -> &M6502 {
        &self.cpu
    }

    pub fn cpu_mut(&mut self) -> &mut M6502 {
        &mut self.cpu
    }

    #[must_use]
    pub fn master_clock(&self) -> u64 {
        self.master_clock
    }

    #[must_use]
    pub fn frame_count(&self) -> u64 {
        self.frame_count
    }
}

impl AcornAtom {
    /// Read one byte with no side effects (alias of `peek_memory`).
    #[must_use]
    pub fn peek(&self, addr: u16) -> u8 {
        self.peek_memory(addr)
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
            self.tick();
            ticks += 1;
        }
        while !self.cpu.instruction_complete() && ticks < 4096 {
            self.tick();
            ticks += 1;
        }
        ticks
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `*` (the COS command prefix) is SHIFT + the `:` key, probed against the
    /// real MOS — not a dedicated key. Confirm SHIFT+Colon echoes `*` (0x2A).
    #[test]
    #[ignore = "needs the real Atom ROM"]
    fn star_is_shift_colon_on_the_real_mos() {
        let path = std::path::PathBuf::from(std::env::var("HOME").expect("HOME"))
            .join(".emu198x/roms/acorn-atom/atom.rom");
        let rom = std::fs::read(&path).expect("real Atom ROM");
        let mut sys = AcornAtom::new(rom, 0x0A00);
        for _ in 0..120 {
            sys.run_frame();
        }
        // The screen cell just after the prompt's last '>' (display code 0x3E).
        let prompt = (0x8000u16..0x8200)
            .rev()
            .find(|&a| sys.peek(a) & 0x3f == 0x3e)
            .map(|a| a + 1)
            .expect("prompt on screen");

        sys.press_key(AtomKey::Shift);
        sys.press_key(AtomKey::Colon);
        sys.run_frame();
        sys.release_key(AtomKey::Colon);
        sys.release_key(AtomKey::Shift);
        for _ in 0..4 {
            sys.run_frame();
        }
        assert_eq!(sys.peek(prompt), 0x2a, "SHIFT+Colon should type '*'");
    }

    /// DELETE — the (4,1) key — removes the char to the left and steps the
    /// cursor back, probed against the real MOS.
    #[test]
    #[ignore = "needs the real Atom ROM"]
    fn delete_removes_the_previous_char() {
        let path = std::path::PathBuf::from(std::env::var("HOME").expect("HOME"))
            .join(".emu198x/roms/acorn-atom/atom.rom");
        let rom = std::fs::read(&path).expect("real Atom ROM");
        let mut sys = AcornAtom::new(rom, 0x0A00);
        for _ in 0..120 {
            sys.run_frame();
        }
        let prompt = (0x8000u16..0x8200)
            .rev()
            .find(|&a| sys.peek(a) & 0x3f == 0x3e)
            .expect("prompt");
        // Type "AB", then DELETE should remove the 'B'.
        for k in [AtomKey::A, AtomKey::B] {
            sys.press_key(k);
            sys.run_frame();
            sys.release_key(k);
            for _ in 0..3 {
                sys.run_frame();
            }
        }
        assert_eq!(sys.peek(prompt + 2), 0x02, "'B' (display 0x02) typed");
        sys.press_key(AtomKey::Delete);
        sys.run_frame();
        sys.release_key(AtomKey::Delete);
        for _ in 0..3 {
            sys.run_frame();
        }
        assert_ne!(sys.peek(prompt + 2), 0x02, "DELETE removed the 'B'");
    }

    /// The two bidirectional cursor keys move the edit cursor on the real MOS:
    /// `CursorUpDown` up / SHIFT-down, `CursorLeftRight` right / SHIFT-left.
    #[test]
    #[ignore = "needs the real Atom ROM"]
    fn cursor_keys_move_the_cursor() {
        let path = std::path::PathBuf::from(std::env::var("HOME").expect("HOME"))
            .join(".emu198x/roms/acorn-atom/atom.rom");
        let rom = std::fs::read(&path).expect("real Atom ROM");

        // Cursor marker = the inverse cell (>= 0x80), as (row, col) on screen.
        let cursor = |sys: &AcornAtom| -> (u16, u16) {
            let a = (0x8000u16..0x8200)
                .find(|&a| sys.peek(a) >= 0x80)
                .expect("a cursor marker");
            ((a - 0x8000) / 32, (a - 0x8000) % 32)
        };
        // Press a key (optionally with SHIFT) from a fresh prompt with "123"
        // typed, returning the cursor delta (drow, dcol).
        let delta = |key: AtomKey, shift: bool| -> (i32, i32) {
            let mut sys = AcornAtom::new(rom.clone(), 0x0A00);
            for _ in 0..120 {
                sys.run_frame();
            }
            for k in [AtomKey::Num1, AtomKey::Num2, AtomKey::Num3] {
                sys.press_key(k);
                sys.run_frame();
                sys.release_key(k);
                for _ in 0..3 {
                    sys.run_frame();
                }
            }
            let (r0, c0) = cursor(&sys);
            if shift {
                sys.press_key(AtomKey::Shift);
            }
            sys.press_key(key);
            sys.run_frame();
            sys.release_key(key);
            if shift {
                sys.release_key(AtomKey::Shift);
            }
            for _ in 0..3 {
                sys.run_frame();
            }
            let (r1, c1) = cursor(&sys);
            (i32::from(r1) - i32::from(r0), i32::from(c1) - i32::from(c0))
        };

        assert_eq!(delta(AtomKey::CursorUpDown, false), (-1, 0), "up");
        assert_eq!(delta(AtomKey::CursorUpDown, true), (1, 0), "shift = down");
        assert_eq!(delta(AtomKey::CursorLeftRight, false), (0, 1), "right");
        assert_eq!(
            delta(AtomKey::CursorLeftRight, true),
            (0, -1),
            "shift = left"
        );
    }

    fn trap_rom() -> Vec<u8> {
        // 24 KB combined ROM. OS reset vector at $FFFC → $D000.
        let mut rom = vec![0xEAu8; 0x6000];
        rom[0x3FFC] = 0x00;
        rom[0x3FFD] = 0xD0;
        rom[0x3000] = 0x4C;
        rom[0x3001] = 0x00;
        rom[0x3002] = 0xD0;
        rom
    }

    /// Save-state must capture LIVE machine state (6502 + PPI + MC6847 VDG +
    /// RAM/video RAM), not cold-boot from ROM. Serialise, advance (so the state
    /// differs), then deserialise the first snapshot and confirm re-serialising
    /// it is byte-identical — every stateful field round-trips, including the
    /// 1 KB video RAM and the VDG's last-scanned buffer.
    #[test]
    fn snapshot_round_trips_live_state() {
        let mut sys = AcornAtom::new(trap_rom(), 0x0A00);
        sys.run_frame();
        sys.poke(0x0100, 0xA5); // a low-RAM byte to carry across the snapshot
        sys.run_frame();
        let s1 = postcard::to_allocvec(&sys).expect("encode snapshot");

        sys.run_frame(); // advance past the snapshot point
        let s2 = postcard::to_allocvec(&sys).expect("encode again");
        assert_ne!(s1, s2, "running a frame should change the serialised state");

        let restored: AcornAtom = postcard::from_bytes(&s1).expect("decode snapshot");
        let s3 = postcard::to_allocvec(&restored).expect("re-encode restored");
        assert_eq!(
            s1, s3,
            "restore should reproduce the snapshot state exactly"
        );
        assert_eq!(restored.peek(0x0100), 0xA5, "poked RAM byte must survive");
    }

    #[test]
    fn frame_advances_count() {
        let mut sys = AcornAtom::new(trap_rom(), 0x0A00);
        let _ = sys.run_frame();
        assert_eq!(sys.frame_count(), 1);
    }

    #[test]
    fn ram_round_trips() {
        let mut sys = AcornAtom::new(trap_rom(), 0x0A00);
        sys.mem_write(0x0100, 0x42);
        assert_eq!(sys.mem_read(0x0100), 0x42);
    }

    #[test]
    fn video_ram_round_trips_and_mirrors() {
        let mut sys = AcornAtom::new(trap_rom(), 0x0A00);
        sys.mem_write(0x8000, 0xAB);
        assert_eq!(sys.mem_read(0x8000), 0xAB);
        assert_eq!(sys.mem_read(0x8400), 0xAB);
    }

    #[test]
    fn vdg_control_register_round_trips() {
        let mut sys = AcornAtom::new(trap_rom(), 0x0A00);
        sys.mem_write(0xB000, 0x80);
        assert_eq!(sys.mem_read(0xB000), 0x80);
    }

    #[test]
    fn rom_writes_ignored() {
        let mut sys = AcornAtom::new(trap_rom(), 0x0A00);
        sys.mem_write(0xF000, 0xFF);
        assert_eq!(sys.mem_read(0xF000), 0xEA);
    }

    #[test]
    fn shift_and_ctrl_register_on_port_b() {
        let mut sys = AcornAtom::new(trap_rom(), 0x0A00);
        // Drive a column and read port B ($B001). Idle = all rows high (active-low).
        sys.mem_write(0xB000, 0x01); // select column 1 (also clears the gfx nibble)
        let idle = sys.mem_read(0xB001);
        assert_eq!(idle & 0xC0, 0xC0, "SHIFT/CTRL idle high (not pressed)");

        // SHIFT pulls port B bit 7 low; CTRL pulls bit 6 low — both regardless
        // of the selected column.
        sys.press_key(AtomKey::Shift);
        sys.mem_write(0xB000, 0x01);
        assert_eq!(sys.mem_read(0xB001) & 0x80, 0, "SHIFT held = PB7 low");
        assert_eq!(sys.mem_read(0xB001) & 0x40, 0x40, "CTRL still high");

        sys.press_key(AtomKey::Ctrl);
        sys.mem_write(0xB000, 0x05); // a different column
        assert_eq!(
            sys.mem_read(0xB001) & 0xC0,
            0,
            "SHIFT+CTRL both low, any column"
        );

        sys.release_key(AtomKey::Shift);
        sys.release_key(AtomKey::Ctrl);
        sys.mem_write(0xB000, 0x01);
        assert_eq!(
            sys.mem_read(0xB001) & 0xC0,
            0xC0,
            "released = both high again"
        );
    }

    #[test]
    fn field_sync_is_high_during_active_video_low_in_flyback() {
        let mut sys = AcornAtom::new(trap_rom(), 0x0A00);
        // Sample PC7 across one full field (FS̄ depends only on master_clock).
        let mut high = 0u64;
        for _ in 0..FIELD_TICKS {
            sys.tick();
            if sys.mem_read(0xB002) & 0x80 != 0 {
                high += 1;
            }
        }
        // FS̄ is high through the 192 active lines of the 312-line PAL field
        // (~61.5%) and low during the 120-line flyback (#373).
        let ratio = high as f64 / FIELD_TICKS as f64;
        assert!(
            (ratio - 192.0 / 312.0).abs() < 0.02,
            "field-sync duty was {ratio}, expected ~{}",
            192.0 / 312.0
        );
    }

    #[test]
    fn css_comes_from_port_c_not_the_keyboard_column() {
        let mut sys = AcornAtom::new(trap_rom(), 0x0A00);
        // Driving keyboard column 0x0F (PA3 high) must NOT enable CSS — PA3 is a
        // keyboard scan line, not the MC6847 colour-set select (#369).
        sys.mem_write(0xB000, 0x0F);
        assert!(!sys.vdg.css, "keyboard column PA3 must not drive CSS");

        // A direct port-C write with bit 3 set enables CSS.
        sys.mem_write(0xB002, 0x08);
        assert!(sys.vdg.css, "PC3 high enables CSS");

        // Clearing PC3 disables it again.
        sys.mem_write(0xB002, 0x00);
        assert!(!sys.vdg.css);

        // The 8255 BSR control word ($B003) sets PC3 too: bit index 3, set bit.
        sys.mem_write(0xB003, 0x07);
        assert!(sys.vdg.css, "BSR set of PC3 enables CSS");
    }

    #[test]
    fn cassette_level_drives_pc5() {
        let mut sys = AcornAtom::new(trap_rom(), 0x0A00);
        // One long cycle: the line is low for 1 ms, then high for 1 ms. The tape
        // advances one 1 µs tick per cycle, so 1500 ticks lands in the high half.
        sys.insert_tape(vec![TapePulse::Cycles {
            half_period_ns: 1_000_000,
            count: 1,
        }]);
        assert!(sys.tape_loaded());

        for _ in 0..500 {
            sys.tick();
        }
        assert_eq!(
            sys.mem_read(0xB002) & 0x20,
            0,
            "PC5 cassette-data low in the first half of the cycle"
        );

        for _ in 0..1000 {
            sys.tick();
        }
        assert_eq!(
            sys.mem_read(0xB002) & 0x20,
            0x20,
            "PC5 cassette-data high in the second half of the cycle"
        );
    }
}
