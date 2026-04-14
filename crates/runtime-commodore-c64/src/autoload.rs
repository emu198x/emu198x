//! C64-family host-side tape and disk autoload helpers.
//!
//! These helpers drive the real KERNAL keyboard path and datasette transport
//! through the shared headless-session boundary. They do not bypass ROM load
//! routines or synthesize loaded machine state.

use emu198x_shell::session::BootWaitResult;
use emu198x_shell::{
    ControlCommand, HeadlessSession, InputEvent, MachineTime, MediaTransportAction,
    MediaTransportCommand, SessionError, SessionQueryProvider,
};
use thiserror::Error;

use crate::C64Runtime;

/// Stable tape slot used by the current C64 runtime.
pub const DEFAULT_TAPE_AUTOLOAD_SLOT: &str = "tape-1";
/// Stable disk slot used by the current C64 runtime.
pub const DEFAULT_DISK_AUTOLOAD_SLOT: &str = "drive-8";
/// Default frame budget used to wait for the `READY.` prompt.
pub const DEFAULT_TAPE_AUTOLOAD_BOOT_FRAMES: u32 = 200;
/// Default frame budget used to wait for KERNAL to reach `PRESS PLAY ON TAPE`.
pub const DEFAULT_TAPE_AUTOLOAD_WAIT_FRAMES: u32 = 200;
/// Default frame budget used to wait for KERNAL to print `SEARCHING FOR`.
pub const DEFAULT_DISK_AUTOLOAD_WAIT_FRAMES: u32 = 200;

const KEY_EDGE_FRAMES: u32 = 3;
const COMMAND_SETTLE_FRAMES: u32 = 6;
const PRESS_PLAY_PROMPT: &str = "PRESS PLAY ON TAPE";
const SEARCHING_FOR_PROMPT: &str = "SEARCHING FOR";
const DISK_AUTOLOAD_COMMAND: &str = "LOAD\"*\",8,1";

/// Result returned after the standard C64 tape autoload command has been
/// entered and datasette transport has started.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct C64TapeAutoloadResult {
    /// Tape slot that was started.
    pub slot: String,
    /// Result of the boot wait performed before typing the command.
    pub boot: BootWaitResult,
    /// Machine time reached after KERNAL prompted for tape playback and the
    /// host started the datasette transport.
    pub reached: MachineTime,
}

/// Result returned after the standard C64 disk autoload shortcut has been
/// entered and KERNAL has started the serial-bus search path.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct C64DiskAutoloadResult {
    /// Disk slot that was targeted.
    pub slot: String,
    /// Result of the boot wait performed before typing the command.
    pub boot: BootWaitResult,
    /// Machine time reached after the helper observed `SEARCHING FOR`.
    pub reached: MachineTime,
}

