#![allow(dead_code)]

use std::path::{Path, PathBuf};

const SM83_TENNANT_DIR_ENV: &str = "EMU198X_SM83_TENNANT_DIR";

/// Locate the Adam Tennant SM83 single-step corpus (Tom Harte-format
/// JSON, one file per opcode in lower-case hex).
///
/// Resolution order: the `EMU198X_SM83_TENNANT_DIR` env var, then a
/// repo-local `test-data/sm83/v2`, then the well-known archive
/// location under `~/Projects/Emu198x-Unclean/GameboyCPUTests/v2`.
pub fn find_sm83_tennant_dir() -> Result<PathBuf, String> {
    let repo_root = repo_root();
    let mut candidates = Vec::new();

    if let Some(path) = std::env::var_os(SM83_TENNANT_DIR_ENV) {
        candidates.push(PathBuf::from(path));
    }

    candidates.push(home_projects_path("198x/assets/test-suites/gameboy/v2"));
    candidates.push(repo_root.join("test-data/sm83/v2"));
    candidates.push(home_projects_path("Emu198x-Unclean/GameboyCPUTests/v2"));

    first_existing_path(candidates).ok_or_else(|| {
        missing_fixture_message(
            "Adam Tennant SM83 single-step corpus",
            SM83_TENNANT_DIR_ENV,
            &[
                repo_root.join("test-data/sm83/v2"),
                home_projects_path("Emu198x-Unclean/GameboyCPUTests/v2"),
            ],
        )
    })
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .to_path_buf()
}

fn home_projects_path(relative: &str) -> PathBuf {
    home_path(&format!("Projects/{relative}"))
}

fn home_path(relative: &str) -> PathBuf {
    match std::env::var_os("HOME") {
        Some(home) => PathBuf::from(home).join(relative),
        None => PathBuf::from("/missing-home").join(relative),
    }
}

fn first_existing_path(candidates: Vec<PathBuf>) -> Option<PathBuf> {
    candidates.into_iter().find(|path| path.exists())
}

fn missing_fixture_message(label: &str, env_var: &str, defaults: &[PathBuf]) -> String {
    let mut message =
        format!("{label} not found. Set {env_var} or place the data in one of these paths:");
    for path in defaults {
        message.push_str("\n  - ");
        message.push_str(&path.display().to_string());
    }
    message
}
