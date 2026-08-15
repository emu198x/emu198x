//! Shared machine vocabulary used by every mode of `emu198x-spectrum`.
//!
//! `MachineKind` enumerates all 13 Spectrum-family variants — the SOLID
//! 8 (16K, 48K, +, 128K, +2, +2A, +2B, +3) plus the five exotics
//! (Pentagon 128, Scorpion ZS-256, Timex TC2048 / TC2068 / TS2068).
//! `ui::menu` drives the Machine submenu off this enum; `script` uses
//! the same enum for `set_machine` step dispatch. Each variant maps to
//! a stable snake-case identifier the script JSON uses.
//!
//! ROM resolution lives here too — both modes reach for the same
//! `~/.emu198x/roms/<system>/<file>.rom` convention shared with the
//! goldens harness in `runtime-sinclair-zx-spectrum/tests/goldens.rs`.

// Most of this module's surface is consumed by `ui::menu` (label, all)
// and the forthcoming script-mode SetMachine handler (script_id,
// from_script_id, read_variant_firmware). On `--no-default-features`
// headless builds with SetMachine support not yet wired, those items
// are dead code; suppress the warning so CI's `-D warnings` flag
// doesn't flip on something that's transient.
#![cfg_attr(not(feature = "ui"), allow(dead_code))]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Every Spectrum-family variant: the SOLID 8 plus the five exotics.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MachineKind {
    Spectrum16K,
    Spectrum48K,
    SpectrumPlus,
    Spectrum128K,
    SpectrumPlus2,
    SpectrumPlus2A,
    SpectrumPlus2B,
    SpectrumPlus3,
    Pentagon128,
    ScorpionZS256,
    TimexTC2048,
    TimexTC2068,
    TimexTS2068,
}

impl MachineKind {
    /// Display label used in the Machine menu.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Spectrum16K => "ZX Spectrum 16K",
            Self::Spectrum48K => "ZX Spectrum 48K",
            Self::SpectrumPlus => "ZX Spectrum+",
            Self::Spectrum128K => "ZX Spectrum 128",
            Self::SpectrumPlus2 => "ZX Spectrum +2",
            Self::SpectrumPlus2A => "ZX Spectrum +2A",
            Self::SpectrumPlus2B => "ZX Spectrum +2B",
            Self::SpectrumPlus3 => "ZX Spectrum +3",
            Self::Pentagon128 => "Pentagon 128",
            Self::ScorpionZS256 => "Scorpion ZS-256",
            Self::TimexTC2048 => "Timex TC2048",
            Self::TimexTC2068 => "Timex TC2068",
            Self::TimexTS2068 => "Timex TS2068",
        }
    }

    /// Snake-case identifier used by `set_machine` script steps.
    #[allow(dead_code)] // wired when script-mode SetMachine support lands
    pub const fn script_id(self) -> &'static str {
        match self {
            Self::Spectrum16K => "spectrum_16k",
            Self::Spectrum48K => "spectrum_48k",
            Self::SpectrumPlus => "spectrum_plus",
            Self::Spectrum128K => "spectrum_128k",
            Self::SpectrumPlus2 => "spectrum_plus2",
            Self::SpectrumPlus2A => "spectrum_plus2a",
            Self::SpectrumPlus2B => "spectrum_plus2b",
            Self::SpectrumPlus3 => "spectrum_plus3",
            Self::Pentagon128 => "pentagon_128",
            Self::ScorpionZS256 => "scorpion_zs256",
            Self::TimexTC2048 => "timex_tc2048",
            Self::TimexTC2068 => "timex_tc2068",
            Self::TimexTS2068 => "timex_ts2068",
        }
    }

    /// Parses a snake-case identifier from a script step or the
    /// `--machine` CLI flag.
    #[must_use]
    pub fn from_script_id(id: &str) -> Option<Self> {
        Some(match id {
            "spectrum_16k" => Self::Spectrum16K,
            "spectrum_48k" => Self::Spectrum48K,
            "spectrum_plus" => Self::SpectrumPlus,
            "spectrum_128k" => Self::Spectrum128K,
            "spectrum_plus2" => Self::SpectrumPlus2,
            "spectrum_plus2a" => Self::SpectrumPlus2A,
            "spectrum_plus2b" => Self::SpectrumPlus2B,
            "spectrum_plus3" => Self::SpectrumPlus3,
            "pentagon_128" => Self::Pentagon128,
            "scorpion_zs256" => Self::ScorpionZS256,
            "timex_tc2048" => Self::TimexTC2048,
            "timex_tc2068" => Self::TimexTC2068,
            "timex_ts2068" => Self::TimexTS2068,
            _ => return None,
        })
    }

    /// Every accepted script identifier, comma-separated, in
    /// [`Self::all`] order. Derived from the enum rather than written
    /// out, so an added variant can't leave a stale list behind in an
    /// error message — which is exactly what happened to the
    /// `set_machine` tool, whose hand-written list named only the SOLID
    /// 8 while `from_script_id` accepted all 13.
    #[must_use]
    pub fn script_id_list() -> String {
        Self::all()
            .iter()
            .map(|kind| kind.script_id())
            .collect::<Vec<_>>()
            .join(", ")
    }

    /// All variants in catalogue order: SOLID 8 (16K → 48K → Plus →
    /// 128K → +2 → +2A → +2B → +3) followed by the five exotics
    /// (Pentagon → Scorpion → TC2048 → TC2068 → TS2068). Stable
    /// iteration order matters for the menu layout and the radio-style
    /// "current" indicator. On Linux the muda menu is gated out (see
    /// ui/menu.rs), so nothing iterates this — suppress the dead_code
    /// lint there only.
    #[cfg_attr(target_os = "linux", allow(dead_code))]
    pub const fn all() -> [Self; 13] {
        [
            Self::Spectrum16K,
            Self::Spectrum48K,
            Self::SpectrumPlus,
            Self::Spectrum128K,
            Self::SpectrumPlus2,
            Self::SpectrumPlus2A,
            Self::SpectrumPlus2B,
            Self::SpectrumPlus3,
            Self::Pentagon128,
            Self::ScorpionZS256,
            Self::TimexTC2048,
            Self::TimexTC2068,
            Self::TimexTS2068,
        ]
    }
}

