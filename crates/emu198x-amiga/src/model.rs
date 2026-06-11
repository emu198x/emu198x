//! Amiga model selection + Kickstart ROM resolution.
//!
//! Shared by every mode — the windowed UI's `parse_cli`, the `--script`
//! runner, and the MCP server — and, crucially, by the MCP `tools.rs`
//! `set_machine` path. `tools.rs` is `#[path]`-included into the
//! `mcp_smoke` integration test (a second crate root), so the items it
//! depends on have to live in a module both roots can reach; this is
//! that module. The bin re-exports it (`pub(crate) use model::*`) so the
//! UI / script / mcp modules keep their `crate::ModelArg` paths, while
//! `tools.rs` refers to it as `crate::model::…` and the test adds a
//! matching `#[path] mod model;`.

use std::env;
use std::path::{Path, PathBuf};

use runtime_commodore_amiga::Model;

/// Model selector shared by the UI's `parse_cli`, the MCP CLI, and ROM
/// resolution. The full eight-model surface (the AGA A1200 included) so
/// `--model a1200` selects the AGA chipset across every mode.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum ModelArg {
    A1000,
    #[default]
    A500,
    A500A501,
    A500Plus,
    A500Maxed,
    A600,
    A1200,
    A2000,
}

impl ModelArg {
    /// Every accepted model id, in canonical order. Single source of
    /// truth for `--model` parsing, the MCP / `--script` `set_machine`
    /// schema, and their error messages.
    pub(crate) const IDS: [&'static str; 8] = [
        "a1000",
        "a500",
        "a500-a501",
        "a500-plus",
        "a500-maxed",
        "a600",
        "a1200",
        "a2000",
    ];

    pub(crate) const fn to_model(self) -> Model {
        match self {
            Self::A1000 => Model::A1000OcsPal,
            Self::A500 => Model::A500OcsPal,
            Self::A500A501 => Model::A500OcsPalA501,
            Self::A500Plus => Model::A500PlusEcsPal,
            Self::A500Maxed => Model::A500OcsPalMaxed,
            Self::A600 => Model::A600EcsPal,
            Self::A1200 => Model::A1200AgaPal,
            Self::A2000 => Model::A2000OcsPal,
        }
    }

    /// Parse a model id (`a1000`, `a500`, `a500-a501`, …). `None` if the
    /// string isn't one of [`Self::IDS`]. Use this anywhere a caller can
    /// supply a bad id (tool arguments, script steps); `parse_model_arg`
    /// wraps it for the CLI, where an unknown id is fatal.
    pub(crate) fn from_id(value: &str) -> Option<Self> {
        Some(match value {
            "a1000" => Self::A1000,
            "a500" => Self::A500,
            "a500-a501" => Self::A500A501,
            "a500-plus" => Self::A500Plus,
            "a500-maxed" => Self::A500Maxed,
            "a600" => Self::A600,
            "a1200" => Self::A1200,
            "a2000" => Self::A2000,
            _ => return None,
        })
    }
}

/// FirmwareSet id for the A1000's bootstrap ROM (it boots a small
/// bootstrap and pulls Kickstart from disk, unlike later models).
pub(crate) const A1000_BOOTSTRAP_ID: &str = "commodore-amiga-a1000-bootstrap-rom";
/// FirmwareSet id for the resident Kickstart ROM (A500 onward).
pub(crate) const KICKSTART_ID: &str = "commodore-amiga-kickstart-rom";

/// Which FirmwareSet id a model's ROM is pushed under. The A1000 carries
/// a bootstrap ROM; every later model carries a resident Kickstart.
/// Shared by the windowed UI, the `--script` runner, and the MCP
/// `set_machine` tool so all three label firmware identically.
pub(crate) const fn firmware_id_for_model_arg(model: ModelArg) -> &'static str {
    match model {
        ModelArg::A1000 => A1000_BOOTSTRAP_ID,
        ModelArg::A500
        | ModelArg::A500A501
        | ModelArg::A500Plus
        | ModelArg::A500Maxed
        | ModelArg::A600
        | ModelArg::A1200
        | ModelArg::A2000 => KICKSTART_ID,
    }
}

