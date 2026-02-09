//! Input handling for the NES.
//!
//! Two layers:
//! 1. `NesButton` — logical button names mapped to controller bit positions.
//! 2. `InputQueue` — timed button events for scripted sequences.

use std::collections::VecDeque;

use crate::controller::{self, Controller};

/// Logical button on the NES controller.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NesButton {
    A,
    B,
    Select,
    Start,
    Up,
    Down,
    Left,
    Right,
}

impl NesButton {
    /// Return the bit position for this button.
    #[must_use]
    pub const fn bit(self) -> u8 {
        match self {
            Self::A => controller::button::A,
            Self::B => controller::button::B,
            Self::Select => controller::button::SELECT,
            Self::Start => controller::button::START,
            Self::Up => controller::button::UP,
            Self::Down => controller::button::DOWN,
            Self::Left => controller::button::LEFT,
            Self::Right => controller::button::RIGHT,
        }
    }
}

/// A timed button event.
#[derive(Debug, Clone)]
pub struct InputEvent {
    /// Frame number at which this event fires.
    pub frame: u64,
    /// Which button.
    pub button: NesButton,
    /// True = press, false = release.
    pub pressed: bool,
}

/// Timed input queue for scripted button sequences.
///
/// Events are sorted by frame number and processed at the start of each frame.
pub struct InputQueue {
    events: VecDeque<InputEvent>,
}

impl InputQueue {
    #[must_use]
    pub fn new() -> Self {
        Self {
            events: VecDeque::new(),
        }
    }

    /// Enqueue a raw input event.
    pub fn push(&mut self, event: InputEvent) {
        let pos = self
            .events
            .iter()
            .position(|e| e.frame > event.frame)
            .unwrap_or(self.events.len());
        self.events.insert(pos, event);
    }

    /// Enqueue a button press and release.
    pub fn enqueue_button(&mut self, button: NesButton, at_frame: u64, hold_frames: u64) {
        self.push(InputEvent {
            frame: at_frame,
            button,
            pressed: true,
        });
        self.push(InputEvent {
            frame: at_frame + hold_frames,
            button,
            pressed: false,
        });
    }

    /// Process all events for the given frame, applying them to controller 1.
    pub fn process(&mut self, frame: u64, controller: &mut Controller) {
        while let Some(event) = self.events.front() {
            if event.frame > frame {
                break;
            }
            let event = self.events.pop_front().expect("front was Some");
            controller.set_button(event.button.bit(), event.pressed);
        }
    }

    /// Number of pending events.
    #[must_use]
    pub fn len(&self) -> usize {
        self.events.len()
    }

    /// Whether the queue is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }
}

impl Default for InputQueue {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enqueue_button_creates_press_and_release() {
        let mut queue = InputQueue::new();
        queue.enqueue_button(NesButton::A, 10, 3);
        assert_eq!(queue.len(), 2);
    }

    #[test]
    fn process_applies_events() {
        let mut queue = InputQueue::new();
        let mut ctrl = Controller::new();

        queue.enqueue_button(NesButton::A, 5, 3);

        // Frame 4: nothing
        queue.process(4, &mut ctrl);
        assert_eq!(ctrl.buttons() & 0x01, 0x00); // A not pressed

        // Frame 5: press
        queue.process(5, &mut ctrl);
        assert_eq!(ctrl.buttons() & 0x01, 0x01); // A pressed

        // Frame 8: release
        queue.process(8, &mut ctrl);
        assert_eq!(ctrl.buttons() & 0x01, 0x00); // A released
    }
}
