//! Cross-system curated catalogue. See `knowledge/decisions/october-catalogue.md`.
//!
//! This crate is the October-launch regression bench: 10 titles per system
//! across the four launch targets (Spectrum, C64, NES, Amiga). Each entry
//! asserts a boot frame hash, optional guest-input startup navigation, and an
//! audio-window hash.
//!
//! The runner dispatches all four system families and the supported variants
//! named by their manifests. A manifest remains incomplete until all of its
//! intended catalogue entries carry verified frame and audio assertions.

use std::hash::Hasher;
use std::path::{Path, PathBuf};

use common_commodore_c64::timing::{TIMING_NTSC_BREADBIN, TIMING_PAL_BREADBIN};
use common_sinclair_zx_spectrum::memory::MemoryBus;
use common_sinclair_zx_spectrum::timing::{TIMING_48K, TIMING_128K, TIMING_PLUS2A};
use emu198x_shell::{
    ControlCommand, FamilyRuntime, FirmwareImage, FirmwareSet, HeadlessSession, InputEvent,
    MachineCore, MediaImage, MediaKind, MediaSet, MediaTransportAction, MediaTransportCommand,
    ScriptStep as ShellScriptStep, SessionQueryProvider, read_firmware_asset, read_media_asset,
};
use machine_sinclair_zx_spectrum_128k::Spectrum128K;
use machine_sinclair_zx_spectrum_plus2::SpectrumPlus2;
use machine_sinclair_zx_spectrum_plus2a::SpectrumPlus2A;
use machine_sinclair_zx_spectrum_plus2b::SpectrumPlus2B;
use machine_sinclair_zx_spectrum_plus3::SpectrumPlus3;
use runtime_commodore_amiga::{AmigaRuntimeKind, AmigaSessionQueryProvider, Model as AmigaModel};
use runtime_commodore_c64::{
    C64Runtime, C64SessionQueryProvider, DEFAULT_DISK_1581_AUTOLOAD_SLOT,
    DriveKind as C64DriveKind, Model as C64Model, autoload_basic_disk, autoload_basic_disk_1581,
    autoload_basic_tape as c64_autoload_basic_tape,
};
use runtime_nintendo_nes::{Model as NesModel, NesRuntime, NesSessionQueryProvider};
use runtime_sinclair_zx_spectrum::{
    DEFAULT_TAPE_AUTOLOAD_BOOT_FRAMES, Model, Spectrum16kRuntime, Spectrum48kRuntime,
    Spectrum128kRuntime, SpectrumPlus2ARuntime, SpectrumPlus2BRuntime, SpectrumPlus2Runtime,
    SpectrumPlus3Runtime, SpectrumPlusRuntime, SpectrumSessionQueryProvider, autoload_basic_tape,
};
use serde::Deserialize;
use thiserror::Error;
use twox_hash::XxHash64;

/// Safety cap on the tape-load wait. At PAL 50 fps this is ~20 minutes
/// of emulation time — far longer than any 48K/128K loader needs.
const MAX_TAPE_LOAD_FRAMES: u32 = 60_000;

/// Frame budget for waiting on the 128K menu boot banner before pressing
/// ENTER for Tape Loader.
const DEFAULT_128K_BOOT_FRAMES: u32 = 250;

/// PPU dots per frame for NTSC NES (341 dots × 262 scanlines).
const NES_NTSC_FRAME_TICKS: u64 = 341 * 262;

/// Frames per second for NTSC NES. Master clock 21.477 MHz / (PPU divisor
/// 4 × 89,342 dots per frame) ≈ 60.0988 Hz.
const NES_NTSC_FRAMES_PER_SEC: f64 = 60.098_8;

/// Frame budget for waiting on C64 KERNAL to reach READY.
const DEFAULT_C64_BOOT_FRAMES: u32 = 240;

/// Frame budget for the C64 disk autoload to see the "SEARCHING FOR"
/// prompt after issuing LOAD.
const DEFAULT_C64_DISK_PROMPT_FRAMES: u32 = 600;

/// Longest exact frame wait allowed in one catalogue startup action.
/// At PAL 50 fps this is twenty minutes of emulated time.
const MAX_STARTUP_WAIT_FRAMES: u32 = 60_000;

/// Longest key, mouse-button, or controller-button hold accepted by the
/// catalogue startup navigator.
const MAX_STARTUP_HOLD_FRAMES: u32 = 120;

/// Aggregate cap for one startup sequence. This prevents a typo in a
/// checked-in manifest from turning one catalogue entry into an unbounded run.
const MAX_STARTUP_TOTAL_FRAMES: u32 = 120_000;

const DEFAULT_STARTUP_HOLD_FRAMES: u32 = 3;
const STARTUP_RELEASE_OBSERVATION_FRAMES: u32 = 1;

/// Top-level manifest shape (one TOML file per system).
#[derive(Debug, Deserialize)]
pub struct Manifest {
    pub system: SystemMeta,
    pub entry: Vec<Entry>,
}

/// System-level metadata. Firmware is variant-scoped because variants
/// within a family (48K vs 128K vs +3) can need a different ROM count
/// and layout. Firmware is optional — NES has no BIOS, for example.
#[derive(Debug, Deserialize)]
pub struct SystemMeta {
    /// Stable system identifier (e.g. `spectrum`, `c64`, `nes`, `amiga`).
    pub id: String,
    /// Per-variant firmware lookup. Empty for systems without a BIOS.
    #[serde(default)]
    pub firmware: Vec<VariantFirmware>,
    /// Routing version the audio hashes in this manifest were captured
    /// against. Compared per-system at run time against the system's
    /// `AUDIO_ROUTING_VERSION` constant. `None` skips the check (legacy
    /// manifests). Mismatch fails loud with a re-capture instruction.
    /// See `knowledge/decisions/spectrum-architecture-review.md` Seam 4.
    #[serde(default)]
    pub audio_routing_version: Option<u32>,
    /// Routing version the frame hashes in this manifest were captured
    /// against. Same semantics as `audio_routing_version`.
    #[serde(default)]
    pub frame_routing_version: Option<u32>,
}

/// Firmware files required to boot one variant. The runner reads each
/// file in declared order and feeds the bytes to the variant's
/// constructor.
#[derive(Debug, Deserialize)]
pub struct VariantFirmware {
    /// Variant identifier matching `Entry.variant`.
    pub variant: String,
    pub files: Vec<FirmwareSpec>,
}

/// One firmware file: stable id (consumed by the shell's firmware set)
/// plus a path resolved against the firmware root.
#[derive(Debug, Deserialize)]
pub struct FirmwareSpec {
    pub id: String,
    pub path: String,
}

/// One catalogue entry: a single curated title with its assertions.
#[derive(Debug, Deserialize)]
pub struct Entry {
    pub id: String,
    pub title: String,
    pub year: u16,
    pub publisher: String,
    /// Variant identifier within the system (e.g. `48k`, `128k`, `+3`).
    pub variant: String,
    /// Media slot. Optional — entries that test the firmware path alone
    /// (e.g. C64 boot-to-READY) leave this empty.
    pub media: Option<Media>,
    pub boot: Boot,
    #[serde(default)]
    pub script: Vec<ScriptStep>,
    /// Sequential startup navigation through release screens, trainers,
    /// selectors, and similar guest UI. This is deliberately restricted to
    /// exact waits and ordinary emulated input. New entries should use this
    /// instead of the legacy absolute-frame `script` form.
    #[serde(default)]
    pub startup: Vec<StartupStep>,
    pub audio: Audio,
}

/// Media slot description. The path is resolved against the catalogue
/// media root.
#[derive(Debug, Deserialize)]
pub struct Media {
    /// Media kind: `tape`, `disk`, `cartridge`, `optical`, `program`, or
    /// `snapshot` — matches the shell's `MediaKind` enum.
    pub kind: String,
    /// Stable slot identifier consumed by the runtime (e.g. `tape-1`).
    pub slot: String,
    /// Path relative to the catalogue media root.
    pub path: String,
    /// Whether the guest may write to this image. Archive media defaults to
    /// read-only; a work disk must opt in explicitly.
    #[serde(default)]
    pub writable: bool,
    /// Extra one-shot tape-motor spin-up delay in phi2 cycles, shifting
    /// the whole tape timeline relative to the machine's frame-locked
    /// boot sequence — i.e. *when within a frame the user pressed PLAY*.
    /// Some loader/game combinations gate their IRQ timing off the
    /// tape-start phase and lose a race at a handful of phases (Novaload
    /// "Monty on the Run"); a small nudge here picks a phase the game
    /// survives, exactly as a real user's PLAY press would.
    #[serde(default)]
    pub play_phase_cycles: u32,
}

/// Boot waypoint. After the system's setup phase completes (tape stops,
/// cartridge boots, KERNAL reaches READY) the runner runs either legacy
/// `script[]` steps or sequential `startup[]` actions, then waits
/// `wait_frames` more frames and captures the frame. For games that need a
/// LOAD-then-RUN sequence (e.g. C64 disk titles), the input happens before
/// this capture so the boot frame lands on the actual title screen.
#[derive(Debug, Deserialize)]
pub struct Boot {
    pub wait_frames: u32,
    /// Expected `xxh64:HEX` of the RGBA8888 frame at the waypoint.
    pub frame_hash: String,
    /// Regions whose RGBA pixels are zeroed before the boot frame is hashed.
    ///
    /// This is reserved for small, understood guest-owned readouts whose
    /// value legitimately depends on the exact capture instant. The PNG
    /// remains unmodified so the complete captured frame is still available
    /// for human review.
    #[serde(default)]
    pub ignore_rects: Vec<BootIgnoreRect>,
}

/// One pixel-aligned rectangle excluded from a boot frame hash.
///
/// Coordinates use a top-left origin and an exclusive right/bottom edge:
/// `(x, y, width, height)`. Width and height must be nonzero, and the whole
/// rectangle must fit within the captured frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BootIgnoreRect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

/// One scripted input step. `at_frame` is counted from the start of the
/// script phase (which is immediately after the system's setup phase
/// completes — tape stop, cartridge boot, etc.). Each press/click/
/// button consumes 3 frames between queueing the press and queueing
/// the release.
///
/// Untagged enum: TOML chooses between `press` (keyboard), `click`
/// (mouse port-0 button), and `button`+`port` (joystick port button)
/// based on which fields are present.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum ScriptStep {
    /// Keyboard key press. `press` is a key name (`enter`, `space`,
    /// `r`, `0`, etc.) — system-specific name lookup applies.
    Press { at_frame: u32, press: String },
    /// Mouse port-0 button click. `click` is `left`, `right`, or
    /// `middle`. Currently only honoured on Amiga; other systems
    /// silently drop pointer events.
    Click { at_frame: u32, click: String },
    /// Joystick button press on a controller port. Port numbering follows
    /// the machine's labelled hardware: Amiga control port 2 is its normal
    /// joystick (`JOY1DAT`), while C64 software commonly uses control port
    /// 2. Port 0 is the cross-system primary-stick alias. `button` is
    /// `fire`, `button1`, etc. — system-specific.
    Button {
        at_frame: u32,
        port: u8,
        button: String,
    },
}

impl ScriptStep {
    fn at_frame(&self) -> u32 {
        match self {
            Self::Press { at_frame, .. }
            | Self::Click { at_frame, .. }
            | Self::Button { at_frame, .. } => *at_frame,
        }
    }
}

/// One sequential, bounded startup-navigation action.
///
/// The catalogue uses this restricted language to pass release screens,
/// trainers, selectors, and prompts without modifying media or guest state.
/// Each input action expands to the shared shell executor's ordinary
/// [`InputEvent`] plus an exact [`ShellScriptStep::RunFrames`] hold and one
/// release-observation frame. Actions run in manifest order; waits are
/// relative to the preceding action.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum StartupStep {
    /// Advance the machine for an exact, bounded number of native frames.
    WaitFrames { frames: u32 },
    /// Press and release one symbolic keyboard key.
    PressKey {
        key: String,
        #[serde(default = "default_startup_hold_frames")]
        hold_frames: u32,
    },
    /// Click one button on the primary Amiga-style mouse device.
    ClickMouse {
        button: String,
        #[serde(default = "default_startup_hold_frames")]
        hold_frames: u32,
    },
    /// Press and release one named controller button on a labelled port.
    PressButton {
        port: u8,
        button: String,
        #[serde(default = "default_startup_hold_frames")]
        hold_frames: u32,
    },
}

const fn default_startup_hold_frames() -> u32 {
    DEFAULT_STARTUP_HOLD_FRAMES
}

impl StartupStep {
    fn frame_cost(&self) -> u32 {
        match self {
            Self::WaitFrames { frames } => *frames,
            Self::PressKey { hold_frames, .. }
            | Self::ClickMouse { hold_frames, .. }
            | Self::PressButton { hold_frames, .. } => {
                hold_frames.saturating_add(STARTUP_RELEASE_OBSERVATION_FRAMES)
            }
        }
    }

    fn shell_steps(&self) -> Vec<ShellScriptStep> {
        match self {
            Self::WaitFrames { frames } => {
                vec![ShellScriptStep::RunFrames { frames: *frames }]
            }
            Self::PressKey { key, hold_frames } => input_press_steps(
                InputEvent::Key {
                    name: key.clone().into(),
                    pressed: true,
                },
                InputEvent::Key {
                    name: key.clone().into(),
                    pressed: false,
                },
                *hold_frames,
            ),
            Self::ClickMouse {
                button,
                hold_frames,
            } => input_press_steps(
                InputEvent::PointerButton {
                    device: "mouse-1".into(),
                    button: button.clone().into(),
                    pressed: true,
                },
                InputEvent::PointerButton {
                    device: "mouse-1".into(),
                    button: button.clone().into(),
                    pressed: false,
                },
                *hold_frames,
            ),
            Self::PressButton {
                port,
                button,
                hold_frames,
            } => input_press_steps(
                InputEvent::Button {
                    port: *port,
                    name: button.clone().into(),
                    pressed: true,
                },
                InputEvent::Button {
                    port: *port,
                    name: button.clone().into(),
                    pressed: false,
                },
                *hold_frames,
            ),
        }
    }
}

fn input_press_steps(
    pressed: InputEvent,
    released: InputEvent,
    hold_frames: u32,
) -> Vec<ShellScriptStep> {
    vec![
        ShellScriptStep::Input {
            events: vec![pressed],
        },
        ShellScriptStep::RunFrames {
            frames: hold_frames,
        },
        ShellScriptStep::Input {
            events: vec![released],
        },
        ShellScriptStep::RunFrames {
            frames: STARTUP_RELEASE_OBSERVATION_FRAMES,
        },
    ]
}

/// Audio capture window. `from_frame` is counted from the boot waypoint
/// capture (i.e. after startup input + `wait_frames` have completed).
#[derive(Debug, Deserialize)]
pub struct Audio {
    pub from_frame: u32,
    pub secs: f32,
    /// Expected `xxh64:HEX` of the captured WAV bytes.
    pub hash: String,
}

/// Outcome of one catalogue entry's run.
#[derive(Debug)]
pub enum EntryOutcome {
    Pass,
    BootHashMismatch { expected: String, actual: String },
    AudioHashMismatch { expected: String, actual: String },
}

/// Captured hashes plus pass/fail outcome for one entry.
///
/// `boot_png` and `audio_wav` are populated for the catalogue CLI's
/// paste-into-manifest workflow (so a human can visually confirm the
/// hash captures what they expect). The integration test ignores them.
#[derive(Debug)]
pub struct RunResult {
    pub boot_hash: String,
    pub audio_hash: String,
    pub outcome: EntryOutcome,
    pub boot_png: Vec<u8>,
    pub audio_wav: Vec<u8>,
}

/// Outcome of one entry's snapshot fidelity check.
///
/// Returned alongside the per-entry [`RunResult`] by the Spectrum, C64 and
/// Amiga snapshot-check wrappers. `Pass` means the entry snapshotted at the boot
/// waypoint, restored into a fresh-from-firmware runtime, and reproduced both
/// the gap-end frame and the audio window byte-identically; the re-encoded
/// snapshot also matched the original.
#[derive(Debug)]
pub enum SnapshotOutcome {
    Pass,
    EncodeFailed {
        reason: String,
    },
    RestoreFailed {
        reason: String,
    },
    FrameHashDrift {
        expected: String,
        actual: String,
    },
    AudioHashDrift {
        expected: String,
        actual: String,
    },
    BytesDrift {
        original_len: usize,
        reencoded_len: usize,
        first_difference: usize,
        differing_bytes: usize,
        original_byte: Option<u8>,
        reencoded_byte: Option<u8>,
    },
}

