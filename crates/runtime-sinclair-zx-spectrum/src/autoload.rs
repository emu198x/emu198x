//! Spectrum-family host-side tape autoload helpers.
//!
//! These helpers do not bypass the machine. They drive the real 48K ROM editor
//! through the shared headless-session boundary, then start the normal tape
//! transport once `LOAD ""` has been entered.

use emu198x_shell::session::BootWaitResult;
use emu198x_shell::{
    ControlCommand, HeadlessSession, InputEvent, MachineTime, MediaTransportAction,
    MediaTransportCommand, SessionError, SessionQueryProvider,
};
use thiserror::Error;

use crate::Spectrum48kRuntime;

/// Default frame budget used to wait for the 48K ROM boot banner before typing
/// the standard tape load command.
pub const DEFAULT_TAPE_AUTOLOAD_BOOT_FRAMES: u32 = 250;

/// Stable tape slot used by the current 48K runtime.
pub const DEFAULT_TAPE_AUTOLOAD_SLOT: &str = "tape-1";

const BASIC_PROMPT_ROW: usize = 23;
const KEY_EDGE_FRAMES: u32 = 2;
const COMMAND_SETTLE_FRAMES: u32 = 10;

/// Result returned after the standard 48K tape autoload command has been
/// entered and tape transport has started.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpectrumTapeAutoloadResult {
    /// Tape slot that was started.
    pub slot: String,
    /// Result of the boot wait performed before typing the command.
    pub boot: BootWaitResult,
    /// Machine time reached after the command has been typed and tape
    /// transport has started.
    pub reached: MachineTime,
}

