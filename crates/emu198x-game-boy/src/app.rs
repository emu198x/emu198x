//! The Game Boy as a [`MachineApp`]: its flags, runtime, and report fields.

use std::path::{Path, PathBuf};

use common_nintendo_game_boy::MCYCLES_PER_FRAME;
use emu198x_shell::launch::{Args, CommonCli, LaunchError, MachineApp, read_rom, script_report};
use emu198x_shell::{HeadlessSession, MachineCore, MediaKind, read_media_asset, startup_media};
use runtime_nintendo_game_boy::{GameBoyRuntime, GameBoySessionQueryProvider, Model};
use serde_json::{Map, Value};

const DEFAULT_CARTRIDGE_SLOT: &str = "cartridge";

/// One `--media SLOT:KIND=PATH` (or `--rom PATH`) from the command line.
#[derive(Debug, PartialEq, Eq)]
pub struct MediaArg {
    pub slot: String,
    pub kind: MediaKind,
    pub path: PathBuf,
}

impl MediaArg {
    fn cartridge(path: PathBuf) -> Self {
        Self {
            slot: DEFAULT_CARTRIDGE_SLOT.to_owned(),
            kind: MediaKind::Cartridge,
            path,
        }
    }
}

/// The machine configuration the flags build up.
#[derive(Debug, PartialEq, Eq)]
pub struct GameBoy {
    pub model: Model,
    pub media: Vec<MediaArg>,
    /// `--load-snapshot PATH`: restored before any media is loaded.
    pub load_snapshot: Option<PathBuf>,
    /// `--save-snapshot PATH`: written after the run.
    pub save_snapshot: Option<PathBuf>,
    pub battery_save: Option<PathBuf>,
    pub no_battery_save: bool,
    /// Whether a bare ROM path (no flag) has been taken; only one is allowed.
    positional_rom: bool,
}

impl Default for GameBoy {
    fn default() -> Self {
        Self {
            model: Model::Dmg,
            media: Vec::new(),
            load_snapshot: None,
            save_snapshot: None,
            battery_save: None,
            no_battery_save: false,
            positional_rom: false,
        }
    }
}

fn parse_model_arg(model: &str) -> Result<Model, LaunchError> {
    Ok(match model {
        "dmg0" => Model::Dmg0,
        "dmg" => Model::Dmg,
        "mgb" => Model::Mgb,
        "sgb" => Model::Sgb,
        "sgb2" => Model::Sgb2,
        _ => {
            return Err(LaunchError::Usage(
                "--model expects dmg0, dmg, mgb, sgb, or sgb2".to_owned(),
            ));
        }
    })
}

fn parse_media_arg(spec: &str) -> Result<MediaArg, LaunchError> {
    let malformed = || LaunchError::Usage("--media requires SLOT:KIND=PATH".to_owned());
    let (slot_and_kind, path) = spec.split_once('=').ok_or_else(malformed)?;
    let (slot, kind) = slot_and_kind.split_once(':').ok_or_else(malformed)?;
    if slot.is_empty() || kind.is_empty() || path.is_empty() {
        return Err(malformed());
    }
    Ok(MediaArg {
        slot: slot.to_owned(),
        kind: parse_media_kind(kind)?,
        path: PathBuf::from(path),
    })
}

fn parse_media_kind(kind: &str) -> Result<MediaKind, LaunchError> {
    Ok(match kind {
        "cartridge" => MediaKind::Cartridge,
        "disk" => MediaKind::Disk,
        "optical" => MediaKind::Optical,
        "program" => MediaKind::Program,
        "snapshot" => MediaKind::Snapshot,
        "tape" => MediaKind::Tape,
        _ => return Err(LaunchError::Usage(format!("unknown media kind: {kind}"))),
    })
}

