//! Catalogue CLI: capture hashes for paste-into-manifest, or run entries
//! and report pass/fail. Driven by per-system TOML manifests under
//! `crates/emu198x-catalogue/manifest/`.
//!
//! Usage:
//!     catalogue capture --entry <id> [--manifest PATH]
//!     catalogue run [--entry <id> | --shard <i>/<n>] [--manifest PATH]

use std::env;
use std::path::PathBuf;
use std::process;

use emu198x_catalogue::{
    CatalogueError, Entry, EntryOutcome, Manifest, RunResult, SnapshotCheckResult, SnapshotOutcome,
    load_manifest, run_amiga_entry_with_snapshot_check, run_c64_entry_with_snapshot_check,
    run_entry, run_entry_for_capture, run_spectrum_entry_with_snapshot_check,
};

const USAGE: &str = "\
Usage:
    catalogue capture --entry <id> [--manifest PATH]
                      [--save-screenshot PATH] [--save-audio PATH]
    catalogue run [--entry <id> | --shard <i>/<n>] [--manifest PATH]

--shard <i>/<n> runs every n-th entry starting at the i-th (1-based), so
n jobs given 1/n .. n/n cover the manifest exactly once between them.

Resolves media and firmware against:
    EMU198X_CATALOGUE_MEDIA_ROOT     (default: /Volumes/Data/Library/ROMs/TOSEC)
    EMU198X_CATALOGUE_FIRMWARE_ROOT  (default: ~/.emu198x/roms)
";

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.is_empty() {
        eprintln!("{USAGE}");
        process::exit(2);
    }
    let result = match args[0].as_str() {
        "capture" => cmd_capture(&args[1..]),
        "run" => cmd_run(&args[1..]),
        "--help" | "-h" => {
            println!("{USAGE}");
            return;
        }
        other => {
            eprintln!("error: unknown subcommand: {other}");
            eprintln!("{USAGE}");
            process::exit(2);
        }
    };
    if let Err(err) = result {
        eprintln!("error: {err}");
        process::exit(1);
    }
}

#[derive(Default)]
struct Args {
    entry: Option<String>,
    manifest: Option<PathBuf>,
    shard: Option<Shard>,
    save_screenshot: Option<PathBuf>,
    save_audio: Option<PathBuf>,
}

fn parse_args(argv: &[String]) -> Result<Args, String> {
    let mut args = Args::default();
    let mut iter = argv.iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--entry" => {
                args.entry = Some(
                    iter.next()
                        .ok_or_else(|| "--entry requires an entry id".to_string())?
                        .clone(),
                );
            }
            "--manifest" => {
                args.manifest = Some(PathBuf::from(
                    iter.next()
                        .ok_or_else(|| "--manifest requires a path".to_string())?,
                ));
            }
            "--shard" => {
                args.shard = Some(Shard::parse(
                    iter.next()
                        .ok_or_else(|| "--shard requires <i>/<n>".to_string())?,
                )?);
            }
            "--save-screenshot" => {
                args.save_screenshot =
                    Some(PathBuf::from(iter.next().ok_or_else(|| {
                        "--save-screenshot requires a path".to_string()
                    })?));
            }
            "--save-audio" => {
                args.save_audio = Some(PathBuf::from(
                    iter.next()
                        .ok_or_else(|| "--save-audio requires a path".to_string())?,
                ));
            }
            other => return Err(format!("unknown flag: {other}")),
        }
    }
    if args.entry.is_some() && args.shard.is_some() {
        return Err("--entry and --shard are mutually exclusive".to_string());
    }
    Ok(args)
}

/// One slice of a manifest for a parallel run: shard `index` (1-based) of
/// `count`. Entries are dealt round-robin, so neighbouring entries — often
/// the same game on sibling variants, with similar run times — land in
/// different shards and the shards come out close in length.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Shard {
    index: usize,
    count: usize,
}

impl Shard {
    fn parse(spec: &str) -> Result<Self, String> {
        let invalid = || format!("--shard expects <i>/<n> with 1 <= i <= n, got {spec:?}");
        let (index, count) = spec.split_once('/').ok_or_else(invalid)?;
        let index: usize = index.trim().parse().map_err(|_| invalid())?;
        let count: usize = count.trim().parse().map_err(|_| invalid())?;
        if index == 0 || index > count {
            return Err(invalid());
        }
        Ok(Self { index, count })
    }

