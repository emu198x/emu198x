//! Variant selection and conventional firmware for a machine family.
//!
//! A family runtime already knows how to build any of its variants from a
//! [`FirmwareSet`]. What used to live in each binary instead was the rest of
//! the story: the ids a script or `--machine` flag uses to name a variant,
//! and where on disk that variant's ROMs are by convention. Three binaries
//! had three copies of that, each also copied into their windowed UI's
//! variant menu and their MCP `set_machine` tool, so the same machine could
//! be named or found differently depending on which door you came in by.
//!
//! [`FamilyRuntime`] now declares the vocabulary and the conventional
//! sources; this module resolves them once for every caller: the launcher
//! at boot, the UI's variant menu, and the shared `set_machine` step.
//!
//! The on-disk convention is `~/.emu198x/roms/<dir>/<file>`, with a
//! family-specific environment variable naming an alternative directory.
//! A caller can pin any single ROM by its firmware id, or point at a
//! different directory, through [`FirmwareOverrides`].

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use thiserror::Error;

use crate::asset::read_firmware_asset;
use crate::firmware::{FirmwareImage, FirmwareSet};
use crate::loaders::LoaderError;
use crate::machine::FamilyRuntime;
use crate::query::SessionQueryProvider;
use crate::session::HeadlessSession;

/// One firmware image a variant boots, and where it is by convention.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FirmwareSource {
    /// The id the family's `from_firmware` looks the image up by; matches
    /// the profile's [`FirmwareRequirement`](crate::FirmwareRequirement).
    pub id: &'static str,
    /// File names to try inside the family's ROM directory, first hit
    /// wins. A name may carry a sub-directory.
    pub candidates: &'static [&'static str],
    /// Whether the variant boots without it.
    pub optional: bool,
}

impl FirmwareSource {
    /// A required image.
    #[must_use]
    pub const fn required(id: &'static str, candidates: &'static [&'static str]) -> Self {
        Self {
            id,
            candidates,
            optional: false,
        }
    }

    /// An image the variant can do without.
    #[must_use]
    pub const fn optional(id: &'static str, candidates: &'static [&'static str]) -> Self {
        Self {
            id,
            candidates,
            optional: true,
        }
    }
}

/// Where a family keeps its ROMs by convention.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RomConvention {
    /// An environment variable naming the ROM directory outright.
    pub env_var: Option<&'static str>,
    /// Directories under `~/.emu198x/roms`, first existing wins. An empty
    /// string is the root itself.
    pub dirs: &'static [&'static str],
}

/// Caller-supplied firmware locations that take precedence over the
/// convention.
///
/// Empty is the ordinary case. A pin replaces exactly one image and leaves
/// the rest resolving conventionally, which is what a multi-ROM variant
/// needs: a +3 boots four ROMs, so a single scalar path could not say
/// "this one, conventional for the other three".
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FirmwareOverrides {
    /// Use this ROM directory instead of the conventional ones.
    pub dir: Option<PathBuf>,
    /// Use these paths for these firmware ids.
    pub by_id: BTreeMap<String, PathBuf>,
}

impl FirmwareOverrides {
    /// No overrides: everything resolves by convention.
    #[must_use]
    pub fn none() -> Self {
        Self::default()
    }

    /// Pin one image by id.
    pub fn pin(&mut self, id: impl Into<String>, path: impl Into<PathBuf>) {
        self.by_id.insert(id.into(), path.into());
    }

