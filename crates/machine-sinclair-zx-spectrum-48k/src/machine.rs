//! ZX Spectrum 48K machine-local composition.
//!
//! This crate now owns the first working 48K machine loop: the Z80 and Ferranti
//! ULA are wired together against the 48K memory map and keyboard matrix.

use common_sinclair_zx_spectrum::timing::{SCREEN_HEIGHT, SCREEN_WIDTH, TIMING_48K};
use common_sinclair_zx_spectrum::ula::Ula;
use common_sinclair_zx_spectrum::{MemoryBus, RomImageError, Spectrum48kMemory};
use emu198x_shell::InputEvent;
use ferranti_ula_6c001e::{BoardIssue, FerrantiUla};
use zilog_z80::Z80;

use crate::keyboard::KeyboardMatrix;
use crate::port::TapeInput;

/// Machine-local state for a stock ZX Spectrum 48K.
pub struct Spectrum48k {
    z80: Z80,
    ula: FerrantiUla,
    memory: Spectrum48kMemory,
    keyboard: KeyboardMatrix,
    tape_input: TapeInput,
    framebuffer: Vec<u8>,
    hc: u32,
}

impl Spectrum48k {
    /// Creates an Issue 3 48K machine with deterministic startup state.
    #[must_use]
    pub fn new() -> Self {
        Self::with_issue(BoardIssue::Issue3)
    }

    /// Creates a 48K machine for the requested board issue.
    #[must_use]
    pub fn with_issue(issue: BoardIssue) -> Self {
        Self {
            z80: Z80::new(),
            ula: FerrantiUla::new(issue),
            memory: Spectrum48kMemory::new(),
            keyboard: KeyboardMatrix::new(),
            tape_input: TapeInput::new(),
            framebuffer: vec![0; SCREEN_WIDTH * SCREEN_HEIGHT],
            hc: 0,
        }
    }

    /// Creates a 48K machine with the supplied 16 KiB ROM image.
    #[must_use]
    pub fn with_rom(issue: BoardIssue, rom: [u8; 16 * 1024]) -> Self {
        Self {
            z80: Z80::new(),
            ula: FerrantiUla::new(issue),
            memory: Spectrum48kMemory::with_rom(rom),
            keyboard: KeyboardMatrix::new(),
            tape_input: TapeInput::new(),
            framebuffer: vec![0; SCREEN_WIDTH * SCREEN_HEIGHT],
            hc: 0,
        }
    }

    /// Returns the configured board issue.
    #[must_use]
    pub const fn issue(&self) -> BoardIssue {
        self.ula.issue()
    }

    /// Returns the 48K memory map.
    #[must_use]
    pub fn memory(&self) -> &Spectrum48kMemory {
        &self.memory
    }

    /// Returns mutable access to the 48K memory map.
    #[must_use]
    pub fn memory_mut(&mut self) -> &mut Spectrum48kMemory {
        &mut self.memory
    }

    /// Returns the pin-level Z80 core.
    #[must_use]
    pub fn z80(&self) -> &Z80 {
        &self.z80
    }

    /// Returns mutable access to the pin-level Z80 core.
    #[must_use]
    pub fn z80_mut(&mut self) -> &mut Z80 {
        &mut self.z80
    }

    /// Returns the current framebuffer.
    #[must_use]
    pub fn framebuffer(&self) -> &[u8] {
        &self.framebuffer
    }

    /// Returns mutable framebuffer access.
    #[must_use]
    pub fn framebuffer_mut(&mut self) -> &mut [u8] {
        &mut self.framebuffer
    }

    /// Returns the current half-cycle counter.
    #[must_use]
    pub const fn hc(&self) -> u32 {
        self.hc
    }

    /// Loads a 16 KiB ROM image.
    ///
    /// # Errors
    ///
    /// Returns an error if the ROM is not exactly 16 KiB.
    pub fn load_rom_bytes(&mut self, bytes: &[u8]) -> Result<(), RomImageError> {
        self.memory.load_rom_bytes(bytes)
    }

    /// Returns the keyboard matrix.
    #[must_use]
    pub fn keyboard(&self) -> &KeyboardMatrix {
        &self.keyboard
    }

    /// Returns mutable access to the keyboard matrix.
    #[must_use]
    pub fn keyboard_mut(&mut self) -> &mut KeyboardMatrix {
        &mut self.keyboard
    }

    /// Applies one host input event to the keyboard matrix.
    ///
    /// Returns `true` when the event maps to a physical key.
    pub fn apply_input_event(&mut self, event: &InputEvent) -> bool {
        self.keyboard.apply_input_event(event)
    }

    /// Returns the current tape input line state.
    #[must_use]
    pub const fn tape_input(&self) -> TapeInput {
        self.tape_input
    }

    /// Sets whether the tape input is connected.
    pub fn set_tape_connected(&mut self, connected: bool) {
        self.tape_input.set_connected(connected);
    }

    /// Sets the current tape EAR level.
    pub fn set_tape_level(&mut self, level: bool) {
        self.tape_input.set_level(level);
    }

    /// Returns the current border colour.
    #[must_use]
    pub fn border_color(&self) -> u8 {
        self.ula.border_color()
    }

    /// Writes to port `$FE`.
    pub fn write_fe(&mut self, value: u8) {
        self.ula.write_fe(value);
    }

    /// Reads port `$FE`.
    #[must_use]
    pub fn read_fe(&self, port: u16) -> u8 {
        let mut value = self.ula.read_fe(port, self.keyboard.rows());
        if self.tape_input.connected() {
            value = (value & !0x40) | if self.tape_input.level() { 0x40 } else { 0x00 };
        }
        value
    }

