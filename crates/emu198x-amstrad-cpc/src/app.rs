//! The Amstrad CPC464 as a [`MachineApp`]: its flags, runtime, and report fields.

use std::path::PathBuf;

use emu198x_shell::launch::{Args, LaunchError, MachineApp};
use emu198x_shell::{FirmwareOverrides, build_variant, build_variant_or_blank};
use emu198x_shell::{MachineCore, MediaImage, MediaKind, MediaSet, read_media_asset};
use runtime_amstrad_cpc::{AmstradCpcRuntime, AmstradCpcSessionQueryProvider, Model};
use serde_json::{Map, Value};

/// One PAL frame: 64 character clocks per line x 312 lines x 4 T-states.
///
/// Must not exceed the machine's own `run_frame` budget, or the harness runs
/// two machine frames per displayed frame and everything plays at double
/// speed.
pub const FRAME_TICKS_PAL: u64 = Model::Cpc464.frame_ticks();

/// The machine configuration the flags build up.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Cpc {
    pub firmware: FirmwareOverrides,
    /// `--tape PATH`: a .cdt cassette put in the deck before the machine runs.
    pub tape: Option<PathBuf>,
}

impl Cpc {
    fn load_startup_tape(&self, runtime: &mut AmstradCpcRuntime) -> Result<(), LaunchError> {
        if let Some(path) = &self.tape {
            let loaded = read_media_asset(path, MediaKind::Tape).map_err(|err| {
                LaunchError::Run(format!(
                    "failed to load tape asset {}: {err}",
                    path.display()
                ))
            })?;
            let mut media = MediaSet::new();
            media.push(MediaImage::new("tape-1", MediaKind::Tape, &loaded.bytes));
            runtime
                .load_media(&media)
                .map_err(|err| LaunchError::Run(format!("tape load failed: {err}")))?;
        }
        Ok(())
    }
}

impl MachineApp for Cpc {
    type Runtime = AmstradCpcRuntime;
    type Query = AmstradCpcSessionQueryProvider;

    const BIN_NAME: &'static str = "emu198x-amstrad-cpc";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");
    const MACHINE_OPTIONS: &'static str =
        "    --rom PATH      CPC464 firmware (32 KB: 16 KB OS + 16 KB BASIC); default
                    ~/.emu198x/roms/amstrad-cpc/cpc464.rom
                    (or set EMU198X_CPC464_ROM)
    --rom-dir DIR   firmware directory (or set EMU198X_CPC_ROM_DIR)
                    --rom also accepts amstrad-cpc464-firmware=PATH
    --tape PATH     .cdt cassette image to insert at start. In BASIC, type
                    RUN\" and press RETURN twice. The CPC drives the cassette
                    motor itself, so playback follows the firmware rather
                    than a host transport control.";
    const CONTROLS: &'static str = "    Esc             quit
    F12             hard reset
    A-Z 0-9 etc.    the CPC keyboard
    Shift / Ctrl    the CPC SHIFT / CONTROL keys
    Arrows          the CPC cursor keys
    Gamepad         joystick 0";

    fn parse_flag(&mut self, flag: &str, args: &mut Args) -> Result<bool, LaunchError> {
        match flag {
            "--rom" => self
                .firmware
                .add_spec(
                    &args.value(flag)?,
                    Model::Cpc464.profile_id(),
                    &Model::Cpc464.firmware_sources(),
                )
                .map_err(|err| LaunchError::Usage(err.to_string()))?,
            "--rom-dir" => self.firmware.dir = Some(args.path(flag)?),
            "--tape" => self.tape = Some(args.path(flag)?),
            _ => return Ok(false),
        }
        Ok(true)
    }

    fn frame_ticks(&self) -> u64 {
        FRAME_TICKS_PAL
    }

    fn query_provider(&self) -> AmstradCpcSessionQueryProvider {
        AmstradCpcSessionQueryProvider
    }

    /// The firmware, and the cassette when `--tape` names one. A reset keeps
    /// the cassette in the deck, so it is inserted here rather than as
    /// startup media.
    fn build_runtime(&self) -> Result<AmstradCpcRuntime, LaunchError> {
        let mut runtime = build_variant::<AmstradCpcRuntime>(Model::Cpc464, &self.firmware)
            .map_err(|err| LaunchError::Run(err.to_string()))?;
        self.load_startup_tape(&mut runtime)?;
        Ok(runtime)
    }

    fn build_mcp_runtime(&self) -> Result<AmstradCpcRuntime, LaunchError> {
        let mut runtime =
            build_variant_or_blank(Model::Cpc464, &self.firmware, AmstradCpcRuntime::blank)
                .map_err(|err| LaunchError::Run(err.to_string()))?;
        self.load_startup_tape(&mut runtime)?;
        Ok(runtime)
    }

