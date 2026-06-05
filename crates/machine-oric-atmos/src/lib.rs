//! Oric-1 / Oric Atmos machine wiring.
//!
//! Fresh-write against the workspace pin-driven bus pattern (RULES.md
//! rule 6). The donor at `Emu198x-Oldest/crates/machine-oric-atmos`
//! used the deprecated `emu_core::Bus` callback and could not port
//! directly; this file uses it as a system spec — VIA at `$0300-$03FF`,
//! AY-via-VIA routing through CA2 (BDIR) and CB2 (BC1), 8×8 keyboard
//! scan via VIA port B column select / port A row read, TEXT + HIRES
//! ULA video modes with serial attributes — but the wiring is written
//! against [`mos_6502::M6502`]'s public pin fields and the
//! `mos-via-6522` chip crate's pin-truth surface.
//!
//! # The Oric
//!
//! The Oric-1 (1983) and Atmos (1984) are 6502-based home computers
//! from Tangerine / Oric Products International. Particularly strong
//! in France, where Loriciels and ESAT made the Atmos the de-facto
//! French home-computer machine in the mid-1980s. Famous for the
//! distinctive **AY-via-VIA wiring**: the AY-3-8910 isn't directly
//! addressable — VIA port A carries the AY data bus, CA2 is wired
//! to AY's BDIR pin, and CB2 to AY's BC1 pin. Software sets PCR
//! into one of four (BDIR, BC1) modes to drive the AY.
//!
//! - **CPU:** 6502A @ 1 MHz
//! - **VIA:** MOS 6522 at `$0300-$030F` (mirrored across `$0300-$03FF`)
//! - **PSG:** AY-3-8910 @ 1 MHz — via our `gi-ay-3-8912` crate
//! - **ULA:** custom — TEXT (40×28) + HIRES (200 lines + 3 text rows)
//! - **RAM:** 48 KB (Oric-1) or 64 KB (Atmos)
//! - **ROM:** 16 KB BASIC + OS at `$C000-$FFFF`
//!
//! # Memory map
//!
//! | Range         | Contents                                       |
//! |---------------|------------------------------------------------|
//! | `$0000-$02FF` | Zero page + system + stack                     |
//! | `$0300-$03FF` | VIA 6522 (every 16 bytes mirror the registers) |
//! | `$0400-$BFFF` | RAM (47.75 KB; 16 KB further on Atmos under ROM)|
//! | `$A000-$BFFF` | HIRES bitmap (top of RAM, doubles as RAM)      |
//! | `$BB80-$BFFF` | TEXT screen + HIRES top 3 text rows ($BF68)    |
//! | `$B400-$BBFF` | Character generator (read by ULA)              |
//! | `$C000-$FFFF` | 16 KB BASIC + OS ROM                           |
//!
//! On the Atmos the full 64 KB RAM lives beneath; ROM remains the
//! visible image at `$C000-$FFFF` for reads, and writes always land
//! in RAM (so the OS can move the BASIC working area underneath).
//!
//! # AY-via-VIA scheme
//!
//! VIA port A = AY data bus. CA2 → AY BDIR; CB2 → AY BC1.
//!
//! | BDIR | BC1 | Operation                              |
//! |------|-----|----------------------------------------|
//! | 0    | 0   | inactive                               |
//! | 0    | 1   | read selected AY register              |
//! | 1    | 0   | write to selected AY register          |
//! | 1    | 1   | latch register address (port A)        |
//!
//! Software programs PCR to put CA2 / CB2 into "fixed high" output
//! mode (PCR & 0x0E == 0x0E for CA2; PCR & 0xE0 == 0xE0 for CB2),
//! sets port A to the desired data byte, then drops the mode back
//! to high-impedance for the next operation.
//!
//! # Keyboard
//!
//! 8×8 matrix, active-low. VIA port B bits 0-2 select the column
//! (0-7); the scan routine drives one row low on VIA port A; the sense
//! returns on VIA PB3, which reads high when the pressed key sits at the
//! selected column and the driven-low row (`(keyboard[col] | row_mask)
//! != 0xFF`). Port A is shared with the AY data bus but carries the row
//! mask when the AY is not being addressed.
//!
//! # Display rendering
//!
//! Donor's render-end-of-frame pattern: every `run_frame` rebuilds
//! the framebuffer from RAM ($BB80 TEXT, $A000 HIRES, $B400
//! charset) using **serial attributes** — bytes `$00-$1F` in the
//! screen image change the ink/paper colour for the rest of the
//! line rather than rendering a glyph. Same 8-colour 3-bit RGB
//! palette as the Acorn / BBC family.

