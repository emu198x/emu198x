//! NES controller input.

/// NES controller button flags.
pub mod buttons {
    pub const A: u8 = 0x01;
    pub const B: u8 = 0x02;
    pub const SELECT: u8 = 0x04;
    pub const START: u8 = 0x08;
    pub const UP: u8 = 0x10;
    pub const DOWN: u8 = 0x20;
    pub const LEFT: u8 = 0x40;
    pub const RIGHT: u8 = 0x80;
}

/// NES controller state.
#[derive(Clone, Copy, Debug, Default)]
pub struct Controller {
    /// Current button state (bitfield).
    pub state: u8,
}

impl Controller {
    /// Create a new controller.
    pub fn new() -> Self {
        Self { state: 0 }
    }

    /// Press a button.
    pub fn press(&mut self, button: u8) {
        self.state |= button;
    }

    /// Release a button.
    pub fn release(&mut self, button: u8) {
        self.state &= !button;
    }

    /// Check if a button is pressed.
    pub fn is_pressed(&self, button: u8) -> bool {
        self.state & button != 0
    }

    /// Set button state directly.
    pub fn set_state(&mut self, state: u8) {
        self.state = state;
    }

    /// Get current state.
    pub fn get_state(&self) -> u8 {
        self.state
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_controller() {
        let mut ctrl = Controller::new();
        assert_eq!(ctrl.get_state(), 0);

        ctrl.press(buttons::A);
        assert!(ctrl.is_pressed(buttons::A));
        assert!(!ctrl.is_pressed(buttons::B));

        ctrl.press(buttons::B);
        assert!(ctrl.is_pressed(buttons::A | buttons::B));

        ctrl.release(buttons::A);
        assert!(!ctrl.is_pressed(buttons::A));
        assert!(ctrl.is_pressed(buttons::B));
    }
}
