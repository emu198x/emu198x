#![allow(dead_code)]

use std::path::{Path, PathBuf};

// The shared corpora live in the *umbrella* checkout at
// `198x/assets/test-suites/`, which is two levels above this repo's
// root (`198x/Emu198x/emu198x`) — not one. The fallbacks below said
// `../assets/...` and so resolved to `198x/Emu198x/assets`, which does
// not exist, silently forcing every local run to depend on the env var
// while `tests/spectrum.md` claimed the path was "already baked in".
const TOM_HARTE_Z80_ENV: &str = "EMU198X_Z80_TOM_HARTE_DIR";
const ZEX_DIR_ENV: &str = "EMU198X_ZEX_DIR";
const FUSE_Z80_TESTS_ENV: &str = "EMU198X_FUSE_Z80_TESTS_DIR";

pub fn find_tom_harte_z80_dir() -> Result<PathBuf, String> {
    let repo_root = repo_root();
    let mut candidates = Vec::new();

    if let Some(path) = std::env::var_os(TOM_HARTE_Z80_ENV) {
        candidates.push(PathBuf::from(path));
    }

    candidates.push(repo_root.join("../../assets/test-suites/processor-tests/z80/v1"));
    candidates.push(repo_root.join("test-data/z80/v1"));

    first_existing_path(candidates).ok_or_else(|| {
        missing_fixture_message(
            "Tom Harte Z80 corpus",
            TOM_HARTE_Z80_ENV,
            &[
                repo_root.join("../../assets/test-suites/processor-tests/z80/v1"),
                repo_root.join("test-data/z80/v1"),
            ],
        )
    })
}

pub fn find_zex_binary(name: &str) -> Result<PathBuf, String> {
    let filename = format!("{name}.com");
    let repo_root = repo_root();
    let mut candidates = Vec::new();

    if let Some(dir) = std::env::var_os(ZEX_DIR_ENV) {
        candidates.push(PathBuf::from(dir).join(&filename));
    }

    candidates.push(
        repo_root
            .join("../../assets/test-suites/zex")
            .join(&filename),
    );
    candidates.push(repo_root.join("test-data/zex").join(&filename));

    first_existing_path(candidates).ok_or_else(|| {
        missing_fixture_message(
            &format!("ZEX binary {filename}"),
            ZEX_DIR_ENV,
            &[
                repo_root
                    .join("../../assets/test-suites/zex")
                    .join(&filename),
                repo_root.join("test-data/zex").join(&filename),
            ],
        )
    })
}

#[allow(dead_code)]
pub fn find_fuse_z80_tests_dir() -> Result<PathBuf, String> {
    let repo_root = repo_root();
    let mut candidates = Vec::new();

    if let Some(path) = std::env::var_os(FUSE_Z80_TESTS_ENV) {
        candidates.push(PathBuf::from(path));
    }

    candidates.push(repo_root.join("test-data/fuse/z80"));

    first_existing_path(candidates).ok_or_else(|| {
        missing_fixture_message(
            "FUSE Z80 test directory",
            FUSE_Z80_TESTS_ENV,
            &[repo_root.join("test-data/fuse/z80")],
        )
    })
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .to_path_buf()
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
