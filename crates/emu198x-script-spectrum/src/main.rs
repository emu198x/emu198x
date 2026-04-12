//! `emu198x-script-spectrum` — minimal headless Spectrum runner.
//!
//! This runner exists to exercise the new runtime boundary directly. It owns
//! firmware supply, tape insertion and playback, frame advancement, and
//! runtime snapshot load/save without introducing any frontend concerns.

use std::fs;
use std::path::{Path, PathBuf};
use std::process;

use common_sinclair_zx_spectrum::timing::TIMING_48K;
use emu198x_shell::{
    HostIo, MachineCore, MediaImage, MediaKind, MediaSet, NullAudioSink, NullFrameSink,
    NullTraceSink,
};
use runtime_sinclair_zx_spectrum::Spectrum48kRuntime;

#[derive(Debug, Default, PartialEq, Eq)]
struct Cli {
    rom: Option<PathBuf>,
    tape: Option<PathBuf>,
    load_snapshot: Option<PathBuf>,
    save_snapshot: Option<PathBuf>,
    frames: u32,
    play_tape: bool,
}

const USAGE: &str = "\
Usage: emu198x-script-spectrum [OPTIONS]

Cold boot:
    --rom PATH                 16 KiB Spectrum 48K ROM image

Media and state:
    --tape PATH                TAP or TZX image to load into tape-1
    --play-tape                start tape playback after loading media
    --load-snapshot PATH       restore a runtime snapshot before running
    --save-snapshot PATH       write a runtime snapshot after running

Execution:
    --frames N                 number of native 48K video frames to run

Other:
    --help, -h                 show this help

Examples:
    emu198x-script-spectrum --rom 48.rom --frames 200
    emu198x-script-spectrum --rom 48.rom --tape manic_miner.tzx --play-tape --frames 500
    emu198x-script-spectrum --load-snapshot state.pst --frames 50 --save-snapshot out.pst
";

fn main() {
    let cli = parse_cli(std::env::args().skip(1));
    if let Err(err) = run(cli) {
        eprintln!("error: {err}");
        process::exit(1);
    }
}

fn parse_cli<I>(args: I) -> Cli
where
    I: IntoIterator<Item = String>,
{
    let mut cli = Cli::default();
    let mut iter = args.into_iter();

    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--rom" => cli.rom = Some(PathBuf::from(next_arg(&mut iter, "--rom"))),
            "--tape" => cli.tape = Some(PathBuf::from(next_arg(&mut iter, "--tape"))),
            "--load-snapshot" => {
                cli.load_snapshot = Some(PathBuf::from(next_arg(&mut iter, "--load-snapshot")));
            }
            "--save-snapshot" => {
                cli.save_snapshot = Some(PathBuf::from(next_arg(&mut iter, "--save-snapshot")));
            }
            "--frames" => {
                cli.frames = next_arg(&mut iter, "--frames")
                    .parse()
                    .unwrap_or_else(|_| die("--frames requires a non-negative integer"));
            }
            "--play-tape" => cli.play_tape = true,
            "--help" | "-h" => {
                println!("{USAGE}");
                process::exit(0);
            }
            _ => die(&format!("unknown flag: {arg}")),
        }
    }

    cli
}

fn next_arg<I>(iter: &mut I, flag: &str) -> String
where
    I: Iterator<Item = String>,
{
    iter.next()
        .unwrap_or_else(|| die(&format!("{flag} requires a path or value")))
}

fn die(message: &str) -> ! {
    eprintln!("error: {message}");
    eprintln!();
    eprintln!("{USAGE}");
    process::exit(2);
}

