//! Dragon 32 machine substrate.
//!
//! This crate owns the reusable board-level state that was first proven in the
//! `emu198x-script-dragon` harness: MC6809 execution, 32 KiB RAM, mirrored
//! 16 KiB BASIC ROM, two MC6821 PIAs, MC6883 SAM latches, keyboard matrix
//! input, and MC6847 text-screen capture. It deliberately stops short of a
//! full native runtime; later runtime crates should build on this API rather
//! than duplicating the harness wiring.

use std::error::Error;
use std::fmt;

use motorola_6809::Mc6809;
use motorola_pia_6821::{Pia6821, PiaPort};
use motorola_sam_6883::Sam6883;
use motorola_vdg_6847::{TextPalette, TextScreen};

/// Dragon 32 RAM size.
pub const RAM_SIZE: usize = 0x8000;
/// Dragon 32 BASIC ROM size.
pub const ROM_SIZE: usize = 0x4000;

/// A raw Dragon keyboard matrix switch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MatrixKey {
    /// Matrix row, `0..8`.
    pub row: usize,
    /// Matrix column, `0..8`.
    pub column: usize,
}

impl MatrixKey {
    /// Create a raw matrix key coordinate.
    #[must_use]
    pub const fn new(row: usize, column: usize) -> Self {
        Self { row, column }
    }

    /// Return the Dragon matrix switch for a semantic key.
    #[must_use]
    pub const fn from_dragon_key(key: DragonKey) -> Self {
        let (row, column) = match key {
            DragonKey::Digit0 => (0, 0),
            DragonKey::Digit1 => (0, 1),
            DragonKey::Digit2 => (0, 2),
            DragonKey::Digit3 => (0, 3),
            DragonKey::Digit4 => (0, 4),
            DragonKey::Digit5 => (0, 5),
            DragonKey::Digit6 => (0, 6),
            DragonKey::Digit7 => (0, 7),
            DragonKey::Digit8 => (1, 0),
            DragonKey::Digit9 => (1, 1),
            DragonKey::Colon => (1, 2),
            DragonKey::Semicolon => (1, 3),
            DragonKey::Comma => (1, 4),
            DragonKey::Minus => (1, 5),
            DragonKey::Period => (1, 6),
            DragonKey::Slash => (1, 7),
            DragonKey::At => (2, 0),
            DragonKey::A => (2, 1),
            DragonKey::B => (2, 2),
            DragonKey::C => (2, 3),
            DragonKey::D => (2, 4),
            DragonKey::E => (2, 5),
            DragonKey::F => (2, 6),
            DragonKey::G => (2, 7),
            DragonKey::H => (3, 0),
            DragonKey::I => (3, 1),
            DragonKey::J => (3, 2),
            DragonKey::K => (3, 3),
            DragonKey::L => (3, 4),
            DragonKey::M => (3, 5),
            DragonKey::N => (3, 6),
            DragonKey::O => (3, 7),
            DragonKey::P => (4, 0),
            DragonKey::Q => (4, 1),
            DragonKey::R => (4, 2),
            DragonKey::S => (4, 3),
            DragonKey::T => (4, 4),
            DragonKey::U => (4, 5),
            DragonKey::V => (4, 6),
            DragonKey::W => (4, 7),
            DragonKey::X => (5, 0),
            DragonKey::Y => (5, 1),
            DragonKey::Z => (5, 2),
            DragonKey::Up => (5, 3),
            DragonKey::Down => (5, 4),
            DragonKey::Left => (5, 5),
            DragonKey::Right => (5, 6),
            DragonKey::Space => (5, 7),
            DragonKey::Enter => (6, 0),
            DragonKey::Clear => (6, 1),
            DragonKey::Break => (6, 2),
            DragonKey::Shift => (6, 7),
        };
        Self { row, column }
    }
}

/// Semantic Dragon keyboard key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DragonKey {
    /// `0`.
    Digit0,
    /// `1`.
    Digit1,
    /// `2`.
    Digit2,
    /// `3`.
    Digit3,
    /// `4`.
    Digit4,
    /// `5`.
    Digit5,
    /// `6`.
    Digit6,
    /// `7`.
    Digit7,
    /// `8`.
    Digit8,
    /// `9`.
    Digit9,
    /// `:`.
    Colon,
    /// `;`.
    Semicolon,
    /// `,`.
    Comma,
    /// `-`.
    Minus,
    /// `.`.
    Period,
    /// `/`.
    Slash,
    /// `@`.
    At,
    /// `A`.
    A,
    /// `B`.
    B,
    /// `C`.
    C,
    /// `D`.
    D,
    /// `E`.
    E,
    /// `F`.
    F,
    /// `G`.
    G,
    /// `H`.
    H,
    /// `I`.
    I,
    /// `J`.
    J,
    /// `K`.
    K,
    /// `L`.
    L,
    /// `M`.
    M,
    /// `N`.
    N,
    /// `O`.
    O,
    /// `P`.
    P,
    /// `Q`.
    Q,
    /// `R`.
    R,
    /// `S`.
    S,
    /// `T`.
    T,
    /// `U`.
    U,
    /// `V`.
    V,
    /// `W`.
    W,
    /// `X`.
    X,
    /// `Y`.
    Y,
    /// `Z`.
    Z,
    /// Up arrow.
    Up,
    /// Down arrow.
    Down,
    /// Left arrow.
    Left,
    /// Right arrow.
    Right,
    /// Space bar.
    Space,
    /// Enter key.
    Enter,
    /// Clear key.
    Clear,
    /// Break key.
    Break,
    /// Shift key.
    Shift,
}