    fn select(self, entries: &[Entry]) -> Vec<&Entry> {
        entries
            .iter()
            .enumerate()
            .filter(|(position, _)| position % self.count == self.index - 1)
            .map(|(_, entry)| entry)
            .collect()
    }
}

fn default_manifest_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("manifest/spectrum.toml")
}

fn media_root() -> PathBuf {
    // Default to the Time Capsule TOSEC library. The manifest's relative paths
    // (`commodore/c64/Games/...`) match TOSEC's layout bar casing, which resolves
    // on the case-insensitive volume. Override with EMU198X_CATALOGUE_MEDIA_ROOT.
    env::var_os("EMU198X_CATALOGUE_MEDIA_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/Volumes/Data/Library/ROMs/TOSEC"))
}

fn firmware_root() -> PathBuf {
    env::var_os("EMU198X_CATALOGUE_FIRMWARE_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| dirs_home().join(".emu198x").join("roms"))
}

fn dirs_home() -> PathBuf {
    env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/"))
}

fn load(args: &Args) -> Result<Manifest, CatalogueError> {
    let path = args.manifest.clone().unwrap_or_else(default_manifest_path);
    load_manifest(&path)
}

fn find_entry<'m>(manifest: &'m Manifest, id: &str) -> Result<&'m Entry, CatalogueError> {
    manifest
        .entry
        .iter()
        .find(|entry| entry.id == id)
        .ok_or_else(|| CatalogueError::EntryNotFound(id.to_string()))
}

fn cmd_capture(argv: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let args = parse_args(argv)?;
    let entry_id = args.entry.clone().ok_or("capture requires --entry <id>")?;
    let manifest = load(&args)?;
    let entry = find_entry(&manifest, &entry_id)?;
    // Capture explicitly bypasses verify_routing_versions: a routing-
    // version mismatch is the *reason* we're capturing — the new
    // hashes resolve it. See `run_entry_for_capture` docs.
    let result = run_entry_for_capture(&manifest, entry, &media_root(), &firmware_root())?;
    print_capture(&entry_id, &result);
    if let Some(path) = &args.save_screenshot {
        std::fs::write(path, &result.boot_png)?;
        println!("  saved screenshot  = {}", path.display());
    }
    if let Some(path) = &args.save_audio {
        std::fs::write(path, &result.audio_wav)?;
        println!("  saved audio       = {}", path.display());
    }
    Ok(())
}

fn print_capture(entry_id: &str, result: &RunResult) {
    println!("{entry_id}:");
    println!("  boot.frame_hash = \"{}\"", result.boot_hash);
    println!("  audio.hash      = \"{}\"", result.audio_hash);
    match &result.outcome {
        EntryOutcome::Pass => println!("  outcome         = pass"),
        EntryOutcome::BootHashMismatch { expected, .. } => {
            println!("  outcome         = boot mismatch (expected {expected})");
        }
        EntryOutcome::AudioHashMismatch { expected, .. } => {
            println!("  outcome         = audio mismatch (expected {expected})");
        }
    }
}

fn cmd_run(argv: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let args = parse_args(argv)?;
    let manifest = load(&args)?;
    let media_root = media_root();
    let firmware_root = firmware_root();

    let entries: Vec<&Entry> = match (&args.entry, args.shard) {
        (Some(id), _) => vec![find_entry(&manifest, id)?],
        (None, Some(shard)) => shard.select(&manifest.entry),
        (None, None) => manifest.entry.iter().collect(),
    };
    if entries.is_empty() {
        // A shard past the end of a short manifest would otherwise pass
        // having checked nothing.
        return Err("no entries selected".into());
    }

    let selected = entries.len();
    let mut failures = 0u32;
    for entry in entries {
        // An entry that cannot run (missing media or firmware, a session
        // error) is reported against its id and counted, and the run goes
        // on, so one bad fixture names itself instead of hiding the rest.
        let (result, snapshot) =
            match run_entry_for_verification(&manifest, entry, &media_root, &firmware_root) {
                Ok(outcome) => outcome,
                Err(err) => {
                    println!("[ERROR] {} ({}) — {err}", entry.id, entry.title);
                    failures = failures.saturating_add(1);
                    continue;
                }
            };
        let mark = match &result.outcome {
            EntryOutcome::Pass => "PASS",
            EntryOutcome::BootHashMismatch { .. } | EntryOutcome::AudioHashMismatch { .. } => {
                "FAIL"
            }
        };
        println!("[{mark}] {} ({})", entry.id, entry.title);
        if !matches!(result.outcome, EntryOutcome::Pass) {
            failures = failures.saturating_add(1);
            print_capture(&entry.id, &result);
        }
        if let Some(snapshot) = snapshot {
            if matches!(snapshot.outcome, SnapshotOutcome::Pass) {
                println!("[SNAP-PASS] {}", entry.id);
            } else {
                println!("[SNAP-FAIL] {} — {:?}", entry.id, snapshot.outcome);
                failures = failures.saturating_add(1);
            }
        }
    }

    println!("{selected} entries run, {failures} failure(s)");
    if failures > 0 {
        process::exit(1);
    }
    Ok(())
}