/// Per-stage data captured during a snapshot fidelity check.
///
/// Populated incrementally as the check progresses; later fields are
/// `None` when an earlier stage failed. The integration test only
/// inspects `outcome`, but the per-stage hashes are useful when
/// debugging a drift surfaced for the first time.
#[derive(Debug)]
pub struct SnapshotCheckResult {
    pub outcome: SnapshotOutcome,
    pub encoded_len: usize,
    pub reencoded_len: Option<usize>,
    pub original_frame_hash: Option<String>,
    pub restored_frame_hash: Option<String>,
    pub original_audio_hash: Option<String>,
    pub restored_audio_hash: Option<String>,
}

#[derive(Debug, Error)]
pub enum CatalogueError {
    #[error("manifest not found: {0}")]
    ManifestNotFound(PathBuf),
    #[error("manifest parse failed: {0}")]
    ManifestParse(toml::de::Error),
    #[error("manifest is invalid: {0}")]
    ManifestInvalid(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("session error: {0}")]
    Session(String),
    #[error("system not supported by catalogue runner: {0}")]
    UnsupportedSystem(String),
    #[error("variant not supported by catalogue runner: {0}")]
    UnsupportedVariant(String),
    #[error("media kind not supported: {0}")]
    UnsupportedMediaKind(String),
    #[error("entry not found: {0}")]
    EntryNotFound(String),
    #[error(
        "{kind} routing version mismatch for system '{system}': manifest declares {found}, runtime is at {expected}. \
         The captured {kind} hashes encode pre-bump behaviour and must be re-captured before this manifest can pass. \
         See knowledge/decisions/spectrum-architecture-review.md Seam 4."
    )]
    RoutingVersionMismatch {
        kind: &'static str,
        system: String,
        expected: u32,
        found: u32,
    },
    #[error("firmware not declared for variant: {0}")]
    FirmwareNotDeclared(String),
    #[error("variant {variant} expects {expected} firmware file(s), manifest declares {actual}")]
    FirmwareCountMismatch {
        variant: String,
        expected: usize,
        actual: usize,
    },
}

/// Loads and parses one TOML manifest file.
///
/// # Errors
///
/// Returns `ManifestNotFound` when the path is missing, `ManifestParse`
/// when the TOML cannot be decoded, and `Io` for other read failures.
pub fn load_manifest(path: &Path) -> Result<Manifest, CatalogueError> {
    let text = std::fs::read_to_string(path).map_err(|err| {
        if err.kind() == std::io::ErrorKind::NotFound {
            CatalogueError::ManifestNotFound(path.to_path_buf())
        } else {
            CatalogueError::Io(err)
        }
    })?;
    parse_manifest(&text)
}

fn parse_manifest(text: &str) -> Result<Manifest, CatalogueError> {
    let manifest = toml::from_str(text).map_err(CatalogueError::ManifestParse)?;
    validate_manifest(&manifest)?;
    Ok(manifest)
}

fn validate_manifest(manifest: &Manifest) -> Result<(), CatalogueError> {
    for entry in &manifest.entry {
        validate_entry_startup(entry)?;
        validate_boot_ignore_rects(entry)?;
    }
    Ok(())
}

fn validate_boot_ignore_rects(entry: &Entry) -> Result<(), CatalogueError> {
    for (index, rect) in entry.boot.ignore_rects.iter().enumerate() {
        if rect.width == 0 || rect.height == 0 {
            return Err(CatalogueError::ManifestInvalid(format!(
                "entry '{}' boot ignore rectangle {index} must have nonzero width and height",
                entry.id
            )));
        }
        if rect.x.checked_add(rect.width).is_none() || rect.y.checked_add(rect.height).is_none() {
            return Err(CatalogueError::ManifestInvalid(format!(
                "entry '{}' boot ignore rectangle {index} overflows its coordinates",
                entry.id
            )));
        }
    }
    Ok(())
}

fn validate_entry_startup(entry: &Entry) -> Result<(), CatalogueError> {
    if !entry.script.is_empty() && !entry.startup.is_empty() {
        return Err(CatalogueError::ManifestInvalid(format!(
            "entry '{}' declares both legacy script steps and sequential startup actions",
            entry.id
        )));
    }

    let mut total_frames = 0u32;
    for (index, step) in entry.startup.iter().enumerate() {
        let (declared_frames, maximum) = match step {
            StartupStep::WaitFrames { frames } => (*frames, MAX_STARTUP_WAIT_FRAMES),
            StartupStep::PressKey {
                key, hold_frames, ..
            } => {
                validate_input_name(entry, index, "key", key)?;
                (*hold_frames, MAX_STARTUP_HOLD_FRAMES)
            }
            StartupStep::ClickMouse {
                button,
                hold_frames,
            } => {
                validate_input_name(entry, index, "mouse button", button)?;
                (*hold_frames, MAX_STARTUP_HOLD_FRAMES)
            }
            StartupStep::PressButton {
                button,
                hold_frames,
                ..
            } => {
                validate_input_name(entry, index, "controller button", button)?;
                (*hold_frames, MAX_STARTUP_HOLD_FRAMES)
            }
        };

        if declared_frames == 0 || declared_frames > maximum {
            return Err(CatalogueError::ManifestInvalid(format!(
                "entry '{}' startup action {} has frame count {declared_frames}; expected 1..={maximum}",
                entry.id, index
            )));
        }
        total_frames = total_frames.checked_add(step.frame_cost()).ok_or_else(|| {
            CatalogueError::ManifestInvalid(format!(
                "entry '{}' startup frame budget overflows",
                entry.id
            ))
        })?;
        if total_frames > MAX_STARTUP_TOTAL_FRAMES {
            return Err(CatalogueError::ManifestInvalid(format!(
                "entry '{}' startup frame budget {total_frames} exceeds {MAX_STARTUP_TOTAL_FRAMES}",
                entry.id
            )));
        }
    }

    Ok(())
}

fn validate_input_name(
    entry: &Entry,
    index: usize,
    kind: &str,
    name: &str,
) -> Result<(), CatalogueError> {
    if name.trim().is_empty() {
        return Err(CatalogueError::ManifestInvalid(format!(
            "entry '{}' startup action {index} has an empty {kind} name",
            entry.id
        )));
    }
    Ok(())
}

/// Hashes one byte slice with xxhash64 and formats as `xxh64:HEX`.
#[must_use]
pub fn hash_xxh64(bytes: &[u8]) -> String {
    let mut hasher = XxHash64::default();
    hasher.write(bytes);
    format!("xxh64:{:016x}", hasher.finish())
}

fn hash_boot_frame(
    entry: &Entry,
    rgba: &mut [u8],
    frame_width: u32,
    frame_height: u32,
) -> Result<String, CatalogueError> {
    validate_boot_ignore_rects(entry)?;

    let expected_len = u64::from(frame_width)
        .checked_mul(u64::from(frame_height))
        .and_then(|pixels| pixels.checked_mul(4))
        .and_then(|bytes| usize::try_from(bytes).ok())
        .ok_or_else(|| {
            CatalogueError::Session(format!(
                "boot frame dimensions {frame_width}x{frame_height} exceed the host address space"
            ))
        })?;
    if rgba.len() != expected_len {
        return Err(CatalogueError::Session(format!(
            "boot frame RGBA length {} does not match {frame_width}x{frame_height} ({expected_len} bytes)",
            rgba.len()
        )));
    }

    for (index, rect) in entry.boot.ignore_rects.iter().enumerate() {
        let right = rect.x.checked_add(rect.width).ok_or_else(|| {
            CatalogueError::ManifestInvalid(format!(
                "entry '{}' boot ignore rectangle {index} overflows its x coordinate",
                entry.id
            ))
        })?;
        let bottom = rect.y.checked_add(rect.height).ok_or_else(|| {
            CatalogueError::ManifestInvalid(format!(
                "entry '{}' boot ignore rectangle {index} overflows its y coordinate",
                entry.id
            ))
        })?;
        if right > frame_width || bottom > frame_height {
            return Err(CatalogueError::ManifestInvalid(format!(
                "entry '{}' boot ignore rectangle {index} at ({}, {}) size {}x{} is outside captured frame {frame_width}x{frame_height}",
                entry.id, rect.x, rect.y, rect.width, rect.height
            )));
        }

        let row_bytes = usize::try_from(u64::from(rect.width) * 4).map_err(|_| {
            CatalogueError::Session(format!(
                "entry '{}' boot ignore rectangle {index} is too wide for the host address space",
                entry.id
            ))
        })?;
        for y in rect.y..bottom {
            let first_pixel = u64::from(y) * u64::from(frame_width) + u64::from(rect.x);
            let byte_start = usize::try_from(first_pixel * 4).map_err(|_| {
                CatalogueError::Session(format!(
                    "entry '{}' boot ignore rectangle {index} offset exceeds the host address space",
                    entry.id
                ))
            })?;
            let byte_end = byte_start.checked_add(row_bytes).ok_or_else(|| {
                CatalogueError::Session(format!(
                    "entry '{}' boot ignore rectangle {index} row exceeds the host address space",
                    entry.id
                ))
            })?;
            let row = rgba.get_mut(byte_start..byte_end).ok_or_else(|| {
                CatalogueError::Session(format!(
                    "entry '{}' boot ignore rectangle {index} does not fit the RGBA buffer",
                    entry.id
                ))
            })?;
            row.fill(0);
        }
    }

    Ok(hash_xxh64(rgba))
}

/// Runs one catalogue entry against the appropriate system runtime.
///
/// `media_root` resolves `entry.media.path`; `firmware_root` resolves
/// each firmware file path declared in `manifest.system.firmware`.
///
/// # Errors
///
/// Returns `UnsupportedSystem` for systems other than `spectrum`,
/// `UnsupportedVariant` for variants not yet wired, `FirmwareNotDeclared`
/// when a variant is missing from `system.firmware`, and `Session` for
/// firmware/media/runtime failures.
/// Verifies that the routing versions declared in the manifest match
/// the runtime's current `AUDIO_ROUTING_VERSION` / `FRAME_ROUTING_VERSION`
/// for the system being run. Manifests without declared versions skip
/// the check (legacy behaviour). Mismatch returns a loud error with
/// re-capture instructions baked into the error message.
///
/// Called at the top of `run_entry` and both snapshot-check wrappers so every
/// catalogue run path enforces the same discipline.
fn verify_routing_versions(manifest: &Manifest) -> Result<(), CatalogueError> {
    match manifest.system.id.as_str() {
        "spectrum" => {
            if let Some(found) = manifest.system.audio_routing_version
                && found != common_sinclair_zx_spectrum::audio::AUDIO_ROUTING_VERSION
            {
                return Err(CatalogueError::RoutingVersionMismatch {
                    kind: "audio",
                    system: "spectrum".into(),
                    expected: common_sinclair_zx_spectrum::audio::AUDIO_ROUTING_VERSION,
                    found,
                });
            }
            if let Some(found) = manifest.system.frame_routing_version
                && found != common_sinclair_zx_spectrum::ula_engine::FRAME_ROUTING_VERSION
            {
                return Err(CatalogueError::RoutingVersionMismatch {
                    kind: "frame",
                    system: "spectrum".into(),
                    expected: common_sinclair_zx_spectrum::ula_engine::FRAME_ROUTING_VERSION,
                    found,
                });
            }
        }
        "c64" => {
            if let Some(found) = manifest.system.audio_routing_version
                && found != mos_sid_6581::AUDIO_ROUTING_VERSION
            {
                return Err(CatalogueError::RoutingVersionMismatch {
                    kind: "audio",
                    system: "c64".into(),
                    expected: mos_sid_6581::AUDIO_ROUTING_VERSION,
                    found,
                });
            }
            if let Some(found) = manifest.system.frame_routing_version
                && found != mos_vic_ii::FRAME_ROUTING_VERSION
            {
                return Err(CatalogueError::RoutingVersionMismatch {
                    kind: "frame",
                    system: "c64".into(),
                    expected: mos_vic_ii::FRAME_ROUTING_VERSION,
                    found,
                });
            }
        }
        "nes" => {
            if let Some(found) = manifest.system.audio_routing_version
                && found != ricoh_apu_2a03::AUDIO_ROUTING_VERSION
            {
                return Err(CatalogueError::RoutingVersionMismatch {
                    kind: "audio",
                    system: "nes".into(),
                    expected: ricoh_apu_2a03::AUDIO_ROUTING_VERSION,
                    found,
                });
            }
            if let Some(found) = manifest.system.frame_routing_version
                && found != ricoh_ppu_2c02::FRAME_ROUTING_VERSION
            {
                return Err(CatalogueError::RoutingVersionMismatch {
                    kind: "frame",
                    system: "nes".into(),
                    expected: ricoh_ppu_2c02::FRAME_ROUTING_VERSION,
                    found,
                });
            }
        }
        // Other systems gain their own routing-version constants as Seam 4
        // ports to them. For now they skip the check by construction.
        _ => {}
    }
    Ok(())
}

pub fn run_entry(
    manifest: &Manifest,
    entry: &Entry,
    media_root: &Path,
    firmware_root: &Path,
) -> Result<RunResult, CatalogueError> {
    verify_routing_versions(manifest)?;
    run_entry_inner(manifest, entry, media_root, firmware_root)
}

/// Capture-mode entry: drives one catalogue entry through the same
/// path as [`run_entry`] but **bypasses** `verify_routing_versions`.
///
/// Why: capture is the action that *resolves* a routing-version
/// mismatch. If `FRAME_ROUTING_VERSION` has just been bumped (because
/// the engine's rendering path changed), every captured frame hash in
/// the manifest is stale by definition. Calling `run_entry` from
/// capture-mode would fail-loud before any work happens — there'd be
/// no way to record the new ground truth. Capture must always
/// reflect the *current* code version; the version check is a
/// `run`-time invariant only.
///
/// `run`-mode (catalogue verification) keeps the strict check via
/// [`run_entry`]. Capture-mode opts out explicitly.
pub fn run_entry_for_capture(
    manifest: &Manifest,
    entry: &Entry,
    media_root: &Path,
    firmware_root: &Path,
) -> Result<RunResult, CatalogueError> {
    run_entry_inner(manifest, entry, media_root, firmware_root)
}

fn run_entry_inner(
    manifest: &Manifest,
    entry: &Entry,
    media_root: &Path,
    firmware_root: &Path,
) -> Result<RunResult, CatalogueError> {
    match manifest.system.id.as_str() {
        "spectrum" => run_spectrum_entry(manifest, entry, media_root, firmware_root),
        "nes" => run_nes_entry(entry, media_root),
        "c64" => run_c64_entry(manifest, entry, media_root, firmware_root),
        "amiga" => run_amiga_entry(manifest, entry, media_root, firmware_root),
        other => Err(CatalogueError::UnsupportedSystem(other.into())),
    }
}

/// Runs one Spectrum catalogue entry **and** proves save-state is
/// lossless on it.
///
/// The entry is driven through the same path as [`run_entry`] (boot
/// waypoint, scripted-input progression, audio-window capture). At the
/// boot waypoint the session is snapshotted; a fresh-from-firmware
/// runtime of the same variant decodes the snapshot, the bytes are
/// re-encoded, and the audio window is re-captured against the
/// restored runtime. The wrapper asserts five things, in order:
///
/// 1. Snapshot encode succeeds.
/// 2. A fresh-from-firmware runtime decodes the snapshot.
/// 3. Re-encoding the restored runtime yields bytes byte-identical
///    to the original encode.
/// 4. Restored runtime's gap-end frame hash matches original
///    (when `audio.from_frame > 0`).
/// 5. Restored runtime's audio hash matches original.
///
/// `RunResult` is the same shape `run_entry` would have returned;
/// `SnapshotCheckResult` reports the per-stage outcome and hashes.
///
/// # Errors
///
/// Returns `UnsupportedSystem` when the manifest is not Spectrum,
/// `UnsupportedVariant` for variants other than `48k`, `128k`, `plus3`,
/// `FirmwareNotDeclared` / `FirmwareCountMismatch` when the manifest
/// firmware section is incomplete, and `Session` for firmware/media/
/// runtime failures during the original run.
pub fn run_spectrum_entry_with_snapshot_check(
    manifest: &Manifest,
    entry: &Entry,
    media_root: &Path,
    firmware_root: &Path,
) -> Result<(RunResult, SnapshotCheckResult), CatalogueError> {
    if manifest.system.id != "spectrum" {
        return Err(CatalogueError::UnsupportedSystem(
            manifest.system.id.clone(),
        ));
    }
    verify_routing_versions(manifest)?;
    match entry.variant.as_str() {
        "16k" => snapshot_check_16k(manifest, entry, media_root, firmware_root),
        "48k" => snapshot_check_48k(manifest, entry, media_root, firmware_root),
        "plus" => snapshot_check_plus(manifest, entry, media_root, firmware_root),
        "128k" => snapshot_check_128k(manifest, entry, media_root, firmware_root),
        "plus2" => snapshot_check_plus2(manifest, entry, media_root, firmware_root),
        "plus2a" => snapshot_check_plus2a(manifest, entry, media_root, firmware_root),
        "plus2b" => snapshot_check_plus2b(manifest, entry, media_root, firmware_root),
        "plus3" => snapshot_check_plus3(manifest, entry, media_root, firmware_root),
        other => Err(CatalogueError::UnsupportedVariant(other.into())),
    }
}

