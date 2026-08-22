//! Jupiter Ace (Jupiter Cantab, 1982) — Z80A + 8 KB Forth ROM.
//!
//! Fresh-write against the workspace pin-driven bus pattern (RULES.md
//! rule 6). The donor at `Emu198x-Oldest/crates/machine-jupiter-ace/`
//! used the deprecated `emu_core::Bus` callback and could not port
//! directly; this file uses it as the system spec, but the CPU wiring
//! goes through [`zilog_z80::Z80`]'s public pin fields and
//! `bus_request()` collapse.
//!
//! # The Jupiter Ace
//!
//! Forth-instead-of-BASIC home machine designed by Steven Vickers and
//! Richard Altwasser (the team behind the ZX Spectrum's ROM). Z80A at
//! 3.25 MHz, 8 KB Forth ROM, 3 KB base RAM (expandable). Character-
//! based 32 × 24 display with a fully user-redefinable 128-glyph
//! character set in RAM — same trick as the Spectrum's UDG, taken
//! further. Commercial flop; cult favourite.
//!
//! - **CPU:** Zilog Z80A at 3.25 MHz
//! - **ROM:** 8 KB Forth interpreter at `$0000-$1FFF`
//! - **Video RAM:** 1 KB at `$2000-$23FF` (768 codes used), mirrored at
//!   `$2400-$27FF`
//! - **Character RAM:** 1 KB at `$2800-$2BFF` (128 × 8-byte glyphs),
//!   mirrored at `$2C00-$2FFF`
//! - **Base RAM:** 1 KB mirrored across `$3000-$3FFF`; expansion at `$4000+`
//! - **Display:** 256 × 192 monochrome, 32 × 24 characters
//! - **Audio:** 1-bit beeper on port `$FE` bit 4
//! - **Keyboard:** identical 8 × 5 matrix to the ZX Spectrum, scanned
//!   via port `$FE` with the row selector in the high address byte
//!
//! # Clock model
//!
//! PAL display: 312 lines × 208 T-states/line = 64,896 T-states per
//! frame at 3.25 MHz, ~50.3 Hz. CPU + display + audio downsampler all
//! tick once per T-state.

mod display;
pub mod input;
mod keyboard;

pub use display::{Display, FB_HEIGHT, FB_WIDTH, TSTATES_PER_FRAME};
pub use input::JupiterAceKey;
pub use keyboard::KeyboardState;

use serde::{Deserialize, Serialize};
use zilog_z80::z80::{BusOp, Z80};

/// Jupiter Ace machine.
///
/// Fully serialisable for save-states: the CPU (Z80), all RAM/ROM, and the
/// display/keyboard carry live state. `audio_buffer` and `io_trace` are host-
/// side debug/output buffers, not machine state, so they are skipped and
/// default-initialised on restore.
#[derive(Serialize, Deserialize)]
pub struct JupiterAce {
    cpu: Z80,
    rom: Vec<u8>,
    video_ram: Vec<u8>,
    char_ram: Vec<u8>,
    ram: Vec<u8>,
    display: Display,
    keyboard: KeyboardState,
    #[serde(skip)]
    audio_buffer: Vec<f32>,
    audio_accum: u64,
    audio_denom: u64,
    master_clock: u64,
    frame_count: u64,
    /// When `Some`, every I/O port access is appended here (debug trace).
    #[serde(skip)]
    io_trace: Option<Vec<IoEvent>>,
}

