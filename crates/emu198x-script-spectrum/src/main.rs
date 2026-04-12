//! `emu198x-script-spectrum` — minimal headless Spectrum runner.
//!
//! This binary is intentionally thin. It parses file paths and simple workflow
//! flags, then hands control to the shared headless session and script layers
//! in `emu198x-shell`.

use std::fs;
use std::path::PathBuf;
use std::process;

use common_sinclair_zx_spectrum::timing::TIMING_48K;
use emu198x_shell::{
    BootArtifacts, ControlCommand, FirmwareImage, FirmwareSet, HeadlessScript, HeadlessSession,
    MediaImage, MediaKind, MediaSet, MediaTransportAction, MediaTransportCommand, boot_machine,
};
use runtime_sinclair_zx_spectrum::Spectrum48kRuntime;

const DEFAULT_ROM_ID: &str = "sinclair-zx-spectrum-48k-rom";
const DEFAULT_TAPE_SLOT: &str = "tape-1";

#[derive(Debug, Default, PartialEq, Eq)]
struct Cli {
    firmware: Vec<FirmwareArg>,
    media: Vec<MediaArg>,
    load_snapshot: Option<PathBuf>,
    save_snapshot: Option<PathBuf>,
    screenshot: Option<PathBuf>,
    audio_capture: Option<PathBuf>,
    script: Option<PathBuf>,
    frames: u32,
    commands: Vec<ControlCommand>,
}

#[derive(Debug, PartialEq, Eq)]
struct FirmwareArg {
    id: String,
    path: PathBuf,
}

#[derive(Debug, PartialEq, Eq)]
struct MediaArg {
    slot: String,
    kind: MediaKind,
    path: PathBuf,
}

#[derive(Debug)]
struct LoadedFirmware {
    id: String,
    bytes: Vec<u8>,
}

#[derive(Debug)]
struct LoadedMedia {
    slot: String,
    kind: MediaKind,
    bytes: Vec<u8>,
}

const USAGE: &str = "\
Usage: emu198x-script-spectrum [OPTIONS]

Cold boot:
    --firmware ID=PATH         firmware image by stable id
    --rom PATH                 alias for --firmware sinclair-zx-spectrum-48k-rom=PATH

Media and control:
    --media SLOT:KIND=PATH     media image by slot and kind
    --tape PATH                alias for --media tape-1:tape=PATH
    --start-slot SLOT          start or resume media transport on one slot
    --stop-slot SLOT           stop media transport on one slot
    --play-tape                alias for --start-slot tape-1

State and automation:
    --load-snapshot PATH       restore a runtime snapshot before running
    --save-snapshot PATH       write a runtime snapshot after running
    --script PATH              execute shared JSON session steps after boot
    --screenshot PATH          write the last emitted frame as PNG
    --audio-capture PATH       write emitted audio as 16-bit PCM WAV

Execution:
    --frames N                 number of native 48K video frames to run

Other:
    --help, -h                 show this help

Examples:
    emu198x-script-spectrum --rom 48.rom --frames 200 --screenshot boot.png
    emu198x-script-spectrum --rom 48.rom --tape manic_miner.tzx --play-tape --frames 500
    emu198x-script-spectrum --rom 48.rom --script capture.json
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
            "--firmware" => cli
                .firmware
                .push(parse_firmware_arg(&next_arg(&mut iter, "--firmware"))),
            "--rom" => cli.firmware.push(FirmwareArg {
                id: DEFAULT_ROM_ID.to_owned(),
                path: PathBuf::from(next_arg(&mut iter, "--rom")),
            }),
            "--media" => cli
                .media
                .push(parse_media_arg(&next_arg(&mut iter, "--media"))),
            "--tape" => cli.media.push(MediaArg {
                slot: DEFAULT_TAPE_SLOT.to_owned(),
                kind: MediaKind::Tape,
                path: PathBuf::from(next_arg(&mut iter, "--tape")),
            }),
            "--start-slot" => {
                cli.commands
                    .push(ControlCommand::MediaTransport(MediaTransportCommand::new(
                        next_arg(&mut iter, "--start-slot"),
                        MediaTransportAction::Start,
                    )))
            }
            "--stop-slot" => {
                cli.commands
                    .push(ControlCommand::MediaTransport(MediaTransportCommand::new(
                        next_arg(&mut iter, "--stop-slot"),
                        MediaTransportAction::Stop,
                    )))
            }
            "--play-tape" => {
                cli.commands
                    .push(ControlCommand::MediaTransport(MediaTransportCommand::new(
                        DEFAULT_TAPE_SLOT,
                        MediaTransportAction::Start,
                    )))
            }
            "--load-snapshot" => {
                cli.load_snapshot = Some(PathBuf::from(next_arg(&mut iter, "--load-snapshot")));
            }
            "--save-snapshot" => {
                cli.save_snapshot = Some(PathBuf::from(next_arg(&mut iter, "--save-snapshot")));
            }
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