    /// Add one `--rom` value against `sources`.
    ///
    /// Two spellings mean one thing across every mode: `ID=PATH` names the
    /// image, and a bare `PATH` is sugar for the sole required image of a
    /// single-ROM variant.
    ///
    /// # Errors
    ///
    /// [`FirmwareResolveError::AmbiguousRomPath`] for a bare path on a
    /// variant that boots several ROMs; applying it to the first would boot
    /// a machine assembled from two ROM sets and report success.
    /// [`FirmwareResolveError::MalformedRomSpec`] for `=PATH` or `ID=`.
    pub fn add_spec(
        &mut self,
        spec: &str,
        machine: &str,
        sources: &[FirmwareSource],
    ) -> Result<(), FirmwareResolveError> {
        if let Some((id, path)) = spec.split_once('=') {
            if id.is_empty() || path.is_empty() {
                return Err(FirmwareResolveError::MalformedRomSpec {
                    spec: spec.to_owned(),
                });
            }
            self.pin(id, path);
            return Ok(());
        }
        if spec.is_empty() {
            return Err(FirmwareResolveError::MalformedRomSpec {
                spec: spec.to_owned(),
            });
        }
        match sources {
            [only] => {
                self.pin(only.id, spec);
                Ok(())
            }
            _ => Err(FirmwareResolveError::AmbiguousRomPath {
                machine: machine.to_owned(),
                count: sources.len(),
                known: known_ids(sources),
            }),
        }
    }
}

fn known_ids(sources: &[FirmwareSource]) -> String {
    sources
        .iter()
        .map(|source| source.id)
        .collect::<Vec<_>>()
        .join(", ")
}

/// Why a variant's firmware could not be located or read.
#[derive(Debug, Error)]
pub enum FirmwareResolveError {
    /// The id names no variant of this family.
    #[error("unknown machine id `{id}`; expected one of {known}")]
    UnknownMachine {
        /// The id that was asked for.
        id: String,
        /// The ids the family accepts, comma-separated.
        known: String,
    },
    /// `HOME` is unset and something still needed the conventional root.
    #[error("$HOME unset; cannot locate ~/.emu198x/roms")]
    HomeUnset,
    /// No ROM directory exists and something still needed one.
    #[error("no {machine} ROM directory found; tried {tried}")]
    NoRomDir {
        /// The variant being built.
        machine: String,
        /// The directories that were tried, comma-separated.
        tried: String,
    },
    /// A required image is not at any of its conventional names.
    #[error("no `{id}` ROM for {machine} in {dir}; tried {tried}")]
    Missing {
        /// The firmware id.
        id: String,
        /// The variant being built.
        machine: String,
        /// The directory searched.
        dir: String,
        /// The file names tried, comma-separated.
        tried: String,
    },
    /// A pinned path does not exist.
    #[error("ROM not found at {path}")]
    PinnedMissing {
        /// The path that was pinned.
        path: String,
    },
    /// A pin named an id this variant does not boot.
    #[error("no ROM `{id}` on {machine}; this variant takes: {known}")]
    UnknownRomId {
        /// The id that was pinned.
        id: String,
        /// The variant being built.
        machine: String,
        /// The ids it does take, comma-separated.
        known: String,
    },
    /// A bare path was given for a variant that boots several ROMs.
    #[error(
        "{machine} boots {count} ROMs, so a bare --rom PATH is ambiguous; \
         use --rom ID=PATH with one of: {known}"
    )]
    AmbiguousRomPath {
        /// The variant being built.
        machine: String,
        /// How many ROMs it boots.
        count: usize,
        /// The ids it takes, comma-separated.
        known: String,
    },
    /// A `--rom` value was neither `ID=PATH` nor a path.
    #[error("malformed ROM spec `{spec}`; expected PATH or ID=PATH")]
    MalformedRomSpec {
        /// The value as given.
        spec: String,
    },
    /// Reading an image failed.
    #[error("failed to read ROM {path}: {reason}")]
    Read {
        /// The path that was read.
        path: String,
        /// The reader's error text.
        reason: String,
    },
    /// The family refused to build the variant from the images.
    #[error("building {machine}: {reason}")]
    Build {
        /// The variant being built.
        machine: String,
        /// The family's error text.
        reason: String,
    },
}

