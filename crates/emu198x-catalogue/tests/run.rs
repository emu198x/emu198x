//! Catalogue regression test: iterates every per-system manifest under
//! `manifest/` and reports a per-entry pass/fail grid.
//!
//! Marked `#[ignore]` because catalogue runs need real ROMs and media
//! that we don't check into the repo. To run locally:
//!
//!     cargo test -p emu198x-catalogue -- --ignored --nocapture
//!
//! Resolves media and firmware against:
//!     EMU198X_CATALOGUE_MEDIA_ROOT     (default: ~/Projects/Emu198x-Unclean/Reference)
//!     EMU198X_CATALOGUE_FIRMWARE_ROOT  (default: ~/.emu198x/roms)

use std::env;
use std::path::PathBuf;

use emu198x_catalogue::{EntryOutcome, load_manifest, run_entry};

fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("manifest")
}

fn media_root() -> PathBuf {
    env::var_os("EMU198X_CATALOGUE_MEDIA_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            home()
                .join("Projects")
                .join("Emu198x-Unclean")
                .join("Reference")
        })
}

fn firmware_root() -> PathBuf {
    env::var_os("EMU198X_CATALOGUE_FIRMWARE_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| home().join(".emu198x").join("roms"))
}

fn home() -> PathBuf {
    env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/"))
}

fn manifest_files() -> Vec<PathBuf> {
    let mut paths: Vec<PathBuf> = std::fs::read_dir(manifest_dir())
        .expect("manifest dir is readable")
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "toml"))
        .collect();
    paths.sort();
    paths
}

#[test]
#[ignore]
fn catalogue_passes_every_entry() {
    let media_root = media_root();
    let firmware_root = firmware_root();

    let mut failures: Vec<String> = Vec::new();
    for manifest_path in manifest_files() {
        let manifest = load_manifest(&manifest_path)
            .unwrap_or_else(|err| panic!("loading {manifest_path:?}: {err}"));
        println!("=== {} ({} entries)", manifest.system.id, manifest.entry.len());
        for entry in &manifest.entry {
            let result = run_entry(&manifest, entry, &media_root, &firmware_root)
                .unwrap_or_else(|err| panic!("{} runner failed: {err}", entry.id));
            match result.outcome {
                EntryOutcome::Pass => {
                    println!("[PASS] {} — {}", entry.id, entry.title);
                }
                EntryOutcome::BootHashMismatch { expected, actual } => {
                    let line = format!(
                        "[FAIL] {} — boot frame hash: expected {expected}, got {actual}",
                        entry.id
                    );
                    println!("{line}");
                    failures.push(line);
                }
                EntryOutcome::AudioHashMismatch { expected, actual } => {
                    let line = format!(
                        "[FAIL] {} — audio hash: expected {expected}, got {actual}",
                        entry.id
                    );
                    println!("{line}");
                    failures.push(line);
                }
            }
        }
    }

    assert!(
        failures.is_empty(),
        "{} catalogue entry/entries failed:\n{}",
        failures.len(),
        failures.join("\n")
    );
}