impl DragonKey {
    /// Every physical Dragon keyboard key represented by the 8x8 matrix.
    pub const ALL: [Self; 52] = [
        Self::Digit0,
        Self::Digit1,
        Self::Digit2,
        Self::Digit3,
        Self::Digit4,
        Self::Digit5,
        Self::Digit6,
        Self::Digit7,
        Self::Digit8,
        Self::Digit9,
        Self::Colon,
        Self::Semicolon,
        Self::Comma,
        Self::Minus,
        Self::Period,
        Self::Slash,
        Self::At,
        Self::A,
        Self::B,
        Self::C,
        Self::D,
        Self::E,
        Self::F,
        Self::G,
        Self::H,
        Self::I,
        Self::J,
        Self::K,
        Self::L,
        Self::M,
        Self::N,
        Self::O,
        Self::P,
        Self::Q,
        Self::R,
        Self::S,
        Self::T,
        Self::U,
        Self::V,
        Self::W,
        Self::X,
        Self::Y,
        Self::Z,
        Self::Up,
        Self::Down,
        Self::Left,
        Self::Right,
        Self::Space,
        Self::Enter,
        Self::Clear,
        Self::Break,
        Self::Shift,
    ];

    /// Return the canonical host/runtime label for this Dragon key.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Digit0 => "0",
            Self::Digit1 => "1",
            Self::Digit2 => "2",
            Self::Digit3 => "3",
            Self::Digit4 => "4",
            Self::Digit5 => "5",
            Self::Digit6 => "6",
            Self::Digit7 => "7",
            Self::Digit8 => "8",
            Self::Digit9 => "9",
            Self::Colon => ":",
            Self::Semicolon => ";",
            Self::Comma => ",",
            Self::Minus => "-",
            Self::Period => ".",
            Self::Slash => "/",
            Self::At => "@",
            Self::A => "a",
            Self::B => "b",
            Self::C => "c",
            Self::D => "d",
            Self::E => "e",
            Self::F => "f",
            Self::G => "g",
            Self::H => "h",
            Self::I => "i",
            Self::J => "j",
            Self::K => "k",
            Self::L => "l",
            Self::M => "m",
            Self::N => "n",
            Self::O => "o",
            Self::P => "p",
            Self::Q => "q",
            Self::R => "r",
            Self::S => "s",
            Self::T => "t",
            Self::U => "u",
            Self::V => "v",
            Self::W => "w",
            Self::X => "x",
            Self::Y => "y",
            Self::Z => "z",
            Self::Up => "up",
            Self::Down => "down",
            Self::Left => "left",
            Self::Right => "right",
            Self::Space => "space",
            Self::Enter => "enter",
            Self::Clear => "clear",
            Self::Break => "break",
            Self::Shift => "shift",
        }
    }

    /// Parse a semantic key label used by host-side input and harnesses.
    #[must_use]
    pub fn from_label(value: &str) -> Option<Self> {
        match value {
            "0" => Some(Self::Digit0),
            "1" => Some(Self::Digit1),
            "2" => Some(Self::Digit2),
            "3" => Some(Self::Digit3),
            "4" => Some(Self::Digit4),
            "5" => Some(Self::Digit5),
            "6" => Some(Self::Digit6),
            "7" => Some(Self::Digit7),
            "8" => Some(Self::Digit8),
            "9" => Some(Self::Digit9),
            ":" => Some(Self::Colon),
            ";" => Some(Self::Semicolon),
            "," => Some(Self::Comma),
            "-" => Some(Self::Minus),
            "." => Some(Self::Period),
            "/" => Some(Self::Slash),
            "@" => Some(Self::At),
            " " => Some(Self::Space),
            _ if value.eq_ignore_ascii_case("a") => Some(Self::A),
            _ if value.eq_ignore_ascii_case("b") => Some(Self::B),
            _ if value.eq_ignore_ascii_case("c") => Some(Self::C),
            _ if value.eq_ignore_ascii_case("d") => Some(Self::D),
            _ if value.eq_ignore_ascii_case("e") => Some(Self::E),
            _ if value.eq_ignore_ascii_case("f") => Some(Self::F),
            _ if value.eq_ignore_ascii_case("g") => Some(Self::G),
            _ if value.eq_ignore_ascii_case("h") => Some(Self::H),
            _ if value.eq_ignore_ascii_case("i") => Some(Self::I),
            _ if value.eq_ignore_ascii_case("j") => Some(Self::J),
            _ if value.eq_ignore_ascii_case("k") => Some(Self::K),
            _ if value.eq_ignore_ascii_case("l") => Some(Self::L),
            _ if value.eq_ignore_ascii_case("m") => Some(Self::M),
            _ if value.eq_ignore_ascii_case("n") => Some(Self::N),
            _ if value.eq_ignore_ascii_case("o") => Some(Self::O),
            _ if value.eq_ignore_ascii_case("p") => Some(Self::P),
            _ if value.eq_ignore_ascii_case("q") => Some(Self::Q),
            _ if value.eq_ignore_ascii_case("r") => Some(Self::R),
            _ if value.eq_ignore_ascii_case("s") => Some(Self::S),
            _ if value.eq_ignore_ascii_case("t") => Some(Self::T),
            _ if value.eq_ignore_ascii_case("u") => Some(Self::U),
            _ if value.eq_ignore_ascii_case("v") => Some(Self::V),
            _ if value.eq_ignore_ascii_case("w") => Some(Self::W),
            _ if value.eq_ignore_ascii_case("x") => Some(Self::X),
            _ if value.eq_ignore_ascii_case("y") => Some(Self::Y),
            _ if value.eq_ignore_ascii_case("z") => Some(Self::Z),
            _ if value.eq_ignore_ascii_case("up") => Some(Self::Up),
            _ if value.eq_ignore_ascii_case("down") => Some(Self::Down),
            _ if value.eq_ignore_ascii_case("left") => Some(Self::Left),
            _ if value.eq_ignore_ascii_case("right") => Some(Self::Right),
            _ if value.eq_ignore_ascii_case("space") => Some(Self::Space),
            _ if value.eq_ignore_ascii_case("enter") || value.eq_ignore_ascii_case("return") => {
                Some(Self::Enter)
            }
            _ if value.eq_ignore_ascii_case("clear") || value.eq_ignore_ascii_case("clr") => {
                Some(Self::Clear)
            }
            _ if value.eq_ignore_ascii_case("break") || value.eq_ignore_ascii_case("brk") => {
                Some(Self::Break)
            }
            _ if value.eq_ignore_ascii_case("shift") => Some(Self::Shift),
            _ => None,
        }
    }
}

