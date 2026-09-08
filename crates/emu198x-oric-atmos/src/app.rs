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
    pub model: Model,
}

impl Default for Oric {
    fn default() -> Self {
        Self {
            firmware: FirmwareOverrides::none(),
            model: Model::Atmos,
        }
    }
}

impl MachineApp for Oric {
    type Runtime = OricRuntime;
    type Query = OricSessionQueryProvider;

    const BIN_NAME: &'static str = "emu198x-oric-atmos";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");
    const MACHINE_OPTIONS: &'static str = "    --rom PATH|ID=PATH 16 KB BASIC + OS ROM; default
                    ~/.emu198x/roms/oric/oric.rom (or set EMU198X_ORIC_ROM)
    --rom-dir DIR   firmware directory (or set EMU198X_ORIC_ROM_DIR)
                    firmware ID: oric-rom; tries oric-1.rom / atmos.rom before oric.rom
    --model NAME    oric-1 | atmos [default: atmos]";
    const CONTROLS: &'static str = "    Esc             quit
    F12             hard reset
    A-Z 0-9 etc.    the Oric keyboard (cursor keys are real Oric keys)
    Shift / Ctrl    the Oric shift / control keys
    Gamepad         IJK joystick (player 1, left stick)";

    fn parse_flag(&mut self, flag: &str, args: &mut Args) -> Result<bool, LaunchError> {
        match flag {
            "--rom" => self
                .firmware
                .add_spec(
                    &args.value(flag)?,
                    self.model.variant_id(),
                    &self.model.firmware_sources(),
                )
                .map_err(|err| LaunchError::Usage(err.to_string()))?,
            "--rom-dir" => self.firmware.dir = Some(args.path(flag)?),
            "--model" => {
                let id = args.value(flag)?;
                self.model = Model::from_variant_id(&id).ok_or_else(|| {
                    LaunchError::Usage(format!("--model expects oric-1|atmos, got {id}"))
                })?;
            }
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
        build_variant::<OricRuntime>(self.model, &self.firmware)
            .map_err(|err| LaunchError::Run(err.to_string()))
    }

    fn build_mcp_runtime(&self) -> Result<OricRuntime, LaunchError> {
        build_variant_or_blank(self.model, &self.firmware, OricRuntime::blank)
            .map_err(|err| LaunchError::Run(err.to_string()))
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
            app.firmware.by_id.get(runtime_oric_atmos::BIOS_FIRMWARE_ID),
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