/// `~/.emu198x/roms` — on-disk firmware bundle convention shared with
/// `runtime-sinclair-zx-spectrum`'s golden tests. `None` only when
/// `HOME` is unset.
pub fn rom_root() -> Option<PathBuf> {
    std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".emu198x/roms"))
}

/// Per-variant ROM bundle: list of `(firmware_id, on-disk path)` pairs
/// the runtime crate's `from_firmware` constructor expects.
///
/// 16K/48K/+ share `sinclair-zx-spectrum-48k/48.rom`; 128K, +2, +2A/+3,
/// and +2B each have their own bundle directory. The +2A and +3 share
/// `amstrad-zx-spectrum-plus3/plus3-{0..3}.rom` (ROM v4.0); the +2B
/// uses its own `amstrad-zx-spectrum-plus2b/plus3-{0..3}.rom` (ROM v4.1).
/// The exotics live under their own `pentagon-128/`, `scorpion-zs256/`,
/// `timex-tc2048/`, and `timex-ts2068/` directories.
pub fn variant_rom_bundle(kind: MachineKind, root: &Path) -> Vec<(&'static str, PathBuf)> {
    match kind {
        MachineKind::Spectrum16K | MachineKind::Spectrum48K | MachineKind::SpectrumPlus => vec![(
            "sinclair-zx-spectrum-48k-rom",
            root.join("sinclair-zx-spectrum-48k/48.rom"),
        )],
        MachineKind::Spectrum128K => vec![
            (
                "sinclair-zx-spectrum-128k-rom-0",
                root.join("sinclair-zx-spectrum-128k/128-0.rom"),
            ),
            (
                "sinclair-zx-spectrum-128k-rom-1",
                root.join("sinclair-zx-spectrum-128k/128-1.rom"),
            ),
        ],
        MachineKind::SpectrumPlus2 => vec![
            (
                "sinclair-zx-spectrum-plus2-rom-0",
                root.join("amstrad-zx-spectrum-plus2/plus2-0.rom"),
            ),
            (
                "sinclair-zx-spectrum-plus2-rom-1",
                root.join("amstrad-zx-spectrum-plus2/plus2-1.rom"),
            ),
        ],
        MachineKind::SpectrumPlus2A | MachineKind::SpectrumPlus3 => (0..4)
            .map(|i| {
                let id: &'static str = match i {
                    0 => "sinclair-zx-spectrum-plus3-rom-0",
                    1 => "sinclair-zx-spectrum-plus3-rom-1",
                    2 => "sinclair-zx-spectrum-plus3-rom-2",
                    _ => "sinclair-zx-spectrum-plus3-rom-3",
                };
                (
                    id,
                    root.join(format!("amstrad-zx-spectrum-plus3/plus3-{i}.rom")),
                )
            })
            .collect(),
        MachineKind::SpectrumPlus2B => (0..4)
            .map(|i| {
                let id: &'static str = match i {
                    0 => "sinclair-zx-spectrum-plus3-rom-0",
                    1 => "sinclair-zx-spectrum-plus3-rom-1",
                    2 => "sinclair-zx-spectrum-plus3-rom-2",
                    _ => "sinclair-zx-spectrum-plus3-rom-3",
                };
                (
                    id,
                    root.join(format!("amstrad-zx-spectrum-plus2b/plus3-{i}.rom")),
                )
            })
            .collect(),
        MachineKind::Pentagon128 => vec![
            ("pentagon-rom-0", root.join("pentagon-128/pentagon-0.rom")),
            ("pentagon-rom-1", root.join("pentagon-128/pentagon-1.rom")),
        ],
        MachineKind::ScorpionZS256 => (0..4)
            .map(|i| {
                let id: &'static str = match i {
                    0 => "scorpion-rom-0",
                    1 => "scorpion-rom-1",
                    2 => "scorpion-rom-2",
                    _ => "scorpion-rom-3",
                };
                (id, root.join(format!("scorpion-zs256/scorpion-{i}.rom")))
            })
            .collect(),
        MachineKind::TimexTC2048 => {
            vec![("timex-tc2048-rom", root.join("timex-tc2048/tc2048.rom"))]
        }
        MachineKind::TimexTC2068 | MachineKind::TimexTS2068 => vec![
            ("timex-ts2068-rom-0", root.join("timex-ts2068/ts2068.rom")),
            ("timex-ts2068-rom-1", root.join("timex-ts2068/exrom.rom")),
        ],
    }
}