impl JupiterAce {
    /// Create a new Jupiter Ace. `rom` must be exactly 8192 bytes
    /// (the Forth interpreter). `ram_size` is the bytes of general
    /// RAM: the first 1 KB is the base user RAM mirrored across
    /// `$3000-$3FFF`, and any remainder backs the `$4000+` expansion.
    pub fn new(rom: Vec<u8>, ram_size: usize) -> Result<Self, String> {
        if rom.len() != 0x2000 {
            return Err(format!(
                "Jupiter Ace ROM must be 8192 bytes, got {}",
                rom.len()
            ));
        }
        let cpu = Z80::new();
        Ok(Self {
            cpu,
            rom,
            video_ram: vec![0; 1024],
            char_ram: vec![0; 1024],
            ram: vec![0; ram_size],
            display: Display::new(),
            keyboard: KeyboardState::new(),
            audio_buffer: Vec::with_capacity(1024),
            audio_accum: 0,
            audio_denom: 3_250_000,
            master_clock: 0,
            frame_count: 0,
            io_trace: None,
        })
    }

    /// Run one full PAL frame (64,896 T-states); returns T-states executed.
    pub fn run_frame(&mut self) -> u64 {
        let start = self.master_clock;
        loop {
            self.tick_tstate();
            if self.display.take_frame_complete() {
                let vram = self.video_ram.clone();
                let cram = self.char_ram.clone();
                self.display.render_frame(&vram, &cram);
                break;
            }
        }
        self.frame_count += 1;
        self.master_clock - start
    }

    fn tick_tstate(&mut self) {
        self.master_clock += 1;
        self.display.tick();
        self.tick_audio();
        // The `zilog-z80` core is a half-cycle state machine: it needs two
        // ticks per T-state (the Spectrum drives it the same way — see
        // `common-sinclair-zx-spectrum` `tick_one_halfcycle`). Driving it
        // once per T-state under-clocked the CPU 2× and meant the IRQ was
        // never sampled at an instruction boundary, so the Forth ROM spun
        // forever waiting for its 50 Hz frame interrupt.
        for _ in 0..2 {
            self.cpu.irq = self.display.interrupt_active();
            self.cpu.tick();
            self.handle_bus();
        }
    }

    fn tick_audio(&mut self) {
        self.audio_accum += 48_000;
        if self.audio_accum >= self.audio_denom {
            self.audio_accum -= self.audio_denom;
            let sample = if self.display.speaker_state {
                0.5
            } else {
                -0.5
            };
            self.audio_buffer.push(sample);
        }
    }

    fn handle_bus(&mut self) {
        match self.cpu.bus_request() {
            Some(BusOp::MemRead) => {
                self.cpu.data_in = self.mem_read(self.cpu.addr);
            }
            Some(BusOp::MemWrite) => {
                self.mem_write(self.cpu.addr, self.cpu.data);
            }
            Some(BusOp::IoRead) => {
                let io_port = (self.cpu.addr & 0xFF) as u8;
                let io_pc = self.cpu.regs.pc;
                let io_val = self.io_read(self.cpu.addr);
                self.cpu.data_in = io_val;
                if let Some(trace) = &mut self.io_trace {
                    trace.push(IoEvent {
                        pc: io_pc,
                        port: io_port,
                        value: io_val,
                        write: false,
                    });
                }
            }
            Some(BusOp::IoWrite) => {
                if let Some(trace) = &mut self.io_trace {
                    trace.push(IoEvent {
                        pc: self.cpu.regs.pc,
                        port: (self.cpu.addr & 0xFF) as u8,
                        value: self.cpu.data,
                        write: true,
                    });
                }
                self.io_write(self.cpu.addr, self.cpu.data);
            }
            Some(BusOp::IntAck) => {
                // Ace ROM uses IM 1; vector ignored. Acknowledging the
                // interrupt releases the held INT line.
                self.cpu.data_in = 0xFF;
                self.display.ack_interrupt();
            }
            None => {}
        }
    }

