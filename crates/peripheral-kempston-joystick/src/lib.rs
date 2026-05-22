//! Kempston Interface joystick — Spectrum edge-connector peripheral.
//!
//! The Kempston Interface (1983, Kempston Micro Electronics) was the
//! most common joystick add-on for the rubber-key 48K and the toastrack
//! 128K. It plugged into the rear edge connector and exposed a single
//! 8-bit read register on any port with `A5=0` and `A0=1` — software
//! canonically reads from `$1F`. Bits 0-4 carry the joystick state:
//!
//! | bit | direction |
//! |---|---|
//! | 0 | right |
//! | 1 | left |
//! | 2 | down |
//! | 3 | up |
//! | 4 | fire |
//!
//! Bits 5-7 are open-drain (read as 0 from the interface, so games
//! AND-mask the byte with `$1F` before testing bits).
//!
//! ## Why a peripheral, not a `u8` field on every machine
//!
//! The original `Peripheral` trait carveout said Kempston was "too
//! simple" to abstract — a single byte and a one-line port read. That
//! reasoning held until we noticed three real costs:
//!
//! 1. **Optionality.** A `pub kempston: u8` field is *always there*.
//!    Every machine emulates a permanently-attached interface even if
//!    no real user ever plugged one in. Software that probes for an
//!    interface (some loaders read `$1F` to detect Kempston) saw zero
//!    instead of the floating bus. With the peripheral, an unattached
//!    interface returns `claims_port = false` and the bus dispatch
//!    falls through to the ULA's floating bus.
//!
//! 2. **Wrong-machine modelling.** The Amstrad +2A / +2B / +3 broke
//!    the rear connector pinout in 1987; classic Kempston interfaces
//!    don't physically fit. Yet the pre-D6 `-plus` crate (and its
//!    Amstrad-class extraction) carried a `pub kempston: u8` field
//!    and decoded the port range. The peripheral pattern lets each
//!    machine declare *whether it can host a Kempston at all*, by
//!    simply not owning one.
//!
//! 3. **Symmetry.** The Sinclair Interface 2 add-on (the 1983
//!    cartridge-shaped one, distinct from the +2's built-in IF2-style
//!    ports) has both a ROM cartridge slot and joystick ports. Future
//!    work will model it as a `Peripheral`. Treating Kempston as a
//!    peripheral keeps the "what add-ons can plug into this machine"
//!    question answerable through one shape.
//!
//! See `knowledge/decisions/spectrum-joystick-architecture.md` for the
//! reasoning trail.

use common_sinclair_zx_spectrum::peripheral::Peripheral;

/// Kempston joystick button bits, as they appear on the read byte at `$1F`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum KempstonButton {
    /// Right (bit 0).
    Right,
    /// Left (bit 1).
    Left,
    /// Down (bit 2).
    Down,
    /// Up (bit 3).
    Up,
    /// Fire (bit 4).
    Fire,
}

impl KempstonButton {
    /// Returns the bit mask for this button on the read byte.
    #[must_use]
    pub const fn mask(self) -> u8 {
        match self {
            Self::Right => 0b0000_0001,
            Self::Left => 0b0000_0010,
            Self::Down => 0b0000_0100,
            Self::Up => 0b0000_1000,
            Self::Fire => 0b0001_0000,
        }
    }

    /// Resolves a stable button index to a [`KempstonButton`]. The
    /// index matches the bit position on the read byte:
    /// `0` right, `1` left, `2` down, `3` up, `4` fire. Returns
    /// `None` for any other value. The runtime input layer uses this
    /// to translate host joystick events without taking a
    /// `KempstonButton` dependency through the [`SpectrumMachine`]
    /// trait boundary.
    #[must_use]
    pub const fn from_index(index: u8) -> Option<Self> {
        match index {
            0 => Some(Self::Right),
            1 => Some(Self::Left),
            2 => Some(Self::Down),
            3 => Some(Self::Up),
            4 => Some(Self::Fire),
            _ => None,
        }
    }
}

/// A Kempston Interface joystick attached to a Spectrum.
///
/// `attached` represents whether the user has the physical interface
/// plugged into the edge connector. When `false`, the peripheral does
/// not claim any port and reads at `$1F` (and friends) fall through to
/// the machine's idle bus value — matching real hardware where a
/// disconnected port reads floating bus.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct KempstonJoystick {
    /// Is the interface plugged in?
    pub attached: bool,
    /// Current joystick state byte. Bits 0-4 = right/left/down/up/fire.
    pub state: u8,
}