use gi_ay_3_8912::Ay3_8912;
use mos_6502::M6502;
use mos_via_6522::Via6522;

/// Framebuffer width (240 pixels = 40 columns × 6 pixels per character).
pub const FB_WIDTH: u32 = 240;
/// Framebuffer height (224 pixels = 28 rows × 8 lines).
pub const FB_HEIGHT: u32 = 224;

const CPU_CLOCK_HZ: u32 = 1_000_000;
const LINES_PER_FRAME: u32 = 312;
const CYCLES_PER_LINE: u32 = 64;
const TICKS_PER_FRAME: u64 = (LINES_PER_FRAME * CYCLES_PER_LINE) as u64;

const AY_SAMPLE_RATE: u32 = 48_000;
const AY_SAMPLES_PER_FRAME: usize = 1024;

const ORIC_PALETTE: [u32; 8] = [
    0xFF00_0000, // 0: Black
    0xFFFF_0000, // 1: Red
    0xFF00_FF00, // 2: Green
    0xFFFF_FF00, // 3: Yellow
    0xFF00_00FF, // 4: Blue
    0xFFFF_00FF, // 5: Magenta
    0xFF00_FFFF, // 6: Cyan
    0xFFFF_FFFF, // 7: White
];

/// Oric model variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OricModel {
    /// Oric-1 (1983): 48 KB RAM.
    Oric1,
    /// Oric Atmos (1984): 64 KB RAM, improved keyboard.
    Atmos,
}

/// One IJK joystick's switch state (active-high here; inverted into the
/// active-low port-A mask when read).
#[derive(Clone, Copy, Debug, Default)]
struct JoyState {
    up: bool,
    down: bool,
    left: bool,
    right: bool,
    fire: bool,
}

/// Oric machine.
pub struct OricAtmos {
    cpu: M6502,
    via: Via6522,
    psg: Ay3_8912,
    ram: [u8; 65536],
    rom: Vec<u8>,
    /// 8×8 keyboard matrix, active-low.
    keyboard: [u8; 8],
    /// IJK joystick interface state, `[left port, right port]`. The IJK is the
    /// de-facto Oric joystick: both sticks are read on VIA port A, selected by
    /// port A bits 6-7 and gated by PB4. See [`OricAtmos::update_ijk_joystick`].
    joystick: [JoyState; 2],
    framebuffer: Vec<u32>,
    ram_size: usize,
    model: OricModel,
    cpu_cycles: u64,
    frame_count: u64,
}

impl OricAtmos {
    /// Create a new Oric with the given 16 KB BASIC + OS ROM and
    /// model variant.
    #[must_use]
    pub fn new(rom: Vec<u8>, model: OricModel) -> Self {
        let ram_size = match model {
            OricModel::Oric1 => 48 * 1024,
            OricModel::Atmos => 64 * 1024,
        };
        let mut cpu = M6502::new();
        cpu.reset();
        Self {
            cpu,
            via: Via6522::new(),
            psg: Ay3_8912::new(CPU_CLOCK_HZ, AY_SAMPLE_RATE, AY_SAMPLES_PER_FRAME),
            ram: [0; 65536],
            rom,
            keyboard: [0xFF; 8],
            joystick: [JoyState::default(); 2],
            framebuffer: vec![ORIC_PALETTE[0]; (FB_WIDTH * FB_HEIGHT) as usize],
            ram_size,
            model,
            cpu_cycles: 0,
            frame_count: 0,
        }
    }

    /// Run one PAL frame.
    pub fn run_frame(&mut self) -> u64 {
        let target = self.cpu_cycles + TICKS_PER_FRAME;
        while self.cpu_cycles < target {
            self.tick_cpu_cycle();
        }
        self.render_display();
        self.frame_count += 1;
        TICKS_PER_FRAME
    }