/// Runs one C64 catalogue entry and proves its save state is lossless.
///
/// The original session follows the ordinary C64 path through KERNAL boot,
/// tape/disk autoload or cartridge startup and scripted input. At the boot
/// waypoint, the complete machine, attached drive, IEC bus and media state are
/// restored into a fresh runtime built from the same firmware and profile. The
/// wrapper then compares re-encoded bytes, the post-waypoint frame and the
/// audio window.
///
/// # Errors
///
/// Returns `UnsupportedSystem` when the manifest is not C64,
/// `UnsupportedVariant` when the profile is not wired, firmware errors for an
/// incomplete manifest, and `Session` for runtime, media or autoload failures.
pub fn run_c64_entry_with_snapshot_check(
    manifest: &Manifest,
    entry: &Entry,
    media_root: &Path,
    firmware_root: &Path,
) -> Result<(RunResult, SnapshotCheckResult), CatalogueError> {
    if manifest.system.id != "c64" {
        return Err(CatalogueError::UnsupportedSystem(
            manifest.system.id.clone(),
        ));
    }
    verify_routing_versions(manifest)?;

    let model = c64_model_from_variant(&entry.variant)
        .ok_or_else(|| CatalogueError::UnsupportedVariant(entry.variant.clone()))?;
    let (files, bytes_storage) = load_c64_firmware(manifest, entry, firmware_root)?;
    let (mut original, frames_per_sec) =
        build_c64_session(model, &entry.variant, files, &bytes_storage, "C64 runtime")?;
    prepare_c64_session(&mut original, entry, media_root)?;

    let (mut restored, restored_frames_per_sec) = build_c64_session(
        model,
        &entry.variant,
        files,
        &bytes_storage,
        "C64 fresh runtime",
    )?;
    debug_assert_eq!(frames_per_sec, restored_frames_per_sec);

    finalize_snapshot_check(&mut original, &mut restored, entry, frames_per_sec)
}

/// Runs one Amiga catalogue entry and proves its save state is lossless.
///
/// The entry follows the ordinary Amiga catalogue path through its native OCS,
/// ECS or AGA runtime. At the boot waypoint, the wrapper snapshots the complete
/// runtime, restores it into a fresh runtime built from the same firmware and
/// profile, then compares re-encoded bytes, the post-waypoint frame and the
/// audio window.
///
/// # Errors
///
/// Returns `UnsupportedSystem` when the manifest is not Amiga,
/// `UnsupportedVariant` when the profile is not wired, firmware errors for an
/// incomplete manifest, and `Session` for runtime or media failures.
pub fn run_amiga_entry_with_snapshot_check(
    manifest: &Manifest,
    entry: &Entry,
    media_root: &Path,
    firmware_root: &Path,
) -> Result<(RunResult, SnapshotCheckResult), CatalogueError> {
    if manifest.system.id != "amiga" {
        return Err(CatalogueError::UnsupportedSystem(
            manifest.system.id.clone(),
        ));
    }
    verify_routing_versions(manifest)?;

    let model = amiga_model_from_variant(&entry.variant)
        .ok_or_else(|| CatalogueError::UnsupportedVariant(entry.variant.clone()))?;
    let files = lookup_firmware(manifest, &entry.variant)?;
    if files.len() != 1 {
        return Err(CatalogueError::FirmwareCountMismatch {
            variant: entry.variant.clone(),
            expected: 1,
            actual: files.len(),
        });
    }
    let firmware_bytes = read_firmware_bytes(firmware_root, &files[0])?;

    let mut firmware_set = FirmwareSet::new();
    firmware_set.push(FirmwareImage::new(files[0].id.clone(), &firmware_bytes));

    let original_runtime = AmigaRuntimeKind::from_firmware(model, &firmware_set)
        .map_err(|err| CatalogueError::Session(format!("Amiga runtime: {err}")))?;
    let native_frame_ticks = original_runtime.native_frame_ticks();
    let frames_per_sec = amiga_frames_per_second(&original_runtime, native_frame_ticks);
    let mut original = HeadlessSession::new_with_query_provider(
        original_runtime,
        native_frame_ticks,
        AmigaSessionQueryProvider,
    );

    if let Some(media) = entry.media.as_ref() {
        let _ = load_media_spec(&mut original, media, media_root)?;
    } else {
        prepare_session_no_media(&mut original)?;
    }

    let fresh_runtime = AmigaRuntimeKind::from_firmware(model, &firmware_set)
        .map_err(|err| CatalogueError::Session(format!("Amiga fresh runtime: {err}")))?;
    let fresh_frame_ticks = fresh_runtime.native_frame_ticks();
    if fresh_frame_ticks != native_frame_ticks {
        return Err(CatalogueError::Session(format!(
            "Amiga fresh runtime frame timing changed from {native_frame_ticks} to {fresh_frame_ticks}"
        )));
    }
    let mut restored = HeadlessSession::new_with_query_provider(
        fresh_runtime,
        fresh_frame_ticks,
        AmigaSessionQueryProvider,
    );

    finalize_snapshot_check(&mut original, &mut restored, entry, frames_per_sec)
}

fn amiga_model_from_variant(variant: &str) -> Option<AmigaModel> {
    match variant {
        "a500-ocs-pal" => Some(AmigaModel::A500OcsPal),
        "a500-ocs-ntsc" => Some(AmigaModel::A500OcsNtsc),
        "a500-ocs-pal-a501" => Some(AmigaModel::A500OcsPalA501),
        "a500-ocs-ntsc-a501" => Some(AmigaModel::A500OcsNtscA501),
        "a500-plus-ecs-pal" => Some(AmigaModel::A500PlusEcsPal),
        "a500-plus-ecs-ntsc" => Some(AmigaModel::A500PlusEcsNtsc),
        "a600-ecs-pal" => Some(AmigaModel::A600EcsPal),
        "a600-ecs-ntsc" => Some(AmigaModel::A600EcsNtsc),
        "a1200-aga-pal" => Some(AmigaModel::A1200AgaPal),
        "a1200-aga-ntsc" => Some(AmigaModel::A1200AgaNtsc),
        _ => None,
    }
}

fn amiga_frames_per_second(runtime: &AmigaRuntimeKind, native_frame_ticks: u64) -> f64 {
    let clock_rate = runtime.profile().clock.rate;
    clock_rate.numerator_hz as f64 / clock_rate.denominator_hz as f64 / native_frame_ticks as f64
}

fn run_amiga_entry(
    manifest: &Manifest,
    entry: &Entry,
    media_root: &Path,
    firmware_root: &Path,
) -> Result<RunResult, CatalogueError> {
    let model = amiga_model_from_variant(&entry.variant)
        .ok_or_else(|| CatalogueError::UnsupportedVariant(entry.variant.clone()))?;

    let files = lookup_firmware(manifest, &entry.variant)?;
    if files.len() != 1 {
        return Err(CatalogueError::FirmwareCountMismatch {
            variant: entry.variant.clone(),
            expected: 1,
            actual: files.len(),
        });
    }
    let firmware_bytes = read_firmware_bytes(firmware_root, &files[0])?;

    let mut firmware_set = FirmwareSet::new();
    firmware_set.push(FirmwareImage::new(files[0].id.clone(), &firmware_bytes));

    let runtime = AmigaRuntimeKind::from_firmware(model, &firmware_set)
        .map_err(|err| CatalogueError::Session(format!("Amiga runtime: {err}")))?;
    let native_frame_ticks = runtime.native_frame_ticks();
    let frames_per_sec = amiga_frames_per_second(&runtime, native_frame_ticks);

    let mut session = HeadlessSession::new_with_query_provider(
        runtime,
        native_frame_ticks,
        AmigaSessionQueryProvider,
    );

    if let Some(media) = entry.media.as_ref() {
        let _ = load_media_spec(&mut session, media, media_root)?;
        // No autoload for Amiga: Kickstart boots itself, then auto-DMAs
        // the boot block off DF0 if a disk is inserted.
    } else {
        prepare_session_no_media(&mut session)?;
    }

    run_assertions(&mut session, entry, frames_per_sec)
}

fn run_spectrum_entry(
    manifest: &Manifest,
    entry: &Entry,
    media_root: &Path,
    firmware_root: &Path,
) -> Result<RunResult, CatalogueError> {
    match entry.variant.as_str() {
        "16k" => run_spectrum_16k_entry(manifest, entry, media_root, firmware_root),
        "48k" => run_spectrum_48k_entry(manifest, entry, media_root, firmware_root),
        "plus" => run_spectrum_plus_entry(manifest, entry, media_root, firmware_root),
        "128k" => run_spectrum_128k_entry(manifest, entry, media_root, firmware_root),
        "plus2" => run_spectrum_plus2_entry(manifest, entry, media_root, firmware_root),
        "plus2a" => run_spectrum_plus2a_entry(manifest, entry, media_root, firmware_root),
        "plus2b" => run_spectrum_plus2b_entry(manifest, entry, media_root, firmware_root),
        "plus3" => run_spectrum_plus3_entry(manifest, entry, media_root, firmware_root),
        other => Err(CatalogueError::UnsupportedVariant(other.into())),
    }
}

fn run_spectrum_16k_entry(
    manifest: &Manifest,
    entry: &Entry,
    media_root: &Path,
    firmware_root: &Path,
) -> Result<RunResult, CatalogueError> {
    let files = lookup_firmware(manifest, &entry.variant)?;
    if files.len() != 1 {
        return Err(CatalogueError::FirmwareCountMismatch {
            variant: entry.variant.clone(),
            expected: 1,
            actual: files.len(),
        });
    }
    let rom_bytes = read_firmware_bytes(firmware_root, &files[0])?;

    let mut firmware_set = FirmwareSet::new();
    firmware_set.push(FirmwareImage::new(files[0].id.clone(), &rom_bytes));

    let runtime = Spectrum16kRuntime::from_firmware(&firmware_set)
        .map_err(|err| CatalogueError::Session(format!("16K runtime: {err}")))?;

    let mut session = HeadlessSession::new_with_query_provider(
        runtime,
        u64::from(TIMING_48K.halfcycles_per_frame),
        SpectrumSessionQueryProvider,
    );

    let media = entry
        .media
        .as_ref()
        .ok_or_else(|| CatalogueError::Session("16K entry requires media".into()))?;
    let media_kind = load_media_spec(&mut session, media, media_root)?;

    autoload_basic_tape(&mut session, &media.slot, DEFAULT_TAPE_AUTOLOAD_BOOT_FRAMES)
        .map_err(|err| CatalogueError::Session(format!("16K autoload: {err}")))?;

    wait_for_tape_stop(&mut session, media_kind, "tape.playing")?;
    run_assertions(&mut session, entry, spectrum_frames_per_sec(&TIMING_48K))
}

fn run_spectrum_plus_entry(
    manifest: &Manifest,
    entry: &Entry,
    media_root: &Path,
    firmware_root: &Path,
) -> Result<RunResult, CatalogueError> {
    let files = lookup_firmware(manifest, &entry.variant)?;
    if files.len() != 1 {
        return Err(CatalogueError::FirmwareCountMismatch {
            variant: entry.variant.clone(),
            expected: 1,
            actual: files.len(),
        });
    }
    let rom_bytes = read_firmware_bytes(firmware_root, &files[0])?;

    let mut firmware_set = FirmwareSet::new();
    firmware_set.push(FirmwareImage::new(files[0].id.clone(), &rom_bytes));

    let runtime = SpectrumPlusRuntime::from_firmware(&firmware_set)
        .map_err(|err| CatalogueError::Session(format!("Spectrum+ runtime: {err}")))?;

    let mut session = HeadlessSession::new_with_query_provider(
        runtime,
        u64::from(TIMING_48K.halfcycles_per_frame),
        SpectrumSessionQueryProvider,
    );

    let media = entry
        .media
        .as_ref()
        .ok_or_else(|| CatalogueError::Session("Spectrum+ entry requires media".into()))?;
    let media_kind = load_media_spec(&mut session, media, media_root)?;

    autoload_basic_tape(&mut session, &media.slot, DEFAULT_TAPE_AUTOLOAD_BOOT_FRAMES)
        .map_err(|err| CatalogueError::Session(format!("Spectrum+ autoload: {err}")))?;

    wait_for_tape_stop(&mut session, media_kind, "tape.playing")?;
    run_assertions(&mut session, entry, spectrum_frames_per_sec(&TIMING_48K))
}

fn run_spectrum_48k_entry(
    manifest: &Manifest,
    entry: &Entry,
    media_root: &Path,
    firmware_root: &Path,
) -> Result<RunResult, CatalogueError> {
    let files = lookup_firmware(manifest, &entry.variant)?;
    if files.len() != 1 {
        return Err(CatalogueError::FirmwareCountMismatch {
            variant: entry.variant.clone(),
            expected: 1,
            actual: files.len(),
        });
    }
    let rom_bytes = read_firmware_bytes(firmware_root, &files[0])?;

    let mut firmware_set = FirmwareSet::new();
    firmware_set.push(FirmwareImage::new(files[0].id.clone(), &rom_bytes));

    let runtime = Spectrum48kRuntime::from_firmware(&firmware_set)
        .map_err(|err| CatalogueError::Session(format!("48K runtime: {err}")))?;

    let mut session = HeadlessSession::new_with_query_provider(
        runtime,
        u64::from(TIMING_48K.halfcycles_per_frame),
        SpectrumSessionQueryProvider,
    );

    let media = entry
        .media
        .as_ref()
        .ok_or_else(|| CatalogueError::Session("48K entry requires media".into()))?;
    let media_kind = load_media_spec(&mut session, media, media_root)?;

    autoload_basic_tape(&mut session, &media.slot, DEFAULT_TAPE_AUTOLOAD_BOOT_FRAMES)
        .map_err(|err| CatalogueError::Session(format!("48K autoload: {err}")))?;

    wait_for_tape_stop(&mut session, media_kind, "tape.playing")?;
    run_assertions(&mut session, entry, spectrum_frames_per_sec(&TIMING_48K))
}

fn run_spectrum_plus2_entry(
    manifest: &Manifest,
    entry: &Entry,
    media_root: &Path,
    firmware_root: &Path,
) -> Result<RunResult, CatalogueError> {
    let files = lookup_firmware(manifest, &entry.variant)?;
    if files.len() != 2 {
        return Err(CatalogueError::FirmwareCountMismatch {
            variant: entry.variant.clone(),
            expected: 2,
            actual: files.len(),
        });
    }
    let rom0 = read_firmware_bytes(firmware_root, &files[0])?;
    let rom1 = read_firmware_bytes(firmware_root, &files[1])?;

    let runtime = build_plus2_runtime(&rom0, &rom1);

    let mut session = HeadlessSession::new_with_query_provider(
        runtime,
        u64::from(TIMING_128K.halfcycles_per_frame),
        SpectrumSessionQueryProvider,
    );

    let media = entry
        .media
        .as_ref()
        .ok_or_else(|| CatalogueError::Session("+2 entry requires media".into()))?;
    let media_kind = load_media_spec(&mut session, media, media_root)?;

    autoload_128k_tape_loader(&mut session, &media.slot, DEFAULT_128K_BOOT_FRAMES)?;

    wait_for_tape_stop(&mut session, media_kind, "tape.playing")?;
    run_assertions(&mut session, entry, spectrum_frames_per_sec(&TIMING_128K))
}

