//! The Atari ST as a [`MachineApp`]: its flags, runtime and report fields.

use std::path::PathBuf;

use emu198x_shell::launch::{Args, LaunchError, MachineApp, read_rom};
use emu198x_shell::{FirmwareOverrides, MediaKind, build_variant, build_variant_or_blank};
use runtime_atari_st::{
    AtariStRuntime, AtariStSessionQueryProvider, Model, Monitor, TOS_FIRMWARE_ID,
};
use serde_json::{Map, Value};

/// The model when the flags do not choose one.
const DEFAULT_MODEL: Model = Model::St520Pal;

/// The machine configuration the flags build up.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct AtariSt {
    /// `--model ID`, when given.
    pub model: Option<Model>,
    /// `--mono` or `--monitor mono`.
    pub mono: bool,
    /// `--tos`, `--rom` and `--rom-dir`.
    pub firmware: FirmwareOverrides,
    /// `--disk PATH` (or the one positional argument) and `--disk-b PATH`:
    /// drives A and B.
    pub disks: [Option<PathBuf>; 2],
}

impl AtariSt {
    #[must_use]
    pub fn model(&self) -> Model {
        self.model.unwrap_or(DEFAULT_MODEL)
    }

    #[must_use]
    pub fn monitor(&self) -> Monitor {
        if self.mono {
            Monitor::Mono
        } else {
            Monitor::Colour
        }
    }
}

impl MachineApp for AtariSt {
    type Runtime = AtariStRuntime;
    type Query = AtariStSessionQueryProvider;

    const BIN_NAME: &'static str = "emu198x-atari-st";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");
    const MACHINE_OPTIONS: &'static str =
        "    --tos PATH      TOS or EmuTOS image (192 KB or 256 KB); default
                    the first of tos.img, tos.rom and the EmuTOS images
                    in ~/.emu198x/roms/atari-st/ (or set EMU198X_ATARI_ST_TOS)
    --rom PATH|ID=PATH pin the TOS image (firmware ID: atari-st-tos)
    --rom-dir DIR   firmware directory (or set EMU198X_ATARI_ST_ROM_DIR)
    --disk PATH     .st or .msa floppy for drive A (a bare PATH too)
    --disk-b PATH   .st or .msa floppy for drive B
    --model ID      520st-pal | 520st-ntsc | 1040st-pal | 1040st-ntsc,
                    or a full profile id (alias: --variant)
                    [default: 520st-pal]
    --mono          plug in the SM124 monochrome monitor
                    (also --monitor colour|mono)";
    const CONTROLS: &'static str = "    Close window    quit (or use the platform Quit shortcut)
    F12             emulator hard reset
    Keyboard        the ST keyboard; Page Up is Help, Page Down Undo
    Mouse           the ST mouse (click in the window to capture it)
    Gamepad         joystick in the joystick port";

    fn parse_flag(&mut self, flag: &str, args: &mut Args) -> Result<bool, LaunchError> {
        match flag {
            "--tos" => self.firmware.pin(TOS_FIRMWARE_ID, args.path(flag)?),
            "--rom" => self
                .firmware
                .add_spec(
                    &args.value(flag)?,
                    self.model().profile_id(),
                    &self.model().firmware_sources(),
                )
                .map_err(|err| LaunchError::Usage(err.to_string()))?,
            "--rom-dir" => self.firmware.dir = Some(args.path(flag)?),
            "--disk" => self.disks[0] = Some(args.path(flag)?),
            "--disk-b" => self.disks[1] = Some(args.path(flag)?),
            "--mono" => self.mono = true,
            "--monitor" => {
                let value = args.value(flag)?;
                self.mono = match value.as_str() {
                    "mono" | "monochrome" => true,
                    "colour" | "color" => false,
                    _ => {
                        return Err(LaunchError::Usage(format!(
                            "{flag} expects colour|mono, got {value}"
                        )));
                    }
                };
            }
            "--variant" | "--model" => {
                let id = args.value(flag)?;
                self.model = Some(Model::from_variant_id(&id).ok_or_else(|| {
                    LaunchError::Usage(format!(
                        "{flag} expects {}, got {id}",
                        Model::VARIANT_IDS.join("|")
                    ))
                })?);
            }
            _ if flag.starts_with('-') => return Ok(false),
            _ if self.disks[0].is_none() => self.disks[0] = Some(PathBuf::from(flag)),
            _ => {
                return Err(LaunchError::Usage(
                    "only one positional disk path is supported".to_owned(),
                ));
            }
        }
        Ok(true)
    }

    fn frame_ticks(&self) -> u64 {
        self.model().frame_ticks(self.monitor())
    }

    fn query_provider(&self) -> AtariStSessionQueryProvider {
        AtariStSessionQueryProvider
    }

    fn build_runtime(&self) -> Result<AtariStRuntime, LaunchError> {
        let mut runtime = build_variant::<AtariStRuntime>(self.model(), &self.firmware)
            .map_err(|err| LaunchError::Run(err.to_string()))?;
        runtime.set_monitor(self.monitor());
        Ok(runtime)
    }