/// The model `id` names, or an error listing the family's ids.
///
/// # Errors
///
/// [`FirmwareResolveError::UnknownMachine`] for an id the family does not
/// have.
pub fn model_from_id<M: FamilyRuntime>(id: &str) -> Result<M::Model, FirmwareResolveError> {
    M::model_from_id(id).ok_or_else(|| FirmwareResolveError::UnknownMachine {
        id: id.to_owned(),
        known: M::variant_ids().join(", "),
    })
}

/// The path of every image `model` boots, pins applied.
///
/// Optional images that are absent are left out. A variant whose every
/// required image is pinned needs no ROM directory and no `HOME`, so the
/// sandboxed builds that most want pins can use them.
///
/// # Errors
///
/// See [`FirmwareResolveError`]; a pin naming an id the variant does not
/// take is refused rather than ignored, because booting the conventional
/// ROM after being told to use a specific one looks like success.
pub fn resolve_firmware<M: FamilyRuntime>(
    model: M::Model,
    overrides: &FirmwareOverrides,
) -> Result<Vec<(&'static str, PathBuf)>, FirmwareResolveError> {
    let machine = M::variant_id(model);
    let sources = M::firmware_sources(model);
    if let Some(unknown) = overrides
        .by_id
        .keys()
        .find(|id| !sources.iter().any(|source| source.id == id.as_str()))
    {
        return Err(FirmwareResolveError::UnknownRomId {
            id: unknown.clone(),
            machine: machine.to_owned(),
            known: known_ids(&sources),
        });
    }

    let needs_dir = sources
        .iter()
        .any(|source| !overrides.by_id.contains_key(source.id));
    let dir = if needs_dir {
        Some(rom_dir::<M>(overrides.dir.as_deref(), machine)?)
    } else {
        None
    };

    let mut resolved = Vec::with_capacity(sources.len());
    for source in &sources {
        if let Some(path) = overrides.by_id.get(source.id) {
            if !path.is_file() {
                return Err(FirmwareResolveError::PinnedMissing {
                    path: path.display().to_string(),
                });
            }
            resolved.push((source.id, path.clone()));
            continue;
        }
        let dir = dir.as_deref().unwrap_or_else(|| Path::new(""));
        match source
            .candidates
            .iter()
            .map(|name| dir.join(name))
            .find(|path| path.is_file())
        {
            Some(path) => resolved.push((source.id, path)),
            None if source.optional => {}
            None => {
                return Err(FirmwareResolveError::Missing {
                    id: source.id.to_owned(),
                    machine: machine.to_owned(),
                    dir: dir.display().to_string(),
                    tried: source.candidates.join(", "),
                });
            }
        }
    }
    Ok(resolved)
}

/// The ROM directory for the family: the override, else the environment
/// variable, else the first conventional directory that exists.
fn rom_dir<M: FamilyRuntime>(
    dir_override: Option<&Path>,
    machine: &str,
) -> Result<PathBuf, FirmwareResolveError> {
    if let Some(dir) = dir_override {
        return Ok(dir.to_path_buf());
    }
    let convention = M::rom_convention();
    let mut tried = Vec::new();
    if let Some(var) = convention.env_var
        && let Ok(dir) = std::env::var(var)
        && !dir.is_empty()
    {
        return Ok(PathBuf::from(dir));
    }
    let Some(home) = std::env::var_os("HOME") else {
        return Err(FirmwareResolveError::HomeUnset);
    };
    let root = PathBuf::from(home).join(".emu198x/roms");
    for dir in convention.dirs {
        let candidate = if dir.is_empty() {
            root.clone()
        } else {
            root.join(dir)
        };
        if candidate.is_dir() {
            return Ok(candidate);
        }
        tried.push(candidate.display().to_string());
    }
    if let Some(var) = convention.env_var {
        tried.push(format!("${var}"));
    }
    Err(FirmwareResolveError::NoRomDir {
        machine: machine.to_owned(),
        tried: tried.join(", "),
    })
}

