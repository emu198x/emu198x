//! `emu198x-script-nes` — minimal headless NES runner.

use std::path::PathBuf;
use std::process;

use emu198x_shell::{
    HeadlessScript, HeadlessSession, MediaImage, MediaKind, MediaSet, ScriptObservation,
    read_media_asset,
};
use runtime_nintendo_nes::{Model, NesRuntime, NesSessionQueryProvider};
use serde::Serialize;

const DEFAULT_CARTRIDGE_SLOT: &str = "cartridge-1";
const NES_FRAME_TICKS: u64 = 341 * 262;

#[derive(Debug, Default, PartialEq, Eq)]
struct Cli {
    media: Vec<MediaArg>,
    screenshot: Option<PathBuf>,
    audio_capture: Option<PathBuf>,
    script: Option<PathBuf>,
    frames: u32,
}

#[derive(Debug, PartialEq, Eq)]
struct MediaArg {
    slot: String,
    kind: MediaKind,
    path: PathBuf,
}

#[derive(Debug)]
struct LoadedMedia {
    slot: String,
    kind: MediaKind,
    bytes: Vec<u8>,
}

#[derive(Debug, Serialize)]
struct RunnerReport {
    observations: Vec<ScriptObservation>,
    time: u64,
    cartridge_loaded: bool,
}

const USAGE: &str = "\
Usage: emu198x-script-nes [OPTIONS]

Media:
    --media SLOT:KIND=PATH     media image by slot and kind
    --rom PATH                 alias for --media cartridge-1:cartridge=PATH

Automation:
    --script PATH              execute shared JSON session steps
    --frames N                 number of native NES video frames to run
    --screenshot PATH          write the last emitted frame as PNG
    --audio-capture PATH       write emitted audio as 16-bit PCM WAV

Other:
    --help, -h                 show this help

Examples:
    emu198x-script-nes --rom nestest.nes --frames 60 --screenshot frame.png
    emu198x-script-nes --rom smb.nes --script steps.json
";

fn main() {
    let cli = parse_cli(std::env::args().skip(1));
    let script_mode = cli.script.is_some();
    match run(cli) {
        Ok(report) => {
            if script_mode {
                let json = serde_json::to_string(&report).unwrap_or_else(|err| {
                    eprintln!("error: failed to serialize runner report: {err}");
                    process::exit(1);
                });
                println!("{json}");
            } else {
                println!(
                    "NES runtime: time={} cartridge_loaded={}",
                    report.time, report.cartridge_loaded
                );
            }
        }
        Err(err) => {
            eprintln!("error: {err}");
            process::exit(1);
        }
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
            "--media" => cli
                .media
                .push(parse_media_arg(&next_arg(&mut iter, "--media"))),
            "--rom" => cli.media.push(MediaArg {
                slot: DEFAULT_CARTRIDGE_SLOT.to_owned(),
                kind: MediaKind::Cartridge,
                path: PathBuf::from(next_arg(&mut iter, "--rom")),
            }),
            "--script" => {
                cli.script = Some(PathBuf::from(next_arg(&mut iter, "--script")));
            }
            "--screenshot" => {
                cli.screenshot = Some(PathBuf::from(next_arg(&mut iter, "--screenshot")));
            }
            "--audio-capture" => {
                cli.audio_capture = Some(PathBuf::from(next_arg(&mut iter, "--audio-capture")));
            }
            "--frames" => {
                cli.frames = next_arg(&mut iter, "--frames")
                    .parse()
                    .unwrap_or_else(|_| die("--frames requires a non-negative integer"));
            }
            "--help" | "-h" => {
                println!("{USAGE}");
                process::exit(0);
            }
            _ => die(&format!("unknown flag: {arg}")),
        }
    }

    cli
}

fn parse_media_arg(spec: &str) -> MediaArg {
    let (slot_and_kind, path) = spec
        .split_once('=')
        .unwrap_or_else(|| die("--media requires SLOT:KIND=PATH"));
    let (slot, kind) = slot_and_kind
        .split_once(':')
        .unwrap_or_else(|| die("--media requires SLOT:KIND=PATH"));
    if slot.is_empty() || kind.is_empty() || path.is_empty() {
        die("--media requires SLOT:KIND=PATH");
    }

    MediaArg {
        slot: slot.to_owned(),
        kind: parse_media_kind(kind),
        path: PathBuf::from(path),
    }
}