/// Resolve the battery `.sav` sidecar path: `None` when disabled, an
/// explicit `--battery-save` path, or `<rom>.sav` next to the cartridge.
pub(crate) fn resolve_battery_save_path(app: &GameBoy) -> Option<PathBuf> {
    if app.no_battery_save {
        return None;
    }
    app.battery_save.clone().or_else(|| {
        app.media
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

/// Load a battery `.sav` into the cartridge. A missing file is fine (fresh
/// save). An explicit `--battery-save` on a non-persistent cartridge is an
/// error; the default path is silently skipped.
fn load_battery_save(
    runtime: &mut GameBoyRuntime,
    path: &Path,
    explicit: bool,
) -> Result<(), LaunchError> {
    if !runtime.has_persistent_cartridge_state() {
        if explicit {
            return Err(LaunchError::Run(
                "loaded cartridge does not have battery-backed RAM".to_owned(),
            ));
        }
        return Ok(());
    }

    match std::fs::read(path) {
        Ok(bytes) => runtime.restore_cartridge_save_image(&bytes).map_err(|err| {
            LaunchError::Run(format!(
                "failed to restore battery save {}: {err}",
                path.display()
            ))
        }),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(LaunchError::Run(format!(
            "failed to read battery save {}: {err}",
            path.display()
        ))),
    }
}

/// Persist the cartridge save image (RAM + RTC footer). The footer stamps
/// wall-clock so the clock keeps running across restarts.
pub(crate) fn write_battery_save(runtime: &mut GameBoyRuntime, path: &Path) -> Result<(), String> {
    if !runtime.has_persistent_cartridge_state() {
        return Ok(());
    }
    let Some(image) = runtime.cartridge_save_image() else {
        return Ok(());
    };
    std::fs::write(path, image)
        .map_err(|err| format!("failed to write battery save {}: {err}", path.display()))
}

fn load_media_bytes(
    entries: &[MediaArg],
) -> Result<Vec<(String, MediaKind, Vec<u8>)>, LaunchError> {
    entries
        .iter()
        .map(|entry| {
            read_media_asset(&entry.path, entry.kind)
                .map(|loaded| (entry.slot.clone(), entry.kind, loaded.bytes))
                .map_err(|err| {
                    LaunchError::Run(format!(
                        "failed to read media {} from {}: {err}",
                        entry.slot,
                        entry.path.display()
                    ))
                })
        })
        .collect()
}

impl MachineApp for GameBoy {
    type Runtime = GameBoyRuntime;
    type Query = GameBoySessionQueryProvider;

    const BIN_NAME: &'static str = "emu198x-game-boy";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");
    /// Headless-only flags; their presence routes to script mode.
    /// `--load-snapshot` is deliberately absent: the window restores one too.
    const SCRIPT_FLAGS: &'static [&'static str] = &["--media", "--save-snapshot"];
    const MACHINE_OPTIONS: &'static str =
        "    --rom PATH      Game Boy ROM image or zip containing one ROM candidate
                    (also accepted as a bare positional path)
    --media SLOT:KIND=PATH  media image by slot and kind; --rom is an
                    alias for --media cartridge:cartridge=PATH
    --model MODEL   dmg0 | dmg | mgb | sgb | sgb2 [default: dmg]
    --load-snapshot PATH    restore a runtime snapshot before starting
    --save-snapshot PATH    write a runtime snapshot after a headless run
    --battery-save PATH     load/write cartridge battery RAM sidecar
    --no-battery-save       disable automatic .sav load/write";
    const CONTROLS: &'static str = "    Esc             quit
    F12             hard reset
    1-4             toggle audio channels: pulse1, pulse2, wave, noise
    5-8             cycle channel gain: 100%, 50%, 25%, muted
    0               reset audio channel controls
    Arrow keys      D-pad
    Z               B
    X               A
    Right Shift     Select
    Enter           Start";

    fn parse_flag(&mut self, flag: &str, args: &mut Args) -> Result<bool, LaunchError> {
        match flag {
            "--model" => self.model = parse_model_arg(&args.value(flag)?)?,
            "--media" => self.media.push(parse_media_arg(&args.value(flag)?)?),
            "--rom" => self.media.push(MediaArg::cartridge(args.path(flag)?)),
            "--load-snapshot" => self.load_snapshot = Some(args.path(flag)?),
            "--save-snapshot" => self.save_snapshot = Some(args.path(flag)?),
            "--battery-save" => self.battery_save = Some(args.path(flag)?),
            "--no-battery-save" => self.no_battery_save = true,
            positional if !positional.starts_with('-') => {
                if self.positional_rom {
                    return Err(LaunchError::Usage(
                        "only one positional ROM path is supported".to_owned(),
                    ));
                }
                self.positional_rom = true;
                self.media
                    .push(MediaArg::cartridge(PathBuf::from(positional)));
            }
            _ => return Ok(false),
        }
        Ok(true)
    }

    fn frame_ticks(&self) -> u64 {
        u64::from(MCYCLES_PER_FRAME)
    }

    fn query_provider(&self) -> GameBoySessionQueryProvider {
        GameBoySessionQueryProvider
    }

    /// Blank machine, then in order: the snapshot, the media, the battery
    /// save. The window and the headless loop both start from this.
    fn build_runtime(&self) -> Result<GameBoyRuntime, LaunchError> {
        if self.no_battery_save && self.battery_save.is_some() {
            return Err(LaunchError::Run(
                "--battery-save conflicts with --no-battery-save".to_owned(),
            ));
        }
        if self.media.is_empty() && self.load_snapshot.is_none() {
            return Err(LaunchError::Run(
                "a cartridge image or snapshot is required; use --rom, --media cartridge:cartridge=PATH, or --load-snapshot"
                    .to_owned(),
            ));
        }

        let loaded = load_media_bytes(&self.media)?;
        let mut runtime = GameBoyRuntime::blank(self.model);
        if let Some(path) = &self.load_snapshot {
            let bytes = read_rom(path, "snapshot")?;
            runtime
                .restore(&bytes)
                .map_err(|err| LaunchError::Run(format!("snapshot restore failed: {err}")))?;
        }
        if !loaded.is_empty() {
            runtime
                .load_media(&startup_media::media_set(&loaded))
                .map_err(|err| LaunchError::Run(format!("machine preparation failed: {err}")))?;
        }
        if let Some(path) = resolve_battery_save_path(self) {
            load_battery_save(&mut runtime, &path, self.battery_save.is_some())?;
        }
        Ok(runtime)
    }

    /// MCP starts blank; the cartridge arrives via `load_media`.
    fn build_mcp_runtime(&self) -> Result<GameBoyRuntime, LaunchError> {
        Ok(GameBoyRuntime::blank(self.model))
    }

    /// Write the snapshot and the battery save before captures.
    fn after_run(
        &self,
        session: &mut HeadlessSession<GameBoyRuntime, GameBoySessionQueryProvider>,
    ) -> Result<(), LaunchError> {
        if let Some(path) = &self.save_snapshot {
            session.save_snapshot(path).map_err(|err| {
                LaunchError::Run(format!(
                    "failed to write snapshot {}: {err}",
                    path.display()
                ))
            })?;
        }
        if let Some(path) = resolve_battery_save_path(self) {
            write_battery_save(session.machine_mut(), &path)?;
        }
        Ok(())
    }

    fn report(&self, runtime: &GameBoyRuntime, report: &mut Map<String, Value>) {
        report.insert(
            "cartridge_loaded".to_owned(),
            runtime.machine().is_some().into(),
        );
    }

    /// JSON when a script ran; a one-line summary for a bare frame run.
    fn run_script(&self, common: &CommonCli, _raw_args: &[String]) -> Result<(), LaunchError> {
        let report = script_report(self, common)?;
        if common.script.is_some() {
            let json = serde_json::to_string(&report).map_err(|err| {
                LaunchError::Run(format!("failed to serialize runner report: {err}"))
            })?;
            println!("{json}");
        } else {
            println!(
                "Game Boy runtime: time={} cartridge_loaded={}",
                report.get("time").and_then(Value::as_u64).unwrap_or(0),
                report
                    .get("cartridge_loaded")
                    .and_then(Value::as_bool)
                    .unwrap_or(false)
            );
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use emu198x_shell::launch::{Mode, Parsed, parse};
    use std::fs;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| (*s).to_owned()).collect()
    }

    fn parsed(list: &[&str]) -> (GameBoy, CommonCli, Mode) {
        match parse::<GameBoy>(&args(list)).expect("parses") {
            Parsed::Run { app, common, mode } => (app, common, mode),
            Parsed::Help => panic!("expected a run"),
        }
    }

    fn cartridge(path: &Path) -> GameBoy {
        GameBoy {
            media: vec![MediaArg::cartridge(path.to_path_buf())],
            ..GameBoy::default()
        }
    }

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
    fn flags_set_model_rom_and_capture_flags() {
        let (app, common, mode) = parsed(&[
            "--model",
            "mgb",
            "--rom",
            "demo.gb",
            "--frames",
            "12",
            "--screenshot",
            "frame.png",
            "--audio-capture",
            "audio.wav",
        ]);
        assert_eq!(app.model, Model::Mgb);
        assert_eq!(
            app.media,
            vec![MediaArg::cartridge(PathBuf::from("demo.gb"))]
        );
        assert_eq!(common.frames, 12);
        assert_eq!(common.screenshot, Some(PathBuf::from("frame.png")));
        assert_eq!(common.audio_capture, Some(PathBuf::from("audio.wav")));
        assert_eq!(mode, Mode::Script);
    }

    #[test]
    fn media_and_save_snapshot_route_headless_but_load_snapshot_does_not() {
        let (.., mode) = parsed(&["--media", "cartridge:cartridge=game.gb"]);
        assert_eq!(mode, Mode::Script);
        let (.., mode) = parsed(&["--rom", "game.gb", "--save-snapshot", "later.pst"]);
        assert_eq!(mode, Mode::Script);
        let (.., mode) = parsed(&["--load-snapshot", "ready.gb.pst"]);
        assert_eq!(mode, Mode::Ui);
    }

    #[test]
    fn a_positional_rom_opens_the_window() {
        let (app, common, mode) = parsed(&["--model", "mgb", "--scale", "5", "game.gb"]);
        assert_eq!(
            app.media,
            vec![MediaArg::cartridge(PathBuf::from("game.gb"))]
        );
        assert_eq!(common.scale, Some(5));
        assert_eq!(mode, Mode::Ui);

        let err = parse::<GameBoy>(&args(&["a.gb", "b.gb"])).expect_err("rejects");
        assert_eq!(
            err,
            LaunchError::Usage("only one positional ROM path is supported".to_owned())
        );
    }

    #[test]
    fn a_bad_model_or_media_spec_is_a_usage_error() {
        assert_eq!(
            parse::<GameBoy>(&args(&["--model", "cgb"])).expect_err("rejects"),
            LaunchError::Usage("--model expects dmg0, dmg, mgb, sgb, or sgb2".to_owned())
        );
        assert_eq!(
            parse::<GameBoy>(&args(&["--media", "cartridge=x.gb"])).expect_err("rejects"),
            LaunchError::Usage("--media requires SLOT:KIND=PATH".to_owned())
        );
        assert_eq!(
            parse::<GameBoy>(&args(&["--media", "cartridge:floppy=x.gb"])).expect_err("rejects"),
            LaunchError::Usage("unknown media kind: floppy".to_owned())
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
    fn battery_save_controls_resolve_the_sidecar() {
        let (app, ..) = parsed(&["--rom", "demo.gb", "--battery-save", "demo-state.sav"]);
        assert_eq!(app.battery_save, Some(PathBuf::from("demo-state.sav")));
        assert_eq!(
            resolve_battery_save_path(&app),
            Some(PathBuf::from("demo-state.sav"))
        );

        let (app, ..) = parsed(&["--rom", "demo.gb", "--no-battery-save"]);
        assert!(app.no_battery_save);
        assert_eq!(resolve_battery_save_path(&app), None);

        let (app, ..) = parsed(&["demo.gb"]);
        assert_eq!(
            resolve_battery_save_path(&app),
            Some(PathBuf::from("demo.sav"))
        );
    }

    #[test]
    fn run_can_capture_png_wav_and_snapshot() {
        let temp_dir = std::env::temp_dir();
        let stem = format!("emu198x-game-boy-{}-capture", std::process::id());
        let rom_path = temp_dir.join(format!("{stem}.gb"));
        let screenshot_path = temp_dir.join(format!("{stem}.png"));
        let audio_path = temp_dir.join(format!("{stem}.wav"));
        let snapshot_path = temp_dir.join(format!("{stem}.pst"));

        fs::write(&rom_path, loop_rom()).expect("temporary ROM write should succeed");

        let result = script_report(
            &GameBoy {
                save_snapshot: Some(snapshot_path.clone()),
                ..cartridge(&rom_path)
            },
            &CommonCli {
                frames: 1,
                screenshot: Some(screenshot_path.clone()),
                audio_capture: Some(audio_path.clone()),
                ..CommonCli::default()
            },
        );

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
        let stem = format!("emu198x-game-boy-{}-battery", std::process::id());
        let rom_path = temp_dir.join(format!("{stem}.gb"));
        let save_path = temp_dir.join(format!("{stem}.sav"));
        let save = vec![0x5A; 0x2000];

        fs::write(&rom_path, battery_ram_rom()).expect("temporary ROM write should succeed");
        fs::write(&save_path, &save).expect("temporary save write should succeed");

        let result = script_report(
            &GameBoy {
                battery_save: Some(save_path.clone()),
                ..cartridge(&rom_path)
            },
            &CommonCli::default(),
        );

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
        let stem = format!("emu198x-game-boy-{}-script", std::process::id());
        let rom_path = temp_dir.join(format!("{stem}.gb"));
        let script_path = temp_dir.join(format!("{stem}.json"));

        fs::write(&rom_path, loop_rom()).expect("temporary ROM write should succeed");
        fs::write(
            &script_path,
            r#"
            [
              {"action":"run_frames","frames":1},
              {"action":"query","path":"cartridge.loaded"}
            ]
            "#,
        )
        .expect("script fixture should write");

        let result = script_report(
            &cartridge(&rom_path),
            &CommonCli {
                script: Some(script_path.clone()),
                ..CommonCli::default()
            },
        );

        assert!(result.is_ok(), "runner should execute script: {result:?}");
        let report = result.expect("script result should be available");
        assert_eq!(report["cartridge_loaded"], Value::Bool(true));
        // The two script steps; the launcher may append a blank-frame note.
        let script_steps = report["observations"]
            .as_array()
            .expect("observations")
            .iter()
            .filter(|observation| observation["kind"] != "blank_frame")
            .count();
        assert_eq!(script_steps, 2);

        let _ = fs::remove_file(rom_path);
        let _ = fs::remove_file(script_path);
    }
}