    fn tick_cpu_cycle(&mut self) {
        self.cpu.tick();
        if self.cpu.rw {
            self.cpu.data_in = self.mem_read(self.cpu.addr);
        } else {
            self.mem_write(self.cpu.addr, self.cpu.data);
        }
        self.via.tick();
        self.psg.tick();
        self.cpu.irq = self.via.irq;
        self.cpu_cycles += 1;
    }

    fn mem_read(&mut self, addr: u16) -> u8 {
        match addr {
            0x0300..=0x03FF => {
                // The IJK joystick drives the VIA port-A input lines; refresh
                // them before the register read resolves.
                self.update_ijk_joystick();
                self.via.read((addr & 0x0F) as u8)
            }
            0xC000..=0xFFFF => self
                .rom
                .get((addr - 0xC000) as usize)
                .copied()
                .unwrap_or(0xFF),
            _ => {
                let idx = addr as usize;
                if idx < self.ram_size {
                    self.ram[idx]
                } else {
                    0xFF
                }
            }
        }
    }

    fn mem_write(&mut self, addr: u16, value: u8) {
        match addr {
            0x0300..=0x03FF => {
                let reg = (addr & 0x0F) as u8;
                self.via.write(reg, value);
                // PCR (reg $0C) or port A (reg $01/$0F) writes can
                // drive the AY bus.
                if reg == 0x0C || reg == 0x01 || reg == 0x0F {
                    self.process_ay_bus();
                }
                // Re-sense the keyboard onto PB3: a port B write ($00)
                // changes the column, and an AY-bus write changes the
                // port-A row mask — either alters which key is sensed.
                self.scan_keyboard();
            }
            _ => {
                let idx = addr as usize;
                if idx < self.ram_size {
                    self.ram[idx] = value;
                }
            }
        }
    }

    /// Inspect VIA control state and drive the AY accordingly.
    fn process_ay_bus(&mut self) {
        let pcr = self.via.peek(0x0C);
        // CA2 → AY BDIR; CB2 → AY BC1. The Oric uses PCR's "fixed
        // high output" mode bit pattern (0b111) to drive these high.
        let bdir = (pcr & 0x0E) == 0x0E;
        let bc1 = (pcr & 0xE0) == 0xE0;
        let port_a = self.via.ora();
        match (bdir, bc1) {
            (true, true) => {
                self.psg.select_register(port_a);
            }
            (true, false) => {
                self.psg.write_data(port_a);
            }
            (false, true) => {
                let ay_data = self.psg.read_data();
                self.via.pa_in = ay_data;
            }
            (false, false) => {}
        }
    }

    /// Sense the keyboard onto VIA PB3.
    ///
    /// The column is selected by VIA port B bits 0-2; the AY-3-8910 port
    /// A drives the row mask (the scanning routine pulls one row low).
    /// PB3 reads high when a pressed key (active-low in `keyboard[col]`)
    /// sits at the selected column and a driven-low row — i.e. when
    /// `keyboard[col] | row_mask` has any zero bit. (MAME oric `write_pb3`.)
    fn scan_keyboard(&mut self) {
        let col = (self.via.orb() & 0x07) as usize;
        // The scan routine drives the row mask on VIA port A directly
        // (one row pulled low at a time); port A is shared with the AY
        // bus but carries the row mask when the AY is not being addressed.
        let row_mask = self.via.ora();
        if (self.keyboard[col] | row_mask) != 0xFF {
            self.via.pb_in |= 0x08;
        } else {
            self.via.pb_in &= !0x08;
        }
    }

