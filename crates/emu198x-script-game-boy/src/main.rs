//! `emu198x-script-game-boy` — minimal headless Game Boy runner.

use std::path::{Path, PathBuf};
use std::process;

use common_nintendo_game_boy::MCYCLES_PER_FRAME;
use emu198x_shell::{
    HeadlessScript, HeadlessSession, MediaImage, MediaKind, MediaSet, ScriptObservation,
    read_media_asset,
};
use runtime_nintendo_game_boy::{GameBoyRuntime, GameBoySessionQueryProvider, Model};
use serde::Serialize;

const DEFAULT_CARTRIDGE_SLOT: &str = "cartridge";

#[derive(Debug, PartialEq, Eq)]
struct Cli {
    model: Model,
    media: Vec<MediaArg>,
    load_snapshot: Option<PathBuf>,
    save_snapshot: Option<PathBuf>,
    screenshot: Option<PathBuf>,
    audio_capture: Option<PathBuf>,
    battery_save: Option<PathBuf>,
    no_battery_save: bool,
    script: Option<PathBuf>,
    frames: u32,
}

impl Default for Cli {
    fn default() -> Self {
        Self {
            model: Model::Dmg,
            media: Vec::new(),
            load_snapshot: None,
            save_snapshot: None,
            screenshot: None,
            audio_capture: None,
            battery_save: None,
            no_battery_save: false,
            script: None,
            frames: 0,
        }
    }
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
Usage: emu198x-script-game-boy [OPTIONS]

Model:
    --model MODEL              dmg0 | dmg | mgb | sgb | sgb2 [default: dmg]

Media:
    --media SLOT:KIND=PATH     media image by slot and kind
    --rom PATH                 alias for --media cartridge:cartridge=PATH

Automation:
    --script PATH              execute shared JSON session steps
    --frames N                 number of native Game Boy video frames to run
    --load-snapshot PATH       restore a runtime snapshot before running
    --save-snapshot PATH       write a runtime snapshot after running
    --screenshot PATH          write the last emitted frame as PNG
    --audio-capture PATH       write emitted audio as 16-bit PCM WAV
    --battery-save PATH        load/write cartridge battery RAM sidecar
    --no-battery-save          disable automatic .sav load/write

Other:
    --help, -h                 show this help

Examples:
    emu198x-script-game-boy --rom tetris.gb --frames 60 --screenshot frame.png
    emu198x-script-game-boy --rom game.gb --script steps.json
    emu198x-script-game-boy --load-snapshot ready.gb.pst --frames 10 --save-snapshot later.gb.pst
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
                    "Game Boy runtime: time={} cartridge_loaded={}",
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
            "--model" => {
                cli.model = parse_model_arg(&next_arg(&mut iter, "--model"));
            }
            "--media" => cli
                .media
                .push(parse_media_arg(&next_arg(&mut iter, "--media"))),
            "--rom" => cli.media.push(MediaArg {
                slot: DEFAULT_CARTRIDGE_SLOT.to_owned(),
                kind: MediaKind::Cartridge,
                path: PathBuf::from(next_arg(&mut iter, "--rom")),
            }),
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
            "--battery-save" => {
                cli.battery_save = Some(PathBuf::from(next_arg(&mut iter, "--battery-save")));
            }
            "--no-battery-save" => {
                cli.no_battery_save = true;
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

fn parse_model_arg(model: &str) -> Model {
    match model {
        "dmg0" => Model::Dmg0,
        "dmg" => Model::Dmg,
        "mgb" => Model::Mgb,
        "sgb" => Model::Sgb,
        "sgb2" => Model::Sgb2,
        _ => die("--model expects dmg0, dmg, mgb, sgb, or sgb2"),
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
    if cli.no_battery_save && cli.battery_save.is_some() {
        return Err("--battery-save conflicts with --no-battery-save".into());
    }
    if cli.media.is_empty() && cli.load_snapshot.is_none() {
        return Err("a cartridge image or snapshot is required; use --rom, --media cartridge:cartridge=PATH, or --load-snapshot".into());
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

    let machine = GameBoyRuntime::blank(cli.model);
    let mut session = HeadlessSession::new_with_query_provider(
        machine,
        u64::from(MCYCLES_PER_FRAME),
        GameBoySessionQueryProvider,
    );

    if let Some(path) = &cli.load_snapshot {
        let bytes = std::fs::read(path)
            .map_err(|err| format!("failed to read snapshot {}: {err}", path.display()))?;
        session
            .restore_snapshot(&bytes)
            .map_err(|err| format!("snapshot restore failed: {err}"))?;
    }

    session
        .prepare(&media, &[])
        .map_err(|err| format!("machine preparation failed: {err}"))?;

    let battery_save_path = resolve_battery_save_path(&cli);
    if let Some(path) = &battery_save_path {
        load_battery_save(session.machine_mut(), path, cli.battery_save.is_some())?;
    }

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

    if let Some(path) = &cli.save_snapshot {
        session
            .save_snapshot(path)
            .map_err(|err| format!("failed to write snapshot {}: {err}", path.display()))?;
    }

    if let Some(path) = &battery_save_path {
        write_battery_save(session.machine(), path)?;
    }

    Ok(RunnerReport {
        observations,
        time: session.time().get(),
        cartridge_loaded: session.machine().machine().is_some(),
    })
}

fn resolve_battery_save_path(cli: &Cli) -> Option<PathBuf> {
    if cli.no_battery_save {
        return None;
    }
    cli.battery_save.clone().or_else(|| {
        cli.media
            .iter()
            .find(|entry| {
                entry.slot == DEFAULT_CARTRIDGE_SLOT && entry.kind == MediaKind::Cartridge
            })
            .map(|entry| default_battery_save_path(&entry.path))
    })
}

fn default_battery_save_path(rom_path: &Path) -> PathBuf {
    let mut path = rom_path.to_path_buf();
    path.set_extension("sav");
    path
}

fn load_battery_save(
    runtime: &mut GameBoyRuntime,
    path: &Path,
    explicit: bool,
) -> Result<(), String> {
    if !runtime.has_battery_backed_ram() {
        if explicit {
            return Err("loaded cartridge does not have battery-backed RAM".to_owned());
        }
        return Ok(());
    }

    match std::fs::read(path) {
        Ok(bytes) => runtime
            .restore_cartridge_ram(&bytes)
            .map_err(|err| format!("failed to restore battery save {}: {err}", path.display())),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(format!(
            "failed to read battery save {}: {err}",
            path.display()
        )),
    }
}

fn write_battery_save(runtime: &GameBoyRuntime, path: &Path) -> Result<(), String> {
    if !runtime.has_battery_backed_ram() {
        return Ok(());
    }
    let Some(ram) = runtime.cartridge_ram() else {
        return Ok(());
    };
    std::fs::write(path, ram)
        .map_err(|err| format!("failed to write battery save {}: {err}", path.display()))
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

    fn loop_rom() -> Vec<u8> {
        let mut rom = vec![0x00; 0x8000];
        rom[0x0100] = 0x18; // JR
        rom[0x0101] = 0xFE; // -2: tight loop
        rom[0x0147] = 0x00; // ROM only
        rom[0x0148] = 0x00; // 32 KiB
        rom[0x0149] = 0x00; // no external RAM
        let mut checksum: u8 = 0;
        for &byte in &rom[0x0134..=0x014C] {
            checksum = checksum.wrapping_sub(byte).wrapping_sub(1);
        }
        rom[0x014D] = checksum;
        rom
    }

    fn battery_ram_rom() -> Vec<u8> {
        let mut rom = loop_rom();
        rom[0x0147] = 0x03; // MBC1 + RAM + battery
        rom[0x0149] = 0x02; // 8 KiB RAM
        let mut checksum: u8 = 0;
        for &byte in &rom[0x0134..=0x014C] {
            checksum = checksum.wrapping_sub(byte).wrapping_sub(1);
        }
        rom[0x014D] = checksum;
        rom
    }

    #[test]
    fn parse_cli_accepts_model_rom_and_capture_flags() {
        let cli = parse_cli([
            "--model".to_string(),
            "mgb".to_string(),
            "--rom".to_string(),
            "demo.gb".to_string(),
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
                model: Model::Mgb,
                media: vec![MediaArg {
                    slot: DEFAULT_CARTRIDGE_SLOT.to_owned(),
                    kind: MediaKind::Cartridge,
                    path: PathBuf::from("demo.gb"),
                }],
                load_snapshot: None,
                save_snapshot: None,
                screenshot: Some(PathBuf::from("frame.png")),
                audio_capture: Some(PathBuf::from("audio.wav")),
                battery_save: None,
                no_battery_save: false,
                script: None,
                frames: 12,
            }
        );
    }

    #[test]
    fn default_battery_save_path_replaces_rom_extension() {
        assert_eq!(
            default_battery_save_path(Path::new("game.gb")),
            PathBuf::from("game.sav")
        );
    }

    #[test]
    fn parse_cli_accepts_battery_save_controls() {
        let cli = parse_cli([
            "--rom".to_string(),
            "demo.gb".to_string(),
            "--battery-save".to_string(),
            "demo-state.sav".to_string(),
        ]);

        assert_eq!(cli.battery_save, Some(PathBuf::from("demo-state.sav")));
        assert_eq!(
            resolve_battery_save_path(&cli),
            Some(PathBuf::from("demo-state.sav"))
        );

        let cli = parse_cli([
            "--rom".to_string(),
            "demo.gb".to_string(),
            "--no-battery-save".to_string(),
        ]);
        assert!(cli.no_battery_save);
        assert_eq!(resolve_battery_save_path(&cli), None);
    }

    #[test]
    fn run_can_capture_png_wav_and_snapshot() {
        let temp_dir = std::env::temp_dir();
        let rom_path = temp_dir.join(format!(
            "emu198x-script-game-boy-{}-capture.gb",
            std::process::id()
        ));
        let screenshot_path = temp_dir.join(format!(
            "emu198x-script-game-boy-{}-capture.png",
            std::process::id()
        ));
        let audio_path = temp_dir.join(format!(
            "emu198x-script-game-boy-{}-capture.wav",
            std::process::id()
        ));
        let snapshot_path = temp_dir.join(format!(
            "emu198x-script-game-boy-{}-capture.pst",
            std::process::id()
        ));

        fs::write(&rom_path, loop_rom()).expect("temporary ROM write should succeed");

        let result = run(Cli {
            model: Model::Dmg,
            media: vec![MediaArg {
                slot: DEFAULT_CARTRIDGE_SLOT.to_owned(),
                kind: MediaKind::Cartridge,
                path: rom_path.clone(),
            }],
            load_snapshot: None,
            save_snapshot: Some(snapshot_path.clone()),
            screenshot: Some(screenshot_path.clone()),
            audio_capture: Some(audio_path.clone()),
            battery_save: None,
            no_battery_save: false,
            script: None,
            frames: 1,
        });

        assert!(result.is_ok(), "runner should capture outputs: {result:?}");
        let png = fs::read(&screenshot_path).expect("screenshot should be written");
        let wav = fs::read(&audio_path).expect("wav should be written");
        assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
        assert_eq!(&wav[..4], b"RIFF");
        assert_eq!(&wav[8..12], b"WAVE");
        assert!(snapshot_path.is_file());

        let _ = fs::remove_file(rom_path);
        let _ = fs::remove_file(screenshot_path);
        let _ = fs::remove_file(audio_path);
        let _ = fs::remove_file(snapshot_path);
    }

    #[test]
    fn run_loads_and_writes_battery_save() {
        let temp_dir = std::env::temp_dir();
        let rom_path = temp_dir.join(format!(
            "emu198x-script-game-boy-{}-battery.gb",
            std::process::id()
        ));
        let save_path = temp_dir.join(format!(
            "emu198x-script-game-boy-{}-battery.sav",
            std::process::id()
        ));
        let save = vec![0x5A; 0x2000];

        fs::write(&rom_path, battery_ram_rom()).expect("temporary ROM write should succeed");
        fs::write(&save_path, &save).expect("temporary save write should succeed");

        let result = run(Cli {
            model: Model::Dmg,
            media: vec![MediaArg {
                slot: DEFAULT_CARTRIDGE_SLOT.to_owned(),
                kind: MediaKind::Cartridge,
                path: rom_path.clone(),
            }],
            load_snapshot: None,
            save_snapshot: None,
            screenshot: None,
            audio_capture: None,
            battery_save: Some(save_path.clone()),
            no_battery_save: false,
            script: None,
            frames: 0,
        });

        assert!(
            result.is_ok(),
            "runner should preserve battery save: {result:?}"
        );
        assert_eq!(
            fs::read(&save_path).expect("battery save should be readable"),
            save
        );

        let _ = fs::remove_file(rom_path);
        let _ = fs::remove_file(save_path);
    }

    #[test]
    fn run_can_execute_shared_json_script() {
        let temp_dir = std::env::temp_dir();
        let rom_path = temp_dir.join(format!(
            "emu198x-script-game-boy-{}-script.gb",
            std::process::id()
        ));
        let script_path = temp_dir.join(format!(
            "emu198x-script-game-boy-{}-steps.json",
            std::process::id()
        ));

        fs::write(&rom_path, loop_rom()).expect("temporary ROM write should succeed");
        fs::write(
            &script_path,
            r#"
            [
              {"action":"run_frames","frames":1},
              {"action":"query","path":"gameboy.cartridge.loaded"}
            ]
            "#,
        )
        .expect("script fixture should write");

        let result = run(Cli {
            model: Model::Dmg,
            media: vec![MediaArg {
                slot: DEFAULT_CARTRIDGE_SLOT.to_owned(),
                kind: MediaKind::Cartridge,
                path: rom_path.clone(),
            }],
            load_snapshot: None,
            save_snapshot: None,
            screenshot: None,
            audio_capture: None,
            battery_save: None,
            no_battery_save: false,
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
