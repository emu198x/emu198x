//! The Oric-1 / Atmos as a [`MachineApp`]: its flags, runtime, and report fields.

#[cfg(test)]
use std::path::PathBuf;

use emu198x_shell::launch::{Args, LaunchError, MachineApp};
use emu198x_shell::{FirmwareOverrides, build_variant, build_variant_or_blank};
use runtime_oric_atmos::{Model, OricRuntime, OricSessionQueryProvider};
use serde_json::{Map, Value};

/// The machine configuration the flags build up.
#[derive(Debug, PartialEq, Eq)]
pub struct Oric {
    pub firmware: FirmwareOverrides,
    /// Resolve bare ROM paths only after all --model flags have been parsed.
    rom_specs: Vec<String>,
    pub model: Model,
    /// Fit the 6551 serial card at `$031C-$031F`.
    pub serial_card: bool,
}

impl Default for Oric {
    fn default() -> Self {
        Self {
            firmware: FirmwareOverrides::none(),
            rom_specs: Vec::new(),
            model: Model::Atmos,
            serial_card: false,
        }
    }
}

impl Oric {
    fn resolved_firmware(&self) -> Result<FirmwareOverrides, LaunchError> {
        let mut firmware = self.firmware.clone();
        for spec in &self.rom_specs {
            firmware
                .add_spec(
                    spec,
                    self.model.variant_id(),
                    &self.model.firmware_sources(),
                )
                .map_err(|err| LaunchError::Usage(err.to_string()))?;
        }
        Ok(firmware)
    }
}

impl MachineApp for Oric {
    type Runtime = OricRuntime;
    type Query = OricSessionQueryProvider;

    const BIN_NAME: &'static str = "emu198x-oric-atmos";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");
    const MACHINE_OPTIONS: &'static str =
        "    --rom PATH|ID=PATH 16 KiB BASIC + OS ROM for the selected model
    --rom-dir DIR   firmware directory (or set EMU198X_ORIC_ROM_DIR)
    --model NAME    oric-1 | atmos [default: atmos]
                    Oric-1: oric1-rom; EMU198X_ORIC1_ROM;
                    oric-1.rom / oric1.rom / basic10.rom
                    Atmos: oric-rom; EMU198X_ORIC_ROM;
                    atmos.rom / oric.rom / basic11b.rom
                    both models have 64 KiB physical RAM, 48 KiB below ROM
    --serial        fit a 6551 ACIA serial card at $031C-$031F";
    const CONTROLS: &'static str = "    Close window    quit (or use the platform Quit shortcut)
    Cmd/Ctrl+Shift+R  hard reset
    Cmd/Ctrl+P      pause / resume
    A-Z 0-9 etc.    the Oric keyboard (cursor keys are real Oric keys)
    Shift / Ctrl    the Oric shift / control keys
    Gamepad         IJK joystick (player 1, left stick)";

    fn parse_flag(&mut self, flag: &str, args: &mut Args) -> Result<bool, LaunchError> {
        match flag {
            "--rom" => self.rom_specs.push(args.value(flag)?),
            "--rom-dir" => self.firmware.dir = Some(args.path(flag)?),
            "--model" => {
                let id = args.value(flag)?;
                self.model = Model::from_variant_id(&id).ok_or_else(|| {
                    LaunchError::Usage(format!("--model expects oric-1|atmos, got {id}"))
                })?;
            }
            "--serial" => self.serial_card = true,
            _ => return Ok(false),
        }
        Ok(true)
    }

    fn frame_ticks(&self) -> u64 {
        self.model.frame_ticks()
    }

    fn query_provider(&self) -> OricSessionQueryProvider {
        OricSessionQueryProvider
    }

    fn build_runtime(&self) -> Result<OricRuntime, LaunchError> {
        let mut runtime = build_variant::<OricRuntime>(self.model, &self.resolved_firmware()?)
            .map_err(|err| LaunchError::Run(err.to_string()))?;
        runtime.set_serial_card(self.serial_card);
        Ok(runtime)
    }