/// Invalid keyboard matrix coordinate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyboardMatrixError {
    /// Requested row.
    pub row: usize,
    /// Requested column.
    pub column: usize,
}

impl fmt::Display for KeyboardMatrixError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "keyboard matrix key {},{} is outside the 8x8 matrix",
            self.row, self.column
        )
    }
}

impl Error for KeyboardMatrixError {}

/// Dragon keyboard matrix state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DragonKeyboard {
    rows: [u8; 8],
}

impl DragonKeyboard {
    /// Create a keyboard with no keys pressed.
    #[must_use]
    pub const fn new() -> Self {
        Self { rows: [0xFF; 8] }
    }

    /// Create a keyboard with the supplied raw matrix keys held closed.
    ///
    /// # Errors
    ///
    /// Returns [`KeyboardMatrixError`] if any key lies outside the 8x8 matrix.
    pub fn with_pressed_keys(keys: &[MatrixKey]) -> Result<Self, KeyboardMatrixError> {
        let mut keyboard = Self::new();
        for key in keys {
            keyboard.press(*key)?;
        }
        Ok(keyboard)
    }

    /// Hold a raw matrix key closed.
    ///
    /// # Errors
    ///
    /// Returns [`KeyboardMatrixError`] if the key lies outside the 8x8 matrix.
    pub fn press(&mut self, key: MatrixKey) -> Result<(), KeyboardMatrixError> {
        if key.row >= self.rows.len() || key.column >= 8 {
            return Err(KeyboardMatrixError {
                row: key.row,
                column: key.column,
            });
        }

        self.rows[key.row] &= !(1 << key.column);
        Ok(())
    }

    /// Release a raw matrix key.
    ///
    /// # Errors
    ///
    /// Returns [`KeyboardMatrixError`] if the key lies outside the 8x8 matrix.
    pub fn release(&mut self, key: MatrixKey) -> Result<(), KeyboardMatrixError> {
        if key.row >= self.rows.len() || key.column >= 8 {
            return Err(KeyboardMatrixError {
                row: key.row,
                column: key.column,
            });
        }

        self.rows[key.row] |= 1 << key.column;
        Ok(())
    }

    /// Return PIA0 port A row input for the current PIA0 port B column output.
    #[must_use]
    pub fn port_a_input(&self, column_output: u8) -> u8 {
        let selected_columns = !column_output;
        if selected_columns == 0 {
            return 0xFF;
        }

        let mut input = 0xFF;
        for (row, columns) in self.rows.iter().take(7).enumerate() {
            let pressed_columns = !columns;
            if pressed_columns & selected_columns != 0 {
                input &= !(1 << row);
            }
        }
        input
    }
}