    /// Resets the pin-level CPU and ULA while keeping the loaded ROM and RAM.
    pub fn reset(&mut self) {
        let issue = self.issue();
        self.z80 = Z80::new();
        self.ula = FerrantiUla::new(issue);
        self.hc = 0;
        self.framebuffer.fill(0);
    }

    /// Runs one native 48K video frame.
    pub fn run_frame(&mut self) {
        while self.hc < TIMING_48K.halfcycles_per_frame {
            if self.hc & 1 == 0 {
                self.ula.tick(
                    &self.memory,
                    self.z80.addr,
                    self.z80.mreq,
                    self.z80.iorq,
                    &mut self.framebuffer,
                );

                if self.ula.cpu_clock_active() {
                    self.z80.tick();
                    self.handle_bus();
                }

                self.z80.irq = self.ula.interrupt_active();
            }

            self.hc += 1;
        }

        self.ula.end_frame();
        self.hc -= TIMING_48K.halfcycles_per_frame;
    }

    fn handle_bus(&mut self) {
        if self.z80.mreq && self.z80.rd {
            self.z80.data_in = self.memory.read(self.z80.addr);
        } else if self.z80.mreq && self.z80.wr {
            self.memory.write(self.z80.addr, self.z80.data);
        } else if self.z80.iorq && self.z80.rd && !self.z80.m1 {
            self.z80.data_in = self.io_read(self.z80.addr);
        } else if self.z80.iorq && self.z80.wr {
            self.io_write(self.z80.addr, self.z80.data);
        } else if self.z80.iorq && self.z80.m1 {
            self.z80.data_in = 0xff;
        }
    }

    fn io_read(&self, port: u16) -> u8 {
        if port & 0x01 == 0 {
            self.read_fe(port)
        } else {
            self.ula.floating_bus()
        }
    }

    fn io_write(&mut self, port: u16, data: u8) {
        if port & 0x01 == 0 {
            self.ula.write_fe(data);
        }
    }
}

impl Default for Spectrum48k {
    fn default() -> Self {
        Self::new()
    }
}

impl MemoryBus for Spectrum48k {
    fn read(&self, addr: u16) -> u8 {
        self.memory.read(addr)
    }

    fn write(&mut self, addr: u16, value: u8) {
        self.memory.write(addr, value);
    }

    fn is_contended(&self, addr: u16) -> bool {
        self.memory.is_contended(addr)
    }

    fn read_screen(&self, addr: u16) -> u8 {
        self.memory.read_screen(addr)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keyboard::SpectrumKey;

    #[test]
    fn machine_defaults_to_issue3() {
        let machine = Spectrum48k::new();

        assert_eq!(machine.issue(), BoardIssue::Issue3);
        assert_eq!(machine.border_color(), 7);
        assert_eq!(machine.framebuffer().len(), SCREEN_WIDTH * SCREEN_HEIGHT);
        assert_eq!(machine.read_fe(0xfffe), 0xbf);
    }

    #[test]
    fn machine_loads_rom_and_exposes_memory_bus() {
        let mut machine = Spectrum48k::with_issue(BoardIssue::Issue3);
        let rom = [0xa5; 16 * 1024];

        machine
            .load_rom_bytes(&rom)
            .expect("16 KiB ROM image should load");
        machine.write(0x8000, 0x42);

        assert_eq!(machine.read(0x0000), 0xa5);
        assert_eq!(machine.read(0x3fff), 0xa5);
        assert_eq!(machine.read(0x8000), 0x42);
    }

    #[test]
    fn machine_runs_frame_without_rom() {
        let mut machine = Spectrum48k::new();
        machine.run_frame();

        assert!(machine.z80().regs.pc > 0 || machine.z80().halt);
        assert_eq!(machine.hc(), 0);
    }

    #[test]
    fn machine_applies_host_key_events() {
        let mut machine = Spectrum48k::new();
        let pressed = InputEvent::Key {
            name: "q".into(),
            pressed: true,
        };

        assert!(machine.apply_input_event(&pressed));
        assert_eq!(machine.read_fe(0xfbfe) & 0x01, 0x00);
    }

    #[test]
    fn machine_exposes_issue_specific_feedback() {
        let mut issue2 = Spectrum48k::with_issue(BoardIssue::Issue2);
        let mut issue3 = Spectrum48k::with_issue(BoardIssue::Issue3);

        issue2.write_fe(0x08);
        issue3.write_fe(0x08);

        assert_eq!(issue2.read_fe(0xfffe) & 0x40, 0x40);
        assert_eq!(issue3.read_fe(0xfffe) & 0x40, 0x00);
    }

    #[test]
    fn connected_tape_input_overrides_feedback() {
        let mut machine = Spectrum48k::new();
        machine.write_fe(0x10);
        machine.set_tape_connected(true);
        machine.set_tape_level(false);
        assert_eq!(machine.read_fe(0xfffe) & 0x40, 0x00);

        machine.set_tape_level(true);
        assert_eq!(machine.read_fe(0xfffe) & 0x40, 0x40);
    }

    #[test]
    fn machine_allows_direct_keyboard_access_for_tests() {
        let mut machine = Spectrum48k::new();
        machine.keyboard_mut().press_key(SpectrumKey::Enter);

        assert_eq!(machine.read_fe(0xbffe) & 0x01, 0x00);
    }

    #[test]
    fn unattached_odd_port_reads_idle_floating_bus() {
        let machine = Spectrum48k::new();

        assert_eq!(machine.io_read(0xffff), 0xff);
    }
}
