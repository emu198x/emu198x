//! Dragon 32 machine substrate.
//!
//! This crate owns the reusable board-level state that was first proven in the
//! `emu198x-script-dragon` harness: MC6809 execution, 32 KiB RAM, mirrored
//! 16 KiB BASIC ROM, SAM-backed RAM paging, two MC6821 PIAs, MC6883 SAM
//! latches, keyboard matrix input, and MC6847 text-screen capture. It
//! deliberately stops short of a full native runtime; later runtime crates
//! should build on this API rather than duplicating the harness wiring.

use std::collections::VecDeque;
use std::error::Error;
use std::fmt;

use motorola_6809::{Mc6809, Mc6809ClockPhase};
use motorola_pia_6821::{Pia6821, PiaPort, PiaSignal};
use motorola_sam_6883::Sam6883;
use motorola_vdg_6847::{
    TEXT_VISIBLE_FRAMEBUFFER_HEIGHT, TEXT_VISIBLE_FRAMEBUFFER_PIXELS, TextPalette, TextScreen,
    VdgControl, VdgPalette,
};

/// Dragon 32 RAM size.
pub const RAM_SIZE: usize = 0x8000;
const FULL_RAM_SIZE: usize = 0x10000;
/// Dragon 32 BASIC ROM size.
pub const ROM_SIZE: usize = 0x4000;
/// Hardware stack pointer observed after Dragon 32 BASIC reaches the `OK` prompt.
const DIRECT_PROGRAM_STACK_POINTER: u16 = 0x7f2a;
/// Host audio sample rate for Dragon mono output.
pub const DRAGON_AUDIO_SAMPLE_RATE: u32 = 48_000;
/// Dragon/XRoar master event tick frequency.
pub const DRAGON_MASTER_HZ: u64 = 14_318_180;
const SLOW_CPU_MASTER_TICKS: u64 = 16;
const CASSETTE_ZERO_HALF_PERIOD_TICKS: u64 = 373 * SLOW_CPU_MASTER_TICKS;
const CASSETTE_ONE_HALF_PERIOD_TICKS: u64 = 186 * SLOW_CPU_MASTER_TICKS;
const CASSETTE_BITS_PER_BYTE: usize = 8;
/// Nominal Dragon 32 MC6809 bus frequency.
pub const DRAGON_CPU_HZ: u64 = 894_886;
const DRAGON_FRAME_HZ: u64 = 50;
const VDG_CLOCK_MASTER_TICKS: u64 = 4;
const VDG_SHORT_CYCLE_FETCH_CLOCKS: u64 = 4;
const VDG_LONG_CYCLE_FETCH_CLOCKS: u64 = 8;
const VDG_LINE_MASTER_TICKS: u64 = 912;
const VDG_PAL_FRAME_LINES: u64 = 312;
const VDG_VISIBLE_FIRST_LINE: u64 = 13;
const VDG_HSYNC_TICKS: u64 = 64;
const VDG_BACK_PORCH_TICKS: u64 = 70;
const VDG_LEFT_BORDER_TICKS: u64 = 120;
const VDG_ACTIVE_AREA_START_LINE: u64 =
    VDG_VISIBLE_FIRST_LINE + motorola_vdg_6847::TEXT_TOP_BORDER_LINES as u64;
const VDG_ACTIVE_AREA_END_LINE: u64 =
    VDG_ACTIVE_AREA_START_LINE + motorola_vdg_6847::TEXT_FRAMEBUFFER_HEIGHT as u64;
const VDG_FRAME_SYNC_FALL_TICK: u64 = VDG_ACTIVE_AREA_END_LINE * VDG_LINE_MASTER_TICKS;
const DRAGON_FRAME_MASTER_TICKS: u64 = VDG_LINE_MASTER_TICKS * VDG_PAL_FRAME_LINES;
const VDG_NTSC_LOGICAL_FRAME_LINES: u64 = 262;
const VDG_DRAGON_PAL_FIRST_PADDING_LINES: u64 = 25;
const VDG_FRAME_SYNC_RISE_TICK: u64 =
    (VDG_NTSC_LOGICAL_FRAME_LINES + VDG_DRAGON_PAL_FIRST_PADDING_LINES) * VDG_LINE_MASTER_TICKS;
/// Number of MC6809 bus cycles in one PAL VDG video frame.
pub const DRAGON_FRAME_CYCLES: u64 = DRAGON_FRAME_MASTER_TICKS / SLOW_CPU_MASTER_TICKS;
const CART_ROM_START: u16 = 0xC000;
const CART_ROM_END: u16 = 0xFEFF;
const CART_IO_START: u16 = 0xFF40;
const CART_IO_END: u16 = 0xFF5F;
const NO_CARTRIDGE_BUS_VALUE: u8 = 0xFF;
const CART_AUTORUN_FIRQ_CYCLES: u64 = DRAGON_CPU_HZ / 10;
const ACIA_STATUS_TRANSMIT_DATA_REGISTER_EMPTY: u8 = 0x10;

/// Dragon board-level memory decode variant.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DragonHardwareModel {
    /// Dragon 32: 32 KiB base RAM, internal ROM at `$8000-$BFFF`, cartridge ROM at `$C000-$FEFF`.
    Dragon32,
    /// Dragon 64 after cold reset: Dragon 32-compatible memory map plus the onboard ACIA.
    Dragon64Compat,
    /// Dragon 64 64K mode: 48 KiB base RAM, system ROM at `$C000-$FEFF`, vectors in the system ROM page.
    Dragon64Mode,
}

const XROAR_AUDIO_MAX_V: f32 = 4.70;
const XROAR_AUDIO_OUTPUT_GAIN: f32 = 0.7;
const XROAR_AUDIO_SOURCE_GAIN: [[f32; 3]; 6] = [
    [
        4.50 / XROAR_AUDIO_MAX_V,
        2.84 / XROAR_AUDIO_MAX_V,
        3.40 / XROAR_AUDIO_MAX_V,
    ],
    [
        0.50 / XROAR_AUDIO_MAX_V,
        0.40 / XROAR_AUDIO_MAX_V,
        0.50 / XROAR_AUDIO_MAX_V,
    ],
    [
        4.70 / XROAR_AUDIO_MAX_V,
        2.84 / XROAR_AUDIO_MAX_V,
        3.40 / XROAR_AUDIO_MAX_V,
    ],
    [0.0, 0.0, 0.0],
    [0.0, 0.0, 0.0],
    [0.0, 0.0, 0.0],
];
const XROAR_AUDIO_SOURCE_OFFSET: [[f32; 3]; 6] = [
    [
        0.20 / XROAR_AUDIO_MAX_V,
        0.18 / XROAR_AUDIO_MAX_V,
        1.30 / XROAR_AUDIO_MAX_V,
    ],
    [
        2.05 / XROAR_AUDIO_MAX_V,
        1.60 / XROAR_AUDIO_MAX_V,
        2.35 / XROAR_AUDIO_MAX_V,
    ],
    [
        0.00 / XROAR_AUDIO_MAX_V,
        0.18 / XROAR_AUDIO_MAX_V,
        1.30 / XROAR_AUDIO_MAX_V,
    ],
    [0.0, 0.0, 0.0],
    [0.0, 0.0, 0.0],
    [0.0, 0.0, 3.90 / XROAR_AUDIO_MAX_V],
];
/// Minimum Dragon analogue joystick axis value.
pub const DRAGON_JOYSTICK_MIN: u16 = 0x0000;
/// Centre Dragon analogue joystick axis value.
pub const DRAGON_JOYSTICK_CENTER: u16 = 0x8000;
/// Maximum Dragon analogue joystick axis value.
pub const DRAGON_JOYSTICK_MAX: u16 = 0xFFFF;

fn joystick_threshold_from_dac(value: u8) -> u16 {
    u16::from((value & 0xFC) | 0x02) << 8
}

/// Dragon cartridge hardware model.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DragonCartridgeKind {
    /// Plain ROM cartridge mapped at `$C000-$FEFF`.
    Rom,
    /// Games Master Cartridge style 16 KiB banked cartridge.
    GamesMaster,
}

/// Register state loaded from a PC-Dragon `.pak` snapshot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DragonSnapshotRegisters {
    /// Program counter.
    pub pc: u16,
    /// X index register.
    pub x: u16,
    /// Y index register.
    pub y: u16,
    /// U stack pointer.
    pub u: u16,
    /// S stack pointer.
    pub s: u16,
    /// Direct page register.
    pub dp: u8,
    /// B accumulator.
    pub b: u8,
    /// A accumulator.
    pub a: u8,
    /// Condition-code register.
    pub cc: u8,
}

/// Peripheral state loaded from a PC-Dragon `.pak` snapshot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DragonSnapshotPeripherals {
    /// Stored value for `$FF02`, Dragon PIA0 port B.
    pub ff02: u8,
    /// Stored value for `$FF03`, Dragon PIA0 port B control.
    pub ff03: u8,
    /// Stored value for `$FF22`, Dragon PIA1 port B.
    pub ff22: u8,
}

/// Failure while directly loading a Dragon binary program into RAM.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DragonProgramLoadError {
    /// The program payload would write outside the Dragon 32 RAM range.
    RamOverflow {
        /// Target load address.
        load_address: u16,
        /// Payload size in bytes.
        len: usize,
    },
}

impl fmt::Display for DragonProgramLoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::RamOverflow { load_address, len } => write!(
                f,
                "Dragon binary payload of {len} bytes at ${load_address:04X} exceeds 32 KiB RAM"
            ),
        }
    }
}

impl Error for DragonProgramLoadError {}

#[derive(Debug, Clone)]
struct DragonCartridge {
    kind: DragonCartridgeKind,
    rom: Box<[u8]>,
    bank: usize,
    bank_mask: usize,
    autorun: bool,
    next_firq_cycle: u64,
}

impl DragonCartridge {
    fn new(kind: DragonCartridgeKind, rom: &[u8], autorun: bool) -> Self {
        let bank_mask = if matches!(kind, DragonCartridgeKind::GamesMaster) {
            rom.len().saturating_sub(1) & 0x3c000
        } else {
            0
        };
        Self {
            kind,
            rom: rom.into(),
            bank: 0,
            bank_mask,
            autorun,
            next_firq_cycle: CART_AUTORUN_FIRQ_CYCLES,
        }
    }

    fn read_rom(&self, addr: u16) -> u8 {
        let offset = match self.kind {
            DragonCartridgeKind::Rom => usize::from(addr - CART_ROM_START),
            DragonCartridgeKind::GamesMaster => self.bank | (usize::from(addr) & 0x3fff),
        };
        self.rom.get(offset).copied().unwrap_or(0xff)
    }

    fn write_io(&mut self, addr: u16, value: u8) {
        if matches!(self.kind, DragonCartridgeKind::GamesMaster) && addr & 1 == 0 {
            self.bank = (usize::from(value) << 14) & self.bank_mask;
        }
    }

    fn should_signal_autorun(&mut self, cycles: u64) -> bool {
        if !self.autorun || cycles < self.next_firq_cycle {
            return false;
        }
        self.next_firq_cycle = self
            .next_firq_cycle
            .saturating_add(CART_AUTORUN_FIRQ_CYCLES);
        true
    }
}

/// Dragon analogue joystick axis.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DragonJoystickAxis {
    /// Horizontal axis.
    X,
    /// Vertical axis.
    Y,
}

impl DragonJoystickAxis {
    const fn index(self) -> usize {
        match self {
            Self::X => 0,
            Self::Y => 1,
        }
    }
}

/// Error returned when addressing a non-existent Dragon joystick input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DragonJoystickError {
    /// Requested joystick port.
    pub port: u8,
    /// Requested button, when the error was for a button line.
    pub button: Option<u8>,
}

impl fmt::Display for DragonJoystickError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(button) = self.button {
            write!(
                f,
                "Dragon joystick button {button} on port {} is out of range",
                self.port
            )
        } else {
            write!(f, "Dragon joystick port {} is out of range", self.port)
        }
    }
}

impl Error for DragonJoystickError {}

#[derive(Debug, Clone)]
struct DragonJoystick {
    axes: [[u16; 2]; 2],
    buttons: u8,
}

impl DragonJoystick {
    fn new() -> Self {
        Self {
            axes: [[DRAGON_JOYSTICK_CENTER; 2]; 2],
            buttons: 0,
        }
    }

    fn axis(&self, port: usize, axis: DragonJoystickAxis) -> u16 {
        self.axes[port & 0x01][axis.index()]
    }

    fn checked_axis(&self, port: u8, axis: DragonJoystickAxis) -> Result<u16, DragonJoystickError> {
        let Some(port_axes) = self.axes.get(usize::from(port)) else {
            return Err(DragonJoystickError { port, button: None });
        };
        Ok(port_axes[axis.index()])
    }

    fn set_axis(
        &mut self,
        port: u8,
        axis: DragonJoystickAxis,
        value: u16,
    ) -> Result<(), DragonJoystickError> {
        let Some(port_axes) = self.axes.get_mut(usize::from(port)) else {
            return Err(DragonJoystickError { port, button: None });
        };
        port_axes[axis.index()] = value;
        Ok(())
    }

    fn set_button(&mut self, button: u8, pressed: bool) -> Result<(), DragonJoystickError> {
        if button >= 2 {
            return Err(DragonJoystickError {
                port: 0,
                button: Some(button),
            });
        }
        let mask = 1 << button;
        if pressed {
            self.buttons |= mask;
        } else {
            self.buttons &= !mask;
        }
        Ok(())
    }

    fn button_mask_low(&self) -> u8 {
        !self.buttons
    }

    fn button(&self, button: u8) -> Result<bool, DragonJoystickError> {
        if button >= 2 {
            return Err(DragonJoystickError {
                port: 0,
                button: Some(button),
            });
        }
        Ok(self.buttons & (1 << button) != 0)
    }
}

impl Default for DragonJoystick {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DragonAudioSource {
    Dac,
    Tape,
    Cart,
    UnusedMuxInput,
    None,
    SingleBit,
}

impl DragonAudioSource {
    const fn index(self) -> usize {
        match self {
            Self::Dac => 0,
            Self::Tape => 1,
            Self::Cart => 2,
            Self::UnusedMuxInput => 3,
            Self::None => 4,
            Self::SingleBit => 5,
        }
    }
}

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
    /// Dragon 64 6551 ACIA serial interface.
    Acia,
    /// Cartridge I/O range.
    Cartridge,
}

/// Instruction fetch trace entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FetchTrace {
    /// Bus cycle when the opcode fetch began.
    pub cycle: u64,
    /// SAM master-clock tick when the opcode fetch began.
    pub master_tick: u64,
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

/// Inclusive CPU address range used for diagnostic bus-write watches.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AddressRange {
    /// First watched address.
    pub start: u16,
    /// Last watched address.
    pub end: u16,
}

impl AddressRange {
    /// Create an inclusive address range.
    #[must_use]
    pub const fn new(start: u16, end: u16) -> Self {
        Self { start, end }
    }

    #[must_use]
    const fn contains(self, addr: u16) -> bool {
        self.start <= addr && addr <= self.end
    }
}

/// CPU register snapshot captured beside a diagnostic memory write.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CpuRegisterTrace {
    /// Accumulator A.
    pub a: u8,
    /// Accumulator B.
    pub b: u8,
    /// Direct-page register.
    pub dp: u8,
    /// Condition-code register.
    pub cc: u8,
    /// Index register X.
    pub x: u16,
    /// Index register Y.
    pub y: u16,
    /// User stack pointer.
    pub u: u16,
    /// Hardware stack pointer.
    pub s: u16,
    /// Program counter.
    pub pc: u16,
}

/// Diagnostic bus-write trace entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MemoryWriteTrace {
    /// Bus cycle.
    pub cycle: u64,
    /// Master-tick offset within the current video frame.
    pub frame_master_tick: u64,
    /// Visible framebuffer line, including top border, when the beam is visible.
    pub line: Option<usize>,
    /// Active-area line, excluding top border, when the beam is in active display.
    pub active_y: Option<usize>,
    /// Active-area pixel column when the beam is in active display.
    pub active_x: Option<usize>,
    /// Address of the opcode fetch that started the current instruction.
    pub instruction_pc: Option<u16>,
    /// CPU address.
    pub addr: u16,
    /// Written value.
    pub value: u8,
    /// CPU registers before the write cycle was executed.
    pub regs: CpuRegisterTrace,
}