impl Default for DragonKeyboard {
    fn default() -> Self {
        Self::new()
    }
}

/// Machine memory/device event observed during a bus cycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryEvent {
    /// Device register read.
    DeviceRead {
        /// Device region.
        device: DeviceRegion,
        /// CPU address.
        addr: u16,
        /// Value returned to the CPU.
        value: u8,
    },
    /// Write into read-only ROM space.
    RomWrite {
        /// CPU address.
        addr: u16,
        /// Value written by the CPU.
        value: u8,
    },
    /// Device register or latch write.
    DeviceWrite {
        /// Device region.
        device: DeviceRegion,
        /// CPU address.
        addr: u16,
        /// Value written by the CPU.
        value: u8,
    },
}

/// Dragon device decode region.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceRegion {
    /// PIA0: keyboard and cassette-side signals.
    Pia0,
    /// PIA1: VDG and audio-side signals.
    Pia1,
    /// SAM write-only latch range.
    Sam,
}

/// Instruction fetch trace entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FetchTrace {
    /// Bus cycle when the opcode fetch began.
    pub cycle: u64,
    /// Program counter.
    pub pc: u16,
    /// Opcode byte.
    pub opcode: u8,
}

/// Read-only write trace entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReadonlyWrite {
    /// Bus cycle.
    pub cycle: u64,
    /// CPU address.
    pub addr: u16,
    /// Written value.
    pub value: u8,
}

/// Device access trace entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeviceAccess {
    /// Bus cycle.
    pub cycle: u64,
    /// `true` for read, `false` for write.
    pub rw: bool,
    /// Device region.
    pub device: DeviceRegion,
    /// CPU address.
    pub addr: u16,
    /// Bus value.
    pub value: u8,
}

/// Reason a bounded machine run stopped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopReason {
    /// Requested cycle limit was reached.
    CycleLimit,
    /// CPU entered its halt state.
    CpuHalted,
}

/// Summary of a bounded Dragon machine run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunReport {
    /// Stop reason.
    pub stop_reason: StopReason,
    /// Bus cycles executed during this run.
    pub cycles: u64,
    /// Instruction-boundary opcode fetches observed during this run.
    pub instructions: u64,
    /// Current CPU PC.
    pub pc: u16,
    /// Current CPU bus address.
    pub addr: u16,
    /// Current CPU bus direction.
    pub rw: bool,
    /// Last opcode fetch observed.
    pub last_fetch: Option<FetchTrace>,
    /// Retained opcode fetch trace.
    pub trace: Vec<FetchTrace>,
    /// Number of fetch trace entries dropped due to the trace limit.
    pub dropped_trace: usize,
    /// Retained device accesses.
    pub device_accesses: Vec<DeviceAccess>,
    /// Number of device access entries dropped due to the trace limit.
    pub dropped_device_accesses: usize,
    /// Retained read-only writes.
    pub readonly_writes: Vec<ReadonlyWrite>,
    /// Number of read-only write entries dropped due to the trace limit.
    pub dropped_readonly_writes: usize,
    /// SAM-selected text screen base at the end of the run.
    pub text_screen_base: u16,
}

#[derive(Debug, Clone)]
struct DragonMemory {
    ram: Box<[u8; RAM_SIZE]>,
    rom: Box<[u8; ROM_SIZE]>,
    pia0: Pia6821,
    pia1: Pia6821,
    sam: Sam6883,
    keyboard: DragonKeyboard,
}

impl DragonMemory {
    fn new_with_keyboard(rom: &[u8; ROM_SIZE], keyboard: DragonKeyboard) -> Self {
        Self {
            ram: Box::new([0; RAM_SIZE]),
            rom: Box::new(*rom),
            pia0: Pia6821::new(),
            pia1: Pia6821::new(),
            sam: Sam6883::new(),
            keyboard,
        }
    }

    fn read_fetch(&self, addr: u16) -> u8 {
        let addr = usize::from(addr);
        if addr < RAM_SIZE {
            self.ram[addr]
        } else {
            self.rom[(addr - RAM_SIZE) & (ROM_SIZE - 1)]
        }
    }

    fn read_bus(&mut self, addr: u16) -> (u8, Option<MemoryEvent>) {
        if let Some((device, offset)) = decode_pia(addr) {
            self.refresh_pia_inputs();
            let value = match device {
                DeviceRegion::Pia0 => self.pia0.read(offset),
                DeviceRegion::Pia1 => self.pia1.read(offset),
                DeviceRegion::Sam => unreachable!("SAM is not a PIA"),
            };
            return (
                value,
                Some(MemoryEvent::DeviceRead {
                    device,
                    addr,
                    value,
                }),
            );
        }

        (self.read_fetch(addr), None)
    }