fn run_spectrum_128k_entry(
    manifest: &Manifest,
    entry: &Entry,
    media_root: &Path,
    firmware_root: &Path,
) -> Result<RunResult, CatalogueError> {
    let files = lookup_firmware(manifest, &entry.variant)?;
    if files.len() != 2 {
        return Err(CatalogueError::FirmwareCountMismatch {
            variant: entry.variant.clone(),
            expected: 2,
            actual: files.len(),
        });
    }
    let rom0 = read_firmware_bytes(firmware_root, &files[0])?;
    let rom1 = read_firmware_bytes(firmware_root, &files[1])?;

    let mut machine = Spectrum128K::new();
    machine.memory.load_roms(&rom0, &rom1);
    let runtime = Spectrum128kRuntime::new(Model::Spectrum128KPal, machine);

    let mut session = HeadlessSession::new_with_query_provider(
        runtime,
        u64::from(TIMING_128K.halfcycles_per_frame),
        SpectrumSessionQueryProvider,
    );

    let media = entry
        .media
        .as_ref()
        .ok_or_else(|| CatalogueError::Session("128K entry requires media".into()))?;
    let media_kind = load_media_spec(&mut session, media, media_root)?;

    autoload_128k_tape_loader(&mut session, &media.slot, DEFAULT_128K_BOOT_FRAMES)?;

    wait_for_tape_stop(&mut session, media_kind, "tape.playing")?;

    // Diagnostic: dump full 64K RAM image at end-of-tape (immediately
    // after the tape transport reports stopped, before the boot wait).
    // Set `EMU198X_DUMP_RAM_EOT=PATH` to enable.
    if let Ok(path) = std::env::var("EMU198X_DUMP_RAM_EOT") {
        let m = session.machine().machine();
        let mut buf = Vec::with_capacity(65536);
        for addr in 0..=0xFFFFu32 {
            buf.push(m.memory.read(addr as u16));
        }
        std::fs::write(&path, &buf)
            .map_err(|err| CatalogueError::Session(format!("dump RAM: {err}")))?;
        let z80 = &m.z80;
        eprintln!(
            "[EOT] PC={:04X} I={:02X} IM={} IFF1={} IFF2={} rom={} bank={} → {}",
            z80.regs.pc,
            z80.regs.i,
            z80.regs.im,
            z80.regs.iff1 as u8,
            z80.regs.iff2 as u8,
            m.memory.current_rom(),
            m.memory.current_bank(),
            path,
        );
    }

    let result = run_assertions(&mut session, entry, spectrum_frames_per_sec(&TIMING_128K))?;

    // Optional: dump a memory window to a file at the audio-capture
    // end-point. Set `EMU198X_DUMP_MEM=START:END:PATH` (hex addrs,
    // inclusive-exclusive); e.g. `fe00:ff00:/tmp/dump.bin`.
    if let Ok(spec) = std::env::var("EMU198X_DUMP_MEM") {
        dump_memory_window(&session, &spec)?;
    }

    Ok(result)
}

fn dump_memory_window(
    session: &HeadlessSession<Spectrum128kRuntime, SpectrumSessionQueryProvider>,
    spec: &str,
) -> Result<(), CatalogueError> {
    let parts: Vec<&str> = spec.split(':').collect();
    if parts.len() != 3 {
        return Err(CatalogueError::Session(format!(
            "EMU198X_DUMP_MEM expects START:END:PATH, got `{spec}`"
        )));
    }
    let start = u16::from_str_radix(parts[0], 16)
        .map_err(|err| CatalogueError::Session(format!("dump start: {err}")))?;
    let end = u16::from_str_radix(parts[1], 16)
        .map_err(|err| CatalogueError::Session(format!("dump end: {err}")))?;
    let m = session.machine().machine();
    let mut buf = Vec::with_capacity(usize::from(end - start));
    for addr in start..end {
        buf.push(m.memory.read(addr));
    }
    std::fs::write(parts[2], &buf)
        .map_err(|err| CatalogueError::Session(format!("dump write: {err}")))?;
    eprintln!(
        "[DUMP] {} bytes from {:04X}..{:04X} → {} (rom={} bank={} screen=bank{})",
        buf.len(),
        start,
        end,
        parts[2],
        m.memory.current_rom(),
        m.memory.current_bank(),
        m.memory.screen_bank(),
    );
    // Z80 IM-2 vector context: the IRQ handler address is read as a
    // little-endian word from `(I*0x100 | 0xFF, +1)` — every Spectrum
    // game's IM-2 setup places a 257-byte table at `I*0x100` filled
    // with one byte X, so the handler lives at `(X*0x101) & 0xFFFF`.
    let z80 = &m.z80;
    let i = z80.regs.i;
    let vector_addr_lo = (u16::from(i) << 8) | 0xFF;
    let vector_addr_hi = vector_addr_lo.wrapping_add(1);
    let vec_lo = m.memory.read(vector_addr_lo);
    let vec_hi = m.memory.read(vector_addr_hi);
    let handler = u16::from(vec_lo) | (u16::from(vec_hi) << 8);
    eprintln!(
        "[Z80] PC={:04X} I={:02X} IM={} IFF1={} IFF2={} | IM2 vector @ {:04X}={:02X} @ {:04X}={:02X} → handler {:04X}",
        z80.regs.pc,
        i,
        z80.regs.im,
        z80.regs.iff1 as u8,
        z80.regs.iff2 as u8,
        vector_addr_lo,
        vec_lo,
        vector_addr_hi,
        vec_hi,
        handler,
    );
    let sp = z80.regs.sp;
    let mut stack = String::new();
    for n in 0..16 {
        let lo = m.memory.read(sp.wrapping_add(n * 2));
        let hi = m.memory.read(sp.wrapping_add(n * 2 + 1));
        stack.push_str(&format!(" {:04X}", u16::from(lo) | (u16::from(hi) << 8)));
    }
    eprintln!("[STK] SP={:04X} top→bottom (16 words):{}", sp, stack);
    Ok(())
}

fn run_nes_entry(entry: &Entry, media_root: &Path) -> Result<RunResult, CatalogueError> {
    if entry.variant != "ntsc" {
        return Err(CatalogueError::UnsupportedVariant(entry.variant.clone()));
    }

    let runtime = NesRuntime::blank(NesModel::NesNtsc);

    let mut session = HeadlessSession::new_with_query_provider(
        runtime,
        NES_NTSC_FRAME_TICKS,
        NesSessionQueryProvider,
    );

    let media = entry
        .media
        .as_ref()
        .ok_or_else(|| CatalogueError::Session("NES entry requires cartridge media".into()))?;
    let _ = load_media_spec(&mut session, media, media_root)?;
    // No autoload: NES cartridge code runs from frame 0.
    // No tape-stop wait: cartridge boots instantly.

    run_assertions(&mut session, entry, NES_NTSC_FRAMES_PER_SEC)
}

fn run_c64_entry(
    manifest: &Manifest,
    entry: &Entry,
    media_root: &Path,
    firmware_root: &Path,
) -> Result<RunResult, CatalogueError> {
    let model = c64_model_from_variant(&entry.variant)
        .ok_or_else(|| CatalogueError::UnsupportedVariant(entry.variant.clone()))?;
    let (files, bytes_storage) = load_c64_firmware(manifest, entry, firmware_root)?;
    let (mut session, frames_per_sec) =
        build_c64_session(model, &entry.variant, files, &bytes_storage, "C64 runtime")?;
    prepare_c64_session(&mut session, entry, media_root)?;
    run_assertions(&mut session, entry, frames_per_sec)
}

type C64CatalogueSession = HeadlessSession<C64Runtime, C64SessionQueryProvider>;

fn c64_model_from_variant(variant: &str) -> Option<C64Model> {
    match variant {
        // These drive-qualified variants are electrically the PAL breadbin.
        // Their firmware sets substitute a different drive DOS ROM.
        "pal" | "pal-1581" | "pal-1571" => Some(C64Model::C64PalBreadbin),
        "ntsc" => Some(C64Model::C64NtscBreadbin),
        _ => None,
    }
}

fn load_c64_firmware<'manifest>(
    manifest: &'manifest Manifest,
    entry: &Entry,
    firmware_root: &Path,
) -> Result<(&'manifest [FirmwareSpec], Vec<Vec<u8>>), CatalogueError> {
    let files = lookup_firmware(manifest, &entry.variant)?;
    // KERNAL, BASIC and CHARGEN are mandatory. A disk entry adds one drive DOS
    // ROM selected by its variant.
    if !(3..=4).contains(&files.len()) {
        return Err(CatalogueError::FirmwareCountMismatch {
            variant: entry.variant.clone(),
            expected: 3,
            actual: files.len(),
        });
    }
    let bytes_storage = files
        .iter()
        .map(|spec| read_firmware_bytes(firmware_root, spec))
        .collect::<Result<_, _>>()?;
    Ok((files, bytes_storage))
}

fn build_c64_session(
    model: C64Model,
    variant: &str,
    files: &[FirmwareSpec],
    bytes_storage: &[Vec<u8>],
    error_context: &str,
) -> Result<(C64CatalogueSession, f64), CatalogueError> {
    let mut firmware_set = FirmwareSet::new();
    for (spec, bytes) in files.iter().zip(bytes_storage) {
        firmware_set.push(FirmwareImage::new(spec.id.clone(), bytes));
    }

    let mut runtime = C64Runtime::from_firmware(model, &firmware_set)
        .map_err(|err| CatalogueError::Session(format!("{error_context}: {err}")))?;

    // The `pal-1571` firmware set carries the 1571 DOS ROM but no 1541, so
    // device 8 is empty after construction. Put a 1571 on device 8 so both the
    // original and restored sessions have the same physical drive topology.
    if variant == "pal-1571" {
        runtime
            .set_port_drive(8, Some(C64DriveKind::C1571))
            .map_err(|err| CatalogueError::Session(format!("C64 1571 attach: {err}")))?;
    }

    let timing = match model {
        C64Model::C64PalBreadbin | C64Model::C64cPal => &TIMING_PAL_BREADBIN,
        C64Model::C64NtscBreadbin | C64Model::C64cNtsc => &TIMING_NTSC_BREADBIN,
    };
    let frames_per_sec = timing.cpu_hz as f64 / f64::from(timing.cycles_per_frame);
    let session = HeadlessSession::new_with_query_provider(
        runtime,
        u64::from(timing.cycles_per_frame),
        C64SessionQueryProvider,
    );
    Ok((session, frames_per_sec))
}

fn prepare_c64_session(
    session: &mut C64CatalogueSession,
    entry: &Entry,
    media_root: &Path,
) -> Result<(), CatalogueError> {
    if let Some(media) = entry.media.as_ref() {
        let media_kind = load_media_spec(session, media, media_root)?;
        match media_kind {
            MediaKind::Disk => {
                // The 1581 sits on IEC device 9 (slot `drive-9`) and needs
                // LOAD"*",9,1; the 1541 on device 8 (`drive-8`) uses ,8,1.
                // Route by slot so a D81 entry drives the 1581.
                if media.slot == DEFAULT_DISK_1581_AUTOLOAD_SLOT {
                    autoload_basic_disk_1581(
                        session,
                        &media.slot,
                        DEFAULT_C64_BOOT_FRAMES,
                        DEFAULT_C64_DISK_PROMPT_FRAMES,
                    )
                    .map_err(|err| {
                        CatalogueError::Session(format!("C64 1581 disk autoload: {err}"))
                    })?;
                } else {
                    autoload_basic_disk(
                        session,
                        &media.slot,
                        DEFAULT_C64_BOOT_FRAMES,
                        DEFAULT_C64_DISK_PROMPT_FRAMES,
                    )
                    .map_err(|err| CatalogueError::Session(format!("C64 disk autoload: {err}")))?;
                }
                // Match the existing disk_autoload regression tests:
                // wait dynamically for "LOADING" text after SEARCHING
                // FOR. The catalogue script's at_frame is then "frames
                // after LOADING appears" rather than "after SEARCHING
                // FOR".
                session
                    .wait_for_query_text_contains("screen.text.lines", "LOADING", 1_500)
                    .map_err(|err| CatalogueError::Session(format!("C64 LOADING wait: {err}")))?;
            }
            MediaKind::Tape => {
                if media.play_phase_cycles > 0 {
                    session
                        .machine_mut()
                        .machine_mut()
                        .set_tape_play_phase_cycles(media.play_phase_cycles);
                }
                c64_autoload_basic_tape(
                    session,
                    &media.slot,
                    DEFAULT_C64_BOOT_FRAMES,
                    DEFAULT_C64_DISK_PROMPT_FRAMES,
                )
                .map_err(|err| CatalogueError::Session(format!("C64 tape autoload: {err}")))?;
                // No tape-stop wait: per the existing tape_autoload
                // regression tests, the C64 tape autoload returns
                // after PRESS PLAY ON TAPE has been simulated, and the
                // game's bootloader handles the rest. wait_frames in
                // the entry covers the full load-to-menu duration.
            }
            MediaKind::Cartridge => {
                // Cartridges autostart from power-on (EasyFlash boots its
                // menu in Ultimax before the KERNAL even runs); the entry's
                // wait_frames covers the whole boot.
            }
            other => {
                return Err(CatalogueError::UnsupportedMediaKind(format!(
                    "c64: {other:?}"
                )));
            }
        }
    } else {
        prepare_session_no_media(session)?;
        session
            .wait_for_boot(DEFAULT_C64_BOOT_FRAMES)
            .map_err(|err| CatalogueError::Session(format!("C64 boot wait: {err}")))?;
    }
    Ok(())
}

fn wait_for_tape_stop<M, Q>(
    session: &mut HeadlessSession<M, Q>,
    media_kind: MediaKind,
    tape_query_path: &str,
) -> Result<(), CatalogueError>
where
    M: MachineCore,
    Q: SessionQueryProvider<M>,
{
    if media_kind == MediaKind::Tape {
        session
            .wait_for_query_bool(tape_query_path, false, MAX_TAPE_LOAD_FRAMES)
            .map_err(|err| CatalogueError::Session(format!("tape stop: {err}")))?;
    }
    Ok(())
}

fn run_spectrum_plus2a_entry(
    manifest: &Manifest,
    entry: &Entry,
    media_root: &Path,
    firmware_root: &Path,
) -> Result<RunResult, CatalogueError> {
    run_spectrum_amstrad_class_entry(
        manifest,
        entry,
        media_root,
        firmware_root,
        build_plus2a_runtime,
        "+2A",
    )
}

fn run_spectrum_plus2b_entry(
    manifest: &Manifest,
    entry: &Entry,
    media_root: &Path,
    firmware_root: &Path,
) -> Result<RunResult, CatalogueError> {
    run_spectrum_amstrad_class_entry(
        manifest,
        entry,
        media_root,
        firmware_root,
        build_plus2b_runtime,
        "+2B",
    )
}

/// Shared entry runner for the disk-less Amstrad-class variants
/// (+2A, +2B). Loads four ROMs, mounts tape media, drives the Amstrad
/// boot menu's ENTER-for-Loader autoload, then waits for the tape to
/// stop. The +3 keeps its own runner because the disk path is
/// title-specific (some auto-run, some wait for ENTER) and stalls at
/// the disk Loader screen anyway.
fn run_spectrum_amstrad_class_entry<R, B>(
    manifest: &Manifest,
    entry: &Entry,
    media_root: &Path,
    firmware_root: &Path,
    build_runtime: B,
    variant_label: &str,
) -> Result<RunResult, CatalogueError>
where
    R: emu198x_shell::MachineCore,
    B: Fn(&[u8], &[u8], &[u8], &[u8]) -> R,
    SpectrumSessionQueryProvider: SessionQueryProvider<R>,
{
    let files = lookup_firmware(manifest, &entry.variant)?;
    if files.len() != 4 {
        return Err(CatalogueError::FirmwareCountMismatch {
            variant: entry.variant.clone(),
            expected: 4,
            actual: files.len(),
        });
    }
    let r0 = read_firmware_bytes(firmware_root, &files[0])?;
    let r1 = read_firmware_bytes(firmware_root, &files[1])?;
    let r2 = read_firmware_bytes(firmware_root, &files[2])?;
    let r3 = read_firmware_bytes(firmware_root, &files[3])?;

    let runtime = build_runtime(&r0, &r1, &r2, &r3);

    let mut session = HeadlessSession::new_with_query_provider(
        runtime,
        u64::from(TIMING_PLUS2A.halfcycles_per_frame),
        SpectrumSessionQueryProvider,
    );

    let media = entry
        .media
        .as_ref()
        .ok_or_else(|| CatalogueError::Session(format!("{variant_label} entry requires media")))?;
    let media_kind = load_media_spec(&mut session, media, media_root)?;

    autoload_128k_tape_loader(&mut session, &media.slot, DEFAULT_128K_BOOT_FRAMES)?;
    wait_for_tape_stop(&mut session, media_kind, "tape.playing")?;

    run_assertions(&mut session, entry, spectrum_frames_per_sec(&TIMING_PLUS2A))
}