impl KempstonJoystick {
    /// Creates an unattached interface. Use [`Self::attached`] to start
    /// with the interface plugged in instead.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            attached: false,
            state: 0,
        }
    }

    /// Creates an attached, idle interface.
    #[must_use]
    pub const fn attached() -> Self {
        Self {
            attached: true,
            state: 0,
        }
    }

    /// Sets a single button's pressed state.
    pub fn set_button(&mut self, button: KempstonButton, pressed: bool) {
        let mask = button.mask();
        if pressed {
            self.state |= mask;
        } else {
            self.state &= !mask;
        }
    }

    /// Returns whether a single button is currently pressed.
    #[must_use]
    pub fn is_pressed(&self, button: KempstonButton) -> bool {
        self.state & button.mask() != 0
    }
}

impl Peripheral for KempstonJoystick {
    fn claims_port(&self, port: u16) -> bool {
        // Standard Kempston decoding: A5=0 AND A0=1. Canonically `$1F`
        // (and any port mirror that satisfies the mask). Honours
        // `attached` so a disconnected interface presents floating bus
        // through the machine's normal fallthrough.
        self.attached && (port & 0x00E0) == 0x0000 && (port & 0x0001) != 0
    }

    fn read(&mut self, _port: u16) -> u8 {
        self.state
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_unattached_and_idle() {
        let k = KempstonJoystick::new();
        assert!(!k.attached);
        assert_eq!(k.state, 0);
    }

    #[test]
    fn attached_constructor_is_attached_and_idle() {
        let k = KempstonJoystick::attached();
        assert!(k.attached);
        assert_eq!(k.state, 0);
    }

    #[test]
    fn unattached_peripheral_does_not_claim_canonical_port() {
        let k = KempstonJoystick::new();
        assert!(!k.claims_port(0x1F));
    }

    #[test]
    fn attached_peripheral_claims_canonical_port() {
        let k = KempstonJoystick::attached();
        assert!(k.claims_port(0x1F));
    }

    #[test]
    fn claims_port_honours_address_decoding() {
        let k = KempstonJoystick::attached();
        // A5=0, A0=1 — claimed.
        assert!(k.claims_port(0x001F));
        assert!(k.claims_port(0xFF1F));
        assert!(k.claims_port(0x0001));
        // A5=0, A0=0 — not claimed (even-port range belongs to ULA).
        assert!(!k.claims_port(0x001E));
        assert!(!k.claims_port(0x0000));
        // A5=1 — not claimed.
        assert!(!k.claims_port(0x003F));
        assert!(!k.claims_port(0xFFFF));
    }

    #[test]
    fn set_button_updates_state_bits() {
        let mut k = KempstonJoystick::attached();
        k.set_button(KempstonButton::Fire, true);
        assert_eq!(k.state, 0b0001_0000);
        k.set_button(KempstonButton::Right, true);
        assert_eq!(k.state, 0b0001_0001);
        k.set_button(KempstonButton::Fire, false);
        assert_eq!(k.state, 0b0000_0001);
    }

    #[test]
    fn is_pressed_reflects_state() {
        let mut k = KempstonJoystick::attached();
        assert!(!k.is_pressed(KempstonButton::Up));
        k.set_button(KempstonButton::Up, true);
        assert!(k.is_pressed(KempstonButton::Up));
        assert!(!k.is_pressed(KempstonButton::Down));
    }

    #[test]
    fn read_returns_state_byte() {
        let mut k = KempstonJoystick::attached();
        k.state = 0b0001_1010;
        assert_eq!(k.read(0x1F), 0b0001_1010);
    }

    #[test]
    fn write_is_a_noop() {
        let mut k = KempstonJoystick::attached();
        k.write(0x1F, 0xFF); // should not panic
        assert_eq!(k.state, 0);
    }

    #[test]
    fn button_masks_match_real_hardware_layout() {
        // Locked-in mapping. If a refactor scrambles the masks, this
        // test catches it before any game-side regression.
        assert_eq!(KempstonButton::Right.mask(), 0b0000_0001);
        assert_eq!(KempstonButton::Left.mask(), 0b0000_0010);
        assert_eq!(KempstonButton::Down.mask(), 0b0000_0100);
        assert_eq!(KempstonButton::Up.mask(), 0b0000_1000);
        assert_eq!(KempstonButton::Fire.mask(), 0b0001_0000);
    }

    #[test]
    fn from_index_round_trips_bit_layout() {
        assert_eq!(KempstonButton::from_index(0), Some(KempstonButton::Right));
        assert_eq!(KempstonButton::from_index(1), Some(KempstonButton::Left));
        assert_eq!(KempstonButton::from_index(2), Some(KempstonButton::Down));
        assert_eq!(KempstonButton::from_index(3), Some(KempstonButton::Up));
        assert_eq!(KempstonButton::from_index(4), Some(KempstonButton::Fire));
    }

    #[test]
    fn from_index_rejects_unmapped_values() {
        assert_eq!(KempstonButton::from_index(5), None);
        assert_eq!(KempstonButton::from_index(0xFF), None);
    }
}
