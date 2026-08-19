//! Sinclair ZX80 (1980) — Z80A + 4 KB ROM + ZX81-family ULA.
//!
//! Fresh-write against the workspace pin-driven bus pattern (RULES.md
//! rule 6). Donor at `Emu198x-Oldest/crates/machine-sinclair-zx80/`
//! used the deprecated `emu_core::Bus` callback; this file uses it as
//! the system spec but wires [`zilog_z80::Z80`] through its public pin
//! fields and `bus_request()` collapse.
//!
//! # The ZX80
//!
//! Sinclair Research's £100 launch — designed by Jim Westwood, sold
//! kit-or-built, around 100,000 units. Forerunner of the ZX81 and
//! Spectrum. Unlike the ZX81, the ZX80 has no NMI-driven display
//! generation: the screen is blanked while the CPU runs (FAST mode)
//! and the display is generated only while the CPU is halted (SLOW
//! mode). The NMI line is not connected to the ULA.
//!
//! - **CPU:** Zilog Z80A at 3.25 MHz
//! - **ROM:** 4 KB at `$0000-$0FFF` (mirrored to `$1000-$3FFF` and
//!   `$8000-$BFFF`)
//! - **RAM:** 1 KB at `$4000-$43FF` (or 16 KB with the expansion pack
//!   filling `$4000-$7FFF`); mirrored to `$C000-$FFFF`
//! - **Display:** 32 × 24 characters from D_FILE, rendered via the ZX81
//!   ULA's display routine (`Zx81Ula`)
//! - **Audio:** none
//! - **Keyboard:** identical 8 × 5 matrix to the Spectrum / ZX81

pub mod input;
mod keyboard;

pub use input::Zx80Key;
pub use keyboard::KeyboardState;
mod video;
pub use video::{FB_HEIGHT, FB_WIDTH, Zx80Video};

use serde::{Deserialize, Serialize};
use zilog_z80::z80::{BusOp, Z80};

/// Sinclair ZX80 machine.
#[derive(Serialize, Deserialize)]
pub struct Zx80 {
    cpu: Z80,
    rom: Vec<u8>,
    ram: Vec<u8>,
    ram_mask: u16,
    video: Zx80Video,
    prev_rfsh: bool,
    prev_halt: bool,
    dbg_m1: u64,
    dbg_m1_high: u64,
    dbg_high_byte: u8,
    dbg_chars: u64,
    keyboard: KeyboardState,
    master_clock: u64,
    frame_count: u64,
    /// When `Some`, every I/O port access is appended here (debug trace).
    #[serde(skip)]
    io_trace: Option<Vec<IoEvent>>,
}

impl Zx80 {
    /// Create a new ZX80. `rom` must be 4 KB. `ram_size` is 1024
    /// (unexpanded) or 16384 (16 KB RAM pack).
    pub fn new(rom: Vec<u8>, ram_size: usize) -> Result<Self, String> {
        if rom.len() != 0x1000 {
            return Err(format!("ZX80 ROM must be 4096 bytes, got {}", rom.len()));
        }
        if !ram_size.is_power_of_two() || ram_size > 0x4000 {
            return Err(format!(
                "ZX80 RAM size must be a power of two <= 16384, got {ram_size}"
            ));
        }
        Ok(Self {
            cpu: Z80::new(),
            rom,
            ram: vec![0; ram_size],
            ram_mask: (ram_size - 1) as u16,
            video: Zx80Video::new(),
            prev_rfsh: false,
            prev_halt: false,
            dbg_m1: 0,
            dbg_m1_high: 0,
            dbg_high_byte: 0,
            dbg_chars: 0,
            keyboard: KeyboardState::new(),
            master_clock: 0,
            frame_count: 0,
            io_trace: None,
        })
    }

    pub fn run_frame(&mut self) -> u64 {
        let start = self.master_clock;
        // Start from paper. A frame in which the CPU never entered the
        // display routine must come out blank rather than holding the last
        // picture — that blanking is the ZX80's defining behaviour, not a
        // dropped frame.
        self.video.clear();
        loop {
            self.tick_tstate();
            if self.video.take_frame_complete() {
                break;
            }
        }
        self.frame_count += 1;
        self.master_clock - start
    }