    fn write(&mut self, addr: u16, value: u8) -> Option<MemoryEvent> {
        let index = usize::from(addr);
        if index < RAM_SIZE {
            self.ram[index] = value;
            None
        } else if let Some((device, offset)) = decode_pia(addr) {
            match device {
                DeviceRegion::Pia0 => {
                    self.pia0.write(offset, value);
                    self.refresh_pia_inputs();
                }
                DeviceRegion::Pia1 => self.pia1.write(offset, value),
                DeviceRegion::Sam => unreachable!("SAM is not a PIA"),
            }
            Some(MemoryEvent::DeviceWrite {
                device,
                addr,
                value,
            })
        } else if let Some(device) = decode_device_write(addr) {
            if device == DeviceRegion::Sam {
                self.sam.write(addr);
            }
            Some(MemoryEvent::DeviceWrite {
                device,
                addr,
                value,
            })
        } else {
            Some(MemoryEvent::RomWrite { addr, value })
        }
    }

    fn refresh_pia_inputs(&mut self) {
        let column_output = self.pia0.output_latch(PiaPort::B) | !self.pia0.ddr(PiaPort::B);
        self.pia0
            .set_input(PiaPort::A, self.keyboard.port_a_input(column_output));
        self.pia0.set_input(PiaPort::B, 0xFF);
        self.pia1.set_input(PiaPort::A, 0xFF);
        self.pia1.set_input(PiaPort::B, 0xFF);
    }

    fn text_screen_base(&self) -> u16 {
        self.sam.display_base()
    }

    fn capture_text_screen(&self) -> TextScreen {
        let base = usize::from(self.text_screen_base());
        TextScreen::capture(|offset| self.ram[(base + offset) & (RAM_SIZE - 1)])
    }
}

/// Tickable Dragon 32 machine.
#[derive(Debug, Clone)]
pub struct Dragon32 {
    cpu: Mc6809,
    memory: DragonMemory,
    cycles: u64,
    instructions: u64,
}

impl Dragon32 {
    /// Build and reset a Dragon 32 around a 16 KiB BASIC ROM.
    #[must_use]
    pub fn new(rom: &[u8; ROM_SIZE]) -> Self {
        Self::new_with_keyboard(rom, DragonKeyboard::new())
    }

    /// Build and reset a Dragon 32 with a specific keyboard matrix state.
    #[must_use]
    pub fn new_with_keyboard(rom: &[u8; ROM_SIZE], keyboard: DragonKeyboard) -> Self {
        let mut machine = Self {
            cpu: Mc6809::new(),
            memory: DragonMemory::new_with_keyboard(rom, keyboard),
            cycles: 0,
            instructions: 0,
        };
        machine.cpu.reset();
        machine
    }

    /// Current CPU program counter.
    #[must_use]
    pub fn pc(&self) -> u16 {
        self.cpu.regs.pc
    }

    /// Current CPU bus address.
    #[must_use]
    pub fn bus_addr(&self) -> u16 {
        self.cpu.addr
    }

    /// Current CPU bus direction, `true` for read.
    #[must_use]
    pub fn bus_rw(&self) -> bool {
        self.cpu.rw
    }

    /// Total bus cycles executed since construction.
    #[must_use]
    pub fn cycles(&self) -> u64 {
        self.cycles
    }

    /// Total instruction-boundary opcode fetches since construction.
    #[must_use]
    pub fn instructions(&self) -> u64 {
        self.instructions
    }

    /// Returns whether the CPU has halted.
    #[must_use]
    pub fn is_halted(&self) -> bool {
        self.cpu.halt
    }

    /// Replace keyboard matrix state.
    pub fn set_keyboard(&mut self, keyboard: DragonKeyboard) {
        self.memory.keyboard = keyboard;
        self.memory.refresh_pia_inputs();
    }

    /// Mutable access to keyboard matrix state.
    pub fn keyboard_mut(&mut self) -> &mut DragonKeyboard {
        &mut self.memory.keyboard
    }

    /// Current SAM-selected text screen base.
    #[must_use]
    pub fn text_screen_base(&self) -> u16 {
        self.memory.text_screen_base()
    }

    /// Capture the current MC6847 32x16 text screen.
    #[must_use]
    pub fn capture_text_screen(&self) -> TextScreen {
        self.memory.capture_text_screen()
    }

    /// Render the current text screen plus border as ARGB8888.
    #[must_use]
    pub fn render_visible_text_argb(&self, palette: TextPalette) -> Vec<u32> {
        self.capture_text_screen().render_visible_argb(palette)
    }

    /// Execute one bus cycle and return any observed memory/device event.
    pub fn step_cycle(&mut self) -> Option<MemoryEvent> {
        let event = if self.cpu.rw {
            let (value, event) = self.memory.read_bus(self.cpu.addr);
            self.cpu.data_in = value;
            event
        } else {
            self.memory.write(self.cpu.addr, self.cpu.data)
        };
        self.cpu.tick();
        self.cycles = self.cycles.saturating_add(1);
        event
    }