fn run_spectrum_plus3_entry(
    manifest: &Manifest,
    entry: &Entry,
    media_root: &Path,
    firmware_root: &Path,
) -> Result<RunResult, CatalogueError> {
    let files = lookup_firmware(manifest, &entry.variant)?;
    if files.len() != 4 {
        return Err(CatalogueError::FirmwareCountMismatch {
            variant: entry.variant.clone(),
            expected: 4,
            actual: files.len(),
        });
    }
    let r0 = read_firmware_bytes(firmware_root, &files[0])?;
    let r1 = read_firmware_bytes(firmware_root, &files[1])?;
    let r2 = read_firmware_bytes(firmware_root, &files[2])?;
    let r3 = read_firmware_bytes(firmware_root, &files[3])?;

    let mut machine = SpectrumPlus3::new();
    machine.memory.load_roms(&r0, &r1, &r2, &r3);
    let runtime = SpectrumPlus3Runtime::new(Model::SpectrumPlus3, machine);

    let mut session = HeadlessSession::new_with_query_provider(
        runtime,
        u64::from(TIMING_PLUS2A.halfcycles_per_frame),
        SpectrumSessionQueryProvider,
    );

    let media = entry
        .media
        .as_ref()
        .ok_or_else(|| CatalogueError::Session("+3 entry requires media".into()))?;
    let media_kind = load_media_spec(&mut session, media, media_root)?;

    match media_kind {
        MediaKind::Disk => {
            // The +3 BIOS boots to an interactive menu (Loader / +3
            // BASIC / Calculator / 48 BASIC) and does NOT auto-run
            // even with a disk in the drive. `autoload_plus3_loader`
            // waits for the menu, presses ENTER (which selects the
            // highlighted Loader option), and lets the disk loader
            // take over — `wait_frames` in the entry then covers the
            // rest of the load.
            autoload_plus3_loader(&mut session, DEFAULT_128K_BOOT_FRAMES)?;
        }
        MediaKind::Tape => {
            wait_for_tape_stop(&mut session, media_kind, "tape.playing")?;
        }
        _ => {}
    }

    run_assertions(&mut session, entry, spectrum_frames_per_sec(&TIMING_PLUS2A))
}

/// +3 autoload: wait for the +3 menu boot banner, press ENTER (selects
/// the highlighted "Loader" option which auto-runs the disk's first
/// program). The disk must already be inserted via load_media_spec.
fn autoload_plus3_loader<Q>(
    session: &mut HeadlessSession<SpectrumPlus3Runtime, Q>,
    max_boot_frames: u32,
) -> Result<(), CatalogueError>
where
    Q: SessionQueryProvider<SpectrumPlus3Runtime>,
{
    session
        .wait_for_boot(max_boot_frames)
        .map_err(|err| CatalogueError::Session(format!("+3 boot wait: {err}")))?;

    // Give the +3 menu extra time to fully paint after boot detection
    // before pressing ENTER. boot.detected fires on the Amstrad
    // copyright text appearing, but the menu input handler may not
    // be ready for several more frames.
    session
        .run_frames(50)
        .map_err(|err| CatalogueError::Session(format!("+3 menu settle: {err}")))?;

    session.queue_input(InputEvent::Key {
        name: "enter".into(),
        pressed: true,
    });
    session
        .run_frames(5)
        .map_err(|err| CatalogueError::Session(format!("+3 enter press: {err}")))?;
    session.queue_input(InputEvent::Key {
        name: "enter".into(),
        pressed: false,
    });
    session
        .run_frames(20)
        .map_err(|err| CatalogueError::Session(format!("+3 enter settle: {err}")))?;
    Ok(())
}

/// 128K-class equivalent of the 48K `autoload_basic_tape`. Waits for
/// the boot menu, presses ENTER (which selects the highlighted "Tape
/// Loader" option), and starts tape transport. Generic over the
/// concrete 128K-class machine so it works for both `Spectrum128K` and
/// `SpectrumPlus2`.
fn autoload_128k_tape_loader<M, Q>(
    session: &mut HeadlessSession<M, Q>,
    slot: &str,
    max_boot_frames: u32,
) -> Result<(), CatalogueError>
where
    M: MachineCore,
    Q: SessionQueryProvider<M>,
{
    session
        .wait_for_boot(max_boot_frames)
        .map_err(|err| CatalogueError::Session(format!("128K boot wait: {err}")))?;

    session.queue_input(InputEvent::Key {
        name: "enter".into(),
        pressed: true,
    });
    session
        .run_frames(2)
        .map_err(|err| CatalogueError::Session(format!("128K enter press: {err}")))?;
    session.queue_input(InputEvent::Key {
        name: "enter".into(),
        pressed: false,
    });
    session
        .run_frames(10)
        .map_err(|err| CatalogueError::Session(format!("128K enter settle: {err}")))?;

    session
        .command(&ControlCommand::MediaTransport(MediaTransportCommand::new(
            slot.to_owned(),
            MediaTransportAction::Start,
        )))
        .map_err(|err| CatalogueError::Session(format!("128K tape start: {err}")))?;

    Ok(())
}

/// Media-loading shared across systems. Pushes one media spec into the
/// session and returns the resolved `MediaKind`. The session must be
/// `prepare`d separately if no media is loaded.
fn load_media_spec<M, Q>(
    session: &mut HeadlessSession<M, Q>,
    media: &Media,
    media_root: &Path,
) -> Result<MediaKind, CatalogueError>
where
    M: MachineCore,
    Q: SessionQueryProvider<M>,
{
    let media_path = media_root.join(&media.path);
    let media_kind = parse_media_kind(&media.kind)?;
    let media_loaded = read_media_asset(&media_path, media_kind)
        .map_err(|err| CatalogueError::Session(format!("media {media_path:?}: {err}")))?;

    let mut media_set = MediaSet::new();
    media_set.push(
        MediaImage::new(media.slot.clone(), media_kind, &media_loaded.bytes)
            .writable(media.writable),
    );

    session
        .prepare(&media_set, &[])
        .map_err(|err| CatalogueError::Session(format!("prepare: {err}")))?;

    Ok(media_kind)
}

/// Prepares the session with no media. Some catalogue entries (e.g.
/// C64 boot-to-READY) test the firmware path alone.
fn prepare_session_no_media<M, Q>(session: &mut HeadlessSession<M, Q>) -> Result<(), CatalogueError>
where
    M: MachineCore,
    Q: SessionQueryProvider<M>,
{
    let media_set = MediaSet::new();
    session
        .prepare(&media_set, &[])
        .map_err(|err| CatalogueError::Session(format!("prepare (no media): {err}")))
}

/// Generic assertion runner. Once the per-system/variant setup has the
/// session loaded and at the start of the input phase (tape stopped,
/// cartridge running, READY shown, etc.), this advances the timeline:
///
///   1. Run legacy `script[]` steps or bounded, sequential `startup[]`
///      actions. Lets disk-loaded titles type RUN and multi-stage loaders
///      advance through guest prompts.
///   2. Wait `boot.wait_frames` more frames.
///   3. Capture boot frame.
///   4. Wait `audio.from_frame` more frames.
///   5. Capture `audio.secs`-second audio window.
///
/// Putting input before the boot capture means disk-game titles can
/// land on their real post-RUN title screen as the boot waypoint
/// rather than the (boring) post-LOAD READY prompt.
fn run_assertions<M, Q>(
    session: &mut HeadlessSession<M, Q>,
    entry: &Entry,
    frames_per_sec: f64,
) -> Result<RunResult, CatalogueError>
where
    M: MachineCore,
    Q: SessionQueryProvider<M>,
{
    let (boot_hash, boot_png) = run_script_then_capture_boot_frame(session, entry)?;
    let (audio_hash, audio_wav, _gap_end_frame_hash) =
        capture_audio_window(session, entry, frames_per_sec)?;
    Ok(build_run_result(
        entry, boot_hash, audio_hash, boot_png, audio_wav,
    ))
}

fn build_run_result(
    entry: &Entry,
    boot_hash: String,
    audio_hash: String,
    boot_png: Vec<u8>,
    audio_wav: Vec<u8>,
) -> RunResult {
    let outcome = if boot_hash != entry.boot.frame_hash {
        EntryOutcome::BootHashMismatch {
            expected: entry.boot.frame_hash.clone(),
            actual: boot_hash.clone(),
        }
    } else if audio_hash != entry.audio.hash {
        EntryOutcome::AudioHashMismatch {
            expected: entry.audio.hash.clone(),
            actual: audio_hash.clone(),
        }
    } else {
        EntryOutcome::Pass
    };

    RunResult {
        boot_hash,
        audio_hash,
        outcome,
        boot_png,
        audio_wav,
    }
}

/// Runs the guest-input phase, the post-input `boot.wait_frames` advance,
/// and captures the boot waypoint frame. After this returns the session
/// is sitting at the boot waypoint with the latest frame populated.
fn run_script_then_capture_boot_frame<M, Q>(
    session: &mut HeadlessSession<M, Q>,
    entry: &Entry,
) -> Result<(String, Vec<u8>), CatalogueError>
where
    M: MachineCore,
    Q: SessionQueryProvider<M>,
{
    validate_entry_startup(entry)?;
    if entry.startup.is_empty() {
        run_legacy_script(session, entry)?;
    } else {
        run_startup_sequence(session, entry)?;
    }

    session
        .run_frames(entry.boot.wait_frames)
        .map_err(|err| CatalogueError::Session(format!("boot wait: {err}")))?;

    let boot_frame = session
        .latest_frame()
        .ok_or_else(|| CatalogueError::Session("no frame at boot waypoint".into()))?;
    let mut boot_rgba = boot_frame
        .rgba_pixels()
        .map_err(|err| CatalogueError::Session(format!("rgba: {err}")))?;
    let boot_hash = hash_boot_frame(entry, &mut boot_rgba, boot_frame.width, boot_frame.height)?;
    let boot_png = boot_frame
        .png_bytes()
        .map_err(|err| CatalogueError::Session(format!("boot png: {err}")))?;

    Ok((boot_hash, boot_png))
}

fn run_legacy_script<M, Q>(
    session: &mut HeadlessSession<M, Q>,
    entry: &Entry,
) -> Result<(), CatalogueError>
where
    M: MachineCore,
    Q: SessionQueryProvider<M>,
{
    // Per-step timing matches runtime-commodore-c64::tests::common::press_key:
    // queue press → run 3 frames → queue release. The release event fires
    // on the next `run_frames` (next step's advance or the boot wait).
    let mut frames_consumed: u32 = 0;
    for step in &entry.script {
        let at_frame = step.at_frame();
        if at_frame > frames_consumed {
            let advance = at_frame - frames_consumed;
            session
                .run_frames(advance)
                .map_err(|err| CatalogueError::Session(format!("script advance: {err}")))?;
            frames_consumed = at_frame;
        }
        match step {
            ScriptStep::Press { press, .. } => {
                session.queue_input(InputEvent::Key {
                    name: press.clone().into(),
                    pressed: true,
                });
                session
                    .run_frames(3)
                    .map_err(|err| CatalogueError::Session(format!("press: {err}")))?;
                session.queue_input(InputEvent::Key {
                    name: press.clone().into(),
                    pressed: false,
                });
            }
            ScriptStep::Click { click, .. } => {
                session.queue_input(InputEvent::PointerButton {
                    device: "mouse-1".into(),
                    button: click.clone().into(),
                    pressed: true,
                });
                session
                    .run_frames(3)
                    .map_err(|err| CatalogueError::Session(format!("click: {err}")))?;
                session.queue_input(InputEvent::PointerButton {
                    device: "mouse-1".into(),
                    button: click.clone().into(),
                    pressed: false,
                });
            }
            ScriptStep::Button { port, button, .. } => {
                session.queue_input(InputEvent::Button {
                    port: *port,
                    name: button.clone().into(),
                    pressed: true,
                });
                session
                    .run_frames(3)
                    .map_err(|err| CatalogueError::Session(format!("button: {err}")))?;
                session.queue_input(InputEvent::Button {
                    port: *port,
                    name: button.clone().into(),
                    pressed: false,
                });
            }
        }
        frames_consumed = frames_consumed.saturating_add(3);
    }

    Ok(())
}

fn run_startup_sequence<M, Q>(
    session: &mut HeadlessSession<M, Q>,
    entry: &Entry,
) -> Result<(), CatalogueError>
where
    M: MachineCore,
    Q: SessionQueryProvider<M>,
{
    for (index, action) in entry.startup.iter().enumerate() {
        for step in action.shell_steps() {
            step.execute(session).map_err(|err| {
                CatalogueError::Session(format!(
                    "startup action {index} for entry '{}': {err}",
                    entry.id
                ))
            })?;
        }
    }

    Ok(())
}

/// Runs the post-waypoint `audio.from_frame` advance, then captures the
/// `audio.secs` audio window. Returns `(audio_hash, audio_wav,
/// gap_end_frame_hash)` — the gap-end frame hash is the rgba hash of
/// the last frame emitted during the gap-advance, or `None` when
/// `audio.from_frame == 0` (no gap to anchor against).
fn capture_audio_window<M, Q>(
    session: &mut HeadlessSession<M, Q>,
    entry: &Entry,
    frames_per_sec: f64,
) -> Result<(String, Vec<u8>, Option<String>), CatalogueError>
where
    M: MachineCore,
    Q: SessionQueryProvider<M>,
{
    let gap_end_frame_hash = if entry.audio.from_frame > 0 {
        session
            .run_frames(entry.audio.from_frame)
            .map_err(|err| CatalogueError::Session(format!("audio gap: {err}")))?;
        let frame = session
            .latest_frame()
            .ok_or_else(|| CatalogueError::Session("no frame at gap end".into()))?;
        let rgba = frame
            .rgba_pixels()
            .map_err(|err| CatalogueError::Session(format!("gap rgba: {err}")))?;
        Some(hash_xxh64(&rgba))
    } else {
        None
    };

    session.clear_audio_capture();

    let audio_frames = (f64::from(entry.audio.secs) * frames_per_sec).round() as u32;
    session
        .run_frames(audio_frames)
        .map_err(|err| CatalogueError::Session(format!("audio run: {err}")))?;

    let audio_wav = session
        .audio_wav_bytes()
        .map_err(|err| CatalogueError::Session(format!("audio: {err}")))?;
    let audio_hash = hash_xxh64(&audio_wav);

    Ok((audio_hash, audio_wav, gap_end_frame_hash))
}

fn snapshot_check_16k(
    manifest: &Manifest,
    entry: &Entry,
    media_root: &Path,
    firmware_root: &Path,
) -> Result<(RunResult, SnapshotCheckResult), CatalogueError> {
    let files = lookup_firmware(manifest, &entry.variant)?;
    if files.len() != 1 {
        return Err(CatalogueError::FirmwareCountMismatch {
            variant: entry.variant.clone(),
            expected: 1,
            actual: files.len(),
        });
    }
    let rom_bytes = read_firmware_bytes(firmware_root, &files[0])?;

    let mut firmware_set = FirmwareSet::new();
    firmware_set.push(FirmwareImage::new(files[0].id.clone(), &rom_bytes));

    let original_runtime = Spectrum16kRuntime::from_firmware(&firmware_set)
        .map_err(|err| CatalogueError::Session(format!("16K runtime: {err}")))?;
    let mut original = HeadlessSession::new_with_query_provider(
        original_runtime,
        u64::from(TIMING_48K.halfcycles_per_frame),
        SpectrumSessionQueryProvider,
    );

    let media = entry
        .media
        .as_ref()
        .ok_or_else(|| CatalogueError::Session("16K entry requires media".into()))?;
    let media_kind = load_media_spec(&mut original, media, media_root)?;

    autoload_basic_tape(
        &mut original,
        &media.slot,
        DEFAULT_TAPE_AUTOLOAD_BOOT_FRAMES,
    )
    .map_err(|err| CatalogueError::Session(format!("16K autoload: {err}")))?;
    wait_for_tape_stop(&mut original, media_kind, "tape.playing")?;

    let fresh_runtime = Spectrum16kRuntime::from_firmware(&firmware_set)
        .map_err(|err| CatalogueError::Session(format!("16K fresh runtime: {err}")))?;
    let mut restored = HeadlessSession::new_with_query_provider(
        fresh_runtime,
        u64::from(TIMING_48K.halfcycles_per_frame),
        SpectrumSessionQueryProvider,
    );

    finalize_snapshot_check(
        &mut original,
        &mut restored,
        entry,
        spectrum_frames_per_sec(&TIMING_48K),
    )
}