/// Candidate ROM filenames to search in a `rom-dir` for a given model.
/// Order matters — first hit wins. Shared between the windowed UI's
/// `resolve_firmware_path` and the MCP path so both stay in sync.
pub(crate) fn rom_candidates_for_model(model: ModelArg) -> &'static [&'static str] {
    match model {
        ModelArg::A1000 => &[
            "a1000-bootstrap.rom",
            "a1000_bootstrap.rom",
            "bootstrap.rom",
        ],
        // A500-family + A2000 (OCS, 256/512 KiB Kickstart).
        ModelArg::A500 | ModelArg::A500A501 | ModelArg::A500Maxed | ModelArg::A2000 => &[
            "kick13.rom",
            "kick12.rom",
            "kick31.rom",
            "kickstart.rom",
            "kick.rom",
        ],
        // ECS chip stack — A500+ ships with Kickstart 2.04, A600 with 2.05/3.1.
        ModelArg::A500Plus | ModelArg::A600 => &[
            "kick204.rom",
            "kick205.rom",
            "kick21.rom",
            "kick31.rom",
            "kick31a600.rom",
            "kickstart.rom",
            "kick.rom",
        ],
        // AGA chip stack — A1200 ships with Kickstart 3.0 / 3.1.
        ModelArg::A1200 => &[
            "kick31a1200.rom",
            "kick30a1200.rom",
            "kick31.rom",
            "kick30.rom",
            "kickstart.rom",
            "kick.rom",
        ],
    }
}

/// Locate the ROM file for `model`, honouring an explicit `kickstart`
/// path first, otherwise searching `rom_dir_override` and the standard
/// fallback directories for [`rom_candidates_for_model`]. Shared by the
/// windowed UI and the MCP mode in [`crate::mcp::run`].
pub(crate) fn find_rom_path(
    model: ModelArg,
    rom_dir_override: Option<&Path>,
    kickstart_override: Option<&Path>,
) -> Result<PathBuf, String> {
    if let Some(path) = kickstart_override {
        return Ok(path.to_path_buf());
    }

    let rom_dir = candidate_rom_dirs(rom_dir_override)
        .into_iter()
        .find(|dir| dir.is_dir())
        .ok_or_else(|| {
            "no Amiga ROM directory found; use --kickstart PATH or --rom-dir DIR".to_owned()
        })?;

    let candidates: &[&str] = rom_candidates_for_model(model);

    for name in candidates {
        let path = rom_dir.join(name);
        if path.is_file() {
            return Ok(path);
        }
    }

    Err(format!(
        "no Amiga firmware ROM found in {}; tried {}",
        rom_dir.display(),
        candidates.join(", ")
    ))
}

fn candidate_rom_dirs(rom_dir_override: Option<&Path>) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(dir) = rom_dir_override {
        dirs.push(dir.to_path_buf());
    }
    if let Some(dir) = env::var_os("EMU198X_AMIGA_ROM_DIR") {
        dirs.push(PathBuf::from(dir));
    }
    if let Some(home) = env::var_os("HOME") {
        dirs.push(Path::new(&home).join(".emu198x/roms/commodore-amiga"));
        dirs.push(Path::new(&home).join(".emu198x/roms/amiga"));
    }
    dirs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_args_map_to_runtime_models() {
        assert_eq!(ModelArg::A1000.to_model(), Model::A1000OcsPal);
        assert_eq!(ModelArg::A500.to_model(), Model::A500OcsPal);
        assert_eq!(ModelArg::A500A501.to_model(), Model::A500OcsPalA501);
        assert_eq!(ModelArg::A500Plus.to_model(), Model::A500PlusEcsPal);
        assert_eq!(ModelArg::A500Maxed.to_model(), Model::A500OcsPalMaxed);
        assert_eq!(ModelArg::A600.to_model(), Model::A600EcsPal);
        assert_eq!(ModelArg::A1200.to_model(), Model::A1200AgaPal);
        assert_eq!(ModelArg::A2000.to_model(), Model::A2000OcsPal);
    }

    #[test]
    fn from_id_accepts_every_advertised_id_and_rejects_junk() {
        for id in ModelArg::IDS {
            assert!(
                ModelArg::from_id(id).is_some(),
                "advertised id `{id}` must parse"
            );
        }
        assert!(ModelArg::from_id("a4000").is_none());
        assert!(ModelArg::from_id("").is_none());
    }

    #[test]
    fn rom_candidates_branch_on_chipset() {
        assert_eq!(
            rom_candidates_for_model(ModelArg::A1200)[0],
            "kick31a1200.rom"
        );
        let ecs = rom_candidates_for_model(ModelArg::A600);
        assert!(
            ecs.iter()
                .any(|n| *n == "kick204.rom" || *n == "kick205.rom")
        );
        assert_eq!(
            rom_candidates_for_model(ModelArg::A1000)[0],
            "a1000-bootstrap.rom"
        );
    }
}
