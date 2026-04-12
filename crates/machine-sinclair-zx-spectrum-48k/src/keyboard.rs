//! ZX Spectrum keyboard matrix.
//!
//! Source references:
//! - `docs/platforms/sinclair-zx-spectrum/language/ZX-SPECTRUM-MEMORY-AND-GRAPHICS-REFERENCE.md`
//! - Adapted from `/Users/stevehill/Projects/Emu198x-Older/crates/runtime-sinclair-zx-spectrum/src/system_impl.rs`
//!
//! The ported part here is the stable physical matrix encoding and key-name
//! mapping, not the older runtime or machine loop.

use emu198x_shell::InputEvent;

const RELEASED_ROW: u8 = 0xff;
const ROW_MASK: u8 = 0x1f;

/// One physical key on the 40-key Spectrum matrix.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SpectrumKey {
    CapsShift,
    Z,
    X,
    C,
    V,
    A,
    S,
    D,
    F,
    G,
    Q,
    W,
    E,
    R,
    T,
    Num1,
    Num2,
    Num3,
    Num4,
    Num5,
    Num0,
    Num9,
    Num8,
    Num7,
    Num6,
    P,
    O,
    I,
    U,
    Y,
    Enter,
    L,
    K,
    J,
    H,
    Space,
    SymbolShift,
    M,
    N,
    B,
}

impl SpectrumKey {
    /// Resolves a stable key name or documented alias.
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        let upper = name.to_ascii_uppercase();
        Some(match upper.as_str() {
            "CAPSSHIFT" | "CAPS" | "LSHIFT" | "RSHIFT" => Self::CapsShift,
            "Z" => Self::Z,
            "X" => Self::X,
            "C" => Self::C,
            "V" => Self::V,
            "A" => Self::A,
            "S" => Self::S,
            "D" => Self::D,
            "F" => Self::F,
            "G" => Self::G,
            "Q" => Self::Q,
            "W" => Self::W,
            "E" => Self::E,
            "R" => Self::R,
            "T" => Self::T,
            "1" => Self::Num1,
            "2" => Self::Num2,
            "3" => Self::Num3,
            "4" => Self::Num4,
            "5" => Self::Num5,
            "0" => Self::Num0,
            "9" => Self::Num9,
            "8" => Self::Num8,
            "7" => Self::Num7,
            "6" => Self::Num6,
            "P" => Self::P,
            "O" => Self::O,
            "I" => Self::I,
            "U" => Self::U,
            "Y" => Self::Y,
            "ENTER" | "RETURN" => Self::Enter,
            "L" => Self::L,
            "K" => Self::K,
            "J" => Self::J,
            "H" => Self::H,
            "SPACE" => Self::Space,
            "SYMBOLSHIFT" | "SYMBOL" | "SS" => Self::SymbolShift,
            "M" => Self::M,
            "N" => Self::N,
            "B" => Self::B,
            _ => return None,
        })
    }

    /// Returns the half-row index and bit position for this key.
    #[must_use]
    pub const fn row_bit(self) -> (usize, u8) {
        match self {
            Self::CapsShift => (0, 0),
            Self::Z => (0, 1),
            Self::X => (0, 2),
            Self::C => (0, 3),
            Self::V => (0, 4),
            Self::A => (1, 0),
            Self::S => (1, 1),
            Self::D => (1, 2),
            Self::F => (1, 3),
            Self::G => (1, 4),
            Self::Q => (2, 0),
            Self::W => (2, 1),
            Self::E => (2, 2),
            Self::R => (2, 3),
            Self::T => (2, 4),
            Self::Num1 => (3, 0),
            Self::Num2 => (3, 1),
            Self::Num3 => (3, 2),
            Self::Num4 => (3, 3),
            Self::Num5 => (3, 4),
            Self::Num0 => (4, 0),
            Self::Num9 => (4, 1),
            Self::Num8 => (4, 2),
            Self::Num7 => (4, 3),
            Self::Num6 => (4, 4),
            Self::P => (5, 0),
            Self::O => (5, 1),
            Self::I => (5, 2),
            Self::U => (5, 3),
            Self::Y => (5, 4),
            Self::Enter => (6, 0),
            Self::L => (6, 1),
            Self::K => (6, 2),
            Self::J => (6, 3),
            Self::H => (6, 4),
            Self::Space => (7, 0),
            Self::SymbolShift => (7, 1),
            Self::M => (7, 2),
            Self::N => (7, 3),
            Self::B => (7, 4),
        }
    }
}