fn snapshot_check_plus(
    manifest: &Manifest,
    entry: &Entry,
    media_root: &Path,
    firmware_root: &Path,
) -> Result<(RunResult, SnapshotCheckResult), CatalogueError> {
    let files = lookup_firmware(manifest, &entry.variant)?;
    if files.len() != 1 {
        return Err(CatalogueError::FirmwareCountMismatch {
            variant: entry.variant.clone(),
            expected: 1,
            actual: files.len(),
        });
    }
    let rom_bytes = read_firmware_bytes(firmware_root, &files[0])?;

    let mut firmware_set = FirmwareSet::new();
    firmware_set.push(FirmwareImage::new(files[0].id.clone(), &rom_bytes));

    let original_runtime = SpectrumPlusRuntime::from_firmware(&firmware_set)
        .map_err(|err| CatalogueError::Session(format!("Spectrum+ runtime: {err}")))?;
    let mut original = HeadlessSession::new_with_query_provider(
        original_runtime,
        u64::from(TIMING_48K.halfcycles_per_frame),
        SpectrumSessionQueryProvider,
    );

    let media = entry
        .media
        .as_ref()
        .ok_or_else(|| CatalogueError::Session("Spectrum+ entry requires media".into()))?;
    let media_kind = load_media_spec(&mut original, media, media_root)?;

    autoload_basic_tape(
        &mut original,
        &media.slot,
        DEFAULT_TAPE_AUTOLOAD_BOOT_FRAMES,
    )
    .map_err(|err| CatalogueError::Session(format!("Spectrum+ autoload: {err}")))?;
    wait_for_tape_stop(&mut original, media_kind, "tape.playing")?;

    let fresh_runtime = SpectrumPlusRuntime::from_firmware(&firmware_set)
        .map_err(|err| CatalogueError::Session(format!("Spectrum+ fresh runtime: {err}")))?;
    let mut restored = HeadlessSession::new_with_query_provider(
        fresh_runtime,
        u64::from(TIMING_48K.halfcycles_per_frame),
        SpectrumSessionQueryProvider,
    );

    finalize_snapshot_check(
        &mut original,
        &mut restored,
        entry,
        spectrum_frames_per_sec(&TIMING_48K),
    )
}

fn snapshot_check_48k(
    manifest: &Manifest,
    entry: &Entry,
    media_root: &Path,
    firmware_root: &Path,
) -> Result<(RunResult, SnapshotCheckResult), CatalogueError> {
    let files = lookup_firmware(manifest, &entry.variant)?;
    if files.len() != 1 {
        return Err(CatalogueError::FirmwareCountMismatch {
            variant: entry.variant.clone(),
            expected: 1,
            actual: files.len(),
        });
    }
    let rom_bytes = read_firmware_bytes(firmware_root, &files[0])?;

    let mut firmware_set = FirmwareSet::new();
    firmware_set.push(FirmwareImage::new(files[0].id.clone(), &rom_bytes));

    let original_runtime = Spectrum48kRuntime::from_firmware(&firmware_set)
        .map_err(|err| CatalogueError::Session(format!("48K runtime: {err}")))?;
    let mut original = HeadlessSession::new_with_query_provider(
        original_runtime,
        u64::from(TIMING_48K.halfcycles_per_frame),
        SpectrumSessionQueryProvider,
    );

    let media = entry
        .media
        .as_ref()
        .ok_or_else(|| CatalogueError::Session("48K entry requires media".into()))?;
    let media_kind = load_media_spec(&mut original, media, media_root)?;

    autoload_basic_tape(
        &mut original,
        &media.slot,
        DEFAULT_TAPE_AUTOLOAD_BOOT_FRAMES,
    )
    .map_err(|err| CatalogueError::Session(format!("48K autoload: {err}")))?;
    wait_for_tape_stop(&mut original, media_kind, "tape.playing")?;

    let fresh_runtime = Spectrum48kRuntime::from_firmware(&firmware_set)
        .map_err(|err| CatalogueError::Session(format!("48K fresh runtime: {err}")))?;
    let mut restored = HeadlessSession::new_with_query_provider(
        fresh_runtime,
        u64::from(TIMING_48K.halfcycles_per_frame),
        SpectrumSessionQueryProvider,
    );

    finalize_snapshot_check(
        &mut original,
        &mut restored,
        entry,
        spectrum_frames_per_sec(&TIMING_48K),
    )
}

fn snapshot_check_plus2(
    manifest: &Manifest,
    entry: &Entry,
    media_root: &Path,
    firmware_root: &Path,
) -> Result<(RunResult, SnapshotCheckResult), CatalogueError> {
    let files = lookup_firmware(manifest, &entry.variant)?;
    if files.len() != 2 {
        return Err(CatalogueError::FirmwareCountMismatch {
            variant: entry.variant.clone(),
            expected: 2,
            actual: files.len(),
        });
    }
    let rom0 = read_firmware_bytes(firmware_root, &files[0])?;
    let rom1 = read_firmware_bytes(firmware_root, &files[1])?;

    let original_runtime = build_plus2_runtime(&rom0, &rom1);
    let mut original = HeadlessSession::new_with_query_provider(
        original_runtime,
        u64::from(TIMING_128K.halfcycles_per_frame),
        SpectrumSessionQueryProvider,
    );

    let media = entry
        .media
        .as_ref()
        .ok_or_else(|| CatalogueError::Session("+2 entry requires media".into()))?;
    let media_kind = load_media_spec(&mut original, media, media_root)?;

    autoload_128k_tape_loader(&mut original, &media.slot, DEFAULT_128K_BOOT_FRAMES)?;
    wait_for_tape_stop(&mut original, media_kind, "tape.playing")?;

    let fresh_runtime = build_plus2_runtime(&rom0, &rom1);
    let mut restored = HeadlessSession::new_with_query_provider(
        fresh_runtime,
        u64::from(TIMING_128K.halfcycles_per_frame),
        SpectrumSessionQueryProvider,
    );

    finalize_snapshot_check(
        &mut original,
        &mut restored,
        entry,
        spectrum_frames_per_sec(&TIMING_128K),
    )
}

fn snapshot_check_128k(
    manifest: &Manifest,
    entry: &Entry,
    media_root: &Path,
    firmware_root: &Path,
) -> Result<(RunResult, SnapshotCheckResult), CatalogueError> {
    let files = lookup_firmware(manifest, &entry.variant)?;
    if files.len() != 2 {
        return Err(CatalogueError::FirmwareCountMismatch {
            variant: entry.variant.clone(),
            expected: 2,
            actual: files.len(),
        });
    }
    let rom0 = read_firmware_bytes(firmware_root, &files[0])?;
    let rom1 = read_firmware_bytes(firmware_root, &files[1])?;

    let original_runtime = build_128k_runtime(&rom0, &rom1);
    let mut original = HeadlessSession::new_with_query_provider(
        original_runtime,
        u64::from(TIMING_128K.halfcycles_per_frame),
        SpectrumSessionQueryProvider,
    );

    let media = entry
        .media
        .as_ref()
        .ok_or_else(|| CatalogueError::Session("128K entry requires media".into()))?;
    let media_kind = load_media_spec(&mut original, media, media_root)?;

    autoload_128k_tape_loader(&mut original, &media.slot, DEFAULT_128K_BOOT_FRAMES)?;
    wait_for_tape_stop(&mut original, media_kind, "tape.playing")?;

    let fresh_runtime = build_128k_runtime(&rom0, &rom1);
    let mut restored = HeadlessSession::new_with_query_provider(
        fresh_runtime,
        u64::from(TIMING_128K.halfcycles_per_frame),
        SpectrumSessionQueryProvider,
    );

    finalize_snapshot_check(
        &mut original,
        &mut restored,
        entry,
        spectrum_frames_per_sec(&TIMING_128K),
    )
}

fn snapshot_check_plus2a(
    manifest: &Manifest,
    entry: &Entry,
    media_root: &Path,
    firmware_root: &Path,
) -> Result<(RunResult, SnapshotCheckResult), CatalogueError> {
    snapshot_check_amstrad_class(
        manifest,
        entry,
        media_root,
        firmware_root,
        build_plus2a_runtime,
        "+2A",
    )
}

fn snapshot_check_plus2b(
    manifest: &Manifest,
    entry: &Entry,
    media_root: &Path,
    firmware_root: &Path,
) -> Result<(RunResult, SnapshotCheckResult), CatalogueError> {
    snapshot_check_amstrad_class(
        manifest,
        entry,
        media_root,
        firmware_root,
        build_plus2b_runtime,
        "+2B",
    )
}

fn snapshot_check_amstrad_class<R, B>(
    manifest: &Manifest,
    entry: &Entry,
    media_root: &Path,
    firmware_root: &Path,
    build_runtime: B,
    variant_label: &str,
) -> Result<(RunResult, SnapshotCheckResult), CatalogueError>
where
    R: emu198x_shell::MachineCore,
    B: Fn(&[u8], &[u8], &[u8], &[u8]) -> R,
    SpectrumSessionQueryProvider: SessionQueryProvider<R>,
{
    let files = lookup_firmware(manifest, &entry.variant)?;
    if files.len() != 4 {
        return Err(CatalogueError::FirmwareCountMismatch {
            variant: entry.variant.clone(),
            expected: 4,
            actual: files.len(),
        });
    }
    let r0 = read_firmware_bytes(firmware_root, &files[0])?;
    let r1 = read_firmware_bytes(firmware_root, &files[1])?;
    let r2 = read_firmware_bytes(firmware_root, &files[2])?;
    let r3 = read_firmware_bytes(firmware_root, &files[3])?;

    let original_runtime = build_runtime(&r0, &r1, &r2, &r3);
    let mut original = HeadlessSession::new_with_query_provider(
        original_runtime,
        u64::from(TIMING_PLUS2A.halfcycles_per_frame),
        SpectrumSessionQueryProvider,
    );

    let media = entry
        .media
        .as_ref()
        .ok_or_else(|| CatalogueError::Session(format!("{variant_label} entry requires media")))?;
    let media_kind = load_media_spec(&mut original, media, media_root)?;

    autoload_128k_tape_loader(&mut original, &media.slot, DEFAULT_128K_BOOT_FRAMES)?;
    wait_for_tape_stop(&mut original, media_kind, "tape.playing")?;

    let fresh_runtime = build_runtime(&r0, &r1, &r2, &r3);
    let mut restored = HeadlessSession::new_with_query_provider(
        fresh_runtime,
        u64::from(TIMING_PLUS2A.halfcycles_per_frame),
        SpectrumSessionQueryProvider,
    );

    finalize_snapshot_check(
        &mut original,
        &mut restored,
        entry,
        spectrum_frames_per_sec(&TIMING_PLUS2A),
    )
}

fn snapshot_check_plus3(
    manifest: &Manifest,
    entry: &Entry,
    media_root: &Path,
    firmware_root: &Path,
) -> Result<(RunResult, SnapshotCheckResult), CatalogueError> {
    let files = lookup_firmware(manifest, &entry.variant)?;
    if files.len() != 4 {
        return Err(CatalogueError::FirmwareCountMismatch {
            variant: entry.variant.clone(),
            expected: 4,
            actual: files.len(),
        });
    }
    let r0 = read_firmware_bytes(firmware_root, &files[0])?;
    let r1 = read_firmware_bytes(firmware_root, &files[1])?;
    let r2 = read_firmware_bytes(firmware_root, &files[2])?;
    let r3 = read_firmware_bytes(firmware_root, &files[3])?;

    let original_runtime = build_plus3_runtime(&r0, &r1, &r2, &r3);
    let mut original = HeadlessSession::new_with_query_provider(
        original_runtime,
        u64::from(TIMING_PLUS2A.halfcycles_per_frame),
        SpectrumSessionQueryProvider,
    );

    let media = entry
        .media
        .as_ref()
        .ok_or_else(|| CatalogueError::Session("+3 entry requires media".into()))?;
    let media_kind = load_media_spec(&mut original, media, media_root)?;

    // Mirror `run_spectrum_plus3_entry`: tape entries wait for the
    // bootloader to stop tape transport, disk entries press ENTER on
    // the Loader menu to start the +3 BIOS's disk handover. Skipping
    // the autoload here would leave every +3 disk entry sitting on
    // the boot menu (same framebuffer hash for every title).
    match media_kind {
        MediaKind::Disk => {
            autoload_plus3_loader(&mut original, DEFAULT_128K_BOOT_FRAMES)?;
        }
        MediaKind::Tape => {
            wait_for_tape_stop(&mut original, media_kind, "tape.playing")?;
        }
        _ => {}
    }

    let fresh_runtime = build_plus3_runtime(&r0, &r1, &r2, &r3);
    let mut restored = HeadlessSession::new_with_query_provider(
        fresh_runtime,
        u64::from(TIMING_PLUS2A.halfcycles_per_frame),
        SpectrumSessionQueryProvider,
    );

    finalize_snapshot_check(
        &mut original,
        &mut restored,
        entry,
        spectrum_frames_per_sec(&TIMING_PLUS2A),
    )
}

fn build_128k_runtime(rom0: &[u8], rom1: &[u8]) -> Spectrum128kRuntime {
    let mut machine = Spectrum128K::new();
    machine.memory.load_roms(rom0, rom1);
    Spectrum128kRuntime::new(Model::Spectrum128KPal, machine)
}

fn build_plus2_runtime(rom0: &[u8], rom1: &[u8]) -> SpectrumPlus2Runtime {
    let mut machine = SpectrumPlus2::new();
    machine.memory.load_roms(rom0, rom1);
    SpectrumPlus2Runtime::new(Model::SpectrumPlus2, machine)
}

fn build_plus2a_runtime(r0: &[u8], r1: &[u8], r2: &[u8], r3: &[u8]) -> SpectrumPlus2ARuntime {
    let mut machine = SpectrumPlus2A::new();
    machine.memory.load_roms(r0, r1, r2, r3);
    SpectrumPlus2ARuntime::new(Model::SpectrumPlus2A, machine)
}

fn build_plus2b_runtime(r0: &[u8], r1: &[u8], r2: &[u8], r3: &[u8]) -> SpectrumPlus2BRuntime {
    let mut machine = SpectrumPlus2B::new();
    machine.memory.load_roms(r0, r1, r2, r3);
    SpectrumPlus2BRuntime::new(Model::SpectrumPlus2B, machine)
}

fn build_plus3_runtime(r0: &[u8], r1: &[u8], r2: &[u8], r3: &[u8]) -> SpectrumPlus3Runtime {
    let mut machine = SpectrumPlus3::new();
    machine.memory.load_roms(r0, r1, r2, r3);
    SpectrumPlus3Runtime::new(Model::SpectrumPlus3, machine)
}