/// Diagnostic opcode-fetch trace entry with CPU registers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WatchedFetchTrace {
    /// Bus cycle when the opcode fetch began.
    pub cycle: u64,
    /// SAM master-clock tick when the opcode fetch began.
    pub master_tick: u64,
    /// Program counter.
    pub pc: u16,
    /// Opcode byte.
    pub opcode: u8,
    /// CPU registers before the instruction was executed.
    pub regs: CpuRegisterTrace,
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

/// PIA control-line signal trace entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PiaSignalTrace {
    /// Bus cycle.
    pub cycle: u64,
    /// PIA receiving the signal.
    pub device: DeviceRegion,
    /// Signal line.
    pub signal: PiaSignal,
    /// External line level.
    pub level: bool,
    /// Port A control register after applying the signal.
    pub control_a: u8,
    /// Port B control register after applying the signal.
    pub control_b: u8,
    /// Whether PIA port A IRQ output is active after applying the signal.
    pub irq_a: bool,
    /// Whether PIA port B IRQ output is active after applying the signal.
    pub irq_b: bool,
}

/// CPU interrupt input kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CpuInterruptKind {
    /// Standard IRQ input.
    Irq,
    /// Fast IRQ input.
    Firq,
}

/// CPU interrupt-line transition trace entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CpuInterruptLineTrace {
    /// Bus cycle.
    pub cycle: u64,
    /// Interrupt input line.
    pub kind: CpuInterruptKind,
    /// New line level.
    pub level: bool,
    /// CPU PC at the transition.
    pub pc: u16,
    /// CPU condition-code register at the transition.
    pub cc: u8,
}

/// CPU interrupt acceptance trace entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CpuInterruptAcceptTrace {
    /// Bus cycle.
    pub cycle: u64,
    /// Accepted interrupt type.
    pub kind: CpuInterruptKind,
    /// PC about to be stacked.
    pub pc: u16,
    /// Condition-code register before entry.
    pub cc: u8,
}

/// VDG byte-render sampling trace entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VdgSampleTrace {
    /// Bus cycle that advanced the beam far enough to sample this byte.
    pub cycle: u64,
    /// Master-tick offset within the current visible frame.
    pub frame_master_tick: u64,
    /// Master-tick offset when this display byte was fetched from RAM.
    pub fetch_frame_master_tick: u64,
    /// Visible framebuffer line, including top border.
    pub line: usize,
    /// Active-area line, excluding top border.
    pub active_y: usize,
    /// Display byte column sampled on this line.
    pub byte_x: usize,
    /// Display-memory offset fetched for this byte.
    pub display_offset: usize,
    /// Raw display byte latched by the VDG.
    pub raw: u8,
    /// SAM-selected display base at the sample point.
    pub display_base: u16,
    /// SAM VDG mode latch bits V0..V2 at the sample point.
    pub sam_video_mode: u8,
    /// SAM display-offset latch bits F0..F6 at the sample point.
    pub sam_display_offset: u8,
    /// PIA1 port-B pin state feeding MC6847 control lines.
    pub pia1_pb: u8,
    /// MC6847 A/G line; true selects full graphics.
    pub graphics: bool,
    /// MC6847 CSS line.
    pub css: bool,
    /// MC6847 INT/EXT line.
    pub int_ext: bool,
    /// MC6847 GM0..GM2 value.
    pub gm: u8,
}

/// VDG mode-control write trace entry with beam position.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VdgModeWriteTrace {
    /// Bus cycle when the PIA1 port-B write was executed.
    pub cycle: u64,
    /// Master-tick offset within the current video frame.
    pub frame_master_tick: u64,
    /// Visible framebuffer line, including top border, when the beam is visible.
    pub line: Option<usize>,
    /// Active-area line, excluding top border, when the beam is in active display.
    pub active_y: Option<usize>,
    /// Active-area pixel column when the beam is in active display.
    pub active_x: Option<usize>,
    /// CPU address written.
    pub addr: u16,
    /// PIA1 port-B value written.
    pub value: u8,
    /// MC6847 A/G line after the write; true selects full graphics.
    pub graphics: bool,
    /// MC6847 CSS line after the write.
    pub css: bool,
    /// MC6847 INT/EXT line after the write.
    pub int_ext: bool,
    /// MC6847 GM0..GM2 value after the write.
    pub gm: u8,
}

/// Current MC6847 beam position within the emulated PAL frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DragonVideoPhase {
    /// Master-tick offset within the current video frame.
    pub frame_master_tick: u64,
    /// Physical scanline index within the PAL frame.
    pub physical_line: usize,
    /// Master-tick offset within the current physical scanline.
    pub line_master_tick: u64,
    /// Visible framebuffer line, including top border, when the beam is visible.
    pub visible_line: Option<usize>,
    /// Active-area line, excluding top border, when the beam is in active display.
    pub active_y: Option<usize>,
    /// Active-area pixel column when the beam is in active display.
    pub active_x: Option<usize>,
}

/// Reason a bounded machine run stopped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopReason {
    /// Requested cycle limit was reached.
    CycleLimit,
    /// CPU entered its halt state.
    CpuHalted,
}

/// Options for a bounded Dragon machine run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunOptions {
    /// Number of recent entries to retain in each diagnostic trace.
    pub trace_limit: usize,
    /// Inclusive opcode-fetch watch ranges.
    pub fetch_watch: Vec<AddressRange>,
    /// Inclusive bus-write watch ranges.
    pub write_watch: Vec<AddressRange>,
}

impl RunOptions {
    /// Create run options with the supplied trace limit.
    #[must_use]
    pub fn new(trace_limit: usize) -> Self {
        Self {
            trace_limit,
            fetch_watch: Vec::new(),
            write_watch: Vec::new(),
        }
    }
}

/// Summary of a bounded Dragon machine run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunReport {
    /// Stop reason.
    pub stop_reason: StopReason,
    /// Bus cycles executed during this run.
    pub cycles: u64,
    /// SAM master-clock ticks executed during this run.
    pub master_ticks: u64,
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
    /// Retained watched opcode fetches.
    pub watched_fetches: Vec<WatchedFetchTrace>,
    /// Number of watched fetch entries dropped due to the trace limit.
    pub dropped_watched_fetches: usize,
    /// Retained watched bus writes.
    pub watched_writes: Vec<MemoryWriteTrace>,
    /// Number of watched write entries dropped due to the trace limit.
    pub dropped_watched_writes: usize,
    /// Retained PIA signal transitions.
    pub pia_signals: Vec<PiaSignalTrace>,
    /// Number of PIA signal entries dropped due to the trace limit.
    pub dropped_pia_signals: usize,
    /// Retained CPU interrupt-line transitions.
    pub interrupt_lines: Vec<CpuInterruptLineTrace>,
    /// Number of CPU interrupt-line entries dropped due to the trace limit.
    pub dropped_interrupt_lines: usize,
    /// Retained CPU interrupt acceptances.
    pub interrupt_accepts: Vec<CpuInterruptAcceptTrace>,
    /// Number of CPU interrupt acceptance entries dropped due to the trace limit.
    pub dropped_interrupt_accepts: usize,
    /// Retained VDG byte-render samples.
    pub vdg_samples: Vec<VdgSampleTrace>,
    /// Number of VDG byte-render samples dropped due to the trace limit.
    pub dropped_vdg_samples: usize,
    /// Retained VDG mode-control writes.
    pub vdg_mode_writes: Vec<VdgModeWriteTrace>,
    /// Number of VDG mode-control writes dropped due to the trace limit.
    pub dropped_vdg_mode_writes: usize,
    /// SAM-selected text screen base at the end of the run.
    pub text_screen_base: u16,
}

#[derive(Debug, Clone)]
struct DragonMemory {
    ram: Box<[u8; FULL_RAM_SIZE]>,
    rom: Box<[u8; ROM_SIZE]>,
    model: DragonHardwareModel,
    pia0: Pia6821,
    pia1: Pia6821,
    sam: Sam6883,
    vdg_display_base: u16,
    keyboard: DragonKeyboard,
    joystick: DragonJoystick,
    cassette: CassettePlayback,
    cartridge: Option<DragonCartridge>,
    cartridge_sound_level: f32,
}

impl DragonMemory {
    #[cfg(test)]
    fn new_with_keyboard(rom: &[u8; ROM_SIZE], keyboard: DragonKeyboard) -> Self {
        Self::new_with_keyboard_and_model(rom, keyboard, DragonHardwareModel::Dragon32)
    }

    fn new_with_keyboard_and_model(
        rom: &[u8; ROM_SIZE],
        keyboard: DragonKeyboard,
        model: DragonHardwareModel,
    ) -> Self {
        Self {
            ram: Box::new([0; FULL_RAM_SIZE]),
            rom: Box::new(*rom),
            model,
            pia0: Pia6821::new(),
            pia1: Pia6821::new(),
            sam: Sam6883::new(),
            vdg_display_base: 0,
            keyboard,
            joystick: DragonJoystick::new(),
            cassette: CassettePlayback::default(),
            cartridge: None,
            cartridge_sound_level: 0.0,
        }
    }

    fn read_fetch(&self, addr: u16) -> u8 {
        if let Some(index) = self.mpu_ram_index(addr) {
            return self.ram[index];
        }

        if let Some(cartridge) = &self.cartridge
            && (CART_ROM_START..=CART_ROM_END).contains(&addr)
        {
            return cartridge.read_rom(addr);
        }

        self.rom_read(addr).unwrap_or(NO_CARTRIDGE_BUS_VALUE)
    }

