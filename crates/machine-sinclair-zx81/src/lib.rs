//! Sinclair ZX81 (1981) — Z80A + 8 KB ROM + ULA with NMI-driven display.
//!
//! Fresh-write against the workspace pin-driven bus pattern (RULES.md
//! rule 6). Donor at `Emu198x-Oldest/crates/machine-sinclair-zx81/`
//! used the deprecated `emu_core::Bus` callback; this file uses it as
//! the system spec but wires [`zilog_z80::Z80`] through its public pin
//! fields and `bus_request()` collapse. The ULA crate is the same one
//! used by the ZX80 — see [`sinclair_zx81_ula`].
//!
//! # The ZX81
//!
//! Sinclair's £49.95 follow-up to the ZX80 (1981). Around 1.5 million
//! units sold worldwide. Pioneered NMI-driven bus-stealing display
//! generation: the ULA programs the Z80's NMI line at the start of
//! each visible scanline; the NMI handler at `$0066` executes a HALT,
//! and during the refresh cycle the ULA puts character ROM data on the
//! data bus instead of the requested opcode — generating one row of
//! display pixels without dedicated video RAM.
//!
//! - **CPU:** Zilog Z80A at 3.25 MHz
//! - **ROM:** 8 KB at `$0000-$1FFF` (mirrored to `$2000-$3FFF` and
//!   `$8000-$BFFF`)
//! - **RAM:** 1 KB at `$4000-$43FF` (or 16 KB with the RAM pack);
//!   mirrored to `$C000-$FFFF`
//! - **Display:** 32 × 24 characters from D_FILE, rendered via the
//!   ULA's NMI-driven path
//! - **Audio:** none
//! - **Keyboard:** identical 8 × 5 matrix to the Spectrum / ZX80
//!
//! # NMI generator control
//!
//! Per the donor: OUT to an even port (e.g. `$FE`) enables the NMI
//! generator; OUT to a port with bit 1 clear (e.g. `$FD`) disables it.
//! ROM uses OUT($FD)/OUT($FE).

pub mod input;
mod keyboard;

pub use input::Zx81Key;
pub use keyboard::KeyboardState;
pub use sinclair_zx81_ula::{FB_HEIGHT, FB_WIDTH, Zx81Ula};

use zilog_z80::z80::{BusOp, Z80};

/// Sinclair ZX81 machine.
pub struct Zx81 {
    cpu: Z80,
    rom: Vec<u8>,
    ram: Vec<u8>,
    ram_mask: u16,
    ula: Zx81Ula,
    keyboard: KeyboardState,
    nmi_enabled: bool,
    master_clock: u64,
    frame_count: u64,
    /// When `Some`, every I/O port access is appended here (debug trace).
    io_trace: Option<Vec<IoEvent>>,
}

impl Zx81 {
    /// Create a new ZX81. `rom` must be 8 KB. `ram_size` is 1024
    /// (unexpanded) or 16384 (16 KB RAM pack).
    pub fn new(rom: Vec<u8>, ram_size: usize) -> Result<Self, String> {
        if rom.len() != 0x2000 {
            return Err(format!("ZX81 ROM must be 8192 bytes, got {}", rom.len()));
        }
        if !ram_size.is_power_of_two() || ram_size > 0x4000 {
            return Err(format!(
                "ZX81 RAM size must be a power of two <= 16384, got {ram_size}"
            ));
        }
        Ok(Self {
            cpu: Z80::new(),
            rom,
            ram: vec![0; ram_size],
            ram_mask: (ram_size - 1) as u16,
            ula: Zx81Ula::new(),
            keyboard: KeyboardState::new(),
            nmi_enabled: false,
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

        let rom = &self.rom;
        let ram = &self.ram;
        let mask = self.ram_mask;
        self.ula.tick(|addr| match addr {
            0x0000..=0x3FFF => rom[(addr & 0x1FFF) as usize],
            0x4000..=0x7FFF => ram[((addr - 0x4000) & mask) as usize],
            0x8000..=0xBFFF => rom[(addr & 0x1FFF) as usize],
            0xC000..=0xFFFF => ram[((addr - 0xC000) & mask) as usize],
        });

        // ULA NMI line is gated by the NMI generator enable bit.
        self.cpu.nmi = self.ula.nmi_active() && self.nmi_enabled;

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
                self.cpu.data_in = 0xFF;
            }
            None => {}
        }
    }

    fn mem_read(&self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x3FFF | 0x8000..=0xBFFF => self.rom[(addr & 0x1FFF) as usize],
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

    fn io_write(&mut self, port: u16, _value: u8) {
        // OUT to even port enables NMI generator; OUT with bit 1 clear disables it.
        if port & 0x01 == 0 {
            self.nmi_enabled = true;
        }
        if port & 0x02 == 0 {
            self.nmi_enabled = false;
        }
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

    pub fn press_key(&mut self, key: Zx81Key) {
        let (row, bit) = key.matrix();
        self.keyboard.set_key(row, bit, true);
    }

    pub fn release_key(&mut self, key: Zx81Key) {
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
    pub fn nmi_enabled(&self) -> bool {
        self.nmi_enabled
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

#[cfg(test)]
mod tests {
    use super::*;

    fn trap_rom() -> Vec<u8> {
        let mut rom = vec![0u8; 0x2000];
        rom[0] = 0xF3;
        rom[1] = 0x76;
        rom
    }

    #[test]
    fn frame_advances_master_clock_and_count() {
        let mut sys = Zx81::new(trap_rom(), 1024).expect("init");
        let clocks = sys.run_frame();
        assert!(clocks > 0);
        assert_eq!(sys.frame_count(), 1);
    }

    #[test]
    fn rom_size_validated() {
        assert!(Zx81::new(vec![0u8; 4096], 1024).is_err());
    }

    #[test]
    fn ram_mirrors_at_c000() {
        let mut sys = Zx81::new(trap_rom(), 1024).expect("init");
        sys.mem_write(0x4000, 0x42);
        assert_eq!(sys.mem_read(0xC000), 0x42);
    }

    #[test]
    fn nmi_enabled_toggles_via_io() {
        let mut sys = Zx81::new(trap_rom(), 1024).expect("init");
        assert!(!sys.nmi_enabled());
        sys.io_write(0x00FE, 0);
        assert!(sys.nmi_enabled());
        sys.io_write(0x00FD, 0);
        assert!(!sys.nmi_enabled());
    }

    #[test]
    fn keyboard_press_release() {
        let mut sys = Zx81::new(trap_rom(), 1024).expect("init");
        sys.press_key(Zx81Key::A);
        assert_eq!(sys.io_read(0xFDFE) & 0x01, 0x00);
        sys.release_key(Zx81Key::A);
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

impl Zx81 {
    /// Observe one byte on the bus without side effects.
    #[must_use]
    pub fn peek(&self, addr: u16) -> u8 {
        self.mem_read(addr)
    }

    /// Write one byte through the bus (RAM accepts it; ROM ignores it).
    pub fn poke(&mut self, addr: u16, value: u8) {
        self.mem_write(addr, value);
    }

    /// Run exactly one whole Z80 instruction, returning the clocks it
    /// consumed. A safety cap prevents an unbounded spin.
    pub fn step_instruction(&mut self) -> u64 {
        let start = self.master_clock;
        let cap = start + 1024;
        while self.cpu.instruction_complete() && self.master_clock < cap {
            self.tick_tstate();
        }
        while !self.cpu.instruction_complete() && self.master_clock < cap {
            self.tick_tstate();
        }
        self.master_clock - start
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