    /// Execute up to `cycle_limit` bus cycles and retain bounded diagnostic
    /// traces.
    #[must_use]
    pub fn run_cycles(&mut self, cycle_limit: u64, trace_limit: usize) -> RunReport {
        let mut trace = Vec::new();
        let mut dropped_trace = 0usize;
        let mut device_accesses = Vec::new();
        let mut dropped_device_accesses = 0usize;
        let mut readonly_writes = Vec::new();
        let mut dropped_readonly_writes = 0usize;
        let mut last_fetch = None;
        let mut run_instructions = 0u64;
        let mut run_cycles = 0u64;
        let mut stop_reason = StopReason::CycleLimit;

        for run_cycle in 0..cycle_limit {
            if self.cpu.instruction_boundary() && self.cpu.rw {
                let fetch = FetchTrace {
                    cycle: run_cycle,
                    pc: self.cpu.addr,
                    opcode: self.memory.read_fetch(self.cpu.addr),
                };
                last_fetch = Some(fetch);
                self.instructions = self.instructions.saturating_add(1);
                run_instructions = run_instructions.saturating_add(1);
                retain_trace(&mut trace, &mut dropped_trace, trace_limit, fetch);
            }

            let event = self.step_cycle();
            run_cycles = run_cycle.saturating_add(1);

            match event {
                Some(MemoryEvent::DeviceRead {
                    device,
                    addr,
                    value,
                }) => {
                    retain_device_access(
                        &mut device_accesses,
                        &mut dropped_device_accesses,
                        trace_limit,
                        DeviceAccess {
                            cycle: run_cycle,
                            rw: true,
                            device,
                            addr,
                            value,
                        },
                    );
                }
                Some(MemoryEvent::RomWrite { addr, value }) => {
                    retain_readonly_write(
                        &mut readonly_writes,
                        &mut dropped_readonly_writes,
                        trace_limit,
                        ReadonlyWrite {
                            cycle: run_cycle,
                            addr,
                            value,
                        },
                    );
                }
                Some(MemoryEvent::DeviceWrite {
                    device,
                    addr,
                    value,
                }) => {
                    retain_device_access(
                        &mut device_accesses,
                        &mut dropped_device_accesses,
                        trace_limit,
                        DeviceAccess {
                            cycle: run_cycle,
                            rw: false,
                            device,
                            addr,
                            value,
                        },
                    );
                }
                None => {}
            }

            if self.cpu.halt {
                stop_reason = StopReason::CpuHalted;
                break;
            }
        }

        RunReport {
            stop_reason,
            cycles: run_cycles,
            instructions: run_instructions,
            pc: self.cpu.regs.pc,
            addr: self.cpu.addr,
            rw: self.cpu.rw,
            last_fetch,
            trace,
            dropped_trace,
            device_accesses,
            dropped_device_accesses,
            readonly_writes,
            dropped_readonly_writes,
            text_screen_base: self.text_screen_base(),
        }
    }
}

fn retain_trace(
    trace: &mut Vec<FetchTrace>,
    dropped_trace: &mut usize,
    trace_limit: usize,
    fetch: FetchTrace,
) {
    if trace_limit == 0 {
        *dropped_trace = dropped_trace.saturating_add(1);
        return;
    }

    if trace.len() == trace_limit {
        trace.remove(0);
        *dropped_trace = dropped_trace.saturating_add(1);
    }
    trace.push(fetch);
}

fn retain_device_access(
    accesses: &mut Vec<DeviceAccess>,
    dropped_accesses: &mut usize,
    access_limit: usize,
    access: DeviceAccess,
) {
    if access_limit == 0 {
        *dropped_accesses = dropped_accesses.saturating_add(1);
        return;
    }

    if accesses.len() == access_limit {
        accesses.remove(0);
        *dropped_accesses = dropped_accesses.saturating_add(1);
    }
    accesses.push(access);
}

fn retain_readonly_write(
    writes: &mut Vec<ReadonlyWrite>,
    dropped_writes: &mut usize,
    write_limit: usize,
    write: ReadonlyWrite,
) {
    if write_limit == 0 {
        *dropped_writes = dropped_writes.saturating_add(1);
        return;
    }

    if writes.len() == write_limit {
        writes.remove(0);
        *dropped_writes = dropped_writes.saturating_add(1);
    }
    writes.push(write);
}

fn decode_pia(addr: u16) -> Option<(DeviceRegion, u8)> {
    match addr {
        0xFF00..=0xFF1F => Some((DeviceRegion::Pia0, (addr & 0x03) as u8)),
        0xFF20..=0xFF3F => Some((DeviceRegion::Pia1, (addr & 0x03) as u8)),
        _ => None,
    }
}