    /// Without TOS, MCP starts empty and waits for firmware.
    fn build_mcp_runtime(&self) -> Result<AtariStRuntime, LaunchError> {
        let mut runtime =
            build_variant_or_blank(self.model(), &self.firmware, AtariStRuntime::blank)
                .map_err(|err| LaunchError::Run(err.to_string()))?;
        runtime.set_monitor(self.monitor());
        Ok(runtime)
    }

    fn startup_media(&self) -> Result<Vec<(String, MediaKind, Vec<u8>)>, LaunchError> {
        let mut media = Vec::new();
        for (slot, path) in ["floppy-0", "floppy-1"].into_iter().zip(&self.disks) {
            if let Some(path) = path {
                media.push((slot.to_owned(), MediaKind::Disk, read_rom(path, "--disk")?));
            }
        }
        Ok(media)
    }

    fn mcp_startup_media(
        &self,
        _slots: &[emu198x_shell::MediaSlot],
        _raw_args: &[String],
    ) -> Result<Vec<(String, MediaKind, Vec<u8>)>, LaunchError> {
        self.startup_media()
    }

    fn report(&self, runtime: &AtariStRuntime, report: &mut Map<String, Value>) {
        let frames = runtime.machine().map_or(0, |m| m.glue.frames());
        report.insert("tos_loaded".to_owned(), runtime.machine().is_some().into());
        report.insert("frames_run".to_owned(), frames.into());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use emu198x_shell::launch::{Mode, Parsed, parse};
    use std::path::Path;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn parse_cli_defaults() {
        let Parsed::Run { app, common, .. } = parse::<AtariSt>(&[]).expect("parses") else {
            panic!("expected a run");
        };
        assert_eq!(app, AtariSt::default());
        assert_eq!(app.model(), Model::St520Pal);
        assert_eq!(app.monitor(), Monitor::Colour);
        assert_eq!(common.frames, 0);
    }

    #[test]
    fn parse_cli_accepts_full_flags() {
        let parsed = parse::<AtariSt>(&args(&[
            "--tos",
            "etos.img",
            "--disk",
            "/tmp/a.st",
            "--disk-b",
            "/tmp/b.msa",
            "--model",
            "1040st-ntsc",
            "--monitor",
            "mono",
            "--frames",
            "60",
        ]))
        .expect("parses");
        let Parsed::Run { app, common, mode } = parsed else {
            panic!("expected a run");
        };
        assert_eq!(
            app.firmware.by_id.get(TOS_FIRMWARE_ID),
            Some(&PathBuf::from("etos.img"))
        );
        assert_eq!(app.disks[0].as_deref(), Some(Path::new("/tmp/a.st")));
        assert_eq!(app.disks[1].as_deref(), Some(Path::new("/tmp/b.msa")));
        assert_eq!(app.model(), Model::St1040Ntsc);
        assert_eq!(app.monitor(), Monitor::Mono);
        assert_eq!(common.frames, 60);
        assert_eq!(mode, Mode::Script);
    }

    #[test]
    fn parse_cli_accepts_a_positional_disk_and_rejects_two() {
        let Parsed::Run { app, mode, .. } =
            parse::<AtariSt>(&args(&["game.msa", "--mono"])).expect("parses")
        else {
            panic!("expected a run");
        };
        assert_eq!(app.disks[0], Some(PathBuf::from("game.msa")));
        assert!(app.mono);
        assert_eq!(mode, Mode::Ui);
        assert!(parse::<AtariSt>(&args(&["a.st", "b.st"])).is_err());
        assert!(parse::<AtariSt>(&args(&["--monitor", "tv"])).is_err());
        assert!(parse::<AtariSt>(&args(&["--model", "falcon"])).is_err());
    }

    #[test]
    fn frame_ticks_follow_the_model_and_monitor() {
        let app = |model, mono| AtariSt {
            model: Some(model),
            mono,
            ..AtariSt::default()
        };
        assert_eq!(app(Model::St520Pal, false).frame_ticks(), 313 * 512);
        assert_eq!(app(Model::St520Ntsc, false).frame_ticks(), 263 * 508);
        assert_eq!(app(Model::St1040Pal, true).frame_ticks(), 501 * 224);
    }

    #[test]
    fn startup_media_puts_each_disk_in_its_drive() {
        let path = std::env::temp_dir().join(format!(
            "emu198x-st-disk-{}-{}.st",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system clock should follow Unix epoch")
                .as_nanos()
        ));
        std::fs::write(&path, [0u8; 512]).expect("write disk");
        let app = AtariSt {
            disks: [None, Some(path.clone())],
            ..AtariSt::default()
        };
        let media = app.startup_media().expect("reads");
        assert_eq!(media.len(), 1);
        assert_eq!(media[0].0, "floppy-1");
        assert_eq!(media[0].1, MediaKind::Disk);
        std::fs::remove_file(path).expect("temporary disk should be removable");
    }
}
