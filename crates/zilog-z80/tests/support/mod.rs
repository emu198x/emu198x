#![allow(dead_code)]

use std::path::{Path, PathBuf};

const TOM_HARTE_Z80_ENV: &str = "EMU198X_Z80_TOM_HARTE_DIR";
const ZEX_DIR_ENV: &str = "EMU198X_ZEX_DIR";
const FUSE_Z80_TESTS_ENV: &str = "EMU198X_FUSE_Z80_TESTS_DIR";

pub fn find_tom_harte_z80_dir() -> Result<PathBuf, String> {
    let repo_root = repo_root();
    let mut candidates = Vec::new();

    if let Some(path) = std::env::var_os(TOM_HARTE_Z80_ENV) {
        candidates.push(PathBuf::from(path));
    }

    candidates.push(repo_root.join("test-data/z80/v1"));
    candidates.push(home_projects_path(
        "Emu198x-Unclean/Reference/test-suites/processor-tests/z80/v1",
    ));
    candidates.push(home_projects_path("Emu198x-archive/test-data/z80/v1"));
    candidates.push(home_projects_path(
        "Reference/test-suites/processor-tests/z80/v1",
    ));

    first_existing_path(candidates).ok_or_else(|| {
        missing_fixture_message(
            "Tom Harte Z80 corpus",
            TOM_HARTE_Z80_ENV,
            &[
                repo_root.join("test-data/z80/v1"),
                home_projects_path("Emu198x-Unclean/Reference/test-suites/processor-tests/z80/v1"),
                home_projects_path("Emu198x-archive/test-data/z80/v1"),
                home_projects_path("Reference/test-suites/processor-tests/z80/v1"),
            ],
        )
    })
}

pub fn find_zex_binary(name: &str) -> Result<PathBuf, String> {
    let filename = format!("{name}.com");
    let mut candidates = Vec::new();

    if let Some(dir) = std::env::var_os(ZEX_DIR_ENV) {
        candidates.push(PathBuf::from(dir).join(&filename));
    }

    candidates.push(home_projects_path(&format!(
        "Emu198x-Unclean/Reference/test-suites/zex/{filename}"
    )));
    candidates.push(home_projects_path(&format!(
        "Emu198x-Unclean/Reference/sinclair/spectrum/{filename}"
    )));
    candidates.push(home_projects_path(&format!(
        "Reference/sinclair/spectrum/{filename}"
    )));

    first_existing_path(candidates).ok_or_else(|| {
        missing_fixture_message(
            &format!("ZEX binary {filename}"),
            ZEX_DIR_ENV,
            &[
                home_projects_path(&format!(
                    "Emu198x-Unclean/Reference/test-suites/zex/{filename}"
                )),
                home_projects_path(&format!(
                    "Emu198x-Unclean/Reference/sinclair/spectrum/{filename}"
                )),
                home_projects_path(&format!("Reference/sinclair/spectrum/{filename}")),
            ],
        )
    })
}

#[allow(dead_code)]
pub fn find_fuse_z80_tests_dir() -> Result<PathBuf, String> {
    let mut candidates = Vec::new();

    if let Some(path) = std::env::var_os(FUSE_Z80_TESTS_ENV) {
        candidates.push(PathBuf::from(path));
    }

    candidates.push(home_projects_path(
        "Emu198x-Unclean/fuse-emulator-fuse/z80/tests",
    ));
    candidates.push(home_projects_path("Reference/fuse-emulator-fuse/z80/tests"));

    first_existing_path(candidates).ok_or_else(|| {
        missing_fixture_message(
            "FUSE Z80 test directory",
            FUSE_Z80_TESTS_ENV,
            &[
                home_projects_path("Emu198x-Unclean/fuse-emulator-fuse/z80/tests"),
                home_projects_path("Reference/fuse-emulator-fuse/z80/tests"),
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
    match dirs::home_dir() {
        Some(home) => home.join("Projects").join(relative),
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
