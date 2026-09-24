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
    resolve_fixture(
        "Tom Harte Z80 corpus",
        TOM_HARTE_Z80_ENV,
        std::env::var_os(TOM_HARTE_Z80_ENV).map(PathBuf::from),
        &[
            repo_root.join("../../assets/test-suites/processor-tests/z80/v1"),
            repo_root.join("test-data/z80/v1"),
        ],
        Path::is_dir,
    )
}

pub fn find_zex_binary(name: &str) -> Result<PathBuf, String> {
    let filename = format!("{name}.com");
    let repo_root = repo_root();
    resolve_fixture(
        &format!("ZEX binary {filename}"),
        ZEX_DIR_ENV,
        std::env::var_os(ZEX_DIR_ENV).map(|dir| PathBuf::from(dir).join(&filename)),
        &[
            repo_root
                .join("../../assets/test-suites/zex")
                .join(&filename),
            repo_root.join("test-data/zex").join(&filename),
        ],
        Path::is_file,
    )
}

#[allow(dead_code)]
pub fn find_fuse_z80_tests_dir() -> Result<PathBuf, String> {
    let repo_root = repo_root();
    // The vendored FUSE source carries the corpus it is the reference
    // for. `198x/emulators/zx-spectrum/fuse-1.7.0/z80/tests` holds
    // `tests.in` and `tests.expected` byte-identically to any separate
    // download, so there is nothing to fetch and nothing to keep in
    // sync — the fixtures and the emulator we score against are the same
    // release. See `../../decisions/` on reference emulators
    // (`RULES.md` rule 32).
    // Two depths: a normal checkout sits at `198x/Emu198x/emu198x`, a
    // `git worktree` one level deeper under `198x/Emu198x/.worktrees/<name>`.
    const VENDORED_FUSE: &str = "emulators/zx-spectrum/fuse-1.7.0/z80/tests";
    resolve_fixture(
        "FUSE Z80 test directory",
        FUSE_Z80_TESTS_ENV,
        std::env::var_os(FUSE_Z80_TESTS_ENV).map(PathBuf::from),
        &[
            repo_root.join("test-data/fuse/z80"),
            repo_root.join("../..").join(VENDORED_FUSE),
            repo_root.join("../../..").join(VENDORED_FUSE),
        ],
        Path::is_dir,
    )
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .to_path_buf()
}

// An explicit corpus is authoritative: substituting another version would make
// the result describe fixtures the caller never selected.
fn resolve_fixture(
    label: &str,
    env_var: &str,
    explicit: Option<PathBuf>,
    defaults: &[PathBuf],
    valid: fn(&Path) -> bool,
) -> Result<PathBuf, String> {
    if let Some(path) = explicit {
        return if valid(&path) {
            Ok(path)
        } else {
            Err(format!(
                "{label} missing or wrong path type at {} (selected by {env_var}); default fallback is disabled.",
                path.display()
            ))
        };
    }
    defaults
        .iter()
        .find(|path| valid(path))
        .cloned()
        .ok_or_else(|| missing_fixture_message(label, env_var, defaults))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_fixture_never_falls_back() {
        let root = std::env::temp_dir().join(format!(
            "emu198x-z80-fixtures-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock is after Unix epoch")
                .as_nanos()
        ));
        struct Cleanup(PathBuf);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
        std::fs::create_dir_all(&root).expect("create fixture directory");
        let _cleanup = Cleanup(root.clone());
        let file = root.join("fixture.com");
        std::fs::write(&file, b"fixture").expect("write fixture binary");
        let missing = root.join("missing");
        for (valid, fallback, wrong_type) in [
            (Path::is_dir as fn(&Path) -> bool, &root, &file),
            (Path::is_file as fn(&Path) -> bool, &file, &root),
        ] {
            let defaults = [fallback.clone()];
            for invalid in [&missing, wrong_type] {
                let error = resolve_fixture(
                    "test corpus",
                    "TEST_DIR",
                    Some(invalid.clone()),
                    &defaults,
                    valid,
                )
                .expect_err("an invalid explicit fixture must not fall back");
                assert!(error.contains("TEST_DIR"));
                assert!(error.contains(&invalid.display().to_string()));
                assert!(error.contains("fallback is disabled"));
            }
            assert_eq!(
                resolve_fixture("test", "TEST_DIR", Some(fallback.clone()), &[], valid)
                    .expect("valid explicit fixture"),
                *fallback
            );
            assert_eq!(
                resolve_fixture(
                    "test",
                    "TEST_DIR",
                    None,
                    &[missing.clone(), wrong_type.clone(), fallback.clone()],
                    valid
                )
                .expect("valid default fixture"),
                *fallback
            );
            assert!(
                resolve_fixture(
                    "test",
                    "TEST_DIR",
                    None,
                    std::slice::from_ref(&missing),
                    valid
                )
                .is_err()
            );
        }
    }
}