fn parse_media_kind(kind: &str) -> MediaKind {
    match kind {
        "cartridge" => MediaKind::Cartridge,
        "disk" => MediaKind::Disk,
        "optical" => MediaKind::Optical,
        "snapshot" => MediaKind::Snapshot,
        "tape" => MediaKind::Tape,
        _ => die(&format!("unknown media kind: {kind}")),
    }
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

fn run(cli: Cli) -> Result<RunnerReport, String> {
    if cli.media.is_empty() {
        return Err(
            "a cartridge image is required; use --rom or --media cartridge-1:cartridge=PATH".into(),
        );
    }

    if (cli.screenshot.is_some() || cli.audio_capture.is_some())
        && cli.frames == 0
        && cli.script.is_none()
    {
        return Err(
            "capture requests require either --frames or --script so the machine emits output"
                .into(),
        );
    }

    let media_storage = load_media_bytes(&cli.media)?;
    let mut media = MediaSet::new();
    for image in &media_storage {
        media.push(MediaImage::new(
            image.slot.clone(),
            image.kind,
            &image.bytes,
        ));
    }

    let machine = NesRuntime::blank(Model::NesNtsc);
    let mut session =
        HeadlessSession::new_with_query_provider(machine, NES_FRAME_TICKS, NesSessionQueryProvider);
    session
        .prepare(&media, &[])
        .map_err(|err| format!("machine preparation failed: {err}"))?;

    let mut observations = Vec::new();
    if let Some(path) = &cli.script {
        let script = HeadlessScript::from_path(path)
            .map_err(|err| format!("failed to load script {}: {err}", path.display()))?;
        observations.extend(
            script
                .execute_collect(&mut session)
                .map_err(|err| format!("script execution failed: {err}"))?,
        );
    }

    if cli.frames > 0 {
        session
            .run_frames(cli.frames)
            .map_err(|err| format!("run failed: {err}"))?;
    }

    if let Some(path) = &cli.screenshot {
        session
            .save_screenshot(path)
            .map_err(|err| format!("failed to write {}: {err}", path.display()))?;
    }

    if let Some(path) = &cli.audio_capture {
        session
            .save_audio_capture(path)
            .map_err(|err| format!("failed to write {}: {err}", path.display()))?;
    }

    Ok(RunnerReport {
        observations,
        time: session.time().get(),
        cartridge_loaded: session.machine().machine().is_some(),
    })
}

fn load_media_bytes(entries: &[MediaArg]) -> Result<Vec<LoadedMedia>, String> {
    entries
        .iter()
        .map(|entry| {
            read_media_asset(&entry.path, entry.kind)
                .map(|loaded| LoadedMedia {
                    slot: entry.slot.clone(),
                    kind: entry.kind,
                    bytes: loaded.bytes,
                })
                .map_err(|err| {
                    format!(
                        "failed to read media {} from {}: {err}",
                        entry.slot,
                        entry.path.display()
                    )
                })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn minimal_ines() -> Vec<u8> {
        let mut prg = vec![0xea; 16 * 1024];
        prg[0x3ffc] = 0x00;
        prg[0x3ffd] = 0x80;
        let chr = vec![0u8; 8 * 1024];
        let mut data = vec![0u8; 16 + prg.len() + chr.len()];
        data[0..4].copy_from_slice(b"NES\x1a");
        data[4] = 1;
        data[5] = 1;
        data[16..16 + prg.len()].copy_from_slice(&prg);
        data[16 + prg.len()..].copy_from_slice(&chr);
        data
    }

    #[test]
    fn parse_cli_accepts_rom_and_capture_flags() {
        let cli = parse_cli([
            "--rom".to_string(),
            "demo.nes".to_string(),
            "--frames".to_string(),
            "12".to_string(),
            "--screenshot".to_string(),
            "frame.png".to_string(),
            "--audio-capture".to_string(),
            "audio.wav".to_string(),
        ]);

        assert_eq!(
            cli,
            Cli {
                media: vec![MediaArg {
                    slot: DEFAULT_CARTRIDGE_SLOT.to_owned(),
                    kind: MediaKind::Cartridge,
                    path: PathBuf::from("demo.nes"),
                }],
                screenshot: Some(PathBuf::from("frame.png")),
                audio_capture: Some(PathBuf::from("audio.wav")),
                script: None,
                frames: 12,
            }
        );
    }

    #[test]
    fn run_can_capture_png_and_wav() {
        let temp_dir = std::env::temp_dir();
        let rom_path = temp_dir.join(format!(
            "emu198x-script-nes-{}-capture.nes",
            std::process::id()
        ));
        let screenshot_path = temp_dir.join(format!(
            "emu198x-script-nes-{}-capture.png",
            std::process::id()
        ));
        let audio_path = temp_dir.join(format!(
            "emu198x-script-nes-{}-capture.wav",
            std::process::id()
        ));

        fs::write(&rom_path, minimal_ines()).expect("temporary ROM write should succeed");

        let result = run(Cli {
            media: vec![MediaArg {
                slot: DEFAULT_CARTRIDGE_SLOT.to_owned(),
                kind: MediaKind::Cartridge,
                path: rom_path.clone(),
            }],
            screenshot: Some(screenshot_path.clone()),
            audio_capture: Some(audio_path.clone()),
            script: None,
            frames: 1,
        });

        assert!(result.is_ok(), "runner should capture outputs: {result:?}");
        let png = fs::read(&screenshot_path).expect("screenshot should be written");
        let wav = fs::read(&audio_path).expect("wav should be written");
        assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
        assert_eq!(&wav[..4], b"RIFF");
        assert_eq!(&wav[8..12], b"WAVE");

        let _ = fs::remove_file(rom_path);
        let _ = fs::remove_file(screenshot_path);
        let _ = fs::remove_file(audio_path);
    }

    #[test]
    fn run_can_execute_shared_json_script() {
        let temp_dir = std::env::temp_dir();
        let rom_path = temp_dir.join(format!(
            "emu198x-script-nes-{}-script.nes",
            std::process::id()
        ));
        let script_path = temp_dir.join(format!(
            "emu198x-script-nes-{}-steps.json",
            std::process::id()
        ));

        fs::write(&rom_path, minimal_ines()).expect("temporary ROM write should succeed");
        fs::write(
            &script_path,
            r#"
            [
              {"action":"run_frames","frames":1},
              {"action":"query","path":"nes.cartridge.loaded"}
            ]
            "#,
        )
        .expect("script fixture should write");

        let result = run(Cli {
            media: vec![MediaArg {
                slot: DEFAULT_CARTRIDGE_SLOT.to_owned(),
                kind: MediaKind::Cartridge,
                path: rom_path.clone(),
            }],
            screenshot: None,
            audio_capture: None,
            script: Some(script_path.clone()),
            frames: 0,
        });

        assert!(result.is_ok(), "runner should execute script: {result:?}");
        let report = result.expect("script result should be available");
        assert_eq!(report.observations.len(), 2);

        let _ = fs::remove_file(rom_path);
        let _ = fs::remove_file(script_path);
    }
}
