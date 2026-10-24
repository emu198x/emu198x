//! The Kempston mouse interface.
//!
//! The interface counts the mouse's quadrature pulses into two free-running
//! 8-bit counters and exposes them, with the buttons, on three partially
//! decoded ports. Software reads a counter twice and takes the difference, so
//! the counters simply wrap.
//!
//! | Port | Decode (mask / value) | Reads |
//! |---|---|---|
//! | `$FADF` | `$0121` / `$0001` | buttons, active low: bit 0 right, bit 1 left, bit 2 middle |
//! | `$FBDF` | `$0521` / `$0101` | X counter, rising to the right |
//! | `$FFDF` | `$0521` / `$0501` | Y counter, rising as the mouse moves *up* |
//!
//! The decoding follows Fuse's `kempmouse.c` (GPL-2, used for facts only),
//! with one addition: A7 must be high. All three ports have A5 low and A0
//! high, inside a Kempston joystick's own partial decode, and Fuse's masks
//! alone would also select the buttons at `$001F`, the joystick's canonical
//! port, where the two would fight over the bus. Every mouse port has A7
//! high and `$1F` has it low, so requiring it keeps the joystick's port the
//! joystick's. A machine hosting both still asks the mouse first.

use common_sinclair_zx_spectrum::peripheral::Peripheral;

/// A Kempston mouse button.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MouseButton {
    /// The right button, bit 0.
    Right,
    /// The left button, bit 1.
    Left,
    /// The middle button, bit 2, on interfaces wired for three.
    Middle,
}

impl MouseButton {
    /// The button's bit on the buttons port.
    #[must_use]
    pub const fn mask(self) -> u8 {
        match self {
            Self::Right => 0b001,
            Self::Left => 0b010,
            Self::Middle => 0b100,
        }
    }

    /// Resolves a host button name: `left`, `right` or `middle`.
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "left" => Some(Self::Left),
            "right" => Some(Self::Right),
            "middle" => Some(Self::Middle),
            _ => None,
        }
    }
}

/// The interface's state.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct KempstonMouse {
    /// Whether the interface is plugged in. An absent interface claims no
    /// ports, so they read the floating bus.
    pub attached: bool,
    /// The horizontal counter.
    pub x: u8,
    /// The vertical counter.
    pub y: u8,
    /// Pressed buttons, as [`MouseButton::mask`] bits. They are active high
    /// here; the port inverts them.
    pub buttons: u8,
}

/// Which of the three registers a port selects.
enum Register {
    Buttons,
    X,
    Y,
}

impl KempstonMouse {
    /// An unplugged interface.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            attached: false,
            x: 0,
            y: 0,
            buttons: 0,
        }
    }

    /// Moves the mouse by a host delta: `dx` to the right and `dy` *down*, as
    /// hosts report it. The Y counter rises as the mouse moves up, so `dy` is
    /// subtracted. The counters are 8 bits wide and wrap.
    pub fn move_by(&mut self, dx: i32, dy: i32) {
        self.x = self.x.wrapping_add(dx as u8);
        self.y = self.y.wrapping_sub(dy as u8);
    }

    /// Presses or releases a button.
    pub fn set_button(&mut self, button: MouseButton, pressed: bool) {
        if pressed {
            self.buttons |= button.mask();
        } else {
            self.buttons &= !button.mask();
        }
    }

    fn register(port: u16) -> Option<Register> {
        if port & 0x0080 == 0 {
            return None;
        }
        if port & 0x0121 == 0x0001 {
            Some(Register::Buttons)
        } else if port & 0x0521 == 0x0101 {
            Some(Register::X)
        } else if port & 0x0521 == 0x0501 {
            Some(Register::Y)
        } else {
            None
        }
    }
}

impl Peripheral for KempstonMouse {
    fn claims_port(&self, port: u16) -> bool {
        self.attached && Self::register(port).is_some()
    }

    fn read(&mut self, port: u16) -> u8 {
        match Self::register(port) {
            Some(Register::Buttons) => !self.buttons,
            Some(Register::X) => self.x,
            Some(Register::Y) => self.y,
            None => 0xFF,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn attached() -> KempstonMouse {
        KempstonMouse {
            attached: true,
            ..KempstonMouse::new()
        }
    }

    #[test]
    fn an_unplugged_interface_claims_nothing() {
        let m = KempstonMouse::new();
        for port in [0xFADF, 0xFBDF, 0xFFDF] {
            assert!(!m.claims_port(port));
        }
    }

    #[test]
    fn the_canonical_ports_decode_to_their_registers() {
        let mut m = attached();
        m.x = 0x12;
        m.y = 0x34;
        m.set_button(MouseButton::Left, true);
        assert_eq!(m.read(0xFADF), 0b1111_1101);
        assert_eq!(m.read(0xFBDF), 0x12);
        assert_eq!(m.read(0xFFDF), 0x34);
    }

    #[test]
    fn decoding_is_partial() {
        let m = attached();
        // A0 and A7 high with A5 and A8 low is enough for the buttons.
        assert!(m.claims_port(0x0081));
        // The joystick's canonical port is never the mouse.
        assert!(!m.claims_port(0x001F));
        // A5 high is never the mouse.
        assert!(!m.claims_port(0xFAFF));
        // A0 low is the ULA.
        assert!(!m.claims_port(0xFADE));
    }

    #[test]
    fn an_unknown_port_reads_high() {
        let mut m = attached();
        assert_eq!(m.read(0xFAFF), 0xFF);
    }

    #[test]
    fn motion_wraps_and_y_rises_upwards() {
        let mut m = attached();
        m.move_by(3, -2);
        assert_eq!((m.x, m.y), (3, 2));
        m.move_by(-5, 4);
        assert_eq!((m.x, m.y), (254, 254));
    }

    #[test]
    fn buttons_press_and_release() {
        let mut m = attached();
        m.set_button(MouseButton::Right, true);
        m.set_button(MouseButton::Middle, true);
        assert_eq!(m.read(0xFADF), 0b1111_1010);
        m.set_button(MouseButton::Right, false);
        assert_eq!(m.read(0xFADF), 0b1111_1011);
        assert_eq!(MouseButton::from_name("middle"), Some(MouseButton::Middle));
        assert_eq!(MouseButton::from_name("fire"), None);
    }
}
