//! The Acorn Electron as a [`MachineApp`]: its flags, runtime, and report fields.

#[cfg(test)]
use std::path::{Path, PathBuf};

use emu198x_shell::launch::{Args, LaunchError, MachineApp};
use emu198x_shell::{FirmwareOverrides, build_variant, build_variant_or_blank};
use runtime_acorn_electron::{ElectronRuntime, ElectronSessionQueryProvider, Model};
use serde_json::{Map, Value};

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Electron {
    pub firmware: FirmwareOverrides,
}

impl MachineApp for Electron {
    type Runtime = ElectronRuntime;
    type Query = ElectronSessionQueryProvider;

    const BIN_NAME: &'static str = "emu198x-acorn-electron";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");
    const MACHINE_OPTIONS: &'static str = "    --os PATH       Electron OS ROM (16 KB); default
                    ~/.emu198x/roms/acorn-electron/os.rom (or set EMU198X_ELECTRON_OS)
    --basic PATH    BBC BASIC II ROM (16 KB); default
                    ~/.emu198x/roms/acorn-electron/basic.rom
                    (or set EMU198X_ELECTRON_BASIC)
    --rom ID=PATH   pin acorn-electron-os or acorn-electron-basic (repeatable)
    --rom-dir DIR   firmware directory (or set EMU198X_ELECTRON_ROM_DIR)";
    const CONTROLS: &'static str = "    Esc             quit
    F12             hard reset
    A-Z 0-9 etc.    the Electron keyboard (cursor keys are real Electron keys)
    Shift / Ctrl    the Electron SHIFT / CTRL keys; Alt = FUNC; End = COPY";

    fn parse_flag(&mut self, flag: &str, args: &mut Args) -> Result<bool, LaunchError> {
        match flag {
            "--os" | "--basic" => {
                let id = if flag == "--os" {
                    runtime_acorn_electron::OS_FIRMWARE_ID
                } else {
                    runtime_acorn_electron::BASIC_FIRMWARE_ID
                };
                self.firmware.by_id.insert(id.to_owned(), args.path(flag)?);
            }
            "--rom" => self
                .firmware
                .add_spec(
                    &args.value(flag)?,
                    Model::Electron.variant_id(),
                    &Model::Electron.firmware_sources(),
                )
                .map_err(|err| LaunchError::Usage(err.to_string()))?,
            "--rom-dir" => self.firmware.dir = Some(args.path(flag)?),
            _ => return Ok(false),
        }
        Ok(true)
    }

    fn frame_ticks(&self) -> u64 {
        Model::Electron.frame_ticks()
    }

    fn query_provider(&self) -> ElectronSessionQueryProvider {
        ElectronSessionQueryProvider
    }

    fn build_runtime(&self) -> Result<ElectronRuntime, LaunchError> {
        build_variant::<ElectronRuntime>(Model::Electron, &self.firmware)
            .map_err(|err| LaunchError::Run(err.to_string()))
    }

    fn build_mcp_runtime(&self) -> Result<ElectronRuntime, LaunchError> {
        build_variant_or_blank(Model::Electron, &self.firmware, ElectronRuntime::blank)
            .map_err(|err| LaunchError::Run(err.to_string()))
    }

    fn mcp_startup_media(
        &self,
        _slots: &[emu198x_shell::MediaSlot],
        _raw_args: &[String],
    ) -> Result<Vec<(String, emu198x_shell::MediaKind, Vec<u8>)>, LaunchError> {
        self.startup_media()
    }

    fn report(&self, runtime: &ElectronRuntime, report: &mut Map<String, Value>) {
        let frames_run = runtime.machine().map_or(0, |m| m.frame_count());
        report.insert("roms_loaded".to_owned(), runtime.machine().is_some().into());
        report.insert("frames_run".to_owned(), frames_run.into());
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
        let Parsed::Run { app, .. } = parse::<Electron>(&[]).expect("parses") else {
            panic!("expected a run");
        };
        assert!(app.firmware.by_id.is_empty());
    }

    #[test]
    fn parse_cli_accepts_full_flags() {
        let parsed = parse::<Electron>(&args(&[
            "--os",
            "/tmp/os",
            "--basic",
            "/tmp/basic",
            "--frames",
            "30",
        ]))
        .expect("parses");
        let Parsed::Run { app, common, mode } = parsed else {
            panic!("expected a run");
        };
        assert_eq!(
            app.firmware
                .by_id
                .get(runtime_acorn_electron::OS_FIRMWARE_ID)
                .expect("parsed by CLI"),
            Path::new("/tmp/os")
        );
        assert_eq!(
            app.firmware
                .by_id
                .get(runtime_acorn_electron::BASIC_FIRMWARE_ID)
                .expect("parsed by CLI"),
            Path::new("/tmp/basic")
        );
        assert_eq!(common.frames, 30);
        assert_eq!(mode, Mode::Script);
    }

    #[test]
    fn parse_cli_accepts_os_basic_scale_video() {
        let parsed = parse::<Electron>(&args(&[
            "--os",
            "os.rom",
            "--basic",
            "basic.rom",
            "--scale",
            "2",
            "--video",
            "crt",
        ]))
        .expect("parses");
        let Parsed::Run { app, common, mode } = parsed else {
            panic!("expected a run");
        };
        assert_eq!(
            app.firmware
                .by_id
                .get(runtime_acorn_electron::OS_FIRMWARE_ID),
            Some(&PathBuf::from("os.rom"))
        );
        assert_eq!(
            app.firmware
                .by_id
                .get(runtime_acorn_electron::BASIC_FIRMWARE_ID),
            Some(&PathBuf::from("basic.rom"))
        );
        assert_eq!(common.scale, Some(2));
        assert_eq!(common.video.as_deref(), Some("crt"));
        assert_eq!(mode, Mode::Ui);
    }
}
