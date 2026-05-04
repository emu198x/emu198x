//! Cross-system curated catalogue. See `wiki/decisions/october-catalogue.md`.
//!
//! This crate is the October-launch regression bench: 10 titles per system
//! across the four launch targets (Spectrum, C64, NES, Amiga). Each entry
//! asserts a boot frame hash, optional scripted-input progression, and an
//! audio-window hash.
//!
//! The crate starts narrow with Spectrum 48K. Schema and runner extend as
//! the C64, NES, and Amiga runtimes are wired in.

use std::hash::Hasher;
use std::path::{Path, PathBuf};

use common_sinclair_zx_spectrum::timing::TIMING_48K;
use emu198x_shell::{
    FirmwareImage, FirmwareSet, HeadlessSession, InputEvent, MediaImage, MediaKind, MediaSet,
    read_firmware_asset, read_media_asset,
};
use runtime_sinclair_zx_spectrum::{
    DEFAULT_TAPE_AUTOLOAD_BOOT_FRAMES, Spectrum48kRuntime, SpectrumSessionQueryProvider,
    autoload_basic_tape,
};
use serde::Deserialize;
use thiserror::Error;
use twox_hash::XxHash64;

/// Safety cap on the tape-load wait. At PAL 50 fps this is ~20 minutes
/// of emulation time — far longer than any 48K loader needs.
const MAX_TAPE_LOAD_FRAMES: u32 = 60_000;

/// Top-level manifest shape (one TOML file per system).
#[derive(Debug, Deserialize)]
pub struct Manifest {
    pub system: SystemMeta,
    pub entry: Vec<Entry>,
}

/// System-level defaults that apply to every entry in the file.
#[derive(Debug, Deserialize)]
pub struct SystemMeta {
    /// Stable system identifier (e.g. `spectrum`, `c64`, `nes`, `amiga`).
    pub id: String,
    /// Firmware identifier consumed by the per-system runtime.
    pub firmware_id: String,
    /// Firmware path relative to the firmware root.
    pub firmware_path: String,
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
    pub media: Media,
    pub boot: Boot,
    #[serde(default)]
    pub script: Vec<ScriptStep>,
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
}

/// Boot waypoint. The runner waits for tape load to finish (when the
/// media is a tape), then runs `wait_frames` more frames, then captures
/// the frame for hashing.
#[derive(Debug, Deserialize)]
pub struct Boot {
    pub wait_frames: u32,
    /// Expected `xxh64:HEX` of the RGBA8888 frame at the waypoint.
    pub frame_hash: String,
}

/// One scripted input step. `at_frame` is counted from the boot waypoint.
#[derive(Debug, Deserialize)]
pub struct ScriptStep {
    pub at_frame: u32,
    /// Spectrum-style key name (e.g. `enter`, `space`, `0`, `a`).
    pub press: String,
}

/// Audio capture window. `from_frame` is counted from the boot waypoint.
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
#[derive(Debug)]
pub struct RunResult {
    pub boot_hash: String,
    pub audio_hash: String,
    pub outcome: EntryOutcome,
}

#[derive(Debug, Error)]
pub enum CatalogueError {
    #[error("manifest not found: {0}")]
    ManifestNotFound(PathBuf),
    #[error("manifest parse failed: {0}")]
    ManifestParse(toml::de::Error),
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
    toml::from_str(&text).map_err(CatalogueError::ManifestParse)
}

/// Hashes one byte slice with xxhash64 and formats as `xxh64:HEX`.
#[must_use]
pub fn hash_xxh64(bytes: &[u8]) -> String {
    let mut hasher = XxHash64::default();
    hasher.write(bytes);
    format!("xxh64:{:016x}", hasher.finish())
}

/// Runs one catalogue entry against the appropriate system runtime.
///
/// `media_root` resolves `entry.media.path`; `firmware_root` resolves
/// `manifest.system.firmware_path`. The runner returns the captured
/// hashes plus a pass/fail outcome — assertion is the caller's concern.
///
/// # Errors
///
/// Returns `UnsupportedSystem` for systems other than `spectrum`,
/// `UnsupportedVariant` for variants not yet wired, and `Session` for
/// firmware/media/runtime failures.
pub fn run_entry(
    manifest: &Manifest,
    entry: &Entry,
    media_root: &Path,
    firmware_root: &Path,
) -> Result<RunResult, CatalogueError> {
    match manifest.system.id.as_str() {
        "spectrum" => run_spectrum_entry(manifest, entry, media_root, firmware_root),
        other => Err(CatalogueError::UnsupportedSystem(other.into())),
    }
}