/// Eight half-rows of active-low key state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeyboardMatrix {
    rows: [u8; 8],
}

impl KeyboardMatrix {
    /// Creates a matrix with all keys released.
    #[must_use]
    pub fn new() -> Self {
        Self {
            rows: [RELEASED_ROW; 8],
        }
    }

    /// Returns the raw half-row bytes.
    #[must_use]
    pub fn rows(&self) -> &[u8; 8] {
        &self.rows
    }

    /// Returns mutable access to the raw half-row bytes.
    #[must_use]
    pub fn rows_mut(&mut self) -> &mut [u8; 8] {
        &mut self.rows
    }

    /// Releases all keys.
    pub fn clear(&mut self) {
        self.rows = [RELEASED_ROW; 8];
    }

    /// Sets one key's pressed state.
    pub fn set_key(&mut self, key: SpectrumKey, pressed: bool) {
        let (row, bit) = key.row_bit();
        let mask = 1u8 << bit;
        if pressed {
            self.rows[row] &= !mask;
        } else {
            self.rows[row] |= mask;
        }
    }

    /// Presses one key.
    pub fn press_key(&mut self, key: SpectrumKey) {
        self.set_key(key, true);
    }

    /// Releases one key.
    pub fn release_key(&mut self, key: SpectrumKey) {
        self.set_key(key, false);
    }

    /// Resolves the keyboard bits selected by the port high byte.
    #[must_use]
    pub fn scan_port(&self, port: u16) -> u8 {
        let high = (port >> 8) as u8;
        let mut keys = ROW_MASK;

        for row in 0..8 {
            if high & (1 << row) == 0 {
                keys &= self.rows[row] & ROW_MASK;
            }
        }

        keys
    }

    /// Applies one host input event to the matrix.
    ///
    /// Returns `true` when the event maps to a physical Spectrum key.
    pub fn apply_input_event(&mut self, event: &InputEvent) -> bool {
        match event {
            InputEvent::Key { name, pressed } => {
                let Some(key) = SpectrumKey::from_name(name.as_ref()) else {
                    return false;
                };
                self.set_key(key, *pressed);
                true
            }
            _ => false,
        }
    }
}

impl Default for KeyboardMatrix {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_name_aliases_resolve_case_insensitively() {
        assert_eq!(SpectrumKey::from_name("caps"), Some(SpectrumKey::CapsShift));
        assert_eq!(SpectrumKey::from_name("Return"), Some(SpectrumKey::Enter));
        assert_eq!(
            SpectrumKey::from_name("symbol"),
            Some(SpectrumKey::SymbolShift)
        );
        assert_eq!(SpectrumKey::from_name("?"), None);
    }

    #[test]
    fn pressing_and_releasing_keys_is_active_low() {
        let mut keyboard = KeyboardMatrix::new();

        keyboard.press_key(SpectrumKey::Q);
        assert_eq!(keyboard.rows()[2] & 0x01, 0x00);

        keyboard.release_key(SpectrumKey::Q);
        assert_eq!(keyboard.rows()[2] & 0x01, 0x01);
    }

    #[test]
    fn scan_port_ands_selected_half_rows() {
        let mut keyboard = KeyboardMatrix::new();
        keyboard.press_key(SpectrumKey::Z);
        keyboard.press_key(SpectrumKey::Q);

        assert_eq!(keyboard.scan_port(0xfefe), 0x1d);
        assert_eq!(keyboard.scan_port(0xfbfe), 0x1e);
        assert_eq!(keyboard.scan_port(0xfafe), 0x1c);
    }

    #[test]
    fn host_key_events_update_matrix() {
        let mut keyboard = KeyboardMatrix::new();

        let pressed = InputEvent::Key {
            name: "space".into(),
            pressed: true,
        };
        let released = InputEvent::Key {
            name: "space".into(),
            pressed: false,
        };

        assert!(keyboard.apply_input_event(&pressed));
        assert_eq!(keyboard.rows()[7] & 0x01, 0x00);

        assert!(keyboard.apply_input_event(&released));
        assert_eq!(keyboard.rows()[7] & 0x01, 0x01);
    }

    #[test]
    fn non_key_events_are_not_handled() {
        let mut keyboard = KeyboardMatrix::new();
        let event = InputEvent::Button {
            port: 0,
            name: "fire".into(),
            pressed: true,
        };

        assert!(!keyboard.apply_input_event(&event));
        assert_eq!(keyboard.rows(), &[0xff; 8]);
    }
}
