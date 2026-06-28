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
pub use sinclair_zx81_ula::{FB_HEIGHT, FB_WIDTH, Zx81Ula};

use serde::{Deserialize, Serialize};
use zilog_z80::z80::{BusOp, Z80};

/// Sinclair ZX80 machine.
#[derive(Serialize, Deserialize)]
pub struct Zx80 {
    cpu: Z80,
    rom: Vec<u8>,
    ram: Vec<u8>,
    ram_mask: u16,
    ula: Zx81Ula,
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
            ula: Zx81Ula::new(),
            keyboard: KeyboardState::new(),
            master_clock: 0,
            frame_count: 0,
            io_trace: None,
        })
    }

    pub fn run_frame(&mut self) -> u64 {
        let start = self.master_clock;
        loop {
            self.tick_tstate();
            if self.ula.take_frame_complete() {
                break;
            }
        }
        self.frame_count += 1;
        self.master_clock - start
    }

    fn tick_tstate(&mut self) {
        self.master_clock += 1;

        // The ULA needs to read memory for display rendering. Snapshot the
        // ROM/RAM borrows out of self.bus to avoid a self-referential borrow.
        let rom = &self.rom;
        let ram = &self.ram;
        let mask = self.ram_mask;
        self.ula.tick(|addr| match addr {
            0x0000..=0x3FFF => rom[(addr & 0x0FFF) as usize],
            0x4000..=0x7FFF => ram[((addr - 0x4000) & mask) as usize],
            0x8000..=0xBFFF => rom[(addr & 0x0FFF) as usize],
            0xC000..=0xFFFF => ram[((addr - 0xC000) & mask) as usize],
        });

        // The ZX80 does NOT wire NMI to the ULA — leave self.cpu.nmi alone.
        self.cpu.tick();
        self.handle_bus();
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

    fn io_read(&self, port: u16) -> u8 {
        if port & 0x01 == 0 {
            return self.keyboard.read((port >> 8) as u8);
        }
        0xFF
    }

    #[must_use]
    pub fn framebuffer(&self) -> &[u32] {
        self.ula.framebuffer()
    }

    #[must_use]
    pub fn framebuffer_width(&self) -> u32 {
        self.ula.framebuffer_width()
    }

    #[must_use]
    pub fn framebuffer_height(&self) -> u32 {
        self.ula.framebuffer_height()
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