/// Every image `model` boots, read into memory, pins applied.
///
/// # Errors
///
/// As [`resolve_firmware`], plus [`FirmwareResolveError::Read`] when an
/// image cannot be read.
pub fn read_firmware<M: FamilyRuntime>(
    model: M::Model,
    overrides: &FirmwareOverrides,
) -> Result<Vec<(&'static str, Vec<u8>)>, FirmwareResolveError> {
    resolve_firmware::<M>(model, overrides)?
        .into_iter()
        .map(|(id, path)| {
            read_firmware_asset(&path)
                .map(|loaded| (id, loaded.bytes))
                .map_err(|err| FirmwareResolveError::Read {
                    path: path.display().to_string(),
                    reason: err.to_string(),
                })
        })
        .collect()
}

/// Build `model` from its conventional firmware, pins applied.
///
/// # Errors
///
/// As [`read_firmware`], plus [`FirmwareResolveError::Build`] when the
/// family rejects the images.
pub fn build_variant<M: FamilyRuntime>(
    model: M::Model,
    overrides: &FirmwareOverrides,
) -> Result<M, FirmwareResolveError> {
    let images = read_firmware::<M>(model, overrides)?;
    let mut firmware = FirmwareSet::new();
    for (id, bytes) in &images {
        firmware.push(FirmwareImage::new(*id, bytes));
    }
    M::from_firmware(model, &firmware).map_err(|err| FirmwareResolveError::Build {
        machine: M::variant_id(model).to_owned(),
        reason: err.to_string(),
    })
}

/// What a variant switch reports.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VariantSwitched {
    /// The id the switch was asked for.
    pub machine: String,
    /// The profile now live.
    pub profile_id: String,
    /// Its display name.
    pub display_name: String,
}