    fn tick_tstate(&mut self) {
        self.master_clock += 1;

        self.video.tick();

        // The ZX80 has no NMI generator — that is the ZX81's addition, and
        // the reason this machine blanks while it thinks. Leave cpu.nmi
        // alone.
        //
        // Two CPU half-cycles per T-state. `Z80::tick` advances one
        // half-cycle — `T1Rise` then `T1Fall` — so calling it once per
        // T-state ran the CPU at half speed: a `NOP` cost 8 T-states against
        // the Z80's 4. The ULA above is denominated in T-states (207 per
        // line, 312 lines, 3.25 MHz), so it was the CPU that was wrong and
        // not the ULA — the machine rendered a full 50 Hz raster while
        // executing half the code that belongs in one.
        for _ in 0..2 {
            self.cpu.tick();
            self.handle_bus();

            // /REFRESH taking the ROM address lines away from the CPU. The
            // address bus holds `I:R`, and the multiplexers take A9-A12 of
            // it. Refresh is not a no-op on this machine: ignoring it
            // removes the display.
            // A `HALT` ends a display line. The interrupt wired to A6
            // releases it, and that release is the line sync — the vertical
            // position is counted from these, not from a clock.
            let halt = self.cpu.halt;
            if halt && !self.prev_halt {
                self.video.line_sync();
            }
            self.prev_halt = halt;

            let rfsh = self.cpu.rfsh;
            if rfsh {
                // INT is wired to address line A6, so an interrupt is
                // generated whenever the address on the bus has A6 low. The
                // refresh address is `I:R`, and `R` counts up as the display
                // is fetched — so the interrupt arrives once per display
                // line, ends the `HALT` that terminates the line, and the
                // ROM's handler moves to the next one.
                //
                // Without this the CPU HALTs on the first line and never
                // leaves: 92% of fetches were phantom NOPs when this was
                // missing.
                self.cpu.irq = self.cpu.addr & 0x0040 == 0;
            }
            if rfsh && !self.prev_rfsh {
                let rom = &self.rom;
                self.video
                    .refresh(self.cpu.addr, |addr| rom[(addr & 0x0FFF) as usize]);
            }
            self.prev_rfsh = rfsh;
        }
    }

    fn handle_bus(&mut self) {
        match self.cpu.bus_request() {
            Some(BusOp::MemRead) => {
                let byte = self.mem_read(self.cpu.addr);
                // A display fetch is answered with $00 on the CPU side while
                // the real byte goes to the character latch. One read, two
                // results — which is the whole trick.
                if self.cpu.m1 {
                    self.dbg_m1 += 1;
                    if self.cpu.addr >= 0x8000 {
                        self.dbg_m1_high += 1;
                        if byte & 0x40 == 0 {
                            self.dbg_chars += 1;
                        }
                    }
                }
                self.cpu.data_in = if self.cpu.m1 {
                    self.video.opcode_fetch(self.cpu.addr, byte).unwrap_or(byte)
                } else {
                    byte
                };
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
                // Any `OUT` stops the vertical sync and ends the frame. The
                // 50/60 Hz difference is a software constant plus a diode at
                // D11, not a mode bit — so this is the only thing that sets
                // the frame rate.
                self.video.vsync_stop();
                // Cassette out exists but is not wired in v1.
            }
            Some(BusOp::IntAck) => {
                self.cpu.data_in = 0xFF;
            }
            None => {}
        }
    }

