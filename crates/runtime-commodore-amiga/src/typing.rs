//! Amiga host-side keyboard typing helpers.
//!
//! Drive the real keyboard through the shared headless-session boundary:
//! a press queues key-down events, runs a hold window so the running
//! software's keyboard scan sees them, then queues the matching key-up.
//! [`type_string`] walks a string character by character through
//! [`keys_for_char`], pressing a Shift chord where a character needs it.
//!
//! These back the binary's `type_string` / `press_key` script steps so
//! the curriculum capture pipeline can type a program into an on-Amiga
//! editor (e.g. AMOS Pro) and trigger it, mirroring the Spectrum and C64
//! typing tools.

use emu198x_shell::{HeadlessSession, InputEvent, MachineTime, SessionError, SessionQueryProvider};

use crate::AmigaRuntimeKind;
use crate::input::keys_for_char;

/// Default frames a key is held down before release. Three frames at
/// 50 Hz is 60 ms — comfortably above a one-frame keyboard scan.
pub const DEFAULT_KEY_HOLD_FRAMES: u32 = 3;
/// Upper bound on the hold window so a script cannot stall the session.
pub const MAX_KEY_HOLD_FRAMES: u32 = 600;
/// Default settle window run after the final character of a string.
pub const DEFAULT_TYPE_SETTLE_FRAMES: u32 = 10;
/// Frames run after each character's release so consecutive identical
/// keys register as separate presses.
const INTER_CHAR_FRAMES: u32 = 2;

fn queue_key<Q: SessionQueryProvider<AmigaRuntimeKind>>(
    session: &mut HeadlessSession<AmigaRuntimeKind, Q>,
    name: &str,
    pressed: bool,
) {
    session.queue_input(InputEvent::Key {
        name: name.to_owned().into(),
        pressed,
    });
}

/// Presses one named key, holds it for `hold_frames` (clamped to a sane
/// range), releases it, and runs one settle frame. Returns the machine
/// time reached.
///
/// # Errors
///
/// Wraps a [`SessionError`] when a frame-run fails.
pub fn press_key<Q: SessionQueryProvider<AmigaRuntimeKind>>(
    session: &mut HeadlessSession<AmigaRuntimeKind, Q>,
    name: &str,
    hold_frames: u32,
) -> Result<MachineTime, SessionError> {
    let hold = hold_frames.clamp(1, MAX_KEY_HOLD_FRAMES);
    queue_key(session, name, true);
    session.run_frames(hold)?;
    queue_key(session, name, false);
    session.run_frames(1)?;
    Ok(session.time())
}

/// Types `text` through the keyboard, character by character, pressing a
/// `Shift` chord where the character needs one, then runs a settle
/// window. Characters with no single-chord Amiga keycap are skipped.
/// Returns the number of characters actually typed.
///
/// # Errors
///
/// Wraps a [`SessionError`] when a frame-run fails.
pub fn type_string<Q: SessionQueryProvider<AmigaRuntimeKind>>(
    session: &mut HeadlessSession<AmigaRuntimeKind, Q>,
    text: &str,
    hold_frames: u32,
    settle_frames: u32,
) -> Result<u32, SessionError> {
    let hold = hold_frames.clamp(1, MAX_KEY_HOLD_FRAMES);
    let mut typed: u32 = 0;

    for ch in text.chars() {
        let Some(keys) = keys_for_char(ch) else {
            continue;
        };

        for key in &keys {
            queue_key(session, key, true);
        }
        session.run_frames(hold)?;
        for key in keys.iter().rev() {
            queue_key(session, key, false);
        }
        session.run_frames(INTER_CHAR_FRAMES)?;
        typed += 1;
    }

    session.run_frames(settle_frames)?;
    Ok(typed)
}