/// Error returned by the Spectrum tape autoload helper.
#[derive(Debug, Error)]
pub enum SpectrumAutoloadError {
    /// One headless session operation failed.
    #[error(transparent)]
    Session(#[from] SessionError),

    /// Tape autoload was requested without loaded tape media.
    #[error("tape autoload requires loaded tape media in slot {slot}")]
    MissingTape { slot: String },

    /// The helper currently only supports the 48K tape slot.
    #[error("tape autoload only supports slot {expected}, got {actual}")]
    UnsupportedSlot {
        /// Supported slot id.
        expected: &'static str,
        /// Requested slot id.
        actual: String,
    },

    /// The 48K BASIC editor prompt was not ready for keyword entry.
    #[error("48K BASIC prompt was not ready for tape autoload; row 23 was {line:?}")]
    PromptNotReady {
        /// Decoded row-23 text seen after the helper tried to expose the prompt.
        line: String,
    },
}

/// Waits for the 48K ROM to boot, types the standard `LOAD ""` command, and
/// starts the current tape.
///
/// This is a host-side convenience helper above the machine/runtime boundary.
/// It preserves exact machine behaviour by driving the real ROM editor and tape
/// transport rather than patching ROM code or bypassing media decoding.
///
/// # Errors
///
/// Returns an error if the requested slot is unsupported, no tape is loaded,
/// the boot wait times out, or the prompt is not ready for keyword entry.
pub fn autoload_basic_tape(
    session: &mut HeadlessSession<
        Spectrum48kRuntime,
        impl SessionQueryProvider<Spectrum48kRuntime>,
    >,
    slot: &str,
    max_boot_frames: u32,
) -> Result<SpectrumTapeAutoloadResult, SpectrumAutoloadError> {
    if slot != DEFAULT_TAPE_AUTOLOAD_SLOT {
        return Err(SpectrumAutoloadError::UnsupportedSlot {
            expected: DEFAULT_TAPE_AUTOLOAD_SLOT,
            actual: slot.to_owned(),
        });
    }

    if !session.machine().machine().tape_is_loaded() {
        return Err(SpectrumAutoloadError::MissingTape {
            slot: slot.to_owned(),
        });
    }

    let boot = session.wait_for_boot(max_boot_frames)?;

    if !basic_prompt_ready(session)? {
        tap_key(session, "enter")?;
    }

    let prompt_line = decoded_prompt_line(session)?;
    if prompt_line.trim_end() != "K" {
        return Err(SpectrumAutoloadError::PromptNotReady { line: prompt_line });
    }

    tap_key(session, "j")?;
    tap_symbol_combo(session, "p")?;
    tap_symbol_combo(session, "p")?;
    tap_key(session, "enter")?;
    session.run_frames(COMMAND_SETTLE_FRAMES)?;
    session.command(&ControlCommand::MediaTransport(MediaTransportCommand::new(
        slot.to_owned(),
        MediaTransportAction::Start,
    )))?;

    Ok(SpectrumTapeAutoloadResult {
        slot: slot.to_owned(),
        boot,
        reached: session.time(),
    })
}

fn basic_prompt_ready(
    session: &HeadlessSession<Spectrum48kRuntime, impl SessionQueryProvider<Spectrum48kRuntime>>,
) -> Result<bool, SessionError> {
    Ok(decoded_prompt_line(session)?.trim_end() == "K")
}

fn decoded_prompt_line(
    session: &HeadlessSession<Spectrum48kRuntime, impl SessionQueryProvider<Spectrum48kRuntime>>,
) -> Result<String, SessionError> {
    let result = session.query("screen.text.lines")?;
    let Some(lines) = result.value.as_array() else {
        return Err(SessionError::UnexpectedQueryValue {
            path: "screen.text.lines".to_owned(),
            expected: "an array of strings",
        });
    };
    let Some(line) = lines.get(BASIC_PROMPT_ROW).and_then(|value| value.as_str()) else {
        return Err(SessionError::UnexpectedQueryValue {
            path: "screen.text.lines".to_owned(),
            expected: "an array of strings",
        });
    };

    Ok(line.to_owned())
}

fn tap_key(
    session: &mut HeadlessSession<
        Spectrum48kRuntime,
        impl SessionQueryProvider<Spectrum48kRuntime>,
    >,
    name: &'static str,
) -> Result<(), SessionError> {
    session.queue_input(InputEvent::Key {
        name: name.into(),
        pressed: true,
    });
    session.run_frames(KEY_EDGE_FRAMES)?;
    session.queue_input(InputEvent::Key {
        name: name.into(),
        pressed: false,
    });
    session.run_frames(KEY_EDGE_FRAMES)?;
    Ok(())
}

fn tap_symbol_combo(
    session: &mut HeadlessSession<
        Spectrum48kRuntime,
        impl SessionQueryProvider<Spectrum48kRuntime>,
    >,
    name: &'static str,
) -> Result<(), SessionError> {
    session.queue_input(InputEvent::Key {
        name: "symbol".into(),
        pressed: true,
    });
    session.run_frames(KEY_EDGE_FRAMES)?;
    session.queue_input(InputEvent::Key {
        name: name.into(),
        pressed: true,
    });
    session.run_frames(KEY_EDGE_FRAMES)?;
    session.queue_input(InputEvent::Key {
        name: name.into(),
        pressed: false,
    });
    session.run_frames(KEY_EDGE_FRAMES)?;
    session.queue_input(InputEvent::Key {
        name: "symbol".into(),
        pressed: false,
    });
    session.run_frames(KEY_EDGE_FRAMES)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SpectrumSessionQueryProvider;
    use emu198x_shell::{FirmwareImage, FirmwareSet, QueryResult};
    use serde_json::json;

    struct FakePromptProvider;

    impl SessionQueryProvider<Spectrum48kRuntime> for FakePromptProvider {
        fn query_paths(&self, _machine: &Spectrum48kRuntime, _prefix: Option<&str>) -> Vec<String> {
            vec!["screen.text.lines".to_owned()]
        }

        fn query(
            &self,
            _machine: &Spectrum48kRuntime,
            path: &str,
        ) -> Result<Option<QueryResult>, emu198x_shell::QueryError> {
            if path != "screen.text.lines" {
                return Ok(None);
            }

            let mut lines = vec![" ".repeat(32); 24];
            lines[BASIC_PROMPT_ROW] = "K                               ".to_owned();
            Ok(Some(QueryResult {
                path: path.to_owned(),
                value: json!(lines),
            }))
        }
    }

    #[test]
    fn autoload_rejects_missing_tape() {
        let mut firmware = FirmwareSet::new();
        firmware.push(FirmwareImage::new(
            "sinclair-zx-spectrum-48k-rom",
            &[0; 16 * 1024],
        ));
        let runtime =
            Spectrum48kRuntime::from_firmware(&firmware).expect("dummy firmware should boot");
        let mut session =
            HeadlessSession::new_with_query_provider(runtime, 1, SpectrumSessionQueryProvider);

        let err = autoload_basic_tape(
            &mut session,
            DEFAULT_TAPE_AUTOLOAD_SLOT,
            DEFAULT_TAPE_AUTOLOAD_BOOT_FRAMES,
        )
        .expect_err("autoload should reject missing tape media");

        assert!(matches!(
            err,
            SpectrumAutoloadError::MissingTape { ref slot }
                if slot == DEFAULT_TAPE_AUTOLOAD_SLOT
        ));
    }

    #[test]
    fn basic_prompt_ready_accepts_literal_k_prompt() {
        let mut firmware = FirmwareSet::new();
        firmware.push(FirmwareImage::new(
            "sinclair-zx-spectrum-48k-rom",
            &[0; 16 * 1024],
        ));
        let runtime =
            Spectrum48kRuntime::from_firmware(&firmware).expect("dummy firmware should boot");
        let session = HeadlessSession::new_with_query_provider(runtime, 1, FakePromptProvider);

        assert!(basic_prompt_ready(&session).expect("prompt query should succeed"));
    }
}
