//! Catalogue CLI: capture hashes for paste-into-manifest, or run entries
//! and report pass/fail. Driven by per-system TOML manifests under
//! `crates/emu198x-catalogue/manifest/`.
//!
//! Usage:
//!     catalogue capture --entry <id> [--manifest PATH]
//!     catalogue run [--entry <id>] [--manifest PATH]

use std::env;
use std::path::PathBuf;
use std::process;

use emu198x_catalogue::{
    CatalogueError, Entry, EntryOutcome, Manifest, RunResult, load_manifest, run_entry,
    run_entry_for_capture,
};

const USAGE: &str = "\
Usage:
    catalogue capture --entry <id> [--manifest PATH]
                      [--save-screenshot PATH] [--save-audio PATH]
    catalogue run [--entry <id>] [--manifest PATH]

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
    Ok(args)
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

    let entries: Vec<&Entry> = match &args.entry {
        Some(id) => vec![find_entry(&manifest, id)?],
        None => manifest.entry.iter().collect(),
    };

    let mut failures = 0u32;
    for entry in entries {
        let result = run_entry(&manifest, entry, &media_root, &firmware_root)?;
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
    }

    if failures > 0 {
        process::exit(1);
    }
    Ok(())
}
