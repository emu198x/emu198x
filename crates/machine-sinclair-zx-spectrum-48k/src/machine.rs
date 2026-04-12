//! ZX Spectrum 48K machine-local composition.
//!
//! This is intentionally not a full machine runtime yet. It collects the
//! machine-local state that the forthcoming pin-level CPU and ULA will share
//! without inventing a fake execution loop.

use common_sinclair_zx_spectrum::{MemoryBus, RomImageError, Spectrum48kMemory};
use emu198x_shell::InputEvent;

use crate::keyboard::KeyboardMatrix;
use crate::port::{PortFeState, TapeInput};

/// Ferranti ULA board revision for 48K machines.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum BoardIssue {
    Issue2,
    #[default]
    Issue3,
}

/// Machine-local state for a stock ZX Spectrum 48K.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Spectrum48k {
    issue: BoardIssue,
    memory: Spectrum48kMemory,
    keyboard: KeyboardMatrix,
    tape_input: TapeInput,
    port_fe: PortFeState,
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
            issue,
            memory: Spectrum48kMemory::new(),
            keyboard: KeyboardMatrix::new(),
            tape_input: TapeInput::new(),
            port_fe: PortFeState::new(),
        }
    }

    /// Creates a 48K machine with the supplied 16 KiB ROM image.
    #[must_use]
    pub fn with_rom(issue: BoardIssue, rom: [u8; 16 * 1024]) -> Self {
        Self {
            issue,
            memory: Spectrum48kMemory::with_rom(rom),
            keyboard: KeyboardMatrix::new(),
            tape_input: TapeInput::new(),
            port_fe: PortFeState::new(),
        }
    }

    /// Returns the configured board issue.
    #[must_use]
    pub const fn issue(&self) -> BoardIssue {
        self.issue
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

    /// Returns the latched `$FE` output state.
    #[must_use]
    pub const fn port_fe(&self) -> PortFeState {
        self.port_fe
    }

    /// Writes to port `$FE`.
    pub fn write_fe(&mut self, value: u8) {
        self.port_fe.write(value);
    }

    /// Reads port `$FE`.
    #[must_use]
    pub fn read_fe(&self, port: u16) -> u8 {
        self.port_fe
            .read(self.issue, &self.keyboard, self.tape_input, port)
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
        assert_eq!(machine.port_fe().border_color(), 7);
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
}
