//! Spectrum-family host-side tape autoload helpers.
//!
//! These helpers do not bypass the machine. They drive the real ROM through
//! the shared headless-session boundary, then start the normal tape transport.
//!
//! Two shapes, because the family boots two ways. A 48K reaches a BASIC
//! editor, so the helper types `LOAD ""` into it. The 128K family reaches a
//! menu whose first entry is the tape loader, so the helper selects that
//! instead — the ROM types the command itself. Which one to use is decided by
//! reading the screen rather than by variant, so a machine is handled by what
//! it actually booted to.

use emu198x_shell::session::BootWaitResult;
use emu198x_shell::{
    ControlCommand, HeadlessSession, InputEvent, MachineCore, MachineTime, MediaTransportAction,
    MediaTransportCommand, SessionError, SessionQueryProvider,
};
use thiserror::Error;

use crate::family_runtime::SpectrumLiveAccess;

/// Default frame budget used to wait for the 48K ROM boot banner before typing
/// the standard tape load command.
pub const DEFAULT_TAPE_AUTOLOAD_BOOT_FRAMES: u32 = 250;

/// Stable tape slot used by the current 48K runtime.
pub const DEFAULT_TAPE_AUTOLOAD_SLOT: &str = "tape-1";

pub(crate) const BASIC_PROMPT_ROW: usize = 23;

/// Screen row holding the first entry of the 128K-family boot menu.
///
/// Confirmed by booting each variant for 300 frames: 128K and +2 show
/// `Tape Loader` here, +2A and +3 show `Loader`, and in every case it is
/// the first entry and the one selected at boot.
const MENU_FIRST_ENTRY_ROW: usize = 8;

/// Frame budget for a 128K-family menu to be drawn after the boot banner.
///
/// Measured: the 128K draws its menu 5 frames after `wait_for_boot`
/// returns, the +3 80 frames after. Generous against that, because
/// spending the budget costs a cold 48K only the frames it then spends
/// waiting for its own prompt anyway.
const MENU_WAIT_FRAMES: u32 = 200;

/// Row-23 text the 128K family shows while its tape loader is listening
/// (`To cancel - press BREAK twice`). Matched on the distinctive word so
/// the check does not depend on the surrounding wording or spacing.
const TAPE_PROMPT_MARKER: &str = "BREAK";

/// Frame budget for that prompt to appear after the menu entry is taken.
///
/// Sized for the +3, which is far slower than the rest of the family: its
/// `Loader` tries the disk drive first and only offers tape once that
/// times out, measured at ~2,600 frames — some 52 seconds of machine
/// time. The 128K, +2 and +2A reach the prompt within a few frames, so
/// they never approach this. Waiting is free for them and the difference
/// between loading and not for the +3.
const TAPE_PROMPT_WAIT_FRAMES: u32 = 4_000;

/// Labels the 128K family gives its tape-loader menu entry.
///
/// `Loader` is the +2A/+3 spelling; ordered longest-first so the more
/// specific label is matched before the substring it contains.
const MENU_LOADER_LABELS: &[&str] = &["Tape Loader", "Loader"];
pub(crate) const KEY_EDGE_FRAMES: u32 = 2;
const COMMAND_SETTLE_FRAMES: u32 = 10;

/// Frames run between successive reads while waiting for the `K` prompt.
const PROMPT_POLL_FRAMES: u32 = 2;

