//! Cross-system curated catalogue. See `wiki/decisions/october-catalogue.md`.
//!
//! This crate is the October-launch regression bench: 10 titles per system
//! across the four launch targets (Spectrum, C64, NES, Amiga). Each entry
//! asserts a boot frame hash, optional scripted-input progression, and an
//! audio-window hash.
//!
//! Currently wired: Spectrum 48K + 128K. Schema and runner extend as the
//! +3, Pentagon, Timex, C64, NES, and Amiga runtimes are wired in.

use std::hash::Hasher;
use std::path::{Path, PathBuf};

use common_commodore_c64::timing::{TIMING_NTSC_BREADBIN, TIMING_PAL_BREADBIN};
use common_sinclair_zx_spectrum::timing::{TIMING_48K, TIMING_128K, TIMING_PLUS2A};
use emu198x_shell::{
    ControlCommand, FirmwareImage, FirmwareSet, HeadlessSession, InputEvent, MachineCore,
    MediaImage, MediaKind, MediaSet, MediaTransportAction, MediaTransportCommand,
    SessionQueryProvider, read_firmware_asset, read_media_asset,
};
use machine_sinclair_zx_spectrum_128k::Spectrum128K;
use machine_sinclair_zx_spectrum_plus::{Model as PlusModel, SpectrumPlus};
use runtime_commodore_amiga::{
    A500_NTSC_FRAME_TICKS, A500_PAL_FRAME_TICKS, AmigaRuntimeKind, AmigaSessionQueryProvider,
    Model as AmigaModel,
};
use runtime_commodore_c64::{
    C64Runtime, C64SessionQueryProvider, Model as C64Model, autoload_basic_disk,
    autoload_basic_tape as c64_autoload_basic_tape,
};
use runtime_nintendo_nes::{Model as NesModel, NesRuntime, NesSessionQueryProvider};
use runtime_sinclair_zx_spectrum::{
    DEFAULT_TAPE_AUTOLOAD_BOOT_FRAMES, Model, Spectrum48kRuntime, Spectrum128kRuntime,
    SpectrumPlusRuntime, SpectrumSessionQueryProvider, autoload_basic_tape,
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

/// Amiga PAL frames per second (50.08 Hz at 28.375 MHz master / 8 / 70908 CCKs).
const AMIGA_PAL_FRAMES_PER_SEC: f64 = 50.0;

/// Amiga NTSC frames per second (~59.94 Hz).
const AMIGA_NTSC_FRAMES_PER_SEC: f64 = 59.94;

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

/// Boot waypoint. After the system's setup phase completes (tape stops,
/// cartridge boots, KERNAL reaches READY) the runner runs any
/// `script[]` steps, then waits `wait_frames` more frames, then captures
/// the frame. For games that need a LOAD-then-RUN sequence (e.g. C64
/// disk titles), the scripted RUN happens before this capture so the
/// boot frame lands on the actual title screen.
#[derive(Debug, Deserialize)]
pub struct Boot {
    pub wait_frames: u32,
    /// Expected `xxh64:HEX` of the RGBA8888 frame at the waypoint.
    pub frame_hash: String,
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
    /// Joystick button press on a controller port. `port` is 0 or 1
    /// (Amiga port-0 mouse / port-1 joystick; C64 port-1 / port-2).
    /// `button` is `fire`, `button1`, etc. — system-specific.
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

/// Audio capture window. `from_frame` is counted from the boot waypoint
/// capture (i.e. after script + wait_frames have completed).
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
/// each firmware file path declared in `manifest.system.firmware`.
///
/// # Errors
///
/// Returns `UnsupportedSystem` for systems other than `spectrum`,
/// `UnsupportedVariant` for variants not yet wired, `FirmwareNotDeclared`
/// when a variant is missing from `system.firmware`, and `Session` for
/// firmware/media/runtime failures.
pub fn run_entry(
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

fn amiga_model_from_variant(variant: &str) -> Option<AmigaModel> {
    match variant {
        "a500-ocs-pal" => Some(AmigaModel::A500OcsPal),
        "a500-ocs-ntsc" => Some(AmigaModel::A500OcsNtsc),
        "a500-ocs-pal-a501" => Some(AmigaModel::A500OcsPalA501),
        "a500-ocs-ntsc-a501" => Some(AmigaModel::A500OcsNtscA501),
        "a500-plus-ecs-pal" => Some(AmigaModel::A500PlusEcsPal),
        "a500-plus-ecs-ntsc" => Some(AmigaModel::A500PlusEcsNtsc),
        _ => None,
    }
}

fn amiga_frame_ticks(model: AmigaModel) -> u64 {
    match model {
        AmigaModel::A500OcsNtsc
        | AmigaModel::A500OcsNtscA501
        | AmigaModel::A500PlusEcsNtsc => A500_NTSC_FRAME_TICKS,
        _ => A500_PAL_FRAME_TICKS,
    }
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

    let mut session = HeadlessSession::new_with_query_provider(
        runtime,
        amiga_frame_ticks(model),
        AmigaSessionQueryProvider,
    );

    if let Some(media) = entry.media.as_ref() {
        let _ = load_media_spec(&mut session, media, media_root)?;
        // No autoload for Amiga: Kickstart boots itself, then auto-DMAs
        // the boot block off DF0 if a disk is inserted.
    } else {
        prepare_session_no_media(&mut session)?;
    }

    let frames_per_sec = if matches!(
        model,
        AmigaModel::A500OcsNtsc
            | AmigaModel::A500OcsNtscA501
            | AmigaModel::A500PlusEcsNtsc
    ) {
        AMIGA_NTSC_FRAMES_PER_SEC
    } else {
        AMIGA_PAL_FRAMES_PER_SEC
    };

    run_assertions(&mut session, entry, frames_per_sec)
}

fn run_spectrum_entry(
    manifest: &Manifest,
    entry: &Entry,
    media_root: &Path,
    firmware_root: &Path,
) -> Result<RunResult, CatalogueError> {
    match entry.variant.as_str() {
        "48k" => run_spectrum_48k_entry(manifest, entry, media_root, firmware_root),
        "128k" => run_spectrum_128k_entry(manifest, entry, media_root, firmware_root),
        "plus3" => run_spectrum_plus3_entry(manifest, entry, media_root, firmware_root),
        other => Err(CatalogueError::UnsupportedVariant(other.into())),
    }
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

    autoload_basic_tape(
        &mut session,
        &media.slot,
        DEFAULT_TAPE_AUTOLOAD_BOOT_FRAMES,
    )
    .map_err(|err| CatalogueError::Session(format!("48K autoload: {err}")))?;

    wait_for_tape_stop(&mut session, media_kind, "spectrum.tape.playing")?;
    run_assertions(&mut session, entry, spectrum_frames_per_sec(&TIMING_48K))
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

    wait_for_tape_stop(&mut session, media_kind, "spectrum.tape.playing")?;
    run_assertions(&mut session, entry, spectrum_frames_per_sec(&TIMING_128K))
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
    let model = match entry.variant.as_str() {
        "pal" => C64Model::C64PalBreadbin,
        "ntsc" => C64Model::C64NtscBreadbin,
        other => return Err(CatalogueError::UnsupportedVariant(other.into())),
    };

    let files = lookup_firmware(manifest, &entry.variant)?;
    // C64 needs 3 ROMs minimum (KERNAL/BASIC/CHARGEN). When disk media
    // is attached, the 1541 DOS ROM must also be loaded — declare it
    // up-front in the manifest. Tolerate either count here so manifests
    // can choose to omit the drive ROM for entries that never use disk.
    if !(3..=4).contains(&files.len()) {
        return Err(CatalogueError::FirmwareCountMismatch {
            variant: entry.variant.clone(),
            expected: 3,
            actual: files.len(),
        });
    }
    let bytes_storage: Vec<Vec<u8>> = files
        .iter()
        .map(|spec| read_firmware_bytes(firmware_root, spec))
        .collect::<Result<_, _>>()?;

    let mut firmware_set = FirmwareSet::new();
    for (spec, bytes) in files.iter().zip(bytes_storage.iter()) {
        firmware_set.push(FirmwareImage::new(spec.id.clone(), bytes));
    }

    let runtime = C64Runtime::from_firmware(model, &firmware_set)
        .map_err(|err| CatalogueError::Session(format!("C64 runtime: {err}")))?;

    let timing = match model {
        C64Model::C64PalBreadbin => &TIMING_PAL_BREADBIN,
        C64Model::C64NtscBreadbin => &TIMING_NTSC_BREADBIN,
    };

    let mut session = HeadlessSession::new_with_query_provider(
        runtime,
        u64::from(timing.cycles_per_frame),
        C64SessionQueryProvider,
    );

    if let Some(media) = entry.media.as_ref() {
        let media_kind = load_media_spec(&mut session, media, media_root)?;
        match media_kind {
            MediaKind::Disk => {
                autoload_basic_disk(
                    &mut session,
                    &media.slot,
                    DEFAULT_C64_BOOT_FRAMES,
                    DEFAULT_C64_DISK_PROMPT_FRAMES,
                )
                .map_err(|err| CatalogueError::Session(format!("C64 disk autoload: {err}")))?;
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
                c64_autoload_basic_tape(
                    &mut session,
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
            other => {
                return Err(CatalogueError::UnsupportedMediaKind(format!("c64: {other:?}")));
            }
        }
    } else {
        prepare_session_no_media(&mut session)?;
        session
            .wait_for_boot(DEFAULT_C64_BOOT_FRAMES)
            .map_err(|err| CatalogueError::Session(format!("C64 boot wait: {err}")))?;
    }

    let frames_per_sec = (timing.cpu_hz as f64) / f64::from(timing.cycles_per_frame);
    run_assertions(&mut session, entry, frames_per_sec)
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

    let mut machine = SpectrumPlus::new(PlusModel::Plus3);
    machine.memory.load_roms(&r0, &r1, &r2, &r3);
    let runtime = SpectrumPlusRuntime::new(Model::SpectrumPlus3, machine);

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

    // Defer autoload: rely on entry.script[] to send the menu keypress
    // for +3, since the menu's behaviour with a disk inserted is
    // title-specific (some auto-run, others wait for ENTER).
    // autoload_plus3_loader(&mut session, DEFAULT_128K_BOOT_FRAMES)?;

    // For tape media on +3, the standard tape-stop wait still applies.
    // For disk, the +3 disk loader runs autonomously after Loader is
    // selected — wait_frames in the entry covers the full load.
    if media_kind == MediaKind::Tape {
        wait_for_tape_stop(&mut session, media_kind, "spectrum.tape.playing")?;
    }

    run_assertions(&mut session, entry, spectrum_frames_per_sec(&TIMING_PLUS2A))
}

/// +3 autoload: wait for the +3 menu boot banner, press ENTER (selects
/// the highlighted "Loader" option which auto-runs the disk's first
/// program). The disk must already be inserted via load_media_spec.
fn autoload_plus3_loader<Q>(
    session: &mut HeadlessSession<SpectrumPlusRuntime, Q>,
    max_boot_frames: u32,
) -> Result<(), CatalogueError>
where
    Q: SessionQueryProvider<SpectrumPlusRuntime>,
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

/// 128K-equivalent of the 48K `autoload_basic_tape`. Waits for the 128K
/// menu, presses ENTER (which selects the highlighted "Tape Loader"
/// option), and starts tape transport.
fn autoload_128k_tape_loader<Q>(
    session: &mut HeadlessSession<Spectrum128kRuntime, Q>,
    slot: &str,
    max_boot_frames: u32,
) -> Result<(), CatalogueError>
where
    Q: SessionQueryProvider<Spectrum128kRuntime>,
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
    media_set.push(MediaImage::new(
        media.slot.clone(),
        media_kind,
        &media_loaded.bytes,
    ));

    session
        .prepare(&media_set, &[])
        .map_err(|err| CatalogueError::Session(format!("prepare: {err}")))?;

    Ok(media_kind)
}

/// Prepares the session with no media. Some catalogue entries (e.g.
/// C64 boot-to-READY) test the firmware path alone.
fn prepare_session_no_media<M, Q>(
    session: &mut HeadlessSession<M, Q>,
) -> Result<(), CatalogueError>
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
/// session loaded and at the start of the script phase (tape stopped,
/// cartridge running, READY shown, etc.), this advances the timeline:
///
///   1. Run `script[]` steps (each `at_frame` is relative to start of
///      this phase). Lets disk-loaded titles type RUN, multi-stage
///      loaders advance through prompts, etc.
///   2. Wait `boot.wait_frames` more frames.
///   3. Capture boot frame.
///   4. Wait `audio.from_frame` more frames.
///   5. Capture `audio.secs`-second audio window.
///
/// Putting script before the boot capture means disk-game titles can
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
    let boot_png = boot_frame
        .png_bytes()
        .map_err(|err| CatalogueError::Session(format!("boot png: {err}")))?;

    if entry.audio.from_frame > 0 {
        session
            .run_frames(entry.audio.from_frame)
            .map_err(|err| CatalogueError::Session(format!("audio gap: {err}")))?;
    }

    session.clear_audio_capture();

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
        boot_png,
        audio_wav,
    })
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

    #[test]
    fn hash_xxh64_format_is_stable() {
        let h = hash_xxh64(b"hello world");
        assert!(h.starts_with("xxh64:"), "got: {h}");
        assert_eq!(h.len(), "xxh64:".len() + 16);
        assert_eq!(h, hash_xxh64(b"hello world"));
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
}
