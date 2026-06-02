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

    /// Parses a snake-case identifier from a script step.
    #[allow(dead_code)] // wired when script-mode SetMachine support lands
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
        MachineKind::TimexTC2048 => vec![(
            "timex-tc2048-rom",
            root.join("timex-tc2048/tc2048.rom"),
        )],
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