/// Frame budget for the `K` prompt to appear after the editor is opened.
///
/// Generous on purpose. The cursor *flashes*, on a ~32-frame cycle, so a
/// wait shorter than one full cycle can miss a prompt that is there —
/// which would turn this fix into a rarer version of the bug it replaces.
const PROMPT_WAIT_FRAMES: u32 = 200;

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
pub fn autoload_basic_tape<R, Q>(
    session: &mut HeadlessSession<R, Q>,
    slot: &str,
    max_boot_frames: u32,
) -> Result<SpectrumTapeAutoloadResult, SpectrumAutoloadError>
where
    R: MachineCore + SpectrumLiveAccess,
    Q: SessionQueryProvider<R>,
{
    if slot != DEFAULT_TAPE_AUTOLOAD_SLOT {
        return Err(SpectrumAutoloadError::UnsupportedSlot {
            expected: DEFAULT_TAPE_AUTOLOAD_SLOT,
            actual: slot.to_owned(),
        });
    }

    if !session.machine().tape_is_loaded() {
        return Err(SpectrumAutoloadError::MissingTape {
            slot: slot.to_owned(),
        });
    }

    let boot = session.wait_for_boot(max_boot_frames)?;

    // The 128K family boots to a menu rather than an editor, so there is
    // no `K` prompt to type into and the 48K path below would time out
    // waiting for one (#50). Decided by what is on screen, not by
    // variant: the machine is handled by what it actually booted to.
    //
    // Waited for, not sampled. `wait_for_boot` returns on the copyright
    // banner, which the 128K draws *before* its menu — at frame 58
    // against the menu's 63, and the +3's menu does not appear until
    // frame 136. A single read here saw a blank row and fell through to
    // the typing path, whose ENTER then selected the loader by accident
    // and left the helper waiting for a `K` that would never come. The
    // blank result was the same tell as #869.
    if wait_for_loader_menu(session)? {
        // The loader entry is the one selected at boot on every variant,
        // so ENTER takes it and the ROM issues the load itself.
        tap_key(session, "enter")?;
        // Then wait for the ROM to actually start listening before the
        // tape rolls. A fixed settle is not enough: the +3 takes far
        // longer to reach its loading prompt than the 128K does, and
        // starting the transport early played the pilot tone at a ROM
        // that was not yet reading it — the tape ran to no effect and the
        // machine sat on "Insert tape and press PLAY" forever.
        wait_for_tape_prompt(session)?;
        session.command(&ControlCommand::MediaTransport(MediaTransportCommand::new(
            slot.to_owned(),
            MediaTransportAction::Start,
        )))?;
        return Ok(SpectrumTapeAutoloadResult {
            slot: slot.to_owned(),
            boot,
            reached: session.time(),
        });
    }

    // On a cold boot row 23 holds the copyright message, so ENTER is
    // tapped to open the editor.
    if !basic_prompt_ready(session)? {
        tap_key(session, "enter")?;
    }

    // Then *wait* for the prompt, rather than reading once.
    //
    // This read used to happen with no frames run in between, so it
    // sampled row 23 before the ROM had repainted it and got 32 spaces —
    // neither the copyright line nor `K` — and autoload failed on every
    // cold 48K boot, in the UI as well as headless (#869). The blank
    // result was the tell: the ROM had cleared the line and not yet drawn
    // the cursor.
    //
    // Polling rather than a fixed settle, because the cursor flashes: a
    // single sample at any fixed delay can land on the wrong half of the
    // cycle. Every other keyboard step in this file already settles
    // deliberately; this one did not.
    wait_for_basic_prompt(session)?;

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

/// Run frames until the 128K-family loader says it is waiting for tape.
///
/// Every variant in the family shows the same cancel hint on row 23 while
/// its tape loader is listening, so that is the signal. Falls through
/// after [`TAPE_PROMPT_WAIT_FRAMES`] rather than failing: starting the
/// transport anyway is what this helper did before, so a ROM that words
/// its prompt differently is no worse off than it was.
fn wait_for_tape_prompt<R, Q>(session: &mut HeadlessSession<R, Q>) -> Result<(), SessionError>
where
    R: MachineCore,
    Q: SessionQueryProvider<R>,
{
    let mut waited = 0;
    while waited < TAPE_PROMPT_WAIT_FRAMES {
        if decoded_prompt_line(session)?.contains(TAPE_PROMPT_MARKER) {
            return Ok(());
        }
        session.run_frames(PROMPT_POLL_FRAMES)?;
        waited += PROMPT_POLL_FRAMES;
    }
    Ok(())
}

/// Wait for a 128K-family boot menu whose first entry is the tape loader.
///
/// `true` means the menu is up and ENTER will take the loader. `false`
/// means this machine reached an editor instead, or showed neither within
/// [`MENU_WAIT_FRAMES`] — a cold 48K sits on its copyright screen until
/// ENTER is tapped, which the caller's typing path does.
///
/// Matching only the known loader labels is deliberate. Treating *any*
/// text on that row as a menu would misread an unrelated screen as one,
/// and a machine whose menu this helper cannot recognise is no worse off
/// than before: it falls through and reports the prompt it did find.
///
/// Waited for, not sampled, for the same reason as everything else here:
/// `wait_for_boot` returns on the copyright banner, and a partly-painted
/// row reads as a truncated label — the 128K was caught mid-paint showing
/// `Tape Loade`.
///
/// Deliberately does not tap ENTER itself: on the 128K family that would
/// select a menu entry before anything had decided which one.
fn wait_for_loader_menu<R, Q>(session: &mut HeadlessSession<R, Q>) -> Result<bool, SessionError>
where
    R: MachineCore,
    Q: SessionQueryProvider<R>,
{
    let mut waited = 0;
    loop {
        if let Some(entry) = menu_first_entry(session)?
            && MENU_LOADER_LABELS
                .iter()
                .any(|label| entry.eq_ignore_ascii_case(label))
        {
            return Ok(true);
        }
        if basic_prompt_ready(session)? || waited >= MENU_WAIT_FRAMES {
            return Ok(false);
        }
        session.run_frames(PROMPT_POLL_FRAMES)?;
        waited += PROMPT_POLL_FRAMES;
    }
}

/// The first entry of the 128K-family boot menu, if one is on screen.
///
/// Returns `None` on a machine that booted to an editor instead, which is
/// how the 48K falls through to the typing path.
///
/// The menu is drawn inside a box of block-graphic characters, so the row
/// carries non-ASCII glyphs either side of the label; those are stripped
/// rather than matched, leaving the label itself.
fn menu_first_entry<R, Q>(session: &HeadlessSession<R, Q>) -> Result<Option<String>, SessionError>
where
    R: MachineCore,
    Q: SessionQueryProvider<R>,
{
    let result = session.query("screen.text.lines")?;
    let Some(lines) = result.value.as_array() else {
        return Err(SessionError::UnexpectedQueryValue {
            path: "screen.text.lines".to_owned(),
            expected: "an array of strings",
        });
    };
    let Some(row) = lines
        .get(MENU_FIRST_ENTRY_ROW)
        .and_then(serde_json::Value::as_str)
    else {
        return Ok(None);
    };
    let label: String = row
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == ' ')
        .collect();
    let label = label.trim().to_owned();
    Ok((!label.is_empty()).then_some(label))
}