/// Error surfaced when locating or reading variant firmware fails.
#[derive(Debug, thiserror::Error)]
pub enum FirmwareError {
    /// `HOME` environment variable is unset; cannot locate
    /// `~/.emu198x/roms`.
    #[error("$HOME unset; cannot locate ROM bundle")]
    HomeUnset,

    /// One required ROM file is missing on disk.
    #[error("ROM not found at {path}")]
    Missing {
        /// Absolute path the binary tried to read.
        path: String,
    },

    /// A `--rom ID=PATH` override named an ID this variant does not have.
    #[error("no ROM `{id}` on {machine}; this variant takes: {known}")]
    UnknownRomId {
        /// The firmware ID the caller asked to override.
        id: String,
        /// The variant's script identifier, e.g. `spectrum_128k`.
        machine: &'static str,
        /// The IDs this variant does take, comma-separated.
        known: String,
    },

    /// A bare `--rom PATH` was given for a variant that boots several
    /// ROMs, so which one it meant is unknowable.
    #[error(
        "{machine} boots {count} ROMs, so a bare --rom PATH is ambiguous; \
         use --rom ID=PATH with one of: {known}"
    )]
    AmbiguousRomPath {
        /// The variant's script identifier.
        machine: &'static str,
        /// How many ROMs the variant boots.
        count: usize,
        /// The IDs this variant takes, comma-separated.
        known: String,
    },

    /// Filesystem read failed for one ROM.
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

/// Caller-supplied ROM paths, keyed by the firmware ID
/// [`variant_rom_bundle`] uses.
///
/// Empty is the ordinary case: every ROM resolves under [`rom_root`].
/// An entry replaces exactly one bundle path and leaves the rest alone,
/// which is what this family needs — a +3 boots four ROMs, so a single
/// scalar `--rom` could not express "this one, conventional for the
/// other three" (#842).
pub type RomOverrides = BTreeMap<String, PathBuf>;

