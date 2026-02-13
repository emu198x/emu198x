//! Input handling for the Amiga.
//!
//! Provides a timed input queue for scripted keyboard sequences.

use std::collections::VecDeque;

use crate::keyboard_map::{self, CharKey, KEY_F1, KEY_RETURN, SHIFT_CODE};

/// A timed keyboard event (raw keycode).
#[derive(Debug, Clone)]
pub struct InputEvent {
    /// Frame number at which this event fires.
    pub frame: u64,
    /// Raw keycode (0-127).
    pub code: u8,
    /// True = key-down, false = key-up.
    pub pressed: bool,
}

/// Timed input queue for scripted key sequences.
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

    /// Enqueue a key press and release.
    pub fn enqueue_key(&mut self, code: u8, at_frame: u64, hold_frames: u64) {
        self.push(InputEvent {
            frame: at_frame,
            code,
            pressed: true,
        });
        self.push(InputEvent {
            frame: at_frame + hold_frames,
            code,
            pressed: false,
        });
    }

    /// Enqueue typing a string.
    ///
    /// Each character is held for `hold_frames` with a `gap_frames` pause.
    /// Returns the next free frame after all characters are queued.
    pub fn enqueue_text(
        &mut self,
        text: &str,
        start_frame: u64,
        hold_frames: u64,
        gap_frames: u64,
    ) -> u64 {
        let mut frame = start_frame;

        for ch in text.chars() {
            let Some(CharKey { code, shift }) = keyboard_map::char_to_key(ch) else {
                continue;
            };
            if shift {
                self.enqueue_key(SHIFT_CODE, frame, hold_frames);
            }
            self.enqueue_key(code, frame, hold_frames);
            frame = frame.saturating_add(hold_frames).saturating_add(gap_frames);
        }

        frame
    }

    /// Enqueue a small auto-boot script for Kickstart menus.
    pub fn enqueue_auto_boot(
        &mut self,
        start_frame: u64,
        hold_frames: u64,
        gap_frames: u64,
        script: AutoBootScript,
    ) -> u64 {
        let mut frame = start_frame;
        match script {
            AutoBootScript::BootMenu => {
                self.enqueue_key(KEY_F1, frame, hold_frames);
                frame = frame.saturating_add(hold_frames).saturating_add(gap_frames);
                self.enqueue_key(KEY_RETURN, frame, hold_frames);
                frame = frame.saturating_add(hold_frames).saturating_add(gap_frames);
            }
            AutoBootScript::Enter => {
                self.enqueue_key(KEY_RETURN, frame, hold_frames);
                frame = frame.saturating_add(hold_frames).saturating_add(gap_frames);
            }
            AutoBootScript::F1 => {
                self.enqueue_key(KEY_F1, frame, hold_frames);
                frame = frame.saturating_add(hold_frames).saturating_add(gap_frames);
            }
        }
        frame
    }

    /// Process all events for the given frame.
    pub fn process<F: FnMut(u8, bool)>(&mut self, frame: u64, mut emit: F) {
        while let Some(event) = self.events.front() {
            if event.frame > frame {
                break;
            }
            let event = self.events.pop_front().expect("front was Some");
            emit(event.code, event.pressed);
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

/// Built-in auto-boot scripts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AutoBootScript {
    /// Press F1 then Return (boot menu).
    BootMenu,
    /// Press Return only.
    Enter,
    /// Press F1 only.
    F1,
}