fn run_entry_for_verification(
    manifest: &Manifest,
    entry: &Entry,
    media_root: &std::path::Path,
    firmware_root: &std::path::Path,
) -> Result<(RunResult, Option<SnapshotCheckResult>), CatalogueError> {
    match manifest.system.id.as_str() {
        "spectrum" => {
            let (result, snapshot) =
                run_spectrum_entry_with_snapshot_check(manifest, entry, media_root, firmware_root)?;
            Ok((result, Some(snapshot)))
        }
        "c64" => {
            let (result, snapshot) =
                run_c64_entry_with_snapshot_check(manifest, entry, media_root, firmware_root)?;
            Ok((result, Some(snapshot)))
        }
        "amiga" => {
            let (result, snapshot) =
                run_amiga_entry_with_snapshot_check(manifest, entry, media_root, firmware_root)?;
            Ok((result, Some(snapshot)))
        }
        _ => run_entry(manifest, entry, media_root, firmware_root).map(|result| (result, None)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest_with(count: usize) -> Manifest {
        let entries: String = (0..count)
            .map(|n| {
                format!(
                    "[[entry]]\nid = \"e{n}\"\ntitle = \"E{n}\"\nyear = 1985\npublisher = \"p\"\n\
                     variant = \"48k\"\n[entry.boot]\nwait_frames = 1\nframe_hash = \"xxh64:0\"\n\
                     [entry.audio]\nfrom_frame = 1\nsecs = 1.0\nhash = \"xxh64:0\"\n"
                )
            })
            .collect();
        toml::from_str(&format!("[system]\nid = \"spectrum\"\n{entries}"))
            .expect("test manifest parses")
    }

    fn ids(entries: &[&Entry]) -> Vec<String> {
        entries.iter().map(|entry| entry.id.clone()).collect()
    }

    #[test]
    fn shards_cover_every_entry_exactly_once() {
        let manifest = manifest_with(103);
        for count in 1..=12 {
            let mut seen: Vec<String> = (1..=count)
                .flat_map(|index| ids(&Shard { index, count }.select(&manifest.entry)))
                .collect();
            assert_eq!(seen.len(), 103, "{count} shards");
            seen.sort();
            seen.dedup();
            assert_eq!(seen.len(), 103, "{count} shards repeat an entry");
        }
    }

    #[test]
    fn shards_deal_round_robin_and_stay_balanced() {
        let manifest = manifest_with(7);
        let shard = |index| ids(&Shard { index, count: 3 }.select(&manifest.entry));
        assert_eq!(shard(1), ["e0", "e3", "e6"]);
        assert_eq!(shard(2), ["e1", "e4"]);
        assert_eq!(shard(3), ["e2", "e5"]);
    }

    #[test]
    fn shard_spec_must_name_a_shard_that_exists() {
        assert_eq!(Shard::parse("2/4"), Ok(Shard { index: 2, count: 4 }));
        for bad in ["0/4", "5/4", "1/0", "4", "a/4", "1/b", ""] {
            assert!(Shard::parse(bad).is_err(), "{bad:?} accepted");
        }
    }

    #[test]
    fn entry_and_shard_cannot_be_combined() {
        let argv: Vec<String> = ["--entry", "e0", "--shard", "1/2"]
            .iter()
            .map(|arg| (*arg).to_string())
            .collect();
        assert!(parse_args(&argv).is_err());
    }
}
