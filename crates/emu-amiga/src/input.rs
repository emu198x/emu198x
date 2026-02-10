//! Input handling for the Amiga.
//!
//! Stub for Phase 1 — no keyboard or joystick input.
//! Provides the timed input queue for scripted sequences (MCP).

use std::collections::VecDeque;

/// Input event for scripted sequences.
#[derive(Debug, Clone)]
pub struct InputEvent {
    /// Frame number at which this event fires.
    pub frame: u64,
    /// Key code or action.
    pub action: InputAction,
}

/// Input action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputAction {
    /// No-op placeholder.
    None,
}

/// Timed input queue.
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

    /// Process all events for the given frame.
    pub fn process(&mut self, frame: u64) {
        while let Some(event) = self.events.front() {
            if event.frame > frame {
                break;
            }
            let _event = self.events.pop_front();
            // Phase 1: no actual input handling
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