    fn mem_read(&self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x3FFF | 0x8000..=0xBFFF => self.rom[(addr & 0x0FFF) as usize],
            0x4000..=0x7FFF => self.ram[((addr - 0x4000) & self.ram_mask) as usize],
            0xC000..=0xFFFF => self.ram[((addr - 0xC000) & self.ram_mask) as usize],
        }
    }

    fn mem_write(&mut self, addr: u16, value: u8) {
        match addr {
            0x0000..=0x3FFF | 0x8000..=0xBFFF => {}
            0x4000..=0x7FFF => {
                self.ram[((addr - 0x4000) & self.ram_mask) as usize] = value;
            }
            0xC000..=0xFFFF => {
                self.ram[((addr - 0xC000) & self.ram_mask) as usize] = value;
            }
        }
    }

    /// `IN` with A0 low reads the keyboard *and* starts the vertical sync.
    /// One instruction, two jobs — there is no timing chip to ask.
    fn io_read(&mut self, port: u16) -> u8 {
        if port & 0x01 == 0 {
            self.video.vsync_start();
            return self.keyboard.read((port >> 8) as u8);
        }
        0xFF
    }

    #[must_use]
    pub fn video_events(&self) -> &[(char, u32)] {
        &self.video.dbg_events
    }

    #[must_use]
    pub fn video_counts(&self) -> (u32, u32, u32, u32, u32) {
        (
            self.video.dbg_overflow,
            self.video.dbg_vsync_start,
            self.video.dbg_vsync_stop,
            self.video.dbg_paint_calls,
            self.video.dbg_forced,
        )
    }

    pub fn video_debug(&self) -> (u32, u32, u32, u32) {
        (
            self.video.dbg_min_line,
            self.video.dbg_max_line,
            self.video.dbg_min_x,
            self.video.dbg_max_x,
        )
    }

    #[must_use]
    pub fn framebuffer(&self) -> &[u32] {
        self.video.framebuffer()
    }

    #[must_use]
    pub fn framebuffer_width(&self) -> u32 {
        FB_WIDTH
    }

    #[must_use]
    pub fn framebuffer_height(&self) -> u32 {
        FB_HEIGHT
    }

    pub fn press_key(&mut self, key: Zx80Key) {
        let (row, bit) = key.matrix();
        self.keyboard.set_key(row, bit, true);
    }

    pub fn release_key(&mut self, key: Zx80Key) {
        let (row, bit) = key.matrix();
        self.keyboard.set_key(row, bit, false);
    }

    pub fn release_all_keys(&mut self) {
        self.keyboard.release_all();
    }

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

impl zilog_z80::Z80Stepper for Zx80 {
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

    fn trap_rom() -> Vec<u8> {
        let mut rom = vec![0u8; 0x1000];
        rom[0] = 0xF3; // DI
        rom[1] = 0x76; // HALT
        rom
    }

    #[test]
    fn frame_advances_master_clock_and_count() {
        let mut sys = Zx80::new(trap_rom(), 1024).expect("init");
        let clocks = sys.run_frame();
        assert!(clocks > 0);
        assert_eq!(sys.frame_count(), 1);
    }

    #[test]
    fn rom_size_validated() {
        assert!(Zx80::new(vec![0u8; 8192], 1024).is_err());
    }

    #[test]
    fn ram_size_validated() {
        assert!(Zx80::new(trap_rom(), 1500).is_err());
    }

    #[test]
    fn ram_mirrors_at_c000() {
        let mut sys = Zx80::new(trap_rom(), 1024).expect("init");
        sys.mem_write(0x4000, 0x42);
        assert_eq!(sys.mem_read(0xC000), 0x42);
    }

    #[test]
    fn rom_mirrors_at_8000() {
        let mut rom = trap_rom();
        rom[0x0010] = 0x55;
        let sys = Zx80::new(rom, 1024).expect("init");
        assert_eq!(sys.mem_read(0x8010), 0x55);
    }

    #[test]
    fn rom_is_read_only() {
        let mut sys = Zx80::new(trap_rom(), 1024).expect("init");
        sys.mem_write(0x0000, 0xFF);
        assert_eq!(sys.mem_read(0x0000), 0xF3);
    }

    #[test]
    fn keyboard_press_release() {
        let mut sys = Zx80::new(trap_rom(), 1024).expect("init");
        sys.press_key(Zx80Key::A);
        // Row 1 (A9 clear → high byte 0xFD).
        assert_eq!(sys.io_read(0xFDFE) & 0x01, 0x00);
        sys.release_key(Zx80Key::A);
        assert_eq!(sys.io_read(0xFDFE) & 0x01, 0x01);
    }

    #[test]
    fn snapshot_round_trips_live_state() {
        let mut sys = Zx80::new(trap_rom(), 1024).expect("init");
        sys.run_frame();

        // Write a RAM byte so the captured state is non-trivial. RAM is at
        // $4000+ on the ZX80; confirm the write landed via the read path.
        sys.poke(0x4000, 0x5A);
        assert_eq!(sys.peek(0x4000), 0x5A);

        let first = postcard::to_allocvec(&sys).expect("encode first");

        // Advance and re-serialise — the master clock / frame count move on,
        // so the two snapshots must differ.
        sys.run_frame();
        let second = postcard::to_allocvec(&sys).expect("encode second");
        assert_ne!(first, second, "advancing a frame must change the snapshot");

        // Restoring the first snapshot and re-encoding must be byte-identical.
        let restored: Zx80 = postcard::from_bytes(&first).expect("decode first");
        let reencoded = postcard::to_allocvec(&restored).expect("re-encode restored");
        assert_eq!(first, reencoded, "round-trip must be byte-identical");
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

impl Zx80 {
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