/// Parse one `--rom ID=PATH` spec.
///
/// Splits on the first `=` only, because a path may contain one.
/// Returns `None` for a malformed spec so the caller can report it with
/// its own usage text.
#[must_use]
pub fn parse_rom_override_spec(spec: &str) -> Option<(String, PathBuf)> {
    let (id, path) = spec.split_once('=')?;
    if id.is_empty() || path.is_empty() {
        return None;
    }
    Some((id.to_owned(), PathBuf::from(path)))
}

/// Turn one `--rom` value into an override entry against `kind`'s bundle.
///
/// Accepts both spellings so one flag means one thing across every mode:
///
/// - `ID=PATH` names the bundle entry explicitly, and is the only form
///   that can express a multi-ROM variant.
/// - a bare `PATH` is sugar for the sole ROM of a single-ROM variant.
///
/// # Errors
///
/// [`FirmwareError::AmbiguousRomPath`] for a bare path on a variant with
/// more than one ROM. Silently applying it to the first entry leaves the
/// rest conventional, which boots a machine assembled from two different
/// ROM sets and reports success.
pub fn rom_override_entry(
    spec: &str,
    kind: MachineKind,
) -> Result<(String, PathBuf), FirmwareError> {
    if let Some(entry) = parse_rom_override_spec(spec) {
        return Ok(entry);
    }
    let bundle = variant_rom_bundle(kind, Path::new(""));
    match bundle.as_slice() {
        [(id, _)] => Ok(((*id).to_owned(), PathBuf::from(spec))),
        _ => Err(FirmwareError::AmbiguousRomPath {
            machine: kind.script_id(),
            count: bundle.len(),
            known: bundle
                .iter()
                .map(|(id, _)| *id)
                .collect::<Vec<_>>()
                .join(", "),
        }),
    }
}