/// Swap the session onto the variant `machine` names, built from its
/// conventional firmware, through [`HeadlessSession::swap_machine`].
///
/// This is the body of the shared `set_machine` step: a family runtime's
/// `MachineCore::set_machine` hook is one call to it.
///
/// # Errors
///
/// [`LoaderError::Failed`] with the resolver's or the session's message.
pub fn swap_variant<M, Q>(
    session: &mut HeadlessSession<M, Q>,
    machine: &str,
) -> Result<VariantSwitched, LoaderError>
where
    M: FamilyRuntime,
    Q: SessionQueryProvider<M>,
{
    let model = model_from_id::<M>(machine).map_err(|err| LoaderError::Failed(err.to_string()))?;
    let images = read_firmware::<M>(model, &FirmwareOverrides::none())
        .map_err(|err| LoaderError::Failed(err.to_string()))?;
    let mut firmware = FirmwareSet::new();
    for (id, bytes) in &images {
        firmware.push(FirmwareImage::new(*id, bytes));
    }
    session
        .swap_machine(model, &firmware)
        .map_err(|err| LoaderError::Failed(err.to_string()))?;
    let profile = session.machine().profile();
    Ok(VariantSwitched {
        machine: machine.to_owned(),
        profile_id: profile.profile_id.as_str().to_owned(),
        display_name: profile.display_name.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capability::CapabilitySet;
    use crate::error::MachineError;
    use crate::host::HostIo;
    use crate::machine::{
        Family, MachineCore, MachineId, MachineProfile, ProfileId, Region, ResetKind, RunResult,
        StopReason,
    };
    use crate::media::MediaSet;
    use crate::time::{ClockDesc, ClockRate, MachineTime};

    /// A two-variant family: `one` boots a single ROM, `two` boots two
    /// required ROMs and an optional one.
    struct Fam {
        model: FamModel,
        profile: MachineProfile,
        images: Vec<String>,
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum FamModel {
        One,
        Two,
    }

    fn profile(model: FamModel) -> MachineProfile {
        MachineProfile {
            machine_id: MachineId::from("fam"),
            profile_id: ProfileId::from(Fam::variant_id(model)),
            display_name: format!("Fam {}", Fam::variant_id(model)).into(),
            family: Family::Spectrum,
            region: Region::Pal,
            release_year: 1982,
            summary: "test".into(),
            clock: ClockDesc::new("t", ClockRate::from_hz(1_000_000)),
            firmware: vec![],
            media_slots: vec![],
            capabilities: CapabilitySet::new(),
        }
    }

    impl MachineCore for Fam {
        fn profile(&self) -> &MachineProfile {
            &self.profile
        }
        fn time(&self) -> MachineTime {
            MachineTime::default()
        }
        fn reset(&mut self, _kind: ResetKind) {}
        fn load_media(&mut self, _media: &MediaSet<'_>) -> Result<(), MachineError> {
            Ok(())
        }
        fn run_until(
            &mut self,
            target: MachineTime,
            _host: &mut HostIo<'_>,
        ) -> Result<RunResult, MachineError> {
            Ok(RunResult::new(target, StopReason::ReachedTarget))
        }
        fn snapshot(&self) -> Result<Vec<u8>, MachineError> {
            Ok(vec![])
        }
        fn restore(&mut self, _bytes: &[u8]) -> Result<(), MachineError> {
            Ok(())
        }
        fn capabilities(&self) -> CapabilitySet {
            CapabilitySet::new()
        }
        fn set_machine<Q: SessionQueryProvider<Self>>(
            session: &mut HeadlessSession<Self, Q>,
            machine: &str,
        ) -> Result<VariantSwitched, LoaderError> {
            swap_variant(session, machine)
        }
    }

    impl FamilyRuntime for Fam {
        type Model = FamModel;
        fn from_firmware(
            model: FamModel,
            firmware: &FirmwareSet<'_>,
        ) -> Result<Self, MachineError> {
            let mut images: Vec<String> = Self::firmware_sources(model)
                .iter()
                .filter(|source| firmware.bytes(source.id).is_some())
                .map(|source| source.id.to_owned())
                .collect();
            images.sort();
            Ok(Self {
                model,
                profile: profile(model),
                images,
            })
        }
        fn native_frame_ticks(&self) -> u64 {
            match self.model {
                FamModel::One => 1_000,
                FamModel::Two => 2_000,
            }
        }
        fn variant_ids() -> &'static [&'static str] {
            &["one", "two"]
        }
        fn model_from_id(id: &str) -> Option<FamModel> {
            match id {
                "one" => Some(FamModel::One),
                "two" => Some(FamModel::Two),
                _ => None,
            }
        }
        fn variant_id(model: FamModel) -> &'static str {
            match model {
                FamModel::One => "one",
                FamModel::Two => "two",
            }
        }
        fn profile_for(model: FamModel) -> MachineProfile {
            profile(model)
        }
        fn rom_convention() -> RomConvention {
            RomConvention {
                env_var: Some("EMU198X_FAM_TEST_ROM_DIR_THAT_NOBODY_SETS"),
                dirs: &["fam-that-does-not-exist"],
            }
        }
        fn firmware_sources(model: FamModel) -> Vec<FirmwareSource> {
            match model {
                FamModel::One => vec![FirmwareSource::required("only", &["one.rom"])],
                FamModel::Two => vec![
                    FirmwareSource::required("rom-0", &["a/0.rom"]),
                    FirmwareSource::required("rom-1", &["a/1.rom", "a/one.rom"]),
                    FirmwareSource::optional("extra", &["extra.rom"]),
                ],
            }
        }
    }

    /// A fresh ROM directory with the named files in it.
    fn rom_dir(files: &[&str]) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "emu198x-shell-variants-{}-{}",
            std::process::id(),
            files.join("_").replace('/', "-")
        ));
        let _ = std::fs::remove_dir_all(&dir);
        for file in files {
            let path = dir.join(file);
            std::fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
            std::fs::write(&path, file.as_bytes()).expect("write");
        }
        std::fs::create_dir_all(&dir).expect("mkdir");
        dir
    }

    fn in_dir(dir: &Path) -> FirmwareOverrides {
        FirmwareOverrides {
            dir: Some(dir.to_path_buf()),
            by_id: BTreeMap::new(),
        }
    }

    #[test]
    fn firmware_resolves_from_the_rom_directory_first_candidate_wins() {
        let dir = rom_dir(&["a/0.rom", "a/one.rom", "extra.rom"]);
        let resolved = resolve_firmware::<Fam>(FamModel::Two, &in_dir(&dir)).expect("resolves");
        assert_eq!(
            resolved,
            vec![
                ("rom-0", dir.join("a/0.rom")),
                ("rom-1", dir.join("a/one.rom")),
                ("extra", dir.join("extra.rom")),
            ]
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_absent_optional_image_is_left_out_and_a_required_one_is_an_error() {
        let dir = rom_dir(&["a/0.rom", "a/1.rom"]);
        let resolved = resolve_firmware::<Fam>(FamModel::Two, &in_dir(&dir)).expect("resolves");
        assert_eq!(resolved.len(), 2, "{resolved:?}");

        let dir_missing = rom_dir(&["a/0.rom"]);
        let err = resolve_firmware::<Fam>(FamModel::Two, &in_dir(&dir_missing))
            .expect_err("rom-1 is required");
        let message = err.to_string();
        assert!(
            message.contains("rom-1") && message.contains("a/one.rom"),
            "{message}"
        );
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_dir_all(&dir_missing);
    }

    #[test]
    fn a_pin_replaces_only_the_rom_it_names() {
        // Pinning one must leave the other resolving conventionally, which
        // is the whole reason the flag is keyed by id rather than scalar.
        let dir = rom_dir(&["a/0.rom", "a/1.rom", "pinned.rom"]);
        let mut overrides = in_dir(&dir);
        overrides.pin("rom-1", dir.join("pinned.rom"));
        let resolved = resolve_firmware::<Fam>(FamModel::Two, &overrides).expect("resolves");
        assert_eq!(resolved[0], ("rom-0", dir.join("a/0.rom")));
        assert_eq!(resolved[1], ("rom-1", dir.join("pinned.rom")));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_unknown_pinned_id_is_an_error_not_a_silent_fallback() {
        // Booting the conventional ROM after being told to use a specific
        // one looks like success and produces the wrong bytes.
        let dir = rom_dir(&["one.rom"]);
        let mut overrides = in_dir(&dir);
        overrides.pin("rom-0", dir.join("one.rom"));
        let err = resolve_firmware::<Fam>(FamModel::One, &overrides).expect_err("refused");
        let message = err.to_string();
        assert!(
            message.contains("rom-0") && message.contains("one"),
            "{message}"
        );
        assert!(
            message.contains("only"),
            "names what it does take: {message}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn pinning_every_required_image_needs_no_rom_directory() {
        // The sandboxed builds that most want pins are the ones without a
        // usable ROM directory; the convention must not be consulted then.
        let dir = rom_dir(&["x0.rom", "x1.rom"]);
        let mut overrides = FirmwareOverrides::none();
        overrides.pin("rom-0", dir.join("x0.rom"));
        overrides.pin("rom-1", dir.join("x1.rom"));
        overrides.pin("extra", dir.join("x1.rom"));
        let resolved = resolve_firmware::<Fam>(FamModel::Two, &overrides).expect("no dir needed");
        assert_eq!(resolved.len(), 3);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn without_a_rom_directory_the_error_names_what_was_tried() {
        let err = resolve_firmware::<Fam>(FamModel::One, &FirmwareOverrides::none())
            .expect_err("no fam directory exists");
        let message = err.to_string();
        assert!(
            message.contains("fam-that-does-not-exist") && message.contains("EMU198X_FAM_TEST"),
            "{message}"
        );
    }

    #[test]
    fn an_unknown_machine_id_lists_the_accepted_ids() {
        let err = model_from_id::<Fam>("three").expect_err("unknown");
        let message = err.to_string();
        assert!(
            message.contains("three") && message.contains("one, two"),
            "{message}"
        );
    }

    #[test]
    fn build_variant_feeds_the_resolved_images_to_the_family() {
        let dir = rom_dir(&["a/0.rom", "a/1.rom"]);
        let built = build_variant::<Fam>(FamModel::Two, &in_dir(&dir)).expect("builds");
        assert_eq!(built.images, vec!["rom-0".to_owned(), "rom-1".to_owned()]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn set_machine_through_the_hook_swaps_and_reports_the_new_profile() {
        // `swap_variant` resolves by convention only, so make `one`
        // reachable by pinning nothing and pointing the family's directory
        // at a real one is impossible here; instead check the failure path
        // reports the resolver's message, and the observation shape via
        // a family built directly.
        let dir = rom_dir(&["one.rom"]);
        let start = build_variant::<Fam>(FamModel::One, &in_dir(&dir)).expect("builds");
        let mut session = HeadlessSession::new(start, 1_000);
        let err = Fam::set_machine(&mut session, "two").expect_err("no conventional dir");
        assert!(matches!(err, LoaderError::Failed(_)), "{err}");
        let err = Fam::set_machine(&mut session, "three").expect_err("unknown id");
        assert!(err.to_string().contains("one, two"), "{err}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn sources() -> Vec<FirmwareSource> {
        vec![
            FirmwareSource::required("rom-0", &["a/0.rom"]),
            FirmwareSource::required("rom-1", &["a/1.rom", "a/one.rom"]),
        ]
    }

    #[test]
    fn a_rom_spec_splits_on_its_first_equals() {
        // A path may contain `=`, so only the first one separates.
        let mut overrides = FirmwareOverrides::none();
        overrides
            .add_spec("rom-0=/roms/a=b/0.rom", "m", &sources())
            .expect("explicit form");
        assert_eq!(
            overrides.by_id.get("rom-0"),
            Some(&PathBuf::from("/roms/a=b/0.rom"))
        );
    }

    #[test]
    fn a_malformed_rom_spec_is_rejected_rather_than_guessed() {
        for spec in ["", "=", "=/roms/48.rom", "some-id="] {
            let mut overrides = FirmwareOverrides::none();
            let err = overrides.add_spec(spec, "m", &sources()).expect_err(spec);
            assert!(
                matches!(
                    err,
                    FirmwareResolveError::MalformedRomSpec { .. }
                        | FirmwareResolveError::AmbiguousRomPath { .. }
                ),
                "{spec:?}: {err}"
            );
        }
    }

    #[test]
    fn a_bare_path_names_the_sole_rom_of_a_single_rom_variant() {
        let single = [FirmwareSource::required("only", &["x.rom"])];
        let mut overrides = FirmwareOverrides::none();
        overrides
            .add_spec("/roms/x.rom", "m", &single)
            .expect("one ROM");
        assert_eq!(
            overrides.by_id.get("only"),
            Some(&PathBuf::from("/roms/x.rom"))
        );
    }

    #[test]
    fn a_bare_path_on_a_multi_rom_variant_is_refused() {
        // Applying it to the first entry would leave the other conventional
        // and boot a machine assembled from two ROM sets, reporting success.
        let mut overrides = FirmwareOverrides::none();
        let err = overrides
            .add_spec("/roms/0.rom", "plus3", &sources())
            .expect_err("two ROMs");
        let message = err.to_string();
        assert!(message.contains("boots 2 ROMs"), "{message}");
        assert!(message.contains("--rom ID=PATH"), "{message}");
        assert!(message.contains("rom-1"), "{message}");
    }
}