    fn mem_read(&self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x1FFF => self.rom[addr as usize],
            // Video RAM ($2000) and character RAM ($2800) are each 1 KB and
            // each mirror once into the following 1 KB ($2400 / $2C00) — the
            // address decode ignores A10. (MAME `ace_mem`.)
            0x2000..=0x27FF => self.video_ram[(addr & 0x03FF) as usize],
            0x2800..=0x2FFF => self.char_ram[(addr & 0x03FF) as usize],
            // Base 1 KB user RAM, mirrored across $3000-$3FFF.
            0x3000..=0x3FFF => self.ram[(addr & 0x03FF) as usize],
            // Expansion RAM at $4000+ (open bus on the unexpanded machine).
            _ => self
                .ram
                .get(0x0400 + (addr as usize - 0x4000))
                .copied()
                .unwrap_or(0xFF),
        }
    }

    fn mem_write(&mut self, addr: u16, value: u8) {
        match addr {
            0x0000..=0x1FFF => {}
            0x2000..=0x27FF => self.video_ram[(addr & 0x03FF) as usize] = value,
            0x2800..=0x2FFF => self.char_ram[(addr & 0x03FF) as usize] = value,
            0x3000..=0x3FFF => self.ram[(addr & 0x03FF) as usize] = value,
            _ => {
                let offset = 0x0400 + (addr as usize - 0x4000);
                if offset < self.ram.len() {
                    self.ram[offset] = value;
                }
            }
        }
    }

    fn io_read(&self, port: u16) -> u8 {
        // Port $FE (bit 0 clear): keyboard read; row selector in high byte.
        if port & 0x01 == 0 {
            return self.keyboard.read((port >> 8) as u8);
        }
        0xFF
    }

    fn io_write(&mut self, port: u16, value: u8) {
        if port & 0x01 == 0 {
            self.display.speaker_state = value & 0x10 != 0;
        }
    }

    #[must_use]
    pub fn framebuffer(&self) -> &[u32] {
        self.display.framebuffer()
    }

    #[must_use]
    pub fn framebuffer_width(&self) -> u32 {
        self.display.framebuffer_width()
    }

    #[must_use]
    pub fn framebuffer_height(&self) -> u32 {
        self.display.framebuffer_height()
    }

    /// Take and clear the queued mono beeper samples.
    pub fn take_audio_buffer(&mut self) -> Vec<f32> {
        std::mem::take(&mut self.audio_buffer)
    }

    pub fn press_key(&mut self, key: JupiterAceKey) {
        let (row, bit) = key.matrix();
        self.keyboard.set_key(row, bit, true);
    }

    pub fn release_key(&mut self, key: JupiterAceKey) {
        let (row, bit) = key.matrix();
        self.keyboard.set_key(row, bit, false);
    }

    pub fn release_all_keys(&mut self) {
        self.keyboard.release_all();
    }

    /// Peek a byte from memory without side effects.
    #[must_use]
    pub fn peek_memory(&self, addr: u16) -> u8 {
        self.mem_read(addr)
    }

    #[must_use]
    pub fn cpu(&self) -> &Z80 {
        &self.cpu
    }

    pub fn cpu_mut(&mut self) -> &mut Z80 {
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

impl zilog_z80::Z80Stepper for JupiterAce {
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

    /// Save-state must capture LIVE machine state, not cold-boot from ROM.
    /// Advance the machine, serialise, advance further (so the state differs),
    /// then deserialise the first snapshot and confirm re-serialising it is
    /// byte-identical — i.e. every stateful field round-trips. A bootstrap-only
    /// snapshot would re-derive a fresh machine and fail the final equality.
    #[test]
    fn snapshot_round_trips_live_state() {
        let mut ace = JupiterAce::new(trap_rom(), 1024).expect("build machine");
        ace.run_frame();
        ace.poke(0x3000, 0xA5); // a base-RAM byte to carry across the snapshot
        ace.run_frame();
        let s1 = postcard::to_allocvec(&ace).expect("encode snapshot");

        ace.run_frame(); // advance past the snapshot point
        let s2 = postcard::to_allocvec(&ace).expect("encode again");
        assert_ne!(s1, s2, "running a frame should change the serialised state");

        let restored: JupiterAce = postcard::from_bytes(&s1).expect("decode snapshot");
        let s3 = postcard::to_allocvec(&restored).expect("re-encode restored");
        assert_eq!(
            s1, s3,
            "restore should reproduce the snapshot state exactly"
        );
    }

    fn trap_rom() -> Vec<u8> {
        // DI ; HALT ; pad to 8 KB
        let mut rom = vec![0u8; 0x2000];
        rom[0] = 0xF3;
        rom[1] = 0x76;
        rom
    }

    #[test]
    fn frame_advances_master_clock_and_count() {
        let mut sys = JupiterAce::new(trap_rom(), 1024).expect("init");
        let clocks = sys.run_frame();
        assert_eq!(clocks, u64::from(TSTATES_PER_FRAME));
        assert_eq!(sys.frame_count(), 1);
    }

    #[test]
    fn framebuffer_correct_size() {
        let sys = JupiterAce::new(trap_rom(), 1024).expect("init");
        assert_eq!(sys.framebuffer_width(), FB_WIDTH);
        assert_eq!(sys.framebuffer_height(), FB_HEIGHT);
        assert_eq!(sys.framebuffer().len(), (FB_WIDTH * FB_HEIGHT) as usize);
    }

    #[test]
    fn rom_too_small_rejected() {
        assert!(JupiterAce::new(vec![0u8; 1024], 1024).is_err());
    }

    #[test]
    fn ram_is_writable_and_readable() {
        let mut sys = JupiterAce::new(trap_rom(), 1024).expect("init");
        sys.mem_write(0x2800, 0x42);
        assert_eq!(sys.mem_read(0x2800), 0x42);
    }

    #[test]
    fn rom_is_read_only() {
        let mut sys = JupiterAce::new(trap_rom(), 1024).expect("init");
        sys.mem_write(0x0000, 0xFF);
        assert_eq!(sys.mem_read(0x0000), 0xF3);
    }

    #[test]
    fn video_and_char_ram_independent() {
        let mut sys = JupiterAce::new(trap_rom(), 1024).expect("init");
        // Video RAM ($2000) and character RAM ($2800) are separate banks.
        sys.mem_write(0x2000, 0xAA);
        sys.mem_write(0x2800, 0xBB);
        assert_eq!(sys.mem_read(0x2000), 0xAA);
        assert_eq!(sys.mem_read(0x2800), 0xBB);
    }

    #[test]
    fn video_and_char_ram_mirror_at_high_kilobyte() {
        let mut sys = JupiterAce::new(trap_rom(), 1024).expect("init");
        // The decode ignores A10, so each 1 KB bank reappears in the next:
        // video RAM at $2400, character RAM at $2C00.
        sys.mem_write(0x2000, 0x11);
        sys.mem_write(0x2800, 0x22);
        assert_eq!(sys.mem_read(0x2400), 0x11, "video RAM mirrors at $2400");
        assert_eq!(sys.mem_read(0x2C00), 0x22, "char RAM mirrors at $2C00");
    }

    #[test]
    fn beeper_port_drives_speaker_state() {
        let mut sys = JupiterAce::new(trap_rom(), 1024).expect("init");
        sys.io_write(0x00FE, 0x10);
        assert!(sys.display.speaker_state);
        sys.io_write(0x00FE, 0x00);
        assert!(!sys.display.speaker_state);
    }

    #[test]
    fn keyboard_press_release() {
        let mut sys = JupiterAce::new(trap_rom(), 1024).expect("init");
        sys.press_key(JupiterAceKey::A);
        // Row 1 = A9 high-byte bit 1 clear (0xFD).
        assert_eq!(sys.io_read(0xFDFE) & 0x01, 0x00);
        sys.release_key(JupiterAceKey::A);
        assert_eq!(sys.io_read(0xFDFE) & 0x01, 0x01);
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

impl JupiterAce {
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
        self.io_trace = Some(Vec::new());
    }

    /// Stop tracing and return the captured I/O events.
    pub fn take_io_trace(&mut self) -> Vec<IoEvent> {
        self.io_trace.take().unwrap_or_default()
    }
}