/// Apply `overrides` to a bundle, replacing paths by firmware ID.
///
/// # Errors
///
/// [`FirmwareError::UnknownRomId`] if an override names an ID the
/// variant does not have. Ignoring it would silently boot the
/// conventional ROM after being told to use a specific one — the
/// failure mode this flag exists to prevent, and one that looks like a
/// success.
pub fn apply_rom_overrides(
    bundle: Vec<(&'static str, PathBuf)>,
    overrides: &RomOverrides,
    kind: MachineKind,
) -> Result<Vec<(&'static str, PathBuf)>, FirmwareError> {
    if let Some(unknown) = overrides.keys().find(|id| {
        !bundle
            .iter()
            .any(|(bundle_id, _)| *bundle_id == id.as_str())
    }) {
        return Err(FirmwareError::UnknownRomId {
            id: unknown.clone(),
            machine: kind.script_id(),
            known: bundle
                .iter()
                .map(|(id, _)| *id)
                .collect::<Vec<_>>()
                .join(", "),
        });
    }
    Ok(bundle
        .into_iter()
        .map(|(id, path)| match overrides.get(id) {
            Some(override_path) => (id, override_path.clone()),
            None => (id, path),
        })
        .collect())
}

/// The variant's ROM bundle with any overrides applied.
///
/// # Errors
///
/// [`FirmwareError::HomeUnset`] if `HOME` is unset *and* the overrides
/// leave some ROM to resolve conventionally. An invocation that names
/// every ROM explicitly has no use for the convention root, and failing
/// it there would make the flag useless in exactly the sandboxed builds
/// that most want it. [`FirmwareError::UnknownRomId`] for an override
/// naming no ROM of this variant.
pub fn resolved_rom_bundle(
    kind: MachineKind,
    overrides: &RomOverrides,
) -> Result<Vec<(&'static str, PathBuf)>, FirmwareError> {
    // `rom_root` fails only with HOME unset. Build against a placeholder
    // in that case: if the overrides name every ROM no placeholder path
    // survives, and if they do not, the check below names the gap.
    let root = rom_root();
    let bundle = variant_rom_bundle(kind, root.as_deref().unwrap_or_else(|| Path::new("")));
    let resolved = apply_rom_overrides(bundle, overrides, kind)?;
    if root.is_none() && resolved.iter().any(|(id, _)| !overrides.contains_key(*id)) {
        return Err(FirmwareError::HomeUnset);
    }
    Ok(resolved)
}

/// Reads every ROM the variant declares from disk into owned byte
/// vectors. Caller assembles a `FirmwareSet` by borrowing into the
/// returned vec — the bytes must outlive the set, so the caller holds
/// both.
pub fn read_variant_firmware(
    kind: MachineKind,
) -> Result<Vec<(&'static str, Vec<u8>)>, FirmwareError> {
    let root = rom_root().ok_or(FirmwareError::HomeUnset)?;
    let bundle = variant_rom_bundle(kind, &root);
    let mut images = Vec::with_capacity(bundle.len());
    for (id, path) in bundle {
        if !path.is_file() {
            return Err(FirmwareError::Missing {
                path: path.display().to_string(),
            });
        }
        let bytes = std::fs::read(&path)?;
        images.push((id, bytes));
    }
    Ok(images)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn script_id_round_trips_for_every_variant() {
        for kind in MachineKind::all() {
            assert_eq!(MachineKind::from_script_id(kind.script_id()), Some(kind));
        }
    }

    #[test]
    fn script_id_returns_none_for_unknown_variant() {
        assert_eq!(MachineKind::from_script_id("spectrum_999k"), None);
        assert_eq!(MachineKind::from_script_id(""), None);
    }

    #[test]
    fn a_rom_spec_splits_on_its_first_equals() {
        // A path may contain `=`, so only the first one separates.
        assert_eq!(
            parse_rom_override_spec("sinclair-zx-spectrum-48k-rom=/roms/a=b/48.rom"),
            Some((
                "sinclair-zx-spectrum-48k-rom".to_owned(),
                PathBuf::from("/roms/a=b/48.rom")
            ))
        );
    }

    #[test]
    fn a_malformed_rom_spec_is_rejected_rather_than_guessed() {
        for spec in ["", "=", "no-equals-here", "=/roms/48.rom", "some-id="] {
            assert_eq!(parse_rom_override_spec(spec), None, "{spec:?}");
        }
    }

    #[test]
    fn an_override_replaces_only_the_rom_it_names() {
        // The 128K takes two ROMs; pinning one must leave the other
        // resolving conventionally, which is the whole reason the flag is
        // keyed by ID rather than being a scalar `--rom`.
        let root = Path::new("/conventional");
        let bundle = variant_rom_bundle(MachineKind::Spectrum128K, root);
        let mut overrides = RomOverrides::new();
        overrides.insert(
            "sinclair-zx-spectrum-128k-rom-1".to_owned(),
            PathBuf::from("/pinned/128-1.rom"),
        );

        let resolved = apply_rom_overrides(bundle, &overrides, MachineKind::Spectrum128K)
            .expect("a known id resolves");
        assert_eq!(resolved.len(), 2);
        assert_eq!(resolved[0].0, "sinclair-zx-spectrum-128k-rom-0");
        assert_eq!(
            resolved[0].1,
            root.join("sinclair-zx-spectrum-128k/128-0.rom"),
            "the un-named ROM must still resolve conventionally"
        );
        assert_eq!(resolved[1].1, PathBuf::from("/pinned/128-1.rom"));
    }

    #[test]
    fn an_unknown_rom_id_is_an_error_not_a_silent_fallback() {
        // Booting the conventional ROM after being told to use a specific
        // one is the failure this flag exists to prevent: it looks like a
        // success and produces the wrong bytes (#842).
        let bundle = variant_rom_bundle(MachineKind::Spectrum48K, Path::new("/conventional"));
        let mut overrides = RomOverrides::new();
        overrides.insert(
            "sinclair-zx-spectrum-128k-rom-0".to_owned(),
            PathBuf::from("/pinned/128-0.rom"),
        );

        let err = apply_rom_overrides(bundle, &overrides, MachineKind::Spectrum48K)
            .expect_err("a 128K id on a 48K boot should be refused");
        let message = err.to_string();
        assert!(
            message.contains("sinclair-zx-spectrum-128k-rom-0"),
            "{message}"
        );
        assert!(message.contains("spectrum_48k"), "{message}");
        // The error names what the variant does take, so the fix is visible.
        assert!(
            message.contains("sinclair-zx-spectrum-48k-rom"),
            "{message}"
        );
    }

    #[test]
    fn every_variants_bundle_ids_are_overridable() {
        // A caller can only pin what the bundle names, so every ID the
        // bundle produces must round-trip through `apply_rom_overrides`.
        // Catches a variant added with an ID that nothing can address.
        for kind in MachineKind::all() {
            let bundle = variant_rom_bundle(kind, Path::new("/conventional"));
            assert!(!bundle.is_empty(), "{} has no ROMs", kind.script_id());
            let overrides: RomOverrides = bundle
                .iter()
                .map(|(id, _)| ((*id).to_owned(), PathBuf::from(format!("/pinned/{id}"))))
                .collect();
            let resolved = apply_rom_overrides(bundle, &overrides, kind)
                .unwrap_or_else(|err| panic!("{}: {err}", kind.script_id()));
            for (id, path) in resolved {
                assert_eq!(
                    path,
                    PathBuf::from(format!("/pinned/{id}")),
                    "{} did not honour {id}",
                    kind.script_id()
                );
            }
        }
    }

    #[test]
    fn a_bare_path_names_the_sole_rom_of_a_single_rom_variant() {
        // The UI's `--rom 48.rom` spelling, which predates the flag taking
        // IDs and must keep working.
        let (id, path) = rom_override_entry("/roms/48.rom", MachineKind::Spectrum48K)
            .expect("48K takes one ROM");
        assert_eq!(id, "sinclair-zx-spectrum-48k-rom");
        assert_eq!(path, PathBuf::from("/roms/48.rom"));
    }

    #[test]
    fn a_bare_path_on_a_multi_rom_variant_is_refused() {
        // Applying it to the first entry would leave the other three
        // conventional and boot a machine assembled from two ROM sets,
        // reporting success. That is what the UI's `--rom` used to do.
        let err = rom_override_entry("/roms/plus3-0.rom", MachineKind::SpectrumPlus3)
            .expect_err("a +3 boots four ROMs");
        let message = err.to_string();
        assert!(message.contains("boots 4 ROMs"), "{message}");
        assert!(message.contains("--rom ID=PATH"), "{message}");
        assert!(
            message.contains("sinclair-zx-spectrum-plus3-rom-3"),
            "{message}"
        );
    }

    #[test]
    fn an_id_spelling_works_on_a_single_rom_variant_too() {
        // One flag, one meaning across every variant: the explicit form is
        // never wrong, so a caller can always use it.
        let (id, path) = rom_override_entry(
            "sinclair-zx-spectrum-48k-rom=/roms/48.rom",
            MachineKind::Spectrum48K,
        )
        .expect("the explicit form is always accepted");
        assert_eq!(id, "sinclair-zx-spectrum-48k-rom");
        assert_eq!(path, PathBuf::from("/roms/48.rom"));
    }

    #[test]
    fn overriding_the_whole_bundle_does_not_need_home() {
        // The sandboxed builds that most want this flag are exactly the
        // ones without a usable `$HOME`; requiring the convention root
        // there would make the flag useless where it matters most.
        //
        // `rom_root` reads the process environment, so this test cannot
        // manipulate it safely alongside others. Assert the same property
        // through the piece that decides it instead: with every ID named,
        // no conventional path survives, so nothing needs the root.
        let bundle = variant_rom_bundle(MachineKind::SpectrumPlus3, Path::new(""));
        let overrides: RomOverrides = bundle
            .iter()
            .map(|(id, _)| ((*id).to_owned(), PathBuf::from(format!("/pinned/{id}"))))
            .collect();
        let resolved = apply_rom_overrides(bundle, &overrides, MachineKind::SpectrumPlus3)
            .expect("all four ids are known");
        assert_eq!(resolved.len(), 4);
        assert!(
            resolved.iter().all(|(_, path)| path.starts_with("/pinned")),
            "a conventional path survived: {resolved:?}"
        );
    }
}