    /// Drive the IJK joystick interface onto the VIA port-A input lines.
    ///
    /// The IJK — the de-facto Oric Atmos joystick interface — hangs off the
    /// printer port and presents the sticks on VIA port A. Modelled from
    /// Oricutron (`joystick.c`):
    ///
    /// * **Enable.** The interface only drives port A while PB4 is configured
    ///   as an output and held **low** (so `port_b_drive_state` bit 4 is 0).
    ///   Otherwise port A is left to the AY bus / keyboard.
    /// * **Select.** Port A bits 6-7 (CPU outputs) pick the stick: bit 6 reads
    ///   the left port, bit 7 the right; both high selects neither.
    /// * **Layout.** Directions and fire come back active low on bits 0-4
    ///   (bit 0 right, 1 left, 2 fire, 3 down, 4 up); bit 5 is always low — the
    ///   IJK-present marker the detection routines look for.
    fn update_ijk_joystick(&mut self) {
        // PB4 must be a driven-low output for the interface to be active.
        if self.via.port_b_drive_state() & 0x10 != 0 {
            return;
        }
        let select = self.via.ora();
        if select & 0xC0 == 0xC0 {
            return; // neither stick selected
        }
        let mut mask = !0x20u8; // bit 5: IJK-present marker (always low)
        if select & 0x40 != 0 {
            mask &= Self::joystick_mask(self.joystick[0]); // left port
        }
        if select & 0x80 != 0 {
            mask &= Self::joystick_mask(self.joystick[1]); // right port
        }
        self.via.pa_in = mask;
    }

    /// The active-low port-A mask one IJK stick would present (`port` 1 = left,
    /// 2 = right): bit 0 right, 1 left, 2 fire, 3 down, 4 up, a pressed control
    /// clearing its bit. For inspection and host-side input wiring.
    #[must_use]
    pub fn joystick_port_mask(&self, port: u8) -> u8 {
        Self::joystick_mask(self.joystick[usize::from(port.clamp(1, 2) - 1)])
    }

    /// The active-low port-A contribution for one stick: a pressed control
    /// clears its bit (bit 0 right, 1 left, 2 fire, 3 down, 4 up).
    fn joystick_mask(state: JoyState) -> u8 {
        let mut m = 0xFFu8;
        for (pressed, bit) in [
            (state.right, 0x01),
            (state.left, 0x02),
            (state.fire, 0x04),
            (state.down, 0x08),
            (state.up, 0x10),
        ] {
            if pressed {
                m &= !bit;
            }
        }
        m
    }