fn decode_device_write(addr: u16) -> Option<DeviceRegion> {
    match addr {
        0xFFC0..=0xFFDF => Some(DeviceRegion::Sam),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use motorola_vdg_6847::{TEXT_VISIBLE_FRAMEBUFFER_HEIGHT, TEXT_VISIBLE_FRAMEBUFFER_WIDTH};

    use super::*;

    fn rom_with_reset_vector(pc: u16) -> [u8; ROM_SIZE] {
        let mut rom = [0; ROM_SIZE];
        let [hi, lo] = pc.to_be_bytes();
        rom[0x3FFE] = hi;
        rom[0x3FFF] = lo;
        rom
    }

    #[test]
    fn memory_maps_rom_and_vector_mirror() {
        let mut rom = rom_with_reset_vector(0x8000);
        rom[0] = 0x12;
        rom[0x3FFE] = 0x80;
        rom[0x3FFF] = 0x00;

        let memory = DragonMemory::new_with_keyboard(&rom, DragonKeyboard::new());

        assert_eq!(memory.read_fetch(0x8000), 0x12);
        assert_eq!(memory.read_fetch(0xC000), 0x12);
        assert_eq!(memory.read_fetch(0xFFFE), 0x80);
        assert_eq!(memory.read_fetch(0xFFFF), 0x00);
    }

    #[test]
    fn machine_records_device_writes_without_stopping() {
        let mut rom = rom_with_reset_vector(0x8000);
        rom[0x0000] = 0x86; // LDA #$55
        rom[0x0001] = 0x55;
        rom[0x0002] = 0xB7; // STA $FF00
        rom[0x0003] = 0xFF;
        rom[0x0004] = 0x00;
        rom[0x0005] = 0x01; // Illegal opcode stop after the write.

        let mut machine = Dragon32::new(&rom);
        let report = machine.run_cycles(64, 8);

        assert_eq!(report.stop_reason, StopReason::CpuHalted);
        assert_eq!(report.device_accesses.len(), 1);
        assert_eq!(
            report.device_accesses[0],
            DeviceAccess {
                cycle: 7,
                rw: false,
                device: DeviceRegion::Pia0,
                addr: 0xFF00,
                value: 0x55,
            }
        );
    }

    #[test]
    fn machine_records_device_reads_without_stopping() {
        let mut rom = rom_with_reset_vector(0x8000);
        rom[0x0000] = 0xB6; // LDA $FF00
        rom[0x0001] = 0xFF;
        rom[0x0002] = 0x00;
        rom[0x0003] = 0x01;

        let mut machine = Dragon32::new(&rom);
        let report = machine.run_cycles(64, 8);

        assert_eq!(report.stop_reason, StopReason::CpuHalted);
        assert_eq!(report.device_accesses.len(), 1);
        assert_eq!(
            report.device_accesses[0],
            DeviceAccess {
                cycle: 5,
                rw: true,
                device: DeviceRegion::Pia0,
                addr: 0xFF00,
                value: 0x00,
            }
        );
        assert_eq!(
            report.last_fetch,
            Some(FetchTrace {
                cycle: 6,
                pc: 0x8003,
                opcode: 0x01,
            })
        );
    }

    #[test]
    fn machine_records_readonly_rom_writes_without_stopping() {
        let mut rom = rom_with_reset_vector(0x8000);
        rom[0x0000] = 0x86; // LDA #$55
        rom[0x0001] = 0x55;
        rom[0x0002] = 0xB7; // STA $9000
        rom[0x0003] = 0x90;
        rom[0x0004] = 0x00;
        rom[0x0005] = 0x01;

        let mut machine = Dragon32::new(&rom);
        let report = machine.run_cycles(64, 8);

        assert_eq!(report.stop_reason, StopReason::CpuHalted);
        assert_eq!(report.readonly_writes.len(), 1);
        assert_eq!(
            report.readonly_writes[0],
            ReadonlyWrite {
                cycle: 7,
                addr: 0x9000,
                value: 0x55,
            }
        );
    }

    #[test]
    fn keyboard_returns_high_when_no_columns_are_selected() {
        let mut keyboard = DragonKeyboard::new();
        keyboard
            .press(MatrixKey::new(2, 3))
            .expect("matrix key should be valid");

        assert_eq!(keyboard.port_a_input(0xFF), 0xFF);
    }

    #[test]
    fn keyboard_pulls_row_low_when_selected_column_has_pressed_key() {
        let mut keyboard = DragonKeyboard::new();
        keyboard
            .press(MatrixKey::new(2, 3))
            .expect("matrix key should be valid");

        assert_eq!(keyboard.port_a_input(0xF7), 0xFB);
    }

    #[test]
    fn keyboard_ands_multiple_selected_columns() {
        let keyboard =
            DragonKeyboard::with_pressed_keys(&[MatrixKey::new(1, 2), MatrixKey::new(3, 5)])
                .expect("matrix keys should be valid");

        assert_eq!(keyboard.port_a_input(0xDB), 0xF5);
    }

    #[test]
    fn keyboard_rejects_out_of_range_matrix_keys() {
        let err = DragonKeyboard::with_pressed_keys(&[MatrixKey::new(8, 0)])
            .expect_err("row 8 should be invalid");

        assert_eq!(err, KeyboardMatrixError { row: 8, column: 0 });
    }

    #[test]
    fn memory_feeds_keyboard_matrix_through_pia0_port_a() {
        let rom = rom_with_reset_vector(0x8000);
        let keyboard = DragonKeyboard::with_pressed_keys(&[MatrixKey::new(0, 1)])
            .expect("matrix key should be valid");
        let mut memory = DragonMemory::new_with_keyboard(&rom, keyboard);

        memory.write(0xFF02, 0xFF); // PIA0 port B DDR: all column-drive bits output.
        memory.write(0xFF03, 0x04); // PIA0 port B data register selected.
        memory.write(0xFF02, 0xFD); // Drive column 1 low.
        memory.write(0xFF01, 0x04); // PIA0 port A data register selected.

        let (value, event) = memory.read_bus(0xFF00);

        assert_eq!(value, 0xFE);
        assert_eq!(
            event,
            Some(MemoryEvent::DeviceRead {
                device: DeviceRegion::Pia0,
                addr: 0xFF00,
                value: 0xFE,
            })
        );
    }

    #[test]
    fn machine_reports_halt_for_illegal_opcode() {
        let mut rom = rom_with_reset_vector(0x8000);
        rom[0x0000] = 0x01;

        let mut machine = Dragon32::new(&rom);
        let report = machine.run_cycles(16, 4);

        assert_eq!(report.stop_reason, StopReason::CpuHalted);
        assert_eq!(
            report.last_fetch,
            Some(FetchTrace {
                cycle: 2,
                pc: 0x8000,
                opcode: 0x01,
            })
        );
    }

    #[test]
    fn machine_can_dump_sam_selected_text_screen() {
        let mut rom = rom_with_reset_vector(0x8000);
        rom[0x0000] = 0x86; // LDA #$01
        rom[0x0001] = 0x01;
        rom[0x0002] = 0xB7; // STA $FFC9: set SAM F1, selecting text base $0400.
        rom[0x0003] = 0xFF;
        rom[0x0004] = 0xC9;
        rom[0x0005] = 0xB7; // STA $0400: MC6847 diagnostic 'A'.
        rom[0x0006] = 0x04;
        rom[0x0007] = 0x00;
        rom[0x0008] = 0x86; // LDA #$02
        rom[0x0009] = 0x02;
        rom[0x000A] = 0xB7; // STA $0401: MC6847 diagnostic 'B'.
        rom[0x000B] = 0x04;
        rom[0x000C] = 0x01;
        rom[0x000D] = 0x01;

        let mut machine = Dragon32::new(&rom);
        let report = machine.run_cycles(128, 8);
        let text_screen = machine.capture_text_screen();

        assert_eq!(report.stop_reason, StopReason::CpuHalted);
        assert_eq!(report.text_screen_base, 0x0400);
        assert_eq!(
            text_screen.to_plain_text().lines().next(),
            Some("AB@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@")
        );
        assert_eq!(
            machine
                .render_visible_text_argb(TextPalette::default())
                .len(),
            TEXT_VISIBLE_FRAMEBUFFER_WIDTH * TEXT_VISIBLE_FRAMEBUFFER_HEIGHT
        );
    }

    #[test]
    fn dragon_key_labels_map_to_confirmed_matrix_positions() {
        assert_eq!(DragonKey::ALL.len(), 52);
        assert_eq!(
            MatrixKey::from_dragon_key(DragonKey::from_label("a").expect("A should parse")),
            MatrixKey::new(2, 1)
        );
        assert_eq!(
            MatrixKey::from_dragon_key(DragonKey::from_label("A").expect("A should parse")),
            MatrixKey::new(2, 1)
        );
        assert_eq!(
            MatrixKey::from_dragon_key(DragonKey::from_label("@").expect("@ should parse")),
            MatrixKey::new(2, 0)
        );
        assert_eq!(
            MatrixKey::from_dragon_key(DragonKey::from_label("space").expect("space should parse")),
            MatrixKey::new(5, 7)
        );
        assert_eq!(
            MatrixKey::from_dragon_key(DragonKey::from_label("right").expect("right should parse")),
            MatrixKey::new(5, 6)
        );

        for key in DragonKey::ALL {
            assert_eq!(DragonKey::from_label(key.label()), Some(key));
        }
    }

    #[test]
    fn dragon_key_parser_accepts_control_key_aliases() {
        assert_eq!(DragonKey::from_label("return"), Some(DragonKey::Enter));
        assert_eq!(DragonKey::from_label("clr"), Some(DragonKey::Clear));
        assert_eq!(DragonKey::from_label("brk"), Some(DragonKey::Break));
    }
}