fn parse_firmware_arg(spec: &str) -> FirmwareArg {
    let (id, path) = spec
        .split_once('=')
        .unwrap_or_else(|| die("--firmware requires ID=PATH"));
    if id.is_empty() || path.is_empty() {
        die("--firmware requires ID=PATH");
    }

    FirmwareArg {
        id: id.to_owned(),
        path: PathBuf::from(path),
    }
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
        "tape" => MediaKind::Tape,
        "disk" => MediaKind::Disk,
        "cartridge" => MediaKind::Cartridge,
        "optical" => MediaKind::Optical,
        "snapshot" => MediaKind::Snapshot,
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

fn run(cli: Cli) -> Result<(), String> {
    if (cli.screenshot.is_some() || cli.audio_capture.is_some())
        && cli.frames == 0
        && cli.script.is_none()
    {
        return Err(
            "capture requests require either --frames or --script so the machine emits output"
                .into(),
        );
    }

    let machine = boot_runtime(&cli)?;
    let mut session = HeadlessSession::new(machine, u64::from(TIMING_48K.halfcycles_per_frame));

    let media_storage = load_media_bytes(&cli.media)?;
    let mut media = MediaSet::new();
    for image in &media_storage {
        media.push(MediaImage::new(
            image.slot.clone(),
            image.kind,
            &image.bytes,
        ));
    }
    session
        .prepare(&media, &cli.commands)
        .map_err(|err| format!("machine preparation failed: {err}"))?;

    if let Some(path) = &cli.script {
        let script = HeadlessScript::from_path(path)
            .map_err(|err| format!("failed to load script {}: {err}", path.display()))?;
        script
            .execute(&mut session)
            .map_err(|err| format!("script execution failed: {err}"))?;
    }

    if cli.frames > 0 {
        session
            .run_frames(cli.frames)
            .map_err(|err| format!("run failed: {err}"))?;
    }

    if let Some(path) = &cli.save_snapshot {
        session
            .save_snapshot(path)
            .map_err(|err| format!("failed to write {}: {err}", path.display()))?;
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

    println!(
        "Spectrum 48K runtime: time={} tape_loaded={} tape_playing={}",
        session.time().get(),
        session.machine().machine().tape_is_loaded(),
        session.machine().machine().tape_is_playing()
    );

    Ok(())
}

fn boot_runtime(cli: &Cli) -> Result<Spectrum48kRuntime, String> {
    let firmware_storage = load_firmware_bytes(&cli.firmware)?;
    let mut firmware = FirmwareSet::new();
    for image in &firmware_storage {
        firmware.push(FirmwareImage::new(image.id.clone(), &image.bytes));
    }

    let snapshot_bytes = match &cli.load_snapshot {
        Some(path) => Some(
            fs::read(path).map_err(|err| format!("failed to read {}: {err}", path.display()))?,
        ),
        None => None,
    };

    boot_machine(
        &BootArtifacts {
            firmware,
            snapshot: snapshot_bytes.as_deref(),
        },
        Spectrum48kRuntime::from_firmware,
        Spectrum48kRuntime::blank,
    )
    .map_err(|err| format!("boot failed: {err}"))
}

fn load_firmware_bytes(entries: &[FirmwareArg]) -> Result<Vec<LoadedFirmware>, String> {
    entries
        .iter()
        .map(|entry| {
            fs::read(&entry.path)
                .map(|bytes| LoadedFirmware {
                    id: entry.id.clone(),
                    bytes,
                })
                .map_err(|err| {
                    format!(
                        "failed to read firmware {} from {}: {err}",
                        entry.id,
                        entry.path.display()
                    )
                })
        })
        .collect()
}

fn load_media_bytes(entries: &[MediaArg]) -> Result<Vec<LoadedMedia>, String> {
    entries
        .iter()
        .map(|entry| {
            fs::read(&entry.path)
                .map(|bytes| LoadedMedia {
                    slot: entry.slot.clone(),
                    kind: entry.kind,
                    bytes,
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

    #[test]
    fn parse_cli_accepts_generic_firmware_media_commands_and_script() {
        let cli = parse_cli([
            "--firmware".to_string(),
            "sinclair-zx-spectrum-48k-rom=48.rom".to_string(),
            "--media".to_string(),
            "tape-1:tape=demo.tzx".to_string(),
            "--start-slot".to_string(),
            "tape-1".to_string(),
            "--script".to_string(),
            "steps.json".to_string(),
            "--frames".to_string(),
            "10".to_string(),
            "--screenshot".to_string(),
            "boot.png".to_string(),
            "--audio-capture".to_string(),
            "boot.wav".to_string(),
            "--save-snapshot".to_string(),
            "out.pst".to_string(),
        ]);

        assert_eq!(
            cli,
            Cli {
                firmware: vec![FirmwareArg {
                    id: "sinclair-zx-spectrum-48k-rom".to_owned(),
                    path: PathBuf::from("48.rom"),
                }],
                media: vec![MediaArg {
                    slot: "tape-1".to_owned(),
                    kind: MediaKind::Tape,
                    path: PathBuf::from("demo.tzx"),
                }],
                load_snapshot: None,
                save_snapshot: Some(PathBuf::from("out.pst")),
                screenshot: Some(PathBuf::from("boot.png")),
                audio_capture: Some(PathBuf::from("boot.wav")),
                script: Some(PathBuf::from("steps.json")),
                frames: 10,
                commands: vec![ControlCommand::MediaTransport(MediaTransportCommand::new(
                    "tape-1",
                    MediaTransportAction::Start,
                ))],
            }
        );
    }

    #[test]
    fn parse_cli_keeps_legacy_spectrum_aliases_working() {
        let cli = parse_cli([
            "--rom".to_string(),
            "48.rom".to_string(),
            "--tape".to_string(),
            "demo.tap".to_string(),
            "--play-tape".to_string(),
        ]);

        assert_eq!(
            cli,
            Cli {
                firmware: vec![FirmwareArg {
                    id: DEFAULT_ROM_ID.to_owned(),
                    path: PathBuf::from("48.rom"),
                }],
                media: vec![MediaArg {
                    slot: DEFAULT_TAPE_SLOT.to_owned(),
                    kind: MediaKind::Tape,
                    path: PathBuf::from("demo.tap"),
                }],
                load_snapshot: None,
                save_snapshot: None,
                screenshot: None,
                audio_capture: None,
                script: None,
                frames: 0,
                commands: vec![ControlCommand::MediaTransport(MediaTransportCommand::new(
                    DEFAULT_TAPE_SLOT,
                    MediaTransportAction::Start,
                ))],
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
            firmware: vec![FirmwareArg {
                id: DEFAULT_ROM_ID.to_owned(),
                path: rom_path.clone(),
            }],
            media: vec![],
            load_snapshot: None,
            save_snapshot: Some(snapshot_path.clone()),
            screenshot: None,
            audio_capture: None,
            script: None,
            frames: 1,
            commands: vec![],
        });

        assert!(result.is_ok(), "runner should complete: {result:?}");
        assert!(snapshot_path.is_file());

        let _ = fs::remove_file(rom_path);
        let _ = fs::remove_file(snapshot_path);
    }

    #[test]
    fn run_can_capture_png_and_wav() {
        let temp_dir = std::env::temp_dir();
        let rom_path = temp_dir.join(format!(
            "emu198x-script-spectrum-{}-capture-rom.bin",
            std::process::id()
        ));
        let screenshot_path = temp_dir.join(format!(
            "emu198x-script-spectrum-{}-capture.png",
            std::process::id()
        ));
        let audio_path = temp_dir.join(format!(
            "emu198x-script-spectrum-{}-capture.wav",
            std::process::id()
        ));

        fs::write(&rom_path, [0u8; 16 * 1024]).expect("temporary ROM write should succeed");

        let result = run(Cli {
            firmware: vec![FirmwareArg {
                id: DEFAULT_ROM_ID.to_owned(),
                path: rom_path.clone(),
            }],
            media: vec![],
            load_snapshot: None,
            save_snapshot: None,
            screenshot: Some(screenshot_path.clone()),
            audio_capture: Some(audio_path.clone()),
            script: None,
            frames: 1,
            commands: vec![],
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
            "emu198x-script-spectrum-{}-script-rom.bin",
            std::process::id()
        ));
        let script_path = temp_dir.join(format!(
            "emu198x-script-spectrum-{}-steps.json",
            std::process::id()
        ));
        let screenshot_path = temp_dir.join(format!(
            "emu198x-script-spectrum-{}-script.png",
            std::process::id()
        ));
        let audio_path = temp_dir.join(format!(
            "emu198x-script-spectrum-{}-script.wav",
            std::process::id()
        ));

        fs::write(&rom_path, [0u8; 16 * 1024]).expect("temporary ROM write should succeed");
        fs::write(
            &script_path,
            format!(
                r#"
                [
                  {{"action":"run_frames","frames":1}},
                  {{"action":"save_screenshot","path":"{}"}},
                  {{"action":"save_audio_capture","path":"{}","reset_after":true}}
                ]
                "#,
                screenshot_path.display(),
                audio_path.display()
            ),
        )
        .expect("script fixture should write");

        let result = run(Cli {
            firmware: vec![FirmwareArg {
                id: DEFAULT_ROM_ID.to_owned(),
                path: rom_path.clone(),
            }],
            media: vec![],
            load_snapshot: None,
            save_snapshot: None,
            screenshot: None,
            audio_capture: None,
            script: Some(script_path.clone()),
            frames: 0,
            commands: vec![],
        });

        assert!(result.is_ok(), "runner should execute script: {result:?}");
        assert!(screenshot_path.is_file());
        assert!(audio_path.is_file());

        let _ = fs::remove_file(rom_path);
        let _ = fs::remove_file(script_path);
        let _ = fs::remove_file(screenshot_path);
        let _ = fs::remove_file(audio_path);
    }
}