    fn mcp_startup_media(
        &self,
        _slots: &[emu198x_shell::MediaSlot],
        _raw_args: &[String],
    ) -> Result<Vec<(String, emu198x_shell::MediaKind, Vec<u8>)>, LaunchError> {
        self.startup_media()
    }

    fn report(&self, runtime: &AmstradCpcRuntime, report: &mut Map<String, Value>) {
        let machine = runtime.machine();
        report.insert("rom_loaded".to_owned(), machine.is_some().into());
        report.insert(
            "tape_loaded".to_owned(),
            machine.is_some_and(|m| m.tape().has_tape()).into(),
        );
        report.insert(
            "frames_run".to_owned(),
            machine.map_or(0, |m| m.frame_count()).into(),
        );
    }

    // The AY-watch verbs are deliberately absent from the default tool set
    // here: the CPC's PSG is reachable through the `psg.registers` query path,
    // but the machine does not carry the write-watch hook those tools drive,
    // and registering a tool the machine cannot serve advertises a capability
    // that does not exist. The launcher's default (base + keyboard) is right.
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use emu198x_shell::launch::{CommonCli, Mode, Parsed, parse, script_report};

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn parse_cli_defaults() {
        let Parsed::Run { app, common, .. } = parse::<Cpc>(&[]).expect("parses") else {
            panic!("expected a run");
        };
        assert_eq!(app.firmware, FirmwareOverrides::none());
        assert!(app.tape.is_none());
        assert_eq!(common.frames, 0);
        assert_eq!(common.scale, None);
        assert_eq!(common.video, None);
    }

    #[test]
    fn parse_cli_reads_the_paths_and_frame_count() {
        let parsed = parse::<Cpc>(&args(&[
            "--rom",
            "cpc464.rom",
            "--tape",
            "game.cdt",
            "--frames",
            "600",
            "--screenshot",
            "out.png",
        ]))
        .expect("parses");
        let Parsed::Run { app, common, mode } = parsed else {
            panic!("expected a run");
        };
        assert_eq!(
            app.firmware.by_id.get(runtime_amstrad_cpc::ROM_FIRMWARE_ID),
            Some(&PathBuf::from("cpc464.rom"))
        );
        assert_eq!(app.tape, Some(PathBuf::from("game.cdt")));
        assert_eq!(common.frames, 600);
        assert_eq!(common.screenshot, Some(PathBuf::from("out.png")));
        assert_eq!(mode, Mode::Script);
    }

    #[test]
    fn parse_cli_accepts_rom_tape_scale_video() {
        let parsed = parse::<Cpc>(&args(&[
            "--rom",
            "cpc464.rom",
            "--tape",
            "game.cdt",
            "--scale",
            "4",
            "--video",
            "crt",
        ]))
        .expect("parses");
        let Parsed::Run { app, common, mode } = parsed else {
            panic!("expected a run");
        };
        assert_eq!(
            app.firmware.by_id.get(runtime_amstrad_cpc::ROM_FIRMWARE_ID),
            Some(&PathBuf::from("cpc464.rom"))
        );
        assert_eq!(app.tape, Some(PathBuf::from("game.cdt")));
        assert_eq!(common.scale, Some(4));
        assert_eq!(common.video.as_deref(), Some("crt"));
        assert_eq!(mode, Mode::Ui);
    }

    #[test]
    fn a_frame_is_the_machines_own_frame_budget() {
        // Exceeding `AmstradCpc::run_frame` runs two machine frames per
        // displayed one, which plays everything at double speed with no error.
        assert_eq!(FRAME_TICKS_PAL, 79_872);
    }

    #[test]
    fn capture_without_anything_to_run_is_refused() {
        // Otherwise the PNG is the power-on frame and reads as a success. The
        // launcher refuses before it reads the firmware, so no ROM is needed.
        let app = Cpc {
            firmware: FirmwareOverrides::none(),
            tape: None,
        };
        let common = CommonCli {
            screenshot: Some(PathBuf::from("out.png")),
            ..CommonCli::default()
        };
        let err = script_report(&app, &common).expect_err("should refuse");
        assert!(
            err.to_string().contains("capture requests require"),
            "{err}"
        );
    }

    #[test]
    fn a_wrong_size_firmware_is_named_rather_than_truncated() {
        let dir = std::env::temp_dir().join("emu198x-cpc-firmware-size");
        fs::create_dir_all(&dir).expect("temp dir");
        let rom = dir.join("short.rom");
        fs::write(&rom, vec![0u8; 16 * 1024]).expect("write short rom");
        let mut app = Cpc::default();
        app.firmware.pin(runtime_amstrad_cpc::ROM_FIRMWARE_ID, &rom);
        let err = app
            .build_runtime()
            .err()
            .expect("16 KB should be refused")
            .to_string();
        assert!(err.contains("16384 bytes"), "{err}");
        assert!(err.contains("32768"), "{err}");
    }
}