fn run(cli: Cli) -> Result<(), String> {
    if cli.rom.is_none() && cli.load_snapshot.is_none() {
        return Err("either --rom or --load-snapshot must be provided".into());
    }

    let mut runtime = match &cli.rom {
        Some(path) => load_runtime_from_rom(path)?,
        None => Spectrum48kRuntime::blank(),
    };

    if let Some(snapshot_path) = &cli.load_snapshot {
        let snapshot = fs::read(snapshot_path)
            .map_err(|err| format!("failed to read {}: {err}", snapshot_path.display()))?;
        runtime
            .restore(&snapshot)
            .map_err(|err| format!("failed to restore {}: {err}", snapshot_path.display()))?;
    }

    if let Some(tape_path) = &cli.tape {
        load_tape(&mut runtime, tape_path)?;
    }

    if cli.play_tape {
        runtime.play_tape();
    }

    if cli.frames > 0 {
        let target = runtime
            .time()
            .saturating_add(u64::from(cli.frames) * u64::from(TIMING_48K.halfcycles_per_frame));
        let mut frame_sink = NullFrameSink;
        let mut audio_sink = NullAudioSink;
        let mut trace_sink = NullTraceSink;
        let mut host = HostIo {
            input_events: &[],
            frame_sink: &mut frame_sink,
            audio_sink: &mut audio_sink,
            trace_sink: &mut trace_sink,
        };
        runtime
            .run_until(target, &mut host)
            .map_err(|err| format!("run failed: {err}"))?;
    }

    if let Some(snapshot_path) = &cli.save_snapshot {
        let bytes = runtime
            .snapshot()
            .map_err(|err| format!("snapshot failed: {err}"))?;
        fs::write(snapshot_path, bytes)
            .map_err(|err| format!("failed to write {}: {err}", snapshot_path.display()))?;
    }

    println!(
        "Spectrum 48K runtime: time={} tape_loaded={} tape_playing={}",
        runtime.time().get(),
        runtime.machine().tape_is_loaded(),
        runtime.machine().tape_is_playing()
    );

    Ok(())
}

fn load_runtime_from_rom(path: &Path) -> Result<Spectrum48kRuntime, String> {
    let rom = fs::read(path).map_err(|err| format!("failed to read {}: {err}", path.display()))?;
    Spectrum48kRuntime::from_rom_bytes(&rom)
        .map_err(|err| format!("failed to load ROM {}: {err}", path.display()))
}

fn load_tape(runtime: &mut Spectrum48kRuntime, path: &Path) -> Result<(), String> {
    let bytes =
        fs::read(path).map_err(|err| format!("failed to read tape {}: {err}", path.display()))?;
    let mut media = MediaSet::new();
    media.push(MediaImage::new("tape-1", MediaKind::Tape, &bytes));
    runtime
        .load_media(&media)
        .map_err(|err| format!("failed to load tape {}: {err}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_cli_accepts_rom_tape_and_snapshot_paths() {
        let cli = parse_cli([
            "--rom".to_string(),
            "48.rom".to_string(),
            "--tape".to_string(),
            "demo.tzx".to_string(),
            "--play-tape".to_string(),
            "--frames".to_string(),
            "10".to_string(),
            "--save-snapshot".to_string(),
            "out.pst".to_string(),
        ]);

        assert_eq!(
            cli,
            Cli {
                rom: Some(PathBuf::from("48.rom")),
                tape: Some(PathBuf::from("demo.tzx")),
                load_snapshot: None,
                save_snapshot: Some(PathBuf::from("out.pst")),
                frames: 10,
                play_tape: true,
            }
        );
    }

    #[test]
    fn run_can_boot_zero_rom_and_write_snapshot() {
        let temp_dir = std::env::temp_dir();
        let rom_path = temp_dir.join(format!(
            "emu198x-script-spectrum-{}-rom.bin",
            std::process::id()
        ));
        let snapshot_path = temp_dir.join(format!(
            "emu198x-script-spectrum-{}-state.pst",
            std::process::id()
        ));

        fs::write(&rom_path, [0u8; 16 * 1024]).expect("temporary ROM write should succeed");

        let result = run(Cli {
            rom: Some(rom_path.clone()),
            tape: None,
            load_snapshot: None,
            save_snapshot: Some(snapshot_path.clone()),
            frames: 1,
            play_tape: false,
        });

        assert!(result.is_ok(), "runner should complete: {result:?}");
        assert!(snapshot_path.is_file());

        let _ = fs::remove_file(rom_path);
        let _ = fs::remove_file(snapshot_path);
    }
}