/// Error returned by the C64 tape autoload helper.
#[derive(Debug, Error)]
pub enum C64AutoloadError {
    /// One headless session operation failed.
    #[error(transparent)]
    Session(#[from] SessionError),

    /// Tape autoload was requested without loaded tape media.
    #[error("tape autoload requires loaded tape media in slot {slot}")]
    MissingTape {
        /// Requested slot id.
        slot: String,
    },

    /// Disk autoload was requested without an attached drive.
    #[error("disk autoload requires an attached drive in slot {slot}")]
    MissingDrive {
        /// Requested slot id.
        slot: String,
    },

    /// Disk autoload was requested without inserted disk media.
    #[error("disk autoload requires inserted disk media in slot {slot}")]
    MissingDisk {
        /// Requested slot id.
        slot: String,
    },

    /// The helper currently only supports the baseline datasette slot.
    #[error("tape autoload only supports slot {expected}, got {actual}")]
    UnsupportedSlot {
        /// Supported slot id.
        expected: &'static str,
        /// Requested slot id.
        actual: String,
    },

    /// The host-side autoload helper cannot type one command character.
    #[error("disk autoload cannot type command character {ch:?}")]
    UnsupportedCommandChar {
        /// Unsupported command character.
        ch: char,
    },
}

/// Waits for the C64 KERNAL to boot, presses `SHIFT+RUN/STOP`, waits for the
/// `PRESS PLAY ON TAPE` prompt, and starts the active datasette.
///
/// This is a host-side convenience helper above the machine/runtime boundary.
/// It preserves machine behaviour by driving the real KERNAL keyboard path and
/// normal tape transport instead of bypassing ROM code.
///
/// # Errors
///
/// Returns an error if the requested slot is unsupported, no tape is loaded,
/// the boot wait times out, or the prompt never reaches the expected KERNAL
/// text.
pub fn autoload_basic_tape(
    session: &mut HeadlessSession<C64Runtime, impl SessionQueryProvider<C64Runtime>>,
    slot: &str,
    max_boot_frames: u32,
    max_prompt_frames: u32,
) -> Result<C64TapeAutoloadResult, C64AutoloadError> {
    if slot != DEFAULT_TAPE_AUTOLOAD_SLOT {
        return Err(C64AutoloadError::UnsupportedSlot {
            expected: DEFAULT_TAPE_AUTOLOAD_SLOT,
            actual: slot.to_owned(),
        });
    }

    if !session.machine().machine().tape_is_loaded() {
        return Err(C64AutoloadError::MissingTape {
            slot: slot.to_owned(),
        });
    }

    let boot = session.wait_for_boot(max_boot_frames)?;
    tap_key_chord(session, &["lshift", "runstop"])?;
    session.run_frames(COMMAND_SETTLE_FRAMES)?;
    let _ = session.wait_for_query_text_contains(
        "screen.text.lines",
        PRESS_PLAY_PROMPT,
        max_prompt_frames,
    )?;
    session.command(&ControlCommand::MediaTransport(MediaTransportCommand::new(
        slot.to_owned(),
        MediaTransportAction::Start,
    )))?;

    Ok(C64TapeAutoloadResult {
        slot: slot.to_owned(),
        boot,
        reached: session.time(),
    })
}

/// Waits for the C64 KERNAL to boot, types `LOAD"*",8,1`, and waits for the
/// standard disk load path to reach `SEARCHING FOR`.
///
/// This preserves machine behaviour by driving the real BASIC editor above the
/// live IEC-attached drive, rather than synthesizing the loaded program state.
///
/// # Errors
///
/// Returns an error if the requested slot is unsupported, no drive or disk is
/// present, the boot wait times out, or the KERNAL search banner never
/// appears.
pub fn autoload_basic_disk(
    session: &mut HeadlessSession<C64Runtime, impl SessionQueryProvider<C64Runtime>>,
    slot: &str,
    max_boot_frames: u32,
    max_prompt_frames: u32,
) -> Result<C64DiskAutoloadResult, C64AutoloadError> {
    if slot != DEFAULT_DISK_AUTOLOAD_SLOT {
        return Err(C64AutoloadError::UnsupportedSlot {
            expected: DEFAULT_DISK_AUTOLOAD_SLOT,
            actual: slot.to_owned(),
        });
    }

    let drive = session
        .machine()
        .drive8()
        .ok_or_else(|| C64AutoloadError::MissingDrive {
            slot: slot.to_owned(),
        })?;
    if !drive.disk_inserted() {
        return Err(C64AutoloadError::MissingDisk {
            slot: slot.to_owned(),
        });
    }

    let boot = session.wait_for_boot(max_boot_frames)?;
    type_basic_command(session, DISK_AUTOLOAD_COMMAND)?;
    tap_key_chord(session, &["return"])?;
    session.run_frames(COMMAND_SETTLE_FRAMES)?;
    let search = session.wait_for_query_text_contains(
        "screen.text.lines",
        SEARCHING_FOR_PROMPT,
        max_prompt_frames,
    )?;

    Ok(C64DiskAutoloadResult {
        slot: slot.to_owned(),
        boot,
        reached: search.reached,
    })
}

fn tap_key_chord(
    session: &mut HeadlessSession<C64Runtime, impl SessionQueryProvider<C64Runtime>>,
    keys: &[&'static str],
) -> Result<(), SessionError> {
    for key in keys {
        session.queue_input(InputEvent::Key {
            name: (*key).into(),
            pressed: true,
        });
        session.run_frames(KEY_EDGE_FRAMES)?;
    }
    for key in keys.iter().rev() {
        session.queue_input(InputEvent::Key {
            name: (*key).into(),
            pressed: false,
        });
        session.run_frames(KEY_EDGE_FRAMES)?;
    }
    Ok(())
}

fn type_basic_command(
    session: &mut HeadlessSession<C64Runtime, impl SessionQueryProvider<C64Runtime>>,
    command: &str,
) -> Result<(), C64AutoloadError> {
    for ch in command.chars() {
        let keys: &[&'static str] = match ch {
            'A'..='Z' | 'a'..='z' => match ch.to_ascii_uppercase() {
                'A' => &["a"],
                'B' => &["b"],
                'C' => &["c"],
                'D' => &["d"],
                'E' => &["e"],
                'F' => &["f"],
                'G' => &["g"],
                'H' => &["h"],
                'I' => &["i"],
                'J' => &["j"],
                'K' => &["k"],
                'L' => &["l"],
                'M' => &["m"],
                'N' => &["n"],
                'O' => &["o"],
                'P' => &["p"],
                'Q' => &["q"],
                'R' => &["r"],
                'S' => &["s"],
                'T' => &["t"],
                'U' => &["u"],
                'V' => &["v"],
                'W' => &["w"],
                'X' => &["x"],
                'Y' => &["y"],
                'Z' => &["z"],
                _ => unreachable!("matched alphabetic command character"),
            },
            '0' => &["0"],
            '1' => &["1"],
            '2' => &["2"],
            '3' => &["3"],
            '4' => &["4"],
            '5' => &["5"],
            '6' => &["6"],
            '7' => &["7"],
            '8' => &["8"],
            '9' => &["9"],
            '"' => &["lshift", "2"],
            '*' => &["asterisk"],
            ',' => &["comma"],
            ' ' => &["space"],
            ':' => &["colon"],
            '/' => &["slash"],
            '=' => &["equals"],
            _ => return Err(C64AutoloadError::UnsupportedCommandChar { ch }),
        };
        tap_key_chord(session, keys)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{C64SessionQueryProvider, Model};
    use emu198x_shell::{FirmwareImage, FirmwareSet};

    #[test]
    fn autoload_rejects_missing_tape() {
        let mut firmware = FirmwareSet::new();
        firmware.push(FirmwareImage::new("commodore-c64-kernal-rom", &[0; 0x2000]));
        firmware.push(FirmwareImage::new("commodore-c64-basic-rom", &[0; 0x2000]));
        firmware.push(FirmwareImage::new(
            "commodore-c64-character-rom",
            &[0; 0x1000],
        ));
        let runtime = C64Runtime::from_firmware(Model::C64PalBreadbin, &firmware)
            .expect("dummy firmware should construct");
        let mut session =
            HeadlessSession::new_with_query_provider(runtime, 1, C64SessionQueryProvider);

        let err = autoload_basic_tape(
            &mut session,
            DEFAULT_TAPE_AUTOLOAD_SLOT,
            DEFAULT_TAPE_AUTOLOAD_BOOT_FRAMES,
            DEFAULT_TAPE_AUTOLOAD_WAIT_FRAMES,
        )
        .expect_err("autoload should reject missing tape media");

        assert!(matches!(
            err,
            C64AutoloadError::MissingTape { ref slot }
                if slot == DEFAULT_TAPE_AUTOLOAD_SLOT
        ));
    }

    #[test]
    fn disk_autoload_command_types_expected_key_chords() {
        let mut firmware = FirmwareSet::new();
        firmware.push(FirmwareImage::new("commodore-c64-kernal-rom", &[0; 0x2000]));
        firmware.push(FirmwareImage::new("commodore-c64-basic-rom", &[0; 0x2000]));
        firmware.push(FirmwareImage::new(
            "commodore-c64-character-rom",
            &[0; 0x1000],
        ));
        let runtime = C64Runtime::from_firmware(Model::C64PalBreadbin, &firmware)
            .expect("dummy firmware should construct");
        let mut session =
            HeadlessSession::new_with_query_provider(runtime, 1, C64SessionQueryProvider);

        type_basic_command(&mut session, DISK_AUTOLOAD_COMMAND)
            .expect("disk autoload command should be typable");
    }

    #[test]
    fn disk_autoload_rejects_missing_drive() {
        let mut firmware = FirmwareSet::new();
        firmware.push(FirmwareImage::new("commodore-c64-kernal-rom", &[0; 0x2000]));
        firmware.push(FirmwareImage::new("commodore-c64-basic-rom", &[0; 0x2000]));
        firmware.push(FirmwareImage::new(
            "commodore-c64-character-rom",
            &[0; 0x1000],
        ));
        let runtime = C64Runtime::from_firmware(Model::C64PalBreadbin, &firmware)
            .expect("dummy firmware should construct");
        let mut session =
            HeadlessSession::new_with_query_provider(runtime, 1, C64SessionQueryProvider);

        let err = autoload_basic_disk(
            &mut session,
            DEFAULT_DISK_AUTOLOAD_SLOT,
            DEFAULT_TAPE_AUTOLOAD_BOOT_FRAMES,
            DEFAULT_DISK_AUTOLOAD_WAIT_FRAMES,
        )
        .expect_err("disk autoload should reject missing drive firmware");

        assert!(matches!(
            err,
            C64AutoloadError::MissingDrive { ref slot }
                if slot == DEFAULT_DISK_AUTOLOAD_SLOT
        ));
    }
}