/// Run frames until row 23 shows the `K` keyword-entry cursor.
///
/// # Errors
///
/// Returns [`SpectrumAutoloadError::PromptNotReady`] carrying the last
/// line seen if the prompt does not appear within [`PROMPT_WAIT_FRAMES`].
fn wait_for_basic_prompt<R, Q>(
    session: &mut HeadlessSession<R, Q>,
) -> Result<(), SpectrumAutoloadError>
where
    R: MachineCore,
    Q: SessionQueryProvider<R>,
{
    let mut waited = 0;
    loop {
        let line = decoded_prompt_line(session)?;
        if line.trim_end() == "K" {
            return Ok(());
        }
        if waited >= PROMPT_WAIT_FRAMES {
            return Err(SpectrumAutoloadError::PromptNotReady { line });
        }
        session.run_frames(PROMPT_POLL_FRAMES)?;
        waited += PROMPT_POLL_FRAMES;
    }
}

fn basic_prompt_ready<R, Q>(session: &HeadlessSession<R, Q>) -> Result<bool, SessionError>
where
    R: MachineCore,
    Q: SessionQueryProvider<R>,
{
    Ok(decoded_prompt_line(session)?.trim_end() == "K")
}

pub(crate) fn decoded_prompt_line<R, Q>(
    session: &HeadlessSession<R, Q>,
) -> Result<String, SessionError>
where
    R: MachineCore,
    Q: SessionQueryProvider<R>,
{
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

/// Presses and releases one Spectrum key through the headless session, driving
/// the real ROM keyboard editor (no ROM patching).
///
/// `name` is a [`common_sinclair_zx_spectrum::SpectrumKey`] name (e.g. `"j"`,
/// `"enter"`, a digit `"1"`). Remember the 48K editor's cursor modes: at the
/// start of a line the cursor is `K`, so a single letter key enters that key's
/// *keyword* (`"j"` → `LOAD`, `"s"` → `SAVE`, `"p"` → `PRINT`, `"e"` → `REM`);
/// after a keyword the cursor is `L`, so letters enter literally. For symbols
/// that need SYMBOL SHIFT (e.g. `"` is SYMBOL SHIFT + `P`) use
/// [`tap_symbol_combo`]. This is the canonical way to type into the 48K BASIC
/// editor from a test or tool — see `docs/systems/sinclair/zx-spectrum/index.md`
/// § "Port $FE I/O, tape SAVE/LOAD, and driving the keyboard from tests".
pub fn tap_key<R, Q>(
    session: &mut HeadlessSession<R, Q>,
    name: &'static str,
) -> Result<(), SessionError>
where
    R: MachineCore,
    Q: SessionQueryProvider<R>,
{
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

/// Presses SYMBOL SHIFT + `name` through the headless session, for the symbols
/// that the 48K editor places on the symbol-shifted layer (e.g. `"` is SYMBOL
/// SHIFT + `P`, `;` is SYMBOL SHIFT + `O`). See [`tap_key`] for the cursor-mode
/// notes.
pub fn tap_symbol_combo<R, Q>(
    session: &mut HeadlessSession<R, Q>,
    name: &'static str,
) -> Result<(), SessionError>
where
    R: MachineCore,
    Q: SessionQueryProvider<R>,
{
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
    use crate::Spectrum48kRuntime;
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

    /// Provider that reports both a detected boot status and a row-23
    /// prompt of literal "K". Used to drive the full autoload success
    /// path without needing a real ROM.
    struct ReadyPromptProvider;

    impl SessionQueryProvider<Spectrum48kRuntime> for ReadyPromptProvider {
        fn query_paths(&self, _machine: &Spectrum48kRuntime, _prefix: Option<&str>) -> Vec<String> {
            vec![
                "boot.detected".to_owned(),
                "boot.reason".to_owned(),
                "boot.row".to_owned(),
                "screen.text.lines".to_owned(),
            ]
        }

        fn query(
            &self,
            _machine: &Spectrum48kRuntime,
            path: &str,
        ) -> Result<Option<QueryResult>, emu198x_shell::QueryError> {
            let value = match path {
                "boot.detected" => json!(true),
                "boot.reason" => json!("found copyright banner on row 23"),
                "boot.row" => json!(23),
                "screen.text.lines" => {
                    let mut lines = vec![" ".repeat(32); 24];
                    lines[BASIC_PROMPT_ROW] = "K                               ".to_owned();
                    json!(lines)
                }
                _ => return Ok(None),
            };
            Ok(Some(QueryResult {
                path: path.to_owned(),
                value,
            }))
        }
    }

    /// Provider returning a non-K prompt so we can drive the
    /// `PromptNotReady` error arm. Reports boot detected so
    /// `wait_for_boot` returns immediately.
    struct StuckPromptProvider;

    impl SessionQueryProvider<Spectrum48kRuntime> for StuckPromptProvider {
        fn query_paths(&self, _machine: &Spectrum48kRuntime, _prefix: Option<&str>) -> Vec<String> {
            vec!["boot.detected".to_owned(), "screen.text.lines".to_owned()]
        }

        fn query(
            &self,
            _machine: &Spectrum48kRuntime,
            path: &str,
        ) -> Result<Option<QueryResult>, emu198x_shell::QueryError> {
            let value = match path {
                "boot.detected" => json!(true),
                "screen.text.lines" => {
                    // Every row shows "X" — no K prompt anywhere.
                    let lines = vec!["X".repeat(32); 24];
                    json!(lines)
                }
                _ => return Ok(None),
            };
            Ok(Some(QueryResult {
                path: path.to_owned(),
                value,
            }))
        }
    }

    fn loaded_runtime() -> Spectrum48kRuntime {
        let mut firmware = FirmwareSet::new();
        firmware.push(FirmwareImage::new(
            "sinclair-zx-spectrum-48k-rom",
            &[0; 16 * 1024],
        ));
        let mut runtime =
            Spectrum48kRuntime::from_firmware(&firmware).expect("dummy firmware should boot");
        // Minimal 19-byte tape header so `tape_is_loaded()` returns true.
        let blocks = vec![common_sinclair_zx_spectrum::tape::TapeBlock {
            flag: 0x00,
            data: vec![0u8; 19],
        }];
        runtime.machine_mut().load_tape_blocks(blocks);
        runtime
    }

    #[test]
    fn autoload_rejects_unsupported_slot() {
        let runtime = loaded_runtime();
        let mut session =
            HeadlessSession::new_with_query_provider(runtime, 1, SpectrumSessionQueryProvider);

        let err = autoload_basic_tape(&mut session, "tape-9", DEFAULT_TAPE_AUTOLOAD_BOOT_FRAMES)
            .expect_err("autoload should reject non-default slots");
        match err {
            SpectrumAutoloadError::UnsupportedSlot { expected, actual } => {
                assert_eq!(expected, DEFAULT_TAPE_AUTOLOAD_SLOT);
                assert_eq!(actual, "tape-9");
            }
            other => panic!("expected UnsupportedSlot, got {other:?}"),
        }
    }

    #[test]
    fn autoload_runs_through_to_tape_transport_start() {
        // Drives the full happy path: boot wait, prompt-K detection,
        // LOAD"" key sequence, and tape transport command.
        let runtime = loaded_runtime();
        let mut session = HeadlessSession::new_with_query_provider(runtime, 1, ReadyPromptProvider);

        let result = autoload_basic_tape(
            &mut session,
            DEFAULT_TAPE_AUTOLOAD_SLOT,
            DEFAULT_TAPE_AUTOLOAD_BOOT_FRAMES,
        )
        .expect("autoload should drive the full success path");

        assert_eq!(result.slot, DEFAULT_TAPE_AUTOLOAD_SLOT);
        assert_eq!(result.boot.row, Some(23));
        assert!(session.machine().machine().tape_is_playing());
    }

    #[test]
    fn autoload_reports_prompt_not_ready_when_row_23_is_not_k() {
        let runtime = loaded_runtime();
        let mut session = HeadlessSession::new_with_query_provider(runtime, 1, StuckPromptProvider);

        let err = autoload_basic_tape(
            &mut session,
            DEFAULT_TAPE_AUTOLOAD_SLOT,
            DEFAULT_TAPE_AUTOLOAD_BOOT_FRAMES,
        )
        .expect_err("autoload must refuse to type into a non-K prompt");

        match err {
            SpectrumAutoloadError::PromptNotReady { line } => {
                assert!(
                    line.starts_with('X'),
                    "PromptNotReady should carry the decoded row, got {line:?}"
                );
            }
            other => panic!("expected PromptNotReady, got {other:?}"),
        }
    }
}