    fn build_mcp_runtime(&self) -> Result<OricRuntime, LaunchError> {
        let mut runtime =
            build_variant_or_blank(self.model, &self.resolved_firmware()?, OricRuntime::blank)
                .map_err(|err| LaunchError::Run(err.to_string()))?;
        runtime.set_serial_card(self.serial_card);
        Ok(runtime)
    }

    fn mcp_startup_media(
        &self,
        _slots: &[emu198x_shell::MediaSlot],
        _raw_args: &[String],
    ) -> Result<Vec<(String, emu198x_shell::MediaKind, Vec<u8>)>, LaunchError> {
        self.startup_media()
    }

    fn report(&self, runtime: &OricRuntime, report: &mut Map<String, Value>) {
        let frames_run = runtime.machine().map_or(0, |m| m.frame_count());
        report.insert("rom_loaded".to_owned(), runtime.machine().is_some().into());
        report.insert("frames_run".to_owned(), frames_run.into());
        report.insert("model".to_owned(), runtime.model().variant_id().into());
        report.insert("serial_card".to_owned(), runtime.serial_card().into());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use emu198x_shell::launch::{Mode, Parsed, parse};

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn parse_cli_defaults() {
        let Parsed::Run { app, .. } = parse::<Oric>(&[]).expect("parses") else {
            panic!("expected a run");
        };
        assert_eq!(app.model, Model::Atmos);
        assert!(!app.serial_card);
    }

    #[test]
    fn serial_fits_the_acia_card() {
        let Parsed::Run { app, .. } = parse::<Oric>(&args(&["--serial"])).expect("parses") else {
            panic!("expected a run");
        };
        assert!(app.serial_card);
        let runtime = app.build_mcp_runtime().expect("runtime");
        assert!(runtime.serial_card());
    }

    #[test]
    fn parse_cli_accepts_rom_model_scale_video() {
        let parsed = parse::<Oric>(&args(&[
            "--rom", "oric.rom", "--model", "oric-1", "--scale", "4", "--video", "crt",
        ]))
        .expect("parses");
        let Parsed::Run { app, common, mode } = parsed else {
            panic!("expected a run");
        };
        assert_eq!(
            app.resolved_firmware()
                .expect("final model firmware")
                .by_id
                .get(runtime_oric_atmos::ORIC1_FIRMWARE_ID),
            Some(&PathBuf::from("oric.rom"))
        );
        assert_eq!(app.model, Model::Oric1);
        assert_eq!(common.scale, Some(4));
        assert_eq!(common.video.as_deref(), Some("crt"));
        assert_eq!(mode, Mode::Ui);
    }

    #[test]
    fn a_bad_model_is_a_usage_error() {
        let err = parse::<Oric>(&args(&["--model", "telestrat"])).expect_err("rejects");
        assert_eq!(
            err,
            LaunchError::Usage("--model expects oric-1|atmos, got telestrat".to_owned())
        );
    }
}

#[cfg(all(test, feature = "ui"))]
mod ui_firmware_tests {
    use super::*;
    use emu198x_shell::launch::{Parsed, parse};
    use emu198x_ui::launch::UiApp;
    struct Directory(std::path::PathBuf);
    impl Drop for Directory {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    #[test]
    fn window_resolves_bare_roms_against_the_final_model() {
        let dir = Directory(
            std::env::temp_dir().join(format!("oric-ui-firmware-{}", std::process::id())),
        );
        std::fs::create_dir_all(&dir.0).expect("fixture directory");
        let path = dir.0.join("custom.rom");
        std::fs::write(&path, vec![0x6a; 16384]).expect("custom ROM");
        let path = path.display().to_string();
        for model in Model::ALL {
            for rom_first in [false, true] {
                let args = if rom_first {
                    vec!["--rom", path.as_str(), "--model", model.variant_id()]
                } else {
                    vec!["--model", model.variant_id(), "--rom", path.as_str()]
                };
                let args = args.into_iter().map(str::to_owned).collect::<Vec<_>>();
                let Parsed::Run { app, .. } = parse::<Oric>(&args).expect("window arguments")
                else {
                    panic!("window launch")
                };
                let runtime = app.build_ui_runtime().expect("window firmware");
                assert_eq!(runtime.model(), model);
                assert_eq!(runtime.machine().expect("loaded").peek(0xc000), 0x6a);
            }
        }
    }
}
