//! The Commodore PET as a [`MachineApp`]: its flags, runtime, and report fields.

use emu198x_shell::launch::{Args, LaunchError, MachineApp, read_rom};
use emu198x_shell::{FirmwareOverrides, MediaKind, build_variant, build_variant_or_blank};
use runtime_commodore_pet::{
    BASIC_FIRMWARE_ID, CHAR_FIRMWARE_ID, EDITOR_FIRMWARE_ID, KERNAL_FIRMWARE_ID, Model, PetRuntime,
    PetSessionQueryProvider,
};
use serde_json::{Map, Value};
use std::path::PathBuf;

/// The machine configuration the flags build up.
#[derive(Debug, PartialEq, Eq)]
pub struct CommodorePet {
    pub firmware: FirmwareOverrides,
    pub model: Model,
    /// `--prg PATH`: a program loaded after boot and auto-RUN.
    pub prg: Option<PathBuf>,
}

impl Default for CommodorePet {
    fn default() -> Self {
        Self {
            firmware: FirmwareOverrides::none(),
            model: Model::Pet40Col,
            prg: None,
        }
    }
}

impl MachineApp for CommodorePet {
    type Runtime = PetRuntime;
    type Query = PetSessionQueryProvider;

    const BIN_NAME: &'static str = "emu198x-commodore-pet";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");
    const MACHINE_OPTIONS: &'static str = "    --kernal PATH   KERNAL ROM (4 KB)
    --basic PATH    BASIC ROM (8 KB)
    --editor PATH   editor ROM (2 KB)
    --char PATH     character ROM (4 KB)
                    ROM defaults: $EMU198X_PET_{KERNAL,BASIC,EDITOR,CHAR}, then
                    ~/.emu198x/roms/commodore-pet/{kernal,basic,editor,chargen}.rom
    --rom ID=PATH  pin one of the four catalogue ROM images
    --rom-dir DIR  firmware directory (or EMU198X_PET_ROM_DIR)
    --model ID     commodore-pet-40col | commodore-pet-80col
    --columns N     40 or 80 [default: 40]
    --prg PATH      load a .prg after boot and auto-RUN it";
    const CONTROLS: &'static str = "    Esc             quit
    F12             hard reset
    A-Z 0-9 etc.    the PET keyboard
    Enter           RETURN";

    fn parse_flag(&mut self, flag: &str, args: &mut Args) -> Result<bool, LaunchError> {
        match flag {
            "--kernal" => self.firmware.pin(KERNAL_FIRMWARE_ID, args.path(flag)?),
            "--basic" => self.firmware.pin(BASIC_FIRMWARE_ID, args.path(flag)?),
            "--editor" => self.firmware.pin(EDITOR_FIRMWARE_ID, args.path(flag)?),
            "--char" => self.firmware.pin(CHAR_FIRMWARE_ID, args.path(flag)?),
            "--columns" | "--model" => {
                let id = args.value(flag)?;
                self.model = Model::from_variant_id(&id).ok_or_else(|| {
                    LaunchError::Usage(format!("{flag} expects 40 or 80, got {id}"))
                })?;
            }
            "--rom-dir" => self.firmware.dir = Some(args.path(flag)?),
            "--rom" => self
                .firmware
                .add_spec(
                    &args.value(flag)?,
                    self.model.variant_id(),
                    &self.model.firmware_sources(),
                )
                .map_err(|err| LaunchError::Usage(err.to_string()))?,
            "--prg" => self.prg = Some(args.path(flag)?),
            _ => return Ok(false),
        }
        Ok(true)
    }

    fn frame_ticks(&self) -> u64 {
        self.model.frame_ticks()
    }

    fn query_provider(&self) -> PetSessionQueryProvider {
        PetSessionQueryProvider
    }

    fn build_runtime(&self) -> Result<PetRuntime, LaunchError> {
        build_variant::<PetRuntime>(self.model, &self.firmware)
            .map_err(|err| LaunchError::Run(err.to_string()))
    }

    fn build_mcp_runtime(&self) -> Result<PetRuntime, LaunchError> {
        build_variant_or_blank::<PetRuntime>(self.model, &self.firmware, PetRuntime::blank)
            .map_err(|err| LaunchError::Run(err.to_string()))
    }

    fn mcp_startup_media(
        &self,
        _slots: &[emu198x_shell::MediaSlot],
        _raw_args: &[String],
    ) -> Result<Vec<(String, MediaKind, Vec<u8>)>, LaunchError> {
        self.startup_media()
    }

    fn startup_media(&self) -> Result<Vec<(String, MediaKind, Vec<u8>)>, LaunchError> {
        let Some(path) = &self.prg else {
            return Ok(Vec::new());
        };
        let bytes = read_rom(path, "--prg")?;
        Ok(vec![("program-1".to_owned(), MediaKind::Program, bytes)])
    }

    fn report(&self, runtime: &PetRuntime, report: &mut Map<String, Value>) {
        let roms_loaded = runtime.machine().is_some();
        let frames_run = runtime.machine().map_or(0, |m| m.frame_count());
        report.insert("roms_loaded".to_owned(), roms_loaded.into());
        report.insert("frames_run".to_owned(), frames_run.into());
        report.insert("columns".to_owned(), runtime.model().screen_chars().into());
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
    fn defaults_to_forty_columns() {
        let parsed = parse::<CommodorePet>(&[]).expect("parses");
        let Parsed::Run { app, common, mode } = parsed else {
            panic!("expected a run");
        };
        assert!(app.firmware.by_id.is_empty());
        assert_eq!(app.model, Model::Pet40Col);
        assert_eq!(common.frames, 0);
        assert_eq!(mode, Mode::Ui);
    }

    #[test]
    fn flags_set_roms_columns_scale_video() {
        let parsed = parse::<CommodorePet>(&args(&[
            "--kernal",
            "k.rom",
            "--columns",
            "80",
            "--scale",
            "2",
            "--video",
            "crt",
            "--prg",
            "hello.prg",
        ]))
        .expect("parses");
        let Parsed::Run { app, common, mode } = parsed else {
            panic!("expected a run");
        };
        assert_eq!(
            app.firmware.by_id.get(KERNAL_FIRMWARE_ID),
            Some(&PathBuf::from("k.rom"))
        );
        assert_eq!(app.model, Model::Pet80Col);
        assert_eq!(app.prg, Some(PathBuf::from("hello.prg")));
        assert_eq!(common.scale, Some(2));
        assert_eq!(common.video.as_deref(), Some("crt"));
        // A bare `--columns` is shared with the UI, so it opens the window.
        assert_eq!(mode, Mode::Ui);
    }

    #[test]
    fn a_bad_column_count_is_a_usage_error() {
        let err = parse::<CommodorePet>(&args(&["--columns", "forty"])).expect_err("rejects");
        assert_eq!(
            err,
            LaunchError::Usage("--columns expects 40 or 80, got forty".to_owned())
        );
    }

    #[test]
    fn model_selects_by_columns() {
        assert_eq!(Model::from_variant_id("40"), Some(Model::Pet40Col));
        assert_eq!(Model::from_variant_id("80"), Some(Model::Pet80Col));
    }
}