    /// Set the IJK joystick state for `port` (1 = left, 2 = right). Read back on
    /// VIA port A once the program enables the interface (PB4 low) and selects
    /// the stick (port A bit 6 / 7). Out-of-range ports clamp to the pair.
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
        self.joystick[idx] = JoyState {
            up,
            down,
            left,
            right,
            fire,
        };
    }

    fn render_display(&mut self) {
        // $26A bit 2 = HIRES mode.
        let hires = self.ram[0x026A] & 0x04 != 0;
        if hires {
            self.render_hires();
        } else {
            self.render_text();
        }
    }

    fn render_text(&mut self) {
        let base = 0xBB80usize;
        let charset_base = 0xB400usize;
        for row in 0..28 {
            let mut ink: u32 = ORIC_PALETTE[7];
            let mut paper: u32 = ORIC_PALETTE[0];
            for col in 0..40 {
                let byte = self.ram[base + row * 40 + col];
                let inverse = byte & 0x80 != 0;
                let effective = byte & 0x7F;
                if effective < 32 {
                    Self::apply_serial_attribute(effective, &mut ink, &mut paper);
                    Self::fill_cell(&mut self.framebuffer, row, col, paper);
                } else {
                    let char_idx = effective as usize;
                    for line in 0..8 {
                        let pattern = self
                            .ram
                            .get(charset_base + char_idx * 8 + line)
                            .copied()
                            .unwrap_or(0);
                        let fb_y = row * 8 + line;
                        if fb_y >= FB_HEIGHT as usize {
                            continue;
                        }
                        for bit in 0..6 {
                            let fb_x = col * 6 + bit;
                            if fb_x >= FB_WIDTH as usize {
                                continue;
                            }
                            let (fg, bg) = if inverse { (paper, ink) } else { (ink, paper) };
                            let pixel = if pattern & (0x20 >> bit) != 0 { fg } else { bg };
                            self.framebuffer[fb_y * FB_WIDTH as usize + fb_x] = pixel;
                        }
                    }
                }
            }
        }
    }

    fn render_hires(&mut self) {
        let text_base = 0xBF68usize;
        let charset_base = 0xB400usize;
        // Top 3 text rows.
        for row in 0..3 {
            let mut ink: u32 = ORIC_PALETTE[7];
            let mut paper: u32 = ORIC_PALETTE[0];
            for col in 0..40 {
                let byte = self.ram[text_base + row * 40 + col];
                let inverse = byte & 0x80 != 0;
                let effective = byte & 0x7F;
                if effective < 32 {
                    Self::apply_serial_attribute(effective, &mut ink, &mut paper);
                    Self::fill_cell(&mut self.framebuffer, row, col, paper);
                } else {
                    let char_idx = effective as usize;
                    for line in 0..8 {
                        let pattern = self
                            .ram
                            .get(charset_base + char_idx * 8 + line)
                            .copied()
                            .unwrap_or(0);
                        let fb_y = row * 8 + line;
                        if fb_y >= FB_HEIGHT as usize {
                            continue;
                        }
                        for bit in 0..6 {
                            let fb_x = col * 6 + bit;
                            if fb_x >= FB_WIDTH as usize {
                                continue;
                            }
                            let (fg, bg) = if inverse { (paper, ink) } else { (ink, paper) };
                            let pixel = if pattern & (0x20 >> bit) != 0 { fg } else { bg };
                            self.framebuffer[fb_y * FB_WIDTH as usize + fb_x] = pixel;
                        }
                    }
                }
            }
        }
        // Bitmap area: 200 lines from $A000.
        let bitmap_base = 0xA000usize;
        for line in 0..200 {
            let mut ink: u32 = ORIC_PALETTE[7];
            let mut paper: u32 = ORIC_PALETTE[0];
            for col in 0..40 {
                let byte = self.ram[bitmap_base + line * 40 + col];
                let inverse = byte & 0x80 != 0;
                let effective = byte & 0x7F;
                let fb_y = 24 + line;
                if fb_y >= FB_HEIGHT as usize {
                    continue;
                }
                if effective < 32 {
                    Self::apply_serial_attribute(effective, &mut ink, &mut paper);
                    for bit in 0..6 {
                        let fb_x = col * 6 + bit;
                        if fb_x < FB_WIDTH as usize {
                            self.framebuffer[fb_y * FB_WIDTH as usize + fb_x] = paper;
                        }
                    }
                } else {
                    for bit in 0..6 {
                        let fb_x = col * 6 + bit;
                        if fb_x >= FB_WIDTH as usize {
                            continue;
                        }
                        let (fg, bg) = if inverse { (paper, ink) } else { (ink, paper) };
                        let pixel = if effective & (0x20 >> bit) != 0 {
                            fg
                        } else {
                            bg
                        };
                        self.framebuffer[fb_y * FB_WIDTH as usize + fb_x] = pixel;
                    }
                }
            }
        }
    }

    fn fill_cell(fb: &mut [u32], row: usize, col: usize, paper: u32) {
        for line in 0..8 {
            let fb_y = row * 8 + line;
            if fb_y >= FB_HEIGHT as usize {
                continue;
            }
            for bit in 0..6 {
                let fb_x = col * 6 + bit;
                if fb_x < FB_WIDTH as usize {
                    fb[fb_y * FB_WIDTH as usize + fb_x] = paper;
                }
            }
        }
    }

    fn apply_serial_attribute(attr: u8, ink: &mut u32, paper: &mut u32) {
        match attr {
            0..=7 => *ink = ORIC_PALETTE[attr as usize],
            16..=23 => *paper = ORIC_PALETTE[(attr - 16) as usize],
            _ => {}
        }
    }

    /// Framebuffer (240×224 ARGB32).
    #[must_use]
    pub fn framebuffer(&self) -> &[u32] {
        &self.framebuffer
    }

    /// Take audio buffer drained from the AY.
    pub fn take_audio_buffer(&mut self) -> Vec<f32> {
        let mut out = vec![0.0_f32; AY_SAMPLES_PER_FRAME];
        self.psg.end_frame(&mut out);
        out
    }

    /// Press a key at the given (column, row) — Oric matrix is
    /// 8 columns × 8 rows.
    pub fn press_key(&mut self, col: usize, row: u8) {
        if col < 8 && row < 8 {
            self.keyboard[col] &= !(1 << row);
        }
    }

    /// Release a key at the given (column, row).
    pub fn release_key(&mut self, col: usize, row: u8) {
        if col < 8 && row < 8 {
            self.keyboard[col] |= 1 << row;
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

    /// VIA reference.
    #[must_use]
    pub fn via(&self) -> &Via6522 {
        &self.via
    }

    /// Model.
    #[must_use]
    pub fn model(&self) -> OricModel {
        self.model
    }

    /// CPU cycles since power-on.
    #[must_use]
    pub fn cpu_cycles(&self) -> u64 {
        self.cpu_cycles
    }

    /// Frame count since power-on.
    #[must_use]
    pub fn frame_count(&self) -> u64 {
        self.frame_count
    }
}

impl OricAtmos {
    /// Read one byte with no side effects (RAM / ROM; `$FF` for the VIA).
    #[must_use]
    pub fn peek(&self, addr: u16) -> u8 {
        match addr {
            0xC000..=0xFFFF => self
                .rom
                .get((addr - 0xC000) as usize)
                .copied()
                .unwrap_or(0xFF),
            0x0300..=0x03FF => 0xFF,
            _ => {
                let idx = addr as usize;
                if idx < self.ram_size {
                    self.ram[idx]
                } else {
                    0xFF
                }
            }
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
        // 16 KB ROM mapped to $C000-$FFFF. JMP self at $C000 for the
        // reset vector to land safely.
        let mut rom = vec![0xEA_u8; 0x4000];
        rom[0x0000] = 0x4C;
        rom[0x0001] = 0x00;
        rom[0x0002] = 0xC0;
        // Reset / IRQ / NMI vectors all point at $C000.
        rom[0x3FFC] = 0x00;
        rom[0x3FFD] = 0xC0;
        rom[0x3FFE] = 0x00;
        rom[0x3FFF] = 0xC0;
        rom[0x3FFA] = 0x00;
        rom[0x3FFB] = 0xC0;
        rom
    }

    #[test]
    fn frame_runs_expected_cycles() {
        let mut sys = OricAtmos::new(trap_rom(), OricModel::Atmos);
        let t = sys.run_frame();
        assert_eq!(t, TICKS_PER_FRAME);
        assert_eq!(sys.frame_count(), 1);
    }

    #[test]
    fn many_frames_complete_without_panic() {
        let mut sys = OricAtmos::new(trap_rom(), OricModel::Atmos);
        for _ in 0..30 {
            sys.run_frame();
        }
        assert_eq!(sys.frame_count(), 30);
    }

    #[test]
    fn rom_visible_at_c000() {
        let mut sys = OricAtmos::new(trap_rom(), OricModel::Atmos);
        assert_eq!(sys.mem_read(0xC000), 0x4C);
        assert_eq!(sys.mem_read(0xFFFC), 0x00);
        assert_eq!(sys.mem_read(0xFFFD), 0xC0);
    }

    #[test]
    fn writes_go_to_ram_under_rom_on_atmos() {
        let mut sys = OricAtmos::new(trap_rom(), OricModel::Atmos);
        // ROM read at $C000.
        assert_eq!(sys.mem_read(0xC000), 0x4C);
        // Write to RAM underneath at $C000.
        sys.mem_write(0xC000, 0x42);
        // ROM still wins on reads.
        assert_eq!(sys.mem_read(0xC000), 0x4C);
        // RAM was updated underneath.
        assert_eq!(sys.ram[0xC000], 0x42);
    }

    #[test]
    fn via_register_round_trip() {
        let mut sys = OricAtmos::new(trap_rom(), OricModel::Atmos);
        // DDRA = $FF, then ORA = $42; check via.ora().
        sys.mem_write(0x0303, 0xFF); // DDRA
        sys.mem_write(0x0301, 0x42); // ORA
        assert_eq!(sys.via.ora(), 0x42);
    }

    #[test]
    fn ay_register_latch_via_pcr() {
        let mut sys = OricAtmos::new(trap_rom(), OricModel::Atmos);
        // DDRA = $FF (port A all output).
        sys.mem_write(0x0303, 0xFF);
        // Put 7 in port A latch (target AY register = 7).
        sys.mem_write(0x0301, 0x07);
        // PCR = $EE → both CA2 and CB2 in "fixed high" output mode
        // → BDIR=1, BC1=1 → latch register address.
        sys.mem_write(0x030C, 0xEE);
        // AY's selected register should now be 7.
        assert_eq!(sys.psg.selected_register(), 7);
    }

    #[test]
    fn keyboard_senses_on_pb3() {
        let mut sys = OricAtmos::new(trap_rom(), OricModel::Atmos);
        // Press the key at column 3, row 5.
        sys.press_key(3, 5);
        // Select column 3 on port B (PB0-2).
        sys.mem_write(0x0300, 0x03);
        // Drive row 5 low on port A: the pressed key grounds the sense,
        // so PB3 reads high. (Writing ORA also re-runs the scan.)
        sys.mem_write(0x0301, !(1u8 << 5));
        assert_ne!(sys.via.pb_in & 0x08, 0, "PB3 should sense the pressed key");
        // Driving a different row leaves the sense clear.
        sys.mem_write(0x0301, !(1u8 << 2));
        assert_eq!(
            sys.via.pb_in & 0x08,
            0,
            "PB3 clear when the row is not driven"
        );
    }

    #[test]
    fn key_press_and_release_round_trip() {
        let mut sys = OricAtmos::new(trap_rom(), OricModel::Atmos);
        sys.press_key(2, 5);
        assert_eq!(sys.keyboard[2] & (1 << 5), 0);
        sys.release_key(2, 5);
        assert_eq!(sys.keyboard[2] & (1 << 5), 1 << 5);
    }

    /// Configure the VIA the way an IJK read routine does: PB4 a driven-low
    /// output (interface enable), port A bits 0-5 inputs and 6-7 outputs.
    fn enable_ijk(sys: &mut OricAtmos) {
        sys.mem_write(0x0302, 0x10); // DDRB: PB4 output
        sys.mem_write(0x0300, 0x00); // ORB: PB4 low
        sys.mem_write(0x0303, 0xC0); // DDRA: bits 6-7 output, 0-5 input
    }

    #[test]
    fn ijk_joystick_reads_on_port_a_when_enabled_and_selected() {
        let mut sys = OricAtmos::new(trap_rom(), OricModel::Atmos);
        enable_ijk(&mut sys);

        // Select the left port (port A bit 6) and read it idle.
        sys.mem_write(0x0301, 0x40);
        let pa = sys.mem_read(0x0301);
        assert_eq!(pa & 0x20, 0, "bit 5 low = IJK present");
        assert_eq!(pa & 0x1F, 0x1F, "no directions held");

        // Press left-port up + fire: bit 4 (up) and bit 2 (fire) go low.
        sys.set_joystick(1, true, false, false, false, true);
        let pa = sys.mem_read(0x0301);
        assert_eq!(pa & 0x10, 0, "up → bit 4 low");
        assert_eq!(pa & 0x04, 0, "fire → bit 2 low");
        assert_eq!(pa & 0x01, 0x01, "right idle high");

        // The right-port stick is independent — selecting bit 7 reads it.
        sys.set_joystick(2, false, false, true, false, false); // left
        sys.mem_write(0x0301, 0x80);
        let pa = sys.mem_read(0x0301);
        assert_eq!(pa & 0x02, 0, "right-port left → bit 1 low");
        assert_eq!(
            pa & 0x10,
            0x10,
            "right-port up idle (left stick not selected)"
        );
    }

    #[test]
    fn ijk_inactive_unless_pb4_is_a_driven_low_output() {
        let mut sys = OricAtmos::new(trap_rom(), OricModel::Atmos);
        // DDRA set for the read, but PB4 left as an input (interface disabled).
        sys.mem_write(0x0303, 0xC0);
        sys.mem_write(0x0301, 0x40); // select left port
        sys.set_joystick(1, true, false, false, false, false);
        // With the interface disabled, the joystick must not drive port A: the
        // marker bit 5 stays high (open lines read high).
        let pa = sys.mem_read(0x0301);
        assert_ne!(pa & 0x20, 0, "no IJK drive while PB4 is not a low output");
    }
}
