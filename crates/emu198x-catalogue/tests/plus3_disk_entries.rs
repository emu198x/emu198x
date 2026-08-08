//! Focused regression for the +3 disk-loader catalogue entries.
//! Runs *only* the entries whose variant is `plus3` and whose media
//! kind is `disk`, so the full SOLID catalogue doesn't have to run
//! end-to-end to validate a +3 disk fix.
//!
//!     cargo test -p emu198x-catalogue --test plus3_disk_entries \
//!         -- --ignored --nocapture

use std::env;
use std::path::PathBuf;

use emu198x_catalogue::{
    EntryOutcome, SnapshotOutcome, load_manifest, run_spectrum_entry_with_snapshot_check,
};

fn manifest_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("manifest/spectrum.toml")
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

#[test]
#[ignore]
fn plus3_disk_entries_load_and_match() {
    let manifest =
        load_manifest(&manifest_path()).unwrap_or_else(|err| panic!("loading manifest: {err}"));
    let media_root = media_root();
    let firmware_root = firmware_root();

    let mut failures: Vec<String> = Vec::new();
    let mut ran = 0;

    for entry in &manifest.entry {
        if entry.variant != "plus3" {
            continue;
        }
        let is_disk = entry.media.as_ref().is_some_and(|m| m.kind == "disk");
        if !is_disk {
            continue;
        }
        ran += 1;

        println!("=== {} — {} ({})", entry.id, entry.title, entry.variant);
        let (result, snapshot) =
            run_spectrum_entry_with_snapshot_check(&manifest, entry, &media_root, &firmware_root)
                .unwrap_or_else(|err| panic!("{} runner failed: {err}", entry.id));

        match result.outcome {
            EntryOutcome::Pass => println!("[PASS] {}", entry.id),
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

        match snapshot.outcome {
            SnapshotOutcome::Pass => println!("[SNAP-PASS] {}", entry.id),
            SnapshotOutcome::FrameHashDrift { expected, actual } => {
                let line = format!(
                    "[SNAP-FAIL] {} — gap-end frame hash drift: expected {expected}, got {actual}",
                    entry.id
                );
                println!("{line}");
                failures.push(line);
            }
            SnapshotOutcome::AudioHashDrift { expected, actual } => {
                let line = format!(
                    "[SNAP-FAIL] {} — audio hash drift: expected {expected}, got {actual}",
                    entry.id
                );
                println!("{line}");
                failures.push(line);
            }
            SnapshotOutcome::BytesDrift {
                original_len,
                reencoded_len,
                ..
            } => {
                let line = format!(
                    "[SNAP-FAIL] {} — bytes drift: orig {original_len}, reencoded {reencoded_len}",
                    entry.id
                );
                println!("{line}");
                failures.push(line);
            }
            SnapshotOutcome::EncodeFailed { reason } => {
                let line = format!("[SNAP-FAIL] {} — encode: {reason}", entry.id);
                println!("{line}");
                failures.push(line);
            }
            SnapshotOutcome::RestoreFailed { reason } => {
                let line = format!("[SNAP-FAIL] {} — restore: {reason}", entry.id);
                println!("{line}");
                failures.push(line);
            }
        }
    }

    assert!(ran > 0, "no plus3 disk entries found in manifest");
    assert!(
        failures.is_empty(),
        "{} +3 disk entry/entries failed:\n{}",
        failures.len(),
        failures.join("\n")
    );
}
