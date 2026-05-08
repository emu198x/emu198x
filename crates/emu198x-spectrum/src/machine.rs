//! Shared machine vocabulary used by every mode of `emu198x-spectrum`.
//!
//! `MachineKind` enumerates the 8 in-scope SOLID variants. `ui::menu`
//! drives the Machine submenu off this enum; `script` uses the same
//! enum for `set_machine` step dispatch. Each variant maps to a stable
//! snake-case identifier the script JSON uses.
//!
//! ROM resolution lives here too — both modes reach for the same
//! `~/.emu198x/roms/<system>/<file>.rom` convention shared with the
//! goldens harness in `runtime-sinclair-zx-spectrum/tests/goldens.rs`.

use std::path::{Path, PathBuf};

/// The eight in-scope October-public variants.
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
        }
    }

    /// Snake-case identifier used by `set_machine` script steps.
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
        }
    }

    /// Parses a snake-case identifier from a script step.
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
            _ => return None,
        })
    }

    /// All eight variants in catalogue order (16K → 48K → Plus → 128K
    /// → +2 → +2A → +2B → +3). Stable iteration order matters for the
    /// menu layout and the radio-style "current" indicator.
    pub const fn all() -> [Self; 8] {
        [
            Self::Spectrum16K,
            Self::Spectrum48K,
            Self::SpectrumPlus,
            Self::Spectrum128K,
            Self::SpectrumPlus2,
            Self::SpectrumPlus2A,
            Self::SpectrumPlus2B,
            Self::SpectrumPlus3,
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
pub fn variant_rom_bundle(kind: MachineKind, root: &Path) -> Vec<(&'static str, PathBuf)> {
    match kind {
        MachineKind::Spectrum16K
        | MachineKind::Spectrum48K
        | MachineKind::SpectrumPlus => vec![(
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

    /// Filesystem read failed for one ROM.
    #[error(transparent)]
    Io(#[from] std::io::Error),
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
}