/// Final stage of a snapshot fidelity check, shared across variants.
///
/// `original` is sitting at the post-setup phase (tape stopped, +3 disk
/// inserted, etc.) and is about to run the script + boot wait. `restored`
/// is a freshly-built session for the same variant with no media. The
/// caller has confirmed both sessions are paired with the same timing.
fn finalize_snapshot_check<M, Q>(
    original: &mut HeadlessSession<M, Q>,
    restored: &mut HeadlessSession<M, Q>,
    entry: &Entry,
    frames_per_sec: f64,
) -> Result<(RunResult, SnapshotCheckResult), CatalogueError>
where
    M: MachineCore,
    Q: SessionQueryProvider<M>,
{
    let (boot_hash, boot_png) = run_script_then_capture_boot_frame(original, entry)?;

    let original_bytes = match original.snapshot_bytes() {
        Ok(bytes) => bytes,
        Err(err) => {
            // Finish the original audio capture so the RunResult is honest
            // about what the original entry produced.
            let (audio_hash, audio_wav, _) = capture_audio_window(original, entry, frames_per_sec)?;
            return Ok((
                build_run_result(entry, boot_hash, audio_hash, boot_png, audio_wav),
                SnapshotCheckResult {
                    outcome: SnapshotOutcome::EncodeFailed {
                        reason: err.to_string(),
                    },
                    encoded_len: 0,
                    reencoded_len: None,
                    original_frame_hash: None,
                    restored_frame_hash: None,
                    original_audio_hash: None,
                    restored_audio_hash: None,
                },
            ));
        }
    };
    let encoded_len = original_bytes.len();

    if let Err(err) = restored.restore_snapshot(&original_bytes) {
        let (audio_hash, audio_wav, _) = capture_audio_window(original, entry, frames_per_sec)?;
        return Ok((
            build_run_result(entry, boot_hash, audio_hash, boot_png, audio_wav),
            SnapshotCheckResult {
                outcome: SnapshotOutcome::RestoreFailed {
                    reason: err.to_string(),
                },
                encoded_len,
                reencoded_len: None,
                original_frame_hash: None,
                restored_frame_hash: None,
                original_audio_hash: None,
                restored_audio_hash: None,
            },
        ));
    }

    let reencoded_bytes = match restored.snapshot_bytes() {
        Ok(bytes) => bytes,
        Err(err) => {
            let (audio_hash, audio_wav, _) = capture_audio_window(original, entry, frames_per_sec)?;
            return Ok((
                build_run_result(entry, boot_hash, audio_hash, boot_png, audio_wav),
                SnapshotCheckResult {
                    outcome: SnapshotOutcome::EncodeFailed {
                        reason: format!("re-encode: {err}"),
                    },
                    encoded_len,
                    reencoded_len: None,
                    original_frame_hash: None,
                    restored_frame_hash: None,
                    original_audio_hash: None,
                    restored_audio_hash: None,
                },
            ));
        }
    };
    let reencoded_len = reencoded_bytes.len();

    let (orig_audio_hash, orig_audio_wav, orig_gap_hash) =
        capture_audio_window(original, entry, frames_per_sec)?;
    let (rest_audio_hash, _rest_audio_wav, rest_gap_hash) =
        capture_audio_window(restored, entry, frames_per_sec)?;

    let run_result = build_run_result(
        entry,
        boot_hash,
        orig_audio_hash.clone(),
        boot_png,
        orig_audio_wav,
    );

    let outcome = if original_bytes != reencoded_bytes {
        let first_difference = original_bytes
            .iter()
            .zip(&reencoded_bytes)
            .position(|(original, reencoded)| original != reencoded)
            .unwrap_or_else(|| original_bytes.len().min(reencoded_bytes.len()));
        let differing_bytes = original_bytes
            .iter()
            .zip(&reencoded_bytes)
            .filter(|(original, reencoded)| original != reencoded)
            .count()
            + original_bytes.len().abs_diff(reencoded_bytes.len());
        SnapshotOutcome::BytesDrift {
            original_len: encoded_len,
            reencoded_len,
            first_difference,
            differing_bytes,
            original_byte: original_bytes.get(first_difference).copied(),
            reencoded_byte: reencoded_bytes.get(first_difference).copied(),
        }
    } else if orig_gap_hash != rest_gap_hash {
        SnapshotOutcome::FrameHashDrift {
            expected: orig_gap_hash.clone().unwrap_or_else(|| "<none>".into()),
            actual: rest_gap_hash.clone().unwrap_or_else(|| "<none>".into()),
        }
    } else if orig_audio_hash != rest_audio_hash {
        SnapshotOutcome::AudioHashDrift {
            expected: orig_audio_hash.clone(),
            actual: rest_audio_hash.clone(),
        }
    } else {
        SnapshotOutcome::Pass
    };

    Ok((
        run_result,
        SnapshotCheckResult {
            outcome,
            encoded_len,
            reencoded_len: Some(reencoded_len),
            original_frame_hash: orig_gap_hash,
            restored_frame_hash: rest_gap_hash,
            original_audio_hash: Some(orig_audio_hash),
            restored_audio_hash: Some(rest_audio_hash),
        },
    ))
}

fn lookup_firmware<'m>(
    manifest: &'m Manifest,
    variant: &str,
) -> Result<&'m [FirmwareSpec], CatalogueError> {
    manifest
        .system
        .firmware
        .iter()
        .find(|vf| vf.variant == variant)
        .map(|vf| vf.files.as_slice())
        .ok_or_else(|| CatalogueError::FirmwareNotDeclared(variant.into()))
}

fn read_firmware_bytes(
    firmware_root: &Path,
    spec: &FirmwareSpec,
) -> Result<Vec<u8>, CatalogueError> {
    let path = firmware_root.join(&spec.path);
    let loaded = read_firmware_asset(&path)
        .map_err(|err| CatalogueError::Session(format!("firmware {path:?}: {err}")))?;
    Ok(loaded.bytes)
}

fn parse_media_kind(kind: &str) -> Result<MediaKind, CatalogueError> {
    match kind {
        "tape" => Ok(MediaKind::Tape),
        "disk" => Ok(MediaKind::Disk),
        "cartridge" => Ok(MediaKind::Cartridge),
        "optical" => Ok(MediaKind::Optical),
        "program" => Ok(MediaKind::Program),
        "snapshot" => Ok(MediaKind::Snapshot),
        other => Err(CatalogueError::UnsupportedMediaKind(other.into())),
    }
}