fn run_spectrum_entry(
    manifest: &Manifest,
    entry: &Entry,
    media_root: &Path,
    firmware_root: &Path,
) -> Result<RunResult, CatalogueError> {
    if entry.variant != "48k" {
        return Err(CatalogueError::UnsupportedVariant(entry.variant.clone()));
    }

    let firmware_path = firmware_root.join(&manifest.system.firmware_path);
    let firmware = read_firmware_asset(&firmware_path)
        .map_err(|err| CatalogueError::Session(format!("firmware {firmware_path:?}: {err}")))?;

    let mut firmware_set = FirmwareSet::new();
    firmware_set.push(FirmwareImage::new(
        manifest.system.firmware_id.clone(),
        &firmware.bytes,
    ));

    let runtime = Spectrum48kRuntime::from_firmware(&firmware_set)
        .map_err(|err| CatalogueError::Session(format!("runtime: {err}")))?;

    let mut session = HeadlessSession::new_with_query_provider(
        runtime,
        u64::from(TIMING_48K.halfcycles_per_frame),
        SpectrumSessionQueryProvider,
    );

    let media_path = media_root.join(&entry.media.path);
    let media_kind = parse_media_kind(&entry.media.kind)?;
    let media_loaded = read_media_asset(&media_path, media_kind)
        .map_err(|err| CatalogueError::Session(format!("media {media_path:?}: {err}")))?;

    let mut media_set = MediaSet::new();
    media_set.push(MediaImage::new(
        entry.media.slot.clone(),
        media_kind,
        &media_loaded.bytes,
    ));

    session
        .prepare(&media_set, &[])
        .map_err(|err| CatalogueError::Session(format!("prepare: {err}")))?;

    autoload_basic_tape(
        &mut session,
        &entry.media.slot,
        DEFAULT_TAPE_AUTOLOAD_BOOT_FRAMES,
    )
    .map_err(|err| CatalogueError::Session(format!("autoload: {err}")))?;

    // For tape media: wait for the tape transport to stop before running
    // boot.wait_frames. This makes wait_frames mean "frames after tape
    // load completes" which is the user-meaningful waypoint.
    if media_kind == MediaKind::Tape {
        session
            .wait_for_query_bool("spectrum.tape.playing", false, MAX_TAPE_LOAD_FRAMES)
            .map_err(|err| CatalogueError::Session(format!("tape stop: {err}")))?;
    }

    // Boot waypoint: run wait_frames more frames, then hash the frame.
    session
        .run_frames(entry.boot.wait_frames)
        .map_err(|err| CatalogueError::Session(format!("boot wait: {err}")))?;

    let boot_frame = session
        .latest_frame()
        .ok_or_else(|| CatalogueError::Session("no frame at boot waypoint".into()))?;
    let boot_rgba = boot_frame
        .rgba_pixels()
        .map_err(|err| CatalogueError::Session(format!("rgba: {err}")))?;
    let boot_hash = hash_xxh64(&boot_rgba);

    // Scripted input. `at_frame` is counted from the boot waypoint.
    let mut frames_since_boot: u32 = 0;
    for step in &entry.script {
        if step.at_frame > frames_since_boot {
            let advance = step.at_frame - frames_since_boot;
            session
                .run_frames(advance)
                .map_err(|err| CatalogueError::Session(format!("script advance: {err}")))?;
            frames_since_boot = step.at_frame;
        }
        session.queue_input(InputEvent::Key {
            name: step.press.clone().into(),
            pressed: true,
        });
        session
            .run_frames(2)
            .map_err(|err| CatalogueError::Session(format!("press: {err}")))?;
        session.queue_input(InputEvent::Key {
            name: step.press.clone().into(),
            pressed: false,
        });
        session
            .run_frames(2)
            .map_err(|err| CatalogueError::Session(format!("release: {err}")))?;
        frames_since_boot = frames_since_boot.saturating_add(4);
    }

    // Audio window. `from_frame` is counted from the boot waypoint.
    if entry.audio.from_frame > frames_since_boot {
        let advance = entry.audio.from_frame - frames_since_boot;
        session
            .run_frames(advance)
            .map_err(|err| CatalogueError::Session(format!("audio gap: {err}")))?;
    }

    session.clear_audio_capture();

    let frames_per_sec =
        (TIMING_48K.master_hz as f64) / f64::from(TIMING_48K.halfcycles_per_frame);
    let audio_frames = (f64::from(entry.audio.secs) * frames_per_sec).round() as u32;
    session
        .run_frames(audio_frames)
        .map_err(|err| CatalogueError::Session(format!("audio run: {err}")))?;

    let audio_wav = session
        .audio_wav_bytes()
        .map_err(|err| CatalogueError::Session(format!("audio: {err}")))?;
    let audio_hash = hash_xxh64(&audio_wav);

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

    Ok(RunResult {
        boot_hash,
        audio_hash,
        outcome,
    })
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_xxh64_format_is_stable() {
        let h = hash_xxh64(b"hello world");
        assert!(h.starts_with("xxh64:"), "got: {h}");
        assert_eq!(h.len(), "xxh64:".len() + 16);
        // Re-hashing the same bytes must produce the same string.
        assert_eq!(h, hash_xxh64(b"hello world"));
    }

    #[test]
    fn manifest_parses_minimal_entry() {
        let toml_text = r#"
[system]
id = "spectrum"
firmware_id = "sinclair-zx-spectrum-48k-rom"
firmware_path = "sinclair-zx-spectrum-48k/48.rom"

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
wait_frames = 600
frame_hash = "xxh64:0000000000000000"

[entry.audio]
from_frame = 700
secs = 2.0
hash = "xxh64:0000000000000000"
"#;
        let manifest: Manifest = toml::from_str(toml_text).expect("manifest parses");
        assert_eq!(manifest.system.id, "spectrum");
        assert_eq!(manifest.entry.len(), 1);
        assert_eq!(manifest.entry[0].id, "manic-miner");
        assert!(manifest.entry[0].script.is_empty());
    }
}
