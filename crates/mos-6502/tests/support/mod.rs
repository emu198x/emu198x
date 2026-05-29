#![allow(dead_code)]

use std::path::{Path, PathBuf};

const LORENZ_6502_DIR_ENV: &str = "EMU198X_6502_LORENZ_DIR";
const TOM_HARTE_6502_DIR_ENV: &str = "EMU198X_6502_TOM_HARTE_DIR";
const DORMANN_6502_DIR_ENV: &str = "EMU198X_6502_DORMANN_DIR";
const C64_KERNAL_ROM_ENV: &str = "EMU198X_C64_KERNAL_ROM";

pub fn find_lorenz_6502_dir() -> Result<PathBuf, String> {
    let repo_root = repo_root();
    let mut candidates = Vec::new();

    if let Some(path) = std::env::var_os(LORENZ_6502_DIR_ENV) {
        candidates.push(PathBuf::from(path));
    }

    candidates.push(repo_root.join("test-data/commodore/c64/lorenz"));
    candidates.push(home_projects_path(
        "Emu198x-Unclean/Reference/commodore/c64/lorenz/Wolfgang Lorenz 6502 test suite",
    ));
    candidates.push(home_projects_path(
        "Reference/commodore/c64/lorenz/Wolfgang Lorenz 6502 test suite",
    ));

    first_existing_path(candidates).ok_or_else(|| {
        missing_fixture_message(
            "Wolfgang Lorenz 6502 suite",
            LORENZ_6502_DIR_ENV,
            &[
                repo_root.join("test-data/commodore/c64/lorenz"),
                home_projects_path(
                    "Emu198x-Unclean/Reference/commodore/c64/lorenz/Wolfgang Lorenz 6502 test suite",
                ),
                home_projects_path(
                    "Reference/commodore/c64/lorenz/Wolfgang Lorenz 6502 test suite",
                ),
            ],
        )
    })
}

pub fn find_tom_harte_6502_dir() -> Result<PathBuf, String> {
    let repo_root = repo_root();
    let mut candidates = Vec::new();

    if let Some(path) = std::env::var_os(TOM_HARTE_6502_DIR_ENV) {
        candidates.push(PathBuf::from(path));
    }

    candidates.push(home_projects_path(
        "198x/assets/test-suites/processor-tests/65x02/6502/v1",
    ));
    candidates.push(repo_root.join("test-data/6502/v1"));
    candidates.push(home_projects_path("Emu198x-Unclean/65x02/6502/v1"));
    candidates.push(home_projects_path(
        "Emu198x-Unclean/Reference/test-suites/processor-tests/6502/v1",
    ));
    candidates.push(home_projects_path(
        "Reference/test-suites/processor-tests/6502/v1",
    ));

    first_existing_path(candidates).ok_or_else(|| {
        missing_fixture_message(
            "Tom Harte 6502 corpus",
            TOM_HARTE_6502_DIR_ENV,
            &[
                repo_root.join("test-data/6502/v1"),
                home_projects_path("Emu198x-Unclean/65x02/6502/v1"),
                home_projects_path("Emu198x-Unclean/Reference/test-suites/processor-tests/6502/v1"),
                home_projects_path("Reference/test-suites/processor-tests/6502/v1"),
            ],
        )
    })
}

pub fn find_dormann_6502_dir() -> Result<PathBuf, String> {
    let repo_root = repo_root();
    let mut candidates = Vec::new();

    if let Some(path) = std::env::var_os(DORMANN_6502_DIR_ENV) {
        candidates.push(PathBuf::from(path));
    }

    candidates.push(home_projects_path(
        "198x/assets/test-suites/6502/klaus-functional-tests",
    ));
    candidates.push(repo_root.join("test-data/6502_65C02_functional_tests"));
    candidates.push(home_projects_path(
        "Emu198x-Unclean/6502_65C02_functional_tests",
    ));
    candidates.push(home_projects_path("Reference/6502_65C02_functional_tests"));

    first_existing_path(candidates).ok_or_else(|| {
        missing_fixture_message(
            "Dormann 6502 functional suite",
            DORMANN_6502_DIR_ENV,
            &[
                repo_root.join("test-data/6502_65C02_functional_tests"),
                home_projects_path("Emu198x-Unclean/6502_65C02_functional_tests"),
                home_projects_path("Reference/6502_65C02_functional_tests"),
            ],
        )
    })
}

pub fn find_c64_kernal_rom() -> Result<PathBuf, String> {
    let mut candidates = Vec::new();

    if let Some(path) = std::env::var_os(C64_KERNAL_ROM_ENV) {
        candidates.push(PathBuf::from(path));
    }

    candidates.push(home_path(".emu198x/roms/commodore-c64/kernal.rom"));
    candidates.push(home_path(".emu198x/roms/commodore-c64/c64-kernal.rom"));
    candidates.push(home_path(".emu198x/roms/c64/kernal.rom"));
    candidates.push(home_path(".emu198x/roms/c64/c64-kernal.rom"));

    first_existing_path(candidates).ok_or_else(|| {
        missing_fixture_message(
            "Commodore 64 KERNAL ROM",
            C64_KERNAL_ROM_ENV,
            &[
                home_path(".emu198x/roms/commodore-c64/kernal.rom"),
                home_path(".emu198x/roms/commodore-c64/c64-kernal.rom"),
                home_path(".emu198x/roms/c64/kernal.rom"),
                home_path(".emu198x/roms/c64/c64-kernal.rom"),
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