    fn read_bus(&mut self, addr: u16) -> (u8, Option<MemoryEvent>) {
        if decode_acia(self.model, addr).is_some() {
            let value = acia_read(addr);
            return (
                value,
                Some(MemoryEvent::DeviceRead {
                    device: DeviceRegion::Acia,
                    addr,
                    value,
                }),
            );
        }

        if let Some((device, offset)) = decode_pia(addr) {
            self.refresh_pia_inputs();
            let value = match device {
                DeviceRegion::Pia0 => self.pia0.read(offset),
                DeviceRegion::Pia1 => self.pia1.read(offset),
                DeviceRegion::Sam => unreachable!("SAM is not a PIA"),
                DeviceRegion::Acia => unreachable!("ACIA is not a PIA"),
                DeviceRegion::Cartridge => unreachable!("cartridge is not a PIA"),
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

        if let Some(index) = self.mpu_ram_index(addr) {
            return (self.ram[index], None);
        }

        if let Some(cartridge) = &self.cartridge
            && (CART_ROM_START..=CART_ROM_END).contains(&addr)
        {
            return (cartridge.read_rom(addr), None);
        }

        (self.read_fetch(addr), None)
    }

    fn write(&mut self, addr: u16, value: u8) -> Option<MemoryEvent> {
        if let Some(index) = self.mpu_ram_index(addr) {
            self.ram[index] = value;
            None
        } else if decode_acia(self.model, addr).is_some() {
            Some(MemoryEvent::DeviceWrite {
                device: DeviceRegion::Acia,
                addr,
                value,
            })
        } else if let Some((device, offset)) = decode_pia(addr) {
            match device {
                DeviceRegion::Pia0 => {
                    self.pia0.write(offset, value);
                    self.refresh_pia_inputs();
                }
                DeviceRegion::Pia1 => {
                    self.pia1.write(offset, value);
                    self.refresh_pia_inputs();
                }
                DeviceRegion::Sam => unreachable!("SAM is not a PIA"),
                DeviceRegion::Acia => unreachable!("ACIA is not a PIA"),
                DeviceRegion::Cartridge => unreachable!("cartridge is not a PIA"),
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
        } else if let Some(cartridge) = &mut self.cartridge
            && (CART_IO_START..=CART_IO_END).contains(&addr)
        {
            cartridge.write_io(addr, value);
            Some(MemoryEvent::DeviceWrite {
                device: DeviceRegion::Cartridge,
                addr,
                value,
            })
        } else {
            Some(MemoryEvent::RomWrite { addr, value })
        }
    }

    fn load_cartridge(&mut self, kind: DragonCartridgeKind, rom: &[u8], autorun: bool) {
        self.cartridge = Some(DragonCartridge::new(kind, rom, autorun));
    }

    fn clear_cartridge(&mut self) {
        self.cartridge = None;
    }

    fn set_cartridge_sound_level(&mut self, level: f32) {
        self.cartridge_sound_level = if level.is_finite() {
            level.clamp(0.0, 1.0)
        } else {
            0.0
        };
    }

    fn restore_snapshot_peripherals(&mut self, peripherals: DragonSnapshotPeripherals) {
        self.sam
            .set_video_mode(sam_video_mode_from_vdg_pins(peripherals.ff22));
        self.pia0.write(0x03, peripherals.ff03 & !0x04);
        self.pia0.write(0x02, 0xff);
        self.pia0.write(0x03, peripherals.ff03);
        self.pia0.write(0x02, peripherals.ff02);
        self.pia0.restore_control(PiaPort::B, peripherals.ff03);

        self.pia1.write(0x03, 0x00);
        self.pia1.write(0x02, 0xff);
        self.pia1.write(0x03, 0x04);
        self.pia1.write(0x02, peripherals.ff22);
        self.refresh_pia_inputs();
    }

    fn tick_cartridge_autorun(&mut self, cycles: u64) {
        if let Some(cartridge) = &mut self.cartridge
            && cartridge.should_signal_autorun(cycles)
        {
            self.pia1.strobe_signal(PiaSignal::Cb1);
        }
    }

    fn refresh_pia_inputs(&mut self) {
        let column_output = self.pia0.output_latch(PiaPort::B) | !self.pia0.ddr(PiaPort::B);
        let mut pia0_pa = self.keyboard.port_a_input(column_output);
        pia0_pa &= self.joystick.button_mask_low();
        if self.joystick_comparator_high() {
            pia0_pa |= 0x80;
        } else {
            pia0_pa &= !0x80;
        }
        self.pia0.set_input(PiaPort::A, pia0_pa);
        self.pia0.set_input(PiaPort::B, 0xFF);
        let pia1_pa = if self.cassette.line_level() {
            0xFF
        } else {
            0xFE
        };
        self.pia1.set_input(PiaPort::A, pia1_pa);
        self.pia1.set_input(PiaPort::B, 0xFF);
    }

    fn joystick_comparator_high(&self) -> bool {
        let port = usize::from(self.pia0.cb2);
        let axis = if self.pia0.ca2 {
            DragonJoystickAxis::Y
        } else {
            DragonJoystickAxis::X
        };
        let threshold = joystick_threshold_from_dac(self.pia1.output_latch(PiaPort::A));
        self.joystick.axis(port, axis) >= threshold
    }

    fn advance_cassette(&mut self, master_ticks: u64) {
        if self.pia1.ca2 {
            self.cassette.advance_ticks(master_ticks);
            self.refresh_pia_inputs();
        }
    }

    fn text_screen_base(&self) -> u16 {
        self.sam.display_base()
    }

    fn vdg_display_base(&self) -> u16 {
        self.vdg_display_base
    }

    fn sync_vdg_display_base_from_sam(&mut self) {
        self.vdg_display_base = self.sam.display_base();
    }

    fn capture_text_screen(&self) -> TextScreen {
        let base = usize::from(self.text_screen_base());
        TextScreen::capture(|offset| self.ram[(base + offset) & (FULL_RAM_SIZE - 1)])
    }

    fn display_byte(&self, offset: usize) -> u8 {
        let base = usize::from(self.vdg_display_base());
        self.ram[(base + offset) & (FULL_RAM_SIZE - 1)]
    }

    fn mpu_ram_index(&self, addr: u16) -> Option<usize> {
        if self.sam.ty() {
            return (addr < 0xFF00).then_some(usize::from(addr));
        }
        if addr < 0x8000 {
            let page = if self.sam.page_select() { RAM_SIZE } else { 0 };
            return Some(page + usize::from(addr));
        }
        if self.model == DragonHardwareModel::Dragon64Mode && addr < 0xC000 {
            return Some(usize::from(addr));
        }
        None
    }

    fn rom_read(&self, addr: u16) -> Option<u8> {
        let index = match self.model {
            DragonHardwareModel::Dragon32 | DragonHardwareModel::Dragon64Compat
                if !(CART_ROM_START..=CART_ROM_END).contains(&addr) =>
            {
                (usize::from(addr).wrapping_sub(RAM_SIZE)) & (ROM_SIZE - 1)
            }
            DragonHardwareModel::Dragon64Mode if addr >= CART_ROM_START => {
                usize::from(addr - CART_ROM_START) & (ROM_SIZE - 1)
            }
            _ => return None,
        };
        Some(self.rom[index])
    }

    fn audio_sample(&self) -> f32 {
        let sbs_index = self.single_bit_sound_index();
        let (source, input) = if self.pia1.cb2 {
            self.muxed_audio_source()
        } else if sbs_index == 0 {
            (DragonAudioSource::None, 0.0)
        } else {
            (DragonAudioSource::SingleBit, 0.0)
        };

        let index = source.index();
        let sample = input * XROAR_AUDIO_SOURCE_GAIN[index][sbs_index]
            + XROAR_AUDIO_SOURCE_OFFSET[index][sbs_index];
        (sample * XROAR_AUDIO_OUTPUT_GAIN).clamp(-1.0, 1.0)
    }

    fn muxed_audio_source(&self) -> (DragonAudioSource, f32) {
        match (u8::from(self.pia0.cb2) << 1) | u8::from(self.pia0.ca2) {
            0 => (
                DragonAudioSource::Dac,
                f32::from(self.pia1.pa & 0xfc) / 252.0,
            ),
            1 => (
                DragonAudioSource::Tape,
                if self.cassette.line_level() { 1.0 } else { 0.0 },
            ),
            2 => (DragonAudioSource::Cart, self.cartridge_sound_level),
            _ => (DragonAudioSource::UnusedMuxInput, 0.0),
        }
    }

    fn single_bit_sound_index(&self) -> usize {
        if self.pia1.ddr(PiaPort::B) & 0x02 == 0 {
            0
        } else if self.pia1.output_latch(PiaPort::B) & 0x02 == 0 {
            1
        } else {
            2
        }
    }
}

#[derive(Debug, Clone)]
struct BeamVideo {
    frame: Vec<u32>,
    cycle_in_frame: u64,
    next_line: usize,
    line_border_rendered: bool,
    next_active_x: usize,
    next_byte: usize,
    current_byte: Option<ActiveByteRender>,
    prefetch_line: usize,
    next_prefetch_x: usize,
    next_prefetch_byte: usize,
    prefetched_bytes: [Option<PrefetchedByte>; motorola_vdg_6847::TEXT_COLUMNS],
    control_initialized: bool,
    render_pin_control: VdgControl,
    pending_css_changes: VecDeque<(u64, bool)>,
    // MC6847 colour-set select is delayed through a two-byte pipeline.
    css_a: bool,
    css_b: bool,
    pending_samples: Vec<VdgSampleTrace>,
}

impl BeamVideo {
    fn new() -> Self {
        Self {
            frame: vec![VdgPalette::default().border; TEXT_VISIBLE_FRAMEBUFFER_PIXELS],
            cycle_in_frame: 0,
            next_line: 0,
            line_border_rendered: false,
            next_active_x: 0,
            next_byte: 0,
            current_byte: None,
            prefetch_line: 0,
            next_prefetch_x: 0,
            next_prefetch_byte: 0,
            prefetched_bytes: [None; motorola_vdg_6847::TEXT_COLUMNS],
            control_initialized: false,
            render_pin_control: VdgControl::default(),
            pending_css_changes: VecDeque::new(),
            css_a: false,
            css_b: false,
            pending_samples: Vec::new(),
        }
    }

    fn frame(&self) -> &[u32] {
        &self.frame
    }

    fn tick(
        &mut self,
        memory: &DragonMemory,
        master_ticks: u64,
        bus_cycle: Option<u64>,
    ) -> BeamVideoTick {
        let mut tick = BeamVideoTick::default();
        let previous = self.cycle_in_frame;
        let target = self.cycle_in_frame.saturating_add(master_ticks);
        self.render_until(memory, target, bus_cycle);
        self.cycle_in_frame = target;
        if self.cycle_in_frame >= DRAGON_FRAME_MASTER_TICKS {
            self.render_until(memory, DRAGON_FRAME_MASTER_TICKS, bus_cycle);
            self.wrap_pending_css_changes();
            self.cycle_in_frame = 0;
            self.next_line = 0;
            self.line_border_rendered = false;
            self.next_active_x = 0;
            self.next_byte = 0;
            self.current_byte = None;
            self.reset_prefetch_state();
            return tick;
        }

        if crosses_frame_sync_fall(previous, self.cycle_in_frame) {
            tick.frame_sync = Some(false);
        } else if crosses_frame_sync_rise(previous, self.cycle_in_frame) {
            tick.frame_sync = Some(true);
        }
        let previous_line = video_line_for_cycle(previous);
        let current_line = video_line_for_cycle(self.cycle_in_frame);
        if current_line != previous_line {
            tick.hsync = Some(false);
        } else if crosses_hsync_rise(previous, self.cycle_in_frame) {
            tick.hsync = Some(true);
        }
        tick
    }

    fn apply_vdg_control_change(&mut self, memory: &DragonMemory) {
        self.ensure_control_initialized(memory);
        let pin_control = VdgControl::from_dragon_pia1_port_b(memory.pia1.pb);
        let pending_css = self
            .pending_css_changes
            .back()
            .map_or(self.render_pin_control.css, |(_, css)| *css);
        self.render_pin_control.graphics = pin_control.graphics;
        self.render_pin_control.int_ext = pin_control.int_ext;
        self.render_pin_control.gm = pin_control.gm;
        if pin_control.css != pending_css {
            self.pending_css_changes.push_back((
                self.cycle_in_frame
                    .saturating_add(vdg_fetch_to_display_ticks(pin_control)),
                pin_control.css,
            ));
        }
    }

    fn drain_pending_samples(&mut self, dest: &mut Vec<VdgSampleTrace>) {
        dest.append(&mut self.pending_samples);
    }

    fn beam_position(&self) -> (Option<usize>, Option<usize>, Option<usize>) {
        let absolute_line = video_line_for_cycle(self.cycle_in_frame);
        let visible_line = absolute_line
            .checked_sub(usize::try_from(VDG_VISIBLE_FIRST_LINE).unwrap_or(usize::MAX))
            .filter(|&line| line < motorola_vdg_6847::TEXT_VISIBLE_FRAMEBUFFER_HEIGHT);
        let active_y = visible_line
            .filter(|&line| active_line(line))
            .map(|line| line - motorola_vdg_6847::TEXT_TOP_BORDER_LINES);
        let active_x = visible_line.and_then(|line| {
            if !active_line(line) {
                return None;
            }
            let start = active_display_start_cycle(line);
            let elapsed = self.cycle_in_frame.checked_sub(start)?;
            let active_x = usize::try_from(elapsed / 2).ok()?;
            (active_x < motorola_vdg_6847::TEXT_FRAMEBUFFER_WIDTH).then_some(active_x)
        });
        (visible_line, active_y, active_x)
    }

    fn render_until(&mut self, memory: &DragonMemory, target_cycle: u64, bus_cycle: Option<u64>) {
        while self.next_line < TEXT_VISIBLE_FRAMEBUFFER_HEIGHT {
            let line_start = line_start_cycle(self.next_line);
            if line_start >= target_cycle {
                break;
            }

            if !self.line_border_rendered {
                self.render_border_line(memory, self.next_line);
                self.line_border_rendered = true;
                if !active_line(self.next_line) {
                    self.advance_line();
                }
                continue;
            }

            if active_line(self.next_line) {
                self.prefetch_until(memory, target_cycle);
            }

            if active_line(self.next_line)
                && self.next_active_x < motorola_vdg_6847::TEXT_FRAMEBUFFER_WIDTH
            {
                let active_tick = active_pixel_cycle(self.next_line, self.next_active_x);
                if active_tick >= target_cycle {
                    break;
                }

                if self.current_byte.is_none() {
                    self.start_active_byte(memory, bus_cycle);
                }
                let Some(current) = self.current_byte else {
                    self.advance_line();
                    continue;
                };
                let target_active_x = active_x_for_target(self.next_line, target_cycle);
                let segment_end = target_active_x
                    .min(current.end_x())
                    .min(motorola_vdg_6847::TEXT_FRAMEBUFFER_WIDTH);
                if segment_end <= self.next_active_x {
                    break;
                }
                self.render_byte_range(current, self.next_active_x, segment_end);
                self.next_active_x = segment_end;
                if self.next_active_x >= current.end_x() {
                    self.next_byte += 1;
                    self.current_byte = None;
                }
                continue;
            }

            self.advance_line();
        }
    }

    fn render_border_line(&mut self, memory: &DragonMemory, line: usize) {
        self.ensure_control_initialized(memory);
        self.apply_pending_css_changes(line_start_cycle(line));
        let palette = VdgPalette::default();
        let start = line * motorola_vdg_6847::TEXT_VISIBLE_FRAMEBUFFER_WIDTH;
        let end = start + motorola_vdg_6847::TEXT_VISIBLE_FRAMEBUFFER_WIDTH;
        self.frame[start..end].fill(palette.border);
        if active_line(line) {
            self.css_a = self.render_pin_control.css;
        }
    }

    fn start_active_byte(&mut self, memory: &DragonMemory, bus_cycle: Option<u64>) {
        self.ensure_control_initialized(memory);
        self.apply_pending_css_changes(active_pixel_cycle(self.next_line, self.next_active_x));
        let pin_control = self.render_pin_control;
        self.css_b = self.css_a;
        self.css_a = pin_control.css;
        let mut render_control = pin_control;
        render_control.css = self.css_b;
        let active_y = self
            .next_line
            .saturating_sub(motorola_vdg_6847::TEXT_TOP_BORDER_LINES);
        self.ensure_prefetch_line();
        let prefetched = self
            .prefetched_bytes
            .get(self.next_byte)
            .and_then(|prefetched| *prefetched);
        let byte = motorola_vdg_6847::decode_beam_byte(
            |offset| {
                prefetched
                    .filter(|prefetched| prefetched.offset == offset)
                    .map_or_else(|| memory.display_byte(offset), |prefetched| prefetched.raw)
            },
            render_control,
            VdgPalette::default(),
            active_y,
            self.next_byte,
        );
        if byte.width() == 0 {
            self.next_active_x = motorola_vdg_6847::TEXT_FRAMEBUFFER_WIDTH;
            return;
        }
        if let Some(cycle) = bus_cycle {
            self.pending_samples.push(VdgSampleTrace {
                cycle,
                frame_master_tick: active_pixel_cycle(self.next_line, self.next_active_x),
                fetch_frame_master_tick: prefetched.map_or(
                    active_pixel_cycle(self.next_line, self.next_active_x),
                    |prefetched| prefetched.fetch_cycle,
                ),
                line: self.next_line,
                active_y,
                byte_x: self.next_byte,
                display_offset: prefetched.map_or(usize::MAX, |prefetched| prefetched.offset),
                raw: prefetched.map_or_else(
                    || {
                        vdg_byte_fetch_offset(render_control, active_y, self.next_byte)
                            .map_or(0, |(offset, _)| memory.display_byte(offset))
                    },
                    |prefetched| prefetched.raw,
                ),
                display_base: memory.vdg_display_base(),
                sam_video_mode: memory.sam.video_mode(),
                sam_display_offset: memory.sam.display_offset(),
                pia1_pb: memory.pia1.pb,
                graphics: render_control.graphics,
                css: render_control.css,
                int_ext: render_control.int_ext,
                gm: render_control.gm,
            });
        }
        self.current_byte = Some(ActiveByteRender {
            line: self.next_line,
            byte_x: self.next_byte,
            start_x: self.next_active_x,
            control: render_control,
            byte,
        });
    }

    fn prefetch_until(&mut self, memory: &DragonMemory, target_cycle: u64) {
        self.ensure_control_initialized(memory);
        self.ensure_prefetch_line();
        let active_y = self
            .next_line
            .saturating_sub(motorola_vdg_6847::TEXT_TOP_BORDER_LINES);
        while self.next_prefetch_byte < motorola_vdg_6847::TEXT_COLUMNS {
            let Some((offset, width)) =
                vdg_byte_fetch_offset(self.render_pin_control, active_y, self.next_prefetch_byte)
            else {
                break;
            };
            let display_cycle = active_pixel_cycle(self.next_line, self.next_prefetch_x);
            let fetch_cycle =
                display_cycle.saturating_sub(vdg_fetch_to_display_ticks(self.render_pin_control));
            if fetch_cycle >= target_cycle {
                break;
            }
            self.prefetched_bytes[self.next_prefetch_byte] = Some(PrefetchedByte {
                offset,
                raw: memory.display_byte(offset),
                fetch_cycle,
            });
            self.next_prefetch_byte += 1;
            self.next_prefetch_x += width;
        }
    }

    fn render_byte_range(&mut self, current: ActiveByteRender, start_x: usize, end_x: usize) {
        current.byte.render_range_into(
            &mut self.frame,
            current.line,
            current.start_x,
            start_x.saturating_sub(current.start_x),
            end_x.saturating_sub(current.start_x),
        );
    }

    fn advance_line(&mut self) {
        self.next_line += 1;
        self.line_border_rendered = false;
        self.next_active_x = 0;
        self.next_byte = 0;
        self.current_byte = None;
        self.reset_prefetch_state();
    }

    fn ensure_prefetch_line(&mut self) {
        if self.prefetch_line != self.next_line {
            self.reset_prefetch_state();
        }
    }

    fn reset_prefetch_state(&mut self) {
        self.prefetch_line = self.next_line;
        self.next_prefetch_x = 0;
        self.next_prefetch_byte = 0;
        self.prefetched_bytes.fill(None);
    }

    fn ensure_control_initialized(&mut self, memory: &DragonMemory) {
        if !self.control_initialized {
            self.render_pin_control = VdgControl::from_dragon_pia1_port_b(memory.pia1.pb);
            self.control_initialized = true;
        }
    }

    fn apply_pending_css_changes(&mut self, display_cycle: u64) {
        while self
            .pending_css_changes
            .front()
            .is_some_and(|(effective_cycle, _)| *effective_cycle <= display_cycle)
        {
            let Some((_, css)) = self.pending_css_changes.pop_front() else {
                break;
            };
            self.render_pin_control.css = css;
        }
    }

    fn wrap_pending_css_changes(&mut self) {
        self.apply_pending_css_changes(DRAGON_FRAME_MASTER_TICKS);
        for (effective_cycle, _) in &mut self.pending_css_changes {
            *effective_cycle = effective_cycle.saturating_sub(DRAGON_FRAME_MASTER_TICKS);
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ActiveByteRender {
    line: usize,
    byte_x: usize,
    start_x: usize,
    control: VdgControl,
    byte: motorola_vdg_6847::VdgBeamByte,
}

impl ActiveByteRender {
    const fn end_x(self) -> usize {
        self.start_x + self.byte.width()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PrefetchedByte {
    offset: usize,
    raw: u8,
    fetch_cycle: u64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct BeamVideoTick {
    hsync: Option<bool>,
    frame_sync: Option<bool>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct CpuPhaseMasterTicks {
    q_high: u64,
    e_high: u64,
    q_low: u64,
    e_low: u64,
}

impl CpuPhaseMasterTicks {
    fn split(master_ticks: u64) -> Self {
        let base = master_ticks / 4;
        let remainder = master_ticks % 4;
        Self {
            q_high: base + u64::from(remainder > 0),
            e_high: base + u64::from(remainder > 1),
            q_low: base + u64::from(remainder > 2),
            e_low: base,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct SamCycleTiming {
    running_fast: bool,
    extend_slow_cycle: bool,
}

impl SamCycleTiming {
    fn tick(&mut self, sam: &Sam6883, addr: u16) -> u64 {
        let fast_cycle = sam.cpu_rate() & 0x02 != 0
            || (sam.cpu_rate() == 0x01 && !Self::is_ram_or_io0(sam, addr));

        if !self.running_fast {
            if fast_cycle {
                self.running_fast = true;
                15
            } else {
                16
            }
        } else if fast_cycle {
            self.extend_slow_cycle = !self.extend_slow_cycle;
            8
        } else {
            self.running_fast = false;
            if self.extend_slow_cycle {
                self.extend_slow_cycle = false;
                25
            } else {
                17
            }
        }
    }

    fn is_ram_or_io0(sam: &Sam6883, addr: u16) -> bool {
        let is_ffxx = addr >> 8 == 0xff;
        let is_io0 = is_ffxx && ((addr >> 5) & 0x07) == 0x00;
        let is_ram = addr & 0x8000 == 0 || (sam.ty() && !is_ffxx);
        is_ram || is_io0
    }
}

fn line_start_cycle(line: usize) -> u64 {
    VDG_VISIBLE_FIRST_LINE
        .saturating_add(line as u64)
        .saturating_mul(VDG_LINE_MASTER_TICKS)
}

fn video_line_for_cycle(cycle: u64) -> usize {
    let line = cycle / VDG_LINE_MASTER_TICKS;
    usize::try_from(line).map_or(usize::MAX, |line| line)
}

fn crosses_hsync_rise(previous: u64, current: u64) -> bool {
    previous % VDG_LINE_MASTER_TICKS < VDG_HSYNC_TICKS
        && current % VDG_LINE_MASTER_TICKS >= VDG_HSYNC_TICKS
}

fn crosses_frame_sync_fall(previous: u64, current: u64) -> bool {
    previous < VDG_FRAME_SYNC_FALL_TICK && current >= VDG_FRAME_SYNC_FALL_TICK
}

fn crosses_frame_sync_rise(previous: u64, current: u64) -> bool {
    previous < VDG_FRAME_SYNC_RISE_TICK && current >= VDG_FRAME_SYNC_RISE_TICK
}

fn active_line(line: usize) -> bool {
    (motorola_vdg_6847::TEXT_TOP_BORDER_LINES
        ..motorola_vdg_6847::TEXT_TOP_BORDER_LINES + motorola_vdg_6847::TEXT_FRAMEBUFFER_HEIGHT)
        .contains(&line)
}

fn active_display_start_cycle(line: usize) -> u64 {
    line_start_cycle(line)
        .saturating_add(VDG_HSYNC_TICKS)
        .saturating_add(VDG_BACK_PORCH_TICKS)
        .saturating_add(VDG_LEFT_BORDER_TICKS)
}

fn active_pixel_cycle(line: usize, active_x: usize) -> u64 {
    active_display_start_cycle(line).saturating_add((active_x as u64).saturating_mul(2))
}

fn active_x_for_target(line: usize, target_cycle: u64) -> usize {
    let elapsed = target_cycle.saturating_sub(active_display_start_cycle(line));
    let active_x = elapsed.saturating_add(1) / 2;
    usize::try_from(active_x).map_or(usize::MAX, |x| {
        x.min(motorola_vdg_6847::TEXT_FRAMEBUFFER_WIDTH)
    })
}

fn vdg_fetch_to_display_ticks(control: VdgControl) -> u64 {
    let fetch_clocks = if control.graphics && matches!(control.gm, 0 | 1 | 3 | 5) {
        VDG_LONG_CYCLE_FETCH_CLOCKS
    } else {
        VDG_SHORT_CYCLE_FETCH_CLOCKS
    };
    fetch_clocks * VDG_CLOCK_MASTER_TICKS
}

fn vdg_byte_fetch_offset(
    control: VdgControl,
    active_y: usize,
    byte_x: usize,
) -> Option<(usize, usize)> {
    if !control.graphics {
        let row = active_y / motorola_vdg_6847::TEXT_CELL_HEIGHT;
        return (byte_x < motorola_vdg_6847::TEXT_COLUMNS)
            .then_some((row * motorola_vdg_6847::TEXT_COLUMNS + byte_x, 8));
    }

    let (row_bytes, byte_width, y_scale) = match control.gm {
        0 => (16, 16, 3),
        1 => (16, 16, 3),
        2 => (32, 8, 3),
        3 => (16, 16, 2),
        4 => (32, 8, 2),
        5 => (16, 16, 1),
        6 => (32, 8, 1),
        _ => (32, 8, 1),
    };
    (byte_x < row_bytes).then_some((active_y / y_scale * row_bytes + byte_x, byte_width))
}

#[derive(Debug, Clone, Default)]
struct CassettePlayback {
    bytes: Vec<u8>,
    bit_index: usize,
    half_ticks_remaining: u64,
    level: bool,
}

impl CassettePlayback {
    fn load(&mut self, bytes: Vec<u8>) {
        self.bytes = bytes;
        self.bit_index = 0;
        self.half_ticks_remaining = self.current_half_period_ticks();
        self.level = false;
    }

    fn clear(&mut self) {
        *self = Self::default();
    }

    fn advance_ticks(&mut self, ticks: u64) {
        if self.finished() {
            return;
        }

        let mut remaining_ticks = ticks;
        while remaining_ticks >= self.half_ticks_remaining {
            remaining_ticks -= self.half_ticks_remaining;
            self.level = !self.level;
            if !self.level {
                self.bit_index = self.bit_index.saturating_add(1);
                if self.finished() {
                    return;
                }
            }
            self.half_ticks_remaining = self.current_half_period_ticks();
        }

        self.half_ticks_remaining = self.half_ticks_remaining.saturating_sub(remaining_ticks);
    }

    fn line_level(&self) -> bool {
        self.level
    }

    fn finished(&self) -> bool {
        self.bit_index >= self.bytes.len().saturating_mul(CASSETTE_BITS_PER_BYTE)
    }

    fn position_bits(&self) -> usize {
        self.bit_index
    }

    fn len_bits(&self) -> usize {
        self.bytes.len().saturating_mul(CASSETTE_BITS_PER_BYTE)
    }

    fn current_half_period_ticks(&self) -> u64 {
        if self.current_bit() {
            CASSETTE_ONE_HALF_PERIOD_TICKS
        } else {
            CASSETTE_ZERO_HALF_PERIOD_TICKS
        }
    }

    fn current_bit(&self) -> bool {
        let byte_index = self.bit_index / CASSETTE_BITS_PER_BYTE;
        let bit_index = self.bit_index % CASSETTE_BITS_PER_BYTE;
        self.bytes
            .get(byte_index)
            .is_some_and(|byte| byte & (1 << bit_index) != 0)
    }
}

#[derive(Debug, Clone)]
struct DragonAudio {
    cycle_accumulator: u64,
    samples: Vec<f32>,
}

impl DragonAudio {
    fn new() -> Self {
        Self {
            cycle_accumulator: 0,
            samples: Vec::with_capacity(
                (u64::from(DRAGON_AUDIO_SAMPLE_RATE) / DRAGON_FRAME_HZ) as usize,
            ),
        }
    }

    fn tick(&mut self, memory: &DragonMemory, master_ticks: u64) {
        self.cycle_accumulator = self
            .cycle_accumulator
            .saturating_add(u64::from(DRAGON_AUDIO_SAMPLE_RATE) * master_ticks);
        while self.cycle_accumulator >= DRAGON_MASTER_HZ {
            self.cycle_accumulator -= DRAGON_MASTER_HZ;
            self.samples.push(memory.audio_sample());
        }
    }

    fn drain_into(&mut self, dest: &mut Vec<f32>) {
        dest.append(&mut self.samples);
    }
}

fn sam_video_mode_from_vdg_pins(vdg_mode: u8) -> u8 {
    match vdg_mode & 0xf0 {
        0x80 | 0x90 => 1,
        0xa0 => 2,
        0xb0 => 3,
        0xc0 => 4,
        0xd0 => 5,
        0xe0 | 0xf0 => 6,
        _ => 0,
    }
}

/// Tickable Dragon 32 machine.
#[derive(Debug, Clone)]
pub struct Dragon32 {
    cpu: Mc6809,
    memory: DragonMemory,
    video: BeamVideo,
    audio: DragonAudio,
    sam_timing: SamCycleTiming,
    cycles: u64,
    master_ticks: u64,
    instructions: u64,
}

struct DiagnosticCycleTrace<'a> {
    cycle: u64,
    pia_signals: &'a mut Vec<PiaSignalTrace>,
    dropped_pia_signals: &'a mut usize,
    vdg_samples: &'a mut Vec<VdgSampleTrace>,
    dropped_vdg_samples: &'a mut usize,
    trace_limit: usize,
}

impl Dragon32 {
    /// Build and reset a Dragon 32 around a 16 KiB BASIC ROM.
    #[must_use]
    pub fn new(rom: &[u8; ROM_SIZE]) -> Self {
        Self::new_with_keyboard(rom, DragonKeyboard::new())
    }

    /// Build and reset a Dragon 64 in its Dragon 32-compatible cold-boot mode.
    #[must_use]
    pub fn new_dragon64(rom: &[u8; ROM_SIZE]) -> Self {
        Self::new_with_keyboard_and_model(
            rom,
            DragonKeyboard::new(),
            DragonHardwareModel::Dragon64Compat,
        )
    }

    /// Build and reset a Dragon 32 with a specific keyboard matrix state.
    #[must_use]
    pub fn new_with_keyboard(rom: &[u8; ROM_SIZE], keyboard: DragonKeyboard) -> Self {
        Self::new_with_keyboard_and_model(rom, keyboard, DragonHardwareModel::Dragon32)
    }

    /// Build and reset a Dragon with a specific keyboard matrix state and hardware model.
    #[must_use]
    pub fn new_with_keyboard_and_model(
        rom: &[u8; ROM_SIZE],
        keyboard: DragonKeyboard,
        model: DragonHardwareModel,
    ) -> Self {
        let mut machine = Self {
            cpu: Mc6809::new(),
            memory: DragonMemory::new_with_keyboard_and_model(rom, keyboard, model),
            video: BeamVideo::new(),
            audio: DragonAudio::new(),
            sam_timing: SamCycleTiming::default(),
            cycles: 0,
            master_ticks: 0,
            instructions: 0,
        };
        machine.cpu.reset();
        machine.memory.pia0.set_signal_level(PiaSignal::Ca1, true);
        machine.memory.pia0.set_signal_level(PiaSignal::Cb1, true);
        machine.memory.sync_vdg_display_base_from_sam();
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

    /// Current MC6809E E/Q phase.
    #[must_use]
    pub fn cpu_clock_phase(&self) -> Mc6809ClockPhase {
        self.cpu.clock_phase()
    }

    /// Total bus cycles executed since construction.
    #[must_use]
    pub fn cycles(&self) -> u64 {
        self.cycles
    }

    /// Total SAM master-clock ticks executed since construction.
    #[must_use]
    pub fn master_ticks(&self) -> u64 {
        self.master_ticks
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

    /// Current MC6809 hardware stack pointer.
    #[must_use]
    pub fn stack_pointer(&self) -> u16 {
        self.cpu.regs.s
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

    /// Set one Dragon analogue joystick axis.
    ///
    /// Ports are zero-based and match the two states selected by PIA0 CB2.
    pub fn set_joystick_axis(
        &mut self,
        port: u8,
        axis: DragonJoystickAxis,
        value: u16,
    ) -> Result<(), DragonJoystickError> {
        self.memory.joystick.set_axis(port, axis, value)?;
        self.memory.refresh_pia_inputs();
        Ok(())
    }

    /// Set one Dragon joystick fire line.
    ///
    /// The Dragon exposes two active-low fire inputs on PIA0 PA0 and PA1.
    pub fn set_joystick_button(
        &mut self,
        button: u8,
        pressed: bool,
    ) -> Result<(), DragonJoystickError> {
        self.memory.joystick.set_button(button, pressed)?;
        self.memory.refresh_pia_inputs();
        Ok(())
    }

    /// Return one Dragon analogue joystick axis value.
    pub fn joystick_axis(
        &self,
        port: u8,
        axis: DragonJoystickAxis,
    ) -> Result<u16, DragonJoystickError> {
        self.memory.joystick.checked_axis(port, axis)
    }

    /// Return whether one Dragon joystick fire line is pressed.
    pub fn joystick_button(&self, button: u8) -> Result<bool, DragonJoystickError> {
        self.memory.joystick.button(button)
    }

    /// Load a byte stream into the emulated cassette input.
    pub fn load_cassette_bytes(&mut self, bytes: Vec<u8>) {
        self.memory.cassette.load(bytes);
        self.memory.refresh_pia_inputs();
    }

    /// Load a Dragon cartridge image.
    pub fn load_cartridge(&mut self, kind: DragonCartridgeKind, rom: &[u8], autorun: bool) {
        self.memory.load_cartridge(kind, rom, autorun);
    }

    /// Remove any emulated cartridge.
    pub fn clear_cartridge(&mut self) {
        self.memory.clear_cartridge();
    }

    /// Set the normalized analogue level present on expansion connector pin 35 (`SND`).
    ///
    /// The Dragon hardware routes this external cartridge/expansion audio input through
    /// the sound mux when PIA0 selects the cartridge source. The exact voltage levels
    /// are cartridge-defined, so this API accepts a normalized `0.0..=1.0` level.
    pub fn set_cartridge_sound_level(&mut self, level: f32) {
        self.memory.set_cartridge_sound_level(level);
    }

    /// Load one machine-code program directly into RAM.
    ///
    /// This mirrors the post-load state that Dragon BASIC's `CLOADM` path leaves
    /// for machine-code programs: bytes are copied into RAM and the EXEC vector
    /// is updated to the supplied execution address. When `autorun` is true, the
    /// CPU program counter is also moved to that execution address. If the ROM
    /// has not yet initialized the hardware stack, a BASIC-idle stack value from
    /// the Dragon 32 ROM is installed so that subroutine calls return through RAM
    /// rather than the reset-time ROM vector area.
    ///
    /// # Errors
    ///
    /// Returns an error when the program would write outside the 32 KiB Dragon
    /// 32 RAM range.
    pub fn load_binary_program(
        &mut self,
        load_address: u16,
        payload: &[u8],
        exec_address: u16,
        autorun: bool,
    ) -> Result<(), DragonProgramLoadError> {
        let start = usize::from(load_address);
        let end = start
            .checked_add(payload.len())
            .ok_or(DragonProgramLoadError::RamOverflow {
                load_address,
                len: payload.len(),
            })?;
        if end > RAM_SIZE {
            return Err(DragonProgramLoadError::RamOverflow {
                load_address,
                len: payload.len(),
            });
        }

        self.memory.ram[start..end].copy_from_slice(payload);
        let exec_bytes = exec_address.to_be_bytes();
        let load_bytes = load_address.to_be_bytes();
        self.memory.ram[0x009d] = exec_bytes[0];
        self.memory.ram[0x009e] = exec_bytes[1];
        self.memory.ram[0x01e2] = 0x02;
        self.memory.ram[0x01e5] = exec_bytes[0];
        self.memory.ram[0x01e6] = exec_bytes[1];
        self.memory.ram[0x01e7] = load_bytes[0];
        self.memory.ram[0x01e8] = load_bytes[1];

        if autorun {
            if self.cpu.regs.s == 0 || usize::from(self.cpu.regs.s) >= RAM_SIZE {
                self.cpu.regs.s = DIRECT_PROGRAM_STACK_POINTER;
            }
            self.cpu.regs.pc = exec_address;
            self.cpu.reset_phase = 0;
            self.cpu.addr = exec_address;
            self.cpu.rw = true;
            self.cpu.sync = true;
            self.cpu.halt = false;
        }

        Ok(())
    }

    /// Load RAM and CPU state from a PC-Dragon snapshot.
    pub fn load_pcdragon_snapshot(
        &mut self,
        load_address: u16,
        ram: &[u8],
        registers: DragonSnapshotRegisters,
        peripherals: Option<DragonSnapshotPeripherals>,
        display_base: Option<u16>,
    ) {
        for (offset, value) in ram.iter().copied().enumerate() {
            let addr = usize::from(load_address).saturating_add(offset);
            if addr < RAM_SIZE {
                self.memory.ram[addr] = value;
            }
        }
        if let Some(display_base) = display_base {
            self.memory.sam.set_display_base(display_base);
            self.memory.sync_vdg_display_base_from_sam();
        }
        if let Some(peripherals) = peripherals {
            self.memory.restore_snapshot_peripherals(peripherals);
        }
        self.cpu.regs.pc = registers.pc;
        self.cpu.regs.x = registers.x;
        self.cpu.regs.y = registers.y;
        self.cpu.regs.u = registers.u;
        self.cpu.regs.s = registers.s;
        self.cpu.regs.dp = registers.dp;
        self.cpu.regs.b = registers.b;
        self.cpu.regs.a = registers.a;
        self.cpu.regs.cc = registers.cc;
        self.cpu.reset_phase = 0;
        self.cpu.addr = registers.pc;
        self.cpu.rw = true;
        self.cpu.sync = true;
        self.cpu.halt = false;
    }

    /// Return the live lower 32 KiB RAM page.
    #[must_use]
    pub fn ram(&self) -> &[u8; RAM_SIZE] {
        self.memory.ram[..RAM_SIZE]
            .try_into()
            .expect("lower Dragon RAM page has fixed size")
    }

    /// Remove the emulated cassette input.
    pub fn clear_cassette(&mut self) {
        self.memory.cassette.clear();
        self.memory.refresh_pia_inputs();
    }

    /// Returns whether the cassette motor relay line is on.
    #[must_use]
    pub fn cassette_motor_on(&self) -> bool {
        self.memory.pia1.ca2
    }

    /// Returns whether the cassette playback stream has finished.
    #[must_use]
    pub fn cassette_finished(&self) -> bool {
        self.memory.cassette.finished()
    }

    /// Returns the current cassette bit position.
    #[must_use]
    pub fn cassette_position_bits(&self) -> usize {
        self.memory.cassette.position_bits()
    }

    /// Returns the cassette stream length in bits.
    #[must_use]
    pub fn cassette_len_bits(&self) -> usize {
        self.memory.cassette.len_bits()
    }

    /// Returns the current cassette input level exposed on PIA1 PA0.
    #[must_use]
    pub fn cassette_line_level(&self) -> bool {
        self.memory.cassette.line_level()
    }

    /// Current PIA1 CA2 level, used by the Dragon cassette relay.
    #[must_use]
    pub fn pia1_ca2(&self) -> bool {
        self.memory.pia1.ca2
    }

    /// Current PIA0 port A data-direction register.
    #[must_use]
    pub fn pia0_ddr_a(&self) -> u8 {
        self.memory.pia0.ddr(PiaPort::A)
    }

    /// Current PIA0 port B data-direction register.
    #[must_use]
    pub fn pia0_ddr_b(&self) -> u8 {
        self.memory.pia0.ddr(PiaPort::B)
    }

    /// Current PIA0 port A control register.
    #[must_use]
    pub fn pia0_control_a(&self) -> u8 {
        self.memory.pia0.control(PiaPort::A)
    }

    /// Current PIA0 port B control register.
    #[must_use]
    pub fn pia0_control_b(&self) -> u8 {
        self.memory.pia0.control(PiaPort::B)
    }

    /// Current PIA1 CB2 level.
    #[must_use]
    pub fn pia1_cb2(&self) -> bool {
        self.memory.pia1.cb2
    }

    /// Current PIA1 port A control register.
    #[must_use]
    pub fn pia1_control_a(&self) -> u8 {
        self.memory.pia1.control(PiaPort::A)
    }

    /// Current PIA1 port B control register.
    #[must_use]
    pub fn pia1_control_b(&self) -> u8 {
        self.memory.pia1.control(PiaPort::B)
    }

    /// Current PIA1 port B data-direction register.
    #[must_use]
    pub fn pia1_ddr_b(&self) -> u8 {
        self.memory.pia1.ddr(PiaPort::B)
    }

    /// Current PIA1 port B output latch.
    #[must_use]
    pub fn pia1_output_b(&self) -> u8 {
        self.memory.pia1.output_latch(PiaPort::B)
    }

    /// Current PIA1 port B external pin levels after DDR/input/output mixing.
    #[must_use]
    pub fn pia1_pins_b(&self) -> u8 {
        self.memory.pia1.pb
    }

    /// Current SAM VDG mode latch bits V0..V2.
    #[must_use]
    pub fn sam_video_mode(&self) -> u8 {
        self.memory.sam.video_mode()
    }

    /// Current SAM display-offset latch bits F0..F6.
    #[must_use]
    pub fn sam_display_offset(&self) -> u8 {
        self.memory.sam.display_offset()
    }

    /// Current SAM-selected text screen base.
    #[must_use]
    pub fn text_screen_base(&self) -> u16 {
        self.memory.text_screen_base()
    }

    /// Current VDG-effective display base.
    #[must_use]
    pub fn video_display_base(&self) -> u16 {
        self.memory.vdg_display_base()
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

    /// Render the current MC6847 output plus border as ARGB8888.
    #[must_use]
    pub fn render_visible_argb(&self, palette: VdgPalette) -> Vec<u32> {
        let control = VdgControl::from_dragon_pia1_port_b(self.pia1_pins_b());
        motorola_vdg_6847::render_visible_argb(
            |offset| self.memory.display_byte(offset),
            control,
            palette,
        )
    }

    /// Return the progressively rendered MC6847 frame buffer as ARGB8888.
    #[must_use]
    pub fn beam_visible_argb(&self) -> &[u32] {
        self.video.frame()
    }

    /// Return the current MC6847 beam phase within the emulated PAL frame.
    #[must_use]
    pub fn video_phase(&self) -> DragonVideoPhase {
        let frame_master_tick = self.video.cycle_in_frame;
        let (visible_line, active_y, active_x) = self.video.beam_position();
        DragonVideoPhase {
            frame_master_tick,
            physical_line: video_line_for_cycle(frame_master_tick),
            line_master_tick: frame_master_tick % VDG_LINE_MASTER_TICKS,
            visible_line,
            active_y,
            active_x,
        }
    }

    /// Return the progressively rendered MC6847 frame expanded to PAL overscan.
    #[must_use]
    pub fn beam_pal_overscan_argb(&self) -> Vec<u32> {
        motorola_vdg_6847::expand_visible_argb_to_pal_overscan(
            self.video.frame(),
            VdgPalette::default().border,
        )
    }

    /// Return the Dragon mono audio output sample rate.
    #[must_use]
    pub const fn audio_sample_rate(&self) -> u32 {
        DRAGON_AUDIO_SAMPLE_RATE
    }

    /// Drain host-bound audio samples accumulated since the last call.
    pub fn drain_audio_samples(&mut self, dest: &mut Vec<f32>) {
        self.audio.drain_into(dest);
    }

    /// Execute one bus cycle and return any observed memory/device event.
    pub fn step_cycle(&mut self) -> Option<MemoryEvent> {
        self.step_cycle_with_phase_windows(None)
    }

    fn step_cycle_with_phase_windows(
        &mut self,
        mut diagnostics: Option<&mut DiagnosticCycleTrace<'_>>,
    ) -> Option<MemoryEvent> {
        let master_ticks = self.sam_timing.tick(&self.memory.sam, self.cpu.addr);
        let phase_ticks = CpuPhaseMasterTicks::split(master_ticks);

        self.memory.tick_cartridge_autorun(self.cycles);

        self.advance_phase_window(phase_ticks.q_high, diagnostics.as_deref_mut());
        self.cpu.tick_phase();

        self.advance_phase_window(phase_ticks.e_high, diagnostics.as_deref_mut());
        self.cpu.irq = self.memory.pia0.irq_a() || self.memory.pia0.irq_b();
        self.cpu.firq = self.memory.pia1.irq_a() || self.memory.pia1.irq_b();
        self.cpu.tick_phase();

        self.advance_phase_window(phase_ticks.q_low, diagnostics.as_deref_mut());
        let previous_pia1_pb = self.memory.pia1.pb;
        let event = if self.cpu.rw {
            let (value, event) = self.memory.read_bus(self.cpu.addr);
            self.cpu.data_in = value;
            event
        } else {
            self.memory.write(self.cpu.addr, self.cpu.data)
        };
        if previous_pia1_pb != self.memory.pia1.pb {
            self.video.apply_vdg_control_change(&self.memory);
        }
        self.cpu.tick_phase();

        self.advance_phase_window(phase_ticks.e_low, diagnostics);
        self.cpu.tick_phase();

        self.audio.tick(&self.memory, master_ticks);
        self.cycles = self.cycles.saturating_add(1);
        self.master_ticks = self.master_ticks.saturating_add(master_ticks);
        event
    }

    fn advance_phase_window(
        &mut self,
        master_ticks: u64,
        mut diagnostics: Option<&mut DiagnosticCycleTrace<'_>>,
    ) {
        self.memory.advance_cassette(master_ticks);
        let bus_cycle = diagnostics
            .as_ref()
            .and_then(|diagnostics| (diagnostics.trace_limit != 0).then_some(diagnostics.cycle));
        let video_tick = self.video.tick(&self.memory, master_ticks, bus_cycle);
        if let Some(diagnostics) = diagnostics.as_deref_mut() {
            let mut pending_vdg_samples = Vec::new();
            self.video.drain_pending_samples(&mut pending_vdg_samples);
            for sample in pending_vdg_samples {
                retain_vdg_sample(
                    diagnostics.vdg_samples,
                    diagnostics.dropped_vdg_samples,
                    diagnostics.trace_limit,
                    sample,
                );
            }
        }
        self.apply_video_tick(video_tick, diagnostics);
    }

    fn apply_video_tick(
        &mut self,
        video_tick: BeamVideoTick,
        mut diagnostics: Option<&mut DiagnosticCycleTrace<'_>>,
    ) {
        if let Some(level) = video_tick.hsync {
            self.memory.pia0.set_signal_level(PiaSignal::Ca1, level);
            if let Some(diagnostics) = diagnostics.as_deref_mut() {
                retain_pia_signal(
                    diagnostics.pia_signals,
                    diagnostics.dropped_pia_signals,
                    diagnostics.trace_limit,
                    pia_signal_trace(
                        diagnostics.cycle,
                        DeviceRegion::Pia0,
                        PiaSignal::Ca1,
                        level,
                        &self.memory.pia0,
                    ),
                );
            }
        }
        if let Some(level) = video_tick.frame_sync {
            self.memory.pia0.set_signal_level(PiaSignal::Cb1, level);
            if !level {
                self.memory.sync_vdg_display_base_from_sam();
            }
            if let Some(diagnostics) = diagnostics {
                retain_pia_signal(
                    diagnostics.pia_signals,
                    diagnostics.dropped_pia_signals,
                    diagnostics.trace_limit,
                    pia_signal_trace(
                        diagnostics.cycle,
                        DeviceRegion::Pia0,
                        PiaSignal::Cb1,
                        level,
                        &self.memory.pia0,
                    ),
                );
            }
        }
    }

    fn vdg_mode_write_trace(&self, cycle: u64, addr: u16, value: u8) -> VdgModeWriteTrace {
        let (line, active_y, active_x) = self.video.beam_position();
        let control = VdgControl::from_dragon_pia1_port_b(value);
        VdgModeWriteTrace {
            cycle,
            frame_master_tick: self.video.cycle_in_frame,
            line,
            active_y,
            active_x,
            addr,
            value,
            graphics: control.graphics,
            css: control.css,
            int_ext: control.int_ext,
            gm: control.gm,
        }
    }

    /// Execute up to `cycle_limit` bus cycles and retain bounded diagnostic
    /// traces.
    #[must_use]
    pub fn run_cycles(&mut self, cycle_limit: u64, trace_limit: usize) -> RunReport {
        self.run_cycles_with_options(cycle_limit, RunOptions::new(trace_limit))
    }

    /// Execute up to `cycle_limit` bus cycles and retain bounded diagnostic
    /// traces.
    #[must_use]
    pub fn run_cycles_with_options(&mut self, cycle_limit: u64, options: RunOptions) -> RunReport {
        let trace_limit = options.trace_limit;
        let start_master_ticks = self.master_ticks;
        let mut trace = Vec::new();
        let mut dropped_trace = 0usize;
        let mut device_accesses = Vec::new();
        let mut dropped_device_accesses = 0usize;
        let mut readonly_writes = Vec::new();
        let mut dropped_readonly_writes = 0usize;
        let mut watched_fetches = Vec::new();
        let mut dropped_watched_fetches = 0usize;
        let mut watched_writes = Vec::new();
        let mut dropped_watched_writes = 0usize;
        let mut pia_signals = Vec::new();
        let mut dropped_pia_signals = 0usize;
        let mut interrupt_lines = Vec::new();
        let mut dropped_interrupt_lines = 0usize;
        let mut interrupt_accepts = Vec::new();
        let mut dropped_interrupt_accepts = 0usize;
        let mut vdg_samples = Vec::new();
        let mut dropped_vdg_samples = 0usize;
        let mut vdg_mode_writes = Vec::new();
        let mut dropped_vdg_mode_writes = 0usize;
        let mut last_fetch = None;
        let mut last_irq = self.cpu.irq;
        let mut last_firq = self.cpu.firq;
        let mut run_instructions = 0u64;
        let mut run_cycles = 0u64;
        let mut stop_reason = StopReason::CycleLimit;

        for run_cycle in 0..cycle_limit {
            if self.cpu.instruction_boundary() && self.cpu.rw {
                if self.cpu.firq && !self.cpu.regs.firq_masked() {
                    retain_interrupt_accept(
                        &mut interrupt_accepts,
                        &mut dropped_interrupt_accepts,
                        trace_limit,
                        CpuInterruptAcceptTrace {
                            cycle: run_cycle,
                            kind: CpuInterruptKind::Firq,
                            pc: self.cpu.regs.pc,
                            cc: self.cpu.regs.cc,
                        },
                    );
                } else if self.cpu.irq && !self.cpu.regs.irq_masked() {
                    retain_interrupt_accept(
                        &mut interrupt_accepts,
                        &mut dropped_interrupt_accepts,
                        trace_limit,
                        CpuInterruptAcceptTrace {
                            cycle: run_cycle,
                            kind: CpuInterruptKind::Irq,
                            pc: self.cpu.regs.pc,
                            cc: self.cpu.regs.cc,
                        },
                    );
                }
                let fetch = FetchTrace {
                    cycle: run_cycle,
                    master_tick: self.master_ticks,
                    pc: self.cpu.addr,
                    opcode: self.memory.read_fetch(self.cpu.addr),
                };
                last_fetch = Some(fetch);
                self.instructions = self.instructions.saturating_add(1);
                run_instructions = run_instructions.saturating_add(1);
                retain_trace(&mut trace, &mut dropped_trace, trace_limit, fetch);
                if options
                    .fetch_watch
                    .iter()
                    .any(|range| range.contains(fetch.pc))
                {
                    retain_watched_fetch(
                        &mut watched_fetches,
                        &mut dropped_watched_fetches,
                        trace_limit,
                        WatchedFetchTrace {
                            cycle: run_cycle,
                            master_tick: fetch.master_tick,
                            pc: fetch.pc,
                            opcode: fetch.opcode,
                            regs: cpu_register_trace(self),
                        },
                    );
                }
            }

            let watched_write = options
                .write_watch
                .iter()
                .any(|range| !self.cpu.rw && range.contains(self.cpu.addr))
                .then(|| {
                    (
                        last_fetch.map(|fetch| fetch.pc),
                        self.cpu.addr,
                        self.cpu.data,
                        cpu_register_trace(self),
                    )
                });

            let event = self.step_cycle_with_diagnostic_trace(
                run_cycle,
                &mut pia_signals,
                &mut dropped_pia_signals,
                &mut vdg_samples,
                &mut dropped_vdg_samples,
                trace_limit,
            );
            run_cycles = run_cycle.saturating_add(1);

            if self.cpu.irq != last_irq {
                last_irq = self.cpu.irq;
                retain_interrupt_line(
                    &mut interrupt_lines,
                    &mut dropped_interrupt_lines,
                    trace_limit,
                    CpuInterruptLineTrace {
                        cycle: run_cycle,
                        kind: CpuInterruptKind::Irq,
                        level: self.cpu.irq,
                        pc: self.cpu.regs.pc,
                        cc: self.cpu.regs.cc,
                    },
                );
            }
            if self.cpu.firq != last_firq {
                last_firq = self.cpu.firq;
                retain_interrupt_line(
                    &mut interrupt_lines,
                    &mut dropped_interrupt_lines,
                    trace_limit,
                    CpuInterruptLineTrace {
                        cycle: run_cycle,
                        kind: CpuInterruptKind::Firq,
                        level: self.cpu.firq,
                        pc: self.cpu.regs.pc,
                        cc: self.cpu.regs.cc,
                    },
                );
            }

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
                    if device == DeviceRegion::Pia1 && (addr & 0x03) == 0x02 {
                        let trace = self.vdg_mode_write_trace(run_cycle, addr, value);
                        retain_vdg_mode_write(
                            &mut vdg_mode_writes,
                            &mut dropped_vdg_mode_writes,
                            trace_limit,
                            trace,
                        );
                    }
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

            if let Some(write) = watched_write {
                let (line, active_y, active_x) = self.video.beam_position();
                retain_watched_write(
                    &mut watched_writes,
                    &mut dropped_watched_writes,
                    trace_limit,
                    MemoryWriteTrace {
                        cycle: run_cycle,
                        frame_master_tick: self.video.cycle_in_frame,
                        line,
                        active_y,
                        active_x,
                        instruction_pc: write.0,
                        addr: write.1,
                        value: write.2,
                        regs: write.3,
                    },
                );
            }

            if self.cpu.halt {
                stop_reason = StopReason::CpuHalted;
                break;
            }
        }

        RunReport {
            stop_reason,
            cycles: run_cycles,
            master_ticks: self.master_ticks.saturating_sub(start_master_ticks),
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
            watched_fetches,
            dropped_watched_fetches,
            watched_writes,
            dropped_watched_writes,
            pia_signals,
            dropped_pia_signals,
            interrupt_lines,
            dropped_interrupt_lines,
            interrupt_accepts,
            dropped_interrupt_accepts,
            vdg_samples,
            dropped_vdg_samples,
            vdg_mode_writes,
            dropped_vdg_mode_writes,
            text_screen_base: self.text_screen_base(),
        }
    }

    fn step_cycle_with_diagnostic_trace(
        &mut self,
        cycle: u64,
        pia_signals: &mut Vec<PiaSignalTrace>,
        dropped_pia_signals: &mut usize,
        vdg_samples: &mut Vec<VdgSampleTrace>,
        dropped_vdg_samples: &mut usize,
        trace_limit: usize,
    ) -> Option<MemoryEvent> {
        let mut diagnostics = DiagnosticCycleTrace {
            cycle,
            pia_signals,
            dropped_pia_signals,
            vdg_samples,
            dropped_vdg_samples,
            trace_limit,
        };
        self.step_cycle_with_phase_windows(Some(&mut diagnostics))
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

fn retain_vdg_mode_write(
    writes: &mut Vec<VdgModeWriteTrace>,
    dropped_writes: &mut usize,
    write_limit: usize,
    write: VdgModeWriteTrace,
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

fn retain_watched_write(
    writes: &mut Vec<MemoryWriteTrace>,
    dropped_writes: &mut usize,
    write_limit: usize,
    write: MemoryWriteTrace,
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

fn retain_watched_fetch(
    fetches: &mut Vec<WatchedFetchTrace>,
    dropped_fetches: &mut usize,
    fetch_limit: usize,
    fetch: WatchedFetchTrace,
) {
    if fetch_limit == 0 {
        *dropped_fetches = dropped_fetches.saturating_add(1);
        return;
    }

    if fetches.len() == fetch_limit {
        fetches.remove(0);
        *dropped_fetches = dropped_fetches.saturating_add(1);
    }
    fetches.push(fetch);
}

fn cpu_register_trace(machine: &Dragon32) -> CpuRegisterTrace {
    CpuRegisterTrace {
        a: machine.cpu.regs.a,
        b: machine.cpu.regs.b,
        dp: machine.cpu.regs.dp,
        cc: machine.cpu.regs.cc,
        x: machine.cpu.regs.x,
        y: machine.cpu.regs.y,
        u: machine.cpu.regs.u,
        s: machine.cpu.regs.s,
        pc: machine.cpu.regs.pc,
    }
}

fn retain_pia_signal(
    signals: &mut Vec<PiaSignalTrace>,
    dropped_signals: &mut usize,
    trace_limit: usize,
    signal: PiaSignalTrace,
) {
    if trace_limit == 0 {
        *dropped_signals = dropped_signals.saturating_add(1);
        return;
    }

    if signals.len() == trace_limit {
        signals.remove(0);
        *dropped_signals = dropped_signals.saturating_add(1);
    }
    signals.push(signal);
}

fn retain_interrupt_line(
    lines: &mut Vec<CpuInterruptLineTrace>,
    dropped_lines: &mut usize,
    trace_limit: usize,
    line: CpuInterruptLineTrace,
) {
    if trace_limit == 0 {
        *dropped_lines = dropped_lines.saturating_add(1);
        return;
    }

    if lines.len() == trace_limit {
        lines.remove(0);
        *dropped_lines = dropped_lines.saturating_add(1);
    }
    lines.push(line);
}

fn retain_interrupt_accept(
    accepts: &mut Vec<CpuInterruptAcceptTrace>,
    dropped_accepts: &mut usize,
    trace_limit: usize,
    accept: CpuInterruptAcceptTrace,
) {
    if trace_limit == 0 {
        *dropped_accepts = dropped_accepts.saturating_add(1);
        return;
    }

    if accepts.len() == trace_limit {
        accepts.remove(0);
        *dropped_accepts = dropped_accepts.saturating_add(1);
    }
    accepts.push(accept);
}

fn retain_vdg_sample(
    samples: &mut Vec<VdgSampleTrace>,
    dropped_samples: &mut usize,
    trace_limit: usize,
    sample: VdgSampleTrace,
) {
    if trace_limit == 0 {
        *dropped_samples = dropped_samples.saturating_add(1);
        return;
    }

    if samples.len() == trace_limit {
        samples.remove(0);
        *dropped_samples = dropped_samples.saturating_add(1);
    }
    samples.push(sample);
}

fn pia_signal_trace(
    cycle: u64,
    device: DeviceRegion,
    signal: PiaSignal,
    level: bool,
    pia: &Pia6821,
) -> PiaSignalTrace {
    PiaSignalTrace {
        cycle,
        device,
        signal,
        level,
        control_a: pia.control(PiaPort::A),
        control_b: pia.control(PiaPort::B),
        irq_a: pia.irq_a(),
        irq_b: pia.irq_b(),
    }
}

fn decode_pia(addr: u16) -> Option<(DeviceRegion, u8)> {
    match addr {
        0xFF00..=0xFF1F => Some((DeviceRegion::Pia0, (addr & 0x03) as u8)),
        0xFF20..=0xFF3F => Some((DeviceRegion::Pia1, (addr & 0x03) as u8)),
        _ => None,
    }
}

fn decode_acia(model: DragonHardwareModel, addr: u16) -> Option<u8> {
    (matches!(
        model,
        DragonHardwareModel::Dragon64Compat | DragonHardwareModel::Dragon64Mode
    ) && (0xFF04..=0xFF07).contains(&addr))
    .then_some((addr & 0x03) as u8)
}

fn acia_read(addr: u16) -> u8 {
    match addr & 0x03 {
        0x01 => ACIA_STATUS_TRANSMIT_DATA_REGISTER_EMPTY,
        _ => 0x00,
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
    use motorola_vdg_6847::{
        DEFAULT_VDG_BLUE, TEXT_LEFT_BORDER_PIXELS, TEXT_TOP_BORDER_LINES,
        TEXT_VISIBLE_FRAMEBUFFER_HEIGHT, TEXT_VISIBLE_FRAMEBUFFER_WIDTH, VdgPalette,
    };

    use super::*;

    fn rom_with_reset_vector(pc: u16) -> [u8; ROM_SIZE] {
        let mut rom = [0; ROM_SIZE];
        let [hi, lo] = pc.to_be_bytes();
        rom[0x3FFE] = hi;
        rom[0x3FFF] = lo;
        rom
    }

    fn assert_sample_near(actual: f32, expected: f32) {
        assert!(
            (actual - expected).abs() < 0.000_001,
            "expected {actual} to be within tolerance of {expected}"
        );
    }

    fn total_phase_ticks(ticks: CpuPhaseMasterTicks) -> u64 {
        ticks.q_high + ticks.e_high + ticks.q_low + ticks.e_low
    }

    fn text_control() -> VdgControl {
        VdgControl::default()
    }

    fn graphics_control(gm: u8) -> VdgControl {
        VdgControl {
            graphics: true,
            gm,
            ..VdgControl::default()
        }
    }

    #[test]
    fn memory_maps_basic_rom_vectors_and_empty_cartridge_slot() {
        let mut rom = rom_with_reset_vector(0x8000);
        rom[0] = 0x12;
        rom[0x3FFE] = 0x80;
        rom[0x3FFF] = 0x00;

        let memory = DragonMemory::new_with_keyboard(&rom, DragonKeyboard::new());

        assert_eq!(memory.read_fetch(0x8000), 0x12);
        assert_eq!(memory.read_fetch(0xC000), NO_CARTRIDGE_BUS_VALUE);
        assert_eq!(memory.read_fetch(0xFEFF), NO_CARTRIDGE_BUS_VALUE);
        assert_eq!(memory.read_fetch(0xFFFE), 0x80);
        assert_eq!(memory.read_fetch(0xFFFF), 0x00);
    }

    #[test]
    fn dragon64_mode_maps_base_ram_and_system_rom_at_c000() {
        let mut rom = rom_with_reset_vector(0xC000);
        rom[0] = 0x12;
        rom[0x3FFE] = 0xC0;
        rom[0x3FFF] = 0x00;

        let mut memory = DragonMemory::new_with_keyboard_and_model(
            &rom,
            DragonKeyboard::new(),
            DragonHardwareModel::Dragon64Mode,
        );

        memory.write(0x8000, 0x34);
        memory.write(0xBFFF, 0x56);

        assert_eq!(memory.read_fetch(0x8000), 0x34);
        assert_eq!(memory.read_fetch(0xBFFF), 0x56);
        assert_eq!(memory.read_fetch(0xC000), 0x12);
        assert_eq!(memory.read_fetch(0xFFFE), 0xC0);
        assert_eq!(memory.read_fetch(0xFFFF), 0x00);
    }

    #[test]
    fn dragon64_decodes_acia_before_pia0_mirror() {
        let rom = rom_with_reset_vector(0xC000);
        let mut memory = DragonMemory::new_with_keyboard_and_model(
            &rom,
            DragonKeyboard::new(),
            DragonHardwareModel::Dragon64Compat,
        );

        let (value, event) = memory.read_bus(0xFF05);

        assert_eq!(value, ACIA_STATUS_TRANSMIT_DATA_REGISTER_EMPTY);
        assert_eq!(
            event,
            Some(MemoryEvent::DeviceRead {
                device: DeviceRegion::Acia,
                addr: 0xFF05,
                value: ACIA_STATUS_TRANSMIT_DATA_REGISTER_EMPTY,
            })
        );
    }

    #[test]
    fn sam_page_select_switches_lower_thirty_two_kib_ram_page() {
        let rom = rom_with_reset_vector(0x8000);
        let mut memory = DragonMemory::new_with_keyboard(&rom, DragonKeyboard::new());

        memory.write(0x0000, 0x11);
        memory.write(0xFFD5, 0x00); // SAM P1 set: low MPU addresses select page 1.
        memory.write(0x0000, 0x22);

        assert_eq!(memory.read_fetch(0x0000), 0x22);
        memory.write(0xFFD4, 0x00); // SAM P1 clear: low MPU addresses select page 0.
        assert_eq!(memory.read_fetch(0x0000), 0x11);
        memory.write(0xFFD5, 0x00);
        assert_eq!(memory.read_fetch(0x0000), 0x22);
    }

    #[test]
    fn sam_type_one_maps_high_reads_and_writes_to_ram_below_io_page() {
        let mut rom = rom_with_reset_vector(0x8000);
        rom[0] = 0x12;
        rom[0x3FFE] = 0x80;
        rom[0x3FFF] = 0x00;
        let mut memory = DragonMemory::new_with_keyboard(&rom, DragonKeyboard::new());
        let cart = vec![0x34; 0x4000];
        memory.load_cartridge(DragonCartridgeKind::Rom, &cart, true);

        assert_eq!(memory.read_fetch(0x8000), 0x12);
        assert_eq!(memory.read_fetch(0xC000), 0x34);

        memory.write(0xFFDF, 0x00); // SAM TY set: map type #1, RAM-based system.
        memory.write(0x8000, 0x45);
        memory.write(0xC000, 0x67);
        memory.write(0xFEFF, 0x89);

        assert_eq!(memory.read_fetch(0x8000), 0x45);
        assert_eq!(memory.read_fetch(0xC000), 0x67);
        assert_eq!(memory.read_fetch(0xFEFF), 0x89);
        assert_eq!(memory.read_fetch(0xFFFE), 0x80);
        assert_eq!(memory.read_fetch(0xFFFF), 0x00);
    }

    #[test]
    fn cpu_phase_master_ticks_preserve_sam_cycle_lengths() {
        let slow = CpuPhaseMasterTicks::split(16);
        assert_eq!(
            slow,
            CpuPhaseMasterTicks {
                q_high: 4,
                e_high: 4,
                q_low: 4,
                e_low: 4
            }
        );
        assert_eq!(total_phase_ticks(slow), 16);

        let fast = CpuPhaseMasterTicks::split(8);
        assert_eq!(
            fast,
            CpuPhaseMasterTicks {
                q_high: 2,
                e_high: 2,
                q_low: 2,
                e_low: 2
            }
        );
        assert_eq!(total_phase_ticks(fast), 8);

        let entry_fast = CpuPhaseMasterTicks::split(15);
        assert_eq!(
            entry_fast,
            CpuPhaseMasterTicks {
                q_high: 4,
                e_high: 4,
                q_low: 4,
                e_low: 3
            }
        );
        assert_eq!(total_phase_ticks(entry_fast), 15);

        let extended_slow = CpuPhaseMasterTicks::split(25);
        assert_eq!(total_phase_ticks(extended_slow), 25);
        assert!(extended_slow.q_high >= extended_slow.e_low);
    }

    #[test]
    fn vdg_horizontal_timing_uses_mc6847_clock_periods() {
        assert_eq!(VDG_LINE_MASTER_TICKS, 228 * VDG_CLOCK_MASTER_TICKS);
        assert_eq!(
            motorola_vdg_6847::TEXT_FRAMEBUFFER_WIDTH as u64 * 2,
            128 * VDG_CLOCK_MASTER_TICKS
        );
        assert_eq!(
            vdg_fetch_to_display_ticks(text_control()),
            VDG_SHORT_CYCLE_FETCH_CLOCKS * VDG_CLOCK_MASTER_TICKS
        );
        assert_eq!(
            vdg_fetch_to_display_ticks(graphics_control(0)),
            VDG_LONG_CYCLE_FETCH_CLOCKS * VDG_CLOCK_MASTER_TICKS
        );
        assert_eq!(
            vdg_fetch_to_display_ticks(graphics_control(6)),
            VDG_SHORT_CYCLE_FETCH_CLOCKS * VDG_CLOCK_MASTER_TICKS
        );
    }

    #[test]
    fn step_cycle_drives_cpu_through_complete_eq_phase_sequence() {
        let rom = rom_with_reset_vector(0x8000);
        let mut machine = Dragon32::new(&rom);

        assert_eq!(machine.cpu_clock_phase(), Mc6809ClockPhase::QHigh);
        assert_eq!(machine.bus_addr(), 0xFFFE);

        machine.step_cycle();
        assert_eq!(machine.cpu_clock_phase(), Mc6809ClockPhase::QHigh);
        assert_eq!(machine.bus_addr(), 0xFFFF);

        machine.step_cycle();
        assert_eq!(machine.cpu_clock_phase(), Mc6809ClockPhase::QHigh);
        assert_eq!(machine.pc(), 0x8000);
        assert_eq!(machine.bus_addr(), 0x8000);
        assert!(machine.bus_rw());
        assert_eq!(machine.cycles(), 2);
        assert_eq!(machine.master_ticks(), 32);
    }

    #[test]
    fn cartridge_rom_overlays_c000_without_replacing_vectors() {
        let mut rom = rom_with_reset_vector(0x8000);
        rom[0] = 0x12;
        rom[0x3FFE] = 0x80;
        rom[0x3FFF] = 0x00;
        let mut memory = DragonMemory::new_with_keyboard(&rom, DragonKeyboard::new());
        let mut cart = vec![0xff; 0x4000];
        cart[0] = 0x34;

        memory.load_cartridge(DragonCartridgeKind::Rom, &cart, true);

        assert_eq!(memory.read_fetch(0x8000), 0x12);
        assert_eq!(memory.read_fetch(0xC000), 0x34);
        assert_eq!(memory.read_fetch(0xFEFF), 0xff);
        assert_eq!(memory.read_fetch(0xFFFE), 0x80);
        assert_eq!(memory.read_fetch(0xFFFF), 0x00);
    }

    #[test]
    fn games_master_cartridge_switches_sixteen_kib_banks() {
        let rom = rom_with_reset_vector(0x8000);
        let mut memory = DragonMemory::new_with_keyboard(&rom, DragonKeyboard::new());
        let mut cart = vec![0xff; 0x8000];
        cart[0] = 0x10;
        cart[0x4000] = 0x20;

        memory.load_cartridge(DragonCartridgeKind::GamesMaster, &cart, true);

        assert_eq!(memory.read_fetch(0xC000), 0x10);
        memory.write(0xFF40, 0x01);
        assert_eq!(memory.read_fetch(0xC000), 0x20);
    }

    #[test]
    fn pcdragon_snapshot_restores_display_peripheral_state() {
        let rom = rom_with_reset_vector(0x8000);
        let mut machine = Dragon32::new(&rom);

        machine.load_pcdragon_snapshot(
            0x2000,
            &[0xaa],
            DragonSnapshotRegisters {
                pc: 0x1234,
                x: 0,
                y: 0,
                u: 0,
                s: 0,
                dp: 0,
                b: 0,
                a: 0,
                cc: 0,
            },
            Some(DragonSnapshotPeripherals {
                ff02: 0xff,
                ff03: 0xb5,
                ff22: 0xfc,
            }),
            Some(0x0600),
        );

        assert_eq!(machine.pia0_control_b(), 0xb5);
        assert_eq!(machine.pia0_ddr_b(), 0xff);
        assert_eq!(machine.pia1_pins_b(), 0xfc);
        assert_eq!(machine.text_screen_base(), 0x0600);
        assert_eq!(machine.sam_video_mode(), 6);
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
                cycle: 7,
                master_tick: 112,
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
    fn machine_records_writes_matching_any_watch_range() {
        let mut rom = rom_with_reset_vector(0x8000);
        rom[0x0000] = 0x86; // LDA #$12
        rom[0x0001] = 0x12;
        rom[0x0002] = 0xB7; // STA $0020
        rom[0x0003] = 0x00;
        rom[0x0004] = 0x20;
        rom[0x0005] = 0x86; // LDA #$34
        rom[0x0006] = 0x34;
        rom[0x0007] = 0xB7; // STA $0040
        rom[0x0008] = 0x00;
        rom[0x0009] = 0x40;
        rom[0x000A] = 0x01;

        let mut machine = Dragon32::new(&rom);
        let mut options = RunOptions::new(8);
        options.write_watch = vec![
            AddressRange::new(0x0020, 0x0020),
            AddressRange::new(0x0040, 0x0040),
        ];
        let report = machine.run_cycles_with_options(128, options);

        assert_eq!(report.stop_reason, StopReason::CpuHalted);
        assert_eq!(report.watched_writes.len(), 2);
        assert_eq!(report.watched_writes[0].addr, 0x0020);
        assert_eq!(report.watched_writes[0].value, 0x12);
        assert_eq!(report.watched_writes[1].addr, 0x0040);
        assert_eq!(report.watched_writes[1].value, 0x34);
    }

    #[test]
    fn active_area_end_raises_pia0_cb1_frame_sync_falling_edge() {
        let rom = rom_with_reset_vector(0x8000);
        let mut machine = Dragon32::new(&rom);

        machine.memory.pia0.set_signal_level(PiaSignal::Cb1, true);
        machine.memory.pia0.write(0x03, 0x05); // PIA0 CB1 falling-edge IRQ enabled.
        machine.video.cycle_in_frame = VDG_FRAME_SYNC_FALL_TICK - 1;
        machine.video.next_line = TEXT_VISIBLE_FRAMEBUFFER_HEIGHT;

        machine.step_cycle();

        assert_eq!(machine.memory.pia0.control(PiaPort::B) & 0x80, 0x80);
        assert!(machine.memory.pia0.irq_b());
        assert_eq!(machine.memory.pia0.read(0x02), 0xff);
        assert_eq!(machine.memory.pia0.control(PiaPort::B) & 0x80, 0);
    }

    #[test]
    fn sam_display_base_writes_update_vdg_on_frame_sync_fall() {
        let rom = rom_with_reset_vector(0x8000);
        let mut machine = Dragon32::new(&rom);

        machine.memory.write(0xFFC9, 0x00); // Set SAM F1, selecting $0400.

        assert_eq!(machine.text_screen_base(), 0x0400);
        assert_eq!(machine.video_display_base(), 0x0000);

        machine.memory.pia0.set_signal_level(PiaSignal::Cb1, true);
        machine.video.cycle_in_frame = VDG_FRAME_SYNC_FALL_TICK - 1;
        machine.video.next_line = TEXT_VISIBLE_FRAMEBUFFER_HEIGHT;
        machine.step_cycle();

        assert_eq!(machine.video_display_base(), 0x0400);
    }

    #[test]
    fn sam_display_base_writes_after_frame_sync_fall_wait_for_next_fall() {
        let rom = rom_with_reset_vector(0x8000);
        let mut machine = Dragon32::new(&rom);

        machine.video.cycle_in_frame = VDG_FRAME_SYNC_FALL_TICK + SLOW_CPU_MASTER_TICKS;
        machine.video.next_line = TEXT_VISIBLE_FRAMEBUFFER_HEIGHT;
        machine.memory.write(0xFFC9, 0x00); // Set SAM F1, selecting $0400.
        machine.step_cycle();

        assert_eq!(machine.text_screen_base(), 0x0400);
        assert_eq!(machine.video_display_base(), 0x0000);

        machine.memory.pia0.set_signal_level(PiaSignal::Cb1, true);
        machine.video.cycle_in_frame = VDG_FRAME_SYNC_FALL_TICK - 1;
        machine.step_cycle();

        assert_eq!(machine.video_display_base(), 0x0400);
    }

    #[test]
    fn vblank_start_raises_pia0_cb1_frame_sync_rising_edge() {
        let rom = rom_with_reset_vector(0x8000);
        let mut machine = Dragon32::new(&rom);

        machine.memory.pia0.set_signal_level(PiaSignal::Cb1, false);
        machine.memory.pia0.write(0x03, 0x07); // PIA0 CB1 rising-edge IRQ enabled.
        machine.video.cycle_in_frame = VDG_FRAME_SYNC_RISE_TICK - 1;
        machine.video.next_line = TEXT_VISIBLE_FRAMEBUFFER_HEIGHT;

        machine.step_cycle();

        assert_eq!(machine.memory.pia0.control(PiaPort::B) & 0x80, 0x80);
        assert!(machine.memory.pia0.irq_b());
        assert_eq!(machine.memory.pia0.read(0x02), 0xff);
        assert_eq!(machine.memory.pia0.control(PiaPort::B) & 0x80, 0);
    }

    #[test]
    fn visible_line_start_raises_pia0_ca1_hsync_falling_edge() {
        let rom = rom_with_reset_vector(0x8000);
        let mut machine = Dragon32::new(&rom);

        machine.memory.pia0.write(0x01, 0x05); // PIA0 CA1 falling-edge IRQ enabled.
        machine.video.cycle_in_frame = line_start_cycle(1) - 1;
        machine.video.next_line = 1;

        machine.step_cycle();

        assert_eq!(machine.memory.pia0.control(PiaPort::A) & 0x80, 0x80);
        assert!(machine.memory.pia0.irq_a());
        assert_eq!(machine.memory.pia0.read(0x00), 0xff);
        assert_eq!(machine.memory.pia0.control(PiaPort::A) & 0x80, 0);
    }

    #[test]
    fn visible_line_hsync_rise_raises_pia0_ca1_rising_edge() {
        let rom = rom_with_reset_vector(0x8000);
        let mut machine = Dragon32::new(&rom);

        let hsync_rise = line_start_cycle(1).saturating_add(VDG_HSYNC_TICKS);
        machine.memory.pia0.set_signal_level(PiaSignal::Ca1, false);
        machine.memory.pia0.write(0x01, 0x07); // PIA0 CA1 rising-edge IRQ enabled.
        machine.video.cycle_in_frame = hsync_rise - 1;
        machine.video.next_line = 1;

        machine.step_cycle();

        assert_eq!(machine.memory.pia0.control(PiaPort::A) & 0x80, 0x80);
        assert!(machine.memory.pia0.irq_a());
        assert_eq!(machine.memory.pia0.read(0x00), 0xff);
        assert_eq!(machine.memory.pia0.control(PiaPort::A) & 0x80, 0);
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
    fn keyboard_enter_pulls_row_six_low_when_column_zero_is_selected() {
        let keyboard = DragonKeyboard::with_pressed_keys(&[MatrixKey::new(6, 0)])
            .expect("enter matrix key should be valid");

        assert_eq!(keyboard.port_a_input(0xFE), 0xBF);
        assert_eq!(keyboard.port_a_input(0xFD), 0xFF);
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
    fn joystick_axis_comparator_drives_pia0_port_a_bit_seven() {
        let rom = rom_with_reset_vector(0x8000);
        let mut memory = DragonMemory::new_with_keyboard(&rom, DragonKeyboard::new());
        memory.pia0.write(0x01, 0x34); // PIA0 CA2 low: X axis, data register selected.
        memory.pia0.write(0x03, 0x34); // PIA0 CB2 low: joystick port 0, data register selected.
        memory.pia1.write(0x00, 0xFC); // PIA1 PA2-PA7 are DAC outputs.
        memory.pia1.write(0x01, 0x04); // PIA1 port A data register selected.
        memory.pia1.write(0x00, 0x80); // Comparator threshold around centre.

        memory
            .joystick
            .set_axis(0, DragonJoystickAxis::X, DRAGON_JOYSTICK_MIN)
            .expect("port 0 X axis should exist");
        memory.refresh_pia_inputs();
        let (low_value, _) = memory.read_bus(0xFF00);
        assert_eq!(low_value & 0x80, 0);

        memory
            .joystick
            .set_axis(0, DragonJoystickAxis::X, DRAGON_JOYSTICK_MAX)
            .expect("port 0 X axis should exist");
        memory.refresh_pia_inputs();
        let (high_value, _) = memory.read_bus(0xFF00);
        assert_eq!(high_value & 0x80, 0x80);
    }

    #[test]
    fn joystick_comparator_threshold_uses_pia1_pa2_to_pa7() {
        assert_eq!(joystick_threshold_from_dac(0x00), 0x0200);
        assert_eq!(joystick_threshold_from_dac(0x03), 0x0200);
        assert_eq!(joystick_threshold_from_dac(0x80), 0x8200);
        assert_eq!(joystick_threshold_from_dac(0x83), 0x8200);
        assert_eq!(joystick_threshold_from_dac(0xFC), 0xFE00);
        assert_eq!(joystick_threshold_from_dac(0xFF), 0xFE00);
    }

    #[test]
    fn joystick_comparator_switches_high_at_dac_threshold() {
        let rom = rom_with_reset_vector(0x8000);
        let mut memory = DragonMemory::new_with_keyboard(&rom, DragonKeyboard::new());
        memory.pia0.write(0x01, 0x34); // PIA0 CA2 low: X axis, data register selected.
        memory.pia0.write(0x03, 0x34); // PIA0 CB2 low: joystick port 0, data register selected.
        memory.pia1.write(0x00, 0xFC); // PIA1 PA2-PA7 are DAC outputs.
        memory.pia1.write(0x01, 0x04); // PIA1 port A data register selected.
        memory.pia1.write(0x00, 0x80);
        let threshold = joystick_threshold_from_dac(0x80);

        memory
            .joystick
            .set_axis(0, DragonJoystickAxis::X, threshold - 1)
            .expect("port 0 X axis should exist");
        memory.refresh_pia_inputs();
        let (below_value, _) = memory.read_bus(0xFF00);
        assert_eq!(below_value & 0x80, 0);

        memory
            .joystick
            .set_axis(0, DragonJoystickAxis::X, threshold)
            .expect("port 0 X axis should exist");
        memory.refresh_pia_inputs();
        let (equal_value, _) = memory.read_bus(0xFF00);
        assert_eq!(equal_value & 0x80, 0x80);
    }

    #[test]
    fn joystick_axis_comparator_honours_pia0_axis_and_port_selects() {
        let rom = rom_with_reset_vector(0x8000);
        let mut memory = DragonMemory::new_with_keyboard(&rom, DragonKeyboard::new());
        memory.pia0.write(0x01, 0x3C); // PIA0 CA2 high: Y axis, data register selected.
        memory.pia0.write(0x03, 0x3C); // PIA0 CB2 high: joystick port 1, data register selected.
        memory.pia1.write(0x00, 0xFC);
        memory.pia1.write(0x01, 0x04);
        memory.pia1.write(0x00, 0x80);
        memory
            .joystick
            .set_axis(0, DragonJoystickAxis::Y, DRAGON_JOYSTICK_MAX)
            .expect("port 0 Y axis should exist");
        memory
            .joystick
            .set_axis(1, DragonJoystickAxis::X, DRAGON_JOYSTICK_MAX)
            .expect("port 1 X axis should exist");
        memory
            .joystick
            .set_axis(1, DragonJoystickAxis::Y, DRAGON_JOYSTICK_MIN)
            .expect("port 1 Y axis should exist");

        memory.refresh_pia_inputs();
        let (selected_value, _) = memory.read_bus(0xFF00);

        assert_eq!(selected_value & 0x80, 0);
    }

    #[test]
    fn joystick_buttons_pull_pia0_port_a_low_bits_low() {
        let rom = rom_with_reset_vector(0x8000);
        let mut memory = DragonMemory::new_with_keyboard(&rom, DragonKeyboard::new());
        memory.pia0.write(0x01, 0x04); // PIA0 port A data register selected.
        memory
            .joystick
            .set_button(0, true)
            .expect("fire button 0 should exist");
        memory
            .joystick
            .set_button(1, true)
            .expect("fire button 1 should exist");

        memory.refresh_pia_inputs();
        let (pressed_value, _) = memory.read_bus(0xFF00);
        assert_eq!(pressed_value & 0x03, 0);

        memory
            .joystick
            .set_button(0, false)
            .expect("fire button 0 should exist");
        memory.refresh_pia_inputs();
        let (released_value, _) = memory.read_bus(0xFF00);
        assert_eq!(released_value & 0x03, 0x01);
    }

    #[test]
    fn cassette_playback_advances_only_when_motor_relay_is_on() {
        let rom = rom_with_reset_vector(0x8000);
        let mut memory = DragonMemory::new_with_keyboard(&rom, DragonKeyboard::new());
        memory.cassette.load(vec![0x01]);

        memory.advance_cassette(SLOW_CPU_MASTER_TICKS);
        assert_eq!(memory.cassette.position_bits(), 0);
        assert!(!memory.cassette.line_level());

        memory.pia1.write(0x01, 0x3C); // PIA1 CA2 high, port A data selected.
        memory.advance_cassette(CASSETTE_ONE_HALF_PERIOD_TICKS);
        assert_eq!(memory.cassette.position_bits(), 0);
        assert!(memory.cassette.line_level());

        memory.advance_cassette(CASSETTE_ONE_HALF_PERIOD_TICKS);
        assert_eq!(memory.cassette.position_bits(), 1);
        assert!(!memory.cassette.line_level());
    }

    #[test]
    fn cassette_input_drives_pia1_port_a_bit_zero() {
        let rom = rom_with_reset_vector(0x8000);
        let mut memory = DragonMemory::new_with_keyboard(&rom, DragonKeyboard::new());
        memory.cassette.load(vec![0x01]);
        memory.pia1.write(0x01, 0x04); // PIA1 port A data register selected.

        memory.refresh_pia_inputs();
        let (low_value, _) = memory.read_bus(0xFF20);
        assert_eq!(low_value & 0x01, 0);

        memory.pia1.write(0x01, 0x3C); // PIA1 CA2 high, port A data selected.
        memory.advance_cassette(CASSETTE_ONE_HALF_PERIOD_TICKS);
        let (high_value, _) = memory.read_bus(0xFF20);
        assert_eq!(high_value & 0x01, 1);
    }

    #[test]
    fn audio_samples_follow_pia1_dac_when_mux_selects_dac() {
        let rom = rom_with_reset_vector(0x8000);
        let mut machine = Dragon32::new(&rom);
        machine.memory.pia1.write(0x00, 0xFC); // PIA1 PA2-PA7 drive the DAC.
        machine.memory.pia1.write(0x01, 0x04); // PIA1 port A data selected.
        machine.memory.pia1.write(0x03, 0x3C); // PIA1 CB2 high: enable sound mux.

        machine.memory.pia1.write(0x00, 0x00);
        let _ = machine.run_cycles(128, 0);
        let mut low_samples = Vec::new();
        machine.drain_audio_samples(&mut low_samples);

        machine.memory.pia1.write(0x00, 0xFC);
        let _ = machine.run_cycles(128, 0);
        let mut high_samples = Vec::new();
        machine.drain_audio_samples(&mut high_samples);

        assert!(!low_samples.is_empty());
        assert!(!high_samples.is_empty());
        assert!(low_samples.iter().all(|sample| *sample < 0.04));
        assert!(high_samples.iter().all(|sample| *sample > 0.69));
    }

    #[test]
    fn audio_dac_levels_match_xroar_voltage_model() {
        let rom = rom_with_reset_vector(0x8000);
        let mut memory = DragonMemory::new_with_keyboard(&rom, DragonKeyboard::new());
        memory.pia1.write(0x00, 0xFC); // PIA1 PA2-PA7 drive the DAC.
        memory.pia1.write(0x01, 0x04); // PIA1 port A data selected.
        memory.pia1.write(0x03, 0x3C); // PIA1 CB2 high: enable sound mux.

        memory.pia1.write(0x00, 0x00);
        assert_sample_near(memory.audio_sample(), (0.20 / 4.70) * 0.7);

        memory.pia1.write(0x00, 0xFC);
        assert_sample_near(memory.audio_sample(), ((4.50 + 0.20) / 4.70) * 0.7);
    }

    #[test]
    fn audio_mux_is_disabled_until_pia1_cb2_enables_it() {
        let rom = rom_with_reset_vector(0x8000);
        let mut memory = DragonMemory::new_with_keyboard(&rom, DragonKeyboard::new());
        memory.pia1.write(0x00, 0xFC); // PIA1 PA2-PA7 drive the DAC.
        memory.pia1.write(0x01, 0x04); // PIA1 port A data selected.
        memory.pia1.write(0x00, 0xFC);

        assert_eq!(memory.muxed_audio_source(), (DragonAudioSource::Dac, 1.0));
        assert_sample_near(memory.audio_sample(), 0.0);

        memory.pia1.write(0x03, 0x3C); // PIA1 CB2 high: enable sound mux.
        assert_sample_near(memory.audio_sample(), ((4.50 + 0.20) / 4.70) * 0.7);
    }

    #[test]
    fn audio_mux_source_is_selected_by_pia0_ca2_and_cb2() {
        let rom = rom_with_reset_vector(0x8000);
        let mut memory = DragonMemory::new_with_keyboard(&rom, DragonKeyboard::new());
        memory.pia1.write(0x00, 0xFC); // PIA1 PA2-PA7 drive the DAC.
        memory.pia1.write(0x01, 0x04); // PIA1 port A data selected.
        memory.pia1.write(0x00, 0xFC);
        memory.pia1.write(0x03, 0x3C); // PIA1 CB2 high: enable sound mux.

        memory.pia0.write(0x01, 0x34); // PIA0 CA2 low.
        memory.pia0.write(0x03, 0x34); // PIA0 CB2 low.
        assert_eq!(memory.muxed_audio_source(), (DragonAudioSource::Dac, 1.0));
        assert_sample_near(memory.audio_sample(), ((4.50 + 0.20) / 4.70) * 0.7);

        memory.pia0.write(0x01, 0x3C); // PIA0 CA2 high.
        memory.pia0.write(0x03, 0x34); // PIA0 CB2 low.
        assert_eq!(memory.muxed_audio_source(), (DragonAudioSource::Tape, 0.0));
        assert_sample_near(memory.audio_sample(), (2.05 / 4.70) * 0.7);

        memory.cassette.load(vec![0x01]);
        memory.pia1.write(0x01, 0x3C); // PIA1 CA2 high: advance cassette to high half-cycle.
        memory.advance_cassette(CASSETTE_ONE_HALF_PERIOD_TICKS);
        assert_eq!(memory.muxed_audio_source(), (DragonAudioSource::Tape, 1.0));
        assert_sample_near(memory.audio_sample(), ((0.50 + 2.05) / 4.70) * 0.7);

        memory.pia0.write(0x01, 0x34); // PIA0 CA2 low.
        memory.pia0.write(0x03, 0x3C); // PIA0 CB2 high.
        assert_eq!(memory.muxed_audio_source(), (DragonAudioSource::Cart, 0.0));
        assert_sample_near(memory.audio_sample(), 0.0);
        memory.set_cartridge_sound_level(1.0);
        assert_eq!(memory.muxed_audio_source(), (DragonAudioSource::Cart, 1.0));
        assert_sample_near(memory.audio_sample(), 0.7);

        memory.pia0.write(0x01, 0x3C); // PIA0 CA2 high.
        memory.pia0.write(0x03, 0x3C); // PIA0 CB2 high.
        assert_eq!(
            memory.muxed_audio_source(),
            (DragonAudioSource::UnusedMuxInput, 0.0)
        );
        assert_sample_near(memory.audio_sample(), 0.0);

        memory.pia1.write(0x03, 0x38); // PIA1 CB2 high, port B DDR selected.
        memory.pia1.write(0x02, 0x02); // PIA1 PB1 drives single-bit sound.
        memory.pia1.write(0x03, 0x3C); // Keep PIA1 CB2 high: enable sound mux.
        memory.pia1.write(0x02, 0x02); // PB1 high must not leak into the unused mux input.
        assert_sample_near(memory.audio_sample(), 0.0);
    }

    #[test]
    fn cartridge_snd_pin_reaches_audio_output_when_mux_selects_cartridge() {
        let rom = rom_with_reset_vector(0x8000);
        let mut machine = Dragon32::new(&rom);
        machine.memory.pia0.write(0x01, 0x34); // PIA0 CA2 low.
        machine.memory.pia0.write(0x03, 0x3C); // PIA0 CB2 high: cartridge source.
        machine.memory.pia1.write(0x03, 0x3C); // PIA1 CB2 high: enable sound mux.

        machine.set_cartridge_sound_level(0.0);
        let _ = machine.run_cycles(128, 0);
        let mut low_samples = Vec::new();
        machine.drain_audio_samples(&mut low_samples);

        machine.set_cartridge_sound_level(1.0);
        let _ = machine.run_cycles(128, 0);
        let mut high_samples = Vec::new();
        machine.drain_audio_samples(&mut high_samples);

        assert!(!low_samples.is_empty());
        assert!(!high_samples.is_empty());
        assert!(low_samples.iter().all(|sample| *sample == 0.0));
        assert!(high_samples.iter().all(|sample| *sample > 0.69));
    }

    #[test]
    fn cartridge_snd_pin_is_normalized_before_mixing() {
        let rom = rom_with_reset_vector(0x8000);
        let mut memory = DragonMemory::new_with_keyboard(&rom, DragonKeyboard::new());
        memory.pia0.write(0x03, 0x3C); // PIA0 CB2 high: cartridge source with CA2 low.
        memory.pia1.write(0x03, 0x3C); // PIA1 CB2 high: enable sound mux.

        memory.set_cartridge_sound_level(-1.0);
        assert_eq!(memory.muxed_audio_source(), (DragonAudioSource::Cart, 0.0));
        assert_sample_near(memory.audio_sample(), 0.0);

        memory.set_cartridge_sound_level(0.5);
        assert_eq!(memory.muxed_audio_source(), (DragonAudioSource::Cart, 0.5));
        assert_sample_near(memory.audio_sample(), 0.35);

        memory.set_cartridge_sound_level(f32::NAN);
        assert_eq!(memory.muxed_audio_source(), (DragonAudioSource::Cart, 0.0));

        memory.set_cartridge_sound_level(2.0);
        assert_eq!(memory.muxed_audio_source(), (DragonAudioSource::Cart, 1.0));
        assert_sample_near(memory.audio_sample(), 0.7);
    }

    #[test]
    fn binary_program_loads_payload_and_exec_vector() {
        let rom = rom_with_reset_vector(0x8000);
        let mut machine = Dragon32::new(&rom);

        machine
            .load_binary_program(0x2800, &[0xcc, 0xfc, 0x39], 0x2801, false)
            .expect("program should fit in Dragon RAM");

        assert_eq!(&machine.ram()[0x2800..0x2803], &[0xcc, 0xfc, 0x39]);
        assert_eq!(machine.ram()[0x009d], 0x28);
        assert_eq!(machine.ram()[0x009e], 0x01);
        assert_eq!(machine.ram()[0x01e2], 0x02);
        assert_eq!(machine.ram()[0x01e5], 0x28);
        assert_eq!(machine.ram()[0x01e6], 0x01);
        assert_eq!(machine.ram()[0x01e7], 0x28);
        assert_eq!(machine.ram()[0x01e8], 0x00);
        assert_eq!(machine.pc(), 0x0000);
        assert_eq!(machine.stack_pointer(), 0x0000);
    }

    #[test]
    fn binary_program_autorun_sets_program_counter() {
        let rom = rom_with_reset_vector(0x8000);
        let mut machine = Dragon32::new(&rom);

        machine
            .load_binary_program(0x2800, &[0x39], 0x2800, true)
            .expect("program should fit in Dragon RAM");

        assert_eq!(machine.pc(), 0x2800);
        assert_eq!(machine.stack_pointer(), DIRECT_PROGRAM_STACK_POINTER);
    }

    #[test]
    fn binary_program_autorun_preserves_initialized_stack_pointer() {
        let rom = rom_with_reset_vector(0x8000);
        let mut machine = Dragon32::new(&rom);
        machine.cpu.regs.s = 0x7000;

        machine
            .load_binary_program(0x2800, &[0x39], 0x2800, true)
            .expect("program should fit in Dragon RAM");

        assert_eq!(machine.pc(), 0x2800);
        assert_eq!(machine.stack_pointer(), 0x7000);
    }

    #[test]
    fn binary_program_rejects_payload_outside_ram() {
        let rom = rom_with_reset_vector(0x8000);
        let mut machine = Dragon32::new(&rom);

        let err = machine
            .load_binary_program(0x7fff, &[0x39, 0x39], 0x7fff, false)
            .expect_err("program must not overflow Dragon 32 RAM");

        assert_eq!(
            err,
            DragonProgramLoadError::RamOverflow {
                load_address: 0x7fff,
                len: 2,
            }
        );
    }

    #[test]
    fn audio_tape_mux_levels_match_xroar_voltage_model() {
        let rom = rom_with_reset_vector(0x8000);
        let mut memory = DragonMemory::new_with_keyboard(&rom, DragonKeyboard::new());
        memory.pia0.write(0x01, 0x3C); // PIA0 CA2 high: select tape mux source.
        memory.pia1.write(0x03, 0x3C); // PIA1 CB2 high: enable sound mux.

        assert_sample_near(memory.audio_sample(), (2.05 / 4.70) * 0.7);

        memory.cassette.load(vec![0x01]);
        memory.pia1.write(0x01, 0x3C); // PIA1 CA2 high: advance cassette to high half-cycle.
        memory.advance_cassette(CASSETTE_ONE_HALF_PERIOD_TICKS);
        assert_sample_near(memory.audio_sample(), ((0.50 + 2.05) / 4.70) * 0.7);
    }

    #[test]
    fn audio_single_bit_levels_match_xroar_voltage_model() {
        let rom = rom_with_reset_vector(0x8000);
        let mut memory = DragonMemory::new_with_keyboard(&rom, DragonKeyboard::new());
        memory.pia1.write(0x02, 0x02); // PIA1 PB1 drives single-bit sound.
        memory.pia1.write(0x03, 0x04); // PIA1 port B data selected, sound mux disabled.

        memory.pia1.write(0x02, 0x00);
        assert_sample_near(memory.audio_sample(), 0.0);

        memory.pia1.write(0x02, 0x02);
        assert_sample_near(memory.audio_sample(), (3.90 / 4.70) * 0.7);
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
                master_tick: 32,
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
    fn machine_renders_pia_selected_graphics_mode() {
        let rom = rom_with_reset_vector(0x8000);
        let mut machine = Dragon32::new(&rom);
        machine.memory.sam.write(0xFFC9); // Display base $0400.
        machine.memory.sync_vdg_display_base_from_sam();
        machine.memory.ram[0x0400] = 0b01_10_11_00;
        machine.memory.pia1.write(0x02, 0xF8); // PIA1 port B DDR.
        machine.memory.pia1.write(0x03, 0x04); // PIA1 port B data selected.
        machine.memory.pia1.write(0x02, 0xE0); // CG6, CSS 0.

        let framebuffer = machine.render_visible_argb(VdgPalette::default());
        let active_origin =
            TEXT_TOP_BORDER_LINES * TEXT_VISIBLE_FRAMEBUFFER_WIDTH + TEXT_LEFT_BORDER_PIXELS;

        assert_eq!(framebuffer[active_origin + 2], DEFAULT_VDG_BLUE);
    }

    #[test]
    fn beam_renderer_uses_left_border_css_pipeline_for_first_byte() {
        let rom = rom_with_reset_vector(0x8000);
        let mut machine = Dragon32::new(&rom);
        machine.memory.pia1.write(0x02, 0xF8); // PIA1 port B DDR.
        machine.memory.pia1.write(0x03, 0x04); // PIA1 port B data selected.
        machine.memory.pia1.write(0x02, 0x08); // Alpha mode, CSS 1.

        machine
            .video
            .render_border_line(&machine.memory, TEXT_TOP_BORDER_LINES);
        machine.video.next_line = TEXT_TOP_BORDER_LINES;
        machine.video.start_active_byte(&machine.memory, Some(0));
        let mut samples = Vec::new();
        machine.video.drain_pending_samples(&mut samples);

        assert_eq!(samples.len(), 1);
        assert!(samples[0].css);
    }

    #[test]
    fn beam_renderer_delays_css_boundary_writes_until_prefetched_bytes_drain() {
        let rom = rom_with_reset_vector(0x8000);
        let mut machine = Dragon32::new(&rom);
        machine.memory.pia1.write(0x02, 0xF8); // PIA1 port B DDR.
        machine.memory.pia1.write(0x03, 0x04); // PIA1 port B data selected.
        machine.memory.pia1.write(0x02, 0xE0); // CG6, CSS 0.

        let active_line = TEXT_TOP_BORDER_LINES;
        let active_start = active_display_start_cycle(active_line);
        machine.video.tick(&machine.memory, active_start, Some(0));
        machine.memory.pia1.write(0x02, 0xE8); // CG6, CSS 1 at active x=0.
        machine.video.apply_vdg_control_change(&machine.memory);
        machine.video.tick(&machine.memory, 160, Some(1));

        let frame = machine.video.frame();
        let active_origin = active_line * TEXT_VISIBLE_FRAMEBUFFER_WIDTH + TEXT_LEFT_BORDER_PIXELS;
        let palette = VdgPalette::default();
        assert_eq!(
            &frame[active_origin..active_origin + 8],
            [palette.colours[0]; 8]
        );
        assert_eq!(
            &frame[active_origin + 8..active_origin + 16],
            [palette.colours[0]; 8]
        );
        assert_eq!(
            &frame[active_origin + 16..active_origin + 24],
            [palette.colours[4]; 8]
        );
    }

    #[test]
    fn beam_renderer_latches_vram_before_pixels_are_displayed() {
        let rom = rom_with_reset_vector(0x8000);
        let mut machine = Dragon32::new(&rom);
        machine.memory.pia1.write(0x02, 0xF8); // PIA1 port B DDR.
        machine.memory.pia1.write(0x03, 0x04); // PIA1 port B data selected.
        machine.memory.pia1.write(0x02, 0xE0); // CG6, CSS 0.

        let active_line = TEXT_TOP_BORDER_LINES;
        let active_start = active_display_start_cycle(active_line);
        let first_fetch = active_start - vdg_fetch_to_display_ticks(graphics_control(6));
        let base = usize::from(machine.memory.text_screen_base());
        machine.memory.ram[base] = 0x00;

        machine
            .video
            .tick(&machine.memory, first_fetch + 1, Some(0));
        machine.memory.ram[base] = 0xFF;
        machine
            .video
            .tick(&machine.memory, active_start + 16, Some(1));

        let frame = machine.video.frame();
        let active_origin = active_line * TEXT_VISIBLE_FRAMEBUFFER_WIDTH + TEXT_LEFT_BORDER_PIXELS;
        let palette = VdgPalette::default();
        assert_eq!(
            &frame[active_origin..active_origin + 8],
            [palette.colours[0]; 8]
        );
    }

    #[test]
    fn beam_renderer_observes_vram_writes_before_latch_point() {
        let rom = rom_with_reset_vector(0x8000);
        let mut machine = Dragon32::new(&rom);
        machine.memory.pia1.write(0x02, 0xF8); // PIA1 port B DDR.
        machine.memory.pia1.write(0x03, 0x04); // PIA1 port B data selected.
        machine.memory.pia1.write(0x02, 0xE0); // CG6, CSS 0.

        let active_line = TEXT_TOP_BORDER_LINES;
        let active_start = active_display_start_cycle(active_line);
        let first_fetch = active_start - vdg_fetch_to_display_ticks(graphics_control(6));
        let base = usize::from(machine.memory.text_screen_base());
        machine.memory.ram[base] = 0x00;

        machine.video.tick(&machine.memory, first_fetch, Some(0));
        machine.memory.ram[base] = 0xFF;
        machine
            .video
            .tick(&machine.memory, active_start + 16, Some(1));

        let frame = machine.video.frame();
        let active_origin = active_line * TEXT_VISIBLE_FRAMEBUFFER_WIDTH + TEXT_LEFT_BORDER_PIXELS;
        let palette = VdgPalette::default();
        assert_eq!(
            &frame[active_origin..active_origin + 8],
            [palette.colours[3]; 8]
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