fn spectrum_frames_per_sec(timing: &common_sinclair_zx_spectrum::timing::FrameTiming) -> f64 {
    (timing.master_hz as f64) / f64::from(timing.halfcycles_per_frame)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest_with_entry_steps(steps: &str) -> String {
        format!(
            r#"
[system]
id = "amiga"

[[entry]]
id = "startup-test"
title = "Startup test"
year = 1988
publisher = "Test"
variant = "a500-ocs-pal-a501"

{steps}

[entry.boot]
wait_frames = 1
frame_hash = "xxh64:0000000000000000"

[entry.audio]
from_frame = 1
secs = 0.02
hash = "xxh64:0000000000000000"
"#
        )
    }

    #[test]
    fn hash_xxh64_format_is_stable() {
        let h = hash_xxh64(b"hello world");
        assert!(h.starts_with("xxh64:"), "got: {h}");
        assert_eq!(h.len(), "xxh64:".len() + 16);
        assert_eq!(h, hash_xxh64(b"hello world"));
    }

    fn manifest_with_boot_ignore_rects(rects: &str) -> String {
        manifest_with_entry_steps("").replacen(
            "frame_hash = \"xxh64:0000000000000000\"",
            &format!("frame_hash = \"xxh64:0000000000000000\"\nignore_rects = [{rects}]"),
            1,
        )
    }

    #[test]
    fn boot_ignore_rectangles_are_optional_and_parse_with_pixel_geometry() {
        let default_manifest =
            parse_manifest(&manifest_with_entry_steps("")).expect("default manifest should parse");
        assert!(default_manifest.entry[0].boot.ignore_rects.is_empty());

        let mut rgba = vec![0x5a; 3 * 2 * 4];
        let expected_hash = hash_xxh64(&rgba);
        let actual_hash = hash_boot_frame(&default_manifest.entry[0], &mut rgba, 3, 2)
            .expect("an unmasked frame should hash");
        assert_eq!(actual_hash, expected_hash);

        let manifest = parse_manifest(&manifest_with_boot_ignore_rects(
            "{ x = 12, y = 34, width = 56, height = 7 }",
        ))
        .expect("well-formed rectangle should parse");
        assert_eq!(
            manifest.entry[0].boot.ignore_rects,
            [BootIgnoreRect {
                x: 12,
                y: 34,
                width: 56,
                height: 7,
            }]
        );
    }

    #[test]
    fn catalogue_media_is_read_only_unless_the_entry_opts_in() {
        let base = manifest_with_entry_steps("").replacen(
            "[entry.boot]",
            "[entry.media]\nkind = \"disk\"\nslot = \"floppy-0\"\npath = \"archive.adf\"\n\n[entry.boot]",
            1,
        );
        let default_manifest = parse_manifest(&base).expect("default media should parse");
        assert!(
            !default_manifest.entry[0]
                .media
                .as_ref()
                .expect("media should exist")
                .writable
        );

        let writable = base.replacen(
            "path = \"archive.adf\"",
            "path = \"archive.adf\"\nwritable = true",
            1,
        );
        let writable_manifest = parse_manifest(&writable).expect("writable media should parse");
        assert!(
            writable_manifest.entry[0]
                .media
                .as_ref()
                .expect("media should exist")
                .writable
        );
    }

    #[test]
    fn boot_ignore_rectangle_zeroes_only_selected_rgba_pixels_before_hashing() {
        let manifest = parse_manifest(&manifest_with_boot_ignore_rects(
            "{ x = 1, y = 0, width = 1, height = 2 }",
        ))
        .expect("rectangle should parse");
        let entry = &manifest.entry[0];
        let mut rgba: Vec<u8> = (1..=24).collect();
        let mut expected = rgba.clone();
        expected[4..8].fill(0);
        expected[16..20].fill(0);

        let actual_hash = hash_boot_frame(entry, &mut rgba, 3, 2).expect("mask should fit");

        assert_eq!(rgba, expected);
        assert_eq!(actual_hash, hash_xxh64(&expected));
    }

    #[test]
    fn manifest_rejects_zero_sized_or_overflowing_boot_ignore_rectangles() {
        for rect in [
            "{ x = 0, y = 0, width = 0, height = 1 }",
            "{ x = 0, y = 0, width = 1, height = 0 }",
            "{ x = 4294967295, y = 0, width = 1, height = 1 }",
        ] {
            let err = parse_manifest(&manifest_with_boot_ignore_rects(rect))
                .expect_err("invalid rectangle must fail");
            assert!(matches!(err, CatalogueError::ManifestInvalid(_)), "{err}");
        }
    }

    #[test]
    fn manifest_rejects_malformed_boot_ignore_rectangles() {
        for rect in [
            "{ x = 0, y = 0, width = 1 }",
            "{ x = 0, y = 0, width = 1, height = 1, right = 1 }",
        ] {
            let err = parse_manifest(&manifest_with_boot_ignore_rects(rect))
                .expect_err("malformed rectangle must fail");
            assert!(matches!(err, CatalogueError::ManifestParse(_)), "{err}");
        }
    }

    #[test]
    fn boot_ignore_rectangle_must_fit_the_captured_frame() {
        let manifest = parse_manifest(&manifest_with_boot_ignore_rects(
            "{ x = 2, y = 0, width = 2, height = 1 }",
        ))
        .expect("bounds depend on captured dimensions");
        let mut rgba = vec![0xff; 3 * 2 * 4];

        let err = hash_boot_frame(&manifest.entry[0], &mut rgba, 3, 2)
            .expect_err("out-of-bounds rectangle must fail");

        assert!(matches!(err, CatalogueError::ManifestInvalid(_)));
        assert!(
            err.to_string().contains("outside captured frame 3x2"),
            "{err}"
        );
    }

    #[test]
    fn startup_actions_parse_as_strictly_tagged_sequential_steps() {
        let text = manifest_with_entry_steps(
            r#"
[[entry.startup]]
action = "wait_frames"
frames = 6500

[[entry.startup]]
action = "press_key"
key = "return"

[[entry.startup]]
action = "click_mouse"
button = "left"
hold_frames = 4

[[entry.startup]]
action = "press_button"
port = 2
button = "fire"
"#,
        );

        let manifest = parse_manifest(&text).expect("startup manifest should parse");
        assert_eq!(
            manifest.entry[0].startup,
            [
                StartupStep::WaitFrames { frames: 6500 },
                StartupStep::PressKey {
                    key: "return".into(),
                    hold_frames: 3,
                },
                StartupStep::ClickMouse {
                    button: "left".into(),
                    hold_frames: 4,
                },
                StartupStep::PressButton {
                    port: 2,
                    button: "fire".into(),
                    hold_frames: 3,
                },
            ]
        );
    }

    #[test]
    fn startup_actions_expand_only_to_shared_time_and_input_steps() {
        let action = StartupStep::ClickMouse {
            button: "left".into(),
            hold_frames: 3,
        };

        assert_eq!(
            action.shell_steps(),
            [
                ShellScriptStep::Input {
                    events: vec![InputEvent::PointerButton {
                        device: "mouse-1".into(),
                        button: "left".into(),
                        pressed: true,
                    }],
                },
                ShellScriptStep::RunFrames { frames: 3 },
                ShellScriptStep::Input {
                    events: vec![InputEvent::PointerButton {
                        device: "mouse-1".into(),
                        button: "left".into(),
                        pressed: false,
                    }],
                },
                ShellScriptStep::RunFrames { frames: 1 },
            ]
        );
    }

    #[test]
    fn adjacent_startup_inputs_observe_release_before_the_next_press() {
        let action = StartupStep::PressKey {
            key: "return".into(),
            hold_frames: 3,
        };
        let steps = [action.clone(), action]
            .iter()
            .flat_map(StartupStep::shell_steps)
            .collect::<Vec<_>>();

        assert!(matches!(
            &steps[2],
            ShellScriptStep::Input {
                events
            } if events == &[InputEvent::Key {
                name: "return".into(),
                pressed: false,
            }]
        ));
        assert_eq!(steps[3], ShellScriptStep::RunFrames { frames: 1 });
        assert!(matches!(
            &steps[4],
            ShellScriptStep::Input {
                events
            } if events == &[InputEvent::Key {
                name: "return".into(),
                pressed: true,
            }]
        ));
    }

    #[test]
    fn legacy_script_entries_remain_valid() {
        let text = manifest_with_entry_steps(
            r#"
[[entry.script]]
at_frame = 6500
click = "left"
"#,
        );

        let manifest = parse_manifest(&text).expect("legacy manifest should parse");
        assert_eq!(manifest.entry[0].script.len(), 1);
        assert!(manifest.entry[0].startup.is_empty());
    }

    #[test]
    fn manifest_rejects_mixed_legacy_and_startup_navigation() {
        let text = manifest_with_entry_steps(
            r#"
[[entry.script]]
at_frame = 6500
click = "left"

[[entry.startup]]
action = "wait_frames"
frames = 6500
"#,
        );

        let err = parse_manifest(&text).expect_err("mixed navigation forms must fail");
        assert!(matches!(err, CatalogueError::ManifestInvalid(_)));
        assert!(err.to_string().contains("declares both"), "{err}");
    }

    #[test]
    fn manifest_rejects_zero_or_excessive_startup_frame_counts() {
        for steps in [
            r#"
[[entry.startup]]
action = "wait_frames"
frames = 0
"#,
            r#"
[[entry.startup]]
action = "press_key"
key = "return"
hold_frames = 121
"#,
        ] {
            let err = parse_manifest(&manifest_with_entry_steps(steps))
                .expect_err("out-of-range startup frames must fail");
            assert!(matches!(err, CatalogueError::ManifestInvalid(_)));
        }
    }

    #[test]
    fn manifest_rejects_unknown_startup_fields() {
        let text = manifest_with_entry_steps(
            r#"
[[entry.startup]]
action = "click_mouse"
button = "left"
patch_guest_memory = true
"#,
        );

        let err = parse_manifest(&text).expect_err("unknown startup fields must fail");
        assert!(matches!(err, CatalogueError::ManifestParse(_)));
    }

    #[test]
    fn amiga_variant_names_cover_the_supported_catalogue_profiles() {
        let expected = [
            ("a500-ocs-pal", AmigaModel::A500OcsPal),
            ("a500-ocs-ntsc", AmigaModel::A500OcsNtsc),
            ("a500-ocs-pal-a501", AmigaModel::A500OcsPalA501),
            ("a500-ocs-ntsc-a501", AmigaModel::A500OcsNtscA501),
            ("a500-plus-ecs-pal", AmigaModel::A500PlusEcsPal),
            ("a500-plus-ecs-ntsc", AmigaModel::A500PlusEcsNtsc),
            ("a600-ecs-pal", AmigaModel::A600EcsPal),
            ("a600-ecs-ntsc", AmigaModel::A600EcsNtsc),
            ("a1200-aga-pal", AmigaModel::A1200AgaPal),
            ("a1200-aga-ntsc", AmigaModel::A1200AgaNtsc),
        ];

        for (variant, model) in expected {
            assert_eq!(amiga_model_from_variant(variant), Some(model), "{variant}");
        }
        assert_eq!(amiga_model_from_variant("a4000-aga-pal"), None);
    }

    #[test]
    fn amiga_catalogue_profiles_dispatch_to_their_native_chipset_tiers() {
        let ocs = AmigaRuntimeKind::blank(AmigaModel::A500OcsPal);
        let ecs = AmigaRuntimeKind::blank(AmigaModel::A600EcsPal);
        let aga = AmigaRuntimeKind::blank(AmigaModel::A1200AgaPal);

        assert!(!ocs.is_ecs());
        assert!(!ocs.is_aga());
        assert!(ecs.is_ecs());
        assert!(!ecs.is_aga());
        assert!(!aga.is_ecs());
        assert!(aga.is_aga());

        let pal_ticks = aga.native_frame_ticks();
        let ntsc_ticks = AmigaRuntimeKind::blank(AmigaModel::A1200AgaNtsc).native_frame_ticks();
        assert_ne!(pal_ticks, ntsc_ticks);
    }

    #[test]
    fn amiga_new_catalogue_profiles_construct_from_firmware() {
        let firmware_bytes = vec![0; 512 * 1024];
        let mut firmware = FirmwareSet::new();
        firmware.push(FirmwareImage::new(
            "commodore-amiga-kickstart-rom",
            &firmware_bytes,
        ));

        for model in [AmigaModel::A600EcsPal, AmigaModel::A600EcsNtsc] {
            let runtime = AmigaRuntimeKind::from_firmware(model, &firmware)
                .expect("A600 catalogue firmware path should construct");
            assert!(runtime.is_ecs(), "{model:?}");
        }
        for model in [AmigaModel::A1200AgaPal, AmigaModel::A1200AgaNtsc] {
            let runtime = AmigaRuntimeKind::from_firmware(model, &firmware)
                .expect("A1200 catalogue firmware path should construct");
            assert!(runtime.is_aga(), "{model:?}");
        }
    }

    #[test]
    fn c64_catalogue_replays_from_the_boot_waypoint() {
        let frame_ticks = u64::from(TIMING_PAL_BREADBIN.cycles_per_frame);
        let frames_per_second =
            TIMING_PAL_BREADBIN.cpu_hz as f64 / f64::from(TIMING_PAL_BREADBIN.cycles_per_frame);
        let mut original = HeadlessSession::new_with_query_provider(
            C64Runtime::blank(C64Model::C64PalBreadbin),
            frame_ticks,
            C64SessionQueryProvider,
        );
        let mut restored = HeadlessSession::new_with_query_provider(
            C64Runtime::blank(C64Model::C64PalBreadbin),
            frame_ticks,
            C64SessionQueryProvider,
        );
        let entry = Entry {
            id: "c64-pal-snapshot".into(),
            title: "C64 PAL snapshot".into(),
            year: 0,
            publisher: "Emu198x".into(),
            variant: "pal".into(),
            media: None,
            boot: Boot {
                wait_frames: 2,
                frame_hash: "xxh64:0000000000000000".into(),
                ignore_rects: vec![],
            },
            script: vec![],
            startup: vec![],
            audio: Audio {
                from_frame: 1,
                secs: 0.02,
                hash: "xxh64:0000000000000000".into(),
            },
        };

        let (_, snapshot) =
            finalize_snapshot_check(&mut original, &mut restored, &entry, frames_per_second)
                .expect("hermetic C64 snapshot check should run");
        assert!(
            matches!(snapshot.outcome, SnapshotOutcome::Pass),
            "{:?}",
            snapshot.outcome
        );
    }

    #[test]
    fn c64_catalogue_variants_select_the_expected_video_standard() {
        for variant in ["pal", "pal-1571", "pal-1581"] {
            assert_eq!(
                c64_model_from_variant(variant),
                Some(C64Model::C64PalBreadbin),
                "{variant}"
            );
        }
        assert_eq!(
            c64_model_from_variant("ntsc"),
            Some(C64Model::C64NtscBreadbin)
        );
        assert_eq!(c64_model_from_variant("pal-c64c"), None);
    }

    #[test]
    fn amiga_new_catalogue_profiles_replay_from_the_boot_waypoint() {
        for (variant, model) in [
            ("a600-ecs-pal", AmigaModel::A600EcsPal),
            ("a600-ecs-ntsc", AmigaModel::A600EcsNtsc),
            ("a1200-aga-pal", AmigaModel::A1200AgaPal),
            ("a1200-aga-ntsc", AmigaModel::A1200AgaNtsc),
        ] {
            let original_runtime = AmigaRuntimeKind::blank(model);
            let native_frame_ticks = original_runtime.native_frame_ticks();
            let frames_per_second = amiga_frames_per_second(&original_runtime, native_frame_ticks);
            let mut original = HeadlessSession::new_with_query_provider(
                original_runtime,
                native_frame_ticks,
                AmigaSessionQueryProvider,
            );
            let mut restored = HeadlessSession::new_with_query_provider(
                AmigaRuntimeKind::blank(model),
                native_frame_ticks,
                AmigaSessionQueryProvider,
            );
            let entry = Entry {
                id: format!("{variant}-snapshot"),
                title: variant.into(),
                year: 0,
                publisher: "Emu198x".into(),
                variant: variant.into(),
                media: None,
                boot: Boot {
                    wait_frames: 2,
                    frame_hash: "xxh64:0000000000000000".into(),
                    ignore_rects: vec![],
                },
                script: vec![],
                startup: vec![],
                audio: Audio {
                    from_frame: 1,
                    secs: 0.02,
                    hash: "xxh64:0000000000000000".into(),
                },
            };

            let (_, snapshot) =
                finalize_snapshot_check(&mut original, &mut restored, &entry, frames_per_second)
                    .expect("hermetic Amiga snapshot check should run");
            assert!(
                matches!(snapshot.outcome, SnapshotOutcome::Pass),
                "{variant}: {:?}",
                snapshot.outcome
            );
        }
    }

    #[test]
    fn manifest_parses_variant_firmware_layout() {
        let toml_text = r#"
[system]
id = "spectrum"

[[system.firmware]]
variant = "48k"
files = [
    { id = "sinclair-zx-spectrum-48k-rom", path = "sinclair-zx-spectrum-48k/48.rom" },
]

[[system.firmware]]
variant = "128k"
files = [
    { id = "sinclair-zx-spectrum-128k-rom-0", path = "sinclair-zx-spectrum-128k/128-0.rom" },
    { id = "sinclair-zx-spectrum-128k-rom-1", path = "sinclair-zx-spectrum-128k/128-1.rom" },
]

[[entry]]
id = "manic-miner"
title = "Manic Miner"
year = 1983
publisher = "Bug-Byte"
variant = "48k"

[entry.media]
kind = "tape"
slot = "tape-1"
path = "spectrum/manic-miner.tap"

[entry.boot]
wait_frames = 60
frame_hash = "xxh64:0000000000000000"

[entry.audio]
from_frame = 100
secs = 2.0
hash = "xxh64:0000000000000000"
"#;
        let manifest: Manifest = toml::from_str(toml_text).expect("manifest parses");
        assert_eq!(manifest.system.id, "spectrum");
        assert_eq!(manifest.system.firmware.len(), 2);
        assert_eq!(manifest.system.firmware[0].variant, "48k");
        assert_eq!(manifest.system.firmware[0].files.len(), 1);
        assert_eq!(manifest.system.firmware[1].variant, "128k");
        assert_eq!(manifest.system.firmware[1].files.len(), 2);
    }

    #[test]
    fn lookup_firmware_finds_variant() {
        let manifest = Manifest {
            system: SystemMeta {
                id: "spectrum".into(),
                audio_routing_version: None,
                frame_routing_version: None,
                firmware: vec![
                    VariantFirmware {
                        variant: "48k".into(),
                        files: vec![FirmwareSpec {
                            id: "rom".into(),
                            path: "48.rom".into(),
                        }],
                    },
                    VariantFirmware {
                        variant: "128k".into(),
                        files: vec![
                            FirmwareSpec {
                                id: "rom0".into(),
                                path: "128-0.rom".into(),
                            },
                            FirmwareSpec {
                                id: "rom1".into(),
                                path: "128-1.rom".into(),
                            },
                        ],
                    },
                ],
            },
            entry: vec![],
        };

        assert_eq!(lookup_firmware(&manifest, "48k").expect("48k").len(), 1);
        assert_eq!(lookup_firmware(&manifest, "128k").expect("128k").len(), 2);
        assert!(matches!(
            lookup_firmware(&manifest, "+3"),
            Err(CatalogueError::FirmwareNotDeclared(_))
        ));
    }

    fn spectrum_manifest_with_versions(audio: Option<u32>, frame: Option<u32>) -> Manifest {
        Manifest {
            system: SystemMeta {
                id: "spectrum".into(),
                audio_routing_version: audio,
                frame_routing_version: frame,
                firmware: vec![],
            },
            entry: vec![],
        }
    }

    fn c64_manifest_with_versions(audio: Option<u32>, frame: Option<u32>) -> Manifest {
        Manifest {
            system: SystemMeta {
                id: "c64".into(),
                audio_routing_version: audio,
                frame_routing_version: frame,
                firmware: vec![],
            },
            entry: vec![],
        }
    }

    fn nes_manifest_with_versions(audio: Option<u32>, frame: Option<u32>) -> Manifest {
        Manifest {
            system: SystemMeta {
                id: "nes".into(),
                audio_routing_version: audio,
                frame_routing_version: frame,
                firmware: vec![],
            },
            entry: vec![],
        }
    }

    #[test]
    fn routing_version_check_passes_when_manifest_omits_versions() {
        let manifest = spectrum_manifest_with_versions(None, None);
        verify_routing_versions(&manifest).expect("legacy manifests should pass");
    }

    #[test]
    fn routing_version_check_passes_when_manifest_matches_runtime() {
        let manifest = spectrum_manifest_with_versions(
            Some(common_sinclair_zx_spectrum::audio::AUDIO_ROUTING_VERSION),
            Some(common_sinclair_zx_spectrum::ula_engine::FRAME_ROUTING_VERSION),
        );
        verify_routing_versions(&manifest).expect("matching versions should pass");
    }

    #[test]
    fn routing_version_check_fails_loud_on_audio_mismatch() {
        let manifest = spectrum_manifest_with_versions(Some(9999), None);
        let err = verify_routing_versions(&manifest).expect_err("audio mismatch must fail");
        match err {
            CatalogueError::RoutingVersionMismatch {
                kind,
                system,
                found,
                ..
            } => {
                assert_eq!(kind, "audio");
                assert_eq!(system, "spectrum");
                assert_eq!(found, 9999);
            }
            other => panic!("expected RoutingVersionMismatch, got {other:?}"),
        }
    }

    #[test]
    fn routing_version_check_fails_loud_on_frame_mismatch() {
        let manifest = spectrum_manifest_with_versions(None, Some(9999));
        let err = verify_routing_versions(&manifest).expect_err("frame mismatch must fail");
        match err {
            CatalogueError::RoutingVersionMismatch {
                kind,
                system,
                found,
                ..
            } => {
                assert_eq!(kind, "frame");
                assert_eq!(system, "spectrum");
                assert_eq!(found, 9999);
            }
            other => panic!("expected RoutingVersionMismatch, got {other:?}"),
        }
    }

    #[test]
    fn routing_version_check_passes_for_c64_when_manifest_matches_runtime() {
        let manifest = c64_manifest_with_versions(
            Some(mos_sid_6581::AUDIO_ROUTING_VERSION),
            Some(mos_vic_ii::FRAME_ROUTING_VERSION),
        );
        verify_routing_versions(&manifest).expect("matching C64 versions should pass");
    }

    #[test]
    fn routing_version_check_fails_loud_on_c64_audio_mismatch() {
        let manifest = c64_manifest_with_versions(Some(9999), None);
        let err = verify_routing_versions(&manifest).expect_err("C64 audio mismatch must fail");
        match err {
            CatalogueError::RoutingVersionMismatch {
                kind,
                system,
                found,
                ..
            } => {
                assert_eq!(kind, "audio");
                assert_eq!(system, "c64");
                assert_eq!(found, 9999);
            }
            other => panic!("expected RoutingVersionMismatch, got {other:?}"),
        }
    }

    #[test]
    fn routing_version_check_fails_loud_on_c64_frame_mismatch() {
        let manifest = c64_manifest_with_versions(None, Some(9999));
        let err = verify_routing_versions(&manifest).expect_err("C64 frame mismatch must fail");
        match err {
            CatalogueError::RoutingVersionMismatch {
                kind,
                system,
                found,
                ..
            } => {
                assert_eq!(kind, "frame");
                assert_eq!(system, "c64");
                assert_eq!(found, 9999);
            }
            other => panic!("expected RoutingVersionMismatch, got {other:?}"),
        }
    }

    #[test]
    fn routing_version_check_passes_for_nes_when_manifest_matches_runtime() {
        let manifest = nes_manifest_with_versions(
            Some(ricoh_apu_2a03::AUDIO_ROUTING_VERSION),
            Some(ricoh_ppu_2c02::FRAME_ROUTING_VERSION),
        );
        verify_routing_versions(&manifest).expect("matching NES versions should pass");
    }

    #[test]
    fn routing_version_check_fails_loud_on_nes_audio_mismatch() {
        let manifest = nes_manifest_with_versions(Some(9999), None);
        let err = verify_routing_versions(&manifest).expect_err("NES audio mismatch must fail");
        match err {
            CatalogueError::RoutingVersionMismatch {
                kind,
                system,
                found,
                ..
            } => {
                assert_eq!(kind, "audio");
                assert_eq!(system, "nes");
                assert_eq!(found, 9999);
            }
            other => panic!("expected RoutingVersionMismatch, got {other:?}"),
        }
    }

    #[test]
    fn routing_version_check_fails_loud_on_nes_frame_mismatch() {
        let manifest = nes_manifest_with_versions(None, Some(9999));
        let err = verify_routing_versions(&manifest).expect_err("NES frame mismatch must fail");
        match err {
            CatalogueError::RoutingVersionMismatch {
                kind,
                system,
                found,
                ..
            } => {
                assert_eq!(kind, "frame");
                assert_eq!(system, "nes");
                assert_eq!(found, 9999);
            }
            other => panic!("expected RoutingVersionMismatch, got {other:?}"),
        }
    }

    /// `run_entry_for_capture` must skip `verify_routing_versions` —
    /// capture is the action that *resolves* a mismatch. Verified by
    /// constructing a manifest with a mismatched frame version and an
    /// unsupported system id: if the version check fired we'd see
    /// `RoutingVersionMismatch`; with it bypassed we see
    /// `UnsupportedSystem` from the inner dispatch.
    #[test]
    fn run_entry_for_capture_bypasses_routing_version_check() {
        let mut manifest = spectrum_manifest_with_versions(None, Some(9999));
        manifest.system.id = "not-a-real-system".into();
        let dummy_entry = Entry {
            id: "dummy".into(),
            title: "dummy".into(),
            year: 0,
            publisher: "".into(),
            variant: "48k".into(),
            media: None,
            boot: Boot {
                wait_frames: 0,
                frame_hash: "xxh64:0000000000000000".into(),
                ignore_rects: vec![],
            },
            script: vec![],
            startup: vec![],
            audio: Audio {
                from_frame: 0,
                secs: 0.0,
                hash: "xxh64:0000000000000000".into(),
            },
        };
        let err = run_entry_for_capture(
            &manifest,
            &dummy_entry,
            std::path::Path::new("/dev/null"),
            std::path::Path::new("/dev/null"),
        )
        .expect_err("capture must still surface the inner dispatch error");
        match err {
            CatalogueError::UnsupportedSystem(name) => {
                assert_eq!(name, "not-a-real-system");
            }
            CatalogueError::RoutingVersionMismatch { .. } => {
                panic!("capture must NOT fire the routing-version check");
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn routing_version_error_message_includes_recapture_instruction() {
        let err = CatalogueError::RoutingVersionMismatch {
            kind: "audio",
            system: "spectrum".into(),
            expected: 2,
            found: 1,
        };
        let msg = err.to_string();
        assert!(
            msg.contains("re-captured"),
            "error should instruct re-capture: {msg}"
        );
        assert!(
            msg.contains("Seam 4"),
            "error should reference Seam 4: {msg}"
        );
    }
}
